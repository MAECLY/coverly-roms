use coverly_core::identify::names::Region::{self, *};
use coverly_core::matching::{
    default_region_order, parse_region_order, strategy_for, RankContext, RegionPref, StrategyKind,
};
use coverly_core::profiles::Media;
use coverly_core::sources::{Candidate, MatchKind, Votes};

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
        origin: None,
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
        embedded_first: false,
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
    cands[0].votes = Some(Votes { up: 2, down: 0 });
    cands[1].votes = Some(Votes { up: 10, down: 0 });
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
        embedded_first: false,
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
        embedded_first: false,
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

use coverly_core::matching::{explain, PickReason};

fn sgdb(name: &str, up: u32, down: u32, kind: MatchKind, score: u8) -> Candidate {
    Candidate {
        votes: Some(Votes { up, down }),
        origin: Some("game 1 · grid 2".into()),
        url: format!("https://cdn2.steamgriddb.com/{name}-{up}-{down}.png"),
        ..c("steamgriddb", name, &[], kind, score)
    }
}

fn pick_with(
    kind: StrategyKind,
    rom: &[Region],
    cands: &[Candidate],
    sources: &[&str],
) -> Candidate {
    let regions = default_region_order();
    let sources: Vec<String> = sources.iter().map(|s| s.to_string()).collect();
    let ctx = RankContext {
        rom_regions: rom,
        regions: &regions,
        sources: &sources,
        embedded_first: false,
    };
    cands[strategy_for(kind).rank(&ctx, cands)[0]].clone()
}

const BOTH: [&str; 2] = ["libretro", "steamgriddb"];

#[test]
fn popularity_lets_votes_beat_an_exact_libretro_name() {
    let cands = vec![
        c(
            "libretro",
            "Golden Sun (USA, Europe)",
            &[Usa, Europe],
            MatchKind::Canonical,
            100,
        ),
        sgdb("Golden Sun", 12, 2, MatchKind::TitleExact, 95),
    ];
    assert_eq!(
        pick_with(StrategyKind::Popularity, &[], &cands, &BOTH).source,
        "steamgriddb"
    );
    // Priority and RomRegion never look at votes.
    assert_eq!(
        pick_with(StrategyKind::Priority, &[], &cands, &BOTH).source,
        "libretro"
    );
    assert_eq!(
        pick_with(StrategyKind::RomRegion, &[], &cands, &BOTH).source,
        "libretro"
    );
}

#[test]
fn popularity_ignores_votes_of_a_fuzzy_match() {
    let cands = vec![
        c("libretro", "Doom (USA)", &[Usa], MatchKind::TitleExact, 95),
        sgdb("Doom", 500, 0, MatchKind::Fuzzy, 85),
    ];
    assert_eq!(
        pick_with(StrategyKind::Popularity, &[], &cands, &BOTH).source,
        "libretro"
    );
}

#[test]
fn unvoted_or_downvoted_grids_lose_to_the_stronger_exact_name() {
    for (up, down) in [(0, 0), (1, 4)] {
        let cands = vec![
            sgdb("Golden Sun", up, down, MatchKind::TitleExact, 95),
            c(
                "libretro",
                "Golden Sun (USA, Europe)",
                &[Usa, Europe],
                MatchKind::Canonical,
                100,
            ),
        ];
        assert_eq!(
            pick_with(StrategyKind::Popularity, &[], &cands, &BOTH).source,
            "libretro",
            "{up}/{down}"
        );
    }
}

#[test]
fn popularity_between_grids_is_by_net_votes() {
    let cands = vec![
        sgdb("Golden Sun", 9, 8, MatchKind::TitleExact, 95),
        sgdb("Golden Sun", 4, 0, MatchKind::TitleExact, 95),
    ];
    let won = pick_with(StrategyKind::Popularity, &[], &cands, &BOTH);
    assert_eq!(won.votes, Some(Votes { up: 4, down: 0 }));
}

