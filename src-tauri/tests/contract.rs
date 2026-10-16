//! The wire contract shared with `src/ipc/types.ts`: Rust must serialise
//! exactly `src/ipc/contract.sample.json`.

use coverly_core::credentials::CredentialKey;
use coverly_core::identify::names::Region;
use coverly_core::identify::IdMethod;
use coverly_core::jobs::ItemErrorKind;
use coverly_core::matching::{PickReason, StrategyKind};
use coverly_core::plan::{Counts, Decision, PlanNote};
use coverly_core::profiles::Media;
use coverly_core::progress::{PauseKind, Phase};
use coverly_core::sources::{KeyCheck, MatchKind, Votes};
use coverly_core::volumes::Volume;
use coverly_core::write::OverwritePolicy;
use coverly_roms_lib::error::IpcError;
use coverly_roms_lib::jobs::view::{
    ActionKind, CandidateView, DecisionIn, EntriesUpdatedEvent, ExportFormat, FillRequest,
    ItemFilter, ItemOutcomeView, JobHistoryView, JobItemView, JobProgressEvent, JobProgressView,
    JobState, JobTotalsView, JobView, OutcomeKind, PauseView, PlanSummaryView, SystemCountsView,
    UndoView,
};
use coverly_roms_lib::library::identify::IdentityView;
use coverly_roms_lib::library::refresh::EntryArtUpdate;
use coverly_roms_lib::library::view::{
    ArtMark, ArtMarks, EntryView, LibrarySettingsView, LibraryView, ProfileView, RecentView,
    ScanProgressEvent, SystemView, WarningView,
};
use coverly_roms_lib::sources::SourceInfoView;
use coverly_roms_lib::update::{
    UpdateCheckView, UpdateInstallView, UpdateOfferView, UpdateProgressEvent,
};
use serde_json::{json, Value};

/// Every item error kind, in declaration order.
const ALL_ITEM_ERROR_KINDS: [ItemErrorKind; 9] = [
    ItemErrorKind::NoArt,
    ItemErrorKind::Network,
    ItemErrorKind::Decode,
    ItemErrorKind::WriteDenied,
    ItemErrorKind::DiskFull,
    ItemErrorKind::NameInvalid,
    ItemErrorKind::OutsideRoot,
    ItemErrorKind::VolumeGone,
    ItemErrorKind::Io,
];

/// Fails to compile when a new kind is added, so the list above (and the
/// TypeScript union) gets it too.
#[allow(dead_code)]
fn every_item_error_kind_is_listed(k: ItemErrorKind) {
    match k {
        ItemErrorKind::NoArt
        | ItemErrorKind::Network
        | ItemErrorKind::Decode
        | ItemErrorKind::WriteDenied
        | ItemErrorKind::DiskFull
        | ItemErrorKind::NameInvalid
        | ItemErrorKind::OutsideRoot
        | ItemErrorKind::VolumeGone
        | ItemErrorKind::Io => {}
    }
}

