//! Plan-only runs against the owner's real pile (read-only, organise spec
//! §6.7, archives spec §1). The pile lives on an external volume, so the
//! tests are ignored and skip with a message when it is not mounted. Run
//! by hand:
//! cargo test -p coverly-core --features test-util --release --test organize_real_sample -- --ignored --nocapture
//! `COVERLY_SAMPLE_DIR` overrides [`DEFAULT_SAMPLE_DIR`].
//!
//! The app cache (cached DATs) is used when present, through a copy in a
//! temp dir so the plan's memo writes never touch the real one; without it
//! the plan runs on headers alone. Nothing is downloaded.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::SystemTime;

use std::time::{Duration, Instant};

use coverly_core::archive::PLAN_HEAD_BUDGET;
use coverly_core::cache::{Cache, CACHE_FILE};
use coverly_core::identify::sniff::NotARom;
use coverly_core::organize::walk::OsMetaProbe;
use coverly_core::organize::{
    plan_organize, ArchiveAfter, ItemNote, LeftReason, OrganizeAction, OrganizeEnv, OrganizeItem,
    OrganizeOptions, OrganizePlan, SheetCheck, Transfer,
};
use coverly_core::paths::AppDirs;
use coverly_core::profiles::builtin_profiles;
use coverly_core::systems::SystemDb;
use tempfile::TempDir;

/// Where the owner keeps the pile (moved there on 2026-10-24).
const DEFAULT_SAMPLE_DIR: &str = "/Volumes/TinyExt/ROMs-Downloaded";

/// The pile, or `None` (with a message) when the volume is not mounted.
fn sample_dir() -> Option<PathBuf> {
    let dir = std::env::var_os("COVERLY_SAMPLE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_SAMPLE_DIR));
    if dir.is_dir() {
        Some(dir)
    } else {
        eprintln!("{} is not mounted; skipping", dir.display());
        None
    }
}

/// Sorted `(relative path, len, mtime)` of every entry under `root`,
/// hidden ones included, so a stray `._*` or a moved file shows up.
fn digest(root: &Path) -> Vec<(PathBuf, u64, Option<SystemTime>)> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).unwrap() {
            let entry = entry.unwrap();
            let meta = fs::symlink_metadata(entry.path()).unwrap();
            if meta.is_dir() {
                stack.push(entry.path());
            }
            let rel = entry.path().strip_prefix(root).unwrap().to_path_buf();
            out.push((rel, meta.len(), meta.modified().ok()));
        }
    }
    out.sort();
    out
}

/// The app cache copied into `tmp`, or an empty in-memory one.
fn cache_copy(tmp: &TempDir) -> Cache {
    let real = AppDirs::resolve(None, None).map(|d| d.cache.join(CACHE_FILE));
    match real {
        Some(path) if path.is_file() => {
            let copy = tmp.path().join(CACHE_FILE);
            fs::copy(&path, &copy).unwrap();
            eprintln!("using a copy of the cached DATs from {}", path.display());
            Cache::open(&copy).unwrap()
        }
        _ => {
            eprintln!("no app cache: headers and extensions only");
            Cache::in_memory().unwrap()
        }
    }
}

fn plan_offline_with_cached_dats(dir: &Path, cache: &Cache) -> OrganizePlan {
    let db = SystemDb::builtin();
    let profiles = builtin_profiles();
    let env = OrganizeEnv {
        db: &db,
        profiles: &profiles,
        cache,
        dats: None,
        probe: &OsMetaProbe,
        online: None,
        now: SystemTime::now(),
    };
    let opts = OrganizeOptions {
        offline: true,
        download_dats: false,
        ..OrganizeOptions::default()
    };
    plan_organize(dir, &opts, &env, &|_| {}, &AtomicBool::new(false)).unwrap()
}

/// Items that move, per target folder.
fn count_by_folder(plan: &OrganizePlan) -> BTreeMap<String, usize> {
    let mut per = BTreeMap::new();
    for item in plan.items.iter().filter(|i| !i.is_left()) {
        *per.entry(item.folder.clone().unwrap_or_default())
            .or_insert(0) += 1;
    }
    per
}

fn left_reason<'a>(plan: &'a OrganizePlan, name: &str) -> Option<&'a LeftReason> {
    plan.items
        .iter()
        .find(|i| i.sources.first().is_some_and(|s| s == Path::new(name)))
        .and_then(|i| match &i.action {
            OrganizeAction::Left { reason } => Some(reason),
            _ => None,
        })
}

