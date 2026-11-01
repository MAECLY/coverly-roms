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

use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::identify::names::Region;
use crate::identify::sniff::{NotARom, ReadAt};

pub mod ctr;
pub mod nintendo_disc;
pub mod nx;
pub mod psf;
pub mod sony;
#[cfg(any(test, feature = "test-util"))]
pub mod testing;
pub mod wad;
pub mod wiiu;
pub mod xbox;
pub mod zarchive;

/// Most bytes read from one item (D24).
pub const MAX_READ: u64 = 4 << 20;
/// Most bytes decompressed (zlib, zstd) for one item (D24).
pub const MAX_DECODED: u64 = 1 << 20;
/// Most seeks (reads that do not continue the previous one) per item.
pub const MAX_SEEKS: u32 = 16;
/// Largest metadata file read whole (`PARAM.SFO`, `meta.xml`, `cnmt.xml`).
pub const MAX_META_FILE: u64 = 64 << 10;

/// What a title is, from its title id or content type (spec §8.0, D11).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
    /// PS5 `sce_sys/param.json` (systems batch 3 §8.2).
    ParamJson,
    /// Xbox GDK `MicrosoftGame.config` (systems batch 3 §8.3).
    MicrosoftGameConfig,
    /// Xbox ERA `AppxManifest.xml` (systems batch 3 §8.3).
    AppxManifest,
}

/// A title id and where it came from, as a plan row shows it (spec §10:
/// «Title ID … en PARAM.SFO», «… en el nombre del ticket», «… tomado del
/// nombre del archivo»).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TitleIdRef {
    pub id: String,
    pub source: IdSource,
}

impl TitleIdRef {
    /// The title id of `c` with its source, when the reader recorded both.
    pub fn of(c: &ConsoleId) -> Option<TitleIdRef> {
        Some(TitleIdRef {
            id: c.title_id.clone()?,
            source: c.id_source?,
        })
    }
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
    /// PS4/PS5 36-character content id; XVC ContentId GUID (systems
    /// batch 3 §8.0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_id: Option<String>,
    /// From the first letter of the content id (systems batch 3 D8).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<Region>,
    /// GDK StoreId, ERA Identity Name, XVC ProductId GUID (systems
    /// batch 3 D14, D15).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub product_id: Option<String>,
    /// GDK DLC: its single AllowedProduct (systems batch 3 D15).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
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
            content_id: None,
            region: None,
            product_id: None,
            parent_id: None,
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
const KEY_EXTS: &[&str] = &["rap", "rif", "dkey", "zrif", "tik", "cetk", "cik"];

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

/// Opens a ROM candidate for reading. Key material is refused by name
/// before the open (D4) with `PermissionDenied`. The walk and the scan
/// already leave it out, but a `.cue`, `.gdi` or `.m3u` is user data and
/// may name one as its track, so a refusal is an ordinary error, never a
/// panic. Under `test-util`, every path is recorded while an open log
/// runs ([`start_open_log`]).
pub fn open_rom(path: &Path) -> io::Result<File> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or_default();
    if is_key_material(&name) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "key material is never opened",
        ));
    }
    #[cfg(feature = "test-util")]
    open_log::record(path);
    File::open(path)
}

/// Every path [`open_rom`] opened, across threads (rayon workers too).
#[cfg(feature = "test-util")]
mod open_log {
    use std::path::{Path, PathBuf};
    use std::sync::{Mutex, PoisonError};

    pub(super) static LOG: Mutex<Option<Vec<PathBuf>>> = Mutex::new(None);

    pub(super) fn record(path: &Path) {
        if let Some(log) = LOG.lock().unwrap_or_else(PoisonError::into_inner).as_mut() {
            log.push(path.to_path_buf());
        }
    }
}

/// Starts (or restarts, empty) the process-wide log of [`open_rom`].
#[cfg(feature = "test-util")]
pub fn start_open_log() {
    *open_log::LOG
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Vec::new());
}

/// Stops the log and returns every path opened since [`start_open_log`].
#[cfg(feature = "test-util")]
pub fn take_open_log() -> Vec<PathBuf> {
    open_log::LOG
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take()
        .unwrap_or_default()
}

