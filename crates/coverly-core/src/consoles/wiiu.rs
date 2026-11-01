//! Nintendo Wii U (spec §8.2): decrypted (loadiine) title folders, NUS
//! downloads, WUX and Cemu's WUA archives, RPX executables and WUHB
//! homebrew bundles.
//!
//! Sources: wiiubrew «meta.xml», «app.xml», «Title database» (title ids);
//! Cemu `src/Cafe/Filesystem/WUD/wud.{h,cpp}` (WUX header, read only);
//! devkitPro `wut-tools` `src/wuhbtool/services/RomFSService.cpp`
//! (`WUHB` at 0); a real loadiine folder, a real RPX head and a real WUA
//! checked on 2026-10-31 (BOM, two-line `longname_en`, `company_code`
//! `0001`, ELF `e_ident[7..9] = CA FE`, WUA root `<titleid>_v<n>`).
//!
//! Every input is in clear: `meta.xml`, `app.xml`, the TMD and the
//! archive tables. `title.tik`/`title.cetk` are key material and never
//! read; encrypted `.app` contents are never opened (D4).
//!
//! Not done (BLOCKED, no real WUD head to verify against): the product
//! code a WUD disc is said to start with. A `.wud` is identified by its
//! extension only, and a WUX by its own header without a serial, since
//! its sector 0 is a WUD's.

use std::fs;
use std::path::{Path, PathBuf};

use quick_xml::events::Event;
use quick_xml::Reader;

use super::zarchive::{self, Archive, Node};
use super::{
    kind_wiiu, ArtFormat, ArtLocator, Budgeted, ConsoleId, DirListing, FolderMatch, IdSource,
    TitleKind, MAX_META_FILE,
};
use crate::identify::sniff::FileAt;

/// Most entries read from the one child folder a probe lists (D24).
const MAX_CHILD_ENTRIES: usize = 256;
/// The TMD header up to the title version.
const TMD_HEAD: usize = 0x1E0;
const TMD_SIG_RSA2048: u32 = 0x0001_0004;
const TMD_TITLE_ID: usize = 0x18C;
const TMD_VERSION: usize = 0x1DC;
const WUX_MAGIC: [u8; 8] = [b'W', b'U', b'X', b'0', 0x2E, 0xD0, 0x99, 0x10];
const WUX_HEADER: u64 = 0x20;

fn be16(h: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_be_bytes(h.get(o..o + 2)?.try_into().ok()?))
}

fn be32(h: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_be_bytes(h.get(o..o + 4)?.try_into().ok()?))
}

fn be64(h: &[u8], o: usize) -> Option<u64> {
    Some(u64::from_be_bytes(h.get(o..o + 8)?.try_into().ok()?))
}

fn le32(h: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(h.get(o..o + 4)?.try_into().ok()?))
}

fn le64(h: &[u8], o: usize) -> Option<u64> {
    Some(u64::from_le_bytes(h.get(o..o + 8)?.try_into().ok()?))
}

/// A 16-hex title id, upper-cased, and its value.
fn title_id(raw: &str) -> Option<(String, u64)> {
    let t = raw.trim();
    if t.len() != 16 || !t.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let value = u64::from_str_radix(t, 16).ok()?;
    Some((t.to_ascii_uppercase(), value))
}

