//! Duplicates from the app backend (plan Task 28): the plan store, picks
//! remembered by content, regrouping without reading again, throttled
//! progress, the Trash availability, the apply behind the job guard and
//! its undo through `organize_undo`. No Tauri runtime.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::write;
use coverly_core::dedupe::testing::{pin_mtime, CountingBytes};
use coverly_core::dedupe::{DedupePhase, DedupeProgress, GroupAction, KeepReason};
use coverly_core::identify::sniff::fixtures;
use coverly_core::settings::DedupeLevel;
use coverly_core::write::trash::FakeTrash;
use coverly_core::write::JobKind;
use coverly_roms_lib::dedupe::view::{
    DedupeEventPhase, DedupeJobState, DedupeJobView, DedupePlanView, DedupeProgressEvent,
    DedupeRequest,
};
use coverly_roms_lib::dedupe::{DedupeEvents, Throttle, KEPT_PLANS};
use coverly_roms_lib::error::IpcError;
use coverly_roms_lib::jobs::FillConfig;
use coverly_roms_lib::state::AppState;
use tempfile::TempDir;

fn state(config: &TempDir) -> AppState {
    AppState::new(
        config.path().to_path_buf(),
        FillConfig::new(config.path().join("cache"), config.path().join("data")),
    )
    .with_trash(Arc::new(FakeTrash::new(config.path().join("trash"))))
}

/// An Anbernic-stock card: `Alpha (USA).gba` stored twice (the copy named
/// `(1)`), `Beta (USA).gba` once, with pinned mtimes.
fn card() -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join("anbernic")).unwrap();
    let files = [
        ("ROMS/GBA/Alpha (USA).gba", b'A'),
        ("ROMS/GBA/Alpha (USA) (1).gba", b'A'),
        ("ROMS/GBA/Beta (USA).gba", b'B'),
    ];
    for (n, (rel, tag)) in files.into_iter().enumerate() {
        write(dir.path(), rel, &fixtures::gba_tagged(tag));
        pin_mtime(&dir.path().join(rel), n as u64 + 1);
    }
    dir
}

fn request() -> DedupeRequest {
    DedupeRequest {
        profile: Some("anbernic-stock".into()),
        offline: Some(true),
        ..DedupeRequest::default()
    }
}

#[derive(Default)]
struct Recorder {
    events: Mutex<Vec<DedupeProgressEvent>>,
    done: Mutex<Option<Sender<DedupeJobView>>>,
}

impl Recorder {
    fn events(&self) -> Vec<DedupeProgressEvent> {
        self.events.lock().unwrap().clone()
    }

    /// A recorder whose `changed` views arrive on the receiver.
    fn waiting() -> (Arc<Recorder>, Receiver<DedupeJobView>) {
        let (tx, rx) = channel();
        let r = Recorder {
            done: Mutex::new(Some(tx)),
            ..Recorder::default()
        };
        (Arc::new(r), rx)
    }
}

impl DedupeEvents for Recorder {
    fn progress(&self, event: DedupeProgressEvent) {
        self.events.lock().unwrap().push(event);
    }

    fn changed(&self, view: DedupeJobView) {
        if let Some(tx) = self.done.lock().unwrap().as_ref() {
            let _ = tx.send(view);
        }
    }
}

fn plan(st: &AppState, root: &Path) -> DedupePlanView {
    st.dedupe_plan(root, &request(), Arc::new(Recorder::default()))
        .unwrap()
}

/// The group of the two Alpha copies.
fn alpha(view: &DedupePlanView) -> &coverly_core::dedupe::DupGroup {
    assert_eq!(view.plan.groups.len(), 1, "{:#?}", view.plan.groups);
    &view.plan.groups[0]
}

fn primary(view: &DedupePlanView, member: usize) -> PathBuf {
    alpha(view).members[member].unit.primary.clone()
}

