//! Extracting an archive's entries into the library (archives spec §9.1).
//!
//! An `Extract` item is committed in phases so that no final file appears
//! until the whole item is known good. This module holds **phase A**:
//! decode every wanted entry to a job temp file in its target folder.
//!
//! 1. Every target folder is checked component by component below the
//!    root: each existing one must be a real directory (not a symlink, a
//!    Windows junction or another reparse point), else
//!    [`ExtractError::UnsafeTarget`] and nothing is written. Missing ones
//!    are created one level at a time, each journaled `DirCreated`.
//! 2. Leftover temps of this job in those folders are deleted.
//! 3. One [`ArchiveReader::stream_entries`] pass decodes the entries.
//! 4. Per entry, write-ahead: the `extracted` intent is journaled (and
//!    synced), the temp is created with `create_new` (plus `O_NOFOLLOW` on
//!    Unix, so a planted symlink is refused, never followed), the stream is
//!    copied through a [`VerifyingReader`] (size and CRC against the plan)
//!    with a 1 MiB buffer, the entry's modification time is set (a failure
//!    is ignored: FAT cannot store every date), the file is synced, then
//!    re-read from the medium where the OS allows it and its CRC compared.
//! 5. Sheets (≤ 64 KiB) are kept in memory for the phase B checks.
//!
//! A full volume (`ENOSPC`, `EDQUOT`, `ERROR_DISK_FULL`,
//! `ERROR_HANDLE_DISK_FULL`), on the target or on the journal's volume, is
//! [`ExtractError::DiskFull`]: the item is rolled back and the job pauses
//! ([`ExtractError::pause`]). Any other failure rolls the item back and
//! fails only that item. Cancellation is checked between entries and every
//! 16 MiB inside one. A rollback deletes this item's temps (only files this
//! run created with `create_new`) and journals `rolled-back` for every
//! intent; the archive is never touched.

use std::fs::{self, File, OpenOptions};
use std::io::{self, ErrorKind, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::SystemTime;

use crate::archive::{ArchiveError, ArchiveReader, Budget, Entry, EntryKind, VerifyingReader};
use crate::disc::sheets::MAX_SHEET_BYTES;
use crate::organize::apply::user_available;
use crate::organize::plan::ExtractEntry;
use crate::progress::PauseKind;
use crate::write::{remove_apple_doubles, ExtractedFile, WriteError, Writer};

/// Copy and re-read buffer.
pub const CHUNK: usize = 1 << 20;
/// Cancellation is checked at least this often inside one entry.
pub const CANCEL_EVERY: u64 = 16 << 20;
/// Space a journal needs to go on: what a full journal volume reports as
/// needed.
pub const JOURNAL_HEADROOM: u64 = 1 << 20;

/// Sheet extensions kept in memory as they pass (phase B parses them).
const SHEET_EXTS: [&str; 4] = ["cue", "gdi", "ccd", "m3u"];

/// One `Extract` item's phase A.
pub struct ExtractRequest<'a> {
    /// The plan item id (journaled with every line).
    pub item: usize,
    /// The archive (absolute, inside the job's source root).
    pub archive: &'a Path,
    /// The plan's entries; each `target` is relative to the writer's root.
    pub entries: &'a [ExtractEntry],
    /// Checked between entries and every [`CANCEL_EVERY`] bytes, and
    /// handed to the decode budget so the backend stops too while it
    /// decodes bytes this item does not want (a solid block's other
    /// entries) or waits for decoder memory.
    pub cancel: &'a Arc<AtomicBool>,
    /// Called with the bytes of this item written to temps so far.
    pub progress: &'a dyn Fn(u64),
}

/// One entry decoded to its temp and verified.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StagedEntry {
    /// The plan entry.
    pub entry: ExtractEntry,
    /// The final path (absolute).
    pub target: PathBuf,
    /// The temp file next to it (absolute).
    pub temp: PathBuf,
    /// What the intent line journaled.
    pub file: ExtractedFile,
    /// CRC-32 of the decoded bytes, equal to the re-read temp's.
    pub crc32: u32,
    /// The bytes of a sheet (≤ 64 KiB), kept for phase B.
    pub bytes: Option<Vec<u8>>,
}

