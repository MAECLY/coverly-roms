//! Translation patches and hacks (mods spec D2): told by their magic,
//! with the key their header or footer carries.

use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

use super::{one_component, AddonFormat, AddonKey};

/// Bytes of a patch's head the sniffer reads.
pub const HEAD_LEN: usize = 64;
/// Bytes of an xdelta3 header read from offset 4 (indicator, secondary
/// compressor, code table, app header).
pub const XDELTA_APP_CAP: usize = 4096;

const XDELTA_MAGIC: [u8; 4] = [0xD6, 0xC3, 0xC4, 0x00];

/// A beat varint (BPS/UPS) at the start of `data`: its value and length.
/// `None` when it does not end within 10 bytes or overflows.
pub fn read_varint(data: &[u8]) -> Option<(u64, usize)> {
    let mut value: u64 = 0;
    let mut shift: u64 = 1;
    for (i, &x) in data.iter().take(10).enumerate() {
        value = value.checked_add(u64::from(x & 0x7f).checked_mul(shift)?)?;
        if x & 0x80 != 0 {
            return Some((value, i + 1));
        }
        shift = shift.checked_mul(128)?;
        value = value.checked_add(shift)?;
    }
    None
}

/// A VCDIFF integer (big-endian base-128) at the start of `data`: its
/// value and length, at most 9 bytes.
fn read_vcdiff_int(data: &[u8]) -> Option<(u64, usize)> {
    let mut value: u64 = 0;
    for (i, &x) in data.iter().take(9).enumerate() {
        value = value.checked_mul(128)?.checked_add(u64::from(x & 0x7f))?;
        if x & 0x80 == 0 {
            return Some((value, i + 1));
        }
    }
    None
}

/// D2: by magic first (the extension is the caller's second check).
/// `head` = the first bytes of the file (64 are enough), `tail12` = its
/// last 12 bytes (BPS/UPS footer), `xdelta_app` = at most 4 KiB from
/// offset 4. Never panics on short or garbage input.
pub fn sniff_patch(
    head: &[u8],
    tail12: Option<[u8; 12]>,
    xdelta_app: Option<&[u8]>,
) -> Option<(AddonFormat, AddonKey)> {
    if let Some(format) = [(b"BPS1", AddonFormat::Bps), (b"UPS1", AddonFormat::Ups)]
        .into_iter()
        .find_map(|(magic, f)| head.starts_with(magic).then_some(f))
    {
        let key = match (read_varint(&head[4..]), tail12) {
            (Some((size, _)), Some(tail)) => AddonKey::SourceCrc {
                crc32: u32::from_le_bytes([tail[0], tail[1], tail[2], tail[3]]),
                size,
            },
            _ => AddonKey::None,
        };
        return Some((format, key));
    }
    if head.starts_with(b"PATCH") {
        // `PATCH` … `EOF`, or `EOF` and a 3-byte truncation size. Without
        // the footer bytes (a head-only caller) the magic alone decides.
        let ends = tail12.is_none_or(|t| &t[9..] == b"EOF" || &t[6..9] == b"EOF");
        return ends.then_some((AddonFormat::Ips, AddonKey::None));
    }
    if head.starts_with(&XDELTA_MAGIC) {
        let key = xdelta_app
            .and_then(xdelta_source_name)
            .map_or(AddonKey::None, |name| AddonKey::SourceName { name });
        return Some((AddonFormat::Xdelta, key));
    }
    if [b"PPF10", b"PPF20", b"PPF30"]
        .iter()
        .any(|m| head.starts_with(*m))
    {
        return Some((AddonFormat::Ppf, AddonKey::None));
    }
    if head.starts_with(b"APS1") {
        return Some((
            AddonFormat::Aps,
            aps_n64_id(head).map_or(AddonKey::None, |id| AddonKey::N64Id { id }),
        ));
    }
    None
}

/// The N64 cart id of an `APS10` header (type 1): three alphanumeric
/// bytes at offset 58.
fn aps_n64_id(head: &[u8]) -> Option<String> {
    if !head.starts_with(b"APS10") || head.get(5) != Some(&1) {
        return None;
    }
    let id = head.get(58..61)?;
    id.iter()
        .all(u8::is_ascii_alphanumeric)
        .then(|| String::from_utf8_lossy(id).into_owned())
}

