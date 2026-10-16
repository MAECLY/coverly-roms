mod common;

use std::sync::atomic::AtomicBool;

use coverly_core::art::resolve_art;
use coverly_core::cache::Cache;
use coverly_core::gather::{gather, select_targets, GatherOptions, Scope, TargetFilter};
use coverly_core::identify::dat::DatStore;
use coverly_core::identify::{IdMethod, Identifier, IdentifyOptions, MatchMethod};
use coverly_core::net::testing::{HangingServer, MockResponse, MockServer};
use coverly_core::net::{NetConfig, PoliteHttp, UreqHttp};
use coverly_core::profiles::Media;
use coverly_core::progress::NoProgress;
use coverly_core::scan::{scan_library, ScanOptions};
use coverly_core::sources::libretro::{LibretroConfig, LibretroSource};
use coverly_core::sources::{ArtSource, MatchKind};
use coverly_core::systems::SystemDb;

const GBA: &str = "Nintendo - Game Boy Advance";
const GBA_DAT: &str = "metadat/no-intro/Nintendo - Game Boy Advance.dat";

/// Anbernic SD: GBA has 3 numbered ROMs (one with art), MAME one set.
fn library() -> tempfile::TempDir {
    let dir = tempfile::TempDir::new().unwrap();
    let r = dir.path();
    common::mkdir(r, "anbernic");
    common::write(r, "ROMS/GBA/001 Golden Sun.gba", "golden-sun-bytes");
    common::write(r, "ROMS/GBA/002 Goldn Adventure.gba", "unknown-bytes");
    common::write(r, "ROMS/GBA/003 Has Art.gba", "has-art");
    common::write(r, "ROMS/GBA/Imgs/003 Has Art.png", "png");
    common::write(r, "ROMS/MAME/10yard.zip", "not really a zip");
    dir
}

fn dat_text() -> String {
    let crc = format!("{:08X}", crc32fast::hash(b"golden-sun-bytes"));
    format!("clrmamepro (\n name \"GBA\"\n)\ngame (\n name \"Golden Sun (USA, Europe)\"\n rom ( name \"Golden Sun (USA, Europe).gba\" size 16 crc {crc} )\n)\n")
}

fn server() -> MockServer {
    let s = MockServer::start();
    s.serve_libretro_root(&[GBA, "MAME"]);
    s.serve_libretro_index(
        GBA,
        "Named_Boxarts",
        &[
            "Golden Sun (USA, Europe).png",
            "Golden Sun (Japan).png",
            "Golden Adventure (USA).png",
        ],
    );
    s.serve_libretro_index(
        "MAME",
        "Named_Boxarts",
        &["10-Yard Fight (World, set 1).png"],
    );
    s.serve_dat(GBA_DAT, &dat_text());
    s.serve_dat("metadat/mame/MAME.dat", "game (\n name \"10-Yard Fight (World, set 1)\"\n rom ( name 10yard.zip size 1 crc 00000000 )\n)\n");
    s.serve_dat("metadat/fbneo-split/FBNeo - Arcade Games.dat", "");
    s
}

struct Env {
    http: PoliteHttp<UreqHttp>,
    cache: Cache,
}

fn env() -> Env {
    let cfg = NetConfig {
        retries: 0,
        ..NetConfig::default()
    };
    Env {
        http: PoliteHttp::new(UreqHttp::new(&cfg), &cfg),
        cache: Cache::in_memory().unwrap(),
    }
}

#[test]
fn select_targets_filters_system_and_missing_media() {
    let lib = library();
    let profile = common::profile("anbernic-stock");
    let db = SystemDb::builtin();
    let scan = scan_library(lib.path(), &profile, &db, &ScanOptions::default());
    let art = resolve_art(&scan, &profile, &db, &[Media::Cover]);
    let filter = TargetFilter {
        systems: vec!["gba".into()],
        media: vec![Media::Cover],
        scope: Scope::Missing,
    };
    let targets = select_targets(&art, &filter);
    let stems: Vec<_> = targets.iter().map(|t| t.rom.stem.as_str()).collect();
    assert_eq!(stems, ["001 Golden Sun", "002 Goldn Adventure"]);
    assert!(targets
        .iter()
        .all(|t| t.strip_numbering && t.folder == "GBA"));
    let all = select_targets(
        &art,
        &TargetFilter {
            systems: vec!["GBA".into()],
            media: vec![Media::Cover],
            scope: Scope::All,
        },
    );
    assert_eq!(all.len(), 3);
    let screenshots = select_targets(
        &art,
        &TargetFilter {
            systems: vec![],
            media: vec![Media::Screenshot],
            scope: Scope::All,
        },
    );
    assert!(
        screenshots.is_empty(),
        "unsupported media are never targets"
    );
}

