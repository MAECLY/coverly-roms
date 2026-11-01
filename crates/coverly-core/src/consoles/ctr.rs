//! Nintendo 3DS images and packages (spec §8.2): NCSD (`.3ds`, `.cci`),
//! NCCH (`.cxi`), CIA, 3DSX homebrew and Azahar's zstd `Z3DS` wrappers
//! (`.zcci`, `.zcxi`, `.zcia`, `.z3dsx`).
//!
//! Layouts from 3dbrew («NCSD», «NCCH», «ExeFS», «CIA», «Title
//! metadata», «SMDH», «3DSX Format»). Only clear fields are read: the
//! NCSD partition table, the NCCH header (program id, product code, the
//! `NoCrypto` flag), the CIA header and TMD, the CIA meta section's SMDH
//! and, on a `NoCrypto` NCCH only, the ExeFS header and its `icon` file.
//! The ticket (which holds the encrypted title key) is never read and no
//! content is decrypted (D4): an encrypted image yields its product code
//! or title id and `encrypted: Some(true)`.
//!
//! `Z3DS` (Azahar `src/common/file_derived.h` at `6e42de80`, format read,
//! no code copied): a 0x20-byte header (`Z3DS`, the underlying magic
//! `NCSD`/`NCCH`/`CIA\0`/`3DSX` at 4, version 1 at 8, header size u16 at
//! 0xA, metadata size u32 at 0xC, compressed and uncompressed sizes u64
//! at 0x10/0x18), the metadata, then zstd seekable frames from
//! `header_size + metadata_size` (`Z3DSReadIOFileImpl::OnZSTDSeek`). The
//! underlying magics are the ones Azahar's loaders pass to the writer
//! (`loader/ncch.cpp`, `loader/loader.cpp`, `loader/3dsx.cpp`). Only the
//! first [`MAX_DECODED`] bytes are decoded; offsets in the decoded data
//! are not file offsets, so a `Z3DS` result carries no art locator.

use std::io::Read;

use super::{kind_3ds, ArtFormat, ArtLocator, Budgeted, ConsoleId, IdSource, MAX_DECODED};
use crate::identify::sniff::{ReadAt, SliceAt};

/// NCSD partition offsets and NCCH region offsets are in media units.
const MEDIA: u64 = 0x200;
/// The NCCH header (signature 0x100 + header 0x100).
const NCCH_HEADER: usize = 0x200;
/// `flags[7]` of the NCCH header: content in clear.
const NO_CRYPTO: u8 = 0x04;
/// An SMDH block: magic, titles, settings, small and large icons.
pub const SMDH_LEN: usize = 0x36C0;
/// The CIA archive header size, always 0x2020.
const CIA_HEADER: u32 = 0x2020;
/// The SMDH sits this far into a CIA meta section.
const CIA_META_SMDH: u64 = 0x400;
/// TMD signature type RSA-2048 with SHA-256; its header starts at 0x140.
const TMD_SIG_RSA2048: u32 = 0x0001_0004;
/// TMD header fields, from the start of the TMD.
const TMD_TITLE_ID: usize = 0x18C;
const TMD_VERSION: usize = 0x1DC;
const TMD_CONTENT_COUNT: usize = 0x1DE;
/// The first content chunk record (after 64 content info records).
const TMD_CHUNK0: usize = 0xB04;
/// One content chunk record: id, index, type, size, SHA-256.
const TMD_CHUNK_LEN: usize = 0x30;
/// Content type bit 0: the content is CIA-encrypted with the title key.
const CONTENT_ENCRYPTED: u16 = 0x0001;
/// Largest TMD read whole (64 contents, far more than any game).
const MAX_TMD: u64 = TMD_CHUNK0 as u64 + 64 * TMD_CHUNK_LEN as u64;
/// Most zstd frames walked to cover the underlying header.
const Z3DS_MAX_FRAMES: usize = 4;
/// Largest zstd window accepted inside a `Z3DS` file (Azahar writes
/// 256 KiB frames, 32 MiB for CIA). The decoder never holds more than it
/// was asked for plus one block; this bounds the window reservation.
const Z3DS_MAX_WINDOW: u64 = 32 << 20;