#[test]
fn plan_store_keeps_four_plans() {
    let config = TempDir::new().unwrap();
    let st = state(&config);
    let lib = card();

    let events = Arc::new(Recorder::default());
    let first = st
        .dedupe_plan(lib.path(), &request(), events.clone())
        .unwrap();
    let got = events.events();
    assert!(!got.is_empty());
    assert!(got.iter().all(|e| e.plan_id == first.plan_id));
    assert_eq!(got[0].phase, DedupeEventPhase::Plan(DedupePhase::Listing));
    let g = alpha(&first);
    assert_eq!(g.system, "gba");
    assert_eq!(g.action, GroupAction::Act);
    assert_eq!(first.plan.totals.extras, 1);
    // A dry run: nothing moved.
    assert!(lib.path().join("ROMS/GBA/Alpha (USA) (1).gba").is_file());
    assert!(!lib.path().join("_Duplicados").exists());

    // The wire shape: the plan's fields next to `plan_id`.
    let json = serde_json::to_value(&first).unwrap();
    assert_eq!(json["plan_id"], first.plan_id);
    assert_eq!(json["groups"][0]["system"], "gba");
    assert!(json.get("plan").is_none());

    let mut ids = vec![first.plan_id];
    for _ in 0..KEPT_PLANS {
        ids.push(plan(&st, lib.path()).plan_id);
    }
    assert_eq!(
        st.dedupe().get(first.plan_id).unwrap_err(),
        IpcError::NoDedupePlan
    );
    for id in &ids[1..] {
        assert!(st.dedupe().get(*id).is_ok(), "plan {id} is kept");
    }
    assert_eq!(
        st.dedupe_set_keeper(first.plan_id, g.id, 1).unwrap_err(),
        IpcError::NoDedupePlan
    );
    let dest = config.path().join("plan.json");
    st.dedupe_export_plan(ids[4], &dest).unwrap();
    let saved: serde_json::Value = serde_json::from_slice(&fs::read(&dest).unwrap()).unwrap();
    assert_eq!(saved["groups"].as_array().unwrap().len(), 1);

    // Bad requests keep their codes.
    assert!(matches!(
        st.dedupe_plan(
            &lib.path().join("missing"),
            &request(),
            Arc::new(Recorder::default())
        ),
        Err(IpcError::NotFound(_))
    ));
    let unknown = DedupeRequest {
        profile: Some("no-such-layout".into()),
        ..request()
    };
    assert_eq!(
        st.dedupe_plan(lib.path(), &unknown, Arc::new(Recorder::default()))
            .unwrap_err(),
        IpcError::UnknownProfile("no-such-layout".into())
    );
    let inside = DedupeRequest {
        quarantine: Some(coverly_core::dedupe::QuarantineChoice::Folder {
            path: lib.path().join("ROMS/GBA/dups"),
        }),
        ..request()
    };
    assert!(matches!(
        st.dedupe_plan(lib.path(), &inside, Arc::new(Recorder::default())),
        Err(IpcError::QuarantineInSystemFolder(_))
    ));
}

#[test]
fn apply_is_guarded_by_the_job_manager() {
    let config = TempDir::new().unwrap();
    let st = state(&config);
    let lib = card();
    let view = plan(&st, lib.path());

    // An organise apply, a fill or a manual write holds this guard.
    let writing = st.jobs().begin_manual().unwrap();
    assert_eq!(
        st.dedupe_apply(view.plan_id, Arc::new(Recorder::default()))
            .unwrap_err(),
        IpcError::JobRunning
    );
    drop(writing);
    assert!(lib.path().join("ROMS/GBA/Alpha (USA) (1).gba").is_file());
    assert_eq!(
        st.dedupe_apply(999, Arc::new(Recorder::default()))
            .unwrap_err(),
        IpcError::NoDedupePlan
    );

    // While the apply runs, nothing else writes.
    let (events, done) = Recorder::waiting();
    let started = st.dedupe_apply(view.plan_id, events).unwrap();
    assert_eq!(started.state, DedupeJobState::Applying);
    let busy = st.jobs().begin_manual().map(|_| ());
    let ended = done.recv_timeout(Duration::from_secs(30)).unwrap();
    assert!(
        busy.is_ok() || busy == Err(IpcError::JobRunning),
        "{busy:?}"
    );
    assert_eq!(ended.state, DedupeJobState::Done, "{ended:?}");
    assert!(st.jobs().begin_manual().is_ok(), "the guard is released");
    assert_eq!(st.dedupe_job().unwrap().state, DedupeJobState::Done);
}

