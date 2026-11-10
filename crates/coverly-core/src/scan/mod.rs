//! Library walker: system folders and the ROMs inside them.
//!
//! [`scan_library`] lists the system dirs of a library (the children of the
//! profile's `roms_root`), resolves each to a known system and walks the
//! known ones for ROM files. Multi-file games (`.cue` + tracks, `.m3u` +
//! discs, `.gdi` + tracks) become one [`RomEntry`] with companions.
//! Scanning is read-only and never fails: unreadable entries and broken
//! symlinks become [`ScanWarning`]s.

pub(crate) mod classify;
pub(crate) mod multifile;

pub use classify::not_a_game;

use std::collections::HashMap;
use std::fs::{self, FileType};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::Serialize;
use walkdir::WalkDir;

use crate::consoles::{identify_dir_on_disk, is_key_material};
use crate::game_folder::{classify_listing, ChildRole, DirShape, FixedFolder, GameBase};
use crate::identify::names::{parse_tags, ReleaseClass};
use crate::organize::target::strip_suffix_ignoring_case;
use crate::organize::walk::{
    list_dir, tree_summary_excluding, OsMetaProbe, WalkOptions, MAX_DEPTH, SHEET_EXTS, TRACK_EXTS,
};
use crate::profiles::Profile;
use crate::systems::{System, SystemDb};

use classify::{extension, is_rom_extension, is_skipped_dir, is_skipped_entry};

/// One game as seen by the user.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RomEntry {
    /// Primary file (the `.m3u`, `.cue` or `.gdi` of a multi-file game).
    pub path: PathBuf,
    /// Folder of the primary file relative to the system dir (empty at the
    /// top level).
    pub rel_dir: PathBuf,
    /// File name of the primary file without its extension.
    pub stem: String,
    /// Extension of the primary file, lowercase, without the dot.
    pub ext: String,
    /// Size of the primary file plus its companions, in bytes.
    pub size: u64,
    /// Last modification of the primary file.
    pub modified: SystemTime,
    /// Files the primary file names (tracks, discs), transitively.
    pub companions: Vec<PathBuf>,
    /// The entry is a game folder (systems batch 2 D26): `path` is the
    /// folder, `ext` the layout's folder-game suffix without the dot
    /// (`ps3`) or empty, `size` the sum of the files inside and `modified`
    /// the newest of them (metadata only). It is never opened as a file.
    pub is_dir: bool,
    /// The updates and DLC in this game's folder (`Updates/`, `DLC/`,
    /// game folders spec D8), sorted by path. They are not games: fill,
    /// gamelists, audit and the library views count top-level entries
    /// only. No header is read for them.
    pub children: Vec<ChildEntry>,
    /// Set on an update or DLC whose folder has no base at its root (an
    /// orphan, D8): it stays a top-level entry so it is still visible,
    /// counted apart.
    pub role: Option<ChildRole>,
    /// Same system + title key with revision tags stripped, region kept
    /// (D15): the revisions of one game share it. `None` for an orphan
    /// and when the name leaves no title.
    pub version_group: Option<String>,
    /// The revision as written (`Rev 1`, `Rev A`, `v1.1`), if any (D15).
    pub revision: Option<String>,
    /// File counts of the game folder's `Mods/`, `Cheats/`, `Extras/`
    /// and `Saves/` (part C D9), set only on a base at a game folder's
    /// root (zeros when it has none of them); `None` elsewhere.
    pub fixed_counts: Option<FixedFolderCounts>,
}

/// How many regular files a game folder keeps in each of its non-game
/// fixed folders (part C D9). Metadata only: nothing is opened, symlinks
/// are never followed, junk and AppleDouble twins are not counted, depth
/// stops at 4 below the fixed folder, and each count stops at
/// [`FIXED_CAP`] with `capped` set.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct FixedFolderCounts {
    pub mods: u32,
    pub cheats: u32,
    pub extras: u32,
    pub saves: u32,
    /// At least one folder reached [`FIXED_CAP`]: its count is the cap.
    pub capped: bool,
}

/// Most files counted in one fixed folder (part C D9). Small in unit
/// tests so the cap is exercised without thousands of files.
pub const FIXED_CAP: u32 = if cfg!(test) { 5 } else { 10_000 };

/// Deepest level below a fixed folder whose files are counted (part C D9).
const FIXED_COUNT_DEPTH: usize = 4;

/// An update or DLC inside a game's folder (game folders spec D8): a file,
/// or a folder a console probe recognises (a PS3 update folder in
/// `Updates/`), measured from metadata only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChildEntry {
    pub path: PathBuf,
    pub role: ChildRole,
    pub size: u64,
    pub modified: SystemTime,
}

