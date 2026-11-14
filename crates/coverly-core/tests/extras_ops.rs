//! Journaled writes inside a game's `Extras/` (extras spec D1, D3, D5, D10,
//! D14, D20, D22).

mod common;

use std::path::{Component, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use coverly_core::dedupe::testing::pin_mtime;
use coverly_core::extras::index::{list, read_index, IndexRead, Listing};
use coverly_core::extras::ops::{
    confine, create_category, forget, move_to, remove_category, rename, rename_category, set_tags,
    trash, ExtrasCtx, ExtrasError, ExtrasNote, ExtrasOutcome,
};
use coverly_core::extras::{CategoryRef, ExtrasCategory, NameRefusal, TagRefusal};
use coverly_core::write::trash::FakeTrash;
use coverly_core::write::{
    job_transfers, list_jobs, same_volume, undo_job, undo_job_with, JobKind,
};
use serde_json::json;
use tempfile::TempDir;

const GAME: &str = "Super Metroid (USA)";
const TRACK: &[u8] = b"fLaC synthetic opening";

/// 2026-11-13T12:00:00Z.
fn noon() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_794_571_200)
}

struct Fx {
    _lib: TempDir,
    data: TempDir,
    trash: FakeTrash,
    /// The library root, resolved (macOS temp folders sit behind a link).
    root: PathBuf,
    game: PathBuf,
    same_volume: fn(&std::path::Path, &std::path::Path) -> bool,
}

impl Fx {
    fn new() -> Fx {
        Fx::with_trash(FakeTrash::new)
    }

    /// A fixture whose Trash `make` builds over a folder in the data dir.
    fn with_trash(make: impl FnOnce(PathBuf) -> FakeTrash) -> Fx {
        let lib = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        let trash = make(data.path().join("fake-trash"));
        let root = lib.path().canonicalize().unwrap();
        let game = root.join("snes").join(GAME);
        std::fs::create_dir_all(&game).unwrap();
        let fx = Fx {
            _lib: lib,
            data,
            trash,
            root,
            game: game.clone(),
            same_volume,
        };
        let rom = game.join(format!("{GAME}.sfc"));
        std::fs::write(&rom, b"synthetic rom").unwrap();
        pin_mtime(&rom, 1);
        fx.put("Soundtrack/01 Opening.flac", TRACK, 2);
        fx
    }

    fn root(&self) -> PathBuf {
        self.root.clone()
    }

    fn game(&self) -> PathBuf {
        self.game.clone()
    }

    fn extras(&self) -> PathBuf {
        self.game().join("Extras")
    }

    fn index_path(&self) -> PathBuf {
        self.extras().join("extras.json")
    }

    fn put(&self, rel: &str, bytes: &[u8], n: u64) -> PathBuf {
        let path = rel.split('/').fold(self.extras(), |p, c| p.join(c));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, bytes).unwrap();
        pin_mtime(&path, n);
        path
    }

    fn ctx(&self) -> ExtrasCtx<'_> {
        ExtrasCtx {
            library_root: &self.root,
            game_dir: &self.game,
            data_dir: self.data.path(),
            trash: &self.trash,
            now: noon(),
            same_volume: self.same_volume,
        }
    }

    fn listing(&self) -> Listing {
        list(&self.extras())
    }

    fn index(&self) -> serde_json::Value {
        serde_json::from_slice(&std::fs::read(self.index_path()).unwrap()).unwrap()
    }

    fn jobs(&self) -> usize {
        list_jobs(self.data.path()).unwrap().len()
    }
}

/// The `/`-joined path as plain components.
fn parts(p: &str) -> Vec<String> {
    p.split('/')
        .fold(PathBuf::new(), |b, c| b.join(c))
        .components()
        .map(|c| match c {
            Component::Normal(n) => n.to_string_lossy().into_owned(),
            other => panic!("not a plain component: {other:?}"),
        })
        .collect()
}

fn file_index(l: &Listing, rel: &str) -> usize {
    let want = parts(rel);
    l.files
        .iter()
        .position(|f| parts(&f.path) == want)
        .unwrap_or_else(|| panic!("{rel} not listed"))
}

fn tags(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| (*s).to_string()).collect()
}

fn record<'a>(index: &'a serde_json::Value, rel: &str) -> &'a serde_json::Value {
    let want = parts(rel);
    index["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| parts(r["path"].as_str().unwrap()) == want)
        .unwrap_or_else(|| panic!("{rel} has no record in {index}"))
}

fn tag_first(fx: &Fx, t: &[&str]) -> ExtrasOutcome {
    let l = fx.listing();
    let i = file_index(&l, "Soundtrack/01 Opening.flac");
    set_tags(&fx.ctx(), &l, i, &tags(t)).unwrap()
}

#[test]
fn tagging_writes_the_index_once_and_undo_restores_no_index() {
    let fx = Fx::new();
    let out = tag_first(&fx, &["tema del jefe final"]);
    let job = out.job.clone().expect("a job");
    let idx = fx.index();
    assert_eq!(
        record(&idx, "Soundtrack/01 Opening.flac")["tags"],
        json!(["tema del jefe final"])
    );
    assert_eq!(idx["version"], 1);
    let jobs = list_jobs(fx.data.path()).unwrap();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].kind, JobKind::Extras);
    assert_eq!(jobs[0].job, job);
    // The result carries the new listing.
    let i = file_index(&out.listing, "Soundtrack/01 Opening.flac");
    assert_eq!(out.listing.files[i].record.tags, ["tema del jefe final"]);

    undo_job(fx.data.path(), &job).unwrap();
    assert!(!fx.index_path().exists());
    let track = fx.extras().join("Soundtrack").join("01 Opening.flac");
    assert_eq!(std::fs::read(&track).unwrap(), TRACK);
}

#[test]
fn a_second_write_replaces_with_backup_and_undo_restores_byte_for_byte() {
    let fx = Fx::new();
    tag_first(&fx, &["uno"]);
    let first = std::fs::read(fx.index_path()).unwrap();
    let out = tag_first(&fx, &["uno", "dos"]);
    assert_eq!(
        record(&fx.index(), "Soundtrack/01 Opening.flac")["tags"],
        json!(["uno", "dos"])
    );
    undo_job(fx.data.path(), &out.job.unwrap()).unwrap();
    assert_eq!(std::fs::read(fx.index_path()).unwrap(), first);
}

#[test]
fn a_malformed_index_is_backed_up_before_the_first_write() {
    let fx = Fx::new();
    let garbage = b"{ not json at all".to_vec();
    std::fs::write(fx.index_path(), &garbage).unwrap();
    let out = tag_first(&fx, &["jefe"]);
    let backup = fx.extras().join("extras.broken-20261113-120000.json");
    assert_eq!(std::fs::read(&backup).unwrap(), garbage);
    assert!(matches!(read_index(&fx.extras()), IndexRead::Ok(_)));
    assert_eq!(
        record(&fx.index(), "Soundtrack/01 Opening.flac")["tags"],
        json!(["jefe"])
    );
    assert!(out.notes.contains(&ExtrasNote::MalformedSaved {
        file: "extras.broken-20261113-120000.json".into()
    }));
    // The backup is neither a listed file nor a category.
    assert!(out
        .listing
        .files
        .iter()
        .all(|f| !f.name.starts_with("extras.")));

    undo_job(fx.data.path(), &out.job.unwrap()).unwrap();
    assert_eq!(std::fs::read(fx.index_path()).unwrap(), garbage);
    assert!(!backup.exists());
}

