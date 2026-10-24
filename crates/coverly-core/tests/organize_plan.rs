//! Planner end to end on a pile shaped like the owner's sample, with
//! synthetic contents and a mock DAT server (organise spec §6.7, §15).

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

use coverly_core::cache::Cache;
use coverly_core::identify::dat::{dat_sources, DatStore};
use coverly_core::identify::evidence::IdLevel;
use coverly_core::identify::sniff::{fixtures as fx, NotARom};
use coverly_core::net::testing::MockServer;
use coverly_core::net::{NetConfig, PoliteHttp, UreqHttp};
use coverly_core::organize::inspect::inspect_file;
use coverly_core::organize::{
    count_library_loose_roms, count_loose_roms, plan_from_survey, plan_organize, probe_folder,
    survey, Choice, Decision, LeftReason, MetaProbe, NameMode, OrganizeAction, OrganizeEnv,
    OrganizeError, OrganizeItem, OrganizeOptions, OrganizePhase, OrganizePlan, OrganizeProgress,
    PlanWarning,
};
use coverly_core::profiles::{builtin_profiles, Profile};
use coverly_core::systems::SystemDb;
use filetime::FileTime;
use tempfile::TempDir;

struct NoCloud;

impl MetaProbe for NoCloud {
    fn cloud_only(&self, _: &Path, _: &fs::Metadata) -> bool {
        false
    }
}

/// Everything the planner consults besides the pile.
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

    fn env(&self) -> OrganizeEnv<'_> {
        OrganizeEnv {
            db: &self.db,
            profiles: &self.profiles,
            cache: &self.cache,
            dats: None,
            probe: &NoCloud,
            online: None,
            now: SystemTime::now(),
        }
    }
}

fn defaults(layout: &str) -> OrganizeOptions {
    OrganizeOptions {
        layout: layout.to_string(),
        ..OrganizeOptions::default()
    }
}

fn plan_with(env: &OrganizeEnv, dir: &Path, opts: &OrganizeOptions) -> OrganizePlan {
    plan_organize(dir, opts, env, &|_| {}, &AtomicBool::new(false)).unwrap()
}

fn plan(world: &World, dir: &Path, opts: OrganizeOptions) -> OrganizePlan {
    plan_with(&world.env(), dir, &opts)
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

/// A deflated zip at `path` with one entry, dated one hour back.
fn zip(path: &Path, inner: &str, bytes: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    common::roms::zip_file(path, &[(inner, bytes, true)]);
    filetime::set_file_mtime(path, hour_ago()).unwrap();
}

/// `(zip name, inner name, inner bytes, system)` of the sample pile.
fn sample_entries() -> Vec<(&'static str, &'static str, Vec<u8>, &'static str)> {
    vec![
        (
            "0319 - Crash Bandicoot XS (E)(Paracox).zip",
            "0319 - Crash Bandicoot XS (E)(Paracox).gba",
            fx::gba_tagged(1),
            "gba",
        ),
        (
            "1490 - Sonic Advance 3 (U)(Venom).zip",
            "1490 - Sonic Advance 3 (U)(Venom).gba",
            fx::gba_tagged(2),
            "gba",
        ),
        (
            "1792 - Tomb Raider - The Prophecy (U)(BatMan).zip",
            "1792 - Tomb Raider.gba",
            fx::gba_tagged(3),
            "gba",
        ),
        (
            "Fire Emblem (E)(Independent).zip",
            "1575 - Fire Emblem (E)(Independent).gba",
            fx::gba_tagged(4),
            "gba",
        ),
        (
            "Pokemon Verde Hoja (S)(Rising Sun).zip",
            "Pokemon Verde Hoja (S).gba",
            fx::gba_tagged(5),
            "gba",
        ),
        (
            "Tetris Worlds (USA).zip",
            "Tetris Worlds (USA).gba",
            fx::gba_tagged(6),
            "gba",
        ),
        (
            "Tetris (JUE) (V1.0) [!].zip",
            "Tetris (JUE) (V1.0) [!].gb",
            fx::gb_tagged(0x00, 1),
            "gb",
        ),
        (
            "Tetris (JUE) (V1.1) [!].zip",
            "Tetris (JUE) (V1.1) [!].gb",
            fx::gb_tagged(0x00, 2),
            "gb",
        ),
        (
            "Tetris 2 (1).zip",
            "Tetris 2 (USA).gb",
            fx::gb_tagged(0x00, 3),
            "gb",
        ),
        (
            "Tetris Attack (1).zip",
            "Tetris Attack (USA).gb",
            fx::gb_tagged(0x00, 4),
            "gb",
        ),
        (
            "Tetris Blast.zip",
            "Tetris Blast (USA).gb",
            fx::gb_tagged(0x00, 5),
            "gb",
        ),
        (
            "Tetris Plus (1).zip",
            "Tetris Plus (USA).gb",
            fx::gb_tagged(0x00, 6),
            "gb",
        ),
        (
            "Magical Tetris Challenge (USA).zip",
            "Magical Tetris Challenge (USA).gbc",
            fx::gb(0xC0),
            "gbc",
        ),
        ("Tetris DX.zip", "Tetris DX (World).gb", fx::gb(0x80), "gbc"),
        (
            "Tetris 2.zip",
            "Tetris 2 (USA).sfc",
            fx::snes(fx::SnesMap::Lo, false),
            "snes",
        ),
        (
            "Tetris Attack.zip",
            "Tetris Attack (USA).sfc",
            fx::snes(fx::SnesMap::Hi, false),
            "snes",
        ),
    ]
}

/// Writes the sample pile into `dir`: mirrors ~/Documents/ROMs-Downloaded
/// by shape only (16 zips, an HTML page named `.zip`, `.DS_Store`).
fn sample_pile(dir: &Path) {
    for (name, inner, bytes, _) in sample_entries() {
        zip(&dir.join(name), inner, &bytes);
    }
    put(&dir.join("3-D Tetris (USA).zip"), &fx::html_page());
    put(&dir.join(".DS_Store"), b"\0\0\0\x01Bud1");
}

fn file_name(path: &Path) -> String {
    path.file_name().unwrap().to_string_lossy().into_owned()
}

fn by_name<'a>(plan: &'a OrganizePlan, name: &str) -> &'a OrganizeItem {
    plan.items
        .iter()
        .find(|i| file_name(&i.sources[0]) == name)
        .unwrap_or_else(|| panic!("no item `{name}`"))
}

fn left_reason(plan: &OrganizePlan, name: &str) -> Option<LeftReason> {
    match &by_name(plan, name).action {
        OrganizeAction::Left { reason } => Some(reason.clone()),
        _ => None,
    }
}

/// Moving items per target folder name.
fn count_by_folder(plan: &OrganizePlan) -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    for item in plan.items.iter().filter(|i| !i.is_left()) {
        let target = item.target.as_ref().expect("a moving item has a target");
        *out.entry(file_name(target.parent().unwrap())).or_default() += 1;
    }
    out
}

/// The plan as JSON with machine-specific values fixed.
fn redact(plan: &OrganizePlan) -> serde_json::Value {
    let mut value = serde_json::to_value(plan).unwrap();
    for key in ["source_root", "target_root", "roms_dir"] {
        value[key] = serde_json::Value::String(format!("<{key}>"));
    }
    value["created_at"] = serde_json::Value::String("<created_at>".into());
    value["fat32"] = serde_json::Value::Bool(false);
    if let Some(items) = value["items"].as_array_mut() {
        for item in items {
            // One snapshot for every OS: targets use `/` like on Unix.
            if let Some(target) = item["target"].as_str().map(|s| s.replace('\\', "/")) {
                item["target"] = serde_json::Value::String(target);
            }
            if let Some(fps) = item["fps"].as_array_mut() {
                for fp in fps {
                    fp["mtime_ns"] = serde_json::Value::from(0);
                }
            }
        }
    }
    value
}

#[test]
fn the_sample_shape_plans_gba_gb_gbc_sfc_and_asks_nothing() {
    let dir = TempDir::new().unwrap();
    sample_pile(dir.path());
    let world = World::new();
    let plan = plan(
        &world,
        dir.path(),
        OrganizeOptions {
            offline: true,
            ..defaults("anbernic-stock")
        },
    );
    assert_eq!(
        count_by_folder(&plan),
        BTreeMap::from([
            ("GB".to_string(), 6),
            ("GBA".to_string(), 6),
            ("GBC".to_string(), 2),
            ("SFC".to_string(), 2),
        ])
    );
    assert_eq!(plan.counts.review, 0);
    assert_eq!(plan.counts.auto, 16);
    assert_eq!(plan.counts.systems, 4);
    assert_eq!(
        left_reason(&plan, "3-D Tetris (USA).zip"),
        Some(LeftReason::NotARom {
            what: NotARom::WebPage
        })
    );
    assert_eq!(plan.counts.junk, 1);
    assert!(plan
        .items
        .iter()
        .filter_map(|i| i.target.as_ref())
        .all(|t| t.starts_with("ROMS")));
    assert_eq!(
        by_name(&plan, "Tetris DX.zip").system_id.as_deref(),
        Some("gbc"),
        "dual-mode without a DAT hit goes to GBC"
    );
    assert!(plan
        .warnings
        .iter()
        .any(|w| matches!(w, PlanWarning::CatalogueMissing { system } if system == "gba")));
    insta::assert_json_snapshot!("sample_offline", redact(&plan));
}

fn http() -> PoliteHttp<UreqHttp> {
    let cfg = NetConfig {
        retries: 0,
        ..NetConfig::default()
    };
    PoliteHttp::new(UreqHttp::new(&cfg), &cfg)
}

