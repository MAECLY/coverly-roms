//! Local SQLite cache (spec §6 `cache/`, §16.1): quick-id memo
//! (`path + size + mtime → hashes`), source indexes and gathered candidates
//! as JSON values with ETag and fetch time, DAT rows, downloaded art files,
//! (schema v2) the organiser's per-file facts and the user's remembered
//! system choices, and (schema v3) DAT serials and game fields, the
//! duplicate finder's content keys per file and the user's keeper picks.
//! It is only a cache: a corrupt or newer database is set aside and rebuilt.
//! A database that is merely busy, locked or unreachable is left alone.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, ErrorCode, OptionalExtension};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::identify::dat::DatGame;
use crate::identify::hash::{HashOutcome, HASH_OUTCOME_VERSION};

/// A `quick_ids` row: the outcome tagged with its shape version. A v1 row
/// (the bare outcome) has no `v` and so never reads back.
#[derive(Deserialize)]
struct StoredOutcome {
    v: u32,
    outcome: HashOutcome,
}

#[derive(Serialize)]
struct StoredOutcomeRef<'a> {
    v: u32,
    outcome: &'a HashOutcome,
}
pub use crate::time::now_unix;

/// Schema version stored in `PRAGMA user_version`.
pub const SCHEMA_VERSION: i32 = 3;
/// File name of the cache database inside the app cache dir.
pub const CACHE_FILE: &str = "coverly.sqlite3";

/// How long to wait for another process (app and CLI) holding the lock.
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

const SCHEMA_V1: &str = "
CREATE TABLE quick_ids (path TEXT PRIMARY KEY, size INTEGER NOT NULL, mtime_ns INTEGER NOT NULL, outcome TEXT NOT NULL);
CREATE TABLE kv (ns TEXT NOT NULL, key TEXT NOT NULL, value TEXT NOT NULL, etag TEXT, fetched_at INTEGER NOT NULL, PRIMARY KEY (ns, key));
CREATE TABLE dat_roms (dat TEXT NOT NULL, game TEXT NOT NULL, rom_name TEXT NOT NULL, rom_stem TEXT NOT NULL,
                       size INTEGER, crc32 TEXT, md5 TEXT, sha1 TEXT);
CREATE INDEX dat_roms_crc ON dat_roms (dat, crc32);
CREATE INDEX dat_roms_stem ON dat_roms (dat, rom_stem);
CREATE TABLE art_files (url TEXT PRIMARY KEY, sha1 TEXT NOT NULL, len INTEGER NOT NULL, fetched_at INTEGER NOT NULL);
";

