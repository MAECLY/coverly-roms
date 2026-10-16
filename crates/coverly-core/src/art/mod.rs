//! Art status: which media each ROM has, and art no ROM uses.
//!
//! [`resolve_art`] renders every slot of the profile for every scanned ROM
//! and looks the expected file up in a per-directory index built once
//! (one `read_dir` per directory, never an `exists()` per ROM and
//! extension). Folder names, file stems and extensions all match
//! case-insensitively, and an image with another extension (`.jpg` where
//! the profile writes `.png`) counts as present. Read-only.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::profiles::{render, ImageFormat, Media, MediaSlot, Profile, TemplateVars};
use crate::scan::{LibraryScan, RomEntry, SystemScan};
use crate::systems::{System, SystemDb};

/// Image extensions accepted as art, in preference order after the slot's
/// own format.
const IMAGE_EXTS: [&str; 4] = ["png", "jpg", "jpeg", "webp"];

/// Batocera cover postfix and the alternative it also accepts (§16.8).
const COVER_POSTFIX: &str = "-image";
const COVER_POSTFIX_ALT: &str = "-thumb";

/// State of one media for one ROM.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum ArtState {
    /// An image exists, at `path` (spelled as on disk).
    Present {
        /// The art file.
        path: PathBuf,
    },
    /// The profile can store this media but no image exists.
    Missing,
    /// The profile cannot store this media for this ROM.
    Unsupported,
}

/// Art status of one ROM.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RomArt {
    /// The ROM.
    pub rom: RomEntry,
    /// State of each requested media.
    pub art: BTreeMap<Media, ArtState>,
}

/// Art status of one system folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SystemArt {
    /// The system folder as scanned, with `roms` left empty (they are in
    /// [`SystemArt::roms`]).
    pub scan: SystemScan,
    /// Every ROM of the folder with its art states, in scan order.
    pub roms: Vec<RomArt>,
    /// Images in this system's art folders that belong to no ROM, sorted.
    /// AppleDouble twins and hidden files are never orphans.
    pub orphans: Vec<PathBuf>,
}

/// Resolves the state of each of `media` for every ROM of `scan`, laid out
/// as `profile`, plus the orphan art of each system.
///
/// A media the profile has no slot for, or whose template needs a libretro
/// name the system lacks, is [`ArtState::Unsupported`]. Orphans are images
/// in the folders the profile's slots point to whose name matches no ROM of
/// the library for any slot; a folder shared by several system folders
/// reports its orphans under the first one.
pub fn resolve_art(
    scan: &LibraryScan,
    profile: &Profile,
    systems: &SystemDb,
    media: &[Media],
) -> Vec<SystemArt> {
    let slots: Vec<(Media, &MediaSlot)> = Media::ALL
        .iter()
        .filter_map(|&m| profile.slot(m).map(|slot| (m, slot)))
        .collect();
    let mut index = DirIndex::new(&scan.root);
    // (resolved art folder, lowercase stem) of every expected image.
    let mut claimed: HashSet<(PathBuf, String)> = HashSet::new();
    let mut result = Vec::with_capacity(scan.systems.len());
    let mut art_dirs: Vec<Vec<PathBuf>> = Vec::with_capacity(scan.systems.len());

    for system_scan in &scan.systems {
        let system = system_scan
            .system_id
            .as_deref()
            .and_then(|id| systems.get(id));
        let mut dirs: Vec<PathBuf> = Vec::new();
        let mut roms = Vec::with_capacity(system_scan.roms.len());

        if let Some(system) = system {
            // Folders where art of ROM-less (sub)folders would live too.
            for (_, slot) in &slots {
                let base = Path::new("");
                if let Some(expected) =
                    expected(scan, profile, system_scan, system, slot, base, "x")
                {
                    if let Some(dir) = index.resolve(&expected.dir) {
                        push_unique(&mut dirs, dir);
                    }
                }
            }
        }

        for rom in &system_scan.roms {
            let mut art = BTreeMap::new();
            for &m in media {
                art.insert(m, ArtState::Unsupported);
            }
            if let Some(system) = system {
                for &(m, slot) in &slots {
                    let Some(expected) = expected(
                        scan,
                        profile,
                        system_scan,
                        system,
                        slot,
                        &rom.rel_dir,
                        &rom.stem,
                    ) else {
                        continue;
                    };
                    let resolved = index.resolve(&expected.dir);
                    let state = match &resolved {
                        Some(dir) => {
                            for stem in &expected.stems {
                                claimed.insert((dir.clone(), stem.clone()));
                            }
                            push_unique(&mut dirs, dir.clone());
                            match index.find(dir, &expected.stems, slot.format) {
                                Some(path) => ArtState::Present { path },
                                None => ArtState::Missing,
                            }
                        }
                        None => ArtState::Missing,
                    };
                    if let Some(entry) = art.get_mut(&m) {
                        *entry = state;
                    }
                }
            }
            roms.push(RomArt {
                rom: rom.clone(),
                art,
            });
        }

        result.push(SystemArt {
            scan: SystemScan {
                roms: Vec::new(),
                ..system_scan.clone()
            },
            roms,
            orphans: Vec::new(),
        });
        art_dirs.push(dirs);
    }

    let mut owned: HashSet<PathBuf> = HashSet::new();
    for (system_art, dirs) in result.iter_mut().zip(art_dirs) {
        for dir in dirs {
            if !owned.insert(dir.clone()) {
                continue;
            }
            for image in index.images(&dir) {
                if !claimed.contains(&(dir.clone(), image.stem.clone())) {
                    system_art.orphans.push(image.path.clone());
                }
            }
        }
        system_art.orphans.sort();
    }
    result
}

