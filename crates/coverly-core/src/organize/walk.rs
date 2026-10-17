//! The read-only walk of a ROM pile (organise spec §7.1): which files are
//! candidates, which stay where they are and why, which folders are
//! already sorted by system and which are too deep to look into. Reads
//! names and metadata only, never file contents.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use walkdir::WalkDir;

use super::plan::LeftReason;
use crate::scan::classify::{
    extension, is_appledouble, is_bios_name, is_hidden, is_junk_file, is_partial_download,
    is_zone_identifier, COMPANION_EXTS, PARTIAL_DOWNLOAD_EXTS,
};
use crate::scan::ScanOptions;
use crate::systems::SystemDb;

/// Default walk depth below the source (spec §7.1).
pub const MAX_DEPTH: usize = 4;

/// Tells whether a file is a cloud placeholder whose bytes are not local
/// (reading it would trigger a download).
pub trait MetaProbe: Sync {
    fn cloud_only(&self, path: &Path, meta: &fs::Metadata) -> bool;
}

/// The real probe: macOS `SF_DATALESS`, Windows `OFFLINE` /
/// `RECALL_ON_DATA_ACCESS`; never cloud-only elsewhere.
#[derive(Clone, Copy, Debug, Default)]
pub struct OsMetaProbe;

impl MetaProbe for OsMetaProbe {
    #[allow(unused_variables)]
    fn cloud_only(&self, path: &Path, meta: &fs::Metadata) -> bool {
        #[cfg(target_os = "macos")]
        {
            use std::os::macos::fs::MetadataExt;
            const SF_DATALESS: u32 = 0x4000_0000;
            return meta.st_flags() & SF_DATALESS != 0;
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            const OFFLINE: u32 = 0x1000;
            const RECALL_ON_DATA_ACCESS: u32 = 0x40_0000;
            return meta.file_attributes() & (OFFLINE | RECALL_ON_DATA_ACCESS) != 0;
        }
        #[allow(unreachable_code)]
        false
    }
}

/// A ROM-like file the planner will inspect.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalkCandidate {
    pub path: PathBuf,
    /// Lowercase extension without the dot.
    pub ext: String,
    pub size: u64,
    /// Modification time in nanoseconds since the Unix epoch.
    pub mtime_ns: i64,
    /// A placeholder: identify by name only, never read.
    pub cloud_only: bool,
}

/// Everything the walk found, each list sorted by path.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Walked {
    pub candidates: Vec<WalkCandidate>,
    pub left: Vec<(PathBuf, LeftReason)>,
    /// Folders that resolve to a system (not walked) and that system's id.
    pub sorted_folders: Vec<(PathBuf, String)>,
    /// Folders at the depth limit that still have content.
    pub deep_folders: Vec<PathBuf>,
    /// Documents, saves and images: they follow a ROM or stay (planner).
    pub companions: Vec<PathBuf>,
    /// Not ROM-like (plan O-D7): counted, listed on demand.
    pub other_files: Vec<PathBuf>,
    /// OS metadata, AppleDouble twins, hidden files, copied ADS streams.
    pub junk: usize,
}

/// Knobs of [`walk`].
pub struct WalkOptions<'a> {
    /// Never entered (the target tree when it is inside the source).
    pub skip_dirs: Vec<PathBuf>,
    /// `<target>/<roms_root>` when inside the source: its system folders
    /// count as sorted, loose files directly in it are candidates.
    pub roms_root: Option<PathBuf>,
    pub art_dirs: &'a [String],
    pub ignored_dirs: &'a [String],
    pub now: SystemTime,
    /// Files modified more recently than this may still be written.
    pub recent: Duration,
    pub probe: &'a dyn MetaProbe,
    /// [`MAX_DEPTH`] in practice.
    pub max_depth: usize,
}

