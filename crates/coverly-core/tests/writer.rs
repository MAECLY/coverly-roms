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

/// An organise job taking files from `<tmp>/S` into `<tmp>/T`, with an
/// archive `S/Next Tetris, The.zip` and the target folder `T/PS`.
fn extract_setup() -> (TempDir, PathBuf, PathBuf, PathBuf, Writer) {
    let tmp = TempDir::new().unwrap();
    let (root, src, data) = (
        tmp.path().join("T"),
        tmp.path().join("S"),
        tmp.path().join("d"),
    );
    fs::create_dir_all(root.join("PS")).unwrap();
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("Next Tetris, The.zip"), b"7z-bytes").unwrap();
    let w = Writer::begin_with(
        &root,
        &data,
        JobMeta {
            kind: JobKind::Organize,
            source_root: Some(src.clone()),
        },
    )
    .unwrap();
    (tmp, root, src, data, w)
}

fn extracted_file(src: &Path, w: &Writer, n: u32) -> ExtractedFile {
    ExtractedFile {
        from: src.join("Next Tetris, The.zip"),
        entry: "Next Tetris, The (Track 01).bin".to_string(),
        size: 5,
        crc32: Some(0x1234_5678),
        temp: format!(".coverly-{}-{n}.tmp", w.job_id()),
    }
}

