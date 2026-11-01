//! GameCube and Wii disc headers through every container (spec §8.1).
//!
//! Only the 0x80-byte disc header is read; it is in clear on every disc,
//! Wii included (the game partition is encrypted with the common key, and
//! nothing past the header is ever read, D4). Offsets checked against
//! Dolphin `Source/Core/DiscIO` (`CISOBlob.h`, `CompressedBlob.h`,
//! `WbfsBlob.h`, `TGCBlob.h`) and `docs/WiaAndRvz.md`.

use super::{Budgeted, ConsoleId, IdSource, MAX_DECODED};

/// `5D 1C 9E A3` at 0x18 of a Wii disc.
const WII_MAGIC: [u8; 4] = [0x5D, 0x1C, 0x9E, 0xA3];
/// `C2 33 9F 3D` at 0x1C of a GameCube disc.
const GC_MAGIC: [u8; 4] = [0xC2, 0x33, 0x9F, 0x3D];
/// The disc header: ID6, disc number, version, magics, title.
const HEADER: usize = 0x80;
/// CISO data starts after its 0x8000-byte header (Dolphin
/// `CISO_HEADER_SIZE`).
const CISO_DATA: u64 = 0x8000;
/// Largest TGC header accepted (real ones are 0x8000).
const TGC_MAX_HEADER: u64 = 0x1_0000;

/// The disc header at the start of `h` (at least 0x80 bytes): Wii by its
/// magic at 0x18, GameCube by its magic at 0x1C, anything else `None`.
/// The ID6 must be the DAT's serial shape, else `None` too.
pub fn disc_header(h: &[u8], detail: &str) -> Option<ConsoleId> {
    let h = h.get(..HEADER)?;
    let system = if h[0x18..0x1C] == WII_MAGIC {
        "wii"
    } else if h[0x1C..0x20] == GC_MAGIC {
        "gamecube"
    } else {
        return None;
    };
    let id6 = std::str::from_utf8(&h[..6]).ok()?;
    let serial = crate::disc::serial::normalise_console_serial(system, id6)?;
    let mut id = ConsoleId::proven(system, detail);
    id.title_id = Some(serial.clone());
    id.serial = Some(serial);
    id.id_source = Some(IdSource::Header);
    // 0-based on the disc: 0 is disc 1.
    id.disc_no = Some(h[0x06].saturating_add(1));
    id.version = Some(h[0x07].to_string());
    id.title = super::clean_title(&String::from_utf8_lossy(&h[0x20..0x60]));
    Some(id)
}

/// A GameCube or Wii disc in any of its containers, by the container's
/// magic at 0 (or the bare disc header).
pub fn identify(b: &mut Budgeted) -> Option<ConsoleId> {
    let n = usize::try_from(b.len().min(0x100)).ok()?;
    let head = b.read(0, n).ok()?;
    match head.get(..4)? {
        b"CISO" => ciso(b, &head),
        // 0xB10BC001 little-endian (Dolphin `GCZ_MAGIC`).
        [0x01, 0xC0, 0x0B, 0xB1] => gcz(b, &head),
        b"WBFS" => wbfs(b, &head),
        b"WIA\x01" => wia(b, "WIA disc header"),
        b"RVZ\x01" => wia(b, "RVZ disc header"),
        // 0xA2380FAE little-endian (Dolphin `TGC_MAGIC`).
        [0xAE, 0x0F, 0x38, 0xA2] => tgc(b, &head),
        _ => disc_header(&head, "disc header"),
    }
}

/// Nintendo CISO and PSP CSO share the `CISO` magic. A Nintendo CISO has
/// a block size at 4 that is a power of two of at least 0x8000 and a
/// block map of 0/1 bytes from 8; a PSP CSO has its header size (0x18) at
/// 4 and the image size at 8.
pub fn is_nintendo_ciso(head: &[u8]) -> bool {
    let bs = head
        .get(4..8)
        .and_then(|b| b.try_into().ok())
        .map(u32::from_le_bytes);
    matches!(bs, Some(bs) if bs >= 0x8000 && bs.is_power_of_two())
        && head.get(8..0x18).is_some_and(|m| m.iter().all(|&x| x <= 1))
}