#[test]
fn with_a_mock_dat_server_hits_come_from_the_catalogue() {
    let dir = TempDir::new().unwrap();
    sample_pile(dir.path());
    let world = World::new();
    let mut dats: BTreeMap<&str, String> = BTreeMap::new();
    for (name, inner, _, system) in sample_entries() {
        let path = dir.path().join(name);
        let len = fs::metadata(&path).unwrap().len();
        let insp = inspect_file(&path, len, &world.db).unwrap();
        let payload = &insp.payloads[0];
        let stem = inner.rsplit_once('.').unwrap().0;
        dats.entry(system).or_default().push_str(&format!(
            "game (\n name \"{stem} [dat]\"\n rom ( name \"{inner}\" size {} crc {} )\n)\n",
            payload.size,
            payload.crc32.as_deref().unwrap()
        ));
    }
    let server = MockServer::start();
    for (system, text) in &dats {
        for dat in dat_sources(world.db.get(system).unwrap()) {
            server.serve_dat(&dat, text);
        }
    }
    let http = http();
    let store = DatStore::new(&http, &world.cache, server.base());
    let env = OrganizeEnv {
        dats: Some(&store),
        ..world.env()
    };
    let plan = plan_with(&env, dir.path(), &defaults("anbernic-stock"));
    let moving: Vec<&OrganizeItem> = plan.items.iter().filter(|i| !i.is_left()).collect();
    assert_eq!(moving.len(), 16);
    for item in moving {
        assert_eq!(item.method, Some(IdLevel::Hash), "{:?}", item.sources);
        assert!(item
            .catalogue_name
            .as_deref()
            .is_some_and(|n| n.ends_with("[dat]")));
    }
    assert_eq!(count_by_folder(&plan)["GBC"], 2);
    assert!(!plan
        .warnings
        .iter()
        .any(|w| matches!(w, PlanWarning::CatalogueMissing { .. })));
}

#[test]
fn folder_names_follow_the_layout() {
    for (layout, gba) in [
        ("anbernic-stock", "ROMS/GBA"),
        ("esde", "ROMs/gba"),
        ("batocera", "roms/gba"),
        ("muos", "ROMS/gba"),
    ] {
        let dir = TempDir::new().unwrap();
        put(&dir.path().join("Game.gba"), &fx::gba());
        let world = World::new();
        let plan = plan(
            &world,
            dir.path(),
            OrganizeOptions {
                offline: true,
                ..defaults(layout)
            },
        );
        assert_eq!(plan.layout, layout);
        let target = by_name(&plan, "Game.gba").target.clone().unwrap();
        assert_eq!(target, Path::new(gba).join("Game.gba"), "{layout}");
    }
}

#[test]
fn an_unknown_layout_is_an_error() {
    let dir = TempDir::new().unwrap();
    let world = World::new();
    let result = plan_organize(
        dir.path(),
        &defaults("nope"),
        &world.env(),
        &|_| {},
        &AtomicBool::new(false),
    );
    assert!(matches!(result, Err(OrganizeError::UnknownLayout(l)) if l == "nope"));
    let missing = dir.path().join("missing");
    let result = plan_organize(
        &missing,
        &defaults("anbernic-stock"),
        &world.env(),
        &|_| {},
        &AtomicBool::new(false),
    );
    assert!(matches!(result, Err(OrganizeError::SourceMissing(_))));
}

#[test]
fn decisions_assign_leave_and_accept() {
    let dir = TempDir::new().unwrap();
    put(&dir.path().join("Blank.gb"), &fx::zeros(32 * 1024));
    put(&dir.path().join("Other.gb"), &fx::zeros(16 * 1024));
    let world = World::new();
    let env = world.env();
    let opts = OrganizeOptions {
        offline: true,
        ..defaults("anbernic-stock")
    };
    let cancel = AtomicBool::new(false);
    let survey = survey(dir.path(), &opts, &env, &|_| {}, &cancel).unwrap();

    let plain = plan_from_survey(&survey, &opts, &[], &env).unwrap();
    let blank = by_name(&plain, "Blank.gb");
    assert_eq!(blank.action, OrganizeAction::Review);
    let mut candidates: Vec<String> = blank.alternatives.iter().map(|a| a.0.clone()).collect();
    candidates.extend(blank.system_id.clone());
    candidates.sort();
    assert_eq!(candidates, ["gb", "gbc"]);
    let blank_id = blank.id;
    let other_id = by_name(&plain, "Other.gb").id;

    let decided = plan_from_survey(
        &survey,
        &opts,
        &[
            Decision {
                item: blank_id,
                choice: Choice::Assign {
                    system: "gbc".into(),
                },
            },
            Decision {
                item: other_id,
                choice: Choice::Leave,
            },
        ],
        &env,
    )
    .unwrap();
    let blank = by_name(&decided, "Blank.gb");
    assert_eq!(blank.action, OrganizeAction::Auto);
    assert_eq!(blank.method, Some(IdLevel::User));
    assert_eq!(blank.confidence, 1.0);
    assert_eq!(
        blank.target.as_deref(),
        Some(Path::new("ROMS/GBC/Blank.gb"))
    );
    assert_eq!(left_reason(&decided, "Other.gb"), Some(LeftReason::Unknown));

    let accepted = plan_from_survey(
        &survey,
        &opts,
        &[Decision {
            item: other_id,
            choice: Choice::Accept,
        }],
        &env,
    )
    .unwrap();
    assert_eq!(by_name(&accepted, "Other.gb").action, OrganizeAction::Auto);

    let bad_system = plan_from_survey(
        &survey,
        &opts,
        &[Decision {
            item: blank_id,
            choice: Choice::Assign {
                system: "nope".into(),
            },
        }],
        &env,
    );
    assert!(matches!(bad_system, Err(OrganizeError::UnknownSystem(s)) if s == "nope"));
    let bad_item = plan_from_survey(
        &survey,
        &opts,
        &[Decision {
            item: 999,
            choice: Choice::Leave,
        }],
        &env,
    );
    assert!(matches!(bad_item, Err(OrganizeError::UnknownItem(999))));

    // The choice is remembered by content: a new plan settles it alone.
    let again = plan_organize(dir.path(), &opts, &env, &|_| {}, &cancel).unwrap();
    let blank = by_name(&again, "Blank.gb");
    assert_eq!(blank.action, OrganizeAction::Auto);
    assert_eq!(blank.system_id.as_deref(), Some("gbc"));
    assert_eq!(blank.method, Some(IdLevel::User));
}

#[test]
fn decisions_round_trip_as_json() {
    let decisions = vec![
        Decision {
            item: 3,
            choice: Choice::Assign {
                system: "gba".into(),
            },
        },
        Decision {
            item: 4,
            choice: Choice::Leave,
        },
    ];
    let json = serde_json::to_string(&decisions).unwrap();
    assert_eq!(
        json,
        r#"[{"item":3,"choice":"assign","system":"gba"},{"item":4,"choice":"leave"}]"#
    );
    let back: Vec<Decision> = serde_json::from_str(&json).unwrap();
    assert_eq!(back.len(), 2);
}

// Windows finds the home folder through the Known Folder API, not HOME.
#[cfg(unix)]
#[test]
fn generic_files_in_a_broad_folder_are_never_proposed() {
    let home = TempDir::new().unwrap();
    let downloads = home.path().join("Downloads");
    put(&downloads.join("installer.iso"), &fx::zeros(64 * 1024));
    put(
        &downloads.join("firmware.bin"),
        &fx::xorshift_bytes(7, 64 * 1024),
    );
    fs::create_dir_all(&downloads).unwrap();
    zip(
        &downloads.join("photos.zip"),
        "beach.jpg",
        b"\xFF\xD8\xFF\xE0\0\x10JFIF\0",
    );
    // The only test of this binary that reads HOME.
    std::env::set_var("HOME", home.path());
    let world = World::new();
    let plan = plan(
        &world,
        &downloads,
        OrganizeOptions {
            offline: true,
            ..defaults("anbernic-stock")
        },
    );
    assert!(plan.broad_root);
    assert_eq!(plan.counts.review, 0);
    assert_eq!(plan.counts.auto, 0);
    assert!(plan.items.iter().all(|i| i.is_left() && i.target.is_none()));
}

#[test]
fn already_sorted_only_folder_warns() {
    let dir = TempDir::new().unwrap();
    put(&dir.path().join("GBA/a.gba"), &fx::gba());
    let world = World::new();
    let plan = plan(
        &world,
        dir.path(),
        OrganizeOptions {
            offline: true,
            ..defaults("anbernic-stock")
        },
    );
    assert!(plan.items.is_empty());
    assert!(plan.warnings.contains(&PlanWarning::SortedOnly));
    assert_eq!(
        plan.sorted_folders,
        vec![(PathBuf::from("GBA"), "gba".to_string())]
    );
}

#[test]
fn replanning_after_an_option_change_reads_no_file() {
    let dir = TempDir::new().unwrap();
    sample_pile(dir.path());
    let world = World::new();
    let env = world.env();
    let opts = OrganizeOptions {
        offline: true,
        ..defaults("anbernic-stock")
    };
    let survey = survey(dir.path(), &opts, &env, &|_| {}, &AtomicBool::new(false)).unwrap();
    let first = plan_from_survey(&survey, &opts, &[], &env).unwrap();

    // Every candidate's facts are memoised: a later survey reads none.
    let mut memoised = 0;
    for entry in fs::read_dir(dir.path()).unwrap().flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "zip") {
            let meta = fs::metadata(&path).unwrap();
            let mtime = FileTime::from_last_modification_time(&meta);
            let ns = mtime.unix_seconds() * 1_000_000_000 + i64::from(mtime.nanoseconds());
            if world
                .cache
                .organize_facts(&path, meta.len(), ns)
                .unwrap()
                .is_some()
            {
                memoised += 1;
            }
        }
    }
    assert_eq!(memoised, 17);

    let flat = OrganizeOptions {
        flat: true,
        names: NameMode::Catalogue,
        ..opts.clone()
    };
    let second = plan_from_survey(&survey, &flat, &[], &env).unwrap();
    assert_eq!(first.items.len(), second.items.len());
    for (a, b) in first.items.iter().zip(&second.items) {
        assert_eq!(a.sources, b.sources);
        assert_eq!(a.system_id, b.system_id);
        if let (Some(ta), Some(tb)) = (&a.target, &b.target) {
            assert!(ta.starts_with("ROMS"));
            assert!(!tb.starts_with("ROMS"));
            assert_eq!(ta.strip_prefix("ROMS").unwrap(), tb);
        }
    }
    assert_eq!(second.names, NameMode::Catalogue);
}

#[test]
fn cancel_stops_planning() {
    let dir = TempDir::new().unwrap();
    sample_pile(dir.path());
    let world = World::new();
    let cancel = AtomicBool::new(true);
    assert!(matches!(
        plan_organize(
            dir.path(),
            &defaults("anbernic-stock"),
            &world.env(),
            &|_| {},
            &cancel
        ),
        Err(OrganizeError::Cancelled)
    ));
}

