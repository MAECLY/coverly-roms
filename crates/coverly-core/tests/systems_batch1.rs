//! Table-driven checks for the systems of batch 1 (plan P-4). Each family
//! task appends rows; every check runs over every row. Names are checked
//! against the research listings committed in `tests/fixtures/research`
//! (plan P-5), so a typo fails offline.

mod common;

use std::collections::{BTreeMap, BTreeSet};

use coverly_core::cache::Cache;
use coverly_core::identify::names::parse_name;
use coverly_core::identify::sniff::{fixtures as fx, sniff, SliceAt};
use coverly_core::net::testing::{MockResponse, MockServer};
use coverly_core::net::{NetConfig, PoliteHttp, UreqHttp};
use coverly_core::profiles::naming::system_folder;
use coverly_core::profiles::{builtin_profiles, Media};
use coverly_core::sources::libretro::{parse_apache_dirs, LibretroConfig, LibretroSource};
use coverly_core::sources::{ArtQuery, ArtSource, MatchKind};
use coverly_core::systems::{Family, SystemDb};

/// (id, family, multi_file)
const SYSTEMS: &[(&str, Family, bool)] = &[
    // Disc family (Task 4).
    ("ps2", Family::Console, true),
    ("3do", Family::Console, true),
    ("pcfx", Family::Console, true),
    ("cdi", Family::Console, true),
    ("amigacd32", Family::Console, true),
    ("cdtv", Family::Console, true),
    ("jaguarcd", Family::Console, true),
    ("fmtowns", Family::Computer, true),
    // Early consoles (Task 5).
    ("colecovision", Family::Console, false),
    ("intellivision", Family::Console, false),
    ("vectrex", Family::Console, false),
    ("odyssey2", Family::Console, false),
    ("channelf", Family::Console, false),
    ("arcadia", Family::Console, false),
    ("scv", Family::Console, false),
    ("jaguar", Family::Console, false),
    // Sega, NEC and Nintendo add-ons (Task 6).
    ("sg1000", Family::Console, false),
    ("pico", Family::Console, false),
    ("supergrafx", Family::Console, false),
    ("n64dd", Family::Console, false),
    ("satellaview", Family::Console, false),
    ("sufami", Family::Console, false),
    // Handhelds (Task 7).
    ("supervision", Family::Handheld, false),
    ("gamecom", Family::Handheld, false),
    ("megaduck", Family::Handheld, false),
    // Computers I (Task 8).
    ("zxspectrum", Family::Computer, false),
    ("zx81", Family::Computer, false),
    ("amstradcpc", Family::Computer, false),
    ("bbcmicro", Family::Computer, false),
    ("thomson", Family::Computer, false),
    ("dragon", Family::Computer, false),
    ("coco", Family::Computer, false),
    ("ti99", Family::Computer, false),
];
/// (profile, id, expected folder, on device): spot checks of spec §6.1.
const TARGETS: &[(&str, &str, &str, Option<bool>)] = &[
    // Disc family (Task 4).
    ("esde", "ps2", "ps2", None),
    ("onion", "ps2", "PS2", Some(false)),
    ("anbernic-stock", "ps2", "PS2", Some(false)),
    ("batocera", "cdtv", "amigacdtv", None),
    ("esde", "cdi", "cdimono1", None),
    ("muos", "3do", "3do", None),
    ("minui", "pcfx", "PCFX", Some(false)),
    // Early consoles (Task 5).
    ("esde", "jaguar", "atarijaguar", None),
    ("onion", "colecovision", "COLECO", None),
    ("minui", "colecovision", "COLECO", None),
    ("onion", "channelf", "FAIRCHILD", None),
    ("onion", "odyssey2", "ODYSSEY", None),
    ("batocera", "odyssey2", "odyssey2", None),
    ("muos", "scv", "scv", None),
    ("onion", "arcadia", "ARCADIA", Some(false)),
    // Sega, NEC and Nintendo add-ons (Task 6).
    ("esde", "sg1000", "sg-1000", None),
    ("onion", "sg1000", "SEGASGONE", None),
    ("minui", "sg1000", "SG1000", None),
    ("batocera", "pico", "pico", None),
    ("muos", "pico", "segapico", None),
    ("esde", "pico", "SEGAPICO", Some(false)),
    ("onion", "supergrafx", "SGFX", None),
    ("anbernic-stock", "supergrafx", "PCE", Some(true)),
    ("minui", "satellaview", "SFC", None),
    ("minui", "supergrafx", "SGFX", Some(false)),
    ("onion", "sufami", "SUFAMI", None),
    ("esde", "n64dd", "n64dd", None),
    // Handhelds (Task 7).
    ("onion", "megaduck", "MEGADUCK", None),
    ("onion", "supervision", "SUPERVISION", None),
    ("muos", "gamecom", "gamecom", Some(false)),
    ("esde", "gamecom", "gamecom", None),
    ("batocera", "megaduck", "megaduck", None),
    ("minui", "megaduck", "MEGADUCK", Some(false)),
    ("onion", "gamecom", "GAMECOM", Some(false)),
    ("garlicos", "supervision", "SUPERVISION", None),
    ("anbernic-stock", "megaduck", "MEGADUCK", Some(false)),
    ("retroarch", "gamecom", "gamecom", None),
    // Computers I (Task 8).
    ("muos", "zxspectrum", "spectrum", None),
    ("onion", "zxspectrum", "ZXS", None),
    ("onion", "amstradcpc", "CPC", None),
    ("minui", "amstradcpc", "CPC", None),
    ("esde", "thomson", "moto", None),
    ("esde", "dragon", "dragon32", None),
    ("batocera", "dragon", "dragon64", None),
    ("muos", "bbcmicro", "bbcmicro", Some(false)),
    ("onion", "zx81", "ZX81", Some(false)),
    ("onion", "thomson", "THOMSON", Some(false)),
    ("minui", "zxspectrum", "ZXS", Some(false)),
    ("batocera", "bbcmicro", "bbcmicro", None),
    ("esde", "ti99", "ti99", None),
    ("muos", "ti99", "ti99", Some(false)),
    ("muos", "coco", "coco", None),
    ("anbernic-stock", "amstradcpc", "CPC", Some(false)),
];
/// Builds the synthetic head of a header fixture.
type Head = fn() -> Vec<u8>;
/// (id, ext, head builder, expected system or None): header rules (§8.1).
const HEADERS: &[(&str, &str, Head, Option<&str>)] = &[
    // Early consoles (Task 5).
    ("vectrex", "vec", fx::vectrex, Some("vectrex")),
    (
        "colecovision",
        "col",
        || fx::colecovision(false),
        Some("colecovision"),
    ),
    (
        "colecovision",
        "rom",
        || fx::colecovision(true),
        Some("colecovision"),
    ),
    ("jaguar", "abs", fx::jaguar_abs, Some("jaguar")),
    ("jaguar", "cof", fx::jaguar_cof, Some("jaguar")),
    ("jaguar", "j64", fx::jaguar_abs, None),
    // Sega, NEC and Nintendo add-ons (Task 6).
    ("pico", "md", fx::pico, Some("pico")),
    ("sufami", "st", || fx::sufami(false), Some("sufami")),
    // Computers I (Task 8). A CPC DSK names both machines at 0.50, the
    // CPC first.
    ("zxspectrum", "tzx", fx::tzx, Some("zxspectrum")),
    ("amstradcpc", "cdt", fx::tzx, Some("amstradcpc")),
    (
        "amstradcpc",
        "dsk",
        || fx::cpc_dsk(false),
        Some("amstradcpc"),
    ),
    (
        "amstradcpc",
        "dsk",
        || fx::cpc_dsk(true),
        Some("amstradcpc"),
    ),
    ("amstradcpc", "sna", fx::cpc_sna, Some("amstradcpc")),
    ("bbcmicro", "uef", fx::uef, Some("bbcmicro")),
];

