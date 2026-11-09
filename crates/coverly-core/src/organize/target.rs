//! Where each organised item goes (organise spec §7.3–§8.2): the system
//! folder under the chosen layout, the final file name (catalogue name,
//! FAT-safe characters, Windows path length), collisions with what is
//! already there or planned earlier, duplicates inside the pile,
//! companions that follow their ROM and disc sets that move as a whole.
//!
//! Everything here is read-only: folders are listed, never created.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use unicode_normalization::UnicodeNormalization;

use crate::game_folder::FixedFolder;
use crate::identify::names::{parse_tags, Region};
use crate::organize::plan::{
    Companion, ExtractEntry, GameRole, ItemKind, ItemNote, LeftReason, NameMode, OrganizeAction,
    OrganizeItem, Playlist, RenameReason, Transfer,
};
use crate::profiles::naming::{system_folder, FolderChoice};
use crate::profiles::Profile;
use crate::scan::find_dir_ignoring_case;
use crate::systems::SystemDb;
use crate::write::{name_key, safe_file_name};

/// FAT32 cannot store a file of this size or larger.
const FAT32_LIMIT: u64 = 4 << 30;
/// Longest path many Windows emulators and Explorer cope with, in UTF-16
/// units (`MAX_PATH` minus the terminating nul).
const WINDOWS_MAX_PATH: usize = 259;
/// Longest file name FAT/exFAT and most file systems store, in bytes.
const MAX_NAME_BYTES: usize = 255;
/// Longest file name FAT/NTFS store, in UTF-16 units.
const MAX_NAME_UNITS: usize = 255;
/// Deepest an extracted file may lie below its system folder, in
/// components (archives spec §8.2).
const MAX_EXTRACT_DEPTH: usize = 16;
/// Longest temp name an extraction writes beside a final file:
/// `.coverly-<job>-<n>.tmp` with a 26-character job id
/// (`20261024T121930Z-ffff-ffff`) and a `u32` counter.
pub const TEMP_NAME_MAX: usize = ".coverly-".len() + 26 + 1 + 10 + ".tmp".len();

/// Companion extensions followed by default (spec §7.4); `state*` matches
/// `state`, `state1`, `state.auto`…
pub const DEFAULT_COMPANION_EXTS: [&str; 12] = [
    "sav", "srm", "rtc", "state*", "cht", "ips", "bps", "ups", "txt", "nfo", "png", "jpg",
];

/// [`DEFAULT_COMPANION_EXTS`] as owned strings.
pub fn default_companion_exts() -> Vec<String> {
    DEFAULT_COMPANION_EXTS
        .iter()
        .map(|e| (*e).to_string())
        .collect()
}

/// The layout organised files land in.
#[derive(Clone, Debug)]
pub struct TargetLayout {
    /// `T`, absolute.
    pub target_root: PathBuf,
    /// `T/<profile.roms_root>` (nested) or `T` (*Directly here*),
    /// absolute. An existing folder in another casing is reused.
    pub roms_dir: PathBuf,
    pub profile: Profile,
    /// `settings.organize.folder_names[profile.id]`.
    pub overrides: BTreeMap<String, String>,
    /// The target volume is FAT32: files of 4 GiB or more stay.
    pub fat32: bool,
    /// Keep full paths within 259 UTF-16 units.
    pub windows_paths: bool,
    pub names: NameMode,
    /// Lower-case extensions without the dot; a trailing `*` is a prefix.
    pub companion_exts: Vec<String>,
    /// A modern game's base, updates and DLC go into one folder
    /// (`<System>/<Game>/`; game folders spec D1, D5): the profile's
    /// option, or the user's override for it, resolved for this run.
    pub game_folders: bool,
}

/// Name keys (NFC, lower case) of each target folder: what is on disk,
/// listed once, plus what earlier items claimed. Replaces a `find_twin`
/// per file, which is O(n²) for thousands of files landing in one folder.
#[derive(Debug, Default)]
pub struct DirIndex {
    dirs: HashMap<PathBuf, DirNames>,
    reads: usize,
}

#[derive(Debug, Default)]
struct DirNames {
    /// Key → the on-disk spellings with that key (several only on
    /// case-sensitive volumes).
    on_disk: HashMap<String, Vec<String>>,
    claimed: HashSet<String>,
}

impl DirIndex {
    pub fn new() -> Self {
        Self::default()
    }

    fn names(&mut self, dir: &Path) -> &mut DirNames {
        if !self.dirs.contains_key(dir) {
            let mut names = DirNames::default();
            if let Ok(entries) = fs::read_dir(dir) {
                self.reads += 1;
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    names.on_disk.entry(name_key(&name)).or_default().push(name);
                }
            }
            self.dirs.insert(dir.to_path_buf(), names);
        }
        self.dirs
            .get_mut(dir)
            .expect("the folder was indexed just above")
    }

    /// `name` (ignoring case and Unicode normalisation) exists in `dir`
    /// or was claimed by an earlier item.
    pub fn taken(&mut self, dir: &Path, name: &str) -> bool {
        let key = name_key(name);
        let names = self.names(dir);
        names.claimed.contains(&key) || names.on_disk.contains_key(&key)
    }

    /// The on-disk twin of `name` in `dir`, the exact spelling first.
    pub fn existing(&mut self, dir: &Path, name: &str) -> Option<PathBuf> {
        let key = name_key(name);
        let spellings = self.names(dir).on_disk.get(&key)?;
        let found = spellings
            .iter()
            .find(|s| s.as_str() == name)
            .or_else(|| spellings.first())?;
        Some(dir.join(found))
    }

    /// The on-disk twin of `name` in `dir` whose spelling sorts first by
    /// bytes: on a case-sensitive volume holding both `DLC/` and `dlc/`,
    /// the one Organizar fills (game folders spec D3).
    pub fn existing_first(&mut self, dir: &Path, name: &str) -> Option<PathBuf> {
        let key = name_key(name);
        let first = self.names(dir).on_disk.get(&key)?.iter().min()?.clone();
        Some(dir.join(first))
    }

    /// `name` (ignoring case and Unicode normalisation) was claimed in
    /// `dir` by an earlier item.
    pub fn claimed(&mut self, dir: &Path, name: &str) -> bool {
        let key = name_key(name);
        self.names(dir).claimed.contains(&key)
    }

    /// Reserves `name` in `dir` for a planned item.
    pub fn claim(&mut self, dir: &Path, name: &str) {
        let key = name_key(name);
        self.names(dir).claimed.insert(key);
    }

    /// How many folders were listed (each at most once).
    pub fn read_dirs(&self) -> usize {
        self.reads
    }
}

/// How the entries of an `Extract` item are laid out (archives spec §8.2,
/// §8.3). The planner sets the item's transfer to [`Transfer::Extract`]
/// with the chosen entries (targets still empty) and hands one shape per
/// item to [`assign_targets_with_extracts`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExtractShape {
    /// Cartridge ROMs: each lands under its base name in the system
    /// folder; one ROM may take the catalogue name like a loose file.
    Roms,
    /// A disc set, laid out by the profile's [`DiscLayout`].
    ///
    /// [`DiscLayout`]: crate::profiles::DiscLayout
    Disc {
        /// One entry name per disc, in disc order (the playlist lines).
        discs: Vec<String>,
        /// A multi-disc game's name without disc tags (its folder and
        /// playlist name).
        title: String,
        multi_disc: bool,
    },
    /// A folder game inside an archive (adjustments spec D23): the
    /// entries below `root` land in one folder of the system folder,
    /// named from `name` (the root folder's name, else the archive stem)
    /// like a folder game on disk.
    Folder { root: String, name: String },
    /// Loose discs of one game moved as one set (adjustments spec D24),
    /// laid out like [`ExtractShape::Disc`]: the profile's
    /// [`DiscLayout`] puts them in the system folder or in `<title>/`, the
    /// members keep their names (relative to their common folder), and
    /// `playlist` asks for a `<title>.m3u` listing `discs` (each disc's
    /// sheet or image, relative to the source root) when the profile
    /// wants one and none is at the target.
    ///
    /// [`DiscLayout`]: crate::profiles::DiscLayout
    LooseDiscs {
        discs: Vec<PathBuf>,
        title: String,
        playlist: bool,
    },
}

/// `layout.roms_dir`, reusing an existing folder in another casing.
fn resolved_roms_dir(layout: &TargetLayout) -> PathBuf {
    match layout.roms_dir.strip_prefix(&layout.target_root) {
        Ok(rel) if !rel.as_os_str().is_empty() => {
            find_dir_ignoring_case(&layout.target_root, &rel.to_string_lossy())
                .unwrap_or_else(|| layout.roms_dir.clone())
        }
        _ => layout.roms_dir.clone(),
    }
}

/// The folder `system_id` goes to (absolute) and the layout's choice for
/// it. An existing folder in any casing is reused; when both `gba/` and
/// `GBA/` exist the one spelled like the layout wins.
pub fn folder_dir(
    layout: &TargetLayout,
    db: &SystemDb,
    system_id: &str,
) -> (PathBuf, FolderChoice) {
    folder_in(&resolved_roms_dir(layout), layout, db, system_id)
}

fn folder_in(
    roms: &Path,
    layout: &TargetLayout,
    db: &SystemDb,
    system_id: &str,
) -> (PathBuf, FolderChoice) {
    let choice = match db.get(system_id) {
        Some(system) => system_folder(&layout.profile, system, db, &layout.overrides),
        None => FolderChoice {
            folder: system_id.to_string(),
            host: None,
            on_device: None,
        },
    };
    let dir =
        find_dir_ignoring_case(roms, &choice.folder).unwrap_or_else(|| roms.join(&choice.folder));
    (dir, choice)
}

/// A file that may follow a ROM.
struct Comp {
    /// Relative to the source root.
    rel: PathBuf,
    file_name: String,
    /// Extension as spelled on disk.
    ext: String,
}

/// Plans the target of every `Auto`/`Review` item that has a system
/// (review items too, so the dry run shows where they would go), in
/// order, so earlier items win a contested name:
///
/// 1. the system folder ([`folder_dir`]) and a *no folder on device* note;
/// 2. the name: original or catalogue name (never for sets), FAT-safe,
///    shortened for Windows paths;
/// 3. files of 4 GiB or more on FAT32 stay;
/// 4. collisions with what is on disk (`same_content` → *already present*)
///    or claimed earlier: ` (n)` for single files, the whole set stays;
/// 5. companions with the ROM's stem follow (renamed with it); if one
///    collides under the catalogue name the item keeps its own name;
/// 6. every final name is claimed in `index`.
///
/// `companions` are the walk's companion files (absolute or relative to
/// `source_root`). Companions matching ROMs that go to different folders
/// are returned as *ambiguous companion*.
pub fn assign_targets(
    items: &mut [OrganizeItem],
    layout: &TargetLayout,
    db: &SystemDb,
    source_root: &Path,
    companions: &[PathBuf],
    index: &mut DirIndex,
    same_content: &dyn Fn(&Path, &OrganizeItem) -> bool,
) -> Vec<(PathBuf, LeftReason)> {
    assign_targets_with_extracts(
        items,
        &[],
        layout,
        db,
        source_root,
        companions,
        index,
        same_content,
    )
}

/// [`assign_targets`] where `shapes[i]` lays out item `i` when its
/// transfer is [`Transfer::Extract`] ([`assign_extract_targets`]). Every
/// item, moved or extracted, claims its names in plan order, so the first
/// item keeps a contested name and a later one stays (`CollisionSet`) or
/// is numbered.
#[allow(clippy::too_many_arguments)]
pub fn assign_targets_with_extracts(
    items: &mut [OrganizeItem],
    shapes: &[Option<ExtractShape>],
    layout: &TargetLayout,
    db: &SystemDb,
    source_root: &Path,
    companions: &[PathBuf],
    index: &mut DirIndex,
    same_content: &dyn Fn(&Path, &OrganizeItem) -> bool,
) -> Vec<(PathBuf, LeftReason)> {
    assign_targets_in(
        items,
        shapes,
        &[],
        layout,
        db,
        source_root,
        companions,
        index,
        same_content,
    )
}

/// Where an item goes below its system folder instead of the folder
/// itself (see [`assign_targets_in`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Subdir {
    /// The frontend's DLC folder ([`dlc_target`]).
    Dlc(PathBuf),
    /// The base's game folder of that name (game folders spec D5,
    /// [`game_folder_name`]): its file is named `<Game>.<ext>`, a set's
    /// members go inside it.
    Game(String),
    /// An update or DLC of a base in a game folder: placed afterwards by
    /// [`game_children_targets`], from its base's final folder.
    Child,
}

