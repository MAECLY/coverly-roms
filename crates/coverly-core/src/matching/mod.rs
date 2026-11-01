//! Bulk selection strategies (ADR-0007, decision D9). Strategies only
//! order candidates; they never fetch or write. Eligible candidates are
//! those of the strongest match kind whose score is within
//! `ELIGIBLE_SCORE_GAP` of the best; the strategy orders them, the rest
//! follow by kind and score. Every tie ends on source order and name, so
//! the result never depends on input order.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use crate::identify::names::Region;
use crate::sources::{Candidate, MatchKind, Votes};

/// Largest score gap below the best candidate that is still eligible.
pub const ELIGIBLE_SCORE_GAP: u8 = 5;

/// One entry of the region preference order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RegionPref {
    /// The ROM's own regions (only `RomRegion` expands it).
    RomOwn,
    Region(Region),
}

/// Which strategy picks the art in bulk.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StrategyKind {
    Priority,
    #[default]
    RomRegion,
    Popularity,
}

/// What a strategy knows about the ROM and the user's preferences.
#[derive(Clone, Copy, Debug)]
pub struct RankContext<'a> {
    /// Regions parsed from the ROM name.
    pub rom_regions: &'a [Region],
    /// Region preference order.
    pub regions: &'a [RegionPref],
    /// Source ids, preferred first.
    pub sources: &'a [String],
    /// Whether the icon inside the game ranks above every other candidate
    /// (systems batch 3 D21: the system has `embedded_first` and the user
    /// enabled `embedded`); otherwise it ranks below them (batch 2 D17).
    pub embedded_first: bool,
}

/// Orders candidates for one ROM and media.
pub trait Strategy: Send + Sync {
    fn kind(&self) -> StrategyKind;
    /// Indices of `cands`, best first; eligible (best tier) candidates first.
    fn rank(&self, ctx: &RankContext, cands: &[Candidate]) -> Vec<usize>;
}

/// `rom, world, usa, europe, japan` (spec §9).
pub fn default_region_order() -> Vec<RegionPref> {
    use Region::*;
    vec![
        RegionPref::RomOwn,
        RegionPref::Region(World),
        RegionPref::Region(Usa),
        RegionPref::Region(Europe),
        RegionPref::Region(Japan),
    ]
}

/// Parses a comma-separated order such as `rom,world,usa,eu,jp`.
pub fn parse_region_order(s: &str) -> Result<Vec<RegionPref>, String> {
    s.split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|p| {
            if p.eq_ignore_ascii_case("rom") {
                Ok(RegionPref::RomOwn)
            } else {
                Region::parse(p)
                    .map(RegionPref::Region)
                    .ok_or_else(|| format!("unknown region `{p}`"))
            }
        })
        .collect()
}

/// Position of the candidate's best region in the preference order
/// (`RomOwn` expanded to the ROM's regions when `expand_rom`, skipped
/// otherwise), or the order's length when none matches.
fn region_rank(c: &Candidate, ctx: &RankContext, expand_rom: bool) -> usize {
    let mut expanded: Vec<Region> = Vec::new();
    for p in ctx.regions {
        match p {
            RegionPref::RomOwn if expand_rom => expanded.extend(ctx.rom_regions.iter().copied()),
            RegionPref::RomOwn => {}
            RegionPref::Region(r) => expanded.push(*r),
        }
    }
    c.regions
        .iter()
        .filter_map(|r| expanded.iter().position(|x| x == r))
        .min()
        .unwrap_or(expanded.len())
}

/// Words that mark a non-retail variant inside `(…)` or `[…]` (No-Intro
/// and GoodTools tags).
const VARIANT_WORDS: [&str; 11] = [
    "beta",
    "proto",
    "prototype",
    "demo",
    "sample",
    "hack",
    "pirate",
    "kiosk",
    "debug",
    "unl",
    "unlicensed",
];