#[test]
fn rust_serialises_the_shared_sample() {
    let view = LibraryView {
        generation: 3,
        root: "/Volumes/SD".into(),
        name: "SD".into(),
        profile: ProfileView {
            id: "anbernic-stock".into(),
            name: "Anbernic stock".into(),
            media: vec![Media::Cover],
            detected: true,
            evidence: vec!["anbernic/ folder".into()],
            fitted: false,
            gamelist: false,
        },
        systems: vec![SystemView {
            index: 0,
            folder: "SFC".into(),
            dir: "ROMS/SFC".into(),
            system_id: Some("snes".into()),
            name: "Super Nintendo".into(),
            roms: 1,
        }],
        entries: vec![EntryView {
            id: 0,
            system: 0,
            rel_path: "ROMS/SFC/Zelda (USA).sfc".into(),
            dir: "ROMS/SFC".into(),
            name: "Zelda (USA)".into(),
            ext: "sfc".into(),
            size: 1_048_576,
            mtime: 1_700_000_000,
            regions: vec![Region::Usa],
            art: ArtMarks {
                cover: ArtMark::Present,
                screenshot: ArtMark::Unsupported,
                title: ArtMark::Unsupported,
            },
        }],
        warnings: vec![WarningView {
            path: "/Volumes/SD/ROMS/BAD".into(),
            message: "permission denied".into(),
        }],
        scan_ms: 42,
    };
    let actual = json!({
        "library_view": view,
        "scan_progress": ScanProgressEvent { generation: 3, done: 1, total: 2, folder: "SFC".into(), roms: 1 },
        "recent": RecentView {
            path: "/Volumes/SD".into(),
            name: "SD".into(),
            profile_id: "anbernic-stock".into(),
            roms: 1,
            opened_at: 1_700_000_000,
            available: true,
        },
        "library_settings": LibrarySettingsView { ignored_dirs: vec!["bios".into()] },
        "identity": IdentityView {
            canonical: Some("Legend of Zelda, The (USA)".into()),
            method: IdMethod::Hash,
            confidence: 1.0,
            title: "Legend of Zelda, The".into(),
            regions: vec![Region::Usa],
            offline: false,
        },
        "job_view": JobView {
            id: 4,
            generation: 3,
            state: JobState::Paused,
            progress: Some(JobProgressView { phase: Phase::Apply, done: 2, total: 5, label: "Zelda (USA)".into() }),
            paused: Some(PauseView {
                phase: Phase::Apply,
                kind: PauseKind::RateLimit,
                detail: "rate limited by thumbnails.libretro.com (HTTP 429)".into(),
            }),
            plan: Some(PlanSummaryView {
                totals: Counts { auto: 3, review: 1, none: 1 },
                per_system: vec![SystemCountsView { folder: "SFC".into(), auto: 3, review: 1, none: 1 }],
                warnings: vec![],
            }),
            decided: 1,
            skipped: 0,
            journal: Some("20261016T050401Z-1bfb-0000".into()),
            result: Some(JobTotalsView { filled: 2, replaced: 0, skipped: 2, no_art: 0, failed: 1 }),
            error: None,
        },
        "job_item": JobItemView {
            index: 0,
            entry_id: Some(0),
            folder: "SFC".into(),
            name: "Zelda (USA)".into(),
            media: Media::Cover,
            action: ActionKind::Review,
            confidence: Some(0.5),
            note: Some(PlanNote::UnreleasedVariant),
            none_reason: None,
            candidates: vec![CandidateView {
                index: 0,
                source: "libretro".into(),
                name: "Zelda (USA) (Beta)".into(),
                regions: vec![Region::Usa],
                kind: MatchKind::TitleExact,
                score: 95,
                confidence: 0.5,
                votes: Some(Votes { up: 8, down: 1 }),
                origin: Some("game 5248 · grid 80211".into()),
                reason: Some(PickReason::Votes { score: 7, next: 2 }),
            }],
            replaces_existing: false,
            decision: Some(Decision::Pick { candidate: 0 }),
            outcome: Some(ItemOutcomeView {
                status: OutcomeKind::Failed,
                error: Some(ItemErrorKind::WriteDenied),
                message: Some("write denied".into()),
            }),
        },
        "job_progress": JobProgressEvent { job: 4, phase: Phase::Gather, done: 1, total: 5, label: "Zelda (USA)".into() },
        "entries_updated": EntriesUpdatedEvent {
            generation: 3,
            entries: vec![EntryArtUpdate {
                id: 0,
                art: ArtMarks { cover: ArtMark::Present, screenshot: ArtMark::Unsupported, title: ArtMark::Unsupported },
            }],
        },
        "undo": UndoView { job: "20261016T050401Z-1bfb-0000".into(), removed: 2, restored: 1, kept: 0, entries: vec![] },
        "history_item": JobHistoryView {
            job: "20261016T050401Z-1bfb-0000".into(),
            root: "/Volumes/SD".into(),
            created_at: "2026-10-16T05:04:01Z".into(),
            entries: 3,
            complete: true,
            undone: false,
        },
        "error": IpcError::NotFound("/Volumes/SD".into()),
        "item_error_kinds": ALL_ITEM_ERROR_KINDS,
        "source_info": SourceInfoView { id: "steamgriddb".into(), built: true, needs: vec![CredentialKey::SteamgriddbApiKey], ready: false },
        "key_checks": [KeyCheck::Ok, KeyCheck::Invalid, KeyCheck::Missing, KeyCheck::Offline, KeyCheck::RateLimited, KeyCheck::Unavailable],
        "update_check_available": UpdateCheckView::Available {
            current: "0.1.0".into(),
            offer: UpdateOfferView { version: "0.2.0".into(), notes: Some("Faster scans.".into()), date: Some("2026-10-16T00:00:00Z".into()) },
        },
        "update_check_up_to_date": UpdateCheckView::UpToDate { current: "0.2.0".into(), latest: "0.2.0".into() },
        "update_check_skipped": UpdateCheckView::Skipped,
        "update_progress": UpdateProgressEvent { downloaded: 512, total: Some(1024) },
        "update_install": UpdateInstallView { restart_required: true },
        "volume": Volume { path: "/Volumes/SD".into(), name: "SD".into() },
        "pick_reasons": [PickReason::Only, PickReason::Consensus { count: 3, next: 1 }, PickReason::SourceOrder { source: "libretro".into() }, PickReason::Region { rom: true }, PickReason::User],
    });
    let expected: Value =
        serde_json::from_str(include_str!("../../src/ipc/contract.sample.json")).unwrap();
    // The settings samples are checked by `settings_samples_match_rust`.
    for (key, value) in actual.as_object().unwrap() {
        assert_eq!(value, &expected[key.as_str()], "sample `{key}`");
    }
}

