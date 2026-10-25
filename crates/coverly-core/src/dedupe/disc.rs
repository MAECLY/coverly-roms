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
//! - A PS1 `.pbp` is read through its `PSISO` blocks and a `.cso` through
//!   its CISO blocks, at most 1 MiB inflated each; never the outer
//!   `PARAM.SFO` (plan DD-3). A multi-disc PBP is its discs in order, like
//!   an m3u.
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
    infer_redump_set, parse_sheet, primary_track, Cso, DiscIdentity, DiscReader, DiscSystem,
    FileSource, Pbp, SheetKind, WindowSource,
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
            // A multi-disc PBP is a set, never one disc of a playlist.
            "pbp" => {
                return Ok(pbp_discs(path)?.and_then(|mut d| {
                    let one = d.len() == 1;
                    one.then(|| d.pop()).flatten().flatten()
                }))
            }
            "cso" => return identify_cso(path),
            _ => {}
        }
        match self.listing.list(path)? {
            Some(entries) => Ok(self.in_archive(path, &entries)),
            None => identify_file(path),
        }
    }

    /// A disc inside the archive at `path`, under one plan head budget.
    /// No read is started that the budget cannot finish: in a solid
    /// archive whose sheet comes after every track, the primary track is
    /// inferred from the Redump names instead.
    fn in_archive(&self, path: &Path, entries: &[ListedEntry]) -> Option<DiscIdentity> {
        let mut budget = Budget::plan_head(self.listing.limits());
        let fits = |budget: &Budget, n: usize, want: u64| {
            let Ok(cost) = self.listing.decode_cost(path, n) else {
                return false;
            };
            let need = cost.saturating_add(want);
            budget.remaining().is_none_or(|left| need <= left)
        };
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        // A `.gdi` names a GD-ROM's tracks better than its `.cue` twin.
        let mut sheets: Vec<(usize, SheetKind)> = entries
            .iter()
            .enumerate()
            .filter_map(|(n, e)| Some((n, sheet_kind(&e.name)?)))
            .collect();
        sheets.sort_by_key(|&(_, kind)| kind != SheetKind::Gdi);
        let primary = sheets.iter().find_map(|&(n, kind)| {
            let e = &entries[n];
            let read = fits(&budget, n, e.size)
                .then(|| {
                    self.listing
                        .head_with(path, n, MAX_SHEET_BYTES, &mut budget)
                })
                .and_then(|r| r.ok().flatten());
            match read {
                Some(text) => {
                    let file = primary_file(kind, &text)?;
                    let dir = e.name.rfind('/').map_or("", |i| &e.name[..=i]);
                    let want = format!("{dir}{file}");
                    names.iter().position(|t| t.eq_ignore_ascii_case(&want))
                }
                None => {
                    let tracks = infer_redump_set(&names, &e.name)?;
                    let gd_rom = kind == SheetKind::Gdi
                        || sheets.iter().any(|&(m, k)| {
                            k == SheetKind::Gdi && same_stem(&entries[m].name, &e.name)
                        });
                    let track = if gd_rom {
                        tracks.get(2)?
                    } else {
                        tracks.first()?
                    };
                    names.iter().position(|t| t == track)
                }
            }
        });
        let candidates: Vec<usize> = match primary {
            Some(n) => vec![n],
            None => (0..entries.len())
                .filter(|&n| sheet_kind(&entries[n].name).is_none())
                .collect(),
        };
        candidates.into_iter().find_map(|n| {
            let want = entries[n].size.min(MAX_WINDOW as u64);
            if !fits(&budget, n, want) {
                return None;
            }
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
            discs.push(self.identify(&disc).ok().flatten());
        }
        Ok(in_order(discs, system, DiscIdSource::M3uPosition))
    }
}

/// The identity of discs `discs` in order: their serials in order, disc
/// 1's volume fingerprint when every disc has one. `None` when any disc is
/// unknown, of another system or without a serial.
fn in_order(
    discs: Vec<Option<DiscIdentity>>,
    system: &str,
    source: DiscIdSource,
) -> Option<DiscId> {
    let discs: Vec<DiscIdentity> = discs
        .into_iter()
        .map(|d| d.filter(|d| is_of(d, system) && !d.serials.is_empty()))
        .collect::<Option<_>>()?;
    let volume = if discs.iter().all(|d| d.volume.is_some()) {
        discs.first()?.volume.clone()
    } else {
        None
    };
    Some(DiscId {
        serials: discs.into_iter().flat_map(|d| d.serials).collect(),
        disc_no: None,
        volume,
        source,
    })
}

impl DiscProbe for ReaderDiscProbe {
    fn probe(&self, path: &Path, system: &str) -> io::Result<Option<DiscId>> {
        match extension(path).as_str() {
            "m3u" => return self.playlist(path, system),
            "pbp" => {
                return Ok(match pbp_discs(path)? {
                    Some(discs) if discs.len() > 1 => {
                        in_order(discs, system, DiscIdSource::SystemCnf)
                    }
                    Some(mut one) => one
                        .pop()
                        .flatten()
                        .filter(|d| is_of(d, system))
                        .map(|d| d.to_dedupe_id()),
                    None => None,
                })
            }
            _ => {}
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

/// Every disc of the PS1 `EBOOT.PBP` at `path`, in order (`None` for a
/// disc that is not identified); `None` when it holds no PS1 disc.
fn pbp_discs(path: &Path) -> io::Result<Option<Vec<Option<DiscIdentity>>>> {
    let Some(mut pbp) = Pbp::open(io::BufReader::new(File::open(path)?))? else {
        return Ok(None);
    };
    let mut out = Vec::with_capacity(pbp.disc_count());
    for n in 0..pbp.disc_count() {
        let mut disc = pbp.disc(n)?;
        out.push(DiscReader::identify_with(&mut disc, None, None)?);
    }
    Ok(Some(out))
}

/// The disc of the `.cso` at `path`.
fn identify_cso(path: &Path) -> io::Result<Option<DiscIdentity>> {
    let Some(mut cso) = Cso::open(io::BufReader::new(File::open(path)?))? else {
        return Ok(None);
    };
    let len = cso.image_len();
    DiscReader::identify_with(&mut cso, Some(len), None)
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

/// Whether two entry names differ only by extension.
fn same_stem(a: &str, b: &str) -> bool {
    let stem = |n: &str| n.rsplit_once('.').map_or(n, |(s, _)| s).to_lowercase();
    stem(a) == stem(b)
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
