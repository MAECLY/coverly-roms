//! Nintendo Switch (spec §8.3): XCI/XCZ cartridge dumps, NSP/NSZ
//! packages and split «archive bit» folders — only what is in clear.
//!
//! Sources: switchbrew «XCI» (card header: `HEAD` at 0x100, the root
//! partition's address u64 at 0x130 and size at 0x138, the encrypted
//! gamecard info at 0x190; HFS0: `HFS0`, file count, string table size,
//! 0x40-byte entries) and «NCA Format» (PFS0: `PFS0`, file count, string
//! table size, 0x18-byte entries `offset u64, size u64, name u32`, data
//! after the string table). An ordinary dump starts with the card header;
//! a dump that keeps the 0x1000-byte key area has it at 0x1000, and its
//! addresses are then counted from there.
//!
//! What is and isn't possible, enforced here (D4, D10):
//! - XCI/XCZ: the card header up to 0x13F, the root HFS0 entry names
//!   (`update`, `normal`, `secure`, `logo`) and the secure partition's
//!   plaintext HFS0 header and names table (adjustments D6) are read. The
//!   gamecard info at 0x190 is encrypted with a console key and is never
//!   read, nor is any NCA. The secure HFS0 lists NCA ids (content
//!   hashes), not title ids: they are kept as [`ConsoleId::nca_ids`] and
//!   the title id comes from the file name, or later from the optional
//!   catalogue's `ncas.json` (`organize::evidence::resolve_switch_title`).
//! - NSP/NSZ: the PFS0 table is read. The title id comes from a ticket's
//!   **entry name** (`<rights id>.tik`, whose first 16 hex digits are the
//!   title id), else from a plaintext `*.cnmt.xml` entry (≤ 64 KiB), else
//!   from the file name; without a ticket or `cnmt.xml` the
//!   `<32 hex>.nca`/`.ncz` entry names are kept as NCA ids. The bodies of
//!   `.tik`, `.cert`, `.nca` and `.ncz` entries are never read.
//! - A loose `<32 hex>.nca`/`.ncz` is known by its name alone (part
//!   [`PartKind::Nca`]); not one byte of it is read.
//! - NRO (homebrew): `NRO0` at 0x10, its size at 0x18 and the `ASET`
//!   asset section there: the NACP title (first non-empty language),
//!   `DisplayVersion` and the JPEG icon's byte range. NSO: part of a
//!   game's ExeFS ([`PartKind::ExeFs`]).
//! - Any format: `[0100…]` (16 hex) and `[vN]` tokens of the file name.
//! - Nothing here downloads anything, and no key is ever needed.

use std::path::PathBuf;

use super::wiiu::{find, get, plain_file, xml_fields};
use super::{
    clean_title, kind_switch, ArtFormat, ArtLocator, Budgeted, ConsoleId, DirListing, FolderMatch,
    IdSource, PartKind, TitleKind, TitleSource, MAX_META_FILE,
};
use crate::identify::sniff::{FileAt, ReadAt};

/// Most entries a PFS0/HFS0 table may declare before it is distrusted.
const MAX_ENTRIES: u32 = 1024;
/// Largest string table read (D24).
const MAX_STRINGS: u32 = 64 << 10;
const PFS0_ENTRY: u64 = 0x18;
const HFS0_ENTRY: u64 = 0x40;
/// The card header bytes read: up to the root partition's size, short of
/// the encrypted gamecard info at 0x190.
const CARD_HEAD: u64 = 0x140;
/// Where the card header sits in a dump that keeps the key area.
const KEY_AREA: u64 = 0x1000;
/// Root HFS0 partition names that may reach the detail (fixed set).
const XCI_PARTITIONS: [&str; 4] = ["update", "normal", "secure", "logo"];
/// The NRO header bytes read after the first 0x10: magic, version, size.
const NRO_HEADER: usize = 0x10;
/// The `ASET` header: magic, version, then icon, NACP and RomFS as
/// `offset u64, size u64` pairs, offsets from the section's start.
const ASET_HEADER: usize = 0x38;
/// The NACP bytes read: 16 language entries of 0x300 (name 0x200,
/// publisher 0x100), then up to the end of `DisplayVersion` (0x3060,
/// 0x10 bytes); switchbrew «NACP».
const NACP_READ: usize = 0x3070;
const NACP_LANGS: usize = 16;
const NACP_LANG: usize = 0x300;
const NACP_NAME: usize = 0x200;
const NACP_DISPLAY_VERSION: usize = 0x3060;
/// Largest NRO icon located (the embedded-art reader caps it again).
const MAX_NRO_ICON: u64 = 1 << 20;

fn le32(h: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(h.get(o..o + 4)?.try_into().ok()?))
}

fn le64(h: &[u8], o: usize) -> Option<u64> {
    Some(u64::from_le_bytes(h.get(o..o + 8)?.try_into().ok()?))
}

/// `[0100XXXXXXXXXXXX]` and `[vN]` tokens of a file name (spec D10): the
/// title id (16 hex starting `01`) and the version, when there is a title
/// id. Parsed by hand: `[`, 16 hex digits, `]`.
pub fn title_id_from_name(name: &str) -> Option<(u64, Option<u32>)> {
    let (tid, ver) = name_tokens(name);
    tid.map(|t| (t, ver))
}

