//! Table-driven checks for the systems of batch 2 (plan P-4): big discs
//! and handhelds with their own format. Each system task appends rows;
//! every check runs over every row. Names are checked against the research
//! listings committed in `tests/fixtures/research` (plan P-5), so a typo
//! fails offline.

use std::collections::{BTreeMap, BTreeSet};

use coverly_core::cache::Cache;
use coverly_core::identify::evidence::Evidence;
use coverly_core::identify::names::parse_name;
use coverly_core::identify::sniff::{sniff, SliceAt};
use coverly_core::net::testing::{MockResponse, MockServer};
use coverly_core::net::{NetConfig, PoliteHttp, UreqHttp};
use coverly_core::profiles::naming::system_folder;
use coverly_core::profiles::{builtin_profiles, Media};
use coverly_core::sources::libretro::{
    encode_segment, parse_apache_dirs, LibretroConfig, LibretroSource,
};
use coverly_core::sources::{ArtQuery, ArtSource, MatchKind};
use coverly_core::systems::{Family, System, SystemDb};

/// (id, family, multi_file)
const SYSTEMS: &[(&str, Family, bool)] = &[
    // Task 7.
    ("gamecube", Family::Console, true),
    ("wii", Family::Console, true),
    // Task 8.
    ("3ds", Family::Handheld, false),
];
/// (profile, id, expected folder, on device): spot checks of spec §6.
const TARGETS: &[(&str, &str, &str, Option<bool>)] = &[
    // Task 7.
    ("esde", "gamecube", "gc", None),
    ("batocera", "gamecube", "gamecube", None),
    ("esde", "wii", "wii", None),
    ("batocera", "wii", "wii", None),
    ("muos", "wii", "wii", Some(false)),
    ("onion", "gamecube", "GAMECUBE", Some(false)),
    ("anbernic-stock", "wii", "WII", Some(false)),
    // Task 8.
    ("esde", "3ds", "n3ds", None),
    ("muos", "3ds", "3ds", None),
    ("batocera", "3ds", "3ds", None),
    ("minui", "3ds", "3DS", Some(false)),
    ("onion", "3ds", "3DS", Some(false)),
];
/// Builds the synthetic head of a header fixture.
type Head = fn() -> Vec<u8>;
/// (id, ext, head builder, expected system or None): header rules that a
/// head alone decides (spec §8).
const HEADERS: &[(&str, &str, Head, Option<&str>)] = &[];
/// Builds a sparse file whose far offsets matter (spec §8.6).
type Sparse = fn() -> coverly_core::consoles::testing::SparseAt;
/// (id, ext, sparse file, expected system, expected serial): one
/// raw-file header rule of the batch.
type ConsoleHeader = (
    &'static str,
    &'static str,
    Sparse,
    Option<&'static str>,
    Option<&'static str>,
);
/// The batch's raw-file header rules, through `sniff` like a real file
/// (plan P-1).
const CONSOLE_HEADERS: &[ConsoleHeader] = &[
    // Task 7: GameCube and Wii through every container, and WiiWare.
    ("gamecube", "iso", gc_iso, Some("gamecube"), Some("GM8E01")),
    ("wii", "iso", wii_iso, Some("wii"), Some("RMGE01")),
    ("wii", "rvz", wii_rvz, Some("wii"), Some("RMGE01")),
    ("gamecube", "wia", gc_wia, Some("gamecube"), Some("GM8E01")),
    ("gamecube", "gcz", gc_gcz, Some("gamecube"), Some("GM8E01")),
    ("wii", "ciso", wii_ciso, Some("wii"), Some("RMGE01")),
    ("wii", "wbfs", wii_wbfs, Some("wii"), Some("RMGE01")),
    ("gamecube", "tgc", gc_tgc, Some("gamecube"), Some("GM8E01")),
    ("wii", "wad", wiiware, Some("wii"), Some("WAAE")),
    // A PSP CSO shares the `CISO` magic and still answers as before.
    ("psp", "cso", psp_cso, Some("psp"), None),
    // Task 8: 3DS cartridges, NCCH, CIA, homebrew and Z3DS.
    ("3ds", "3ds", ctr_3ds, Some("3ds"), Some("CTR-P-AMKP")),
    ("3ds", "cci", ctr_cci, Some("3ds"), Some("CTR-P-AMKP")),
    ("3ds", "cxi", ctr_cxi, Some("3ds"), Some("CTR-P-BP4J")),
    ("3ds", "cia", ctr_cia, Some("3ds"), Some("CTR-P-EKJA")),
    ("3ds", "cia", ctr_cia_encrypted, Some("3ds"), None),
    ("3ds", "3dsx", ctr_3dsx, Some("3ds"), None),
    ("3ds", "zcci", ctr_zcci, Some("3ds"), Some("CTR-P-AMKP")),
];

mod gcwii {
    use coverly_core::consoles::testing::{self as t, SparseAt};

