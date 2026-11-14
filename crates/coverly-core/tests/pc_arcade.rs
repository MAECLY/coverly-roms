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

#[test]
fn organize_type_x_source_keeps_the_existing_skips() {
    use coverly_core::organize::LeftReason;
    // P4: set-aside and quarantine folders and partial downloads are
    // skipped before a child folder of a `type-x/` pile counts as a game.
    let world = World::new();
    let (s, t) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let sys = typex_library(s.path());
    touch(
        &sys.join("_Duplicados").join("Old Copy").join("game.exe"),
        8,
    );
    touch(&sys.join("_Duplicates").join("game.exe"), 9);
    fs::create_dir_all(sys.join("Raiden V.download")).unwrap();
    let p = plan(&world, &sys, t.path(), "esde");
    let mut games: Vec<String> = Vec::new();
    let mut partial: Vec<String> = Vec::new();
    for i in &p.items {
        match &i.action {
            OrganizeAction::Left {
                reason: LeftReason::FolderGame { .. },
            } => games.push(name(&i.sources[0])),
            OrganizeAction::Left {
                reason: LeftReason::PartialDownload,
            } => partial.push(name(&i.sources[0])),
            _ => {}
        }
    }
    games.sort();
    assert_eq!(
        games,
        [
            "Akai Katana Shin for NesicaxLive",
            "Chaos Code.exe",
            "Empty Game",
            "Raiden IV (Type X)"
        ]
    );
    // (Empty root files read as partial downloads too; only the folder
    // matters here.)
    assert!(
        partial.iter().any(|n| n == "Raiden V.download"),
        "{partial:?}"
    );
    assert!(
        p.items.iter().all(|i| i
            .sources
            .iter()
            .all(|src| !src.starts_with("_Duplicados") && !src.starts_with("_Duplicates"))),
        "a quarantine is never walked"
    );
}

/// A dedupe env over `cache` with the given byte reader (scan → plan).
fn dedupe_plan(
    root: &Path,
    p: &Profile,
    cache: &Cache,
    bytes: &dyn coverly_core::dedupe::keys::FileBytes,
) -> coverly_core::dedupe::DedupePlan {
    use coverly_core::archive::Limits;
    use coverly_core::dedupe::archive::ArchiveEntryListing;
    use coverly_core::dedupe::testing::FakeDiscProbe;
    use coverly_core::dedupe::{plan_dedupe, DedupeEnv, DedupeOptions};
    use coverly_core::scan::{scan_library, ScanOptions};
    use coverly_core::settings::{DedupeLevel, Settings};
    let db = SystemDb::builtin();
    let settings = Settings::default();
    let mut opts = DedupeOptions::from_settings(&settings.dedupe);
    opts.levels = vec![
        DedupeLevel::Exact,
        DedupeLevel::Format,
        DedupeLevel::Version,
    ];
    opts.one_game_one_rom = true;
    let env = DedupeEnv {
        cache,
        db: &db,
        profile: p,
        settings: &settings,
        listing: &ArchiveEntryListing::new(Limits::default()),
        discs: &FakeDiscProbe::new(),
        dats: None,
        bytes,
        progress: &|_| {},
        cancel: &AtomicBool::new(false),
    };
    let scan = scan_library(root, p, &db, &ScanOptions::default());
    plan_dedupe(&scan, &opts, &env).unwrap()
}

#[test]
fn dedupe_skips_typex() {
    use coverly_core::dedupe::archive::ArchiveEntryListing;
    use coverly_core::dedupe::testing::CountingBytes;
    use coverly_core::dedupe::units::{units_from_scan, Exclusion, UnitOptions};
    use coverly_core::scan::{scan_library, ScanOptions};
    // P15: the same title twice, same bytes: no group, nothing read.
    let p = profile("esde");
    let tmp = TempDir::new().unwrap();
    let sys = tmp.path().join(&p.roms_root).join("type-x");
    let bytes: Vec<u8> = (0..=255u8).cycle().take(4096).collect();
    for (game, n) in [("Raiden IV (Type X)", 1), ("Raiden IV.exe", 2)] {
        let f = sys.join(game).join("game.exe");
        fs::create_dir_all(f.parent().unwrap()).unwrap();
        fs::write(&f, &bytes).unwrap();
        pin_mtime(&f, n);
    }
    let cache = Cache::in_memory().unwrap();
    let counting = CountingBytes::new();
    let plan = dedupe_plan(tmp.path(), &p, &cache, &counting);
    assert!(plan.groups.is_empty(), "{:?}", plan.groups);
    assert_eq!(counting.opens(), 0, "no file opened");

    let db = SystemDb::builtin();
    let scan = scan_library(tmp.path(), &p, &db, &ScanOptions::default());
    let set = units_from_scan(
        &scan,
        &db,
        &UnitOptions::for_profile(&p),
        &ArchiveEntryListing::new(coverly_core::archive::Limits::default()),
    );
    assert!(set.units.is_empty(), "{:?}", set.units);
    assert!(set
        .excluded
        .iter()
        .any(|(path, why)| path == &sys && *why == Exclusion::Folder));
}

