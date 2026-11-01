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
//! - Xbox One and Series folders (systems batch 3 D18): the GDK
//!   `MicrosoftGame.config` (root `Game`; `Identity`, `ExecutableList/
//!   Executable/@TargetDeviceFamily`, `ShellVisuals`, `TitleId`,
//!   `StoreId`, `MSAAppId`, `AllowedProducts/AllowedProduct`, the
//!   `TargetDeviceFamilyForDLC` element) as Microsoft's `Xbox-GDK-Samples`
//!   configs write it, and the ERA `AppxManifest.xml` whose `Package`
//!   declares `http://schemas.microsoft.com/appx/2013/xbox/manifest`
//!   (Microsoft `Xbox-ATG-Samples` XDK manifests). Logos are MRT names
//!   (`Logo.png` stored as `Logo.scale-200.png`), resolved here (D21).
//!
//! Only plaintext is read: the volume descriptor, the root table, the
//! XBE header and certificate, the XEX header and its execution info,
//! the STFS header block, the two manifests. An XEX's PE image
//! (encrypted or compressed), an STFS package's data blocks, a GDK or
//! ERA folder's executables and `resources.pri` are never read.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

use super::wiiu::{find, list_child, plain_file, read_head};
use super::zarchive::{self, Archive, ZarKind};
use super::{
    clean_title, ArtFormat, ArtLocator, Budgeted, ConsoleId, DirListing, FolderLeave, FolderMatch,
    IdSource, TitleKind, MAX_META_FILE,
};
use crate::identify::sniff::{FileAt, NotARom};

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

/// The `detail` of a PS4 `.zar` verdict (systems batch 3 D12).
pub const ZAR_PARAM_SFO_DETAIL: &str = "ZAR sce_sys/param.sfo";

/// A ZArchive by its tree (ZAR, §8.2; systems batch 3 D12, see
/// [`zarchive::classify`]): an Xbox 360 tree's `default.xex` is read
/// through its first block for the XEX header; a PS4 tree's
/// `sce_sys/param.sfo` and a PS5 tree's `sce_sys/param.json` are read
/// whole (at most [`zarchive::MAX_FILE_DECODED`]). `None` without a
/// ZArchive footer, with a tree that is unreadable or over budget, with
/// no decisive or two systems' names, or when the file read does not
/// parse: there is no magic-only verdict, so a `.zar` (Shared by three
/// systems) goes to review. A Cemu WUA is read by `wiiu::wua`.
pub fn zar(b: &mut Budgeted, _ext: &str) -> Option<ConsoleId> {
    let archive = zarchive::footer(b)?.and_then(|f| zarchive::open(b, f))?;
    let read = |b: &mut Budgeted, name: &str| {
        let i = archive.lookup(Archive::ROOT, &["sce_sys", name])?;
        archive.read_file(b, i, zarchive::MAX_FILE_DECODED)
    };
    match zarchive::classify(&archive) {
        ZarKind::Xbox360 => {
            let head = archive
                .child(Archive::ROOT, "default.xex")
                .and_then(|i| archive.read_head(b, i, ZAR_XEX_HEAD))?;
            let mut at = |o: u64, n: usize| -> Option<Vec<u8>> {
                let o = usize::try_from(o).ok()?;
                head.get(o..o.checked_add(n)?).map(<[u8]>::to_vec)
            };
            let mut id = xex2(&mut at)?;
            id.id_source = id.title_id.is_some().then_some(IdSource::DefaultXex);
            id.detail = "ZAR default.xex".into();
            Some(id)
        }
        kind @ (ZarKind::Ps4 | ZarKind::Ps4Dlc) => {
            let psf = super::psf::parse(&read(b, "param.sfo")?)?;
            let mut id = super::sony::from_psf(&psf, "ps4", ZAR_PARAM_SFO_DETAIL);
            if kind == ZarKind::Ps4Dlc && psf.text("CATEGORY").is_none() {
                id.kind = TitleKind::Dlc;
            }
            Some(id)
        }
        ZarKind::Ps5 => {
            let mut id = super::orbis::param_json(&read(b, "param.json")?, true)?;
            id.detail = if id.detail == super::orbis::PARAM_JSON_PRO_DETAIL {
                super::orbis::ZAR_PARAM_JSON_PRO_DETAIL
            } else {
                super::orbis::ZAR_PARAM_JSON_DETAIL
            }
            .into();
            Some(id)
        }
        ZarKind::Ambiguous | ZarKind::Nothing => None,
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
        leave: None,
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
        leave: None,
    })
}

/// The GDK probe file (D18).
pub const GDK_CONFIG: &str = "MicrosoftGame.config";
/// The ERA probe file (D18).
pub const ERA_MANIFEST: &str = "AppxManifest.xml";
/// The namespace an Xbox One ERA `Package` declares (D18).
pub const XBOX_MANIFEST_NS: &str = "http://schemas.microsoft.com/appx/2013/xbox/manifest";
/// Most components of a manifest-relative logo path (D21).
const MAX_LOGO_COMPONENTS: usize = 8;
/// Deepest element nesting a manifest may have.
const MAX_XML_DEPTH: usize = 32;
/// Confidence of a GDK folder whose executables add a `PC` build to one
/// console family: a single-choice review, never Auto (D18).
const GDK_WITH_PC: f32 = 0.60;
/// Confidence of a GDK config whose families name the system.
const GDK_PROVEN: f32 = 0.95;
/// Confidence of a GDK config that names no family: review (D18).
const GDK_NO_FAMILY: f32 = 0.50;

/// A manifest's text: at most [`MAX_META_FILE`] bytes, UTF-8 (BOM
/// stripped) or UTF-16 LE/BE with a BOM; anything else, or text with a
/// NUL (UTF-16 without a BOM), is `None`.
fn manifest_text(bytes: &[u8]) -> Option<String> {
    if bytes.len() as u64 > MAX_META_FILE {
        return None;
    }
    let utf16 = |rest: &[u8], unit: fn([u8; 2]) -> u16| -> Option<String> {
        let (pairs, tail) = rest.as_chunks::<2>();
        if !tail.is_empty() {
            return None;
        }
        let units: Vec<u16> = pairs.iter().map(|&c| unit(c)).collect();
        String::from_utf16(&units).ok()
    };
    let text = if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        utf16(rest, u16::from_le_bytes)?
    } else if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        utf16(rest, u16::from_be_bytes)?
    } else {
        let body = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
        std::str::from_utf8(body).ok()?.to_owned()
    };
    (!text.contains('\0')).then_some(text)
}

