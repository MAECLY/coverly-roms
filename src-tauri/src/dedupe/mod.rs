//! Duplicates from the app (duplicates spec §11, §12): the plans the backend
//! keeps under a `plan_id` (the webview only sends the id and its picks
//! back), cancelling the plan being made, and the one apply (or resume)
//! running on a worker thread behind the fill jobs' manual-write guard, so
//! it never runs beside a fill or an organise job. Undo goes through
//! `organize_undo`. Free of Tauri types; `AppState` drives it.

pub mod view;

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use coverly_core::cache::{Cache, CACHE_FILE};
use coverly_core::dedupe::art::{group_catalogue_name, group_cover_path, member_art_path};
use coverly_core::dedupe::{
    DedupeApplyEnv, DedupeDecisions, DedupeError, DedupeItemStatus, DedupeOptions, DedupePlan,
    DedupeProgress, DedupeReport, DedupeSkip,
};
use coverly_core::jobs::ItemErrorKind;
use coverly_core::organize::apply::ApplyError;
use coverly_core::profiles::Media;
use coverly_core::write::trash::TrashBackend;

use crate::error::IpcError;
use crate::jobs::OwnedManualWrite;
use crate::library::art::media_name;
use view::{DedupeEventPhase, DedupeJobState, DedupeJobView, DedupePlanView, DedupeProgressEvent};

/// Plans kept for picks, regrouping, export and apply; older ones are
/// dropped.
pub const KEPT_PLANS: usize = 4;

/// Progress is sent at most this often (phase changes and ends always).
const PROGRESS_EVERY: Duration = Duration::from_millis(100);

/// Where planning and applying report to (Tauri events in the app, a
/// recorder in tests).
pub trait DedupeEvents: Send + Sync {
    /// Throttled progress of a plan or an apply.
    fn progress(&self, event: DedupeProgressEvent);
    /// An apply or resume ended (done, paused, cancelled or failed).
    fn changed(&self, _view: DedupeJobView) {}
}

/// One duplicates plan as the backend keeps it, with the picks made on it.
#[derive(Debug)]
pub struct StoredDedupe {
    /// Absolute library root.
    pub root: PathBuf,
    /// The options the plan was made (or last regrouped) with.
    pub opts: DedupeOptions,
    /// The plan with its internals: picks and regroups read nothing again.
    pub plan: DedupePlan,
    /// Groups skipped one by one.
    pub skipped_groups: BTreeSet<u64>,
    /// Systems skipped whole.
    pub skipped_systems: BTreeSet<String>,
    pub trash_available: bool,
}

impl StoredDedupe {
    pub fn view(&self, plan_id: u64) -> DedupePlanView {
        DedupePlanView {
            plan_id,
            plan: self.plan.clone(),
            trash_available: self.trash_available,
        }
    }

    /// What apply is told besides the plan (the picks are in it already).
    pub fn decisions(&self) -> DedupeDecisions {
        DedupeDecisions {
            keepers: Vec::new(),
            skipped_groups: self.skipped_groups.iter().copied().collect(),
            skipped_systems: self.skipped_systems.iter().cloned().collect(),
        }
    }

    /// Skips (or takes back) group `group`.
    pub fn skip_group(&mut self, group: u64, skip: bool) -> Result<(), IpcError> {
        let system = self
            .plan
            .groups
            .iter()
            .find(|g| g.id == group)
            .map(|g| g.system.clone())
            .ok_or_else(|| IpcError::BadRequest(DedupeError::NoGroup(group).to_string()))?;
        if skip {
            self.skipped_groups.insert(group);
        } else {
            self.skipped_groups.remove(&group);
        }
        // A group of a skipped system stays skipped.
        let skipped = skip || self.skipped_systems.contains(&system);
        coverly_core::dedupe::skip_group(&mut self.plan, group, skipped);
        Ok(())
    }

