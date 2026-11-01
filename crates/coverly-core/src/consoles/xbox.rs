//! Xbox and Xbox 360 (spec §8.5): XDVDFS images (XISO and the Redump
//! XGD1/XGD2/XGD3 layouts), XBE and XEX2 executables, STFS packages
//! (`CON `, `LIVE`, `PIRS`: Games on Demand, Xbox Live Arcade, title
//! updates, DLC), their folders, Xenia's ZAR archives and the
//! `.xbox360` launcher files.
//!
//! Sources, checked for this reader:
//! - xboxdevwiki «XDVDFS»: the volume descriptor `MICROSOFT*XBOX*MEDIA`
//!   at sector 32 of the game partition, the root table sector (u32 LE)
//!   at +0x14 and its size at +0x18; a table entry is `left u16`, `right
//!   u16` (offsets in 4-byte units, 0 = none), `sector u32`, `size u32`,
//!   `attributes u8` (0x10 = directory), `name length u8`, the name;
//!   unused bytes are 0xFF. The partition offsets of the Redump layouts
//!   are those of extract-xiso (`0x18300000` XGD1, `0x0FD90000` XGD2,
//!   `0x02080000` XGD3).
//! - xboxdevwiki «XBE»: `XBEH`, base address at 0x104, certificate
//!   address at 0x118; certificate title id at +0x8, title name (40
//!   UTF-16LE units) at +0xC, version at +0xAC.
//! - xenia `src/xenia/kernel/util/xex2_info.h`: header count at 0x14,
//!   `(key, value)` pairs from 0x18, `XEX_HEADER_EXECUTION_INFO`
//!   `0x00040006`; execution info `media_id`, `version`, `base_version`,
//!   `title_id` (+0xC), `platform`, `executable_table`, `disc_number`
//!   (+0x12), `disc_count` (+0x13). The plan's +0x16/+0x17 for the disc
//!   fields did not check out; xenia's offsets are used.
//! - free60 «STFS» (metadata v1): content type at 0x344, title id at
//!   0x360, display name (UTF-16BE, 0x80 bytes for the first locale) at
//!   0x411, title name at 0x1691, thumbnail size at 0x1712 and the
//!   thumbnail (at most 0x4000 bytes) at 0x171A. Content types:
//!   `0x00007000` Games on Demand, `0x000D0000` Arcade title,
//!   `0x000B0000` installer (title update), `0x00000002` marketplace
//!   content (DLC).
//!
//! Only plaintext is read: the volume descriptor, the root table, the
//! XBE header and certificate, the XEX header and its execution info,
//! the STFS header block. An XEX's PE image (encrypted or compressed)
//! and an STFS package's data blocks are never read.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::wiiu::{find, list_child, plain_file};
use super::zarchive::{self, Archive};
use super::{
    ArtFormat, ArtLocator, Budgeted, ConsoleId, DirListing, FolderMatch, IdSource, TitleKind,
    MAX_META_FILE,
};
use crate::identify::sniff::FileAt;

const XDVDFS_MAGIC: &[u8; 20] = b"MICROSOFT*XBOX*MEDIA";
/// XISO, Redump XGD1, XGD2, XGD3 partition offsets (four reads at most).
pub const PARTITIONS: [u64; 4] = [0, 0x1830_0000, 0x0FD9_0000, 0x0208_0000];
const SECTOR: u64 = 2048;
/// The volume descriptor is at sector 32 of the game partition.
const VOLUME_AT: u64 = 32 * SECTOR;
const VOLUME_LEN: usize = 0x20;
/// Largest root table read (D24).
const MAX_ROOT: u64 = 64 << 10;
/// Most root-table entries visited.
pub const MAX_ENTRIES: usize = 2048;
const ENTRY_HEAD: usize = 14;
const ATTR_DIRECTORY: u8 = 0x10;
const XBE_HEADER: usize = 0x178;
const XBE_CERT_LEN: usize = 0xB0;
/// The certificate must start within the first 64 KiB of the XBE.
const MAX_CERT: u64 = 0x1_0000;
const XEX_HEAD: usize = 0x18;
const XEX_EXEC_INFO: u32 = 0x0004_0006;
const MAX_XEX_HEADERS: u32 = 64;
const XEX_EXEC_LEN: usize = 0x18;
/// The STFS header block read whole: magic to the thumbnail size.
const STFS_HEAD: usize = 0x171A;
const STFS_THUMB: u64 = 0x171A;
const STFS_THUMB_MAX: u64 = 0x4000;
/// Content types (free60 «STFS»).
const CT_GOD: u32 = 0x0000_7000;
const CT_XBLA: u32 = 0x000D_0000;
const CT_UPDATE: u32 = 0x000B_0000;
const CT_DLC: u32 = 0x0000_0002;
/// The bytes of a ZAR's `default.xex` decoded for its header: one block.
const ZAR_XEX_HEAD: u64 = zarchive::BLOCK;
/// Longest launcher file whose text is read as a title id (D14).
const LAUNCHER_MAX: usize = 64;

