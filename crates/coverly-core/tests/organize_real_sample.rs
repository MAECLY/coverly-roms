//! Plan-only run against the owner's real pile (read-only, organise spec
//! §6.7). Run by hand:
//! COVERLY_SAMPLE_DIR=~/Documents/ROMs-Downloaded cargo test -p coverly-core --test organize_real_sample -- --ignored --nocapture
//!
//! The app cache (cached DATs) is used when present, through a copy in a
//! temp dir so the plan's memo writes never touch the real one; without it
//! the plan runs on headers alone. Nothing is downloaded.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::SystemTime;

use coverly_core::cache::{Cache, CACHE_FILE};
use coverly_core::organize::walk::OsMetaProbe;
use coverly_core::organize::{
    plan_organize, LeftReason, OrganizeAction, OrganizeEnv, OrganizeOptions, OrganizePlan,
};
use coverly_core::paths::AppDirs;
use coverly_core::profiles::builtin_profiles;
use coverly_core::systems::SystemDb;
use tempfile::TempDir;

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

#[test]
#[ignore = "needs COVERLY_SAMPLE_DIR; plan only, never writes"]
fn the_real_sample_plans_as_the_spec_says() {
    let Ok(dir) = std::env::var("COVERLY_SAMPLE_DIR") else {
        eprintln!("COVERLY_SAMPLE_DIR not set; skipping");
        return;
    };
    let dir = PathBuf::from(dir);
    let before = digest(&dir);
    let tmp = TempDir::new().unwrap();
    let cache = cache_copy(&tmp);
    let plan = plan_offline_with_cached_dats(&dir, &cache);
    print_table(&plan);
    let per = count_by_folder(&plan);
    assert_eq!(per.get("GBA"), Some(&6), "{per:?}");
    assert_eq!(per.get("GB"), Some(&6), "{per:?}");
    assert_eq!(per.get("GBC"), Some(&2), "{per:?}");
    assert_eq!(per.get("SFC"), Some(&2), "{per:?}");
    assert_eq!(per.len(), 4, "{per:?}");
    assert_eq!(plan.counts.review, 0);
    assert!(matches!(
        left_reason(&plan, "3-D Tetris (USA).zip"),
        Some(LeftReason::NotARom { .. })
    ));
    assert!(plan.counts.junk >= 1, ".DS_Store counted as junk");
    assert_eq!(digest(&dir), before, "read-only");
}
