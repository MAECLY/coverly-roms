use std::fs;
use std::path::{Path, PathBuf};

use coverly_core::write::{
    list_jobs, undo_job, undo_last, OverwritePolicy, RemoveOutcome, WriteError, Writer,
};
use tempfile::TempDir;

fn setup() -> (TempDir, TempDir, PathBuf) {
    let lib = TempDir::new().unwrap();
    let data = TempDir::new().unwrap();
    let root = lib.path().to_path_buf();
    fs::create_dir_all(root.join("ROMS/GBA/Imgs")).unwrap();
    (lib, data, root)
}

fn img(root: &Path, name: &str) -> PathBuf {
    root.join("ROMS/GBA/Imgs").join(name)
}

#[test]
fn resume_appends_to_the_same_journal_and_one_undo_reverts_both() {
    let (_lib, data, root) = setup();
    let mut w = Writer::begin(&root, data.path()).unwrap();
    w.write(&img(&root, "a.png"), b"a", OverwritePolicy::Never)
        .unwrap();
    let job = w.finish().unwrap();
    let mut w = Writer::resume(&root, data.path(), &job).unwrap();
    assert_eq!(w.job_id(), job);
    w.write(&img(&root, "b.png"), b"b", OverwritePolicy::Never)
        .unwrap();
    w.finish().unwrap();
    let jobs = list_jobs(data.path()).unwrap();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].entries, 2);
    assert!(jobs[0].complete);
    let undo = undo_last(data.path()).unwrap();
    assert_eq!(undo.removed.len(), 2);
    assert!(!img(&root, "a.png").exists() && !img(&root, "b.png").exists());
}

#[test]
fn resume_refuses_another_library_and_unknown_jobs() {
    let (_lib, data, root) = setup();
    let other = TempDir::new().unwrap();
    let job = Writer::begin(&root, data.path()).unwrap().finish().unwrap();
    assert!(matches!(
        Writer::resume(other.path(), data.path(), &job),
        Err(WriteError::Journal(_))
    ));
    assert!(Writer::resume(&root, data.path(), "../evil").is_err());
    assert!(Writer::resume(&root, data.path(), "20990101T000000Z-0000-0000").is_err());
}

#[test]
fn resume_cuts_a_torn_last_line() {
    let (_lib, data, root) = setup();
    let mut w = Writer::begin(&root, data.path()).unwrap();
    w.write(&img(&root, "a.png"), b"a", OverwritePolicy::Never)
        .unwrap();
    let job = w.job_id().to_string();
    drop(w); // a crash: no `done` line
    let journal = data.path().join("jobs").join(format!("{job}.jsonl"));
    let mut text = fs::read_to_string(&journal).unwrap();
    text.push_str("{\"type\":\"entry\",\"tar");
    fs::write(&journal, text).unwrap();
    let mut w = Writer::resume(&root, data.path(), &job).unwrap();
    w.write(&img(&root, "b.png"), b"b", OverwritePolicy::Never)
        .unwrap();
    w.finish().unwrap();
    assert_eq!(undo_last(data.path()).unwrap().removed.len(), 2);
}

#[test]
fn undo_restores_removed_art() {
    let (_lib, data, root) = setup();
    fs::write(img(&root, "a.png"), b"original").unwrap();
    let mut w = Writer::begin(&root, data.path()).unwrap();
    let out = w.remove(&img(&root, "a.png")).unwrap();
    let RemoveOutcome::Removed { backup, .. } = out else {
        panic!("{out:?}")
    };
    assert_eq!(fs::read(&backup).unwrap(), b"original");
    w.finish().unwrap();
    assert!(!img(&root, "a.png").exists());
    let undo = undo_last(data.path()).unwrap();
    assert_eq!(undo.restored.len(), 1);
    assert_eq!(fs::read(img(&root, "a.png")).unwrap(), b"original");
}