fn ciso(b: &mut Budgeted, head: &[u8]) -> Option<ConsoleId> {
    // Block 0 must be present for its data to sit at 0x8000.
    if !is_nintendo_ciso(head) || head[8] != 1 {
        return None;
    }
    disc_header(&b.read(CISO_DATA, HEADER).ok()?, "CISO disc header")
}

fn u32le(head: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(head.get(o..o + 4)?.try_into().ok()?))
}

fn u64le(bytes: &[u8]) -> Option<u64> {
    Some(u64::from_le_bytes(bytes.get(..8)?.try_into().ok()?))
}

/// GCZ: a 0x20-byte header, `num_blocks` u64 pointers (top bit = stored),
/// `num_blocks` u32 hashes, then the blocks. Only block 0 is read: stored,
/// or one zlib inflate of at most the block size and [`MAX_DECODED`].
fn gcz(b: &mut Budgeted, head: &[u8]) -> Option<ConsoleId> {
    const STORED: u64 = 1 << 63;
    let sub_type = u32le(head, 4)?;
    let block_size = u64::from(u32le(head, 0x18)?);
    let blocks = u64::from(u32le(head, 0x1C)?);
    let table = blocks.checked_mul(12)?;
    let data = 0x20u64.checked_add(table)?;
    if blocks == 0 || block_size == 0 || data > b.len() {
        return None;
    }
    let p0 = u64le(&b.read(0x20, 8).ok()?)?;
    let start = data.checked_add(p0 & !STORED)?;
    let header = if p0 & STORED != 0 {
        b.read(start, HEADER).ok()?
    } else {
        let end = if blocks > 1 {
            data.checked_add(u64le(&b.read(0x28, 8).ok()?)? & !STORED)?
        } else {
            b.len()
        };
        let clen = end
            .checked_sub(start)?
            .min(block_size)
            .min(MAX_DECODED)
            .min(b.len().checked_sub(start)?);
        let raw = b.read(start, usize::try_from(clen).ok()?).ok()?;
        inflate_prefix(b, &raw, HEADER)?
    };
    let mut id = disc_header(&header, "GCZ disc header")?;
    let gcz_system = if sub_type == 1 { "wii" } else { "gamecube" };
    if gcz_system != id.system {
        id.detail = "GCZ disc header (sub-type disagrees)".into();
    }
    Some(id)
}

/// Inflates zlib `raw` until `want` bytes, charging the budget.
fn inflate_prefix(b: &mut Budgeted, raw: &[u8], want: usize) -> Option<Vec<u8>> {
    use std::io::Read;
    b.decoded(want as u64).ok()?;
    let mut out = vec![0u8; want];
    let mut z = flate2::read::ZlibDecoder::new(raw);
    z.read_exact(&mut out).ok()?;
    Some(out)
}

/// WBFS: `hd_sec_sz_s` at 8; the first disc slot's header copy is at
/// `1 << hd_sec_sz_s`.
fn wbfs(b: &mut Budgeted, head: &[u8]) -> Option<ConsoleId> {
    let shift = *head.get(8)?;
    // 512-byte to 4 KiB sectors.
    if !(9..=12).contains(&shift) {
        return None;
    }
    disc_header(&b.read(1u64 << shift, HEADER).ok()?, "WBFS disc header")
}

/// WIA/RVZ: `wia_disc_t` at 0x48 starts with `disc_type` (u32 BE: 1
/// GameCube, 2 Wii) and keeps a raw copy of the disc header (`dhead`) at
/// 0x58. Nothing is decompressed.
fn wia(b: &mut Budgeted, detail: &str) -> Option<ConsoleId> {
    let h = b.read(0, 0xD8).ok()?;
    let disc_type = u32::from_be_bytes(h[0x48..0x4C].try_into().ok()?);
    let mut id = disc_header(&h[0x58..0xD8], detail)?;
    let want = match disc_type {
        1 => "gamecube",
        2 => "wii",
        _ => id.system.as_str(),
    };
    if want != id.system {
        id.detail = format!("{detail} (disc type disagrees)");
    }
    Some(id)
}

