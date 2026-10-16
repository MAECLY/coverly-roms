use std::fs;
use std::path::{Path, PathBuf};

use coverly_core::write::*;
use tempfile::TempDir;

fn names_under(root: &Path) -> Vec<String> {
    walkdir::WalkDir::new(root)
        .into_iter()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect()
}

fn setup() -> (TempDir, TempDir, PathBuf) {
    let (lib, data) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let imgs = lib.path().join("ROMS/SFC/Imgs");
    fs::create_dir_all(&imgs).unwrap();
    (lib, data, imgs)
}

#[test]
fn creates_files_atomically_without_leftovers() {
    let (lib, data, imgs) = setup();
    let mut w = Writer::begin(lib.path(), data.path()).unwrap();
    let target = imgs.join("Zelda (USA).png");
    assert!(matches!(
        w.write(&target, b"new", OverwritePolicy::Never).unwrap(),
        WriteOutcome::Created { .. }
    ));
    w.finish().unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"new");
    let names = names_under(lib.path());
    assert!(
        names
            .iter()
            .all(|n| !n.starts_with("._") && !n.starts_with(".coverly-")),
        "{names:?}"
    );
}

#[test]
fn creates_missing_art_folders_inside_the_root() {
    let (lib, data, _) = setup();
    let mut w = Writer::begin(lib.path(), data.path()).unwrap();
    let target = lib.path().join("ROMS/GBA/Imgs/001 Golden Sun.png");
    w.write(&target, b"x", OverwritePolicy::Never).unwrap();
    assert!(target.exists());
}

#[test]
fn never_policy_skips_existing_and_case_insensitive_twin() {
    let (lib, data, imgs) = setup();
    fs::write(imgs.join("zelda (usa).png"), b"mine").unwrap();
    let mut w = Writer::begin(lib.path(), data.path()).unwrap();
    let out = w
        .write(
            &imgs.join("Zelda (USA).png"),
            b"new",
            OverwritePolicy::Never,
        )
        .unwrap();
    assert!(matches!(out, WriteOutcome::SkippedExists { .. }));
    assert_eq!(fs::read(imgs.join("zelda (usa).png")).unwrap(), b"mine");
}

#[test]
fn backup_policy_keeps_the_old_bytes_outside_the_library() {
    let (lib, data, imgs) = setup();
    let target = imgs.join("Zelda (USA).png");
    fs::write(&target, b"old").unwrap();
    let mut w = Writer::begin(lib.path(), data.path()).unwrap();
    let job = w.job_id().to_string();
    let WriteOutcome::Replaced { backup, .. } =
        w.write(&target, b"new", OverwritePolicy::Backup).unwrap()
    else {
        panic!()
    };
    w.finish().unwrap();
    assert_eq!(
        backup,
        data.path()
            .join("backups")
            .join(&job)
            .join("ROMS/SFC/Imgs/Zelda (USA).png")
    );
    assert_eq!(fs::read(&backup).unwrap(), b"old");
    assert_eq!(fs::read(&target).unwrap(), b"new");
}

#[test]
fn refuses_targets_outside_the_root() {
    let (lib, data, _) = setup();
    let mut w = Writer::begin(lib.path(), data.path()).unwrap();
    for bad in [
        lib.path().join("../evil.png"),
        lib.path().join("ROMS/../../evil.png"),
        PathBuf::from("/tmp/evil.png"),
        lib.path().to_path_buf(),
    ] {
        assert!(
            matches!(
                w.write(&bad, b"x", OverwritePolicy::Never),
                Err(WriteError::OutsideRoot(_))
            ),
            "{}",
            bad.display()
        );
    }
}

#[test]
fn begin_requires_an_absolute_root() {
    // The journal stores the root; a relative one would make `coverly undo`
    // from another working directory touch the wrong files.
    let data = TempDir::new().unwrap();
    assert!(matches!(
        Writer::begin(Path::new("relative/sd"), data.path()),
        Err(WriteError::OutsideRoot(_))
    ));
}

#[test]
fn listing_or_undoing_with_no_jobs_folder() {
    let data = TempDir::new().unwrap();
    assert!(list_jobs(data.path()).unwrap().is_empty());
    assert!(matches!(
        undo_last(data.path()),
        Err(WriteError::NothingToUndo)
    ));
}

#[cfg(unix)]
#[test]
fn refuses_symlink_escape() {
    let (lib, data, _) = setup();
    let outside = TempDir::new().unwrap();
    std::os::unix::fs::symlink(outside.path(), lib.path().join("ROMS/GBA")).unwrap();
    let mut w = Writer::begin(lib.path(), data.path()).unwrap();
    let err = w
        .write(
            &lib.path().join("ROMS/GBA/Imgs/x.png"),
            b"x",
            OverwritePolicy::Never,
        )
        .unwrap_err();
    assert!(matches!(err, WriteError::OutsideRoot(_)), "{err}");
    assert!(fs::read_dir(outside.path()).unwrap().next().is_none());
}