fn le16(h: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(h.get(o..o + 2)?.try_into().ok()?))
}

fn le32(h: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(h.get(o..o + 4)?.try_into().ok()?))
}

fn le64(h: &[u8], o: usize) -> Option<u64> {
    Some(u64::from_le_bytes(h.get(o..o + 8)?.try_into().ok()?))
}

fn be16(h: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_be_bytes(h.get(o..o + 2)?.try_into().ok()?))
}

fn be32(h: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_be_bytes(h.get(o..o + 4)?.try_into().ok()?))
}

fn be64(h: &[u8], o: usize) -> Option<u64> {
    Some(u64::from_be_bytes(h.get(o..o + 8)?.try_into().ok()?))
}

fn align64(x: u64) -> Option<u64> {
    Some(x.checked_add(63)? & !63)
}

/// Whether `[off, off + len)` lies inside the file.
fn fits(b: &Budgeted, off: u64, len: u64) -> bool {
    off.checked_add(len).is_some_and(|end| end <= b.len())
}

/// A 3DS image or package, by its magic; the CIA header only under
/// `.cia` (its magic is a size field, too weak alone).
pub fn identify(b: &mut Budgeted, ext: &str) -> Option<ConsoleId> {
    let n = usize::try_from(b.len().min(0x200)).ok()?;
    if n < 0x20 {
        return None;
    }
    let head = b.read(0, n).ok()?;
    match head.get(0x100..0x104) {
        Some(b"NCSD") => return ncsd(b, &head, "NCSD header"),
        Some(b"NCCH") => return ncch_at(b, 0, "NCCH header"),
        _ => {}
    }
    match head.get(..4)? {
        b"3DSX" => dsx(b, &head),
        b"Z3DS" => z3ds(b, &head),
        _ if ext == "cia" && le32(&head, 0) == Some(CIA_HEADER) => cia(b, &head),
        _ => None,
    }
}

/// The NCCH header at `at`: program id, product code, `NoCrypto`, and on
/// a `NoCrypto` NCCH the ExeFS `icon` (title and art).
fn ncch_at(b: &mut Budgeted, at: u64, detail: &str) -> Option<ConsoleId> {
    if !fits(b, at, NCCH_HEADER as u64) {
        return None;
    }
    let h = b.read(at, NCCH_HEADER).ok()?;
    if &h[0x100..0x104] != b"NCCH" {
        return None;
    }
    let program = le64(&h, 0x118)?;
    let mut id = ConsoleId::proven("3ds", detail);
    id.serial = std::str::from_utf8(&h[0x150..0x160])
        .ok()
        .and_then(|code| crate::disc::serial::normalise_console_serial("3ds", code));
    id.title_id = Some(format!("{program:016X}"));
    id.id_source = Some(IdSource::Header);
    id.kind = kind_3ds(program);
    // 0x112 is the NCCH format version (0 or 2), not the title version;
    // only a CIA's TMD carries that in clear.
    let no_crypto = h[0x188 + 7] & NO_CRYPTO != 0;
    id.encrypted = Some(!no_crypto);
    if no_crypto {
        if let Some(icon) = exefs_icon(b, at, &h) {
            if let Ok(smdh) = b.read(icon, SMDH_LEN) {
                if smdh.starts_with(b"SMDH") {
                    id.title = smdh_title(&smdh);
                    id.art.push(ArtLocator::Range {
                        offset: icon,
                        len: SMDH_LEN as u64,
                        format: ArtFormat::Smdh,
                    });
                }
            }
        }
    }
    Some(id)
}