#[test]
fn salvaged_records_keep_their_tags_through_the_backup() {
    let fx = Fx::new();
    let bad = json!({"version": 1, "files": [
        {"path": "Soundtrack/01 Opening.flac", "type": "audio", "provenance": "official",
         "origin": {"how": "organize"}, "tags": ["guardada"], "size": TRACK.len(),
         "mtime_ns": (1_700_000_002_i64) * 1_000_000_000, "added_at": "2026-01-01T00:00:00Z"},
        {"nope": 1}
    ]});
    std::fs::write(fx.index_path(), serde_json::to_vec(&bad).unwrap()).unwrap();
    fx.put("Soundtrack/02 Boss.flac", b"boss", 3);
    let l = fx.listing();
    let i = file_index(&l, "Soundtrack/02 Boss.flac");
    set_tags(&fx.ctx(), &l, i, &tags(&["jefe"])).unwrap();
    let idx = fx.index();
    assert_eq!(
        record(&idx, "Soundtrack/01 Opening.flac")["tags"],
        json!(["guardada"])
    );
    assert_eq!(
        record(&idx, "Soundtrack/01 Opening.flac")["provenance"],
        "official"
    );
    assert_eq!(
        record(&idx, "Soundtrack/02 Boss.flac")["tags"],
        json!(["jefe"])
    );
}

#[test]
fn two_backups_in_one_second_get_a_numeric_suffix() {
    let fx = Fx::new();
    std::fs::write(fx.index_path(), b"first garbage").unwrap();
    tag_first(&fx, &["a"]);
    std::fs::write(fx.index_path(), b"second garbage").unwrap();
    let out = tag_first(&fx, &["b"]);
    let first = fx.extras().join("extras.broken-20261113-120000.json");
    let second = fx.extras().join("extras.broken-20261113-120000-2.json");
    assert_eq!(std::fs::read(first).unwrap(), b"first garbage");
    assert_eq!(std::fs::read(&second).unwrap(), b"second garbage");
    assert!(out.notes.contains(&ExtrasNote::MalformedSaved {
        file: "extras.broken-20261113-120000-2.json".into()
    }));
}

#[test]
fn a_newer_index_refuses_every_write() {
    let fx = Fx::new();
    let newer = br#"{"version":2,"files":[]}"#;
    std::fs::write(fx.index_path(), newer).unwrap();
    let l = fx.listing();
    let i = file_index(&l, "Soundtrack/01 Opening.flac");
    let err = set_tags(&fx.ctx(), &l, i, &tags(&["x"])).unwrap_err();
    assert!(matches!(err, ExtrasError::IndexNewer), "{err:?}");
    assert_eq!(fx.jobs(), 0);
    assert_eq!(std::fs::read(fx.index_path()).unwrap(), newer);
}

#[cfg(unix)]
#[test]
fn an_index_that_cannot_be_read_refuses_every_write() {
    let fx = Fx::new();
    let outside = fx.data.path().join("outside.json");
    std::fs::write(&outside, b"{}").unwrap();
    std::os::unix::fs::symlink(&outside, fx.index_path()).unwrap();
    let l = fx.listing();
    let i = file_index(&l, "Soundtrack/01 Opening.flac");
    let err = set_tags(&fx.ctx(), &l, i, &tags(&["x"])).unwrap_err();
    assert!(matches!(err, ExtrasError::IndexUnreadable), "{err:?}");
    assert_eq!(fx.jobs(), 0);
    assert_eq!(std::fs::read(&outside).unwrap(), b"{}");
}

#[test]
fn forget_drops_a_missing_record_and_is_undoable() {
    let fx = Fx::new();
    let gone = fx.put("Soundtrack/02 Boss.flac", b"boss!", 3);
    tag_first(&fx, &["uno"]);
    let l = fx.listing();
    let i = file_index(&l, "Soundtrack/02 Boss.flac");
    set_tags(&fx.ctx(), &l, i, &tags(&["jefe"])).unwrap();
    let before = std::fs::read(fx.index_path()).unwrap();
    std::fs::remove_file(gone).unwrap();
    let l = fx.listing();
    assert_eq!(l.missing.len(), 1);
    assert_eq!(l.missing[0].tags, ["jefe"]);
    let out = forget(&fx.ctx(), &l, 0).unwrap();
    assert!(out.listing.missing.is_empty());
    let idx = fx.index();
    assert_eq!(idx["files"].as_array().unwrap().len(), 1);
    record(&idx, "Soundtrack/01 Opening.flac");
    undo_job(fx.data.path(), &out.job.unwrap()).unwrap();
    assert_eq!(std::fs::read(fx.index_path()).unwrap(), before);
    assert_eq!(fx.listing().missing.len(), 1);
}

#[test]
fn tags_over_the_limit_are_refused_with_the_reason() {
    let fx = Fx::new();
    let l = fx.listing();
    let many: Vec<String> = (0..33).map(|i| format!("t{i}")).collect();
    let err = set_tags(&fx.ctx(), &l, 0, &many).unwrap_err();
    assert!(
        matches!(err, ExtrasError::Tags(TagRefusal::TooMany)),
        "{err:?}"
    );
    let err = set_tags(&fx.ctx(), &l, 0, &tags(&["  "])).unwrap_err();
    assert!(
        matches!(err, ExtrasError::Tags(TagRefusal::Empty)),
        "{err:?}"
    );
    assert_eq!(fx.jobs(), 0);
    assert!(!fx.index_path().exists());
}

#[test]
fn the_reconciled_view_is_what_gets_written() {
    let fx = Fx::new();
    fx.put("Soundtrack/02 Boss.flac", b"boss", 3);
    let l = fx.listing();
    let i = file_index(&l, "Soundtrack/02 Boss.flac");
    set_tags(&fx.ctx(), &l, i, &tags(&["jefe"])).unwrap();
    // Moved and renamed outside the app.
    std::fs::create_dir_all(fx.extras().join("Remixes")).unwrap();
    std::fs::rename(
        fx.extras().join("Soundtrack").join("02 Boss.flac"),
        fx.extras().join("Remixes").join("Boss (remix).flac"),
    )
    .unwrap();
    tag_first(&fx, &["apertura"]);
    let idx = fx.index();
    assert_eq!(idx["files"].as_array().unwrap().len(), 2);
    assert_eq!(
        record(&idx, "Remixes/Boss (remix).flac")["tags"],
        json!(["jefe"])
    );
    assert_eq!(
        record(&idx, "Soundtrack/01 Opening.flac")["tags"],
        json!(["apertura"])
    );
}

#[test]
fn found_files_get_explicit_records_on_a_write() {
    let fx = Fx::new();
    fx.put("Artwork/front.png", b"png", 4);
    tag_first(&fx, &["x"]);
    let idx = fx.index();
    let r = record(&idx, "Artwork/front.png");
    assert_eq!(r["provenance"], "user");
    assert_eq!(r["origin"]["how"], "found");
    assert_eq!(r["type"], "image");
    assert_eq!(r["tags"], json!([]));
    assert_eq!(r["added_at"], "2026-11-13T12:00:00Z");
}

