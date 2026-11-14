//! The Extras section's read side over a real `AppState` (part D D3, D4,
//! D22): a game's `Extras/` listed by id with a revision, handles of an
//! older listing refused, nothing ever written by a read, and Extras jobs
//! named after their game in the history.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use coverly_core::dedupe::testing::pin_mtime;
use coverly_core::extras::index::{list, IndexState};
use coverly_core::extras::ops::{create_category, ExtrasCtx};
use coverly_core::extras::{CategoryRef, ExtraType, ExtrasCategory, Provenance};
use coverly_core::organize::apply::user_available;
use coverly_core::systems::SystemDb;
use coverly_core::write::trash::FakeTrash;
use coverly_core::write::JobKind;
use coverly_roms_lib::error::IpcError;
use coverly_roms_lib::jobs::FillConfig;
use coverly_roms_lib::library::extras::{Playable, Writability};
use coverly_roms_lib::library::view::LibraryView;
use coverly_roms_lib::state::AppState;
use serde_json::json;
use tempfile::TempDir;

const GAME: &str = "ROMS/3DS/Lantern Harbor (USA)";

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

/// A 3DS game folder with extras in a fixed, a user and no category, an
/// index that tags one file and remembers one that is gone, and a loose
/// GBA game.
fn library(root: &Path) {
    put(
        root,
        &format!("{GAME}/Lantern Harbor (USA).cia"),
        &[1u8; 64],
    );
    put(
        root,
        &format!("{GAME}/Extras/Soundtrack/01 Opening.flac"),
        &[2u8; 32],
    );
    put(
        root,
        &format!("{GAME}/Extras/Artwork/front.png"),
        &[3u8; 16],
    );
    put(root, &format!("{GAME}/Extras/Remixes/boss.mp3"), &[4u8; 8]);
    put(root, &format!("{GAME}/Extras/readme.txt"), b"hello");
    put(root, &format!("{GAME}/Extras/Themes/harbor.p3t"), &[5u8; 8]);
    let index = json!({
        "version": 1,
        "categories": {},
        "files": [
            {
                "path": "Soundtrack/01 Opening.flac",
                "type": "audio",
                "provenance": "user",
                "origin": { "how": "added" },
                "tags": ["tema del jefe final"],
                "size": 32,
                "mtime_ns": 1_000_000_000i64,
                "added_at": "2026-11-13T12:00:00Z"
            },
            {
                "path": "Videos/gone.mp4",
                "type": "video",
                "provenance": "official",
                "origin": { "how": "organize" },
                "tags": ["intro"],
                "size": 99,
                "mtime_ns": 1_000_000_000i64,
                "added_at": "2026-11-13T12:00:00Z"
            }
        ]
    });
    put(
        root,
        &format!("{GAME}/Extras/extras.json"),
        serde_json::to_string_pretty(&index).unwrap().as_bytes(),
    );
    put(root, "ROMS/GBA/Moss Garden (USA).gba", &[6u8; 64]);
}

/// Every path below `root` with its size and modification time.
fn snapshot(root: &Path) -> BTreeMap<PathBuf, (bool, u64, SystemTime)> {
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for e in fs::read_dir(&dir).unwrap().flatten() {
            let m = fs::symlink_metadata(e.path()).unwrap();
            out.insert(e.path(), (m.is_dir(), m.len(), m.modified().unwrap()));
            if m.is_dir() {
                stack.push(e.path());
            }
        }
    }
    out
}