fn le16(h: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(h.get(o..o + 2)?.try_into().ok()?))
}

fn le32(h: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(h.get(o..o + 4)?.try_into().ok()?))
}

fn be32(h: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_be_bytes(h.get(o..o + 4)?.try_into().ok()?))
}

fn is_hex(name: &str, n: usize) -> bool {
    name.len() == n && name.bytes().all(|b| b.is_ascii_hexdigit())
}

/// UTF-16 text up to its first NUL, cleaned.
fn utf16_title(units: impl Iterator<Item = u16>) -> Option<String> {
    let units: Vec<u16> = units.take_while(|&u| u != 0).collect();
    super::clean_title(&String::from_utf16_lossy(&units))
}

/// Reads `n` bytes at an offset of the executable or package being read.
pub type BytesAt<'a> = dyn FnMut(u64, usize) -> Option<Vec<u8>> + 'a;

/// The XBE certificate serial: publisher letters, a dash, three digits
/// (`0x4541000D` → `EA-013`, Redump's form).
pub fn xbe_serial(title_id: u32) -> Option<String> {
    let (a, b) = ((title_id >> 24) as u8, (title_id >> 16) as u8);
    (a.is_ascii_alphanumeric() && b.is_ascii_alphanumeric())
        .then(|| format!("{}{}-{:03}", a as char, b as char, title_id & 0xFFFF))
        .and_then(|s| crate::disc::serial::normalise_console_serial("xbox", &s))
}

/// An XBE: the header, then the certificate. A certificate outside the
/// first 64 KiB (or unreadable) leaves the magic-only verdict.
pub fn xbe(bytes_at: &mut BytesAt) -> Option<ConsoleId> {
    let h = bytes_at(0, XBE_HEADER)?;
    if &h[..4] != b"XBEH" {
        return None;
    }
    let magic = ConsoleId::magic("xbox", "XBE header");
    let base = le32(&h, 0x104)?;
    let addr = le32(&h, 0x118)?;
    let Some(cert) = addr
        .checked_sub(base)
        .map(u64::from)
        .filter(|&c| c < MAX_CERT)
    else {
        return Some(magic);
    };
    let Some(c) = bytes_at(cert, XBE_CERT_LEN) else {
        return Some(magic);
    };
    let tid = le32(&c, 8)?;
    let mut id = ConsoleId::proven("xbox", "XBE certificate");
    if tid != 0 {
        id.serial = xbe_serial(tid);
        id.title_id = Some(format!("{tid:08X}"));
    }
    id.title = utf16_title(
        c[0xC..0x5C]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|p| u16::from_le_bytes(*p)),
    );
    id.version = le32(&c, 0xAC).map(|v| v.to_string());
    id.id_source = Some(IdSource::Header);
    Some(id)
}

/// An XEX: `XEX2` with its execution info (title id, version, disc);
/// the older `XEX1`, `XEX%` and `XEX-` heads name the system only.
pub fn xex2(bytes_at: &mut BytesAt) -> Option<ConsoleId> {
    let h = bytes_at(0, XEX_HEAD)?;
    let magic = ConsoleId::magic("xbox360", "XEX header");
    match &h[..4] {
        b"XEX2" => {}
        b"XEX1" | b"XEX%" | b"XEX-" => return Some(magic),
        _ => return None,
    }
    let count = be32(&h, 0x14)?;
    if count == 0 || count > MAX_XEX_HEADERS {
        return Some(magic);
    }
    let Some(headers) = bytes_at(XEX_HEAD as u64, count as usize * 8) else {
        return Some(magic);
    };
    let Some(at) = headers
        .as_chunks::<8>()
        .0
        .iter()
        .find(|kv| be32(&kv[..], 0) == Some(XEX_EXEC_INFO))
        .and_then(|kv| be32(&kv[..], 4))
    else {
        return Some(magic);
    };
    let Some(e) = bytes_at(u64::from(at), XEX_EXEC_LEN) else {
        return Some(magic);
    };
    let tid = be32(&e, 0xC)?;
    if tid == 0 {
        return Some(magic);
    }
    let mut id = ConsoleId::proven("xbox360", "XEX2 execution info");
    let hex = format!("{tid:08X}");
    id.serial = crate::disc::serial::normalise_console_serial("xbox360", &hex);
    id.title_id = Some(hex);
    id.version = be32(&e, 4).map(|v| format!("{v:08X}"));
    let (disc, discs) = (e[0x12], e[0x13]);
    id.disc_no = (disc > 0 && discs > 1).then_some(disc);
    id.kind = TitleKind::Base;
    id.id_source = Some(IdSource::Header);
    Some(id)
}