#[test]
fn stale_listing_is_refused() {
    let fx = Fx::new();
    let l = fx.listing();
    let i = file_index(&l, "Soundtrack/01 Opening.flac");
    pin_mtime(&fx.extras().join("Soundtrack").join("01 Opening.flac"), 9);
    let err = set_tags(&fx.ctx(), &l, i, &tags(&["x"])).unwrap_err();
    assert!(matches!(err, ExtrasError::Changed), "{err:?}");
    assert_eq!(fx.jobs(), 0);
    assert!(!fx.index_path().exists());
}

#[test]
fn an_index_changed_since_the_listing_is_refused() {
    let fx = Fx::new();
    let l = fx.listing();
    tag_first(&fx, &["otra ventana"]);
    let err = set_tags(&fx.ctx(), &l, 0, &tags(&["x"])).unwrap_err();
    assert!(matches!(err, ExtrasError::Changed), "{err:?}");
    assert_eq!(
        record(&fx.index(), "Soundtrack/01 Opening.flac")["tags"],
        json!(["otra ventana"])
    );
}

#[test]
fn an_index_out_of_range_is_refused() {
    let fx = Fx::new();
    let l = fx.listing();
    assert!(matches!(
        set_tags(&fx.ctx(), &l, 7, &tags(&["x"])).unwrap_err(),
        ExtrasError::Changed
    ));
    assert!(matches!(
        forget(&fx.ctx(), &l, 0).unwrap_err(),
        ExtrasError::Changed
    ));
    assert_eq!(fx.jobs(), 0);
}

#[test]
fn a_game_folder_outside_the_library_is_refused() {
    let fx = Fx::new();
    let other = tempfile::tempdir().unwrap();
    let game = other.path().canonicalize().unwrap();
    std::fs::create_dir_all(game.join("Extras")).unwrap();
    std::fs::write(game.join("Extras").join("a.png"), b"a").unwrap();
    let ctx = ExtrasCtx {
        game_dir: &game,
        ..fx.ctx()
    };
    let l = list(&game.join("Extras"));
    let err = set_tags(&ctx, &l, 0, &tags(&["x"])).unwrap_err();
    assert!(matches!(err, ExtrasError::OutsideExtras(_)), "{err:?}");
    assert_eq!(fx.jobs(), 0);
}

#[test]
fn the_index_is_only_ever_written_inside_extras() {
    let fx = Fx::new();
    let extras = fx.extras();
    assert!(confine(&extras, &extras.join("extras.json")).is_ok());
    assert!(confine(&extras, &extras.join("Soundtrack").join("new.flac")).is_ok());
    for bad in [
        fx.root().join("snes").join("x.json"),
        fx.game().join("x.json"),
        extras.join("..").join("x.json"),
        extras
            .join("Soundtrack")
            .join("..")
            .join("..")
            .join("x.json"),
        extras.clone(),
    ] {
        assert!(
            matches!(confine(&extras, &bad), Err(ExtrasError::OutsideExtras(_))),
            "{}",
            bad.display()
        );
    }
}

#[cfg(unix)]
#[test]
fn confine_refuses_a_category_that_is_a_symlink_out_of_extras() {
    let fx = Fx::new();
    let outside = fx.data.path().join("elsewhere");
    std::fs::create_dir_all(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, fx.extras().join("Linked")).unwrap();
    let target = fx.extras().join("Linked").join("x.flac");
    assert!(matches!(
        confine(&fx.extras(), &target),
        Err(ExtrasError::OutsideExtras(_))
    ));
}

#[test]
fn a_found_record_keeps_its_added_at_on_later_writes() {
    let fx = Fx::new();
    fx.put("Artwork/front.png", b"png", 4);
    tag_first(&fx, &["x"]);
    assert_eq!(
        record(&fx.index(), "Artwork/front.png")["added_at"],
        "2026-11-13T12:00:00Z"
    );
    let later = ExtrasCtx {
        now: noon() + Duration::from_secs(86_400),
        ..fx.ctx()
    };
    let l = fx.listing();
    let i = file_index(&l, "Soundtrack/01 Opening.flac");
    set_tags(&later, &l, i, &tags(&["y"])).unwrap();
    assert_eq!(
        record(&fx.index(), "Artwork/front.png")["added_at"],
        "2026-11-13T12:00:00Z"
    );
}

#[cfg(unix)]
#[test]
fn confine_refuses_an_extras_folder_that_is_a_symlink() {
    let fx = Fx::new();
    let other = fx.root().join("snes").join("Other");
    std::fs::create_dir_all(other.join("Extras")).unwrap();
    let game = fx.root().join("snes").join("Linked game");
    std::fs::create_dir_all(&game).unwrap();
    let extras = game.join("Extras");
    std::os::unix::fs::symlink(other.join("Extras"), &extras).unwrap();
    assert!(matches!(
        confine(&extras, &extras.join("extras.json")),
        Err(ExtrasError::OutsideExtras(_))
    ));
}

// ---- Rename and move (D1, D14, D22) ----

/// The names in `Extras/<rel>` as the disk spells them.
fn names_in(fx: &Fx, rel: &str) -> Vec<String> {
    let dir = rel.split('/').fold(fx.extras(), |p, c| p.join(c));
    let mut out: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    out.sort();
    out
}

fn opening(fx: &Fx) -> (Listing, usize) {
    let l = fx.listing();
    let i = file_index(&l, "Soundtrack/01 Opening.flac");
    (l, i)
}

#[test]
fn rename_edits_the_stem_keeps_extension_and_tags() {
    let fx = Fx::new();
    tag_first(&fx, &["apertura"]);
    let (l, i) = opening(&fx);
    let out = rename(&fx.ctx(), &l, i, "  Opening theme ").unwrap();
    assert_eq!(names_in(&fx, "Soundtrack"), ["Opening theme.flac"]);
    let moved = fx.extras().join("Soundtrack").join("Opening theme.flac");
    assert_eq!(std::fs::read(&moved).unwrap(), TRACK);
    let idx = fx.index();
    assert_eq!(idx["files"].as_array().unwrap().len(), 1);
    assert_eq!(
        record(&idx, "Soundtrack/Opening theme.flac")["tags"],
        json!(["apertura"])
    );
    let j = file_index(&out.listing, "Soundtrack/Opening theme.flac");
    assert_eq!(out.listing.files[j].record.tags, ["apertura"]);
    assert!(out.listing.missing.is_empty());

    undo_job(fx.data.path(), &out.job.unwrap()).unwrap();
    assert_eq!(names_in(&fx, "Soundtrack"), ["01 Opening.flac"]);
    assert_eq!(
        record(&fx.index(), "Soundtrack/01 Opening.flac")["tags"],
        json!(["apertura"])
    );
}

#[test]
fn rename_to_a_taken_name_is_refused_never_suffixed() {
    let fx = Fx::new();
    fx.put("Soundtrack/02 Boss.flac", b"boss", 3);
    let (l, i) = opening(&fx);
    for typed in ["02 Boss", "02 BOSS"] {
        let err = rename(&fx.ctx(), &l, i, typed).unwrap_err();
        assert!(
            matches!(err, ExtrasError::Name(NameRefusal::Taken)),
            "{typed}: {err:?}"
        );
    }
    assert_eq!(fx.jobs(), 0);
    assert_eq!(
        names_in(&fx, "Soundtrack"),
        ["01 Opening.flac", "02 Boss.flac"]
    );
    assert!(!fx.index_path().exists());
}

