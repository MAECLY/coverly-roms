//! The optional Switch name catalogue (adjustments D2–D6, D10).
//!
//! `blawar/titledb` (MIT) is downloaded on demand, only while
//! `catalogs.switch.enabled` is on, never bundled. One region file is kept
//! (`ES.es.json` for a Spanish UI, else `US.en.json`, with `US.en.json` as
//! the fallback), reduced while streaming to id → name, publisher, release
//! date and region in its own `catalog.sqlite`; no raw copy ever touches
//! the disk. `ncas.json` (NCA id → title id) is fetched only through
//! [`SwitchCatalog::ensure_ncas`], when a walk saw NCA ids without a title
//! id. A refresh happens at most every `refresh_days`, as a conditional GET
//! with `If-None-Match` and `If-Modified-Since`.
//!
//! Keys (D1): no table stores a key, rights id or anything beyond the
//! whitelist, an entry with a non-null `key` abandons the import, and no
//! error or status echoes catalogue content.
//!
//! Identification never calls into the network: callers run
//! [`SwitchCatalog::ensure`] before a plan or scan (plan P-2) and hand the
//! catalogue to identification as a [`TitleNames`] (plan P-1).

mod ncas;
mod store;
mod titledb;

use std::cell::Cell;
use std::fmt;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::Serialize;

use crate::consoles::TitleSource;
use crate::net::hosts::TITLEDB_BASE;
use crate::net::{Http, HttpRequest, HttpResponse, NetError};
use crate::settings::SwitchCatalogSettings;
use store::{Meta, MetaState, Table};

/// The catalogue's database, beside the cache in the data folder.
pub const CATALOG_FILE: &str = "catalog.sqlite";

/// Most decoded bytes read from one catalogue file (D5; a region file is
/// about 90 MB, `ncas.json` 88 MB).
pub const MAX_CATALOG_BYTES: u64 = 160 * 1024 * 1024;

const US_FILE: &str = "US.en.json";
const ES_FILE: &str = "ES.es.json";
const NCAS_FILE: &str = "ncas.json";

/// The shortest refresh interval, whatever the settings say (D4).
const MIN_REFRESH_SECS: i64 = 7 * 24 * 60 * 60;

/// How often the progress callback runs while reading, in bytes.
const PROGRESS_STEP: u64 = 1024 * 1024;

/// A title name from the catalogue (or, later, its fallback).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogTitle {
    pub name: String,
    pub publisher: Option<String>,
    /// ISO date `YYYY-MM-DD`.
    pub release: Option<String>,
    pub region: Option<String>,
    pub source: TitleSource,
}

/// Synchronous, local lookups identification may make (plan P-1). `None`
/// in `OrganizeEnv` means no catalogue (tests, `--offline`, setting off).
pub trait TitleNames: Send + Sync {
    fn title(&self, tid: u64) -> Option<CatalogTitle>;
    /// Title id and content type for an NCA id (spec D6).
    fn by_nca(&self, nca: &[u8; 16]) -> Option<(u64, u8)>;
    fn icon_url(&self, tid: u64) -> Option<String> {
        let _ = tid;
        None
    }
}

/// What the Sources group shows about the catalogue (spec §7, §10).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum CatalogStatus {
    Off,
    /// On, but nothing downloaded yet (or a first download was cut short).
    Pending,
    Downloading {
        done: u64,
        total: Option<u64>,
    },
    /// `updated` is the ISO date the rows last changed; `fallback` says
    /// they come from the United States list instead of the UI language's.
    Ready {
        count: u64,
        updated: String,
        fallback: bool,
    },
    FailedOld {
        updated: String,
    },
    FailedNone,
    /// An entry carried data Coverly does not accept (a non-null `key`).
    Rejected,
    RemoveFailed,
}

/// Why the catalogue database could not be opened or removed. Messages
/// never carry catalogue content (D1).
#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    #[error("catalogue database: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("catalogue file: {0}")]
    Io(#[from] std::io::Error),
}

/// The Switch catalogue over its own `catalog.sqlite`.
pub struct SwitchCatalog {
    path: PathBuf,
    /// Lookups only (`query_only`); imports open their own connection, so
    /// reads see the old rows until the swap commits.
    reader: Mutex<Connection>,
    /// One import at a time in this process.
    ensure_lock: Mutex<()>,
    /// Bytes read by the running download, for [`status`](Self::status).
    progress: Mutex<Option<(u64, Option<u64>)>>,
    /// Last import failure (byte offset and field name only).
    last_error: Mutex<Option<String>>,
    max_bytes: u64,
}

