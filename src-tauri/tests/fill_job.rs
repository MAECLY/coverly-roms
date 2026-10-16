mod common;

use std::sync::Arc;
use std::time::Duration;

use common::fill::{fill_world, request, wait_for, Recorder, ADVENTURE_USA};
use common::open;
use coverly_core::fill::Endpoints;
use coverly_core::jobs::ItemErrorKind;
use coverly_core::net::testing::{HangingServer, MockResponse};
use coverly_core::net::NetConfig;
use coverly_core::plan::Decision;
use coverly_core::profiles::Media;
use coverly_core::progress::{PauseKind, Phase};
use coverly_core::write::{list_jobs, undo_last};
use coverly_roms_lib::error::IpcError;
use coverly_roms_lib::jobs::view::{
    ActionKind, DecisionIn, ItemFilter, JobProgressEvent, JobState, JobView, OutcomeKind,
};
use coverly_roms_lib::jobs::{FillConfig, JobEvents, JobManager};
use coverly_roms_lib::library::Session;

fn session(w: &common::fill::FillWorld) -> Arc<Session> {
    Arc::new(open(w.lib.path(), None).unwrap().0)
}

fn planned(
    w: &common::fill::FillWorld,
    jobs: &JobManager,
    rec: &Arc<Recorder>,
    ids: Option<Vec<u32>>,
) -> u64 {
    let v = jobs
        .start(w.cfg.clone(), session(w), &request(ids), rec.clone())
        .unwrap();
    assert_eq!(v.state, JobState::Gathering);
    wait_for(rec, JobState::Planned).id
}

fn cover(w: &common::fill::FillWorld, stem: &str) -> std::path::PathBuf {
    w.lib.path().join(format!("ROMS/GBA/Imgs/{stem}.png"))
}

#[test]
fn dry_run_classifies_items_and_writes_nothing() {
    let w = fill_world();
    let (jobs, rec) = (JobManager::default(), Arc::new(Recorder::default()));
    let id = planned(&w, &jobs, &rec, None);
    let v = jobs.current().unwrap();
    let plan = v.plan.unwrap();
    assert_eq!(
        (plan.totals.auto, plan.totals.review, plan.totals.none),
        (3, 1, 0)
    );
    assert_eq!(plan.per_system[0].folder, "GBA");
    assert!(!w.lib.path().join("ROMS/GBA/Imgs").exists());
    assert!(rec
        .progress
        .lock()
        .unwrap()
        .iter()
        .any(|p| p.phase == Phase::Gather && p.job == id));
    let review = jobs.items(id, ItemFilter::Review).unwrap();
    assert_eq!(review.len(), 1);
    assert_eq!(review[0].name, "004 Gldn Rsh");
    assert_eq!(review[0].entry_id, Some(3));
    assert_eq!(review[0].action, ActionKind::Review);
    assert!(!review[0].candidates.is_empty());
    assert_eq!(jobs.items(id, ItemFilter::All).unwrap().len(), 4);
}

#[test]
fn a_selection_only_plans_those_entries() {
    let w = fill_world();
    let (jobs, rec) = (JobManager::default(), Arc::new(Recorder::default()));
    let id = planned(&w, &jobs, &rec, Some(vec![0]));
    let all = jobs.items(id, ItemFilter::All).unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].entry_id, Some(0));
}

#[test]
fn review_decisions_feed_the_apply_and_touched_entries_are_reported() {
    let w = fill_world();
    let (jobs, rec) = (JobManager::default(), Arc::new(Recorder::default()));
    let id = planned(&w, &jobs, &rec, None);
    let review = jobs.items(id, ItemFilter::Review).unwrap();
    let v = jobs
        .decide(
            id,
            &[DecisionIn {
                index: review[0].index,
                decision: Decision::Pick { candidate: 0 },
            }],
        )
        .unwrap();
    assert_eq!(v.decided, 1);
    jobs.run(w.cfg.clone(), id, rec.clone()).unwrap();
    let done = wait_for(&rec, JobState::Done);
    let result = done.result.unwrap();
    assert_eq!((result.filled, result.no_art, result.failed), (3, 1, 0));
    assert!(cover(&w, "004 Gldn Rsh").exists());
    let written = rec.written.lock().unwrap().clone();
    assert_eq!(written.len(), 1);
    let (generation, mut ids) = written[0].clone();
    ids.sort_unstable();
    assert_eq!((generation, ids), (7, vec![0, 1, 3]));
    let failed = jobs.items(id, ItemFilter::Failed).unwrap();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].name, "003 Missing Game");
    let outcome = failed[0].outcome.clone().unwrap();
    assert_eq!(outcome.status, OutcomeKind::Failed);
    assert_eq!(outcome.error, Some(ItemErrorKind::NoArt));
}