#[test]
fn case_only_rename_goes_through_a_temporary_name() {
    let fx = Fx::new();
    fx.put("Soundtrack/boss.flac", b"boss", 3);
    let l = fx.listing();
    let i = file_index(&l, "Soundtrack/boss.flac");
    let out = rename(&fx.ctx(), &l, i, "Boss").unwrap();
    let job = out.job.unwrap();
    assert_eq!(
        names_in(&fx, "Soundtrack"),
        ["01 Opening.flac", "Boss.flac"]
    );
    let moves = job_transfers(fx.data.path(), &job).unwrap();
    assert_eq!(moves.len(), 2, "{moves:?}");
    let temp = moves[0].target.file_name().unwrap().to_string_lossy();
    assert!(temp.starts_with(".coverly-"), "{temp}");
    assert_eq!(moves[1].from, moves[0].target);
    record(&fx.index(), "Soundtrack/Boss.flac");

    undo_job(fx.data.path(), &job).unwrap();
    assert_eq!(
        names_in(&fx, "Soundtrack"),
        ["01 Opening.flac", "boss.flac"]
    );
}

#[test]
fn renaming_to_the_same_name_changes_nothing() {
    let fx = Fx::new();
    let (l, i) = opening(&fx);
    let out = rename(&fx.ctx(), &l, i, "01 Opening").unwrap();
    assert!(out.job.is_none());
    assert_eq!(fx.jobs(), 0);
    assert!(!fx.index_path().exists());
}

#[test]
fn rename_refuses_names_that_are_not_portable_or_paths_too_long() {
    let fx = Fx::new();
    let (l, i) = opening(&fx);
    let long = "x".repeat(250);
    for (typed, why) in [
        ("", NameRefusal::Empty),
        ("   ", NameRefusal::Empty),
        ("a:b", NameRefusal::NotPortable),
        ("a/b", NameRefusal::NotPortable),
        ("a?b", NameRefusal::NotPortable),
        ("CON", NameRefusal::NotPortable),
        ("tab\there", NameRefusal::NotPortable),
        (".hidden", NameRefusal::Dot),
        (long.as_str(), NameRefusal::PathTooLong),
    ] {
        let err = rename(&fx.ctx(), &l, i, typed).unwrap_err();
        assert!(
            matches!(err, ExtrasError::Name(w) if w == why),
            "{typed:?}: {err:?}"
        );
    }
    assert_eq!(fx.jobs(), 0);
    assert_eq!(names_in(&fx, "Soundtrack"), ["01 Opening.flac"]);
}

#[test]
fn rename_of_a_stale_listing_is_refused() {
    let fx = Fx::new();
    let (l, i) = opening(&fx);
    pin_mtime(&fx.extras().join("Soundtrack").join("01 Opening.flac"), 9);
    let err = rename(&fx.ctx(), &l, i, "New").unwrap_err();
    assert!(matches!(err, ExtrasError::Changed), "{err:?}");
    assert!(matches!(
        rename(&fx.ctx(), &l, 9, "New").unwrap_err(),
        ExtrasError::Changed
    ));
    assert_eq!(fx.jobs(), 0);
}

#[test]
fn move_to_another_category_creates_it_and_carries_the_record() {
    let fx = Fx::new();
    tag_first(&fx, &["jefe"]);
    let (l, i) = opening(&fx);
    let out = move_to(
        &fx.ctx(),
        &l,
        &[i],
        &CategoryRef::Fixed(ExtrasCategory::Videos),
    )
    .unwrap();
    assert_eq!(names_in(&fx, "Videos"), ["01 Opening.flac"]);
    assert!(names_in(&fx, "Soundtrack").is_empty());
    let idx = fx.index();
    assert_eq!(idx["files"].as_array().unwrap().len(), 1);
    assert_eq!(
        record(&idx, "Videos/01 Opening.flac")["tags"],
        json!(["jefe"])
    );
    let j = file_index(&out.listing, "Videos/01 Opening.flac");
    assert_eq!(
        out.listing.files[j].category,
        CategoryRef::Fixed(ExtrasCategory::Videos)
    );

    undo_job(fx.data.path(), &out.job.unwrap()).unwrap();
    assert!(!fx.extras().join("Videos").exists());
    assert_eq!(names_in(&fx, "Soundtrack"), ["01 Opening.flac"]);
    record(&fx.index(), "Soundtrack/01 Opening.flac");
}

#[test]
fn move_several_files_is_one_job() {
    let fx = Fx::new();
    fx.put("Soundtrack/02 Boss.flac", b"boss", 3);
    fx.put("Artwork/front.png", b"png", 4);
    let l = fx.listing();
    let a = file_index(&l, "Soundtrack/01 Opening.flac");
    let b = file_index(&l, "Soundtrack/02 Boss.flac");
    let c = file_index(&l, "Artwork/front.png");
    let to = CategoryRef::User("Remixes".into());
    std::fs::create_dir_all(fx.extras().join("Remixes")).unwrap();
    let l = fx.listing();
    let out = move_to(&fx.ctx(), &l, &[a, b, c, a], &to).unwrap();
    assert_eq!(fx.jobs(), 1);
    assert_eq!(
        names_in(&fx, "Remixes"),
        ["01 Opening.flac", "02 Boss.flac", "front.png"]
    );
    undo_job(fx.data.path(), &out.job.unwrap()).unwrap();
    assert_eq!(
        names_in(&fx, "Soundtrack"),
        ["01 Opening.flac", "02 Boss.flac"]
    );
    assert_eq!(names_in(&fx, "Artwork"), ["front.png"]);
    // An existing category is kept by the undo.
    assert!(fx.extras().join("Remixes").is_dir());
}

#[test]
fn move_to_a_user_category_that_does_not_exist_is_refused() {
    let fx = Fx::new();
    let (l, i) = opening(&fx);
    let err = move_to(&fx.ctx(), &l, &[i], &CategoryRef::User("Nope".into())).unwrap_err();
    assert!(matches!(err, ExtrasError::Changed), "{err:?}");
    assert_eq!(fx.jobs(), 0);
}

#[test]
fn move_into_a_taken_name_suffixes_and_says_so() {
    let fx = Fx::new();
    fx.put("Videos/01 Opening.flac", b"another one", 5);
    let (l, i) = opening(&fx);
    let out = move_to(
        &fx.ctx(),
        &l,
        &[i],
        &CategoryRef::Fixed(ExtrasCategory::Videos),
    )
    .unwrap();
    assert_eq!(
        names_in(&fx, "Videos"),
        ["01 Opening (2).flac", "01 Opening.flac"]
    );
    assert_eq!(
        std::fs::read(fx.extras().join("Videos").join("01 Opening (2).flac")).unwrap(),
        TRACK
    );
    assert!(out.notes.contains(&ExtrasNote::RenamedDup {
        name: "01 Opening (2).flac".into()
    }));
    record(&fx.index(), "Videos/01 Opening (2).flac");
}

