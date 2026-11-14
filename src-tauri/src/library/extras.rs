//! A game's `Extras/` for the panel (part D D4, D16, D17, D22, P-4):
//! listed by entry id on a blocking worker and never written by a read.
//! Each listing gets a revision and stays in the session cache, so a later
//! write or media request names a file by `(rev, index)`, never by path.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use coverly_core::consoles::is_key_material;
use coverly_core::extras::index::{list, IndexState, ListedFile, Listing};
use coverly_core::extras::ops::extras_dir;
use coverly_core::extras::{
    CategoryRef, ExtraFormat, ExtraRecord, ExtraType, ExtrasCategory, Provenance,
};
use coverly_core::game_folder::FixedFolder;
use coverly_core::organize::plan::dir_writable;
use serde::Serialize;

use super::art::inside_root;
use super::Session;
use crate::error::IpcError;

/// Systems whose electronic manual is part of the game (D16): `Manuals/`
/// shows a note instead.
pub const MANUAL_INSIDE: [&str; 4] = ["3ds", "wiiu", "switch", "psvita"];

/// Played or shown inside the app (D17).
const INLINE: &[&str] = &[
    "mp3", "m4a", "aac", "wav", "flac", "ogg", "opus", // audio
    "mp4", "m4v", "webm", "mov", // video
    "png", "jpg", "jpeg", "gif", "webp", "avif", "bmp", "svg", "heic", "tif", "tiff", // image
    "pdf", "txt", "md", "nfo", // document
];

/// Not played here, but safe to hand to another app (D17).
const EXTERNAL: &[&str] = &[
    "spc", "nsf", "nsfe", "gbs", "vgm", "vgz", "psf", "minipsf", "psf2", "minipsf2", "gsf",
    "minigsf", "usf", "miniusf", "2sf", "mini2sf", "ssf", "minissf", "dsf", "minidsf", "qsf",
    "miniqsf", "cbz", "epub",
];

/// What the panel may do with a file (D17).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Playable {
    /// Played or shown in the app (and may open in another app).
    Inline,
    /// Not played here; «Abrir con otra app» is offered.
    External,
    /// Only shown in the file manager: packages, executables, scripts,
    /// shortcuts, symlinks, key material and anything unknown.
    RevealOnly,
}

/// [`Playable`] of a file named `name`; a symlink is never served.
pub fn playable_of(name: &str, link: bool) -> Playable {
    if link || is_key_material(name) {
        return Playable::RevealOnly;
    }
    let Some((_, ext)) = name.rsplit_once('.') else {
        return Playable::RevealOnly;
    };
    let ext = ext.to_ascii_lowercase();
    if INLINE.contains(&ext.as_str()) {
        Playable::Inline
    } else if EXTERNAL.contains(&ext.as_str()) {
        Playable::External
    } else {
        Playable::RevealOnly
    }
}

/// Whether the panel may write this game's extras now.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Writability {
    Yes,
    /// Not a game folder, or a folder the app cannot write.
    ReadOnly,
    /// A job or another write holds the library.
    Busy,
    /// `extras.json` comes from a newer Coverly (D5).
    Newer,
}

/// One category of the section: every fixed one (a drop slot until it
/// exists), then user ones, then the files loose in `Extras/`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CategoryView {
    pub category: CategoryRef,
    /// A folder on disk (always true for [`CategoryRef::Root`]).
    pub present: bool,
    pub count: u32,
    /// A spelling is a symlink: listed, never entered.
    pub link: bool,
    /// Copy to devices (block 4, D21): `None` inherits the default.
    pub sync: Option<bool>,
}

/// One listed file; `index` is its handle within this listing's `rev`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ExtraFileView {
    pub index: u32,
    pub category: CategoryRef,
    /// `/`-joined, relative to `Extras/`: shown, never sent back.
    pub path: String,
    pub name: String,
    pub kind: ExtraType,
    pub provenance: Provenance,
    pub format: Option<ExtraFormat>,
    pub tags: Vec<String>,
    pub size: u64,
    pub link: bool,
    pub playable: Playable,
}

/// A record whose file is gone («No encontrados», D4); `index` names it
/// for «Olvidar».
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MissingView {
    pub index: u32,
    pub category: CategoryRef,
    pub path: String,
    pub name: String,
    pub kind: ExtraType,
    pub provenance: Provenance,
    pub tags: Vec<String>,
    pub size: u64,
}