/// Both tokens of a file name, each when present.
fn name_tokens(name: &str) -> (Option<u64>, Option<u32>) {
    let bytes = name.as_bytes();
    let mut tid = None;
    let mut ver = None;
    for (i, _) in name.match_indices('[') {
        let rest = &bytes[i + 1..];
        if rest.len() >= 17 && rest[16] == b']' && rest[..16].iter().all(u8::is_ascii_hexdigit) {
            // 16 ASCII hex digits: always UTF-8.
            let s = std::str::from_utf8(&rest[..16]).unwrap_or_default();
            if s.starts_with("01") {
                tid = u64::from_str_radix(s, 16).ok();
            }
        } else if rest.first() == Some(&b'v') {
            let digits: String = rest[1..]
                .iter()
                .take_while(|b| b.is_ascii_digit())
                .map(|&b| b as char)
                .collect();
            if !digits.is_empty() && rest.get(1 + digits.len()) == Some(&b']') {
                ver = digits.parse().ok();
            }
        }
    }
    (tid, ver)
}

/// One entry of a partition table: its name and its body's byte range.
struct Entry {
    name: String,
    start: u64,
    size: u64,
}

/// A PFS0/HFS0 table at `at`: `None` when the counts are over the caps or
/// the table does not fit in the file. Only the table is read; an entry
/// whose body falls outside the file keeps its name and is never read.
fn partition(b: &mut Budgeted, at: u64, magic: &[u8; 4], entry: u64) -> Option<Vec<Entry>> {
    let h = b.read(at, 0x10).ok()?;
    if h.get(..4)? != magic {
        return None;
    }
    let count = le32(&h, 4)?;
    let strings = le32(&h, 8)?;
    if count == 0 || count > MAX_ENTRIES || strings > MAX_STRINGS {
        return None;
    }
    let table = u64::from(count).checked_mul(entry)?;
    let size = table.checked_add(u64::from(strings))?;
    let data = at.checked_add(0x10)?.checked_add(size)?;
    if data > b.len() {
        return None;
    }
    let t = b.read(at + 0x10, usize::try_from(size).ok()?).ok()?;
    let (rows, names) = t.split_at(usize::try_from(table).ok()?);
    let mut out = Vec::with_capacity(count as usize);
    for row in rows.chunks_exact(usize::try_from(entry).ok()?) {
        let offset = le64(row, 0)?;
        let len = le64(row, 8)?;
        let name_at = usize::try_from(le32(row, 0x10)?).ok()?;
        let raw = names.get(name_at..)?;
        let end = raw.iter().position(|&c| c == 0).unwrap_or(raw.len());
        let name = String::from_utf8_lossy(&raw[..end]).into_owned();
        let start = data.checked_add(offset)?;
        out.push(Entry {
            name,
            start,
            size: len,
        });
    }
    Some(out)
}

fn ends_with_ci(name: &str, suffix: &str) -> bool {
    name.len() >= suffix.len()
        && name.is_char_boundary(name.len() - suffix.len())
        && name[name.len() - suffix.len()..].eq_ignore_ascii_case(suffix)
}

/// A ticket entry name `<32 hex>.tik`: the title id is the rights id's
/// first 16 hex digits. The ticket's bytes are never read (D4).
fn ticket_title_id(name: &str) -> Option<u64> {
    let stem = name.get(..name.len().checked_sub(4)?)?;
    if !ends_with_ci(name, ".tik")
        || stem.len() != 32
        || !stem.bytes().all(|b| b.is_ascii_hexdigit())
        || !stem.starts_with("01")
    {
        return None;
    }
    u64::from_str_radix(&stem[..16], 16).ok()
}

/// The lower-case NCA id of an entry or file named `<32 hex>.nca`,
/// `<32 hex>.ncz` or `<32 hex>.cnmt.nca` (a meta NCA): exactly 32 hex
/// digits before the extension (D6).
pub fn nca_id_from_name(name: &str) -> Option<String> {
    let stem = [".cnmt.nca", ".nca", ".ncz"]
        .iter()
        .find(|ext| ends_with_ci(name, ext))
        .map(|ext| &name[..name.len() - ext.len()])?;
    (stem.len() == 32 && stem.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| stem.to_ascii_lowercase())
}

/// Whether `c` is a Switch package whose content is always encrypted:
/// an NSP/NSZ (PFS0), an XCI/XCZ (card header, HFS0) or a loose NCA. Its
/// name is never read from inside it (spec D11); a homebrew NRO or a
/// game's NSO is in clear.
pub fn is_encrypted_package(c: &ConsoleId) -> bool {
    c.system == "switch"
        && (c.part == Some(PartKind::Nca)
            || ["PFS0", "HFS0", "card header"]
                .iter()
                .any(|d| c.detail.contains(d)))
}

/// The NCA ids among `names`, sorted and without repeats.
fn nca_ids<'a>(names: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut ids: Vec<String> = names.filter_map(nca_id_from_name).collect();
    ids.sort();
    ids.dedup();
    ids
}

/// `<Id>0x0100…</Id>`, `<Version>`, `<Type>` of a plaintext `cnmt.xml`.
fn parse_cnmt_xml(bytes: &[u8]) -> Option<(u64, Option<u32>, TitleKind)> {
    let f = xml_fields(bytes, &["Id", "Version", "Type"])?;
    let raw = get(&f, "Id")?;
    let hex = raw
        .strip_prefix("0x")
        .or_else(|| raw.strip_prefix("0X"))
        .unwrap_or(raw);
    if hex.len() != 16 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let tid = u64::from_str_radix(hex, 16).ok()?;
    let version = get(&f, "Version").and_then(|v| v.parse().ok());
    let kind = match get(&f, "Type") {
        Some("Application") => TitleKind::Base,
        Some("Patch") => TitleKind::Update,
        Some("AddOnContent") => TitleKind::Dlc,
        _ => TitleKind::Unknown,
    };
    Some((tid, version, kind))
}