#[test]
fn move_to_root_category_puts_the_file_directly_in_extras() {
    let fx = Fx::new();
    let (l, i) = opening(&fx);
    let out = move_to(&fx.ctx(), &l, &[i], &CategoryRef::Root).unwrap();
    assert!(fx.extras().join("01 Opening.flac").is_file());
    record(&fx.index(), "01 Opening.flac");
    let j = file_index(&out.listing, "01 Opening.flac");
    assert_eq!(out.listing.files[j].category, CategoryRef::Root);
}

#[test]
fn moving_within_its_own_category_changes_nothing() {
    let fx = Fx::new();
    let (l, i) = opening(&fx);
    let out = move_to(
        &fx.ctx(),
        &l,
        &[i],
        &CategoryRef::Fixed(ExtrasCategory::Soundtrack),
    )
    .unwrap();
    assert!(out.job.is_none());
    assert_eq!(fx.jobs(), 0);
}

#[cfg(unix)]
#[test]
fn a_symlinked_file_cannot_be_moved_or_renamed() {
    let fx = Fx::new();
    let outside = fx.data.path().join("outside.flac");
    std::fs::write(&outside, b"outside").unwrap();
    std::os::unix::fs::symlink(&outside, fx.extras().join("Soundtrack").join("link.flac")).unwrap();
    let l = fx.listing();
    let i = file_index(&l, "Soundtrack/link.flac");
    assert!(l.files[i].link);
    let err = rename(&fx.ctx(), &l, i, "other").unwrap_err();
    assert!(matches!(err, ExtrasError::Link(_)), "{err:?}");
    let err = move_to(&fx.ctx(), &l, &[i], &CategoryRef::Root).unwrap_err();
    assert!(matches!(err, ExtrasError::Link(_)), "{err:?}");
    assert_eq!(fx.jobs(), 0);
    assert_eq!(std::fs::read(&outside).unwrap(), b"outside");
}

#[cfg(unix)]
#[test]
fn a_symlinked_category_is_never_a_move_target() {
    let fx = Fx::new();
    let outside = fx.data.path().join("elsewhere");
    std::fs::create_dir_all(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, fx.extras().join("Linked")).unwrap();
    let (l, i) = opening(&fx);
    let err = move_to(&fx.ctx(), &l, &[i], &CategoryRef::User("Linked".into())).unwrap_err();
    assert!(matches!(err, ExtrasError::Link(_)), "{err:?}");
    assert_eq!(fx.jobs(), 0);
    assert!(std::fs::read_dir(&outside).unwrap().next().is_none());
}

/// The game folder's tree; the index is compared by bytes only (undo
/// restores it byte for byte by writing it again).
fn snapshot(fx: &Fx) -> std::collections::BTreeMap<Vec<String>, (Vec<u8>, i64)> {
    let mut t = common::tree(&fx.game());
    for (k, v) in &mut t {
        if k.last().is_some_and(|n| n == "extras.json") {
            v.1 = 0;
        }
    }
    t
}

#[test]
fn every_op_then_undo_restores_tree_and_json() {
    type Op = fn(&Fx) -> ExtrasOutcome;
    let ops: [(&str, Op); 5] = [
        ("tag", |fx| tag_first(fx, &["jefe"])),
        ("rename", |fx| {
            let (l, i) = opening(fx);
            rename(&fx.ctx(), &l, i, "Opening").unwrap()
        }),
        ("case-only rename", |fx| {
            let (l, i) = opening(fx);
            rename(&fx.ctx(), &l, i, "01 OPENING").unwrap()
        }),
        ("move", |fx| {
            let (l, i) = opening(fx);
            move_to(
                &fx.ctx(),
                &l,
                &[i],
                &CategoryRef::Fixed(ExtrasCategory::Guides),
            )
            .unwrap()
        }),
        ("move to root", |fx| {
            let (l, i) = opening(fx);
            move_to(&fx.ctx(), &l, &[i], &CategoryRef::Root).unwrap()
        }),
    ];
    for (what, op) in ops {
        for with_index in [false, true] {
            let fx = Fx::new();
            fx.put("Artwork/front.png", b"png", 4);
            if with_index {
                let l = fx.listing();
                let i = file_index(&l, "Artwork/front.png");
                set_tags(&fx.ctx(), &l, i, &tags(&["portada"])).unwrap();
            }
            let before = snapshot(&fx);
            let out = op(&fx);
            assert_ne!(snapshot(&fx), before, "{what}");
            undo_job(fx.data.path(), &out.job.unwrap()).unwrap();
            assert_eq!(snapshot(&fx), before, "{what} (index {with_index})");
        }
    }
}

#[test]
fn move_refuses_a_path_too_long_for_windows() {
    let fx = Fx::new();
    // A loose file whose path fits, but not once inside `Guides/` (D14,
    // spec §10: refused on every OS).
    let base = fx.extras().to_string_lossy().encode_utf16().count();
    let stem = "x".repeat(259 - base - 1 - 3 - ".flac".len());
    let name = format!("{stem}.flac");
    fx.put(&name, b"long", 3);
    let l = fx.listing();
    let i = file_index(&l, &name);
    let err = move_to(
        &fx.ctx(),
        &l,
        &[i],
        &CategoryRef::Fixed(ExtrasCategory::Guides),
    )
    .unwrap_err();
    assert!(
        matches!(err, ExtrasError::Name(NameRefusal::PathTooLong)),
        "{err:?}"
    );
    assert_eq!(fx.jobs(), 0);
    assert!(!fx.extras().join("Guides").exists());
    assert!(fx.extras().join(&name).is_file());
}

#[test]
fn a_file_named_like_the_index_is_never_moved_onto_it() {
    // A user's `Guides/extras.json` is a normal file (D4); moved to the
    // root it would become the index or be hidden as one: refused (a
    // reserved name, D1), as a rename to it is.
    for with_index in [false, true] {
        let fx = Fx::new();
        if with_index {
            tag_first(&fx, &["apertura"]);
        }
        fx.put("Guides/extras.json", b"user notes", 3);
        fx.put("Guides/EXTRAS.broken-1.json", b"more notes", 4);
        let l = fx.listing();
        let jobs = fx.jobs();
        let index = std::fs::read(fx.index_path()).ok();
        for rel in ["Guides/extras.json", "Guides/EXTRAS.broken-1.json"] {
            let i = file_index(&l, rel);
            let err = move_to(&fx.ctx(), &l, &[i], &CategoryRef::Root).unwrap_err();
            assert!(
                matches!(err, ExtrasError::Name(NameRefusal::Reserved)),
                "{rel} ({with_index}): {err:?}"
            );
        }
        assert_eq!(fx.jobs(), jobs);
        assert_eq!(std::fs::read(fx.index_path()).ok(), index);
        assert_eq!(
            names_in(&fx, "Guides"),
            ["EXTRAS.broken-1.json", "extras.json"]
        );
        // Into another category it is a normal move.
        let i = file_index(&l, "Guides/extras.json");
        let out = move_to(
            &fx.ctx(),
            &l,
            &[i],
            &CategoryRef::Fixed(ExtrasCategory::Manuals),
        )
        .unwrap();
        file_index(&out.listing, "Manuals/extras.json");
    }
}

