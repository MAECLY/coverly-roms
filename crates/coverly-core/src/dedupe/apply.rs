//! Setting duplicate copies aside (duplicates spec §7–§9).
//!
//! [`apply_dedupe`] applies the user's decisions to a copy of the plan,
//! checks that every group keeps one copy, preflights the destination
//! (free space across volumes, FAT32 sizes, names, path lengths, the
//! Trash of each volume), then moves each extra, its travelling
//! companions and art into `<quarantine>/<system folder>/<rel_dir>/` (or
//! sends them to the Trash) through one journaled [`Writer`]. Errors of
//! the volume pause the run; [`resume_dedupe`] finishes the same job.
//!
//! The Trash action reuses the journaled Trash backend organise built for
//! archives (`write::trash`, plan deviation AP-3): the same three journal
//! ops carry a set-aside ROM and its companions, so undo brings them back
//! from the exact place the OS put them.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs::{self, File};
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::cache::{Cache, ContentRow};
use crate::gamelist::{rel_slash, remove_from};
use crate::jobs::ItemErrorKind;
use crate::organize::apply::{user_available, ApplyError};
use crate::organize::target::{file_system_is_fat32, DirIndex};
use crate::volumes::volume_uuid;
use crate::write::trash::{TrashBackend, TrashError};
use crate::write::{
    ensure_within, job_archive_steps, job_transfers, safe_file_name, same_volume, ArchiveRecord,
    ArchiveStep, Collision, JobKind, JobMeta, MoveOutcome, TransferRecord, WriteError, Writer,
};

use super::plan::{set_keeper, skip_group};
use super::units::mtime_ns;
use super::{DedupeDecisions, DedupeError, DedupePlan, GroupAction, Member, ReportWhy};

/// FAT32 cannot store a file of this size or larger.
const FAT32_LIMIT: u64 = 4 << 30;
/// Longest file name most file systems store, in bytes and UTF-16 units.
const MAX_NAME: usize = 255;
/// Longest path Windows tools cope with, in UTF-16 units.
const WINDOWS_MAX_PATH: usize = 259;
/// Longest path macOS and Linux calls take, in bytes.
const UNIX_MAX_PATH: usize = 1023;
/// Progress is reported at most this often.
const PROGRESS_EVERY: Duration = Duration::from_millis(100);

/// What happened to one extra copy.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum DedupeItemStatus {
    /// In the quarantine; `to` is where its primary file landed.
    Quarantined {
        to: PathBuf,
    },
    /// In the Trash, at `location` when the OS said where.
    Trashed {
        location: Option<PathBuf>,
    },
    Skipped {
        reason: DedupeSkip,
    },
    Failed {
        kind: ItemErrorKind,
        detail: String,
    },
}

/// Why an extra copy stayed where it was.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum DedupeSkip {
    /// The copy that stays changed or vanished since the plan: the whole
    /// group is left alone (hard rule 3).
    KeeperChanged,
    /// A file of this copy changed since the plan (hard rule 9).
    ChangedSincePlan,
    SourceMissing,
    /// The destination cannot hold it (hard rule 13).
    DestinationLimit {
        why: DestLimit,
    },
    /// Its volume has no Trash.
    TrashUnavailable,
    /// Real saves that differ (spec §8.3).
    Conflict,
    /// The group is only reported (spec §7.3).
    ReportOnly,
    /// You left the group or its system out.
    Excluded,
    Cancelled,
    /// A file of the set already sits in the quarantine under that name:
    /// a set is never renamed by halves.
    Taken,
}

/// What the destination cannot hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DestLimit {
    /// 4 GiB or more on FAT32.
    Fat32Size,
    /// A name FAT, exFAT or NTFS reject (`: * ? " < > |`, a trailing dot
    /// or space).
    BadName,
    PathTooLong,
    /// A file of a set whose name only differs by case from one there.
    CaseCollision,
}

/// Things worth saying about a finished run.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum DedupeApplyNote {
    /// The game list of `system` lies outside the folder Coverly writes
    /// into (a quarantine on another volume): its entries stay.
    GamelistNotUpdated { system: String },
    /// Art left with a copy and `keeper` has none in that slot now.
    ArtLeaves { keeper: PathBuf },
}

/// One extra copy and what happened to it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DedupeOutcome {
    pub group: u64,
    pub member: usize,
    #[serde(flatten)]
    pub status: DedupeItemStatus,
    /// Where its files are now: the unit's files, then companions and art.
    pub files: Vec<PathBuf>,
}

/// What an apply (or a resume) did.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DedupeReport {
    /// The journal of the job; `None` when nothing changed.
    pub job: Option<String>,
    pub outcomes: Vec<DedupeOutcome>,
    /// Bytes of the copies set aside (they left the ROMs folder).
    pub bytes_out: u64,
    /// Copies that stay without art the extras took away.
    pub art_left: Vec<PathBuf>,
    /// Set when the run stopped on a full, read-only or vanished volume.
    pub paused: Option<ItemErrorKind>,
    /// `(group, member)` not attempted yet: what a resume does.
    pub resumable: Vec<(u64, usize)>,
    pub notes: Vec<DedupeApplyNote>,
}

/// What apply needs besides the plan.
pub struct DedupeApplyEnv<'a> {
    pub data_dir: &'a Path,
    pub trash: &'a dyn TrashBackend,
    /// Keeper picks are remembered here and content rows follow moves.
    pub cache: &'a Cache,
    /// `(done, total)` extras, at most every 100 ms.
    pub progress: &'a (dyn Fn(usize, usize) + Sync),
    /// Checked between groups, never inside a set.
    pub cancel: &'a AtomicBool,
}

