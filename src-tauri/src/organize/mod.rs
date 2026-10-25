//! Organising a loose pile from the app (organise spec §4, §11): the plans
//! the backend keeps under a `plan_id` (the webview only sends the id and
//! its decisions back), the survey a re-plan reuses, cancelling the plan
//! being made, and the one apply (or resume) running on a worker thread
//! behind the fill jobs' manual-write guard. Free of Tauri types;
//! `AppState` drives it.

pub mod view;

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use coverly_core::organize::{
    ApplyEnv, ApplyError, ItemStatus, OrganizeError, OrganizeOptions, OrganizePhase, OrganizePlan,
    OrganizeProgress, OrganizeReport, SkipReason, Survey,
};

use coverly_core::archive::external::FoundTool;
use coverly_core::jobs::ItemErrorKind;
use coverly_core::profiles::Profile;

use crate::error::IpcError;
use crate::jobs::OwnedManualWrite;
use view::{OrganizeJobState, OrganizeJobView, OrganizeProgressEvent, OrganizeRequest};

/// Plans kept for export, re-planning and apply; older ones are dropped.
pub const KEPT_PLANS: usize = 4;

/// Progress is sent at most this often (phase changes and ends always).
const PROGRESS_EVERY: Duration = Duration::from_millis(100);

/// Where planning and applying report to (Tauri events in the app, a
/// recorder in tests).
pub trait OrganizeEvents: Send + Sync {
    /// Throttled progress of a plan or an apply.
    fn progress(&self, event: OrganizeProgressEvent);
    /// An apply or resume ended (done, paused, cancelled or failed).
    fn changed(&self, _view: OrganizeJobView) {}
}

/// One dry run as the backend keeps it.
#[derive(Debug)]
pub struct StoredPlan {
    pub request: OrganizeRequest,
    /// Absolute.
    pub source: PathBuf,
    /// The options the plan was made with (request over settings).
    pub opts: OrganizeOptions,
    /// The layout profile as it was when surveyed: the walk skipped its
    /// ROMs and art folders, so an edited custom profile surveys again.
    pub layout: Profile,
    /// Whether online identification was available to the survey.
    pub online: bool,
    /// What the pile held: re-plans with other decisions, names or mode
    /// reuse it without reading a file.
    pub survey: Arc<Survey>,
    pub plan: OrganizePlan,
}

#[derive(Default)]
struct Inner {
    next_id: u64,
    /// Oldest first, at most [`KEPT_PLANS`].
    plans: VecDeque<(u64, Arc<StoredPlan>)>,
    /// The survey being made, with its cancel flag.
    running: Option<(u64, Arc<AtomicBool>)>,
    /// The current (or last) apply of this app session.
    apply: Option<Arc<ApplyJob>>,
}

/// The plans of the organise screen and the one survey in progress.
#[derive(Default)]
pub struct OrganizeManager {
    inner: Mutex<Inner>,
}

impl OrganizeManager {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// A new plan id.
    pub fn next_id(&self) -> u64 {
        let mut inner = self.lock();
        inner.next_id += 1;
        inner.next_id
    }

    /// Marks survey `id` as the one in progress and returns its cancel
    /// flag. A survey still running is cancelled: the newer request
    /// supersedes it.
    pub fn begin(&self, id: u64) -> Arc<AtomicBool> {
        let flag = Arc::new(AtomicBool::new(false));
        let mut inner = self.lock();
        if let Some((_, old)) = inner.running.replace((id, Arc::clone(&flag))) {
            old.store(true, Ordering::SeqCst);
        }
        flag
    }

    /// Survey `id` ended (done, failed or cancelled).
    pub fn finish(&self, id: u64) {
        let mut inner = self.lock();
        if inner.running.as_ref().is_some_and(|(r, _)| *r == id) {
            inner.running = None;
        }
    }

    /// Cancels survey `id` if it is still running; anything else is a no-op.
    pub fn cancel(&self, id: u64) {
        let inner = self.lock();
        if let Some((r, flag)) = &inner.running {
            if *r == id {
                flag.store(true, Ordering::SeqCst);
            }
        }
    }

    /// Keeps `plan` under `id`, dropping the oldest beyond [`KEPT_PLANS`].
    pub fn store(&self, id: u64, plan: StoredPlan) -> Arc<StoredPlan> {
        let plan = Arc::new(plan);
        let mut inner = self.lock();
        inner.plans.push_back((id, Arc::clone(&plan)));
        while inner.plans.len() > KEPT_PLANS {
            inner.plans.pop_front();
        }
        plan
    }

