mod common;

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use common::fill::{fill_world, request, wait_for, FillWorld, Recorder, BOX, GBA};
use common::write;
use coverly_core::image::probe_dimensions;
use coverly_core::net::testing::{png_bytes, MockResponse};
use coverly_core::profiles::Media;
use coverly_core::sources::libretro::encode_segment;
use coverly_core::systems::SystemDb;
use coverly_roms_lib::error::IpcError;
use coverly_roms_lib::jobs::view::{ExportFormat, JobState};
use coverly_roms_lib::library::view::ArtMark;
use coverly_roms_lib::state::AppState;

fn opened(w: &FillWorld) -> (AppState, u64) {
    let st = AppState::new(w.dirs.path().to_path_buf(), (*w.cfg).clone());
    common::fast(&st);
    let view = st
        .open(w.lib.path(), None, &SystemDb::builtin(), &mut |_| {})
        .unwrap();
    (st, view.generation)
}

fn cover(w: &FillWorld, stem: &str) -> PathBuf {
    w.lib.path().join(format!("ROMS/GBA/Imgs/{stem}.png"))
}

#[test]
fn change_art_writes_the_picked_candidate_and_undo_removes_it() {
    let w = fill_world();
    let (st, generation) = opened(&w);
    let cands = st.entry_candidates(generation, 1, Media::Cover).unwrap();
    let japan = cands
        .iter()
        .find(|c| c.name == "Golden Adventure (Japan)")
        .unwrap()
        .index;
    let preview = st
        .entry_candidate_art(generation, 1, Media::Cover, japan)
        .unwrap();
    assert_eq!(&preview[..8], b"\x89PNG\r\n\x1a\n");
    let updates = st
        .set_entry_art(generation, 1, Media::Cover, japan)
        .unwrap();
    assert_eq!(updates.len(), 1);
    assert_eq!((updates[0].id, updates[0].art.cover), (1, ArtMark::Present));
    assert!(cover(&w, "002 Golden Adventure").exists());
    assert_eq!(st.undo_last_job(generation).unwrap().removed, 1);
    assert!(!cover(&w, "002 Golden Adventure").exists());
}

/// Writes the Japanese cover of entry 1, then makes the Golden Sun box
/// (entry 0) answer `broken` and tries to change entry 0 to it.
fn failed_change_after_a_real_one(broken: MockResponse) -> (FillWorld, AppState, u64, IpcError) {
    let w = fill_world();
    let (st, generation) = opened(&w);
    let cands = st.entry_candidates(generation, 1, Media::Cover).unwrap();
    let japan = cands
        .iter()
        .find(|c| c.name == "Golden Adventure (Japan)")
        .unwrap()
        .index;
    st.set_entry_art(generation, 1, Media::Cover, japan)
        .unwrap();
    let before: Vec<String> = st
        .job_history()
        .unwrap()
        .into_iter()
        .map(|j| j.job)
        .collect();
    assert_eq!(before.len(), 1);
    let cands = st.entry_candidates(generation, 0, Media::Cover).unwrap();
    let sun = cands
        .iter()
        .find(|c| c.name == "Golden Sun (USA, Europe)")
        .unwrap()
        .index;
    w.server.mock(
        &format!(
            "/{}/{BOX}/{}",
            encode_segment(GBA),
            encode_segment("Golden Sun (USA, Europe).png")
        ),
        broken,
    );
    let err = st
        .set_entry_art(generation, 0, Media::Cover, sun)
        .unwrap_err();
    let after: Vec<String> = st
        .job_history()
        .unwrap()
        .into_iter()
        .map(|j| j.job)
        .collect();
    assert_eq!(after, before, "a change that wrote nothing leaves no job");
    (w, st, generation, err)
}

#[test]
fn a_change_to_a_bad_image_leaves_no_job_and_undo_reverts_the_real_one() {
    let (w, st, generation, err) =
        failed_change_after_a_real_one(MockResponse::ok(b"<html>oops</html>".to_vec()));
    assert!(matches!(err, IpcError::BadImage(_)), "{err:?}");
    assert!(!cover(&w, "001 Golden Sun").exists());
    let undo = st.undo_last_job(generation).unwrap();
    assert_eq!(undo.removed, 1);
    assert!(!cover(&w, "002 Golden Adventure").exists());
}

#[test]
fn a_rate_limited_change_leaves_no_job_and_undo_reverts_the_real_one() {
    let (w, st, generation, err) = failed_change_after_a_real_one(MockResponse::status(429));
    assert!(matches!(err, IpcError::Network(_)), "{err:?}");
    assert_eq!(st.undo_last_job(generation).unwrap().removed, 1);
    assert!(!cover(&w, "002 Golden Adventure").exists());
}

#[test]
fn set_art_needs_the_candidates_of_that_entry() {
    let w = fill_world();
    let (st, generation) = opened(&w);
    assert!(matches!(
        st.set_entry_art(generation, 1, Media::Cover, 0),
        Err(IpcError::BadRequest(_))
    ));
    st.entry_candidates(generation, 0, Media::Cover).unwrap();
    assert!(matches!(
        st.set_entry_art(generation, 1, Media::Cover, 0),
        Err(IpcError::BadRequest(_))
    ));
    assert!(matches!(
        st.set_entry_art(generation, 0, Media::Cover, 42),
        Err(IpcError::BadRequest(_))
    ));
}

