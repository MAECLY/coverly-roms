//! Library walker: system folders and the ROMs inside them.
//!
//! [`scan_library`] lists the system dirs of a library (the children of the
//! profile's `roms_root`), resolves each to a known system and walks the
//! known ones for ROM files. Multi-file games (`.cue` + tracks, `.m3u` +
//! discs, `.gdi` + tracks) become one [`RomEntry`] with companions.
//! Scanning is read-only and never fails: unreadable entries and broken
//! symlinks become [`ScanWarning`]s.

mod classify;
mod multifile;

use std::fs::{self, FileType};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::Serialize;
use walkdir::WalkDir;

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
}

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

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                scan.warnings.push(io_warning(&roms_dir, &err));
                continue;
            }
        };
        let folder_name = entry.file_name().to_string_lossy().into_owned();
        if is_skipped_entry(&folder_name, opts) {
            continue;
        }
        let dir = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !matches!(kind(&dir, file_type, opts, &mut scan.warnings), Kind::Dir) {
            continue;
        }
        let system = systems.resolve_folder(&folder_name);
        let roms = system
            .map(|s| scan_system(&dir, s, profile, opts, &mut scan.warnings))
            .unwrap_or_default();
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
    let walker = WalkDir::new(dir)
        .min_depth(1)
        .follow_links(opts.follow_symlinks)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !is_skipped_entry(&name, opts)
                && !(e.file_type().is_dir() && is_skipped_dir(&name, &profile.art_dir_names, opts))
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
        if is_rom_extension(&extension(entry.path()), system, opts) {
            candidates.push(entry.into_path());
        }
    }

    let mut roms: Vec<RomEntry> = multifile::group_sets(candidates, warnings)
        .into_iter()
        .filter_map(|(path, companions)| rom_entry(dir, path, companions, warnings))
        .collect();
    roms.sort_by(|a, b| {
        a.rel_dir
            .cmp(&b.rel_dir)
            .then_with(|| a.stem.to_lowercase().cmp(&b.stem.to_lowercase()))
            .then_with(|| a.path.cmp(&b.path))
    });
    roms
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
fn find_dir_ignoring_case(root: &Path, rel: &str) -> Option<PathBuf> {
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
