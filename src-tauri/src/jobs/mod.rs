//! Fill jobs of the desktop app (spec §7 steps 3–9, §8 Jobs). At most one
//! job exists at a time: the dry run gathers on a worker thread
//! (`gathering` → `planned`), review decisions are recorded on the job,
//! `run` applies the decided plan on another thread (`applying` → `done`);
//! a rate limit, going offline or a full/read-only volume pauses it, and
//! resume continues into the same journal (D21); cancel stops it. The
//! webview follows it through [`JobEvents`].

pub mod view;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use coverly_core::art::{resolve_art, RomArt, SystemArt};
use coverly_core::cache::{Cache, CACHE_FILE};
use coverly_core::credentials::SourceKeys;
use coverly_core::fill::{Endpoints, FillEnv, FillSettings};
use coverly_core::gather::Scope;
use coverly_core::jobs::{ApplyOptions, ItemStatus, JobReport};
use coverly_core::matching::parse_region_order;
use coverly_core::net::{NetConfig, PoliteHttp, UreqHttp};
use coverly_core::plan::{Decision, Plan, PlanAction};
use coverly_core::profiles::Media;
use coverly_core::progress::{PauseKind, Phase, ProgressEvent, ProgressSink};
use coverly_core::settings::Settings;
use coverly_core::systems::SystemDb;
use coverly_core::write::OverwritePolicy;

use crate::error::IpcError;
use crate::library::refresh::{entry_index, rom_path, subset_scan};
use crate::library::Session;
use view::{
    candidates_of, DecisionIn, FillRequest, ItemFilter, JobItemView, JobProgressEvent,
    JobProgressView, JobState, JobTotalsView, JobView, PauseView, PlanSummaryView,
};

/// Where fills keep their cache and journals, how they reach the network,
/// and the settings they run with: a snapshot taken when a job starts, kept
/// by the job for its dry run, its run and every resume.
#[derive(Clone, Debug)]
pub struct FillConfig {
    pub cache_dir: PathBuf,
    /// Journals and backups (the settings' backup location, D27).
    pub data_dir: PathBuf,
    pub endpoints: Endpoints,
    pub net: NetConfig,
    pub settings: Settings,
    /// Where manual identities live: the app's own data dir, so moving the
    /// backup location never forgets them (D32).
    pub overrides_dir: PathBuf,
    /// Source credentials read when the job started (never re-read by
    /// workers; redacted in `Debug`).
    pub keys: SourceKeys,
}

impl FillConfig {
    /// The real servers with the default network settings.
    pub fn new(cache_dir: PathBuf, data_dir: PathBuf) -> FillConfig {
        FillConfig {
            cache_dir,
            overrides_dir: data_dir.clone(),
            data_dir,
            endpoints: Endpoints::default(),
            net: NetConfig::default(),
            settings: Settings::default(),
            keys: SourceKeys::default(),
        }
    }
}