impl SwitchCatalog {
    /// Opens (creating) `catalog.sqlite` in `data_dir`. A garbage file or
    /// one written by a newer build is deleted and created again: it is a
    /// cache of a public download.
    pub fn open(data_dir: &Path) -> Result<SwitchCatalog, CatalogError> {
        std::fs::create_dir_all(data_dir)?;
        let path = data_dir.join(CATALOG_FILE);
        let conn = match store::open(&path) {
            Ok(conn) => conn,
            Err(err) if store::is_unusable(&err) => {
                delete_files(&path)?;
                store::open(&path)?
            }
            Err(err) => return Err(err.into()),
        };
        conn.pragma_update(None, "query_only", true)?;
        Ok(SwitchCatalog {
            path,
            reader: Mutex::new(conn),
            ensure_lock: Mutex::new(()),
            progress: Mutex::new(None),
            last_error: Mutex::new(None),
            max_bytes: MAX_CATALOG_BYTES,
        })
    }

    /// Caps the decoded bytes read from one file (tests; default
    /// [`MAX_CATALOG_BYTES`]).
    pub fn with_max_bytes(mut self, n: u64) -> SwitchCatalog {
        self.max_bytes = n;
        self
    }

    /// Downloads or refreshes the region file when due (D2–D4, D10): never
    /// while the setting is off; no request while the rows are younger than
    /// `refresh_days`; a 304 only moves the check time; a failure keeps the
    /// old rows. `deadline` caps a first download inside a plan: past it,
    /// or on `cancel`, nothing is kept and the result is `Pending` (a
    /// refresh cut short reports the rows it still has).
    #[allow(clippy::too_many_arguments)]
    pub fn ensure(
        &self,
        http: &dyn Http,
        s: &SwitchCatalogSettings,
        locale: &str,
        now: SystemTime,
        cancel: &AtomicBool,
        deadline: Option<Instant>,
        progress: &dyn Fn(u64, Option<u64>),
    ) -> CatalogStatus {
        if !s.enabled {
            return CatalogStatus::Off;
        }
        let _one = lock(&self.ensure_lock);
        let Ok(prev) = self.meta(Table::Titledb) else {
            return CatalogStatus::FailedNone;
        };
        let now_s = unix_secs(now);
        let keep_icons = s.icons_on();
        // `iconUrl` was not kept before: one unconditional fetch (D4, D8).
        let icons_added = keep_icons && !prev.icons && prev.state == MetaState::Ok;
        if !is_due(&prev, s, now_s) && !icons_added {
            return status_of(&prev);
        }
        let desired = region_file(locale);
        let files: &[&str] = if desired == US_FILE {
            &[US_FILE]
        } else {
            &[desired, US_FILE]
        };
        let job = Job {
            table: Table::Titledb,
            keep_icons,
            fresh: icons_added,
            now_s,
            cancel,
            deadline,
            progress,
        };
        self.run_with(http, &job, files, &prev)
    }

    /// Downloads or refreshes `ncas.json` when due (D6), with the same
    /// rules as [`ensure`](Self::ensure). Called only when a walk saw NCA
    /// ids without a title id.
    pub fn ensure_ncas(
        &self,
        http: &dyn Http,
        s: &SwitchCatalogSettings,
        now: SystemTime,
        cancel: &AtomicBool,
        deadline: Option<Instant>,
        progress: &dyn Fn(u64, Option<u64>),
    ) -> CatalogStatus {
        if !s.enabled {
            return CatalogStatus::Off;
        }
        let _one = lock(&self.ensure_lock);
        let Ok(prev) = self.meta(Table::Ncas) else {
            return CatalogStatus::FailedNone;
        };
        let now_s = unix_secs(now);
        if !is_due(&prev, s, now_s) {
            return status_of(&prev);
        }
        let job = Job {
            table: Table::Ncas,
            keep_icons: false,
            fresh: false,
            now_s,
            cancel,
            deadline,
            progress,
        };
        self.run_with(http, &job, &[NCAS_FILE], &prev)
    }

    /// The titles catalogue's state, without any request.
    pub fn status(&self, s: &SwitchCatalogSettings) -> CatalogStatus {
        if !s.enabled {
            return CatalogStatus::Off;
        }
        if let Some((done, total)) = *lock(&self.progress) {
            return CatalogStatus::Downloading { done, total };
        }
        match self.meta(Table::Titledb) {
            Ok(meta) => status_of(&meta),
            Err(_) => CatalogStatus::FailedNone,
        }
    }

