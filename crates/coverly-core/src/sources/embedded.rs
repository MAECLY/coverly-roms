//! Embedded covers (systems batch 2, spec D17, §8.7): the plaintext icon
//! some games carry (PS3 `ICON0.PNG`, PS Vita `sce_sys/icon0.png`, Wii U
//! `meta/iconTex.tga`, the Xbox 360 STFS thumbnail, the 3DS SMDH large
//! icon) as a last-resort cover; for the current generation (systems
//! batch 3 D21: PS4/PS5 `sce_sys/icon0.png`, the PS4 PKG entry `0x1200`,
//! the GDK and ERA manifest logos) as the first cover tried, where the
//! system has `embedded_first`.
//!
//! Nothing is remembered between the candidate and the fetch: both re-run
//! the budgeted console readers (`consoles`, headers only, D24) on the
//! ROM's path to get the icon's locator, and the fetch reads only a
//! locator the reader gave again, at most [`MAX_ART_BYTES`]. Icons are
//! decoded under `image::Limits` (at most [`MAX_SIDE`] px a side and
//! [`MAX_ALLOC`] bytes) and always answered as a freshly encoded PNG,
//! which the fill pipeline then re-encodes for the profile: raw embedded
//! bytes never reach a card. Only `Media::Cover`, no key, no network.

use std::fs::{self, File};
use std::io::{Cursor, Read};
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use super::{ArtQuery, ArtSource, Candidate, MatchKind, SourceError};
use crate::consoles::{self, ArtFormat, ArtLocator, Budgeted, DirListing};
use crate::identify::sniff::FileAt;
use crate::organize::walk::{list_dir, MetaProbe, OsMetaProbe};
use crate::profiles::Media;
use crate::systems::System;

/// Source id, in settings, candidates and journals.
pub const ID: &str = "embedded";
/// Most bytes read for one icon.
pub const MAX_ART_BYTES: u64 = 2 << 20;
/// Largest icon side decoded, in pixels.
pub const MAX_SIDE: u32 = 1024;
/// Most bytes the decoder may allocate for one icon.
pub const MAX_ALLOC: u64 = 16 << 20;
/// The systems whose readers can give an icon locator; any other ROM is
/// never opened by this source.
pub const SYSTEMS: [&str; 9] = [
    "3ds",
    "ps3",
    "ps4",
    "ps5",
    "psvita",
    "wiiu",
    "xbox360",
    "xboxone",
    "xboxseries",
];
/// Most names listed at the root of a folder game.
const MAX_ROOT_NAMES: usize = 4096;
/// The scheme of a candidate URL (never fetched over the network).
const SCHEME: &str = "embedded:";

/// What a candidate URL carries: the ROM, the icon's locator and a stamp
/// of the ROM (size and mtime; for a file inside a folder game, also the
/// icon file's) so a changed icon is not served from the art store under
/// an old URL.
#[derive(Debug, Serialize, Deserialize)]
struct Key {
    path: PathBuf,
    art: ArtLocator,
    stamp: String,
}

/// The art source over icons inside games.
#[derive(Clone, Copy, Debug, Default)]
pub struct EmbeddedSource;

impl ArtSource for EmbeddedSource {
    fn id(&self) -> &str {
        ID
    }

    /// One candidate per icon the reader finds that decodes within the
    /// limits; none for other media, without a path, or for systems
    /// without embedded art.
    fn candidates(&self, q: &ArtQuery) -> Result<Vec<Candidate>, SourceError> {
        let Some(path) = q.rom_path else {
            return Ok(Vec::new());
        };
        if q.media != Media::Cover || !SYSTEMS.contains(&q.system.id.as_str()) {
            return Ok(Vec::new());
        }
        let Some(rom_stamp) = stamp(path) else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        for art in locators(path) {
            if load(path, &art).is_err() {
                continue;
            }
            let origin = describe(&art);
            // A folder's own metadata does not change when a file deep
            // inside it is replaced: stamp the icon file too.
            let stamp = match &art {
                ArtLocator::File { rel, .. } => match stamp(&path.join(rel)) {
                    Some(inner) => format!("{rom_stamp}/{inner}"),
                    None => continue,
                },
                _ => rom_stamp.clone(),
            };
            let key = Key {
                path: path.to_path_buf(),
                art,
                stamp,
            };
            let Ok(json) = serde_json::to_string(&key) else {
                continue; // a path that is not UTF-8
            };
            out.push(Candidate {
                source: ID.to_string(),
                media: Media::Cover,
                name: q.stem.to_string(),
                url: format!("{SCHEME}{json}"),
                fallback_url: None,
                regions: Vec::new(),
                kind: MatchKind::Canonical,
                score: 100,
                votes: None,
                consensus: 0,
                origin,
            });
        }
        Ok(out)
    }

    fn has_name(&self, _: &System, _: Media, _: &str) -> Result<bool, SourceError> {
        Ok(false)
    }

    /// The icon of `c` as a PNG. The locator must be one the reader gives
    /// for the ROM now; anything else is `NotFound`.
    fn fetch(&self, c: &Candidate) -> Result<Vec<u8>, SourceError> {
        let missing = || SourceError::NotFound(c.url.clone());
        let key: Key = c
            .url
            .strip_prefix(SCHEME)
            .and_then(|json| serde_json::from_str(json).ok())
            .ok_or_else(missing)?;
        if !locators(&key.path).contains(&key.art) {
            return Err(missing());
        }
        let img = load(&key.path, &key.art).map_err(|()| missing())?;
        let mut out = Cursor::new(Vec::new());
        img.write_to(&mut out, ::image::ImageFormat::Png)
            .map_err(|_| missing())?;
        Ok(out.into_inner())
    }
}

