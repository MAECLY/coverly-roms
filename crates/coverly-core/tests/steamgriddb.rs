//! SteamGridDB source (M6-01, D35): search, grids, votes and the cache,
//! all against a mock of the v2 API with recorded fixtures.

use coverly_core::cache::Cache;
use coverly_core::credentials::Secret;
use coverly_core::identify::names::parse_name;
use coverly_core::net::testing::{png_bytes, MockResponse, MockServer};
use coverly_core::net::{NetConfig, PoliteHttp, UreqHttp};
use coverly_core::profiles::Media;
use coverly_core::sources::steamgriddb::{grids_path, search_path, SgdbConfig, SteamGridDbSource};
use coverly_core::sources::{ArtQuery, ArtSource, MatchKind, Votes};
use coverly_core::systems::SystemDb;

const FIX: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/steamgriddb/");

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!("{FIX}{name}")).unwrap()
}

pub struct World {
    pub server: MockServer,
    pub http: PoliteHttp<UreqHttp>,
    pub cache: Cache,
    pub cfg: SgdbConfig,
}

fn world() -> World {
    let server = MockServer::start();
    let cfg = SgdbConfig {
        base: server.url("/sgdb"),
        ..SgdbConfig::default()
    };
    server.serve_json(
        &format!("/sgdb{}", search_path("Golden Sun")),
        &fixture("search_golden_sun.json"),
    );
    server.serve_json(
        &format!("/sgdb{}", grids_path(5248, &cfg)),
        &fixture("grids_5248.json"),
    );
    server.serve_json(
        &format!("/sgdb{}", search_path("Doom")),
        &fixture("search_doom.json"),
    );
    server.serve_json(
        &format!("/sgdb{}", grids_path(1, &cfg)),
        &fixture("grids_1.json"),
    );
    server.serve_json(
        &format!("/sgdb{}", grids_path(2, &cfg)),
        &fixture("grids_2.json"),
    );
    server.mock(
        "/cdn/grid/80211.png",
        MockResponse::ok(png_bytes(600, 900)).header("Content-Type", "image/png"),
    );
    let net = NetConfig {
        retries: 0,
        ..NetConfig::default()
    };
    World {
        http: PoliteHttp::new(UreqHttp::new(&net), &net),
        cache: Cache::in_memory().unwrap(),
        server,
        cfg,
    }
}

fn cover_query<'a>(
    system: &'a coverly_core::systems::System,
    rom: &'a coverly_core::identify::names::ParsedName,
    stem: &'a str,
) -> ArtQuery<'a> {
    ArtQuery {
        system,
        media: Media::Cover,
        canonical: None,
        rom,
        stem,
        by_name: true,
        fuzzy: true,
        min_score: 65,
        max_candidates: 5,
        hash: None,
        size: 0,
        file_name: stem,
    }
}

#[test]
fn grids_of_an_exact_title_come_back_with_votes_best_first_and_the_key_as_bearer() {
    let w = world();
    let src = SteamGridDbSource::new(
        &w.http,
        &w.cache,
        Some(Secret::new("sk-test")),
        w.cfg.clone(),
    );
    let db = SystemDb::builtin();
    let gba = db.get("gba").unwrap();
    let rom = parse_name("Golden Sun (USA, Europe)", false);
    let c = src
        .candidates(&cover_query(gba, &rom, "Golden Sun (USA, Europe)"))
        .unwrap();
    let ids: Vec<&str> = c.iter().map(|c| c.origin.as_deref().unwrap()).collect();
    assert_eq!(
        ids,
        [
            "game 5248 · grid 80211 · 600x900 alternate",
            "game 5248 · grid 80212 · 342x482 material",
            "game 5248 · grid 80213 · 600x900 alternate",
        ]
    );
    assert!(c.iter().all(|c| c.source == "steamgriddb"
        && c.name == "Golden Sun"
        && c.kind == MatchKind::TitleExact
        && c.score == 95
        && c.regions.is_empty()));
    assert_eq!(c[0].votes, Some(Votes { up: 8, down: 1 }));
    assert!(w
        .server
        .requests()
        .iter()
        .all(|r| r.header("Authorization") == Some("Bearer sk-test")));
    // The image comes from the CDN without the key.
    let bytes = src.fetch(&c[0]).unwrap();
    assert!(!bytes.is_empty());
    let cdn = w
        .server
        .requests()
        .into_iter()
        .find(|r| r.path.starts_with("/cdn/"))
        .unwrap();
    assert_eq!(cdn.header("Authorization"), None);
}

#[test]
fn search_and_grids_are_cached() {
    let w = world();
    let src = SteamGridDbSource::new(&w.http, &w.cache, Some(Secret::new("k")), w.cfg.clone());
    let db = SystemDb::builtin();
    let rom = parse_name("Golden Sun (USA)", false);
    let q = cover_query(db.get("gba").unwrap(), &rom, "Golden Sun (USA)");
    let first = src.candidates(&q).unwrap();
    let again = SteamGridDbSource::new(&w.http, &w.cache, Some(Secret::new("k")), w.cfg.clone());
    assert_eq!(again.candidates(&q).unwrap(), first);
    assert_eq!(
        w.server
            .hits(&format!("/sgdb{}", search_path("Golden Sun"))),
        1
    );
    assert_eq!(
        w.server.hits(&format!("/sgdb{}", grids_path(5248, &w.cfg))),
        1
    );
}

#[test]
fn ambiguous_titles_are_fuzzy_and_go_to_review() {
    let w = world();
    let src = SteamGridDbSource::new(&w.http, &w.cache, Some(Secret::new("k")), w.cfg.clone());
    let db = SystemDb::builtin();
    let rom = parse_name("Doom (USA)", false);
    let c = src
        .candidates(&cover_query(db.get("snes").unwrap(), &rom, "Doom (USA)"))
        .unwrap();
    assert_eq!(c.len(), 2);
    assert!(c
        .iter()
        .all(|c| c.kind == MatchKind::Fuzzy && c.score == 85));
}

#[test]
fn only_covers_and_never_without_a_name_match() {
    let w = world();
    let src = SteamGridDbSource::new(&w.http, &w.cache, Some(Secret::new("k")), w.cfg.clone());
    let db = SystemDb::builtin();
    let rom = parse_name("Golden Sun (USA)", false);
    let mut q = cover_query(db.get("gba").unwrap(), &rom, "Golden Sun (USA)");
    q.media = Media::Screenshot;
    assert!(src.candidates(&q).unwrap().is_empty());
    q.media = Media::Cover;
    q.by_name = false; // hash-only and unidentified
    assert!(src.candidates(&q).unwrap().is_empty());
    assert!(w.server.requests().is_empty());
}

#[test]
fn the_canonical_name_is_searched_when_known() {
    let w = world();
    let src = SteamGridDbSource::new(&w.http, &w.cache, Some(Secret::new("k")), w.cfg.clone());
    let db = SystemDb::builtin();
    let rom = parse_name("001 GS", true);
    let mut q = cover_query(db.get("gba").unwrap(), &rom, "001 GS");
    q.canonical = Some("Golden Sun (USA, Europe)");
    assert_eq!(src.candidates(&q).unwrap().len(), 3);
}
