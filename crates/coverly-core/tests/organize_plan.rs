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
    plan_from_survey, plan_organize, probe_folder, survey, Choice, Decision, LeftReason, MetaProbe,
    NameMode, OrganizeAction, OrganizeEnv, OrganizeError, OrganizeItem, OrganizeOptions,
    OrganizePhase, OrganizePlan, OrganizeProgress, PlanWarning,
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