    /// Skips (or takes back) every group of `system`; groups skipped one by
    /// one stay skipped.
    pub fn skip_system(&mut self, system: &str, skip: bool) {
        if skip {
            self.skipped_systems.insert(system.to_string());
        } else {
            self.skipped_systems.remove(system);
        }
        self.reapply_skips(Some(system));
    }

    /// The skips again over regrouped groups (all systems, or one).
    pub fn reapply_skips(&mut self, only: Option<&str>) {
        let groups: Vec<(u64, String)> = self
            .plan
            .groups
            .iter()
            .filter(|g| only.is_none_or(|s| g.system == s))
            .map(|g| (g.id, g.system.clone()))
            .collect();
        for (id, system) in groups {
            let skip = self.skipped_groups.contains(&id) || self.skipped_systems.contains(&system);
            coverly_core::dedupe::skip_group(&mut self.plan, id, skip);
        }
    }
}

type Kept = Arc<Mutex<StoredDedupe>>;

/// Where a duplicate group's cover or a copy's own art comes from (dedupe
/// UI spec D4, D16), copied out of the kept plan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArtSource {
    /// An art file the plan recorded, to be read only inside `root`.
    Library { root: PathBuf, path: PathBuf },
    /// An already downloaded libretro-thumbnails image of the group's
    /// catalogue name (`system` is the system id).
    Libretro { system: String, name: String },
}

/// A resolved libretro URL per `(plan_id, group, media)`; `None` also
/// remembers "nothing downloaded" (plan DP-2).
type LibretroMemo = HashMap<(u64, u64, Media), Option<String>>;

#[derive(Default)]
struct Inner {
    next_id: u64,
    /// Oldest first, at most [`KEPT_PLANS`].
    plans: VecDeque<(u64, Kept)>,
    /// The plan being made, with its cancel flag.
    running: Option<(u64, Arc<AtomicBool>)>,
    /// The current (or last) apply of this app session.
    apply: Option<Arc<ApplyJob>>,
    /// Libretro covers already looked up for the kept plans.
    libretro: LibretroMemo,
}

