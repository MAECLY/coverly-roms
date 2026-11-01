//! `PARAM.SFO` (PSF), the plaintext metadata file of PS3, PS Vita (and
//! later PS4) titles (spec §8.4). Source: psdevwiki «PARAM.SFO».
//!
//! Layout, all little-endian: magic `\0PSF` at 0, version at 4, key-table
//! start at 8, data-table start at 0xC, entry count at 0x10, then one
//! 16-byte entry per key from 0x14 (key offset u16, `fmt` u16, length
//! u32, max length u32, data offset u32). `fmt` is `0x0004` (UTF-8
//! special, not NUL-terminated), `0x0204` (UTF-8) or `0x0404` (u32).
//!
//! The whole file is untrusted: at most [`MAX_META_FILE`] bytes, 256
//! entries, 1 KiB per value, ASCII keys, and every offset checked. Out of
//! bounds is "not a PSF", never a partial answer.

use super::MAX_META_FILE;

/// Most entries a PSF may declare.
const MAX_ENTRIES: usize = 256;
/// Longest value read.
const MAX_VALUE: usize = 1024;
/// The fixed header before the entry table.
const HEADER: usize = 0x14;
/// One entry of the entry table.
const ENTRY: usize = 16;

/// A parsed `PARAM.SFO`: its entries in table order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Psf {
    pub entries: Vec<(String, PsfValue)>,
}

/// One value of a [`Psf`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PsfValue {
    Text(String),
    Int(u32),
}

impl Psf {
    /// The text value of `key`, trimmed; `None` when absent, empty or an
    /// integer.
    pub fn text(&self, key: &str) -> Option<&str> {
        self.entries.iter().find_map(|(k, v)| match v {
            PsfValue::Text(t) if k == key => Some(t.trim()).filter(|t| !t.is_empty()),
            _ => None,
        })
    }

    /// The integer value of `key`.
    pub fn int(&self, key: &str) -> Option<u32> {
        self.entries.iter().find_map(|(k, v)| match v {
            PsfValue::Int(n) if k == key => Some(*n),
            _ => None,
        })
    }
}

fn le16(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        b.get(o..o.checked_add(2)?)?.try_into().ok()?,
    ))
}

fn le32(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        b.get(o..o.checked_add(4)?)?.try_into().ok()?,
    ))
}

/// Parses a whole `PARAM.SFO`; `None` when it is not one or any bound
/// fails. Entries of an unknown `fmt` are skipped.
pub fn parse(b: &[u8]) -> Option<Psf> {
    if b.len() as u64 > MAX_META_FILE || b.get(..4)? != b"\0PSF" {
        return None;
    }
    let key_start = usize::try_from(le32(b, 8)?).ok()?;
    let data_start = usize::try_from(le32(b, 0xC)?).ok()?;
    let count = usize::try_from(le32(b, 0x10)?).ok()?;
    if count > MAX_ENTRIES || key_start > b.len() || data_start > b.len() {
        return None;
    }
    let mut entries = Vec::with_capacity(count);
    for i in 0..count {
        let e = HEADER.checked_add(i.checked_mul(ENTRY)?)?;
        let key_off = usize::from(le16(b, e)?);
        let fmt = le16(b, e + 2)?;
        let len = usize::try_from(le32(b, e + 4)?).ok()?;
        let data_off = usize::try_from(le32(b, e + 12)?).ok()?;
        let key_bytes = b.get(key_start.checked_add(key_off)?..)?;
        let key_end = key_bytes.iter().position(|&c| c == 0)?;
        let key = std::str::from_utf8(&key_bytes[..key_end]).ok()?;
        if key.is_empty() || !key.is_ascii() || len > MAX_VALUE {
            return None;
        }
        let at = data_start.checked_add(data_off)?;
        let raw = b.get(at..at.checked_add(len)?)?;
        let value = match fmt {
            0x0404 => PsfValue::Int(le32(raw, 0)?),
            0x0004 | 0x0204 => PsfValue::Text(
                String::from_utf8_lossy(raw)
                    .trim_end_matches('\0')
                    .to_string(),
            ),
            _ => continue,
        };
        entries.push((key.to_string(), value));
    }
    Some(Psf { entries })
}

