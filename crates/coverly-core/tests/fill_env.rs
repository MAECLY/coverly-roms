mod common;

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use coverly_core::art::resolve_art;
use coverly_core::cache::Cache;
use coverly_core::fill::{
    remove_art, write_local_image, ArtError, Endpoints, FillEnv, FillSettings, ManualOptions,
    KEEP_JOBS,
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
use coverly_core::write::{list_jobs, undo_last, RemoveOutcome, WriteError, WriteOutcome};
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
        &ManualOptions::default(),
    )
    .unwrap();
    assert!(job.is_some());
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
        &ManualOptions::default(),
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
            &ManualOptions::default(),
        ),
        Err(ArtError::NoSlot)
    ));
}

#[test]
fn removed_art_is_backed_up_and_undo_brings_it_back() {
    let w = world();
    let target = w.img(SUN);
    common::write(w.lib.path(), &format!("ROMS/GBA/Imgs/{SUN}.png"), "old art");
    let (_, out) = remove_art(w.lib.path(), &target, w.data.path(), KEEP_JOBS).unwrap();
    assert!(matches!(out, RemoveOutcome::Removed { .. }));
    assert!(!target.exists());
    undo_last(w.data.path()).unwrap();
    assert_eq!(fs::read_to_string(&target).unwrap(), "old art");
}

/// A manual write that wrote `target` (job of its own), for the tests below.
fn real_job(w: &World, target: &std::path::Path) -> String {
    let (job, outcome) = write_local_image(
        w.lib.path(),
        &common::profile("anbernic-stock"),
        Media::Cover,
        target,
        &png_bytes(100, 100),
        w.data.path(),
        &ManualOptions::default(),
    )
    .unwrap();
    assert!(
        matches!(outcome, WriteOutcome::Created { .. }),
        "{outcome:?}"
    );
    job.unwrap()
}

fn job_ids(w: &World) -> Vec<String> {
    list_jobs(w.data.path())
        .unwrap()
        .into_iter()
        .map(|j| j.job)
        .collect()
}

#[test]
fn refused_or_no_op_manual_writes_leave_no_job_behind() {
    let w = world();
    let target = w.img(SUN);
    let job = real_job(&w, &target);
    let local = |t: &std::path::Path| {
        write_local_image(
            w.lib.path(),
            &common::profile("anbernic-stock"),
            Media::Cover,
            t,
            &png_bytes(100, 100),
            w.data.path(),
            &ManualOptions::default(),
        )
    };
    let outside = w.data.path().join("evil.png");
    assert!(matches!(
        local(&outside),
        Err(ArtError::Write(WriteError::OutsideRoot(_)))
    ));
    assert!(matches!(
        local(&w.img("a?b")),
        Err(ArtError::Write(WriteError::NameInvalid(_)))
    ));
    // A folder in the way is never replaced: nothing written.
    let folder = w.img(RUSH);
    fs::create_dir_all(&folder).unwrap();
    assert!(matches!(
        local(&folder),
        Ok((None, WriteOutcome::SkippedExists { .. }))
    ));
    assert!(matches!(
        remove_art(w.lib.path(), &folder, w.data.path(), KEEP_JOBS),
        Err(WriteError::NotAFile(_))
    ));
    assert!(matches!(
        remove_art(
            w.lib.path(),
            &w.img("Nothing Here"),
            w.data.path(),
            KEEP_JOBS
        ),
        Ok((None, RemoveOutcome::Missing))
    ));
    assert!(matches!(
        remove_art(w.lib.path(), &outside, w.data.path(), KEEP_JOBS),
        Err(WriteError::OutsideRoot(_))
    ));
    assert_eq!(job_ids(&w), std::slice::from_ref(&job));

    // Undo reverts the real job, not a phantom one.
    assert_eq!(undo_last(w.data.path()).unwrap().job, job);
    assert!(!target.exists());
}

#[test]
fn failed_manual_writes_never_prune_a_real_job() {
    let w = world();
    let job = real_job(&w, &w.img(SUN));
    for _ in 0..(coverly_core::fill::KEEP_JOBS + 2) {
        let _ = remove_art(
            w.lib.path(),
            &w.img("Nothing Here"),
            w.data.path(),
            KEEP_JOBS,
        );
        let _ = remove_art(
            w.lib.path(),
            &w.data.path().join("x.png"),
            w.data.path(),
            KEEP_JOBS,
        );
    }
    assert_eq!(job_ids(&w), [job]);
}