/// The plans of the duplicates screen, the one plan in progress and the
/// one apply.
#[derive(Default)]
pub struct DedupeManager {
    inner: Mutex<Inner>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl DedupeManager {
    /// A new plan id.
    pub fn next_id(&self) -> u64 {
        let mut inner = lock(&self.inner);
        inner.next_id += 1;
        inner.next_id
    }

    /// Marks plan `id` as the one being made and returns its cancel flag.
    /// A plan still being made is cancelled: the newer request supersedes
    /// it.
    pub fn begin(&self, id: u64) -> Arc<AtomicBool> {
        let flag = Arc::new(AtomicBool::new(false));
        let mut inner = lock(&self.inner);
        if let Some((_, old)) = inner.running.replace((id, Arc::clone(&flag))) {
            old.store(true, Ordering::SeqCst);
        }
        flag
    }

    /// Plan `id` ended (done, failed or cancelled).
    pub fn finish(&self, id: u64) {
        let mut inner = lock(&self.inner);
        if inner.running.as_ref().is_some_and(|(r, _)| *r == id) {
            inner.running = None;
        }
    }

    /// Cancels plan `id` while it is being made; anything else is a no-op.
    pub fn cancel(&self, id: u64) {
        let inner = lock(&self.inner);
        if let Some((r, flag)) = &inner.running {
            if *r == id {
                flag.store(true, Ordering::SeqCst);
            }
        }
    }

    /// Keeps `plan` under `id`, dropping the oldest beyond [`KEPT_PLANS`].
    pub fn store(&self, id: u64, plan: StoredDedupe) -> DedupePlanView {
        let view = plan.view(id);
        let mut inner = lock(&self.inner);
        inner.plans.push_back((id, Arc::new(Mutex::new(plan))));
        while inner.plans.len() > KEPT_PLANS {
            if let Some((old, _)) = inner.plans.pop_front() {
                inner.libretro.retain(|(plan, _, _), _| *plan != old);
            }
        }
        view
    }

    /// Plan `id`, while it is kept.
    pub fn get(&self, id: u64) -> Result<Kept, IpcError> {
        lock(&self.inner)
            .plans
            .iter()
            .find(|(i, _)| *i == id)
            .map(|(_, p)| Arc::clone(p))
            .ok_or(IpcError::NoDedupePlan)
    }

    /// Runs `edit` on plan `id` and returns the plan as it is then.
    pub fn edit<T>(
        &self,
        id: u64,
        edit: impl FnOnce(&mut StoredDedupe) -> Result<T, IpcError>,
    ) -> Result<DedupePlanView, IpcError> {
        let kept = self.get(id)?;
        let mut stored = lock(&kept);
        edit(&mut stored)?;
        Ok(stored.view(id))
    }

    /// The roots and quarantines of the kept plans and of the current
    /// apply (what `reveal_path` may show).
    pub fn known_folders(&self) -> Vec<PathBuf> {
        let inner = lock(&self.inner);
        let mut out = Vec::new();
        for (_, p) in &inner.plans {
            let p = lock(p);
            out.push(p.root.clone());
            out.push(p.plan.quarantine.clone());
        }
        if let Some(job) = &inner.apply {
            out.push(PathBuf::from(&job.root));
            out.push(PathBuf::from(&job.quarantine));
        }
        out
    }

    /// Where the art of group `group` (`member: None`, the group cover) or
    /// of one copy of it comes from in plan `plan_id`. The location is
    /// copied out, so no file is read while the plan is locked. A copy's
    /// own art is only what it recorded; the group cover falls back to the
    /// catalogue name on libretro. An unknown plan is `NoDedupePlan`; an
    /// unknown group or member, or no art at all, is `ArtMissing`.
    pub fn art_source(
        &self,
        plan_id: u64,
        group: u64,
        member: Option<usize>,
        media: Media,
    ) -> Result<ArtSource, IpcError> {
        let kept = self.get(plan_id)?;
        let stored = lock(&kept);
        let plan = &stored.plan;
        let library = |path: &std::path::Path| ArtSource::Library {
            root: stored.root.clone(),
            path: path.to_path_buf(),
        };
        let found = match member {
            Some(m) => member_art_path(plan, group, m, media).map(library),
            None => group_cover_path(plan, group, media)
                .map(library)
                .or_else(|| {
                    group_catalogue_name(plan, group).map(|(system, name)| ArtSource::Libretro {
                        system: system.to_string(),
                        name: name.to_string(),
                    })
                }),
        };
        found.ok_or_else(|| IpcError::ArtMissing(media_name(media)))
    }

    /// The libretro URL remembered for the cover of `group` in plan
    /// `plan_id`: `None` when not looked up yet, `Some(None)` when nothing
    /// was downloaded.
    pub fn libretro_memo(&self, plan_id: u64, group: u64, media: Media) -> Option<Option<String>> {
        lock(&self.inner)
            .libretro
            .get(&(plan_id, group, media))
            .cloned()
    }

    /// Remembers the libretro lookup of `group`'s cover while plan
    /// `plan_id` is kept.
    pub fn remember_libretro(&self, plan_id: u64, group: u64, media: Media, url: Option<String>) {
        let mut inner = lock(&self.inner);
        if inner.plans.iter().any(|(id, _)| *id == plan_id) {
            inner.libretro.insert((plan_id, group, media), url);
        }
    }

    /// Drops every kept plan (the library changed on disk).
    pub fn forget_all(&self) {
        let mut inner = lock(&self.inner);
        inner.plans.clear();
        inner.libretro.clear();
    }

    /// The current (or last) apply.
    pub fn current_apply(&self) -> Option<DedupeJobView> {
        lock(&self.inner).apply.as_ref().map(|j| j.view())
    }

    /// Asks the running apply to stop between groups; a no-op when none
    /// runs.
    pub fn cancel_apply(&self) {
        if let Some(job) = &lock(&self.inner).apply {
            if job.view().state == DedupeJobState::Applying {
                job.cancel.store(true, Ordering::SeqCst);
            }
        }
    }

    /// Runs `apply` on a worker thread holding `guard` (fills, organise
    /// jobs and other writes are refused until it ends) as the current
    /// apply. Progress goes to `events` (throttled) and the end as
    /// `changed`; a panic ends it as `failed`.
    pub fn spawn_apply<F>(
        &self,
        start: ApplyStart,
        guard: OwnedManualWrite,
        events: Arc<dyn DedupeEvents>,
        apply: F,
    ) -> DedupeJobView
    where
        F: FnOnce(&DedupeApplyEnv) -> Result<DedupeReport, ApplyError> + Send + 'static,
    {
        let job = Arc::new(ApplyJob {
            plan_id: start.plan_id,
            root: start.root,
            quarantine: start.quarantine,
            trash: start.trash,
            cancel: AtomicBool::new(false),
            data: Mutex::new(ApplyData {
                job: start.job,
                state: DedupeJobState::Applying,
                progress: None,
                report: None,
                error: None,
            }),
        });
        lock(&self.inner).apply = Some(Arc::clone(&job));
        let view = job.view();
        let data_dir = start.data_dir;
        let cache_dir = start.cache_dir;
        let backend = start.backend;
        std::thread::spawn(move || {
            let throttle = Throttle::new(job.plan_id, Arc::clone(&events));
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let cache = Cache::open(&cache_dir.join(CACHE_FILE))
                    .map_err(|e| IpcError::Io(e.to_string()))?;
                let progress = |done: usize, total: usize| {
                    let event = DedupeProgressEvent {
                        plan_id: job.plan_id,
                        phase: DedupeEventPhase::Apply,
                        done,
                        total,
                    };
                    lock(&job.data).progress = Some(event);
                    throttle.send(event);
                };
                let env = DedupeApplyEnv {
                    data_dir: &data_dir,
                    trash: backend.as_ref(),
                    cache: &cache,
                    progress: &progress,
                    cancel: &job.cancel,
                };
                apply(&env).map_err(apply_error)
            }))
            .unwrap_or_else(|_| {
                Err(IpcError::Io(
                    "the duplicates job stopped unexpectedly".into(),
                ))
            });
            job.end(outcome);
            let view = job.view();
            match &view.report {
                Some(r) => log::info!(
                    "duplicates job {:?} {:?}: {} copies, {} bytes set aside",
                    view.job,
                    view.state,
                    r.outcomes.iter().filter(|o| set_aside(&o.status)).count(),
                    r.bytes_out
                ),
                None => log::warn!("duplicates job {:?} failed: {:?}", view.job, view.error),
            }
            // Writes are free again before the end is announced: an undo
            // asked for on that event must not find the job still running.
            drop(guard);
            events.changed(view);
        });
        view
    }
}