/// Sets the title id, its source, the kind and the version.
fn set_title(id: &mut ConsoleId, tid: u64, source: IdSource, version: Option<u32>) {
    id.title_id = Some(format!("{tid:016X}"));
    id.id_source = Some(source);
    id.kind = kind_switch(tid);
    if let Some(v) = version {
        id.version = Some(v.to_string());
    }
}

/// The file-name title id, when nothing in clear named one.
fn from_name(id: &mut ConsoleId, name: &str) {
    if let Some((tid, ver)) = title_id_from_name(name) {
        set_title(id, tid, IdSource::FileName, ver);
    }
}

/// An NSP/NSZ by its PFS0 table (spec §8.3, D10).
fn pfs0(b: &mut Budgeted, name: &str) -> Option<ConsoleId> {
    let Some(entries) = partition(b, 0, b"PFS0", PFS0_ENTRY) else {
        let mut id = ConsoleId::magic("switch", "PFS0");
        from_name(&mut id, name);
        return Some(id);
    };
    let nsz = entries.iter().any(|e| ends_with_ci(&e.name, ".ncz"));
    let prefix = if nsz { "NSZ " } else { "" };
    let ticket = entries.iter().find_map(|e| ticket_title_id(&e.name));
    let cnmt = entries
        .iter()
        .find(|e| ends_with_ci(&e.name, ".cnmt.xml") && e.size <= MAX_META_FILE)
        .and_then(|e| {
            let fits = e
                .start
                .checked_add(e.size)
                .is_some_and(|end| end <= b.len());
            fits.then_some(())?;
            parse_cnmt_xml(&b.read(e.start, usize::try_from(e.size).ok()?).ok()?)
        });
    let (name_tid, name_ver) = name_tokens(name);
    let mut id;
    if let Some(tid) = ticket {
        id = ConsoleId::proven("switch", &format!("{prefix}PFS0 ticket name"));
        let version = cnmt
            .filter(|c| c.0 == tid)
            .and_then(|c| c.1)
            .or(name_ver.filter(|_| name_tid.is_none_or(|t| t == tid)));
        set_title(&mut id, tid, IdSource::TicketName, version);
    } else if let Some((tid, version, kind)) = cnmt {
        id = ConsoleId::proven("switch", &format!("{prefix}PFS0 cnmt.xml"));
        set_title(&mut id, tid, IdSource::CnmtXml, version);
        if id.kind == TitleKind::Unknown {
            id.kind = kind;
        }
    } else {
        id = ConsoleId::proven("switch", &format!("{prefix}PFS0"));
        from_name(&mut id, name);
        id.nca_ids = nca_ids(entries.iter().map(|e| e.name.as_str()));
    }
    Some(id)
}

/// An XCI/XCZ by its card header at `base` and its root HFS0 (spec
/// §8.3): the title id only from the file name; the secure partition's
/// NCA names as NCA ids (D6). No table is read inside the card header
/// (`base` .. `base + 0x200`), where the gamecard info is.
fn xci(b: &mut Budgeted, base: u64, card: &[u8], ext: &str, name: &str) -> Option<ConsoleId> {
    let label = if ext == "xcz" { "XCZ" } else { "XCI" };
    let past_card = base.checked_add(0x200)?;
    let root = le64(card, 0x30)
        .and_then(|o| base.checked_add(o))
        .filter(|&at| at >= past_card)
        .and_then(|at| partition(b, at, b"HFS0", HFS0_ENTRY));
    let secure = root.as_ref().and_then(|entries| {
        let secure = entries.iter().find(|e| e.name == "secure")?;
        // Only the secure HFS0's header and names table: never an NCA.
        let inner = partition(b, secure.start, b"HFS0", HFS0_ENTRY)?;
        Some(nca_ids(inner.iter().map(|e| e.name.as_str())))
    });
    let mut id = match root {
        Some(entries) => {
            let mut parts: Vec<&str> = XCI_PARTITIONS
                .iter()
                .copied()
                .filter(|p| entries.iter().any(|e| e.name == *p))
                .collect();
            parts.dedup();
            if parts.is_empty() {
                ConsoleId::proven("switch", &format!("{label} HFS0"))
            } else {
                ConsoleId::proven("switch", &format!("{label} HFS0: {}", parts.join(", ")))
            }
        }
        None => ConsoleId::magic("switch", &format!("{label} card header")),
    };
    from_name(&mut id, name);
    id.nca_ids = secure.unwrap_or_default();
    Some(id)
}

/// A loose NCA by its name alone (D6): part of a title, never read.
fn loose_nca(ext: &str, name: &str) -> Option<ConsoleId> {
    if !matches!(ext, "nca" | "ncz") {
        return None;
    }
    let nca = nca_id_from_name(name)?;
    let mut id = ConsoleId::magic("switch", "NCA name");
    id.part = Some(PartKind::Nca);
    id.nca_ids = vec![nca];
    Some(id)
}

/// The first non-empty NUL-terminated text of `bytes`, cleaned.
fn nacp_text(bytes: &[u8]) -> Option<String> {
    let end = bytes.iter().position(|&c| c == 0).unwrap_or(bytes.len());
    clean_title(&String::from_utf8_lossy(&bytes[..end]))
}

