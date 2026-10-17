//! Header magic per system (spec §6.3): pure checks over bounded reads.
//! Reads a 128 KiB prefix plus a few targeted offsets; never the whole
//! file. Checks run most specific first inside a family.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};

use serde::{Deserialize, Serialize};

use super::evidence::{Evidence, IdLevel};

/// Bytes read from the start of every candidate.
pub const PREFIX: usize = 128 * 1024;
/// Largest offset reachable by stream-skipping a deflated zip entry.
pub const STREAM_CAP: u64 = 8 * 1024 * 1024;
/// Systems every real dump of which carries the checked signature: a
/// failed sniff is negative evidence for them (O-D5).
pub const DECISIVE: [&str; 8] = ["gb", "gbc", "gba", "nds", "n64", "nes", "fds", "sega32x"];
/// Extensions of games (and disc-set companions) that are text by nature:
/// PICO-8 carts, ScummVM launchers, DOSBox configs and batch files, BASIC
/// listings, cue sheets and playlists. Only a web page is "not a ROM" there.
pub const TEXT_NATIVE_EXTS: [&str; 10] = [
    "p8", "scummvm", "svm", "conf", "bat", "bas", "cue", "m3u", "gdi", "ccd",
];

/// Bounded reads at given offsets.
pub trait ReadAt {
    /// Reads up to `buf.len()` bytes at `offset`; fewer at the end.
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> io::Result<usize>;
}

/// In-memory bytes (tests, already-read prefixes).
pub struct SliceAt<'a>(pub &'a [u8]);

impl ReadAt for SliceAt<'_> {
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> io::Result<usize> {
        let Ok(start) = usize::try_from(offset) else {
            return Ok(0);
        };
        if start >= self.0.len() {
            return Ok(0);
        }
        let n = buf.len().min(self.0.len() - start);
        buf[..n].copy_from_slice(&self.0[start..start + n]);
        Ok(n)
    }
}

/// A raw file: seeks.
pub struct FileAt(pub File);

