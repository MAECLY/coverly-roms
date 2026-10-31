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
pub const DECISIVE: [&str; 9] = [
    "gb", "gbc", "gba", "nds", "n64", "nes", "fds", "sega32x", "vectrex",
];
/// Extensions of games (and disc-set companions) that are text by nature:
/// PICO-8 carts, ScummVM launchers, DOSBox configs and batch files, BASIC
/// listings, cue sheets and playlists, Intel HEX (Arduboy), LowRes NX
/// sources, Lua games and `.game` files (spec D26). Only a web page is
/// "not a ROM" there.
pub const TEXT_NATIVE_EXTS: [&str; 14] = [
    "p8", "scummvm", "svm", "conf", "bat", "bas", "cue", "m3u", "gdi", "ccd", "hex", "nx", "lua",
    "game",
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
    /// An archive holding only a ROM patch (`.ips`, `.bps`…): it needs
    /// the original ROM and does not play on its own.
    Patch,
}

/// Container formats recognised by their magic (contents not read here).
///
/// The magic table is spec `2026-10-23-organize-archives-and-discs.md`
/// §3.1; [`crate::archive::detect::detect_container`] decides it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ContainerMagic {
    /// `PK\x03\x04` or an empty archive's `PK\x05\x06` at offset 0.
    Zip,
    /// No zip magic at 0, but a valid end-of-central-directory record at
    /// the end of the file: a self-extracting or prefixed zip.
    ZipEocdOnly,
    /// Spanned/split zip marker `PK\x07\x08`, or an EOCD on disk ≠ 0:
    /// one part of a multi-volume set (spec §3.2 rule 6).
    ZipSpanned,
    SevenZip,
    Rar4,
    Rar5,
    Gzip,
    Bzip2,
    Xz,
    Zstd,
    /// LZ4 frame or legacy format.
    Lz4,
    Tar,
    /// The legacy `.lzma` format: no magic, extension and plausible
    /// properties only.
    LzmaAlone,
    /// A disc image, not an archive.
    Chd,
    /// Recognised, never opened: `Unsupported { format }`.
    Cab,
    Arj,
    Lzh,
    /// Unix `compress` (`.Z`).
    Compress,
    /// ECM-encoded disc image.
    Ecm,
}

impl ContainerMagic {
    /// One part of a multi-volume set, never opened alone.
    pub fn is_multi_volume(self) -> bool {
        self == Self::ZipSpanned
    }

    /// Any zip shape (local header, prefixed, spanned).
    pub fn is_zip(self) -> bool {
        matches!(self, Self::Zip | Self::ZipEocdOnly | Self::ZipSpanned)
    }