#[test]
fn progress_reports_walk_and_identify() {
    let dir = TempDir::new().unwrap();
    sample_pile(dir.path());
    let world = World::new();
    let seen: Mutex<Vec<OrganizeProgress>> = Mutex::new(Vec::new());
    let opts = OrganizeOptions {
        offline: true,
        ..defaults("anbernic-stock")
    };
    plan_organize(
        dir.path(),
        &opts,
        &world.env(),
        &|p| seen.lock().unwrap().push(p),
        &AtomicBool::new(false),
    )
    .unwrap();
    let seen = seen.into_inner().unwrap();
    assert!(seen.iter().any(|p| p.phase == OrganizePhase::Walk));
    let last = seen
        .iter()
        .rev()
        .find(|p| p.phase == OrganizePhase::Identify)
        .expect("identify progress");
    assert_eq!((last.done, last.total), (17, 17));
}

#[test]
fn equal_raw_files_are_hashed_and_marked_duplicate() {
    let dir = TempDir::new().unwrap();
    put(&dir.path().join("Game.gba"), &fx::gba_tagged(9));
    put(&dir.path().join("Game (1).gba"), &fx::gba_tagged(9));
    put(&dir.path().join("Other.gba"), &fx::gba_tagged(10));
    let world = World::new();
    let plan = plan(
        &world,
        dir.path(),
        OrganizeOptions {
            offline: true,
            ..defaults("anbernic-stock")
        },
    );
    assert_eq!(
        left_reason(&plan, "Game (1).gba"),
        Some(LeftReason::Duplicate {
            of: PathBuf::from("Game.gba")
        })
    );
    assert_eq!(by_name(&plan, "Other.gba").action, OrganizeAction::Auto);
    assert_eq!(plan.counts.auto, 2);
}

#[test]
fn a_disc_set_moves_as_one_item() {
    let dir = TempDir::new().unwrap();
    put(
        &dir.path().join("Game.cue"),
        b"FILE \"Game.bin\" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 00:00:00\n",
    );
    put(&dir.path().join("Game.bin"), &fx::psx_iso(true, false));
    put(
        &dir.path().join("Broken.cue"),
        b"FILE \"Broken.bin\" BINARY\n  TRACK 01 MODE2/2352\n",
    );
    let world = World::new();
    let plan = plan(
        &world,
        dir.path(),
        OrganizeOptions {
            offline: true,
            ..defaults("anbernic-stock")
        },
    );
    let game = by_name(&plan, "Game.cue");
    assert_eq!(
        game.sources,
        vec![PathBuf::from("Game.cue"), PathBuf::from("Game.bin")]
    );
    assert_eq!(game.kind, coverly_core::organize::ItemKind::Set);
    assert!(!plan
        .items
        .iter()
        .any(|i| file_name(&i.sources[0]) == "Game.bin"));
    assert_eq!(
        left_reason(&plan, "Broken.cue"),
        Some(LeftReason::SetIncomplete {
            missing: vec!["Broken.bin".into()]
        })
    );
}

#[test]
fn probe_counts_names_only() {
    let dir = TempDir::new().unwrap();
    sample_pile(dir.path());
    put(
        &dir.path().join("Tetris Rush.zip.crdownload"),
        b"half a zip",
    );
    put(&dir.path().join("GBA/a.gba"), &fx::gba());
    let probe = probe_folder(dir.path(), &SystemDb::builtin(), &NoCloud).unwrap();
    assert_eq!(probe.rom_like, 17);
    assert_eq!(probe.partial_downloads, 1);
    assert_eq!(probe.cloud_only, 0);
    assert_eq!(probe.sorted_folders, ["GBA"]);
    assert!(probe.writable);
    assert!(!probe.broad_root);
    assert!(!probe.truncated);
}

#[test]
fn loose_roms_are_counted_in_one_folder_only() {
    let dir = TempDir::new().unwrap();
    put(&dir.path().join("Tetris Worlds (USA).zip"), b"zip");
    put(&dir.path().join("Pokemon.gba"), &fx::gba());
    put(&dir.path().join("Half.zip.crdownload"), b"half");
    put(&dir.path().join(".DS_Store"), b"junk");
    put(&dir.path().join("._Pokemon.gba"), b"appledouble");
    put(&dir.path().join("readme.txt"), b"text");
    put(&dir.path().join("GBA/sorted.gba"), &fx::gba());
    put(&dir.path().join("deeper/elsewhere.gba"), &fx::gba());
    let db = SystemDb::builtin();
    assert_eq!(count_loose_roms(dir.path(), &db), 2);
    assert_eq!(count_loose_roms(&dir.path().join("missing"), &db), 0);

    // A library: its root plus its ROMs folder, whatever its casing.
    put(&dir.path().join("roms/Loose.gb"), b"gb");
    put(&dir.path().join("roms/GB/sorted.gb"), b"gb");
    assert_eq!(count_library_loose_roms(dir.path(), "ROMS", &db), 3);
    assert_eq!(count_library_loose_roms(dir.path(), "", &db), 2);
}

/// GBA header with a two-byte title tag, complement recomputed.
fn gba_tag16(tag: u16) -> Vec<u8> {
    let mut b = fx::gba();
    b[0xA0..0xA2].copy_from_slice(&tag.to_le_bytes());
    b[0xA2] = b'X';
    b[0xBD] = b[0xA0..=0xBC]
        .iter()
        .fold(0u8, |c, v| c.wrapping_sub(*v))
        .wrapping_sub(0x19);
    b
}

#[test]
#[ignore = "perf gate: run with --ignored in CI-local gates"]
fn ten_thousand_gba_headers_plan_without_hashing() {
    let dir = TempDir::new().unwrap();
    for n in 0..10_000u16 {
        put(&dir.path().join(format!("g{n:05}.gba")), &gba_tag16(n));
    }
    let world = World::new();
    let before = coverly_core::organize::plan::HASH_CALLS.load(Ordering::SeqCst);
    let start = Instant::now();
    let plan = plan(
        &world,
        dir.path(),
        OrganizeOptions {
            offline: true,
            ..defaults("anbernic-stock")
        },
    );
    let elapsed = start.elapsed();
    assert!(elapsed < Duration::from_secs(30), "{elapsed:?}");
    assert_eq!(plan.counts.auto, 10_000);
    let methods: BTreeSet<_> = plan.items.iter().map(|i| i.method).collect();
    assert_eq!(methods, BTreeSet::from([Some(IdLevel::Header)]));
    assert_eq!(
        coverly_core::organize::plan::HASH_CALLS.load(Ordering::SeqCst) - before,
        0
    );
}

#[test]
fn a_disc_set_with_a_track_still_being_written_stays_in_place() {
    let dir = TempDir::new().unwrap();
    put(
        &dir.path().join("Game.cue"),
        b"FILE \"Game.bin\" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 00:00:00\n",
    );
    // The track is modified right now: the walk leaves it as recent.
    fs::write(dir.path().join("Game.bin"), fx::psx_iso(true, false)).unwrap();
    let world = World::new();
    let plan = plan(
        &world,
        dir.path(),
        OrganizeOptions {
            offline: true,
            ..defaults("anbernic-stock")
        },
    );
    assert_eq!(
        left_reason(&plan, "Game.cue"),
        Some(LeftReason::RecentlyModified)
    );
    // The track is not listed a second time as an item of its own.
    let mentions = plan
        .items
        .iter()
        .flat_map(|i| i.sources.iter())
        .filter(|s| file_name(s) == "Game.bin")
        .count();
    assert_eq!(mentions, 1);
    assert_eq!(plan.counts.roms, 0);
}

#[cfg(unix)]
#[test]
fn a_disc_set_with_a_symlinked_track_stays_in_place() {
    let dir = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    put(&outside.path().join("Real.bin"), &fx::psx_iso(true, false));
    put(
        &dir.path().join("Game.cue"),
        b"FILE \"Game.bin\" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 00:00:00\n",
    );
    std::os::unix::fs::symlink(outside.path().join("Real.bin"), dir.path().join("Game.bin"))
        .unwrap();
    let world = World::new();
    let plan = plan(
        &world,
        dir.path(),
        OrganizeOptions {
            offline: true,
            ..defaults("anbernic-stock")
        },
    );
    assert_eq!(left_reason(&plan, "Game.cue"), Some(LeftReason::Symlink));
    assert_eq!(plan.counts.roms, 0);
}

#[cfg(unix)]
#[test]
fn a_disc_set_whose_track_sits_behind_a_symlinked_folder_stays_in_place() {
    let dir = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    put(&outside.path().join("Game.bin"), &fx::psx_iso(true, false));
    put(
        &dir.path().join("Game.cue"),
        b"FILE \"tracks/Game.bin\" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 00:00:00\n",
    );
    // `tracks` looks like a folder inside the pile but points outside it.
    std::os::unix::fs::symlink(outside.path(), dir.path().join("tracks")).unwrap();
    let world = World::new();
    let plan = plan(
        &world,
        dir.path(),
        OrganizeOptions {
            offline: true,
            ..defaults("anbernic-stock")
        },
    );
    assert!(
        matches!(
            left_reason(&plan, "Game.cue"),
            Some(LeftReason::Symlink | LeftReason::SetOutsideFolder)
        ),
        "{:?}",
        left_reason(&plan, "Game.cue")
    );
    assert_eq!(plan.counts.roms, 0);
    assert!(outside.path().join("Game.bin").exists());
}

#[test]
fn a_disc_set_whose_track_climbs_out_with_dot_dot_stays_in_place() {
    let root = TempDir::new().unwrap();
    let dir = root.path().join("pile");
    put(&root.path().join("Game.bin"), &fx::psx_iso(true, false));
    put(
        &dir.join("Game.cue"),
        b"FILE \"../Game.bin\" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 00:00:00\n",
    );
    let world = World::new();
    let plan = plan(
        &world,
        &dir,
        OrganizeOptions {
            offline: true,
            ..defaults("anbernic-stock")
        },
    );
    assert!(
        left_reason(&plan, "Game.cue").is_some(),
        "a track outside the pile must hold the set back"
    );
    assert_eq!(plan.counts.roms, 0);
}

