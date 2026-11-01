//! The two seams through which duplicates reads inside containers (plan
//! DD-1): [`EntryListing`] for archive entries and [`DiscProbe`] for disc
//! identity. Both are narrow, so the planner never sees the readers: the
//! real ones are [`ArchiveEntryListing`](super::archive::ArchiveEntryListing)
//! over the archive reader and [`ReaderDiscProbe`](super::disc::ReaderDiscProbe)
//! over the disc reader; the fakes live in `dedupe::testing`. A third,
//! [`DatLookup`], answers catalogue questions for levels 2 and 3. Read-only.

use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::cache::Cache;
use crate::identify::dat::dat_sources;
use crate::identify::hash::HeaderRule;
use crate::systems::SystemDb;

/// One file entry of an archive, as its directory states it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListedEntry {
    pub name: String,
    /// Uncompressed size.
    pub size: u64,
    /// CRC32 from the directory, when the format stores one.
    pub crc32: Option<u32>,
}

/// Reads archive entries without extracting them.
pub trait EntryListing: Send + Sync {
    /// The file entries of `path` (directories left out), from the archive's
    /// directory only. `None` when `path` is not an archive this lister
    /// reads; an error when it is one but cannot be read.
    fn list(&self, path: &Path) -> io::Result<Option<Vec<ListedEntry>>>;

    /// Payload SHA1 (upper-case hex, `rule` applied) of entry `entry`, an
    /// index into [`EntryListing::list`]. Decodes at most `budget` bytes:
    /// `None` when the entry is larger. An error when the entry does not
    /// decode to the size and CRC its directory states.
    fn payload_sha1(
        &self,
        path: &Path,
        entry: usize,
        rule: HeaderRule,
        budget: u64,
    ) -> io::Result<Option<String>>;

    /// The first `len` decoded bytes of entry `entry` (fewer when the entry
    /// is shorter), to find a header that only its magic shows. `None`
    /// when this lister cannot decode: the entry then counts as headerless.
    fn head(&self, path: &Path, entry: usize, len: usize) -> io::Result<Option<Vec<u8>>> {
        let _ = (path, entry, len);
        Ok(None)
    }
}

/// What tells two builds of one disc apart (spec §5.4): the ISO 9660
/// primary volume descriptor and the SHA1 of the disc's own boot file.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VolumeFingerprint {
    /// PVD volume space size, in logical blocks.
    pub space_size: u32,
    /// PVD creation date and time (17-byte dec-datetime).
    pub created: [u8; 17],
    /// PVD modification date and time.
    pub modified: [u8; 17],
    /// Upper-case hex SHA1 of `SYSTEM.CNF`, `IP.BIN` or `PSP_GAME/PARAM.SFO`.
    pub boot_sha1: String,
}

/// Where a disc's serials came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum DiscIdSource {
    /// The `BOOT` line of `SYSTEM.CNF` (PS1, PS2).
    SystemCnf,
    /// `DISC_ID` of a `PARAM.SFO` (PSP, PBP index).
    ParamSfo,
    /// The product number of `IP.BIN` (Saturn, Sega CD, Dreamcast).
    IpBin,
    /// `UMD_DATA.BIN` of a UMD image.
    UmdData,
    /// CHD metadata or its first hunks.
    Chd,
    /// The disc's position in an m3u playlist.
    M3uPosition,
    /// A system signature or the file system alone, with no boot record
    /// read (PC Engine CD, Neo Geo CD, 3DO; a PlayStation volume whose
    /// `SYSTEM.CNF` was out of reach, its serial from the boot file name).
    Signature,
}

/// The identity of a disc unit (one disc, or a multi-disc unit in order).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscId {
    /// Normalised serials (`SLUS-00543`), in disc order.
    pub serials: Vec<String>,
    /// 1-based disc number when known.
    pub disc_no: Option<u8>,
    /// `None` when unreadable: a level 3 group then stays report-only.
    pub volume: Option<VolumeFingerprint>,
    pub source: DiscIdSource,
}

/// Reads disc identity for level 3 (spec §5.4).
pub trait DiscProbe: Send + Sync {
    /// The identity of the disc unit at `path` in `system`; `None` when the
    /// probe cannot tell (not a disc, no serial found).
    fn probe(&self, path: &Path, system: &str) -> io::Result<Option<DiscId>>;

    /// Whether this probe reads discs at all. When false, level 3 is off
    /// and the plan says it needs disc reading.
    fn reads_discs(&self) -> bool {
        true
    }
}

