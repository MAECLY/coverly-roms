//! Writes into the library (spec §10, §16.10): never outside the root
//! (lexically and through symlinks), FAT/exFAT-safe names, temp file +
//! fsync + rename, an overwrite policy, backups of replaced files in the
//! app data dir, and a JSON-lines journal flushed per entry so a job can be
//! undone even after a crash. Never sets extended attributes and never uses
//! `fs::copy` (it copies xattrs on macOS, which makes `._*` files on FAT).
//! macOS still tags every file and folder a process creates with
//! `com.apple.provenance`, which user space cannot remove; on FAT/exFAT it
//! lands in a `._<name>` AppleDouble file, so the writer deletes the
//! sidecars of the paths it created (never one that was already there).
//!
//! Layout in the app data dir: journals `jobs/<job>.jsonl` (renamed
//! `<job>.undone.jsonl` after undo) and backups
//! `backups/<job>/<path relative to the library root>`.

use std::fs::{self, File, OpenOptions};
use std::io::{self, ErrorKind, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use unicode_normalization::UnicodeNormalization;

use crate::identify::hash::hex_upper;
use crate::time::{compact_utc, rfc3339_utc};

/// What to do when the target (or a case-insensitive twin) already exists.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OverwritePolicy {
    /// Keep the existing file.
    #[default]
    Never,
    /// Back the existing file up in the app data dir, then replace it.
    Backup,
}

/// Result of one [`Writer::write`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub enum WriteOutcome {
    Created { path: PathBuf },
    Replaced { path: PathBuf, backup: PathBuf },
    SkippedExists { existing: PathBuf },
}

/// Result of one [`Writer::remove`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub enum RemoveOutcome {
    Removed {
        path: PathBuf,
        backup: PathBuf,
    },
    /// Nothing at the target.
    Missing,
}

#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    #[error("refusing to write outside the library: {0}")]
    OutsideRoot(PathBuf),
    #[error("file name not valid on FAT/exFAT: {0}")]
    NameInvalid(String),
    #[error("write denied (read-only or no permission): {0}")]
    Denied(PathBuf),
    #[error("disk full")]
    DiskFull,
    #[error("nothing to undo")]
    NothingToUndo,
    #[error("not a regular file: {0}")]
    NotAFile(PathBuf),
    #[error("job journal is damaged: {0}")]
    Journal(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// A FAT/exFAT-safe version of `name`: control characters and
/// `< > : " / \ | ? *` become `_`, trailing spaces and dots are dropped,
/// DOS device names get a `_` prefix and the result fits in 255 bytes
/// (keeping a short extension). Empty results are refused.
pub fn safe_file_name(name: &str) -> Result<String, WriteError> {
    let mut s: String = name
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    while s.ends_with(' ') || s.ends_with('.') {
        s.pop();
    }
    if s.is_empty() {
        return Err(WriteError::NameInvalid(name.to_string()));
    }
    let base = s
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end()
        .to_ascii_uppercase();
    if RESERVED.contains(&base.as_str()) {
        s.insert(0, '_');
    }
    if s.len() > 255 {
        let (stem, ext) = match s.rfind('.') {
            Some(i) if s.len() - i <= 10 => (s[..i].to_string(), s[i..].to_string()),
            _ => (s.clone(), String::new()),
        };
        let mut cut = 255 - ext.len();
        while !stem.is_char_boundary(cut) {
            cut -= 1;
        }
        s = format!("{}{}", &stem[..cut], ext);
    }
    Ok(s)
}

/// `target` if it lies strictly inside `root`: lexically (no `..`, no
/// other root) and on disk (the deepest existing ancestor resolves inside
/// the canonical root, and the target itself is not a symlink).
pub fn ensure_within(root: &Path, target: &Path) -> Result<PathBuf, WriteError> {
    let outside = || WriteError::OutsideRoot(target.to_path_buf());
    let rel = target.strip_prefix(root).map_err(|_| outside())?;
    if rel.as_os_str().is_empty() || rel.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(outside());
    }
    let canon_root = root.canonicalize()?;
    let mut probe = target.parent().ok_or_else(outside)?.to_path_buf();
    while fs::symlink_metadata(&probe).is_err() {
        probe = probe.parent().ok_or_else(outside)?.to_path_buf();
    }
    if !probe.canonicalize()?.starts_with(&canon_root) {
        return Err(outside());
    }
    if fs::symlink_metadata(target).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(outside());
    }
    Ok(target.to_path_buf())
}

