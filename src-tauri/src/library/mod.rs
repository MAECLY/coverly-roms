//! An opened library: scanning a folder into a [`LibraryView`] for the UI,
//! and keeping, per entry, the paths only the backend may touch.

pub mod art;
pub mod identify;
pub mod view;

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use coverly_core::art::{resolve_art, ArtState};
use coverly_core::identify::names::parse_name;
use coverly_core::profiles::{builtin_profiles, detect, fit_to_root, Media, Profile};
use coverly_core::scan::{scan_library_with_progress, RomEntry, ScanOptions, ScanProgress};
use coverly_core::systems::SystemDb;

use crate::error::IpcError;
use view::{ArtMarks, EntryView, LibraryView, ProfileView, SystemView, WarningView};

/// Backend-only data of one entry (same index as its [`EntryView`]).
#[derive(Debug)]
pub struct SessionEntry {
    pub system_id: String,
    pub rom: RomEntry,
    /// Art files that exist, per media.
    pub art: BTreeMap<Media, PathBuf>,
}

/// The library currently open in the app.
#[derive(Debug)]
pub struct Session {
    pub generation: u64,
    /// The folder as picked.
    pub root: PathBuf,
    /// `root` canonicalised: nothing outside it is ever read.
    pub root_canon: PathBuf,
    pub entries: Vec<SessionEntry>,
}

/// What to open.
pub struct OpenRequest<'a> {
    pub path: &'a Path,
    /// A built-in profile id, or `None` to auto-detect.
    pub profile_id: Option<&'a str>,
    pub scan: &'a ScanOptions,
    pub generation: u64,
}

/// Checks the folder, picks (or detects) the profile, fits it to the
/// folder, scans with `progress` and resolves every media's art state.
/// Read-only.
pub fn open_session(
    req: &OpenRequest<'_>,
    systems: &SystemDb,
    progress: &mut dyn FnMut(ScanProgress),
) -> Result<(Session, LibraryView), IpcError> {
    let root = req.path;
    let shown = root.display().to_string();
    match fs::metadata(root) {
        Ok(meta) if meta.is_dir() => {}
        Ok(_) => return Err(IpcError::NotAFolder(shown)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Err(IpcError::NotFound(shown)),
        Err(_) => return Err(IpcError::Unreadable(shown)),
    }
    let root_canon = fs::canonicalize(root).map_err(|_| IpcError::Unreadable(shown.clone()))?;
    let (base, detected, evidence) = choose_profile(root, req.profile_id)?;
    let profile = fit_to_root(&base, root, systems);
    let fitted = profile.roms_root != base.roms_root;

    let started = Instant::now();
    let scan = scan_library_with_progress(root, &profile, systems, req.scan, progress);
    let art = resolve_art(&scan, &profile, systems, &Media::ALL);
    let scan_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);

    let mut warnings: Vec<WarningView> = scan.warnings.iter().map(warning).collect();
    let mut system_views = Vec::with_capacity(art.len());
    let mut entry_views = Vec::new();
    let mut entries = Vec::new();
    for system_art in &art {
        let index = system_views.len() as u16;
        let known = system_art
            .scan
            .system_id
            .as_deref()
            .and_then(|id| systems.get(id));
        system_views.push(SystemView {
            index,
            folder: system_art.scan.folder_name.clone(),
            dir: rel_string(root, &system_art.scan.dir),
            system_id: system_art.scan.system_id.clone(),
            name: known
                .map(|s| s.name.clone())
                .unwrap_or_else(|| system_art.scan.folder_name.clone()),
            roms: system_art.roms.len() as u32,
        });
        warnings.extend(system_art.warnings.iter().map(warning));
        for rom_art in &system_art.roms {
            let rom = &rom_art.rom;
            let rel_path = rel_string(root, &rom.path);
            let dir = rel_path
                .rsplit_once('/')
                .map(|(d, _)| d.to_string())
                .unwrap_or_default();
            entry_views.push(EntryView {
                id: entries.len() as u32,
                system: index,
                rel_path,
                dir,
                name: rom.stem.clone(),
                ext: rom.ext.clone(),
                size: rom.size,
                mtime: unix_secs(rom.modified),
                regions: parse_name(&rom.stem, false).regions,
                art: ArtMarks::from_states(&rom_art.art),
            });
            entries.push(SessionEntry {
                system_id: system_art.scan.system_id.clone().unwrap_or_default(),
                rom: rom.clone(),
                art: rom_art
                    .art
                    .iter()
                    .filter_map(|(media, state)| match state {
                        ArtState::Present { path } => Some((*media, path.clone())),
                        _ => None,
                    })
                    .collect(),
            });
        }
    }

    let view = LibraryView {
        generation: req.generation,
        root: shown.clone(),
        name: root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or(shown),
        profile: ProfileView {
            id: profile.id.clone(),
            name: profile.name.clone(),
            media: Media::ALL
                .into_iter()
                .filter(|m| profile.slot(*m).is_some())
                .collect(),
            detected,
            evidence,
            fitted,
        },
        systems: system_views,
        entries: entry_views,
        warnings,
        scan_ms,
    };
    let session = Session {
        generation: req.generation,
        root: root.to_path_buf(),
        root_canon,
        entries,
    };
    Ok((session, view))
}

/// The requested built-in profile, or the best detected one.
fn choose_profile(root: &Path, id: Option<&str>) -> Result<(Profile, bool, Vec<String>), IpcError> {
    let all = builtin_profiles();
    if let Some(id) = id {
        return all
            .into_iter()
            .find(|p| p.id.eq_ignore_ascii_case(id))
            .map(|p| (p, false, Vec::new()))
            .ok_or_else(|| IpcError::UnknownProfile(id.to_string()));
    }
    let found = detect(root);
    let best = found
        .first()
        .ok_or_else(|| IpcError::NoProfile(root.display().to_string()))?;
    let profile = all
        .into_iter()
        .find(|p| p.id == best.profile_id)
        .expect("detect only returns built-in profile ids");
    Ok((profile, true, best.evidence.clone()))
}

/// `path` relative to `root`, `/`-separated (lossy for non-UTF-8 names).
fn rel_string(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn unix_secs(t: SystemTime) -> i64 {
    t.duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

fn warning(w: &coverly_core::scan::ScanWarning) -> WarningView {
    WarningView {
        path: w.path.display().to_string(),
        message: w.message.clone(),
    }
}