/// v1 → v2: CRC lookups across every DAT, rows of one game, the organiser's
/// memoised facts (`path + size + mtime → JSON`) and the user's system
/// choices keyed by content (`crc32 + size`). Idempotent.
const SCHEMA_V2: &str = "
CREATE INDEX IF NOT EXISTS dat_roms_crc_any ON dat_roms (crc32);
CREATE INDEX IF NOT EXISTS dat_roms_game ON dat_roms (dat, game);
CREATE TABLE IF NOT EXISTS organize_ids (path TEXT PRIMARY KEY, size INTEGER NOT NULL, mtime_ns INTEGER NOT NULL, facts TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS organize_choices (crc32 TEXT NOT NULL, size INTEGER NOT NULL, system_id TEXT NOT NULL, PRIMARY KEY (crc32, size));
";

/// v2 → v3 (spec §12.3): DAT serials and game fields, the duplicate
/// finder's content keys per file (keyed by volume UUID, never `dev_t`)
/// with a reverse index by key, and the user's keeper picks. Old rows stay
/// valid, but older DAT rows have no serials and a conditional request
/// would keep them so (304), so every cached DAT (`identify::dat` meta
/// namespace `dat-meta`) loses its ETag and fetch time and downloads again
/// on next use; its old rows remain the offline fallback. Runs once, under
/// the migration lock.
const SCHEMA_V3: &str = "
ALTER TABLE dat_roms ADD COLUMN serial TEXT;
ALTER TABLE dat_roms ADD COLUMN region TEXT;
ALTER TABLE dat_roms ADD COLUMN cloneof TEXT;
ALTER TABLE dat_roms ADD COLUMN isbios INTEGER NOT NULL DEFAULT 0;
CREATE INDEX IF NOT EXISTS dat_roms_serial ON dat_roms (dat, serial);
CREATE TABLE IF NOT EXISTS content_keys (volume_uuid TEXT NOT NULL, path TEXT NOT NULL, size INTEGER NOT NULL,
  mtime_ns INTEGER NOT NULL, file_id INTEGER, keys_json TEXT NOT NULL, serials_json TEXT, volume_fp TEXT,
  PRIMARY KEY (volume_uuid, path));
CREATE INDEX IF NOT EXISTS content_keys_file_id ON content_keys (volume_uuid, file_id);
CREATE TABLE IF NOT EXISTS content_key_index (key TEXT NOT NULL, volume_uuid TEXT NOT NULL, path TEXT NOT NULL,
  PRIMARY KEY (key, volume_uuid, path));
CREATE INDEX IF NOT EXISTS content_key_index_path ON content_key_index (volume_uuid, path);
CREATE TABLE IF NOT EXISTS dedupe_choices (system TEXT NOT NULL, keeper_key TEXT NOT NULL, signature TEXT NOT NULL,
  saves_fp TEXT NOT NULL, chosen_at INTEGER NOT NULL, PRIMARY KEY (system, keeper_key));
UPDATE kv SET etag = NULL, fetched_at = 0 WHERE ns = 'dat-meta';
";

/// Why the cache could not be used.
#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("cache database: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("cache data: {0}")]
    Json(#[from] serde_json::Error),
    #[error("cache folder: {0}")]
    Io(#[from] std::io::Error),
}

fn walk_size(path: &Path) -> u64 {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return 0;
    };
    if meta.is_file() {
        return meta.len();
    }
    if !meta.is_dir() {
        return 0;
    }
    std::fs::read_dir(path)
        .map(|r| r.flatten().map(|e| walk_size(&e.path())).sum())
        .unwrap_or(0)
}

/// Suffixes of the database file and its SQLite side files.
const DB_SUFFIXES: [&str; 4] = ["", "-journal", "-wal", "-shm"];

/// Bytes used by the cache database (with its journal files) and the
/// downloaded-art store.
pub fn cache_usage(cache_dir: &Path) -> u64 {
    let db: u64 = DB_SUFFIXES
        .iter()
        .map(|s| walk_size(&cache_dir.join(format!("{CACHE_FILE}{s}"))))
        .sum();
    db + walk_size(&cache_dir.join("art"))
}

/// Deletes the cache database and the art store; returns the bytes freed.
/// The next `Cache::open` creates a fresh database. Identity overrides and
/// job data live in the data dir and are kept.
pub fn clear_cache(cache_dir: &Path) -> std::io::Result<u64> {
    let before = cache_usage(cache_dir);
    for s in DB_SUFFIXES {
        match std::fs::remove_file(cache_dir.join(format!("{CACHE_FILE}{s}"))) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
    }
    match std::fs::remove_dir_all(cache_dir.join("art")) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
        _ => {}
    }
    Ok(before.saturating_sub(cache_usage(cache_dir)))
}

/// A cached JSON value with its fetch time (Unix seconds) and HTTP ETag.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stored<T> {
    pub value: T,
    pub fetched_at: i64,
    pub etag: Option<String>,
}

/// One ROM entry of a DAT file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatRow {
    pub game: String,
    pub rom_name: String,
    pub size: Option<u64>,
    pub crc32: Option<String>,
    pub md5: Option<String>,
    pub sha1: Option<String>,
    /// The normalised disc or product serial (`SLUS-00543`), when the DAT
    /// gives one on the ROM line or the game block.
    pub serial: Option<String>,
}

/// The duplicate finder's facts about one file (schema v3 `content_keys`).
///
/// The cache stores the keys as opaque JSON; `dedupe` owns their format.
/// `file_id` (the inode or NTFS file id) is only a hint for following a
/// renamed file: the caller decides whether the volume's ids are stable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContentRow {
    /// The volume's UUID ([`crate::volumes::volume_uuid`]).
    pub volume_uuid: String,
    pub path: PathBuf,
    pub size: u64,
    pub mtime_ns: i64,
    pub file_id: Option<u64>,
    /// The content keys, as `dedupe` serialises them.
    pub keys_json: String,
    /// The disc serials and number, as `dedupe` serialises them.
    pub serials_json: Option<String>,
    /// The level 3 volume fingerprint, when read.
    pub volume_fp: Option<String>,
    /// The keys this file is found under in the reverse index. Read back
    /// sorted and without repeats.
    pub index_keys: Vec<String>,
}