#[test]
fn undo_keeps_art_the_user_added_after_a_removal() {
    let (_lib, data, root) = setup();
    fs::write(img(&root, "a.png"), b"original").unwrap();
    let mut w = Writer::begin(&root, data.path()).unwrap();
    w.remove(&img(&root, "a.png")).unwrap();
    w.finish().unwrap();
    fs::write(img(&root, "a.png"), b"user art").unwrap();
    let undo = undo_last(data.path()).unwrap();
    assert_eq!(undo.skipped_modified.len(), 1);
    assert_eq!(fs::read(img(&root, "a.png")).unwrap(), b"user art");
}

#[test]
fn remove_skips_missing_files_and_never_removes_folders_or_outside_paths() {
    let (_lib, data, root) = setup();
    let outside = TempDir::new().unwrap();
    fs::write(outside.path().join("x.png"), b"x").unwrap();
    fs::create_dir(img(&root, "dir.png")).unwrap();
    let mut w = Writer::begin(&root, data.path()).unwrap();
    assert_eq!(
        w.remove(&img(&root, "none.png")).unwrap(),
        RemoveOutcome::Missing
    );
    assert!(matches!(
        w.remove(&img(&root, "dir.png")),
        Err(WriteError::NotAFile(_))
    ));
    assert!(matches!(
        w.remove(&outside.path().join("x.png")),
        Err(WriteError::OutsideRoot(_))
    ));
    assert!(img(&root, "dir.png").is_dir() && outside.path().join("x.png").exists());
}

#[test]
fn resume_cuts_a_torn_last_line_inside_a_multibyte_name() {
    let (_lib, data, root) = setup();
    let mut w = Writer::begin(&root, data.path()).unwrap();
    w.write(
        &img(&root, "ゼルダの伝説.png"),
        b"a",
        OverwritePolicy::Never,
    )
    .unwrap();
    let job = w.job_id().to_string();
    drop(w); // a crash while appending the next line
    let journal = data.path().join("jobs").join(format!("{job}.jsonl"));
    let mut bytes = fs::read(&journal).unwrap();
    let torn = "{\"type\":\"entry\",\"target\":\"ゼ".as_bytes();
    // Cut inside the last character: the file is not valid UTF-8.
    bytes.extend_from_slice(&torn[..torn.len() - 1]);
    fs::write(&journal, &bytes).unwrap();
    let jobs = list_jobs(data.path()).unwrap();
    assert_eq!(jobs[0].entries, 1);
    let mut w = Writer::resume(&root, data.path(), &job).unwrap();
    w.write(&img(&root, "ポケモン.png"), b"b", OverwritePolicy::Never)
        .unwrap();
    w.finish().unwrap();
    assert_eq!(undo_last(data.path()).unwrap().removed.len(), 2);
}

#[test]
fn undo_reads_a_journal_torn_inside_a_multibyte_name() {
    let (_lib, data, root) = setup();
    let mut w = Writer::begin(&root, data.path()).unwrap();
    w.write(&img(&root, "ゼルダ.png"), b"a", OverwritePolicy::Never)
        .unwrap();
    let job = w.job_id().to_string();
    drop(w);
    let journal = data.path().join("jobs").join(format!("{job}.jsonl"));
    let mut bytes = fs::read(&journal).unwrap();
    bytes.extend_from_slice(&"ゼ".as_bytes()[..2]);
    fs::write(&journal, &bytes).unwrap();
    assert_eq!(undo_last(data.path()).unwrap().removed.len(), 1);
}

#[test]
fn a_resumed_write_survives_temp_files_left_by_a_crash() {
    let (_lib, data, root) = setup();
    let mut w = Writer::begin(&root, data.path()).unwrap();
    w.write(&img(&root, "a.png"), b"a", OverwritePolicy::Never)
        .unwrap();
    let job = w.job_id().to_string();
    drop(w); // a crash that left temp files behind
    for k in 0..8 {
        fs::write(img(&root, &format!(".coverly-{job}-{k}.tmp")), b"stale").unwrap();
    }
    let mut w = Writer::resume(&root, data.path(), &job).unwrap();
    w.write(&img(&root, "b.png"), b"b", OverwritePolicy::Never)
        .unwrap();
    w.finish().unwrap();
    assert_eq!(fs::read(img(&root, "b.png")).unwrap(), b"b");
}

