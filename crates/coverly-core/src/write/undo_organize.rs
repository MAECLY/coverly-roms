//! Undo of the organise ops of a journal (spec §10): moved files go back
//! where they came from, copies and the folders the job created are
//! removed, and manual identities follow the files back. Nothing is ever
//! replaced: a source that is taken again keeps its file and the moved
//! copy stays (`skipped_occupied`); a target changed after the job stays
//! too (`skipped_modified`). Whole files are only read when a CRC must
//! decide (a changed mtime, or two files that may be the same copy).
//!
//! Extracted items (archives spec §9.2): every archive the job sent to
//! the Trash is brought back first ([`restore_archives`]); an archive
//! that cannot come back keeps its extracted files (the only copy of the
//! game left). Extracted files are then deleted when they still match
//! the job's fingerprint, or, for an intent never completed, when their
//! size and a full CRC-32 re-read equal what the archive lists.

use std::collections::HashSet;
use std::fs;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};

use super::noclobber::{is_cross_device, with_sharing_retries};
use super::transfer::{clean_job_temps, crc_of, fault, mtime_ns, same_entry, stream_copy};
use super::trash::TrashBackend;
use super::{
    absolute_from, apple_double, create_dirs, ensure_within, find_twin, is_job_temp, map_io,
    remove_apple_doubles, rename_noclobber, CopyStage, Fingerprint, Line, Op, UndoReport, Via,
    WriteError,
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
    /// Rekeyed manual identities (`from`, `to`), restored once every file
    /// had its chance to go back (see [`restore_overrides`]).
    pub overrides: Vec<(PathBuf, PathBuf)>,
    /// Archives that could not be brought back, and their plan items:
    /// what was extracted from them stays.
    pub lost_archives: HashSet<PathBuf>,
    pub lost_items: HashSet<usize>,
    /// Extraction temps whose intent is settled by a later line (a
    /// completion or a rollback), so the intent itself decides nothing.
    pub settled_temps: HashSet<String>,
}

/// Whether `op` is one of the organise ops this module undoes.
pub(super) fn is_organize_op(op: &Op) -> bool {
    matches!(
        op,
        Op::Moved { .. }
            | Op::Copied { .. }
            | Op::DirCreated
            | Op::OverrideMoved { .. }
            | Op::Extracted { .. }
            | Op::RolledBack { .. }
            | Op::ArchiveTrash { .. }
            | Op::ArchiveTrashed { .. }
            | Op::ArchiveKept { .. }
    )
}

/// Undoes one organise entry.
pub(super) fn undo_entry(
    ctx: &mut Ctx<'_>,
    target: PathBuf,
    op: Op,
    fp: Option<Fingerprint>,
    item: Option<usize>,
    report: &mut UndoReport,
) -> Result<(), WriteError> {
    match op {
        Op::OverrideMoved { from, to } => {
            ctx.overrides.push((from, to));
            Ok(())
        }
        Op::DirCreated => {
            remove_created_dir(ctx, &target, report);
            Ok(())
        }
        Op::Moved { from, via } => {
            let target = within_either(ctx, &target)?;
            let from = source_path(ctx.source_root, &from)?;
            match fp {
                Some(fp) => undo_move(ctx, &target, &from, via, &fp, report),
                None => undo_intent(ctx, &target, &from, true, report),
            }
        }
        Op::Copied { from } => {
            let target = within_either(ctx, &target)?;
            let from = source_path(ctx.source_root, &from)?;
            match fp {
                Some(fp) => undo_copy(ctx, &target, &fp, report),
                None => undo_intent(ctx, &target, &from, false, report),
            }
        }
        Op::Created | Op::Replaced { .. } | Op::Removed { .. } | Op::Unknown => {
            unreachable!("only organise ops reach here")
        }
        Op::Extracted {
            from,
            size,
            crc32,
            temp,
            ..
        } => {
            let target = ensure_within(ctx.root, &target)?;
            let from = absolute_from(ctx.source_root, from);
            if ctx.lost_archives.contains(&from)
                || item.is_some_and(|i| ctx.lost_items.contains(&i))
            {
                return Ok(());
            }
            match fp {
                Some(fp) => {
                    ctx.settled_temps.insert(temp);
                    undo_copy(ctx, &target, &fp, report)
                }
                None => {
                    if !ctx.settled_temps.contains(&temp) {
                        undo_extract_intent(ctx, &target, size, crc32, report)?;
                    }
                    remove_job_temp(ctx, &target, &temp);
                    Ok(())
                }
            }
        }
        Op::RolledBack { temp } => {
            let target = ensure_within(ctx.root, &target)?;
            remove_job_temp(ctx, &target, &temp);
            ctx.settled_temps.insert(temp);
            Ok(())
        }
        // Brought back before any entry is undone (`restore_archives`).
        Op::ArchiveTrash { .. } | Op::ArchiveTrashed { .. } | Op::ArchiveKept { .. } => Ok(()),
    }
}