fn print_table(plan: &OrganizePlan) {
    for item in &plan.items {
        let action = match &item.action {
            OrganizeAction::Auto => "auto".to_string(),
            OrganizeAction::Review => "review".to_string(),
            OrganizeAction::Left { reason } => format!("left {reason:?}"),
        };
        let why: Vec<String> = item
            .why
            .iter()
            .map(|e| format!("{:?}:{}:{:.2}", e.kind, e.system, e.weight))
            .collect();
        eprintln!(
            "| {} | {} | {:.2} | {} | {} |",
            item.sources[0].display(),
            item.folder.as_deref().unwrap_or("-"),
            item.confidence,
            action,
            why.join(", ")
        );
    }
    eprintln!("counts: {:?}", plan.counts);
}

/// Cartridge items as `origin/main` (96896da, before archives) planned
/// them on the same pile on 2026-10-24: `(source, folder or None = left)`.
/// The one left (only an `.ips` inside) was `Unknown` there and is
/// `NotARom::Patch` here.
/// The pile's `ROMS/` holds games already in place, so they are not items.
const CARTRIDGES_ON_MAIN: &[(&str, Option<&str>)] = &[
    ("Avatar - the Last Airbender GBA.zip", Some("GBA")),
    ("Dragonball Z - Supersonic Warriors GBA.zip", Some("GBA")),
    ("FIFA Soccer 07.zip", Some("GBA")),
    (
        "Pokemon - Leaf Green Version (U) (V1.1) (1).zip",
        Some("GBA"),
    ),
    ("Pokemon Jupiter - 6.04 (Ruby Hack) (1).zip", None),
    (
        "Pokemon Ultra Violet (1.22) LSA (Fire Red Hack).zip",
        Some("GBA"),
    ),
];

fn item<'a>(plan: &'a OrganizePlan, name: &str) -> &'a OrganizeItem {
    plan.items
        .iter()
        .find(|i| i.sources.first().is_some_and(|s| s == Path::new(name)))
        .unwrap_or_else(|| panic!("no item for {name}"))
}

#[test]
#[ignore = "needs the real pile; plan only, never writes"]
fn the_real_sample_plans_as_the_spec_says() {
    let Some(dir) = sample_dir() else { return };
    let before = digest(&dir);
    let tmp = TempDir::new().unwrap();
    let cache = cache_copy(&tmp);
    let plan = plan_offline_with_cached_dats(&dir, &cache);
    print_table(&plan);
    let per = count_by_folder(&plan);
    assert_eq!(per.get("GBA"), Some(&5), "{per:?}");
    assert_eq!(per.get("PS"), Some(&1), "{per:?}");
    assert_eq!(per.get("DREAMCAST"), Some(&1), "{per:?}");
    assert_eq!(per.get("SATURN"), Some(&1), "{per:?}");
    assert_eq!(per.len(), 4, "{per:?}");
    assert_eq!(plan.counts.review, 0);
    assert!(matches!(
        left_reason(&plan, "3-D Tetris (USA).zip"),
        Some(LeftReason::NotARom { .. })
    ));
    assert!(plan.counts.junk >= 1, ".DS_Store counted as junk");
    assert_eq!(digest(&dir), before, "read-only");
}

/// One disc archive of archives spec §1: what the plan must say.
struct DiscCase {
    name: &'static str,
    system: &'static str,
    folder: &'static str,
    serial: &'static str,
    files: usize,
    bytes: u64,
    sheet_deferred: bool,
    twin_sheets: bool,
}

const DISC_CASES: &[DiscCase] = &[
    DiscCase {
        name: "Next Tetris, The.zip",
        system: "psx",
        folder: "PS",
        serial: "SLUS-00862",
        files: 12,
        bytes: 405_749_493,
        sheet_deferred: false,
        twin_sheets: false,
    },
    DiscCase {
        name: "Next Tetris, The_ On-Line Edition.zip",
        system: "dreamcast",
        folder: "DREAMCAST",
        serial: "T-40214N",
        files: 16,
        bytes: 1_191_067_563,
        sheet_deferred: true,
        twin_sheets: true,
    },
    DiscCase {
        name: "Tetris Plus.zip",
        system: "saturn",
        folder: "SATURN",
        serial: "T-5704H",
        files: 52,
        bytes: 591_881_790,
        sheet_deferred: true,
        twin_sheets: false,
    },
];