/// Applies `plan` with `decisions`: the extras of every acting group go to
/// the quarantine (or the Trash), journaled so undo puts them back. The
/// plan, decisions applied, is saved next to the journal before the first
/// move, so a paused or interrupted job can be resumed.
pub fn apply_dedupe(
    plan: &DedupePlan,
    decisions: &DedupeDecisions,
    env: &DedupeApplyEnv,
) -> Result<DedupeReport, ApplyError> {
    let mut plan = plan.clone();
    decide(&mut plan, decisions, env.cache)?;
    check_plan(&plan)?;
    let works = works(&plan);
    let mut report = DedupeReport::default();
    left_alone(&plan, &mut report);
    if works.is_empty() {
        return Ok(report);
    }
    let layout = Layout::of(&plan);
    if !plan.root.is_dir() {
        pause_all(&plan, &works, &mut report, ItemErrorKind::VolumeGone);
        return Ok(report);
    }
    check_inside(&plan, &layout)?;
    check_space(&plan, &works)?;
    if layout.source_root.is_some() {
        fs::create_dir_all(&layout.root).map_err(WriteError::from)?;
    }
    let writer = Writer::begin_with(
        &layout.root,
        env.data_dir,
        JobMeta {
            kind: JobKind::Dedupe,
            source_root: layout.source_root.clone(),
        },
    )?;
    if let Err(e) = save_plan(env.data_dir, writer.job_id(), &plan) {
        let _ = writer.discard();
        return Err(e.into());
    }
    let run = Run::new(&plan, env, writer, layout, Vec::new(), Vec::new(), false);
    Ok(run.run(&works, report))
}

/// Continues the paused or interrupted duplicates job `job` from its saved
/// plan: extras the journal shows as started are finished, the rest are
/// revalidated and set aside into the same journal.
pub fn resume_dedupe(job: &str, env: &DedupeApplyEnv) -> Result<DedupeReport, ApplyError> {
    let plan = load_dedupe_plan(env.data_dir, job)?;
    // The plan outlives an undo; the journal is renamed by it.
    let undone = env
        .data_dir
        .join("jobs")
        .join(format!("{job}.undone.jsonl"));
    if undone.exists() {
        return Err(ApplyError::Undone(job.to_string()));
    }
    check_plan(&plan)?;
    let works = works(&plan);
    let mut report = DedupeReport::default();
    left_alone(&plan, &mut report);
    let layout = Layout::of(&plan);
    if !plan.root.is_dir() || !layout.root.is_dir() {
        report.job = Some(job.to_string());
        pause_all(&plan, &works, &mut report, ItemErrorKind::VolumeGone);
        return Ok(report);
    }
    check_inside(&plan, &layout)?;
    let writer = Writer::resume(&layout.root, env.data_dir, job)?;
    // The journal must be this plan's: a duplicates job taking files from
    // the library the plan names, whatever its header was edited to say.
    let source = layout.source_root.as_deref().unwrap_or(&layout.root);
    if writer.kind() != JobKind::Dedupe || writer.source_root() != source {
        return Err(WriteError::Journal(format!(
            "job {job} is not a duplicates job of {}",
            plan.root.display()
        ))
        .into());
    }
    let records = job_transfers(env.data_dir, job)?;
    let steps = job_archive_steps(env.data_dir, job)?;
    let dirs: Vec<PathBuf> = quarantine_dirs(&plan, &works)
        .into_iter()
        .filter(|d| ensure_within(&layout.root, d).is_ok())
        .collect();
    writer.clean_temps(&dirs);
    let run = Run::new(&plan, env, writer, layout, records, steps, true);
    Ok(run.run(&works, report))
}

