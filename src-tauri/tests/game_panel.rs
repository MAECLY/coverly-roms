//! The game panel's commands over a real `AppState` (part C D3, D7, D11):
//! revision defaults saved per library, the art updates and DLC carry
//! read into their slots in an undoable job, and the lazy details.

use std::fs;
use std::path::{Path, PathBuf};

use coverly_core::art::children::{ChildOrigin, SkipWhy};
use coverly_core::catalog::latest::{LatestKnown, LatestSource, LatestVersions};
use coverly_core::consoles::testing as ct;
use coverly_core::dedupe::testing::pin_mtime;
use coverly_core::settings::{read_settings, SETTINGS_FILE};
use coverly_core::systems::SystemDb;
use coverly_roms_lib::error::IpcError;
use coverly_roms_lib::jobs::FillConfig;
use coverly_roms_lib::library::view::LibraryView;
use coverly_roms_lib::state::AppState;
use tempfile::TempDir;

fn state(config: &TempDir) -> AppState {
    AppState::new(
        config.path().to_path_buf(),
        FillConfig::new(config.path().join("cache"), config.path().join("data")),
    )
}

/// Writes `bytes` at `rel` (`/`-separated, joined by components) with a
/// pinned mtime.
fn put(root: &Path, rel: &str, bytes: &[u8]) -> PathBuf {
    let p = rel.split('/').fold(root.to_path_buf(), |p, c| p.join(c));
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(&p, bytes).unwrap();
    pin_mtime(&p, 1);
    p
}

fn open(st: &AppState, root: &Path) -> LibraryView {
    st.open(
        root,
        Some("coverly-library"),
        &SystemDb::builtin(),
        &mut |_| {},
    )
    .unwrap()
}

fn id_of(view: &LibraryView, name: &str) -> u32 {
    view.entries
        .iter()
        .find(|e| e.name == name)
        .unwrap_or_else(|| panic!("no entry {name}"))
        .id
}

/// Two revisions of one game and a lone game, under `ROMS/3DS/`.
fn revisions(root: &Path) {
    put(root, "ROMS/3DS/Star Courier (USA).cia", &[1u8; 64]);
    put(root, "ROMS/3DS/Star Courier (USA) (Rev 1).cia", &[2u8; 64]);
    put(root, "ROMS/3DS/Moss Garden (USA).cia", &[3u8; 64]);
}

fn saved_choices(config: &TempDir, root: &Path) -> Option<Vec<(String, String)>> {
    read_settings(&config.path().join(SETTINGS_FILE), &[])
        .revision_defaults_of(root)
        .map(|m| m.clone().into_iter().collect())
}

#[test]
fn set_default_revision_validates_membership() {
    let config = TempDir::new().unwrap();
    let st = state(&config);
    let other = TempDir::new().unwrap();
    revisions(other.path());
    let view = open(&st, other.path());
    let original = id_of(&view, "Star Courier (USA)");
    let group = view.entries[original as usize]
        .content
        .version_group
        .clone()
        .expect("two revisions share a group");
    st.set_default_revision(view.generation, original).unwrap();

    let lib = TempDir::new().unwrap();
    revisions(lib.path());
    let view = open(&st, lib.path());
    let g = view.generation;
    // A lone game is no revision; nor is an id that does not exist.
    assert!(matches!(
        st.set_default_revision(g, id_of(&view, "Moss Garden (USA)")),
        Err(IpcError::BadRequest(_))
    ));
    assert_eq!(
        st.set_default_revision(g, 99).unwrap_err(),
        IpcError::UnknownEntry(99)
    );
    let rev1 = id_of(&view, "Star Courier (USA) (Rev 1)");
    let choices = st.set_default_revision(g, rev1).unwrap();
    assert_eq!(
        choices.into_iter().collect::<Vec<_>>(),
        [(
            group.clone(),
            "ROMS/3DS/Star Courier (USA) (Rev 1).cia".to_string()
        )]
    );
    assert_eq!(
        saved_choices(&config, lib.path()),
        Some(vec![(
            group.clone(),
            "ROMS/3DS/Star Courier (USA) (Rev 1).cia".to_string()
        )])
    );
    // The other library's choice is untouched (its folder is not open).
    assert_eq!(
        saved_choices(&config, other.path()),
        Some(vec![(
            group.clone(),
            "ROMS/3DS/Star Courier (USA).cia".to_string()
        )])
    );
    // The reopened library sends its own choice.
    let view = open(&st, lib.path());
    assert_eq!(
        view.revision_defaults.get(&group).map(String::as_str),
        Some("ROMS/3DS/Star Courier (USA) (Rev 1).cia")
    );
    // A stale generation is refused.
    assert_eq!(
        st.set_default_revision(g, rev1).unwrap_err(),
        IpcError::StaleLibrary
    );
}

