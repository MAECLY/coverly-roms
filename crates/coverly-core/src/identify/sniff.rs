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
pub const DECISIVE: [&str; 15] = [
    "gb", "gbc", "gba", "nds", "n64", "nes", "fds", "sega32x", "vectrex", "pico", "sufami",
    "uzebox", "vircon32", "doom", "3ds",
];
/// Extensions of games (and disc-set companions) that are text by nature:
/// PICO-8 carts, ScummVM launchers, DOSBox configs and batch files, BASIC
/// listings, cue sheets and playlists, Intel HEX (Arduboy), LowRes NX
/// sources, Lua games and `.game` files (spec D26), and the Vita3K and
/// RPCS3 launchers `.psvita` and `.psn` and the Xenia launcher `.xbox360`
/// (systems batch 2 D14), and the ES-DE «game serial» file `.ps4`
/// (systems batch 3 D17). Only a web page is "not a ROM" there.
pub const TEXT_NATIVE_EXTS: [&str; 18] = [
    "p8", "scummvm", "svm", "conf", "bat", "bas", "cue", "m3u", "gdi", "ccd", "hex", "nx", "lua",
    "game", "psvita", "psn", "xbox360", "ps4",
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
    /// A macOS installer package (`xar!` named `.pkg`, systems batch 2 D7).
    Installer,
    /// An Autodesk 3D Studio model (main chunk `4D 4D` named `.3ds`).
    Model3d,
    /// A PC game's data pack: a Valve VPK (`34 12 AA 55`) named `.vpk`,
    /// as Steam workshop folders hold by the hundred (systems batch 2).
    PcGameData,
    /// A console's own system file or folder (firmware modules, a
    /// console system package), never a game (systems batch 3).
    ConsoleSystem,
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
    /// What a console header reader found (`consoles::identify`): system,
    /// title id, kind, embedded-art locators. Memoised with the facts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub console: Option<crate::consoles::ConsoleId>,
    /// Bytes of prefix actually read (0 = unreadable or empty).
    pub read: usize,
}

impl Sniff {
    pub(crate) fn hit(&mut self, system: &str, weight: f32, detail: impl Into<String>) {
        self.hits
            .push(Evidence::new(IdLevel::Header, system, weight, detail));
    }

