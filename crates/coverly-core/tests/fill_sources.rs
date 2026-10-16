//! `FillEnv` over every source: libretro and SteamGridDB on one mock
//! server (never the real network), keys from a `SourceKeys` snapshot.

mod common;

use std::sync::atomic::AtomicBool;

use coverly_core::art::resolve_art;
use coverly_core::cache::Cache;
use coverly_core::credentials::{Secret, SourceKeys};
use coverly_core::fill::{ArtError, Endpoints, FillEnv, FillSettings};
use coverly_core::identify::MatchMethod;
use coverly_core::jobs::ApplyOptions;
use coverly_core::matching::{PickReason, StrategyKind};
use coverly_core::net::testing::{png_bytes, MockResponse, MockServer};
use coverly_core::net::{NetConfig, PoliteHttp, UreqHttp};
use coverly_core::plan::{Plan, PlanAction};
use coverly_core::profiles::Media;
use coverly_core::progress::NoProgress;
use coverly_core::scan::{scan_library, ScanOptions};
use coverly_core::sources::steamgriddb::{grids_path, search_path, SgdbConfig};
use coverly_core::sources::Candidate;
use coverly_core::systems::SystemDb;
use tempfile::TempDir;

const GBA: &str = "Nintendo - Game Boy Advance";
const FIX: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/steamgriddb/");

struct World {
    server: MockServer,
    http: PoliteHttp<UreqHttp>,
    cache: Cache,
    lib: TempDir,
    cache_dir: TempDir,
    data: TempDir,
    endpoints: Endpoints,
}

fn world() -> World {
    let server = MockServer::start();
    server.serve_libretro_root(&[GBA]);
    server.serve_libretro_index(GBA, "Named_Boxarts", &["Golden Sun (USA, Europe).png"]);
    server.serve_libretro_image(
        GBA,
        "Named_Boxarts",
        "Golden Sun (USA, Europe).png",
        png_bytes(500, 700),
    );
    let sg = SgdbConfig {
        base: server.url("/sgdb"),
        ..SgdbConfig::default()
    };
    let f = |n: &str| std::fs::read_to_string(format!("{FIX}{n}")).unwrap();
    server.serve_json(
        &format!("/sgdb{}", search_path("Golden Sun")),
        &f("search_golden_sun.json"),
    );
    server.serve_json(
        &format!("/sgdb{}", grids_path(5248, &sg)),
        &f("grids_5248.json"),
    );
    server.mock(
        "/cdn/grid/80211.png",
        MockResponse::ok(png_bytes(600, 900)).header("Content-Type", "image/png"),
    );
    let lib = TempDir::new().unwrap();
    common::mkdir(lib.path(), "anbernic");
    common::write(lib.path(), "ROMS/GBA/Golden Sun (USA, Europe).gba", "gs");
    let net = NetConfig {
        retries: 0,
        ..NetConfig::default()
    };
    World {
        endpoints: Endpoints {
            libretro: server.base(),
            libretro_raw: server.url("/raw"),
            database: server.base(),
            steamgriddb: server.url("/sgdb"),
            ..Endpoints::default()
        },
        http: PoliteHttp::new(UreqHttp::new(&net), &net),
        cache: Cache::in_memory().unwrap(),
        lib,
        cache_dir: TempDir::new().unwrap(),
        data: TempDir::new().unwrap(),
        server,
    }
}

impl World {
    fn env<'a>(&'a self, keys: &'a SourceKeys) -> FillEnv<'a> {
        FillEnv {
            cache: &self.cache,
            http: &self.http,
            endpoints: &self.endpoints,
            cache_dir: self.cache_dir.path(),
            data_dir: self.data.path(),
            keys,
        }
    }
}

fn settings(strategy: StrategyKind) -> FillSettings {
    FillSettings {
        strategy,
        match_method: MatchMethod::NameOnly,
        sources: vec!["libretro".into(), "steamgriddb".into()],
        ..FillSettings::default()
    }
}

fn plan(w: &World, keys: &SourceKeys, strategy: StrategyKind) -> Plan {
    let profile = common::profile("anbernic-stock");
    let db = SystemDb::builtin();
    let scan = scan_library(w.lib.path(), &profile, &db, &ScanOptions::default());
    let art = resolve_art(&scan, &profile, &db, &[Media::Cover]);
    w.env(keys).plan(
        w.lib.path(),
        &profile,
        &db,
        &art,
        &settings(strategy),
        &NoProgress,
        &AtomicBool::new(false),
    )
}

fn chosen(p: &Plan) -> &Candidate {
    match &p.items[0].action {
        PlanAction::Auto { chosen, .. } | PlanAction::Review { chosen, .. } => chosen,
        other => panic!("{other:?}"),
    }
}

fn sgdb_key() -> SourceKeys {
    SourceKeys {
        steamgriddb: Some(Secret::new("sk")),
        ..SourceKeys::default()
    }
}