#[test]
fn rust_reads_the_shared_requests() {
    let req: Value =
        serde_json::from_str(include_str!("../../src/ipc/contract.requests.json")).unwrap();
    let one: FillRequest = serde_json::from_value(req["fill_request"].clone()).unwrap();
    assert_eq!(one.entry_ids, Some(vec![0, 2]));
    assert_eq!(one.strategy, StrategyKind::RomRegion);
    assert_eq!(one.overwrite, OverwritePolicy::Never);
    let all: FillRequest = serde_json::from_value(req["fill_request_all"].clone()).unwrap();
    assert_eq!(all.entry_ids, None);
    assert_eq!(all.media, vec![Media::Cover, Media::Title]);
    assert_eq!(all.overwrite, OverwritePolicy::Backup);
    let decisions: Vec<DecisionIn> = serde_json::from_value(req["decisions"].clone()).unwrap();
    assert_eq!(
        decisions,
        vec![
            DecisionIn {
                index: 0,
                decision: Decision::Accept
            },
            DecisionIn {
                index: 1,
                decision: Decision::Pick { candidate: 2 }
            },
            DecisionIn {
                index: 2,
                decision: Decision::Skip
            },
        ]
    );
    let filters: Vec<ItemFilter> = serde_json::from_value(req["item_filters"].clone()).unwrap();
    assert_eq!(filters.len(), 4);
    let formats: Vec<ExportFormat> = serde_json::from_value(req["export_formats"].clone()).unwrap();
    assert_eq!(formats, vec![ExportFormat::Csv, ExportFormat::Json]);
}

/// The profile and keychain shapes added in M5 (Task 11 moves them into
/// the shared sample files).
#[test]
fn profile_views_and_keychain_errors_keep_their_wire_shape() {
    use coverly_core::profiles::custom::ProfileFile;
    use coverly_roms_lib::profiles::{ProfileInfoView, ProfilePreviewView};

    let onion = coverly_core::profiles::builtin_profiles()
        .into_iter()
        .find(|p| p.id == "onion")
        .unwrap();
    let view = serde_json::to_value(ProfileInfoView::of(&onion, true)).unwrap();
    assert_eq!(view["builtin"], json!(true));
    assert!(view["mirror_subdirs"].is_boolean());
    let back: ProfileFile = serde_json::from_value(view["profile"].clone()).unwrap();
    assert_eq!(back.id, "onion");
    let preview = ProfilePreviewView {
        paths: [("cover".to_string(), "Roms/GBA/Imgs/Zelda.png".to_string())].into(),
        errors: Vec::new(),
    };
    assert_eq!(
        serde_json::to_value(preview).unwrap(),
        json!({ "paths": { "cover": "Roms/GBA/Imgs/Zelda.png" }, "errors": [] })
    );
    assert_eq!(
        serde_json::to_value(IpcError::KeychainUnavailable).unwrap(),
        json!({ "code": "keychain-unavailable" })
    );
    assert_eq!(
        serde_json::to_value(IpcError::KeychainFailed("denied".into())).unwrap(),
        json!({ "code": "keychain-failed", "detail": "denied" })
    );
}