#[test]
fn set_keeper_persists_choice() {
    let config = TempDir::new().unwrap();
    let st = state(&config);
    let lib = card();
    let view = plan(&st, lib.path());
    let g = alpha(&view);
    let other = 1 - g.keeper;
    let chosen = primary(&view, other);

    let picked = st.dedupe_set_keeper(view.plan_id, g.id, other).unwrap();
    assert_eq!(picked.plan_id, view.plan_id);
    assert_eq!(alpha(&picked).keeper, other);
    assert_eq!(alpha(&picked).reason, KeepReason::UserChoice);
    // The kept plan has it too (what apply and export use).
    let again = st.dedupe_skip_group(view.plan_id, g.id, false).unwrap();
    assert_eq!(alpha(&again).keeper, other);
    assert!(matches!(
        st.dedupe_set_keeper(view.plan_id, g.id, 9),
        Err(IpcError::BadRequest(_))
    ));

    // A new plan of the library remembers the pick by content.
    let next = plan(&st, lib.path());
    let g = alpha(&next);
    assert_eq!(g.members[g.keeper].unit.primary, chosen);
    assert_eq!(g.reason, KeepReason::UserChoice);

    // "Apply the rules again" forgets it.
    assert!(st.dedupe_reset_choices(lib.path(), None).unwrap() >= 1);
    let reset = plan(&st, lib.path());
    let g = alpha(&reset);
    assert_ne!(g.members[g.keeper].unit.primary, chosen);
    assert_ne!(g.reason, KeepReason::UserChoice);
}

#[test]
fn skipping_a_group_or_a_system_leaves_it_alone() {
    let config = TempDir::new().unwrap();
    let st = state(&config);
    let lib = card();
    let view = plan(&st, lib.path());
    let id = alpha(&view).id;

    let skipped = st.dedupe_skip_system(view.plan_id, "gba", true).unwrap();
    assert!(matches!(
        alpha(&skipped).action,
        GroupAction::ReportOnly { .. }
    ));
    assert_eq!(skipped.plan.totals.extras, 0);
    // Taking the group back keeps it skipped while its system is.
    let still = st.dedupe_skip_group(view.plan_id, id, false).unwrap();
    assert!(matches!(
        alpha(&still).action,
        GroupAction::ReportOnly { .. }
    ));
    let back = st.dedupe_skip_system(view.plan_id, "gba", false).unwrap();
    assert_eq!(alpha(&back).action, GroupAction::Act);
    assert_eq!(back.plan.totals.extras, 1);
    assert!(matches!(
        st.dedupe_skip_group(view.plan_id, 12345, true),
        Err(IpcError::BadRequest(_))
    ));

    // A skipped group is never touched by apply.
    st.dedupe_skip_group(view.plan_id, id, true).unwrap();
    let (events, done) = Recorder::waiting();
    st.dedupe_apply(view.plan_id, events).unwrap();
    let ended = done.recv_timeout(Duration::from_secs(30)).unwrap();
    assert_eq!(ended.state, DedupeJobState::Done, "{ended:?}");
    assert!(lib.path().join("ROMS/GBA/Alpha (USA) (1).gba").is_file());
    assert!(lib.path().join("ROMS/GBA/Alpha (USA).gba").is_file());
}

#[test]
fn regroup_does_not_rehash() {
    let config = TempDir::new().unwrap();
    let bytes = Arc::new(CountingBytes::new());
    let st = state(&config).with_file_bytes(bytes.clone());
    let lib = card();
    let view = plan(&st, lib.path());
    let opened = bytes.opens();
    assert!(opened > 0, "the first plan reads the copies");

    let exact = DedupeRequest {
        levels: Some(vec![DedupeLevel::Exact]),
        one_game_one_rom: Some(true),
        ..DedupeRequest::default()
    };
    let regrouped = st.dedupe_regroup(view.plan_id, &exact).unwrap();
    assert_eq!(regrouped.plan_id, view.plan_id);
    assert_eq!(alpha(&regrouped).id, alpha(&view).id);
    let to_trash = DedupeRequest {
        quarantine: Some(coverly_core::dedupe::QuarantineChoice::Trash),
        ..DedupeRequest::default()
    };
    let trashed = st.dedupe_regroup(view.plan_id, &to_trash).unwrap();
    assert!(trashed.plan.trash);
    assert!(alpha(&trashed).members.iter().all(|m| m.dest.is_none()));
    assert_eq!(bytes.opens(), opened, "no file was opened again");
}

