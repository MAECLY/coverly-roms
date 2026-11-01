//! The read-only walk of a ROM pile (organise spec §7.1): which files are
//! candidates, which stay where they are and why, which folders are
//! already sorted by system and which are too deep to look into. Reads
//! names and metadata only, plus the first 4 bytes of a `.wad` directly
//! in a folder (the folder-game proof, spec D15). Key and licence files
//! are left by name before anything opens them, and emulator data trees
//! are left whole (systems batch 2 D4, §12).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use super::plan::LeftReason;
use crate::archive::detect::multi_volume_part;
use crate::consoles::{identify_dir, is_key_material, ConsoleId, DirListing, FolderLeave};
use crate::identify::sniff::is_doom_wad;
use crate::organize::inspect::names_an_archive;
use crate::profiles::naming::system_folder;
use crate::profiles::Profile;
use crate::scan::classify::{
    extension, is_appledouble, is_bios_name, is_hidden, is_junk_file, is_partial_download,
    is_zone_identifier, COMPANION_EXTS, PARTIAL_DOWNLOAD_EXTS,
};
use crate::scan::ScanOptions;
use crate::systems::SystemDb;

/// Default walk depth below the source (spec §7.1).
pub const MAX_DEPTH: usize = 4;

/// Systems whose games are folders (spec D15). A pile folder directly
/// holding a file proven for one of them (an `IWAD`/`PWAD`, a `.solarus`
/// or `.lutro` package, `main.lua` beside `conf.lua`, a `.scummvm`/`.svm`
/// launcher) is left in place as one item.
pub const FOLDER_GAME_SYSTEMS: [&str; 4] = ["doom", "solarus", "lutro", "scummvm"];

/// Tells whether a file is a cloud placeholder whose bytes are not local
/// (reading it would trigger a download).
pub trait MetaProbe: Sync {
    fn cloud_only(&self, path: &Path, meta: &fs::Metadata) -> bool;

    /// The length a folder game's [`tree_summary`] counts for a file:
    /// its metadata length (tests answer more, never writing 4 GiB).
    fn file_len(&self, path: &Path, meta: &fs::Metadata) -> u64 {
        let _ = path;
        meta.len()
    }
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
    /// MAME CHD folders (spec D16): a `<set>.zip` candidate and the `.chd`
    /// files of the `<set>/` folder beside it, which holds nothing else.
    /// The CHDs are not candidates; the planner attaches them to the set
    /// when it moves as an arcade set and leaves them otherwise.
    pub chd_folders: Vec<(PathBuf, Vec<PathBuf>)>,
    /// OS metadata, AppleDouble twins, hidden files, copied ADS streams.
    pub junk: usize,
    /// Folders a console probe proved to be one game (spec D12): not
    /// walked, moved whole.
    pub folder_games: Vec<WalkFolderGame>,
    /// Folders two probes claimed for different systems (review), with
    /// those systems in probe order and the folder's tree.
    pub folder_conflicts: Vec<(PathBuf, Vec<String>, TreeSummary)>,
}

/// A folder game the walk found (spec D12, plan P-3).
#[derive(Clone, Debug, PartialEq)]
pub struct WalkFolderGame {
    /// Absolute.
    pub path: PathBuf,
    pub id: ConsoleId,
    /// The probe file, relative to `path`.
    pub probe: PathBuf,
    pub tree: TreeSummary,
}

/// Most entries [`tree_summary`] looks at before it stops (D27).
pub const TREE_MAX_ENTRIES: usize = 100_000;
/// Deepest folder [`tree_summary`] enters below the game folder (D27).
pub const TREE_MAX_DEPTH: usize = 16;

/// A folder game's tree, from metadata only (spec D27).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreeSummary {
    /// Files counted (OS junk is not).
    pub files: u64,
    pub bytes: u64,
    /// Newest file modification, nanoseconds since the Unix epoch.
    pub newest_mtime_ns: i64,
    pub largest_file: u64,
    /// Longest path below the folder, in UTF-16 units (Windows limit
    /// check).
    pub longest_rel_utf16: usize,
    /// Over [`TREE_MAX_ENTRIES`] entries or [`TREE_MAX_DEPTH`] levels:
    /// the counts are a lower bound.
    pub truncated: bool,
    /// A symlink or reparse point inside.
    pub symlink: bool,
    /// A `.part`/`.crdownload` file inside.
    pub partial: bool,
    /// A zero-byte file inside: a partial download for most systems, an
    /// ordinary file in a console dump (systems batch 3 D24).
    #[serde(default)]
    pub zero_bytes: bool,
    /// A file modified within the `recent` window.
    pub recent: bool,
    /// A cloud placeholder inside.
    pub cloud_only: bool,
}