/// Upper-case hex SHA-1 of `bytes`.
pub fn sha1_hex(bytes: &[u8]) -> String {
    hex_upper(&Sha1::digest(bytes))
}

/// One line of a job journal.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum Line {
    Header {
        job: String,
        root: PathBuf,
        created_at: String,
    },
    Entry {
        target: PathBuf,
        op: Op,
        sha1: String,
    },
    Done,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
enum Op {
    Created,
    Replaced { backup: PathBuf },
    Removed { backup: PathBuf },
}

static COUNTER: AtomicU32 = AtomicU32::new(0);

fn new_job_id() -> String {
    format!(
        "{}-{:04x}-{:04x}",
        compact_utc(SystemTime::now()),
        std::process::id() & 0xffff,
        COUNTER.fetch_add(1, Ordering::SeqCst)
    )
}

/// A write job: every file it writes is journaled before `write` returns.
pub struct Writer {
    root: PathBuf,
    job: String,
    backups: PathBuf,
    journal: File,
    journal_path: PathBuf,
    counter: u32,
    /// Changes this run made to the library (journaled and done).
    changes: usize,
    resumed: bool,
}

impl Writer {
    /// Starts a job writing into `root` (must be absolute: the journal
    /// keeps it so `undo` works from any working directory).
    pub fn begin(root: &Path, data_dir: &Path) -> Result<Writer, WriteError> {
        if !root.is_absolute() {
            return Err(WriteError::OutsideRoot(root.to_path_buf()));
        }
        let jobs = data_dir.join("jobs");
        fs::create_dir_all(&jobs).map_err(|e| map_io(e, &jobs))?;
        let (job, journal, journal_path) = loop {
            let job = new_job_id();
            let path = jobs.join(format!("{job}.jsonl"));
            match OpenOptions::new().append(true).create_new(true).open(&path) {
                Ok(file) => break (job, file, path),
                Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(map_io(e, &path)),
            }
        };
        let mut writer = Writer {
            root: root.to_path_buf(),
            backups: data_dir.join("backups").join(&job),
            job,
            journal,
            journal_path,
            counter: 0,
            changes: 0,
            resumed: false,
        };
        let header = Line::Header {
            job: writer.job.clone(),
            root: writer.root.clone(),
            created_at: rfc3339_utc(SystemTime::now()),
        };
        writer.append(&header)?;
        Ok(writer)
    }

    /// Continues the journal of job `job` (paused earlier), so one undo
    /// reverts every run of it (D21). The journal must exist, not be
    /// undone and belong to `root`. A torn last line is cut off first.
    pub fn resume(root: &Path, data_dir: &Path, job: &str) -> Result<Writer, WriteError> {
        if !valid_job_id(job) {
            return Err(WriteError::Journal(format!("not a job id: {job}")));
        }
        let path = data_dir.join("jobs").join(format!("{job}.jsonl"));
        // Bytes, not text: a tear can split a multi-byte character.
        let bytes = fs::read(&path).map_err(|e| map_io(e, &path))?;
        if bytes.last().is_some_and(|&b| b != b'\n') {
            let keep = bytes.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
            let file = OpenOptions::new()
                .write(true)
                .open(&path)
                .map_err(|e| map_io(e, &path))?;
            file.set_len(keep as u64).map_err(|e| map_io(e, &path))?;
            file.sync_all()?;
        }
        let (header, entries, _) = parse(&path)?;
        let Line::Header {
            root: journal_root, ..
        } = header
        else {
            unreachable!("parse returns a header first")
        };
        if journal_root != root {
            return Err(WriteError::Journal(format!(
                "{} belongs to {}",
                path.display(),
                journal_root.display()
            )));
        }
        let journal = OpenOptions::new()
            .append(true)
            .open(&path)
            .map_err(|e| map_io(e, &path))?;
        Ok(Writer {
            root: root.to_path_buf(),
            job: job.to_string(),
            backups: data_dir.join("backups").join(job),
            journal,
            journal_path: path,
            counter: u32::try_from(entries.len()).unwrap_or(u32::MAX),
            changes: 0,
            resumed: true,
        })
    }