    /// Plan `id`, while it is kept.
    pub fn get(&self, id: u64) -> Result<Arc<StoredPlan>, IpcError> {
        self.lock()
            .plans
            .iter()
            .find(|(i, _)| *i == id)
            .map(|(_, p)| Arc::clone(p))
            .ok_or(IpcError::NoPlan)
    }

    /// The newest kept survey of `source` made with options that only
    /// differ from `opts` in what a plan decides on its own (mode, names),
    /// with the same `layout` profile and online availability.
    pub fn survey_for(
        &self,
        source: &Path,
        opts: &OrganizeOptions,
        layout: &Profile,
        online: bool,
    ) -> Option<Arc<Survey>> {
        let key = survey_key(opts);
        self.lock()
            .plans
            .iter()
            .rev()
            .find(|(_, p)| {
                p.source == source
                    && survey_key(&p.opts) == key
                    && p.layout == *layout
                    && p.online == online
            })
            .map(|(_, p)| Arc::clone(&p.survey))
    }

    /// Drops every kept plan (e.g. after the pile changed on disk).
    pub fn forget_all(&self) {
        self.lock().plans.clear();
    }

    /// Makes `job` the current apply. The caller holds the manual-write
    /// guard, so no other apply runs.
    fn install(&self, job: Arc<ApplyJob>) {
        self.lock().apply = Some(job);
    }

    /// The current (or last) apply.
    pub fn current_apply(&self) -> Option<OrganizeJobView> {
        self.lock().apply.as_ref().map(|j| j.view())
    }

    /// Asks the running apply to stop before its next item (or inside an
    /// extraction, before its files are renamed into place); a no-op when
    /// none runs.
    pub fn cancel_apply(&self) {
        if let Some(job) = &self.lock().apply {
            if job.view().state == OrganizeJobState::Applying {
                job.cancel.store(true, Ordering::SeqCst);
            }
        }
    }

    /// Runs `apply` on a worker thread holding `guard` (fills and other
    /// writes are refused until it ends) as the current apply. Progress
    /// goes to `events` (throttled) and the end as `changed`; a panic ends
    /// it as `failed`.
    pub fn spawn_apply<F>(
        &self,
        start: ApplyStart,
        guard: OwnedManualWrite,
        events: Arc<dyn OrganizeEvents>,
        apply: F,
    ) -> OrganizeJobView
    where
        F: FnOnce(&ApplyEnv) -> Result<OrganizeReport, ApplyError> + Send + 'static,
    {
        let job = Arc::new(ApplyJob {
            plan_id: start.plan_id,
            target_root: start.target_root,
            layout: start.layout,
            data_dir: start.data_dir,
            external_tools: start.external_tools,
            cancel: AtomicBool::new(false),
            data: Mutex::new(ApplyData {
                job: start.job,
                state: OrganizeJobState::Applying,
                progress: None,
                report: None,
                error: None,
            }),
        });
        self.install(Arc::clone(&job));
        let view = job.view();
        std::thread::spawn(move || {
            let _guard = guard;
            let throttle = Throttle::new(job.plan_id, Arc::clone(&events));
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let progress = |p: OrganizeProgress| {
                    job.data().progress = Some(OrganizeProgressEvent {
                        plan_id: job.plan_id,
                        phase: p.phase,
                        done: p.done,
                        total: p.total,
                    });
                    throttle.progress(p);
                };
                // External tools off: an extraction never looks for one;
                // otherwise they are found on first need.
                let no_tools: &[FoundTool] = &[];
                let env = ApplyEnv {
                    data_dir: &job.data_dir,
                    progress: &progress,
                    cancel: &job.cancel,
                    trash: &coverly_core::write::trash::OsTrash,
                    tools: (!job.external_tools).then_some(no_tools),
                };
                apply(&env).map_err(IpcError::from)
            }))
            .unwrap_or_else(|_| Err(IpcError::Io("the organise job stopped unexpectedly".into())));
            job.end(outcome);
            let view = job.view();
            match &view.report {
                Some(r) => log::info!(
                    "organise job {:?} {:?}: {} moved, {} copied, {} skipped, {} failed",
                    view.job,
                    view.state,
                    r.moved,
                    r.copied,
                    r.skipped,
                    r.failed
                ),
                None => log::warn!("organise job {:?} failed: {:?}", view.job, view.error),
            }
            events.changed(view);
        });
        view
    }
}