/// [`assign_targets_with_extracts`] where `subdirs[i]`, when set, says
/// where item `i` goes below its system folder ([`Subdir`]): a
/// [`Subdir::Dlc`] item keeps its own name, takes no folder game suffix
/// (only the base game is listed) and gets [`ItemNote::DlcFolder`]; a
/// [`Subdir::Game`] base goes into its game folder; a [`Subdir::Child`]
/// gets no target here. `item.folder` still names the system folder.
#[allow(clippy::too_many_arguments)]
pub fn assign_targets_in(
    items: &mut [OrganizeItem],
    shapes: &[Option<ExtractShape>],
    subdirs: &[Option<Subdir>],
    layout: &TargetLayout,
    db: &SystemDb,
    source_root: &Path,
    companions: &[PathBuf],
    index: &mut DirIndex,
    same_content: &dyn Fn(&Path, &OrganizeItem) -> bool,
) -> Vec<(PathBuf, LeftReason)> {
    let roms = resolved_roms_dir(layout);
    let mut folders: HashMap<String, (PathBuf, FolderChoice)> = HashMap::new();
    let mut dirs: Vec<Option<(PathBuf, FolderChoice)>> = Vec::with_capacity(items.len());
    for (pos, item) in items.iter().enumerate() {
        let child = matches!(subdirs.get(pos), Some(Some(Subdir::Child)));
        let dir = match (&item.system_id, item.moves_in() && !child) {
            (Some(system), true) => Some(
                folders
                    .entry(system.clone())
                    .or_insert_with(|| folder_in(&roms, layout, db, system))
                    .clone(),
            ),
            _ => None,
        };
        dirs.push(dir);
    }

    let (attached, left) = match_companions(items, &dirs, layout, source_root, companions);

    for (pos, item) in items.iter_mut().enumerate() {
        let Some((system_dir, choice)) = &dirs[pos] else {
            continue;
        };
        let subdir = subdirs.get(pos).and_then(Option::as_ref);
        let in_dlc = matches!(subdir, Some(Subdir::Dlc(_)));
        let game_stem = match subdir {
            Some(Subdir::Game(name)) => Some(name.as_str()),
            _ => None,
        };
        let dir = &match subdir {
            Some(Subdir::Dlc(sub)) => system_dir.join(sub),
            Some(Subdir::Game(name)) => system_dir.join(name),
            _ => system_dir.clone(),
        };
        item.target = None;
        item.set_targets.clear();
        item.companions.clear();
        item.rename = None;
        item.playlist = None;
        item.folder = system_dir
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .or_else(|| Some(choice.folder.clone()));
        if choice.on_device == Some(false) {
            let note = ItemNote::NoFolderOnDevice {
                folder: choice.folder.clone(),
            };
            if !item.notes.contains(&note) {
                item.notes.push(note);
            }
        }
        if layout.fat32 && too_big_for_fat(item) {
            item.leave(LeftReason::TooBigForFat);
            continue;
        }
        let comps: Vec<&Comp> = attached[pos].iter().collect();
        let shape = shapes.get(pos).and_then(Option::as_ref);
        let outcome = match (shape, &item.transfer) {
            (Some(ExtractShape::Folder { root, name }), Transfer::Extract { .. }) => {
                extract_folder(item, (root, name), dir, layout, !in_dlc, &comps, index)
            }
            (Some(shape), Transfer::Extract { .. }) => {
                assign_extract_targets(item, shape, dir, layout, &comps, index)
            }
            (
                Some(ExtractShape::LooseDiscs {
                    discs,
                    title,
                    playlist,
                }),
                Transfer::AsIs,
            ) if item.kind == ItemKind::Set => {
                place_loose_discs(item, (discs, title, *playlist), dir, layout, &comps, index)
            }
            _ => match item.kind {
                ItemKind::Set => plan_set(item, dir, &comps, index, same_content),
                ItemKind::Folder => plan_folder(item, dir, layout, !in_dlc, index),
                ItemKind::File | ItemKind::Archive => {
                    let original = file_name(&item.sources[0]);
                    let stem = split_name(&original).0.to_string();
                    plan_single(
                        item,
                        (&original, &stem),
                        game_stem,
                        dir,
                        layout,
                        &comps,
                        index,
                        same_content,
                    )
                }
            },
        };
        match outcome {
            Planned::Placed(placed) => {
                for (dir, name) in &placed.claims {
                    index.claim(dir, name);
                }
                let rel = |p: &Path| relative(&layout.target_root, p);
                item.target = Some(rel(&placed.target));
                item.set_targets = placed.set_targets.iter().map(|p| rel(p)).collect();
                item.rename = placed.rename;
                if let Transfer::Extract { entries, m3u, .. } = &mut item.transfer {
                    for (entry, target) in entries.iter_mut().zip(&placed.entry_targets) {
                        entry.target = rel(target);
                    }
                    *m3u = placed.m3u.as_deref().map(rel);
                } else if let Some(m3u) = &placed.m3u {
                    item.playlist = Some(Playlist {
                        path: rel(m3u),
                        discs: placed.playlist_discs.iter().map(|p| rel(p)).collect(),
                    });
                }
                item.companions = placed
                    .companions
                    .into_iter()
                    .map(|(source, target)| Companion {
                        source,
                        target: rel(&target),
                        licence: false,
                    })
                    .collect();
                item.notes
                    .retain(|n| !matches!(n, ItemNote::DlcFolder { .. }));
                if in_dlc {
                    item.notes.push(ItemNote::DlcFolder {
                        layout: layout.profile.name.clone(),
                    });
                }
            }
            Planned::Left(LeftReason::AlreadyPresent { at }) => {
                item.leave(LeftReason::AlreadyPresent {
                    at: relative(&layout.target_root, &at),
                })
            }
            Planned::Left(reason) => item.leave(reason),
        }
    }
    left
}

/// Companions attached to each item (by position) and the ambiguous ones.
fn match_companions(
    items: &[OrganizeItem],
    dirs: &[Option<(PathBuf, FolderChoice)>],
    layout: &TargetLayout,
    source_root: &Path,
    companions: &[PathBuf],
) -> (Vec<Vec<Comp>>, Vec<(PathBuf, LeftReason)>) {
    let mut attached: Vec<Vec<Comp>> = items.iter().map(|_| Vec::new()).collect();
    let mut left = Vec::new();
    let mut by_stem: HashMap<(PathBuf, String), Vec<usize>> = HashMap::new();
    let mut sources: HashSet<&Path> = HashSet::new();
    // Every `.wbfs` of the pile by stem, moving or not: a split part with
    // none is an incomplete set, one whose image stays stays with it.
    let mut wbfs_stems: HashSet<(PathBuf, String)> = HashSet::new();
    for (pos, item) in items.iter().enumerate() {
        sources.extend(item.sources.iter().map(PathBuf::as_path));
        if let Some(primary) = item.sources.first().filter(|p| is_wbfs(p)) {
            let name = file_name(primary);
            wbfs_stems.insert((parent(primary), name_key(split_name(&name).0)));
        }
        let (Some(_), Some(primary)) = (&dirs[pos], item.sources.first()) else {
            continue;
        };
        // A folder game takes no companion: its files travel inside it.
        if item.kind == ItemKind::Folder {
            continue;
        }
        let stem = split_name(&file_name(primary)).0.to_string();
        by_stem
            .entry((parent(primary), name_key(&stem)))
            .or_default()
            .push(pos);
    }
    for given in companions {
        let rel = given
            .strip_prefix(source_root)
            .unwrap_or(given)
            .to_path_buf();
        if sources.contains(rel.as_path()) {
            continue;
        }
        let name = file_name(&rel);
        let folder = parent(&rel);
        // A split WBFS part follows its `.wbfs` only, whatever the
        // companion setting says (systems batch 2 §12).
        let (stem, ext) = split_name(&name);
        if let Some(ext) = ext.filter(|e| is_wbfs_part(e)) {
            let key = (folder.clone(), name_key(stem));
            if !wbfs_stems.contains(&key) {
                left.push((
                    given.clone(),
                    LeftReason::SetIncomplete {
                        missing: vec![format!("{stem}.wbfs")],
                    },
                ));
                continue;
            }
            let image = by_stem.get(&key).and_then(|matches| {
                matches
                    .iter()
                    .copied()
                    .find(|&pos| items[pos].sources.first().is_some_and(|p| is_wbfs(p)))
            });
            if let Some(pos) = image {
                attached[pos].push(Comp {
                    rel,
                    ext: ext.to_string(),
                    file_name: name.clone(),
                });
            }
            continue;
        }
        // `Game.sav`, but also `Game.state.auto`: the stem may end at any
        // dot, the last one first.
        let found = name
            .match_indices('.')
            .map(|(i, _)| i)
            .rev()
            .filter(|&i| i > 0 && i + 1 < name.len())
            .find_map(|i| {
                let ext = &name[i + 1..];
                if !companion_ext_matches(&layout.companion_exts, ext) {
                    return None;
                }
                by_stem
                    .get(&(folder.clone(), name_key(&name[..i])))
                    .map(|matches| (ext, matches))
            });
        let Some((ext, matches)) = found else {
            continue;
        };
        let folders: HashSet<String> = matches
            .iter()
            .filter_map(|&pos| dirs[pos].as_ref())
            .map(|(dir, _)| name_key(&dir.to_string_lossy()))
            .collect();
        if folders.len() > 1 {
            left.push((given.clone(), LeftReason::AmbiguousCompanion));
            continue;
        }
        attached[matches[0]].push(Comp {
            rel,
            ext: ext.to_string(),
            file_name: name.clone(),
        });
    }
    (attached, left)
}

/// Same-stem companions of each set-aside item (duplicates spec §9.1):
/// a save, state or patch beside a pile copy that is set aside goes with
/// it, to the same place below its quarantine. A companion already
/// attached to an item, or whose stem also names an item that is not set
/// aside in the same folder (a shared stem), stays.
pub(crate) fn attach_set_aside_companions(
    items: &mut [OrganizeItem],
    companions: &[PathBuf],
    source_root: &Path,
    companion_exts: &[String],
) {
    let mut claimed: HashSet<PathBuf> = items
        .iter()
        .flat_map(|i| {
            i.sources
                .iter()
                .cloned()
                .chain(i.companions.iter().map(|c| c.source.clone()))
        })
        .collect();
    let stem_key = |primary: &Path| (parent(primary), name_key(split_name(&file_name(primary)).0));
    let staying: HashSet<(PathBuf, String)> = items
        .iter()
        .filter(|i| !i.is_set_aside())
        .filter_map(|i| i.sources.first().map(|p| stem_key(p)))
        .collect();
    for item in items.iter_mut() {
        let OrganizeAction::SetAside { dest, .. } = &item.action else {
            continue;
        };
        let Some(primary) = item.sources.first() else {
            continue;
        };
        let key = stem_key(primary);
        if staying.contains(&key) {
            continue;
        }
        // The quarantine folder: `dest` without the primary's own path.
        let mut base = dest.clone();
        if !dest.ends_with(primary) || !primary.components().all(|_| base.pop()) {
            continue;
        }
        let mut found = Vec::new();
        for given in companions {
            let rel = given.strip_prefix(source_root).unwrap_or(given);
            if claimed.contains(rel) || parent(rel) != key.0 {
                continue;
            }
            let name = file_name(rel);
            let matches = name
                .match_indices('.')
                .map(|(i, _)| i)
                .filter(|&i| i > 0 && i + 1 < name.len())
                .any(|i| {
                    let ext = &name[i + 1..];
                    name_key(&name[..i]) == key.1
                        && if is_wbfs_part(ext) {
                            is_wbfs(primary)
                        } else {
                            companion_ext_matches(companion_exts, ext)
                        }
                });
            if matches {
                found.push(Companion {
                    source: rel.to_path_buf(),
                    target: base.join(rel),
                    licence: false,
                });
            }
        }
        for c in found {
            claimed.insert(c.source.clone());
            item.companions.push(c);
        }
    }
}

/// `wbf1`…`wbf3`: the parts a WBFS tool splits a 4 GiB image into for
/// FAT32 (libwbfs `wbfs_split`), any case.
pub(crate) fn is_wbfs_part(ext: &str) -> bool {
    let e = ext.to_ascii_lowercase();
    matches!(e.as_str(), "wbf1" | "wbf2" | "wbf3")
}

fn is_wbfs(path: &Path) -> bool {
    split_name(&file_name(path))
        .1
        .is_some_and(|e| e.eq_ignore_ascii_case("wbfs"))
}

pub(crate) fn companion_ext_matches(patterns: &[String], ext: &str) -> bool {
    let ext = ext.to_lowercase();
    patterns.iter().any(|p| {
        let p = p.trim().trim_start_matches('.').to_lowercase();
        match p.strip_suffix('*') {
            Some(prefix) => ext.starts_with(prefix),
            None => ext == p,
        }
    })
}

