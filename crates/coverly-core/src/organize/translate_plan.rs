//! Organizar and the per-emulator translators (mods spec D16, plan P-1).
//!
//! For each system with an emulator (the run's choice, else the layout
//! profile's default), every master-shaped group of the plan — a base
//! with updates, DLC, mods or cheats linked to it, or addons linked to a
//! base already in the library — is laid out by [`translate`] and the
//! placements become targets:
//!
//! - a profile default runs [`TranslateScope::Conservative`], game
//!   folders on or off (Q11; the part A game folder names stay pinned): a
//!   base, update or DLC keeps the target today's rules gave it and only
//!   gains notes; addons go to the emulator's data folder when the run
//!   names it, and a patch that only a chosen emulator would rename stays
//!   with [`LeftReason::PatchNotApplied`];
//! - an emulator chosen in the run runs [`TranslateScope::Full`]: updates
//!   and DLC may move where the emulator reads them, patches beside their
//!   ROM.
//!
//! An addon for the emulator's data folder when the run names none keeps
//! the place it already has (its game folder's `Mods/` or `Cheats/`);
//! with no place it stays with [`LeftReason::AddonForEmulator`].
//!
//! A base is never moved, except by ES-DE's «directories as files» (D15),
//! which renames a game folder that is not on disk yet. A place already
//! taken is never written over: a game entry keeps today's target, an
//! addon stays with [`LeftReason::AddonNameTaken`] (or *already present*).
//! The emulator data folder is written only when the run names it
//! (`OrganizeOptions::emulator_data`, CLI only); nothing is guessed.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::addons::{valid_game_id, AddonKind};
use crate::consoles::TitleKind;
use crate::systems::SystemDb;
use crate::translate::{
    translate, Destination, EmulatorChoice, EmulatorId, EntryForm, FrontendMode, GameIds,
    MasterAddon, MasterEntry, MasterGame, Placement, PlacementTo, PlacementView, TranslateNote,
    TranslateScope, TranslateTarget, TranslatedGame, TranslatedGameView,
};

use super::addons::AddonCandidate;
use super::plan::{
    effective_kind, title_group, GameRole, ItemKind, ItemNote, LeftReason, OrganizeAction,
    OrganizeError, OrganizeItem, OrganizeOptions, SurveyEntry, Transfer,
};
use super::roots::{anchor_of, OsRootProbe, TargetAnchor};
use super::target::{check_lengths, folder_dir, DirIndex, TargetLayout};

/// An emulator's own data folder a plan writes into (D16, D17): apply
/// creates it by the D19 rule when it did not exist at plan time and
/// journals it as an extra root.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmulatorRoot {
    pub emulator: EmulatorId,
    /// Absolute, as the run named it.
    pub path: PathBuf,
    /// Its nearest existing ancestor when it did not exist at plan time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<TargetAnchor>,
}

/// Why a run's emulator data folder is refused (D16).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EmulatorDataWhy {
    /// Not an absolute path.
    Relative,
    /// The target root itself.
    IsTarget,
    /// Inside the pile.
    InsideSource,
    /// Console keys or licences sit at its top (never touched).
    HoldsKeys,
}

impl std::fmt::Display for EmulatorDataWhy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            EmulatorDataWhy::Relative => "not an absolute path",
            EmulatorDataWhy::IsTarget => "it is the target folder",
            EmulatorDataWhy::InsideSource => "it is inside the source folder",
            EmulatorDataWhy::HoldsKeys => "it holds console keys",
        })
    }
}

/// `a` and `b` name the same folder (resolved when both exist).
fn same_dir(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a.components().eq(b.components()),
    }
}

/// `inner` is `outer` or below it (resolved when both exist).
fn within(inner: &Path, outer: &Path) -> bool {
    let resolve = |p: &Path| fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    // A folder that does not exist yet: its nearest existing ancestor
    // resolved, plus the rest.
    let mut probe = inner.to_path_buf();
    let mut rest = Vec::new();
    while !probe.exists() {
        match (probe.file_name().map(ToOwned::to_owned), probe.parent()) {
            (Some(name), Some(parent)) => {
                rest.push(name);
                probe = parent.to_path_buf();
            }
            _ => break,
        }
    }
    let mut inner = resolve(&probe);
    for name in rest.into_iter().rev() {
        inner.push(name);
    }
    inner.starts_with(resolve(outer))
}

