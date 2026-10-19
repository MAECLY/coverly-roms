//! Building groups per system with the weakest link (duplicates spec §5.5).
//!
//! The keys of levels 1–3 become links between units, and a group is one
//! connected component of those links inside one system:
//!
//! 1. exact copies collapse into a **copy cluster** (equal level 1 key and
//!    soft-patch set);
//! 2. clusters that share a disc identity join (level 3);
//! 3. with other versions reported or "one game, one ROM" on, clusters that
//!    share a catalogue title join (level 2).
//!
//! Every link carries the relation it proves and, when it only proves
//! enough to show, the reason. A group is as strict as its strictest link:
//! one report-only link makes the whole group report-only. Each member's
//! relation to the keeper is the weakest link on the strongest path
//! between them ([`RawGroup::relations_to`]).
//!
//! The system is the first part of every key, and units are only ever
//! compared with units of their own system: a key whose system disagrees
//! with its unit is dropped, and a link across systems is never made.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::systems::SystemDb;

use super::keys::{ExactKey, FormatKey, FormatLink, KeyedUnit, VersionKey, VERSION_CONFIDENCE};
use super::source::DatLookup;
use super::units::Unit;
use super::{DedupeOptions, GroupAction, Relation, ReportWhy};
use crate::settings::DedupeLevel;

/// A group with more members than this is shown collapsed and is
/// report-only until opened and confirmed (spec §10).
pub const BIG_GROUP: usize = 50;

/// Confidence of a level 1 link: proven bytes, an arcade set (two or more
/// 32-bit CRCs that all agree), and a CRC-only or raw-data-only match.
const EXACT_CONFIDENCE: f32 = 1.0;
const ARCADE_CONFIDENCE: f32 = 0.99;
const UNCONFIRMED_CONFIDENCE: f32 = 0.97;

/// What the grouping needs besides the keys.
pub struct GroupOpts<'a> {
    /// Which systems are arcade systems (the zip stem is the driver).
    pub db: &'a SystemDb,
    /// Arcade set names and functional headers are checked here.
    pub dats: &'a dyn DatLookup,
    /// Join by disc identity (level 3).
    pub formats: bool,
    /// Join other versions (level 2), report-only unless
    /// `one_game_one_rom`.
    pub versions: bool,
    /// Other versions, and other builds of one disc, act.
    pub one_game_one_rom: bool,
    /// See [`BIG_GROUP`].
    pub big_group: usize,
}

impl<'a> GroupOpts<'a> {
    /// The defaults: exact copies and other formats, no other versions.
    pub fn new(db: &'a SystemDb, dats: &'a dyn DatLookup) -> GroupOpts<'a> {
        GroupOpts {
            db,
            dats,
            formats: true,
            versions: false,
            one_game_one_rom: false,
            big_group: BIG_GROUP,
        }
    }

    /// The levels a run's options ask for. "One game, one ROM" joins other
    /// versions even when level 2 is not listed.
    pub fn for_options(
        db: &'a SystemDb,
        dats: &'a dyn DatLookup,
        opts: &DedupeOptions,
    ) -> GroupOpts<'a> {
        GroupOpts {
            formats: opts.levels.contains(&DedupeLevel::Format),
            versions: opts.one_game_one_rom || opts.levels.contains(&DedupeLevel::Version),
            one_game_one_rom: opts.one_game_one_rom,
            ..GroupOpts::new(db, dats)
        }
    }
}

/// A link between two units of one system.
#[derive(Clone, Debug, PartialEq)]
pub struct Link {
    /// Unit indices.
    pub a: usize,
    pub b: usize,
    pub relation: Relation,
    /// `Some` when the link may only be shown.
    pub why: Option<ReportWhy>,
    /// 0.0–1.0; 0.0 for a report-only disc link, which proves no match.
    pub confidence: f32,
}

/// A group before its keeper is chosen.
#[derive(Clone, Debug, PartialEq)]
pub struct RawGroup {
    pub system: String,
    /// `(unit index, relation)` sorted by unit; relations are to the first
    /// member until the keeper is known ([`RawGroup::relations_to`]).
    pub members: Vec<(usize, Relation)>,
    /// The strictest of the links; [`ReportWhy::BigGroup`] when every link
    /// acts but the group is larger than [`GroupOpts::big_group`].
    pub action: GroupAction,
    /// When set, only these units may stay: the one copy whose functional
    /// header the catalogue lists, the one arcade copy with a valid set
    /// name.
    pub keeper_only: Option<Vec<usize>>,
    /// Sorted content keys of the members, for the stable group id.
    pub keys: Vec<String>,
    pub links: Vec<Link>,
}

