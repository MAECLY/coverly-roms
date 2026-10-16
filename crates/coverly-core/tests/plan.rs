mod common;

use std::collections::BTreeMap;
use std::path::PathBuf;

use coverly_core::art::{resolve_art, target_path};
use coverly_core::gather::{
    select_targets, GatherItem, GatherTargetSummary, Gathered, Scope, TargetFilter,
};
use coverly_core::identify::names::{parse_name, Region};
use coverly_core::identify::{IdMethod, Identity};
use coverly_core::plan::{build_plan, NoneReason, PlanAction, PlanOptions};
use coverly_core::profiles::Media;
use coverly_core::scan::{scan_library, ScanOptions};
use coverly_core::sources::{Candidate, MatchKind};
use coverly_core::systems::SystemDb;

fn lib() -> tempfile::TempDir {
    let dir = tempfile::TempDir::new().unwrap();
    let r = dir.path();
    common::mkdir(r, "anbernic");
    for f in [
        "001 Golden Sun.gba",
        "002 Goldn Adventure.gba",
        "003 Nothing.gba",
        "Hacks/004 Odd.gba",
        "005 Broken.gba",
    ] {
        common::write(r, &format!("ROMS/GBA/{f}"), f);
    }
    dir
}

fn cand(name: &str, kind: MatchKind, score: u8) -> Candidate {
    Candidate {
        source: "libretro".into(),
        media: Media::Cover,
        name: name.into(),
        url: format!("https://x/{name}.png"),
        fallback_url: None,
        regions: vec![Region::Usa],
        kind,
        score,
        votes: None,
        consensus: 0,
        origin: None,
    }
}

fn gathered(root: &std::path::Path) -> Gathered {
    let profile = common::profile("anbernic-stock");
    let db = SystemDb::builtin();
    let scan = scan_library(root, &profile, &db, &ScanOptions::default());
    let art = resolve_art(&scan, &profile, &db, &[Media::Cover]);
    let targets = select_targets(
        &art,
        &TargetFilter {
            systems: vec![],
            media: vec![Media::Cover],
            scope: Scope::Missing,
        },
    );
    // Scan order is rel_dir then stem: 001, 002, 003, 005, Hacks/004 — so map by stem.
    let cands_for = |stem: &str| -> Vec<Candidate> {
        match stem {
            "001 Golden Sun" => vec![cand("Golden Sun (USA, Europe)", MatchKind::Canonical, 100)],
            "002 Goldn Adventure" => vec![cand("Golden Adventure (USA)", MatchKind::Fuzzy, 70)],
            "004 Odd" => vec![cand("Something (USA)", MatchKind::Fuzzy, 60)],
            _ => vec![],
        }
    };
    let items = targets
        .iter()
        .map(|t| (t, cands_for(&t.rom.stem)))
        .map(|(t, c)| GatherItem {
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
            errors: if t.rom.stem == "005 Broken" {
                BTreeMap::from([(Media::Cover, "HTTP 500".to_string())])
            } else {
                BTreeMap::new()
            },
            candidates: BTreeMap::from([(Media::Cover, c)]),
        })
        .collect();
    Gathered {
        items,
        ..Gathered::default()
    }
}

#[test]
fn target_paths_follow_the_profile_template() {
    let root = PathBuf::from("/sd");
    let profile = common::profile("anbernic-stock");
    let db = SystemDb::builtin();
    let gba = db.get("gba").unwrap();
    let sys_dir = root.join("ROMS/GBA");
    assert_eq!(
        target_path(
            &root,
            &profile,
            &sys_dir,
            gba,
            &PathBuf::from("Hacks"),
            "004 Odd",
            Media::Cover
        ),
        Some(root.join("ROMS/GBA/Hacks/Imgs/004 Odd.png"))
    );
    assert_eq!(
        target_path(
            &root,
            &profile,
            &sys_dir,
            gba,
            &PathBuf::new(),
            "x",
            Media::Screenshot
        ),
        None
    );
}