fn set_aside(status: &DedupeItemStatus) -> bool {
    matches!(
        status,
        DedupeItemStatus::Quarantined { .. } | DedupeItemStatus::Trashed { .. }
    )
}

/// A duplicates job's missing plan is its own code; the rest as organise.
fn apply_error(e: ApplyError) -> IpcError {
    match e {
        ApplyError::NoPlan(_) => IpcError::NoDedupePlan,
        e => e.into(),
    }
}

/// What an apply starts with.
pub struct ApplyStart {
    pub plan_id: u64,
    /// The journal a resume continues.
    pub job: Option<String>,
    pub root: String,
    pub quarantine: String,
    pub trash: bool,
    /// Journals and saved plans.
    pub data_dir: PathBuf,
    /// The cache keeper picks are remembered in.
    pub cache_dir: PathBuf,
    /// Where `trash` plans send the copies.
    pub backend: Arc<dyn TrashBackend>,
}

/// One apply (or resume) of a duplicates plan.
struct ApplyJob {
    plan_id: u64,
    root: String,
    quarantine: String,
    trash: bool,
    cancel: AtomicBool,
    data: Mutex<ApplyData>,
}

struct ApplyData {
    job: Option<String>,
    state: DedupeJobState,
    progress: Option<DedupeProgressEvent>,
    report: Option<DedupeReport>,
    error: Option<IpcError>,
}

