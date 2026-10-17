//! Moving and copying ROMs into the library for organise jobs (spec §9):
//! every change is journaled before it happens (write-ahead), nothing is
//! ever replaced, copies are streamed (never `fs::copy`, which carries
//! xattrs and makes `._*` files on FAT), keep the source's modification
//! time and are verified by re-reading them before a source is deleted.

use std::fs::{self, File, OpenOptions};
use std::io::{self, ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use super::noclobber::{is_cross_device, rename_noclobber_indexed, with_sharing_retries};
use super::{
    apple_double, ensure_within, find_twin, map_io, name_key, remove_apple_doubles,
    rename_noclobber, safe_file_name, same_volume, Fingerprint, Op, Via, WriteError, Writer,
};

/// What to do when the target name (or a case/NFC twin of it) is taken.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Collision {
    /// Use the next free `"{stem} ({n}).{ext}"`, from 2.
    Rename,
    /// Leave the source where it is.
    Skip,
}

/// Result of one [`Writer::move_file`] or [`Writer::copy_file`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub enum MoveOutcome {
    /// The file is now at `to` (`renamed`: under another name than asked).
    Moved { to: PathBuf, renamed: bool },
    /// A copy is at `to`; the source stays (asked for).
    Copied { to: PathBuf, renamed: bool },
    /// A move across volumes copied the file but could not delete the
    /// source: both exist.
    CopiedNotRemoved { to: PathBuf, renamed: bool },
    /// The target name is taken and the collision policy is
    /// [`Collision::Skip`].
    SkippedExists { existing: PathBuf },
}

/// The points of a move or copy where tests inject faults.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CopyStage {
    /// The same-volume rename of the source.
    Rename,
    /// The re-read of a finished copy.
    Verify,
    /// Deleting the source after a copy.
    Delete,
}

/// A fault hook: an error to return instead of doing `stage` on the path.
pub type CopyFault = fn(CopyStage, &Path) -> Option<io::Error>;

#[cfg(feature = "test-util")]
thread_local! {
    static COPY_FAULT: std::cell::Cell<Option<CopyFault>> = const { std::cell::Cell::new(None) };
}

/// Test hook: makes the moves and copies of the calling thread fail at the
/// stages `f` returns an error for. `None` clears it.
#[doc(hidden)]
#[cfg(feature = "test-util")]
pub fn set_copy_fault(f: Option<CopyFault>) {
    COPY_FAULT.with(|c| c.set(f));
}

pub(super) fn fault(stage: CopyStage, path: &Path) -> Option<io::Error> {
    #[cfg(feature = "test-util")]
    {
        COPY_FAULT.with(|c| c.get()).and_then(|f| f(stage, path))
    }
    #[cfg(not(feature = "test-util"))]
    {
        let _ = (stage, path);
        None
    }
}

/// Copy and verify buffer size.
const CHUNK: usize = 1 << 20;
/// Names tried after a collision before giving up.
const MAX_NAMES: u32 = 100;

pub(super) fn mtime_ns(t: SystemTime) -> i64 {
    match t.duration_since(UNIX_EPOCH) {
        Ok(d) => i64::try_from(d.as_nanos()).unwrap_or(i64::MAX),
        Err(e) => -i64::try_from(e.duration().as_nanos()).unwrap_or(i64::MAX),
    }
}

fn fingerprint(meta: &fs::Metadata, crc32: Option<u32>) -> Fingerprint {
    Fingerprint {
        size: meta.len(),
        mtime_ns: meta.modified().map_or(0, mtime_ns),
        crc32,
    }
}

/// `"{stem} ({n}).{ext}"` (no extension: `"{stem} ({n})"`).
fn numbered(name: &str, n: u32) -> String {
    match name.rfind('.').filter(|&i| i > 0) {
        Some(i) => format!("{} ({n}){}", &name[..i], &name[i..]),
        None => format!("{name} ({n})"),
    }
}

/// The first `numbered(name, n)` from `from_n` with no twin in `dir`.
fn next_free(dir: &Path, name: &str, from_n: u32) -> Result<(String, u32), WriteError> {
    for n in from_n..from_n + MAX_NAMES {
        let candidate = numbered(name, n);
        if find_twin(dir, &candidate)?.is_none() {
            return Ok((candidate, n));
        }
    }
    Err(WriteError::Io(io::Error::new(
        ErrorKind::AlreadyExists,
        format!("no free name for {name} in {}", dir.display()),
    )))
}

