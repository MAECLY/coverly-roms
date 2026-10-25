//! Applying a duplicates plan: copies set aside in the quarantine or the
//! Trash, journaled and undoable (duplicates spec §7–§9).

use std::fs;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicIsize, Ordering};
use std::time::{Duration, SystemTime};

use coverly_core::dedupe::apply::{trash_file, TrashKept, TrashStep};
use coverly_core::dedupe::testing::pin_mtime;
use coverly_core::write::trash::FakeTrash;
use coverly_core::write::{
    job_archive_steps, list_jobs, prune_jobs_at, set_journal_fault, undo_job_with, ArchiveStep,
    JobKind, JobMeta, Writer,
};
use tempfile::TempDir;

struct Lib {
    tmp: TempDir,
    root: PathBuf,
    data: PathBuf,
}

impl Lib {
    fn new() -> Lib {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("SD");
        let data = tmp.path().join("data");
        fs::create_dir_all(root.join("ROMS/GB")).unwrap();
        Lib { tmp, root, data }
    }

    fn trash_dir(&self) -> PathBuf {
        self.tmp.path().join("trash")
    }

    /// `ROMS/<rel>` with `bytes`, its mtime pinned to `n`.
    fn file(&self, rel: &str, bytes: &[u8], n: u64) -> PathBuf {
        let path = self.root.join("ROMS").join(rel);
        fs::write(&path, bytes).unwrap();
        pin_mtime(&path, n);
        path
    }

    fn writer(&self) -> Writer {
        Writer::begin_with(
            &self.root,
            &self.data,
            JobMeta {
                kind: JobKind::Dedupe,
                source_root: None,
            },
        )
        .unwrap()
    }
}

fn entries_of(dir: &Path) -> usize {
    fs::read_dir(dir).map_or(0, |d| d.count())
}

#[test]
fn trashed_rom_and_companion_come_back_on_undo() {
    let lib = Lib::new();
    let rom = lib.file("GB/Tetris (1).gb", b"tetris rom bytes", 1);
    let sav = lib.file("GB/Tetris (1).sav", b"battery save", 2);
    let trash = FakeTrash::new(lib.trash_dir());

    let mut w = lib.writer();
    for path in [&rom, &sav] {
        let step = trash_file(&mut w, &trash, path, Some(0)).unwrap();
        assert!(
            matches!(step, TrashStep::Trashed { location: Some(_) }),
            "{step:?}"
        );
        assert!(!path.exists());
    }
    let job = w.finish().unwrap();
    assert_eq!(trash.trashed(), 2);
    let listed = list_jobs(&lib.data).unwrap();
    assert_eq!(listed[0].kind, JobKind::Dedupe);

    let report = undo_job_with(&lib.data, &job, &trash).unwrap();
    assert_eq!(fs::read(&rom).unwrap(), b"tetris rom bytes");
    assert_eq!(fs::read(&sav).unwrap(), b"battery save");
    let mut restored = report.restored.clone();
    restored.sort();
    assert_eq!(restored, [rom, sav]);
    assert!(report.archive_lost.is_empty(), "{report:?}");
}

#[test]
fn trash_intent_is_journaled_before_the_call() {
    let lib = Lib::new();
    let rom = lib.file("GB/Tetris (1).gb", b"tetris", 1);
    let size = fs::metadata(&rom).unwrap().len();
    let trash = FakeTrash::new(lib.trash_dir()).fail_after(0);

    let mut w = lib.writer();
    let step = trash_file(&mut w, &trash, &rom, Some(3)).unwrap();
    assert!(
        matches!(
            step,
            TrashStep::Kept {
                why: TrashKept::Refused
            }
        ),
        "{step:?}"
    );
    let job = w.finish().unwrap();
    assert_eq!(fs::read(&rom).unwrap(), b"tetris");

    let steps = job_archive_steps(&lib.data, &job).unwrap();
    assert_eq!(steps.len(), 2, "{steps:?}");
    assert_eq!(steps[0].from, rom);
    assert_eq!(steps[0].item, Some(3));
    let ArchiveStep::TrashIntent { fp: Some(fp) } = steps[0].step else {
        panic!("{steps:?}");
    };
    assert_eq!(fp.size, size);
    assert_eq!(steps[1].step, ArchiveStep::Kept);
}

