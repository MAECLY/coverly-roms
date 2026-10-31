//! Spec D4 / §14.1: key and licence files are never opened by the walk,
//! the plan's inspection, the library scan's hashing or dedupe (an open
//! hook records every path a ROM reader opens).

mod common;

use std::fs;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;
use std::time::SystemTime;

use coverly_core::archive::Limits;
use coverly_core::cache::Cache;
use coverly_core::consoles::{start_open_log, take_open_log};
use coverly_core::dedupe::archive::ArchiveEntryListing;
use coverly_core::dedupe::disc::ReaderDiscProbe;
use coverly_core::dedupe::keys::DiskBytes;
use coverly_core::dedupe::testing::pin_mtime;
use coverly_core::dedupe::{plan_dedupe, DedupeEnv, DedupeOptions};
use coverly_core::identify::sniff::fixtures as fx;
use coverly_core::organize::plan::{LeftReason, OrganizeAction};
use coverly_core::organize::{plan_organize, MetaProbe, OrganizeEnv, OrganizeOptions};
use coverly_core::profiles::builtin_profiles;
use coverly_core::scan::{scan_library, ScanOptions};
use coverly_core::settings::Settings;
use coverly_core::systems::SystemDb;
use tempfile::TempDir;

/// The open log is one per process: the tests of this file take turns.
static SERIAL: Mutex<()> = Mutex::new(());

const KEYS: [&str; 4] = ["prod.keys", "title.keys", "x.rap", "work.bin"];

struct NoCloud;

impl MetaProbe for NoCloud {
    fn cloud_only(&self, _: &Path, _: &fs::Metadata) -> bool {
        false
    }
}

fn put(path: &Path, bytes: &[u8], n: u64) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
    pin_mtime(path, n);
}

fn opened(log: &[std::path::PathBuf], name: &str) -> bool {
    log.iter().any(|p| p.file_name().unwrap() == name)
}

#[test]
fn key_material_is_never_opened() {
    let _turn = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let pile = TempDir::new().unwrap();
    for (n, name) in KEYS.iter().enumerate() {
        put(&pile.path().join(name), &[7u8; 2048], n as u64);
    }
    put(&pile.path().join("Game.gba"), &fx::gba(), 9);

    let db = SystemDb::builtin();
    let profiles = builtin_profiles();
    let cache = Cache::in_memory().unwrap();
    let env = OrganizeEnv {
        db: &db,
        profiles: &profiles,
        cache: &cache,
        dats: None,
        probe: &NoCloud,
        online: None,
        now: SystemTime::now(),
    };
    let opts = OrganizeOptions {
        layout: "esde".to_string(),
        offline: true,
        ..OrganizeOptions::default()
    };
    start_open_log();
    let plan = plan_organize(pile.path(), &opts, &env, &|_| {}, &AtomicBool::new(false)).unwrap();
    let log = take_open_log();

    for name in KEYS {
        assert!(!opened(&log, name), "{name} opened");
        let item = plan
            .items
            .iter()
            .find(|i| i.sources[0].file_name().unwrap() == name)
            .unwrap_or_else(|| panic!("{name} has no item"));
        assert!(
            matches!(
                &item.action,
                OrganizeAction::Left {
                    reason: LeftReason::KeyMaterial
                }
            ),
            "{name}: {:?}",
            item.action
        );
    }
    assert!(opened(&log, "Game.gba"), "the hook records opens");
}

#[test]
fn key_material_in_a_library_is_never_hashed() {
    let _turn = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let root = TempDir::new().unwrap();
    // `.bin` is a Mega Drive extension: only the name keeps `work.bin` out.
    put(&root.path().join("ROMS/MD/work.bin"), &[7u8; 4096], 1);
    put(&root.path().join("ROMS/GBA/Game.gba"), &fx::gba(), 2);
    put(&root.path().join("ROMS/GBA/Copy.gba"), &fx::gba(), 3);

    let db = SystemDb::builtin();
    let profile = common::profile("anbernic-stock");
    start_open_log();
    let scan = scan_library(root.path(), &profile, &db, &ScanOptions::default());
    let in_scan = scan
        .systems
        .iter()
        .flat_map(|s| s.roms.iter())
        .any(|r| r.path.file_name().unwrap() == "work.bin");
    assert!(!in_scan, "a key file is never a library entry");

    let cache = Cache::in_memory().unwrap();
    let settings = Settings::default();
    let listing = ArchiveEntryListing::new(Limits::default());
    let discs = ReaderDiscProbe::new(ArchiveEntryListing::new(Limits::default()));
    let cancel = AtomicBool::new(false);
    let env = DedupeEnv {
        cache: &cache,
        db: &db,
        profile: &profile,
        settings: &settings,
        listing: &listing,
        discs: &discs,
        dats: None,
        bytes: &DiskBytes,
        progress: &|_| {},
        cancel: &cancel,
    };
    plan_dedupe(&scan, &DedupeOptions::from_settings(&settings.dedupe), &env).unwrap();
    let log = take_open_log();
    assert!(!opened(&log, "work.bin"), "work.bin opened");
    assert!(opened(&log, "Game.gba"), "the hook records dedupe reads");
}
