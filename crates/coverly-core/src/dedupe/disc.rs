//! [`ReaderDiscProbe`]: the [`DiscProbe`] seam over the disc reader
//! (`crate::disc`), for level 3 (duplicates spec §5.4).
//!
//! - A raw image (`.bin`, `.iso`, `.img`…) is read sector by sector from
//!   disk, with its length.
//! - A `.cue` or `.gdi` set is read through the track its sheet names as
//!   primary (a GD-ROM's track 03, never the low-density track 01); a
//!   `.ccd` through its same-stem `.img`.
//! - Inside an archive, the primary track a sheet in the archive names
//!   (by name, case-insensitive, in the sheet's folder), else every entry in
//!   order, through a window of at most [`MAX_WINDOW`] decoded bytes. A
//!   `SYSTEM.CNF` listed in the window but stored past it is fetched with a
//!   skip on a fresh reader; every byte decoded for one probe is charged to
//!   one plan head budget.
//! - An `.m3u` is its discs in order: their serials in order, disc 1's
//!   volume fingerprint when every disc has one. A disc that cannot be read
//!   makes the whole playlist unknown.
//! - A CHD is not read (plan DD-2).
//!
//! The answer is given only for the system asked about. Read-only.

use std::fs::{self, File};
use std::io::{self, Read};
use std::ops::Range;
use std::path::{Component, Path, PathBuf};

use crate::archive::Budget;
use crate::disc::sheets::MAX_SHEET_BYTES;
use crate::disc::source::MAX_WINDOW;
use crate::disc::{
    parse_sheet, primary_track, DiscIdentity, DiscReader, DiscSystem, FileSource, SheetKind,
    WindowSource,
};

use super::archive::ArchiveEntryListing;
use super::source::{DiscId, DiscIdSource, DiscProbe, EntryListing, ListedEntry};

/// [`DiscProbe`] over the disc reader (module docs).
#[derive(Debug)]
pub struct ReaderDiscProbe {
    listing: ArchiveEntryListing,
}

impl ReaderDiscProbe {
    /// A probe that reads disc archives through `listing`.
    pub fn new(listing: ArchiveEntryListing) -> Self {
        ReaderDiscProbe { listing }
    }

    /// The identity of one disc at `path` (never an m3u).
    fn identify(&self, path: &Path) -> io::Result<Option<DiscIdentity>> {
        let ext = extension(path);
        match ext.as_str() {
            "chd" | "m3u" => return Ok(None),
            "cue" | "gdi" => {
                let kind = SheetKind::from_extension(&ext).unwrap_or(SheetKind::Cue);
                let text = read_capped(path, MAX_SHEET_BYTES)?;
                return match primary_file(kind, &text).and_then(|f| beside(path, &f)) {
                    Some(track) => identify_file(&track),
                    None => Ok(None),
                };
            }
            "ccd" => return identify_file(&path.with_extension("img")),
            _ => {}
        }
        match self.listing.list(path)? {
            Some(entries) => Ok(self.in_archive(path, &entries)),
            None => identify_file(path),
        }
    }

    /// A disc inside the archive at `path`, under one plan head budget.
    fn in_archive(&self, path: &Path, entries: &[ListedEntry]) -> Option<DiscIdentity> {
        let mut budget = Budget::plan_head(self.listing.limits());
        let primary = entries.iter().enumerate().find_map(|(n, e)| {
            let kind = sheet_kind(&e.name)?;
            let text = self
                .listing
                .head_with(path, n, MAX_SHEET_BYTES, &mut budget)
                .ok()??;
            let file = primary_file(kind, &text)?;
            let dir = e.name.rfind('/').map_or("", |i| &e.name[..=i]);
            let want = format!("{dir}{file}");
            entries
                .iter()
                .position(|t| t.name.eq_ignore_ascii_case(&want))
        });
        let candidates: Vec<usize> = match primary {
            Some(n) => vec![n],
            None => (0..entries.len())
                .filter(|&n| sheet_kind(&entries[n].name).is_none())
                .collect(),
        };
        candidates.into_iter().find_map(|n| {
            let head = self
                .listing
                .head_with(path, n, MAX_WINDOW, &mut budget)
                .ok()??;
            let mut src = WindowSource::new(head);
            let budget = &mut budget;
            let mut fetch = |range: Range<u64>| self.listing.read_range(path, n, range, budget);
            DiscReader::identify_with(&mut src, Some(entries[n].size), Some(&mut fetch))
                .ok()
                .flatten()
        })
    }