impl RomEntry {
    /// The game's name: `stem`, without the folder-game suffix a profile
    /// keeps in a folder entry's media stem (ES-DE names the media of
    /// `Game.ps3/` `Game.ps3`, but the game is still `Game`).
    pub fn title_stem(&self) -> &str {
        if !self.is_dir || self.ext.is_empty() {
            return &self.stem;
        }
        let cut = self.stem.len().checked_sub(self.ext.len() + 1);
        match cut {
            Some(c)
                if c > 0
                    && self.stem.is_char_boundary(c)
                    && self.stem[c..].starts_with('.')
                    && self.stem[c + 1..].eq_ignore_ascii_case(&self.ext) =>
            {
                &self.stem[..c]
            }
            _ => &self.stem,
        }
    }
}

/// One child folder of the ROMs root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SystemScan {
    /// The folder.
    pub dir: PathBuf,
    /// Its name as on disk, e.g. `SFC`.
    pub folder_name: String,
    /// Id of the matching system; `None` for unknown folders (`APPS`,
    /// `bios`…), which are listed but not scanned.
    pub system_id: Option<String>,
    /// ROMs found, sorted by sub-folder then name.
    pub roms: Vec<RomEntry>,
}

/// A problem that did not stop the scan.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ScanWarning {
    /// Entry the problem is about.
    pub path: PathBuf,
    /// Human-readable description.
    pub message: String,
}

/// Progress of [`scan_library_with_progress`]: one event before the first
/// folder (`done == 0`, empty `folder`) and one after each folder of the
/// ROMs root, known or not. No event is sent when the ROMs folder is
/// missing or unreadable (the scan then only holds a warning).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ScanProgress {
    /// Folders of the ROMs root scanned so far.
    pub done: usize,
    /// Folders of the ROMs root to scan (hidden and skipped ones excluded).
    pub total: usize,
    /// Name of the folder just scanned (empty in the first event).
    pub folder: String,
    /// ROMs found so far.
    pub roms: usize,
}

/// Result of [`scan_library`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryScan {
    /// Library root as given.
    pub root: PathBuf,
    /// Id of the profile used.
    pub profile_id: String,
    /// Every non-hidden folder of the ROMs root, sorted by name ignoring
    /// case.
    pub systems: Vec<SystemScan>,
    /// Problems met while scanning.
    pub warnings: Vec<ScanWarning>,
}

/// What a scan includes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScanOptions {
    /// Scan dot-prefixed files and folders. AppleDouble `._*` twins are
    /// skipped regardless.
    pub include_hidden: bool,
    /// Descend into symlinked folders. Symlinked files are always read.
    pub follow_symlinks: bool,
    /// Folder names never scanned, at any depth inside a system dir
    /// (compared case-insensitively).
    pub ignored_dirs: Vec<String>,
    /// Extensions never taken as ROMs (lowercase, no dot).
    pub ignored_exts: Vec<String>,
    /// Names of duplicates quarantine folders (duplicates spec §7.1):
    /// never listed as a system folder of the ROMs root and never walked
    /// inside a system folder, compared case-insensitively.
    pub quarantine_names: Vec<String>,
}

/// The quarantine folder names every scan skips by default: the two names
/// the app picks from its language.
pub const QUARANTINE_NAMES: [&str; 2] = ["_Duplicados", "_Duplicates"];

impl Default for ScanOptions {
    fn default() -> Self {
        let strings = |items: &[&str]| items.iter().map(|s| s.to_string()).collect();
        ScanOptions {
            include_hidden: false,
            follow_symlinks: false,
            ignored_dirs: strings(&["bios", "saves", "states", "Shortcuts"]),
            ignored_exts: strings(&[
                "txt", "xml", "json", "cfg", "ini", "db", "png", "jpg", "jpeg", "webp", "bmp",
                "srm", "sav", "state", "pdf", "md", "nfo", "log", "dat",
            ]),
            quarantine_names: strings(&QUARANTINE_NAMES),
        }
    }
}

/// Scans `root` laid out as `profile`.
///
/// System dirs are the children of `root/profile.roms_root` (each
/// component matched case-insensitively). Unknown folders are listed with
/// `system_id: None` and no ROMs. Inside a known system, files with one of
/// the system's extensions are ROMs (any non-ignored extension if the
/// system lists none); art and ignored folders are skipped at any depth,
/// other sub-folders are walked. Never writes and never panics on I/O
/// errors.
pub fn scan_library(
    root: &Path,
    profile: &Profile,
    systems: &SystemDb,
    opts: &ScanOptions,
) -> LibraryScan {
    scan_library_with_progress(root, profile, systems, opts, &mut |_| {})
}

/// [`scan_library`], calling `progress` before the first system folder and
/// after each one (see [`ScanProgress`]).
pub fn scan_library_with_progress(
    root: &Path,
    profile: &Profile,
    systems: &SystemDb,
    opts: &ScanOptions,
    progress: &mut dyn FnMut(ScanProgress),
) -> LibraryScan {
    scan_filtered(root, profile, systems, opts, None, progress)
}

