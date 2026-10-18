use std::time::{Duration, Instant};

use coverly_core::cache::Cache;
use coverly_core::credentials::{DevCredentials, Secret, UserCredentials};
use coverly_core::identify::hash::{Digests, HashOutcome, RomHashes};
use coverly_core::identify::names::{parse_name, Region};
use coverly_core::net::testing::{png_bytes, MockResponse, MockServer};
use coverly_core::net::{NetConfig, PoliteHttp, UreqHttp};
use coverly_core::organize::evidence::OnlineLookup;
use coverly_core::organize::online::ScreenScraperLookup;
use coverly_core::profiles::Media;
use coverly_core::sources::screenscraper::{
    check_account, media_type, ScreenScraperSource, SsConfig, QUERY_VALUE, SOFTNAME,
};
use coverly_core::sources::{ArtQuery, ArtSource, KeyCheck, MatchKind, SourceError};
use coverly_core::systems::SystemDb;
use percent_encoding::utf8_percent_encode;

const FIX: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/screenscraper/");
fn fixture(n: &str) -> String {
    std::fs::read_to_string(format!("{FIX}{n}")).unwrap()
}
fn enc(v: &str) -> String {
    utf8_percent_encode(v, QUERY_VALUE).to_string()
}

fn dev() -> DevCredentials {
    DevCredentials::from_values(Some("dev"), Some("devpw")).unwrap()
}
fn user() -> UserCredentials {
    UserCredentials {
        user: Secret::new("me"),
        password: Secret::new("mypw"),
    }
}
fn creds() -> String {
    format!(
        "devid=dev&devpassword=devpw&softname={}&output=json&ssid=me&sspassword=mypw",
        enc(SOFTNAME)
    )
}

fn hashes() -> HashOutcome {
    HashOutcome::Content(RomHashes {
        full: Digests {
            crc32: "abcd1234".into(),
            md5: "0123456789abcdef0123456789abcdef".into(),
            sha1: "0123456789abcdef0123456789abcdef01234567".into(),
            size: 16,
        },
        headerless: None,
        payload_sha1: None,
    })
}

fn jeu_infos_path() -> String {
    format!(
        "/ss/jeuInfos.php?{}&systemeid=12&romtype=rom&romnom={}&romtaille=16&crc=ABCD1234&md5={}&sha1={}",
        creds(), enc("Golden Sun (USA, Europe).gba"),
        "0123456789ABCDEF0123456789ABCDEF", "0123456789ABCDEF0123456789ABCDEF01234567"
    )
}

struct W {
    server: MockServer,
    http: PoliteHttp<UreqHttp>,
    cache: Cache,
    cfg: SsConfig,
}

fn w() -> W {
    let server = MockServer::start();
    let net = NetConfig {
        retries: 0,
        ..NetConfig::default()
    };
    let cfg = SsConfig {
        base: server.url("/ss"),
        min_interval: Duration::from_millis(1),
        ..SsConfig::default()
    };
    W {
        http: PoliteHttp::new(UreqHttp::new(&net), &net),
        cache: Cache::in_memory().unwrap(),
        server,
        cfg,
    }
}

fn run(
    w: &W,
    media: Media,
    hash: Option<&HashOutcome>,
) -> Result<Vec<coverly_core::sources::Candidate>, SourceError> {
    let db = SystemDb::builtin();
    let src = ScreenScraperSource::new(&w.http, &w.cache, dev(), Some(user()), w.cfg.clone());
    let rom = parse_name("Golden Sun (USA, Europe)", false);
    src.candidates(&ArtQuery {
        system: db.get("gba").unwrap(),
        media,
        canonical: None,
        rom: &rom,
        stem: "Golden Sun (USA, Europe)",
        by_name: true,
        fuzzy: true,
        min_score: 65,
        max_candidates: 5,
        hash,
        size: 16,
        file_name: "Golden Sun (USA, Europe).gba",
    })
}

#[test]
fn jeu_infos_url_carries_every_hash_the_size_the_name_and_the_system() {
    let w = w();
    w.server
        .serve_json(&jeu_infos_path(), &fixture("jeu_infos_golden_sun.json"));
    let c = run(&w, Media::Cover, Some(&hashes())).unwrap();
    assert_eq!(w.server.requests()[0].path, jeu_infos_path());
    assert!(c
        .iter()
        .all(|c| c.kind == MatchKind::Canonical && c.score == 100 && c.name == "Golden Sun"));
}

