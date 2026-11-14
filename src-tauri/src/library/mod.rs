//! An opened library: scanning a folder into a [`LibraryView`] for the UI,
//! and keeping, per entry, the paths only the backend may touch.

pub mod art;
pub mod details;
pub mod extras;
pub mod identify;
pub mod refresh;
pub mod view;

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use coverly_core::art::children::{child_stem, present};
use coverly_core::art::resolve_art;
use coverly_core::catalog::latest::{version_facts, version_in_name};
use coverly_core::dedupe::keeper::version_order;
use coverly_core::game_folder::ChildRole;
use coverly_core::identify::names::parse_name;
use coverly_core::organize::{count_library_loose_roms, DEFAULT_LAYOUT};
use coverly_core::profiles::custom::{find_profile, ProfileFile};
use coverly_core::profiles::{detect_with_fallback, fit_to_root, Media, Profile};
use coverly_core::scan::{
    scan_library_with_progress, ChildEntry, RomEntry, ScanOptions, ScanProgress,
};
use coverly_core::settings::Settings;
use coverly_core::systems::SystemDb;

use crate::error::IpcError;
use view::{
    ArtMarks, ChildView, DemoView, EntryView, FolderCountsView, GameContent, LibraryView,
    NewerView, ProfileView, SystemView, WarningView,
};

/// One folder of the ROMs root, as the backend needs it later.
#[derive(Clone, Debug)]
pub struct SessionSystem {
    pub dir: PathBuf,
    /// Folder name as on disk.
    pub folder: String,
    pub system_id: Option<String>,
}

/// Backend-only data of one entry (same index as its [`EntryView`]).
#[derive(Clone, Debug)]
pub struct SessionEntry {
    /// Index in [`Session::systems`].
    pub system: u16,
    pub system_id: String,
    pub rom: RomEntry,
    /// Art files that exist, per media.
    pub art: BTreeMap<Media, PathBuf>,
}

/// The library currently open in the app.
#[derive(Clone, Debug)]
pub struct Session {
    pub generation: u64,
    /// The folder as picked.
    pub root: PathBuf,
    /// `root` canonicalised: nothing outside it is ever read.
    pub root_canon: PathBuf,
    /// The profile fitted to `root` with the Art settings of the open
    /// applied (what the scan used): jobs, the drawer, refreshes and the art
    /// status all use it; art setting changes need a reopen.
    pub profile: Profile,
    pub systems: Vec<SessionSystem>,
    pub entries: Vec<SessionEntry>,
}