#[test]
fn identifies_by_hash_name_and_arcade_dat_with_quick_id_memo() {
    let (lib, s, e) = (library(), server(), env());
    let dats = DatStore::new(&e.http, &e.cache, s.base());
    let db = SystemDb::builtin();
    let profile = common::profile("anbernic-stock");
    let scan = scan_library(lib.path(), &profile, &db, &ScanOptions::default());
    let rom = |folder: &str, stem: &str| {
        scan.systems
            .iter()
            .find(|x| x.folder_name == folder)
            .unwrap()
            .roms
            .iter()
            .find(|r| r.stem == stem)
            .unwrap()
            .clone()
    };

    let id = Identifier::new(&e.cache, &dats, IdentifyOptions::default());
    let golden = rom("GBA", "001 Golden Sun");
    let identity = id.identify(&golden, db.get("gba").unwrap(), true).unwrap();
    assert_eq!(
        (identity.method, identity.canonical.as_deref()),
        (IdMethod::Hash, Some("Golden Sun (USA, Europe)"))
    );
    let mtime = coverly_core::cache::mtime_ns(golden.modified);
    assert!(
        e.cache
            .quick_id(&golden.path, golden.size, mtime)
            .unwrap()
            .is_some(),
        "hash memoised"
    );

    let other = id
        .identify(
            &rom("GBA", "002 Goldn Adventure"),
            db.get("gba").unwrap(),
            true,
        )
        .unwrap();
    assert_eq!(
        (other.method, other.canonical, other.parsed.title.as_str()),
        (IdMethod::Name, None, "Goldn Adventure")
    );

    let arcade = id
        .identify(&rom("MAME", "10yard"), db.get("mame").unwrap(), false)
        .unwrap();
    assert_eq!(
        (arcade.method, arcade.canonical.as_deref()),
        (IdMethod::ArcadeDat, Some("10-Yard Fight (World, set 1)"))
    );

    let hash_only = Identifier::new(
        &e.cache,
        &dats,
        IdentifyOptions {
            method: MatchMethod::HashOnly,
            ..IdentifyOptions::default()
        },
    );
    assert_eq!(
        hash_only
            .identify(
                &rom("GBA", "002 Goldn Adventure"),
                db.get("gba").unwrap(),
                true
            )
            .unwrap()
            .method,
        IdMethod::Unknown
    );

    let name_only = Identifier::new(
        &e.cache,
        &dats,
        IdentifyOptions {
            method: MatchMethod::NameOnly,
            ..IdentifyOptions::default()
        },
    );
    assert_eq!(
        name_only
            .identify(&golden, db.get("gba").unwrap(), true)
            .unwrap()
            .method,
        IdMethod::Name
    );
}