/// The source ROM name of an xdelta3 header. `app` starts at the header
/// indicator (offset 4). xdelta3 writes its app header as
/// `<target>/<target compression>/<source>/<source compression>` (plan
/// P-2); only the final path component of the source is kept, and only
/// when it is one safe component.
fn xdelta_source_name(app: &[u8]) -> Option<String> {
    let indicator = *app.first()?;
    let mut at = 1usize;
    if indicator & 0x01 != 0 {
        at = at.checked_add(1)?;
    }
    if indicator & 0x02 != 0 {
        let (len, n) = read_vcdiff_int(app.get(at..)?)?;
        at = at.checked_add(n)?.checked_add(usize::try_from(len).ok()?)?;
    }
    if indicator & 0x04 == 0 {
        return None;
    }
    let (len, n) = read_vcdiff_int(app.get(at..)?)?;
    let len = usize::try_from(len).ok().filter(|l| *l <= XDELTA_APP_CAP)?;
    at = at.checked_add(n)?;
    let bytes = app.get(at..at.checked_add(len)?)?;
    let text = String::from_utf8_lossy(bytes);
    let (_target, rest) = text.split_once('/')?;
    let (_target_comp, rest) = rest.split_once('/')?;
    let (source, _source_comp) = rest.rsplit_once('/')?;
    let name = source.rsplit(['/', '\\']).next()?;
    one_component(name).map(str::to_owned)
}

/// `.ips` is 0, `.ips1` … `.ips9` (RetroArch's patch chain) their digit;
/// case-insensitive.
pub fn ips_chain_index(name: &str) -> Option<u8> {
    let (_, ext) = name.rsplit_once('.')?;
    let ext = ext.to_ascii_lowercase();
    let rest = ext.strip_prefix("ips")?;
    match rest.as_bytes() {
        [] => Some(0),
        [d @ b'1'..=b'9'] => Some(d - b'0'),
        _ => None,
    }
}

/// Reads at most `buf.len()` bytes, stopping early only at the end.
fn read_up_to<R: Read>(r: &mut R, buf: &mut [u8]) -> io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        match r.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(filled)
}

/// The bounded reads of one patch (D2): the first 64 bytes, then the last
/// 12 for BPS/UPS/IPS or at most 4 KiB from offset 4 for xdelta. Never
/// reads the whole patch.
pub fn read_patch_from<R: Read + Seek>(r: &mut R) -> io::Result<Option<(AddonFormat, AddonKey)>> {
    let mut head = [0u8; HEAD_LEN];
    let n = read_up_to(r, &mut head)?;
    let head = &head[..n];
    let mut tail = None;
    let mut app = None;
    if head.starts_with(b"BPS1") || head.starts_with(b"UPS1") {
        let len = r.seek(SeekFrom::End(0))?;
        if len >= 16 {
            r.seek(SeekFrom::End(-12))?;
            let mut t = [0u8; 12];
            r.read_exact(&mut t)?;
            tail = Some(t);
        }
    } else if head.starts_with(b"PATCH") {
        // The `EOF` check needs the end; a file shorter than 12 bytes is
        // all in `head`, right-aligned into the 12-byte window.
        let len = r.seek(SeekFrom::End(0))?;
        let mut t = [0u8; 12];
        match usize::try_from(len).ok().filter(|l| *l < 12) {
            Some(l) => t[12 - l..].copy_from_slice(&head[..l]),
            None => {
                r.seek(SeekFrom::End(-12))?;
                r.read_exact(&mut t)?;
            }
        }
        tail = Some(t);
    } else if head.starts_with(&XDELTA_MAGIC) {
        r.seek(SeekFrom::Start(4))?;
        let mut buf = vec![0u8; XDELTA_APP_CAP];
        let got = read_up_to(r, &mut buf)?;
        buf.truncate(got);
        app = Some(buf);
    }
    Ok(sniff_patch(head, tail, app.as_deref()))
}

