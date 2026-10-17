//! Undo of organise jobs, later-jobs checks and kind-aware pruning (plan
//! Task 20).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use coverly_core::identify::overrides::{remember_manual, Overrides, OVERRIDES_FILE};
use coverly_core::write::*;
use tempfile::TempDir;

/// `<tmp>/S` (source pile), `<tmp>/T` (library root), `<tmp>/d` (app data).
fn setup() -> (TempDir, PathBuf, PathBuf, PathBuf) {
    let tmp = TempDir::new().unwrap();
    let (src, root, data) = (
        tmp.path().join("S"),
        tmp.path().join("T"),
        tmp.path().join("d"),
    );
    fs::create_dir_all(&src).unwrap();
    fs::create_dir_all(&root).unwrap();
    (tmp, src, root, data)
}

fn organise_job(root: &Path, data: &Path, source: &Path) -> Writer {
    Writer::begin_with(
        root,
        data,
        JobMeta {
            kind: JobKind::Organize,
            source_root: Some(source.to_path_buf()),
        },
    )
    .unwrap()
}

fn json_path(p: &Path) -> String {
    serde_json::to_string(p.to_str().unwrap()).unwrap()
}

/// Writes a journal by hand: an organise header plus `entries` (JSON
/// lines) and `done`.
fn hand_journal(data: &Path, job: &str, root: &Path, source: &Path, entries: &[String]) {
    let jobs = data.join("jobs");
    fs::create_dir_all(&jobs).unwrap();
    let mut text = format!(
        "{{\"type\":\"header\",\"job\":\"{job}\",\"root\":{},\"source_root\":{},\"kind\":\"organize\",\"created_at\":\"2026-10-17T10:00:00Z\"}}\n",
        json_path(root),
        json_path(source)
    );
    for e in entries {
        text.push_str(e);
        text.push('\n');
    }
    text.push_str("{\"type\":\"done\"}\n");
    fs::write(jobs.join(format!("{job}.jsonl")), text).unwrap();
}

/// Moves the files `names` (created in `src` with their name as bytes)
/// into `root/ROMS/GB`; returns the job id.
fn move_into_gb(src: &Path, root: &Path, data: &Path, names: &[&str]) -> String {
    let mut w = organise_job(root, data, src);
    for name in names {
        let from = src.join(name);
        fs::write(&from, name.as_bytes()).unwrap();
        w.move_file(
            &from,
            &root.join("ROMS/GB").join(name),
            Collision::Rename,
            None,
        )
        .unwrap();
    }
    w.finish().unwrap()
}

#[test]
fn undo_moves_back_and_removes_created_folders() {
    let (_tmp, src, root, data) = setup();
    let job = move_into_gb(&src, &root, &data, &["Tetris.gb", "Dr. Mario.gb"]);
    assert!(root.join("ROMS/GB/Tetris.gb").exists());
    assert!(!src.join("Tetris.gb").exists());

    let report = undo_job(&data, &job).unwrap();
    assert_eq!(fs::read(src.join("Tetris.gb")).unwrap(), b"Tetris.gb");
    assert_eq!(fs::read(src.join("Dr. Mario.gb")).unwrap(), b"Dr. Mario.gb");
    assert!(!root.join("ROMS").exists());
    let mut restored = report.restored.clone();
    restored.sort();
    assert_eq!(restored, [src.join("Dr. Mario.gb"), src.join("Tetris.gb")]);
    assert_eq!(
        report.dirs_removed,
        [root.join("ROMS/GB"), root.join("ROMS")]
    );
    assert!(report.skipped_modified.is_empty() && report.skipped_occupied.is_empty());
    assert!(list_jobs(&data).unwrap()[0].undone);
}

