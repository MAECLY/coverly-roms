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

use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{self, ErrorKind, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use unicode_normalization::UnicodeNormalization;

use crate::identify::hash::hex_upper;
use crate::time::{compact_utc, parse_compact_utc, rfc3339_utc};

mod noclobber;
mod transfer;
mod undo_organize;
use noclobber::is_cross_device;
#[doc(hidden)]
pub use noclobber::rename_noclobber_using;
pub use noclobber::{rename_noclobber, same_volume};
#[cfg(feature = "test-util")]
#[doc(hidden)]
pub use transfer::set_copy_fault;
pub use transfer::{Collision, CopyFault, CopyStage, MoveOutcome};

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
    #[error("cannot move across volumes: {0}")]
    CrossDevice(PathBuf),
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

/// The first bytes of an AppleDouble file.
const APPLE_DOUBLE_MAGIC: [u8; 4] = [0x00, 0x05, 0x16, 0x07];

/// Deletes the AppleDouble files (`._*` regular files starting with the
/// AppleDouble magic) directly in `dir`; returns how many. Never recurses,
/// never touches anything else; errors only skip files.
pub fn clean_apple_doubles(dir: &Path) -> usize {
    let Ok(read) = fs::read_dir(dir) else {
        return 0;
    };
    let mut removed = 0;
    for entry in read.flatten() {
        let name = entry.file_name();
        if !name.to_string_lossy().starts_with("._") {
            continue;
        }
        let path = entry.path();
        let is_file = fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_file());
        let mut head = [0u8; 4];
        let magic = is_file
            && File::open(&path)
                .and_then(|mut f| io::Read::read_exact(&mut f, &mut head))
                .is_ok()
            && head == APPLE_DOUBLE_MAGIC;
        if magic && fs::remove_file(&path).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// Whether `root` is on a removable volume by its mount point (macOS
/// `/Volumes/…`; Linux `/media`, `/run/media`, `/mnt`). Other systems: no.
pub fn on_removable_volume(root: &Path) -> bool {
    let prefixes: &[&str] = if cfg!(target_os = "macos") {
        &["/Volumes/"]
    } else if cfg!(target_os = "linux") {
        &["/media/", "/run/media/", "/mnt/"]
    } else {
        &[]
    };
    let text = root.to_string_lossy();
    prefixes.iter().any(|p| text.starts_with(p))
}

/// Upper-case hex SHA-1 of `bytes`.
pub fn sha1_hex(bytes: &[u8]) -> String {
    hex_upper(&Sha1::digest(bytes))
}

/// What a job did: fill art (and gamelists) or organise a ROM pile.
/// Journals written before organise jobs existed have no kind: fill.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum JobKind {
    #[default]
    Fill,
    Organize,
}

/// Enough of a file's state to tell, at undo time, whether it is still the
/// file the job left there, without re-reading multi-GB files: size and
/// modification time, plus the CRC-32 when the job computed one (copies).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fingerprint {
    pub size: u64,
    pub mtime_ns: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crc32: Option<u32>,
}

/// How a moved file got to its target: a rename on the same volume, or a
/// verified copy followed by deleting the source.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Via {
    Rename,
    Copy,
}

/// The header fields of a new job besides its root.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct JobMeta {
    pub kind: JobKind,
    /// Where an organise job takes files from (absolute); `None` means the
    /// job root.
    pub source_root: Option<PathBuf>,
}

/// One line of a job journal. Fields added after the first format are all
/// optional, so older journals parse unchanged; older builds ignore them.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum Line {
    Header {
        job: String,
        root: PathBuf,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source_root: Option<PathBuf>,
        #[serde(default)]
        kind: JobKind,
        created_at: String,
    },
    Entry {
        target: PathBuf,
        op: Op,
        /// SHA-1 of the bytes written (fill ops). Organise ops use `fp`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sha1: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fp: Option<Fingerprint>,
        /// Index of the plan item the entry belongs to (organise jobs).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        item: Option<usize>,
    },
    Done,
}

