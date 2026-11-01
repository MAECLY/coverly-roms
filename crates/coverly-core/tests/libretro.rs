use coverly_core::cache::Cache;
use coverly_core::identify::names::parse_name;
use coverly_core::net::testing::{apache_listing, png_bytes, MockResponse, MockServer};
use coverly_core::net::{NetConfig, PoliteHttp, UreqHttp};
use coverly_core::profiles::Media;
use coverly_core::sources::libretro::{
    encode_segment, parse_apache_dirs, parse_apache_files, raw_repo_name, LibretroConfig,
    LibretroSource,
};
use coverly_core::sources::{ArtQuery, ArtSource, MatchKind, SourceError};
use coverly_core::systems::SystemDb;

const GBA: &str = "Nintendo - Game Boy Advance";

fn http() -> PoliteHttp<UreqHttp> {
    let cfg = NetConfig {
        retries: 0,
        ..NetConfig::default()
    };
    PoliteHttp::new(UreqHttp::new(&cfg), &cfg)
}

fn cfg(server: &MockServer) -> LibretroConfig {
    LibretroConfig {
        base: server.base(),
        raw_base: server.url("/raw"),
        ..LibretroConfig::default()
    }
}

fn gba_server() -> MockServer {
    let server = MockServer::start();
    server.serve_libretro_root(&[GBA, "MAME"]);
    server.serve_libretro_index(
        GBA,
        "Named_Boxarts",
        &[
            "Golden Sun (USA, Europe).png",
            "Golden Sun (Japan).png",
            "Golden Sun - The Lost Age (USA, Europe).png",
            "Street Fighter Alpha 3 (USA).png",
            "Batman _ Robin_ X (USA).png",
        ],
    );
    server
}

fn query<'a>(
    system: &'a coverly_core::systems::System,
    rom: &'a coverly_core::identify::names::ParsedName,
    stem: &'a str,
    canonical: Option<&'a str>,
) -> ArtQuery<'a> {
    ArtQuery {
        system,
        media: Media::Cover,
        canonical,
        rom,
        stem,
        by_name: true,
        fuzzy: true,
        min_score: 65,
        max_candidates: 5,
        hash: None,
        size: 0,
        file_name: stem,
        rom_path: None,
    }
}

#[test]
fn apache_index_parsing_keeps_only_safe_png_names() {
    let html = apache_listing(
        "x",
        &[
            "?C=N;O=D".into(),
            "/Nintendo%20-%20Game%20Boy%20Advance/".into(),
            "Golden%20Sun%20(USA,%20Europe).png".into(),
            "Zelda%20(USA).PNG".into(),
            "readme.txt".into(),
        ],
    );
    assert_eq!(
        parse_apache_files(&html),
        ["Golden Sun (USA, Europe).png", "Zelda (USA).PNG"]
    );
}

#[test]
fn apache_index_drops_traversal_and_control_names() {
    let html = apache_listing(
        "x",
        &[
            "..%2F..%2Fevil.png".into(),
            "a%2Fb.png".into(),
            "a%5Cb.png".into(),
            "%00x.png".into(),
            "..".into(),
            "ok.png".into(),
            "%E2%80%AEgnp.exe.png".into(),
        ],
    );
    assert_eq!(parse_apache_files(&html), ["ok.png"]);
}

#[test]
fn apache_index_decodes_html_entities_in_hrefs() {
    // The live server escapes `&` in attributes:
    // href="Tom%20&amp;%20Jerry%203%20(199x)(Dragon%20Co.)(AS)%5bp%5d.png"
    let html = apache_listing(
        "x",
        &[
            "Tom%20&amp;%20Jerry%203%20(AS)%5bp%5d.png".into(),
            "Q&#39;bert%20&quot;X&quot;%20&#x26;%20&lt;Y&gt;.png".into(),
            // Entities must not smuggle separators or traversal in.
            "..&#47;evil.png".into(),
            "a&#x5C;b.png".into(),
            // Decoding happens once: `%26amp;` stays a literal `&amp;`.
            "A%26amp;B.png".into(),
            // Unknown entities are left alone.
            "R&D&copy;.png".into(),
        ],
    );
    assert_eq!(
        parse_apache_files(&html),
        [
            "Tom & Jerry 3 (AS)[p].png",
            "Q'bert \"X\" & <Y>.png",
            "A&amp;B.png",
            "R&D&copy;.png",
        ]
    );
}