#[test]
fn popularity_without_votes_or_consensus_follows_the_source_order() {
    let mut cands = vec![
        c(
            "libretro",
            "Golden Sun (USA)",
            &[Usa],
            MatchKind::TitleExact,
            95,
        ),
        c(
            "screenscraper",
            "Golden Sun",
            &[Usa],
            MatchKind::TitleExact,
            95,
        ),
    ];
    assert_eq!(
        pick_with(
            StrategyKind::Popularity,
            &[],
            &cands,
            &["screenscraper", "libretro"]
        )
        .source,
        "screenscraper"
    );
    cands.reverse();
    assert_eq!(
        pick_with(
            StrategyKind::Popularity,
            &[],
            &cands,
            &["libretro", "screenscraper"]
        )
        .source,
        "libretro"
    );
}

#[test]
fn popularity_is_deterministic_for_any_input_order() {
    let base = vec![
        c(
            "libretro",
            "Golden Sun (USA, Europe)",
            &[Usa, Europe],
            MatchKind::Canonical,
            100,
        ),
        c(
            "libretro",
            "Golden Sun (Japan)",
            &[Japan],
            MatchKind::TitleExact,
            95,
        ),
        sgdb("Golden Sun", 3, 1, MatchKind::TitleExact, 95),
        sgdb("Golden Sun", 3, 1, MatchKind::TitleExact, 95),
        sgdb("Golden Sum", 50, 0, MatchKind::Fuzzy, 80),
    ];
    let names = |cands: &[Candidate]| {
        let regions = default_region_order();
        let sources: Vec<String> = BOTH.iter().map(|s| s.to_string()).collect();
        let ctx = RankContext {
            rom_regions: &[],
            regions: &regions,
            sources: &sources,
            embedded_first: false,
        };
        strategy_for(StrategyKind::Popularity)
            .rank(&ctx, cands)
            .into_iter()
            .map(|i| cands[i].url.clone())
            .collect::<Vec<_>>()
    };
    let want = names(&base);
    let mut rev = base.clone();
    rev.reverse();
    assert_eq!(names(&rev), want);
    let mut rot = base.clone();
    rot.rotate_left(2);
    assert_eq!(names(&rot), want);
}

fn why(kind: StrategyKind, rom: &[Region], cands: &[Candidate], sources: &[&str]) -> PickReason {
    let regions = default_region_order();
    let sources: Vec<String> = sources.iter().map(|s| s.to_string()).collect();
    let ctx = RankContext {
        rom_regions: rom,
        regions: &regions,
        sources: &sources,
        embedded_first: false,
    };
    let order = strategy_for(kind).rank(&ctx, cands);
    explain(
        kind,
        &ctx,
        &cands[order[0]],
        order.get(1).map(|&i| &cands[i]),
        &[],
    )
}