#[test]
fn undo_moves_back_in_place_and_keeps_the_mtime() {
    let (_tmp, _src, root, data) = setup();
    let from = root.join("Tetris.gb");
    fs::write(&from, b"GB").unwrap();
    let old = SystemTime::UNIX_EPOCH + Duration::from_secs(1_600_000_000);
    fs::File::options()
        .write(true)
        .open(&from)
        .unwrap()
        .set_modified(old)
        .unwrap();
    let mut w = organise_job(&root, &data, &root);
    w.move_file(
        &from,
        &root.join("ROMS/GB/Tetris.gb"),
        Collision::Rename,
        None,
    )
    .unwrap();
    w.finish().unwrap();

    undo_last(&data).unwrap();
    assert_eq!(fs::read(&from).unwrap(), b"GB");
    assert_eq!(fs::metadata(&from).unwrap().modified().unwrap(), old);
    assert!(!root.join("ROMS").exists());
}

#[test]
fn undo_keeps_a_moved_file_that_was_changed() {
    let (_tmp, src, root, data) = setup();
    let job = move_into_gb(&src, &root, &data, &["Tetris.gb"]);
    let target = root.join("ROMS/GB/Tetris.gb");
    let mut f = fs::OpenOptions::new().append(true).open(&target).unwrap();
    io::Write::write_all(&mut f, b"+save").unwrap();
    drop(f);

    let report = undo_job(&data, &job).unwrap();
    assert_eq!(report.skipped_modified, std::slice::from_ref(&target));
    assert!(report.restored.is_empty());
    assert_eq!(fs::read(&target).unwrap(), b"Tetris.gb+save");
    assert!(!src.join("Tetris.gb").exists());
    // The folder still holds the user's file: kept.
    assert!(root.join("ROMS/GB").is_dir());
    assert!(report.dirs_removed.is_empty());
}

#[test]
fn undo_accepts_a_touched_but_identical_copy_by_crc() {
    let (_tmp, src, root, data) = setup();
    let from = src.join("Tetris.gb");
    fs::write(&from, b"TETRIS").unwrap();
    let mut w = organise_job(&root, &data, &src);
    let target = root.join("ROMS/GB/Tetris.gb");
    w.copy_file(&from, &target, Collision::Rename, None)
        .unwrap();
    let job = w.finish().unwrap();
    // Same bytes, new mtime: the CRC proves it is still the copy.
    let later = SystemTime::now() + Duration::from_secs(3600);
    fs::File::options()
        .write(true)
        .open(&target)
        .unwrap()
        .set_modified(later)
        .unwrap();

    let report = undo_job(&data, &job).unwrap();
    assert_eq!(report.removed, std::slice::from_ref(&target));
    assert!(!target.exists());
}

#[test]
fn undo_does_not_overwrite_a_reoccupied_source() {
    let (_tmp, src, root, data) = setup();
    let job = move_into_gb(&src, &root, &data, &["Tetris.gb"]);
    fs::write(src.join("Tetris.gb"), b"another file").unwrap();

    let report = undo_job(&data, &job).unwrap();
    let target = root.join("ROMS/GB/Tetris.gb");
    assert_eq!(report.skipped_occupied, std::slice::from_ref(&target));
    assert_eq!(fs::read(&target).unwrap(), b"Tetris.gb");
    assert_eq!(fs::read(src.join("Tetris.gb")).unwrap(), b"another file");
}

#[test]
fn undo_treats_a_case_twin_at_the_source_as_occupied() {
    let (_tmp, src, root, data) = setup();
    let job = move_into_gb(&src, &root, &data, &["Tetris.gb"]);
    fs::write(src.join("TETRIS.GB"), b"another file").unwrap();

    let report = undo_job(&data, &job).unwrap();
    assert_eq!(report.skipped_occupied, [root.join("ROMS/GB/Tetris.gb")]);
    assert!(root.join("ROMS/GB/Tetris.gb").exists());
}

