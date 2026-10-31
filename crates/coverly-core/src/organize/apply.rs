//! Applying an organise plan (organise spec §9, §10): every item is
//! re-validated against the plan, moved or copied through the journaled
//! [`Writer`] (never replacing anything), with one directory index for the
//! whole run, manual identities following moved ROMs, a pause on a full,
//! read-only or vanished volume and a resume into the same journal, so one
//! undo reverts every run of a job.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, ErrorKind, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::archive::{open_archive_with, FoundTool, Limits, ToolsOnDemand};
use crate::dedupe::apply::{trash_file, TrashStep};
use crate::disc::sheets::SheetKind;
use crate::identify::overrides::{Overrides, OVERRIDES_FILE};
use crate::jobs::ItemErrorKind;
use crate::organize::extract::{
    archive_unchanged, decode_total, dispose_archive, extract_and_commit, m3u_text, Adopted,
    ArchiveFate, CommitRequest, ExtractError, ExtractRequest, ExtractTick,
};
use crate::organize::plan::{
    ArchiveAfter, ExtractEntry, ExtractSource, ItemKind, OrganizeAction, OrganizeItem,
    OrganizePhase, OrganizePlan, OrganizeProgress, Transfer, TransferMode, SPACE_RESERVE,
};
use crate::organize::target::DirIndex;
use crate::organize::walk::{tree_summary, OsMetaProbe, WalkOptions};
use crate::progress::PauseKind;
use crate::write::trash::TrashBackend;
use crate::write::{
    file_matches_fp, job_archive_steps, job_transfers, remove_apple_doubles, rename_noclobber,
    same_volume, ArchiveRecord, ArchiveStep, Collision, Fingerprint, JobKind, JobMeta, MoveOutcome,
    TransferRecord, WriteError, Writer,
};

/// What happened to one item.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum ItemStatus {
    Moved,
    Copied,
    /// A move across volumes copied the file but could not delete the
    /// source: both exist.
    CopiedNotRemoved,
    /// The archive's entries were extracted and committed; in move mode
    /// with nothing left in it, the archive went to the Trash.
    Extracted,
    /// Extracted, but the archive meant for the Trash stays where it was
    /// (no Trash on its volume, the Trash refused it, or it changed).
    ExtractedKept,
    /// A duplicate set aside in the pile's quarantine (`targets` say
    /// where its files are now).
    Quarantined,
    /// A duplicate sent to the Trash (`targets` are its places there, when
    /// the OS said).
    Trashed,
    Skipped {
        reason: SkipReason,
    },
    Failed {
        kind: ItemErrorKind,
        detail: String,
    },
}

/// Why an item was not moved.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SkipReason {
    /// A source's size or modification time differs from the plan.
    ChangedSincePlan,
    SourceMissing,
    /// The same content already sits at the target.
    AlreadyPresent,
    /// A file of a set is taken at the target: the whole set stays.
    CollisionSet,
    /// A review item without the user's answer.
    NotDecided,
    Cancelled,
    /// The external tool the plan opened the archive with is gone or is
    /// another version now.
    ToolGone,
    /// A duplicate meant for the Trash: its volume has none, or the Trash
    /// refused it. It stays where it was.
    TrashUnavailable,
}

/// One item of the plan and what happened to it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ItemOutcome {
    /// The plan item's id.
    pub item: usize,
    #[serde(flatten)]
    pub status: ItemStatus,
    /// Where its files are now (absolute): ROM or set members, then
    /// companions.
    pub targets: Vec<PathBuf>,
}

/// What an apply (or resume) did.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct OrganizeReport {
    /// The journal of the job; `None` when nothing changed.
    pub job: Option<String>,
    pub moved: usize,
    pub copied: usize,
    /// Archives whose entries were extracted (left out of the JSON when
    /// none was).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub extracted: usize,
    pub skipped: usize,
    pub failed: usize,
    /// Duplicates set aside or sent to the Trash (left out of the JSON
    /// when none was).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub set_aside: usize,
    pub items: Vec<ItemOutcome>,
    /// Set when the run stopped on a full, read-only or vanished volume.
    pub paused: Option<PauseKind>,
    /// Which of those it was (`disk-full`, `write-denied`, `volume-gone`).
    #[serde(default)]
    pub pause_reason: Option<ItemErrorKind>,
    /// Items not attempted yet: what a resume does.
    pub resumable_items: Vec<usize>,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

/// What apply needs besides the plan.
pub struct ApplyEnv<'a> {
    pub data_dir: &'a Path,
    pub progress: &'a (dyn Fn(OrganizeProgress) + Sync),
    /// Checked between items, and between and inside the entries of an
    /// extraction until its files are renamed into place.
    pub cancel: &'a AtomicBool,
    /// Where move mode sends an archive whose entries were all extracted.
    pub trash: &'a dyn TrashBackend,
    /// The external tools extractions may use; `None` finds them on first
    /// need (never inside the source or target roots).
    pub tools: Option<&'a [FoundTool]>,
}