impl RawGroup {
    /// Each member's relation to `keeper` (a unit index): the weakest link
    /// on the strongest path between them, `Exact` for the keeper itself.
    /// In member order. A `keeper` that is not a member leaves the
    /// relations as they are.
    pub fn relations_to(&self, keeper: usize) -> Vec<(usize, Relation)> {
        if !self.members.iter().any(|(u, _)| *u == keeper) {
            return self.members.clone();
        }
        let mut reached: HashMap<usize, Relation> = HashMap::from([(keeper, Relation::Exact)]);
        for level in [Relation::Exact, Relation::Format, Relation::Version] {
            let mut frontier: Vec<usize> = reached.keys().copied().collect();
            while let Some(at) = frontier.pop() {
                for l in self.links.iter().filter(|l| l.relation <= level) {
                    let other = match (l.a == at, l.b == at) {
                        (true, _) => l.b,
                        (_, true) => l.a,
                        _ => continue,
                    };
                    if let std::collections::hash_map::Entry::Vacant(e) = reached.entry(other) {
                        e.insert(level);
                        frontier.push(other);
                    }
                }
            }
        }
        self.members
            .iter()
            .map(|(u, r)| (*u, reached.get(u).copied().unwrap_or(*r)))
            .collect()
    }
}

/// Groups of the units, per system, sorted by system then first member.
///
/// `l1` is the output of [`super::keys::level_one`] (`l1[i].unit` names
/// the unit), `l2` is index-aligned with `l1`, `l3` with `units`. Missing
/// entries count as no key.
pub fn build_groups(
    units: &[Unit],
    l1: &[KeyedUnit],
    l2: &[Option<VersionKey>],
    l3: &[Option<FormatKey>],
    opts: &GroupOpts,
) -> Vec<RawGroup> {
    let mut by_system: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (i, u) in units.iter().enumerate() {
        by_system.entry(u.system.as_str()).or_default().push(i);
    }
    let mut keyed: Vec<Option<&KeyedUnit>> = vec![None; units.len()];
    let mut version: Vec<Option<&VersionKey>> = vec![None; units.len()];
    for (k, v) in l1
        .iter()
        .zip(l2.iter().map(Some).chain(std::iter::repeat(None)))
    {
        if let Some(slot) = keyed.get_mut(k.unit) {
            *slot = Some(k);
            version[k.unit] = v.and_then(Option::as_ref);
        }
    }

    let mut out = Vec::new();
    for (system, members) in by_system {
        let mut sys = SystemGroups::new(system, units, opts);
        sys.copy_clusters(&members, &keyed);
        sys.chd_raw_links(&members, &keyed);
        if opts.formats {
            let format: Vec<(usize, &FormatKey)> = members
                .iter()
                .filter_map(|&i| Some((i, l3.get(i)?.as_ref()?)))
                .filter(|(_, k)| k.system == system)
                .collect();
            sys.format_links(&format, &version);
        }
        if opts.versions {
            let versions: Vec<(usize, &VersionKey)> = members
                .iter()
                .filter_map(|&i| Some((i, version[i]?)))
                .filter(|(_, k)| k.system == system)
                .collect();
            sys.version_links(&versions);
        }
        out.extend(sys.finish());
    }
    out
}

/// The links of one system while they are being found.
struct SystemGroups<'s, 'a> {
    system: &'s str,
    units: &'s [Unit],
    opts: &'s GroupOpts<'a>,
    links: Vec<Link>,
    /// Content keys per unit.
    keys: BTreeMap<usize, BTreeSet<String>>,
    /// Keeper restrictions per copy cluster: (members, allowed keepers).
    only: Vec<(Vec<usize>, Vec<usize>)>,
    /// Copy cluster of each unit with a level 1 key.
    cluster_of: HashMap<usize, usize>,
}

impl<'s, 'a> SystemGroups<'s, 'a> {
    fn new(system: &'s str, units: &'s [Unit], opts: &'s GroupOpts<'a>) -> Self {
        SystemGroups {
            system,
            units,
            opts,
            links: Vec::new(),
            keys: BTreeMap::new(),
            only: Vec::new(),
            cluster_of: HashMap::new(),
        }
    }