#[test]
fn undo_after_a_crash_between_copy_and_delete_removes_the_copy() {
    let (_tmp, src, root, data) = setup();
    let from = src.join("Tetris.gb");
    let target = root.join("ROMS/GB/Tetris.gb");
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(&from, b"TETRIS").unwrap();
    fs::write(&target, b"TETRIS").unwrap();
    // A crash-left temp of the job next to the target.
    let job = "20261017T100000Z-0001-0000";
    let temp = root.join("ROMS/GB").join(format!(".coverly-{job}-9.tmp"));
    fs::write(&temp, b"TET").unwrap();
    hand_journal(
        &data,
        job,
        &root,
        &src,
        &[
            format!(
                "{{\"type\":\"entry\",\"target\":{},\"op\":{{\"kind\":\"dir-created\"}}}}",
                json_path(&root.join("ROMS"))
            ),
            format!(
                "{{\"type\":\"entry\",\"target\":{},\"op\":{{\"kind\":\"dir-created\"}}}}",
                json_path(&root.join("ROMS/GB"))
            ),
            format!(
                "{{\"type\":\"entry\",\"target\":{},\"op\":{{\"kind\":\"moved\",\"from\":{},\"via\":\"copy\"}}}}",
                json_path(&target),
                json_path(&from)
            ),
        ],
    );

    let report = undo_job(&data, job).unwrap();
    assert!(!target.exists());
    assert_eq!(report.restored, std::slice::from_ref(&from));
    assert_eq!(fs::read(&from).unwrap(), b"TETRIS");
    assert!(!temp.exists(), "the job's temp is cleaned with its folder");
    assert!(!root.join("ROMS").exists());
}

#[test]
fn an_intent_without_a_copy_is_a_noop() {
    let (_tmp, src, root, data) = setup();
    let from = src.join("Tetris.gb");
    fs::write(&from, b"TETRIS").unwrap();
    let target = root.join("Tetris.gb");
    // Someone else's different file at the target: never deleted.
    fs::write(&target, b"OTHER!").unwrap();
    let job = "20261017T100000Z-0001-0001";
    hand_journal(
        &data,
        job,
        &root,
        &src,
        &[format!(
            "{{\"type\":\"entry\",\"target\":{},\"op\":{{\"kind\":\"copied\",\"from\":{}}}}}",
            json_path(&target),
            json_path(&from)
        )],
    );
    let report = undo_job(&data, job).unwrap();
    assert!(report.removed.is_empty() && report.restored.is_empty());
    assert_eq!(fs::read(&target).unwrap(), b"OTHER!");
    assert_eq!(fs::read(&from).unwrap(), b"TETRIS");
}

#[test]
fn undo_of_a_cross_volume_move_streams_the_file_back() {
    let (_tmp, src, root, data) = setup();
    let from = src.join("Tetris.gb");
    fs::write(&from, b"TETRIS").unwrap();
    let target = root.join("ROMS/GB/Tetris.gb");
    let mut w = organise_job(&root, &data, &src);
    w.move_file(&from, &target, Collision::Rename, None)
        .unwrap();
    let job = w.finish().unwrap();

    // The rename back fails as if the volumes differed.
    set_copy_fault(Some(|stage, _| match stage {
        CopyStage::Rename => Some(io::Error::from_raw_os_error(if cfg!(windows) {
            17
        } else {
            18
        })),
        _ => None,
    }));
    let report = undo_job(&data, &job);
    set_copy_fault(None);
    let report = report.unwrap();
    assert_eq!(report.restored, std::slice::from_ref(&from));
    assert_eq!(fs::read(&from).unwrap(), b"TETRIS");
    assert!(!target.exists());
    assert!(!root.join("ROMS").exists());
    let litter: Vec<_> = fs::read_dir(&src)
        .unwrap()
        .flatten()
        .map(|e| e.file_name())
        .filter(|n| n != "Tetris.gb")
        .collect();
    assert!(litter.is_empty(), "{litter:?}");
}