#[test]
fn volume_without_trash_keeps_the_file() {
    let lib = Lib::new();
    let rom = lib.file("GB/Tetris (1).gb", b"tetris", 1);
    let trash = FakeTrash::unavailable(lib.trash_dir());

    let mut w = lib.writer();
    let step = trash_file(&mut w, &trash, &rom, None).unwrap();
    assert!(
        matches!(
            step,
            TrashStep::Kept {
                why: TrashKept::Unavailable
            }
        ),
        "{step:?}"
    );
    w.finish().unwrap();
    assert_eq!(fs::read(&rom).unwrap(), b"tetris");
    assert_eq!(entries_of(&lib.trash_dir()), 0);
}

#[test]
fn a_missing_file_is_gone_not_an_error() {
    let lib = Lib::new();
    let rom = lib.root.join("ROMS/GB/Tetris (1).gb");
    let trash = FakeTrash::new(lib.trash_dir());

    let mut w = lib.writer();
    let step = trash_file(&mut w, &trash, &rom, None).unwrap();
    assert!(matches!(step, TrashStep::Gone), "{step:?}");
    assert_eq!(trash.trashed(), 0);
}

#[test]
fn trash_emptied_is_reported_lost_not_an_error() {
    let lib = Lib::new();
    let rom = lib.file("GB/Tetris (1).gb", b"tetris", 1);
    let sav = lib.file("GB/Tetris (1).sav", b"save", 2);
    let trash = FakeTrash::new(lib.trash_dir());

    let mut w = lib.writer();
    let mut locations = Vec::new();
    for path in [&rom, &sav] {
        let TrashStep::Trashed {
            location: Some(at), ..
        } = trash_file(&mut w, &trash, path, Some(0)).unwrap()
        else {
            panic!("not trashed: {}", path.display());
        };
        locations.push(at);
    }
    let job = w.finish().unwrap();
    // The user empties the Trash of the ROM (the save is still there).
    fs::remove_file(&locations[0]).unwrap();

    let report = undo_job_with(&lib.data, &job, &trash).unwrap();
    assert_eq!(report.archive_lost, std::slice::from_ref(&rom));
    assert!(!rom.exists());
    assert_eq!(report.restored, std::slice::from_ref(&sav));
    assert_eq!(fs::read(&sav).unwrap(), b"save");
}

static APPENDS_LEFT: AtomicIsize = AtomicIsize::new(-1);

fn full_on_the_nth_append() -> Option<io::Error> {
    (APPENDS_LEFT.fetch_sub(1, Ordering::SeqCst) <= 1)
        .then(|| io::Error::from(ErrorKind::StorageFull))
}

/// Trashes one ROM; with `fault`, the journal fills up right after the
/// intent line. Returns the job id and whether `trash_file` failed.
fn trash_one(lib: &Lib, name: &str, n: u64, fault: bool) -> (String, bool) {
    let rom = lib.file(&format!("GB/{name}"), b"rom", n);
    let trash = FakeTrash::new(lib.trash_dir());
    let mut w = lib.writer();
    if fault {
        APPENDS_LEFT.store(2, Ordering::SeqCst);
        set_journal_fault(Some(full_on_the_nth_append));
    }
    let result = trash_file(&mut w, &trash, &rom, Some(0));
    set_journal_fault(None);
    let job = w.job_id().to_string();
    if !fault {
        w.finish().unwrap();
    }
    (job, result.is_err())
}

#[test]
fn unresolved_trash_intent_keeps_the_dedupe_journal() {
    let lib = Lib::new();
    let (resolved, failed) = trash_one(&lib, "Tetris (1).gb", 1, false);
    assert!(!failed);
    let (unresolved, failed) = trash_one(&lib, "Tetris (2).gb", 2, true);
    assert!(failed, "the journal fault must surface");
    let steps = job_archive_steps(&lib.data, &unresolved).unwrap();
    assert_eq!(steps.len(), 1, "{steps:?}");
    assert!(matches!(steps[0].step, ArchiveStep::TrashIntent { .. }));

    let later = SystemTime::now() + Duration::from_secs(400 * 86_400);
    prune_jobs_at(&lib.data, 0, 90, later).unwrap();
    let left: Vec<String> = list_jobs(&lib.data)
        .unwrap()
        .into_iter()
        .map(|j| j.job)
        .collect();
    assert_eq!(
        left,
        std::slice::from_ref(&unresolved),
        "resolved job {resolved} pruned"
    );

    // Undo finds the file in the Trash by name and fingerprint.
    let rom = lib.root.join("ROMS/GB/Tetris (2).gb");
    assert!(!rom.exists());
    let report = undo_job_with(&lib.data, &unresolved, &FakeTrash::new(lib.trash_dir())).unwrap();
    assert_eq!(report.restored, std::slice::from_ref(&rom));
    assert_eq!(fs::read(&rom).unwrap(), b"rom");
}
