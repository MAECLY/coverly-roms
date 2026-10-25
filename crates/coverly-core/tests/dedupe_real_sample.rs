//! Duplicates dry run against the owner's real library and pile
//! (duplicates spec §15, read-only). Run by hand:
//!
//! ```text
//! COVERLY_LIBRARY_DIR=~/Documents/ROMS COVERLY_SAMPLE_DIR=/Volumes/TinyExt/ROMs-Downloaded \
//!   cargo test -p coverly-core --test dedupe_real_sample -- --ignored --nocapture
//! ```
//!
//! Plan only: nothing is moved, and a tree digest (sizes + mtimes) of both
//! folders is compared before and after. The app cache (cached catalogues)
//! is used through a copy in a temp dir, so the planner's cache writes
//! never touch the real one; nothing is downloaded. Discs are not read
//! inside yet (`NoDiscProbe`, Phase A), so §15.3 waits for the disc reader.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::SystemTime;

use coverly_core::cache::{Cache, CACHE_FILE};
use coverly_core::dedupe::keys::DiskBytes;
use coverly_core::dedupe::source::{NoDiscProbe, ZipListing};
use coverly_core::dedupe::units::{units_from_scan, Exclusion, UnitOptions};
use coverly_core::dedupe::{
    plan_dedupe, regroup, DedupeEnv, DedupeOptions, DedupePlan, DupGroup, GroupAction, Relation,
    ReportWhy,
};
use coverly_core::paths::AppDirs;
use coverly_core::profiles::{builtin_profiles, Profile};
use coverly_core::scan::{scan_library, LibraryScan, ScanOptions};
use coverly_core::settings::{DedupeLevel, Settings};
use coverly_core::systems::SystemDb;
use tempfile::TempDir;

/// Spec §15.1, level 1 inside one system, as found: folder → (groups,
/// extras, bytes freed if the largest member stays).
///
/// MD is the measured value, one group above the spec's 14: `MD/413 James
/// Pond 2-Codename RoboCod.bin` and `MD/James Pond II - Codename Robocod
/// (USA, Europe).md` have the same SHA1 (`e00c9ab1…`, 524,288 bytes). The
/// spec's throwaway scan did not key `.md` files; a byte-identical pair in
/// one folder is a level 1 group by definition, so the planner is right.
/// The spec total (88, 89, 1,529,567,750) becomes (89, 90, 1,530,092,038).
const FOUND: [(&str, usize, usize, u64); 9] = [
    ("SMS", 23, 23, 7_734_272),
    ("FC", 17, 17, 5_243_280),
    ("SFC", 14, 14, 22_283_264),
    ("MD", 15, 15, 12_582_912),
    ("GBA", 10, 10, 53_842_560),
    ("GB", 5, 6, 2_129_920),
    ("GG", 2, 2, 1_048_576),
    ("PS", 2, 2, 563_272_656),
    ("DREAMCAST", 1, 1, 861_954_598),
];

/// Spec §15.2: pairs that are never offered as one game, in one folder.
const NEVER_OFFERED: [(&str, &str, &str); 8] = [
    ("PS", "Driver 2.img", "Driver 2.bin"),
    (
        "PS",
        "Colony Wars [Disc1of2].cue",
        "Colony Wars [Disc2of2].cue",
    ),
    ("PS", "Xenogears (Disc 1).cue", "Xenogears (Disc 2).cue"),
    ("PS", "Grandia.bios", "Grandia.PBP"),
    ("SFC", "Jungle Book", "Jungle Book"),
    ("SFC", "Blues Brothers", "Blues Brothers"),
    ("SFC", "King of Dragons", "King of Dragons"),
    ("SFC", "Great Circus Mystery", "Great Circus Mystery"),
];

/// Spec §15.2: different content under one name that may meet only as
/// other versions (level 2, when both hit the catalogue), never as copies.
const VERSIONS_AT_MOST: [(&str, &str, &str); 1] =
    [("PCE", "Taito Chase H.Q. (Japan)", "Taito Chase H.Q. (USA)")];

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