/// Whether `name` is not a retail release: a beta, prototype, demo,
/// sample, hack, pirate, kiosk, debug or unlicensed variant — `(Beta)`,
/// `(Proto 2)`, `(Unl)`, GoodTools `[h1]`, `[Hack by …]`,
/// `(Sound Restoration Hack)`. Only bracketed tags count, so titles such
/// as `.hack` or `Hackers` are releases.
pub fn is_unreleased_variant(name: &str) -> bool {
    let lower = name.to_lowercase();
    let found = bracket_groups(&lower).any(|g| {
        let g = g.trim();
        let goodtools_hack =
            g == "h" || (g.starts_with('h') && g.as_bytes().get(1).is_some_and(u8::is_ascii_digit));
        goodtools_hack
            || g.split(|c: char| !c.is_alphanumeric())
                .any(|w| VARIANT_WORDS.contains(&w))
    });
    found
}

/// The text inside each closed `(…)` and `[…]` of `s` (not nested).
fn bracket_groups(s: &str) -> impl Iterator<Item = &str> {
    let mut rest = s;
    std::iter::from_fn(move || loop {
        let open = rest.find(['(', '['])?;
        let close = if rest.as_bytes()[open] == b'(' {
            ')'
        } else {
            ']'
        };
        let after = &rest[open + 1..];
        match after.find(close) {
            Some(end) => {
                rest = &after[end + 1..];
                return Some(&after[..end]);
            }
            // Unclosed: not a tag; look further on.
            None => rest = after,
        }
    })
}

fn source_rank(c: &Candidate, ctx: &RankContext) -> usize {
    ctx.sources
        .iter()
        .position(|s| *s == c.source)
        .unwrap_or(ctx.sources.len())
}

/// The tier a strategy compares in. Popularity merges every exact kind
/// so votes can choose between an exact libretro name and a SteamGridDB
/// grid of the same title (D33); fuzzy is always the lowest tier.
fn tier(kind: MatchKind, merge_exact: bool) -> u8 {
    match kind {
        MatchKind::Fuzzy => 0,
        _ if merge_exact => 3,
        MatchKind::TitleExact => 1,
        MatchKind::NameExact => 2,
        MatchKind::Canonical => 3,
    }
}

/// Whether `c` competes with `best`: same tier, score within
/// `ELIGIBLE_SCORE_GAP`.
fn eligible(best: &Candidate, c: &Candidate, merge_exact: bool) -> bool {
    tier(c.kind, merge_exact) == tier(best.kind, merge_exact)
        && c.score.saturating_add(ELIGIBLE_SCORE_GAP) >= best.score
}

/// Net SteamGridDB votes; a candidate without votes counts 0.
pub fn vote_score(c: &Candidate) -> i64 {
    c.votes.map_or(0, Votes::score)
}

/// Position of the first preferred language `c`'s name carries (D28), or
/// `langs.len()` when none.
pub fn language_rank(c: &Candidate, langs: &[String]) -> usize {
    let have = crate::identify::names::languages(&c.name);
    langs
        .iter()
        .position(|l| have.contains(l))
        .unwrap_or(langs.len())
}

/// Final tie-break: score, source order, name, source id.
fn tail(a: &Candidate, b: &Candidate, ctx: &RankContext) -> Ordering {
    b.score
        .cmp(&a.score)
        .then(source_rank(a, ctx).cmp(&source_rank(b, ctx)))
        .then_with(|| a.name.cmp(&b.name))
        .then_with(|| a.source.cmp(&b.source))
        .then_with(|| a.url.cmp(&b.url))
}

/// Whether `c` is an icon inside the game (systems batch 2 D17, batch 3
/// D21): it ranks below every other candidate, whatever its kind, score
/// or the user's source order, or above all of them when the context says
/// [`RankContext::embedded_first`].
fn is_embedded(c: &Candidate) -> bool {
    c.source == crate::sources::embedded::ID
}

