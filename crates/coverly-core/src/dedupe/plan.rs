//! `plan_dedupe` (duplicates spec §12.1): a whole library as a dry run.
//!
//! The pipeline is units → level 1 → catalogues (downloads capped by
//! `dedupe.max_dat_downloads`) → serials (level 3) → level 2 → groups →
//! saves → keeper → companions → destinations → totals. Nothing in the
//! library is written: the cache keeps what was read, so a warm run reads
//! nothing, and a cancelled run keeps what it had read.
//!
//! A plan carries what the planner knew ([`PlanInternals`]), so changing
//! the options ([`regroup`]), picking a keeper ([`set_keeper`]) or skipping
//! a group ([`skip_group`]) never reads a ROM again.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::cache::{Cache, CacheError, StoredChoice};
use crate::identify::dat::{dat_sources, DatStore};
use crate::identify::hash::hash_stream;
use crate::identify::names::{parse_tags, NameTags, Region};
use crate::profiles::Profile;
use crate::scan::LibraryScan;
use crate::settings::{ContainerClass, Settings};
use crate::systems::SystemDb;
use crate::write::{name_key, on_removable_volume, same_volume};

use super::companions::{companions_with, has_art, CompanionCtx, DirListings, MemberCompanions};
use super::group::{build_groups, GroupOpts, RawGroup};
use super::keeper::{
    choose, container_name, played_from_gamelist, region_order, remembered_pick_holds,
    saves_fingerprint, Candidate, ContainerOrder, Played,
};
use super::keys::{
    catalogue_crcs, level_one, level_three, level_two, workers_for, ExactKey, FileBytes, FormatKey,
    KeyCtx, KeyedUnit, VersionKey,
};
use super::saves::{conflict, off_card, OffCard, SaveIndex, SaveInfo};
use super::source::{CacheDats, DatHit, DatLookup, DiscProbe, EntryListing};
use super::units::{units_from_scan, Exclusion, Unit, UnitKind, UnitOptions};
use super::{
    group_id, DedupeOptions, DedupePhase, DedupePlan, DupGroup, GroupAction, Member,
    QuarantineChoice, ReportWhy, Totals, PLAN_VERSION,
};

/// The quarantine folder's name until the app fixes one from its language
/// (`settings.dedupe.quarantine_name`).
pub const DEFAULT_QUARANTINE_NAME: &str = "_Duplicados";

/// Cache namespace of the catalogue CRCs read for level 2.
const CRCS_NS: &str = "dedupe-crcs";

/// Where a plan is. Throttling is the caller's.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DedupeProgress {
    pub phase: DedupePhase,
    pub done: usize,
    pub total: usize,
    pub current: Option<PathBuf>,
}

/// What a plan reads with.
pub struct DedupeEnv<'a> {
    pub cache: &'a Cache,
    pub db: &'a SystemDb,
    pub profile: &'a Profile,
    pub settings: &'a Settings,
    /// Lists and decodes archive entries.
    pub listing: &'a dyn EntryListing,
    /// Reads disc serials and volume fingerprints.
    pub discs: &'a dyn DiscProbe,
    /// Downloads missing catalogues; `None` uses the cached ones only.
    pub dats: Option<&'a DatStore<'a>>,
    /// Opens loose files (a seam so tests can count reads).
    pub bytes: &'a dyn FileBytes,
    pub progress: &'a dyn Fn(DedupeProgress),
    pub cancel: &'a AtomicBool,
}

/// Why a plan, or a change to one, failed.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum DedupeError {
    /// Planning was cancelled; what was read stays in the cache.
    #[error("duplicates planning was cancelled")]
    Cancelled,
    #[error("no duplicates group {0} in the plan")]
    NoGroup(u64),
    #[error("duplicates group {group} has no member {member}")]
    NoMember { group: u64, member: usize },
    /// A conflict has no keeper, and some copies may never stay (the one
    /// arcade set with a valid name, the one catalogued header).
    #[error("member {member} of duplicates group {group} cannot be the copy that stays")]
    NotAKeeper { group: u64, member: usize },
    /// The plan was read back from disk without what the planner knew.
    #[error("the duplicates plan must be made again")]
    NoState,
    #[error("cache: {0}")]
    Cache(String),
    /// A custom quarantine may be anywhere except inside a system folder
    /// (spec §7.1), nor around one: its copies would be found again as
    /// games.
    #[error("the duplicates folder {} is inside or around a system folder", .0.display())]
    QuarantineInSystemFolder(PathBuf),
    /// Applying would set aside every copy of a group, or move the copy
    /// that stays (hard rule 4): nothing is written.
    #[error("duplicates group {0} would keep no copy")]
    EveryMemberMoves(u64),
}

impl From<CacheError> for DedupeError {
    fn from(e: CacheError) -> Self {
        DedupeError::Cache(e.to_string())
    }
}

/// Where the quarantine goes by default (spec §7.1).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuarantineSpot {
    /// The quarantine folder.
    pub dir: PathBuf,
    /// The writer's root: it holds both the ROMs folder and the quarantine.
    pub writer_root: PathBuf,
    /// The folder of system folders.
    pub roms_dir: PathBuf,
    /// Always false: the default quarantine is always inside the library
    /// (plan DD-11). Kept for older readers of the type.
    pub fallback: bool,
}