#[derive(Debug, thiserror::Error)]
pub enum ApplyError {
    #[error("not enough space: {needed} bytes needed, {free} free")]
    NoSpace { needed: u64, free: u64 },
    #[error(transparent)]
    Write(#[from] WriteError),
    #[error("the plan of job {0} is missing")]
    NoPlan(String),
    /// The job was undone: resuming it would redo it in a journal undo no
    /// longer lists.
    #[error("job {0} was undone: there is nothing to resume")]
    Undone(String),
    /// A duplicates plan or its decisions cannot be applied (nothing was
    /// written).
    #[error(transparent)]
    Dedupe(#[from] crate::dedupe::DedupeError),
}

/// Apply progress is reported at most this often.
const PROGRESS_EVERY: Duration = Duration::from_millis(100);

#[cfg(feature = "test-util")]
type FreeSpaceFn = fn(&Path) -> Option<u64>;

#[cfg(feature = "test-util")]
thread_local! {
    static FREE_SPACE: std::cell::Cell<Option<FreeSpaceFn>> = const { std::cell::Cell::new(None) };
    static CLUSTER_SIZE: std::cell::Cell<Option<u64>> = const { std::cell::Cell::new(None) };
    static LAST_DIR_READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Test hook: makes [`free_space`] (and [`user_available`]) on the calling
/// thread answer with `f`. `None` clears it.
#[doc(hidden)]
#[cfg(feature = "test-util")]
pub fn set_free_space_override(f: Option<FreeSpaceFn>) {
    FREE_SPACE.with(|c| c.set(f));
}

/// Test hook: makes [`cluster_size`] on the calling thread answer
/// `bytes`. `None` clears it.
#[doc(hidden)]
#[cfg(feature = "test-util")]
pub fn set_cluster_size_override(bytes: Option<u64>) {
    CLUSTER_SIZE.with(|c| c.set(bytes));
}

/// Test hook: how many folders the last apply on the calling thread
/// listed into its directory index.
#[doc(hidden)]
#[cfg(feature = "test-util")]
pub fn last_apply_dir_reads() -> usize {
    LAST_DIR_READS.with(std::cell::Cell::get)
}

/// Bytes free for this user on the volume holding `path` (or its nearest
/// existing parent); `None` when it cannot be told. The same as
/// [`user_available`].
pub fn free_space(path: &Path) -> Option<u64> {
    user_available(path)
}

/// Bytes the calling user may still write on the volume holding `path`
/// (or its nearest existing parent): `statvfs` `f_bavail × f_frsize`
/// (blocks reserved for root are not counted), Windows
/// `GetDiskFreeSpaceExW`'s caller quota. `None` when it cannot be told.
pub fn user_available(path: &Path) -> Option<u64> {
    #[cfg(feature = "test-util")]
    if let Some(f) = FREE_SPACE.with(std::cell::Cell::get) {
        return f(path);
    }
    os_free_space(existing_ancestor(path)?)
}

/// The allocation unit of the volume holding `path` (or its nearest
/// existing parent): every file takes a whole number of these. `statvfs`
/// `f_frsize`, Windows `GetDiskFreeSpaceW` sectors per cluster × bytes
/// per sector. `None` when it cannot be told.
pub fn cluster_size(path: &Path) -> Option<u64> {
    #[cfg(feature = "test-util")]
    if let Some(bytes) = CLUSTER_SIZE.with(std::cell::Cell::get) {
        return Some(bytes);
    }
    os_cluster_size(existing_ancestor(path)?).filter(|&c| c > 0)
}

fn existing_ancestor(path: &Path) -> Option<&Path> {
    path.ancestors()
        .find(|p| !p.as_os_str().is_empty() && p.exists())
}

#[cfg(unix)]
fn statvfs_of(path: &Path) -> Option<libc::statvfs> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let c = CString::new(path.as_os_str().as_bytes()).ok()?;
    // SAFETY: `statvfs` is plain old data; all zeroes is a valid value.
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: `c` is a NUL-terminated path and `st` a valid out pointer.
    if unsafe { libc::statvfs(c.as_ptr(), &mut st) } != 0 {
        return None;
    }
    Some(st)
}

#[cfg(unix)]
fn os_free_space(path: &Path) -> Option<u64> {
    let st = statvfs_of(path)?;
    // The field types differ between platforms.
    #[allow(clippy::unnecessary_cast, clippy::useless_conversion)]
    let (avail, frsize) = (u64::from(st.f_bavail), st.f_frsize as u64);
    Some(avail.saturating_mul(frsize))
}

#[cfg(unix)]
fn os_cluster_size(path: &Path) -> Option<u64> {
    let st = statvfs_of(path)?;
    #[allow(clippy::unnecessary_cast)]
    Some(st.f_frsize as u64)
}

#[cfg(windows)]
fn wide_path(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

#[cfg(windows)]
fn os_free_space(path: &Path) -> Option<u64> {
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let wide = wide_path(path);
    let mut free: u64 = 0;
    // SAFETY: `wide` is NUL-terminated; the null pointers are optional
    // outputs the call skips. The first output is the caller's quota.
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut free,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    (ok != 0).then_some(free)
}

#[cfg(windows)]
fn os_cluster_size(path: &Path) -> Option<u64> {
    use windows_sys::Win32::Storage::FileSystem::{GetDiskFreeSpaceW, GetVolumePathNameW};
    let wide = wide_path(path);
    let mut root = [0u16; 1024];
    // SAFETY: `wide` is NUL-terminated and `root` holds `len` u16s.
    let ok = unsafe { GetVolumePathNameW(wide.as_ptr(), root.as_mut_ptr(), root.len() as u32) };
    if ok == 0 {
        return None;
    }
    let (mut sectors, mut bytes, mut free, mut total) = (0u32, 0u32, 0u32, 0u32);
    // SAFETY: `root` was NUL-terminated by the call above (a volume root
    // with its trailing backslash); the outputs are valid pointers.
    let ok = unsafe {
        GetDiskFreeSpaceW(
            root.as_ptr(),
            &mut sectors,
            &mut bytes,
            &mut free,
            &mut total,
        )
    };
    (ok != 0).then(|| u64::from(sectors) * u64::from(bytes))
}

#[cfg(not(any(unix, windows)))]
fn os_free_space(_: &Path) -> Option<u64> {
    None
}

#[cfg(not(any(unix, windows)))]
fn os_cluster_size(_: &Path) -> Option<u64> {
    None
}

/// Applies `plan`: the `Auto` items with a target are moved (or copied)
/// into place. The plan is saved next to the journal before the first
/// action, so a paused or interrupted job can be resumed.
pub fn apply_organize(plan: &OrganizePlan, env: &ApplyEnv) -> Result<OrganizeReport, ApplyError> {
    let mut report = OrganizeReport::default();
    not_decided(plan, &mut report);
    let pending: Vec<&OrganizeItem> = plan.items.iter().filter(|i| applicable(i)).collect();
    if pending.is_empty() {
        return Ok(report);
    }
    if let Some(reason) = roots_gone(plan) {
        pause(&mut report, reason, &pending);
        return Ok(report);
    }
    check_space(plan, &pending)?;
    let writer = Writer::begin_with(
        &plan.target_root,
        env.data_dir,
        JobMeta {
            kind: JobKind::Organize,
            source_root: Some(plan.source_root.clone()),
        },
    )?;
    if let Err(e) = save_plan(env.data_dir, writer.job_id(), plan) {
        let _ = writer.discard();
        return Err(e.into());
    }
    Ok(Run::new(plan, env, writer, false, Vec::new()).run(&pending, 0, report))
}

/// Continues the paused or interrupted job `job` from its saved plan:
/// items the journal shows as done are counted, the rest are re-validated
/// and applied into the same journal.
pub fn resume_organize(job: &str, env: &ApplyEnv) -> Result<OrganizeReport, ApplyError> {
    let plan = load_plan(env.data_dir, job)?;
    let mut report = OrganizeReport::default();
    not_decided(&plan, &mut report);
    let records = job_transfers(env.data_dir, job)?;
    let steps = job_archive_steps(env.data_dir, job)?;
    let applicable: Vec<&OrganizeItem> = plan.items.iter().filter(|i| applicable(i)).collect();
    let mut pending = Vec::new();
    let mut started = HashMap::new();
    for item in &applicable {
        if item.is_set_aside() {
            match set_aside_done(&plan, item, &records, &steps) {
                Some((status, targets)) => record_outcome(&mut report, item.id, status, targets),
                None => pending.push(*item),
            }
            continue;
        }
        if matches!(item.transfer, Transfer::Extract { .. }) {
            match ExtractJournal::of(&plan, item, &records, &steps).done(&plan, item) {
                Some((status, targets)) => record_outcome(&mut report, item.id, status, targets),
                None => pending.push(*item),
            }
            continue;
        }
        match finished_record(&plan, item, &records) {
            Some(record) => {
                let status = match (record.copied, plan.mode) {
                    (false, _) => ItemStatus::Moved,
                    (true, TransferMode::Copy) => ItemStatus::Copied,
                    (true, TransferMode::Move) => ItemStatus::CopiedNotRemoved,
                };
                if has_leftovers(&plan, item, &records) {
                    // The ROM is in place but set members or companions
                    // are not (a pause or a crash in the middle of the
                    // item): the run finishes them.
                    started.insert(item.id, (status, record.target.clone()));
                    pending.push(*item);
                } else {
                    let targets = item_targets(&plan, item, &records);
                    record_outcome(&mut report, item.id, status, targets);
                }
            }
            None => pending.push(*item),
        }
    }
    let done = applicable.len() - pending.len();
    if let Some(reason) = roots_gone(&plan) {
        report.job = Some(job.to_string());
        pause(&mut report, reason, &pending);
        return Ok(report);
    }
    check_space(&plan, &pending)?;
    let writer = Writer::resume(&plan.target_root, env.data_dir, job)?;
    writer.clean_temps(&target_dirs(&plan, &applicable));
    let mut run = Run::new(&plan, env, writer, true, records);
    run.started = started;
    run.archive_steps = steps;
    Ok(run.run(&pending, done, report))
}

/// Items apply moves: automatic ones with a target (review items become
/// automatic only through the user's decisions), and duplicates set aside
/// as they are (never extracted first).
fn applicable(item: &OrganizeItem) -> bool {
    (item.action == OrganizeAction::Auto && item.target.is_some())
        || (item.is_set_aside() && item.transfer == Transfer::AsIs)
}

/// Review items are reported as waiting for the user.
fn not_decided(plan: &OrganizePlan, report: &mut OrganizeReport) {
    for item in plan
        .items
        .iter()
        .filter(|i| i.action == OrganizeAction::Review)
    {
        let status = ItemStatus::Skipped {
            reason: SkipReason::NotDecided,
        };
        record_outcome(report, item.id, status, Vec::new());
    }
}

fn record_outcome(
    report: &mut OrganizeReport,
    item: usize,
    status: ItemStatus,
    targets: Vec<PathBuf>,
) {
    match status {
        ItemStatus::Moved => report.moved += 1,
        ItemStatus::Copied | ItemStatus::CopiedNotRemoved => report.copied += 1,
        ItemStatus::Extracted | ItemStatus::ExtractedKept => report.extracted += 1,
        ItemStatus::Quarantined | ItemStatus::Trashed => report.set_aside += 1,
        ItemStatus::Skipped { .. } => report.skipped += 1,
        ItemStatus::Failed { .. } => report.failed += 1,
    }
    report.items.push(ItemOutcome {
        item,
        status,
        targets,
    });
}

fn pause(report: &mut OrganizeReport, reason: ItemErrorKind, rest: &[&OrganizeItem]) {
    pause_as(report, reason, PauseKind::Volume, rest);
}

fn pause_as(
    report: &mut OrganizeReport,
    reason: ItemErrorKind,
    kind: PauseKind,
    rest: &[&OrganizeItem],
) {
    report.paused = Some(kind);
    report.pause_reason = Some(reason);
    report.resumable_items = rest.iter().map(|i| i.id).collect();
}

/// `volume-gone` when the source or the target root is not there.
fn roots_gone(plan: &OrganizePlan) -> Option<ItemErrorKind> {
    (!plan.source_root.is_dir() || !plan.target_root.is_dir()).then_some(ItemErrorKind::VolumeGone)
}

/// A copy, or a move across volumes, needs the items' bytes plus 1 %
/// free on the target volume. Unknown free space is not checked.
fn check_space(plan: &OrganizePlan, items: &[&OrganizeItem]) -> Result<(), ApplyError> {
    let copies =
        plan.mode == TransferMode::Copy || !same_volume(&plan.source_root, &plan.target_root);
    if !copies || items.is_empty() {
        return Ok(());
    }
    // A duplicate set aside stays on the pile's volume.
    let sum: u64 = items
        .iter()
        .filter(|i| !i.is_set_aside())
        .map(|i| i.size)
        .sum();
    let needed = sum.saturating_add(sum.div_ceil(100));
    match free_space(&plan.target_root) {
        Some(free) if free < needed => Err(ApplyError::NoSpace { needed, free }),
        _ => Ok(()),
    }
}

fn plan_path(data_dir: &Path, job: &str) -> PathBuf {
    data_dir.join("jobs").join(format!("{job}.plan.json"))
}

fn save_plan(data_dir: &Path, job: &str, plan: &OrganizePlan) -> Result<(), WriteError> {
    let text = serde_json::to_vec_pretty(plan).map_err(|e| WriteError::Journal(e.to_string()))?;
    let path = plan_path(data_dir, job);
    let mut file = File::create_new(&path)?;
    io::Write::write_all(&mut file, &text)?;
    file.sync_all()?;
    Ok(())
}

/// The plan saved with organise job `job` (what [`resume_organize`]
/// continues from). [`ApplyError::NoPlan`] when there is none.
pub fn load_plan(data_dir: &Path, job: &str) -> Result<OrganizePlan, ApplyError> {
    let valid = !job.is_empty()
        && job
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    if !valid {
        return Err(ApplyError::NoPlan(job.to_string()));
    }
    let bytes = match fs::read(plan_path(data_dir, job)) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == ErrorKind::NotFound => {
            return Err(ApplyError::NoPlan(job.to_string()))
        }
        Err(e) => return Err(WriteError::Io(e).into()),
    };
    serde_json::from_slice(&bytes)
        .map_err(|e| WriteError::Journal(format!("plan of job {job}: {e}")).into())
}

/// A duplicate's files: where each goes in the pile's quarantine (the
/// plan's `dest` for the primary, the same place below the quarantine for
/// the other files of a set). Absolute.
fn set_aside_paths(plan: &OrganizePlan, item: &OrganizeItem) -> Vec<(PathBuf, PathBuf)> {
    let OrganizeAction::SetAside { dest, .. } = &item.action else {
        return Vec::new();
    };
    let src = &plan.source_root;
    // `dest` is `<quarantine>/<primary's path>`: the quarantine is what is
    // left once the primary's path is taken off.
    let base = item
        .sources
        .first()
        .and_then(|first| {
            let mut base = dest.clone();
            for _ in first.components() {
                if !base.pop() {
                    return None;
                }
            }
            dest.ends_with(first).then_some(base)
        })
        .or_else(|| dest.parent().map(Path::to_path_buf))
        .unwrap_or_default();
    item.sources
        .iter()
        .enumerate()
        .map(|(i, rel)| {
            let to = if i == 0 {
                src.join(dest)
            } else {
                src.join(&base).join(rel)
            };
            (src.join(rel), to)
        })
        .collect()
}

/// A duplicate an earlier run of the job finished: every file in the
/// quarantine by the journal, or (Trash) every file's last line `trashed`.
fn set_aside_done(
    plan: &OrganizePlan,
    item: &OrganizeItem,
    records: &[TransferRecord],
    steps: &[ArchiveRecord],
) -> Option<(ItemStatus, Vec<PathBuf>)> {
    let paths = set_aside_paths(plan, item);
    if paths.is_empty() {
        return None;
    }
    let mut targets = Vec::with_capacity(paths.len());
    for (from, _) in &paths {
        if plan.set_aside_trash {
            match last_step(steps, item.id, from) {
                Some(ArchiveStep::Trashed { trash }) => targets.extend(trash.clone()),
                _ => return None,
            }
        } else {
            targets.push(record_of(records, item.id, from)?.target.clone());
        }
    }
    // A companion still at its source with no line yet: the run stopped
    // before it followed.
    for companion in &item.companions {
        let from = plan.source_root.join(&companion.source);
        if let Some(record) = record_of(records, item.id, &from) {
            targets.push(record.target.clone());
        } else if let Some(step) = last_step(steps, item.id, &from) {
            if let ArchiveStep::Trashed { trash } = step {
                targets.extend(trash.clone());
            }
        } else if fs::symlink_metadata(&from).is_ok_and(|m| m.file_type().is_file()) {
            return None;
        }
    }
    let status = if plan.set_aside_trash {
        ItemStatus::Trashed
    } else {
        ItemStatus::Quarantined
    };
    Some((status, targets))
}

/// The newest archive line of `from` for item `id`.
fn last_step<'s>(steps: &'s [ArchiveRecord], id: usize, from: &Path) -> Option<&'s ArchiveStep> {
    steps
        .iter()
        .rev()
        .find(|s| s.item == Some(id) && s.from == from)
        .map(|s| &s.step)
}