#[test]
fn root_listing_lists_system_folders() {
    let html = apache_listing(
        "/",
        &[
            "?C=M;O=A".into(),
            "Atari%20-%208-bit/".into(),
            "MAME/".into(),
            "stats/".into(),
        ],
    );
    assert_eq!(parse_apache_dirs(&html), ["Atari - 8-bit", "MAME"]);
}

/// The committed root listing (2026-10-31) holds the PS4 folder (systems
/// batch 3 D21) and none for PS5, Xbox One or Xbox Series.
#[test]
fn committed_root_listing_has_ps4_only_of_the_current_generation() {
    let html = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/research/thumbnails-root.html"
    ))
    .unwrap();
    let dirs = parse_apache_dirs(&html);
    assert!(dirs.iter().any(|d| d == "Sony - PlayStation 4"));
    for missing in ["PlayStation 5", "Xbox One", "Xbox Series"] {
        assert!(
            !dirs.iter().any(|d| d.contains(missing)),
            "{missing}: {dirs:?}"
        );
    }
    let db = SystemDb::builtin();
    assert_eq!(
        db.get("ps4").unwrap().libretro.as_deref(),
        Some("Sony - PlayStation 4")
    );
    for id in ["ps5", "xboxone", "xboxseries"] {
        assert_eq!(db.get(id).unwrap().libretro, None, "{id}");
    }
}

#[test]
fn raw_repo_names_and_url_segments() {
    assert_eq!(raw_repo_name(GBA), "Nintendo_-_Game_Boy_Advance");
    assert_eq!(raw_repo_name("Atari - 8-bit"), "Atari_-_8-bit_Family");
    assert_eq!(raw_repo_name("NEC - PC-8001 - PC-8801"), "NEC_-_PC-88");
    assert_eq!(
        encode_segment("Golden Sun (USA, Europe).png"),
        "Golden%20Sun%20(USA%2C%20Europe).png"
    );
}

#[test]
fn matching_order_canonical_name_title_fuzzy() {
    let server = gba_server();
    let (http, cache) = (http(), Cache::in_memory().unwrap());
    let src = LibretroSource::new(&http, &cache, cfg(&server));
    let db = SystemDb::builtin();
    let gba = db.get("gba").unwrap();

    let rom = parse_name("001 Golden Sun", true);
    let c = src
        .candidates(&query(
            gba,
            &rom,
            "001 Golden Sun",
            Some("Golden Sun (USA, Europe)"),
        ))
        .unwrap();
    assert_eq!(
        (c[0].kind, c[0].name.as_str(), c[0].score),
        (MatchKind::Canonical, "Golden Sun (USA, Europe)", 100)
    );
    assert!(c
        .iter()
        .any(|x| x.kind == MatchKind::TitleExact && x.name == "Golden Sun (Japan)"));
    assert!(
        c.iter().all(|x| !x.name.contains("Lost Age")),
        "a sequel subtitle is not the same title"
    );

    let rom = parse_name("Golden Sun (USA, Europe)", false);
    let c = src
        .candidates(&query(gba, &rom, "Golden Sun (USA, Europe)", None))
        .unwrap();
    assert_eq!(c[0].kind, MatchKind::NameExact);

    let rom = parse_name("001 Golden Sun", true);
    let c = src
        .candidates(&query(gba, &rom, "001 Golden Sun", None))
        .unwrap();
    assert_eq!(
        c.iter().filter(|x| x.kind == MatchKind::TitleExact).count(),
        2
    );

    let rom = parse_name("Goldn Sun", false);
    let c = src
        .candidates(&query(gba, &rom, "Goldn Sun", None))
        .unwrap();
    assert_eq!(c[0].kind, MatchKind::Fuzzy);
    assert!(c[0].score >= 65 && c[0].score < 95, "{}", c[0].score);

    let rom = parse_name("Street Fighter Alpha 2", false);
    assert!(
        src.candidates(&query(gba, &rom, "Street Fighter Alpha 2", None))
            .unwrap()
            .is_empty(),
        "numeral guard"
    );

    let rom = parse_name("Goldn Sun", false);
    let mut q = query(gba, &rom, "Goldn Sun", None);
    q.by_name = false;
    assert!(
        src.candidates(&q).unwrap().is_empty(),
        "hash-only never matches by name"
    );
}