/// Walks `source` without following links or crossing volumes and sorts
/// every entry into the lists of [`Walked`].
///
/// Unreadable sub-folders are skipped; an unreadable `source` is the
/// returned error.
pub fn walk(source: &Path, db: &SystemDb, opts: &WalkOptions) -> io::Result<Walked> {
    fs::read_dir(source)?;

    let mut out = Walked::default();
    let mut sorted: Vec<(PathBuf, String)> = Vec::new();
    let mut refused_links: Vec<PathBuf> = Vec::new();
    let skip_keys: Vec<String> = opts.skip_dirs.iter().map(|p| dir_key(p)).collect();
    let roms_root_key = opts.roms_root.as_deref().map(dir_key);
    let mut partial_dirs: Vec<PathBuf> = Vec::new();
    let ignored: Vec<String> = opts
        .ignored_dirs
        .iter()
        .chain(opts.art_dirs)
        .map(|d| d.to_lowercase())
        .collect();

    let walker = WalkDir::new(source)
        .follow_links(false)
        .same_file_system(true)
        .min_depth(1)
        .max_depth(opts.max_depth)
        .into_iter()
        .filter_entry(|entry| {
            if !entry.file_type().is_dir() {
                return true;
            }
            if entry.metadata().is_ok_and(|m| is_reparse_point(&m)) {
                refused_links.push(entry.path().to_path_buf());
                return false;
            }
            let name = entry.file_name().to_string_lossy();
            if is_hidden(&name) || ignored.contains(&name.to_lowercase()) {
                return false;
            }
            // Safari keeps an unfinished download inside `<name>.download/`.
            if is_partial_download(&name) {
                partial_dirs.push(entry.path().to_path_buf());
                return false;
            }
            let key = if skip_keys.is_empty() && roms_root_key.is_none() {
                String::new()
            } else {
                dir_key(entry.path())
            };
            if skip_keys.contains(&key) {
                return false;
            }
            if roms_root_key.as_deref() == Some(key.as_str()) {
                return true;
            }
            match db.resolve_folder(&name) {
                Some(system) => {
                    sorted.push((entry.path().to_path_buf(), system.id.clone()));
                    false
                }
                None => true,
            }
        });

    let ignored_exts = ScanOptions::default().ignored_exts;
    let mut siblings: HashMap<PathBuf, HashSet<String>> = HashMap::new();

    for entry in walker {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        let file_type = entry.file_type();
        if file_type.is_dir() {
            if entry.depth() == opts.max_depth && has_children(path) {
                out.deep_folders.push(path.to_path_buf());
            }
            continue;
        }
        let Ok(meta) = entry.metadata() else {
            out.left.push((path.to_path_buf(), LeftReason::Unreadable));
            continue;
        };
        if file_type.is_symlink() || is_reparse_point(&meta) {
            out.left.push((path.to_path_buf(), LeftReason::Symlink));
            continue;
        }
        let name = entry.file_name().to_string_lossy();
        if is_appledouble(&name) || is_junk_file(&name) || is_zone_identifier(&name) {
            out.junk += 1;
            continue;
        }
        if is_hidden(&name) {
            out.junk += 1;
            continue;
        }
        if is_partial_download(&name)
            || meta.len() == 0
            || has_partial_sibling(&mut siblings, path, &name)
        {
            out.left
                .push((path.to_path_buf(), LeftReason::PartialDownload));
            continue;
        }
        if is_bios_name(&name) {
            out.left.push((path.to_path_buf(), LeftReason::Bios));
            continue;
        }
        let mtime = meta.modified().unwrap_or(UNIX_EPOCH);
        if is_recent(opts.now, mtime, opts.recent) {
            out.left
                .push((path.to_path_buf(), LeftReason::RecentlyModified));
            continue;
        }
        let ext = extension(path);
        if db.is_rom_like_extension(&ext) {
            out.candidates.push(WalkCandidate {
                path: path.to_path_buf(),
                cloud_only: opts.probe.cloud_only(path, &meta),
                ext,
                size: meta.len(),
                mtime_ns: mtime_ns(mtime),
            });
        } else if is_companion(&name, &ext) || ignored_exts.contains(&ext) {
            out.companions.push(path.to_path_buf());
        } else {
            out.other_files.push(path.to_path_buf());
        }
    }

    out.left
        .extend(refused_links.into_iter().map(|p| (p, LeftReason::Symlink)));
    out.left.extend(
        partial_dirs
            .into_iter()
            .map(|p| (p, LeftReason::PartialDownload)),
    );
    out.sorted_folders = sorted;
    out.candidates.sort_by(|a, b| a.path.cmp(&b.path));
    out.left.sort_by(|a, b| a.0.cmp(&b.0));
    out.sorted_folders.sort();
    out.deep_folders.sort();
    out.companions.sort();
    out.other_files.sort();
    Ok(out)
}

/// Whether picking `path` means picking a whole personal area or volume:
/// `/`, a drive root, a mounted volume (`/Volumes/X`, `/media/<user>/X`,
/// `/run/media/<user>/X`), the home folder and its Desktop, Documents and
/// Downloads.
pub fn is_broad_root(path: &Path) -> bool {
    let abs = normalize(&std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf()));
    if abs.parent().is_none() {
        return true;
    }
    let parts: Vec<String> = abs
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    let parts: Vec<&str> = parts.iter().map(String::as_str).collect();
    if cfg!(unix)
        && matches!(
            parts.as_slice(),
            ["Volumes", _] | ["media", _, _] | ["run", "media", _, _]
        )
    {
        return true;
    }

    let key = path_key(&abs);
    let mut broad: Vec<PathBuf> = Vec::new();
    if let Some(base) = directories::BaseDirs::new() {
        let home = base.home_dir().to_path_buf();
        for child in ["Desktop", "Documents", "Downloads"] {
            broad.push(home.join(child));
        }
        broad.push(home);
    }
    if let Some(user) = directories::UserDirs::new() {
        broad.extend(
            [user.desktop_dir(), user.document_dir(), user.download_dir()]
                .into_iter()
                .flatten()
                .map(Path::to_path_buf),
        );
    }
    broad.iter().any(|b| path_key(&normalize(b)) == key)
}