fn too_big_for_fat(item: &OrganizeItem) -> bool {
    // A folder game is judged per contained file by the planner (D27):
    // its total does not matter.
    if item.kind == ItemKind::Folder {
        return false;
    }
    // An extracted file is judged by its own declared size, not the
    // archive's (archives spec §8.4).
    if let Transfer::Extract { entries, .. } = &item.transfer {
        return entries.iter().any(|e| e.size >= FAT32_LIMIT);
    }
    let largest_source = item.fps.iter().map(|fp| fp.size).max();
    let largest = match item.kind {
        ItemKind::Set => largest_source.unwrap_or(item.size),
        _ => largest_source.unwrap_or(0).max(item.size),
    };
    largest >= FAT32_LIMIT
}

/// What [`plan_single`] / [`plan_set`] decided (paths absolute).
enum Planned {
    Placed(Placed),
    Left(LeftReason),
}

struct Placed {
    target: PathBuf,
    set_targets: Vec<PathBuf>,
    rename: Option<RenameReason>,
    /// (source relative to S, absolute target).
    companions: Vec<(PathBuf, PathBuf)>,
    /// Every (folder, name) to claim.
    claims: Vec<(PathBuf, String)>,
    /// The target of each extracted entry, in the transfer's order.
    entry_targets: Vec<PathBuf>,
    /// A generated playlist.
    m3u: Option<PathBuf>,
    /// The targets a loose set's playlist lists, in disc order.
    playlist_discs: Vec<PathBuf>,
}

/// One search for a free name from a base stem.
enum Search {
    Found {
        name: String,
        n: u32,
        cut: bool,
        companions: Vec<(PathBuf, String)>,
    },
    Present(PathBuf),
    /// The unnumbered name is free but a companion's is not.
    CompanionBlocked,
}

/// One file named `original` in `dir`; companions beside the source
/// matched `companion_stem` (the source's stem, or the archive's stem
/// for a ROM extracted from it).
///
/// `game_stem`, a base going into its game folder (game folders spec
/// D5), names the file `<game_stem>.<ext>` instead
/// ([`RenameReason::GameFolder`] when that differs from `original`).
#[allow(clippy::too_many_arguments)]
fn plan_single(
    item: &OrganizeItem,
    (original, companion_stem): (&str, &str),
    game_stem: Option<&str>,
    dir: &Path,
    layout: &TargetLayout,
    comps: &[&Comp],
    index: &mut DirIndex,
    same_content: &dyn Fn(&Path, &OrganizeItem) -> bool,
) -> Planned {
    let original_stem = companion_stem;
    let keep = match game_stem {
        Some(game) => {
            let wanted = match split_name(original).1 {
                Some(ext) => format!("{game}.{ext}"),
                None => game.to_string(),
            };
            let mut base = safe_base(&wanted, original, false);
            base.reason = (base.name != original).then(|| RenameReason::GameFolder {
                from: original.to_string(),
            });
            base
        }
        None => safe_base(original, original, false),
    };
    let catalogue = match (&layout.names, &item.catalogue_name) {
        (NameMode::Catalogue, Some(game)) if !game.trim().is_empty() && game_stem.is_none() => {
            let ext = split_name(original).1;
            let wanted = match ext {
                Some(ext) => format!("{}.{ext}", game.trim()),
                None => game.trim().to_string(),
            };
            let base = safe_base(&wanted, original, true);
            (base.name != keep.name).then_some(base)
        }
        _ => None,
    };

    let mut search = |base: &Base, stop_when_blocked: bool| {
        find_free(
            item,
            dir,
            base,
            original_stem,
            comps,
            layout.windows_paths,
            stop_when_blocked,
            index,
            same_content,
        )
    };
    let (base, found) = match catalogue {
        Some(cat) => match search(&cat, true) {
            Search::CompanionBlocked => {
                let found = search(&keep, false);
                (keep, found)
            }
            found => (cat, found),
        },
        None => {
            let found = search(&keep, false);
            (keep, found)
        }
    };
    match found {
        Search::Present(at) => Planned::Left(LeftReason::AlreadyPresent { at }),
        Search::CompanionBlocked => unreachable!("only the catalogue search stops when blocked"),
        Search::Found {
            name,
            n,
            cut,
            companions,
        } => {
            let rename = if n > 1 {
                Some(RenameReason::Collision {
                    name: compose(dir, &base.stem, base.ext.as_deref(), &[], 1, false).0,
                })
            } else if cut {
                Some(RenameReason::PathLength)
            } else {
                base.reason
            };
            let mut claims = vec![(dir.to_path_buf(), name.clone())];
            let companions = companions
                .into_iter()
                .map(|(source, name)| {
                    claims.push((dir.to_path_buf(), name.clone()));
                    (source, dir.join(name))
                })
                .collect();
            Planned::Placed(Placed {
                target: dir.join(name),
                set_targets: Vec::new(),
                rename,
                companions,
                claims,
                entry_targets: Vec::new(),
                m3u: None,
                playlist_discs: Vec::new(),
            })
        }
    }
}

/// A candidate name before collisions: FAT-safe stem and extension and
/// why it differs from the source name.
struct Base {
    name: String,
    stem: String,
    ext: Option<String>,
    reason: Option<RenameReason>,
}

/// `wanted` made FAT-safe. A catalogue name reports `Catalogue` (the safe
/// characters are part of it); otherwise a change reports `SafeName`.
fn safe_base(wanted: &str, original: &str, catalogue: bool) -> Base {
    let name = safe_file_name(wanted).unwrap_or_else(|_| original.to_string());
    let reason = if catalogue {
        Some(RenameReason::Catalogue {
            from: original.to_string(),
        })
    } else if name != wanted {
        Some(RenameReason::SafeName {
            from: wanted.to_string(),
        })
    } else {
        None
    };
    let (stem, ext) = split_name(&name);
    Base {
        stem: stem.to_string(),
        ext: ext.map(str::to_string),
        name,
        reason,
    }
}

/// The first `n` (1 = no suffix) whose ROM name and companion names are
/// all free in `dir`. An on-disk twin with the same content ends the
/// search as *already present*.
#[allow(clippy::too_many_arguments)]
fn find_free(
    item: &OrganizeItem,
    dir: &Path,
    base: &Base,
    original_stem: &str,
    comps: &[&Comp],
    windows_paths: bool,
    stop_when_blocked: bool,
    index: &mut DirIndex,
    same_content: &dyn Fn(&Path, &OrganizeItem) -> bool,
) -> Search {
    // Companions take the ROM's stem: their names must fit too.
    let comp_exts: Vec<&str> = comps.iter().map(|c| c.ext.as_str()).collect();
    for n in 1u32.. {
        let (name, cut) = compose(
            dir,
            &base.stem,
            base.ext.as_deref(),
            &comp_exts,
            n,
            windows_paths,
        );
        if index.taken(dir, &name) {
            if let Some(existing) = index.existing(dir, &name) {
                if same_content(&existing, item) {
                    return Search::Present(existing);
                }
            }
            continue;
        }
        let stem = split_name(&name).0;
        let mut keys: HashSet<String> = HashSet::from([name_key(&name)]);
        let mut companions = Vec::with_capacity(comps.len());
        let mut blocked = false;
        for comp in comps {
            let comp_name = if stem == original_stem {
                comp.file_name.clone()
            } else {
                format!("{stem}.{}", comp.ext)
            };
            if !keys.insert(name_key(&comp_name)) || index.taken(dir, &comp_name) {
                blocked = true;
                break;
            }
            companions.push((comp.rel.clone(), comp_name));
        }
        if blocked {
            if stop_when_blocked && n == 1 {
                return Search::CompanionBlocked;
            }
            continue;
        }
        return Search::Found {
            name,
            n,
            cut,
            companions,
        };
    }
    unreachable!("a folder holds finitely many names")
}

/// `{stem}[ ({n})][.{ext}]`, the stem cut until the name fits 255 bytes
/// and, with `windows_paths`, the full path fits 259 UTF-16 units. The
/// same stem with each of `also_exts` (companions renamed with the ROM)
/// must fit as well. The flag says whether the stem was cut.
fn compose(
    dir: &Path,
    stem: &str,
    ext: Option<&str>,
    also_exts: &[&str],
    n: u32,
    windows_paths: bool,
) -> (String, bool) {
    let suffix = if n > 1 {
        format!(" ({n})")
    } else {
        String::new()
    };
    let dot_ext = ext.map(|e| format!(".{e}")).unwrap_or_default();
    let mut stem = stem.to_string();
    let mut cut = false;
    loop {
        let name = format!("{stem}{suffix}{dot_ext}");
        let too_long = |name: &str| {
            name.len() > MAX_NAME_BYTES
                || (windows_paths
                    && dir.join(name).to_string_lossy().encode_utf16().count() > WINDOWS_MAX_PATH)
        };
        let any_too_long = too_long(&name)
            || also_exts
                .iter()
                .any(|e| too_long(&format!("{stem}{suffix}.{e}")));
        if !any_too_long || stem.chars().count() <= 1 {
            return (name, cut);
        }
        stem.pop();
        while stem.chars().count() > 1 && (stem.ends_with(' ') || stem.ends_with('.')) {
            stem.pop();
        }
        cut = true;
    }
}

/// A folder game keeps its folder name, plus the layout's suffix for its
/// system when the name lacks it (systems batch 2 D13, any ASCII case),
/// FAT-safe. A name taken on disk or by an earlier item gets the next
/// free ` (n)`, inserted before the suffix (`Game (2).ps3`): a folder is
/// never merged into another, nor taken for «already present».
fn plan_folder(
    item: &mut OrganizeItem,
    dir: &Path,
    layout: &TargetLayout,
    with_suffix: bool,
    index: &mut DirIndex,
) -> Planned {
    let original = file_name(&item.sources[0]);
    let (name, rename) = folder_game_name(item, &original, dir, layout, with_suffix, index);
    Planned::Placed(Placed {
        target: dir.join(&name),
        set_targets: Vec::new(),
        rename,
        companions: Vec::new(),
        claims: vec![(dir.to_path_buf(), name)],
        entry_targets: Vec::new(),
        m3u: None,
        playlist_discs: Vec::new(),
    })
}

/// The name a folder game called `original` takes in `dir`: FAT-safe,
/// with the layout's suffix for its system when it lacks it (noted
/// [`ItemNote::FolderSuffix`]), and the next free ` (n)` before the
/// suffix when the name is taken.
fn folder_game_name(
    item: &mut OrganizeItem,
    original: &str,
    dir: &Path,
    layout: &TargetLayout,
    with_suffix: bool,
    index: &mut DirIndex,
) -> (String, Option<RenameReason>) {
    let safe = safe_file_name(original).unwrap_or_else(|_| original.to_string());
    let suffix = item
        .system_id
        .as_deref()
        .filter(|_| with_suffix)
        .and_then(|s| layout.profile.folder_game_suffix.get(s))
        .filter(|s| !s.is_empty());
    let (stem, suffix, appended) = match suffix {
        Some(suffix) => match strip_suffix_ignoring_case(&safe, suffix) {
            Some(stem) => (stem.to_string(), safe[stem.len()..].to_string(), false),
            None => (safe.clone(), suffix.clone(), true),
        },
        None => (safe.clone(), String::new(), false),
    };
    let wanted = format!("{stem}{suffix}");
    let name = (1u32..)
        .map(|n| {
            if n == 1 {
                wanted.clone()
            } else {
                format!("{stem} ({n}){suffix}")
            }
        })
        .find(|name| !index.taken(dir, name))
        .expect("a folder holds finitely many names");
    let rename = if name != wanted {
        Some(RenameReason::Collision {
            name: wanted.clone(),
        })
    } else if safe != original {
        Some(RenameReason::SafeName {
            from: original.to_string(),
        })
    } else {
        None
    };
    item.notes
        .retain(|n| !matches!(n, ItemNote::FolderSuffix { .. }));
    if appended {
        item.notes.push(ItemNote::FolderSuffix {
            name: name.clone(),
            layout: layout.profile.name.clone(),
        });
    }
    (name, rename)
}

