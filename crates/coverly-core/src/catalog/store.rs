//! `catalog.sqlite`: the catalogue's own database (adjustments D5). It is
//! never the shared cache, so the cache's `SCHEMA_VERSION` stays as it is
//! (a rolled-back app keeps its cache) and a minutes-long import never
//! holds the write lock that scans and the CLI share.
//!
//! No table has a column for a key, a rights id, a description or any
//! other field the reducers drop (D1, D5).

use std::path::Path;
use std::time::Duration;

use rusqlite::{params, Connection, ErrorCode, OptionalExtension};

/// Schema version in this file's own `PRAGMA user_version`.
pub const STORE_VERSION: i32 = 1;

const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

const SCHEMA_V1: &str = "
CREATE TABLE IF NOT EXISTS catalog_meta (
    name TEXT PRIMARY KEY,
    etag TEXT,
    last_modified TEXT,
    checked_at INTEGER NOT NULL DEFAULT 0,
    updated_at INTEGER,
    rows INTEGER NOT NULL DEFAULT 0,
    source_file TEXT,
    fallback INTEGER NOT NULL DEFAULT 0,
    icons INTEGER NOT NULL DEFAULT 0,
    state TEXT NOT NULL DEFAULT 'none'
);
CREATE TABLE IF NOT EXISTS titledb (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    publisher TEXT,
    release TEXT,
    region TEXT,
    icon_url TEXT
);
CREATE TABLE IF NOT EXISTS ncas (
    nca BLOB PRIMARY KEY,
    title INTEGER NOT NULL,
    content_type INTEGER NOT NULL
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS nlib (
    tid INTEGER PRIMARY KEY,
    name TEXT,
    publisher TEXT,
    release TEXT,
    fetched_at INTEGER NOT NULL,
    miss INTEGER NOT NULL DEFAULT 0
);
";

/// The staging twin of a table, filled during an import and renamed over
/// the live one on success (same transaction).
pub(crate) fn staging_ddl(table: Table) -> &'static str {
    match table {
        Table::Titledb => {
            "DROP TABLE IF EXISTS titledb_staging;
             CREATE TABLE titledb_staging (
                 id INTEGER PRIMARY KEY,
                 name TEXT NOT NULL,
                 publisher TEXT,
                 release TEXT,
                 region TEXT,
                 icon_url TEXT
             );"
        }
        Table::Ncas => {
            "DROP TABLE IF EXISTS ncas_staging;
             CREATE TABLE ncas_staging (
                 nca BLOB PRIMARY KEY,
                 title INTEGER NOT NULL,
                 content_type INTEGER NOT NULL
             ) WITHOUT ROWID;"
        }
    }
}

/// Replaces the live table by its staging twin.
pub(crate) fn swap_ddl(table: Table) -> &'static str {
    match table {
        Table::Titledb => "DROP TABLE titledb; ALTER TABLE titledb_staging RENAME TO titledb;",
        Table::Ncas => "DROP TABLE ncas; ALTER TABLE ncas_staging RENAME TO ncas;",
    }
}

/// A downloaded dataset with its own table and meta row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Table {
    Titledb,
    Ncas,
}

impl Table {
    pub(crate) fn meta_name(self) -> &'static str {
        match self {
            Table::Titledb => "titledb",
            Table::Ncas => "ncas",
        }
    }
}

/// How the last import of a dataset ended.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum MetaState {
    /// Never downloaded.
    #[default]
    None,
    Ok,
    Failed,
    Rejected,
}

impl MetaState {
    fn as_str(self) -> &'static str {
        match self {
            MetaState::None => "none",
            MetaState::Ok => "ok",
            MetaState::Failed => "failed",
            MetaState::Rejected => "rejected",
        }
    }

    fn parse(s: &str) -> MetaState {
        match s {
            "ok" => MetaState::Ok,
            "failed" => MetaState::Failed,
            "rejected" => MetaState::Rejected,
            _ => MetaState::None,
        }
    }
}

/// One `catalog_meta` row: validators, times (Unix seconds) and outcome.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Meta {
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    /// Last request answered 200 or 304 (or a rejection): the refresh clock.
    pub checked_at: i64,
    /// Last time the rows changed.
    pub updated_at: Option<i64>,
    pub rows: u64,
    pub source_file: Option<String>,
    /// The rows come from `US.en.json` although another file was asked.
    pub fallback: bool,
    /// The rows kept `iconUrl` (icons were on at import).
    pub icons: bool,
    pub state: MetaState,
}