#[test]
fn maps_box_2d_ss_and_sstitle_per_region_and_drops_hostile_entries() {
    let w = w();
    w.server
        .serve_json(&jeu_infos_path(), &fixture("jeu_infos_golden_sun.json"));
    let covers = run(&w, Media::Cover, Some(&hashes())).unwrap();
    let regions: Vec<Vec<Region>> = covers.iter().map(|c| c.regions.clone()).collect();
    assert_eq!(
        covers.len(),
        3,
        "us, eu, jp; duplicate, bad region, bad type and support dropped"
    );
    assert!(
        regions.contains(&vec![Region::Usa])
            && regions.contains(&vec![Region::Europe])
            && regions.contains(&vec![Region::Japan])
    );
    assert_eq!(
        run(&w, Media::Screenshot, Some(&hashes())).unwrap().len(),
        1
    );
    assert_eq!(
        run(&w, Media::Title, Some(&hashes())).unwrap()[0].regions,
        vec![Region::World]
    );
    assert_eq!(media_type(Media::Cover, true), "flyer");
    assert_eq!(media_type(Media::Cover, false), "box-2D");
}

#[test]
fn candidate_urls_never_carry_credentials_and_fetch_adds_them() {
    let w = w();
    w.server
        .serve_json(&jeu_infos_path(), &fixture("jeu_infos_golden_sun.json"));
    let c = run(&w, Media::Cover, Some(&hashes())).unwrap();
    let us = c.iter().find(|c| c.regions == [Region::Usa]).unwrap();
    assert_eq!(
        us.url,
        format!(
            "{}/mediaJeu.php?systemeid=12&jeuid=3&media={}",
            w.cfg.base,
            enc("box-2D(us)")
        )
    );
    let json = serde_json::to_string(&c).unwrap();
    for secret in ["devpw", "mypw", "devid=", "ssid="] {
        assert!(!json.contains(secret), "{secret}");
    }
    let image_path = format!(
        "/ss/mediaJeu.php?systemeid=12&jeuid=3&media={}&{}",
        enc("box-2D(us)"),
        creds()
    );
    w.server
        .mock(&image_path, MockResponse::ok(png_bytes(10, 10)));
    let src = ScreenScraperSource::new(&w.http, &w.cache, dev(), Some(user()), w.cfg.clone());
    assert!(!src.fetch(us).unwrap().is_empty());
}

#[test]
fn without_hashes_it_searches_by_title() {
    let w = w();
    let path = format!(
        "/ss/jeuRecherche.php?{}&systemeid=12&recherche={}",
        creds(),
        enc("Golden Sun")
    );
    w.server
        .serve_json(&path, &fixture("jeu_recherche_golden_sun.json"));
    let c = run(&w, Media::Cover, None).unwrap();
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].kind, MatchKind::TitleExact);
}

fn jeu_infos_path_for(file: &str) -> String {
    jeu_infos_path().replace(&enc("Golden Sun (USA, Europe).gba"), &enc(file))
}