#[test]
fn progress_is_throttled() {
    let sink = Arc::new(Recorder::default());
    let t = Throttle::new(7, sink.clone());
    let p = |phase, done, total| DedupeProgress {
        phase,
        done,
        total,
        current: None,
    };
    t.progress(&p(DedupePhase::Listing, 0, 1));
    for done in 0..=500 {
        t.progress(&p(DedupePhase::Fingerprints, done, 500));
    }
    let got = sink.events();
    assert!(got.iter().all(|e| e.plan_id == 7));
    assert_eq!(got[0].phase, DedupeEventPhase::Plan(DedupePhase::Listing));
    assert_eq!(
        got[1].phase,
        DedupeEventPhase::Plan(DedupePhase::Fingerprints)
    );
    assert_eq!(got.last().unwrap().done, 500, "the end always goes out");
    assert!(got.len() < 10, "throttled: {} events", got.len());
}

#[test]
fn trash_available_is_reported() {
    let config = TempDir::new().unwrap();
    let lib = card();
    let st = state(&config);
    let view = plan(&st, lib.path());
    assert!(view.trash_available);
    assert_eq!(
        serde_json::to_value(&view).unwrap()["trash_available"],
        true
    );

    let st = AppState::new(
        config.path().to_path_buf(),
        FillConfig::new(config.path().join("cache"), config.path().join("data")),
    )
    .with_trash(Arc::new(FakeTrash::unavailable(
        config.path().join("trash"),
    )));
    let view = plan(&st, lib.path());
    assert!(!view.trash_available);
}

#[test]
fn undo_of_a_dedupe_job_goes_through_organize_undo() {
    let config = TempDir::new().unwrap();
    let st = state(&config);
    let lib = card();
    let view = plan(&st, lib.path());
    let g = alpha(&view);
    let extra = g.members[1 - g.keeper].clone();
    let dest = extra.dest.clone().expect("the extra has a destination");
    assert!(dest.starts_with(lib.path().join("_Duplicados")), "{dest:?}");

    let (events, done) = Recorder::waiting();
    st.dedupe_apply(view.plan_id, events.clone()).unwrap();
    let ended = done.recv_timeout(Duration::from_secs(30)).unwrap();
    assert_eq!(ended.state, DedupeJobState::Done, "{ended:?}");
    assert!(!ended.trash);
    let job = ended.job.clone().expect("a journal");
    assert!(!extra.unit.primary.exists());
    assert!(dest.is_file());
    assert!(events
        .events()
        .iter()
        .any(|e| e.phase == DedupeEventPhase::Apply));
    // Applying dropped the kept plans: the library changed.
    assert_eq!(
        st.dedupe().get(view.plan_id).unwrap_err(),
        IpcError::NoDedupePlan
    );
    let history = st.job_history().unwrap();
    assert_eq!(history[0].job, job);
    assert_eq!(history[0].kind, JobKind::Dedupe);
    // The file manager may show the copy where it went.
    assert_eq!(st.revealable(&dest).unwrap(), dest);
    assert!(matches!(
        st.revealable(Path::new("/etc/passwd")),
        Err(IpcError::OutsideLibrary(_))
    ));

    let undo = st.organize_undo(&job, false).unwrap();
    assert_eq!(undo.report.len(), 1);
    assert!(extra.unit.primary.is_file(), "the copy is back");
    assert!(!dest.exists());
    assert_eq!(
        st.dedupe_resume(&job, Arc::new(Recorder::default()))
            .unwrap_err(),
        IpcError::BadRequest(format!("job {job} has nothing left to resume"))
    );
}

