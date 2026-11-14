//! PC arcade (spec 2026-10-26-pc-arcade): `cave3rd` like `model2`, and
//! `typex` as one game per folder, recognised only by its location and
//! never read. Fixtures are empty files and folders with pinned mtimes;
//! paths are compared by component.

mod common;

use std::fs;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::SystemTime;

use coverly_core::cache::{Cache, DatRow};
use coverly_core::dedupe::testing::pin_mtime;
use coverly_core::organize::{
    plan_organize, MetaProbe, OrganizeAction, OrganizeEnv, OrganizeOptions, OrganizePlan,
};
use coverly_core::profiles::{builtin_profiles, Profile};
use coverly_core::systems::{Family, SystemDb};
use tempfile::TempDir;

struct NoCloud;
impl MetaProbe for NoCloud {
    fn cloud_only(&self, _: &Path, _: &fs::Metadata) -> bool {
        false
    }
}

fn profile(id: &str) -> Profile {
    builtin_profiles()
        .into_iter()
        .find(|p| p.id == id)
        .unwrap_or_else(|| panic!("no profile {id}"))
}

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
            titles: None,
            now: SystemTime::now(),
        }
    }
    /// Seeds `dat` in the cache with `set`, one row per `(rom, bytes)`.
    fn cache_set(&self, dat: &str, set: &str, roms: &[(&str, &[u8])]) {
        let rows: Vec<DatRow> = roms
            .iter()
            .map(|(rom, bytes)| DatRow {
                game: set.into(),
                rom_name: (*rom).into(),
                size: Some(bytes.len() as u64),
                crc32: Some(format!("{:08X}", crc32fast::hash(bytes))),
                md5: None,
                sha1: None,
                serial: None,
            })
            .collect();
        self.cache.replace_dat(dat, &rows).unwrap();
    }
}

fn plan(world: &World, source: &Path, target: &Path, layout: &str) -> OrganizePlan {
    let opts = OrganizeOptions {
        layout: layout.into(),
        offline: true,
        target: Some(target.to_path_buf()),
        ..OrganizeOptions::default()
    };
    plan_organize(
        source,
        &opts,
        &world.env(),
        &|_| {},
        &AtomicBool::new(false),
    )
    .unwrap()
}

const MAME_DAT: &str = "metadat/mame/MAME.dat";
const FBNEO_DAT: &str = "metadat/fbneo-split/FBNeo - Arcade Games.dat";

#[test]
fn cave3rd_entry_and_order() {
    let db = SystemDb::builtin();
    let s = db.get("cave3rd").expect("cave3rd");
    assert_eq!(s.name, "Cave CV1000");
    assert_eq!(s.family, Family::Arcade);
    assert!(s.arcade);
    assert_eq!(s.extensions, ["zip", "7z"]);
    assert_eq!(s.libretro.as_deref(), Some("MAME"));
    assert_eq!(s.dats, [MAME_DAT]);
    assert_eq!(s.screenscraper, Some(75));
    assert!(!s.accepts_extension("bin"));
    for alias in ["cave3rd", "CAVE3RD", "Cave3rd"] {
        assert_eq!(
            db.resolve_folder(alias).map(|s| s.id.as_str()),
            Some("cave3rd")
        );
    }
    let pos = |id: &str| db.all().iter().position(|s| s.id == id).unwrap();
    assert!(pos("cave3rd") > pos("mame"));
    assert!(pos("cave3rd") > pos("triforce"));
    assert_eq!(
        db.system_for_dat(MAME_DAT).map(|s| s.id.as_str()),
        Some("mame")
    );
}

#[test]
fn cave3rd_folders_per_profile() {
    use coverly_core::profiles::ProfileFolder;
    let named = |p: &str| match profile(p).folders.get("cave3rd") {
        Some(ProfileFolder::Named(n)) => Some(n.clone()),
        _ => None,
    };
    assert_eq!(named("batocera").as_deref(), Some("cave3rd"));
    for p in [
        "esde",
        "muos",
        "onion",
        "garlicos",
        "minui",
        "anbernic-stock",
    ] {
        assert_eq!(named(p), None, "{p}");
    }
}