/// Refuses an emulator data folder the run may not write into (D16): a
/// relative path, the target root itself, a folder inside the pile, or
/// one holding key material by name at its top (never opened).
pub(crate) fn check_emulator_data(
    opts: &OrganizeOptions,
    source_root: &Path,
    target_root: &Path,
) -> Result<(), OrganizeError> {
    for (&emulator, path) in &opts.emulator_data {
        let why = if !path.is_absolute() {
            Some(EmulatorDataWhy::Relative)
        } else if same_dir(path, target_root) {
            Some(EmulatorDataWhy::IsTarget)
        } else if within(path, source_root) {
            Some(EmulatorDataWhy::InsideSource)
        } else if fs::read_dir(path).is_ok_and(|entries| {
            entries
                .flatten()
                .any(|e| crate::consoles::is_key_material(&e.file_name().to_string_lossy()))
        }) {
            Some(EmulatorDataWhy::HoldsKeys)
        } else {
            None
        };
        if let Some(why) = why {
            return Err(OrganizeError::EmulatorData {
                emulator,
                path: path.clone(),
                why,
            });
        }
    }
    Ok(())
}

/// The emulator `system` is laid out for in this run and how far its
/// translator may go (D16): the run's choice (full; `none` = no
/// translator), else the profile's default (always conservative, Q11).
/// A run's emulator that does not run the system is ignored (the default
/// applies), and so is a default that does not.
pub(crate) fn emulator_for(
    system: &str,
    opts: &OrganizeOptions,
    layout: &TargetLayout,
) -> Option<(EmulatorId, TranslateScope)> {
    let runs = |e: &EmulatorId| e.systems().contains(&system);
    match opts.emulators.get(system) {
        Some(EmulatorChoice::None) => None,
        Some(EmulatorChoice::Emulator(e)) if runs(e) => Some((*e, TranslateScope::Full)),
        _ => layout
            .profile
            .emulators
            .get(system)
            .filter(|e| runs(e))
            .map(|e| (*e, TranslateScope::Conservative)),
    }
}

/// What the pass needs of the plan around it.
pub(crate) struct TranslateWorld<'a> {
    pub layout: &'a TargetLayout,
    pub db: &'a SystemDb,
    pub opts: &'a OrganizeOptions,
    pub entries: &'a [SurveyEntry],
    pub addons: &'a [AddonCandidate],
    /// The source root (absolute).
    pub src: &'a Path,
}

