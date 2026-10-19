//! Choosing the copy to keep (duplicates spec §6): ordered rules compared
//! pairwise, the user's pick always first and the tie-breaker always last,
//! and a typed reason naming the rule and the two values it compared.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use unicode_normalization::UnicodeNormalization;

use super::saves::SaveInfo;
use super::units::{Unit, UnitKind};
use super::Relation;
use crate::identify::names::{NameTags, Region, ReleaseClass, Revision};
use crate::settings::{container_order_for, ContainerClass, KeepRule, RuleToggle};

/// Dump quality as weighed by the `verified` rule (spec §6.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum DumpQuality {
    /// The content hash is in the catalogue.
    DatMatch,
    /// Not in the catalogue, or no catalogue.
    Unknown,
    /// Tagged `[!]` or plain.
    Good,
    /// Tagged `[b]`.
    Bad,
    /// Tagged `[o]`.
    Overdump,
    /// No copier header (SNES, SMS/GG).
    Headerless,
    /// With a copier header.
    Headered,
}

/// Why the keeper of a group stays: the rule that decided and the values
/// it compared, never prose (spec §6.2). The UI and the CLI localise it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum KeepReason {
    UserChoice,
    HasSave {
        path: PathBuf,
    },
    Played {
        playcount: u32,
    },
    InLibrary,
    Verified {
        kept: DumpQuality,
        other: DumpQuality,
    },
    Retail {
        other: ReleaseClass,
    },
    Region {
        kept: Region,
        other: Region,
    },
    /// As written: `V1.1`, `Rev 1`.
    Revision {
        kept: String,
        other: String,
    },
    /// Containers as named in `dedupe.container_order` (`chd`, `pbp`,
    /// `cue`, `zip`, `raw`).
    Container {
        kept: String,
        other: String,
        profile: String,
    },
    HasCover,
    CatalogueName {
        name: String,
    },
    Tie,
}

/// What the frontend's game list says about one copy (`<playcount>`,
/// `<lastplayed>`, `<favorite>`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Played {
    pub playcount: u32,
    /// As written by the frontend (`20260102T101010`, which sorts by time).
    pub lastplayed: Option<String>,
    pub favorite: bool,
}

impl Played {
    fn any(&self) -> bool {
        self.playcount > 0 || self.lastplayed.is_some() || self.favorite
    }
}

/// The container order that applies to one system, resolved from
/// `dedupe.container_order` (spec §6.3). Empty = no preference.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContainerOrder {
    /// The profile the order belongs to, for [`KeepReason::Container`].
    pub profile: String,
    /// Containers, preferred first (`chd`, `pbp`, `cue`, `zip`, `raw`).
    pub order: Vec<String>,
}

impl ContainerOrder {
    pub fn new(profile: &str, order: &[&str]) -> ContainerOrder {
        ContainerOrder {
            profile: profile.to_string(),
            order: order.iter().map(|c| c.to_lowercase()).collect(),
        }
    }

    /// No preference: the rule never decides.
    pub fn none(profile: &str) -> ContainerOrder {
        ContainerOrder::new(profile, &[])
    }

    /// The order for `system` (an id) of `class` under `profile`.
    pub fn resolve(
        map: &BTreeMap<String, BTreeMap<String, Vec<String>>>,
        profile: &str,
        system: &str,
        class: ContainerClass,
    ) -> ContainerOrder {
        ContainerOrder {
            profile: profile.to_string(),
            order: container_order_for(map, profile, system, class)
                .unwrap_or_default()
                .iter()
                .map(|c| c.to_lowercase())
                .collect(),
        }
    }

    /// Position of the unit's container in the order, with the name that
    /// matched; `None` when the order does not list it.
    fn rank(&self, unit: &Unit) -> Option<(usize, String)> {
        let name = container_name(unit);
        let raw = (unit.kind == UnitKind::Single).then_some("raw");
        self.order
            .iter()
            .position(|c| *c == name || Some(c.as_str()) == raw)
            .map(|i| (i, self.order[i].clone()))
    }
}

/// The container of a unit as `dedupe.container_order` names it: `zip` for
/// a zip or arcade set, `chd`, else the lowercase extension of `primary`
/// (`cue`, `pbp`, `m3u`, `gb`). A loose ROM also answers to `raw`.
pub fn container_name(unit: &Unit) -> String {
    match unit.kind {
        UnitKind::Zip | UnitKind::Arcade => "zip".into(),
        UnitKind::Chd => "chd".into(),
        _ => unit.ext.to_lowercase(),
    }
}

