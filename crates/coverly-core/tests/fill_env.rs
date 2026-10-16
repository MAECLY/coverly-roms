mod common;

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use coverly_core::art::resolve_art;
use coverly_core::cache::Cache;
use coverly_core::fill::{
    remove_art, write_local_image, ArtError, Endpoints, FillEnv, FillSettings,
};
use coverly_core::image::probe_dimensions;
use coverly_core::jobs::{ApplyOptions, ItemStatus};
use coverly_core::net::testing::{png_bytes, MockServer};
use coverly_core::net::{NetConfig, PoliteHttp, UreqHttp};
use coverly_core::plan::{Decision, NoneReason, Plan, PlanAction};
use coverly_core::profiles::Media;
use coverly_core::progress::NoProgress;
use coverly_core::scan::{scan_library, ScanOptions};
use coverly_core::systems::SystemDb;
use coverly_core::write::{undo_last, RemoveOutcome, WriteOutcome};
use tempfile::TempDir;

const GBA: &str = "Nintendo - Game Boy Advance";
const BOX: &str = "Named_Boxarts";
const SUN: &str = "Golden Sun";
const RUSH: &str = "Gldn Rsh";

struct World {
    lib: TempDir,
    cache_dir: TempDir,
    data: TempDir,
    server: MockServer,
    http: PoliteHttp<UreqHttp>,
    cache: Cache,
    endpoints: Endpoints,
}

fn world() -> World {
    let lib = TempDir::new().unwrap();
    let r = lib.path();
    common::mkdir(r, "anbernic");
    common::write(r, &format!("ROMS/GBA/{SUN}.gba"), "golden-sun-bytes");
    common::write(r, &format!("ROMS/GBA/{RUSH}.gba"), "rush");
    let server = MockServer::start();
    server.serve_libretro_root(&[GBA]);
    server.serve_libretro_index(
        GBA,
        BOX,
        &[
            "Golden Sun (USA, Europe).png",
            "Golden Sun (Japan).png",
            "Golden Rush (USA).png",
        ],
    );
    server.serve_libretro_image(
        GBA,
        BOX,
        "Golden Sun (USA, Europe).png",
        png_bytes(1000, 1400),
    );
    server.serve_libretro_image(GBA, BOX, "Golden Sun (Japan).png", png_bytes(500, 700));
    server.serve_libretro_image(GBA, BOX, "Golden Rush (USA).png", png_bytes(200, 200));
    let cfg = NetConfig {
        retries: 0,
        ..NetConfig::default()
    };
    World {
        endpoints: Endpoints {
            libretro: server.base(),
            libretro_raw: server.url("/raw"),
            database: server.base(),
        },
        http: PoliteHttp::new(UreqHttp::new(&cfg), &cfg),
        cache: Cache::in_memory().unwrap(),
        lib,
        cache_dir: TempDir::new().unwrap(),
        data: TempDir::new().unwrap(),
        server,
    }
}

impl World {
    fn env(&self) -> FillEnv<'_> {
        FillEnv {
            cache: &self.cache,
            http: &self.http,
            endpoints: &self.endpoints,
            cache_dir: self.cache_dir.path(),
            data_dir: self.data.path(),
        }
    }

    fn plan(&self) -> Plan {
        let profile = common::profile("anbernic-stock");
        let db = SystemDb::builtin();
        let scan = scan_library(self.lib.path(), &profile, &db, &ScanOptions::default());
        let art = resolve_art(&scan, &profile, &db, &[Media::Cover]);
        self.env().plan(
            self.lib.path(),
            &profile,
            &db,
            &art,
            &FillSettings::default(),
            &NoProgress,
            &AtomicBool::new(false),
        )
    }

    fn img(&self, stem: &str) -> PathBuf {
        self.lib.path().join(format!("ROMS/GBA/Imgs/{stem}.png"))
    }
}

fn at(plan: &Plan, stem: &str) -> usize {
    plan.items.iter().position(|i| i.stem == stem).unwrap()
}

#[test]
fn plan_is_a_dry_run_with_auto_and_review_items() {
    let w = world();
    let p = w.plan();
    assert!(matches!(
        p.items[at(&p, SUN)].action,
        PlanAction::Auto { .. }
    ));
    assert!(matches!(
        p.items[at(&p, RUSH)].action,
        PlanAction::Review { .. }
    ));
    assert!(!w.lib.path().join("ROMS/GBA/Imgs").exists());
}