/// `<size>-<mtime seconds>` of `path` (a folder's own metadata).
fn stamp(path: &Path) -> Option<String> {
    let meta = fs::metadata(path).ok()?;
    let secs = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    Some(format!("{}-{secs}", meta.len()))
}

/// The icon locators the console readers give for the ROM at `path`: a
/// folder game through the folder probes, a VPK through its zip listing,
/// any other file through the budgeted header readers. A cloud
/// placeholder is never read.
fn locators(path: &Path) -> Vec<ArtLocator> {
    let Ok(meta) = fs::metadata(path) else {
        return Vec::new();
    };
    if meta.is_dir() {
        return dir_art(path);
    }
    if !meta.is_file() || OsMetaProbe.cloud_only(path, &meta) {
        return Vec::new();
    }
    let ext = crate::scan::classify::extension(path);
    if ext == "vpk" {
        let limits = crate::archive::Limits::default();
        return crate::organize::inspect::vpk_console(path, &limits)
            .map(|id| id.art)
            .unwrap_or_default();
    }
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let Ok(file) = consoles::open_rom(path) else {
        return Vec::new();
    };
    let mut at = FileAt(file);
    let mut src = Budgeted::new(&mut at, meta.len());
    consoles::identify(&mut src, &ext, &name)
        .map(|id| id.art)
        .unwrap_or_default()
}

/// The icon locators of a folder game (the walk's own probes, spec D12).
fn dir_art(dir: &Path) -> Vec<ArtLocator> {
    let Some(listed) = list_dir(dir) else {
        return Vec::new();
    };
    if listed.len() > MAX_ROOT_NAMES {
        return Vec::new();
    }
    let names: Vec<String> = listed.into_iter().map(|(n, _, _)| n).collect();
    match consoles::identify_dir(&DirListing { dir, names: &names }) {
        Ok(Some(found)) => found.id.art,
        _ => Vec::new(),
    }
}

/// A short, fixed description of where the icon is, for the candidate.
fn describe(art: &ArtLocator) -> Option<String> {
    let text = match art {
        ArtLocator::Range {
            format: ArtFormat::Smdh,
            ..
        } => "SMDH icon".to_string(),
        ArtLocator::Range { .. } => "icon in the file header".to_string(),
        ArtLocator::ZipEntry { name, .. } => name.clone(),
        ArtLocator::File { rel, .. } => rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/"),
    };
    super::clean_text(&text, 80)
}

/// The icon at `art` inside the ROM at `path`, decoded within the limits.
fn load(path: &Path, art: &ArtLocator) -> Result<::image::DynamicImage, ()> {
    let (bytes, format) = match art {
        ArtLocator::Range {
            offset,
            len,
            format,
        } => (read_range(path, *offset, *len)?, *format),
        ArtLocator::ZipEntry { name, format } => (
            crate::organize::inspect::vpk_entry(path, name, MAX_ART_BYTES).ok_or(())?,
            *format,
        ),
        ArtLocator::File { rel, format } => (read_inside(path, rel)?, *format),
    };
    decode(&bytes, format)
}

/// `len` bytes at `offset` of the file, through a budgeted reader.
fn read_range(path: &Path, offset: u64, len: u64) -> Result<Vec<u8>, ()> {
    if len == 0 || len > MAX_ART_BYTES {
        return Err(());
    }
    let meta = fs::metadata(path).map_err(drop)?;
    if !meta.is_file() || OsMetaProbe.cloud_only(path, &meta) {
        return Err(());
    }
    let mut at = FileAt(consoles::open_rom(path).map_err(drop)?);
    let mut src = Budgeted::new(&mut at, meta.len());
    let n = usize::try_from(len).map_err(drop)?;
    src.read(offset, n).map_err(drop)
}

/// The whole file `rel` inside the folder game `dir`: a plain relative
/// path, a regular file (never a link), local, at most `MAX_ART_BYTES`.
fn read_inside(dir: &Path, rel: &Path) -> Result<Vec<u8>, ()> {
    if rel.as_os_str().is_empty() || !rel.components().all(|c| matches!(c, Component::Normal(_))) {
        return Err(());
    }
    let path = dir.join(rel);
    let meta = fs::symlink_metadata(&path).map_err(drop)?;
    if !meta.is_file() || meta.len() > MAX_ART_BYTES || OsMetaProbe.cloud_only(&path, &meta) {
        return Err(());
    }
    let file: File = consoles::open_rom(&path).map_err(drop)?;
    let mut bytes = Vec::new();
    file.take(MAX_ART_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(drop)?;
    if bytes.len() as u64 > MAX_ART_BYTES {
        return Err(());
    }
    Ok(bytes)
}

/// Decodes an icon: SMDH through the 3DS tile decoder (always 48×48),
/// PNG and TGA through `image` under [`MAX_SIDE`] and [`MAX_ALLOC`].
fn decode(bytes: &[u8], format: ArtFormat) -> Result<::image::DynamicImage, ()> {
    let format = match format {
        ArtFormat::Smdh => {
            return consoles::ctr::smdh_icon_png(bytes)
                .map(::image::DynamicImage::ImageRgba8)
                .ok_or(());
        }
        ArtFormat::Png => ::image::ImageFormat::Png,
        ArtFormat::Tga => ::image::ImageFormat::Tga,
    };
    let mut limits = ::image::Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(MAX_ALLOC);
    let mut reader = ::image::ImageReader::with_format(Cursor::new(bytes), format);
    reader.limits(limits);
    reader.decode().map_err(drop)
}