fn file_name_of(path: &Path) -> Result<&str, WriteError> {
    path.file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| WriteError::NameInvalid(path.display().to_string()))
}

/// What a move or a copy should do once the target is settled.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Move,
    Copy,
}

/// The checked inputs of a move or copy.
struct Prepared {
    from: PathBuf,
    parent: PathBuf,
    name: String,
    /// The name asked for; `renamed` compares against it.
    asked: String,
    /// The next `n` to try after a collision.
    next_n: u32,
    /// The caller's directory index vouches for the name: no listing.
    indexed: bool,
}

impl Writer {
    /// Creates `dir` (inside the root) and its missing parents, journaling
    /// each folder before creating it so undo can remove the ones the job
    /// made. Folders that already exist are not journaled.
    pub fn ensure_dir(&mut self, dir: &Path) -> Result<(), WriteError> {
        if dir == self.root {
            return Ok(());
        }
        ensure_within(&self.root, dir)?;
        let mut missing = Vec::new();
        let mut probe = Some(dir);
        // Only a NotFound folder is ours (see `create_dirs`).
        while let Some(p) = probe.filter(|p| {
            *p != self.root
                && fs::symlink_metadata(p).is_err_and(|e| e.kind() == ErrorKind::NotFound)
        }) {
            missing.push(p.to_path_buf());
            probe = p.parent();
        }
        missing.reverse();
        for folder in &missing {
            self.append_entry(folder.clone(), Op::DirCreated, None, None, None)?;
            match fs::create_dir(folder) {
                Ok(()) => self.changes += 1,
                // Appeared meanwhile: not ours, but the entry is harmless
                // (undo only removes empty folders).
                Err(e) if e.kind() == ErrorKind::AlreadyExists => {}
                Err(e) => return Err(map_io(e, folder)),
            }
        }
        remove_apple_doubles(&missing);
        Ok(())
    }

    /// Moves `from` (inside the source root) to `to` (inside the root):
    /// a no-clobber rename on one volume, else a verified copy and then the
    /// source deleted. A target that only differs from the source by case
    /// is a no-op.
    pub fn move_file(
        &mut self,
        from: &Path,
        to: &Path,
        collision: Collision,
        item: Option<usize>,
    ) -> Result<MoveOutcome, WriteError> {
        self.transfer(from, to, collision, item, Mode::Move, true)
    }

    /// [`Writer::move_file`] for a caller that keeps a directory index and
    /// has checked that `to`'s name (ignoring case) is free: the target
    /// folder is not listed again (once per file is O(n²) for thousands of
    /// files landing in one folder). The no-clobber rename and the
    /// exclusive create still refuse a name taken meanwhile.
    pub fn move_file_indexed(
        &mut self,
        from: &Path,
        to: &Path,
        collision: Collision,
        item: Option<usize>,
    ) -> Result<MoveOutcome, WriteError> {
        self.transfer(from, to, collision, item, Mode::Move, false)
    }

    /// Copies `from` (inside the source root) to `to` (inside the root),
    /// streamed and verified; the source stays.
    pub fn copy_file(
        &mut self,
        from: &Path,
        to: &Path,
        collision: Collision,
        item: Option<usize>,
    ) -> Result<MoveOutcome, WriteError> {
        self.transfer(from, to, collision, item, Mode::Copy, true)
    }

    /// [`Writer::copy_file`] without listing the target folder (see
    /// [`Writer::move_file_indexed`]).
    pub fn copy_file_indexed(
        &mut self,
        from: &Path,
        to: &Path,
        collision: Collision,
        item: Option<usize>,
    ) -> Result<MoveOutcome, WriteError> {
        self.transfer(from, to, collision, item, Mode::Copy, false)
    }

    /// Deletes this job's temp files (`.coverly-<job>-*.tmp`) and their
    /// AppleDouble sidecars directly in `dirs`, best effort: what a crash
    /// in the middle of a copy leaves behind.
    pub fn clean_temps(&self, dirs: &[PathBuf]) {
        clean_job_temps(&self.job, dirs);
    }