/// Measures the tree below `dir` with `read_dir` and `symlink_metadata`
/// only (never a file's content), without following links, within
/// [`TREE_MAX_ENTRIES`] and [`TREE_MAX_DEPTH`] (spec D27).
pub fn tree_summary(dir: &Path, opts: &WalkOptions) -> TreeSummary {
    let mut out = TreeSummary::default();
    let mut seen = 0usize;
    let mut stack: Vec<(PathBuf, usize)> = vec![(dir.to_path_buf(), 0)];
    while let Some((folder, depth)) = stack.pop() {
        let Ok(entries) = fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.flatten() {
            seen += 1;
            if seen > TREE_MAX_ENTRIES {
                out.truncated = true;
                return out;
            }
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(meta) = fs::symlink_metadata(&path) else {
                continue;
            };
            // A link is a link whatever its name: a `._x` or `Thumbs.db`
            // link must not hide from the gate.
            if meta.file_type().is_symlink() || is_reparse_point(&meta) {
                out.symlink = true;
                continue;
            }
            // Only junk *files* are OS litter: a folder named so is copied
            // with the rest (`write::transfer::list_tree`), so it counts.
            if meta.is_file()
                && (is_appledouble(&name) || is_junk_file(&name) || is_zone_identifier(&name))
            {
                continue;
            }
            if let Ok(rel) = path.strip_prefix(dir) {
                let units = rel.as_os_str().to_string_lossy().encode_utf16().count();
                out.longest_rel_utf16 = out.longest_rel_utf16.max(units);
            }
            if meta.is_dir() {
                if depth + 1 > TREE_MAX_DEPTH {
                    out.truncated = true;
                } else {
                    stack.push((path, depth + 1));
                }
                continue;
            }
            let len = opts.probe.file_len(&path, &meta);
            out.files += 1;
            out.bytes = out.bytes.saturating_add(len);
            out.largest_file = out.largest_file.max(len);
            let mtime = meta.modified().unwrap_or(UNIX_EPOCH);
            out.newest_mtime_ns = out.newest_mtime_ns.max(mtime_ns(mtime));
            if is_partial_download(&name) {
                out.partial = true;
            }
            if len == 0 {
                out.zero_bytes = true;
            }
            if is_recent(opts.now, mtime, opts.recent) {
                out.recent = true;
            }
            if opts.probe.cloud_only(&path, &meta) {
                out.cloud_only = true;
            }
        }
    }
    out
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
    /// The target layout: its own folder names count as sorted too
    /// ([`SystemDb::resolve_folder_in`]); `None` = global aliases only.
    pub profile: Option<&'a Profile>,
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
    let mut folder_games: Vec<(PathBuf, LeftReason)> = Vec::new();
    let mut console_folders: Vec<WalkFolderGame> = Vec::new();
    let mut folder_conflicts: Vec<(PathBuf, Vec<String>, TreeSummary)> = Vec::new();
    let mut emulator_data: Vec<(PathBuf, LeftReason)> = Vec::new();
    // Folders a probe or the overlay rule leaves whole (plan P-2, D11 d).
    let mut left_dirs: Vec<(PathBuf, LeftReason)> = Vec::new();
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
            if is_emulator_data_root(entry.path(), &name) {
                emulator_data.push((
                    entry.path().to_path_buf(),
                    LeftReason::EmulatorData {
                        folder: name.into_owned(),
                    },
                ));
                return false;
            }
            if let Some(system) = db.resolve_folder_in(opts.profile, &name) {
                sorted.push((entry.path().to_path_buf(), system.id.clone()));
                return false;
            }
            // One listing serves the console probes and the batch 1 rule.
            let Some(listed) = list_dir(entry.path()) else {
                return true;
            };
            if is_shadps4_user_dir(&listed) {
                emulator_data.push((
                    entry.path().to_path_buf(),
                    LeftReason::EmulatorData {
                        folder: name.into_owned(),
                    },
                ));
                return false;
            }
            let overlay = overlay_base(entry.path(), &name);
            if let Some((base, Overlay::Mods)) = &overlay {
                left_dirs.push((
                    entry.path().to_path_buf(),
                    LeftReason::GameOverlay { of: base.clone() },
                ));
                return false;
            }
            let names: Vec<String> = listed.iter().map(|(n, _, _)| n.clone()).collect();
            let listing = DirListing {
                dir: entry.path(),
                names: &names,
            };
            match identify_dir(&listing) {
                Ok(Some(found)) => {
                    if let Some(leave) = found.leave {
                        let reason = match leave {
                            FolderLeave::NotARom(what) => LeftReason::NotARom { what },
                            FolderLeave::Unreadable => LeftReason::Unreadable,
                        };
                        left_dirs.push((entry.path().to_path_buf(), reason));
                        return false;
                    }
                    console_folders.push(WalkFolderGame {
                        path: entry.path().to_path_buf(),
                        tree: tree_summary(entry.path(), opts),
                        id: found.id,
                        probe: found.probe,
                    });
                    return false;
                }
                Err(conflict) => {
                    folder_conflicts.push((
                        entry.path().to_path_buf(),
                        conflict.systems,
                        tree_summary(entry.path(), opts),
                    ));
                    return false;
                }
                Ok(None) => {}
            }
            // An update overlay no probe proves cannot move alone (D11 d).
            if let Some((base, Overlay::Update)) = overlay {
                left_dirs.push((
                    entry.path().to_path_buf(),
                    LeftReason::GameOverlay { of: base },
                ));
                return false;
            }
            match folder_game_in(&listed, db, opts) {
                Some(reason) => {
                    folder_games.push((entry.path().to_path_buf(), reason));
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
        // By name, before the size, the siblings or a byte (D4).
        if is_key_material(&name) {
            out.left.push((path.to_path_buf(), LeftReason::KeyMaterial));
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
        // An extensionless file named like a GUID, as the Xbox One and
        // Series store their packages (systems batch 3 D19): a candidate
        // when it has the room for an XVD header and the `msft-xvd` magic
        // at 0x200 (8 bytes peeked, after the key-material check above);
        // a cache blob with such a name, or a placeholder that cannot be
        // peeked without a download, is another file.
        if ext.is_empty() && crate::consoles::xbox::is_guid_name(&name) {
            let cloud_only = opts.probe.cloud_only(path, &meta);
            if !cloud_only
                && meta.len() >= crate::consoles::xbox::XVD_MIN_LEN
                && crate::consoles::xbox::is_xvd_file(path)
            {
                out.candidates.push(WalkCandidate {
                    path: path.to_path_buf(),
                    cloud_only,
                    ext,
                    size: meta.len(),
                    mtime_ns: mtime_ns(mtime),
                });
            } else {
                out.other_files.push(path.to_path_buf());
            }
            continue;
        }
        // Every archive the reader opens is a candidate (a `.rar`, a
        // `.tar.gz`, a `game.gba.gz`), not only zip and 7z, and so is every
        // part of a split 7z: the planner gathers a set's parts into one
        // item that stays (archives spec §3.2 rule 6).
        if db.is_rom_like_extension(&ext) || names_an_archive(&ext) || is_7z_volume(&name) {
            out.candidates.push(WalkCandidate {
                path: path.to_path_buf(),
                cloud_only: opts.probe.cloud_only(path, &meta),
                ext,
                size: meta.len(),
                mtime_ns: mtime_ns(mtime),
            });
        } else if is_companion(&name, &ext)
            || crate::organize::target::is_wbfs_part(&ext)
            || ignored_exts.contains(&ext)
        {
            out.companions.push(path.to_path_buf());
        } else {
            out.other_files.push(path.to_path_buf());
        }
    }

    out.left
        .extend(refused_links.into_iter().map(|p| (p, LeftReason::Symlink)));
    out.left.extend(folder_games);
    out.left.extend(emulator_data);
    out.left.extend(left_dirs);
    out.left.extend(
        partial_dirs
            .into_iter()
            .map(|p| (p, LeftReason::PartialDownload)),
    );
    out.sorted_folders = sorted;
    console_folders.sort_by(|a, b| a.path.cmp(&b.path));
    out.folder_games = console_folders;
    folder_conflicts.sort_by(|a, b| a.0.cmp(&b.0));
    out.folder_conflicts = folder_conflicts;
    out.candidates.sort_by(|a, b| a.path.cmp(&b.path));
    out.chd_folders = take_chd_folders(&mut out.candidates);
    out.left.sort_by(|a, b| a.0.cmp(&b.0));
    out.sorted_folders.sort();
    out.deep_folders.sort();
    out.companions.sort();
    out.other_files.sort();
    Ok(out)
}

/// One `read_dir` of `dir`: each entry's name, path and whether it is a
/// file, sorted by name, AppleDouble twins and OS junk left out; `None`
/// when it cannot be listed.
pub(crate) fn list_dir(dir: &Path) -> Option<Vec<(String, PathBuf, bool)>> {
    let mut out: Vec<(String, PathBuf, bool)> = fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| {
            let is_file = e.file_type().is_ok_and(|t| t.is_file());
            (
                e.file_name().to_string_lossy().into_owned(),
                e.path(),
                is_file,
            )
        })
        .filter(|(name, _, _)| {
            !(is_appledouble(name) || is_junk_file(name) || is_zone_identifier(name))
        })
        .collect();
    out.sort();
    Some(out)
}

/// The folder-game reason of the folder `listed` lists when it directly
/// holds a file proven for one of [`FOLDER_GAME_SYSTEMS`] (spec D15);
/// `None` otherwise. Only a `.wad` is read (its first 4 bytes), never one
/// in the cloud.
fn folder_game_in(
    listed: &[(String, PathBuf, bool)],
    db: &SystemDb,
    opts: &WalkOptions,
) -> Option<LeftReason> {
    let files: Vec<(&String, &PathBuf)> = listed
        .iter()
        .filter(|(name, _, is_file)| *is_file && !(is_hidden(name) || is_key_material(name)))
        .map(|(name, path, _)| (name, path))
        .collect();
    let has = |want: &str| files.iter().any(|(n, _)| n.eq_ignore_ascii_case(want));
    let proven = files
        .iter()
        .find_map(|(name, path)| match extension(Path::new(name)).as_str() {
            "solarus" => Some("solarus"),
            "lutro" => Some("lutro"),
            "scummvm" | "svm" => Some("scummvm"),
            "wad" if starts_like_a_wad(path, opts.probe) => Some("doom"),
            _ => None,
        });
    let proven = proven.or_else(|| (has("main.lua") && has("conf.lua")).then_some("lutro"))?;
    let system = db.get(proven)?;
    let folder = match opts.profile {
        Some(profile) => system_folder(profile, system, db, &BTreeMap::new()).folder,
        None => system
            .aliases
            .first()
            .cloned()
            .unwrap_or_else(|| system.id.clone()),
    };
    Some(LeftReason::FolderGame {
        system: system.id.clone(),
        folder,
    })
}

/// Names of the folders that hold an emulator's own installed data
/// (systems batch 2 §12): Vita3K `ux0`, RPCS3 `dev_hdd0`, Cemu `mlc01`,
/// Ryujinx/yuzu `nand`. Azahar's `sdmc` counts only with its
/// `Nintendo 3DS` child, the name being common elsewhere. A loose
/// `sys_modules` folder (console firmware modules, systems batch 3 D25)
/// counts in any case; shadPS4's user dir is recognised from its
/// listing ([`is_shadps4_user_dir`]).
const EMULATOR_DATA_ROOTS: [&str; 4] = ["ux0", "dev_hdd0", "mlc01", "nand"];

/// Whether the folder `dir` named `name` is an emulator's data root: its
/// files are installed titles, saves and licences that break if moved.
fn is_emulator_data_root(dir: &Path, name: &str) -> bool {
    if EMULATOR_DATA_ROOTS
        .iter()
        .any(|root| name.eq_ignore_ascii_case(root))
        || name.eq_ignore_ascii_case("sys_modules")
    {
        return true;
    }
    name.eq_ignore_ascii_case("sdmc")
        && fs::symlink_metadata(dir.join("Nintendo 3DS")).is_ok_and(|m| m.is_dir())
}

/// Whether the folder `listed` lists is shadPS4's user directory
/// (systems batch 3 D25): it holds both a `sys_modules/` and a
/// `game_data/` directory (`SYSMODULES_DIR`, `METADATA_DIR` of shadPS4's
/// `path_util.h`), names in any case, each checked with
/// `symlink_metadata` (a link is no directory here).
fn is_shadps4_user_dir(listed: &[(String, PathBuf, bool)]) -> bool {
    let dir = |want: &str| {
        listed.iter().any(|(name, path, _)| {
            name.eq_ignore_ascii_case(want) && fs::symlink_metadata(path).is_ok_and(|m| m.is_dir())
        })
    };
    dir("sys_modules") && dir("game_data")
}

/// The kind of a shadPS4 overlay folder (systems batch 3 D11).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Overlay {
    /// `-mods`: loose replacement files, never walked.
    Mods,
    /// `-UPDATE` or `-patch`: a folder game when a probe proves it.
    Update,
}

