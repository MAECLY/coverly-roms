//! PS3 and PS Vita (spec §8.4): JB, HDD/PSN and Vita app folders, PS3
//! disc images, PKG packages (PS3 and Vita), VPK packages and the
//! `.psvita`/`.psn` launcher files of ES-DE and Batocera.
//!
//! Sources: psdevwiki «PARAM.SFO», «PKG files» (magic, revision and type
//! at 4 and 6, content id at 0x30), «Bluray disc» (`PS3VOLUME` ISO 9660
//! volume); the Batocera `es_systems.yml` and ES-DE `es_systems.xml`
//! extension lists for the launcher files.
//!
//! Only plaintext is read: `PARAM.SFO` (through [`super::psf`]), the
//! ISO 9660 directories and the first 0x60 bytes of a PKG. A PKG body,
//! `.rap`/`.rif`/zRIF licences, `sce_sys/package/work.bin` and the
//! encrypted regions of a disc are never read (D4).
//!
//! Not done (BLOCKED): the plaintext disc id at sector 1 of an encrypted
//! Redump PS3 image (`PlayStation3` at 0x800, the title id at 0x810) and
//! the `PS3_DISC.SFB` fields could not be checked against a real head,
//! so an image whose `PARAM.SFO` is not in clear is a magic-only `ps3`
//! verdict and the name decides the rest.

use std::io;
use std::path::PathBuf;

use super::psf::{self, Psf};
use super::wiiu::{find, list_child, plain_file, read_head};
use super::{
    ArtFormat, ArtLocator, Budgeted, ConsoleId, DirListing, FolderMatch, IdSource, TitleKind,
    MAX_META_FILE,
};
use crate::disc::iso9660::{self, FileExtent};
use crate::disc::source::{SectorLayout, SectorSource, USER_BYTES};

/// The `detail` of a PKG verdict; the plan notes it `InstallFirst`.
pub const PKG_DETAIL: &str = "PKG header";
const PKG_MAGIC: [u8; 4] = [0x7F, b'P', b'K', b'G'];
const PKG_CONTENT_ID: usize = 0x30;
const PKG_CONTENT_ID_LEN: usize = 0x24;
/// The volume (and system) id of a PS3 disc's ISO 9660 descriptor.
const PS3_VOLUME: &str = "PS3VOLUME";
/// Longest launcher file whose text is read as a title id (D14).
const LAUNCHER_MAX: usize = 64;

/// A Sony title id as stored: nine ASCII letters and digits, upper-cased
/// (`BLUS30443`, `PCSE00844`; homebrew uses any nine).
fn title_id(raw: &str) -> Option<String> {
    let t = raw.trim();
    (t.len() == 9 && t.bytes().all(|b| b.is_ascii_alphanumeric())).then(|| t.to_ascii_uppercase())
}

/// A retail title id: four letters and five digits.
fn retail_id(t: &str) -> bool {
    let b = t.as_bytes();
    b.len() == 9
        && b[..4].iter().all(u8::is_ascii_alphabetic)
        && b[4..].iter().all(u8::is_ascii_digit)
}

/// PS4 titles share `sce_sys/param.sfo` and the `gd`/`gp` categories
/// with the Vita; their `CUSA` ids are left for Tanda 3.
fn is_ps4(title_id: &str) -> bool {
    title_id.starts_with("CUSA")
}

