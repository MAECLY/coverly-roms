//! Art slots of a game's updates and DLC (part C D10).
//!
//! Each child file of a game folder has two slots of its own, beside it:
//! `<dir of child>/.media/<stem>-icon.png` (square) and
//! `<stem>-banner.png`, with where each came from in
//! `<dir of child>/.media/<stem>.art.json`. They are a separate
//! [`ChildMedia`] enum, not new [`crate::profiles::Media`] values, so the
//! profile slots of top-level games and the fill matrix are untouched.
//! Only the `coverly-library` layout stores them (other layouts have no
//! game folders).
//!
//! A slot path longer than [`WINDOWS_MAX_PATH`] UTF-16 units, or a slot
//! file name over [`MAX_NAME_BYTES`] bytes, is refused with
//! [`SlotError::PathTooLong`] on every OS, never truncated: a truncated
//! name would no longer map back to its child, and the library has to stay
//! portable to Windows and FAT cards. Two children with the same stem
//! (`x.pkg`, `x.nsp`) share their slots, the same rule as top-level
//! entries.
//!
//! This module only names paths and reads; writes go through the journaled
//! writer (`write::ensure_within`, backup, undo).

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::organize::target::{MAX_NAME_BYTES, WINDOWS_MAX_PATH};

/// Folder beside a child that holds its art (the library layout's art
/// folder).
pub const CHILD_MEDIA_DIR: &str = ".media";

/// Suffix of the origin file: `<stem>.art.json`.
const ART_JSON_SUFFIX: &str = ".art.json";

/// Larger origin files are not ours: read as empty.
const MAX_ART_JSON_BYTES: u64 = 64 * 1024;

/// A slot of a child's art.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ChildMedia {
    /// Square icon (`<stem>-icon.png`).
    Icon,
    /// Wide banner (`<stem>-banner.png`).
    Banner,
}

impl ChildMedia {
    /// Both slots, in display order.
    pub const ALL: [ChildMedia; 2] = [ChildMedia::Icon, ChildMedia::Banner];

    /// The suffix the slot adds to the child's stem, extension included.
    pub fn file_suffix(self) -> &'static str {
        match self {
            ChildMedia::Icon => "-icon.png",
            ChildMedia::Banner => "-banner.png",
        }
    }
}

/// Where a child's art came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ChildOrigin {
    /// Read in the clear from the child's own package (D11).
    Package,
    /// Picked by the user from a ScreenScraper search.
    Screenscraper,
    /// Picked by the user from a SteamGridDB search.
    Steamgriddb,
    /// Chosen by the user from a file.
    User,
}

/// Where one slot's file came from and when (unix seconds).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OriginStamp {
    /// The source.
    pub origin: ChildOrigin,
    /// When it was written, unix seconds.
    pub at: i64,
}

/// The contents of `<stem>.art.json`: the origin of each slot.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtJson {
    /// Origin of the icon, if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<OriginStamp>,
    /// Origin of the banner, if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub banner: Option<OriginStamp>,
}

impl ArtJson {
    /// The stamp of `media`.
    pub fn get(&self, media: ChildMedia) -> Option<&OriginStamp> {
        match media {
            ChildMedia::Icon => self.icon.as_ref(),
            ChildMedia::Banner => self.banner.as_ref(),
        }
    }

    /// Sets (or clears) the stamp of `media`.
    pub fn set(&mut self, media: ChildMedia, stamp: Option<OriginStamp>) {
        match media {
            ChildMedia::Icon => self.icon = stamp,
            ChildMedia::Banner => self.banner = stamp,
        }
    }

    /// The bytes written to `<stem>.art.json` (pretty JSON, trailing
    /// newline).
    pub fn to_bytes(&self) -> Vec<u8> {
        // Serialising a struct of options and plain enums cannot fail.
        let mut bytes = serde_json::to_vec_pretty(self).unwrap_or_default();
        bytes.push(b'\n');
        bytes
    }
}

/// Which slots of a child have a file, and where each came from (only
/// for a slot that has one).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ChildArtMarks {
    /// `<stem>-icon.png` exists.
    pub icon: bool,
    /// `<stem>-banner.png` exists.
    pub banner: bool,
    /// Origin of the icon, from `.art.json`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_origin: Option<ChildOrigin>,
    /// Origin of the banner, from `.art.json`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub banner_origin: Option<ChildOrigin>,
}

