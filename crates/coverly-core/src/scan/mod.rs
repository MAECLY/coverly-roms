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

use std::fs::{self, FileType};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::Serialize;
use walkdir::WalkDir;

use crate::consoles::{identify_dir, DirListing};
use crate::organize::target::strip_suffix_ignoring_case;
use crate::organize::walk::{list_dir, tree_summary, OsMetaProbe, WalkOptions, MAX_DEPTH};
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
    let mut folder_games: Vec<(PathBuf, Option<&String>)> = Vec::new();
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
            let suffixed = suffix.filter(|s| strip_suffix_ignoring_case(&name, s).is_some());
            if suffixed.is_some() || is_console_folder(e.path()) {
                folder_games.push((e.path().to_path_buf(), suffixed));
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
        // Key and licence files are never library entries, so fill and
        // dedupe never read them (systems batch 2 D4).
        if crate::consoles::is_key_material(&entry.file_name().to_string_lossy()) {
            continue;
        }
        if is_rom_extension(&extension(entry.path()), system, opts) {
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
            .map(|(path, suffix)| folder_entry(dir, path, suffix, profile)),
    );
    roms.sort_by(|a, b| {
        a.rel_dir
            .cmp(&b.rel_dir)
            .then_with(|| a.stem.to_lowercase().cmp(&b.stem.to_lowercase()))
            .then_with(|| a.path.cmp(&b.path))
    });
    roms
}

/// Whether a console probe proves `dir` to be one game (spec D12; the
/// same pure probes the organise walk runs). Two probes naming different
/// systems are not one game here either: Organise sends those to review,
/// and the scan keeps walking them as plain folders.
fn is_console_folder(dir: &Path) -> bool {
    let Some(listed) = list_dir(dir) else {
        return false;
    };
    let names: Vec<String> = listed.into_iter().map(|(n, _, _)| n).collect();
    matches!(
        identify_dir(&DirListing { dir, names: &names }),
        Ok(Some(_))
    )
}

/// The entry of a folder game (spec D26): measured from metadata only
/// (the D27 tree summary), never opened. `suffix` is the layout's suffix
/// the folder name ends in, if any.
fn folder_entry(
    system_dir: &Path,
    path: PathBuf,
    suffix: Option<&String>,
    profile: &Profile,
) -> RomEntry {
    let walk_opts = WalkOptions {
        skip_dirs: Vec::new(),
        roms_root: None,
        art_dirs: &[],
        ignored_dirs: &[],
        now: SystemTime::UNIX_EPOCH,
        recent: Duration::ZERO,
        probe: &OsMetaProbe,
        max_depth: MAX_DEPTH,
        profile: Some(profile),
    };
    let tree = tree_summary(&path, &walk_opts);
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
    let modified = if tree.files > 0 {
        u64::try_from(tree.newest_mtime_ns)
            .map(|ns| SystemTime::UNIX_EPOCH + Duration::from_nanos(ns))
            .unwrap_or(SystemTime::UNIX_EPOCH)
    } else {
        fs::metadata(&path)
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH)
    };
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
