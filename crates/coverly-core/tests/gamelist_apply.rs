//! A Batocera fill merges gamelist.xml; Undo restores it byte for byte.

mod common;

use std::fs;
use std::sync::atomic::AtomicBool;

use coverly_core::art::resolve_art;
use coverly_core::cache::Cache;
use coverly_core::credentials::SourceKeys;
use coverly_core::fill::{Endpoints, FillEnv, FillSettings};
use coverly_core::gather::Scope;
use coverly_core::jobs::ApplyOptions;
use coverly_core::net::testing::{png_bytes, MockServer};
use coverly_core::net::{NetConfig, PoliteHttp, UreqHttp};
use coverly_core::profiles::Media;
use coverly_core::progress::NoProgress;
use coverly_core::scan::{scan_library, ScanOptions};
use coverly_core::systems::SystemDb;
use coverly_core::write::{undo_last, OverwritePolicy};

const SNES: &str = "Nintendo - Super Nintendo Entertainment System";
const ORIGINAL: &str = "<?xml version=\"1.0\"?>\n<gameList>\n\t<game id=\"9\">\n\t\t<path>./Other.sfc</path>\n\t\t<desc>keep me</desc>\n\t</game>\n</gameList>\n";

fn run(
    gamelist: Option<&[u8]>,
    gamelist_is_dir: bool,
) -> (
    tempfile::TempDir,
    coverly_core::jobs::JobReport,
    tempfile::TempDir,
) {
    run_named("gamelist.xml", gamelist, gamelist_is_dir)
}

fn run_named(
    name: &str,
    gamelist: Option<&[u8]>,
    gamelist_is_dir: bool,
) -> (
    tempfile::TempDir,
    coverly_core::jobs::JobReport,
    tempfile::TempDir,
) {
    run_with(
        |r| {
            if gamelist_is_dir {
                common::mkdir(r, &format!("roms/snes/{name}"));
            } else if let Some(bytes) = gamelist {
                fs::write(r.join("roms/snes").join(name), bytes).unwrap();
            }
        },
        Scope::Missing,
        OverwritePolicy::Never,
    )
}

fn run_with(
    setup: impl Fn(&std::path::Path),
    scope: Scope,
    overwrite: OverwritePolicy,
) -> (
    tempfile::TempDir,
    coverly_core::jobs::JobReport,
    tempfile::TempDir,
) {
    let lib = tempfile::tempdir().unwrap();
    let r = lib.path();
    common::write(r, "roms/snes/Zelda.sfc", "zelda");
    common::write(r, "roms/snes/Other.sfc", "other");
    setup(r);
    let server = MockServer::start();
    server.serve_libretro_root(&[SNES]);
    server.serve_libretro_index(SNES, "Named_Boxarts", &["Zelda (USA).png"]);
    server.serve_libretro_image(
        SNES,
        "Named_Boxarts",
        "Zelda (USA).png",
        png_bytes(400, 560),
    );
    let cfg = NetConfig {
        retries: 0,
        ..NetConfig::default()
    };
    let http = PoliteHttp::new(UreqHttp::new(&cfg), &cfg);
    let cache = Cache::in_memory().unwrap();
    let endpoints = Endpoints {
        libretro: server.base(),
        libretro_raw: server.url("/raw"),
        database: server.base(),
        ..Endpoints::default()
    };
    let cache_dir = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let env = FillEnv {
        cache: &cache,
        http: &http,
        endpoints: &endpoints,
        cache_dir: cache_dir.path(),
        data_dir: data.path(),
        keys: &SourceKeys::default(),
    };
    let profile = common::profile("batocera");
    let db = SystemDb::builtin();
    let scan = scan_library(r, &profile, &db, &ScanOptions::default());
    let art = resolve_art(&scan, &profile, &db, &[Media::Cover]);
    let settings = FillSettings {
        media: vec![Media::Cover],
        scope,
        ..FillSettings::default()
    };
    let plan = env.plan(
        r,
        &profile,
        &db,
        &art,
        &settings,
        &NoProgress,
        &AtomicBool::new(false),
    );
    let report = env
        .apply(
            &plan,
            &profile,
            &ApplyOptions {
                overwrite,
                ..ApplyOptions::default()
            },
            &NoProgress,
            &AtomicBool::new(false),
        )
        .unwrap();
    (lib, report, data)
}

#[test]
fn a_batocera_fill_merges_the_gamelist_and_undo_restores_it() {
    let (lib, report, data) = run(Some(ORIGINAL.as_bytes()), false);
    assert_eq!(report.filled, 1, "{report:?}");
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let path = lib.path().join("roms/snes/gamelist.xml");
    let merged = fs::read_to_string(&path).unwrap();
    assert!(merged.contains("<desc>keep me</desc>") && merged.contains("<game id=\"9\">"));
    assert!(merged.contains("<path>./Zelda.sfc</path>"));
    assert!(merged.contains("<thumbnail>./images/Zelda-image.png</thumbnail>"));
    assert!(lib.path().join("roms/snes/images/Zelda-image.png").exists());
    let undo = undo_last(data.path()).unwrap();
    assert!(undo.skipped_modified.is_empty(), "{undo:?}");
    assert_eq!(
        fs::read(&path).unwrap(),
        ORIGINAL.as_bytes(),
        "byte for byte"
    );
    assert!(!lib.path().join("roms/snes/images/Zelda-image.png").exists());
}

