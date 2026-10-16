mod common;

use std::sync::Arc;

use common::fill::fill_world;
use coverly_core::identify::IdMethod;
use coverly_core::profiles::Media;
use coverly_core::systems::SystemDb;
use coverly_roms_lib::error::IpcError;
use coverly_roms_lib::state::AppState;

#[test]
fn search_pick_is_remembered_and_keeps_the_file_name() {
    let w = fill_world();
    let st = AppState::new(w.dirs.path().join("config"), (*w.cfg).clone());
    common::fast(&st);
    let view = st
        .open(w.lib.path(), None, &SystemDb::builtin(), &mut |_| {})
        .unwrap();
    let g = view.generation;
    let found = st
        .search_entry_art(g, 3, Media::Cover, "golden sun")
        .unwrap();
    let pick = found
        .iter()
        .find(|c| c.name == "Golden Sun (USA, Europe)")
        .expect("listed");
    let bytes = st.entry_search_art(g, 3, Media::Cover, pick.index).unwrap();
    assert!(!bytes.is_empty());
    let updates = st
        .set_entry_art_from_search(g, 3, Media::Cover, pick.index)
        .unwrap();
    assert_eq!(updates.len(), 1);
    assert!(
        w.lib.path().join("ROMS/GBA/Imgs/004 Gldn Rsh.png").exists(),
        "the ROM's own name"
    );
    assert!(!w
        .lib
        .path()
        .join("ROMS/GBA/Imgs/Golden Sun (USA, Europe).png")
        .exists());
    let id = st.identify_entry(g, 3, &SystemDb::builtin()).unwrap();
    assert_eq!(id.method, IdMethod::Manual);
    assert_eq!(id.canonical.as_deref(), Some("Golden Sun (USA, Europe)"));
    st.undo_last_job(g).unwrap();
    assert!(!w.lib.path().join("ROMS/GBA/Imgs/004 Gldn Rsh.png").exists());
    assert_eq!(
        st.identify_entry(g, 3, &SystemDb::builtin())
            .unwrap()
            .method,
        IdMethod::Manual,
        "a manual match is data, not undone with the art"
    );
    st.clear_identity_override(g, 3).unwrap();
    assert_ne!(
        st.identify_entry(g, 3, &SystemDb::builtin())
            .unwrap()
            .method,
        IdMethod::Manual
    );
}

#[test]
fn manual_matches_survive_a_new_backup_folder() {
    let w = fill_world();
    let st = AppState::new(w.dirs.path().join("config"), (*w.cfg).clone());
    common::fast(&st);
    let g = st
        .open(w.lib.path(), None, &SystemDb::builtin(), &mut |_| {})
        .unwrap()
        .generation;
    let found = st
        .search_entry_art(g, 3, Media::Cover, "golden sun")
        .unwrap();
    st.set_entry_art_from_search(g, 3, Media::Cover, found[0].index)
        .unwrap();
    let moved = w.dirs.path().join("moved-backups");
    st.update_settings(&serde_json::json!({"safety": {"backup_dir": moved.to_string_lossy()}}))
        .unwrap();
    assert_eq!(
        st.identify_entry(g, 3, &SystemDb::builtin())
            .unwrap()
            .method,
        IdMethod::Manual
    );
    assert!(!moved
        .join(coverly_core::identify::overrides::OVERRIDES_FILE)
        .exists());
}

#[test]
fn hostile_queries_and_unlisted_picks_are_refused() {
    let w = fill_world();
    let st = Arc::new(AppState::new(
        w.dirs.path().join("config"),
        (*w.cfg).clone(),
    ));
    common::fast(&st);
    let g = st
        .open(w.lib.path(), None, &SystemDb::builtin(), &mut |_| {})
        .unwrap()
        .generation;
    for q in ["", "x\u{1b}[31m", &"y".repeat(5000)] {
        assert!(
            matches!(
                st.search_entry_art(g, 3, Media::Cover, q),
                Err(IpcError::BadRequest(_))
            ),
            "{q:?}"
        );
    }
    assert!(
        matches!(
            st.set_entry_art_from_search(g, 3, Media::Cover, 0),
            Err(IpcError::BadRequest(_))
        ),
        "search first"
    );
}