/// A homebrew NRO (switchbrew «NRO», «NACP»): `NRO0` at 0x10, the NRO's
/// size at 0x18, and at that offset an `ASET` section whose NACP gives the
/// title and `DisplayVersion` and whose icon is a JPEG. Without a usable
/// asset section it is a magic-only verdict, still homebrew.
fn nro(b: &mut Budgeted) -> Option<ConsoleId> {
    let h = b.read(0x10, NRO_HEADER).ok()?;
    if h.get(..4)? != b"NRO0" {
        return None;
    }
    let mut id = ConsoleId::magic("switch", "NRO header");
    id.kind = TitleKind::Homebrew;
    let Some(aset) = le32(&h, 8).and_then(|size| nro_assets(b, u64::from(size))) else {
        return Some(id);
    };
    let (title, version, icon) = aset;
    id.confidence = 0.95;
    id.detail = "NRO NACP".into();
    if title.is_some() {
        id.title_source = Some(TitleSource::Header);
    }
    id.title = title;
    id.version = version;
    id.art.extend(icon);
    Some(id)
}

/// The NACP title and version and the icon locator of the `ASET` section
/// at `at`; `None` when the section or its NACP does not fit in the file.
#[allow(clippy::type_complexity)]
fn nro_assets(
    b: &mut Budgeted,
    at: u64,
) -> Option<(Option<String>, Option<String>, Option<ArtLocator>)> {
    let end = at.checked_add(ASET_HEADER as u64)?;
    if at < 0x20 || end > b.len() {
        return None;
    }
    let h = b.read(at, ASET_HEADER).ok()?;
    if h.get(..4)? != b"ASET" {
        return None;
    }
    let (nacp_off, nacp_len) = (le64(&h, 0x18)?, le64(&h, 0x20)?);
    let nacp_at = at.checked_add(nacp_off)?;
    let fits = |start: u64, len: u64| start.checked_add(len).is_some_and(|e| e <= b.len());
    if nacp_len < NACP_READ as u64 || !fits(nacp_at, NACP_READ as u64) {
        return None;
    }
    let (icon_off, icon_len) = (le64(&h, 0x08)?, le64(&h, 0x10)?);
    let icon = at
        .checked_add(icon_off)
        .filter(|&o| icon_len > 0 && icon_len <= MAX_NRO_ICON && fits(o, icon_len))
        .map(|offset| ArtLocator::Range {
            offset,
            len: icon_len,
            format: ArtFormat::Jpeg,
        });
    let nacp = b.read(nacp_at, NACP_READ).ok()?;
    let title = nacp
        .as_chunks::<NACP_LANG>()
        .0
        .iter()
        .take(NACP_LANGS)
        .find_map(|lang| nacp_text(&lang[..NACP_NAME]));
    let version = nacp_text(nacp.get(NACP_DISPLAY_VERSION..NACP_READ)?);
    Some((title, version, icon))
}

/// A Switch file by its own header: `PFS0` at 0 (NSP, NSZ), `HEAD` at
/// 0x100 (XCI, XCZ; at 0x1100 with the key area, `.xci`/`.xcz` only),
/// `NRO0` at 0x10 and `NSO0` at 0 under their weak extensions. `name` is
/// the file name, for its title-id tokens.
///
/// The first read is 0x10 bytes, so a small PFS0 table is read before
/// anything of its first entry, and the card header is read only up to
/// 0x13F: the encrypted gamecard info at 0x190 is never touched.
pub fn identify(b: &mut Budgeted, ext: &str, name: &str) -> Option<ConsoleId> {
    if let Some(id) = loose_nca(ext, name) {
        return Some(id);
    }
    if b.len() < 0x10 {
        return None;
    }
    let h0 = b.read(0, 0x10).ok()?;
    if h0.starts_with(b"PFS0") {
        return pfs0(b, name);
    }
    if ext == "nso" && h0.starts_with(b"NSO0") {
        let mut id = ConsoleId::magic("switch", "NSO header");
        id.part = Some(PartKind::ExeFs);
        return Some(id);
    }
    if ext == "nro" && b.len() >= 0x20 {
        if let Some(id) = nro(b) {
            return Some(id);
        }
    }
    if b.len() >= CARD_HEAD {
        let card = b.read(0x100, (CARD_HEAD - 0x100) as usize).ok();
        if let Some(card) = card.filter(|c| c.starts_with(b"HEAD")) {
            return xci(b, 0, &card, ext, name);
        }
    }
    if matches!(ext, "xci" | "xcz") && b.len() >= KEY_AREA + CARD_HEAD {
        let card = b
            .read(KEY_AREA + 0x100, (CARD_HEAD - 0x100) as usize)
            .ok()?;
        if card.starts_with(b"HEAD") {
            return xci(b, KEY_AREA, &card, ext, name);
        }
    }
    None
}

/// A Switch dump recognised before anything else reads the file (D4):
/// the generic head read (128 KiB), the tail read and the head CRC would
/// reach a small NSP's ticket and NCA bodies, an XCI's encrypted gamecard
/// info and the key area some XCI dumps keep. Only this reader's own
/// reads happen. `Some` with the bytes read when the file is a dump under
/// a Switch extension, or a table that checks out (0.95) under any other;
/// anything weaker is left to the usual order of readers.
pub fn identify_first(
    src: &mut dyn ReadAt,
    len: u64,
    ext: &str,
    name: &str,
) -> Option<(ConsoleId, u64)> {
    if let Some(id) = loose_nca(ext, name) {
        return Some((id, 0));
    }
    let mut b = Budgeted::new(src, len);
    let id = identify(&mut b, ext, name)?;
    let own = matches!(ext, "xci" | "xcz" | "nsp" | "nsz");
    (own || id.confidence >= 0.95).then(|| (id, b.used().read))
}