#[test]
fn a_raw_file_equal_to_a_zipped_one_is_a_duplicate() {
    let dir = TempDir::new().unwrap();
    zip(
        &dir.path().join("Game.zip"),
        "Game.gba",
        &fx::gba_tagged(21),
    );
    put(&dir.path().join("Game copy.gba"), &fx::gba_tagged(21));
    put(&dir.path().join("Other.gba"), &fx::gba_tagged(22));
    let world = World::new();
    let plan = plan(
        &world,
        dir.path(),
        OrganizeOptions {
            offline: true,
            ..defaults("anbernic-stock")
        },
    );
    let dup = ["Game.zip", "Game copy.gba"]
        .iter()
        .filter(|n| matches!(left_reason(&plan, n), Some(LeftReason::Duplicate { .. })))
        .count();
    assert_eq!(dup, 1, "{:#?}", plan.items);
    assert_eq!(by_name(&plan, "Other.gba").action, OrganizeAction::Auto);
}

#[test]
fn probe_skips_ignored_folders() {
    let dir = TempDir::new().unwrap();
    put(&dir.path().join("bios/scph1001.bin"), &[1u8; 512]);
    put(&dir.path().join("saves/x.gba"), &fx::gba());
    put(&dir.path().join("Game.gba"), &fx::gba());
    let probe = probe_folder(dir.path(), &SystemDb::builtin(), &NoCloud).unwrap();
    assert_eq!(probe.rom_like, 1);
}

#[test]
fn dat_downloads_report_the_dats_phase() {
    let dir = TempDir::new().unwrap();
    zip(
        &dir.path().join("Game.zip"),
        "Game.gba",
        &fx::gba_tagged(31),
    );
    let world = World::new();
    let server = MockServer::start();
    for dat in dat_sources(world.db.get("gba").unwrap()) {
        server.serve_dat(
            &dat,
            "game (\n name \"x\"\n rom ( name \"x.gba\" size 1 crc 00000000 )\n)\n",
        );
    }
    let http = http();
    let store = DatStore::new(&http, &world.cache, server.base());
    let env = OrganizeEnv {
        dats: Some(&store),
        ..world.env()
    };
    let seen: Mutex<Vec<OrganizeProgress>> = Mutex::new(Vec::new());
    plan_organize(
        dir.path(),
        &defaults("anbernic-stock"),
        &env,
        &|p| seen.lock().unwrap().push(p),
        &AtomicBool::new(false),
    )
    .unwrap();
    let seen = seen.into_inner().unwrap();
    assert!(
        seen.iter()
            .any(|p| p.phase == OrganizePhase::Dats && p.done >= 1),
        "{seen:?}"
    );
}

// --- Plan types for extraction (archives spec §4.2, §8.1, §11.2) ---

mod extract_types {
    use std::path::PathBuf;

    use coverly_core::archive::names::UnsafeWhy;
    use coverly_core::archive::Tool;
    use coverly_core::dedupe::source::{DiscId, DiscIdSource};
    use coverly_core::organize::{
        ArchiveAfter, ExtractEntry, ExtractSource, ItemNote, LeftReason, OrganizeAction,
        OrganizePlan, SheetCheck, Transfer,
    };

    const PLAN_V1: &str = include_str!("fixtures/organize/plan-v1.json");

    #[test]
    fn old_plan_json_without_transfer_parses_as_as_is() {
        let plan: OrganizePlan = serde_json::from_str(PLAN_V1).unwrap();
        assert!(!plan.items.is_empty());
        for item in &plan.items {
            assert_eq!(item.transfer, Transfer::AsIs);
            assert_eq!(item.disc, None);
        }
        // Round trip keeps the new defaulted fields.
        let json = serde_json::to_value(&plan).unwrap();
        assert_eq!(json["items"][0]["transfer"]["kind"], "as-is");
        let again: OrganizePlan = serde_json::from_value(json).unwrap();
        assert_eq!(again, plan);
    }

    #[test]
    fn unsupported_archive_old_shape_parses() {
        let plan: OrganizePlan = serde_json::from_str(PLAN_V1).unwrap();
        let reasons: Vec<&LeftReason> = plan
            .items
            .iter()
            .filter_map(|i| match &i.action {
                OrganizeAction::Left { reason } => Some(reason),
                _ => None,
            })
            .collect();
        assert!(reasons.contains(&&LeftReason::UnsupportedArchive {
            format: None,
            need: None
        }));
        let old: LeftReason = serde_json::from_str(r#"{"kind":"unsupported-archive"}"#).unwrap();
        assert_eq!(
            old,
            LeftReason::UnsupportedArchive {
                format: None,
                need: None
            }
        );
        let new = LeftReason::UnsupportedArchive {
            format: Some("rar".into()),
            need: Some(Tool::Unrar),
        };
        let json = serde_json::to_value(&new).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"kind": "unsupported-archive", "format": "rar", "need": "unrar"})
        );
    }

    fn round_trip<T>(value: &T, kind: &str)
    where
        T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
    {
        let json = serde_json::to_value(value).unwrap();
        assert_eq!(json["kind"], kind, "{json}");
        let back: T = serde_json::from_value(json).unwrap();
        assert_eq!(&back, value);
    }

    #[test]
    fn new_left_reasons_round_trip() {
        let table = [
            (LeftReason::EncryptedArchive, "encrypted-archive"),
            (
                LeftReason::ArchiveNeedsMemory { mib: 1536 },
                "archive-needs-memory",
            ),
            (
                LeftReason::ArchiveTooBig {
                    entries: Some(20_000),
                    bytes: None,
                },
                "archive-too-big",
            ),
            (
                LeftReason::MultiVolumeArchive { parts: 3 },
                "multi-volume-archive",
            ),
            (
                LeftReason::UnsafeArchiveEntry {
                    name: "../evil.bin".into(),
                    why: UnsafeWhy::DotDot,
                },
                "unsafe-archive-entry",
            ),
            (
                LeftReason::SetUnsafeName {
                    name: "Track?01.bin".into(),
                },
                "set-unsafe-name",
            ),
            (LeftReason::PathTooLong, "path-too-long"),
            (LeftReason::NestedArchive, "nested-archive"),
            (LeftReason::RepackArcade, "repack-arcade"),
            (
                LeftReason::FolderFull {
                    folder: "PS".into(),
                },
                "folder-full",
            ),
            (LeftReason::UnsafeTarget, "unsafe-target"),
            (
                LeftReason::UnsupportedArchive {
                    format: Some("rar".into()),
                    need: Some(Tool::SevenZip),
                },
                "unsupported-archive",
            ),
        ];
        for (reason, kind) in &table {
            round_trip(reason, kind);
        }
        let json = serde_json::to_value(&table[4].0).unwrap();
        assert_eq!(json["why"], "dot-dot");
    }

    #[test]
    fn new_notes_round_trip() {
        let table = [
            (
                ItemNote::ContainerMismatch {
                    ext: "zip".into(),
                    actual: "7z".into(),
                },
                "container-mismatch",
            ),
            (
                ItemNote::TwinSheets {
                    kept: vec!["gdi".into(), "cue".into()],
                },
                "twin-sheets",
            ),
            (ItemNote::NoStoredChecksum, "no-stored-checksum"),
            (ItemNote::ArchiveNotFullyRead, "archive-not-fully-read"),
            (
                ItemNote::ArchiveKeptLeftovers {
                    count: 2,
                    examples: vec!["Game.sav".into(), "readme.txt".into()],
                },
                "archive-kept-leftovers",
            ),
            (ItemNote::SheetNotReadYet, "sheet-not-read-yet"),
            (ItemNote::MultiRomExtract { count: 3 }, "multi-rom-extract"),
        ];
        for (note, kind) in &table {
            round_trip(note, kind);
        }
    }

    #[test]
    fn extract_transfer_and_disc_id_round_trip() {
        let transfer = Transfer::Extract {
            entries: vec![ExtractEntry {
                index: 0,
                name: "Next Tetris, The (USA)/Next Tetris, The (USA).cue".into(),
                size: 1_234,
                crc32: Some(0xDEAD_BEEF),
                target: PathBuf::from("ROMS/PS/Next Tetris, The (USA).cue"),
            }],
            bytes: 1_234,
            archive_after: ArchiveAfter::Trash,
            source: ExtractSource::Tool {
                tool: Tool::Unrar,
                version: "7.01".into(),
            },
            sheet_check: SheetCheck::Deferred,
            m3u: Some(PathBuf::from("ROMS/PS/Game.m3u")),
        };
        round_trip(&transfer, "extract");
        let json = serde_json::to_value(&transfer).unwrap();
        assert_eq!(json["archive_after"], "trash");
        assert_eq!(json["sheet_check"], "deferred");
        assert_eq!(json["source"]["kind"], "tool");
        assert_eq!(json["source"]["tool"], "unrar");
        // An extract without the optional fields parses with defaults.
        let minimal: Transfer = serde_json::from_value(serde_json::json!({
            "kind": "extract", "entries": [], "bytes": 0, "archive_after": "keep"
        }))
        .unwrap();
        assert_eq!(
            minimal,
            Transfer::Extract {
                entries: vec![],
                bytes: 0,
                archive_after: ArchiveAfter::Keep,
                source: ExtractSource::Builtin,
                sheet_check: SheetCheck::Read,
                m3u: None,
            }
        );
        let disc = DiscId {
            serials: vec!["SLUS-00862".into()],
            disc_no: Some(1),
            volume: None,
            source: DiscIdSource::SystemCnf,
        };
        let back: DiscId = serde_json::from_value(serde_json::to_value(&disc).unwrap()).unwrap();
        assert_eq!(back, disc);
    }
}

/// The planner's extraction decisions (archives spec §8): disc archives
/// extract their set per the profile's disc layout, cartridges extract
/// only when the profile's emulators cannot load the container, and the
/// archive goes to the Trash only when nothing else is in it.
mod extract {
    use super::*;
    use coverly_core::archive::testing::{sevenz, SevenZShape};
    use coverly_core::archive::Format;
    use coverly_core::disc::testing as dt;
    use coverly_core::organize::plan::{
        ArchiveAfter, ExtractEntry, ExtractSource, ItemNote, SheetCheck, Transfer, TransferMode,
    };

