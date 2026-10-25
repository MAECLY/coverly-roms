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
        loose_files: 3,
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
            kind: coverly_core::write::JobKind::Fill,
            undoable: true,
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
    let found = coverly_core::archive::external::FoundTool {
        tool: coverly_core::archive::Tool::SevenZip,
        path: "/opt/homebrew/bin/7zz".into(),
        version: "24.08".into(),
        rar5: true,
    };
    let mut view =
        serde_json::to_value(SettingsView::new(Settings::default(), &report).with_tools(&[found]))
            .unwrap();
    let view = view.as_object_mut().unwrap();
    view.remove("settings");
    let tools = view.remove("external_tools_found").unwrap();
    assert_eq!(tools, sample["settings_external_tools_found"]);
    assert_eq!(Value::Object(view.clone()), sample["settings_view_meta"]);
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

/// One organise plan with an automatic, a review and a left item.
fn organize_plan_sample() -> coverly_core::organize::OrganizePlan {
    use coverly_core::identify::evidence::{Evidence, IdLevel};
    use coverly_core::organize::{
        Counts as OrganizeCounts, ItemKind, ItemNote, LeftReason, NameMode, OrganizeAction,
        OrganizeItem, OrganizePlan, PlanWarning, RenameReason, SourceFp, TransferMode,
    };
    let item = |id: usize, source: &str, action: OrganizeAction| OrganizeItem {
        id,
        sources: vec![source.into()],
        kind: ItemKind::Archive,
        size: 4_194_304,
        fps: vec![SourceFp {
            size: 4_194_304,
            mtime_ns: 1_700_000_000_000_000_000,
        }],
        target: None,
        set_targets: vec![],
        system_id: None,
        folder: None,
        confidence: 0.0,
        method: None,
        why: vec![],
        alternatives: vec![],
        rename: None,
        notes: vec![],
        companions: vec![],
        catalogue_name: None,
        transfer: Default::default(),
        disc: None,
        action,
    };
    let auto = OrganizeItem {
        target: Some("/Users/me/ROMs-Downloaded/ROMS/GBA/Tetris Worlds (USA).zip".into()),
        system_id: Some("gba".into()),
        folder: Some("GBA".into()),
        confidence: 1.0,
        method: Some(IdLevel::Hash),
        why: vec![Evidence::new(
            IdLevel::Hash,
            "gba",
            1.0,
            "CRC 1A2B3C4D in No-Intro GBA",
        )],
        rename: Some(RenameReason::Collision {
            name: "Tetris Worlds (USA) (2).zip".into(),
        }),
        catalogue_name: Some("Tetris Worlds (USA)".into()),
        ..item(0, "Tetris Worlds (USA).zip", OrganizeAction::Auto)
    };
    let review = OrganizeItem {
        system_id: Some("gb".into()),
        folder: Some("GB".into()),
        confidence: 0.5,
        method: Some(IdLevel::Extension),
        why: vec![Evidence::new(IdLevel::Extension, "gb", 0.5, ".gb")],
        alternatives: vec![("gbc".into(), 0.25)],
        notes: vec![ItemNote::NoFolderOnDevice {
            folder: "GB".into(),
        }],
        ..item(1, "Tetris 2.zip", OrganizeAction::Review)
    };
    let left = item(
        2,
        "Tetris Attack.zip.crdownload",
        OrganizeAction::Left {
            reason: LeftReason::PartialDownload,
        },
    );
    OrganizePlan {
        source_root: "/Users/me/ROMs-Downloaded".into(),
        target_root: "/Users/me/ROMs-Downloaded".into(),
        roms_dir: "/Users/me/ROMs-Downloaded/ROMS".into(),
        layout: "anbernic-stock".into(),
        mode: TransferMode::Move,
        names: NameMode::Keep,
        items: vec![auto, review, left],
        counts: OrganizeCounts {
            roms: 2,
            systems: 2,
            auto: 1,
            review: 1,
            left: 1,
            junk: 1,
            other_files: 0,
            deep_folders: 0,
            bytes: 8_388_608,
            set_aside: 0,
            in_library: 0,
        },
        sorted_folders: vec![],
        deep_folders: vec![],
        other_files: vec![],
        warnings: vec![PlanWarning::CatalogueMissing {
            system: "gb".into(),
        }],
        broad_root: false,
        fat32: false,
        move_allowed: true,
        target_detected: None,
        created_at: "2026-10-17T10:32:00Z".into(),
        duplicates: vec![],
        set_aside_trash: false,
    }
}

