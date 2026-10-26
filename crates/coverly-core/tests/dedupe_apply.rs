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

#[test]
fn a_resumed_run_finds_the_trashed_copy_gone_and_undo_restores_it_once() {
    let lib = Lib::new();
    let rom = lib.file("GB/Tetris (1).gb", b"tetris rom bytes", 1);
    let sav = lib.file("GB/Tetris (1).sav", b"battery save", 2);
    let trash = FakeTrash::new(lib.trash_dir());

    // First run: the ROM goes, then the run pauses (no `finish`).
    let mut w = lib.writer();
    let step = trash_file(&mut w, &trash, &rom, Some(0)).unwrap();
    assert!(matches!(step, TrashStep::Trashed { .. }), "{step:?}");
    let job = w.job_id().to_string();
    drop(w);

    // The resume replays the item: the ROM is already in the Trash.
    let mut w = Writer::resume(&lib.root, &lib.data, &job).unwrap();
    let before = job_archive_steps(&lib.data, &job).unwrap().len();
    let again = trash_file(&mut w, &trash, &rom, Some(0)).unwrap();
    assert_eq!(again, TrashStep::Gone);
    assert_eq!(
        job_archive_steps(&lib.data, &job).unwrap().len(),
        before,
        "a gone file adds no journal line"
    );
    let step = trash_file(&mut w, &trash, &sav, Some(0)).unwrap();
    assert!(matches!(step, TrashStep::Trashed { .. }), "{step:?}");
    w.finish().unwrap();
    assert_eq!(trash.trashed(), 2);

    let report = undo_job_with(&lib.data, &job, &trash).unwrap();
    let mut restored = report.restored.clone();
    restored.sort();
    assert_eq!(restored, [rom.clone(), sav.clone()]);
    assert!(report.archive_lost.is_empty(), "{report:?}");
    assert_eq!(fs::read(&rom).unwrap(), b"tetris rom bytes");
    assert_eq!(fs::read(&sav).unwrap(), b"battery save");
    assert_eq!(
        fs::read_dir(lib.trash_dir())
            .unwrap()
            .flatten()
            .filter(|e| entries_of(&e.path()) > 0)
            .count(),
        0
    );
}

#[test]
fn a_trash_io_error_leaves_the_file_and_undo_reports_nothing_lost() {
    let lib = Lib::new();
    let rom = lib.file("GB/Tetris (1).gb", b"tetris", 1);
    // The Trash folder cannot be created: every call fails with an I/O error.
    fs::write(lib.trash_dir(), b"not a folder").unwrap();
    let trash = FakeTrash::new(lib.trash_dir());

    let mut w = lib.writer();
    assert!(trash_file(&mut w, &trash, &rom, Some(0)).is_err());
    let job = w.job_id().to_string();
    drop(w);
    assert_eq!(fs::read(&rom).unwrap(), b"tetris");
    let steps = job_archive_steps(&lib.data, &job).unwrap();
    assert_eq!(steps.len(), 1, "{steps:?}");
    assert!(matches!(steps[0].step, ArchiveStep::TrashIntent { .. }));

    let report = undo_job_with(&lib.data, &job, &trash).unwrap();
    assert!(report.archive_lost.is_empty(), "{report:?}");
    assert!(report.restored.is_empty(), "{report:?}");
    assert_eq!(fs::read(&rom).unwrap(), b"tetris");
}

// --- The run: `apply_dedupe` and `resume_dedupe` ---

mod run {
    use std::collections::BTreeMap;
    use std::fs;
    use std::io;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

    use coverly_core::archive::Limits;
    use coverly_core::cache::Cache;
    use coverly_core::dedupe::archive::ArchiveEntryListing;
    use coverly_core::dedupe::testing::{pin_mtime, CountingBytes, FakeDiscProbe};
    use coverly_core::dedupe::{
        apply_dedupe, plan_dedupe, resume_dedupe, set_keeper, DedupeApplyEnv, DedupeApplyNote,
        DedupeDecisions, DedupeEnv, DedupeError, DedupeItemStatus, DedupeOptions, DedupePlan,
        DedupeReport, DedupeSkip, DestLimit, DupGroup, GroupAction, KeepReason, QuarantineChoice,
    };
    use coverly_core::organize::apply::ApplyError;
    use coverly_core::organize::target::set_fat32_override;
    use coverly_core::profiles::{builtin_profiles, Profile};
    use coverly_core::scan::{scan_library, ScanOptions};
    use coverly_core::settings::Settings;
    use coverly_core::systems::SystemDb;
    use coverly_core::write::trash::{FakeTrash, TrashBackend};
    use coverly_core::write::{list_jobs, set_copy_fault, undo_job_with, CopyStage};
    use tempfile::TempDir;

    /// Synthetic ROM bytes: binary, distinct per `seed`.
    fn rom(seed: u8, len: usize) -> Vec<u8> {
        (0..len)
            .map(|i| if i % 3 == 0 { 0 } else { (i as u8) ^ seed })
            .collect()
    }

    /// A library in a temp folder, its cache and its data folder.
    struct Card {
        tmp: TempDir,
        root: PathBuf,
        data: PathBuf,
        cache: Cache,
        profile: Profile,
        settings: Settings,
        cancel: AtomicBool,
        next: AtomicU64,
    }