/// Where the ExeFS `icon` file of a clear NCCH starts, if it is there
/// and at least one SMDH long.
fn exefs_icon(b: &mut Budgeted, at: u64, h: &[u8]) -> Option<u64> {
    let exefs = at.checked_add(u64::from(le32(h, 0x1A0)?).checked_mul(MEDIA)?)?;
    let exefs_len = u64::from(le32(h, 0x1A4)?).checked_mul(MEDIA)?;
    if exefs_len < 0x200 || !fits(b, exefs, 0xA0) {
        return None;
    }
    let table = b.read(exefs, 0xA0).ok()?;
    let entry = table
        .as_chunks::<0x10>()
        .0
        .iter()
        .find(|e| e[..8] == *b"icon\0\0\0\0")?;
    let (off, size) = (u64::from(le32(entry, 8)?), u64::from(le32(entry, 12)?));
    let icon = exefs.checked_add(0x200)?.checked_add(off)?;
    (size >= SMDH_LEN as u64 && fits(b, icon, SMDH_LEN as u64)).then_some(icon)
}

/// NCSD: partition 0's NCCH, else a magic-only verdict.
fn ncsd(b: &mut Budgeted, head: &[u8], detail: &str) -> Option<ConsoleId> {
    let found = le32(head, 0x120)
        .and_then(|p0| u64::from(p0).checked_mul(MEDIA))
        .filter(|&off| off > 0)
        .and_then(|off| ncch_at(b, off, detail));
    found.or_else(|| Some(ConsoleId::magic("3ds", detail)))
}

/// CIA: the TMD title id, version and content 0's encryption bit; when
/// content 0 is not CIA-encrypted, its NCCH header for the product code;
/// the meta section's SMDH for the title and the icon.
fn cia(b: &mut Budgeted, head: &[u8]) -> Option<ConsoleId> {
    let magic = ConsoleId::magic("3ds", "CIA header");
    let sizes = (|| {
        Some((
            u64::from(le32(head, 0x08)?),
            u64::from(le32(head, 0x0C)?),
            u64::from(le32(head, 0x10)?),
            u64::from(le32(head, 0x14)?),
            le64(head, 0x18)?,
        ))
    })();
    let Some((cert, ticket, tmd, meta, content)) = sizes else {
        return Some(magic);
    };
    let offsets = (|| {
        let cert_off = align64(u64::from(CIA_HEADER))?;
        let ticket_off = align64(cert_off.checked_add(cert)?)?;
        let tmd_off = align64(ticket_off.checked_add(ticket)?)?;
        let content_off = align64(tmd_off.checked_add(tmd)?)?;
        let meta_off = align64(content_off.checked_add(content)?)?;
        Some((tmd_off, content_off, meta_off))
    })();
    let Some((tmd_off, content_off, meta_off)) = offsets else {
        return Some(magic);
    };
    let tmd_len = tmd.min(MAX_TMD);
    if tmd_len < (TMD_CHUNK0 + TMD_CHUNK_LEN) as u64 || !fits(b, tmd_off, tmd_len) {
        return Some(magic);
    }
    let Ok(t) = b.read(tmd_off, tmd_len as usize) else {
        return Some(magic);
    };
    if be32(&t, 0) != Some(TMD_SIG_RSA2048) || be16(&t, TMD_CONTENT_COUNT) == Some(0) {
        return Some(magic);
    }
    let Some(tid) = be64(&t, TMD_TITLE_ID) else {
        return Some(magic);
    };
    let encrypted = be16(&t, TMD_CHUNK0 + 6).is_some_and(|ty| ty & CONTENT_ENCRYPTED != 0);
    let mut id = if encrypted {
        let mut id = ConsoleId::proven("3ds", "CIA TMD");
        id.encrypted = Some(true);
        id
    } else {
        // A clear content 0 is an NCCH; its header is in clear even when
        // the NCCH itself is encrypted.
        ncch_at(b, content_off, "CIA TMD").unwrap_or_else(|| ConsoleId::proven("3ds", "CIA TMD"))
    };
    id.title_id = Some(format!("{tid:016X}"));
    id.kind = kind_3ds(tid);
    id.version = be16(&t, TMD_VERSION).map(|v| v.to_string());
    id.id_source = Some(IdSource::Header);
    // Offsets inside a clear content 0 are not offsets of the CIA file
    // when the content is CIA-encrypted; the meta section always is.
    let smdh_off = meta_off.checked_add(CIA_META_SMDH);
    if let Some(smdh_off) = smdh_off.filter(|_| meta >= CIA_META_SMDH + SMDH_LEN as u64) {
        if fits(b, smdh_off, SMDH_LEN as u64) {
            if let Ok(smdh) = b.read(smdh_off, SMDH_LEN) {
                if smdh.starts_with(b"SMDH") {
                    if let Some(title) = smdh_title(&smdh) {
                        id.title = Some(title);
                    }
                    id.art.retain(|a| !matches!(a, ArtLocator::Range { .. }));
                    id.art.push(ArtLocator::Range {
                        offset: smdh_off,
                        len: SMDH_LEN as u64,
                        format: ArtFormat::Smdh,
                    });
                }
            }
        }
    }
    Some(id)
}