/// shadPS4 finds overlays by name: the base folder's path plus one of
/// these suffixes (`src/core/file_sys/fs.cpp`, D11). Matched ASCII
/// case-insensitively (f).
const OVERLAY_SUFFIXES: [(&str, Overlay); 3] = [
    ("-mods", Overlay::Mods),
    ("-UPDATE", Overlay::Update),
    ("-patch", Overlay::Update),
];

/// The base folder name and overlay kind of the folder `dir` named
/// `name` when it is `<base><suffix>` and `<parent>/<base>` is a
/// directory holding `sce_sys/` (one `symlink_metadata` each, D11 d).
fn overlay_base(dir: &Path, name: &str) -> Option<(String, Overlay)> {
    let (base, kind) = OVERLAY_SUFFIXES.iter().find_map(|(suffix, kind)| {
        let cut = name.len().checked_sub(suffix.len()).filter(|&c| c > 0)?;
        let tail = name.get(cut..)?;
        tail.eq_ignore_ascii_case(suffix)
            .then(|| (&name[..cut], *kind))
    })?;
    let base_dir = dir.parent()?.join(base);
    let is_dir = |p: &Path| fs::symlink_metadata(p).is_ok_and(|m| m.is_dir());
    (is_dir(&base_dir) && is_dir(&base_dir.join("sce_sys"))).then(|| (base.to_string(), kind))
}

