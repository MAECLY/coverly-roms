//! Undo of duplicates jobs (duplicates spec §8, plan Task 24): the
//! organise undo path, per op, puts set-aside copies, their saves, art and
//! game-list entries back, from the quarantine or from the Trash.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

use coverly_core::cache::Cache;
use coverly_core::dedupe::source::ZipListing;
use coverly_core::dedupe::testing::{pin_mtime, CountingBytes, FakeDiscProbe};
use coverly_core::dedupe::{
    apply_dedupe, plan_dedupe, DedupeApplyEnv, DedupeDecisions, DedupeEnv, DedupeItemStatus,
    DedupeOptions, DedupePlan, DedupeReport, QuarantineChoice,
};
use coverly_core::profiles::{builtin_profiles, Profile};
use coverly_core::scan::{scan_library, ScanOptions};
use coverly_core::settings::Settings;
use coverly_core::systems::SystemDb;
use coverly_core::write::trash::{FakeTrash, TrashBackend, TrashError};
use coverly_core::write::{
    later_jobs, list_jobs, set_copy_fault, undo_job_with, undo_jobs_in_order_with, CopyStage,
    Fingerprint, OverwritePolicy, Writer,
};
use tempfile::TempDir;

/// Synthetic ROM bytes: binary, distinct per `seed`.
fn rom(seed: u8, len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| if i % 3 == 0 { 0 } else { (i as u8) ^ seed })
        .collect()
}

/// A library in a temp folder, its cache, its data folder and its Trash.
struct Card {
    tmp: TempDir,
    root: PathBuf,
    data: PathBuf,
    cache: Cache,
    profile: Profile,
    settings: Settings,
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

    fn plan_with(&self, quarantine: Option<QuarantineChoice>) -> DedupePlan {
        let mut opts = DedupeOptions::from_settings(&self.settings.dedupe);
        if let Some(q) = quarantine {
            opts.quarantine = q;
        }
        let db = SystemDb::builtin();
        let scan = scan_library(&self.root, &self.profile, &db, &ScanOptions::default());
        let env = DedupeEnv {
            cache: &self.cache,
            db: &db,
            profile: &self.profile,
            settings: &self.settings,
            listing: &ZipListing,
            discs: &FakeDiscProbe::new(),
            dats: None,
            bytes: &CountingBytes::new(),
            progress: &|_| {},
            cancel: &AtomicBool::new(false),
        };
        plan_dedupe(&scan, &opts, &env).unwrap()
    }

    fn plan(&self) -> DedupePlan {
        self.plan_with(None)
    }

    /// Applies `plan` with the default decisions, sending to `trash`.
    fn apply_with(&self, plan: &DedupePlan, trash: &dyn TrashBackend) -> DedupeReport {
        self.apply_decided(plan, &DedupeDecisions::default(), trash)
    }

    fn apply_decided(
        &self,
        plan: &DedupePlan,
        decisions: &DedupeDecisions,
        trash: &dyn TrashBackend,
    ) -> DedupeReport {
        let cancel = AtomicBool::new(false);
        let env = DedupeApplyEnv {
            data_dir: &self.data,
            trash,
            cache: &self.cache,
            progress: &|_, _| {},
            cancel: &cancel,
        };
        apply_dedupe(plan, decisions, &env).unwrap()
    }

    /// Applies `plan` to the quarantine; returns the job id.
    fn apply(&self, plan: &DedupePlan) -> String {
        self.apply_with(plan, &self.trash()).job.unwrap()
    }

    fn q(&self) -> PathBuf {
        self.root.join("_Duplicados")
    }
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

/// Every folder under `dir`.
fn folders(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(read) = fs::read_dir(&d) else { continue };
        for e in read.flatten() {
            if e.path().is_dir() {
                out.push(e.path());
                stack.push(e.path());
            }
        }
    }
    out.sort();
    out
}

fn mtime(p: &Path) -> std::time::SystemTime {
    fs::metadata(p).unwrap().modified().unwrap()
}

fn sorted(mut v: Vec<PathBuf>) -> Vec<PathBuf> {
    v.sort();
    v
}

fn the_job(card: &Card, job: &str) -> coverly_core::write::JobSummary {
    list_jobs(&card.data)
        .unwrap()
        .into_iter()
        .find(|j| j.job == job)
        .unwrap()
}