#[test]
fn extras_list_returns_categories_files_missing_and_a_rev() {
    let config = TempDir::new().unwrap();
    let st = state(&config);
    let lib = TempDir::new().unwrap();
    library(lib.path());
    let view = open(&st, lib.path());
    let id = id_of(&view, "Lantern Harbor (USA)");

    let extras = st.extras_list(view.generation, id).unwrap();
    assert_eq!(extras.entry_id, id);
    assert!(extras.rev > 0);
    assert!(extras.game_folder);
    assert_eq!(extras.index, IndexState::Ok);
    assert_eq!(extras.writable, Writability::Yes);
    assert_eq!(extras.system, "3ds");
    assert!(
        extras.manual_inside,
        "the 3DS keeps its manual inside the game"
    );
    assert!(!extras.capped);

    // Every fixed category is a slot (present or not), then user ones,
    // then the files loose in `Extras/`.
    let cats: Vec<(&CategoryRef, bool, u32)> = extras
        .categories
        .iter()
        .map(|c| (&c.category, c.present, c.count))
        .collect();
    let fixed = |c| CategoryRef::Fixed(c);
    assert_eq!(
        cats,
        [
            (&fixed(ExtrasCategory::Soundtrack), true, 1),
            (&fixed(ExtrasCategory::Manuals), false, 0),
            (&fixed(ExtrasCategory::Artwork), true, 1),
            (&fixed(ExtrasCategory::Videos), false, 0),
            (&fixed(ExtrasCategory::Themes), true, 1),
            (&fixed(ExtrasCategory::Guides), false, 0),
            (&CategoryRef::User("Remixes".into()), true, 1),
            (&CategoryRef::Root, true, 1),
        ]
    );

    let files: Vec<(u32, &str, ExtraType, Playable)> = extras
        .files
        .iter()
        .map(|f| (f.index, f.path.as_str(), f.kind, f.playable))
        .collect();
    assert_eq!(files.len(), 5, "{files:?}");
    for (i, f) in extras.files.iter().enumerate() {
        assert_eq!(f.index as usize, i);
    }
    let by_path = |p: &str| {
        extras
            .files
            .iter()
            .find(|f| f.path.split('/').eq(p.split('/')))
            .unwrap_or_else(|| panic!("no file {p}"))
    };
    let flac = by_path("Soundtrack/01 Opening.flac");
    assert_eq!(flac.name, "01 Opening.flac");
    assert_eq!(flac.kind, ExtraType::Audio);
    assert_eq!(flac.tags, ["tema del jefe final"]);
    assert_eq!(flac.size, 32);
    assert_eq!(flac.playable, Playable::Inline);
    assert_eq!(flac.provenance, Provenance::User);
    assert_eq!(by_path("readme.txt").category, CategoryRef::Root);
    assert_eq!(by_path("readme.txt").playable, Playable::Inline);
    assert_eq!(by_path("Themes/harbor.p3t").playable, Playable::RevealOnly);
    assert_eq!(by_path("Remixes/boss.mp3").tags, Vec::<String>::new());

    assert_eq!(extras.missing.len(), 1);
    let gone = &extras.missing[0];
    assert_eq!(gone.index, 0);
    assert_eq!(gone.name, "gone.mp4");
    assert_eq!(gone.category, CategoryRef::Fixed(ExtrasCategory::Videos));
    assert_eq!(gone.tags, ["intro"]);
    assert_eq!(gone.provenance, Provenance::Official);

    // Ids only: no absolute path crosses the bridge.
    let wire = serde_json::to_string(&extras).unwrap();
    assert!(!wire.contains(&*lib.path().to_string_lossy()), "{wire}");
    let canon = fs::canonicalize(lib.path()).unwrap();
    assert!(!wire.contains(&*canon.to_string_lossy()), "{wire}");
}

#[test]
fn each_listing_bumps_rev_and_handles_from_an_old_rev_are_refused() {
    let config = TempDir::new().unwrap();
    let st = state(&config);
    let lib = TempDir::new().unwrap();
    library(lib.path());
    let view = open(&st, lib.path());
    let g = view.generation;
    let id = id_of(&view, "Lantern Harbor (USA)");

    let first = st.extras_list(g, id).unwrap();
    let second = st.extras_list(g, id).unwrap();
    assert!(second.rev > first.rev);

    assert_eq!(
        st.extras_handle(g, id, first.rev, 0).unwrap_err(),
        IpcError::ExtrasChanged
    );
    let handle = st.extras_handle(g, id, second.rev, 0).unwrap();
    assert_eq!(handle.file.name, second.files[0].name);
    assert!(matches!(
        st.extras_handle(g, id, second.rev, 99),
        Err(IpcError::BadRequest(_))
    ));
    // Another entry's listing has its own revision.
    let other = id_of(&view, "Moss Garden (USA)");
    assert_eq!(
        st.extras_handle(g, other, second.rev, 0).unwrap_err(),
        IpcError::ExtrasChanged
    );
}

#[test]
fn extras_list_for_a_non_game_folder_entry_is_empty_and_never_creates_anything() {
    let config = TempDir::new().unwrap();
    let st = state(&config);
    let lib = TempDir::new().unwrap();
    library(lib.path());
    let view = open(&st, lib.path());
    let before = snapshot(lib.path());

    let id = id_of(&view, "Moss Garden (USA)");
    let extras = st.extras_list(view.generation, id).unwrap();
    assert!(!extras.game_folder);
    assert_eq!(extras.writable, Writability::ReadOnly);
    assert_eq!(extras.system, "gba");
    assert!(!extras.manual_inside);
    assert!(extras.categories.is_empty());
    assert!(extras.files.is_empty());
    assert!(extras.missing.is_empty());
    assert_eq!(extras.index, IndexState::Absent);
    assert_eq!(snapshot(lib.path()), before);

    assert_eq!(
        st.extras_list(view.generation, 999).unwrap_err(),
        IpcError::UnknownEntry(999)
    );
}

