//! Organising a loose pile from the app backend: the probe, the plan store
//! (re-plans reuse the survey), cancelling, and the weak `system-folders`
//! detection when opening an organised folder. No Tauri runtime.

mod common;

use std::fs::{self, File};
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::{Duration, SystemTime};

use common::write;
use coverly_core::identify::sniff::fixtures;
use coverly_core::organize::{Choice, Decision, OrganizeAction, OrganizePhase};
use coverly_core::systems::SystemDb;
use coverly_roms_lib::error::IpcError;
use coverly_roms_lib::jobs::FillConfig;
use coverly_roms_lib::organize::view::{OrganizeProgressEvent, OrganizeRequest};
use coverly_roms_lib::organize::OrganizeEvents;
use coverly_roms_lib::state::AppState;
use tempfile::TempDir;

fn state(config: &TempDir) -> AppState {
    AppState::new(
        config.path().to_path_buf(),
        FillConfig::new(config.path().join("cache"), config.path().join("data")),
    )
}

/// Writes `bytes` to `root/rel`, dated an hour ago (never "still being
/// written").
fn write_old(root: &Path, rel: &str, bytes: &[u8]) {
    write(root, rel, bytes);
    let hour_ago = SystemTime::now() - Duration::from_secs(3600);
    File::options()
        .write(true)
        .open(root.join(rel))
        .unwrap()
        .set_modified(hour_ago)
        .unwrap();
}

/// A pile of three different Game Boy Advance ROMs (synthetic headers).
fn gba_pile() -> TempDir {
    let dir = TempDir::new().unwrap();
    let names = ["Alpha (USA).gba", "Beta (USA).gba", "Gamma (USA).gba"];
    for (tag, name) in b"ABG".iter().copied().zip(names) {
        write_old(dir.path(), name, &fixtures::gba_tagged(tag));
    }
    dir
}

fn offline(source: &Path) -> OrganizeRequest {
    OrganizeRequest {
        source: source.display().to_string(),
        offline: Some(true),
        ..OrganizeRequest::default()
    }
}

#[derive(Default)]
struct Recorder {
    events: Mutex<Vec<OrganizeProgressEvent>>,
    /// When set, the first event cancels its plan through this state.
    cancel_with: OnceLock<Weak<AppState>>,
}

impl Recorder {
    fn events(&self) -> Vec<OrganizeProgressEvent> {
        self.events.lock().unwrap().clone()
    }
}

impl OrganizeEvents for Recorder {
    fn progress(&self, event: OrganizeProgressEvent) {
        if let Some(st) = self.cancel_with.get().and_then(Weak::upgrade) {
            st.organize_cancel(event.plan_id).unwrap();
        }
        self.events.lock().unwrap().push(event);
    }
}

#[test]
fn probe_finds_rom_like_files_in_a_plain_folder() {
    let config = TempDir::new().unwrap();
    let st = state(&config);
    let pile = TempDir::new().unwrap();
    write(pile.path(), "Tetris (JUE) (V1.0) [!].zip", b"zip");
    write(pile.path(), "Tetris Worlds (USA).gba", b"gba");
    write(pile.path(), "Tetris 2.zip.crdownload", b"part");
    write(pile.path(), ".DS_Store", b"junk");

    let probe = st.probe_folder(pile.path()).unwrap();
    assert_eq!(probe.rom_like, 2);
    assert_eq!(probe.partial_downloads, 1);
    assert_eq!(probe.cloud_only, 0);
    assert!(probe.sorted_folders.is_empty());
    assert!(probe.writable);
    assert!(!probe.broad_root);
    assert!(!probe.truncated);
    let json = serde_json::to_value(&probe).unwrap();
    assert_eq!(json["rom_like"], 2);
    assert_eq!(json["sorted_folders"], serde_json::json!([]));

    assert!(matches!(
        st.probe_folder(&pile.path().join("missing")),
        Err(IpcError::NotFound(_))
    ));
    assert!(matches!(
        st.probe_folder(&pile.path().join("Tetris Worlds (USA).gba")),
        Err(IpcError::NotAFolder(_))
    ));
}