#[test]
fn organize_keeps_cave3rd_and_sends_a_loose_set_to_fbneo() {
    let world = World::new();
    let roms: [(&str, &[u8]); 1] = [("u4", b"mushihime synthetic")];
    world.cache_set(MAME_DAT, "mushisam", &roms);
    world.cache_set(FBNEO_DAT, "mushisam", &roms);
    let (s, t) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    // The sorted copy hashes like the loose one (the FBNeo DAT names it),
    // so only its `cave3rd/` location keeps it from moving to `fbneo`.
    let sorted = s.path().join("cave3rd").join("mushisam.zip");
    fs::create_dir_all(sorted.parent().unwrap()).unwrap();
    common::roms::zip_file(&sorted, &[(roms[0].0, roms[0].1, true)]);
    pin_mtime(&sorted, 1);
    let loose = s.path().join("mushisam.zip");
    common::roms::zip_file(&loose, &[(roms[0].0, roms[0].1, true)]);
    pin_mtime(&loose, 1);

    let p = plan(&world, s.path(), t.path(), "batocera");
    // Item sources are relative to the source root; compared by component.
    let loose_item = p
        .items
        .iter()
        .find(|i| i.sources[0] == Path::new("mushisam.zip"))
        .expect("loose set is an item");
    assert_eq!(loose_item.system_id.as_deref(), Some("fbneo"));
    assert!(matches!(loose_item.action, OrganizeAction::Auto));
    // The set already inside `cave3rd/` stays: no item names it.
    assert!(p
        .items
        .iter()
        .flat_map(|i| i.sources.iter())
        .all(|src| src.components().all(|c| c.as_os_str() != "cave3rd")));
    assert_eq!(p.items.len(), 1);
    // ...because `cave3rd/` is a sorted folder resolving to `cave3rd`.
    assert!(p
        .sorted_folders
        .iter()
        .any(|(dir, id)| dir == Path::new("cave3rd") && id == "cave3rd"));
}

#[test]
fn typex_entry_and_aliases() {
    use coverly_core::organize::walk::{is_folder_per_game, FOLDER_PER_GAME_SYSTEMS};
    let db = SystemDb::builtin();
    let s = db.get("typex").expect("typex");
    assert_eq!(s.name, "Taito Type X");
    assert_eq!(s.family, Family::Arcade);
    assert!(!s.arcade, "no short names: never the arcade-DAT path");
    assert!(s.extensions.is_empty());
    assert_eq!(
        s.weak_extensions,
        ["exe", "bat", "lnk", "sh", "desktop", "appimage"]
    );
    assert_eq!(s.libretro, None);
    assert!(s.dats.is_empty());
    assert_eq!(s.screenscraper, Some(112));
    assert!(s.pc_only && !s.no_emulator);
    for alias in ["typex", "TYPEX", "type-x", "Type-X"] {
        assert_eq!(
            db.resolve_folder(alias).map(|s| s.id.as_str()),
            Some("typex"),
            "{alias}"
        );
        for p in builtin_profiles() {
            assert_eq!(
                db.resolve_folder_in(Some(&p), alias).map(|s| s.id.as_str()),
                Some("typex"),
                "{alias} under {}",
                p.id
            );
        }
    }
    // Location only: a weak extension is never evidence by itself.
    assert!(db
        .systems_for_extension("exe")
        .iter()
        .all(|s| s.id != "typex"));
    assert_eq!(FOLDER_PER_GAME_SYSTEMS, ["typex"]);
    assert!(is_folder_per_game("typex") && !is_folder_per_game("scummvm"));
    // Only typex is PC only.
    let pc: Vec<&str> = db
        .all()
        .iter()
        .filter(|s| s.pc_only)
        .map(|s| s.id.as_str())
        .collect();
    assert_eq!(pc, ["typex"]);
}