#[test]
fn requests_are_spaced_by_the_min_interval() {
    let mut w = w();
    w.cfg.min_interval = Duration::from_millis(150);
    let src = ScreenScraperSource::new(&w.http, &w.cache, dev(), Some(user()), w.cfg.clone());
    let db = SystemDb::builtin();
    let rom = parse_name("x", false);
    let h = hashes();
    let started = Instant::now();
    for file in ["a.gba", "b.gba", "c.gba"] {
        w.server.serve_json(
            &jeu_infos_path_for(file),
            &fixture("jeu_infos_golden_sun.json"),
        );
        src.candidates(&ArtQuery {
            system: db.get("gba").unwrap(),
            media: Media::Cover,
            canonical: None,
            rom: &rom,
            stem: "x",
            by_name: true,
            fuzzy: false,
            min_score: 65,
            max_candidates: 5,
            hash: Some(&h),
            size: 16,
            file_name: file,
        })
        .unwrap();
    }
    assert_eq!(w.server.requests().len(), 3);
    assert!(
        started.elapsed() >= Duration::from_millis(300),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(
        SsConfig::default().min_interval,
        Duration::from_millis(1200)
    );
}

#[test]
fn maxthreads_from_ssuser_set_the_thread_count() {
    let w = w();
    w.server
        .serve_json(&jeu_infos_path(), &fixture("jeu_infos_golden_sun.json"));
    let src = ScreenScraperSource::new(&w.http, &w.cache, dev(), Some(user()), w.cfg.clone());
    assert_eq!(src.threads(), 1);
    let db = SystemDb::builtin();
    let rom = parse_name("x", false);
    let h = hashes();
    src.candidates(&ArtQuery {
        system: db.get("gba").unwrap(),
        media: Media::Cover,
        canonical: None,
        rom: &rom,
        stem: "x",
        by_name: true,
        fuzzy: false,
        min_score: 65,
        max_candidates: 5,
        hash: Some(&h),
        size: 16,
        file_name: "Golden Sun (USA, Europe).gba",
    })
    .unwrap();
    assert_eq!(src.threads(), 3);
}

#[test]
fn a_reached_quota_skips_screenscraper_for_the_run() {
    for status in [430u16, 431] {
        let w = w();
        w.server
            .mock(&jeu_infos_path(), MockResponse::status(status));
        let e = run(&w, Media::Cover, Some(&hashes())).unwrap_err();
        assert!(
            matches!(e, SourceError::Exhausted { status: s, .. } if s == status),
            "{e:?}"
        );
        assert!(
            e.is_source_wide(),
            "a quota never pauses: a resume would hit it again"
        );
    }
    // requeststoday ≥ maxrequestsperday stops before the server says so.
    let w = w();
    // Edited as JSON, so the fixture's formatting does not matter.
    let mut full: serde_json::Value =
        serde_json::from_str(&fixture("jeu_infos_golden_sun.json")).unwrap();
    full["response"]["ssuser"]["requeststoday"] = "20000".into();
    let full = full.to_string();
    w.server.serve_json(&jeu_infos_path(), &full);
    let src = ScreenScraperSource::new(&w.http, &w.cache, dev(), Some(user()), w.cfg.clone());
    let db = SystemDb::builtin();
    let rom = parse_name("x", false);
    let h = hashes();
    let q = ArtQuery {
        system: db.get("gba").unwrap(),
        media: Media::Cover,
        canonical: None,
        rom: &rom,
        stem: "x",
        by_name: true,
        fuzzy: false,
        min_score: 65,
        max_candidates: 5,
        hash: Some(&h),
        size: 16,
        file_name: "Golden Sun (USA, Europe).gba",
    };
    src.candidates(&q).unwrap();
    // A cached game needs no request; a title search does, and is refused.
    let mut q2 = q.clone();
    q2.hash = None;
    assert!(matches!(
        src.candidates(&q2),
        Err(SourceError::Exhausted { status: 430, .. })
    ));
    assert_eq!(w.server.requests().len(), 1);
}

#[test]
fn closed_servers_pause_and_refused_credentials_do_not() {
    for (status, pauses) in [(401u16, true), (423, true), (403, false), (426, false)] {
        let w = w();
        w.server
            .mock(&jeu_infos_path(), MockResponse::status(status));
        let e = run(&w, Media::Cover, Some(&hashes())).unwrap_err();
        let paused = matches!(&e, SourceError::Net(n) if n.pauses());
        assert_eq!(paused, pauses, "{status}: {e}");
        if !pauses {
            assert!(e.is_source_wide());
        }
    }
}

#[test]
fn errors_never_show_secrets() {
    let w = w();
    w.server.mock(&jeu_infos_path(), MockResponse::status(500));
    let e = run(&w, Media::Cover, Some(&hashes()))
        .unwrap_err()
        .to_string();
    assert!(!e.contains("devpw") && !e.contains("mypw"), "{e}");
    let dbg = format!("{:?}", dev());
    assert!(!dbg.contains("devpw") && !dbg.contains("\"dev\""), "{dbg}");
}

#[test]
fn hostile_ids_regions_and_media_types_are_dropped() {
    let w = w();
    w.server.serve_json(&jeu_infos_path(),
        r#"{"response":{"jeu":{"id":"../3","noms":[{"region":"wor","text":"Golden Sun"}],"medias":[{"type":"box-2D","parent":"jeu","region":"us"}]}}}"#);
    assert!(run(&w, Media::Cover, Some(&hashes())).unwrap().is_empty());
    let w2 = self::w();
    w2.server
        .mock(&jeu_infos_path(), MockResponse::ok("not json"));
    assert!(matches!(
        run(&w2, Media::Cover, Some(&hashes())),
        Err(SourceError::BadResponse { .. })
    ));
}

#[test]
fn a_system_without_a_screenscraper_id_is_unavailable() {
    let w = w();
    let db = SystemDb::builtin();
    let src = ScreenScraperSource::new(&w.http, &w.cache, dev(), None, w.cfg.clone());
    let rom = parse_name("x", false);
    let pico = db.get("pico8").unwrap();
    let e = src
        .candidates(&ArtQuery {
            system: pico,
            media: Media::Cover,
            canonical: None,
            rom: &rom,
            stem: "x",
            by_name: true,
            fuzzy: false,
            min_score: 65,
            max_candidates: 5,
            hash: None,
            size: 0,
            file_name: "x.p8",
        })
        .unwrap_err();
    assert!(matches!(e, SourceError::SystemUnavailable { .. }));
}

#[test]
fn the_user_account_is_sent_only_when_complete_and_checked_cheaply() {
    let w = w();
    let path = format!("/ss/ssuserInfos.php?{}", creds());
    w.server.serve_json(&path, &fixture("user_infos.json"));
    assert_eq!(
        check_account(&w.http, &w.cfg, &dev(), Some(&user())),
        KeyCheck::Ok
    );
    assert_eq!(
        check_account(&w.http, &w.cfg, &dev(), None),
        KeyCheck::Missing
    );
    w.server.mock(&path, MockResponse::status(403));
    assert_eq!(
        check_account(&w.http, &w.cfg, &dev(), Some(&user())),
        KeyCheck::Invalid
    );
    assert_eq!(DevCredentials::from_values(Some(" "), Some("x")), None);
    assert_eq!(DevCredentials::from_values(Some("a"), None), None);
}

// --- Organise: optional online identification by CRC (Task 24) ---

fn zip_entry_path(sid: u32, inner: &str) -> String {
    format!(
        "/ss/jeuInfos.php?{}&systemeid={sid}&romtype=rom&romnom={}&romtaille=16&crc=ABCD1234",
        creds(),
        enc(inner)
    )
}

#[test]
fn identify_by_hash_returns_the_game_name() {
    let w = w();
    w.server
        .serve_json(&jeu_infos_path(), &fixture("jeu_infos_golden_sun.json"));
    let src = ScreenScraperSource::new(&w.http, &w.cache, dev(), Some(user()), w.cfg.clone());
    let name = src
        .identify_by_hash(12, "Golden Sun (USA, Europe).gba", &hashes())
        .unwrap();
    assert_eq!(name.as_deref(), Some("Golden Sun"));
    // Cached like every ScreenScraper answer: no second request.
    let again = src
        .identify_by_hash(12, "Golden Sun (USA, Europe).gba", &hashes())
        .unwrap();
    assert_eq!(again.as_deref(), Some("Golden Sun"));
    assert_eq!(w.server.requests().len(), 1);
    // A ROM that was not hashed is never asked.
    assert_eq!(
        src.identify_by_hash(12, "x.gba", &HashOutcome::NameOnly)
            .unwrap(),
        None
    );
    assert_eq!(w.server.requests().len(), 1);
}

#[test]
fn the_lookup_asks_at_most_three_systems_and_never_when_quota_is_gone() {
    let db = SystemDb::builtin();
    let inner = "Golden Sun (USA, Europe).gba";
    let w1 = w();
    w1.server.serve_json(
        &zip_entry_path(12, inner),
        &fixture("jeu_infos_golden_sun.json"),
    );
    let src = ScreenScraperSource::new(&w1.http, &w1.cache, dev(), Some(user()), w1.cfg.clone());
    let lookup = ScreenScraperLookup::new(&src, &db);
    let systems: Vec<String> = ["nope", "fbneo", "gb", "gbc", "nes", "gba"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    // Unknown systems and systems without a ScreenScraper id are skipped;
    // only the first three with one are asked (gba is fourth: never).
    assert!(lookup.lookup(&systems, inner, "abcd1234", 16).is_empty());
    let paths: Vec<String> = w1.server.requests().into_iter().map(|r| r.path).collect();
    assert_eq!(
        paths,
        vec![
            zip_entry_path(9, inner),
            zip_entry_path(10, inner),
            zip_entry_path(3, inner)
        ]
    );
    // The first hit wins and stops the lookup.
    let found = lookup.lookup(
        &["gba".to_string(), "gb".to_string()],
        inner,
        "abcd1234",
        16,
    );
    assert_eq!(found, vec![("gba".to_string(), "Golden Sun".to_string())]);
    assert_eq!(w1.server.hits(&zip_entry_path(12, inner)), 1);
    assert_eq!(w1.server.requests().len(), 4);
    // No candidate systems: nothing to bound the question, nothing asked.
    assert!(lookup.lookup(&[], inner, "abcd1234", 16).is_empty());
    assert_eq!(w1.server.requests().len(), 4);

    // A used-up quota: the first answer stops the source, the rest of the
    // run never asks again and the lookup answers nothing.
    let w = w();
    w.server
        .mock(&zip_entry_path(9, inner), MockResponse::status(430));
    let src = ScreenScraperSource::new(&w.http, &w.cache, dev(), Some(user()), w.cfg.clone());
    let lookup = ScreenScraperLookup::new(&src, &db);
    let gb_gbc = ["gb".to_string(), "gbc".to_string()];
    assert!(lookup.lookup(&gb_gbc, inner, "abcd1234", 16).is_empty());
    assert_eq!(w.server.requests().len(), 1);
    assert!(lookup
        .lookup(&gb_gbc, "other.gb", "1234abcd", 16)
        .is_empty());
    assert_eq!(w.server.requests().len(), 1);
}

/// The Golden Sun answer with `jeu` edited by `edit`.
fn golden_sun_with(edit: impl Fn(&mut serde_json::Value)) -> String {
    let mut v: serde_json::Value =
        serde_json::from_str(&fixture("jeu_infos_golden_sun.json")).unwrap();
    edit(&mut v["response"]["jeu"]);
    v.to_string()
}

#[test]
fn identify_by_hash_trusts_only_an_answer_for_the_asked_system_and_crc() {
    let inner = "Golden Sun (USA, Europe).gba";
    let zip = HashOutcome::ZipEntry {
        crc32: "abcd1234".into(),
        size: 16,
        inner_name: inner.into(),
        payload_sha1: None,
    };
    // Asked as Game Boy (9) but the server names a GBA game: the system is
    // the very question, so the answer says nothing about it.
    let w1 = w();
    w1.server.serve_json(
        &zip_entry_path(9, inner),
        &fixture("jeu_infos_golden_sun.json"),
    );
    let src = ScreenScraperSource::new(&w1.http, &w1.cache, dev(), Some(user()), w1.cfg.clone());
    assert_eq!(src.identify_by_hash(9, inner, &zip).unwrap(), None);

    // The server matched another ROM (by file name): its CRC differs.
    let w2 = w();
    w2.server.serve_json(
        &zip_entry_path(12, inner),
        &golden_sun_with(|j| {
            j["rom"] = serde_json::json!({ "romcrc": "11112222", "romfilename": inner })
        }),
    );
    let src = ScreenScraperSource::new(&w2.http, &w2.cache, dev(), Some(user()), w2.cfg.clone());
    assert_eq!(src.identify_by_hash(12, inner, &zip).unwrap(), None);

    // The same answer naming our CRC (any case) is trusted.
    let w3 = w();
    w3.server.serve_json(
        &zip_entry_path(12, inner),
        &golden_sun_with(|j| j["rom"] = serde_json::json!({ "romcrc": "AbCd1234" })),
    );
    let src = ScreenScraperSource::new(&w3.http, &w3.cache, dev(), Some(user()), w3.cfg.clone());
    assert_eq!(
        src.identify_by_hash(12, inner, &zip).unwrap().as_deref(),
        Some("Golden Sun")
    );
}
