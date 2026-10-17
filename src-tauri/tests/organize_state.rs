//! Organising a loose pile from the app backend: the probe, the plan store
//! (re-plans reuse the survey), cancelling, and the weak `system-folders`
//! detection when opening an organised folder. No Tauri runtime.

mod common;

use std::fs::{self, File};
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::{Duration, SystemTime};

use common::write;
use coverly_core::identify::sniff::fixtures;
use coverly_core::organize::{Choice, Decision, OrganizeAction, OrganizePhase};
use coverly_core::systems::SystemDb;
use coverly_roms_lib::error::IpcError;
use coverly_roms_lib::jobs::FillConfig;
use coverly_roms_lib::organize::view::{OrganizeProgressEvent, OrganizeRequest};
use coverly_roms_lib::organize::OrganizeEvents;
use coverly_roms_lib::state::AppState;
use tempfile::TempDir;

fn state(config: &TempDir) -> AppState {
    AppState::new(
        config.path().to_path_buf(),
        FillConfig::new(config.path().join("cache"), config.path().join("data")),
    )
}

/// Writes `bytes` to `root/rel`, dated an hour ago (never "still being
/// written").
fn write_old(root: &Path, rel: &str, bytes: &[u8]) {
    write(root, rel, bytes);
    let hour_ago = SystemTime::now() - Duration::from_secs(3600);
    File::options()
        .write(true)
        .open(root.join(rel))
        .unwrap()
        .set_modified(hour_ago)
        .unwrap();
}

/// A pile of three different Game Boy Advance ROMs (synthetic headers).
fn gba_pile() -> TempDir {
    let dir = TempDir::new().unwrap();
    let names = ["Alpha (USA).gba", "Beta (USA).gba", "Gamma (USA).gba"];
    for (tag, name) in b"ABG".iter().copied().zip(names) {
        write_old(dir.path(), name, &fixtures::gba_tagged(tag));
    }
    dir
}

fn offline(source: &Path) -> OrganizeRequest {
    OrganizeRequest {
        source: source.display().to_string(),
        offline: Some(true),
        ..OrganizeRequest::default()
    }
}

#[derive(Default)]
struct Recorder {
    events: Mutex<Vec<OrganizeProgressEvent>>,
    /// When set, the first event cancels its plan through this state.
    cancel_with: OnceLock<Weak<AppState>>,
}

impl Recorder {
    fn events(&self) -> Vec<OrganizeProgressEvent> {
        self.events.lock().unwrap().clone()
    }
}

impl OrganizeEvents for Recorder {
    fn progress(&self, event: OrganizeProgressEvent) {
        if let Some(st) = self.cancel_with.get().and_then(Weak::upgrade) {
            st.organize_cancel(event.plan_id).unwrap();
        }
        self.events.lock().unwrap().push(event);
    }
}

#[test]
fn probe_finds_rom_like_files_in_a_plain_folder() {
    let config = TempDir::new().unwrap();
    let st = state(&config);
    let pile = TempDir::new().unwrap();
    write(pile.path(), "Tetris (JUE) (V1.0) [!].zip", b"zip");
    write(pile.path(), "Tetris Worlds (USA).gba", b"gba");
    write(pile.path(), "Tetris 2.zip.crdownload", b"part");
    write(pile.path(), ".DS_Store", b"junk");

    let probe = st.probe_folder(pile.path()).unwrap();
    assert_eq!(probe.rom_like, 2);
    assert_eq!(probe.partial_downloads, 1);
    assert_eq!(probe.cloud_only, 0);
    assert!(probe.sorted_folders.is_empty());
    assert!(probe.writable);
    assert!(!probe.broad_root);
    assert!(!probe.truncated);
    let json = serde_json::to_value(&probe).unwrap();
    assert_eq!(json["rom_like"], 2);
    assert_eq!(json["sorted_folders"], serde_json::json!([]));

    assert!(matches!(
        st.probe_folder(&pile.path().join("missing")),
        Err(IpcError::NotFound(_))
    ));
    assert!(matches!(
        st.probe_folder(&pile.path().join("Tetris Worlds (USA).gba")),
        Err(IpcError::NotAFolder(_))
    ));
}