/// The children of the root element of a Wii U XML file, as text, for the
/// `wanted` names. `None` when the input is over [`MAX_META_FILE`], is
/// not well formed, has a DTD or uses an entity other than the five XML
/// ones (no DTD means no entity can be defined, so nothing expands).
pub(super) fn xml_fields(bytes: &[u8], wanted: &[&str]) -> Option<Vec<(String, String)>> {
    if bytes.len() as u64 > MAX_META_FILE {
        return None;
    }
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().expand_empty_elements = false;
    let mut out: Vec<(String, String)> = Vec::new();
    let mut depth = 0usize;
    let mut field: Option<usize> = None;
    let mut buf = Vec::new();
    loop {
        let event = reader.read_event_into(&mut buf).ok()?;
        match event {
            Event::DocType(_) => return None,
            Event::Start(e) => {
                depth += 1;
                field = None;
                if depth == 2 {
                    let name = std::str::from_utf8(e.local_name().as_ref())
                        .ok()?
                        .to_owned();
                    if wanted.contains(&name.as_str()) {
                        out.push((name, String::new()));
                        field = Some(out.len() - 1);
                    }
                }
            }
            Event::End(_) => {
                depth = depth.checked_sub(1)?;
                field = None;
            }
            Event::Text(t) => {
                if let Some(i) = field {
                    out[i].1.push_str(&t.decode().ok()?);
                }
            }
            Event::GeneralRef(r) => {
                let c = if r.is_char_ref() {
                    r.resolve_char_ref().ok()??
                } else {
                    match &*r {
                        b"lt" => '<',
                        b"gt" => '>',
                        b"amp" => '&',
                        b"apos" => '\'',
                        b"quot" => '"',
                        _ => return None,
                    }
                };
                if let Some(i) = field {
                    out[i].1.push(c);
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    (depth == 0).then_some(out)
}

pub(super) fn get<'a>(fields: &'a [(String, String)], name: &str) -> Option<&'a str> {
    fields
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, v)| v.trim())
        .filter(|v| !v.is_empty())
}

/// A title from a `meta.xml` name field: its line breaks become spaces.
fn display_name(raw: &str) -> Option<String> {
    let one_line = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    super::clean_title(&one_line)
}

/// `meta/meta.xml`: title id, version, product code (ID4, plus the maker
/// from `company_code` as an ID6 when it is `00XX`), the English name.
pub fn parse_meta_xml(bytes: &[u8]) -> Option<ConsoleId> {
    let f = xml_fields(
        bytes,
        &[
            "title_id",
            "title_version",
            "product_code",
            "company_code",
            "longname_en",
            "shortname_en",
            "longname_ja",
        ],
    )?;
    let (tid, value) = title_id(get(&f, "title_id")?)?;
    let mut id = ConsoleId::proven("wiiu", "meta.xml");
    id.title_id = Some(tid);
    id.kind = kind_wiiu(value);
    id.id_source = Some(IdSource::MetaXml);
    id.version = get(&f, "title_version").map(str::to_owned);
    let id4 = get(&f, "product_code")
        .and_then(|p| crate::disc::serial::normalise_console_serial("wiiu", p))
        .filter(|c| c.len() == 4);
    let maker = get(&f, "company_code")
        .and_then(|c| c.strip_prefix("00"))
        .filter(|m| m.len() == 2);
    id.serial = match (id4, maker) {
        (Some(code), Some(m)) => {
            crate::disc::serial::normalise_console_serial("wiiu", &format!("{code}{m}"))
                .or(Some(code))
        }
        (code, _) => code,
    };
    id.title = ["longname_en", "shortname_en", "longname_ja"]
        .iter()
        .find_map(|k| get(&f, k).and_then(display_name));
    Some(id)
}

/// `code/app.xml`: title id and version only. Its `title_version` is
/// `hexBinary` (`0010` is version 16), unlike meta.xml's decimal one, so
/// it is given in decimal like every other Wii U version.
pub fn parse_app_xml(bytes: &[u8]) -> Option<ConsoleId> {
    let f = xml_fields(bytes, &["title_id", "title_version"])?;
    let (tid, value) = title_id(get(&f, "title_id")?)?;
    let mut id = ConsoleId::proven("wiiu", "app.xml");
    id.title_id = Some(tid);
    id.kind = kind_wiiu(value);
    id.id_source = Some(IdSource::AppXml);
    id.version = get(&f, "title_version")
        .filter(|v| v.len() <= 8 && v.bytes().all(|b| b.is_ascii_hexdigit()))
        .and_then(|v| u32::from_str_radix(v, 16).ok())
        .map(|v| v.to_string());
    Some(id)
}

/// The listed name equal to `want` ignoring ASCII case.
pub(super) fn find<'a>(names: &'a [String], want: &str) -> Option<&'a str> {
    names
        .iter()
        .find(|n| n.eq_ignore_ascii_case(want))
        .map(String::as_str)
}

/// A regular file (never a symlink) of at most `max` bytes.
pub(super) fn plain_file(path: &Path, max: u64) -> Option<u64> {
    let meta = fs::symlink_metadata(path).ok()?;
    (meta.file_type().is_file() && meta.len() <= max).then_some(meta.len())
}

