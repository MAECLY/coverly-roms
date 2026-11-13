//! A game folder's `Extras/` (extras spec D1, D2, D14, D20): fixed suggested
//! categories with English names on disk, user categories inside
//! `Extras/`, the `extras.json` model and the name and tag rules.

pub mod index;
pub mod official;
pub mod ops;
pub mod rips;
#[cfg(any(test, feature = "test-util"))]
pub mod testing;

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use unicode_normalization::UnicodeNormalization;

use crate::organize::target::WINDOWS_MAX_PATH;
use crate::write::{name_key, safe_file_name};

/// The index file inside `Extras/` (D2).
pub const EXTRAS_JSON: &str = "extras.json";
/// Longest user category name, in characters (D1).
pub const MAX_CATEGORY_CHARS: usize = 64;
/// Longest tag, in characters (D20).
pub const MAX_TAG_CHARS: usize = 64;
/// Most tags one file carries (D20).
pub const MAX_TAGS: usize = 32;
/// Longest file name most file systems store, in bytes.
const MAX_NAME_BYTES: usize = 255;

/// The fixed suggested categories, with English folder names on disk (D1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExtrasCategory {
    Soundtrack,
    Manuals,
    Artwork,
    Videos,
    Themes,
    Guides,
}

impl ExtrasCategory {
    pub const ALL: [ExtrasCategory; 6] = [
        Self::Soundtrack,
        Self::Manuals,
        Self::Artwork,
        Self::Videos,
        Self::Themes,
        Self::Guides,
    ];

    /// The folder name on disk.
    pub fn dir_name(self) -> &'static str {
        match self {
            Self::Soundtrack => "Soundtrack",
            Self::Manuals => "Manuals",
            Self::Artwork => "Artwork",
            Self::Videos => "Videos",
            Self::Themes => "Themes",
            Self::Guides => "Guides",
        }
    }

    /// The fixed category a folder name stands for, ignoring case and
    /// Unicode normalization; anything else (plurals, spaces) is not one.
    pub fn from_dir_name(name: &str) -> Option<Self> {
        let key = name_key(name);
        Self::ALL
            .into_iter()
            .find(|c| name_key(c.dir_name()) == key)
    }
}

/// Where a file sits inside `Extras/`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", content = "name", rename_all = "kebab-case")]
pub enum CategoryRef {
    Fixed(ExtrasCategory),
    User(String),
    Root,
}

/// What a file is, from its extension (D2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExtraType {
    Audio,
    Video,
    Image,
    Document,
    Other,
}

const AUDIO: &[&str] = &[
    "mp3", "m4a", "aac", "wav", "flac", "ogg", "opus", "spc", "nsf", "nsfe", "gbs", "vgm", "vgz",
    "psf", "minipsf", "psf2", "gsf", "minigsf", "usf", "miniusf", "2sf", "mini2sf", "ssf", "dsf",
    "qsf",
];
const VIDEO: &[&str] = &["mp4", "m4v", "webm", "mov", "mkv", "avi"];
const IMAGE: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "avif", "bmp", "svg", "heic", "tif", "tiff",
];
const DOCUMENT: &[&str] = &["pdf", "txt", "md", "nfo", "cbz", "epub"];

/// The type of a file with extension `ext` (no dot, any case).
pub fn type_of(ext: &str) -> ExtraType {
    let e = ext.to_ascii_lowercase();
    let has = |list: &[&str]| list.contains(&e.as_str());
    if has(AUDIO) {
        ExtraType::Audio
    } else if has(VIDEO) {
        ExtraType::Video
    } else if has(IMAGE) {
        ExtraType::Image
    } else if has(DOCUMENT) {
        ExtraType::Document
    } else {
        ExtraType::Other
    }
}

const MANUAL_WORDS: &[&str] = &["manual", "manuel", "handbuch", "manuale", "説明書"];
const PACKAGES: &[&str] = &["p3t", "pkg", "ptf", "cia"];

