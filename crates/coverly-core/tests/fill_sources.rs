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
        ["libretro", "steamgriddb", "embedded"]
    );
}

// ---------------------------------------------------------------------
// Embedded covers (systems batch 2, D17): plaintext icons inside games.
// ---------------------------------------------------------------------

mod embedded {
    use super::*;
    use coverly_core::consoles::testing::{cia, vita_sfo, wiiu_folder};
    use coverly_core::identify::names::parse_name;
    use coverly_core::sources::embedded::{EmbeddedSource, ID};
    use coverly_core::sources::{ArtQuery, ArtSource, MatchKind};
    use std::io::Write;
    use std::path::{Path, PathBuf};

    const N3DS: &str = "Nintendo - Nintendo 3DS";

    fn query_candidates(system: &str, media: Media, path: &Path) -> Vec<Candidate> {
        let db = SystemDb::builtin();
        let sys = db.get(system).unwrap();
        let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
        let rom = parse_name(&stem, false);
        let file_name = path.file_name().unwrap().to_string_lossy().into_owned();
        EmbeddedSource
            .candidates(&ArtQuery {
                system: sys,
                media,
                canonical: None,
                rom: &rom,
                stem: &stem,
                by_name: true,
                fuzzy: true,
                min_score: 65,
                max_candidates: 5,
                hash: None,
                size: 0,
                file_name: &file_name,
                rom_path: Some(path),
            })
            .unwrap()
    }

    fn decode(bytes: &[u8]) -> image::DynamicImage {
        assert_eq!(
            image::guess_format(bytes).unwrap(),
            image::ImageFormat::Png,
            "fetch always answers a PNG"
        );
        image::load_from_memory(bytes).unwrap()
    }