/// The app cache copied into `tmp`, or an empty one there.
fn cache_copy(tmp: &TempDir) -> Cache {
    let copy = tmp.path().join(CACHE_FILE);
    match AppDirs::resolve(None, None).map(|d| d.cache.join(CACHE_FILE)) {
        Some(path) if path.is_file() => {
            fs::copy(&path, &copy).unwrap();
            eprintln!("using a copy of the app cache from {}", path.display());
        }
        _ => eprintln!("no app cache: no catalogues, no level 2"),
    }
    Cache::open(&copy).unwrap()
}

fn anbernic(roms_root: &str) -> Profile {
    let mut p = builtin_profiles()
        .into_iter()
        .find(|p| p.id == "anbernic-stock")
        .unwrap();
    p.roms_root = roms_root.to_string();
    p
}

/// Every level on, offline, nothing remembered.
fn all_levels() -> DedupeOptions {
    DedupeOptions {
        levels: vec![
            DedupeLevel::Exact,
            DedupeLevel::Format,
            DedupeLevel::Version,
        ],
        offline: true,
        ..DedupeOptions::from_settings(&Settings::default().dedupe)
    }
}

fn plan(scan: &LibraryScan, profile: &Profile, cache: &Cache) -> DedupePlan {
    let db = SystemDb::builtin();
    let settings = Settings::default();
    let env = DedupeEnv {
        cache,
        db: &db,
        profile,
        settings: &settings,
        listing: &ZipListing,
        discs: &NoDiscProbe,
        dats: None,
        bytes: &DiskBytes,
        progress: &|_| {},
        cancel: &AtomicBool::new(false),
    };
    plan_dedupe(scan, &all_levels(), &env).unwrap()
}