/// `target` inside the job's root, or inside its source root: a pile copy
/// the duplicates step set aside stays on the pile's side (moves and the
/// folders they created).
fn within_either(ctx: &Ctx<'_>, target: &Path) -> Result<PathBuf, WriteError> {
    ensure_within(ctx.root, target).or_else(|e| match ensure_within(ctx.source_root, target) {
        Ok(t) => Ok(t),
        Err(_) => Err(e),
    })
}

/// Where the newest line for an archive leaves it.
enum ArchiveLast {
    /// Sent to the Trash, maybe (no line after the intent: a crash during
    /// the Trash call).
    TrashIntent,
    /// In the Trash, at the location when the OS said.
    Trashed(Option<PathBuf>),
    /// Never left.
    Kept,
}

/// One archive of the journal: its newest step and the fingerprint its
/// trash intent recorded.
struct ArchiveState {
    from: PathBuf,
    item: Option<usize>,
    last: ArchiveLast,
    fp: Option<Fingerprint>,
}

/// Brings every archive the job sent to the Trash back to where it was,
/// newest first, each resolved by its last line: already back (it matches
/// the intent's fingerprint) → done; a recorded Trash location holding
/// it → a no-clobber rename back (across volumes, a verified copy); no
/// location (or a stale one) → looked up in the Trash by name and
/// fingerprint. An archive that cannot come back is reported in
/// `archive_lost` and its item's extracted files are kept.
pub(super) fn restore_archives(
    ctx: &mut Ctx<'_>,
    entries: &[Line],
    trash: &dyn TrashBackend,
    report: &mut UndoReport,
) -> Result<(), WriteError> {
    let mut states: Vec<ArchiveState> = Vec::new();
    for line in entries {
        let Line::Entry { op, fp, item, .. } = line else {
            continue;
        };
        let (from, last) = match op {
            Op::ArchiveTrash { from } => (from, ArchiveLast::TrashIntent),
            Op::ArchiveTrashed { from, trash } => (from, ArchiveLast::Trashed(trash.clone())),
            Op::ArchiveKept { from } => (from, ArchiveLast::Kept),
            _ => continue,
        };
        let from = absolute_from(ctx.source_root, from.clone());
        // Newest line last: the archive's earlier lines are superseded.
        let earlier = states
            .iter()
            .position(|s| s.from == from)
            .map(|i| states.remove(i));
        let intent_fp = match last {
            ArchiveLast::TrashIntent => *fp,
            _ => earlier.as_ref().and_then(|s| s.fp),
        };
        states.push(ArchiveState {
            from,
            item: *item,
            last,
            fp: intent_fp,
        });
    }
    for state in states.into_iter().rev() {
        if matches!(state.last, ArchiveLast::Kept) {
            continue;
        }
        let from = source_path(ctx.source_root, &state.from)?;
        match restore_archive(ctx, &from, &state, trash)? {
            Restore::NeverLeft => {}
            Restore::Back => report.restored.push(from),
            Restore::Lost => {
                ctx.lost_archives.insert(state.from.clone());
                ctx.lost_items.extend(state.item);
                report.archive_lost.push(from);
            }
        }
    }
    Ok(())
}

enum Restore {
    /// The Trash call never happened: the archive is where it was.
    NeverLeft,
    /// At its place again (now, or by an earlier, interrupted undo).
    Back,
    Lost,
}

