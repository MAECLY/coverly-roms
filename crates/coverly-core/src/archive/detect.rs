//! Container detection by magic (spec `2026-10-23-organize-archives-and-discs.md`
//! §3, owner decision D1).
//!
//! Magic wins over the extension; the extension is only a fallback
//! ([`by_extension`]) for formats without magic (v7 tar, LZMA alone) and for
//! prefixed zips whose end record the caller did not read. Every magic is
//! validated a little further than its first bytes (reserved bits, header
//! CRC, tar checksum, EOCD offsets) so that plain text or a ROM that happens
//! to start with the same bytes is not taken for an archive.
//!
//! Detection only looks at bytes the caller already has: the 128 KiB prefix
//! read by `inspect_file` and, optionally, the file's tail (last 64 KiB + 22
//! bytes) for the zip end-of-central-directory record. Nothing here reads a
//! file or allocates in proportion to the input.

use crate::identify::sniff::ContainerMagic;

use super::{MAX_ENTRY_SIZE, MAX_HEADER};

/// Bytes of tail a caller should pass to find a zip EOCD: the record (22)
/// plus the longest comment (65 535), rounded to 64 KiB + 22.
pub const EOCD_TAIL: usize = 64 * 1024 + 22;

const EOCD_SIG: &[u8; 4] = b"PK\x05\x06";
const EOCD_LEN: usize = 22;
const ZIP64_LOCATOR_SIG: &[u8; 4] = b"PK\x06\x07";
const ZIP64_LOCATOR_LEN: usize = 20;
const SEVENZ_SIG: [u8; 6] = [0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C];
const XZ_SIG: [u8; 6] = [0xFD, b'7', b'z', b'X', b'Z', 0x00];
const BZ_BLOCK: [u8; 6] = [0x31, 0x41, 0x59, 0x26, 0x53, 0x59];
const BZ_END: [u8; 6] = [0x17, 0x72, 0x45, 0x38, 0x50, 0x90];
const TAR_BLOCK: usize = 512;
/// Largest ARJ basic header (ARJ technical notes).
const ARJ_MAX_HEADER: u16 = 2600;
/// LZMA properties byte: `(pb * 5 + lp) * 9 + lc` with pb ≤ 4, lp ≤ 4, lc ≤ 8.
const LZMA_PROPS_MAX: u8 = 225;
const LZMA_ALONE_HEADER: usize = 13;

/// `n` bytes at `offset` of `buf`, if present.
fn at(buf: &[u8], offset: usize, n: usize) -> Option<&[u8]> {
    buf.get(offset..offset.checked_add(n)?)
}

fn starts(buf: &[u8], offset: usize, magic: &[u8]) -> bool {
    at(buf, offset, magic.len()) == Some(magic)
}

fn u16_le(buf: &[u8], offset: usize) -> Option<u16> {
    at(buf, offset, 2).map(|b| u16::from_le_bytes([b[0], b[1]]))
}