    /// Records the verdicts of `consoles::identify_all`: one header hit
    /// per verdict (an XVC header gives two of equal weight, systems batch
    /// 3 D19), the first verdict as [`Sniff::console`]. A console's own
    /// XVD (`xbox::XVD_SYSTEM_DETAIL`) is no ROM and no hit.
    pub fn add_console(&mut self, ids: Vec<crate::consoles::ConsoleId>) {
        if let [only] = ids.as_slice() {
            if only.detail == crate::consoles::xbox::XVD_SYSTEM_DETAIL {
                self.not_a_rom = Some(NotARom::ConsoleSystem);
                return;
            }
        }
        for id in &ids {
            self.hit(&id.system, id.confidence, id.detail.clone());
        }
        if let Some(first) = ids.into_iter().next() {
            self.serial = first.serial.clone();
            self.console = Some(first);
        }
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
    sniff_named(src, len, ext, "")
}

/// [`sniff`] with the file (or archive entry) name, which the console
/// readers may use (Switch title-id tokens, spec D10); `""` when unknown.
pub fn sniff_named(src: &mut dyn ReadAt, len: u64, ext: &str, name: &str) -> Sniff {
    let ext = ext.to_ascii_lowercase();
    if let Some(s) = sniff_switch_first(src, len, &ext, name) {
        return s;
    }
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
    if let Some(kind) = not_a_console_file(&head, &ext) {
        s.not_a_rom = Some(kind);
        return s;
    }
    if let Some(kind) = text_kind(&head) {
        // `[`/`{` also open INI files and configs: under a text-native
        // extension only markup (an HTML error page) is not a game.
        if !TEXT_NATIVE_EXTS.contains(&ext.as_str()) || starts_with_markup(&head) {
            s.not_a_rom = Some(kind);
        } else if let Some(id) = crate::consoles::sony::launcher(&head, &ext)
            .or_else(|| crate::consoles::xbox::launcher(&head, &ext))
        {
            s.hit(&id.system, id.confidence, id.detail.clone());
            s.serial = id.serial.clone();
            s.console = Some(id);
        }
        return s;
    }
    // A Cemu WUA or a Xenia ZAR (ZArchive) starts with its first zstd
    // block, so its footer is checked before the head is taken for a
    // `.zst` file.
    if ext == "wua" || ext == "zar" || head.starts_with(&[0x28, 0xB5, 0x2F, 0xFD]) {
        let wua = {
            let mut b = crate::consoles::Budgeted::new(&mut *src, len);
            crate::consoles::wiiu::wua(&mut b, &ext)
        };
        let id = wua.or_else(|| {
            let mut b = crate::consoles::Budgeted::new(&mut *src, len);
            crate::consoles::xbox::zar(&mut b, &ext)
        });
        if let Some(id) = id {
            s.hit(&id.system, id.confidence, id.detail.clone());
            s.serial = id.serial.clone();
            s.console = Some(id);
            return s;
        }
    }
    if let Some(c) = container(&head, len) {
        s.container = Some(c);
        return s;
    }
    // Disc images first (sector 16), then the console header readers of
    // systems batch 2 (plan P-1), then cartridges, most specific first.
    discs(src, &head, len, &ext, &mut s);
    if s.hits.is_empty() && s.unsupported.is_none() {
        let mut b = crate::consoles::Budgeted::new(&mut *src, len);
        let ids = crate::consoles::identify_all(&mut b, &ext, name);
        s.add_console(ids);
    }
    if s.hits.is_empty() && s.unsupported.is_none() {
        cartridges(src, &head, len, &ext, &mut s);
    }
    s
}

/// A Switch dump, recognised before any generic head read (spec D4): the
/// 128 KiB prefix would reach a small NSP's ticket and NCA bodies and an
/// XCI's encrypted gamecard info, so only `consoles::nx` reads it. `ext`
/// is lowercase. `None` when it is not one (see `nx::identify_first`).
pub fn sniff_switch_first(src: &mut dyn ReadAt, len: u64, ext: &str, name: &str) -> Option<Sniff> {
    let (id, read) = crate::consoles::nx::identify_first(src, len, ext, name)?;
    let mut s = Sniff {
        read: usize::try_from(read).unwrap_or(usize::MAX),
        ..Sniff::default()
    };
    s.hit(&id.system, id.confidence, id.detail.clone());
    s.serial = id.serial.clone();
    s.console = Some(id);
    Some(s)
}

/// Files that share an extension with a console format and are told
/// apart by two cheap magics (systems batch 2 D7): every macOS installer
/// is a `xar!` archive named `.pkg`, and a 3D Studio model opens with the
/// main chunk `4D 4D`. A `.3ds` carrying `NCSD` or `NCCH` at 0x100 is a
/// 3DS image whatever its first bytes. A Valve pack named `.vpk` (the
/// Source engine's archives, seen by the hundred in a real Steam library)
/// is no Vita package: a Vita VPK is a zip.
fn not_a_console_file(head: &[u8], ext: &str) -> Option<NotARom> {
    match ext {
        "pkg" if head.starts_with(b"xar!") => Some(NotARom::Installer),
        "vpk" if head.starts_with(&[0x34, 0x12, 0xAA, 0x55]) => Some(NotARom::PcGameData),
        "3ds"
            if head.starts_with(&[0x4D, 0x4D])
                && !matches!(head.get(0x100..0x104), Some(b"NCSD" | b"NCCH")) =>
        {
            Some(NotARom::Model3d)
        }
        _ => None,
    }
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
fn discs(src: &mut dyn ReadAt, head: &[u8], len: u64, ext: &str, s: &mut Sniff) {
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
        // A Nintendo CISO shares the magic: the console readers take it.
        if crate::consoles::nintendo_disc::is_nintendo_ciso(head) {
            return;
        }
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
            break; // another ISO 9660 disc: the disc reader below decides
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
    loose_disc(src, head, len, ext, s);
}

/// Systems a loose disc image (an `.iso`, or the first track of a cue
/// set) is told by through [`crate::disc::DiscReader`]'s signatures, the
/// rules archived disc sets already go through (systems batch 1, spec
/// §8.2). Only this batch's systems: a loose PC Engine CD or Neo Geo CD
/// keeps its earlier result.
const LOOSE_DISC_SYSTEMS: [&str; 5] = ["3do", "pcfx", "cdi", "amigacd32", "cdtv"];

/// Extensions a loose disc image or track is stored under.
const LOOSE_DISC_EXTS: [&str; 3] = ["iso", "bin", "img"];

/// Runs the disc reader over a loose image the rules above said nothing
/// about, and records its verdict for the [`LOOSE_DISC_SYSTEMS`].
fn loose_disc(src: &mut dyn ReadAt, head: &[u8], len: u64, ext: &str, s: &mut Sniff) {
    if !LOOSE_DISC_EXTS.contains(&ext) {
        return;
    }
    let first = &head[..head.len().min(crate::disc::source::DETECT_BYTES)];
    let layout = crate::disc::source::detect_layout(first);
    let mut disc = ReadAtSectors {
        src,
        head,
        len,
        layout,
    };
    let Some(found) = crate::disc::DiscReader::identify(&mut disc) else {
        return;
    };
    if let crate::disc::DiscSystem::Known(system) = found.system {
        if LOOSE_DISC_SYSTEMS.contains(&system) {
            s.hit(system, found.confidence, found.detail);
        }
    }
}

/// A [`crate::disc::source::SectorSource`] over a [`ReadAt`], served from
/// the already-read `head` where it reaches.
struct ReadAtSectors<'a> {
    src: &'a mut dyn ReadAt,
    head: &'a [u8],
    len: u64,
    layout: crate::disc::source::SectorLayout,
}

impl crate::disc::source::SectorSource for ReadAtSectors<'_> {
    fn layout(&self) -> crate::disc::source::SectorLayout {
        self.layout
    }