/// A [`ConsoleId`] from a parsed `PARAM.SFO` (spec §8.4): `TITLE_ID` as
/// the title id and (in the DAT's form) the serial, `TITLE` with its line
/// breaks folded, `APP_VER` else `VERSION`, and the kind by `CATEGORY`
/// (PS3 `DG`/`HG` base, `GD` update; Vita `gd` base, `gp` update).
pub fn from_psf(psf: &Psf, system: &str, detail: &str) -> ConsoleId {
    let mut id = ConsoleId::proven(system, detail);
    id.id_source = Some(IdSource::ParamSfo);
    if let Some(tid) = psf.text("TITLE_ID").and_then(title_id) {
        id.serial = crate::disc::serial::normalise_console_serial(system, &tid);
        id.title_id = Some(tid);
    }
    id.title = psf
        .text("TITLE")
        .map(|t| t.split_whitespace().collect::<Vec<_>>().join(" "))
        .and_then(|t| super::clean_title(&t));
    id.version = psf
        .text("APP_VER")
        .or_else(|| psf.text("VERSION"))
        .and_then(super::clean_title);
    id.kind = match (system, psf.text("CATEGORY")) {
        ("ps3", Some("DG" | "HG")) | ("psvita", Some("gd")) => TitleKind::Base,
        ("ps3", Some("GD")) | ("psvita", Some("gp")) => TitleKind::Update,
        _ => TitleKind::Unknown,
    };
    id
}

/// Reads and parses the `PARAM.SFO` at `rel` inside the probed folder
/// (a regular file of at most [`MAX_META_FILE`] bytes, never a link).
fn folder_psf(l: &DirListing, rel: &std::path::Path) -> Option<Psf> {
    let path = l.dir.join(rel);
    let len = plain_file(&path, MAX_META_FILE)?;
    psf::parse(&read_head(&path, len, MAX_META_FILE as usize)?)
}

/// The art locator of `name` among `names` under `dir` (relative), when
/// it is a regular file.
fn icon(l: &DirListing, dir: Option<&str>, names: &[String], name: &str) -> Option<ArtLocator> {
    let found = find(names, name)?;
    let rel = match dir {
        Some(d) => PathBuf::from(d).join(found),
        None => PathBuf::from(found),
    };
    plain_file(&l.dir.join(&rel), u64::MAX)?;
    Some(ArtLocator::File {
        rel,
        format: ArtFormat::Png,
    })
}

/// A PS3 JB (disc) folder (spec D12): `PS3_GAME` among its names, a
/// real directory whose one listing holds `PARAM.SFO`; the icon is
/// `PS3_GAME/ICON0.PNG`.
pub fn probe_ps3_jb(l: &DirListing) -> Option<FolderMatch> {
    let game = find(l.names, "PS3_GAME")?;
    let names = list_child(&l.dir.join(game))?;
    let probe = PathBuf::from(game).join(find(&names, "PARAM.SFO")?);
    let psf = folder_psf(l, &probe)?;
    let mut id = from_psf(&psf, "ps3", "PARAM.SFO");
    id.art.extend(icon(l, Some(game), &names, "ICON0.PNG"));
    Some(FolderMatch { id, probe })
}

/// A PS3 HDD/PSN title folder (spec D12): `PARAM.SFO` and a `USRDIR`
/// directory at its root; the icon is `ICON0.PNG`. The `PARAM.SFO` must
/// carry a `TITLE_ID`: the `PSP_GAME/` folder of an extracted PSP UMD has
/// the same two names, but its `PARAM.SFO` has `DISC_ID` instead
/// (psdevwiki «PARAM.SFO»).
pub fn probe_ps3_hdd(l: &DirListing) -> Option<FolderMatch> {
    let usrdir = find(l.names, "USRDIR")?;
    if !std::fs::symlink_metadata(l.dir.join(usrdir)).ok()?.is_dir() {
        return None;
    }
    let probe = PathBuf::from(find(l.names, "PARAM.SFO")?);
    let psf = folder_psf(l, &probe)?;
    let mut id = from_psf(&psf, "ps3", "HDD PARAM.SFO");
    id.title_id.as_ref()?;
    id.art.extend(icon(l, None, l.names, "ICON0.PNG"));
    Some(FolderMatch { id, probe })
}

