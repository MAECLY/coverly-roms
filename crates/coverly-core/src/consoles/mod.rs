//! Console header readers of the modern systems (spec 2026-10-26 batch 2,
//! §8): GameCube, Wii, Wii U, 3DS, Switch, PS3, PS Vita, Xbox, Xbox 360.
//!
//! Every reader is a pure function over a [`Budgeted`] source and reads
//! **only plaintext metadata** within the D24 caps: at most
//! [`MAX_READ`] bytes read, [`MAX_DECODED`] bytes decompressed and
//! [`MAX_SEEKS`] seeks per item. No reader takes a key, derives one or
//! decrypts anything (D4): key and licence files are recognised by name
//! with [`is_key_material`] and never opened.
//!
//! A reader's answer is a [`ConsoleId`]: the system, the serial in the
//! DAT's form (`disc::serial::normalise_console_serial`), the title id,
//! the kind (base, update, DLC, demo), and where an embedded icon is
//! ([`ArtLocator`], never the bytes).

use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::identify::sniff::ReadAt;

#[cfg(any(test, feature = "test-util"))]
pub mod testing;

/// Most bytes read from one item (D24).
pub const MAX_READ: u64 = 4 << 20;
/// Most bytes decompressed (zlib, zstd) for one item (D24).
pub const MAX_DECODED: u64 = 1 << 20;
/// Most seeks (reads that do not continue the previous one) per item.
pub const MAX_SEEKS: u32 = 16;
/// Largest metadata file read whole (`PARAM.SFO`, `meta.xml`, `cnmt.xml`).
pub const MAX_META_FILE: u64 = 64 << 10;

/// What a title is, from its title id or content type (spec §8.0, D11).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TitleKind {
    Base,
    Update,
    Dlc,
    Demo,
    #[default]
    Unknown,
}

/// The encoding of an embedded icon.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ArtFormat {
    Png,
    Tga,
    /// A 3DS SMDH block (48×48 RGB565 tiles).
    Smdh,
}

/// Where an embedded icon is; never the bytes (spec D17).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ArtLocator {
    /// A byte range of the file itself (STFS thumbnail, CIA meta SMDH).
    Range {
        offset: u64,
        len: u64,
        format: ArtFormat,
    },
    /// A zip entry (VPK `sce_sys/icon0.png`).
    ZipEntry { name: String, format: ArtFormat },
    /// A file inside a folder game, relative to the folder.
    File { rel: PathBuf, format: ArtFormat },
}

/// Where a title id came from, for the «why» line (spec §10).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IdSource {
    ParamSfo,
    MetaXml,
    AppXml,
    DefaultXbe,
    DefaultXex,
    CnmtXml,
    TicketName,
    FileName,
    Header,
}

/// What a console header reader found (spec §8.0).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConsoleId {
    /// System id (`gamecube`, `3ds`, …).
    pub system: String,
    /// 0.95 magic plus a structure that checked out, 0.90 magic only.
    pub confidence: f32,
    /// The serial in the DAT's form (D22).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serial: Option<String>,
    /// 16 hex (Nintendo), 8 hex (Xbox 360), `AAAA00000` (Sony).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_id: Option<String>,
    /// Plaintext title, cleaned ([`clean_title`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default)]
    pub kind: TitleKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disc_no: Option<u8>,
    /// Content encryption, when the header says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encrypted: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub art: Vec<ArtLocator>,
    /// Where the title id came from, for the «why» line (spec §10).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id_source: Option<IdSource>,
    /// Fixed vocabulary («RVZ disc header», «PFS0 ticket name»…).
    pub detail: String,
}

impl ConsoleId {
    fn with(system: &str, confidence: f32, detail: &str) -> ConsoleId {
        ConsoleId {
            system: system.to_owned(),
            confidence,
            serial: None,
            title_id: None,
            title: None,
            version: None,
            kind: TitleKind::Unknown,
            disc_no: None,
            encrypted: None,
            art: Vec::new(),
            id_source: None,
            detail: detail.to_owned(),
        }
    }

