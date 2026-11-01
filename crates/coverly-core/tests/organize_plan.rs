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
use coverly_core::dedupe::testing::pin_mtime;
use coverly_core::dedupe::Relation;
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
    // One snapshot for every OS: every path uses `/` like on Unix.
    forward_slashes(&mut value);
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

/// Rewrites `\` as `/` in every string of `value`. No plan string other
/// than a path holds a backslash, so this touches paths only.
fn forward_slashes(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::String(s) if s.contains('\\') => *s = s.replace('\\', "/"),
        serde_json::Value::Array(values) => values.iter_mut().for_each(forward_slashes),
        serde_json::Value::Object(map) => map.values_mut().for_each(forward_slashes),
        _ => {}
    }
}

#[test]
fn redacted_plans_write_every_path_with_forward_slashes() {
    // Windows serialises `PathBuf` with `\`: companions, other files and
    // set members below a folder must snapshot like on Unix.
    let mut value = serde_json::json!({
        "items": [{
            "target": "ROMs\\mame\\sfiii.zip",
            "sources": ["sfiii.zip"],
            "companions": [{ "source": "sfiii\\sfiii.chd", "target": "ROMs\\mame\\sfiii\\sfiii.chd" }],
            "set_targets": ["ROMs\\psx\\Game\\Game (Track 2).bin"],
            "reason": { "kind": "already-present", "at": "ROMs\\psx\\a.chd" }
        }],
        "other_files": ["Cave Story\\data\\Stage\\0.pxm"],
        "sorted_folders": [["GBA\\Hacks", "gba"]]
    });
    forward_slashes(&mut value);
    let text = value.to_string();
    assert!(!text.contains('\\'), "{text}");
    assert_eq!(
        value["items"][0]["companions"][0]["source"],
        "sfiii/sfiii.chd"
    );
    assert_eq!(value["other_files"][0], "Cave Story/data/Stage/0.pxm");
    assert_eq!(value["sorted_folders"][0][0], "GBA/Hacks");
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
fn equal_raw_files_are_hashed_and_one_is_set_aside() {
    let dir = TempDir::new().unwrap();
    put(&dir.path().join("Game.gba"), &fx::gba_tagged(9));
    put(&dir.path().join("Game (1).gba"), &fx::gba_tagged(9));
    put(&dir.path().join("Other.gba"), &fx::gba_tagged(10));
    // The tie-breaker keeps the oldest copy.
    pin_mtime(&dir.path().join("Game.gba"), 1);
    pin_mtime(&dir.path().join("Game (1).gba"), 2);
    let world = World::new();
    let plan = plan(
        &world,
        dir.path(),
        OrganizeOptions {
            offline: true,
            ..defaults("anbernic-stock")
        },
    );
    match &by_name(&plan, "Game (1).gba").action {
        OrganizeAction::SetAside {
            keeper,
            relation,
            dest,
            ..
        } => {
            assert_eq!(keeper, &PathBuf::from("ROMS/GBA/Game.gba"));
            assert_eq!(*relation, Relation::Exact);
            assert_eq!(dest, &PathBuf::from("_Duplicados/Game (1).gba"));
        }
        other => panic!("not set aside: {other:?}"),
    }
    assert_eq!(by_name(&plan, "Other.gba").action, OrganizeAction::Auto);
    assert_eq!(plan.counts.auto, 2);
    assert_eq!(plan.counts.set_aside, 1);
    assert_eq!(plan.counts.in_library, 0);
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
    let probe = probe_folder(dir.path(), &SystemDb::builtin(), None, &NoCloud).unwrap();
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
        .filter(|n| by_name(&plan, n).is_set_aside())
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
    let probe = probe_folder(dir.path(), &SystemDb::builtin(), None, &NoCloud).unwrap();
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
            // The wire shape the UI reads (`src/ipc/types.ts`).
            assert_eq!(
                serde_json::to_value(PlanWarning::NoSpace {
                    needed: 2000,
                    free: 1000
                })
                .unwrap(),
                serde_json::json!({ "kind": "no-space", "needed": 2000, "free": 1000 })
            );
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
        fn fat32_folder_slots_count_items_planned_before() {
            let world = World::new();
            let s = TempDir::new().unwrap();
            // 11 slots (cue 3 + two tracks 4 each) and 16 (two sheets and
            // two tracks of 30 units, 4 each); other serials, so neither is
            // the other's duplicate.
            ps1_archive(&s.path().join("Next Tetris, The.zip"), "SLUS_009.99");
            two_disc_archive(&s.path().join("Final Game.7z"));
            let _guard = Overrides::set(Some(|_| Some(1 << 50)), None);
            set_fat32_override(Some(true));

            // 64,982 slots used: either archive fits alone, not both.
            let t = TempDir::new().unwrap();
            fill_folder(&t.path().join("ROMS/PS"), 3_249);
            let p = plan(
                &world,
                s.path(),
                opts("anbernic-stock", t.path(), TransferMode::Move),
            );
            let full: Vec<&str> = ["Next Tetris, The.zip", "Final Game.7z"]
                .into_iter()
                .filter(|n| {
                    left_reason(&p, n)
                        == Some(LeftReason::FolderFull {
                            folder: "PS".into(),
                        })
                })
                .collect();
            assert_eq!(full.len(), 1, "{:#?}", p.items);
            let placed = if full[0] == "Final Game.7z" {
                "Next Tetris, The.zip"
            } else {
                "Final Game.7z"
            };
            assert_eq!(left_reason(&p, placed), None);
            assert!(!targets(by_name(&p, placed)).is_empty());
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
        fn multi_volume_later_parts_alone_or_three_never_move() {
            let world = World::new();
            let t = TempDir::new().unwrap();
            let whole = sevenz(
                &[("Split Game.gba", &fx::gba())],
                SevenZShape::non_solid_lzma(),
            );
            let third = whole.len() / 3;
            // Three parts: one item holding all three, in volume order.
            let s = TempDir::new().unwrap();
            put(&s.path().join("Split Game.7z.003"), &whole[2 * third..]);
            put(&s.path().join("Split Game.7z.001"), &whole[..third]);
            put(
                &s.path().join("Split Game.7z.002"),
                &whole[third..2 * third],
            );
            let p = plan(
                &world,
                s.path(),
                opts("anbernic-stock", t.path(), TransferMode::Move),
            );
            let split: Vec<&OrganizeItem> = p
                .items
                .iter()
                .filter(|i| i.sources.iter().any(|s| file_name(s).starts_with("Split")))
                .collect();
            assert_eq!(split.len(), 1, "{:#?}", p.items);
            let names: Vec<String> = split[0].sources.iter().map(|s| file_name(s)).collect();
            assert_eq!(
                names,
                [
                    "Split Game.7z.001",
                    "Split Game.7z.002",
                    "Split Game.7z.003"
                ]
            );
            assert!(split[0].is_left());

            // Later parts without their first: never moved, never a ROM.
            let s = TempDir::new().unwrap();
            put(
                &s.path().join("Split Game.7z.002"),
                &whole[third..2 * third],
            );
            put(&s.path().join("Split Game.7z.003"), &whole[2 * third..]);
            let p = plan(
                &world,
                s.path(),
                opts("anbernic-stock", t.path(), TransferMode::Move),
            );
            let split: Vec<&OrganizeItem> = p
                .items
                .iter()
                .filter(|i| i.sources.iter().any(|s| file_name(s).starts_with("Split")))
                .collect();
            assert_eq!(split.len(), 1, "{:#?}", p.items);
            assert_eq!(split[0].sources.len(), 2);
            for item in split {
                assert!(item.target.is_none(), "{item:#?}");
                assert!(
                    matches!(
                        item.action,
                        OrganizeAction::Left {
                            reason: LeftReason::MultiVolumeArchive { .. }
                        }
                    ),
                    "{item:#?}"
                );
            }
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

// --- Duplicates as an Organise step (duplicates spec §4, §7.1; plan Task 26) ---

mod duplicates {
    use super::*;
    use coverly_core::archive::testing::{sevenz, SevenZShape};
    use coverly_core::dedupe::{KeepReason, DEFAULT_QUARANTINE_NAME};
    use coverly_core::disc::testing as dt;
    use coverly_core::identify::sniff::fixtures::SnesMap;
    use coverly_core::profiles::Profile;
    use coverly_core::scan::{scan_library, ScanOptions};
    use coverly_core::settings::OrganizeDedupeMode;

    /// `<tmp>/pile` and `<tmp>/card` (an Anbernic card with `ROMS/`).
    fn pile_and_card() -> (TempDir, PathBuf, PathBuf) {
        let tmp = TempDir::new().unwrap();
        let pile = tmp.path().join("pile");
        let card = tmp.path().join("card");
        fs::create_dir_all(&pile).unwrap();
        fs::create_dir_all(card.join("ROMS")).unwrap();
        (tmp, pile, card)
    }

    fn into(card: &Path, mode: OrganizeDedupeMode) -> OrganizeOptions {
        OrganizeOptions {
            target: Some(card.to_path_buf()),
            offline: true,
            duplicates: mode,
            ..defaults("anbernic-stock")
        }
    }

    /// `(keeper, relation, reason, dest)` of a set-aside item.
    fn set_aside_of(item: &OrganizeItem) -> (PathBuf, Relation, KeepReason, PathBuf) {
        match &item.action {
            OrganizeAction::SetAside {
                keeper,
                relation,
                reason,
                dest,
            } => (keeper.clone(), *relation, reason.clone(), dest.clone()),
            other => panic!("{:?} is not set aside: {other:?}", item.sources),
        }
    }

    fn write7z(path: &Path, files: &[(&str, &[u8])]) {
        put(path, &sevenz(files, SevenZShape::non_solid_lzma()));
    }

    #[test]
    fn pile_item_equal_to_a_library_rom_under_another_name_is_in_library_and_set_aside() {
        let (_tmp, pile, card) = pile_and_card();
        let rom = fx::gb_tagged(0x00, 7);
        put(&card.join("ROMS/GB/Tetris (World).gb"), &rom);
        put(&pile.join("tetris copy.gb"), &rom);
        put(&pile.join("Other.gb"), &fx::gb_tagged(0x00, 8));
        let world = World::new();
        let plan = plan(&world, &pile, into(&card, OrganizeDedupeMode::SetAside));

        let item = by_name(&plan, "tetris copy.gb");
        let (keeper, relation, _, dest) = set_aside_of(item);
        assert_eq!(keeper, PathBuf::from("ROMS/GB/Tetris (World).gb"));
        assert_eq!(relation, Relation::Exact);
        assert_eq!(
            dest,
            Path::new(DEFAULT_QUARANTINE_NAME).join("tetris copy.gb")
        );
        assert_eq!(item.target, None, "never copied into the library");
        assert_eq!(by_name(&plan, "Other.gb").action, OrganizeAction::Auto);
        assert_eq!(plan.counts.set_aside, 1);
        assert_eq!(plan.counts.in_library, 1);
        assert_eq!(plan.counts.roms, 1);
        assert_eq!(plan.duplicates.len(), 1);
        let group = &plan.duplicates[0];
        assert!(group.members[group.keeper].unit.in_library);
    }

    #[test]
    fn organise_groups_carry_facts() {
        let (_tmp, pile, card) = pile_and_card();
        let rom = fx::gb_tagged(0x00, 7);
        put(&card.join("ROMS/GB/Tetris (World).gb"), &rom);
        put(&pile.join("tetris copy.gb"), &rom);
        let world = World::new();
        let plan = plan(&world, &pile, into(&card, OrganizeDedupeMode::SetAside));

        let group = &plan.duplicates[0];
        let keeper = &group.members[group.keeper];
        assert!(keeper.unit.in_library);
        assert!(keeper.facts.tags_from.is_some(), "{:?}", keeper.facts);
        assert_eq!(
            keeper.facts.regions,
            [coverly_core::identify::names::Region::World]
        );
    }

    #[test]
    fn pile_7z_equal_to_a_library_zip_is_in_library() {
        let (_tmp, pile, card) = pile_and_card();
        let rom = fx::gb_tagged(0x00, 9);
        zip(&card.join("ROMS/GB/Game (USA).zip"), "Game (USA).gb", &rom);
        write7z(&pile.join("Game.7z"), &[("Game.gb", &rom)]);
        let world = World::new();
        let plan = plan(&world, &pile, into(&card, OrganizeDedupeMode::SetAside));

        let item = by_name(&plan, "Game.7z");
        let (keeper, relation, ..) = set_aside_of(item);
        assert_eq!(keeper, PathBuf::from("ROMS/GB/Game (USA).zip"));
        assert_eq!(relation, Relation::Exact);
        assert_eq!(plan.counts.in_library, 1);
    }

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

    #[test]
    fn pile_disc_archive_equal_to_a_library_cue_set_by_serial_is_a_format_duplicate() {
        let (_tmp, pile, card) = pile_and_card();
        let t1 = "Next Tetris, The (USA) (Track 01).bin";
        let t2 = "Next Tetris, The (USA) (Track 02).bin";
        let track = dt::psx_track(true, "cdrom:\\SLUS_008.62;1");
        let ps = card.join("ROMS/PS");
        put(&ps.join("Next Tetris, The (USA).cue"), &cue(&[t1, t2]));
        put(&ps.join(t1), &track);
        put(&ps.join(t2), &[0u8; 2352 * 4]);
        // The same disc in a 7z on the pile, with another audio track
        // rip: not the same bytes, the same disc.
        let dir = "Next Tetris, The (USA)";
        write7z(
            &pile.join("Next Tetris.7z"),
            &[
                (
                    &format!("{dir}/Next Tetris, The (USA).cue"),
                    &cue(&[t1, t2]),
                ),
                (&format!("{dir}/{t1}"), &track),
                (&format!("{dir}/{t2}"), &[1u8; 2352 * 4]),
            ],
        );
        let world = World::new();
        let plan = plan(&world, &pile, into(&card, OrganizeDedupeMode::SetAside));

        let item = by_name(&plan, "Next Tetris.7z");
        let (keeper, relation, ..) = set_aside_of(item);
        assert_eq!(keeper, PathBuf::from("ROMS/PS/Next Tetris, The (USA).cue"));
        assert_eq!(relation, Relation::Format);
        assert_eq!(item.transfer, coverly_core::organize::Transfer::AsIs);
    }

    #[test]
    fn pile_pair_headered_and_headerless_is_one_group() {
        let (_tmp, pile, card) = pile_and_card();
        put(&pile.join("Mario.smc"), &fx::snes(SnesMap::Lo, true));
        put(&pile.join("Mario.sfc"), &fx::snes(SnesMap::Lo, false));
        let world = World::new();
        let plan = plan(&world, &pile, into(&card, OrganizeDedupeMode::SetAside));

        assert_eq!(plan.duplicates.len(), 1, "{:#?}", plan.items);
        // Headerless wins for a copier header.
        let (keeper, relation, reason, _) = set_aside_of(by_name(&plan, "Mario.smc"));
        assert_eq!(relation, Relation::Exact);
        assert!(matches!(reason, KeepReason::Verified { .. }), "{reason:?}");
        assert_eq!(keeper, PathBuf::from("ROMS/SFC/Mario.sfc"));
        assert_eq!(by_name(&plan, "Mario.sfc").action, OrganizeAction::Auto);
    }

    #[test]
    fn keeper_follows_rules_not_the_shortest_name() {
        let (_tmp, pile, card) = pile_and_card();
        let rom = fx::gb_tagged(0x00, 11);
        let long = pile.join("Tetris (World) (Rev 1).gb");
        let short = pile.join("Tetris.gb");
        put(&long, &rom);
        put(&short, &rom);
        // The tie-breaker keeps the oldest copy, whatever its name.
        pin_mtime(&long, 1);
        pin_mtime(&short, 2);
        let world = World::new();
        let plan = plan(&world, &pile, into(&card, OrganizeDedupeMode::SetAside));

        let (keeper, _, reason, dest) = set_aside_of(by_name(&plan, "Tetris.gb"));
        assert_eq!(reason, KeepReason::Tie);
        assert_eq!(keeper, PathBuf::from("ROMS/GB/Tetris (World) (Rev 1).gb"));
        assert_eq!(dest, Path::new(DEFAULT_QUARANTINE_NAME).join("Tetris.gb"));
        assert_eq!(
            by_name(&plan, "Tetris (World) (Rev 1).gb").action,
            OrganizeAction::Auto
        );
    }

    #[test]
    fn pile_extras_stay_on_the_pile_volume() {
        let (_tmp, pile, card) = pile_and_card();
        let rom = fx::gb_tagged(0x00, 12);
        put(&card.join("ROMS/GB/Kept.gb"), &rom);
        put(&pile.join("sub/Copy.gb"), &rom);
        let world = World::new();
        let plan = plan(&world, &pile, into(&card, OrganizeDedupeMode::SetAside));

        let item = by_name(&plan, "Copy.gb");
        assert_eq!(item.sources, vec![PathBuf::from("sub/Copy.gb")]);
        let (_, _, _, dest) = set_aside_of(item);
        // Relative to the pile, keeping the pile's own shape.
        assert_eq!(dest, Path::new(DEFAULT_QUARANTINE_NAME).join("sub/Copy.gb"));
        assert!(item.target.is_none() && item.companions.is_empty());
        assert_eq!(plan.source_root, pile);
        // Nothing to write on the card: no space needed for it.
        assert!(!plan
            .warnings
            .iter()
            .any(|w| matches!(w, PlanWarning::NoSpace { .. })));
    }

    #[test]
    fn report_mode_moves_nothing_extra() {
        let (_tmp, pile, card) = pile_and_card();
        let rom = fx::gb_tagged(0x00, 13);
        put(&card.join("ROMS/GB/Tetris (World).gb"), &rom);
        put(&pile.join("tetris copy.gb"), &rom);
        let world = World::new();
        let plan = plan(&world, &pile, into(&card, OrganizeDedupeMode::Report));

        assert_eq!(
            left_reason(&plan, "tetris copy.gb"),
            Some(LeftReason::InLibrary {
                path: PathBuf::from("ROMS/GB/Tetris (World).gb"),
                relation: Relation::Exact,
            })
        );
        assert!(!plan.items.iter().any(|i| i.is_set_aside()));
        assert_eq!(plan.counts.set_aside, 0);
        assert_eq!(plan.counts.in_library, 1);
        assert_eq!(plan.counts.roms, 0);
        assert_eq!(plan.duplicates.len(), 1);
    }

    #[test]
    fn off_mode_matches_old_behaviour_minus_shortest_name() {
        let (_tmp, pile, card) = pile_and_card();
        let rom = fx::gba_tagged(14);
        put(&card.join("ROMS/GBA/Game (USA).gba"), &rom);
        put(&pile.join("Game.gba"), &rom);
        put(&pile.join("Game (1).gba"), &rom);
        let world = World::new();
        let plan = plan(&world, &pile, into(&card, OrganizeDedupeMode::Off));

        // No step: every copy moves, nothing is a duplicate.
        assert_eq!(by_name(&plan, "Game.gba").action, OrganizeAction::Auto);
        assert_eq!(by_name(&plan, "Game (1).gba").action, OrganizeAction::Auto);
        assert!(plan.duplicates.is_empty());
        assert_eq!((plan.counts.set_aside, plan.counts.in_library), (0, 0));
        assert_eq!(plan.counts.auto, 2);
    }

    #[test]
    fn walk_and_scan_skip_quarantine() {
        let (_tmp, pile, card) = pile_and_card();
        put(&pile.join("_Duplicados/Old.gb"), &fx::gb_tagged(0x00, 15));
        put(&pile.join("_duplicates/Older.gb"), &fx::gb_tagged(0x00, 16));
        put(&pile.join("New.gb"), &fx::gb_tagged(0x00, 17));
        let world = World::new();
        let plan = plan(&world, &pile, into(&card, OrganizeDedupeMode::SetAside));
        let names: Vec<String> = plan
            .items
            .iter()
            .map(|i| file_name(&i.sources[0]))
            .collect();
        assert_eq!(names, vec!["New.gb".to_string()]);

        // A library opened as system folders keeps its quarantine inside:
        // the scan neither lists it as a system nor walks one inside GB.
        let lib = pile.join("lib");
        put(&lib.join("GB/Tetris.gb"), &fx::gb_tagged(0x00, 18));
        put(
            &lib.join("GB/_Duplicados/Tetris.gb"),
            &fx::gb_tagged(0x00, 18),
        );
        put(
            &lib.join("_Duplicados/GB/Tetris.gb"),
            &fx::gb_tagged(0x00, 18),
        );
        let open = Profile {
            roms_root: String::new(),
            ..world
                .profiles
                .iter()
                .find(|p| p.id == "anbernic-stock")
                .unwrap()
                .clone()
        };
        let scan = scan_library(&lib, &open, &world.db, &ScanOptions::default());
        let folders: Vec<&str> = scan
            .systems
            .iter()
            .map(|s| s.folder_name.as_str())
            .collect();
        assert_eq!(folders, vec!["GB"]);
        assert_eq!(scan.systems[0].roms.len(), 1);
        assert_eq!(scan.systems[0].roms[0].path, lib.join("GB/Tetris.gb"));
    }

    #[test]
    fn old_plan_json_with_duplicate_left_reason_still_loads() {
        let plan: OrganizePlan =
            serde_json::from_str(include_str!("fixtures/organize/plan-v1.json")).unwrap();
        assert_eq!(
            left_reason(&plan, "Tetris 2 (1).zip"),
            Some(LeftReason::Duplicate {
                of: PathBuf::from("Tetris 2.zip")
            })
        );
        assert!(plan.duplicates.is_empty());
        assert_eq!((plan.counts.set_aside, plan.counts.in_library), (0, 0));
        // The new fields stay out of an old-shaped plan's JSON.
        let json = serde_json::to_value(&plan).unwrap();
        assert!(json.get("duplicates").is_none());
        assert!(json["counts"].get("set_aside").is_none());
    }

    #[test]
    fn set_aside_and_in_library_round_trip() {
        let (_tmp, pile, card) = pile_and_card();
        let rom = fx::gb_tagged(0x00, 19);
        put(&card.join("ROMS/GB/Tetris (World).gb"), &rom);
        put(&pile.join("tetris copy.gb"), &rom);
        let world = World::new();
        let plan = plan(&world, &pile, into(&card, OrganizeDedupeMode::SetAside));
        let json = serde_json::to_value(&plan).unwrap();
        let item = &json["items"][0];
        assert_eq!(item["action"], "set-aside");
        assert_eq!(item["relation"], "exact");
        assert_eq!(json["counts"]["set_aside"], 1);
        let again: OrganizePlan = serde_json::from_value(json).unwrap();
        assert_eq!(again, plan);

        let reason = LeftReason::InLibrary {
            path: PathBuf::from("ROMS/GB/Tetris (World).gb"),
            relation: Relation::Format,
        };
        let v = serde_json::to_value(&reason).unwrap();
        assert_eq!(v["kind"], "in-library");
        assert_eq!(serde_json::from_value::<LeftReason>(v).unwrap(), reason);
    }
}

mod duplicates_same_file {
    use super::*;
    use coverly_core::settings::OrganizeDedupeMode;

    fn card_with_tetris() -> (TempDir, PathBuf) {
        let tmp = TempDir::new().unwrap();
        let card = tmp.path().join("card");
        put(&card.join("ROMS/GB/Tetris.gb"), &fx::gb_tagged(0x00, 40));
        (tmp, card)
    }

    fn assert_never_set_aside(plan: &OrganizePlan) {
        assert!(
            !plan.items.iter().any(|i| i.is_set_aside()),
            "the only copy is set aside: {:#?}",
            plan.items
        );
        assert!(plan.duplicates.is_empty(), "{:#?}", plan.duplicates);
    }

    #[test]
    fn a_system_folder_organised_into_its_own_card_is_not_its_own_duplicate() {
        let (_tmp, card) = card_with_tetris();
        let world = World::new();
        let opts = OrganizeOptions {
            target: Some(card.clone()),
            offline: true,
            duplicates: OrganizeDedupeMode::SetAside,
            ..defaults("anbernic-stock")
        };
        let plan = plan(&world, &card.join("ROMS/GB"), opts);
        assert_never_set_aside(&plan);
    }

    #[test]
    fn a_pile_reached_by_another_spelling_is_not_its_own_duplicate() {
        let (_tmp, card) = card_with_tetris();
        let world = World::new();
        let opts = OrganizeOptions {
            target: Some(card.clone()),
            offline: true,
            duplicates: OrganizeDedupeMode::SetAside,
            ..defaults("anbernic-stock")
        };
        // `card/ROMS/GB/../GB`: the same folder, another path.
        let plan = plan(&world, &card.join("ROMS/GB/../GB"), opts);
        assert_never_set_aside(&plan);
    }

    #[cfg(unix)]
    #[test]
    fn a_pile_reached_through_a_link_is_not_its_own_duplicate() {
        let (tmp, card) = card_with_tetris();
        let link = tmp.path().join("link");
        std::os::unix::fs::symlink(card.join("ROMS/GB"), &link).unwrap();
        let world = World::new();
        let opts = OrganizeOptions {
            target: Some(card.clone()),
            offline: true,
            duplicates: OrganizeDedupeMode::SetAside,
            ..defaults("anbernic-stock")
        };
        let plan = plan(&world, &link, opts);
        assert_never_set_aside(&plan);
    }
}

#[test]
fn the_duplicates_step_follows_the_region_order_of_the_settings() {
    let mut settings = coverly_core::settings::Settings::default();
    settings.matching.regions = vec!["Europe".into(), "USA".into()];
    let opts = OrganizeOptions::from_settings(&settings, None);
    assert_eq!(opts.regions, settings.matching.regions);
}

/// The memoised facts row of `path`, by its size and mtime.
fn stored_facts(world: &World, path: &Path) -> Option<serde_json::Value> {
    let meta = fs::metadata(path).unwrap();
    let mtime = FileTime::from_last_modification_time(&meta);
    let ns = mtime.unix_seconds() * 1_000_000_000 + i64::from(mtime.nanoseconds());
    let json = world.cache.organize_facts(path, meta.len(), ns).unwrap()?;
    Some(serde_json::from_str(&json).unwrap())
}

#[test]
fn facts_of_version_7_are_read_again() {
    // Batch 3 D28: version 7 facts carry no content id, region or Xbox
    // product id on `Sniff.console`.
    let dir = TempDir::new().unwrap();
    let rom = dir.path().join("Game.gba");
    put(&rom, &fx::gba());
    pin_mtime(&rom, 7);
    let world = World::new();
    let opts = OrganizeOptions {
        offline: true,
        ..defaults("anbernic-stock")
    };
    plan(&world, dir.path(), opts.clone());
    let mut stored = stored_facts(&world, &rom).expect("facts memoised");
    assert_eq!(stored["v"], 8);

    // A version 7 row with a stale verdict is not trusted.
    stored["v"] = 7.into();
    stored["inspection"]["payloads"][0]["sniff"]["hits"] = serde_json::json!([]);
    let meta = fs::metadata(&rom).unwrap();
    let mtime = FileTime::from_last_modification_time(&meta);
    let ns = mtime.unix_seconds() * 1_000_000_000 + i64::from(mtime.nanoseconds());
    world
        .cache
        .put_organize_facts(&rom, meta.len(), ns, &stored.to_string())
        .unwrap();
    let again = plan(&world, dir.path(), opts);
    let item = again
        .items
        .iter()
        .find(|i| i.system_id.as_deref() == Some("gba"));
    assert!(item.is_some(), "the header is read again");
    let stored = stored_facts(&world, &rom).unwrap();
    assert_eq!(stored["v"], 8);
    assert_eq!(
        stored["inspection"]["payloads"][0]["sniff"]["hits"][0]["system"],
        "gba"
    );
}

#[test]
fn an_arduboy_package_moves_as_one_raw_file() {
    // D25: an `.arduboy` is a zip holding `info.json` beside its `.hex`;
    // it moves as-is into the Arduboy folder, never extracted. A loose
    // Intel HEX beside it never moves (D4).
    use coverly_core::organize::Transfer;
    let dir = TempDir::new().unwrap();
    let pkg = dir.path().join("Arduventure.arduboy");
    common::roms::zip_file(
        &pkg,
        &[
            ("info.json", b"{\"title\":\"Arduventure\"}", true),
            ("Arduventure.hex", b":00000001FF\n", true),
        ],
    );
    filetime::set_file_mtime(&pkg, hour_ago()).unwrap();
    put(&dir.path().join("firmware.hex"), b":00000001FF\r\n");
    let world = World::new();
    let plan = plan(
        &world,
        dir.path(),
        OrganizeOptions {
            offline: true,
            ..defaults("batocera")
        },
    );
    let item = by_name(&plan, "Arduventure.arduboy");
    assert_eq!(item.system_id.as_deref(), Some("arduboy"));
    assert_eq!(item.action, OrganizeAction::Auto, "{:?}", item.why);
    assert_eq!(item.transfer, Transfer::AsIs);
    assert_eq!(item.sources.len(), 1);
    let target: Vec<String> = item
        .target
        .as_ref()
        .unwrap()
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        target.iter().rev().take(2).collect::<Vec<_>>(),
        [&"Arduventure.arduboy".to_string(), &"arduboy".to_string()]
    );
    assert!(plan
        .items
        .iter()
        .filter(|i| file_name(&i.sources[0]) == "firmware.hex")
        .all(OrganizeItem::is_left));
}

#[test]
fn a_folder_game_is_one_left_item_with_the_layout_folder() {
    // D15: `Freedoom Phase 1/` holding an IWAD is one item left in place,
    // naming the layout's Doom folder (the user's name when one is set);
    // a loose `DOOM.WAD` moves on its own.
    let dir = TempDir::new().unwrap();
    let mut wad = fx::wad(b"IWAD");
    put(&dir.path().join("Freedoom Phase 1/freedoom1.wad"), &wad);
    put(
        &dir.path().join("Freedoom Phase 1/README.txt"),
        b"Freedoom\n",
    );
    wad[100] = 7;
    put(&dir.path().join("DOOM.WAD"), &wad);
    let world = World::new();
    let plan_for = |names: BTreeMap<String, String>| {
        plan(
            &world,
            dir.path(),
            OrganizeOptions {
                offline: true,
                folder_names: names,
                ..defaults("batocera")
            },
        )
    };
    let p = plan_for(BTreeMap::new());
    assert_eq!(
        left_reason(&p, "Freedoom Phase 1"),
        Some(LeftReason::FolderGame {
            system: "doom".into(),
            folder: "prboom".into(),
        })
    );
    assert!(p
        .items
        .iter()
        .all(|i| file_name(&i.sources[0]) != "README.txt"
            && file_name(&i.sources[0]) != "freedoom1.wad"));
    let loose = by_name(&p, "DOOM.WAD").target.clone().unwrap();
    assert_eq!(loose, Path::new("roms").join("prboom").join("DOOM.WAD"));

    let p = plan_for(BTreeMap::from([("doom".to_string(), "doom".to_string())]));
    assert_eq!(
        left_reason(&p, "Freedoom Phase 1"),
        Some(LeftReason::FolderGame {
            system: "doom".into(),
            folder: "doom".into(),
        })
    );
}

/// A MAME DAT in the cache listing `set` with one ROM per `(name, bytes)`.
fn cache_mame_set(world: &World, set: &str, roms: &[(&str, &[u8])]) {
    let rows: Vec<coverly_core::cache::DatRow> = roms
        .iter()
        .map(|(name, bytes)| coverly_core::cache::DatRow {
            game: set.into(),
            rom_name: (*name).into(),
            size: Some(bytes.len() as u64),
            crc32: Some(format!("{:08X}", crc32fast::hash(bytes))),
            md5: None,
            sha1: None,
            serial: None,
        })
        .collect();
    world
        .cache
        .replace_dat("metadat/mame/MAME.dat", &rows)
        .unwrap();
}

#[test]
fn loose_model2_set_goes_to_mame() {
    // D16: MAME runs Model 2 and the libretro MAME DAT has no driver
    // field, so a loose Model 2 set confirmed by the MAME DAT goes to
    // `mame`, never to `model2`.
    let world = World::new();
    let (s, t) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let roms: [(&str, &[u8]); 2] = [("epr-16722a.12", b"daytona one"), ("mpr-16528.10", b"two")];
    cache_mame_set(&world, "daytona", &roms);
    let path = s.path().join("daytona.zip");
    common::roms::zip_file(
        &path,
        &[(roms[0].0, roms[0].1, true), (roms[1].0, roms[1].1, true)],
    );
    pin_mtime(&path, 1);
    let p = plan(
        &world,
        s.path(),
        OrganizeOptions {
            offline: true,
            target: Some(t.path().to_path_buf()),
            ..defaults("esde")
        },
    );
    let item = by_name(&p, "daytona.zip");
    assert_eq!(item.system_id.as_deref(), Some("mame"));
    assert_eq!(item.action, OrganizeAction::Auto);
    assert_eq!(item.method, Some(IdLevel::Hash));
    // Inside a `model2/` folder a set is already sorted and stays.
    put(&s.path().join("model2").join("vf2.zip"), b"PK\x05\x06");
    let p = plan(
        &world,
        s.path(),
        OrganizeOptions {
            offline: true,
            target: Some(t.path().to_path_buf()),
            ..defaults("esde")
        },
    );
    assert!(p
        .items
        .iter()
        .all(|i| file_name(&i.sources[0]) != "vf2.zip"));
}

#[test]
fn mame_chd_folder_moves_with_its_set() {
    // D16: a `<set>/` folder holding only `.chd` beside `<set>.zip` is a
    // companion of the set: one item, and both land in the arcade folder.
    use coverly_core::organize::{apply_organize, ApplyEnv};
    use coverly_core::write::trash::FakeTrash;
    let world = World::new();
    let (s, t, data) = (
        TempDir::new().unwrap(),
        TempDir::new().unwrap(),
        TempDir::new().unwrap(),
    );
    let roms: [(&str, &[u8]); 2] = [("sfiii-simm1.0", b"simm one"), ("sfiii-simm1.1", b"simm 2")];
    cache_mame_set(&world, "sfiii", &roms);
    let zip_path = s.path().join("sfiii.zip");
    common::roms::zip_file(
        &zip_path,
        &[(roms[0].0, roms[0].1, true), (roms[1].0, roms[1].1, true)],
    );
    pin_mtime(&zip_path, 1);
    let chd = s.path().join("sfiii").join("sfiii.chd");
    put(&chd, b"MComprHD synthetic");
    pin_mtime(&chd, 2);

    let p = plan(
        &world,
        s.path(),
        OrganizeOptions {
            offline: true,
            target: Some(t.path().to_path_buf()),
            ..defaults("batocera")
        },
    );
    assert_eq!(p.items.len(), 1, "{:#?}", p.items);
    let item = by_name(&p, "sfiii.zip");
    assert_eq!(
        (item.system_id.as_deref(), &item.action),
        (Some("mame"), &OrganizeAction::Auto)
    );
    let target = item.target.clone().unwrap();
    let folder = target.parent().unwrap().to_path_buf();
    assert_eq!(folder.file_name().unwrap(), "mame");
    assert_eq!(item.companions.len(), 1);
    let comp = &item.companions[0];
    assert_eq!(comp.source, Path::new("sfiii").join("sfiii.chd"));
    assert_eq!(comp.target, folder.join("sfiii").join("sfiii.chd"));

    let env = ApplyEnv {
        data_dir: data.path(),
        progress: &|_| {},
        cancel: &AtomicBool::new(false),
        trash: &FakeTrash::unavailable("no-trash"),
        tools: Some(&[]),
    };
    let report = apply_organize(&p, &env).unwrap();
    assert_eq!((report.moved, report.failed), (1, 0), "{report:?}");
    let arcade = t.path().join(&folder);
    assert_eq!(
        fs::read(arcade.join("sfiii").join("sfiii.chd")).unwrap(),
        b"MComprHD synthetic"
    );
    assert!(arcade.join("sfiii.zip").is_file());
    assert!(!chd.exists() && !zip_path.exists());
}

#[test]
fn chd_folder_beside_a_set_that_stays_is_left() {
    // Without an arcade set to follow, the CHDs are left where they are.
    let world = World::new();
    let (s, t) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    common::roms::zip_file(&s.path().join("mystery.zip"), &[("a.bin", b"aa", true)]);
    pin_mtime(&s.path().join("mystery.zip"), 1);
    let chd = s.path().join("mystery").join("disc.chd");
    put(&chd, b"MComprHD synthetic");
    pin_mtime(&chd, 2);
    let p = plan(
        &world,
        s.path(),
        OrganizeOptions {
            offline: true,
            target: Some(t.path().to_path_buf()),
            ..defaults("batocera")
        },
    );
    assert_eq!(left_reason(&p, "disc.chd"), Some(LeftReason::Unknown));
    assert!(by_name(&p, "mystery.zip").companions.is_empty());
}

#[test]
fn chd_folder_of_a_set_already_in_the_library_is_already_present() {
    // A second run (or a copy run) finds `<arcade>/sfiii.zip` and its
    // `sfiii/sfiii.chd` already there: both are reported already present,
    // the CHD never as unknown.
    let world = World::new();
    let (s, t) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let roms: [(&str, &[u8]); 2] = [("sfiii-simm1.0", b"simm one"), ("sfiii-simm1.1", b"simm 2")];
    cache_mame_set(&world, "sfiii", &roms);
    let zip_path = s.path().join("sfiii.zip");
    common::roms::zip_file(
        &zip_path,
        &[(roms[0].0, roms[0].1, true), (roms[1].0, roms[1].1, true)],
    );
    pin_mtime(&zip_path, 1);
    let chd = s.path().join("sfiii").join("sfiii.chd");
    put(&chd, b"MComprHD synthetic");
    pin_mtime(&chd, 2);
    let opts = || OrganizeOptions {
        offline: true,
        target: Some(t.path().to_path_buf()),
        ..defaults("batocera")
    };
    let first = plan(&world, s.path(), opts());
    let item = by_name(&first, "sfiii.zip");
    let zip_at = item.target.clone().unwrap();
    let chd_at = item.companions[0].target.clone();
    fs::create_dir_all(t.path().join(&chd_at).parent().unwrap()).unwrap();
    fs::copy(&zip_path, t.path().join(&zip_at)).unwrap();
    fs::copy(&chd, t.path().join(&chd_at)).unwrap();

    let again = plan(&world, s.path(), opts());
    assert_eq!(
        left_reason(&again, "sfiii.zip"),
        Some(LeftReason::AlreadyPresent { at: zip_at })
    );
    assert_eq!(
        left_reason(&again, "sfiii.chd"),
        Some(LeftReason::AlreadyPresent { at: chd_at })
    );
}

/// Every file below `dir`, depth first.
fn files_below(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            out.extend(files_below(&path));
        } else {
            out.push(path);
        }
    }
    out
}

/// The retro pile of spec §9.3 (synthetic bytes, real names), every
/// mtime pinned in the past.
fn retro_pile(world: &World, dir: &Path) {
    use coverly_core::disc::testing as dt;
    put(
        &dir.join("Gran Turismo 4 (Europe).iso"),
        &dt::ps2_iso_booting("SCES_517.19"),
    );
    put(&dir.join("Tempest 2000 (World).j64"), &fx::zeros(4096));
    put(&dir.join("Manic Miner (1983)(Bug-Byte).tzx"), &fx::tzx());
    put(&dir.join("Arkanoid (1987).dsk"), &fx::cpc_dsk(false));
    put(&dir.join("Sailor Moon (Japan).st"), &fx::sufami(false));
    put(&dir.join("Prince of Persia.st"), &fx::zeros(737_280));
    put(&dir.join("DOOM.WAD"), &fx::wad(b"IWAD"));
    put(
        &dir.join("Freedoom Phase 1").join("freedoom1.wad"),
        &fx::wad(b"IWAD"),
    );
    put(
        &dir.join("Freedoom Phase 1").join("README.txt"),
        b"Freedoom\n",
    );
    put(&dir.join("Cave Story").join("Doukutsu.exe"), b"MZ\x90\x00");
    put(
        &dir.join("Cave Story")
            .join("data")
            .join("Stage")
            .join("0.pxm"),
        &[1u8; 64],
    );
    let roms: [(&str, &[u8]); 2] = [("sfiii-simm1.0", b"simm one"), ("sfiii-simm1.1", b"simm 2")];
    cache_mame_set(world, "sfiii", &roms);
    common::roms::zip_file(
        &dir.join("sfiii.zip"),
        &[(roms[0].0, roms[0].1, true), (roms[1].0, roms[1].1, true)],
    );
    put(&dir.join("sfiii").join("sfiii.chd"), b"MComprHD synthetic");
    put(&dir.join("game.bin"), &[3u8; 512]);
    for path in files_below(dir) {
        pin_mtime(&path, 1);
    }
}

#[test]
fn systems_batch_1_pile_plans() {
    // Spec §9.3 / §14.1: the retro pile of systems batch 1 under the three
    // layouts whose folder stories differ most (Named, Named with scoped
    // aliases, Absent with the `NoFolderOnDevice` note).
    for layout in ["esde", "batocera", "onion"] {
        let world = World::new();
        let (s, t) = (TempDir::new().unwrap(), TempDir::new().unwrap());
        retro_pile(&world, s.path());
        let p = plan(
            &world,
            s.path(),
            OrganizeOptions {
                offline: true,
                target: Some(t.path().to_path_buf()),
                ..defaults(layout)
            },
        );
        insta::assert_json_snapshot!(format!("systems_batch_1__{layout}"), redact(&p));
    }
    // A loose image carries no `DiscId` (only archived disc sets do, as for
    // PS1): the PS2 serial of the pile is the sniff's.
    let bytes = coverly_core::disc::testing::ps2_iso_booting("SCES_517.19");
    let s = coverly_core::identify::sniff::sniff(
        &mut coverly_core::identify::sniff::SliceAt(&bytes),
        bytes.len() as u64,
        "iso",
    );
    assert_eq!(s.serial.as_deref(), Some("SCES-51719"));
}

#[test]
fn an_encrypted_cia_moves_with_the_azahar_note() {
    use coverly_core::organize::ItemNote;
    // Systems batch 2 D4: an encrypted 3DS package is identified by its
    // TMD title id and moved normally, with a note about the emulator.
    let world = World::new();
    let (s, t) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let cia = coverly_core::consoles::testing::cia(0x0004_0000_0005_5D00, true, true);
    put(&s.path().join("Pokemon Y (Europe).cia"), &cia.bytes());
    // Homebrew is never encrypted: no note.
    let dsx = coverly_core::consoles::testing::dsx_with_smdh("Homebrew Tool");
    put(&s.path().join("Homebrew Tool.3dsx"), &dsx.bytes());
    for path in files_below(s.path()) {
        pin_mtime(&path, 1);
    }
    let p = plan(
        &world,
        s.path(),
        OrganizeOptions {
            offline: true,
            target: Some(t.path().to_path_buf()),
            ..defaults("esde")
        },
    );
    let enc = by_name(&p, "Pokemon Y (Europe).cia");
    assert_eq!(enc.system_id.as_deref(), Some("3ds"));
    assert!(enc.moves_in(), "{:?}", enc.action);
    assert!(enc.notes.contains(&ItemNote::EncryptedContent {
        emulator: "Azahar".into()
    }));
    assert_eq!(enc.folder.as_deref(), Some("n3ds"));
    let clear = by_name(&p, "Homebrew Tool.3dsx");
    assert_eq!(clear.system_id.as_deref(), Some("3ds"));
    assert!(!clear
        .notes
        .iter()
        .any(|n| matches!(n, ItemNote::EncryptedContent { .. })));
}

#[test]
fn switch_update_is_listed_under_its_base() {
    use coverly_core::organize::ItemNote;
    // Systems batch 2 D11: an update follows its base in the plan with a
    // note naming it; DLC whose base is not in the pile names the base id.
    let world = World::new();
    let (s, t) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    // Different bytes per file: equal files would be duplicates.
    let nsp = |fill: u8| {
        coverly_core::consoles::testing::nsp(&[(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.nca",
            &[fill; 64],
        )])
        .bytes()
    };
    let base = "Super Mario Odyssey [0100000000010000][v0].nsp";
    // Sorts before its base by path: only the grouping puts it after.
    let update = "Odyssey Patch [0100000000010800][v393216].nsp";
    let dlc = "Other Game DLC [0100000000021001][v0].nsp";
    let loose = "Another Game [0100000000030000][v0].nsp";
    for (fill, name) in [base, update, dlc, loose].into_iter().enumerate() {
        put(&s.path().join(name), &nsp(fill as u8 + 1));
    }
    let p = plan(
        &world,
        s.path(),
        OrganizeOptions {
            offline: true,
            target: Some(t.path().to_path_buf()),
            ..defaults("esde")
        },
    );
    let names: Vec<String> = p.items.iter().map(|i| file_name(&i.sources[0])).collect();
    let at = |n: &str| names.iter().position(|x| x == n).unwrap();
    assert_eq!(at(update), at(base) + 1, "{names:?}");
    for (i, item) in p.items.iter().enumerate() {
        assert_eq!(item.id, i);
    }
    for name in [base, update, dlc, loose] {
        let item = by_name(&p, name);
        assert_eq!(item.system_id.as_deref(), Some("switch"), "{name}");
        assert!(item.moves_in(), "{name}: {:?}", item.action);
        assert_eq!(item.folder.as_deref(), Some("switch"), "{name}");
    }
    assert!(by_name(&p, update).notes.contains(&ItemNote::Update {
        of: "Super Mario Odyssey".into()
    }));
    assert!(by_name(&p, dlc).notes.contains(&ItemNote::Dlc {
        of: "0100000000020000".into()
    }));
    assert!(!by_name(&p, base)
        .notes
        .iter()
        .any(|n| matches!(n, ItemNote::Update { .. } | ItemNote::Dlc { .. })));
    assert_eq!(p.updates, 2);
    // The summary count is in the JSON only when there is one.
    let json = serde_json::to_value(&p).unwrap();
    assert_eq!(json["updates"], 2);
}

#[test]
fn ps3_and_vita_pkgs_move_with_the_install_first_note() {
    use coverly_core::organize::{ItemNote, Transfer};
    // Systems batch 2 §8.4: a PKG is installed by the emulator, so it moves
    // to the system folder as it is with a note naming the emulator; a VPK
    // is a Vita package moved as it is, with no such note.
    let world = World::new();
    let (s, t) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let ps3 = coverly_core::consoles::testing::pkg(1, "UP0001-NPUB30024_00-0000000000000001");
    put(&s.path().join("NPUB30024.pkg"), &ps3.bytes());
    let vita = coverly_core::consoles::testing::pkg(2, "EP0001-PCSB00245_00-0000000000000000");
    put(&s.path().join("Vita Game.pkg"), &vita.bytes());
    let vpk = s.path().join("Persona 4 Golden.vpk");
    coverly_core::consoles::testing::vpk(&vpk, "PCSE00120");
    filetime::set_file_mtime(&vpk, hour_ago()).unwrap();
    let p = plan(
        &world,
        s.path(),
        OrganizeOptions {
            offline: true,
            target: Some(t.path().to_path_buf()),
            ..defaults("esde")
        },
    );
    let item = by_name(&p, "NPUB30024.pkg");
    assert_eq!(item.system_id.as_deref(), Some("ps3"));
    assert!(item.moves_in(), "{:?}", item.action);
    assert!(item.notes.contains(&ItemNote::InstallFirst {
        emulator: "RPCS3".into()
    }));
    assert_eq!(item.folder.as_deref(), Some("ps3"));
    let item = by_name(&p, "Vita Game.pkg");
    assert_eq!(item.system_id.as_deref(), Some("psvita"));
    assert!(item.notes.contains(&ItemNote::InstallFirst {
        emulator: "Vita3K".into()
    }));
    let item = by_name(&p, "Persona 4 Golden.vpk");
    assert_eq!(item.system_id.as_deref(), Some("psvita"));
    assert!(item.moves_in(), "{:?}", item.action);
    assert_eq!(item.transfer, Transfer::AsIs);
    assert!(!item
        .notes
        .iter()
        .any(|n| matches!(n, ItemNote::InstallFirst { .. })));
}

#[test]
fn a_macos_installer_pkg_is_left_as_not_a_rom() {
    // Systems batch 2 D7: a loose `.pkg` is a candidate so its header can
    // name it; a `xar!` installer is left in place, never «unknown».
    let world = World::new();
    let (s, t) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let mut bytes = b"xar!\0\x1c\0\x01".to_vec();
    bytes.resize(4096, 0);
    put(&s.path().join("Install Thing.pkg"), &bytes);
    let p = plan(
        &world,
        s.path(),
        OrganizeOptions {
            offline: true,
            target: Some(t.path().to_path_buf()),
            ..defaults("esde")
        },
    );
    assert_eq!(
        left_reason(&p, "Install Thing.pkg"),
        Some(LeftReason::NotARom {
            what: coverly_core::identify::sniff::NotARom::Installer
        })
    );
}

/// The modern pile of systems batch 2 (spec §9.3): synthetic heads, real
/// names, every mtime pinned in the past. Disc and card images are their
/// heads only (the file ends after the last header), so a far offset
/// reads as end of file; the Redump XGD1 offset is covered by the
/// `SparseAt` tests of `systems_batch2.rs`, here the Xbox image is an
/// XISO at offset 0.
fn modern_pile(dir: &Path) {
    use coverly_core::consoles::testing as ct;
    let disc = |id6: &str, wii: bool, number: u8| {
        let mut h = ct::gc_header(id6, wii);
        h[6] = number;
        h
    };
    put(
        &dir.join("Metroid Prime (USA).rvz"),
        &ct::wia(&disc("GM8E01", false, 0), true, 1).head_bytes(),
    );
    put(
        &dir.join("Resident Evil 4 (USA) (Disc 1).rvz"),
        &ct::wia(&disc("G4BE08", false, 0), true, 1).head_bytes(),
    );
    put(
        &dir.join("Resident Evil 4 (USA) (Disc 2).rvz"),
        &ct::wia(&disc("G4BE08", false, 1), true, 1).head_bytes(),
    );
    put(
        &dir.join("Super Mario Galaxy (Europe).wbfs"),
        &ct::wbfs(&disc("RMGP01", true, 0)).head_bytes(),
    );
    put(&dir.join("Super Mario Galaxy (Europe).wbf1"), &[0u8; 1024]);
    put(
        &dir.join("Excitebike - World Rally (Europe) (WiiWare).wad"),
        &ct::wii_wad(0x0001_0001_5741_4150).head_bytes(),
    );
    ct::wiiu_folder(
        &dir.join("Super Mario 3D World [0005000010145D00]"),
        "0005000010145D00",
        "WUP-P-ARDP",
        "Super Mario 3D World",
    );
    put(
        &dir.join("Mario Kart 7 (Europe).3ds"),
        &ct::ncsd("CTR-P-AMKP", 0x0004_0000_0003_0800, true).head_bytes(),
    );
    put(
        &dir.join("Pokemon Y (Europe).cia"),
        &ct::cia(0x0004_0000_0005_5E00, true, false).head_bytes(),
    );
    put(&dir.join("model.3ds"), &[0x4D, 0x4D, 0x10, 0, 0, 0, 2, 0]);
    put(
        &dir.join("Super Mario Odyssey [0100000000010000][v0].nsp"),
        &ct::nsp(&[("01000000000100000000000000000000.tik", &[0; 64])]).bytes(),
    );
    put(
        &dir.join("Super Mario Odyssey [0100000000010800][v393216].nsp"),
        &ct::nsp(&[]).bytes(),
    );
    put(
        &dir.join("Zelda BotW.xci"),
        &ct::xci(&["secure"]).head_bytes(),
    );
    ct::vita_app(&dir.join("PCSE00844"), "PCSE00844");
    ct::vpk(&dir.join("Persona 4 Golden.vpk"), "PCSB00245");
    ct::ps3_jb(&dir.join("Demon's Souls (BLUS-30443)"), "BLUS30443", "DG");
    put(
        &dir.join("NPUB30024.pkg"),
        &ct::pkg(1, "UP0001-NPUB30024_00-0000000000000000").head_bytes(),
    );
    put(
        &dir.join("Halo (USA).xiso"),
        &ct::xiso_at(0, &[("default.xbe", ct::xbe(0x4D53_0004, "Halo"))]).bytes(),
    );
    put(&dir.join("Halo 3 (USA).xex"), &ct::xex2(0x4D53_07E6));
    ct::god_folder(dir, 0x5841_0889, 0x000D_0000, b"LIVE");
    for k in ["prod.keys", "title.keys", "NPUB30024.rap"] {
        put(&dir.join(k), &[9u8; 128]);
    }
    put(
        &dir.join("ux0")
            .join("app")
            .join("PCSB00245")
            .join("eboot.bin"),
        &[1u8; 64],
    );
    put(&dir.join("Foo.pkg"), b"xar!\0\x1c\0\x01");
    for path in files_below(dir) {
        pin_mtime(&path, 1);
    }
}

#[test]
fn systems_batch_2_pile_plans() {
    use coverly_core::organize::{ItemKind, ItemNote};
    // Spec §9.3 / §14: the modern pile of systems batch 2 under the three
    // layouts whose folder stories differ most (ES-DE and Batocera name
    // every folder and rename folder games to `.ps3`, muOS lacks some).
    for layout in ["esde", "batocera", "muos"] {
        let world = World::new();
        let (s, t) = (TempDir::new().unwrap(), TempDir::new().unwrap());
        modern_pile(s.path());
        let p = plan(
            &world,
            s.path(),
            OrganizeOptions {
                offline: true,
                target: Some(t.path().to_path_buf()),
                ..defaults(layout)
            },
        );
        // Asserts that hold whatever the snapshot says.
        for key in ["prod.keys", "title.keys", "NPUB30024.rap"] {
            assert_eq!(
                left_reason(&p, key),
                Some(LeftReason::KeyMaterial),
                "{layout} {key}"
            );
        }
        assert_eq!(
            left_reason(&p, "ux0"),
            Some(LeftReason::EmulatorData {
                folder: "ux0".into()
            }),
            "{layout}"
        );
        for name in ["model.3ds", "Foo.pkg"] {
            assert!(
                matches!(left_reason(&p, name), Some(LeftReason::NotARom { .. })),
                "{layout} {name}: {:?}",
                by_name(&p, name).action
            );
        }
        // §9.3: the split WBFS part is part of its image's item.
        let galaxy = by_name(&p, "Super Mario Galaxy (Europe).wbfs");
        assert_eq!(
            galaxy
                .companions
                .iter()
                .map(|c| file_name(&c.source))
                .collect::<Vec<_>>(),
            ["Super Mario Galaxy (Europe).wbf1"],
            "{layout}"
        );
        assert!(p.other_files.is_empty(), "{layout}: {:?}", p.other_files);
        let souls = by_name(&p, "Demon's Souls (BLUS-30443)");
        assert_eq!(souls.kind, ItemKind::Folder, "{layout}");
        assert_eq!(souls.sources.len(), 1, "{layout}");
        assert_eq!(souls.system_id.as_deref(), Some("ps3"), "{layout}");
        if layout != "muos" {
            assert!(souls.moves_in(), "{layout}: {:?}", souls.action);
            let target = souls.target.as_ref().unwrap();
            assert_eq!(
                target.file_name().unwrap(),
                "Demon's Souls (BLUS-30443).ps3",
                "{layout}"
            );
        }
        let names: Vec<String> = p.items.iter().map(|i| file_name(&i.sources[0])).collect();
        let at = |n: &str| names.iter().position(|x| x == n).unwrap();
        let base = "Super Mario Odyssey [0100000000010000][v0].nsp";
        let update = "Super Mario Odyssey [0100000000010800][v393216].nsp";
        assert_eq!(at(update), at(base) + 1, "{layout}: {names:?}");
        assert!(
            by_name(&p, update)
                .notes
                .iter()
                .any(|n| matches!(n, ItemNote::Update { .. })),
            "{layout}: {:?}",
            by_name(&p, update).notes
        );
        insta::assert_json_snapshot!(format!("systems_batch_2__{layout}"), redact(&p));
    }
}

#[test]
fn a_pile_folder_game_and_its_file_twin_are_reported_and_both_move() {
    use coverly_core::consoles::testing as ct;
    use coverly_core::dedupe::{GroupAction, ReportWhy, UnitKind};
    use coverly_core::organize::ItemKind;
    // D28: a folder game is a duplicates unit in the pile too. A Wii U
    // loadiine folder and a WUA of the same title id, kind and version
    // are one reported group; neither is set aside.
    let world = World::new();
    let (s, t) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let folder = "Super Mario 3D World [0005000010145D00]";
    ct::wiiu_folder(
        &s.path().join(folder),
        "0005000010145D00",
        "WUP-P-ARDP",
        "Super Mario 3D World",
    );
    put(
        &s.path().join("Super Mario 3D World.wua"),
        &ct::wua().bytes(),
    );
    for path in files_below(s.path()) {
        pin_mtime(&path, 1);
    }
    let p = plan(
        &world,
        s.path(),
        OrganizeOptions {
            offline: true,
            target: Some(t.path().to_path_buf()),
            ..defaults("esde")
        },
    );
    let dir = by_name(&p, folder);
    assert_eq!(dir.kind, ItemKind::Folder);
    assert!(dir.moves_in(), "{:?}", dir.action);
    let wua = by_name(&p, "Super Mario 3D World.wua");
    assert!(wua.moves_in(), "{:?}", wua.action);
    let group = p
        .duplicates
        .iter()
        .find(|g| g.members.iter().any(|m| m.unit.kind == UnitKind::Folder))
        .expect("a group with the folder game");
    assert_eq!(group.members.len(), 2);
    assert_eq!(
        group.action,
        GroupAction::ReportOnly {
            why: ReportWhy::FolderGame
        }
    );
}

/// Faked folder sizes of the current-generation pile (systems batch 3
/// §9.3): a file named `eboot.bin` (or `Game0.exe`) counts the size of
/// its folder's game, never written; the metadata length otherwise. A
/// loose file's size is its metadata length (the hook covers folder
/// trees only), so the PS4 PKG is its synthetic head.
struct CurrentGenSizes;

impl MetaProbe for CurrentGenSizes {
    fn cloud_only(&self, _: &Path, _: &fs::Metadata) -> bool {
        false
    }

    fn file_len(&self, path: &Path, meta: &fs::Metadata) -> u64 {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let folder = path
            .parent()
            .and_then(Path::file_name)
            .and_then(|n| n.to_str())
            .unwrap_or("");
        match (folder, name) {
            ("CUSA00900", "eboot.bin") => 48_120_000_000,
            ("CUSA00900-UPDATE", "eboot.bin") => 14_800_000_000,
            ("Forza Horizon 5", "Game0.exe") => 110_400_000_000,
            _ => meta.len(),
        }
    }
}

/// The current-generation pile of systems batch 3 (spec §9.3): synthetic
/// heads, manifests and `param` files built by the batch 3 builders, real
/// names, every mtime pinned in the past.
fn current_pile(dir: &Path) {
    use coverly_core::consoles::testing as ct;
    const BLOODBORNE: &str = "UP9000-CUSA00900_00-BLOODBORNE000000";
    let sfo = |title_id: &str, category: &str, content_id: &str, title: &str| {
        ct::param_sfo_with(
            &[
                ("APP_VER", "01.09"),
                ("CATEGORY", category),
                ("CONTENT_ID", content_id),
                ("TITLE", title),
                ("TITLE_ID", title_id),
                ("VERSION", "01.00"),
            ],
            &[("APP_TYPE", 1), ("PUBTOOLVER", 0x0289_0000)],
        )
    };
    // PS4: the game folder, its update and mods siblings, a DLC label folder.
    let base = dir.join("CUSA00900");
    ct::ps4_app(&base, "CUSA00900", "gd", BLOODBORNE);
    ct::put_file(
        &base,
        "sce_sys/param.sfo",
        &sfo("CUSA00900", "gd", BLOODBORNE, "Bloodborne"),
    );
    ct::ps4_app(&dir.join("CUSA00900-UPDATE"), "CUSA00900", "gp", BLOODBORNE);
    ct::put_file(&dir.join("CUSA00900-mods"), "x.bin", &[1u8; 64]);
    let dlc = dir.join("CUSA00900-DLC-ABC");
    ct::put_file(
        &dlc,
        "sce_sys/param.sfo",
        &sfo(
            "CUSA00900",
            "ac",
            "UP9000-CUSA00900_00-OLDHUNTERS000000",
            "The Old Hunters",
        ),
    );
    ct::put_file(&dlc, "content.dat", &[7u8; 64]);
    // PS4 files: a store package and a ZArchive.
    let gravity = "EP0002-CUSA04447_00-GRAVITYRUSH00000";
    let gravity_sfo = ct::ps4_sfo("CUSA04447", "gd", gravity, 1);
    ct::put_file(
        dir,
        &format!("{gravity}.pkg"),
        &ct::cnt_pkg(gravity, 0x1A, 0, &[(0x1000, 0, gravity_sfo.clone())]).bytes(),
    );
    ct::put_file(
        dir,
        "Gravity Rush.zar",
        &ct::zarchive(&[
            ("eboot.bin", &[0u8; 64]),
            ("sce_sys/param.sfo", &gravity_sfo),
        ])
        .bytes(),
    );
    // PS5: a Pro-enhanced game folder, its DLC, a store package, an exFAT image.
    ct::ps5_app(
        &dir.join("PPSA01284"),
        &ct::ps5_param_json("PPSA01284", "Returnal", true),
        true,
    );
    ct::ps5_app(
        &dir.join("PPSA01284 DLC"),
        &ct::ps5_param_json("PPSA01284", "Returnal Soundtrack", false),
        false,
    );
    let ratchet = "UP9000-PPSA01284_00-RATCHET000000000";
    ct::put_file(
        dir,
        &format!("{ratchet}.pkg"),
        &ct::fih_pkg(ratchet, 0x1A).bytes(),
    );
    ct::put_file(dir, "PPSA09519.exfat", &ct::exfat_image().bytes());
    // Xbox: Scarlett, Xbox One (GDK and ERA), a PC build, a GUID-named XVC.
    ct::gdk_folder(
        &dir.join("Forza Horizon 5"),
        &ct::GdkSpec {
            families: &["Scarlett"],
            display_name: "Forza Horizon 5",
            title_id: "2B3C4D5E",
            store_id: "9NKX70BBCDRN",
            ..Default::default()
        },
    );
    ct::gdk_folder(
        &dir.join("Halo 5 Guardians"),
        &ct::GdkSpec {
            families: &["XboxOne"],
            display_name: "Halo 5: Guardians",
            title_id: "3C4D5E6F",
            store_id: "BRRC2BP0G9P0",
            ..Default::default()
        },
    );
    ct::era_folder(
        &dir.join("Sunset Overdrive"),
        &ct::era_manifest(
            "Microsoft.SunsetOverdrive",
            "Sunset Overdrive",
            "Sunset Overdrive",
            true,
        ),
    );
    ct::gdk_folder(
        &dir.join("Microsoft Flight Simulator"),
        &ct::GdkSpec {
            families: &["PC"],
            display_name: "Microsoft Flight Simulator",
            title_id: "4D5E6F70",
            store_id: "9NXVC0482QS5",
            ..Default::default()
        },
    );
    ct::put_file(
        dir,
        "4f0b3e0a-1a2b-4c3d-8e9f-c1d2e3f4a5b6",
        &ct::xvd_head_with(
            "4f0b3e0a-1a2b-4c3d-8e9f-c1d2e3f4a5b6",
            ct::XVD_PRODUCT_ID,
            0x01,
            0,
            [4, 3, 2, 1],
            0x1_0000,
        )
        .bytes(),
    );
    // Emulator data and key material, left in place.
    let shad = dir.join("shadPS4");
    ct::put_file(&shad, "sys_modules/libSceNgs2.sprx", &[1u8; 64]);
    ct::put_file(&shad, "game_data/CUSA00900/save.dat", &[2u8; 64]);
    ct::put_file(&shad, "addcont/CUSA00900/DLC1/content.dat", &[3u8; 64]);
    ct::put_file(dir, "UP9000-CUSA00900_00.rif", &[9u8; 128]);
    ct::put_file(dir, "x.cik", &[9u8; 32]);
    for path in files_below(dir) {
        pin_mtime(&path, 1);
    }
}

#[test]
fn systems_batch_3_pile_plans() {
    use coverly_core::organize::apply::set_free_space_override;
    use coverly_core::organize::{ItemKind, ItemNote, TransferMode};
    // Spec §9.3 / §14: the current-generation pile of systems batch 3 under
    // the two layouts whose folder stories differ most (ES-DE plain names
    // and no PS5 folder; Batocera `.ps4` names and its `DLC/` path), in
    // copy mode so the plan carries the large-transfer warning.
    struct Free;
    impl Drop for Free {
        fn drop(&mut self) {
            set_free_space_override(None);
        }
    }
    let _free = Free;
    set_free_space_override(Some(|_| Some(412_100_000_000)));
    for layout in ["esde", "batocera"] {
        let world = World::new();
        let (s, t) = (TempDir::new().unwrap(), TempDir::new().unwrap());
        current_pile(s.path());
        let env = OrganizeEnv {
            probe: &CurrentGenSizes,
            ..world.env()
        };
        let p = plan_with(
            &env,
            s.path(),
            &OrganizeOptions {
                offline: true,
                target: Some(t.path().to_path_buf()),
                mode: Some(TransferMode::Copy),
                ..defaults(layout)
            },
        );
        // Asserts that hold whatever the snapshot says.
        for key in ["UP9000-CUSA00900_00.rif", "x.cik"] {
            assert_eq!(
                left_reason(&p, key),
                Some(LeftReason::KeyMaterial),
                "{layout} {key}"
            );
        }
        assert!(
            matches!(
                left_reason(&p, "shadPS4"),
                Some(LeftReason::EmulatorData { .. })
            ),
            "{layout}: {:?}",
            by_name(&p, "shadPS4").action
        );
        assert_eq!(
            left_reason(&p, "Microsoft Flight Simulator"),
            Some(LeftReason::NotARom {
                what: NotARom::PcGameData
            }),
            "{layout}"
        );
        assert!(
            matches!(
                left_reason(&p, "CUSA00900-mods"),
                Some(LeftReason::GameOverlay { .. })
            ),
            "{layout}: {:?}",
            by_name(&p, "CUSA00900-mods").action
        );
        let forza = by_name(&p, "Forza Horizon 5");
        assert_eq!(forza.kind, ItemKind::Folder, "{layout}");
        assert_eq!(forza.system_id.as_deref(), Some("xboxseries"), "{layout}");
        for name in ["Halo 5 Guardians", "Sunset Overdrive"] {
            assert_eq!(
                by_name(&p, name).system_id.as_deref(),
                Some("xboxone"),
                "{layout} {name}"
            );
        }
        let guid = by_name(&p, "4f0b3e0a-1a2b-4c3d-8e9f-c1d2e3f4a5b6");
        assert_eq!(guid.action, OrganizeAction::Review, "{layout}");
        assert!(guid.notes.contains(&ItemNote::NoEmulator), "{layout}");
        assert!(
            by_name(&p, "EP0002-CUSA04447_00-GRAVITYRUSH00000.pkg")
                .notes
                .contains(&ItemNote::ExtractFirst),
            "{layout}"
        );
        for name in [
            "UP9000-PPSA01284_00-RATCHET000000000.pkg",
            "PPSA09519.exfat",
        ] {
            assert!(
                by_name(&p, name).notes.contains(&ItemNote::NoEmulator),
                "{layout} {name}: {:?}",
                by_name(&p, name).notes
            );
        }
        assert!(
            by_name(&p, "PPSA01284")
                .notes
                .contains(&ItemNote::ProEnhanced),
            "{layout}"
        );
        // Updates and DLC right after their base.
        let names: Vec<String> = p.items.iter().map(|i| file_name(&i.sources[0])).collect();
        let at = |n: &str| names.iter().position(|x| x == n).unwrap();
        assert_eq!(
            at("CUSA00900-UPDATE"),
            at("CUSA00900") + 1,
            "{layout}: {names:?}"
        );
        assert_eq!(
            at("PPSA01284 DLC"),
            at("PPSA01284") + 1,
            "{layout}: {names:?}"
        );
        assert!(
            p.warnings
                .iter()
                .any(|w| matches!(w, PlanWarning::LargeTransfer { .. })),
            "{layout}: {:?}",
            p.warnings
        );
        assert!(
            !p.warnings
                .iter()
                .any(|w| matches!(w, PlanWarning::NoSpace { .. })),
            "{layout}: {:?}",
            p.warnings
        );
        // The duplicates report names its units by absolute path.
        let mut value = redact(&p);
        under_source_root(&mut value, s.path());
        insta::assert_json_snapshot!(format!("systems_batch_3__{layout}"), value);
    }
}

/// Rewrites every string of `value` that starts with `root` (as given or
/// canonical, `\\?\` dropped, after [`redact`] turned `\` into `/`) as
/// `<source_root>` plus the rest.
fn under_source_root(value: &mut serde_json::Value, root: &Path) {
    fn slashed(path: &Path) -> String {
        let s = path.to_string_lossy().replace('\\', "/");
        s.strip_prefix("//?/").map(str::to_string).unwrap_or(s)
    }
    fn walk(value: &mut serde_json::Value, roots: &[String]) {
        match value {
            serde_json::Value::String(s) => {
                let plain = s.strip_prefix("//?/").unwrap_or(s).to_string();
                if let Some(rest) = roots.iter().find_map(|r| plain.strip_prefix(r.as_str())) {
                    *s = format!("<source_root>{rest}");
                }
            }
            serde_json::Value::Array(values) => values.iter_mut().for_each(|v| walk(v, roots)),
            serde_json::Value::Object(map) => map.values_mut().for_each(|v| walk(v, roots)),
            _ => {}
        }
    }
    let mut roots = vec![slashed(root)];
    if let Ok(canonical) = fs::canonicalize(root) {
        roots.push(slashed(&canonical));
    }
    walk(value, &roots);
}