/// A folder game out of an archive (adjustments spec D23): the folder is
/// named as [`plan_folder`] names one on disk, and every entry below
/// `root` lands at its path below the root inside it; `with_suffix` as
/// for [`plan_folder`] (no suffix in a DLC folder). Each component
/// must be stored unchanged and fit; the folder's name is claimed in
/// `dir`, so no later item shares it.
#[allow(clippy::too_many_arguments)]
fn extract_folder(
    item: &mut OrganizeItem,
    (root, name): (&str, &str),
    dir: &Path,
    layout: &TargetLayout,
    with_suffix: bool,
    comps: &[&Comp],
    index: &mut DirIndex,
) -> Planned {
    let Transfer::Extract { entries, .. } = &item.transfer else {
        return Planned::Left(LeftReason::Unknown);
    };
    let entries = entries.clone();
    if name.trim().is_empty() {
        return Planned::Left(LeftReason::SetUnsafeName {
            name: name.to_string(),
        });
    }
    let (folder_name, rename) = folder_game_name(item, name, dir, layout, with_suffix, index);
    let folder = dir.join(&folder_name);
    let skip = if root.is_empty() {
        0
    } else {
        root.split('/').count()
    };
    let mut targets = Vec::with_capacity(entries.len());
    for e in &entries {
        let mut target = folder.clone();
        for part in e.name.split('/').filter(|p| !p.is_empty()).skip(skip) {
            if name_too_long(part) {
                return Planned::Left(LeftReason::PathTooLong);
            }
            if safe_file_name(part).ok().as_deref() != Some(part) {
                return Planned::Left(LeftReason::SetUnsafeName {
                    name: e.name.clone(),
                });
            }
            target.push(part);
        }
        targets.push(target);
    }
    place_extracted(targets, None, dir, &folder, layout, comps, index).map_placed(|p| {
        p.target = folder.clone();
        p.rename = rename;
        p.claims.push((dir.to_path_buf(), folder_name));
    })
}

/// `name` without `suffix` at its end (ASCII case-insensitive), when the
/// rest is not empty.
pub(crate) fn strip_suffix_ignoring_case<'a>(name: &'a str, suffix: &str) -> Option<&'a str> {
    let cut = name.len().checked_sub(suffix.len()).filter(|&c| c > 0)?;
    (name.is_char_boundary(cut) && name[cut..].eq_ignore_ascii_case(suffix)).then(|| &name[..cut])
}

/// The folder below `system`'s folder where `layout` reads DLC of the
/// base game `title_id` (systems batch 3 D16): the profile's
/// `dlc_folder` template with `{title_id}` replaced, one component per
/// `/` segment. `None` when the profile has no such folder for the
/// system, or the title id is not a plain code (ASCII letters and
/// digits, at most 32): it never names a path of its own.
pub fn dlc_target(layout: &TargetLayout, system: &str, title_id: &str) -> Option<PathBuf> {
    let template = layout.profile.dlc_folder.get(system)?;
    let plain = !title_id.is_empty()
        && title_id.len() <= 32
        && title_id.bytes().all(|b| b.is_ascii_alphanumeric());
    if !plain {
        return None;
    }
    let filled = template.replace(crate::profiles::custom::DLC_FOLDER_TOKEN, title_id);
    let mut out = PathBuf::new();
    for seg in filled.split('/') {
        if seg.is_empty() || seg == "." || seg == ".." {
            return None;
        }
        out.push(seg);
    }
    (!out.as_os_str().is_empty()).then_some(out)
}

/// A shadPS4 overlay folder and its base, both items of the plan (D11):
/// same parent directory, `sibling` named `<base name><suffix>` with
/// `suffix` one of `-UPDATE`/`-patch` (ASCII case-insensitive, `suffix`
/// keeps the source's spelling), the sibling's console kind `Update`.
/// `base` and `sibling` are positions in the plan's items.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OverlayPair {
    pub base: usize,
    pub sibling: usize,
    pub suffix: String,
}

/// Every [`OverlayPair`] of `items`; `is_update(i)` says whether item `i`
/// is a PS4 update by its header (the plan's survey knows), and
/// `archive_folder(i)` names the folder game item `i` extracts out of an
/// archive (adjustments spec D23), which pairs like a folder of that
/// name. The base is the folder item beside the sibling named exactly
/// like the sibling without its suffix, else one whose name differs only
/// in ASCII case (a case-insensitive volume, where shadPS4 finds it the
/// same way).
pub fn overlay_pairs(
    items: &[OrganizeItem],
    is_update: &dyn Fn(usize) -> bool,
    archive_folder: &dyn Fn(usize) -> Option<String>,
) -> Vec<OverlayPair> {
    let folder_name = |pos: usize, item: &OrganizeItem| -> Option<String> {
        if item.sources.is_empty() {
            return None;
        }
        match item.kind {
            ItemKind::Folder => Some(file_name(&item.sources[0])),
            ItemKind::Archive => archive_folder(pos),
            _ => None,
        }
    };
    let mut by_name: HashMap<(PathBuf, String), Vec<(usize, String)>> = HashMap::new();
    for (pos, item) in items.iter().enumerate() {
        let Some(name) = folder_name(pos, item) else {
            continue;
        };
        by_name
            .entry((parent(&item.sources[0]), name.to_ascii_lowercase()))
            .or_default()
            .push((pos, name));
    }
    let mut pairs = Vec::new();
    for (pos, item) in items.iter().enumerate() {
        if !is_update(pos) {
            continue;
        }
        let Some(name) = folder_name(pos, item) else {
            continue;
        };
        let Some((stem, suffix)) = crate::consoles::sony::UPDATE_SUFFIXES.iter().find_map(|s| {
            strip_suffix_ignoring_case(&name, s).map(|stem| (stem, &name[stem.len()..]))
        }) else {
            continue;
        };
        let key = (parent(&item.sources[0]), stem.to_ascii_lowercase());
        let Some(found) = by_name.get(&key) else {
            continue;
        };
        let candidates: Vec<&(usize, String)> = found.iter().filter(|(b, _)| *b != pos).collect();
        let exact = candidates.iter().find(|(_, n)| n == stem).map(|(b, _)| *b);
        if let Some(base) = exact.or_else(|| candidates.first().map(|(b, _)| *b)) {
            pairs.push(OverlayPair {
                base,
                sibling: pos,
                suffix: suffix.to_string(),
            });
        }
    }
    pairs
}

/// D11 (e): after the user's decisions, a sibling takes its base's action:
/// base left (any reason) or set aside → sibling
/// `Left(GameOverlay { of: <base title> })`; base in review → sibling in
/// review with the base's system and alternatives; base moved → sibling
/// keeps its own action (Auto or Review by its own evidence; a sibling
/// Review never blocks the base). A sibling already left or set aside
/// keeps that. `titles(base)` is the base's title for the row.
pub fn siblings_follow(
    items: &mut [OrganizeItem],
    pairs: &[OverlayPair],
    titles: &dyn Fn(usize) -> String,
) {
    for pair in pairs {
        let Some(base) = items.get(pair.base) else {
            continue;
        };
        let (action, system, alternatives) = (
            base.action.clone(),
            base.system_id.clone(),
            base.alternatives.clone(),
        );
        let Some(sibling) = items.get_mut(pair.sibling) else {
            continue;
        };
        if !sibling.moves_in() {
            continue;
        }
        match action {
            OrganizeAction::Auto => {}
            OrganizeAction::Review => {
                sibling.action = OrganizeAction::Review;
                sibling.system_id = system;
                sibling.alternatives = alternatives;
            }
            OrganizeAction::Left { .. } | OrganizeAction::SetAside { .. } => {
                sibling.leave(LeftReason::GameOverlay {
                    of: titles(pair.base),
                });
            }
        }
    }
}

/// D11 (b): after targets, each sibling's target is
/// `<dir of base target>/<base final file name><suffix>`, so a collision
/// rename or a frontend suffix of the base carries over
/// (`CUSA00900 (2)-UPDATE`, `CUSA00900.ps4-UPDATE`). A sibling whose name
/// is taken (on disk or by another item) leaves the base and all its
/// siblings `CollisionSet`. A sibling with no base item, or one going to
/// another system than its base, keeps its own target (c).
pub fn sibling_targets(items: &mut [OrganizeItem], pairs: &[OverlayPair], layout: &TargetLayout) {
    let mut groups: BTreeMap<usize, Vec<&OverlayPair>> = BTreeMap::new();
    for pair in pairs {
        groups.entry(pair.base).or_default().push(pair);
    }
    let mut on_disk = DirIndex::new();
    for (base, group) in groups {
        let Some(b) = items.get(base) else { continue };
        let Some(base_target) = b.target.clone().filter(|_| b.moves_in()) else {
            continue;
        };
        let (system, base_rename, base_folder) =
            (b.system_id.clone(), b.rename.clone(), b.folder.clone());
        let dir = parent(&base_target);
        let base_name = file_name(&base_target);
        let following: Vec<(usize, String)> = group
            .iter()
            .filter(|p| {
                items
                    .get(p.sibling)
                    .is_some_and(|s| s.moves_in() && s.system_id == system)
            })
            .map(|p| (p.sibling, format!("{base_name}{}", p.suffix)))
            .collect();
        if following.is_empty() {
            continue;
        }
        let taken = |sibling: usize, name: &str, on_disk: &mut DirIndex| {
            let key = name_key(name);
            name_too_long(name)
                || on_disk.taken(&layout.target_root.join(&dir), name)
                || following
                    .iter()
                    .any(|(other, n)| *other != sibling && name_key(n) == key)
                || items.iter().enumerate().any(|(pos, other)| {
                    pos != sibling
                        && other
                            .target
                            .as_ref()
                            .is_some_and(|t| parent(t) == dir && name_key(&file_name(t)) == key)
                })
        };
        if following
            .iter()
            .any(|(sibling, name)| taken(*sibling, name, &mut on_disk))
        {
            items[base].leave(LeftReason::CollisionSet);
            for (sibling, _) in &following {
                items[*sibling].leave(LeftReason::CollisionSet);
            }
            continue;
        }
        for (sibling, name) in following {
            let item = &mut items[sibling];
            let original = file_name(&item.sources[0]);
            let suffix = name[base_name.len()..].to_string();
            item.rename = match &base_rename {
                Some(RenameReason::Collision { name }) => Some(RenameReason::Collision {
                    name: format!("{name}{suffix}"),
                }),
                Some(RenameReason::SafeName { .. }) => {
                    Some(RenameReason::SafeName { from: original })
                }
                _ => None,
            };
            let folder = dir.join(&name);
            // A folder extracted out of an archive takes its entries along.
            if let (Some(before), Transfer::Extract { entries, .. }) =
                (item.target.clone(), &mut item.transfer)
            {
                for e in entries.iter_mut() {
                    if let Ok(rel) = e.target.strip_prefix(&before) {
                        e.target = folder.join(rel);
                    }
                }
            }
            item.target = Some(folder);
            item.folder = base_folder.clone();
            // Only the base is listed by the frontend: the sibling's own
            // suffix note no longer applies.
            item.notes
                .retain(|n| !matches!(n, ItemNote::FolderSuffix { .. }));
        }
    }
}

// --- Game folders (game folders spec D5, D6, D20) ---

/// Systems whose games get a folder when the option is on (D5, D14):
/// the ones with updates, DLC or demos by header. Retro systems keep
/// today's placement (§13 Q8).
pub const GAME_FOLDER_SYSTEMS: &[&str] = &[
    "3ds",
    "switch",
    "wiiu",
    "wii",
    "ps3",
    "psvita",
    "psp",
    "ps4",
    "ps5",
    "xbox360",
    "xboxone",
    "xboxseries",
];

/// `system` is one of [`GAME_FOLDER_SYSTEMS`].
pub fn is_game_folder_system(system: &str) -> bool {
    GAME_FOLDER_SYSTEMS.contains(&system)
}

/// `<Game>` in `system_dir` (absolute) and whether that folder is already
/// on disk (D5, D6, D10, D20): `stem` (the base's final stem) plus
/// ` (<Region>)` in No-Intro words when the stem has no region tag, NFC,
/// FAT-safe (reserved names, forbidden characters, trailing dots and
/// spaces). An existing real folder of that name in any case or Unicode
/// form is reused under its own spelling; a name an earlier item claimed,
/// or a file or link in its place, takes the next free ` (n)`. The name
/// is claimed in `index`. `None` when no safe name is left.
pub fn game_folder_name(
    stem: &str,
    region: Option<Region>,
    system_dir: &Path,
    index: &mut DirIndex,
) -> Option<(String, bool)> {
    let stem = stem.trim();
    let wanted = match region {
        Some(region) if parse_tags(stem).regions.is_empty() => {
            format!("{stem} ({})", region.no_intro_word())
        }
        _ => stem.to_string(),
    };
    let wanted: String = wanted.nfc().collect();
    let safe = safe_file_name(&wanted).ok()?;
    for n in 1u32..=u32::from(u16::MAX) {
        let name = if n == 1 {
            safe.clone()
        } else {
            format!("{safe} ({n})")
        };
        if name_too_long(&name) {
            return None;
        }
        if let Some(found) = game_folder(system_dir, &name, index) {
            let exists = found.symlink_metadata().is_ok();
            let name = file_name(&found);
            index.claim(system_dir, &name);
            return Some((name, exists));
        }
    }
    None
}

