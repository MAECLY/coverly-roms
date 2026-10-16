//! Settings in the app: load once, update with field errors, reset,
//! snapshots per job, applied when opening a library.

mod common;

use std::fs;
use std::time::Duration;

use coverly_core::profiles::Media;
use coverly_core::settings::{FieldErrorCode, LoadNotice, ProxyChoice, ThemeChoice, SETTINGS_FILE};
use coverly_core::systems::SystemDb;
use coverly_roms_lib::error::IpcError;
use coverly_roms_lib::jobs::FillConfig;
use coverly_roms_lib::state::AppState;
use serde_json::json;
use tempfile::TempDir;

fn state(dir: &TempDir) -> AppState {
    AppState::new(
        dir.path().join("config"),
        FillConfig::new(dir.path().join("cache"), dir.path().join("data")),
    )
}

#[test]
fn starts_with_defaults_and_no_notice() {
    let dir = TempDir::new().unwrap();
    let st = state(&dir);
    let view = st.settings_view();
    assert_eq!(view.settings, coverly_core::settings::Settings::default());
    assert_eq!(view.notice, None);
    assert!(
        !dir.path().join("config").join(SETTINGS_FILE).exists(),
        "nothing written until a change"
    );
}

#[test]
fn a_corrupt_file_at_startup_reports_the_reset() {
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join("config")).unwrap();
    fs::write(
        dir.path().join("config").join(SETTINGS_FILE),
        "\u{0}\u{1}garbage",
    )
    .unwrap();
    let view = state(&dir).settings_view();
    assert_eq!(view.notice, Some(LoadNotice::ResetCorrupt));
    assert!(view.backup.unwrap().contains("settings.json.bad-"));
}

#[test]
fn update_saves_valid_changes_and_returns_field_errors_without_saving() {
    let dir = TempDir::new().unwrap();
    let st = state(&dir);
    let ok = st
        .update_settings(&json!({"general": {"theme": "dark"}, "network": {"timeout_secs": 60}}))
        .unwrap();
    assert!(ok.errors.is_empty());
    assert_eq!(ok.settings.general.theme, ThemeChoice::Dark);
    let saved = fs::read_to_string(dir.path().join("config").join(SETTINGS_FILE)).unwrap();
    assert!(saved.contains("\"theme\": \"dark\""));
    let bad = st
        .update_settings(&json!({"network": {"timeout_secs": 1, "proxy": "manual"}}))
        .unwrap();
    let fields: Vec<(&str, FieldErrorCode)> = bad
        .errors
        .iter()
        .map(|e| (e.field.as_str(), e.code))
        .collect();
    assert!(fields.contains(&("network.timeout_secs", FieldErrorCode::Range)));
    assert!(fields.contains(&("network.proxy_url", FieldErrorCode::Required)));
    assert_eq!(
        bad.settings.network.timeout_secs, 60,
        "the current settings come back"
    );
    assert_eq!(
        st.settings().network.proxy,
        ProxyChoice::System,
        "nothing was saved"
    );
}

#[test]
fn scan_changes_ask_for_a_reopen() {
    let dir = TempDir::new().unwrap();
    let st = state(&dir);
    assert!(
        !st.update_settings(&json!({"general": {"theme": "light"}}))
            .unwrap()
            .reopen
    );
    assert!(
        st.update_settings(&json!({"library": {"include_hidden": true}}))
            .unwrap()
            .reopen
    );
}