#[test]
fn a_plan_is_stored_and_replanning_with_decisions_reuses_the_survey() {
    let config = TempDir::new().unwrap();
    let st = state(&config);
    let pile = gba_pile();
    let req = offline(pile.path());

    let first_events = Arc::new(Recorder::default());
    let first = st.organize_plan(&req, &[], first_events.clone()).unwrap();
    let events = first_events.events();
    assert!(!events.is_empty());
    assert!(events.iter().all(|e| e.plan_id == first.plan_id));
    assert_eq!(events[0].phase, OrganizePhase::Walk);
    assert_eq!(first.plan.layout, "anbernic-stock");
    assert_eq!(first.plan.items.len(), 3);
    assert!(first
        .plan
        .items
        .iter()
        .all(|i| i.action == OrganizeAction::Auto && i.system_id.as_deref() == Some("gba")));
    assert_eq!(first.plan.counts.roms, 3);

    let second_events = Arc::new(Recorder::default());
    let leave = [Decision {
        item: 0,
        choice: Choice::Leave,
    }];
    let second = st
        .organize_plan(&req, &leave, second_events.clone())
        .unwrap();
    assert!(second_events.events().is_empty(), "the survey was reused");
    assert_ne!(second.plan_id, first.plan_id);
    assert!(second.plan.items[0].is_left());
    assert_eq!(second.plan.counts.roms, 2);

    // A dry run: nothing moved.
    assert!(pile.path().join("Alpha (USA).gba").is_file());
    assert!(!pile.path().join("ROMS").exists());

    // The wire shape: `plan_id` next to the plan as is.
    let json = serde_json::to_value(&second).unwrap();
    assert_eq!(json["plan_id"], second.plan_id);
    assert_eq!(json["plan"]["layout"], "anbernic-stock");

    // Both plans stay available (export) until newer ones push them out.
    let dest = config.path().join("plan.json");
    st.organize_export_plan(first.plan_id, &dest).unwrap();
    let saved: serde_json::Value = serde_json::from_slice(&fs::read(&dest).unwrap()).unwrap();
    assert_eq!(saved["layout"], "anbernic-stock");
    assert_eq!(saved["items"].as_array().unwrap().len(), 3);
    assert_eq!(st.organize_export_plan(999, &dest), Err(IpcError::NoPlan));

    // A different layout surveys again.
    let other = OrganizeRequest {
        layout: Some("batocera".into()),
        ..offline(pile.path())
    };
    let third_events = Arc::new(Recorder::default());
    let third = st.organize_plan(&other, &[], third_events.clone()).unwrap();
    assert!(!third_events.events().is_empty());
    assert_eq!(third.plan.layout, "batocera");

    // Bad requests keep their codes.
    let unknown = OrganizeRequest {
        layout: Some("no-such-layout".into()),
        ..offline(pile.path())
    };
    assert_eq!(
        st.organize_plan(&unknown, &[], Arc::new(Recorder::default())),
        Err(IpcError::UnknownProfile("no-such-layout".into()))
    );
    let bad_item = [Decision {
        item: 99,
        choice: Choice::Leave,
    }];
    assert!(matches!(
        st.organize_plan(&req, &bad_item, Arc::new(Recorder::default())),
        Err(IpcError::BadRequest(_))
    ));
    assert!(matches!(
        st.organize_plan(
            &offline(&pile.path().join("missing")),
            &[],
            Arc::new(Recorder::default())
        ),
        Err(IpcError::NotFound(_))
    ));
}

#[test]
fn keeps_only_the_last_four_plans() {
    let config = TempDir::new().unwrap();
    let st = state(&config);
    let pile = gba_pile();
    let req = offline(pile.path());
    let ids: Vec<u64> = (0..5)
        .map(|_| {
            st.organize_plan(&req, &[], Arc::new(Recorder::default()))
                .unwrap()
                .plan_id
        })
        .collect();
    let dest = config.path().join("plan.json");
    assert_eq!(
        st.organize_export_plan(ids[0], &dest),
        Err(IpcError::NoPlan)
    );
    for id in &ids[1..] {
        st.organize_export_plan(*id, &dest).unwrap();
    }
}

#[test]
fn cancelling_a_plan_in_progress_returns_cancelled() {
    let config = TempDir::new().unwrap();
    let st = Arc::new(state(&config));
    let pile = gba_pile();
    let events = Arc::new(Recorder::default());
    events.cancel_with.set(Arc::downgrade(&st)).unwrap();

    let out = st.organize_plan(&offline(pile.path()), &[], events.clone());
    assert_eq!(out, Err(IpcError::Cancelled));
    let plan_id = events.events()[0].plan_id;
    // Nothing was stored, and cancelling again (or an unknown plan) is harmless.
    assert_eq!(
        st.organize_export_plan(plan_id, &config.path().join("plan.json")),
        Err(IpcError::NoPlan)
    );
    st.organize_cancel(plan_id).unwrap();
    st.organize_cancel(12345).unwrap();

    // The next plan runs normally.
    let next = st
        .organize_plan(&offline(pile.path()), &[], Arc::new(Recorder::default()))
        .unwrap();
    assert!(next.plan_id > plan_id);
}