/// Where a job reports to (Tauri events in the app, a recorder in tests).
pub trait JobEvents: Send + Sync {
    /// Throttled progress (at most ~20 a second; first and last always).
    fn progress(&self, event: JobProgressEvent);
    /// The job changed state.
    fn changed(&self, view: JobView);
    /// Art was written for `ids` of library `generation` (sent before the
    /// final `changed`).
    fn written(&self, generation: u64, ids: Vec<u32>);
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Runs `f` with a fill environment on `cfg`: its own cache connection
/// and HTTP client. Each call has its own per-host politeness and 429
/// backoff state, so a candidate preview taken while a job applies does not
/// wait behind that job's rate limit. Accepted: previews are single
/// requests the user asks for one at a time, served from the art cache
/// once fetched; a 429 on one only fails that preview.
pub fn with_env<T>(
    cfg: &FillConfig,
    f: impl FnOnce(&FillEnv<'_>) -> Result<T, IpcError>,
) -> Result<T, IpcError> {
    let cache =
        Cache::open(&cfg.cache_dir.join(CACHE_FILE)).map_err(|e| IpcError::Io(e.to_string()))?;
    let http = PoliteHttp::new(UreqHttp::new(&cfg.net), &cfg.net);
    let env = FillEnv {
        cache: &cache,
        http: &http,
        endpoints: &cfg.endpoints,
        cache_dir: &cfg.cache_dir,
        data_dir: &cfg.data_dir,
        keys: &cfg.keys,
    };
    f(&env)
}

struct Job {
    id: u64,
    session: Arc<Session>,
    /// The settings snapshot the job started with (data folder, network,
    /// output): the dry run, the run and every resume share it.
    cfg: Arc<FillConfig>,
    /// `None` = the whole library.
    entry_filter: Option<Vec<u32>>,
    settings: FillSettings,
    overwrite: OverwritePolicy,
    cancel: AtomicBool,
    data: Mutex<JobData>,
}

struct JobData {
    state: JobState,
    progress: Option<JobProgressView>,
    paused: Option<PauseView>,
    /// The plan as gathered (decisions are applied when it runs).
    plan: Option<Plan>,
    /// Entry id of each plan item.
    entry_ids: Vec<Option<u32>>,
    decisions: BTreeMap<usize, Decision>,
    /// Outcomes, aligned with `plan.items`.
    report: Option<JobReport>,
    error: Option<String>,
}

impl JobData {
    fn new() -> JobData {
        JobData {
            state: JobState::Gathering,
            progress: None,
            paused: None,
            plan: None,
            entry_ids: Vec::new(),
            decisions: BTreeMap::new(),
            report: None,
            error: None,
        }
    }

    /// The plan with the review decisions applied.
    fn decided_plan(&self) -> Option<Plan> {
        let mut plan = self.plan.clone()?;
        plan.decide(&self.decisions);
        Some(plan)
    }
}

impl Job {
    fn running(&self) -> bool {
        matches!(
            lock(&self.data).state,
            JobState::Gathering | JobState::Applying
        )
    }

    fn view(&self) -> JobView {
        let d = lock(&self.data);
        JobView {
            id: self.id,
            generation: self.session.generation,
            state: d.state,
            progress: d.progress.clone(),
            paused: d.paused.clone(),
            plan: d.plan.as_ref().map(PlanSummaryView::of),
            decided: d.decisions.len(),
            skipped: d
                .decisions
                .values()
                .filter(|c| matches!(c, Decision::Skip))
                .count(),
            journal: d.report.as_ref().and_then(|r| r.job_id.clone()),
            result: d.report.as_ref().map(JobTotalsView::of),
            error: d.error.clone(),
        }
    }
}

/// Core progress → job state + throttled events.
struct Bridge<'a> {
    job: &'a Job,
    events: &'a dyn JobEvents,
    last: Mutex<Option<Instant>>,
}

impl<'a> Bridge<'a> {
    fn new(job: &'a Job, events: &'a dyn JobEvents) -> Bridge<'a> {
        Bridge {
            job,
            events,
            last: Mutex::new(None),
        }
    }
}

impl ProgressSink for Bridge<'_> {
    fn event(&self, e: ProgressEvent) {
        let (phase, done, total, label) = match e {
            ProgressEvent::Started { phase, total } => (phase, 0, total, String::new()),
            ProgressEvent::Item {
                phase,
                done,
                total,
                label,
            } => (phase, done, total, label),
            ProgressEvent::Warning(_) | ProgressEvent::Finished { .. } => return,
        };
        lock(&self.job.data).progress = Some(JobProgressView {
            phase,
            done,
            total,
            label: label.clone(),
        });
        let mut last = lock(&self.last);
        let due = done == 0
            || done + 1 >= total
            || last.is_none_or(|t| t.elapsed() >= Duration::from_millis(50));
        if due {
            *last = Some(Instant::now());
            drop(last);
            self.events.progress(JobProgressEvent {
                job: self.job.id,
                phase,
                done,
                total,
                label,
            });
        }
    }
}

/// The job's fill settings: the snapshot's matching and sources, with the
/// wizard's media, scope, strategy and regions (empty = the settings' order).
fn settings_for(
    session: &Session,
    req: &FillRequest,
    cfg: &FillConfig,
) -> Result<FillSettings, IpcError> {
    let mut media: Vec<Media> = Vec::new();
    for &m in &req.media {
        if session.profile.slot(m).is_some() && !media.contains(&m) {
            media.push(m);
        }
    }
    if media.is_empty() {
        return Err(IpcError::BadRequest(
            "this layout cannot store the chosen art".into(),
        ));
    }
    let base = cfg.settings.fill_settings();
    let mut regions = parse_region_order(&req.regions.join(",")).map_err(IpcError::BadRequest)?;
    if regions.is_empty() {
        regions = base.regions.clone();
    }
    Ok(FillSettings {
        media,
        scope: if req.overwrite == OverwritePolicy::Backup {
            Scope::All
        } else {
            Scope::Missing
        },
        strategy: req.strategy,
        regions,
        overrides_dir: Some(cfg.overrides_dir.clone()),
        ..base
    })
}