/// A [`DiscProbe`] that reads nothing: level 3 is disabled with the "needs
/// disc reading" note (tests, and callers that read no discs).
#[derive(Clone, Copy, Debug, Default)]
pub struct NoDiscProbe;

impl DiscProbe for NoDiscProbe {
    fn probe(&self, _path: &Path, _system: &str) -> io::Result<Option<DiscId>> {
        Ok(None)
    }

    fn reads_discs(&self) -> bool {
        false
    }
}

/// One catalogue entry a lookup found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatHit {
    /// The DAT game name (`Tetris (World) (Rev 1)`).
    pub game: String,
    /// The DAT marks the game as a BIOS (`isbios`, a `[BIOS]` name).
    pub is_bios: bool,
}

/// The catalogue questions levels 2 and 3 ask, scoped to one system: no
/// answer ever comes from another system's catalogue.
pub trait DatLookup: Send + Sync {
    /// Entries of `system`'s catalogues whose ROM has this CRC32 and size
    /// (or no size). A hit by hash, never by name.
    fn by_crc(&self, system: &str, crc32: u32, size: u64) -> Vec<DatHit>;

    /// Entries of `system`'s catalogues with this normalised serial.
    fn by_serial(&self, system: &str, serial: &str) -> Vec<DatHit>;

    /// Whether `name` is a set (game) name in `system`'s catalogues: an
    /// arcade core loads a set by its zip stem. `None` when no catalogue
    /// of `system` is at hand, so names cannot be told apart.
    fn set_name(&self, _system: &str, _name: &str) -> Option<bool> {
        None
    }
}

/// A [`DatLookup`] with no catalogue: offline, or nothing cached.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoDats;

impl DatLookup for NoDats {
    fn by_crc(&self, _system: &str, _crc32: u32, _size: u64) -> Vec<DatHit> {
        Vec::new()
    }

    fn by_serial(&self, _system: &str, _serial: &str) -> Vec<DatHit> {
        Vec::new()
    }
}

/// [`DatLookup`] over the DATs already in the cache (nothing is
/// downloaded here). A cache error reads as no hit.
pub struct CacheDats<'a> {
    cache: &'a Cache,
    db: &'a SystemDb,
}

impl<'a> CacheDats<'a> {
    pub fn new(cache: &'a Cache, db: &'a SystemDb) -> CacheDats<'a> {
        CacheDats { cache, db }
    }

    fn dats(&self, system: &str) -> Vec<String> {
        self.db.get(system).map(dat_sources).unwrap_or_default()
    }
}

impl DatLookup for CacheDats<'_> {
    fn by_crc(&self, system: &str, crc32: u32, size: u64) -> Vec<DatHit> {
        let crc = format!("{crc32:08X}");
        self.dats(system)
            .iter()
            .flat_map(|dat| self.cache.dat_by_crc(dat, &crc).unwrap_or_default())
            .filter(|r| r.size.is_none_or(|s| s == size))
            .map(|r| hit(r.game))
            .collect()
    }

    /// GameCube and Wii try the ID6, then its Redump middle key
    /// ([`crate::identify::dat::serial_keys`]); the first key with rows
    /// answers.
    fn by_serial(&self, system: &str, serial: &str) -> Vec<DatHit> {
        let dats = self.dats(system);
        crate::identify::dat::serial_keys(system, serial)
            .iter()
            .map(|key| {
                dats.iter()
                    .flat_map(|dat| self.cache.dat_by_serial(dat, key).unwrap_or_default())
                    .map(|r| hit(r.game))
                    .collect::<Vec<_>>()
            })
            .find(|hits| !hits.is_empty())
            .unwrap_or_default()
    }

    fn set_name(&self, system: &str, name: &str) -> Option<bool> {
        let cached = self.cache.cached_dats().ok()?;
        let dats: Vec<String> = self
            .dats(system)
            .into_iter()
            .filter(|d| cached.contains(d))
            .collect();
        if dats.is_empty() {
            return None;
        }
        let name = name.to_lowercase();
        Some(dats.iter().any(|dat| {
            self.cache
                .dat_rows_of_game(dat, &name)
                .is_ok_and(|rows| !rows.is_empty())
        }))
    }
}

/// A cached DAT row keeps no `isbios` column for its game, so the BIOS
/// mark comes from the name; units already left out every BIOS file the
/// game list names (spec §5.1).
fn hit(game: String) -> DatHit {
    let is_bios = game.to_lowercase().contains("[bios]");
    DatHit { game, is_bios }
}