    /// A magic-only verdict (0.90): system known, nothing else read.
    pub fn magic(system: &str, detail: &str) -> ConsoleId {
        Self::with(system, 0.90, detail)
    }

    /// Magic plus a structure that checked out (0.95).
    pub fn proven(system: &str, detail: &str) -> ConsoleId {
        Self::with(system, 0.95, detail)
    }
}

/// Why a [`Budgeted`] read was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadErr {
    /// Past the end of the file (or an offset that overflows).
    Bounds,
    /// Over one of the D24 caps.
    Budget,
    /// The source failed.
    Io,
}

/// What a [`Budgeted`] source has spent so far.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Used {
    pub read: u64,
    pub decoded: u64,
    pub seeks: u32,
}

/// A `ReadAt` that refuses reads past the file length or the D24 caps.
///
/// A read that does not start where the previous one ended counts one
/// seek; the first read at offset 0 does not. A refused read spends
/// nothing.
pub struct Budgeted<'a> {
    inner: &'a mut dyn ReadAt,
    len: u64,
    used: Used,
    next: u64,
}

impl<'a> Budgeted<'a> {
    pub fn new(inner: &'a mut dyn ReadAt, len: u64) -> Self {
        Budgeted {
            inner,
            len,
            used: Used::default(),
            next: 0,
        }
    }

    /// The declared length of the file.
    pub fn len(&self) -> u64 {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn used(&self) -> Used {
        self.used
    }

    /// Exactly `n` bytes at `offset`, or an error (never a short read).
    pub fn read(&mut self, offset: u64, n: usize) -> Result<Vec<u8>, ReadErr> {
        let want = u64::try_from(n).map_err(|_| ReadErr::Bounds)?;
        let end = offset.checked_add(want).ok_or(ReadErr::Bounds)?;
        if end > self.len {
            return Err(ReadErr::Bounds);
        }
        let read = self
            .used
            .read
            .checked_add(want)
            .filter(|&r| r <= MAX_READ)
            .ok_or(ReadErr::Budget)?;
        let seeks = if offset == self.next {
            self.used.seeks
        } else {
            self.used.seeks + 1
        };
        if seeks > MAX_SEEKS {
            return Err(ReadErr::Budget);
        }
        let mut buf = vec![0u8; n];
        let mut got = 0usize;
        while got < n {
            // `got` ≤ `n`, which fits in u64 (checked above).
            let at = offset + got as u64;
            match self.inner.read_at(at, &mut buf[got..]) {
                Ok(0) => return Err(ReadErr::Bounds),
                Ok(k) => got += k,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(_) => return Err(ReadErr::Io),
            }
        }
        self.used.read = read;
        self.used.seeks = seeks;
        self.next = end;
        Ok(buf)
    }

    /// Charges `n` decoded bytes (zlib, zstd) against `MAX_DECODED`.
    pub fn decoded(&mut self, n: u64) -> Result<(), ReadErr> {
        let total = self
            .used
            .decoded
            .checked_add(n)
            .filter(|&d| d <= MAX_DECODED)
            .ok_or(ReadErr::Budget)?;
        self.used.decoded = total;
        Ok(())
    }
}

impl ReadAt for Budgeted<'_> {
    /// Up to `buf.len()` bytes, fewer at the end of the file; a read over
    /// a cap is an error.
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> io::Result<usize> {
        let left = self.len.saturating_sub(offset);
        let n = usize::try_from(left).map_or(buf.len(), |l| l.min(buf.len()));
        if n == 0 {
            return Ok(0);
        }
        let bytes = self
            .read(offset, n)
            .map_err(|e| io::Error::other(format!("budgeted read refused: {e:?}")))?;
        buf[..n].copy_from_slice(&bytes);
        Ok(n)
    }
}

