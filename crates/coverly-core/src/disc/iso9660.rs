//! ISO 9660 as far as disc identification needs it, and the PlayStation
//! rules over it (spec §7).
//!
//! Only the primary volume descriptor (LBA 16) and the root directory are
//! read: at most [`MAX_ROOT_RECORDS`] records, depth 1, and a file is read
//! up to a caller cap. Every field comes from untrusted bytes, so every
//! offset is checked and nothing recurses.

use std::io;
use std::ops::Range;

use sha1::{Digest, Sha1};

use super::serial::boot_file_serial;
use super::source::{SectorLayout, SectorSource, USER_BYTES};
use super::DiscSystem;
use crate::dedupe::source::VolumeFingerprint;

/// Where the primary volume descriptor lives.
pub const PVD_LBA: u32 = 16;

/// Root directory records examined, `.` and `..` included.
pub const MAX_ROOT_RECORDS: usize = 64;

/// Largest `SYSTEM.CNF` read (spec §7).
pub const SYSTEM_CNF_CAP: usize = 4096;

/// Most sectors any CD holds (99 minutes at 75 sectors a second).
pub const CD_MAX_SECTORS: u32 = 99 * 60 * 75;

/// Largest CD image: [`CD_MAX_SECTORS`] raw 2352-byte sectors. A
/// `PLAYSTATION` volume on a larger image is a PS2 DVD.
pub const CD_MAX_BYTES: u64 = CD_MAX_SECTORS as u64 * 2352;

/// Sectors of root directory read at most: 64 records of at most 255
/// bytes each fit in 8 sectors (records never cross a sector).
const MAX_ROOT_SECTORS: u32 = (MAX_ROOT_RECORDS * 255).div_ceil(USER_BYTES) as u32;

/// Shortest valid directory record (33 fixed bytes and a 1-byte name).
const MIN_RECORD: usize = 34;

/// Directory record flag: the record is a directory.
const FLAG_DIR: u8 = 0x02;

/// One directory record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirRecord {
    /// First logical block of the extent (extended attributes included).
    pub lba: u32,
    /// Extent length in bytes.
    pub size: u32,
    /// Blocks of extended attribute record before the data.
    pub ext_attr: u8,
    /// File flags (0x02 = directory).
    pub flags: u8,
    /// The identifier as stored (`SYSTEM.CNF;1`; `\0` for `.`, `\x01` for
    /// `..`), decoded lossily.
    pub name: String,
}

impl DirRecord {
    pub fn is_dir(&self) -> bool {
        self.flags & FLAG_DIR != 0
    }

    /// One record from the start of `rec`; `None` when `rec` is too short
    /// for the record it states or the name runs past it.
    fn parse(rec: &[u8]) -> Option<DirRecord> {
        let len = usize::from(*rec.first()?);
        if len < MIN_RECORD || len > rec.len() {
            return None;
        }
        let rec = &rec[..len];
        let name_len = usize::from(rec[32]);
        let name = rec.get(33..33 + name_len)?;
        Some(DirRecord {
            lba: u32::from_le_bytes(rec[2..6].try_into().ok()?),
            size: u32::from_le_bytes(rec[10..14].try_into().ok()?),
            ext_attr: rec[1],
            flags: rec[25],
            name: String::from_utf8_lossy(name).into_owned(),
        })
    }

    /// Whether the identifier names `wanted`, ignoring case, the `;N`
    /// version and a trailing dot (`SYSTEM.CNF;1` names `system.cnf`).
    fn names(&self, wanted: &str) -> bool {
        bare_name(&self.name).eq_ignore_ascii_case(bare_name(wanted))
    }
}

fn bare_name(name: &str) -> &str {
    let stem = name.split_once(';').map_or(name, |(s, _)| s);
    stem.strip_suffix('.').unwrap_or(stem)
}

/// The fields of the primary volume descriptor that identification and the
/// volume fingerprint read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pvd {
    /// System identifier, trailing spaces removed (`PLAYSTATION`).
    pub system_id: String,
    /// Volume identifier, trailing spaces removed.
    pub volume_id: String,
    /// Volume space size, in logical blocks.
    pub space_size: u32,
    /// Volume creation date and time (17-byte dec-datetime).
    pub created: [u8; 17],
    /// Volume modification date and time.
    pub modified: [u8; 17],
    /// The root directory record.
    pub root: DirRecord,
}