/// 3DSX homebrew: the SMDH of the extended header (title and icon); no
/// serial, no DAT. Magic plus a title is 0.90.
fn dsx(b: &mut Budgeted, head: &[u8]) -> Option<ConsoleId> {
    let mut id = ConsoleId::magic("3ds", "3DSX header");
    if le16(head, 4).is_some_and(|hs| hs > 0x20) {
        let (off, size) = (le32(head, 0x20), le32(head, 0x24));
        if let (Some(off), Some(size)) = (off, size) {
            let off = u64::from(off);
            if size as usize >= SMDH_LEN && fits(b, off, SMDH_LEN as u64) {
                if let Ok(smdh) = b.read(off, SMDH_LEN) {
                    if smdh.starts_with(b"SMDH") {
                        id.title = smdh_short_title(&smdh);
                        id.art.push(ArtLocator::Range {
                            offset: off,
                            len: SMDH_LEN as u64,
                            format: ArtFormat::Smdh,
                        });
                    }
                }
            }
        }
    }
    Some(id)
}

/// A reader of the compressed bytes of a `Z3DS` file, sequential, within
/// the budget, stopping at `end`.
struct BudgetRead<'b, 'a> {
    b: &'b mut Budgeted<'a>,
    pos: u64,
    end: u64,
}

impl Read for BudgetRead<'_, '_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let left = self.end.saturating_sub(self.pos);
        let n = usize::try_from(left).map_or(buf.len(), |l| l.min(buf.len()).min(64 << 10));
        if n == 0 {
            return Ok(0);
        }
        let bytes = self
            .b
            .read(self.pos, n)
            .map_err(|e| std::io::Error::other(format!("budgeted read refused: {e:?}")))?;
        buf[..n].copy_from_slice(&bytes);
        self.pos += n as u64;
        Ok(n)
    }
}

/// Decodes up to `want` bytes of the zstd frames starting at `start`.
fn z3ds_prefix(b: &mut Budgeted, start: u64, want: usize) -> Option<Vec<u8>> {
    use ruzstd::decoding::{FrameDecoder, StreamingDecoder};
    let end = b.len();
    let mut src = BudgetRead { b, pos: start, end };
    let mut out: Vec<u8> = Vec::new();
    for _ in 0..Z3DS_MAX_FRAMES {
        if out.len() >= want {
            break;
        }
        let mut frame = FrameDecoder::new();
        frame.set_max_window_size(Z3DS_MAX_WINDOW);
        let Ok(dec) = StreamingDecoder::new_with_decoder(&mut src, frame) else {
            break;
        };
        let before = out.len();
        let limit = u64::try_from(want - before).ok()?;
        if dec.take(limit).read_to_end(&mut out).is_err() {
            out.truncate(before);
            break;
        }
        if out.len() == before {
            break;
        }
    }
    let decoded = u64::try_from(out.len()).ok()?;
    src.b.decoded(decoded).ok()?;
    (!out.is_empty()).then_some(out)
}