/// What phase A left: every entry of the item in a verified temp, in the
/// order they were decoded. Nothing final exists yet.
#[derive(Debug, Default)]
pub struct PhaseA {
    pub staged: Vec<StagedEntry>,
}

/// Why phase A stopped. The item was rolled back in every case.
#[derive(Debug, thiserror::Error)]
pub enum ExtractError {
    /// A target folder (or the temp name) is a symlink, a junction, a
    /// reparse point or not a folder: nothing is written through it.
    #[error("unsafe target: {}", path.display())]
    UnsafeTarget { path: PathBuf },
    /// A volume filled up: `needed` bytes to write the item again, `free`
    /// on that volume. The job pauses.
    #[error("disk full: {needed} bytes needed, {free} free")]
    DiskFull { needed: u64, free: u64 },
    #[error("cancelled")]
    Cancelled,
    /// The archive no longer lists an entry as the plan does.
    #[error("archive entry changed since the plan: {entry}")]
    ListingChanged { entry: String },
    /// The decode pass ended without reaching this entry.
    #[error("archive entry not decoded: {entry}")]
    EntryMissing { entry: String },
    /// The temp read back differs from the decoded bytes.
    #[error("the extracted file reads back differently: {entry}")]
    RereadMismatch { entry: String },
    /// Damaged data, a CRC or size mismatch, a tool failure…
    #[error(transparent)]
    Archive(ArchiveError),
    #[error(transparent)]
    Write(WriteError),
}

impl ExtractError {
    /// The pause this failure puts the job in: only a full volume (the
    /// next items would fail the same way).
    pub fn pause(&self) -> Option<PauseKind> {
        match *self {
            ExtractError::DiskFull { needed, free } => Some(PauseKind::DiskFull { needed, free }),
            _ => None,
        }
    }
}

/// Test fault points of an extraction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    /// Temp writes of the calling thread fail with `StorageFull` once
    /// this many bytes were written to temps (counted per item).
    StorageFullAfter(u64),
    /// The re-read of every temp sees other bytes.
    RereadMismatch,
    /// Setting a temp's modification time fails.
    SetModified,
}

#[cfg(feature = "test-util")]
thread_local! {
    static WRITE_FAULT: std::cell::Cell<Option<Fault>> = const { std::cell::Cell::new(None) };
}

/// Test hook: injects `f` into the extractions of the calling thread.
/// `None` clears it.
#[doc(hidden)]
#[cfg(feature = "test-util")]
pub fn set_write_fault(f: Option<Fault>) {
    WRITE_FAULT.with(|c| c.set(f));
}

fn fault() -> Option<Fault> {
    #[cfg(feature = "test-util")]
    {
        WRITE_FAULT.with(std::cell::Cell::get)
    }
    #[cfg(not(feature = "test-util"))]
    {
        None
    }
}

/// An intent journaled by this item, and whether its temp was created.
struct Pending {
    /// The archive entry index.
    index: usize,
    target: PathBuf,
    temp: PathBuf,
    name: String,
    created: bool,
}

