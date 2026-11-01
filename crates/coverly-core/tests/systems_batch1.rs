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
    // Computers II (Task 9).
    ("x68000", Family::Computer, false),
    ("x1", Family::Computer, false),
    ("pc98", Family::Computer, false),
    ("pc88", Family::Computer, false),
    // Computers III (Task 10).
    ("plus4", Family::Computer, false),
    ("pet", Family::Computer, false),
    ("c128", Family::Computer, false),
    ("atarixe", Family::Computer, false),
    ("msx2", Family::Computer, false),
    ("msxturbor", Family::Computer, false),
    ("apple2", Family::Computer, false),
    // Fantasy consoles (Task 11).
    ("tic80", Family::Engine, false),
    ("wasm4", Family::Engine, false),
    ("arduboy", Family::Handheld, false),
    ("uzebox", Family::Console, false),
    ("lowresnx", Family::Engine, false),
    ("vircon32", Family::Engine, false),
    // Ports and engines (Task 12).
    ("doom", Family::Engine, false),
    ("quake", Family::Engine, false),
    ("cavestory", Family::Engine, false),
    ("wolf3d", Family::Engine, false),
    ("mrboom", Family::Engine, false),
    ("dinothawr", Family::Engine, false),
    ("lutro", Family::Engine, false),
    ("solarus", Family::Engine, false),
    ("flashback", Family::Engine, false),
    ("xrick", Family::Engine, false),
    // Extra arcade (Task 13).
    ("model2", Family::Arcade, false),
    ("model3", Family::Arcade, false),
    ("naomi2", Family::Arcade, false),
    ("triforce", Family::Arcade, false),
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
    // Computers II (Task 9).
    ("muos", "pc88", "pc8000", None),
    ("esde", "pc88", "pc88", None),
    ("esde", "x68000", "x68000", None),
    ("batocera", "x1", "x1", None),
    ("muos", "pc98", "pc98", None),
    ("onion", "x68000", "X68000", Some(false)),
    ("minui", "pc98", "PC98", Some(false)),
    // Computers III (Task 10); the D23 hosts are in `profile_folders.rs`.
    ("batocera", "atarixe", "xegs", None),
    ("esde", "atarixe", "atarixe", None),
    ("batocera", "plus4", "cplus4", None),
    ("esde", "plus4", "plus4", None),
    ("muos", "pet", "pet", None),
    ("minui", "plus4", "PLUS4", None),
    ("minui", "pet", "PET", None),
    ("minui", "c128", "C128", None),
    ("esde", "c128", "c128", Some(false)),
    ("batocera", "msx2", "msx2", None),
    ("muos", "msxturbor", "msxturbor", Some(false)),
    ("esde", "msxturbor", "msxturbor", None),
    ("batocera", "apple2", "apple2", None),
    ("onion", "apple2", "APPLE2", Some(false)),
    // Fantasy consoles (Task 11).
    ("onion", "tic80", "TIC", None),
    ("esde", "wasm4", "wasm4", None),
    ("minui", "tic80", "TIC", Some(false)),
    ("batocera", "arduboy", "arduboy", None),
    ("muos", "uzebox", "uzebox", None),
    ("esde", "lowresnx", "lowresnx", None),
    ("batocera", "vircon32", "vircon32", None),
    ("onion", "vircon32", "VIRCON32", Some(false)),
    // Ports and engines (Task 12).
    ("batocera", "doom", "prboom", None),
    ("minui", "doom", "PRBOOM", None),
    ("batocera", "wolf3d", "ecwolf", None),
    ("muos", "wolf3d", "wolfenstein3d", None),
    ("batocera", "flashback", "reminiscence", None),
    ("esde", "dinothawr", "dinothawr", Some(false)),
    // Extra arcade (Task 13).
    ("esde", "model2", "model2", None),
    ("batocera", "triforce", "triforce", None),
    ("onion", "naomi2", "NAOMI2", Some(false)),
    ("esde", "model3", "model3", None),
    ("batocera", "naomi2", "naomi2", None),
    ("muos", "model2", "model2", Some(false)),
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
    // Computers II (Task 9) and the `.hdf`/`.fdi`/`.dim` rules (P-2).
    ("pc98", "nhd", fx::nhd, Some("pc98")),
    ("x68000", "hdf", fx::x68k_scsi, Some("x68000")),
    ("amiga", "hdf", fx::amiga_rdb, Some("amiga")),
    ("amiga", "hdf", fx::amiga_dos, Some("amiga")),
    ("amiga", "fdi", fx::fdi2, Some("amiga")),
    ("atarist", "dim", fx::st_dim, Some("atarist")),
    // Computers III (Task 10). The `.prg` load address is a 0.40
    // tie-break only.
    ("apple2", "woz", || fx::woz(2), Some("apple2")),
    ("apple2", "woz", || fx::woz(1), Some("apple2")),
    ("apple2", "2mg", fx::two_img, Some("apple2")),
    ("plus4", "prg", || fx::prg([0x01, 0x10]), Some("plus4")),
    // Fantasy consoles (Task 11). `\0asm` only proves WebAssembly.
    ("uzebox", "uze", fx::uzebox, Some("uzebox")),
    ("vircon32", "v32", fx::vircon32, Some("vircon32")),
    ("wasm4", "wasm", fx::wasm, None),
    // Ports and engines (Task 12). `PACK` is OpenBOR's magic too.
    ("doom", "wad", || fx::wad(b"IWAD"), Some("doom")),
    ("doom", "wad", || fx::wad(b"PWAD"), Some("doom")),
    ("quake", "pak", fx::pack, None),
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

