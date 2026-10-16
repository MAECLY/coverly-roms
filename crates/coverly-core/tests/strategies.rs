use coverly_core::identify::names::Region::{self, *};
use coverly_core::matching::{
    default_region_order, parse_region_order, strategy_for, RankContext, RegionPref, StrategyKind,
};
use coverly_core::profiles::Media;
use coverly_core::sources::{Candidate, MatchKind};

fn c(source: &str, name: &str, regions: &[Region], kind: MatchKind, score: u8) -> Candidate {
    Candidate {
        source: source.into(),
        media: Media::Cover,
        name: name.into(),
        url: format!("https://x/{name}"),
        fallback_url: None,
        regions: regions.to_vec(),
        kind,
        score,
        votes: None,
        consensus: 0,
    }
}

fn golden() -> Vec<Candidate> {
    vec![
        c(
            "libretro",
            "Golden Sun (Japan)",
            &[Japan],
            MatchKind::TitleExact,
            95,
        ),
        c(
            "libretro",
            "Golden Sun (USA, Europe)",
            &[Usa, Europe],
            MatchKind::TitleExact,
            95,
        ),
        c("libretro", "Golden Sum (USA)", &[Usa], MatchKind::Fuzzy, 80),
    ]
}

fn pick(kind: StrategyKind, rom: &[Region], cands: &[Candidate]) -> String {
    let regions = default_region_order();
    let sources = vec!["libretro".to_string(), "other".to_string()];
    let ctx = RankContext {
        rom_regions: rom,
        regions: &regions,
        sources: &sources,
    };
    cands[strategy_for(kind).rank(&ctx, cands)[0]].name.clone()
}

#[test]
fn priority_follows_the_region_order_without_the_rom_region() {
    assert_eq!(
        pick(StrategyKind::Priority, &[Japan], &golden()),
        "Golden Sun (USA, Europe)"
    );
}

#[test]
fn rom_region_prefers_the_rom_own_region_then_the_order() {
    assert_eq!(
        pick(StrategyKind::RomRegion, &[Japan], &golden()),
        "Golden Sun (Japan)"
    );
    assert_eq!(
        pick(StrategyKind::RomRegion, &[], &golden()),
        "Golden Sun (USA, Europe)"
    );
}

#[test]
fn a_weaker_match_kind_never_beats_the_best_tier() {
    let cands = vec![
        c(
            "libretro",
            "Golden Sun (Japan)",
            &[Japan],
            MatchKind::TitleExact,
            95,
        ),
        c("libretro", "Golden Sum (USA)", &[Usa], MatchKind::Fuzzy, 94),
    ];
    for kind in [
        StrategyKind::Priority,
        StrategyKind::RomRegion,
        StrategyKind::Popularity,
    ] {
        assert_eq!(pick(kind, &[Usa], &cands), "Golden Sun (Japan)", "{kind:?}");
    }
}

#[test]
fn popularity_uses_votes_then_consensus_then_priority() {
    let mut cands = golden();
    cands[0].consensus = 3;
    cands[1].consensus = 1;
    assert_eq!(
        pick(StrategyKind::Popularity, &[], &cands),
        "Golden Sun (Japan)"
    );
    cands[0].votes = Some(2);
    cands[1].votes = Some(10);
    assert_eq!(
        pick(StrategyKind::Popularity, &[], &cands),
        "Golden Sun (USA, Europe)"
    );
    let plain = golden();
    assert_eq!(
        pick(StrategyKind::Popularity, &[Japan], &plain),
        "Golden Sun (USA, Europe)",
        "falls back to Priority (D9)"
    );
}

#[test]
fn source_order_breaks_ties_under_priority() {
    let cands = vec![
        c("other", "Zelda (USA)", &[Usa], MatchKind::TitleExact, 95),
        c("libretro", "Zelda (USA)", &[Usa], MatchKind::TitleExact, 95),
    ];
    let ranked_first = pick(StrategyKind::Priority, &[], &cands);
    assert_eq!(ranked_first, "Zelda (USA)");
    let regions = default_region_order();
    let sources = vec!["libretro".to_string(), "other".to_string()];
    let ctx = RankContext {
        rom_regions: &[],
        regions: &regions,
        sources: &sources,
    };
    assert_eq!(
        cands[strategy_for(StrategyKind::Priority).rank(&ctx, &cands)[0]].source,
        "libretro"
    );
}

#[test]
fn ranking_is_deterministic_for_any_input_order() {
    let base = golden();
    let mut rev = base.clone();
    rev.reverse();
    for kind in [
        StrategyKind::Priority,
        StrategyKind::RomRegion,
        StrategyKind::Popularity,
    ] {
        assert_eq!(
            pick(kind, &[Europe], &base),
            pick(kind, &[Europe], &rev),
            "{kind:?}"
        );
    }
}

#[test]
fn prototypes_and_betas_never_beat_a_release() {
    // By name alone "(USA) (Proto)" sorts before "(USA, Europe)"; a release must still win.
    let cands = vec![
        c(
            "libretro",
            "Zelda (USA) (Proto)",
            &[Usa],
            MatchKind::TitleExact,
            95,
        ),
        c(
            "libretro",
            "Zelda (USA, Europe)",
            &[Usa, Europe],
            MatchKind::TitleExact,
            95,
        ),
        c(
            "libretro",
            "Zelda (Europe)",
            &[Europe],
            MatchKind::TitleExact,
            95,
        ),
    ];
    for kind in [
        StrategyKind::Priority,
        StrategyKind::RomRegion,
        StrategyKind::Popularity,
    ] {
        assert_eq!(
            pick(kind, &[Usa], &cands),
            "Zelda (USA, Europe)",
            "{kind:?}"
        );
    }
    let only_proto_or_europe = vec![cands[0].clone(), cands[2].clone()];
    assert_eq!(
        pick(StrategyKind::RomRegion, &[Usa], &only_proto_or_europe),
        "Zelda (Europe)"
    );
}

#[test]
fn every_candidate_is_ranked_once() {
    let regions = default_region_order();
    let ctx = RankContext {
        rom_regions: &[],
        regions: &regions,
        sources: &[],
    };
    let mut idx = strategy_for(StrategyKind::RomRegion).rank(&ctx, &golden());
    idx.sort();
    assert_eq!(idx, [0, 1, 2]);
}

#[test]
fn region_order_parsing() {
    assert_eq!(
        parse_region_order("rom, world,USA,eu,jp").unwrap(),
        [
            RegionPref::RomOwn,
            RegionPref::Region(World),
            RegionPref::Region(Usa),
            RegionPref::Region(Europe),
            RegionPref::Region(Japan),
        ]
    );
    assert_eq!(
        parse_region_order("rom,world,usa,europe,japan").unwrap(),
        default_region_order()
    );
    assert!(parse_region_order("usa,mars").unwrap_err().contains("mars"));
}