    pub fn job_id(&self) -> &str {
        &self.job
    }

    /// Writes `bytes` to `target` atomically under `policy`.
    pub fn write(
        &mut self,
        target: &Path,
        bytes: &[u8],
        policy: OverwritePolicy,
    ) -> Result<WriteOutcome, WriteError> {
        let target = ensure_within(&self.root, target)?;
        let name = target
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| WriteError::NameInvalid(target.display().to_string()))?;
        if safe_file_name(name)? != name {
            return Err(WriteError::NameInvalid(name.to_string()));
        }
        let parent = target
            .parent()
            .ok_or_else(|| WriteError::OutsideRoot(target.clone()))?;
        remove_apple_doubles(&create_dirs(parent)?);

        let existing = find_twin(parent, name)?;
        let (dest, backup) = match (existing, policy) {
            (None, _) => (target.clone(), None),
            (Some(existing), OverwritePolicy::Never) => {
                return Ok(WriteOutcome::SkippedExists { existing });
            }
            (Some(existing), OverwritePolicy::Backup) => {
                let meta = fs::symlink_metadata(&existing).map_err(|e| map_io(e, &existing))?;
                if !meta.file_type().is_file() {
                    // Never replace a folder or a symlink.
                    return Ok(WriteOutcome::SkippedExists { existing });
                }
                let backup = self.back_up(&existing)?;
                (existing, Some(backup))
            }
        };

        // A crash (or an earlier run of a resumed job) can leave a temp
        // file with the next name behind: skip past it.
        let mut tries = 0;
        loop {
            let tmp = format!(".coverly-{}-{}.tmp", self.job, self.counter);
            self.counter = self.counter.wrapping_add(1);
            match atomic_write(&dest, bytes, &tmp) {
                Err(WriteError::Io(e)) if e.kind() == ErrorKind::AlreadyExists && tries < 1000 => {
                    tries += 1;
                }
                result => break result?,
            }
        }