/// The primary volume descriptor at LBA 16, or `None` when the source has
/// none there (not ISO 9660, or out of range).
pub fn read_pvd(src: &mut dyn SectorSource) -> io::Result<Option<Pvd>> {
    let mut d = [0u8; USER_BYTES];
    if !src.read_user(PVD_LBA, &mut d)? || d[0] != 1 || &d[1..6] != b"CD001" {
        return Ok(None);
    }
    let text = |r: std::ops::Range<usize>| {
        String::from_utf8_lossy(&d[r])
            .trim_end_matches([' ', '\0'])
            .to_owned()
    };
    let Some(root) = DirRecord::parse(&d[156..190]) else {
        return Ok(None);
    };
    let date = |at: usize| -> [u8; 17] { d[at..at + 17].try_into().unwrap_or([0; 17]) };
    Ok(Some(Pvd {
        system_id: text(8..40),
        volume_id: text(40..72),
        space_size: u32::from_le_bytes(d[80..84].try_into().unwrap_or([0; 4])),
        created: date(813),
        modified: date(830),
        root,
    }))
}

/// Where a file's data lies on the track.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileExtent {
    /// First logical block of the data (extended attributes skipped).
    pub lba: u32,
    /// Data length in bytes.
    pub size: u32,
}

impl FileExtent {
    /// The extent of a file record; `None` when it starts past the address
    /// space.
    pub(crate) fn of(record: &DirRecord) -> Option<FileExtent> {
        Some(FileExtent {
            lba: record.lba.checked_add(u32::from(record.ext_attr))?,
            size: record.size,
        })
    }

    /// Stored bytes of the track that hold the first `cap` bytes of the
    /// file in `layout`: whole sectors from the first one. A caller that
    /// only has the head of an entry reads this range (a skip within its
    /// budget) and wraps it in [`WindowSource::at`](super::WindowSource::at).
    /// `None` when the range does not fit in `u64`.
    pub fn stored_range(&self, layout: SectorLayout, cap: usize) -> Option<Range<u64>> {
        let want = u64::from(self.size).min(cap as u64);
        let sectors = want.div_ceil(USER_BYTES as u64);
        let size = layout.sector_size() as u64;
        let start = u64::from(self.lba).checked_mul(size)?;
        let end = start.checked_add(sectors.checked_mul(size)?)?;
        Some(start..end)
    }
}

/// The first `cap` bytes (all of them when shorter) of the file `name` in
/// the root directory. Only the first [`MAX_ROOT_RECORDS`] records are
/// examined and no subdirectory is entered; a directory of that name is
/// not a file. `None` when not found or when its sectors are out of range.
pub fn find_root_file(
    src: &mut dyn SectorSource,
    pvd: &Pvd,
    name: &str,
    cap: usize,
) -> io::Result<Option<Vec<u8>>> {
    let records = root_records(src, &pvd.root)?;
    let Some(extent) = file_named(&records, name).and_then(FileExtent::of) else {
        return Ok(None);
    };
    read_extent(src, extent, cap)
}

/// The first `cap` bytes (all of them when shorter) of the file at
/// `extent`; `None` when a sector of them is out of range for `src`.
pub fn read_extent(
    src: &mut dyn SectorSource,
    extent: FileExtent,
    cap: usize,
) -> io::Result<Option<Vec<u8>>> {
    let want = (extent.size as usize).min(cap);
    let mut out = Vec::with_capacity(want.min(SYSTEM_CNF_CAP));
    let mut sector = [0u8; USER_BYTES];
    let mut lba = Some(extent.lba);
    while out.len() < want {
        let Some(n) = lba else {
            return Ok(None);
        };
        if !src.read_user(n, &mut sector)? {
            return Ok(None);
        }
        let take = (want - out.len()).min(USER_BYTES);
        out.extend_from_slice(&sector[..take]);
        lba = n.checked_add(1);
    }
    Ok(Some(out))
}

/// The directory record `name` among `records` (`.` and `..` never
/// match: they are stored as `\0` and `\x01`).
pub(crate) fn dir_named<'a>(records: &'a [DirRecord], name: &str) -> Option<&'a DirRecord> {
    records.iter().find(|r| r.is_dir() && r.names(name))
}

/// The file (not directory) record `name` among `records`.
pub(crate) fn file_named<'a>(records: &'a [DirRecord], name: &str) -> Option<&'a DirRecord> {
    records.iter().find(|r| !r.is_dir() && r.names(name))
}