fn research(name: &str) -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/research")
            .join(name),
    )
    .unwrap()
}

#[test]
fn research_listings_are_complete() {
    let dirs = parse_apache_dirs(&research("thumbnails-root.html"));
    assert!(dirs.len() >= 120, "{} thumbnail folders", dirs.len());
    let dats = research("libretro-database-dats.txt");
    assert!(dats.lines().any(|l| l.starts_with("metadat/redump/")));
    assert!(dats.lines().any(|l| l.starts_with("metadat/no-intro/")));
    let icons = research("retroarch-assets-monochrome.txt");
    assert!(icons.lines().all(|l| l.starts_with("src/xmb/monochrome/")));
}

#[test]
fn batch_systems_load_with_family_and_disc_flag() {
    let db = SystemDb::builtin();
    for (id, family, multi) in SYSTEMS {
        let s = db.get(id).unwrap_or_else(|| panic!("missing {id}"));
        assert_eq!((s.family, s.multi_file), (*family, *multi), "{id}");
    }
}

#[test]
fn libretro_names_exist_on_the_thumbnail_server() {
    let dirs: BTreeSet<String> = parse_apache_dirs(&research("thumbnails-root.html"))
        .into_iter()
        .collect();
    for s in SystemDb::builtin().all() {
        if let Some(name) = &s.libretro {
            assert!(
                dirs.contains(name),
                "{}: `{name}` not on thumbnails.libretro.com",
                s.id
            );
        }
    }
}