/// What to open.
pub struct OpenRequest<'a> {
    pub path: &'a Path,
    /// A built-in or custom profile id, or `None` for the default profile
    /// (else auto-detection).
    pub profile_id: Option<&'a str>,
    pub scan: &'a ScanOptions,
    pub generation: u64,
    /// The custom profiles that may be chosen.
    pub custom: &'a [ProfileFile],
    /// The settings' default profile, used when `profile_id` is `None`.
    pub default_profile: Option<&'a str>,
    /// Settings whose Art group adjusts the profile (`None` = as built).
    pub settings: Option<&'a Settings>,
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
    let fallback = req
        .settings
        .map_or(DEFAULT_LAYOUT, |s| s.organize.default_layout.as_str());
    let (base, detected, evidence) = choose_profile(
        root,
        req.profile_id,
        req.custom,
        req.default_profile,
        fallback,
        systems,
    )?;
    let fitted_profile = fit_to_root(&base, root, systems);
    let fitted = fitted_profile.roms_root != base.roms_root;
    let profile = req.settings.map_or_else(
        || fitted_profile.clone(),
        |s| s.art_profile(&fitted_profile),
    );

    let started = Instant::now();
    let scan = scan_library_with_progress(root, &profile, systems, req.scan, progress);
    let art = resolve_art(&scan, &profile, systems, &Media::ALL);
    let scan_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);

    let mut warnings: Vec<WarningView> = scan.warnings.iter().map(warning).collect();
    let mut system_views = Vec::with_capacity(art.len());
    let mut session_systems = Vec::with_capacity(art.len());
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
        session_systems.push(SessionSystem {
            dir: system_art.scan.dir.clone(),
            folder: system_art.scan.folder_name.clone(),
            system_id: system_art.scan.system_id.clone(),
        });
        warnings.extend(system_art.warnings.iter().map(warning));
        let roms: Vec<&RomEntry> = system_art.roms.iter().map(|r| &r.rom).collect();
        let system_id = system_art.scan.system_id.as_deref().unwrap_or_default();
        let mut contents = game_contents(root, system_id, &roms, entries.len() as u32).into_iter();
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
                name: rom.title_stem().to_string(),
                ext: rom.ext.clone(),
                size: rom.size,
                mtime: unix_secs(rom.modified),
                regions: parse_name(rom.title_stem(), false).regions,
                art: ArtMarks::from_states(&rom_art.art),
                content: contents.next().unwrap_or_default(),
            });
            entries.push(SessionEntry {
                system: index,
                system_id: system_art.scan.system_id.clone().unwrap_or_default(),
                rom: rom.clone(),
                art: refresh::present_paths(&rom_art.art),
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
            gamelist: profile.gamelist,
        },
        systems: system_views,
        entries: entry_views,
        warnings,
        scan_ms,
        loose_files: count_library_loose_roms(root, &profile.roms_root, systems),
        revision_defaults: req
            .settings
            .and_then(|s| s.revision_defaults_of(root))
            .cloned()
            .unwrap_or_default(),
    };
    let session = Session {
        generation: req.generation,
        root: root.to_path_buf(),
        root_canon,
        profile: profile.clone(),
        systems: session_systems,
        entries,
    };
    Ok((session, view))
}

/// The profile the library at `root` is read with, picked and fitted as
/// [`open_session`] does it (the Art settings applied).
pub fn library_profile(
    root: &Path,
    profile_id: Option<&str>,
    custom: &[ProfileFile],
    settings: &Settings,
    systems: &SystemDb,
) -> Result<Profile, IpcError> {
    let (base, _, _) = choose_profile(
        root,
        profile_id,
        custom,
        settings.library.default_profile.as_deref(),
        settings.organize.default_layout.as_str(),
        systems,
    )?;
    Ok(settings.art_profile(&fit_to_root(&base, root, systems)))
}