/// A game's `Extras/` as the panel shows it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ExtrasView {
    pub entry_id: u32,
    pub rev: u64,
    /// False for an entry that is no game folder: nothing to list or write.
    pub game_folder: bool,
    pub index: IndexState,
    pub writable: Writability,
    pub categories: Vec<CategoryView>,
    pub files: Vec<ExtraFileView>,
    pub missing: Vec<MissingView>,
    pub capped: bool,
    pub system: String,
    /// The system keeps the e-manual inside the game (D16).
    pub manual_inside: bool,
}

/// One listing kept by the session.
#[derive(Debug)]
pub struct ListedExtras {
    pub rev: u64,
    /// The game folder as scanned (under the library root); `None` for an
    /// entry that is no game folder.
    pub game_dir: Option<PathBuf>,
    pub listing: Listing,
}

/// A file named by `(rev, index)` of the session's listing.
#[derive(Clone, Debug)]
pub struct ExtrasHandle {
    pub rev: u64,
    pub game_dir: PathBuf,
    pub file: ListedFile,
}

/// The last listing of each entry of the open library (P-4). Revisions
/// grow for the whole process, so one from a closed library never names a
/// listing of the next.
#[derive(Debug, Default)]
pub struct ExtrasCache {
    generation: u64,
    last_rev: u64,
    by_entry: HashMap<u32, Arc<ListedExtras>>,
}

impl ExtrasCache {
    /// Keeps `listing` as entry `entry`'s newest, under a new revision
    /// (generations only grow: an older one is answered but not kept).
    pub fn store(
        &mut self,
        generation: u64,
        entry: u32,
        game_dir: Option<PathBuf>,
        listing: Listing,
    ) -> Arc<ListedExtras> {
        if generation > self.generation {
            self.by_entry.clear();
            self.generation = generation;
        }
        self.last_rev += 1;
        let listed = Arc::new(ListedExtras {
            rev: self.last_rev,
            game_dir,
            listing,
        });
        // A read of a library closed meanwhile lands late: answered, never
        // kept, so it cannot evict the open library's listings.
        if generation == self.generation {
            self.by_entry.insert(entry, Arc::clone(&listed));
        }
        listed
    }

    /// Entry `entry`'s listing, if `rev` is still its newest.
    pub fn get(
        &self,
        generation: u64,
        entry: u32,
        rev: u64,
    ) -> Result<Arc<ListedExtras>, IpcError> {
        match self.by_entry.get(&entry) {
            Some(l) if self.generation == generation && l.rev == rev => Ok(Arc::clone(l)),
            _ => Err(IpcError::ExtrasChanged),
        }
    }
}

/// The game folder of entry `id`, if it is the base at a game folder's
/// root (part C counts its fixed folders); checked inside the library.
pub fn game_dir(session: &Session, id: u32) -> Result<Option<PathBuf>, IpcError> {
    let entry = session
        .entries
        .get(id as usize)
        .ok_or(IpcError::UnknownEntry(id))?;
    let rom = &entry.rom;
    if rom.role.is_some() || rom.fixed_counts.is_none() {
        return Ok(None);
    }
    let dir = if rom.is_dir {
        rom.path.clone()
    } else {
        match rom.path.parent() {
            Some(p) => p.to_path_buf(),
            None => return Ok(None),
        }
    };
    inside_root(session, &dir)?;
    Ok(Some(dir))
}

/// Lists entry `id`'s `Extras/` (blocking). Reads only.
pub fn read_listing(session: &Session, id: u32) -> Result<(Option<PathBuf>, Listing), IpcError> {
    let dir = game_dir(session, id)?;
    let listing = match &dir {
        Some(d) => list(&extras_dir(d)),
        None => empty_listing(),
    };
    Ok((dir, listing))
}

fn empty_listing() -> Listing {
    Listing {
        index: IndexState::Absent,
        categories: Vec::new(),
        files: Vec::new(),
        missing: Vec::new(),
        unreached: Vec::new(),
        capped: false,
        base: None,
    }
}