/// How a child's file name is built (D6): `<Game> (v<version>)`,
/// `<Game> - <DLC name>`, or a folder child's own name.
enum ChildName {
    Update { game: String, version: String },
    Dlc { game: String, name: String },
    Own { name: String },
}

impl ChildName {
    /// The name with collision number `n` (1 = none) and `ext`, the
    /// `<DLC name>` shortened first, then the `<Game>` prefix (never the
    /// version, never the folder), until it fits 255 bytes and, with
    /// `windows_paths`, `dir` joined with it fits 259 UTF-16 units.
    fn fit(&self, n: u32, ext: Option<&str>, dir: &Path, windows_paths: bool) -> Option<String> {
        let suffix = if n > 1 {
            format!(" ({n})")
        } else {
            String::new()
        };
        let dot_ext = ext.map(|e| format!(".{e}")).unwrap_or_default();
        let (mut game, mut part) = match self {
            ChildName::Update { game, version } => (game.clone(), version.clone()),
            ChildName::Dlc { game, name } => (game.clone(), name.clone()),
            ChildName::Own { name } => (String::new(), name.clone()),
        };
        let fits = |name: &str| {
            !name_too_long(name)
                && !(windows_paths
                    && dir.join(name).to_string_lossy().encode_utf16().count() > WINDOWS_MAX_PATH)
        };
        let shorten = |s: &mut String| {
            if s.chars().count() <= 1 {
                return false;
            }
            s.pop();
            while s.chars().count() > 1 && (s.ends_with(' ') || s.ends_with('.')) {
                s.pop();
            }
            true
        };
        loop {
            let name = match self {
                ChildName::Update { .. } => format!("{game} ({part}){suffix}{dot_ext}"),
                ChildName::Dlc { .. } => format!("{game} - {part}{suffix}{dot_ext}"),
                ChildName::Own { .. } => format!("{part}{suffix}{dot_ext}"),
            };
            if fits(&name) {
                return Some(name);
            }
            let shortened = match self {
                ChildName::Update { .. } => shorten(&mut game),
                ChildName::Dlc { .. } => shorten(&mut part) || shorten(&mut game),
                ChildName::Own { .. } => shorten(&mut part),
            };
            if !shortened {
                return None;
            }
        }
    }
}

/// Untrusted header text as one path component (D6): cleaned like a
/// header title, then FAT-safe (separators, `..`, controls, reserved
/// names); `None` when nothing is left.
fn safe_component(raw: &str) -> Option<String> {
    let clean = crate::consoles::clean_title(raw)?;
    safe_file_name(&clean).ok()
}

/// The game folder (relative to the target root) a child of `base` goes
/// into and whether it was on disk: a moving folder game is its own
/// folder; a moving base file or set its planned `game.folder`; a base
/// already present in its game folder that folder.
fn base_game_folder(base: &OrganizeItem) -> Option<(PathBuf, bool)> {
    match &base.action {
        OrganizeAction::Auto | OrganizeAction::Review if base.kind == ItemKind::Folder => {
            base.target.clone().map(|t| (t, false))
        }
        OrganizeAction::Auto | OrganizeAction::Review => base
            .game
            .as_ref()
            .and_then(|g| Some((g.folder.clone()?, g.folder_exists))),
        OrganizeAction::Left {
            reason: LeftReason::AlreadyPresent { at },
        } => base
            .game
            .as_ref()
            .filter(|g| g.folder.is_some())
            .map(|_| (parent(at), true)),
        _ => None,
    }
}

/// After [`assign_targets_in`] (game folders spec D5, D6, D20, plan
/// P-3): every update or DLC whose `game.of` base was planned into a game
/// folder (or, with no `of`, an orphan whose `game.folder` the library
/// lookup chose, D13) gets `<folder>/Updates/<Game> (v<version>).<ext>` or
/// `<folder>/DLC/<Game> - <DLC name>.<ext>` (a folder child keeps its own
/// name below `Updates/` or `DLC/`), [`RenameReason::GameFolder`] when
/// the name changed, and the base's `folder` and `folder_exists`; so a
/// ` (2)` on the base's folder carries over. A base left (any reason but
/// *already present*) leaves its children with the same reason (§12).
/// Fixed folder spelling: an existing case variant is reused (the first
/// by bytes); a file or link named like it leaves the child
/// `UnsafeTarget`. Inside it, the same content → *already present*; a
/// different file → ` (2)` and Review. `<DLC name>` comes from
/// `dlc_name(child)` (untrusted text, made safe here; `DLC` when nothing
/// is left). Names that do not fit lose their DLC part first, then their
/// `<Game>` prefix; still too long → `PathTooLong`.
pub fn game_children_targets(
    items: &mut [OrganizeItem],
    layout: &TargetLayout,
    index: &mut DirIndex,
    same_content: &dyn Fn(&Path, &OrganizeItem) -> bool,
    dlc_name: &dyn Fn(usize) -> Option<String>,
) {
    // A folder game is its own game folder.
    for item in items.iter_mut() {
        if item.kind != ItemKind::Folder || !item.moves_in() {
            continue;
        }
        let target = item.target.clone();
        if let Some(link) = item.game.as_mut().filter(|g| g.role == GameRole::Base) {
            link.folder = target;
        }
    }
    for pos in 0..items.len() {
        let Some(link) = items[pos].game.clone() else {
            continue;
        };
        let fixed = match link.role {
            GameRole::Update => FixedFolder::Updates,
            GameRole::Dlc => FixedFolder::Dlc,
            _ => continue,
        };
        if !items[pos].moves_in() || items[pos].target.is_some() {
            continue;
        }
        let (system_folder, folder, folder_exists) = match link.of {
            Some(b) if b == pos => continue,
            Some(b) => {
                let Some(base) = items.get(b) else { continue };
                let system_folder = base.folder.clone();
                let Some((folder, folder_exists)) = base_game_folder(base) else {
                    let reason = match &base.action {
                        OrganizeAction::Left { reason } => reason.clone(),
                        _ => LeftReason::Unknown,
                    };
                    items[pos].leave(reason);
                    continue;
                };
                (system_folder, folder, folder_exists)
            }
            // An orphan (D13): the game folder the lookup chose.
            None => match link.folder.clone() {
                Some(folder) => (items[pos].folder.clone(), folder, link.folder_exists),
                None => continue,
            },
        };
        if layout.fat32 && too_big_for_fat(&items[pos]) {
            items[pos].leave(LeftReason::TooBigForFat);
            continue;
        }
        let folder_abs = layout.target_root.join(&folder);
        let fixed_dir = match index.existing_first(&folder_abs, fixed.dir_name()) {
            Some(existing) => {
                let real_dir = fs::symlink_metadata(&existing).is_ok_and(|m| m.is_dir());
                if !real_dir {
                    items[pos].leave(LeftReason::UnsafeTarget);
                    continue;
                }
                existing
            }
            None => folder_abs.join(fixed.dir_name()),
        };
        let item = &items[pos];
        let original = file_name(&item.sources[0]);
        let game = file_name(&folder);
        let (shape, ext) = if item.kind == ItemKind::Folder {
            let name = safe_file_name(&original).unwrap_or_else(|_| original.clone());
            (ChildName::Own { name }, None)
        } else {
            let ext = split_name(&original).1.map(str::to_string);
            let shape = match fixed {
                FixedFolder::Updates => ChildName::Update {
                    game,
                    version: link
                        .version
                        .as_deref()
                        .and_then(safe_component)
                        .map_or_else(|| "Update".to_string(), |v| format!("v{v}")),
                },
                _ => ChildName::Dlc {
                    game,
                    name: dlc_name(pos)
                        .as_deref()
                        .and_then(safe_component)
                        .unwrap_or_else(|| "DLC".to_string()),
                },
            };
            (shape, ext)
        };
        let mut review = false;
        let mut outcome: Result<String, LeftReason> = Err(LeftReason::PathTooLong);
        for n in 1u32..=u32::from(u16::MAX) {
            let Some(name) = shape.fit(n, ext.as_deref(), &fixed_dir, layout.windows_paths) else {
                outcome = Err(LeftReason::PathTooLong);
                break;
            };
            if !index.taken(&fixed_dir, &name) {
                outcome = Ok(name);
                break;
            }
            if let Some(existing) = index.existing(&fixed_dir, &name) {
                if same_content(&existing, item) {
                    outcome = Err(LeftReason::AlreadyPresent {
                        at: relative(&layout.target_root, &existing),
                    });
                    break;
                }
                review = true;
            }
        }
        let name = match outcome {
            Ok(name) => name,
            Err(reason) => {
                items[pos].leave(reason);
                continue;
            }
        };
        index.claim(&folder_abs, &file_name(&fixed_dir));
        index.claim(&fixed_dir, &name);
        let item = &mut items[pos];
        item.target = Some(relative(&layout.target_root, &fixed_dir.join(&name)));
        item.set_targets.clear();
        item.companions.clear();
        item.playlist = None;
        item.folder = system_folder;
        item.rename = (name != original).then_some(RenameReason::GameFolder { from: original });
        if review && item.action == OrganizeAction::Auto {
            item.action = OrganizeAction::Review;
        }
        if let Some(link) = item.game.as_mut() {
            link.folder = Some(folder);
            link.folder_exists = folder_exists;
        }
    }
}

/// After the target gates (D27 trees, FAT folder slots): a base that a
/// gate left (any reason but *already present*) leaves its updates and
/// DLC with the same reason and no longer names a game folder (§12), so a
/// game folder never holds an update without its base by surprise.
pub fn game_children_follow(items: &mut [OrganizeItem]) {
    let mut follow: Vec<(usize, LeftReason)> = Vec::new();
    for (pos, item) in items.iter().enumerate() {
        let Some(b) = item
            .game
            .as_ref()
            .filter(|g| matches!(g.role, GameRole::Update | GameRole::Dlc))
            .and_then(|g| g.of)
            .filter(|&b| b != pos)
        else {
            continue;
        };
        if !item.moves_in() {
            continue;
        }
        match items.get(b).map(|base| &base.action) {
            Some(OrganizeAction::Left {
                reason: LeftReason::AlreadyPresent { .. },
            }) => {}
            Some(OrganizeAction::Left { reason }) => follow.push((pos, reason.clone())),
            Some(OrganizeAction::SetAside { .. }) => follow.push((pos, LeftReason::Unknown)),
            _ => {}
        }
    }
    for (pos, reason) in follow {
        items[pos].leave(reason);
    }
    for item in items.iter_mut() {
        let present = matches!(
            item.action,
            OrganizeAction::Left {
                reason: LeftReason::AlreadyPresent { .. }
            }
        );
        if item.moves_in() || present {
            continue;
        }
        if let Some(link) = item.game.as_mut() {
            link.folder = None;
            link.folder_exists = false;
        }
    }
}

/// A disc set keeps its member names and layout; any collision leaves the
/// whole set (the cue text names its tracks).
fn plan_set(
    item: &OrganizeItem,
    dir: &Path,
    comps: &[&Comp],
    index: &mut DirIndex,
    same_content: &dyn Fn(&Path, &OrganizeItem) -> bool,
) -> Planned {
    let base = parent(&item.sources[0]);
    let targets: Vec<PathBuf> = item
        .sources
        .iter()
        .map(|source| match source.strip_prefix(&base) {
            Ok(inner) if !inner.as_os_str().is_empty() => dir.join(inner),
            _ => dir.join(file_name(source)),
        })
        .collect();
    let mut claims: Vec<(PathBuf, String)> = Vec::new();
    let mut keys: HashSet<(String, String)> = HashSet::new();
    let mut place = |index: &mut DirIndex, path: &Path| -> bool {
        let folder = path.parent().unwrap_or(dir).to_path_buf();
        let name = file_name(path);
        let key = (name_key(&folder.to_string_lossy()), name_key(&name));
        if !keys.insert(key) || index.taken(&folder, &name) {
            return false;
        }
        claims.push((folder, name));
        true
    };

    let primary = &targets[0];
    if !place(index, primary) {
        let folder = primary.parent().unwrap_or(dir);
        let present = index
            .existing(folder, &file_name(primary))
            .filter(|existing| same_content(existing, item));
        return Planned::Left(match present {
            Some(at) => LeftReason::AlreadyPresent { at },
            None => LeftReason::CollisionSet,
        });
    }
    for member in &targets[1..] {
        if !place(index, member) {
            return Planned::Left(LeftReason::CollisionSet);
        }
    }
    let mut companions = Vec::with_capacity(comps.len());
    for comp in comps {
        let target = dir.join(&comp.file_name);
        if !place(index, &target) {
            return Planned::Left(LeftReason::CollisionSet);
        }
        companions.push((comp.rel.clone(), target));
    }
    Planned::Placed(Placed {
        target: targets[0].clone(),
        set_targets: targets[1..].to_vec(),
        rename: None,
        companions,
        claims,
        entry_targets: Vec::new(),
        m3u: None,
        playlist_discs: Vec::new(),
    })
}