fn journal_lines(data: &Path, job: &str) -> Vec<serde_json::Value> {
    fs::read_to_string(data.join("jobs").join(format!("{job}.jsonl")))
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

#[test]
fn extracted_intent_then_completion_round_trip() {
    let (_tmp, root, src, data, mut w) = extract_setup();
    let job = w.job_id().to_string();
    let target = root.join("PS/Next Tetris, The (Track 01).bin");
    let file = extracted_file(&src, &w, 0);
    w.extract_intent(&target, &file, Some(3)).unwrap();
    assert!(!w.has_entries(), "an intent alone changes nothing");
    fs::write(&target, b"track").unwrap();
    let fp = w
        .extract_done(&target, &file, 0xCAFE_F00D, Some(3))
        .unwrap();
    assert_eq!((fp.size, fp.crc32), (5, Some(0xCAFE_F00D)));
    assert!(w.has_entries());
    w.finish().unwrap();

    let lines = journal_lines(&data, &job);
    let intent = &lines[1];
    assert_eq!(intent["op"]["kind"], "extracted");
    assert_eq!(intent["op"]["entry"], "Next Tetris, The (Track 01).bin");
    assert_eq!(intent["op"]["size"], 5);
    assert_eq!(intent["op"]["crc32"], 0x1234_5678);
    assert_eq!(intent["op"]["temp"], file.temp.as_str());
    assert_eq!(intent["item"], 3);
    assert!(intent.get("fp").is_none(), "{intent}");
    let done = &lines[2];
    assert_eq!(done["op"], intent["op"]);
    assert_eq!(done["fp"]["crc32"], 0xCAFE_F00D_u32);
    assert_eq!(done["fp"]["size"], 5);

    let records = job_transfers(&data, &job).unwrap();
    assert_eq!(records.len(), 2);
    for (record, finished) in records.iter().zip([false, true]) {
        assert_eq!(record.item, Some(3));
        assert_eq!(record.from, file.from);
        assert_eq!(record.target, target);
        assert_eq!(record.finished, finished);
        let ex = record.extracted.as_ref().unwrap();
        assert_eq!(
            (ex.entry.as_str(), ex.size, ex.crc32, ex.temp.as_str()),
            (
                file.entry.as_str(),
                5,
                Some(0x1234_5678),
                file.temp.as_str()
            )
        );
        assert!(!ex.rolled_back);
    }
    assert_eq!(records[1].fp, Some(fp));
}

#[test]
fn journal_is_append_only_later_line_supersedes() {
    let (_tmp, root, src, data, mut w) = extract_setup();
    let job = w.job_id().to_string();
    let journal = data.join("jobs").join(format!("{job}.jsonl"));
    let archive = src.join("Next Tetris, The.zip");
    let target = root.join("PS/Next Tetris, The (Track 01).bin");
    let (first, second) = (extracted_file(&src, &w, 0), extracted_file(&src, &w, 1));
    let mut before = fs::read(&journal).unwrap();
    let mut grew = || {
        let now = fs::read(&journal).unwrap();
        assert!(now.len() > before.len() && now.starts_with(&before));
        assert!(now.ends_with(b"\n"));
        before = now;
    };
    // A first attempt rolled back, a second one committed.
    w.extract_intent(&target, &first, Some(0)).unwrap();
    grew();
    w.rolled_back(&target, &first.temp, Some(0)).unwrap();
    grew();
    w.extract_intent(&target, &second, Some(0)).unwrap();
    grew();
    fs::write(&target, b"track").unwrap();
    w.extract_done(&target, &second, 7, Some(0)).unwrap();
    grew();
    // The archive: intent, then the Trash refused it.
    let fp = w.archive_trash_intent(&archive, Some(0)).unwrap();
    assert_eq!((fp.size, fp.crc32), (8, None));
    grew();
    w.archive_kept(&archive, Some(0)).unwrap();
    grew();
    w.finish().unwrap();

    let records = job_transfers(&data, &job).unwrap();
    let flags: Vec<(String, bool, bool)> = records
        .iter()
        .map(|r| {
            let ex = r.extracted.as_ref().unwrap();
            (ex.temp.clone(), ex.rolled_back, r.finished)
        })
        .collect();
    assert_eq!(
        flags,
        [
            (first.temp.clone(), true, false),
            (second.temp.clone(), false, false),
            (second.temp.clone(), false, true),
        ]
    );
    let steps = job_archive_steps(&data, &job).unwrap();
    assert_eq!(steps.len(), 2);
    assert!(steps.iter().all(|s| s.from == archive && s.item == Some(0)));
    assert_eq!(steps[0].step, ArchiveStep::TrashIntent { fp: Some(fp) });
    assert_eq!(steps[1].step, ArchiveStep::Kept);
    let kinds: Vec<String> = journal_lines(&data, &job)
        .iter()
        .filter_map(|l| l["op"]["kind"].as_str().map(str::to_string))
        .collect();
    assert_eq!(
        kinds,
        [
            "extracted",
            "rolled-back",
            "extracted",
            "extracted",
            "archive-trash",
            "archive-kept"
        ]
    );
}

#[test]
fn archive_trashed_records_where_the_archive_went() {
    let (_tmp, _root, src, data, mut w) = extract_setup();
    let job = w.job_id().to_string();
    let archive = src.join("Next Tetris, The.zip");
    w.archive_trash_intent(&archive, Some(1)).unwrap();
    let trash = src.join(".Trashes/501/Next Tetris, The.zip");
    fs::remove_file(&archive).unwrap();
    w.archive_trashed(&archive, Some(&trash), Some(1)).unwrap();
    assert!(w.has_entries(), "the archive left its folder");
    w.archive_trashed(&archive, None, Some(1)).unwrap();
    w.finish().unwrap();
    let steps = job_archive_steps(&data, &job).unwrap();
    assert_eq!(
        steps[1].step,
        ArchiveStep::Trashed {
            trash: Some(trash.clone())
        }
    );
    assert_eq!(steps[2].step, ArchiveStep::Trashed { trash: None });
    let lines = journal_lines(&data, &job);
    assert_eq!(lines[2]["op"]["kind"], "archive-trashed");
    assert_eq!(lines[3]["op"]["trash"], serde_json::Value::Null);
    // Extraction steps are not moves or copies.
    assert!(job_transfers(&data, &job).unwrap().is_empty());
}

#[test]
fn extract_ops_refuse_paths_outside_and_foreign_temps() {
    let (tmp, root, src, _data, mut w) = extract_setup();
    let target = root.join("PS/a.bin");
    let outside = tmp.path().join("elsewhere.bin");
    let good = extracted_file(&src, &w, 0);
    assert!(matches!(
        w.extract_intent(&outside, &good, None),
        Err(WriteError::OutsideRoot(_))
    ));
    let from_outside = ExtractedFile {
        from: tmp.path().join("other.zip"),
        ..good.clone()
    };
    assert!(matches!(
        w.extract_intent(&target, &from_outside, None),
        Err(WriteError::OutsideRoot(_))
    ));
    for temp in [
        "../x.tmp".to_string(),
        format!("sub/.coverly-{}-0.tmp", w.job_id()),
        format!("sub\\.coverly-{}-0.tmp", w.job_id()),
        ".coverly-20200101T000000Z-0000-0000-0.tmp".to_string(),
        format!(".coverly-{}-0.bin", w.job_id()),
        "Track 01.bin".to_string(),
    ] {
        let bad = ExtractedFile {
            temp: temp.clone(),
            ..good.clone()
        };
        assert!(
            matches!(
                w.extract_intent(&target, &bad, None),
                Err(WriteError::NameInvalid(_))
            ),
            "{temp}"
        );
        assert!(w.rolled_back(&target, &temp, None).is_err(), "{temp}");
    }
    assert!(matches!(
        w.archive_trash_intent(&outside, None),
        Err(WriteError::OutsideRoot(_))
    ));
    assert!(matches!(
        w.archive_trash_intent(&src.join("missing.zip"), None),
        Err(WriteError::Io(_))
    ));
    // Only the archive's own folder is accepted, never the target root.
    fs::write(root.join("PS/b.zip"), b"x").unwrap();
    assert!(w.archive_kept(&root.join("PS/b.zip"), None).is_err());
    // Nothing reached the journal.
    assert!(!w.has_entries());
}

#[test]
fn older_build_sees_unknown_and_refuses_undo() {
    // What a build from before this change does with an extraction
    // journal: every new op kind is unknown to it, so the job is listed
    // but never half undone. Simulated with an op kind no build knows.
    let (_tmp, root, src, data, mut w) = extract_setup();
    let job = w.job_id().to_string();
    let moved_from = src.join("Tetris.gba");
    fs::write(&moved_from, b"ROM").unwrap();
    fs::create_dir_all(root.join("GBA")).unwrap();
    let moved_to = root.join("GBA/Tetris.gba");
    w.move_file(&moved_from, &moved_to, Collision::Skip, Some(0))
        .unwrap();
    drop(w);
    let journal = data.join("jobs").join(format!("{job}.jsonl"));
    let mut text = fs::read_to_string(&journal).unwrap();
    text.push_str(&format!(
        "{{\"type\":\"entry\",\"target\":\"{}\",\"op\":{{\"kind\":\"extracted-v9\",\"from\":\"{}\",\"entry\":\"x\"}},\"item\":1}}\n",
        json_path(&root.join("PS/x.bin")),
        json_path(&src.join("Next Tetris, The.zip"))
    ));
    fs::write(&journal, text).unwrap();
    let s = &list_jobs(&data).unwrap()[0];
    assert_eq!((s.entries, s.undoable), (2, false));
    let err = undo_job(&data, &job).unwrap_err();
    assert!(
        matches!(&err, WriteError::Journal(m) if m.contains("newer")),
        "{err}"
    );
    assert!(moved_to.exists() && !moved_from.exists(), "nothing undone");
    assert!(!list_jobs(&data).unwrap()[0].undone);
}

#[test]
fn an_extraction_journal_is_undoable() {
    // Extraction lines are undone like the rest of the job: a completed
    // extracted file that still matches its fingerprint goes.
    let (_tmp, root, src, data, mut w) = extract_setup();
    let job = w.job_id().to_string();
    let target = root.join("PS/Next Tetris, The (Track 01).bin");
    let file = extracted_file(&src, &w, 0);
    w.extract_intent(&target, &file, Some(0)).unwrap();
    fs::write(&target, b"track").unwrap();
    w.extract_done(&target, &file, crc32fast::hash(b"track"), Some(0))
        .unwrap();
    w.finish().unwrap();
    let s = &list_jobs(&data).unwrap()[0];
    assert!(s.undoable);
    let report = undo_job(&data, &job).unwrap();
    assert_eq!(report.removed, std::slice::from_ref(&target));
    assert!(!target.exists());
    assert!(list_jobs(&data).unwrap()[0].undone);
}

#[test]
fn journal_from_before_change_still_undoes() {
    let tmp = TempDir::new().unwrap();
    let (root, src, data) = (
        tmp.path().join("T"),
        tmp.path().join("S"),
        tmp.path().join("d"),
    );
    let job = "20261018T120000Z-1234-0000";
    fs::create_dir_all(root.join("GBA")).unwrap();
    fs::create_dir_all(root.join("GB")).unwrap();
    fs::create_dir_all(&src).unwrap();
    let at = filetime::FileTime::from_unix_time(1_700_000_000, 0);
    let (moved, copy, copy_src) = (
        root.join("GBA/Tetris.gba"),
        root.join("GB/Tetris.gb"),
        src.join("Tetris.gb"),
    );
    fs::write(&moved, b"GBA!").unwrap();
    fs::write(&copy, b"GB").unwrap();
    fs::write(&copy_src, b"GB").unwrap();
    for f in [&moved, &copy] {
        filetime::set_file_mtime(f, at).unwrap();
    }
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/journals/organize-v1.jsonl");
    let text = fs::read_to_string(fixture)
        .unwrap()
        .replace("@ROOT@", &json_path(&root))
        .replace("@SRC@", &json_path(&src));
    fs::create_dir_all(data.join("jobs")).unwrap();
    fs::write(data.join("jobs").join(format!("{job}.jsonl")), text).unwrap();

    let s = &list_jobs(&data).unwrap()[0];
    assert_eq!(
        (s.kind, s.entries, s.complete, s.undoable),
        (JobKind::Organize, 5, true, true)
    );
    let records = job_transfers(&data, job).unwrap();
    assert_eq!(records.len(), 3);
    assert!(records.iter().all(|r| r.extracted.is_none()));
    assert!(job_archive_steps(&data, job).unwrap().is_empty());

    let report = undo_job(&data, job).unwrap();
    assert_eq!(report.restored, [src.join("Tetris.gba")]);
    assert_eq!(report.removed, std::slice::from_ref(&copy));
    assert_eq!(fs::read(src.join("Tetris.gba")).unwrap(), b"GBA!");
    assert!(!moved.exists() && !copy.exists() && copy_src.exists());
    assert!(!root.join("GBA").exists() && !root.join("GB").exists());
}

#[test]
fn extract_temps_are_unique_within_a_job_across_resumed_runs() {
    let (_tmp, root, src, data, mut w) = extract_setup();
    let job = w.job_id().to_string();
    let target = root.join("PS/Next Tetris, The (Track 01).bin");
    // An earlier run used a temp numbered past its line count (the copy
    // counter skips names a crash left behind), then stopped.
    let late = extracted_file(&src, &w, 40);
    w.extract_intent(&target, &late, Some(0)).unwrap();
    drop(w);

    let mut w = Writer::resume(&root, &data, &job).unwrap();
    // The journaled temp is never handed to a second intent...
    assert!(matches!(
        w.extract_intent(&target, &late, Some(0)),
        Err(WriteError::NameInvalid(_))
    ));
    // ...and the temps this run mints never collide with it.
    let minted: Vec<String> = (0..64).map(|_| w.extract_temp()).collect();
    assert!(!minted.contains(&late.temp), "{minted:?}");
    let mut unique = minted.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), minted.len());
    let fresh = ExtractedFile {
        temp: minted[0].clone(),
        ..late.clone()
    };
    w.extract_intent(&target, &fresh, Some(0)).unwrap();
    // The earlier intent can still be rolled back by the resumed run.
    w.rolled_back(&target, &late.temp, Some(0)).unwrap();
    w.finish().unwrap();

    let records = job_transfers(&data, &job).unwrap();
    let flags: Vec<(String, bool)> = records
        .iter()
        .map(|r| {
            let ex = r.extracted.as_ref().unwrap();
            (ex.temp.clone(), ex.rolled_back)
        })
        .collect();
    assert_eq!(
        flags,
        [(late.temp.clone(), true), (fresh.temp.clone(), false)]
    );
}