fn restore_archive(
    ctx: &mut Ctx<'_>,
    from: &Path,
    state: &ArchiveState,
    trash: &dyn TrashBackend,
) -> Result<Restore, WriteError> {
    let Some(fp) = state.fp else {
        return Ok(Restore::Lost);
    };
    let at_from = || -> Result<Restore, WriteError> {
        if !fs::symlink_metadata(from).is_ok_and(|m| m.file_type().is_file()) {
            return Ok(Restore::Lost);
        }
        Ok(match state.last {
            ArchiveLast::TrashIntent => Restore::NeverLeft,
            _ => Restore::Back,
        })
    };
    if let Some(occupant) = occupant(from)? {
        // Back already (or never left) when it is the archive itself;
        // anything else in its place is not ours to move.
        let meta = fs::symlink_metadata(&occupant).map_err(|e| map_io(e, &occupant))?;
        if occupant == from && meta.file_type().is_file() && matches_fp(from, &meta, &fp)? {
            return at_from();
        }
        return Ok(Restore::Lost);
    }
    let recorded = match &state.last {
        ArchiveLast::Trashed(Some(at)) => Some(at.clone()),
        _ => None,
    }
    .filter(|at| {
        fs::symlink_metadata(at)
            .ok()
            .filter(|m| m.file_type().is_file())
            .is_some_and(|m| matches_fp(at, &m, &fp).unwrap_or(false))
    });
    let found = recorded.or_else(|| {
        let name = from.file_name()?.to_str()?;
        trash.find(from, name, &fp)
    });
    let Some(at) = found else {
        return Ok(Restore::Lost);
    };
    if put_back(ctx, &at, from)? {
        Ok(Restore::Back)
    } else {
        Ok(Restore::Lost)
    }
}

/// An extraction intent never completed (a crash between the rename and
/// its journal line): the file at `target` is the job's only when its
/// size and a full CRC-32 re-read equal what the archive lists.
fn undo_extract_intent(
    ctx: &Ctx<'_>,
    target: &Path,
    size: u64,
    crc32: Option<u32>,
    report: &mut UndoReport,
) -> Result<(), WriteError> {
    let Some(meta) = target_meta(ctx, target)? else {
        return Ok(());
    };
    if !meta.file_type().is_file() {
        return Ok(());
    }
    let ours = meta.len() == size
        && match crc32 {
            Some(crc) => crc_of(target)? == crc,
            None => false,
        };
    if ours {
        delete_ours(target)?;
        report.removed.push(target.to_path_buf());
    } else {
        report.skipped_modified.push(target.to_path_buf());
    }
    Ok(())
}