#[cfg(unix)]
#[test]
fn a_read_only_art_folder_leaves_no_job_and_no_backup() {
    use std::os::unix::fs::PermissionsExt;
    let w = world();
    let job = real_job(&w, &w.img(SUN));
    let imgs = w.lib.path().join("ROMS/GBA/Imgs");
    fs::set_permissions(&imgs, fs::Permissions::from_mode(0o555)).unwrap();
    let new = write_local_image(
        w.lib.path(),
        &common::profile("anbernic-stock"),
        Media::Cover,
        &w.img(RUSH),
        &png_bytes(100, 100),
        w.data.path(),
        &ManualOptions::default(),
    );
    // Replacing backs the old art up before the write is refused.
    let replace = write_local_image(
        w.lib.path(),
        &common::profile("anbernic-stock"),
        Media::Cover,
        &w.img(SUN),
        &png_bytes(120, 120),
        w.data.path(),
        &ManualOptions::default(),
    );
    let removed = remove_art(w.lib.path(), &w.img(SUN), w.data.path(), KEEP_JOBS);
    fs::set_permissions(&imgs, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        matches!(new, Err(ArtError::Write(WriteError::Denied(_)))),
        "{new:?}"
    );
    assert!(
        matches!(replace, Err(ArtError::Write(WriteError::Denied(_)))),
        "{replace:?}"
    );
    assert!(matches!(removed, Err(WriteError::Denied(_))), "{removed:?}");
    assert!(w.img(SUN).exists());
    assert_eq!(job_ids(&w), [job]);
    let backups: Vec<_> = fs::read_dir(w.data.path().join("backups"))
        .map(|r| r.flatten().map(|e| e.file_name()).collect())
        .unwrap_or_default();
    assert!(backups.is_empty(), "{backups:?}");
}

#[test]
fn a_cancel_is_marked_in_the_report_but_not_in_the_plan() {
    let w = world();
    let profile = common::profile("anbernic-stock");
    let db = SystemDb::builtin();
    let scan = scan_library(w.lib.path(), &profile, &db, &ScanOptions::default());
    let art = resolve_art(&scan, &profile, &db, &[Media::Cover]);
    let cancel = AtomicBool::new(true);
    let p = w.env().plan(
        w.lib.path(),
        &profile,
        &db,
        &art,
        &FillSettings::default(),
        &NoProgress,
        &cancel,
    );
    // The caller tells a cancel apart by its own flag (FillEnv::plan doc).
    assert_eq!(p.paused, None);
    assert!(p.items.is_empty());

    let full = w.plan();
    let r = w
        .env()
        .apply(
            &full,
            &profile,
            &ApplyOptions::default(),
            &NoProgress,
            &cancel,
        )
        .unwrap();
    assert_eq!(r.paused, None);
    assert!(r.cancelled);
    assert!(r.items.iter().all(|i| i.status == ItemStatus::NotAttempted));
    assert!(!w.lib.path().join("ROMS/GBA/Imgs").exists());
}

#[test]
fn thresholds_and_fuzzy_from_settings_reach_the_plan() {
    let w = world();
    let profile = common::profile("anbernic-stock");
    let db = SystemDb::builtin();
    let scan = scan_library(w.lib.path(), &profile, &db, &ScanOptions::default());
    let art = resolve_art(&scan, &profile, &db, &[Media::Cover]);
    let mut s = coverly_core::settings::Settings::default();
    s.matching.auto_threshold = 1.0;
    s.matching.fuzzy = false;
    s.art.media = vec![Media::Cover];
    let plan = w.env().plan(
        w.lib.path(),
        &profile,
        &db,
        &art,
        &s.fill_settings(),
        &NoProgress,
        &AtomicBool::new(false),
    );
    let sun = &plan.items[at(&plan, SUN)];
    assert!(
        matches!(sun.action, PlanAction::Review { .. }),
        "a title match (0.95) is below an auto threshold of 1.0: {:?}",
        sun.action
    );
    let rush = &plan.items[at(&plan, RUSH)];
    assert!(
        matches!(
            rush.action,
            PlanAction::None {
                reason: NoneReason::NoCandidates
            }
        ),
        "only a fuzzy match exists for `Gldn Rsh`: {:?}",
        rush.action
    );
}

#[test]
fn apply_prunes_to_the_undo_depth_and_cleans_appledouble_junk() {
    let w = world();
    let profile = common::profile("anbernic-stock");
    // Three earlier jobs.
    for i in 0..3 {
        let mut writer = coverly_core::write::Writer::begin(w.lib.path(), w.data.path()).unwrap();
        writer
            .write(
                &w.lib.path().join(format!("ROMS/GBA/Imgs/old{i}.png")),
                b"x",
                coverly_core::write::OverwritePolicy::Never,
            )
            .unwrap();
        writer.finish().unwrap();
    }
    fs::create_dir_all(w.lib.path().join("ROMS/GBA/Imgs")).unwrap();
    fs::write(
        w.lib.path().join("ROMS/GBA/Imgs/._junk.png"),
        [0x00, 0x05, 0x16, 0x07, 0, 2, 0, 0],
    )
    .unwrap();
    let plan = w.plan();
    let opts = ApplyOptions {
        keep_jobs: 2,
        clean_apple_double: true,
        include_review: false,
        ..ApplyOptions::default()
    };
    w.env()
        .apply(&plan, &profile, &opts, &NoProgress, &AtomicBool::new(false))
        .unwrap();
    assert_eq!(list_jobs(w.data.path()).unwrap().len(), 2);
    assert!(!w.lib.path().join("ROMS/GBA/Imgs/._junk.png").exists());
}