#[test]
fn a_numbered_name_too_long_for_windows_puts_the_file_back() {
    let fx = Fx::new();
    // `Guides/<name>` fits exactly; `Guides/<name> (2)` does not.
    let guides = fx.extras().join("Guides");
    let base = guides.to_string_lossy().encode_utf16().count();
    let stem = "y".repeat(259 - base - 1 - ".flac".len());
    let name = format!("{stem}.flac");
    fx.put(&format!("Guides/{name}"), b"there", 3);
    fx.put(&name, b"loose", 4);
    let l = fx.listing();
    let i = file_index(&l, &name);
    let err = move_to(
        &fx.ctx(),
        &l,
        &[i],
        &CategoryRef::Fixed(ExtrasCategory::Guides),
    )
    .unwrap_err();
    assert!(
        matches!(err, ExtrasError::Name(NameRefusal::PathTooLong)),
        "{err:?}"
    );
    assert_eq!(std::fs::read(fx.extras().join(&name)).unwrap(), b"loose");
    assert_eq!(names_in(&fx, "Guides"), std::slice::from_ref(&name));
    assert!(!fx.index_path().exists());
}

// ---- Trash and own categories (D1, D8, D10) ----

fn user(name: &str) -> CategoryRef {
    CategoryRef::User(name.to_string())
}

fn index_bytes(fx: &Fx) -> Option<Vec<u8>> {
    std::fs::read(fx.index_path()).ok()
}

#[test]
fn trash_uses_the_trash_and_undo_brings_it_back_from_there() {
    let fx = Fx::new();
    tag_first(&fx, &["jefe"]);
    let (l, i) = opening(&fx);
    let out = trash(&fx.ctx(), &l, &[i]).unwrap();
    let track = fx.extras().join("Soundtrack").join("01 Opening.flac");
    assert!(!track.exists());
    assert_eq!(fx.trash.trashed(), 1);
    assert!(fx.index()["files"].as_array().unwrap().is_empty());
    assert!(out.notes.is_empty(), "{:?}", out.notes);
    assert!(out.listing.files.is_empty());
    assert_eq!(list_jobs(fx.data.path()).unwrap()[0].kind, JobKind::Extras);

    undo_job_with(fx.data.path(), &out.job.unwrap(), &fx.trash).unwrap();
    assert_eq!(std::fs::read(&track).unwrap(), TRACK);
    assert_eq!(
        record(&fx.index(), "Soundtrack/01 Opening.flac")["tags"],
        json!(["jefe"])
    );
}

#[test]
fn without_a_trash_the_file_is_set_aside_in_coverly_trash_on_the_same_volume() {
    let fx = Fx::with_trash(FakeTrash::unavailable);
    let (l, i) = opening(&fx);
    let out = trash(&fx.ctx(), &l, &[i]).unwrap();
    let job = out.job.clone().unwrap();
    let aside = ["snes", GAME, "Extras", "Soundtrack", "01 Opening.flac"]
        .iter()
        .fold(fx.root().join(".coverly-trash").join(&job), |p, c| {
            p.join(c)
        });
    assert_eq!(std::fs::read(&aside).unwrap(), TRACK);
    assert!(!fx
        .extras()
        .join("Soundtrack")
        .join("01 Opening.flac")
        .exists());
    assert!(out.notes.contains(&ExtrasNote::SetAside {
        folder: format!(".coverly-trash/{job}")
    }));
    assert!(fx.index()["files"].as_array().unwrap().is_empty());

    undo_job(fx.data.path(), &job).unwrap();
    assert_eq!(names_in(&fx, "Soundtrack"), ["01 Opening.flac"]);
    assert!(!fx.root().join(".coverly-trash").exists());
}

#[test]
fn set_aside_is_refused_across_volumes() {
    let mut fx = Fx::with_trash(FakeTrash::unavailable);
    fx.same_volume = |_, _| false;
    let (l, i) = opening(&fx);
    assert!(matches!(
        trash(&fx.ctx(), &l, &[i]).unwrap_err(),
        ExtrasError::NoTrash
    ));
    assert_eq!(names_in(&fx, "Soundtrack"), ["01 Opening.flac"]);
    assert!(!fx.root().join(".coverly-trash").exists());
    assert!(index_bytes(&fx).is_none());
    assert_eq!(fx.jobs(), 0);
}

#[test]
fn a_trash_failure_never_deletes() {
    let fx = Fx::with_trash(|d| FakeTrash::new(d).fail_after(0));
    tag_first(&fx, &["jefe"]);
    let (l, i) = opening(&fx);
    let out = trash(&fx.ctx(), &l, &[i]).unwrap();
    assert_eq!(names_in(&fx, "Soundtrack"), ["01 Opening.flac"]);
    assert!(out.notes.contains(&ExtrasNote::TrashFailed {
        name: "01 Opening.flac".into()
    }));
    assert_eq!(
        record(&fx.index(), "Soundtrack/01 Opening.flac")["tags"],
        json!(["jefe"])
    );
}

#[test]
fn trashing_several_files_is_one_job_and_keeps_the_others() {
    let fx = Fx::new();
    fx.put("Soundtrack/02 Boss.flac", b"boss", 3);
    fx.put("Artwork/front.png", b"png", 4);
    let l = fx.listing();
    let a = file_index(&l, "Soundtrack/01 Opening.flac");
    let b = file_index(&l, "Artwork/front.png");
    let out = trash(&fx.ctx(), &l, &[a, b, a]).unwrap();
    assert_eq!(fx.trash.trashed(), 2);
    assert_eq!(fx.jobs(), 1);
    assert_eq!(names_in(&fx, "Soundtrack"), ["02 Boss.flac"]);
    let idx = fx.index();
    assert_eq!(idx["files"].as_array().unwrap().len(), 1);
    record(&idx, "Soundtrack/02 Boss.flac");
    undo_job_with(fx.data.path(), &out.job.unwrap(), &fx.trash).unwrap();
    assert_eq!(
        names_in(&fx, "Soundtrack"),
        ["01 Opening.flac", "02 Boss.flac"]
    );
    assert_eq!(names_in(&fx, "Artwork"), ["front.png"]);
}

#[test]
fn trash_of_a_stale_listing_is_refused() {
    let fx = Fx::new();
    let (l, i) = opening(&fx);
    pin_mtime(&fx.extras().join("Soundtrack").join("01 Opening.flac"), 9);
    assert!(matches!(
        trash(&fx.ctx(), &l, &[i]).unwrap_err(),
        ExtrasError::Changed
    ));
    assert_eq!(fx.trash.trashed(), 0);
    assert_eq!(fx.jobs(), 0);
}

#[test]
fn create_category_validates_and_creates_the_folder() {
    let fx = Fx::new();
    let l = fx.listing();
    for (raw, why) in [
        ("", NameRefusal::Empty),
        (".remixes", NameRefusal::Dot),
        ("a/b", NameRefusal::NotPortable),
        ("Remixes.", NameRefusal::NotPortable),
        ("soundtrack", NameRefusal::Taken),
        ("extras.json", NameRefusal::Reserved),
    ] {
        match create_category(&fx.ctx(), &l, raw) {
            Err(ExtrasError::Name(got)) => assert_eq!(got, why, "{raw:?}"),
            other => panic!("{raw:?}: {other:?}"),
        }
    }
    assert_eq!(fx.jobs(), 0);

    let out = create_category(&fx.ctx(), &l, "  Remixes ").unwrap();
    assert!(fx.extras().join("Remixes").is_dir());
    assert_eq!(
        fx.index()["categories"],
        json!({ "Remixes": { "sync": null } })
    );
    assert!(out
        .listing
        .categories
        .iter()
        .any(|c| c.category == user("Remixes") && c.count == 0));
    // Now taken, in any case.
    assert!(matches!(
        create_category(&fx.ctx(), &out.listing, "REMIXES").unwrap_err(),
        ExtrasError::Name(NameRefusal::Taken)
    ));

    undo_job(fx.data.path(), &out.job.unwrap()).unwrap();
    assert!(!fx.extras().join("Remixes").exists());
    assert!(index_bytes(&fx).is_none());
    assert_eq!(names_in(&fx, ""), ["Soundtrack"]);
}

