//! Wii WAD packages (WiiWare, Virtual Console, channels; spec §8.1).
//!
//! Layout from wiibrew «WAD files» and «Title metadata»: a 0x20-byte
//! header (`0x20`, type `Is` or `ib`), the section sizes, then the
//! certificate chain, ticket, TMD and content, each 64-byte aligned from
//! 0x40. Only the header and the TMD are read; the ticket (which holds
//! the encrypted title key) is never read (D4), and the content is
//! encrypted, so the title name comes from the DAT.
//!
//! BLOCKED: the No-Intro Wii (Digital) DAT lists `tmd.<n>` rows by CRC;
//! matching the TMD section's CRC was not verified on a real WAD (none
//! was at hand), so the reader returns the id only and WADs match by
//! name (spec §8.1).

use super::{kind_wii, Budgeted, ConsoleId, IdSource};

/// The header size field, always 0x20.
const HEADER_SIZE: u32 = 0x20;
/// The sections start here.
const SECTIONS: u64 = 0x40;
/// Largest TMD read; a larger one stays magic-only.
const MAX_TMD: u64 = 0x2000;
/// Title id offset in the TMD (after the RSA-2048 signature block and
/// the issuer, version, CRL versions and system version).
const TMD_TITLE_ID: usize = 0x18C;
/// RSA-2048 with SHA-1, the TMD signature type.
const TMD_SIG_RSA2048: u32 = 0x0001_0001;

fn align64(x: u64) -> Option<u64> {
    Some(x.checked_add(63)? & !63)
}

/// Whether `head` opens like an installable WAD.
pub fn is_wad_header(head: &[u8]) -> bool {
    head.get(..4) == Some(&HEADER_SIZE.to_be_bytes()[..])
        && matches!(head.get(4..8), Some(b"Is\0\0" | b"ib\0\0"))
}

fn be32(h: &[u8], o: usize) -> Option<u64> {
    Some(u64::from(u32::from_be_bytes(
        h.get(o..o + 4)?.try_into().ok()?,
    )))
}

/// A Wii WAD: the TMD's title id, its ID4 as the serial and the kind.
pub fn identify(b: &mut Budgeted) -> Option<ConsoleId> {
    if b.len() < SECTIONS {
        return None;
    }
    let h = b.read(0, 0x20).ok()?;
    if !is_wad_header(&h) {
        return None;
    }
    let (cert, ticket, tmd) = (be32(&h, 0x08)?, be32(&h, 0x10)?, be32(&h, 0x14)?);
    let ticket_off = align64(SECTIONS.checked_add(cert)?)?;
    let tmd_off = align64(ticket_off.checked_add(ticket)?)?;
    let magic = ConsoleId::magic("wii", "WAD header");
    let fits = tmd_off.checked_add(tmd).is_some_and(|end| end <= b.len());
    if !fits || tmd > MAX_TMD || (tmd as usize) < TMD_TITLE_ID + 8 {
        return Some(magic);
    }
    let Ok(t) = b.read(tmd_off, tmd as usize) else {
        return Some(magic);
    };
    if be32(&t, 0) != Some(u64::from(TMD_SIG_RSA2048)) {
        return Some(magic);
    }
    let tid = u64::from_be_bytes(t[TMD_TITLE_ID..TMD_TITLE_ID + 8].try_into().ok()?);
    let mut id = ConsoleId::proven("wii", "WAD TMD");
    id.title_id = Some(format!("{tid:016X}"));
    let id4 = &t[TMD_TITLE_ID + 4..TMD_TITLE_ID + 8];
    if id4.iter().all(u8::is_ascii_alphanumeric) {
        id.serial = std::str::from_utf8(id4)
            .ok()
            .and_then(|s| crate::disc::serial::normalise_console_serial("wii", s));
    }
    id.kind = kind_wii(tid);
    id.id_source = Some(IdSource::Header);
    Some(id)
}

#[cfg(test)]
mod tests {
    use super::super::testing::{self as t, RangeLog, SparseAt};
    use super::super::TitleKind;
    use super::*;

    fn run(src: &mut SparseAt) -> Option<ConsoleId> {
        let len = src.len;
        identify(&mut Budgeted::new(src, len))
    }

    #[test]
    fn title_id_id4_and_kind() {
        let id = run(&mut t::wii_wad(0x0001_0001_5741_4145)).unwrap();
        assert_eq!(
            (
                id.system.as_str(),
                id.serial.as_deref(),
                id.title_id.as_deref()
            ),
            ("wii", Some("WAAE"), Some("0001000157414145"))
        );
        assert_eq!(
            (id.kind, id.confidence, id.detail.as_str()),
            (TitleKind::Base, 0.95, "WAD TMD")
        );
        assert_eq!(id.title, None);
        let dlc = run(&mut t::wii_wad(0x0001_0005_5741_4145)).unwrap();
        assert_eq!(dlc.kind, TitleKind::Dlc);
        // An ID4 that is not text gives no serial.
        let odd = run(&mut t::wii_wad(0x0001_0001_0000_0002)).unwrap();
        assert_eq!(odd.serial, None);
    }

    #[test]
    fn not_a_wad() {
        let mut doom = SparseAt::new(0x1000);
        doom.put(0, b"IWAD\x10\0\0\0");
        assert_eq!(run(&mut doom), None);
        assert_eq!(run(&mut SparseAt::new(0x10)), None);
    }

    #[test]
    fn a_huge_or_cut_tmd_is_magic_only() {
        let mut s = t::wii_wad(0x0001_0001_5741_4145);
        s.put(0x14, &0x10_0000u32.to_be_bytes());
        let id = run(&mut s).unwrap();
        assert_eq!((id.confidence, id.title_id), (0.90, None));
        let mut s = t::wii_wad(0x0001_0001_5741_4145);
        s.put(0x10, &u32::MAX.to_be_bytes());
        assert_eq!(run(&mut s).unwrap().confidence, 0.90);
    }

    #[test]
    fn the_ticket_is_never_read() {
        let mut src = t::wii_wad(0x0001_0001_5741_4145);
        let len = src.len;
        let mut log = RangeLog::new(&mut src);
        let id = {
            let mut b = Budgeted::new(&mut log, len);
            identify(&mut b)
        };
        assert!(id.is_some());
        assert!(!log.touched(t::WAD_TICKET.0, t::WAD_TICKET.1));
        assert!(log.touched(t::WAD_TMD, 0x200));
    }
}