impl ApplyJob {
    fn view(&self) -> DedupeJobView {
        let d = lock(&self.data);
        DedupeJobView {
            job: d.job.clone(),
            plan_id: self.plan_id,
            state: d.state,
            progress: d.progress,
            report: d.report.clone(),
            root: self.root.clone(),
            quarantine: self.quarantine.clone(),
            trash: self.trash,
            error: d.error.clone(),
        }
    }

    /// Records how the worker ended.
    fn end(&self, outcome: Result<DedupeReport, IpcError>) {
        let mut d = lock(&self.data);
        d.progress = None;
        match outcome {
            Ok(report) if report.paused.is_some() && report.job.is_none() && d.job.is_none() => {
                // Paused before anything was journaled (the library was
                // gone at the start): there is no job to resume.
                let kind = report.paused.unwrap_or(ItemErrorKind::VolumeGone);
                d.state = DedupeJobState::Failed;
                d.error = Some(crate::state::item_error(kind, &self.root));
                d.report = Some(report);
            }
            Ok(report) => {
                d.state = end_state(&report);
                if report.job.is_some() {
                    d.job = report.job.clone();
                }
                d.report = Some(report);
            }
            Err(e) => {
                d.state = DedupeJobState::Failed;
                d.error = Some(e);
            }
        }
    }
}

/// Paused, cancelled (some copy was skipped for it) or done.
pub fn end_state(report: &DedupeReport) -> DedupeJobState {
    let cancelled = report.outcomes.iter().any(|o| {
        o.status
            == DedupeItemStatus::Skipped {
                reason: DedupeSkip::Cancelled,
            }
    });
    if report.paused.is_some() {
        DedupeJobState::Paused
    } else if cancelled {
        DedupeJobState::Cancelled
    } else {
        DedupeJobState::Done
    }
}

/// Sends the progress of `plan_id` at most every 100 ms; the first event of
/// a phase and the last of a count always go through.
pub struct Throttle {
    plan_id: u64,
    events: Arc<dyn DedupeEvents>,
    last: Mutex<Option<(DedupeEventPhase, Instant)>>,
}

impl Throttle {
    pub fn new(plan_id: u64, events: Arc<dyn DedupeEvents>) -> Throttle {
        Throttle {
            plan_id,
            events,
            last: Mutex::new(None),
        }
    }

    /// Planning progress (the planner reports from several threads).
    pub fn progress(&self, p: &DedupeProgress) {
        self.send(DedupeProgressEvent {
            plan_id: self.plan_id,
            phase: DedupeEventPhase::Plan(p.phase),
            done: p.done,
            total: p.total,
        });
    }

    /// The event is sent while the lock is held, so the final count is
    /// never overtaken by an older one.
    pub fn send(&self, event: DedupeProgressEvent) {
        let now = Instant::now();
        let mut last = lock(&self.last);
        let due = match *last {
            None => true,
            Some((phase, at)) => {
                phase != event.phase
                    || (event.total > 0 && event.done >= event.total)
                    || now.duration_since(at) >= PROGRESS_EVERY
            }
        };
        if !due {
            return;
        }
        *last = Some((event.phase, now));
        self.events.progress(event);
    }
}

