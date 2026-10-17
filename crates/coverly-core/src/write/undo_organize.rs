//! Undo of the organise ops of a journal (spec §10): moved files go back
//! where they came from, copies and the folders the job created are
//! removed, and manual identities follow the files back. Nothing is ever
//! replaced: a source that is taken again keeps its file and the moved
//! copy stays (`skipped_occupied`); a target changed after the job stays
//! too (`skipped_modified`). Whole files are only read when a CRC must
//! decide (a changed mtime, or two files that may be the same copy).

use std::fs;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};

use super::noclobber::{is_cross_device, with_sharing_retries};
use super::transfer::{clean_job_temps, crc_of, fault, mtime_ns, stream_copy};
use super::{
    apple_double, create_dirs, ensure_within, find_twin, map_io, remove_apple_doubles,
    rename_noclobber, CopyStage, Fingerprint, Op, UndoReport, Via, WriteError,
};
use crate::identify::overrides::{Overrides, OVERRIDES_FILE};

/// How deep an emptied folder is searched for leftovers before it counts
/// as holding the user's things.
const LITTER_DEPTH: u32 = 8;

/// What one organise undo needs besides the entry.
pub(super) struct Ctx<'a> {
    pub root: &'a Path,
    pub source_root: &'a Path,
    pub data_dir: &'a Path,
    pub job: &'a str,
    /// Temp file counter of the copies back across volumes.
    pub counter: u32,
}

/// Whether `op` is one of the organise ops this module undoes.
pub(super) fn is_organize_op(op: &Op) -> bool {
    matches!(
        op,
        Op::Moved { .. } | Op::Copied { .. } | Op::DirCreated | Op::OverrideMoved { .. }
    )
}

/// Undoes one organise entry.
pub(super) fn undo_entry(
    ctx: &mut Ctx<'_>,
    target: PathBuf,
    op: Op,
    fp: Option<Fingerprint>,
    report: &mut UndoReport,
) -> Result<(), WriteError> {
    match op {
        Op::OverrideMoved { from, to } => restore_override(ctx.data_dir, &from, &to),
        Op::DirCreated => {
            remove_created_dir(ctx, &target, report);
            Ok(())
        }
        Op::Moved { from, via } => {
            let target = ensure_within(ctx.root, &target)?;
            let from = source_path(ctx.source_root, &from)?;
            match fp {
                Some(fp) => undo_move(ctx, &target, &from, via, &fp, report),
                None => undo_intent(ctx, &target, &from, true, report),
            }
        }
        Op::Copied { from } => {
            let target = ensure_within(ctx.root, &target)?;
            let from = source_path(ctx.source_root, &from)?;
            match fp {
                Some(fp) => undo_copy(ctx, &target, &fp, report),
                None => undo_intent(ctx, &target, &from, false, report),
            }
        }
        Op::Created | Op::Replaced { .. } | Op::Removed { .. } | Op::Unknown => {
            unreachable!("only organise ops reach here")
        }
    }
}

/// `from` as an absolute path inside the source root (journals written by
/// hand or by a later format may keep it relative to it).
fn source_path(source_root: &Path, from: &Path) -> Result<PathBuf, WriteError> {
    let from = if from.is_absolute() {
        from.to_path_buf()
    } else {
        source_root.join(from)
    };
    let rel = from
        .strip_prefix(source_root)
        .map_err(|_| WriteError::OutsideRoot(from.clone()))?;
    if rel.as_os_str().is_empty() || rel.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(WriteError::OutsideRoot(from));
    }
    // A symlink put where the source was is an occupant, not a way out.
    if fs::symlink_metadata(&from).is_ok_and(|m| m.file_type().is_symlink()) {
        return Ok(from);
    }
    ensure_within(source_root, &from)
}

/// The file now at `target` is still the one the job left there: same
/// size and modification time, or (mtime changed) the same CRC-32.
fn matches_fp(target: &Path, meta: &fs::Metadata, fp: &Fingerprint) -> Result<bool, WriteError> {
    if meta.len() != fp.size {
        return Ok(false);
    }
    if meta.modified().map(mtime_ns).ok() == Some(fp.mtime_ns) {
        return Ok(true);
    }
    match fp.crc32 {
        Some(crc) => Ok(crc_of(target)? == crc),
        None => Ok(false),
    }
}