/// The default quarantine of the library at `root` (spec §7.1, plan
/// DD-11): always `<root>/<name>`, inside the library, so ROM moves, art
/// moves and `gamelist.xml` edits share one writer root and stay renames
/// on one volume. `/Volumes/SD/_Duplicados` for `roms_root = "ROMS"`;
/// `~/Documents/ROMS/_Duplicados` for a folder opened as system folders.
/// Walks, scans and later runs skip it (`Exclusion::Quarantine`).
pub fn default_quarantine(root: &Path, roms_root: &str, name: &str) -> QuarantineSpot {
    let roms_root = roms_root.trim_matches(|c| c == '/' || c == '\\');
    QuarantineSpot {
        dir: root.join(name),
        writer_root: root.to_path_buf(),
        roms_dir: if roms_root.is_empty() {
            root.to_path_buf()
        } else {
            root.join(roms_root)
        },
        fallback: false,
    }
}

/// What the planner knew when it made a plan. Opaque; two plans compare
/// equal whatever their internals.
#[derive(Clone, Default)]
pub struct PlanInternals(Option<Arc<PlanState>>);

impl fmt::Debug for PlanInternals {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self.0 {
            Some(_) => "PlanInternals(..)",
            None => "PlanInternals(none)",
        })
    }
}

impl PartialEq for PlanInternals {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

/// One plan's groups over the shared [`Base`].
struct PlanState {
    base: Arc<Base>,
    /// Raw groups with their ids, in plan order.
    raw: Vec<(u64, RawGroup)>,
    opts: DedupeOptions,
}

/// Everything read once: units, keys, catalogue answers, saves.
struct Base {
    root: PathBuf,
    roms_dir: PathBuf,
    spot: QuarantineSpot,
    /// The scanned system folders a quarantine may not overlap (empty when
    /// the units did not come from one library scan).
    system_dirs: Vec<PathBuf>,
    units: Vec<Unit>,
    l1: Vec<KeyedUnit>,
    l2: Vec<Option<VersionKey>>,
    l3: Vec<Option<FormatKey>>,
    /// The catalogue answers the grouping got.
    dats: Snapshot,
    db: SystemDb,
    profile: Profile,
    companion_exts: Vec<String>,
    regions: Vec<Region>,
    saves: SaveIndex,
    facts: Mutex<HashMap<usize, Arc<UnitFacts>>>,
    /// Units with a remembered pick → the saves fingerprint stored with it.
    remembered: HashMap<usize, String>,
    /// First dot-segment of each file stem (compared as FAT compares) →
    /// units: the only units that can share a companion or an image.
    stem_index: HashMap<String, Vec<usize>>,
    /// Folder listings for companions, read once per plan.
    dirs: Mutex<DirListings>,
    /// Units picked in this session, latest last.
    session: Mutex<Vec<usize>>,
    /// Groups you chose to skip.
    skipped: Mutex<HashSet<u64>>,
}

/// What the keeper rules know about one unit.
struct UnitFacts {
    saves: Vec<SaveInfo>,
    played: Option<Played>,
    has_cover: bool,
    tags: NameTags,
    dat_name: Option<String>,
    headered_functional: Option<bool>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Plans the duplicates of a scanned library. A dry run: nothing in the
/// library is written. Fails only on cancel (or a cache error when
/// resetting remembered choices).
pub fn plan_dedupe(
    scan: &LibraryScan,
    opts: &DedupeOptions,
    env: &DedupeEnv,
) -> Result<DedupePlan, DedupeError> {
    let report = |phase, done, total| {
        (env.progress)(DedupeProgress {
            phase,
            done,
            total,
            current: None,
        })
    };
    let check = || {
        if env.cancel.load(Ordering::Relaxed) {
            Err(DedupeError::Cancelled)
        } else {
            Ok(())
        }
    };
    let root = scan.root.clone();
    let name = env
        .settings
        .dedupe
        .quarantine_name
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .unwrap_or(DEFAULT_QUARANTINE_NAME);
    let spot = default_quarantine(&root, &env.profile.roms_root, name);
    let quarantine = quarantine_dir(&spot, &opts.quarantine);
    let system_dirs: Vec<&Path> = scan.systems.iter().map(|s| s.dir.as_path()).collect();
    check_quarantine(&quarantine, &system_dirs)?;

    // Units.
    let folders = scan.systems.len();
    report(DedupePhase::Listing, 0, folders);
    let scoped = scoped(scan, opts, &env.settings.dedupe.exclude_systems);
    let mut unit_opts = UnitOptions::for_profile(env.profile);
    unit_opts.quarantine = Some(quarantine.clone());
    let set = units_from_scan(&scoped, env.db, &unit_opts, env.listing);
    report(DedupePhase::Listing, folders, folders);
    check()?;
    let units = set.units;
    let systems: BTreeSet<String> = units.iter().map(|u| u.system.clone()).collect();
    if opts.reset_choices {
        env.cache
            .clear_dedupe_choices(&systems.iter().cloned().collect::<Vec<_>>())?;
    }

    // Level 1.
    let volume = crate::volumes::volume_uuid(&root);
    let ctx = KeyCtx {
        db: env.db,
        listing: env.listing,
        bytes: env.bytes,
        cache: Some(env.cache),
        volume_uuid: volume.as_deref(),
        max_hash_bytes: u64::from(env.settings.matching.max_hash_mb) * 1024 * 1024,
        workers: workers_for(&root),
    };
    let mut l1 = level_one(&units, &ctx, &report, env.cancel);
    check()?;

    // Catalogues, then the CRCs level 2 looks them up by.
    let catalogued = catalogues(&units, opts, env, &report);
    check()?;
    catalogue_crcs_for(&units, &mut l1, &catalogued, &ctx, env);
    check()?;

    // Levels 3 and 2, and the groups, recording every catalogue answer.
    let snapshot = Snapshot::default();
    let (l2, l3, raw) = {
        let cached = CacheDats::new(env.cache, env.db);
        let rec = Recorder {
            inner: &cached,
            snap: &snapshot,
        };
        report(DedupePhase::Serials, 0, units.len());
        let l3 = level_three(&units, env.discs, &rec);
        report(DedupePhase::Serials, units.len(), units.len());
        check()?;
        let l2 = level_two(&units, &l1, &rec);
        let raw = raw_groups(&units, &l1, &l2, &l3, env.db, &rec, opts);
        (l2, l3, raw)
    };

    let remembered = if opts.reset_choices {
        HashMap::new()
    } else {
        units
            .iter()
            .enumerate()
            .filter_map(|(i, u)| {
                let key = content_key(u, &l1[i], l2[i].as_ref(), l3[i].as_ref())?;
                let choice = env.cache.dedupe_choice(&u.system, &key).ok()??;
                Some((i, choice.saves_fp))
            })
            .collect()
    };

    let mut not_roms = Vec::new();
    let mut unreadable = Vec::new();
    for (path, why) in set.excluded {
        match why {
            Exclusion::NotARom => not_roms.push(path),
            Exclusion::Unreadable => unreadable.push(path),
            _ => {}
        }
    }
    let chd_unread = units
        .iter()
        .zip(&l3)
        .filter(|(u, k)| u.kind == UnitKind::Chd && k.is_none())
        .count();
    let no_catalogue = systems
        .iter()
        .filter(|s| !catalogued.contains(*s))
        .cloned()
        .collect();

    let index = stem_index(&units);
    let base = Arc::new(Base {
        root: root.clone(),
        roms_dir: spot.roms_dir.clone(),
        saves: SaveIndex::new(&env.profile.id, &root),
        spot,
        system_dirs: scan.systems.iter().map(|s| s.dir.clone()).collect(),
        units,
        l1,
        l2,
        l3,
        dats: snapshot,
        db: env.db.clone(),
        profile: env.profile.clone(),
        companion_exts: env.settings.organize.companion_exts.clone(),
        regions: region_order(&env.settings.matching.regions),
        facts: Mutex::new(HashMap::new()),
        stem_index: index,
        dirs: Mutex::new(DirListings::default()),
        remembered,
        session: Mutex::new(Vec::new()),
        skipped: Mutex::new(HashSet::new()),
    });
    let mut plan = DedupePlan {
        version: PLAN_VERSION,
        root,
        roms_dir: base.roms_dir.clone(),
        profile_id: env.profile.id.clone(),
        not_roms,
        unreadable,
        no_catalogue,
        chd_unread,
        ..DedupePlan::default()
    };
    fill(
        &mut plan,
        Arc::new(PlanState {
            base,
            raw,
            opts: opts.clone(),
        }),
    );
    Ok(plan)
}

/// What [`group_units`] reads with: the library the units are compared
/// against (saves, game lists and art are looked up there) and the
/// readers.
pub struct GroupEnv<'a> {
    pub cache: &'a Cache,
    pub db: &'a SystemDb,
    pub profile: &'a Profile,
    /// The library root.
    pub root: &'a Path,
    /// The folder of system folders.
    pub roms_dir: &'a Path,
    /// Lists and decodes archive entries.
    pub listing: &'a dyn EntryListing,
    /// Reads disc serials and volume fingerprints.
    pub discs: &'a dyn DiscProbe,
    /// Catalogue answers (cached catalogues only: nothing is downloaded).
    pub dats: &'a dyn DatLookup,
    /// `content_keys` rows are read and written for this volume only:
    /// `None` keeps the cache out (units on several volumes).
    pub volume_uuid: Option<&'a str>,
    /// Lower-case extensions that follow a ROM.
    pub companion_exts: &'a [String],
    /// `settings.matching.regions`.
    pub regions: &'a [String],
    pub cancel: &'a AtomicBool,
}

/// Groups `units` that may come from two places, a library and an
/// organise pile (`Unit::in_library`), with the planner's keys and keeper
/// rules (spec §4, Organise step). When a group mixes library and pile
/// copies, only a library copy may stay: a pile copy is never organised
/// over a game the library already holds. Extras get no `dest` (the
/// caller decides where they go). Nothing is written but the cache; fails
/// only on cancel.
pub fn group_units(
    units: Vec<Unit>,
    opts: &DedupeOptions,
    env: &GroupEnv,
) -> Result<Vec<DupGroup>, DedupeError> {
    let check = || {
        if env.cancel.load(Ordering::Relaxed) {
            Err(DedupeError::Cancelled)
        } else {
            Ok(())
        }
    };
    let units: Vec<Unit> = units
        .into_iter()
        .enumerate()
        .map(|(i, u)| Unit { id: i, ..u })
        .collect();
    let ctx = KeyCtx {
        db: env.db,
        listing: env.listing,
        bytes: &super::keys::DiskBytes,
        cache: Some(env.cache),
        volume_uuid: env.volume_uuid,
        max_hash_bytes: crate::identify::hash::DEFAULT_HASH_MAX_BYTES,
        workers: workers_for(env.root),
    };
    let l1 = level_one(&units, &ctx, &|_, _, _| {}, env.cancel);
    check()?;
    let snapshot = Snapshot::default();
    let (l2, l3, mut raw) = {
        let rec = Recorder {
            inner: env.dats,
            snap: &snapshot,
        };
        let l3 = level_three(&units, env.discs, &rec);
        check()?;
        let l2 = level_two(&units, &l1, &rec);
        let raw = raw_groups(&units, &l1, &l2, &l3, env.db, &rec, opts);
        (l2, l3, raw)
    };
    for (_, group) in &mut raw {
        library_keeps(&units, group);
    }
    let remembered = units
        .iter()
        .enumerate()
        .filter_map(|(i, u)| {
            let key = content_key(u, &l1[i], l2[i].as_ref(), l3[i].as_ref())?;
            let choice = env.cache.dedupe_choice(&u.system, &key).ok()??;
            Some((i, choice.saves_fp))
        })
        .collect();
    let spot = default_quarantine(env.root, &env.profile.roms_root, DEFAULT_QUARANTINE_NAME);
    let base = Base {
        root: env.root.to_path_buf(),
        roms_dir: env.roms_dir.to_path_buf(),
        saves: SaveIndex::new(&env.profile.id, env.root),
        spot,
        system_dirs: Vec::new(),
        stem_index: stem_index(&units),
        units,
        l1,
        l2,
        l3,
        dats: snapshot,
        db: env.db.clone(),
        profile: env.profile.clone(),
        companion_exts: env.companion_exts.to_vec(),
        regions: region_order(env.regions),
        facts: Mutex::new(HashMap::new()),
        dirs: Mutex::new(DirListings::default()),
        remembered,
        session: Mutex::new(Vec::new()),
        skipped: Mutex::new(HashSet::new()),
    };
    let no_dest = DedupeOptions {
        quarantine: QuarantineChoice::Trash,
        ..opts.clone()
    };
    Ok(raw
        .iter()
        .map(|(id, group)| base.build_group(*id, group, &no_dest))
        .collect())
}

/// In a group of library and pile copies, only a library copy may stay
/// (the copies the group already limited the keeper to, when one of them
/// is in the library).
fn library_keeps(units: &[Unit], group: &mut RawGroup) {
    let library: Vec<usize> = group
        .members
        .iter()
        .map(|(u, _)| *u)
        .filter(|&u| units[u].in_library)
        .collect();
    if library.is_empty() || library.len() == group.members.len() {
        return;
    }
    let allowed: Vec<usize> = match &group.keeper_only {
        Some(only) => library
            .iter()
            .copied()
            .filter(|u| only.contains(u))
            .collect(),
        None => library.clone(),
    };
    group.keeper_only = Some(if allowed.is_empty() { library } else { allowed });
}

/// The plan for other options, from what the planner already read: no ROM
/// is read again. A plan with no internals (read back from disk) comes
/// back unchanged. A new quarantine gets the same check as in [`plan`].
pub fn regroup(plan: &DedupePlan, opts: &DedupeOptions) -> Result<DedupePlan, DedupeError> {
    let Some(state) = &plan.internals.0 else {
        return Ok(plan.clone());
    };
    let base = Arc::clone(&state.base);
    let dirs: Vec<&Path> = base.system_dirs.iter().map(PathBuf::as_path).collect();
    check_quarantine(&quarantine_dir(&base.spot, &opts.quarantine), &dirs)?;
    let raw = raw_groups(
        &base.units,
        &base.l1,
        &base.l2,
        &base.l3,
        &base.db,
        &base.dats,
        opts,
    );
    let mut out = plan.clone();
    fill(
        &mut out,
        Arc::new(PlanState {
            base,
            raw,
            opts: opts.clone(),
        }),
    );
    Ok(out)
}

/// Makes `member` the copy that stays in `group`, and remembers the pick
/// by content for later runs (spec §9.3).
pub fn set_keeper(
    plan: &mut DedupePlan,
    group: u64,
    member: usize,
    cache: &Cache,
) -> Result<(), DedupeError> {
    let pos = plan
        .groups
        .iter()
        .position(|g| g.id == group)
        .ok_or(DedupeError::NoGroup(group))?;
    let g = &plan.groups[pos];
    if member >= g.members.len() {
        return Err(DedupeError::NoMember { group, member });
    }
    if matches!(g.action, GroupAction::Conflict { .. }) {
        return Err(DedupeError::NotAKeeper { group, member });
    }
    let state = plan.internals.0.clone().ok_or(DedupeError::NoState)?;
    let raw = state
        .raw
        .iter()
        .find(|(id, _)| *id == group)
        .map(|(_, r)| r)
        .ok_or(DedupeError::NoState)?;
    let unit = raw.members[member].0;
    if raw
        .keeper_only
        .as_ref()
        .is_some_and(|only| !only.contains(&unit))
    {
        return Err(DedupeError::NotAKeeper { group, member });
    }
    let base = &state.base;
    let keeper_key = base.content_key(unit).ok_or(DedupeError::NoState)?;
    let facts: Vec<Arc<UnitFacts>> = raw.members.iter().map(|(u, _)| base.facts_of(*u)).collect();
    let saves_fp = saves_fingerprint(&base.candidates(raw, &facts));
    // An older pick of another copy in this group would compete with this
    // one on the next run: only the latest pick is remembered.
    let others: Vec<String> = raw
        .members
        .iter()
        .filter(|(u, _)| *u != unit)
        .filter_map(|(u, _)| base.content_key(*u))
        .collect();
    cache.forget_dedupe_choices(&raw.system, &others)?;
    cache.put_dedupe_choice(&StoredChoice {
        system: raw.system.clone(),
        keeper_key,
        signature: raw.keys.join("\n"),
        saves_fp,
        chosen_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64),
    })?;
    lock(&base.session).push(unit);
    plan.groups[pos] = base.build_group(group, raw, &state.opts);
    plan.totals = totals(&plan.groups);
    Ok(())
}