/// A keeper the user picked for a group (schema v3 `dedupe_choices`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredChoice {
    pub system: String,
    /// The content key of the chosen keeper.
    pub keeper_key: String,
    /// The group signature at the time of the pick (sorted member keys).
    pub signature: String,
    /// The saves fingerprint at the time of the pick.
    pub saves_fp: String,
    /// Unix seconds.
    pub chosen_at: i64,
}

/// The cache database. `Send + Sync`: one connection behind a mutex.
pub struct Cache {
    conn: Mutex<Connection>,
}

impl Cache {
    /// Opens (or creates, with its parent folders) the cache at `path`.
    ///
    /// A garbage file or one with a newer schema is renamed to
    /// `<name>.corrupt-<unix secs>` and a fresh database is created. A
    /// database another process holds (busy/locked), or one that cannot be
    /// opened or written for environmental reasons, is an error and is
    /// never moved.
    pub fn open(path: &Path) -> Result<Cache, CacheError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        match Connection::open(path)
            .map_err(CacheError::from)
            .and_then(prepare)
        {
            Ok(conn) => Ok(Cache {
                conn: Mutex::new(conn),
            }),
            Err(err) if !is_unusable_database(&err) => Err(err),
            Err(_) => {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| CACHE_FILE.to_owned());
                let aside = path.with_file_name(format!("{name}.corrupt-{}", now_unix()));
                std::fs::rename(path, &aside)?;
                for suffix in ["-journal", "-wal", "-shm"] {
                    let _ = std::fs::remove_file(path.with_file_name(format!("{name}{suffix}")));
                }
                Ok(Cache {
                    conn: Mutex::new(prepare(Connection::open(path)?)?),
                })
            }
        }
    }

    /// A private in-memory cache (tests, `--no-cache`).
    pub fn in_memory() -> Result<Cache, CacheError> {
        Ok(Cache {
            conn: Mutex::new(prepare(Connection::open_in_memory()?)?),
        })
    }

    fn conn(&self) -> MutexGuard<'_, Connection> {
        self.conn
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The hashes memoised for `path`, only if its size and mtime match.
    pub fn quick_id(
        &self,
        path: &Path,
        size: u64,
        mtime_ns: i64,
    ) -> Result<Option<HashOutcome>, CacheError> {
        let row: Option<String> = self
            .conn()
            .query_row(
                "SELECT outcome FROM quick_ids WHERE path = ?1 AND size = ?2 AND mtime_ns = ?3",
                params![path.to_string_lossy(), size as i64, mtime_ns],
                |r| r.get(0),
            )
            .optional()?;
        // An outcome of another version, or one that no longer deserialises,
        // is a miss: the file is hashed again and the row replaced.
        Ok(row
            .and_then(|json| serde_json::from_str::<StoredOutcome>(&json).ok())
            .filter(|s| s.v == HASH_OUTCOME_VERSION)
            .map(|s| s.outcome))
    }

    /// Memoises the hashes of `path` at this size and mtime.
    pub fn put_quick_id(
        &self,
        path: &Path,
        size: u64,
        mtime_ns: i64,
        outcome: &HashOutcome,
    ) -> Result<(), CacheError> {
        self.conn().execute(
            "INSERT OR REPLACE INTO quick_ids (path, size, mtime_ns, outcome) VALUES (?1, ?2, ?3, ?4)",
            params![
                path.to_string_lossy(),
                size as i64,
                mtime_ns,
                serde_json::to_string(&StoredOutcomeRef {
                    v: HASH_OUTCOME_VERSION,
                    outcome,
                })?
            ],
        )?;
        Ok(())
    }

    /// The JSON value stored under `(ns, key)`.
    pub fn get_json<T: DeserializeOwned>(
        &self,
        ns: &str,
        key: &str,
    ) -> Result<Option<Stored<T>>, CacheError> {
        let row: Option<(String, Option<String>, i64)> = self
            .conn()
            .query_row(
                "SELECT value, etag, fetched_at FROM kv WHERE ns = ?1 AND key = ?2",
                params![ns, key],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        // A value that no longer deserialises (older format) is a miss.
        Ok(row.and_then(|(json, etag, fetched_at)| {
            serde_json::from_str(&json).ok().map(|value| Stored {
                value,
                fetched_at,
                etag,
            })
        }))
    }

    /// Stores `value` under `(ns, key)`, fetched now.
    pub fn put_json<T: Serialize>(
        &self,
        ns: &str,
        key: &str,
        value: &T,
        etag: Option<&str>,
    ) -> Result<(), CacheError> {
        self.conn().execute(
            "INSERT OR REPLACE INTO kv (ns, key, value, etag, fetched_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![ns, key, serde_json::to_string(value)?, etag, now_unix()],
        )?;
        Ok(())
    }

    /// Marks `(ns, key)` as fetched now (an HTTP 304 revalidation).
    pub fn touch_json(&self, ns: &str, key: &str) -> Result<(), CacheError> {
        self.conn().execute(
            "UPDATE kv SET fetched_at = ?3 WHERE ns = ?1 AND key = ?2",
            params![ns, key, now_unix()],
        )?;
        Ok(())
    }

    /// Replaces every row of `dat` with `rows`, in one transaction. The
    /// game fields (`cloneof`, `region`, `isbios`) are left empty; use
    /// [`Cache::replace_dat_games`] to keep them.
    pub fn replace_dat(&self, dat: &str, rows: &[DatRow]) -> Result<(), CacheError> {
        self.replace_dat_with(dat, rows.iter().map(|r| (r, None)))
    }

    /// Replaces every row of `dat` with the ROMs of `games`, keeping each
    /// game's `cloneof`, `region` and BIOS mark, in one transaction.
    pub fn replace_dat_games(&self, dat: &str, games: &[DatGame]) -> Result<(), CacheError> {
        self.replace_dat_with(
            dat,
            games
                .iter()
                .flat_map(|g| g.roms.iter().map(move |r| (r, Some(g)))),
        )
    }

    fn replace_dat_with<'a>(
        &self,
        dat: &str,
        rows: impl Iterator<Item = (&'a DatRow, Option<&'a DatGame>)>,
    ) -> Result<(), CacheError> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM dat_roms WHERE dat = ?1", params![dat])?;
        {
            let mut insert = tx.prepare(
                "INSERT INTO dat_roms (dat, game, rom_name, rom_stem, size, crc32, md5, sha1, \
                 serial, region, cloneof, isbios) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            )?;
            for (r, game) in rows {
                let stem = r
                    .rom_name
                    .rsplit_once('.')
                    .map(|(s, _)| s)
                    .unwrap_or(&r.rom_name)
                    .to_lowercase();
                insert.execute(params![
                    dat,
                    r.game,
                    r.rom_name,
                    stem,
                    r.size.map(|s| s as i64),
                    r.crc32.as_deref().map(str::to_uppercase),
                    r.md5,
                    r.sha1,
                    r.serial,
                    game.and_then(|g| g.region.as_deref()),
                    game.and_then(|g| g.cloneof.as_deref()),
                    game.is_some_and(|g| g.is_bios),
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Rows of `dat` with this CRC32 (any case), ordered by game.
    pub fn dat_by_crc(&self, dat: &str, crc32: &str) -> Result<Vec<DatRow>, CacheError> {
        self.dat_query(DatFilter::Crc, dat, &crc32.to_uppercase())
    }

    /// Rows of `dat` whose ROM name without extension is `stem`, ignoring
    /// case, ordered by game.
    pub fn dat_by_rom_stem(&self, dat: &str, stem: &str) -> Result<Vec<DatRow>, CacheError> {
        self.dat_query(DatFilter::Stem, dat, &stem.to_lowercase())
    }

    /// Rows of `dat` with this normalised serial
    /// ([`crate::identify::dat::normalise_serial`]), ordered by game.
    pub fn dat_by_serial(&self, dat: &str, serial: &str) -> Result<Vec<DatRow>, CacheError> {
        self.dat_query(DatFilter::Serial, dat, serial)
    }

    fn dat_query(
        &self,
        filter: DatFilter,
        dat: &str,
        value: &str,
    ) -> Result<Vec<DatRow>, CacheError> {
        let conn = self.conn();
        let column = match filter {
            DatFilter::Crc => "crc32",
            DatFilter::Stem => "rom_stem",
            DatFilter::Serial => "serial",
        };
        let mut stmt = conn.prepare(&format!(
            "SELECT game, rom_name, size, crc32, md5, sha1, serial FROM dat_roms \
             WHERE dat = ?1 AND {column} = ?2 ORDER BY game, rom_name"
        ))?;
        let rows = stmt.query_map(params![dat, value], |r| dat_row(r, 0))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Rows of every cached DAT with this CRC32 (any case) whose size is
    /// `size` or unknown, as `(dat, row)` ordered by DAT, game and ROM name.
    pub fn dat_by_crc_any(
        &self,
        crc32: &str,
        size: u64,
    ) -> Result<Vec<(String, DatRow)>, CacheError> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT dat, game, rom_name, size, crc32, md5, sha1, serial FROM dat_roms \
             WHERE crc32 = ?1 AND (size IS NULL OR size = ?2) ORDER BY dat, game, rom_name",
        )?;
        let rows = stmt.query_map(params![crc32.to_uppercase(), size as i64], |r| {
            Ok((r.get(0)?, dat_row(r, 1)?))
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Every ROM row of `game` in `dat` (a disc set's tracks, a multi-ROM
    /// game), ordered by ROM name.
    pub fn dat_rows_of_game(&self, dat: &str, game: &str) -> Result<Vec<DatRow>, CacheError> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT game, rom_name, size, crc32, md5, sha1, serial FROM dat_roms \
             WHERE dat = ?1 AND game = ?2 ORDER BY rom_name",
        )?;
        let rows = stmt.query_map(params![dat, game], |r| dat_row(r, 0))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// `(rom_stem, game)` of every row of `dat`, ordered by stem then game
    /// so fuzzy-match ties resolve the same way on every run; stems are
    /// lower-case.
    pub fn dat_stems(&self, dat: &str) -> Result<Vec<(String, String)>, CacheError> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT rom_stem, game FROM dat_roms WHERE dat = ?1 ORDER BY rom_stem, game",
        )?;
        let rows = stmt.query_map(params![dat], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Names of the DATs with rows in the cache, sorted.
    pub fn cached_dats(&self) -> Result<Vec<String>, CacheError> {
        let conn = self.conn();
        let mut stmt = conn.prepare("SELECT DISTINCT dat FROM dat_roms ORDER BY dat")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// The organiser's facts (JSON) memoised for `path`, only if its size
    /// and mtime match.
    pub fn organize_facts(
        &self,
        path: &Path,
        size: u64,
        mtime_ns: i64,
    ) -> Result<Option<String>, CacheError> {
        Ok(self
            .conn()
            .query_row(
                "SELECT facts FROM organize_ids WHERE path = ?1 AND size = ?2 AND mtime_ns = ?3",
                params![path.to_string_lossy(), size as i64, mtime_ns],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// Memoises the organiser's facts (JSON) for `path` at this size and
    /// mtime, replacing any older entry for the path.
    pub fn put_organize_facts(
        &self,
        path: &Path,
        size: u64,
        mtime_ns: i64,
        json: &str,
    ) -> Result<(), CacheError> {
        self.conn().execute(
            "INSERT OR REPLACE INTO organize_ids (path, size, mtime_ns, facts) VALUES (?1, ?2, ?3, ?4)",
            params![path.to_string_lossy(), size as i64, mtime_ns, json],
        )?;
        Ok(())
    }

    /// The system the user chose for content with this CRC32 (any case)
    /// and size.
    pub fn organize_choice(&self, crc32: &str, size: u64) -> Result<Option<String>, CacheError> {
        Ok(self
            .conn()
            .query_row(
                "SELECT system_id FROM organize_choices WHERE crc32 = ?1 AND size = ?2",
                params![crc32.to_uppercase(), size as i64],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// Remembers the user's system for content with this CRC32 and size.
    pub fn put_organize_choice(
        &self,
        crc32: &str,
        size: u64,
        system_id: &str,
    ) -> Result<(), CacheError> {
        self.conn().execute(
            "INSERT OR REPLACE INTO organize_choices (crc32, size, system_id) VALUES (?1, ?2, ?3)",
            params![crc32.to_uppercase(), size as i64, system_id],
        )?;
        Ok(())
    }

    /// The content row of `path` on `volume_uuid`, only if its size and
    /// mtime match.
    pub fn content_row(
        &self,
        volume_uuid: &str,
        path: &Path,
        size: u64,
        mtime_ns: i64,
    ) -> Result<Option<ContentRow>, CacheError> {
        let conn = self.conn();
        let row = conn
            .query_row(
                &format!(
                    "SELECT {CONTENT_COLUMNS} FROM content_keys \
                     WHERE volume_uuid = ?1 AND path = ?2 AND size = ?3 AND mtime_ns = ?4"
                ),
                params![volume_uuid, path.to_string_lossy(), size as i64, mtime_ns],
                content_row,
            )
            .optional()?;
        row.map(|r| with_index_keys(&conn, r)).transpose()
    }

    /// The content row with this file id on `volume_uuid`, whatever its
    /// path (the first by path when a file system reused the id). A hint
    /// only: the caller checks size, mtime and whether ids are stable.
    pub fn content_by_file_id(
        &self,
        volume_uuid: &str,
        file_id: u64,
    ) -> Result<Option<ContentRow>, CacheError> {
        let conn = self.conn();
        let row = conn
            .query_row(
                &format!(
                    "SELECT {CONTENT_COLUMNS} FROM content_keys \
                     WHERE volume_uuid = ?1 AND file_id = ?2 ORDER BY path LIMIT 1"
                ),
                params![volume_uuid, file_id as i64],
                content_row,
            )
            .optional()?;
        row.map(|r| with_index_keys(&conn, r)).transpose()
    }

    /// `(volume_uuid, path)` of every file indexed under `key`, ordered.
    pub fn content_by_key(&self, key: &str) -> Result<Vec<(String, PathBuf)>, CacheError> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT volume_uuid, path FROM content_key_index WHERE key = ?1 \
             ORDER BY volume_uuid, path",
        )?;
        let rows = stmt.query_map(params![key], |r| {
            Ok((r.get(0)?, PathBuf::from(r.get::<_, String>(1)?)))
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Stores `row`, replacing any older row for its path and that row's
    /// index entries, in one transaction.
    pub fn put_content_row(&self, row: &ContentRow) -> Result<(), CacheError> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let path = row.path.to_string_lossy();
        tx.execute(
            "INSERT OR REPLACE INTO content_keys \
             (volume_uuid, path, size, mtime_ns, file_id, keys_json, serials_json, volume_fp) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                row.volume_uuid,
                path,
                row.size as i64,
                row.mtime_ns,
                row.file_id.map(|id| id as i64),
                row.keys_json,
                row.serials_json,
                row.volume_fp
            ],
        )?;
        tx.execute(
            "DELETE FROM content_key_index WHERE volume_uuid = ?1 AND path = ?2",
            params![row.volume_uuid, path],
        )?;
        {
            let mut insert = tx.prepare(
                "INSERT OR IGNORE INTO content_key_index (key, volume_uuid, path) VALUES (?1, ?2, ?3)",
            )?;
            for key in &row.index_keys {
                insert.execute(params![key, row.volume_uuid, path])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Moves the content row of `from` (and its index entries) to `to` on
    /// `volume_uuid`, after a quarantine move or an undo. A row already at
    /// `to` is replaced; no row at `from` changes nothing.
    pub fn rekey_content(
        &self,
        volume_uuid: &str,
        from: &Path,
        to: &Path,
    ) -> Result<(), CacheError> {
        let (from, to) = (from.to_string_lossy(), to.to_string_lossy());
        if from == to {
            return Ok(());
        }
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let exists: Option<i64> = tx
            .query_row(
                "SELECT 1 FROM content_keys WHERE volume_uuid = ?1 AND path = ?2",
                params![volume_uuid, from],
                |r| r.get(0),
            )
            .optional()?;
        if exists.is_some() {
            for table in ["content_keys", "content_key_index"] {
                tx.execute(
                    &format!("DELETE FROM {table} WHERE volume_uuid = ?1 AND path = ?2"),
                    params![volume_uuid, to],
                )?;
                tx.execute(
                    &format!("UPDATE {table} SET path = ?3 WHERE volume_uuid = ?1 AND path = ?2"),
                    params![volume_uuid, from, to],
                )?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// The keeper the user picked in `system` for this content key.
    pub fn dedupe_choice(
        &self,
        system: &str,
        keeper_key: &str,
    ) -> Result<Option<StoredChoice>, CacheError> {
        Ok(self
            .conn()
            .query_row(
                "SELECT system, keeper_key, signature, saves_fp, chosen_at FROM dedupe_choices \
                 WHERE system = ?1 AND keeper_key = ?2",
                params![system, keeper_key],
                |r| {
                    Ok(StoredChoice {
                        system: r.get(0)?,
                        keeper_key: r.get(1)?,
                        signature: r.get(2)?,
                        saves_fp: r.get(3)?,
                        chosen_at: r.get(4)?,
                    })
                },
            )
            .optional()?)
    }

    /// Remembers a keeper pick, replacing any older pick of that content
    /// in that system.
    pub fn put_dedupe_choice(&self, c: &StoredChoice) -> Result<(), CacheError> {
        self.conn().execute(
            "INSERT OR REPLACE INTO dedupe_choices (system, keeper_key, signature, saves_fp, chosen_at) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![c.system, c.keeper_key, c.signature, c.saves_fp, c.chosen_at],
        )?;
        Ok(())
    }

    /// Forgets every keeper pick of `systems`; returns how many. An empty
    /// list forgets nothing.
    pub fn clear_dedupe_choices(&self, systems: &[String]) -> Result<usize, CacheError> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let mut cleared = 0;
        for system in systems {
            cleared += tx.execute(
                "DELETE FROM dedupe_choices WHERE system = ?1",
                params![system],
            )?;
        }
        tx.commit()?;
        Ok(cleared)
    }

    /// Records a downloaded art file: its SHA-1 and length.
    pub fn put_art_file(&self, url: &str, sha1: &str, len: u64) -> Result<(), CacheError> {
        self.conn().execute(
            "INSERT OR REPLACE INTO art_files (url, sha1, len, fetched_at) VALUES (?1, ?2, ?3, ?4)",
            params![url, sha1, len as i64, now_unix()],
        )?;
        Ok(())
    }

    /// The SHA-1 and length recorded for `url`.
    pub fn art_file(&self, url: &str) -> Result<Option<(String, u64)>, CacheError> {
        Ok(self
            .conn()
            .query_row(
                "SELECT sha1, len FROM art_files WHERE url = ?1",
                params![url],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64)),
            )
            .optional()?)
    }
}

/// A `DatRow` from columns `game, rom_name, size, crc32, md5, sha1,
/// serial` starting at `at`.
fn dat_row(r: &rusqlite::Row<'_>, at: usize) -> rusqlite::Result<DatRow> {
    Ok(DatRow {
        game: r.get(at)?,
        rom_name: r.get(at + 1)?,
        size: r.get::<_, Option<i64>>(at + 2)?.map(|s| s as u64),
        crc32: r.get(at + 3)?,
        md5: r.get(at + 4)?,
        sha1: r.get(at + 5)?,
        serial: r.get(at + 6)?,
    })
}

/// The `content_keys` columns [`content_row`] reads, in order.
const CONTENT_COLUMNS: &str =
    "volume_uuid, path, size, mtime_ns, file_id, keys_json, serials_json, volume_fp";

/// A `ContentRow` (without index keys) from [`CONTENT_COLUMNS`].
fn content_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<ContentRow> {
    Ok(ContentRow {
        volume_uuid: r.get(0)?,
        path: PathBuf::from(r.get::<_, String>(1)?),
        size: r.get::<_, i64>(2)? as u64,
        mtime_ns: r.get(3)?,
        file_id: r.get::<_, Option<i64>>(4)?.map(|id| id as u64),
        keys_json: r.get(5)?,
        serials_json: r.get(6)?,
        volume_fp: r.get(7)?,
        index_keys: Vec::new(),
    })
}

/// `row` with its reverse-index keys, sorted.
fn with_index_keys(conn: &Connection, mut row: ContentRow) -> Result<ContentRow, CacheError> {
    let mut stmt = conn.prepare(
        "SELECT key FROM content_key_index WHERE volume_uuid = ?1 AND path = ?2 ORDER BY key",
    )?;
    row.index_keys = stmt
        .query_map(params![row.volume_uuid, row.path.to_string_lossy()], |r| {
            r.get(0)
        })?
        .collect::<Result<_, _>>()?;
    Ok(row)
}

#[derive(Clone, Copy)]
enum DatFilter {
    Crc,
    Stem,
    Serial,
}

/// Whether `err` means the file itself is unusable (garbage, newer schema)
/// rather than busy, locked, unreachable or out of space.
fn is_unusable_database(err: &CacheError) -> bool {
    match err {
        CacheError::Sql(rusqlite::Error::SqliteFailure(e, _)) => !matches!(
            e.code,
            ErrorCode::DatabaseBusy
                | ErrorCode::DatabaseLocked
                | ErrorCode::CannotOpen
                | ErrorCode::PermissionDenied
                | ErrorCode::ReadOnly
                | ErrorCode::DiskFull
                | ErrorCode::SystemIoFailure
                | ErrorCode::OutOfMemory
        ),
        CacheError::Sql(_) => true,
        CacheError::Json(_) | CacheError::Io(_) => false,
    }
}

/// Applies pragmas and migrations; fails on a database it cannot use.
fn prepare(conn: Connection) -> Result<Connection, CacheError> {
    conn.busy_timeout(BUSY_TIMEOUT)?;
    let version: i32 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    match version {
        SCHEMA_VERSION => {}
        0..=2 => migrate(&conn)?,
        _ => return Err(CacheError::Sql(rusqlite::Error::InvalidQuery)),
    }
    // Touch a table so a garbage file fails here, not on first use.
    conn.query_row("SELECT count(*) FROM kv", [], |r| r.get::<_, i64>(0))?;
    Ok(conn)
}

/// Brings a v0 (empty), v1 or v2 database to `SCHEMA_VERSION` in one write
/// transaction. The version is read again under the lock, so a second
/// process that raced here finds the work done and changes nothing.
fn migrate(conn: &Connection) -> Result<(), CacheError> {
    conn.execute_batch("BEGIN IMMEDIATE")?;
    let result = (|| -> Result<(), CacheError> {
        let version: i32 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        match version {
            0 => {
                conn.execute_batch(SCHEMA_V1)?;
                conn.execute_batch(SCHEMA_V2)?;
                conn.execute_batch(SCHEMA_V3)?;
            }
            1 => {
                conn.execute_batch(SCHEMA_V2)?;
                conn.execute_batch(SCHEMA_V3)?;
            }
            2 => conn.execute_batch(SCHEMA_V3)?,
            SCHEMA_VERSION => return Ok(()),
            _ => return Err(CacheError::Sql(rusqlite::Error::InvalidQuery)),
        }
        conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        Ok(())
    })();
    match result {
        Ok(()) => conn.execute_batch("COMMIT")?,
        Err(err) => {
            let _ = conn.execute_batch("ROLLBACK");
            return Err(err);
        }
    }
    Ok(())
}

/// A file modification time as nanoseconds since the Unix epoch (0 before).
pub fn mtime_ns(t: SystemTime) -> i64 {
    t.duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_nanos()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn busy_and_locked_are_not_unusable() {
        let failure = |code| {
            CacheError::Sql(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error {
                    code,
                    extended_code: 0,
                },
                None,
            ))
        };
        assert!(!is_unusable_database(&failure(ErrorCode::DatabaseBusy)));
        assert!(!is_unusable_database(&failure(ErrorCode::DatabaseLocked)));
        assert!(!is_unusable_database(&failure(ErrorCode::DiskFull)));
        assert!(is_unusable_database(&failure(ErrorCode::NotADatabase)));
        assert!(is_unusable_database(&failure(ErrorCode::DatabaseCorrupt)));
        assert!(is_unusable_database(&CacheError::Sql(
            rusqlite::Error::InvalidQuery
        )));
    }

    #[test]
    fn mtime_ns_of_the_epoch_and_before_is_zero() {
        assert_eq!(mtime_ns(UNIX_EPOCH), 0);
        assert_eq!(mtime_ns(UNIX_EPOCH - Duration::from_secs(1)), 0);
        // Windows keeps file times in 100 ns ticks, so stay on a whole tick.
        assert_eq!(mtime_ns(UNIX_EPOCH + Duration::from_nanos(700)), 700);
    }
}