#[test]
fn dat_paths_exist_in_the_pinned_database() {
    let paths: BTreeSet<String> = research("libretro-database-dats.txt")
        .lines()
        .map(String::from)
        .collect();
    let db = SystemDb::builtin();
    for (id, _, _) in SYSTEMS {
        for dat in coverly_core::identify::dat::dat_sources(db.get(id).unwrap()) {
            assert!(paths.contains(&dat), "{id}: {dat}");
        }
    }
}

#[test]
fn organise_targets_match_the_research_table() {
    let db = SystemDb::builtin();
    let ps = builtin_profiles();
    let none = BTreeMap::new();
    for (profile, id, folder, on_device) in TARGETS {
        let p = ps.iter().find(|p| &p.id == profile).unwrap();
        let c = system_folder(p, db.get(id).unwrap(), &db, &none);
        assert_eq!(
            (c.folder.as_str(), c.on_device),
            (*folder, *on_device),
            "{profile} {id}"
        );
    }
}

#[test]
fn header_rules() {
    for (id, ext, head, want) in HEADERS {
        let bytes = head();
        let s = sniff(&mut SliceAt(&bytes), bytes.len() as u64, ext);
        // The strongest hit; on a tie the first listed (the likelier).
        let best = s
            .hits
            .iter()
            .fold(
                None,
                |best: Option<&coverly_core::identify::evidence::Evidence>, h| match best {
                    Some(b) if b.weight >= h.weight => Some(b),
                    _ => Some(h),
                },
            )
            .map(|h| h.system.as_str());
        assert_eq!(best, *want, "{id} .{ext}");
    }
}

#[test]
fn cover_lookup_finds_batch_systems() {
    let db = SystemDb::builtin();
    // Until a family task adds its rows there is nothing to look up.
    let Some(system) = SYSTEMS
        .iter()
        .filter_map(|(id, _, _)| db.get(id))
        .find(|s| s.libretro.is_some())
    else {
        return;
    };
    let name = system.libretro.clone().unwrap();
    let server = MockServer::start();
    server.mock("/", MockResponse::ok(research("thumbnails-root.html")));
    server.serve_libretro_index(&name, "Named_Boxarts", &["Test Game (World).png"]);
    let cfg = NetConfig {
        retries: 0,
        ..NetConfig::default()
    };
    let http = PoliteHttp::new(UreqHttp::new(&cfg), &cfg);
    let cache = Cache::in_memory().unwrap();
    let src = LibretroSource::new(
        &http,
        &cache,
        LibretroConfig {
            base: server.base(),
            raw_base: server.url("/raw"),
            ..LibretroConfig::default()
        },
    );
    assert_eq!(src.server_system(system).unwrap(), name);
    let stem = "Test Game (World)";
    let rom = parse_name(stem, false);
    let c = src
        .candidates(&ArtQuery {
            system,
            media: Media::Cover,
            canonical: None,
            rom: &rom,
            stem,
            by_name: true,
            fuzzy: true,
            min_score: 65,
            max_candidates: 5,
            hash: None,
            size: 0,
            file_name: stem,
        })
        .unwrap();
    assert_eq!(
        (c[0].kind, c[0].name.as_str()),
        (MatchKind::NameExact, stem),
        "{}",
        system.id
    );
}

#[test]
fn cdi_folder_is_scoped_but_cdimono1_is_global() {
    // D22: `cdi` (and so its first alias `CDI`) is scoped, so a `cdi/`
    // folder of DiscJuggler images in Downloads is still organised; under a
    // profile, and as `cdimono1`, the folder is CD-i's.
    let db = SystemDb::builtin();
    for name in ["cdi", "CDI"] {
        assert!(db.resolve_folder(name).is_none(), "{name}");
    }
    let id = |s: Option<&coverly_core::systems::System>| s.map(|s| s.id.clone());
    assert_eq!(id(db.resolve_folder("cdimono1")), Some("cdi".into()));
    for p in builtin_profiles() {
        assert_eq!(
            id(db.resolve_folder_in(Some(&p), "cdi")),
            Some("cdi".into()),
            "{}",
            p.id
        );
    }
}

#[test]
fn knulli_and_esde_odyssey2_aliases_are_global() {
    // D7 / §6.1 notes: Knulli's `o2em` and ES-DE's `videopac` scan as
    // Odyssey² with no profile.
    let db = SystemDb::builtin();
    for name in ["o2em", "videopac", "odyssey2", "ODYSSEY"] {
        assert_eq!(
            db.resolve_folder(name).map(|s| s.id.as_str()),
            Some("odyssey2"),
            "{name}"
        );
    }
}