/// The plan saved with duplicates job `job` (what [`resume_dedupe`]
/// continues from). [`ApplyError::NoPlan`] when there is none.
pub fn load_dedupe_plan(data_dir: &Path, job: &str) -> Result<DedupePlan, ApplyError> {
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

fn plan_path(data_dir: &Path, job: &str) -> PathBuf {
    data_dir.join("jobs").join(format!("{job}.plan.json"))
}

fn save_plan(data_dir: &Path, job: &str, plan: &DedupePlan) -> Result<(), WriteError> {
    let text = serde_json::to_vec_pretty(plan).map_err(|e| WriteError::Journal(e.to_string()))?;
    let mut file = File::create_new(plan_path(data_dir, job))?;
    io::Write::write_all(&mut file, &text)?;
    file.sync_all()?;
    Ok(())
}

/// The user's decisions applied to the plan: picks first (remembered by
/// content for later runs), then the groups and systems left out.
fn decide(plan: &mut DedupePlan, d: &DedupeDecisions, cache: &Cache) -> Result<(), DedupeError> {
    for &(group, member) in &d.keepers {
        let g = plan
            .groups
            .iter()
            .find(|g| g.id == group)
            .ok_or(DedupeError::NoGroup(group))?;
        if member >= g.members.len() {
            return Err(DedupeError::NoMember { group, member });
        }
        if g.keeper != member {
            set_keeper(plan, group, member, cache)?;
        }
    }
    for &group in &d.skipped_groups {
        if !plan.groups.iter().any(|g| g.id == group) {
            return Err(DedupeError::NoGroup(group));
        }
        skip_group(plan, group, true);
    }
    let systems: Vec<u64> = plan
        .groups
        .iter()
        .filter(|g| d.skipped_systems.contains(&g.system))
        .map(|g| g.id)
        .collect();
    for group in systems {
        skip_group(plan, group, true);
    }
    Ok(())
}

/// Every acting group keeps exactly one copy that does not move (hard
/// rule 4), checked again here whatever the plan says.
fn check_plan(plan: &DedupePlan) -> Result<(), DedupeError> {
    for g in plan.groups.iter().filter(|g| g.action == GroupAction::Act) {
        let Some(keeper) = g.members.get(g.keeper) else {
            return Err(DedupeError::NoMember {
                group: g.id,
                member: g.keeper,
            });
        };
        let every = g.members.len() < 2
            || keeper.dest.is_some()
            || g.members.iter().enumerate().any(|(i, m)| {
                i != g.keeper
                    && (m.dest.is_some() == plan.trash
                        || m.unit.files.iter().any(|f| keeper.unit.files.contains(f))
                        || m.dest.as_deref() == Some(m.unit.primary.as_path()))
            });
        if every {
            return Err(DedupeError::EveryMemberMoves(g.id));
        }
    }
    // No extra may carry away a file a copy that stays uses: a keeper's
    // files and companions, a shared companion, or any file of a group that
    // does not act, in this group or another one.
    let mut stays: BTreeSet<&Path> = BTreeSet::new();
    for g in &plan.groups {
        for m in &g.members {
            let stays_put = g.action != GroupAction::Act || m.id == g.keeper;
            if stays_put {
                stays.extend(m.unit.files.iter().map(PathBuf::as_path));
                stays.extend(m.travels.iter().map(|t| t.from.as_path()));
            }
            stays.extend(m.stays.iter().map(|s| s.path.as_path()));
        }
    }
    for g in plan.groups.iter().filter(|g| g.action == GroupAction::Act) {
        let takes = g
            .members
            .iter()
            .filter(|m| m.id != g.keeper)
            .flat_map(|m| {
                let travels = m.travels.iter().map(|t| &t.from);
                m.unit.files.iter().chain(travels)
            })
            .any(|f| stays.contains(f.as_path()));
        if takes {
            return Err(DedupeError::EveryMemberMoves(g.id));
        }
    }
    Ok(())
}

/// One extra copy to set aside: indices into the plan, and its item number
/// in the journal.
#[derive(Clone, Copy, Debug)]
struct Work {
    g: usize,
    m: usize,
    item: usize,
}

/// The extras of the acting groups, in plan order.
fn works(plan: &DedupePlan) -> Vec<Work> {
    let mut out = Vec::new();
    for (g, group) in plan.groups.iter().enumerate() {
        if group.action != GroupAction::Act {
            continue;
        }
        for m in (0..group.members.len()).filter(|&m| m != group.keeper) {
            let item = out.len();
            out.push(Work { g, m, item });
        }
    }
    out
}

/// The copies of groups that do not act, reported as skipped.
fn left_alone(plan: &DedupePlan, report: &mut DedupeReport) {
    for g in &plan.groups {
        let reason = match &g.action {
            GroupAction::Act => continue,
            GroupAction::Conflict { .. } => DedupeSkip::Conflict,
            GroupAction::ReportOnly {
                why: ReportWhy::UserSkipped,
            } => DedupeSkip::Excluded,
            GroupAction::ReportOnly { .. } => DedupeSkip::ReportOnly,
        };
        for m in g.members.iter().filter(|m| m.id != g.keeper) {
            report.outcomes.push(DedupeOutcome {
                group: g.id,
                member: m.id,
                status: DedupeItemStatus::Skipped {
                    reason: reason.clone(),
                },
                files: Vec::new(),
            });
        }
    }
}

fn pause_all(plan: &DedupePlan, works: &[Work], report: &mut DedupeReport, kind: ItemErrorKind) {
    report.paused = Some(kind);
    report.resumable = works.iter().map(|w| ids(plan, w)).collect();
}

fn ids(plan: &DedupePlan, w: &Work) -> (u64, usize) {
    let g = &plan.groups[w.g];
    (g.id, g.members[w.m].id)
}

/// Where the writer writes from and to.
#[derive(Clone, Debug)]
struct Layout {
    /// The writer's root: the library root, or a quarantine outside it.
    root: PathBuf,
    /// The library root when the writer's root is the quarantine.
    source_root: Option<PathBuf>,
}

impl Layout {
    /// The library root holds the default quarantine and is where the
    /// Trash takes files from; a quarantine outside the library is the
    /// writer's root, with the library as the source root.
    fn of(plan: &DedupePlan) -> Layout {
        if plan.trash || plan.quarantine.starts_with(&plan.root) {
            Layout {
                root: plan.root.clone(),
                source_root: None,
            }
        } else {
            Layout {
                root: plan.quarantine.clone(),
                source_root: Some(plan.root.clone()),
            }
        }
    }
}

/// The quarantine of a plan that sets copies aside inside the library
/// resolves inside it on disk: `_Duplicados` (or a folder above it) made a
/// symlink out of the library is refused before anything moves, rather
/// than failing copy by copy. Every write is also checked on its own.
fn check_inside(plan: &DedupePlan, layout: &Layout) -> Result<(), WriteError> {
    if plan.trash || layout.source_root.is_some() {
        return Ok(());
    }
    ensure_within(&layout.root, &plan.quarantine).map(|_| ())
}

/// Copies across volumes need their bytes plus 1 % free on the
/// quarantine's volume. Unknown free space is not checked.
fn check_space(plan: &DedupePlan, works: &[Work]) -> Result<(), ApplyError> {
    if plan.trash || same_volume(&plan.root, &plan.quarantine) {
        return Ok(());
    }
    let sum: u64 = works
        .iter()
        .map(|w| {
            let m = &plan.groups[w.g].members[w.m];
            let travels: u64 = m
                .travels
                .iter()
                .filter_map(|t| fs::metadata(&t.from).ok())
                .map(|meta| meta.len())
                .sum();
            m.size + travels
        })
        .sum();
    let needed = sum.saturating_add(sum.div_ceil(100));
    match user_available(&plan.quarantine) {
        Some(free) if free < needed => Err(ApplyError::NoSpace { needed, free }),
        _ => Ok(()),
    }
}

/// Where `file` (of a copy in `system`) goes in the quarantine: the same
/// place relative to the folder of system folders, else to the library
/// root, else `<quarantine>/<system>/<name>`.
fn quarantine_path(plan: &DedupePlan, system: &str, file: &Path) -> PathBuf {
    let q = &plan.quarantine;
    if let Ok(rel) = file.strip_prefix(&plan.roms_dir) {
        if !rel.as_os_str().is_empty() {
            return q.join(rel);
        }
    }
    if let Ok(rel) = file.strip_prefix(&plan.root) {
        if !rel.as_os_str().is_empty() {
            return q.join(rel);
        }
    }
    q.join(system).join(file.file_name().unwrap_or_default())
}

/// The quarantine folders the extras land in (for temp clean-up).
fn quarantine_dirs(plan: &DedupePlan, works: &[Work]) -> Vec<PathBuf> {
    let mut dirs = BTreeSet::new();
    for w in works {
        let g = &plan.groups[w.g];
        let m = &g.members[w.m];
        let files = m.unit.files.iter().chain(m.travels.iter().map(|t| &t.from));
        for f in files {
            if let Some(dir) = quarantine_path(plan, &g.system, f).parent() {
                dirs.insert(dir.to_path_buf());
            }
        }
    }
    dirs.into_iter().collect()
}

/// What the destination cannot hold of `m`'s files, checked once per
/// destination folder before anything is copied (hard rule 13).
fn dest_limit(
    plan: &DedupePlan,
    system: &str,
    m: &Member,
    fat32: &mut HashMap<PathBuf, bool>,
) -> Option<DestLimit> {
    for f in &m.unit.files {
        let dest = quarantine_path(plan, system, f);
        let name = dest.file_name()?.to_string_lossy().into_owned();
        if safe_file_name(&name).ok().as_deref() != Some(name.as_str()) {
            return Some(DestLimit::BadName);
        }
        let below = dest.strip_prefix(&plan.quarantine).unwrap_or(&dest);
        let too_long = below.components().any(|c| {
            let part = c.as_os_str().to_string_lossy();
            part.len() > MAX_NAME || part.encode_utf16().count() > MAX_NAME
        }) || dest.as_os_str().len() > UNIX_MAX_PATH
            || (cfg!(windows) && dest.to_string_lossy().encode_utf16().count() > WINDOWS_MAX_PATH);
        if too_long {
            return Some(DestLimit::PathTooLong);
        }
        let size = fs::symlink_metadata(f).map_or(0, |meta| meta.len());
        if size >= FAT32_LIMIT {
            let dir = dest.parent().unwrap_or(&plan.quarantine).to_path_buf();
            let on_fat32 = *fat32
                .entry(dir)
                .or_insert_with_key(|dir| file_system_is_fat32(dir));
            if on_fat32 {
                return Some(DestLimit::Fat32Size);
            }
        }
    }
    None
}

/// `"{stem} ({n}).{ext}"`, as the writer numbers names.
fn numbered(name: &str, n: u32) -> String {
    match name.rfind('.').filter(|&i| i > 0) {
        Some(i) => format!("{} ({n}){}", &name[..i], &name[i..]),
        None => format!("{name} ({n})"),
    }
}

fn split(path: &Path) -> Option<(&Path, &str)> {
    Some((path.parent()?, path.file_name()?.to_str()?))
}

fn is_file(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_file())
}