        let (op, outcome) = match backup {
            None => (Op::Created, WriteOutcome::Created { path: dest.clone() }),
            Some(backup) => (
                Op::Replaced {
                    backup: backup.clone(),
                },
                WriteOutcome::Replaced {
                    path: dest.clone(),
                    backup,
                },
            ),
        };
        self.append(&Line::Entry {
            target: dest,
            op,
            sha1: sha1_hex(bytes),
        })?;
        self.changes += 1;
        Ok(outcome)
    }

    /// Deletes the art file `target`: backed up first and journaled before
    /// the delete, so undo restores it. A missing target is
    /// [`RemoveOutcome::Missing`]; folders and symlinks are never removed.
    pub fn remove(&mut self, target: &Path) -> Result<RemoveOutcome, WriteError> {
        let target = ensure_within(&self.root, target)?;
        let meta = match fs::symlink_metadata(&target) {
            Ok(meta) => meta,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(RemoveOutcome::Missing),
            Err(e) => return Err(map_io(e, &target)),
        };
        if !meta.file_type().is_file() {
            return Err(WriteError::NotAFile(target));
        }
        let bytes = fs::read(&target).map_err(|e| map_io(e, &target))?;
        let backup = self.back_up(&target)?;
        self.append(&Line::Entry {
            target: target.clone(),
            op: Op::Removed {
                backup: backup.clone(),
            },
            sha1: sha1_hex(&bytes),
        })?;
        // A failed delete leaves the file as it was: the entry stays in the
        // journal (its undo is a no-op) but it is not a change.
        fs::remove_file(&target).map_err(|e| map_io(e, &target))?;
        self.changes += 1;
        Ok(RemoveOutcome::Removed {
            path: target,
            backup,
        })
    }

    /// Marks the job complete and returns its id.
    pub fn finish(mut self) -> Result<String, WriteError> {
        self.append(&Line::Done)?;
        Ok(self.job)
    }

    /// Whether this run changed the library (a file written or removed).
    pub fn has_entries(&self) -> bool {
        self.changes > 0
    }

    /// Drops a job that changed nothing: deletes its journal and backups,
    /// so it is neither listed, undone nor counted when pruning. Refused
    /// (and the journal kept) for a job that changed something or was
    /// resumed.
    pub fn discard(self) -> Result<(), WriteError> {
        if self.changes > 0 || self.resumed {
            return Err(WriteError::Journal(format!(
                "job {} cannot be discarded",
                self.job
            )));
        }
        let Writer {
            journal,
            journal_path,
            backups,
            ..
        } = self;
        drop(journal);
        fs::remove_file(&journal_path).map_err(|e| map_io(e, &journal_path))?;
        match fs::remove_dir_all(&backups) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
            Err(e) => Err(map_io(e, &backups)),
        }
    }

    fn back_up(&self, existing: &Path) -> Result<PathBuf, WriteError> {
        let rel = existing
            .strip_prefix(&self.root)
            .map_err(|_| WriteError::OutsideRoot(existing.to_path_buf()))?;
        let backup = self.backups.join(rel);
        if backup.exists() {
            // Written twice in one job: keep the original bytes.
            return Ok(backup);
        }
        if let Some(dir) = backup.parent() {
            fs::create_dir_all(dir).map_err(|e| map_io(e, dir))?;
        }
        let old = fs::read(existing).map_err(|e| map_io(e, existing))?;
        let mut file = File::create(&backup).map_err(|e| map_io(e, &backup))?;
        file.write_all(&old).map_err(|e| map_io(e, &backup))?;
        file.sync_all().map_err(|e| map_io(e, &backup))?;
        Ok(backup)
    }

    fn append(&mut self, line: &Line) -> Result<(), WriteError> {
        let mut text =
            serde_json::to_string(line).map_err(|e| WriteError::Journal(e.to_string()))?;
        text.push('\n');
        self.journal.write_all(text.as_bytes())?;
        self.journal.sync_all()?;
        Ok(())
    }
}

/// The entry of `dir` whose name equals `name` ignoring case (NFC, lower
/// case): FAT and exFAT are case-insensitive. An exact match wins.
fn find_twin(dir: &Path, name: &str) -> Result<Option<PathBuf>, WriteError> {
    let key = |s: &str| s.nfc().collect::<String>().to_lowercase();
    let wanted = key(name);
    let mut found = None;
    for entry in fs::read_dir(dir).map_err(|e| map_io(e, dir))? {
        let entry = entry?;
        let file_name = entry.file_name();
        let Some(other) = file_name.to_str() else {
            continue;
        };
        if other == name {
            return Ok(Some(entry.path()));
        }
        if found.is_none() && key(other) == wanted {
            found = Some(entry.path());
        }
    }
    Ok(found)
}

/// `dir/._name`: the AppleDouble file macOS keeps next to `path` on
/// FAT/exFAT for the extended attributes those file systems cannot store.
fn apple_double(path: &Path) -> Option<PathBuf> {
    let name = path.file_name()?.to_str()?;
    Some(path.with_file_name(format!("._{name}")))
}

/// Deletes the AppleDouble sidecars of `paths`, best effort. Only for
/// paths this writer created, or whose sidecar did not exist before.
fn remove_apple_doubles(paths: &[PathBuf]) {
    for sidecar in paths.iter().filter_map(|p| apple_double(p)) {
        let _ = fs::remove_file(sidecar);
    }
}

/// `create_dir_all(dir)`, returning the folders that did not exist.
fn create_dirs(dir: &Path) -> Result<Vec<PathBuf>, WriteError> {
    let mut missing = Vec::new();
    let mut probe = Some(dir);
    // Only a NotFound folder counts as ours: one we cannot stat may already
    // exist, and its sidecar must survive.
    while let Some(p) = probe.filter(|p| {
        fs::symlink_metadata(p).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
    }) {
        missing.push(p.to_path_buf());
        probe = p.parent();
    }
    fs::create_dir_all(dir).map_err(|e| map_io(e, dir))?;
    Ok(missing)
}