/// The kind and the fixed detail of an STFS content type.
fn content_kind(ctype: u32) -> (TitleKind, &'static str) {
    match ctype {
        CT_GOD => (TitleKind::Base, "GOD package"),
        CT_XBLA => (TitleKind::Base, "XBLA package"),
        CT_UPDATE => (TitleKind::Update, "title update package"),
        CT_DLC => (TitleKind::Dlc, "DLC package"),
        _ => (TitleKind::Unknown, "STFS package"),
    }
}

/// The No-Intro Xbox 360 (Digital) rom-name prefix of an STFS item:
/// `<titleid8><contenttype8>` (spec §8.5), from its title id and the
/// content type its detail names. `None` for anything else.
pub fn dat_name_prefix(id: &ConsoleId) -> Option<String> {
    if id.system != "xbox360" {
        return None;
    }
    let tid = id.title_id.as_deref().filter(|t| is_hex(t, 8))?;
    let ctype = [CT_GOD, CT_XBLA, CT_UPDATE, CT_DLC]
        .into_iter()
        .find(|&c| content_kind(c).1 == id.detail)?;
    Some(format!("{}{ctype:08X}", tid.to_ascii_uppercase()))
}

/// An STFS package header: content type (kind), title id, display name
/// (else title name) and the thumbnail's byte range. A file too short
/// for the header block is the magic alone.
pub fn stfs(b: &mut Budgeted) -> Option<ConsoleId> {
    let magic = b.read(0, 4).ok()?;
    if !matches!(&magic[..], b"CON " | b"LIVE" | b"PIRS") {
        return None;
    }
    if b.len() < STFS_HEAD as u64 {
        return Some(ConsoleId::magic("xbox360", "STFS header"));
    }
    let Ok(h) = b.read(0, STFS_HEAD) else {
        return Some(ConsoleId::magic("xbox360", "STFS header"));
    };
    let ctype = be32(&h, 0x344)?;
    let tid = be32(&h, 0x360)?;
    let (kind, detail) = content_kind(ctype);
    let mut id = ConsoleId::proven("xbox360", detail);
    id.kind = kind;
    if tid != 0 {
        let hex = format!("{tid:08X}");
        id.serial = crate::disc::serial::normalise_console_serial("xbox360", &hex);
        id.title_id = Some(hex);
        id.id_source = Some(IdSource::Header);
    }
    let text = |at: usize| {
        utf16_title(
            h[at..at + 0x80]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|p| u16::from_be_bytes(*p)),
        )
    };
    id.title = text(0x411).or_else(|| text(0x1691));
    let thumb = u64::from(be32(&h, 0x1712)?);
    if thumb > 0
        && thumb <= STFS_THUMB_MAX
        && STFS_THUMB.checked_add(thumb).is_some_and(|e| e <= b.len())
    {
        id.art.push(ArtLocator::Range {
            offset: STFS_THUMB,
            len: thumb,
            format: ArtFormat::Png,
        });
    }
    Some(id)
}

/// Which executable an XDVDFS root names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Exe {
    Xbe,
    Xex,
}

/// `default.xex` (else `default.xbe`) in a root table: `(which, start
/// sector, size)`. The tree is walked iteratively from offset 0, each
/// entry once, at most [`MAX_ENTRIES`] entries. `default.xex` wins: it
/// is only on Xbox 360 discs, which may carry a `default.xbe` too.
fn root_exe(table: &[u8]) -> Option<(Exe, u32, u32)> {
    let mut stack = vec![0usize];
    let mut seen: HashSet<usize> = HashSet::new();
    let mut xbe = None;
    while let Some(o) = stack.pop() {
        if seen.len() >= MAX_ENTRIES {
            break;
        }
        if !seen.insert(o) {
            continue;
        }
        let Some(e) = table.get(o..o + ENTRY_HEAD) else {
            continue;
        };
        let (left, right) = (le16(e, 0)?, le16(e, 2)?);
        if left == 0xFFFF && right == 0xFFFF {
            continue;
        }
        let n = usize::from(e[13]);
        let Some(name) = table.get(o + ENTRY_HEAD..o + ENTRY_HEAD + n) else {
            continue;
        };
        if e[12] & ATTR_DIRECTORY == 0 {
            let at = (le32(e, 4)?, le32(e, 8)?);
            if name.eq_ignore_ascii_case(b"default.xex") {
                return Some((Exe::Xex, at.0, at.1));
            }
            if name.eq_ignore_ascii_case(b"default.xbe") {
                xbe = Some((Exe::Xbe, at.0, at.1));
            }
        }
        for child in [left, right] {
            if child != 0 && child != 0xFFFF {
                stack.push(usize::from(child) * 4);
            }
        }
    }
    xbe
}

