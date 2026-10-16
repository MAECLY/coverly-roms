use std::fs;
use std::path::{Path, PathBuf};

use coverly_core::write::{
    list_jobs, undo_last, OverwritePolicy, RemoveOutcome, WriteError, Writer,
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