    /// The last import failure, as a byte offset and field name only.
    pub fn last_error(&self) -> Option<String> {
        lock(&self.last_error).clone()
    }

    /// Deletes `catalog.sqlite` and its WAL files (the read handle is
    /// dropped first, so this works on Windows while this process is the
    /// only user). Another process holding the file makes it fail: the
    /// caller shows `RemoveFailed`.
    pub fn remove(self, data_dir: &Path) -> Result<(), CatalogError> {
        drop(self);
        delete_files(&data_dir.join(CATALOG_FILE))?;
        Ok(())
    }

    fn meta(&self, table: Table) -> rusqlite::Result<Meta> {
        store::read_meta(&lock(&self.reader), table)
    }

    /// Tries `files` in order (the locale's, then the US fallback) and
    /// records the outcome in the meta row.
    fn run_with(&self, http: &dyn Http, job: &Job, files: &[&str], prev: &Meta) -> CatalogStatus {
        for (i, file) in files.iter().enumerate() {
            let last = i + 1 == files.len();
            let fallback = i > 0;
            // A failed refresh keeps its validators: the retry is still a
            // conditional GET for the rows on disk (D4).
            let conditional = !job.fresh
                && matches!(prev.state, MetaState::Ok | MetaState::Failed)
                && prev.rows > 0
                && prev.source_file.as_deref() == Some(*file);
            let outcome = self.fetch(http, job, file, prev, conditional, fallback);
            let mut meta = prev.clone();
            match outcome {
                Outcome::Imported { rows } => {
                    return CatalogStatus::Ready {
                        count: rows,
                        updated: iso_day(job.now_s),
                        fallback,
                    }
                }
                Outcome::NotModified => {
                    meta.state = MetaState::Ok;
                    meta.checked_at = job.now_s;
                    meta.fallback = fallback;
                    if self.write_meta(job.table, &meta).is_err() {
                        return failed(prev);
                    }
                    return status_of(&meta);
                }
                Outcome::NotFound | Outcome::Unreadable if !last => continue,
                Outcome::Rejected => {
                    meta.state = MetaState::Rejected;
                    meta.checked_at = job.now_s;
                    let _ = self.write_meta(job.table, &meta);
                    return CatalogStatus::Rejected;
                }
                // Only a first download is pending; a refresh cut short
                // still has its rows.
                Outcome::Pending if prev.rows > 0 => return status_of(prev),
                Outcome::Pending => return CatalogStatus::Pending,
                Outcome::NotFound | Outcome::Unreadable | Outcome::Failed => {
                    meta.state = MetaState::Failed;
                    let _ = self.write_meta(job.table, &meta);
                    return failed(prev);
                }
            }
        }
        failed(prev)
    }

