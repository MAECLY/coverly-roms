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

use crate::organize::plan::{
    Companion, ExtractEntry, ItemKind, ItemNote, LeftReason, NameMode, OrganizeAction,
    OrganizeItem, RenameReason, Transfer,
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
    let roms = resolved_roms_dir(layout);
    let mut folders: HashMap<String, (PathBuf, FolderChoice)> = HashMap::new();
    let mut dirs: Vec<Option<(PathBuf, FolderChoice)>> = Vec::with_capacity(items.len());
    for item in items.iter() {
        let dir = match (&item.system_id, item.moves_in()) {
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
        let Some((dir, choice)) = &dirs[pos] else {
            continue;
        };
        item.target = None;
        item.set_targets.clear();
        item.companions.clear();
        item.rename = None;
        item.folder = dir
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
            (Some(shape), Transfer::Extract { .. }) => {
                assign_extract_targets(item, shape, dir, layout, &comps, index)
            }
            _ => match item.kind {
                ItemKind::Set => plan_set(item, dir, &comps, index, same_content),
                ItemKind::Folder => plan_folder(item, dir, layout, index),
                ItemKind::File | ItemKind::Archive => {
                    let original = file_name(&item.sources[0]);
                    let stem = split_name(&original).0.to_string();
                    plan_single(
                        item,
                        (&original, &stem),
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
                }
                item.companions = placed
                    .companions
                    .into_iter()
                    .map(|(source, target)| Companion {
                        source,
                        target: rel(&target),
                    })
                    .collect();
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
    for (pos, item) in items.iter().enumerate() {
        sources.extend(item.sources.iter().map(PathBuf::as_path));
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
                    name_key(&name[..i]) == key.1
                        && companion_ext_matches(companion_exts, &name[i + 1..])
                });
            if matches {
                found.push(Companion {
                    source: rel.to_path_buf(),
                    target: base.join(rel),
                });
            }
        }
        for c in found {
            claimed.insert(c.source.clone());
            item.companions.push(c);
        }
    }
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
fn plan_single(
    item: &OrganizeItem,
    (original, companion_stem): (&str, &str),
    dir: &Path,
    layout: &TargetLayout,
    comps: &[&Comp],
    index: &mut DirIndex,
    same_content: &dyn Fn(&Path, &OrganizeItem) -> bool,
) -> Planned {
    let original_stem = companion_stem;
    let keep = safe_base(original, original, false);
    let catalogue = match (&layout.names, &item.catalogue_name) {
        (NameMode::Catalogue, Some(game)) if !game.trim().is_empty() => {
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
    index: &mut DirIndex,
) -> Planned {
    let original = file_name(&item.sources[0]);
    let safe = safe_file_name(&original).unwrap_or_else(|_| original.clone());
    let suffix = item
        .system_id
        .as_deref()
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
        Some(RenameReason::SafeName { from: original })
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
    Planned::Placed(Placed {
        target: dir.join(&name),
        set_targets: Vec::new(),
        rename,
        companions: Vec::new(),
        claims: vec![(dir.to_path_buf(), name)],
        entry_targets: Vec::new(),
        m3u: None,
    })
}

/// `name` without `suffix` at its end (ASCII case-insensitive), when the
/// rest is not empty.
fn strip_suffix_ignoring_case<'a>(name: &'a str, suffix: &str) -> Option<&'a str> {
    let cut = name.len().checked_sub(suffix.len()).filter(|&c| c > 0)?;
    (name.is_char_boundary(cut) && name[cut..].eq_ignore_ascii_case(suffix)).then(|| &name[..cut])
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
        Transfer::AsIs => item.target.iter().chain(&item.set_targets).collect(),
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

    #[test]
    fn compose_cuts_the_stem_to_255_bytes() {
        let (name, cut) = compose(Path::new("/t"), &"é".repeat(200), Some("gb"), &[], 2, false);
        assert!(cut);
        assert!(name.len() <= MAX_NAME_BYTES);
        assert!(name.ends_with(" (2).gb"));
    }
}
