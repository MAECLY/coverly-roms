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
//! Writes go only through the journaled writer (`write::ensure_within`,
//! backup, undo). [`read_child_art`] (D11) fills the slots with the art a
//! child carries **in the clear** (3DS CIA meta SMDH, PS3 PKG `ICON0.PNG`,
//! Vita/PS4/PS5 `sce_sys/icon0.png`, Wii U `meta/iconTex.tga`, Xbox 360
//! STFS thumbnail; banners only from the plaintext files of a folder
//! child). A key, ticket or licence file is refused by name before
//! anything is opened, Switch children give nothing (their NCAs are
//! encrypted), and an existing slot file is never replaced.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::organize::target::{MAX_NAME_BYTES, WINDOWS_MAX_PATH};
use crate::sources::embedded;
use crate::write::{OverwritePolicy, WriteError, WriteOutcome, Writer};

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

/// The origin file of a child whose stem is `stem`, in the art folder
/// `dir`, with the same limits as [`slot_in`]. For a child that is not on
/// disk yet (a planned move).
pub fn art_json_in(dir: &Path, stem: &str) -> Result<PathBuf, SlotError> {
    if stem.is_empty() {
        return Err(SlotError::NotAChild);
    }
    let name = format!("{stem}{ART_JSON_SUFFIX}");
    if name.len() > MAX_NAME_BYTES {
        return Err(SlotError::PathTooLong);
    }
    let path = dir.join(name);
    if path.to_string_lossy().encode_utf16().count() > WINDOWS_MAX_PATH {
        return Err(SlotError::PathTooLong);
    }
    Ok(path)
}

/// The art files of `child` on disk (its filled slots, then its origin
/// file), each with the slot it fills (`None`: the origin file). Nothing
/// is opened: only the names beside the child are looked up.
pub fn files_of(child: &Path) -> Vec<(Option<ChildMedia>, PathBuf)> {
    let is_file = |p: &Path| fs::symlink_metadata(p).is_ok_and(|m| m.file_type().is_file());
    let mut out: Vec<(Option<ChildMedia>, PathBuf)> = ChildMedia::ALL
        .into_iter()
        .filter_map(|media| {
            child_slot_path(child, media)
                .ok()
                .filter(|p| is_file(p))
                .map(|p| (Some(media), p))
        })
        .collect();
    let json = art_json_path(child);
    if !json.as_os_str().is_empty() && is_file(&json) {
        out.push((None, json));
    }
    out
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

/// Why [`read_child_art`] wrote nothing to a slot of a child.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SkipWhy {
    /// A key, ticket or licence file (`consoles::is_key_material`): never
    /// opened.
    KeyMaterial,
    /// The child carries no art in the clear (a Switch NSP, an encrypted
    /// package, an unknown file).
    NoArtInClear,
    /// The slot already has a file (or a case-insensitive twin): kept.
    Exists,
    /// The slot path is too long for Windows or FAT ([`SlotError`]).
    PathTooLong,
    /// The library cannot be written (read-only volume, no permission).
    ReadOnly,
    /// The slot would land outside the library, or its name is not valid
    /// on FAT/exFAT.
    Refused,
}

/// What [`read_child_art`] did.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ChildArtReport {
    /// Slot files written, in the order of the children and slots.
    pub written: Vec<PathBuf>,
    /// Children (or one slot of a child) left without new art, and why.
    pub skipped: Vec<(PathBuf, SkipWhy)>,
}

/// The icon an update, DLC or demo at `child` carries in the clear, as a
/// fresh PNG, for a preview (part C D15: Organizar's child rows, read from
/// the pile's source file). Nothing is written; a key file is refused by
/// name before anything is opened, and a package without art in the clear
/// (a Switch NCA) gives `None`.
pub fn icon_preview(child: &Path) -> Option<Vec<u8>> {
    let name = child.file_name()?.to_string_lossy();
    if crate::consoles::is_key_material(&name) {
        return None;
    }
    embedded::child_locators(child)
        .iter()
        .find_map(|art| embedded::child_png(child, art).ok())
}

