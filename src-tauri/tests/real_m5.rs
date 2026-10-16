//! M5 exit check through the app's real core path on a COPY (never the
//! source, never /Volumes): an ignored folder and a max-size setting
//! change what a dry run and a fill do; Undo restores the copy. Run by
//! hand with the real art servers:
//!
//! ```text
//! COVERLY_M5_COPY=$SC/m5-copy COVERLY_M5_WORK=$SC/m5-work \
//!   cargo test -p coverly-roms --release --test real_m5 -- --ignored --nocapture
//! ```

mod common;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use common::fill::Recorder;
use coverly_core::image::probe_dimensions;
use coverly_core::matching::StrategyKind;
use coverly_core::profiles::Media;
use coverly_core::systems::SystemDb;
use coverly_core::write::{sha1_hex, OverwritePolicy};
use coverly_roms_lib::jobs::view::{FillRequest, JobState, JobView};
use coverly_roms_lib::jobs::FillConfig;
use coverly_roms_lib::state::AppState;
use serde_json::json;

/// SHA-1 of every file under `root`, by relative path.
fn tree(root: &Path) -> BTreeMap<PathBuf, String> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<PathBuf, String>) {
        for e in fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(root, &p, out);
            } else {
                let rel = p.strip_prefix(root).unwrap().to_path_buf();
                out.insert(rel, sha1_hex(&fs::read(&p).unwrap()));
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

/// `common::fill::wait_for` with a limit long enough for the real servers.
fn wait(rec: &Recorder, state: JobState, limit: Duration) -> JobView {
    let start = Instant::now();
    loop {
        if let Some(v) = rec.changed.lock().unwrap().last() {
            if v.state == state {
                return v.clone();
            }
            assert!(
                !matches!(v.state, JobState::Paused | JobState::Failed),
                "the job stopped: {v:?}"
            );
        }
        assert!(start.elapsed() < limit, "timed out waiting for {state:?}");
        std::thread::sleep(Duration::from_millis(200));
    }
}

#[test]
#[ignore = "real network and a copy of real ROMs: run by hand (M5 Task 16)"]
fn settings_change_a_real_dry_run_and_fill_on_a_copy() {
    let copy = PathBuf::from(std::env::var("COVERLY_M5_COPY").expect("COVERLY_M5_COPY"));
    let work = PathBuf::from(std::env::var("COVERLY_M5_WORK").expect("COVERLY_M5_WORK"));
    let home = PathBuf::from(std::env::var("HOME").unwrap_or_default());
    for forbidden in [PathBuf::from("/Volumes"), home.join("Documents/ROMS")] {
        assert!(
            !copy.starts_with(&forbidden) && !work.starts_with(&forbidden),
            "refusing to write under {}",
            forbidden.display()
        );
    }
    let before = tree(&copy);
    let st = Arc::new(AppState::new(
        work.join("config"),
        FillConfig::new(work.join("cache"), work.join("data")),
    ));
    let v0 = st
        .open(&copy, None, &SystemDb::builtin(), &mut |_| {})
        .unwrap();
    let junk = v0
        .entries
        .iter()
        .filter(|e| e.rel_path.split('/').any(|c| c == "Junk"))
        .count();
    println!(
        "default settings ({}): {} ROMs ({junk} under Junk/)",
        v0.profile.id,
        v0.entries.len()
    );
    assert!(
        junk > 0,
        "prepare the copy with a Junk/ subfolder holding a few ROMs"
    );

    let out = st
        .update_settings(&json!({
            "library": {"ignored_dirs": ["bios", "saves", "states", "Shortcuts", "Junk"]},
            "art": {"max_size": {"anbernic-stock": {"width": 120, "height": 160}}, "media": ["cover"]}
        }))
        .unwrap();
    assert!(out.errors.is_empty(), "{:?}", out.errors);
    assert!(out.reopen, "an ignored folder asks for a reopen");
    let v1 = st
        .open(&copy, None, &SystemDb::builtin(), &mut |_| {})
        .unwrap();
    println!("ignored Junk/: {} ROMs", v1.entries.len());
    assert_eq!(v1.entries.len(), v0.entries.len() - junk);

    let rec = Arc::new(Recorder::default());
    let request = FillRequest {
        entry_ids: None,
        media: vec![Media::Cover],
        strategy: StrategyKind::RomRegion,
        // Empty: the settings' region order is used.
        regions: Vec::new(),
        overwrite: OverwritePolicy::Never,
    };
    let t0 = Instant::now();
    let job = st.start_fill(v1.generation, &request, rec.clone()).unwrap();
    let planned = wait(&rec, JobState::Planned, Duration::from_secs(900));
    let totals = planned.plan.as_ref().unwrap().totals;
    println!(
        "dry run {:.1} s: auto {} review {} none {}",
        t0.elapsed().as_secs_f64(),
        totals.auto,
        totals.review,
        totals.none
    );
    assert_eq!(tree(&copy), before, "the dry run writes nothing");
    st.jobs().run(job.id, rec.clone()).unwrap();
    let done = wait(&rec, JobState::Done, Duration::from_secs(1800));
    let result = done.result.unwrap();
    println!(
        "filled {} replaced {} skipped {} no-art {} failed {}",
        result.filled, result.replaced, result.skipped, result.no_art, result.failed
    );
    let after = tree(&copy);
    let mut written = 0;
    for path in after.keys().filter(|p| !before.contains_key(*p)) {
        assert!(
            !path.components().any(|c| c.as_os_str() == "Junk"),
            "{} is in an ignored folder",
            path.display()
        );
        let (w, h) = probe_dimensions(&fs::read(copy.join(path)).unwrap()).unwrap();
        assert!(w <= 120 && h <= 160, "{} is {w}x{h}", path.display());
        written += 1;
    }
    println!("{written} new files, all within 120x160");
    assert!(written > 0);
    let undo = st.undo_last_job(v1.generation).unwrap();
    println!(
        "undo {}: removed {} restored {} kept {}",
        undo.job, undo.removed, undo.restored, undo.kept
    );
    assert_eq!(tree(&copy), before, "undo restores the copy exactly");
}
