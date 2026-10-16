//! The M4 exit check through the app's job path (not the GUI) on a COPY of
//! real systems, with the real art servers. Run by hand:
//!
//! ```text
//! COVERLY_FILL_COPY=$SC/m4-copy COVERLY_FILL_WORK=$SC/m4-work \
//!   cargo test -p coverly-roms --release --test real_fill -- --ignored --nocapture
//! ```
//!
//! Writes only inside the copy and the work folder.

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
use coverly_core::settings::LIBRARY_SETTINGS_FILE;
use coverly_core::systems::SystemDb;
use coverly_core::write::{sha1_hex, OverwritePolicy};
use coverly_roms_lib::jobs::view::{
    ActionKind, FillRequest, ItemFilter, JobProgressEvent, JobState, JobView,
};
use coverly_roms_lib::jobs::{FillConfig, JobEvents};
use coverly_roms_lib::library::view::ArtMark;
use coverly_roms_lib::state::AppState;

struct Refreshing {
    state: Arc<AppState>,
    rec: Recorder,
}

impl JobEvents for Refreshing {
    fn progress(&self, event: JobProgressEvent) {
        self.rec.progress(event);
    }
    fn changed(&self, view: JobView) {
        self.rec.changed(view);
    }
    fn written(&self, generation: u64, ids: Vec<u32>) {
        let _ = self
            .state
            .refresh_entries(generation, &ids, &SystemDb::builtin());
        self.rec.written(generation, ids);
    }
}

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
#[ignore = "real network and a copy of real ROMs: run by hand (M4 Task 13)"]
fn fills_a_copy_auto_only_and_undo_restores_it() {
    let copy = PathBuf::from(std::env::var("COVERLY_FILL_COPY").expect("COVERLY_FILL_COPY"));
    let work = PathBuf::from(std::env::var("COVERLY_FILL_WORK").expect("COVERLY_FILL_WORK"));
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
        work.join(LIBRARY_SETTINGS_FILE),
        FillConfig::new(work.join("cache"), work.join("data")),
    ));
    let view = st
        .open(&copy, None, &SystemDb::builtin(), &mut |_| {})
        .unwrap();
    let missing_before = view
        .entries
        .iter()
        .filter(|e| e.art.cover == ArtMark::Missing)
        .count();
    println!(
        "opened {} ({}): {} ROMs, {missing_before} without cover",
        view.root,
        view.profile.id,
        view.entries.len()
    );
    let ev = Arc::new(Refreshing {
        state: st.clone(),
        rec: Recorder::default(),
    });

    let t0 = Instant::now();
    let request = FillRequest {
        entry_ids: None,
        media: vec![Media::Cover],
        strategy: StrategyKind::RomRegion,
        regions: ["rom", "world", "usa", "europe", "japan"]
            .map(String::from)
            .to_vec(),
        overwrite: OverwritePolicy::Never,
    };
    let id = st
        .start_fill(view.generation, &request, ev.clone())
        .unwrap()
        .id;
    let planned = wait(&ev.rec, JobState::Planned, Duration::from_secs(900));
    let plan = planned.plan.clone().unwrap();
    println!(
        "dry run {:.1} s: auto {} review {} none {}",
        t0.elapsed().as_secs_f64(),
        plan.totals.auto,
        plan.totals.review,
        plan.totals.none
    );
    for s in &plan.per_system {
        println!(
            "  {}: auto {} review {} none {}",
            s.folder, s.auto, s.review, s.none
        );
    }
    assert_eq!(tree(&copy), before, "the dry run writes nothing");
    for item in st.jobs().items(id, ItemFilter::All).unwrap() {
        if item.name.ends_with("Final Fantasy IV") || item.name.ends_with("Contra Advance") {
            println!(
                "  M2-11 {}: {:?} {:?} -> {:?}",
                item.name,
                item.action,
                item.note,
                item.candidates.first().map(|c| c.name.clone())
            );
            assert_ne!(
                item.action,
                ActionKind::Auto,
                "{} must not be auto",
                item.name
            );
        }
    }

    let t1 = Instant::now();
    st.jobs().run(st.fill_config(), id, ev.clone()).unwrap();
    let done = wait(&ev.rec, JobState::Done, Duration::from_secs(1800));
    let result = done.result.unwrap();
    println!(
        "apply (auto only) {:.1} s: filled {} replaced {} skipped {} no-art {} failed {} journal {:?}",
        t1.elapsed().as_secs_f64(),
        result.filled,
        result.replaced,
        result.skipped,
        result.no_art,
        result.failed,
        done.journal
    );
    let after = tree(&copy);
    let written: Vec<&PathBuf> = after.keys().filter(|p| !before.contains_key(*p)).collect();
    assert_eq!(written.len(), result.filled, "one new file per filled item");
    for p in &written {
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        assert!(
            !name.starts_with("._") && !name.starts_with(".coverly-"),
            "{name}"
        );
        let bytes = fs::read(copy.join(p)).unwrap();
        assert!(
            bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
            "{} is not a PNG",
            p.display()
        );
        let (w, h) = probe_dimensions(&bytes).unwrap();
        assert!(w <= 250 && h <= 360, "{} is {w}x{h}", p.display());
    }
    let session = st.session(view.generation).unwrap();
    let missing_after = session
        .entries
        .iter()
        .filter(|e| !e.art.contains_key(&Media::Cover))
        .count();
    println!("without cover: {missing_before} -> {missing_after}");
    assert_eq!(missing_after, missing_before - result.filled);

    let undo = st.undo_last_job(view.generation).unwrap();
    println!(
        "undo {}: removed {} restored {} kept {}",
        undo.job, undo.removed, undo.restored, undo.kept
    );
    assert_eq!(tree(&copy), before, "undo restores the copy byte for byte");
}
