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
    ArtMark, ArtMarks, ChildView, DemoView, EntryView, FolderCountsView, GameContent,
    LibrarySettingsView, LibraryView, NewerView, ProfileView, RecentView, ScanProgressEvent,
    SystemView, WarningView,
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
            content: GameContent::default(),
        }],
        warnings: vec![WarningView {
            path: "/Volumes/SD/ROMS/BAD".into(),
            message: "permission denied".into(),
        }],
        scan_ms: 42,
        loose_files: 3,
        revision_defaults: [(
            "snes:star courier (usa)".to_string(),
            "ROMS/SFC/Star Courier (USA) (Rev 1).sfc".to_string(),
        )]
        .into(),
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
                addons: None,
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
            game: None,
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
        "pick_reasons": [PickReason::Only, PickReason::Consensus { count: 3, next: 1 }, PickReason::SourceOrder { source: "libretro".into() }, PickReason::EmbeddedFirst, PickReason::Region { rom: true }, PickReason::User],
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
    let slots: Vec<coverly_core::art::children::ChildMedia> =
        serde_json::from_value(req["child_media"].clone()).unwrap();
    assert_eq!(
        slots,
        [
            coverly_core::art::children::ChildMedia::Icon,
            coverly_core::art::children::ChildMedia::Banner
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
    // Game folders spec D2: the library profile is a built-in, on.
    let library = sample["builtin_profiles"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["profile"]["id"] == "coverly-library")
        .expect("coverly-library in the built-ins");
    assert_eq!(library["profile"]["game_folders"], json!(true));
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
    // Game folders spec D1: one override set, `null` clears another.
    assert_eq!(
        s.organize.game_folders,
        [("coverly-library".to_string(), false)].into()
    );
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
        title_id: None,
        content_id: None,
        playlist: None,
        title_source: None,
        action,
        game: None,
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
        updates: 0,
        integration_notices: Vec::new(),
        target_anchor: None,
        translated: Vec::new(),
        emulator_roots: Vec::new(),
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
            folder_game: false,
        },
        disc: Some(DiscId {
            serials: vec!["SLUS-00862".into()],
            disc_no: None,
            volume: None,
            source: DiscIdSource::SystemCnf,
        }),
        title_id: None,
        content_id: None,
        playlist: None,
        title_source: None,
        action: OrganizeAction::Auto,
        game: None,
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
                entries_done: 4,
                entries_total: 12,
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
                licences_left: Vec::new(),
            }],
            paused: None,
            pause_reason: None,
            resumable_items: vec![],
            extras_saved: vec![],
            extras_unwritten: vec![],
        },
        "organize_undo_archive_lost": UndoReport {
            job: job.into(),
            removed: vec![],
            restored: vec![],
            skipped_modified: vec![],
            skipped_occupied: vec![],
            dirs_removed: vec![],
            dirs_kept: vec!["/Users/me/ROMs-Downloaded/ROMS/GB".into()],
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
                        licences_left: Vec::new(),
                    },
                    ItemOutcome {
                        item: 1,
                        status: OrganizeItemStatus::Skipped { reason: SkipReason::NotDecided },
                        targets: vec![],
                        licences_left: Vec::new(),
                    },
                ],
                paused: Some(PauseKind::Volume),
                pause_reason: Some(ItemErrorKind::DiskFull),
                resumable_items: vec![],
                extras_saved: vec![],
                extras_unwritten: vec![],
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
                dirs_kept: vec![],
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
                game: None,
            }],
        },
        "organize_errors": [
            IpcError::NoPlan,
            IpcError::LaterJobs("20261017T104500Z-1bfb-0001".into()),
            IpcError::NoSpace("8473027/1000".into()),
            IpcError::LargeTransfer {
                bytes: 187_420_000_000,
                items: 3,
                copies: true,
                free: Some(412_100_000_000),
                volume: "TinyExt".into(),
                huge: vec![coverly_core::organize::apply::HugeItem {
                    item: 4,
                    name: "CUSA00900".into(),
                    system: "ps4".into(),
                    bytes: 61_000_000_000,
                    extract: false,
                    followers: vec![5],
                }],
                rest: 126_420_000_000,
            },
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
    use coverly_core::organize::{
        Choice, Decision as OrganizeDecision, EmulatorChoice, EmulatorId, NameMode, TransferMode,
    };
    use coverly_core::settings::OrganizeDedupeMode;
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
    assert_eq!(full.duplicates, Some(OrganizeDedupeMode::Report));
    assert_eq!(full.game_folders, Some(true));
    assert_eq!(
        full.emulators,
        std::collections::BTreeMap::from([
            (
                "ps4".to_string(),
                EmulatorChoice::Emulator(EmulatorId::Shadps4)
            ),
            ("snes".to_string(), EmulatorChoice::None),
        ])
    );
    let bare: OrganizeRequest =
        serde_json::from_value(req["organize_request_bare"].clone()).unwrap();
    assert_eq!(bare.source, "/Users/me/ROMs-Downloaded");
    assert_eq!(bare.layout, None);
    assert_eq!(bare.duplicates, None);
    assert_eq!(bare.game_folders, None);
    assert!(bare.emulators.is_empty());
    assert!(
        !bare.rescan,
        "a request reuses the kept survey unless asked"
    );
    let confirm: coverly_core::organize::apply::LargeConfirm =
        serde_json::from_value(req["organize_confirm"].clone()).unwrap();
    assert_eq!(
        confirm,
        coverly_core::organize::apply::LargeConfirm {
            bytes: 126_420_000_000,
            items: vec![4, 7],
        }
    );
    let links: Vec<coverly_core::credits::CreditLink> =
        serde_json::from_value(req["credit_links"].clone()).unwrap();
    assert_eq!(
        links,
        [
            coverly_core::credits::CreditLink::Site,
            coverly_core::credits::CreditLink::License
        ]
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
        ArtSeen, DedupePlan, DupGroup, GroupAction, KeepReason, Member, MemberFacts, PlayedSeen,
        Relation, SavesSeen, TagSource, Totals, Travel, TravelKind, UnitKind, UnitRef,
    };
    use coverly_core::identify::names::{DumpFlags, ModClass, ReleaseClass};
    let member = |id: usize, path: &str, mtime_ns: i64, dest: Option<&str>| Member {
        id,
        unit: UnitRef {
            primary: path.into(),
            files: vec![path.into()],
            kind: UnitKind::Single,
            mtime_ns,
            in_library: true,
            mtimes_ns: Vec::new(),
            holds_children: false,
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
        facts: MemberFacts::default(),
    };
    // The keeper carries every fact, so the sample pins each key's
    // spelling; the extra has none and is written without `facts`.
    let mut keeper = member(
        0,
        "/Volumes/SD/ROMS/GBA/Tetris Worlds (USA).gba",
        1_700_000_000_000_000_000,
        None,
    );
    keeper.facts = MemberFacts {
        tags_from: Some(TagSource::Catalogue),
        regions: vec![Region::Usa],
        languages: vec!["en".into()],
        revision: Some("Rev 1".into()),
        class: Some(ReleaseClass::Beta),
        flags: DumpFlags {
            verified: true,
            ..DumpFlags::default()
        },
        flags_from: Some(TagSource::FileName),
        modification: Some(ModClass::Translation),
        modification_from: Some(TagSource::FileName),
        other: vec!["Virtual Console".into()],
        saves: Some(SavesSeen {
            count: 1,
            last_ns: 1_700_000_050_000_000_000,
        }),
        played: Some(PlayedSeen {
            count: 4,
            last: Some("20260102T101010".into()),
        }),
        art: vec![ArtSeen {
            media: Media::Cover,
            path: "/Volumes/SD/ROMS/GBA/Imgs/Tetris Worlds (USA).png".into(),
            size: 12_288,
        }],
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
            members: vec![keeper, extra],
            action: GroupAction::Act,
            tie: false,
            choice_dropped: false,
            basis: None,
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

#[test]
fn organise_set_aside_views_match_the_shared_sample() {
    use coverly_core::dedupe::{KeepReason, Relation};
    use coverly_core::identify::evidence::{Evidence, IdLevel};
    use coverly_core::organize::{
        Companion, Counts, ItemKind, ItemOutcome, ItemStatus, LeftReason, OrganizeAction,
        OrganizeItem, OrganizeReport, SourceFp, Transfer,
    };

    let item = OrganizeItem {
        id: 4,
        sources: vec!["Tetris (1).gb".into()],
        kind: ItemKind::File,
        size: 32_768,
        fps: vec![SourceFp {
            size: 32_768,
            mtime_ns: 1_700_000_000_000_000_000,
        }],
        target: None,
        set_targets: vec![],
        system_id: Some("gb".into()),
        folder: Some("GB".into()),
        confidence: 1.0,
        method: Some(IdLevel::Hash),
        why: vec![Evidence::new(IdLevel::Hash, "gb", 1.0, "CRC in the GB DAT")],
        alternatives: vec![],
        rename: None,
        notes: vec![],
        companions: vec![Companion {
            source: "Tetris (1).sav".into(),
            target: "_Duplicados/Tetris (1).sav".into(),
            licence: false,
            child_art: false,
        }],
        catalogue_name: Some("Tetris (World) (Rev 1)".into()),
        transfer: Transfer::AsIs,
        disc: None,
        title_id: None,
        content_id: None,
        playlist: None,
        title_source: None,
        action: OrganizeAction::SetAside {
            keeper: "ROMS/GB/Tetris (World) (Rev 1).gb".into(),
            relation: Relation::Exact,
            reason: KeepReason::InLibrary,
            dest: "_Duplicados/Tetris (1).gb".into(),
        },
        game: None,
    };
    let actual = json!({
        "organize_set_aside_item": item,
        "organize_in_library_reason": LeftReason::InLibrary {
            path: "ROMS/GB/Tetris (World) (Rev 1).gb".into(),
            relation: Relation::Version,
        },
        "organize_counts_set_aside": Counts {
            roms: 1,
            systems: 1,
            auto: 1,
            set_aside: 1,
            in_library: 2,
            bytes: 32_768,
            ..Counts::default()
        },
        "organize_report_extras": OrganizeReport {
            job: Some("20261114T120000Z-1bfb-0005".into()),
            moved: 2,
            extras_saved: vec![
                "/Users/me/ROMs/PS3/Resistance (USA)/Extras/extras.broken-20261114-120000.json"
                    .into(),
            ],
            extras_unwritten: vec!["/Users/me/ROMs/PS3/Tidewalker/Extras".into()],
            ..OrganizeReport::default()
        },
        "organize_report_set_aside": OrganizeReport {
            job: Some("20261025T111500Z-1bfb-0004".into()),
            set_aside: 2,
            items: vec![
                ItemOutcome {
                    item: 4,
                    status: ItemStatus::Quarantined,
                    targets: vec!["/Users/me/ROMs-Downloaded/_Duplicados/Tetris (1).gb".into()],
                    licences_left: Vec::new(),
                },
                ItemOutcome {
                    item: 5,
                    status: ItemStatus::Trashed,
                    targets: vec![],
                    licences_left: Vec::new(),
                },
            ],
            ..OrganizeReport::default()
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
fn system_families_and_folder_games_match_the_shared_sample() {
    use coverly_core::organize::LeftReason;
    use coverly_roms_lib::ipc::list_systems;

    let systems = list_systems();
    let pick: Vec<_> = ["ps2", "gba", "zxspectrum", "mame", "wasm4", "xboxseries"]
        .iter()
        .map(|id| systems.iter().find(|s| s.id == *id).unwrap().clone())
        .collect();
    let actual = json!({
        "system_infos": pick,
        "organize_folder_game_reason": LeftReason::FolderGame {
            system: "doom".into(),
            folder: "prboom".into(),
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

/// Systems batch 2 (spec §10): a game folder with its title id and the
/// new notes, the new reasons and the plan's update count.
#[test]
fn systems_batch_2_items_match_the_shared_sample() {
    use coverly_core::consoles::{IdSource, TitleIdRef};
    use coverly_core::identify::evidence::{Evidence, IdLevel};
    use coverly_core::identify::sniff::NotARom;
    use coverly_core::organize::{
        ItemKind, ItemNote, LeftReason, OrganizeAction, OrganizeItem, SourceFp, Transfer,
    };

    let folder = OrganizeItem {
        id: 7,
        sources: vec!["Demon's Souls (BLUS-30443)".into()],
        kind: ItemKind::Folder,
        size: 23_410_000_000,
        fps: vec![SourceFp {
            size: 23_410_000_000,
            mtime_ns: 1_700_000_000_000_000_000,
        }],
        target: Some("ps3/Demon's Souls (BLUS-30443).ps3".into()),
        set_targets: vec![],
        system_id: Some("ps3".into()),
        folder: Some("ps3".into()),
        confidence: 0.95,
        method: Some(IdLevel::Header),
        why: vec![Evidence::new(IdLevel::Header, "ps3", 0.95, "PARAM.SFO")],
        alternatives: vec![],
        rename: None,
        notes: vec![
            ItemNote::FolderSuffix {
                name: "Demon's Souls (BLUS-30443).ps3".into(),
                layout: "ES-DE".into(),
            },
            ItemNote::Update {
                of: "Demon's Souls".into(),
            },
            ItemNote::Dlc {
                of: "Demon's Souls".into(),
            },
            ItemNote::EncryptedContent {
                emulator: "Azahar".into(),
            },
            ItemNote::InstallFirst {
                emulator: "RPCS3".into(),
            },
        ],
        companions: vec![],
        catalogue_name: None,
        transfer: Transfer::AsIs,
        disc: None,
        title_id: Some(TitleIdRef {
            id: "BLUS30443".into(),
            source: IdSource::ParamSfo,
        }),
        content_id: None,
        playlist: None,
        title_source: None,
        action: OrganizeAction::Auto,
        game: None,
    };
    let sources: Vec<IdSource> = vec![
        IdSource::ParamSfo,
        IdSource::MetaXml,
        IdSource::AppXml,
        IdSource::DefaultXbe,
        IdSource::DefaultXex,
        IdSource::CnmtXml,
        IdSource::TicketName,
        IdSource::FileName,
        IdSource::Header,
        IdSource::ParamJson,
        IdSource::MicrosoftGameConfig,
        IdSource::AppxManifest,
        IdSource::NcaCatalog,
        IdSource::Tmd,
        IdSource::Npdm,
        IdSource::Xbe,
        IdSource::Xex,
        IdSource::SelfHeader,
        IdSource::Pbp,
    ];
    let mut plan = organize_plan_sample();
    plan.updates = 2;
    // Through the text form, as on the wire (f32 0.95 prints as "0.95").
    let folder: Value = serde_json::from_str(&serde_json::to_string(&folder).unwrap()).unwrap();
    let actual = json!({
        "organize_folder_item": folder,
        "organize_id_sources": sources,
        "organize_batch2_reasons": [
            LeftReason::KeyMaterial,
            LeftReason::EmulatorData { folder: "ux0".into() },
            LeftReason::NotARom { what: NotARom::Installer },
            LeftReason::NotARom { what: NotARom::Model3d },
            LeftReason::NotARom { what: NotARom::PcGameData },
        ],
        "organize_plan_updates": serde_json::to_value(&plan).unwrap()["updates"],
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
    // Absent when there is none, as every older plan.
    assert!(serde_json::to_value(organize_plan_sample()).unwrap()["updates"].is_null());
}

/// Systems batch 3 (spec §10): a PS4 folder with its content id and the
/// new notes, the new reasons and the large-transfer warning.
#[test]
fn systems_batch_3_items_match_the_shared_sample() {
    use coverly_core::consoles::{IdSource, TitleIdRef};
    use coverly_core::identify::evidence::{Evidence, IdLevel};
    use coverly_core::identify::sniff::NotARom;
    use coverly_core::organize::{
        ItemKind, ItemNote, LeftReason, OrganizeAction, OrganizeItem, PlanWarning, SourceFp,
        Transfer,
    };

    let folder = OrganizeItem {
        id: 3,
        sources: vec!["Bloodborne (CUSA00900)".into()],
        kind: ItemKind::Folder,
        size: 48_000_000_000,
        fps: vec![SourceFp {
            size: 48_000_000_000,
            mtime_ns: 1_700_000_000_000_000_000,
        }],
        target: Some("ps4/DLC/CUSA00900/Bloodborne (CUSA00900)".into()),
        set_targets: vec![],
        system_id: Some("ps4".into()),
        folder: Some("ps4".into()),
        confidence: 0.95,
        method: Some(IdLevel::Header),
        why: vec![Evidence::new(IdLevel::Header, "ps4", 0.95, "param.sfo")],
        alternatives: vec![],
        rename: None,
        notes: vec![
            ItemNote::ExtractFirst,
            ItemNote::NoEmulator,
            ItemNote::ProEnhanced,
            ItemNote::DlcFolder {
                layout: "Batocera".into(),
            },
        ],
        companions: vec![],
        catalogue_name: None,
        transfer: Transfer::AsIs,
        disc: None,
        title_id: Some(TitleIdRef {
            id: "CUSA00900".into(),
            source: IdSource::ParamSfo,
        }),
        content_id: Some("UP9000-CUSA00900_00-BLOODBORNE000000".into()),
        playlist: None,
        title_source: None,
        action: OrganizeAction::Auto,
        game: None,
    };
    let folder: Value = serde_json::from_str(&serde_json::to_string(&folder).unwrap()).unwrap();
    let actual = json!({
        "organize_batch3_item": folder,
        "organize_batch3_reasons": [
            LeftReason::GameOverlay { of: "Bloodborne".into() },
            LeftReason::NotARom { what: NotARom::ConsoleSystem },
        ],
        "organize_batch3_warnings": [
            PlanWarning::LargeTransfer { bytes: 12_000_000_000, items: 2 },
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

/// Adjustments (spec §10): game parts, declined items, the new notes and
/// the console kinds, parts and title sources.
#[test]
fn adjustment_values_match_the_shared_sample() {
    use coverly_core::consoles::{IdSource, PartKind, TitleIdRef, TitleKind, TitleSource};
    use coverly_core::identify::evidence::{Evidence, IdLevel};
    use coverly_core::organize::{
        Choice, Companion, Decision, ItemKind, ItemNote, LeftReason, OrganizeAction, OrganizeItem,
        Playlist, SourceFp, Transfer,
    };

    // A cartridge the optional catalogue named through its NCA ids (D6, D7).
    let cartridge = OrganizeItem {
        id: 1,
        sources: vec!["Cartridge.xci".into()],
        kind: ItemKind::File,
        size: 5_600_000_000,
        fps: vec![SourceFp {
            size: 5_600_000_000,
            mtime_ns: 1_700_000_000_000_000_000,
        }],
        target: Some("switch/Cartridge.xci".into()),
        set_targets: vec![],
        system_id: Some("switch".into()),
        folder: Some("switch".into()),
        confidence: 0.95,
        method: Some(IdLevel::Header),
        why: vec![Evidence::new(IdLevel::Header, "switch", 0.95, "HEAD")],
        alternatives: vec![],
        rename: None,
        notes: vec![],
        companions: vec![],
        catalogue_name: Some("Super Mario Odyssey".into()),
        transfer: Transfer::AsIs,
        disc: None,
        title_id: Some(TitleIdRef {
            id: "0100000000010000".into(),
            source: IdSource::NcaCatalog,
        }),
        content_id: None,
        playlist: None,
        title_source: Some(TitleSource::Titledb),
        action: OrganizeAction::Auto,
        game: None,
    };
    let cartridge: Value =
        serde_json::from_str(&serde_json::to_string(&cartridge).unwrap()).unwrap();

    let actual = json!({
        "adjustments_catalogue_item": cartridge,
        "adjustments_reasons": [
            LeftReason::GamePart {
                part: PartKind::ExeFs,
                system: "switch".into(),
                title_id: None,
            },
            LeftReason::GamePart {
                part: PartKind::Backup,
                system: "psvita".into(),
                title_id: Some("PCSE00120".into()),
            },
            LeftReason::NotConfirmed,
        ],
        "adjustments_notes": [
            ItemNote::Homebrew,
            ItemNote::Vita7z,
            ItemNote::MissingDisc { n: 2 },
            ItemNote::EncryptedName,
            ItemNote::ArchiveGames { count: 2 },
            ItemNote::Ps4OrVita,
            ItemNote::ArchiveCaseCollision,
            ItemNote::DiscSet,
        ],
        // A 3DS product code's unknown region letter (part C §5, P-1).
        "region_letter_note": ItemNote::RegionLetterUnknown { letter: 'T' },
        // A child's own art that stays, and one that follows (part C D10 d).
        "child_art_note": ItemNote::ChildArtStays { count: 2 },
        "child_art_companion": Companion {
            source: "Pokemon Y/DLC/.media/Pack-icon.png".into(),
            target: "3DS/Pokemon Y/DLC/.media/Pokemon Y - Pack-icon.png".into(),
            licence: false,
            child_art: true,
        },
        "adjustments_playlist": Playlist {
            path: "ROMS/Saturn/Grandia (Japan).m3u".into(),
            discs: vec![
                "ROMS/Saturn/Grandia (Japan) (Disc 1).cue".into(),
                "ROMS/Saturn/Grandia (Japan) (Disc 2).cue".into(),
            ],
        },
        "adjustments_decisions": [Decision {
            item: 4,
            choice: Choice::Decline,
        }],
        "adjustments_part_kinds": [
            PartKind::ExeFs,
            PartKind::Nca,
            PartKind::Library,
            PartKind::TitleMeta,
            PartKind::Module,
            PartKind::Backup,
        ],
        "adjustments_title_kinds": [
            TitleKind::Base,
            TitleKind::Update,
            TitleKind::Dlc,
            TitleKind::Demo,
            TitleKind::Homebrew,
            TitleKind::System,
            TitleKind::Unknown,
        ],
        "adjustments_title_sources": [
            TitleSource::Header,
            TitleSource::Titledb,
            TitleSource::Nlib,
            TitleSource::FileName,
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

/// Game folders (spec D16, §10): an update in its game folder with a
/// licence beside it, the fixed-folder reason, the new notes, renames and
/// not-a-ROM values, every fixed folder and every role.
#[test]
fn game_folder_values_match_the_shared_sample() {
    use coverly_core::consoles::{FixedFolder, IdSource, TitleIdRef};
    use coverly_core::identify::evidence::{Evidence, IdLevel};
    use coverly_core::identify::names::Region;
    use coverly_core::identify::sniff::NotARom;
    use coverly_core::organize::{
        Companion, GameLink, GameRole, ItemKind, ItemNote, LeftReason, OrganizeAction,
        OrganizeItem, RenameReason, SourceFp, Transfer,
    };

    let mut link = GameLink::new(GameRole::Update);
    link.version = Some("1.5".into());
    link.region = Some(Region::Europe);
    link.of = Some(1);
    link.folder = Some("3DS/Pokemon Y".into());
    link.newest_here = true;
    let update = OrganizeItem {
        id: 2,
        sources: vec!["pky-update.cia".into()],
        kind: ItemKind::File,
        size: 300_000_000,
        fps: vec![SourceFp {
            size: 300_000_000,
            mtime_ns: 1_700_000_000_000_000_000,
        }],
        target: Some("3DS/Pokemon Y/Updates/Pokemon Y (v1.5).cia".into()),
        set_targets: vec![],
        system_id: Some("3ds".into()),
        folder: Some("3DS".into()),
        confidence: 0.95,
        method: Some(IdLevel::Header),
        why: vec![Evidence::new(IdLevel::Header, "3ds", 0.95, "CIA")],
        alternatives: vec![],
        rename: Some(RenameReason::GameFolder {
            from: "pky-update.cia".into(),
        }),
        notes: vec![ItemNote::Update {
            of: "Pokemon Y".into(),
        }],
        companions: vec![Companion {
            source: "pky-update.tik".into(),
            target: "3DS/Pokemon Y/Updates/Pokemon Y (v1.5).tik".into(),
            licence: true,
            child_art: false,
        }],
        catalogue_name: None,
        transfer: Transfer::AsIs,
        disc: None,
        title_id: Some(TitleIdRef {
            id: "0004000E00055E00".into(),
            source: IdSource::Header,
        }),
        content_id: None,
        playlist: None,
        title_source: None,
        game: Some(link),
        action: OrganizeAction::Auto,
    };
    let update: Value = serde_json::from_str(&serde_json::to_string(&update).unwrap()).unwrap();
    let actual = json!({
        "game_folder_item": update,
        "game_folder_reasons": [
            LeftReason::GameFolderContent {
                folder: FixedFolder::Mods,
                of: "Pokemon Y".into(),
            },
            LeftReason::NotARom { what: NotARom::EmulatorCache },
            LeftReason::NotARom { what: NotARom::EmulatorConfig },
            LeftReason::NotARom { what: NotARom::Music },
            LeftReason::LicenceOf {
                of: "Resistance.pkg".into(),
            },
        ],
        "game_folder_notes": [
            ItemNote::DemoOf { of: "Pokemon Y".into() },
            ItemNote::BaseMissing,
        ],
        "game_folder_fixed_folders": [
            FixedFolder::Updates,
            FixedFolder::Dlc,
            FixedFolder::Mods,
            FixedFolder::Cheats,
            FixedFolder::Extras,
            FixedFolder::Saves,
        ],
        "game_folder_roles": [
            GameRole::Base,
            GameRole::Update,
            GameRole::Dlc,
            GameRole::Demo,
            GameRole::Mod,
            GameRole::Cheat,
            GameRole::Extra,
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

/// Mods spec D16, §12: a plan holding an addon row, the new reasons and
/// notes and a translated layout reads back as it was saved (a paused
/// job's `jobs/<job>.plan.json`), and a plan saved before part B, without
/// `translated` or `target_anchor`, still reads.
#[test]
fn a_plan_with_addons_and_a_translated_layout_round_trips() {
    use coverly_core::organize::{
        ItemNote, LeftReason, OrganizeAction, OrganizeItem, OrganizePlan, TranslatedGameView,
    };

    let sample: Value =
        serde_json::from_str(include_str!("../../src/ipc/contract.sample.json")).unwrap();
    let mut plan = organize_plan_sample();
    let addon: OrganizeItem = serde_json::from_value(sample["addon_item"].clone()).unwrap();
    assert!(addon.game.as_ref().unwrap().addon.is_some());
    plan.items.push(addon);
    let reasons: Vec<LeftReason> = serde_json::from_value(sample["addon_reasons"].clone()).unwrap();
    let notes: Vec<ItemNote> = serde_json::from_value(sample["addon_notes"].clone()).unwrap();
    for (n, reason) in reasons.iter().enumerate() {
        let mut item = plan.items[0].clone();
        item.id = 10 + n;
        item.action = OrganizeAction::Left {
            reason: reason.clone(),
        };
        item.notes = notes.clone();
        plan.items.push(item);
    }
    let game: TranslatedGameView =
        serde_json::from_value(sample["translated_game"].clone()).unwrap();
    plan.translated = vec![game];

    let text = serde_json::to_string(&plan).unwrap();
    let back: OrganizePlan = serde_json::from_str(&text).unwrap();
    assert_eq!(back, plan);
    let wire: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(wire["translated"], json!([sample["translated_game"]]));

    let mut old = serde_json::to_value(organize_plan_sample()).unwrap();
    let fields = old.as_object_mut().unwrap();
    assert!(!fields.contains_key("translated"), "left out when empty");
    assert!(!fields.contains_key("target_anchor"), "left out when none");
    fields.remove("emulator_roots");
    let old: OrganizePlan = serde_json::from_value(old).unwrap();
    assert!(old.translated.is_empty());
    assert_eq!(old.target_anchor, None);
}

/// Fail to compile when an addon, emulator or translate value is added,
/// so `addon_values_match_the_shared_sample` (and the TypeScript unions)
/// get it too (mods spec D10, D16).
#[allow(dead_code, clippy::too_many_arguments)]
fn every_addon_value_is_listed(
    kind: coverly_core::addons::AddonKind,
    format: coverly_core::addons::AddonFormat,
    via: coverly_core::addons::LinkVia,
    fit: coverly_core::addons::HeaderFit,
    why: coverly_core::addons::UnlinkedWhy,
    emulator: coverly_core::organize::EmulatorId,
    not: coverly_core::organize::NotSupportedWhy,
    note: coverly_core::organize::TranslateNote,
    to: coverly_core::organize::PlacementTo,
) {
    use coverly_core::addons::{AddonFormat as F, AddonKind, HeaderFit, LinkVia, UnlinkedWhy};
    use coverly_core::organize::{EmulatorId as E, NotSupportedWhy, PlacementTo, TranslateNote};
    match kind {
        AddonKind::Patch | AddonKind::Cheat | AddonKind::Mod => {}
    }
    match format {
        F::Bps
        | F::Ups
        | F::Ips
        | F::Xdelta
        | F::Ppf
        | F::Aps
        | F::RetroArchCht
        | F::FceuxCht
        | F::DuckStationCht
        | F::Pnach
        | F::DolphinIni
        | F::PpssppIni
        | F::CtrCheats
        | F::SwitchCheats
        | F::Pchtxt
        | F::Rpcs3Patch
        | F::Shadps4Cheat
        | F::Shadps4Patch
        | F::XeniaPatch
        | F::Vita3kPatch
        | F::MelonDsCheats
        | F::MgbaCheats
        | F::DesmumeCheats
        | F::CodeText
        | F::TexturePack
        | F::DolphinGraphicMod
        | F::Riivolution
        | F::LayeredFs
        | F::SwitchMod
        | F::ExefsPatches
        | F::CemuGraphicPack
        | F::VitaRepatch
        | F::MesenHdPack
        | F::Msu1
        | F::XeniaPlugin
        | F::N64HiresTextures
        | F::Shadps4Mods => {}
    }
    match via {
        LinkVia::SourceCrc
        | LinkVia::Dat
        | LinkVia::Name
        | LinkVia::Readme
        | LinkVia::Key
        | LinkVia::Folder => {}
    }
    match fit {
        HeaderFit::Matches | HeaderFit::NeedsHeaderless | HeaderFit::NeedsHeadered => {}
    }
    match why {
        UnlinkedWhy::NoMatch
        | UnlinkedWhy::Several { .. }
        | UnlinkedWhy::Many { .. }
        | UnlinkedWhy::NoKey
        | UnlinkedWhy::Budget
        | UnlinkedWhy::Partial => {}
    }
    // `EmulatorId::ALL` feeds the sample: this keeps it whole.
    match emulator {
        E::Shadps4
        | E::Rpcs3
        | E::Azahar
        | E::Cemu
        | E::Vita3k
        | E::Ryujinx
        | E::YuzuFamily
        | E::RetroArch
        | E::DuckStation
        | E::Pcsx2
        | E::Dolphin
        | E::Ppsspp
        | E::Xenia
        | E::Flycast
        | E::Mesen => {}
    }
    match not {
        NotSupportedWhy::Rpcs3Mods
        | NotSupportedWhy::VitaRepatch
        | NotSupportedWhy::SeveralPatches
        | NotSupportedWhy::OlderUpdate
        | NotSupportedWhy::ConfigExists => {}
    }
    match note {
        TranslateNote::InstallInEmulator { .. }
        | TranslateNote::ExtractFirst
        | TranslateNote::NotSupported { .. }
        | TranslateNote::HeaderMismatch { .. }
        | TranslateNote::PointEmulatorHere { .. }
        | TranslateNote::PatchNotApplied { .. } => {}
    }
    match to {
        PlacementTo::System { .. } | PlacementTo::EmulatorData { .. } | PlacementTo::Library => {}
    }
}

/// Mods spec D10, D16: an addon row, the new reasons and notes, every
/// emulator id and why, and a translated layout use the spellings the
/// webview's union types name.
#[test]
fn addon_values_match_the_shared_sample() {
    use coverly_core::addons::{
        AddonFormat, AddonKind, AddonLink, HeaderFit, LinkVia, UnlinkedWhy,
    };
    use coverly_core::identify::evidence::{Evidence, IdLevel};
    use coverly_core::organize::{
        EmulatorId, GameLink, GameRole, ItemKind, ItemNote, LeftReason, NotSupportedWhy,
        OrganizeAction, OrganizeItem, PlacementTo, PlacementView, SourceFp, Transfer,
        TranslateNote, TranslatedGameView,
    };

    let mut link = GameLink::new(GameRole::Cheat);
    link.of = Some(0);
    link.folder = Some("PS1/Crash Bandicoot (USA)".into());
    link.addon = Some(AddonLink {
        kind: AddonKind::Cheat,
        format: AddonFormat::DuckStationCht,
        key_text: "SCUS-94900".into(),
        via: LinkVia::Key,
        header: None,
    });
    let cheat = OrganizeItem {
        id: 1,
        sources: vec!["SCUS-94900.cht".into()],
        kind: ItemKind::File,
        size: 2_048,
        fps: vec![SourceFp {
            size: 2_048,
            mtime_ns: 1_700_000_000_000_000_000,
        }],
        target: Some("PS1/Crash Bandicoot (USA)/Cheats/SCUS-94900.cht".into()),
        set_targets: vec![],
        system_id: Some("psx".into()),
        folder: Some("PS1".into()),
        confidence: 0.95,
        method: Some(IdLevel::Header),
        why: vec![Evidence::new(IdLevel::Header, "psx", 0.95, "SCUS-94900")],
        alternatives: vec![],
        rename: None,
        notes: vec![ItemNote::CheatOf {
            of: "Crash Bandicoot".into(),
        }],
        companions: vec![],
        catalogue_name: None,
        transfer: Transfer::AsIs,
        disc: None,
        title_id: None,
        content_id: None,
        playlist: None,
        title_source: None,
        game: Some(link),
        action: OrganizeAction::Auto,
    };
    let cheat: Value = serde_json::from_str(&serde_json::to_string(&cheat).unwrap()).unwrap();
    let actual = json!({
        "addon_item": cheat,
        "addon_reasons": [
            LeftReason::AddonUnlinked {
                kind: AddonKind::Patch,
                format: AddonFormat::Ips,
                why: UnlinkedWhy::NoKey,
            },
            LeftReason::AddonUnlinked {
                kind: AddonKind::Mod,
                format: AddonFormat::CemuGraphicPack,
                why: UnlinkedWhy::Several { count: 2 },
            },
            LeftReason::AddonUnlinked {
                kind: AddonKind::Cheat,
                format: AddonFormat::Rpcs3Patch,
                why: UnlinkedWhy::Many { count: 400 },
            },
            LeftReason::AddonNameTaken {
                at: "PS1/Crash Bandicoot (USA)/Cheats".into(),
            },
            LeftReason::AddonNoPlace {
                kind: AddonKind::Mod,
            },
            LeftReason::AddonForEmulator {
                emulator: "DuckStation".into(),
                path: "cheats/SCUS-94900.cht".into(),
            },
            LeftReason::NotSupported {
                emulator: "RPCS3".into(),
                why: NotSupportedWhy::Rpcs3Mods,
            },
            LeftReason::PatchNotApplied {
                emulator: "RetroArch".into(),
            },
        ],
        "addon_notes": [
            ItemNote::ModOf { of: "Zelda".into() },
            ItemNote::PatchOf { of: "Zelda".into() },
            ItemNote::CheatOf { of: "Zelda".into() },
            ItemNote::HeaderMismatch {
                needs: HeaderFit::NeedsHeaderless,
            },
            ItemNote::PointEmulatorHere {
                emulator: "shadPS4".into(),
                folder: "addcont".into(),
            },
        ],
        "addon_kinds": [AddonKind::Patch, AddonKind::Cheat, AddonKind::Mod],
        "addon_formats": [
            AddonFormat::Bps,
            AddonFormat::Ups,
            AddonFormat::Ips,
            AddonFormat::Xdelta,
            AddonFormat::Ppf,
            AddonFormat::Aps,
            AddonFormat::RetroArchCht,
            AddonFormat::FceuxCht,
            AddonFormat::DuckStationCht,
            AddonFormat::Pnach,
            AddonFormat::DolphinIni,
            AddonFormat::PpssppIni,
            AddonFormat::CtrCheats,
            AddonFormat::SwitchCheats,
            AddonFormat::Pchtxt,
            AddonFormat::Rpcs3Patch,
            AddonFormat::Shadps4Cheat,
            AddonFormat::Shadps4Patch,
            AddonFormat::XeniaPatch,
            AddonFormat::Vita3kPatch,
            AddonFormat::MelonDsCheats,
            AddonFormat::MgbaCheats,
            AddonFormat::DesmumeCheats,
            AddonFormat::CodeText,
            AddonFormat::TexturePack,
            AddonFormat::DolphinGraphicMod,
            AddonFormat::Riivolution,
            AddonFormat::LayeredFs,
            AddonFormat::SwitchMod,
            AddonFormat::ExefsPatches,
            AddonFormat::CemuGraphicPack,
            AddonFormat::VitaRepatch,
            AddonFormat::MesenHdPack,
            AddonFormat::Msu1,
            AddonFormat::XeniaPlugin,
            AddonFormat::N64HiresTextures,
            AddonFormat::Shadps4Mods,
        ],
        "addon_vias": [
            LinkVia::SourceCrc,
            LinkVia::Dat,
            LinkVia::Name,
            LinkVia::Readme,
            LinkVia::Key,
            LinkVia::Folder,
        ],
        "header_fits": [
            HeaderFit::Matches,
            HeaderFit::NeedsHeaderless,
            HeaderFit::NeedsHeadered,
        ],
        "emulator_ids": EmulatorId::ALL,
        "not_supported_whys": [
            NotSupportedWhy::Rpcs3Mods,
            NotSupportedWhy::VitaRepatch,
            NotSupportedWhy::SeveralPatches,
            NotSupportedWhy::OlderUpdate,
            NotSupportedWhy::ConfigExists,
        ],
        "unlinked_whys": [
            UnlinkedWhy::NoMatch,
            UnlinkedWhy::Several { count: 3 },
            UnlinkedWhy::Many { count: 400 },
            UnlinkedWhy::NoKey,
            UnlinkedWhy::Budget,
            UnlinkedWhy::Partial,
        ],
        "translated_game": TranslatedGameView {
            system: "ps4".into(),
            base: Some(0),
            emulator: EmulatorId::Shadps4,
            placements: vec![
                PlacementView {
                    from: "Bloodborne/CUSA00900".into(),
                    to: PlacementTo::System {
                        path: "CUSA00900".into(),
                    },
                    note: None,
                },
                PlacementView {
                    from: "Bloodborne/Updates/CUSA00900-patch".into(),
                    to: PlacementTo::System {
                        path: "CUSA00900-UPDATE".into(),
                    },
                    note: None,
                },
                PlacementView {
                    from: "Bloodborne/DLC/dlc.pkg".into(),
                    to: PlacementTo::Library,
                    note: Some(TranslateNote::ExtractFirst),
                },
                PlacementView {
                    from: "Bloodborne/Cheats/CUSA00900_01.09.json".into(),
                    to: PlacementTo::EmulatorData {
                        path: "cheats/CUSA00900_01.09.json".into(),
                    },
                    note: None,
                },
                PlacementView {
                    from: "Bloodborne/DLC/CUSA00900-DLC".into(),
                    to: PlacementTo::System {
                        path: "addcont/CUSA00900/CUSA00900-DLC".into(),
                    },
                    note: Some(TranslateNote::PointEmulatorHere {
                        emulator: EmulatorId::Shadps4,
                        folder: "addcont".into(),
                    }),
                },
            ],
        },
        "translate_notes": [
            TranslateNote::InstallInEmulator {
                emulator: EmulatorId::Rpcs3,
            },
            TranslateNote::ExtractFirst,
            TranslateNote::NotSupported {
                emulator: EmulatorId::RetroArch,
                why: NotSupportedWhy::SeveralPatches,
            },
            TranslateNote::HeaderMismatch {
                needs: HeaderFit::NeedsHeadered,
            },
            TranslateNote::PointEmulatorHere {
                emulator: EmulatorId::Ryujinx,
                folder: "DLC".into(),
            },
            TranslateNote::PatchNotApplied {
                emulator: EmulatorId::RetroArch,
            },
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
fn catalogue_statuses_match_the_shared_sample() {
    use coverly_core::catalog::CatalogStatus;

    let actual = json!({
        "catalog_statuses": [
            CatalogStatus::Off,
            CatalogStatus::Pending,
            CatalogStatus::Downloading {
                done: 12_582_912,
                total: Some(91_750_400),
            },
            CatalogStatus::Downloading {
                done: 1_048_576,
                total: None,
            },
            CatalogStatus::Ready {
                count: 17_342,
                updated: "2026-10-30".into(),
                fallback: false,
            },
            CatalogStatus::Ready {
                count: 16_904,
                updated: "2026-10-30".into(),
                fallback: true,
            },
            CatalogStatus::FailedOld {
                updated: "2026-10-23".into(),
            },
            CatalogStatus::FailedNone,
            CatalogStatus::Rejected,
            CatalogStatus::RemoveFailed,
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

/// Every `State`, in declaration order; fails to compile when one is added.
#[allow(dead_code)]
fn every_integration_state_is_listed(s: coverly_core::integrations::State) {
    use coverly_core::integrations::State as S;
    match s {
        S::Off
        | S::Unavailable
        | S::NotConfigured
        | S::Saved
        | S::Connected
        | S::Error
        | S::Quota => {}
    }
}

#[test]
fn integration_views_match_the_shared_sample() {
    use coverly_core::integrations::{
        Input, KeychainKind, Notice, Problem, Reason, SsQuota, State,
    };
    use coverly_core::sources::{SourceTrouble, TroubleKind};
    use coverly_roms_lib::integrations::{FieldView, IntegrationView, TestResult};

    let field = |key, set| FieldView { key, set };
    let view = |id: &str, enabled, fields, state| IntegrationView {
        id: id.into(),
        enabled,
        built: true,
        fields,
        state,
        reason: None,
        checked_at: None,
        quota: None,
        quota_at: None,
    };
    let used_up = SsQuota {
        requests_today: 20_000,
        max_requests_per_day: 20_000,
        max_threads: 1,
    };
    let ss = |set| {
        vec![
            field(CredentialKey::ScreenscraperUser, set),
            field(CredentialKey::ScreenscraperPassword, set),
        ]
    };
    let actual = json!({
        "integration_views": [
            IntegrationView {
                reason: Some(Reason::Check(KeyCheck::Invalid)),
                checked_at: Some(1_793_577_600),
                ..view("steamgriddb", true, vec![field(CredentialKey::SteamgriddbApiKey, true)], State::Error)
            },
            IntegrationView {
                checked_at: Some(1_793_577_600),
                quota: Some(used_up),
                quota_at: Some(1_793_577_660),
                ..view("screenscraper", true, ss(true), State::Quota)
            },
            view("switch-catalog", false, vec![], State::Off),
            IntegrationView {
                reason: Some(Reason::Missing(Input::ApiKey)),
                ..view("steamgriddb", true, vec![field(CredentialKey::SteamgriddbApiKey, false)], State::NotConfigured)
            },
            IntegrationView {
                reason: Some(Reason::Keychain(KeychainKind::Locked)),
                ..view("screenscraper", true, ss(false), State::Error)
            },
            view("updates", true, vec![], State::Saved),
        ],
        "integration_states": [
            State::Off,
            State::Unavailable,
            State::NotConfigured,
            State::Saved,
            State::Connected,
            State::Error,
            State::Quota,
        ],
        "keychain_kinds": [
            KeychainKind::Unavailable,
            KeychainKind::Locked,
            KeychainKind::Denied,
            KeychainKind::Failed,
        ],
        "integration_test_result": TestResult {
            verdict: KeyCheck::Ok,
            quota: Some(SsQuota {
                requests_today: 1234,
                max_requests_per_day: 20_000,
                max_threads: 1,
            }),
        },
        "integration_test_result_bare": TestResult { verdict: KeyCheck::Invalid, quota: None },
        "source_troubles": [
            SourceTrouble { source: "steamgriddb".into(), kind: TroubleKind::MissingKey },
            SourceTrouble { source: "steamgriddb".into(), kind: TroubleKind::Auth },
            SourceTrouble { source: "screenscraper".into(), kind: TroubleKind::Exhausted },
        ],
        "integration_notices": [
            Notice { integration: "steamgriddb".into(), problem: Problem::Missing },
            Notice { integration: "screenscraper".into(), problem: Problem::Rejected },
            Notice { integration: "screenscraper".into(), problem: Problem::Quota },
            Notice { integration: "switch-catalog".into(), problem: Problem::Off },
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

/// Addon views D2: a game with every content field, an orphan linked to
/// it, a demo of it and a folder game that is its own game folder; the plain retro entry of `library_view` keeps its
/// shape (no content field at all).
#[test]
fn game_content_entries_match_the_shared_sample() {
    use coverly_core::art::children::{ChildArtMarks, ChildOrigin};
    use coverly_core::game_folder::ChildRole;

    let entry = |id: u32, rel: &str, name: &str, content: GameContent| EntryView {
        id,
        system: 1,
        rel_path: rel.into(),
        dir: rel.rsplit_once('/').map(|(d, _)| d.to_string()).unwrap(),
        name: name.into(),
        ext: "nsp".into(),
        size: 2_000_000,
        mtime: 1_700_000_000,
        regions: vec![],
        art: ArtMarks {
            cover: ArtMark::Present,
            screenshot: ArtMark::Unsupported,
            title: ArtMark::Unsupported,
        },
        content,
    };
    let game = entry(
        10,
        "switch/Tidewalker/Tidewalker (Rev 1) (v1.0.0).nsp",
        "Tidewalker (Rev 1) (v1.0.0)",
        GameContent {
            children: vec![
                ChildView {
                    index: 1,
                    rel_path: "switch/Tidewalker/Updates/Tidewalker (v1.0.2).nsp".into(),
                    name: "Tidewalker (v1.0.2)".into(),
                    role: ChildRole::Update,
                    version: Some("1.0.2".into()),
                    size: 300_000,
                    mtime: 1_700_000_100,
                    art: ChildArtMarks {
                        icon: true,
                        banner: false,
                        icon_origin: Some(ChildOrigin::Package),
                        banner_origin: None,
                    },
                },
                ChildView {
                    index: 0,
                    rel_path: "switch/Tidewalker/DLC/Tidewalker - Reef Pack.nsp".into(),
                    name: "Tidewalker - Reef Pack".into(),
                    role: ChildRole::Dlc,
                    version: None,
                    size: 50_000,
                    mtime: 1_700_000_200,
                    art: ChildArtMarks::default(),
                },
            ],
            version_group: Some("switch:tidewalker".into()),
            revision: Some("Rev 1".into()),
            version: Some("1.0.0".into()),
            installed: Some("1.0.2".into()),
            newer_elsewhere: Some(NewerView {
                entry: 11,
                version: "1.3.0".into(),
            }),
            addons: Some(FolderCountsView {
                mods: 1,
                cheats: 2,
                extras: 3,
                saves: 0,
                capped: true,
            }),
            ..GameContent::default()
        },
    );
    let orphan = entry(
        11,
        "switch/Tidewalker (Update)/Updates/Tidewalker (v1.3.0).nsp",
        "Tidewalker (v1.3.0)",
        GameContent {
            orphan: Some(ChildRole::Update),
            orphan_of: Some(10),
            version: Some("1.3.0".into()),
            ..GameContent::default()
        },
    );
    let demo = entry(
        12,
        "switch/Tidewalker (Demo).nsp",
        "Tidewalker (Demo)",
        GameContent {
            demo: Some(DemoView {
                of: Some(10),
                of_name: Some("Tidewalker (Rev 1) (v1.0.0)".into()),
            }),
            ..GameContent::default()
        },
    );
    // A folder game that is its own game folder (Columns keeps it in place).
    let folder = entry(
        13,
        "switch/Coral Drift",
        "Coral Drift",
        GameContent {
            addons: Some(FolderCountsView {
                mods: 0,
                cheats: 1,
                extras: 0,
                saves: 0,
                capped: false,
            }),
            folder_root: true,
            ..GameContent::default()
        },
    );
    let actual = json!({ "game_content_entries": [game, orphan, demo, folder] });
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

/// Part C D3, D11 b: the game panel's lazy facts (every field) and what
/// «Leer el arte incluido» answers.
#[test]
fn game_panel_answers_match_the_shared_sample() {
    use coverly_core::art::children::{ChildArtMarks, SkipWhy};
    use coverly_core::catalog::latest::{LatestKnown, LatestSource};
    use coverly_core::consoles::ctr_region::Certainty;
    use coverly_core::consoles::TitleKind;
    use coverly_core::game_folder::ChildRole;
    use coverly_roms_lib::library::details::{ChildDetailView, GameDetailsView, RegionVerdictView};
    use coverly_roms_lib::library::view::{ChildArtReadView, ChildSkipView};

    let details = GameDetailsView {
        request: 4,
        title_id: Some("0004000000055E00".into()),
        product_code: Some("CTR-P-AXXT".into()),
        region: Some(RegionVerdictView {
            letter: "S".into(),
            code: "CTR-P-AXXS".into(),
            region: Some(Region::Europe),
            country: Some(Region::Spain),
            certainty: Certainty::Certain,
        }),
        latest: Some(LatestKnown {
            version: "1.3.0".into(),
            source: LatestSource::TitleDb,
        }),
        latest_installed: Some(false),
        children: vec![
            ChildDetailView {
                index: 0,
                title_id: Some("0004000E00055E00".into()),
                kind: Some(TitleKind::Update),
            },
            ChildDetailView {
                index: 1,
                title_id: None,
                kind: None,
            },
        ],
    };
    let bare = GameDetailsView {
        request: 5,
        ..GameDetailsView::default()
    };
    let read = ChildArtReadView {
        job: Some("20261113-120000-ab12".into()),
        written: 1,
        skipped: vec![ChildSkipView {
            index: 1,
            why: SkipWhy::NoArtInClear,
        }],
        children: vec![ChildView {
            index: 0,
            rel_path: "ROMS/3DS/Lantern Harbor/DLC/Lantern Harbor - Pack.cia".into(),
            name: "Lantern Harbor - Pack".into(),
            role: ChildRole::Dlc,
            version: None,
            size: 64,
            mtime: 1_700_000_000,
            art: ChildArtMarks {
                icon: true,
                banner: false,
                icon_origin: Some(coverly_core::art::children::ChildOrigin::Package),
                banner_origin: None,
            },
        }],
    };
    let nothing = ChildArtReadView {
        job: None,
        written: 0,
        skipped: vec![],
        children: vec![],
    };
    let actual = json!({
        "game_details": details,
        "game_details_bare": bare,
        "child_art_read": read,
        "child_art_read_nothing": nothing,
        "child_skip_reasons": [
            SkipWhy::KeyMaterial,
            SkipWhy::NoArtInClear,
            SkipWhy::Exists,
            SkipWhy::PathTooLong,
            SkipWhy::ReadOnly,
            SkipWhy::Refused,
        ],
        "region_certainties": [Certainty::Certain, Certainty::Likely, Certainty::Unknown],
        "unknown_child": IpcError::UnknownChild(3),
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
fn extras_views_match_the_shared_sample() {
    use coverly_core::extras::index::IndexState;
    use coverly_core::extras::{CategoryRef, ExtraFormat, ExtraType, ExtrasCategory, Provenance};
    use coverly_core::write::JobKind;
    use coverly_roms_lib::library::extras::{
        CategoryView, ExtraFileView, ExtrasView, MissingView, Playable, Writability,
    };

    let slot = |c: ExtrasCategory, present: bool, count: u32| CategoryView {
        category: CategoryRef::Fixed(c),
        present,
        count,
        link: false,
        sync: None,
    };
    let view = ExtrasView {
        entry_id: 7,
        rev: 12,
        game_folder: true,
        index: IndexState::Ok,
        writable: Writability::Yes,
        categories: vec![
            CategoryView {
                sync: Some(false),
                ..slot(ExtrasCategory::Soundtrack, true, 1)
            },
            slot(ExtrasCategory::Manuals, false, 0),
            slot(ExtrasCategory::Artwork, false, 0),
            slot(ExtrasCategory::Videos, false, 0),
            slot(ExtrasCategory::Themes, true, 1),
            slot(ExtrasCategory::Guides, false, 0),
            CategoryView {
                category: CategoryRef::User("Remixes".into()),
                present: true,
                count: 0,
                link: false,
                sync: None,
            },
            CategoryView {
                category: CategoryRef::Root,
                present: true,
                count: 1,
                link: false,
                sync: None,
            },
        ],
        files: vec![
            ExtraFileView {
                index: 0,
                category: CategoryRef::Fixed(ExtrasCategory::Soundtrack),
                path: "Soundtrack/01 Opening.flac".into(),
                name: "01 Opening.flac".into(),
                kind: ExtraType::Audio,
                provenance: Provenance::User,
                format: None,
                tags: vec!["tema del jefe final".into()],
                size: 31_457_280,
                link: false,
                playable: Playable::Inline,
            },
            ExtraFileView {
                index: 1,
                category: CategoryRef::Fixed(ExtrasCategory::Themes),
                path: "Themes/Resistance Theme.p3t".into(),
                name: "Resistance Theme.p3t".into(),
                kind: ExtraType::Other,
                provenance: Provenance::Official,
                format: Some(ExtraFormat::Ps3P3t),
                tags: vec![],
                size: 2_097_152,
                link: false,
                playable: Playable::RevealOnly,
            },
            ExtraFileView {
                index: 2,
                category: CategoryRef::Root,
                path: "track01.spc".into(),
                name: "track01.spc".into(),
                kind: ExtraType::Audio,
                provenance: Provenance::User,
                format: Some(ExtraFormat::Spc),
                tags: vec![],
                size: 66_048,
                link: false,
                playable: Playable::External,
            },
        ],
        missing: vec![MissingView {
            index: 0,
            category: CategoryRef::Fixed(ExtrasCategory::Videos),
            path: "Videos/intro.mp4".into(),
            name: "intro.mp4".into(),
            kind: ExtraType::Video,
            provenance: Provenance::Official,
            tags: vec!["intro".into()],
            size: 99,
        }],
        capped: false,
        system: "ps3".into(),
        manual_inside: false,
    };
    let bare = ExtrasView {
        entry_id: 8,
        rev: 13,
        game_folder: false,
        index: IndexState::Absent,
        writable: Writability::ReadOnly,
        categories: vec![],
        files: vec![],
        missing: vec![],
        capped: false,
        system: "3ds".into(),
        manual_inside: true,
    };
    let history = JobHistoryView {
        job: "20261114T120000Z-1bfb-0000".into(),
        root: "/Volumes/SD".into(),
        created_at: "2026-11-14T12:00:00Z".into(),
        entries: 2,
        complete: true,
        undone: false,
        kind: JobKind::Extras,
        undoable: true,
        game: Some("Resistance (USA)".into()),
    };
    let actual = json!({
        "extras_view": view,
        "extras_view_bare": bare,
        "extras_index_states": [
            IndexState::Ok,
            IndexState::Absent,
            IndexState::Malformed { bad_records: 2, unreadable: None },
            IndexState::Newer { version: 2 },
        ],
        "extras_writabilities": [
            Writability::Yes,
            Writability::ReadOnly,
            Writability::Busy,
            Writability::Newer,
        ],
        "extras_playables": [Playable::Inline, Playable::External, Playable::RevealOnly],
        "extras_errors": [
            IpcError::ExtrasChanged,
            IpcError::ExtrasNewer,
            IpcError::ExtrasRefused { why: "taken".into() },
        ],
        "history_item_extras": history,
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
fn extras_write_views_match_the_shared_sample() {
    use coverly_core::extras::index::IndexState;
    use coverly_core::extras::ops::{ExtrasError, ExtrasNote};
    use coverly_core::extras::{CategoryRef, ExtrasCategory, NameRefusal, TagRefusal};
    use coverly_roms_lib::library::extras::{
        ExtrasUndone, ExtrasView, ExtrasWriteView, Writability,
    };

    let listing = ExtrasView {
        entry_id: 7,
        rev: 14,
        game_folder: true,
        index: IndexState::Ok,
        writable: Writability::Yes,
        categories: vec![],
        files: vec![],
        missing: vec![],
        capped: false,
        system: "ps3".into(),
        manual_inside: false,
    };
    let write = ExtrasWriteView {
        job: Some("20261114T120000Z-1bfb-0001".into()),
        listing: listing.clone(),
        notes: vec![
            ExtrasNote::MalformedSaved {
                file: "extras.broken-20261114-120000.json".into(),
            },
            ExtrasNote::SetAside {
                folder: ".coverly-trash/20261114T120000Z-1bfb-0001".into(),
            },
            ExtrasNote::StaysNoTrash {
                name: "boss.mp3".into(),
            },
            ExtrasNote::TrashFailed {
                name: "intro.mp4".into(),
            },
            ExtrasNote::FolderKept {
                folder: "Remixes".into(),
            },
            ExtrasNote::RenamedDup {
                name: "boss (2).mp3".into(),
            },
        ],
    };
    let nothing = ExtrasWriteView {
        job: None,
        listing: listing.clone(),
        notes: vec![],
    };
    let undone = ExtrasUndone {
        job: "20261114T120000Z-1bfb-0001".into(),
        removed: 0,
        restored: 2,
        kept: 1,
        listing,
        updated: None,
    };
    let updated = EntriesUpdatedEvent {
        generation: 3,
        entries: vec![EntryArtUpdate {
            id: 7,
            art: ArtMarks {
                cover: ArtMark::Present,
                screenshot: ArtMark::Missing,
                title: ArtMark::Unsupported,
            },
            addons: Some(FolderCountsView {
                mods: 0,
                cheats: 1,
                extras: 4,
                saves: 0,
                capped: false,
            }),
        }],
    };
    let refusals: Vec<IpcError> = [
        ExtrasError::Name(NameRefusal::Empty),
        ExtrasError::Name(NameRefusal::TooLong),
        ExtrasError::Name(NameRefusal::NotPortable),
        ExtrasError::Name(NameRefusal::Dot),
        ExtrasError::Name(NameRefusal::Reserved),
        ExtrasError::Name(NameRefusal::Taken),
        ExtrasError::Name(NameRefusal::PathTooLong),
        ExtrasError::Tags(TagRefusal::Empty),
        ExtrasError::Tags(TagRefusal::TooLong),
        ExtrasError::Tags(TagRefusal::TooMany),
        ExtrasError::IndexUnreadable,
        ExtrasError::NoTrash,
        ExtrasError::TooMany,
        ExtrasError::Link("Soundtrack/link.mp3".into()),
    ]
    .into_iter()
    .map(IpcError::from)
    .collect();
    let actual = json!({
        "extras_write": write,
        "extras_write_nothing": nothing,
        "extras_undone": undone,
        "entries_updated_addons": updated,
        "extras_refusals": refusals,
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

    // What the webview sends: categories as the listing names them.
    let req: Value =
        serde_json::from_str(include_str!("../../src/ipc/contract.requests.json")).unwrap();
    let refs: Vec<CategoryRef> =
        serde_json::from_value(req["extras_category_refs"].clone()).unwrap();
    assert_eq!(
        refs,
        [
            CategoryRef::Fixed(ExtrasCategory::Soundtrack),
            CategoryRef::User("Remixes".into()),
            CategoryRef::Root,
        ]
    );
}

/// Adds from a picker or an OS drop (part D D11, D13): plans by index and
/// drops by token, never a path; background progress and end.
#[test]
fn extras_add_views_match_the_shared_sample() {
    use coverly_core::extras::ops::ExtrasNote;
    use coverly_core::extras::{CategoryRef, ExtraType, ExtrasCategory, RipTags};
    use coverly_roms_lib::library::extras::{
        AddChoice, AddEnd, AddFileView, AddFinishedEvent, AddPlanView, AddProgressEvent,
        AddStarted, DropEvent, DropPosition, DroppedItem, ExtrasView, ExtrasWriteView, Writability,
    };

    let plan = AddPlanView {
        plan: 3,
        entry_id: 7,
        files: vec![
            AddFileView {
                index: 0,
                name: "01 Opening.spc".into(),
                size: 66_048,
                kind: ExtraType::Audio,
                category: ExtrasCategory::Soundtrack,
                rip: Some(RipTags {
                    game: Some("Super Metroid".into()),
                    title: Some("Opening".into()),
                    ..RipTags::default()
                }),
            },
            AddFileView {
                index: 1,
                name: "manual.pdf".into(),
                size: 1_200_000,
                kind: ExtraType::Document,
                category: ExtrasCategory::Manuals,
                rip: None,
            },
        ],
        total: 1_266_048,
        keys: 2,
        hidden: 1,
        too_many: false,
    };
    let listing = ExtrasView {
        entry_id: 7,
        rev: 15,
        game_folder: true,
        index: coverly_core::extras::index::IndexState::Ok,
        writable: Writability::Yes,
        categories: vec![],
        files: vec![],
        missing: vec![],
        capped: false,
        system: "snes".into(),
        manual_inside: false,
    };
    let write = ExtrasWriteView {
        job: Some("20261114T120000Z-1bfb-0002".into()),
        listing,
        notes: vec![
            ExtrasNote::Sanitised {
                names: vec!["a_b.mp3".into()],
            },
            ExtrasNote::KeysSkipped { n: 2 },
            ExtrasNote::HiddenSkipped { n: 1 },
            ExtrasNote::Cancelled { copied: 3 },
        ],
    };
    let actual = json!({
        "extras_add_plan": plan,
        "extras_add_started_done": AddStarted::Done { write: write.clone() },
        "extras_add_started_background": AddStarted::Background { add: 3, n: 12, total: 4_000_000_000u64 },
        "extras_add_progress": AddProgressEvent { entry_id: 7, add: 3, i: 3, n: 12, done: 1_200_000_000, total: 4_000_000_000 },
        "extras_added_done": AddFinishedEvent { entry_id: 7, add: 3, end: AddEnd::Done { write } },
        "extras_added_failed": AddFinishedEvent {
            entry_id: 7,
            add: 4,
            end: AddEnd::Failed { error: IpcError::ExtrasRefused { why: "too-many".into() } },
        },
        "extras_drop": DropEvent {
            token: 9,
            position: DropPosition { x: 812.0, y: 430.5 },
            items: vec![
                DroppedItem { name: "01 Opening.flac".into(), size: 31_457_280, is_dir: false },
                DroppedItem { name: "Scans".into(), size: 0, is_dir: true },
            ],
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

    // What the dialog sends: a category per chosen file of the plan.
    let req: Value =
        serde_json::from_str(include_str!("../../src/ipc/contract.requests.json")).unwrap();
    let choices: Vec<AddChoice> =
        serde_json::from_value(req["extras_add_choices"].clone()).unwrap();
    assert_eq!(
        choices,
        [
            AddChoice {
                index: 0,
                category: CategoryRef::Fixed(ExtrasCategory::Soundtrack),
            },
            AddChoice {
                index: 1,
                category: CategoryRef::User("Escaneos".into()),
            },
        ]
    );
}

/// Part D in Organizar and the viewer: an extra row, its reasons and
/// notes, the extras vocabularies, a granted media file and a text view.
#[test]
fn extras_organize_and_media_values_match_the_shared_sample() {
    use coverly_core::addons::LinkVia;
    use coverly_core::extras::{ExtraFormat, ExtraType, ExtrasCategory, Provenance};
    use coverly_core::identify::evidence::{Evidence, IdLevel};
    use coverly_core::organize::plan::ExtraLink;
    use coverly_core::organize::{
        GameLink, GameRole, ItemKind, ItemNote, LeftReason, OrganizeAction, OrganizeItem, SourceFp,
        Transfer,
    };
    use coverly_roms_lib::library::extras_media::{MediaKind, MediaView, TextEncoding, TextView};

    let mut link = GameLink::new(GameRole::Extra);
    link.of = Some(0);
    link.folder = Some("PS3/Tidewalker (USA)".into());
    link.extra = Some(ExtraLink {
        category: ExtrasCategory::Themes,
        format: ExtraFormat::Ps3P3t,
        provenance: Provenance::Official,
        via: LinkVia::Key,
        content_type: None,
        rip: None,
    });
    let extra = OrganizeItem {
        id: 1,
        sources: vec!["Tidewalker Theme.p3t".into()],
        kind: ItemKind::File,
        size: 2_097_152,
        fps: vec![SourceFp {
            size: 2_097_152,
            mtime_ns: 1_700_000_000_000_000_000,
        }],
        target: Some("PS3/Tidewalker (USA)/Extras/Themes/Tidewalker Theme.p3t".into()),
        set_targets: vec![],
        system_id: Some("ps3".into()),
        folder: Some("PS3".into()),
        confidence: 0.95,
        method: Some(IdLevel::Header),
        why: vec![Evidence::new(IdLevel::Header, "ps3", 0.95, "BCUS98174")],
        alternatives: vec![],
        rename: None,
        notes: vec![ItemNote::ExtraOf {
            category: ExtrasCategory::Themes,
            of: "Tidewalker".into(),
            format: ExtraFormat::Ps3P3t,
            content_type: None,
        }],
        companions: vec![],
        catalogue_name: None,
        transfer: Transfer::AsIs,
        disc: None,
        title_id: None,
        content_id: None,
        playlist: None,
        title_source: None,
        game: Some(link),
        action: OrganizeAction::Auto,
    };
    let extra: Value = serde_json::from_str(&serde_json::to_string(&extra).unwrap()).unwrap();
    let actual = json!({
        "extra_item": extra,
        "extra_reasons": [
            LeftReason::ExtraWithoutGame {
                category: ExtrasCategory::Soundtrack,
            },
            LeftReason::ExtrasIndexNewer,
        ],
        "extra_notes": [
            ItemNote::ExtraOf {
                category: ExtrasCategory::Themes,
                of: "Tidewalker".into(),
                format: ExtraFormat::Ps3ThemePkg,
                content_type: Some(9),
            },
            ItemNote::ExtraRip {
                of: "Super Metroid".into(),
                format: ExtraFormat::Spc,
                tag: "Super Metroid".into(),
            },
            ItemNote::ExtrasBackupFirst,
        ],
        "extras_categories": ExtrasCategory::ALL,
        "extra_types": [
            ExtraType::Audio,
            ExtraType::Video,
            ExtraType::Image,
            ExtraType::Document,
            ExtraType::Other,
        ],
        "extra_formats": [
            ExtraFormat::Ps3P3t,
            ExtraFormat::Ps3ThemePkg,
            ExtraFormat::Ps3AvatarPkg,
            ExtraFormat::VitaThemePkg,
            ExtraFormat::PspPtf,
            ExtraFormat::X360Theme,
            ExtraFormat::X360Gamerpic,
            ExtraFormat::X360Avatar,
            ExtraFormat::X360Video,
            ExtraFormat::N3dsSoundtrackCia,
            ExtraFormat::Spc,
            ExtraFormat::Nsf,
            ExtraFormat::Gbs,
            ExtraFormat::Vgm,
            ExtraFormat::Psf,
        ],
        "extras_media": MediaView {
            src_path: "/Volumes/SD/PS3/Tidewalker (USA)/Extras/Soundtrack/01 Opening.flac".into(),
            mime: "audio/flac".into(),
            kind: MediaKind::Audio,
        },
        "extras_media_kinds": [MediaKind::Audio, MediaKind::Video, MediaKind::Image, MediaKind::Pdf],
        "extras_text": TextView {
            text: "Tidewalker\n".into(),
            truncated: false,
            encoding: TextEncoding::Cp437,
        },
        "extras_text_encodings": [TextEncoding::Utf8, TextEncoding::Cp437, TextEncoding::Latin1],
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

/// The tag rule the webview mirrors (D20): every case of the shared table
/// gives the same tags, or the same refusal, in Rust.
#[test]
fn extras_tag_cases_match_rust() {
    use coverly_core::extras::normalize_tags;

    let req: Value =
        serde_json::from_str(include_str!("../../src/ipc/contract.requests.json")).unwrap();
    let cases = req["extras_tag_cases"].as_array().unwrap();
    assert!(cases.len() >= 5);
    for case in cases {
        let input: Vec<String> = serde_json::from_value(case["input"].clone()).unwrap();
        let got = match normalize_tags(&input) {
            Ok(tags) => json!({ "tags": tags }),
            Err(why) => json!({ "refused": why }),
        };
        let mut want = case.clone();
        want.as_object_mut().unwrap().remove("input");
        assert_eq!(got, want, "tags {input:?}");
    }
}