/// [`scan_library`] that walks only the system folders resolving to one
/// of `only` (system ids); the other folders are listed with no ROMs, as
/// unknown folders are. The duplicates step of Organise reads only the
/// folders its plan touches.
pub fn scan_systems(
    root: &Path,
    profile: &Profile,
    systems: &SystemDb,
    opts: &ScanOptions,
    only: &[String],
) -> LibraryScan {
    scan_filtered(root, profile, systems, opts, Some(only), &mut |_| {})
}

fn is_quarantine(name: &str, opts: &ScanOptions) -> bool {
    opts.quarantine_names
        .iter()
        .any(|q| q.eq_ignore_ascii_case(name))
}

fn scan_filtered(
    root: &Path,
    profile: &Profile,
    systems: &SystemDb,
    opts: &ScanOptions,
    only: Option<&[String]>,
    progress: &mut dyn FnMut(ScanProgress),
) -> LibraryScan {
    let mut scan = LibraryScan {
        root: root.to_path_buf(),
        profile_id: profile.id.clone(),
        systems: Vec::new(),
        warnings: Vec::new(),
    };
    let Some(roms_dir) = find_dir_ignoring_case(root, &profile.roms_root) else {
        scan.warnings.push(ScanWarning {
            path: root.join(&profile.roms_root),
            message: format!("ROMs folder `{}` not found", profile.roms_root),
        });
        return scan;
    };
    let entries = match fs::read_dir(&roms_dir) {
        Ok(entries) => entries,
        Err(err) => {
            scan.warnings.push(io_warning(&roms_dir, &err));
            return scan;
        }
    };

    let mut dirs: Vec<(String, PathBuf)> = Vec::new();
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                scan.warnings.push(io_warning(&roms_dir, &err));
                continue;
            }
        };
        let folder_name = entry.file_name().to_string_lossy().into_owned();
        if is_skipped_entry(&folder_name, opts) || is_quarantine(&folder_name, opts) {
            continue;
        }
        let dir = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !matches!(kind(&dir, file_type, opts, &mut scan.warnings), Kind::Dir) {
            continue;
        }
        dirs.push((folder_name, dir));
    }

    let total = dirs.len();
    let mut found = 0;
    progress(ScanProgress {
        done: 0,
        total,
        folder: String::new(),
        roms: 0,
    });
    for (index, (folder_name, dir)) in dirs.into_iter().enumerate() {
        let system = systems.resolve_folder_in(Some(profile), &folder_name);
        let wanted = |s: &System| only.is_none_or(|ids| ids.contains(&s.id));
        let roms = system
            .filter(|s| wanted(s))
            .map(|s| scan_system(&dir, s, profile, opts, &mut scan.warnings))
            .unwrap_or_default();
        found += roms.len();
        progress(ScanProgress {
            done: index + 1,
            total,
            folder: folder_name.clone(),
            roms: found,
        });
        scan.systems.push(SystemScan {
            dir,
            folder_name,
            system_id: system.map(|s| s.id.clone()),
            roms,
        });
    }
    scan.systems.sort_by(|a, b| {
        a.folder_name
            .to_lowercase()
            .cmp(&b.folder_name.to_lowercase())
            .then_with(|| a.folder_name.cmp(&b.folder_name))
    });
    scan
}