/// TGC (GameCube only): `tgc_header_size` u32 BE at 8; the embedded GCM
/// starts there (Dolphin `TGCBlob::Read` adds it to every offset).
fn tgc(b: &mut Budgeted, head: &[u8]) -> Option<ConsoleId> {
    let off = u64::from(u32::from_be_bytes(head.get(8..12)?.try_into().ok()?));
    if off > TGC_MAX_HEADER {
        return None;
    }
    let id = disc_header(&b.read(off, HEADER).ok()?, "TGC disc header")?;
    (id.system == "gamecube").then_some(id)
}

#[cfg(test)]
mod tests {
    use super::super::testing::{self as t, SparseAt};
    use super::super::{Budgeted, MAX_READ};
    use super::*;
    use crate::identify::sniff::ReadAt;

    fn run(src: &mut SparseAt) -> Option<ConsoleId> {
        let len = src.len;
        identify(&mut Budgeted::new(src, len))
    }

    fn gc() -> Vec<u8> {
        t::gc_header("GM8E01", false)
    }

    fn wii() -> Vec<u8> {
        t::gc_header("RMGE01", true)
    }

    #[test]
    fn plain_disc_header() {
        let id = run(&mut t::gc_disc("GM8E01", false)).unwrap();
        assert_eq!(
            (
                id.system.as_str(),
                id.serial.as_deref(),
                id.title.as_deref()
            ),
            ("gamecube", Some("GM8E01"), Some("Test Game"))
        );
        assert_eq!(
            (id.confidence, id.disc_no, id.version.as_deref()),
            (0.95, Some(1), Some("0"))
        );
        assert_eq!(id.id_source, Some(IdSource::Header));
        assert_eq!(id.detail, "disc header");
        let id = run(&mut t::gc_disc("RMGE01", true)).unwrap();
        assert_eq!(
            (id.system.as_str(), id.title_id.as_deref()),
            ("wii", Some("RMGE01"))
        );
        // No magic, or an ID6 that is not a serial: not a disc.
        assert_eq!(run(&mut SparseAt::new(0x10_0000)), None);
        assert_eq!(run(&mut t::gc_disc("GM 8E1", false)), None);
    }

    #[test]
    fn disc_number_is_zero_based() {
        let mut h = gc();
        h[6] = 1;
        h[7] = 2;
        let id = disc_header(&h, "disc header").unwrap();
        assert_eq!((id.disc_no, id.version.as_deref()), (Some(2), Some("2")));
        assert_eq!(disc_header(&h[..0x7F], "x"), None);
    }

    #[test]
    fn ciso_and_psp_cso() {
        let mut s = t::ciso(&wii());
        let id = run(&mut s).unwrap();
        assert_eq!(
            (id.system.as_str(), id.detail.as_str()),
            ("wii", "CISO disc header")
        );
        let mut head = vec![0u8; 0x18];
        s.read_at(0, &mut head).unwrap();
        assert!(is_nintendo_ciso(&head));
        let mut cso = t::psp_cso();
        let mut head = vec![0u8; 0x18];
        cso.read_at(0, &mut head).unwrap();
        assert!(!is_nintendo_ciso(&head));
        assert_eq!(run(&mut cso), None);
        // Block 0 absent: the header is not at 0x8000.
        let mut s = t::ciso(&wii());
        s.put(8, &[0]);
        assert_eq!(run(&mut s), None);
    }