/// Skips `group` (report-only, nothing moves) or takes it back.
pub fn skip_group(plan: &mut DedupePlan, group: u64, skip: bool) {
    if let Some(state) = &plan.internals.0 {
        let mut skipped = lock(&state.base.skipped);
        if skip {
            skipped.insert(group);
        } else {
            skipped.remove(&group);
        }
    }
    let Some(pos) = plan.groups.iter().position(|g| g.id == group) else {
        return;
    };
    let skipped = GroupAction::ReportOnly {
        why: ReportWhy::UserSkipped,
    };
    let g = &mut plan.groups[pos];
    if skip {
        if g.action == GroupAction::Act {
            g.action = skipped;
            for m in &mut g.members {
                m.dest = None;
            }
        }
    } else if g.action == skipped {
        let rebuilt = plan.internals.0.as_ref().and_then(|state| {
            let (_, raw) = state.raw.iter().find(|(id, _)| *id == group)?;
            Some(state.base.build_group(group, raw, &state.opts))
        });
        match rebuilt {
            Some(new) => *g = new,
            None => {
                g.action = GroupAction::Act;
                let keeper = g.keeper;
                let quarantine = !plan.trash;
                for m in g
                    .members
                    .iter_mut()
                    .filter(|m| quarantine && m.id != keeper)
                {
                    m.dest = Some(dest_for(
                        &plan.roms_dir,
                        &plan.quarantine,
                        &m.unit.primary,
                        &g.system,
                    ));
                }
            }
        }
    }
    plan.totals = totals(&plan.groups);
}

