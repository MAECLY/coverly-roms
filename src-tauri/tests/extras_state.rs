//! The Extras section over a real `AppState` (part D D3, D4, D8, D10,
//! D22): a game's `Extras/` listed by id with a revision, handles of an
//! older listing refused, nothing ever written by a read, Extras jobs
//! named after their game in the history; every write one journaled job
//! behind the one-writer guard, refused on an old revision, a read-only
//! folder or a newer index, undone by its own command, and followed by the
//! entry's new counts.

mod common;

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
use coverly_roms_lib::library::extras::{
    ExtrasAction, ExtrasUndone, ExtrasView, ExtrasWritten, Playable, Writability,
};
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

/// Writability asks the OS whether the app may write, not the mode bits:
/// an `Extras/` its owner cannot write is view only even when others may.
#[cfg(unix)]
#[test]
fn an_extras_folder_the_app_cannot_write_is_read_only() {
    use std::os::unix::fs::PermissionsExt;
    if running_as_root() {
        return;
    }
    let config = TempDir::new().unwrap();
    let st = state(&config);
    let lib = TempDir::new().unwrap();
    library(lib.path());
    let view = open(&st, lib.path());
    let id = id_of(&view, "Lantern Harbor (USA)");
    let extras = GAME
        .split('/')
        .fold(lib.path().to_path_buf(), |p, c| p.join(c))
        .join("Extras");
    fs::set_permissions(&extras, fs::Permissions::from_mode(0o577)).unwrap();
    let listed = st.extras_list(view.generation, id);
    fs::set_permissions(&extras, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(listed.unwrap().writable, Writability::ReadOnly);
}

#[cfg(unix)]
fn running_as_root() -> bool {
    std::env::var("USER").is_ok_and(|u| u == "root")
}

/// Windows ignores the read-only attribute on folders (Explorer sets it on
/// customised ones), so it never makes the extras view only.
#[cfg(windows)]
#[test]
fn a_folder_with_the_read_only_attribute_stays_writable_on_windows() {
    let config = TempDir::new().unwrap();
    let st = state(&config);
    let lib = TempDir::new().unwrap();
    library(lib.path());
    let view = open(&st, lib.path());
    let id = id_of(&view, "Lantern Harbor (USA)");
    let extras = GAME
        .split('/')
        .fold(lib.path().to_path_buf(), |p, c| p.join(c))
        .join("Extras");
    let mut perms = fs::metadata(&extras).unwrap().permissions();
    perms.set_readonly(true);
    fs::set_permissions(&extras, perms.clone()).unwrap();
    let listed = st.extras_list(view.generation, id);
    #[allow(clippy::permissions_set_readonly_false)]
    perms.set_readonly(false);
    fs::set_permissions(&extras, perms).unwrap();
    assert_eq!(listed.unwrap().writable, Writability::Yes);
}

// ---- Writes (part D D8, D10, D22) ----

/// A state whose Trash is a folder of `trash`.
fn state_with_trash(config: &TempDir, trash: &TempDir) -> AppState {
    state(config).with_trash(std::sync::Arc::new(FakeTrash::new(trash.path())))
}

/// The game folder of [`library`] below `root`.
fn game(root: &Path) -> PathBuf {
    GAME.split('/').fold(root.to_path_buf(), |p, c| p.join(c))
}

/// `rel` (`/`-separated) below the game's `Extras/`.
fn extra(root: &Path, rel: &str) -> PathBuf {
    rel.split('/')
        .fold(game(root).join("Extras"), |p, c| p.join(c))
}

/// The handle of the listed file at `rel` (compared by components).
fn index_of(view: &ExtrasView, rel: &str) -> u32 {
    view.files
        .iter()
        .find(|f| f.path.split('/').eq(rel.split('/')))
        .unwrap_or_else(|| panic!("no file {rel}"))
        .index
}

fn has(view: &ExtrasView, rel: &str) -> bool {
    view.files
        .iter()
        .any(|f| f.path.split('/').eq(rel.split('/')))
}

struct Opened {
    st: AppState,
    generation: u64,
    id: u32,
}

impl Opened {
    fn list(&self) -> ExtrasView {
        self.st.extras_list(self.generation, self.id).unwrap()
    }

    fn write(&self, rev: u64, action: ExtrasAction) -> Result<ExtrasWritten, IpcError> {
        self.st.extras_write(self.generation, self.id, rev, action)
    }

    fn undo(&self, job: &str) -> Result<ExtrasUndone, IpcError> {
        self.st.extras_undo(self.generation, self.id, job)
    }
}

fn opened(st: AppState, lib: &TempDir) -> Opened {
    library(lib.path());
    let view = open(&st, lib.path());
    let id = id_of(&view, "Lantern Harbor (USA)");
    Opened {
        st,
        generation: view.generation,
        id,
    }
}

#[test]
fn extras_rename_keeps_tags_and_undo_brings_the_name_back() {
    let config = TempDir::new().unwrap();
    let lib = TempDir::new().unwrap();
    let o = opened(state(&config), &lib);
    let before = snapshot(lib.path());
    let listed = o.list();
    let done = o
        .write(
            listed.rev,
            ExtrasAction::Rename {
                index: index_of(&listed, "Soundtrack/01 Opening.flac"),
                stem: "01 Overture".into(),
            },
        )
        .unwrap();
    let job = done.view.job.clone().expect("a journaled job");
    let listing = &done.view.listing;
    assert!(listing.rev > listed.rev);
    assert!(has(listing, "Soundtrack/01 Overture.flac"));
    assert!(!has(listing, "Soundtrack/01 Opening.flac"));
    let renamed = &listing.files[index_of(listing, "Soundtrack/01 Overture.flac") as usize];
    assert_eq!(renamed.tags, ["tema del jefe final"]);
    assert!(extra(lib.path(), "Soundtrack/01 Overture.flac").is_file());
    // The new listing is the cached one: its handles work at once.
    assert!(o
        .st
        .extras_handle(o.generation, o.id, listing.rev, 0)
        .is_ok());

    let undone = o.undo(&job).unwrap();
    assert_eq!(undone.job, job);
    assert!(has(&undone.listing, "Soundtrack/01 Opening.flac"));
    assert!(extra(lib.path(), "Soundtrack/01 Opening.flac").is_file());
    assert!(!extra(lib.path(), "Soundtrack/01 Overture.flac").exists());
    // Byte for byte: the index too (only mtimes of folders may move).
    let files = |s: BTreeMap<PathBuf, (bool, u64, SystemTime)>| {
        s.into_iter()
            .filter(|(_, (dir, _, _))| !dir)
            .map(|(p, (_, len, _))| (p, len))
            .collect::<Vec<_>>()
    };
    assert_eq!(files(snapshot(lib.path())), files(before));
    assert!(fs::read_to_string(extra(lib.path(), "extras.json"))
        .unwrap()
        .contains("01 Opening.flac"));
}

#[test]
fn extras_move_files_a_loose_file_into_a_category() {
    let config = TempDir::new().unwrap();
    let lib = TempDir::new().unwrap();
    let o = opened(state(&config), &lib);
    let listed = o.list();
    let done = o
        .write(
            listed.rev,
            ExtrasAction::Move {
                files: vec![
                    index_of(&listed, "readme.txt"),
                    index_of(&listed, "Remixes/boss.mp3"),
                ],
                to: CategoryRef::Fixed(ExtrasCategory::Guides),
            },
        )
        .unwrap();
    assert!(done.view.job.is_some());
    assert!(has(&done.view.listing, "Guides/readme.txt"));
    assert!(has(&done.view.listing, "Guides/boss.mp3"));
    assert!(extra(lib.path(), "Guides/readme.txt").is_file());
    assert!(!extra(lib.path(), "readme.txt").exists());
}

#[test]
fn extras_trash_sends_files_to_the_trash_and_undo_brings_them_back() {
    let config = TempDir::new().unwrap();
    let lib = TempDir::new().unwrap();
    let bin = TempDir::new().unwrap();
    let o = opened(state_with_trash(&config, &bin), &lib);
    let listed = o.list();
    let done = o
        .write(
            listed.rev,
            ExtrasAction::Trash {
                files: vec![index_of(&listed, "Artwork/front.png")],
            },
        )
        .unwrap();
    let job = done.view.job.clone().unwrap();
    assert!(!has(&done.view.listing, "Artwork/front.png"));
    assert!(!extra(lib.path(), "Artwork/front.png").exists());
    assert!(
        snapshot(bin.path())
            .keys()
            .any(|p| p.file_name().is_some_and(|n| n == "front.png")),
        "in the Trash, not deleted"
    );
    let undone = o.undo(&job).unwrap();
    assert!(has(&undone.listing, "Artwork/front.png"));
    assert_eq!(
        fs::read(extra(lib.path(), "Artwork/front.png")).unwrap(),
        [3u8; 16]
    );
}

#[test]
fn extras_set_tags_writes_the_index_in_a_job() {
    let config = TempDir::new().unwrap();
    let lib = TempDir::new().unwrap();
    let o = opened(state(&config), &lib);
    let listed = o.list();
    let done = o
        .write(
            listed.rev,
            ExtrasAction::SetTags {
                index: index_of(&listed, "Remixes/boss.mp3"),
                tags: vec!["jefe".into(), " remix ".into()],
            },
        )
        .unwrap();
    let listing = &done.view.listing;
    let boss = &listing.files[index_of(listing, "Remixes/boss.mp3") as usize];
    assert_eq!(boss.tags, ["jefe", "remix"]);
    let index = fs::read_to_string(extra(lib.path(), "extras.json")).unwrap();
    assert!(index.contains("\"remix\""), "{index}");
    // Refused tags never start a job and reach the webview as a reason.
    let too_many: Vec<String> = (0..40).map(|i| format!("t{i}")).collect();
    assert_eq!(
        o.write(
            listing.rev,
            ExtrasAction::SetTags {
                index: 0,
                tags: too_many
            }
        )
        .unwrap_err(),
        IpcError::ExtrasRefused {
            why: "tags-too-many".into()
        }
    );
}

#[test]
fn extras_forget_drops_a_missing_record_until_undone() {
    let config = TempDir::new().unwrap();
    let lib = TempDir::new().unwrap();
    let o = opened(state(&config), &lib);
    let listed = o.list();
    assert_eq!(listed.missing.len(), 1);
    let done = o
        .write(listed.rev, ExtrasAction::Forget { missing: 0 })
        .unwrap();
    assert!(done.view.listing.missing.is_empty());
    let undone = o.undo(&done.view.job.unwrap()).unwrap();
    assert_eq!(undone.listing.missing.len(), 1);
    assert_eq!(undone.listing.missing[0].tags, ["intro"]);
}

#[test]
fn extras_categories_are_created_renamed_and_removed() {
    let config = TempDir::new().unwrap();
    let lib = TempDir::new().unwrap();
    let bin = TempDir::new().unwrap();
    let o = opened(state_with_trash(&config, &bin), &lib);

    let listed = o.list();
    let made = o
        .write(
            listed.rev,
            ExtrasAction::CategoryCreate {
                name: "Remezclas".into(),
            },
        )
        .unwrap();
    assert!(extra(lib.path(), "Remezclas").is_dir());
    assert!(made
        .view
        .listing
        .categories
        .iter()
        .any(|c| c.category == CategoryRef::User("Remezclas".into())));
    // A taken name is a reason, never prose.
    assert_eq!(
        o.write(
            made.view.listing.rev,
            ExtrasAction::CategoryCreate {
                name: "remixes".into()
            }
        )
        .unwrap_err(),
        IpcError::ExtrasRefused {
            why: "taken".into()
        }
    );

    let renamed = o
        .write(
            made.view.listing.rev,
            ExtrasAction::CategoryRename {
                from: "Remixes".into(),
                to: "Mezclas".into(),
            },
        )
        .unwrap();
    assert!(has(&renamed.view.listing, "Mezclas/boss.mp3"));
    assert!(extra(lib.path(), "Mezclas/boss.mp3").is_file());

    let removed = o
        .write(
            renamed.view.listing.rev,
            ExtrasAction::CategoryRemove {
                name: "Mezclas".into(),
            },
        )
        .unwrap();
    assert!(!extra(lib.path(), "Mezclas").exists());
    assert!(!has(&removed.view.listing, "Mezclas/boss.mp3"));
    assert!(snapshot(bin.path())
        .keys()
        .any(|p| p.file_name().is_some_and(|n| n == "boss.mp3")));
    let undone = o.undo(&removed.view.job.unwrap()).unwrap();
    assert!(has(&undone.listing, "Mezclas/boss.mp3"));
}

#[test]
fn writes_are_refused_while_a_job_runs() {
    let config = TempDir::new().unwrap();
    let lib = TempDir::new().unwrap();
    let o = opened(state(&config), &lib);
    let listed = o.list();
    let before = snapshot(lib.path());
    {
        let _writing = o.st.jobs().begin_manual().unwrap();
        assert_eq!(
            o.write(listed.rev, ExtrasAction::Forget { missing: 0 })
                .unwrap_err(),
            IpcError::JobRunning
        );
    }
    assert_eq!(snapshot(lib.path()), before);
    // The guard is released after a write, refused or not.
    o.write(listed.rev, ExtrasAction::Forget { missing: 0 })
        .unwrap();
    assert!(o.st.jobs().begin_manual().is_ok());
}

#[test]
fn writes_are_refused_while_a_job_is_paused() {
    use common::fill::{fill_world, request, wait_for, Recorder, ADVENTURE_USA};
    use coverly_core::net::testing::MockResponse;
    use coverly_core::net::NetConfig;
    use coverly_roms_lib::jobs::view::JobState;

    let w = fill_world();
    let serial = FillConfig {
        net: NetConfig {
            concurrency: 1,
            retries: 0,
            ..NetConfig::default()
        },
        ..(*w.cfg).clone()
    };
    let st = AppState::new(w.dirs.path().to_path_buf(), serial);
    common::fast(&st);
    let view = st
        .open(w.lib.path(), None, &SystemDb::builtin(), &mut |_| {})
        .unwrap();
    st.update_settings(&json!({"network": {"concurrency": 1}}))
        .unwrap();
    let rec = std::sync::Arc::new(Recorder::default());
    let id = st
        .start_fill(view.generation, &request(None), rec.clone())
        .unwrap()
        .id;
    wait_for(&rec, JobState::Planned);
    w.server.mock(
        ADVENTURE_USA,
        MockResponse::status(429).header("Retry-After", "0"),
    );
    st.jobs().run(id, rec.clone()).unwrap();
    wait_for(&rec, JobState::Paused);
    // A paused job resumes into its journal: nothing may write meanwhile.
    assert_eq!(
        st.extras_write(view.generation, 0, 1, ExtrasAction::Forget { missing: 0 })
            .unwrap_err(),
        IpcError::JobRunning
    );
    assert_eq!(
        st.extras_undo(view.generation, 0, "20261114T120000Z-1bfb-0000")
            .unwrap_err(),
        IpcError::JobRunning
    );
    st.jobs().cancel(id).unwrap();
}

#[cfg(unix)]
#[test]
fn writes_are_refused_on_a_read_only_volume() {
    use std::os::unix::fs::PermissionsExt;
    if running_as_root() {
        return;
    }
    let config = TempDir::new().unwrap();
    let lib = TempDir::new().unwrap();
    let o = opened(state(&config), &lib);
    let extras = game(lib.path()).join("Extras");
    fs::set_permissions(&extras, fs::Permissions::from_mode(0o555)).unwrap();
    let listed = o.list();
    let refused = o.write(
        listed.rev,
        ExtrasAction::CategoryCreate {
            name: "Remezclas".into(),
        },
    );
    fs::set_permissions(&extras, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        matches!(refused, Err(IpcError::WriteDenied(_))),
        "{refused:?}"
    );
    assert!(!extra(lib.path(), "Remezclas").exists());
}

#[cfg(unix)]
#[test]
fn undo_is_refused_on_a_read_only_volume() {
    use std::os::unix::fs::PermissionsExt;
    if running_as_root() {
        return;
    }
    let config = TempDir::new().unwrap();
    let lib = TempDir::new().unwrap();
    let o = opened(state(&config), &lib);
    let listed = o.list();
    let done = o
        .write(
            listed.rev,
            ExtrasAction::CategoryCreate {
                name: "Remezclas".into(),
            },
        )
        .unwrap();
    let job = done.view.job.unwrap();
    let extras = game(lib.path()).join("Extras");
    fs::set_permissions(&extras, fs::Permissions::from_mode(0o555)).unwrap();
    let refused = o.undo(&job);
    fs::set_permissions(&extras, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        matches!(refused, Err(IpcError::WriteDenied(_))),
        "{refused:?}"
    );
    assert!(extra(lib.path(), "Remezclas").is_dir());
    // Still undoable once the folder can be written again.
    o.undo(&job).unwrap();
    assert!(!extra(lib.path(), "Remezclas").exists());
}

#[test]
fn writes_are_refused_with_a_newer_index() {
    let config = TempDir::new().unwrap();
    let lib = TempDir::new().unwrap();
    let o = opened(state(&config), &lib);
    let newer = br#"{ "version": 9, "files": [] }"#;
    put(lib.path(), &format!("{GAME}/Extras/extras.json"), newer);
    let listed = o.list();
    let before = snapshot(lib.path());
    assert_eq!(
        o.write(
            listed.rev,
            ExtrasAction::SetTags {
                index: 0,
                tags: vec!["x".into()]
            }
        )
        .unwrap_err(),
        IpcError::ExtrasNewer
    );
    assert_eq!(snapshot(lib.path()), before);
    assert_eq!(fs::read(extra(lib.path(), "extras.json")).unwrap(), newer);
}

#[test]
fn stale_rev_is_refused_with_extras_changed() {
    let config = TempDir::new().unwrap();
    let lib = TempDir::new().unwrap();
    let o = opened(state(&config), &lib);
    let old = o.list();
    let new = o.list();
    let rename = |rev| {
        o.write(
            rev,
            ExtrasAction::Rename {
                index: 0,
                stem: "renamed".into(),
            },
        )
    };
    assert_eq!(rename(old.rev).unwrap_err(), IpcError::ExtrasChanged);
    // Files changed on disk since the newest listing: refused as well.
    put(
        lib.path(),
        &format!("{GAME}/Extras/Artwork/back.png"),
        &[9u8; 4],
    );
    assert_eq!(rename(new.rev).unwrap_err(), IpcError::ExtrasChanged);
    assert!(!snapshot(lib.path())
        .keys()
        .any(|p| p.file_stem().is_some_and(|s| s == "renamed")));
    // An entry that is no game folder has nothing to write.
    let gba =
        o.st.session(o.generation)
            .unwrap()
            .entries
            .iter()
            .position(|e| e.rom.stem == "Moss Garden (USA)")
            .unwrap() as u32;
    let bare = o.st.extras_list(o.generation, gba).unwrap();
    assert!(matches!(
        o.st.extras_write(
            o.generation,
            gba,
            bare.rev,
            ExtrasAction::CategoryCreate { name: "X".into() }
        ),
        Err(IpcError::WriteDenied(_))
    ));
}

#[test]
fn entries_updated_is_emitted_with_the_new_extras_count() {
    let config = TempDir::new().unwrap();
    let lib = TempDir::new().unwrap();
    let bin = TempDir::new().unwrap();
    let o = opened(state_with_trash(&config, &bin), &lib);
    let listed = o.list();
    let done = o
        .write(
            listed.rev,
            ExtrasAction::Trash {
                files: vec![
                    index_of(&listed, "readme.txt"),
                    index_of(&listed, "Themes/harbor.p3t"),
                ],
            },
        )
        .unwrap();
    let event = done.updated.expect("an entries-updated event");
    assert_eq!(event.generation, o.generation);
    assert_eq!(event.entries.len(), 1);
    assert_eq!(event.entries[0].id, o.id);
    let addons = event.entries[0].addons.expect("the new counts");
    assert_eq!(addons.extras, 3, "five files, two sent to the Trash");
    // The session keeps the new count for the next view.
    let session = o.st.session(o.generation).unwrap();
    let counts = session.entries[o.id as usize].rom.fixed_counts.unwrap();
    assert_eq!(counts.extras, 3);
    let wire = serde_json::to_value(&event).unwrap();
    assert_eq!(wire["entries"][0]["addons"]["extras"], 3);

    let undone = o.undo(&done.view.job.unwrap()).unwrap();
    let back = undone.updated.expect("undo refreshes the count too");
    assert_eq!(back.entries[0].addons.unwrap().extras, 5);
}

#[test]
fn undo_last_job_still_ignores_extras_jobs() {
    let config = TempDir::new().unwrap();
    let lib = TempDir::new().unwrap();
    let o = opened(state(&config), &lib);
    let listed = o.list();
    o.write(
        listed.rev,
        ExtrasAction::CategoryCreate {
            name: "Remezclas".into(),
        },
    )
    .unwrap();
    assert_eq!(
        o.st.undo_last_job(o.generation).unwrap_err(),
        IpcError::NothingToUndo
    );
    assert!(extra(lib.path(), "Remezclas").is_dir());
}

#[test]
fn extras_undo_refuses_a_job_of_another_game_or_kind() {
    let config = TempDir::new().unwrap();
    let lib = TempDir::new().unwrap();
    let o = opened(state(&config), &lib);
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
    let view = open(&o.st, lib.path());
    let o = Opened {
        st: o.st,
        generation: view.generation,
        id: id_of(&view, "Lantern Harbor (USA)"),
    };
    let kite = id_of(&view, "Kite Days (USA)");
    let listed = o.list();
    let done = o
        .write(
            listed.rev,
            ExtrasAction::CategoryCreate {
                name: "Remezclas".into(),
            },
        )
        .unwrap();
    let job = done.view.job.unwrap();
    assert!(o.st.extras_list(o.generation, kite).unwrap().game_folder);
    assert_eq!(
        o.st.extras_undo(o.generation, kite, &job).unwrap_err(),
        IpcError::NothingToUndo
    );
    assert_eq!(o.undo("not-a-job").unwrap_err(), IpcError::NothingToUndo);
    assert!(extra(lib.path(), "Remezclas").is_dir());
    o.undo(&job).unwrap();
    assert!(!extra(lib.path(), "Remezclas").exists());
    assert_eq!(o.undo(&job).unwrap_err(), IpcError::NothingToUndo);
}

// ---- Adds from a picker or an OS drop (part D D11, D13, P-5) ----

mod adds {
    use std::sync::mpsc::{channel, Receiver, Sender};
    use std::sync::{Arc, Mutex};

    use coverly_core::consoles::{start_open_log, take_open_log};
    use coverly_core::extras::ops::ExtrasNote;
    use coverly_roms_lib::jobs::view::EntriesUpdatedEvent;
    use coverly_roms_lib::library::extras::{
        AddChoice, AddEnd, AddFinishedEvent, AddProgressEvent, AddStarted, DropPosition,
        ExtrasAddEvents, ExtrasWriteView, Picker,
    };

    use super::*;

    struct Picks(Vec<PathBuf>);

    impl Picker for Picks {
        fn pick_files(&self) -> Option<Vec<PathBuf>> {
            Some(self.0.clone())
        }
    }

    struct NoPick;

    impl Picker for NoPick {
        fn pick_files(&self) -> Option<Vec<PathBuf>> {
            None
        }
    }

    /// Records a background add; with `hold_at`, the copy waits after
    /// that file until the test lets it go.
    struct Recorder {
        progress: Mutex<Vec<AddProgressEvent>>,
        updated: Mutex<Vec<EntriesUpdatedEvent>>,
        finished: Mutex<Sender<AddFinishedEvent>>,
        hold_at: Option<u32>,
        reached: Mutex<Sender<()>>,
        release: Mutex<Receiver<()>>,
    }

    struct Handles {
        rec: Arc<Recorder>,
        finished: Receiver<AddFinishedEvent>,
        reached: Receiver<()>,
        release: Sender<()>,
    }

    fn recorder(hold_at: Option<u32>) -> Handles {
        let (fin_tx, finished) = channel();
        let (reach_tx, reached) = channel();
        let (release, rel_rx) = channel();
        Handles {
            rec: Arc::new(Recorder {
                progress: Mutex::default(),
                updated: Mutex::default(),
                finished: Mutex::new(fin_tx),
                hold_at,
                reached: Mutex::new(reach_tx),
                release: Mutex::new(rel_rx),
            }),
            finished,
            reached,
            release,
        }
    }

    impl ExtrasAddEvents for Recorder {
        fn progress(&self, event: AddProgressEvent) {
            let i = event.i;
            self.progress.lock().unwrap().push(event);
            if Some(i) == self.hold_at {
                self.reached.lock().unwrap().send(()).unwrap();
                self.release.lock().unwrap().recv().unwrap();
            }
        }

        fn finished(&self, event: AddFinishedEvent) {
            self.finished.lock().unwrap().send(event).unwrap();
        }

        fn entries_updated(&self, event: EntriesUpdatedEvent) {
            self.updated.lock().unwrap().push(event);
        }
    }

    fn wait(rx: &Receiver<AddFinishedEvent>) -> AddFinishedEvent {
        rx.recv_timeout(Duration::from_secs(60))
            .expect("the add ends")
    }

    fn done(started: AddStarted) -> ExtrasWriteView {
        match started {
            AddStarted::Done { write } => write,
            other => panic!("not done at once: {other:?}"),
        }
    }

    /// Source files outside the library, pinned.
    fn sources(dir: &TempDir, names: &[&str]) -> Vec<PathBuf> {
        names
            .iter()
            .map(|n| put(dir.path(), n, n.as_bytes()))
            .collect()
    }

    fn many(dir: &TempDir, n: usize) -> Vec<PathBuf> {
        let names: Vec<String> = (0..n).map(|i| format!("scan {i:02}.txt")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        sources(dir, &refs)
    }

    fn all(n: usize, category: CategoryRef) -> Vec<AddChoice> {
        (0..n)
            .map(|i| AddChoice {
                index: u32::try_from(i).unwrap(),
                category: category.clone(),
            })
            .collect()
    }

    fn position() -> DropPosition {
        DropPosition { x: 10.0, y: 20.0 }
    }

    #[test]
    fn the_picker_runs_off_the_main_thread() {
        // The command runs the picker on a blocking worker; the state
        // only ever sees what the picker answers, through this seam.
        let config = TempDir::new().unwrap();
        let lib = TempDir::new().unwrap();
        let src = TempDir::new().unwrap();
        let o = opened(state(&config), &lib);
        let picked = sources(
            &src,
            &["01 Theme.mp3", "manual.pdf", "prod.keys", ".DS_Store"],
        );
        let before = snapshot(lib.path());
        assert_eq!(
            o.st.extras_add_pick(o.generation, o.id, &NoPick).unwrap(),
            None
        );

        start_open_log();
        let plan =
            o.st.extras_add_pick(o.generation, o.id, &Picks(picked.clone()))
                .unwrap()
                .expect("a plan");
        let opened = take_open_log();
        assert!(
            !opened.iter().any(|p| p.ends_with("prod.keys")),
            "{opened:?}"
        );
        assert_eq!(snapshot(lib.path()), before, "a plan never writes");
        assert_eq!(plan.entry_id, o.id);
        let names: Vec<(&str, ExtrasCategory)> = plan
            .files
            .iter()
            .map(|f| (f.name.as_str(), f.category))
            .collect();
        assert_eq!(
            names,
            [
                ("01 Theme.mp3", ExtrasCategory::Soundtrack),
                ("manual.pdf", ExtrasCategory::Manuals),
            ]
        );
        assert_eq!((plan.keys, plan.hidden, plan.too_many), (1, 1, false));
        assert_eq!(plan.total, 12 + 10);
        let json = serde_json::to_string(&plan).unwrap();
        assert!(
            !json.contains(src.path().to_string_lossy().as_ref()),
            "{json}"
        );

        let (h, listed) = (recorder(None), o.list());
        let choices = [
            AddChoice {
                index: 0,
                category: CategoryRef::Fixed(ExtrasCategory::Soundtrack),
            },
            AddChoice {
                index: 1,
                category: CategoryRef::User("Escaneos".into()),
            },
        ];
        let write = done(
            o.st.extras_add_commit(
                o.generation,
                o.id,
                listed.rev,
                plan.plan,
                &choices,
                h.rec.clone(),
            )
            .unwrap(),
        );
        assert!(has(&write.listing, "Soundtrack/01 Theme.mp3"));
        assert!(has(&write.listing, "Escaneos/manual.pdf"));
        assert!(picked[0].is_file(), "a copy, never a move");
        assert!(write.notes.contains(&ExtrasNote::KeysSkipped { n: 1 }));
        assert!(write.notes.contains(&ExtrasNote::HiddenSkipped { n: 1 }));
        assert!(!extra(lib.path(), "prod.keys").exists());
        assert_eq!(h.rec.updated.lock().unwrap().len(), 1);
        assert!(h.rec.progress.lock().unwrap().is_empty(), "a small add");

        // A plan is committed once.
        assert!(matches!(
            o.st.extras_add_commit(
                o.generation,
                o.id,
                write.listing.rev,
                plan.plan,
                &choices,
                h.rec.clone(),
            ),
            Err(IpcError::BadRequest(_))
        ));
        o.undo(&write.job.unwrap()).unwrap();
        assert!(!extra(lib.path(), "Soundtrack/01 Theme.mp3").exists());
        assert!(!extra(lib.path(), "Escaneos").exists());
    }

    #[test]
    fn a_plan_of_another_entry_or_an_old_rev_is_refused() {
        let config = TempDir::new().unwrap();
        let lib = TempDir::new().unwrap();
        let src = TempDir::new().unwrap();
        let o = opened(state(&config), &lib);
        let picked = sources(&src, &["a.mp3"]);
        let h = recorder(None);
        let plan =
            o.st.extras_add_pick(o.generation, o.id, &Picks(picked.clone()))
                .unwrap()
                .unwrap();
        let old = o.list();
        let new = o.list();
        let soundtrack = all(1, CategoryRef::Fixed(ExtrasCategory::Soundtrack));
        assert_eq!(
            o.st.extras_add_commit(
                o.generation,
                o.id,
                old.rev,
                plan.plan,
                &soundtrack,
                h.rec.clone()
            )
            .unwrap_err(),
            IpcError::ExtrasChanged
        );
        let gba = 1 - o.id.min(1);
        assert!(o
            .st
            .extras_add_commit(
                o.generation,
                gba,
                new.rev,
                plan.plan,
                &soundtrack,
                h.rec.clone()
            )
            .is_err());
        assert!(!extra(lib.path(), "Soundtrack/a.mp3").exists());
        // A choice outside the plan is refused before anything is copied.
        assert!(matches!(
            o.st.extras_add_commit(
                o.generation,
                o.id,
                new.rev,
                plan.plan,
                &all(2, CategoryRef::Root),
                h.rec.clone()
            ),
            Err(IpcError::BadRequest(_))
        ));
        assert!(!extra(lib.path(), "a.mp3").exists());
    }

    #[test]
    fn extras_add_dropped_copies_only_the_recorded_paths() {
        let config = TempDir::new().unwrap();
        let lib = TempDir::new().unwrap();
        let src = TempDir::new().unwrap();
        let o = opened(state(&config), &lib);
        let files = sources(&src, &["a.mp3", "b.mp3", "Scans/c.png", "Scans/x.rap"]);
        start_open_log();
        let event =
            o.st.extras_drop(vec![files[0].clone(), src.path().join("Scans")], position());
        assert_eq!(event.items.len(), 2);
        let h = recorder(None);
        let listed = o.list();
        let write = done(
            o.st.extras_add_dropped(
                o.generation,
                o.id,
                listed.rev,
                event.token,
                CategoryRef::Fixed(ExtrasCategory::Artwork),
                h.rec.clone(),
            )
            .unwrap(),
        );
        let opened = take_open_log();
        assert!(!opened.iter().any(|p| p.ends_with("x.rap")), "{opened:?}");
        assert!(has(&write.listing, "Artwork/a.mp3"));
        assert!(has(&write.listing, "Artwork/c.png"));
        assert!(!has(&write.listing, "Artwork/b.mp3"), "never dropped");
        assert!(!extra(lib.path(), "Artwork/x.rap").exists());
        assert!(write.notes.contains(&ExtrasNote::KeysSkipped { n: 1 }));
        // The token is spent.
        assert_eq!(
            o.st.extras_add_dropped(
                o.generation,
                o.id,
                write.listing.rev,
                event.token,
                CategoryRef::Root,
                h.rec.clone(),
            )
            .unwrap_err(),
            IpcError::ExtrasRefused {
                why: "drop-expired".into()
            }
        );
        // A drop on no category asks which one: the plan of that drop.
        let other = o.st.extras_drop(vec![files[1].clone()], position());
        let plan =
            o.st.extras_add_plan(o.generation, o.id, other.token)
                .unwrap();
        assert_eq!(plan.files.len(), 1);
        assert_eq!(plan.files[0].name, "b.mp3");
        assert_eq!(plan.files[0].category, ExtrasCategory::Soundtrack);
        assert_eq!(
            o.st.extras_add_plan(o.generation, o.id, other.token)
                .unwrap_err(),
            IpcError::ExtrasRefused {
                why: "drop-expired".into()
            }
        );
    }

    #[test]
    fn add_progress_events_and_cancel_keep_what_was_copied() {
        let config = TempDir::new().unwrap();
        let lib = TempDir::new().unwrap();
        let src = TempDir::new().unwrap();
        let o = opened(state(&config), &lib);
        let n = 51;
        let picked = many(&src, n);
        let plan =
            o.st.extras_add_pick(o.generation, o.id, &Picks(picked))
                .unwrap()
                .unwrap();
        let h = recorder(Some(1));
        let listed = o.list();
        let started =
            o.st.extras_add_commit(
                o.generation,
                o.id,
                listed.rev,
                plan.plan,
                &all(n, CategoryRef::Fixed(ExtrasCategory::Guides)),
                h.rec.clone(),
            )
            .unwrap();
        let AddStarted::Background {
            add,
            n: count,
            total,
        } = started
        else {
            panic!("more than 50 files run in the background: {started:?}");
        };
        assert_eq!((add, count), (plan.plan, 51));
        assert_eq!(total, plan.total);
        h.reached
            .recv_timeout(Duration::from_secs(60))
            .expect("the first file is copied");
        assert!(o.st.extras_add_cancel(add));
        assert!(!o.st.extras_add_cancel(add + 100), "no such add");
        h.release.send(()).unwrap();
        let end = wait(&h.finished);
        assert_eq!((end.entry_id, end.add), (o.id, add));
        let AddEnd::Done { write } = end.end else {
            panic!("{:?}", end.end);
        };
        assert!(write.notes.contains(&ExtrasNote::Cancelled { copied: 1 }));
        let copied: Vec<_> = write
            .listing
            .files
            .iter()
            .filter(|f| f.category == CategoryRef::Fixed(ExtrasCategory::Guides))
            .collect();
        assert_eq!(copied.len(), 1, "{copied:?}");
        let progress = h.rec.progress.lock().unwrap().clone();
        assert_eq!(
            progress.iter().map(|p| p.i).collect::<Vec<_>>(),
            [0, 1],
            "before the first file and after each"
        );
        assert!(progress
            .iter()
            .all(|p| p.entry_id == o.id && p.add == add && p.n == 51 && p.total == total));
        assert_eq!(h.rec.updated.lock().unwrap().len(), 1);
        // The kept copy is one undoable job; the guard is free again.
        assert!(!o.st.extras_add_cancel(add), "it ended");
        let undone = o.undo(&write.job.unwrap()).unwrap();
        assert!(!undone
            .listing
            .files
            .iter()
            .any(|f| f.category == CategoryRef::Fixed(ExtrasCategory::Guides)));
    }

    #[test]
    fn a_big_add_runs_in_the_background_and_other_writes_see_busy() {
        let config = TempDir::new().unwrap();
        let lib = TempDir::new().unwrap();
        let src = TempDir::new().unwrap();
        let o = opened(state(&config), &lib);
        let n = 51;
        let picked = many(&src, n);
        let h = recorder(Some(2));
        let event = o.st.extras_drop(picked, position());
        let listed = o.list();
        let started =
            o.st.extras_add_dropped(
                o.generation,
                o.id,
                listed.rev,
                event.token,
                CategoryRef::User("Escaneos".into()),
                h.rec.clone(),
            )
            .unwrap();
        let AddStarted::Background { add, .. } = started else {
            panic!("{started:?}");
        };
        h.reached.recv_timeout(Duration::from_secs(60)).unwrap();
        // The library stays usable for reads; writes see the busy reason.
        let during = o.list();
        assert_eq!(during.writable, Writability::Busy);
        assert_eq!(
            o.write(during.rev, ExtrasAction::Forget { missing: 0 })
                .unwrap_err(),
            IpcError::JobRunning
        );
        let other = recorder(None);
        let again =
            o.st.extras_drop(vec![src.path().join("scan 00.txt")], position());
        assert_eq!(
            o.st.extras_add_dropped(
                o.generation,
                o.id,
                during.rev,
                again.token,
                CategoryRef::Root,
                other.rec.clone(),
            )
            .unwrap_err(),
            IpcError::JobRunning
        );
        h.release.send(()).unwrap();
        let end = wait(&h.finished);
        assert_eq!(end.add, add);
        let AddEnd::Done { write } = end.end else {
            panic!("{:?}", end.end);
        };
        assert!(write.job.is_some());
        assert_eq!(
            write
                .listing
                .files
                .iter()
                .filter(|f| f.category == CategoryRef::User("Escaneos".into()))
                .count(),
            n
        );
        assert_eq!(h.rec.progress.lock().unwrap().last().unwrap().i, 51);
        assert!(o.st.jobs().begin_manual().is_ok(), "the guard is released");
    }
}
