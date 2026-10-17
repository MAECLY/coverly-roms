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

// --- organize::apply: whole plans (plan Task 21) ---

mod plans {
    use std::collections::BTreeSet;
    use std::fs;
    use std::io;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, SystemTime};

    use coverly_core::cache::Cache;
    use coverly_core::identify::overrides::{remember_manual, Overrides, OVERRIDES_FILE};
    use coverly_core::identify::sniff::fixtures as fx;
    use coverly_core::jobs::ItemErrorKind;
    use coverly_core::organize::apply::{last_apply_dir_reads, set_free_space_override};
    use coverly_core::organize::{
        apply_organize, plan_organize, resume_organize, ApplyEnv, ApplyError, ItemOutcome,
        ItemStatus, MetaProbe, OrganizeAction, OrganizeEnv, OrganizeOptions, OrganizePhase,
        OrganizePlan, OrganizeProgress, OrganizeReport, SkipReason, TransferMode,
    };
    use coverly_core::profiles::{builtin_profiles, Profile};
    use coverly_core::progress::PauseKind;
    use coverly_core::systems::SystemDb;
    use coverly_core::write::{
        list_jobs, set_copy_fault, twin_listings, undo_job, CopyStage, JobKind,
    };
    use filetime::FileTime;
    use tempfile::TempDir;

    struct NoCloud;

    impl MetaProbe for NoCloud {
        fn cloud_only(&self, _: &Path, _: &fs::Metadata) -> bool {
            false
        }
    }

    struct World {
        db: SystemDb,
        profiles: Vec<Profile>,
        cache: Cache,
    }

    impl World {
        fn new() -> World {
            World {
                db: SystemDb::builtin(),
                profiles: builtin_profiles(),
                cache: Cache::in_memory().unwrap(),
            }
        }

        fn plan(&self, dir: &Path, opts: OrganizeOptions) -> OrganizePlan {
            let env = OrganizeEnv {
                db: &self.db,
                profiles: &self.profiles,
                cache: &self.cache,
                dats: None,
                probe: &NoCloud,
                online: None,
                now: SystemTime::now(),
            };
            plan_organize(dir, &opts, &env, &|_| {}, &AtomicBool::new(false)).unwrap()
        }
    }

    fn offline() -> OrganizeOptions {
        OrganizeOptions {
            layout: "anbernic-stock".into(),
            offline: true,
            ..OrganizeOptions::default()
        }
    }

    /// `<tmp>/pile` (the ROMs) and `<tmp>/data` (app data).
    fn pile() -> (TempDir, PathBuf, PathBuf) {
        let tmp = TempDir::new().unwrap();
        let pile = tmp.path().join("pile");
        fs::create_dir_all(&pile).unwrap();
        let data = tmp.path().join("data");
        (tmp, pile, data)
    }

    fn hour_ago() -> FileTime {
        FileTime::from_system_time(SystemTime::now() - Duration::from_secs(3600))
    }

    /// Writes `bytes` at `path` (parents created) dated one hour back.
    fn put(path: &Path, bytes: &[u8]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
        filetime::set_file_mtime(path, hour_ago()).unwrap();
    }

    fn apply_with(
        plan: &OrganizePlan,
        data: &Path,
        cancel: &AtomicBool,
    ) -> Result<OrganizeReport, ApplyError> {
        let env = ApplyEnv {
            data_dir: data,
            progress: &|_| {},
            cancel,
        };
        apply_organize(plan, &env)
    }

    fn apply(plan: &OrganizePlan, data: &Path) -> OrganizeReport {
        apply_with(plan, data, &AtomicBool::new(false)).unwrap()
    }

    fn resume(job: &str, data: &Path) -> OrganizeReport {
        let env = ApplyEnv {
            data_dir: data,
            progress: &|_| {},
            cancel: &AtomicBool::new(false),
        };
        resume_organize(job, &env).unwrap()
    }

    fn id_of(plan: &OrganizePlan, name: &str) -> usize {
        plan.items
            .iter()
            .find(|i| i.sources[0] == Path::new(name))
            .unwrap_or_else(|| panic!("no item {name}"))
            .id
    }

    fn outcome(report: &OrganizeReport, id: usize) -> &ItemOutcome {
        report
            .items
            .iter()
            .find(|o| o.item == id)
            .unwrap_or_else(|| panic!("no outcome for item {id}: {report:?}"))
    }

    fn skip_reason(report: &OrganizeReport, id: usize) -> Option<SkipReason> {
        match &outcome(report, id).status {
            ItemStatus::Skipped { reason } => Some(*reason),
            _ => None,
        }
    }

    /// Every file under `dir`, relative, sorted.
    fn tree(dir: &Path) -> BTreeSet<PathBuf> {
        walkdir(dir)
            .into_iter()
            .map(|p| p.strip_prefix(dir).unwrap().to_path_buf())
            .collect()
    }

    fn walkdir(dir: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        let Ok(read) = fs::read_dir(dir) else {
            return out;
        };
        for entry in read.flatten() {
            let path = entry.path();
            if entry.file_type().unwrap().is_dir() {
                out.extend(walkdir(&path));
            } else {
                out.push(path);
            }
        }
        out
    }

    fn set(paths: &[&str]) -> BTreeSet<PathBuf> {
        paths.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn apply_moves_auto_items_and_leaves_review_and_left_items() {
        let (_tmp, pile, data) = pile();
        put(&pile.join("Advance.gba"), &fx::gba_tagged(1));
        put(&pile.join("Boy.gb"), &fx::gb_tagged(0x00, 1));
        put(&pile.join("Half.gba.crdownload"), b"partial");
        put(&pile.join("notes.txt"), b"hello");
        let world = World::new();
        let mut plan = world.plan(&pile, offline());
        let (gba, gb) = (id_of(&plan, "Advance.gba"), id_of(&plan, "Boy.gb"));
        assert_eq!(plan.mode, TransferMode::Move);
        // The user has not answered for the Game Boy file yet.
        plan.items.iter_mut().find(|i| i.id == gb).unwrap().action = OrganizeAction::Review;

        let report = apply(&plan, &data);
        assert_eq!(report.moved, 1, "{report:?}");
        assert_eq!(report.failed, 0);
        assert!(report.paused.is_none());
        assert!(matches!(outcome(&report, gba).status, ItemStatus::Moved));
        assert_eq!(
            outcome(&report, gba).targets,
            [pile.join("ROMS/GBA/Advance.gba")]
        );
        assert_eq!(skip_reason(&report, gb), Some(SkipReason::NotDecided));
        assert_eq!(
            tree(&pile),
            set(&[
                "ROMS/GBA/Advance.gba",
                "Boy.gb",
                "Half.gba.crdownload",
                "notes.txt"
            ])
        );

        // One finished organise job, with its plan, undone in one go.
        let job = report.job.clone().expect("a job");
        let jobs = list_jobs(&data).unwrap();
        assert_eq!(jobs[0].job, job);
        assert_eq!(jobs[0].kind, JobKind::Organize);
        assert!(jobs[0].complete);
        let saved: OrganizePlan = serde_json::from_slice(
            &fs::read(data.join("jobs").join(format!("{job}.plan.json"))).unwrap(),
        )
        .unwrap();
        assert_eq!(saved, plan);
        undo_job(&data, &job).unwrap();
        assert_eq!(
            tree(&pile),
            set(&["Advance.gba", "Boy.gb", "Half.gba.crdownload", "notes.txt"])
        );
    }

    #[test]
    fn copy_mode_into_another_root_keeps_the_pile() {
        let (tmp, pile, data) = pile();
        put(&pile.join("Advance.gba"), &fx::gba_tagged(1));
        put(&pile.join("Boy.gb"), &fx::gb_tagged(0x00, 1));
        let card = tmp.path().join("card");
        fs::create_dir_all(&card).unwrap();
        let world = World::new();
        let plan = world.plan(
            &pile,
            OrganizeOptions {
                target: Some(card.clone()),
                mode: Some(TransferMode::Copy),
                ..offline()
            },
        );
        let before = tree(&pile);
        let report = apply(&plan, &data);
        assert_eq!((report.copied, report.moved), (2, 0), "{report:?}");
        assert!(report
            .items
            .iter()
            .all(|o| matches!(o.status, ItemStatus::Copied)));
        assert_eq!(tree(&pile), before, "the pile is untouched");
        assert_eq!(
            tree(&card),
            set(&["ROMS/GBA/Advance.gba", "ROMS/GB/Boy.gb"])
        );
        assert_eq!(
            fs::read(card.join("ROMS/GBA/Advance.gba")).unwrap(),
            fx::gba_tagged(1)
        );
        undo_job(&data, report.job.as_deref().unwrap()).unwrap();
        assert!(tree(&card).is_empty());
        assert_eq!(tree(&pile), before);
    }

    #[test]
    fn a_target_created_after_planning_gets_a_numbered_name() {
        let (_tmp, pile, data) = pile();
        put(&pile.join("Advance.gba"), &fx::gba_tagged(1));
        let world = World::new();
        let plan = world.plan(&pile, offline());
        // Another program puts a different file there meanwhile, in
        // another case.
        put(&pile.join("ROMS/GBA/advance.GBA"), b"someone else's");

        let report = apply(&plan, &data);
        let id = id_of(&plan, "Advance.gba");
        assert!(matches!(outcome(&report, id).status, ItemStatus::Moved));
        assert_eq!(
            outcome(&report, id).targets,
            [pile.join("ROMS/GBA/Advance (2).gba")]
        );
        assert_eq!(
            fs::read(pile.join("ROMS/GBA/advance.GBA")).unwrap(),
            b"someone else's"
        );
    }

    #[test]
    fn the_same_content_already_at_the_target_is_skipped() {
        let (_tmp, pile, data) = pile();
        put(&pile.join("Advance.gba"), &fx::gba_tagged(1));
        let world = World::new();
        let plan = world.plan(&pile, offline());
        put(&pile.join("ROMS/GBA/Advance.gba"), &fx::gba_tagged(1));

        let report = apply(&plan, &data);
        let id = id_of(&plan, "Advance.gba");
        assert_eq!(skip_reason(&report, id), Some(SkipReason::AlreadyPresent));
        assert!(pile.join("Advance.gba").exists());
        assert_eq!(report.job, None, "nothing changed: no job");
    }

    #[test]
    fn a_source_changed_after_planning_is_skipped() {
        let (_tmp, pile, data) = pile();
        put(&pile.join("Advance.gba"), &fx::gba_tagged(1));
        put(&pile.join("Other.gba"), &fx::gba_tagged(2));
        let world = World::new();
        let plan = world.plan(&pile, offline());
        // Rewritten with other bytes (and a newer date) after planning.
        let mut bytes = fx::gba_tagged(3);
        bytes.extend_from_slice(b"more");
        fs::write(pile.join("Advance.gba"), &bytes).unwrap();
        fs::remove_file(pile.join("Other.gba")).unwrap();

        let report = apply(&plan, &data);
        assert_eq!(
            skip_reason(&report, id_of(&plan, "Advance.gba")),
            Some(SkipReason::ChangedSincePlan)
        );
        assert_eq!(
            skip_reason(&report, id_of(&plan, "Other.gba")),
            Some(SkipReason::SourceMissing)
        );
        assert_eq!(fs::read(pile.join("Advance.gba")).unwrap(), bytes);
        assert_eq!(report.moved, 0);
    }

    #[test]
    fn a_set_collision_skips_the_whole_set() {
        let (_tmp, pile, data) = pile();
        put(
            &pile.join("Game.cue"),
            b"FILE \"Game.bin\" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 00:00:00\n",
        );
        put(&pile.join("Game.bin"), &fx::psx_iso(true, false));
        let world = World::new();
        let plan = world.plan(&pile, offline());
        let id = id_of(&plan, "Game.cue");
        let item = plan.items.iter().find(|i| i.id == id).unwrap();
        assert_eq!(item.action, OrganizeAction::Auto, "{item:?}");
        // A track with the set's name appears in the target folder.
        let track = plan.target_root.join(&item.set_targets[0]);
        put(&track, b"not this game");

        let report = apply(&plan, &data);
        assert_eq!(skip_reason(&report, id), Some(SkipReason::CollisionSet));
        assert!(pile.join("Game.cue").exists() && pile.join("Game.bin").exists());
        assert!(!plan
            .target_root
            .join(item.target.as_ref().unwrap())
            .exists());
        assert_eq!(fs::read(&track).unwrap(), b"not this game");
    }

    #[test]
    fn not_enough_space_refuses_to_start_with_numbers() {
        let (tmp, pile, data) = pile();
        put(&pile.join("Advance.gba"), &fx::gba_tagged(1));
        let card = tmp.path().join("card");
        fs::create_dir_all(&card).unwrap();
        let world = World::new();
        let plan = world.plan(
            &pile,
            OrganizeOptions {
                target: Some(card),
                mode: Some(TransferMode::Copy),
                ..offline()
            },
        );
        set_free_space_override(Some(|_| Some(10)));
        let result = apply_with(&plan, &data, &AtomicBool::new(false));
        set_free_space_override(None);
        let size = fx::gba_tagged(1).len() as u64;
        match result {
            Err(ApplyError::NoSpace { needed, free }) => {
                assert_eq!(free, 10);
                assert_eq!(needed, size + size.div_ceil(100));
            }
            other => panic!("expected NoSpace, got {other:?}"),
        }
        assert!(list_jobs(&data).unwrap().is_empty(), "no journal");
        assert!(pile.join("Advance.gba").exists());
    }

    #[test]
    fn free_space_reads_the_volume() {
        let tmp = TempDir::new().unwrap();
        let free = coverly_core::organize::free_space(tmp.path());
        assert!(free.is_some_and(|f| f > 0), "{free:?}");
        // A folder not created yet is measured on its nearest parent.
        assert!(coverly_core::organize::free_space(&tmp.path().join("a/b")).is_some());
    }

    /// Three Game Boy files and one GBA file, planned in place.
    fn three_gb(world: &World) -> (TempDir, PathBuf, PathBuf, OrganizePlan) {
        let (tmp, pile, data) = pile();
        put(&pile.join("A.gba"), &fx::gba_tagged(1));
        for (n, name) in ["B.gb", "C.gb", "D.gb"].iter().enumerate() {
            put(&pile.join(name), &fx::gb_tagged(0x00, n as u8 + 1));
        }
        let plan = world.plan(&pile, offline());
        assert_eq!(plan.items[0].sources[0], Path::new("A.gba"));
        (tmp, pile, data, plan)
    }

    #[cfg(unix)]
    #[test]
    fn a_read_only_target_pauses_and_resume_finishes_into_the_same_journal() {
        use std::os::unix::fs::PermissionsExt;
        let world = World::new();
        let (_tmp, pile, data, plan) = three_gb(&world);
        let gb_dir = pile.join("ROMS/GB");
        fs::create_dir_all(&gb_dir).unwrap();
        fs::set_permissions(&gb_dir, fs::Permissions::from_mode(0o555)).unwrap();

        let report = apply(&plan, &data);
        fs::set_permissions(&gb_dir, fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(report.paused, Some(PauseKind::Volume), "{report:?}");
        assert_eq!(report.pause_reason, Some(ItemErrorKind::WriteDenied));
        assert_eq!(report.moved, 1);
        let resumable: Vec<usize> = ["B.gb", "C.gb", "D.gb"]
            .iter()
            .map(|n| id_of(&plan, n))
            .collect();
        assert_eq!(report.resumable_items, resumable);
        let job = report.job.clone().unwrap();
        assert!(!list_jobs(&data).unwrap()[0].complete, "paused, not done");

        let resumed = resume(&job, &data);
        assert_eq!(resumed.job.as_deref(), Some(job.as_str()));
        assert!(resumed.paused.is_none(), "{resumed:?}");
        assert_eq!(resumed.moved, 4, "counts merge both runs: {resumed:?}");
        assert!(resumed.resumable_items.is_empty());
        assert_eq!(
            tree(&pile),
            set(&[
                "ROMS/GBA/A.gba",
                "ROMS/GB/B.gb",
                "ROMS/GB/C.gb",
                "ROMS/GB/D.gb"
            ])
        );
        let jobs = list_jobs(&data).unwrap();
        assert_eq!(jobs.len(), 1, "one journal for both runs");
        assert!(jobs[0].complete);

        undo_job(&data, &job).unwrap();
        assert_eq!(tree(&pile), set(&["A.gba", "B.gb", "C.gb", "D.gb"]));
    }

    fn disk_full_on_gb(stage: CopyStage, path: &Path) -> Option<io::Error> {
        (stage == CopyStage::Rename && path.extension().is_some_and(|e| e == "gb"))
            .then(|| io::Error::from_raw_os_error(28))
    }

    #[cfg(unix)]
    #[test]
    fn a_full_disk_pauses() {
        let world = World::new();
        let (_tmp, pile, data, plan) = three_gb(&world);
        set_copy_fault(Some(disk_full_on_gb));
        let report = apply(&plan, &data);
        set_copy_fault(None);
        assert_eq!(report.paused, Some(PauseKind::Volume));
        assert_eq!(report.pause_reason, Some(ItemErrorKind::DiskFull));
        assert_eq!(report.resumable_items.len(), 3);
        assert!(pile.join("B.gb").exists());
    }

    fn disk_full_on_sav(stage: CopyStage, path: &Path) -> Option<io::Error> {
        (stage == CopyStage::Rename && path.extension().is_some_and(|e| e == "sav"))
            .then(|| io::Error::from_raw_os_error(28))
    }

    #[cfg(unix)]
    #[test]
    fn a_pause_on_a_companion_is_finished_by_resume() {
        let (_tmp, pile, data) = pile();
        put(&pile.join("Boy.gb"), &fx::gb_tagged(0x00, 1));
        put(&pile.join("Boy.sav"), b"save");
        let world = World::new();
        let plan = world.plan(&pile, offline());
        let id = id_of(&plan, "Boy.gb");
        assert_eq!(plan.items[id].companions.len(), 1, "{:?}", plan.items[id]);

        set_copy_fault(Some(disk_full_on_sav));
        let report = apply(&plan, &data);
        set_copy_fault(None);
        assert_eq!(report.pause_reason, Some(ItemErrorKind::DiskFull));
        assert_eq!(report.resumable_items, [id]);
        assert!(pile.join("ROMS/GB/Boy.gb").exists());
        assert!(pile.join("Boy.sav").exists(), "the save is still behind");

        let job = report.job.clone().unwrap();
        let resumed = resume(&job, &data);
        assert!(resumed.paused.is_none(), "{resumed:?}");
        assert_eq!(resumed.moved, 1, "{resumed:?}");
        assert_eq!(tree(&pile), set(&["ROMS/GB/Boy.gb", "ROMS/GB/Boy.sav"]));
        assert_eq!(
            outcome(&resumed, id).targets,
            [pile.join("ROMS/GB/Boy.gb"), pile.join("ROMS/GB/Boy.sav")]
        );

        undo_job(&data, &job).unwrap();
        assert_eq!(tree(&pile), set(&["Boy.gb", "Boy.sav"]));
    }

    #[cfg(unix)]
    #[test]
    fn a_copy_resume_does_not_copy_a_companion_twice() {
        use std::os::unix::fs::PermissionsExt;
        let (tmp, pile, data) = pile();
        put(&pile.join("Advance.gba"), &fx::gba_tagged(1));
        put(&pile.join("Advance.sav"), b"save");
        put(&pile.join("Boy.gb"), &fx::gb_tagged(0x00, 1));
        let card = tmp.path().join("card");
        let gb_dir = card.join("ROMS/GB");
        fs::create_dir_all(&gb_dir).unwrap();
        let world = World::new();
        let plan = world.plan(
            &pile,
            OrganizeOptions {
                target: Some(card.clone()),
                mode: Some(TransferMode::Copy),
                ..offline()
            },
        );
        // The GB copy pauses the run after the GBA game and its save.
        fs::set_permissions(&gb_dir, fs::Permissions::from_mode(0o555)).unwrap();
        let report = apply(&plan, &data);
        fs::set_permissions(&gb_dir, fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(
            report.pause_reason,
            Some(ItemErrorKind::WriteDenied),
            "{report:?}"
        );
        let job = report.job.clone().unwrap();

        let resumed = resume(&job, &data);
        assert!(resumed.paused.is_none(), "{resumed:?}");
        assert_eq!(resumed.copied, 2, "{resumed:?}");
        assert_eq!(
            tree(&card),
            set(&[
                "ROMS/GBA/Advance.gba",
                "ROMS/GBA/Advance.sav",
                "ROMS/GB/Boy.gb"
            ])
        );
    }

    fn bad_copy_of_bin(stage: CopyStage, path: &Path) -> Option<io::Error> {
        (stage == CopyStage::Verify && path.extension().is_some_and(|e| e == "bin"))
            .then(|| io::Error::new(io::ErrorKind::InvalidData, "copy verification failed"))
    }

    #[cfg(unix)]
    #[test]
    fn a_copied_set_rolled_back_is_copied_whole_by_resume() {
        use std::os::unix::fs::PermissionsExt;
        let (tmp, pile, data) = pile();
        put(
            &pile.join("Game.cue"),
            b"FILE \"Game.bin\" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 00:00:00\n",
        );
        put(&pile.join("Game.bin"), &fx::psx_iso(true, false));
        put(&pile.join("Zz.gb"), &fx::gb_tagged(0x00, 1));
        let card = tmp.path().join("card");
        let gb_dir = card.join("ROMS/GB");
        fs::create_dir_all(&gb_dir).unwrap();
        let world = World::new();
        let plan = world.plan(
            &pile,
            OrganizeOptions {
                target: Some(card.clone()),
                mode: Some(TransferMode::Copy),
                ..offline()
            },
        );
        let set_id = id_of(&plan, "Game.cue");
        assert!(id_of(&plan, "Zz.gb") > set_id, "the set goes first");
        // The track's copy fails (the cue's copy is rolled back), then the
        // Game Boy folder pauses the run.
        fs::set_permissions(&gb_dir, fs::Permissions::from_mode(0o555)).unwrap();
        set_copy_fault(Some(bad_copy_of_bin));
        let report = apply(&plan, &data);
        set_copy_fault(None);
        fs::set_permissions(&gb_dir, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(
            matches!(outcome(&report, set_id).status, ItemStatus::Failed { .. }),
            "{report:?}"
        );
        assert_eq!(report.pause_reason, Some(ItemErrorKind::WriteDenied));
        let set_dir = card.join(
            plan.items[set_id]
                .target
                .as_ref()
                .unwrap()
                .parent()
                .unwrap(),
        );
        assert!(super::names_in(&set_dir).is_empty(), "the cue went back");

        let resumed = resume(report.job.as_deref().unwrap(), &data);
        assert!(resumed.paused.is_none(), "{resumed:?}");
        assert!(
            matches!(outcome(&resumed, set_id).status, ItemStatus::Copied),
            "{resumed:?}"
        );
        assert_eq!(
            super::names_in(&set_dir),
            ["Game.bin", "Game.cue"],
            "the whole set"
        );
    }

    #[test]
    fn leftover_temps_of_the_job_are_cleaned_on_resume() {
        let world = World::new();
        let (_tmp, pile, data, plan) = three_gb(&world);
        set_copy_fault(Some(disk_full_on_gb));
        let report = apply(&plan, &data);
        set_copy_fault(None);
        let job = report.job.clone().unwrap();
        // What a crash in the middle of a copy leaves behind.
        let gb_dir = pile.join("ROMS/GB");
        let temp = gb_dir.join(format!(".coverly-{job}-7.tmp"));
        put(&temp, b"half a copy");
        let foreign = gb_dir.join(".coverly-otherjob-1.tmp");
        put(&foreign, b"not ours");

        let resumed = resume(&job, &data);
        assert!(resumed.paused.is_none(), "{resumed:?}");
        assert!(!temp.exists(), "the job's temp is gone");
        assert!(foreign.exists(), "another job's temp stays");
    }

    static CANCEL_AFTER_FIRST: AtomicBool = AtomicBool::new(false);

    fn cancel_on_first_rename(stage: CopyStage, _: &Path) -> Option<io::Error> {
        if stage == CopyStage::Rename {
            CANCEL_AFTER_FIRST.store(true, Ordering::SeqCst);
        }
        None
    }

    #[test]
    fn cancel_between_items_keeps_what_was_done_undoable() {
        let world = World::new();
        let (_tmp, pile, data, plan) = three_gb(&world);
        set_copy_fault(Some(cancel_on_first_rename));
        let report = apply_with(&plan, &data, &CANCEL_AFTER_FIRST).unwrap();
        set_copy_fault(None);
        assert_eq!(report.moved, 1, "{report:?}");
        for name in ["B.gb", "C.gb", "D.gb"] {
            assert_eq!(
                skip_reason(&report, id_of(&plan, name)),
                Some(SkipReason::Cancelled)
            );
        }
        assert!(report.paused.is_none());
        assert!(pile.join("ROMS/GBA/A.gba").exists());
        let job = report.job.clone().unwrap();
        assert!(list_jobs(&data).unwrap()[0].complete);
        undo_job(&data, &job).unwrap();
        assert_eq!(tree(&pile), set(&["A.gba", "B.gb", "C.gb", "D.gb"]));
    }

    #[test]
    fn apply_uses_a_directory_index() {
        let (_tmp, pile, data) = pile();
        for n in 0..300u32 {
            // Distinct contents, or the copies would be duplicates.
            let mut bytes = fx::gba_tagged((n % 250) as u8);
            bytes.extend_from_slice(&n.to_le_bytes());
            put(&pile.join(format!("Game {n:03}.gba")), &bytes);
        }
        let world = World::new();
        let plan = world.plan(&pile, offline());
        let listings = twin_listings();
        let seen: std::sync::Mutex<Vec<OrganizeProgress>> = std::sync::Mutex::new(Vec::new());
        let env = ApplyEnv {
            data_dir: &data,
            progress: &|p| seen.lock().unwrap().push(p),
            cancel: &AtomicBool::new(false),
        };
        let report = apply_organize(&plan, &env).unwrap();
        assert_eq!(report.moved, 300, "{:?}", report.items.first());
        assert!(last_apply_dir_reads() <= 3, "{}", last_apply_dir_reads());
        assert_eq!(twin_listings() - listings, 0, "no folder listing per file");
        let seen = seen.into_inner().unwrap();
        assert!(seen.iter().all(|p| p.phase == OrganizePhase::Apply));
        assert_eq!(seen.last().map(|p| (p.done, p.total)), Some((300, 300)));
    }

    #[test]
    fn identity_overrides_follow_moved_roms() {
        let (_tmp, pile, data) = pile();
        put(&pile.join("Advance.gba"), &fx::gba_tagged(1));
        let world = World::new();
        let plan = world.plan(&pile, offline());
        let from = plan.source_root.join("Advance.gba");
        let to = plan.target_root.join("ROMS/GBA/Advance.gba");
        remember_manual(&data, &from, "Advance Wars (USA)", "manual").unwrap();

        let report = apply(&plan, &data);
        assert_eq!(report.moved, 1);
        let o = Overrides::read(&data.join(OVERRIDES_FILE));
        assert!(o.get(&from).is_none());
        assert_eq!(o.get(&to).unwrap().name, "Advance Wars (USA)");
        let job = report.job.unwrap();
        let journal = fs::read_to_string(data.join("jobs").join(format!("{job}.jsonl"))).unwrap();
        assert!(journal.contains("\"override-moved\""));

        undo_job(&data, &job).unwrap();
        let o = Overrides::read(&data.join(OVERRIDES_FILE));
        assert_eq!(o.get(&from).unwrap().name, "Advance Wars (USA)");
        assert!(o.get(&to).is_none());
    }

    #[test]
    fn resume_of_a_job_without_a_plan_fails() {
        let (_tmp, _pile, data) = pile();
        let env = ApplyEnv {
            data_dir: &data,
            progress: &|_| {},
            cancel: &AtomicBool::new(false),
        };
        assert!(matches!(
            resume_organize("20261017T000000Z-0000-0000", &env),
            Err(ApplyError::NoPlan(_))
        ));
    }
}
