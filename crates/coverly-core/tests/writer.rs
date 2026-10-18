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
    // Built as text: on Windows `join("a:b.png")` reads `a:` as a drive and
    // replaces the whole path, so the name would never reach the check.
    let colon = std::path::PathBuf::from(format!(
        "{}{}a:b.png",
        imgs.display(),
        std::path::MAIN_SEPARATOR
    ));
    assert!(matches!(
        w.write(&colon, b"x", OverwritePolicy::Never),
        Err(WriteError::NameInvalid(_))
    ));
    // That drive-relative path is refused too, whatever the reason.
    assert!(w
        .write(&imgs.join("a:b.png"), b"x", OverwritePolicy::Never)
        .is_err());
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

#[test]
fn discarding_an_empty_job_leaves_no_journal_and_no_backups() {
    let (lib, data, imgs) = setup();
    let mut w = Writer::begin(lib.path(), data.path()).unwrap();
    let job = w.job_id().to_string();
    assert!(!w.has_entries());
    // A refused write appends nothing.
    assert!(w
        .write(&imgs.join("a?b.png"), b"x", OverwritePolicy::Backup)
        .is_err());
    assert!(!w.has_entries());
    w.discard().unwrap();
    assert!(list_jobs(data.path()).unwrap().is_empty());
    assert!(!data
        .path()
        .join("jobs")
        .join(format!("{job}.jsonl"))
        .exists());
    assert!(!data.path().join("backups").join(&job).exists());
}

#[test]
fn a_job_with_entries_is_never_discarded() {
    let (lib, data, imgs) = setup();
    let mut w = Writer::begin(lib.path(), data.path()).unwrap();
    w.write(&imgs.join("a.png"), b"x", OverwritePolicy::Never)
        .unwrap();
    assert!(w.has_entries());
    assert!(matches!(w.discard(), Err(WriteError::Journal(_))));
    assert_eq!(list_jobs(data.path()).unwrap().len(), 1);
    undo_last(data.path()).unwrap();
    assert!(!imgs.join("a.png").exists());
}

/// `path` as the inside of a JSON string (backslashes escaped on Windows).
fn json_path(path: &Path) -> String {
    let quoted = serde_json::to_string(path).unwrap();
    quoted[1..quoted.len() - 1].to_string()
}

#[test]
fn an_old_fill_journal_still_parses_and_undoes() {
    let (lib, data) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let job = "20261001T120000Z-1234-0000";
    let imgs = lib.path().join("ROMS/GBA/Imgs");
    fs::create_dir_all(&imgs).unwrap();
    fs::write(imgs.join("Tetris.png"), b"TETRIS-ART").unwrap();
    fs::write(imgs.join("Old.png"), b"NEW-ART").unwrap();
    let backup = data.path().join("backups").join(job).join("ROMS/GBA/Imgs");
    fs::create_dir_all(&backup).unwrap();
    fs::write(backup.join("Old.png"), b"USER-ART").unwrap();

    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/journals/fill-v1.jsonl");
    let text = fs::read_to_string(fixture)
        .unwrap()
        .replace("@ROOT@", &json_path(lib.path()))
        .replace("@DATA@", &json_path(data.path()));
    fs::create_dir_all(data.path().join("jobs")).unwrap();
    fs::write(data.path().join("jobs").join(format!("{job}.jsonl")), text).unwrap();

    let jobs = list_jobs(data.path()).unwrap();
    assert_eq!(jobs.len(), 1);
    let s = &jobs[0];
    assert_eq!(s.job, job);
    assert_eq!((s.kind, s.source_root.as_deref()), (JobKind::Fill, None));
    assert_eq!((s.entries, s.complete, s.undoable), (2, true, true));

    let report = undo_last(data.path()).unwrap();
    assert_eq!(report.removed, [imgs.join("Tetris.png")]);
    assert_eq!(report.restored, [imgs.join("Old.png")]);
    assert!(!imgs.join("Tetris.png").exists());
    assert_eq!(fs::read(imgs.join("Old.png")).unwrap(), b"USER-ART");
}