    /// One conditional or full GET of `file`, reduced while streaming into
    /// a staging table that replaces the live one only on success.
    fn fetch(
        &self,
        http: &dyn Http,
        job: &Job,
        file: &str,
        prev: &Meta,
        conditional: bool,
        fallback: bool,
    ) -> Outcome {
        let mut req = HttpRequest::get(format!("{TITLEDB_BASE}/{file}")).max_bytes(self.max_bytes);
        if conditional {
            req = req
                .if_none_match(prev.etag.clone())
                .if_modified_since(prev.last_modified.clone());
        }
        let Ok(mut conn) = store::open(&self.path) else {
            return Outcome::Failed;
        };
        let Ok(tx) = conn.transaction_with_behavior(TransactionBehavior::Immediate) else {
            return Outcome::Failed;
        };
        if tx.execute_batch(store::staging_ddl(job.table)).is_err() {
            return Outcome::Failed;
        }
        let ctl = Control::new(job.cancel, job.deadline);
        let mut reduced: Option<Result<u64, Fail>> = None;
        let answer = {
            let sql = match job.table {
                Table::Titledb => {
                    "INSERT OR IGNORE INTO titledb_staging (id, name, publisher, release, region, icon_url)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)"
                }
                Table::Ncas => {
                    "INSERT OR IGNORE INTO ncas_staging (nca, title, content_type) VALUES (?1, ?2, ?3)"
                }
            };
            let Ok(mut insert) = tx.prepare(sql) else {
                return Outcome::Failed;
            };
            let report = |done: u64| self.report(done, job.progress);
            let mut sink = |_: &HttpResponse, body: &mut dyn Read| -> Result<(), NetError> {
                let mut rows = 0u64;
                let mut capped = Capped::new(body, &ctl, self.max_bytes, &report);
                let mut add = |changed: rusqlite::Result<usize>| -> Result<(), Stop> {
                    rows += changed.map_err(|_| Stop::Store)? as u64;
                    Ok(())
                };
                let result = match job.table {
                    Table::Titledb => {
                        titledb::reduce(&mut capped, job.keep_icons, &ctl, &mut |r| {
                            add(insert.execute(params![
                                store::tid_to_sql(r.id),
                                r.name,
                                r.publisher,
                                r.release,
                                r.region,
                                r.icon_url
                            ]))
                        })
                    }
                    Table::Ncas => ncas::reduce(&mut capped, &ctl, &mut |r| {
                        add(insert.execute(params![
                            &r.nca[..],
                            store::tid_to_sql(r.title),
                            r.content_type
                        ]))
                    }),
                };
                report(capped.read);
                reduced = Some(result.map(|()| rows));
                Ok(())
            };
            http.get_stream(&req, &mut sink)
        };
        *lock(&self.progress) = None;
        let resp = match answer {
            Ok(resp) => resp,
            Err(_) => return Outcome::Failed,
        };
        match resp.status {
            304 if conditional => return Outcome::NotModified,
            404 | 410 => return Outcome::NotFound,
            200..=299 => {}
            _ => return Outcome::Failed,
        }
        let rows = match reduced {
            Some(Ok(rows)) if rows > 0 => rows,
            // An empty catalogue never replaces the rows.
            Some(Ok(_)) => return Outcome::Unreadable,
            Some(Err(fail)) => {
                *lock(&self.last_error) = Some(fail.to_string());
                return match fail {
                    Fail::Stopped(Stop::Rejected) => Outcome::Rejected,
                    Fail::Stopped(Stop::Cancelled | Stop::Deadline) => Outcome::Pending,
                    Fail::Stopped(Stop::TooLarge | Stop::Store) | Fail::Io { .. } => {
                        Outcome::Failed
                    }
                    Fail::Parse { .. } => Outcome::Unreadable,
                };
            }
            None => return Outcome::Failed,
        };
        let meta = Meta {
            etag: resp.etag,
            last_modified: resp.last_modified,
            checked_at: job.now_s,
            updated_at: Some(job.now_s),
            rows,
            source_file: Some(file.to_owned()),
            fallback,
            icons: job.keep_icons,
            state: MetaState::Ok,
        };
        let committed = tx
            .execute_batch(store::swap_ddl(job.table))
            .and_then(|()| store::write_meta(&tx, job.table, &meta))
            .and_then(|()| tx.commit());
        match committed {
            Ok(()) => {
                *lock(&self.last_error) = None;
                Outcome::Imported { rows }
            }
            Err(_) => Outcome::Failed,
        }
    }

    fn write_meta(&self, table: Table, meta: &Meta) -> rusqlite::Result<()> {
        let conn = store::open(&self.path)?;
        store::write_meta(&conn, table, meta)
    }

    fn report(&self, done: u64, progress: &dyn Fn(u64, Option<u64>)) {
        *lock(&self.progress) = Some((done, None));
        progress(done, None);
    }
}

impl TitleNames for SwitchCatalog {
    fn title(&self, tid: u64) -> Option<CatalogTitle> {
        let conn = lock(&self.reader);
        let mut stmt = conn
            .prepare_cached("SELECT name, publisher, release, region FROM titledb WHERE id = ?1")
            .ok()?;
        stmt.query_row(params![store::tid_to_sql(tid)], |r| {
            Ok(CatalogTitle {
                name: r.get(0)?,
                publisher: r.get(1)?,
                release: r.get(2)?,
                region: r.get(3)?,
                source: TitleSource::Titledb,
            })
        })
        .optional()
        .ok()
        .flatten()
    }

    fn by_nca(&self, nca: &[u8; 16]) -> Option<(u64, u8)> {
        let conn = lock(&self.reader);
        let mut stmt = conn
            .prepare_cached("SELECT title, content_type FROM ncas WHERE nca = ?1")
            .ok()?;
        stmt.query_row(params![&nca[..]], |r| {
            Ok((store::tid_from_sql(r.get(0)?), r.get::<_, u8>(1)?))
        })
        .optional()
        .ok()
        .flatten()
    }