/// What the pass returns for the plan.
#[derive(Debug, Default)]
pub(crate) struct Translated {
    pub views: Vec<TranslatedGameView>,
    pub roots: Vec<EmulatorRoot>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Member {
    Base,
    Update,
    Dlc,
    Addon,
}

/// One master-shaped group of the plan.
struct Group {
    system: String,
    /// The base item, when it is in the plan.
    base: Option<usize>,
    /// A library base's title (addons linked to a base already there).
    library_title: Option<String>,
    members: Vec<(usize, Member)>,
}

/// A game role an addon row has.
fn is_addon_role(item: &OrganizeItem) -> bool {
    item.game
        .as_ref()
        .is_some_and(|g| matches!(g.role, GameRole::Mod | GameRole::Cheat))
}

/// An addon row the pass may still place: moving in, or left only
/// because the layout had no place for it.
fn addon_open(item: &OrganizeItem) -> bool {
    item.moves_in()
        || matches!(
            item.action,
            OrganizeAction::Left {
                reason: LeftReason::AddonNoPlace { .. }
            }
        )
}

/// The title a library-linked addon names its base by.
fn library_title(item: &OrganizeItem) -> Option<String> {
    item.notes.iter().find_map(|n| match n {
        ItemNote::ModOf { of } | ItemNote::PatchOf { of } | ItemNote::CheatOf { of } => {
            Some(of.clone())
        }
        _ => None,
    })
}

/// Bases with their linked children by title id or by `game.of`, and
/// addons linked to library bases, in plan order.
fn groups(items: &[OrganizeItem], entries: &[SurveyEntry]) -> Vec<Group> {
    let child_role = |item: &OrganizeItem| {
        item.game
            .as_ref()
            .is_some_and(|g| g.role != GameRole::Base && g.role != GameRole::Demo)
    };
    let mut bases: HashMap<(String, String), usize> = HashMap::new();
    for (i, item) in items.iter().enumerate() {
        let Some(((system, id), kind)) = entries.get(i).and_then(title_group) else {
            continue;
        };
        if kind == TitleKind::Base
            && item.moves_in()
            && !child_role(item)
            && item.system_id.as_deref() == Some(system.as_str())
        {
            bases.entry((system, id)).or_insert(i);
        }
    }
    let mut by_base: BTreeMap<usize, Vec<(usize, Member)>> = BTreeMap::new();
    let mut library: BTreeMap<(String, String), Vec<(usize, Member)>> = BTreeMap::new();
    for (j, item) in items.iter().enumerate() {
        if is_addon_role(item) {
            if !addon_open(item) {
                continue;
            }
            let link = item.game.as_ref().expect("an addon role has a link");
            match link.of {
                Some(b) if items.get(b).is_some_and(OrganizeItem::moves_in) => {
                    by_base.entry(b).or_default().push((j, Member::Addon));
                }
                Some(_) => {}
                None => {
                    if let (Some(system), Some(title)) =
                        (item.system_id.clone(), library_title(item))
                    {
                        library
                            .entry((system, title))
                            .or_default()
                            .push((j, Member::Addon));
                    }
                }
            }
            continue;
        }
        if !item.moves_in() {
            continue;
        }
        let linked = item.game.as_ref().and_then(|g| match g.role {
            GameRole::Update => Some((g.of?, Member::Update)),
            GameRole::Dlc => Some((g.of?, Member::Dlc)),
            _ => None,
        });
        let titled = || {
            let entry = entries.get(j)?;
            let ((system, id), _) = title_group(entry)?;
            let member = match effective_kind(entry) {
                TitleKind::Update => Member::Update,
                TitleKind::Dlc => Member::Dlc,
                _ => return None,
            };
            let b = *bases.get(&(system.clone(), id))?;
            (item.system_id.as_deref() == Some(system.as_str())).then_some((b, member))
        };
        if let Some((b, member)) = linked.or_else(titled).filter(|(b, _)| *b != j) {
            if items.get(b).is_some_and(OrganizeItem::moves_in) {
                by_base.entry(b).or_default().push((j, member));
            }
        }
    }
    let mut out: Vec<Group> = by_base
        .into_iter()
        .filter_map(|(b, mut members)| {
            let system = items[b].system_id.clone()?;
            members.insert(0, (b, Member::Base));
            Some(Group {
                system,
                base: Some(b),
                library_title: None,
                members,
            })
        })
        .collect();
    out.extend(library.into_iter().map(|((system, title), members)| Group {
        system,
        base: None,
        library_title: Some(title),
        members,
    }));
    out
}

/// `path` relative to `dir` when it is below it.
fn below(path: &Path, dir: &Path) -> Option<PathBuf> {
    path.strip_prefix(dir)
        .ok()
        .filter(|r| !r.as_os_str().is_empty())
        .map(Path::to_path_buf)
}

fn name_of(path: &Path) -> Option<String> {
    path.file_name().map(|n| n.to_string_lossy().into_owned())
}

/// Where a member sits in the master layout of this plan: its target
/// relative to the system folder, else its own name.
fn master_path(item: &OrganizeItem, layout: &TargetLayout, system_dir: &Path) -> Option<PathBuf> {
    item.target
        .as_ref()
        .and_then(|t| below(&layout.target_root.join(t), system_dir))
        .or_else(|| {
            item.sources
                .first()
                .and_then(|s| name_of(s))
                .map(PathBuf::from)
        })
}

/// An installable package by its extension.
fn is_package(path: &Path) -> bool {
    path.extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .is_some_and(|e| matches!(e.as_str(), "pkg" | "cia" | "nsp" | "nsz" | "wua" | "vpk"))
}

fn form_of(item: &OrganizeItem, path: &Path) -> EntryForm {
    if item.kind == ItemKind::Folder {
        EntryForm::Extracted
    } else if is_package(path) || item.notes.contains(&ItemNote::ExtractFirst) {
        EntryForm::Package
    } else if item.kind == ItemKind::Archive {
        EntryForm::File
    } else {
        EntryForm::Image
    }
}

/// The game's ids, each validated later by the translator that uses it.
fn ids_of(entry: Option<&SurveyEntry>, item: &OrganizeItem) -> GameIds {
    let console = entry.and_then(|e| e.console.as_ref());
    let title_id = console.and_then(|c| c.title_id.clone());
    let serial = console
        .and_then(|c| c.serial.clone())
        .or_else(|| item.disc.as_ref().and_then(|d| d.serials.first().cloned()));
    let game_id = [title_id.as_deref(), serial.as_deref()]
        .into_iter()
        .flatten()
        .find_map(|s| valid_game_id(&s.replace(['-', '_', '.'], "")));
    GameIds {
        title_id,
        serial,
        game_id,
    }
}

/// The version order an update compares by (newest wins, D24).
fn version_order(entry: Option<&SurveyEntry>) -> Option<Vec<u64>> {
    let c = entry?.console.as_ref()?;
    crate::dedupe::keeper::version_order(&c.system, c.version.as_deref()?)
}

/// The master game of one group, and which item each `from` path is.
fn master_game(
    group: &Group,
    items: &[OrganizeItem],
    w: &TranslateWorld<'_>,
    system_dir: &Path,
) -> Option<(MasterGame, HashMap<PathBuf, usize>)> {
    let mut from: HashMap<PathBuf, usize> = HashMap::new();
    let mut note = |path: PathBuf, i: usize| from.insert(path, i).is_none();
    let (base, title, ids) = match group.base {
        Some(b) => {
            let item = &items[b];
            let path = master_path(item, w.layout, system_dir)?;
            let title = item
                .game
                .as_ref()
                .and_then(|g| g.folder.as_deref())
                .and_then(name_of)
                .or_else(|| {
                    let name = name_of(&path)?;
                    Some(match name.rsplit_once('.') {
                        Some((stem, _)) if item.kind != ItemKind::Folder => stem.to_string(),
                        _ => name,
                    })
                })?;
            let entry = MasterEntry {
                form: form_of(item, &path),
                is_dir: item.kind == ItemKind::Folder,
                path: path.clone(),
                title_id: None,
                version: None,
                newest: false,
            };
            if !note(path, b) {
                return None;
            }
            (entry, title, ids_of(w.entries.get(b), item))
        }
        None => {
            let title = group.library_title.clone()?;
            // A base already in the library is no item: it is never
            // placed, and a rule that needs its file name does not apply.
            let entry = MasterEntry {
                path: PathBuf::from(&title),
                is_dir: true,
                form: EntryForm::File,
                title_id: None,
                version: None,
                newest: false,
            };
            (entry, title, GameIds::default())
        }
    };
    let mut game = MasterGame {
        system: group.system.clone(),
        title,
        ids,
        base,
        updates: Vec::new(),
        dlc: Vec::new(),
        mods: Vec::new(),
        cheats: Vec::new(),
        taken_in_emulator: Vec::new(),
    };
    // Updates: the newest by version, or the one game folders marked.
    let newest = group
        .members
        .iter()
        .filter(|(_, m)| *m == Member::Update)
        .filter_map(|(j, _)| Some((version_order(w.entries.get(*j))?, *j)))
        .max()
        .map(|(_, j)| j);
    for &(j, member) in &group.members {
        let item = &items[j];
        match member {
            Member::Base => {}
            Member::Update | Member::Dlc => {
                let path = master_path(item, w.layout, system_dir)?;
                let console = w.entries.get(j).and_then(|e| e.console.as_ref());
                let entry = MasterEntry {
                    form: form_of(item, &path),
                    is_dir: item.kind == ItemKind::Folder,
                    path: path.clone(),
                    title_id: console.and_then(|c| c.title_id.clone()),
                    version: console.and_then(|c| c.version.clone()),
                    newest: Some(j) == newest || item.game.as_ref().is_some_and(|g| g.newest_here),
                };
                if !note(path, j) {
                    return None;
                }
                if member == Member::Update {
                    game.updates.push(entry);
                } else {
                    game.dlc.push(entry);
                }
            }
            Member::Addon => {
                let link = item.game.as_ref()?.addon.as_ref()?;
                let abs = w.src.join(item.sources.first()?);
                let candidate = w.addons.iter().find(|c| c.path == abs)?;
                let path = master_path(item, w.layout, system_dir)?;
                let addon = MasterAddon {
                    path: path.clone(),
                    is_dir: candidate.is_dir,
                    kind: link.kind,
                    format: link.format,
                    key: candidate.key.clone(),
                    header: link.header,
                };
                if !note(path, j) {
                    return None;
                }
                if link.kind == AddonKind::Cheat {
                    game.cheats.push(addon);
                } else {
                    game.mods.push(addon);
                }
            }
        }
    }
    Some((game, from))
}

/// The emulator data folder the run named for `emulator`, when it can be
/// written: there, or missing under an existing folder (D19).
fn data_root(
    emulator: EmulatorId,
    w: &TranslateWorld<'_>,
    roots: &mut BTreeMap<EmulatorId, EmulatorRoot>,
) -> Option<PathBuf> {
    if let Some(root) = roots.get(&emulator) {
        return Some(root.path.clone());
    }
    let path = w.opts.emulator_data.get(&emulator)?;
    // A file of that name is no folder to write into.
    if path.symlink_metadata().is_ok_and(|m| !m.is_dir()) {
        return None;
    }
    let anchor = if path.is_dir() {
        None
    } else {
        Some(anchor_of(path, &OsRootProbe)?)
    };
    roots.insert(
        emulator,
        EmulatorRoot {
            emulator,
            path: path.clone(),
            anchor,
        },
    );
    Some(path.clone())
}

/// What happened when a member was given a new place.
enum Placed {
    /// It moves there now.
    Moved,
    /// It keeps the place it had.
    Kept,
    /// The place is taken (or too long): `reason` for an addon.
    Blocked(LeftReason),
}

fn same_place(a: &Path, b: &Path) -> bool {
    a.components().eq(b.components())
}

/// Gives `item` the target `abs` (under the target root or an emulator
/// data root `base`): a place taken on disk or by an earlier item is never
/// written over.
fn place_at(
    item: &mut OrganizeItem,
    abs: &Path,
    base: &Path,
    layout: &TargetLayout,
    index: &mut DirIndex,
    same_content: &dyn Fn(&Path, &OrganizeItem) -> bool,
) -> Placed {
    if item.kind == ItemKind::Set || !item.set_targets.is_empty() || item.transfer != Transfer::AsIs
    {
        return Placed::Kept;
    }
    let current = item.target.as_ref().map(|t| layout.target_root.join(t));
    if current.as_deref().is_some_and(|c| same_place(c, abs)) {
        return Placed::Kept;
    }
    let (Some(dir), Some(name)) = (abs.parent(), name_of(abs)) else {
        return Placed::Kept;
    };
    if let Some(reason) = check_lengths(base, abs, layout.windows_paths) {
        return Placed::Blocked(reason);
    }
    let shown = |p: &Path| below(p, &layout.target_root).unwrap_or_else(|| p.to_path_buf());
    if index.taken(dir, &name) {
        let present = item.kind == ItemKind::File
            && index
                .existing(dir, &name)
                .is_some_and(|e| same_content(&e, item));
        return Placed::Blocked(if present {
            LeftReason::AlreadyPresent { at: shown(abs) }
        } else {
            LeftReason::AddonNameTaken { at: shown(dir) }
        });
    }
    // Every folder between the base and the place is claimed too, so no
    // later item takes a file of that name.
    let mut parent = abs.parent();
    while let Some(p) = parent.filter(|p| p.starts_with(base) && *p != base) {
        if let (Some(up), Some(n)) = (p.parent(), name_of(p)) {
            index.claim(up, &n);
        }
        parent = p.parent();
    }
    index.claim(dir, &name);
    let old_dir = current
        .as_deref()
        .and_then(Path::parent)
        .map(Path::to_path_buf);
    let new_target = shown(abs);
    if let (Some(old_dir), Some(new_dir)) = (old_dir, new_target.parent()) {
        for c in &mut item.companions {
            let c_dir = layout.target_root.join(&c.target);
            if c_dir.parent().is_some_and(|d| same_place(d, &old_dir)) {
                if let Some(n) = name_of(&c.target) {
                    c.target = new_dir.join(n);
                }
            }
        }
    }
    item.target = Some(new_target);
    if item.is_left() {
        item.action = OrganizeAction::Auto;
    }
    Placed::Moved
}

fn push_note(item: &mut OrganizeItem, note: ItemNote) {
    if !item.notes.contains(&note) {
        item.notes.push(note);
    }
}

/// The note a translator note becomes on a row that still moves.
fn item_note(note: &TranslateNote) -> Option<ItemNote> {
    match note {
        TranslateNote::InstallInEmulator { emulator } => Some(ItemNote::InstallFirst {
            emulator: emulator.display_name().to_string(),
        }),
        TranslateNote::ExtractFirst => Some(ItemNote::ExtractFirst),
        TranslateNote::HeaderMismatch { needs } => Some(ItemNote::HeaderMismatch { needs: *needs }),
        TranslateNote::PointEmulatorHere { emulator, folder } => {
            Some(ItemNote::PointEmulatorHere {
                emulator: emulator.display_name().to_string(),
                folder: folder.to_string_lossy().into_owned(),
            })
        }
        TranslateNote::NotSupported { .. } | TranslateNote::PatchNotApplied { .. } => None,
    }
}

/// The reason a translator note leaves a row that has no place.
fn left_reason(note: &TranslateNote) -> Option<LeftReason> {
    match note {
        TranslateNote::NotSupported { emulator, why } => Some(LeftReason::NotSupported {
            emulator: emulator.display_name().to_string(),
            why: *why,
        }),
        TranslateNote::PatchNotApplied { emulator } => Some(LeftReason::PatchNotApplied {
            emulator: emulator.display_name().to_string(),
        }),
        _ => None,
    }
}

/// The `System(..)` placement of the base in a translated game.
fn base_placement<'a>(laid: &'a TranslatedGame, base: &MasterEntry) -> Option<&'a Path> {
    laid.entries
        .iter()
        .find(|p| p.from == base.path)
        .and_then(|p| match &p.to {
            Destination::System(at) => Some(at.as_path()),
            _ => None,
        })
}