/// The category «Añadir» suggests for a file name (D13).
pub fn suggested_category(name: &str) -> ExtrasCategory {
    let path = Path::new(name);
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if PACKAGES.contains(&ext.as_str()) {
        return ExtrasCategory::Themes;
    }
    match type_of(&ext) {
        ExtraType::Audio => ExtrasCategory::Soundtrack,
        ExtraType::Video => ExtrasCategory::Videos,
        ExtraType::Image => ExtrasCategory::Artwork,
        ExtraType::Document if ext == "pdf" => {
            let stem = name_key(path.file_stem().and_then(|s| s.to_str()).unwrap_or(""));
            if MANUAL_WORDS.iter().any(|w| stem.contains(w)) {
                ExtrasCategory::Manuals
            } else {
                ExtrasCategory::Guides
            }
        }
        ExtraType::Document | ExtraType::Other => ExtrasCategory::Guides,
    }
}

/// Why a typed category or file name is refused (never rewritten).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NameRefusal {
    Empty,
    TooLong,
    NotPortable,
    Dot,
    Reserved,
    Taken,
    PathTooLong,
}

/// Why a tag list is refused (D20).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TagRefusal {
    Empty,
    TooLong,
    TooMany,
}

/// Whether `name` is the same name on every system: no separators and
/// unchanged by the FAT/exFAT/Windows rules.
fn portable(name: &str) -> bool {
    !name.contains(['/', '\\']) && safe_file_name(name).is_ok_and(|s| s == name)
}

/// Whether a name key (see [`name_key`]) is the index or one of its
/// malformed-file backups, which no folder or file may take (D1, D5).
fn is_reserved_key(key: &str) -> bool {
    key == EXTRAS_JSON || (key.starts_with("extras.broken-") && key.ends_with(".json"))
}

/// A user category name: trimmed, NFC, refused (never rewritten) when it
/// would not be the same name on every system (D1).
pub fn validate_category_name(raw: &str, existing_user: &[String]) -> Result<String, NameRefusal> {
    let name: String = raw.trim().nfc().collect();
    if name.is_empty() {
        return Err(NameRefusal::Empty);
    }
    if name.chars().count() > MAX_CATEGORY_CHARS {
        return Err(NameRefusal::TooLong);
    }
    if name.starts_with('.') {
        return Err(NameRefusal::Dot);
    }
    if !portable(&name) {
        return Err(NameRefusal::NotPortable);
    }
    let key = name_key(&name);
    if is_reserved_key(&key) {
        return Err(NameRefusal::Reserved);
    }
    if ExtrasCategory::from_dir_name(&name).is_some()
        || existing_user.iter().any(|u| name_key(u) == key)
    {
        return Err(NameRefusal::Taken);
    }
    Ok(name)
}

/// A new file name in `dir` from a typed stem, keeping `ext` (D14).
pub fn validate_file_stem(dir: &Path, stem: &str, ext: &str) -> Result<String, NameRefusal> {
    let stem: String = stem.trim().nfc().collect();
    if stem.is_empty() {
        return Err(NameRefusal::Empty);
    }
    let name = if ext.is_empty() {
        stem
    } else {
        format!("{stem}.{ext}")
    };
    if name.len() > MAX_NAME_BYTES {
        return Err(NameRefusal::TooLong);
    }
    if name.starts_with('.') {
        return Err(NameRefusal::Dot);
    }
    if !portable(&name) {
        return Err(NameRefusal::NotPortable);
    }
    if is_reserved_key(&name_key(&name)) {
        return Err(NameRefusal::Reserved);
    }
    if dir.join(&name).to_string_lossy().encode_utf16().count() > WINDOWS_MAX_PATH {
        return Err(NameRefusal::PathTooLong);
    }
    Ok(name)
}

/// Tags trimmed, NFC, deduplicated ignoring case (the first spelling
/// wins) and capped (D20).
pub fn normalize_tags(raw: &[String]) -> Result<Vec<String>, TagRefusal> {
    let mut out: Vec<String> = Vec::new();
    for t in raw {
        let t: String = t.trim().nfc().collect();
        if t.is_empty() {
            return Err(TagRefusal::Empty);
        }
        if t.chars().count() > MAX_TAG_CHARS {
            return Err(TagRefusal::TooLong);
        }
        let key = t.to_lowercase();
        if !out.iter().any(|o| o.to_lowercase() == key) {
            out.push(t);
        }
    }
    if out.len() > MAX_TAGS {
        return Err(TagRefusal::TooMany);
    }
    Ok(out)
}