/// The system folder a path sits in, below the plan's ROMs folder.
fn folder(plan: &DedupePlan, path: &Path) -> String {
    path.strip_prefix(&plan.roms_dir)
        .unwrap_or(path)
        .components()
        .next()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The folder of a group's keeper.
fn group_folder(plan: &DedupePlan, g: &DupGroup) -> String {
    folder(plan, &g.members[g.keeper].unit.primary)
}

/// Found level 1 totals per folder: every exact group, whatever its
/// action; bytes as if the largest member stayed (spec §15.1).
fn found(plan: &DedupePlan) -> BTreeMap<String, (usize, usize, u64)> {
    let mut per: BTreeMap<String, (usize, usize, u64)> = BTreeMap::new();
    for g in &plan.groups {
        assert!(
            g.members.iter().all(|m| m.relation == Relation::Exact),
            "exact-only plan has a {:?} member in {}",
            g.members.iter().map(|m| m.relation).max(),
            g.title
        );
        let largest = g.members.iter().map(|m| m.size).max().unwrap_or(0);
        let total: u64 = g.members.iter().map(|m| m.size).sum();
        let e = per.entry(group_folder(plan, g)).or_default();
        e.0 += 1;
        e.1 += g.members.len() - 1;
        e.2 += total - largest;
    }
    per
}

fn print_groups(plan: &DedupePlan) {
    for g in &plan.groups {
        let action = match &g.action {
            GroupAction::Act => "act".to_string(),
            GroupAction::ReportOnly { why } => format!("report {why:?}"),
            GroupAction::Conflict { .. } => "conflict".to_string(),
            other => format!("{other:?}"),
        };
        let members: Vec<String> = g
            .members
            .iter()
            .map(|m| {
                let keep = if m.id == g.keeper { "*" } else { "" };
                format!(
                    "{keep}{} ({:?}, {} B)",
                    file_name(&m.unit.primary),
                    m.relation,
                    m.size
                )
            })
            .collect();
        eprintln!(
            "| {} | {} | {action} | {:?} | {} |",
            group_folder(plan, g),
            g.title,
            g.reason,
            members.join(" = ")
        );
    }
}

/// The first 16 bytes of a file, in hex.
fn header_hex(path: &Path) -> String {
    use std::io::Read;
    let mut buf = [0u8; 16];
    let n = fs::File::open(path)
        .and_then(|mut f| f.read(&mut buf))
        .unwrap_or(0);
    buf[..n].iter().map(|b| format!("{b:02X}")).collect()
}

/// No group holds both files of a "never offered" pair (matched by name
/// prefix, so `Jungle Book.smc` and `Jungle Book (1).sfc` both match), and
/// a "versions at most" pair shares a group only as other versions.
fn assert_never_offered(plan: &DedupePlan) {
    let pairs = NEVER_OFFERED
        .iter()
        .map(|p| (p, false))
        .chain(VERSIONS_AT_MOST.iter().map(|p| (p, true)));
    for (&(dir, a, b), version_ok) in pairs {
        for g in plan
            .groups
            .iter()
            .filter(|g| group_folder(plan, g).eq_ignore_ascii_case(dir))
        {
            let names: Vec<String> = g
                .members
                .iter()
                .map(|m| file_name(&m.unit.primary))
                .collect();
            let has = |p: &str| names.iter().filter(|n| n.starts_with(p)).count();
            let together = if a == b {
                has(a) >= 2
            } else {
                has(a) >= 1 && has(b) >= 1
            };
            let as_versions = version_ok
                && g.members
                    .iter()
                    .filter(|m| m.id != g.keeper)
                    .filter(|m| {
                        let n = file_name(&m.unit.primary);
                        n.starts_with(a) || n.starts_with(b)
                    })
                    .all(|m| m.relation == Relation::Version);
            assert!(
                !together || as_versions,
                "{dir}: {a} and {b} grouped: {names:?}"
            );
        }
    }
}

#[test]
#[ignore = "needs COVERLY_LIBRARY_DIR and COVERLY_SAMPLE_DIR; plan only, never writes"]
fn the_real_library_plans_duplicates_as_the_spec_says() {
    let (Ok(library), Ok(sample)) = (
        std::env::var("COVERLY_LIBRARY_DIR"),
        std::env::var("COVERLY_SAMPLE_DIR"),
    ) else {
        eprintln!("COVERLY_LIBRARY_DIR or COVERLY_SAMPLE_DIR not set; skipping");
        return;
    };
    let (library, sample) = (PathBuf::from(library), PathBuf::from(sample));
    let before = (digest(&library), digest(&sample));
    let tmp = TempDir::new().unwrap();
    let cache = cache_copy(&tmp);
    let db = SystemDb::builtin();

    // The library: system folders directly in `~/Documents/ROMS`.
    let lib_profile = anbernic("");
    let lib_scan = scan_library(&library, &lib_profile, &db, &ScanOptions::default());

    // `PS/Grandia.bios` is never a unit: the scan does not list it as a
    // ROM, and if it ever did, the BIOS rule would set it aside.
    let set = units_from_scan(&lib_scan, &db, &UnitOptions::for_profile(&lib_profile));
    let bios = Path::new("PS/Grandia.bios");
    if let Some((_, why)) = set.excluded.iter().find(|(p, _)| p.ends_with(bios)) {
        assert_eq!(why, &Exclusion::Bios, "PS/Grandia.bios");
    }
    assert!(
        set.units
            .iter()
            .all(|u| !u.files.iter().any(|f| f.ends_with(bios))),
        "PS/Grandia.bios is part of a unit"
    );

    let full = plan(&lib_scan, &lib_profile, &cache);
    // Level 1 only, from what was read: §15.1 as found.
    let exact = regroup(
        &full,
        &DedupeOptions {
            levels: vec![DedupeLevel::Exact],
            ..all_levels()
        },
    );
    eprintln!("--- level 1 groups ---");
    print_groups(&exact);

    for p in [&full, &exact] {
        // Zero groups span two systems.
        for g in &p.groups {
            let folders: Vec<String> = g
                .members
                .iter()
                .map(|m| folder(p, &m.unit.primary))
                .collect();
            assert!(
                folders.iter().all(|f| *f == folders[0]),
                "{} spans {folders:?}",
                g.title
            );
            assert!(
                !folders.iter().any(|f| f.eq_ignore_ascii_case("PORTS")),
                "PORTS is never compared: {}",
                g.title
            );
        }
        assert_never_offered(p);
    }

    let per = found(&exact);
    eprintln!("found per folder: {per:?}");
    let mut sum = (0, 0, 0);
    for (dir, groups, extras, bytes) in FOUND {
        assert_eq!(
            per.get(dir),
            Some(&(groups, extras, bytes)),
            "{dir}: found {per:?}"
        );
        sum = (sum.0 + groups, sum.1 + extras, sum.2 + bytes);
    }
    assert_eq!(per.len(), FOUND.len(), "only the §15.1 folders: {per:?}");
    assert_eq!(sum, (89, 90, 1_530_092_038));

    // Evidence: what the default rules would act on, and the FC groups
    // whose iNES headers differ (spec §15.1 re-measure note).
    eprintln!("acting (level 1): {:?}", exact.totals);
    eprintln!("acting (all levels): {:?}", full.totals);
    // Other versions add report-only groups; they never hold back an exact
    // copy that acts at level 1.
    assert_eq!(
        (full.totals.extras, full.totals.bytes),
        (exact.totals.extras, exact.totals.bytes),
        "level 2 swallowed exact copies"
    );
    for g in exact.groups.iter().filter(|g| {
        matches!(
            g.action,
            GroupAction::ReportOnly {
                why: ReportWhy::HeaderDiffers
            }
        )
    }) {
        eprintln!("header differs: {} ({})", g.title, group_folder(&exact, g));
        for m in &g.members {
            eprintln!(
                "  {} {}",
                header_hex(&m.unit.primary),
                file_name(&m.unit.primary)
            );
        }
    }

    // The pile: `ROMS/GB/Tetris (JUE) (V1.0)` and `(V1.1)` are one game in
    // two versions, report-only without 1G1R, V1.1 stays (spec §15.4).
    let pile_profile = anbernic("ROMS");
    let pile_scan = scan_library(&sample, &pile_profile, &db, &ScanOptions::default());
    let pile = plan(&pile_scan, &pile_profile, &cache);
    eprintln!("--- pile groups ---");
    print_groups(&pile);
    let tetris = pile
        .groups
        .iter()
        .find(|g| {
            g.members
                .iter()
                .any(|m| file_name(&m.unit.primary) == "Tetris (JUE) (V1.0) [!].zip")
        })
        .expect("Tetris V1.0 and V1.1 grouped");
    let mut names: Vec<String> = tetris
        .members
        .iter()
        .map(|m| file_name(&m.unit.primary))
        .collect();
    names.sort();
    assert_eq!(
        names,
        ["Tetris (JUE) (V1.0) [!].zip", "Tetris (JUE) (V1.1) [!].zip"]
    );
    assert_eq!(
        tetris.action,
        GroupAction::ReportOnly {
            why: ReportWhy::LevelTwoOff
        }
    );
    assert!(tetris
        .members
        .iter()
        .any(|m| m.relation == Relation::Version));
    assert_eq!(
        file_name(&tetris.members[tetris.keeper].unit.primary),
        "Tetris (JUE) (V1.1) [!].zip",
        "{:?}",
        tetris.reason
    );
    // Two exact copies of one catalogue game are not "other versions".
    let worlds = pile
        .groups
        .iter()
        .find(|g| {
            g.members
                .iter()
                .any(|m| file_name(&m.unit.primary) == "Tetris Worlds (USA) (1).zip")
        })
        .expect("Tetris Worlds copies grouped");
    assert_eq!(worlds.action, GroupAction::Act, "{}", worlds.title);
    for g in &pile.groups {
        let folders: Vec<String> = g
            .members
            .iter()
            .map(|m| folder(&pile, &m.unit.primary))
            .collect();
        assert!(folders.iter().all(|f| *f == folders[0]), "{folders:?}");
    }

    assert_eq!(
        (digest(&library), digest(&sample)),
        before,
        "the dry run wrote nothing"
    );
}
