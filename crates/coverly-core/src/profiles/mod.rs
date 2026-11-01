//! Layout profiles: where a frontend or handheld firmware expects ROMs and
//! each kind of art.
//!
//! A [`Profile`] names the folder holding the system dirs, the folders that
//! hold art (never scanned as ROMs) and one [`MediaSlot`] per media type it
//! can store. Slot paths are templates rendered with [`render`]; [`detect`]
//! guesses the profile of a library from marker folders.

mod builtin;
pub mod custom;
mod detect;
mod fit;
pub mod naming;
mod template;

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

use crate::archive::ContainerKind;
use crate::systems::System;

pub use builtin::builtin_profiles;
pub use detect::{detect, detect_with_fallback, Detection};
pub use fit::fit_to_root;
pub use template::{render, TemplateVars};

/// How organise names the system folders it creates (spec §8.1).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FolderNaming {
    /// The system's first folder alias (`GBA`, `SFC`, `FC`…): handheld SD
    /// card names.
    #[default]
    FirstAlias,
    /// The system id (`gba`, `snes`…), as frontends name their folders.
    Id,
}

/// Where organise extracts a disc game's sheet and tracks (spec §8.2,
/// research R1 in `docs/02-research/layouts.md`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum DiscLayout {
    /// Sheet and tracks straight into the system folder (`PS/<cue>`).
    Flat {
        /// Generate `<stem>.m3u` beside a multi-disc game's sheets.
        #[serde(default)]
        m3u_for_multi: bool,
    },
    /// One folder per game, named like its primary sheet
    /// (`PS/<stem>/<stem>.cue`).
    GameFolder {
        /// Generate `<stem>/<stem>.m3u` for a multi-disc game.
        #[serde(default)]
        m3u_for_multi: bool,
    },
}

impl Default for DiscLayout {
    fn default() -> Self {
        Self::Flat {
            m3u_for_multi: false,
        }
    }
}

impl DiscLayout {
    /// Whether each game gets its own folder.
    pub fn is_game_folder(self) -> bool {
        matches!(self, Self::GameFolder { .. })
    }

    /// Whether a multi-disc game gets a generated `.m3u`.
    pub fn m3u_for_multi(self) -> bool {
        match self {
            Self::Flat { m3u_for_multi } | Self::GameFolder { m3u_for_multi } => m3u_for_multi,
        }
    }
}

/// The containers a profile loads as they are when nothing else is known
/// (spec §8.3): zip and 7z.
pub fn default_accepts_containers() -> Vec<ContainerKind> {
    vec![ContainerKind::Zip, ContainerKind::SevenZip]
}

/// Systems whose emulators read no archive (R1-9, R1-20: spruceOS lists
/// none for PSP and the PPSSPP core reads none).
const NO_ARCHIVE_SYSTEMS: [&str; 1] = ["psp"];

/// The per-system container overrides every profile starts from: the
/// systems in [`NO_ARCHIVE_SYSTEMS`] always extract, and a `ps5` app
/// archive keeps its zip, 7z or rar (systems batch 3 D13: the archive is
/// the app's distribution form, not a compressed ROM).
pub fn default_container_overrides() -> BTreeMap<String, Vec<ContainerKind>> {
    let mut m: BTreeMap<String, Vec<ContainerKind>> = NO_ARCHIVE_SYSTEMS
        .iter()
        .map(|s| (s.to_string(), Vec::new()))
        .collect();
    m.insert(
        "ps5".to_string(),
        vec![
            ContainerKind::Zip,
            ContainerKind::SevenZip,
            ContainerKind::Rar,
        ],
    );
    m
}

/// A built-in frontend's folder for one system (systems batch 1, spec D5
/// and §6.1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProfileFolder {
    /// The frontend's folder for the system (spec §6.1).
    Named(String),
    /// The frontend has no folder for it: organise uses the naming rule
    /// and marks the folder as not on the device (D5).
    Absent,
}

/// A kind of game art.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Media {
    /// Box art (front cover).
    Cover,
    /// In-game screenshot.
    Screenshot,
    /// Title screen.
    Title,
}

impl Media {
    /// Every media type, in display order.
    pub const ALL: [Media; 3] = [Media::Cover, Media::Screenshot, Media::Title];
}

/// Image file format a slot is written in.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageFormat {
    /// PNG.
    Png,
    /// JPEG.
    Jpg,
}