    /// RAR 4 or RAR 5.
    pub fn is_rar(self) -> bool {
        matches!(self, Self::Rar4 | Self::Rar5)
    }
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
    /// The disc serial the header names (`SLUS-00862`, `T-5704H`), read
    /// by `disc::`; `None` for carts and discs without one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serial: Option<String>,
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
    if let Some(c) = container(&head, len) {
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
pub(crate) fn text_kind(head: &[u8]) -> Option<NotARom> {
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

/// Container magic of the prefix alone (no tail: a prefixed zip is found
/// by `inspect`, which reads the end of the file).
fn container(head: &[u8], len: u64) -> Option<ContainerMagic> {
    crate::archive::detect::detect_container(head, len, None)
}

/// ISO 9660 primary volume descriptor at sector 16, as a 2048-byte ISO or
/// a raw 2352-byte Mode 2 image (user data at +24 of each sector).
#[derive(Clone, Copy)]
struct IsoLayout {
    sector: u64,
    data: u64,
}
const ISO: IsoLayout = IsoLayout {
    sector: 2048,
    data: 0,
};
const RAW: IsoLayout = IsoLayout {
    sector: 2352,
    data: 24,
};

/// Largest CD image: 99 minutes of raw 2352-byte sectors.
const CD_MAX: u64 = 99 * 60 * 75 * 2352;
/// Largest UMD (dual layer, 1.8 GB).
const UMD_MAX: u64 = 1_800_000_000;

impl IsoLayout {
    fn offset(self, lba: u64, within: u64) -> u64 {
        lba * self.sector + self.data + within
    }
}

/// Disc images: Saturn and Dreamcast by their boot sector magic, PSP by
/// `CISO` or the `PSP GAME` volume id, PlayStation by the ISO 9660 system
/// id, then `SYSTEM.CNF` to tell PS1 (`BOOT`) from PS2 (`BOOT2`).
fn discs(src: &mut dyn ReadAt, head: &[u8], len: u64, s: &mut Sniff) {
    for (magic, system, detail) in [
        (&b"SEGA SEGASATURN"[..], "saturn", "SEGA SEGASATURN"),
        (&b"SEGA SEGAKATANA"[..], "dreamcast", "SEGA SEGAKATANA"),
    ] {
        if starts(head, 0, magic) || starts(head, 0x10, magic) {
            s.hit(system, 0.90, detail);
            s.serial = ip_bin_serial(head);
            return;
        }
    }
    if starts(head, 0, b"CISO") {
        // PCSX2 reads CSO too: a PS2 volume inside says so (spec §12).
        if let Some((system, weight, detail, serial)) = cso_playstation(src, len) {
            s.hit(system, weight, detail);
            s.serial = serial;
            return;
        }
        // Otherwise only an image a UMD can hold is a PSP game.
        let size = at(head, 8, 8).map(|b| u64::from_le_bytes(b.try_into().unwrap_or([0; 8])));
        if size.is_some_and(|n| n <= UMD_MAX) {
            s.hit("psp", 0.85, "CSO");
        }
        return;
    }
    for layout in [ISO, RAW] {
        let Some(pvd) = read_block(src, head, layout.offset(16, 0), 2048, len) else {
            continue;
        };
        if &pvd[1..6] != b"CD001" {
            continue;
        }
        // Real UMD dumps carry `PSP GAME` as the system id (the volume id
        // is the game code); accept it in either field.
        if &pvd[8..16] == b"PSP GAME" || &pvd[40..48] == b"PSP GAME" {
            s.hit("psp", 0.85, "PSP GAME");
            return;
        }
        if &pvd[8..19] != b"PLAYSTATION" {
            return; // another ISO 9660 disc: nothing to say
        }
        let cnf = system_cnf(src, head, layout, &pvd, len);
        if let Some(text) = &cnf {
            let lines = crate::disc::iso9660::parse_system_cnf(text.as_bytes());
            s.serial = lines
                .boot2
                .or(lines.boot)
                .as_deref()
                .and_then(crate::disc::serial::boot_file_serial);
        }
        match cnf {
            Some(text) if text.contains("BOOT2") => s.hit("ps2", 0.95, "SYSTEM.CNF BOOT2"),
            Some(text) if text.contains("BOOT") => s.hit("psx", 0.95, "SYSTEM.CNF BOOT"),
            // No CD holds this much: a PS2 DVD whose SYSTEM.CNF was not found.
            _ if len > CD_MAX => s.hit("ps2", 0.80, "PLAYSTATION > CD"),
            _ => s.hit("psx", 0.80, "PLAYSTATION"),
        }
        return;
    }
}

/// A [`ReadAt`] as a `Read + Seek` stream for the disc readers.
struct AtCursor<'a> {
    src: &'a mut dyn ReadAt,
    pos: u64,
    len: u64,
}

impl io::Read for AtCursor<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.src.read_at(self.pos, buf)?;
        self.pos += n as u64;
        Ok(n)
    }
}

impl io::Seek for AtCursor<'_> {
    fn seek(&mut self, to: io::SeekFrom) -> io::Result<u64> {
        let pos = match to {
            io::SeekFrom::Start(n) => Some(n),
            io::SeekFrom::End(d) => self.len.checked_add_signed(d),
            io::SeekFrom::Current(d) => self.pos.checked_add_signed(d),
        };
        self.pos = pos.ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
        Ok(self.pos)
    }
}