fn check_disc(plan: &OrganizePlan, case: &DiscCase) {
    let it = item(plan, case.name);
    let ctx = format!("{}: {it:#?}", case.name);
    assert!(matches!(it.action, OrganizeAction::Auto), "{ctx}");
    assert_eq!(it.system_id.as_deref(), Some(case.system), "{ctx}");
    assert_eq!(it.folder.as_deref(), Some(case.folder), "{ctx}");
    let disc = it.disc.as_ref().expect("disc id");
    assert_eq!(disc.serials, vec![case.serial.to_string()], "{ctx}");
    let Transfer::Extract {
        entries,
        bytes,
        archive_after,
        sheet_check,
        ..
    } = &it.transfer
    else {
        panic!("not an extract: {ctx}");
    };
    assert_eq!(entries.len(), case.files, "{ctx}");
    assert_eq!(*bytes, case.bytes, "{ctx}");
    assert_eq!(
        entries.iter().map(|e| e.size).sum::<u64>(),
        case.bytes,
        "{ctx}"
    );
    // The plan's default mode on one volume is move: nothing is left
    // behind, so the archive goes to the Trash once verified.
    assert_eq!(*archive_after, ArchiveAfter::Trash, "{ctx}");
    let prefix = Path::new("ROMS").join(case.folder);
    for e in entries {
        assert!(
            e.target.starts_with(&prefix),
            "{}: {ctx}",
            e.target.display()
        );
        assert!(e.crc32.is_some(), "7z stores a CRC per entry: {ctx}");
    }
    let mismatch = ItemNote::ContainerMismatch {
        ext: "zip".into(),
        actual: "7z".into(),
    };
    assert!(it.notes.contains(&mismatch), "{ctx}");
    let deferred = *sheet_check == SheetCheck::Deferred;
    assert_eq!(deferred, case.sheet_deferred, "{ctx}");
    assert_eq!(
        it.notes.contains(&ItemNote::SheetNotReadYet),
        case.sheet_deferred,
        "{ctx}"
    );
    assert_eq!(
        it.notes
            .iter()
            .any(|n| matches!(n, ItemNote::TwinSheets { .. })),
        case.twin_sheets,
        "{ctx}"
    );
    assert!(!it.notes.contains(&ItemNote::ArchiveNotFullyRead), "{ctx}");
}

#[test]
#[ignore = "needs the real pile; plan only, never writes"]
fn downloads_disc_archives_plan() {
    let Some(dir) = sample_dir() else { return };
    let before = digest(&dir);
    let tmp = TempDir::new().unwrap();
    let cache = cache_copy(&tmp);
    let opts = OrganizeOptions::default();
    // Every archive the plan opens is bounded by this budget (charged per
    // archive by `Budget::plan_head`); the run time below is a sanity
    // check that the three ~0.4 GB solid 7z files were not decoded whole.
    assert_eq!(opts.limits.plan_head_budget, PLAN_HEAD_BUDGET);
    assert_eq!(PLAN_HEAD_BUDGET, 64 * 1024 * 1024);
    let started = Instant::now();
    let plan = plan_offline_with_cached_dats(&dir, &cache);
    let took = started.elapsed();
    print_table(&plan);
    eprintln!("plan took {took:?}");
    assert!(took < Duration::from_secs(60), "{took:?}");

    for case in DISC_CASES {
        check_disc(&plan, case);
    }
    assert!(matches!(
        left_reason(&plan, "3-D Tetris (USA).zip"),
        Some(LeftReason::NotARom { .. })
    ));
    for (name, folder) in CARTRIDGES_ON_MAIN {
        let it = item(&plan, name);
        assert_eq!(it.folder.as_deref(), *folder, "{name}: {it:#?}");
        assert_eq!(it.is_left(), folder.is_none(), "{name}");
        if folder.is_some() {
            // Same system and action as on main, not just the folder.
            assert_eq!(it.system_id.as_deref(), Some("gba"), "{name}: {it:#?}");
            assert!(matches!(it.action, OrganizeAction::Auto), "{name}: {it:#?}");
        } else {
            assert!(
                matches!(
                    it.action,
                    OrganizeAction::Left {
                        reason: LeftReason::NotARom {
                            what: NotARom::Patch
                        }
                    }
                ),
                "{name}: an IPS patch, not a ROM: {it:#?}"
            );
        }
        assert_eq!(
            it.transfer,
            Transfer::AsIs,
            "{name}: cartridges move as they are"
        );
    }
    let known = DISC_CASES.len() + CARTRIDGES_ON_MAIN.len() + 1;
    assert_eq!(plan.items.len(), known, "no other items");
    assert_eq!(digest(&dir), before, "read-only");
}