/// The journal record showing `item` done: its primary file was copied
/// (the copy finished), or moved and is no longer at its source.
fn finished_record<'r>(
    plan: &OrganizePlan,
    item: &OrganizeItem,
    records: &'r [TransferRecord],
) -> Option<&'r TransferRecord> {
    record_of(
        records,
        item.id,
        &plan.source_root.join(item.sources.first()?),
    )
}

/// The finished journal record of `from` (a file of `item`): a finished
/// copy that is still at its target (a set rolled back deletes the copies
/// it made), or a finished move whose source is gone. Files extracted
/// from an archive never count as the archive moved or copied.
fn record_of<'r>(
    records: &'r [TransferRecord],
    id: usize,
    from: &Path,
) -> Option<&'r TransferRecord> {
    records.iter().rev().find(|r| {
        r.item == Some(id)
            && r.extracted.is_none()
            && r.from == from
            && r.finished
            && if r.copied {
                fs::symlink_metadata(&r.target).is_ok_and(|m| m.file_type().is_file())
            } else {
                fs::symlink_metadata(from).is_err()
            }
    })
}

/// Set members or companions of a started item still at their source
/// without a finished record: what a resume still has to transfer.
fn has_leftovers(plan: &OrganizePlan, item: &OrganizeItem, records: &[TransferRecord]) -> bool {
    let members = item.sources.iter().skip(1);
    let companions = item.companions.iter().map(|c| &c.source);
    members.chain(companions).any(|rel| {
        let from = plan.source_root.join(rel);
        fs::symlink_metadata(&from).is_ok_and(|m| m.file_type().is_file())
            && record_of(records, item.id, &from).is_none()
    })
}

/// Where a finished item's files are by the journal: primary, members,
/// then companions.
fn item_targets(
    plan: &OrganizePlan,
    item: &OrganizeItem,
    records: &[TransferRecord],
) -> Vec<PathBuf> {
    let companions = item.companions.iter().map(|c| &c.source);
    item.sources
        .iter()
        .chain(companions)
        .filter_map(|rel| record_of(records, item.id, &plan.source_root.join(rel)))
        .map(|r| r.target.clone())
        .collect()
}

/// What earlier runs of the job journaled for one entry of an `Extract`
/// item: its latest intent, unless a `rolled-back` line undid it.
#[derive(Clone, Debug, PartialEq, Eq)]
enum EntryLog {
    /// Nothing, or only rolled-back intents.
    None,
    /// An intent without its completion: a crash in phase A/B (the temp
    /// is a leftover), or between the rename and the completion line.
    Intent { temp: String },
    /// Renamed into place and journaled complete.
    Completed { temp: String, fp: Fingerprint },
}

/// What earlier runs of the job journaled for one `Extract` item (resume,
/// archives spec §9.3).
struct ExtractJournal {
    /// One per plan entry, in plan order.
    entries: Vec<EntryLog>,
    /// The item's last archive line.
    archive: Option<ArchiveStep>,
}

impl ExtractJournal {
    fn of(
        plan: &OrganizePlan,
        item: &OrganizeItem,
        records: &[TransferRecord],
        steps: &[ArchiveRecord],
    ) -> ExtractJournal {
        let Transfer::Extract { entries, .. } = &item.transfer else {
            return ExtractJournal {
                entries: Vec::new(),
                archive: None,
            };
        };
        // The item's own lines, found once: a resume of thousands of
        // items must not rescan the whole journal for every entry.
        let of_item: Vec<&TransferRecord> = records
            .iter()
            .filter(|r| r.item == Some(item.id) && r.extracted.is_some())
            .collect();
        let entries = entries
            .iter()
            .map(|e| {
                let target = plan.target_root.join(&e.target);
                let mine: Vec<&TransferRecord> = of_item
                    .iter()
                    .copied()
                    .filter(|r| {
                        r.target == target
                            && r.extracted.as_ref().is_some_and(|x| x.entry == e.name)
                    })
                    .collect();
                // A rollback line marks the intent; a completion it undid
                // came before it, so the whole temp's story is undone.
                let undone = |temp: &str| {
                    mine.iter().any(|r| {
                        r.extracted
                            .as_ref()
                            .is_some_and(|x| x.temp == temp && x.rolled_back)
                    })
                };
                let mut log = EntryLog::None;
                for r in &mine {
                    let Some(x) = r.extracted.as_ref() else {
                        continue;
                    };
                    log = match (undone(&x.temp), r.finished, r.fp) {
                        (true, ..) => EntryLog::None,
                        (false, true, Some(fp)) => EntryLog::Completed {
                            temp: x.temp.clone(),
                            fp,
                        },
                        _ => EntryLog::Intent {
                            temp: x.temp.clone(),
                        },
                    };
                }
                log
            })
            .collect();
        let archive = steps
            .iter()
            .rev()
            .find(|s| s.item == Some(item.id))
            .map(|s| s.step.clone());
        ExtractJournal { entries, archive }
    }

    /// An earlier run left something of the item behind.
    fn started(&self) -> bool {
        self.archive.is_some() || self.entries.iter().any(|e| *e != EntryLog::None)
    }

    /// The item's outcome when an earlier run finished it: every entry
    /// journaled complete, the playlist in place, and (move mode, archive
    /// to the Trash) its archive line final, not just the intent.
    fn done(&self, plan: &OrganizePlan, item: &OrganizeItem) -> Option<(ItemStatus, Vec<PathBuf>)> {
        let Transfer::Extract {
            entries,
            archive_after,
            m3u,
            ..
        } = &item.transfer
        else {
            return None;
        };
        if entries.is_empty() || self.entries.len() != entries.len() {
            return None;
        }
        let mut targets: Vec<PathBuf> = Vec::with_capacity(entries.len() + 1);
        for (e, log) in entries.iter().zip(&self.entries) {
            if !matches!(log, EntryLog::Completed { .. }) {
                return None;
            }
            targets.push(plan.target_root.join(&e.target));
        }
        if let Some(m3u) = m3u {
            let m3u = plan.target_root.join(m3u);
            if !fs::symlink_metadata(&m3u).is_ok_and(|m| m.file_type().is_file()) {
                return None;
            }
            targets.push(m3u);
        }
        let status = if trashes(plan, *archive_after) {
            match self.archive {
                Some(ArchiveStep::Trashed { .. }) => ItemStatus::Extracted,
                Some(ArchiveStep::Kept) => ItemStatus::ExtractedKept,
                _ => return None,
            }
        } else {
            ItemStatus::Extracted
        };
        Some((status, targets))
    }
}