#[test]
fn esde_typex_art_path() {
    use coverly_core::art::target_path;
    use coverly_core::profiles::Media;
    let db = SystemDb::builtin();
    let p = profile("esde");
    let root = Path::new("lib");
    let sys_dir = root.join(&p.roms_root).join("type-x");
    let got = target_path(
        root,
        &p,
        &sys_dir,
        db.get("typex").unwrap(),
        Path::new(""),
        "Chaos Code.exe",
        Media::Cover,
    )
    .unwrap();
    // Known limitation (P8, owner question): ES-DE reads
    // downloaded_media/type-x/, Coverly writes the system id. A later
    // `{system_folder}` fix changes this on purpose.
    let want = root.join("downloaded_media").join("typex").join("covers");
    assert_eq!(got.parent().unwrap(), want);
    assert_eq!(got.file_stem().unwrap(), "Chaos Code.exe");
    assert_eq!(got.extension().unwrap(), "png");
}

// --- The zero-read tripwire (spec §10, G2, P5) ---

/// Makes every file under each game folder unreadable for the life of
/// the guard (Unix: mode 0o000, folders stay 0o755; Windows: an
/// exclusive handle per file). `None` when running as root on Unix,
/// where permissions stop nothing: the test still runs its asserts.
struct Locked {
    #[cfg(windows)]
    _handles: Vec<fs::File>,
    #[cfg(unix)]
    files: Vec<PathBuf>,
}

/// Every file below the game folders `games`.
fn files_in(games: &[PathBuf]) -> Vec<PathBuf> {
    games
        .iter()
        .flat_map(|g| walkdir::WalkDir::new(g).into_iter().filter_map(Result::ok))
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .collect()
}

fn lock_games(games: &[PathBuf]) -> Option<Locked> {
    let files = files_in(games);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // SAFETY: geteuid has no preconditions and cannot fail.
        if unsafe { libc::geteuid() } == 0 {
            eprintln!("typex_reads_no_byte: running as root, permission lock skipped");
            return None;
        }
        for f in &files {
            fs::set_permissions(f, fs::Permissions::from_mode(0o000)).unwrap();
        }
        let locked = Locked { files };
        for f in &locked.files {
            assert!(fs::File::open(f).is_err(), "{} still opens", f.display());
        }
        Some(locked)
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        let handles: Vec<fs::File> = files
            .iter()
            .map(|f| {
                fs::OpenOptions::new()
                    .read(true)
                    .share_mode(0)
                    .open(f)
                    .unwrap()
            })
            .collect();
        for f in &files {
            assert!(fs::File::open(f).is_err(), "{} still opens", f.display());
        }
        Some(Locked { _handles: handles })
    }
}

#[cfg(unix)]
impl Drop for Locked {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        for f in &self.files {
            let _ = fs::set_permissions(f, fs::Permissions::from_mode(0o644));
        }
    }
}

/// Records every `file_len` question of the organize walk.
#[derive(Default)]
struct CountingProbe {
    seen: std::sync::Mutex<Vec<PathBuf>>,
}

impl MetaProbe for CountingProbe {
    fn cloud_only(&self, _: &Path, _: &fs::Metadata) -> bool {
        false
    }
    fn file_len(&self, path: &Path, meta: &fs::Metadata) -> u64 {
        self.seen.lock().unwrap().push(path.to_path_buf());
        meta.len()
    }
}

/// Every string in `v` (keys excluded), depth first.
fn strings(v: &serde_json::Value, out: &mut Vec<String>) {
    match v {
        serde_json::Value::String(s) => out.push(s.clone()),
        serde_json::Value::Array(a) => a.iter().for_each(|x| strings(x, out)),
        serde_json::Value::Object(o) => o.values().for_each(|x| strings(x, out)),
        _ => {}
    }
}

