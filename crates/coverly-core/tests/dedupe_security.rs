//! Security properties of applying and undoing duplicates jobs: nothing is
//! moved, trashed or brought back outside the library (symlinks, tampered
//! journals and plans), a copy that changed since the plan stays, and the
//! Trash gives back only the exact item it took.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use coverly_core::archive::Limits;
use coverly_core::cache::Cache;
use coverly_core::dedupe::archive::ArchiveEntryListing;
use coverly_core::dedupe::testing::{pin_mtime, CountingBytes, FakeDiscProbe};
use coverly_core::dedupe::{
    apply_dedupe, plan_dedupe, resume_dedupe, DedupeApplyEnv, DedupeDecisions, DedupeEnv,
    DedupeOptions, DedupePlan, DedupeReport, QuarantineChoice,
};
use coverly_core::organize::apply::ApplyError;
use coverly_core::profiles::{builtin_profiles, Profile};
use coverly_core::scan::{scan_library, ScanOptions};
use coverly_core::settings::Settings;
use coverly_core::systems::SystemDb;
use coverly_core::write::trash::{FakeTrash, TrashBackend};
use coverly_core::write::{undo_job_with, WriteError};
use tempfile::TempDir;

/// Synthetic ROM bytes: binary, distinct per `seed`.
fn rom(seed: u8, len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| if i % 3 == 0 { 0 } else { (i as u8) ^ seed })
        .collect()
}

/// A library in a temp folder, its cache, its data folder and its Trash,
/// next to a folder outside it that nothing may touch.
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
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("SD");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(tmp.path().join("outside")).unwrap();
        Card {
            root,
            data: tmp.path().join("data"),
            tmp,
            cache: Cache::in_memory().unwrap(),
            profile: builtin_profiles()
                .into_iter()
                .find(|p| p.id == "anbernic-stock")
                .unwrap(),
            settings: Settings::default(),
            next: AtomicU64::new(0),
        }
    }

    fn outside(&self) -> PathBuf {
        self.tmp.path().join("outside")
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
            listing: &ArchiveEntryListing::new(Limits::default()),
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

    fn apply_on(
        &self,
        plan: &DedupePlan,
        trash: &dyn TrashBackend,
        cancel: bool,
    ) -> Result<DedupeReport, ApplyError> {
        let cancel = AtomicBool::new(cancel);
        let env = DedupeApplyEnv {
            data_dir: &self.data,
            trash,
            cache: &self.cache,
            progress: &|_, _| {},
            cancel: &cancel,
        };
        apply_dedupe(plan, &DedupeDecisions::default(), &env)
    }

    fn apply(&self, plan: &DedupePlan) -> Result<DedupeReport, ApplyError> {
        self.apply_on(plan, &self.trash(), false)
    }

    fn resume(&self, job: &str) -> Result<DedupeReport, ApplyError> {
        let cancel = AtomicBool::new(false);
        let trash = self.trash();
        let env = DedupeApplyEnv {
            data_dir: &self.data,
            trash: &trash,
            cache: &self.cache,
            progress: &|_, _| {},
            cancel: &cancel,
        };
        resume_dedupe(job, &env)
    }

    fn q(&self) -> PathBuf {
        self.root.join("_Duplicados")
    }

    fn journal(&self, job: &str) -> PathBuf {
        self.data.join("jobs").join(format!("{job}.jsonl"))
    }
}

/// Every file under `dir` (symlinks not followed): path → bytes.
fn tree(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(read) = fs::read_dir(&d) else { continue };
        for e in read.flatten() {
            let p = e.path();
            let Ok(meta) = fs::symlink_metadata(&p) else {
                continue;
            };
            if meta.is_dir() {
                stack.push(p);
            } else if meta.is_file() {
                out.insert(p.clone(), fs::read(&p).unwrap());
            }
        }
    }
    out
}

/// Tetris twice in GB: the copy in `old/` goes.
fn tetris(card: &Card) -> (PathBuf, PathBuf) {
    let bytes = rom(1, 32 * 1024);
    let keeper = card.put("ROMS/GB/Tetris.gb", &bytes);
    let extra = card.put("ROMS/GB/old/Tetris (1).gb", &bytes);
    (keeper, extra)
}

#[cfg(unix)]
mod symlinks {
    use std::os::unix::fs::symlink;

    use super::*;