    fn transfer(
        &mut self,
        from: &Path,
        to: &Path,
        collision: Collision,
        item: Option<usize>,
        mode: Mode,
        list_target: bool,
    ) -> Result<MoveOutcome, WriteError> {
        let from = ensure_within(&self.source_root, from)?;
        let to = ensure_within(&self.root, to)?;
        let meta = fs::symlink_metadata(&from).map_err(|e| map_io(e, &from))?;
        if !meta.file_type().is_file() {
            return Err(WriteError::NotAFile(from));
        }
        let asked = file_name_of(&to)?.to_string();
        if safe_file_name(&asked)? != asked {
            return Err(WriteError::NameInvalid(asked));
        }
        let parent = to
            .parent()
            .ok_or_else(|| WriteError::OutsideRoot(to.clone()))?
            .to_path_buf();
        // Already there under another case: nothing to do (a case-only
        // rename would fail or be invisible on FAT/exFAT).
        if mode == Mode::Move
            && from.parent() == Some(parent.as_path())
            && name_key(file_name_of(&from)?) == name_key(&asked)
        {
            return Ok(MoveOutcome::Moved {
                to: from,
                renamed: false,
            });
        }
        self.ensure_dir(&parent)?;
        let mut prepared = Prepared {
            from,
            parent,
            name: asked.clone(),
            asked,
            next_n: 2,
            indexed: !list_target,
        };
        let twin = if list_target {
            find_twin(&prepared.parent, &prepared.name)?
        } else {
            None
        };
        if let Some(existing) = twin {
            // The twin is the source itself, reached through a folder that
            // only differs by case (`gba/x.gba` asked as `GBA/x.gba` on a
            // case-insensitive volume): already there.
            if mode == Mode::Move && same_entry(&prepared.from, &existing) {
                return Ok(MoveOutcome::Moved {
                    to: prepared.from,
                    renamed: false,
                });
            }
            if collision == Collision::Skip {
                return Ok(MoveOutcome::SkippedExists { existing });
            }
            self.take_next_name(&mut prepared)?;
        }
        if mode == Mode::Move && same_volume(&prepared.from, &prepared.parent) {
            match self.rename_into(&mut prepared, collision, &meta, item)? {
                Renamed::Done(outcome) => return Ok(outcome),
                Renamed::CrossDevice => {}
            }
        }
        self.copy_into(&mut prepared, collision, mode, item)
    }

    fn take_next_name(&self, p: &mut Prepared) -> Result<(), WriteError> {
        let (name, n) = next_free(&p.parent, &p.asked, p.next_n)?;
        p.name = name;
        p.next_n = n + 1;
        // The index was wrong about this folder: list it from now on.
        p.indexed = false;
        Ok(())
    }

    /// Step 5: journal, then rename without replacing; a name taken
    /// meanwhile moves on to the next free one.
    fn rename_into(
        &mut self,
        p: &mut Prepared,
        collision: Collision,
        meta: &fs::Metadata,
        item: Option<usize>,
    ) -> Result<Renamed, WriteError> {
        // A rename keeps the size and the modification time.
        let fp = fingerprint(meta, None);
        for _ in 0..MAX_NAMES {
            let target = p.parent.join(&p.name);
            self.append_entry(
                target.clone(),
                Op::Moved {
                    from: p.from.clone(),
                    via: Via::Rename,
                },
                None,
                Some(fp),
                item,
            )?;
            let result = with_sharing_retries(|| match fault(CopyStage::Rename, &p.from) {
                Some(e) => Err(e),
                None if p.indexed => rename_noclobber_indexed(&p.from, &target),
                None => rename_noclobber(&p.from, &target),
            });
            match result {
                Ok(()) => {
                    self.changes += 1;
                    return Ok(Renamed::Done(MoveOutcome::Moved {
                        renamed: p.name != p.asked,
                        to: target,
                    }));
                }
                Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                    if collision == Collision::Skip {
                        return Ok(Renamed::Done(MoveOutcome::SkippedExists {
                            existing: target,
                        }));
                    }
                    self.take_next_name(p)?;
                }
                Err(e) if is_cross_device(&e) => return Ok(Renamed::CrossDevice),
                Err(e) => return Err(map_io(e, &p.from)),
            }
        }
        Err(too_many_collisions(&p.parent, &p.asked))
    }

    /// Step 6: journal the intent, stream a verified copy, journal the
    /// result; a move then deletes the source.
    fn copy_into(
        &mut self,
        p: &mut Prepared,
        collision: Collision,
        mode: Mode,
        item: Option<usize>,
    ) -> Result<MoveOutcome, WriteError> {
        let source = p.from.clone();
        let op = || match mode {
            Mode::Move => Op::Moved {
                from: source.clone(),
                via: Via::Copy,
            },
            Mode::Copy => Op::Copied {
                from: source.clone(),
            },
        };
        for _ in 0..MAX_NAMES {
            let target = p.parent.join(&p.name);
            self.append_entry(target.clone(), op(), None, None, item)?;
            let fp = match self.stream_copy(&p.from, &target, !p.indexed) {
                Ok(fp) => fp,
                Err(WriteError::Io(e)) if e.kind() == ErrorKind::AlreadyExists => {
                    if collision == Collision::Skip {
                        return Ok(MoveOutcome::SkippedExists { existing: target });
                    }
                    self.take_next_name(p)?;
                    continue;
                }
                Err(e) => return Err(e),
            };
            self.changes += 1;
            self.append_entry(target.clone(), op(), None, Some(fp), item)?;
            let renamed = p.name != p.asked;
            if mode == Mode::Copy {
                return Ok(MoveOutcome::Copied {
                    to: target,
                    renamed,
                });
            }
            let deleted = with_sharing_retries(|| match fault(CopyStage::Delete, &p.from) {
                Some(e) => Err(e),
                None => fs::remove_file(&p.from),
            });
            return match deleted {
                Ok(()) => Ok(MoveOutcome::Moved {
                    to: target,
                    renamed,
                }),
                Err(_) => {
                    self.append_entry(
                        target.clone(),
                        Op::Copied {
                            from: p.from.clone(),
                        },
                        None,
                        Some(fp),
                        item,
                    )?;
                    Ok(MoveOutcome::CopiedNotRemoved {
                        to: target,
                        renamed,
                    })
                }
            };
        }
        Err(too_many_collisions(&p.parent, &p.asked))
    }

    fn stream_copy(
        &mut self,
        from: &Path,
        to: &Path,
        list_target: bool,
    ) -> Result<Fingerprint, WriteError> {
        let prefix = format!(".coverly-{}-", self.job);
        stream_copy(from, to, &prefix, &mut self.counter, list_target)
    }
}