/// What an apply starts with.
pub struct ApplyStart {
    pub plan_id: u64,
    /// The journal a resume continues.
    pub job: Option<String>,
    pub target_root: String,
    pub layout: String,
    /// Journals and saved plans (the job's settings snapshot).
    pub data_dir: PathBuf,
    /// `organize.external_tools` is on: extractions may run 7-Zip or unrar.
    pub external_tools: bool,
}

/// One apply (or resume) of an organise plan.
pub struct ApplyJob {
    plan_id: u64,
    target_root: String,
    layout: String,
    data_dir: PathBuf,
    external_tools: bool,
    cancel: AtomicBool,
    data: Mutex<ApplyData>,
}

struct ApplyData {
    job: Option<String>,
    state: OrganizeJobState,
    progress: Option<OrganizeProgressEvent>,
    report: Option<OrganizeReport>,
    error: Option<IpcError>,
}

impl ApplyJob {
    fn data(&self) -> MutexGuard<'_, ApplyData> {
        self.data.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn view(&self) -> OrganizeJobView {
        let d = self.data();
        OrganizeJobView {
            job: d.job.clone(),
            plan_id: self.plan_id,
            state: d.state,
            progress: d.progress,
            report: d.report.clone(),
            target_root: self.target_root.clone(),
            layout: self.layout.clone(),
            error: d.error.clone(),
        }
    }

    /// Records how the worker ended.
    fn end(&self, outcome: Result<OrganizeReport, IpcError>) {
        let mut d = self.data();
        d.progress = None;
        match outcome {
            Ok(report) if report.paused.is_some() && report.job.is_none() => {
                // Paused before anything was journaled (a root was gone at
                // the start): there is no job `organize_resume` could
                // continue, so it ends as failed with the reason.
                let kind = report.pause_reason.unwrap_or(ItemErrorKind::VolumeGone);
                d.state = OrganizeJobState::Failed;
                d.error = Some(crate::state::item_error(kind, &self.target_root));
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
                d.state = OrganizeJobState::Failed;
                d.error = Some(e);
            }
        }
    }
}

/// Paused, cancelled (some item was skipped for it) or done.
fn end_state(report: &OrganizeReport) -> OrganizeJobState {
    let cancelled = report.items.iter().any(|i| {
        i.status
            == ItemStatus::Skipped {
                reason: SkipReason::Cancelled,
            }
    });
    if report.paused.is_some() {
        OrganizeJobState::Paused
    } else if cancelled {
        OrganizeJobState::Cancelled
    } else {
        OrganizeJobState::Done
    }
}

/// The options a survey depends on: everything but the transfer mode and
/// the names (layout, target, flat, offline, thresholds, folders…).
fn survey_key(opts: &OrganizeOptions) -> OrganizeOptions {
    OrganizeOptions {
        mode: None,
        names: Default::default(),
        ..opts.clone()
    }
}

/// Sends planning progress of `plan_id` at most every 100 ms; the first
/// event of a phase and the last of a count always go through.
pub struct Throttle {
    plan_id: u64,
    events: Arc<dyn OrganizeEvents>,
    last: Mutex<Option<(OrganizePhase, Instant)>>,
}

impl Throttle {
    pub fn new(plan_id: u64, events: Arc<dyn OrganizeEvents>) -> Throttle {
        Throttle {
            plan_id,
            events,
            last: Mutex::new(None),
        }
    }

    /// Survey workers call this from several threads: the event is sent
    /// while the lock is held, so the final count is never overtaken by an
    /// older one (the UI would otherwise stay at `n - 1 / n`).
    pub fn progress(&self, p: OrganizeProgress) {
        let now = Instant::now();
        let mut last = self.last.lock().unwrap_or_else(PoisonError::into_inner);
        let due = match *last {
            None => true,
            Some((phase, at)) => {
                // An extraction's byte counts are progress, not a new phase;
                // its start (a new item to name) and its end (the bar full)
                // always go out.
                let extract_edge = match p.phase {
                    OrganizePhase::Extract {
                        bytes_done,
                        bytes_total,
                        ..
                    } => bytes_done == 0 || bytes_done >= bytes_total,
                    _ => false,
                };
                std::mem::discriminant(&phase) != std::mem::discriminant(&p.phase)
                    || extract_edge
                    || (p.total > 0 && p.done >= p.total)
                    || now.duration_since(at) >= PROGRESS_EVERY
            }
        };
        if !due {
            return;
        }
        *last = Some((p.phase, now));
        self.events.progress(OrganizeProgressEvent {
            plan_id: self.plan_id,
            phase: p.phase,
            done: p.done,
            total: p.total,
        });
    }
}