/// Phase A of one `Extract` item (see the module docs). On error the item
/// is already rolled back: no temp of it is left and every intent has its
/// `rolled-back` line (best effort when the journal itself is full).
pub fn extract_to_temps(
    writer: &mut Writer,
    reader: &mut dyn ArchiveReader,
    req: &ExtractRequest,
) -> Result<PhaseA, ExtractError> {
    let root = writer.root().to_path_buf();
    let total: u64 = req.entries.iter().map(|e| e.size).sum();
    let mut targets = Vec::with_capacity(req.entries.len());
    for e in req.entries {
        targets.push(target_path(&root, &e.target)?);
    }
    let mut dirs: Vec<PathBuf> = Vec::new();
    for t in &targets {
        let dir = t
            .parent()
            .ok_or_else(|| ExtractError::UnsafeTarget { path: t.clone() })?;
        if !dirs.iter().any(|d| d == dir) {
            dirs.push(dir.to_path_buf());
        }
    }
    for dir in &dirs {
        prepare_dir(writer, &root, dir, total)?;
    }
    writer.clean_temps(&dirs);
    check_listing(reader.entries(), req.entries)?;
    if req.cancel.load(Ordering::Relaxed) {
        return Err(ExtractError::Cancelled);
    }

    let mut wanted: Vec<usize> = req.entries.iter().map(|e| e.index).collect();
    wanted.sort_unstable();
    let mut state = Run {
        writer,
        req,
        targets: &targets,
        total,
        written: 0,
        pending: Vec::new(),
        staged: Vec::new(),
        failure: None,
    };
    let mut budget = Budget::apply().with_cancel(Arc::clone(req.cancel));
    let streamed = reader.stream_entries(&wanted, &mut budget, &mut |entry, stream| {
        match state.entry(entry, stream) {
            Ok(()) => Ok(()),
            Err(e) => {
                state.failure = Some(e);
                // A stop signal only: the failure above is reported.
                Err(ArchiveError::Cancelled)
            }
        }
    });
    let result = match (state.failure.take(), streamed) {
        (Some(e), _) => Err(e),
        (None, Err(ArchiveError::Cancelled)) => Err(ExtractError::Cancelled),
        (None, Err(e)) => Err(ExtractError::Archive(e)),
        (None, Ok(())) => match req
            .entries
            .iter()
            .find(|e| !state.staged.iter().any(|s| s.entry.index == e.index))
        {
            Some(missing) => Err(ExtractError::EntryMissing {
                entry: missing.name.clone(),
            }),
            None => Ok(()),
        },
    };
    match result {
        Ok(()) => Ok(PhaseA {
            staged: state.staged,
        }),
        Err(e) => {
            roll_back_pending(state.writer, req.item, &state.pending);
            Err(e)
        }
    }
}

/// Rolls back staged entries (phase B failure, or a cancel after phase
/// A): deletes their temps and journals `rolled-back` for each, best
/// effort. Nothing final exists yet, so nothing else is touched.
pub fn roll_back(writer: &mut Writer, item: usize, staged: &[StagedEntry]) {
    let pending: Vec<Pending> = staged
        .iter()
        .map(|s| Pending {
            index: s.entry.index,
            target: s.target.clone(),
            temp: s.temp.clone(),
            name: s.file.temp.clone(),
            created: true,
        })
        .collect();
    roll_back_pending(writer, item, &pending);
}

fn roll_back_pending(writer: &mut Writer, item: usize, pending: &[Pending]) {
    for p in pending {
        if p.created && fs::symlink_metadata(&p.temp).is_ok_and(|m| m.file_type().is_file()) {
            let _ = fs::remove_file(&p.temp);
            remove_apple_doubles(std::slice::from_ref(&p.temp));
        }
        let _ = writer.rolled_back(&p.target, &p.name, Some(item));
    }
}

/// `root/rel` for a plan target: relative, with plain components only.
fn target_path(root: &Path, rel: &Path) -> Result<PathBuf, ExtractError> {
    let plain = rel.components().all(|c| matches!(c, Component::Normal(_)));
    if !plain || rel.as_os_str().is_empty() {
        return Err(ExtractError::UnsafeTarget {
            path: root.join(rel),
        });
    }
    Ok(root.join(rel))
}

/// Checks every existing folder from below `root` down to `dir`, creates
/// the missing ones (journaled, one level at a time), then checks again:
/// a folder swapped for a link meanwhile is still refused.
fn prepare_dir(
    writer: &mut Writer,
    root: &Path,
    dir: &Path,
    needed: u64,
) -> Result<(), ExtractError> {
    check_components(root, dir)?;
    writer.ensure_dir(dir).map_err(|e| {
        let journal = writer.journal_path().to_path_buf();
        match classify_write(e, dir, needed) {
            // `ensure_dir` journals each folder before making it: a full
            // volume is the journal's when the library still has room.
            ExtractError::DiskFull { free, .. } if free >= needed => {
                classify_journal(WriteError::DiskFull, &journal)
            }
            other => other,
        }
    })?;
    check_components(root, dir)
}

