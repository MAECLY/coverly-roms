//! Journaled writes inside a game's `Extras/` (extras spec D1, D3, D5, D10,
//! D14, D20, D22).

mod common;

use std::path::{Component, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use coverly_core::dedupe::testing::pin_mtime;
use coverly_core::extras::index::{list, read_index, IndexRead, Listing};
use coverly_core::extras::ops::{
    confine, forget, move_to, rename, set_tags, ExtrasCtx, ExtrasError, ExtrasNote, ExtrasOutcome,
};
use coverly_core::extras::{CategoryRef, ExtrasCategory, NameRefusal, TagRefusal};
use coverly_core::write::trash::FakeTrash;
use coverly_core::write::{job_transfers, list_jobs, undo_job, JobKind};
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
}

impl Fx {
    fn new() -> Fx {
        let lib = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        let trash = FakeTrash::new(data.path().join("fake-trash"));
        let root = lib.path().canonicalize().unwrap();
        let game = root.join("snes").join(GAME);
        std::fs::create_dir_all(&game).unwrap();
        let fx = Fx {
            _lib: lib,
            data,
            trash,
            root,
            game: game.clone(),
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