#[test]
fn arcadia_folder_is_scoped_but_arcadia2001_is_global() {
    // D22: `arcadia` is an everyday word, so only a profile resolves it.
    let db = SystemDb::builtin();
    for name in ["arcadia", "ARCADIA"] {
        assert!(db.resolve_folder(name).is_none(), "{name}");
    }
    assert_eq!(
        db.resolve_folder("arcadia2001").map(|s| s.id.as_str()),
        Some("arcadia")
    );
    for p in builtin_profiles() {
        assert_eq!(
            db.resolve_folder_in(Some(&p), "arcadia")
                .map(|s| s.id.as_str()),
            Some("arcadia"),
            "{}",
            p.id
        );
    }
}

#[test]
fn famicom_and_fc_stay_nes() {
    // D17: Famicom variants are aliases, not entries; BS-X is Satellaview.
    let db = SystemDb::builtin();
    for (name, want) in [
        ("famicom", "nes"),
        ("FC", "nes"),
        ("bsx", "satellaview"),
        ("bs-x", "satellaview"),
    ] {
        assert_eq!(
            db.resolve_folder(name).map(|s| s.id.as_str()),
            Some(want),
            "{name}"
        );
    }
    assert!(db.get("famicom").is_none());
}

#[test]
fn mega_duck_covers_come_from_screenscraper_only() {
    // §7 / §11 step 6: no libretro-thumbnails folder and no DAT, so the
    // only cover source is ScreenScraper (system 90).
    let db = SystemDb::builtin();
    let duck = db.get("megaduck").unwrap();
    assert_eq!(duck.libretro, None);
    assert!(coverly_core::identify::dat::dat_sources(duck).is_empty());
    assert_eq!(duck.screenscraper, Some(90));
    for (id, ss) in [("supervision", 207), ("gamecom", 121)] {
        let s = db.get(id).unwrap();
        assert_eq!(s.screenscraper, Some(ss), "{id}");
        assert_eq!(
            coverly_core::identify::dat::dat_sources(s),
            [format!(
                "metadat/no-intro/{}.dat",
                s.libretro.as_ref().unwrap()
            )],
            "{id}"
        );
    }
}

#[test]
fn computers_i_catalogues() {
    // Spec §7: the TOSEC and `dat/` files are listed explicitly; BBC
    // Micro, Dragon, CoCo and TI-99/4A have no libretro-thumbnails folder
    // and no DAT, so their covers come from ScreenScraper only.
    let db = SystemDb::builtin();
    let dats = |id: &str| coverly_core::identify::dat::dat_sources(db.get(id).unwrap());
    assert_eq!(
        dats("zxspectrum"),
        [
            "metadat/tosec/Sinclair - ZX Spectrum.dat",
            "metadat/no-intro/Sinclair - ZX Spectrum +3.dat",
            "dat/Sinclair - ZX Spectrum.dat",
        ]
    );
    assert_eq!(dats("zx81"), ["dat/Sinclair - ZX 81.dat"]);
    assert_eq!(
        dats("amstradcpc"),
        ["metadat/tosec/Amstrad - CPC.dat", "dat/Amstrad - CPC.dat"]
    );
    assert_eq!(dats("thomson"), ["metadat/tosec/Thomson - MOTO.dat"]);
    for (id, ss) in [
        ("bbcmicro", 37),
        ("dragon", 91),
        ("coco", 144),
        ("ti99", 205),
    ] {
        let s = db.get(id).unwrap();
        assert_eq!(s.libretro, None, "{id}");
        assert!(dats(id).is_empty(), "{id}");
        assert_eq!(s.screenscraper, Some(ss), "{id}");
    }
    for (id, ss) in [
        ("zxspectrum", 76),
        ("zx81", 77),
        ("amstradcpc", 65),
        ("thomson", 141),
    ] {
        assert_eq!(db.get(id).unwrap().screenscraper, Some(ss), "{id}");
    }
}

#[test]
fn dsk_tap_sna_ccc_cas_are_shared() {
    // D3: `.dsk` turns Shared on purpose; `.sna`, `.ccc` are new and
    // Shared; `.tap` and `.cas` already were.
    use coverly_core::systems::ExtClass;
    let db = SystemDb::builtin();
    for ext in ["dsk", "tap", "sna", "ccc", "cas"] {
        assert_eq!(db.extension_class(ext), ExtClass::Shared, "{ext}");
    }
    for ext in ["tzx", "cdt", "uef", "ssd", "rpk", "k7", "81"] {
        assert_eq!(db.extension_class(ext), ExtClass::Unique, "{ext}");
    }
}