/// Where and how a profile stores one media type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediaSlot {
    /// Path template, e.g. `{system_dir}/{rel_dir}/Imgs/{stem}.png`
    /// (see [`render`] for the tokens).
    pub template: String,
    /// Format the image is written in.
    pub format: ImageFormat,
    /// Largest size the frontend displays well, as `(width, height)`.
    pub max_size: Option<(u32, u32)>,
}

/// A layout definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Profile {
    /// Stable id, e.g. `anbernic-stock`.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Folder, relative to the library root, whose children are the system
    /// dirs: `""` (the root itself), `"ROMS"`, `"Roms"`… Compared
    /// case-insensitively.
    pub roms_root: String,
    /// Folder names that hold art and are never scanned as ROMs
    /// (compared case-insensitively).
    pub art_dir_names: Vec<String>,
    /// Whether art mirrors ROM subfolders (templates use `{rel_dir}`). Flat
    /// layouts can have stem collisions between subfolders.
    pub mirror_subdirs: bool,
    /// One slot per media the profile can store; absent media cannot be
    /// stored by this layout.
    pub slots: HashMap<Media, MediaSlot>,
    /// The frontend reads one format only (RetroArch: PNG); the output
    /// format setting does not apply.
    pub fixed_format: bool,
    /// The frontend reads art paths from `<system dir>/gamelist.xml`
    /// (Batocera/Knulli): fills merge them in (spec §16.8, D31).
    pub gamelist: bool,
    /// How organise names system folders (spec §8.1).
    pub system_folder: FolderNaming,
    /// Systems the device keeps in another system's folder (Anbernic:
    /// `ngpc → ngp`, `wonderswancolor → wonderswan`).
    pub host_folders: BTreeMap<String, String>,
    /// Arcade systems, preferred first, when a set is in several DATs.
    pub arcade_order: Vec<String>,
    /// Folders the device shows, when known (empty = unknown).
    pub known_folders: Vec<String>,
    /// Where disc games are extracted (spec §8.2).
    pub disc_layout: DiscLayout,
    /// Containers whose cartridge ROMs are moved as they are; any other
    /// container is extracted (spec §8.3).
    pub accepts_containers: Vec<ContainerKind>,
    /// Per-system replacements of `accepts_containers` (system id →
    /// containers; empty = always extract).
    pub container_overrides: BTreeMap<String, Vec<ContainerKind>>,
    /// Per-system folders of this frontend (system id → folder), checked
    /// before the global aliases (spec D5, D6). Systems not listed use the
    /// naming rule as before.
    pub folders: BTreeMap<String, ProfileFolder>,
    /// Suffix a folder game of a system must end in on this layout
    /// (system id → suffix with its dot: `ps3` → `.ps3` on ES-DE and
    /// Batocera, systems batch 2 D13). Empty elsewhere.
    pub folder_game_suffix: BTreeMap<String, String>,
    /// The frontend names the media of a folder game after the folder's
    /// whole name, suffix included (ES-DE: `getStem` keeps a directory's
    /// extension, so `Game.ps3/` has `Game.ps3.png`). Otherwise the stem
    /// drops the suffix (Batocera cuts at the last dot, folders too).
    pub folder_stem_keeps_suffix: bool,
    /// Folder, relative to a system's folder, where the frontend reads a
    /// system's DLC (system id → template whose only token is
    /// `{title_id}`, the base game's id): Batocera `ps4 → DLC/{title_id}`
    /// (systems batch 3 D16). Empty elsewhere: DLC sits beside its game.
    pub dlc_folder: BTreeMap<String, String>,
}

impl Profile {
    /// The system id whose `Named` folder is `name` (case-insensitive).
    pub fn folder_owner(&self, name: &str) -> Option<&str> {
        let key = name.trim().to_lowercase();
        if key.is_empty() {
            return None;
        }
        self.folders.iter().find_map(|(id, f)| match f {
            ProfileFolder::Named(n) if n.to_lowercase() == key => Some(id.as_str()),
            _ => None,
        })
    }

    /// The slot for `media`, if this layout can store it.
    pub fn slot(&self, media: Media) -> Option<&MediaSlot> {
        self.slots.get(&media)
    }

    /// Whether a `system` ROM inside a `kind` container is moved as it is
    /// (`false` = extracted). Disc systems never take an archive as-is,
    /// whatever the profile says: their sets are extracted (spec D4).
    pub fn accepts_as_is(&self, system: &System, kind: ContainerKind) -> bool {
        if system.multi_file {
            return false;
        }
        self.container_overrides
            .get(&system.id)
            .unwrap_or(&self.accepts_containers)
            .contains(&kind)
    }
}