    fn icon_url(&self, tid: u64) -> Option<String> {
        let conn = lock(&self.reader);
        let mut stmt = conn
            .prepare_cached("SELECT icon_url FROM titledb WHERE id = ?1")
            .ok()?;
        stmt.query_row(params![store::tid_to_sql(tid)], |r| {
            r.get::<_, Option<String>>(0)
        })
        .optional()
        .ok()
        .flatten()
        .flatten()
    }
}

/// One download: which table, whether icons are kept, whether the fetch
/// must be unconditional, the clock and the stop signals.
struct Job<'a> {
    table: Table,
    keep_icons: bool,
    fresh: bool,
    now_s: i64,
    cancel: &'a AtomicBool,
    deadline: Option<Instant>,
    progress: &'a dyn Fn(u64, Option<u64>),
}

/// How one GET of one file ended.
enum Outcome {
    Imported {
        rows: u64,
    },
    NotModified,
    NotFound,
    /// Not the expected JSON (an HTML outage page, a truncated or empty
    /// file): the next file in line is tried.
    Unreadable,
    Rejected,
    Pending,
    Failed,
}

/// Why a reducer stopped before the end of the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stop {
    /// A non-null `key` (D5, Q11).
    Rejected,
    Cancelled,
    Deadline,
    /// Over [`SwitchCatalog::with_max_bytes`].
    TooLarge,
    /// The staging insert failed.
    Store,
}

/// A failed reduction. Only a byte offset and a whitelisted field name are
/// kept, never catalogue text (D1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Fail {
    Stopped(Stop),
    Io {
        offset: u64,
    },
    Parse {
        offset: u64,
        field: Option<&'static str>,
    },
}

impl fmt::Display for Fail {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Fail::Stopped(Stop::Rejected) => {
                f.write_str("catalogue rejected: an entry carries a key")
            }
            Fail::Stopped(Stop::Cancelled) => f.write_str("catalogue download cancelled"),
            Fail::Stopped(Stop::Deadline) => f.write_str("catalogue download postponed"),
            Fail::Stopped(Stop::TooLarge) => f.write_str("catalogue file over the size cap"),
            Fail::Stopped(Stop::Store) => f.write_str("catalogue rows could not be stored"),
            Fail::Io { offset } => write!(f, "catalogue read failed at byte {offset}"),
            Fail::Parse {
                offset,
                field: Some(field),
            } => {
                write!(
                    f,
                    "catalogue parse error near byte {offset} in field {field}"
                )
            }
            Fail::Parse {
                offset,
                field: None,
            } => write!(f, "catalogue parse error near byte {offset}"),
        }
    }
}

/// Shared state of one reduction: the stop signals, why it stopped, the
/// field being read and the bytes read so far.
pub(crate) struct Control<'a> {
    cancel: &'a AtomicBool,
    deadline: Option<Instant>,
    stopped: Cell<Option<Stop>>,
    field: Cell<Option<&'static str>>,
    bytes: Cell<u64>,
    entries: Cell<u64>,
}

impl<'a> Control<'a> {
    fn new(cancel: &'a AtomicBool, deadline: Option<Instant>) -> Control<'a> {
        Control {
            cancel,
            deadline,
            stopped: Cell::new(None),
            field: Cell::new(None),
            bytes: Cell::new(0),
            entries: Cell::new(0),
        }
    }

    /// Before each entry: stops on cancel, and (every 256 entries, the
    /// first included) once the deadline has passed.
    pub(crate) fn check<E: serde::de::Error>(&self) -> Result<(), E> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err(self.stop(Stop::Cancelled));
        }
        let n = self.entries.get();
        self.entries.set(n.wrapping_add(1));
        if n.is_multiple_of(256) && self.deadline.is_some_and(|d| Instant::now() >= d) {
            return Err(self.stop(Stop::Deadline));
        }
        Ok(())
    }

    /// Records the first stop reason and returns a content-free error.
    pub(crate) fn stop<E: serde::de::Error>(&self, why: Stop) -> E {
        if self.stopped.get().is_none() {
            self.stopped.set(Some(why));
        }
        E::custom("stopped")
    }

    pub(crate) fn finish(&self, parsed: Result<(), serde_json::Error>) -> Result<(), Fail> {
        if let Some(why) = self.stopped.get() {
            return Err(Fail::Stopped(why));
        }
        parsed.map_err(|e| {
            if e.is_io() {
                Fail::Io {
                    offset: self.bytes.get(),
                }
            } else {
                Fail::Parse {
                    offset: self.bytes.get(),
                    field: self.field.get(),
                }
            }
        })
    }
}