/// Whether an extracted item's archive goes to the Trash afterwards.
fn trashes(plan: &OrganizePlan, after: ArchiveAfter) -> bool {
    after == ArchiveAfter::Trash && plan.mode == TransferMode::Move
}

/// Every folder the items' files go to (absolute, without repeats).
fn target_dirs(plan: &OrganizePlan, items: &[&OrganizeItem]) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    for item in items {
        let extracted: Vec<&PathBuf> = match &item.transfer {
            Transfer::Extract { entries, m3u, .. } => {
                entries.iter().map(|e| &e.target).chain(m3u).collect()
            }
            Transfer::AsIs => Vec::new(),
        };
        let targets = item
            .target
            .iter()
            .chain(&item.set_targets)
            .chain(item.companions.iter().map(|c| &c.target))
            .chain(extracted);
        for target in targets {
            if let Some(dir) = plan.target_root.join(target).parent() {
                if !dirs.iter().any(|d| d == dir) {
                    dirs.push(dir.to_path_buf());
                }
            }
        }
    }
    dirs
}

/// Bytes the target needs free for the extractions among `items`: every
/// entry and playlist rounded up to whole `cluster`s, plus 1 % and the
/// reserve (archives spec §8.4); 0 when none extracts.
fn extract_need(items: &[&OrganizeItem], cluster: u64) -> u64 {
    let cluster = cluster.max(1);
    let round = |n: u64| n.div_ceil(cluster).saturating_mul(cluster);
    let sum = items
        .iter()
        .filter_map(|i| match &i.transfer {
            Transfer::Extract { entries, m3u, .. } => Some(
                entries
                    .iter()
                    .map(|e| round(e.size))
                    .chain(m3u.as_ref().map(|_| round(1)))
                    .fold(0u64, u64::saturating_add),
            ),
            Transfer::AsIs => None,
        })
        .fold(0u64, u64::saturating_add);
    if sum == 0 {
        return 0;
    }
    sum.saturating_add(sum.div_ceil(100))
        .saturating_add(SPACE_RESERVE)
}

/// Runs `f` with a cancel flag of its own that follows `outer` (polled by
/// a helper thread until `f` returns): the decode budget of an extraction
/// needs a shared flag, the apply environment lends a plain one.
fn with_linked_cancel<T>(outer: &AtomicBool, f: impl FnOnce(&Arc<AtomicBool>) -> T) -> T {
    /// Stops the helper when `f` returns or unwinds: the scope waits for
    /// it, so a panic in `f` must not leave it polling forever.
    struct Over<'f>(&'f AtomicBool);
    impl Drop for Over<'_> {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }
    let inner = Arc::new(AtomicBool::new(outer.load(Ordering::SeqCst)));
    let over = AtomicBool::new(false);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            while !over.load(Ordering::SeqCst) {
                if outer.load(Ordering::SeqCst) {
                    inner.store(true, Ordering::SeqCst);
                    return;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        });
        let _over = Over(&over);
        f(&inner)
    })
}

fn mtime_ns(meta: &fs::Metadata) -> Option<i64> {
    let t = meta.modified().ok()?;
    Some(match t.duration_since(UNIX_EPOCH) {
        Ok(d) => i64::try_from(d.as_nanos()).unwrap_or(i64::MAX),
        Err(e) => -i64::try_from(e.duration().as_nanos()).unwrap_or(i64::MAX),
    })
}

/// `"{stem} ({n}).{ext}"` (no extension: `"{stem} ({n})"`), as the writer
/// numbers names.
fn numbered(name: &str, n: u32) -> String {
    match name.rfind('.').filter(|&i| i > 0) {
        Some(i) => format!("{} ({n}){}", &name[..i], &name[i..]),
        None => format!("{name} ({n})"),
    }
}

fn stem(name: &str) -> &str {
    match name.rfind('.').filter(|&i| i > 0) {
        Some(i) => &name[..i],
        None => name,
    }
}

fn split(path: &Path) -> Option<(&Path, &str)> {
    Some((path.parent()?, path.file_name()?.to_str()?))
}

/// CRC-32 of a file; `None` when it cannot be read.
fn crc_of(path: &Path) -> Option<u32> {
    let mut file = crate::consoles::open_rom(path).ok()?;
    let mut hasher = crc32fast::Hasher::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        match file.read(&mut buf) {
            Ok(0) => return Some(hasher.finalize()),
            Ok(n) => hasher.update(&buf[..n]),
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(_) => return None,
        }
    }
}

/// Two regular files with the same size and bytes (by CRC-32).
fn same_content(a: &Path, b: &Path) -> bool {
    match (fs::metadata(a), fs::metadata(b)) {
        (Ok(ma), Ok(mb)) if ma.is_file() && mb.is_file() && ma.len() == mb.len() => {
            crc_of(a).is_some_and(|x| Some(x) == crc_of(b))
        }
        _ => false,
    }
}

/// How one item ended, or the pause that stopped the run.
enum Step {
    Done(ItemStatus, Vec<PathBuf>),
    Pause(ItemErrorKind),
    /// A pause of another kind than [`PauseKind::Volume`] (a full disk
    /// during an extraction, with the numbers).
    PauseAs(ItemErrorKind, PauseKind),
    /// Stopped at an injected crash point: the run ends as if killed (no
    /// more lines, the job not finished).
    Abort,
}

/// One apply (or resume) run.
struct Run<'a> {
    plan: &'a OrganizePlan,
    env: &'a ApplyEnv<'a>,
    writer: Writer,
    resumed: bool,
    /// One listing per target folder for the whole run.
    index: DirIndex,
    overrides_path: PathBuf,
    /// The manual identities as the run started; `None` without any.
    overrides: Option<Overrides>,
    /// The journal's transfers before this run (resume only).
    records: Vec<TransferRecord>,
    /// Items whose primary file a previous run already transferred, with
    /// its status and where it went (resume only).
    started: HashMap<usize, (ItemStatus, PathBuf)>,
    /// Items done before the current one, and the run's item count (for
    /// extraction progress).
    at: (usize, usize),
    /// Free space the current and later extractions need.
    extract_need: u64,
    /// The external tools, found on first need.
    tools: ToolsOnDemand,
    /// The journal's archive lines before this run (resume only).
    archive_steps: Vec<ArchiveRecord>,
}

impl<'a> Run<'a> {
    fn new(
        plan: &'a OrganizePlan,
        env: &'a ApplyEnv<'a>,
        writer: Writer,
        resumed: bool,
        records: Vec<TransferRecord>,
    ) -> Self {
        let overrides_path = env.data_dir.join(OVERRIDES_FILE);
        let overrides = Some(Overrides::read(&overrides_path)).filter(|o| !o.is_empty());
        Run {
            plan,
            env,
            writer,
            resumed,
            index: DirIndex::new(),
            overrides_path,
            overrides,
            records,
            started: HashMap::new(),
            at: (0, 0),
            extract_need: 0,
            tools: ToolsOnDemand::new(&[&plan.source_root, &plan.target_root]),
            archive_steps: Vec::new(),
        }
    }

    fn run(
        mut self,
        pending: &[&OrganizeItem],
        done_before: usize,
        mut report: OrganizeReport,
    ) -> OrganizeReport {
        let total = done_before + pending.len();
        let mut last: Option<Instant> = None;
        for (n, item) in pending.iter().enumerate() {
            if self.env.cancel.load(Ordering::SeqCst) {
                for rest in &pending[n..] {
                    let status = ItemStatus::Skipped {
                        reason: SkipReason::Cancelled,
                    };
                    record_outcome(&mut report, rest.id, status, Vec::new());
                }
                break;
            }
            if last.is_none_or(|t| t.elapsed() >= PROGRESS_EVERY) {
                last = Some(Instant::now());
                self.progress(done_before + n, total);
            }
            self.at = (done_before + n, total);
            if matches!(item.transfer, Transfer::Extract { .. }) {
                let cluster = cluster_size(&self.plan.target_root).unwrap_or(4096);
                self.extract_need = extract_need(&pending[n..], cluster);
            }
            match self.apply_item(item) {
                Step::Done(status, targets) => {
                    record_outcome(&mut report, item.id, status, targets)
                }
                Step::Pause(reason) => {
                    pause(&mut report, reason, &pending[n..]);
                    break;
                }
                Step::PauseAs(reason, kind) => {
                    pause_as(&mut report, reason, kind, &pending[n..]);
                    break;
                }
                Step::Abort => {
                    report.resumable_items = pending[n..].iter().map(|i| i.id).collect();
                    report.job = Some(self.writer.job_id().to_string());
                    return report;
                }
            }
        }
        #[cfg(feature = "test-util")]
        LAST_DIR_READS.with(|c| c.set(self.index.read_dirs()));
        let job = self.writer.job_id().to_string();
        report.job = if report.paused.is_some() {
            self.progress(total - report.resumable_items.len(), total);
            Some(job)
        } else {
            self.progress(total, total);
            if self.writer.has_entries() || self.resumed {
                Some(self.writer.finish().unwrap_or(job))
            } else {
                // Nothing changed: no job to list or undo.
                let _ = self.writer.discard();
                let _ = fs::remove_file(plan_path(self.env.data_dir, &job));
                None
            }
        };
        report
    }