/// Deletes the temp files of job `job` (`.coverly-<job>-<n>.tmp`) and
/// their AppleDouble sidecars directly in `dirs`, best effort.
pub(super) fn clean_job_temps(job: &str, dirs: &[PathBuf]) {
    let prefix = format!(".coverly-{job}-");
    for dir in dirs {
        let Ok(read) = fs::read_dir(dir) else {
            continue;
        };
        for entry in read.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let ours = name
                .strip_prefix(&prefix)
                .and_then(|rest| rest.strip_suffix(".tmp"))
                .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
            let path = entry.path();
            if ours && fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_file()) {
                let _ = fs::remove_file(&path);
                remove_apple_doubles(&[path]);
            }
        }
    }
}

/// Copies `from` to `to` through a temp file next to `to` named
/// `<tmp_prefix><n>.tmp` (`n` from `counter`): CRC-32 while copying,
/// fsync, the source's modification time, a no-clobber rename into place,
/// then a re-read that must give the same CRC. On any error nothing is
/// left at `to` or in the temp file. `AlreadyExists` means `to` was taken
/// meanwhile. `list_target: false` skips listing `to`'s folder for case
/// twins (the caller's directory index checked it).
pub(super) fn stream_copy(
    from: &Path,
    to: &Path,
    tmp_prefix: &str,
    counter: &mut u32,
    list_target: bool,
) -> Result<Fingerprint, WriteError> {
    let dir = to
        .parent()
        .ok_or_else(|| WriteError::OutsideRoot(to.to_path_buf()))?;
    let mut source = File::open(from).map_err(|e| map_io(e, from))?;
    let modified = source
        .metadata()
        .and_then(|m| m.modified())
        .map_err(|e| map_io(e, from))?;
    // A crash (or an earlier run of a resumed job) can leave a temp
    // file with the next name behind: skip past it.
    let (tmp, mut file) = loop {
        let tmp = dir.join(format!("{tmp_prefix}{counter}.tmp"));
        *counter = counter.wrapping_add(1);
        match OpenOptions::new().write(true).create_new(true).open(&tmp) {
            Ok(file) => break (tmp, file),
            Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(map_io(e, to)),
        }
    };
    let copied = (|| {
        let mut hasher = crc32fast::Hasher::new();
        let mut buf = vec![0u8; CHUNK];
        loop {
            let n = match source.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(e) => return Err(map_io(e, from)),
            };
            hasher.update(&buf[..n]);
            file.write_all(&buf[..n]).map_err(|e| map_io(e, to))?;
        }
        file.set_modified(modified).map_err(|e| map_io(e, to))?;
        file.sync_all().map_err(|e| map_io(e, to))?;
        Ok(hasher.finalize())
    })();
    drop(file);
    let crc = match copied.and_then(|crc| {
        let renamed = if list_target {
            rename_noclobber(&tmp, to)
        } else {
            rename_noclobber_indexed(&tmp, to)
        };
        renamed.map(|()| crc).map_err(|e| match e.kind() {
            ErrorKind::AlreadyExists => WriteError::Io(e),
            _ => map_io(e, to),
        })
    }) {
        Ok(crc) => crc,
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            remove_apple_doubles(&[tmp]);
            return Err(e);
        }
    };
    remove_apple_doubles(&[tmp]);
    // `to` is new: its sidecar (if the volume made one) is ours.
    remove_apple_doubles(&[to.to_path_buf()]);
    let verified = match fault(CopyStage::Verify, to) {
        Some(e) => Err(WriteError::Io(e)),
        None => crc_of(to).and_then(|again| {
            if again == crc {
                Ok(())
            } else {
                Err(WriteError::Io(io::Error::new(
                    ErrorKind::InvalidData,
                    "copy verification failed",
                )))
            }
        }),
    };
    if let Err(e) = verified {
        let _ = fs::remove_file(to);
        if let Some(sidecar) = apple_double(to) {
            let _ = fs::remove_file(sidecar);
        }
        return Err(e);
    }
    let meta = fs::metadata(to).map_err(|e| map_io(e, to))?;
    Ok(fingerprint(&meta, Some(crc)))
}