#[test]
fn gather_stores_candidates_and_reuses_them_offline() {
    let (lib, e) = (library(), env());
    let db = SystemDb::builtin();
    let profile = common::profile("anbernic-stock");
    let scan = scan_library(lib.path(), &profile, &db, &ScanOptions::default());
    let art = resolve_art(&scan, &profile, &db, &[Media::Cover]);
    let targets = select_targets(
        &art,
        &TargetFilter {
            systems: vec!["GBA".into()],
            media: vec![Media::Cover],
            scope: Scope::Missing,
        },
    );

    let first = {
        let s = server();
        let dats = DatStore::new(&e.http, &e.cache, s.base());
        let id = Identifier::new(&e.cache, &dats, IdentifyOptions::default());
        let src = LibretroSource::new(
            &e.http,
            &e.cache,
            LibretroConfig {
                base: s.base(),
                raw_base: s.url("/raw"),
                ..Default::default()
            },
        );
        let sources: Vec<&dyn ArtSource> = vec![&src];
        gather(
            &targets,
            &db,
            &sources,
            &id,
            &e.cache,
            &GatherOptions::default(),
            &NoProgress,
            &AtomicBool::new(false),
        )
    };
    assert_eq!(first.items.len(), 2);
    let golden = &first.items[0];
    assert_eq!(
        golden.candidates[&Media::Cover][0].kind,
        MatchKind::Canonical
    );
    let fuzzy = &first.items[1];
    assert_eq!(
        fuzzy.candidates[&Media::Cover][0].name,
        "Golden Adventure (USA)"
    );

    // Phase two needs no network: the server is gone, the cache answers.
    let dats = DatStore::new(&e.http, &e.cache, "http://127.0.0.1:9");
    let id = Identifier::new(&e.cache, &dats, IdentifyOptions::default());
    let src = LibretroSource::new(
        &e.http,
        &e.cache,
        LibretroConfig {
            base: "http://127.0.0.1:9".into(),
            raw_base: "http://127.0.0.1:9".into(),
            ..Default::default()
        },
    );
    let sources: Vec<&dyn ArtSource> = vec![&src];
    let again = gather(
        &targets,
        &db,
        &sources,
        &id,
        &e.cache,
        &GatherOptions::default(),
        &NoProgress,
        &AtomicBool::new(false),
    );
    assert_eq!(again.items[0].candidates, first.items[0].candidates);
    assert!(again.paused.is_none() && again.items.iter().all(|i| i.errors.is_empty()));
}

#[test]
fn rate_limit_pauses_gathering() {
    let (lib, e) = (library(), env());
    let s = server();
    s.mock(
        "/Nintendo%20-%20Game%20Boy%20Advance/Named_Boxarts/",
        MockResponse::status(429).header("Retry-After", "0"),
    );
    let db = SystemDb::builtin();
    let profile = common::profile("anbernic-stock");
    let scan = scan_library(lib.path(), &profile, &db, &ScanOptions::default());
    let art = resolve_art(&scan, &profile, &db, &[Media::Cover]);
    let targets = select_targets(
        &art,
        &TargetFilter {
            systems: vec!["GBA".into()],
            media: vec![Media::Cover],
            scope: Scope::Missing,
        },
    );
    let dats = DatStore::new(&e.http, &e.cache, s.base());
    let id = Identifier::new(&e.cache, &dats, IdentifyOptions::default());
    let src = LibretroSource::new(
        &e.http,
        &e.cache,
        LibretroConfig {
            base: s.base(),
            raw_base: s.url("/raw"),
            ..Default::default()
        },
    );
    let sources: Vec<&dyn ArtSource> = vec![&src];
    let g = gather(
        &targets,
        &db,
        &sources,
        &id,
        &e.cache,
        &GatherOptions::default(),
        &NoProgress,
        &AtomicBool::new(false),
    );
    assert!(
        g.paused.as_deref().unwrap_or("").contains("rate limited"),
        "{:?}",
        g.paused
    );
    assert!(g.items.len() < targets.len());
}

#[test]
fn going_offline_pauses_gathering() {
    let (lib, e) = (library(), env());
    let db = SystemDb::builtin();
    let profile = common::profile("anbernic-stock");
    let scan = scan_library(lib.path(), &profile, &db, &ScanOptions::default());
    let art = resolve_art(&scan, &profile, &db, &[Media::Cover]);
    let targets = select_targets(
        &art,
        &TargetFilter {
            systems: vec!["GBA".into()],
            media: vec![Media::Cover],
            scope: Scope::Missing,
        },
    );
    // Nothing cached and nothing listening: the first ROM must pause the phase,
    // not fail every ROM one by one with retries.
    let dats = DatStore::new(&e.http, &e.cache, "http://127.0.0.1:9");
    let id = Identifier::new(&e.cache, &dats, IdentifyOptions::default());
    let src = LibretroSource::new(
        &e.http,
        &e.cache,
        LibretroConfig {
            base: "http://127.0.0.1:9".into(),
            raw_base: "http://127.0.0.1:9".into(),
            ..Default::default()
        },
    );
    let sources: Vec<&dyn ArtSource> = vec![&src];
    let g = gather(
        &targets,
        &db,
        &sources,
        &id,
        &e.cache,
        &GatherOptions::default(),
        &NoProgress,
        &AtomicBool::new(false),
    );
    assert!(
        g.paused.as_deref().unwrap_or("").contains("offline"),
        "{:?}",
        g.paused
    );
    assert!(g.items.is_empty());
}