/// A decrypted PS Vita app folder (spec D12): `sce_sys` among its names,
/// a real directory whose one listing holds `param.sfo`; the icon is
/// `sce_sys/icon0.png`. `sce_sys/package/work.bin` (a licence) moves
/// with the folder and is never read.
pub fn probe_vita_app(l: &DirListing) -> Option<FolderMatch> {
    let sys = find(l.names, "sce_sys")?;
    let names = list_child(&l.dir.join(sys))?;
    let probe = PathBuf::from(sys).join(find(&names, "param.sfo")?);
    let psf = folder_psf(l, &probe)?;
    let id = from_psf(&psf, "psvita", "sce_sys/param.sfo");
    if id.title_id.as_deref().is_some_and(is_ps4) {
        return None;
    }
    let mut id = id;
    id.art.extend(icon(l, Some(sys), &names, "icon0.png"));
    Some(FolderMatch { id, probe })
}

/// The `param.sfo` of a VPK (a zip read by `organize::inspect`, never
/// extracted, D16), with the icon's entry name when there is one.
pub fn from_vpk(sfo: &[u8], icon_entry: Option<&str>) -> Option<ConsoleId> {
    let id = from_psf(&psf::parse(sfo)?, "psvita", "VPK param.sfo");
    if id.title_id.as_deref().is_some_and(is_ps4) {
        return None;
    }
    let mut id = id;
    id.art.extend(icon_entry.map(|name| ArtLocator::ZipEntry {
        name: name.to_owned(),
        format: ArtFormat::Png,
    }));
    Some(id)
}

/// A `.psvita` (Vita3K) or `.psn` (RPCS3) launcher file whose whole text
/// is a retail title id (D14): at most [`LAUNCHER_MAX`] bytes, the system
/// by the extension. Coverly reads these, it never writes them.
pub fn launcher(head: &[u8], ext: &str) -> Option<ConsoleId> {
    let system = match ext {
        "psvita" => "psvita",
        "psn" => "ps3",
        _ => return None,
    };
    if head.len() > LAUNCHER_MAX {
        return None;
    }
    let tid = title_id(
        std::str::from_utf8(head)
            .ok()?
            .trim_start_matches('\u{feff}'),
    )
    .filter(|t| retail_id(t))?;
    let mut id = ConsoleId::magic(system, "launcher file");
    id.serial = crate::disc::serial::normalise_console_serial(system, &tid);
    id.title_id = Some(tid);
    id.id_source = Some(IdSource::FileName);
    Some(id)
}

/// A PS3 or Vita file by its own header: a PKG under any extension, a
/// PS3 disc image under `.iso`.
pub fn identify(b: &mut Budgeted, ext: &str) -> Option<ConsoleId> {
    if b.len() < 8 {
        return None;
    }
    let magic = b.read(0, 4).ok()?;
    if magic == PKG_MAGIC {
        return pkg(b);
    }
    if ext == "iso" {
        return ps3_iso(b);
    }
    None
}

/// A PKG (psdevwiki «PKG files»): type 1 PS3, type 2 PSP or Vita; the
/// title id is `content_id[7..16]` (`UP0001-NPUB30001_00-…`). A type 2
/// package whose title id is not a Vita one (`PCS…`, `NPXS…`), or has no
/// readable title id at all, may be a PSP or PS1 package: it names no
/// system. The body is never read.
fn pkg(b: &mut Budgeted) -> Option<ConsoleId> {
    let h = b.read(0, PKG_CONTENT_ID + PKG_CONTENT_ID_LEN).ok()?;
    let kind = u16::from_be_bytes([h[6], h[7]]);
    let content = &h[PKG_CONTENT_ID..];
    let tid = std::str::from_utf8(content.get(7..16)?)
        .ok()
        .filter(|t| retail_id(t))
        .map(str::to_ascii_uppercase);
    let system = match kind {
        1 => "ps3",
        2 => match &tid {
            Some(t) if t.starts_with("PCS") || t.starts_with("NPXS") => "psvita",
            _ => return None,
        },
        _ => return None,
    };
    let Some(tid) = tid else {
        return Some(ConsoleId::magic(system, PKG_DETAIL));
    };
    let mut id = ConsoleId::proven(system, PKG_DETAIL);
    id.serial = crate::disc::serial::normalise_console_serial(system, &tid);
    id.title_id = Some(tid);
    id.id_source = Some(IdSource::Header);
    Some(id)
}