/// A split «archive bit» file (spec D12): a folder named `*.nsp`/`*.xci`
/// (or `.nsz`/`.xcz`) holding `00`, `01`… The `00` part's head is read
/// through [`identify`] with the folder's name; nothing else is opened.
pub fn probe_split(l: &DirListing) -> Option<FolderMatch> {
    let folder = l.dir.file_name()?.to_string_lossy().into_owned();
    let (_, ext) = folder.rsplit_once('.')?;
    let ext = ext.to_ascii_lowercase();
    if !matches!(ext.as_str(), "nsp" | "nsz" | "xci" | "xcz") {
        return None;
    }
    let first = find(l.names, "00")?;
    find(l.names, "01")?;
    let path = l.dir.join(first);
    let len = plain_file(&path, u64::MAX)?;
    let mut src = FileAt(super::open_rom(&path).ok()?);
    let mut b = Budgeted::new(&mut src, len);
    let mut id = identify(&mut b, &ext, &folder)?;
    id.detail = if ext.starts_with("ns") {
        "split NSP folder".into()
    } else {
        "split XCI folder".into()
    };
    Some(FolderMatch {
        id,
        probe: PathBuf::from(first),
        leave: None,
    })
}

#[cfg(test)]
mod tests {
    use std::ops::Range;

    use super::super::testing::{self, RangeLog, SparseAt};
    use super::*;

    fn overlaps(a: &Range<u64>, b: &Range<u64>) -> bool {
        a.start < b.end && b.start < a.end
    }

    fn run(f: &mut SparseAt, ext: &str, name: &str) -> Option<ConsoleId> {
        let len = f.len;
        super::super::identify(&mut Budgeted::new(f, len), ext, name)
    }

    const CNMT: &[u8] = b"<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<ContentMeta>\n  <Type>Patch</Type>\n  <Id>0x0100000000010800</Id>\n  <Version>393216</Version>\n</ContentMeta>\n";

    #[test]
    fn xci_is_switch_without_title_id() {
        let mut f = testing::xci(&["update", "normal", "secure"]);
        let len = f.len;
        let secure = f.entry_range(2);
        let mut log = RangeLog::new(&mut f);
        let id =
            super::super::identify(&mut Budgeted::new(&mut log, len), "xci", "Mario.xci").unwrap();
        assert_eq!(id.system, "switch");
        assert_eq!(id.confidence, 0.95);
        assert_eq!(id.detail, "XCI HFS0: update, normal, secure");
        assert_eq!((id.title_id, id.kind), (None, TitleKind::Unknown));
        let (info, n) = testing::XCI_CARD_INFO;
        assert!(!log.touched(info, n), "{:?}", log.ranges);
        // Of the secure partition only the place of its HFS0 header is
        // looked at (D6); this filler is no HFS0, so nothing more is read.
        assert!(!log.touched(secure.start + 0x10, secure.end - secure.start - 0x10));
        // The title id, when there is one, comes from the file name only.
        let mut f = testing::xci(&["secure"]);
        let id = run(&mut f, "xcz", "Zelda [0100000000010000][v0].xcz").unwrap();
        assert_eq!(id.title_id.as_deref(), Some("0100000000010000"));
        assert_eq!(id.id_source, Some(IdSource::FileName));
        assert!(id.detail.starts_with("XCZ"), "{}", id.detail);
    }

    #[test]
    fn xci_with_a_bad_root_is_magic_only() {
        let mut f = testing::xci(&["secure"]);
        f.put(0x130, &u64::MAX.to_le_bytes());
        let id = run(&mut f, "xci", "x.xci").unwrap();
        assert_eq!(
            (id.confidence, id.detail.as_str()),
            (0.90, "XCI card header")
        );
    }

    #[test]
    fn xci_with_the_key_area_is_read_from_0x1000() {
        let inner = testing::xci(&["normal", "secure"]);
        let mut f = SparseAt::new(inner.len + 0x1000);
        for (o, bytes) in &inner.parts {
            f.put(o + 0x1000, bytes);
        }
        let id = run(&mut f, "xci", "x.xci").unwrap();
        assert_eq!(id.detail, "XCI HFS0: normal, secure");
        // Under another extension the far offset is not looked at.
        assert_eq!(run(&mut f, "bin", "x.bin"), None);
    }

    #[test]
    fn nsp_ticket_name_gives_title_id() {
        let tik = "01000000000108000000000000000000.tik";
        let mut f = testing::nsp(&[
            ("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.nca", &[0x55; 64]),
            (tik, &[0x66; 0x2C0]),
        ]);
        let id = run(&mut f, "nsp", "Mario Update [v393216].nsp").unwrap();
        assert_eq!(id.title_id.as_deref(), Some("0100000000010800"));
        assert_eq!(id.id_source, Some(IdSource::TicketName));
        assert_eq!(id.kind, TitleKind::Update);
        assert_eq!(id.version.as_deref(), Some("393216"));
        assert_eq!(
            (id.confidence, id.detail.as_str()),
            (0.95, "PFS0 ticket name")
        );
    }