/// Lays out the entries of an `Extract` item in `dir`, its system folder
/// (archives spec §8.2, §8.3), checking names, lengths and collisions
/// against what is on disk and what earlier items reserved in `index`:
///
/// - [`ExtractShape::Roms`]: one ROM is named like a loose file (FAT-safe,
///   catalogue name, ` (n)` on a collision, *already present* when the
///   same bytes are there); several keep their base names and any
///   collision leaves the item (`CollisionSet`).
/// - [`ExtractShape::Disc`]: paths are kept relative to the members'
///   common folder in the archive, which is dropped in a flat layout and
///   replaced by `<stem>/` in a game-folder one; member names are never
///   changed (sheets reference them), so a name the target cannot store
///   is `SetUnsafeName`, a path over the limits `PathTooLong`, and any
///   collision `CollisionSet`. A multi-disc game gets a `<title>.m3u`
///   when the profile asks for one and the archive holds none.
///
/// Every final path is checked against 255 bytes and UTF-16 units per
/// component and, with `layout.windows_paths`, 259 units for both the
/// final path and the temp file written beside it.
fn assign_extract_targets(
    item: &OrganizeItem,
    shape: &ExtractShape,
    dir: &Path,
    layout: &TargetLayout,
    comps: &[&Comp],
    index: &mut DirIndex,
) -> Planned {
    let Transfer::Extract { entries, .. } = &item.transfer else {
        return Planned::Left(LeftReason::Unknown);
    };
    if entries.is_empty() {
        return Planned::Left(LeftReason::Unknown);
    }
    match shape {
        ExtractShape::Folder { .. } | ExtractShape::LooseDiscs { .. } => {
            Planned::Left(LeftReason::Unknown)
        }
        ExtractShape::Roms if entries.len() == 1 => {
            extract_one_rom(item, &entries[0], dir, layout, comps, index)
        }
        ExtractShape::Roms => {
            let mut targets = Vec::with_capacity(entries.len());
            for e in entries {
                // No safe spelling at all (only dots and spaces): the OS
                // would store another name than the one planned.
                let Ok(safe) = safe_file_name(base_name(&e.name)) else {
                    return Planned::Left(LeftReason::SetUnsafeName {
                        name: e.name.clone(),
                    });
                };
                targets.push(dir.join(safe));
            }
            place_extracted(targets, None, dir, dir, layout, comps, index)
        }
        ExtractShape::Disc {
            discs,
            title,
            multi_disc,
        } => extract_disc(
            entries,
            (discs, title, *multi_disc),
            dir,
            layout,
            comps,
            index,
        ),
    }
}

/// One cartridge ROM out of an archive: named like a loose file.
fn extract_one_rom(
    item: &OrganizeItem,
    entry: &ExtractEntry,
    dir: &Path,
    layout: &TargetLayout,
    comps: &[&Comp],
    index: &mut DirIndex,
) -> Planned {
    let original = base_name(&entry.name).to_string();
    let archive = file_name(&item.sources[0]);
    let companion_stem = split_name(&archive).0.to_string();
    let same = |existing: &Path, _: &OrganizeItem| same_as_entry(existing, entry);
    let mut planned = plan_single(
        item,
        (&original, &companion_stem),
        None,
        dir,
        layout,
        comps,
        index,
        &same,
    );
    if let Planned::Placed(placed) = &mut planned {
        if let Some(reason) = check_lengths(dir, &placed.target, layout.windows_paths) {
            return Planned::Left(reason);
        }
        placed.entry_targets = vec![placed.target.clone()];
    }
    planned
}

/// A disc set out of an archive, laid out per the profile.
fn extract_disc(
    entries: &[ExtractEntry],
    (discs, title, multi_disc): (&[String], &str, bool),
    dir: &Path,
    layout: &TargetLayout,
    comps: &[&Comp],
    index: &mut DirIndex,
) -> Planned {
    let disc_layout = layout.profile.disc_layout;
    let first = discs
        .first()
        .map(String::as_str)
        .unwrap_or(entries[0].name.as_str());
    // `<stem>`: the primary sheet's stem, a multi-disc game's title.
    let stem = if multi_disc && !title.trim().is_empty() {
        title.trim().to_string()
    } else {
        split_name(base_name(first)).0.to_string()
    };
    // The folder and the playlist are no set members (no sheet names
    // them), so a title the card cannot store as it is gets the safe
    // spelling (`Vol.` → `Vol`, `CON` → `_CON`); members keep theirs.
    let safe_stem = || safe_file_name(&stem).ok();
    let unsafe_stem = || Planned::Left(LeftReason::SetUnsafeName { name: stem.clone() });
    let folder = if disc_layout.is_game_folder() {
        let Some(safe) = safe_stem() else {
            return unsafe_stem();
        };
        match game_folder(dir, &safe, index) {
            Some(folder) => folder,
            None => return Planned::Left(LeftReason::CollisionSet),
        }
    } else {
        dir.to_path_buf()
    };
    let common = common_folder(entries.iter().map(|e| e.name.as_str()));
    let mut targets = Vec::with_capacity(entries.len());
    for e in entries {
        let inner = e.name[common.len()..].trim_start_matches('/');
        let mut target = folder.clone();
        for part in inner.split('/').filter(|p| !p.is_empty()) {
            if name_too_long(part) {
                return Planned::Left(LeftReason::PathTooLong);
            }
            if safe_file_name(part).ok().as_deref() != Some(part) {
                return Planned::Left(LeftReason::SetUnsafeName {
                    name: e.name.clone(),
                });
            }
            target.push(part);
        }
        targets.push(target);
    }
    let has_playlist = entries.iter().any(|e| {
        split_name(base_name(&e.name))
            .1
            .is_some_and(|x| x.eq_ignore_ascii_case("m3u"))
    });
    let m3u = if multi_disc && disc_layout.m3u_for_multi() && !has_playlist {
        match safe_stem() {
            Some(safe) => Some(folder.join(format!("{safe}.m3u"))),
            None => return unsafe_stem(),
        }
    } else {
        None
    };
    if let Some(m3u) = &m3u {
        let name = file_name(m3u);
        if safe_file_name(&name).ok().as_deref() != Some(name.as_str()) {
            return Planned::Left(LeftReason::SetUnsafeName { name });
        }
    }
    // The playlist's sheet or image first: what the dry run shows.
    let primary = entries.iter().position(|e| e.name == first).unwrap_or(0);
    place_extracted(targets, m3u, dir, &folder, layout, comps, index).map_placed(|p| {
        p.target = p.entry_targets[primary].clone();
        // The game folder's own name, so no later item (another archive
        // whose stem differs only in case) shares it.
        if folder != dir {
            p.claims.push((dir.to_path_buf(), file_name(&folder)));
        }
    })
}

/// Loose discs of one game (adjustments spec D24), placed as
/// [`extract_disc`] places an extracted multi-disc archive: in `dir`
/// (flat) or in `<title>/` (game folder), every member under its own name
/// relative to the members' common folder, any collision leaving the whole
/// set (`CollisionSet`). With `playlist` and a profile that asks for one,
/// `<title>.m3u` is planned unless a file of that name is already there or
/// claimed: that one is kept and the set gets none (no-clobber).
fn place_loose_discs(
    item: &OrganizeItem,
    (discs, title, playlist): (&[PathBuf], &str, bool),
    dir: &Path,
    layout: &TargetLayout,
    comps: &[&Comp],
    index: &mut DirIndex,
) -> Planned {
    let disc_layout = layout.profile.disc_layout;
    let title = title.trim();
    let safe = safe_file_name(title).ok().filter(|t| !t.is_empty());
    let unsafe_title = || {
        Planned::Left(LeftReason::SetUnsafeName {
            name: title.to_string(),
        })
    };
    let folder = if disc_layout.is_game_folder() {
        let Some(safe) = &safe else {
            return unsafe_title();
        };
        match game_folder(dir, safe, index) {
            Some(folder) => folder,
            None => return Planned::Left(LeftReason::CollisionSet),
        }
    } else {
        dir.to_path_buf()
    };
    let base = common_parent(&item.sources);
    let mut targets = Vec::with_capacity(item.sources.len());
    for source in &item.sources {
        let inner = source.strip_prefix(&base).unwrap_or(source);
        let mut target = folder.clone();
        for part in inner.components() {
            let part = part.as_os_str().to_string_lossy();
            if name_too_long(&part) {
                return Planned::Left(LeftReason::PathTooLong);
            }
            if safe_file_name(&part).ok().as_deref() != Some(&*part) {
                return Planned::Left(LeftReason::SetUnsafeName {
                    name: source.to_string_lossy().into_owned(),
                });
            }
            target.push(&*part);
        }
        targets.push(target);
    }
    let playlist_discs: Vec<PathBuf> = discs
        .iter()
        .filter_map(|d| item.sources.iter().position(|s| s == d))
        .map(|i| targets[i].clone())
        .collect();
    let m3u = match (&safe, playlist && disc_layout.m3u_for_multi()) {
        (Some(safe), true) => {
            let name = format!("{safe}.m3u");
            let kept = index.existing(&folder, &name).is_some() || index.claimed(&folder, &name);
            (!kept).then(|| folder.join(name))
        }
        (None, true) => return unsafe_title(),
        _ => None,
    };
    place_extracted(targets, m3u, dir, &folder, layout, comps, index).map_placed(|p| {
        p.set_targets = p.entry_targets.split_off(1);
        p.target = p.entry_targets.remove(0);
        p.playlist_discs = playlist_discs;
        if folder != dir {
            p.claims.push((dir.to_path_buf(), file_name(&folder)));
        }
    })
}

/// The deepest folder every path of `paths` is below (relative paths).
fn common_parent(paths: &[PathBuf]) -> PathBuf {
    let mut common: Vec<std::path::Component> = match paths.first().and_then(|p| p.parent()) {
        Some(parent) => parent.components().collect(),
        None => return PathBuf::new(),
    };
    for path in &paths[1..] {
        let parent: Vec<std::path::Component> = path
            .parent()
            .map(|p| p.components().collect())
            .unwrap_or_default();
        let same = common
            .iter()
            .zip(&parent)
            .take_while(|(a, b)| a == b)
            .count();
        common.truncate(same);
    }
    common.iter().collect()
}

/// The folder `stem` in `dir` a game-folder extraction writes into: an
/// existing real folder of that name in any case is reused under its own
/// spelling; one an earlier item claimed, or a file or link in its place,
/// is a collision (`None`).
fn game_folder(dir: &Path, stem: &str, index: &mut DirIndex) -> Option<PathBuf> {
    if index.claimed(dir, stem) {
        return None;
    }
    match index.existing(dir, stem) {
        Some(existing) => fs::symlink_metadata(&existing)
            .is_ok_and(|m| m.is_dir())
            .then_some(existing),
        None => Some(dir.join(stem)),
    }
}

impl Planned {
    fn map_placed(mut self, f: impl FnOnce(&mut Placed)) -> Planned {
        if let Planned::Placed(p) = &mut self {
            f(p);
        }
        self
    }
}

/// Reserves `targets` (one per extracted entry), the playlist and the
/// companions (into `folder`, names kept): lengths first, then any name
/// taken on disk, by an earlier item or twice within the item →
/// `CollisionSet`.
#[allow(clippy::too_many_arguments)]
fn place_extracted(
    targets: Vec<PathBuf>,
    m3u: Option<PathBuf>,
    dir: &Path,
    folder: &Path,
    layout: &TargetLayout,
    comps: &[&Comp],
    index: &mut DirIndex,
) -> Planned {
    let companions: Vec<(PathBuf, PathBuf)> = comps
        .iter()
        .map(|c| (c.rel.clone(), folder.join(&c.file_name)))
        .collect();
    let all: Vec<&PathBuf> = targets
        .iter()
        .chain(&m3u)
        .chain(companions.iter().map(|(_, t)| t))
        .collect();
    for path in &all {
        if let Some(reason) = check_lengths(dir, path, layout.windows_paths) {
            return Planned::Left(reason);
        }
    }
    let mut claims: Vec<(PathBuf, String)> = Vec::with_capacity(all.len());
    let mut keys: HashSet<(String, String)> = HashSet::new();
    for path in all {
        let parent = path.parent().unwrap_or(dir).to_path_buf();
        let name = file_name(path);
        let key = (name_key(&parent.to_string_lossy()), name_key(&name));
        if !keys.insert(key) || index.taken(&parent, &name) {
            return Planned::Left(LeftReason::CollisionSet);
        }
        claims.push((parent, name));
    }
    Planned::Placed(Placed {
        target: targets[0].clone(),
        set_targets: Vec::new(),
        rename: None,
        companions,
        claims,
        entry_targets: targets,
        m3u,
        playlist_discs: Vec::new(),
    })
}