fn pause_view(phase: Phase, why: Option<&String>, kind: Option<PauseKind>) -> Option<PauseView> {
    why.map(|detail| PauseView {
        phase,
        kind: kind.unwrap_or(PauseKind::Offline),
        detail: detail.clone(),
    })
}

/// Runs a job phase; a panic becomes an error, so the job ends `failed`
/// instead of staying `gathering`/`applying` (and `busy`) forever.
fn guarded<T>(f: impl FnOnce() -> Result<T, IpcError>) -> Result<T, IpcError> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f))
        .unwrap_or_else(|_| Err(IpcError::Io("the fill job stopped unexpectedly".into())))
}

/// Adds the unselected ROMs of each selected folder to `art`, with no art
/// states so they are never targets. The core decides per folder whether
/// its ROMs are numbered (`001 Name`) from every stem of the folder; a
/// selection must not change that decision, or a selected ROM would match
/// worse than in a whole-library fill.
fn add_folder_peers(session: &Session, wanted: &[u32], art: &mut [SystemArt]) {
    let selected: BTreeSet<u32> = wanted.iter().copied().collect();
    for (id, entry) in session.entries.iter().enumerate() {
        if selected.contains(&(id as u32)) {
            continue;
        }
        let dir = &session.systems[entry.system as usize].dir;
        if let Some(system) = art.iter_mut().find(|s| &s.scan.dir == dir) {
            system.roms.push(RomArt {
                rom: entry.rom.clone(),
                art: BTreeMap::new(),
            });
        }
    }
}

fn spawn_gather(job: Arc<Job>, events: Arc<dyn JobEvents>) {
    std::thread::spawn(move || {
        let planned = guarded(|| {
            with_env(&job.cfg, |env| {
                let db = SystemDb::builtin();
                let session = &job.session;
                let ids: Vec<u32> = match &job.entry_filter {
                    Some(ids) => ids.clone(),
                    None => (0..session.entries.len() as u32).collect(),
                };
                let (scan, wanted) = subset_scan(session, &ids);
                let mut art = resolve_art(&scan, &session.profile, &db, &job.settings.media);
                add_folder_peers(session, &wanted, &mut art);
                let root =
                    std::path::absolute(&session.root).map_err(|e| IpcError::Io(e.to_string()))?;
                let sink = Bridge::new(&job, events.as_ref());
                Ok(env.plan(
                    &root,
                    &session.profile,
                    &db,
                    &art,
                    &job.settings,
                    &sink,
                    &job.cancel,
                ))
            })
        });
        {
            let mut d = lock(&job.data);
            d.progress = None;
            match planned {
                Err(e) => {
                    d.state = JobState::Failed;
                    d.error = Some(e.to_string());
                }
                Ok(plan) => {
                    let index = entry_index(&job.session);
                    d.entry_ids = plan
                        .items
                        .iter()
                        .map(|i| {
                            rom_path(&job.session, &i.folder, &i.rel_path)
                                .and_then(|p| index.get(&p).copied())
                        })
                        .collect();
                    d.decisions.clear();
                    d.report = None;
                    d.paused = pause_view(Phase::Gather, plan.paused.as_ref(), plan.pause_kind);
                    d.state = if job.cancel.load(Ordering::SeqCst) {
                        JobState::Cancelled
                    } else if d.paused.is_some() {
                        JobState::Paused
                    } else {
                        JobState::Planned
                    };
                    d.plan = Some(plan);
                }
            }
        }
        events.changed(job.view());
    });
}