    fn link(&mut self, a: usize, b: usize, relation: Relation, why: Option<ReportWhy>, c: f32) {
        // The hard rule: never compare across systems (spec §8.1.1).
        let same = self.units[a].system == self.system && self.units[b].system == self.system;
        debug_assert!(same, "a link across systems");
        if !same || a == b {
            return;
        }
        self.links.push(Link {
            a,
            b,
            relation,
            why,
            confidence: c,
        });
    }

    fn key(&mut self, unit: usize, key: String) {
        self.keys.entry(unit).or_default().insert(key);
    }

    /// Level 1: units with an equal group key, with the arcade, header and
    /// confirmation guards of spec §5.2.
    fn copy_clusters(&mut self, members: &[usize], keyed: &[Option<&KeyedUnit>]) {
        let mut clusters: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for &i in members {
            if let Some(key) = keyed[i].and_then(|k| k.group_key(self.system)) {
                clusters.entry(key).or_default().push(i);
            }
        }
        let arcade = self.opts.db.get(self.system).is_some_and(|s| s.arcade);
        for (n, (key, mut cluster)) in clusters.into_iter().enumerate() {
            for &u in &cluster {
                self.cluster_of.insert(u, n);
            }
            let first = keyed[cluster[0]].expect("clustered units have keys");
            let mut reasons: Vec<ReportWhy> = Vec::new();
            let mut only: Option<Vec<usize>> = None;
            if arcade {
                match self.arcade_names(&cluster) {
                    None => reasons.push(ReportWhy::ArcadeNoDat),
                    Some(valid) if valid.len() >= 2 => {
                        // Valid set names run other drivers: those never
                        // join. Their copies stay among themselves.
                        cluster.retain(|u| !valid.contains(u));
                    }
                    Some(valid) if valid.len() == 1 => only = Some(valid),
                    Some(_) => {}
                }
            }
            if cluster.len() < 2 {
                continue;
            }
            if !first.confirmed || first.exact.as_ref().is_some_and(|e| !e.is_confirmed()) {
                reasons.push(ReportWhy::Unconfirmed);
            }
            match self.header_keepers(&cluster, keyed) {
                HeaderCheck::Same => {}
                HeaderCheck::OnlyThese(units) => only = Some(units),
                HeaderCheck::Differs => reasons.push(ReportWhy::HeaderDiffers),
            }
            let why = strictest(reasons);
            let confidence = match (&first.exact, &why) {
                (_, Some(ReportWhy::Unconfirmed)) => UNCONFIRMED_CONFIDENCE,
                (Some(ExactKey::Arcade { .. }), _) => ARCADE_CONFIDENCE,
                _ => EXACT_CONFIDENCE,
            };
            for &u in &cluster[1..] {
                self.link(cluster[0], u, Relation::Exact, why.clone(), confidence);
            }
            for &u in &cluster {
                self.key(u, key.clone());
            }
            if let Some(only) = only {
                self.only.push((cluster, only));
            }
        }
    }

    /// The members whose stem is a set name in the system's catalogue;
    /// `None` when no catalogue is at hand.
    fn arcade_names(&self, cluster: &[usize]) -> Option<Vec<usize>> {
        let mut valid = Vec::new();
        for &u in cluster {
            if self.opts.dats.set_name(self.system, &self.units[u].stem)? {
                valid.push(u);
            }
        }
        Some(valid)
    }

    /// Functional headers of a cluster (spec §5.2): equal (or absent),
    /// exactly one listed by the catalogue, or differing.
    fn header_keepers(&self, cluster: &[usize], keyed: &[Option<&KeyedUnit>]) -> HeaderCheck {
        let headers: BTreeSet<&[u8]> = cluster
            .iter()
            .filter_map(|&u| keyed[u]?.header.as_deref())
            .collect();
        if headers.len() < 2 {
            return HeaderCheck::Same;
        }
        // A header is the catalogue's when the whole content, header
        // included, hits by hash; the headerless CRC is shared by every
        // member and says nothing about the header.
        let listed: BTreeSet<&[u8]> = cluster
            .iter()
            .filter_map(|&u| {
                let k = keyed[u]?;
                let header = k.header.as_deref()?;
                let whole = self.units[u].payload_size?;
                let hit = k
                    .dat_crcs
                    .iter()
                    .filter(|(_, size)| *size == whole)
                    .any(|&(crc, size)| !self.opts.dats.by_crc(self.system, crc, size).is_empty());
                hit.then_some(header)
            })
            .collect();
        match listed.into_iter().collect::<Vec<_>>().as_slice() {
            [one] => HeaderCheck::OnlyThese(
                cluster
                    .iter()
                    .copied()
                    .filter(|&u| keyed[u].and_then(|k| k.header.as_deref()) == Some(*one))
                    .collect(),
            ),
            _ => HeaderCheck::Differs,
        }
    }