#[test]
fn undecided_review_items_are_not_written() {
    let w = fill_world();
    let (jobs, rec) = (JobManager::default(), Arc::new(Recorder::default()));
    let id = planned(&w, &jobs, &rec, None);
    jobs.run(w.cfg.clone(), id, rec.clone()).unwrap();
    assert_eq!(wait_for(&rec, JobState::Done).result.unwrap().filled, 2);
    assert!(!cover(&w, "004 Gldn Rsh").exists());
    let review = jobs.items(id, ItemFilter::Review).unwrap();
    assert_eq!(
        review[0].outcome.as_ref().unwrap().status,
        OutcomeKind::SkippedReview
    );
}

#[test]
fn a_skipped_item_is_not_written() {
    let w = fill_world();
    let (jobs, rec) = (JobManager::default(), Arc::new(Recorder::default()));
    let id = planned(&w, &jobs, &rec, None);
    jobs.decide(
        id,
        &[DecisionIn {
            index: 0,
            decision: Decision::Skip,
        }],
    )
    .unwrap();
    jobs.run(w.cfg.clone(), id, rec.clone()).unwrap();
    wait_for(&rec, JobState::Done);
    assert!(!cover(&w, "001 Golden Sun").exists());
    assert!(cover(&w, "002 Golden Adventure").exists());
}

#[test]
fn resume_after_a_rate_limit_finishes_the_same_journal() {
    let w = fill_world();
    let (jobs, rec) = (JobManager::default(), Arc::new(Recorder::default()));
    let id = planned(&w, &jobs, &rec, None);
    w.server.mock(
        ADVENTURE_USA,
        MockResponse::status(429).header("Retry-After", "0"),
    );
    // One worker: 001 is written, then 002 meets the rate limit.
    let serial = Arc::new(FillConfig {
        net: NetConfig {
            concurrency: 1,
            retries: 0,
            ..NetConfig::default()
        },
        ..(*w.cfg).clone()
    });
    jobs.run(serial.clone(), id, rec.clone()).unwrap();
    let paused = wait_for(&rec, JobState::Paused);
    let why = paused.paused.clone().unwrap();
    assert_eq!((why.phase, why.kind), (Phase::Apply, PauseKind::RateLimit));
    let journal = paused.journal.clone().unwrap();
    w.server.mock(
        ADVENTURE_USA,
        MockResponse::ok(b"<html>oops</html>".to_vec()),
    );
    jobs.resume(serial, id, rec.clone()).unwrap();
    let done = wait_for(&rec, JobState::Done);
    assert_eq!(done.journal, Some(journal));
    assert_eq!(done.result.unwrap().filled, 2);
    assert_eq!(list_jobs(&w.cfg.data_dir).unwrap().len(), 1);
    assert_eq!(undo_last(&w.cfg.data_dir).unwrap().removed.len(), 2);
}

#[test]
fn a_second_job_is_refused_while_one_runs() {
    let w = fill_world();
    let (jobs, rec) = (JobManager::default(), Arc::new(Recorder::default()));
    let hang = HangingServer::start();
    let slow = Arc::new(FillConfig {
        endpoints: Endpoints {
            libretro: hang.base(),
            libretro_raw: hang.base(),
            database: hang.base(),
        },
        net: NetConfig {
            retries: 0,
            timeout: Duration::from_millis(500),
            ..NetConfig::default()
        },
        ..(*w.cfg).clone()
    });
    let s = session(&w);
    let first = jobs
        .start(slow.clone(), s.clone(), &request(None), rec.clone())
        .unwrap();
    assert_eq!(
        jobs.start(slow, s.clone(), &request(None), rec.clone()),
        Err(IpcError::JobRunning)
    );
    assert!(jobs.busy());
    jobs.cancel(first.id).unwrap();
    assert_eq!(wait_for(&rec, JobState::Cancelled).id, first.id);
    assert!(!jobs.busy());
    // A stopped job is replaced; its id is then stale.
    jobs.start(w.cfg.clone(), s, &request(None), rec.clone())
        .unwrap();
    assert_eq!(jobs.cancel(first.id), Err(IpcError::StaleJob));
    wait_for(&rec, JobState::Planned);
}