    #[test]
    fn nsp_cnmt_xml_gives_id_and_version() {
        let mut f = testing::nsp(&[
            ("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb.nca", &[0x55; 64]),
            ("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb.cnmt.xml", CNMT),
        ]);
        let id = run(&mut f, "nsp", "x.nsp").unwrap();
        assert_eq!(id.title_id.as_deref(), Some("0100000000010800"));
        assert_eq!(id.id_source, Some(IdSource::CnmtXml));
        assert_eq!(id.version.as_deref(), Some("393216"));
        assert_eq!(id.kind, TitleKind::Update);
        assert_eq!(id.detail, "PFS0 cnmt.xml");
        // A DTD is refused: the id falls back to the file name.
        let bad = b"<!DOCTYPE x [<!ENTITY a \"b\">]><ContentMeta><Id>0x0100000000010800</Id></ContentMeta>";
        let mut f = testing::nsp(&[("c.cnmt.xml", bad)]);
        let id = run(&mut f, "nsp", "Game [0100000000020000].nsp").unwrap();
        assert_eq!(id.title_id.as_deref(), Some("0100000000020000"));
        assert_eq!(id.id_source, Some(IdSource::FileName));
    }

    #[test]
    fn nsz_entries_ncz() {
        let tik = "01000000000100000000000000000000.tik";
        let mut f = testing::nsz(&[
            ("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.ncz", &[0x55; 64]),
            (tik, &[0x66; 0x2C0]),
        ]);
        let id = run(&mut f, "nsz", "Mario.nsz").unwrap();
        assert_eq!(id.detail, "NSZ PFS0 ticket name");
        assert_eq!(id.kind, TitleKind::Base);
    }

    #[test]
    fn plain_nsp_takes_the_file_name() {
        let mut f = testing::nsp(&[("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.nca", &[0x55; 64])]);
        let id = run(&mut f, "nsp", "Mario [0100000000011001][v65536].nsp").unwrap();
        assert_eq!(id.detail, "PFS0");
        assert_eq!(id.title_id.as_deref(), Some("0100000000011001"));
        assert_eq!(id.kind, TitleKind::Dlc);
        assert_eq!(id.version.as_deref(), Some("65536"));
        let id = run(&mut f, "nsp", "Mario.nsp").unwrap();
        assert_eq!((id.title_id, id.id_source), (None, None));
    }

    #[test]
    fn file_name_tokens() {
        assert_eq!(
            title_id_from_name("Mario [0100000000010000][v65536].nsp"),
            Some((0x0100_0000_0001_0000, Some(65536)))
        );
        assert_eq!(title_id_from_name("Mario (USA).nsp"), None);
        assert_eq!(title_id_from_name("Mario [0005000010145D00].nsp"), None);
        assert_eq!(title_id_from_name("Mario [v].nsp"), None);
        assert_eq!(
            title_id_from_name("Mario [v2] [0100000000010000].nsp"),
            Some((0x0100_0000_0001_0000, Some(2)))
        );
        assert_eq!(
            title_id_from_name("[0100000000010000][v]"),
            Some((0x0100_0000_0001_0000, None))
        );
        // Multi-byte text around the tokens never panics.
        assert_eq!(title_id_from_name("ñ[ü[0100"), None);
    }

    #[test]
    fn pfs0_count_over_cap_is_magic_only() {
        let mut f = testing::nsp(&[("a.nca", &[0x55; 64])]);
        f.put(4, &0x8000_0000u32.to_le_bytes());
        let len = f.len;
        let mut b = Budgeted::new(&mut f, len);
        let id = super::super::identify(&mut b, "nsp", "Game [0100000000010000].nsp").unwrap();
        assert_eq!((id.confidence, id.detail.as_str()), (0.90, "PFS0"));
        assert_eq!(id.title_id.as_deref(), Some("0100000000010000"));
        assert!(b.used().read <= 0x20, "{:?}", b.used());
        // A string table past the file, and one past the cap.
        for strings in [0x1000u32, MAX_STRINGS + 1] {
            let mut f = testing::nsp(&[("a.nca", &[0x55; 64])]);
            f.put(8, &strings.to_le_bytes());
            assert_eq!(run(&mut f, "nsp", "x.nsp").unwrap().confidence, 0.90);
        }
    }

    #[test]
    fn nsp_ticket_and_nca_bytes_are_never_read() {
        let tik = "01000000000100000000000000000000.tik";
        let mut f = testing::nsp(&[
            ("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.nca", &[0x55; 4096]),
            (tik, &[0x66; 0x2C0]),
            (
                "x.cnmt.xml",
                b"<ContentMeta><Id>0x0100000000010000</Id><Version>0</Version></ContentMeta>",
            ),
        ]);
        let len = f.len;
        let (nca, tik_range) = (f.entry_range(0), f.entry_range(1));
        let mut log = testing::RangeLog {
            inner: &mut f,
            ranges: Vec::new(),
        };
        let id =
            super::super::identify(&mut Budgeted::new(&mut log, len), "nsp", "Mario.nsp").unwrap();
        assert_eq!(id.title_id.as_deref(), Some("0100000000010000"));
        for (o, n) in &log.ranges {
            let r = *o..*o + *n as u64;
            assert!(
                !overlaps(&r, &nca) && !overlaps(&r, &tik_range),
                "read {r:?}"
            );
        }
    }