/// Takes out of `candidates` the `.chd` files of every `<set>/` folder
/// that sits beside a `<set>.zip` candidate (any case, a set name per
/// [`is_set_name`]) and directly holds
/// only `.chd` files (junk aside, no sub-folder): MAME's CHD-per-game
/// layout (spec D16). Returns each set with its CHDs, sorted.
fn take_chd_folders(candidates: &mut Vec<WalkCandidate>) -> Vec<(PathBuf, Vec<PathBuf>)> {
    let zips: HashMap<(PathBuf, String), PathBuf> = candidates
        .iter()
        .filter(|c| c.ext == "zip")
        .filter_map(|c| {
            let stem = c.path.file_stem()?.to_string_lossy().to_lowercase();
            is_set_name(&stem).then_some(())?;
            Some(((c.path.parent()?.to_path_buf(), stem), c.path.clone()))
        })
        .collect();
    if zips.is_empty() {
        return Vec::new();
    }
    let mut sets: BTreeMap<PathBuf, Vec<PathBuf>> = BTreeMap::new();
    let mut checked: HashMap<PathBuf, bool> = HashMap::new();
    for c in candidates.iter().filter(|c| c.ext == "chd") {
        let Some(folder) = c.path.parent() else {
            continue;
        };
        let (Some(beside), Some(name)) = (folder.parent(), folder.file_name()) else {
            continue;
        };
        let key = (beside.to_path_buf(), name.to_string_lossy().to_lowercase());
        let Some(zip) = zips.get(&key) else {
            continue;
        };
        let only_chds = *checked
            .entry(folder.to_path_buf())
            .or_insert_with(|| holds_only_chds(folder));
        if only_chds {
            sets.entry(zip.clone()).or_default().push(c.path.clone());
        }
    }
    let taken: HashSet<&PathBuf> = sets.values().flatten().collect();
    candidates.retain(|c| !taken.contains(&c.path));
    sets.into_iter()
        .map(|(zip, mut chds)| {
            chds.sort();
            (zip, chds)
        })
        .collect()
}