/// Walks one known system dir for ROMs.
///
/// Game folders (spec D3, D8, §8.1) are respected whatever the profile:
/// a directory with exactly one base at its root keeps its `Updates/`
/// and `DLC/` files as `children` of that base and its `Mods/`,
/// `Cheats/`, `Extras/` and `Saves/` unscanned; a directory with fixed
/// folders but no single base keeps its `Updates/`/`DLC/` files as
/// top-level entries with a `role`. A fixed folder directly under the
/// system folder (Batocera `ps4/DLC/`) is scanned as today.
fn scan_system(
    dir: &Path,
    system: &System,
    profile: &Profile,
    opts: &ScanOptions,
    warnings: &mut Vec<ScanWarning>,
) -> Vec<RomEntry> {
    let suffix = profile
        .folder_game_suffix
        .get(&system.id)
        .filter(|s| !s.is_empty());
    let mut folder_games: Vec<FolderGame> = Vec::new();
    // Game folders found, by path, with their base.
    let mut game_dirs: HashMap<PathBuf, GameBase> = HashMap::new();
    // `Updates/`/`DLC/` of a game folder: (fixed dir, role, game dir).
    let mut child_dirs: Vec<(PathBuf, ChildRole, PathBuf)> = Vec::new();
    // `Updates/`/`DLC/` of a folder with no single base.
    let mut orphan_dirs: Vec<(PathBuf, ChildRole)> = Vec::new();
    // `Mods/`, `Cheats/`, `Extras/`, `Saves/` of each game folder, counted
    // for its base once the walk is done (part C D9).
    let mut counted_dirs: HashMap<PathBuf, Vec<(FixedFolder, PathBuf)>> = HashMap::new();
    let walker = WalkDir::new(dir)
        .min_depth(1)
        .follow_links(opts.follow_symlinks)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            if is_skipped_entry(&name, opts) {
                return false;
            }
            if !e.file_type().is_dir() {
                return true;
            }
            if is_skipped_dir(&name, &profile.art_dir_names, opts) || is_quarantine(&name, opts) {
                return false;
            }
            // The system folder itself is never a game (filter_entry also
            // sees entries below `min_depth`).
            if e.depth() == 0 {
                return true;
            }
            if e.depth() >= 2 {
                if let Some(fixed) = FixedFolder::from_dir_name(&name) {
                    let parent = e.path().parent().unwrap_or(dir);
                    if game_dirs.contains_key(parent) {
                        // Taken by the game folder itself: its children
                        // are listed below, the rest is never scanned.
                        return false;
                    }
                    if let Some(role) = fixed.child_role() {
                        orphan_dirs.push((e.path().to_path_buf(), role));
                    }
                }
            }
            let listed = list_dir(e.path()).unwrap_or_default();
            let suffixed = suffix.filter(|s| strip_suffix_ignoring_case(&name, s).is_some());
            let folder_game = suffixed.is_some() || is_console_folder(e.path(), &listed);
            let shape = game_folder_shape(&listed, system, opts, folder_game);
            let mut fixed_dirs = Vec::new();
            if let DirShape::GameFolder { base, fixed } = shape {
                for (folder, spelling) in fixed {
                    let path = e.path().join(&spelling);
                    match folder.child_role() {
                        Some(role) => child_dirs.push((path.clone(), role, e.path().to_path_buf())),
                        None => counted_dirs
                            .entry(e.path().to_path_buf())
                            .or_default()
                            .push((folder, path.clone())),
                    }
                    fixed_dirs.push(path);
                }
                game_dirs.insert(e.path().to_path_buf(), base);
            }
            if folder_game {
                folder_games.push(FolderGame {
                    path: e.path().to_path_buf(),
                    suffix: suffixed,
                    fixed: fixed_dirs,
                });
                return false;
            }
            true
        });

    let mut candidates = Vec::new();
    for item in walker {
        let entry = match item {
            Ok(entry) => entry,
            Err(err) => {
                let path = err.path().unwrap_or(dir).to_path_buf();
                let message = match err.io_error() {
                    Some(io) => io.to_string(),
                    None => err.to_string(),
                };
                warnings.push(ScanWarning { path, message });
                continue;
            }
        };
        if !matches!(
            kind(entry.path(), entry.file_type(), opts, warnings),
            Kind::File
        ) {
            continue;
        }
        if is_candidate(dir, entry.path(), system, opts) {
            candidates.push(entry.into_path());
        }
    }

    let mut roms: Vec<RomEntry> = multifile::group_sets(candidates, warnings)
        .into_iter()
        .filter_map(|(path, companions)| rom_entry(dir, path, companions, warnings))
        .collect();
    roms.extend(
        folder_games
            .into_iter()
            .map(|game| folder_entry(dir, game.path, game.suffix, &game.fixed, profile)),
    );
    for rom in &mut roms {
        if let Some((_, role)) = orphan_dirs
            .iter()
            .filter(|(d, _)| rom.path.starts_with(d))
            .max_by_key(|(d, _)| d.components().count())
        {
            rom.role = Some(*role);
        }
    }
    attach_children(
        dir, system, profile, opts, &game_dirs, child_dirs, &mut roms, warnings,
    );
    set_fixed_counts(&game_dirs, &counted_dirs, opts, &mut roms);
    for rom in &mut roms {
        if rom.role.is_none() {
            set_version_group(rom, &system.id);
        }
    }
    roms.sort_by(|a, b| {
        a.rel_dir
            .cmp(&b.rel_dir)
            .then_with(|| a.stem.to_lowercase().cmp(&b.stem.to_lowercase()))
            .then_with(|| a.path.cmp(&b.path))
    });
    roms
}

/// A folder game found by the walk, with the fixed folders its size leaves
/// out (D5).
struct FolderGame<'a> {
    path: PathBuf,
    suffix: Option<&'a String>,
    fixed: Vec<PathBuf>,
}

/// Whether the file `path` below the system folder `dir` is a ROM
/// candidate of `system`.
fn is_candidate(dir: &Path, path: &Path, system: &System, opts: &ScanOptions) -> bool {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or_default();
    // Key and licence files are never library entries, so fill and
    // dedupe never read them (systems batch 2 D4).
    if is_key_material(&name) {
        return false;
    }
    // Firmware, shader caches and emulator configs are not games
    // (game folders D18): by name and the folders below the system
    // folder only, as the organise walk does.
    if classify::not_a_game(path.strip_prefix(dir).unwrap_or(path)).is_some() {
        return false;
    }
    is_rom_extension(&extension(path), system, opts)
}