/// ES-DE «directories as files» renamed the base's game folder (only the
/// folder: same file name, same depth).
fn folder_renamed(from: &Path, to: &Path) -> bool {
    !same_place(from, to)
        && from.file_name() == to.file_name()
        && from.components().count() == to.components().count()
        && from.components().count() == 2
}

/// The pass (D16): lays out every master-shaped group of a system with an
/// emulator and turns the placements into targets, notes and reasons.
pub(crate) fn apply(
    items: &mut [OrganizeItem],
    w: &TranslateWorld<'_>,
    index: &mut DirIndex,
    same_content: &dyn Fn(&Path, &OrganizeItem) -> bool,
) -> Translated {
    let mut out = Translated::default();
    let mut roots: BTreeMap<EmulatorId, EmulatorRoot> = BTreeMap::new();
    let frontend = if w.layout.profile.id == "esde" {
        FrontendMode::DirAsGame
    } else {
        FrontendMode::Plain
    };
    for group in groups(items, w.entries) {
        let Some((emulator, scope)) = emulator_for(&group.system, w.opts, w.layout) else {
            continue;
        };
        let (system_dir, _) = folder_dir(w.layout, w.db, &group.system);
        let Some((mut game, from)) = master_game(&group, items, w, &system_dir) else {
            continue;
        };
        let target = TranslateTarget {
            emulator: Some(emulator),
            frontend,
            game_folders: w.layout.game_folders,
            scope,
        };
        let mut laid = translate(&game, &target);
        // A name the emulator data folder already has is never written
        // over (D15, Q5): laid out again knowing it.
        if let Some(root) = w.opts.emulator_data.get(&emulator) {
            for p in &laid.entries {
                if let Destination::EmulatorData { path, .. } = &p.to {
                    if root.join(path).symlink_metadata().is_ok() {
                        game.taken_in_emulator.push(path.clone());
                    }
                }
            }
            if !game.taken_in_emulator.is_empty() {
                laid = translate(&game, &target);
            }
        }
        let full = scope == TranslateScope::Full;
        // The base: kept, except a game folder ES-DE shows as a file.
        let laid_base = base_placement(&laid, &game.base).map(Path::to_path_buf);
        let mut base_dir = None;
        if let Some(b) = group.base {
            let base_now = items[b]
                .target
                .as_ref()
                .map(|t| w.layout.target_root.join(t));
            base_dir = base_now
                .as_deref()
                .and_then(Path::parent)
                .map(Path::to_path_buf);
            let renamed = laid_base
                .as_deref()
                .filter(|to| full && folder_renamed(&game.base.path, to));
            let folder_new = items[b]
                .game
                .as_ref()
                .is_some_and(|g| g.folder.is_some() && !g.folder_exists);
            if let Some(to) = renamed.filter(|_| folder_new) {
                let abs = system_dir.join(to);
                let old_folder = items[b].game.as_ref().and_then(|g| g.folder.clone());
                if let Placed::Moved = place_at(
                    &mut items[b],
                    &abs,
                    &system_dir,
                    w.layout,
                    index,
                    same_content,
                ) {
                    let new_folder = abs.parent().and_then(|d| below(d, &w.layout.target_root));
                    let (old_abs, new_abs) = match (&old_folder, abs.parent()) {
                        (Some(old), Some(new)) => {
                            (w.layout.target_root.join(old), new.to_path_buf())
                        }
                        _ => (PathBuf::new(), PathBuf::new()),
                    };
                    // Everything that was going into the old folder follows
                    // it: the base's companions, then its children.
                    let moved = |t: &Path| -> Option<PathBuf> {
                        let rest = w.layout.target_root.join(t);
                        let rest = rest.strip_prefix(&old_abs).ok()?;
                        Some(new_abs.join(rest))
                    };
                    if !old_abs.as_os_str().is_empty() {
                        for c in &mut items[b].companions {
                            if let Some(to) = moved(&c.target) {
                                c.target = below(&to, &w.layout.target_root).unwrap_or(to);
                            }
                        }
                        for &(j, _) in group.members.iter().filter(|(j, _)| *j != b) {
                            if let Some(to) = items[j].target.as_deref().and_then(moved) {
                                let _ = place_at(
                                    &mut items[j],
                                    &to,
                                    &system_dir,
                                    w.layout,
                                    index,
                                    same_content,
                                );
                            }
                        }
                    }
                    for &(j, _) in &group.members {
                        if let Some(g) = items[j].game.as_mut() {
                            if g.folder == old_folder {
                                g.folder.clone_from(&new_folder);
                            }
                        }
                    }
                    base_dir = abs.parent().map(Path::to_path_buf);
                }
            }
        }
        let laid_base_dir = laid_base
            .as_deref()
            .and_then(Path::parent)
            .map(Path::to_path_buf);
        let mut views: Vec<PlacementView> = Vec::new();
        let mut noted = false;
        for placement in &laid.entries {
            let Some(&j) = from.get(&placement.from) else {
                continue;
            };
            let member = group
                .members
                .iter()
                .find(|(m, _)| *m == j)
                .map_or(Member::Addon, |(_, m)| *m);
            noted |= placement.note.is_some();
            if member != Member::Base {
                place_member(
                    &mut items[j],
                    member,
                    placement,
                    PlaceCtx {
                        full,
                        library: group.base.is_none(),
                        system_dir: &system_dir,
                        base_dir: base_dir.as_deref(),
                        laid_base_dir: laid_base_dir.as_deref(),
                    },
                    w,
                    &mut roots,
                    index,
                    same_content,
                );
            } else if let Some(note) = placement.note.as_ref().and_then(item_note) {
                push_note(&mut items[j], note);
            }
            views.push(PlacementView {
                from: placement.from.clone(),
                to: shown_place(&items[j], w.layout, &system_dir, &roots),
                note: placement.note.clone(),
            });
        }
        let has_addon = group.members.iter().any(|(_, m)| *m == Member::Addon);
        if full || has_addon || noted {
            out.views.push(TranslatedGameView {
                system: group.system.clone(),
                base: group.base,
                emulator,
                placements: views,
            });
        }
    }
    // Only the roots something goes into.
    out.roots = roots
        .into_values()
        .filter(|r| {
            items
                .iter()
                .any(|i| i.target.as_deref().is_some_and(|t| t.starts_with(&r.path)))
        })
        .collect();
    out
}