fn check_components(root: &Path, dir: &Path) -> Result<(), ExtractError> {
    let rel = dir
        .strip_prefix(root)
        .map_err(|_| ExtractError::UnsafeTarget {
            path: dir.to_path_buf(),
        })?;
    let mut at = root.to_path_buf();
    for c in rel.components() {
        at.push(c);
        match fs::symlink_metadata(&at) {
            Ok(meta) if is_real_dir(&meta) => {}
            Ok(_) => return Err(ExtractError::UnsafeTarget { path: at }),
            // Missing from here on: created by `ensure_dir`.
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(ExtractError::Write(WriteError::Io(e))),
        }
    }
    Ok(())
}

/// A directory that is not a symlink, a junction or another reparse point.
fn is_real_dir(meta: &fs::Metadata) -> bool {
    let ft = meta.file_type();
    if ft.is_symlink() || !ft.is_dir() {
        return false;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return false;
        }
    }
    true
}

/// The reader still lists every plan entry as a file of the same name and
/// size.
fn check_listing(listing: &[Entry], wanted: &[ExtractEntry]) -> Result<(), ExtractError> {
    for (i, w) in wanted.iter().enumerate() {
        let same = listing
            .iter()
            .find(|e| e.index == w.index)
            .is_some_and(|e| {
                e.kind == EntryKind::File
                    && e.unsafe_name.is_none()
                    && e.size == w.size
                    && e.name.as_str() == w.name
                    && (w.crc32.is_none() || e.crc32.is_none() || e.crc32 == w.crc32)
            });
        let twice = wanted[..i].iter().any(|o| o.index == w.index);
        if !same || twice {
            return Err(ExtractError::ListingChanged {
                entry: w.name.clone(),
            });
        }
    }
    Ok(())
}

/// The state of one decode pass.
struct Run<'r, 'a> {
    writer: &'r mut Writer,
    req: &'r ExtractRequest<'a>,
    targets: &'r [PathBuf],
    total: u64,
    /// Bytes of this item written to temps so far.
    written: u64,
    pending: Vec<Pending>,
    staged: Vec<StagedEntry>,
    failure: Option<ExtractError>,
}