/// How one extra ended, or why the run stops at it.
enum Step {
    Done(DedupeItemStatus, Vec<PathBuf>),
    /// The volume is the problem: this extra and the rest are resumable.
    Pause(ItemErrorKind),
}

fn skipped(reason: DedupeSkip) -> Step {
    Step::Done(DedupeItemStatus::Skipped { reason }, Vec::new())
}

/// Whether a copy is still the one the plan saw: every file there, the
/// primary's modification time (each file's, for a set the plan timed)
/// and the total size unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Check {
    Same,
    Changed,
    Missing,
}

fn check_unit(m: &Member) -> Check {
    let mut size = 0u64;
    for (i, f) in m.unit.files.iter().enumerate() {
        let meta = match fs::symlink_metadata(f) {
            Ok(meta) if meta.file_type().is_file() => meta,
            Ok(_) => return Check::Changed,
            Err(_) => return Check::Missing,
        };
        let planned = match m.unit.mtimes_ns.get(i) {
            Some(&t) if m.unit.mtimes_ns.len() == m.unit.files.len() => Some(t),
            _ if i == 0 => Some(m.unit.mtime_ns),
            _ => None,
        };
        if planned.is_some_and(|t| mtime_ns(&meta) != t) {
            return Check::Changed;
        }
        size += meta.len();
    }
    if size == m.size {
        Check::Same
    } else {
        Check::Changed
    }
}

/// The finished journal record of `from` for `item`: a finished move whose
/// source is gone.
fn record_of<'r>(
    records: &'r [TransferRecord],
    item: usize,
    from: &Path,
) -> Option<&'r TransferRecord> {
    records.iter().rev().find(|r| {
        r.item == Some(item)
            && r.extracted.is_none()
            && r.from == from
            && r.finished
            && fs::symlink_metadata(from).is_err()
    })
}

/// `lines` grouped by the item `item_of` names (lines of no item are
/// never looked up), each group in journal order.
fn by_item<T>(lines: Vec<T>, item_of: impl Fn(&T) -> Option<usize>) -> HashMap<usize, Vec<T>> {
    let mut out: HashMap<usize, Vec<T>> = HashMap::new();
    for line in lines {
        if let Some(item) = item_of(&line) {
            out.entry(item).or_default().push(line);
        }
    }
    out
}

struct Run<'a> {
    plan: &'a DedupePlan,
    env: &'a DedupeApplyEnv<'a>,
    writer: Writer,
    layout: Layout,
    resumed: bool,
    /// One listing per quarantine folder for the whole run.
    index: DirIndex,
    /// The journal's transfers and Trash lines before this run (resume),
    /// by item, in journal order: a resume of a job of many thousand copies
    /// looks each one up without walking the whole journal.
    records: HashMap<usize, Vec<TransferRecord>>,
    steps: HashMap<usize, Vec<ArchiveRecord>>,
    /// Keepers already named in `art_left`, systems already noted for
    /// their game list: one note each, without a scan of every note.
    art_seen: BTreeSet<PathBuf>,
    gamelists_noted: BTreeSet<String>,
    /// Extras the destination cannot hold, by item.
    limits: HashMap<usize, DestLimit>,
    /// A sample path per volume and whether it has a Trash.
    trash_ok: Vec<(PathBuf, bool)>,
    /// Game list → entries to remove, flushed once per run.
    gamelists: BTreeMap<PathBuf, Vec<String>>,
    /// Content rows follow moves: the volumes of library and quarantine.
    volumes: (Option<String>, Option<String>),
}

