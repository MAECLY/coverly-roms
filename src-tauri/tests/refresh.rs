mod common;

use std::path::PathBuf;

use common::{anbernic_tree, open, write};
use coverly_core::profiles::Media;
use coverly_core::settings::LIBRARY_SETTINGS_FILE;
use coverly_core::systems::SystemDb;
use coverly_roms_lib::error::IpcError;
use coverly_roms_lib::jobs::FillConfig;
use coverly_roms_lib::library::art::read_art_bytes;
use coverly_roms_lib::library::refresh::{ids_for_art_paths, refresh_entries, subset_scan};
use coverly_roms_lib::library::view::ArtMark;
use coverly_roms_lib::state::AppState;
use tempfile::TempDir;

#[test]
fn refresh_reads_only_the_touched_entries() {
    let tree = anbernic_tree(&["FC", "GBA"], 3, 0);
    let (session, view) = open(tree.path(), None).unwrap();
    let id = view
        .entries
        .iter()
        .find(|e| e.rel_path == "ROMS/GBA/Game 00001 (USA).gba")
        .unwrap()
        .id;
    write(tree.path(), "ROMS/GBA/Imgs/Game 00001 (USA).png", b"png");
    let (next, updates) = refresh_entries(&session, &[id, 9999], &SystemDb::builtin());
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].id, id);
    assert_eq!(updates[0].art.cover, ArtMark::Present);
    assert!(next.entries[id as usize].art.contains_key(&Media::Cover));
    assert_eq!(next.entries.len(), session.entries.len());
    assert!(
        session.entries[id as usize].art.is_empty(),
        "the old session is untouched"
    );
}

#[test]
fn subset_scan_groups_entries_by_system_in_session_order() {
    let tree = anbernic_tree(&["FC", "GBA"], 3, 0);
    let (session, _) = open(tree.path(), None).unwrap();
    // FC holds ids 0..=2, GBA 3..=5.
    let (scan, order) = subset_scan(&session, &[5, 0, 5, 4, 77]);
    assert_eq!(order, vec![0, 4, 5]);
    assert_eq!(scan.systems.len(), 2);
    assert_eq!(scan.systems[0].folder_name, "FC");
    assert_eq!(scan.systems[0].roms.len(), 1);
    assert_eq!(scan.systems[1].roms.len(), 2);
    assert_eq!(scan.profile_id, "anbernic-stock");
}

#[test]
fn art_paths_map_back_to_their_entries() {
    let tree = anbernic_tree(&["FC", "GBA"], 3, 0);
    let (session, _) = open(tree.path(), None).unwrap();
    let ids = ids_for_art_paths(
        &session,
        &[
            tree.path().join("ROMS/GBA/Imgs/Game 00001 (USA).png"),
            PathBuf::from("/elsewhere/Other.png"),
        ],
    );
    // Same stem in both systems: refreshing one extra entry is harmless.
    assert_eq!(ids, vec![1, 4]);
}

#[test]
fn app_state_refreshes_the_open_generation_only() {
    let tree = anbernic_tree(&["GBA"], 2, 0);
    let config = TempDir::new().unwrap();
    let st = AppState::new(
        config.path().join(LIBRARY_SETTINGS_FILE),
        FillConfig::new(config.path().join("cache"), config.path().join("data")),
    );
    let db = SystemDb::builtin();
    let view = st.open(tree.path(), None, &db, &mut |_| {}).unwrap();
    write(
        tree.path(),
        "ROMS/GBA/Imgs/Game 00000 (USA).png",
        b"\x89PNG\r\n\x1a\nrest",
    );
    let updates = st.refresh_entries(view.generation, &[0], &db).unwrap();
    assert_eq!(updates[0].art.cover, ArtMark::Present);
    let session = st.session(view.generation).unwrap();
    assert!(read_art_bytes(&session, 0, Media::Cover)
        .unwrap()
        .starts_with(b"\x89PNG"));
    assert_eq!(
        st.refresh_entries(view.generation + 1, &[0], &db),
        Err(IpcError::StaleLibrary)
    );
}