#[test]
fn completion_and_rollback_need_a_journaled_intent() {
    let (_tmp, root, src, _data, mut w) = extract_setup();
    let target = root.join("PS/Next Tetris, The (Track 01).bin");
    let file = extracted_file(&src, &w, 0);
    fs::write(&target, b"track").unwrap();
    assert!(matches!(
        w.extract_done(&target, &file, 1, Some(0)),
        Err(WriteError::NameInvalid(_))
    ));
    assert!(matches!(
        w.rolled_back(&target, &file.temp, Some(0)),
        Err(WriteError::NameInvalid(_))
    ));
    assert!(!w.has_entries());
}

// --- Duplicates jobs (duplicates plan Task 21) ---

fn dedupe_job(root: &Path, data: &Path) -> Writer {
    Writer::begin_with(
        root,
        data,
        JobMeta {
            kind: JobKind::Dedupe,
            source_root: None,
        },
    )
    .unwrap()
}

/// A journal with only a header of `kind` and a done line.
fn bare_journal(data: &Path, job: &str, kind: &str) {
    let jobs = data.join("jobs");
    fs::create_dir_all(&jobs).unwrap();
    fs::write(
        jobs.join(format!("{job}.jsonl")),
        format!(
            "{{\"type\":\"header\",\"job\":\"{job}\",\"root\":\"/lib\",\"kind\":\"{kind}\",\"created_at\":\"x\"}}\n{{\"type\":\"done\"}}\n"
        ),
    )
    .unwrap();
}