/// `Z3DS`: the first decoded bytes of the underlying file, read by the
/// rules above; magic-only when they cannot be decoded.
fn z3ds(b: &mut Budgeted, head: &[u8]) -> Option<ConsoleId> {
    let magic = ConsoleId::magic("3ds", "Z3DS header");
    let parsed = (|| {
        if head.get(8) != Some(&1) {
            return None;
        }
        let underlying: [u8; 4] = head.get(4..8)?.try_into().ok()?;
        let header_size = u64::from(le16(head, 0xA)?);
        let metadata = u64::from(le32(head, 0xC)?);
        let uncompressed = le64(head, 0x18)?;
        let start = header_size.checked_add(metadata)?;
        (header_size >= 0x20 && start < b.len()).then_some((underlying, start, uncompressed))
    })();
    let Some((underlying, start, uncompressed)) = parsed else {
        return Some(magic);
    };
    let left = MAX_DECODED.saturating_sub(b.used().decoded);
    let want = usize::try_from(uncompressed.min(left)).ok()?;
    let Some(data) = z3ds_prefix(b, start, want) else {
        return Some(magic);
    };
    // A wrapper inside a wrapper is not something Azahar writes.
    if data.starts_with(b"Z3DS") {
        return Some(magic);
    }
    let mut inner = SliceAt(&data);
    let len = data.len() as u64;
    let mut ib = Budgeted::new(&mut inner as &mut dyn ReadAt, len);
    let found = match &underlying {
        b"NCSD" | b"NCCH" | b"3DSX" => identify(&mut ib, ""),
        b"CIA\0" => identify(&mut ib, "cia"),
        _ => None,
    };
    let Some(mut id) = found else {
        return Some(magic);
    };
    id.detail = format!("Z3DS {}", id.detail);
    // Offsets in the decoded bytes are not offsets of this file.
    id.art.clear();
    Some(id)
}

/// The title of a retail SMDH: English long, English short, then
/// Japanese (UTF-16LE, 16 languages × short 0x80 / long 0x100 /
/// publisher 0x80 from 0x8). Line breaks become spaces.
pub fn smdh_title(smdh: &[u8]) -> Option<String> {
    smdh_titles(smdh, false)
}

/// The title of a homebrew SMDH: the short field first, since devkitPro
/// and bannertool write the app's name there and its description (by
/// default «Built with devkitARM & libctru») in the long one.
fn smdh_short_title(smdh: &[u8]) -> Option<String> {
    smdh_titles(smdh, true)
}

fn smdh_titles(smdh: &[u8], short_first: bool) -> Option<String> {
    let field = |lang: usize, off: usize, len: usize| -> Option<String> {
        let at = 0x8 + lang * 0x200 + off;
        let raw = smdh.get(at..at + len)?;
        let units: Vec<u16> = raw
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&c| u16::from_le_bytes(c))
            .take_while(|&u| u != 0)
            .collect();
        let text = String::from_utf16_lossy(&units).replace(['\n', '\r'], " ");
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        super::clean_title(&text)
    };
    let (a, b) = if short_first {
        ((0, 0x80), (0x80, 0x100))
    } else {
        ((0x80, 0x100), (0, 0x80))
    };
    field(1, a.0, a.1)
        .or_else(|| field(1, b.0, b.1))
        .or_else(|| field(0, a.0, a.1))
        .or_else(|| field(0, b.0, b.1))
}

/// Spreads the low three bits of `v` to the even bit positions.
fn spread3(v: u32) -> u32 {
    (v & 1) | ((v & 2) << 1) | ((v & 4) << 2)
}

/// The large 48×48 icon of an SMDH (RGB565 at 0x24C0): 8×8 tiles in
/// row-major order, pixels in Morton (Z) order inside a tile (x bits on
/// the even positions, y bits on the odd ones).
pub fn smdh_icon_png(smdh: &[u8]) -> Option<::image::RgbaImage> {
    const SIDE: u32 = 48;
    const AT: usize = 0x24C0;
    let px = smdh.get(AT..AT + (SIDE * SIDE * 2) as usize)?;
    if !smdh.starts_with(b"SMDH") {
        return None;
    }
    let mut img = ::image::RgbaImage::new(SIDE, SIDE);
    for y in 0..SIDE {
        for x in 0..SIDE {
            let tile = (y / 8) * (SIDE / 8) + x / 8;
            let i = tile * 64 + (spread3(x % 8) | (spread3(y % 8) << 1));
            let o = i as usize * 2;
            let v = u16::from_le_bytes([px[o], px[o + 1]]);
            img.put_pixel(x, y, ::image::Rgba(rgb565(v)));
        }
    }
    Some(img)
}