/// The body reader: counts bytes, enforces the cap (whatever the transport
/// did) and reports progress.
struct Capped<'a, 'c> {
    inner: &'a mut dyn Read,
    ctl: &'a Control<'c>,
    max: u64,
    report: &'a dyn Fn(u64),
    read: u64,
    reported: u64,
}

impl<'a, 'c> Capped<'a, 'c> {
    fn new(
        inner: &'a mut dyn Read,
        ctl: &'a Control<'c>,
        max: u64,
        report: &'a dyn Fn(u64),
    ) -> Self {
        Capped {
            inner,
            ctl,
            max,
            report,
            read: 0,
            reported: 0,
        }
    }
}

impl Read for Capped<'_, '_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        let total = self.read.saturating_add(n as u64);
        if total > self.max {
            if self.ctl.stopped.get().is_none() {
                self.ctl.stopped.set(Some(Stop::TooLarge));
            }
            return Err(std::io::Error::other("over the size cap"));
        }
        self.read = total;
        self.ctl.bytes.set(total);
        if total - self.reported >= PROGRESS_STEP {
            self.reported = total;
            (self.report)(total);
        }
        Ok(n)
    }
}

/// Whether a refresh is due: never downloaded, the last attempt failed,
/// or `refresh_days` (at least 7) have passed. A clock that went back
/// counts as due.
fn is_due(meta: &Meta, s: &SwitchCatalogSettings, now_s: i64) -> bool {
    match meta.state {
        MetaState::None | MetaState::Failed => true,
        MetaState::Ok | MetaState::Rejected => {
            now_s < meta.checked_at
                || now_s - meta.checked_at >= s.refresh_secs().max(MIN_REFRESH_SECS)
        }
    }
}

fn status_of(meta: &Meta) -> CatalogStatus {
    match meta.state {
        MetaState::None => CatalogStatus::Pending,
        MetaState::Ok => CatalogStatus::Ready {
            count: meta.rows,
            updated: iso_day(meta.updated_at.unwrap_or(meta.checked_at)),
            fallback: meta.fallback,
        },
        MetaState::Failed => failed(meta),
        MetaState::Rejected => CatalogStatus::Rejected,
    }
}

/// A failure keeps the previous rows, when there are any.
fn failed(prev: &Meta) -> CatalogStatus {
    match prev.updated_at {
        Some(at) if prev.rows > 0 => CatalogStatus::FailedOld {
            updated: iso_day(at),
        },
        _ => CatalogStatus::FailedNone,
    }
}

/// The region file for a UI locale (D3, §13 Q17): Spanish reads the
/// Spanish list, everything else the United States one.
fn region_file(locale: &str) -> &'static str {
    let lang = locale.split(['-', '_']).next().unwrap_or("");
    if lang.eq_ignore_ascii_case("es") {
        ES_FILE
    } else {
        US_FILE
    }
}

fn unix_secs(t: SystemTime) -> i64 {
    t.duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// `YYYY-MM-DD` (UTC) of Unix seconds.
fn iso_day(secs: i64) -> String {
    let t = UNIX_EPOCH + Duration::from_secs(u64::try_from(secs).unwrap_or(0));
    crate::time::rfc3339_utc(t)[..10].to_owned()
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Deletes the database and its journal files; a missing file is fine.
fn delete_files(path: &Path) -> std::io::Result<()> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| CATALOG_FILE.to_owned());
    for suffix in ["", "-wal", "-shm", "-journal"] {
        match std::fs::remove_file(path.with_file_name(format!("{name}{suffix}"))) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn region_file_follows_the_ui_language() {
        assert_eq!(region_file("es"), ES_FILE);
        assert_eq!(region_file("es-ES"), ES_FILE);
        assert_eq!(region_file("ES_mx"), ES_FILE);
        assert_eq!(region_file("en"), US_FILE);
        assert_eq!(region_file("fr"), US_FILE);
        assert_eq!(region_file(""), US_FILE);
    }

    #[test]
    fn days() {
        assert_eq!(iso_day(1_793_491_200), "2026-11-01");
        assert_eq!(iso_day(-5), "1970-01-01");
    }

    #[test]
    fn failure_messages_hold_no_catalogue_text() {
        let f = Fail::Parse {
            offset: 12,
            field: Some("name"),
        };
        assert_eq!(
            f.to_string(),
            "catalogue parse error near byte 12 in field name"
        );
    }
}