/// Applies `plan`. `resumed`: the original indices of `plan.items` and
/// the journal to continue.
fn spawn_apply(
    job: Arc<Job>,
    events: Arc<dyn JobEvents>,
    plan: Plan,
    resumed: Option<(Vec<usize>, String)>,
) {
    std::thread::spawn(move || {
        let applied = guarded(|| {
            let cfg = &job.cfg;
            with_env(cfg, |env| {
                let root = std::path::absolute(&job.session.root)
                    .map_err(|e| IpcError::Io(e.to_string()))?;
                let opts = ApplyOptions {
                    overwrite: job.overwrite,
                    // Only auto items and accepted/picked review items (D20).
                    include_review: false,
                    concurrency: cfg.net.concurrency.clamp(1, 6),
                    resume_job: resumed.as_ref().map(|(_, journal)| journal.clone()),
                    ..cfg.settings.apply_options(&root)
                };
                let sink = Bridge::new(&job, events.as_ref());
                env.apply(&plan, &job.session.profile, &opts, &sink, &job.cancel)
                    .map_err(IpcError::from)
            })
        });
        let mut touched = Vec::new();
        {
            let mut d = lock(&job.data);
            d.progress = None;
            match applied {
                Err(e) => {
                    d.state = JobState::Failed;
                    d.error = Some(e.to_string());
                }
                Ok(report) => {
                    for (k, item) in report.items.iter().enumerate() {
                        if matches!(
                            item.status,
                            ItemStatus::Filled { .. } | ItemStatus::Replaced { .. }
                        ) {
                            let n = resumed.as_ref().map_or(k, |(indices, _)| indices[k]);
                            if let Some(Some(id)) = d.entry_ids.get(n) {
                                touched.push(*id);
                            }
                        }
                    }
                    let merged = match (resumed, d.report.take()) {
                        (Some((indices, _)), Some(mut first)) => {
                            first.merge_resumed(&indices, report);
                            first
                        }
                        _ => report,
                    };
                    d.paused = pause_view(Phase::Apply, merged.paused.as_ref(), merged.pause_kind);
                    d.state = if merged.cancelled {
                        JobState::Cancelled
                    } else if d.paused.is_some() {
                        JobState::Paused
                    } else {
                        JobState::Done
                    };
                    d.report = Some(merged);
                }
            }
        }
        if !touched.is_empty() {
            events.written(job.session.generation, touched);
        }
        let view = job.view();
        if let Some(t) = &view.result {
            log::info!(
                "fill job {} {:?}: {} filled, {} replaced, {} skipped, {} without art, {} failed",
                view.id,
                view.state,
                t.filled,
                t.replaced,
                t.skipped,
                t.no_art,
                t.failed
            );
        }
        events.changed(view);
    });
}

/// The one fill job of the app.
#[derive(Default)]
pub struct JobManager {
    seq: AtomicU64,
    current: Mutex<Option<Arc<Job>>>,
    /// A manual write (drawer change, local image, removal, undo, an
    /// organise apply) runs.
    manual: Arc<AtomicBool>,
}

/// A manual write in progress; dropping it ends it.
#[must_use]
pub struct ManualWrite<'a>(&'a AtomicBool);

impl Drop for ManualWrite<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// A manual write a worker thread holds (an organise apply); dropping it
/// ends it, like [`ManualWrite`].
#[must_use]
pub struct OwnedManualWrite(Arc<AtomicBool>);

impl Drop for OwnedManualWrite {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

impl JobManager {
    /// Marks a manual write in progress until the guard drops, so the
    /// library never has two writers: refused while a job gathers or
    /// applies or another manual write runs, and `start`, `run` and
    /// `resume` are refused while it is held. The check and the mark are
    /// made under the locks those take.
    pub fn begin_manual(&self) -> Result<ManualWrite<'_>, IpcError> {
        self.mark_manual()?;
        Ok(ManualWrite(&self.manual))
    }

    /// [`Self::begin_manual`] for a write that outlives the call (it runs
    /// on a worker thread that drops the guard when it ends).
    pub fn begin_manual_owned(&self) -> Result<OwnedManualWrite, IpcError> {
        self.mark_manual()?;
        Ok(OwnedManualWrite(Arc::clone(&self.manual)))
    }

    /// Whether the job is paused (it resumes into its journal: nothing
    /// else may move the files under it meanwhile).
    pub fn paused(&self) -> bool {
        lock(&self.current)
            .as_ref()
            .is_some_and(|j| lock(&j.data).state == JobState::Paused)
    }

