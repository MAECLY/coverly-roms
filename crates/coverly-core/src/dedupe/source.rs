//! The two seams through which duplicates reads inside containers (plan
//! DD-1): [`EntryListing`] for archive entries and [`DiscProbe`] for disc
//! identity. Both are narrow so the archive and disc readers can be swapped
//! in later without touching the planner. A third, [`DatLookup`], answers
//! catalogue questions for levels 2 and 3. Read-only.

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::cache::Cache;
use crate::identify::dat::dat_sources;
use crate::identify::hash::{hash_stream, HeaderRule};
use crate::systems::SystemDb;

/// An archive with more entries than this is not listed (a ROM set never
/// comes close); listing it is an error so the unit is reported unreadable.
const MAX_ENTRIES: usize = 4096;

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

/// [`EntryListing`] for zip files, through the `zip` crate. Other archive
/// formats are name-only until the archive reader lands (plan Task 25).
#[derive(Clone, Copy, Debug, Default)]
pub struct ZipListing;

impl ZipListing {
    fn open(path: &Path) -> io::Result<Option<zip::ZipArchive<File>>> {
        let is_zip = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("zip"));
        if !is_zip {
            return Ok(None);
        }
        let archive = zip::ZipArchive::new(File::open(path)?).map_err(invalid)?;
        if archive.len() > MAX_ENTRIES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("more than {MAX_ENTRIES} entries"),
            ));
        }
        Ok(Some(archive))
    }

    /// The listed entries with their index in the archive.
    fn entries(archive: &mut zip::ZipArchive<File>) -> io::Result<Vec<(usize, ListedEntry)>> {
        let mut out = Vec::new();
        for i in 0..archive.len() {
            let entry = archive.by_index_raw(i).map_err(invalid)?;
            if entry.is_dir() {
                continue;
            }
            let name = entry.name().map_err(invalid)?.into_owned();
            out.push((
                i,
                ListedEntry {
                    name,
                    size: entry.size(),
                    crc32: Some(entry.crc32()),
                },
            ));
        }
        Ok(out)
    }
}

impl EntryListing for ZipListing {
    fn list(&self, path: &Path) -> io::Result<Option<Vec<ListedEntry>>> {
        let Some(mut archive) = Self::open(path)? else {
            return Ok(None);
        };
        let entries = Self::entries(&mut archive)?;
        Ok(Some(entries.into_iter().map(|(_, e)| e).collect()))
    }

    fn payload_sha1(
        &self,
        path: &Path,
        entry: usize,
        rule: HeaderRule,
        budget: u64,
    ) -> io::Result<Option<String>> {
        let Some(mut archive) = Self::open(path)? else {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "not a zip"));
        };
        let entries = Self::entries(&mut archive)?;
        let Some((index, listed)) = entries.into_iter().nth(entry) else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("no entry {entry}"),
            ));
        };
        if listed.size > budget {
            return Ok(None);
        }
        let file = archive.by_index(index).map_err(invalid)?;
        // One byte past the size shows an entry that inflates to more.
        let h = hash_stream(file.take(listed.size.saturating_add(1)), listed.size, rule)?;
        let crc = listed.crc32.map(|c| format!("{c:08X}"));
        if h.full.size != listed.size || crc.as_deref() != Some(h.full.crc32.as_str()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("entry {} does not match its directory", listed.name),
            ));
        }
        Ok(h.payload_sha1)
    }

    fn head(&self, path: &Path, entry: usize, len: usize) -> io::Result<Option<Vec<u8>>> {
        let Some(mut archive) = Self::open(path)? else {
            return Ok(None);
        };
        let entries = Self::entries(&mut archive)?;
        let Some((index, _)) = entries.into_iter().nth(entry) else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("no entry {entry}"),
            ));
        };
        let file = archive.by_index(index).map_err(invalid)?;
        let mut head = Vec::with_capacity(len);
        file.take(len as u64).read_to_end(&mut head)?;
        Ok(Some(head))
    }
}

fn invalid(e: impl std::error::Error + Send + Sync + 'static) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, e)
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
/// disc reading" note until the disc reader lands (plan Task 25).
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

    fn by_serial(&self, system: &str, serial: &str) -> Vec<DatHit> {
        self.dats(system)
            .iter()
            .flat_map(|dat| self.cache.dat_by_serial(dat, serial).unwrap_or_default())
            .map(|r| hit(r.game))
            .collect()
    }
}

/// A cached DAT row keeps no `isbios` column for its game, so the BIOS
/// mark comes from the name; units already left out every BIOS file the
/// game list names (spec §5.1).
fn hit(game: String) -> DatHit {
    let is_bios = game.to_lowercase().contains("[bios]");
    DatHit { game, is_bios }
}