/// Exact file names (lower-case) of key and licence files.
const KEY_NAMES: &[&str] = &[
    "prod.keys",
    "title.keys",
    "dev.keys",
    "aes_keys.txt",
    "seeddb.bin",
    "act.dat",
    "work.bin",
];

/// Extensions (lower-case) of licence and ticket files.
const KEY_EXTS: &[&str] = &["rap", "rif", "dkey", "zrif", "tik", "cetk"];

/// Key or licence files: never opened, hashed, uploaded or moved (D4).
///
/// `keys.txt` is deliberately absent: the name is far too common (notes,
/// readmes, game configs) to treat as key material, and the emulators
/// that read console keys use the exact names listed here.
pub fn is_key_material(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if KEY_NAMES.contains(&lower.as_str()) {
        return true;
    }
    match lower.rsplit_once('.') {
        Some((stem, ext)) => !stem.is_empty() && KEY_EXTS.contains(&ext),
        None => false,
    }
}

/// Every file reader of the batch, in a fixed order; `None` = not a
/// console format this batch knows. `name` is the file name (Switch
/// title-id tokens, spec D10); `""` when unknown.
///
/// The readers join this list, in this order, as their systems land:
/// `nintendo_disc`, `wad`, `ctr`, `nx`, `wiiu`, `sony`, `xbox`. Each is
/// called only when its magic or extension can apply.
pub fn identify(src: &mut Budgeted, ext: &str, name: &str) -> Option<ConsoleId> {
    let _ = (src, ext, name);
    None
}

/// The high 32 bits of a title id.
fn high(title_id: u64) -> u32 {
    (title_id >> 32) as u32
}

/// 3DS title kind by the high word (3dbrew «Titles»: platform 4, content
/// category `0x0` normal, `0x2` demo, `0xE` patch, `0x8C` add-on content).
pub fn kind_3ds(title_id: u64) -> TitleKind {
    match high(title_id) {
        0x0004_0000 => TitleKind::Base,
        0x0004_000E => TitleKind::Update,
        0x0004_008C => TitleKind::Dlc,
        0x0004_0002 => TitleKind::Demo,
        _ => TitleKind::Unknown,
    }
}

/// Wii U title kind by the high word (wiiubrew «Title database»:
/// `00050000` games, `0005000E` updates, `0005000C` DLC, `00050002`
/// demos).
pub fn kind_wiiu(title_id: u64) -> TitleKind {
    match high(title_id) {
        0x0005_0000 => TitleKind::Base,
        0x0005_000E => TitleKind::Update,
        0x0005_000C => TitleKind::Dlc,
        0x0005_0002 => TitleKind::Demo,
        _ => TitleKind::Unknown,
    }
}

/// Wii title kind by the high word: `00010001` downloaded channels
/// (WiiWare, Virtual Console), `00010005` DLC.
pub fn kind_wii(title_id: u64) -> TitleKind {
    match high(title_id) {
        0x0001_0001 => TitleKind::Base,
        0x0001_0005 => TitleKind::Dlc,
        _ => TitleKind::Unknown,
    }
}

/// Switch title kind by the low bits: an application ends in `000` with
/// bit 12 clear, its patch is the application id + `0x800`, and its
/// add-on content is `(id & !0xFFF) + 0x1000 + index` (nxdumptool
/// `TITLE_PATCH_ID_OFFSET`, `TITLE_ADDONCONTENT_ID_OFFSET`).
pub fn kind_switch(title_id: u64) -> TitleKind {
    match title_id & 0x1FFF {
        0 => TitleKind::Base,
        0x800 => TitleKind::Update,
        x if x & 0x1000 != 0 => TitleKind::Dlc,
        _ => TitleKind::Unknown,
    }
}

/// `0100000000010800` → base `0100000000010000` (updates and DLC).
pub fn switch_base(title_id: u64) -> u64 {
    title_id & !0x1FFF
}