/// Ranks the ordinary candidates, then the embedded ones, or the other
/// way round with [`RankContext::embedded_first`] (each group by
/// [`rank_among`]).
fn rank_with(
    ctx: &RankContext,
    cands: &[Candidate],
    merge_exact: bool,
    order: impl Fn(&Candidate, &Candidate) -> Ordering,
) -> Vec<usize> {
    let (icons, main): (Vec<usize>, Vec<usize>) =
        (0..cands.len()).partition(|&i| is_embedded(&cands[i]));
    let (first, then) = if ctx.embedded_first {
        (icons, main)
    } else {
        (main, icons)
    };
    let mut out = rank_among(ctx, cands, &first, merge_exact, &order);
    out.extend(rank_among(ctx, cands, &then, merge_exact, &order));
    out
}

/// Splits `among` into (eligible, rest) and sorts both: releases before
/// unreleased variants, then `order`, for eligible; kind/score/name for
/// the rest.
fn rank_among(
    ctx: &RankContext,
    cands: &[Candidate],
    among: &[usize],
    merge_exact: bool,
    order: &impl Fn(&Candidate, &Candidate) -> Ordering,
) -> Vec<usize> {
    let Some(best) = among.iter().map(|&i| &cands[i]).max_by(|a, b| {
        tier(a.kind, merge_exact)
            .cmp(&tier(b.kind, merge_exact))
            .then(a.score.cmp(&b.score))
    }) else {
        return Vec::new();
    };
    let (mut good, mut rest): (Vec<usize>, Vec<usize>) = among
        .iter()
        .copied()
        .partition(|&i| eligible(best, &cands[i], merge_exact));
    // A release always beats a beta/proto/demo/hack of the same title, whatever the strategy.
    good.sort_by(|&a, &b| {
        is_unreleased_variant(&cands[a].name)
            .cmp(&is_unreleased_variant(&cands[b].name))
            .then_with(|| order(&cands[a], &cands[b]))
            .then_with(|| tail(&cands[a], &cands[b], ctx))
    });
    rest.sort_by(|&a, &b| {
        cands[b]
            .kind
            .cmp(&cands[a].kind)
            .then_with(|| tail(&cands[a], &cands[b], ctx))
    });
    good.extend(rest);
    good
}

/// Source order, then the region order without the ROM's own region.
#[derive(Clone, Copy, Debug, Default)]
pub struct Priority;

/// The ROM's own region first, then the region order, then source order.
#[derive(Clone, Copy, Debug, Default)]
pub struct RomRegion;

/// Net SteamGridDB votes, then match kind, then consensus (listings of
/// the same source + other sources in an overlapping region, D34), then
/// Priority (D9, D33). Every exact match kind is one tier here, so votes
/// can beat an exact name; a fuzzy match never competes with an exact one.
#[derive(Clone, Copy, Debug, Default)]
pub struct Popularity;

impl Strategy for Priority {
    fn kind(&self) -> StrategyKind {
        StrategyKind::Priority
    }
    fn rank(&self, ctx: &RankContext, cands: &[Candidate]) -> Vec<usize> {
        rank_with(ctx, cands, false, |a, b| {
            source_rank(a, ctx)
                .cmp(&source_rank(b, ctx))
                .then(region_rank(a, ctx, false).cmp(&region_rank(b, ctx, false)))
        })
    }
}

impl Strategy for RomRegion {
    fn kind(&self) -> StrategyKind {
        StrategyKind::RomRegion
    }
    fn rank(&self, ctx: &RankContext, cands: &[Candidate]) -> Vec<usize> {
        rank_with(ctx, cands, false, |a, b| {
            region_rank(a, ctx, true)
                .cmp(&region_rank(b, ctx, true))
                .then(source_rank(a, ctx).cmp(&source_rank(b, ctx)))
        })
    }
}