/// Panics when any path in `value` (absolute, or relative to `base`) lies
/// strictly inside one of `games`.
fn names_nothing_inside(what: &str, value: &impl serde::Serialize, base: &Path, games: &[PathBuf]) {
    let mut all = Vec::new();
    strings(&serde_json::to_value(value).unwrap(), &mut all);
    for s in all {
        let p = Path::new(&s);
        for candidate in [p.to_path_buf(), base.join(p)] {
            for g in games {
                assert!(
                    !(candidate.starts_with(g) && candidate != *g),
                    "{what} names {s} inside {}",
                    g.display()
                );
            }
        }
    }
}

#[test]
fn typex_reads_no_byte() {
    use coverly_core::audit::build_report;
    use coverly_core::cache::mtime_ns;
    use coverly_core::consoles::{start_open_log, take_open_log};
    use coverly_core::dedupe::testing::CountingBytes;
    use coverly_core::identify::dat::DatStore;
    use coverly_core::identify::{Identifier, IdentifyOptions, MatchMethod};
    use coverly_core::net::{NetConfig, PoliteHttp, UreqHttp};
    use coverly_core::profiles::Media;
    use coverly_core::scan::{scan_library, ScanOptions};
    use coverly_core::sources::embedded::EmbeddedSource;
    use coverly_core::sources::{ArtQuery, ArtSource};

    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("SD");
    let p = profile("esde");
    let db = SystemDb::builtin();
    let sys = typex_library(&root.join(&p.roms_root));
    // A key-named file and a non-empty blob inside a game: neither is read.
    let raiden = sys.join("Raiden IV (Type X)");
    let invented: Vec<u8> = (0..=255u8).cycle().take(1000).collect();
    fs::write(raiden.join("license.dat"), &invented[..16]).unwrap();
    pin_mtime(&raiden.join("license.dat"), 10);
    fs::write(raiden.join("data").join("blob.bin"), &invented).unwrap();
    pin_mtime(&raiden.join("data").join("blob.bin"), 11);
    let games: Vec<PathBuf> = [
        "Raiden IV (Type X)",
        "Chaos Code.exe",
        "Akai Katana Shin for NesicaxLive",
        "Empty Game",
    ]
    .iter()
    .map(|g| sys.join(g))
    .collect();
    let inside = files_in(&games);
    assert_eq!(inside.len(), 5, "{inside:?}");
    let target = tmp.path().join("target");
    fs::create_dir_all(&target).unwrap();

    // Mtimes are pinned above, before the lock. Dropped before `tmp`.
    let _lock = lock_games(&games);
    start_open_log();

    // Scan.
    let scan = scan_library(&root, &p, &db, &ScanOptions::default());
    for w in &scan.warnings {
        assert!(
            !games.iter().any(|g| w.path.starts_with(g) && w.path != *g),
            "scan warning {w:?}"
        );
    }
    let typex = scan
        .systems
        .iter()
        .find(|s| s.system_id.as_deref() == Some("typex"))
        .unwrap();
    assert_eq!(typex.roms.iter().filter(|r| r.is_dir).count(), 4);
    for r in &typex.roms {
        assert!(!games.iter().any(|g| r.path.starts_with(g) && r.path != *g));
        assert!(r.children.is_empty());
        if r.is_dir {
            let want: u64 = files_in(std::slice::from_ref(&r.path))
                .iter()
                .map(|f| fs::metadata(f).unwrap().len())
                .sum();
            assert_eq!(r.size, want, "{}", r.path.display());
        }
    }
    let raiden_rom = typex.roms.iter().find(|r| r.path == raiden).unwrap();
    assert_eq!(raiden_rom.size, 1016);

    // Identify, in every mode.
    let system = db.get("typex").unwrap();
    let cache = Cache::in_memory().unwrap();
    let net = NetConfig {
        retries: 0,
        ..NetConfig::default()
    };
    let http = PoliteHttp::new(UreqHttp::new(&net), &net);
    let dats = DatStore::new(&http, &cache, "http://127.0.0.1:9");
    for method in [
        MatchMethod::NameOnly,
        MatchMethod::HashThenName,
        MatchMethod::HashOnly,
    ] {
        let id = Identifier::new(
            &cache,
            &dats,
            IdentifyOptions {
                method,
                ..IdentifyOptions::default()
            },
        );
        for r in &typex.roms {
            let got = id.identify(r, system, false).unwrap();
            assert!(got.hash.is_none(), "{method:?} {}", r.path.display());
        }
    }

    // Fill's embedded source (the one that opens ROMs) offers nothing.
    for r in typex.roms.iter().filter(|r| r.is_dir) {
        let parsed = coverly_core::identify::names::parse_name(r.title_stem(), false);
        let file_name = name(&r.path);
        let got = EmbeddedSource
            .candidates(&ArtQuery {
                system,
                media: Media::Cover,
                canonical: None,
                rom: &parsed,
                stem: &r.stem,
                by_name: true,
                fuzzy: true,
                min_score: 65,
                max_candidates: 5,
                hash: None,
                size: r.size,
                file_name: &file_name,
                rom_path: Some(&r.path),
            })
            .unwrap_or_default();
        assert!(got.is_empty(), "{}", r.path.display());
    }

    // Dedupe.
    let counting = CountingBytes::new();
    let dplan = dedupe_plan(&root, &p, &cache, &counting);
    assert_eq!(counting.opens(), 0);
    assert!(dplan.groups.is_empty());
    names_nothing_inside("dedupe plan", &dplan, &root, &games);

    // Organize, from the library root and from `type-x/` itself.
    let probe = CountingProbe::default();
    let profiles = builtin_profiles();
    let env = OrganizeEnv {
        db: &db,
        profiles: &profiles,
        cache: &cache,
        dats: None,
        probe: &probe,
        online: None,
        titles: None,
        now: SystemTime::now(),
    };
    for source in [&root, &sys] {
        let opts = OrganizeOptions {
            layout: "esde".into(),
            offline: true,
            target: Some(target.clone()),
            ..OrganizeOptions::default()
        };
        let plan = plan_organize(source, &opts, &env, &|_| {}, &AtomicBool::new(false)).unwrap();
        names_nothing_inside("organize plan", &plan, source, &games);
    }
    // A game is left whole as a row (P16): the walk does not even measure
    // what is inside, so the counting probe never hears of those files.
    // Sizes come from metadata in the scan, checked above under the lock.
    let seen = probe.seen.lock().unwrap().clone();
    for f in &seen {
        assert!(
            !games.iter().any(|g| f.starts_with(g)),
            "the walk measured {}",
            f.display()
        );
    }

    // Audit (rescans, resolves art).
    let report = build_report(&root, &p, &db, &ScanOptions::default(), &Media::ALL);
    names_nothing_inside("audit report", &report, &root, &games);

    // Nothing was opened through the console readers.
    let opened = take_open_log();
    for o in &opened {
        assert!(
            !games.iter().any(|g| o.starts_with(g)),
            "opened {}",
            o.display()
        );
    }

    // No hash was ever attempted, no organize facts kept.
    for f in &inside {
        let meta = fs::metadata(f).unwrap();
        let mtime = mtime_ns(meta.modified().unwrap());
        assert!(cache.quick_id(f, meta.len(), mtime).unwrap().is_none());
        assert!(cache
            .organize_facts(f, meta.len(), mtime)
            .unwrap()
            .is_none());
    }
    for r in &typex.roms {
        let mtime = mtime_ns(r.modified);
        assert!(cache.quick_id(&r.path, r.size, mtime).unwrap().is_none());
    }
}