/// Two identical GB copies: `Tetris.gb` stays (it has a save),
/// `old/Tetris (1).gb` goes.
fn tetris(card: &Card) -> (PathBuf, PathBuf) {
    let bytes = rom(1, 32 * 1024);
    let keeper = card.put("ROMS/GB/Tetris.gb", &bytes);
    card.put("ROMS/GB/Tetris.sav", &rom(20, 8192));
    let extra = card.put("ROMS/GB/old/Tetris (1).gb", &bytes);
    (keeper, extra)
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
fn undo_restores_rom_companions_art_and_gamelist() {
    let card = Card::with_profile("batocera");
    let bytes = rom(1, 32 * 1024);
    let keeper = card.put("roms/gb/Tetris.gb", &bytes);
    let extra = card.put("roms/gb/Tetris (1).gb", &bytes);
    let state = card.put("roms/gb/Tetris (1).state1", b"save state of the copy");
    let art = card.put(
        "roms/gb/images/Tetris (1)-image.png",
        b"\x89PNG\r\n\x1a\nart",
    );
    let xml = "<?xml version=\"1.0\"?>\n<gameList>\n\t<game>\n\t\t<path>./Tetris.gb</path>\n\t\t<name>Tetris</name>\n\t</game>\n\t<game>\n\t\t<path>./Tetris (1).gb</path>\n\t\t<name>Tetris copy</name>\n\t</game>\n</gameList>\n";
    let gamelist = card.put("roms/gb/gamelist.xml", xml.as_bytes());
    let plan = card.plan();
    let g = plan.groups.iter().find(|g| g.system == "gb").unwrap();
    let m = g.members.iter().find(|m| m.unit.primary == keeper).unwrap();
    // The copy has a save state, so the rules would keep it: pick the other.
    let keep = DedupeDecisions {
        keepers: vec![(g.id, m.id)],
        ..DedupeDecisions::default()
    };
    let before = tree(&card.root);
    let stamps: Vec<_> = [&extra, &state, &art].iter().map(|p| mtime(p)).collect();

    let job = card.apply_decided(&plan, &keep, &card.trash()).job.unwrap();
    assert!(!extra.exists() && !state.exists() && !art.exists());
    let q = card.q();
    assert!(q.join("gb/Tetris (1).gb").is_file());
    assert!(q.join("gb/Tetris (1).state1").is_file());
    assert!(q.join("gb/images/Tetris (1)-image.png").is_file());
    assert!(!fs::read_to_string(&gamelist)
        .unwrap()
        .contains("Tetris copy"));

    let report = undo_job_with(&card.data, &job, &card.trash()).unwrap();
    assert_eq!(tree(&card.root), before, "{report:#?}");
    assert_eq!(fs::read_to_string(&gamelist).unwrap(), xml);
    let now: Vec<_> = [&extra, &state, &art].iter().map(|p| mtime(p)).collect();
    assert_eq!(now, stamps, "moved back in place, mtimes kept");
    assert_eq!(
        sorted(report.restored.clone()),
        sorted(vec![extra, state, art, gamelist])
    );
    assert!(report.skipped_modified.is_empty() && report.skipped_occupied.is_empty());
    assert!(!q.exists(), "{report:#?}");
    assert!(the_job(&card, &job).undone);
}

#[test]
fn undo_refuses_with_a_later_job_unless_include_later() {
    let card = Card::new();
    let (_, extra) = tetris(&card);
    let plan = card.plan();
    let before = tree(&card.root);
    let dedupe = card.apply(&plan);
    // A later fill job writes art beside the keeper.
    let mut fill = Writer::begin(&card.root, &card.data).unwrap();
    fill.write(
        &card.root.join("ROMS/GB/Imgs/Tetris.png"),
        b"art",
        OverwritePolicy::Never,
    )
    .unwrap();
    let fill = fill.finish().unwrap();

    let later: Vec<String> = later_jobs(&card.data, &dedupe)
        .unwrap()
        .into_iter()
        .map(|j| j.job)
        .collect();
    assert_eq!(later, std::slice::from_ref(&fill), "the caller refuses");
    assert!(later_jobs(&card.data, &fill).unwrap().is_empty());

    let reports =
        undo_jobs_in_order_with(&card.data, &[dedupe.clone(), fill.clone()], &card.trash())
            .unwrap();
    assert_eq!(
        reports.iter().map(|r| r.job.clone()).collect::<Vec<_>>(),
        [fill, dedupe.clone()],
        "newest first"
    );
    assert!(extra.is_file());
    assert_eq!(tree(&card.root), before);
    assert!(later_jobs(&card.data, &dedupe).unwrap().is_empty());
}

#[test]
fn undo_keeps_a_file_that_took_the_original_place() {
    let card = Card::new();
    let (_, extra) = tetris(&card);
    let plan = card.plan();
    let job = card.apply(&plan);
    let parked = card.q().join("GB/old/Tetris (1).gb");
    assert!(parked.is_file());
    // The user copies another file to the freed name.
    fs::write(&extra, b"a new download").unwrap();

    let report = undo_job_with(&card.data, &job, &card.trash()).unwrap();
    assert_eq!(report.skipped_occupied, std::slice::from_ref(&parked));
    assert_eq!(fs::read(&extra).unwrap(), b"a new download");
    assert_eq!(fs::read(&parked).unwrap(), rom(1, 32 * 1024));
    assert!(
        !card.q().join(format!("{job}.txt")).exists(),
        "the list file goes"
    );
}

/// A Trash that counts the lookups by name.
struct CountingTrash {
    inner: FakeTrash,
    finds: AtomicUsize,
}

impl TrashBackend for CountingTrash {
    fn trash(&self, path: &Path) -> Result<Option<PathBuf>, TrashError> {
        self.inner.trash(path)
    }

    fn find(&self, volume_of: &Path, name: &str, fp: &Fingerprint) -> Option<PathBuf> {
        self.finds.fetch_add(1, Ordering::SeqCst);
        self.inner.find(volume_of, name, fp)
    }

    fn available(&self, path: &Path) -> bool {
        self.inner.available(path)
    }
}

#[test]
fn undo_brings_trashed_copies_back_from_the_exact_place() {
    let card = Card::new();
    let (keeper, extra) = tetris(&card);
    let art = card.put("ROMS/GB/old/Imgs/Tetris (1).png", b"\x89PNG\r\n\x1a\nart");
    let plan = card.plan_with(Some(QuarantineChoice::Trash));
    let before = tree(&card.root);
    let trash = CountingTrash {
        inner: card.trash(),
        finds: AtomicUsize::new(0),
    };
    let report = card.apply_with(&plan, &trash);
    assert!(
        report
            .outcomes
            .iter()
            .any(|o| matches!(o.status, DedupeItemStatus::Trashed { location: Some(_) })),
        "{report:#?}"
    );
    assert!(!extra.exists() && !art.exists() && keeper.is_file());
    assert_eq!(trash.inner.trashed(), 2);

    let undo = undo_job_with(&card.data, report.job.as_deref().unwrap(), &trash).unwrap();
    assert_eq!(trash.finds.load(Ordering::SeqCst), 0, "no lookup by name");
    assert!(undo.archive_lost.is_empty(), "{undo:?}");
    assert_eq!(sorted(undo.restored), sorted(vec![extra, art]));
    assert_eq!(tree(&card.root), before);
    assert!(
        tree(trash.inner.dir()).is_empty(),
        "the Trash gave them back"
    );
}

#[test]
fn undo_finds_a_trashed_copy_whose_location_line_is_missing() {
    let card = Card::new();
    let (_, extra) = tetris(&card);
    let plan = card.plan_with(Some(QuarantineChoice::Trash));
    let before = tree(&card.root);
    let trash = card.trash();
    let job = card.apply_with(&plan, &trash).job.unwrap();
    assert!(!extra.exists());
    // A crash right after the Trash call: the journal ends with the
    // intent, the location line and what follows were never written.
    let journal = card.data.join("jobs").join(format!("{job}.jsonl"));
    let text = fs::read_to_string(&journal).unwrap();
    let cut = text.find("\"archive-trashed\"").expect("a location line");
    let cut = text[..cut].rfind('\n').unwrap() + 1;
    fs::write(&journal, &text[..cut]).unwrap();
    assert!(text[..cut].contains("\"archive-trash\""), "{text}");

    let trash = CountingTrash {
        inner: card.trash(),
        finds: AtomicUsize::new(0),
    };
    let undo = undo_job_with(&card.data, &job, &trash).unwrap();
    assert_eq!(trash.finds.load(Ordering::SeqCst), 1);
    assert_eq!(undo.restored, std::slice::from_ref(&extra));
    assert!(undo.archive_lost.is_empty(), "{undo:?}");
    assert_eq!(tree(&card.root), before);
}

#[test]
fn undo_reports_a_copy_the_trash_lost() {
    let card = Card::new();
    let (_, extra) = tetris(&card);
    let art = card.put("ROMS/GB/old/Imgs/Tetris (1).png", b"\x89PNG\r\n\x1a\nart");
    let plan = card.plan_with(Some(QuarantineChoice::Trash));
    let trash = card.trash();
    let job = card.apply_with(&plan, &trash).job.unwrap();
    // The user empties the Trash of the ROM; the cover is still there.
    let slot = fs::read_dir(trash.dir())
        .unwrap()
        .flatten()
        .map(|e| e.path().join("Tetris (1).gb"))
        .find(|p| p.is_file())
        .unwrap();
    fs::remove_file(slot).unwrap();

    let undo = undo_job_with(&card.data, &job, &trash).unwrap();
    assert_eq!(undo.archive_lost, std::slice::from_ref(&extra));
    assert_eq!(undo.restored, std::slice::from_ref(&art));
    assert!(!extra.exists() && art.is_file());
    assert!(
        the_job(&card, &job).undone,
        "a lost copy still ends the job"
    );
}

fn cross_device_then_crash(stage: CopyStage, _: &Path) -> Option<io::Error> {
    match stage {
        CopyStage::Rename => Some(io::Error::from(io::ErrorKind::CrossesDevices)),
        CopyStage::Verify => panic!("power cut during the copy"),
        CopyStage::Delete => None,
    }
}

#[test]
fn undo_after_crash_mid_copy_removes_the_partial_copy() {
    let card = Card::new();
    let (_, extra) = tetris(&card);
    let plan = card.plan();
    let before = tree(&card.root);
    set_copy_fault(Some(cross_device_then_crash));
    let crashed = panic::catch_unwind(AssertUnwindSafe(|| card.apply(&plan)));
    set_copy_fault(None);
    assert!(crashed.is_err());
    let jobs = list_jobs(&card.data).unwrap();
    assert_eq!(jobs.len(), 1);
    assert!(!jobs[0].complete);
    let job = jobs[0].job.clone();
    // The copy landed but was never verified; the source is still there,
    // and a temp of a second copy was cut short.
    let parked = card.q().join("GB/old/Tetris (1).gb");
    assert!(parked.is_file() && extra.is_file());
    let temp = card.q().join(format!("GB/old/.coverly-{job}-9.tmp"));
    fs::write(&temp, b"half a ROM").unwrap();

    let report = undo_job_with(&card.data, &job, &card.trash()).unwrap();
    assert!(!parked.exists() && !temp.exists(), "{report:#?}");
    assert_eq!(report.restored, std::slice::from_ref(&extra));
    assert_eq!(fs::read(&extra).unwrap(), rom(1, 32 * 1024));
    assert!(!card.q().exists(), "{report:#?}");
    assert_eq!(tree(&card.root), before);
}

#[test]
fn undo_removes_the_job_list_file_and_empty_quarantine_folders() {
    let card = Card::new();
    let (_, extra) = tetris(&card);
    let plan = card.plan();
    let job = card.apply(&plan);
    let q = card.q();
    let list = q.join(format!("{job}.txt"));
    assert!(list.is_file());
    // Finder litter in a quarantine folder does not keep it.
    fs::write(q.join("GB/.DS_Store"), b"finder").unwrap();

    let report = undo_job_with(&card.data, &job, &card.trash()).unwrap();
    assert!(extra.is_file());
    assert_eq!(report.removed, std::slice::from_ref(&list));
    assert!(!q.exists(), "{report:#?}");
    assert_eq!(
        sorted(report.dirs_removed.clone()),
        [q.clone(), q.join("GB"), q.join("GB/old")]
    );

    // A quarantine that also holds an earlier job's copies stays, with
    // them, and only this job's folders and list go.
    let card = Card::new();
    let (_, extra) = tetris(&card);
    let earlier = card.put("_Duplicados/SFC/Mario (1).sfc", b"an earlier job's copy");
    let plan = card.plan();
    let job = card.apply(&plan);
    let report = undo_job_with(&card.data, &job, &card.trash()).unwrap();
    assert!(extra.is_file());
    assert_eq!(fs::read(&earlier).unwrap(), b"an earlier job's copy");
    assert!(!card.q().join(format!("{job}.txt")).exists());
    assert_eq!(folders(&card.q()), [card.q().join("SFC")], "{report:#?}");
}

fn cross_device(stage: CopyStage, _: &Path) -> Option<io::Error> {
    (stage == CopyStage::Rename).then(|| io::Error::from(io::ErrorKind::CrossesDevices))
}

/// Whether `dir`'s volume tells `x` and `X` apart (ext4, case-sensitive
/// APFS): only there can two copies differ by nothing but case.
fn case_sensitive(dir: &Path) -> bool {
    let probe = dir.join("case-probe");
    fs::write(&probe, b"").unwrap();
    let sensitive = !dir.join("CASE-PROBE").exists();
    fs::remove_file(&probe).unwrap();
    sensitive
}

/// `Tetris.gb` and `TETRIS.gb` side by side on a case-sensitive volume:
/// the copy set aside across volumes must never be deleted at undo because
/// its kept twin has the same bytes. The twin is not the copy's own place
/// (a crash between the verified copy and the source deletion), so the
/// copy stays in the quarantine, reported, never lost.
#[test]
fn undo_never_deletes_a_copy_whose_name_a_case_twin_holds() {
    let card = Card::new();
    if !case_sensitive(&card.root) {
        eprintln!("skipped: the temp volume ignores case");
        return;
    }
    let bytes = rom(1, 32 * 1024);
    let keeper = card.put("ROMS/GB/Tetris.gb", &bytes);
    let extra = card.put("ROMS/GB/TETRIS.gb", &bytes);
    let plan = card.plan();
    let g = &plan.groups[0];
    let m = g.members.iter().find(|m| m.unit.primary == keeper).unwrap();
    let keep = DedupeDecisions {
        keepers: vec![(g.id, m.id)],
        ..DedupeDecisions::default()
    };
    set_copy_fault(Some(cross_device));
    let job = card.apply_decided(&plan, &keep, &card.trash()).job;
    set_copy_fault(None);
    let job = job.unwrap();
    let parked = card.q().join("GB/TETRIS.gb");
    assert!(parked.is_file() && !extra.exists() && keeper.is_file());

    let report = undo_job_with(&card.data, &job, &card.trash()).unwrap();
    assert_eq!(fs::read(&keeper).unwrap(), bytes, "{report:#?}");
    assert_eq!(fs::read(&parked).unwrap(), bytes, "{report:#?}");
    assert_eq!(report.skipped_occupied, std::slice::from_ref(&parked));
    assert!(report.restored.is_empty(), "{report:#?}");
}

fn interrupt_tetris(stage: CopyStage, path: &Path) -> Option<io::Error> {
    (stage == CopyStage::Rename && path.to_string_lossy().contains("Tetris (1)"))
        .then(|| io::Error::other("unplugged"))
}

#[test]
fn undo_is_idempotent_after_an_interrupted_undo() {
    let card = Card::new();
    let (tetris, mario) = two_groups(&card);
    let plan = card.plan();
    assert_eq!(plan.groups[0].system, "gb");
    let before = tree(&card.root);
    let job = card.apply(&plan);
    assert!(!tetris.exists() && !mario.exists());

    // Undo runs newest first: Mario comes back, then Tetris fails.
    set_copy_fault(Some(interrupt_tetris));
    let first = undo_job_with(&card.data, &job, &card.trash());
    set_copy_fault(None);
    assert!(first.is_err(), "{first:?}");
    assert!(mario.is_file() && !tetris.exists());
    assert!(!the_job(&card, &job).undone);

    let again = undo_job_with(&card.data, &job, &card.trash()).unwrap();
    assert_eq!(again.restored, std::slice::from_ref(&tetris));
    assert!(again.skipped_modified.is_empty() && again.skipped_occupied.is_empty());
    assert_eq!(tree(&card.root), before);
    assert!(the_job(&card, &job).undone);
    assert!(undo_job_with(&card.data, &job, &card.trash()).is_err());
}