impl Strategy for Popularity {
    fn kind(&self) -> StrategyKind {
        StrategyKind::Popularity
    }
    fn rank(&self, ctx: &RankContext, cands: &[Candidate]) -> Vec<usize> {
        rank_with(ctx, cands, true, |a, b| {
            vote_score(b)
                .cmp(&vote_score(a))
                .then(b.kind.cmp(&a.kind))
                .then(b.consensus.cmp(&a.consensus))
                .then(source_rank(a, ctx).cmp(&source_rank(b, ctx)))
                .then(region_rank(a, ctx, false).cmp(&region_rank(b, ctx, false)))
        })
    }
}

/// Why the first of a ranked list was picked over the second.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "why", rename_all = "kebab-case")]
pub enum PickReason {
    /// The only candidate.
    Only,
    /// It matches the ROM better (kind or score).
    BetterMatch,
    /// A release beats a beta/proto/demo/hack (D22).
    Release,
    /// More net SteamGridDB votes than the next one.
    Votes { score: i64, next: i64 },
    /// More listings, sources and regions agree on it (D9 b, D34).
    Consensus { count: u8, next: u8 },
    /// Earlier in the user's source order.
    SourceOrder { source: String },
    /// Earlier in the region order; `rom` = it is the ROM's own region.
    Region { rom: bool },
    /// It carries the preferred language (D28).
    Language,
    /// Equal on every rule: the first by name, so the result is stable.
    Tie,
    /// The user picked it (review queue, drawer).
    User,
}

/// Names the rule that put `first` before `second` in a ranking by
/// `kind` (followed by the D28 language tie-break).
pub fn explain(
    kind: StrategyKind,
    ctx: &RankContext,
    first: &Candidate,
    second: Option<&Candidate>,
    languages: &[String],
) -> PickReason {
    let Some(second) = second else {
        return PickReason::Only;
    };
    // The icon inside the game wins by the source order alone (D21).
    if ctx.embedded_first && is_embedded(first) && !is_embedded(second) {
        return PickReason::SourceOrder {
            source: first.source.clone(),
        };
    }
    if !is_unreleased_variant(&first.name) && is_unreleased_variant(&second.name) {
        return PickReason::Release;
    }
    let merge = kind == StrategyKind::Popularity;
    if !eligible(first, second, merge) {
        return PickReason::BetterMatch;
    }
    let by_source = || PickReason::SourceOrder {
        source: first.source.clone(),
    };
    let src = |c: &Candidate| source_rank(c, ctx);
    match kind {
        StrategyKind::Popularity => {
            if vote_score(first) != vote_score(second) {
                return PickReason::Votes {
                    score: vote_score(first),
                    next: vote_score(second),
                };
            }
            if first.kind != second.kind {
                return PickReason::BetterMatch;
            }
            if first.consensus != second.consensus {
                return PickReason::Consensus {
                    count: first.consensus,
                    next: second.consensus,
                };
            }
            if src(first) != src(second) {
                return by_source();
            }
            if region_rank(first, ctx, false) != region_rank(second, ctx, false) {
                return PickReason::Region { rom: false };
            }
        }
        StrategyKind::Priority => {
            if src(first) != src(second) {
                return by_source();
            }
            if region_rank(first, ctx, false) != region_rank(second, ctx, false) {
                return PickReason::Region { rom: false };
            }
        }
        StrategyKind::RomRegion => {
            if region_rank(first, ctx, true) != region_rank(second, ctx, true) {
                let rom = first.regions.iter().any(|r| ctx.rom_regions.contains(r));
                return PickReason::Region { rom };
            }
            if src(first) != src(second) {
                return by_source();
            }
        }
    }
    if language_rank(first, languages) != language_rank(second, languages) {
        return PickReason::Language;
    }
    if first.score != second.score {
        return PickReason::BetterMatch;
    }
    PickReason::Tie
}

/// The strategy for `kind`.
pub fn strategy_for(kind: StrategyKind) -> Box<dyn Strategy> {
    match kind {
        StrategyKind::Priority => Box::new(Priority),
        StrategyKind::RomRegion => Box::new(RomRegion),
        StrategyKind::Popularity => Box::new(Popularity),
    }
}