    #[test]
    fn gcz_stored_and_compressed() {
        for stored in [true, false] {
            let id = run(&mut t::gcz(&gc(), stored)).unwrap();
            assert_eq!(
                (id.system.as_str(), id.detail.as_str()),
                ("gamecube", "GCZ disc header"),
                "stored {stored}"
            );
            let id = run(&mut t::gcz(&wii(), stored)).unwrap();
            assert_eq!(id.serial.as_deref(), Some("RMGE01"));
        }
        let mut src = t::gcz(&gc(), false);
        let len = src.len;
        let mut b = Budgeted::new(&mut src, len);
        identify(&mut b).unwrap();
        assert_eq!(b.used().decoded, 0x80);
        // A GCZ of zeros has no disc header.
        assert_eq!(run(&mut t::gcz(&[0u8; 0x80], true)), None);
    }

    #[test]
    fn gcz_sub_type_disagreeing_is_noted() {
        let id = run(&mut t::gcz_with(&gc(), true, 1, 2)).unwrap();
        assert_eq!(
            (id.system.as_str(), id.detail.as_str()),
            ("gamecube", "GCZ disc header (sub-type disagrees)")
        );
    }

    #[test]
    fn budget_gcz_adversarial() {
        let mut src = t::gcz_with(&gc(), true, 0, 0x8000_0000);
        let len = src.len;
        let mut b = Budgeted::new(&mut src, len);
        assert_eq!(identify(&mut b), None);
        assert!(b.used().read <= MAX_READ);
        let mut src = t::gcz_with(&gc(), true, 0, 0);
        assert_eq!(run(&mut src), None);
    }

    #[test]
    fn wbfs_header_copy() {
        let id = run(&mut t::wbfs(&wii())).unwrap();
        assert_eq!(
            (id.system.as_str(), id.detail.as_str()),
            ("wii", "WBFS disc header")
        );
        assert_eq!(run(&mut t::wbfs_shift(&wii(), 12)).unwrap().system, "wii");
        assert_eq!(run(&mut t::wbfs_shift(&wii(), 30)), None);
        assert_eq!(run(&mut t::wbfs_shift(&wii(), 8)), None);
    }

    #[test]
    fn wia_and_rvz_keep_a_raw_header() {
        let id = run(&mut t::wia(&wii(), false, 2)).unwrap();
        assert_eq!(
            (id.system.as_str(), id.detail.as_str()),
            ("wii", "WIA disc header")
        );
        let id = run(&mut t::wia(&gc(), true, 1)).unwrap();
        assert_eq!(
            (id.system.as_str(), id.detail.as_str()),
            ("gamecube", "RVZ disc header")
        );
        let id = run(&mut t::wia(&gc(), true, 2)).unwrap();
        assert_eq!(
            (id.system.as_str(), id.detail.as_str()),
            ("gamecube", "RVZ disc header (disc type disagrees)")
        );
        assert_eq!(run(&mut t::wia(&[0u8; 0x80], true, 2)), None);
    }

    #[test]
    fn tgc_embedded_gcm() {
        let id = run(&mut t::tgc(&gc())).unwrap();
        assert_eq!(
            (id.system.as_str(), id.detail.as_str()),
            ("gamecube", "TGC disc header")
        );
        assert_eq!(run(&mut t::tgc(&wii())), None);
        let mut far = t::tgc(&gc());
        far.put(8, &0x10_0000u32.to_be_bytes());
        assert_eq!(run(&mut far), None);
        // A Game.com `.tgc` has no TGC magic.
        assert_eq!(run(&mut SparseAt::new(0x8000)), None);
    }

    #[test]
    fn budget_every_format() {
        for mut src in [
            t::gc_disc("GM8E01", false),
            t::ciso(&wii()),
            t::gcz(&gc(), false),
            t::wbfs(&wii()),
            t::wia(&wii(), true, 2),
            t::tgc(&gc()),
        ] {
            let len = src.len;
            let mut b = Budgeted::new(&mut src, len);
            assert!(identify(&mut b).is_some());
            let u = b.used();
            assert!(u.read <= MAX_READ && u.decoded <= MAX_DECODED && u.seeks <= 16);
            assert!(u.read <= 0x10_000, "{u:?}");
        }
    }
}