// The organise ops are written by `Writer::move_file` / `copy_file` /
// `ensure_dir` and the override rewrite of `organize::apply`.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
enum Op {
    Created,
    Replaced {
        backup: PathBuf,
    },
    Removed {
        backup: PathBuf,
    },
    /// `target` came from `from` (absolute, inside the source root).
    Moved {
        from: PathBuf,
        via: Via,
    },
    /// `target` is a copy of `from`; the source stays.
    Copied {
        from: PathBuf,
    },
    /// The job created the folder `target`.
    DirCreated,
    /// A manual-override entry keyed by `from` was rekeyed to `to`.
    OverrideMoved {
        from: PathBuf,
        to: PathBuf,
    },
    /// `target` holds the entry `entry` of the archive `from` (absolute,
    /// inside the source root). Written twice, like a copied `Moved`: the
    /// intent (no `fp`) before the entry is decoded to the job temp file
    /// `temp` in `target`'s folder, and the completion (`fp` with the CRC
    /// computed while decoding) once the temp was renamed to `target`.
    /// `size` and `crc32` are what the archive lists.
    Extracted {
        from: PathBuf,
        entry: String,
        size: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        crc32: Option<u32>,
        temp: String,
    },
    /// The extraction whose intent named the job temp `temp` was undone
    /// before its rename (the temp is deleted, `target` never written).
    RolledBack {
        temp: String,
    },
    /// About to send the archive `from` to the Trash (`fp` = the archive).
    ArchiveTrash {
        from: PathBuf,
    },
    /// The archive `from` is in the Trash, at `trash` when the OS said.
    ArchiveTrashed {
        from: PathBuf,
        trash: Option<PathBuf>,
    },
    /// The archive `from` stays where it was (no Trash on its volume).
    ArchiveKept {
        from: PathBuf,
    },
    /// An op this build does not know (a journal from a newer Coverly):
    /// the job is listed but cannot be undone.
    #[serde(other)]
    Unknown,
}

impl Op {
    /// One of the extraction ops (an archive entry or the archive itself).
    fn is_extract_op(&self) -> bool {
        matches!(
            self,
            Op::Extracted { .. }
                | Op::RolledBack { .. }
                | Op::ArchiveTrash { .. }
                | Op::ArchiveTrashed { .. }
                | Op::ArchiveKept { .. }
        )
    }
}

/// Why this build refuses to undo a journal, if it does: an op it does
/// not know (a newer Coverly), or an extraction, whose undo (deleting the
/// extracted files only once the archive is back from the Trash) is not
/// part of this build yet. Either way the whole job is refused, never
/// half undone.
fn undo_refusal(entries: &[Line]) -> Option<&'static str> {
    let ops = || {
        entries.iter().filter_map(|e| match e {
            Line::Entry { op, .. } => Some(op),
            _ => None,
        })
    };
    if ops().any(|op| matches!(op, Op::Unknown)) {
        return Some("written by a newer Coverly; update to undo it");
    }
    if ops().any(Op::is_extract_op) {
        return Some("extracted archives cannot be undone by this version");
    }
    None
}

/// Whether every entry of a journal can be undone by this build.
fn entries_undoable(entries: &[Line]) -> bool {
    undo_refusal(entries).is_none()
}

/// One archive entry an extraction writes, as its journal lines name it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtractedFile {
    /// The archive (absolute, inside the job's source root).
    pub from: PathBuf,
    /// The entry's name inside the archive.
    pub entry: String,
    /// The size and CRC-32 the archive lists for the entry.
    pub size: u64,
    pub crc32: Option<u32>,
    /// The job temp file the entry is decoded to: a bare name
    /// `.coverly-<job>-<n>.tmp` in the target's folder.
    pub temp: String,
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
    /// Where moved and copied files come from; the root for fill jobs.
    source_root: PathBuf,
    kind: JobKind,
    job: String,
    backups: PathBuf,
    journal: File,
    journal_path: PathBuf,
    counter: u32,
    /// The job temps an `extracted` intent names in this journal (this
    /// run's and earlier runs'): each is used by one intent only, so a
    /// completion or rollback line always points at one extraction.
    extract_temps: HashSet<String>,
    /// Changes this run made to the library (done, journaled or not).
    changes: usize,
    resumed: bool,
}

impl Writer {
    /// Starts a job writing into `root` (must be absolute: the journal
    /// keeps it so `undo` works from any working directory).
    pub fn begin(root: &Path, data_dir: &Path) -> Result<Writer, WriteError> {
        Writer::begin_with(root, data_dir, JobMeta::default())
    }