/// Whether the app cannot write `game_dir`'s extras: the deepest existing
/// of `Extras/` and the game folder is not writable for this process (a
/// read-only mount included), or unreadable. Mode bits alone would miss
/// both, and Windows ignores the read-only attribute on folders.
fn read_only(game_dir: &Path) -> bool {
    let extras = extras_dir(game_dir);
    let probe = if extras.is_dir() {
        extras
    } else {
        game_dir.to_path_buf()
    };
    !probe.is_dir() || !dir_writable(&probe)
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// The category a record's `/`-joined path sits in.
fn category_of_path(path: &str) -> CategoryRef {
    match path.split_once('/') {
        None => CategoryRef::Root,
        Some((first, _)) => ExtrasCategory::from_dir_name(first)
            .map_or_else(|| CategoryRef::User(first.to_string()), CategoryRef::Fixed),
    }
}

fn categories(listing: &Listing, game_folder: bool) -> Vec<CategoryView> {
    if !game_folder {
        return Vec::new();
    }
    let sync = |name: &str| listing.base.as_ref().and_then(|b| b.sync_for(name));
    let mut out: Vec<CategoryView> = ExtrasCategory::ALL
        .into_iter()
        .map(|c| {
            let listed = listing
                .categories
                .iter()
                .find(|l| l.category == CategoryRef::Fixed(c));
            CategoryView {
                category: CategoryRef::Fixed(c),
                present: listed.is_some(),
                count: listed.map_or(0, |l| count(l.count)),
                link: listed.is_some_and(|l| l.link),
                sync: sync(c.dir_name()),
            }
        })
        .collect();
    for l in &listing.categories {
        let name = match &l.category {
            CategoryRef::Fixed(_) => continue,
            CategoryRef::User(n) => Some(n.as_str()),
            CategoryRef::Root => None,
        };
        out.push(CategoryView {
            category: l.category.clone(),
            present: true,
            count: count(l.count),
            link: l.link,
            sync: name.and_then(sync),
        });
    }
    out
}

fn file_view(index: usize, f: &ListedFile) -> ExtraFileView {
    ExtraFileView {
        index: count(index),
        category: f.category.clone(),
        path: f.path.clone(),
        name: f.name.clone(),
        kind: f.record.kind,
        provenance: f.record.provenance,
        format: f.record.origin.format,
        tags: f.record.tags.clone(),
        size: f.size,
        link: f.link,
        playable: playable_of(&f.name, f.link),
    }
}

fn missing_view(index: usize, r: &ExtraRecord) -> MissingView {
    MissingView {
        index: count(index),
        category: category_of_path(&r.path),
        path: r.path.clone(),
        name: r.path.rsplit('/').next().unwrap_or_default().to_string(),
        kind: r.kind,
        provenance: r.provenance,
        tags: r.tags.clone(),
        size: r.size,
    }
}

/// The view of a kept listing. `occupied`: a job or a write holds the
/// library.
pub fn view(entry_id: u32, system: &str, listed: &ListedExtras, occupied: bool) -> ExtrasView {
    let l = &listed.listing;
    let game_folder = listed.game_dir.is_some();
    let writable = match &listed.game_dir {
        None => Writability::ReadOnly,
        Some(_) if matches!(l.index, IndexState::Newer { .. }) => Writability::Newer,
        Some(d) if read_only(d) => Writability::ReadOnly,
        Some(_) if occupied => Writability::Busy,
        Some(_) => Writability::Yes,
    };
    ExtrasView {
        entry_id,
        rev: listed.rev,
        game_folder,
        index: l.index,
        writable,
        categories: categories(l, game_folder),
        files: l
            .files
            .iter()
            .enumerate()
            .map(|(i, f)| file_view(i, f))
            .collect(),
        missing: l
            .missing
            .iter()
            .enumerate()
            .map(|(i, r)| missing_view(i, r))
            .collect(),
        capped: l.capped,
        system: system.to_string(),
        manual_inside: MANUAL_INSIDE.contains(&system),
    }
}

/// The file at `index` of a kept listing.
pub fn handle(listed: &ListedExtras, index: u32) -> Result<ExtrasHandle, IpcError> {
    let (Some(game_dir), Some(file)) = (
        listed.game_dir.as_ref(),
        listed.listing.files.get(index as usize),
    ) else {
        return Err(IpcError::BadRequest(format!("no extra {index}")));
    };
    Ok(ExtrasHandle {
        rev: listed.rev,
        game_dir: game_dir.clone(),
        file: file.clone(),
    })
}

/// The game folder an Extras job wrote in: the folder holding the first
/// `Extras/` among its targets (a set-aside path in `.coverly-trash` keeps
/// the game's path below the job folder).
pub fn job_game(root: &Path, targets: &[PathBuf]) -> Option<String> {
    let extras = FixedFolder::Extras.dir_name();
    targets.iter().find_map(|t| {
        let rel = t.strip_prefix(root).unwrap_or(t);
        let parts: Vec<&OsStr> = rel
            .components()
            .filter_map(|c| match c {
                Component::Normal(n) => Some(n),
                _ => None,
            })
            .collect();
        parts
            .windows(2)
            .find(|w| {
                w[1].to_str()
                    .is_some_and(|n| n.eq_ignore_ascii_case(extras))
            })
            .and_then(|w| w[0].to_str())
            .map(str::to_string)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playable_follows_the_allow_lists() {
        assert_eq!(playable_of("01 Opening.FLAC", false), Playable::Inline);
        assert_eq!(playable_of("intro.webm", false), Playable::Inline);
        assert_eq!(playable_of("manual.pdf", false), Playable::Inline);
        assert_eq!(playable_of("notes.nfo", false), Playable::Inline);
        assert_eq!(playable_of("track.spc", false), Playable::External);
        assert_eq!(playable_of("artbook.cbz", false), Playable::External);
        for name in [
            "theme.p3t",
            "avatar.pkg",
            "setup.exe",
            "run.sh",
            "link.lnk",
            "movie.mkv",
            "README",
            "game.rap",
            "prod.keys",
        ] {
            assert_eq!(playable_of(name, false), Playable::RevealOnly, "{name}");
        }
        assert_eq!(playable_of("front.png", true), Playable::RevealOnly);
    }

    #[test]
    fn a_job_is_named_after_the_folder_holding_extras() {
        let root = Path::new("lib");
        let game = root.join("ROMS").join("PS3").join("Resistance (USA)");
        assert_eq!(
            job_game(root, &[game.join("Extras").join("extras.json")]).as_deref(),
            Some("Resistance (USA)")
        );
        let aside = root
            .join(".coverly-trash")
            .join("job")
            .join("ROMS")
            .join("PS3")
            .join("Resistance (USA)")
            .join("extras")
            .join("Soundtrack")
            .join("a.mp3");
        assert_eq!(
            job_game(root, &[aside]).as_deref(),
            Some("Resistance (USA)")
        );
        assert_eq!(job_game(root, &[root.join("ROMS").join("x.png")]), None);
        assert_eq!(job_game(root, &[]), None);
    }

    #[test]
    fn missing_records_keep_their_category() {
        assert_eq!(category_of_path("a.mp3"), CategoryRef::Root);
        assert_eq!(
            category_of_path("soundtrack/a.mp3"),
            CategoryRef::Fixed(ExtrasCategory::Soundtrack)
        );
        assert_eq!(
            category_of_path("Remixes/deep/a.mp3"),
            CategoryRef::User("Remixes".into())
        );
    }

    #[test]
    fn a_listing_of_a_closed_library_never_evicts_the_open_one() {
        let mut cache = ExtrasCache::default();
        let new = cache.store(3, 0, None, empty_listing());
        // A read that started before the library was reopened lands late.
        let late = cache.store(2, 0, None, empty_listing());
        assert!(late.rev > new.rev);
        assert!(
            cache.get(3, 0, new.rev).is_ok(),
            "the open library keeps its listing"
        );
        assert_eq!(
            cache.get(2, 0, late.rev).unwrap_err(),
            IpcError::ExtrasChanged
        );
    }

    #[test]
    fn a_new_generation_forgets_older_listings() {
        let mut cache = ExtrasCache::default();
        let a = cache.store(1, 0, None, empty_listing());
        assert!(cache.get(1, 0, a.rev).is_ok());
        let b = cache.store(2, 1, None, empty_listing());
        assert!(b.rev > a.rev);
        assert_eq!(cache.get(2, 0, a.rev).unwrap_err(), IpcError::ExtrasChanged);
        assert_eq!(cache.get(1, 1, b.rev).unwrap_err(), IpcError::ExtrasChanged);
        assert!(cache.get(2, 1, b.rev).is_ok());
    }
}