/// Where a slot expects the art of one ROM.
struct Expected {
    /// Folder of the image, as rendered.
    dir: PathBuf,
    /// Accepted file stems, lowercase, in preference order.
    stems: Vec<String>,
}

/// Renders `slot` for a ROM at `rel_dir`/`stem`, or `None` when the
/// template needs a libretro name the system lacks.
fn expected(
    scan: &LibraryScan,
    profile: &Profile,
    system_scan: &SystemScan,
    system: &System,
    slot: &MediaSlot,
    rel_dir: &Path,
    stem: &str,
) -> Option<Expected> {
    if slot.template.contains("{libretro}") && system.libretro.is_none() {
        return None;
    }
    let rel_dir = if profile.mirror_subdirs {
        rel_dir
    } else {
        Path::new("")
    };
    let path = render(
        &slot.template,
        &TemplateVars {
            root: &scan.root,
            system_dir: &system_scan.dir,
            rel_dir,
            stem,
            system_id: &system.id,
            libretro: system.libretro.as_deref(),
        },
    );
    let dir = path.parent()?.to_path_buf();
    let file_stem = path.file_stem()?.to_string_lossy().to_lowercase();
    let mut stems = vec![file_stem.clone()];
    if let Some(base) = file_stem.strip_suffix(COVER_POSTFIX) {
        stems.push(format!("{base}{COVER_POSTFIX_ALT}"));
    }
    Some(Expected { dir, stems })
}

fn push_unique(dirs: &mut Vec<PathBuf>, dir: PathBuf) {
    if !dirs.contains(&dir) {
        dirs.push(dir);
    }
}

/// An image file of an indexed folder.
struct Image {
    /// File stem, lowercase.
    stem: String,
    /// Extension, lowercase.
    ext: String,
    /// The file, spelled as on disk.
    path: PathBuf,
}

/// The entries of one folder, read once.
#[derive(Default)]
struct Listing {
    /// Sub-folder names as on disk.
    dirs: Vec<String>,
    /// Image files, sorted by name.
    images: Vec<Image>,
    /// Lowercase stem -> indexes into `images`.
    by_stem: HashMap<String, Vec<usize>>,
}

