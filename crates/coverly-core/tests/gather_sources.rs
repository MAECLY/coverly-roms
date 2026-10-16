mod common;

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use coverly_core::art::resolve_art;
use coverly_core::cache::Cache;
use coverly_core::gather::{
    gather, merge_candidates, select_targets, GatherOptions, Scope, TargetFilter,
};
use coverly_core::identify::dat::DatStore;
use coverly_core::identify::names::Region::{self, *};
use coverly_core::identify::{Identifier, IdentifyOptions, MatchMethod};
use coverly_core::net::testing::MockServer;
use coverly_core::net::{NetConfig, NetError, PoliteHttp, UreqHttp};
use coverly_core::profiles::Media;
use coverly_core::progress::NoProgress;
use coverly_core::scan::{scan_library, ScanOptions};
use coverly_core::sources::{ArtQuery, ArtSource, Candidate, MatchKind, SourceError};
use coverly_core::systems::{System, SystemDb};

fn cand(source: &str, name: &str, regions: &[Region], url: &str) -> Candidate {
    Candidate {
        source: source.into(),
        media: Media::Cover,
        name: name.into(),
        url: url.into(),
        fallback_url: None,
        regions: regions.to_vec(),
        kind: MatchKind::TitleExact,
        score: 95,
        votes: None,
        consensus: 1,
        origin: Some(format!("{source}:{url}")),
    }
}

/// An in-memory source: fixed candidates or a fixed error, counting calls.
struct Fake {
    id: &'static str,
    cands: Vec<Candidate>,
    err: Option<fn() -> SourceError>,
    calls: AtomicUsize,
}

impl Fake {
    fn ok(id: &'static str, cands: Vec<Candidate>) -> Fake {
        Fake {
            id,
            cands,
            err: None,
            calls: AtomicUsize::new(0),
        }
    }
    fn failing(id: &'static str, err: fn() -> SourceError) -> Fake {
        Fake {
            id,
            cands: vec![],
            err: Some(err),
            calls: AtomicUsize::new(0),
        }
    }
}

impl ArtSource for Fake {
    fn id(&self) -> &str {
        self.id
    }
    fn candidates(&self, _: &ArtQuery) -> Result<Vec<Candidate>, SourceError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        match self.err {
            Some(e) => Err(e()),
            None => Ok(self.cands.clone()),
        }
    }
    fn has_name(&self, _: &System, _: Media, _: &str) -> Result<bool, SourceError> {
        Ok(false)
    }
    fn fetch(&self, _: &Candidate) -> Result<Vec<u8>, SourceError> {
        Err(SourceError::NotFound(String::new()))
    }
}

fn three_roms() -> tempfile::TempDir {
    let dir = tempfile::TempDir::new().unwrap();
    common::mkdir(dir.path(), "anbernic");
    for f in [
        "Golden Sun (USA).gba",
        "Advance Wars (USA).gba",
        "Mother 3 (Japan).gba",
    ] {
        common::write(dir.path(), &format!("ROMS/GBA/{f}"), f);
    }
    dir
}

fn run(sources: &[&dyn ArtSource], consensus: bool) -> coverly_core::gather::Gathered {
    let lib = three_roms();
    let server = MockServer::start();
    let cfg = NetConfig {
        retries: 0,
        ..NetConfig::default()
    };
    let http = PoliteHttp::new(UreqHttp::new(&cfg), &cfg);
    let cache = Cache::in_memory().unwrap();
    let dats = DatStore::new(&http, &cache, server.base());
    let identifier = Identifier::new(
        &cache,
        &dats,
        IdentifyOptions {
            method: MatchMethod::NameOnly,
            arcade_translation: false,
            ..IdentifyOptions::default()
        },
    );
    let profile = common::profile("anbernic-stock");
    let db = SystemDb::builtin();
    let scan = scan_library(lib.path(), &profile, &db, &ScanOptions::default());
    let art = resolve_art(&scan, &profile, &db, &[Media::Cover]);
    let targets = select_targets(
        &art,
        &TargetFilter {
            systems: vec![],
            media: vec![Media::Cover],
            scope: Scope::Missing,
        },
    );
    let opts = GatherOptions {
        consensus,
        refresh: true,
        ..GatherOptions::default()
    };
    gather(
        &targets,
        &db,
        sources,
        &identifier,
        &cache,
        &opts,
        &NoProgress,
        &AtomicBool::new(false),
    )
}

