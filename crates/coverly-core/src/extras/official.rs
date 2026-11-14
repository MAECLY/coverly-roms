//! Telling official extras apart from their plaintext headers (extras spec D6, §5).
//!
//! Only these, never from a file [`is_key_material`] names and never by
//! a key: a PS3 `.p3t` (magic `P3TF`); a PS3 PKG (type 1) whose metadata
//! content type is `0x09` (theme) or `0x0D` (avatar); a Vita PKG (type 2)
//! of content type `0x1F`; an Xbox 360 STFS package whose content type is
//! a theme, gamer picture, avatar item or video (see
//! [`crate::consoles::xbox::stfs_extra`]); a 3DS `.cia` of an application
//! (`00040000`) whose SMDH title or file name holds a whole soundtrack
//! word; and last a PSP `.ptf` by its extension, when no console magic
//! claims the file (no PTF magic was confirmed by a source, so none is
//! checked). Everything else is `None` and stays what it is today: a
//! PKG of another type is a game, DLC or game data; a CIA without such a
//! word is a game. No video heuristic: «video» or «trailer» in a name
//! also matches real games.

use std::path::Path;

use unicode_normalization::UnicodeNormalization;

use super::{ExtraFormat, ExtrasCategory};
use crate::consoles::sony::{self, PkgContentType};
use crate::consoles::xbox::{self, StfsExtra};
use crate::consoles::{self, is_key_material, Budgeted, TitleKind};
use crate::identify::sniff::{FileAt, SliceAt};

/// The most bytes of a `.p3t` searched for its theme XML's `name`.
const P3T_NAME_WINDOW: u64 = 64 * 1024;
const P3T_MAGIC: &[u8] = b"P3TF";
const PKG_MAGIC: &[u8] = &[0x7F, b'P', b'K', b'G'];

/// Soundtrack words matched as whole words (lower-case, word by word).
const SOUNDTRACK_WORDS: &[&[&str]] = &[
    &["soundtrack"],
    &["sound", "track"],
    &["ost"],
    &["banda", "sonora"],
];
/// Soundtrack words matched as substrings (no spaces between words).
const SOUNDTRACK_SUBSTRINGS: &[&str] = &["サウンドトラック"];

/// An official extra told apart by its header (D6).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OfficialExtra {
    pub category: ExtrasCategory,
    pub format: ExtraFormat,
    /// The title id the header names (`BLUS30001`, `4D530001`,
    /// `0004000000123400`), for the link (D7).
    pub title_id: Option<String>,
    /// The name the header gives (theme XML, STFS display name, SMDH
    /// title), else the file stem where the format has no name field.
    pub display_name: Option<String>,
    /// The raw content type of a PKG or STFS package, shown in the plan so
    /// a wrong guess is visible in the dry run (Q3).
    pub content_type: Option<u32>,
}

/// The official extra in the file at `path`, or `None`. Key material is
/// refused by name before any open; a folder, a symlink or a special
/// file is never opened.
pub fn sniff_path(path: &Path) -> Option<OfficialExtra> {
    let name = path.file_name()?.to_str()?;
    if is_key_material(name) {
        return None;
    }
    let file = crate::addons::open_regular(path).ok()?;
    let len = file.metadata().ok()?.len();
    let mut src = FileAt(file);
    sniff_official(name, &mut Budgeted::new(&mut src, len))
}

/// [`sniff_official`] over bytes already in memory (tests).
pub fn sniff_bytes(name: &str, bytes: &[u8]) -> Option<OfficialExtra> {
    let mut src = SliceAt(bytes);
    sniff_official(name, &mut Budgeted::new(&mut src, bytes.len() as u64))
}

/// The official extra `b` holds, `name` being its file name, or `None`.
/// Checked in a fixed order: key material, `.p3t`, PKG, STFS, `.cia`,
/// `.ptf`.
pub fn sniff_official(name: &str, b: &mut Budgeted) -> Option<OfficialExtra> {
    if is_key_material(name) || b.is_empty() {
        return None;
    }
    let ext = Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    let n = usize::try_from(b.len().min(4)).ok()?;
    let head = b.read(0, n).ok()?;
    if head == P3T_MAGIC {
        return (ext == "p3t").then(|| p3t(name, b)).flatten();
    }
    if head == PKG_MAGIC {
        return pkg(name, b);
    }
    if matches!(&head[..], b"CON " | b"LIVE" | b"PIRS") {
        return stfs(b);
    }
    match ext.as_str() {
        "cia" => cia(name, b),
        "ptf" => ptf(name, b),
        _ => None,
    }
}

fn stem(name: &str) -> Option<String> {
    let stem = Path::new(name).file_stem()?.to_str()?;
    crate::sources::clean_text(stem.trim(), 128)
}

fn theme(format: ExtraFormat) -> OfficialExtra {
    OfficialExtra {
        category: ExtrasCategory::Themes,
        format,
        title_id: None,
        display_name: None,
        content_type: None,
    }
}