fn job_ids(data: &Path) -> Vec<String> {
    list_jobs(data)
        .unwrap()
        .into_iter()
        .map(|j| j.job)
        .collect()
}

/// 2026-10-17T00:00:00Z.
fn oct_17() -> std::time::SystemTime {
    std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_792_195_200)
}

#[test]
fn dedupe_job_lists_with_kind_dedupe() {
    let tmp = TempDir::new().unwrap();
    let (root, data) = (tmp.path().join("T"), tmp.path().join("d"));
    fs::create_dir_all(&root).unwrap();
    let w = dedupe_job(&root, &data);
    assert_eq!(w.kind(), JobKind::Dedupe);
    let journal = w.journal_path().to_path_buf();
    let job = w.finish().unwrap();
    let header = fs::read_to_string(&journal).unwrap();
    let first = header.lines().next().unwrap();
    assert!(first.contains("\"kind\":\"dedupe\""), "{first}");
    let s = list_jobs(&data)
        .unwrap()
        .into_iter()
        .find(|j| j.job == job)
        .unwrap();
    assert_eq!(s.kind, JobKind::Dedupe);
    assert!(s.undoable);
    let w = Writer::resume(&root, &data, &job).unwrap();
    assert_eq!(w.kind(), JobKind::Dedupe);
}