#[test]
fn settings_samples_match_rust() {
    use coverly_core::credentials::{CredentialKey, CredentialStore, MemoryStore};
    use coverly_core::profiles::custom::ProfileFile;
    use coverly_core::settings::{
        FieldError, FieldErrorCode, LoadNotice, LoadReport, Settings, RANGES,
    };
    use coverly_roms_lib::prefs::SettingsView;
    use coverly_roms_lib::profiles::ProfileInfoView;

    let sample: Value =
        serde_json::from_str(include_str!("../../src/ipc/contract.sample.json")).unwrap();
    // Through the text form, as on the wire: `to_value` widens the f32
    // thresholds (0.9 → 0.8999999761581421) while `to_string` prints the
    // shortest f32 form ("0.9") that the UI receives.
    let wire: Value =
        serde_json::from_str(&serde_json::to_string(&Settings::default()).unwrap()).unwrap();
    assert_eq!(wire, sample["settings"]);
    assert_eq!(serde_json::to_value(RANGES).unwrap(), sample["ranges"]);
    let e = FieldError {
        field: "network.timeout_secs".into(),
        code: FieldErrorCode::Range,
        min: Some(5.0),
        max: Some(300.0),
    };
    assert_eq!(serde_json::to_value(e).unwrap(), sample["field_error"]);
    let report = LoadReport {
        notice: Some(LoadNotice::Fixed),
        backup: Some("/config/settings.json.bak-1".into()),
        fixed: vec!["network.timeout_secs".into()],
    };
    let mut view = serde_json::to_value(SettingsView::new(Settings::default(), &report)).unwrap();
    view.as_object_mut().unwrap().remove("settings");
    assert_eq!(view, sample["settings_view_meta"]);
    let store = MemoryStore::default();
    store.set(CredentialKey::SteamgriddbApiKey, "x").unwrap();
    assert_eq!(
        serde_json::to_value(coverly_core::credentials::status(&store)).unwrap(),
        sample["credential_status"]
    );
    let info: ProfileFile =
        serde_json::from_value(sample["profile_info"]["profile"].clone()).unwrap();
    let view = ProfileInfoView::of(&info.to_profile(), false);
    assert_eq!(serde_json::to_value(view).unwrap(), sample["profile_info"]);
    let builtins: Vec<ProfileInfoView> = coverly_core::profiles::builtin_profiles()
        .iter()
        .map(|p| ProfileInfoView::of(p, true))
        .collect();
    assert_eq!(
        serde_json::to_value(builtins).unwrap(),
        sample["builtin_profiles"]
    );
}

#[test]
fn settings_requests_deserialize() {
    use coverly_core::credentials::CredentialKey;
    use coverly_core::profiles::custom::PreviewExample;
    use coverly_core::settings::Settings;
    use coverly_roms_lib::prefs::LinkId;

    let req: Value =
        serde_json::from_str(include_str!("../../src/ipc/contract.requests.json")).unwrap();
    let s = Settings::default()
        .with_patch(&req["settings_patch"], &[])
        .unwrap();
    assert_eq!(s.network.timeout_secs, 60);
    assert_eq!(s.safety.backup_dir, None);
    for id in req["link_ids"].as_array().unwrap() {
        serde_json::from_value::<LinkId>(id.clone()).unwrap();
    }
    for k in req["credential_keys"].as_array().unwrap() {
        assert!(CredentialKey::parse(k.as_str().unwrap()).is_some());
    }
    serde_json::from_value::<PreviewExample>(req["preview_example"].clone()).unwrap();
}