#[test]
fn a_plan_is_stored_and_replanning_with_decisions_reuses_the_survey() {
    let config = TempDir::new().unwrap();
    let st = state(&config);
    let pile = gba_pile();
    let req = offline(pile.path());

    let first_events = Arc::new(Recorder::default());
    let first = st.organize_plan(&req, &[], first_events.clone()).unwrap();
    let events = first_events.events();
    assert!(!events.is_empty());
    assert!(events.iter().all(|e| e.plan_id == first.plan_id));
    assert_eq!(events[0].phase, OrganizePhase::Walk);
    assert_eq!(first.plan.layout, "anbernic-stock");
    assert_eq!(first.plan.items.len(), 3);
    assert!(first
        .plan
        .items
        .iter()
        .all(|i| i.action == OrganizeAction::Auto && i.system_id.as_deref() == Some("gba")));
    assert_eq!(first.plan.counts.roms, 3);

    let second_events = Arc::new(Recorder::default());
    let leave = [Decision {
        item: 0,
        choice: Choice::Leave,
    }];
    let second = st
        .organize_plan(&req, &leave, second_events.clone())
        .unwrap();
    assert!(second_events.events().is_empty(), "the survey was reused");
    assert_ne!(second.plan_id, first.plan_id);
    assert!(second.plan.items[0].is_left());
    assert_eq!(second.plan.counts.roms, 2);

    // A dry run: nothing moved.
    assert!(pile.path().join("Alpha (USA).gba").is_file());
    assert!(!pile.path().join("ROMS").exists());

    // The wire shape: `plan_id` next to the plan as is.
    let json = serde_json::to_value(&second).unwrap();
    assert_eq!(json["plan_id"], second.plan_id);
    assert_eq!(json["plan"]["layout"], "anbernic-stock");

    // Both plans stay available (export) until newer ones push them out.
    let dest = config.path().join("plan.json");
    st.organize_export_plan(first.plan_id, &dest).unwrap();
    let saved: serde_json::Value = serde_json::from_slice(&fs::read(&dest).unwrap()).unwrap();
    assert_eq!(saved["layout"], "anbernic-stock");
    assert_eq!(saved["items"].as_array().unwrap().len(), 3);
    assert_eq!(st.organize_export_plan(999, &dest), Err(IpcError::NoPlan));

    // A different layout surveys again.
    let other = OrganizeRequest {
        layout: Some("batocera".into()),
        ..offline(pile.path())
    };
    let third_events = Arc::new(Recorder::default());
    let third = st.organize_plan(&other, &[], third_events.clone()).unwrap();
    assert!(!third_events.events().is_empty());
    assert_eq!(third.plan.layout, "batocera");

    // Bad requests keep their codes.
    let unknown = OrganizeRequest {
        layout: Some("no-such-layout".into()),
        ..offline(pile.path())
    };
    assert_eq!(
        st.organize_plan(&unknown, &[], Arc::new(Recorder::default())),
        Err(IpcError::UnknownProfile("no-such-layout".into()))
    );
    let bad_item = [Decision {
        item: 99,
        choice: Choice::Leave,
    }];
    assert!(matches!(
        st.organize_plan(&req, &bad_item, Arc::new(Recorder::default())),
        Err(IpcError::BadRequest(_))
    ));
    assert!(matches!(
        st.organize_plan(
            &offline(&pile.path().join("missing")),
            &[],
            Arc::new(Recorder::default())
        ),
        Err(IpcError::NotFound(_))
    ));
}

#[test]
fn keeps_only_the_last_four_plans() {
    let config = TempDir::new().unwrap();
    let st = state(&config);
    let pile = gba_pile();
    let req = offline(pile.path());
    let ids: Vec<u64> = (0..5)
        .map(|_| {
            st.organize_plan(&req, &[], Arc::new(Recorder::default()))
                .unwrap()
                .plan_id
        })
        .collect();
    let dest = config.path().join("plan.json");
    assert_eq!(
        st.organize_export_plan(ids[0], &dest),
        Err(IpcError::NoPlan)
    );
    for id in &ids[1..] {
        st.organize_export_plan(*id, &dest).unwrap();
    }
}