/// One member of a group as the rules see it.
#[derive(Clone, Debug)]
pub struct Candidate<'a> {
    /// `Member::id`.
    pub member: usize,
    pub unit: &'a Unit,
    /// Real saves only (blank ones are not saves).
    pub saves: &'a [SaveInfo],
    pub played: Option<Played>,
    /// Tags of the catalogue name (or the file name when there is none).
    pub tags: Option<&'a NameTags>,
    /// The catalogue game name, when the content is in the catalogue.
    pub dat_name: Option<&'a str>,
    /// Art in at least one of the profile's slots.
    pub has_cover: bool,
    /// Relation to the group (the first member until the keeper is known).
    pub relation: Relation,
    /// `None`: no header. `Some(false)`: a copier header that is not part
    /// of the game (SNES, SMS/GG). `Some(true)`: a header emulators read
    /// (NES, Lynx, A7800).
    pub headered_functional: Option<bool>,
}

impl Candidate<'_> {
    fn played_key(&self) -> (bool, u32, Option<&str>, bool) {
        match &self.played {
            Some(p) => (p.any(), p.playcount, p.lastplayed.as_deref(), p.favorite),
            None => (false, 0, None, false),
        }
    }

    fn dump(&self) -> DumpQuality {
        match self.tags.map(|t| t.flags) {
            Some(f) if f.bad => DumpQuality::Bad,
            Some(f) if f.overdump => DumpQuality::Overdump,
            _ => DumpQuality::Good,
        }
    }
}

/// The order of a region list: `settings.matching.regions` without the
/// `rom` entry and without words that are not regions.
pub fn region_order(regions: &[String]) -> Vec<Region> {
    let mut out = Vec::new();
    for r in regions {
        if let Some(r) = Region::parse(r) {
            if !out.contains(&r) {
                out.push(r);
            }
        }
    }
    out
}

/// What decided between two candidates: `Greater` means the first stays.
type Verdict = (Ordering, KeepReason);

fn prefer<T: Ord>(a: T, b: T, reason: impl FnOnce(bool) -> KeepReason) -> Option<Verdict> {
    match a.cmp(&b) {
        Ordering::Equal => None,
        o => Some((o, reason(o == Ordering::Greater))),
    }
}

fn norm_name(s: &str) -> String {
    s.nfc().collect::<String>().trim().to_lowercase()
}

/// Ranks of `regions` in the order; the best one and its region.
fn best_region(tags: &NameTags, order: &[Region]) -> (usize, Region) {
    tags.regions
        .iter()
        .map(|r| (order.iter().position(|o| o == r).unwrap_or(order.len()), *r))
        .min_by_key(|(i, _)| *i)
        .unwrap_or((order.len(), Region::World))
}

struct Ctx<'r> {
    rules: &'r [RuleToggle],
    regions: &'r [Region],
    order: &'r ContainerOrder,
}