/// Why `path` (below the system folder `dir`) cannot be written as it is:
/// a component over 255 bytes or UTF-16 units, more than
/// [`MAX_EXTRACT_DEPTH`] components below `dir`, or, with
/// `windows_paths`, the path or the temp file beside it over 259 units.
fn check_lengths(dir: &Path, path: &Path, windows_paths: bool) -> Option<LeftReason> {
    let below = path.strip_prefix(dir).unwrap_or(path);
    let parts: Vec<String> = below
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    if parts.len() > MAX_EXTRACT_DEPTH || parts.iter().any(|p| name_too_long(p)) {
        return Some(LeftReason::PathTooLong);
    }
    if windows_paths {
        let units = |p: &Path| p.to_string_lossy().encode_utf16().count();
        let parent = path.parent().unwrap_or(dir);
        let temp = units(parent) + 1 + TEMP_NAME_MAX;
        if units(path).max(temp) > WINDOWS_MAX_PATH {
            return Some(LeftReason::PathTooLong);
        }
    }
    None
}

/// A file name over 255 UTF-8 bytes (APFS, ext4) or 255 UTF-16 units
/// (FAT, NTFS).
fn name_too_long(name: &str) -> bool {
    name.len() > MAX_NAME_BYTES || name.encode_utf16().count() > MAX_NAME_UNITS
}

/// The last component of an archive entry name (`/`-separated).
fn base_name(name: &str) -> &str {
    name.rsplit('/').next().unwrap_or(name)
}

/// The longest folder prefix (ending in `/`, or empty) every name shares.
fn common_folder<'a>(mut names: impl Iterator<Item = &'a str>) -> String {
    let Some(first) = names.next() else {
        return String::new();
    };
    let mut prefix: Vec<&str> = first.split('/').collect();
    prefix.pop();
    for name in names {
        let mut parts: Vec<&str> = name.split('/').collect();
        parts.pop();
        let same = prefix
            .iter()
            .zip(&parts)
            .take_while(|(a, b)| a == b)
            .count();
        prefix.truncate(same);
    }
    prefix.iter().map(|p| format!("{p}/")).collect()
}

/// Whether the file at `existing` holds the entry's bytes: same size and,
/// when the listing stores one, same CRC32.
fn same_as_entry(existing: &Path, entry: &ExtractEntry) -> bool {
    let Ok(meta) = fs::metadata(existing) else {
        return false;
    };
    let Some(crc) = entry.crc32 else {
        return false;
    };
    meta.len() == entry.size && file_crc32(existing) == Some(crc)
}