/// A disc game in a 7z named `.zip`, extracted into the system folder
/// (the real case's PS1 archive, archives spec §1).
fn organize_extract_item_sample() -> coverly_core::organize::OrganizeItem {
    use coverly_core::dedupe::source::{DiscId, DiscIdSource};
    use coverly_core::identify::evidence::{Evidence, IdLevel};
    use coverly_core::organize::{
        ArchiveAfter, ExtractEntry, ExtractSource, ItemKind, ItemNote, OrganizeAction,
        OrganizeItem, SheetCheck, SourceFp, Transfer,
    };
    let entry = |index: usize, name: &str, size: u64, crc32: u32| ExtractEntry {
        index,
        name: format!("Next Tetris, The (USA)/{name}"),
        size,
        crc32: Some(crc32),
        target: format!("ROMS/PS/{name}").into(),
    };
    OrganizeItem {
        id: 3,
        sources: vec!["Next Tetris, The.zip".into()],
        kind: ItemKind::Archive,
        size: 197_012_345,
        fps: vec![SourceFp {
            size: 197_012_345,
            mtime_ns: 1_700_000_000_000_000_000,
        }],
        target: Some("/Users/me/ROMs-Downloaded/ROMS/PS/Next Tetris, The (USA).cue".into()),
        set_targets: vec![],
        system_id: Some("psx".into()),
        folder: Some("PS".into()),
        confidence: 0.75,
        method: Some(IdLevel::Header),
        why: vec![Evidence::new(
            IdLevel::Header,
            "psx",
            0.75,
            "SYSTEM.CNF BOOT + licence text",
        )],
        alternatives: vec![],
        rename: None,
        notes: vec![ItemNote::ContainerMismatch {
            ext: "zip".into(),
            actual: "7z".into(),
        }],
        companions: vec![],
        catalogue_name: None,
        transfer: Transfer::Extract {
            entries: vec![
                entry(0, "Next Tetris, The (USA).cue", 1_214, 0x0A1B_2C3D),
                entry(
                    1,
                    "Next Tetris, The (USA) (Track 01).bin",
                    385_000_000,
                    0x1122_3344,
                ),
                entry(
                    2,
                    "Next Tetris, The (USA) (Track 02).bin",
                    20_748_279,
                    0x5566_7788,
                ),
            ],
            bytes: 405_749_493,
            archive_after: ArchiveAfter::Trash,
            source: ExtractSource::Builtin,
            sheet_check: SheetCheck::Read,
            m3u: None,
        },
        disc: Some(DiscId {
            serials: vec!["SLUS-00862".into()],
            disc_no: None,
            volume: None,
            source: DiscIdSource::SystemCnf,
        }),
        action: OrganizeAction::Auto,
    }
}