#[test]
fn undo_of_a_trashed_dedupe_job_brings_the_copy_back() {
    let config = TempDir::new().unwrap();
    let st = state(&config);
    let lib = card();
    let req = DedupeRequest {
        quarantine: Some(coverly_core::dedupe::QuarantineChoice::Trash),
        ..request()
    };
    let view = st
        .dedupe_plan(lib.path(), &req, Arc::new(Recorder::default()))
        .unwrap();
    let g = alpha(&view);
    let extra = g.members[1 - g.keeper].unit.primary.clone();

    let (events, done) = Recorder::waiting();
    st.dedupe_apply(view.plan_id, events).unwrap();
    let ended = done.recv_timeout(Duration::from_secs(30)).unwrap();
    assert_eq!(ended.state, DedupeJobState::Done, "{ended:?}");
    assert!(ended.trash);
    assert!(!extra.exists());
    st.organize_undo(ended.job.as_deref().unwrap(), false)
        .unwrap();
    assert!(extra.is_file(), "back from the Trash");
}

/// A Trash that holds its first file until the test lets it go, so an
/// apply can be caught while it runs.
struct GatedTrash {
    inner: FakeTrash,
    entered: Mutex<Option<Sender<()>>>,
    release: Mutex<Option<Receiver<()>>>,
}

impl GatedTrash {
    fn new(dir: PathBuf) -> (Arc<GatedTrash>, Receiver<()>, Sender<()>) {
        let (entered_tx, entered_rx) = channel();
        let (release_tx, release_rx) = channel();
        let t = GatedTrash {
            inner: FakeTrash::new(dir),
            entered: Mutex::new(Some(entered_tx)),
            release: Mutex::new(Some(release_rx)),
        };
        (Arc::new(t), entered_rx, release_tx)
    }
}

impl coverly_core::write::trash::TrashBackend for GatedTrash {
    fn trash(
        &self,
        path: &Path,
    ) -> Result<Option<PathBuf>, coverly_core::write::trash::TrashError> {
        if let Some(tx) = self.entered.lock().unwrap().take() {
            let _ = tx.send(());
            if let Some(rx) = self.release.lock().unwrap().take() {
                let _ = rx.recv_timeout(Duration::from_secs(30));
            }
        }
        self.inner.trash(path)
    }

    fn find(
        &self,
        volume_of: &Path,
        name: &str,
        fp: &coverly_core::write::Fingerprint,
    ) -> Option<PathBuf> {
        self.inner.find(volume_of, name, fp)
    }

    fn available(&self, path: &Path) -> bool {
        self.inner.available(path)
    }
}

/// Two games stored twice each, pinned mtimes.
fn two_pairs() -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join("anbernic")).unwrap();
    let files = [
        ("ROMS/GBA/Alpha (USA).gba", b'A'),
        ("ROMS/GBA/Alpha (USA) (1).gba", b'A'),
        ("ROMS/GBA/Beta (USA).gba", b'B'),
        ("ROMS/GBA/Beta (USA) (1).gba", b'B'),
    ];
    for (n, (rel, tag)) in files.into_iter().enumerate() {
        write(dir.path(), rel, &fixtures::gba_tagged(tag));
        pin_mtime(&dir.path().join(rel), n as u64 + 1);
    }
    dir
}

/// Every file under `root` (but the quarantine) with its bytes.
fn snapshot(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn walk(dir: &Path, root: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
        for e in fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk(&p, root, out);
            } else {
                out.push((
                    p.strip_prefix(root).unwrap().to_path_buf(),
                    fs::read(&p).unwrap(),
                ));
            }
        }
    }
    let mut out = Vec::new();
    walk(&root.join("ROMS"), root, &mut out);
    out.sort();
    out
}

