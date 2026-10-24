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
use crate::identify::overrides::{Overrides, OVERRIDES_FILE};
use crate::jobs::ItemErrorKind;
use crate::organize::extract::{
    decode_total, dispose_archive, extract_and_commit, ArchiveFate, CommitRequest, ExtractError,
    ExtractRequest,
};
use crate::organize::plan::{
    ArchiveAfter, ExtractSource, ItemKind, OrganizeAction, OrganizeItem, OrganizePhase,
    OrganizePlan, OrganizeProgress, Transfer, TransferMode, SPACE_RESERVE,
};
use crate::organize::target::DirIndex;
use crate::progress::PauseKind;
use crate::write::trash::TrashBackend;
use crate::write::{
    job_transfers, rename_noclobber, same_volume, Collision, JobKind, JobMeta, MoveOutcome,
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
    let applicable: Vec<&OrganizeItem> = plan.items.iter().filter(|i| applicable(i)).collect();
    let mut pending = Vec::new();
    let mut started = HashMap::new();
    for item in &applicable {
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
    Ok(run.run(&pending, done, report))
}

/// Items apply moves: automatic ones with a target (review items become
/// automatic only through the user's decisions).
fn applicable(item: &OrganizeItem) -> bool {
    item.action == OrganizeAction::Auto && item.target.is_some()
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
    let sum: u64 = items.iter().map(|i| i.size).sum();
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
        let out = f(&inner);
        over.store(true, Ordering::SeqCst);
        out
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
    let mut file = File::open(path).ok()?;
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
        if let Some(step) = self.revalidate(item) {
            return step;
        }
        if matches!(item.transfer, Transfer::Extract { .. }) {
            return self.apply_extract(item);
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
    /// archive goes to the Trash once the reader has closed it.
    fn apply_extract(&mut self, item: &OrganizeItem) -> Step {
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
        let report = |bytes_done: u64| {
            progress(OrganizeProgress {
                phase: OrganizePhase::Extract {
                    bytes_done,
                    bytes_total,
                },
                done,
                total,
            })
        };
        report(0);
        let last = std::cell::Cell::new(Instant::now());
        let on_bytes = |bytes: u64| {
            if last.get().elapsed() >= PROGRESS_EVERY {
                last.set(Instant::now());
                report(bytes);
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
                        progress: &on_bytes,
                    },
                    archive_fp: fp,
                    sheet_check: *sheet_check,
                    m3u: m3u.as_deref(),
                },
            )
        });
        // Closed before the Trash: Windows cannot move an open file.
        drop(reader);
        let targets = match committed {
            Ok(targets) => targets,
            Err(e) => return self.on_extract_error(e),
        };
        report(bytes_total);
        for t in &targets {
            if let Some((dir, name)) = split(t) {
                self.index.claim(dir, name);
            }
        }
        let trash = *archive_after == ArchiveAfter::Trash && self.plan.mode == TransferMode::Move;
        if !trash {
            return Step::Done(ItemStatus::Extracted, targets);
        }
        match dispose_archive(&mut self.writer, item.id, &archive, fp, self.env.trash) {
            ArchiveFate::Trashed { .. } => Step::Done(ItemStatus::Extracted, targets),
            ArchiveFate::Kept { .. } => Step::Done(ItemStatus::ExtractedKept, targets),
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
}