#[test]
fn undo_of_a_copy_deletes_only_an_unchanged_target() {
    let (_tmp, src, root, data) = setup();
    let mut w = organise_job(&root, &data, &src);
    let (a, b) = (src.join("a.gb"), src.join("b.gb"));
    fs::write(&a, b"AAAA").unwrap();
    fs::write(&b, b"BBBB").unwrap();
    let (ta, tb) = (root.join("GB/a.gb"), root.join("GB/b.gb"));
    w.copy_file(&a, &ta, Collision::Rename, None).unwrap();
    w.copy_file(&b, &tb, Collision::Rename, None).unwrap();
    let job = w.finish().unwrap();
    fs::write(&tb, b"BBBB but edited").unwrap();

    let report = undo_job(&data, &job).unwrap();
    assert_eq!(report.removed, std::slice::from_ref(&ta));
    assert_eq!(report.skipped_modified, std::slice::from_ref(&tb));
    assert!(!ta.exists() && tb.exists());
    assert_eq!(fs::read(&a).unwrap(), b"AAAA");
    assert_eq!(fs::read(&b).unwrap(), b"BBBB");
    assert!(root.join("GB").is_dir(), "still holds the edited file");
}

#[test]
fn a_folder_with_macos_litter_is_still_removed() {
    let (_tmp, src, root, data) = setup();
    let job = move_into_gb(&src, &root, &data, &["Tetris.gb"]);
    fs::write(root.join("ROMS/GB/.DS_Store"), b"finder").unwrap();
    fs::write(root.join("ROMS/GB/._Tetris.gb"), [0u8, 5, 22, 7, 0, 0]).unwrap();
    fs::write(root.join("ROMS/._GB"), [0u8, 5, 22, 7, 1]).unwrap();

    let report = undo_job(&data, &job).unwrap();
    assert!(!root.join("ROMS").exists());
    assert_eq!(report.dirs_removed.len(), 2);
}

#[test]
fn a_folder_with_user_files_is_kept() {
    let (_tmp, src, root, data) = setup();
    let job = move_into_gb(&src, &root, &data, &["Tetris.gb"]);
    fs::write(root.join("ROMS/GB/notes.txt"), b"mine").unwrap();
    fs::write(root.join("ROMS/GB/.DS_Store"), b"finder").unwrap();

    let report = undo_job(&data, &job).unwrap();
    assert!(src.join("Tetris.gb").exists());
    assert_eq!(fs::read(root.join("ROMS/GB/notes.txt")).unwrap(), b"mine");
    assert!(
        root.join("ROMS/GB/.DS_Store").exists(),
        "nothing is cleaned in a kept folder"
    );
    assert!(report.dirs_removed.is_empty());
}

#[test]
fn undo_needs_the_source_volume() {
    let (tmp, src, root, data) = setup();
    let job = move_into_gb(&src, &root, &data, &["Tetris.gb"]);
    let away = tmp.path().join("ejected");
    fs::rename(&src, &away).unwrap();

    assert!(matches!(
        undo_job(&data, &job),
        Err(WriteError::Denied(p)) if p == src
    ));
    assert!(root.join("ROMS/GB/Tetris.gb").exists());
    assert!(!list_jobs(&data).unwrap()[0].undone);

    // Plugged back in: the undo works.
    fs::rename(&away, &src).unwrap();
    undo_job(&data, &job).unwrap();
    assert!(src.join("Tetris.gb").exists());
}

#[test]
fn undo_needs_the_target_volume() {
    let (tmp, src, root, data) = setup();
    let job = move_into_gb(&src, &root, &data, &["Tetris.gb"]);
    let away = tmp.path().join("ejected");
    fs::rename(&root, &away).unwrap();
    assert!(matches!(
        undo_job(&data, &job),
        Err(WriteError::Denied(p)) if p == root
    ));
    assert!(!list_jobs(&data).unwrap()[0].undone);
}

#[test]
fn undo_refuses_when_later_jobs_wrote_under_the_target() {
    let (_tmp, src, root, data) = setup();
    let organise = move_into_gb(&src, &root, &data, &["Tetris.gb"]);
    let mut fill = Writer::begin(&root, &data).unwrap();
    fill.write(
        &root.join("ROMS/GB/Imgs/Tetris.png"),
        b"art",
        OverwritePolicy::Never,
    )
    .unwrap();
    let fill = fill.finish().unwrap();
    // A job elsewhere is not a later job of this target.
    let other_root = src.join("elsewhere");
    fs::create_dir_all(&other_root).unwrap();
    Writer::begin(&other_root, &data).unwrap().finish().unwrap();

    let later: Vec<String> = later_jobs(&data, &organise)
        .unwrap()
        .into_iter()
        .map(|j| j.job)
        .collect();
    assert_eq!(later, std::slice::from_ref(&fill));
    assert!(later_jobs(&data, &fill).unwrap().is_empty());

    let reports = undo_jobs_in_order(&data, &[organise.clone(), fill.clone()]).unwrap();
    assert_eq!(
        reports.iter().map(|r| r.job.clone()).collect::<Vec<_>>(),
        [fill.clone(), organise.clone()],
        "newest first"
    );
    assert!(src.join("Tetris.gb").exists());
    assert!(!root.join("ROMS").exists());
    assert!(later_jobs(&data, &organise).unwrap().is_empty());
}