#[test]
fn create_category_refuses_a_name_a_loose_file_holds() {
    let fx = Fx::new();
    fx.put("remixes", b"loose", 3);
    let l = fx.listing();
    assert!(matches!(
        create_category(&fx.ctx(), &l, "Remixes").unwrap_err(),
        ExtrasError::Name(NameRefusal::Taken)
    ));
    assert_eq!(fx.jobs(), 0);
}

#[test]
fn rename_category_moves_the_folder_and_rewrites_record_paths() {
    let fx = Fx::new();
    fx.put("Remixes/boss.flac", b"boss", 3);
    fx.put("Remixes/live/encore.flac", b"encore", 4);
    let l = fx.listing();
    let i = file_index(&l, "Remixes/boss.flac");
    let l = set_tags(&fx.ctx(), &l, i, &tags(&["jefe"]))
        .unwrap()
        .listing;
    let before = std::fs::read(fx.index_path()).unwrap();

    let out = rename_category(&fx.ctx(), &l, "Remixes", "Arreglos").unwrap();
    assert!(!fx.extras().join("Remixes").exists());
    assert_eq!(names_in(&fx, "Arreglos"), ["boss.flac", "live"]);
    let idx = fx.index();
    assert_eq!(record(&idx, "Arreglos/boss.flac")["tags"], json!(["jefe"]));
    record(&idx, "Arreglos/live/encore.flac");
    assert!(idx["categories"].get("Remixes").is_none());
    assert_eq!(idx["categories"]["Arreglos"], json!({ "sync": null }));
    let j = file_index(&out.listing, "Arreglos/live/encore.flac");
    assert_eq!(out.listing.files[j].category, user("Arreglos"));

    undo_job(fx.data.path(), &out.job.unwrap()).unwrap();
    assert!(!fx.extras().join("Arreglos").exists());
    assert_eq!(names_in(&fx, "Remixes"), ["boss.flac", "live"]);
    assert_eq!(std::fs::read(fx.index_path()).unwrap(), before);
}

#[test]
fn case_only_category_rename_goes_through_a_temporary_name() {
    let fx = Fx::new();
    fx.put("remixes/boss.flac", b"boss", 3);
    let l = fx.listing();
    let out = rename_category(&fx.ctx(), &l, "remixes", "Remixes").unwrap();
    let names = names_in(&fx, "");
    assert!(names.contains(&"Remixes".to_string()), "{names:?}");
    assert!(!names.contains(&"remixes".to_string()), "{names:?}");
    assert!(
        !names.iter().any(|n| n.starts_with(".coverly-")),
        "{names:?}"
    );
    record(&fx.index(), "Remixes/boss.flac");

    undo_job(fx.data.path(), &out.job.unwrap()).unwrap();
    let names = names_in(&fx, "");
    assert!(names.contains(&"remixes".to_string()), "{names:?}");
    assert!(!names.contains(&"Remixes".to_string()), "{names:?}");
}

#[test]
fn rename_category_refuses_taken_and_unknown_names() {
    let fx = Fx::new();
    fx.put("Remixes/boss.flac", b"boss", 3);
    fx.put("Covers/a.png", b"png", 4);
    let l = fx.listing();
    for (to, why) in [
        ("covers", NameRefusal::Taken),
        ("Manuals", NameRefusal::Taken),
        ("", NameRefusal::Empty),
        ("a:b", NameRefusal::NotPortable),
    ] {
        match rename_category(&fx.ctx(), &l, "Remixes", to) {
            Err(ExtrasError::Name(got)) => assert_eq!(got, why, "{to:?}"),
            other => panic!("{to:?}: {other:?}"),
        }
    }
    assert!(matches!(
        rename_category(&fx.ctx(), &l, "Nope", "Other").unwrap_err(),
        ExtrasError::Changed
    ));
    // The same name is no job.
    assert!(rename_category(&fx.ctx(), &l, "Remixes", "Remixes")
        .unwrap()
        .job
        .is_none());
    assert_eq!(fx.jobs(), 0);
}

#[test]
fn fixed_categories_cannot_be_renamed_or_removed() {
    let fx = Fx::new();
    let l = fx.listing();
    assert!(matches!(
        rename_category(&fx.ctx(), &l, "Soundtrack", "Música").unwrap_err(),
        ExtrasError::Name(NameRefusal::Reserved)
    ));
    assert!(matches!(
        remove_category(&fx.ctx(), &l, "soundtrack").unwrap_err(),
        ExtrasError::Name(NameRefusal::Reserved)
    ));
    assert_eq!(names_in(&fx, "Soundtrack"), ["01 Opening.flac"]);
    assert_eq!(fx.jobs(), 0);
}

#[test]
fn remove_empty_category_is_dir_removed_and_undo_recreates_it() {
    let fx = Fx::new();
    std::fs::create_dir(fx.extras().join("Remixes")).unwrap();
    let l = fx.listing();
    let out = remove_category(&fx.ctx(), &l, "Remixes").unwrap();
    assert!(!fx.extras().join("Remixes").exists());
    assert!(out
        .listing
        .categories
        .iter()
        .all(|c| c.category != user("Remixes")));
    assert_eq!(fx.trash.trashed(), 0);

    undo_job(fx.data.path(), &out.job.unwrap()).unwrap();
    assert!(fx.extras().join("Remixes").is_dir());
    assert!(index_bytes(&fx).is_none());
}

#[test]
fn remove_category_with_files_trashes_them_then_removes_the_folder() {
    let fx = Fx::new();
    fx.put("Remixes/boss.flac", b"boss", 3);
    fx.put("Remixes/live/encore.flac", b"encore", 4);
    let l = tag_boss_and_record_remixes(&fx);
    let out = remove_category(&fx.ctx(), &l, "Remixes").unwrap();
    assert!(!fx.extras().join("Remixes").exists());
    assert_eq!(fx.trash.trashed(), 2);
    let idx = fx.index();
    assert_eq!(idx["files"].as_array().unwrap().len(), 1);
    record(&idx, "Soundtrack/01 Opening.flac");
    assert!(idx["categories"].get("Remixes").is_none());

    undo_job_with(fx.data.path(), &out.job.unwrap(), &fx.trash).unwrap();
    assert_eq!(names_in(&fx, "Remixes"), ["boss.flac", "live"]);
    assert_eq!(names_in(&fx, "Remixes/live"), ["encore.flac"]);
    let idx = fx.index();
    assert_eq!(record(&idx, "Remixes/boss.flac")["tags"], json!(["jefe"]));
    assert_eq!(idx["categories"]["Remixes"], json!({ "sync": null }));
}