enum Renamed {
    Done(MoveOutcome),
    /// The source is on another volume after all: copy instead.
    CrossDevice,
}

fn too_many_collisions(dir: &Path, name: &str) -> WriteError {
    WriteError::Io(io::Error::new(
        ErrorKind::AlreadyExists,
        format!("no free name for {name} in {}", dir.display()),
    ))
}

/// Whether `a` and `b` name the same directory entry: the same file in
/// the same folder (a hard link elsewhere is another entry).
fn same_entry(a: &Path, b: &Path) -> bool {
    let (Some(pa), Some(pb)) = (a.parent(), b.parent()) else {
        return false;
    };
    let (Some(na), Some(nb)) = (
        a.file_name().and_then(|n| n.to_str()),
        b.file_name().and_then(|n| n.to_str()),
    ) else {
        return false;
    };
    name_key(na) == name_key(nb) && same_file(pa, pb) && same_file(a, b)
}

/// Whether two existing paths are the same file (or folder) on disk.
fn same_file(a: &Path, b: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        matches!(
            (fs::symlink_metadata(a), fs::symlink_metadata(b)),
            (Ok(x), Ok(y)) if x.dev() == y.dev() && x.ino() == y.ino()
        )
    }
    #[cfg(not(unix))]
    {
        matches!(
            (fs::canonicalize(a), fs::canonicalize(b)),
            (Ok(x), Ok(y)) if x == y
        )
    }
}

/// CRC-32 of the file at `path`, read in 1 MiB chunks.
pub(super) fn crc_of(path: &Path) -> Result<u32, WriteError> {
    let mut file = File::open(path).map_err(|e| map_io(e, path))?;
    let mut hasher = crc32fast::Hasher::new();
    let mut buf = vec![0u8; CHUNK];
    loop {
        match file.read(&mut buf) {
            Ok(0) => return Ok(hasher.finalize()),
            Ok(n) => hasher.update(&buf[..n]),
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(e) => return Err(map_io(e, path)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbered_names_keep_the_extension() {
        assert_eq!(numbered("Tetris.gb", 2), "Tetris (2).gb");
        assert_eq!(
            numbered("Tetris (JUE) [!].gb", 3),
            "Tetris (JUE) [!] (3).gb"
        );
        assert_eq!(numbered("README", 2), "README (2)");
        assert_eq!(numbered(".hidden", 2), ".hidden (2)");
    }

    #[test]
    fn mtimes_before_the_epoch_are_negative() {
        let t = UNIX_EPOCH - std::time::Duration::from_nanos(5);
        assert_eq!(mtime_ns(t), -5);
        assert_eq!(
            mtime_ns(UNIX_EPOCH + std::time::Duration::from_secs(1)),
            1_000_000_000
        );
    }
}