/// Every file reader of the batch, in a fixed order; `None` = not a
/// console format this batch knows. `name` is the file name (Switch
/// title-id tokens, spec D10); `""` when unknown.
///
/// The readers join this list, in this order, as their systems land:
/// `nx`, `nintendo_disc`, `wad`, `ctr`, `wiiu`, `sony`, `xbox`. Each is
/// called only when its magic or extension can apply.
///
/// `nx` goes first and reads only 0x10 bytes before it knows: the other
/// readers' first reads (0x100 and 0x200 bytes) would reach into the
/// first entry of a small NSP (a ticket, an NCA) and into an XCI's
/// encrypted gamecard info at 0x190, which are never read (D4).
pub fn identify(src: &mut Budgeted, ext: &str, name: &str) -> Option<ConsoleId> {
    if let Some(id) = nx::identify(src, ext, name) {
        return Some(id);
    }
    // The disc magics are specific enough for any extension, and so are
    // the 8-byte WAD header and the 3DS magics (`NCSD`/`NCCH` at 0x100,
    // `3DSX`, `Z3DS`); the CIA header only counts under `.cia`.
    nintendo_disc::identify(src)
        .or_else(|| wad::identify(src))
        .or_else(|| ctr::identify(src, ext))
        .or_else(|| wiiu::identify(src, ext))
        .or_else(|| sony::identify(src, ext))
        .or_else(|| xbox::identify(src, ext))
}

/// The names the walk already listed for one directory (spec D12): every
/// entry, files and folders, AppleDouble twins and OS junk left out.
pub struct DirListing<'a> {
    pub dir: &'a Path,
    pub names: &'a [String],
}

/// A folder probe (spec D12): looks only at `names`, may
/// `symlink_metadata` named sub-paths, may `read_dir` one named child
/// (at most 256 entries) and read one probe file (at most
/// [`MAX_META_FILE`]) through [`Budgeted`].
pub type DirProbe = fn(&DirListing) -> Option<FolderMatch>;

/// Why a probe that recognised a folder leaves it in place (systems
/// batch 3 plan P-2): the walk turns it into one left item, not descended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FolderLeave {
    /// The folder is not a console game (a GDK `PC` build, a console
    /// system folder).
    NotARom(NotARom),
    /// The probe file exists but cannot be read (Windows ACLs).
    Unreadable,
}

/// What a folder probe proved: the game and its probe file.
#[derive(Clone, Debug, PartialEq)]
pub struct FolderMatch {
    pub id: ConsoleId,
    /// The probe file, relative to the folder.
    pub probe: PathBuf,
    /// Set when the folder is recognised but stays where it is.
    pub leave: Option<FolderLeave>,
}

/// Two probes matched one folder and named different systems: it is not
/// a real game, so it goes to review.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirConflict {
    /// Every system named, in probe order, each once.
    pub systems: Vec<String>,
}

/// A probe that may name several systems for one folder (the `sce_sys`
/// probe sees a PSF contradiction itself, systems batch 3 D5): each
/// match counts as if a separate probe had made it.
pub type DirProbeMany = fn(&DirListing) -> Vec<FolderMatch>;

/// One entry of the probe list.
#[derive(Clone, Copy)]
enum ProbeFn {
    One(DirProbe),
    Many(DirProbeMany),
}

/// The built-in folder probes, in the fixed order of spec D12 and
/// systems batch 3 §5.1: PS3 JB, PS3 HDD, the `sce_sys` app probe (PS4,
/// PS5 and PS Vita, one listing of `sce_sys`; it replaces the Vita
/// probe), Wii U loadiine then NUS, then Xbox (`default.xbe`), Xbox 360
/// (`default.xex`, GOD/XBLA), then split Switch files.
const DIR_PROBES: &[ProbeFn] = &[
    ProbeFn::One(sony::probe_ps3_jb),
    ProbeFn::One(sony::probe_ps3_hdd),
    ProbeFn::Many(sony::sce_matches),
    ProbeFn::One(wiiu::probe_loadiine),
    ProbeFn::One(wiiu::probe_nus),
    ProbeFn::One(xbox::probe_xbe),
    ProbeFn::One(xbox::probe_xex),
    ProbeFn::One(xbox::probe_god),
    ProbeFn::One(nx::probe_split),
];