/// A pile picked at a Type X game, or at a folder inside one (P5, P16):
/// the walk lists, measures and opens nothing, and proposes nothing, so a
/// game's own files are never read nor moved out of it.
#[test]
fn organize_never_walks_inside_a_type_x_game() {
    let (s, t) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let sys = typex_library(&s.path().join("ROMS"));
    let game = sys.join("Raiden IV (Type X)");
    // Invented bytes: ROM-like names that would otherwise be sorted.
    let binary: Vec<u8> = (0..=255u8).cycle().take(4096).collect();
    for (rel, n) in [
        (Path::new("data").join("sound.zip"), 20),
        (PathBuf::from("mushisam.zip"), 21),
        (PathBuf::from("Raiden.sfc"), 22),
    ] {
        fs::write(game.join(&rel), &binary).unwrap();
        pin_mtime(&game.join(&rel), n);
    }
    let db = SystemDb::builtin();
    let profiles = builtin_profiles();
    let cache = Cache::in_memory().unwrap();
    for source in [game.clone(), game.join("data")] {
        for layout in ["esde", "batocera"] {
            let probe = CountingProbe::default();
            let env = OrganizeEnv {
                db: &db,
                profiles: &profiles,
                cache: &cache,
                dats: None,
                probe: &probe,
                online: None,
                titles: None,
                now: SystemTime::now(),
            };
            let opts = OrganizeOptions {
                layout: layout.into(),
                offline: true,
                target: Some(t.path().to_path_buf()),
                ..OrganizeOptions::default()
            };
            let p = plan_organize(&source, &opts, &env, &|_| {}, &AtomicBool::new(false)).unwrap();
            assert!(
                p.items.is_empty(),
                "{layout} {}: {:?}",
                source.display(),
                p.items.iter().map(|i| &i.sources).collect::<Vec<_>>()
            );
            let seen = probe.seen.lock().unwrap().clone();
            assert!(seen.is_empty(), "{layout}: the walk measured {seen:?}");
        }
    }
    // A pile that only holds a folder named like the system is still
    // walked: the rule is about being inside a game, not near one.
    let world = World::new();
    let p = plan(&world, &sys, t.path(), "esde");
    assert!(!p.items.is_empty());
}