fn file_crc32(path: &Path) -> Option<u32> {
    use std::io::Read;
    let mut file = crate::consoles::open_rom(path).ok()?;
    let mut hasher = crc32fast::Hasher::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        match file.read(&mut buf) {
            Ok(0) => return Some(hasher.finalize()),
            Ok(n) => hasher.update(&buf[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return None,
        }
    }
}

#[cfg(feature = "test-util")]
thread_local! {
    static FAT32: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
    static WINDOWS_PATHS: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

/// Test hook: makes [`windows_paths`] on the calling thread answer
/// `on`. `None` clears it.
#[doc(hidden)]
#[cfg(feature = "test-util")]
pub fn set_windows_paths_override(on: Option<bool>) {
    WINDOWS_PATHS.with(|c| c.set(on));
}

/// Whether planned paths must fit Windows' 259 UTF-16 units: on Windows.
pub fn windows_paths() -> bool {
    #[cfg(feature = "test-util")]
    if let Some(on) = WINDOWS_PATHS.with(std::cell::Cell::get) {
        return on;
    }
    cfg!(windows)
}

/// Why a folder game placed at `target` (absolute) cannot be written
/// whole (spec D27): a member over 4 GiB on FAT32 (`fat32`), or, with
/// `windows_paths`, a member path past 259 UTF-16 units once below it.
pub fn folder_gate(
    target: &Path,
    tree: &crate::organize::walk::TreeSummary,
    fat32: bool,
    windows_paths: bool,
) -> Option<LeftReason> {
    if fat32 && tree.largest_file >= FAT32_LIMIT {
        return Some(LeftReason::TooBigForFat);
    }
    let units = target.to_string_lossy().encode_utf16().count();
    if windows_paths && units + 1 + tree.longest_rel_utf16 > WINDOWS_MAX_PATH {
        return Some(LeftReason::PathTooLong);
    }
    None
}

/// Test hook: makes [`file_system_is_fat32`] on the calling thread answer
/// `fat32`. `None` clears it.
#[doc(hidden)]
#[cfg(feature = "test-util")]
pub fn set_fat32_override(fat32: Option<bool>) {
    FAT32.with(|c| c.set(fat32));
}

/// Directory slots a FAT32 folder may use once a plan is applied: a
/// folder holds at most 65,536 32-byte slots; the rest is headroom
/// (archives spec §8.2).
pub const FAT_DIR_SLOTS_MAX: u64 = 65_000;

/// The 32-byte directory slots `name` takes on FAT32: its long-name
/// slots (13 UTF-16 units each) plus the short-name slot.
pub fn fat_name_slots(name: &str) -> u64 {
    (name.encode_utf16().count() as u64).div_ceil(13) + 1
}

/// The directory slots `dir` uses now: `.` and `..` plus every entry's
/// name. A folder that does not exist (or cannot be listed) is counted
/// as a new, empty one.
pub fn fat_dir_slots(dir: &Path) -> u64 {
    let names: u64 = match fs::read_dir(dir) {
        Ok(entries) => entries
            .flatten()
            .map(|e| fat_name_slots(&e.file_name().to_string_lossy()))
            .sum(),
        Err(_) => 0,
    };
    2 + names
}

/// Every path a placed item writes, absolute: its files (or extracted
/// entries and playlist) and its companions.
fn written_paths(item: &OrganizeItem, target_root: &Path) -> Vec<PathBuf> {
    let mut rel: Vec<&PathBuf> = match &item.transfer {
        Transfer::Extract { entries, m3u, .. } => {
            entries.iter().map(|e| &e.target).chain(m3u).collect()
        }
        Transfer::AsIs => item
            .target
            .iter()
            .chain(&item.set_targets)
            .chain(item.playlist.as_ref().map(|p| &p.path))
            .collect(),
    };
    rel.extend(item.companions.iter().map(|c| &c.target));
    rel.into_iter().map(|p| target_root.join(p)).collect()
}

/// On a FAT32 target, leaves with [`LeftReason::FolderFull`] every placed
/// item, in plan order, that would take a folder past
/// [`FAT_DIR_SLOTS_MAX`] slots: the folder's existing entries plus every
/// file and new sub-folder planned before it (archives spec §8.2).
pub fn limit_fat_folders(items: &mut [OrganizeItem], target_root: &Path) {
    let mut used: HashMap<PathBuf, u64> = HashMap::new();
    let mut created: HashSet<PathBuf> = HashSet::new();
    for item in items.iter_mut().filter(|i| i.moves_in()) {
        let paths = written_paths(item, target_root);
        if paths.is_empty() {
            continue;
        }
        let mut charge: BTreeMap<PathBuf, u64> = BTreeMap::new();
        let mut new_dirs: Vec<PathBuf> = Vec::new();
        for path in &paths {
            let Some(parent) = path.parent() else {
                continue;
            };
            *charge.entry(parent.to_path_buf()).or_default() += fat_name_slots(&file_name(path));
            // Folders the item creates take a slot set in their parent.
            let mut dir = parent;
            while dir != target_root
                && dir.starts_with(target_root)
                && !created.contains(dir)
                && !new_dirs.iter().any(|d| d == dir)
                && !dir.is_dir()
            {
                let Some(up) = dir.parent() else { break };
                new_dirs.push(dir.to_path_buf());
                *charge.entry(up.to_path_buf()).or_default() += fat_name_slots(&file_name(dir));
                dir = up;
            }
        }
        let full = charge.iter().find(|(dir, add)| {
            let now = *used
                .entry((*dir).clone())
                .or_insert_with(|| fat_dir_slots(dir));
            now + **add > FAT_DIR_SLOTS_MAX
        });
        if let Some((dir, _)) = full {
            let folder = file_name(dir);
            item.leave(LeftReason::FolderFull { folder });
            continue;
        }
        for (dir, add) in charge {
            *used.entry(dir).or_insert(2) += add;
        }
        created.extend(new_dirs);
    }
}

/// Whether `path` (or its nearest existing ancestor) is on FAT32: macOS
/// `statfs` type `msdos`, Linux the longest matching mount of type `vfat`,
/// Windows `GetVolumeInformationW` name `FAT32`. Any error → `false`.
pub fn file_system_is_fat32(path: &Path) -> bool {
    #[cfg(feature = "test-util")]
    if let Some(fat32) = FAT32.with(std::cell::Cell::get) {
        return fat32;
    }
    match path.ancestors().find(|p| p.exists()) {
        Some(existing) => fs_is_fat32(existing),
        None => false,
    }
}

#[cfg(target_os = "macos")]
fn fs_is_fat32(path: &Path) -> bool {
    use std::ffi::{CStr, CString};
    use std::os::unix::ffi::OsStrExt;

    let Ok(c_path) = CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    let mut stat = std::mem::MaybeUninit::<libc::statfs>::zeroed();
    // SAFETY: `c_path` is a valid nul-terminated string and `stat` points
    // to writable memory of the right size; it is read only on success.
    let ok = unsafe { libc::statfs(c_path.as_ptr(), stat.as_mut_ptr()) } == 0;
    if !ok {
        return false;
    }
    // SAFETY: `statfs` succeeded and filled the struct.
    let stat = unsafe { stat.assume_init() };
    // SAFETY: the kernel nul-terminates `f_fstypename` within its bounds.
    let kind = unsafe { CStr::from_ptr(stat.f_fstypename.as_ptr()) };
    kind.to_bytes() == b"msdos"
}

#[cfg(target_os = "linux")]
fn fs_is_fat32(path: &Path) -> bool {
    let Ok(path) = path.canonicalize() else {
        return false;
    };
    let Ok(mountinfo) = fs::read_to_string("/proc/self/mountinfo") else {
        return false;
    };
    mount_fs_type(&mountinfo, &path).is_some_and(|t| t == "vfat")
}

/// The file system type of the longest mount point containing `path` in
/// a `/proc/self/mountinfo` text.
#[cfg(any(target_os = "linux", test))]
fn mount_fs_type(mountinfo: &str, path: &Path) -> Option<String> {
    let unescape = |s: &str| {
        s.replace("\\040", " ")
            .replace("\\011", "\t")
            .replace("\\012", "\n")
            .replace("\\134", "\\")
    };
    let mut best: Option<(usize, String)> = None;
    for line in mountinfo.lines() {
        let Some((left, right)) = line.split_once(" - ") else {
            continue;
        };
        let (Some(mount), Some(kind)) = (left.split(' ').nth(4), right.split(' ').next()) else {
            continue;
        };
        let mount = PathBuf::from(unescape(mount));
        if !path.starts_with(&mount) {
            continue;
        }
        let depth = mount.components().count();
        if best.as_ref().is_none_or(|(d, _)| depth >= *d) {
            best = Some((depth, kind.to_string()));
        }
    }
    best.map(|(_, kind)| kind)
}

#[cfg(windows)]
fn fs_is_fat32(path: &Path) -> bool {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{GetVolumeInformationW, GetVolumePathNameW};

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut root = [0u16; 1024];
    // SAFETY: `wide` is nul-terminated and `root` holds `len` u16s.
    let ok = unsafe { GetVolumePathNameW(wide.as_ptr(), root.as_mut_ptr(), root.len() as u32) };
    if ok == 0 {
        return false;
    }
    let mut fs_name = [0u16; 64];
    // SAFETY: `root` was nul-terminated by the call above; the name buffer
    // holds `len` u16s; the other out-pointers may be null.
    let ok = unsafe {
        GetVolumeInformationW(
            root.as_ptr(),
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            fs_name.as_mut_ptr(),
            fs_name.len() as u32,
        )
    };
    if ok == 0 {
        return false;
    }
    let end = fs_name
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(fs_name.len());
    String::from_utf16_lossy(&fs_name[..end]).eq_ignore_ascii_case("FAT32")
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
fn fs_is_fat32(_path: &Path) -> bool {
    false
}

/// `path` relative to `root` when inside it, else as is.
fn relative(root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(root).unwrap_or(path).to_path_buf()
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn parent(path: &Path) -> PathBuf {
    path.parent().map(Path::to_path_buf).unwrap_or_default()
}

/// `("Tetris", Some("gb"))` for `Tetris.gb`; a leading dot is not an
/// extension.
fn split_name(name: &str) -> (&str, Option<&str>) {
    match name.rfind('.') {
        Some(i) if i > 0 && i + 1 < name.len() => (&name[..i], Some(&name[i + 1..])),
        _ => (name, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout_with_dlc(template: &str) -> TargetLayout {
        let mut profile = crate::profiles::builtin_profiles()
            .into_iter()
            .find(|p| p.id == "batocera")
            .expect("the Batocera profile");
        profile
            .dlc_folder
            .insert("ps4".into(), template.to_string());
        TargetLayout {
            target_root: PathBuf::from("t"),
            roms_dir: PathBuf::from("t").join("roms"),
            profile,
            overrides: BTreeMap::new(),
            fat32: false,
            windows_paths: false,
            names: NameMode::Keep,
            companion_exts: Vec::new(),
            game_folders: false,
        }
    }

    #[test]
    fn dlc_target_fills_the_title_id_by_component() {
        let layout = layout_with_dlc("DLC/{title_id}");
        assert_eq!(
            dlc_target(&layout, "ps4", "CUSA00900"),
            Some(PathBuf::from("DLC").join("CUSA00900"))
        );
        assert_eq!(dlc_target(&layout, "ps5", "PPSA01284"), None);
        for bad in [
            "",
            "../x",
            "CUSA/00900",
            "CUSA 00900",
            "C:",
            &"A".repeat(33),
        ] {
            assert_eq!(dlc_target(&layout, "ps4", bad), None, "{bad:?}");
        }
        let layout = layout_with_dlc("../{title_id}");
        assert_eq!(dlc_target(&layout, "ps4", "CUSA00900"), None);
    }

    #[test]
    fn split_name_keeps_dot_files_whole() {
        assert_eq!(split_name("Tetris.gb"), ("Tetris", Some("gb")));
        assert_eq!(split_name("a.b.zip"), ("a.b", Some("zip")));
        assert_eq!(split_name(".hidden"), (".hidden", None));
        assert_eq!(split_name("noext"), ("noext", None));
    }

    #[test]
    fn companion_patterns_match_state_slots() {
        let pats = default_companion_exts();
        assert!(companion_ext_matches(&pats, "SAV"));
        assert!(companion_ext_matches(&pats, "state1"));
        assert!(companion_ext_matches(&pats, "state"));
        assert!(!companion_ext_matches(&pats, "gba"));
    }

    #[test]
    fn mountinfo_picks_the_longest_mount_point() {
        let info = "\
22 1 8:1 / / rw - ext4 /dev/sda1 rw
40 22 8:17 / /media/me/SD\\040CARD rw,nosuid - vfat /dev/sdb1 rw
41 22 8:18 / /media/me/other rw - exfat /dev/sdb2 rw
";
        assert_eq!(
            mount_fs_type(info, Path::new("/media/me/SD CARD/ROMS")).as_deref(),
            Some("vfat")
        );
        assert_eq!(
            mount_fs_type(info, Path::new("/media/me/other/x")).as_deref(),
            Some("exfat")
        );
        assert_eq!(
            mount_fs_type(info, Path::new("/home/me")).as_deref(),
            Some("ext4")
        );
    }

    /// A moving file item of the plan (3DS), source `name`.
    fn file_item(id: usize, name: &str) -> OrganizeItem {
        OrganizeItem {
            id,
            sources: vec![PathBuf::from(name)],
            kind: ItemKind::File,
            size: 10,
            fps: Vec::new(),
            target: None,
            set_targets: Vec::new(),
            system_id: Some("3ds".into()),
            folder: Some("3DS".into()),
            confidence: 1.0,
            method: None,
            why: Vec::new(),
            alternatives: Vec::new(),
            rename: None,
            notes: Vec::new(),
            companions: Vec::new(),
            catalogue_name: None,
            transfer: Transfer::AsIs,
            disc: None,
            title_id: None,
            content_id: None,
            playlist: None,
            title_source: None,
            game: None,
            action: OrganizeAction::Auto,
        }
    }

    fn game_layout(root: &Path, windows_paths: bool) -> TargetLayout {
        TargetLayout {
            target_root: root.to_path_buf(),
            roms_dir: root.to_path_buf(),
            profile: crate::profiles::builtin_profiles()
                .into_iter()
                .find(|p| p.id == "coverly-library")
                .expect("the library profile"),
            overrides: BTreeMap::new(),
            fat32: false,
            windows_paths,
            names: NameMode::Keep,
            companion_exts: Vec::new(),
            game_folders: true,
        }
    }

    /// A base placed in `3DS/<game>/` and one child of `role` per name.
    fn family(game: &str, role: GameRole, children: &[&str]) -> Vec<OrganizeItem> {
        let mut base = file_item(0, "base.cia");
        let folder = Path::new("3DS").join(game);
        base.target = Some(folder.join(format!("{game}.cia")));
        let mut link = crate::organize::plan::GameLink::new(GameRole::Base);
        link.folder = Some(folder);
        base.game = Some(link);
        let mut items = vec![base];
        for (i, name) in children.iter().enumerate() {
            let mut child = file_item(i + 1, name);
            let mut link = crate::organize::plan::GameLink::new(role);
            link.of = Some(0);
            link.version = Some("1.1.0".into());
            child.game = Some(link);
            items.push(child);
        }
        items
    }

    fn components(p: &Path) -> Vec<String> {
        p.components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect()
    }

    fn never_same(_: &Path, _: &OrganizeItem) -> bool {
        false
    }

    #[test]
    fn untrusted_dlc_names_stay_inside_the_dlc_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let layout = game_layout(tmp.path(), false);
        let mut items = family("Game", GameRole::Dlc, &["a.cia", "b.cia", "c.cia", "d.cia"]);
        let names = ["../../x", "CON", "Pack.", "\u{0}\u{1}"];
        game_children_targets(
            &mut items,
            &layout,
            &mut DirIndex::new(),
            &never_same,
            &|i| names.get(i - 1).map(|n| n.to_string()),
        );
        let got: Vec<Vec<String>> = items[1..]
            .iter()
            .map(|it| components(it.target.as_deref().expect("placed")))
            .collect();
        for parts in &got {
            assert_eq!(parts.len(), 4, "{parts:?}");
            assert_eq!(parts[..3], ["3DS", "Game", "DLC"]);
        }
        assert_eq!(got[0][3], "Game - .._.._x.cia");
        // A device name is made safe as a component of its own; the
        // trailing dot is trimmed; nothing printable left → `DLC`.
        assert_eq!(got[1][3], "Game - _CON.cia");
        assert_eq!(got[2][3], "Game - Pack.cia");
        assert_eq!(got[3][3], "Game - DLC.cia");
        assert!(matches!(
            items[1].rename,
            Some(RenameReason::GameFolder { .. })
        ));
    }

    #[test]
    fn a_device_name_or_trailing_dot_folder_gets_its_safe_form() {
        let tmp = tempfile::tempdir().unwrap();
        let mut index = DirIndex::new();
        let (name, exists) = game_folder_name("CON", None, tmp.path(), &mut index).unwrap();
        assert_eq!((name.as_str(), exists), ("_CON", false));
        let (name, _) = game_folder_name("Dr. Mario.", None, tmp.path(), &mut index).unwrap();
        assert_eq!(name, "Dr. Mario");
        // A region joins a name without one, never a second one.
        let (name, _) =
            game_folder_name("Zelda", Some(Region::Europe), tmp.path(), &mut index).unwrap();
        assert_eq!(name, "Zelda (Europe)");
        let (name, _) =
            game_folder_name("Zelda (USA)", Some(Region::Europe), tmp.path(), &mut index).unwrap();
        assert_eq!(name, "Zelda (USA)");
        // Claimed already: the next free name.
        let (name, _) =
            game_folder_name("Zelda (USA)", Some(Region::Europe), tmp.path(), &mut index).unwrap();
        assert_eq!(name, "Zelda (USA) (2)");
    }

    #[test]
    fn a_long_dlc_name_is_shortened_before_the_game_and_never_the_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let units = |p: &Path| p.to_string_lossy().encode_utf16().count();
        let game = "G".repeat(40);
        let folder_units = units(&root.join("3DS").join(&game).join("DLC"));
        // Room for the game prefix and a few characters of the DLC name.
        let room = WINDOWS_MAX_PATH - folder_units - 1;
        assert!(room > game.len() + 10, "temp path too long for the test");
        let layout = game_layout(root, true);
        let mut items = family(&game, GameRole::Dlc, &["a.cia"]);
        let long = "D".repeat(400);
        game_children_targets(
            &mut items,
            &layout,
            &mut DirIndex::new(),
            &never_same,
            &|_| Some(long.clone()),
        );
        let target = items[1].target.clone().expect("placed");
        assert!(units(&root.join(&target)) <= WINDOWS_MAX_PATH);
        let parts = components(&target);
        assert_eq!(parts[..3], ["3DS".to_string(), game.clone(), "DLC".into()]);
        assert!(parts[3].starts_with(&format!("{game} - D")), "{parts:?}");
        assert!(parts[3].ends_with("D.cia"), "{parts:?}");

        // No room even for one character of each: path too long.
        let deep = "F".repeat(WINDOWS_MAX_PATH);
        let mut items = family(&deep, GameRole::Dlc, &["a.cia"]);
        game_children_targets(
            &mut items,
            &layout,
            &mut DirIndex::new(),
            &never_same,
            &|_| Some(long.clone()),
        );
        assert_eq!(
            items[1].action,
            OrganizeAction::Left {
                reason: LeftReason::PathTooLong
            }
        );
    }

    #[test]
    fn both_dlc_spellings_on_disk_fill_the_first_and_create_no_third() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let game = root.join("3DS").join("Game");
        fs::create_dir_all(game.join("dlc")).unwrap();
        // Only a case-sensitive volume can hold both spellings.
        if fs::create_dir(game.join("DLC")).is_err() || !game.join("dlc").is_dir() {
            return;
        }
        let entries = || fs::read_dir(&game).unwrap().count();
        if entries() != 2 {
            return;
        }
        let layout = game_layout(root, false);
        let mut items = family("Game", GameRole::Dlc, &["a.cia"]);
        game_children_targets(
            &mut items,
            &layout,
            &mut DirIndex::new(),
            &never_same,
            &|_| Some("Pack".into()),
        );
        assert_eq!(
            components(items[1].target.as_deref().expect("placed")),
            ["3DS", "Game", "DLC", "Game - Pack.cia"]
        );
        assert_eq!(entries(), 2);
    }

    #[test]
    fn children_follow_a_left_base() {
        let tmp = tempfile::tempdir().unwrap();
        let layout = game_layout(tmp.path(), false);
        // Left before the children pass: they leave with its reason.
        let mut items = family("Game", GameRole::Update, &["u.cia"]);
        items[0].leave(LeftReason::TooBigForFat);
        game_children_targets(
            &mut items,
            &layout,
            &mut DirIndex::new(),
            &never_same,
            &|_| None,
        );
        assert_eq!(
            items[1].action,
            OrganizeAction::Left {
                reason: LeftReason::TooBigForFat
            }
        );
        // Left by a later gate: the follow pass leaves them too and the
        // base no longer names a folder.
        let mut items = family("Game", GameRole::Update, &["u.cia"]);
        game_children_targets(
            &mut items,
            &layout,
            &mut DirIndex::new(),
            &never_same,
            &|_| None,
        );
        assert_eq!(
            components(items[1].target.as_deref().unwrap()),
            ["3DS", "Game", "Updates", "Game (v1.1.0).cia"]
        );
        items[0].leave(LeftReason::PathTooLong);
        game_children_follow(&mut items);
        assert_eq!(
            items[1].action,
            OrganizeAction::Left {
                reason: LeftReason::PathTooLong
            }
        );
        assert_eq!(items[0].game.as_ref().unwrap().folder, None);
    }

    #[test]
    fn compose_cuts_the_stem_to_255_bytes() {
        let (name, cut) = compose(Path::new("/t"), &"é".repeat(200), Some("gb"), &[], 2, false);
        assert!(cut);
        assert!(name.len() <= MAX_NAME_BYTES);
        assert!(name.ends_with(" (2).gb"));
    }
}