#[test]
fn cancelling_a_plan_in_progress_returns_cancelled() {
    let config = TempDir::new().unwrap();
    let st = Arc::new(state(&config));
    let pile = gba_pile();
    let events = Arc::new(Recorder::default());
    events.cancel_with.set(Arc::downgrade(&st)).unwrap();

    let out = st.organize_plan(&offline(pile.path()), &[], events.clone());
    assert_eq!(out, Err(IpcError::Cancelled));
    let plan_id = events.events()[0].plan_id;
    // Nothing was stored, and cancelling again (or an unknown plan) is harmless.
    assert_eq!(
        st.organize_export_plan(plan_id, &config.path().join("plan.json")),
        Err(IpcError::NoPlan)
    );
    st.organize_cancel(plan_id).unwrap();
    st.organize_cancel(12345).unwrap();

    // The next plan runs normally.
    let next = st
        .organize_plan(&offline(pile.path()), &[], Arc::new(Recorder::default()))
        .unwrap();
    assert!(next.plan_id > plan_id);
}

#[test]
fn opening_an_organised_folder_without_markers_suggests_the_default_layout() {
    let config = TempDir::new().unwrap();
    let st = state(&config);
    let dir = TempDir::new().unwrap();
    write(dir.path(), "ROMS/GBA/a.gba", b"x");

    let view = st
        .open(dir.path(), None, &SystemDb::builtin(), &mut |_| {})
        .unwrap();
    assert_eq!(view.profile.id, "anbernic-stock");
    assert!(view.profile.detected);
    assert_eq!(view.profile.evidence, vec!["system-folders".to_string()]);
    assert_eq!(view.entries.len(), 1);
}

#[test]
fn the_weak_rule_follows_the_organise_default_layout() {
    let config = TempDir::new().unwrap();
    let st = state(&config);
    let out = st
        .update_settings(&serde_json::json!({"organize": {"default_layout": "batocera"}}))
        .unwrap();
    assert!(out.errors.is_empty(), "{:?}", out.errors);
    let dir = TempDir::new().unwrap();
    write(dir.path(), "ROMs/gba/a.gba", b"x");

    let view = st
        .open(dir.path(), None, &SystemDb::builtin(), &mut |_| {})
        .unwrap();
    assert_eq!(view.profile.id, "batocera");
    assert_eq!(view.profile.evidence, vec!["system-folders".to_string()]);
}

#[test]
fn a_plain_pile_still_answers_no_profile() {
    let config = TempDir::new().unwrap();
    let st = state(&config);
    let pile = gba_pile();
    let err = st
        .open(pile.path(), None, &SystemDb::builtin(), &mut |_| {})
        .unwrap_err();
    assert!(matches!(err, IpcError::NoProfile(_)));
    assert_eq!(
        serde_json::to_value(&err).unwrap()["code"],
        serde_json::json!("no-profile")
    );
}

fn custom_layout(roms_root: &str) -> coverly_core::profiles::custom::ProfileFile {
    use coverly_core::profiles::custom::{ProfileFile, SlotFile};
    use coverly_core::profiles::{FolderNaming, ImageFormat};
    ProfileFile {
        id: "mine".into(),
        name: "Mine".into(),
        roms_root: roms_root.into(),
        art_dirs: vec!["covers".into()],
        gamelist: false,
        slots: std::collections::BTreeMap::from([(
            "cover".into(),
            SlotFile {
                template: "{system_dir}/covers/{stem}.png".into(),
                format: ImageFormat::Png,
                max_width: None,
                max_height: None,
            },
        )]),
        system_folder: FolderNaming::default(),
        folder_names: std::collections::BTreeMap::new(),
        host_folders: std::collections::BTreeMap::new(),
    }
}

#[test]
fn editing_the_chosen_custom_layout_surveys_again() {
    let config = TempDir::new().unwrap();
    let st = state(&config);
    let saved = st.save_profile(custom_layout("Games"), None).unwrap();
    assert!(saved.errors.is_empty(), "{:?}", saved.errors);
    let pile = gba_pile();
    let req = OrganizeRequest {
        layout: Some("mine".into()),
        ..offline(pile.path())
    };
    let first = st
        .organize_plan(&req, &[], Arc::new(Recorder::default()))
        .unwrap();
    assert!(
        first.plan.roms_dir.ends_with("Games"),
        "{:?}",
        first.plan.roms_dir
    );

    // Same id, other layout: a kept survey (walked with the old folders)
    // must not be reused.
    let saved = st
        .save_profile(custom_layout("Library"), Some("mine".into()))
        .unwrap();
    assert!(saved.errors.is_empty(), "{:?}", saved.errors);
    let events = Arc::new(Recorder::default());
    let second = st.organize_plan(&req, &[], events.clone()).unwrap();
    assert!(!events.events().is_empty(), "surveyed again");
    assert!(
        second.plan.roms_dir.ends_with("Library"),
        "{:?}",
        second.plan.roms_dir
    );
}