/// The PS2 verdict of a CISO image whose volume says `PLAYSTATION` and
/// boots through `BOOT2` (or is larger than a CD): system, weight, detail
/// and serial. `None` for anything else, a read error included: the CSO
/// rule then decides as before. Inflates at most
/// [`crate::disc::MAX_INFLATED`] bytes.
fn cso_playstation(
    src: &mut dyn ReadAt,
    len: u64,
) -> Option<(&'static str, f32, &'static str, Option<String>)> {
    let cursor = AtCursor { src, pos: 0, len };
    let mut cso = crate::disc::Cso::open(cursor).ok()??;
    let total = cso.image_len();
    let disc = crate::disc::identify_playstation(&mut cso, Some(total)).ok()??;
    if disc.system != crate::disc::DiscSystem::Known("ps2") {
        return None;
    }
    let (weight, detail) = if disc.confidence >= 0.95 {
        (0.95, "SYSTEM.CNF BOOT2")
    } else {
        (0.80, "PLAYSTATION > CD")
    };
    Some(("ps2", weight, detail, disc.serial))
}

/// The product number of a Sega boot header at 0 or 0x10 of `head`.
fn ip_bin_serial(head: &[u8]) -> Option<String> {
    crate::disc::sega::ip_bin_in_head(head).and_then(|ip| ip.serial)
}

/// `n` bytes at `offset`, from the prefix when it covers them.
fn read_block(
    src: &mut dyn ReadAt,
    head: &[u8],
    offset: u64,
    n: usize,
    len: u64,
) -> Option<Vec<u8>> {
    if offset.checked_add(n as u64)? > len {
        return None;
    }
    if let Some(slice) = usize::try_from(offset).ok().and_then(|o| at(head, o, n)) {
        return Some(slice.to_vec());
    }
    let mut buf = vec![0u8; n];
    (read_exact_at(src, offset, &mut buf) == n).then_some(buf)
}

/// Text of `SYSTEM.CNF` from the root directory (one directory read, at
/// most 2 sectors, file capped at 2 KiB).
fn system_cnf(
    src: &mut dyn ReadAt,
    head: &[u8],
    layout: IsoLayout,
    pvd: &[u8],
    len: u64,
) -> Option<String> {
    let root = &pvd[156..156 + 34];
    let lba = u64::from(u32::from_le_bytes(root[2..6].try_into().ok()?));
    let size = u32::from_le_bytes(root[10..14].try_into().ok()?).min(4096) as usize;
    let mut dir = Vec::new();
    for i in 0..size.div_ceil(2048) as u64 {
        dir.extend(read_block(src, head, layout.offset(lba + i, 0), 2048, len)?);
    }
    let mut pos = 0;
    while pos + 33 < dir.len() {
        let rec_len = dir[pos] as usize;
        if rec_len == 0 {
            // Records never cross a sector: skip to the next one.
            pos = (pos / 2048 + 1) * 2048;
            continue;
        }
        let name_len = dir[pos + 32] as usize;
        let name = dir.get(pos + 33..pos + 33 + name_len)?;
        if name.eq_ignore_ascii_case(b"SYSTEM.CNF;1") {
            let file_lba = u64::from(u32::from_le_bytes(dir[pos + 2..pos + 6].try_into().ok()?));
            let file_len =
                (u32::from_le_bytes(dir[pos + 10..pos + 14].try_into().ok()?) as usize).min(2048);
            let bytes = read_block(src, head, layout.offset(file_lba, 0), file_len, len)?;
            return Some(String::from_utf8_lossy(&bytes).to_ascii_uppercase());
        }
        pos += rec_len;
    }
    None
}

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
    if early_consoles(head, ext, s) {
        return;
    }
    // A GBA header carrying its logo too (48 bits) outranks the SNES
    // header, which real GBA code at 0x7FC0 can mimic.
    if gba_signature(head) == Some(true) {
        s.hit("gba", 0.95, "fixed 0x96, logo");
        return;
    }
    if let Some(weight) = snes_header(src, head, len) {
        let detail = if weight >= 0.95 {
            "internal header"
        } else {
            "map mode + title"
        };
        s.hit("snes", weight, detail);
        return;
    }
    if starts(head, 0, b"COPYRIGHT BY SNK CORPORATION")
        || starts(head, 0, b" LICENSED BY SNK CORPORATION")
    {
        let color = head.get(0x23) == Some(&0x10);
        s.hit(if color { "ngpc" } else { "ngp" }, 0.90, "SNK");
        return;
    }
    // WonderSwan has no magic, only a 16-byte footer opening with a far
    // jump (0xEA), colour flag at byte 7: too weak to trust outside its own
    // extensions.
    if matches!(ext, "ws" | "wsc") && len >= 16 && len.is_multiple_of(65536) {
        let mut foot = [0u8; 16];
        if read_exact_at(src, len - 16, &mut foot) == 16 && foot[0] == 0xEA {
            let color = foot[7] & 1 == 1;
            s.hit(
                if color {
                    "wonderswancolor"
                } else {
                    "wonderswan"
                },
                0.85,
                "footer",
            );
            return;
        }
    }
    // GBA without its logo last: one fixed byte plus a one-byte complement
    // (16 bits), weaker than any magic above, so a file that carries one of
    // those and matches it by chance keeps its own system.
    if gba_signature(head) == Some(false) {
        s.hit("gba", 0.95, "fixed 0x96");
        return;
    }
    // ColecoVision BIOS test bytes (`AA 55` shows the title screen, `55 AA`
    // skips it): 16 bits, so they only support a folder or `.col`, and only
    // under the extensions a Coleco cart is dumped with.
    if matches!(ext, "col" | "rom" | "bin")
        && matches!(at(head, 0, 2), Some([0xAA, 0x55] | [0x55, 0xAA]))
    {
        s.hit("colecovision", 0.60, "AA55");
    }
}