    impl Card {
        fn new() -> Card {
            Card::with_profile("anbernic-stock")
        }

        fn with_profile(id: &str) -> Card {
            let tmp = TempDir::new().unwrap();
            let root = tmp.path().join("SD");
            fs::create_dir_all(&root).unwrap();
            Card {
                root,
                data: tmp.path().join("data"),
                tmp,
                cache: Cache::in_memory().unwrap(),
                profile: builtin_profiles().into_iter().find(|p| p.id == id).unwrap(),
                settings: Settings::default(),
                cancel: AtomicBool::new(false),
                next: AtomicU64::new(0),
            }
        }

        /// `root/rel` with `bytes`, its mtime pinned after the last one.
        fn put(&self, rel: &str, bytes: &[u8]) -> PathBuf {
            let p = self.root.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(&p, bytes).unwrap();
            pin_mtime(&p, self.next.fetch_add(1, Ordering::Relaxed));
            p
        }

        fn trash(&self) -> FakeTrash {
            FakeTrash::new(self.tmp.path().join("trash"))
        }

        fn opts(&self) -> DedupeOptions {
            DedupeOptions::from_settings(&self.settings.dedupe)
        }

        fn plan(&self) -> DedupePlan {
            self.plan_with(&self.opts())
        }

        fn plan_with(&self, opts: &DedupeOptions) -> DedupePlan {
            let db = SystemDb::builtin();
            let scan = scan_library(&self.root, &self.profile, &db, &ScanOptions::default());
            let env = DedupeEnv {
                cache: &self.cache,
                db: &db,
                profile: &self.profile,
                settings: &self.settings,
                listing: &ArchiveEntryListing::new(Limits::default()),
                discs: &FakeDiscProbe::new(),
                dats: None,
                bytes: &CountingBytes::new(),
                progress: &|_| {},
                cancel: &AtomicBool::new(false),
            };
            plan_dedupe(&scan, opts, &env).unwrap()
        }