impl<'a> Run<'a> {
    fn new(
        plan: &'a DedupePlan,
        env: &'a DedupeApplyEnv<'a>,
        writer: Writer,
        layout: Layout,
        records: Vec<TransferRecord>,
        steps: Vec<ArchiveRecord>,
        resumed: bool,
    ) -> Run<'a> {
        let volumes = if plan.trash {
            (None, None)
        } else {
            (volume_uuid(&plan.root), volume_uuid(&layout.root))
        };
        Run {
            plan,
            env,
            writer,
            layout,
            resumed,
            index: DirIndex::new(),
            records: by_item(records, |r| r.item),
            steps: by_item(steps, |s| s.item),
            art_seen: BTreeSet::new(),
            gamelists_noted: BTreeSet::new(),
            limits: HashMap::new(),
            trash_ok: Vec::new(),
            gamelists: BTreeMap::new(),
            volumes,
        }
    }

    fn run(mut self, works: &[Work], mut report: DedupeReport) -> DedupeReport {
        if !self.plan.trash {
            let mut fat32 = HashMap::new();
            for w in works {
                let g = &self.plan.groups[w.g];
                if let Some(limit) = dest_limit(self.plan, &g.system, &g.members[w.m], &mut fat32) {
                    self.limits.insert(w.item, limit);
                }
            }
        }
        let total = works.len();
        let mut last: Option<Instant> = None;
        let mut stopped = false;
        let mut at = 0;
        while at < works.len() {
            let g = works[at].g;
            let end = at + works[at..].iter().take_while(|w| w.g == g).count();
            if self.env.cancel.load(Ordering::SeqCst) {
                for w in &works[at..] {
                    let (group, member) = ids(self.plan, w);
                    report.outcomes.push(DedupeOutcome {
                        group,
                        member,
                        status: DedupeItemStatus::Skipped {
                            reason: DedupeSkip::Cancelled,
                        },
                        files: Vec::new(),
                    });
                    report.resumable.push((group, member));
                }
                stopped = true;
                break;
            }
            if last.is_none_or(|t| t.elapsed() >= PROGRESS_EVERY) {
                last = Some(Instant::now());
                (self.env.progress)(at, total);
            }
            if let Some((kind, from)) = self.group(&works[at..end], &mut report) {
                report.paused = Some(kind);
                report.resumable = works[at + from..]
                    .iter()
                    .map(|w| ids(self.plan, w))
                    .collect();
                stopped = true;
                break;
            }
            at = end;
        }
        self.flush_gamelists(&mut report);
        if !stopped {
            self.write_list(&report);
        }
        let job = self.writer.job_id().to_string();
        if stopped {
            (self.env.progress)(total - report.resumable.len(), total);
            if self.writer.has_entries() || self.resumed {
                report.job = Some(job);
            } else {
                // Nothing changed: no job to resume; applying again does
                // what is left.
                let _ = self.writer.discard();
                let _ = fs::remove_file(plan_path(self.env.data_dir, &job));
            }
            return report;
        }
        (self.env.progress)(total, total);
        report.job = if self.writer.has_entries() || self.resumed {
            Some(self.writer.finish().unwrap_or(job))
        } else {
            let _ = self.writer.discard();
            let _ = fs::remove_file(plan_path(self.env.data_dir, &job));
            None
        };
        report
    }

    /// The extras of one group. `Some((kind, n))` pauses the run at its
    /// `n`th extra.
    fn group(
        &mut self,
        batch: &[Work],
        report: &mut DedupeReport,
    ) -> Option<(ItemErrorKind, usize)> {
        let plan = self.plan;
        let g = &plan.groups[batch[0].g];
        let keeper = &g.members[g.keeper];
        let mut keeper_ok: Option<bool> = None;
        for (n, w) in batch.iter().enumerate() {
            let m = &g.members[w.m];
            let step = if self.started(w, m) {
                if plan.trash {
                    self.trash_extra(w, m)
                } else {
                    self.finish_started(w, &g.system, m)
                }
            } else {
                let ok = *keeper_ok.get_or_insert_with(|| check_unit(keeper) == Check::Same);
                if !ok && !plan.root.is_dir() {
                    return Some((ItemErrorKind::VolumeGone, n));
                }
                match self.ready(w, m, ok) {
                    Some(step) => step,
                    None if plan.trash => self.trash_extra(w, m),
                    None => self.quarantine_extra(w, &g.system, m),
                }
            };
            match step {
                Step::Pause(kind) => return Some((kind, n)),
                Step::Done(status, files) => {
                    if matches!(
                        status,
                        DedupeItemStatus::Quarantined { .. } | DedupeItemStatus::Trashed { .. }
                    ) {
                        self.set_aside(&g.system, keeper, m, report);
                    }
                    report.outcomes.push(DedupeOutcome {
                        group: g.id,
                        member: m.id,
                        status,
                        files,
                    });
                }
            }
        }
        None
    }

    /// Why `m` cannot be set aside now, if it cannot.
    fn ready(&mut self, w: &Work, m: &Member, keeper_ok: bool) -> Option<Step> {
        if !keeper_ok {
            return Some(skipped(DedupeSkip::KeeperChanged));
        }
        if let Some(why) = self.limits.get(&w.item) {
            return Some(skipped(DedupeSkip::DestinationLimit { why: *why }));
        }
        if self.plan.trash && !self.trash_available(&m.unit.primary) {
            return Some(skipped(DedupeSkip::TrashUnavailable));
        }
        match check_unit(m) {
            Check::Same => None,
            Check::Changed => Some(skipped(DedupeSkip::ChangedSincePlan)),
            Check::Missing if !self.plan.root.is_dir() => {
                Some(Step::Pause(ItemErrorKind::VolumeGone))
            }
            Check::Missing => Some(skipped(DedupeSkip::SourceMissing)),
        }
    }

    /// An earlier run of this job already moved or trashed some of `m`'s
    /// files (resume only).
    fn started(&self, w: &Work, m: &Member) -> bool {
        if !self.resumed {
            return false;
        }
        if self.plan.trash {
            return self.steps.contains_key(&w.item);
        }
        let files = m.unit.files.iter().chain(m.travels.iter().map(|t| &t.from));
        files
            .into_iter()
            .any(|f| record_of(self.records_of(w.item), w.item, f).is_some())
    }

    /// The journal's transfers of `item` before this run.
    fn records_of(&self, item: usize) -> &[TransferRecord] {
        self.records.get(&item).map_or(&[], Vec::as_slice)
    }

    fn trash_available(&mut self, path: &Path) -> bool {
        if let Some((_, ok)) = self.trash_ok.iter().find(|(p, _)| same_volume(p, path)) {
            return *ok;
        }
        let ok = self.env.trash.available(path);
        self.trash_ok.push((path.to_path_buf(), ok));
        ok
    }

    /// A pause for errors that stop the whole run (full, read-only or
    /// vanished volume), else the extra fails and the run goes on.
    fn on_error(&self, e: &WriteError) -> Step {
        if !self.plan.root.is_dir() || !self.layout.root.is_dir() {
            return Step::Pause(ItemErrorKind::VolumeGone);
        }
        match e {
            WriteError::DiskFull => Step::Pause(ItemErrorKind::DiskFull),
            WriteError::Denied(_) => Step::Pause(ItemErrorKind::WriteDenied),
            _ => Step::Done(
                DedupeItemStatus::Failed {
                    kind: ItemErrorKind::from(e),
                    detail: e.to_string(),
                },
                Vec::new(),
            ),
        }
    }

    /// The first free name for `want` in its folder by the index: itself,
    /// then `" (2)"`, `" (3)"`, … (hard rule 6).
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

    /// Moves `from` to `to` (already free by the index) for `item`; the
    /// landing place is claimed and the content row follows.
    fn move_one(
        &mut self,
        from: &Path,
        to: &Path,
        collision: Collision,
        item: usize,
    ) -> Result<MoveOutcome, WriteError> {
        let outcome = self
            .writer
            .move_file_indexed(from, to, collision, Some(item))?;
        if let MoveOutcome::Moved { to, .. } | MoveOutcome::CopiedNotRemoved { to, .. } = &outcome {
            if let Some((dir, name)) = split(to) {
                self.index.claim(dir, name);
            }
            self.follow_content(from, to);
        }
        Ok(outcome)
    }

    /// The content row of a moved file follows it (spec §7.1, step 8).
    /// Best effort: a row left behind is only a cache miss.
    fn follow_content(&self, from: &Path, to: &Path) {
        let (Some(src), Some(dst)) = &self.volumes else {
            return;
        };
        if src == dst {
            let _ = self.env.cache.rekey_content(src, from, to);
            return;
        }
        let Ok(meta) = fs::metadata(to) else {
            return;
        };
        if let Ok(Some(row)) = self
            .env
            .cache
            .content_row(src, from, meta.len(), mtime_ns(&meta))
        {
            let _ = self.env.cache.put_content_row(&ContentRow {
                volume_uuid: dst.clone(),
                path: to.to_path_buf(),
                ..row
            });
        }
    }

    /// Sets `m` aside in the quarantine: the unit's files (a set all or
    /// nothing), then its travelling companions and art.
    fn quarantine_extra(&mut self, w: &Work, system: &str, m: &Member) -> Step {
        let files = &m.unit.files;
        let dests: Vec<PathBuf> = files
            .iter()
            .map(|f| quarantine_path(self.plan, system, f))
            .collect();
        let mut landed: Vec<PathBuf> = Vec::new();
        let mut not_removed = false;
        if files.len() == 1 {
            let to = self.free_name(&dests[0]);
            match self.move_one(&files[0], &to, Collision::Rename, w.item) {
                Ok(MoveOutcome::Moved { to, .. }) => landed.push(to),
                Ok(MoveOutcome::CopiedNotRemoved { to, .. }) => {
                    not_removed = true;
                    landed.push(to);
                }
                Ok(_) => return skipped(DedupeSkip::Taken),
                Err(e) => return self.on_error(&e),
            }
        } else {
            for d in &dests {
                let Some((dir, name)) = split(d) else {
                    return skipped(DedupeSkip::DestinationLimit {
                        why: DestLimit::BadName,
                    });
                };
                if let Some(existing) = self.index.existing(dir, name) {
                    let exact = existing.file_name().and_then(|n| n.to_str()) == Some(name);
                    return skipped(if exact {
                        DedupeSkip::Taken
                    } else {
                        DedupeSkip::DestinationLimit {
                            why: DestLimit::CaseCollision,
                        }
                    });
                }
                if self.index.claimed(dir, name) {
                    return skipped(DedupeSkip::Taken);
                }
            }
            let mut done: Vec<(PathBuf, PathBuf)> = Vec::new();
            for (from, to) in files.iter().zip(&dests) {
                match self.move_one(from, to, Collision::Skip, w.item) {
                    Ok(MoveOutcome::Moved { to, .. }) => done.push((from.clone(), to)),
                    Ok(MoveOutcome::CopiedNotRemoved { to, .. }) => {
                        not_removed = true;
                        done.push((from.clone(), to));
                    }
                    Ok(_) => {
                        let left = self.roll_back(&done);
                        if left.is_empty() {
                            return skipped(DedupeSkip::Taken);
                        }
                        return split_set(left, "its name is taken in the quarantine");
                    }
                    Err(e) => {
                        let left = self.roll_back(&done);
                        return match self.on_error(&e) {
                            Step::Done(DedupeItemStatus::Failed { detail, .. }, _)
                                if !left.is_empty() =>
                            {
                                split_set(left, &detail)
                            }
                            step => step,
                        };
                    }
                }
            }
            landed.extend(done.into_iter().map(|(_, to)| to));
        }
        let primary = landed[0].clone();
        if let Some(pause) = self.move_travels(w, system, m, &mut landed) {
            return pause;
        }
        if not_removed {
            return Step::Done(
                DedupeItemStatus::Failed {
                    kind: ItemErrorKind::Io,
                    detail: "copied to the quarantine, but the original could not be removed"
                        .into(),
                },
                landed,
            );
        }
        Step::Done(DedupeItemStatus::Quarantined { to: primary }, landed)
    }

    /// Puts the files of a set already moved back home, newest first,
    /// renamed or (across volumes) copied back and verified; their content
    /// rows go back with them. Returns the files that stayed in the
    /// quarantine: the journal keeps them, so undo brings them back.
    fn roll_back(&mut self, done: &[(PathBuf, PathBuf)]) -> Vec<PathBuf> {
        let mut left = Vec::new();
        for (from, to) in done.iter().rev() {
            match self.writer.move_back(to, from) {
                Ok(()) => self.follow_content(to, from),
                Err(_) => left.push(to.clone()),
            }
        }
        left.reverse();
        left
    }

    /// Travelling companions and art follow into the same relative places,
    /// each under a free name. One that cannot move stays where it is and
    /// never fails the extra; `Some` is a pause (the volume itself).
    fn move_travels(
        &mut self,
        w: &Work,
        system: &str,
        m: &Member,
        landed: &mut Vec<PathBuf>,
    ) -> Option<Step> {
        for t in &m.travels {
            if let Some(r) = record_of(self.records_of(w.item), w.item, &t.from) {
                landed.push(r.target.clone());
                continue;
            }
            if !is_file(&t.from) {
                continue;
            }
            let to = self.free_name(&quarantine_path(self.plan, system, &t.from));
            match self.move_one(&t.from, &to, Collision::Rename, w.item) {
                Ok(MoveOutcome::Moved { to, .. } | MoveOutcome::CopiedNotRemoved { to, .. }) => {
                    landed.push(to)
                }
                Ok(_) => {}
                Err(e) => {
                    if let Step::Pause(kind) = self.on_error(&e) {
                        return Some(Step::Pause(kind));
                    }
                }
            }
        }
        None
    }

    /// Finishes an extra an earlier run of the job started: what the
    /// journal shows moved is counted, what is still at its place follows.
    fn finish_started(&mut self, w: &Work, system: &str, m: &Member) -> Step {
        let mut landed = Vec::new();
        for from in &m.unit.files {
            if let Some(r) = record_of(self.records_of(w.item), w.item, from) {
                landed.push(r.target.clone());
                continue;
            }
            if !is_file(from) {
                continue;
            }
            let to = quarantine_path(self.plan, system, from);
            match self.move_one(from, &to, Collision::Skip, w.item) {
                Ok(MoveOutcome::Moved { to, .. } | MoveOutcome::CopiedNotRemoved { to, .. }) => {
                    landed.push(to)
                }
                Ok(_) => {
                    let detail = format!(
                        "{} of the set stayed: its name is taken in the quarantine",
                        from.display()
                    );
                    let kind = ItemErrorKind::Io;
                    return Step::Done(DedupeItemStatus::Failed { kind, detail }, landed);
                }
                Err(e) => return self.on_error(&e),
            }
        }
        if let Some(pause) = self.move_travels(w, system, m, &mut landed) {
            return pause;
        }
        match landed.first() {
            Some(primary) => Step::Done(
                DedupeItemStatus::Quarantined {
                    to: primary.clone(),
                },
                landed,
            ),
            None => skipped(DedupeSkip::SourceMissing),
        }
    }

    /// Sends `m`'s files, then its travelling companions and art, to the
    /// Trash, each journaled on its own (spec §7.2). Files already gone
    /// (a resumed run) are skipped without a journal line.
    fn trash_extra(&mut self, w: &Work, m: &Member) -> Step {
        let units = m.unit.files.len();
        let mut location = self.trashed_at(w.item, &m.unit.primary);
        let mut landed = Vec::new();
        let travels = m.travels.iter().map(|t| &t.from);
        for (k, f) in m.unit.files.iter().chain(travels).enumerate() {
            if k >= units && !is_file(f) {
                continue;
            }
            match trash_file(&mut self.writer, self.env.trash, f, Some(w.item)) {
                Ok(TrashStep::Trashed { location: at }) => {
                    if k == 0 {
                        location.clone_from(&at);
                    }
                    landed.push(at.unwrap_or_else(|| f.clone()));
                }
                Ok(TrashStep::Gone) => {
                    if let Some(at) = self.trashed_at(w.item, f) {
                        landed.push(at);
                    }
                }
                Ok(TrashStep::Kept { why }) if k == 0 => {
                    return match why {
                        TrashKept::Unavailable => skipped(DedupeSkip::TrashUnavailable),
                        TrashKept::Refused => Step::Done(
                            DedupeItemStatus::Failed {
                                kind: ItemErrorKind::Io,
                                detail: format!("the Trash refused {}", f.display()),
                            },
                            Vec::new(),
                        ),
                    };
                }
                Ok(TrashStep::Kept { .. }) if k < units => {
                    let detail = format!(
                        "{} of the set stayed: the Trash refused it; undo brings the rest back",
                        f.display()
                    );
                    let kind = ItemErrorKind::Io;
                    return Step::Done(DedupeItemStatus::Failed { kind, detail }, landed);
                }
                Ok(TrashStep::Kept { .. }) => {}
                Err(e) => return self.on_error(&e),
            }
        }
        if landed.is_empty() {
            return skipped(DedupeSkip::SourceMissing);
        }
        Step::Done(DedupeItemStatus::Trashed { location }, landed)
    }

    /// Where an earlier run of the job put `from` in the Trash.
    fn trashed_at(&self, item: usize, from: &Path) -> Option<PathBuf> {
        let steps = self.steps.get(&item).map_or(&[][..], Vec::as_slice);
        steps.iter().rev().find_map(|s| match &s.step {
            ArchiveStep::Trashed { trash } if s.item == Some(item) && s.from == from => {
                trash.clone()
            }
            _ => None,
        })
    }

    /// What setting `m` aside means for the report and the game list.
    fn set_aside(&mut self, system: &str, keeper: &Member, m: &Member, report: &mut DedupeReport) {
        report.bytes_out += m.size;
        if !m.art_leaves.is_empty() && self.art_seen.insert(keeper.unit.primary.clone()) {
            report.art_left.push(keeper.unit.primary.clone());
            report.notes.push(DedupeApplyNote::ArtLeaves {
                keeper: keeper.unit.primary.clone(),
            });
        }
        if let Some(entry) = &m.gamelist {
            if entry.gamelist.starts_with(&self.layout.root) && self.layout.source_root.is_none() {
                self.gamelists
                    .entry(entry.gamelist.clone())
                    .or_default()
                    .push(entry.path.clone());
            } else {
                if self.gamelists_noted.insert(system.to_string()) {
                    note_gamelist(report, system);
                }
            }
        }
    }

    /// Removes the entries of the extras set aside from each game list, one
    /// backed-up write per list (undo restores it byte for byte).
    fn flush_gamelists(&mut self, report: &mut DedupeReport) {
        let lists = std::mem::take(&mut self.gamelists);
        for (path, roms) in lists {
            if remove_from(&mut self.writer, &path, &roms).is_err() {
                let system = self
                    .plan
                    .groups
                    .iter()
                    .flat_map(|g| g.members.iter().map(move |m| (g, m)))
                    .find(|(_, m)| m.gamelist.as_ref().is_some_and(|e| e.gamelist == path))
                    .map(|(g, _)| g.system.clone())
                    .unwrap_or_default();
                note_gamelist(report, &system);
            }
        }
    }

    /// `<quarantine>/<job>.txt`, for people who look at the folder without
    /// Coverly (spec §7.1): one line per extra set aside,
    /// `<kept> <- <set aside> (<relation>, <keep reason>)`. Quarantine
    /// action only; best effort.
    fn write_list(&mut self, report: &DedupeReport) {
        if self.plan.trash {
            return;
        }
        let mut text = String::new();
        for o in &report.outcomes {
            if !matches!(o.status, DedupeItemStatus::Quarantined { .. }) {
                continue;
            }
            let Some(g) = self.plan.groups.iter().find(|g| g.id == o.group) else {
                continue;
            };
            let (Some(keeper), Some(m)) = (g.members.get(g.keeper), g.members.get(o.member)) else {
                continue;
            };
            let kebab = |v: serde_json::Value| match v {
                serde_json::Value::String(s) => s,
                serde_json::Value::Object(map) => map
                    .get("kind")
                    .and_then(|k| k.as_str())
                    .unwrap_or_default()
                    .to_string(),
                _ => String::new(),
            };
            let relation = serde_json::to_value(m.relation).map_or_else(|_| String::new(), kebab);
            let reason = serde_json::to_value(&g.reason).map_or_else(|_| String::new(), kebab);
            text.push_str(&format!(
                "{} <- {} ({relation}, {reason})\n",
                self.rel(&keeper.unit.primary),
                self.rel(&m.unit.primary)
            ));
        }
        if text.is_empty() {
            return;
        }
        let q = self.plan.quarantine.clone();
        if self.writer.ensure_dir(&q).is_err() {
            return;
        }
        let target = q.join(format!("{}.txt", self.writer.job_id()));
        if !is_file(&target) {
            let _ = self.writer.create_new_file(&target, text.as_bytes(), None);
        }
    }

    /// `path` relative to the folder of system folders, `/`-separated.
    fn rel(&self, path: &Path) -> String {
        let rel = path
            .strip_prefix(&self.plan.roms_dir)
            .or_else(|_| path.strip_prefix(&self.plan.root))
            .unwrap_or(path);
        rel_slash(rel).trim_start_matches("./").to_string()
    }
}