/// Why a child has no slot path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotError {
    /// The slot path is over [`WINDOWS_MAX_PATH`] UTF-16 units or its
    /// file name over [`MAX_NAME_BYTES`] bytes (the «path too long» item
    /// reason). Checked on every OS.
    PathTooLong,
    /// The child path has no parent folder or no file name, or its name
    /// is not UTF-8.
    NotAChild,
}

/// The stem a child's slots are named after: the file name without its
/// extension, or a folder child's whole name (folders are named like
/// top-level folder entries, whose stem is the whole name).
///
/// `None` for a name that is not UTF-8: its slot names would carry U+FFFD,
/// so two such children would share slots and the file would no longer map
/// back to its child (and no portable card stores such a name anyway).
pub fn child_stem(child: &Path) -> Option<String> {
    let stem = if child.is_dir() {
        child.file_name()
    } else {
        child.file_stem()
    };
    stem.and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

/// The child's art folder: `<dir of child>/.media`.
fn media_dir(child: &Path) -> Option<PathBuf> {
    child
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(|p| p.join(CHILD_MEDIA_DIR))
}

/// `<dir of child>/.media/<stem>-icon.png` (or `-banner.png`).
///
/// [`SlotError::PathTooLong`] when the path is over 259 UTF-16 units or
/// the slot's file name over 255 bytes, on every OS.
pub fn child_slot_path(child: &Path, media: ChildMedia) -> Result<PathBuf, SlotError> {
    let dir = media_dir(child).ok_or(SlotError::NotAChild)?;
    let stem = child_stem(child).ok_or(SlotError::NotAChild)?;
    slot_in(&dir, &stem, media)
}

/// The slot `media` of a child whose stem is `stem`, in the art folder
/// `dir` (`<dir of child>/.media`), with the same limits as
/// [`child_slot_path`]. For a child that is not on disk yet (a planned
/// move).
pub fn slot_in(dir: &Path, stem: &str, media: ChildMedia) -> Result<PathBuf, SlotError> {
    if stem.is_empty() {
        return Err(SlotError::NotAChild);
    }
    let name = format!("{stem}{}", media.file_suffix());
    if name.len() > MAX_NAME_BYTES {
        return Err(SlotError::PathTooLong);
    }
    let path = dir.join(name);
    if path.to_string_lossy().encode_utf16().count() > WINDOWS_MAX_PATH {
        return Err(SlotError::PathTooLong);
    }
    Ok(path)
}

/// `<dir of child>/.media/<stem>.art.json`. A child with no parent or
/// name gets a path that never exists (an empty one).
pub fn art_json_path(child: &Path) -> PathBuf {
    match (media_dir(child), child_stem(child)) {
        (Some(dir), Some(stem)) => dir.join(format!("{stem}{ART_JSON_SUFFIX}")),
        _ => PathBuf::new(),
    }
}

/// The origins of a child's art. A missing, oversized or invalid file
/// reads as empty (no origin known).
pub fn read_art_json(child: &Path) -> ArtJson {
    let path = art_json_path(child);
    if path.as_os_str().is_empty() {
        return ArtJson::default();
    }
    let Ok(file) = fs::File::open(&path) else {
        return ArtJson::default();
    };
    let mut bytes = Vec::new();
    if file
        .take(MAX_ART_JSON_BYTES + 1)
        .read_to_end(&mut bytes)
        .is_err()
        || bytes.len() as u64 > MAX_ART_JSON_BYTES
    {
        return ArtJson::default();
    }
    serde_json::from_slice(&bytes).unwrap_or_default()
}

/// Which slots of `child` have a file, with each present slot's origin.
/// A slot whose path is too long is absent.
pub fn present(child: &Path) -> ChildArtMarks {
    let has = |media| child_slot_path(child, media).is_ok_and(|p| p.is_file());
    let icon = has(ChildMedia::Icon);
    let banner = has(ChildMedia::Banner);
    if !icon && !banner {
        return ChildArtMarks::default();
    }
    let origins = read_art_json(child);
    let origin = |media, on: bool| on.then(|| origins.get(media).map(|s| s.origin)).flatten();
    ChildArtMarks {
        icon,
        banner,
        icon_origin: origin(ChildMedia::Icon, icon),
        banner_origin: origin(ChildMedia::Banner, banner),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Component;

    use super::*;

    fn at(root: &Path, rel: &str) -> PathBuf {
        rel.split('/').fold(root.to_path_buf(), |p, c| p.join(c))
    }

    fn names(path: &Path) -> Vec<String> {
        path.components()
            .filter_map(|c| match c {
                Component::Normal(n) => Some(n.to_string_lossy().into_owned()),
                _ => None,
            })
            .collect()
    }

    fn units(path: &Path) -> usize {
        path.to_string_lossy().encode_utf16().count()
    }

    #[test]
    fn slots_sit_in_media_beside_the_child() {
        let root = Path::new("lib");
        let dlc = at(root, "ROMS/PS3/Game/DLC/Pack.pkg");
        assert_eq!(
            names(&child_slot_path(&dlc, ChildMedia::Icon).unwrap()),
            [
                "lib",
                "ROMS",
                "PS3",
                "Game",
                "DLC",
                ".media",
                "Pack-icon.png"
            ]
        );
        assert_eq!(
            names(&child_slot_path(&dlc, ChildMedia::Banner).unwrap()),
            [
                "lib",
                "ROMS",
                "PS3",
                "Game",
                "DLC",
                ".media",
                "Pack-banner.png"
            ]
        );
        let update = at(root, "ROMS/PS3/Game/Updates/Game (v1.2).pkg");
        assert_eq!(
            names(&child_slot_path(&update, ChildMedia::Icon).unwrap()),
            [
                "lib",
                "ROMS",
                "PS3",
                "Game",
                "Updates",
                ".media",
                "Game (v1.2)-icon.png"
            ]
        );
        assert_eq!(
            names(&art_json_path(&update)),
            [
                "lib",
                "ROMS",
                "PS3",
                "Game",
                "Updates",
                ".media",
                "Game (v1.2).art.json"
            ]
        );
    }

    #[test]
    fn two_children_with_one_stem_share_their_slots() {
        let dir = at(Path::new("lib"), "Game/DLC");
        for media in ChildMedia::ALL {
            assert_eq!(
                child_slot_path(&dir.join("x.pkg"), media),
                child_slot_path(&dir.join("x.nsp"), media)
            );
        }
        assert_eq!(
            art_json_path(&dir.join("x.pkg")),
            art_json_path(&dir.join("x.nsp"))
        );
    }

    #[test]
    fn a_folder_child_is_named_by_its_whole_name() {
        let lib = tempfile::TempDir::new().unwrap();
        let folder = at(lib.path(), "Game/Updates/Game v1.05");
        fs::create_dir_all(&folder).unwrap();
        let icon = child_slot_path(&folder, ChildMedia::Icon).unwrap();
        assert_eq!(icon.file_name().unwrap(), "Game v1.05-icon.png");
    }

    #[test]
    fn a_path_over_259_utf16_units_is_too_long_on_every_os() {
        // `é` is two UTF-8 bytes and one UTF-16 unit: a byte count would
        // refuse the 259-unit path too.
        let stem = "é".repeat(60);
        let probe = at(Path::new("r"), "Game/DLC")
            .join(CHILD_MEDIA_DIR)
            .join(format!("{stem}-icon.png"));
        let pad = 259 - units(&probe) - 1;
        let pad = "p".repeat(pad);
        let fits = at(Path::new("r"), &pad)
            .join("Game")
            .join("DLC")
            .join(format!("{stem}.pkg"));
        let slot = child_slot_path(&fits, ChildMedia::Icon).unwrap();
        assert_eq!(units(&slot), 259);
        assert!(slot.to_string_lossy().len() > 259);

        let over = at(Path::new("r"), &format!("{pad}p"))
            .join("Game")
            .join("DLC")
            .join(format!("{stem}.pkg"));
        assert_eq!(
            child_slot_path(&over, ChildMedia::Icon),
            Err(SlotError::PathTooLong)
        );
    }

    #[test]
    fn a_slot_name_over_255_bytes_is_too_long() {
        // 117 × `é` + 11 × `a` = 245 bytes: `-icon.png` makes 254 bytes,
        // `-banner.png` 256.
        let stem = format!("{}{}", "é".repeat(117), "a".repeat(11));
        let child = at(Path::new("r"), "DLC").join(format!("{stem}.cia"));
        let icon = child_slot_path(&child, ChildMedia::Icon).unwrap();
        assert_eq!(icon.file_name().unwrap().to_string_lossy().len(), 254);
        assert_eq!(
            child_slot_path(&child, ChildMedia::Banner),
            Err(SlotError::PathTooLong)
        );
    }

    #[test]
    fn a_path_with_no_parent_or_name_has_no_slot() {
        assert_eq!(
            child_slot_path(Path::new("x.pkg"), ChildMedia::Icon),
            Err(SlotError::NotAChild)
        );
        assert_eq!(
            slot_in(Path::new("d"), "", ChildMedia::Icon),
            Err(SlotError::NotAChild)
        );
        assert_eq!(art_json_path(Path::new("x.pkg")), PathBuf::new());
        assert_eq!(read_art_json(Path::new("x.pkg")), ArtJson::default());
    }

    /// A name that is not UTF-8 would be written with U+FFFD in its slot
    /// names: two such children would share one slot and the file would no
    /// longer map back to its child, so it gets no slot at all.
    #[cfg(unix)]
    #[test]
    fn a_child_whose_name_is_not_utf8_has_no_slot() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let dir = at(Path::new("lib"), "Game/DLC");
        let a = dir.join(OsStr::from_bytes(b"Pack \xff.pkg"));
        let b = dir.join(OsStr::from_bytes(b"Pack \xfe.pkg"));
        for child in [&a, &b] {
            assert_eq!(child_stem(child), None);
            for media in ChildMedia::ALL {
                assert_eq!(child_slot_path(child, media), Err(SlotError::NotAChild));
            }
            assert_eq!(art_json_path(child), PathBuf::new());
            assert_eq!(present(child), ChildArtMarks::default());
        }
    }

    #[test]
    fn art_json_round_trips_and_keeps_its_wire_shape() {
        let lib = tempfile::TempDir::new().unwrap();
        let child = at(lib.path(), "Game/DLC/Pack.cia");
        let art = ArtJson {
            icon: Some(OriginStamp {
                origin: ChildOrigin::Package,
                at: 1_700_000_000,
            }),
            banner: Some(OriginStamp {
                origin: ChildOrigin::Steamgriddb,
                at: 1_700_000_001,
            }),
        };
        let path = art_json_path(&child);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, art.to_bytes()).unwrap();
        assert_eq!(read_art_json(&child), art);

        let only_icon = ArtJson {
            icon: Some(OriginStamp {
                origin: ChildOrigin::Screenscraper,
                at: 5,
            }),
            banner: None,
        };
        let value: serde_json::Value = serde_json::from_slice(&only_icon.to_bytes()).unwrap();
        assert_eq!(
            value,
            serde_json::json!({ "icon": { "origin": "screenscraper", "at": 5 } })
        );
        let user: ChildOrigin = serde_json::from_str("\"user\"").unwrap();
        assert_eq!(user, ChildOrigin::User);
        assert_eq!(
            serde_json::to_string(&ChildMedia::Banner).unwrap(),
            "\"banner\""
        );
    }

    #[test]
    fn missing_or_invalid_art_json_reads_as_empty() {
        let lib = tempfile::TempDir::new().unwrap();
        let child = at(lib.path(), "Game/DLC/Pack.cia");
        assert_eq!(read_art_json(&child), ArtJson::default());
        let path = art_json_path(&child);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"{ not json").unwrap();
        assert_eq!(read_art_json(&child), ArtJson::default());
        fs::write(&path, br#"{"icon":{"origin":"somewhere","at":1}}"#).unwrap();
        assert_eq!(read_art_json(&child), ArtJson::default());
        let mut huge = br#"{"icon":{"origin":"user","at":1}}"#.to_vec();
        huge.resize(MAX_ART_JSON_BYTES as usize + 1, b' ');
        fs::write(&path, huge).unwrap();
        assert_eq!(read_art_json(&child), ArtJson::default());
    }

    #[test]
    fn present_marks_files_with_their_origin() {
        let lib = tempfile::TempDir::new().unwrap();
        let child = at(lib.path(), "Game/DLC/Pack.cia");
        assert_eq!(present(&child), ChildArtMarks::default());

        let icon = child_slot_path(&child, ChildMedia::Icon).unwrap();
        fs::create_dir_all(icon.parent().unwrap()).unwrap();
        fs::write(&icon, b"png").unwrap();
        // An icon with no origin file: present, origin unknown.
        assert_eq!(
            present(&child),
            ChildArtMarks {
                icon: true,
                ..ChildArtMarks::default()
            }
        );
        // The banner's stamp is ignored while its file is missing.
        let art = ArtJson {
            icon: Some(OriginStamp {
                origin: ChildOrigin::Package,
                at: 1,
            }),
            banner: Some(OriginStamp {
                origin: ChildOrigin::User,
                at: 2,
            }),
        };
        fs::write(art_json_path(&child), art.to_bytes()).unwrap();
        assert_eq!(
            present(&child),
            ChildArtMarks {
                icon: true,
                banner: false,
                icon_origin: Some(ChildOrigin::Package),
                banner_origin: None,
            }
        );
    }
}