    /// A cue sheet over `files`, one track per file.
    fn cue(files: &[&str]) -> Vec<u8> {
        let mut s = String::new();
        for (i, f) in files.iter().enumerate() {
            let mode = if i == 0 { "MODE2/2352" } else { "AUDIO" };
            s.push_str(&format!(
                "FILE \"{f}\" BINARY\r\n  TRACK {:02} {mode}\r\n    INDEX 01 00:00:00\r\n",
                i + 1
            ));
        }
        s.into_bytes()
    }

    fn write7z(path: &Path, files: &[(&str, &[u8])]) {
        put(path, &sevenz(files, SevenZShape::non_solid_lzma()));
    }

    const PS1_DIR: &str = "Next Tetris, The (USA)";
    const PS1_T1: &str = "Next Tetris, The (USA) (Track 01).bin";
    const PS1_T2: &str = "Next Tetris, The (USA) (Track 02).bin";
    const PS1_CUE: &str = "Next Tetris, The (USA).cue";

    /// The PS1 archive of the real case: a 7z named `.zip`, one top
    /// folder, a cue and two tracks.
    fn ps1_archive(path: &Path, serial: &str) {
        let sheet = format!("{PS1_DIR}/{PS1_CUE}");
        let t1 = format!("{PS1_DIR}/{PS1_T1}");
        let t2 = format!("{PS1_DIR}/{PS1_T2}");
        let boot = format!("cdrom:\\{serial};1");
        write7z(
            path,
            &[
                (&sheet, &cue(&[PS1_T1, PS1_T2])),
                (&t1, &dt::psx_track(true, &boot)),
                (&t2, &[0u8; 2352 * 4]),
            ],
        );
    }

    fn opts(layout: &str, target: &Path, mode: TransferMode) -> OrganizeOptions {
        OrganizeOptions {
            target: Some(target.to_path_buf()),
            mode: Some(mode),
            offline: true,
            ..defaults(layout)
        }
    }

    /// Entries, playlist and archive fate of an extract item, with `/`
    /// paths on every OS.
    fn extract_json(item: &OrganizeItem) -> serde_json::Value {
        let mut v = serde_json::to_value(&item.transfer).unwrap();
        if let Some(entries) = v["entries"].as_array_mut() {
            for e in entries {
                if let Some(t) = e["target"].as_str().map(|s| s.replace('\\', "/")) {
                    e["target"] = t.into();
                }
            }
        }
        if let Some(m) = v["m3u"].as_str().map(|s| s.replace('\\', "/")) {
            v["m3u"] = m.into();
        }
        let target = item
            .target
            .as_ref()
            .map(|t| t.to_string_lossy().replace('\\', "/"));
        serde_json::json!({
            "action": serde_json::to_value(&item.action).unwrap()["action"],
            "system": item.system_id,
            "target": target,
            "notes": item.notes,
            "transfer": v,
        })
    }

    fn entries(item: &OrganizeItem) -> &[ExtractEntry] {
        match &item.transfer {
            Transfer::Extract { entries, .. } => entries,
            other => panic!("not an extraction: {other:?}"),
        }
    }

    fn archive_after(item: &OrganizeItem) -> ArchiveAfter {
        match &item.transfer {
            Transfer::Extract { archive_after, .. } => *archive_after,
            other => panic!("not an extraction: {other:?}"),
        }
    }

    fn targets(item: &OrganizeItem) -> Vec<String> {
        entries(item)
            .iter()
            .map(|e| e.target.to_string_lossy().replace('\\', "/"))
            .collect()
    }

    fn m3u(item: &OrganizeItem) -> Option<String> {
        match &item.transfer {
            Transfer::Extract { m3u, .. } => {
                m3u.as_ref().map(|p| p.to_string_lossy().replace('\\', "/"))
            }
            _ => None,
        }
    }

    #[test]
    fn ps1_archive_extracts_flat_on_anbernic() {
        let world = World::new();
        let (s, t) = (TempDir::new().unwrap(), TempDir::new().unwrap());
        ps1_archive(&s.path().join("Next Tetris, The.zip"), "SLUS_008.62");
        let p = plan(
            &world,
            s.path(),
            opts("anbernic-stock", t.path(), TransferMode::Move),
        );
        let item = by_name(&p, "Next Tetris, The.zip");
        assert_eq!(item.action, OrganizeAction::Auto);
        assert_eq!(item.kind, coverly_core::organize::ItemKind::Archive);
        insta::assert_json_snapshot!("ps1_archive_extracts_flat_on_anbernic", extract_json(item));
    }

    #[test]
    fn ps1_archive_game_folder_on_minui() {
        let world = World::new();
        let (s, t) = (TempDir::new().unwrap(), TempDir::new().unwrap());
        ps1_archive(&s.path().join("Next Tetris, The.zip"), "SLUS_008.62");
        let p = plan(
            &world,
            s.path(),
            opts("minui", t.path(), TransferMode::Move),
        );
        let item = by_name(&p, "Next Tetris, The.zip");
        insta::assert_json_snapshot!("ps1_archive_game_folder_on_minui", extract_json(item));
    }

    /// Two discs of one game, cue + one track each.
    fn two_disc_archive(path: &Path) {
        let d1 = "Final Game (USA) (Disc 1).bin";
        let d2 = "Final Game (USA) (Disc 2).bin";
        write7z(
            path,
            &[
                ("Final Game (USA) (Disc 1).cue", &cue(&[d1])),
                ("Final Game (USA) (Disc 2).cue", &cue(&[d2])),
                (d1, &dt::psx_track(true, "cdrom:\\SLUS_008.62;1")),
                (d2, &dt::psx_track(true, "cdrom:\\SLUS_008.63;1")),
            ],
        );
    }

    #[test]
    fn multi_disc_game_folder_with_m3u_on_esde() {
        let world = World::new();
        let (s, t) = (TempDir::new().unwrap(), TempDir::new().unwrap());
        two_disc_archive(&s.path().join("Final Game.7z"));
        let p = plan(&world, s.path(), opts("esde", t.path(), TransferMode::Move));
        let item = by_name(&p, "Final Game.7z");
        assert!(m3u(item)
            .unwrap()
            .ends_with("/Final Game (USA)/Final Game (USA).m3u"));
        insta::assert_json_snapshot!(
            "multi_disc_game_folder_with_m3u_on_esde",
            extract_json(item)
        );
    }

    #[test]
    fn flat_with_m3u_on_batocera() {
        let world = World::new();
        let (s, t) = (TempDir::new().unwrap(), TempDir::new().unwrap());
        two_disc_archive(&s.path().join("Final Game.7z"));
        let p = plan(
            &world,
            s.path(),
            opts("batocera", t.path(), TransferMode::Move),
        );
        let item = by_name(&p, "Final Game.7z");
        assert!(m3u(item).unwrap().ends_with("/psx/Final Game (USA).m3u"));
        insta::assert_json_snapshot!("flat_with_m3u_on_batocera", extract_json(item));

        // Anbernic stock keeps the discs flat and generates nothing.
        let p = plan(
            &world,
            s.path(),
            opts("anbernic-stock", t.path(), TransferMode::Move),
        );
        assert_eq!(m3u(by_name(&p, "Final Game.7z")), None);
    }

    const GDI: &[u8] =
        b"3\r\n1 0 4 2352 track01.bin 0\r\n2 756 0 2352 track02.raw 0\r\n3 45000 4 2352 track03.bin 0\r\n";

    const GD_CUE: &[u8] = b"REM SINGLE-DENSITY AREA\r\n\
        FILE \"track01.bin\" BINARY\r\n  TRACK 01 MODE1/2352\r\n    INDEX 01 00:00:00\r\n\
        FILE \"track02.raw\" BINARY\r\n  TRACK 02 AUDIO\r\n    INDEX 01 00:00:00\r\n\
        REM HIGH-DENSITY AREA\r\n\
        FILE \"track03.bin\" BINARY\r\n  TRACK 03 MODE1/2352\r\n    INDEX 01 00:00:00\r\n";