/// 2048-byte sectors of a disc image through the item's budget.
struct Sectors<'a, 'b>(&'a mut Budgeted<'b>);

impl SectorSource for Sectors<'_, '_> {
    fn layout(&self) -> SectorLayout {
        SectorLayout::Iso2048
    }

    fn read_user(&mut self, lba: u32, out: &mut [u8; USER_BYTES]) -> io::Result<bool> {
        let at = u64::from(lba) * USER_BYTES as u64;
        match self.0.read(at, USER_BYTES) {
            Ok(bytes) => {
                out.copy_from_slice(&bytes);
                Ok(true)
            }
            Err(_) => Ok(false),
        }
    }

    fn raw_sector(&mut self, _: u32) -> io::Result<Option<Vec<u8>>> {
        Ok(None)
    }
}

/// A PS3 disc image: ISO 9660 with `PS3VOLUME`; `PS3_GAME/PARAM.SFO`
/// is read through one listing of the root and one of `PS3_GAME` (each at
/// most `MAX_ROOT_RECORDS` records). A `PARAM.SFO` that is not in clear
/// (an encrypted region) leaves the magic-only verdict. The icon is the
/// byte range of `PS3_GAME/ICON0.PNG`.
fn ps3_iso(b: &mut Budgeted) -> Option<ConsoleId> {
    let len = b.len();
    let mut src = Sectors(b);
    let pvd = iso9660::read_pvd(&mut src).ok()??;
    if pvd.volume_id != PS3_VOLUME && pvd.system_id != PS3_VOLUME {
        return None;
    }
    let magic = ConsoleId::magic("ps3", PS3_VOLUME);
    let Ok(root) = iso9660::root_records(&mut src, &pvd.root) else {
        return Some(magic);
    };
    let Some(game) = iso9660::dir_named(&root, "PS3_GAME") else {
        return Some(magic);
    };
    let Ok(records) = iso9660::root_records(&mut src, game) else {
        return Some(magic);
    };
    let Some(param) = iso9660::file_named(&records, "PARAM.SFO")
        .and_then(FileExtent::of)
        .filter(|e| u64::from(e.size) <= MAX_META_FILE)
    else {
        return Some(magic);
    };
    let Some(psf) = iso9660::read_extent(&mut src, param, MAX_META_FILE as usize)
        .ok()
        .flatten()
        .and_then(|bytes| psf::parse(&bytes))
    else {
        return Some(magic);
    };
    let mut id = from_psf(&psf, "ps3", "PS3 ISO PARAM.SFO");
    let icon = iso9660::file_named(&records, "ICON0.PNG").and_then(FileExtent::of);
    if let Some(e) = icon {
        let offset = u64::from(e.lba) * USER_BYTES as u64;
        let size = u64::from(e.size);
        if size > 0 && offset.checked_add(size).is_some_and(|end| end <= len) {
            id.art.push(ArtLocator::Range {
                offset,
                len: size,
                format: ArtFormat::Png,
            });
        }
    }
    Some(id)
}

#[cfg(test)]
mod tests {
    use super::super::testing::{self, RangeLog, SparseAt};
    use super::*;

    fn run(f: &mut SparseAt, ext: &str) -> Option<ConsoleId> {
        let len = f.len;
        let mut b = Budgeted::new(f, len);
        let id = identify(&mut b, ext);
        let used = b.used();
        assert!(used.read <= super::super::MAX_READ && used.seeks <= super::super::MAX_SEEKS);
        id
    }

