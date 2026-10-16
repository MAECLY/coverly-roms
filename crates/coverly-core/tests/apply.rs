mod common;

use std::fs;
use std::sync::atomic::AtomicBool;

use coverly_core::art::resolve_art;
use coverly_core::cache::Cache;
use coverly_core::gather::{gather, select_targets, GatherOptions, Scope, TargetFilter};
use coverly_core::identify::dat::DatStore;
use coverly_core::identify::{Identifier, IdentifyOptions};
use coverly_core::image::probe_dimensions;
use coverly_core::jobs::{
    apply_plan, ApplyOptions, ArtStore, ItemErrorKind, ItemStatus, JobReport,
};
use coverly_core::net::testing::{png_bytes, HangingServer, MockResponse, MockServer};
use coverly_core::net::{NetConfig, PoliteHttp, UreqHttp};
use coverly_core::plan::{build_plan, Plan, PlanAction, PlanOptions};
use coverly_core::profiles::Media;
use coverly_core::progress::NoProgress;
use coverly_core::scan::{scan_library, ScanOptions};
use coverly_core::sources::libretro::{LibretroConfig, LibretroSource};
use coverly_core::sources::ArtSource;
use coverly_core::systems::SystemDb;
use coverly_core::write::undo_last;
use tempfile::TempDir;

const GBA: &str = "Nintendo - Game Boy Advance";
const BOX: &str = "Named_Boxarts";

struct World {
    lib: TempDir,
    data: TempDir,
    cache_dir: TempDir,
    http: PoliteHttp<UreqHttp>,
    cache: Cache,
    server: MockServer,
}

fn world() -> World {
    let lib = TempDir::new().unwrap();
    let r = lib.path();
    common::mkdir(r, "anbernic");
    common::write(r, "ROMS/GBA/001 Golden Sun.gba", "golden-sun-bytes");
    common::write(r, "ROMS/GBA/002 Golden Adventure.gba", "adv");
    common::write(r, "ROMS/GBA/003 Missing Game.gba", "missing");
    common::write(r, "ROMS/GBA/004 Gldn Rsh.gba", "rush");
    let server = MockServer::start();
    server.serve_libretro_root(&[GBA]);
    server.serve_libretro_index(
        GBA,
        BOX,
        &[
            "Golden Sun (USA, Europe).png",
            "Golden Adventure (USA).png",
            "Golden Adventure (Japan).png",
            "Missing Game (USA).png",
            "Golden Rush (USA).png",
        ],
    );
    let crc = format!("{:08X}", crc32fast::hash(b"golden-sun-bytes"));
    server.serve_dat("metadat/no-intro/Nintendo - Game Boy Advance.dat",
        &format!("game (\n name \"Golden Sun (USA, Europe)\"\n rom ( name \"g.gba\" size 16 crc {crc} )\n)\n"));
    server.serve_libretro_image(
        GBA,
        BOX,
        "Golden Sun (USA, Europe).png",
        png_bytes(1000, 1400),
    );
    server.serve_libretro_image(
        GBA,
        BOX,
        "Golden Adventure (USA).png",
        b"<html>oops</html>".to_vec(),
    );
    server.serve_libretro_image(
        GBA,
        BOX,
        "Golden Adventure (Japan).png",
        png_bytes(300, 300),
    );
    server.serve_libretro_image(GBA, BOX, "Golden Rush (USA).png", png_bytes(200, 200));
    let cfg = NetConfig {
        retries: 0,
        ..NetConfig::default()
    };
    World {
        lib,
        data: TempDir::new().unwrap(),
        cache_dir: TempDir::new().unwrap(),
        http: PoliteHttp::new(UreqHttp::new(&cfg), &cfg),
        cache: Cache::in_memory().unwrap(),
        server,
    }
}

fn libretro(w: &World) -> LibretroSource<'_> {
    LibretroSource::new(
        &w.http,
        &w.cache,
        LibretroConfig {
            base: w.server.base(),
            raw_base: w.server.url("/raw"),
            ..Default::default()
        },
    )
}

fn plan(w: &World) -> Plan {
    let profile = common::profile("anbernic-stock");
    let db = SystemDb::builtin();
    let scan = scan_library(w.lib.path(), &profile, &db, &ScanOptions::default());
    let art = resolve_art(&scan, &profile, &db, &[Media::Cover]);
    let targets = select_targets(
        &art,
        &TargetFilter {
            systems: vec![],
            media: vec![Media::Cover],
            scope: Scope::Missing,
        },
    );
    let dats = DatStore::new(&w.http, &w.cache, w.server.base());
    let id = Identifier::new(&w.cache, &dats, IdentifyOptions::default());
    let src = libretro(w);
    let sources: Vec<&dyn ArtSource> = vec![&src];
    let g = gather(
        &targets,
        &db,
        &sources,
        &id,
        &w.cache,
        &GatherOptions::default(),
        &NoProgress,
        &AtomicBool::new(false),
    );
    build_plan(w.lib.path(), &profile, &db, &g, &PlanOptions::default())
}