impl Run<'_, '_> {
    fn entry(&mut self, entry: &Entry, stream: &mut dyn Read) -> Result<(), ExtractError> {
        if self.req.cancel.load(Ordering::Relaxed) {
            return Err(ExtractError::Cancelled);
        }
        let Some(pos) = self.req.entries.iter().position(|e| e.index == entry.index) else {
            // Not ours (a backend handing out more than asked): skipped.
            return Ok(());
        };
        if self.pending.iter().any(|p| p.index == entry.index) {
            // A backend handing one entry out twice: never staged twice.
            return Err(ExtractError::Archive(ArchiveError::Damaged {
                detail: format!("entry {} streamed twice", entry.index),
            }));
        }
        let want = self.req.entries[pos].clone();
        let target = self.targets[pos].clone();
        let dir = target.parent().unwrap_or(&target).to_path_buf();
        let name = self.writer.extract_temp();
        let temp = dir.join(&name);
        let file = ExtractedFile {
            from: self.req.archive.to_path_buf(),
            entry: want.name.clone(),
            size: want.size,
            crc32: want.crc32,
            temp: name.clone(),
        };
        self.writer
            .extract_intent(&target, &file, Some(self.req.item))
            .map_err(|e| classify_journal(e, self.writer.journal_path()))?;
        self.pending.push(Pending {
            index: want.index,
            target: target.clone(),
            temp: temp.clone(),
            name,
            created: false,
        });
        let mut out = match create_temp(&temp) {
            Ok(f) => f,
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                // Our own leftovers were cleaned: anything here is foreign
                // (a planted symlink), never written through or deleted.
                return Err(ExtractError::UnsafeTarget { path: temp });
            }
            Err(e) => return Err(classify_io(e, &dir, self.total)),
        };
        if let Some(p) = self.pending.last_mut() {
            p.created = true;
        }
        let keep = is_sheet(&want.name) && want.size <= MAX_SHEET_BYTES as u64;
        let mut kept = keep.then(|| Vec::with_capacity(want.size as usize));
        let mut verify = VerifyingReader::new(stream, want.size, want.crc32);
        let mut buf = vec![0u8; CHUNK];
        let mut since_check = 0u64;
        loop {
            let n = match verify.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(e) => {
                    return Err(match ArchiveError::from(e) {
                        ArchiveError::Cancelled => ExtractError::Cancelled,
                        other => ExtractError::Archive(other),
                    })
                }
            };
            self.write_chunk(&mut out, &buf[..n], &dir)?;
            if let Some(k) = kept.as_mut() {
                k.extend_from_slice(&buf[..n]);
            }
            (self.req.progress)(self.written);
            since_check += n as u64;
            if since_check >= CANCEL_EVERY {
                since_check = 0;
                if self.req.cancel.load(Ordering::Relaxed) {
                    return Err(ExtractError::Cancelled);
                }
            }
        }
        let Some(crc32) = verify.computed_crc32() else {
            // Not reached: EOF without a verified size reports an error.
            return Err(ExtractError::Archive(ArchiveError::Damaged {
                detail: "size".into(),
            }));
        };
        if let Some(mtime) = entry.mtime {
            // FAT cannot store years before 1980 or after 2107, and a
            // read-only attribute refuses it: never fails the item.
            let _ = set_modified(&out, mtime);
        }
        out.sync_all()
            .map_err(|e| classify_io(e, &dir, self.total))?;
        drop(out);
        let again = reread_crc(&temp).map_err(|e| classify_io(e, &dir, self.total))?;
        if again != Some((want.size, crc32)) {
            return Err(ExtractError::RereadMismatch { entry: want.name });
        }
        self.staged.push(StagedEntry {
            entry: want,
            target,
            temp,
            file,
            crc32,
            bytes: kept,
        });
        Ok(())
    }

    fn write_chunk(
        &mut self,
        out: &mut File,
        chunk: &[u8],
        dir: &Path,
    ) -> Result<(), ExtractError> {
        let allowed = match fault() {
            Some(Fault::StorageFullAfter(limit)) => {
                let room = limit.saturating_sub(self.written);
                usize::try_from(room).unwrap_or(usize::MAX).min(chunk.len())
            }
            _ => chunk.len(),
        };
        let result = out.write_all(&chunk[..allowed]).and_then(|()| {
            if allowed < chunk.len() {
                Err(ErrorKind::StorageFull.into())
            } else {
                Ok(())
            }
        });
        self.written += allowed as u64;
        result.map_err(|e| classify_io(e, dir, self.total))
    }
}

fn is_sheet(name: &str) -> bool {
    name.rsplit_once('.')
        .is_some_and(|(_, ext)| SHEET_EXTS.iter().any(|s| s.eq_ignore_ascii_case(ext)))
}

/// `create_new` (`O_CREAT | O_EXCL`, which refuses any existing name, a
/// dangling symlink included), plus `O_NOFOLLOW` on Unix.
fn create_temp(path: &Path) -> io::Result<File> {
    let mut opts = OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.custom_flags(libc::O_NOFOLLOW);
    }
    opts.open(path)
}

fn set_modified(file: &File, mtime: SystemTime) -> io::Result<()> {
    if fault() == Some(Fault::SetModified) {
        return Err(io::Error::other("set_modified fault"));
    }
    file.set_modified(mtime)
}

/// Size and CRC-32 of the temp read back, past the page cache where the
/// OS allows it (`F_NOCACHE` on macOS, `POSIX_FADV_DONTNEED` on Linux; a
/// normal read elsewhere, which may hit the cache).
fn reread_crc(path: &Path) -> io::Result<Option<(u64, u32)>> {
    let mut opts = OpenOptions::new();
    opts.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = opts.open(path)?;
    bypass_cache(&file);
    let mut hasher = crc32fast::Hasher::new();
    let mut buf = vec![0u8; CHUNK];
    let mut size = 0u64;
    loop {
        match file.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                hasher.update(&buf[..n]);
                size += n as u64;
            }
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    let mut crc = hasher.finalize();
    if fault() == Some(Fault::RereadMismatch) {
        crc ^= 1;
    }
    Ok(Some((size, crc)))
}