    /// Starts a job of `meta.kind` writing into `root`; an organise job
    /// also names the folder its files come from (absolute, journaled in
    /// the header so undo can put them back).
    pub fn begin_with(root: &Path, data_dir: &Path, meta: JobMeta) -> Result<Writer, WriteError> {
        if !root.is_absolute() {
            return Err(WriteError::OutsideRoot(root.to_path_buf()));
        }
        if let Some(source) = meta.source_root.as_deref().filter(|s| !s.is_absolute()) {
            return Err(WriteError::OutsideRoot(source.to_path_buf()));
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
            source_root: meta
                .source_root
                .clone()
                .unwrap_or_else(|| root.to_path_buf()),
            kind: meta.kind,
            backups: data_dir.join("backups").join(&job),
            job,
            journal,
            journal_path,
            counter: 0,
            extract_temps: HashSet::new(),
            changes: 0,
            resumed: false,
        };
        let header = Line::Header {
            job: writer.job.clone(),
            root: writer.root.clone(),
            source_root: meta.source_root,
            kind: meta.kind,
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
            root: journal_root,
            source_root,
            kind,
            ..
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
        let extract_temps: HashSet<String> = entries
            .iter()
            .filter_map(|e| match e {
                Line::Entry {
                    op: Op::Extracted { temp, .. },
                    ..
                } => Some(temp.clone()),
                _ => None,
            })
            .collect();
        // Past every line and every journaled temp number: the temp
        // counter of an earlier run can run ahead of its line count.
        let counter = extract_temps
            .iter()
            .filter_map(|t| temp_number(job, t))
            .map(|n| n.saturating_add(1))
            .chain([u32::try_from(entries.len()).unwrap_or(u32::MAX)])
            .max()
            .unwrap_or(0);
        Ok(Writer {
            root: root.to_path_buf(),
            source_root: source_root.unwrap_or_else(|| root.to_path_buf()),
            kind,
            job: job.to_string(),
            backups: data_dir.join("backups").join(job),
            journal,
            journal_path: path,
            counter,
            extract_temps,
            changes: 0,
            resumed: true,
        })
    }

    pub fn job_id(&self) -> &str {
        &self.job
    }

    pub fn kind(&self) -> JobKind {
        self.kind
    }

    /// Where the job's files come from: the root unless the header names
    /// another folder.
    pub fn source_root(&self) -> &Path {
        &self.source_root
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
        // The library has changed: count it before journaling, so a failed
        // append never lets `discard` delete the backup of what was replaced.
        self.changes += 1;

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
        self.append_entry(dest, op, Some(sha1_hex(bytes)), None, None)?;
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
        self.append_entry(
            target.clone(),
            Op::Removed {
                backup: backup.clone(),
            },
            Some(sha1_hex(&bytes)),
            None,
            None,
        )?;
        // A failed delete leaves the file as it was: the entry stays in the
        // journal (its undo is a no-op) but it is not a change.
        fs::remove_file(&target).map_err(|e| map_io(e, &target))?;
        self.changes += 1;
        Ok(RemoveOutcome::Removed {
            path: target,
            backup,
        })
    }

    /// Journals that the manual identity keyed by `from` is about to be
    /// rekeyed to `to` (both absolute), so undo puts it back. The caller
    /// rewrites the overrides file after this returns.
    pub fn record_override_moved(
        &mut self,
        from: &Path,
        to: &Path,
        item: Option<usize>,
    ) -> Result<(), WriteError> {
        self.append_entry(
            to.to_path_buf(),
            Op::OverrideMoved {
                from: from.to_path_buf(),
                to: to.to_path_buf(),
            },
            None,
            None,
            item,
        )?;
        self.changes += 1;
        Ok(())
    }

    /// Journals that the archive entry `file` is about to be decoded to
    /// its temp file for `target` (inside the root). Nothing is written
    /// to the library yet; [`Writer::extract_done`] or
    /// [`Writer::rolled_back`] follows.
    /// `file.temp` must be a job temp no earlier intent of this job used
    /// (see [`Writer::extract_temp`]).
    pub fn extract_intent(
        &mut self,
        target: &Path,
        file: &ExtractedFile,
        item: Option<usize>,
    ) -> Result<(), WriteError> {
        let target = ensure_within(&self.root, target)?;
        let op = self.extracted_op(file)?;
        if self.extract_temps.contains(&file.temp) {
            return Err(WriteError::NameInvalid(file.temp.clone()));
        }
        self.append_entry(target, op, None, None, item)?;
        self.extract_temps.insert(file.temp.clone());
        Ok(())
    }

    /// A fresh job temp name (`.coverly-<job>-<n>.tmp`) for an
    /// extraction: never one an intent of this job (in this run or an
    /// earlier one) already named.
    pub fn extract_temp(&mut self) -> String {
        loop {
            let temp = format!(".coverly-{}-{}.tmp", self.job, self.counter);
            self.counter = self.counter.wrapping_add(1);
            if !self.extract_temps.contains(&temp) {
                return temp;
            }
        }
    }

    /// Journals that the temp of `file` was renamed to `target` (a
    /// regular file now): the completion line, with `target`'s size and
    /// modification time and the CRC-32 computed while decoding. Returns
    /// that fingerprint.
    pub fn extract_done(
        &mut self,
        target: &Path,
        file: &ExtractedFile,
        crc32: u32,
        item: Option<usize>,
    ) -> Result<Fingerprint, WriteError> {
        let target = ensure_within(&self.root, target)?;
        let op = self.extracted_op(file)?;
        self.check_intent(&file.temp)?;
        let meta = fs::symlink_metadata(&target).map_err(|e| map_io(e, &target))?;
        if !meta.file_type().is_file() {
            return Err(WriteError::NotAFile(target));
        }
        // The rename already happened: count it even if the append fails.
        self.changes += 1;
        let fp = Fingerprint {
            size: meta.len(),
            mtime_ns: meta.modified().map_or(0, transfer::mtime_ns),
            crc32: Some(crc32),
        };
        self.append_entry(target, op, None, Some(fp), item)?;
        Ok(fp)
    }

    /// Journals that the extraction for `target` whose intent named the
    /// job temp `temp` was rolled back (its temp deleted, no rename).
    pub fn rolled_back(
        &mut self,
        target: &Path,
        temp: &str,
        item: Option<usize>,
    ) -> Result<(), WriteError> {
        let target = ensure_within(&self.root, target)?;
        self.check_job_temp(temp)?;
        self.check_intent(temp)?;
        let op = Op::RolledBack {
            temp: temp.to_string(),
        };
        self.append_entry(target, op, None, None, item)
    }

    /// Journals that the archive `from` (a regular file inside the source
    /// root) is about to go to the Trash, with its fingerprint (size and
    /// modification time), which it returns.
    pub fn archive_trash_intent(
        &mut self,
        from: &Path,
        item: Option<usize>,
    ) -> Result<Fingerprint, WriteError> {
        let from = ensure_within(&self.source_root, from)?;
        let meta = fs::symlink_metadata(&from).map_err(|e| map_io(e, &from))?;
        if !meta.file_type().is_file() {
            return Err(WriteError::NotAFile(from));
        }
        let fp = Fingerprint {
            size: meta.len(),
            mtime_ns: meta.modified().map_or(0, transfer::mtime_ns),
            crc32: None,
        };
        let op = Op::ArchiveTrash { from: from.clone() };
        self.append_entry(from, op, None, Some(fp), item)?;
        Ok(fp)
    }

    /// Journals that the archive `from` went to the Trash, at `trash`
    /// when the OS reported where.
    pub fn archive_trashed(
        &mut self,
        from: &Path,
        trash: Option<&Path>,
        item: Option<usize>,
    ) -> Result<(), WriteError> {
        let from = ensure_within(&self.source_root, from)?;
        // The archive left its folder: count it even if the append fails.
        self.changes += 1;
        let op = Op::ArchiveTrashed {
            from: from.clone(),
            trash: trash.map(Path::to_path_buf),
        };
        self.append_entry(from, op, None, None, item)
    }

    /// Journals that the archive `from` stays where it is (its volume has
    /// no Trash, or the Trash refused it).
    pub fn archive_kept(&mut self, from: &Path, item: Option<usize>) -> Result<(), WriteError> {
        let from = ensure_within(&self.source_root, from)?;
        let op = Op::ArchiveKept { from: from.clone() };
        self.append_entry(from, op, None, None, item)
    }

    /// The `extracted` op of `file`, once its archive is inside the source
    /// root and its temp is one of this job's.
    fn extracted_op(&self, file: &ExtractedFile) -> Result<Op, WriteError> {
        let from = ensure_within(&self.source_root, &file.from)?;
        self.check_job_temp(&file.temp)?;
        Ok(Op::Extracted {
            from,
            entry: file.entry.clone(),
            size: file.size,
            crc32: file.crc32,
            temp: file.temp.clone(),
        })
    }

    /// An `extracted` intent of this job named `temp`.
    fn check_intent(&self, temp: &str) -> Result<(), WriteError> {
        if self.extract_temps.contains(temp) {
            Ok(())
        } else {
            Err(WriteError::NameInvalid(temp.to_string()))
        }
    }

    /// `temp` is a bare file name `.coverly-<this job>-<n>.tmp`: undo and
    /// resume only ever delete such names, next to the target.
    fn check_job_temp(&self, temp: &str) -> Result<(), WriteError> {
        if is_job_temp(&self.job, temp) {
            Ok(())
        } else {
            Err(WriteError::NameInvalid(temp.to_string()))
        }
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

    /// Appends (and flushes) one journal entry.
    fn append_entry(
        &mut self,
        target: PathBuf,
        op: Op,
        sha1: Option<String>,
        fp: Option<Fingerprint>,
        item: Option<usize>,
    ) -> Result<(), WriteError> {
        self.append(&Line::Entry {
            target,
            op,
            sha1,
            fp,
            item,
        })
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

/// Whether `name` is a temp file name of job `job`:
/// `.coverly-<job>-<digits>.tmp`, with no path separator.
pub(crate) fn is_job_temp(job: &str, name: &str) -> bool {
    temp_digits(job, name).is_some()
}

/// The `<digits>` of the job temp name `name` of job `job`.
fn temp_digits<'n>(job: &str, name: &'n str) -> Option<&'n str> {
    name.strip_prefix(".coverly-")
        .and_then(|rest| rest.strip_prefix(job))
        .and_then(|rest| rest.strip_prefix('-'))
        .and_then(|rest| rest.strip_suffix(".tmp"))
        .filter(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// The number of the job temp name `name` (`None` past `u32`).
fn temp_number(job: &str, name: &str) -> Option<u32> {
    temp_digits(job, name)?.parse().ok()
}

/// The key file names are compared by: NFC, lower case. FAT and exFAT
/// are case-insensitive and macOS hands out decomposed names.
pub(crate) fn name_key(name: &str) -> String {
    name.nfc().collect::<String>().to_lowercase()
}

/// The entry of `dir` whose name equals `name` ignoring case (NFC, lower
/// case): FAT and exFAT are case-insensitive. An exact match wins.
pub(crate) fn find_twin(dir: &Path, name: &str) -> Result<Option<PathBuf>, WriteError> {
    #[cfg(feature = "test-util")]
    TWIN_LISTINGS.with(|c| c.set(c.get() + 1));
    let key = name_key;
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

#[cfg(feature = "test-util")]
thread_local! {
    static TWIN_LISTINGS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Test hook: how many folders the writer listed to look for a case twin
/// on the calling thread so far.
#[doc(hidden)]
#[cfg(feature = "test-util")]
pub fn twin_listings() -> usize {
    TWIN_LISTINGS.with(std::cell::Cell::get)
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
    if is_cross_device(&e) {
        return WriteError::CrossDevice(path.to_path_buf());
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
    /// Moved files whose original place is taken again by another file:
    /// both are left as they are.
    pub skipped_occupied: Vec<PathBuf>,
    /// Folders the job created and the undo removed (they held nothing
    /// of the user's).
    pub dirs_removed: Vec<PathBuf>,
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
    pub kind: JobKind,
    /// The folder an organise job took its files from, when the journal
    /// names one.
    pub source_root: Option<PathBuf>,
    /// False for a journal this build cannot undo (written by a newer
    /// Coverly, or unreadable).
    pub undoable: bool,
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

/// A move, copy or extracted file an organise journal records, as resume
/// needs it. Lines are reported in journal order: an intent and its
/// completion are two records (the later one supersedes).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferRecord {
    /// The plan item it belongs to.
    pub item: Option<usize>,
    /// Absolute, inside the job's source root (the archive, for an
    /// extracted file).
    pub from: PathBuf,
    pub target: PathBuf,
    /// A copy (the source stays) rather than a move. False for an
    /// extracted file (what happens to its archive is an [`ArchiveRecord`]).
    pub copied: bool,
    /// The entry carries the target's fingerprint: the copy finished (a
    /// rename is journaled with it before it happens, so a move by rename
    /// is only done once the source is gone), or the extracted file was
    /// renamed into place.
    pub finished: bool,
    /// The target's fingerprint when `finished`.
    pub fp: Option<Fingerprint>,
    /// Set for a file extracted from the archive `from`.
    pub extracted: Option<ExtractedRecord>,
}

/// The archive entry behind an extracted [`TransferRecord`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtractedRecord {
    pub entry: String,
    /// The size and CRC-32 the archive lists.
    pub size: u64,
    pub crc32: Option<u32>,
    /// The job temp file (a bare name in `target`'s folder).
    pub temp: String,
    /// A later `rolled-back` line undid this intent.
    pub rolled_back: bool,
}

/// What happened to an archive after its files were extracted, one per
/// journal line, in order (the last line for an archive is its state).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchiveRecord {
    pub item: Option<usize>,
    /// The archive, absolute, inside the job's source root.
    pub from: PathBuf,
    pub step: ArchiveStep,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArchiveStep {
    /// About to go to the Trash; `fp` is the archive's when journaled.
    /// Unresolved while no later line follows it.
    TrashIntent { fp: Option<Fingerprint> },
    /// In the Trash, at `trash` when the OS said where.
    Trashed { trash: Option<PathBuf> },
    /// Left where it was.
    Kept,
}

/// The source root and the entries of job `job`'s journal (not undone).
fn job_entries(data_dir: &Path, job: &str) -> Result<(PathBuf, Vec<Line>), WriteError> {
    if !valid_job_id(job) {
        return Err(WriteError::Journal(format!("not a job id: {job}")));
    }
    let path = data_dir.join("jobs").join(format!("{job}.jsonl"));
    let (header, entries, _) = parse(&path)?;
    let Line::Header {
        root, source_root, ..
    } = header
    else {
        unreachable!("parse returns a header first")
    };
    Ok((source_root.unwrap_or(root), entries))
}

/// `from` made absolute against the source root (journals written by
/// hand or by a later format may keep it relative).
fn absolute_from(source_root: &Path, from: PathBuf) -> PathBuf {
    if from.is_absolute() {
        from
    } else {
        source_root.join(from)
    }
}

/// The moves, copies and extracted files journaled by job `job` (not
/// undone), in order.
pub fn job_transfers(data_dir: &Path, job: &str) -> Result<Vec<TransferRecord>, WriteError> {
    let (source_root, entries) = job_entries(data_dir, job)?;
    let mut out: Vec<TransferRecord> = Vec::new();
    for line in entries {
        let Line::Entry {
            target,
            op,
            fp,
            item,
            ..
        } = line
        else {
            continue;
        };
        let (from, copied, extracted) = match op {
            Op::Moved { from, .. } => (from, false, None),
            Op::Copied { from } => (from, true, None),
            Op::Extracted {
                from,
                entry,
                size,
                crc32,
                temp,
            } => (
                from,
                false,
                Some(ExtractedRecord {
                    entry,
                    size,
                    crc32,
                    temp,
                    rolled_back: false,
                }),
            ),
            Op::RolledBack { temp } => {
                // Its intent: the latest one naming the temp, not renamed.
                let intent = out.iter_mut().rev().find(|r| {
                    !r.finished && r.extracted.as_ref().is_some_and(|ex| ex.temp == temp)
                });
                if let Some(ex) = intent.and_then(|r| r.extracted.as_mut()) {
                    ex.rolled_back = true;
                }
                continue;
            }
            _ => continue,
        };
        out.push(TransferRecord {
            item,
            from: absolute_from(&source_root, from),
            target,
            copied,
            finished: fp.is_some(),
            fp,
            extracted,
        });
    }
    Ok(out)
}

/// The archive steps journaled by job `job` (not undone), in order.
pub fn job_archive_steps(data_dir: &Path, job: &str) -> Result<Vec<ArchiveRecord>, WriteError> {
    let (source_root, entries) = job_entries(data_dir, job)?;
    Ok(archive_steps(&source_root, entries))
}

fn archive_steps(source_root: &Path, entries: Vec<Line>) -> Vec<ArchiveRecord> {
    entries
        .into_iter()
        .filter_map(|line| {
            let Line::Entry { op, fp, item, .. } = line else {
                return None;
            };
            let (from, step) = match op {
                Op::ArchiveTrash { from } => (from, ArchiveStep::TrashIntent { fp }),
                Op::ArchiveTrashed { from, trash } => (from, ArchiveStep::Trashed { trash }),
                Op::ArchiveKept { from } => (from, ArchiveStep::Kept),
                _ => return None,
            };
            Some(ArchiveRecord {
                item,
                from: absolute_from(source_root, from),
                step,
            })
        })
        .collect()
}

/// Whether a journal sends an archive to the Trash with no line saying
/// where it went (a crash during the Trash call): only that journal can
/// still find the archive again, so it is never pruned.
fn has_unresolved_trash(path: &Path) -> bool {
    let Ok((
        Line::Header {
            root, source_root, ..
        },
        entries,
        _,
    )) = parse(path)
    else {
        return false;
    };
    let steps = archive_steps(&source_root.unwrap_or(root), entries);
    steps.iter().enumerate().any(|(i, s)| {
        matches!(s.step, ArchiveStep::TrashIntent { .. })
            && !steps[i + 1..].iter().any(|later| later.from == s.from)
    })
}

/// All journaled jobs, newest first.
pub fn list_jobs(data_dir: &Path) -> Result<Vec<JobSummary>, WriteError> {
    let mut out = Vec::new();
    for journal in journals(data_dir)? {
        let summary = match parse(&journal.path) {
            Ok((
                Line::Header {
                    root,
                    source_root,
                    kind,
                    created_at,
                    ..
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
                kind,
                source_root,
                undoable: entries_undoable(&entries),
            },
            _ => JobSummary {
                job: journal.job,
                root: PathBuf::new(),
                created_at: String::new(),
                entries: 0,
                complete: false,
                undone: journal.undone,
                kind: JobKind::default(),
                source_root: None,
                undoable: false,
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
    let Line::Header {
        root, source_root, ..
    } = header
    else {
        unreachable!("parse returns a header first")
    };
    let source_root = source_root.unwrap_or_else(|| root.clone());
    // Refuse before touching anything: undoing part of a journal from a
    // newer build would leave the library half reverted.
    if let Some(why) = undo_refusal(&entries) {
        return Err(WriteError::Journal(why.to_string()));
    }
    // An organise undo needs both volumes: with the card pulled every
    // target would look gone and the job would be marked undone for
    // nothing. Report it like a write to a missing volume (a pause).
    let organise = |pred: fn(&Op) -> bool| {
        entries
            .iter()
            .any(|e| matches!(e, Line::Entry { op, .. } if pred(op)))
    };
    if organise(undo_organize::is_organize_op) && !root.is_dir() {
        return Err(WriteError::Denied(root));
    }
    if organise(|op| matches!(op, Op::Moved { .. } | Op::Copied { .. })) && !source_root.is_dir() {
        return Err(WriteError::Denied(source_root));
    }
    let backups = data_dir.join("backups").join(&journal.job);
    let mut report = UndoReport {
        job: journal.job.clone(),
        removed: Vec::new(),
        restored: Vec::new(),
        skipped_modified: Vec::new(),
        skipped_occupied: Vec::new(),
        dirs_removed: Vec::new(),
    };
    let mut ctx = undo_organize::Ctx {
        root: &root,
        source_root: &source_root,
        data_dir,
        job: &journal.job,
        counter: 0,
        overrides: Vec::new(),
    };
    // A target replaced twice in one job (a gamelist written by a run and
    // its resume) shares one backup holding the original bytes: once the
    // last write is restored, the earlier replace entries are done too.
    let mut restored_here: std::collections::BTreeSet<PathBuf> = std::collections::BTreeSet::new();
    for (n, entry) in entries.into_iter().rev().enumerate() {
        let Line::Entry {
            target,
            op,
            sha1,
            fp,
            ..
        } = entry
        else {
            continue;
        };
        // Organise ops never read whole files unless a CRC must decide.
        if undo_organize::is_organize_op(&op) {
            undo_organize::undo_entry(&mut ctx, target, op, fp, &mut report)?;
            continue;
        }
        let target = ensure_within(&root, &target)?;
        if matches!(op, Op::Replaced { .. }) && restored_here.contains(&target) {
            continue;
        }
        // A fill entry without its checksum cannot prove the file is still
        // the one the job wrote: leave it.
        let Some(sha1) = sha1 else {
            report.skipped_modified.push(target);
            continue;
        };
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
                restored_here.insert(target.clone());
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
                restored_here.insert(target.clone());
                report.restored.push(target);
            }
            // Organise ops are undone above; unknown ones refused before.
            (
                Op::Moved { .. }
                | Op::Copied { .. }
                | Op::DirCreated
                | Op::OverrideMoved { .. }
                | Op::Extracted { .. }
                | Op::RolledBack { .. }
                | Op::ArchiveTrash { .. }
                | Op::ArchiveTrashed { .. }
                | Op::ArchiveKept { .. }
                | Op::Unknown,
                _,
            ) => unreachable!("handled or refused before reading the target"),
        }
    }
    undo_organize::restore_overrides(&mut ctx)?;
    let undone = journal
        .path
        .with_file_name(format!("{}.undone.jsonl", journal.job));
    fs::rename(&journal.path, &undone).map_err(|e| map_io(e, &journal.path))?;
    Ok(report)
}

/// The jobs newer than `job` that are not undone and wrote under its
/// root (or above it), or took files from there (a later organise of the
/// organised tree): undoing `job` alone would orphan what they wrote or
/// moved on (art and gamelists of ROMs an organise undo moves back, ROMs
/// already moved elsewhere). Newest first.
pub fn later_jobs(data_dir: &Path, job: &str) -> Result<Vec<JobSummary>, WriteError> {
    let jobs = list_jobs(data_dir)?;
    let this = jobs
        .iter()
        .find(|j| j.job == job)
        .ok_or(WriteError::NothingToUndo)?;
    let root = this.root.clone();
    Ok(jobs
        .into_iter()
        .filter(|j| {
            j.job.as_str() > job
                && !j.undone
                && !j.root.as_os_str().is_empty()
                && std::iter::once(&j.root)
                    .chain(j.source_root.as_ref())
                    .any(|p| p.starts_with(&root) || root.starts_with(p))
        })
        .collect())
}

/// Undoes every job of `jobs`, newest first (the order they must be
/// undone in). Every job must exist and not be undone yet, checked before
/// anything is touched; the first failure stops the rest.
pub fn undo_jobs_in_order(data_dir: &Path, jobs: &[String]) -> Result<Vec<UndoReport>, WriteError> {
    let mut ids: Vec<&str> = jobs.iter().map(String::as_str).collect();
    ids.sort_unstable_by(|a, b| b.cmp(a));
    ids.dedup();
    let known = journals(data_dir)?;
    if ids
        .iter()
        .any(|id| !valid_job_id(id) || !known.iter().any(|j| j.job == *id && !j.undone))
    {
        return Err(WriteError::NothingToUndo);
    }
    ids.into_iter().map(|id| undo_job(data_dir, id)).collect()
}

/// Days an organise journal is kept even beyond the newest `keep`.
pub const KEEP_ORGANIZE_DAYS: u32 = 90;

/// Prunes journals (and their backups and plans), counting fill and
/// organise jobs separately: [`prune_jobs_at`] with
/// [`KEEP_ORGANIZE_DAYS`] and the current time.
pub fn prune_jobs(data_dir: &Path, keep: usize) -> Result<(), WriteError> {
    prune_jobs_at(data_dir, keep, KEEP_ORGANIZE_DAYS, SystemTime::now())
}

/// Keeps the newest `keep` fill journals and the newest `keep` organise
/// journals, plus every organise journal younger than
/// `keep_organize_days` at `now` (its id gives its age); deletes the rest
/// with their `backups/<job>` and `jobs/<job>.plan.json`. A few fill runs
/// must never destroy the only way to put hundreds of moved ROMs back.
pub fn prune_jobs_at(
    data_dir: &Path,
    keep: usize,
    keep_organize_days: u32,
    now: SystemTime,
) -> Result<(), WriteError> {
    let max_age = Duration::from_secs(u64::from(keep_organize_days) * 86_400);
    let young = |job: &str| {
        parse_compact_utc(job)
            .is_some_and(|at| now.duration_since(at).map_or(true, |age| age < max_age))
    };
    let (mut fills, mut organises) = (0usize, 0usize);
    for journal in journals(data_dir)? {
        let kept = match header_kind(&journal.path) {
            JobKind::Fill => {
                fills += 1;
                fills <= keep
            }
            JobKind::Organize => {
                organises += 1;
                organises <= keep || young(&journal.job)
            }
        };
        if kept || has_unresolved_trash(&journal.path) {
            continue;
        }
        fs::remove_file(&journal.path).map_err(|e| map_io(e, &journal.path))?;
        let plan = data_dir
            .join("jobs")
            .join(format!("{}.plan.json", journal.job));
        remove_if_present(&plan, |p| fs::remove_file(p))?;
        remove_if_present(&data_dir.join("backups").join(&journal.job), |p| {
            fs::remove_dir_all(p)
        })?;
    }
    Ok(())
}

/// `remove(path)`, where a missing `path` is no error.
fn remove_if_present(
    path: &Path,
    remove: impl Fn(&Path) -> io::Result<()>,
) -> Result<(), WriteError> {
    match remove(path) {
        Err(e) if e.kind() != ErrorKind::NotFound => Err(map_io(e, path)),
        _ => Ok(()),
    }
}

/// The kind in a journal's header line; fill when it cannot be read.
fn header_kind(path: &Path) -> JobKind {
    use std::io::{BufRead, BufReader, Read};
    let Ok(file) = File::open(path) else {
        return JobKind::Fill;
    };
    let mut first = Vec::new();
    if BufReader::new(file.take(1 << 16))
        .read_until(b'\n', &mut first)
        .is_err()
    {
        return JobKind::Fill;
    }
    match serde_json::from_slice::<Line>(&first) {
        Ok(Line::Header { kind, .. }) => kind,
        _ => JobKind::Fill,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rename_across_volumes_maps_to_cross_device() {
        #[cfg(unix)]
        let e = io::Error::from_raw_os_error(18);
        #[cfg(windows)]
        let e = io::Error::from_raw_os_error(17);
        assert!(matches!(
            map_io(e, Path::new("/sd/x.gba")),
            WriteError::CrossDevice(p) if p == Path::new("/sd/x.gba")
        ));
    }

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
    fn a_journal_failure_after_a_replace_keeps_the_backup() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("root");
        let data = tmp.path().join("data");
        fs::create_dir_all(root.join("Imgs")).unwrap();
        let target = root.join("Imgs").join("a.png");
        fs::write(&target, b"ORIGINAL-USER-ART").unwrap();

        let mut writer = Writer::begin(&root, &data).unwrap();
        // A read-only handle: the entry append fails after the art is
        // already replaced (as on a full data disk).
        writer.journal = File::open(&writer.journal_path).unwrap();
        let journal_path = writer.journal_path.clone();
        let backups = writer.backups.clone();

        assert!(writer
            .write(&target, b"new art", OverwritePolicy::Backup)
            .is_err());
        assert_eq!(fs::read(&target).unwrap(), b"new art");
        assert!(writer.has_entries(), "the library changed");
        assert!(writer.discard().is_err(), "a changed job is never dropped");
        assert!(journal_path.exists());
        assert_eq!(
            fs::read(backups.join("Imgs").join("a.png")).unwrap(),
            b"ORIGINAL-USER-ART"
        );
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