/// A loose disc image (`.iso`, or a cue set's `.bin` track) of this
/// batch's disc systems is told by the same disc signatures an archived
/// set goes through; a loose PC Engine CD or Neo Geo CD, and any other
/// extension, keep their earlier result (no hit).
#[test]
fn loose_disc_images_are_told_by_their_signature() {
    use coverly_core::disc::testing::{self as dt, AmigaTm};
    let cases: Vec<(&str, Vec<u8>, &str, Option<&str>)> = vec![
        ("3do iso", dt::opera_3do(false), "iso", Some("3do")),
        ("3do raw bin", dt::opera_3do(true), "bin", Some("3do")),
        ("pcfx bin", dt::pcfx_track(true), "bin", Some("pcfx")),
        ("cdi bin", dt::cdi_track(1), "bin", Some("cdi")),
        (
            "cd32 iso",
            dt::amiga_cd("CDTV", AmigaTm::Cd32Block),
            "iso",
            Some("amigacd32"),
        ),
        (
            "cdtv iso",
            dt::amiga_cd("CDTV", AmigaTm::CdtvFile),
            "iso",
            Some("cdtv"),
        ),
        (
            "amiga cd without a mark",
            dt::amiga_cd("CDTV", AmigaTm::None),
            "iso",
            None,
        ),
        ("pce cd bin", dt::pce_cd_track(), "bin", None),
        ("neocd iso", dt::neocd_iso(), "iso", None),
        ("3do under .md", dt::opera_3do(false), "md", None),
        ("ps2 iso", dt::ps2_iso(), "iso", Some("ps2")),
    ];
    for (label, bytes, ext, want) in cases {
        let s = sniff(&mut SliceAt(&bytes), bytes.len() as u64, ext);
        let got: Vec<&str> = s.hits.iter().map(|h| h.system.as_str()).collect();
        assert_eq!(got, want.into_iter().collect::<Vec<_>>(), "{label}");
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
            rom_path: None,
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

#[test]
fn computers_ii_catalogues() {
    // Spec §7: libretro-thumbnails folders, DATs and ScreenScraper ids.
    let db = SystemDb::builtin();
    let dats = |id: &str| coverly_core::identify::dat::dat_sources(db.get(id).unwrap());
    assert_eq!(
        dats("x68000"),
        [
            "metadat/no-intro/Sharp - X68000.dat",
            "metadat/tosec/Sharp - X68000.dat",
        ]
    );
    assert_eq!(
        dats("x1"),
        [
            "metadat/no-intro/Sharp - X1.dat",
            "metadat/tosec/Sharp - X1.dat"
        ]
    );
    assert_eq!(
        dats("pc98"),
        [
            "metadat/tosec/NEC - PC-98.dat",
            "metadat/redump/NEC - PC-98.dat"
        ]
    );
    assert_eq!(dats("pc88"), ["metadat/tosec/NEC - PC-88.dat"]);
    for (id, libretro, ss) in [
        ("x68000", "Sharp - X68000", 79),
        ("x1", "Sharp - X1", 220),
        ("pc98", "NEC - PC-98", 208),
        ("pc88", "NEC - PC-88", 221),
    ] {
        let s = db.get(id).unwrap();
        assert_eq!(s.libretro.as_deref(), Some(libretro), "{id}");
        assert_eq!(s.screenscraper, Some(ss), "{id}");
    }
}

#[test]
fn hdf_fdi_dim_d88_2hd_hdm_are_shared() {
    // D3: `.hdf`, `.fdi`, `.dim` turn Shared on purpose; `.d88`, `.2hd`,
    // `.hdm` are new and Shared. `.2d` is a weak X1 extension.
    use coverly_core::systems::ExtClass;
    let db = SystemDb::builtin();
    for ext in ["hdf", "fdi", "dim", "d88", "2hd", "hdm"] {
        assert_eq!(db.extension_class(ext), ExtClass::Shared, "{ext}");
    }
    for ext in ["xdf", "dx1", "hdi", "nhd", "t88", "cmt"] {
        assert_eq!(db.extension_class(ext), ExtClass::Unique, "{ext}");
    }
    assert_eq!(db.extension_class("2d"), ExtClass::Unknown);
}

#[test]
fn msx1_still_msx_and_msx2_moves() {
    // D6: the `msx2` alias moves from `msx` to the new system; `msx1`
    // (ES-DE's and Batocera's MSX folder) and `MSX` stay MSX.
    let db = SystemDb::builtin();
    for (name, want) in [
        ("msx1", "msx"),
        ("MSX", "msx"),
        ("MSX2", "msx2"),
        ("msx2", "msx2"),
        ("msxturbor", "msxturbor"),
        ("xegs", "atarixe"),
        ("A800", "a800"),
        ("cplus4", "plus4"),
        ("c128", "c128"),
        ("appleii", "apple2"),
    ] {
        assert_eq!(
            db.resolve_folder(name).map(|s| s.id.as_str()),
            Some(want),
            "{name}"
        );
    }
    // `turbor` is scoped (D22).
    assert!(db.resolve_folder("turbor").is_none());
}

#[test]
fn computers_iii_catalogues() {
    let db = SystemDb::builtin();
    let dats = |id: &str| coverly_core::identify::dat::dat_sources(db.get(id).unwrap());
    assert_eq!(dats("plus4"), ["metadat/no-intro/Commodore - Plus-4.dat"]);
    assert_eq!(dats("pet"), ["metadat/tosec/Commodore - PET.dat"]);
    assert_eq!(dats("msx2"), ["metadat/no-intro/Microsoft - MSX2.dat"]);
    assert_eq!(dats("apple2"), ["metadat/tosec/Apple - II.dat"]);
    assert!(dats("c128").is_empty());
    assert!(dats("msxturbor").is_empty());
    for (id, libretro, ss) in [
        ("plus4", Some("Commodore - Plus-4"), Some(99)),
        ("pet", Some("Commodore - PET"), None),
        ("c128", None, None),
        ("atarixe", Some("Atari - 8-bit"), Some(43)),
        ("msx2", Some("Microsoft - MSX2"), Some(116)),
        ("msxturbor", None, Some(118)),
        ("apple2", None, Some(86)),
    ] {
        let s = db.get(id).unwrap();
        assert_eq!(
            (s.libretro.as_deref(), s.screenscraper),
            (libretro, ss),
            "{id}"
        );
    }
}

#[test]
fn computers_iii_extension_classes() {
    // D3: `.dsk` stays Shared and gains msx2, msxturbor, apple2; the
    // Commodore and Atari formats stay their old owner's (weak only).
    use coverly_core::systems::ExtClass;
    let db = SystemDb::builtin();
    let owners = |ext: &str| {
        let mut v: Vec<String> = db
            .systems_for_extension(ext)
            .into_iter()
            .map(|s| s.id.clone())
            .collect();
        v.sort();
        v
    };
    assert_eq!(db.extension_class("dsk"), ExtClass::Shared);
    for id in ["msx2", "msxturbor", "apple2"] {
        assert!(owners("dsk").contains(&id.to_string()), "{id}");
    }
    for ext in ["prg", "d64", "t64", "crt"] {
        assert_eq!(db.extension_class(ext), ExtClass::Shared, "{ext}");
        assert_eq!(owners(ext), ["c64", "vic20"], "{ext}");
    }
    for (ext, owner) in [
        ("d71", "c64"),
        ("d81", "c64"),
        ("atr", "a800"),
        ("xex", "a800"),
        ("mx2", "msx"),
    ] {
        assert_eq!(db.extension_class(ext), ExtClass::Unique, "{ext}");
        assert_eq!(owners(ext), [owner], "{ext}");
    }
    for ext in ["woz", "2mg", "nib"] {
        assert_eq!(owners(ext), ["apple2"], "{ext}");
    }
    assert_eq!(db.extension_class("do"), ExtClass::Unknown);
}

#[test]
fn fantasy_consoles_catalogues() {
    let db = SystemDb::builtin();
    let dats = |id: &str| coverly_core::identify::dat::dat_sources(db.get(id).unwrap());
    assert_eq!(dats("tic80"), ["dat/TIC-80.dat"]);
    assert_eq!(dats("wasm4"), ["dat/WASM-4.dat"]);
    assert_eq!(
        dats("arduboy"),
        ["metadat/no-intro/Arduboy Inc - Arduboy.dat"]
    );
    assert_eq!(dats("uzebox"), ["dat/Uzebox.dat"]);
    assert_eq!(dats("lowresnx"), ["dat/LowRes NX.dat"]);
    assert_eq!(dats("vircon32"), ["dat/Vircon32.dat"]);
    for (id, libretro, ss) in [
        ("tic80", Some("TIC-80"), 222),
        ("wasm4", Some("WASM-4"), 262),
        ("arduboy", Some("Arduboy Inc - Arduboy"), 263),
        ("uzebox", None, 216),
        ("lowresnx", Some("LowRes NX"), 244),
        ("vircon32", Some("Vircon32"), 272),
    ] {
        let s = db.get(id).unwrap();
        assert_eq!(
            (s.libretro.as_deref(), s.screenscraper),
            (libretro, Some(ss)),
            "{id}"
        );
    }
}

#[test]
fn fantasy_consoles_extension_classes() {
    // D4: `.wasm` and `.hex` are weak only (`.hex` in two systems), so
    // neither is a class; `.tic`, `.uze`, `.nx`, `.v32`, `.arduboy` are
    // their system's own.
    use coverly_core::systems::ExtClass;
    let db = SystemDb::builtin();
    for ext in ["wasm", "hex"] {
        assert_eq!(db.extension_class(ext), ExtClass::Unknown, "{ext}");
    }
    for (ext, owner) in [
        ("tic", "tic80"),
        ("uze", "uzebox"),
        ("nx", "lowresnx"),
        ("v32", "vircon32"),
        ("arduboy", "arduboy"),
    ] {
        assert_eq!(db.extension_class(ext), ExtClass::Unique, "{ext}");
        let owners: Vec<String> = db
            .systems_for_extension(ext)
            .into_iter()
            .map(|s| s.id.clone())
            .collect();
        assert_eq!(owners, [owner], "{ext}");
    }
    for id in ["arduboy", "uzebox"] {
        assert!(db.get(id).unwrap().accepts_extension("hex"), "{id}");
    }
    assert!(db.get("wasm4").unwrap().accepts_extension("wasm"));
    // `TIC` is TIC-80's folder everywhere; the Thomson keeps its own.
    assert_eq!(
        db.resolve_folder("TIC").map(|s| s.id.as_str()),
        Some("tic80")
    );
    assert_eq!(
        db.resolve_folder("thomson").map(|s| s.id.as_str()),
        Some("thomson")
    );
}

#[test]
fn ports_and_engines_catalogues() {
    // Spec §7: libretro-database `dat/` DATs and ScreenScraper ids (Doom
    // and Quake share 135, PC ports; Solarus has neither a thumbnail
    // folder nor a DAT).
    let db = SystemDb::builtin();
    for (id, libretro, dat, ss) in [
        ("doom", Some("DOOM"), Some("dat/DOOM.dat"), Some(135)),
        ("quake", Some("Quake"), Some("dat/Quake.dat"), Some(135)),
        (
            "cavestory",
            Some("Cave Story"),
            Some("dat/Cave Story.dat"),
            None,
        ),
        (
            "wolf3d",
            Some("Wolfenstein 3D"),
            Some("dat/Wolfenstein 3D.dat"),
            None,
        ),
        ("mrboom", Some("MrBoom"), Some("dat/MrBoom.dat"), None),
        (
            "dinothawr",
            Some("Dinothawr"),
            Some("dat/Dinothawr.dat"),
            None,
        ),
        ("lutro", Some("Lutro"), Some("dat/Lutro.dat"), Some(206)),
        ("solarus", None, None, Some(223)),
        (
            "flashback",
            Some("Flashback"),
            Some("dat/Flashback.dat"),
            None,
        ),
        (
            "xrick",
            Some("Rick Dangerous"),
            Some("dat/Rick Dangerous.dat"),
            None,
        ),
    ] {
        let s = db.get(id).unwrap();
        assert_eq!(s.libretro.as_deref(), libretro, "{id}");
        assert_eq!(s.screenscraper, ss, "{id}");
        let dats: Vec<String> = coverly_core::identify::dat::dat_sources(s);
        assert_eq!(
            dats,
            dat.map(String::from).into_iter().collect::<Vec<_>>(),
            "{id}"
        );
    }
}

#[test]
fn ports_and_engines_extension_classes() {
    // D15/D4: `.wad`, `.lutro`, `.solarus` are unique; `.pak` stays
    // OpenBOR's (weak for Quake), `.game` and `.lua` are weak only.
    use coverly_core::systems::ExtClass;
    let db = SystemDb::builtin();
    for (ext, owner) in [("wad", "doom"), ("lutro", "lutro"), ("solarus", "solarus")] {
        assert_eq!(db.extension_class(ext), ExtClass::Unique, "{ext}");
        assert_eq!(
            db.systems_for_extension(ext)
                .iter()
                .map(|s| s.id.as_str())
                .collect::<Vec<_>>(),
            [owner],
            "{ext}"
        );
    }
    assert_eq!(
        db.systems_for_extension("pak")
            .iter()
            .map(|s| s.id.as_str())
            .collect::<Vec<_>>(),
        ["openbor"]
    );
    for ext in ["game", "lua"] {
        assert_eq!(db.extension_class(ext), ExtClass::Unknown, "{ext}");
    }
}

#[test]
fn extra_arcade_catalogues() {
    // Spec §7 / D16: Model 2 and Model 3 are MAME sets, so they share the
    // MAME thumbnail folder and DAT; NAOMI 2 has its own folder and Redump
    // DAT; Triforce has neither (folder only). Every row is arcade.
    let db = SystemDb::builtin();
    for (id, libretro, ss) in [
        ("model2", Some("MAME"), Some(75)),
        ("model3", Some("MAME"), Some(75)),
        ("naomi2", Some("Sega - Naomi 2"), None),
        ("triforce", None, None),
    ] {
        let s = db.get(id).unwrap();
        assert!(s.arcade, "{id}");
        assert_eq!(
            (s.libretro.as_deref(), s.screenscraper),
            (libretro, ss),
            "{id}"
        );
    }
    // `.iso` stays Generic: Triforce only accepts it inside its folder.
    use coverly_core::systems::ExtClass;
    assert_eq!(db.extension_class("iso"), ExtClass::Generic);
    assert!(db.get("triforce").unwrap().accepts_extension("iso"));
    // No new system for System 16/18/24/32, Konami or Cave (D16).
    for id in [
        "system16", "system18", "system24", "system32", "konami", "cave",
    ] {
        assert!(db.get(id).is_none(), "{id}");
    }
}

#[test]
fn model2_and_model3_covers_use_the_mame_folder() {
    // §12: two systems with the same libretro folder look covers up there.
    let db = SystemDb::builtin();
    let server = MockServer::start();
    server.mock("/", MockResponse::ok(research("thumbnails-root.html")));
    server.serve_libretro_index("MAME", "Named_Boxarts", &["Daytona USA (Japan).png"]);
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
    for id in ["model2", "model3"] {
        let system = db.get(id).unwrap();
        assert_eq!(src.server_system(system).unwrap(), "MAME", "{id}");
        let stem = "Daytona USA (Japan)";
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
                rom_path: None,
            })
            .unwrap();
        assert_eq!(
            (c[0].kind, c[0].name.as_str()),
            (MatchKind::NameExact, stem),
            "{id}"
        );
    }
}