#[test]
fn a_missing_gamelist_is_created_and_undo_removes_it() {
    let (lib, report, data) = run(None, false);
    assert_eq!(report.filled, 1);
    let path = lib.path().join("roms/snes/gamelist.xml");
    assert!(fs::read_to_string(&path)
        .unwrap()
        .contains("<path>./Zelda.sfc</path>"));
    undo_last(data.path()).unwrap();
    assert!(!path.exists());
}

#[test]
fn a_broken_or_odd_gamelist_is_left_alone_with_a_warning() {
    for (bytes, is_dir) in [
        (Some(b"<!DOCTYPE x><gameList/>".as_slice()), false),
        (Some(b"\xff\xfe<gameList/>".as_slice()), false),
        (Some(b"<gameList><game>".as_slice()), false),
        (None, true),
    ] {
        let (lib, report, _data) = run(bytes, is_dir);
        assert_eq!(report.filled, 1, "the art is written anyway");
        assert_eq!(report.warnings.len(), 1, "{:?}", report.warnings);
        let path = lib.path().join("roms/snes/gamelist.xml");
        if let Some(b) = bytes {
            assert_eq!(fs::read(&path).unwrap(), b);
        } else {
            assert!(path.is_dir());
        }
    }
}

#[test]
fn a_differently_cased_gamelist_is_merged_not_replaced() {
    let (lib, report, data) = run_named("GameList.xml", Some(ORIGINAL.as_bytes()), false);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let dir = lib.path().join("roms/snes");
    let names: Vec<String> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.to_lowercase() == "gamelist.xml")
        .collect();
    assert_eq!(names, ["GameList.xml"], "one gamelist, its spelling kept");
    let merged = fs::read_to_string(dir.join("GameList.xml")).unwrap();
    assert!(
        merged.contains("<desc>keep me</desc>") && merged.contains("<path>./Zelda.sfc</path>"),
        "{merged}"
    );
    undo_last(data.path()).unwrap();
    assert_eq!(
        fs::read(dir.join("GameList.xml")).unwrap(),
        ORIGINAL.as_bytes()
    );
}

#[test]
fn a_profile_without_the_gamelist_flag_never_touches_one() {
    let (lib, report, _data) = {
        // Same library, laid out as Batocera, but with the flag off.
        let lib = tempfile::tempdir().unwrap();
        let r = lib.path();
        common::write(r, "roms/snes/Zelda.sfc", "zelda");
        fs::write(r.join("roms/snes/gamelist.xml"), ORIGINAL).unwrap();
        let server = MockServer::start();
        server.serve_libretro_root(&[SNES]);
        server.serve_libretro_index(SNES, "Named_Boxarts", &["Zelda (USA).png"]);
        server.serve_libretro_image(
            SNES,
            "Named_Boxarts",
            "Zelda (USA).png",
            png_bytes(400, 560),
        );
        let cfg = NetConfig {
            retries: 0,
            ..NetConfig::default()
        };
        let http = PoliteHttp::new(UreqHttp::new(&cfg), &cfg);
        let cache = Cache::in_memory().unwrap();
        let endpoints = Endpoints {
            libretro: server.base(),
            libretro_raw: server.url("/raw"),
            database: server.base(),
            ..Endpoints::default()
        };
        let cache_dir = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        let env = FillEnv {
            cache: &cache,
            http: &http,
            endpoints: &endpoints,
            cache_dir: cache_dir.path(),
            data_dir: data.path(),
            keys: &SourceKeys::default(),
        };
        let mut profile = common::profile("batocera");
        profile.gamelist = false;
        let db = SystemDb::builtin();
        let scan = scan_library(r, &profile, &db, &ScanOptions::default());
        let art = resolve_art(&scan, &profile, &db, &[Media::Cover]);
        let settings = FillSettings {
            media: vec![Media::Cover],
            ..FillSettings::default()
        };
        let plan = env.plan(
            r,
            &profile,
            &db,
            &art,
            &settings,
            &NoProgress,
            &AtomicBool::new(false),
        );
        let report = env
            .apply(
                &plan,
                &profile,
                &ApplyOptions::default(),
                &NoProgress,
                &AtomicBool::new(false),
            )
            .unwrap();
        (lib, report, data)
    };
    assert_eq!(report.filled, 1);
    assert_eq!(
        fs::read_to_string(lib.path().join("roms/snes/gamelist.xml")).unwrap(),
        ORIGINAL
    );
}