fn rule(r: KeepRule, a: &Candidate, b: &Candidate, ctx: &Ctx) -> Option<Verdict> {
    // Between two other versions of a game; else the copies are proven to
    // be the same game (level 1, or level 3).
    let versions = Relation::weakest([a.relation, b.relation]) == Some(Relation::Version);
    match r {
        KeepRule::HasSave => prefer(!a.saves.is_empty(), !b.saves.is_empty(), |a_wins| {
            let w = if a_wins { a } else { b };
            KeepReason::HasSave {
                path: w.saves[0].path.clone(),
            }
        }),
        KeepRule::Played => prefer(a.played_key(), b.played_key(), |a_wins| {
            let w = if a_wins { a } else { b };
            KeepReason::Played {
                playcount: w.played.as_ref().map_or(0, |p| p.playcount),
            }
        }),
        KeepRule::InLibrary => prefer(a.unit.in_library, b.unit.in_library, |_| {
            KeepReason::InLibrary
        }),
        KeepRule::Verified => verified(a, b, versions),
        KeepRule::Retail if versions => {
            let (ta, tb) = (a.tags?, b.tags?);
            // Lower class is better.
            prefer(tb.class, ta.class, |a_wins| KeepReason::Retail {
                other: if a_wins { tb.class } else { ta.class },
            })
        }
        // Level 1/3 copies share their regions; tags that differ there are
        // file names, and a name never chooses (spec §1).
        KeepRule::Region if versions => {
            let (ta, tb) = (a.tags?, b.tags?);
            if ta.regions.is_empty() || tb.regions.is_empty() {
                return None;
            }
            let (ra, rb) = (best_region(ta, ctx.regions), best_region(tb, ctx.regions));
            prefer(rb.0, ra.0, |a_wins| {
                let (k, o) = if a_wins { (ra.1, rb.1) } else { (rb.1, ra.1) };
                KeepReason::Region { kept: k, other: o }
            })
        }
        KeepRule::Revision if versions => {
            let (ta, tb) = (a.tags?, b.tags?);
            let rev = |t: &NameTags| {
                t.revision.unwrap_or(Revision {
                    major: 0,
                    minor: 0,
                    letter: 0,
                })
            };
            let text = |t: &NameTags| t.revision_text.clone().unwrap_or_default();
            prefer(rev(ta), rev(tb), |a_wins| {
                let (k, o) = if a_wins { (ta, tb) } else { (tb, ta) };
                KeepReason::Revision {
                    kept: text(k),
                    other: text(o),
                }
            })
        }
        KeepRule::Container => {
            let rank = |c: &Candidate| {
                ctx.order
                    .rank(c.unit)
                    .unwrap_or_else(|| (ctx.order.order.len(), container_name(c.unit)))
            };
            if ctx.order.order.is_empty() {
                return None;
            }
            let (ra, rb) = (rank(a), rank(b));
            prefer(rb.0, ra.0, |a_wins| {
                let (k, o) = if a_wins { (ra.1, rb.1) } else { (rb.1, ra.1) };
                KeepReason::Container {
                    kept: k,
                    other: o,
                    profile: ctx.order.profile.clone(),
                }
            })
        }
        KeepRule::Cover => prefer(a.has_cover, b.has_cover, |_| KeepReason::HasCover),
        KeepRule::CatalogueName if !versions => {
            let named = |c: &Candidate| {
                c.dat_name
                    .is_some_and(|n| norm_name(n) == norm_name(&c.unit.stem))
            };
            prefer(named(a), named(b), |a_wins| KeepReason::CatalogueName {
                name: (if a_wins { a } else { b })
                    .dat_name
                    .unwrap_or_default()
                    .to_string(),
            })
        }
        _ => None,
    }
}

/// Catalogue hit, then dump flags, then the header (spec §6.1 `verified`).
/// Dump flags are weighed only between versions: between proven copies
/// they can only differ by file name.
fn verified(a: &Candidate, b: &Candidate, versions: bool) -> Option<Verdict> {
    let dat = |c: &Candidate| {
        if c.dat_name.is_some() {
            DumpQuality::DatMatch
        } else {
            DumpQuality::Unknown
        }
    };
    let pair = |a_wins: bool, qa: DumpQuality, qb: DumpQuality| {
        let (kept, other) = if a_wins { (qa, qb) } else { (qb, qa) };
        KeepReason::Verified { kept, other }
    };
    let rank_dump = |q: DumpQuality| match q {
        DumpQuality::Good => 2,
        DumpQuality::Overdump => 1,
        _ => 0,
    };
    let (da, db) = (dat(a), dat(b));
    if let Some(v) = prefer(
        da == DumpQuality::DatMatch,
        db == DumpQuality::DatMatch,
        |w| pair(w, da, db),
    ) {
        return Some(v);
    }
    let (qa, qb) = (a.dump(), b.dump());
    if versions {
        if let Some(v) = prefer(rank_dump(qa), rank_dump(qb), |w| pair(w, qa, qb)) {
            return Some(v);
        }
    }
    // A copier header loses to none; a functional header beats none: a
    // headerless NES, Lynx or A7800 copy is never preferred.
    let header = |c: &Candidate| match c.headered_functional {
        None => (1, DumpQuality::Headerless),
        Some(false) => (0, DumpQuality::Headered),
        Some(true) => (2, DumpQuality::Headered),
    };
    let (ha, hb) = (header(a), header(b));
    // Two headered copies (copier and functional) are not compared here.
    if ha.1 == hb.1 {
        return None;
    }
    prefer(ha.0, hb.0, |w| pair(w, ha.1, hb.1))
}

/// Oldest modified time, then path order.
fn tie(a: &Candidate, b: &Candidate) -> Ordering {
    b.unit
        .mtime_ns
        .cmp(&a.unit.mtime_ns)
        .then_with(|| b.unit.primary.cmp(&a.unit.primary))
        .then_with(|| b.member.cmp(&a.member))
}

/// `a` against `b` over the enabled rules, then the tie.
fn compare(a: &Candidate, b: &Candidate, ctx: &Ctx) -> Verdict {
    ctx.rules
        .iter()
        .filter(|t| t.on)
        .find_map(|t| rule(t.rule, a, b, ctx))
        .unwrap_or_else(|| (tie(a, b), KeepReason::Tie))
}