#[test]
fn classifies_items_by_thresholds() {
    let dir = lib();
    let plan = build_plan(
        dir.path(),
        &common::profile("anbernic-stock"),
        &SystemDb::builtin(),
        &gathered(dir.path()),
        &PlanOptions::default(),
    );
    let by_stem = |s: &str| plan.items.iter().find(|i| i.stem == s).unwrap();
    match &by_stem("001 Golden Sun").action {
        PlanAction::Auto {
            chosen, confidence, ..
        } => {
            assert_eq!(chosen.name, "Golden Sun (USA, Europe)");
            assert_eq!(*confidence, 1.0);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        by_stem("001 Golden Sun").target,
        dir.path().join("ROMS/GBA/Imgs/001 Golden Sun.png")
    );
    assert!(
        matches!(by_stem("002 Goldn Adventure").action, PlanAction::Review { confidence, .. } if (confidence - 0.70).abs() < 1e-6)
    );
    assert!(matches!(
        by_stem("003 Nothing").action,
        PlanAction::None {
            reason: NoneReason::NoCandidates
        }
    ));
    assert!(matches!(
        by_stem("004 Odd").action,
        PlanAction::None {
            reason: NoneReason::BelowThreshold { .. }
        }
    ));
    assert!(
        matches!(&by_stem("005 Broken").action, PlanAction::None { reason: NoneReason::SourceError { message } } if message.contains("500"))
    );
    assert_eq!(
        by_stem("004 Odd").target,
        dir.path().join("ROMS/GBA/Hacks/Imgs/004 Odd.png")
    );
    let gba = plan.per_system["GBA"];
    assert_eq!((gba.auto, gba.review, gba.none), (1, 1, 3));
    assert_eq!(plan.totals, gba);
}

#[test]
fn plan_serialises_with_flat_actions() {
    let dir = lib();
    let plan = build_plan(
        dir.path(),
        &common::profile("anbernic-stock"),
        &SystemDb::builtin(),
        &gathered(dir.path()),
        &PlanOptions::default(),
    );
    let v = serde_json::to_value(&plan).unwrap();
    let first = &v["items"][0];
    assert_eq!(first["action"], "auto");
    assert_eq!(first["media"], "cover");
    assert_eq!(first["chosen"]["kind"], "canonical");
    assert!(v["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i["reason"] == "no-candidates"));
}

use coverly_core::matching::{PickReason, StrategyKind};
use coverly_core::plan::Decision;

#[test]
fn every_chosen_item_says_why_and_none_items_do_not() {
    let dir = lib();
    let g = gathered(dir.path());
    let plan = build_plan(
        dir.path(),
        &common::profile("anbernic-stock"),
        &SystemDb::builtin(),
        &g,
        &PlanOptions::default(),
    );
    for item in &plan.items {
        match &item.action {
            PlanAction::None { .. } => assert_eq!(item.reason, None, "{}", item.stem),
            _ => assert_eq!(item.reason, Some(PickReason::Only), "{}", item.stem),
        }
    }
    let json = serde_json::to_value(&plan).unwrap();
    let golden = json["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["stem"] == "001 Golden Sun")
        .unwrap();
    assert_eq!(golden["reason"], serde_json::json!({ "why": "only" }));
}

#[test]
fn popularity_items_explain_votes_and_a_pick_becomes_the_users() {
    let dir = lib();
    let mut g = gathered(dir.path());
    let golden = g
        .items
        .iter_mut()
        .find(|i| i.target.stem == "001 Golden Sun")
        .unwrap();
    let mut grid = cand("Golden Sun", MatchKind::TitleExact, 95);
    grid.source = "steamgriddb".into();
    grid.regions = vec![];
    grid.url = "https://cdn2.steamgriddb.com/grid/1.png".into();
    grid.votes = Some(coverly_core::sources::Votes { up: 8, down: 1 });
    golden.candidates.get_mut(&Media::Cover).unwrap().push(grid);
    let opts = PlanOptions {
        strategy: StrategyKind::Popularity,
        sources: vec!["libretro".into(), "steamgriddb".into()],
        ..PlanOptions::default()
    };
    let mut plan = build_plan(
        dir.path(),
        &common::profile("anbernic-stock"),
        &SystemDb::builtin(),
        &g,
        &opts,
    );
    let n = plan
        .items
        .iter()
        .position(|i| i.stem == "001 Golden Sun")
        .unwrap();
    assert_eq!(
        plan.items[n].reason,
        Some(PickReason::Votes { score: 7, next: 0 })
    );
    plan.decide(&BTreeMap::from([(n, Decision::Pick { candidate: 1 })]));
    assert_eq!(plan.items[n].reason, Some(PickReason::User));
}