#[test]
fn extras_list_never_writes() {
    let config = TempDir::new().unwrap();
    let st = state(&config);
    let lib = TempDir::new().unwrap();
    library(lib.path());
    // A malformed index is reported, never backed up nor rewritten by a read.
    put(
        lib.path(),
        &format!("{GAME}/Extras/extras.json"),
        b"{ not json",
    );
    // A game folder without `Extras/` gets none from a read.
    put(
        lib.path(),
        "ROMS/3DS/Kite Days (USA)/Kite Days (USA).cia",
        &[7u8; 64],
    );
    put(
        lib.path(),
        "ROMS/3DS/Kite Days (USA)/Mods/patch.bin",
        &[8u8; 8],
    );
    let view = open(&st, lib.path());
    let before = snapshot(lib.path());
    let data_before = snapshot(config.path());

    let broken = st
        .extras_list(view.generation, id_of(&view, "Lantern Harbor (USA)"))
        .unwrap();
    assert!(
        matches!(broken.index, IndexState::Malformed { .. }),
        "{:?}",
        broken.index
    );
    let kite = st
        .extras_list(view.generation, id_of(&view, "Kite Days (USA)"))
        .unwrap();
    assert!(kite.game_folder);
    assert_eq!(kite.index, IndexState::Absent);
    assert!(kite.files.is_empty());
    assert!(kite.categories.iter().all(|c| !c.present));
    assert_eq!(kite.categories.len(), ExtrasCategory::ALL.len());

    assert_eq!(snapshot(lib.path()), before);
    assert_eq!(snapshot(config.path()), data_before, "no journal either");
}

#[test]
fn a_newer_index_or_a_running_job_makes_the_listing_view_only() {
    let config = TempDir::new().unwrap();
    let st = state(&config);
    let lib = TempDir::new().unwrap();
    library(lib.path());
    let view = open(&st, lib.path());
    let id = id_of(&view, "Lantern Harbor (USA)");
    {
        let _writing = st.jobs().begin_manual().unwrap();
        let busy = st.extras_list(view.generation, id).unwrap();
        assert_eq!(busy.writable, Writability::Busy);
        assert_eq!(busy.files.len(), 5, "a busy library still lists");
    }
    put(
        lib.path(),
        &format!("{GAME}/Extras/extras.json"),
        br#"{ "version": 9, "files": [] }"#,
    );
    let newer = st.extras_list(view.generation, id).unwrap();
    assert_eq!(newer.index, IndexState::Newer { version: 9 });
    assert_eq!(newer.writable, Writability::Newer);
}

#[test]
fn old_generation_is_refused() {
    let config = TempDir::new().unwrap();
    let st = state(&config);
    let lib = TempDir::new().unwrap();
    library(lib.path());
    let old = open(&st, lib.path());
    let id = id_of(&old, "Lantern Harbor (USA)");
    let listed = st.extras_list(old.generation, id).unwrap();
    let new = open(&st, lib.path());
    assert_eq!(
        st.extras_list(old.generation, id).unwrap_err(),
        IpcError::StaleLibrary
    );
    assert_eq!(
        st.extras_handle(old.generation, id, listed.rev, 0)
            .unwrap_err(),
        IpcError::StaleLibrary
    );
    // The reopened library's listings start afresh: the old revision is
    // nothing there.
    assert_eq!(
        st.extras_handle(new.generation, id, listed.rev, 0)
            .unwrap_err(),
        IpcError::ExtrasChanged
    );
    let fresh = st.extras_list(new.generation, id).unwrap();
    assert!(fresh.rev > listed.rev);
}

#[test]
fn job_history_names_extras_jobs() {
    let config = TempDir::new().unwrap();
    let st = state(&config);
    let lib = TempDir::new().unwrap();
    library(lib.path());
    let root = std::path::absolute(lib.path()).unwrap();
    let game = GAME.split('/').fold(root.clone(), |p, c| p.join(c));
    let trash_dir = TempDir::new().unwrap();
    let trash = FakeTrash::new(trash_dir.path());
    let data = config.path().join("data");
    let ctx = ExtrasCtx {
        library_root: &root,
        game_dir: &game,
        data_dir: &data,
        trash: &trash,
        now: SystemTime::UNIX_EPOCH + Duration::from_secs(1_763_035_200),
        same_volume: |_, _| true,
        available: user_available,
    };
    let listing = list(&game.join("Extras"));
    let done = create_category(&ctx, &listing, "Remezclas").unwrap();
    let job = done.job.expect("a journaled job");

    let history = st.job_history().unwrap();
    let item = history.iter().find(|h| h.job == job).unwrap();
    assert_eq!(item.kind, JobKind::Extras);
    assert_eq!(item.game.as_deref(), Some("Lantern Harbor (USA)"));
    let wire = serde_json::to_value(item).unwrap();
    assert_eq!(wire["game"], "Lantern Harbor (USA)");
}