/// Builds the groups of `state` into `plan`, with the quarantine of its
/// options and the totals.
fn fill(plan: &mut DedupePlan, state: Arc<PlanState>) {
    let base = &state.base;
    plan.quarantine = quarantine_dir(&base.spot, &state.opts.quarantine);
    plan.trash = state.opts.quarantine == QuarantineChoice::Trash;
    plan.same_volume = state.opts.quarantine != QuarantineChoice::Trash
        && same_volume(&base.root, &plan.quarantine);
    plan.removable = on_removable_volume(&base.root);
    plan.groups = state
        .raw
        .iter()
        .map(|(id, raw)| base.build_group(*id, raw, &state.opts))
        .collect();
    plan.off_card_saves = if off_card(&base.profile.id) == OffCard::Internal {
        let systems: BTreeSet<String> = plan.groups.iter().map(|g| g.system.clone()).collect();
        systems.into_iter().collect()
    } else {
        Vec::new()
    };
    plan.totals = totals(&plan.groups);
    plan.internals = PlanInternals(Some(state));
}

impl Base {
    fn facts_of(&self, i: usize) -> Arc<UnitFacts> {
        if let Some(f) = lock(&self.facts).get(&i) {
            return Arc::clone(f);
        }
        let f = Arc::new(self.compute_facts(i));
        Arc::clone(lock(&self.facts).entry(i).or_insert(f))
    }