    /// `_Duplicados` is a symlink out of the library: apply refuses before
    /// anything moves, and nothing lands outside.
    #[test]
    fn a_symlinked_quarantine_is_refused() {
        let card = Card::new();
        let (keeper, extra) = tetris(&card);
        symlink(card.outside(), card.q()).unwrap();
        let plan = card.plan();
        let before = tree(&card.root);
        let result = card.apply(&plan);
        assert!(
            matches!(result, Err(ApplyError::Write(WriteError::OutsideRoot(_)))),
            "{result:?}"
        );
        assert!(
            tree(&card.outside()).is_empty(),
            "{:?}",
            tree(&card.outside())
        );
        assert_eq!(tree(&card.root), before);
        assert!(keeper.is_file() && extra.is_file());
    }

    /// A folder inside the quarantine links out of the library: the copy
    /// that would land through it stays home.
    #[test]
    fn a_symlinked_folder_inside_the_quarantine_is_never_written_through() {
        let card = Card::new();
        let (_, extra) = tetris(&card);
        fs::create_dir_all(card.q()).unwrap();
        symlink(card.outside(), card.q().join("GB")).unwrap();
        let plan = card.plan();
        let bytes = fs::read(&extra).unwrap();
        let report = card.apply(&plan);
        assert!(tree(&card.outside()).is_empty(), "{report:?}");
        assert_eq!(fs::read(&extra).unwrap(), bytes, "{report:?}");
    }

    /// A copy reached through a symlinked folder of the library is not the
    /// library's: it is never moved out of where it really is.
    #[test]
    fn a_copy_behind_a_symlinked_folder_stays_where_it_is() {
        let card = Card::new();
        let bytes = rom(1, 32 * 1024);
        let keeper = card.put("ROMS/GB/Tetris.gb", &bytes);
        let real = card.outside().join("old");
        fs::create_dir_all(&real).unwrap();
        fs::write(real.join("Tetris (1).gb"), &bytes).unwrap();
        pin_mtime(&real.join("Tetris (1).gb"), 50);
        symlink(&real, card.root.join("ROMS/GB/old")).unwrap();
        let before = tree(&card.outside());
        let plan = card.plan();
        let _ = card.apply(&plan);
        assert_eq!(tree(&card.outside()), before);
        assert!(keeper.is_file());
        let _ = card.apply_on(
            &card.plan_with(Some(QuarantineChoice::Trash)),
            &card.trash(),
            false,
        );
        assert_eq!(tree(&card.outside()), before);
    }

    /// After apply, the quarantine folder is swapped for a symlink out of
    /// the library: undo neither follows it nor deletes through it.
    #[test]
    fn undo_never_follows_a_quarantine_swapped_for_a_symlink() {
        let card = Card::new();
        let (_, extra) = tetris(&card);
        let plan = card.plan();
        let job = card.apply(&plan).unwrap().job.unwrap();
        assert!(!extra.exists());
        // Its content moved out, a link left in its place.
        let moved = card.outside().join("moved");
        fs::rename(card.q().join("GB"), &moved).unwrap();
        symlink(&moved, card.q().join("GB")).unwrap();
        let before = tree(&card.outside());
        let _ = undo_job_with(&card.data, &job, &card.trash());
        assert_eq!(tree(&card.outside()), before);
        assert!(!extra.exists(), "nothing came back through the link");
    }

    /// After apply, the copy's home folder is swapped for a symlink out of
    /// the library: undo never puts the copy through it.
    #[test]
    fn undo_never_puts_a_copy_back_through_a_symlinked_home() {
        let card = Card::new();
        let (_, extra) = tetris(&card);
        let plan = card.plan();
        let job = card.apply(&plan).unwrap().job.unwrap();
        let home = extra.parent().unwrap();
        if home.exists() {
            fs::remove_dir_all(home).unwrap();
        }
        symlink(card.outside(), home).unwrap();
        let _ = undo_job_with(&card.data, &job, &card.trash());
        assert!(tree(&card.outside()).is_empty());
        assert!(card.q().join("GB/old/Tetris (1).gb").is_file());
    }
}

/// A duplicates job whose journal header was edited to take files from
/// another folder: resume refuses it before anything moves.
#[test]
fn resume_refuses_a_journal_whose_source_root_was_changed() {
    let card = Card::new();
    let (_, extra) = tetris(&card);
    let plan = card.plan();
    let job = card.apply(&plan).unwrap().job.unwrap();
    let journal = card.journal(&job);
    let text = fs::read_to_string(&journal).unwrap();
    let (header, rest) = text.split_once('\n').unwrap();
    let mut value: serde_json::Value = serde_json::from_str(header).unwrap();
    value["source_root"] = serde_json::to_value(card.outside()).unwrap();
    fs::write(&journal, format!("{value}\n{rest}")).unwrap();
    let result = card.resume(&job);
    assert!(result.is_err(), "{result:?}");
    assert!(!extra.exists());
    assert!(tree(&card.outside()).is_empty());
}
