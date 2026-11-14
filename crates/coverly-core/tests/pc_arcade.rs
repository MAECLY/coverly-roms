//! PC arcade (spec 2026-10-26-pc-arcade): `cave3rd` like `model2`, and
//! `typex` as one game per folder, recognised only by its location and
//! never read. Fixtures are empty files and folders with pinned mtimes;
//! paths are compared by component.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
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

#[test]
fn typex_folders_per_profile_and_art_union() {
    use coverly_core::profiles::{builtin_art_dir_names, ProfileFolder};
    let named = |p: &str| match profile(p).folders.get("typex") {
        Some(ProfileFolder::Named(n)) => Some(n.clone()),
        _ => None,
    };
    assert_eq!(named("esde").as_deref(), Some("type-x"));
    for p in [
        "batocera",
        "muos",
        "onion",
        "garlicos",
        "minui",
        "anbernic-stock",
    ] {
        assert_eq!(named(p), None, "{p}");
        assert!(
            matches!(profile(p).folders.get("typex"), Some(ProfileFolder::Absent)),
            "{p}"
        );
    }
    let union = builtin_art_dir_names();
    for p in builtin_profiles() {
        for d in &p.art_dir_names {
            assert!(
                union.iter().any(|u| u.eq_ignore_ascii_case(d)),
                "{d} of {}",
                p.id
            );
        }
    }
    // Spec P4 names these; a Batocera `type-x/images/` read as ES-DE, or a
    // library `.res/`, is never a game.
    for d in [
        "images",
        "videos",
        "manuals",
        "magazines",
        "Imgs",
        ".media",
        ".res",
    ] {
        assert!(union.iter().any(|u| u.eq_ignore_ascii_case(d)), "{d}");
    }
    for (i, a) in union.iter().enumerate() {
        assert!(!a.trim().is_empty(), "empty art folder name");
        assert!(
            !union[i + 1..].iter().any(|b| b.eq_ignore_ascii_case(a)),
            "{a} listed twice"
        );
    }
}

/// Creates the empty file `path` (and its parents) with a pinned mtime.
fn touch(path: &Path, n: u64) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, b"").unwrap();
    pin_mtime(path, n);
}

/// The last component of `path`.
fn name(path: &Path) -> String {
    path.file_name().unwrap().to_string_lossy().into_owned()
}

/// The §10 Type X games in `<parent>/type-x/`; returns that folder.
fn typex_library(parent: &Path) -> PathBuf {
    let sys = parent.join("type-x");
    touch(&sys.join("Raiden IV (Type X)").join("game.exe"), 1);
    fs::create_dir_all(sys.join("Raiden IV (Type X)").join("data")).unwrap();
    touch(&sys.join("Chaos Code.exe").join("Chaos Code.exe"), 2);
    touch(
        &sys.join("Akai Katana Shin for NesicaxLive")
            .join("Game.exe"),
        3,
    );
    touch(&sys.join("Homura.lnk"), 4);
    touch(&sys.join("images").join("Raiden IV (Type X).png"), 5);
    touch(&sys.join("readme.txt"), 6);
    touch(&sys.join(".git").join("HEAD"), 7);
    fs::create_dir_all(sys.join("Empty Game")).unwrap();
    sys
}