#[test]
fn consensus_counts_other_media_listings_and_is_part_of_the_cache_key() {
    let (lib, e) = (library(), env());
    let s = server();
    s.serve_libretro_index(GBA, "Named_Snaps", &["Golden Sun (Japan).png"]);
    s.serve_libretro_index(GBA, "Named_Titles", &["Golden Sun (Japan).png"]);
    let db = SystemDb::builtin();
    let profile = common::profile("anbernic-stock");
    let scan = scan_library(lib.path(), &profile, &db, &ScanOptions::default());
    let art = resolve_art(&scan, &profile, &db, &[Media::Cover]);
    let targets = select_targets(
        &art,
        &TargetFilter {
            systems: vec!["GBA".into()],
            media: vec![Media::Cover],
            scope: Scope::Missing,
        },
    );
    let dats = DatStore::new(&e.http, &e.cache, s.base());
    let id = Identifier::new(&e.cache, &dats, IdentifyOptions::default());
    let src = LibretroSource::new(
        &e.http,
        &e.cache,
        LibretroConfig {
            base: s.base(),
            raw_base: s.url("/raw"),
            ..Default::default()
        },
    );
    let sources: Vec<&dyn ArtSource> = vec![&src];
    let consensus_of = |g: &coverly_core::gather::Gathered, name: &str| {
        g.items[0].candidates[&Media::Cover]
            .iter()
            .find(|c| c.name == name)
            .unwrap()
            .consensus
    };
    // First without consensus (cached), then with it: the cached run must not be reused.
    let plain = gather(
        &targets,
        &db,
        &sources,
        &id,
        &e.cache,
        &GatherOptions::default(),
        &NoProgress,
        &AtomicBool::new(false),
    );
    assert_eq!(consensus_of(&plain, "Golden Sun (Japan)"), 0);
    let opts = GatherOptions {
        consensus: true,
        ..GatherOptions::default()
    };
    let g = gather(
        &targets,
        &db,
        &sources,
        &id,
        &e.cache,
        &opts,
        &NoProgress,
        &AtomicBool::new(false),
    );
    assert_eq!(consensus_of(&g, "Golden Sun (Japan)"), 3);
    assert_eq!(consensus_of(&g, "Golden Sun (USA, Europe)"), 1);
}

#[test]
fn hash_only_without_a_dat_never_falls_back_to_the_name() {
    let lib = tempfile::TempDir::new().unwrap();
    common::mkdir(lib.path(), "anbernic");
    common::write(lib.path(), "ROMS/PICO/Celeste.p8", "pico");
    let e = env();
    let dats = DatStore::new(&e.http, &e.cache, "http://127.0.0.1:9");
    let db = SystemDb::builtin();
    let profile = common::profile("anbernic-stock");
    let scan = scan_library(lib.path(), &profile, &db, &ScanOptions::default());
    let rom = scan
        .systems
        .iter()
        .find(|x| x.folder_name == "PICO")
        .unwrap()
        .roms[0]
        .clone();
    let pico = db.get("pico8").unwrap();
    let hash_only = Identifier::new(
        &e.cache,
        &dats,
        IdentifyOptions {
            method: MatchMethod::HashOnly,
            ..IdentifyOptions::default()
        },
    );
    assert_eq!(
        hash_only.identify(&rom, pico, false).unwrap().method,
        IdMethod::Unknown
    );
    let default = Identifier::new(&e.cache, &dats, IdentifyOptions::default());
    assert_eq!(
        default.identify(&rom, pico, false).unwrap().method,
        IdMethod::Name
    );
}