#[derive(Clone, Copy)]
struct PlaceCtx<'a> {
    full: bool,
    /// The group's base is in the library, not an item.
    library: bool,
    system_dir: &'a Path,
    /// The folder the base's item lands in.
    base_dir: Option<&'a Path>,
    /// The folder the translator put the base in (relative to the system
    /// folder).
    laid_base_dir: Option<&'a Path>,
}

impl PlaceCtx<'_> {
    /// Where a `System(path)` placement lands: beside the base when the
    /// translator put it beside the base, else under the system folder.
    fn on_system(&self, path: &Path) -> Option<PathBuf> {
        let name = path.file_name()?;
        match (self.base_dir, self.laid_base_dir, path.parent()) {
            (Some(dir), Some(laid), Some(parent)) if same_place(parent, laid) => {
                Some(dir.join(name))
            }
            _ => Some(self.system_dir.join(path)),
        }
    }
}

/// One update, DLC or addon (D16): see the module documentation.
#[allow(clippy::too_many_arguments)]
fn place_member(
    item: &mut OrganizeItem,
    member: Member,
    placement: &Placement,
    ctx: PlaceCtx<'_>,
    w: &TranslateWorld<'_>,
    roots: &mut BTreeMap<EmulatorId, EmulatorRoot>,
    index: &mut DirIndex,
    same_content: &dyn Fn(&Path, &OrganizeItem) -> bool,
) {
    let addon = member == Member::Addon;
    let note = placement.note.as_ref();
    if !addon {
        // A game entry: notes always; a new place in the full scope only.
        match note {
            Some(n @ TranslateNote::NotSupported { .. }) if ctx.full => {
                if let Some(reason) = left_reason(n) {
                    item.leave(reason);
                }
                return;
            }
            Some(n) => {
                if let Some(note) = item_note(n) {
                    push_note(item, note);
                }
            }
            None => {}
        }
        if let (true, Destination::System(path)) = (ctx.full, &placement.to) {
            if let Some(abs) = ctx.on_system(path) {
                // A taken place keeps today's target: nothing is lost.
                let _ = place_at(item, &abs, ctx.system_dir, w.layout, index, same_content);
            }
        }
        return;
    }
    if let Some(note) = note.and_then(item_note) {
        push_note(item, note);
    }
    match &placement.to {
        Destination::System(path) => {
            if ctx.library {
                return;
            }
            let Some(abs) = ctx.on_system(path) else {
                return;
            };
            if let Placed::Blocked(reason) =
                place_at(item, &abs, ctx.system_dir, w.layout, index, same_content)
            {
                item.leave(reason);
            }
        }
        Destination::EmulatorData { emulator, path } => match data_root(*emulator, w, roots) {
            Some(root) => {
                let abs = root.join(path);
                if let Placed::Blocked(reason) =
                    place_at(item, &abs, &root, w.layout, index, same_content)
                {
                    item.leave(reason);
                }
            }
            // Its game folder's `Mods/` or `Cheats/` when it has one.
            None if item.moves_in() && item.target.is_some() => {}
            None => item.leave(LeftReason::AddonForEmulator {
                emulator: emulator.display_name().to_string(),
                path: path.clone(),
            }),
        },
        Destination::Library => {
            // Today's place, when it has one; else the reason it stays.
            let has_place = item.moves_in() && item.target.is_some();
            if !has_place {
                if let Some(reason) = note.and_then(left_reason) {
                    item.leave(reason);
                }
            }
        }
    }
}

/// Where the preview shows a member once the pass is done.
fn shown_place(
    item: &OrganizeItem,
    layout: &TargetLayout,
    system_dir: &Path,
    roots: &BTreeMap<EmulatorId, EmulatorRoot>,
) -> PlacementTo {
    if let OrganizeAction::Left {
        reason: LeftReason::AddonForEmulator { path, .. },
    } = &item.action
    {
        return PlacementTo::EmulatorData { path: path.clone() };
    }
    let Some(target) = item.target.as_ref().filter(|_| item.moves_in()) else {
        return PlacementTo::Library;
    };
    // An emulator data folder may sit outside the target root (an
    // absolute target) or below it (a handheld card holding both).
    let abs = layout.target_root.join(target);
    if let Some(path) = roots.values().find_map(|r| below(&abs, &r.path)) {
        return PlacementTo::EmulatorData { path };
    }
    match below(&abs, system_dir) {
        Some(path) => PlacementTo::System { path },
        None => PlacementTo::System {
            path: target.clone(),
        },
    }
}