/// RGB565 to RGBA8, each channel widened by repeating its high bits.
pub fn rgb565(v: u16) -> [u8; 4] {
    let (r, g, b) = ((v >> 11) & 0x1F, (v >> 5) & 0x3F, v & 0x1F);
    [
        ((r << 3) | (r >> 2)) as u8,
        ((g << 2) | (g >> 4)) as u8,
        ((b << 3) | (b >> 2)) as u8,
        255,
    ]
}

#[cfg(test)]
mod tests {
    use super::super::testing::{self as t, RangeLog, SparseAt};
    use super::super::{TitleKind, MAX_READ, MAX_SEEKS};
    use super::*;

    fn run(src: &mut SparseAt, ext: &str) -> Option<ConsoleId> {
        let len = src.len;
        identify(&mut Budgeted::new(src, len), ext)
    }

    #[test]
    fn ncsd_reads_partition_zero() {
        let id = run(
            &mut t::ncsd("CTR-P-AMKP", 0x0004_0000_0003_0800, false),
            "3ds",
        )
        .unwrap();
        assert_eq!(id.system, "3ds");
        assert_eq!(id.confidence, 0.95);
        assert_eq!(id.serial.as_deref(), Some("CTR-P-AMKP"));
        assert_eq!(id.title_id.as_deref(), Some("0004000000030800"));
        assert_eq!(id.kind, TitleKind::Base);
        assert_eq!(id.encrypted, Some(true));
        assert_eq!(id.detail, "NCSD header");
        assert!(id.art.is_empty() && id.title.is_none());
    }

    #[test]
    fn ncsd_no_crypto_reads_the_exefs_icon() {
        let mut src = t::ncsd("CTR-P-AMKP", 0x0004_0000_0003_0800, true);
        let id = run(&mut src, "cci").unwrap();
        assert_eq!(id.encrypted, Some(false));
        assert_eq!(id.title.as_deref(), Some("Test Game"));
        assert!(matches!(
            id.art[..],
            [ArtLocator::Range {
                format: ArtFormat::Smdh,
                len,
                ..
            }] if len == SMDH_LEN as u64
        ));
    }

    #[test]
    fn ncch_format_version_is_not_the_title_version() {
        // 0x112 is the NCCH format version (makerom `formatVersion`: 0 or
        // 2 on every retail image), not the title's version.
        let mut src = t::ncsd("CTR-P-AMKP", 0x0004_0000_0003_0800, false);
        src.put(t::NCSD_P0 + 0x112, &2u16.to_le_bytes());
        let id = run(&mut src, "3ds").unwrap();
        assert_eq!(id.version, None);
        // A CIA still gives the TMD title version.
        let id = run(&mut t::cia(0x0004_0000_0005_5D00, false, false), "cia").unwrap();
        assert_eq!(id.version.as_deref(), Some("1040"));
    }

    #[test]
    fn ncch_alone_is_a_cxi() {
        let id = run(
            &mut t::ncch("CTR-P-BP4J", 0x0004_000E_0003_0800, false),
            "cxi",
        )
        .unwrap();
        assert_eq!(
            (id.serial.as_deref(), id.kind, id.detail.as_str()),
            (Some("CTR-P-BP4J"), TitleKind::Update, "NCCH header")
        );
    }

    #[test]
    fn not_a_3ds_image() {
        let mut zeros = SparseAt::new(0x10_0000);
        assert_eq!(run(&mut zeros, "3ds"), None);
        // The CIA header size alone is not enough under another extension.
        let mut cia = t::cia(0x0004_0000_0003_0800, false, false);
        assert_eq!(run(&mut cia, "bin"), None);
    }