/// Whether a file came with the game (detected by Organizar) or from the user.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Provenance {
    Official,
    User,
}

/// How a file got into `Extras/`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OriginHow {
    Organize,
    Added,
    Found,
}

/// The detected format of an official extra or a music rip.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExtraFormat {
    #[serde(rename = "ps3-p3t")]
    Ps3P3t,
    #[serde(rename = "ps3-theme-pkg")]
    Ps3ThemePkg,
    #[serde(rename = "ps3-avatar-pkg")]
    Ps3AvatarPkg,
    #[serde(rename = "vita-theme-pkg")]
    VitaThemePkg,
    #[serde(rename = "psp-ptf")]
    PspPtf,
    #[serde(rename = "x360-theme")]
    X360Theme,
    #[serde(rename = "x360-gamerpic")]
    X360Gamerpic,
    #[serde(rename = "x360-avatar")]
    X360Avatar,
    #[serde(rename = "x360-video")]
    X360Video,
    #[serde(rename = "3ds-soundtrack-cia")]
    N3dsSoundtrackCia,
    Spc,
    Nsf,
    Gbs,
    Vgm,
    Psf,
}

/// Tags read from a music rip's own header (D15).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RipTags {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,
    /// System ids the format implies (D15), for linking; not stored.
    #[serde(skip)]
    pub systems: &'static [&'static str],
    #[serde(flatten)]
    pub rest: BTreeMap<String, Value>,
}

/// Where a file came from; unknown fields are kept as they were.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Origin {
    pub how: OriginHow,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<ExtraFormat>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rip: Option<RipTags>,
    #[serde(flatten)]
    pub rest: BTreeMap<String, Value>,
}

/// One file in `extras.json`; unknown fields are kept as they were.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExtraRecord {
    /// `/`-separated, relative to `Extras/`, the on-disk spelling.
    pub path: String,
    #[serde(rename = "type")]
    pub kind: ExtraType,
    pub provenance: Provenance,
    pub origin: Origin,
    #[serde(default)]
    pub tags: Vec<String>,
    pub size: u64,
    pub mtime_ns: i64,
    pub added_at: String,
    #[serde(flatten)]
    pub rest: BTreeMap<String, Value>,
}

/// Per-category settings; unknown fields are kept as they were.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CategoryMeta {
    /// Copy to devices: `None` inherits the default (block 4), D21.
    /// Serialized as `null`.
    #[serde(default)]
    pub sync: Option<bool>,
    #[serde(flatten)]
    pub rest: BTreeMap<String, Value>,
}

/// The whole `Extras/extras.json`; unknown fields pass through (D2).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExtrasFile {
    pub version: u32,
    #[serde(default)]
    pub categories: BTreeMap<String, CategoryMeta>,
    #[serde(default)]
    pub files: Vec<ExtraRecord>,
    #[serde(flatten)]
    pub rest: BTreeMap<String, Value>,
}

impl ExtrasFile {
    pub const VERSION: u32 = 1;

    /// A fresh index with no categories and no files.
    pub fn empty() -> Self {
        Self {
            version: Self::VERSION,
            categories: BTreeMap::new(),
            files: Vec::new(),
            rest: BTreeMap::new(),
        }
    }