/// Reads the bounded bytes of one file and sniffs it: the only function
/// here that touches the file system. Callers check
/// [`crate::consoles::is_key_material`] before calling; the file is
/// opened through [`crate::consoles::open_rom`], which refuses key
/// material by name all the same.
pub fn read_patch(path: &Path) -> io::Result<Option<(AddonFormat, AddonKey)>> {
    let mut file = crate::consoles::open_rom(path)?;
    read_patch_from(&mut file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addons::testing as t;

    #[test]
    fn bps_and_ups_give_the_source_crc_and_size() {
        let b = t::bps(1_048_576, 0x2D20_6BF7);
        let tail: [u8; 12] = b[b.len() - 12..].try_into().unwrap();
        assert_eq!(
            sniff_patch(&b[..b.len().min(64)], Some(tail), None),
            Some((
                AddonFormat::Bps,
                AddonKey::SourceCrc {
                    crc32: 0x2D20_6BF7,
                    size: 1_048_576
                }
            ))
        );
        let u = t::ups(40_976, 0x1A2B_3C4D);
        let tail: [u8; 12] = u[u.len() - 12..].try_into().unwrap();
        assert_eq!(
            sniff_patch(&u[..u.len().min(64)], Some(tail), None),
            Some((
                AddonFormat::Ups,
                AddonKey::SourceCrc {
                    crc32: 0x1A2B_3C4D,
                    size: 40_976
                }
            ))
        );
    }

    #[test]
    fn varint_edges_round_trip() {
        for n in [0, 1, 127, 128, 16_511, 16_512, u32::MAX as u64, 1 << 40] {
            assert_eq!(
                read_varint(&t::varint(n)),
                Some((n, t::varint(n).len())),
                "{n}"
            );
        }
        assert_eq!(read_varint(&[]), None);
        assert_eq!(read_varint(&[0x00, 0x00]), None, "unterminated");
        assert_eq!(read_varint(&[0x7f; 16]), None, "too long");
    }

    #[test]
    fn ips_ppf_aps_and_keyless_forms() {
        assert_eq!(
            sniff_patch(&t::ips(), None, None),
            Some((AddonFormat::Ips, AddonKey::None))
        );
        for v in [1, 2, 3] {
            assert_eq!(
                sniff_patch(&t::ppf(v), None, None),
                Some((AddonFormat::Ppf, AddonKey::None))
            );
        }
        assert_eq!(sniff_patch(b"PPF40rest", None, None), None);
        // IPS is `PATCH` … `EOF` (or `EOF` and a 3-byte truncation size).
        let mut tail = [0u8; 12];
        tail[9..].copy_from_slice(b"EOF");
        assert_eq!(
            sniff_patch(&t::ips(), Some(tail), None),
            Some((AddonFormat::Ips, AddonKey::None))
        );
        tail[6..9].copy_from_slice(b"EOF");
        tail[9..].copy_from_slice(&[0x01, 0x00, 0x00]);
        assert_eq!(
            sniff_patch(&t::ips(), Some(tail), None),
            Some((AddonFormat::Ips, AddonKey::None)),
            "EOF and a truncation size"
        );
        assert_eq!(
            sniff_patch(b"PATCH NOTES for 1.2", Some(*b"ed in v1.2.\n"), None),
            None,
            "text that starts with PATCH and has no EOF is no IPS"
        );
        assert_eq!(
            sniff_patch(&t::aps_n64("NSM"), None, None),
            Some((AddonFormat::Aps, AddonKey::N64Id { id: "NSM".into() }))
        );
        assert_eq!(
            sniff_patch(b"APS1\x00rest", None, None),
            Some((AddonFormat::Aps, AddonKey::None))
        );
        assert_eq!(
            sniff_patch(&t::aps_n64("N/\0"), None, None),
            Some((AddonFormat::Aps, AddonKey::None)),
            "a cart id that is not alphanumeric is no key"
        );
    }

    #[test]
    fn xdelta_takes_the_source_name_from_the_third_app_header_field() {
        let x = t::xdelta(Some("patched.iso//Game (USA).iso/"));
        assert_eq!(
            sniff_patch(&x[..4], None, Some(&x[4..])),
            Some((
                AddonFormat::Xdelta,
                AddonKey::SourceName {
                    name: "Game (USA).iso".into()
                }
            ))
        );
        let x = t::xdelta(Some("out.iso//../../etc/Game (USA).iso/"));
        assert_eq!(
            sniff_patch(&x[..4], None, Some(&x[4..])),
            Some((
                AddonFormat::Xdelta,
                AddonKey::SourceName {
                    name: "Game (USA).iso".into()
                }
            ))
        );
        let x = t::xdelta(Some("out.iso//C:\\roms\\Game (USA).iso/"));
        assert_eq!(
            sniff_patch(&x[..4], None, Some(&x[4..])),
            Some((
                AddonFormat::Xdelta,
                AddonKey::SourceName {
                    name: "Game (USA).iso".into()
                }
            ))
        );
        for app in ["out.iso//../", "out.iso//", "out.iso", "out.iso///"] {
            let x = t::xdelta(Some(app));
            assert_eq!(
                sniff_patch(&x[..4], None, Some(&x[4..])),
                Some((AddonFormat::Xdelta, AddonKey::None)),
                "{app}"
            );
        }
        let x = t::xdelta(None);
        assert_eq!(
            sniff_patch(&x[..4], None, Some(&x[4..])),
            Some((AddonFormat::Xdelta, AddonKey::None))
        );
        assert_eq!(
            sniff_patch(&x[..4], None, None),
            Some((AddonFormat::Xdelta, AddonKey::None))
        );
    }

    #[test]
    fn xdelta_skips_the_secondary_compressor_and_code_table() {
        let app = "t.iso//Game (Japan).iso/";
        let mut x = vec![0xD6, 0xC3, 0xC4, 0x00, 0x01 | 0x02 | 0x04, 0x02];
        x.extend(t::vcdiff_int(3));
        x.extend([9, 9, 9]);
        x.extend(t::vcdiff_int(app.len() as u64));
        x.extend(app.as_bytes());
        assert_eq!(
            sniff_patch(&x[..4], None, Some(&x[4..])),
            Some((
                AddonFormat::Xdelta,
                AddonKey::SourceName {
                    name: "Game (Japan).iso".into()
                }
            ))
        );
        // A declared app header longer than the 4 KiB cap is no key.
        let mut x = vec![0xD6, 0xC3, 0xC4, 0x00, 0x04];
        x.extend(t::vcdiff_int(1 << 20));
        x.extend(app.as_bytes());
        assert_eq!(
            sniff_patch(&x[..4], None, Some(&x[4..])),
            Some((AddonFormat::Xdelta, AddonKey::None))
        );
    }

    #[test]
    fn short_or_garbage_input_never_panics() {
        for len in 0..80 {
            let b = t::bps(1, 1);
            let s = &b[..len.min(b.len())];
            let _ = sniff_patch(s, None, None);
            let _ = sniff_patch(s, s.get(..12).and_then(|x| x.try_into().ok()), Some(s));
            let x = t::xdelta(Some("a//b/"));
            let s = &x[..len.min(x.len())];
            let _ = sniff_patch(s, None, s.get(4..));
            let a = t::aps_n64("NSM");
            let _ = sniff_patch(&a[..len.min(a.len())], None, None);
        }
        assert_eq!(
            sniff_patch(b"BPS1", None, None),
            Some((AddonFormat::Bps, AddonKey::None))
        );
        assert_eq!(
            sniff_patch(b"BPS1", Some([0; 12]), None),
            Some((AddonFormat::Bps, AddonKey::None)),
            "no source size, no key"
        );
        assert_eq!(sniff_patch(b"not a patch", None, None), None);
        assert_eq!(sniff_patch(b"", None, None), None);
    }

    #[test]
    fn ips_chain_names() {
        assert_eq!(ips_chain_index("Game.ips"), Some(0));
        assert_eq!(ips_chain_index("Game.IPS3"), Some(3));
        assert_eq!(ips_chain_index("Game.ips1"), Some(1));
        assert_eq!(ips_chain_index("Game.ips9"), Some(9));
        assert_eq!(ips_chain_index("Game.ips0"), None);
        assert_eq!(ips_chain_index("Game.ips10"), None);
        assert_eq!(ips_chain_index("Game.bps"), None);
        assert_eq!(ips_chain_index("ips"), None);
    }
}