/// Early consoles with a magic of their own (spec §8.1). The Vectrex
/// Executive ROM boots a cartridge only when it opens with the `g GCE`
/// copyright line (libretro-vecx `bios/system.bin` holds the line it
/// expects at 0x1101, `g GCE 1982` + `0x80`). Jaguar developer programs
/// carry the DRI `.abs` (`60 1B`) or COFF (`01 50`) magic: two bytes,
/// weight 0.50, only under their own extension.
fn early_consoles(head: &[u8], ext: &str, s: &mut Sniff) -> bool {
    if starts(head, 0, b"g GCE") {
        s.hit("vectrex", 0.95, "g GCE");
        return true;
    }
    let magic = at(head, 0, 2);
    match ext {
        "abs" if magic == Some(&[0x60, 0x1B][..]) => s.hit("jaguar", 0.50, "DRI ABS"),
        "cof" if magic == Some(&[0x01, 0x50][..]) => s.hit("jaguar", 0.50, "COFF"),
        _ => return false,
    }
    true
}

/// GBA fixed byte 0x96 and header complement: `Some(true)` when the
/// Nintendo logo opens at 0x04 as well.
fn gba_signature(head: &[u8]) -> Option<bool> {
    let h = at(head, 0, 0xC0)?;
    (h[0xB2] == 0x96 && gba_complement(h) == h[0xBD])
        .then(|| starts(h, 0x04, &[0x24, 0xFF, 0xAE, 0x51]))
}