/// Tags `Remixes/boss.flac` and gives `Remixes` an entry in the index.
fn tag_boss_and_record_remixes(fx: &Fx) -> Listing {
    let l = fx.listing();
    let i = file_index(&l, "Remixes/boss.flac");
    set_tags(&fx.ctx(), &l, i, &tags(&["jefe"])).unwrap();
    let mut file = fx.index();
    file["categories"] = json!({ "Remixes": { "sync": null } });
    std::fs::write(fx.index_path(), serde_json::to_vec_pretty(&file).unwrap()).unwrap();
    fx.listing()
}

#[test]
fn remove_category_without_a_trash_sets_its_files_aside() {
    let fx = Fx::with_trash(FakeTrash::unavailable);
    fx.put("Remixes/boss.flac", b"boss", 3);
    let l = fx.listing();
    let out = remove_category(&fx.ctx(), &l, "Remixes").unwrap();
    let job = out.job.clone().unwrap();
    assert!(!fx.extras().join("Remixes").exists());
    let aside = ["snes", GAME, "Extras", "Remixes", "boss.flac"]
        .iter()
        .fold(fx.root().join(".coverly-trash").join(&job), |p, c| {
            p.join(c)
        });
    assert_eq!(std::fs::read(&aside).unwrap(), b"boss");

    undo_job(fx.data.path(), &job).unwrap();
    assert_eq!(names_in(&fx, "Remixes"), ["boss.flac"]);
    assert!(!fx.root().join(".coverly-trash").exists());
}

#[test]
fn remove_category_with_litter_left_refuses_the_folder_removal_but_keeps_files_trashed_undoably() {
    let fx = Fx::new();
    fx.put("Remixes/boss.flac", b"boss", 3);
    fx.put("Remixes/.DS_Store", b"litter", 4);
    let l = fx.listing();
    let out = remove_category(&fx.ctx(), &l, "Remixes").unwrap();
    assert_eq!(fx.trash.trashed(), 1);
    assert_eq!(names_in(&fx, "Remixes"), [".DS_Store"]);
    assert!(out.notes.contains(&ExtrasNote::FolderKept {
        folder: "Remixes".into()
    }));

    undo_job_with(fx.data.path(), &out.job.unwrap(), &fx.trash).unwrap();
    assert_eq!(names_in(&fx, "Remixes"), [".DS_Store", "boss.flac"]);
}

#[test]
fn a_kept_category_folder_keeps_its_settings_in_the_index() {
    let fx = Fx::new();
    fx.put("Remixes/boss.flac", b"boss", 3);
    fx.put("Remixes/.DS_Store", b"litter", 4);
    tag_boss_and_record_remixes(&fx);
    let mut file = fx.index();
    file["categories"] = json!({ "Remixes": { "sync": true } });
    std::fs::write(fx.index_path(), serde_json::to_vec_pretty(&file).unwrap()).unwrap();
    let l = fx.listing();
    let out = remove_category(&fx.ctx(), &l, "Remixes").unwrap();
    assert!(out.notes.contains(&ExtrasNote::FolderKept {
        folder: "Remixes".into()
    }));
    // The folder is still there and still listed: its settings stay too.
    assert!(out
        .listing
        .categories
        .iter()
        .any(|c| c.category == user("Remixes")));
    assert_eq!(fx.index()["categories"]["Remixes"], json!({ "sync": true }));
}

/// On Windows a set-aside path past the path limit is refused before
/// anything is written, never failed half-way through the job.
#[cfg(windows)]
#[test]
fn a_set_aside_path_too_long_for_windows_is_refused_before_writing() {
    let fx = Fx::with_trash(FakeTrash::unavailable);
    // The file itself fits (255 units); `.coverly-trash\<job>\` pushes
    // its set-aside path past the limit.
    let dir_units = fx.extras().join("Remixes").as_os_str().len();
    let stem = "b".repeat(255 - dir_units - 1 - ".flac".len());
    let rel = format!("Remixes/{stem}.flac");
    fx.put(&rel, b"boss", 3);
    let l = fx.listing();
    let i = file_index(&l, &rel);
    assert!(matches!(
        trash(&fx.ctx(), &l, &[i]).unwrap_err(),
        ExtrasError::NoTrash
    ));
    assert_eq!(fx.jobs(), 0);
    assert!(fx
        .extras()
        .join("Remixes")
        .join(format!("{stem}.flac"))
        .is_file());
}

#[test]
fn trash_and_category_ops_then_undo_restore_tree_and_json() {
    type Op = fn(&Fx) -> ExtrasOutcome;
    let ops: [(&str, Op); 5] = [
        ("trash", |fx| {
            let (l, i) = opening(fx);
            trash(&fx.ctx(), &l, &[i]).unwrap()
        }),
        ("create", |fx| {
            create_category(&fx.ctx(), &fx.listing(), "Arreglos").unwrap()
        }),
        ("rename", |fx| {
            rename_category(&fx.ctx(), &fx.listing(), "Remixes", "Arreglos").unwrap()
        }),
        ("case-only rename", |fx| {
            rename_category(&fx.ctx(), &fx.listing(), "Remixes", "REMIXES").unwrap()
        }),
        ("remove", |fx| {
            remove_category(&fx.ctx(), &fx.listing(), "Remixes").unwrap()
        }),
    ];
    for (what, op) in ops {
        for trash_ok in [true, false] {
            for with_index in [false, true] {
                let fx = if trash_ok {
                    Fx::new()
                } else {
                    Fx::with_trash(FakeTrash::unavailable)
                };
                fx.put("Remixes/boss.flac", b"boss", 3);
                if with_index {
                    let l = fx.listing();
                    let i = file_index(&l, "Remixes/boss.flac");
                    set_tags(&fx.ctx(), &l, i, &tags(&["jefe"])).unwrap();
                }
                let before = snapshot(&fx);
                let lib_before = common::tree(&fx.root());
                let out = op(&fx);
                assert_ne!(snapshot(&fx), before, "{what}");
                undo_job_with(fx.data.path(), &out.job.unwrap(), &fx.trash).unwrap();
                assert_eq!(
                    snapshot(&fx),
                    before,
                    "{what} (trash {trash_ok}, index {with_index})"
                );
                let mut lib_after = common::tree(&fx.root());
                let mut lib_before = lib_before;
                for t in [&mut lib_after, &mut lib_before] {
                    t.retain(|k, _| !k.last().is_some_and(|n| n == "extras.json"));
                }
                assert_eq!(lib_after, lib_before, "{what}: nothing left beside");
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn a_set_aside_folder_that_is_a_symlink_is_never_used() {
    let fx = Fx::with_trash(FakeTrash::unavailable);
    let elsewhere = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(elsewhere.path(), fx.root().join(".coverly-trash")).unwrap();
    let (l, i) = opening(&fx);
    assert!(matches!(
        trash(&fx.ctx(), &l, &[i]).unwrap_err(),
        ExtrasError::NoTrash
    ));
    assert_eq!(names_in(&fx, "Soundtrack"), ["01 Opening.flac"]);
    assert!(std::fs::read_dir(elsewhere.path())
        .unwrap()
        .next()
        .is_none());
    assert_eq!(fx.jobs(), 0);
}