/// Reads the art each of `children` carries in the clear into its empty
/// slots, through `writer` (journaled: undoing the job removes the files,
/// restores the `.art.json` it replaced and drops a `.media/` folder it
/// created once empty). Each written slot is stamped
/// [`ChildOrigin::Package`] in `<stem>.art.json`.
///
/// Per child, at most one skip reason is reported: a key file, no art in
/// the clear, every slot it has art for already filled, a path too long,
/// or a refused or read-only write. The error is only for a failure that
/// stops the job (disk full, a damaged journal, an I/O error).
pub fn read_child_art(
    children: &[&Path],
    writer: &mut Writer,
) -> Result<ChildArtReport, WriteError> {
    let mut report = ChildArtReport::default();
    for child in children {
        if let Some(why) = read_one(child, writer, &mut report.written)? {
            report.skipped.push((child.to_path_buf(), why));
        }
    }
    Ok(report)
}

/// Fills the empty slots of `child`; the skip reason when it wrote none.
fn read_one(
    child: &Path,
    writer: &mut Writer,
    written: &mut Vec<PathBuf>,
) -> Result<Option<SkipWhy>, WriteError> {
    // Keys first, by name: nothing below opens a key file.
    let name = child
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or_default();
    if crate::consoles::is_key_material(&name) {
        return Ok(Some(SkipWhy::KeyMaterial));
    }
    let mut slots = Vec::with_capacity(ChildMedia::ALL.len());
    for media in ChildMedia::ALL {
        match child_slot_path(child, media) {
            Ok(path) => slots.push((media, path)),
            Err(SlotError::PathTooLong) => return Ok(Some(SkipWhy::PathTooLong)),
            Err(SlotError::NotAChild) => return Ok(Some(SkipWhy::NoArtInClear)),
        }
    }
    // Never read a child whose slots could not be written anyway.
    let json_path = art_json_path(child);
    for path in slots.iter().map(|(_, p)| p).chain([&json_path]) {
        if writer.ensure_within_any(path).is_err() {
            return Ok(Some(SkipWhy::Refused));
        }
    }

    // Only the empty slots are read for: a filled one (or its case
    // twin) is kept, so the package is not even opened for it.
    let mut empty = Vec::with_capacity(slots.len());
    for (media, path) in slots {
        match slot_taken(&path) {
            Ok(true) => {}
            Ok(false) => empty.push((media, path)),
            Err(e) => return soft(e).map(Some),
        }
    }
    if empty.is_empty() {
        return Ok(Some(SkipWhy::Exists));
    }

    let mut found = Vec::new();
    for (media, path) in &empty {
        let png = match media {
            ChildMedia::Icon => embedded::child_locators(child)
                .iter()
                .find_map(|art| embedded::child_png(child, art).ok()),
            ChildMedia::Banner => embedded::child_banner_locators(child)
                .iter()
                .find_map(|art| embedded::child_banner_png(child, art).ok()),
        };
        if let Some(png) = png {
            found.push((*media, path.clone(), png));
        }
    }
    if found.is_empty() {
        // A filled slot means the child's art is already there: that is
        // the reason, not a package without art.
        let filled = empty.len() < ChildMedia::ALL.len();
        return Ok(Some(if filled {
            SkipWhy::Exists
        } else {
            SkipWhy::NoArtInClear
        }));
    }

    let mut stamped = Vec::new();
    let mut skip = None;
    for (media, path, png) in found {
        if let Some(dir) = path.parent() {
            if let Err(e) = writer.ensure_dir(dir) {
                return soft(e).map(Some);
            }
        }
        match writer.write(&path, &png, OverwritePolicy::Never) {
            Ok(WriteOutcome::Created { path } | WriteOutcome::Replaced { path, .. }) => {
                written.push(path);
                stamped.push(media);
            }
            Ok(WriteOutcome::SkippedExists { .. }) => skip = Some(SkipWhy::Exists),
            Err(e) => skip = Some(soft(e)?),
        }
    }
    if stamped.is_empty() {
        return Ok(skip);
    }
    let mut art = read_art_json(child);
    let at = now_unix();
    for media in stamped {
        art.set(
            media,
            Some(OriginStamp {
                origin: ChildOrigin::Package,
                at,
            }),
        );
    }
    // The origin file is ours: an older one is backed up and restored by
    // undo.
    if let Err(e) = writer.write(&json_path, &art.to_bytes(), OverwritePolicy::Backup) {
        soft(e)?;
    }
    Ok(None)
}