#[test]
fn reset_keeps_recents_and_backs_up_the_file() {
    let dir = TempDir::new().unwrap();
    let st = state(&dir);
    let lib = common::small_library();
    st.open(lib.path(), None, &SystemDb::builtin(), &mut |_| {})
        .unwrap();
    st.update_settings(&json!({"art": {"jpeg_quality": 70}}))
        .unwrap();
    let view = st.reset_settings().unwrap();
    assert_eq!(view.settings.art.jpeg_quality, 90);
    assert_eq!(
        view.settings.library.recent.len(),
        1,
        "recents survive a reset"
    );
    let names: Vec<String> = fs::read_dir(dir.path().join("config"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        names.iter().any(|n| n.starts_with("settings.json.reset-")),
        "{names:?}"
    );
}

#[test]
fn the_fill_config_is_a_snapshot_of_the_settings() {
    let dir = TempDir::new().unwrap();
    let st = state(&dir);
    let backups = dir.path().join("elsewhere");
    st.update_settings(&json!({
        "network": {"timeout_secs": 12, "concurrency": 2},
        "safety": {"backup_dir": backups.to_string_lossy()}
    }))
    .unwrap();
    let before = st.fill_config();
    assert_eq!(before.net.timeout, Duration::from_secs(12));
    assert_eq!(before.net.concurrency, 2);
    assert_eq!(before.data_dir, backups);
    assert_eq!(before.settings.network.timeout_secs, 12);
    st.update_settings(&json!({"network": {"timeout_secs": 99}}))
        .unwrap();
    assert_eq!(
        before.net.timeout,
        Duration::from_secs(12),
        "a running job keeps its snapshot"
    );
    assert_eq!(st.fill_config().net.timeout, Duration::from_secs(99));
}

#[test]
fn a_job_runs_with_the_settings_it_started_with() {
    let w = common::fill::fill_world();
    let st = AppState::new(w.dirs.path().join("config"), (*w.cfg).clone());
    common::fast(&st);
    let view = st
        .open(w.lib.path(), None, &SystemDb::builtin(), &mut |_| {})
        .unwrap();
    let rec = std::sync::Arc::new(common::fill::Recorder::default());
    let id = st
        .start_fill(view.generation, &common::fill::request(None), rec.clone())
        .unwrap()
        .id;
    common::fill::wait_for(&rec, coverly_roms_lib::jobs::view::JobState::Planned);
    // The user moves the backup folder between the dry run and Run.
    let moved = w.dirs.path().join("moved-backups");
    st.update_settings(&json!({"safety": {"backup_dir": moved.to_string_lossy()}}))
        .unwrap();
    st.jobs().run(id, rec.clone()).unwrap();
    common::fill::wait_for(&rec, coverly_roms_lib::jobs::view::JobState::Done);
    assert!(
        !coverly_core::write::list_jobs(&w.cfg.data_dir)
            .unwrap()
            .is_empty(),
        "the journal is in the folder the job started with"
    );
    assert!(coverly_core::write::list_jobs(&moved).map_or(true, |j| j.is_empty()));
}

#[test]
fn opening_a_library_applies_the_settings() {
    let dir = TempDir::new().unwrap();
    let st = state(&dir);
    let lib = common::small_library();
    fs::create_dir_all(lib.path().join("ROMS/GBA/Junk")).unwrap();
    fs::write(lib.path().join("ROMS/GBA/Junk/Hidden Gem.gba"), b"x").unwrap();
    st.update_settings(&json!({
        "library": {"ignored_dirs": ["Junk"]},
        "art": {"single_slot_media": "screenshot"}
    }))
    .unwrap();
    let view = st
        .open(lib.path(), None, &SystemDb::builtin(), &mut |_| {})
        .unwrap();
    assert!(view.entries.iter().all(|e| !e.rel_path.contains("Junk")));
    assert_eq!(view.profile.media, [Media::Screenshot]);
}

#[test]
fn a_backup_folder_inside_the_library_is_refused() {
    let dir = TempDir::new().unwrap();
    let st = state(&dir);
    let lib = common::small_library();
    let view = st
        .open(lib.path(), None, &SystemDb::builtin(), &mut |_| {})
        .unwrap();
    let inside = std::path::absolute(lib.path()).unwrap().join("backups");
    st.update_settings(&json!({"safety": {"backup_dir": inside.to_string_lossy()}}))
        .unwrap();
    let err = st
        .remove_entry_art(view.generation, 0, Media::Cover)
        .unwrap_err();
    assert!(
        matches!(err, IpcError::BadRequest(ref m) if m.contains("backup folder")),
        "{err:?}"
    );
}

#[test]
fn clear_cache_reports_freed_bytes_and_is_refused_while_a_job_runs() {
    let dir = TempDir::new().unwrap();
    let st = state(&dir);
    let art = dir.path().join("cache/art/ab");
    fs::create_dir_all(&art).unwrap();
    fs::write(art.join("ab".repeat(20)), vec![1u8; 5000]).unwrap();
    assert!(st.cache_usage() >= 5000);
    {
        let _busy = st.jobs().begin_manual().unwrap();
        assert_eq!(st.clear_cache().unwrap_err(), IpcError::JobRunning);
    }
    assert!(st.clear_cache().unwrap() >= 5000);
    assert_eq!(st.cache_usage(), 0);
}
