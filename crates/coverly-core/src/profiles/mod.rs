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

pub use builtin::builtin_profiles;
pub use detect::{detect, Detection};
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
}

impl Profile {
    /// The slot for `media`, if this layout can store it.
    pub fn slot(&self, media: Media) -> Option<&MediaSlot> {
        self.slots.get(&media)
    }
}
