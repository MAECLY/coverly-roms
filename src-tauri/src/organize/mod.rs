//! Organising a loose pile from the app (organise spec §4, §11): the plans
//! the backend keeps under a `plan_id` (the webview only sends the id and
//! its decisions back), the survey a re-plan reuses, and cancelling the
//! plan being made. Free of Tauri types; `AppState` drives it.

pub mod view;

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use coverly_core::organize::{
    OrganizeError, OrganizeOptions, OrganizePhase, OrganizePlan, OrganizeProgress, Survey,
};

use coverly_core::profiles::Profile;

use crate::error::IpcError;
use view::{OrganizeProgressEvent, OrganizeRequest};

/// Plans kept for export, re-planning and apply; older ones are dropped.
pub const KEPT_PLANS: usize = 4;

/// Progress is sent at most this often (phase changes and ends always).
const PROGRESS_EVERY: Duration = Duration::from_millis(100);

/// Where planning reports to (Tauri events in the app, a recorder in tests).
pub trait OrganizeEvents: Send + Sync {
    fn progress(&self, event: OrganizeProgressEvent);
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
                phase != p.phase
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
