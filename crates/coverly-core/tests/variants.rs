//! M2-11: the libretro candidates of the M2 end-to-end dry run (cache
//! namespace `candidates-2`, 2026-10-16) for two real GBA ROMs of the SD.

mod common;

use std::collections::BTreeMap;

use coverly_core::art::resolve_art;
use coverly_core::gather::{
    select_targets, GatherItem, GatherTargetSummary, Gathered, Scope, TargetFilter,
};
use coverly_core::identify::names::{parse_name, Region};
use coverly_core::identify::{IdMethod, Identity};
use coverly_core::matching::is_unreleased_variant;
use coverly_core::plan::{build_plan, Plan, PlanAction, PlanItem, PlanNote, PlanOptions};
use coverly_core::profiles::Media;
use coverly_core::scan::{scan_library, ScanOptions};
use coverly_core::sources::{Candidate, MatchKind};
use coverly_core::systems::SystemDb;

fn cand(name: &str, regions: &[Region], kind: MatchKind, score: u8) -> Candidate {
    Candidate {
        source: "libretro".into(),
        media: Media::Cover,
        name: name.into(),
        url: format!("https://x/{name}.png"),
        fallback_url: None,
        regions: regions.to_vec(),
        kind,
        score,
        votes: None,
        consensus: 0,
        origin: None,
    }
}

fn real_candidates(stem: &str) -> Vec<Candidate> {
    use MatchKind::*;
    use Region::*;
    match stem {
        "057 Final Fantasy IV" => vec![
            cand(
                "Final Fantasy IV - Sound Restoration hack [Hack by Bregalad]",
                &[],
                Fuzzy,
                90,
            ),
            cand(
                "Final Fantasy IV Advance (Europe) (En,Fr,De,Es,It)",
                &[Europe],
                Fuzzy,
                65,
            ),
            cand("Final Fantasy IV Advance (Japan)", &[Japan], Fuzzy, 65),
            cand(
                "Final Fantasy IV Advance (Japan) (Rev 1)",
                &[Japan],
                Fuzzy,
                65,
            ),
            cand(
                "Final Fantasy IV Advance (Japan) (Rev 1) (Virtual Console)",
                &[Japan],
                Fuzzy,
                65,
            ),
        ],
        "054 Contra Advance" => vec![cand("Contra Advance (USA) (Beta)", &[Usa], TitleExact, 95)],
        "070 Zelda (Beta)" => vec![cand("Zelda (USA) (Beta)", &[Usa], TitleExact, 95)],
        "071 Dumped" => vec![cand("Dumped (USA) (Proto)", &[Usa], Canonical, 100)],
        "072 Golden Sun" => vec![
            cand("Golden Sun (USA) (Beta)", &[Usa], TitleExact, 95),
            cand("Golden Sun (USA, Europe)", &[Usa, Europe], TitleExact, 95),
        ],
        _ => vec![],
    }
}

/// The plan of a GBA folder holding `stems`, with the candidates above.
fn plan_for(stems: &[&str]) -> (tempfile::TempDir, Plan) {
    let dir = tempfile::TempDir::new().unwrap();
    let r = dir.path();
    common::mkdir(r, "anbernic");
    for s in stems {
        common::write(r, &format!("ROMS/GBA/{s}.gba"), s);
    }
    let profile = common::profile("anbernic-stock");
    let db = SystemDb::builtin();
    let scan = scan_library(r, &profile, &db, &ScanOptions::default());
    let art = resolve_art(&scan, &profile, &db, &[Media::Cover]);
    let targets = select_targets(
        &art,
        &TargetFilter {
            systems: vec![],
            media: vec![Media::Cover],
            scope: Scope::Missing,
        },
    );
    let items = targets
        .iter()
        .map(|t| GatherItem {
            target: GatherTargetSummary {
                system_id: t.system_id.clone(),
                folder: t.folder.clone(),
                system_dir: t.system_dir.clone(),
                rel_dir: t.rom.rel_dir.clone(),
                path: t.rom.path.clone(),
                stem: t.rom.stem.clone(),
                media: t.media.clone(),
            },
            identity: Identity {
                canonical: None,
                method: IdMethod::Name,
                confidence: 0.0,
                parsed: parse_name(&t.rom.stem, true),
                hash: None,
            },
            candidates: BTreeMap::from([(Media::Cover, real_candidates(&t.rom.stem))]),
            errors: BTreeMap::new(),
        })
        .collect();
    let gathered = Gathered {
        items,
        ..Gathered::default()
    };
    let plan = build_plan(r, &profile, &db, &gathered, &PlanOptions::default());
    (dir, plan)
}