#[test]
fn explain_names_the_deciding_rule() {
    let lib = |n: &str, r: &[Region], k, s| c("libretro", n, r, k, s);
    assert_eq!(
        why(
            StrategyKind::Popularity,
            &[],
            &[lib("A (USA)", &[Usa], MatchKind::Canonical, 100)],
            &BOTH
        ),
        PickReason::Only
    );
    assert_eq!(
        why(
            StrategyKind::Popularity,
            &[],
            &[
                lib("A (USA)", &[Usa], MatchKind::Canonical, 100),
                sgdb("A", 7, 2, MatchKind::TitleExact, 95)
            ],
            &BOTH
        ),
        PickReason::Votes { score: 5, next: 0 }
    );
    let mut a = lib("A (USA)", &[Usa], MatchKind::TitleExact, 95);
    a.consensus = 3;
    let mut b = lib("A (Japan)", &[Japan], MatchKind::TitleExact, 95);
    b.consensus = 1;
    assert_eq!(
        why(
            StrategyKind::Popularity,
            &[],
            &[b.clone(), a.clone()],
            &BOTH
        ),
        PickReason::Consensus { count: 3, next: 1 }
    );
    assert_eq!(
        why(
            StrategyKind::Popularity,
            &[],
            &[
                lib("A (USA)", &[Usa], MatchKind::Canonical, 100),
                lib("A (Japan)", &[Japan], MatchKind::TitleExact, 95)
            ],
            &BOTH
        ),
        PickReason::BetterMatch
    );
    assert_eq!(
        why(
            StrategyKind::Priority,
            &[],
            &[
                c("steamgriddb", "A", &[], MatchKind::TitleExact, 95),
                lib("A (USA)", &[Usa], MatchKind::TitleExact, 95)
            ],
            &BOTH
        ),
        PickReason::SourceOrder {
            source: "libretro".into()
        }
    );
    assert_eq!(
        why(
            StrategyKind::Priority,
            &[],
            &[
                lib("A (Japan)", &[Japan], MatchKind::TitleExact, 95),
                lib("A (USA)", &[Usa], MatchKind::TitleExact, 95)
            ],
            &BOTH
        ),
        PickReason::Region { rom: false }
    );
    assert_eq!(
        why(
            StrategyKind::RomRegion,
            &[Japan],
            &[
                lib("A (USA)", &[Usa], MatchKind::TitleExact, 95),
                lib("A (Japan)", &[Japan], MatchKind::TitleExact, 95)
            ],
            &BOTH
        ),
        PickReason::Region { rom: true }
    );
    assert_eq!(
        why(
            StrategyKind::Priority,
            &[],
            &[
                lib("A (USA) (Beta)", &[Usa], MatchKind::TitleExact, 95),
                lib("A (USA)", &[Usa], MatchKind::TitleExact, 95)
            ],
            &BOTH
        ),
        PickReason::Release
    );
    assert_eq!(
        why(
            StrategyKind::Priority,
            &[],
            &[
                lib("A (USA) (v1)", &[Usa], MatchKind::TitleExact, 95),
                lib("A (USA) (v2)", &[Usa], MatchKind::TitleExact, 95)
            ],
            &BOTH
        ),
        PickReason::Tie
    );
    assert_eq!(
        serde_json::to_value(PickReason::Votes { score: 5, next: 0 }).unwrap(),
        serde_json::json!({"why":"votes","score":5,"next":0})
    );
}

/// Systems batch 3 D21: with `embedded_first` the icon inside the game
/// beats a canonical box art of the ROM's own region in every strategy,
/// and the «why» names the source order; without it the icon is last.
#[test]
fn embedded_first_puts_the_icon_inside_the_game_on_top() {
    use coverly_core::matching::{explain, PickReason};
    let regions = default_region_order();
    let sources = vec!["embedded".to_string(), "libretro".to_string()];
    let cands = vec![
        c(
            "libretro",
            "Test Game (Japan)",
            &[Japan],
            MatchKind::Canonical,
            100,
        ),
        c(
            "embedded",
            "Test Game (Japan)",
            &[],
            MatchKind::Canonical,
            100,
        ),
    ];
    for kind in [
        StrategyKind::Priority,
        StrategyKind::RomRegion,
        StrategyKind::Popularity,
    ] {
        for embedded_first in [true, false] {
            let ctx = RankContext {
                rom_regions: &[Japan],
                regions: &regions,
                sources: &sources,
                embedded_first,
            };
            let order = strategy_for(kind).rank(&ctx, &cands);
            let top = &cands[order[0]];
            let want = if embedded_first {
                "embedded"
            } else {
                "libretro"
            };
            assert_eq!(top.source, want, "{kind:?} {embedded_first}");
            if embedded_first {
                assert_eq!(
                    explain(kind, &ctx, top, Some(&cands[order[1]]), &[]),
                    PickReason::SourceOrder {
                        source: "embedded".into()
                    },
                    "{kind:?}"
                );
            }
        }
    }
}