fn u32_le(buf: &[u8], offset: usize) -> Option<u32> {
    at(buf, offset, 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn u64_le(buf: &[u8], offset: usize) -> Option<u64> {
    at(buf, offset, 8).map(|b| {
        let mut a = [0u8; 8];
        a.copy_from_slice(b);
        u64::from_le_bytes(a)
    })
}

/// The container `head` (the first bytes of a file of `len` bytes) opens
/// with, by magic (spec §3.1). `tail` is the end of the same file (at most
/// [`EOCD_TAIL`] bytes, ending at EOF); with it a zip with a prefix (SFX
/// stub, junk) is found by its end record. Without it only magic at the
/// start counts: the caller passes the head again as `tail` when it holds
/// the whole file.
///
/// Formats without magic (v7 tar, LZMA alone) are never returned here: they
/// come from [`by_extension`] confirmed by [`tar_header_ok`] or
/// [`lzma_alone_plausible`].
pub fn detect_container(head: &[u8], len: u64, tail: Option<&[u8]>) -> Option<ContainerMagic> {
    by_magic(head).or_else(|| tail.and_then(|t| zip_by_eocd(t, len)))
}

/// Magic at the start of the file, most specific first; the two-byte
/// magics (ARJ, Unix compress) come last and are validated further.
fn by_magic(h: &[u8]) -> Option<ContainerMagic> {
    use ContainerMagic as M;
    if starts(h, 0, b"PK\x03\x04") || starts(h, 0, EOCD_SIG) {
        return Some(M::Zip);
    }
    if starts(h, 0, b"PK\x07\x08") {
        return Some(M::ZipSpanned);
    }
    if starts(h, 0, &SEVENZ_SIG) {
        // Format version major 0 (every 7-Zip release so far).
        return (at(h, 6, 2).is_some_and(|v| v[0] == 0)).then_some(M::SevenZip);
    }
    if starts(h, 0, b"Rar!\x1a\x07\x00") {
        return Some(M::Rar4);
    }
    if starts(h, 0, b"Rar!\x1a\x07\x01\x00") {
        return Some(M::Rar5);
    }
    if starts(h, 0, b"MComprHD") {
        return Some(M::Chd);
    }
    if starts(h, 0, &XZ_SIG) {
        return xz_stream_header_ok(h).then_some(M::Xz);
    }
    if starts(h, 0, &[0x1F, 0x8B, 0x08]) {
        // FLG bits 5–7 are reserved and must be zero (RFC 1952 §2.3.1).
        return (at(h, 3, 1).is_some_and(|f| f[0] & 0xE0 == 0)).then_some(M::Gzip);
    }
    if starts(h, 0, b"BZh") {
        let level_ok = at(h, 3, 1).is_some_and(|l| (b'1'..=b'9').contains(&l[0]));
        let block_ok = starts(h, 4, &BZ_BLOCK) || starts(h, 4, &BZ_END);
        return (level_ok && block_ok).then_some(M::Bzip2);
    }
    if is_skippable(h, 0) {
        return after_skippable(h);
    }
    if let Some(frame) = zstd_or_lz4(h, 0) {
        return frame;
    }
    if starts(h, 257, b"ustar\x0000") || starts(h, 257, b"ustar  \0") {
        return tar_header_ok(h).then_some(M::Tar);
    }
    if starts(h, 0, b"MSCF") {
        // CFHEADER.reserved1 is always zero.
        return (u32_le(h, 4) == Some(0)).then_some(M::Cab);
    }
    if starts(h, 0, b"ECM\0") {
        return Some(M::Ecm);
    }
    if lzh_method(h) {
        return Some(M::Lzh);
    }
    if starts(h, 0, &[0x60, 0xEA]) {
        return arj_header_ok(h).then_some(M::Arj);
    }
    if starts(h, 0, &[0x1F, 0x9D]) {
        // Flags: bits 5–6 reserved, low 5 bits = max code width 9..=16.
        return at(h, 2, 1)
            .is_some_and(|f| f[0] & 0x60 == 0 && (9..=16).contains(&(f[0] & 0x1F)))
            .then_some(M::Compress);
    }
    None
}

/// A zstd/lz4 skippable frame (magic 0x184D2A50..=0x184D2A5F) at `pos`.
fn is_skippable(h: &[u8], pos: usize) -> bool {
    at(h, pos, 4).is_some_and(|m| m[0] & 0xF0 == 0x50 && m[1..] == [0x2A, 0x4D, 0x18])
}

/// The frame at `pos`, if it opens with zstd or lz4 magic: `Some(None)`
/// when the magic is there but its header check fails.
fn zstd_or_lz4(h: &[u8], pos: usize) -> Option<Option<ContainerMagic>> {
    use ContainerMagic as M;
    if starts(h, pos, &[0x28, 0xB5, 0x2F, 0xFD]) {
        // Frame header descriptor bit 3 is reserved (RFC 8878 §3.1.1.1.1).
        return Some(
            at(h, pos + 4, 1)
                .is_some_and(|d| d[0] & 0x08 == 0)
                .then_some(M::Zstd),
        );
    }
    if starts(h, pos, &[0x04, 0x22, 0x4D, 0x18]) {
        // FLG version bits (7–6) must be `01` (LZ4 frame format §3).
        return Some(
            at(h, pos + 4, 1)
                .is_some_and(|f| f[0] >> 6 == 0b01)
                .then_some(M::Lz4),
        );
    }
    if starts(h, pos, &[0x02, 0x21, 0x4C, 0x18]) {
        return Some(Some(M::Lz4));
    }
    None
}

/// Most skippable frames walked past before deciding.
const MAX_SKIPPABLE: usize = 16;

/// Skippable frames come before zstd and lz4 frames alike: the first frame
/// after them (within `h`) decides. When the head ends first, zstd (the
/// common case) is assumed and the reader decides (spec §3.2 rule 3).
fn after_skippable(h: &[u8]) -> Option<ContainerMagic> {
    let mut pos = 0usize;
    for _ in 0..MAX_SKIPPABLE {
        if !is_skippable(h, pos) {
            return zstd_or_lz4(h, pos).unwrap_or(None);
        }
        let Some(size) = u32_le(h, pos + 4) else {
            break;
        };
        match usize::try_from(size)
            .ok()
            .and_then(|n| pos.checked_add(8)?.checked_add(n))
        {
            Some(next) if next < h.len() => pos = next,
            _ => break,
        }
    }
    Some(ContainerMagic::Zstd)
}

/// An ARJ main header (ARJ technical notes): basic header size within
/// [`ARJ_MAX_HEADER`], followed by the CRC32 of the basic header. Two
/// bytes of magic alone are what raw 6502/68000 code can start with.
fn arj_header_ok(h: &[u8]) -> bool {
    let Some(size) = u16_le(h, 2) else {
        return false;
    };
    if !(1..=ARJ_MAX_HEADER).contains(&size) {
        return false;
    }
    let size = usize::from(size);
    match (at(h, 4, size), u32_le(h, 4 + size)) {
        (Some(basic), Some(crc)) => crc32fast::hash(basic) == crc,
        _ => false,
    }
}

/// The xz stream header: flags byte 0 zero, byte 1 high nibble zero, and
/// the CRC32 of the two flag bytes (xz file format §2.1.1).
fn xz_stream_header_ok(h: &[u8]) -> bool {
    let (Some(flags), Some(crc)) = (at(h, 6, 2), u32_le(h, 8)) else {
        return false;
    };
    flags[0] == 0 && flags[1] & 0xF0 == 0 && crc32fast::hash(flags) == crc
}

/// LHA/LZH method id `-lh?-` / `-lz?-` at offset 2.
fn lzh_method(h: &[u8]) -> bool {
    let Some(m) = at(h, 2, 5) else {
        return false;
    };
    m[0] == b'-'
        && m[1] == b'l'
        && matches!(m[2], b'h' | b'z')
        && (m[3].is_ascii_digit() || m[3].is_ascii_lowercase())
        && m[4] == b'-'
}

/// A zip end-of-central-directory record that ends the file, found in
/// `tail` (the last bytes of a `len`-byte file). Its central directory must
/// lie inside the file, before the record (spec §3.1 "EOCD offsets inside
/// the file"). Disk numbers other than 0 mark a multi-volume part.
fn zip_by_eocd(tail: &[u8], len: u64) -> Option<ContainerMagic> {
    let tail_len = u64::try_from(tail.len()).ok()?;
    if tail_len > len || tail.len() < EOCD_LEN {
        return None;
    }
    let base = len - tail_len;
    // The record ends the file; the comment length tells where it starts.
    // Search from the end, closest record first, within one comment's reach.
    let lowest = tail.len().saturating_sub(EOCD_TAIL);
    let mut pos = tail.len() - EOCD_LEN;
    loop {
        if starts(tail, pos, EOCD_SIG) {
            if let Some(magic) = eocd_at(tail, pos, base) {
                return Some(magic);
            }
        }
        if pos == lowest {
            return None;
        }
        pos -= 1;
    }
}

/// The EOCD record at `pos` of `tail` (`base` = absolute offset of `tail`).
fn eocd_at(tail: &[u8], pos: usize, base: u64) -> Option<ContainerMagic> {
    let disk = u16_le(tail, pos + 4)?;
    let cd_disk = u16_le(tail, pos + 6)?;
    let cd_size = u32_le(tail, pos + 12)?;
    let cd_offset = u32_le(tail, pos + 16)?;
    let comment = usize::from(u16_le(tail, pos + 20)?);
    if pos + EOCD_LEN + comment != tail.len() {
        return None;
    }
    let eocd_abs = base + u64::try_from(pos).ok()?;
    // APPNOTE 4.4.1.4: a field moved to the zip64 record may read -1,
    // disk numbers included; the locator then holds the disk count.
    let zip64 =
        cd_size == u32::MAX || cd_offset == u32::MAX || disk == u16::MAX || cd_disk == u16::MAX;
    if zip64 {
        return zip64_locator(tail, pos, base);
    }
    if disk != 0 || cd_disk != 0 {
        return Some(ContainerMagic::ZipSpanned);
    }
    let cd_size = u64::from(cd_size);
    let cd_offset = u64::from(cd_offset);
    if cd_size > MAX_HEADER || cd_size > eocd_abs {
        return None;
    }
    // Offsets are relative to the zip proper; with a prefix the directory
    // sits later in the file than it says, never earlier.
    let cd_start = eocd_abs - cd_size;
    (cd_offset <= cd_start).then_some(ContainerMagic::ZipEocdOnly)
}

/// The zip64 locator right before the EOCD at `pos` of `tail`: the zip64
/// record it points at must start before it, and more than one disk (or a
/// record on a later disk) marks a part of a set. The reader validates the
/// record itself.
fn zip64_locator(tail: &[u8], pos: usize, base: u64) -> Option<ContainerMagic> {
    let locator = pos.checked_sub(ZIP64_LOCATOR_LEN)?;
    if !starts(tail, locator, ZIP64_LOCATOR_SIG) {
        return None;
    }
    let record_disk = u32_le(tail, locator + 4)?;
    let record_offset = u64_le(tail, locator + 8)?;
    let total_disks = u32_le(tail, locator + 16)?;
    if record_offset >= base + u64::try_from(locator).ok()? {
        return None;
    }
    if record_disk != 0 || total_disks > 1 {
        return Some(ContainerMagic::ZipSpanned);
    }
    Some(ContainerMagic::ZipEocdOnly)
}

/// Whether the first 512 bytes of `h` are a tar header with a valid
/// checksum (offset 148, octal). This is all a v7 tar has to show, so it
/// confirms a `.tar` extension; ustar magic alone is never trusted.
pub fn tar_header_ok(h: &[u8]) -> bool {
    let Some(block) = at(h, 0, TAR_BLOCK) else {
        return false;
    };
    let Some(stored) = parse_octal(&block[148..156]) else {
        return false;
    };
    let field_as_spaces = 8 * u64::from(b' ');
    let (mut unsigned, mut signed) = (field_as_spaces, field_as_spaces as i64);
    for (i, &b) in block.iter().enumerate() {
        if !(148..156).contains(&i) {
            unsigned += u64::from(b);
            signed += i64::from(b as i8);
        }
    }
    // Old tars summed signed bytes; accept either (POSIX tar rationale).
    stored == unsigned || i64::try_from(stored).is_ok_and(|s| s == signed)
}

/// An octal number padded with spaces and ended by NUL or space.
fn parse_octal(field: &[u8]) -> Option<u64> {
    let digits: &[u8] = {
        let start = field.iter().position(|&b| b != b' ')?;
        let rest = &field[start..];
        let end = rest
            .iter()
            .position(|&b| b == 0 || b == b' ')
            .unwrap_or(rest.len());
        &rest[..end]
    };
    if digits.is_empty() {
        return None;
    }
    digits.iter().try_fold(0u64, |acc, &d| {
        (b'0'..=b'7')
            .contains(&d)
            .then(|| acc * 8 + u64::from(d - b'0'))
    })
}

/// Whether `head` (of a `len`-byte file named `.lzma`) has a plausible
/// LZMA-alone header: properties byte below 225, a dictionary within
/// `dict_cap`, and a declared size that is unknown (`u64::MAX`) or within
/// [`MAX_ENTRY_SIZE`]. The format has no magic, so this only confirms the
/// extension (spec §3.1). The decoder still checks the dictionary itself
/// (S2, S3); this never lowers it.
pub fn lzma_alone_plausible(head: &[u8], len: u64, dict_cap: u64) -> bool {
    if len < LZMA_ALONE_HEADER as u64 {
        return false;
    }
    let (Some(props), Some(dict), Some(size)) = (at(head, 0, 1), u32_le(head, 1), u64_le(head, 5))
    else {
        return false;
    };
    props[0] < LZMA_PROPS_MAX
        && u64::from(dict) <= dict_cap
        && (size == u64::MAX || size <= MAX_ENTRY_SIZE)
}

/// The container an extension proposes (spec §3.1 fallback column), case
/// insensitive. Used when the bytes have no magic: the format's reader then
/// decides (spec §3.2 rule 3).
pub fn by_extension(ext: &str) -> Option<ContainerMagic> {
    use ContainerMagic as M;
    let ext = ext.to_ascii_lowercase();
    Some(match ext.as_str() {
        "zip" | "zipx" => M::Zip,
        "7z" => M::SevenZip,
        "rar" | "r00" | "cbr" => M::Rar4,
        "gz" | "tgz" | "gzip" => M::Gzip,
        "bz2" | "tbz" | "tbz2" => M::Bzip2,
        "xz" | "txz" => M::Xz,
        "zst" | "tzst" => M::Zstd,
        "lz4" => M::Lz4,
        "tar" => M::Tar,
        "lzma" => M::LzmaAlone,
        "chd" => M::Chd,
        "cab" => M::Cab,
        "arj" => M::Arj,
        "lzh" | "lha" => M::Lzh,
        "z" => M::Compress,
        "ecm" => M::Ecm,
        _ => return None,
    })
}

/// One file of a multi-volume set, by name (spec §3.1, §3.2 rule 6).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct VolumePart {
    /// The part that opens the set (`.7z.001`, `.part1.rar`). Old-style
    /// RAR (`.rar` + `.rNN`) and split zip (`.zNN` + `.zip`) name their
    /// opening part like a plain archive, so it is found by magic or flag,
    /// not here.
    pub first: bool,
    /// The file name without the volume suffix, as written: parts with the
    /// same stem in one folder belong together.
    pub set_stem: String,
}

/// Whether `name` (a file name, not a path) is a volume of a split archive:
/// `.7z.NNN`, `.partN.rar`, `.rNN` or `.zNN` (case insensitive).
pub fn multi_volume_part(name: &str) -> Option<VolumePart> {
    let lower = name.to_ascii_lowercase();
    let part = |first: bool, suffix_len: usize| {
        let stem = &name[..name.len() - suffix_len];
        (!stem.is_empty()).then(|| VolumePart {
            first,
            set_stem: stem.to_string(),
        })
    };
    let (rest, ext) = lower.rsplit_once('.')?;
    // `.7z.001`, `.7z.002`, …: three or more digits, numbered from 1.
    if ext.len() >= 3 && ext.bytes().all(|b| b.is_ascii_digit()) {
        let n: u64 = ext.parse().ok()?;
        if n == 0 || !rest.ends_with(".7z") {
            return None;
        }
        return part(n == 1, ext.len() + 4);
    }
    // `.part1.rar`, `.part01.rar`, …
    if ext == "rar" {
        let (_, vol) = rest.rsplit_once('.')?;
        let digits = vol.strip_prefix("part")?;
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let n: u64 = digits.parse().ok()?;
        if n == 0 {
            return None;
        }
        return part(n == 1, vol.len() + 5);
    }
    // `.r00`–`.r99` follow a `.rar`; `.z01`–`.z99` precede a `.zip`.
    let bytes = ext.as_bytes();
    if bytes.len() == 3 && bytes[1].is_ascii_digit() && bytes[2].is_ascii_digit() {
        let n = (bytes[1] - b'0') * 10 + (bytes[2] - b'0');
        let ok = match bytes[0] {
            b'r' => true,
            b'z' => n >= 1,
            _ => false,
        };
        if ok {
            return part(false, 4);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_octal_forms() {
        assert_eq!(parse_octal(b"0000644\0"), Some(0o644));
        assert_eq!(parse_octal(b"  01234 "), Some(0o1234));
        assert_eq!(parse_octal(b"       \0"), None);
        assert_eq!(parse_octal(b"0000980\0"), None);
    }

    #[test]
    fn eocd_search_stays_within_one_comment() {
        // A record 70 KiB before EOF cannot be the end record: no comment
        // is that long, so the search never looks there.
        let mut file = vec![0u8; 100];
        file.extend_from_slice(EOCD_SIG);
        file.extend_from_slice(&[0u8; 18]);
        file.extend(vec![0x33u8; 70 * 1024]);
        assert_eq!(zip_by_eocd(&file, file.len() as u64), None);
    }
}