/// Opens (creating) the file at `path` and brings it to [`STORE_VERSION`].
pub(crate) fn open(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.busy_timeout(BUSY_TIMEOUT)?;
    // WAL: lookups keep reading the old rows while an import writes.
    conn.pragma_update(None, "journal_mode", "WAL")?;
    let version: i32 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    match version {
        STORE_VERSION => {}
        0 => {
            conn.execute_batch("BEGIN IMMEDIATE")?;
            let done = conn
                .execute_batch(SCHEMA_V1)
                .and_then(|()| conn.pragma_update(None, "user_version", STORE_VERSION));
            match done {
                Ok(()) => conn.execute_batch("COMMIT")?,
                Err(err) => {
                    let _ = conn.execute_batch("ROLLBACK");
                    return Err(err);
                }
            }
        }
        // A newer build's file: it cannot be read here.
        _ => return Err(rusqlite::Error::InvalidQuery),
    }
    // Touch a table so a garbage file fails here, not on first use.
    conn.query_row("SELECT count(*) FROM catalog_meta", [], |r| {
        r.get::<_, i64>(0)
    })?;
    Ok(conn)
}

/// Whether opening failed because of the file itself (garbage, newer
/// schema) rather than the environment (busy, permissions, disk full). The
/// catalogue is a cache, so such a file is deleted and created again.
pub(crate) fn is_unusable(err: &rusqlite::Error) -> bool {
    match err {
        rusqlite::Error::SqliteFailure(e, _) => !matches!(
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
        _ => true,
    }
}

pub(crate) fn read_meta(conn: &Connection, table: Table) -> rusqlite::Result<Meta> {
    conn.query_row(
        "SELECT etag, last_modified, checked_at, updated_at, rows, source_file, fallback, icons, state
         FROM catalog_meta WHERE name = ?1",
        params![table.meta_name()],
        |r| {
            Ok(Meta {
                etag: r.get(0)?,
                last_modified: r.get(1)?,
                checked_at: r.get(2)?,
                updated_at: r.get(3)?,
                rows: u64::try_from(r.get::<_, i64>(4)?).unwrap_or(0),
                source_file: r.get(5)?,
                fallback: r.get::<_, i64>(6)? != 0,
                icons: r.get::<_, i64>(7)? != 0,
                state: MetaState::parse(&r.get::<_, String>(8)?),
            })
        },
    )
    .optional()
    .map(Option::unwrap_or_default)
}

pub(crate) fn write_meta(conn: &Connection, table: Table, m: &Meta) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO catalog_meta
             (name, etag, last_modified, checked_at, updated_at, rows, source_file, fallback, icons, state)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            table.meta_name(),
            m.etag,
            m.last_modified,
            m.checked_at,
            m.updated_at,
            i64::try_from(m.rows).unwrap_or(i64::MAX),
            m.source_file,
            i64::from(m.fallback),
            i64::from(m.icons),
            m.state.as_str(),
        ],
    )?;
    Ok(())
}

/// A title id as stored: the `u64` bit pattern in SQLite's `i64`.
pub(crate) fn tid_to_sql(tid: u64) -> i64 {
    i64::from_ne_bytes(tid.to_ne_bytes())
}

pub(crate) fn tid_from_sql(v: i64) -> u64 {
    u64::from_ne_bytes(v.to_ne_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_ids_round_trip_through_i64() {
        for tid in [0u64, 0x0100_0000_0000_0000, u64::MAX, 0x8000_0000_0000_0000] {
            assert_eq!(tid_from_sql(tid_to_sql(tid)), tid);
        }
    }

    #[test]
    fn meta_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let conn = open(&dir.path().join("c.sqlite")).unwrap();
        assert_eq!(read_meta(&conn, Table::Titledb).unwrap(), Meta::default());
        let m = Meta {
            etag: Some("\"e\"".into()),
            last_modified: Some("Mon, 14 Sep 2026 05:33:00 GMT".into()),
            checked_at: 10,
            updated_at: Some(9),
            rows: 3,
            source_file: Some("US.en.json".into()),
            fallback: true,
            icons: true,
            state: MetaState::Rejected,
        };
        write_meta(&conn, Table::Titledb, &m).unwrap();
        assert_eq!(read_meta(&conn, Table::Titledb).unwrap(), m);
        assert_eq!(read_meta(&conn, Table::Ncas).unwrap(), Meta::default());
    }

    #[test]
    fn a_newer_file_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("c.sqlite");
        drop(open(&path).unwrap());
        let conn = Connection::open(&path).unwrap();
        conn.pragma_update(None, "user_version", STORE_VERSION + 1)
            .unwrap();
        drop(conn);
        let err = open(&path).unwrap_err();
        assert!(is_unusable(&err));
    }
}