    /// D4 through the real entry point: the sniff's own head read must
    /// not reach a small NSP's ticket and NCA bodies, an XCI's encrypted
    /// gamecard info or the key area of an XCI that keeps it.
    #[test]
    fn the_sniff_never_reads_tickets_ncas_or_the_gamecard_info() {
        use crate::identify::sniff::sniff_named;
        let tik = "01000000000100000000000000000000.tik";
        let mut f = testing::nsp(&[
            ("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.nca", &[0x55; 4096]),
            (tik, &[0x66; 0x2C0]),
        ]);
        let len = f.len;
        let (nca, tik_range) = (f.entry_range(0), f.entry_range(1));
        for ext in ["nsp", "bin"] {
            let mut log = RangeLog::new(&mut f);
            let s = sniff_named(&mut log, len, ext, "Mario.nsp");
            let id = s.console.expect("switch");
            assert_eq!(id.title_id.as_deref(), Some("0100000000010000"));
            assert!(s.read > 0);
            for &(o, n) in &log.ranges {
                let r = o..o + n as u64;
                assert!(
                    !overlaps(&r, &nca) && !overlaps(&r, &tik_range),
                    "{ext}: read {r:?}"
                );
            }
        }
        let (info, n) = testing::XCI_CARD_INFO;
        let mut f = testing::xci(&["update", "normal", "secure"]);
        let len = f.len;
        let mut log = RangeLog::new(&mut f);
        let s = sniff_named(&mut log, len, "xci", "Mario.xci");
        assert_eq!(s.console.map(|c| c.system).as_deref(), Some("switch"));
        assert!(!log.touched(info, n), "{:?}", log.ranges);
        // With the key area kept, its key fields (the initial data's auth
        // data, MAC and nonce at 0x10..0x3C, the title key area from
        // 0x200) are not read; only the package id and the reserved bytes
        // where an ordinary dump has `HEAD` are.
        let inner = testing::xci(&["secure"]);
        let mut f = SparseAt::new(inner.len + KEY_AREA);
        for (o, bytes) in &inner.parts {
            f.put(o + KEY_AREA, bytes);
        }
        let len = f.len;
        let mut log = RangeLog::new(&mut f);
        let s = sniff_named(&mut log, len, "xci", "Mario.xci");
        assert_eq!(s.console.map(|c| c.system).as_deref(), Some("switch"));
        assert!(!log.touched(0x10, 0xF0), "{:?}", log.ranges);
        assert!(!log.touched(0x200, KEY_AREA - 0x200), "{:?}", log.ranges);
        assert!(!log.touched(KEY_AREA + info, n), "{:?}", log.ranges);
    }

    #[test]
    fn weak_homebrew_formats_need_their_extension() {
        let mut nro = SparseAt::new(0x1000);
        nro.put(0x10, b"NRO0");
        assert_eq!(run(&mut nro, "nro", "a.nro").unwrap().detail, "NRO header");
        assert_eq!(run(&mut nro, "bin", "a.bin"), None);
        let mut nso = SparseAt::new(0x1000);
        nso.put(0, b"NSO0");
        assert_eq!(
            run(&mut nso, "nso", "main.nso").unwrap().detail,
            "NSO header"
        );
        assert_eq!(run(&mut nso, "bin", "a.bin"), None);
    }

    const NCA_A: &str = "0123456789abcdef0123456789abcdef";
    const NCA_B: &str = "FEDCBA9876543210FEDCBA9876543210";

    #[test]
    fn xci_secure_names_give_nca_ids_without_reading_any_nca() {
        let a = format!("{NCA_A}.nca");
        let b = format!("{NCA_B}.cnmt.nca");
        let mut f = testing::xci_with_secure(&[&b, &a, "short.nca", "logo.bin"]);
        let len = f.len;
        let (info, n) = testing::XCI_CARD_INFO;
        let mut forbidden: Vec<Range<u64>> = Vec::new();
        forbidden.push(info..info + n);
        forbidden.extend(f.entries[3..].iter().cloned());
        let mut guard = testing::PanicOn {
            inner: &mut f,
            ranges: forbidden,
        };
        let id =
            super::super::identify(&mut Budgeted::new(&mut guard, len), "xci", "Game.xci").unwrap();
        assert_eq!(id.detail, "XCI HFS0: update, normal, secure");
        assert_eq!(id.title_id, None);
        assert_eq!(
            id.nca_ids,
            vec![NCA_A.to_string(), NCA_B.to_ascii_lowercase()]
        );
    }

    #[test]
    fn xci_secure_partition_past_the_file_keeps_the_root_verdict() {
        let mut f = testing::xci_with_secure(&[&format!("{NCA_A}.nca")]);
        let secure = f.entry_range(2);
        f.len = secure.start + 8;
        let id = run(&mut f, "xci", "Game.xci").unwrap();
        assert_eq!(id.confidence, 0.95);
        assert!(id.nca_ids.is_empty());
    }

    #[test]
    fn nsp_without_ticket_or_cnmt_lists_its_nca_names() {
        let a = format!("{NCA_A}.nca");
        let b = format!("{NCA_B}.ncz");
        let mut f = testing::nsp_pfs0(&[&b, &a, "x.nca", "0123.nca"]);
        let len = f.len;
        let forbidden = f.entries.clone();
        let mut guard = testing::PanicOn {
            inner: &mut f,
            ranges: forbidden,
        };
        let id =
            super::super::identify(&mut Budgeted::new(&mut guard, len), "nsz", "Game.nsz").unwrap();
        assert_eq!(id.title_id, None);
        assert_eq!(
            id.nca_ids,
            vec![NCA_A.to_string(), NCA_B.to_ascii_lowercase()]
        );
        // A ticket names the title: no NCA ids are kept.
        let tik = "01000000000100000000000000000000.tik";
        let mut f = testing::nsp_pfs0(&[&a, tik]);
        let id = run(&mut f, "nsp", "Game.nsp").unwrap();
        assert_eq!(id.title_id.as_deref(), Some("0100000000010000"));
        assert!(id.nca_ids.is_empty());
    }