fn apply(w: &World, plan: &Plan, opts: &ApplyOptions, cancel: bool) -> JobReport {
    let src = libretro(w);
    let sources: Vec<&dyn ArtSource> = vec![&src];
    let store = ArtStore::new(w.cache_dir.path(), &w.cache);
    apply_plan(
        plan,
        &common::profile("anbernic-stock"),
        &sources,
        &store,
        w.data.path(),
        opts,
        &NoProgress,
        &AtomicBool::new(cancel),
    )
    .unwrap()
}

fn status<'a>(r: &'a JobReport, stem: &str) -> &'a ItemStatus {
    &r.items
        .iter()
        .find(|i| i.rel_path.to_string_lossy().contains(stem))
        .unwrap()
        .status
}

fn img(w: &World, stem: &str) -> std::path::PathBuf {
    w.lib.path().join(format!("ROMS/GBA/Imgs/{stem}.png"))
}

#[test]
fn fills_resizes_and_undo_removes() {
    let w = world();
    let p = plan(&w);
    let r = apply(
        &w,
        &p,
        &ApplyOptions {
            concurrency: 1,
            ..ApplyOptions::default()
        },
        false,
    );
    assert!(
        matches!(status(&r, "001"), ItemStatus::Filled { name, .. } if name == "Golden Sun (USA, Europe)")
    );
    let (wd, ht) = probe_dimensions(&fs::read(img(&w, "001 Golden Sun")).unwrap()).unwrap();
    assert!(wd <= 250 && ht <= 360, "{wd}x{ht}");
    assert!(r.job_id.is_some());
    undo_last(w.data.path()).unwrap();
    assert!(!img(&w, "001 Golden Sun").exists());
}

#[test]
fn bad_image_falls_back_to_next_candidate() {
    let w = world();
    let r = apply(&w, &plan(&w), &ApplyOptions::default(), false);
    assert!(
        matches!(status(&r, "002"), ItemStatus::Filled { name, .. } if name == "Golden Adventure (Japan)"),
        "{:?}",
        status(&r, "002")
    );
}

#[test]
fn missing_images_on_both_hosts_are_no_art() {
    let w = world();
    let r = apply(&w, &plan(&w), &ApplyOptions::default(), false);
    assert!(matches!(
        status(&r, "003"),
        ItemStatus::Failed {
            kind: ItemErrorKind::NoArt,
            ..
        }
    ));
    assert_eq!(r.no_art, 1);
}

#[test]
fn review_items_are_skipped_with_auto_only() {
    let w = world();
    let r = apply(
        &w,
        &plan(&w),
        &ApplyOptions {
            include_review: false,
            ..ApplyOptions::default()
        },
        false,
    );
    assert_eq!(status(&r, "004"), &ItemStatus::SkippedReview);
    assert!(!img(&w, "004 Gldn Rsh").exists());
}

#[test]
fn target_created_after_planning_is_skipped() {
    let w = world();
    let p = plan(&w);
    common::write(w.lib.path(), "ROMS/GBA/Imgs/001 Golden Sun.png", "user art");
    let r = apply(&w, &p, &ApplyOptions::default(), false);
    assert_eq!(status(&r, "001"), &ItemStatus::SkippedExists);
    assert_eq!(
        fs::read_to_string(img(&w, "001 Golden Sun")).unwrap(),
        "user art"
    );
}

#[test]
fn rate_limit_pauses_and_journal_allows_undo() {
    let w = world();
    let p = plan(&w);
    w.server.mock(
        "/Nintendo%20-%20Game%20Boy%20Advance/Named_Boxarts/Golden%20Adventure%20(USA).png",
        MockResponse::status(429).header("Retry-After", "0"),
    );
    let r = apply(
        &w,
        &p,
        &ApplyOptions {
            concurrency: 1,
            ..ApplyOptions::default()
        },
        false,
    );
    assert!(r.paused.is_some());
    assert!(matches!(status(&r, "001"), ItemStatus::Filled { .. }));
    assert_eq!(status(&r, "004"), &ItemStatus::NotAttempted);
    undo_last(w.data.path()).unwrap();
    assert!(!img(&w, "001 Golden Sun").exists());
}