/// Deletes the job temp `temp` next to `target` (best effort; a name that
/// is not one of this job's temps is never touched).
fn remove_job_temp(ctx: &Ctx<'_>, target: &Path, temp: &str) {
    let Some(dir) = target.parent() else {
        return;
    };
    if !is_job_temp(ctx.job, temp) {
        return;
    }
    let path = dir.join(temp);
    if fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_file()) {
        let _ = fs::remove_file(&path);
        remove_apple_doubles(&[path]);
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

/// `path` is a regular file the job left there as `fp` says (size, and
/// the modification time within the volume's precision or else the
/// CRC-32): what undo and resume treat as the job's file.
pub(crate) fn file_matches_fp(path: &Path, fp: &Fingerprint) -> bool {
    fs::symlink_metadata(path).is_ok_and(|meta| {
        meta.file_type().is_file() && matches_fp(path, &meta, fp).unwrap_or(false)
    })
}

/// The file now at `target` is still the one the job left there: same
/// size and modification time, or (mtime changed) the same CRC-32.
fn matches_fp(target: &Path, meta: &fs::Metadata, fp: &Fingerprint) -> Result<bool, WriteError> {
    if meta.len() != fp.size {
        return Ok(false);
    }
    if meta
        .modified()
        .map(mtime_ns)
        .is_ok_and(|now| same_mtime(now, fp.mtime_ns))
    {
        return Ok(true);
    }
    match fp.crc32 {
        Some(crc) => Ok(crc_of(target)? == crc),
        None => Ok(false),
    }
}

/// Whether the modification time `now` read back is the `recorded` one,
/// allowing for the volume storing less than the job saw: FAT keeps even
/// seconds (`now` a whole even second within 2 s), Windows 100 ns ticks
/// (`now` a whole tick within one).
pub(super) fn same_mtime(now: i64, recorded: i64) -> bool {
    const FAT: i64 = 2_000_000_000;
    const TICK: i64 = 100;
    let gap = now.abs_diff(recorded);
    now == recorded
        || (now % FAT == 0 && gap < FAT.unsigned_abs())
        || (now % TICK == 0 && gap < TICK.unsigned_abs())
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

/// Reports `target` as kept because its source name is taken, once: a
/// move across volumes journals the rename it tried, the copy intent and
/// the copy, and all three name the same target.
fn occupied(report: &mut UndoReport, target: &Path) {
    if !report.skipped_occupied.iter().any(|p| p == target) {
        report.skipped_occupied.push(target.to_path_buf());
    }
}

/// Whether `occupant` (what [`occupant`] found for `from`) is the file at
/// `from` itself, under any spelling the volume folds to it, rather than
/// a case/NFC twin a case-sensitive volume keeps beside it.
fn is_from_itself(occupant: &Path, from: &Path) -> bool {
    occupant == from || same_entry(occupant, from)
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
        // copy; a rename never leaves two. A case/NFC twin with the same
        // bytes is another file (a kept duplicate on a case-sensitive
        // volume), never the source: the target is then the only copy of
        // that file and stays.
        if via == Via::Copy && is_from_itself(&occupant, from) && same_bytes(&occupant, target)? {
            delete_ours(target)?;
            report.restored.push(from.to_path_buf());
        } else {
            occupied(report, target);
        }
        return Ok(());
    }
    if put_back(ctx, target, from)? {
        report.restored.push(from.to_path_buf());
    } else {
        occupied(report, target);
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
            match stream_copy(target, from, &prefix, &mut ctx.counter, true) {
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
    if !is_from_itself(&occupant, from) {
        // A twin of the source's name, not the source: the target may be
        // the only copy left of it.
        occupied(report, target);
        return Ok(());
    }
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
    let Ok(dir) = within_either(ctx, dir) else {
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

/// Puts each manual identity the job rekeyed from `from` to `to` back
/// under `from`, after the files were undone: an identity follows its
/// file, so one whose file is still at `to` (changed after the job, or its
/// source taken again) stays there, and `from` keeps any identity of its
/// own it has by now.
pub(super) fn restore_overrides(ctx: &mut Ctx<'_>) -> Result<(), WriteError> {
    let pending = std::mem::take(&mut ctx.overrides);
    let path = ctx.data_dir.join(OVERRIDES_FILE);
    if pending.is_empty() || fs::symlink_metadata(&path).is_err() {
        return Ok(());
    }
    let mut overrides = Overrides::load_for_update(&path);
    let mut changed = false;
    for (from, to) in pending {
        match fs::symlink_metadata(&to) {
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            // Still there (or unreadable): the file did not go back.
            _ => continue,
        }
        let Some(identity) = overrides.get(&to).cloned() else {
            continue;
        };
        if overrides.get(&from).is_some() {
            continue;
        }
        overrides.remove(&to);
        overrides.set(&from, identity);
        changed = true;
    }
    if !changed {
        return Ok(());
    }
    overrides.save(&path).map_err(|e| map_io(e, &path))
}

#[cfg(test)]
mod tests {
    use super::same_mtime;

    #[test]
    fn mtimes_match_within_what_the_volume_stores() {
        let t = 1_700_000_000_123_456_789i64;
        assert!(same_mtime(t, t));
        // FAT: rounded to an even second, up to 2 s off.
        assert!(same_mtime(
            1_700_000_000_000_000_000,
            1_700_000_001_999_999_999
        ));
        assert!(!same_mtime(
            1_700_000_000_000_000_000,
            1_700_000_002_000_000_001
        ));
        // Windows: whole 100 ns ticks.
        assert!(same_mtime(1_700_000_000_123_456_700, t));
        assert!(!same_mtime(1_700_000_000_123_456_600, t));
        // A precise mtime that differs is a different mtime.
        assert!(!same_mtime(t + 1, t));
    }
}