#[test]
fn unsafe_names_are_refused() {
    let (lib, data, imgs) = setup();
    let mut w = Writer::begin(lib.path(), data.path()).unwrap();
    assert!(matches!(
        w.write(&imgs.join("a:b.png"), b"x", OverwritePolicy::Never),
        Err(WriteError::NameInvalid(_))
    ));
}

#[test]
fn fat_safe_names() {
    assert_eq!(safe_file_name("a:b?.png").unwrap(), "a_b_.png");
    assert_eq!(safe_file_name("CON.png").unwrap(), "_CON.png");
    assert_eq!(safe_file_name("name. ").unwrap(), "name");
    assert_eq!(safe_file_name("\u{7}x").unwrap(), "_x");
    assert_eq!(
        safe_file_name("Batman & Robin (USA).png").unwrap(),
        "Batman & Robin (USA).png"
    );
    assert!(safe_file_name("").is_err());
    assert!(safe_file_name("...").is_err());
    let long = format!("{}.png", "é".repeat(200));
    let s = safe_file_name(&long).unwrap();
    assert!(s.len() <= 255 && s.ends_with(".png"), "{}", s.len());
}

#[test]
fn undo_removes_created_and_restores_replaced_files() {
    let (lib, data, imgs) = setup();
    let replaced = imgs.join("Old.png");
    fs::write(&replaced, b"old").unwrap();
    let created = imgs.join("New.png");
    let mut w = Writer::begin(lib.path(), data.path()).unwrap();
    w.write(&created, b"made", OverwritePolicy::Never).unwrap();
    w.write(&replaced, b"newer", OverwritePolicy::Backup)
        .unwrap();
    let job = w.finish().unwrap();
    let report = undo_last(data.path()).unwrap();
    assert_eq!(report.job, job);
    assert_eq!((report.removed.len(), report.restored.len()), (1, 1));
    assert!(!created.exists());
    assert_eq!(fs::read(&replaced).unwrap(), b"old");
    assert!(list_jobs(data.path()).unwrap()[0].undone);
    assert!(matches!(
        undo_last(data.path()),
        Err(WriteError::NothingToUndo)
    ));
}

#[test]
fn undo_keeps_files_changed_after_the_job() {
    let (lib, data, imgs) = setup();
    let created = imgs.join("New.png");
    let mut w = Writer::begin(lib.path(), data.path()).unwrap();
    w.write(&created, b"made", OverwritePolicy::Never).unwrap();
    w.finish().unwrap();
    fs::write(&created, b"user edited").unwrap();
    let report = undo_last(data.path()).unwrap();
    assert_eq!(report.skipped_modified, std::slice::from_ref(&created));
    assert_eq!(fs::read(&created).unwrap(), b"user edited");
}

#[test]
fn an_unfinished_job_can_still_be_undone() {
    let (lib, data, imgs) = setup();
    let created = imgs.join("New.png");
    {
        let mut w = Writer::begin(lib.path(), data.path()).unwrap();
        w.write(&created, b"made", OverwritePolicy::Never).unwrap();
        // dropped without finish(): crash, eject, pause
    }
    assert!(!list_jobs(data.path()).unwrap()[0].complete);
    undo_last(data.path()).unwrap();
    assert!(!created.exists());
}

#[test]
fn prune_keeps_the_newest_jobs() {
    let (lib, data, imgs) = setup();
    for i in 0..3 {
        let mut w = Writer::begin(lib.path(), data.path()).unwrap();
        w.write(&imgs.join(format!("{i}.png")), b"x", OverwritePolicy::Never)
            .unwrap();
        w.finish().unwrap();
    }
    prune_jobs(data.path(), 2).unwrap();
    assert_eq!(list_jobs(data.path()).unwrap().len(), 2);
}

#[test]
fn a_torn_last_journal_line_does_not_block_undo() {
    let (lib, data, imgs) = setup();
    let created = imgs.join("New.png");
    let mut w = Writer::begin(lib.path(), data.path()).unwrap();
    let job = w.job_id().to_string();
    w.write(&created, b"made", OverwritePolicy::Never).unwrap();
    drop(w);
    let journal = data.path().join("jobs").join(format!("{job}.jsonl"));
    let mut text = fs::read_to_string(&journal).unwrap();
    text.push_str("{\"type\":\"entr");
    fs::write(&journal, text).unwrap();
    undo_last(data.path()).unwrap();
    assert!(!created.exists());
}

#[test]
fn undo_job_refuses_hostile_ids() {
    let data = TempDir::new().unwrap();
    for bad in ["../x", "a/b", "", ".."] {
        assert!(
            matches!(undo_job(data.path(), bad), Err(WriteError::NothingToUndo)),
            "{bad}"
        );
    }
}