fn item<'a>(plan: &'a Plan, stem: &str) -> &'a PlanItem {
    plan.items.iter().find(|i| i.stem == stem).unwrap()
}

#[test]
fn final_fantasy_iv_never_takes_the_sound_restoration_hack_as_auto() {
    let (_dir, plan) = plan_for(&["057 Final Fantasy IV"]);
    let it = item(&plan, "057 Final Fantasy IV");
    let PlanAction::Review {
        chosen,
        alternatives,
        ..
    } = &it.action
    else {
        panic!("{:?}", it.action)
    };
    // The retail GBA game is Final Fantasy IV Advance; the hack stays an option.
    assert_eq!(
        chosen.name,
        "Final Fantasy IV Advance (Europe) (En,Fr,De,Es,It)"
    );
    assert!(alternatives
        .iter()
        .any(|c| c.name.contains("Sound Restoration hack")));
    assert_eq!(it.note, Some(PlanNote::UnreleasedVariant));
    assert_eq!(plan.totals.auto, 0);
}

#[test]
fn contra_advance_beta_goes_to_review() {
    let (_dir, plan) = plan_for(&["054 Contra Advance"]);
    let it = item(&plan, "054 Contra Advance");
    let PlanAction::Review { chosen, .. } = &it.action else {
        panic!("{:?}", it.action)
    };
    assert_eq!(chosen.name, "Contra Advance (USA) (Beta)");
    assert_eq!(it.note, Some(PlanNote::UnreleasedVariant));
}

#[test]
fn a_beta_rom_may_take_beta_art_automatically() {
    let (_dir, plan) = plan_for(&["070 Zelda (Beta)"]);
    let it = item(&plan, "070 Zelda (Beta)");
    assert!(
        matches!(it.action, PlanAction::Auto { .. }),
        "{:?}",
        it.action
    );
    assert_eq!(it.note, None);
}

#[test]
fn a_canonical_match_to_a_prototype_stays_auto() {
    let (_dir, plan) = plan_for(&["071 Dumped"]);
    assert!(matches!(
        item(&plan, "071 Dumped").action,
        PlanAction::Auto { .. }
    ));
}

#[test]
fn a_release_in_the_best_tier_still_wins_as_auto() {
    let (_dir, plan) = plan_for(&["072 Golden Sun"]);
    let it = item(&plan, "072 Golden Sun");
    let PlanAction::Auto { chosen, .. } = &it.action else {
        panic!("{:?}", it.action)
    };
    assert_eq!(chosen.name, "Golden Sun (USA, Europe)");
    assert_eq!(it.note, None);
}

#[test]
fn variant_tags_are_recognised() {
    for name in [
        "Contra Advance (USA) (Beta)",
        "Zelda (USA) (Proto 2)",
        "Tetris (Demo)",
        "Game (USA) (Sample)",
        "Final Fantasy IV - Sound Restoration hack [Hack by Bregalad]",
        "Sonic [h1]",
        "Sonic [h]",
        "Mario (Sound Restoration Hack)",
        "Action 52 (USA) (Unl)",
        "Game (Unlicensed)",
        "Pirates (Pirate)",
        "Game (Kiosk)",
        "Game (Debug Version)",
    ] {
        assert!(is_unreleased_variant(name), "{name}");
    }
    for name in [
        "Golden Sun (USA, Europe)",
        ".hack--Infection (USA)",
        "Hackers (USA)",
        "Demolition Man (USA)",
        "Shadow Hearts (Rev 1)",
        "Betrayal at Krondor",
        "Sonic [!]",
        "Unclosed (Beta",
    ] {
        assert!(!is_unreleased_variant(name), "{name}");
    }
}