    #[test]
    fn dreamcast_twins_both_extracted_note_in_flat() {
        let world = World::new();
        let (s, t) = (TempDir::new().unwrap(), TempDir::new().unwrap());
        write7z(
            &s.path().join("Tetris Online.zip"),
            &[
                ("Tetris Online.cue", GD_CUE),
                ("Tetris Online.gdi", GDI),
                ("track01.bin", &dt::dreamcast_track03("T-99999N  ", "1/1")),
                ("track02.raw", &[0u8; 2352 * 4]),
                ("track03.bin", &dt::dreamcast_track03("T40214N   ", "1/1")),
            ],
        );
        let twins = |item: &OrganizeItem| {
            item.notes
                .iter()
                .any(|n| matches!(n, ItemNote::TwinSheets { .. }))
        };
        let p = plan(
            &world,
            s.path(),
            opts("anbernic-stock", t.path(), TransferMode::Move),
        );
        let item = by_name(&p, "Tetris Online.zip");
        assert_eq!(item.system_id.as_deref(), Some("dreamcast"));
        let names: Vec<&str> = entries(item).iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"Tetris Online.cue") && names.contains(&"Tetris Online.gdi"));
        assert_eq!(entries(item).len(), 5, "no source byte is dropped");
        assert!(twins(item), "a flat menu lists both sheets");
        // The item's target is the gdi: what the playlist would name.
        assert!(item.target.as_ref().unwrap().ends_with("Tetris Online.gdi"));

        // One folder per game: the menu shows the folder once, no note.
        let p = plan(
            &world,
            s.path(),
            opts("minui", t.path(), TransferMode::Move),
        );
        let item = by_name(&p, "Tetris Online.zip");
        assert_eq!(entries(item).len(), 5);
        assert!(!twins(item));
    }

    #[test]
    fn cartridge_7z_as_is_on_anbernic_extract_on_minui() {
        let world = World::new();
        let (s, t) = (TempDir::new().unwrap(), TempDir::new().unwrap());
        put(
            &s.path().join("Tetris DX.7z"),
            &sevenz(
                &[("Tetris DX.gbc", &fx::gb(0xC0))],
                SevenZShape::non_solid_lzma(),
            ),
        );
        let p = plan(
            &world,
            s.path(),
            opts("anbernic-stock", t.path(), TransferMode::Move),
        );
        let item = by_name(&p, "Tetris DX.7z");
        assert_eq!(item.action, OrganizeAction::Auto);
        assert_eq!(item.transfer, Transfer::AsIs);
        assert!(item.target.as_ref().unwrap().ends_with("Tetris DX.7z"));

        let p = plan(
            &world,
            s.path(),
            opts("minui", t.path(), TransferMode::Move),
        );
        let item = by_name(&p, "Tetris DX.7z");
        assert_eq!(item.action, OrganizeAction::Auto);
        let got = targets(item);
        assert_eq!(got.len(), 1);
        assert!(got[0].ends_with("/Tetris DX.gbc"), "{got:?}");
        assert_eq!(item.target.as_ref().unwrap(), &entries(item)[0].target);
        assert_eq!(archive_after(item), ArchiveAfter::Trash);
    }

    fn gz(data: &[u8]) -> Vec<u8> {
        use std::io::Write;
        let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        e.write_all(data).unwrap();
        e.finish().unwrap()
    }

    #[test]
    fn game_gba_gz_extracts_everywhere() {
        let world = World::new();
        let (s, t) = (TempDir::new().unwrap(), TempDir::new().unwrap());
        put(&s.path().join("game.gba.gz"), &gz(&fx::gba()));
        for layout in ["anbernic-stock", "minui", "esde", "batocera", "onion"] {
            let p = plan(&world, s.path(), opts(layout, t.path(), TransferMode::Move));
            let item = by_name(&p, "game.gba.gz");
            assert_eq!(item.system_id.as_deref(), Some("gba"), "{layout}");
            let got = targets(item);
            assert_eq!(got.len(), 1, "{layout}");
            assert!(got[0].ends_with("/game.gba"), "{layout}: {got:?}");
            assert_eq!(archive_after(item), ArchiveAfter::Trash, "{layout}");
        }
    }

    #[test]
    fn arcade_set_in_rar_repack_arcade() {
        let world = World::new();
        let (s, t) = (TempDir::new().unwrap(), TempDir::new().unwrap());
        put(
            &s.path().join("sf2.zip"),
            &sevenz(
                &[("sf2.01", b"one"), ("sf2.02", b"two")],
                SevenZShape::non_solid_lzma(),
            ),
        );
        common::roms::zip_file(
            &s.path().join("sf2b.zip"),
            &[("sf2.01", b"one", true), ("sf2.02", b"two", true)],
        );
        filetime::set_file_mtime(s.path().join("sf2b.zip"), hour_ago()).unwrap();
        let env = world.env();
        let o = opts("anbernic-stock", t.path(), TransferMode::Move);
        let mut sv = survey(s.path(), &o, &env, &|_| {}, &AtomicBool::new(false)).unwrap();
        // Stand the 7z in for a RAR an external tool listed: arcade
        // detection does not depend on the format.
        let rar = sv
            .entries
            .iter_mut()
            .find(|e| e.sources[0].ends_with("sf2.zip"))
            .unwrap();
        let insp = rar.inspection.as_mut().expect("an archive");
        insp.container = coverly_core::organize::inspect::Container::Rar;
        insp.format = Some(Format::Rar { v5: true });
        let ids: Vec<usize> = ["sf2.zip", "sf2b.zip"]
            .iter()
            .map(|n| {
                sv.entries
                    .iter()
                    .position(|e| e.sources[0].ends_with(n))
                    .unwrap()
            })
            .collect();
        let decisions: Vec<Decision> = ids
            .iter()
            .map(|&item| Decision {
                item,
                choice: Choice::Assign {
                    system: "mame".into(),
                },
            })
            .collect();
        let p = plan_from_survey(&sv, &o, &decisions, &env).unwrap();
        assert_eq!(left_reason(&p, "sf2.zip"), Some(LeftReason::RepackArcade));
        // A zip set moves as it is, never extracted.
        let zipped = by_name(&p, "sf2b.zip");
        assert_eq!(zipped.action, OrganizeAction::Auto);
        assert_eq!(zipped.transfer, Transfer::AsIs);
    }

    #[test]
    fn sav_beside_rom_keeps_archive_with_leftovers() {
        let world = World::new();
        let (s, t) = (TempDir::new().unwrap(), TempDir::new().unwrap());
        put(
            &s.path().join("Game.7z"),
            &sevenz(
                &[("Game.gba", &fx::gba()), ("Game.sav", b"my progress")],
                SevenZShape::non_solid_lzma(),
            ),
        );
        let p = plan(
            &world,
            s.path(),
            opts("minui", t.path(), TransferMode::Move),
        );
        let item = by_name(&p, "Game.7z");
        assert_eq!(targets(item).len(), 1);
        assert_eq!(archive_after(item), ArchiveAfter::Keep);
        assert!(item.notes.contains(&ItemNote::ArchiveKeptLeftovers {
            count: 1,
            examples: vec!["Game.sav".into()]
        }));
    }

    fn junk_only(path: &Path) {
        put(
            path,
            &sevenz(
                &[
                    ("Game.gba", &fx::gba()),
                    (".DS_Store", b"\0\0\0\x01Bud1"),
                    ("__MACOSX/._Game.gba", b"\x00\x05\x16\x07"),
                ],
                SevenZShape::non_solid_lzma(),
            ),
        );
    }

    #[test]
    fn junk_only_leftovers_trash_archive() {
        let world = World::new();
        let (s, t) = (TempDir::new().unwrap(), TempDir::new().unwrap());
        junk_only(&s.path().join("Game.7z"));
        let p = plan(
            &world,
            s.path(),
            opts("minui", t.path(), TransferMode::Move),
        );
        let item = by_name(&p, "Game.7z");
        assert_eq!(targets(item).len(), 1);
        assert_eq!(archive_after(item), ArchiveAfter::Trash);
        assert!(!item
            .notes
            .iter()
            .any(|n| matches!(n, ItemNote::ArchiveKeptLeftovers { .. })));
    }

    /// A move plan of `Game.7z` (junk-only) on MinUI after `edit` changes
    /// what the survey's inspection says.
    fn junk_only_plan_after(
        world: &World,
        edit: impl FnOnce(&mut coverly_core::organize::inspect::Inspection),
    ) -> OrganizePlan {
        let (s, t) = (TempDir::new().unwrap(), TempDir::new().unwrap());
        junk_only(&s.path().join("Game.7z"));
        let env = world.env();
        let o = opts("minui", t.path(), TransferMode::Move);
        let mut sv = survey(s.path(), &o, &env, &|_| {}, &AtomicBool::new(false)).unwrap();
        let entry = sv
            .entries
            .iter_mut()
            .find(|e| e.sources[0].ends_with("Game.7z"))
            .unwrap();
        edit(entry.inspection.as_mut().expect("an archive"));
        plan_from_survey(&sv, &o, &[], &env).unwrap()
    }

    #[test]
    fn an_incomplete_listing_is_never_extracted_nor_trashed() {
        let world = World::new();
        // Entries past the listing's end are unknown (maybe saves): the
        // archive moves as it is and is never sent to the Trash.
        let p = junk_only_plan_after(&world, |insp| insp.listing_complete = false);
        let item = by_name(&p, "Game.7z");
        assert_eq!(item.transfer, Transfer::AsIs);
        // Control: the same archive, fully listed, goes to the Trash.
        let p = junk_only_plan_after(&world, |_| {});
        assert_eq!(archive_after(by_name(&p, "Game.7z")), ArchiveAfter::Trash);
    }

    #[test]
    fn an_unsafe_entry_that_looks_like_junk_keeps_the_archive() {
        let world = World::new();
        // `__MACOSX/../Game.srm` names a save outside the junk folder: it
        // is never extracted, so the archive is its only copy.
        let p = junk_only_plan_after(&world, |insp| {
            insp.entries
                .push(coverly_core::organize::inspect::ZipEntryInfo {
                    name: "__MACOSX/../Game.srm".into(),
                    crc32: Some("00000000".into()),
                    size: 8,
                    index: Some(99),
                    unsafe_why: Some(coverly_core::archive::names::UnsafeWhy::DotDot),
                });
        });
        let item = by_name(&p, "Game.7z");
        assert_eq!(archive_after(item), ArchiveAfter::Keep);
        assert!(item.notes.contains(&ItemNote::ArchiveKeptLeftovers {
            count: 1,
            examples: vec!["__MACOSX/../Game.srm".into()]
        }));
    }

    #[test]
    fn copy_mode_keeps_archive() {
        let world = World::new();
        let (s, t) = (TempDir::new().unwrap(), TempDir::new().unwrap());
        junk_only(&s.path().join("Game.7z"));
        ps1_archive(&s.path().join("Next Tetris, The.zip"), "SLUS_008.62");
        let p = plan(
            &world,
            s.path(),
            opts("minui", t.path(), TransferMode::Copy),
        );
        assert_eq!(p.mode, TransferMode::Copy);
        for name in ["Game.7z", "Next Tetris, The.zip"] {
            let item = by_name(&p, name);
            assert_eq!(archive_after(item), ArchiveAfter::Keep, "{name}");
            assert!(!item
                .notes
                .iter()
                .any(|n| matches!(n, ItemNote::ArchiveKeptLeftovers { .. })));
        }
    }

    #[test]
    fn two_archives_same_track_name_flat_second_collision_set() {
        let world = World::new();
        let (s, t) = (TempDir::new().unwrap(), TempDir::new().unwrap());
        for (archive, sheet, serial) in [
            ("Alpha.7z", "Alpha.cue", "SLUS_008.62"),
            ("Beta.7z", "Beta.cue", "SLUS_009.99"),
        ] {
            let boot = format!("cdrom:\\{serial};1");
            write7z(
                &s.path().join(archive),
                &[
                    (sheet, &cue(&["Track 01.bin"])),
                    ("Track 01.bin", &dt::psx_track(true, &boot)),
                ],
            );
        }
        let p = plan(
            &world,
            s.path(),
            opts("anbernic-stock", t.path(), TransferMode::Move),
        );
        assert_eq!(by_name(&p, "Alpha.7z").action, OrganizeAction::Auto);
        assert!(targets(by_name(&p, "Alpha.7z"))
            .iter()
            .any(|t| t.ends_with("/PS/Track 01.bin")));
        assert_eq!(left_reason(&p, "Beta.7z"), Some(LeftReason::CollisionSet));
        assert_eq!(by_name(&p, "Beta.7z").transfer, Transfer::AsIs);

        // In a game folder each set has its own folder: no collision.
        let p = plan(
            &world,
            s.path(),
            opts("minui", t.path(), TransferMode::Move),
        );
        assert_eq!(left_reason(&p, "Beta.7z"), None);
    }

    #[test]
    fn existing_target_file_collision_set() {
        let world = World::new();
        let (s, t) = (TempDir::new().unwrap(), TempDir::new().unwrap());
        ps1_archive(&s.path().join("Next Tetris, The.zip"), "SLUS_008.62");
        // Another casing of a track name already sits in `PS/`.
        put(
            &t.path()
                .join("ROMS/PS/next tetris, the (usa) (track 02).bin"),
            b"other",
        );
        let p = plan(
            &world,
            s.path(),
            opts("anbernic-stock", t.path(), TransferMode::Move),
        );
        assert_eq!(
            left_reason(&p, "Next Tetris, The.zip"),
            Some(LeftReason::CollisionSet)
        );
    }

    #[test]
    fn top_folder_dropped_in_flat_replaced_in_game_folder() {
        let world = World::new();
        let (s, t) = (TempDir::new().unwrap(), TempDir::new().unwrap());
        ps1_archive(&s.path().join("Next Tetris, The.zip"), "SLUS_008.62");
        let p = plan(
            &world,
            s.path(),
            opts("anbernic-stock", t.path(), TransferMode::Move),
        );
        let got = targets(by_name(&p, "Next Tetris, The.zip"));
        assert_eq!(
            got,
            [
                format!("ROMS/PS/{PS1_CUE}"),
                format!("ROMS/PS/{PS1_T1}"),
                format!("ROMS/PS/{PS1_T2}"),
            ]
        );
        let p = plan(
            &world,
            s.path(),
            opts("minui", t.path(), TransferMode::Move),
        );
        let item = by_name(&p, "Next Tetris, The.zip");
        let folder = item.folder.clone().unwrap();
        let stem = "Next Tetris, The (USA)";
        let got = targets(item);
        assert_eq!(got.len(), 3);
        for (target, name) in got.iter().zip([PS1_CUE, PS1_T1, PS1_T2]) {
            assert!(
                target.ends_with(&format!("/{folder}/{stem}/{name}")),
                "{target}"
            );
        }
    }

    #[test]
    fn extract_entries_carry_listing_crc_and_size() {
        let world = World::new();
        let (s, t) = (TempDir::new().unwrap(), TempDir::new().unwrap());
        let track = dt::psx_track(true, "cdrom:\\SLUS_008.62;1");
        let sheet = cue(&["Game (USA).bin"]);
        write7z(
            &s.path().join("Game.7z"),
            &[("Game (USA).cue", &sheet), ("Game (USA).bin", &track)],
        );
        let p = plan(
            &world,
            s.path(),
            opts("anbernic-stock", t.path(), TransferMode::Move),
        );
        let item = by_name(&p, "Game.7z");
        let got = entries(item);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].name, "Game (USA).cue");
        assert_eq!(got[0].size, sheet.len() as u64);
        assert_eq!(got[0].crc32, Some(crc32fast::hash(&sheet)));
        assert_eq!(got[1].name, "Game (USA).bin");
        assert_eq!(got[1].size, track.len() as u64);
        assert_eq!(got[1].crc32, Some(crc32fast::hash(&track)));
        assert_ne!(got[0].index, got[1].index);
        match &item.transfer {
            Transfer::Extract {
                bytes,
                source,
                sheet_check,
                ..
            } => {
                assert_eq!(*bytes, (sheet.len() + track.len()) as u64);
                assert_eq!(*source, ExtractSource::Builtin);
                assert_eq!(*sheet_check, SheetCheck::Read);
            }
            other => panic!("{other:?}"),
        }
        assert!(!item.notes.contains(&ItemNote::NoStoredChecksum));
    }

    /// Plan-time checks of archives spec §8.4: free space (cluster
    /// rounding, reserve, the user's quota), FAT32 file and folder limits,
    /// decoder memory, multi-volume sets and incomplete listings.
    mod checks {
        use super::*;
        use coverly_core::archive::testing::{tar_gz, SevenZCoder};
        use coverly_core::archive::Limits;
        use coverly_core::organize::apply::{set_cluster_size_override, set_free_space_override};
        use coverly_core::organize::target::set_fat32_override;

        const MIB: u64 = 1024 * 1024;
        const RESERVE: u64 = 64 * MIB;

        /// Restores the thread's volume overrides when dropped, even when
        /// an assertion fails.
        struct Overrides;

        impl Overrides {
            fn set(free: Option<fn(&Path) -> Option<u64>>, cluster: Option<u64>) -> Overrides {
                set_free_space_override(free);
                set_cluster_size_override(cluster);
                Overrides
            }
        }

        impl Drop for Overrides {
            fn drop(&mut self) {
                set_free_space_override(None);
                set_cluster_size_override(None);
                set_fat32_override(None);
            }
        }

        fn round_up(size: u64, cluster: u64) -> u64 {
            size.div_ceil(cluster) * cluster
        }

        /// What the plan says it needs: the extracted bytes rounded up to
        /// whole clusters, 1 % and the reserve.
        fn needed(sizes: &[u64], cluster: u64) -> u64 {
            let sum: u64 = sizes.iter().map(|&s| round_up(s, cluster)).sum();
            sum + sum.div_ceil(100) + RESERVE
        }

        fn no_space(p: &OrganizePlan) -> Option<(u64, u64)> {
            p.warnings.iter().find_map(|w| match w {
                PlanWarning::NoSpace { needed, free } => Some((*needed, *free)),
                _ => None,
            })
        }

        fn entry_sizes(item: &OrganizeItem) -> Vec<u64> {
            entries(item).iter().map(|e| e.size).collect()
        }

        #[test]
        fn free_space_counts_cluster_rounding_and_reserve() {
            let world = World::new();
            let (s, t) = (TempDir::new().unwrap(), TempDir::new().unwrap());
            ps1_archive(&s.path().join("Next Tetris, The.zip"), "SLUS_008.62");
            let o = opts("anbernic-stock", t.path(), TransferMode::Move);

            let _guard = Overrides::set(Some(|_| Some(1)), Some(32 * 1024));
            let p = plan(&world, s.path(), o.clone());
            let item = by_name(&p, "Next Tetris, The.zip");
            let sizes = entry_sizes(item);
            assert_eq!(sizes.len(), 3);
            let want = needed(&sizes, 32 * 1024);
            // 3 files of a few KiB take three whole 32 KiB clusters.
            assert!(want >= 3 * 32 * 1024 + RESERVE);
            assert_eq!(no_space(&p), Some((want, 1)));
            // The item is still planned: the warning is about the volume.
            assert_eq!(item.action, OrganizeAction::Auto);

            // Smaller clusters waste less.
            let _guard = Overrides::set(Some(|_| Some(1)), Some(512));
            let p = plan(&world, s.path(), o.clone());
            assert_eq!(no_space(&p), Some((needed(&sizes, 512), 1)));

            // Enough room: no warning.
            let _guard = Overrides::set(Some(|_| Some(1 << 40)), Some(32 * 1024));
            let p = plan(&world, s.path(), o);
            assert_eq!(no_space(&p), None);
        }

        #[test]
        fn move_same_volume_does_not_credit_archive() {
            let world = World::new();
            let (s, t) = (TempDir::new().unwrap(), TempDir::new().unwrap());
            ps1_archive(&s.path().join("Next Tetris, The.zip"), "SLUS_008.62");
            // A cartridge zip that moves as it is on the same volume needs
            // no new space.
            zip(&s.path().join("Advance.zip"), "Advance.gba", &fx::gba());
            let o = opts("anbernic-stock", t.path(), TransferMode::Move);

            let _guard = Overrides::set(Some(|_| Some(0)), Some(4096));
            let p = plan(&world, s.path(), o.clone());
            assert_eq!(p.mode, TransferMode::Move);
            let archive = s.path().join("Next Tetris, The.zip");
            let archive_size = fs::metadata(&archive).unwrap().len();
            let item = by_name(&p, "Next Tetris, The.zip");
            assert_eq!(archive_after(item), ArchiveAfter::Trash);
            let want = needed(&entry_sizes(item), 4096);
            // The Trash frees nothing on the same volume: the archive's
            // size is not taken off.
            assert_eq!(no_space(&p), Some((want, 0)));
            assert!(want > archive_size);
            assert_eq!(by_name(&p, "Advance.zip").transfer, Transfer::AsIs);

            // Only as-is moves on one volume: nothing to check.
            fs::remove_file(&archive).unwrap();
            let p = plan(&world, s.path(), o);
            assert_eq!(by_name(&p, "Advance.zip").action, OrganizeAction::Auto);
            assert_eq!(no_space(&p), None);
        }

        #[test]
        fn no_space_warning_has_exact_numbers() {
            let world = World::new();
            let (s, t) = (TempDir::new().unwrap(), TempDir::new().unwrap());
            ps1_archive(&s.path().join("Next Tetris, The.zip"), "SLUS_008.62");
            let rom = fx::gba_tagged(7);
            put(&s.path().join("Advance.gba"), &rom);
            let o = opts("anbernic-stock", t.path(), TransferMode::Copy);

            let _guard = Overrides::set(Some(|_| Some(1000)), Some(4096));
            let p = plan(&world, s.path(), o);
            assert_eq!(p.mode, TransferMode::Copy);
            // A copy writes the loose file too, rounded like the rest.
            let mut sizes = entry_sizes(by_name(&p, "Next Tetris, The.zip"));
            sizes.push(rom.len() as u64);
            assert_eq!(by_name(&p, "Advance.gba").action, OrganizeAction::Auto);
            assert_eq!(no_space(&p), Some((needed(&sizes, 4096), 1000)));
        }

        #[test]
        fn fat32_track_4gib_declared_too_big_for_fat() {
            let world = World::new();
            let (s, t) = (TempDir::new().unwrap(), TempDir::new().unwrap());
            ps1_archive(&s.path().join("Next Tetris, The.zip"), "SLUS_008.62");
            let env = world.env();
            let o = opts("anbernic-stock", t.path(), TransferMode::Move);
            let sv = survey(s.path(), &o, &env, &|_| {}, &AtomicBool::new(false)).unwrap();
            let _guard = Overrides::set(Some(|_| Some(1 << 50)), None);
            set_fat32_override(Some(true));

            let with_track = |size: u64| {
                let mut sv = sv.clone();
                let entry = sv
                    .entries
                    .iter_mut()
                    .find(|e| e.sources[0].ends_with("Next Tetris, The.zip"))
                    .unwrap();
                let insp = entry.inspection.as_mut().expect("an archive");
                let track = insp
                    .entries
                    .iter_mut()
                    .find(|e| e.name.ends_with(PS1_T2))
                    .unwrap();
                // The declared size, which the extraction's `take` makes
                // binding.
                track.size = size;
                plan_from_survey(&sv, &o, &[], &env).unwrap()
            };
            let p = with_track(4 * 1024 * MIB);
            assert!(p.fat32);
            assert_eq!(
                left_reason(&p, "Next Tetris, The.zip"),
                Some(LeftReason::TooBigForFat)
            );
            assert_eq!(by_name(&p, "Next Tetris, The.zip").transfer, Transfer::AsIs);

            let p = with_track(4 * 1024 * MIB - 1);
            assert_eq!(left_reason(&p, "Next Tetris, The.zip"), None);
        }

        /// `count` files in `dir` whose 247-unit names take 20 FAT32
        /// directory slots each.
        fn fill_folder(dir: &Path, count: usize) {
            fs::create_dir_all(dir).unwrap();
            for i in 0..count {
                let name = format!("{i:05}{}.bin", "x".repeat(238));
                assert_eq!(name.encode_utf16().count(), 247);
                fs::write(dir.join(name), b"").unwrap();
            }
        }

        #[test]
        fn fat32_folder_slots_folder_full() {
            let world = World::new();
            let s = TempDir::new().unwrap();
            ps1_archive(&s.path().join("Next Tetris, The.zip"), "SLUS_008.62");
            let _guard = Overrides::set(Some(|_| Some(1 << 50)), None);
            set_fat32_override(Some(true));

            // 3,250 × 20 + `.` and `..` = 65,002 slots: over 65,000.
            let full = TempDir::new().unwrap();
            fill_folder(&full.path().join("ROMS/PS"), 3_250);
            let p = plan(
                &world,
                s.path(),
                opts("anbernic-stock", full.path(), TransferMode::Move),
            );
            assert_eq!(
                left_reason(&p, "Next Tetris, The.zip"),
                Some(LeftReason::FolderFull {
                    folder: "PS".into()
                })
            );

            // 3,249 × 20 + 2 = 64,982, plus the cue (3) and two tracks
            // (4 each) = 64,993: still fits.
            let room = TempDir::new().unwrap();
            fill_folder(&room.path().join("ROMS/PS"), 3_249);
            let p = plan(
                &world,
                s.path(),
                opts("anbernic-stock", room.path(), TransferMode::Move),
            );
            assert_eq!(left_reason(&p, "Next Tetris, The.zip"), None);
            assert_eq!(targets(by_name(&p, "Next Tetris, The.zip")).len(), 3);

            // Not FAT32: no slot limit.
            set_fat32_override(Some(false));
            let p = plan(
                &world,
                s.path(),
                opts("anbernic-stock", full.path(), TransferMode::Move),
            );
            assert_eq!(left_reason(&p, "Next Tetris, The.zip"), None);
        }

        #[test]
        fn archive_needs_memory_left_with_mib() {
            let world = World::new();
            let (s, t) = (TempDir::new().unwrap(), TempDir::new().unwrap());
            put(
                &s.path().join("Big Dictionary.7z"),
                &sevenz(
                    &[("Big Dictionary.gba", &fx::gba())],
                    SevenZShape {
                        solid: false,
                        coder: SevenZCoder::Lzma { dict: 16 << 20 },
                        sheets_last: false,
                    },
                ),
            );
            let o = OrganizeOptions {
                limits: Limits {
                    decoder_mem_cap: 4 * MIB,
                    external_tools: false,
                    ..Limits::default()
                },
                ..opts("anbernic-stock", t.path(), TransferMode::Move)
            };
            let p = plan(&world, s.path(), o);
            match left_reason(&p, "Big Dictionary.7z") {
                Some(LeftReason::ArchiveNeedsMemory { mib }) => {
                    assert!((16..=32).contains(&mib), "{mib} MiB")
                }
                other => panic!("expected ArchiveNeedsMemory, got {other:?}"),
            }
        }

        #[test]
        fn multi_volume_7z_001_002_one_item_002_never_moved() {
            let world = World::new();
            let (s, t) = (TempDir::new().unwrap(), TempDir::new().unwrap());
            let whole = sevenz(
                &[("Split Game.gba", &fx::gba())],
                SevenZShape::non_solid_lzma(),
            );
            let (a, b) = whole.split_at(whole.len() / 2);
            put(&s.path().join("Split Game.7z.001"), a);
            put(&s.path().join("Split Game.7z.002"), b);
            let p = plan(
                &world,
                s.path(),
                opts("anbernic-stock", t.path(), TransferMode::Move),
            );
            let parts: Vec<&OrganizeItem> = p
                .items
                .iter()
                .filter(|i| i.sources.iter().any(|s| file_name(s).starts_with("Split")))
                .collect();
            assert_eq!(parts.len(), 1, "{:#?}", p.items);
            let item = parts[0];
            assert_eq!(
                item.action,
                OrganizeAction::Left {
                    reason: LeftReason::MultiVolumeArchive { parts: 2 }
                }
            );
            let names: Vec<String> = item.sources.iter().map(|s| file_name(s)).collect();
            assert_eq!(names, ["Split Game.7z.001", "Split Game.7z.002"]);
            assert_eq!(item.size, whole.len() as u64);
            assert_eq!(item.fps.len(), 2);
            assert!(item.target.is_none());
            assert!(!p
                .other_files
                .iter()
                .any(|f| file_name(f).starts_with("Split")));
        }

        #[test]
        fn truncated_tar_listing_never_extract() {
            let world = World::new();
            let (s, t) = (TempDir::new().unwrap(), TempDir::new().unwrap());
            let pad: Vec<u8> = (0..2 * 1024 * 1024u32)
                .map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8)
                .collect();
            put(
                &s.path().join("Game.tar.gz"),
                &tar_gz(
                    &[("Game.gba", &fx::gba()), ("pad.dat", &pad), ("z.txt", b"z")],
                    0,
                ),
            );
            let o = OrganizeOptions {
                limits: Limits {
                    plan_listing_budget: 256 * 1024,
                    external_tools: false,
                    ..Limits::default()
                },
                // MinUI loads no tar: a whole listing would be extracted.
                ..opts("minui", t.path(), TransferMode::Move)
            };
            let env = world.env();
            let sv = survey(s.path(), &o, &env, &|_| {}, &AtomicBool::new(false)).unwrap();
            let p = plan_from_survey(&sv, &o, &[], &env).unwrap();
            let item = by_name(&p, "Game.tar.gz");
            assert_eq!(item.action, OrganizeAction::Review);
            assert!(item.notes.contains(&ItemNote::ArchiveNotFullyRead));
            assert_eq!(item.transfer, Transfer::AsIs);

            for choice in [
                Choice::Accept,
                Choice::Assign {
                    system: "gba".into(),
                },
            ] {
                let decisions = [Decision {
                    item: item.id,
                    choice,
                }];
                let p = plan_from_survey(&sv, &o, &decisions, &env).unwrap();
                let item = by_name(&p, "Game.tar.gz");
                assert_eq!(item.action, OrganizeAction::Auto);
                assert_eq!(item.transfer, Transfer::AsIs, "never extracted");
                assert!(item.target.as_ref().unwrap().ends_with("Game.tar.gz"));
            }

            // An arcade set must be a zip whatever its listing: a cut tar
            // is never moved into the arcade folder as it is.
            let decisions = [Decision {
                item: item.id,
                choice: Choice::Assign {
                    system: "mame".into(),
                },
            }];
            let p = plan_from_survey(&sv, &o, &decisions, &env).unwrap();
            assert_eq!(
                left_reason(&p, "Game.tar.gz"),
                Some(LeftReason::RepackArcade)
            );
            assert!(by_name(&p, "Game.tar.gz").target.is_none());
        }

        #[test]
        fn plan_head_budget_asserted_64mib() {
            assert_eq!(
                OrganizeOptions::default().limits.plan_head_budget,
                RESERVE,
                "the plan head budget is 64 MiB"
            );
            // A solid archive storing its sheet after 72 MiB of tracks, as
            // the real Saturn and Dreamcast archives do (stored, so the
            // test needs no slow encoder).
            let world = World::new();
            let (s, t) = (TempDir::new().unwrap(), TempDir::new().unwrap());
            let names = [
                "Tetris Plus (Japan) (Track 01).bin",
                "Tetris Plus (Japan) (Track 02).bin",
            ];
            let mut t1 = dt::saturn_track("T-5704H   ");
            t1.resize(MIB as usize, 0);
            let t2 = vec![0x55u8; 71 * MIB as usize];
            put(
                &s.path().join("Tetris Plus.zip"),
                &sevenz(
                    &[
                        ("Tetris Plus (Japan).cue", &cue(&names)),
                        (names[0], &t1),
                        (names[1], &t2),
                    ],
                    SevenZShape {
                        solid: true,
                        coder: SevenZCoder::Copy,
                        sheets_last: true,
                    },
                ),
            );
            let env = world.env();
            let o = opts("anbernic-stock", t.path(), TransferMode::Move);
            let sv = survey(s.path(), &o, &env, &|_| {}, &AtomicBool::new(false)).unwrap();
            let insp = sv.entries[0].inspection.as_ref().expect("an archive");
            assert!(insp.decoded > 0);
            assert!(insp.decoded <= RESERVE, "decoded {}", insp.decoded);
            let p = plan_from_survey(&sv, &o, &[], &env).unwrap();
            let item = by_name(&p, "Tetris Plus.zip");
            assert_eq!(item.system_id.as_deref(), Some("saturn"));
            assert!(item.notes.contains(&ItemNote::SheetNotReadYet));
            assert_eq!(entries(item).len(), 3);
        }
    }
}