/// The first `n` bytes of `path` (or all of it when shorter), through
/// [`Budgeted`]; `path` was checked by [`plain_file`].
pub(super) fn read_head(path: &Path, len: u64, n: usize) -> Option<Vec<u8>> {
    let file = super::open_rom(path).ok()?;
    let mut src = FileAt(file);
    let mut b = Budgeted::new(&mut src, len);
    let n = usize::try_from(len).ok()?.min(n);
    b.read(0, n).ok()
}

/// Up to [`MAX_CHILD_ENTRIES`] names of the directory `dir` (never a
/// symlinked one).
pub(super) fn list_child(dir: &Path) -> Option<Vec<String>> {
    if !fs::symlink_metadata(dir).ok()?.file_type().is_dir() {
        return None;
    }
    #[cfg(test)]
    list_count::bump();
    Some(
        fs::read_dir(dir)
            .ok()?
            .flatten()
            .take(MAX_CHILD_ENTRIES)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect(),
    )
}

/// How many directories [`list_child`] listed on this thread (unit tests
/// of the one-listing rule, systems batch 3 D5).
#[cfg(test)]
pub(super) mod list_count {
    use std::cell::Cell;

    thread_local! {
        static COUNT: Cell<usize> = const { Cell::new(0) };
    }

    pub(in super::super) fn bump() {
        COUNT.with(|c| c.set(c.get() + 1));
    }

    /// The count since the last call, then zero.
    pub(in super::super) fn take() -> usize {
        COUNT.with(|c| c.replace(0))
    }
}

/// A decrypted (loadiine) title folder (spec D12): `code` and `meta` or
/// `content` among its names. `meta/meta.xml` is read (one listing of
/// `meta`), else `code/app.xml`; the icon is `meta/iconTex.tga`.
pub fn probe_loadiine(l: &DirListing) -> Option<FolderMatch> {
    let code = find(l.names, "code")?;
    let meta = find(l.names, "meta");
    if meta.is_none() && find(l.names, "content").is_none() {
        return None;
    }
    let meta_names = meta.and_then(|m| list_child(&l.dir.join(m)));
    let in_meta = |want: &str| -> Option<PathBuf> {
        let m = meta?;
        let name = find(meta_names.as_deref()?, want)?;
        Some(PathBuf::from(m).join(name))
    };
    let read = |rel: &Path| -> Option<Vec<u8>> {
        let path = l.dir.join(rel);
        let len = plain_file(&path, MAX_META_FILE)?;
        read_head(&path, len, MAX_META_FILE as usize)
    };
    let from_meta = in_meta("meta.xml").and_then(|rel| Some((parse_meta_xml(&read(&rel)?)?, rel)));
    let (mut id, probe) = match from_meta {
        Some(found) => found,
        None => {
            let rel = PathBuf::from(code).join("app.xml");
            (parse_app_xml(&read(&rel)?)?, rel)
        }
    };
    if let Some(rel) = in_meta("iconTex.tga") {
        if plain_file(&l.dir.join(&rel), u64::MAX).is_some() {
            id.art.push(ArtLocator::File {
                rel,
                format: ArtFormat::Tga,
            });
        }
    }
    Some(FolderMatch {
        id,
        probe,
        leave: None,
    })
}

/// A NUS download folder (spec D12): `title.tmd` among its names whose
/// title id is a Wii U one (`0005xxxx`). 3DS CDN (`0004xxxx`) and Wii
/// NUS (`0001xxxx`) folders are not folder games in this batch. Only the
/// TMD header is read; `title.tik` and the `.app` contents never are.
pub fn probe_nus(l: &DirListing) -> Option<FolderMatch> {
    let name = find(l.names, "title.tmd")?;
    let path = l.dir.join(name);
    let len = plain_file(&path, 1 << 20)?;
    let h = read_head(&path, len, TMD_HEAD)?;
    let id = tmd_id(&h, "NUS title.tmd")?;
    Some(FolderMatch {
        id,
        probe: PathBuf::from(name),
        leave: None,
    })
}