#[test]
fn a_hanging_network_pauses_gathering_after_the_first_rom() {
    let lib = library();
    let hang = HangingServer::start();
    let cfg = NetConfig {
        retries: 0,
        timeout: std::time::Duration::from_millis(300),
        ..NetConfig::default()
    };
    let http = PoliteHttp::new(UreqHttp::new(&cfg), &cfg);
    let cache = Cache::in_memory().unwrap();
    let db = SystemDb::builtin();
    let profile = common::profile("anbernic-stock");
    let scan = scan_library(lib.path(), &profile, &db, &ScanOptions::default());
    let art = resolve_art(&scan, &profile, &db, &[Media::Cover]);
    let targets = select_targets(
        &art,
        &TargetFilter {
            systems: vec!["GBA".into()],
            media: vec![Media::Cover],
            scope: Scope::Missing,
        },
    );
    assert_eq!(targets.len(), 2);
    let dats = DatStore::new(&http, &cache, hang.base());
    let id = Identifier::new(&cache, &dats, IdentifyOptions::default());
    let src = LibretroSource::new(
        &http,
        &cache,
        LibretroConfig {
            base: hang.base(),
            raw_base: hang.base(),
            ..Default::default()
        },
    );
    let sources: Vec<&dyn ArtSource> = vec![&src];
    let g = gather(
        &targets,
        &db,
        &sources,
        &id,
        &cache,
        &GatherOptions::default(),
        &NoProgress,
        &AtomicBool::new(false),
    );
    // A timeout is a transport failure: pause, do not wait it out per ROM.
    assert!(
        g.paused.as_deref().unwrap_or("").contains("timed out"),
        "{:?}",
        g.paused
    );
    assert!(g.items.is_empty() && g.warnings.is_empty(), "{g:?}");
    assert_eq!(hang.connections(), 1, "the second ROM never asks again");
}

#[test]
fn two_roms_with_one_dat_identity_do_not_share_a_name_match() {
    // A misnamed dump: the file says "Golden Adventure (USA)", the bytes are
    // Golden Sun. Its exact-name candidate must not leak to the other ROM.
    let lib = tempfile::TempDir::new().unwrap();
    common::mkdir(lib.path(), "anbernic");
    common::write(
        lib.path(),
        "ROMS/GBA/Golden Adventure (USA).gba",
        "golden-sun-bytes",
    );
    common::write(lib.path(), "ROMS/GBA/Golden Sun.gba", "golden-sun-bytes");
    let (s, e) = (server(), env());
    let db = SystemDb::builtin();
    let profile = common::profile("anbernic-stock");
    let scan = scan_library(lib.path(), &profile, &db, &ScanOptions::default());
    let art = resolve_art(&scan, &profile, &db, &[Media::Cover]);
    let targets = select_targets(
        &art,
        &TargetFilter {
            systems: vec!["GBA".into()],
            media: vec![Media::Cover],
            scope: Scope::Missing,
        },
    );
    let dats = DatStore::new(&e.http, &e.cache, s.base());
    let id = Identifier::new(&e.cache, &dats, IdentifyOptions::default());
    let src = LibretroSource::new(
        &e.http,
        &e.cache,
        LibretroConfig {
            base: s.base(),
            raw_base: s.url("/raw"),
            ..Default::default()
        },
    );
    let sources: Vec<&dyn ArtSource> = vec![&src];
    let g = gather(
        &targets,
        &db,
        &sources,
        &id,
        &e.cache,
        &GatherOptions::default(),
        &NoProgress,
        &AtomicBool::new(false),
    );
    let names = |stem: &str| -> Vec<String> {
        let item = g.items.iter().find(|i| i.target.stem == stem).unwrap();
        assert_eq!(
            item.identity.canonical.as_deref(),
            Some("Golden Sun (USA, Europe)")
        );
        item.candidates[&Media::Cover]
            .iter()
            .map(|c| c.name.clone())
            .collect()
    };
    assert!(names("Golden Adventure (USA)").contains(&"Golden Adventure (USA)".to_string()));
    assert!(
        !names("Golden Sun").contains(&"Golden Adventure (USA)".to_string()),
        "{:?}",
        names("Golden Sun")
    );
}