#[test]
fn archive_and_disc_views_match_the_shared_sample() {
    use coverly_core::organize::{ItemOutcome, ItemStatus, OrganizePhase, OrganizeReport};
    use coverly_core::write::UndoReport;
    use coverly_roms_lib::organize::view::OrganizeProgressEvent;

    let job = "20261024T091500Z-1bfb-0002";
    let actual = json!({
        "organize_extract_item": organize_extract_item_sample(),
        "organize_progress_extract": OrganizeProgressEvent {
            plan_id: 3,
            phase: OrganizePhase::Extract {
                item: 3,
                bytes_done: 104_857_600,
                bytes_total: 405_749_493,
            },
            done: 0,
            total: 1,
        },
        "organize_report_extracted": OrganizeReport {
            job: Some(job.into()),
            moved: 0,
            copied: 0,
            extracted: 1,
            skipped: 0,
            failed: 0,
            set_aside: 0,
            items: vec![ItemOutcome {
                item: 3,
                status: ItemStatus::ExtractedKept,
                targets: vec![
                    "/Users/me/ROMs-Downloaded/ROMS/PS/Next Tetris, The (USA).cue".into(),
                ],
            }],
            paused: None,
            pause_reason: None,
            resumable_items: vec![],
        },
        "organize_undo_archive_lost": UndoReport {
            job: job.into(),
            removed: vec![],
            restored: vec![],
            skipped_modified: vec![],
            skipped_occupied: vec![],
            dirs_removed: vec![],
            archive_lost: vec!["/Users/me/ROMs-Downloaded/Next Tetris, The.zip".into()],
        },
    });
    let expected: Value =
        serde_json::from_str(include_str!("../../src/ipc/contract.sample.json")).unwrap();
    for (key, value) in actual.as_object().unwrap() {
        assert_eq!(
            value,
            &expected[key.as_str()],
            "sample `{key}`:\n{}",
            serde_json::to_string_pretty(value).unwrap()
        );
    }
}

#[test]
fn organise_views_match_the_shared_sample() {
    use coverly_core::organize::{
        ItemOutcome, ItemStatus as OrganizeItemStatus, OrganizePhase, OrganizeReport, SkipReason,
    };
    use coverly_core::write::{JobKind, UndoReport};
    use coverly_roms_lib::organize::view::{
        FolderProbeView, OrganizeJobState, OrganizeJobView, OrganizePlanView,
        OrganizeProgressEvent, OrganizeUndoView,
    };

    let job = "20261017T103200Z-1bfb-0000";
    let progress = OrganizeProgressEvent {
        plan_id: 3,
        phase: OrganizePhase::Apply,
        done: 1,
        total: 2,
    };
    let actual = json!({
        "folder_probe": FolderProbeView {
            rom_like: 17,
            partial_downloads: 3,
            cloud_only: 0,
            sorted_folders: vec![],
            writable: true,
            broad_root: false,
            truncated: false,
        },
        "organize_plan_view": OrganizePlanView { plan_id: 3, plan: organize_plan_sample() },
        "organize_progress": progress,
        "organize_job": OrganizeJobView {
            job: Some(job.into()),
            plan_id: 3,
            state: OrganizeJobState::Paused,
            progress: None,
            report: Some(OrganizeReport {
                job: Some(job.into()),
                moved: 1,
                copied: 0,
                extracted: 0,
                skipped: 1,
                failed: 0,
                set_aside: 0,
                items: vec![
                    ItemOutcome {
                        item: 0,
                        status: OrganizeItemStatus::Moved,
                        targets: vec!["/Users/me/ROMs-Downloaded/ROMS/GBA/Tetris Worlds (USA).zip".into()],
                    },
                    ItemOutcome {
                        item: 1,
                        status: OrganizeItemStatus::Skipped { reason: SkipReason::NotDecided },
                        targets: vec![],
                    },
                ],
                paused: Some(PauseKind::Volume),
                pause_reason: Some(ItemErrorKind::DiskFull),
                resumable_items: vec![],
            }),
            target_root: "/Users/me/ROMs-Downloaded".into(),
            layout: "anbernic-stock".into(),
            error: None,
        },
        "organize_job_applying": OrganizeJobView {
            job: None,
            plan_id: 3,
            state: OrganizeJobState::Applying,
            progress: Some(progress),
            report: None,
            target_root: "/Users/me/ROMs-Downloaded".into(),
            layout: "anbernic-stock".into(),
            error: None,
        },
        "organize_job_failed": OrganizeJobView {
            job: None,
            plan_id: 3,
            state: OrganizeJobState::Failed,
            progress: None,
            report: None,
            target_root: "/Users/me/ROMs-Downloaded".into(),
            layout: "anbernic-stock".into(),
            error: Some(IpcError::NoSpace("8473027/1000".into())),
        },
        "organize_undo": OrganizeUndoView {
            report: vec![UndoReport {
                job: job.into(),
                removed: vec![],
                restored: vec!["/Users/me/ROMs-Downloaded/Tetris Worlds (USA).zip".into()],
                skipped_modified: vec![],
                skipped_occupied: vec![],
                dirs_removed: vec!["/Users/me/ROMs-Downloaded/ROMS/GBA".into()],
                archive_lost: vec![],
            }],
            later_jobs: vec![JobHistoryView {
                job: "20261017T104500Z-1bfb-0001".into(),
                root: "/Users/me/ROMs-Downloaded".into(),
                created_at: "2026-10-17T10:45:00Z".into(),
                entries: 1,
                complete: true,
                undone: false,
                kind: JobKind::Fill,
                undoable: true,
            }],
        },
        "organize_errors": [
            IpcError::NoPlan,
            IpcError::LaterJobs("20261017T104500Z-1bfb-0001".into()),
            IpcError::NoSpace("8473027/1000".into()),
        ],
    });
    let expected: Value =
        serde_json::from_str(include_str!("../../src/ipc/contract.sample.json")).unwrap();
    for (key, value) in actual.as_object().unwrap() {
        assert_eq!(
            value,
            &expected[key.as_str()],
            "sample `{key}`:\n{}",
            serde_json::to_string_pretty(value).unwrap()
        );
    }
}