/// Writes `bytes` to a temp file named `tmp_name` next to `dest`, fsyncs
/// it and renames it over `dest`. The temp file never survives an error,
/// and neither do the AppleDouble sidecars the write caused.
fn atomic_write(dest: &Path, bytes: &[u8], tmp_name: &str) -> Result<(), WriteError> {
    let dir = dest
        .parent()
        .ok_or_else(|| WriteError::OutsideRoot(dest.to_path_buf()))?;
    let tmp = dir.join(tmp_name);
    let dest_had_sidecar = apple_double(dest).is_some_and(|s| fs::symlink_metadata(s).is_ok());
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
            .map_err(|e| map_io(e, dest))?;
        file.write_all(bytes).map_err(|e| map_io(e, dest))?;
        file.sync_all().map_err(|e| map_io(e, dest))?;
        drop(file);
        fs::rename(&tmp, dest).map_err(|e| map_io(e, dest))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    remove_apple_doubles(&[tmp]);
    if result.is_ok() && !dest_had_sidecar {
        remove_apple_doubles(&[dest.to_path_buf()]);
    }
    result
}

fn map_io(e: io::Error, path: &Path) -> WriteError {
    match e.kind() {
        ErrorKind::PermissionDenied | ErrorKind::ReadOnlyFilesystem => {
            return WriteError::Denied(path.to_path_buf())
        }
        ErrorKind::StorageFull => return WriteError::DiskFull,
        _ => {}
    }
    match e.raw_os_error() {
        #[cfg(unix)]
        Some(30) => WriteError::Denied(path.to_path_buf()),
        #[cfg(unix)]
        Some(28) => WriteError::DiskFull,
        #[cfg(windows)]
        Some(112) | Some(39) => WriteError::DiskFull,
        _ => WriteError::Io(e),
    }
}

/// What [`undo_job`] did.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct UndoReport {
    pub job: String,
    pub removed: Vec<PathBuf>,
    pub restored: Vec<PathBuf>,
    /// Files changed (or deleted) after the job: left as they are.
    pub skipped_modified: Vec<PathBuf>,
}

/// One journaled job, for `coverly undo --list`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct JobSummary {
    pub job: String,
    pub root: PathBuf,
    pub created_at: String,
    pub entries: usize,
    pub complete: bool,
    pub undone: bool,
}

/// A parsed journal file.
struct Journal {
    job: String,
    path: PathBuf,
    undone: bool,
}

/// Journals in `<data>/jobs`, newest first. A missing folder means none.
fn journals(data_dir: &Path) -> Result<Vec<Journal>, WriteError> {
    let dir = data_dir.join("jobs");
    let read = match fs::read_dir(&dir) {
        Ok(read) => read,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(map_io(e, &dir)),
    };
    let mut out = Vec::new();
    for entry in read {
        let entry = entry?;
        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else {
            continue;
        };
        let (job, undone) = if let Some(job) = name.strip_suffix(".undone.jsonl") {
            (job, true)
        } else if let Some(job) = name.strip_suffix(".jsonl") {
            (job, false)
        } else {
            continue;
        };
        if !valid_job_id(job) {
            continue;
        }
        out.push(Journal {
            job: job.to_string(),
            path: entry.path(),
            undone,
        });
    }
    // Ids start with a compact UTC time, so they sort by creation.
    out.sort_by(|a, b| b.job.cmp(&a.job));
    Ok(out)
}