    fn progress(&self, done: usize, total: usize) {
        (self.env.progress)(OrganizeProgress {
            phase: OrganizePhase::Apply,
            done,
            total,
        });
    }

    /// A pause for errors that stop the whole run (full, read-only or
    /// vanished volume), else the item fails and the run goes on.
    fn on_error(&self, e: &WriteError) -> Step {
        if roots_gone(self.plan).is_some() {
            return Step::Pause(ItemErrorKind::VolumeGone);
        }
        match e {
            WriteError::DiskFull => Step::Pause(ItemErrorKind::DiskFull),
            WriteError::Denied(_) => Step::Pause(ItemErrorKind::WriteDenied),
            _ => Step::Done(
                ItemStatus::Failed {
                    kind: ItemErrorKind::from(e),
                    detail: e.to_string(),
                },
                Vec::new(),
            ),
        }
    }

    /// Each source must still be the file the plan saw.
    fn revalidate(&self, item: &OrganizeItem) -> Option<Step> {
        for (i, rel) in item.sources.iter().enumerate() {
            let path = self.plan.source_root.join(rel);
            let meta = match fs::symlink_metadata(&path) {
                Ok(meta) => meta,
                Err(e) if e.kind() == ErrorKind::NotFound => {
                    if roots_gone(self.plan).is_some() {
                        return Some(Step::Pause(ItemErrorKind::VolumeGone));
                    }
                    return Some(skipped(SkipReason::SourceMissing));
                }
                Err(e) => return Some(self.on_error(&WriteError::Io(e))),
            };
            if item.kind == ItemKind::Folder {
                if !self.same_tree(item, &path, &meta) {
                    return Some(skipped(SkipReason::ChangedSincePlan));
                }
                continue;
            }
            let same = meta.file_type().is_file()
                && item
                    .fps
                    .get(i)
                    .is_none_or(|fp| fp.size == meta.len() && Some(fp.mtime_ns) == mtime_ns(&meta));
            if !same {
                return Some(skipped(SkipReason::ChangedSincePlan));
            }
        }
        None
    }

    /// A folder game is still the tree the plan measured: a real folder
    /// (not a link) whose total bytes and newest mtime match (D25).
    fn same_tree(&self, item: &OrganizeItem, path: &Path, meta: &fs::Metadata) -> bool {
        if !meta.is_dir() || meta.file_type().is_symlink() {
            return false;
        }
        let opts = WalkOptions {
            skip_dirs: Vec::new(),
            roms_root: None,
            art_dirs: &[],
            ignored_dirs: &[],
            now: std::time::SystemTime::now(),
            recent: Duration::ZERO,
            probe: &OsMetaProbe,
            max_depth: 0,
            profile: None,
        };
        let tree = tree_summary(path, &opts);
        item.fps
            .first()
            .is_some_and(|fp| fp.size == tree.bytes && fp.mtime_ns == tree.newest_mtime_ns)
    }

    fn transfer(
        &mut self,
        from: &Path,
        to: &Path,
        collision: Collision,
        id: usize,
    ) -> Result<MoveOutcome, WriteError> {
        let outcome = match self.plan.mode {
            TransferMode::Move => self.writer.move_file_indexed(from, to, collision, Some(id)),
            TransferMode::Copy => self.writer.copy_file_indexed(from, to, collision, Some(id)),
        }?;
        if let MoveOutcome::Moved { to, .. }
        | MoveOutcome::Copied { to, .. }
        | MoveOutcome::CopiedNotRemoved { to, .. } = &outcome
        {
            if let Some((dir, name)) = split(to) {
                self.index.claim(dir, name);
            }
        }
        Ok(outcome)
    }

    /// The first free name for `want` in its folder by the index: itself,
    /// then `" (2)"`, `" (3)"`, …
    fn free_name(&mut self, want: &Path) -> PathBuf {
        let Some((dir, name)) = split(want) else {
            return want.to_path_buf();
        };
        if !self.index.taken(dir, name) {
            return want.to_path_buf();
        }
        let dir = dir.to_path_buf();
        (2..1000)
            .map(|n| numbered(name, n))
            .find(|candidate| !self.index.taken(&dir, candidate))
            .map_or_else(|| want.to_path_buf(), |free| dir.join(free))
    }

    fn apply_item(&mut self, item: &OrganizeItem) -> Step {
        if let Some((status, primary)) = self.started.remove(&item.id) {
            return self.finish_started(item, status, primary);
        }
        if self.resumed && matches!(item.transfer, Transfer::Extract { .. }) {
            let journal = ExtractJournal::of(self.plan, item, &self.records, &self.archive_steps);
            if journal.started() {
                return self.resume_extract(item, &journal);
            }
        }
        if item.is_set_aside() {
            return self.apply_set_aside(item);
        }
        if let Some(step) = self.revalidate(item) {
            return step;
        }
        if matches!(item.transfer, Transfer::Extract { .. }) {
            return self.apply_extract(item, &[]);
        }
        // Moving a folder game needs the directory transfer of D25, which
        // is not there yet: such an item is never treated as a file.
        if item.kind == ItemKind::Folder {
            return skipped(SkipReason::NotDecided);
        }
        let Some(target) = &item.target else {
            return skipped(SkipReason::NotDecided);
        };
        let root = &self.plan.target_root;
        let sources: Vec<PathBuf> = item
            .sources
            .iter()
            .map(|s| self.plan.source_root.join(s))
            .collect();
        let dests: Vec<PathBuf> = std::iter::once(target)
            .chain(&item.set_targets)
            .map(|t| root.join(t))
            .collect();
        let step = if item.kind == ItemKind::Set || sources.len() > 1 {
            self.apply_set(item.id, &sources, &dests)
        } else {
            self.apply_single(item.id, &sources[0], &dests[0])
        };
        let Step::Done(status, mut targets) = step else {
            return step;
        };
        if matches!(
            status,
            ItemStatus::Skipped { .. } | ItemStatus::Failed { .. }
        ) {
            return Step::Done(status, targets);
        }
        if self.plan.mode == TransferMode::Move {
            for (from, to) in sources.iter().zip(&targets) {
                self.rekey_override(item.id, from, to);
            }
        }
        if let Some(pause) = self.move_companions(item, &mut targets) {
            return pause;
        }
        Step::Done(status, targets)
    }

    /// Sets a duplicate aside (duplicates spec §7.1): its files go to the
    /// pile's quarantine as instant renames on the pile's volume, a set
    /// whole (every name must be free), or to the Trash. Files an earlier
    /// run of the job already set aside are not done again.
    fn apply_set_aside(&mut self, item: &OrganizeItem) -> Step {
        let paths = set_aside_paths(self.plan, item);
        if paths.is_empty() {
            return skipped(SkipReason::NotDecided);
        }
        let started = paths.iter().any(|(from, _)| {
            record_of(&self.records, item.id, from).is_some()
                || last_step(&self.archive_steps, item.id, from).is_some()
        });
        if !started {
            if let Some(step) = self.revalidate(item) {
                return step;
            }
        }
        if self.plan.set_aside_trash {
            return self.trash_set_aside(item, &paths);
        }
        let set = paths.len() > 1;
        if set
            && paths.iter().any(|(from, to)| {
                record_of(&self.records, item.id, from).is_none()
                    && fs::symlink_metadata(to).is_ok()
            })
        {
            return skipped(SkipReason::CollisionSet);
        }
        let collision = if set {
            Collision::Skip
        } else {
            Collision::Rename
        };
        let mut targets = Vec::with_capacity(paths.len());
        for (from, to) in &paths {
            if let Some(record) = record_of(&self.records, item.id, from) {
                targets.push(record.target.clone());
                continue;
            }
            match self
                .writer
                .set_aside_file(from, to, collision, Some(item.id))
            {
                Ok(
                    MoveOutcome::Moved { to, .. }
                    | MoveOutcome::Copied { to, .. }
                    | MoveOutcome::CopiedNotRemoved { to, .. },
                ) => targets.push(to),
                Ok(MoveOutcome::SkippedExists { existing }) => {
                    if targets.is_empty() {
                        return skipped(SkipReason::CollisionSet);
                    }
                    // Undo puts the files already set aside back.
                    let detail = format!(
                        "{} of the set stayed: {} is taken",
                        from.display(),
                        existing.display()
                    );
                    return Step::Done(
                        ItemStatus::Failed {
                            kind: ItemErrorKind::Io,
                            detail,
                        },
                        targets,
                    );
                }
                Err(e) => return self.on_error(&e),
            }
        }
        if let Some(pause) = self.set_aside_companions(item, &mut targets) {
            return pause;
        }
        Step::Done(ItemStatus::Quarantined, targets)
    }

    /// The companions of a set-aside item follow it below the quarantine
    /// (or to the Trash); one that cannot stays where it is and never fails
    /// the item, unless the volume itself is the problem (`Some`: a pause).
    fn set_aside_companions(
        &mut self,
        item: &OrganizeItem,
        targets: &mut Vec<PathBuf>,
    ) -> Option<Step> {
        let src = self.plan.source_root.clone();
        for companion in &item.companions {
            let from = src.join(&companion.source);
            if let Some(record) = record_of(&self.records, item.id, &from) {
                targets.push(record.target.clone());
                continue;
            }
            if let Some(ArchiveStep::Trashed { trash }) =
                last_step(&self.archive_steps, item.id, &from)
            {
                targets.extend(trash.clone());
                continue;
            }
            if !fs::symlink_metadata(&from).is_ok_and(|m| m.file_type().is_file()) {
                continue;
            }
            let result = if self.plan.set_aside_trash {
                trash_file(&mut self.writer, self.env.trash, &from, Some(item.id)).map(|step| {
                    if let TrashStep::Trashed { location } = step {
                        targets.extend(location);
                    }
                })
            } else {
                self.writer
                    .set_aside_file(
                        &from,
                        &src.join(&companion.target),
                        Collision::Rename,
                        Some(item.id),
                    )
                    .map(|outcome| match outcome {
                        MoveOutcome::Moved { to, .. }
                        | MoveOutcome::Copied { to, .. }
                        | MoveOutcome::CopiedNotRemoved { to, .. } => targets.push(to),
                        MoveOutcome::SkippedExists { .. } => {}
                    })
            };
            if let Err(e) = result {
                if let Step::Pause(reason) = self.on_error(&e) {
                    return Some(Step::Pause(reason));
                }
            }
        }
        None
    }

