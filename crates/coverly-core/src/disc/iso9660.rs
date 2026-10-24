//! ISO 9660 as far as disc identification needs it, and the PlayStation
//! rules over it (spec §7).
//!
//! Only the primary volume descriptor (LBA 16) and the root directory are
//! read: at most [`MAX_ROOT_RECORDS`] records, depth 1, and a file is read
//! up to a caller cap. Every field comes from untrusted bytes, so every
//! offset is checked and nothing recurses.

use std::io;

use sha1::{Digest, Sha1};

use super::serial::boot_file_serial;
use super::source::{SectorSource, USER_BYTES};
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
    let Some(record) = find_root_record(src, &pvd.root, name)? else {
        return Ok(None);
    };
    let want = (record.size as usize).min(cap);
    let start = record.lba.checked_add(u32::from(record.ext_attr));
    let mut out = Vec::with_capacity(want.min(SYSTEM_CNF_CAP));
    let mut sector = [0u8; USER_BYTES];
    let mut lba = start;
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

/// The file record `name` among the first [`MAX_ROOT_RECORDS`] records of
/// the directory `root`.
fn find_root_record(
    src: &mut dyn SectorSource,
    root: &DirRecord,
    name: &str,
) -> io::Result<Option<DirRecord>> {
    let sectors = root.size.div_ceil(USER_BYTES as u32).min(MAX_ROOT_SECTORS);
    let Some(first) = root.lba.checked_add(u32::from(root.ext_attr)) else {
        return Ok(None);
    };
    let mut seen = 0;
    let mut sector = [0u8; USER_BYTES];
    for i in 0..sectors {
        let Some(lba) = first.checked_add(i) else {
            return Ok(None);
        };
        if !src.read_user(lba, &mut sector)? {
            return Ok(None);
        }
        let mut pos = 0;
        // A zero length byte pads the rest of the sector: records never
        // cross one.
        while pos < USER_BYTES && sector[pos] != 0 {
            let Some(record) = DirRecord::parse(&sector[pos..]) else {
                return Ok(None); // garbage: stop, never guess
            };
            seen += 1;
            if !record.is_dir() && record.names(name) {
                return Ok(Some(record));
            }
            if seen >= MAX_ROOT_RECORDS {
                return Ok(None);
            }
            pos += usize::from(sector[pos]);
        }
    }
    Ok(None)
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
    /// `Known("psx")`, or `Unsupported("PlayStation 2")` (no `ps2` system).
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
}

/// The PlayStation verdict for a data track: `None` unless its primary
/// volume says `PLAYSTATION`. `image_len` is the whole image's length when
/// known (a window holds only the head): a `PLAYSTATION` volume without a
/// boot line on more than a CD holds is a PS2 DVD.
pub fn identify_playstation(
    src: &mut dyn SectorSource,
    image_len: Option<u64>,
) -> io::Result<Option<PlayStationDisc>> {
    let Some(pvd) = read_pvd(src)? else {
        return Ok(None);
    };
    if pvd.system_id != "PLAYSTATION" {
        return Ok(None);
    }
    let cnf_bytes = find_root_file(src, &pvd, "SYSTEM.CNF", SYSTEM_CNF_CAP)?;
    let cnf = cnf_bytes
        .as_deref()
        .map(parse_system_cnf)
        .unwrap_or_default();
    let licence = has_licence(src)?;
    const PS2: DiscSystem = DiscSystem::Unsupported("PlayStation 2");
    const PSX: DiscSystem = DiscSystem::Known("psx");
    let dvd = image_len.is_some_and(|n| n > CD_MAX_BYTES) || pvd.space_size > CD_MAX_SECTORS;
    let (system, serial, confidence, detail) = if let Some(boot2) = &cnf.boot2 {
        (PS2, boot_file_serial(boot2), 0.95, "SYSTEM.CNF BOOT2")
    } else if let Some(boot) = &cnf.boot {
        (PSX, boot_file_serial(boot), 0.95, "SYSTEM.CNF BOOT")
    } else if dvd {
        (PS2, None, 0.80, "PLAYSTATION on a DVD-size image")
    } else {
        (PSX, None, 0.80, "PLAYSTATION")
    };
    let mut detail = detail.to_owned();
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