/// A plaintext title from a header field: NULs and padding trimmed,
/// controls and bidi overrides removed, at most 128 characters; `None`
/// when nothing printable is left. Used by the readers as they land.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn clean_title(raw: &str) -> Option<String> {
    crate::sources::clean_text(raw.trim_end_matches('\0').trim(), 128)
}

#[cfg(test)]
mod tests {
    use super::testing::{RangeLog, SparseAt};
    use super::*;

    #[test]
    fn is_key_material_list() {
        for name in [
            "prod.keys",
            "TITLE.KEYS",
            "dev.keys",
            "aes_keys.txt",
            "seeddb.bin",
            "act.dat",
            "work.bin",
            "NPUB30024.rap",
            "x.rif",
            "game.dkey",
            "a.zrif",
            "0100.tik",
            "t.cetk",
        ] {
            assert!(is_key_material(name), "{name}");
        }
        for name in [
            "keys.txt",
            "prod.keys.txt",
            "game.iso",
            "title.tmd",
            "work.bin.bak",
            "ticket",
        ] {
            assert!(!is_key_material(name), "{name}");
        }
    }

    #[test]
    fn budgeted_refuses_past_caps() {
        let mut src = SparseAt::new(1 << 40);
        src.put(0, b"HEAD");
        let mut b = Budgeted::new(&mut src, 1 << 40);
        assert_eq!(b.read(0, 4).unwrap(), b"HEAD");
        assert_eq!(b.read((1 << 40) - 2, 4), Err(ReadErr::Bounds));
        assert_eq!(b.read(u64::MAX - 1, 4), Err(ReadErr::Bounds));
        assert_eq!(b.read(8, (MAX_READ + 1) as usize), Err(ReadErr::Budget));
        for i in 0..MAX_SEEKS {
            b.read(u64::from(i) * 4096 + 100, 1).unwrap();
        }
        assert_eq!(b.read(999_999, 1), Err(ReadErr::Budget));
        assert!(b.decoded(MAX_DECODED + 1).is_err());
        assert!(b.used().seeks <= MAX_SEEKS && b.used().read <= MAX_READ);
    }

    #[test]
    fn budgeted_sequential_reads_are_not_seeks() {
        let mut src = SparseAt::new(64);
        src.put(4, b"abcd");
        let mut b = Budgeted::new(&mut src, 64);
        assert_eq!(b.read(0, 4).unwrap(), [0; 4]);
        assert_eq!(b.read(4, 4).unwrap(), b"abcd");
        assert_eq!(b.used().seeks, 0);
        assert_eq!(b.read(16, 2).unwrap(), [0; 2]);
        assert_eq!(
            b.used(),
            Used {
                read: 10,
                decoded: 0,
                seeks: 1
            }
        );
        b.decoded(MAX_DECODED).unwrap();
        assert_eq!(b.decoded(1), Err(ReadErr::Budget));
    }

    #[test]
    fn budgeted_short_source_is_bounds() {
        // The declared length is longer than the bytes behind it.
        let mut src = SparseAt::new(8);
        let mut b = Budgeted::new(&mut src, 16);
        assert_eq!(b.read(4, 8), Err(ReadErr::Bounds));
        assert_eq!(b.used(), Used::default());
    }

    #[test]
    fn sparse_later_put_wins_where_parts_overlap() {
        // A field patched into a header block, at a lower and at a higher
        // offset than the block: the later `put` wins either way.
        let mut src = SparseAt::new(16);
        src.put(4, b"BBBB").put(2, b"aaa").put(7, b"cc");
        let mut buf = [0u8; 10];
        assert_eq!(src.read_at(0, &mut buf).unwrap(), 10);
        assert_eq!(&buf, b"\0\0aaaBBcc\0");
    }