    fn mark_manual(&self) -> Result<(), IpcError> {
        let slot = lock(&self.current);
        let data = slot.as_ref().map(|j| lock(&j.data));
        let running = data
            .as_ref()
            .is_some_and(|d| matches!(d.state, JobState::Gathering | JobState::Applying));
        if running
            || self
                .manual
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_err()
        {
            return Err(IpcError::JobRunning);
        }
        Ok(())
    }

    fn manual_running(&self) -> bool {
        self.manual.load(Ordering::SeqCst)
    }

    /// Whether the job is gathering or applying.
    pub fn busy(&self) -> bool {
        lock(&self.current).as_ref().is_some_and(|j| j.running())
    }

    pub fn current(&self) -> Option<JobView> {
        lock(&self.current).as_ref().map(|j| j.view())
    }

    fn job(&self, id: u64) -> Result<Arc<Job>, IpcError> {
        match lock(&self.current).as_ref() {
            None => Err(IpcError::NoJob),
            Some(j) if j.id != id => Err(IpcError::StaleJob),
            Some(j) => Ok(Arc::clone(j)),
        }
    }

    /// Starts the dry run of `req` over `session`. Refused while another
    /// job gathers or applies; a stopped job is replaced.
    pub fn start(
        &self,
        cfg: Arc<FillConfig>,
        session: Arc<Session>,
        req: &FillRequest,
        events: Arc<dyn JobEvents>,
    ) -> Result<JobView, IpcError> {
        let settings = settings_for(&session, req, &cfg)?;
        let job = {
            let mut slot = lock(&self.current);
            if self.manual_running() || slot.as_ref().is_some_and(|j| j.running()) {
                return Err(IpcError::JobRunning);
            }
            let job = Arc::new(Job {
                id: self.seq.fetch_add(1, Ordering::SeqCst) + 1,
                session,
                cfg,
                entry_filter: req.entry_ids.clone(),
                settings,
                overwrite: req.overwrite,
                cancel: AtomicBool::new(false),
                data: Mutex::new(JobData::new()),
            });
            *slot = Some(Arc::clone(&job));
            job
        };
        spawn_gather(Arc::clone(&job), events);
        Ok(job.view())
    }

    pub fn items(&self, job: u64, filter: ItemFilter) -> Result<Vec<JobItemView>, IpcError> {
        let job = self.job(job)?;
        let d = lock(&job.data);
        let Some(plan) = &d.plan else {
            return Ok(Vec::new());
        };
        let outcome = |n: usize| {
            d.report
                .as_ref()
                .and_then(|r| r.items.get(n))
                .map(|r| &r.status)
        };
        Ok(plan
            .items
            .iter()
            .enumerate()
            .filter(|(n, item)| match filter {
                ItemFilter::All => true,
                ItemFilter::Auto => matches!(item.action, PlanAction::Auto { .. }),
                ItemFilter::Review => matches!(item.action, PlanAction::Review { .. }),
                ItemFilter::Failed => matches!(outcome(*n), Some(ItemStatus::Failed { .. })),
            })
            .map(|(n, item)| {
                JobItemView::new(
                    n,
                    item,
                    d.entry_ids.get(n).copied().flatten(),
                    d.decisions.get(&n).copied(),
                    outcome(n),
                )
            })
            .collect())
    }

    /// Records review decisions (only before the job runs).
    pub fn decide(&self, job: u64, decisions: &[DecisionIn]) -> Result<JobView, IpcError> {
        let job = self.job(job)?;
        {
            let mut d = lock(&job.data);
            if self.manual_running() {
                return Err(IpcError::JobRunning);
            }
            if d.state != JobState::Planned {
                return Err(IpcError::BadRequest(
                    "decisions are taken before the job runs".into(),
                ));
            }
            let items = d.plan.as_ref().map_or(&[][..], |p| &p.items[..]);
            for x in decisions {
                let item = items
                    .get(x.index as usize)
                    .ok_or_else(|| IpcError::BadRequest(format!("no item {}", x.index)))?;
                // Decisions the plan would drop silently (a `none` item, a
                // pick past the last candidate) are refused instead.
                let candidates = candidates_of(item).len();
                if candidates == 0 {
                    return Err(IpcError::BadRequest(format!(
                        "item {} has nothing to decide",
                        x.index
                    )));
                }
                if let Decision::Pick { candidate } = x.decision {
                    if candidate >= candidates {
                        return Err(IpcError::BadRequest(format!(
                            "no candidate {candidate} for item {}",
                            x.index
                        )));
                    }
                }
            }
            for x in decisions {
                d.decisions.insert(x.index as usize, x.decision);
            }
        }
        Ok(job.view())
    }