    /// CHDs with the same raw data and another combined SHA1: the same
    /// data with other track metadata, report-only.
    fn chd_raw_links(&mut self, members: &[usize], keyed: &[Option<&KeyedUnit>]) {
        let mut by_raw: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
        for &i in members {
            let Some(k) = keyed[i] else { continue };
            let raw = match (&k.exact, &k.chd_raw) {
                (_, Some(raw)) => raw.as_str(),
                (Some(ExactKey::ChdRaw { rawsha1 }), None) => rawsha1.as_str(),
                _ => continue,
            };
            by_raw.entry(raw).or_default().push(i);
        }
        for (raw, list) in by_raw {
            // One representative per copy cluster: units already in one
            // cluster are linked there.
            let mut seen: BTreeMap<usize, usize> = BTreeMap::new();
            for &u in &list {
                let cluster = self.cluster_of.get(&u).copied().unwrap_or(usize::MAX - u);
                seen.entry(cluster).or_insert(u);
            }
            let reps: Vec<usize> = seen.into_values().collect();
            if reps.len() < 2 {
                continue;
            }
            for &u in &reps[1..] {
                self.link(
                    reps[0],
                    u,
                    Relation::Exact,
                    Some(ReportWhy::ChdMetadataDiffers),
                    UNCONFIRMED_CONFIDENCE,
                );
            }
            for &u in &list {
                self.key(u, format!("l1:{}:chdraw:{raw}", self.system));
            }
        }
    }

    /// Level 3: units sharing a serial, linked as [`FormatKey::link`] says.
    /// With "one game, one ROM" another build of one disc is another
    /// version and acts, but only when both builds have the same level 2
    /// key: a level 2 group needs a catalogue identity by hash (spec
    /// §5.3), and a serial alone is not one (translations keep it).
    fn format_links(&mut self, format: &[(usize, &FormatKey)], version: &[Option<&VersionKey>]) {
        let system = self.system;
        let same_version = |a: usize, b: usize| match (version[a], version[b]) {
            (Some(x), Some(y)) => {
                x.system == system && y.system == system && x.key_string() == y.key_string()
            }
            _ => false,
        };
        let mut by_serial: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
        for (n, (_, k)) in format.iter().enumerate() {
            for s in &k.serials {
                by_serial.entry(s.as_str()).or_default().push(n);
            }
        }
        let mut pairs: BTreeSet<(usize, usize)> = BTreeSet::new();
        for list in by_serial.values() {
            for (x, &a) in list.iter().enumerate() {
                for &b in &list[x + 1..] {
                    if a != b {
                        pairs.insert((a.min(b), a.max(b)));
                    }
                }
            }
        }
        let mut keyed_units = BTreeSet::new();
        for (a, b) in pairs {
            let ((ua, ka), (ub, kb)) = (format[a], format[b]);
            let (relation, why, confidence) = match ka.link(kb) {
                None => continue,
                Some(FormatLink::Acts { confidence }) => (Relation::Format, None, confidence),
                Some(FormatLink::ReportOnly {
                    why: ReportWhy::BuildDiffers,
                }) if self.opts.one_game_one_rom && same_version(ua, ub) => {
                    (Relation::Version, None, VERSION_CONFIDENCE)
                }
                Some(FormatLink::ReportOnly { why }) => (Relation::Format, Some(why), 0.0),
            };
            self.link(ua, ub, relation, why, confidence);
            keyed_units.insert(a);
            keyed_units.insert(b);
        }
        for n in keyed_units {
            let (u, k) = format[n];
            self.key(u, k.key_string());
        }
    }