    /// A VPK (zip) with a Vita `param.sfo` and `icon` as `sce_sys/icon0.png`.
    fn vpk_with_icon(path: &Path, icon: &[u8]) {
        let file = std::fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, bytes) in [
            ("eboot.bin", vec![0u8; 256]),
            ("sce_sys/param.sfo", vita_sfo("PCSE00001")),
            ("sce_sys/icon0.png", icon.to_vec()),
        ] {
            zip.start_file(name, opts).unwrap();
            zip.write_all(&bytes).unwrap();
        }
        zip.finish().unwrap();
        coverly_core::dedupe::testing::pin_mtime(path, 1);
    }

    /// Writes `bytes` as the Wii U folder's `meta/iconTex.tga`, mtime pinned.
    fn put_icon_tga(game: &Path, bytes: &[u8], n: u64) {
        let icon = game.join("meta").join("iconTex.tga");
        std::fs::write(&icon, bytes).unwrap();
        coverly_core::dedupe::testing::pin_mtime(&icon, n);
    }

    fn tga_bytes(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(w, h, image::Rgba([10, 120, 200, 255]));
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Tga).unwrap();
        out.into_inner()
    }

    /// A clear CIA whose meta section holds an SMDH (48×48 icon).
    fn write_cia(path: &Path) -> Vec<u8> {
        let bytes = cia(0x0004_0000_0005_5D00, false, true).bytes();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, &bytes).unwrap();
        coverly_core::dedupe::testing::pin_mtime(path, 1);
        bytes
    }

    #[test]
    fn embedded_png_from_vpk_entry() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("Game.vpk");
        vpk_with_icon(&path, &png_bytes(128, 128));
        let found = query_candidates("psvita", Media::Cover, &path);
        assert_eq!(found.len(), 1, "{found:?}");
        let c = &found[0];
        assert_eq!((c.source.as_str(), c.kind), (ID, MatchKind::Canonical));
        assert_eq!(c.media, Media::Cover);
        let img = decode(&EmbeddedSource.fetch(c).unwrap());
        assert_eq!((img.width(), img.height()), (128, 128));
    }

    #[test]
    fn embedded_tga_from_wiiu_folder() {
        let dir = TempDir::new().unwrap();
        let game = dir.path().join("Test Game [0005000010145D00]");
        wiiu_folder(&game, "0005000010145D00", "WUP-P-ARDP", "Test Game");
        put_icon_tga(&game, &tga_bytes(128, 128), 1);
        let found = query_candidates("wiiu", Media::Cover, &game);
        assert_eq!(found.len(), 1, "{found:?}");
        let img = decode(&EmbeddedSource.fetch(&found[0]).unwrap());
        assert_eq!((img.width(), img.height()), (128, 128));
        assert_eq!(img.to_rgba8().get_pixel(5, 5).0, [10, 120, 200, 255]);
    }

    /// The art store keeps a fetched icon by its URL: an icon replaced
    /// inside a folder game (whose own size and mtime do not change) must
    /// give a new URL, or the old icon would be served from the store.
    #[test]
    fn embedded_url_follows_the_icon_inside_a_folder() {
        let dir = TempDir::new().unwrap();
        let game = dir.path().join("Test Game [0005000010145D00]");
        wiiu_folder(&game, "0005000010145D00", "WUP-P-ARDP", "Test Game");
        put_icon_tga(&game, &tga_bytes(128, 128), 1);
        let before = query_candidates("wiiu", Media::Cover, &game).remove(0);
        put_icon_tga(&game, &tga_bytes(64, 64), 2);
        let after = query_candidates("wiiu", Media::Cover, &game).remove(0);
        assert_ne!(before.url, after.url);
        let img = decode(&EmbeddedSource.fetch(&after).unwrap());
        assert_eq!((img.width(), img.height()), (64, 64));
    }

    #[test]
    fn embedded_smdh_icon_becomes_png() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("Test Game (USA).cia");
        write_cia(&path);
        let found = query_candidates("3ds", Media::Cover, &path);
        assert_eq!(found.len(), 1, "{found:?}");
        let img = decode(&EmbeddedSource.fetch(&found[0]).unwrap());
        assert_eq!((img.width(), img.height()), (48, 48));
    }

    #[test]
    fn embedded_answers_cover_only() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("Game.vpk");
        vpk_with_icon(&path, &png_bytes(64, 64));
        for media in [Media::Screenshot, Media::Title] {
            assert!(query_candidates("psvita", media, &path).is_empty());
        }
        // Without a path there is nothing to read.
        let db = SystemDb::builtin();
        let rom = parse_name("Game", false);
        let none = EmbeddedSource
            .candidates(&ArtQuery {
                system: db.get("psvita").unwrap(),
                media: Media::Cover,
                canonical: None,
                rom: &rom,
                stem: "Game",
                by_name: true,
                fuzzy: true,
                min_score: 65,
                max_candidates: 5,
                hash: None,
                size: 0,
                file_name: "Game.vpk",
                rom_path: None,
            })
            .unwrap();
        assert!(none.is_empty());
    }

    #[test]
    fn embedded_png_over_limits_has_no_candidate() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("Big.vpk");
        vpk_with_icon(&path, &png_bytes(2048, 2048));
        assert!(query_candidates("psvita", Media::Cover, &path).is_empty());
    }

    #[test]
    fn embedded_fetch_refuses_a_locator_the_reader_did_not_give() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("Game.vpk");
        vpk_with_icon(&path, &png_bytes(64, 64));
        let mut c = query_candidates("psvita", Media::Cover, &path).remove(0);
        c.url = c.url.replace("sce_sys/icon0.png", "eboot.bin");
        assert!(EmbeddedSource.fetch(&c).is_err());
        c.url = "embedded:{not json".into();
        assert!(EmbeddedSource.fetch(&c).is_err());
    }

    /// A Batocera card with `roms/3ds/Test Game (USA).cia` and a libretro
    /// mock that has its box art.
    fn world_3ds() -> (World, PathBuf, Vec<u8>) {
        let w = world();
        w.server.serve_libretro_root(&[GBA, N3DS]);
        w.server
            .serve_libretro_index(N3DS, "Named_Boxarts", &["Test Game (USA).png"]);
        w.server.serve_libretro_image(
            N3DS,
            "Named_Boxarts",
            "Test Game (USA).png",
            png_bytes(500, 700),
        );
        let rom = w
            .lib
            .path()
            .join("roms")
            .join("3ds")
            .join("Test Game (USA).cia");
        let raw = write_cia(&rom);
        (w, rom, raw)
    }

    fn plan_with(w: &World, sources: &[&str]) -> Plan {
        let profile = common::profile("batocera");
        let db = SystemDb::builtin();
        let scan = scan_library(w.lib.path(), &profile, &db, &ScanOptions::default());
        let art = resolve_art(&scan, &profile, &db, &[Media::Cover]);
        let settings = FillSettings {
            strategy: StrategyKind::RomRegion,
            match_method: MatchMethod::NameOnly,
            sources: sources.iter().map(|s| (*s).to_string()).collect(),
            ..FillSettings::default()
        };
        w.env(&SourceKeys::default()).plan(
            w.lib.path(),
            &profile,
            &db,
            &art,
            &settings,
            &NoProgress,
            &AtomicBool::new(false),
        )
    }

    fn the_3ds_item(p: &Plan) -> &coverly_core::plan::PlanItem {
        p.items
            .iter()
            .find(|i| i.system_id == "3ds")
            .unwrap_or_else(|| panic!("{:?}", p.items))
    }

    #[test]
    fn embedded_is_last() {
        let (w, _, _) = world_3ds();
        // Even listed first, the embedded icon ranks below real box art.
        for order in [["libretro", ID], [ID, "libretro"]] {
            let p = plan_with(&w, &order);
            let item = the_3ds_item(&p);
            let PlanAction::Auto {
                chosen,
                alternatives,
                ..
            } = &item.action
            else {
                panic!("{:?}", item.action)
            };
            assert_eq!(chosen.source, "libretro", "{order:?}");
            assert!(
                alternatives.iter().any(|c| c.source == ID),
                "{alternatives:?}"
            );
        }
    }

    #[test]
    fn embedded_output_is_reencoded() {
        let (w, rom, raw) = world_3ds();
        let p = plan_with(&w, &[ID]);
        let item = the_3ds_item(&p);
        let PlanAction::Auto { chosen, .. } = &item.action else {
            panic!("{:?}", item.action)
        };
        assert_eq!(chosen.source, ID);
        let profile = common::profile("batocera");
        let report = w
            .env(&SourceKeys::default())
            .apply(
                &p,
                &profile,
                &ApplyOptions::default(),
                &NoProgress,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(report.filled, 1, "{report:?}");
        let card = rom
            .parent()
            .unwrap()
            .join("images")
            .join("Test Game (USA)-image.png");
        let written = std::fs::read(&card).unwrap();
        // The raw icon is RGB565 tiles inside the CIA; the card is a PNG
        // the pipeline encoded, so no PNG signature exists in the ROM.
        let sig = &written[..8];
        assert_eq!(sig, b"\x89PNG\r\n\x1a\n");
        assert!(!raw.windows(8).any(|x| x == sig));
        let img = decode(&written);
        let slot = profile.slot(Media::Cover).unwrap();
        assert_eq!((img.width(), img.height()), (48, 48));
        if let Some((mw, mh)) = slot.max_size {
            assert!(img.width() <= mw && img.height() <= mh);
        }
    }

    // -----------------------------------------------------------------
    // Systems batch 3 (D21): the current generation's embedded icons,
    // tried first for the four systems with `embedded_first`.
    // -----------------------------------------------------------------

    use coverly_core::consoles::testing as t;

    const PS4: &str = "Sony - PlayStation 4";

    /// Writes `bytes` at `rel` (with `/`) inside `dir`, mtime pinned.
    fn put(dir: &Path, rel: &str, bytes: &[u8]) {
        t::put_file(dir, rel, bytes);
    }

    #[test]
    fn embedded_ps4_folder_icon_is_a_cover() {
        let dir = TempDir::new().unwrap();
        let game = dir.path().join("CUSA04447");
        t::ps4_app(
            &game,
            "CUSA04447",
            "gd",
            "EP0002-CUSA04447_00-GRAVITYRUSH00000",
        );
        put(&game, "sce_sys/icon0.png", &png_bytes(512, 512));
        let found = query_candidates("ps4", Media::Cover, &game);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].origin.as_deref(), Some("sce_sys/icon0.png"));
        let img = decode(&EmbeddedSource.fetch(&found[0]).unwrap());
        assert_eq!((img.width(), img.height()), (512, 512));
    }

    /// `pic0.png` and `pic1.png` are backgrounds: no media slot holds
    /// them, so the reader gives the icon only (D21).
    #[test]
    fn embedded_ps4_and_ps5_backgrounds_are_not_covers() {
        let dir = TempDir::new().unwrap();
        let ps4 = dir.path().join("CUSA04447");
        t::ps4_app(
            &ps4,
            "CUSA04447",
            "gd",
            "EP0002-CUSA04447_00-GRAVITYRUSH00000",
        );
        let ps5 = dir.path().join("PPSA01284");
        t::ps5_app(
            &ps5,
            &t::ps5_param_json("PPSA01284", "Returnal", false),
            true,
        );
        for game in [&ps4, &ps5] {
            put(game, "sce_sys/pic0.png", &png_bytes(64, 36));
            put(game, "sce_sys/pic1.png", &png_bytes(64, 36));
        }
        for (system, game) in [("ps4", &ps4), ("ps5", &ps5)] {
            let found = query_candidates(system, Media::Cover, game);
            let origins: Vec<_> = found.iter().map(|c| c.origin.as_deref()).collect();
            assert_eq!(origins, [Some("sce_sys/icon0.png")], "{system}");
        }
    }

    #[test]
    fn embedded_ps5_folder_icon_is_a_cover() {
        let dir = TempDir::new().unwrap();
        let game = dir.path().join("PPSA01284");
        t::ps5_app(
            &game,
            &t::ps5_param_json("PPSA01284", "Returnal", false),
            true,
        );
        let found = query_candidates("ps5", Media::Cover, &game);
        assert_eq!(found.len(), 1, "{found:?}");
        let img = decode(&EmbeddedSource.fetch(&found[0]).unwrap());
        assert_eq!((img.width(), img.height()), (8, 8));
    }

    #[test]
    fn embedded_gdk_square480_logo_is_a_cover() {
        let dir = TempDir::new().unwrap();
        let game = dir.path().join("Sample Game");
        t::gdk_folder(&game, &t::GdkSpec::default());
        put(&game, "LargeLogo.png", &png_bytes(480, 480));
        for system in ["xboxone", "xboxseries"] {
            let found = query_candidates(system, Media::Cover, &game);
            assert_eq!(found.len(), 1, "{system}: {found:?}");
            assert_eq!(found[0].origin.as_deref(), Some("LargeLogo.png"));
            let img = decode(&EmbeddedSource.fetch(&found[0]).unwrap());
            assert_eq!((img.width(), img.height()), (480, 480));
        }
    }

    /// A manifest logo stored only as an MRT variant: the probe resolves
    /// the largest `scale-*` into a plain file locator.
    #[test]
    fn embedded_gdk_mrt_logo_is_read_as_a_plain_file() {
        let dir = TempDir::new().unwrap();
        let game = dir.path().join("Sample Game");
        let spec = t::GdkSpec {
            logos: [
                "Assets\\LargeLogo.png",
                "Assets\\StoreLogo.png",
                "Assets\\Logo.png",
            ],
            ..Default::default()
        };
        put(
            &game,
            "MicrosoftGame.config",
            t::gdk_config(&spec).as_bytes(),
        );
        put(&game, "Game0.exe", &[0u8; 64]);
        put(&game, "Assets/LargeLogo.scale-100.png", &png_bytes(48, 48));
        put(&game, "Assets/LargeLogo.scale-200.png", &png_bytes(96, 96));
        let found = query_candidates("xboxone", Media::Cover, &game);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(
            found[0].origin.as_deref(),
            Some("Assets/LargeLogo.scale-200.png")
        );
        let img = decode(&EmbeddedSource.fetch(&found[0]).unwrap());
        assert_eq!((img.width(), img.height()), (96, 96));
    }

    #[test]
    fn embedded_era_logo_is_a_cover() {
        let dir = TempDir::new().unwrap();
        let game = dir.path().join("Sample ERA");
        t::era_folder(
            &game,
            &t::era_manifest("Sample.Game", "Sample Game", "Sample Game", true),
        );
        put(&game, "Logo.png", &png_bytes(150, 150));
        let found = query_candidates("xboxone", Media::Cover, &game);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].origin.as_deref(), Some("Logo.png"));
        let img = decode(&EmbeddedSource.fetch(&found[0]).unwrap());
        assert_eq!((img.width(), img.height()), (150, 150));
    }

    /// Writes a PS4 CNT package whose entry `0x1200` holds `icon`.
    fn write_ps4_pkg(path: &Path, icon: Vec<u8>) {
        let sfo = t::ps4_sfo("CUSA04447", "gd", "EP0002-CUSA04447_00-GRAVITYRUSH00000", 1);
        let pkg = t::cnt_pkg(
            "EP0002-CUSA04447_00-GRAVITYRUSH00000",
            0x1A,
            0,
            &[(0x1000, 0, sfo), (0x1200, 0, icon)],
        );
        std::fs::write(path, pkg.bytes()).unwrap();
        coverly_core::dedupe::testing::pin_mtime(path, 1);
    }

    #[test]
    fn embedded_ps4_pkg_icon_entry_is_a_cover() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("Gravity Rush.pkg");
        write_ps4_pkg(&path, png_bytes(512, 512));
        let found = query_candidates("ps4", Media::Cover, &path);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].origin.as_deref(), Some("icon in the file header"));
        let img = decode(&EmbeddedSource.fetch(&found[0]).unwrap());
        assert_eq!((img.width(), img.height()), (512, 512));
    }

    #[test]
    fn embedded_ps4_pkg_icon_that_is_not_a_png_has_no_candidate() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("Broken.pkg");
        write_ps4_pkg(&path, vec![0xE7; 512]);
        assert!(query_candidates("ps4", Media::Cover, &path).is_empty());
    }

    /// A Batocera card with a PS4 app folder (icon 64×64) and a 3DS CIA,
    /// and a libretro mock that has both box arts.
    fn world_current() -> World {
        let (w, _, _) = world_3ds();
        w.server.serve_libretro_root(&[GBA, N3DS, PS4]);
        w.server
            .serve_libretro_index(PS4, "Named_Boxarts", &["Test Game (Japan).png"]);
        w.server.serve_libretro_image(
            PS4,
            "Named_Boxarts",
            "Test Game (Japan).png",
            png_bytes(500, 700),
        );
        let game = w
            .lib
            .path()
            .join("roms")
            .join("ps4")
            .join("Test Game (Japan)");
        t::ps4_app(
            &game,
            "CUSA04447",
            "gd",
            "JP0002-CUSA04447_00-GRAVITYRUSH00000",
        );
        put(&game, "sce_sys/icon0.png", &png_bytes(64, 64));
        w
    }

    fn chosen_of<'p>(p: &'p Plan, system: &str) -> (&'p Candidate, &'p [Candidate]) {
        let item = p
            .items
            .iter()
            .find(|i| i.system_id == system)
            .unwrap_or_else(|| panic!("{system}: {:?}", p.items));
        match &item.action {
            PlanAction::Auto {
                chosen,
                alternatives,
                ..
            }
            | PlanAction::Review {
                chosen,
                alternatives,
                ..
            } => (chosen, alternatives),
            other => panic!("{system}: {other:?}"),
        }
    }

    #[test]
    fn embedded_first_for_current_generation_only() {
        let w = world_current();
        let p = plan_with(&w, &["libretro", "screenscraper", ID]);
        let (ps4, ps4_alts) = chosen_of(&p, "ps4");
        assert_eq!(ps4.source, ID, "{ps4_alts:?}");
        // The user's order puts `embedded` last: the «why» must not say
        // «first source in your order».
        let item = p.items.iter().find(|i| i.system_id == "ps4").unwrap();
        assert_eq!(item.reason, Some(PickReason::EmbeddedFirst));
        assert!(
            ps4_alts.iter().any(|c| c.source == "libretro"),
            "{ps4_alts:?}"
        );
        // A system without the flag keeps the user's order and the
        // icon last.
        let (n3ds, _) = chosen_of(&p, "3ds");
        assert_eq!(n3ds.source, "libretro");
    }

    #[test]
    fn embedded_first_needs_embedded_enabled() {
        let w = world_current();
        let p = plan_with(&w, &["libretro"]);
        let (ps4, alts) = chosen_of(&p, "ps4");
        assert_eq!(ps4.source, "libretro");
        assert!(alts.iter().all(|c| c.source != ID), "{alts:?}");
    }
}