#[test]
fn popularity_picks_the_most_voted_steamgriddb_cover_and_writes_it() {
    let w = world();
    let keys = sgdb_key();
    let p = plan(&w, &keys, StrategyKind::Popularity);
    assert_eq!(chosen(&p).source, "steamgriddb");
    assert_eq!(
        p.items[0].reason,
        Some(PickReason::Votes { score: 7, next: 2 })
    );
    assert!(matches!(p.items[0].action, PlanAction::Auto { .. }));
    let report = w
        .env(&keys)
        .apply(
            &p,
            &common::profile("anbernic-stock"),
            &ApplyOptions::default(),
            &NoProgress,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(report.filled, 1);
    assert!(w
        .lib
        .path()
        .join("ROMS/GBA/Imgs/Golden Sun (USA, Europe).png")
        .exists());
}

#[test]
fn priority_keeps_libretro_first() {
    let w = world();
    let p = plan(&w, &sgdb_key(), StrategyKind::Priority);
    assert_eq!(chosen(&p).source, "libretro");
}

#[test]
fn an_enabled_steamgriddb_without_a_key_is_skipped_with_a_warning() {
    let w = world();
    let p = plan(&w, &SourceKeys::default(), StrategyKind::Popularity);
    assert!(
        p.warnings
            .contains(&"steamgriddb: no API key stored; skipped".to_string()),
        "{:?}",
        p.warnings
    );
    assert_eq!(chosen(&p).source, "libretro");
    assert!(w
        .server
        .requests()
        .iter()
        .all(|r| !r.path.starts_with("/sgdb")));
}

#[test]
fn a_refused_key_warns_and_libretro_still_fills() {
    let w = world();
    w.server.mock(
        &format!("/sgdb{}", search_path("Golden Sun")),
        MockResponse::status(401),
    );
    let keys = SourceKeys {
        steamgriddb: Some(Secret::new("bad")),
        ..SourceKeys::default()
    };
    let p = plan(&w, &keys, StrategyKind::Popularity);
    assert_eq!(p.paused, None);
    assert!(
        p.warnings
            .iter()
            .any(|w| w.contains("steamgriddb refused access (HTTP 401)")),
        "{:?}",
        p.warnings
    );
    assert_eq!(chosen(&p).source, "libretro");
}

#[test]
fn a_steamgriddb_plan_still_applies_after_the_key_is_removed() {
    let w = world();
    let p = plan(&w, &sgdb_key(), StrategyKind::Popularity);
    assert_eq!(chosen(&p).source, "steamgriddb");
    let none = SourceKeys::default();
    let report = w
        .env(&none)
        .apply(
            &p,
            &common::profile("anbernic-stock"),
            &ApplyOptions::default(),
            &NoProgress,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(report.filled, 1, "images come from the CDN without the key");
}

#[test]
fn a_steamgriddb_candidate_previews_without_the_key() {
    let w = world();
    let p = plan(&w, &sgdb_key(), StrategyKind::Popularity);
    let c = chosen(&p).clone();
    let none = SourceKeys::default();
    let img = w
        .env(&none)
        .candidate_image(&common::profile("anbernic-stock"), &c)
        .unwrap();
    assert!(!img.bytes.is_empty());
    let mut odd = c.clone();
    odd.source = "nope".into();
    assert!(matches!(
        w.env(&none)
            .candidate_image(&common::profile("anbernic-stock"), &odd),
        Err(ArtError::UnknownSource(s)) if s == "nope"
    ));
}

#[test]
fn search_by_title_asks_every_usable_source() {
    let w = world();
    let db = SystemDb::builtin();
    let gba = db.get("gba").unwrap();
    let keys = sgdb_key();
    let found = w
        .env(&keys)
        .search_title(
            gba,
            Media::Cover,
            "Golden Sun",
            &settings(StrategyKind::Popularity),
        )
        .unwrap();
    assert!(found.iter().any(|c| c.source == "libretro"), "{found:?}");
    assert!(found.iter().any(|c| c.source == "steamgriddb"), "{found:?}");
    let mut pairs: Vec<(&str, &str)> = found
        .iter()
        .map(|c| (c.source.as_str(), c.url.as_str()))
        .collect();
    pairs.sort_unstable();
    pairs.dedup();
    assert_eq!(pairs.len(), found.len(), "deduped by source and URL");
    // Without a key SteamGridDB is not asked.
    let none = SourceKeys::default();
    let found = w
        .env(&none)
        .search_title(
            gba,
            Media::Cover,
            "Golden Sun",
            &settings(StrategyKind::Popularity),
        )
        .unwrap();
    assert!(found.iter().all(|c| c.source == "libretro"), "{found:?}");
}

#[test]
fn screenscraper_is_skipped_in_a_build_without_dev_credentials() {
    if coverly_core::credentials::DevCredentials::from_build().is_some() {
        eprintln!("skipped: this build carries ScreenScraper dev credentials");
        return;
    }
    let w = world();
    let keys = SourceKeys::default();
    let env = w.env(&keys);
    let sources = env.sources(false, 3600);
    let (on, warnings) = sources.enabled(&["screenscraper".into(), "libretro".into()]);
    assert_eq!(on.iter().map(|s| s.id()).collect::<Vec<_>>(), ["libretro"]);
    assert_eq!(
        warnings,
        ["screenscraper: not available in this build; skipped"]
    );
    assert!(sources.by_id("screenscraper").is_none());
    assert_eq!(
        sources.all().iter().map(|s| s.id()).collect::<Vec<_>>(),
        ["libretro", "steamgriddb"]
    );
}