/// A set that could not move whole and whose files `left` could not go
/// back either: it is reported failed with where those files are.
fn split_set(left: Vec<PathBuf>, why: &str) -> Step {
    let detail = format!(
        "the set could not be set aside ({why}) and {} of its files stayed in the quarantine; undo brings them back",
        left.len()
    );
    Step::Done(
        DedupeItemStatus::Failed {
            kind: ItemErrorKind::Io,
            detail,
        },
        left,
    )
}

fn note_gamelist(report: &mut DedupeReport, system: &str) {
    let note = DedupeApplyNote::GamelistNotUpdated {
        system: system.to_string(),
    };
    if !report.notes.contains(&note) {
        report.notes.push(note);
    }
}

/// What sending one file to the Trash did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrashStep {
    /// In the Trash, at `location` when the OS said where.
    Trashed { location: Option<PathBuf> },
    /// Still where it was.
    Kept { why: TrashKept },
    /// Nothing was there any more.
    Gone,
}

/// Why a file stayed where it was.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrashKept {
    /// Its volume has no Trash.
    Unavailable,
    /// The Trash refused it.
    Refused,
}

/// Sends the file `path` (inside the job's source root) to `trash`,
/// journaled for plan item `item`: `archive_trash_intent` before the
/// call (so undo can look the file up by name and fingerprint even if the
/// run dies during it), then `archive_trashed` with the location the OS
/// gave. A volume without a Trash or a refusal journals `archive_kept`
/// and leaves the file in place; a file no longer there is
/// [`TrashStep::Gone`]. A Trash I/O error and a failed journal append are
/// returned with the intent left unresolved: the journal is then kept and
/// undo looks the file up at its place and in the Trash.
#[doc(hidden)]
pub fn trash_file(
    w: &mut Writer,
    trash: &dyn TrashBackend,
    path: &Path,
    item: Option<usize>,
) -> Result<TrashStep, WriteError> {
    match w.archive_trash_intent(path, item) {
        Ok(_) => {}
        Err(WriteError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(TrashStep::Gone)
        }
        Err(e) => return Err(e),
    }
    let why = match trash.trash(path) {
        Ok(location) => {
            w.archive_trashed(path, location.as_deref(), item)?;
            return Ok(TrashStep::Trashed { location });
        }
        Err(TrashError::Missing(_)) => {
            // Gone between the intent and the call: nothing left to undo.
            w.archive_kept(path, item)?;
            return Ok(TrashStep::Gone);
        }
        Err(TrashError::Unavailable(_)) => TrashKept::Unavailable,
        Err(TrashError::Refused { .. }) => TrashKept::Refused,
        // Where the file is now is not known: the intent stays the last
        // line, so undo checks its place and the Trash.
        Err(TrashError::Io(e)) => return Err(WriteError::Io(e)),
    };
    w.archive_kept(path, item)?;
    Ok(TrashStep::Kept { why })
}