#[test]
fn canonical_names_are_sanitised_like_libretro() {
    let server = gba_server();
    let (http, cache) = (http(), Cache::in_memory().unwrap());
    let src = LibretroSource::new(&http, &cache, cfg(&server));
    let db = SystemDb::builtin();
    let rom = parse_name("x", false);
    let c = src
        .candidates(&query(
            db.get("gba").unwrap(),
            &rom,
            "x",
            Some("Batman & Robin: X (USA)"),
        ))
        .unwrap();
    assert_eq!(c[0].name, "Batman _ Robin_ X (USA)");
    assert_eq!(
        c[0].url,
        server.url("/Nintendo%20-%20Game%20Boy%20Advance/Named_Boxarts/Batman%20_%20Robin_%20X%20(USA).png")
    );
}

#[test]
fn index_is_cached_and_revalidated_with_etag() {
    let server = gba_server();
    let (http, cache) = (http(), Cache::in_memory().unwrap());
    let db = SystemDb::builtin();
    let gba = db.get("gba").unwrap();
    let rom = parse_name("Golden Sun", false);
    let index_path = "/Nintendo%20-%20Game%20Boy%20Advance/Named_Boxarts/";
    LibretroSource::new(&http, &cache, cfg(&server))
        .candidates(&query(gba, &rom, "Golden Sun", None))
        .unwrap();
    LibretroSource::new(&http, &cache, cfg(&server))
        .candidates(&query(gba, &rom, "Golden Sun", None))
        .unwrap();
    assert_eq!(
        server.hits(index_path),
        1,
        "second source instance reads the SQLite copy"
    );
    assert_eq!(server.hits("/"), 1);

    server.mock(index_path, MockResponse::status(304));
    let expired = LibretroConfig {
        ttl_secs: 0,
        ..cfg(&server)
    };
    let c = LibretroSource::new(&http, &cache, expired)
        .candidates(&query(gba, &rom, "Golden Sun", None))
        .unwrap();
    assert!(!c.is_empty(), "304 keeps the cached listing");
    let last = server
        .requests()
        .into_iter()
        .rfind(|r| r.path == index_path)
        .unwrap();
    assert_eq!(last.header("if-none-match"), Some("\"listing-v1\""));
}

#[test]
fn unknown_systems_are_unavailable() {
    let server = gba_server();
    let (http, cache) = (http(), Cache::in_memory().unwrap());
    let src = LibretroSource::new(&http, &cache, cfg(&server));
    let db = SystemDb::builtin();
    let rom = parse_name("x", false);
    let err = src
        .candidates(&query(db.get("snes").unwrap(), &rom, "x", None))
        .unwrap_err();
    assert!(
        matches!(err, SourceError::SystemUnavailable { .. }),
        "{err}"
    );
}

#[test]
fn fetch_falls_back_to_the_raw_github_host() {
    let server = gba_server();
    server.mock(
        "/raw/Nintendo_-_Game_Boy_Advance/master/Named_Boxarts/Golden%20Sun%20(Japan).png",
        MockResponse::ok(png_bytes(10, 10)),
    );
    let (http, cache) = (http(), Cache::in_memory().unwrap());
    let src = LibretroSource::new(&http, &cache, cfg(&server));
    let db = SystemDb::builtin();
    let rom = parse_name("Golden Sun", false);
    let c = src
        .candidates(&query(db.get("gba").unwrap(), &rom, "Golden Sun", None))
        .unwrap();
    let japan = c.iter().find(|x| x.name == "Golden Sun (Japan)").unwrap();
    assert_eq!(src.fetch(japan).unwrap(), png_bytes(10, 10));
    let usa = c
        .iter()
        .find(|x| x.name == "Golden Sun (USA, Europe)")
        .unwrap();
    assert!(matches!(src.fetch(usa), Err(SourceError::NotFound(_))));
}

#[test]
fn has_name_checks_other_media_indexes() {
    let server = gba_server();
    server.serve_libretro_index(GBA, "Named_Snaps", &["Golden Sun (Japan).png"]);
    let (http, cache) = (http(), Cache::in_memory().unwrap());
    let src = LibretroSource::new(&http, &cache, cfg(&server));
    let db = SystemDb::builtin();
    let gba = db.get("gba").unwrap();
    assert!(src
        .has_name(gba, Media::Screenshot, "golden sun (japan)")
        .unwrap());
    assert!(!src
        .has_name(gba, Media::Screenshot, "golden sun (usa, europe)")
        .unwrap());
    assert!(
        !src.has_name(gba, Media::Title, "golden sun (japan)")
            .unwrap(),
        "missing index (404) = no names"
    );
}