/// The requested profile (built-in or custom), else the default profile
/// when it still exists, else the best detected one; a tree organised by
/// system without markers suggests `fallback` (evidence `system-folders`).
fn choose_profile(
    root: &Path,
    id: Option<&str>,
    custom: &[ProfileFile],
    default: Option<&str>,
    fallback: &str,
    systems: &SystemDb,
) -> Result<(Profile, bool, Vec<String>), IpcError> {
    if let Some(id) = id {
        return find_profile(custom, id)
            .map(|p| (p, false, Vec::new()))
            .ok_or_else(|| IpcError::UnknownProfile(id.to_string()));
    }
    if let Some(p) = default.and_then(|d| find_profile(custom, d)) {
        return Ok((p, false, Vec::new()));
    }
    let found = detect_with_fallback(root, fallback, systems);
    let best = found
        .first()
        .ok_or_else(|| IpcError::NoProfile(root.display().to_string()))?;
    let profile =
        find_profile(custom, &best.profile_id).expect("detect only returns built-in profile ids");
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

/// The version group and `/`-separated `rel_path` of entry `id`, when it
/// is a current member of a group of more than one (D7): what
/// `set_default_revision` stores. No group or path comes from the webview.
pub fn revision_choice(session: &Session, id: u32) -> Result<(String, String), IpcError> {
    let entry = session
        .entries
        .get(id as usize)
        .ok_or(IpcError::UnknownEntry(id))?;
    let group = entry
        .rom
        .version_group
        .as_deref()
        .filter(|g| group_members(session, g) > 1)
        .ok_or_else(|| {
            IpcError::BadRequest(format!("entry {id} is not one of several revisions"))
        })?;
    Ok((
        group.to_string(),
        rel_string(&session.root, &entry.rom.path),
    ))
}

/// Whether `rel` (compared by components) is still a member of `group`
/// and the group still has more than one member: a choice that is not is
/// pruned from the library's settings.
pub fn revision_live(session: &Session, group: &str, rel: &str) -> bool {
    group_members(session, group) > 1
        && session.entries.iter().any(|e| {
            e.rom.version_group.as_deref() == Some(group)
                && coverly_core::settings::same_rel(&rel_string(&session.root, &e.rom.path), rel)
        })
}

fn group_members(session: &Session, group: &str) -> usize {
    session
        .entries
        .iter()
        .filter(|e| e.rom.role.is_none() && e.rom.version_group.as_deref() == Some(group))
        .count()
}

/// The children of entry `id` as the webview lists them (newest first),
/// read again from disk: their art marks after a write.
pub fn entry_children(session: &Session, id: u32) -> Result<Vec<ChildView>, IpcError> {
    let entry = session
        .entries
        .get(id as usize)
        .ok_or(IpcError::UnknownEntry(id))?;
    Ok(child_views(&session.root, &entry.system_id, &entry.rom))
}

/// The game-content fields of one system's entries (addon views D2,
/// D4–D6), in scan order. `roms` is the system's scan, whose indexes
/// (`demo_of`, `orphan_of`) point into it; `first_id` is the entry id of
/// `roms[0]`. Child art marks are read from the child slots on disk.
fn game_contents(
    root: &Path,
    system_id: &str,
    roms: &[&RomEntry],
    first_id: u32,
) -> Vec<GameContent> {
    let id = |i: usize| first_id + i as u32;
    let mut group_sizes: BTreeMap<&str, usize> = BTreeMap::new();
    for rom in roms {
        if let Some(group) = rom.version_group.as_deref() {
            *group_sizes.entry(group).or_default() += 1;
        }
    }
    // The versions of the orphan updates linked to each game (D6).
    let mut linked_updates: BTreeMap<usize, Vec<(usize, String)>> = BTreeMap::new();
    for (j, rom) in roms.iter().enumerate() {
        if let (Some(ChildRole::Update), Some(game)) = (rom.role, rom.orphan_of) {
            if let Some(v) = version_in_name(rom.title_stem()) {
                linked_updates.entry(game).or_default().push((j, v));
            }
        }
    }
    roms.iter()
        .enumerate()
        .map(|(i, rom)| {
            let version = version_in_name(rom.title_stem());
            let mut content = GameContent {
                orphan: rom.role,
                orphan_of: rom.orphan_of.map(id),
                version_group: rom
                    .version_group
                    .clone()
                    .filter(|g| group_sizes.get(g.as_str()).is_some_and(|&n| n > 1)),
                revision: rom.revision.clone(),
                demo: rom.is_demo.then(|| DemoView {
                    of: rom.demo_of.map(id),
                    of_name: rom
                        .demo_of
                        .and_then(|j| roms.get(j))
                        .map(|g| g.title_stem().to_string()),
                }),
                addons: rom.fixed_counts.map(|f| FolderCountsView {
                    mods: f.mods,
                    cheats: f.cheats,
                    extras: f.extras,
                    saves: f.saves,
                    capped: f.capped,
                }),
                folder_root: rom.is_dir
                    && rom.role.is_none()
                    && (!rom.children.is_empty() || rom.fixed_counts.is_some()),
                ..GameContent::default()
            };
            if rom.role.is_none() {
                content.children = child_views(root, system_id, rom);
                let updates: Vec<Option<&str>> = content
                    .children
                    .iter()
                    .filter(|c| c.role == ChildRole::Update)
                    .map(|c| c.version.as_deref())
                    .collect();
                let orphans: Vec<(usize, &str)> = linked_updates
                    .get(&i)
                    .map(|l| l.iter().map(|(j, v)| (*j, v.as_str())).collect())
                    .unwrap_or_default();
                let facts = version_facts(system_id, version.as_deref(), &updates, &orphans);
                content.installed = facts.installed;
                content.newer_elsewhere = facts.newer_elsewhere.map(|(j, v)| NewerView {
                    entry: id(j),
                    version: v.to_string(),
                });
            }
            content.version = version;
            content
        })
        .collect()
}

/// The name a child shows: its stem without the art suffixes.
fn child_name(child: &ChildEntry) -> String {
    child_stem(&child.path).unwrap_or_else(|| {
        child
            .path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
    })
}

/// D5 for one game: the newest version its own folder holds (the same
/// facts `game_contents` sends as `installed`, without the orphans).
pub(crate) fn installed_of(system_id: &str, rom: &RomEntry) -> Option<String> {
    let updates: Vec<Option<String>> = rom
        .children
        .iter()
        .filter(|c| c.role == ChildRole::Update)
        .map(|c| version_in_name(&child_name(c)))
        .collect();
    let updates: Vec<Option<&str>> = updates.iter().map(Option::as_deref).collect();
    let base = version_in_name(rom.title_stem());
    version_facts(system_id, base.as_deref(), &updates, &[]).installed
}

/// The children of a game, newest version first by `version_order`
/// (a child without a version, or whose version does not parse, after
/// every versioned one), then by name.
fn child_views(root: &Path, system_id: &str, rom: &RomEntry) -> Vec<ChildView> {
    let mut children: Vec<(Option<Vec<u64>>, ChildView)> = rom
        .children
        .iter()
        .enumerate()
        .map(|(index, child)| {
            let name = child_name(child);
            let version = version_in_name(&name);
            let order = version.as_deref().and_then(|v| version_order(system_id, v));
            let view = ChildView {
                index: index as u32,
                rel_path: rel_string(root, &child.path),
                name,
                role: child.role,
                version,
                size: child.size,
                mtime: unix_secs(child.modified),
                art: present(&child.path),
            };
            (order, view)
        })
        .collect();
    children.sort_by(|(a, x), (b, y)| {
        // `Some` is greater than `None`: reversed, versioned children come
        // first, the newest of them first.
        b.cmp(a).then_with(|| x.name.cmp(&y.name))
    });
    children.into_iter().map(|(_, v)| v).collect()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime};

    use coverly_core::game_folder::ChildRole;
    use coverly_core::identify::names::Region;
    use coverly_core::scan::{ChildEntry, FixedFolderCounts, RomEntry};
    use serde_json::json;

    use super::view::{ArtMark, ArtMarks, EntryView, GameContent, NewerView};
    use super::*;

    const SYSTEM: &str = "switch";

    fn at() -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000)
    }

    /// `<root>/ROMS/switch/<dir…>/<stem>.nsp`, built by components.
    fn rom(root: &Path, stem: &str, dir: &[&str]) -> RomEntry {
        let rel: PathBuf = dir.iter().collect();
        RomEntry {
            path: root
                .join("ROMS")
                .join(SYSTEM)
                .join(&rel)
                .join(format!("{stem}.nsp")),
            rel_dir: rel,
            stem: stem.to_string(),
            ext: "nsp".to_string(),
            size: 10,
            modified: at(),
            companions: Vec::new(),
            is_dir: false,
            children: Vec::new(),
            role: None,
            version_group: None,
            revision: None,
            fixed_counts: None,
            is_demo: false,
            demo_of: None,
            orphan_of: None,
        }
    }

    fn child(base: &RomEntry, folder: &str, name: &str, role: ChildRole) -> ChildEntry {
        ChildEntry {
            path: base
                .path
                .parent()
                .unwrap()
                .join(folder)
                .join(format!("{name}.nsp")),
            role,
            size: 5,
            modified: at(),
        }
    }

    fn root() -> PathBuf {
        PathBuf::from("lib")
    }

    #[test]
    fn a_plain_retro_entry_serialises_as_before() {
        let entry = EntryView {
            id: 0,
            system: 0,
            rel_path: "ROMS/SFC/Zelda (USA).sfc".into(),
            dir: "ROMS/SFC".into(),
            name: "Zelda (USA)".into(),
            ext: "sfc".into(),
            size: 1_048_576,
            mtime: 1_700_000_000,
            regions: vec![Region::Usa],
            art: ArtMarks {
                cover: ArtMark::Present,
                screenshot: ArtMark::Unsupported,
                title: ArtMark::Unsupported,
            },
            content: GameContent::default(),
        };
        assert_eq!(
            serde_json::to_value(&entry).unwrap(),
            json!({
                "id": 0,
                "system": 0,
                "rel_path": "ROMS/SFC/Zelda (USA).sfc",
                "dir": "ROMS/SFC",
                "name": "Zelda (USA)",
                "ext": "sfc",
                "size": 1_048_576,
                "mtime": 1_700_000_000,
                "regions": ["usa"],
                "art": { "cover": "present", "screenshot": "unsupported", "title": "unsupported" }
            })
        );
        // A lone retro ROM gets no content field from the scan either: its
        // version group has one member and its name carries no version.
        let mut zelda = rom(&root(), "Zelda (USA)", &[]);
        zelda.version_group = Some("snes:zelda (usa)".into());
        let content = game_contents(&root(), "snes", &[&zelda], 0);
        assert_eq!(content, vec![GameContent::default()]);
    }

    #[test]
    fn children_newest_first_and_installed_from_rust_order() {
        let r = root();
        let mut game = rom(&r, "Tidewalker", &["Tidewalker"]);
        game.children = vec![
            child(&game, "DLC", "Tidewalker - Reef Pack", ChildRole::Dlc),
            child(&game, "Updates", "Tidewalker (v1.0.2)", ChildRole::Update),
            child(&game, "Updates", "Tidewalker (v1.0.10)", ChildRole::Update),
        ];
        let content = game_contents(&r, SYSTEM, &[&game], 4);
        let c = &content[0];
        let order: Vec<(u32, Option<&str>)> = c
            .children
            .iter()
            .map(|ch| (ch.index, ch.version.as_deref()))
            .collect();
        // Natural text order would put 1.0.2 first; `version_order` does not.
        assert_eq!(order, [(2, Some("1.0.10")), (1, Some("1.0.2")), (0, None)]);
        assert_eq!(c.installed.as_deref(), Some("1.0.10"));
        assert_eq!(c.newer_elsewhere, None);
        let first = &c.children[0];
        assert_eq!(first.name, "Tidewalker (v1.0.10)");
        assert_eq!(first.role, ChildRole::Update);
        assert_eq!(
            first.rel_path,
            "ROMS/switch/Tidewalker/Updates/Tidewalker (v1.0.10).nsp"
        );
        assert_eq!(first.size, 5);
        assert_eq!(first.mtime, 1_700_000_000);
        assert!(!first.art.icon && !first.art.banner);
    }

    #[test]
    fn equal_versions_claim_no_installed() {
        let r = root();
        let mut game = rom(&r, "Lantern Harbor", &["Lantern Harbor"]);
        game.children = vec![
            child(
                &game,
                "Updates",
                "Lantern Harbor (v1.05)",
                ChildRole::Update,
            ),
            child(&game, "Updates", "Lantern Harbor (v1.5)", ChildRole::Update),
        ];
        let content = game_contents(&r, SYSTEM, &[&game], 0);
        assert_eq!(content[0].installed, None);
        assert_eq!(content[0].children.len(), 2);
    }

    #[test]
    fn an_orphan_links_to_its_game_and_the_game_sees_it_newer() {
        let r = root();
        let mut game = rom(&r, "Tidewalker", &["Tidewalker"]);
        game.children = vec![child(
            &game,
            "Updates",
            "Tidewalker (v1.0.2)",
            ChildRole::Update,
        )];
        game.fixed_counts = Some(FixedFolderCounts {
            mods: 1,
            cheats: 2,
            extras: 0,
            saves: 0,
            capped: false,
        });
        let mut orphan = rom(
            &r,
            "Tidewalker (v1.3.0)",
            &["Tidewalker (Update)", "Updates"],
        );
        orphan.role = Some(ChildRole::Update);
        orphan.orphan_of = Some(0);
        let mut dlc = rom(&r, "Tidewalker - Coral (v2.0)", &["Loose", "DLC"]);
        dlc.role = Some(ChildRole::Dlc);
        dlc.orphan_of = Some(0);
        let content = game_contents(&r, SYSTEM, &[&game, &orphan, &dlc], 10);
        assert_eq!(content[0].installed.as_deref(), Some("1.0.2"));
        // Only an update can be newer; the DLC's version is not compared.
        assert_eq!(
            content[0].newer_elsewhere,
            Some(NewerView {
                entry: 11,
                version: "1.3.0".into()
            })
        );
        let addons = content[0].addons.unwrap();
        assert_eq!((addons.mods, addons.cheats, addons.capped), (1, 2, false));
        assert_eq!(content[1].orphan, Some(ChildRole::Update));
        assert_eq!(content[1].orphan_of, Some(10));
        assert_eq!(content[1].version.as_deref(), Some("1.3.0"));
        assert!(content[1].children.is_empty());
        assert_eq!(content[1].installed, None);
        assert_eq!(content[2].orphan, Some(ChildRole::Dlc));
        let wire = serde_json::to_value(&content[0]).unwrap();
        assert_eq!(
            wire["addons"],
            json!({ "mods": 1, "cheats": 2, "extras": 0, "saves": 0 })
        );
    }

    /// A folder game holding its own `Updates/` (Wii U, PS3) is the game
    /// folder itself: the webview must not lift it one folder up, so it
    /// says so; a file base at a game folder's root and a plain folder game
    /// say nothing.
    #[test]
    fn a_folder_game_with_content_says_its_folder_is_the_game() {
        let r = root();
        let mut folder = rom(&r, "Lantern Harbor", &["Nintendo"]);
        folder.path = r
            .join("ROMS")
            .join(SYSTEM)
            .join("Nintendo")
            .join("Lantern Harbor");
        folder.is_dir = true;
        folder.children = vec![ChildEntry {
            path: folder
                .path
                .join("Updates")
                .join("Lantern Harbor (v1.05).nsp"),
            role: ChildRole::Update,
            size: 5,
            modified: at(),
        }];
        let mut counted = rom(&r, "Pebble League", &[]);
        counted.path = r.join("ROMS").join(SYSTEM).join("Pebble League");
        counted.is_dir = true;
        counted.fixed_counts = Some(FixedFolderCounts {
            mods: 0,
            cheats: 1,
            extras: 0,
            saves: 0,
            capped: false,
        });
        let mut plain = rom(&r, "Doom", &[]);
        plain.path = r.join("ROMS").join(SYSTEM).join("Doom");
        plain.is_dir = true;
        let mut file = rom(&r, "Tidewalker", &["Tidewalker"]);
        file.children = vec![child(&file, "DLC", "Tidewalker - Reef", ChildRole::Dlc)];
        let content = game_contents(&r, SYSTEM, &[&folder, &counted, &plain, &file], 0);
        let wire: Vec<serde_json::Value> = content
            .iter()
            .map(|c| serde_json::to_value(c).unwrap())
            .collect();
        assert_eq!(wire[0]["folder_root"], json!(true));
        assert_eq!(wire[1]["folder_root"], json!(true));
        assert!(wire[2].get("folder_root").is_none());
        assert!(wire[3].get("folder_root").is_none());
    }

    /// D5/D6 together: a game in doubt (an own update whose name carries
    /// no version, or a tie) claims no installed version and never calls a
    /// linked orphan newer; a game with no version at all does.
    #[test]
    fn a_game_in_doubt_never_calls_an_orphan_newer() {
        let r = root();
        let linked_orphan = |r: &Path| {
            let mut orphan = rom(r, "Tidewalker (v1.3.0)", &["Loose", "Updates"]);
            orphan.role = Some(ChildRole::Update);
            orphan.orphan_of = Some(0);
            orphan
        };

        let mut unversioned = rom(&r, "Tidewalker", &["Tidewalker"]);
        unversioned.children = vec![child(
            &unversioned,
            "Updates",
            "Tidewalker (Update)",
            ChildRole::Update,
        )];
        let content = game_contents(&r, SYSTEM, &[&unversioned, &linked_orphan(&r)], 0);
        assert_eq!(content[0].installed, None);
        assert_eq!(content[0].newer_elsewhere, None);

        let mut tie = rom(&r, "Tidewalker", &["Tidewalker"]);
        tie.children = vec![
            child(&tie, "Updates", "Tidewalker (v1.05)", ChildRole::Update),
            child(&tie, "Updates", "Tidewalker (v1.5)", ChildRole::Update),
        ];
        let content = game_contents(&r, SYSTEM, &[&tie, &linked_orphan(&r)], 0);
        assert_eq!(content[0].installed, None);
        assert_eq!(content[0].newer_elsewhere, None);

        // No version anywhere in the library: the orphan is the newest.
        let bare = rom(&r, "Tidewalker", &["Tidewalker"]);
        let content = game_contents(&r, SYSTEM, &[&bare, &linked_orphan(&r)], 0);
        assert_eq!(content[0].installed, None);
        assert_eq!(
            content[0].newer_elsewhere,
            Some(NewerView {
                entry: 1,
                version: "1.3.0".into()
            })
        );
    }

    #[test]
    fn a_demo_names_its_full_game_and_revisions_share_a_group() {
        let r = root();
        let game = rom(&r, "Lantern Harbor (Europe)", &[]);
        let mut demo = rom(&r, "Lantern Harbor (Europe) (Demo)", &[]);
        demo.is_demo = true;
        demo.demo_of = Some(0);
        let mut lone = rom(&r, "Paper Skies (Demo)", &[]);
        lone.is_demo = true;
        let mut rev0 = rom(&r, "Star Courier (USA)", &[]);
        rev0.version_group = Some("snes:star courier (usa)".into());
        let mut rev1 = rom(&r, "Star Courier (USA) (Rev 1)", &[]);
        rev1.version_group = Some("snes:star courier (usa)".into());
        rev1.revision = Some("Rev 1".into());
        let content = game_contents(&r, "snes", &[&game, &demo, &lone, &rev0, &rev1], 3);
        assert_eq!(content[0].demo, None);
        let demo = content[1].demo.as_ref().unwrap();
        assert_eq!(demo.of, Some(3));
        assert_eq!(demo.of_name.as_deref(), Some("Lantern Harbor (Europe)"));
        assert_eq!(content[2].demo, Some(DemoView::default()));
        assert_eq!(
            serde_json::to_value(&content[2]).unwrap(),
            json!({ "demo": {} })
        );
        assert_eq!(
            content[3].version_group.as_deref(),
            Some("snes:star courier (usa)")
        );
        assert_eq!(content[3].revision, None);
        assert_eq!(content[4].version_group, content[3].version_group);
        assert_eq!(content[4].revision.as_deref(), Some("Rev 1"));
    }
}