/// The shape of one directory from its listing (§8.1): a ROM-like file
/// is one with an extension of `system`; with a sheet at the root its
/// track extensions are not further games. Nothing is opened.
fn game_folder_shape(
    listed: &[(String, PathBuf, bool)],
    system: &System,
    opts: &ScanOptions,
    folder_game: bool,
) -> DirShape {
    if !listed
        .iter()
        .any(|(name, _, is_file)| !is_file && FixedFolder::from_dir_name(name).is_some())
    {
        return DirShape::Plain;
    }
    let entries: Vec<(String, bool)> = listed
        .iter()
        .filter(|(name, _, _)| !is_skipped_entry(name, opts))
        .map(|(name, path, is_file)| {
            let is_dir = !is_file
                && (FixedFolder::from_dir_name(name).is_none()
                    || fs::symlink_metadata(path).is_ok_and(|m| m.is_dir()));
            (name.clone(), is_dir)
        })
        .collect();
    let ext_of = |name: &str| extension(Path::new(name));
    let has_sheet = entries
        .iter()
        .any(|(name, is_dir)| !is_dir && SHEET_EXTS.contains(&ext_of(name).as_str()));
    let rom_like = |name: &str| {
        let ext = ext_of(name);
        !is_key_material(name)
            && !(has_sheet && TRACK_EXTS.contains(&ext.as_str()))
            && is_rom_extension(&ext, system, opts)
    };
    let sheet = |name: &str| SHEET_EXTS.contains(&ext_of(name).as_str());
    classify_listing(&entries, &rom_like, &sheet, folder_game)
}

/// Lists each game folder's `Updates/`/`DLC/` and attaches what it holds
/// to the base entry at the folder's root (D8). When that base produced
/// no entry (unreadable), the children stay top-level with their role,
/// so nothing disappears.
#[allow(clippy::too_many_arguments)]
fn attach_children(
    dir: &Path,
    system: &System,
    profile: &Profile,
    opts: &ScanOptions,
    game_dirs: &HashMap<PathBuf, GameBase>,
    child_dirs: Vec<(PathBuf, ChildRole, PathBuf)>,
    roms: &mut Vec<RomEntry>,
    warnings: &mut Vec<ScanWarning>,
) {
    if child_dirs.is_empty() {
        return;
    }
    // One index of the entries by (path, is_dir): a library of many game
    // folders looks each base up once, not by a walk over every entry.
    let index: HashMap<(PathBuf, bool), usize> = roms
        .iter()
        .enumerate()
        .map(|(i, r)| ((r.path.clone(), r.is_dir), i))
        .collect();
    let mut orphans: Vec<RomEntry> = Vec::new();
    for (fixed_dir, role, game_dir) in child_dirs {
        let children = list_children(dir, &fixed_dir, role, system, profile, opts);
        let base = game_dirs
            .get(&game_dir)
            .and_then(|base| index.get(&base_key(&game_dir, base)).copied());
        match base {
            Some(i) => roms[i].children.extend(children),
            None => orphans.extend(children.into_iter().filter_map(|child| {
                let mut entry = if child.path.is_dir() {
                    folder_entry(dir, child.path, None, &[], profile)
                } else {
                    rom_entry(dir, child.path, Vec::new(), warnings)?
                };
                entry.role = Some(child.role);
                Some(entry)
            })),
        }
    }
    for rom in roms.iter_mut() {
        rom.children.sort_by(|a, b| a.path.cmp(&b.path));
    }
    roms.extend(orphans);
}

/// The `(path, is_dir)` of the entry a game folder's base produces.
fn base_key(game_dir: &Path, base: &GameBase) -> (PathBuf, bool) {
    match base {
        GameBase::SelfIsFolderGame => (game_dir.to_path_buf(), true),
        GameBase::File(name) | GameBase::Sheet(name) => (game_dir.join(name), false),
    }
}

/// Sets [`RomEntry::fixed_counts`] on the base at each game folder's root
/// (part C D9): zeros when the folder keeps none of `Mods/`, `Cheats/`,
/// `Extras/`, `Saves/`. An orphan never gets counts.
fn set_fixed_counts(
    game_dirs: &HashMap<PathBuf, GameBase>,
    counted_dirs: &HashMap<PathBuf, Vec<(FixedFolder, PathBuf)>>,
    opts: &ScanOptions,
    roms: &mut [RomEntry],
) {
    if game_dirs.is_empty() {
        return;
    }
    let index: HashMap<(PathBuf, bool), usize> = roms
        .iter()
        .enumerate()
        .filter(|(_, r)| r.role.is_none())
        .map(|(i, r)| ((r.path.clone(), r.is_dir), i))
        .collect();
    for (game_dir, base) in game_dirs {
        let Some(&i) = index.get(&base_key(game_dir, base)) else {
            continue;
        };
        let mut counts = FixedFolderCounts::default();
        for (folder, path) in counted_dirs.get(game_dir).into_iter().flatten() {
            let (n, capped) = count_fixed(path, opts);
            counts.capped |= capped;
            let slot = match folder {
                FixedFolder::Mods => &mut counts.mods,
                FixedFolder::Cheats => &mut counts.cheats,
                FixedFolder::Extras => &mut counts.extras,
                FixedFolder::Saves => &mut counts.saves,
                FixedFolder::Updates | FixedFolder::Dlc => continue,
            };
            add_capped(slot, n, &mut counts.capped);
        }
        roms[i].fixed_counts = Some(counts);
    }
}