/// A `.p3t`: the theme XML's first `<name>` within the first 64 KiB
/// (plaintext in the package), else the file stem.
fn p3t(name: &str, b: &mut Budgeted) -> Option<OfficialExtra> {
    let n = usize::try_from(b.len().min(P3T_NAME_WINDOW)).ok()?;
    let window = b.read(0, n).ok()?;
    let mut x = theme(ExtraFormat::Ps3P3t);
    x.display_name = xml_name(&window).or_else(|| stem(name));
    Some(x)
}

/// The text of the first `<name>…</name>` in `bytes`, when it is plain
/// UTF-8 text without markup.
fn xml_name(bytes: &[u8]) -> Option<String> {
    const OPEN: &[u8] = b"<name>";
    const CLOSE: &[u8] = b"</name>";
    let start = bytes.windows(OPEN.len()).position(|w| w == OPEN)? + OPEN.len();
    let rest = bytes.get(start..)?;
    let len = rest.windows(CLOSE.len()).position(|w| w == CLOSE)?;
    let text = std::str::from_utf8(rest.get(..len)?).ok()?;
    if text.contains(['<', '>']) {
        return None;
    }
    crate::sources::clean_text(text.trim(), 128)
}

/// A PS3 theme or avatar PKG, or a Vita theme PKG, by its metadata
/// content type. Its name sits in the encrypted body, so the display
/// name is the file stem.
fn pkg(name: &str, b: &mut Budgeted) -> Option<OfficialExtra> {
    let (kind, ctype) = sony::pkg_content_type(b)?;
    let format = match (kind, ctype) {
        (1, PkgContentType::Theme) => ExtraFormat::Ps3ThemePkg,
        (1, PkgContentType::Avatar) => ExtraFormat::Ps3AvatarPkg,
        (2, PkgContentType::VitaTheme) => ExtraFormat::VitaThemePkg,
        _ => return None,
    };
    let mut x = theme(format);
    x.title_id = sony::pkg_title_id(b);
    x.content_type = Some(ctype.raw());
    x.display_name = stem(name);
    Some(x)
}

/// An Xbox 360 STFS extra by its content type; the title id and display
/// name from the same header reader the games use.
fn stfs(b: &mut Budgeted) -> Option<OfficialExtra> {
    let ctype = xbox::stfs_content_type(b)?;
    let (category, format) = match xbox::stfs_extra(ctype)? {
        StfsExtra::Theme => (ExtrasCategory::Themes, ExtraFormat::X360Theme),
        StfsExtra::GamerPicture => (ExtrasCategory::Themes, ExtraFormat::X360Gamerpic),
        StfsExtra::AvatarItem => (ExtrasCategory::Themes, ExtraFormat::X360Avatar),
        StfsExtra::Video => (ExtrasCategory::Videos, ExtraFormat::X360Video),
    };
    let id = xbox::stfs(b);
    Some(OfficialExtra {
        category,
        format,
        title_id: id.as_ref().and_then(|i| i.title_id.clone()),
        display_name: id.and_then(|i| i.title),
        content_type: Some(ctype),
    })
}

/// A 3DS soundtrack application: a CIA the 3DS reader proves to be an
/// application (`00040000`) whose SMDH title or file stem holds a whole
/// soundtrack word.
fn cia(name: &str, b: &mut Budgeted) -> Option<OfficialExtra> {
    let id = consoles::ctr::identify(b, "cia")?;
    if id.system != "3ds" || id.kind != TitleKind::Base {
        return None;
    }
    let title = id.title.clone();
    let stem = stem(name);
    let hit = title.as_deref().is_some_and(has_soundtrack_word)
        || stem.as_deref().is_some_and(has_soundtrack_word);
    hit.then(|| OfficialExtra {
        category: ExtrasCategory::Soundtrack,
        format: ExtraFormat::N3dsSoundtrackCia,
        title_id: id.title_id,
        display_name: title.or(stem),
        content_type: None,
    })
}

/// Whether `text` holds a soundtrack word as a whole word (split on
/// characters that are not alphanumeric, case-insensitive, NFC), or the
/// Japanese word anywhere.
pub fn has_soundtrack_word(text: &str) -> bool {
    let text: String = text.nfc().collect::<String>().to_lowercase();
    if SOUNDTRACK_SUBSTRINGS.iter().any(|w| text.contains(w)) {
        return true;
    }
    let words: Vec<&str> = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    SOUNDTRACK_WORDS
        .iter()
        .any(|phrase| words.windows(phrase.len()).any(|w| w == *phrase))
}

/// A PSP theme `.ptf`, by its extension, when no console reader claims
/// the bytes.
fn ptf(name: &str, b: &mut Budgeted) -> Option<OfficialExtra> {
    if !consoles::identify_all(b, "ptf", name).is_empty() {
        return None;
    }
    let mut x = theme(ExtraFormat::PspPtf);
    x.display_name = stem(name);
    Some(x)
}