/// A Wii U title from a TMD header (same layout as the 3DS and Wii TMD).
fn tmd_id(h: &[u8], detail: &str) -> Option<ConsoleId> {
    if be32(h, 0)? != TMD_SIG_RSA2048 {
        return None;
    }
    let tid = be64(h, TMD_TITLE_ID)?;
    if tid >> 48 != 0x0005 {
        return None;
    }
    let mut id = ConsoleId::proven("wiiu", detail);
    id.title_id = Some(format!("{tid:016X}"));
    id.kind = kind_wiiu(tid);
    id.version = be16(h, TMD_VERSION).map(|v| v.to_string());
    id.id_source = Some(IdSource::Header);
    Some(id)
}

/// A Wii U file by its own header: WUX, RPX, WUHB, a WUA (`.wua` only:
/// its footer is at the end of the file), a lone `.tmd`.
pub fn identify(b: &mut Budgeted, ext: &str) -> Option<ConsoleId> {
    let n = usize::try_from(b.len().min(0x40)).ok()?;
    if n < 0x10 {
        return None;
    }
    let head = b.read(0, n).ok()?;
    if head.starts_with(&WUX_MAGIC) {
        return wux(b, &head);
    }
    if head.starts_with(b"\x7FELF") && head.get(4..9) == Some(&[1, 2, 1, 0xCA, 0xFE][..]) {
        // ELF32, big-endian, Cafe OS ABI (`EI_OSABI 0xCA`, `EI_ABIVERSION
        // 0xFE`): an RPX or RPL. Nothing in it names the title.
        return Some(ConsoleId::magic("wiiu", "RPX header"));
    }
    if head.starts_with(b"WUHB") {
        return Some(ConsoleId::magic("wiiu", "WUHB header"));
    }
    if ext == "wua" {
        return wua(b, ext);
    }
    if ext == "tmd" && head.len() >= 0x10 {
        let h = b
            .read(0, TMD_HEAD.min(usize::try_from(b.len()).ok()?))
            .ok()?;
        return tmd_id(&h, "title.tmd");
    }
    None
}

/// WUX (Cemu `wud.cpp`): sector size u32 LE at 8 (a power of two from
/// 0x100), uncompressed size u64 LE at 0x10, one u32 LE index per sector
/// from 0x20, sectors from the next sector boundary. The header and the
/// first index entry are checked against the file length; no sector is
/// read (the WUD product code rule is not verified, see the module docs).
fn wux(b: &Budgeted, head: &[u8]) -> Option<ConsoleId> {
    let magic = ConsoleId::magic("wiiu", "WUX header");
    let checked = (|| {
        let sector = u64::from(le32(head, 8)?);
        let size = le64(head, 0x10)?;
        if !sector.is_power_of_two() || !(0x100..0x1000_0000).contains(&sector) || size == 0 {
            return None;
        }
        let sectors = size.div_ceil(sector);
        let index_end = WUX_HEADER.checked_add(sectors.checked_mul(4)?)?;
        let array = index_end.checked_next_multiple_of(sector)?;
        (array < b.len()).then_some(())
    })();
    Some(match checked {
        Some(()) => ConsoleId::proven("wiiu", "WUX header"),
        None => magic,
    })
}

/// The kind words of the WUA detail line.
fn kind_word(kind: TitleKind) -> &'static str {
    match kind {
        TitleKind::Base => "base",
        TitleKind::Update => "update",
        TitleKind::Dlc => "dlc",
        TitleKind::Demo => "demo",
        TitleKind::Homebrew => "homebrew",
        TitleKind::Unknown => "unknown",
    }
}

/// A WUA root folder name: `<16 hex>_v<version>` of a Wii U title.
fn wua_title(name: &str) -> Option<(String, u64, String)> {
    let (tid, version) = name.split_once("_v")?;
    let (tid, value) = title_id(tid)?;
    (value >> 48 == 0x0005 && !version.is_empty() && version.bytes().all(|b| b.is_ascii_digit()))
        .then(|| (tid, value, version.to_owned()))
}