    #[test]
    fn from_psf_reads_ids_title_version_and_kind() {
        let psf = psf::parse(&testing::ps3_sfo("BLUS30443", "DG")).unwrap();
        let id = from_psf(&psf, "ps3", "PARAM.SFO");
        assert_eq!(id.serial.as_deref(), Some("BLUS-30443"));
        assert_eq!(id.title_id.as_deref(), Some("BLUS30443"));
        assert_eq!(id.title.as_deref(), Some("Test Game"));
        assert_eq!(id.version.as_deref(), Some("01.00"));
        assert_eq!(id.kind, TitleKind::Base);
        assert_eq!(id.id_source, Some(IdSource::ParamSfo));
        assert_eq!(id.confidence, 0.95);
        let update = psf::parse(&testing::ps3_sfo("BLUS30443", "GD")).unwrap();
        assert_eq!(from_psf(&update, "ps3", "x").kind, TitleKind::Update);
        let hdd = psf::parse(&testing::ps3_sfo("NPUB30024", "HG")).unwrap();
        assert_eq!(from_psf(&hdd, "ps3", "x").kind, TitleKind::Base);
        let vita = psf::parse(&testing::vita_sfo("PCSE00844")).unwrap();
        let v = from_psf(&vita, "psvita", "x");
        assert_eq!(
            (v.serial.as_deref(), v.kind, v.title.as_deref()),
            (Some("PCSE-00844"), TitleKind::Base, Some("Test Game"))
        );
        // A Vita category means nothing on PS3.
        assert_eq!(from_psf(&vita, "ps3", "x").kind, TitleKind::Unknown);
        // A homebrew id: a title id, no DAT serial.
        let hb = psf::parse(&testing::param_sfo(&[("TITLE_ID", "VITASHELL")])).unwrap();
        let h = from_psf(&hb, "psvita", "x");
        assert_eq!((h.title_id.as_deref(), h.serial), (Some("VITASHELL"), None));
    }

    #[test]
    fn pkg_ps3_and_vita() {
        let mut ps3 = testing::pkg(1, "UP0001-NPUB30024_00-0000000000000001");
        let id = run(&mut ps3, "pkg").unwrap();
        assert_eq!(
            (id.system.as_str(), id.serial.as_deref(), id.detail.as_str()),
            ("ps3", Some("NPUB-30024"), PKG_DETAIL)
        );
        let mut vita = testing::pkg(2, "EP0001-PCSB00245_00-0000000000000000");
        let id = run(&mut vita, "pkg").unwrap();
        assert_eq!(
            (id.system.as_str(), id.title_id.as_deref()),
            ("psvita", Some("PCSB00245"))
        );
        // A PSP package (type 2, NPUZ minis id) is not this batch's.
        let mut psp = testing::pkg(2, "UP0001-NPUZ00001_00-0000000000000000");
        assert_eq!(run(&mut psp, "pkg"), None);
        // An unreadable content id is the magic alone.
        let mut odd = testing::pkg(1, "garbage");
        let id = run(&mut odd, "pkg").unwrap();
        assert_eq!((id.confidence, id.serial), (0.90, None));
        // Type 2 is PSP or Vita: without a readable id it names neither.
        let mut odd = testing::pkg(2, "garbage");
        assert_eq!(run(&mut odd, "pkg"), None);
        // An unknown type is nothing.
        let mut other = testing::pkg(9, "UP0001-NPUB30024_00-0000000000000001");
        assert_eq!(run(&mut other, "pkg"), None);
    }

    #[test]
    fn pkg_body_is_never_read() {
        let mut f = testing::pkg(1, "UP0001-NPUB30024_00-0000000000000001");
        let len = f.len;
        let mut log = RangeLog::new(&mut f);
        assert!(identify(&mut Budgeted::new(&mut log, len), "pkg").is_some());
        assert!(!log.touched(0xC0, len - 0xC0), "{:?}", log.ranges);
    }

    #[test]
    fn ps3_iso_reads_param_sfo_through_two_listings() {
        let mut f = testing::ps3_iso("BLUS30443");
        let id = run(&mut f, "iso").unwrap();
        assert_eq!(id.system, "ps3");
        assert_eq!(id.serial.as_deref(), Some("BLUS-30443"));
        assert_eq!(id.detail, "PS3 ISO PARAM.SFO");
        assert_eq!(
            id.art,
            [ArtLocator::Range {
                offset: u64::from(testing::PS3_ISO_LBAS[4]) * 2048,
                len: 4,
                format: ArtFormat::Png,
            }]
        );
        // Under another extension the image is not looked at.
        let mut f = testing::ps3_iso("BLUS30443");
        assert_eq!(run(&mut f, "bin"), None);
    }