/// The Sega family, most specific first (spec §6.3): Sega CD, 32X and Pico
/// also carry `SEGA` at 0x100, so they are checked before Mega Drive.
/// Returns whether a signature matched (a hit or the Pico veto).
fn sega(head: &[u8], len: u64, ext: &str, s: &mut Sniff) -> bool {
    if starts(head, 0, b"SEGADISCSYSTEM") || starts(head, 0x10, b"SEGADISCSYSTEM") {
        s.hit("segacd", 0.95, "SEGADISCSYSTEM");
        s.serial = ip_bin_serial(head);
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
    // A Super Wild Card (SNES) copier header also has 0xAA 0xBB at 8–9 and
    // most SNES sizes + 512 pass the length test: the first block must
    // also deinterleave to the `SEGA` every Mega Drive header carries.
    if len % 16384 == 512
        && at(head, 8, 2) == Some(&[0xAA, 0xBB][..])
        && (smd_starts(head, 0x100, b"SEGA") || smd_starts(head, 0x101, b"SEGA"))
    {
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

/// Whether the first SMD block (after the 512-byte header) deinterleaves
/// to `magic` at `offset`: even bytes sit in the block's second 8 KiB,
/// odd bytes in its first.
fn smd_starts(head: &[u8], offset: usize, magic: &[u8]) -> bool {
    magic.iter().enumerate().all(|(i, want)| {
        let j = offset + i;
        let k = if j.is_multiple_of(2) {
            0x2000 + j / 2
        } else {
            j / 2
        };
        j < 0x4000 && head.get(512 + k) == Some(want)
    })
}

/// SNES map-mode bytes (0x15 of the internal header): LoROM, HiROM,
/// ExLoROM/S-DD1, SA-1, ExHiROM, their FastROM variants and SPC7110.
const SNES_MAPS: [u8; 10] = [0x20, 0x21, 0x22, 0x23, 0x25, 0x30, 0x31, 0x32, 0x35, 0x3A];
/// Plausible ROM-size bytes (0x17): 2^n KiB, 256 KiB to 8 MiB.
const SNES_SIZES: std::ops::RangeInclusive<u8> = 0x08..=0x0D;

/// SNES internal header at LoROM/HiROM/ExHiROM, after a 512-byte copier
/// header when `len % 1024 == 512`: 0.95 when the checksum and its
/// complement agree under a valid map mode or ROM size (old renamers let a
/// 22-byte title overwrite the map byte), 0.85 for a valid map mode under
/// a printable title.
fn snes_header(src: &mut dyn ReadAt, head: &[u8], len: u64) -> Option<f32> {
    let base: u64 = if len % 1024 == 512 { 512 } else { 0 };
    let mut weak = None;
    for offset in [0x7FC0u64, 0xFFC0, 0x40_FFC0] {
        let pos = base + offset;
        if pos + 0x20 > len {
            continue;
        }
        let mut h = [0u8; 0x20];
        let got = match usize::try_from(pos).ok().and_then(|p| at(head, p, 0x20)) {
            Some(slice) => {
                h.copy_from_slice(slice);
                0x20
            }
            None => read_exact_at(src, pos, &mut h),
        };
        if got < 0x20 {
            continue;
        }
        let map_ok = SNES_MAPS.contains(&h[0x15]);
        let complement = u16::from_le_bytes([h[0x1C], h[0x1D]]);
        let checksum = u16::from_le_bytes([h[0x1E], h[0x1F]]);
        // 0xFFFF/0x0000 is erased filler next to zeros, not a checksum.
        let paired = checksum ^ complement == 0xFFFF && complement != 0xFFFF;
        if paired && (map_ok || SNES_SIZES.contains(&h[0x17])) {
            return Some(0.95);
        }
        if !map_ok {
            continue;
        }
        let title = &h[..21];
        if title.iter().all(|b| (0x20..0x7F).contains(b)) && title.iter().any(|b| *b != b' ') {
            weak = Some(0.85);
        }
    }
    weak
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

    /// [`gb`] with `tag` as the first title byte (distinct contents), the
    /// header checksum recomputed.
    pub fn gb_tagged(cgb: u8, tag: u8) -> Vec<u8> {
        let mut b = gb(cgb);
        b[0x134] = tag;
        b[0x14D] = b[0x134..=0x14C]
            .iter()
            .fold(0u8, |x, v| x.wrapping_sub(*v).wrapping_sub(1));
        b
    }

    /// [`gba`] with `tag` as the first title byte (distinct contents), the
    /// complement check recomputed.
    pub fn gba_tagged(tag: u8) -> Vec<u8> {
        let mut b = gba();
        b[0xA0] = tag;
        b[0xBD] = b[0xA0..=0xBC]
            .iter()
            .fold(0u8, |c, v| c.wrapping_sub(*v))
            .wrapping_sub(0x19);
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

    /// [`md`] in SMD format: 512-byte copier header, then 16 KiB blocks
    /// with odd bytes in the first half and even bytes in the second.
    pub fn smd_interleaved() -> Vec<u8> {
        let rom = md();
        let mut b = zeros(512 + rom.len());
        b[0] = (rom.len() / 16384) as u8;
        b[8] = 0xAA;
        b[9] = 0xBB;
        b[10] = 0x06;
        for (n, block) in rom.chunks(16384).enumerate() {
            let out = &mut b[512 + n * 16384..512 + (n + 1) * 16384];
            for i in 0..8192 {
                out[i] = block[2 * i + 1];
                out[0x2000 + i] = block[2 * i];
            }
        }
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

    /// A Vectrex cartridge: the `g GCE 1982` copyright line at 0, closed by
    /// `0x80`, as the Executive ROM expects it.
    pub fn vectrex() -> Vec<u8> {
        let mut b = zeros(8 * 1024);
        b[..11].copy_from_slice(b"g GCE 1982\x80");
        b
    }

    /// A ColecoVision cartridge opening with the BIOS test bytes (`AA 55`,
    /// or `55 AA` when `swapped`, the variant that skips the title screen).
    pub fn colecovision(swapped: bool) -> Vec<u8> {
        let mut b = zeros(16 * 1024);
        b[..2].copy_from_slice(if swapped {
            &[0x55, 0xAA]
        } else {
            &[0xAA, 0x55]
        });
        b
    }

    /// A Jaguar DRI `.abs` program (magic `60 1B`).
    pub fn jaguar_abs() -> Vec<u8> {
        let mut b = zeros(4096);
        b[..2].copy_from_slice(&[0x60, 0x1B]);
        b
    }

    /// A Jaguar COFF `.cof` program (magic `01 50`).
    pub fn jaguar_cof() -> Vec<u8> {
        let mut b = zeros(4096);
        b[..2].copy_from_slice(&[0x01, 0x50]);
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

    #[derive(Clone, Copy, Debug)]
    pub enum SnesMap {
        Lo,
        Hi,
        ExHi,
    }

    /// A SNES internal header with a valid checksum/complement pair at the
    /// map's offset, after a 512-byte copier header when `copier`.
    pub fn snes(map: SnesMap, copier: bool) -> Vec<u8> {
        let (offset, len, mode) = match map {
            SnesMap::Lo => (0x7FC0usize, 0x8_0000usize, 0x20u8),
            SnesMap::Hi => (0xFFC0, 0x10_0000, 0x21),
            SnesMap::ExHi => (0x40_FFC0, 0x41_0000, 0x25),
        };
        let base = if copier { 512 } else { 0 };
        let mut b = zeros(len + base);
        let h = &mut b[base + offset..base + offset + 0x20];
        h[..21].copy_from_slice(b"SYNTH SNES           ");
        h[0x15] = mode;
        h[0x1C..0x1E].copy_from_slice(&0x5A5Au16.to_le_bytes());
        h[0x1E..0x20].copy_from_slice(&(0x5A5Au16 ^ 0xFFFF).to_le_bytes());
        b
    }

    pub fn ngp(color: bool) -> Vec<u8> {
        let mut b = zeros(64 * 1024);
        b[..28].copy_from_slice(b"COPYRIGHT BY SNK CORPORATION");
        if color {
            b[0x23] = 0x10;
        }
        b
    }

    /// A WonderSwan footer: 0xEA (far jump) 16 bytes from the end, colour
    /// flag 9 bytes from the end.
    pub fn ws(color: bool) -> Vec<u8> {
        let mut b = zeros(128 * 1024);
        let n = b.len();
        b[n - 16] = 0xEA;
        b[n - 9] = u8::from(color);
        b
    }

    /// Sectors in a synthetic disc image.
    const DISC_SECTORS: usize = 24;

    /// A disc image of [`DISC_SECTORS`] sectors whose user data is written
    /// by `fill(lba, data)`: 2048-byte sectors, or raw 2352-byte Mode 2
    /// sectors (sync, header, user data at +24) when `raw`.
    fn disc(raw: bool, mut fill: impl FnMut(usize, &mut [u8])) -> Vec<u8> {
        let (size, data) = if raw { (2352, 24) } else { (2048, 0) };
        let mut b = zeros(DISC_SECTORS * size);
        for lba in 0..DISC_SECTORS {
            let sector = &mut b[lba * size..(lba + 1) * size];
            if raw {
                sector[1..11].fill(0xFF);
                sector[15] = 2;
            }
            fill(lba, &mut sector[data..data + 2048]);
        }
        b
    }

    /// An ISO 9660 primary volume descriptor (sector 16) with system id
    /// `system` and volume id `volume`, root directory at LBA 20.
    fn pvd(data: &mut [u8], system: &[u8], volume: &[u8]) {
        data[0] = 1;
        data[1..6].copy_from_slice(b"CD001");
        data[6] = 1;
        data[8..40].fill(b' ');
        data[8..8 + system.len()].copy_from_slice(system);
        data[40..72].fill(b' ');
        data[40..40 + volume.len()].copy_from_slice(volume);
        dir_record(&mut data[156..190], 20, 2048, b"\0");
    }

    /// One ISO 9660 directory record pointing at `lba` (`size` bytes).
    pub(crate) fn dir_record(rec: &mut [u8], lba: u32, size: u32, name: &[u8]) -> usize {
        let len = (33 + name.len()).next_multiple_of(2);
        rec[0] = len as u8;
        rec[2..6].copy_from_slice(&lba.to_le_bytes());
        rec[6..10].copy_from_slice(&lba.to_be_bytes());
        rec[10..14].copy_from_slice(&size.to_le_bytes());
        rec[14..18].copy_from_slice(&size.to_be_bytes());
        rec[32] = name.len() as u8;
        rec[33..33 + name.len()].copy_from_slice(name);
        len
    }

    /// A PlayStation disc: `PLAYSTATION` PVD, a root directory (LBA 20)
    /// holding `SYSTEM.CNF;1` (LBA 21) that boots a PS1 (`BOOT`) or PS2
    /// (`BOOT2`) executable.
    pub fn psx_iso(raw: bool, boot2: bool) -> Vec<u8> {
        let cnf: &[u8] = if boot2 {
            b"BOOT2 = cdrom0:\\SLUS_200.01;1\r\nVER = 1.00\r\nVMODE = NTSC\r\n"
        } else {
            b"BOOT = cdrom:\\SLUS_000.01;1\r\nTCB = 4\r\nEVENT = 10\r\n"
        };
        disc(raw, |lba, data| match lba {
            16 => pvd(data, b"PLAYSTATION", b"SYNTHPSX"),
            20 => {
                dir_record(data, 21, cnf.len() as u32, b"SYSTEM.CNF;1");
            }
            21 => data[..cnf.len()].copy_from_slice(cnf),
            _ => {}
        })
    }

    /// Renames the `SYSTEM.CNF;1` record of a [`psx_iso`] image.
    pub fn drop_system_cnf(iso: &mut [u8]) {
        let name = b"SYSTEM.CNF;1";
        let pos = iso
            .windows(name.len())
            .position(|w| w == name)
            .expect("a SYSTEM.CNF record");
        iso[pos..pos + name.len()].copy_from_slice(b"README.TXT;1");
    }

    pub fn saturn() -> Vec<u8> {
        let mut b = zeros(32 * 1024);
        b[..15].copy_from_slice(b"SEGA SEGASATURN");
        b
    }

    /// The first sector of a Dreamcast high-density data track (IP.BIN).
    pub fn dreamcast_track() -> Vec<u8> {
        let mut b = zeros(32 * 1024);
        b[..15].copy_from_slice(b"SEGA SEGAKATANA");
        b
    }

    /// A UMD image: ISO 9660 with the `PSP GAME` system id and the game
    /// code as volume id, as real dumps carry them.
    pub fn psp_iso() -> Vec<u8> {
        disc(false, |lba, data| {
            if lba == 16 {
                pvd(data, b"PSP GAME", b"ULUS10041");
            }
        })
    }

    /// A compressed ISO (`CISO` header, 24 bytes).
    pub fn cso() -> Vec<u8> {
        let mut b = zeros(32 * 1024);
        b[..4].copy_from_slice(b"CISO");
        b[4..8].copy_from_slice(&24u32.to_le_bytes());
        b[8..16].copy_from_slice(&(48u64 * 1024 * 1024).to_le_bytes());
        b[16..20].copy_from_slice(&2048u32.to_le_bytes());
        b[20] = 1;
        b
    }

    /// `len` pseudo-random bytes (xorshift64*), reproducible by `seed`.
    pub fn xorshift_bytes(seed: u64, len: usize) -> Vec<u8> {
        let mut x = seed.max(1);
        let mut out = Vec::with_capacity(len + 8);
        while out.len() < len {
            x ^= x >> 12;
            x ^= x << 25;
            x ^= x >> 27;
            out.extend_from_slice(&x.wrapping_mul(0x2545_F491_4F6C_DD1D).to_le_bytes());
        }
        out.truncate(len);
        out
    }

    pub fn html_page() -> Vec<u8> {
        b"<!DOCTYPE html>\n<html><head><title>Download</title></head><body>Not found</body></html>\n"
            .to_vec()
    }
}
