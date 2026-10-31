//! The disc signatures that are neither PlayStation nor Sega (spec §7,
//! systems batch 1 §8.2): PC Engine CD, PC-FX, Neo Geo CD, 3DO, CD-i,
//! Amiga CD32 / CDTV, Jaguar CD and PSP UMD images.
//!
//! Each check reads a fixed sector or the root directory, bounded like the
//! rest of `disc::`.

use std::io;

use super::iso9660::{file_named, read_extent, root_records, FileExtent, Pvd};
use super::serial::normalise_serial;
use super::source::{SectorSource, USER_BYTES};

/// The PC Engine CD boot sector text, at 0x20 of logical sector 1 of the
/// first data track (usually track 02).
pub const PCE_CD: &[u8] = b"PC Engine CD-ROM SYSTEM";

/// The Opera file system's volume header that starts a 3DO disc: record
/// type 1, five sync bytes `5A`, structure version 1.
pub const OPERA: [u8; 7] = [0x01, 0x5A, 0x5A, 0x5A, 0x5A, 0x5A, 0x01];

/// The PC-FX boot header at byte 0 of sector 0 of the data track
/// (rcheevos `rc_hash_pcfx_cd`).
pub const PCFX: &[u8] = b"PC-FX:Hu_CD-ROM";

/// The Green Book disc label at sector 16 of track 1: record type 1 (file
/// structure volume descriptor), `CD-I `, structure version 1 (CD-i Full
/// Functional Specification, chapter III).
pub const CDI_LABEL: [u8; 7] = [0x01, b'C', b'D', b'-', b'I', b' ', 0x01];

/// PVD system ids WinUAE (`blkdev.cpp`, `getunitinfo`) reads as an Amiga
/// CD32 or CDTV volume.
const AMIGA_CD_IDS: [&str; 3] = ["CDTV", "CD32", "COMM"];

/// The CRC-32 of the `CD32.TM` trademark block that CD32 discs store at
/// sector 21, right after the volume descriptors (WinUAE `blkdev.cpp`).
pub const CD32_TM_CRC: u32 = 0xE56C_340F;

/// Sector of the CD32 trademark block.
const CD32_TM_LBA: u32 = 21;

/// The Jaguar CD boot header, in the first sector of the first track of
/// session 2 (rcheevos `rc_hash_jaguar_cd`).
pub const JAGUAR_CD: &[u8; 32] = b"ATARI APPROVED DATA HEADER ATRI ";

/// The header follows 64 bytes of `ATRI` at an unspecified offset.
const JAGUAR_PREAMBLE: usize = 64;

/// Largest `UMD_DATA.BIN` read: its serial is the first field.
const UMD_DATA_CAP: usize = 64;

/// Whether logical sector 1 carries the PC Engine CD boot text at 0x20.
pub fn is_pce_cd(src: &mut dyn SectorSource) -> io::Result<bool> {
    let mut sector = [0u8; USER_BYTES];
    Ok(src.read_user(1, &mut sector)? && sector[0x20..].starts_with(PCE_CD))
}

/// Whether logical sector 0 starts with the PC-FX boot header.
pub fn is_pcfx(sector0: &[u8]) -> bool {
    sector0.starts_with(PCFX)
}

/// Whether logical sector 16 holds the CD-i disc label.
pub fn is_cdi(src: &mut dyn SectorSource) -> io::Result<bool> {
    let mut sector = [0u8; USER_BYTES];
    Ok(src.read_user(16, &mut sector)? && sector.starts_with(&CDI_LABEL))
}

/// The Amiga CD system of an ISO 9660 volume whose system id WinUAE reads
/// as CD32 or CDTV: `amigacd32` when sector 21 is the `CD32.TM` block,
/// `cdtv` when the root holds `CDTV.TM`, else `None` (the volume alone
/// does not tell the two apart).
pub fn amiga_cd(src: &mut dyn SectorSource, pvd: &Pvd) -> io::Result<Option<&'static str>> {
    if !AMIGA_CD_IDS.iter().any(|id| pvd.system_id.starts_with(id)) {
        return Ok(None);
    }
    let mut sector = [0u8; USER_BYTES];
    if src.read_user(CD32_TM_LBA, &mut sector)? && crc32fast::hash(&sector) == CD32_TM_CRC {
        return Ok(Some("amigacd32"));
    }
    let records = root_records(src, &pvd.root)?;
    Ok(file_named(&records, "CDTV.TM").map(|_| "cdtv"))
}

/// Whether `bytes`, the head of the first track of session 2, carry the
/// Jaguar CD boot header as stored or with every 16-bit word swapped.
pub fn is_jaguar_cd(bytes: &[u8]) -> bool {
    let mut swapped = *JAGUAR_CD;
    for pair in swapped.as_chunks_mut::<2>().0 {
        pair.swap(0, 1);
    }
    bytes.len() > JAGUAR_PREAMBLE
        && bytes[JAGUAR_PREAMBLE..]
            .windows(JAGUAR_CD.len())
            .any(|w| w == JAGUAR_CD || w == swapped)
}

/// Whether logical sector 0 starts with the Opera volume header (3DO).
pub fn is_opera(sector0: &[u8]) -> bool {
    sector0.starts_with(&OPERA)
}

/// Whether the root directory holds `IPL.TXT` and no `SYSTEM.CNF`: the
/// Neo Geo CD boot list. The caller has ruled out the Sega and NEC
/// signatures first.
pub fn is_neocd(src: &mut dyn SectorSource, pvd: &Pvd) -> io::Result<bool> {
    let records = root_records(src, &pvd.root)?;
    Ok(file_named(&records, "IPL.TXT").is_some() && file_named(&records, "SYSTEM.CNF").is_none())
}

/// Whether the volume is a UMD: `PSP GAME` as system id (real dumps) or
/// volume id, as `identify::sniff` checks it.
pub fn is_umd(pvd: &Pvd) -> bool {
    pvd.system_id.starts_with("PSP GAME") || pvd.volume_id.starts_with("PSP GAME")
}

/// The serial of a UMD from its root `UMD_DATA.BIN` (`ULUS-10041|…`);
/// `None` when the file is missing, unreadable or holds no serial.
pub fn umd_serial(src: &mut dyn SectorSource, pvd: &Pvd) -> io::Result<Option<String>> {
    let records = root_records(src, &pvd.root)?;
    let Some(extent) = file_named(&records, "UMD_DATA.BIN").and_then(FileExtent::of) else {
        return Ok(None);
    };
    let Some(bytes) = read_extent(src, extent, UMD_DATA_CAP)? else {
        return Ok(None);
    };
    let first = bytes.split(|&b| b == b'|').next().unwrap_or(&[]);
    Ok(std::str::from_utf8(first).ok().and_then(normalise_serial))
}