#[cfg(unix)]
#[test]
fn a_symlinked_gamelist_is_never_followed() {
    let outside = tempfile::tempdir().unwrap();
    let real = outside.path().join("gamelist.xml");
    fs::write(&real, ORIGINAL).unwrap();
    let lib = tempfile::tempdir().unwrap();
    common::mkdir(lib.path(), "roms/snes");
    std::os::unix::fs::symlink(&real, lib.path().join("roms/snes/gamelist.xml")).unwrap();
    let report = {
        let r = lib.path();
        common::write(r, "roms/snes/Zelda.sfc", "zelda");
        let server = MockServer::start();
        server.serve_libretro_root(&[SNES]);
        server.serve_libretro_index(SNES, "Named_Boxarts", &["Zelda (USA).png"]);
        server.serve_libretro_image(
            SNES,
            "Named_Boxarts",
            "Zelda (USA).png",
            png_bytes(400, 560),
        );
        let cfg = NetConfig {
            retries: 0,
            ..NetConfig::default()
        };
        let http = PoliteHttp::new(UreqHttp::new(&cfg), &cfg);
        let cache = Cache::in_memory().unwrap();
        let endpoints = Endpoints {
            libretro: server.base(),
            libretro_raw: server.url("/raw"),
            database: server.base(),
            ..Endpoints::default()
        };
        let cache_dir = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        let env = FillEnv {
            cache: &cache,
            http: &http,
            endpoints: &endpoints,
            cache_dir: cache_dir.path(),
            data_dir: data.path(),
            keys: &SourceKeys::default(),
        };
        let profile = common::profile("batocera");
        let db = SystemDb::builtin();
        let scan = scan_library(r, &profile, &db, &ScanOptions::default());
        let art = resolve_art(&scan, &profile, &db, &[Media::Cover]);
        let settings = FillSettings {
            media: vec![Media::Cover],
            ..FillSettings::default()
        };
        let plan = env.plan(
            r,
            &profile,
            &db,
            &art,
            &settings,
            &NoProgress,
            &AtomicBool::new(false),
        );
        env.apply(
            &plan,
            &profile,
            &ApplyOptions::default(),
            &NoProgress,
            &AtomicBool::new(false),
        )
        .unwrap()
    };
    assert_eq!(report.filled, 1);
    assert_eq!(report.warnings.len(), 1, "{:?}", report.warnings);
    assert_eq!(fs::read_to_string(&real).unwrap(), ORIGINAL);
    assert!(
        fs::symlink_metadata(lib.path().join("roms/snes/gamelist.xml"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[test]
fn a_replaced_case_twin_is_named_as_spelled_on_disk() {
    // The art exists as `zelda-IMAGE.png`; the writer replaces that file
    // instead of adding `Zelda-image.png` next to it.
    let (lib, report, _data) = run_with(
        |r| {
            common::mkdir(r, "roms/snes/images");
            fs::write(
                r.join("roms/snes/images/zelda-IMAGE.png"),
                png_bytes(10, 10),
            )
            .unwrap();
        },
        Scope::All,
        OverwritePolicy::Backup,
    );
    assert_eq!(report.replaced, 1, "{report:?}");
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let merged = fs::read_to_string(lib.path().join("roms/snes/gamelist.xml")).unwrap();
    assert!(
        merged.contains("<thumbnail>./images/zelda-IMAGE.png</thumbnail>"),
        "{merged}"
    );
}

#[test]
fn a_local_image_on_batocera_updates_the_gamelist_too() {
    let lib = tempfile::tempdir().unwrap();
    let r = lib.path();
    common::write(r, "roms/snes/Zelda.sfc", "zelda");
    fs::write(r.join("roms/snes/gamelist.xml"), ORIGINAL).unwrap();
    let data = tempfile::tempdir().unwrap();
    let profile = common::profile("batocera");
    let system_dir = r.join("roms/snes");
    let rel = std::path::PathBuf::from("Zelda.sfc");
    let rom = coverly_core::fill::RomRef {
        system_id: "snes",
        folder: "snes",
        system_dir: &system_dir,
        rel_path: &rel,
        stem: "Zelda",
    };
    let target = system_dir.join("images/Zelda-image.png");
    let (job, _) = coverly_core::fill::write_local_image(
        r,
        &profile,
        &rom,
        Media::Cover,
        &target,
        &png_bytes(300, 400),
        data.path(),
        &coverly_core::fill::ManualOptions::default(),
    )
    .unwrap();
    assert!(job.is_some());
    let merged = fs::read_to_string(system_dir.join("gamelist.xml")).unwrap();
    assert!(
        merged.contains("<thumbnail>./images/Zelda-image.png</thumbnail>"),
        "{merged}"
    );
    undo_last(data.path()).unwrap();
    assert_eq!(
        fs::read_to_string(system_dir.join("gamelist.xml")).unwrap(),
        ORIGINAL
    );
}
