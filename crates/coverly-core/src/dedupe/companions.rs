//! Files that travel with a set-aside copy, and files that stay because
//! the keeper shares them (duplicates spec §9.1).
//!
//! For each extra copy: same-stem companions in the same folder (every
//! dot, case-insensitive), art in every profile media slot, macOS `._`
//! twins of everything that moves, and the copy's `gamelist.xml` entry.
//! A companion or an image the keeper also uses stays where it is.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::art::target_path;
use crate::gamelist::{game_fields, gamelist_path, rel_slash};
use crate::profiles::{Media, Profile};
use crate::systems::SystemDb;
use crate::write::name_key;

use super::group::RawGroup;
use super::saves::is_save_suffix;
use super::units::Unit;

/// What a travelling file is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "media", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum TravelKind {
    Save,
    State,
    Cheat,
    Patch,
    Text,
    Art(Media),
    /// A macOS `._` twin.
    AppleDouble,
    /// Any other companion extension of `organize.companion_exts` (an image
    /// beside the ROM, a user-added extension).
    Other,
}

/// A file that leaves with the extra copy.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Travel {
    pub from: PathBuf,
    pub kind: TravelKind,
}

/// A companion with a stem shared with another member; it stays in place.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedStay {
    pub path: PathBuf,
    /// The member (`Member::id`) that keeps using it.
    pub with: usize,
}

/// The `<game>` of an extra copy in its system's `gamelist.xml`, removed
/// when the copy is set aside (profiles with `gamelist: true`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GamelistEntry {
    /// The game list as spelled on disk.
    pub gamelist: PathBuf,
    /// The copy's path relative to the system folder, `./`-prefixed, as
    /// looked up (the file may spell it in another case).
    pub path: String,
}

/// Media slots whose art leaves with the extra while the keeper has none:
/// the keeper ends up without that art (spec §9.1, "Art leaving with the
/// extra").
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtLeaves {
    pub media: Vec<Media>,
}

impl ArtLeaves {
    /// No art leaves.
    pub fn is_empty(&self) -> bool {
        self.media.is_empty()
    }
}

/// What travels with one member, what stays because another member uses
/// it, its game-list entry and the art the keeper is left without. Empty
/// for the keeper.
pub type MemberCompanions = (
    Vec<Travel>,
    Vec<SharedStay>,
    Option<GamelistEntry>,
    ArtLeaves,
);

/// What [`companions_for`] needs to know about the library.
#[derive(Clone, Copy, Debug)]
pub struct CompanionCtx<'a> {
    /// Library root (art templates use `{root}`).
    pub root: &'a Path,
    pub profile: &'a Profile,
    pub db: &'a SystemDb,
    /// `settings.organize.companion_exts`; save extensions always count.
    pub companion_exts: &'a [String],
}

/// Extensions of soft patches, which always travel with their copy.
const PATCH_EXTS: [&str; 5] = ["ips", "bps", "ups", "xdelta", "ppf"];
/// Image extensions an art slot may hold besides the rendered one.
const ART_EXTS: [&str; 3] = ["png", "jpg", "jpeg"];

