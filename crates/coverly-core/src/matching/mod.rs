//! Bulk selection strategies (ADR-0007, decision D9). Strategies only
//! order candidates; they never fetch or write. Eligible candidates are
//! those of the strongest match kind whose score is within
//! `ELIGIBLE_SCORE_GAP` of the best; the strategy orders them, the rest
//! follow by kind and score. Every tie ends on source order and name, so
//! the result never depends on input order.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use crate::identify::names::Region;
use crate::sources::Candidate;

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

/// Whether the variant is not a retail release (No-Intro / GoodTools tags).
fn unreleased(c: &Candidate) -> bool {
    const TAGS: [&str; 9] = [
        "(beta", "(proto", "(demo", "(sample", "(hack", "(pirate", "(kiosk", "(debug", "[h",
    ];
    let lower = c.name.to_lowercase();
    TAGS.iter().any(|t| lower.contains(t))
}

fn source_rank(c: &Candidate, ctx: &RankContext) -> usize {
    ctx.sources
        .iter()
        .position(|s| *s == c.source)
        .unwrap_or(ctx.sources.len())
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

/// Splits into (eligible, rest) and sorts both: releases before
/// unreleased variants, then `order`, for eligible; kind/score/name for
/// the rest.
fn rank_with(
    ctx: &RankContext,
    cands: &[Candidate],
    order: impl Fn(&Candidate, &Candidate) -> Ordering,
) -> Vec<usize> {
    let Some(best) = cands
        .iter()
        .max_by(|a, b| a.kind.cmp(&b.kind).then(a.score.cmp(&b.score)))
    else {
        return Vec::new();
    };
    let eligible = |c: &Candidate| {
        c.kind == best.kind && c.score.saturating_add(ELIGIBLE_SCORE_GAP) >= best.score
    };
    let (mut good, mut rest): (Vec<usize>, Vec<usize>) =
        (0..cands.len()).partition(|&i| eligible(&cands[i]));
    // A release always beats a beta/proto/demo/hack of the same title, whatever the strategy.
    good.sort_by(|&a, &b| {
        unreleased(&cands[a])
            .cmp(&unreleased(&cands[b]))
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

/// SteamGridDB votes (`Candidate::votes`, filled by the M6 source) first;
/// for consoles, consensus = how many media listings of the same source
/// contain the same file (gathered with `GatherOptions::consensus`); then
/// Priority (D9).
#[derive(Clone, Copy, Debug, Default)]
pub struct Popularity;

impl Strategy for Priority {
    fn kind(&self) -> StrategyKind {
        StrategyKind::Priority
    }
    fn rank(&self, ctx: &RankContext, cands: &[Candidate]) -> Vec<usize> {
        rank_with(ctx, cands, |a, b| {
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
        rank_with(ctx, cands, |a, b| {
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
        rank_with(ctx, cands, |a, b| {
            b.votes
                .unwrap_or(0)
                .cmp(&a.votes.unwrap_or(0))
                .then(b.consensus.cmp(&a.consensus))
                .then(source_rank(a, ctx).cmp(&source_rank(b, ctx)))
                .then(region_rank(a, ctx, false).cmp(&region_rank(b, ctx, false)))
        })
    }
}

/// The strategy for `kind`.
pub fn strategy_for(kind: StrategyKind) -> Box<dyn Strategy> {
    match kind {
        StrategyKind::Priority => Box::new(Priority),
        StrategyKind::RomRegion => Box::new(RomRegion),
        StrategyKind::Popularity => Box::new(Popularity),
    }
}