#[test]
fn a_new_fill_journal_is_a_fill_job() {
    let (lib, data, imgs) = setup();
    let mut w = Writer::begin(lib.path(), data.path()).unwrap();
    w.write(&imgs.join("a.png"), b"x", OverwritePolicy::Never)
        .unwrap();
    w.finish().unwrap();
    let s = &list_jobs(data.path()).unwrap()[0];
    assert_eq!(
        (s.kind, s.source_root.as_deref(), s.undoable),
        (JobKind::Fill, None, true)
    );
}

#[test]
fn an_organise_header_round_trips() {
    let tmp = tempfile::tempdir().unwrap();
    let (root, src, data) = (
        tmp.path().join("T"),
        tmp.path().join("S"),
        tmp.path().join("d"),
    );
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&src).unwrap();
    let w = Writer::begin_with(
        &root,
        &data,
        JobMeta {
            kind: JobKind::Organize,
            source_root: Some(src.clone()),
        },
    )
    .unwrap();
    let job = w.finish().unwrap();
    let s = list_jobs(&data)
        .unwrap()
        .into_iter()
        .find(|j| j.job == job)
        .unwrap();
    assert_eq!(
        (s.kind, s.source_root.as_deref()),
        (JobKind::Organize, Some(src.as_path()))
    );
    assert!(s.undoable);
    // A resumed organise job keeps its kind and source root.
    let w = Writer::resume(&root, &data, &job).unwrap();
    assert_eq!(w.kind(), JobKind::Organize);
    assert_eq!(w.source_root(), src.as_path());
}

#[test]
fn begin_with_requires_an_absolute_source_root() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(matches!(
        Writer::begin_with(
            tmp.path(),
            &tmp.path().join("d"),
            JobMeta {
                kind: JobKind::Organize,
                source_root: Some(PathBuf::from("relative/pile")),
            },
        ),
        Err(WriteError::OutsideRoot(_))
    ));
}

#[test]
fn a_journal_from_a_newer_version_is_listed_as_not_undoable() {
    let (lib, data, imgs) = setup();
    let created = imgs.join("a.png");
    let mut w = Writer::begin(lib.path(), data.path()).unwrap();
    let job = w.job_id().to_string();
    w.write(&created, b"x", OverwritePolicy::Never).unwrap();
    drop(w);
    let journal = data.path().join("jobs").join(format!("{job}.jsonl"));
    let mut text = fs::read_to_string(&journal).unwrap();
    text.push_str(&format!(
        "{{\"type\":\"entry\",\"target\":\"{}\",\"op\":{{\"kind\":\"teleported\",\"to\":\"/elsewhere\"}},\"later\":1}}\n",
        json_path(&imgs.join("b.png"))
    ));
    fs::write(&journal, text).unwrap();

    let s = &list_jobs(data.path()).unwrap()[0];
    assert_eq!(
        (s.job.as_str(), s.entries, s.undoable),
        (job.as_str(), 2, false)
    );
    let err = undo_job(data.path(), &job).unwrap_err();
    assert!(
        matches!(&err, WriteError::Journal(m) if m.contains("newer")),
        "{err}"
    );
    let err = undo_last(data.path()).unwrap_err();
    assert!(
        matches!(&err, WriteError::Journal(m) if m.contains("newer")),
        "{err}"
    );
    // Nothing was touched and the job is still pending.
    assert_eq!(fs::read(&created).unwrap(), b"x");
    assert!(!list_jobs(data.path()).unwrap()[0].undone);
}

#[test]
fn a_fill_entry_without_a_checksum_is_left_alone() {
    let (lib, data, imgs) = setup();
    let created = imgs.join("a.png");
    let mut w = Writer::begin(lib.path(), data.path()).unwrap();
    let job = w.job_id().to_string();
    w.write(&created, b"x", OverwritePolicy::Never).unwrap();
    drop(w);
    let journal = data.path().join("jobs").join(format!("{job}.jsonl"));
    let text = fs::read_to_string(&journal).unwrap();
    let start = text.find(",\"sha1\":").unwrap();
    let end = start + text[start..].find('}').unwrap();
    let stripped = format!("{}{}", &text[..start], &text[end..]);
    assert!(!stripped.contains("sha1"), "{stripped}");
    fs::write(&journal, stripped).unwrap();

    let report = undo_last(data.path()).unwrap();
    assert_eq!(report.skipped_modified, std::slice::from_ref(&created));
    assert!(created.exists());
}