    #[test]
    fn a_loose_nca_is_known_by_its_name_alone() {
        let name = format!("{NCA_A}.nca");
        let mut f = SparseAt::new(0x4000);
        f.put(0x200, b"NCA3");
        let len = f.len;
        let mut guard = testing::PanicOn {
            inner: &mut f,
            ranges: std::iter::once(0..len).collect(),
        };
        let id = super::super::identify(&mut Budgeted::new(&mut guard, len), "nca", &name).unwrap();
        assert_eq!(id.system, "switch");
        assert_eq!(id.part, Some(PartKind::Nca));
        assert_eq!(
            (id.title_id, id.title, id.kind),
            (None, None, TitleKind::Unknown)
        );
        assert_eq!(id.nca_ids, vec![NCA_A.to_string()]);
        let (found, read) =
            identify_first(&mut guard, len, "ncz", &format!("{NCA_B}.ncz")).unwrap();
        assert_eq!((found.part, read), (Some(PartKind::Nca), 0));
        assert_eq!(found.nca_ids, vec![NCA_B.to_ascii_lowercase()]);
        // A meta NCA is named `<id>.cnmt.nca`.
        let meta = format!("{NCA_A}.cnmt.nca");
        assert_eq!(identify_first(&mut guard, len, "nca", &meta).unwrap().1, 0);
    }

    #[test]
    fn nro_nacp_gives_homebrew_title_version_and_icon() {
        let icon = [0xFF, 0xD8, 0xFF, 0xE0, 0, 0, 0, 0];
        let mut f = testing::nro("Checkpoint", "3.8.0", &icon);
        let id = run(&mut f, "nro", "checkpoint.nro").unwrap();
        assert_eq!(id.system, "switch");
        assert_eq!((id.confidence, id.detail.as_str()), (0.95, "NRO NACP"));
        assert_eq!(id.kind, TitleKind::Homebrew);
        assert_eq!(id.title.as_deref(), Some("Checkpoint"));
        assert_eq!(id.title_source, Some(TitleSource::Header));
        assert_eq!(id.version.as_deref(), Some("3.8.0"));
        assert_eq!(
            id.art,
            vec![ArtLocator::Range {
                offset: testing::NRO_SIZE + 0x38,
                len: icon.len() as u64,
                format: ArtFormat::Jpeg,
            }]
        );
        // No icon: no locator.
        let mut f = testing::nro("Checkpoint", "3.8.0", &[]);
        assert!(run(&mut f, "nro", "a.nro").unwrap().art.is_empty());
        // An asset section past the file: magic only, still homebrew.
        let mut f = testing::nro("Checkpoint", "3.8.0", &[]);
        f.len = testing::NRO_SIZE + 0x10;
        let id = run(&mut f, "nro", "a.nro").unwrap();
        assert_eq!((id.confidence, id.detail.as_str()), (0.90, "NRO header"));
        assert_eq!((id.kind, id.title), (TitleKind::Homebrew, None));
    }

    #[test]
    fn nso_is_a_part_of_a_game() {
        let mut f = testing::nso();
        let id = run(&mut f, "nso", "main.nso").unwrap();
        assert_eq!(id.part, Some(PartKind::ExeFs));
        assert_eq!(id.kind, TitleKind::Unknown);
    }

    #[test]
    fn budget_adversarial_hfs0() {
        let mut f = testing::xci(&["secure"]);
        f.put(testing::XCI_HFS0 + 4, &0x8000_0000u32.to_le_bytes());
        let len = f.len;
        let mut b = Budgeted::new(&mut f, len);
        let id = super::super::identify(&mut b, "xci", "x.xci").unwrap();
        assert_eq!(id.confidence, 0.90);
        let used = b.used();
        assert!(used.read < 0x1000 && used.seeks <= 4, "{used:?}");
    }

    mod split {
        use std::fs;

        use super::*;

        #[test]
        fn split_folder_probe() {
            let tmp = tempfile::TempDir::new().unwrap();
            let dir = tmp.path().join("Mario [0100000000010000][v0].nsp");
            fs::create_dir(&dir).unwrap();
            let f = testing::nsp(&[("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.nca", &[0x55; 64])]);
            fs::write(dir.join("00"), f.bytes()).unwrap();
            fs::write(dir.join("01"), [0u8; 16]).unwrap();
            let names = vec!["00".to_string(), "01".to_string()];
            let found = probe_split(&DirListing {
                dir: &dir,
                names: &names,
            })
            .unwrap();
            assert_eq!(found.id.detail, "split NSP folder");
            assert_eq!(found.id.title_id.as_deref(), Some("0100000000010000"));
            assert_eq!(found.probe, PathBuf::from("00"));
            // Not a split file: one part only, or another folder name.
            let one = vec!["00".to_string()];
            assert!(probe_split(&DirListing {
                dir: &dir,
                names: &one
            })
            .is_none());
            let other = tmp.path().join("Mario");
            fs::create_dir(&other).unwrap();
            assert!(probe_split(&DirListing {
                dir: &other,
                names: &names
            })
            .is_none());
        }
    }
}