#[test]
fn scan_typex_folder_games() {
    use coverly_core::identify::names::parse_name;
    use coverly_core::scan::{scan_library, ScanOptions};
    for (layout, chaos_stem) in [("esde", "Chaos Code.exe"), ("batocera", "Chaos Code")] {
        let p = profile(layout);
        let tmp = TempDir::new().unwrap();
        // ES-DE's folder is `type-x`; Batocera has none, the global alias resolves.
        typex_library(&tmp.path().join(&p.roms_root));
        let scan = scan_library(
            tmp.path(),
            &p,
            &SystemDb::builtin(),
            &ScanOptions::default(),
        );
        assert!(scan.warnings.is_empty(), "{layout}: {:?}", scan.warnings);
        let sys = scan
            .systems
            .iter()
            .find(|s| s.folder_name == "type-x")
            .unwrap();
        assert_eq!(sys.system_id.as_deref(), Some("typex"));
        let mut rows: Vec<(String, bool, String)> = sys
            .roms
            .iter()
            .map(|r| {
                (
                    name(&r.path),
                    r.is_dir,
                    parse_name(r.title_stem(), false).title,
                )
            })
            .collect();
        rows.sort();
        assert_eq!(
            rows,
            [
                (
                    "Akai Katana Shin for NesicaxLive".into(),
                    true,
                    "Akai Katana Shin".into()
                ),
                ("Chaos Code.exe".into(), true, "Chaos Code".into()),
                ("Empty Game".into(), true, "Empty Game".into()),
                ("Homura.lnk".into(), false, "Homura".into()),
                ("Raiden IV (Type X)".into(), true, "Raiden IV".into()),
            ],
            "{layout}"
        );
        let chaos = sys
            .roms
            .iter()
            .find(|r| name(&r.path) == "Chaos Code.exe")
            .unwrap();
        assert_eq!(chaos.stem, chaos_stem, "{layout}");
        assert_eq!(chaos.ext, "exe");
        let raiden = sys
            .roms
            .iter()
            .find(|r| name(&r.path) == "Raiden IV (Type X)")
            .unwrap();
        assert_eq!(raiden.ext, "", "{layout}: no suffix, no ext");
        for r in sys.roms.iter().filter(|r| r.is_dir) {
            assert!(r.children.is_empty() && r.fixed_counts.is_none() && r.role.is_none());
            assert_eq!(r.size, 0);
            // Exactly one level below the system folder, by component.
            assert_eq!(r.path.parent().unwrap().file_name().unwrap(), "type-x");
            assert!(r.rel_dir.as_os_str().is_empty());
        }
    }
}

#[test]
fn scan_typex_uppercase_suffix() {
    use coverly_core::identify::names::parse_name;
    use coverly_core::scan::{scan_library, ScanOptions};
    for layout in ["esde", "batocera"] {
        let p = profile(layout);
        let tmp = TempDir::new().unwrap();
        let sys = tmp.path().join(&p.roms_root).join("type-x");
        touch(&sys.join("Chaos Code.EXE").join("Chaos Code.exe"), 1);
        let scan = scan_library(
            tmp.path(),
            &p,
            &SystemDb::builtin(),
            &ScanOptions::default(),
        );
        let sys = scan
            .systems
            .iter()
            .find(|s| s.folder_name == "type-x")
            .unwrap();
        assert_eq!(sys.roms.len(), 1, "{layout}");
        let r = &sys.roms[0];
        assert!(r.is_dir);
        assert_eq!(name(&r.path), "Chaos Code.EXE");
        assert_eq!(r.ext, "exe", "{layout}");
        assert_eq!(
            parse_name(r.title_stem(), false).title,
            "Chaos Code",
            "{layout}"
        );
    }
}

#[test]
fn scan_typex_skips_the_profile_art_folders_too() {
    use coverly_core::scan::{scan_library, ScanOptions};
    // A custom profile's own art folder is not a built-in one: inside a
    // typex folder it is still art, never a game.
    let mut p = profile("esde");
    p.art_dir_names = vec!["Covers".into()];
    let tmp = TempDir::new().unwrap();
    let sys = tmp.path().join(&p.roms_root).join("type-x");
    touch(&sys.join("Raiden IV (Type X)").join("game.exe"), 1);
    touch(&sys.join("covers").join("Raiden IV (Type X).png"), 2);
    // A built-in art folder stays skipped as well.
    touch(&sys.join("images").join("Raiden IV (Type X).png"), 3);
    let scan = scan_library(
        tmp.path(),
        &p,
        &SystemDb::builtin(),
        &ScanOptions::default(),
    );
    let sys = scan
        .systems
        .iter()
        .find(|s| s.folder_name == "type-x")
        .unwrap();
    let names: Vec<String> = sys.roms.iter().map(|r| name(&r.path)).collect();
    assert_eq!(names, ["Raiden IV (Type X)"]);
}