#[cfg(test)]
mod tests {
    use super::super::testing::param_sfo;
    use super::*;

    fn sample() -> Vec<u8> {
        param_sfo(&[
            ("CATEGORY", "DG"),
            ("TITLE", "Demon's Souls"),
            ("TITLE_ID", "BLUS30443"),
            ("APP_VER", "01.00"),
        ])
    }

    #[test]
    fn parses_text_and_int_values() {
        let psf = parse(&sample()).unwrap();
        assert_eq!(psf.text("TITLE_ID"), Some("BLUS30443"));
        assert_eq!(psf.text("TITLE"), Some("Demon's Souls"));
        assert_eq!(psf.text("CATEGORY"), Some("DG"));
        assert_eq!(psf.text("APP_VER"), Some("01.00"));
        // `param_sfo` adds `PARENTAL_LEVEL` as a u32 (fmt 0x0404).
        assert_eq!(psf.int("PARENTAL_LEVEL"), Some(5));
        assert_eq!(psf.text("PARENTAL_LEVEL"), None);
        assert_eq!(psf.text("VERSION"), None);
    }

    /// Byte offsets of the [`param_sfo`] layout, for the bound tests.
    fn entry(i: usize) -> usize {
        HEADER + i * ENTRY
    }

    #[test]
    fn every_bound_violation_is_not_a_psf() {
        let good = sample();
        assert!(parse(&good).is_some());
        let patch = |at: usize, bytes: &[u8]| {
            let mut b = good.clone();
            b[at..at + bytes.len()].copy_from_slice(bytes);
            b
        };
        // Count 257.
        assert_eq!(parse(&patch(0x10, &257u32.to_le_bytes())), None);
        // Key table start past the end.
        assert_eq!(parse(&patch(8, &0xFFFF_0000u32.to_le_bytes())), None);
        // Key offset past the end.
        assert_eq!(parse(&patch(entry(0), &0xFFF0u16.to_le_bytes())), None);
        // Length 1025.
        assert_eq!(parse(&patch(entry(0) + 4, &1025u32.to_le_bytes())), None);
        // Data offset past the end.
        assert_eq!(
            parse(&patch(entry(0) + 12, &0x00FF_FFFFu32.to_le_bytes())),
            None
        );
        // Data table start past the end.
        assert_eq!(parse(&patch(0xC, &0xFFFF_0000u32.to_le_bytes())), None);
        // Not the magic.
        assert_eq!(parse(&patch(0, b"\0PSX")), None);
        // Over 64 KiB.
        let mut big = good.clone();
        big.resize((MAX_META_FILE + 1) as usize, 0);
        assert_eq!(parse(&big), None);
        // Truncated inside the entry table.
        assert_eq!(parse(&good[..0x20]), None);
    }

    #[test]
    fn a_non_ascii_key_is_not_a_psf() {
        let mut b = sample();
        let key_start = le32(&b, 8).unwrap() as usize;
        b[key_start] = 0xC3;
        b[key_start + 1] = 0xA9;
        assert_eq!(parse(&b), None);
    }

    #[test]
    fn unknown_fmt_is_skipped_and_special_text_is_read() {
        let mut b = sample();
        // Entry 0 (CATEGORY) gets an unknown fmt; entry 1 (TITLE) the
        // «special» UTF-8 form without a NUL.
        b[entry(0) + 2..entry(0) + 4].copy_from_slice(&0x0104u16.to_le_bytes());
        b[entry(1) + 2..entry(1) + 4].copy_from_slice(&0x0004u16.to_le_bytes());
        let psf = parse(&b).unwrap();
        assert_eq!(psf.text("CATEGORY"), None);
        assert_eq!(psf.text("TITLE"), Some("Demon's Souls"));
        assert_eq!(psf.text("TITLE_ID"), Some("BLUS30443"));
    }

    #[test]
    fn empty_table_parses() {
        let psf = parse(&param_sfo(&[])).unwrap();
        assert_eq!(psf.entries.len(), 1, "only PARENTAL_LEVEL");
    }
}