/// The companions of every member of `group`, index-aligned with
/// `group.members`. `keeper` is a unit index (as in
/// [`RawGroup::relations_to`]); its entry is empty. A file is listed for
/// one member at most: a companion two extras share travels with the
/// first. A companion or image that a unit outside the group also uses
/// (another game with the same stem beside the extra) neither travels nor
/// is listed as a shared stay: it is simply left where it is.
pub fn companions_for(
    group: &RawGroup,
    keeper: usize,
    units: &[Unit],
    ctx: &CompanionCtx,
) -> Vec<MemberCompanions> {
    let mut dirs = Dirs::default();
    let in_group: HashSet<usize> = group.members.iter().map(|(u, _)| *u).collect();
    let extra_stems: HashSet<String> = group
        .members
        .iter()
        .filter(|(u, _)| *u != keeper)
        .filter_map(|(u, _)| units.get(*u))
        .flat_map(|u| file_stems(u).into_iter().map(|(_, s)| s))
        .collect();
    // Units outside the group whose stem could share a companion or an
    // image with an extra: the same stem, or one a dot-prefix of the other.
    let neighbours: Vec<&Unit> = units
        .iter()
        .enumerate()
        .filter(|(i, _)| !in_group.contains(i))
        .map(|(_, u)| u)
        .filter(|u| {
            file_stems(u).iter().any(|(_, s)| {
                extra_stems.iter().any(|e| {
                    e == s || e.starts_with(&format!("{s}.")) || s.starts_with(&format!("{e}."))
                })
            })
        })
        .collect();
    let unit_files: HashSet<String> = group
        .members
        .iter()
        .filter_map(|(u, _)| units.get(*u))
        .chain(neighbours.iter().copied())
        .flat_map(|u| u.files.iter())
        .map(|f| path_key(f))
        .collect();
    let keeper_pos = group.members.iter().position(|(u, _)| *u == keeper);
    let keeper_unit = units.get(keeper).filter(|_| keeper_pos.is_some());

    // What units outside the group use: never moved.
    let mut outside: HashSet<String> = HashSet::new();
    for n in &neighbours {
        for (path, _) in same_stem(n, ctx, &unit_files, &mut dirs) {
            outside.insert(path_key(&path));
        }
        for (_, path) in art_of(n, ctx, &mut dirs) {
            outside.insert(path_key(&path));
        }
    }

    // What the keeper uses, by path key: its companions and its art.
    let mut kept: HashSet<String> = HashSet::new();
    let mut keeper_art: HashMap<Media, PathBuf> = HashMap::new();
    if let Some(k) = keeper_unit {
        for (path, _) in same_stem(k, ctx, &unit_files, &mut dirs) {
            kept.insert(path_key(&path));
        }
        for (media, path) in art_of(k, ctx, &mut dirs) {
            kept.insert(path_key(&path));
            keeper_art.insert(media, path);
        }
    }

    let mut claimed: HashSet<String> = HashSet::new();
    group
        .members
        .iter()
        .map(|(u, _)| {
            let unit = match units.get(*u) {
                Some(unit) if *u != keeper => unit,
                _ => return empty(),
            };
            let with = keeper_pos.unwrap_or(0);
            let mut travels: Vec<Travel> = Vec::new();
            let mut stays: Vec<SharedStay> = Vec::new();
            let mut leaves = ArtLeaves::default();
            let mut take = |path: PathBuf, kind: TravelKind, travels: &mut Vec<Travel>| {
                let key = path_key(&path);
                if kept.contains(&key) {
                    stays.push(SharedStay { path, with });
                    false
                } else if outside.contains(&key) {
                    false
                } else if claimed.insert(key) {
                    travels.push(Travel { from: path, kind });
                    true
                } else {
                    false
                }
            };

            for (path, kind) in same_stem(unit, ctx, &unit_files, &mut dirs) {
                take(path, kind, &mut travels);
            }
            for patch in &unit.patches {
                if !unit_files.contains(&path_key(patch)) {
                    take(patch.clone(), TravelKind::Patch, &mut travels);
                }
            }
            for (media, path) in art_of(unit, ctx, &mut dirs) {
                if take(path, TravelKind::Art(media), &mut travels)
                    && !keeper_art.contains_key(&media)
                {
                    leaves.media.push(media);
                }
            }

            // `._` twins of the copy's own files and of what travels.
            let movers: Vec<PathBuf> = unit
                .files
                .iter()
                .cloned()
                .chain(travels.iter().map(|t| t.from.clone()))
                .collect();
            for path in movers {
                if let Some(twin) = apple_double(&path, &mut dirs) {
                    take(twin, TravelKind::AppleDouble, &mut travels);
                }
            }

            travels.sort_by(|a, b| a.from.cmp(&b.from));
            stays.sort_by(|a, b| a.path.cmp(&b.path));
            stays.dedup();
            let entry = gamelist_entry(unit, ctx);
            (travels, stays, entry, leaves)
        })
        .collect()
}

fn empty() -> MemberCompanions {
    (Vec::new(), Vec::new(), None, ArtLeaves::default())
}

/// Same-folder files whose name is a stem of one of `unit`'s files, a dot,
/// and a companion or save extension: `Game.sav`, `GAME.STATE.AUTO`.
/// Files of any member are never companions.
fn same_stem(
    unit: &Unit,
    ctx: &CompanionCtx,
    unit_files: &HashSet<String>,
    dirs: &mut Dirs,
) -> Vec<(PathBuf, TravelKind)> {
    let stems = file_stems(unit);
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for (folder, stem) in &stems {
        for (name, path) in dirs.list(folder).iter() {
            if name.starts_with("._") || unit_files.contains(&path_key(path)) {
                continue;
            }
            // The stem may end at any dot, the last one first.
            let kind = name
                .match_indices('.')
                .map(|(i, _)| i)
                .rev()
                .filter(|&i| i > 0 && i + 1 < name.len())
                .filter(|&i| name_key(&name[..i]) == *stem)
                .find_map(|i| kind_of(&name[i + 1..], ctx.companion_exts));
            if let Some(kind) = kind {
                if seen.insert(path_key(path)) {
                    out.push((path.clone(), kind));
                }
            }
        }
    }
    out
}