#[test]
fn while_an_apply_runs_nothing_else_writes_and_a_cancel_resumes_to_the_end() {
    let config = TempDir::new().unwrap();
    let (trash, entered, release) = GatedTrash::new(config.path().join("trash"));
    let st = AppState::new(
        config.path().to_path_buf(),
        FillConfig::new(config.path().join("cache"), config.path().join("data")),
    )
    .with_trash(trash);
    let lib = two_pairs();
    let before = snapshot(lib.path());
    let req = DedupeRequest {
        quarantine: Some(coverly_core::dedupe::QuarantineChoice::Trash),
        ..request()
    };
    let view = st
        .dedupe_plan(lib.path(), &req, Arc::new(Recorder::default()))
        .unwrap();
    assert_eq!(view.plan.groups.len(), 2, "{:#?}", view.plan.groups);
    let keepers: Vec<PathBuf> = view
        .plan
        .groups
        .iter()
        .map(|g| g.members[g.keeper].unit.primary.clone())
        .collect();
    let other = st
        .dedupe_plan(lib.path(), &req, Arc::new(Recorder::default()))
        .unwrap();

    let (events, done) = Recorder::waiting();
    st.dedupe_apply(view.plan_id, events).unwrap();
    entered.recv_timeout(Duration::from_secs(30)).unwrap();
    // Caught mid-apply: every other write is refused.
    assert_eq!(st.dedupe_job().unwrap().state, DedupeJobState::Applying);
    assert_eq!(
        st.jobs().begin_manual().map(|_| ()),
        Err(IpcError::JobRunning)
    );
    assert_eq!(
        st.dedupe_apply(other.plan_id, Arc::new(Recorder::default()))
            .map(|_| ()),
        Err(IpcError::NoDedupePlan),
        "applying dropped the other kept plans"
    );
    let again = st
        .dedupe_plan(lib.path(), &req, Arc::new(Recorder::default()))
        .unwrap();
    assert_eq!(
        st.dedupe_apply(again.plan_id, Arc::new(Recorder::default()))
            .map(|_| ()),
        Err(IpcError::JobRunning)
    );
    st.dedupe_cancel_apply().unwrap();
    release.send(()).unwrap();

    let ended = done.recv_timeout(Duration::from_secs(30)).unwrap();
    assert_eq!(ended.state, DedupeJobState::Cancelled, "{ended:?}");
    let job = ended.job.clone().expect("a journal to resume");
    for k in &keepers {
        assert!(k.is_file(), "the copy that stays never moves: {k:?}");
    }
    let left = snapshot(lib.path()).len();
    assert_eq!(left, 3, "one extra set aside before the cancel");

    // Resume sets the rest aside; a second resume is refused, nothing moves.
    let (events, done) = Recorder::waiting();
    st.dedupe_resume(&job, events).unwrap();
    let resumed = done.recv_timeout(Duration::from_secs(30)).unwrap();
    assert_eq!(resumed.state, DedupeJobState::Done, "{resumed:?}");
    assert_eq!(resumed.job.as_deref(), Some(job.as_str()));
    let after = snapshot(lib.path());
    assert_eq!(after.len(), 2);
    for k in &keepers {
        assert!(k.is_file(), "{k:?}");
    }
    assert!(matches!(
        st.dedupe_resume(&job, Arc::new(Recorder::default())),
        Err(IpcError::BadRequest(_))
    ));
    assert_eq!(snapshot(lib.path()), after);
    assert_eq!(
        st.dedupe_resume("20990101T000000Z-0000-0000", Arc::new(Recorder::default()))
            .map(|_| ()),
        Err(IpcError::NoDedupePlan)
    );

    // Undo brings every copy back byte for byte, under its own name.
    st.organize_undo(&job, false).unwrap();
    assert_eq!(snapshot(lib.path()), before);
}

#[test]
fn undo_of_a_quarantined_job_restores_the_library_byte_for_byte() {
    let config = TempDir::new().unwrap();
    let st = state(&config);
    let lib = two_pairs();
    let before = snapshot(lib.path());
    let view = plan(&st, lib.path());
    let (events, done) = Recorder::waiting();
    st.dedupe_apply(view.plan_id, events).unwrap();
    let ended = done.recv_timeout(Duration::from_secs(30)).unwrap();
    assert_eq!(ended.state, DedupeJobState::Done, "{ended:?}");
    assert_eq!(snapshot(lib.path()).len(), 2);
    let job = ended.job.unwrap();
    // A finished job has nothing to resume.
    assert!(matches!(
        st.dedupe_resume(&job, Arc::new(Recorder::default())),
        Err(IpcError::BadRequest(_))
    ));
    st.organize_undo(&job, false).unwrap();
    assert_eq!(snapshot(lib.path()), before);
}