#[test]
fn rust_reads_the_shared_organise_requests() {
    use coverly_core::organize::{Choice, Decision as OrganizeDecision, NameMode, TransferMode};
    use coverly_roms_lib::organize::view::OrganizeRequest;

    let req: Value =
        serde_json::from_str(include_str!("../../src/ipc/contract.requests.json")).unwrap();
    let full: OrganizeRequest = serde_json::from_value(req["organize_request"].clone()).unwrap();
    assert_eq!(full.source, "/Users/me/ROMs-Downloaded");
    assert_eq!(full.layout.as_deref(), Some("anbernic-stock"));
    assert_eq!(full.target.as_deref(), Some("/Volumes/SD"));
    assert_eq!(full.flat, Some(false));
    assert_eq!(full.mode, Some(TransferMode::Copy));
    assert_eq!(full.names, Some(NameMode::Catalogue));
    assert_eq!(full.offline, Some(true));
    assert!(full.rescan);
    let bare: OrganizeRequest =
        serde_json::from_value(req["organize_request_bare"].clone()).unwrap();
    assert_eq!(bare.source, "/Users/me/ROMs-Downloaded");
    assert_eq!(bare.layout, None);
    assert!(
        !bare.rescan,
        "a request reuses the kept survey unless asked"
    );
    let decisions: Vec<OrganizeDecision> =
        serde_json::from_value(req["organize_decisions"].clone()).unwrap();
    assert_eq!(
        decisions,
        vec![
            OrganizeDecision {
                item: 1,
                choice: Choice::Accept
            },
            OrganizeDecision {
                item: 2,
                choice: Choice::Assign {
                    system: "gbc".into()
                }
            },
            OrganizeDecision {
                item: 3,
                choice: Choice::Leave
            },
        ]
    );
}