#[test]
fn decisions_turn_items_into_auto_or_skipped() {
    let w = world();
    let mut p = w.plan();
    let (sun, rush) = (at(&p, SUN), at(&p, RUSH));
    p.decide(&BTreeMap::from([
        (rush, Decision::Accept),
        (sun, Decision::Pick { candidate: 1 }),
    ]));
    assert!(matches!(p.items[rush].action, PlanAction::Auto { .. }));
    let PlanAction::Auto {
        chosen,
        alternatives,
        ..
    } = &p.items[sun].action
    else {
        panic!("{:?}", p.items[sun].action)
    };
    assert_eq!(chosen.name, "Golden Sun (Japan)");
    assert!(
        alternatives.is_empty(),
        "a picked image never falls back to another"
    );
    assert_eq!(p.totals.auto, 2);

    let mut skipped = w.plan();
    skipped.decide(&BTreeMap::from([
        (rush, Decision::Skip),
        (99, Decision::Accept),
    ]));
    assert!(matches!(
        skipped.items[rush].action,
        PlanAction::None {
            reason: NoneReason::Skipped
        }
    ));
    assert_eq!(skipped.totals.none, 1);
}

#[test]
fn apply_writes_the_picked_candidate() {
    let w = world();
    let mut p = w.plan();
    let (sun, rush) = (at(&p, SUN), at(&p, RUSH));
    p.decide(&BTreeMap::from([
        (sun, Decision::Pick { candidate: 1 }),
        (rush, Decision::Skip),
    ]));
    let r = w
        .env()
        .apply(
            &p,
            &common::profile("anbernic-stock"),
            &ApplyOptions {
                include_review: false,
                ..ApplyOptions::default()
            },
            &NoProgress,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert!(
        matches!(&r.items[sun].status, ItemStatus::Filled { name, .. } if name == "Golden Sun (Japan)")
    );
    assert_eq!(r.items[rush].status, ItemStatus::SkippedNoMatch);
    let (wd, ht) = probe_dimensions(&fs::read(w.img(SUN)).unwrap()).unwrap();
    assert!(wd <= 250 && ht <= 360, "{wd}x{ht}");
}

#[test]
fn candidate_image_is_re_encoded_for_the_slot_and_downloaded_once() {
    let w = world();
    let p = w.plan();
    let PlanAction::Auto { chosen, .. } = &p.items[at(&p, SUN)].action else {
        panic!()
    };
    let profile = common::profile("anbernic-stock");
    let img = w.env().candidate_image(&profile, chosen).unwrap();
    assert!(img.width <= 250 && img.height <= 360);
    assert_eq!(&img.bytes[..8], b"\x89PNG\r\n\x1a\n");
    w.env().candidate_image(&profile, chosen).unwrap();
    assert_eq!(
        w.server.hits(
            "/Nintendo%20-%20Game%20Boy%20Advance/Named_Boxarts/Golden%20Sun%20(USA%2C%20Europe).png"
        ),
        1
    );
}

#[test]
fn a_local_image_is_re_encoded_backed_up_and_undoable() {
    let w = world();
    let target = w.img(SUN);
    common::write(w.lib.path(), &format!("ROMS/GBA/Imgs/{SUN}.png"), "old art");
    let (job, outcome) = write_local_image(
        w.lib.path(),
        &common::profile("anbernic-stock"),
        Media::Cover,
        &target,
        &png_bytes(800, 800),
        w.data.path(),
    )
    .unwrap();
    assert!(!job.is_empty());
    assert!(
        matches!(outcome, WriteOutcome::Replaced { .. }),
        "{outcome:?}"
    );
    let (wd, ht) = probe_dimensions(&fs::read(&target).unwrap()).unwrap();
    assert!(wd <= 250 && ht <= 360);
    undo_last(w.data.path()).unwrap();
    assert_eq!(fs::read_to_string(&target).unwrap(), "old art");
}

#[test]
fn a_local_file_that_is_not_an_image_is_refused_before_writing() {
    let w = world();
    let target = w.img(SUN);
    let r = write_local_image(
        w.lib.path(),
        &common::profile("anbernic-stock"),
        Media::Cover,
        &target,
        b"<html>not an image</html>",
        w.data.path(),
    );
    assert!(matches!(r, Err(ArtError::Image(_))), "{r:?}");
    assert!(!target.exists());
    assert!(matches!(
        write_local_image(
            w.lib.path(),
            &common::profile("anbernic-stock"),
            Media::Screenshot,
            &target,
            &png_bytes(10, 10),
            w.data.path(),
        ),
        Err(ArtError::NoSlot)
    ));
}

#[test]
fn removed_art_is_backed_up_and_undo_brings_it_back() {
    let w = world();
    let target = w.img(SUN);
    common::write(w.lib.path(), &format!("ROMS/GBA/Imgs/{SUN}.png"), "old art");
    let (_, out) = remove_art(w.lib.path(), &target, w.data.path()).unwrap();
    assert!(matches!(out, RemoveOutcome::Removed { .. }));
    assert!(!target.exists());
    undo_last(w.data.path()).unwrap();
    assert_eq!(fs::read_to_string(&target).unwrap(), "old art");
}