    #[test]
    fn ps3_iso_with_param_sfo_not_in_clear_is_magic_only() {
        let mut f = testing::ps3_iso_with(&[0xA7; 600]);
        let id = run(&mut f, "iso").unwrap();
        assert_eq!(
            (
                id.system.as_str(),
                id.confidence,
                id.serial,
                id.detail.as_str()
            ),
            ("ps3", 0.90, None, PS3_VOLUME)
        );
    }

    #[test]
    fn another_iso_is_not_ps3() {
        let mut f = SparseAt::new(1 << 20);
        let mut pvd = vec![0u8; 2048];
        pvd[0] = 1;
        pvd[1..6].copy_from_slice(b"CD001");
        pvd[8..72].fill(b' ');
        pvd[40..49].copy_from_slice(b"SOMEDISC1");
        f.put(16 * 2048, &pvd);
        assert_eq!(run(&mut f, "iso"), None);
    }

    #[test]
    fn launcher_text_is_a_title_id() {
        let id = launcher(b"PCSB00245\n", "psvita").unwrap();
        assert_eq!(
            (
                id.system.as_str(),
                id.title_id.as_deref(),
                id.serial.as_deref()
            ),
            ("psvita", Some("PCSB00245"), Some("PCSB-00245"))
        );
        assert_eq!(launcher(b"NPUB30024", "psn").unwrap().system, "ps3");
        assert_eq!(launcher(b"hello there", "psvita"), None);
        assert_eq!(launcher(b"PCSB00245", "txt"), None);
        assert_eq!(launcher(&[b' '; 65], "psn"), None);
    }