/// One step of [`xml_walk`]: an element opens (its path of local names,
/// root first) or text arrives inside the element at `path`.
enum Node<'a, 'e> {
    Open(&'a [String], &'a BytesStart<'e>),
    Text(&'a [String], &'a str),
}

/// Walks a manifest's elements with quick-xml: no DTD (a DOCTYPE is
/// refused), only the five XML entities and character references, end
/// names checked, at most [`MAX_XML_DEPTH`] levels. `None` when the text
/// is not well formed or `f` says stop.
fn xml_walk(text: &str, mut f: impl FnMut(Node) -> Option<()>) -> Option<()> {
    let mut reader = Reader::from_str(text);
    let mut path: Vec<String> = Vec::new();
    let local = |e: &BytesStart| -> Option<String> {
        Some(
            std::str::from_utf8(e.local_name().as_ref())
                .ok()?
                .to_owned(),
        )
    };
    loop {
        match reader.read_event().ok()? {
            Event::DocType(_) => return None,
            Event::Start(e) => {
                path.push(local(&e)?);
                if path.len() > MAX_XML_DEPTH {
                    return None;
                }
                f(Node::Open(&path, &e))?;
            }
            Event::Empty(e) => {
                path.push(local(&e)?);
                if path.len() > MAX_XML_DEPTH {
                    return None;
                }
                f(Node::Open(&path, &e))?;
                path.pop();
            }
            Event::End(_) => {
                path.pop()?;
            }
            Event::Text(t) => {
                let s = t.decode().ok()?;
                f(Node::Text(&path, &s))?;
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
                f(Node::Text(&path, c.encode_utf8(&mut [0u8; 4])))?;
            }
            Event::Eof => break,
            _ => {}
        }
    }
    path.is_empty().then_some(())
}

/// The value of the unprefixed attribute `name` of `e`, trimmed; `None`
/// when absent, empty or not decodable.
fn attr(e: &BytesStart, name: &str) -> Option<String> {
    e.attributes()
        .flatten()
        .find(|a| a.key.as_ref() == name.as_bytes())
        .and_then(|a| a.unescape_value().ok().map(|v| v.trim().to_owned()))
        .filter(|v| !v.is_empty())
}

/// Appends `s` to the text field opened last (`Some`), if any.
fn push_text(field: &mut Option<String>, s: &str) {
    if let Some(v) = field {
        v.push_str(s);
    }
}

/// A trimmed, non-empty element text.
fn text_of(field: Option<String>) -> Option<String> {
    field.map(|v| v.trim().to_owned()).filter(|v| !v.is_empty())
}

/// A store or package id as the manifests write it (`9NBLGGH4R2R6`,
/// `Microsoft.SunsetOverdrive`): 1 to 64 ASCII letters, digits, `.`,
/// `-` or `_`.
fn clean_id(raw: &str) -> Option<String> {
    let v = raw.trim();
    (!v.is_empty()
        && v.len() <= 64
        && v.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_')))
    .then(|| v.to_owned())
}

/// A display name unless it is an unresolved `ms-resource:` reference
/// (no `resources.pri` parser in this batch, D18).
fn plain_name(raw: Option<&str>) -> Option<String> {
    raw.filter(|n| {
        !n.trim_start()
            .get(..12)
            .is_some_and(|p| p.eq_ignore_ascii_case("ms-resource:"))
    })
    .and_then(clean_title)
}

/// The fields of a GDK `MicrosoftGame.config` (D18).
#[derive(Debug, Default, PartialEq)]
struct Gdk {
    name: Option<String>,
    version: Option<String>,
    title_id: Option<String>,
    store_id: Option<String>,
    /// Read for the record; no [`ConsoleId`] field holds it yet.
    #[allow(dead_code)]
    msa_app_id: Option<String>,
    display_name: Option<String>,
    /// `Square480x480Logo`, `StoreLogo`, `Square150x150Logo`, in that
    /// order, as written.
    logos: Vec<String>,
    /// Every `ExecutableList/Executable/@TargetDeviceFamily`.
    families: Vec<String>,
    /// The `TargetDeviceFamilyForDLC` element, else an attribute of that
    /// name on any element.
    dlc_family: Option<String>,
    allowed_products: Vec<String>,
}

/// Parses a GDK config's text (root element `Game`). `XboxCompatibility`
/// and `VirtualMachine/DlcCompatibility` are never read (D18).
fn parse_gdk(text: &str) -> Option<Gdk> {
    let mut g = Gdk::default();
    let (mut title_id, mut store_id, mut msa, mut dlc_elem) = (None, None, None, None);
    let mut dlc_attr: Option<String> = None;
    let mut product: Option<String> = None;
    let mut seen_root = false;
    xml_walk(text, |node| {
        match node {
            Node::Open(path, e) => {
                if path.len() == 1 {
                    if seen_root || path[0] != "Game" {
                        return None;
                    }
                    seen_root = true;
                }
                if let Some(p) = product.take() {
                    g.allowed_products.push(p);
                }
                if dlc_attr.is_none() {
                    dlc_attr = attr(e, "TargetDeviceFamilyForDLC");
                }
                let p: Vec<&str> = path.iter().map(String::as_str).collect();
                match p.as_slice() {
                    ["Game", "Identity"] => {
                        g.name = attr(e, "Name");
                        g.version = attr(e, "Version");
                    }
                    ["Game", "ShellVisuals"] => {
                        g.display_name = attr(e, "DefaultDisplayName");
                        g.logos = ["Square480x480Logo", "StoreLogo", "Square150x150Logo"]
                            .iter()
                            .filter_map(|k| attr(e, k))
                            .collect();
                    }
                    ["Game", "ExecutableList", "Executable"] => {
                        g.families.extend(attr(e, "TargetDeviceFamily"));
                    }
                    ["Game", "TitleId"] => title_id = Some(String::new()),
                    ["Game", "StoreId"] => store_id = Some(String::new()),
                    ["Game", "MSAAppId"] => msa = Some(String::new()),
                    ["Game", "TargetDeviceFamilyForDLC"] => dlc_elem = Some(String::new()),
                    ["Game", "AllowedProducts", "AllowedProduct"] => {
                        product = Some(String::new());
                    }
                    _ => {}
                }
            }
            Node::Text(path, s) => {
                let p: Vec<&str> = path.iter().map(String::as_str).collect();
                match p.as_slice() {
                    ["Game", "TitleId"] => push_text(&mut title_id, s),
                    ["Game", "StoreId"] => push_text(&mut store_id, s),
                    ["Game", "MSAAppId"] => push_text(&mut msa, s),
                    ["Game", "TargetDeviceFamilyForDLC"] => push_text(&mut dlc_elem, s),
                    ["Game", "AllowedProducts", "AllowedProduct"] => push_text(&mut product, s),
                    _ => {}
                }
            }
        }
        Some(())
    })?;
    if !seen_root {
        return None;
    }
    g.allowed_products.extend(product);
    g.allowed_products = g
        .allowed_products
        .iter()
        .filter_map(|p| clean_id(p))
        .collect();
    g.title_id = text_of(title_id);
    g.store_id = text_of(store_id);
    g.msa_app_id = text_of(msa);
    g.dlc_family = text_of(dlc_elem).or(dlc_attr);
    Some(g)
}

/// The console systems and whether a PC build is named, from GDK device
/// families; unknown families are ignored.
fn gdk_families<'a>(families: impl Iterator<Item = &'a str>) -> (Vec<&'static str>, bool) {
    let mut systems: Vec<&'static str> = Vec::new();
    let mut pc = false;
    for f in families {
        let sys = if f.eq_ignore_ascii_case("XboxOne") {
            "xboxone"
        } else if f.eq_ignore_ascii_case("Scarlett") {
            "xboxseries"
        } else {
            pc |= f.eq_ignore_ascii_case("PC");
            continue;
        };
        if !systems.contains(&sys) {
            systems.push(sys);
        }
    }
    // Probe order: Xbox One first.
    systems.sort_by_key(|s| *s != "xboxone");
    (systems, pc)
}

/// The folder's own name, as a title fallback.
fn folder_title(l: &DirListing) -> Option<String> {
    clean_title(&l.dir.file_name()?.to_string_lossy())
}

/// The first of `rels` that resolves inside the folder (D21), at most
/// one extra listing for all of them.
fn first_logo<'a>(l: &DirListing, rels: impl Iterator<Item = &'a str>) -> Option<ArtLocator> {
    let mut listed = false;
    rels.into_iter()
        .find_map(|rel| logo_locator(l, rel, &mut listed))
}