/// Adds `n` files to one fixed folder's count. Two spellings of one
/// folder on a case-sensitive disk (`Mods/`, `mods/`) add up; a sum past
/// [`FIXED_CAP`] stays at the cap and sets `capped`, so the cap is never
/// shown as an exact count.
fn add_capped(slot: &mut u32, n: u32, capped: &mut bool) {
    let sum = slot.saturating_add(n);
    if sum > FIXED_CAP {
        *capped = true;
    }
    *slot = sum.min(FIXED_CAP);
}

/// Counts the regular files below one fixed folder (part C D9): metadata
/// only, at most [`FIXED_COUNT_DEPTH`] levels deep, symlinks never
/// followed whatever the options say (a symlink is not counted), entries
/// [`is_skipped_entry`] leaves out not counted and not entered. Stops at
/// [`FIXED_CAP`]; the flag says whether it did.
fn count_fixed(dir: &Path, opts: &ScanOptions) -> (u32, bool) {
    let mut count: u32 = 0;
    let walker = WalkDir::new(dir)
        .min_depth(1)
        .max_depth(FIXED_COUNT_DEPTH)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !is_skipped_entry(&e.file_name().to_string_lossy(), opts));
    for entry in walker.flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        if count == FIXED_CAP {
            return (count, true);
        }
        count += 1;
    }
    (count, false)
}

/// The updates or DLC below one fixed folder: each ROM candidate file,
/// and each folder a console probe recognises as one entry (not entered).
/// The profile's art folders (`DLC/.media/`, part C D10 (a)) and the
/// ignored folders are never entered. Metadata only.
fn list_children(
    system_dir: &Path,
    fixed_dir: &Path,
    role: ChildRole,
    system: &System,
    profile: &Profile,
    opts: &ScanOptions,
) -> Vec<ChildEntry> {
    let mut out = Vec::new();
    let mut folders: Vec<PathBuf> = Vec::new();
    let walker = WalkDir::new(fixed_dir)
        .min_depth(1)
        .max_depth(MAX_DEPTH)
        .follow_links(opts.follow_symlinks)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            if is_skipped_entry(&name, opts) {
                return false;
            }
            if !e.file_type().is_dir() || e.depth() == 0 {
                return true;
            }
            if is_skipped_dir(&name, &profile.art_dir_names, opts) || is_quarantine(&name, opts) {
                return false;
            }
            let listed = list_dir(e.path()).unwrap_or_default();
            if is_console_folder(e.path(), &listed) {
                folders.push(e.path().to_path_buf());
                return false;
            }
            true
        });
    for entry in walker.flatten() {
        if !entry.file_type().is_file() && !entry.path_is_symlink() {
            continue;
        }
        let Ok(meta) = fs::metadata(entry.path()) else {
            continue;
        };
        if !meta.is_file() || !is_candidate(system_dir, entry.path(), system, opts) {
            continue;
        }
        out.push(ChildEntry {
            path: entry.into_path(),
            role,
            size: meta.len(),
            modified: meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
        });
    }
    for path in folders {
        let tree = tree_summary_excluding(&path, &tree_options(None), &[]);
        out.push(ChildEntry {
            modified: tree_modified(&path, &tree),
            size: tree.bytes,
            path,
            role,
        });
    }
    out
}

/// Fills [`RomEntry::version_group`] and [`RomEntry::revision`] from the
/// title stem's tags (D15): system, title key (revision tags stripped),
/// the regions as written in order, and the release class when it is not
/// retail.
fn set_version_group(rom: &mut RomEntry, system_id: &str) {
    let tags = parse_tags(rom.title_stem());
    rom.revision = tags.revision_text.clone();
    if tags.title_key.is_empty() {
        return;
    }
    let regions: Vec<&str> = tags.regions.iter().map(|r| r.no_intro_word()).collect();
    let mut group = format!("{system_id}:{}", tags.title_key);
    if !regions.is_empty() {
        group.push_str(&format!(" ({})", regions.join(", ").to_lowercase()));
    }
    // `parse_tags` drops the release class from the title key, but a demo
    // is its own game (D12) and a beta or prototype is not a revision of
    // the release: the class stays part of the group.
    if let Some(class) = release_class_word(tags.class) {
        group.push_str(&format!(" [{class}]"));
    }
    rom.version_group = Some(group);
}