#[cfg(any(test, feature = "test-util"))]
thread_local! {
    static EXTRA_PROBES: std::cell::RefCell<Vec<DirProbe>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Test hook (plan P-7): `Some(p)` makes `p` the only extra probe on the
/// calling thread, after the built-in ones; `None` clears the extras.
#[doc(hidden)]
#[cfg(any(test, feature = "test-util"))]
pub fn set_extra_probe(p: Option<DirProbe>) {
    EXTRA_PROBES.with(|c| *c.borrow_mut() = p.into_iter().collect());
}

/// Test hook: adds one more extra probe on the calling thread.
#[doc(hidden)]
#[cfg(any(test, feature = "test-util"))]
pub fn add_extra_probe(p: DirProbe) {
    EXTRA_PROBES.with(|c| c.borrow_mut().push(p));
}

fn probes() -> Vec<ProbeFn> {
    #[allow(unused_mut)]
    let mut all: Vec<ProbeFn> = DIR_PROBES.to_vec();
    #[cfg(any(test, feature = "test-util"))]
    EXTRA_PROBES.with(|c| all.extend(c.borrow().iter().copied().map(ProbeFn::One)));
    all
}

/// Whether `listing` is a folder game (spec D12): the probes run in
/// their fixed order and the first match wins; a later match naming
/// another system makes it a [`DirConflict`] (review).
pub fn identify_dir(listing: &DirListing) -> Result<Option<FolderMatch>, DirConflict> {
    let mut first: Option<FolderMatch> = None;
    let mut systems: Vec<String> = Vec::new();
    for probe in probes() {
        let found = match probe {
            ProbeFn::One(p) => p(listing).into_iter().collect(),
            ProbeFn::Many(p) => p(listing),
        };
        for found in found {
            if !systems.contains(&found.id.system) {
                systems.push(found.id.system.clone());
            }
            first.get_or_insert(found);
        }
    }
    if systems.len() > 1 {
        return Err(DirConflict { systems });
    }
    Ok(first)
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

/// The title id of the base game `id` belongs to (spec D11), for
/// grouping updates and DLC with their base: Switch `id & !0x1FFF`, 3DS
/// and Wii U the base high word with the same low 32 bits, Sony (PS3,
/// Vita, PS4, PS5: systems batch 3 D15) and Xbox 360 the title id itself. `None` for other systems or without a
/// title id.
pub fn base_title_id(id: &ConsoleId) -> Option<String> {
    let tid = id.title_id.as_deref()?.trim();
    let hex = || {
        (tid.len() == 16)
            .then(|| u64::from_str_radix(tid, 16).ok())
            .flatten()
    };
    match id.system.as_str() {
        "switch" => Some(format!("{:016X}", switch_base(hex()?))),
        "3ds" => Some(format!("00040000{:08X}", hex()? as u32)),
        "wiiu" => Some(format!("00050000{:08X}", hex()? as u32)),
        "ps3" | "psvita" | "xbox360" | "ps4" | "ps5" if !tid.is_empty() => {
            Some(tid.to_ascii_uppercase())
        }
        _ => None,
    }
}

/// The region of a PS4/PS5 content id, from its first letter, as the
/// shadPS4 launcher's `GameListUtils::GetRegion(content_id[0])` reads it
/// (systems batch 3 D8).
pub fn region_from_content_id(content_id: &str) -> Option<Region> {
    match content_id.as_bytes().first()? {
        b'U' => Some(Region::Usa),
        b'E' => Some(Region::Europe),
        b'J' => Some(Region::Japan),
        b'H' => Some(Region::Asia),
        b'I' => Some(Region::World),
        _ => None,
    }
}

/// A plaintext title from a header field: NULs and padding trimmed,
/// controls and bidi overrides removed, at most 128 characters; `None`
/// when nothing printable is left. Used by the readers as they land.
pub(crate) fn clean_title(raw: &str) -> Option<String> {
    crate::sources::clean_text(raw.trim_end_matches('\0').trim(), 128)
}

#[cfg(test)]
mod tests {
    use super::testing::{self, RangeLog, SparseAt};
    use super::*;
    use crate::identify::names::Region;
    use crate::identify::sniff::NotARom;

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
    fn base_title_ids() {
        let with = |system: &str, tid: &str| {
            let mut id = ConsoleId::proven(system, "x");
            id.title_id = Some(tid.into());
            base_title_id(&id)
        };
        assert_eq!(
            with("switch", "0100000000011001").as_deref(),
            Some("0100000000010000")
        );
        assert_eq!(
            with("3ds", "0004000E00055D00").as_deref(),
            Some("0004000000055D00")
        );
        assert_eq!(
            with("wiiu", "0005000C10145D00").as_deref(),
            Some("0005000010145D00")
        );
        assert_eq!(with("ps3", "blus30001").as_deref(), Some("BLUS30001"));
        // Systems batch 3 D15: the PS4/PS5 title id itself, upper-cased.
        assert_eq!(with("ps4", "cusa00900").as_deref(), Some("CUSA00900"));
        assert_eq!(with("ps5", "PPSA01234").as_deref(), Some("PPSA01234"));
        assert_eq!(with("ps4", " "), None);
        assert_eq!(with("switch", "nothex"), None);
        assert_eq!(with("gamecube", "0100000000010000"), None);
    }

    #[test]
    fn identify_zeros_is_nothing() {
        let mut src = SparseAt::new(0x8000);
        let mut b = Budgeted::new(&mut src, 0x8000);
        assert_eq!(identify(&mut b, "iso", "Game.iso"), None);
    }

    #[test]
    fn identify_discs_and_wads() {
        let mut disc = testing::gc_disc("GM8E01", false);
        let len = disc.len;
        let id = identify(&mut Budgeted::new(&mut disc, len), "iso", "").unwrap();
        assert_eq!(id.system, "gamecube");
        let mut wad = testing::wii_wad(0x0001_0001_5741_4145);
        let len = wad.len;
        let mut log = RangeLog::new(&mut wad);
        let id = identify(&mut Budgeted::new(&mut log, len), "wad", "").unwrap();
        assert_eq!(
            (id.system.as_str(), id.serial.as_deref()),
            ("wii", Some("WAAE"))
        );
        assert!(!log.touched(testing::WAD_TICKET.0, testing::WAD_TICKET.1));
    }

    fn probe_gb(l: &DirListing) -> Option<FolderMatch> {
        l.names.iter().any(|n| n == "A").then(|| FolderMatch {
            id: ConsoleId::proven("gb", "a"),
            probe: PathBuf::from("A"),
            leave: None,
        })
    }

    fn probe_gb_too(l: &DirListing) -> Option<FolderMatch> {
        l.names.iter().any(|n| n == "B").then(|| FolderMatch {
            id: ConsoleId::proven("gb", "b"),
            probe: PathBuf::from("B"),
            leave: None,
        })
    }

    fn probe_gba(l: &DirListing) -> Option<FolderMatch> {
        l.names.iter().any(|n| n == "B").then(|| FolderMatch {
            id: ConsoleId::proven("gba", "b"),
            probe: PathBuf::from("B"),
            leave: None,
        })
    }

    #[test]
    fn identify_dir_first_match_wins_and_systems_conflict() {
        let dir = Path::new("x");
        let names = ["A".to_string(), "B".to_string()];
        let listing = DirListing { dir, names: &names };
        set_extra_probe(None);
        assert_eq!(identify_dir(&listing), Ok(None));
        set_extra_probe(Some(probe_gb));
        add_extra_probe(probe_gb_too);
        let found = identify_dir(&listing).unwrap().unwrap();
        assert_eq!((found.id.detail.as_str(), found.probe), ("a", "A".into()));
        add_extra_probe(probe_gba);
        assert_eq!(
            identify_dir(&listing),
            Err(DirConflict {
                systems: vec!["gb".into(), "gba".into()]
            })
        );
        let only_a = ["A".to_string()];
        let listing = DirListing {
            dir,
            names: &only_a,
        };
        assert_eq!(identify_dir(&listing).unwrap().unwrap().id.system, "gb");
        set_extra_probe(None);
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
    fn cik_is_key_material() {
        assert!(is_key_material("x.cik"));
        assert!(is_key_material("X.CIK"));
        // Spec D4: `sce_sys/license.dat` moves with its folder and is
        // never opened, but it is not listed as key material.
        assert!(!is_key_material("license.dat"));
    }

    #[test]
    fn console_id_new_fields_serde() {
        let bare = ConsoleId::proven("ps4", "PKG header");
        assert_eq!(
            (
                &bare.content_id,
                bare.region,
                &bare.product_id,
                &bare.parent_id
            ),
            (&None, None, &None, &None)
        );
        assert_eq!(
            serde_json::to_string(&bare).unwrap(),
            r#"{"system":"ps4","confidence":0.95,"kind":"unknown","detail":"PKG header"}"#
        );
        let old: ConsoleId = serde_json::from_str(
            r#"{"system":"ps3","confidence":0.9,"kind":"base","detail":"PARAM.SFO"}"#,
        )
        .unwrap();
        assert_eq!(
            (&old.content_id, old.region, &old.product_id, &old.parent_id),
            (&None, None, &None, &None)
        );
        let mut full = bare.clone();
        full.content_id = Some("UP9000-CUSA00900_00-BLOODBORNE000000".into());
        full.region = Some(Region::Usa);
        full.product_id = Some("9NBLGGH4R315".into());
        full.parent_id = Some("9N5SZL6TKX1Z".into());
        full.id_source = Some(IdSource::MicrosoftGameConfig);
        let json = serde_json::to_string(&full).unwrap();
        assert!(
            json.contains(r#""content_id":"UP9000-CUSA00900_00-BLOODBORNE000000""#),
            "{json}"
        );
        assert!(json.contains(r#""region":"usa""#), "{json}");
        assert!(json.contains(r#""product_id":"9NBLGGH4R315""#), "{json}");
        assert!(json.contains(r#""parent_id":"9N5SZL6TKX1Z""#), "{json}");
        assert!(
            json.contains(r#""id_source":"microsoft-game-config""#),
            "{json}"
        );
        let back: ConsoleId = serde_json::from_str(&json).unwrap();
        assert_eq!(back, full);
        for (src, text) in [
            (IdSource::ParamJson, "\"param-json\""),
            (IdSource::MicrosoftGameConfig, "\"microsoft-game-config\""),
            (IdSource::AppxManifest, "\"appx-manifest\""),
        ] {
            assert_eq!(serde_json::to_string(&src).unwrap(), text);
        }
    }

    #[test]
    fn region_letters() {
        for (id, region) in [
            ("UP9000-CUSA00900_00-BLOODBORNE000000", Region::Usa),
            ("EP9000-CUSA00207_00-BLOODBORNE000000", Region::Europe),
            ("JP9000-CUSA00208_00-BLOODBORNE000000", Region::Japan),
            ("HP9000-CUSA01363_00-BLOODBORNE000000", Region::Asia),
            ("IP9000-CUSA00001_00-TESTTEST00000000", Region::World),
        ] {
            assert_eq!(region_from_content_id(id), Some(region), "{id}");
        }
        for id in ["KP9000-CUSA00001_00-X", "X", ""] {
            assert_eq!(region_from_content_id(id), None, "{id}");
        }
    }

    #[test]
    fn folder_leave_is_carried_by_a_match() {
        let m = FolderMatch {
            id: ConsoleId::proven("xboxone", "MicrosoftGame.config"),
            probe: PathBuf::from("MicrosoftGame.config"),
            leave: Some(FolderLeave::NotARom(NotARom::PcGameData)),
        };
        assert_eq!(m.leave, Some(FolderLeave::NotARom(NotARom::PcGameData)));
        assert_ne!(Some(FolderLeave::Unreadable), m.leave);
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
