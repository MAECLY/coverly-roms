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

fn grid_json(id: u64, url: &str, up: u32) -> String {
    format!(
        r#"{{"id":{id},"style":"alternate","width":600,"height":900,"nsfw":false,"humor":false,"epilepsy":false,"mime":"image/png","url":{},"upvotes":{up},"downvotes":0}}"#,
        serde_json::to_string(url).unwrap()
    )
}

#[test]
fn grids_whose_url_authority_is_odd_are_dropped() {
    let w = world();
    let hostile = [
        "https://evil.com\\.steamgriddb.com/a.png",
        "https://evil.com%2f.steamgriddb.com/a.png",
        "https://ev%69l.steamgriddb.com/a.png",
        "https://evil.com .steamgriddb.com/a.png",
        "https://user@cdn2.steamgriddb.com/a.png",
    ];
    let mut grids: Vec<String> = hostile
        .iter()
        .enumerate()
        .map(|(i, u)| grid_json(900 + i as u64, u, 99))
        .collect();
    grids.push(grid_json(1, "https://cdn2.steamgriddb.com/grid/ok.png", 1));
    w.server.serve_json(
        &format!("/sgdb{}", grids_path(5248, &w.cfg)),
        &format!(r#"{{"success":true,"data":[{}]}}"#, grids.join(",")),
    );
    let src = SteamGridDbSource::new(&w.http, &w.cache, Some(Secret::new("k")), w.cfg.clone());
    let db = SystemDb::builtin();
    let rom = parse_name("Golden Sun (USA)", false);
    let c = src
        .candidates(&cover_query(
            db.get("gba").unwrap(),
            &rom,
            "Golden Sun (USA)",
        ))
        .unwrap();
    let urls: Vec<&str> = c.iter().map(|c| c.url.as_str()).collect();
    assert_eq!(urls, ["https://cdn2.steamgriddb.com/grid/ok.png"]);
}

#[test]
fn an_empty_or_malformed_key_is_a_key_error_without_any_request() {
    use coverly_core::sources::SourceError;
    let w = world();
    let db = SystemDb::builtin();
    let rom = parse_name("Golden Sun (USA)", false);
    let q = cover_query(db.get("gba").unwrap(), &rom, "Golden Sun (USA)");
    let empty = SteamGridDbSource::new(&w.http, &w.cache, Some(Secret::new("")), w.cfg.clone());
    let err = empty.candidates(&q).unwrap_err();
    assert!(matches!(err, SourceError::MissingKey { .. }), "{err:?}");
    for bad in ["a\r\nX", "sk test", "sk\u{0}x", "clé"] {
        let src = SteamGridDbSource::new(&w.http, &w.cache, Some(Secret::new(bad)), w.cfg.clone());
        let err = src.candidates(&q).unwrap_err();
        assert!(matches!(err, SourceError::Auth { .. }), "{bad:?} → {err:?}");
        assert!(err.is_source_wide());
    }
    assert!(w.server.requests().is_empty());
}

#[test]
fn changing_the_filters_or_the_base_does_not_reuse_cached_grids() {
    let w = world();
    let db = SystemDb::builtin();
    let rom = parse_name("Golden Sun (USA)", false);
    let q = cover_query(db.get("gba").unwrap(), &rom, "Golden Sun (USA)");
    let src = SteamGridDbSource::new(&w.http, &w.cache, Some(Secret::new("k")), w.cfg.clone());
    src.candidates(&q).unwrap();
    let humor = SgdbConfig {
        humor: true,
        ..w.cfg.clone()
    };
    w.server.serve_json(
        &format!("/sgdb{}", grids_path(5248, &humor)),
        &fixture("grids_5248.json"),
    );
    let src = SteamGridDbSource::new(&w.http, &w.cache, Some(Secret::new("k")), humor.clone());
    src.candidates(&q).unwrap();
    assert_eq!(
        w.server.hits(&format!("/sgdb{}", grids_path(5248, &humor))),
        1
    );
    // Another base (another server) asks again too.
    let other = SgdbConfig {
        base: w.server.url("/other"),
        ..w.cfg.clone()
    };
    w.server.serve_json(
        &format!("/other{}", search_path("Golden Sun")),
        &fixture("search_golden_sun.json"),
    );
    let src = SteamGridDbSource::new(&w.http, &w.cache, Some(Secret::new("k")), other);
    let _ = src.candidates(&q);
    assert_eq!(
        w.server
            .hits(&format!("/other{}", search_path("Golden Sun"))),
        1
    );
}

#[test]
fn a_404_search_is_not_cached() {
    let w = world();
    let db = SystemDb::builtin();
    let rom = parse_name("Nothing Here (USA)", false);
    let q = cover_query(db.get("gba").unwrap(), &rom, "Nothing Here (USA)");
    let src = SteamGridDbSource::new(&w.http, &w.cache, Some(Secret::new("k")), w.cfg.clone());
    assert!(src.candidates(&q).unwrap().is_empty());
    assert!(src.candidates(&q).unwrap().is_empty());
    assert_eq!(
        w.server
            .hits(&format!("/sgdb{}", search_path("Nothing Here"))),
        2
    );
}

// ---- Task 7: refused keys, rate limits, hostile answers, "test key" ----

use coverly_core::net::testing::HangingServer;
use coverly_core::net::NetError;
use coverly_core::sources::steamgriddb::check_key;
use coverly_core::sources::{Candidate, KeyCheck, SourceError};

fn gba_query_run(w: &World, key: Option<&str>) -> Result<Vec<Candidate>, SourceError> {
    let src = SteamGridDbSource::new(&w.http, &w.cache, key.map(Secret::new), w.cfg.clone());
    let db = SystemDb::builtin();
    let rom = parse_name("Golden Sun (USA)", false);
    src.candidates(&cover_query(
        db.get("gba").unwrap(),
        &rom,
        "Golden Sun (USA)",
    ))
}

#[test]
fn a_refused_key_is_a_per_source_error_not_a_pause() {
    let w = world();
    w.server.mock(
        &format!("/sgdb{}", search_path("Golden Sun")),
        MockResponse::status(401).header("Content-Type", "application/json"),
    );
    let e = gba_query_run(&w, Some("bad")).unwrap_err();
    assert!(matches!(e, SourceError::Auth { status: 401, .. }), "{e:?}");
    assert!(e.is_source_wide());
    // The same with the recorded body of a refused key, and with a 403.
    let w = world();
    let mut refused = MockResponse::ok(fixture("error_401.json").into_bytes());
    refused.status = 403;
    w.server
        .mock(&format!("/sgdb{}", search_path("Golden Sun")), refused);
    let e = gba_query_run(&w, Some("bad")).unwrap_err();
    assert!(matches!(e, SourceError::Auth { status: 403, .. }), "{e:?}");
    assert!(!format!("{e:?} {e}").contains("bad"));
}

#[test]
fn no_key_asks_nothing() {
    let w = world();
    assert!(matches!(
        gba_query_run(&w, None),
        Err(SourceError::MissingKey { .. })
    ));
    assert!(w.server.requests().is_empty());
}

#[test]
fn a_429_after_the_retries_pauses() {
    let w = world();
    w.server.mock(
        &format!("/sgdb{}", search_path("Golden Sun")),
        MockResponse::status(429).header("Retry-After", "0"),
    );
    match gba_query_run(&w, Some("k")) {
        Err(SourceError::Net(e)) => assert!(e.pauses(), "{e}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_body_that_is_not_json_or_not_successful_is_a_bad_response() {
    let w = world();
    let path = format!("/sgdb{}", search_path("Golden Sun"));
    w.server.mock(&path, MockResponse::ok("<html>"));
    assert!(matches!(
        gba_query_run(&w, Some("k")),
        Err(SourceError::BadResponse { .. })
    ));
    let w = world();
    w.server
        .serve_json(&path, r#"{"success":false,"errors":["x"]}"#);
    assert!(matches!(
        gba_query_run(&w, Some("k")),
        Err(SourceError::BadResponse { .. })
    ));
    let w = world();
    w.server
        .serve_json(&path, r#"{"success":true,"data":{"id":1}}"#);
    assert!(matches!(
        gba_query_run(&w, Some("k")),
        Err(SourceError::BadResponse { .. })
    ));
}

#[test]
fn an_oversized_answer_fails_this_rom_only() {
    let w = world();
    w.server.mock(
        &format!("/sgdb{}", search_path("Golden Sun")),
        MockResponse::ok(vec![b' '; 3 * 1024 * 1024]),
    );
    match gba_query_run(&w, Some("k")) {
        Err(e @ SourceError::Net(NetError::TooLarge { .. })) => {
            assert!(!e.is_source_wide());
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn hostile_grids_are_dropped_one_by_one() {
    let w = world();
    w.server.serve_json(
        &format!("/sgdb{}", grids_path(5248, &w.cfg)),
        &fixture("grids_hostile.json"),
    );
    let c = gba_query_run(&w, Some("k")).unwrap();
    assert_eq!(c.len(), 1, "{c:#?}");
    assert!(c[0].url.ends_with("/cdn/grid/8.png"));
    assert_eq!(
        c[0].origin.as_deref(),
        Some("game 5248 · grid 8 · 600x900 alt")
    );
    assert_eq!(c[0].votes, Some(Votes { up: 1, down: 0 }));
    // Nothing was asked of a foreign host.
    assert!(w
        .server
        .requests()
        .iter()
        .all(|r| r.path.starts_with("/sgdb/")));
}

#[test]
fn hostile_game_names_are_cleaned_or_dropped() {
    let w = world();
    w.server.serve_json(
        &format!("/sgdb{}", search_path("Golden Sun")),
        r#"{"success":true,"data":[{"id":5248,"name":"Golden Sun\u202e"},{"id":9,"name":"\u0000"},{"id":"x","name":"Golden Sun"},7]}"#,
    );
    let c = gba_query_run(&w, Some("k")).unwrap();
    assert!(!c.is_empty());
    assert!(c.iter().all(|c| c.name == "Golden Sun"), "{c:#?}");
    assert!(c
        .iter()
        .all(|c| c.kind == MatchKind::TitleExact && c.score == 95));
}

#[test]
fn test_key_reports_ok_invalid_missing_and_offline_without_echoing_the_key() {
    let w = world();
    let base = w.cfg.base.clone();
    let mario = format!("/sgdb{}", search_path("mario"));
    w.server.serve_json(&mario, r#"{"success":true,"data":[]}"#);
    assert_eq!(
        check_key(&w.http, &base, Some(&Secret::new("good"))),
        KeyCheck::Ok
    );
    let sent = w.server.requests();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].header("Authorization"), Some("Bearer good"));
    assert_eq!(check_key(&w.http, &base, None), KeyCheck::Missing);
    assert_eq!(
        check_key(&w.http, &base, Some(&Secret::new(""))),
        KeyCheck::Missing
    );
    assert_eq!(
        check_key(&w.http, &base, Some(&Secret::new("a\r\nX-Evil: 1"))),
        KeyCheck::Invalid
    );
    assert_eq!(w.server.requests().len(), 1, "no usable key, no request");
    w.server.mock(&mario, MockResponse::status(401));
    assert_eq!(
        check_key(&w.http, &base, Some(&Secret::new("bad"))),
        KeyCheck::Invalid
    );
    w.server.mock(&mario, MockResponse::status(403));
    assert_eq!(
        check_key(&w.http, &base, Some(&Secret::new("bad"))),
        KeyCheck::Invalid
    );
    w.server
        .mock(&mario, MockResponse::status(429).header("Retry-After", "0"));
    assert_eq!(
        check_key(&w.http, &base, Some(&Secret::new("k"))),
        KeyCheck::RateLimited
    );
    w.server.mock(&mario, MockResponse::status(500));
    assert_eq!(
        check_key(&w.http, &base, Some(&Secret::new("k"))),
        KeyCheck::Unavailable
    );
    let hang = HangingServer::start();
    let quick = NetConfig {
        retries: 0,
        timeout: std::time::Duration::from_millis(200),
        ..NetConfig::default()
    };
    let http = PoliteHttp::new(UreqHttp::new(&quick), &quick);
    assert_eq!(
        check_key(&http, &hang.base(), Some(&Secret::new("k"))),
        KeyCheck::Offline
    );
}

#[test]
fn test_key_trusts_a_2xx_only_when_it_is_a_successful_api_answer() {
    let w = world();
    let base = w.cfg.base.clone();
    let mario = format!("/sgdb{}", search_path("mario"));
    // A captive portal page, an unsuccessful answer and an empty body all
    // come back 200 but do not prove the key works.
    for body in [
        "<html>Sign in to the Wi-Fi</html>",
        r#"{"success":false,"errors":["x"]}"#,
        r#"{"data":[]}"#,
        "",
    ] {
        w.server
            .mock(&mario, MockResponse::ok(body.as_bytes().to_vec()));
        assert_eq!(
            check_key(&w.http, &base, Some(&Secret::new("k"))),
            KeyCheck::Unavailable,
            "{body:?}"
        );
    }
}