        fn env<'a>(
            &'a self,
            trash: &'a dyn TrashBackend,
            progress: &'a (dyn Fn(usize, usize) + Sync),
        ) -> DedupeApplyEnv<'a> {
            DedupeApplyEnv {
                data_dir: &self.data,
                trash,
                cache: &self.cache,
                progress,
                cancel: &self.cancel,
            }
        }

        fn apply(
            &self,
            plan: &DedupePlan,
            decisions: &DedupeDecisions,
        ) -> Result<DedupeReport, ApplyError> {
            let trash = self.trash();
            apply_dedupe(plan, decisions, &self.env(&trash, &|_, _| {}))
        }

        fn resume(&self, job: &str) -> DedupeReport {
            let trash = self.trash();
            resume_dedupe(job, &self.env(&trash, &|_, _| {})).unwrap()
        }

        fn q(&self) -> PathBuf {
            self.root.join("_Duplicados")
        }
    }

    fn group<'a>(plan: &'a DedupePlan, system: &str) -> &'a DupGroup {
        plan.groups
            .iter()
            .find(|g| g.system == system)
            .unwrap_or_else(|| panic!("no {system} group in {:#?}", plan.groups))
    }

    /// The decision that keeps the copy at `path`.
    fn keep(plan: &DedupePlan, path: &Path) -> DedupeDecisions {
        let (g, m) = plan
            .groups
            .iter()
            .find_map(|g| {
                let m = g.members.iter().find(|m| m.unit.primary == path)?;
                Some((g.id, m.id))
            })
            .unwrap_or_else(|| panic!("no member at {}", path.display()));
        DedupeDecisions {
            keepers: vec![(g, m)],
            ..DedupeDecisions::default()
        }
    }

    fn status_of<'a>(
        report: &'a DedupeReport,
        plan: &DedupePlan,
        path: &Path,
    ) -> &'a DedupeItemStatus {
        let (g, m) = plan
            .groups
            .iter()
            .find_map(|g| {
                let m = g.members.iter().find(|m| m.unit.primary == path)?;
                Some((g.id, m.id))
            })
            .unwrap();
        &report
            .outcomes
            .iter()
            .find(|o| o.group == g && o.member == m)
            .unwrap_or_else(|| panic!("no outcome for {}: {report:#?}", path.display()))
            .status
    }

    /// Every file under `dir`: path → bytes.
    fn tree(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
        let mut out = BTreeMap::new();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(d) = stack.pop() {
            let Ok(read) = fs::read_dir(&d) else { continue };
            for e in read.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else {
                    out.insert(p.clone(), fs::read(&p).unwrap());
                }
            }
        }
        out
    }

    /// Two identical GB copies: `Tetris.gb` stays (it has a save),
    /// `old/Tetris (1).gb` goes, with its cover and its `._` twin.
    fn tetris(card: &Card) -> (PathBuf, PathBuf) {
        let bytes = rom(1, 32 * 1024);
        let keeper = card.put("ROMS/GB/Tetris.gb", &bytes);
        card.put("ROMS/GB/Tetris.sav", &rom(20, 8192));
        let extra = card.put("ROMS/GB/old/Tetris (1).gb", &bytes);
        (keeper, extra)
    }

    #[test]
    fn quarantine_path_mirrors_rel_dir_and_art_follows() {
        let card = Card::new();
        let (keeper, extra) = tetris(&card);
        let art = card.put("ROMS/GB/old/Imgs/Tetris (1).png", b"\x89PNG\r\n\x1a\nart");
        let twin = card.put("ROMS/GB/old/._Tetris (1).gb", b"\0\x05\x16\x07apple");
        let shared = card.put("ROMS/GB/Tetris.sav", &rom(20, 8192));
        let plan = card.plan();
        let g = group(&plan, "gb");
        assert_eq!(g.members[g.keeper].unit.primary, keeper);

        let report = card.apply(&plan, &DedupeDecisions::default()).unwrap();
        let q = card.q();
        let to = q.join("GB/old/Tetris (1).gb");
        assert_eq!(
            status_of(&report, &plan, &extra),
            &DedupeItemStatus::Quarantined { to: to.clone() }
        );
        assert!(!extra.exists() && !art.exists() && !twin.exists());
        assert_eq!(fs::read(&to).unwrap(), rom(1, 32 * 1024));
        assert!(q.join("GB/old/Imgs/Tetris (1).png").is_file());
        assert!(q.join("GB/old/._Tetris (1).gb").is_file());
        assert!(
            keeper.is_file() && shared.is_file(),
            "the keeper and its save stay"
        );
        assert_eq!(report.bytes_out, 32 * 1024);
        assert!(report.job.is_some());
        assert_eq!(list_jobs(&card.data).unwrap().len(), 1);
    }

    /// A Batocera card whose game list names both copies.
    fn batocera_tetris(card: &Card) -> (PathBuf, PathBuf, PathBuf, String) {
        let bytes = rom(1, 32 * 1024);
        let keeper = card.put("roms/gb/Tetris.gb", &bytes);
        let extra = card.put("roms/gb/Tetris (1).gb", &bytes);
        let xml = "<?xml version=\"1.0\"?>\n<gameList>\n\t<game>\n\t\t<path>./Tetris.gb</path>\n\t\t<name>Tetris</name>\n\t</game>\n\t<game>\n\t\t<path>./Tetris (1).gb</path>\n\t\t<name>Tetris copy</name>\n\t</game>\n</gameList>\n".to_string();
        let gamelist = card.put("roms/gb/gamelist.xml", xml.as_bytes());
        (keeper, extra, gamelist, xml)
    }

    #[test]
    fn gamelist_entry_removed_with_backup() {
        let card = Card::with_profile("batocera");
        let (keeper, extra, gamelist, xml) = batocera_tetris(&card);
        let plan = card.plan();
        let m = group(&plan, "gb")
            .members
            .iter()
            .find(|m| m.unit.primary == extra)
            .unwrap();
        assert!(m.gamelist.is_some(), "{m:#?}");

        let report = card.apply(&plan, &keep(&plan, &keeper)).unwrap();
        assert!(matches!(
            status_of(&report, &plan, &extra),
            DedupeItemStatus::Quarantined { .. }
        ));
        let now = fs::read_to_string(&gamelist).unwrap();
        assert!(!now.contains("Tetris copy"), "{now}");
        assert!(now.contains("<path>./Tetris.gb</path>"), "{now}");
        assert!(report.notes.is_empty(), "{:?}", report.notes);

        let undo =
            undo_job_with(&card.data, report.job.as_deref().unwrap(), &card.trash()).unwrap();
        assert_eq!(fs::read_to_string(&gamelist).unwrap(), xml, "{undo:?}");
        assert!(extra.is_file());
    }

    #[test]
    fn gamelist_outside_the_writer_root_is_noted_not_written() {
        let card = Card::with_profile("batocera");
        let (keeper, extra, gamelist, xml) = batocera_tetris(&card);
        let elsewhere = card.tmp.path().join("elsewhere");
        let mut opts = card.opts();
        opts.quarantine = QuarantineChoice::Folder {
            path: elsewhere.clone(),
        };
        let plan = card.plan_with(&opts);

        let report = card.apply(&plan, &keep(&plan, &keeper)).unwrap();
        assert_eq!(
            status_of(&report, &plan, &extra),
            &DedupeItemStatus::Quarantined {
                to: elsewhere.join("gb/Tetris (1).gb")
            }
        );
        assert_eq!(fs::read_to_string(&gamelist).unwrap(), xml);
        assert_eq!(
            report.notes,
            [DedupeApplyNote::GamelistNotUpdated {
                system: "gb".into()
            }]
        );
    }

    #[test]
    fn collision_in_quarantine_gets_numbered() {
        let card = Card::new();
        let (_, extra) = tetris(&card);
        let earlier = card.put("_Duplicados/GB/old/Tetris (1).gb", b"an earlier job's copy");
        let plan = card.plan();
        let report = card.apply(&plan, &DedupeDecisions::default()).unwrap();
        let to = card.q().join("GB/old/Tetris (1) (2).gb");
        assert_eq!(
            status_of(&report, &plan, &extra),
            &DedupeItemStatus::Quarantined { to: to.clone() }
        );
        assert_eq!(fs::read(&earlier).unwrap(), b"an earlier job's copy");
        assert_eq!(fs::read(&to).unwrap(), rom(1, 32 * 1024));
    }

    #[test]
    fn keeper_changed_skips_the_group() {
        let card = Card::new();
        let (keeper, extra) = tetris(&card);
        let extra2 = card.put("ROMS/GB/Tetris (2).gb", &rom(1, 32 * 1024));
        let plan = card.plan();
        fs::write(&keeper, b"edited since the plan").unwrap();
        let before = tree(&card.root);

        let report = card.apply(&plan, &DedupeDecisions::default()).unwrap();
        for p in [&extra, &extra2] {
            assert_eq!(
                status_of(&report, &plan, p),
                &DedupeItemStatus::Skipped {
                    reason: DedupeSkip::KeeperChanged
                }
            );
        }
        assert_eq!(tree(&card.root), before);
        assert_eq!(report.job, None, "nothing changed: no job");
        assert!(list_jobs(&card.data).unwrap_or_default().is_empty());
    }

    #[test]
    fn changed_extra_is_left_alone() {
        let card = Card::new();
        let (_, extra) = tetris(&card);
        let extra2 = card.put("ROMS/GB/Tetris (2).gb", &rom(1, 32 * 1024));
        let plan = card.plan();
        pin_mtime(&extra2, 500);

        let report = card.apply(&plan, &DedupeDecisions::default()).unwrap();
        assert_eq!(
            status_of(&report, &plan, &extra2),
            &DedupeItemStatus::Skipped {
                reason: DedupeSkip::ChangedSincePlan
            }
        );
        assert!(extra2.is_file());
        assert!(matches!(
            status_of(&report, &plan, &extra),
            DedupeItemStatus::Quarantined { .. }
        ));
    }

    const CUE: &[u8] = b"FILE \"Crash (Track 1).bin\" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 00:00:00\nFILE \"Crash (Track 2).bin\" BINARY\n  TRACK 02 AUDIO\n    INDEX 01 00:00:00\n";

    /// Two identical PS cue sets of two bins; the one in `old/` goes.
    fn crash(card: &Card) -> (PathBuf, [PathBuf; 3]) {
        let keeper = card.put("ROMS/PS/Crash.cue", CUE);
        card.put("ROMS/PS/Crash (Track 1).bin", &rom(5, 2352 * 4));
        card.put("ROMS/PS/Crash (Track 2).bin", &rom(6, 2352 * 2));
        let cue = card.put("ROMS/PS/old/Crash.cue", CUE);
        let t1 = card.put("ROMS/PS/old/Crash (Track 1).bin", &rom(5, 2352 * 4));
        let t2 = card.put("ROMS/PS/old/Crash (Track 2).bin", &rom(6, 2352 * 2));
        (keeper, [cue, t1, t2])
    }

    fn fail_track_two(stage: CopyStage, path: &Path) -> Option<io::Error> {
        (stage == CopyStage::Rename && path.ends_with(Path::new("old").join("Crash (Track 2).bin")))
            .then(|| io::Error::other("injected"))
    }

    #[test]
    fn set_moves_all_or_nothing() {
        // A missing bin: the set is skipped before any of it moves.
        let card = Card::new();
        let (keeper, set) = crash(&card);
        let plan = card.plan();
        let g = group(&plan, "psx");
        assert_eq!(g.members[g.keeper].unit.primary, keeper, "{g:#?}");
        assert_eq!(g.members[1 - g.keeper].unit.files.len(), 3);
        fs::remove_file(&set[2]).unwrap();
        let report = card.apply(&plan, &DedupeDecisions::default()).unwrap();
        assert_eq!(
            status_of(&report, &plan, &set[0]),
            &DedupeItemStatus::Skipped {
                reason: DedupeSkip::SourceMissing
            }
        );
        assert!(set[0].is_file() && set[1].is_file());
        assert!(!card.q().exists());

        // A write error on the second bin: the files already moved go back.
        let card = Card::new();
        let (_, set) = crash(&card);
        let plan = card.plan();
        set_copy_fault(Some(fail_track_two));
        let report = card.apply(&plan, &DedupeDecisions::default());
        set_copy_fault(None);
        let report = report.unwrap();
        assert!(
            matches!(
                status_of(&report, &plan, &set[0]),
                DedupeItemStatus::Failed { .. }
            ),
            "{report:#?}"
        );
        for f in &set {
            assert!(f.is_file(), "{} is back", f.display());
        }
        assert!(tree(&card.q()).is_empty(), "{:?}", tree(&card.q()));
    }

    #[test]
    fn conflict_and_report_only_groups_never_move() {
        let card = Card::new();
        // MD: exact copies whose real saves differ.
        let sonic = rom(9, 8 * 1024);
        card.put("ROMS/MD/Sonic.md", &sonic);
        let sonic1 = card.put("ROMS/MD/Sonic (1).md", &sonic);
        card.put("ROMS/MD/Sonic.srm", &rom(10, 8192));
        card.put("ROMS/MD/Sonic (1).srm", &rom(11, 8192));
        // GB: skipped by you.
        let (_, extra) = tetris(&card);
        let plan = card.plan();
        assert!(matches!(
            group(&plan, "megadrive").action,
            GroupAction::Conflict { .. }
        ));
        let before = tree(&card.root);
        let decisions = DedupeDecisions {
            skipped_systems: vec!["gb".into()],
            ..DedupeDecisions::default()
        };
        let report = card.apply(&plan, &decisions).unwrap();
        assert_eq!(
            status_of(&report, &plan, &sonic1),
            &DedupeItemStatus::Skipped {
                reason: DedupeSkip::Conflict
            }
        );
        assert_eq!(
            status_of(&report, &plan, &extra),
            &DedupeItemStatus::Skipped {
                reason: DedupeSkip::Excluded
            }
        );
        assert_eq!(tree(&card.root), before);
        assert_eq!(report.job, None);
    }

    #[test]
    fn never_moves_every_member() {
        let card = Card::new();
        let (keeper, _) = tetris(&card);
        let plan = card.plan();
        let before = tree(&card.root);
        let gid = group(&plan, "gb").id;

        let bad = DedupeDecisions {
            keepers: vec![(gid, 99)],
            ..DedupeDecisions::default()
        };
        let err = card.apply(&plan, &bad).unwrap_err();
        assert!(
            matches!(
                err,
                ApplyError::Dedupe(DedupeError::NoMember { member: 99, .. })
            ),
            "{err:?}"
        );

        // A plan that would set the keeper aside too.
        let mut tampered = plan.clone();
        let g = &mut tampered.groups[0];
        let k = g.keeper;
        g.members[k].dest = Some(card.q().join("GB/Tetris.gb"));
        let err = card
            .apply(&tampered, &DedupeDecisions::default())
            .unwrap_err();
        assert!(
            matches!(err, ApplyError::Dedupe(DedupeError::EveryMemberMoves(id)) if id == gid),
            "{err:?}"
        );
        assert_eq!(tree(&card.root), before);
        assert!(keeper.is_file());
        assert!(list_jobs(&card.data).unwrap_or_default().is_empty());
    }

    fn cross_device(stage: CopyStage, _: &Path) -> Option<io::Error> {
        (stage == CopyStage::Rename).then(|| io::Error::from(io::ErrorKind::CrossesDevices))
    }

    #[test]
    fn cross_volume_copies_verify_then_delete() {
        let card = Card::new();
        let (_, extra) = tetris(&card);
        let plan = card.plan();
        set_copy_fault(Some(cross_device));
        let report = card.apply(&plan, &DedupeDecisions::default());
        set_copy_fault(None);
        let report = report.unwrap();
        let to = card.q().join("GB/old/Tetris (1).gb");
        assert_eq!(
            status_of(&report, &plan, &extra),
            &DedupeItemStatus::Quarantined { to: to.clone() }
        );
        assert!(
            !extra.exists(),
            "the source is gone after the verified copy"
        );
        assert_eq!(fs::read(&to).unwrap(), rom(1, 32 * 1024));
        let job = report.job.unwrap();
        let journal = fs::read_to_string(card.data.join(format!("jobs/{job}.jsonl"))).unwrap();
        assert!(journal.contains("\"via\":\"copy\""), "{journal}");
    }

    #[test]
    fn fat32_destination_skips_before_any_copy() {
        let card = Card::new();
        let (_, extra) = tetris(&card);
        let mut plan = card.plan();
        // The extra grows to 4.2 GiB (sparse) and the plan says so.
        let big = (4u64 << 30) + 200 * 1024 * 1024;
        fs::OpenOptions::new()
            .write(true)
            .open(&extra)
            .unwrap()
            .set_len(big)
            .unwrap();
        pin_mtime(&extra, 900);
        let modified = fs::metadata(&extra).unwrap().modified().unwrap();
        let mtime_ns = modified
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as i64;
        for g in &mut plan.groups {
            for m in g.members.iter_mut().filter(|m| m.unit.primary == extra) {
                m.size = big;
                m.unit.mtime_ns = mtime_ns;
            }
        }
        set_fat32_override(Some(true));
        let report = card.apply(&plan, &DedupeDecisions::default());
        set_fat32_override(None);
        let report = report.unwrap();
        assert_eq!(
            status_of(&report, &plan, &extra),
            &DedupeItemStatus::Skipped {
                reason: DedupeSkip::DestinationLimit {
                    why: DestLimit::Fat32Size
                }
            }
        );
        assert!(!card.q().exists(), "nothing was copied");
        assert_eq!(fs::metadata(&extra).unwrap().len(), big);
    }

    #[cfg(unix)]
    #[test]
    fn bad_name_and_case_collision_on_exfat() {
        let card = Card::new();
        let bytes = rom(1, 32 * 1024);
        card.put("ROMS/GB/Tetris.gb", &bytes);
        card.put("ROMS/GB/Tetris.sav", &rom(20, 8192));
        let odd = card.put("ROMS/GB/Tet?ris.gb", &bytes);
        let (_, set) = crash(&card);
        // An earlier job left a bin spelt in another case.
        let other = card.put("_Duplicados/PS/old/crash (track 2).BIN", b"earlier");
        let plan = card.plan();
        let report = card.apply(&plan, &DedupeDecisions::default()).unwrap();
        assert_eq!(
            status_of(&report, &plan, &odd),
            &DedupeItemStatus::Skipped {
                reason: DedupeSkip::DestinationLimit {
                    why: DestLimit::BadName
                }
            }
        );
        assert!(odd.is_file());
        assert_eq!(
            status_of(&report, &plan, &set[0]),
            &DedupeItemStatus::Skipped {
                reason: DedupeSkip::DestinationLimit {
                    why: DestLimit::CaseCollision
                }
            }
        );
        for f in &set {
            assert!(f.is_file());
        }
        assert_eq!(fs::read(&other).unwrap(), b"earlier");
    }

    fn full_on_mario(stage: CopyStage, path: &Path) -> Option<io::Error> {
        (stage == CopyStage::Rename && path.to_string_lossy().contains("Mario"))
            .then(|| io::Error::from(io::ErrorKind::StorageFull))
    }

    /// Tetris (GB) and Mario (SFC): two groups, GB first.
    fn two_groups(card: &Card) -> (PathBuf, PathBuf) {
        let (_, tetris) = tetris(card);
        let mario = rom(3, 64 * 1024);
        card.put("ROMS/SFC/Mario.sfc", &mario);
        let extra = card.put("ROMS/SFC/Mario (1).sfc", &mario);
        (tetris, extra)
    }

    #[test]
    fn disk_full_pauses_and_resume_finishes() {
        let card = Card::new();
        let (tetris, mario) = two_groups(&card);
        let plan = card.plan();
        assert_eq!(plan.groups[0].system, "gb");
        set_copy_fault(Some(full_on_mario));
        let report = card.apply(&plan, &DedupeDecisions::default());
        set_copy_fault(None);
        let report = report.unwrap();
        assert_eq!(
            report.paused,
            Some(coverly_core::jobs::ItemErrorKind::DiskFull)
        );
        let sfc = group(&plan, "snes");
        assert_eq!(report.resumable, [(sfc.id, 1 - sfc.keeper)]);
        assert!(!tetris.exists() && mario.is_file());
        let job = report.job.clone().unwrap();

        let again = card.resume(&job);
        assert_eq!(again.job.as_deref(), Some(job.as_str()));
        assert_eq!(again.paused, None);
        assert!(!mario.exists());
        assert!(card.q().join("SFC/Mario (1).sfc").is_file());
        assert!(card.q().join("GB/old/Tetris (1).gb").is_file());
        for p in [&tetris, &mario] {
            assert!(matches!(
                status_of(&again, &plan, p),
                DedupeItemStatus::Quarantined { .. }
            ));
        }
        assert_eq!(list_jobs(&card.data).unwrap().len(), 1, "one job");
        let undo = undo_job_with(&card.data, &job, &card.trash()).unwrap();
        assert!(tetris.is_file() && mario.is_file(), "{undo:?}");
    }

    #[test]
    fn cancel_between_groups_leaves_a_resumable_job() {
        let card = Card::new();
        let (tetris, mario) = two_groups(&card);
        let plan = card.plan();
        let trash = card.trash();
        let cancel_on_first = |_: usize, _: usize| card.cancel.store(true, Ordering::SeqCst);
        let report = apply_dedupe(
            &plan,
            &DedupeDecisions::default(),
            &card.env(&trash, &cancel_on_first),
        )
        .unwrap();
        assert!(!tetris.exists(), "the first group finished");
        assert!(mario.is_file());
        assert_eq!(
            status_of(&report, &plan, &mario),
            &DedupeItemStatus::Skipped {
                reason: DedupeSkip::Cancelled
            }
        );
        let sfc = group(&plan, "snes");
        assert_eq!(report.resumable, [(sfc.id, 1 - sfc.keeper)]);
        let job = report.job.unwrap();

        card.cancel.store(false, Ordering::SeqCst);
        let again = card.resume(&job);
        assert!(!mario.exists());
        assert_eq!(again.job.as_deref(), Some(job.as_str()));
        assert!(again.resumable.is_empty());
    }

    #[test]
    fn writes_the_job_list_file_in_the_quarantine() {
        let card = Card::new();
        tetris(&card);
        let plan = card.plan();
        let report = card.apply(&plan, &DedupeDecisions::default()).unwrap();
        let job = report.job.unwrap();
        let list = fs::read_to_string(card.q().join(format!("{job}.txt"))).unwrap();
        assert_eq!(
            list,
            "GB/Tetris.gb <- GB/old/Tetris (1).gb (exact, has-save)\n"
        );
        // Journaled `Created`: undo removes it.
        undo_job_with(&card.data, &job, &card.trash()).unwrap();
        assert!(!card.q().join(format!("{job}.txt")).exists());
    }

    #[test]
    fn trash_action_sets_aside_through_the_backend_and_undo_brings_it_back() {
        let card = Card::new();
        let (keeper, extra) = tetris(&card);
        let art = card.put("ROMS/GB/old/Imgs/Tetris (1).png", b"\x89PNG\r\n\x1a\nart");
        let mut opts = card.opts();
        opts.quarantine = QuarantineChoice::Trash;
        let plan = card.plan_with(&opts);
        assert!(plan.trash);
        let trash = card.trash();
        let report = apply_dedupe(
            &plan,
            &DedupeDecisions::default(),
            &card.env(&trash, &|_, _| {}),
        )
        .unwrap();
        assert!(
            matches!(
                status_of(&report, &plan, &extra),
                DedupeItemStatus::Trashed { location: Some(_) }
            ),
            "{report:#?}"
        );
        assert!(!extra.exists() && !art.exists() && keeper.is_file());
        assert_eq!(trash.trashed(), 2);
        assert!(!card.q().exists(), "no quarantine, no list file");

        let undo = undo_job_with(&card.data, report.job.as_deref().unwrap(), &trash).unwrap();
        assert!(undo.archive_lost.is_empty(), "{undo:?}");
        assert_eq!(fs::read(&extra).unwrap(), rom(1, 32 * 1024));
        assert!(art.is_file());
    }

    #[test]
    fn trash_unavailable_on_the_volume_skips_with_a_reason() {
        let card = Card::new();
        let (_, extra) = tetris(&card);
        let mut opts = card.opts();
        opts.quarantine = QuarantineChoice::Trash;
        let plan = card.plan_with(&opts);
        let trash = FakeTrash::unavailable(card.tmp.path().join("trash"));
        let report = apply_dedupe(
            &plan,
            &DedupeDecisions::default(),
            &card.env(&trash, &|_, _| {}),
        )
        .unwrap();
        assert_eq!(
            status_of(&report, &plan, &extra),
            &DedupeItemStatus::Skipped {
                reason: DedupeSkip::TrashUnavailable
            }
        );
        assert!(extra.is_file());
        assert_eq!(report.job, None);
    }

    #[test]
    fn content_cache_rows_follow_the_moved_files() {
        let card = Card::new();
        let (_, extra) = tetris(&card);
        let plan = card.plan();
        let vol = coverly_core::volumes::volume_uuid(&card.root).unwrap();
        let meta = fs::metadata(&extra).unwrap();
        let unit = &plan
            .groups
            .iter()
            .flat_map(|g| &g.members)
            .find(|m| m.unit.primary == extra)
            .unwrap()
            .unit;
        let mtime = unit.mtime_ns;
        assert!(card
            .cache
            .content_row(&vol, &unit.primary, meta.len(), mtime)
            .unwrap()
            .is_some());

        card.apply(&plan, &DedupeDecisions::default()).unwrap();
        // The cache keys paths as text: build them with the OS separator.
        let to: PathBuf = card.q().join("GB/old/Tetris (1).gb").components().collect();
        let extra: PathBuf = extra.components().collect();
        assert!(card
            .cache
            .content_row(&vol, &to, meta.len(), mtime)
            .unwrap()
            .is_some());
        assert!(card
            .cache
            .content_row(&vol, &extra, meta.len(), mtime)
            .unwrap()
            .is_none());
    }

    #[test]
    fn remembered_choice_by_path_survives_apply() {
        let card = Card::new();
        let bytes = rom(1, 32 * 1024);
        card.put("ROMS/GB/Tetris.gb", &bytes);
        let picked = card.put("ROMS/GB/old/Tetris (1).gb", &bytes);
        let mut plan = card.plan();
        let g = group(&plan, "gb").clone();
        assert_ne!(
            g.members[g.keeper].unit.primary, picked,
            "the rules keep the other"
        );
        let m = g.members.iter().find(|m| m.unit.primary == picked).unwrap();
        // Remembered by content key: the file id on Unix, the path on
        // Windows (no file id in std there). The keeper never moves, so
        // either key still names it after apply.
        set_keeper(&mut plan, g.id, m.id, &card.cache).unwrap();

        let report = card.apply(&plan, &DedupeDecisions::default()).unwrap();
        assert!(report.job.is_some());
        assert!(picked.is_file(), "the keeper never moves");

        // A new copy joins: the remembered pick still holds.
        card.put("ROMS/GB/Tetris (3).gb", &bytes);
        let next = card.plan();
        let g = group(&next, "gb");
        assert_eq!(g.members[g.keeper].unit.primary, picked);
        assert_eq!(g.reason, KeepReason::UserChoice, "{g:#?}");
    }

    #[test]
    fn apply_writes_nothing_outside_the_writer_root() {
        let card = Card::new();
        let (_, _) = tetris(&card);
        let sibling = card.tmp.path().join("sibling");
        fs::create_dir_all(sibling.join("GB")).unwrap();
        fs::write(sibling.join("GB/Tetris.gb"), rom(1, 32 * 1024)).unwrap();
        let before = tree(&sibling);
        let plan = card.plan();
        let report = card.apply(&plan, &DedupeDecisions::default()).unwrap();
        assert!(report.job.is_some());
        assert_eq!(tree(&sibling), before);
        // Only the data folder and the library changed.
        let mut top: Vec<String> = fs::read_dir(card.tmp.path())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        top.sort();
        assert_eq!(top, ["SD", "data", "sibling"]);
    }

    /// A plan whose extra would carry away a file a copy that stays uses
    /// (its own group's keeper, another group's keeper) is refused before
    /// any write: the copy that stays never loses a file.
    #[test]
    fn an_extra_never_takes_a_file_the_keeper_needs() {
        use coverly_core::dedupe::{Travel, TravelKind};
        let card = Card::new();
        let (keeper, _) = tetris(&card);
        let mario = rom(3, 64 * 1024);
        card.put("ROMS/SFC/Mario.sfc", &mario);
        let mario = card.put("ROMS/SFC/Mario (1).sfc", &mario);
        let plan = card.plan();
        let before = tree(&card.root);
        let refused = |tampered: &DedupePlan, gid: u64| {
            let err = card
                .apply(tampered, &DedupeDecisions::default())
                .unwrap_err();
            assert!(
                matches!(err, ApplyError::Dedupe(DedupeError::EveryMemberMoves(id)) if id == gid),
                "{err:?}"
            );
        };
        let travel = |plan: &mut DedupePlan, system: &str, from: &Path| -> u64 {
            let g = plan.groups.iter_mut().find(|g| g.system == system).unwrap();
            let extra = 1 - g.keeper;
            g.members[extra].travels.push(Travel {
                from: from.to_path_buf(),
                kind: TravelKind::Text,
            });
            g.id
        };

        // The GB extra's travels name its own keeper.
        let mut tampered = plan.clone();
        let gid = travel(&mut tampered, "gb", &keeper);
        refused(&tampered, gid);

        // The SFC extra's travels name the GB keeper.
        let mut tampered = plan.clone();
        let gid = travel(&mut tampered, "snes", &keeper);
        refused(&tampered, gid);

        assert_eq!(tree(&card.root), before);
        assert!(keeper.is_file() && mario.is_file());
        assert!(list_jobs(&card.data).unwrap_or_default().is_empty());
    }

    /// A set rolled back in the run leaves a journal whose undo finds every
    /// file at home: nothing moves, nothing is lost, bytes unchanged.
    #[test]
    fn undo_after_a_rolled_back_set_changes_nothing() {
        let card = Card::new();
        let (_, set) = crash(&card);
        let plan = card.plan();
        let before = tree(&card.root);
        set_copy_fault(Some(fail_track_two));
        let report = card.apply(&plan, &DedupeDecisions::default());
        set_copy_fault(None);
        let report = report.unwrap();
        for f in &set {
            assert!(f.is_file());
        }
        let library = |t: BTreeMap<PathBuf, Vec<u8>>| -> BTreeMap<PathBuf, Vec<u8>> {
            t.into_iter()
                .filter(|(p, _)| !p.starts_with(card.q()))
                .collect()
        };
        assert_eq!(library(tree(&card.root)), before);
        if let Some(job) = report.job.as_deref() {
            let undo = undo_job_with(&card.data, job, &card.trash()).unwrap();
            assert!(undo.archive_lost.is_empty(), "{undo:?}");
            assert!(undo.skipped_occupied.is_empty(), "{undo:?}");
            assert_eq!(library(tree(&card.root)), before, "{undo:?}");
        }
    }

    /// Every rename crosses volumes; the copy of the second bin into the
    /// quarantine fails its verify.
    fn cross_device_and_track_two_unverified(stage: CopyStage, path: &Path) -> Option<io::Error> {
        match stage {
            CopyStage::Rename => Some(io::Error::from(io::ErrorKind::CrossesDevices)),
            CopyStage::Verify => path
                .ends_with(Path::new("_Duplicados/PS/old").join("Crash (Track 2).bin"))
                .then(|| io::Error::other("injected")),
            CopyStage::Delete => None,
        }
    }

    /// A set copied across volumes is rolled back by copying its files home
    /// again (a rename cannot cross volumes): the set stays whole in the
    /// library, byte for byte, and nothing of it is left in the quarantine.
    #[test]
    fn a_set_copied_across_volumes_rolls_back_whole() {
        let card = Card::new();
        let (_, set) = crash(&card);
        let plan = card.plan();
        let bytes: Vec<Vec<u8>> = set.iter().map(|f| fs::read(f).unwrap()).collect();
        let mtimes: Vec<_> = set
            .iter()
            .map(|f| fs::metadata(f).unwrap().modified().unwrap())
            .collect();
        set_copy_fault(Some(cross_device_and_track_two_unverified));
        let report = card.apply(&plan, &DedupeDecisions::default());
        set_copy_fault(None);
        let report = report.unwrap();
        assert!(
            matches!(
                status_of(&report, &plan, &set[0]),
                DedupeItemStatus::Failed { .. }
            ),
            "{report:#?}"
        );
        for (i, f) in set.iter().enumerate() {
            assert_eq!(fs::read(f).unwrap(), bytes[i], "{} is back", f.display());
            assert_eq!(fs::metadata(f).unwrap().modified().unwrap(), mtimes[i]);
        }
        assert!(tree(&card.q()).is_empty(), "{:?}", tree(&card.q()));
    }

    /// Resuming a job that already finished sets nothing aside twice and
    /// keeps every copy where the first run put it.
    #[test]
    fn resuming_a_finished_job_is_harmless() {
        let card = Card::new();
        let (keeper, extra) = tetris(&card);
        let plan = card.plan();
        let report = card.apply(&plan, &DedupeDecisions::default()).unwrap();
        let job = report.job.unwrap();
        let after = tree(&card.root);
        let trash = card.trash();
        let again = resume_dedupe(&job, &card.env(&trash, &|_, _| {}));
        assert_eq!(tree(&card.root), after, "{again:?}");
        assert!(keeper.is_file() && !extra.exists());
        undo_job_with(&card.data, &job, &card.trash()).unwrap();
        assert_eq!(fs::read(&extra).unwrap(), rom(1, 32 * 1024));
    }
}