    /// Sends a duplicate's files to the Trash through the journaled
    /// Trash steps (undo brings them back from there).
    fn trash_set_aside(&mut self, item: &OrganizeItem, paths: &[(PathBuf, PathBuf)]) -> Step {
        let mut targets = Vec::new();
        for (n, (from, _)) in paths.iter().enumerate() {
            if let Some(ArchiveStep::Trashed { trash }) =
                last_step(&self.archive_steps, item.id, from)
            {
                targets.extend(trash.clone());
                continue;
            }
            match trash_file(&mut self.writer, self.env.trash, from, Some(item.id)) {
                Ok(TrashStep::Trashed { location }) => targets.extend(location),
                Ok(TrashStep::Gone) => {}
                Ok(TrashStep::Kept { .. }) if n == 0 => {
                    return skipped(SkipReason::TrashUnavailable)
                }
                Ok(TrashStep::Kept { .. }) => {
                    let detail = format!("{} stayed: the Trash refused it", from.display());
                    return Step::Done(
                        ItemStatus::Failed {
                            kind: ItemErrorKind::Io,
                            detail,
                        },
                        targets,
                    );
                }
                Err(e) => return self.on_error(&e),
            }
        }
        if let Some(pause) = self.set_aside_companions(item, &mut targets) {
            return pause;
        }
        Step::Done(ItemStatus::Trashed, targets)
    }

    /// Finishes an item whose primary file an earlier run already put at
    /// `primary`: the set members and companions still at their source
    /// follow it, as they would have in one go.
    fn finish_started(
        &mut self,
        item: &OrganizeItem,
        mut status: ItemStatus,
        primary: PathBuf,
    ) -> Step {
        let root = &self.plan.target_root;
        let mut targets = vec![primary];
        for (rel, dest) in item.sources.iter().skip(1).zip(&item.set_targets) {
            let from = self.plan.source_root.join(rel);
            if let Some(record) = record_of(&self.records, item.id, &from) {
                targets.push(record.target.clone());
                continue;
            }
            let dest = root.join(dest);
            let failed = |detail: String| ItemStatus::Failed {
                kind: ItemErrorKind::Io,
                detail,
            };
            if !fs::symlink_metadata(&from).is_ok_and(|m| m.file_type().is_file()) {
                let detail = format!("{} of the set is gone", from.display());
                return Step::Done(failed(detail), targets);
            }
            match self.transfer(&from, &dest, Collision::Skip, item.id) {
                Ok(MoveOutcome::SkippedExists { existing }) => {
                    let detail = format!(
                        "{} of the set stayed: {} is taken",
                        from.display(),
                        existing.display()
                    );
                    return Step::Done(failed(detail), targets);
                }
                Ok(outcome) => {
                    if matches!(outcome, MoveOutcome::CopiedNotRemoved { .. }) {
                        status = ItemStatus::CopiedNotRemoved;
                    }
                    if let MoveOutcome::Moved { to, .. }
                    | MoveOutcome::Copied { to, .. }
                    | MoveOutcome::CopiedNotRemoved { to, .. } = outcome
                    {
                        targets.push(to);
                    }
                }
                Err(e) => return self.on_error(&e),
            }
        }
        if self.plan.mode == TransferMode::Move {
            let sources: Vec<PathBuf> = item
                .sources
                .iter()
                .map(|s| self.plan.source_root.join(s))
                .collect();
            for (from, to) in sources.iter().zip(targets.clone()) {
                self.rekey_override(item.id, from, &to);
            }
        }
        if let Some(pause) = self.move_companions(item, &mut targets) {
            return pause;
        }
        Step::Done(status, targets)
    }