    /// The sync flag of `category`: the exact spelling first, else any
    /// spelling equal ignoring case and normalization (D1); `None` when
    /// unset or inheriting.
    pub fn sync_for(&self, category: &str) -> Option<bool> {
        let meta = self.categories.get(category).or_else(|| {
            let key = name_key(category);
            self.categories
                .iter()
                .find(|(k, _)| name_key(k) == key)
                .map(|(_, m)| m)
        });
        meta.and_then(|m| m.sync)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_categories_have_english_names_and_match_case_insensitively() {
        let names: Vec<_> = ExtrasCategory::ALL.iter().map(|c| c.dir_name()).collect();
        assert_eq!(
            names,
            [
                "Soundtrack",
                "Manuals",
                "Artwork",
                "Videos",
                "Themes",
                "Guides"
            ]
        );
        assert_eq!(
            ExtrasCategory::from_dir_name("soundtrack"),
            Some(ExtrasCategory::Soundtrack)
        );
        assert_eq!(ExtrasCategory::from_dir_name("Soundtracks"), None);
        assert_eq!(ExtrasCategory::from_dir_name("Manuals "), None);
    }

    #[test]
    fn type_comes_from_the_extension() {
        for (ext, want) in [
            ("flac", ExtraType::Audio),
            ("MP3", ExtraType::Audio),
            ("opus", ExtraType::Audio),
            ("mp4", ExtraType::Video),
            ("webm", ExtraType::Video),
            ("mov", ExtraType::Video),
            ("png", ExtraType::Image),
            ("svg", ExtraType::Image),
            ("heic", ExtraType::Image),
            ("pdf", ExtraType::Document),
            ("txt", ExtraType::Document),
            ("nfo", ExtraType::Document),
            ("p3t", ExtraType::Other),
            ("pkg", ExtraType::Other),
            ("cia", ExtraType::Other),
            ("spc", ExtraType::Audio),
            ("nsf", ExtraType::Audio),
            ("exe", ExtraType::Other),
        ] {
            assert_eq!(type_of(ext), want, "{ext}");
        }
    }

    #[test]
    fn suggested_category_by_type_and_manual_words() {
        assert_eq!(
            suggested_category("01 Opening.flac"),
            ExtrasCategory::Soundtrack
        );
        assert_eq!(suggested_category("Trailer.mp4"), ExtrasCategory::Videos);
        assert_eq!(suggested_category("Box front.png"), ExtrasCategory::Artwork);
        assert_eq!(
            suggested_category("Walkthrough.pdf"),
            ExtrasCategory::Guides
        );
        assert_eq!(
            suggested_category("Super Metroid Manual.pdf"),
            ExtrasCategory::Manuals
        );
        assert_eq!(suggested_category("Handbuch.PDF"), ExtrasCategory::Manuals);
        assert_eq!(suggested_category("説明書.pdf"), ExtrasCategory::Manuals);
        assert_eq!(suggested_category("Theme.p3t"), ExtrasCategory::Themes);
    }

    #[test]
    fn category_names_are_refused_never_rewritten() {
        let existing = ["Remixes".to_string()];
        assert_eq!(
            validate_category_name("  Live  ", &existing),
            Ok("Live".into())
        );
        let too_long = "x".repeat(65);
        for (bad, why) in [
            ("", NameRefusal::Empty),
            ("a:b", NameRefusal::NotPortable),
            ("CON", NameRefusal::NotPortable),
            ("Trailing.", NameRefusal::NotPortable),
            (".hidden", NameRefusal::Dot),
            ("extras.json", NameRefusal::Reserved),
            ("soundtrack", NameRefusal::Taken),
            ("REMIXES", NameRefusal::Taken),
            (too_long.as_str(), NameRefusal::TooLong),
            ("a/b", NameRefusal::NotPortable),
        ] {
            assert_eq!(validate_category_name(bad, &existing), Err(why), "{bad:?}");
        }
        // NFD input is stored NFC.
        assert_eq!(
            validate_category_name("Cancio\u{301}n", &[]),
            Ok("Canción".into())
        );
    }

    #[test]
    fn stems_keep_the_extension_and_respect_windows_paths() {
        let dir = std::path::Path::new("/L/Game/Extras/Soundtrack");
        assert_eq!(
            validate_file_stem(dir, "Boss (live)", "flac"),
            Ok("Boss (live).flac".into())
        );
        assert_eq!(
            validate_file_stem(dir, "a?b", "flac"),
            Err(NameRefusal::NotPortable)
        );
        let long = "x".repeat(250);
        assert_eq!(
            validate_file_stem(dir, &long, "flac"),
            Err(NameRefusal::PathTooLong)
        );
    }

    #[test]
    fn tags_are_trimmed_nfc_deduped_and_capped() {
        let got = normalize_tags(&[
            "  tema del jefe final ".into(),
            "Tema del jefe final".into(),
            "versión orquestal".into(),
        ])
        .unwrap();
        assert_eq!(got, ["tema del jefe final", "versión orquestal"]);
        assert_eq!(normalize_tags(&["".into()]), Err(TagRefusal::Empty));
        assert_eq!(normalize_tags(&["x".repeat(65)]), Err(TagRefusal::TooLong));
        let many: Vec<String> = (0..33).map(|i| format!("t{i}")).collect();
        assert_eq!(normalize_tags(&many), Err(TagRefusal::TooMany));
    }

    #[test]
    fn extras_file_keeps_unknown_fields_and_sync_tristate() {
        let text = r#"{"version":1,"future":{"a":1},"categories":{"Manuals":{"sync":true},"Videos":{"sync":null}},
            "files":[{"path":"Soundtrack/01.flac","type":"audio","provenance":"user","origin":{"how":"added"},
            "tags":[],"size":1,"mtime_ns":0,"added_at":"2026-11-13T00:00:00Z","rating":5}]}"#;
        let f: ExtrasFile = serde_json::from_str(text).unwrap();
        assert_eq!(f.sync_for("Manuals"), Some(true));
        assert_eq!(f.sync_for("Videos"), None);
        assert_eq!(f.sync_for("Artwork"), None);
        let back = serde_json::to_value(&f).unwrap();
        assert_eq!(back["future"]["a"], 1);
        assert_eq!(back["files"][0]["rating"], 5);
    }