/// A Cemu WUA (ZArchive): one item for every title it holds (base,
/// update, DLC), named after the base and its `meta/meta.xml`. `None`
/// without a ZArchive footer, or for a ZArchive without Wii U titles
/// under any extension other than `.wua` (Xenia's `.zar` are Xbox 360).
pub fn wua(b: &mut Budgeted, ext: &str) -> Option<ConsoleId> {
    let footer = zarchive::footer(b)?;
    let magic = (ext == "wua").then(|| ConsoleId::magic("wiiu", "ZArchive footer"));
    let Some(archive) = footer.and_then(|f| zarchive::open(b, f)) else {
        return magic;
    };
    let titles: Vec<(usize, String, u64, String)> = archive
        .children(Archive::ROOT)
        .filter(|&i| matches!(archive.node(i), Some(Node::Dir { .. })))
        .filter_map(|i| {
            let (tid, value, version) = wua_title(archive.name(i)?)?;
            Some((i, tid, value, version))
        })
        .collect();
    if titles.is_empty() {
        return magic;
    }
    let base = titles
        .iter()
        .find(|t| kind_wiiu(t.2) == TitleKind::Base)
        .unwrap_or(&titles[0]);
    let mut kinds: Vec<&str> = Vec::new();
    for t in &titles {
        let word = kind_word(kind_wiiu(t.2));
        if !kinds.contains(&word) {
            kinds.push(word);
        }
    }
    let mut id = ConsoleId::proven("wiiu", &format!("WUA: {}", kinds.join(", ")));
    id.title_id = Some(base.1.clone());
    id.kind = kind_wiiu(base.2);
    id.version = Some(base.3.clone());
    id.id_source = Some(IdSource::Header);
    let meta = archive
        .lookup(base.0, &["meta", "meta.xml"])
        .and_then(|i| archive.read_file(b, i, MAX_META_FILE))
        .and_then(|bytes| parse_meta_xml(&bytes));
    if let Some(meta) = meta {
        id.serial = meta.serial;
        id.title = meta.title;
    }
    Some(id)
}

#[cfg(test)]
mod tests {
    use super::super::testing::{self as t, RangeLog, SparseAt};
    use super::*;