/// The word a non-retail release class adds to a version group.
fn release_class_word(class: ReleaseClass) -> Option<&'static str> {
    match class {
        ReleaseClass::Retail => None,
        ReleaseClass::Beta => Some("beta"),
        ReleaseClass::Proto => Some("proto"),
        ReleaseClass::Demo => Some("demo"),
        ReleaseClass::Sample => Some("sample"),
    }
}

/// Whether a console probe proves `dir`, listed once as `listed` (the
/// listing the game folder test reuses, D8), to be one game (spec D12; the
/// same pure probes the organise walk runs). Two probes naming different
/// systems are not one game here either: Organise sends those to review,
/// and the scan keeps walking them as plain folders. So does a folder a
/// probe recognises but leaves in place (a GDK `PC` build, an unreadable
/// config: systems batch 3 plan P-2), which Organise never takes for a
/// console game.
fn is_console_folder(dir: &Path, listed: &[(String, PathBuf, bool)]) -> bool {
    if listed.is_empty() {
        return false;
    }
    let names: Vec<String> = listed.iter().map(|(n, _, _)| n.clone()).collect();
    matches!(
        identify_dir_on_disk(dir, &names),
        Ok(Some(found)) if found.leave.is_none()
    )
}

/// The entry of a folder game (spec D26): measured from metadata only
/// (the D27 tree summary), never opened. `suffix` is the layout's suffix
/// the folder name ends in, if any; `fixed` the game folder's fixed
/// folders, left out of the size (game folders spec D5).
fn folder_entry(
    system_dir: &Path,
    path: PathBuf,
    suffix: Option<&String>,
    fixed: &[PathBuf],
    profile: &Profile,
) -> RomEntry {
    let tree = tree_summary_excluding(&path, &tree_options(Some(profile)), fixed);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (stem, ext) = match suffix.and_then(|s| strip_suffix_ignoring_case(&name, s)) {
        Some(stem) => {
            let ext = name[stem.len()..].trim_start_matches('.').to_lowercase();
            let stem = if profile.folder_stem_keeps_suffix {
                name.clone()
            } else {
                stem.to_string()
            };
            (stem, ext)
        }
        None => (name, String::new()),
    };
    let modified = tree_modified(&path, &tree);
    let rel_dir = path
        .parent()
        .and_then(|p| p.strip_prefix(system_dir).ok())
        .map(Path::to_path_buf)
        .unwrap_or_default();
    RomEntry {
        path,
        rel_dir,
        stem,
        ext,
        size: tree.bytes,
        modified,
        companions: Vec::new(),
        is_dir: true,
        children: Vec::new(),
        role: None,
        version_group: None,
        revision: None,
        fixed_counts: None,
    }
}

/// The options a scan measures a folder tree with (no recent window).
fn tree_options(profile: Option<&Profile>) -> WalkOptions<'_> {
    WalkOptions {
        skip_dirs: Vec::new(),
        roms_root: None,
        art_dirs: &[],
        ignored_dirs: &[],
        now: SystemTime::UNIX_EPOCH,
        recent: Duration::ZERO,
        probe: &OsMetaProbe,
        max_depth: MAX_DEPTH,
        profile,
        cache: None,
    }
}

/// The newest file of a measured tree, or the folder's own mtime when it
/// holds no file.
fn tree_modified(path: &Path, tree: &crate::organize::walk::TreeSummary) -> SystemTime {
    if tree.files > 0 {
        u64::try_from(tree.newest_mtime_ns)
            .map(|ns| SystemTime::UNIX_EPOCH + Duration::from_nanos(ns))
            .unwrap_or(SystemTime::UNIX_EPOCH)
    } else {
        fs::metadata(path)
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH)
    }
}

/// Builds the entry of a primary file, or warns if it cannot be read.
fn rom_entry(
    system_dir: &Path,
    path: PathBuf,
    companions: Vec<PathBuf>,
    warnings: &mut Vec<ScanWarning>,
) -> Option<RomEntry> {
    let meta = match fs::metadata(&path) {
        Ok(meta) => meta,
        Err(err) => {
            warnings.push(io_warning(&path, &err));
            return None;
        }
    };
    let companion_size: u64 = companions
        .iter()
        .filter_map(|c| fs::metadata(c).ok())
        .map(|m| m.len())
        .sum();
    let rel_dir = path
        .parent()
        .and_then(|p| p.strip_prefix(system_dir).ok())
        .map(Path::to_path_buf)
        .unwrap_or_default();
    Some(RomEntry {
        stem: path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default(),
        ext: extension(&path),
        size: meta.len() + companion_size,
        modified: meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
        rel_dir,
        path,
        companions,
        is_dir: false,
        children: Vec::new(),
        role: None,
        version_group: None,
        revision: None,
        fixed_counts: None,
    })
}

/// What an entry is, after resolving symlinks.
enum Kind {
    File,
    Dir,
    Other,
}