    /// Companions follow the ROM (already at `targets[0]`), under its final
    /// name; a companion an earlier run transferred is not done again.
    /// `Some` is a pause: the volume itself is the problem.
    fn move_companions(&mut self, item: &OrganizeItem, targets: &mut Vec<PathBuf>) -> Option<Step> {
        let root = &self.plan.target_root;
        let planned = item
            .target
            .as_deref()
            .and_then(Path::file_name)
            .and_then(|n| n.to_str())
            .unwrap_or("");
        let actual = targets
            .first()
            .and_then(|t| t.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("");
        let (planned_stem, actual_stem) = (stem(planned).to_string(), stem(actual).to_string());
        for companion in &item.companions {
            let from = self.plan.source_root.join(&companion.source);
            if let Some(record) = record_of(&self.records, item.id, &from) {
                targets.push(record.target.clone());
                continue;
            }
            if !fs::symlink_metadata(&from).is_ok_and(|m| m.file_type().is_file()) {
                continue;
            }
            let mut to = root.join(&companion.target);
            if planned_stem != actual_stem {
                if let Some(rest) = to
                    .file_name()
                    .and_then(|n| n.to_str())
                    .and_then(|n| n.strip_prefix(planned_stem.as_str()))
                {
                    to = to.with_file_name(format!("{actual_stem}{rest}"));
                }
            }
            let to = self.free_name(&to);
            match self.transfer(&from, &to, Collision::Rename, item.id) {
                Ok(
                    MoveOutcome::Moved { to, .. }
                    | MoveOutcome::Copied { to, .. }
                    | MoveOutcome::CopiedNotRemoved { to, .. },
                ) => targets.push(to),
                Ok(MoveOutcome::SkippedExists { .. }) => {}
                // The ROM is in place; a companion that cannot follow
                // stays where it is unless the volume itself is the
                // problem.
                Err(e) => {
                    if let Step::Pause(reason) = self.on_error(&e) {
                        return Some(Step::Pause(reason));
                    }
                }
            }
        }
        None
    }

    fn apply_single(&mut self, id: usize, from: &Path, want: &Path) -> Step {
        if let Some((dir, name)) = split(want) {
            if let Some(existing) = self.index.existing(dir, name) {
                if same_content(from, &existing) {
                    return skipped(SkipReason::AlreadyPresent);
                }
            }
        }
        let to = self.free_name(want);
        match self.transfer(from, &to, Collision::Rename, id) {
            Ok(MoveOutcome::Moved { to, .. }) => Step::Done(ItemStatus::Moved, vec![to]),
            Ok(MoveOutcome::Copied { to, .. }) => Step::Done(ItemStatus::Copied, vec![to]),
            Ok(MoveOutcome::CopiedNotRemoved { to, .. }) => {
                Step::Done(ItemStatus::CopiedNotRemoved, vec![to])
            }
            // Not with `Collision::Rename`; kept for completeness.
            Ok(MoveOutcome::SkippedExists { .. }) => skipped(SkipReason::AlreadyPresent),
            Err(e) => self.on_error(&e),
        }
    }

    /// A set moves whole or not at all: every member's name must be free,
    /// and a member that finds its name taken meanwhile puts the members
    /// already moved back.
    fn apply_set(&mut self, id: usize, sources: &[PathBuf], dests: &[PathBuf]) -> Step {
        if dests.len() != sources.len()
            || dests
                .iter()
                .any(|d| split(d).is_none_or(|(dir, name)| self.index.taken(dir, name)))
        {
            return skipped(SkipReason::CollisionSet);
        }
        let mut done: Vec<(PathBuf, MoveOutcome)> = Vec::new();
        let mut status = match self.plan.mode {
            TransferMode::Move => ItemStatus::Moved,
            TransferMode::Copy => ItemStatus::Copied,
        };
        for (from, to) in sources.iter().zip(dests) {
            match self.transfer(from, to, Collision::Skip, id) {
                Ok(MoveOutcome::SkippedExists { .. }) => {
                    let stayed = roll_back(&done);
                    if stayed.is_empty() {
                        return skipped(SkipReason::CollisionSet);
                    }
                    // A move across volumes cannot be renamed back: say
                    // where those members are (undo puts them back).
                    let status = ItemStatus::Failed {
                        kind: ItemErrorKind::Io,
                        detail: "a file of the set is taken at the target and the files \
                                 already moved could not go back"
                            .into(),
                    };
                    return Step::Done(status, stayed);
                }
                Ok(outcome) => {
                    if matches!(outcome, MoveOutcome::CopiedNotRemoved { .. }) {
                        status = ItemStatus::CopiedNotRemoved;
                    }
                    done.push((from.clone(), outcome));
                }
                Err(e) => {
                    let _ = roll_back(&done);
                    return self.on_error(&e);
                }
            }
        }
        let targets = done
            .into_iter()
            .filter_map(|(_, o)| match o {
                MoveOutcome::Moved { to, .. }
                | MoveOutcome::Copied { to, .. }
                | MoveOutcome::CopiedNotRemoved { to, .. } => Some(to),
                MoveOutcome::SkippedExists { .. } => None,
            })
            .collect();
        Step::Done(status, targets)
    }

    /// Extracts an `Extract` item (archives spec §9.1): the external tool
    /// is the one the plan used, the volume has room for what is left to
    /// extract, then phases A–C, then (move mode, nothing left behind) the
    /// archive goes to the Trash once the reader has closed it. `adopted`
    /// entries (resume) are in place already: only checked, never
    /// rewritten.
    fn apply_extract(&mut self, item: &OrganizeItem, adopted: &[Adopted]) -> Step {
        let Transfer::Extract {
            entries,
            archive_after,
            source,
            sheet_check,
            m3u,
            ..
        } = &item.transfer
        else {
            return skipped(SkipReason::NotDecided);
        };
        let Some(rel) = item.sources.first() else {
            return skipped(SkipReason::SourceMissing);
        };
        let archive = self.plan.source_root.join(rel);
        let tool: Vec<FoundTool> = match source {
            ExtractSource::Builtin => Vec::new(),
            ExtractSource::Tool { tool, version } => {
                let found = match self.env.tools {
                    Some(tools) => tools,
                    None => self.tools.get(),
                };
                match found
                    .iter()
                    .find(|t| t.tool == *tool && t.version == *version)
                {
                    Some(t) => vec![t.clone()],
                    None => return skipped(SkipReason::ToolGone),
                }
            }
        };
        if self.extract_need > 0 {
            if let Some(free) =
                user_available(&self.plan.target_root).filter(|&f| f < self.extract_need)
            {
                let needed = self.extract_need;
                return Step::PauseAs(
                    ItemErrorKind::DiskFull,
                    PauseKind::DiskFull { needed, free },
                );
            }
        }
        let limits = Limits {
            external_tools: !tool.is_empty(),
            ..Limits::default()
        };
        let mut reader = match open_archive_with(&archive, &limits, Some(&tool)) {
            Ok(Some(reader)) => reader,
            Ok(None) => {
                return failed(ItemErrorKind::Decode, "not an archive any more".into());
            }
            Err(e) => return failed(ItemErrorKind::Decode, e.to_string()),
        };
        if !tool.is_empty()
            && reader.tool().map(|t| (&t.tool, &t.version))
                != tool.first().map(|t| (&t.tool, &t.version))
        {
            return skipped(SkipReason::ToolGone);
        }
        let bytes_total = decode_total(reader.entries(), entries);
        let (done, total) = self.at;
        let progress = self.env.progress;
        let item_id = item.id;
        let entries_total = entries.len();
        let report = |bytes_done: u64, entries_done: usize| {
            progress(OrganizeProgress {
                phase: OrganizePhase::Extract {
                    item: item_id,
                    bytes_done,
                    bytes_total,
                    entries_done,
                    entries_total,
                },
                done,
                total,
            })
        };
        report(0, 0);
        let last = std::cell::Cell::new(Instant::now());
        let all_staged = std::cell::Cell::new(false);
        let on_tick = |tick: ExtractTick| {
            // The count rides the 100 ms cadence; only the moment every
            // entry is staged goes out at once (phase B starts and no tick
            // follows it).
            let edge = tick.entries_done == entries_total && !all_staged.replace(true);
            if edge || last.get().elapsed() >= PROGRESS_EVERY {
                last.set(Instant::now());
                report(tick.bytes, tick.entries_done);
            }
        };
        let fp = item.fps.first().copied();
        let cancel = self.env.cancel;
        let writer = &mut self.writer;
        let committed = with_linked_cancel(cancel, |flag| {
            extract_and_commit(
                writer,
                reader.as_mut(),
                &CommitRequest {
                    extract: ExtractRequest {
                        item: item.id,
                        archive: &archive,
                        entries,
                        cancel: flag,
                        progress: &on_tick,
                    },
                    archive_fp: fp,
                    sheet_check: *sheet_check,
                    m3u: m3u.as_deref(),
                    adopted,
                },
            )
        });
        // Closed before the Trash: Windows cannot move an open file.
        drop(reader);
        let targets = match committed {
            Ok(targets) => targets,
            Err(e) => return self.on_extract_error(e),
        };
        report(bytes_total, entries_total);
        for t in &targets {
            if let Some((dir, name)) = split(t) {
                self.index.claim(dir, name);
            }
        }
        if !trashes(self.plan, *archive_after) {
            return Step::Done(ItemStatus::Extracted, targets);
        }
        self.dispose(item.id, &archive, fp, targets)
    }

    /// Phase D: the archive to the Trash (it must still be the planned
    /// file), and the item's status by what became of it.
    fn dispose(
        &mut self,
        id: usize,
        archive: &Path,
        fp: Option<crate::organize::plan::SourceFp>,
        targets: Vec<PathBuf>,
    ) -> Step {
        match dispose_archive(&mut self.writer, id, archive, fp, self.env.trash) {
            ArchiveFate::Trashed { .. } => Step::Done(ItemStatus::Extracted, targets),
            ArchiveFate::Kept { .. } => Step::Done(ItemStatus::ExtractedKept, targets),
            ArchiveFate::Aborted => Step::Abort,
        }
    }

    /// Resumes an `Extract` item an earlier run started (archives spec
    /// §9.3; leftover job temps were deleted when the run began):
    /// - every entry complete and its final intact: only the playlist and
    ///   the archive's fate are left (a Trash intent without a follow-up
    ///   is trashed again, or looked up in the Trash when the archive is
    ///   gone from its place);
    /// - else, the archive changed or vanished since the plan: everything
    ///   the item put in place is rolled back (each final only while it is
    ///   still the job's file);
    /// - else phases A–C again: complete finals that still match their
    ///   line, and finals of an intent that match the listing's size and a
    ///   full CRC re-read, are adopted (decoded to a sink and checked,
    ///   never rewritten); the rest is extracted again.
    fn resume_extract(&mut self, item: &OrganizeItem, journal: &ExtractJournal) -> Step {
        let Transfer::Extract {
            entries,
            archive_after,
            m3u,
            ..
        } = &item.transfer
        else {
            return skipped(SkipReason::NotDecided);
        };
        let Some(rel) = item.sources.first() else {
            return skipped(SkipReason::SourceMissing);
        };
        if let Some(reason) = roots_gone(self.plan) {
            return Step::Pause(reason);
        }
        let root = self.plan.target_root.clone();
        let archive = self.plan.source_root.join(rel);
        let fp = item.fps.first().copied();
        let logs = entries.iter().zip(&journal.entries);
        let intact = logs.clone().all(|(e, log)| match log {
            EntryLog::Completed { fp, .. } => file_matches_fp(&root.join(&e.target), fp),
            _ => false,
        });
        if intact && journal.entries.len() == entries.len() {
            let mut targets: Vec<PathBuf> = entries.iter().map(|e| root.join(&e.target)).collect();
            if let Some(m3u) = m3u {
                let m3u = root.join(m3u);
                if let Err(step) = self.ensure_m3u(item.id, entries, &m3u) {
                    return step;
                }
                targets.push(m3u);
            }
            return self.resume_archive(item, &archive, *archive_after, journal, targets);
        }
        if !archive_unchanged(&archive, fp) {
            let missing = !fs::symlink_metadata(&archive).is_ok_and(|m| m.file_type().is_file());
            self.roll_back_started(item.id, entries, journal);
            return skipped(if missing {
                SkipReason::SourceMissing
            } else {
                SkipReason::ChangedSincePlan
            });
        }
        let mut adopted = Vec::new();
        for (e, log) in logs {
            let target = root.join(&e.target);
            match log {
                EntryLog::None => {}
                EntryLog::Completed { temp, fp } => {
                    if file_matches_fp(&target, fp) {
                        let Some(crc32) = fp.crc32.or_else(|| crc_of(&target)) else {
                            return failed(
                                ItemErrorKind::Io,
                                format!("{} unreadable", target.display()),
                            );
                        };
                        adopted.push(Adopted {
                            index: e.index,
                            temp: temp.clone(),
                            crc32,
                            journaled: true,
                        });
                    } else if fs::symlink_metadata(&target).is_ok() {
                        let detail = format!("{} changed since it was extracted", target.display());
                        return failed(ItemErrorKind::Io, detail);
                    }
                    // Gone since: extracted again under a new intent.
                }
                EntryLog::Intent { temp } => {
                    let same_size = fs::symlink_metadata(&target)
                        .is_ok_and(|m| m.file_type().is_file() && m.len() == e.size);
                    let crc = same_size.then(|| crc_of(&target)).flatten();
                    match crc.filter(|c| e.crc32.is_none_or(|want| want == *c)) {
                        Some(crc32) => adopted.push(Adopted {
                            index: e.index,
                            temp: temp.clone(),
                            crc32,
                            journaled: false,
                        }),
                        None => {
                            // Never renamed (its temp was a leftover), or
                            // the file there is not the entry: the intent
                            // is closed, the entry extracted again.
                            if let Err(e) = self.writer.rolled_back(&target, temp, Some(item.id)) {
                                return self.on_error(&e);
                            }
                        }
                    }
                }
            }
        }
        // The adopted finals take their room already: the free-space
        // check counts only what is left to extract (archives spec §8.4).
        if !adopted.is_empty() {
            let cluster = cluster_size(&self.plan.target_root).unwrap_or(4096).max(1);
            let in_place = entries
                .iter()
                .filter(|e| adopted.iter().any(|a| a.index == e.index))
                .map(|e| e.size.div_ceil(cluster).saturating_mul(cluster))
                .fold(0u64, u64::saturating_add);
            self.extract_need = self.extract_need.saturating_sub(in_place);
        }
        self.apply_extract(item, &adopted)
    }

    /// The generated playlist of a resumed item whose entries are all in
    /// place: written when it is missing; one with the very text is kept.
    fn ensure_m3u(&mut self, id: usize, entries: &[ExtractEntry], m3u: &Path) -> Result<(), Step> {
        let sheets: Vec<PathBuf> = entries
            .iter()
            .filter(|e| {
                let ext = e.name.rsplit_once('.').map_or("", |(_, x)| x);
                matches!(
                    SheetKind::from_extension(ext),
                    Some(SheetKind::Cue | SheetKind::Gdi)
                )
            })
            .map(|e| self.plan.target_root.join(&e.target))
            .collect();
        let refs: Vec<&Path> = sheets.iter().map(PathBuf::as_path).collect();
        let Some(text) = m3u_text(&refs, m3u) else {
            return Err(failed(
                ItemErrorKind::Io,
                format!("{} cannot list the discs", m3u.display()),
            ));
        };
        if fs::symlink_metadata(m3u).is_ok() {
            return match fs::read(m3u) {
                Ok(bytes) if bytes == text.as_bytes() => Ok(()),
                _ => Err(skipped(SkipReason::CollisionSet)),
            };
        }
        match self.writer.create_new_file(m3u, text.as_bytes(), Some(id)) {
            Ok(()) => Ok(()),
            Err(WriteError::Io(e)) if e.kind() == ErrorKind::AlreadyExists => {
                Err(skipped(SkipReason::CollisionSet))
            }
            Err(e) => Err(self.on_error(&e)),
        }
    }

    /// What is left of phase D for a resumed item whose files are all in
    /// place.
    fn resume_archive(
        &mut self,
        item: &OrganizeItem,
        archive: &Path,
        after: ArchiveAfter,
        journal: &ExtractJournal,
        targets: Vec<PathBuf>,
    ) -> Step {
        if !trashes(self.plan, after) {
            return Step::Done(ItemStatus::Extracted, targets);
        }
        let at_from = fs::symlink_metadata(archive).is_ok_and(|m| m.file_type().is_file());
        match &journal.archive {
            Some(ArchiveStep::Trashed { .. }) => Step::Done(ItemStatus::Extracted, targets),
            Some(ArchiveStep::Kept) => Step::Done(ItemStatus::ExtractedKept, targets),
            // Killed during the Trash call: the archive left; only the
            // Trash knows where it went.
            Some(ArchiveStep::TrashIntent { fp }) if !at_from => {
                let name = archive.file_name().and_then(|n| n.to_str());
                let found = match (name, fp) {
                    (Some(name), Some(fp)) => self.env.trash.find(archive, name, fp),
                    _ => None,
                };
                if let Err(e) =
                    self.writer
                        .archive_trashed(archive, found.as_deref(), Some(item.id))
                {
                    return self.on_error(&e);
                }
                Step::Done(ItemStatus::Extracted, targets)
            }
            // Killed before the Trash call, or before phase D began.
            _ if at_from => self.dispose(item.id, archive, item.fps.first().copied(), targets),
            // Gone from its place without a line saying so: nothing left
            // to send to the Trash.
            _ => Step::Done(ItemStatus::Extracted, targets),
        }
    }

    /// Rolls back what earlier runs put in place for an item whose archive
    /// changed since the plan: each final that is still the job's file (a
    /// complete one by its fingerprint, one with only an intent by the
    /// listing's size and a full CRC re-read) is deleted, and every open
    /// intent gets its `rolled-back` line (one whose final could not be
    /// deleted stays open for undo). Best effort.
    fn roll_back_started(&mut self, id: usize, entries: &[ExtractEntry], journal: &ExtractJournal) {
        for (e, log) in entries.iter().zip(&journal.entries) {
            let target = self.plan.target_root.join(&e.target);
            let (temp, ours) = match log {
                EntryLog::None => continue,
                EntryLog::Completed { temp, fp } => (temp, file_matches_fp(&target, fp)),
                EntryLog::Intent { temp } => {
                    let ours = e.crc32.is_some()
                        && fs::symlink_metadata(&target)
                            .is_ok_and(|m| m.file_type().is_file() && m.len() == e.size)
                        && crc_of(&target) == e.crc32;
                    (temp, ours)
                }
            };
            if ours {
                if fs::remove_file(&target).is_err() {
                    // Still in place: its lines stay open, so undo still
                    // knows it as the job's file.
                    continue;
                }
                remove_apple_doubles(std::slice::from_ref(&target));
            }
            let _ = self.writer.rolled_back(&target, temp, Some(id));
        }
    }

    /// How a failed extraction ends: rolled back in every case; a full
    /// volume pauses the job, a write the volume refuses pauses as for a
    /// move, anything else skips or fails the item only.
    fn on_extract_error(&self, e: ExtractError) -> Step {
        if let Some(PauseKind::DiskFull { needed, free }) = e.pause() {
            return Step::PauseAs(
                ItemErrorKind::DiskFull,
                PauseKind::DiskFull { needed, free },
            );
        }
        match e {
            ExtractError::Aborted => Step::Abort,
            ExtractError::Cancelled => skipped(SkipReason::Cancelled),
            ExtractError::ChangedSincePlan | ExtractError::ListingChanged { .. } => {
                skipped(SkipReason::ChangedSincePlan)
            }
            ExtractError::Collision { .. } => skipped(SkipReason::CollisionSet),
            ExtractError::UnsafeTarget { .. } => failed(ItemErrorKind::OutsideRoot, e.to_string()),
            ExtractError::Write(w) => self.on_error(&w),
            ExtractError::Archive(_)
            | ExtractError::EntryMissing { .. }
            | ExtractError::SetIncomplete { .. } => failed(ItemErrorKind::Decode, e.to_string()),
            ExtractError::RereadMismatch { .. } | ExtractError::DiskFull { .. } => {
                failed(ItemErrorKind::Io, e.to_string())
            }
        }
    }

    /// The manual identity keyed by `from` moves to `to`: journaled first
    /// so undo puts it back; best effort (a failure keeps the old key).
    fn rekey_override(&mut self, id: usize, from: &Path, to: &Path) {
        if from == to
            || !self
                .overrides
                .as_ref()
                .is_some_and(|o| o.get(from).is_some())
        {
            return;
        }
        if self
            .writer
            .record_override_moved(from, to, Some(id))
            .is_err()
        {
            return;
        }
        let mut current = Overrides::load_for_update(&self.overrides_path);
        let Some(identity) = current.get(from).cloned() else {
            return;
        };
        current.remove(from);
        current.set(to, identity);
        let _ = current.save(&self.overrides_path);
    }
}

fn skipped(reason: SkipReason) -> Step {
    Step::Done(ItemStatus::Skipped { reason }, Vec::new())
}

fn failed(kind: ItemErrorKind, detail: String) -> Step {
    Step::Done(ItemStatus::Failed { kind, detail }, Vec::new())
}

/// Puts the members of a set that were already moved back, newest first;
/// copies are deleted. Best effort: returns the members that stayed at
/// the target (in the journal, so undo handles them; a resume finishes
/// the set when its first file stayed).
fn roll_back(done: &[(PathBuf, MoveOutcome)]) -> Vec<PathBuf> {
    let mut stayed = Vec::new();
    for (from, outcome) in done.iter().rev() {
        let (to, back) = match outcome {
            MoveOutcome::Moved { to, .. } if to != from => (to, rename_noclobber(to, from)),
            MoveOutcome::Copied { to, .. } | MoveOutcome::CopiedNotRemoved { to, .. } => {
                (to, fs::remove_file(to))
            }
            _ => continue,
        };
        if back.is_err() {
            stayed.push(to.clone());
        }
    }
    stayed.reverse();
    stayed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbered_names_match_the_writer() {
        assert_eq!(numbered("Tetris.gb", 2), "Tetris (2).gb");
        assert_eq!(numbered("README", 3), "README (3)");
        assert_eq!(stem("Game (2).cue"), "Game (2)");
        assert_eq!(stem(".hidden"), ".hidden");
    }

    #[test]
    fn reports_round_trip_through_json() {
        let report = OrganizeReport {
            job: Some("j".into()),
            moved: 1,
            items: vec![
                ItemOutcome {
                    item: 3,
                    status: ItemStatus::Skipped {
                        reason: SkipReason::ChangedSincePlan,
                    },
                    targets: Vec::new(),
                },
                ItemOutcome {
                    item: 4,
                    status: ItemStatus::Failed {
                        kind: ItemErrorKind::Io,
                        detail: "x".into(),
                    },
                    targets: vec![PathBuf::from("/t/a.gb")],
                },
            ],
            paused: Some(PauseKind::Volume),
            pause_reason: Some(ItemErrorKind::DiskFull),
            ..OrganizeReport::default()
        };
        let json = serde_json::to_value(&report).unwrap();
        assert_eq!(json["items"][0]["status"], "skipped");
        assert_eq!(json["items"][0]["reason"], "changed-since-plan");
        assert_eq!(json["pause_reason"], "disk-full");
        let back: OrganizeReport = serde_json::from_value(json).unwrap();
        assert_eq!(back, report);
    }

    #[test]
    fn a_panic_inside_a_linked_cancel_unwinds_instead_of_hanging() {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let outer = AtomicBool::new(false);
            let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                with_linked_cancel(&outer, |_| -> () { panic!("a bug inside an extraction") })
            }));
            let _ = tx.send(caught.is_err());
        });
        let unwound = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("with_linked_cancel hung after a panic");
        assert!(unwound);
    }
}