#[test]
fn a_stale_choice_of_the_open_library_is_pruned_on_save() {
    let config = TempDir::new().unwrap();
    let st = state(&config);
    let lib = TempDir::new().unwrap();
    revisions(lib.path());
    put(lib.path(), "ROMS/3DS/Kite Days (USA).cia", &[4u8; 64]);
    let kite_rev = put(
        lib.path(),
        "ROMS/3DS/Kite Days (USA) (Rev 1).cia",
        &[5u8; 64],
    );
    let view = open(&st, lib.path());
    let kite_group = view.entries[id_of(&view, "Kite Days (USA)") as usize]
        .content
        .version_group
        .clone()
        .unwrap();
    st.set_default_revision(view.generation, id_of(&view, "Kite Days (USA) (Rev 1)"))
        .unwrap();

    // Kite Days loses its second revision: its choice goes on the next save.
    fs::remove_file(kite_rev).unwrap();
    let view = open(&st, lib.path());
    let choices = st
        .set_default_revision(view.generation, id_of(&view, "Star Courier (USA)"))
        .unwrap();
    assert!(!choices.contains_key(&kite_group), "{choices:?}");
    assert_eq!(choices.len(), 1);
}

/// `Game/DLC/Pack.cia` with a meta SMDH and an update without one.
fn game_with_children(root: &Path) {
    let g = "ROMS/3DS/Lantern Harbor (USA)";
    put(
        root,
        &format!("{g}/Lantern Harbor (USA).cia"),
        &ct::cia(0x0004_0000_0005_5E00, false, false).head_bytes(),
    );
    put(
        root,
        &format!("{g}/DLC/Lantern Harbor (USA) - Pack.cia"),
        &ct::cia(0x0004_008C_0005_5E00, false, true).head_bytes(),
    );
    put(
        root,
        &format!("{g}/Updates/Lantern Harbor (USA) (v1.1).cia"),
        &ct::cia(0x0004_000E_0005_5E00, false, false).head_bytes(),
    );
}

#[test]
fn read_child_art_writes_the_package_icon_and_undoes() {
    let config = TempDir::new().unwrap();
    let st = state(&config);
    let lib = TempDir::new().unwrap();
    game_with_children(lib.path());
    let view = open(&st, lib.path());
    let id = id_of(&view, "Lantern Harbor (USA)");
    let entry = &view.entries[id as usize];
    let dlc = entry
        .content
        .children
        .iter()
        .find(|c| c.name.ends_with("Pack"))
        .unwrap()
        .index;
    let update = entry
        .content
        .children
        .iter()
        .find(|c| c.name.ends_with("(v1.1)"))
        .unwrap()
        .index;

    let out = st.read_child_art(view.generation, id).unwrap();
    assert!(out.job.is_some());
    assert_eq!(out.written, 1);
    let marks = &out.children.iter().find(|c| c.index == dlc).unwrap().art;
    assert!(marks.icon);
    assert_eq!(marks.icon_origin, Some(ChildOrigin::Package));
    assert_eq!(
        out.skipped
            .iter()
            .map(|s| (s.index, s.why))
            .collect::<Vec<_>>(),
        [(update, SkipWhy::NoArtInClear)]
    );
    let icon = lib
        .path()
        .join("ROMS")
        .join("3DS")
        .join("Lantern Harbor (USA)")
        .join("DLC")
        .join(".media")
        .join("Lantern Harbor (USA) - Pack-icon.png");
    assert!(fs::read(&icon).unwrap().starts_with(b"\x89PNG"));
    let bytes = coverly_roms_lib::library::art::read_child_art_bytes(
        &st.session(view.generation).unwrap(),
        id,
        dlc,
        coverly_core::art::children::ChildMedia::Icon,
    )
    .unwrap();
    assert_eq!(bytes, fs::read(&icon).unwrap());

    // Again: the slot is kept, nothing written, no job left behind.
    let again = st.read_child_art(view.generation, id).unwrap();
    assert_eq!((again.job, again.written), (None, 0));
    assert!(again
        .skipped
        .iter()
        .any(|s| s.index == dlc && s.why == SkipWhy::Exists));

    // The job is undone like any manual write.
    st.undo_last_job(view.generation).unwrap();
    assert!(!icon.exists());
    assert!(!icon.parent().unwrap().exists());

    assert_eq!(
        st.read_child_art(view.generation, 99).unwrap_err(),
        IpcError::UnknownEntry(99)
    );
}

#[test]
fn game_details_asks_the_title_list_offline() {
    struct Knows;
    impl LatestVersions for Knows {
        fn latest(&self, system: &str, base_title_id: &str) -> Option<LatestKnown> {
            (system == "3ds" && base_title_id == "0004000000055E00").then(|| LatestKnown {
                version: "1.3.0".into(),
                source: LatestSource::TitleDb,
            })
        }
    }
    let config = TempDir::new().unwrap();
    let lib = TempDir::new().unwrap();
    game_with_children(lib.path());

    let st = state(&config);
    let view = open(&st, lib.path());
    let id = id_of(&view, "Lantern Harbor (USA)");
    let details = st.game_details(view.generation, id, 7).unwrap();
    assert_eq!(details.request, 7);
    assert_eq!(details.latest, None);
    assert_eq!(details.product_code.as_deref(), Some("CTR-P-EKJA"));
    assert_eq!(details.children.len(), 2);

    let st = state(&config).with_title_list(Box::new(Knows));
    let view = open(&st, lib.path());
    let details = st.game_details(view.generation, id, 8).unwrap();
    assert_eq!(details.latest.map(|l| l.version), Some("1.3.0".into()));
    assert_eq!(
        st.game_details(view.generation, 99, 9).unwrap_err(),
        IpcError::UnknownEntry(99)
    );
}