/// The folder and the FAT-compared stem of each of `unit`'s files.
fn file_stems(unit: &Unit) -> Vec<(PathBuf, String)> {
    let mut stems: Vec<(PathBuf, String)> = unit
        .files
        .iter()
        .filter_map(|f| {
            let name = f.file_name()?.to_string_lossy().into_owned();
            let stem = match name.rfind('.') {
                Some(i) if i > 0 => &name[..i],
                _ => name.as_str(),
            };
            Some((f.parent()?.to_path_buf(), name_key(stem)))
        })
        .collect();
    stems.sort();
    stems.dedup();
    stems
}

/// What a companion with the suffix `suffix` (everything after the stem's
/// dot) is, or `None` when it is no companion.
fn kind_of(suffix: &str, companion_exts: &[String]) -> Option<TravelKind> {
    let lower = suffix.to_lowercase();
    if lower.starts_with("state") {
        return Some(TravelKind::State);
    }
    if is_save_suffix(&lower) {
        return Some(TravelKind::Save);
    }
    if PATCH_EXTS.contains(&lower.as_str()) {
        return Some(TravelKind::Patch);
    }
    if !companion_ext_matches(companion_exts, &lower) {
        return None;
    }
    Some(match lower.as_str() {
        "cht" => TravelKind::Cheat,
        "txt" | "nfo" => TravelKind::Text,
        _ => TravelKind::Other,
    })
}

// TODO(merge): use organize::target::match_companions (Task 26).
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

/// The art of `unit` on disk, per media slot of the profile. The rendered
/// file is found ignoring case, in PNG or JPEG.
fn art_of(unit: &Unit, ctx: &CompanionCtx, dirs: &mut Dirs) -> Vec<(Media, PathBuf)> {
    let Some(system) = ctx.db.get(&unit.system) else {
        return Vec::new();
    };
    let system_dir = system_dir(unit);
    Media::ALL
        .iter()
        .filter_map(|&media| {
            let want = target_path(
                ctx.root,
                ctx.profile,
                &system_dir,
                system,
                &unit.rel_dir,
                &unit.stem,
                media,
            )?;
            let folder = want.parent()?;
            let name = want.file_name()?.to_string_lossy().into_owned();
            let (stem, ext) = match name.rfind('.') {
                Some(i) if i > 0 => (&name[..i], &name[i + 1..]),
                _ => (name.as_str(), ""),
            };
            let stem = name_key(stem);
            let ext = ext.to_lowercase();
            let list = dirs.list(folder);
            let found = std::iter::once(ext.as_str())
                .chain(ART_EXTS.iter().copied().filter(|e| *e != ext))
                .find_map(|e| {
                    let wanted = format!("{stem}.{e}");
                    list.iter()
                        .find(|(n, _)| name_key(n) == wanted)
                        .map(|(_, p)| p.clone())
                })?;
            Some((media, found))
        })
        .collect()
}

/// The `._` twin of `path`, ignoring case.
fn apple_double(path: &Path, dirs: &mut Dirs) -> Option<PathBuf> {
    let folder = path.parent()?;
    let name = path.file_name()?.to_string_lossy();
    let wanted = name_key(&format!("._{name}"));
    dirs.list(folder)
        .iter()
        .find(|(n, _)| name_key(n) == wanted)
        .map(|(_, p)| p.clone())
}

/// The `<game>` of `unit` in its system's `gamelist.xml`, when the profile
/// reads one.
fn gamelist_entry(unit: &Unit, ctx: &CompanionCtx) -> Option<GamelistEntry> {
    if !ctx.profile.gamelist {
        return None;
    }
    let system_dir = system_dir(unit);
    let rel = unit.primary.strip_prefix(&system_dir).ok()?;
    game_fields(&system_dir, rel)?;
    Some(GamelistEntry {
        gamelist: gamelist_path(&system_dir),
        path: rel_slash(rel),
    })
}

/// The system folder of `unit`: the primary's folder less `rel_dir`.
fn system_dir(unit: &Unit) -> PathBuf {
    let mut dir = unit
        .primary
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();
    for _ in unit.rel_dir.components() {
        dir.pop();
    }
    dir
}

/// A path compared as FAT and exFAT compare it.
fn path_key(path: &Path) -> String {
    name_key(&path.to_string_lossy())
}

/// Folder listings (hidden files included), read once.
#[derive(Default)]
struct Dirs {
    listed: HashMap<PathBuf, Vec<(String, PathBuf)>>,
}

impl Dirs {
    fn list(&mut self, dir: &Path) -> &[(String, PathBuf)] {
        self.listed.entry(dir.to_path_buf()).or_insert_with(|| {
            let Ok(read) = fs::read_dir(dir) else {
                return Vec::new();
            };
            let mut out: Vec<(String, PathBuf)> = read
                .filter_map(Result::ok)
                .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
                .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
                .collect();
            out.sort();
            out
        })
    }
}
