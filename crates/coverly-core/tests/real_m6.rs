//! Real SteamGridDB / ScreenScraper checks, run by hand once Miguel
//! provides credentials (never committed):
//!
//! ```text
//! COVERLY_SGDB_KEY=… cargo test -p coverly-core --test real_m6 -- --ignored --nocapture
//! COVERLY_SS_DEVID=… COVERLY_SS_DEVPASSWORD=… cargo test -p coverly-core --test real_m6 -- --ignored --nocapture
//! ```
//! Without them every test prints why it skipped and passes.

use coverly_core::cache::Cache;
use coverly_core::credentials::{DevCredentials, Secret, SourceKeys};
use coverly_core::identify::names::parse_name;
use coverly_core::net::{NetConfig, PoliteHttp, UreqHttp};
use coverly_core::profiles::Media;
use coverly_core::sources::screenscraper::{ScreenScraperSource, SsConfig};
use coverly_core::sources::steamgriddb::{
    check_key, SgdbConfig, SteamGridDbSource, STEAMGRIDDB_BASE,
};
use coverly_core::sources::{ArtQuery, ArtSource, KeyCheck};
use coverly_core::systems::SystemDb;

fn http() -> PoliteHttp<UreqHttp> {
    let net = NetConfig::default();
    PoliteHttp::new(UreqHttp::new(&net), &net)
}

#[test]
#[ignore = "real SteamGridDB; needs COVERLY_SGDB_KEY"]
fn real_m6_steamgriddb_key_finds_voted_covers() {
    let keys = SourceKeys::from_env(|n| std::env::var(n).ok());
    let Some(key) = keys.steamgriddb else {
        eprintln!("skipped: COVERLY_SGDB_KEY is not set");
        return;
    };
    let http = http();
    assert_eq!(check_key(&http, STEAMGRIDDB_BASE, Some(&key)), KeyCheck::Ok);
    assert_eq!(
        check_key(
            &http,
            STEAMGRIDDB_BASE,
            Some(&Secret::new("definitely-not-a-key"))
        ),
        KeyCheck::Invalid
    );
    let cache = Cache::in_memory().unwrap();
    let src = SteamGridDbSource::new(&http, &cache, Some(key), SgdbConfig::default());
    let db = SystemDb::builtin();
    for (system, title) in [
        ("snes", "Super Mario World (USA)"),
        ("gba", "Golden Sun (USA, Europe)"),
        ("psx", "Crash Bandicoot (USA)"),
    ] {
        let rom = parse_name(title, false);
        let c = src
            .candidates(&ArtQuery {
                system: db.get(system).unwrap(),
                media: Media::Cover,
                canonical: None,
                rom: &rom,
                stem: title,
                by_name: true,
                fuzzy: true,
                min_score: 65,
                max_candidates: 5,
                hash: None,
                size: 0,
                file_name: title,
                rom_path: None,
            })
            .unwrap();
        eprintln!(
            "{title}: {} covers, best {:?} {:?}",
            c.len(),
            c.first().and_then(|c| c.votes),
            c.first().and_then(|c| c.origin.clone())
        );
        if let Some(first) = c.first() {
            assert!(first.url.starts_with("https://") && first.votes.is_some());
            assert!(!src.fetch(first).unwrap().is_empty());
        }
    }
}

#[test]
#[ignore = "real ScreenScraper; needs a build with COVERLY_SS_DEVID/COVERLY_SS_DEVPASSWORD"]
fn real_m6_screenscraper_dev_credentials_find_box_art() {
    let Some(dev) = DevCredentials::from_build() else {
        eprintln!("skipped: this build has no ScreenScraper dev credentials");
        return;
    };
    let keys = SourceKeys::from_env(|n| std::env::var(n).ok());
    let http = http();
    let cache = Cache::in_memory().unwrap();
    let src = ScreenScraperSource::new(
        &http,
        &cache,
        dev,
        keys.screenscraper_user,
        SsConfig::default(),
    );
    let db = SystemDb::builtin();
    let stem = "Sonic The Hedgehog (USA, Europe)";
    let rom = parse_name(stem, false);
    let c = src
        .candidates(&ArtQuery {
            system: db.get("megadrive").unwrap(),
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
            file_name: "Sonic The Hedgehog (USA, Europe).md",
            rom_path: None,
        })
        .unwrap();
    eprintln!("{} covers; threads {}", c.len(), src.threads());
    assert!(!c.is_empty());
    assert!(c.iter().all(|c| !c.url.contains("devpassword")));
}