#[test]
fn going_offline_pauses_the_job() {
    let w = world();
    let mut p = plan(&w);
    for item in &mut p.items {
        if let PlanAction::Auto {
            chosen,
            alternatives,
            ..
        }
        | PlanAction::Review {
            chosen,
            alternatives,
            ..
        } = &mut item.action
        {
            for c in std::iter::once(chosen).chain(alternatives.iter_mut()) {
                c.url = "http://127.0.0.1:9/gone.png".into();
                c.fallback_url = None;
            }
        }
    }
    let r = apply(
        &w,
        &p,
        &ApplyOptions {
            concurrency: 1,
            ..ApplyOptions::default()
        },
        false,
    );
    assert!(
        r.paused.as_deref().unwrap_or("").contains("offline"),
        "{:?}",
        r.paused
    );
    assert_eq!(r.filled, 0);
    assert!(
        r.items.iter().all(|i| !matches!(
            i.status,
            ItemStatus::Failed {
                kind: ItemErrorKind::Network,
                ..
            }
        )),
        "offline pauses instead of failing items one by one"
    );
}

#[test]
fn cancel_writes_nothing() {
    let w = world();
    let r = apply(&w, &plan(&w), &ApplyOptions::default(), true);
    assert!(r.cancelled);
    assert!(!w.lib.path().join("ROMS/GBA/Imgs").exists());
}

#[test]
fn art_store_avoids_downloading_twice() {
    let w = world();
    let p = plan(&w);
    let path =
        "/Nintendo%20-%20Game%20Boy%20Advance/Named_Boxarts/Golden%20Sun%20(USA%2C%20Europe).png";
    apply(&w, &p, &ApplyOptions::default(), false);
    undo_last(w.data.path()).unwrap();
    apply(&w, &p, &ApplyOptions::default(), false);
    assert_eq!(w.server.hits(path), 1);
    assert!(img(&w, "001 Golden Sun").exists());
}

#[test]
fn a_timeout_pauses_the_job_without_trying_every_item() {
    let w = world();
    let mut p = plan(&w);
    let hang = HangingServer::start();
    for item in &mut p.items {
        if let PlanAction::Auto {
            chosen,
            alternatives,
            ..
        }
        | PlanAction::Review {
            chosen,
            alternatives,
            ..
        } = &mut item.action
        {
            for c in std::iter::once(chosen).chain(alternatives.iter_mut()) {
                c.url = format!("{}/slow.png", hang.base());
                c.fallback_url = None;
            }
        }
    }
    let cfg = NetConfig {
        retries: 0,
        timeout: std::time::Duration::from_millis(300),
        ..NetConfig::default()
    };
    let http = PoliteHttp::new(UreqHttp::new(&cfg), &cfg);
    let src = LibretroSource::new(
        &http,
        &w.cache,
        LibretroConfig {
            base: w.server.base(),
            raw_base: w.server.url("/raw"),
            ..Default::default()
        },
    );
    let sources: Vec<&dyn ArtSource> = vec![&src];
    let store = ArtStore::new(w.cache_dir.path(), &w.cache);
    let opts = ApplyOptions {
        concurrency: 1,
        ..ApplyOptions::default()
    };
    let r = apply_plan(
        &p,
        &common::profile("anbernic-stock"),
        &sources,
        &store,
        w.data.path(),
        &opts,
        &NoProgress,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(r.paused.is_some(), "{r:?}");
    assert_eq!(hang.connections(), 1, "one timeout pauses the queue");
    assert!(r
        .items
        .iter()
        .all(|i| !matches!(i.status, ItemStatus::Failed { .. })));
}

#[test]
fn art_store_rejects_tampered_files() {
    let w = world();
    let store = ArtStore::new(w.cache_dir.path(), &w.cache);
    store
        .put("https://example.org/a.png", b"image bytes")
        .unwrap();
    assert_eq!(
        store.get("https://example.org/a.png").as_deref(),
        Some(&b"image bytes"[..])
    );
    assert_eq!(store.get("https://example.org/other.png"), None);
    for e in walk(w.cache_dir.path()) {
        fs::write(e, b"tampered").unwrap();
    }
    assert_eq!(store.get("https://example.org/a.png"), None);
}

fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = vec![];
    for e in fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else {
            out.push(p);
        }
    }
    out
}

#[cfg(unix)]
#[test]
fn read_only_volume_pauses_the_job() {
    use std::os::unix::fs::PermissionsExt;
    let w = world();
    let p = plan(&w);
    let imgs = w.lib.path().join("ROMS/GBA/Imgs");
    fs::create_dir_all(&imgs).unwrap();
    fs::set_permissions(&imgs, fs::Permissions::from_mode(0o555)).unwrap();
    let r = apply(
        &w,
        &p,
        &ApplyOptions {
            concurrency: 1,
            ..ApplyOptions::default()
        },
        false,
    );
    fs::set_permissions(&imgs, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        r.paused.as_deref().unwrap_or("").contains("read-only"),
        "{:?}",
        r.paused
    );
    assert!(
        matches!(
            status(&r, "001"),
            ItemStatus::Failed {
                kind: ItemErrorKind::WriteDenied,
                ..
            }
        ),
        "{:?}",
        status(&r, "001")
    );
    assert_eq!(status(&r, "004"), &ItemStatus::NotAttempted);
    assert_eq!(r.failed, 1);
    assert!(r.job_id.is_some());
}
