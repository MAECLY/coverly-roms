//! Sega boot headers (`IP.BIN`, the system area of the first data track):
//! Saturn, Dreamcast and Sega CD (spec §7).
//!
//! The header is the user data of the track's first logical sectors. It is
//! parsed from fixed offsets only, and every field comes from untrusted
//! bytes: a product number passes only through
//! [`normalise_serial`], and nothing read from the disc ever reaches a
//! detail string.

use std::io;

use super::serial::normalise_serial;
use super::source::{SectorSource, USER_BYTES};

/// Bytes of `IP.BIN` hashed for the volume fingerprint: the first 32 KiB
/// of the track's user data (16 logical sectors).
pub const IP_BIN_BYTES: usize = 32 * 1024;

/// Bytes of the header [`parse_ip_bin`] reads: the Sega CD serial field
/// ends at 0x18E.
pub const IP_BIN_HEAD: usize = 0x190;

/// Which Sega system a boot header belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SegaSystem {
    /// `SEGA SEGASATURN` (system id `saturn`).
    Saturn,
    /// `SEGA SEGAKATANA`, the high-density data track (`dreamcast`).
    Dreamcast,
    /// `SEGADISCSYSTEM` (`segacd`): a disc, never a Mega Drive cartridge.
    SegaCd,
}

impl SegaSystem {
    /// The `systems.toml` id.
    pub fn id(self) -> &'static str {
        match self {
            SegaSystem::Saturn => "saturn",
            SegaSystem::Dreamcast => "dreamcast",
            SegaSystem::SegaCd => "segacd",
        }
    }
}

/// What a Sega boot header says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IpBin {
    pub system: SegaSystem,
    /// Normalised product number (`T-5704H`, `T-40214N`, `MK-4407`);
    /// `None` when the field holds no serial.
    pub serial: Option<String>,
    /// Disc number and count from the device field (`GD-ROM1/1`,
    /// `CD-1/2`), when both are digits and the number is within the count.
    pub disc: Option<(u8, u8)>,
    /// Detail for the UI and the CLI: a fixed string, never disc bytes
    /// (`SEGA SEGASATURN`, `SEGADISCSYSTEM + SEGA MEGA DRIVE`).
    pub detail: &'static str,
}

/// The header magic of each system, as [`identify::sniff`] checks it too.
///
/// [`identify::sniff`]: crate::identify::sniff
const SATURN: &[u8] = b"SEGA SEGASATURN";
const KATANA: &[u8] = b"SEGA SEGAKATANA";
const SEGA_CD: &[u8] = b"SEGADISCSYSTEM";

/// Parses a boot header from `head`, the user data of the track's first
/// sector onwards (at least [`IP_BIN_HEAD`] bytes for every field; a
/// shorter head still matches the magic). `None` when no Sega magic is at
/// offset 0.
pub fn parse_ip_bin(head: &[u8]) -> Option<IpBin> {
    let field = |at: usize, n: usize| head.get(at..at + n);
    let text = |at: usize, n: usize| field(at, n).and_then(|b| std::str::from_utf8(b).ok());
    let serial = |at: usize, n: usize| text(at, n).and_then(normalise_serial);
    if head.starts_with(SATURN) {
        // Device information at 0x38: `CD-1/1  `.
        let disc = field(0x38, 8)
            .and_then(|b| b.strip_prefix(b"CD-"))
            .and_then(disc_of);
        return Some(IpBin {
            system: SegaSystem::Saturn,
            serial: serial(0x20, 10),
            disc,
            detail: "SEGA SEGASATURN",
        });
    }
    if head.starts_with(KATANA) {
        // Device information at 0x20: `XXXX GD-ROMn/m  `, `n/m` at 0x2B.
        return Some(IpBin {
            system: SegaSystem::Dreamcast,
            serial: serial(0x40, 10),
            disc: field(0x2B, 3).and_then(disc_of),
            detail: "SEGA SEGAKATANA",
        });
    }
    if head.starts_with(SEGA_CD) {
        let detail = match field(0x100, 16) {
            Some(t) if t.starts_with(b"SEGA MEGA DRIVE") => "SEGADISCSYSTEM + SEGA MEGA DRIVE",
            Some(t) if t.starts_with(b"SEGA GENESIS") => "SEGADISCSYSTEM + SEGA GENESIS",
            _ => "SEGADISCSYSTEM",
        };
        return Some(IpBin {
            system: SegaSystem::SegaCd,
            serial: serial(0x180, 14),
            disc: None,
            detail,
        });
    }
    None
}