    #[test]
    fn vpk_param_sfo_and_ps4_ids() {
        let id = from_vpk(&testing::vita_sfo("PCSE00844"), Some("sce_sys/icon0.png")).unwrap();
        assert_eq!(id.detail, "VPK param.sfo");
        assert_eq!(id.serial.as_deref(), Some("PCSE-00844"));
        assert_eq!(
            id.art,
            [ArtLocator::ZipEntry {
                name: "sce_sys/icon0.png".into(),
                format: ArtFormat::Png
            }]
        );
        assert_eq!(from_vpk(&testing::vita_sfo("CUSA00001"), None), None);
        assert_eq!(from_vpk(b"not a psf", None), None);
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
        fn jb_folder() {
            let tmp = tempfile::TempDir::new().unwrap();
            let dir = tmp.path().join("Demon's Souls (BLUS-30443)");
            testing::ps3_jb(&dir, "BLUS30443", "DG");
            let m = probe(&dir, probe_ps3_jb).unwrap();
            assert_eq!(m.probe, Path::new("PS3_GAME").join("PARAM.SFO"));
            assert_eq!(m.id.serial.as_deref(), Some("BLUS-30443"));
            assert_eq!(
                m.id.art,
                [ArtLocator::File {
                    rel: Path::new("PS3_GAME").join("ICON0.PNG"),
                    format: ArtFormat::Png
                }]
            );
            // PS3_GAME holds PARAM.SFO and USRDIR too, but the JB probe
            // names the folder above it; the HDD probe sees no USRDIR here.
            assert_eq!(probe(&dir, probe_ps3_hdd), None);
            assert_eq!(probe(&dir, probe_vita_app), None);
        }

        #[test]
        fn hdd_folder() {
            let tmp = tempfile::TempDir::new().unwrap();
            let dir = tmp.path().join("NPUB30024");
            testing::ps3_hdd(&dir, "NPUB30024", "HG");
            let m = probe(&dir, probe_ps3_hdd).unwrap();
            assert_eq!(m.probe, Path::new("PARAM.SFO"));
            assert_eq!(
                (m.id.title_id.as_deref(), m.id.kind),
                (Some("NPUB30024"), TitleKind::Base)
            );
            assert_eq!(m.id.art.len(), 1);
            // A PARAM.SFO without USRDIR is not a title folder.
            std::fs::remove_dir_all(dir.join("USRDIR")).unwrap();
            assert_eq!(probe(&dir, probe_ps3_hdd), None);
        }

        #[test]
        fn an_extracted_psp_umd_folder_is_not_a_ps3_hdd_title() {
            // `PSP_GAME/` of an extracted UMD holds `PARAM.SFO` and
            // `USRDIR/` too, but a PSP `PARAM.SFO` has `DISC_ID`, no
            // `TITLE_ID` (psdevwiki «PARAM.SFO»): not a PS3 title.
            let tmp = tempfile::TempDir::new().unwrap();
            let dir = tmp.path().join("PSP_GAME");
            std::fs::create_dir_all(dir.join("USRDIR")).unwrap();
            let sfo = testing::param_sfo(&[
                ("CATEGORY", "UG"),
                ("DISC_ID", "ULUS10041"),
                ("TITLE", "Test Game"),
            ]);
            std::fs::write(dir.join("PARAM.SFO"), sfo).unwrap();
            std::fs::write(dir.join("USRDIR").join("DATA.BIN"), [0u8; 16]).unwrap();
            assert_eq!(probe(&dir, probe_ps3_hdd), None);
        }

        #[test]
        fn vita_folder() {
            let tmp = tempfile::TempDir::new().unwrap();
            let dir = tmp.path().join("PCSE00844");
            testing::vita_app(&dir, "PCSE00844");
            let m = probe(&dir, probe_vita_app).unwrap();
            assert_eq!(m.probe, Path::new("sce_sys").join("param.sfo"));
            assert_eq!(m.id.system, "psvita");
            assert_eq!(m.id.serial.as_deref(), Some("PCSE-00844"));
            assert_eq!(
                m.id.art,
                [ArtLocator::File {
                    rel: Path::new("sce_sys").join("icon0.png"),
                    format: ArtFormat::Png
                }]
            );
        }

        #[test]
        fn a_ps4_app_folder_is_not_a_vita_game() {
            let tmp = tempfile::TempDir::new().unwrap();
            let dir = tmp.path().join("CUSA00001");
            testing::vita_app(&dir, "CUSA00001");
            assert_eq!(probe(&dir, probe_vita_app), None);
        }

        #[test]
        fn a_folder_with_a_broken_param_sfo_is_not_a_game() {
            let tmp = tempfile::TempDir::new().unwrap();
            let dir = tmp.path().join("X");
            testing::ps3_jb(&dir, "BLUS30443", "DG");
            std::fs::write(dir.join("PS3_GAME").join("PARAM.SFO"), b"\0PSFbroken").unwrap();
            assert_eq!(probe(&dir, probe_ps3_jb), None);
        }

        #[cfg(unix)]
        #[test]
        fn a_symlinked_probe_path_is_no_match() {
            let tmp = tempfile::TempDir::new().unwrap();
            let real = tmp.path().join("real");
            testing::ps3_jb(&real, "BLUS30443", "DG");
            let dir = tmp.path().join("Linked");
            std::fs::create_dir(&dir).unwrap();
            std::os::unix::fs::symlink(real.join("PS3_GAME"), dir.join("PS3_GAME")).unwrap();
            assert_eq!(probe(&dir, probe_ps3_jb), None);
            let vita = tmp.path().join("Vita");
            testing::vita_app(&vita, "PCSE00844");
            let sfo = vita.join("sce_sys").join("param.sfo");
            std::fs::rename(&sfo, tmp.path().join("elsewhere.sfo")).unwrap();
            std::os::unix::fs::symlink(tmp.path().join("elsewhere.sfo"), &sfo).unwrap();
            assert_eq!(probe(&vita, probe_vita_app), None);
        }
    }
}
