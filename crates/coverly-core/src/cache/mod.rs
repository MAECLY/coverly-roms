//! Local SQLite cache (spec §6 `cache/`, §16.1): quick-id memo
//! (`path + size + mtime → hashes`), source indexes and gathered candidates
//! as JSON values with ETag and fetch time, DAT rows, downloaded art files.
//! It is only a cache: a corrupt or newer database is set aside and rebuilt.
//! A database that is merely busy, locked or unreachable is left alone.

use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, ErrorCode, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};

use crate::identify::hash::HashOutcome;
pub use crate::time::now_unix;

/// Schema version stored in `PRAGMA user_version`.
pub const SCHEMA_VERSION: i32 = 1;
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
        // An outcome that no longer deserialises (older format) is a miss.
        Ok(row.and_then(|json| serde_json::from_str(&json).ok()))
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
                serde_json::to_string(outcome)?
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

    /// Replaces every row of `dat` with `rows`, in one transaction.
    pub fn replace_dat(&self, dat: &str, rows: &[DatRow]) -> Result<(), CacheError> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM dat_roms WHERE dat = ?1", params![dat])?;
        {
            let mut insert = tx.prepare(
                "INSERT INTO dat_roms (dat, game, rom_name, rom_stem, size, crc32, md5, sha1) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )?;
            for r in rows {
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
                    r.sha1
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
        };
        let mut stmt = conn.prepare(&format!(
            "SELECT game, rom_name, size, crc32, md5, sha1 FROM dat_roms \
             WHERE dat = ?1 AND {column} = ?2 ORDER BY game, rom_name"
        ))?;
        let rows = stmt.query_map(params![dat, value], |r| {
            Ok(DatRow {
                game: r.get(0)?,
                rom_name: r.get(1)?,
                size: r.get::<_, Option<i64>>(2)?.map(|s| s as u64),
                crc32: r.get(3)?,
                md5: r.get(4)?,
                sha1: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
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

#[derive(Clone, Copy)]
enum DatFilter {
    Crc,
    Stem,
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
        0 => {
            conn.execute_batch(SCHEMA_V1)?;
            conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        }
        SCHEMA_VERSION => {}
        _ => return Err(CacheError::Sql(rusqlite::Error::InvalidQuery)),
    }
    // Touch a table so a garbage file fails here, not on first use.
    conn.query_row("SELECT count(*) FROM kv", [], |r| r.get::<_, i64>(0))?;
    Ok(conn)
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
        assert_eq!(mtime_ns(UNIX_EPOCH + Duration::from_nanos(7)), 7);
    }
}
