//! The disc signatures that are neither PlayStation nor Sega (spec §7):
//! PC Engine CD, Neo Geo CD, 3DO and PSP UMD images.
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

/// Largest `UMD_DATA.BIN` read: its serial is the first field.
const UMD_DATA_CAP: usize = 64;

/// Whether logical sector 1 carries the PC Engine CD boot text at 0x20.
pub fn is_pce_cd(src: &mut dyn SectorSource) -> io::Result<bool> {
    let mut sector = [0u8; USER_BYTES];
    Ok(src.read_user(1, &mut sector)? && sector[0x20..].starts_with(PCE_CD))
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