/// [`parse_ip_bin`] at offset 0 of `head` (2048-byte sectors) or at 0x10
/// (raw 2352-byte Mode 1 sectors: sync and header first). For a raw head
/// whose layout is not known yet, such as a cartridge-style prefix.
pub fn ip_bin_in_head(head: &[u8]) -> Option<IpBin> {
    parse_ip_bin(head).or_else(|| head.get(0x10..).and_then(parse_ip_bin))
}

/// `n/m` from the start of `b`: both one digit, `1 ≤ n ≤ m`.
fn disc_of(b: &[u8]) -> Option<(u8, u8)> {
    let &[n, b'/', m, ..] = b else {
        return None;
    };
    let (n, m) = (digit(n)?, digit(m)?);
    (1..=m).contains(&n).then_some((n, m))
}

fn digit(b: u8) -> Option<u8> {
    (b'1'..=b'9').contains(&b).then(|| b - b'0')
}

/// The boot header of the track behind `src`, read from logical sector 0.
pub fn read_ip_bin(src: &mut dyn SectorSource) -> io::Result<Option<IpBin>> {
    let mut sector = [0u8; USER_BYTES];
    if !src.read_user(0, &mut sector)? {
        return Ok(None);
    }
    Ok(parse_ip_bin(&sector))
}

/// The first [`IP_BIN_BYTES`] of the track's user data, for the volume
/// fingerprint; `None` when the source does not hold all of them.
pub fn ip_bin_bytes(src: &mut dyn SectorSource) -> io::Result<Option<Vec<u8>>> {
    let mut out = Vec::with_capacity(IP_BIN_BYTES);
    let mut sector = [0u8; USER_BYTES];
    for lba in 0..(IP_BIN_BYTES / USER_BYTES) as u32 {
        if !src.read_user(lba, &mut sector)? {
            return Ok(None);
        }
        out.extend_from_slice(&sector);
    }
    Ok(Some(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(magic: &[u8]) -> Vec<u8> {
        let mut h = vec![b' '; IP_BIN_HEAD];
        h[..magic.len()].copy_from_slice(magic);
        h
    }

    #[test]
    fn disc_fields_need_digits_within_the_count() {
        assert_eq!(disc_of(b"1/1"), Some((1, 1)));
        assert_eq!(disc_of(b"2/3  "), Some((2, 3)));
        assert_eq!(disc_of(b"3/2"), None);
        assert_eq!(disc_of(b"0/1"), None);
        assert_eq!(disc_of(b"1-1"), None);
        assert_eq!(disc_of(b"1/"), None);
        assert_eq!(disc_of(b""), None);
    }

    #[test]
    fn short_and_foreign_heads() {
        assert_eq!(parse_ip_bin(b""), None);
        assert_eq!(parse_ip_bin(b"SEGA SEGASATUR"), None);
        assert_eq!(parse_ip_bin(&header(b"SEGA MEGA DRIVE")), None);
        // The magic alone, with no room for the fields, still matches.
        let bare = parse_ip_bin(SATURN).unwrap();
        assert_eq!(bare.system, SegaSystem::Saturn);
        assert_eq!((bare.serial, bare.disc), (None, None));
        // A raw head carries the header at 0x10.
        let mut raw = vec![0u8; 0x10];
        raw.extend(header(KATANA));
        assert_eq!(ip_bin_in_head(&raw).unwrap().system, SegaSystem::Dreamcast);
    }
}