/// A duplicates plan of one Game Boy Advance group: the copy named `(1)`
/// goes to the quarantine with its save.
fn dedupe_plan_sample() -> coverly_core::dedupe::DedupePlan {
    use coverly_core::dedupe::{
        DedupePlan, DupGroup, GroupAction, KeepReason, Member, Relation, Totals, Travel,
        TravelKind, UnitKind, UnitRef,
    };
    let member = |id: usize, path: &str, mtime_ns: i64, dest: Option<&str>| Member {
        id,
        unit: UnitRef {
            primary: path.into(),
            files: vec![path.into()],
            kind: UnitKind::Single,
            mtime_ns,
            in_library: true,
        },
        relation: Relation::Exact,
        size: 4_194_304,
        travels: Vec::new(),
        stays: Vec::new(),
        dest: dest.map(Into::into),
        catalogue_name: Some("Tetris Worlds (USA)".into()),
        serials: Vec::new(),
        confidence: 1.0,
        gamelist: None,
        art_leaves: Default::default(),
    };
    let mut extra = member(
        1,
        "/Volumes/SD/ROMS/GBA/Tetris Worlds (USA) (1).gba",
        1_700_000_100_000_000_000,
        Some("/Volumes/SD/_Duplicados/GBA/Tetris Worlds (USA) (1).gba"),
    );
    extra.travels.push(Travel {
        from: "/Volumes/SD/ROMS/GBA/Tetris Worlds (USA) (1).sav".into(),
        kind: TravelKind::Save,
    });
    DedupePlan {
        root: "/Volumes/SD".into(),
        roms_dir: "/Volumes/SD/ROMS".into(),
        quarantine: "/Volumes/SD/_Duplicados".into(),
        same_volume: true,
        removable: true,
        profile_id: "anbernic-stock".into(),
        groups: vec![DupGroup {
            id: 4_503_599_627_370_495,
            system: "gba".into(),
            title: "Tetris Worlds (USA)".into(),
            keeper: 0,
            reason: KeepReason::CatalogueName {
                name: "Tetris Worlds (USA)".into(),
            },
            runner_up: Some((1, KeepReason::Tie)),
            members: vec![
                member(
                    0,
                    "/Volumes/SD/ROMS/GBA/Tetris Worlds (USA).gba",
                    1_700_000_000_000_000_000,
                    None,
                ),
                extra,
            ],
            action: GroupAction::Act,
            tie: false,
            choice_dropped: false,
        }],
        totals: Totals {
            groups: 1,
            extras: 1,
            bytes: 4_194_304,
            conflicts: 0,
            by_system: [("gba".to_string(), (1, 1, 4_194_304))].into(),
        },
        ..DedupePlan::default()
    }
}