    #[test]
    fn range_log_records_reads() {
        let mut src = SparseAt::new(32);
        src.put(8, b"xy");
        let mut log = RangeLog::new(&mut src);
        let mut buf = [0u8; 4];
        assert_eq!(log.read_at(7, &mut buf).unwrap(), 4);
        assert_eq!(buf, [0, b'x', b'y', 0]);
        assert_eq!(log.read_at(40, &mut buf).unwrap(), 0);
        assert_eq!(log.ranges, vec![(7, 4), (40, 4)]);
    }

    #[test]
    fn title_kinds() {
        assert_eq!(kind_3ds(0x0004_0000_0003_0800), TitleKind::Base);
        assert_eq!(kind_3ds(0x0004_000E_0003_0800), TitleKind::Update);
        assert_eq!(kind_3ds(0x0004_008C_0003_0800), TitleKind::Dlc);
        assert_eq!(kind_3ds(0x0004_0002_0003_0800), TitleKind::Demo);
        assert_eq!(kind_3ds(0x0004_0010_0003_0800), TitleKind::Unknown);
        assert_eq!(kind_wiiu(0x0005_000E_1010_ED00), TitleKind::Update);
        assert_eq!(kind_wiiu(0x0005_0000_1010_ED00), TitleKind::Base);
        assert_eq!(kind_wiiu(0x0005_000C_1010_ED00), TitleKind::Dlc);
        assert_eq!(kind_wiiu(0x0005_0002_1010_ED00), TitleKind::Demo);
        assert_eq!(kind_wii(0x0001_0001_5741_4145), TitleKind::Base);
        assert_eq!(kind_wii(0x0001_0005_5741_4145), TitleKind::Dlc);
        assert_eq!(kind_wii(0x0001_0002_4841_4141), TitleKind::Unknown);
        assert_eq!(kind_switch(0x0100_0000_0001_0000), TitleKind::Base);
        assert_eq!(kind_switch(0x0100_0000_0001_0800), TitleKind::Update);
        assert_eq!(kind_switch(0x0100_0000_0001_1001), TitleKind::Dlc);
        assert_eq!(kind_switch(0x0100_0000_0001_0001), TitleKind::Unknown);
        assert_eq!(switch_base(0x0100_0000_0001_1001), 0x0100_0000_0001_0000);
        assert_eq!(switch_base(0x0100_0000_0001_0800), 0x0100_0000_0001_0000);
    }

    #[test]
    fn identify_knows_nothing_yet() {
        let mut src = SparseAt::new(0x8000);
        let mut b = Budgeted::new(&mut src, 0x8000);
        assert_eq!(identify(&mut b, "iso", "Game.iso"), None);
    }

    #[test]
    fn console_id_verdicts_and_serde() {
        let m = ConsoleId::magic("gamecube", "GCM disc header");
        assert_eq!((m.confidence, m.kind), (0.90, TitleKind::Unknown));
        let mut p = ConsoleId::proven("3ds", "NCSD header");
        assert_eq!(p.confidence, 0.95);
        let bare = serde_json::to_string(&p).unwrap();
        assert_eq!(
            bare,
            r#"{"system":"3ds","confidence":0.95,"kind":"unknown","detail":"NCSD header"}"#
        );
        p.kind = TitleKind::Dlc;
        p.id_source = Some(IdSource::TicketName);
        p.art.push(ArtLocator::Range {
            offset: 0x171A,
            len: 64,
            format: ArtFormat::Png,
        });
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains(r#""kind":"dlc""#), "{json}");
        assert!(json.contains(r#""id_source":"ticket-name""#), "{json}");
        assert!(json.contains(r#""kind":"range""#), "{json}");
        let back: ConsoleId = serde_json::from_str(&json).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn clean_title_trims_padding_and_controls() {
        assert_eq!(
            clean_title("Metroid Prime\0\0\0").as_deref(),
            Some("Metroid Prime")
        );
        assert_eq!(clean_title("  A\u{202E}B \0").as_deref(), Some("AB"));
        assert_eq!(clean_title("\0\0"), None);
    }
}