    #[test]
    fn budget_adversarial_partition_offset_is_magic_only() {
        let mut src = t::ncsd("CTR-P-AMKP", 0x0004_0000_0003_0800, false);
        src.put(0x120, &0xFFFF_FFFFu32.to_le_bytes());
        let len = src.len;
        let mut b = Budgeted::new(&mut src, len);
        let id = identify(&mut b, "3ds").unwrap();
        assert_eq!((id.confidence, id.serial), (0.90, None));
        assert!(b.used().read <= MAX_READ && b.used().seeks <= MAX_SEEKS);
    }

    #[test]
    fn clear_cia_gives_the_product_code() {
        let id = run(&mut t::cia(0x0004_0000_0005_5D00, false, false), "cia").unwrap();
        assert_eq!(id.serial.as_deref(), Some("CTR-P-EKJA"));
        assert_eq!(id.title_id.as_deref(), Some("0004000000055D00"));
        assert_eq!(
            (id.version.as_deref(), id.detail.as_str()),
            (Some("1040"), "CIA TMD")
        );
    }

    #[test]
    fn encrypted_cia_gives_title_id_only() {
        let mut src = t::cia(0x0004_0000_0005_5D00, true, false);
        let len = src.len;
        let mut log = RangeLog::new(&mut src);
        let id = identify(&mut Budgeted::new(&mut log, len), "cia").unwrap();
        assert_eq!(id.serial, None);
        assert_eq!(id.title_id.as_deref(), Some("0004000000055D00"));
        assert_eq!(id.encrypted, Some(true));
        assert_eq!(id.kind, TitleKind::Base);
        // The ticket (title key) and the encrypted content are never read.
        let (ticket, ticket_len) = t::CIA_TICKET;
        assert!(!log.touched(ticket, ticket_len));
        assert!(!log.touched(t::CIA_CONTENT, 0x200));
    }

    #[test]
    fn cia_meta_title_and_art() {
        let mut src = t::cia(0x0004_0000_0005_5D00, true, true);
        let id = run(&mut src, "cia").unwrap();
        assert_eq!(id.title.as_deref(), Some("Test Game"));
        let [ArtLocator::Range {
            offset,
            len,
            format,
        }] = id.art[..]
        else {
            panic!("{:?}", id.art);
        };
        assert_eq!((len, format), (SMDH_LEN as u64, ArtFormat::Smdh));
        let mut smdh = vec![0u8; SMDH_LEN];
        src.read_at(offset, &mut smdh).unwrap();
        assert!(smdh.starts_with(b"SMDH"));
    }

    #[test]
    fn cia_with_a_bad_tmd_is_magic_only() {
        let mut src = t::cia(0x0004_0000_0005_5D00, false, false);
        src.put(t::CIA_TMD, &[0, 1, 0, 1]);
        let id = run(&mut src, "cia").unwrap();
        assert_eq!((id.confidence, id.title_id), (0.90, None));
    }

    #[test]
    fn dsx_title_from_smdh() {
        let id = run(&mut t::dsx_with_smdh("Homebrew\nLauncher"), "3dsx").unwrap();
        assert_eq!((id.confidence, id.serial.as_deref()), (0.90, None));
        assert_eq!(id.title.as_deref(), Some("Homebrew Launcher"));
        assert_eq!(id.art.len(), 1);
        // No extended header: magic only, no title.
        let mut bare = t::dsx_with_smdh("x");
        bare.put(4, &0x20u16.to_le_bytes());
        let id = run(&mut bare, "3dsx").unwrap();
        assert_eq!((id.title, id.art.len()), (None, 0));
    }

    #[test]
    fn dsx_title_is_the_short_title_not_the_description() {
        // devkitPro writes APP_TITLE to the short field and APP_DESCRIPTION
        // (default «Built with devkitARM & libctru») to the long one.
        let mut src = t::dsx_with_smdh("FTPD");
        let long = 0x100 + 0x8 + 0x200 + 0x80;
        let mut field = vec![0u8; 0x100];
        let d: Vec<u8> = "Built with devkitARM & libctru"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        field[..d.len()].copy_from_slice(&d);
        src.put(long, &field);
        let id = run(&mut src, "3dsx").unwrap();
        assert_eq!(id.title.as_deref(), Some("FTPD"));
    }

