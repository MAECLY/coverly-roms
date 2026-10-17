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
    Companion, ItemKind, ItemNote, LeftReason, NameMode, OrganizeItem, RenameReason,
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
    let roms = resolved_roms_dir(layout);
    let mut folders: HashMap<String, (PathBuf, FolderChoice)> = HashMap::new();
    let mut dirs: Vec<Option<(PathBuf, FolderChoice)>> = Vec::with_capacity(items.len());
    for item in items.iter() {
        let dir = match (&item.system_id, item.is_left()) {
            (Some(system), false) => Some(
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
        let outcome = match item.kind {
            ItemKind::Set => plan_set(item, dir, &comps, index, same_content),
            ItemKind::File | ItemKind::Archive => {
                plan_single(item, dir, layout, &comps, index, same_content)
            }
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

fn companion_ext_matches(patterns: &[String], ext: &str) -> bool {
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

fn plan_single(
    item: &OrganizeItem,
    dir: &Path,
    layout: &TargetLayout,
    comps: &[&Comp],
    index: &mut DirIndex,
    same_content: &dyn Fn(&Path, &OrganizeItem) -> bool,
) -> Planned {
    let original = file_name(&item.sources[0]);
    let (original_stem, _) = split_name(&original);
    let keep = safe_base(&original, &original, false);
    let catalogue = match (&layout.names, &item.catalogue_name) {
        (NameMode::Catalogue, Some(game)) if !game.trim().is_empty() => {
            let ext = split_name(&original).1;
            let wanted = match ext {
                Some(ext) => format!("{}.{ext}", game.trim()),
                None => game.trim().to_string(),
            };
            let base = safe_base(&wanted, &original, true);
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
    })
}

/// Leaves every item but one of each content group in place as a
/// duplicate: the kept one has the shortest file name, then the first
/// alphabetically. `keys[i]` is item `i`'s CRC32 + size (or SHA1 + size);
/// `None` never matches. Items already left are not considered.
pub fn mark_duplicates(items: &mut [OrganizeItem], keys: &[Option<(String, u64)>]) {
    let mut groups: HashMap<&(String, u64), Vec<usize>> = HashMap::new();
    for (pos, item) in items.iter().enumerate() {
        if let Some(Some(key)) = keys.get(pos) {
            if !item.is_left() {
                groups.entry(key).or_default().push(pos);
            }
        }
    }
    for mut group in groups.into_values().filter(|g| g.len() > 1) {
        group.sort_by_key(|&pos| {
            let name = items[pos]
                .sources
                .first()
                .map(|s| file_name(s))
                .unwrap_or_default();
            (name.chars().count(), name)
        });
        let of = items[group[0]].sources[0].clone();
        for &pos in &group[1..] {
            items[pos].leave(LeftReason::Duplicate { of: of.clone() });
        }
    }
}

/// Whether `path` (or its nearest existing ancestor) is on FAT32: macOS
/// `statfs` type `msdos`, Linux the longest matching mount of type `vfat`,
/// Windows `GetVolumeInformationW` name `FAT32`. Any error → `false`.
pub fn file_system_is_fat32(path: &Path) -> bool {
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