/// Whether `stem` can be a MAME set name (`sfiii`, `kof98_2`): ASCII
/// letters, digits and `_` only. A console game unpacked beside its zip
/// (`Final Fantasy VII (USA)/`) is never one, so its CHDs stay candidates.
fn is_set_name(stem: &str) -> bool {
    !stem.is_empty() && stem.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// Whether `dir` directly holds `.chd` files and nothing else but OS junk.
fn holds_only_chds(dir: &Path) -> bool {
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };
    let mut any = false;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if is_hidden(&name)
            || is_appledouble(&name)
            || is_junk_file(&name)
            || is_zone_identifier(&name)
        {
            continue;
        }
        if !entry.file_type().is_ok_and(|t| t.is_file()) || extension(Path::new(&name)) != "chd" {
            return false;
        }
        any = true;
    }
    any
}

/// Whether the local file at `path` opens with `IWAD`/`PWAD`; a cloud
/// placeholder is never read (that would download it).
fn starts_like_a_wad(path: &Path, probe: &dyn MetaProbe) -> bool {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return false;
    };
    if !meta.is_file() || probe.cloud_only(path, &meta) {
        return false;
    }
    let mut head = [0u8; 4];
    crate::consoles::open_rom(path)
        .and_then(|mut f| io::Read::read_exact(&mut f, &mut head))
        .is_ok()
        && is_doom_wad(&head)
}