#[test]
fn candidates_from_all_sources_are_merged_with_provenance() {
    let a = Fake::ok(
        "libretro",
        vec![cand("libretro", "Golden Sun (USA)", &[Usa], "https://l/1")],
    );
    let b = Fake::ok(
        "steamgriddb",
        vec![cand("steamgriddb", "Golden Sun", &[], "https://s/1")],
    );
    let g = run(&[&a, &b], false);
    for item in &g.items {
        let c = &item.candidates[&Media::Cover];
        let sources: Vec<&str> = c.iter().map(|c| c.source.as_str()).collect();
        assert!(sources.contains(&"libretro") && sources.contains(&"steamgriddb"));
        assert!(c
            .iter()
            .all(|c| c.origin.as_deref() == Some(&format!("{}:{}", c.source, c.url)[..])));
    }
}

#[test]
fn a_repeated_image_of_one_source_is_kept_once() {
    let mut all = vec![
        cand("libretro", "A (USA)", &[Usa], "https://l/a"),
        cand("libretro", "A (USA)", &[Usa], "https://l/a"),
        cand("steamgriddb", "A", &[], "https://l/a"),
    ];
    merge_candidates(&mut all, false);
    assert_eq!(
        all.len(),
        2,
        "same URL from another source is another candidate"
    );
}

#[test]
fn cross_source_consensus_counts_other_sources_in_an_overlapping_region() {
    let mut all = vec![
        cand(
            "libretro",
            "Golden Sun (USA, Europe)",
            &[Usa, Europe],
            "https://l/ue",
        ),
        cand("libretro", "Golden Sun (Japan)", &[Japan], "https://l/j"),
        cand("screenscraper", "Golden Sun", &[Usa], "https://ss/us"),
        cand("screenscraper", "Golden Sun", &[Europe], "https://ss/eu"),
        cand("steamgriddb", "Golden Sun", &[], "https://s/1"),
    ];
    merge_candidates(&mut all, true);
    let get = |url: &str| all.iter().find(|c| c.url == url).unwrap().consensus;
    assert_eq!(
        get("https://l/ue"),
        2,
        "screenscraper agrees once (a source counts once)"
    );
    assert_eq!(get("https://l/j"), 1);
    assert_eq!(get("https://ss/us"), 2);
    assert_eq!(get("https://s/1"), 1, "no region: no agreement");
    let mut off = vec![
        cand("libretro", "Golden Sun (USA)", &[Usa], "https://l/u"),
        cand("screenscraper", "Golden Sun", &[Usa], "https://ss/u"),
    ];
    merge_candidates(&mut off, false);
    assert!(
        off.iter().all(|c| c.consensus == 1),
        "only measured for Popularity"
    );
}

#[test]
fn a_source_with_a_refused_key_is_asked_once_and_the_others_still_answer() {
    let lib = Fake::ok(
        "libretro",
        vec![cand("libretro", "Golden Sun (USA)", &[Usa], "https://l/1")],
    );
    let sgdb = Fake::failing("steamgriddb", || SourceError::Auth {
        source: "steamgriddb".into(),
        status: 401,
    });
    let g = run(&[&lib, &sgdb], false);
    assert_eq!(sgdb.calls.load(Ordering::SeqCst), 1);
    assert_eq!(g.paused, None);
    assert_eq!(
        g.warnings
            .iter()
            .filter(|w| w.contains("steamgriddb refused access"))
            .count(),
        1
    );
    assert_eq!(g.items.len(), 3);
    assert!(g
        .items
        .iter()
        .all(|i| !i.candidates[&Media::Cover].is_empty()));
}

#[test]
fn a_rate_limit_from_any_source_pauses() {
    let lib = Fake::ok(
        "libretro",
        vec![cand("libretro", "Golden Sun (USA)", &[Usa], "https://l/1")],
    );
    let sgdb = Fake::failing("steamgriddb", || {
        NetError::RateLimited {
            host: "www.steamgriddb.com".into(),
            status: 429,
        }
        .into()
    });
    let g = run(&[&lib, &sgdb], false);
    assert!(g.paused.unwrap().contains("HTTP 429"));
    assert_eq!(g.items.len(), 1);
}

#[test]
fn per_rom_errors_of_several_sources_are_all_kept() {
    let a = Fake::failing("libretro", || SourceError::NotFound("https://l/x".into()));
    let b = Fake::failing("screenscraper", || SourceError::BadResponse {
        source: "screenscraper".into(),
        detail: "not JSON".into(),
    });
    let g = run(&[&a, &b], false);
    let e = &g.items[0].errors[&Media::Cover];
    assert!(
        e.contains("no image at https://l/x") && e.contains("not JSON"),
        "{e}"
    );
}