/// A manifest-relative logo path resolved inside the folder (D21):
/// split on `\` and `/`; an empty, `.` or `..` component, a root or a
/// drive prefix, more than [`MAX_LOGO_COMPONENTS`] components, a name
/// that is not `.png` or a symlink anywhere on the way drops it. The
/// exact name comes first, then the same name in another case, then the
/// largest MRT `<stem>.scale-<n>.png` beside it. Names at the folder's
/// root come from the walk's listing; a subfolder costs one listing, and
/// `listed` allows one per probe.
fn logo_locator(l: &DirListing, rel: &str, listed: &mut bool) -> Option<ArtLocator> {
    let parts: Vec<&str> = rel.split(['\\', '/']).collect();
    if parts.len() > MAX_LOGO_COMPONENTS
        || parts
            .iter()
            .any(|c| c.is_empty() || *c == "." || *c == ".." || c.contains(':'))
    {
        return None;
    }
    let (file, parents) = parts.split_last()?;
    let stem = file
        .len()
        .checked_sub(4)
        .filter(|&n| {
            file.get(n..)
                .is_some_and(|e| e.eq_ignore_ascii_case(".png"))
        })
        .and_then(|n| file.get(..n))?;
    let mut at = l.dir.to_path_buf();
    let mut rel_dir = PathBuf::new();
    for c in parents {
        at.push(c);
        rel_dir.push(c);
        if !std::fs::symlink_metadata(&at).ok()?.file_type().is_dir() {
            return None;
        }
    }
    let found = |name: &str| -> Option<ArtLocator> {
        let meta = std::fs::symlink_metadata(at.join(name)).ok()?;
        meta.file_type().is_file().then(|| ArtLocator::File {
            rel: rel_dir.join(name),
            format: ArtFormat::Png,
        })
    };
    if let Ok(meta) = std::fs::symlink_metadata(at.join(file)) {
        return meta.file_type().is_file().then(|| ArtLocator::File {
            rel: rel_dir.join(file),
            format: ArtFormat::Png,
        });
    }
    let listing: Vec<String>;
    let names: &[String] = if parents.is_empty() {
        l.names
    } else {
        if *listed {
            return None;
        }
        *listed = true;
        listing = list_child(&at)?;
        &listing
    };
    if let Some(same) = names.iter().find(|n| n.eq_ignore_ascii_case(file)) {
        return found(same);
    }
    let prefix = format!("{}.scale-", stem.to_ascii_lowercase());
    let best = names
        .iter()
        .filter_map(|n| {
            let lower = n.to_ascii_lowercase();
            let scale = lower.strip_prefix(&prefix)?.strip_suffix(".png")?;
            (!scale.is_empty() && scale.bytes().all(|b| b.is_ascii_digit()))
                .then(|| scale.parse::<u32>().ok())
                .flatten()
                .map(|s| (s, n))
        })
        .max_by_key(|(s, _)| *s)?;
    found(best.1)
}

/// Reads a probe file of at most [`MAX_META_FILE`] bytes; `Err(())` when
/// it is present but cannot be read (or is not a plain small file).
fn read_probe(path: &Path) -> Result<Vec<u8>, ()> {
    let len = plain_file(path, MAX_META_FILE).ok_or(())?;
    read_head(path, len, MAX_META_FILE as usize).ok_or(())
}

/// An Xbox One or Series GDK game folder (D18): `MicrosoftGame.config`
/// among its names (any case). A config that cannot be read leaves the
/// folder `Unreadable`; a PC build leaves it as PC game data; executables
/// for both consoles, or no family at all, give one match per console
/// (a conflict: review); a PC build beside one console family is that
/// system at [`GDK_WITH_PC`] (single-choice review). Executables make a
/// base game, a DLC config's `TargetDeviceFamilyForDLC` a DLC.
pub fn probe_gdk(l: &DirListing) -> Vec<FolderMatch> {
    let Some(name) = find(l.names, GDK_CONFIG) else {
        return Vec::new();
    };
    let probe = PathBuf::from(name);
    let leave = |why: FolderLeave| {
        vec![FolderMatch {
            id: ConsoleId::magic("xboxone", GDK_CONFIG),
            probe: probe.clone(),
            leave: Some(why),
        }]
    };
    let Ok(bytes) = read_probe(&l.dir.join(name)) else {
        return leave(FolderLeave::Unreadable);
    };
    let Some(g) = manifest_text(&bytes).and_then(|t| parse_gdk(&t)) else {
        return Vec::new();
    };
    let (mut systems, mut pc) = gdk_families(g.families.iter().map(String::as_str));
    let mut kind = TitleKind::Base;
    if systems.is_empty() && !pc {
        (systems, pc) = gdk_families(g.dlc_family.as_deref().into_iter());
        kind = TitleKind::Dlc;
    }
    let confidence = match (systems.len(), pc) {
        (0, true) => return leave(FolderLeave::NotARom(NotARom::PcGameData)),
        (0, false) => {
            systems = vec!["xboxone", "xboxseries"];
            kind = TitleKind::Unknown;
            GDK_NO_FAMILY
        }
        (1, true) => GDK_WITH_PC,
        _ => GDK_PROVEN,
    };
    let art = first_logo(l, g.logos.iter().map(String::as_str));
    systems
        .into_iter()
        .map(|system| {
            let mut id = ConsoleId::proven(system, GDK_CONFIG);
            id.confidence = confidence;
            id.kind = kind;
            id.title_id = g
                .title_id
                .as_deref()
                .filter(|t| is_hex(t, 8) && !t.eq_ignore_ascii_case("FFFFFFFF"))
                .map(str::to_ascii_uppercase);
            // The identity's source, also when it has no title id: the
            // grouping key (D15) needs to know a GDK config gave it.
            id.id_source = Some(IdSource::MicrosoftGameConfig);
            id.product_id = g.store_id.as_deref().and_then(clean_id);
            id.parent_id = match g.allowed_products.as_slice() {
                [one] => Some(one.clone()),
                _ => None,
            };
            id.version = g.version.clone();
            id.title = plain_name(g.display_name.as_deref()).or_else(|| folder_title(l));
            id.art.extend(art.clone());
            FolderMatch {
                id,
                probe: probe.clone(),
                leave: None,
            }
        })
        .collect()
}

