//! Layout profiles: where a frontend or handheld firmware expects ROMs and
//! each kind of art.
//!
//! A [`Profile`] names the folder holding the system dirs, the folders that
//! hold art (never scanned as ROMs) and one [`MediaSlot`] per media type it
//! can store. Slot paths are templates rendered with [`render`]; [`detect`]
//! guesses the profile of a library from marker folders.

mod builtin;
mod detect;
mod fit;
mod template;

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub use builtin::builtin_profiles;
pub use detect::{detect, Detection};
pub use fit::fit_to_root;
pub use template::{render, TemplateVars};

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
}

impl Profile {
    /// The slot for `media`, if this layout can store it.
    pub fn slot(&self, media: Media) -> Option<&MediaSlot> {
        self.slots.get(&media)
    }
}