#[cfg(target_os = "macos")]
fn bypass_cache(file: &File) {
    use std::os::unix::io::AsRawFd;
    // SAFETY: `fcntl` on an open descriptor with an integer argument.
    unsafe {
        libc::fcntl(file.as_raw_fd(), libc::F_NOCACHE, 1);
    }
}

#[cfg(target_os = "linux")]
fn bypass_cache(file: &File) {
    use std::os::unix::io::AsRawFd;
    // SAFETY: `posix_fadvise` on an open descriptor; the data was synced,
    // so dropping its clean pages makes the read go to the medium.
    unsafe {
        libc::posix_fadvise(file.as_raw_fd(), 0, 0, libc::POSIX_FADV_DONTNEED);
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn bypass_cache(_: &File) {}

/// Whether `e` says the volume (or the user's quota) is full.
pub fn is_disk_full(e: &io::Error) -> bool {
    if matches!(e.kind(), ErrorKind::StorageFull | ErrorKind::QuotaExceeded) {
        // A spent decode budget is `QuotaExceeded` too (AP-8): never a
        // full disk.
        return !crate::archive::is_budget_spent(e);
    }
    match e.raw_os_error() {
        #[cfg(unix)]
        Some(code) => code == libc::ENOSPC || code == libc::EDQUOT,
        // ERROR_HANDLE_DISK_FULL, ERROR_DISK_FULL.
        #[cfg(windows)]
        Some(code) => code == 39 || code == 112,
        _ => false,
    }
}

fn disk_full(dir: &Path, needed: u64) -> ExtractError {
    ExtractError::DiskFull {
        needed,
        free: user_available(dir).unwrap_or(0),
    }
}

fn classify_io(e: io::Error, dir: &Path, needed: u64) -> ExtractError {
    if is_disk_full(&e) {
        disk_full(dir, needed)
    } else {
        ExtractError::Write(WriteError::Io(e))
    }
}

fn classify_write(e: WriteError, dir: &Path, needed: u64) -> ExtractError {
    match e {
        WriteError::DiskFull => disk_full(dir, needed),
        WriteError::Io(e) => classify_io(e, dir, needed),
        other => ExtractError::Write(other),
    }
}

/// A journal append failed: a full journal volume pauses like a full
/// target, reporting that volume.
fn classify_journal(e: WriteError, journal: &Path) -> ExtractError {
    let dir = journal.parent().unwrap_or(journal);
    classify_write(e, dir, JOURNAL_HEADROOM)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sheets_are_kept_by_extension() {
        assert!(is_sheet("Game.cue"));
        assert!(is_sheet("disc/Game.GDI"));
        assert!(!is_sheet("Game (Track 1).bin"));
        assert!(!is_sheet("cue"));
    }

    #[test]
    fn plan_targets_must_be_plain_relative_paths() {
        let root = Path::new("/lib");
        assert!(target_path(root, Path::new("PS/Game.cue")).is_ok());
        assert!(target_path(root, Path::new("../x.bin")).is_err());
        assert!(target_path(root, Path::new("PS/./x.bin")).is_ok());
        assert!(target_path(root, Path::new("")).is_err());
    }

    #[test]
    fn full_volumes_are_recognised_and_budgets_are_not() {
        assert!(is_disk_full(&io::Error::from(ErrorKind::StorageFull)));
        assert!(is_disk_full(&io::Error::from(ErrorKind::QuotaExceeded)));
        let spent: io::Error = ArchiveError::BudgetSpent {
            what: crate::archive::BudgetKind::DecodedBytes,
        }
        .into();
        assert!(!is_disk_full(&spent));
        assert!(!is_disk_full(&io::Error::from(ErrorKind::PermissionDenied)));
    }
}
