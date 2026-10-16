mod common;

use std::path::PathBuf;

use common::anbernic_tree;
use coverly_core::systems::SystemDb;
use coverly_roms_lib::error::IpcError;
use coverly_roms_lib::jobs::FillConfig;
use coverly_roms_lib::library::view::ScanProgressEvent;
use coverly_roms_lib::library::{open_session, OpenRequest};
use coverly_roms_lib::state::AppState;
use tempfile::TempDir;

fn state(config: &TempDir) -> AppState {
    AppState::new(
        config.path().to_path_buf(),
        FillConfig::new(config.path().join("cache"), config.path().join("data")),
    )
}

#[test]
fn open_installs_the_session_and_remembers_the_library() {
    let config = TempDir::new().unwrap();
    let tree = anbernic_tree(&["GBA"], 3, 1);
    let st = state(&config);
    let mut events: Vec<ScanProgressEvent> = Vec::new();
    let view = st
        .open(tree.path(), None, &SystemDb::builtin(), &mut |e| {
            events.push(e)
        })
        .unwrap();
    assert_eq!(view.generation, 1);
    assert!(events.iter().all(|e| e.generation == 1));
    assert_eq!(st.session(1).unwrap().entries.len(), 3);

    let recent = st.recent();
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].path, tree.path().display().to_string());
    assert_eq!(recent[0].roms, 3);
    assert!(recent[0].available);
    let saved = coverly_core::settings::read_settings(
        &config.path().join(coverly_core::settings::SETTINGS_FILE),
        &[],
    );
    assert_eq!(saved.library.recent.len(), 1);
}

#[test]
fn stale_generation_is_rejected() {
    let config = TempDir::new().unwrap();
    let tree = anbernic_tree(&["GBA"], 1, 0);
    let st = state(&config);
    assert_eq!(st.session(1).unwrap_err(), IpcError::NoLibrary);
    st.open(tree.path(), None, &SystemDb::builtin(), &mut |_| {})
        .unwrap();
    st.open(tree.path(), None, &SystemDb::builtin(), &mut |_| {})
        .unwrap();
    assert_eq!(st.session(1).unwrap_err(), IpcError::StaleLibrary);
    assert!(st.session(2).is_ok());
}

#[test]
fn a_newer_open_supersedes_an_older_one() {
    let config = TempDir::new().unwrap();
    let tree = anbernic_tree(&["GBA"], 1, 0);
    let st = state(&config);
    let older = st.begin_open();
    let newer = st.begin_open();
    let scan = |generation| {
        open_session(
            &OpenRequest {
                path: tree.path(),
                profile_id: None,
                scan: &Default::default(),
                generation,
            },
            &SystemDb::builtin(),
            &mut |_| {},
        )
        .unwrap()
        .0
    };
    st.install(scan(newer)).unwrap();
    assert_eq!(st.install(scan(older)).unwrap_err(), IpcError::Superseded);
    assert_eq!(st.session(newer).unwrap().generation, newer);
}

#[test]
fn open_succeeds_when_settings_cannot_be_saved() {
    let config = TempDir::new().unwrap();
    // The config "folder" is a file: saving fails, opening must not.
    let config_dir = config.path().join("as-file");
    std::fs::write(&config_dir, "not a folder").unwrap();
    let st = AppState::new(
        config_dir,
        FillConfig::new(config.path().join("cache"), config.path().join("data")),
    );
    let tree = anbernic_tree(&["GBA"], 1, 0);
    assert!(st
        .open(tree.path(), None, &SystemDb::builtin(), &mut |_| {})
        .is_ok());
}

#[test]
fn failed_open_keeps_the_previous_library() {
    let config = TempDir::new().unwrap();
    let tree = anbernic_tree(&["GBA"], 1, 0);
    let st = state(&config);
    st.open(tree.path(), None, &SystemDb::builtin(), &mut |_| {})
        .unwrap();
    let missing = PathBuf::from("/definitely/not/here");
    assert!(st
        .open(&missing, None, &SystemDb::builtin(), &mut |_| {})
        .is_err());
    // The failed attempt took generation 2; the open library is still 1.
    assert!(st.session(1).is_ok());
}

#[test]
fn recents_report_unavailable_folders_and_can_be_forgotten() {
    let config = TempDir::new().unwrap();
    let tree = anbernic_tree(&["GBA"], 1, 0);
    let st = state(&config);
    st.open(tree.path(), None, &SystemDb::builtin(), &mut |_| {})
        .unwrap();
    let gone = tree.path().to_path_buf();
    drop(tree);
    let recent = st.recent();
    assert!(!recent[0].available);
    assert!(st.forget(&gone).unwrap().is_empty());
}

#[test]
fn ignored_dirs_are_saved_normalised() {
    let config = TempDir::new().unwrap();
    let st = state(&config);
    let view = st
        .set_ignored_dirs(vec![" bios ".into(), "a/b".into(), "Saves".into()])
        .unwrap();
    assert_eq!(view.ignored_dirs, ["bios", "Saves"]);
    assert_eq!(st.library_settings().ignored_dirs, ["bios", "Saves"]);
}

#[test]
fn open_announces_its_generation_before_scanning() {
    let config = TempDir::new().unwrap();
    let st = state(&config);
    let mut events: Vec<ScanProgressEvent> = Vec::new();
    // Even an open that fails at once first tells the UI its generation.
    let missing = PathBuf::from("/definitely/not/here");
    assert!(st
        .open(&missing, None, &SystemDb::builtin(), &mut |e| events
            .push(e))
        .is_err());
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].generation, 1);
    assert_eq!(events[0].done, 0);
    assert_eq!(events[0].roms, 0);
}