/// The first [`MAX_ROOT_RECORDS`] records of the directory `root`, `.` and
/// `..` included. The walk stops at the first record that does not parse
/// or the first sector out of range, keeping what came before.
pub(crate) fn root_records(
    src: &mut dyn SectorSource,
    root: &DirRecord,
) -> io::Result<Vec<DirRecord>> {
    let mut records = Vec::new();
    let sectors = root.size.div_ceil(USER_BYTES as u32).min(MAX_ROOT_SECTORS);
    let Some(first) = root.lba.checked_add(u32::from(root.ext_attr)) else {
        return Ok(records);
    };
    let mut sector = [0u8; USER_BYTES];
    for i in 0..sectors {
        let Some(lba) = first.checked_add(i) else {
            return Ok(records);
        };
        if !src.read_user(lba, &mut sector)? {
            return Ok(records);
        }
        let mut pos = 0;
        // A zero length byte pads the rest of the sector: records never
        // cross one.
        while pos < USER_BYTES && sector[pos] != 0 {
            let Some(record) = DirRecord::parse(&sector[pos..]) else {
                return Ok(records); // garbage: stop, never guess
            };
            records.push(record);
            if records.len() >= MAX_ROOT_RECORDS {
                return Ok(records);
            }
            pos += usize::from(sector[pos]);
        }
    }
    Ok(records)
}

/// The serial of the one root file named like a PlayStation boot
/// executable (`SLUS_008.62;1`); `None` when there is none or more than
/// one distinct serial.
fn root_name_serial(records: &[DirRecord]) -> Option<String> {
    let mut found: Option<String> = None;
    for record in records.iter().filter(|r| !r.is_dir()) {
        let Some(serial) = boot_file_serial(&record.name) else {
            continue;
        };
        match &found {
            Some(seen) if *seen != serial => return None,
            _ => found = Some(serial),
        }
    }
    found
}

/// The boot lines of a `SYSTEM.CNF`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SystemCnf {
    /// The `BOOT` value (PS1): `cdrom:\SLUS_008.62;1`.
    pub boot: Option<String>,
    /// The `BOOT2` value (PS2): `cdrom0:\SLUS_200.62;1`.
    pub boot2: Option<String>,
}

/// The `BOOT` and `BOOT2` lines of `SYSTEM.CNF` text (`KEY = VALUE`, key
/// case ignored; the first line of each key counts).
pub fn parse_system_cnf(bytes: &[u8]) -> SystemCnf {
    let text = String::from_utf8_lossy(&bytes[..bytes.len().min(SYSTEM_CNF_CAP)]);
    let mut cnf = SystemCnf::default();
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim().to_owned();
        match key.trim().to_ascii_uppercase().as_str() {
            "BOOT" if cnf.boot.is_none() => cnf.boot = Some(value),
            "BOOT2" if cnf.boot2.is_none() => cnf.boot2 = Some(value),
            _ => {}
        }
    }
    cnf
}

/// The volume fingerprint of dedupe spec §5.4: PVD space size and dates,
/// and the upper-case hex SHA1 of the boot file.
pub fn volume_fingerprint(pvd: &Pvd, boot_file: &[u8]) -> VolumeFingerprint {
    let digest = Sha1::digest(boot_file);
    VolumeFingerprint {
        space_size: pvd.space_size,
        created: pvd.created,
        modified: pvd.modified,
        boot_sha1: digest.iter().map(|b| format!("{b:02X}")).collect(),
    }
}

/// The text of the licence screen data in raw sector 4 of a PlayStation
/// disc, compared with its runs of spaces collapsed.
const LICENCE: &[u8] = b"LICENSED BY SONY COMPUTER ENTERTAINMENT";

/// A disc whose primary volume says `PLAYSTATION`.
#[derive(Clone, Debug, PartialEq)]
pub struct PlayStationDisc {
    /// `Known("psx")` or `Known("ps2")`.
    pub system: DiscSystem,
    /// Normalised serial from the boot file name (`SLUS-00862`).
    pub serial: Option<String>,
    /// 0.95 with a boot line; 0.80 from the volume alone.
    pub confidence: f32,
    /// What decided, as data for the UI and the CLI to render
    /// (`SYSTEM.CNF BOOT`, `PLAYSTATION + licence text`).
    pub detail: String,
    /// The Sony licence text was found in raw sector 4: extra evidence,
    /// shown in `detail`, never a stronger verdict on its own.
    pub licence: bool,
    /// `None` without a readable `SYSTEM.CNF`.
    pub volume: Option<VolumeFingerprint>,
    /// `SYSTEM.CNF` is listed in the root directory but its sectors lie
    /// past the source (a window over an entry's head: on real discs it
    /// often sits after a 4 MiB `PSX.BSE`). The caller may read
    /// [`FileExtent::stored_range`] and call [`identify_playstation_with`]
    /// for the full verdict; until then the serial, if any, comes from the
    /// boot executable's name in the root.
    pub pending_boot: Option<FileExtent>,
}