/// Lexically removes `.` and `..` components.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

/// Comparison key of a path: case-folded where the usual filesystem is
/// case-insensitive (macOS, Windows).
fn path_key(path: &Path) -> String {
    let text = normalize(path).to_string_lossy().into_owned();
    if cfg!(any(target_os = "macos", windows)) {
        text.to_lowercase()
    } else {
        text
    }
}

/// Comparison key of a directory, resolved through symlinks when it
/// exists so a canonical `skip_dirs` entry matches a walk started from a
/// symlinked or relative source.
fn dir_key(path: &Path) -> String {
    let resolved = fs::canonicalize(path)
        .or_else(|_| std::path::absolute(path))
        .unwrap_or_else(|_| path.to_path_buf());
    path_key(&resolved)
}

/// Whether `dir` holds anything besides OS metadata and hidden entries.
fn has_children(dir: &Path) -> bool {
    fs::read_dir(dir).is_ok_and(|it| {
        it.flatten().any(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            !(is_hidden(&name) || is_junk_file(&name) || is_zone_identifier(&name))
        })
    })
}

/// Whether a download is still writing next to `name` (`x.zip` beside
/// `x.zip.part`, `x.zip.crdownload`…). Directory listings are cached.
fn has_partial_sibling(
    cache: &mut HashMap<PathBuf, HashSet<String>>,
    path: &Path,
    name: &str,
) -> bool {
    let Some(dir) = path.parent() else {
        return false;
    };
    let names = cache.entry(dir.to_path_buf()).or_insert_with(|| {
        fs::read_dir(dir)
            .map(|it| {
                it.flatten()
                    .map(|e| e.file_name().to_string_lossy().to_lowercase())
                    .collect()
            })
            .unwrap_or_default()
    });
    let lower = name.to_lowercase();
    PARTIAL_DOWNLOAD_EXTS
        .iter()
        .any(|ext| names.contains(&format!("{lower}.{ext}")))
}

/// Within `recent` of `now`, in either direction (a clock-skewed future
/// mtime counts as fresh only when it is close).
fn is_recent(now: SystemTime, mtime: SystemTime, recent: Duration) -> bool {
    match now.duration_since(mtime) {
        Ok(age) => age < recent,
        Err(ahead) => ahead.duration() < recent,
    }
}

fn mtime_ns(mtime: SystemTime) -> i64 {
    match mtime.duration_since(UNIX_EPOCH) {
        Ok(d) => i64::try_from(d.as_nanos()).unwrap_or(i64::MAX),
        Err(e) => i64::try_from(e.duration().as_nanos()).map_or(i64::MIN, |n| -n),
    }
}

/// Saves, states, patches, cheats, notes and images.
fn is_companion(name: &str, ext: &str) -> bool {
    let lower = name.to_lowercase();
    COMPANION_EXTS.iter().any(|c| match c.strip_suffix('*') {
        // `Game.state`, `Game.state1`, `Game.state.auto`; not `x.statements.dmg`.
        Some(prefix) => {
            (ext.starts_with(prefix) && ext[prefix.len()..].chars().all(|c| c.is_ascii_digit()))
                || lower.contains(&format!(".{prefix}."))
        }
        None => ext == *c,
    })
}

#[allow(unused_variables)]
fn is_reparse_point(meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        return meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0;
    }
    #[allow(unreachable_code)]
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn companions_match_state_slots() {
        assert!(is_companion("Game.state1", "state1"));
        assert!(is_companion("Game.state.auto", "auto"));
        assert!(is_companion("Game.SRM", "srm"));
        assert!(!is_companion("setup.dmg", "dmg"));
    }

    #[test]
    fn recent_is_symmetric_around_now() {
        let now = SystemTime::now();
        let minute = Duration::from_secs(60);
        assert!(is_recent(now, now - Duration::from_secs(5), minute));
        assert!(is_recent(now, now + Duration::from_secs(5), minute));
        assert!(!is_recent(now, now - Duration::from_secs(3600), minute));
    }

    #[test]
    fn normalize_drops_dot_segments() {
        assert_eq!(normalize(Path::new("/a/./b/../c")), PathBuf::from("/a/c"));
    }
}