#[test]
fn opening_an_organised_folder_without_markers_suggests_the_default_layout() {
    let config = TempDir::new().unwrap();
    let st = state(&config);
    let dir = TempDir::new().unwrap();
    write(dir.path(), "ROMS/GBA/a.gba", b"x");

    let view = st
        .open(dir.path(), None, &SystemDb::builtin(), &mut |_| {})
        .unwrap();
    assert_eq!(view.profile.id, "anbernic-stock");
    assert!(view.profile.detected);
    assert_eq!(view.profile.evidence, vec!["system-folders".to_string()]);
    assert_eq!(view.entries.len(), 1);
}

#[test]
fn the_weak_rule_follows_the_organise_default_layout() {
    let config = TempDir::new().unwrap();
    let st = state(&config);
    let out = st
        .update_settings(&serde_json::json!({"organize": {"default_layout": "batocera"}}))
        .unwrap();
    assert!(out.errors.is_empty(), "{:?}", out.errors);
    let dir = TempDir::new().unwrap();
    write(dir.path(), "ROMs/gba/a.gba", b"x");

    let view = st
        .open(dir.path(), None, &SystemDb::builtin(), &mut |_| {})
        .unwrap();
    assert_eq!(view.profile.id, "batocera");
    assert_eq!(view.profile.evidence, vec!["system-folders".to_string()]);
}

#[test]
fn a_plain_pile_still_answers_no_profile() {
    let config = TempDir::new().unwrap();
    let st = state(&config);
    let pile = gba_pile();
    let err = st
        .open(pile.path(), None, &SystemDb::builtin(), &mut |_| {})
        .unwrap_err();
    assert!(matches!(err, IpcError::NoProfile(_)));
    assert_eq!(
        serde_json::to_value(&err).unwrap()["code"],
        serde_json::json!("no-profile")
    );
}

fn custom_layout(roms_root: &str) -> coverly_core::profiles::custom::ProfileFile {
    use coverly_core::profiles::custom::{ProfileFile, SlotFile};
    use coverly_core::profiles::{FolderNaming, ImageFormat};
    ProfileFile {
        id: "mine".into(),
        name: "Mine".into(),
        roms_root: roms_root.into(),
        art_dirs: vec!["covers".into()],
        gamelist: false,
        slots: std::collections::BTreeMap::from([(
            "cover".into(),
            SlotFile {
                template: "{system_dir}/covers/{stem}.png".into(),
                format: ImageFormat::Png,
                max_width: None,
                max_height: None,
            },
        )]),
        system_folder: FolderNaming::default(),
        folder_names: std::collections::BTreeMap::new(),
        host_folders: std::collections::BTreeMap::new(),
    }
}

#[test]
fn editing_the_chosen_custom_layout_surveys_again() {
    let config = TempDir::new().unwrap();
    let st = state(&config);
    let saved = st.save_profile(custom_layout("Games"), None).unwrap();
    assert!(saved.errors.is_empty(), "{:?}", saved.errors);
    let pile = gba_pile();
    let req = OrganizeRequest {
        layout: Some("mine".into()),
        ..offline(pile.path())
    };
    let first = st
        .organize_plan(&req, &[], Arc::new(Recorder::default()))
        .unwrap();
    assert!(
        first.plan.roms_dir.ends_with("Games"),
        "{:?}",
        first.plan.roms_dir
    );

    // Same id, other layout: a kept survey (walked with the old folders)
    // must not be reused.
    let saved = st
        .save_profile(custom_layout("Library"), Some("mine".into()))
        .unwrap();
    assert!(saved.errors.is_empty(), "{:?}", saved.errors);
    let events = Arc::new(Recorder::default());
    let second = st.organize_plan(&req, &[], events.clone()).unwrap();
    assert!(!events.events().is_empty(), "surveyed again");
    assert!(
        second.plan.roms_dir.ends_with("Library"),
        "{:?}",
        second.plan.roms_dir
    );
}