/// The best of `cands` by the rules (a running tournament over candidates
/// in tie order, so the input order never matters).
fn best<'c, 'a>(cands: &[&'c Candidate<'a>], ctx: &Ctx) -> Option<&'c Candidate<'a>> {
    let mut sorted: Vec<&Candidate> = cands.to_vec();
    sorted.sort_by(|a, b| tie(b, a));
    let mut it = sorted.into_iter();
    let first = it.next()?;
    Some(it.fold(first, |w, c| {
        if compare(c, w, ctx).0 == Ordering::Greater {
            c
        } else {
            w
        }
    }))
}

/// Picks the copy to keep. `user` is the user's pick and `forced` a copy
/// that must stay (both `Member::id`; ignored when not a candidate);
/// `forced` comes first. Returns the keeper, the reason, the runner-up
/// (the best other copy and the rule it lost on) and whether the
/// tie-breaker alone decided.
pub fn choose(
    cands: &[Candidate],
    rules: &[RuleToggle],
    regions: &[Region],
    order: &ContainerOrder,
    user: Option<usize>,
    forced: Option<usize>,
) -> (usize, KeepReason, Option<(usize, KeepReason)>, bool) {
    let ctx = Ctx {
        rules,
        regions,
        order,
    };
    let all: Vec<&Candidate> = cands.iter().collect();
    let chosen = [forced, user]
        .into_iter()
        .flatten()
        .find_map(|m| cands.iter().find(|c| c.member == m));
    let Some(keeper) = chosen.or_else(|| best(&all, &ctx)) else {
        return (0, KeepReason::Tie, None, false);
    };
    let others: Vec<&Candidate> = all
        .iter()
        .copied()
        .filter(|c| c.member != keeper.member)
        .collect();
    let runner = best(&others, &ctx);
    if chosen.is_some() {
        return (
            keeper.member,
            KeepReason::UserChoice,
            runner.map(|r| (r.member, KeepReason::UserChoice)),
            false,
        );
    }
    let Some(runner) = runner else {
        return (keeper.member, KeepReason::Tie, None, false);
    };
    let (ord, reason) = compare(keeper, runner, &ctx);
    // The rules are not always transitive (a rule may weigh only some
    // pairs); a keeper that does not beat its runner-up is a tie.
    let reason = if ord == Ordering::Greater {
        reason
    } else {
        KeepReason::Tie
    };
    let tie = reason == KeepReason::Tie;
    (
        keeper.member,
        reason.clone(),
        Some((runner.member, reason)),
        tie,
    )
}

/// A fingerprint of every member's real saves (their contents, whatever
/// their names or member ids, which a new copy joining may renumber),
/// stored with a pick (spec §9.3).
pub fn saves_fingerprint(cands: &[Candidate]) -> String {
    let mut all: Vec<&str> = cands
        .iter()
        .flat_map(|c| c.saves.iter().map(|s| s.sha1.as_str()))
        .collect();
    all.sort_unstable();
    let mut h = Sha1::new();
    for sha1 in all {
        h.update(sha1.as_bytes());
        h.update([0u8]);
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Whether a remembered pick still holds (spec §6.1): the copy is still a
/// member, no other copy has a real save it lacks, and no save changed
/// since the pick (`saves_fp` from [`saves_fingerprint`] then).
pub fn remembered_pick_holds(cands: &[Candidate], pick: usize, saves_fp: &str) -> bool {
    let Some(kept) = cands.iter().find(|c| c.member == pick) else {
        return false;
    };
    let other_has_more = kept.saves.is_empty()
        && cands
            .iter()
            .any(|c| c.member != pick && !c.saves.is_empty());
    !other_has_more && saves_fingerprint(cands) == saves_fp
}

/// What `system_dir/gamelist.xml` says about the ROM at `rel` (relative to
/// the system dir). `None` when there is no game list, no entry, or the
/// entry has no play evidence.
pub fn played_from_gamelist(system_dir: &Path, rel: &Path) -> Option<Played> {
    let fields = crate::gamelist::game_fields(system_dir, rel)?;
    let get = |name: &str| {
        fields
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.trim())
            .filter(|v| !v.is_empty())
    };
    let played = Played {
        playcount: get("playcount").and_then(|v| v.parse().ok()).unwrap_or(0),
        lastplayed: get("lastplayed").map(str::to_string),
        favorite: get("favorite").is_some_and(|v| v.eq_ignore_ascii_case("true")),
    };
    played.any().then_some(played)
}