    fn compute_facts(&self, i: usize) -> UnitFacts {
        let u = &self.units[i];
        let serials = self.l3[i]
            .as_ref()
            .map(|k| k.serials.clone())
            .unwrap_or_default();
        let played = if self.profile.gamelist {
            let dir = system_dir(u);
            u.primary
                .strip_prefix(&dir)
                .ok()
                .and_then(|rel| played_from_gamelist(&dir, rel))
        } else {
            None
        };
        UnitFacts {
            saves: self.saves.saves_for(u, &serials),
            played,
            has_cover: has_art(u, &self.companion_ctx()),
            tags: self.l2[i]
                .as_ref()
                .map(|k| k.tags.clone())
                .unwrap_or_else(|| parse_tags(&u.stem)),
            dat_name: self.l2[i].as_ref().map(|k| k.game.clone()),
            headered_functional: headered(u, &self.l1[i]),
        }
    }

    fn companion_ctx(&self) -> CompanionCtx<'_> {
        CompanionCtx {
            root: &self.root,
            profile: &self.profile,
            db: &self.db,
            companion_exts: &self.companion_exts,
        }
    }

    /// [`super::companions::companions_for`] over the group and the units that could share a
    /// file with it, not the whole library: the result is the same, and a
    /// library of thousands of ROMs is not walked once per group.
    fn companions(&self, raw: &RawGroup, keeper: usize) -> Vec<MemberCompanions> {
        let mut scope: BTreeSet<usize> = raw.members.iter().map(|(u, _)| *u).collect();
        let members: Vec<usize> = scope.iter().copied().collect();
        for &u in &members {
            for key in stem_keys(&self.units[u]) {
                if let Some(list) = self.stem_index.get(&key) {
                    scope.extend(list.iter().copied());
                }
            }
        }
        let scope: Vec<usize> = scope.into_iter().collect();
        let at = |u: usize| scope.binary_search(&u).expect("in scope");
        let units: Vec<Unit> = scope.iter().map(|&u| self.units[u].clone()).collect();
        let local = RawGroup {
            members: raw.members.iter().map(|(u, r)| (at(*u), *r)).collect(),
            keeper_only: None,
            links: Vec::new(),
            ..raw.clone()
        };
        companions_with(
            &local,
            at(keeper),
            &units,
            &self.companion_ctx(),
            &mut lock(&self.dirs),
        )
    }

    fn content_key(&self, i: usize) -> Option<String> {
        content_key(
            &self.units[i],
            &self.l1[i],
            self.l2[i].as_ref(),
            self.l3[i].as_ref(),
        )
    }

    fn candidates<'s>(&'s self, raw: &RawGroup, facts: &'s [Arc<UnitFacts>]) -> Vec<Candidate<'s>> {
        raw.members
            .iter()
            .zip(facts)
            .enumerate()
            .map(|(m, ((u, relation), f))| Candidate {
                member: m,
                unit: &self.units[*u],
                saves: &f.saves,
                played: f.played.clone(),
                tags: Some(&f.tags),
                dat_name: f.dat_name.as_deref(),
                has_cover: f.has_cover,
                relation: *relation,
                headered_functional: f.headered_functional,
            })
            .collect()
    }

    /// The group as the plan shows it: keeper, reason, relations to the
    /// keeper, companions and destinations.
    fn build_group(&self, id: u64, raw: &RawGroup, opts: &DedupeOptions) -> DupGroup {
        let facts: Vec<Arc<UnitFacts>> =
            raw.members.iter().map(|(u, _)| self.facts_of(*u)).collect();
        let cands = self.candidates(raw, &facts);
        let allowed: Vec<Candidate> = match &raw.keeper_only {
            Some(only) => cands
                .iter()
                .filter(|c| only.contains(&raw.members[c.member].0))
                .cloned()
                .collect(),
            None => cands.clone(),
        };

        // A pick of this session first; a remembered one only while it
        // holds (spec §6.1).
        let session = lock(&self.session)
            .iter()
            .rev()
            .find_map(|u| raw.members.iter().position(|(m, _)| m == u));
        let mut choice_dropped = false;
        let user = session.or_else(|| {
            let (m, fp) = raw
                .members
                .iter()
                .enumerate()
                .find_map(|(m, (u, _))| Some((m, self.remembered.get(u)?)))?;
            if remembered_pick_holds(&cands, m, fp) {
                Some(m)
            } else {
                choice_dropped = true;
                None
            }
        });
        let forced = raw.members.iter().position(|(u, _)| {
            let unit = &self.units[*u];
            opts.forced_keepers
                .iter()
                .any(|p| unit.primary == *p || unit.files.contains(p))
        });
        let class = match self.db.get(&raw.system) {
            Some(s) if s.arcade => ContainerClass::Arcade,
            Some(s) if s.multi_file => ContainerClass::Disc,
            _ => ContainerClass::Cart,
        };
        let order =
            ContainerOrder::resolve(&opts.container_order, &self.profile.id, &raw.system, class);
        let (keeper, reason, runner_up, tie) =
            choose(&allowed, &opts.rules, &self.regions, &order, user, forced);

        let keeper_unit = raw.members[keeper].0;
        let relations = raw.relations_to(keeper_unit);
        let companions = self.companions(raw, keeper_unit);
        let saves: Vec<(usize, Vec<SaveInfo>)> = raw
            .members
            .iter()
            .zip(&facts)
            .map(|((u, _), f)| (*u, f.saves.clone()))
            .collect();
        let mut action = match conflict(&saves) {
            Some(saves) => GroupAction::Conflict { saves },
            None => raw.action.clone(),
        };
        if action == GroupAction::Act && lock(&self.skipped).contains(&id) {
            action = GroupAction::ReportOnly {
                why: ReportWhy::UserSkipped,
            };
        }
        let quarantine = match &opts.quarantine {
            QuarantineChoice::Trash => None,
            choice => Some(quarantine_dir(&self.spot, choice)),
        };

        let members: Vec<Member> = raw
            .members
            .iter()
            .zip(companions)
            .enumerate()
            .map(|(m, ((u, _), (travels, stays, gamelist, art_leaves)))| {
                let unit = &self.units[*u];
                let dest = match &quarantine {
                    Some(q) if m != keeper && action == GroupAction::Act => {
                        Some(dest_for(&self.roms_dir, q, &unit.primary, &unit.system))
                    }
                    _ => None,
                };
                Member {
                    id: m,
                    unit: unit.to_ref(),
                    relation: relations[m].1,
                    size: unit.size,
                    travels,
                    stays,
                    dest,
                    catalogue_name: facts[m].dat_name.clone(),
                    serials: self.l3[*u]
                        .as_ref()
                        .map(|k| k.serials.clone())
                        .unwrap_or_default(),
                    confidence: if m == keeper {
                        1.0
                    } else {
                        link_confidence(raw, *u)
                    },
                    gamelist,
                    art_leaves,
                }
            })
            .collect();
        let title = facts[keeper]
            .dat_name
            .clone()
            .or_else(|| facts.iter().find_map(|f| f.dat_name.clone()))
            .unwrap_or_else(|| self.units[keeper_unit].stem.clone());
        DupGroup {
            id,
            system: raw.system.clone(),
            title,
            keeper,
            reason,
            runner_up,
            members,
            action,
            tie,
            choice_dropped,
        }
    }
}