/// Writes `png` (an image the user picked for this child, already
/// re-encoded as PNG) into the `media` slot of `child`, stamped with
/// `origin` in `<stem>.art.json`, through `writer` (D11: «Buscar arte»
/// writes only on an explicit pick). An existing slot file, or its
/// case-insensitive twin, is replaced with the usual backup, so undoing
/// the job gives it back; the child itself is never opened.
///
/// `None` when the slot was written; else why nothing was: a key or
/// licence file (refused by name), a slot path too long, or a refused or
/// read-only write. The error is only for a failure that stops the job.
pub fn write_child_slot(
    child: &Path,
    media: ChildMedia,
    png: &[u8],
    origin: ChildOrigin,
    writer: &mut Writer,
) -> Result<Option<SkipWhy>, WriteError> {
    let name = child
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or_default();
    if crate::consoles::is_key_material(&name) {
        return Ok(Some(SkipWhy::KeyMaterial));
    }
    let path = match child_slot_path(child, media) {
        Ok(path) => path,
        Err(SlotError::PathTooLong) => return Ok(Some(SkipWhy::PathTooLong)),
        Err(SlotError::NotAChild) => return Ok(Some(SkipWhy::Refused)),
    };
    let json_path = art_json_path(child);
    for p in [&path, &json_path] {
        if writer.ensure_within_any(p).is_err() {
            return Ok(Some(SkipWhy::Refused));
        }
    }
    if let Some(dir) = path.parent() {
        if let Err(e) = writer.ensure_dir(dir) {
            return soft(e).map(Some);
        }
    }
    match writer.write(&path, png, OverwritePolicy::Backup) {
        Ok(WriteOutcome::Created { .. } | WriteOutcome::Replaced { .. }) => {}
        Ok(WriteOutcome::SkippedExists { .. }) => return Ok(Some(SkipWhy::Exists)),
        Err(e) => return soft(e).map(Some),
    }
    let mut art = read_art_json(child);
    art.set(
        media,
        Some(OriginStamp {
            origin,
            at: now_unix(),
        }),
    );
    if let Err(e) = writer.write(&json_path, &art.to_bytes(), OverwritePolicy::Backup) {
        soft(e)?;
    }
    Ok(None)
}

/// Whether the slot `path` (or a case-insensitive twin of it) already
/// holds an entry. A missing `.media/` folder means an empty slot.
fn slot_taken(path: &Path) -> Result<bool, WriteError> {
    let (Some(dir), Some(name)) = (path.parent(), path.file_name().and_then(|n| n.to_str())) else {
        return Ok(false);
    };
    if !dir.is_dir() {
        return Ok(false);
    }
    Ok(crate::write::find_twin(dir, name)?.is_some())
}

/// A write error that only skips the child; anything else stops the job.
fn soft(e: WriteError) -> Result<SkipWhy, WriteError> {
    match e {
        WriteError::Denied(_) => Ok(SkipWhy::ReadOnly),
        WriteError::OutsideRoot(_) | WriteError::NameInvalid(_) | WriteError::NotAFile(_) => {
            Ok(SkipWhy::Refused)
        }
        other => Err(other),
    }
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
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