#[test]
fn organize_keeps_type_x() {
    use coverly_core::organize::LeftReason;
    let world = World::new();
    for layout in ["esde", "batocera"] {
        let (s, t) = (TempDir::new().unwrap(), TempDir::new().unwrap());
        let sys = typex_library(s.path());
        // Pile at the root: `type-x/` resolves, is sorted, never walked.
        let p = plan(&world, s.path(), t.path(), layout);
        // Item sources are relative to the pile root.
        assert!(
            p.items
                .iter()
                .all(|i| i.sources.iter().all(|src| !src.starts_with("type-x"))),
            "{layout}"
        );
        // Pile picked at `type-x/` itself (P16): one FolderGame row per game.
        let p = plan(&world, &sys, t.path(), layout);
        let mut left: Vec<String> = p
            .items
            .iter()
            .filter_map(|i| match &i.action {
                OrganizeAction::Left {
                    reason: LeftReason::FolderGame { system, .. },
                } => {
                    assert_eq!(system, "typex", "{layout}");
                    Some(name(&i.sources[0]))
                }
                _ => None,
            })
            .collect();
        left.sort();
        assert_eq!(
            left,
            [
                "Akai Katana Shin for NesicaxLive",
                "Chaos Code.exe",
                "Empty Game",
                "Raiden IV (Type X)"
            ],
            "{layout}"
        );
        assert!(
            p.items
                .iter()
                .all(|i| i.sources.iter().all(|src| src.components().count() == 1)),
            "nothing below a game is an item ({layout})"
        );
        assert!(
            p.items
                .iter()
                .all(|i| i.system_id.as_deref() != Some("typex")
                    || matches!(i.action, OrganizeAction::Left { .. })),
            "nothing moves to typex ({layout})"
        );
    }
}

#[test]
fn loose_game_exe_is_not_typex() {
    use coverly_core::organize::{plan_from_survey, survey, Choice, Decision};
    let world = World::new();
    let (s, t) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    // Invented bytes: an empty file reads as a partial download.
    let binary: Vec<u8> = (0..=255u8).cycle().take(4096).collect();
    for (rel, n) in [
        (Path::new("Raiden IV (Type X)").join("game.exe"), 1),
        (PathBuf::from("game.exe"), 2),
        (PathBuf::from("Homura.lnk"), 3),
        (PathBuf::from("Mystery.bin"), 4),
    ] {
        let path = s.path().join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, &binary).unwrap();
        pin_mtime(&path, n);
    }
    let p = plan(&world, s.path(), t.path(), "esde");
    assert!(p
        .items
        .iter()
        .all(|i| i.system_id.as_deref() != Some("typex")));
    assert!(p
        .items
        .iter()
        .all(|i| i.alternatives.iter().all(|(id, _)| id != "typex")));

    // A crafted choice naming typex is refused: every item stays as it was.
    let opts = OrganizeOptions {
        layout: "esde".into(),
        offline: true,
        target: Some(t.path().to_path_buf()),
        ..OrganizeOptions::default()
    };
    let env = world.env();
    let surveyed = survey(s.path(), &opts, &env, &|_| {}, &AtomicBool::new(false)).unwrap();
    let before = plan_from_survey(&surveyed, &opts, &[], &env).unwrap();
    // `.exe`, `.bat` and launcher extensions are never an item's: each
    // launcher is an other file, left untouched.
    assert!(before.items.iter().all(|i| i.sources.iter().all(|src| {
        let n = name(src);
        n != "game.exe" && n != "Homura.lnk"
    })));
    assert_eq!(before.counts.other_files, 3);
    // An unknown ROM-like file is left unknown: decidable, so the refusal
    // below is real.
    let mystery = before
        .items
        .iter()
        .find(|i| i.sources.iter().any(|src| name(src) == "Mystery.bin"))
        .expect("an item for Mystery.bin");
    assert!(
        matches!(
            mystery.action,
            OrganizeAction::Review
                | OrganizeAction::Left {
                    reason: coverly_core::organize::LeftReason::Unknown
                }
        ),
        "{:?}",
        mystery.action
    );
    for item in &before.items {
        let after = plan_from_survey(
            &surveyed,
            &opts,
            &[Decision {
                item: item.id,
                choice: Choice::Assign {
                    system: "typex".into(),
                },
            }],
            &env,
        )
        .unwrap();
        let got = &after.items[item.id];
        assert_ne!(
            got.system_id.as_deref(),
            Some("typex"),
            "{:?}",
            item.sources
        );
        assert_eq!(got.system_id, item.system_id, "{:?}", item.sources);
        assert_eq!(got.action, item.action, "{:?}", item.sources);
        assert_eq!(got.method, item.method, "{:?}", item.sources);
    }
    // The refused choice is not remembered either.
    let again = plan(&world, s.path(), t.path(), "esde");
    assert!(again
        .items
        .iter()
        .all(|i| i.system_id.as_deref() != Some("typex")));
}