#[test]
fn a_local_image_replaces_the_art_with_a_backup() {
    let w = fill_world();
    write(w.lib.path(), "ROMS/GBA/Imgs/001 Golden Sun.png", b"old art");
    let (st, generation) = opened(&w);
    let file = w.dirs.path().join("pick.png");
    fs::write(&file, png_bytes(800, 800)).unwrap();
    let updates = st
        .set_entry_art_from_file(generation, 0, Media::Cover, &file)
        .unwrap();
    assert_eq!(updates[0].art.cover, ArtMark::Present);
    let (wd, ht) = probe_dimensions(&fs::read(cover(&w, "001 Golden Sun")).unwrap()).unwrap();
    assert!(wd <= 250 && ht <= 360, "{wd}x{ht}");
    assert_eq!(st.undo_last_job(generation).unwrap().restored, 1);
    assert_eq!(fs::read(cover(&w, "001 Golden Sun")).unwrap(), b"old art");
}

#[test]
fn a_file_that_is_not_an_image_is_refused() {
    let w = fill_world();
    let (st, generation) = opened(&w);
    let file = w.dirs.path().join("evil.png");
    fs::write(&file, b"<html><script>alert(1)</script></html>").unwrap();
    assert!(matches!(
        st.set_entry_art_from_file(generation, 0, Media::Cover, &file),
        Err(IpcError::BadImage(_))
    ));
    assert!(!cover(&w, "001 Golden Sun").exists());
}

#[test]
fn removing_art_backs_it_up_and_undo_restores_it() {
    let w = fill_world();
    write(w.lib.path(), "ROMS/GBA/Imgs/001 Golden Sun.png", b"old art");
    let (st, generation) = opened(&w);
    let updates = st.remove_entry_art(generation, 0, Media::Cover).unwrap();
    assert_eq!(updates[0].art.cover, ArtMark::Missing);
    assert!(!cover(&w, "001 Golden Sun").exists());
    let undo = st.undo_last_job(generation).unwrap();
    assert_eq!(undo.restored, 1);
    assert_eq!(undo.entries[0].art.cover, ArtMark::Present);
    assert_eq!(fs::read(cover(&w, "001 Golden Sun")).unwrap(), b"old art");
    assert_eq!(
        st.remove_entry_art(generation, 1, Media::Cover),
        Err(IpcError::ArtMissing("cover".into()))
    );
}

#[test]
fn audit_export_neutralises_formulas() {
    let w = fill_world();
    write(w.lib.path(), "ROMS/GBA/=HYPERLINK(1).gba", b"x");
    let (st, generation) = opened(&w);
    let csv = w.dirs.path().join("audit.csv");
    st.export_audit(generation, ExportFormat::Csv, &csv)
        .unwrap();
    let text = fs::read_to_string(&csv).unwrap();
    assert!(text.starts_with("system,folder,rel_path,stem,size,modified,cover"));
    assert!(text.contains("'=HYPERLINK(1)"), "{text}");
    let json = w.dirs.path().join("audit.json");
    st.export_audit(generation, ExportFormat::Json, &json)
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&fs::read_to_string(&json).unwrap()).unwrap();
    assert_eq!(v["totals"]["roms"], 5);
}

#[test]
fn job_report_export_lists_every_item() {
    let w = fill_world();
    let (st, generation) = opened(&w);
    let rec = Arc::new(Recorder::default());
    let id = st
        .start_fill(generation, &request(None), rec.clone())
        .unwrap()
        .id;
    wait_for(&rec, JobState::Planned);
    assert!(matches!(
        st.export_job_report(id, ExportFormat::Csv, &w.dirs.path().join("early.csv")),
        Err(IpcError::BadRequest(_))
    ));
    st.jobs().run(id, rec.clone()).unwrap();
    wait_for(&rec, JobState::Done);
    let csv = w.dirs.path().join("job.csv");
    st.export_job_report(id, ExportFormat::Csv, &csv).unwrap();
    let text = fs::read_to_string(&csv).unwrap();
    assert_eq!(text.lines().count(), 5);
    assert!(
        text.contains("GBA,003 Missing Game.gba,cover,failed,,,no-art,"),
        "{text}"
    );
    let json = w.dirs.path().join("job.json");
    st.export_job_report(id, ExportFormat::Json, &json).unwrap();
    let v: serde_json::Value = serde_json::from_str(&fs::read_to_string(&json).unwrap()).unwrap();
    assert_eq!(v["filled"], 2);
}

#[test]
fn a_drawer_write_and_a_fill_never_write_at_once() {
    let w = fill_world();
    write(w.lib.path(), "ROMS/GBA/Imgs/001 Golden Sun.png", b"old art");
    let (st, generation) = opened(&w);
    let rec = Arc::new(Recorder::default());
    {
        let _writing = st.jobs().begin_manual().unwrap();
        assert!(matches!(
            st.jobs().begin_manual(),
            Err(IpcError::JobRunning)
        ));
        assert!(matches!(
            st.remove_entry_art(generation, 0, Media::Cover),
            Err(IpcError::JobRunning)
        ));
        assert!(matches!(
            st.undo_last_job(generation),
            Err(IpcError::JobRunning)
        ));
        assert!(matches!(
            st.start_fill(generation, &request(None), rec.clone()),
            Err(IpcError::JobRunning)
        ));
    }
    assert!(cover(&w, "001 Golden Sun").exists());
    let id = st
        .start_fill(generation, &request(None), rec.clone())
        .unwrap()
        .id;
    wait_for(&rec, JobState::Planned);
    {
        let _writing = st.jobs().begin_manual().unwrap();
        assert!(matches!(
            st.jobs().run(id, rec.clone()),
            Err(IpcError::JobRunning)
        ));
    }
    st.jobs().run(id, rec.clone()).unwrap();
    wait_for(&rec, JobState::Done);
}
