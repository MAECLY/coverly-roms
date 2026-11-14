//! Journaled writes inside a game's `Extras/` (extras spec D3, D5, D10, D20).

use std::path::{Component, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use coverly_core::dedupe::testing::pin_mtime;
use coverly_core::extras::index::{list, read_index, IndexRead, Listing};
use coverly_core::extras::ops::{
    confine, forget, set_tags, ExtrasCtx, ExtrasError, ExtrasNote, ExtrasOutcome,
};
use coverly_core::extras::TagRefusal;
use coverly_core::write::trash::FakeTrash;
use coverly_core::write::{list_jobs, undo_job, JobKind};
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