#[test]
fn dedupe_contract_samples_match_views() {
    use coverly_core::dedupe::{
        DedupeItemStatus, DedupeOutcome, DedupePhase, DedupeReport, DedupeSkip,
    };
    use coverly_roms_lib::dedupe::view::{
        DedupeEventPhase, DedupeJobState, DedupeJobView, DedupePlanView, DedupeProgressEvent,
    };

    let job = "20261025T101500Z-1bfb-0003";
    let progress = DedupeProgressEvent {
        plan_id: 5,
        phase: DedupeEventPhase::Plan(DedupePhase::Fingerprints),
        done: 40,
        total: 631,
    };
    let applying = DedupeProgressEvent {
        plan_id: 5,
        phase: DedupeEventPhase::Apply,
        done: 1,
        total: 1,
    };
    let job_view = |state, report, error| DedupeJobView {
        job: Some(job.into()),
        plan_id: 5,
        state,
        progress: None,
        report,
        root: "/Volumes/SD".into(),
        quarantine: "/Volumes/SD/_Duplicados".into(),
        trash: false,
        error,
    };
    let actual = json!({
        "dedupe_plan_view": DedupePlanView {
            plan_id: 5,
            plan: dedupe_plan_sample(),
            trash_available: false,
        },
        "dedupe_progress": progress,
        "dedupe_progress_apply": applying,
        "dedupe_job": job_view(
            DedupeJobState::Done,
            Some(DedupeReport {
                job: Some(job.into()),
                outcomes: vec![
                    DedupeOutcome {
                        group: 4_503_599_627_370_495,
                        member: 1,
                        status: DedupeItemStatus::Quarantined {
                            to: "/Volumes/SD/_Duplicados/GBA/Tetris Worlds (USA) (1).gba".into(),
                        },
                        files: vec![
                            "/Volumes/SD/_Duplicados/GBA/Tetris Worlds (USA) (1).gba".into(),
                            "/Volumes/SD/_Duplicados/GBA/Tetris Worlds (USA) (1).sav".into(),
                        ],
                    },
                    DedupeOutcome {
                        group: 17,
                        member: 2,
                        status: DedupeItemStatus::Skipped { reason: DedupeSkip::Conflict },
                        files: vec![],
                    },
                ],
                bytes_out: 4_194_304,
                art_left: vec![],
                paused: None,
                resumable: vec![],
                notes: vec![],
            }),
            None,
        ),
        "dedupe_job_applying": DedupeJobView {
            job: None,
            state: DedupeJobState::Applying,
            progress: Some(applying),
            ..job_view(DedupeJobState::Applying, None, None)
        },
        "dedupe_job_failed": DedupeJobView {
            job: None,
            ..job_view(
                DedupeJobState::Failed,
                None,
                Some(IpcError::NoSpace("4194304/1000".into())),
            )
        },
        "dedupe_errors": [
            IpcError::NoDedupePlan,
            IpcError::KeeperRequired("duplicates group 17 would keep no copy".into()),
            IpcError::QuarantineInSystemFolder("/Volumes/SD/ROMS/GBA/dups".into()),
        ],
    });
    let expected: Value =
        serde_json::from_str(include_str!("../../src/ipc/contract.sample.json")).unwrap();
    for (key, value) in actual.as_object().unwrap() {
        assert_eq!(
            value,
            &expected[key.as_str()],
            "sample `{key}`:\n{}",
            serde_json::to_string_pretty(value).unwrap()
        );
    }
}

#[test]
fn rust_reads_the_shared_dedupe_requests() {
    use coverly_core::dedupe::QuarantineChoice;
    use coverly_core::settings::{DedupeLevel, KeepRule, RuleToggle};
    use coverly_roms_lib::dedupe::view::DedupeRequest;

    let req: Value =
        serde_json::from_str(include_str!("../../src/ipc/contract.requests.json")).unwrap();
    let full: DedupeRequest = serde_json::from_value(req["dedupe_request"].clone()).unwrap();
    assert_eq!(full.profile.as_deref(), Some("anbernic-stock"));
    assert_eq!(
        full.levels,
        Some(vec![DedupeLevel::Exact, DedupeLevel::Format])
    );
    assert_eq!(full.one_game_one_rom, Some(false));
    assert_eq!(
        full.rules,
        Some(vec![
            RuleToggle {
                rule: KeepRule::HasSave,
                on: true
            },
            RuleToggle {
                rule: KeepRule::Cover,
                on: false
            },
        ])
    );
    assert_eq!(full.systems, Some(vec!["gba".into(), "psx".into()]));
    assert_eq!(full.offline, Some(true));
    assert_eq!(
        full.quarantine,
        Some(QuarantineChoice::Folder {
            path: "/Users/me/Duplicados".into()
        })
    );
    assert_eq!(
        full.forced_keepers,
        ["/Volumes/SD/ROMS/GBA/Tetris Worlds (USA).gba"]
    );
    assert!(full.reset_choices);
    let bare: DedupeRequest = serde_json::from_value(req["dedupe_request_bare"].clone()).unwrap();
    assert_eq!(bare, DedupeRequest::default());
    let trash: DedupeRequest = serde_json::from_value(req["dedupe_request_trash"].clone()).unwrap();
    assert_eq!(trash.quarantine, Some(QuarantineChoice::Trash));
}