#[test]
fn undo_jobs_in_order_checks_every_job_first() {
    let (_tmp, src, root, data) = setup();
    let organise = move_into_gb(&src, &root, &data, &["Tetris.gb"]);
    assert!(matches!(
        undo_jobs_in_order(
            &data,
            &[organise.clone(), "20990101T000000Z-0000-0000".into()]
        ),
        Err(WriteError::NothingToUndo)
    ));
    assert!(root.join("ROMS/GB/Tetris.gb").exists());
}

#[test]
fn undo_restores_moved_identity_overrides() {
    let (_tmp, src, root, data) = setup();
    let from = src.join("Tetris 2.zip");
    let to = root.join("ROMS/GB/Tetris 2.zip");
    remember_manual(&data, &to, "Tetris 2 (USA)", "libretro").unwrap();
    let job = "20261017T100000Z-0001-0002";
    hand_journal(
        &data,
        job,
        &root,
        &src,
        &[format!(
            "{{\"type\":\"entry\",\"target\":{},\"op\":{{\"kind\":\"override-moved\",\"from\":{},\"to\":{}}}}}",
            json_path(&to),
            json_path(&from),
            json_path(&to)
        )],
    );
    undo_job(&data, job).unwrap();
    let o = Overrides::read(&data.join(OVERRIDES_FILE));
    assert_eq!(o.get(&from).unwrap().name, "Tetris 2 (USA)");
    assert!(o.get(&to).is_none());
}

/// A journal named `job` of `kind` with no entries.
fn empty_journal(data: &Path, job: &str, kind: &str) {
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

fn job_at(day: u32, n: u32) -> String {
    format!("202607{day:02}T0000{n:02}Z-0000-0000")
}

#[test]
fn organise_journals_survive_thirty_fill_jobs() {
    let tmp = TempDir::new().unwrap();
    let data = tmp.path();
    // 2026-10-17: 91 days after 2026-07-18.
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_792_195_200);
    // 21 organise journals: one 91 days old, twenty younger than 90 days
    // but still "older" in id order than the fills.
    let old_organise = job_at(18, 0);
    empty_journal(data, &old_organise, "organize");
    fs::write(
        data.join("jobs").join(format!("{old_organise}.plan.json")),
        "{}",
    )
    .unwrap();
    fs::create_dir_all(data.join("backups").join(&old_organise)).unwrap();
    let mut young = Vec::new();
    for n in 0..20 {
        let job = format!("202609{:02}T000000Z-0000-0000", n + 1);
        empty_journal(data, &job, "organize");
        young.push(job);
    }
    // 30 fill journals, the newest.
    let mut fills = Vec::new();
    for n in 0..30 {
        let job = format!("20261016T0000{n:02}Z-0000-0000");
        empty_journal(data, &job, "fill");
        fills.push(job);
    }

    prune_jobs_at(data, 20, 90, now).unwrap();
    let left: Vec<String> = list_jobs(data)
        .unwrap()
        .into_iter()
        .map(|j| j.job)
        .collect();
    for job in &young {
        assert!(left.contains(job), "{job} kept");
    }
    assert_eq!(left.iter().filter(|j| fills.contains(j)).count(), 20);
    assert!(left.contains(&fills[29]) && !left.contains(&fills[0]));
    assert!(!left.contains(&old_organise));
    assert!(!data
        .join("jobs")
        .join(format!("{old_organise}.plan.json"))
        .exists());
    assert!(!data.join("backups").join(&old_organise).exists());
}