    /// Applies the decided plan on a worker thread, with the settings
    /// snapshot the job started with.
    pub fn run(&self, job: u64, events: Arc<dyn JobEvents>) -> Result<JobView, IpcError> {
        let job = self.job(job)?;
        let plan = {
            let mut d = lock(&job.data);
            if self.manual_running() {
                return Err(IpcError::JobRunning);
            }
            if d.state != JobState::Planned {
                return Err(IpcError::BadRequest("the job is not ready to run".into()));
            }
            let plan = d.decided_plan().ok_or(IpcError::NoJob)?;
            d.state = JobState::Applying;
            plan
        };
        spawn_apply(Arc::clone(&job), events, plan, None);
        Ok(job.view())
    }

    /// Continues a paused job: the dry run again, or the items not done yet
    /// into the same journal (in the data folder the job started with).
    pub fn resume(&self, job: u64, events: Arc<dyn JobEvents>) -> Result<JobView, IpcError> {
        let job = self.job(job)?;
        let mut d = lock(&job.data);
        if self.manual_running() {
            return Err(IpcError::JobRunning);
        }
        if d.state != JobState::Paused {
            return Err(IpcError::BadRequest("the job is not paused".into()));
        }
        let phase = d.paused.as_ref().map_or(Phase::Apply, |p| p.phase);
        if phase == Phase::Gather {
            d.paused = None;
            d.state = JobState::Gathering;
            drop(d);
            spawn_gather(Arc::clone(&job), events);
        } else {
            let plan = d.decided_plan().ok_or(IpcError::NoJob)?;
            let report = d.report.as_ref().ok_or(IpcError::NoJob)?;
            let indices = report.resumable_items();
            let (todo, resumed) = match report.job_id.clone() {
                Some(journal) => (plan.subset(&indices), Some((indices, journal))),
                None => (plan, None),
            };
            // Cleared only now: a failed check above leaves the pause intact.
            d.paused = None;
            d.state = JobState::Applying;
            drop(d);
            spawn_apply(Arc::clone(&job), events, todo, resumed);
        }
        Ok(job.view())
    }

    /// Stops a running job (its thread ends it as `cancelled`) or ends a
    /// planned or paused one.
    pub fn cancel(&self, job: u64) -> Result<JobView, IpcError> {
        let job = self.job(job)?;
        job.cancel.store(true, Ordering::SeqCst);
        {
            let mut d = lock(&job.data);
            if matches!(d.state, JobState::Planned | JobState::Paused) {
                d.state = JobState::Cancelled;
                d.paused = None;
            }
        }
        Ok(job.view())
    }

    /// Candidate `candidate` of item `index`, re-encoded as it would be written.
    pub fn candidate_image(
        &self,
        cfg: &FillConfig,
        job: u64,
        index: u32,
        candidate: u32,
    ) -> Result<Vec<u8>, IpcError> {
        let job = self.job(job)?;
        let c = {
            let d = lock(&job.data);
            let item = d
                .plan
                .as_ref()
                .and_then(|p| p.items.get(index as usize))
                .ok_or_else(|| IpcError::BadRequest(format!("no item {index}")))?;
            candidates_of(item)
                .get(candidate as usize)
                .map(|c| (*c).clone())
                .ok_or_else(|| IpcError::BadRequest(format!("no candidate {candidate}")))?
        };
        with_env(cfg, |env| {
            Ok(env.candidate_image(&job.session.profile, &c)?.bytes)
        })
    }

    /// The outcomes so far (for exports).
    pub fn report(&self, job: u64) -> Result<JobReport, IpcError> {
        let job = self.job(job)?;
        let d = lock(&job.data);
        d.report
            .clone()
            .ok_or_else(|| IpcError::BadRequest("the job has not run yet".into()))
    }
}