#[test]
fn a_resumed_run_that_crashes_is_not_complete() {
    let (_lib, data, root) = setup();
    let mut w = Writer::begin(&root, data.path()).unwrap();
    w.write(&img(&root, "a.png"), b"a", OverwritePolicy::Never)
        .unwrap();
    let job = w.finish().unwrap();
    let mut w = Writer::resume(&root, data.path(), &job).unwrap();
    w.write(&img(&root, "b.png"), b"b", OverwritePolicy::Never)
        .unwrap();
    drop(w); // a crash: no second `done`
    let jobs = list_jobs(data.path()).unwrap();
    assert_eq!(jobs[0].entries, 2);
    assert!(!jobs[0].complete);
}

#[test]
fn undo_restores_a_gamelist_written_twice_in_one_job() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("lib");
    let data = tmp.path().join("data");
    fs::create_dir_all(root.join("roms/snes")).unwrap();
    let gl = root.join("roms/snes/gamelist.xml");
    fs::write(&gl, b"ORIGINAL").unwrap();
    let mut w = Writer::begin(&root, &data).unwrap();
    w.write(&gl, b"FIRST", OverwritePolicy::Backup).unwrap();
    let job = w.job_id().to_string();
    w.finish().unwrap();
    let mut w = Writer::resume(&root, &data, &job).unwrap();
    w.write(&gl, b"SECOND", OverwritePolicy::Backup).unwrap();
    w.finish().unwrap();
    let report = undo_job(&data, &job).unwrap();
    assert_eq!(fs::read(&gl).unwrap(), b"ORIGINAL");
    assert!(report.skipped_modified.is_empty(), "{report:?}");
}

#[test]
fn undo_removes_a_file_created_then_replaced_in_one_job() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("lib");
    let data = tmp.path().join("data");
    fs::create_dir_all(root.join("roms/snes")).unwrap();
    let gl = root.join("roms/snes/gamelist.xml");
    let mut w = Writer::begin(&root, &data).unwrap();
    w.write(&gl, b"FIRST", OverwritePolicy::Backup).unwrap();
    let job = w.job_id().to_string();
    w.finish().unwrap();
    let mut w = Writer::resume(&root, &data, &job).unwrap();
    w.write(&gl, b"SECOND", OverwritePolicy::Backup).unwrap();
    w.finish().unwrap();
    let report = undo_job(&data, &job).unwrap();
    assert!(!gl.exists());
    assert!(report.skipped_modified.is_empty(), "{report:?}");
}

mod extraction {
    use super::*;
    use coverly_core::write::{
        job_archive_steps, job_transfers, prune_jobs_at, ArchiveStep, ExtractedFile, JobKind,
        JobMeta,
    };
    use std::time::{Duration, SystemTime};

    fn organise(root: &Path, src: &Path, data: &Path) -> Writer {
        Writer::begin_with(
            root,
            data,
            JobMeta {
                kind: JobKind::Organize,
                source_root: Some(src.to_path_buf()),
            },
        )
        .unwrap()
    }