    #[test]
    fn sync_false_is_kept_apart_from_inherit() {
        let text = r#"{"version":1,"categories":{"Videos":{"sync":false},"Manuals":{}}}"#;
        let f: ExtrasFile = serde_json::from_str(text).unwrap();
        assert_eq!(f.sync_for("videos"), Some(false));
        assert_eq!(f.sync_for("Manuals"), None);
        let back = serde_json::to_value(&f).unwrap();
        assert_eq!(back["categories"]["Videos"]["sync"], false);
        assert!(back["categories"]["Manuals"]["sync"].is_null());
    }

    #[test]
    fn sync_prefers_the_exact_spelling_of_a_category() {
        let text =
            r#"{"version":1,"categories":{"Manuals":{"sync":null},"manuals":{"sync":true}}}"#;
        let f: ExtrasFile = serde_json::from_str(text).unwrap();
        assert_eq!(f.sync_for("manuals"), Some(true));
        assert_eq!(f.sync_for("Manuals"), None);
    }

    #[test]
    fn unknown_rip_fields_survive_a_rewrite() {
        let text =
            r#"{"how":"organize","format":"spc","rip":{"game":"Super Metroid","album":"OST"}}"#;
        let o: Origin = serde_json::from_str(text).unwrap();
        assert_eq!(
            o.rip.as_ref().unwrap().game.as_deref(),
            Some("Super Metroid")
        );
        let back = serde_json::to_value(&o).unwrap();
        assert_eq!(back["rip"]["album"], "OST");
    }

    #[test]
    fn a_file_is_never_renamed_to_the_index_or_its_backups() {
        let dir = std::path::Path::new("/L/Game/Extras");
        assert_eq!(
            validate_file_stem(dir, "extras", "json"),
            Err(NameRefusal::Reserved)
        );
        assert_eq!(
            validate_file_stem(dir, "Extras", "JSON"),
            Err(NameRefusal::Reserved)
        );
        assert_eq!(
            validate_file_stem(dir, "extras.broken-20261113-120000", "json"),
            Err(NameRefusal::Reserved)
        );
        assert_eq!(
            validate_file_stem(dir, "extras notes", "json"),
            Ok("extras notes.json".into())
        );
    }
}
