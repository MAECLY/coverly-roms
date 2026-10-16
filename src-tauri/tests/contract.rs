//! The wire contract shared with `src/ipc/types.ts`: Rust must serialise
//! exactly `src/ipc/contract.sample.json`.

use coverly_core::identify::names::Region;
use coverly_core::identify::IdMethod;
use coverly_core::profiles::Media;
use coverly_roms_lib::error::IpcError;
use coverly_roms_lib::library::identify::IdentityView;
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
        "error": IpcError::NotFound("/Volumes/SD".into()),
    });
    let expected: Value =
        serde_json::from_str(include_str!("../../src/ipc/contract.sample.json")).unwrap();
    assert_eq!(actual, expected);
}
