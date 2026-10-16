//! What the webview sees of a library: compact, serialisable, path-free
//! except for display strings. Field names are the wire contract mirrored
//! by `src/ipc/types.ts` (pinned by `tests/contract.rs`).

use std::collections::BTreeMap;

use coverly_core::art::ArtState;
use coverly_core::identify::names::Region;
use coverly_core::profiles::Media;
use coverly_core::scan::ScanProgress;
use serde::Serialize;

/// State of one media for one entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ArtMark {
    Present,
    Missing,
    Unsupported,
}

/// State of every media for one entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ArtMarks {
    pub cover: ArtMark,
    pub screenshot: ArtMark,
    pub title: ArtMark,
}

impl ArtMarks {
    /// Marks from the core's art states (absent media are unsupported).
    pub fn from_states(states: &BTreeMap<Media, ArtState>) -> ArtMarks {
        let mark = |media: Media| match states.get(&media) {
            Some(ArtState::Present { .. }) => ArtMark::Present,
            Some(ArtState::Missing) => ArtMark::Missing,
            _ => ArtMark::Unsupported,
        };
        ArtMarks {
            cover: mark(Media::Cover),
            screenshot: mark(Media::Screenshot),
            title: mark(Media::Title),
        }
    }
}

/// One ROM.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct EntryView {
    /// Index in [`LibraryView::entries`]; the handle for every later call.
    pub id: u32,
    /// Index in [`LibraryView::systems`].
    pub system: u16,
    /// Primary file relative to the library root, `/`-separated.
    pub rel_path: String,
    /// Folder of `rel_path` (`""` at the root).
    pub dir: String,
    /// File name without extension.
    pub name: String,
    /// Extension, lowercase, without the dot.
    pub ext: String,
    /// Bytes, companions included.
    pub size: u64,
    /// Last modification, Unix seconds.
    pub mtime: i64,
    /// Regions parsed from the name.
    pub regions: Vec<Region>,
    pub art: ArtMarks,
}

/// One folder of the ROMs root.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SystemView {
    pub index: u16,
    /// Folder name as on disk, e.g. `SFC`.
    pub folder: String,
    /// Folder relative to the library root, `/`-separated.
    pub dir: String,
    /// `None` for folders that match no system (listed, not scanned).
    pub system_id: Option<String>,
    /// System display name, or the folder name when unknown.
    pub name: String,
    pub roms: u32,
}

/// The layout profile used.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ProfileView {
    pub id: String,
    pub name: String,
    /// Media this layout can store, in display order.
    pub media: Vec<Media>,
    /// Auto-detected (vs chosen by id).
    pub detected: bool,
    /// Markers that were found when detected.
    pub evidence: Vec<String>,
    /// The picked folder is the ROMs folder itself (see `fit_to_root`).
    pub fitted: bool,
    /// Fills also merge the art paths into `gamelist.xml` (D31).
    pub gamelist: bool,
}

/// A problem met while scanning.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct WarningView {
    pub path: String,
    pub message: String,
}

/// Everything the explorer shows.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LibraryView {
    /// Identifies this opening; later calls must pass it back.
    pub generation: u64,
    /// The folder as picked, for display.
    pub root: String,
    /// Its last component.
    pub name: String,
    pub profile: ProfileView,
    pub systems: Vec<SystemView>,
    pub entries: Vec<EntryView>,
    pub warnings: Vec<WarningView>,
    /// Scan + art resolution time.
    pub scan_ms: u64,
}

/// Payload of the `library-scan-progress` event.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ScanProgressEvent {
    pub generation: u64,
    pub done: usize,
    pub total: usize,
    pub folder: String,
    pub roms: usize,
}

impl ScanProgressEvent {
    pub fn new(generation: u64, p: ScanProgress) -> ScanProgressEvent {
        ScanProgressEvent {
            generation,
            done: p.done,
            total: p.total,
            folder: p.folder,
            roms: p.roms,
        }
    }
}

/// A recent library, with whether its folder is reachable right now.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RecentView {
    pub path: String,
    pub name: String,
    pub profile_id: String,
    pub roms: u32,
    pub opened_at: i64,
    pub available: bool,
}

/// The library settings the explorer edits.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LibrarySettingsView {
    pub ignored_dirs: Vec<String>,
}
