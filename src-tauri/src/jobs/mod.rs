//! Fill jobs of the desktop app (spec §7 steps 3–9, §8 Jobs). At most one
//! job exists at a time: the dry run gathers on a worker thread
//! (`gathering` → `planned`), review decisions are recorded on the job,
//! `run` applies the decided plan on another thread (`applying` → `done`);
//! a rate limit, going offline or a full/read-only volume pauses it, and
//! resume continues into the same journal (D21); cancel stops it. The
//! webview follows it through [`JobEvents`].

pub mod view;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use coverly_core::art::resolve_art;
use coverly_core::cache::{Cache, CACHE_FILE};
use coverly_core::fill::{Endpoints, FillEnv, FillSettings};
use coverly_core::gather::Scope;
use coverly_core::jobs::{ApplyOptions, ItemStatus, JobReport};
use coverly_core::matching::{default_region_order, parse_region_order};
use coverly_core::net::{NetConfig, PoliteHttp, UreqHttp};
use coverly_core::plan::{Decision, Plan, PlanAction};
use coverly_core::profiles::Media;
use coverly_core::progress::{PauseKind, Phase, ProgressEvent, ProgressSink};
use coverly_core::systems::SystemDb;
use coverly_core::write::OverwritePolicy;

use crate::error::IpcError;
use crate::library::refresh::{entry_index, rom_path, subset_scan};
use crate::library::Session;
use view::{
    candidates_of, DecisionIn, FillRequest, ItemFilter, JobItemView, JobProgressEvent,
    JobProgressView, JobState, JobTotalsView, JobView, PauseView, PlanSummaryView,
};

/// Where fills keep their cache and journals, and how they reach the network.
#[derive(Clone, Debug)]
pub struct FillConfig {
    pub cache_dir: PathBuf,
    pub data_dir: PathBuf,
    pub endpoints: Endpoints,
    pub net: NetConfig,
}

impl FillConfig {
    /// The real servers with the default network settings.
    pub fn new(cache_dir: PathBuf, data_dir: PathBuf) -> FillConfig {
        FillConfig {
            cache_dir,
            data_dir,
            endpoints: Endpoints::default(),
            net: NetConfig::default(),
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
/// and HTTP client.
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
    };
    f(&env)
}

struct Job {
    id: u64,
    session: Arc<Session>,
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

fn settings_for(session: &Session, req: &FillRequest) -> Result<FillSettings, IpcError> {
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
    let mut regions = parse_region_order(&req.regions.join(",")).map_err(IpcError::BadRequest)?;
    if regions.is_empty() {
        regions = default_region_order();
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
        ..FillSettings::default()
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

fn spawn_gather(job: Arc<Job>, cfg: Arc<FillConfig>, events: Arc<dyn JobEvents>) {
    std::thread::spawn(move || {
        let planned = guarded(|| {
            with_env(&cfg, |env| {
                let db = SystemDb::builtin();
                let session = &job.session;
                let ids: Vec<u32> = match &job.entry_filter {
                    Some(ids) => ids.clone(),
                    None => (0..session.entries.len() as u32).collect(),
                };
                let (scan, _) = subset_scan(session, &ids);
                let art = resolve_art(&scan, &session.profile, &db, &job.settings.media);
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
    cfg: Arc<FillConfig>,
    events: Arc<dyn JobEvents>,
    plan: Plan,
    resumed: Option<(Vec<usize>, String)>,
) {
    std::thread::spawn(move || {
        let applied = guarded(|| {
            with_env(&cfg, |env| {
                let opts = ApplyOptions {
                    overwrite: job.overwrite,
                    // Only auto items and accepted/picked review items (D20).
                    include_review: false,
                    concurrency: cfg.net.concurrency.clamp(1, 6),
                    resume_job: resumed.as_ref().map(|(_, journal)| journal.clone()),
                    ..ApplyOptions::default()
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
        events.changed(job.view());
    });
}

/// The one fill job of the app.
#[derive(Default)]
pub struct JobManager {
    seq: AtomicU64,
    current: Mutex<Option<Arc<Job>>>,
}

impl JobManager {
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
        let settings = settings_for(&session, req)?;
        let job = {
            let mut slot = lock(&self.current);
            if slot.as_ref().is_some_and(|j| j.running()) {
                return Err(IpcError::JobRunning);
            }
            let job = Arc::new(Job {
                id: self.seq.fetch_add(1, Ordering::SeqCst) + 1,
                session,
                entry_filter: req.entry_ids.clone(),
                settings,
                overwrite: req.overwrite,
                cancel: AtomicBool::new(false),
                data: Mutex::new(JobData::new()),
            });
            *slot = Some(Arc::clone(&job));
            job
        };
        spawn_gather(Arc::clone(&job), cfg, events);
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
            if d.state != JobState::Planned {
                return Err(IpcError::BadRequest(
                    "decisions are taken before the job runs".into(),
                ));
            }
            let n = d.plan.as_ref().map_or(0, |p| p.items.len());
            if let Some(bad) = decisions.iter().find(|x| x.index as usize >= n) {
                return Err(IpcError::BadRequest(format!("no item {}", bad.index)));
            }
            for x in decisions {
                d.decisions.insert(x.index as usize, x.decision);
            }
        }
        Ok(job.view())
    }

    /// Applies the decided plan on a worker thread.
    pub fn run(
        &self,
        cfg: Arc<FillConfig>,
        job: u64,
        events: Arc<dyn JobEvents>,
    ) -> Result<JobView, IpcError> {
        let job = self.job(job)?;
        let plan = {
            let mut d = lock(&job.data);
            if d.state != JobState::Planned {
                return Err(IpcError::BadRequest("the job is not ready to run".into()));
            }
            let plan = d.decided_plan().ok_or(IpcError::NoJob)?;
            d.state = JobState::Applying;
            plan
        };
        spawn_apply(Arc::clone(&job), cfg, events, plan, None);
        Ok(job.view())
    }

    /// Continues a paused job: the dry run again, or the items not done yet
    /// into the same journal.
    pub fn resume(
        &self,
        cfg: Arc<FillConfig>,
        job: u64,
        events: Arc<dyn JobEvents>,
    ) -> Result<JobView, IpcError> {
        let job = self.job(job)?;
        let mut d = lock(&job.data);
        if d.state != JobState::Paused {
            return Err(IpcError::BadRequest("the job is not paused".into()));
        }
        let phase = d.paused.take().map_or(Phase::Apply, |p| p.phase);
        if phase == Phase::Gather {
            d.state = JobState::Gathering;
            drop(d);
            spawn_gather(Arc::clone(&job), cfg, events);
        } else {
            let plan = d.decided_plan().ok_or(IpcError::NoJob)?;
            let report = d.report.as_ref().ok_or(IpcError::NoJob)?;
            let indices = report.resumable_items();
            let (todo, resumed) = match report.job_id.clone() {
                Some(journal) => (plan.subset(&indices), Some((indices, journal))),
                None => (plan, None),
            };
            d.state = JobState::Applying;
            drop(d);
            spawn_apply(Arc::clone(&job), cfg, events, todo, resumed);
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