impl From<OrganizeError> for IpcError {
    fn from(e: OrganizeError) -> Self {
        match e {
            OrganizeError::SourceMissing(p) => IpcError::NotFound(p.display().to_string()),
            OrganizeError::Unreadable(p, _) => IpcError::Unreadable(p.display().to_string()),
            OrganizeError::UnknownLayout(id) => IpcError::UnknownProfile(id),
            OrganizeError::Cancelled => IpcError::Cancelled,
            other @ (OrganizeError::UnknownSystem(_) | OrganizeError::UnknownItem(_)) => {
                IpcError::BadRequest(other.to_string())
            }
        }
    }
}

impl From<ApplyError> for IpcError {
    fn from(e: ApplyError) -> Self {
        match e {
            ApplyError::NoSpace { needed, free } => IpcError::NoSpace(format!("{needed}/{free}")),
            ApplyError::Write(w) => w.into(),
            ApplyError::NoPlan(_) => IpcError::NoPlan,
            e @ ApplyError::Undone(_) => IpcError::BadRequest(e.to_string()),
            ApplyError::Dedupe(d) => IpcError::BadRequest(d.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use coverly_core::organize::{NameMode, TransferMode};

    struct Sink(Mutex<Vec<OrganizeProgressEvent>>);

    impl OrganizeEvents for Sink {
        fn progress(&self, event: OrganizeProgressEvent) {
            self.0.lock().unwrap().push(event);
        }
    }

    #[test]
    fn throttle_keeps_phase_changes_and_ends() {
        let sink = Arc::new(Sink(Mutex::new(Vec::new())));
        let t = Throttle::new(9, sink.clone());
        let p = |phase, done, total| OrganizeProgress { phase, done, total };
        t.progress(p(OrganizePhase::Walk, 0, 0));
        t.progress(p(OrganizePhase::Identify, 0, 500));
        for done in 1..500 {
            t.progress(p(OrganizePhase::Identify, done, 500));
        }
        t.progress(p(OrganizePhase::Identify, 500, 500));
        let got = sink.0.lock().unwrap().clone();
        assert!(got.iter().all(|e| e.plan_id == 9));
        assert_eq!(got.first().unwrap().phase, OrganizePhase::Walk);
        assert_eq!(got[1].phase, OrganizePhase::Identify);
        assert_eq!(got.last().unwrap().done, 500);
        assert!(got.len() < 10, "throttled: {} events", got.len());
    }

    #[test]
    fn extract_byte_counts_are_throttled_like_one_phase() {
        let sink = Arc::new(Sink(Mutex::new(Vec::new())));
        let t = Throttle::new(2, sink.clone());
        let p = |bytes_done| OrganizeProgress {
            phase: OrganizePhase::Extract {
                item: 0,
                bytes_done,
                bytes_total: 1000,
            },
            done: 0,
            total: 1,
        };
        for b in 0..1000 {
            t.progress(p(b));
        }
        let got = sink.0.lock().unwrap().clone();
        assert!(!got.is_empty());
        assert!(got.len() < 10, "throttled: {} events", got.len());
    }

    #[test]
    fn an_extractions_start_and_end_are_never_dropped() {
        let sink = Arc::new(Sink(Mutex::new(Vec::new())));
        let t = Throttle::new(2, sink.clone());
        let p = |item, bytes_done| OrganizeProgress {
            phase: OrganizePhase::Extract {
                item,
                bytes_done,
                bytes_total: 1000,
            },
            done: item,
            total: 3,
        };
        // Two quick extractions in a row, well inside one throttle window.
        for item in 0..2 {
            for b in [0, 10, 500, 1000] {
                t.progress(p(item, b));
            }
        }
        let got: Vec<_> = sink.0.lock().unwrap().iter().map(|e| e.phase).collect();
        let at = |item, bytes_done| OrganizePhase::Extract {
            item,
            bytes_done,
            bytes_total: 1000,
        };
        // Each item's start (its name shows up) and its end (the bar fills)
        // reach the webview; the steps in between are throttled.
        assert_eq!(got, vec![at(0, 0), at(0, 1000), at(1, 0), at(1, 1000)]);
    }

    /// Blocks inside `progress` for `done == 1` until released.
    struct Gate {
        events: Mutex<Vec<OrganizeProgressEvent>>,
        entered: std::sync::mpsc::SyncSender<()>,
        release: Mutex<std::sync::mpsc::Receiver<()>>,
    }

    impl OrganizeEvents for Gate {
        fn progress(&self, event: OrganizeProgressEvent) {
            if event.done == 1 {
                self.entered.send(()).unwrap();
                self.release.lock().unwrap().recv().unwrap();
            }
            self.events.lock().unwrap().push(event);
        }
    }

    #[test]
    fn the_final_count_is_never_overtaken_by_an_older_one() {
        // Survey workers report from several threads: the event for the
        // last item must reach the UI last, or it stays at 1/2.
        let (entered_tx, entered_rx) = std::sync::mpsc::sync_channel(1);
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let gate = Arc::new(Gate {
            events: Mutex::new(Vec::new()),
            entered: entered_tx,
            release: Mutex::new(release_rx),
        });
        let t = Arc::new(Throttle::new(1, gate.clone()));
        let p = |done| OrganizeProgress {
            phase: OrganizePhase::Identify,
            done,
            total: 2,
        };
        let first = {
            let t = Arc::clone(&t);
            std::thread::spawn(move || t.progress(p(1)))
        };
        entered_rx.recv().unwrap();
        let last = {
            let t = Arc::clone(&t);
            std::thread::spawn(move || t.progress(p(2)))
        };
        std::thread::sleep(Duration::from_millis(50));
        release_tx.send(()).unwrap();
        first.join().unwrap();
        last.join().unwrap();
        let got = gate.events.lock().unwrap().clone();
        assert_eq!(got.last().unwrap().done, 2, "{got:?}");
    }

    #[test]
    fn a_newer_survey_cancels_the_running_one() {
        let m = OrganizeManager::default();
        let a = m.next_id();
        let flag_a = m.begin(a);
        let b = m.next_id();
        let flag_b = m.begin(b);
        assert!(flag_a.load(Ordering::SeqCst));
        assert!(!flag_b.load(Ordering::SeqCst));
        m.finish(a); // late finish of the superseded one changes nothing
        m.cancel(b);
        assert!(flag_b.load(Ordering::SeqCst));
        m.finish(b);
        m.cancel(b);
    }

    #[test]
    fn mode_and_names_do_not_need_a_new_survey() {
        let base = OrganizeOptions::default();
        let other = OrganizeOptions {
            mode: Some(TransferMode::Copy),
            names: NameMode::Catalogue,
            ..base.clone()
        };
        assert_eq!(survey_key(&base), survey_key(&other));
        let offline = OrganizeOptions {
            offline: true,
            ..base.clone()
        };
        assert_ne!(survey_key(&base), survey_key(&offline));
        let flat = OrganizeOptions {
            flat: true,
            ..base.clone()
        };
        assert_ne!(survey_key(&base), survey_key(&flat));
    }

    #[test]
    fn apply_errors_keep_stable_codes() {
        assert_eq!(
            IpcError::from(ApplyError::NoSpace {
                needed: 2000,
                free: 1000
            }),
            IpcError::NoSpace("2000/1000".into())
        );
        assert_eq!(
            IpcError::from(ApplyError::NoPlan("j".into())),
            IpcError::NoPlan
        );
        assert_eq!(
            IpcError::from(ApplyError::Write(coverly_core::write::WriteError::DiskFull)),
            IpcError::DiskFull
        );
    }

    #[test]
    fn a_report_with_cancelled_items_ends_cancelled() {
        use coverly_core::organize::ItemOutcome;
        let mut report = OrganizeReport::default();
        assert_eq!(end_state(&report), OrganizeJobState::Done);
        report.items.push(ItemOutcome {
            item: 0,
            status: ItemStatus::Skipped {
                reason: SkipReason::Cancelled,
            },
            targets: Vec::new(),
        });
        assert_eq!(end_state(&report), OrganizeJobState::Cancelled);
        report.paused = Some(coverly_core::progress::PauseKind::Volume);
        assert_eq!(end_state(&report), OrganizeJobState::Paused);
    }

    #[test]
    fn organise_errors_keep_stable_codes() {
        assert_eq!(
            IpcError::from(OrganizeError::UnknownLayout("x".into())),
            IpcError::UnknownProfile("x".into())
        );
        assert_eq!(
            IpcError::from(OrganizeError::Cancelled),
            IpcError::Cancelled
        );
        assert!(matches!(
            IpcError::from(OrganizeError::UnknownItem(3)),
            IpcError::BadRequest(_)
        ));
        assert!(matches!(
            IpcError::from(OrganizeError::SourceMissing("/x".into())),
            IpcError::NotFound(_)
        ));
    }
}