/// The key a pick is remembered by (spec §9.3): the content (the level 1
/// key, else the disc or catalogue identity with the container and size,
/// so two formats of one disc stay apart) plus the file id, which follows
/// a rename, so that one of two exact copies can be told from the other.
/// Without a file id, the path inside the library stands in.
fn content_key(
    u: &Unit,
    k1: &KeyedUnit,
    k2: Option<&VersionKey>,
    k3: Option<&FormatKey>,
) -> Option<String> {
    let content = match k1.group_key(&u.system) {
        Some(key) => key,
        None => {
            let identity = k3
                .map(FormatKey::key_string)
                .or_else(|| k2.map(VersionKey::key_string))?;
            format!("{identity}|{}|{}", container_name(u), u.size)
        }
    };
    Some(match u.file_id {
        Some(id) => format!("{content}#id:{id}"),
        None => format!("{content}#path:{}", u.primary.display()),
    })
}

/// The first dot-segment of each file stem of `u`, as FAT compares names.
/// Two stems where one is the other plus a dot and more share it.
fn stem_keys(u: &Unit) -> BTreeSet<String> {
    u.files
        .iter()
        .filter_map(|f| {
            let name = f.file_name()?.to_string_lossy().into_owned();
            let stem = match name.rfind('.') {
                Some(i) if i > 0 => &name[..i],
                _ => name.as_str(),
            };
            let key = name_key(stem);
            Some(key.split('.').next().unwrap_or_default().to_string())
        })
        .collect()
}

fn stem_index(units: &[Unit]) -> HashMap<String, Vec<usize>> {
    let mut index: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, u) in units.iter().enumerate() {
        for key in stem_keys(u) {
            index.entry(key).or_default().push(i);
        }
    }
    index
}

