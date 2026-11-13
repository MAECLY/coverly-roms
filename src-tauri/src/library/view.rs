//! What the webview sees of a library: compact, serialisable, path-free
//! except for display strings. Field names are the wire contract mirrored
//! by `src/ipc/types.ts` (pinned by `tests/contract.rs`).

use std::collections::BTreeMap;

use coverly_core::art::children::{ChildArtMarks, SkipWhy};
use coverly_core::art::ArtState;
use coverly_core::game_folder::ChildRole;
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
    /// What the game holds and what it is to other entries (addon views
    /// D2): every field is left out when empty, so an ordinary retro entry
    /// keeps its shape.
    #[serde(flatten)]
    pub content: GameContent,
}

/// The game-content fields of an [`EntryView`] (addon views D2, D4–D6).
/// Facts and orderings are decided here, in Rust; the webview only folds.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct GameContent {
    /// The updates and DLC in the game's folder, newest version first
    /// (by `version_order`), then by name.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<ChildView>,
    /// Set on an update or DLC with no base at its folder's root.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orphan: Option<ChildRole>,
    /// The entry id of the game an orphan belongs to, by title key (D6).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orphan_of: Option<u32>,
    /// The revisions of one retro game share it; sent only when the group
    /// has more than one member in the system.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version_group: Option<String>,
    /// The revision as written (`Rev 1`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    /// Set on a demo, with its full game when the library has it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub demo: Option<DemoView>,
    /// The entry's own version, only when its name carries one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// The newest version in the library for this game (D5).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installed: Option<String>,
    /// A linked orphan update newer than `installed` (D6, in-library).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub newer_elsewhere: Option<NewerView>,
    /// File counts of the game folder's `Mods/`, `Cheats/`, `Extras/` and
    /// `Saves/` (D9), on a base at a game folder's root.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub addons: Option<FolderCountsView>,
    /// Set on a folder game whose own folder is its game folder (it holds
    /// the `Updates/`, `DLC/` or fixed folders): the entry already is that
    /// folder, so Columns lists it where it is instead of a folder up.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub folder_root: bool,
}

/// An update or DLC inside a game's folder. Addressed by the entry id and
/// `index`; `rel_path` is for display only.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ChildView {
    /// Position in the scan's `RomEntry::children`.
    pub index: u32,
    /// Relative to the library root, `/`-separated.
    pub rel_path: String,
    /// File name without extension (a folder child's whole name).
    pub name: String,
    pub role: ChildRole,
    /// The version its name carries (`(v1.05)`, `[v131072]`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub size: u64,
    /// Unix seconds.
    pub mtime: i64,
    pub art: ChildArtMarks,
}

/// A child the «Leer el arte incluido» job left without new art, and why.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ChildSkipView {
    /// `ChildView.index`.
    pub index: u32,
    pub why: SkipWhy,
}

/// What `read_child_art` did (part C D11 b).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ChildArtReadView {
    /// The job to undo; absent when nothing was written.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job: Option<String>,
    /// Slot files written.
    pub written: u32,
    pub skipped: Vec<ChildSkipView>,
    /// The game's children again, with their new art marks (same order
    /// as `EntryView.children`).
    pub children: Vec<ChildView>,
}

/// What a demo is linked to.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct DemoView {
    /// The full game's entry id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub of: Option<u32>,
    /// The full game's name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub of_name: Option<String>,
}

/// A newer update of the game that is in the library, outside its folder.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct NewerView {
    /// The orphan's entry id.
    pub entry: u32,
    pub version: String,
}

/// File counts of a game folder's non-game fixed folders.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct FolderCountsView {
    pub mods: u32,
    pub cheats: u32,
    pub extras: u32,
    pub saves: u32,
    /// A count reached the cap; only sent when true.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub capped: bool,
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
    /// ROM-like files directly in the root or the ROMs folder, outside any
    /// system folder: what "Organise loose files" offers to sort.
    pub loose_files: usize,
    /// The revision chosen per version group in this library (D7):
    /// version group → `rel_path`. Left out when none was chosen; the
    /// webview falls back to the highest revision when a path is not a
    /// member of its group any more.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub revision_defaults: BTreeMap<String, String>,
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