    #[test]
    fn job_transfers_reports_extracted() {
        let tmp = TempDir::new().unwrap();
        let (root, src, data) = (
            tmp.path().join("T"),
            tmp.path().join("S"),
            tmp.path().join("d"),
        );
        fs::create_dir_all(root.join("PS/Next Tetris, The")).unwrap();
        fs::create_dir_all(&src).unwrap();
        let archive = src.join("Next Tetris, The.zip");
        fs::write(&archive, b"7z").unwrap();
        fs::write(src.join("Tetris.gba"), b"ROM").unwrap();
        fs::create_dir_all(root.join("GBA")).unwrap();

        let mut w = organise(&root, &src, &data);
        let job = w.job_id().to_string();
        w.move_file(
            &src.join("Tetris.gba"),
            &root.join("GBA/Tetris.gba"),
            coverly_core::write::Collision::Skip,
            Some(0),
        )
        .unwrap();
        let dir = root.join("PS/Next Tetris, The");
        let mut finals = Vec::new();
        for (n, name) in ["Next Tetris, The.cue", "Track 01.bin"].iter().enumerate() {
            let file = ExtractedFile {
                from: archive.clone(),
                entry: format!("Next Tetris, The/{name}"),
                size: 3,
                crc32: None,
                temp: format!(".coverly-{job}-{n}.tmp"),
            };
            let target = dir.join(name);
            w.extract_intent(&target, &file, Some(1)).unwrap();
            fs::write(&target, b"abc").unwrap();
            w.extract_done(&target, &file, 0x352441C2, Some(1)).unwrap();
            finals.push(target);
        }
        drop(w); // a crash before the archive step

        // A resumed run keeps appending to the same journal.
        let mut w = Writer::resume(&root, &data, &job).unwrap();
        w.archive_trash_intent(&archive, Some(1)).unwrap();
        drop(w);

        let records = job_transfers(&data, &job).unwrap();
        let moved: Vec<_> = records.iter().filter(|r| r.extracted.is_none()).collect();
        assert_eq!(moved.len(), 1);
        assert!(!moved[0].copied && moved[0].finished);
        let done: Vec<_> = records
            .iter()
            .filter(|r| r.extracted.is_some() && r.finished)
            .collect();
        assert_eq!(done.len(), 2);
        for (record, target) in done.iter().zip(&finals) {
            assert_eq!(record.item, Some(1));
            assert_eq!(&record.target, target);
            assert_eq!(record.from, archive);
            assert!(!record.copied);
            assert_eq!(record.fp.unwrap().crc32, Some(0x352441C2));
            assert_eq!(record.extracted.as_ref().unwrap().crc32, None);
        }
        let steps = job_archive_steps(&data, &job).unwrap();
        assert_eq!(steps.len(), 1);
        assert!(matches!(
            steps[0].step,
            ArchiveStep::TrashIntent { fp: Some(_) }
        ));
    }

    #[test]
    fn prune_keeps_a_job_with_an_unresolved_archive_trash() {
        let tmp = TempDir::new().unwrap();
        let data = tmp.path();
        let jobs = data.join("jobs");
        fs::create_dir_all(&jobs).unwrap();
        let header = |job: &str| {
            format!(
                "{{\"type\":\"header\",\"job\":\"{job}\",\"root\":\"/lib\",\"source_root\":\"/pile\",\"kind\":\"organize\",\"created_at\":\"x\"}}\n"
            )
        };
        let trash = "{\"type\":\"entry\",\"target\":\"/pile/a.zip\",\"op\":{\"kind\":\"archive-trash\",\"from\":\"/pile/a.zip\"},\"fp\":{\"size\":2,\"mtime_ns\":0},\"item\":0}\n";
        let trashed = "{\"type\":\"entry\",\"target\":\"/pile/a.zip\",\"op\":{\"kind\":\"archive-trashed\",\"from\":\"/pile/a.zip\",\"trash\":null},\"item\":0}\n";
        let kept = "{\"type\":\"entry\",\"target\":\"/pile/b.zip\",\"op\":{\"kind\":\"archive-kept\",\"from\":\"/pile/b.zip\"},\"item\":1}\n";
        // Old jobs, beyond both the count and the age limit.
        let (open, resolved, other) = (
            "20200101T000000Z-0000-0000",
            "20200101T000001Z-0000-0000",
            "20200101T000002Z-0000-0000",
        );
        fs::write(
            jobs.join(format!("{open}.jsonl")),
            format!("{}{trash}{kept}", header(open)),
        )
        .unwrap();
        fs::write(
            jobs.join(format!("{resolved}.jsonl")),
            format!("{}{trash}{trashed}", header(resolved)),
        )
        .unwrap();
        fs::write(
            jobs.join(format!("{other}.jsonl")),
            format!("{}{kept}", header(other)),
        )
        .unwrap();
        let newest = "20261020T000000Z-0000-0000";
        fs::write(jobs.join(format!("{newest}.jsonl")), header(newest)).unwrap();

        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_792_195_200);
        prune_jobs_at(data, 1, 90, now).unwrap();
        let left: Vec<String> = list_jobs(data)
            .unwrap()
            .into_iter()
            .map(|j| j.job)
            .collect();
        assert_eq!(left, [newest, open]);
    }
}