/// `Some(true)`: a header emulators read; `Some(false)`: a copier header;
/// `None`: no header.
fn headered(u: &Unit, k: &KeyedUnit) -> Option<bool> {
    if k.header.is_some() {
        return Some(true);
    }
    match &k.exact {
        Some(ExactKey::Payload { size, .. }) if *size < u.payload_size.unwrap_or(u.size) => {
            Some(false)
        }
        _ => None,
    }
}

/// The best confidence of the links that reach `unit`.
fn link_confidence(raw: &RawGroup, unit: usize) -> f32 {
    raw.links
        .iter()
        .filter(|l| l.a == unit || l.b == unit)
        .map(|l| l.confidence)
        .reduce(f32::max)
        .unwrap_or(1.0)
}

/// Where `primary` goes in the quarantine: the same place relative to the
/// folder of system folders (`<quarantine>/GB/sub/Tetris.gb`).
fn dest_for(roms_dir: &Path, quarantine: &Path, primary: &Path, system: &str) -> PathBuf {
    match primary.strip_prefix(roms_dir) {
        Ok(rel) => quarantine.join(rel),
        Err(_) => quarantine
            .join(system)
            .join(primary.file_name().unwrap_or_default()),
    }
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

/// Refuses a quarantine that is a system folder, lies inside one or holds
/// one (the library root, the ROMs folder): its copies would be found
/// again as games, or land where they already are. Compared lexically
/// and, where both exist, as the file system resolves them (case,
/// symlinks).
pub(crate) fn check_quarantine(
    quarantine: &Path,
    system_dirs: &[&Path],
) -> Result<(), DedupeError> {
    let resolved = resolve_existing(quarantine);
    let overlaps = |a: &Path, b: &Path| a.starts_with(b) || b.starts_with(a);
    let bad = system_dirs.iter().any(|dir| {
        overlaps(quarantine, dir)
            || match (&resolved, dir.canonicalize()) {
                (Some(q), Ok(d)) => overlaps(q, &d),
                _ => false,
            }
    });
    if bad {
        Err(DedupeError::QuarantineInSystemFolder(
            quarantine.to_path_buf(),
        ))
    } else {
        Ok(())
    }
}

/// `path` resolved through its deepest existing ancestor (the rest
/// appended as written); `None` when no ancestor resolves.
fn resolve_existing(path: &Path) -> Option<PathBuf> {
    let mut rest = Vec::new();
    let mut probe = path;
    loop {
        if let Ok(canon) = probe.canonicalize() {
            return Some(rest.iter().rev().fold(canon, |acc: PathBuf, c| acc.join(c)));
        }
        rest.push(probe.file_name()?.to_os_string());
        probe = probe.parent()?;
    }
}

fn quarantine_dir(spot: &QuarantineSpot, choice: &QuarantineChoice) -> PathBuf {
    match choice {
        QuarantineChoice::Folder { path } => path.clone(),
        _ => spot.dir.clone(),
    }
}

/// The scan without the excluded systems and those not asked for.
fn scoped(scan: &LibraryScan, opts: &DedupeOptions, exclude: &[String]) -> LibraryScan {
    let systems = scan
        .systems
        .iter()
        .filter(|s| {
            let id = s.system_id.as_deref().unwrap_or("");
            let folder = s.folder_name.to_lowercase();
            !exclude.iter().any(|e| *e == folder || e == id)
                && opts
                    .systems
                    .as_ref()
                    .is_none_or(|list| list.iter().any(|x| x == id))
        })
        .cloned()
        .collect();
    LibraryScan {
        root: scan.root.clone(),
        profile_id: scan.profile_id.clone(),
        systems,
        warnings: scan.warnings.clone(),
    }
}

/// Raw groups for `opts`, with their stable ids.
fn raw_groups(
    units: &[Unit],
    l1: &[KeyedUnit],
    l2: &[Option<VersionKey>],
    l3: &[Option<FormatKey>],
    db: &SystemDb,
    dats: &dyn DatLookup,
    opts: &DedupeOptions,
) -> Vec<(u64, RawGroup)> {
    build_groups(units, l1, l2, l3, &GroupOpts::for_options(db, dats, opts))
        .into_iter()
        .filter(|g| {
            opts.systems
                .as_ref()
                .is_none_or(|list| list.contains(&g.system))
        })
        .map(|g| (group_id(&g.system, &g.keys), g))
        .collect()
}

/// Downloads missing catalogues of systems with two or more units, up to
/// `dedupe.max_dat_downloads` (none when offline or without a store), and
/// returns the systems with a catalogue at hand.
fn catalogues(
    units: &[Unit],
    opts: &DedupeOptions,
    env: &DedupeEnv,
    report: &dyn Fn(DedupePhase, usize, usize),
) -> BTreeSet<String> {
    let mut count: BTreeMap<&str, usize> = BTreeMap::new();
    for u in units {
        *count.entry(u.system.as_str()).or_default() += 1;
    }
    let sources = |system: &str| env.db.get(system).map(dat_sources).unwrap_or_default();
    let cached_now = || -> HashSet<String> {
        env.cache
            .cached_dats()
            .unwrap_or_default()
            .into_iter()
            .collect()
    };
    let mut cached = cached_now();
    if let (false, Some(store)) = (opts.offline, env.dats) {
        let mut wanted: Vec<String> = Vec::new();
        for (system, n) in &count {
            if *n < 2 {
                continue;
            }
            for dat in sources(system) {
                if !cached.contains(&dat) && !wanted.contains(&dat) {
                    wanted.push(dat);
                }
            }
        }
        wanted.truncate(env.settings.dedupe.max_dat_downloads as usize);
        report(DedupePhase::Catalogues, 0, wanted.len());
        for (k, dat) in wanted.iter().enumerate() {
            if env.cancel.load(Ordering::Relaxed) {
                break;
            }
            // A catalogue that cannot be had leaves its system without one.
            let _ = store.ensure(dat);
            report(DedupePhase::Catalogues, k + 1, wanted.len());
        }
        if !wanted.is_empty() {
            cached = cached_now();
        }
    }
    count
        .keys()
        .filter(|system| sources(system).iter().any(|d| cached.contains(d)))
        .map(|s| s.to_string())
        .collect()
}

/// The CRCs catalogues list, for units level 1 did not need to read, in
/// systems with a catalogue: a zip's from its directory, a loose ROM's by
/// a read within `max_hash_mb`. Cached, so a warm run reads nothing.
fn catalogue_crcs_for(
    units: &[Unit],
    l1: &mut [KeyedUnit],
    catalogued: &BTreeSet<String>,
    ctx: &KeyCtx,
    env: &DedupeEnv,
) {
    for (u, k) in units.iter().zip(l1.iter_mut()) {
        if !k.dat_crcs.is_empty() || !catalogued.contains(&u.system) {
            continue;
        }
        if env.cancel.load(Ordering::Relaxed) {
            return;
        }
        let key = format!("{}|{}|{}", u.primary.display(), u.size, u.mtime_ns);
        if let Ok(Some(stored)) = env.cache.get_json::<Vec<(u32, u64)>>(CRCS_NS, &key) {
            k.dat_crcs = stored.value;
            continue;
        }
        let crcs = match u.kind {
            UnitKind::Zip => zip_crcs(u, ctx.listing),
            UnitKind::Single if u.size <= ctx.max_hash_bytes => raw_crcs(u, ctx),
            _ => None,
        };
        let Some(crcs) = crcs else {
            continue;
        };
        // The cache is an optimisation: a failed write only costs a read.
        let _ = env.cache.put_json(CRCS_NS, &key, &crcs, None);
        k.dat_crcs = crcs;
    }
}

fn zip_crcs(u: &Unit, listing: &dyn EntryListing) -> Option<Vec<(u32, u64)>> {
    let entries = listing.list(&u.primary).ok()??;
    let entry = entries
        .iter()
        .find(|e| Some(e.size) == u.payload_size)
        .or_else(|| (entries.len() == 1).then(|| &entries[0]))?;
    Some(vec![(entry.crc32?, entry.size)])
}

fn raw_crcs(u: &Unit, ctx: &KeyCtx) -> Option<Vec<(u32, u64)>> {
    let rule = ctx.db.get(&u.system).map(|s| s.header).unwrap_or_default();
    let reader = ctx.bytes.open(&u.primary).ok()?;
    let h = hash_stream(reader, u.size, rule).ok()?;
    Some(catalogue_crcs(&h))
}

/// Catalogue answers, recorded while planning and replayed after: the
/// grouping asks the same questions whatever the options.
#[derive(Default)]
struct Snapshot {
    crc: Mutex<HashMap<(String, u32, u64), Vec<DatHit>>>,
    serial: Mutex<HashMap<(String, String), Vec<DatHit>>>,
    sets: Mutex<HashMap<(String, String), Option<bool>>>,
}

impl DatLookup for Snapshot {
    fn by_crc(&self, system: &str, crc32: u32, size: u64) -> Vec<DatHit> {
        lock(&self.crc)
            .get(&(system.to_string(), crc32, size))
            .cloned()
            .unwrap_or_default()
    }

    fn by_serial(&self, system: &str, serial: &str) -> Vec<DatHit> {
        lock(&self.serial)
            .get(&(system.to_string(), serial.to_string()))
            .cloned()
            .unwrap_or_default()
    }

    fn set_name(&self, system: &str, name: &str) -> Option<bool> {
        lock(&self.sets)
            .get(&(system.to_string(), name.to_string()))
            .copied()
            .flatten()
    }
}

/// Passes questions on and records the answers in a [`Snapshot`].
struct Recorder<'a> {
    inner: &'a dyn DatLookup,
    snap: &'a Snapshot,
}