/// The PlayStation verdict for a data track: `None` unless its primary
/// volume says `PLAYSTATION`. `image_len` is the whole image's length when
/// known (a window holds only the head): a `PLAYSTATION` volume without a
/// boot line on more than a CD holds is a PS2 DVD.
pub fn identify_playstation(
    src: &mut dyn SectorSource,
    image_len: Option<u64>,
) -> io::Result<Option<PlayStationDisc>> {
    identify_playstation_with(src, image_len, None)
}

/// [`identify_playstation`] with the `SYSTEM.CNF` bytes already in hand
/// (read from [`PlayStationDisc::pending_boot`]); `None` reads it from
/// `src`. Bytes past [`SYSTEM_CNF_CAP`] are ignored.
pub fn identify_playstation_with(
    src: &mut dyn SectorSource,
    image_len: Option<u64>,
    system_cnf: Option<&[u8]>,
) -> io::Result<Option<PlayStationDisc>> {
    let Some(pvd) = read_pvd(src)? else {
        return Ok(None);
    };
    if pvd.system_id != "PLAYSTATION" {
        return Ok(None);
    }
    let records = root_records(src, &pvd.root)?;
    let mut pending_boot = None;
    let cnf_bytes = match system_cnf {
        Some(bytes) => Some(bytes[..bytes.len().min(SYSTEM_CNF_CAP)].to_vec()),
        None => match file_named(&records, "SYSTEM.CNF").and_then(FileExtent::of) {
            Some(extent) => {
                let read = read_extent(src, extent, SYSTEM_CNF_CAP)?;
                if read.is_none() {
                    pending_boot = Some(extent);
                }
                read
            }
            None => None,
        },
    };
    let cnf = cnf_bytes
        .as_deref()
        .map(parse_system_cnf)
        .unwrap_or_default();
    let licence = has_licence(src)?;
    const PS2: DiscSystem = DiscSystem::Known("ps2");
    const PSX: DiscSystem = DiscSystem::Known("psx");
    let dvd = image_len.is_some_and(|n| n > CD_MAX_BYTES) || pvd.space_size > CD_MAX_SECTORS;
    let (system, mut serial, confidence, detail) = if let Some(boot2) = &cnf.boot2 {
        (PS2, boot_file_serial(boot2), 0.95, "SYSTEM.CNF BOOT2")
    } else if let Some(boot) = &cnf.boot {
        (PSX, boot_file_serial(boot), 0.95, "SYSTEM.CNF BOOT")
    } else if dvd {
        (PS2, None, 0.80, "PLAYSTATION on a DVD-size image")
    } else {
        (PSX, None, 0.80, "PLAYSTATION")
    };
    let mut detail = detail.to_owned();
    // Without a boot line, the boot executable's name in the root still
    // names the serial: shown, never a stronger verdict.
    if cnf.boot.is_none() && cnf.boot2.is_none() {
        serial = root_name_serial(&records);
        if serial.is_some() {
            detail.push_str(" + boot file name");
        }
    }
    if licence {
        detail.push_str(" + licence text");
    }
    Ok(Some(PlayStationDisc {
        system,
        serial,
        confidence,
        detail,
        licence,
        volume: cnf_bytes.map(|b| volume_fingerprint(&pvd, &b)),
        pending_boot,
    }))
}

/// Whether raw sector 4 carries the Sony licence text.
fn has_licence(src: &mut dyn SectorSource) -> io::Result<bool> {
    let Some(sector) = src.raw_sector(4)? else {
        return Ok(false);
    };
    let mut text = Vec::with_capacity(sector.len());
    for &b in &sector {
        let b = if b == 0 { b' ' } else { b.to_ascii_uppercase() };
        if !(b == b' ' && text.last() == Some(&b' ')) {
            text.push(b);
        }
    }
    Ok(text.windows(LICENCE.len()).any(|w| w == LICENCE))
}