#[test]
fn dedupe_jobs_survive_thirty_fill_jobs() {
    let tmp = TempDir::new().unwrap();
    let data = tmp.path();
    // Ten days before 2026-10-17, older in id order than every fill.
    let dedupe = "20261007T000000Z-0000-0000".to_string();
    bare_journal(data, &dedupe, "dedupe");
    let fills: Vec<String> = (0..30)
        .map(|n| format!("20261016T0000{n:02}Z-0000-0000"))
        .collect();
    for job in &fills {
        bare_journal(data, job, "fill");
    }
    prune_jobs_at(data, 20, 90, oct_17()).unwrap();
    let left = job_ids(data);
    assert!(left.contains(&dedupe), "{left:?}");
    assert_eq!(left.iter().filter(|j| fills.contains(j)).count(), 20);
}

#[test]
fn dedupe_and_organise_are_counted_apart() {
    let tmp = TempDir::new().unwrap();
    let data = tmp.path();
    // All older than 90 days at 2026-10-17; keep 1 per kind.
    let (old_org, new_org) = (
        "20260101T000000Z-0000-0000".to_string(),
        "20260301T000000Z-0000-0000".to_string(),
    );
    let (old_dd, new_dd) = (
        "20260102T000000Z-0000-0000".to_string(),
        "20260201T000000Z-0000-0000".to_string(),
    );
    bare_journal(data, &old_org, "organize");
    bare_journal(data, &new_org, "organize");
    bare_journal(data, &old_dd, "dedupe");
    bare_journal(data, &new_dd, "dedupe");
    prune_jobs_at(data, 1, 90, oct_17()).unwrap();
    assert_eq!(job_ids(data), [new_org, new_dd]);
}

#[test]
fn an_older_reader_never_undoes_a_dedupe_journal() {
    /// `JobKind` as builds before duplicates had it.
    #[derive(Debug, serde::Deserialize)]
    #[serde(rename_all = "kebab-case")]
    #[allow(dead_code)]
    enum OldJobKind {
        Fill,
        Organize,
    }
    #[derive(Debug, serde::Deserialize)]
    #[allow(dead_code)]
    struct OldHeader {
        kind: OldJobKind,
    }
    let tmp = TempDir::new().unwrap();
    let (root, data) = (tmp.path().join("T"), tmp.path().join("d"));
    fs::create_dir_all(&root).unwrap();
    let w = dedupe_job(&root, &data);
    let journal = w.journal_path().to_path_buf();
    w.finish().unwrap();
    let text = fs::read_to_string(journal).unwrap();
    let header = text.lines().next().unwrap();
    // An older build cannot read the header: it lists the job unreadable
    // (not undoable) and never half-undoes it.
    assert!(serde_json::from_str::<OldHeader>(header).is_err());
}