fn valid_job_id(job: &str) -> bool {
    !job.is_empty()
        && job
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Parses a journal. A torn last line (crash while appending, possibly
/// inside a multi-byte character) is ignored; any other bad line is an
/// error. `done` is whether the last run finished: an entry after a `done`
/// line (a resumed run) clears it.
fn parse(path: &Path) -> Result<(Line, Vec<Line>, bool), WriteError> {
    let bytes = fs::read(path).map_err(|e| map_io(e, path))?;
    let torn = bytes.last().is_some_and(|&b| b != b'\n');
    let lines: Vec<&[u8]> = bytes
        .split(|&b| b == b'\n')
        .filter(|l| !l.trim_ascii().is_empty())
        .collect();
    let mut parsed = Vec::with_capacity(lines.len());
    for (i, raw) in lines.iter().enumerate() {
        match serde_json::from_slice::<Line>(raw) {
            Ok(line) => parsed.push(line),
            Err(_) if i + 1 == lines.len() && torn => {}
            Err(e) => {
                return Err(WriteError::Journal(format!(
                    "{}: line {}: {e}",
                    path.display(),
                    i + 1
                )))
            }
        }
    }
    let mut iter = parsed.into_iter();
    let header = match iter.next() {
        Some(h @ Line::Header { .. }) => h,
        _ => {
            return Err(WriteError::Journal(format!(
                "{}: no header",
                path.display()
            )))
        }
    };
    let mut entries = Vec::new();
    let mut done = false;
    for line in iter {
        match line {
            Line::Entry { .. } => {
                entries.push(line);
                done = false;
            }
            Line::Done => done = true,
            Line::Header { .. } => {
                return Err(WriteError::Journal(format!(
                    "{}: second header",
                    path.display()
                )))
            }
        }
    }
    Ok((header, entries, done))
}

/// All journaled jobs, newest first.
pub fn list_jobs(data_dir: &Path) -> Result<Vec<JobSummary>, WriteError> {
    let mut out = Vec::new();
    for journal in journals(data_dir)? {
        let summary = match parse(&journal.path) {
            Ok((
                Line::Header {
                    root, created_at, ..
                },
                entries,
                done,
            )) => JobSummary {
                job: journal.job,
                root,
                created_at,
                entries: entries.len(),
                complete: done,
                undone: journal.undone,
            },
            _ => JobSummary {
                job: journal.job,
                root: PathBuf::new(),
                created_at: String::new(),
                entries: 0,
                complete: false,
                undone: journal.undone,
            },
        };
        out.push(summary);
    }
    Ok(out)
}

/// Undoes the newest job that is not undone yet.
pub fn undo_last(data_dir: &Path) -> Result<UndoReport, WriteError> {
    let journal = journals(data_dir)?
        .into_iter()
        .find(|j| !j.undone)
        .ok_or(WriteError::NothingToUndo)?;
    undo_journal(data_dir, &journal)
}

/// Undoes the job `job` (an id from [`list_jobs`]).
pub fn undo_job(data_dir: &Path, job: &str) -> Result<UndoReport, WriteError> {
    if !valid_job_id(job) {
        return Err(WriteError::NothingToUndo);
    }
    let journal = journals(data_dir)?
        .into_iter()
        .find(|j| j.job == job && !j.undone)
        .ok_or(WriteError::NothingToUndo)?;
    undo_journal(data_dir, &journal)
}

fn undo_journal(data_dir: &Path, journal: &Journal) -> Result<UndoReport, WriteError> {
    let (header, entries, _) = parse(&journal.path)?;
    let Line::Header { root, .. } = header else {
        unreachable!("parse returns a header first")
    };
    let backups = data_dir.join("backups").join(&journal.job);
    let mut report = UndoReport {
        job: journal.job.clone(),
        removed: Vec::new(),
        restored: Vec::new(),
        skipped_modified: Vec::new(),
    };
    for (n, entry) in entries.into_iter().rev().enumerate() {
        let Line::Entry { target, op, sha1 } = entry else {
            continue;
        };
        let target = ensure_within(&root, &target)?;
        let current = match fs::read(&target) {
            Ok(bytes) => Some(bytes),
            Err(e) if e.kind() == ErrorKind::NotFound => None,
            Err(e) => return Err(map_io(e, &target)),
        };
        match (op, current) {
            // Already gone: nothing to remove.
            (Op::Created, None) => {}
            (Op::Replaced { .. }, None) => report.skipped_modified.push(target),
            (Op::Removed { backup }, None) => {
                let backup = ensure_within(&backups, &backup)?;
                let old = fs::read(&backup).map_err(|e| map_io(e, &backup))?;
                if let Some(parent) = target.parent() {
                    remove_apple_doubles(&create_dirs(parent)?);
                }
                let tmp = format!(".coverly-undo-{}-{n}.tmp", journal.job);
                atomic_write(&target, &old, &tmp)?;
                report.restored.push(target);
            }
            // The delete never happened (crash between journal and delete).
            (Op::Removed { .. }, Some(bytes)) if sha1_hex(&bytes) == sha1 => {}
            (Op::Removed { .. }, Some(_)) => report.skipped_modified.push(target),
            (_, Some(bytes)) if sha1_hex(&bytes) != sha1 => report.skipped_modified.push(target),
            (Op::Created, Some(_)) => {
                fs::remove_file(&target).map_err(|e| map_io(e, &target))?;
                report.removed.push(target);
            }
            (Op::Replaced { backup }, Some(_)) => {
                let backup = ensure_within(&backups, &backup)?;
                let old = fs::read(&backup).map_err(|e| map_io(e, &backup))?;
                let tmp = format!(".coverly-undo-{}-{n}.tmp", journal.job);
                atomic_write(&target, &old, &tmp)?;
                report.restored.push(target);
            }
        }
    }
    let undone = journal
        .path
        .with_file_name(format!("{}.undone.jsonl", journal.job));
    fs::rename(&journal.path, &undone).map_err(|e| map_io(e, &journal.path))?;
    Ok(report)
}

/// Keeps the newest `keep` journals (and their backups); deletes the rest.
pub fn prune_jobs(data_dir: &Path, keep: usize) -> Result<(), WriteError> {
    for journal in journals(data_dir)?.into_iter().skip(keep) {
        fs::remove_file(&journal.path).map_err(|e| map_io(e, &journal.path))?;
        let backups = data_dir.join("backups").join(&journal.job);
        match fs::remove_dir_all(&backups) {
            Ok(()) => {}
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(e) => return Err(map_io(e, &backups)),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apple_double_is_the_dot_underscore_sibling() {
        assert_eq!(
            apple_double(Path::new("/r/ROMS/GBA/Imgs/a b.png")),
            Some(PathBuf::from("/r/ROMS/GBA/Imgs/._a b.png"))
        );
    }

    #[test]
    fn create_dirs_reports_only_the_folders_it_created() {
        let tmp = tempfile::tempdir().unwrap();
        let existing = tmp.path().join("ROMS");
        fs::create_dir(&existing).unwrap();
        let deep = existing.join("GBA").join("Imgs");
        let mut created = create_dirs(&deep).unwrap();
        created.sort();
        assert_eq!(created, [existing.join("GBA"), deep.clone()]);
        assert!(deep.is_dir());
        assert!(create_dirs(&deep).unwrap().is_empty());
    }

    #[test]
    fn removes_the_sidecars_of_new_paths_only() {
        let tmp = tempfile::tempdir().unwrap();
        let (new, old) = (tmp.path().join("new.png"), tmp.path().join("old.png"));
        for p in ["new.png", "._new.png", "old.png", "._old.png"] {
            fs::write(tmp.path().join(p), b"x").unwrap();
        }
        remove_apple_doubles(std::slice::from_ref(&new));
        assert!(!tmp.path().join("._new.png").exists());
        assert!(new.exists() && old.exists() && tmp.path().join("._old.png").exists());
        // Missing sidecars are not an error.
        remove_apple_doubles(&[new]);
    }

    #[test]
    fn atomic_write_keeps_a_sidecar_that_was_already_there() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("a.png");
        fs::write(tmp.path().join("._a.png"), b"user metadata").unwrap();
        atomic_write(&dest, b"png", ".coverly-t-0.tmp").unwrap();
        assert_eq!(fs::read(&dest).unwrap(), b"png");
        assert_eq!(
            fs::read(tmp.path().join("._a.png")).unwrap(),
            b"user metadata"
        );
    }
}