    /// The discs of the playlist at `path`, in order.
    fn playlist(&self, path: &Path, system: &str) -> io::Result<Option<DiscId>> {
        let text = read_capped(path, MAX_SHEET_BYTES)?;
        let Ok(sheet) = parse_sheet(SheetKind::M3u, &text) else {
            return Ok(None);
        };
        if sheet.files.is_empty() {
            return Ok(None);
        }
        let mut discs = Vec::with_capacity(sheet.files.len());
        for file in &sheet.files {
            let Some(disc) = beside(path, file) else {
                return Ok(None);
            };
            // A playlist never lists another: nothing recurses.
            if extension(&disc) == "m3u" {
                return Ok(None);
            }
            match self.identify(&disc) {
                Ok(Some(id)) if is_of(&id, system) && !id.serials.is_empty() => discs.push(id),
                _ => return Ok(None),
            }
        }
        let volume = if discs.iter().all(|d| d.volume.is_some()) {
            discs[0].volume.clone()
        } else {
            None
        };
        Ok(Some(DiscId {
            serials: discs.into_iter().flat_map(|d| d.serials).collect(),
            disc_no: None,
            volume,
            source: DiscIdSource::M3uPosition,
        }))
    }
}

impl DiscProbe for ReaderDiscProbe {
    fn probe(&self, path: &Path, system: &str) -> io::Result<Option<DiscId>> {
        if extension(path) == "m3u" {
            return self.playlist(path, system);
        }
        Ok(self
            .identify(path)?
            .filter(|d| is_of(d, system))
            .map(|d| d.to_dedupe_id()))
    }
}

fn is_of(d: &DiscIdentity, system: &str) -> bool {
    matches!(d.system, DiscSystem::Known(id) if id == system)
}

/// A raw image on disk, read sector by sector.
fn identify_file(path: &Path) -> io::Result<Option<DiscIdentity>> {
    let len = fs::metadata(path)?.len();
    let mut src = FileSource::open(path)?;
    DiscReader::identify_with(&mut src, Some(len), None)
}

fn extension(path: &Path) -> String {
    path.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// A `.cue` or `.gdi` entry name's sheet kind (never an m3u).
fn sheet_kind(name: &str) -> Option<SheetKind> {
    let ext = Path::new(name).extension()?.to_str()?;
    SheetKind::from_extension(ext).filter(|k| *k != SheetKind::M3u)
}

/// The file name of `sheet`'s primary track.
fn primary_file(kind: SheetKind, text: &[u8]) -> Option<String> {
    let sheet = parse_sheet(kind, text).ok()?;
    let track = &sheet.tracks[primary_track(&sheet)?];
    sheet.files.get(track.file).cloned()
}

/// `name`, as a sheet or playlist writes it, next to `list`: relative
/// names only, never one that climbs out of the list's folder.
fn beside(list: &Path, name: &str) -> Option<PathBuf> {
    let rel = PathBuf::from(name.replace('\\', "/"));
    let plain = rel
        .components()
        .all(|c| matches!(c, Component::Normal(_) | Component::CurDir));
    let dir = list.parent().unwrap_or(Path::new(""));
    (plain && !name.is_empty()).then(|| dir.join(rel))
}

/// At most `cap` bytes of `path`.
fn read_capped(path: &Path, cap: usize) -> io::Result<Vec<u8>> {
    let mut out = Vec::new();
    File::open(path)?.take(cap as u64).read_to_end(&mut out)?;
    Ok(out)
}