impl ReadAt for FileAt {
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> io::Result<usize> {
        self.0.seek(SeekFrom::Start(offset))?;
        let mut n = 0;
        while n < buf.len() {
            match self.0.read(&mut buf[n..]) {
                Ok(0) => break,
                Ok(k) => n += k,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        Ok(n)
    }
}

/// A forward-only stream (deflated zip entry): bytes are kept as they are
/// inflated, up to `cap`; offsets beyond the cap read as end of file.
pub struct StreamAt<R: Read> {
    reader: R,
    seen: Vec<u8>,
    cap: u64,
    eof: bool,
}

impl<R: Read> StreamAt<R> {
    pub fn new(reader: R, cap: u64) -> StreamAt<R> {
        StreamAt {
            reader,
            seen: Vec::new(),
            cap,
            eof: false,
        }
    }

    fn fill_to(&mut self, end: u64) -> io::Result<()> {
        let end = end.min(self.cap);
        let mut chunk = [0u8; 64 * 1024];
        while !self.eof && (self.seen.len() as u64) < end {
            let want = ((end - self.seen.len() as u64) as usize).min(chunk.len());
            match self.reader.read(&mut chunk[..want]) {
                Ok(0) => self.eof = true,
                Ok(n) => self.seen.extend_from_slice(&chunk[..n]),
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
}

impl<R: Read> ReadAt for StreamAt<R> {
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> io::Result<usize> {
        self.fill_to(offset.saturating_add(buf.len() as u64))?;
        SliceAt(&self.seen).read_at(offset, buf)
    }
}

/// What a file says it is not.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NotARom {
    WebPage,
    Text,
}

/// Container formats recognised by their magic (contents not read here).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ContainerMagic {
    Zip,
    SevenZip,
    Chd,
    Rar,
}

/// Result of [`sniff`].
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Sniff {
    /// Header evidence (`kind` = [`IdLevel::Header`]).
    pub hits: Vec<Evidence>,
    /// Hard vetoes (e.g. `gb` for a GBC-only cart).
    pub excluded: Vec<String>,
    /// A recognised system Coverly does not organise ("Sega Pico", …).
    pub unsupported: Option<String>,
    pub not_a_rom: Option<NotARom>,
    pub container: Option<ContainerMagic>,
    /// Bytes of prefix actually read (0 = unreadable or empty).
    pub read: usize,
}

impl Sniff {
    fn hit(&mut self, system: &str, weight: f32, detail: impl Into<String>) {
        self.hits
            .push(Evidence::new(IdLevel::Header, system, weight, detail));
    }
}

/// Reads at `offset` until `buf` is full or the source ends.
fn read_exact_at(src: &mut dyn ReadAt, offset: u64, buf: &mut [u8]) -> usize {
    src.read_at(offset, buf).unwrap_or(0)
}

/// `n` bytes at `offset` of `head`, if present.
fn at(head: &[u8], offset: usize, n: usize) -> Option<&[u8]> {
    head.get(offset..offset.checked_add(n)?)
}

fn starts(head: &[u8], offset: usize, magic: &[u8]) -> bool {
    at(head, offset, magic.len()) == Some(magic)
}

/// Header magic of the file behind `src` (`len` bytes, extension `ext`).
pub fn sniff(src: &mut dyn ReadAt, len: u64, ext: &str) -> Sniff {
    let ext = ext.to_ascii_lowercase();
    let mut head = vec![0u8; PREFIX.min(usize::try_from(len).unwrap_or(PREFIX))];
    let n = read_exact_at(src, 0, &mut head);
    head.truncate(n);
    let mut s = Sniff {
        read: n,
        ..Sniff::default()
    };
    if n == 0 {
        return s;
    }
    if let Some(kind) = text_kind(&head) {
        // `[`/`{` also open INI files and configs: under a text-native
        // extension only markup (an HTML error page) is not a game.
        if !TEXT_NATIVE_EXTS.contains(&ext.as_str()) || starts_with_markup(&head) {
            s.not_a_rom = Some(kind);
        }
        return s;
    }
    if let Some(c) = container(&head) {
        s.container = Some(c);
        return s;
    }
    // Disc images first (sector 16), then cartridges, most specific first.
    discs(src, &head, len, &mut s);
    if s.hits.is_empty() && s.unsupported.is_none() {
        cartridges(src, &head, len, &ext, &mut s);
    }
    s
}

/// Printable text (no NUL/control bytes but tab/CR/LF in the first 512
/// bytes, valid UTF-8): a web page when it starts like one.
fn text_kind(head: &[u8]) -> Option<NotARom> {
    let probe = &head[..head.len().min(512)];
    let text = match std::str::from_utf8(probe) {
        Ok(t) => t,
        // A multi-byte character cut at byte 512 is still text.
        Err(e) if e.error_len().is_none() => std::str::from_utf8(&probe[..e.valid_up_to()]).ok()?,
        Err(_) => return None,
    };
    if text.is_empty()
        || text
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\t' | '\r' | '\n'))
    {
        return None;
    }
    let start = text
        .trim_start_matches('\u{feff}')
        .trim_start()
        .to_ascii_lowercase();
    let web = ["<!doctype", "<html", "<?xml", "<head", "<body", "{", "["];
    Some(if web.iter().any(|w| start.starts_with(w)) {
        NotARom::WebPage
    } else {
        NotARom::Text
    })
}

/// Text that opens with `<` (HTML/XML), after a BOM and whitespace.
fn starts_with_markup(head: &[u8]) -> bool {
    let head = head.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(head);
    head.iter().find(|b| !b.is_ascii_whitespace()) == Some(&b'<')
}

fn container(head: &[u8]) -> Option<ContainerMagic> {
    if starts(head, 0, b"PK\x03\x04") || starts(head, 0, b"PK\x05\x06") {
        Some(ContainerMagic::Zip)
    } else if starts(head, 0, &[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C]) {
        Some(ContainerMagic::SevenZip)
    } else if starts(head, 0, b"MComprHD") {
        Some(ContainerMagic::Chd)
    } else if starts(head, 0, b"Rar!\x1a\x07") {
        Some(ContainerMagic::Rar)
    } else {
        None
    }
}

/// Disc images (Task 8 fills this in).
fn discs(_src: &mut dyn ReadAt, _head: &[u8], _len: u64, _s: &mut Sniff) {}

fn cartridges(src: &mut dyn ReadAt, head: &[u8], len: u64, ext: &str, s: &mut Sniff) {
    // NDS before GBA (at the end): NDS cartridges embed a GBA-style area.
    if let Some(h) = at(head, 0, 0x160) {
        let logo_crc = u16::from_le_bytes([h[0x15C], h[0x15D]]);
        let header_crc = u16::from_le_bytes([h[0x15E], h[0x15F]]);
        if logo_crc == 0xCF56 && crc16_modbus(&h[..0x15E]) == header_crc {
            s.hit("nds", 0.95, "NDS header");
            return;
        }
    }
    if let Some(h) = at(head, 0, 0x150) {
        if h[0x104..0x10C] == GB_LOGO && gb_header_checksum(h) == h[0x14D] {
            match h[0x143] {
                0xC0 => {
                    s.hit("gbc", 0.95, "CGB 0xC0");
                    s.excluded.push("gb".into());
                }
                0x80 => s.hit("gbc", 0.85, "CGB 0x80"),
                _ => s.hit("gb", 0.95, "DMG"),
            }
            return;
        }
    }
    if starts(head, 0, b"FDS\x1a")
        || starts(head, 0, b"\x01*NINTENDO-HVC*")
        || starts(head, 16, b"\x01*NINTENDO-HVC*")
    {
        s.hit("fds", 0.95, "FDS");
        return;
    }
    if starts(head, 0, b"NES\x1a") {
        s.hit("nes", 0.95, "iNES");
        return;
    }
    if starts(head, 0, b"UNIF") {
        s.hit("nes", 0.95, "UNIF");
        return;
    }
    for (magic, order) in [
        ([0x80, 0x37, 0x12, 0x40], "z64"),
        ([0x37, 0x80, 0x40, 0x12], "v64"),
        ([0x40, 0x12, 0x37, 0x80], "n64"),
    ] {
        if starts(head, 0, &magic) {
            s.hit("n64", 0.95, order);
            return;
        }
    }
    if starts(head, 0, b"LYNX") {
        s.hit("lynx", 0.95, "LYNX");
        return;
    }
    if starts(head, 1, b"ATARI7800") {
        s.hit("a7800", 0.95, "ATARI7800");
        return;
    }
    if sega(head, len, ext, s) {
        return;
    }
    // Task 7 adds SNES, SNK and Bandai here.
    let _ = src;
    // GBA last: its signature is one fixed byte plus a one-byte complement
    // (16 bits), weaker than any magic above, so a file that carries one of
    // those and matches it by chance keeps its own system.
    if let Some(h) = at(head, 0, 0xC0) {
        if h[0xB2] == 0x96 && gba_complement(h) == h[0xBD] {
            let logo = if starts(h, 0x04, &[0x24, 0xFF, 0xAE, 0x51]) {
                "fixed 0x96, logo"
            } else {
                "fixed 0x96"
            };
            s.hit("gba", 0.95, logo);
        }
    }
}

/// The Sega family, most specific first (spec §6.3): Sega CD, 32X and Pico
/// also carry `SEGA` at 0x100, so they are checked before Mega Drive.
/// Returns whether a signature matched (a hit or the Pico veto).
fn sega(head: &[u8], len: u64, ext: &str, s: &mut Sniff) -> bool {
    if starts(head, 0, b"SEGADISCSYSTEM") || starts(head, 0x10, b"SEGADISCSYSTEM") {
        s.hit("segacd", 0.95, "SEGADISCSYSTEM");
        return true;
    }
    let sega_at_100 = starts(head, 0x100, b"SEGA") || starts(head, 0x101, b"SEGA");
    if starts(head, 0x100, b"SEGA 32X")
        || (sega_at_100 && contains(&head[..0x400.min(head.len())], b"MARS"))
    {
        s.hit("sega32x", 0.95, "SEGA 32X");
        return true;
    }
    if starts(head, 0x100, b"SEGA PICO") {
        s.unsupported = Some("Sega Pico".into());
        return true;
    }
    if sega_at_100 {
        s.hit("megadrive", 0.90, "SEGA at 0x100");
        return true;
    }
    if len % 16384 == 512 && at(head, 8, 2) == Some(&[0xAA, 0xBB][..]) {
        s.hit("megadrive", 0.90, "SMD interleaved");
        return true;
    }
    for offset in [0x7FF0usize, 0x3FF0, 0x1FF0] {
        if starts(head, offset, b"TMR SEGA") {
            // Region nibble: 3/4 SMS, 5/6/7 Game Gear. Only a tiebreak;
            // a `.gg`/`.sms` extension decides.
            let region = head.get(offset + 0xF).map_or(0, |b| b >> 4);
            match ext {
                "gg" => s.hit("gamegear", 0.90, "TMR SEGA"),
                "sms" => s.hit("mastersystem", 0.90, "TMR SEGA"),
                _ => {
                    let (system, other) = if (5..=7).contains(&region) {
                        ("gamegear", "mastersystem")
                    } else {
                        ("mastersystem", "gamegear")
                    };
                    s.hit(system, 0.70, format!("TMR SEGA, region {region}"));
                    s.hit(other, 0.60, "TMR SEGA");
                }
            }
            return true;
        }
    }
    false
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

/// First bytes of the Nintendo logo every Game Boy header carries at 0x104.
const GB_LOGO: [u8; 8] = [0xCE, 0xED, 0x66, 0x66, 0xCC, 0x0D, 0x00, 0x0B];

/// Game Boy header checksum over 0x134..=0x14C.
fn gb_header_checksum(h: &[u8]) -> u8 {
    h[0x134..=0x14C]
        .iter()
        .fold(0u8, |x, b| x.wrapping_sub(*b).wrapping_sub(1))
}

/// GBA complement check over 0xA0..=0xBC.
fn gba_complement(h: &[u8]) -> u8 {
    h[0xA0..=0xBC]
        .iter()
        .fold(0u8, |c, b| c.wrapping_sub(*b))
        .wrapping_sub(0x19)
}

/// CRC-16/MODBUS (init 0xFFFF, reflected poly 0xA001), as the NDS header uses.
pub(crate) fn crc16_modbus(data: &[u8]) -> u16 {
    let mut crc = 0xFFFFu16;
    for &b in data {
        crc ^= u16::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xA001
            } else {
                crc >> 1
            };
        }
    }
    crc
}

/// Synthetic headers for tests: only the bytes the checks read.
#[cfg(any(test, feature = "test-util"))]
pub mod fixtures {
    pub fn zeros(len: usize) -> Vec<u8> {
        vec![0u8; len]
    }

    pub fn nes() -> Vec<u8> {
        let mut b = zeros(16 + 16384);
        b[..4].copy_from_slice(b"NES\x1a");
        b
    }

    pub fn unif() -> Vec<u8> {
        let mut b = zeros(64);
        b[..4].copy_from_slice(b"UNIF");
        b
    }

    pub fn fds(with_header: bool) -> Vec<u8> {
        let mut b = zeros(65500 + 16);
        if with_header {
            b[..4].copy_from_slice(b"FDS\x1a");
            b[16..31].copy_from_slice(b"\x01*NINTENDO-HVC*");
        } else {
            b[..15].copy_from_slice(b"\x01*NINTENDO-HVC*");
        }
        b
    }

    #[derive(Clone, Copy)]
    pub enum N64Order {
        Z64,
        V64,
        N64,
    }

    pub fn n64(order: N64Order) -> Vec<u8> {
        let mut b = zeros(4096);
        b[..4].copy_from_slice(match order {
            N64Order::Z64 => &[0x80, 0x37, 0x12, 0x40],
            N64Order::V64 => &[0x37, 0x80, 0x40, 0x12],
            N64Order::N64 => &[0x40, 0x12, 0x37, 0x80],
        });
        b
    }

    pub fn lynx() -> Vec<u8> {
        let mut b = zeros(1024);
        b[..4].copy_from_slice(b"LYNX");
        b
    }

    pub fn a7800() -> Vec<u8> {
        let mut b = zeros(1024);
        b[1..10].copy_from_slice(b"ATARI7800");
        b
    }

    pub fn gb(cgb: u8) -> Vec<u8> {
        let mut b = zeros(32 * 1024);
        b[0x104..0x10C].copy_from_slice(&[0xCE, 0xED, 0x66, 0x66, 0xCC, 0x0D, 0x00, 0x0B]);
        b[0x134..0x13C].copy_from_slice(b"SYNTHGB\0");
        b[0x143] = cgb;
        b[0x14D] = b[0x134..=0x14C]
            .iter()
            .fold(0u8, |x, v| x.wrapping_sub(*v).wrapping_sub(1));
        b
    }

    pub fn gba() -> Vec<u8> {
        let mut b = zeros(64 * 1024);
        b[0x04..0x08].copy_from_slice(&[0x24, 0xFF, 0xAE, 0x51]);
        b[0xA0..0xAC].copy_from_slice(b"SYNTHGBA\0\0\0\0");
        b[0xB2] = 0x96;
        b[0xBD] = b[0xA0..=0xBC]
            .iter()
            .fold(0u8, |c, v| c.wrapping_sub(*v))
            .wrapping_sub(0x19);
        b
    }

    /// An NDS header after `edit`, with the header CRC recomputed.
    pub fn nds_with(edit: impl FnOnce(&mut [u8])) -> Vec<u8> {
        let mut b = zeros(64 * 1024);
        b[..12].copy_from_slice(b"SYNTHNDS\0\0\0\0");
        b[0x15C..0x15E].copy_from_slice(&0xCF56u16.to_le_bytes());
        edit(&mut b);
        let crc = super::crc16_modbus(&b[..0x15E]);
        b[0x15E..0x160].copy_from_slice(&crc.to_le_bytes());
        b
    }

    pub fn nds() -> Vec<u8> {
        nds_with(|_| {})
    }

    pub fn md() -> Vec<u8> {
        let mut b = zeros(128 * 1024);
        b[0x100..0x10F].copy_from_slice(b"SEGA MEGA DRIVE");
        b
    }

    pub fn smd_interleaved() -> Vec<u8> {
        let mut b = zeros(16384 * 8 + 512);
        b[8] = 0xAA;
        b[9] = 0xBB;
        b
    }

    pub fn sega32x() -> Vec<u8> {
        let mut b = zeros(128 * 1024);
        b[0x100..0x108].copy_from_slice(b"SEGA 32X");
        b
    }

    pub fn pico() -> Vec<u8> {
        let mut b = zeros(128 * 1024);
        b[0x100..0x109].copy_from_slice(b"SEGA PICO");
        b
    }

    /// `SEGADISCSYSTEM` at 0 (ISO) or 0x10 (raw 2352-byte sectors).
    pub fn segacd_iso(raw: bool) -> Vec<u8> {
        let mut b = zeros(64 * 1024);
        let at = if raw { 0x10 } else { 0 };
        b[at..at + 14].copy_from_slice(b"SEGADISCSYSTEM");
        b
    }

    /// `TMR SEGA` at `offset` with `region_byte` at +0xF.
    pub fn sms_gg(offset: usize, region_byte: u8) -> Vec<u8> {
        let mut b = zeros((offset + 0x10).max(32 * 1024));
        b[offset..offset + 8].copy_from_slice(b"TMR SEGA");
        b[offset + 0xF] = region_byte;
        b
    }

    pub fn html_page() -> Vec<u8> {
        b"<!DOCTYPE html>\n<html><head><title>Download</title></head><body>Not found</body></html>\n"
            .to_vec()
    }
}