    fn budget(s: &mut SparseAt) -> Budgeted<'_> {
        let len = s.len;
        Budgeted::new(s, len)
    }

    #[test]
    fn meta_xml_fields() {
        let xml = t::meta_xml("0005000010145D00", "WUP-P-ARDP", "Super Mario 3D World");
        let id = parse_meta_xml(xml.as_bytes()).unwrap();
        assert_eq!(id.system, "wiiu");
        assert_eq!(id.title_id.as_deref(), Some("0005000010145D00"));
        assert_eq!(id.serial.as_deref(), Some("ARDP01"));
        assert_eq!(id.version.as_deref(), Some("0"));
        assert_eq!(id.kind, TitleKind::Base);
        assert_eq!(id.id_source, Some(IdSource::MetaXml));
        assert_eq!(id.title.as_deref(), Some("Super Mario 3D World (Test)"));
        assert_eq!(id.confidence, 0.95);
    }

    #[test]
    fn meta_xml_escapes_and_fallbacks() {
        let xml = t::meta_xml("0005000E10145D00", "WUP-P-ARDP", "Tom &amp; Jerry &#233;").replace(
            "<company_code type=\"string\" length=\"8\">0001</company_code>",
            "",
        );
        let id = parse_meta_xml(xml.as_bytes()).unwrap();
        assert_eq!(id.title.as_deref(), Some("Tom & Jerry é (Test)"));
        assert_eq!(id.serial.as_deref(), Some("ARDP"), "no maker: the ID4");
        assert_eq!(id.kind, TitleKind::Update);
        let no_en = t::meta_xml("0005000010145D00", "WUP-P-ARDP", "")
            .replace("\n(Test)", "")
            .replace(">Short<", "><");
        assert_eq!(
            parse_meta_xml(no_en.as_bytes()).unwrap().title.as_deref(),
            Some("ゲーム")
        );
    }

    #[test]
    fn meta_xml_with_doctype_is_rejected() {
        let xml = t::meta_xml("0005000010145D00", "WUP-P-ARDP", "&ent;")
            .replace("<menu ", "<!DOCTYPE menu [<!ENTITY ent \"x\">]>\n<menu ");
        assert_eq!(parse_meta_xml(xml.as_bytes()), None);
        // An undefined entity without a DTD is refused too.
        let xml = t::meta_xml("0005000010145D00", "WUP-P-ARDP", "&ent;");
        assert_eq!(parse_meta_xml(xml.as_bytes()), None);
    }

    #[test]
    fn meta_xml_over_64k_is_rejected() {
        let mut xml = t::meta_xml("0005000010145D00", "WUP-P-ARDP", "Game");
        xml.push_str(&" ".repeat(MAX_META_FILE as usize));
        assert_eq!(parse_meta_xml(xml.as_bytes()), None);
        assert_eq!(parse_meta_xml(b"<menu><title_id>"), None, "not well formed");
        let bad_id = t::meta_xml("00050000XYZ45D00", "WUP-P-ARDP", "Game");
        assert_eq!(parse_meta_xml(bad_id.as_bytes()), None);
    }

    #[test]
    fn app_xml_title_id() {
        let id = parse_app_xml(t::app_xml("0005000C10145D00").as_bytes()).unwrap();
        assert_eq!(id.title_id.as_deref(), Some("0005000C10145D00"));
        assert_eq!(id.kind, TitleKind::Dlc);
        // `title_version` is `hexBinary` in app.xml (decimal in meta.xml
        // and the TMD): `0010` is version 16.
        assert_eq!(id.version.as_deref(), Some("16"));
        assert_eq!(id.id_source, Some(IdSource::AppXml));
        assert_eq!(id.serial, None);
        let odd = t::app_xml("0005000010145D00").replace(">0010<", ">00G0<");
        assert_eq!(parse_app_xml(odd.as_bytes()).unwrap().version, None);
    }

    #[test]
    fn loadiine_probe_reads_meta_xml_and_finds_the_icon() {
        let dir = tempfile::tempdir().unwrap();
        t::wiiu_folder(dir.path(), "0005000010145D00", "WUP-P-ARDP", "Game");
        let names = ["code".to_string(), "content".into(), "meta".into()];
        let found = probe_loadiine(&DirListing {
            dir: dir.path(),
            names: &names,
        })
        .unwrap();
        assert_eq!(found.probe, Path::new("meta").join("meta.xml"));
        assert_eq!(found.id.id_source, Some(IdSource::MetaXml));
        assert_eq!(
            found.id.art,
            [ArtLocator::File {
                rel: Path::new("meta").join("iconTex.tga"),
                format: ArtFormat::Tga
            }]
        );
        // Without `meta`, `code/app.xml` answers.
        fs::remove_dir_all(dir.path().join("meta")).unwrap();
        let names = ["code".to_string(), "content".into()];
        let found = probe_loadiine(&DirListing {
            dir: dir.path(),
            names: &names,
        })
        .unwrap();
        assert_eq!(found.probe, Path::new("code").join("app.xml"));
        assert_eq!(found.id.id_source, Some(IdSource::AppXml));
        assert!(found.id.art.is_empty());
        // `code` alone is not a title folder.
        let names = ["code".to_string()];
        assert!(probe_loadiine(&DirListing {
            dir: dir.path(),
            names: &names
        })
        .is_none());
    }

    #[test]
    fn nus_probe_wiiu_tmd_only() {
        let dir = tempfile::tempdir().unwrap();
        t::nus_folder(dir.path(), 0x0005_0000_1010_ED00);
        let names = [
            "00000000.app".to_string(),
            "title.tik".into(),
            "title.tmd".into(),
        ];
        let found = probe_nus(&DirListing {
            dir: dir.path(),
            names: &names,
        })
        .unwrap();
        assert_eq!(found.probe, Path::new("title.tmd"));
        assert_eq!(found.id.title_id.as_deref(), Some("000500001010ED00"));
        assert_eq!(found.id.version.as_deref(), Some("32"));
        assert_eq!(found.id.detail, "NUS title.tmd");
    }

    #[test]
    fn nus_3ds_tmd_is_not_a_folder_game() {
        for tid in [0x0004_0000_0003_0800u64, 0x0001_0001_5741_4145] {
            let dir = tempfile::tempdir().unwrap();
            t::nus_folder(dir.path(), tid);
            let names = ["title.tmd".to_string()];
            assert!(probe_nus(&DirListing {
                dir: dir.path(),
                names: &names
            })
            .is_none());
        }
    }

    #[test]
    fn wux_header() {
        let mut s = t::wux();
        let id = identify(&mut budget(&mut s), "wux").unwrap();
        assert_eq!((id.system.as_str(), id.confidence), ("wiiu", 0.95));
        assert_eq!(id.serial, None);
        // A sector size that is not a power of two: magic only.
        let mut s = t::wux();
        s.put(8, &0x8001u32.to_le_bytes());
        assert_eq!(
            identify(&mut budget(&mut s), "wux").unwrap().confidence,
            0.90
        );
        // An index table past the end of the file: magic only.
        let mut s = t::wux();
        s.put(0x10, &u64::MAX.to_le_bytes());
        assert_eq!(
            identify(&mut budget(&mut s), "wux").unwrap().confidence,
            0.90
        );
    }

    #[test]
    fn rpx_and_wuhb_are_magic_only() {
        let mut rpx = SparseAt::new(0x1000);
        rpx.put(0, &[0x7F, b'E', b'L', b'F', 1, 2, 1, 0xCA, 0xFE]);
        let id = identify(&mut budget(&mut rpx), "rpx").unwrap();
        assert_eq!((id.detail.as_str(), id.confidence), ("RPX header", 0.90));
        let mut elf = SparseAt::new(0x1000);
        elf.put(0, &[0x7F, b'E', b'L', b'F', 1, 2, 1, 0, 0]);
        assert_eq!(identify(&mut budget(&mut elf), "rpx"), None);
        let mut wuhb = SparseAt::new(0x1000);
        wuhb.put(0, b"WUHB");
        assert_eq!(
            identify(&mut budget(&mut wuhb), "wuhb").unwrap().detail,
            "WUHB header"
        );
    }

    #[test]
    fn lone_tmd_by_extension() {
        let mut s = SparseAt::new(0xB34);
        s.put(0, &t::tmd(0x0005_000E_1010_ED00));
        let id = identify(&mut budget(&mut s), "tmd").unwrap();
        assert_eq!(id.kind, TitleKind::Update);
        let mut s = SparseAt::new(0xB34);
        s.put(0, &t::tmd(0x0004_0000_0003_0800));
        assert_eq!(identify(&mut budget(&mut s), "tmd"), None);
    }

    #[test]
    fn zarchive_lists_titles_and_reads_meta() {
        let mut s = t::wua();
        let id = identify(&mut budget(&mut s), "wua").unwrap();
        assert_eq!(id.detail, "WUA: base, update");
        assert_eq!(id.title_id.as_deref(), Some("0005000010145D00"));
        assert_eq!(id.kind, TitleKind::Base);
        assert_eq!(id.version.as_deref(), Some("0"));
        assert_eq!(id.serial.as_deref(), Some("ARDP01"));
        assert_eq!(id.title.as_deref(), Some("Super Mario 3D World (Test)"));
        // A ZArchive without Wii U titles is not a WUA under `.zar`.
        let mut zar = t::zarchive(&[("default.xex", b"XEX2".as_slice())]);
        assert_eq!(wua(&mut budget(&mut zar), "zar"), None);
        assert_eq!(wua(&mut budget(&mut zar), "wua").unwrap().confidence, 0.90);
    }

    #[test]
    fn zarchive_adversarial_footer() {
        let mut s = t::wua();
        let tail = s.len - zarchive::FOOTER_LEN;
        s.put(tail + 0x30, &(s.len - 4).to_be_bytes());
        s.put(tail + 0x38, &u64::MAX.to_be_bytes());
        let len = s.len;
        let mut log = RangeLog::new(&mut s);
        let mut b = Budgeted::new(&mut log, len);
        let id = identify(&mut b, "wua").unwrap();
        assert_eq!(
            (id.detail.as_str(), id.confidence),
            ("ZArchive footer", 0.90)
        );
        assert!(b.used().read <= super::super::MAX_READ);
    }

    #[test]
    fn budget_wua_reads_only_tables_and_one_block() {
        let mut s = t::wua();
        let len = s.len;
        let mut b = Budgeted::new(&mut s, len);
        wua(&mut b, "wua").unwrap();
        assert!(b.used().decoded <= zarchive::BLOCK, "{:?}", b.used());
        assert!(b.used().seeks <= super::super::MAX_SEEKS);
    }
}
