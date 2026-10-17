//! Organise apply: writer-level moves and copies first (plan Task 19).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use coverly_core::write::*;
use tempfile::TempDir;

/// A job dir pair: `<tmp>/T` (library root), `<tmp>/d` (app data).
fn setup() -> (TempDir, PathBuf, PathBuf) {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("T");
    fs::create_dir_all(&root).unwrap();
    let data = tmp.path().join("d");
    (tmp, root, data)
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

/// The journal lines after the header, as JSON values.
fn journal_entries(data: &Path, job: &str) -> Vec<serde_json::Value> {
    let text = fs::read_to_string(data.join("jobs").join(format!("{job}.jsonl"))).unwrap();
    text.lines()
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
        .filter(|v| v["type"] == "entry")
        .collect()
}

fn op_kinds(entries: &[serde_json::Value]) -> Vec<String> {
    entries
        .iter()
        .map(|e| e["op"]["kind"].as_str().unwrap().to_string())
        .collect()
}

fn names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn mtime(p: &Path) -> SystemTime {
    fs::metadata(p).unwrap().modified().unwrap()
}

#[test]
fn move_in_place_renames_and_journals_before_acting() {
    let (_tmp, root, data) = setup();
    let from = root.join("Sonic Advance 3.gba");
    fs::write(&from, b"GBA-ROM").unwrap();
    let to = root.join("ROMS/GBA/Sonic Advance 3.gba");

    let mut w = organise_job(&root, &data, &root);
    let job = w.job_id().to_string();
    let out = w.move_file(&from, &to, Collision::Rename, Some(7)).unwrap();
    assert_eq!(
        out,
        MoveOutcome::Moved {
            to: to.clone(),
            renamed: false
        }
    );
    assert!(w.has_entries());
    w.finish().unwrap();

    assert!(!from.exists());
    assert_eq!(fs::read(&to).unwrap(), b"GBA-ROM");
    let entries = journal_entries(&data, &job);
    assert_eq!(op_kinds(&entries), ["dir-created", "dir-created", "moved"]);
    assert_eq!(entries[0]["target"], root.join("ROMS").to_str().unwrap());
    assert_eq!(
        entries[1]["target"],
        root.join("ROMS/GBA").to_str().unwrap()
    );
    assert_eq!(entries[2]["op"]["via"], "rename");
    assert_eq!(entries[2]["op"]["from"], from.to_str().unwrap());
    assert_eq!(entries[2]["item"], 7);
    assert_eq!(entries[2]["fp"]["size"], 7);
}

#[test]
fn ensure_dir_journals_only_the_folders_it_creates() {
    let (_tmp, root, data) = setup();
    fs::create_dir_all(root.join("ROMS")).unwrap();
    let mut w = organise_job(&root, &data, &root);
    let job = w.job_id().to_string();
    w.ensure_dir(&root.join("ROMS/GB/Imgs")).unwrap();
    w.ensure_dir(&root.join("ROMS/GB/Imgs")).unwrap();
    w.finish().unwrap();
    assert!(root.join("ROMS/GB/Imgs").is_dir());
    let entries = journal_entries(&data, &job);
    assert_eq!(op_kinds(&entries), ["dir-created", "dir-created"]);
    assert_eq!(entries[0]["target"], root.join("ROMS/GB").to_str().unwrap());
}

#[test]
fn ensure_dir_refuses_a_folder_outside_the_root() {
    let (tmp, root, data) = setup();
    let mut w = organise_job(&root, &data, &root);
    assert!(matches!(
        w.ensure_dir(&tmp.path().join("elsewhere")),
        Err(WriteError::OutsideRoot(_))
    ));
}

#[test]
fn a_source_outside_the_source_root_is_refused() {
    let (tmp, root, data) = setup();
    let src = tmp.path().join("S");
    fs::create_dir_all(&src).unwrap();
    let stray = tmp.path().join("stray.gb");
    fs::write(&stray, b"x").unwrap();
    let mut w = organise_job(&root, &data, &src);
    assert!(matches!(
        w.move_file(&stray, &root.join("GB/stray.gb"), Collision::Rename, None),
        Err(WriteError::OutsideRoot(_))
    ));
    assert!(stray.exists());
}

#[test]
fn a_folder_source_is_refused() {
    let (_tmp, root, data) = setup();
    fs::create_dir_all(root.join("game.gb")).unwrap();
    let mut w = organise_job(&root, &data, &root);
    assert!(matches!(
        w.move_file(
            &root.join("game.gb"),
            &root.join("GB/game.gb"),
            Collision::Rename,
            None
        ),
        Err(WriteError::NotAFile(_))
    ));
}

#[test]
fn copy_keeps_the_source_and_the_mtime() {
    let (tmp, root, data) = setup();
    let src = tmp.path().join("S");
    fs::create_dir_all(&src).unwrap();
    let from = src.join("Tetris (JUE) (V1.0) [!].gb");
    let bytes: Vec<u8> = (0..3_000_000u32).map(|i| (i % 251) as u8).collect();
    fs::write(&from, &bytes).unwrap();
    let old = SystemTime::now() - Duration::from_secs(86_400 * 30);
    filetime::set_file_mtime(&from, filetime::FileTime::from_system_time(old)).unwrap();
    let to = root.join("ROMS/GB/Tetris (JUE) (V1.0) [!].gb");

    let mut w = organise_job(&root, &data, &src);
    let job = w.job_id().to_string();
    let out = w.copy_file(&from, &to, Collision::Rename, Some(0)).unwrap();
    assert_eq!(
        out,
        MoveOutcome::Copied {
            to: to.clone(),
            renamed: false
        }
    );
    w.finish().unwrap();

    assert_eq!(fs::read(&from).unwrap(), bytes, "source untouched");
    assert_eq!(fs::read(&to).unwrap(), bytes);
    let drift = mtime(&to)
        .duration_since(old)
        .unwrap_or_else(|e| e.duration());
    assert!(drift <= Duration::from_secs(2), "mtime kept: {drift:?}");

    let entries = journal_entries(&data, &job);
    assert_eq!(
        op_kinds(&entries),
        ["dir-created", "dir-created", "copied", "copied"]
    );
    assert!(
        entries[2].get("fp").is_none(),
        "intent entry before copying"
    );
    let fp = &entries[3]["fp"];
    assert_eq!(fp["size"], bytes.len() as u64);
    assert_eq!(fp["crc32"], crc32fast::hash(&bytes));
}

#[test]
fn copy_verifies_by_rereading_and_keeps_the_source_on_mismatch() {
    let (tmp, root, data) = setup();
    let src = tmp.path().join("S");
    fs::create_dir_all(&src).unwrap();
    let from = src.join("Tetris Worlds (USA).gba");
    fs::write(&from, b"GBA-BYTES").unwrap();
    fs::create_dir_all(root.join("GBA")).unwrap();
    let to = root.join("GBA/Tetris Worlds (USA).gba");

    set_copy_fault(Some(|stage, _| {
        (stage == CopyStage::Verify).then(|| io::Error::new(io::ErrorKind::InvalidData, "corrupt"))
    }));
    let mut w = organise_job(&root, &data, &src);
    let job = w.job_id().to_string();
    let err = w
        .copy_file(&from, &to, Collision::Rename, None)
        .unwrap_err();
    set_copy_fault(None);
    assert!(matches!(err, WriteError::Io(_)), "{err:?}");
    assert!(!w.has_entries());
    w.finish().unwrap();

    assert!(!to.exists(), "no target left");
    assert_eq!(fs::read(&from).unwrap(), b"GBA-BYTES");
    assert!(names_in(&root.join("GBA")).is_empty(), "no temp left");
    let entries = journal_entries(&data, &job);
    assert_eq!(op_kinds(&entries), ["copied"]);
    assert!(entries[0].get("fp").is_none());
}

#[test]
fn a_cross_volume_move_copies_then_deletes_the_source() {
    let (tmp, root, data) = setup();
    let src = tmp.path().join("S");
    fs::create_dir_all(&src).unwrap();
    let from = src.join("Tetris 2.nes");
    fs::write(&from, b"NES\x1a....").unwrap();
    let to = root.join("FC/Tetris 2.nes");

    set_copy_fault(Some(|stage, _| {
        (stage == CopyStage::Rename).then(|| io::Error::from(io::ErrorKind::CrossesDevices))
    }));
    let mut w = organise_job(&root, &data, &src);
    let job = w.job_id().to_string();
    let out = w.move_file(&from, &to, Collision::Rename, Some(1));
    set_copy_fault(None);
    assert_eq!(
        out.unwrap(),
        MoveOutcome::Moved {
            to: to.clone(),
            renamed: false
        }
    );
    w.finish().unwrap();
    assert!(!from.exists());
    assert_eq!(fs::read(&to).unwrap(), b"NES\x1a....");
    let entries = journal_entries(&data, &job);
    assert_eq!(
        op_kinds(&entries),
        ["dir-created", "moved", "moved", "moved"]
    );
    assert_eq!(entries[1]["op"]["via"], "rename");
    assert_eq!(entries[2]["op"]["via"], "copy");
    assert!(entries[2].get("fp").is_none());
    assert_eq!(entries[3]["op"]["via"], "copy");
    assert_eq!(entries[3]["fp"]["crc32"], crc32fast::hash(b"NES\x1a...."));
}

#[test]
fn a_failed_source_delete_is_reported_as_copied() {
    let (tmp, root, data) = setup();
    let src = tmp.path().join("S");
    fs::create_dir_all(&src).unwrap();
    let from = src.join("Tetris Attack.sfc");
    fs::write(&from, b"SNES").unwrap();
    let to = root.join("SFC/Tetris Attack.sfc");

    set_copy_fault(Some(|stage, _| match stage {
        CopyStage::Rename => Some(io::Error::from_raw_os_error(if cfg!(windows) {
            17
        } else {
            18
        })),
        CopyStage::Delete => Some(io::Error::from(io::ErrorKind::PermissionDenied)),
        CopyStage::Verify => None,
    }));
    let mut w = organise_job(&root, &data, &src);
    let job = w.job_id().to_string();
    let out = w.move_file(&from, &to, Collision::Rename, None);
    set_copy_fault(None);
    assert_eq!(
        out.unwrap(),
        MoveOutcome::CopiedNotRemoved {
            to: to.clone(),
            renamed: false
        }
    );
    w.finish().unwrap();
    assert!(from.exists() && to.exists());
    let entries = journal_entries(&data, &job);
    assert_eq!(op_kinds(&entries).last().unwrap(), "copied");
    assert_eq!(
        entries.last().unwrap()["fp"],
        entries[entries.len() - 2]["fp"]
    );
}

#[test]
fn an_existing_target_is_renamed_or_skipped() {
    let (_tmp, root, data) = setup();
    fs::create_dir_all(root.join("GB")).unwrap();
    fs::write(root.join("GB/tetris.gb"), b"EXISTING").unwrap();
    fs::write(root.join("a.gb"), b"A").unwrap();
    fs::write(root.join("b.gb"), b"B").unwrap();

    let mut w = organise_job(&root, &data, &root);
    let out = w
        .move_file(
            &root.join("a.gb"),
            &root.join("GB/Tetris.gb"),
            Collision::Rename,
            None,
        )
        .unwrap();
    assert_eq!(
        out,
        MoveOutcome::Moved {
            to: root.join("GB/Tetris (2).gb"),
            renamed: true
        }
    );
    let out = w
        .move_file(
            &root.join("b.gb"),
            &root.join("GB/Tetris.gb"),
            Collision::Skip,
            None,
        )
        .unwrap();
    assert_eq!(
        out,
        MoveOutcome::SkippedExists {
            existing: root.join("GB/tetris.gb")
        }
    );
    w.finish().unwrap();
    assert_eq!(fs::read(root.join("GB/tetris.gb")).unwrap(), b"EXISTING");
    assert_eq!(fs::read(root.join("GB/Tetris (2).gb")).unwrap(), b"A");
    assert_eq!(fs::read(root.join("b.gb")).unwrap(), b"B");
}

#[test]
fn a_case_only_rename_is_a_noop() {
    let (_tmp, root, data) = setup();
    fs::create_dir_all(root.join("x")).unwrap();
    let from = root.join("x/tetris.gb");
    fs::write(&from, b"T").unwrap();
    set_copy_fault(Some(|stage, _| {
        (stage == CopyStage::Rename).then(|| io::Error::other("no rename expected"))
    }));
    let mut w = organise_job(&root, &data, &root);
    let job = w.job_id().to_string();
    let out = w.move_file(&from, &root.join("x/Tetris.gb"), Collision::Rename, None);
    set_copy_fault(None);
    assert_eq!(
        out.unwrap(),
        MoveOutcome::Moved {
            to: from.clone(),
            renamed: false
        }
    );
    assert!(!w.has_entries());
    w.finish().unwrap();
    assert_eq!(names_in(&root.join("x")), ["tetris.gb"]);
    assert!(journal_entries(&data, &job).is_empty());
}

#[test]
fn no_appledouble_or_temp_files_are_left() {
    let (tmp, root, data) = setup();
    let src = tmp.path().join("S");
    fs::create_dir_all(&src).unwrap();
    for n in ["a.gba", "b.gba"] {
        fs::write(src.join(n), n.as_bytes()).unwrap();
    }
    let mut w = organise_job(&root, &data, &src);
    w.copy_file(
        &src.join("a.gba"),
        &root.join("GBA/a.gba"),
        Collision::Rename,
        None,
    )
    .unwrap();
    w.copy_file(
        &src.join("b.gba"),
        &root.join("GBA/b.gba"),
        Collision::Rename,
        None,
    )
    .unwrap();
    w.finish().unwrap();
    let names = names_in(&root.join("GBA"));
    assert_eq!(names, ["a.gba", "b.gba"]);
    assert!(!names
        .iter()
        .any(|n| n.starts_with("._") || n.ends_with(".tmp")));
}

#[test]
fn clean_temps_removes_only_this_jobs_temps() {
    let (_tmp, root, data) = setup();
    let dir = root.join("GBA");
    fs::create_dir_all(&dir).unwrap();
    let w = organise_job(&root, &data, &root);
    let mine = format!(".coverly-{}-3.tmp", w.job_id());
    fs::write(dir.join(&mine), b"partial").unwrap();
    fs::write(dir.join(".coverly-other-job-1.tmp"), b"other").unwrap();
    fs::write(dir.join("keep.gba"), b"rom").unwrap();
    w.clean_temps(std::slice::from_ref(&dir));
    assert_eq!(names_in(&dir), [".coverly-other-job-1.tmp", "keep.gba"]);
}

/// Whether `dir` lies on a case-insensitive file system (APFS default,
/// FAT, exFAT, NTFS).
fn case_insensitive(dir: &Path) -> bool {
    let probe = dir.join("CaseProbe");
    fs::write(&probe, b"").unwrap();
    let insensitive = dir.join("caseprobe").exists();
    fs::remove_file(&probe).unwrap();
    insensitive
}

#[test]
fn a_target_folder_differing_only_by_case_is_a_noop() {
    // ROMS/gba/x.gba asked to go to ROMS/GBA/x.gba on a case-insensitive
    // volume: it is already there, not a collision with itself.
    let (_tmp, root, data) = setup();
    if !case_insensitive(&root) {
        return;
    }
    fs::create_dir_all(root.join("ROMS/gba")).unwrap();
    let from = root.join("ROMS/gba/x.gba");
    fs::write(&from, b"X").unwrap();
    for collision in [Collision::Rename, Collision::Skip] {
        let mut w = organise_job(&root, &data, &root);
        let job = w.job_id().to_string();
        let out = w.move_file(&from, &root.join("ROMS/GBA/x.gba"), collision, None);
        assert_eq!(
            out.unwrap(),
            MoveOutcome::Moved {
                to: from.clone(),
                renamed: false
            },
            "{collision:?}"
        );
        w.finish().unwrap();
        assert_eq!(names_in(&root.join("ROMS/gba")), ["x.gba"]);
        assert!(journal_entries(&data, &job).is_empty());
    }
}