/// The fields of an ERA `AppxManifest.xml` (D18).
#[derive(Debug, Default)]
struct Era {
    xbox: bool,
    name: Option<String>,
    version: Option<String>,
    display_name: Option<String>,
    logo: Option<String>,
    visual_name: Option<String>,
    visual_logo: Option<String>,
}

/// Parses an ERA manifest's text (root element `Package`, any prefix).
fn parse_era(text: &str) -> Option<Era> {
    let mut m = Era::default();
    let (mut display, mut logo): (Option<String>, Option<String>) = (None, None);
    let mut seen_root = false;
    let mut in_app = false;
    xml_walk(text, |node| {
        match node {
            Node::Open(path, e) => {
                let p: Vec<&str> = path.iter().map(String::as_str).collect();
                match p.as_slice() {
                    ["Package"] => {
                        if seen_root {
                            return None;
                        }
                        seen_root = true;
                        m.xbox = e.attributes().flatten().any(|a| {
                            let k = a.key.as_ref();
                            (k == b"xmlns" || k.starts_with(b"xmlns:"))
                                && a.unescape_value()
                                    .is_ok_and(|v| v.trim() == XBOX_MANIFEST_NS)
                        });
                    }
                    [_] => return None,
                    ["Package", "Identity"] => {
                        m.name = attr(e, "Name");
                        m.version = attr(e, "Version");
                    }
                    ["Package", "Properties", "DisplayName"] => display = Some(String::new()),
                    ["Package", "Properties", "Logo"] => logo = Some(String::new()),
                    ["Package", "Applications", "Application", "VisualElements"] if !in_app => {
                        in_app = true;
                        m.visual_name = attr(e, "DisplayName");
                        m.visual_logo = attr(e, "Logo");
                    }
                    _ => {}
                }
            }
            Node::Text(path, s) => {
                let p: Vec<&str> = path.iter().map(String::as_str).collect();
                match p.as_slice() {
                    ["Package", "Properties", "DisplayName"] => push_text(&mut display, s),
                    ["Package", "Properties", "Logo"] => push_text(&mut logo, s),
                    _ => {}
                }
            }
        }
        Some(())
    })?;
    if !seen_root {
        return None;
    }
    m.display_name = text_of(display);
    m.logo = text_of(logo);
    Some(m)
}