    pub fn gc() -> Vec<u8> {
        t::gc_header("GM8E01", false)
    }
    pub fn wii() -> Vec<u8> {
        t::gc_header("RMGE01", true)
    }
    pub fn gc_iso() -> SparseAt {
        t::gc_disc("GM8E01", false)
    }
    pub fn wii_iso() -> SparseAt {
        t::gc_disc("RMGE01", true)
    }
    pub fn wii_rvz() -> SparseAt {
        t::wia(&wii(), true, 2)
    }
    pub fn gc_wia() -> SparseAt {
        t::wia(&gc(), false, 1)
    }
    pub fn gc_gcz() -> SparseAt {
        t::gcz(&gc(), false)
    }
    pub fn wii_ciso() -> SparseAt {
        t::ciso(&wii())
    }
    pub fn wii_wbfs() -> SparseAt {
        t::wbfs(&wii())
    }
    pub fn gc_tgc() -> SparseAt {
        t::tgc(&gc())
    }
    pub fn wiiware() -> SparseAt {
        t::wii_wad(0x0001_0001_5741_4145)
    }
    pub fn psp_cso() -> SparseAt {
        t::psp_cso()
    }
}
use gcwii::*;

mod ctr {
    use coverly_core::consoles::testing::{self as t, SparseAt};

    pub fn ctr_3ds() -> SparseAt {
        t::ncsd("CTR-P-AMKP", 0x0004_0000_0003_0800, false)
    }
    pub fn ctr_cci() -> SparseAt {
        t::ncsd("CTR-P-AMKP", 0x0004_0000_0003_0800, true)
    }
    pub fn ctr_cxi() -> SparseAt {
        t::ncch("CTR-P-BP4J", 0x0004_0000_0003_0800, false)
    }
    pub fn ctr_cia() -> SparseAt {
        t::cia(0x0004_0000_0005_5D00, false, true)
    }
    pub fn ctr_cia_encrypted() -> SparseAt {
        t::cia(0x0004_0000_0005_5D00, true, true)
    }
    pub fn ctr_3dsx() -> SparseAt {
        t::dsx_with_smdh("Homebrew Launcher")
    }
    pub fn ctr_zcci() -> SparseAt {
        t::z3ds(b"NCSD", &ctr_3ds())
    }
}
use ctr::*;

fn research(name: &str) -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/research")
            .join(name),
    )
    .unwrap()
}

/// The strongest hit; on a tie the first listed (the likelier).
fn best(hits: &[Evidence]) -> Option<&str> {
    hits.iter()
        .fold(None, |best: Option<&Evidence>, h| match best {
            Some(b) if b.weight >= h.weight => Some(b),
            _ => Some(h),
        })
        .map(|h| h.system.as_str())
}

fn http() -> PoliteHttp<UreqHttp> {
    let cfg = NetConfig {
        retries: 0,
        ..NetConfig::default()
    };
    PoliteHttp::new(UreqHttp::new(&cfg), &cfg)
}

fn cover_by_name(
    src: &LibretroSource<'_>,
    system: &System,
    stem: &str,
) -> Vec<coverly_core::sources::Candidate> {
    let rom = parse_name(stem, false);
    src.candidates(&ArtQuery {
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
    .unwrap()
}

#[test]
fn research_listings_are_complete() {
    let dirs = parse_apache_dirs(&research("thumbnails-root.html"));
    assert!(dirs.len() >= 120, "{} thumbnail folders", dirs.len());
    let dats = research("libretro-database-dats.txt");
    assert!(dats.lines().any(|l| l.starts_with("metadat/redump/")));
    assert!(dats.lines().any(|l| l.starts_with("metadat/no-intro/")));
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

/// Every `libretro_also` folder exists on the thumbnail server too.
#[test]
fn libretro_also_names_exist_on_the_thumbnail_server() {
    let dirs: BTreeSet<String> = parse_apache_dirs(&research("thumbnails-root.html"))
        .into_iter()
        .collect();
    for s in SystemDb::builtin().all() {
        for name in &s.libretro_also {
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
        assert_eq!(best(&s.hits), *want, "{id} .{ext}");
    }
}

/// The batch's raw-file header rules go through `sniff` like a real file
/// (P-1): the strongest hit names the system and the serial is read.
#[test]
fn console_header_rules() {
    for (id, ext, file, want, serial) in CONSOLE_HEADERS {
        let mut f = file();
        let len = f.len;
        let s = sniff(&mut f, len, ext);
        assert_eq!(best(&s.hits), *want, "{id} .{ext}");
        assert_eq!(s.serial.as_deref(), *serial, "{id} .{ext} serial");
    }
}

/// Each batch system with a libretro folder finds a cover by file name;
/// one with `libretro_also` folders also finds a cover that only an extra
/// folder has (spec D9).
#[test]
fn cover_lookup_finds_batch_systems() {
    let db = SystemDb::builtin();
    let http = http();
    for system in SYSTEMS.iter().filter_map(|(id, _, _)| db.get(id)) {
        let Some(name) = system.libretro.clone() else {
            continue;
        };
        let server = MockServer::start();
        server.mock("/", MockResponse::ok(research("thumbnails-root.html")));
        server.serve_libretro_index(&name, "Named_Boxarts", &["Test Game (World).png"]);
        for also in &system.libretro_also {
            server.serve_libretro_index(also, "Named_Boxarts", &["Only There (World).png"]);
        }
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
        let c = cover_by_name(&src, system, stem);
        assert_eq!(
            (c[0].kind, c[0].name.as_str()),
            (MatchKind::NameExact, stem),
            "{}",
            system.id
        );
        if let Some(also) = system.libretro_also.first() {
            let stem = "Only There (World)";
            let c = cover_by_name(&src, system, stem);
            assert_eq!(
                (c[0].kind, c[0].name.as_str()),
                (MatchKind::NameExact, stem),
                "{} via {also}",
                system.id
            );
            assert!(
                c[0].url.contains(&encode_segment(also)),
                "{}: {}",
                system.id,
                c[0].url
            );
        }
    }
}