/// `name` is a part of a split 7z (`.7z.001`, `.7z.002`, …). Other
/// volume names (`.partN.rar`, `.rNN`, `.zNN`) are either archives already
/// or gathered by their set's first part.
fn is_7z_volume(name: &str) -> bool {
    multi_volume_part(name).is_some()
        && name
            .rsplit_once('.')
            .is_some_and(|(_, ext)| ext.bytes().all(|b| b.is_ascii_digit()))
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
pub(crate) fn is_reparse_point(meta: &fs::Metadata) -> bool {
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

    fn tree_opts(probe: &dyn MetaProbe) -> WalkOptions<'_> {
        WalkOptions {
            skip_dirs: Vec::new(),
            roms_root: None,
            art_dirs: &[],
            ignored_dirs: &[],
            now: SystemTime::now(),
            recent: Duration::from_secs(60),
            probe,
            max_depth: MAX_DEPTH,
            profile: None,
        }
    }

    #[test]
    fn tree_summary_records_zero_bytes_apart_from_partial() {
        // Systems batch 3 D24: an empty file is its own signal.
        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.path().join("G");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("empty.dat"), b"").unwrap();
        let probe = OsMetaProbe;
        let t = tree_summary(&root, &tree_opts(&probe));
        assert!(t.zero_bytes && !t.partial);
        fs::write(root.join("x.iso.part"), b"xx").unwrap();
        let t = tree_summary(&root, &tree_opts(&probe));
        assert!(t.zero_bytes && t.partial);
        // Facts stored before the field read it as false.
        let mut json = serde_json::to_value(&t).unwrap();
        json.as_object_mut().unwrap().remove("zero_bytes");
        let back: TreeSummary = serde_json::from_value(json).unwrap();
        assert!(!back.zero_bytes && back.partial);
    }

    #[test]
    fn tree_summary_counts_metadata_and_stops_past_the_depth_cap() {
        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.path().join("G");
        let mut deep = root.clone();
        for i in 0..TREE_MAX_DEPTH {
            deep.push(format!("d{i}"));
        }
        fs::create_dir_all(&deep).unwrap();
        let old = SystemTime::now() - Duration::from_secs(3600);
        for (path, len) in [(root.join("a.bin"), 3usize), (deep.join("z.bin"), 7)] {
            fs::write(&path, vec![1u8; len]).unwrap();
            fs::File::options()
                .write(true)
                .open(&path)
                .and_then(|f| f.set_modified(old))
                .unwrap();
        }
        fs::write(root.join(".DS_Store"), b"junk").unwrap();
        let probe = OsMetaProbe;
        let t = tree_summary(&root, &tree_opts(&probe));
        // `d0/…/d15` is the 16th level below the folder: still entered.
        assert!(!t.truncated);
        assert_eq!((t.files, t.bytes, t.largest_file), (2, 10, 7));
        assert!(!t.partial && !t.recent && !t.symlink && !t.cloud_only);
        assert!(t.longest_rel_utf16 >= "d0/d1/d2/d3/d4/d5/d6/d7/d8/d9".len());

        // A 17th level is past the cap: not entered, the counts a floor.
        fs::create_dir(deep.join("d16")).unwrap();
        let t = tree_summary(&root, &tree_opts(&probe));
        assert!(t.truncated);
        assert_eq!(t.files, 2);

        fs::remove_dir_all(root.join("d0").join("d1")).unwrap();
        let t = tree_summary(&root, &tree_opts(&probe));
        assert!(!t.truncated);
        assert_eq!(t.files, 1);
        assert_eq!(t.longest_rel_utf16, "a.bin".len());
    }

    #[test]
    fn normalize_drops_dot_segments() {
        assert_eq!(normalize(Path::new("/a/./b/../c")), PathBuf::from("/a/c"));
    }
}