    #[test]
    fn z3ds_around_ncsd() {
        let inner = t::ncsd("CTR-P-AMKP", 0x0004_0000_0003_0800, false);
        let mut src = t::z3ds(b"NCSD", &inner);
        let len = src.len;
        let mut b = Budgeted::new(&mut src, len);
        let id = identify(&mut b, "zcci").unwrap();
        assert_eq!(id.serial.as_deref(), Some("CTR-P-AMKP"));
        assert_eq!(id.detail, "Z3DS NCSD header");
        assert!(id.art.is_empty());
        assert!(b.used().decoded <= MAX_DECODED && b.used().decoded > 0);
    }

    #[test]
    fn z3ds_around_cia_and_3dsx() {
        let inner = t::cia(0x0004_0000_0005_5D00, false, false);
        let id = run(&mut t::z3ds(b"CIA\0", &inner), "zcia").unwrap();
        assert_eq!(id.serial.as_deref(), Some("CTR-P-EKJA"));
        let inner = t::dsx_with_smdh("Tool");
        let id = run(&mut t::z3ds(b"3DSX", &inner), "z3dsx").unwrap();
        assert_eq!((id.title.as_deref(), id.art.len()), (Some("Tool"), 0));
    }

    #[test]
    fn z3ds_garbage_is_magic_only() {
        let mut src = SparseAt::new(0x1000);
        let mut h = Vec::from(*b"Z3DSNCSD");
        h.extend([1, 0]);
        h.extend(0x20u16.to_le_bytes());
        h.extend(0u32.to_le_bytes());
        h.extend(0x100u64.to_le_bytes());
        h.extend(0x8000u64.to_le_bytes());
        src.put(0, &h).put(0x20, &[0xAB; 0x100]);
        let id = run(&mut src, "zcci").unwrap();
        assert_eq!((id.confidence, id.detail.as_str()), (0.90, "Z3DS header"));
    }

    #[test]
    fn smdh_titles() {
        let s = t::smdh("Pokémon Y");
        assert_eq!(smdh_title(&s).as_deref(), Some("Pokémon Y"));
        assert_eq!(smdh_title(&vec![0u8; SMDH_LEN]), None);
    }

    #[test]
    fn smdh_tile_order() {
        // Tile 0: pixel slot `i` holds value `i`; tile 1 holds 0xFFFF.
        let mut s = t::smdh("x");
        for i in 0..64u16 {
            let o = 0x24C0 + usize::from(i) * 2;
            s[o..o + 2].copy_from_slice(&i.to_le_bytes());
        }
        for i in 64..128usize {
            let o = 0x24C0 + i * 2;
            s[o..o + 2].copy_from_slice(&0xFFFFu16.to_le_bytes());
        }
        let img = smdh_icon_png(&s).unwrap();
        assert_eq!(img.dimensions(), (48, 48));
        // Morton order: slot 1 = (1, 0), slot 2 = (0, 1), slot 3 = (1, 1),
        // slot 4 = (2, 0), slot 8 = (0, 2), slot 63 = (7, 7).
        for (slot, x, y) in [
            (0u16, 0, 0),
            (1, 1, 0),
            (2, 0, 1),
            (3, 1, 1),
            (4, 2, 0),
            (8, 0, 2),
            (63, 7, 7),
        ] {
            assert_eq!(img.get_pixel(x, y).0, rgb565(slot), "slot {slot}");
        }
        // Tile 1 is the next 8 columns of the first tile row.
        assert_eq!(img.get_pixel(8, 0).0, [255, 255, 255, 255]);
        assert_eq!(img.get_pixel(15, 7).0, [255, 255, 255, 255]);
        assert_eq!(rgb565(0xF800), [255, 0, 0, 255]);
        assert_eq!(smdh_icon_png(&s[..0x100]), None);
    }
}