// ---- Apply, resume and undo (Task 28) ----

use coverly_core::write::{JobKind, OverwritePolicy, Writer};
use coverly_roms_lib::organize::view::{OrganizeJobState, OrganizeJobView};
use std::sync::mpsc;

/// Records apply events; optionally holds the first apply progress event
/// until released, so a test can act while the apply runs.
#[derive(Default)]
struct ApplyRecorder {
    progress: Mutex<Vec<OrganizeProgressEvent>>,
    changed: Mutex<Vec<OrganizeJobView>>,
    hold: Mutex<Option<(mpsc::SyncSender<()>, mpsc::Receiver<()>)>>,
}

impl ApplyRecorder {
    /// A recorder whose first progress event blocks: `entered` fires when
    /// the apply reached it, sending on `release` lets it go on.
    fn holding() -> (Arc<ApplyRecorder>, mpsc::Receiver<()>, mpsc::Sender<()>) {
        let (entered_tx, entered_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::channel();
        let rec = ApplyRecorder {
            hold: Mutex::new(Some((entered_tx, release_rx))),
            ..ApplyRecorder::default()
        };
        (Arc::new(rec), entered_rx, release_tx)
    }

    /// The final view, waiting up to 10 s for the worker to end.
    fn wait_end(&self) -> OrganizeJobView {
        let start = std::time::Instant::now();
        loop {
            if let Some(v) = self
                .changed
                .lock()
                .unwrap()
                .iter()
                .find(|v| v.state != OrganizeJobState::Applying)
            {
                return v.clone();
            }
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "apply never ended"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

impl OrganizeEvents for ApplyRecorder {
    fn progress(&self, event: OrganizeProgressEvent) {
        let held = self.hold.lock().unwrap().take();
        if let Some((entered, release)) = held {
            entered.send(()).unwrap();
            release.recv().unwrap();
        }
        self.progress.lock().unwrap().push(event);
    }

    fn changed(&self, view: OrganizeJobView) {
        self.changed.lock().unwrap().push(view);
    }
}

fn data_dir(config: &TempDir) -> std::path::PathBuf {
    config.path().join("data")
}

#[test]
fn apply_runs_behind_the_manual_guard_and_refuses_a_parallel_fill() {
    let config = TempDir::new().unwrap();
    let st = state(&config);
    // An organised library to start a fill on.
    let lib = TempDir::new().unwrap();
    write(lib.path(), "ROMS/GBA/a.gba", b"x");
    let generation = st
        .open(lib.path(), None, &SystemDb::builtin(), &mut |_| {})
        .unwrap()
        .generation;

    let pile = gba_pile();
    let plan = st
        .organize_plan(&offline(pile.path()), &[], Arc::new(Recorder::default()))
        .unwrap();
    let (events, entered, release) = ApplyRecorder::holding();
    let started = st
        .organize_apply(plan.plan_id, &[], events.clone())
        .unwrap();
    assert_eq!(started.state, OrganizeJobState::Applying);
    assert_eq!(started.plan_id, plan.plan_id);
    assert_eq!(started.layout, "anbernic-stock");
    assert_eq!(started.target_root, pile.path().display().to_string());
    entered.recv_timeout(Duration::from_secs(10)).unwrap();

    // While it runs: no fill, no second apply, no undo.
    let fill = coverly_roms_lib::jobs::view::FillRequest {
        entry_ids: None,
        media: vec![coverly_core::profiles::Media::Cover],
        strategy: coverly_core::matching::StrategyKind::RomRegion,
        regions: vec![],
        overwrite: OverwritePolicy::Never,
    };
    struct NoJobEvents;
    impl coverly_roms_lib::jobs::JobEvents for NoJobEvents {
        fn progress(&self, _: coverly_roms_lib::jobs::view::JobProgressEvent) {}
        fn changed(&self, _: coverly_roms_lib::jobs::view::JobView) {}
        fn written(&self, _: u64, _: Vec<u32>) {}
    }
    assert_eq!(
        st.start_fill(generation, &fill, Arc::new(NoJobEvents))
            .unwrap_err(),
        IpcError::JobRunning
    );
    let again = st
        .organize_plan(&offline(pile.path()), &[], Arc::new(Recorder::default()))
        .unwrap();
    assert_eq!(
        st.organize_apply(again.plan_id, &[], Arc::new(ApplyRecorder::default()))
            .unwrap_err(),
        IpcError::JobRunning
    );
    assert_eq!(
        st.organize_job().map(|v| v.state),
        Some(OrganizeJobState::Applying)
    );

    release.send(()).unwrap();
    let end = events.wait_end();
    assert_eq!(end.state, OrganizeJobState::Done, "{end:?}");
    let report = end.report.clone().unwrap();
    assert_eq!(report.moved, 3);
    assert!(end.job.is_some());
    assert!(pile.path().join("ROMS/GBA/Alpha (USA).gba").is_file());
    assert!(!pile.path().join("Alpha (USA).gba").exists());
    let progress = events.progress.lock().unwrap().clone();
    assert!(progress
        .iter()
        .all(|e| e.phase == OrganizePhase::Apply && e.plan_id == plan.plan_id));
    assert_eq!(progress.last().map(|e| (e.done, e.total)), Some((3, 3)));
    // The wire shape.
    let json = serde_json::to_value(&end).unwrap();
    assert_eq!(json["state"], "done");
    assert_eq!(json["report"]["moved"], 3);

    // The guard is released: fills start again; the pile changed, so the
    // kept plans are gone.
    st.start_fill(generation, &fill, Arc::new(NoJobEvents))
        .unwrap();
    assert_eq!(
        st.organize_apply(plan.plan_id, &[], Arc::new(ApplyRecorder::default()))
            .unwrap_err(),
        IpcError::NoPlan
    );
}

#[test]
fn apply_rechecks_decisions_against_the_stored_plan() {
    let config = TempDir::new().unwrap();
    let st = state(&config);
    let pile = gba_pile();
    let plan = st
        .organize_plan(&offline(pile.path()), &[], Arc::new(Recorder::default()))
        .unwrap();
    let unknown_item = [Decision {
        item: 42,
        choice: Choice::Leave,
    }];
    assert!(matches!(
        st.organize_apply(
            plan.plan_id,
            &unknown_item,
            Arc::new(ApplyRecorder::default())
        ),
        Err(IpcError::BadRequest(_))
    ));
    let unknown_system = [Decision {
        item: 0,
        choice: Choice::Assign {
            system: "no-such-system".into(),
        },
    }];
    assert!(matches!(
        st.organize_apply(
            plan.plan_id,
            &unknown_system,
            Arc::new(ApplyRecorder::default())
        ),
        Err(IpcError::BadRequest(_))
    ));
    assert_eq!(
        st.organize_apply(999, &[], Arc::new(ApplyRecorder::default()))
            .unwrap_err(),
        IpcError::NoPlan
    );
    assert!(st.organize_job().is_none(), "nothing started");
    // Nothing moved by the refused requests.
    assert!(pile.path().join("Alpha (USA).gba").is_file());

    // The decisions the backend applies are the ones sent with apply.
    let leave_first = [Decision {
        item: 0,
        choice: Choice::Leave,
    }];
    let events = Arc::new(ApplyRecorder::default());
    st.organize_apply(plan.plan_id, &leave_first, events.clone())
        .unwrap();
    let end = events.wait_end();
    assert_eq!(end.state, OrganizeJobState::Done);
    assert_eq!(end.report.unwrap().moved, 2);
    assert!(pile.path().join("Alpha (USA).gba").is_file());
}

#[test]
fn cancelling_an_apply_stops_before_the_next_item() {
    let config = TempDir::new().unwrap();
    let st = state(&config);
    let pile = gba_pile();
    let plan = st
        .organize_plan(&offline(pile.path()), &[], Arc::new(Recorder::default()))
        .unwrap();
    let (events, entered, release) = ApplyRecorder::holding();
    st.organize_apply(plan.plan_id, &[], events.clone())
        .unwrap();
    entered.recv_timeout(Duration::from_secs(10)).unwrap();
    st.organize_cancel_apply().unwrap();
    release.send(()).unwrap();
    let end = events.wait_end();
    assert_eq!(end.state, OrganizeJobState::Cancelled, "{end:?}");
    // The item already started finishes; the rest stay where they are.
    let report = end.report.unwrap();
    assert_eq!((report.moved, report.skipped), (1, 2), "{report:?}");
    let left = ["Alpha (USA).gba", "Beta (USA).gba", "Gamma (USA).gba"]
        .iter()
        .filter(|n| pile.path().join(n).is_file())
        .count();
    assert_eq!(left, 2);
    // The one move is journaled: undo puts it back.
    let job = end.job.expect("a journal for the moved item");
    st.organize_undo(&job, false).unwrap();
    for name in ["Alpha (USA).gba", "Beta (USA).gba", "Gamma (USA).gba"] {
        assert!(pile.path().join(name).is_file(), "{name}");
    }
    // Nothing runs: cancelling again is harmless.
    st.organize_cancel_apply().unwrap();
}

/// Applies the default plan of `pile` and returns the job id.
fn apply_all(st: &AppState, pile: &Path) -> String {
    let plan = st
        .organize_plan(&offline(pile), &[], Arc::new(Recorder::default()))
        .unwrap();
    let events = Arc::new(ApplyRecorder::default());
    st.organize_apply(plan.plan_id, &[], events.clone())
        .unwrap();
    let end = events.wait_end();
    assert_eq!(end.state, OrganizeJobState::Done, "{end:?}");
    end.job.unwrap()
}

#[test]
fn undo_refuses_when_a_later_fill_wrote_under_the_target_and_undoes_all_in_order_when_asked() {
    let config = TempDir::new().unwrap();
    let st = state(&config);
    let pile = gba_pile();
    let job = apply_all(&st, pile.path());

    // A later fill wrote a cover under the organised tree.
    let root = std::path::absolute(pile.path()).unwrap();
    let mut fill = Writer::begin(&root, &data_dir(&config)).unwrap();
    fill.write(
        &root.join("ROMS/GBA/Imgs/Alpha (USA).png"),
        b"art",
        OverwritePolicy::Never,
    )
    .unwrap();
    let fill = fill.finish().unwrap();

    match st.organize_undo(&job, false) {
        Err(IpcError::LaterJobs(detail)) => assert!(detail.contains(&fill), "{detail}"),
        other => panic!("expected later-jobs, got {other:?}"),
    }
    assert!(
        pile.path().join("ROMS/GBA/Alpha (USA).gba").is_file(),
        "nothing undone"
    );

    let undone = st.organize_undo(&job, true).unwrap();
    assert_eq!(
        undone
            .report
            .iter()
            .map(|r| r.job.clone())
            .collect::<Vec<_>>(),
        [fill.clone(), job.clone()],
        "newest first"
    );
    assert_eq!(undone.later_jobs.len(), 1);
    assert_eq!(undone.later_jobs[0].job, fill);
    assert!(pile.path().join("Alpha (USA).gba").is_file());
    assert!(!pile.path().join("ROMS").exists());
    let json = serde_json::to_value(&undone).unwrap();
    assert_eq!(json["later_jobs"][0]["kind"], "fill");

    // Undone already: nothing left to undo.
    assert_eq!(
        st.organize_undo(&job, true).unwrap_err(),
        IpcError::NothingToUndo
    );
    let history = st.job_history().unwrap();
    assert!(history.iter().all(|h| h.undone));
}

#[test]
fn undo_without_later_jobs_puts_the_pile_back() {
    let config = TempDir::new().unwrap();
    let st = state(&config);
    let pile = gba_pile();
    let job = apply_all(&st, pile.path());
    let undone = st.organize_undo(&job, false).unwrap();
    assert_eq!(undone.report.len(), 1);
    assert_eq!(undone.report[0].restored.len(), 3);
    assert!(undone.later_jobs.is_empty());
    for name in ["Alpha (USA).gba", "Beta (USA).gba", "Gamma (USA).gba"] {
        assert!(pile.path().join(name).is_file(), "{name}");
    }
}

#[test]
fn an_interrupted_job_is_listed_as_resumable() {
    let config = TempDir::new().unwrap();
    let st = state(&config);
    let pile = gba_pile();
    let job = apply_all(&st, pile.path());

    // As if the app stopped before the end: the journal has no `done`
    // line, two files are back in the pile (never moved), the plan stays.
    let journal = data_dir(&config).join("jobs").join(format!("{job}.jsonl"));
    let text = fs::read_to_string(&journal).unwrap();
    let kept: Vec<&str> = text
        .lines()
        .filter(|l| !l.contains("\"done\""))
        .filter(|l| !l.contains("Beta (USA)") && !l.contains("Gamma (USA)"))
        .collect();
    fs::write(&journal, kept.join("\n") + "\n").unwrap();
    for name in ["Beta (USA).gba", "Gamma (USA).gba"] {
        fs::rename(
            pile.path().join("ROMS/GBA").join(name),
            pile.path().join(name),
        )
        .unwrap();
    }

    let history = st.job_history().unwrap();
    let item = history.iter().find(|h| h.job == job).unwrap();
    assert_eq!(item.kind, JobKind::Organize);
    assert!(!item.complete);
    assert!(item.undoable);
    let json = serde_json::to_value(item).unwrap();
    assert_eq!(json["kind"], "organize");
    assert_eq!(json["undoable"], true);

    let events = Arc::new(ApplyRecorder::default());
    let resumed = st.organize_resume(&job, events.clone()).unwrap();
    assert_eq!(resumed.state, OrganizeJobState::Applying);
    assert_eq!(resumed.job.as_deref(), Some(job.as_str()));
    let end = events.wait_end();
    assert_eq!(end.state, OrganizeJobState::Done, "{end:?}");
    let report = end.report.unwrap();
    assert_eq!(report.job.as_deref(), Some(job.as_str()));
    assert_eq!(report.moved, 3);
    for name in ["Alpha (USA).gba", "Beta (USA).gba", "Gamma (USA).gba"] {
        assert!(pile.path().join("ROMS/GBA").join(name).is_file(), "{name}");
    }
    assert!(st
        .job_history()
        .unwrap()
        .iter()
        .any(|h| h.job == job && h.complete));

    // An unknown job has no plan to resume.
    assert_eq!(
        st.organize_resume(
            "20990101T000000Z-0000-0000",
            Arc::new(ApplyRecorder::default())
        )
        .unwrap_err(),
        IpcError::NoPlan
    );
}

#[test]
fn an_apply_that_finds_the_pile_gone_fails_instead_of_pausing_without_a_journal() {
    let config = TempDir::new().unwrap();
    let st = state(&config);
    let parent = TempDir::new().unwrap();
    let pile = parent.path().join("pile");
    fs::create_dir(&pile).unwrap();
    for (tag, name) in b"AB"
        .iter()
        .copied()
        .zip(["Alpha (USA).gba", "Beta (USA).gba"])
    {
        write_old(&pile, name, &fixtures::gba_tagged(tag));
    }
    let plan = st
        .organize_plan(&offline(&pile), &[], Arc::new(Recorder::default()))
        .unwrap();
    // The card is pulled between the plan and the apply.
    fs::rename(&pile, parent.path().join("elsewhere")).unwrap();

    let events = Arc::new(ApplyRecorder::default());
    st.organize_apply(plan.plan_id, &[], events.clone())
        .unwrap();
    let end = events.wait_end();
    // Nothing was journaled, so there is nothing `organize_resume` could
    // continue: a paused state would offer a resume that cannot work.
    assert_eq!(end.job, None);
    assert_eq!(end.state, OrganizeJobState::Failed, "{end:?}");
    assert!(
        matches!(end.error, Some(IpcError::VolumeGone(_))),
        "{:?}",
        end.error
    );
    assert!(st.job_history().unwrap().is_empty());
}

#[test]
fn apply_refuses_a_backup_folder_inside_the_pile() {
    let config = TempDir::new().unwrap();
    let pile = gba_pile();
    let st = AppState::new(
        config.path().to_path_buf(),
        FillConfig::new(
            config.path().join("cache"),
            pile.path().join("coverly-data"),
        ),
    );
    let plan = st
        .organize_plan(&offline(pile.path()), &[], Arc::new(Recorder::default()))
        .unwrap();
    assert!(matches!(
        st.organize_apply(plan.plan_id, &[], Arc::new(ApplyRecorder::default())),
        Err(IpcError::BadRequest(_))
    ));
    assert!(st.organize_job().is_none(), "nothing started");
    assert!(pile.path().join("Alpha (USA).gba").is_file());
    assert!(!pile.path().join("coverly-data").exists());
}