#[test]
fn a_young_organise_journal_beyond_the_count_is_kept() {
    let tmp = TempDir::new().unwrap();
    let data = tmp.path();
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_792_195_200); // 2026-10-17
    let (a, b, c) = (job_at(20, 0), job_at(20, 1), job_at(10, 0));
    for job in [&a, &b, &c] {
        empty_journal(data, job, "organize");
    }
    // keep 1: `b` is the newest; `a` is younger than 90 days; `c` is not.
    prune_jobs_at(data, 1, 90, now).unwrap();
    let left: Vec<String> = list_jobs(data)
        .unwrap()
        .into_iter()
        .map(|j| j.job)
        .collect();
    assert_eq!(left, [b, a]);
}

#[test]
fn prune_jobs_keeps_both_kinds() {
    let tmp = TempDir::new().unwrap();
    let data = tmp.path();
    empty_journal(data, "20200101T000000Z-0000-0000", "organize");
    for n in 0..3 {
        empty_journal(data, &format!("20261016T00000{n}Z-0000-0000"), "fill");
    }
    prune_jobs(data, 2).unwrap();
    let kinds: Vec<JobKind> = list_jobs(data)
        .unwrap()
        .into_iter()
        .map(|j| j.kind)
        .collect();
    assert_eq!(kinds, [JobKind::Fill, JobKind::Fill, JobKind::Organize]);
}

#[test]
fn a_manual_identity_stays_with_a_file_that_was_not_moved_back() {
    let (_tmp, src, root, data) = setup();
    let from = src.join("Tetris 2.zip");
    let to = root.join("ROMS/GB/Tetris 2.zip");
    fs::create_dir_all(to.parent().unwrap()).unwrap();
    // Changed after the job (size differs from the fingerprint): it stays.
    fs::write(&to, b"changed").unwrap();
    remember_manual(&data, &to, "Tetris 2 (USA)", "libretro").unwrap();
    let job = "20261017T100000Z-0001-0003";
    hand_journal(
        &data,
        job,
        &root,
        &src,
        &[
            format!(
                "{{\"type\":\"entry\",\"target\":{},\"op\":{{\"kind\":\"moved\",\"from\":{},\"via\":\"rename\"}},\"fp\":{{\"size\":999,\"mtime_ns\":0}}}}",
                json_path(&to),
                json_path(&from)
            ),
            format!(
                "{{\"type\":\"entry\",\"target\":{},\"op\":{{\"kind\":\"override-moved\",\"from\":{},\"to\":{}}}}}",
                json_path(&to),
                json_path(&from),
                json_path(&to)
            ),
        ],
    );
    let report = undo_job(&data, job).unwrap();
    assert_eq!(report.skipped_modified, std::slice::from_ref(&to));
    let o = Overrides::read(&data.join(OVERRIDES_FILE));
    assert_eq!(o.get(&to).unwrap().name, "Tetris 2 (USA)");
    assert!(o.get(&from).is_none());
}

#[test]
fn a_later_organise_of_the_target_is_a_later_job() {
    let (tmp, src, root, data) = setup();
    let first = move_into_gb(&src, &root, &data, &["Tetris.gb"]);
    // Organise the organised tree again, somewhere unrelated.
    let elsewhere = tmp.path().join("U");
    fs::create_dir_all(&elsewhere).unwrap();
    let mut w = organise_job(&elsewhere, &data, &root);
    w.move_file(
        &root.join("ROMS/GB/Tetris.gb"),
        &elsewhere.join("GB/Tetris.gb"),
        Collision::Rename,
        None,
    )
    .unwrap();
    let second = w.finish().unwrap();
    let later: Vec<String> = later_jobs(&data, &first)
        .unwrap()
        .into_iter()
        .map(|j| j.job)
        .collect();
    assert_eq!(later, std::slice::from_ref(&second));
    undo_jobs_in_order(&data, &[first, second]).unwrap();
    assert!(src.join("Tetris.gb").exists());
}