/// Whatever takes the name `from` in its folder (any case or Unicode
/// spelling); `None` when the name, or the folder, is free.
fn occupant(from: &Path) -> Result<Option<PathBuf>, WriteError> {
    let (Some(parent), Some(name)) = (from.parent(), from.file_name().and_then(|n| n.to_str()))
    else {
        return Err(WriteError::NameInvalid(from.display().to_string()));
    };
    match fs::symlink_metadata(parent) {
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(map_io(e, parent)),
        Ok(m) if !m.is_dir() => return Ok(Some(parent.to_path_buf())),
        Ok(_) => {}
    }
    find_twin(parent, name)
}

/// Two regular files with the same size and CRC-32.
fn same_bytes(a: &Path, b: &Path) -> Result<bool, WriteError> {
    let (Ok(ma), Ok(mb)) = (fs::symlink_metadata(a), fs::symlink_metadata(b)) else {
        return Ok(false);
    };
    if !ma.file_type().is_file() || !mb.file_type().is_file() || ma.len() != mb.len() {
        return Ok(false);
    }
    Ok(crc_of(a)? == crc_of(b)?)
}

/// Deletes `target` (a file the job made) and its AppleDouble sidecar.
fn delete_ours(target: &Path) -> Result<(), WriteError> {
    with_sharing_retries(|| fs::remove_file(target)).map_err(|e| map_io(e, target))?;
    remove_apple_doubles(&[target.to_path_buf()]);
    Ok(())
}

/// The target is gone: only the temp files a crash may have left remain.
fn target_gone(ctx: &Ctx<'_>, target: &Path) {
    if let Some(dir) = target.parent() {
        clean_job_temps(ctx.job, &[dir.to_path_buf()]);
    }
}

/// `target`'s metadata; `None` (after cleaning temps) when it is gone.
fn target_meta(ctx: &Ctx<'_>, target: &Path) -> Result<Option<fs::Metadata>, WriteError> {
    match fs::symlink_metadata(target) {
        Ok(meta) => Ok(Some(meta)),
        Err(e) if e.kind() == ErrorKind::NotFound => {
            target_gone(ctx, target);
            Ok(None)
        }
        Err(e) => Err(map_io(e, target)),
    }
}

/// A finished move: put the file back at `from`.
fn undo_move(
    ctx: &mut Ctx<'_>,
    target: &Path,
    from: &Path,
    via: Via,
    fp: &Fingerprint,
    report: &mut UndoReport,
) -> Result<(), WriteError> {
    let Some(meta) = target_meta(ctx, target)? else {
        return Ok(());
    };
    if !meta.file_type().is_file() || !matches_fp(target, &meta, fp)? {
        report.skipped_modified.push(target.to_path_buf());
        return Ok(());
    }
    if let Some(occupant) = occupant(from)? {
        // A move across volumes copies, journals, then deletes the source:
        // a crash in between leaves both. Only then is the target a spare
        // copy; a rename never leaves two.
        if via == Via::Copy && same_bytes(&occupant, target)? {
            delete_ours(target)?;
            report.restored.push(from.to_path_buf());
        } else {
            report.skipped_occupied.push(target.to_path_buf());
        }
        return Ok(());
    }
    if put_back(ctx, target, from)? {
        report.restored.push(from.to_path_buf());
    } else {
        report.skipped_occupied.push(target.to_path_buf());
    }
    Ok(())
}

/// Moves `target` back to the free name `from`: a no-clobber rename, or
/// across volumes a verified copy and then the target deleted. `false`
/// when `from` was taken meanwhile (nothing changed).
fn put_back(ctx: &mut Ctx<'_>, target: &Path, from: &Path) -> Result<bool, WriteError> {
    let parent = from
        .parent()
        .ok_or_else(|| WriteError::OutsideRoot(from.to_path_buf()))?;
    remove_apple_doubles(&create_dirs(parent)?);
    let renamed = with_sharing_retries(|| match fault(CopyStage::Rename, target) {
        Some(e) => Err(e),
        None => rename_noclobber(target, from),
    });
    match renamed {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == ErrorKind::AlreadyExists => Ok(false),
        Err(e) if is_cross_device(&e) => {
            let prefix = format!(".coverly-undo-{}-", ctx.job);
            match stream_copy(target, from, &prefix, &mut ctx.counter) {
                Ok(_) => {}
                Err(WriteError::Io(e)) if e.kind() == ErrorKind::AlreadyExists => return Ok(false),
                Err(e) => return Err(e),
            }
            delete_ours(target)?;
            Ok(true)
        }
        Err(e) => Err(map_io(e, target)),
    }
}