/// An Xbox One ERA game folder (D18): `AppxManifest.xml` among its names
/// whose `Package` declares [`XBOX_MANIFEST_NS`]; without it the folder
/// is a Windows app. A folder with a GDK config is left to
/// [`probe_gdk`] (GDK PC builds carry an `AppxManifest.xml` too).
pub fn probe_era(l: &DirListing) -> Option<FolderMatch> {
    if find(l.names, GDK_CONFIG).is_some() {
        return None;
    }
    let name = find(l.names, ERA_MANIFEST)?;
    let bytes = read_probe(&l.dir.join(name)).ok()?;
    let m = parse_era(&manifest_text(&bytes)?)?;
    if !m.xbox {
        return None;
    }
    let mut id = ConsoleId::proven("xboxone", ERA_MANIFEST);
    id.kind = TitleKind::Base;
    id.id_source = Some(IdSource::AppxManifest);
    id.product_id = m.name.as_deref().and_then(clean_id);
    id.version = m.version.clone();
    id.title = plain_name(m.display_name.as_deref())
        .or_else(|| plain_name(m.visual_name.as_deref()))
        .or_else(|| folder_title(l));
    id.art.extend(first_logo(
        l,
        m.visual_logo
            .iter()
            .chain(m.logo.iter())
            .map(String::as_str),
    ));
    Some(FolderMatch {
        id,
        probe: PathBuf::from(name),
        leave: None,
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

    /// A Redump image opens with its video partition (an ISO 9660 DVD
    /// volume); the disc rules must leave it to this reader, and the
    /// readers before it must leave the budget for the game partition.
    #[test]
    fn redump_images_behind_a_video_partition() {
        let video = crate::disc::testing::IsoBuilder::new("")
            .dir("VIDEO_TS")
            .file("VIDEO_TS/VIDEO_TS.IFO", &[0u8; 64])
            .build();
        for (p, exe, system, serial) in [
            (t::XGD1, "default.xbe", "xbox", "MS-004"),
            (0x0FD9_0000, "default.xex", "xbox360", "4D5307E6"),
            (0x0208_0000, "default.xex", "xbox360", "4D5307E6"),
        ] {
            let bytes = if exe == "default.xbe" {
                t::xbe(0x4D53_0004, "Halo")
            } else {
                t::xex2(0x4D53_07E6)
            };
            let mut f = t::xiso_at(p, &[(exe, bytes)]);
            f.put(0, &video);
            let len = f.len;
            let s = crate::identify::sniff::sniff_named(&mut f, len, "iso", "Game (USA).iso");
            let hit = s.hits.first().unwrap_or_else(|| panic!("{p:#x}: {s:?}"));
            assert_eq!(hit.system, system, "{p:#x}");
            assert_eq!(s.serial.as_deref(), Some(serial), "{p:#x}");
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
        let mut f = t::zarchive(&[("media/a.bin", &[0u8; 8])]);
        let len = f.len;
        assert_eq!(zar(&mut Budgeted::new(&mut f, len), "zst"), None);
    }

    /// Spec D12: the magic alone no longer names the Xbox 360.
    #[test]
    fn zar_without_default_xex_has_no_verdict() {
        let mut f = t::zarchive(&[("media/a.bin", &[0u8; 8])]);
        let len = f.len;
        assert_eq!(zar(&mut Budgeted::new(&mut f, len), "zar"), None);
        // An unreadable `default.xex` is no verdict either.
        let mut f = t::zarchive(&[("default.xex", b"not a xex".as_slice())]);
        let len = f.len;
        assert_eq!(zar(&mut Budgeted::new(&mut f, len), "zar"), None);
        // Two systems in one tree.
        let xex = t::xex2(0x4D53_07E6);
        let mut f = t::zarchive(&[("default.xex", &xex), ("eboot.bin", &[0u8; 8])]);
        let len = f.len;
        assert_eq!(zar(&mut Budgeted::new(&mut f, len), "zar"), None);
    }

    #[test]
    fn zar_with_a_ps4_tree_reads_its_param_sfo() {
        let sfo = t::ps4_sfo("CUSA04447", "gd", "EP0002-CUSA04447_00-GRAVITYRUSH00000", 1);
        let mut f = t::zarchive(&[("eboot.bin", &[0u8; 64]), ("sce_sys/param.sfo", &sfo)]);
        let len = f.len;
        let id = zar(&mut Budgeted::new(&mut f, len), "zar").unwrap();
        assert_eq!(id.system, "ps4");
        assert_eq!(id.title_id.as_deref(), Some("CUSA04447"));
        assert_eq!(id.kind, TitleKind::Base);
        assert_eq!(id.detail, ZAR_PARAM_SFO_DETAIL);
        assert_eq!(id.id_source, Some(IdSource::ParamSfo));
        assert_eq!(id.region, Some(crate::identify::names::Region::Europe));
        // DLC: `param.sfo` without `eboot.bin`; no category still reads
        // as DLC.
        let sfo = t::ps4_sfo("CUSA04447", "", "EP0002-CUSA04447_00-GRAVITYRUSHDLC01", 1);
        let mut f = t::zarchive(&[("sce_sys/param.sfo", &sfo)]);
        let len = f.len;
        let id = zar(&mut Budgeted::new(&mut f, len), "zar").unwrap();
        assert_eq!((id.system.as_str(), id.kind), ("ps4", TitleKind::Dlc));
        // A PS4 tree whose `param.sfo` does not parse: no verdict.
        let mut f = t::zarchive(&[("eboot.bin", &[0u8; 64]), ("sce_sys/param.sfo", b"junk")]);
        let len = f.len;
        assert_eq!(zar(&mut Budgeted::new(&mut f, len), "zar"), None);
    }

    #[test]
    fn zar_with_a_ps5_tree_reads_its_param_json() {
        let json = t::ps5_param_json("PPSA01284", "Returnal", false);
        let mut f = t::zarchive(&[("eboot.bin", &[0u8; 64]), ("sce_sys/param.json", &json)]);
        let len = f.len;
        let id = zar(&mut Budgeted::new(&mut f, len), "zar").unwrap();
        assert_eq!(id.system, "ps5");
        assert_eq!(id.title_id.as_deref(), Some("PPSA01284"));
        assert_eq!(id.title.as_deref(), Some("Returnal"));
        assert_eq!(id.kind, TitleKind::Base);
        assert_eq!(id.detail, super::super::orbis::ZAR_PARAM_JSON_DETAIL);
        let json = t::ps5_param_json("PPSA01284", "Returnal", true);
        let mut f = t::zarchive(&[("eboot.bin", &[0u8; 64]), ("sce_sys/param.json", &json)]);
        let len = f.len;
        let id = zar(&mut Budgeted::new(&mut f, len), "zar").unwrap();
        assert_eq!(id.detail, super::super::orbis::ZAR_PARAM_JSON_PRO_DETAIL);
    }

    #[test]
    fn zar_over_the_tree_budget_has_no_verdict() {
        // More than `MAX_SECTION` bytes of file tree (16-byte entries).
        let names: Vec<String> = (0..17_000).map(|i| format!("f{i:05}")).collect();
        let sfo = t::ps4_sfo("CUSA04447", "gd", "EP0002-CUSA04447_00-GRAVITYRUSH00000", 1);
        let mut files: Vec<(&str, &[u8])> =
            vec![("eboot.bin", &[0u8; 64]), ("sce_sys/param.sfo", &sfo)];
        files.extend(names.iter().map(|n| (n.as_str(), b"x".as_slice())));
        let mut f = t::zarchive(&files);
        let len = f.len;
        let mut b = Budgeted::new(&mut f, len);
        let footer = zarchive::footer(&mut b).unwrap().unwrap();
        assert!(footer.tree.size > zarchive::MAX_SECTION);
        assert_eq!(zar(&mut b, "zar"), None);
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

        /// A probe file that is a symlink is never followed (spec §8.6).
        #[cfg(unix)]
        #[test]
        fn symlinked_probe_files_are_no_match() {
            let tmp = tempfile::TempDir::new().unwrap();
            let real = tmp.path().join("real.xbe");
            std::fs::write(&real, t::xbe(0x4D53_0004, "Halo")).unwrap();
            let halo = tmp.path().join("Halo");
            std::fs::create_dir(&halo).unwrap();
            std::os::unix::fs::symlink(&real, halo.join("default.xbe")).unwrap();
            assert_eq!(probe(&halo, probe_xbe), None);
            let top = t::god_folder(tmp.path(), 0x5841_0889, CT_XBLA, b"LIVE");
            let hash = top.join("000D0000").join(t::STFS_HASH);
            std::fs::rename(&hash, tmp.path().join("hash")).unwrap();
            std::os::unix::fs::symlink(tmp.path().join("hash"), &hash).unwrap();
            assert_eq!(probe(&top, probe_god), None);
            // A symlinked content-type folder is not listed either.
            let other = tmp.path().join("other");
            std::fs::create_dir(&other).unwrap();
            let top2 = t::god_folder(&other, 0x5841_0889, CT_XBLA, b"LIVE");
            let linked = tmp.path().join("linked").join("58410889");
            std::fs::create_dir_all(&linked).unwrap();
            std::os::unix::fs::symlink(top2.join("000D0000"), linked.join("000D0000")).unwrap();
            assert_eq!(probe(&linked, probe_god), None);
        }

        fn gdk(dir: &Path, spec: &t::GdkSpec) -> Vec<FolderMatch> {
            t::gdk_folder(dir, spec);
            let names = listing_of(dir);
            probe_gdk(&DirListing { dir, names: &names })
        }

        fn systems(found: &[FolderMatch]) -> Vec<(&str, f32, TitleKind)> {
            found
                .iter()
                .map(|m| (m.id.system.as_str(), m.id.confidence, m.id.kind))
                .collect()
        }

        /// Every row of the D18 table (plan Task 10).
        #[test]
        fn gdk_families_table() {
            let tmp = tempfile::TempDir::new().unwrap();
            type Row<'a> = (
                &'a [&'a str],
                Option<&'a str>,
                &'a [(&'a str, f32, TitleKind)],
            );
            let rows: &[Row] = &[
                (&["XboxOne"], None, &[("xboxone", 0.95, TitleKind::Base)]),
                (
                    &["Scarlett", "Scarlett"],
                    None,
                    &[("xboxseries", 0.95, TitleKind::Base)],
                ),
                (
                    &["XboxOne", "Scarlett"],
                    None,
                    &[
                        ("xboxone", 0.95, TitleKind::Base),
                        ("xboxseries", 0.95, TitleKind::Base),
                    ],
                ),
                (
                    &["PC", "XboxOne"],
                    None,
                    &[("xboxone", 0.60, TitleKind::Base)],
                ),
                (
                    &["Scarlett", "PC"],
                    None,
                    &[("xboxseries", 0.60, TitleKind::Base)],
                ),
                (&[], Some("XboxOne"), &[("xboxone", 0.95, TitleKind::Dlc)]),
                (
                    &[],
                    Some("Scarlett"),
                    &[("xboxseries", 0.95, TitleKind::Dlc)],
                ),
                (
                    &[],
                    None,
                    &[
                        ("xboxone", 0.50, TitleKind::Unknown),
                        ("xboxseries", 0.50, TitleKind::Unknown),
                    ],
                ),
            ];
            for (i, (families, dlc, want)) in rows.iter().enumerate() {
                let dir = tmp.path().join(format!("g{i}"));
                let spec = t::GdkSpec {
                    families,
                    dlc_family: *dlc,
                    ..Default::default()
                };
                let found = gdk(&dir, &spec);
                assert_eq!(systems(&found), *want, "{families:?} {dlc:?}");
                assert!(found.iter().all(|m| m.leave.is_none()));
                assert!(found
                    .iter()
                    .all(|m| m.probe == Path::new("MicrosoftGame.config")));
            }
            // Every executable PC, or a PC DLC: a PC build, left.
            for (i, (families, dlc)) in [(&["PC", "PC"][..], None), (&[][..], Some("PC"))]
                .into_iter()
                .enumerate()
            {
                let dir = tmp.path().join(format!("pc{i}"));
                let spec = t::GdkSpec {
                    families,
                    dlc_family: dlc,
                    ..Default::default()
                };
                let found = gdk(&dir, &spec);
                assert_eq!(found.len(), 1);
                assert_eq!(
                    found[0].leave,
                    Some(FolderLeave::NotARom(NotARom::PcGameData))
                );
            }
        }

        #[test]
        fn gdk_fields() {
            let tmp = tempfile::TempDir::new().unwrap();
            let found = gdk(&tmp.path().join("Halo"), &t::GdkSpec::default());
            let id = &found[0].id;
            assert_eq!(id.title_id.as_deref(), Some("6B9D1A2C"));
            assert_eq!(id.id_source, Some(IdSource::MicrosoftGameConfig));
            assert_eq!(id.product_id.as_deref(), Some("9NSAMPLE0001"));
            assert_eq!(id.parent_id, None);
            assert_eq!(id.version.as_deref(), Some("1.2.3.0"));
            assert_eq!(id.title.as_deref(), Some("Sample Game"));
            assert_eq!(id.detail, "MicrosoftGame.config");
            assert_eq!(
                id.art,
                [ArtLocator::File {
                    rel: PathBuf::from("LargeLogo.png"),
                    format: ArtFormat::Png
                }]
            );
            // A lower-case title id is upper-cased; `FFFFFFFF` is no id.
            let lower = gdk(
                &tmp.path().join("lower"),
                &t::GdkSpec {
                    title_id: "6b9d1a2c",
                    ..Default::default()
                },
            );
            assert_eq!(lower[0].id.title_id.as_deref(), Some("6B9D1A2C"));
            for none in ["FFFFFFFF", "ffffffff", "6B9D1A2", "XYZ"] {
                let found = gdk(
                    &tmp.path().join(format!("t{none}")),
                    &t::GdkSpec {
                        title_id: none,
                        ..Default::default()
                    },
                );
                assert_eq!(found[0].id.title_id, None, "{none}");
            }
        }

        #[test]
        fn gdk_dlc_parent_needs_exactly_one_allowed_product() {
            let tmp = tempfile::TempDir::new().unwrap();
            for (i, attr) in [false, true].into_iter().enumerate() {
                let one = gdk(
                    &tmp.path().join(format!("one{i}")),
                    &t::GdkSpec {
                        families: &[],
                        dlc_family: Some("Scarlett"),
                        dlc_attr: attr,
                        allowed: &["9NBASEGAME01"],
                        store_id: "9NDLCPACK001",
                        ..Default::default()
                    },
                );
                assert_eq!(systems(&one), [("xboxseries", 0.95, TitleKind::Dlc)]);
                assert_eq!(one[0].id.parent_id.as_deref(), Some("9NBASEGAME01"));
                assert_eq!(one[0].id.product_id.as_deref(), Some("9NDLCPACK001"));
            }
            let two = gdk(
                &tmp.path().join("two"),
                &t::GdkSpec {
                    families: &[],
                    dlc_family: Some("XboxOne"),
                    allowed: &["9NBASEGAME01", "9NBASEGAME02"],
                    ..Default::default()
                },
            );
            assert_eq!(two[0].id.kind, TitleKind::Dlc);
            assert_eq!(two[0].id.parent_id, None);
        }

        /// `XboxCompatibility` is never read for the system (D18).
        #[test]
        fn xbox_compatibility_scarlett_stays_xbox_one() {
            let tmp = tempfile::TempDir::new().unwrap();
            let found = gdk(
                &tmp.path().join("bc"),
                &t::GdkSpec {
                    families: &["XboxOne"],
                    compat: Some("Scarlett"),
                    ..Default::default()
                },
            );
            assert_eq!(systems(&found), [("xboxone", 0.95, TitleKind::Base)]);
        }

        #[test]
        fn gdk_ms_resource_name_falls_back_to_the_folder() {
            let tmp = tempfile::TempDir::new().unwrap();
            let found = gdk(
                &tmp.path().join("Gears 5"),
                &t::GdkSpec {
                    display_name: "ms-resource:ApplicationDisplayName",
                    ..Default::default()
                },
            );
            assert_eq!(found[0].id.title.as_deref(), Some("Gears 5"));
        }

        #[test]
        fn gdk_encodings() {
            let tmp = tempfile::TempDir::new().unwrap();
            let text = t::gdk_config(&t::GdkSpec::default());
            // UTF-16 files declare it, as Visual Studio saves them.
            let text16 = text.replacen("encoding=\"utf-8\"", "encoding=\"utf-16\"", 1);
            assert_ne!(text16, text);
            let units: Vec<u16> = text16.encode_utf16().collect();
            let le: Vec<u8> = [0xFF, 0xFE]
                .into_iter()
                .chain(units.iter().flat_map(|u| u.to_le_bytes()))
                .collect();
            let be: Vec<u8> = [0xFE, 0xFF]
                .into_iter()
                .chain(units.iter().flat_map(|u| u.to_be_bytes()))
                .collect();
            let bare: Vec<u8> = units.iter().flat_map(|u| u.to_le_bytes()).collect();
            let bom8: Vec<u8> = [0xEF, 0xBB, 0xBF].into_iter().chain(text.bytes()).collect();
            let doctype = text.replacen("<Game ", "<!DOCTYPE Game [<!ENTITY e \"x\">]>\n<Game ", 1);
            for (name, bytes, hit) in [
                ("le", le, true),
                ("be", be, true),
                ("bom8", bom8, true),
                ("bare16", bare, false),
                ("doctype", doctype.into_bytes(), false),
                (
                    "other-root",
                    text.replace("Game", "Gamex").into_bytes(),
                    false,
                ),
            ] {
                let dir = tmp.path().join(name);
                std::fs::create_dir(&dir).unwrap();
                std::fs::write(dir.join("MicrosoftGame.config"), bytes).unwrap();
                let names = listing_of(&dir);
                let found = probe_gdk(&DirListing {
                    dir: &dir,
                    names: &names,
                });
                assert_eq!(!found.is_empty(), hit, "{name}");
                if hit {
                    assert_eq!(found[0].id.system, "xboxone", "{name}");
                }
            }
        }

        /// A config over 64 KiB is never parsed (spec §8.4): the folder is
        /// left as unreadable.
        #[test]
        fn oversized_gdk_config_is_not_parsed() {
            let tmp = tempfile::TempDir::new().unwrap();
            let dir = tmp.path().join("big");
            std::fs::create_dir(&dir).unwrap();
            let mut text = t::gdk_config(&t::GdkSpec::default());
            text.push_str(&format!("<!--{}-->", "x".repeat(MAX_META_FILE as usize)));
            assert_eq!(manifest_text(text.as_bytes()), None);
            std::fs::write(dir.join(GDK_CONFIG), text).unwrap();
            let names = listing_of(&dir);
            let found = probe_gdk(&DirListing {
                dir: &dir,
                names: &names,
            });
            assert_eq!(found.len(), 1);
            assert_eq!(found[0].leave, Some(FolderLeave::Unreadable));
        }

        #[test]
        fn gdk_config_name_is_case_insensitive() {
            let tmp = tempfile::TempDir::new().unwrap();
            let dir = tmp.path().join("lower");
            std::fs::create_dir(&dir).unwrap();
            let text = t::gdk_config(&t::GdkSpec::default());
            std::fs::write(dir.join("microsoftgame.config"), text).unwrap();
            let names = listing_of(&dir);
            let found = probe_gdk(&DirListing {
                dir: &dir,
                names: &names,
            });
            assert_eq!(found[0].probe, Path::new("microsoftgame.config"));
        }

        /// A config the probe cannot open leaves the folder `Unreadable`.
        #[cfg(unix)]
        #[test]
        fn unreadable_gdk_config_leaves_the_folder() {
            use std::os::unix::fs::PermissionsExt;
            let tmp = tempfile::TempDir::new().unwrap();
            let dir = tmp.path().join("XboxGames");
            t::gdk_folder(&dir, &t::GdkSpec::default());
            let config = dir.join("MicrosoftGame.config");
            std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o000)).unwrap();
            struct Restore<'a>(&'a Path);
            impl Drop for Restore<'_> {
                fn drop(&mut self) {
                    let _ =
                        std::fs::set_permissions(self.0, std::fs::Permissions::from_mode(0o644));
                }
            }
            let _restore = Restore(&config);
            if std::fs::File::open(&config).is_ok() {
                return; // Running as root: permissions do not apply.
            }
            let names = listing_of(&dir);
            let found = probe_gdk(&DirListing {
                dir: &dir,
                names: &names,
            });
            assert_eq!(found.len(), 1);
            assert_eq!(found[0].leave, Some(FolderLeave::Unreadable));
            assert_eq!(found[0].id.system, "xboxone");
        }

        fn era(dir: &Path, manifest: &str) -> Option<FolderMatch> {
            t::era_folder(dir, manifest);
            probe(dir, probe_era)
        }

        #[test]
        fn era_needs_the_xbox_namespace() {
            let tmp = tempfile::TempDir::new().unwrap();
            let m = era(
                &tmp.path().join("Sunset Overdrive"),
                &t::era_manifest("Microsoft.Sunset", "Sunset Overdrive", "Sunset", true),
            )
            .unwrap();
            assert_eq!(
                (m.id.system.as_str(), m.id.kind, m.id.confidence),
                ("xboxone", TitleKind::Base, 0.95)
            );
            assert_eq!(m.probe, Path::new("AppxManifest.xml"));
            assert_eq!(m.id.product_id.as_deref(), Some("Microsoft.Sunset"));
            assert_eq!(m.id.version.as_deref(), Some("1.0.4.0"));
            assert_eq!(m.id.title.as_deref(), Some("Sunset Overdrive"));
            assert_eq!(m.id.id_source, Some(IdSource::AppxManifest));
            assert_eq!(m.id.detail, "AppxManifest.xml");
            // VisualElements/@Logo first, then Properties/Logo.
            assert_eq!(
                m.id.art,
                [ArtLocator::File {
                    rel: PathBuf::from("Logo.png"),
                    format: ArtFormat::Png
                }]
            );
            // A Windows app manifest: no Xbox namespace, no game.
            let windows = era(
                &tmp.path().join("Calculator"),
                &t::era_manifest("Microsoft.Calc", "Calculator", "Calculator", false),
            );
            assert_eq!(windows, None);
            // The namespace under the default prefix counts too.
            let default_ns = t::era_manifest("Microsoft.X", "X", "X", false).replace(
                "http://schemas.microsoft.com/appx/2010/manifest",
                XBOX_MANIFEST_NS,
            );
            assert!(era(&tmp.path().join("Default"), &default_ns).is_some());
        }

        #[test]
        fn era_ms_resource_chain() {
            let tmp = tempfile::TempDir::new().unwrap();
            let visual = era(
                &tmp.path().join("a"),
                &t::era_manifest("A.B", "ms-resource:Name", "Visual Name", true),
            )
            .unwrap();
            assert_eq!(visual.id.title.as_deref(), Some("Visual Name"));
            let folder = era(
                &tmp.path().join("Ryse Son of Rome"),
                &t::era_manifest("A.B", "ms-resource:Name", "ms-resource:Other", true),
            )
            .unwrap();
            assert_eq!(folder.id.title.as_deref(), Some("Ryse Son of Rome"));
        }

        /// A folder with a GDK config is the GDK probe's: the ERA probe
        /// stays out so one folder never conflicts with itself.
        #[test]
        fn era_skips_a_gdk_folder() {
            let tmp = tempfile::TempDir::new().unwrap();
            let dir = tmp.path().join("Both");
            t::gdk_folder(
                &dir,
                &t::GdkSpec {
                    families: &["Scarlett"],
                    ..Default::default()
                },
            );
            t::put_file(
                &dir,
                "AppxManifest.xml",
                t::era_manifest("A.B", "Both", "Both", true).as_bytes(),
            );
            assert_eq!(probe(&dir, probe_era), None);
        }

        fn locate(dir: &Path, rel: &str) -> Option<PathBuf> {
            let names = listing_of(dir);
            let l = DirListing { dir, names: &names };
            let mut listed = false;
            match logo_locator(&l, rel, &mut listed)? {
                ArtLocator::File { rel, format } => {
                    assert_eq!(format, ArtFormat::Png);
                    Some(rel)
                }
                other => panic!("{other:?}"),
            }
        }

        #[test]
        fn logo_paths_stay_inside_the_folder() {
            let tmp = tempfile::TempDir::new().unwrap();
            let dir = tmp.path().join("game");
            t::put_file(&dir, "Assets/Logo.png", &t::png_8x8());
            t::put_file(tmp.path(), "secret.png", &t::png_8x8());
            assert_eq!(
                locate(&dir, "Assets\\Logo.png"),
                Some(Path::new("Assets").join("Logo.png"))
            );
            assert_eq!(
                locate(&dir, "Assets/Logo.png"),
                Some(Path::new("Assets").join("Logo.png"))
            );
            for bad in [
                "..\\secret.png",
                "..\\..\\secret.png",
                "Assets\\..\\..\\secret.png",
                "C:\\x.png",
                "\\x.png",
                "/x.png",
                "",
                ".\\Assets\\Logo.png",
                "a\\b\\c\\d\\e\\f\\g\\h\\i.png",
                "Assets\\Logo.jpg",
            ] {
                assert_eq!(locate(&dir, bad), None, "{bad}");
            }
        }

        #[test]
        fn logo_mrt_scale_picks_the_largest() {
            let tmp = tempfile::TempDir::new().unwrap();
            let dir = tmp.path().join("game");
            for name in [
                "Logo.scale-100.png",
                "Logo.scale-200.png",
                "Logo.scale-150.png",
                "Logo.scale-x.png",
                "OtherLogo.scale-400.png",
            ] {
                t::put_file(&dir, &format!("Assets/{name}"), &t::png_8x8());
            }
            assert_eq!(
                locate(&dir, "Assets\\Logo.png"),
                Some(Path::new("Assets").join("Logo.scale-200.png"))
            );
            // At the root the folder's own names are used, no listing.
            t::put_file(&dir, "Store.scale-125.png", &t::png_8x8());
            let names = listing_of(&dir);
            let l = DirListing {
                dir: &dir,
                names: &names,
            };
            let mut listed = true;
            assert_eq!(
                logo_locator(&l, "Store.png", &mut listed),
                Some(ArtLocator::File {
                    rel: PathBuf::from("Store.scale-125.png"),
                    format: ArtFormat::Png
                })
            );
            // One listing per probe: a second subfolder is not listed.
            assert_eq!(logo_locator(&l, "Assets\\Logo.png", &mut listed), None);
        }

        /// D21 budget: a GDK probe lists at most one subfolder for its
        /// logos, however many of them need an MRT lookup.
        #[test]
        fn gdk_logos_cost_at_most_one_listing() {
            let tmp = tempfile::TempDir::new().unwrap();
            let dir = tmp.path().join("g");
            t::gdk_folder(
                &dir,
                &t::GdkSpec {
                    logos: ["A\\Big.png", "B\\Store.png", "C\\Small.png"],
                    ..Default::default()
                },
            );
            for (from, to) in [
                ("A/Big.png", "A/Big.scale-50.txt"),
                ("B/Store.png", "B/Store.txt"),
                ("C/Small.png", "C/Small.scale-200.png"),
            ] {
                std::fs::rename(dir.join(from), dir.join(to)).unwrap();
            }
            let names = listing_of(&dir);
            crate::consoles::wiiu::list_count::take();
            let found = probe_gdk(&DirListing {
                dir: &dir,
                names: &names,
            });
            assert_eq!(crate::consoles::wiiu::list_count::take(), 1);
            // The one listing went to `A`; `C`'s MRT name is not looked up.
            assert_eq!(found[0].id.art, []);
        }

        /// A GDK folder whose first logo is missing takes the next one.
        #[test]
        fn gdk_logo_order() {
            let tmp = tempfile::TempDir::new().unwrap();
            let dir = tmp.path().join("g");
            let found = gdk(
                &dir,
                &t::GdkSpec {
                    logos: ["Assets\\Big.png", "Assets\\Store.png", "Small.png"],
                    ..Default::default()
                },
            );
            assert_eq!(
                found[0].id.art,
                [ArtLocator::File {
                    rel: Path::new("Assets").join("Big.png"),
                    format: ArtFormat::Png
                }]
            );
            std::fs::remove_file(dir.join("Assets").join("Big.png")).unwrap();
            std::fs::remove_file(dir.join("Assets").join("Store.png")).unwrap();
            let names = listing_of(&dir);
            let again = probe_gdk(&DirListing {
                dir: &dir,
                names: &names,
            });
            assert_eq!(
                again[0].id.art,
                [ArtLocator::File {
                    rel: PathBuf::from("Small.png"),
                    format: ArtFormat::Png
                }]
            );
        }

        /// A symlinked logo, or a logo behind a symlinked folder, is
        /// dropped (spec §8.4).
        #[cfg(unix)]
        #[test]
        fn symlinked_logos_are_dropped() {
            let tmp = tempfile::TempDir::new().unwrap();
            let dir = tmp.path().join("game");
            t::put_file(tmp.path(), "outside/Logo.png", &t::png_8x8());
            std::fs::create_dir_all(&dir).unwrap();
            std::os::unix::fs::symlink(tmp.path().join("outside"), dir.join("Assets")).unwrap();
            std::os::unix::fs::symlink(
                tmp.path().join("outside").join("Logo.png"),
                dir.join("Logo.png"),
            )
            .unwrap();
            assert_eq!(locate(&dir, "Assets\\Logo.png"), None);
            assert_eq!(locate(&dir, "Logo.png"), None);
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