impl From<DedupeError> for IpcError {
    fn from(e: DedupeError) -> Self {
        match e {
            DedupeError::Cancelled => IpcError::Cancelled,
            DedupeError::NoState => IpcError::NoDedupePlan,
            DedupeError::Cache(m) => IpcError::Io(m),
            DedupeError::QuarantineInSystemFolder(p) => {
                IpcError::QuarantineInSystemFolder(p.display().to_string())
            }
            e @ (DedupeError::NotAKeeper { .. } | DedupeError::EveryMemberMoves(_)) => {
                IpcError::KeeperRequired(e.to_string())
            }
            e => IpcError::BadRequest(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use coverly_core::dedupe::DedupeOutcome;

    #[test]
    fn dedupe_errors_keep_stable_codes() {
        assert_eq!(IpcError::from(DedupeError::Cancelled), IpcError::Cancelled);
        assert_eq!(IpcError::from(DedupeError::NoState), IpcError::NoDedupePlan);
        assert!(matches!(
            IpcError::from(DedupeError::QuarantineInSystemFolder("/sd/ROMS/GB/x".into())),
            IpcError::QuarantineInSystemFolder(p) if p == "/sd/ROMS/GB/x"
        ));
        assert!(matches!(
            IpcError::from(DedupeError::NotAKeeper {
                group: 1,
                member: 2
            }),
            IpcError::KeeperRequired(_)
        ));
        assert!(matches!(
            IpcError::from(DedupeError::EveryMemberMoves(4)),
            IpcError::KeeperRequired(_)
        ));
        assert!(matches!(
            IpcError::from(DedupeError::NoGroup(4)),
            IpcError::BadRequest(_)
        ));
        assert_eq!(
            apply_error(ApplyError::NoPlan("j".into())),
            IpcError::NoDedupePlan
        );
        assert!(matches!(
            apply_error(ApplyError::Dedupe(DedupeError::EveryMemberMoves(4))),
            IpcError::KeeperRequired(_)
        ));
    }

    #[test]
    fn a_report_with_cancelled_copies_ends_cancelled() {
        let mut report = DedupeReport::default();
        assert_eq!(end_state(&report), DedupeJobState::Done);
        report.outcomes.push(DedupeOutcome {
            group: 1,
            member: 1,
            status: DedupeItemStatus::Skipped {
                reason: DedupeSkip::Cancelled,
            },
            files: Vec::new(),
        });
        assert_eq!(end_state(&report), DedupeJobState::Cancelled);
        report.paused = Some(ItemErrorKind::DiskFull);
        assert_eq!(end_state(&report), DedupeJobState::Paused);
    }

    fn empty_stored() -> StoredDedupe {
        StoredDedupe {
            root: PathBuf::from("lib"),
            opts: DedupeOptions::default(),
            plan: DedupePlan::default(),
            skipped_groups: Default::default(),
            skipped_systems: Default::default(),
            trash_available: false,
        }
    }

    #[test]
    fn libretro_memo_lives_only_as_long_as_its_plan() {
        let m = DedupeManager::default();
        let first = m.next_id();
        m.store(first, empty_stored());
        m.remember_libretro(first, 7, Media::Cover, Some("u".into()));
        m.remember_libretro(first, 7, Media::Title, None);
        assert_eq!(
            m.libretro_memo(first, 7, Media::Cover),
            Some(Some("u".into()))
        );
        assert_eq!(m.libretro_memo(first, 7, Media::Title), Some(None));

        // A lookup for a plan that is not kept is not remembered.
        m.remember_libretro(first + 100, 7, Media::Cover, Some("x".into()));
        assert_eq!(m.libretro_memo(first + 100, 7, Media::Cover), None);

        // Evicting the plan drops its memo; a newer plan's stays.
        let mut last = first;
        for _ in 0..KEPT_PLANS {
            last = m.next_id();
            m.store(last, empty_stored());
        }
        m.remember_libretro(last, 7, Media::Cover, Some("v".into()));
        assert_eq!(m.libretro_memo(first, 7, Media::Cover), None);
        assert_eq!(m.libretro_memo(first, 7, Media::Title), None);
        assert_eq!(
            m.libretro_memo(last, 7, Media::Cover),
            Some(Some("v".into()))
        );
    }

    #[test]
    fn a_newer_plan_cancels_the_running_one() {
        let m = DedupeManager::default();
        let a = m.next_id();
        let flag_a = m.begin(a);
        let b = m.next_id();
        let flag_b = m.begin(b);
        assert!(flag_a.load(Ordering::SeqCst));
        assert!(!flag_b.load(Ordering::SeqCst));
        m.finish(a);
        m.cancel(b);
        assert!(flag_b.load(Ordering::SeqCst));
    }
}