/// An intent entry (journaled before a copy, never completed): a crash
/// after the copy finished leaves a target equal to the source, which is
/// the job's copy. Anything else at the target is not the job's.
fn undo_intent(
    ctx: &Ctx<'_>,
    target: &Path,
    from: &Path,
    moved: bool,
    report: &mut UndoReport,
) -> Result<(), WriteError> {
    let Some(meta) = target_meta(ctx, target)? else {
        return Ok(());
    };
    if !meta.file_type().is_file() {
        return Ok(());
    }
    let Some(occupant) = occupant(from)? else {
        return Ok(());
    };
    if same_bytes(&occupant, target)? {
        delete_ours(target)?;
        if moved {
            report.restored.push(from.to_path_buf());
        } else {
            report.removed.push(target.to_path_buf());
        }
    }
    Ok(())
}

/// A finished copy: remove it unless it changed.
fn undo_copy(
    ctx: &Ctx<'_>,
    target: &Path,
    fp: &Fingerprint,
    report: &mut UndoReport,
) -> Result<(), WriteError> {
    let Some(meta) = target_meta(ctx, target)? else {
        return Ok(());
    };
    if !meta.file_type().is_file() || !matches_fp(target, &meta, fp)? {
        report.skipped_modified.push(target.to_path_buf());
        return Ok(());
    }
    delete_ours(target)?;
    report.removed.push(target.to_path_buf());
    Ok(())
}

/// Files the system leaves in folders: Finder's `.DS_Store` and
/// AppleDouble `._*` files.
fn is_litter_name(name: &str) -> bool {
    name == ".DS_Store" || name.starts_with("._")
}

/// Whether `dir` holds nothing but litter and folders that themselves
/// hold nothing else (an emptied `Imgs/` left by an undone fill job).
fn only_litter(dir: &Path, depth: u32) -> bool {
    let Ok(read) = fs::read_dir(dir) else {
        return false;
    };
    for entry in read {
        let Ok(entry) = entry else {
            return false;
        };
        let Ok(kind) = entry.file_type() else {
            return false;
        };
        let name = entry.file_name();
        let litter = if kind.is_dir() {
            depth > 0 && only_litter(&entry.path(), depth - 1)
        } else {
            kind.is_file() && name.to_str().is_some_and(is_litter_name)
        };
        if !litter {
            return false;
        }
    }
    true
}

/// Deletes the litter (and emptied folders) in `dir`, best effort.
fn remove_litter(dir: &Path) {
    let Ok(read) = fs::read_dir(dir) else {
        return;
    };
    for entry in read.flatten() {
        let path = entry.path();
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => {
                remove_litter(&path);
                let _ = fs::remove_dir(&path);
            }
            Ok(kind) if kind.is_file() => {
                let _ = fs::remove_file(&path);
            }
            _ => {}
        }
    }
}

/// A folder the job created: removed when it holds nothing of the user's
/// (litter and emptied folders do not count); otherwise kept untouched.
fn remove_created_dir(ctx: &Ctx<'_>, dir: &Path, report: &mut UndoReport) {
    let Ok(dir) = ensure_within(ctx.root, dir) else {
        return;
    };
    if !fs::symlink_metadata(&dir).is_ok_and(|m| m.is_dir()) {
        return;
    }
    clean_job_temps(ctx.job, std::slice::from_ref(&dir));
    if !only_litter(&dir, LITTER_DEPTH) {
        return;
    }
    remove_litter(&dir);
    if fs::remove_dir(&dir).is_ok() {
        // The folder's own sidecar in its parent is the job's too.
        if let Some(sidecar) = apple_double(&dir) {
            let _ = fs::remove_file(sidecar);
        }
        report.dirs_removed.push(dir);
    }
}

/// Puts the manual identity the job rekeyed from `from` to `to` back
/// under `from`, unless `from` has one of its own by now.
fn restore_override(data_dir: &Path, from: &Path, to: &Path) -> Result<(), WriteError> {
    let path = data_dir.join(OVERRIDES_FILE);
    if fs::symlink_metadata(&path).is_err() {
        return Ok(());
    }
    let mut overrides = Overrides::load_for_update(&path);
    let Some(identity) = overrides.get(to).cloned() else {
        return Ok(());
    };
    if overrides.get(from).is_some() {
        return Ok(());
    }
    overrides.remove(to);
    overrides.set(from, identity);
    overrides.save(&path).map_err(|e| map_io(e, &path))
}
