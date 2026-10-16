mod common;

use std::sync::Arc;
use std::time::Duration;

use common::fill::{fill_world, request, wait_for, FillWorld, Recorder, ADVENTURE_USA};
use coverly_core::fill::Endpoints;
use coverly_core::net::testing::{HangingServer, MockResponse};
use coverly_core::net::NetConfig;
use coverly_core::profiles::Media;
use coverly_core::systems::SystemDb;
use coverly_roms_lib::error::IpcError;
use coverly_roms_lib::jobs::view::{JobProgressEvent, JobState, JobView};
use coverly_roms_lib::jobs::{FillConfig, JobEvents};
use coverly_roms_lib::library::art::read_art_bytes;
use coverly_roms_lib::library::view::ArtMark;
use coverly_roms_lib::state::AppState;

/// Events that refresh the state the way the Tauri ones do.
struct Refreshing {
    state: Arc<AppState>,
    rec: Recorder,
}

impl JobEvents for Refreshing {
    fn progress(&self, event: JobProgressEvent) {
        self.rec.progress(event);
    }
    fn changed(&self, view: JobView) {
        self.rec.changed(view);
    }
    fn written(&self, generation: u64, ids: Vec<u32>) {
        let _ = self
            .state
            .refresh_entries(generation, &ids, &SystemDb::builtin());
        self.rec.written(generation, ids);
    }
}

fn opened(w: &FillWorld, cfg: FillConfig) -> (Arc<AppState>, Arc<Refreshing>, u64) {
    let st = Arc::new(AppState::new(w.dirs.path().to_path_buf(), cfg));
    common::fast(&st);
    let view = st
        .open(w.lib.path(), None, &SystemDb::builtin(), &mut |_| {})
        .unwrap();
    let ev = Arc::new(Refreshing {
        state: st.clone(),
        rec: Recorder::default(),
    });
    (st, ev, view.generation)
}

#[test]
fn start_with_a_stale_generation_is_refused() {
    let w = fill_world();
    let (st, ev, generation) = opened(&w, (*w.cfg).clone());
    assert_eq!(
        st.start_fill(generation + 1, &request(None), ev),
        Err(IpcError::StaleLibrary)
    );
}

#[test]
fn a_finished_job_updates_the_library_and_undo_restores_it() {
    let w = fill_world();
    let (st, ev, generation) = opened(&w, (*w.cfg).clone());
    let id = st
        .start_fill(generation, &request(None), ev.clone())
        .unwrap()
        .id;
    wait_for(&ev.rec, JobState::Planned);
    st.jobs().run(id, ev.clone()).unwrap();
    wait_for(&ev.rec, JobState::Done);
    let session = st.session(generation).unwrap();
    assert!(read_art_bytes(&session, 0, Media::Cover).is_ok());

    let history = st.job_history().unwrap();
    assert_eq!(history.len(), 1);
    assert!(history[0].complete && !history[0].undone);
    assert_eq!(history[0].entries, 2);

    let undo = st.undo_last_job(generation).unwrap();
    assert_eq!((undo.removed, undo.restored, undo.kept), (2, 0, 0));
    let mut ids: Vec<u32> = undo.entries.iter().map(|e| e.id).collect();
    ids.sort_unstable();
    assert_eq!(ids, vec![0, 1]);
    assert!(undo.entries.iter().all(|e| e.art.cover == ArtMark::Missing));
    assert_eq!(
        read_art_bytes(&st.session(generation).unwrap(), 0, Media::Cover),
        Err(IpcError::ArtMissing("cover".into()))
    );
    assert!(st.job_history().unwrap()[0].undone);
    assert_eq!(st.undo_last_job(generation), Err(IpcError::NothingToUndo));
}

#[test]
fn undo_is_refused_while_a_job_runs() {
    let w = fill_world();
    let hang = HangingServer::start();
    let slow = FillConfig {
        endpoints: Endpoints {
            libretro: hang.base(),
            libretro_raw: hang.base(),
            database: hang.base(),
            steamgriddb: hang.base(),
            screenscraper: hang.base(),
        },
        net: NetConfig {
            retries: 0,
            timeout: Duration::from_millis(500),
            ..NetConfig::default()
        },
        ..(*w.cfg).clone()
    };
    let (st, ev, generation) = opened(&w, slow);
    // The shortest timeout the settings allow (the job's network comes
    // from the settings).
    st.update_settings(&serde_json::json!({"network": {"timeout_secs": 5}}))
        .unwrap();
    let id = st
        .start_fill(generation, &request(None), ev.clone())
        .unwrap()
        .id;
    assert_eq!(st.undo_last_job(generation), Err(IpcError::JobRunning));
    st.jobs().cancel(id).unwrap();
    wait_for(&ev.rec, JobState::Cancelled);
}

#[test]
fn undo_is_refused_while_a_job_is_paused() {
    let w = fill_world();
    let serial = FillConfig {
        net: NetConfig {
            concurrency: 1,
            retries: 0,
            ..NetConfig::default()
        },
        ..(*w.cfg).clone()
    };
    let (st, ev, generation) = opened(&w, serial);
    // The job's network comes from the settings (the base keeps timings).
    st.update_settings(&serde_json::json!({"network": {"concurrency": 1}}))
        .unwrap();
    let id = st
        .start_fill(generation, &request(None), ev.clone())
        .unwrap()
        .id;
    wait_for(&ev.rec, JobState::Planned);
    w.server.mock(
        ADVENTURE_USA,
        MockResponse::status(429).header("Retry-After", "0"),
    );
    st.jobs().run(id, ev.clone()).unwrap();
    wait_for(&ev.rec, JobState::Paused);
    // Resume appends to this journal: undoing it now would break the resume.
    assert_eq!(st.undo_last_job(generation), Err(IpcError::JobRunning));
    st.jobs().cancel(id).unwrap();
    assert_eq!(st.undo_last_job(generation).unwrap().removed, 1);
}

#[test]
fn undo_only_touches_the_open_library() {
    let w = fill_world();
    let (st, ev, generation) = opened(&w, (*w.cfg).clone());
    let id = st
        .start_fill(generation, &request(None), ev.clone())
        .unwrap()
        .id;
    wait_for(&ev.rec, JobState::Planned);
    st.jobs().run(id, ev.clone()).unwrap();
    wait_for(&ev.rec, JobState::Done);
    let cover = w.lib.path().join("ROMS/GBA/Imgs/001 Golden Sun.png");
    assert!(cover.exists());
    // Another library is opened: the first library's job is not its to undo.
    let other = common::anbernic_tree(&["GBA"], 2, 0);
    let view = st
        .open(other.path(), None, &SystemDb::builtin(), &mut |_| {})
        .unwrap();
    assert_eq!(
        st.undo_last_job(view.generation),
        Err(IpcError::NothingToUndo)
    );
    assert!(cover.exists());
    assert!(!st.job_history().unwrap()[0].undone);
}