/// Classifies an entry. A symlink is read through: to a file it is a file,
/// to a folder it is a folder only with `follow_symlinks`, and a broken one
/// is a warning.
fn kind(
    path: &Path,
    file_type: FileType,
    opts: &ScanOptions,
    warnings: &mut Vec<ScanWarning>,
) -> Kind {
    if !file_type.is_symlink() {
        return if file_type.is_dir() {
            Kind::Dir
        } else if file_type.is_file() {
            Kind::File
        } else {
            Kind::Other
        };
    }
    match fs::metadata(path) {
        Ok(meta) if meta.is_file() => Kind::File,
        Ok(meta) if meta.is_dir() && opts.follow_symlinks => Kind::Dir,
        Ok(_) => Kind::Other,
        Err(err) => {
            warnings.push(ScanWarning {
                path: path.to_path_buf(),
                message: format!("broken symlink: {err}"),
            });
            Kind::Other
        }
    }
}

/// Finds `rel` (`/`-separated, may be empty) below `root`, matching each
/// component case-insensitively and preferring the exact spelling.
pub(crate) fn find_dir_ignoring_case(root: &Path, rel: &str) -> Option<PathBuf> {
    let mut current = root.to_path_buf();
    for component in rel.split(['/', '\\']).filter(|c| !c.is_empty()) {
        let exact = current.join(component);
        let found = fs::read_dir(&current).ok().and_then(|entries| {
            let names: Vec<_> = entries.flatten().map(|e| e.file_name()).collect();
            names
                .iter()
                .find(|n| n.as_os_str() == component)
                .or_else(|| {
                    names
                        .iter()
                        .find(|n| n.to_string_lossy().eq_ignore_ascii_case(component))
                })
                .map(|n| current.join(n))
        });
        current = found.unwrap_or(exact);
    }
    current.is_dir().then_some(current)
}

fn io_warning(path: &Path, err: &std::io::Error) -> ScanWarning {
    ScanWarning {
        path: path.to_path_buf(),
        message: err.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn put(root: &Path, rel: &str) -> PathBuf {
        let p = rel.split('/').fold(root.to_path_buf(), |p, c| p.join(c));
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, [0u8; 4]).unwrap();
        crate::dedupe::testing::pin_mtime(&p, 1);
        p
    }

    #[test]
    fn counting_stops_at_the_cap() {
        let tmp = TempDir::new().unwrap();
        for i in 0..7 {
            put(tmp.path(), &format!("Mods/m{i}.bps"));
        }
        assert_eq!(
            count_fixed(&tmp.path().join("Mods"), &ScanOptions::default()),
            (FIXED_CAP, true)
        );
        let exact = TempDir::new().unwrap();
        for i in 0..FIXED_CAP {
            put(exact.path(), &format!("Mods/m{i}.bps"));
        }
        assert_eq!(
            count_fixed(&exact.path().join("Mods"), &ScanOptions::default()),
            (FIXED_CAP, false)
        );
    }

    #[test]
    fn two_spellings_over_the_cap_are_flagged_capped() {
        // `Mods/` and `mods/` on a case-sensitive disk add up; a sum past
        // the cap is the cap and says so.
        let mut capped = false;
        let mut slot = 3;
        add_capped(&mut slot, 4, &mut capped);
        assert_eq!((slot, capped), (FIXED_CAP, true));
        let mut capped = false;
        let mut slot = 2;
        add_capped(&mut slot, 3, &mut capped);
        assert_eq!((slot, capped), (FIXED_CAP, false));
        add_capped(&mut slot, 0, &mut capped);
        assert_eq!((slot, capped), (FIXED_CAP, false));
    }

    #[test]
    fn depth_and_symlinks() {
        let tmp = TempDir::new().unwrap();
        put(tmp.path(), "Mods/a/b/c/four.bps");
        put(tmp.path(), "Mods/a/b/c/d/five.bps");
        let opts = ScanOptions {
            follow_symlinks: true,
            ..ScanOptions::default()
        };
        #[cfg(unix)]
        {
            put(tmp.path(), "Elsewhere/one.bps");
            put(tmp.path(), "Elsewhere/two.bps");
            std::os::unix::fs::symlink(tmp.path().join("Elsewhere"), tmp.path().join("Mods/link"))
                .unwrap();
        }
        assert_eq!(count_fixed(&tmp.path().join("Mods"), &opts), (1, false));
    }

    #[test]
    fn hidden_files_count_only_with_hidden_on() {
        let tmp = TempDir::new().unwrap();
        put(tmp.path(), "Cheats/.hidden.cht");
        put(tmp.path(), "Cheats/._twin.cht");
        put(tmp.path(), "Cheats/c.cht");
        let dir = tmp.path().join("Cheats");
        assert_eq!(count_fixed(&dir, &ScanOptions::default()), (1, false));
        let hidden = ScanOptions {
            include_hidden: true,
            ..ScanOptions::default()
        };
        assert_eq!(count_fixed(&dir, &hidden), (2, false));
    }
}