    /// Level 2: units whose catalogue titles share a key. Report-only
    /// unless "one game, one ROM" is on.
    fn version_links(&mut self, versions: &[(usize, &VersionKey)]) {
        let mut buckets: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for &(u, k) in versions {
            buckets.entry(k.key_string()).or_default().push(u);
        }
        let why = (!self.opts.one_game_one_rom).then_some(ReportWhy::LevelTwoOff);
        for (key, list) in buckets {
            if list.len() < 2 {
                continue;
            }
            // One representative per copy cluster: exact copies of one
            // catalogue game share its key, but they are not other
            // versions of each other, and a version link between them
            // would make their exact group report-only.
            let mut seen: BTreeMap<usize, usize> = BTreeMap::new();
            for &u in &list {
                let cluster = self.cluster_of.get(&u).copied().unwrap_or(usize::MAX - u);
                seen.entry(cluster).or_insert(u);
            }
            let mut reps: Vec<usize> = seen.into_values().collect();
            reps.sort_unstable();
            for &u in &reps[1..] {
                self.link(
                    reps[0],
                    u,
                    Relation::Version,
                    why.clone(),
                    VERSION_CONFIDENCE,
                );
            }
            for &u in &list {
                self.key(u, key.clone());
            }
        }
    }

    /// Connected components of two or more units, as groups.
    fn finish(self) -> Vec<RawGroup> {
        let mut uf = UnionFind::default();
        for l in &self.links {
            uf.union(l.a, l.b);
        }
        let mut components: BTreeMap<usize, (Vec<usize>, Vec<Link>)> = BTreeMap::new();
        for l in self.links.iter() {
            let entry = components.entry(uf.find(l.a)).or_default();
            entry.0.extend([l.a, l.b]);
            entry.1.push(l.clone());
        }
        let mut groups: Vec<RawGroup> = Vec::new();
        for (mut units, links) in components.into_values() {
            units.sort_unstable();
            units.dedup();
            if units.len() < 2 {
                continue;
            }
            let action = match strictest(links.iter().filter_map(|l| l.why.clone())) {
                Some(why) => GroupAction::ReportOnly { why },
                None if units.len() > self.opts.big_group => GroupAction::ReportOnly {
                    why: ReportWhy::BigGroup {
                        members: units.len(),
                    },
                },
                None => GroupAction::Act,
            };
            let mut only: Vec<usize> = self
                .only
                .iter()
                .filter(|(cluster, _)| units.binary_search(&cluster[0]).is_ok())
                .flat_map(|(_, only)| only.iter().copied())
                .collect();
            only.sort_unstable();
            only.dedup();
            let keys: BTreeSet<String> = units
                .iter()
                .filter_map(|u| self.keys.get(u))
                .flatten()
                .cloned()
                .collect();
            let mut group = RawGroup {
                system: self.system.to_string(),
                members: units.iter().map(|&u| (u, Relation::Exact)).collect(),
                action,
                keeper_only: (!only.is_empty()).then_some(only),
                keys: keys.into_iter().collect(),
                links,
            };
            group.members = group.relations_to(units[0]);
            groups.push(group);
        }
        groups.sort_by_key(|g| g.members[0].0);
        groups
    }
}

enum HeaderCheck {
    /// One header or none: nothing to tell apart.
    Same,
    /// Exactly one header is the catalogue's: only these may stay.
    OnlyThese(Vec<usize>),
    Differs,
}

/// How strict a reason is: the strictest reason of a group names it. A big
/// group is the least strict, as it is the only one you may override.
fn rank(why: &ReportWhy) -> u8 {
    match why {
        ReportWhy::Modified => 10,
        ReportWhy::SerialConflict => 9,
        ReportWhy::HeaderDiffers => 8,
        ReportWhy::BuildDiffers => 7,
        ReportWhy::ChdMetadataDiffers => 6,
        ReportWhy::PartialSet => 5,
        ReportWhy::Unconfirmed => 4,
        ReportWhy::ArcadeNoDat => 3,
        ReportWhy::LevelTwoOff => 2,
        ReportWhy::UserSkipped => 1,
        ReportWhy::BigGroup { .. } => 0,
    }
}

fn strictest(reasons: impl IntoIterator<Item = ReportWhy>) -> Option<ReportWhy> {
    reasons.into_iter().max_by_key(rank)
}

/// Union-find over unit indices, created on demand.
#[derive(Default)]
struct UnionFind {
    parent: HashMap<usize, usize>,
}

impl UnionFind {
    fn find(&mut self, x: usize) -> usize {
        let p = *self.parent.entry(x).or_insert(x);
        if p == x {
            return x;
        }
        let root = self.find(p);
        self.parent.insert(x, root);
        root
    }

    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            // The smaller index is the root, so roots are stable.
            let (lo, hi) = (ra.min(rb), ra.max(rb));
            self.parent.insert(hi, lo);
        }
    }
}