#[test]
fn run_twice_is_refused_and_decisions_need_a_planned_job() {
    let w = fill_world();
    let (jobs, rec) = (JobManager::default(), Arc::new(Recorder::default()));
    let id = planned(&w, &jobs, &rec, None);
    jobs.run(w.cfg.clone(), id, rec.clone()).unwrap();
    assert!(matches!(
        jobs.run(w.cfg.clone(), id, rec.clone()),
        Err(IpcError::BadRequest(_))
    ));
    assert!(matches!(
        jobs.decide(
            id,
            &[DecisionIn {
                index: 0,
                decision: Decision::Accept
            }]
        ),
        Err(IpcError::BadRequest(_))
    ));
    wait_for(&rec, JobState::Done);
}

#[test]
fn bad_requests_are_refused_before_any_work() {
    let w = fill_world();
    let (jobs, rec) = (JobManager::default(), Arc::new(Recorder::default()));
    let mut screenshots = request(None);
    screenshots.media = vec![Media::Screenshot];
    assert!(matches!(
        jobs.start(w.cfg.clone(), session(&w), &screenshots, rec.clone()),
        Err(IpcError::BadRequest(_))
    ));
    let mut mars = request(None);
    mars.regions = vec!["mars".into()];
    assert!(matches!(
        jobs.start(w.cfg.clone(), session(&w), &mars, rec.clone()),
        Err(IpcError::BadRequest(_))
    ));
    assert_eq!(jobs.current(), None);
    let id = planned(&w, &jobs, &rec, None);
    assert!(matches!(
        jobs.decide(
            id,
            &[DecisionIn {
                index: 99,
                decision: Decision::Accept
            }]
        ),
        Err(IpcError::BadRequest(_))
    ));
}

#[test]
fn candidate_images_are_png_previews() {
    let w = fill_world();
    let (jobs, rec) = (JobManager::default(), Arc::new(Recorder::default()));
    let id = planned(&w, &jobs, &rec, None);
    let bytes = jobs.candidate_image(&w.cfg, id, 0, 0).unwrap();
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    assert!(matches!(
        jobs.candidate_image(&w.cfg, id, 0, 9),
        Err(IpcError::BadRequest(_))
    ));
}

/// Events whose progress hook panics, as a bug in a worker would.
struct Panicking(Recorder);

impl JobEvents for Panicking {
    fn progress(&self, _: JobProgressEvent) {
        panic!("boom");
    }
    fn changed(&self, view: JobView) {
        self.0.changed(view);
    }
    fn written(&self, generation: u64, ids: Vec<u32>) {
        self.0.written(generation, ids);
    }
}

#[test]
fn a_panicking_worker_fails_the_job_instead_of_leaving_it_busy() {
    let w = fill_world();
    let jobs = JobManager::default();
    let ev = Arc::new(Panicking(Recorder::default()));
    jobs.start(w.cfg.clone(), session(&w), &request(None), ev.clone())
        .unwrap();
    let failed = wait_for(&ev.0, JobState::Failed);
    assert!(failed.error.is_some());
    assert!(!jobs.busy());
    // The manager still works: a new job plans normally.
    let rec = Arc::new(Recorder::default());
    planned(&w, &jobs, &rec, None);
}

#[test]
fn cancelling_a_planned_job_ends_it() {
    let w = fill_world();
    let (jobs, rec) = (JobManager::default(), Arc::new(Recorder::default()));
    let id = planned(&w, &jobs, &rec, None);
    assert_eq!(jobs.cancel(id).unwrap().state, JobState::Cancelled);
    assert!(matches!(
        jobs.run(w.cfg.clone(), id, rec.clone()),
        Err(IpcError::BadRequest(_))
    ));
}