impl DatLookup for Recorder<'_> {
    fn by_crc(&self, system: &str, crc32: u32, size: u64) -> Vec<DatHit> {
        let hits = self.inner.by_crc(system, crc32, size);
        lock(&self.snap.crc).insert((system.to_string(), crc32, size), hits.clone());
        hits
    }

    fn by_serial(&self, system: &str, serial: &str) -> Vec<DatHit> {
        let hits = self.inner.by_serial(system, serial);
        lock(&self.snap.serial).insert((system.to_string(), serial.to_string()), hits.clone());
        hits
    }

    fn set_name(&self, system: &str, name: &str) -> Option<bool> {
        let answer = self.inner.set_name(system, name);
        lock(&self.snap.sets).insert((system.to_string(), name.to_string()), answer);
        answer
    }
}

/// Counts for the summary: every group, and for the groups that act, the
/// copies set aside and their bytes (spec §7.1: bytes leaving the ROMs
/// folder). Conflicts and report-only groups move nothing.
fn totals(groups: &[DupGroup]) -> Totals {
    let mut t = Totals::default();
    for g in groups {
        t.groups += 1;
        match g.action {
            GroupAction::Conflict { .. } => t.conflicts += 1,
            GroupAction::Act => {
                let extras: Vec<&Member> = g.members.iter().filter(|m| m.id != g.keeper).collect();
                let bytes: u64 = extras.iter().map(|m| m.size).sum();
                t.extras += extras.len();
                t.bytes += bytes;
                let entry = t.by_system.entry(g.system.clone()).or_default();
                entry.0 += 1;
                entry.1 += extras.len();
                entry.2 += bytes;
            }
            _ => {}
        }
    }
    t
}