/// Every file and folder below `dir`, relative to it, as component lists.
fn rel_tree(dir: &Path) -> std::collections::BTreeSet<Vec<String>> {
    fn walk(base: &Path, dir: &Path, out: &mut std::collections::BTreeSet<Vec<String>>) {
        for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            out.insert(
                path.strip_prefix(base)
                    .unwrap()
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect(),
            );
            if entry.file_type().unwrap().is_dir() {
                walk(base, &path, out);
            }
        }
    }
    let mut out = std::collections::BTreeSet::new();
    walk(dir, dir, &mut out);
    out
}

#[test]
fn organize_round_trip_keeps_cave3rd_and_type_x() {
    use coverly_core::organize::{apply_organize, ApplyEnv, TransferMode};
    use coverly_core::write::trash::FakeTrash;
    use coverly_core::write::undo_job;
    let world = World::new();
    let roms: [(&str, &[u8]); 1] = [("u4", b"mushihime synthetic")];
    world.cache_set(MAME_DAT, "mushisam", &roms);
    world.cache_set(FBNEO_DAT, "mushisam", &roms);
    let tmp = TempDir::new().unwrap();
    let (pile, card, data) = (
        tmp.path().join("pile"),
        tmp.path().join("card"),
        tmp.path().join("data"),
    );
    fs::create_dir_all(&card).unwrap();
    for rel in [
        Path::new("cave3rd").join("mushisam.zip"),
        PathBuf::from("mushisam.zip"),
    ] {
        let path = pile.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        common::roms::zip_file(&path, &[(roms[0].0, roms[0].1, true)]);
        pin_mtime(&path, 1);
    }
    touch(
        &pile
            .join("type-x")
            .join("Raiden IV (Type X)")
            .join("game.exe"),
        2,
    );
    touch(&pile.join("game.exe"), 3);
    let before = rel_tree(&pile);

    let opts = OrganizeOptions {
        layout: "batocera".into(),
        offline: true,
        target: Some(card.clone()),
        mode: Some(TransferMode::Move),
        ..OrganizeOptions::default()
    };
    let p = plan_organize(&pile, &opts, &world.env(), &|_| {}, &AtomicBool::new(false)).unwrap();
    // Dry run: only the loose set moves, and to fbneo.
    let moving: Vec<_> = p
        .items
        .iter()
        .filter(|i| matches!(i.action, OrganizeAction::Auto))
        .collect();
    assert_eq!(moving.len(), 1, "{:?}", p.items);
    assert_eq!(moving[0].sources[0], Path::new("mushisam.zip"));
    assert_eq!(moving[0].system_id.as_deref(), Some("fbneo"));

    let report = apply_organize(
        &p,
        &ApplyEnv {
            data_dir: &data,
            progress: &|_| {},
            cancel: &AtomicBool::new(false),
            trash: &FakeTrash::unavailable("no-trash"),
            tools: Some(&[]),
        },
    )
    .unwrap();
    let after = rel_tree(&pile);
    let mut expected = before.clone();
    expected.remove(&vec!["mushisam.zip".to_string()]);
    assert_eq!(after, expected, "only the loose set left the pile");
    let landed = rel_tree(&card);
    assert!(
        landed
            .iter()
            .any(|c| c.len() >= 2 && c[c.len() - 2] == "fbneo" && c[c.len() - 1] == "mushisam.zip"),
        "{landed:?}"
    );
    assert!(landed
        .iter()
        .all(|c| !c.iter().any(|n| n == "cave3rd" || n == "type-x")));

    undo_job(&data, report.job.as_deref().unwrap()).unwrap();
    assert_eq!(rel_tree(&pile), before, "undo restores the pile");
    assert!(
        rel_tree(&card)
            .iter()
            .all(|c| !c.iter().any(|n| n.ends_with(".zip"))),
        "undo empties the card"
    );
}
