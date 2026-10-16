//! The wire contract shared with `src/ipc/types.ts`: Rust must serialise
//! exactly `src/ipc/contract.sample.json`.

use coverly_core::identify::names::Region;
use coverly_core::identify::IdMethod;
use coverly_core::jobs::ItemErrorKind;
use coverly_core::matching::StrategyKind;
use coverly_core::plan::{Counts, Decision, PlanNote};
use coverly_core::profiles::Media;
use coverly_core::progress::{PauseKind, Phase};
use coverly_core::sources::MatchKind;
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
use serde_json::{json, Value};

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
        "settings": LibrarySettingsView { ignored_dirs: vec!["bios".into()] },
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
    });
    let expected: Value =
        serde_json::from_str(include_str!("../../src/ipc/contract.sample.json")).unwrap();
    assert_eq!(actual, expected);
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