/// Lazily built index of folders below the library root, with
/// case-insensitive folder resolution.
struct DirIndex {
    root: PathBuf,
    /// Rendered folder -> folder as on disk (`None`: does not exist).
    resolved: HashMap<PathBuf, Option<PathBuf>>,
    /// Folder as on disk -> its listing.
    listings: HashMap<PathBuf, Listing>,
}

impl DirIndex {
    fn new(root: &Path) -> DirIndex {
        DirIndex {
            root: root.to_path_buf(),
            resolved: HashMap::new(),
            listings: HashMap::new(),
        }
    }

    /// The on-disk spelling of `dir`, matching each component below the
    /// library root case-insensitively (the exact spelling wins).
    fn resolve(&mut self, dir: &Path) -> Option<PathBuf> {
        if let Some(cached) = self.resolved.get(dir) {
            return cached.clone();
        }
        let found = if dir == self.root || !dir.starts_with(&self.root) {
            dir.is_dir().then(|| dir.to_path_buf())
        } else {
            match (dir.parent(), dir.file_name()) {
                (Some(parent), Some(name)) => {
                    let name = name.to_string_lossy().into_owned();
                    self.resolve(parent).and_then(|parent| {
                        let listing = self.listing(&parent);
                        listing
                            .dirs
                            .iter()
                            .find(|d| **d == name)
                            .or_else(|| listing.dirs.iter().find(|d| eq_ignore_case(d, &name)))
                            .map(|d| parent.join(d))
                    })
                }
                _ => dir.is_dir().then(|| dir.to_path_buf()),
            }
        };
        self.resolved.insert(dir.to_path_buf(), found.clone());
        found
    }

    /// The listing of an on-disk folder, read on first use. Unreadable
    /// folders list as empty (the scan already reports I/O problems).
    fn listing(&mut self, dir: &Path) -> &Listing {
        self.listings
            .entry(dir.to_path_buf())
            .or_insert_with(|| read_listing(dir))
    }

    /// The best image in `dir` whose stem is one of `stems` (preference
    /// order), preferring the slot's own format, then [`IMAGE_EXTS`] order.
    fn find(&mut self, dir: &Path, stems: &[String], format: ImageFormat) -> Option<PathBuf> {
        let own = match format {
            ImageFormat::Png => "png",
            ImageFormat::Jpg => "jpg",
        };
        let rank = |ext: &str| {
            if ext == own {
                0
            } else {
                1 + IMAGE_EXTS.iter().position(|e| *e == ext).unwrap_or(0)
            }
        };
        let listing = self.listing(dir);
        stems.iter().find_map(|stem| {
            listing.by_stem.get(stem).and_then(|indexes| {
                indexes
                    .iter()
                    .map(|&i| &listing.images[i])
                    .min_by_key(|image| rank(&image.ext))
                    .map(|image| image.path.clone())
            })
        })
    }

    /// Every image of an on-disk folder.
    fn images(&mut self, dir: &Path) -> &[Image] {
        &self.listing(dir).images
    }
}

fn read_listing(dir: &Path) -> Listing {
    let mut listing = Listing::default();
    let Ok(entries) = fs::read_dir(dir) else {
        return listing;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        // Follow symlinks: a linked folder or image counts like a real one.
        let Ok(meta) = fs::metadata(&path) else {
            continue;
        };
        if meta.is_dir() {
            listing.dirs.push(name);
            continue;
        }
        // AppleDouble twins (`._x.png`) and hidden files are never art.
        if !meta.is_file() || name.starts_with('.') {
            continue;
        }
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if !IMAGE_EXTS.contains(&ext.as_str()) {
            continue;
        }
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        listing.images.push(Image { stem, ext, path });
    }
    listing.dirs.sort();
    listing.images.sort_by(|a, b| a.path.cmp(&b.path));
    for (i, image) in listing.images.iter().enumerate() {
        listing
            .by_stem
            .entry(image.stem.clone())
            .or_default()
            .push(i);
    }
    listing
}

fn eq_ignore_case(a: &str, b: &str) -> bool {
    a == b || a.to_lowercase() == b.to_lowercase()
}