/// An XDVDFS image (`.iso`, `.xiso`): the first partition offset whose
/// volume descriptor checks out, its root table, then the executable at
/// its root. Without one, an `.xiso` is the magic-only `xbox` verdict
/// and an `.iso` names nothing.
fn xdvdfs(b: &mut Budgeted, ext: &str) -> Option<ConsoleId> {
    let len = b.len();
    let mut found = None;
    for p in PARTITIONS {
        let at = p.checked_add(VOLUME_AT)?;
        if at.checked_add(VOLUME_LEN as u64)? > len {
            continue;
        }
        match b.read(at, VOLUME_LEN) {
            Ok(vd) if vd.starts_with(XDVDFS_MAGIC) => {
                found = Some((p, vd));
                break;
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    let (p, vd) = found?;
    let fallback = (ext == "xiso").then(|| ConsoleId::magic("xbox", "XDVDFS volume"));
    let (root_sector, root_size) = (le32(&vd, 0x14)?, le32(&vd, 0x18)?);
    if root_size == 0 || u64::from(root_size) > MAX_ROOT {
        return fallback;
    }
    let Some(root_at) = u64::from(root_sector)
        .checked_mul(SECTOR)
        .and_then(|o| o.checked_add(p))
    else {
        return fallback;
    };
    let Ok(table) = b.read(root_at, root_size as usize) else {
        return fallback;
    };
    let Some((exe, sector, size)) = root_exe(&table) else {
        return fallback;
    };
    let size = u64::from(size);
    let Some(start) = u64::from(sector)
        .checked_mul(SECTOR)
        .and_then(|o| o.checked_add(p))
        .filter(|s| s.checked_add(size).is_some_and(|e| e <= len))
    else {
        return fallback;
    };
    let mut at = |o: u64, n: usize| -> Option<Vec<u8>> {
        if o.checked_add(n as u64)? > size {
            return None;
        }
        b.read(start.checked_add(o)?, n).ok()
    };
    let (id, source, detail) = match exe {
        Exe::Xbe => (xbe(&mut at), IdSource::DefaultXbe, "XISO default.xbe"),
        Exe::Xex => (xex2(&mut at), IdSource::DefaultXex, "XISO default.xex"),
    };
    match id {
        Some(mut id) => {
            id.id_source = id.title_id.is_some().then_some(source);
            id.detail = detail.into();
            Some(id)
        }
        None => fallback,
    }
}

/// A Xenia ZAR (ZArchive, §8.2): a `default.xex` at its root is read
/// through its first block for the XEX header; otherwise an `.zar` is
/// the magic-only `xbox360` verdict. `None` without a ZArchive footer,
/// and for a ZArchive without `default.xex` under another extension (a
/// Cemu WUA is read by `wiiu::wua`).
pub fn zar(b: &mut Budgeted, ext: &str) -> Option<ConsoleId> {
    let footer = zarchive::footer(b)?;
    let magic = (ext == "zar").then(|| ConsoleId::magic("xbox360", "ZArchive footer"));
    let Some(archive) = footer.and_then(|f| zarchive::open(b, f)) else {
        return magic;
    };
    let Some(head) = archive
        .child(Archive::ROOT, "default.xex")
        .and_then(|i| archive.read_head(b, i, ZAR_XEX_HEAD))
    else {
        return magic;
    };
    let mut at = |o: u64, n: usize| -> Option<Vec<u8>> {
        let o = usize::try_from(o).ok()?;
        head.get(o..o.checked_add(n)?).map(<[u8]>::to_vec)
    };
    match xex2(&mut at) {
        Some(mut id) => {
            id.id_source = id.title_id.is_some().then_some(IdSource::DefaultXex);
            id.detail = "ZAR default.xex".into();
            Some(id)
        }
        None => magic,
    }
}

/// A `.xbox360` launcher (ES-DE, Batocera) whose whole text is an
/// 8-hex title id (D14). Coverly reads these, it never writes them.
pub fn launcher(head: &[u8], ext: &str) -> Option<ConsoleId> {
    if ext != "xbox360" || head.len() > LAUNCHER_MAX {
        return None;
    }
    let text = std::str::from_utf8(head)
        .ok()?
        .trim_start_matches('\u{feff}')
        .trim();
    if !is_hex(text, 8) {
        return None;
    }
    let tid = text.to_ascii_uppercase();
    let mut id = ConsoleId::magic("xbox360", "launcher file");
    id.serial = crate::disc::serial::normalise_console_serial("xbox360", &tid);
    id.title_id = Some(tid);
    id.id_source = Some(IdSource::FileName);
    Some(id)
}

/// An Xbox or Xbox 360 file by its own header: XBE, XEX, STFS under any
/// extension; ZAR under `.zar`; an XDVDFS image under `.iso`/`.xiso`.
pub fn identify(b: &mut Budgeted, ext: &str) -> Option<ConsoleId> {
    if b.len() < 4 {
        return None;
    }
    let head = b.read(0, 4).ok()?;
    let mut whole = |o: u64, n: usize| b.read(o, n).ok();
    match &head[..] {
        b"XBEH" => xbe(&mut whole),
        b"XEX2" | b"XEX1" | b"XEX%" | b"XEX-" => xex2(&mut whole),
        b"CON " | b"LIVE" | b"PIRS" => stfs(b),
        _ if ext == "zar" => zar(b, ext),
        _ if ext == "iso" || ext == "xiso" => xdvdfs(b, ext),
        _ => None,
    }
}

/// Runs `f` over the regular file `path` through a fresh [`Budgeted`].
fn with_file<R>(path: &Path, f: impl FnOnce(&mut Budgeted) -> Option<R>) -> Option<R> {
    let len = plain_file(path, u64::MAX)?;
    let mut src = FileAt(super::open_rom(path).ok()?);
    let mut b = Budgeted::new(&mut src, len);
    f(&mut b)
}

/// An executable read in a folder probe: the first 64 KiB only.
fn probe_exe(
    l: &DirListing,
    file: &str,
    read: fn(&mut BytesAt) -> Option<ConsoleId>,
    source: IdSource,
) -> Option<FolderMatch> {
    let name = find(l.names, file)?;
    let mut id = with_file(&l.dir.join(name), |b| {
        let mut at = |o: u64, n: usize| {
            if o.checked_add(n as u64)? > MAX_META_FILE {
                return None;
            }
            b.read(o, n).ok()
        };
        read(&mut at)
    })?;
    id.id_source = id.title_id.is_some().then_some(source);
    id.detail = file.into();
    Some(FolderMatch {
        id,
        probe: PathBuf::from(name),
    })
}

/// An extracted Xbox game folder (spec D12): `default.xbe` among its
/// names, its header and certificate read.
pub fn probe_xbe(l: &DirListing) -> Option<FolderMatch> {
    probe_exe(l, "default.xbe", xbe, IdSource::DefaultXbe)
}

/// An extracted Xbox 360 game folder (spec D12): `default.xex` among its
/// names, its header and execution info read.
pub fn probe_xex(l: &DirListing) -> Option<FolderMatch> {
    probe_exe(l, "default.xex", xex2, IdSource::DefaultXex)
}

/// A GOD or XBLA folder (spec D12): the folder is named by an 8-hex
/// title id and holds an 8-hex content-type folder, whose one listing
/// holds a 40-hex file opening with `CON `, `LIVE` or `PIRS`. Only that
/// STFS header is read. 8-hex folders are common in caches: without the
/// magic it is no match. The thumbnail range is relative to the probe
/// file.
pub fn probe_god(l: &DirListing) -> Option<FolderMatch> {
    let own = l.dir.file_name()?.to_str()?;
    if !is_hex(own, 8) {
        return None;
    }
    let mut subs: Vec<&String> = l
        .names
        .iter()
        .filter(|n| is_hex(n, 8))
        .filter(|n| {
            std::fs::symlink_metadata(l.dir.join(n.as_str())).is_ok_and(|m| m.file_type().is_dir())
        })
        .collect();
    // The base content types first; one listing only.
    subs.sort_by_key(|n| match u32::from_str_radix(n, 16) {
        Ok(CT_GOD) => (0, n.to_ascii_uppercase()),
        Ok(CT_XBLA) => (1, n.to_ascii_uppercase()),
        _ => (2, n.to_ascii_uppercase()),
    });
    let sub = subs.first()?;
    let names = list_child(&l.dir.join(sub.as_str()))?;
    let mut hashes: Vec<&String> = names.iter().filter(|n| is_hex(n, 40)).collect();
    hashes.sort();
    let hash = hashes.first()?;
    let id = with_file(&l.dir.join(sub.as_str()).join(hash.as_str()), stfs)?;
    Some(FolderMatch {
        id,
        probe: PathBuf::from(sub.as_str()).join(hash.as_str()),
    })
}

#[cfg(test)]
mod tests {
    use super::super::testing::{self as t, RangeLog, SparseAt};
    use super::super::{MAX_READ, MAX_SEEKS};
    use super::*;

    fn run(f: &mut SparseAt, ext: &str) -> Option<ConsoleId> {
        let len = f.len;
        let mut b = Budgeted::new(f, len);
        let id = identify(&mut b, ext);
        let used = b.used();
        assert!(used.read <= MAX_READ && used.seeks <= MAX_SEEKS, "{used:?}");
        id
    }

    fn file(bytes: &[u8]) -> SparseAt {
        let mut s = SparseAt::new(bytes.len() as u64);
        s.put(0, bytes);
        s
    }

    #[test]
    fn xbe_serials() {
        assert_eq!(xbe_serial(0x4541_000D).as_deref(), Some("EA-013"));
        assert_eq!(xbe_serial(0x4D53_0004).as_deref(), Some("MS-004"));
        assert_eq!(xbe_serial(0x0000_0004), None);
        // Four digits are not Redump's form.
        assert_eq!(xbe_serial(0x4D53_1000), None);
    }

    #[test]
    fn loose_xbe_certificate() {
        let id = run(&mut file(&t::xbe(0x4541_000D, "Burnout 3")), "xbe").unwrap();
        assert_eq!(id.system, "xbox");
        assert_eq!(id.serial.as_deref(), Some("EA-013"));
        assert_eq!(id.title_id.as_deref(), Some("4541000D"));
        assert_eq!(id.title.as_deref(), Some("Burnout 3"));
        assert_eq!(id.version.as_deref(), Some("1"));
        assert_eq!(
            (id.confidence, id.detail.as_str()),
            (0.95, "XBE certificate")
        );
    }

    #[test]
    fn xbe_with_a_far_certificate_is_magic_only() {
        let mut x = t::xbe(0x4541_000D, "Burnout 3");
        x[0x118..0x11C].copy_from_slice(&(0x0001_0000u32 + 0x2_0000).to_le_bytes());
        let id = run(&mut file(&x), "xbe").unwrap();
        assert_eq!((id.confidence, id.serial), (0.90, None));
        // An address below the base is no certificate either.
        x[0x118..0x11C].copy_from_slice(&0x10u32.to_le_bytes());
        assert_eq!(run(&mut file(&x), "xbe").unwrap().confidence, 0.90);
    }

    #[test]
    fn xdvdfs_at_each_partition_offset() {
        for p in PARTITIONS {
            let mut f = t::xiso_at(
                p,
                &[
                    ("media", vec![0u8; 16]),
                    ("default.xbe", t::xbe(0x4D53_0004, "Halo")),
                ],
            );
            let id = run(&mut f, "iso").unwrap_or_else(|| panic!("{p:#x}"));
            assert_eq!(id.system, "xbox", "{p:#x}");
            assert_eq!(id.serial.as_deref(), Some("MS-004"), "{p:#x}");
            assert_eq!(id.id_source, Some(IdSource::DefaultXbe));
            assert_eq!(id.detail, "XISO default.xbe");
        }
    }

    #[test]
    fn xdvdfs_default_xex_names_xbox360_even_beside_an_xbe() {
        let mut f = t::xiso_at(
            0x0208_0000,
            &[
                ("default.xbe", t::xbe(0x4D53_0004, "Stub")),
                ("default.xex", t::xex2(0x4D53_07E6)),
            ],
        );
        let id = run(&mut f, "iso").unwrap();
        assert_eq!(id.system, "xbox360");
        assert_eq!(id.serial.as_deref(), Some("4D5307E6"));
        assert_eq!(id.id_source, Some(IdSource::DefaultXex));
    }

    #[test]
    fn xdvdfs_without_an_executable() {
        let files = [("readme.txt", vec![b'x'; 8])];
        assert_eq!(run(&mut t::xiso_at(0, &files), "iso"), None);
        let id = run(&mut t::xiso_at(0, &files), "xiso").unwrap();
        assert_eq!((id.system.as_str(), id.confidence), ("xbox", 0.90));
        // Not an XDVDFS image at all.
        assert_eq!(run(&mut SparseAt::new(1 << 30), "xiso"), None);
    }

    /// A root table of `entries` raw entries at sector 33 of an XISO.
    fn xiso_with_table(table: &[u8]) -> SparseAt {
        let mut f = t::xiso_at(0, &[("x", vec![0u8; 4])]);
        let mut vd = vec![0u8; 0x20];
        vd[..20].copy_from_slice(XDVDFS_MAGIC);
        vd[0x14..0x18].copy_from_slice(&33u32.to_le_bytes());
        vd[0x18..0x1C].copy_from_slice(&(table.len() as u32).to_le_bytes());
        f.put(VOLUME_AT, &vd);
        f.len = f.len.max(33 * SECTOR + table.len() as u64 + SECTOR);
        f.put(33 * SECTOR, table);
        f
    }

    fn entry(left: u16, right: u16, name: &str) -> Vec<u8> {
        let mut e = vec![0xFFu8; (ENTRY_HEAD + name.len()).next_multiple_of(4)];
        e[0..2].copy_from_slice(&left.to_le_bytes());
        e[2..4].copy_from_slice(&right.to_le_bytes());
        e[4..12].fill(0);
        e[12] = 0x20;
        e[13] = name.len() as u8;
        e[14..14 + name.len()].copy_from_slice(name.as_bytes());
        e
    }

    #[test]
    fn xdvdfs_cycle_terminates() {
        // A → right B; B's left is itself and its right is C, whose right
        // points back to B.
        let mut table = entry(0, 4, "aa");
        table.extend(entry(4, 8, "bb"));
        table.extend(entry(0, 4, "cc"));
        assert_eq!(table.len(), 48);
        assert_eq!(root_exe(&table), None);
        assert_eq!(run(&mut xiso_with_table(&table), "iso"), None);
    }

    /// `n` entries chained through their right pointers, then
    /// `default.xbe`.
    fn chain(n: usize) -> Vec<u8> {
        let mut table = Vec::new();
        for i in 0..n {
            table.extend(entry(0, ((i + 1) * 4) as u16, "aa"));
        }
        table.extend(entry(0, 0, "default.xbe"));
        table
    }

    #[test]
    fn xdvdfs_2049_entries_stop_at_cap() {
        let table = chain(MAX_ENTRIES);
        assert!(table.len() as u64 <= MAX_ROOT);
        // The executable is the 2049th entry: past the cap.
        assert_eq!(root_exe(&table), None);
        assert_eq!(
            run(&mut xiso_with_table(&table), "xiso")
                .unwrap()
                .confidence,
            0.90
        );
        // The 2048th is still read.
        assert_eq!(
            root_exe(&chain(MAX_ENTRIES - 1)).map(|e| e.0),
            Some(Exe::Xbe)
        );
    }

    #[test]
    fn xex2_execution_info() {
        let id = run(&mut file(&t::xex2(0x4D53_07E6)), "xex").unwrap();
        assert_eq!(id.system, "xbox360");
        assert_eq!(id.title_id.as_deref(), Some("4D5307E6"));
        assert_eq!(id.serial.as_deref(), Some("4D5307E6"));
        assert_eq!(id.version.as_deref(), Some("20000100"));
        assert_eq!((id.kind, id.disc_no), (TitleKind::Base, None));
        assert_eq!(id.id_source, Some(IdSource::Header));
        // Disc 2 of 2.
        let mut x = t::xex2(0x4D53_07E6);
        x[t::XEX_EXEC_INFO + 0x12] = 2;
        x[t::XEX_EXEC_INFO + 0x13] = 2;
        assert_eq!(run(&mut file(&x), "xex").unwrap().disc_no, Some(2));
        // An older head is the system only.
        let mut old = t::xex2(1);
        old[..4].copy_from_slice(b"XEX1");
        let id = run(&mut file(&old), "xex").unwrap();
        assert_eq!((id.confidence, id.title_id), (0.90, None));
        // A header count over the cap is not read.
        let mut many = t::xex2(1);
        many[0x14..0x18].copy_from_slice(&0x8000_0000u32.to_be_bytes());
        assert_eq!(run(&mut file(&many), "xex").unwrap().confidence, 0.90);
    }

    #[test]
    fn stfs_fields_and_thumbnail_locator() {
        let thumb = [0x89, b'P', b'N', b'G', 0, 0];
        let mut f = file(&t::stfs(b"LIVE", 0x5841_0889, CT_XBLA, &thumb));
        let id = run(&mut f, "").unwrap();
        assert_eq!(id.system, "xbox360");
        assert_eq!(id.title_id.as_deref(), Some("58410889"));
        assert_eq!(id.title.as_deref(), Some("Test Arcade Game"));
        assert_eq!(
            (id.kind, id.detail.as_str()),
            (TitleKind::Base, "XBLA package")
        );
        assert_eq!(
            id.art,
            [ArtLocator::Range {
                offset: 0x171A,
                len: 6,
                format: ArtFormat::Png
            }]
        );
        assert_eq!(dat_name_prefix(&id).as_deref(), Some("58410889000D0000"));
        for (ctype, kind) in [
            (CT_GOD, TitleKind::Base),
            (CT_UPDATE, TitleKind::Update),
            (CT_DLC, TitleKind::Dlc),
            (0x0009_0000, TitleKind::Unknown),
        ] {
            let id = run(&mut file(&t::stfs(b"CON ", 1, ctype, &[])), "").unwrap();
            assert_eq!(id.kind, kind, "{ctype:#x}");
            assert!(id.art.is_empty());
        }
        let unknown = run(&mut file(&t::stfs(b"PIRS", 1, 0x0009_0000, &[])), "").unwrap();
        assert_eq!(dat_name_prefix(&unknown), None);
        // Without a display name the title name is used; a thumbnail size
        // over 16 KiB is no locator.
        let mut h = t::stfs(b"CON ", 1, CT_GOD, &[]);
        h[0x411..0x491].fill(0);
        h[0x1712..0x1716].copy_from_slice(&0x8000u32.to_be_bytes());
        let id = run(&mut file(&h), "").unwrap();
        assert_eq!(id.title.as_deref(), Some("Test Title"));
        assert!(id.art.is_empty());
        // A short file with the magic is the system only.
        let id = run(&mut file(b"CON \0\0\0\0"), "").unwrap();
        assert_eq!(id.confidence, 0.90);
    }

    #[test]
    fn stfs_data_blocks_are_never_read() {
        let mut bytes = t::stfs(b"LIVE", 0x5841_0889, CT_XBLA, &[1, 2]);
        bytes.resize(0x10_0000, 0xAB);
        let mut f = file(&bytes);
        let len = f.len;
        let mut log = RangeLog::new(&mut f);
        assert!(identify(&mut Budgeted::new(&mut log, len), "").is_some());
        assert!(!log.touched(0x171A, len - 0x171A), "{:?}", log.ranges);
    }

    #[test]
    fn zar_reads_the_default_xex_header() {
        let xex = t::xex2(0x4D53_07E6);
        let mut f = t::zarchive(&[("default.xex", &xex), ("media/a.bin", &[0u8; 8])]);
        let len = f.len;
        let id = zar(&mut Budgeted::new(&mut f, len), "zar").unwrap();
        assert_eq!(id.system, "xbox360");
        assert_eq!(id.serial.as_deref(), Some("4D5307E6"));
        assert_eq!(id.detail, "ZAR default.xex");
        // Without one: the magic under `.zar`, nothing elsewhere.
        let mut f = t::zarchive(&[("media/a.bin", &[0u8; 8])]);
        let len = f.len;
        let id = zar(&mut Budgeted::new(&mut f, len), "zar").unwrap();
        assert_eq!((id.confidence, id.serial), (0.90, None));
        let mut f = t::zarchive(&[("media/a.bin", &[0u8; 8])]);
        let len = f.len;
        assert_eq!(zar(&mut Budgeted::new(&mut f, len), "zst"), None);
    }

    #[test]
    fn launcher_text_is_a_title_id() {
        let id = launcher(b"4d5307e6\n", "xbox360").unwrap();
        assert_eq!(
            (id.title_id.as_deref(), id.serial.as_deref()),
            (Some("4D5307E6"), Some("4D5307E6"))
        );
        assert_eq!(launcher(b"Halo 3/default.xex", "xbox360"), None);
        assert_eq!(launcher(b"4D5307E6", "psn"), None);
    }

    #[test]
    fn other_files_are_nothing() {
        assert_eq!(run(&mut file(b"\0\0\0\0\0\0\0\0"), "iso"), None);
        assert_eq!(run(&mut file(b"XB"), "xbe"), None);
    }

    mod folders {
        use std::path::Path;

        use super::*;

        fn listing_of(dir: &Path) -> Vec<String> {
            let mut names: Vec<String> = std::fs::read_dir(dir)
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            names.sort();
            names
        }

        fn probe(dir: &Path, p: super::super::super::DirProbe) -> Option<FolderMatch> {
            let names = listing_of(dir);
            p(&DirListing { dir, names: &names })
        }

        #[test]
        fn xbe_and_xex_folders() {
            let tmp = tempfile::TempDir::new().unwrap();
            let halo = tmp.path().join("Halo");
            std::fs::create_dir(&halo).unwrap();
            std::fs::write(halo.join("default.xbe"), t::xbe(0x4D53_0004, "Halo")).unwrap();
            let m = probe(&halo, probe_xbe).unwrap();
            assert_eq!(m.probe, Path::new("default.xbe"));
            assert_eq!(
                (m.id.system.as_str(), m.id.serial.as_deref()),
                ("xbox", Some("MS-004"))
            );
            assert_eq!(m.id.id_source, Some(IdSource::DefaultXbe));
            assert_eq!(probe(&halo, probe_xex), None);
            let h3 = tmp.path().join("Halo 3");
            std::fs::create_dir(&h3).unwrap();
            std::fs::write(h3.join("default.xex"), t::xex2(0x4D53_07E6)).unwrap();
            let m = probe(&h3, probe_xex).unwrap();
            assert_eq!(m.id.system, "xbox360");
            assert_eq!(m.id.id_source, Some(IdSource::DefaultXex));
        }

        #[test]
        fn god_and_xbla_folders() {
            let tmp = tempfile::TempDir::new().unwrap();
            let top = t::god_folder(tmp.path(), 0x5841_0889, CT_XBLA, b"LIVE");
            let m = probe(&top, probe_god).unwrap();
            assert_eq!(m.probe, Path::new("000D0000").join(t::STFS_HASH));
            assert_eq!(m.id.title_id.as_deref(), Some("58410889"));
            assert_eq!(m.id.kind, TitleKind::Base);
            let god = t::god_folder(tmp.path(), 0x4D53_07E6, CT_GOD, b"CON ");
            assert_eq!(probe(&god, probe_god).unwrap().id.detail, "GOD package");
        }

        #[test]
        fn eight_hex_cache_folder_is_not_god() {
            let tmp = tempfile::TempDir::new().unwrap();
            let top = tmp.path().join("1A2B3C4D");
            let sub = top.join("00000001");
            std::fs::create_dir_all(&sub).unwrap();
            std::fs::write(sub.join(t::STFS_HASH), [0u8; 64]).unwrap();
            assert_eq!(probe(&top, probe_god), None);
            // The right layout under a name that is no title id.
            let named = tmp.path().join("Arcade");
            std::fs::create_dir(&named).unwrap();
            let inner = t::god_folder(&named, 0x5841_0889, CT_XBLA, b"LIVE");
            std::fs::rename(inner.join("000D0000"), named.join("000D0000")).unwrap();
            assert_eq!(probe(&named, probe_god), None);
        }
    }
}