    fn read_user(
        &mut self,
        lba: u32,
        out: &mut [u8; crate::disc::source::USER_BYTES],
    ) -> io::Result<bool> {
        let offset =
            u64::from(lba) * self.layout.sector_size() as u64 + self.layout.user_offset() as u64;
        match read_block(self.src, self.head, offset, out.len(), self.len) {
            Some(bytes) => {
                out.copy_from_slice(&bytes);
                Ok(true)
            }
            None => Ok(false),
        }
    }

    fn raw_sector(&mut self, n: u32) -> io::Result<Option<Vec<u8>>> {
        let size = self.layout.sector_size();
        let offset = u64::from(n) * size as u64;
        Ok(read_block(self.src, self.head, offset, size, self.len))
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
            // Unit code (D6): DSi and DSiWare stay `nds`, only the detail
            // says so.
            let detail = match h[0x12] {
                0x03 => "NDS header (DSi)",
                0x02 => "NDS header (DSi-enhanced)",
                _ => "NDS header",
            };
            s.hit("nds", 0.95, detail);
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
    if early_consoles(head, ext, s) || engines(head, s) || computer_media(head, ext, s) {
        return;
    }
    // A GBA header carrying its logo too (48 bits) outranks the SNES
    // header, which real GBA code at 0x7FC0 can mimic.
    if gba_signature(head) == Some(true) {
        s.hit("gba", 0.95, "fixed 0x96, logo");
        return;
    }
    // Snes9x `is_SufamiTurbo_Cart` (memmap.cpp): a cart opens with
    // `BANDAI SFC-ADX`; the base unit's BIOS also does, with
    // `SFC-ADX BACKUP` after it. Checked before the SNES header.
    if starts(head, 0, b"BANDAI SFC-ADX") && !starts(head, 0x10, b"SFC-ADX BACKUP") {
        s.hit("sufami", 0.95, "BANDAI SFC-ADX");
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
    if fantasy_consoles(head, s) {
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

/// Fantasy consoles with a header (spec §8.1):
/// - Uzebox: a `.uze` opens with the 6-byte marker `UZEBOX`, which
///   `packrom` writes and the bootloader checks (Uzebox
///   `tools/packrom/packrom.cpp` at 2fc77093).
/// - Vircon32: a cartridge opens with `V32-CART`; a BIOS with `V32-BIOS`
///   is not a game (vircon32-libretro `VirconDefinitions/FileFormats.hpp`
///   at 8b4ff35e).
///
/// WASM-4's `\0asm` only proves WebAssembly, so it names nothing.
fn fantasy_consoles(head: &[u8], s: &mut Sniff) -> bool {
    if starts(head, 0, b"UZEBOX") {
        s.hit("uzebox", 0.95, "UZEBOX");
        return true;
    }
    if starts(head, 0, b"V32-CART") {
        s.hit("vircon32", 0.95, "V32-CART");
        return true;
    }
    false
}

/// Engine data with a header (spec D15): a Doom WAD opens with `IWAD` (a
/// game) or `PWAD` (a patch or mod), then the lump count and directory
/// offset. Quake's `PACK` is OpenBOR's magic too, so it names nothing.
fn engines(head: &[u8], s: &mut Sniff) -> bool {
    if is_doom_wad(head) {
        s.hit("doom", 0.95, if head[0] == b'I' { "IWAD" } else { "PWAD" });
        return true;
    }
    false
}

/// Whether `head` opens like a Doom WAD (`IWAD`/`PWAD`): the proof a
/// folder game gives in the organise walk (D15).
pub fn is_doom_wad(head: &[u8]) -> bool {
    head.starts_with(b"IWAD") || head.starts_with(b"PWAD")
}

/// Tape and disk images of home computers (spec §8.1). TZX (format 1.20:
/// `ZXTape!` + `0x1A`) is the Spectrum's tape format and also the CPC's
/// under `.cdt`. A CPCEMU disk (`MV - CPC…` or `EXTENDED CPC DSK File`)
/// opens CPC and Spectrum +3 disks alike, so it names both at 0.50 (a
/// review), the CPC first. UEF (`UEF File!` + NUL) is the BBC Micro's
/// tape format; a CPCEMU snapshot (`MV - SNA`) decides a `.sna`; a gzip
/// UEF is read through the archive layer
/// (`organize::inspect`).
fn computer_media(head: &[u8], ext: &str, s: &mut Sniff) -> bool {
    if starts(head, 0, b"ZXTape!\x1A") {
        let system = if ext == "cdt" {
            "amstradcpc"
        } else {
            "zxspectrum"
        };
        s.hit(system, 0.90, "TZX");
        return true;
    }
    if ext == "dsk" && (starts(head, 0, b"MV - CPC") || starts(head, 0, b"EXTENDED CPC DSK File")) {
        s.hit("amstradcpc", 0.50, "CPC DSK");
        s.hit("zxspectrum", 0.50, "CPC DSK");
        return true;
    }
    // A CPCEMU snapshot (`MV - SNA`); a Spectrum `.sna` carries no magic.
    if ext == "sna" && starts(head, 0, b"MV - SNA") {
        s.hit("amstradcpc", 0.90, "CPC SNA");
        return true;
    }
    if starts(head, 0, UEF_MAGIC) {
        s.hit("bbcmicro", 0.95, "UEF");
        return true;
    }
    apple2_disks(head, s) || commodore_prg(head, ext, s) || shared_disks(head, ext, s)
}

/// Apple II disk containers (spec §8.1):
/// - WOZ (Applesauce WOZ 1.0 and 2.0 references): `WOZ1` or `WOZ2`, then
///   `FF 0A 0D 0A`, which a 7-bit or line-ending conversion would break.
/// - 2IMG (the 2IMG format spec): `2IMG`, a 4-byte creator tag, then the
///   header size, 64 (`40 00`).
fn apple2_disks(head: &[u8], s: &mut Sniff) -> bool {
    let woz = (starts(head, 0, b"WOZ1") || starts(head, 0, b"WOZ2"))
        && starts(head, 4, &[0xFF, 0x0A, 0x0D, 0x0A]);
    if woz {
        s.hit("apple2", 0.95, "WOZ");
        return true;
    }
    if starts(head, 0, b"2IMG") && starts(head, 8, &[0x40, 0x00]) {
        s.hit("apple2", 0.95, "2IMG");
        return true;
    }
    false
}

/// A Commodore `.prg` opens with its little-endian load address, the
/// machine's BASIC start: `$1001` Plus/4, `$1C01` C128, `$0401` PET,
/// `$0801` C64 (each machine's memory map). Two bytes, so 0.40: below the
/// review line, a tie-break between the folder's candidates only.
fn commodore_prg(head: &[u8], ext: &str, s: &mut Sniff) -> bool {
    if ext != "prg" {
        return false;
    }
    let system = match at(head, 0, 2) {
        Some([0x01, 0x10]) => "plus4",
        Some([0x01, 0x1C]) => "c128",
        Some([0x01, 0x04]) => "pet",
        Some([0x01, 0x08]) => "c64",
        _ => return false,
    };
    s.hit(system, 0.40, "PRG load address");
    true
}

/// Hard and floppy disk images under `.nhd`, `.hdf`, `.fdi` and `.dim`
/// (spec §8.1, plan P-2), each keyed by its extension. `.hdf`, `.fdi` and
/// `.dim` are Shared (D3), so these bytes are what keeps a loose Amiga or
/// Atari ST disk automatic:
/// - `T98HDDIMAGE.R0` + NUL: a T98-Next PC-98 hard disk (Neko Project II
///   kai `fdd/sxsihdd.c`, DOSBox-X `bios_disk.cpp`).
/// - `RDSK` (Rigid Disk Block) or `DOS` + flag 0–7 (a partition's boot
///   block): an Amiga hardfile (WinUAE `hardfile.cpp`). `X68SCSI1`: an
///   X68000 SCSI disk (NetBSD/x68k `disksubr.c`).
/// - `Formatted Disk Image file`: FDI 2.0, written by Disk2FDI for Amiga
///   disks (WinUAE `disk.cpp`); a PC-98 Anex86 `.fdi` has no magic.
/// - FastCopy DIM: `42 42` at 0 with bytes 3 and 0x0A zero, as Hatari
///   checks it (`src/floppies/dim.c`); an X68000 DIM opens with its media
///   type byte instead.
fn shared_disks(head: &[u8], ext: &str, s: &mut Sniff) -> bool {
    match ext {
        "nhd" if starts(head, 0, b"T98HDDIMAGE.R0\0") => s.hit("pc98", 0.95, "T98 NHD"),
        "hdf" if starts(head, 0, b"RDSK") || amiga_rdb_block(head) => {
            s.hit("amiga", 0.90, "RDB");
        }
        "hdf" if starts(head, 0, b"DOS") && head.get(3).is_some_and(|f| *f <= 7) => {
            s.hit("amiga", 0.90, "DOS boot block");
        }
        "hdf" if starts(head, 0, b"X68SCSI1") => s.hit("x68000", 0.90, "X68SCSI1"),
        "fdi" if starts(head, 0, b"Formatted Disk Image file") => {
            s.hit("amiga", 0.90, "FDI 2.0");
        }
        "dim"
            if starts(head, 0, &[0x42, 0x42])
                && head.get(3) == Some(&0)
                && head.get(0x0A) == Some(&0) =>
        {
            s.hit("atarist", 0.85, "FastCopy DIM");
        }
        _ => return false,
    }
    true
}

/// An Amiga Rigid Disk Block in blocks 1–15: AmigaOS `hardblocks.h` lets
/// it sit in any of the first 16 (RDB_LOCATION_LIMIT) 512-byte blocks, and
/// WinUAE `filesys.cpp` (`rdb_mount`) scans them, accepting `RDSK` whose
/// first SummedLongs big-endian longs sum to zero. Block 0 needs only the
/// id (spec §8.1); later blocks also need the checksum.
fn amiga_rdb_block(head: &[u8]) -> bool {
    (1..16).any(|block| {
        let Some(b) = at(head, block * 512, 512) else {
            return false;
        };
        if &b[..4] != b"RDSK" {
            return false;
        }
        let summed = u32::from_be_bytes([b[4], b[5], b[6], b[7]]) as usize;
        (2..=128).contains(&summed)
            && b[..summed * 4]
                .as_chunks::<4>()
                .0
                .iter()
                .fold(0u32, |sum, l| sum.wrapping_add(u32::from_be_bytes(*l)))
                == 0
    })
}

/// Opening bytes of an uncompressed UEF tape.
pub const UEF_MAGIC: &[u8] = b"UEF File!\0";

/// Whether the 16-byte console field at 0x100 names the Pico, matched as
/// the cores match it: Genesis Plus GX looks for `SEGA PICO` anywhere in
/// it (`core/loadrom.c` at f27655b5), PicoDrive for ` PICO `
/// (`pico/media.c` at 26ecb2b6).
fn pico_console_field(head: &[u8]) -> bool {
    at(head, 0x100, 16).is_some_and(|f| contains(f, b"SEGA PICO") || contains(f, b" PICO "))
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
/// Returns whether a signature matched.
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
    if pico_console_field(head) {
        s.hit("pico", 0.95, "SEGA PICO");
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

    /// A Sufami Turbo cartridge (`bios`: the base unit's own ROM, whose
    /// second line reads `SFC-ADX BACKUP`).
    pub fn sufami(bios: bool) -> Vec<u8> {
        let mut b = zeros(if bios { 256 } else { 512 } * 1024);
        b[..14].copy_from_slice(b"BANDAI SFC-ADX");
        let line: &[u8; 14] = if bios {
            b"SFC-ADX BACKUP"
        } else {
            b"SYNTH SUFAMI  "
        };
        b[0x10..0x1E].copy_from_slice(line);
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

    /// A TZX tape (ZX Spectrum `.tzx`, Amstrad CPC `.cdt`): `ZXTape!`,
    /// `0x1A`, version 1.20, then one standard-speed data block (ID 0x10).
    pub fn tzx() -> Vec<u8> {
        let mut b = zeros(2048);
        b[..10].copy_from_slice(b"ZXTape!\x1A\x01\x14");
        b[10] = 0x10;
        b
    }

    /// A CPCEMU disk image: the standard `MV - CPCEMU Disk-File` header or
    /// the `EXTENDED CPC DSK File` one, 40 tracks, one side.
    pub fn cpc_dsk(extended: bool) -> Vec<u8> {
        let mut b = zeros(16 * 1024);
        let line: &[u8] = if extended {
            b"EXTENDED CPC DSK File\r\nDisk-Info\r\n"
        } else {
            b"MV - CPCEMU Disk-File\r\nDisk-Info\r\n"
        };
        b[..line.len()].copy_from_slice(line);
        b[0x30] = 40;
        b[0x31] = 1;
        b
    }

    /// A CPCEMU snapshot (Amstrad CPC `.sna`): `MV - SNA`, then the
    /// register block, version 3 at 0x10, 64 KB of memory.
    pub fn cpc_sna() -> Vec<u8> {
        let mut b = zeros(0x100 + 64 * 1024);
        b[..8].copy_from_slice(b"MV - SNA");
        b[0x10] = 3;
        b[0x6B] = 64;
        b
    }

    /// A BBC Micro UEF tape: `UEF File!`, NUL, version 0.10, then one
    /// chunk header.
    pub fn uef() -> Vec<u8> {
        let mut b = zeros(1024);
        b[..12].copy_from_slice(b"UEF File!\0\x0A\x00");
        b[12] = 0x00;
        b[13] = 0x01;
        b
    }

    /// A T98-Next hard disk (PC-98 `.nhd`): `T98HDDIMAGE.R0` + NUL, then
    /// the 512-byte header size at 0x110.
    pub fn nhd() -> Vec<u8> {
        let mut b = zeros(4096);
        b[..15].copy_from_slice(b"T98HDDIMAGE.R0\0");
        b[0x110..0x114].copy_from_slice(&512u32.to_le_bytes());
        b
    }

    /// An Amiga hard disk with a Rigid Disk Block: `RDSK`, the block size
    /// in longs (64) at 4.
    pub fn amiga_rdb() -> Vec<u8> {
        let mut b = zeros(4096);
        b[..4].copy_from_slice(b"RDSK");
        b[4..8].copy_from_slice(&64u32.to_be_bytes());
        b
    }

    /// An Amiga hard disk whose Rigid Disk Block sits in `block` (its
    /// SummedLongs 64 and a checksum that makes them sum to zero).
    pub fn amiga_rdb_at(block: usize) -> Vec<u8> {
        let mut b = zeros(16 * 1024);
        let o = block * 512;
        b[o..o + 4].copy_from_slice(b"RDSK");
        b[o + 4..o + 8].copy_from_slice(&64u32.to_be_bytes());
        b[o + 0x10..o + 0x14].copy_from_slice(&7u32.to_be_bytes());
        let sum = b[o..o + 256]
            .as_chunks::<4>()
            .0
            .iter()
            .fold(0u32, |s, l| s.wrapping_add(u32::from_be_bytes(*l)));
        b[o + 8..o + 12].copy_from_slice(&0u32.wrapping_sub(sum).to_be_bytes());
        b
    }

    /// An Amiga partition hardfile: `DOS` + flag 1 (FFS) opens its boot
    /// block.
    pub fn amiga_dos() -> Vec<u8> {
        let mut b = zeros(4096);
        b[..4].copy_from_slice(b"DOS\x01");
        b
    }

    /// An X68000 SCSI hard disk: `X68SCSI1` opens block 0.
    pub fn x68k_scsi() -> Vec<u8> {
        let mut b = zeros(4096);
        b[..8].copy_from_slice(b"X68SCSI1");
        b[8..10].copy_from_slice(&512u16.to_be_bytes());
        b
    }

    /// An FDI 2.0 disk (Disk2FDI): the signature line, a creator, the
    /// comment field, EOF (0x1A) and version 2.0.
    pub fn fdi2() -> Vec<u8> {
        let mut b = zeros(4096);
        let line = b"Formatted Disk Image file\r\n";
        b[..line.len()].copy_from_slice(line);
        b[27..57].fill(b' ');
        b[57..59].copy_from_slice(b"\r\n");
        b[59..139].fill(b' ');
        b[139] = 0x1A;
        b[140] = 2;
        b[141] = 0;
        b
    }

    /// An Atari ST FastCopy DIM: `42 42` at 0, then the geometry block.
    pub fn st_dim() -> Vec<u8> {
        let mut b = zeros(32 + 720 * 1024);
        b[0] = 0x42;
        b[1] = 0x42;
        b[6] = 1;
        b[8] = 79;
        b[9] = 1;
        b
    }

    /// An Apple II WOZ disk (`version` 1 or 2): `WOZn`, `FF 0A 0D 0A`, a
    /// zero CRC, then the `INFO` chunk id.
    pub fn woz(version: u8) -> Vec<u8> {
        let mut b = zeros(4096);
        b[..4].copy_from_slice(&[b'W', b'O', b'Z', b'0' + version]);
        b[4..8].copy_from_slice(&[0xFF, 0x0A, 0x0D, 0x0A]);
        b[12..16].copy_from_slice(b"INFO");
        b
    }

    /// An Apple II 2IMG disk: `2IMG`, creator `CTKG`, header size 64,
    /// version 1, ProDOS order.
    pub fn two_img() -> Vec<u8> {
        let mut b = zeros(4096);
        b[..8].copy_from_slice(b"2IMGCTKG");
        b[8..10].copy_from_slice(&64u16.to_le_bytes());
        b[10..12].copy_from_slice(&1u16.to_le_bytes());
        b[12] = 1;
        b
    }

    /// An Uzebox `.uze` ROM: the 512-byte header `packrom` writes, marker
    /// `UZEBOX`, header version 1, target 0 (ATmega644), program size,
    /// year, then the name and the program.
    pub fn uzebox() -> Vec<u8> {
        let mut b = zeros(512 + 4096);
        b[..6].copy_from_slice(b"UZEBOX");
        b[6] = 1;
        b[8..12].copy_from_slice(&4096u32.to_le_bytes());
        b[12..14].copy_from_slice(&2008u16.to_le_bytes());
        b[14..18].copy_from_slice(b"Game");
        b
    }

    /// A Vircon32 cartridge: `V32-CART`, then the version and revision
    /// words and the title.
    pub fn vircon32() -> Vec<u8> {
        let mut b = zeros(4096);
        b[..8].copy_from_slice(b"V32-CART");
        b[8..12].copy_from_slice(&1u32.to_le_bytes());
        b[16..20].copy_from_slice(b"Game");
        b
    }

    /// A WebAssembly module (`\0asm`, version 1): a WASM-4 cart, or any
    /// other `.wasm` file.
    pub fn wasm() -> Vec<u8> {
        let mut b = zeros(1024);
        b[..8].copy_from_slice(b"\0asm\x01\0\0\0");
        b
    }

    /// A Doom WAD: `magic` (`IWAD` or `PWAD`), the lump count and the
    /// directory offset (little-endian), then zeros.
    pub fn wad(magic: &[u8; 4]) -> Vec<u8> {
        let mut b = zeros(4096);
        b[..4].copy_from_slice(magic);
        b[4..8].copy_from_slice(&1u32.to_le_bytes());
        b[8..12].copy_from_slice(&4080u32.to_le_bytes());
        b[4080..4084].copy_from_slice(&12u32.to_le_bytes());
        b[4088..4096].copy_from_slice(b"PLAYPAL\0");
        b
    }

    /// A Quake/OpenBOR `PACK` archive: magic, directory offset and size.
    pub fn pack() -> Vec<u8> {
        let mut b = zeros(4096);
        b[..4].copy_from_slice(b"PACK");
        b[4..8].copy_from_slice(&4032u32.to_le_bytes());
        b[8..12].copy_from_slice(&64u32.to_le_bytes());
        b
    }

    /// A Commodore `.prg`: the little-endian load address, then a BASIC
    /// line link.
    pub fn prg(load: [u8; 2]) -> Vec<u8> {
        let mut b = zeros(1024);
        b[..2].copy_from_slice(&load);
        b[2] = 0x0B;
        b[3] = load[1];
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
