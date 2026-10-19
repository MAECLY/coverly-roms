//! Duplicates: choosing the copy to keep (spec §6). Every candidate here is
//! synthetic; the only files on disk are game lists in temporary folders.

use std::fs;
use std::path::{Path, PathBuf};

use coverly_core::dedupe::keeper::{
    choose, container_name, played_from_gamelist, region_order, remembered_pick_holds,
    saves_fingerprint, Candidate, ContainerOrder, Played,
};
use coverly_core::dedupe::units::Unit;
use coverly_core::dedupe::{DumpQuality, KeepReason, Relation, SaveInfo, SaveKey, UnitKind};
use coverly_core::identify::names::{parse_tags, Region, ReleaseClass};
use coverly_core::settings::{ContainerClass, DedupeSettings, KeepRule, RuleToggle};

// --- Builders ---

fn unit(id: usize, system: &str, file: &str, kind: UnitKind, mtime_ns: i64) -> Unit {
    let primary = PathBuf::from(system).join(file);
    Unit {
        id,
        system: system.to_string(),
        primary: primary.clone(),
        files: vec![primary.clone()],
        kind,
        size: 512 * 1024,
        payload_size: Some(512 * 1024),
        mtime_ns,
        file_id: None,
        patches: Vec::new(),
        rel_dir: PathBuf::new(),
        stem: primary.file_stem().unwrap().to_string_lossy().into_owned(),
        ext: primary
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default(),
        in_library: true,
    }
}

fn save(path: &str, seed: u8) -> SaveInfo {
    SaveInfo {
        path: PathBuf::from(path),
        size: 8192,
        mtime_ns: 1,
        sha1: format!("{seed:02x}").repeat(20),
        keyed_by: SaveKey::Stem,
    }
}

fn cand<'a>(member: usize, unit: &'a Unit) -> Candidate<'a> {
    Candidate {
        member,
        unit,
        saves: &[],
        played: None,
        tags: None,
        dat_name: None,
        has_cover: false,
        relation: Relation::Exact,
        headered_functional: None,
    }
}

fn all_rules() -> Vec<RuleToggle> {
    DedupeSettings::default().keep_rules
}

fn no_order() -> ContainerOrder {
    ContainerOrder::none("anbernic-stock")
}

fn default_regions() -> Vec<Region> {
    region_order(&["rom", "world", "usa", "europe", "japan"].map(String::from))
}

fn pick(
    cands: &[Candidate],
    rules: &[RuleToggle],
    order: &ContainerOrder,
) -> (usize, KeepReason, bool) {
    let (keeper, reason, _, tie) = choose(cands, rules, &default_regions(), order, None, None);
    (keeper, reason, tie)
}

// --- One test per rule ---

#[test]
fn has_save_rule() {
    let a = unit(0, "GB", "Tetris.gb", UnitKind::Single, 10);
    let b = unit(1, "GB", "Tetris (1).gb", UnitKind::Single, 5);
    let saves = [save("GB/Tetris (1).srm", 7)];
    let mut cb = cand(1, &b);
    cb.saves = &saves;
    let (k, reason, tie) = pick(&[cand(0, &a), cb], &all_rules(), &no_order());
    assert_eq!(k, 1);
    assert_eq!(
        reason,
        KeepReason::HasSave {
            path: PathBuf::from("GB/Tetris (1).srm")
        }
    );
    assert!(!tie);
}

#[test]
fn played_rule() {
    let a = unit(0, "GB", "Tetris.gb", UnitKind::Single, 5);
    let b = unit(1, "GB", "Tetris (1).gb", UnitKind::Single, 10);
    let mut cb = cand(1, &b);
    cb.played = Some(Played {
        playcount: 3,
        lastplayed: Some("20260101T120000".into()),
        favorite: false,
    });
    let (k, reason, _) = pick(&[cand(0, &a), cb], &all_rules(), &no_order());
    assert_eq!(k, 1);
    assert_eq!(reason, KeepReason::Played { playcount: 3 });
}

#[test]
fn in_library_rule() {
    let a = unit(0, "GB", "Tetris.gb", UnitKind::Single, 5);
    let mut b = unit(1, "GB", "Tetris.gb", UnitKind::Single, 10);
    b.in_library = true;
    let mut a2 = a.clone();
    a2.in_library = false;
    let (k, reason, _) = pick(&[cand(0, &a2), cand(1, &b)], &all_rules(), &no_order());
    assert_eq!(k, 1);
    assert_eq!(reason, KeepReason::InLibrary);
}

#[test]
fn verified_rule_prefers_a_catalogue_hit_and_a_good_dump() {
    let a = unit(0, "GB", "Tetris.gb", UnitKind::Single, 5);
    let b = unit(1, "GB", "Tetris (W).gb", UnitKind::Single, 10);
    let mut cb = cand(1, &b);
    cb.dat_name = Some("Tetris (World)");
    let (k, reason, _) = pick(&[cand(0, &a), cb], &all_rules(), &no_order());
    assert_eq!(k, 1);
    assert_eq!(
        reason,
        KeepReason::Verified {
            kept: DumpQuality::DatMatch,
            other: DumpQuality::Unknown
        }
    );

    let bad_tags = parse_tags("Tetris (W) [b1]");
    let good_tags = parse_tags("Tetris (W) [!]");
    let mut ca = cand(0, &a);
    ca.tags = Some(&bad_tags);
    let mut cb = cand(1, &b);
    cb.tags = Some(&good_tags);
    cb.relation = Relation::Version;
    let (k, reason, _) = pick(&[ca, cb], &all_rules(), &no_order());
    assert_eq!(k, 1);
    assert_eq!(
        reason,
        KeepReason::Verified {
            kept: DumpQuality::Good,
            other: DumpQuality::Bad
        }
    );
}

#[test]
fn retail_rule_only_between_versions() {
    let a = unit(0, "GB", "Tetris (Beta).gb", UnitKind::Single, 5);
    let b = unit(1, "GB", "Tetris.gb", UnitKind::Single, 10);
    let beta = parse_tags("Tetris (World) (Beta)");
    let retail = parse_tags("Tetris (World)");
    let mut ca = cand(0, &a);
    ca.tags = Some(&beta);
    let mut cb = cand(1, &b);
    cb.tags = Some(&retail);
    cb.relation = Relation::Version;
    let (k, reason, _) = pick(&[ca.clone(), cb.clone()], &all_rules(), &no_order());
    assert_eq!(k, 1);
    assert_eq!(
        reason,
        KeepReason::Retail {
            other: ReleaseClass::Beta
        }
    );
    // Proven exact copies never weigh the release class: the tie decides.
    cb.relation = Relation::Exact;
    let (k, reason, tie) = pick(&[ca, cb], &all_rules(), &no_order());
    assert_eq!((k, reason, tie), (0, KeepReason::Tie, true));
}

#[test]
fn region_rule() {
    let a = unit(0, "GB", "Tetris (J).gb", UnitKind::Single, 5);
    let b = unit(1, "GB", "Tetris (U).gb", UnitKind::Single, 10);
    let jp = parse_tags("Tetris (Japan)");
    let us = parse_tags("Tetris (USA)");
    let mut ca = cand(0, &a);
    ca.tags = Some(&jp);
    ca.relation = Relation::Version;
    let mut cb = cand(1, &b);
    cb.tags = Some(&us);
    cb.relation = Relation::Version;
    let (k, reason, _) = pick(&[ca, cb], &all_rules(), &no_order());
    assert_eq!(k, 1);
    assert_eq!(
        reason,
        KeepReason::Region {
            kept: Region::Usa,
            other: Region::Japan
        }
    );
}

#[test]
fn revision_rule() {
    let a = unit(0, "GB", "Tetris.gb", UnitKind::Single, 5);
    let b = unit(1, "GB", "Tetris (Rev 1).gb", UnitKind::Single, 10);
    let v0 = parse_tags("Tetris (World)");
    let v1 = parse_tags("Tetris (World) (Rev 1)");
    let mut ca = cand(0, &a);
    ca.tags = Some(&v0);
    ca.relation = Relation::Version;
    let mut cb = cand(1, &b);
    cb.tags = Some(&v1);
    cb.relation = Relation::Version;
    let (k, reason, _) = pick(&[ca, cb], &all_rules(), &no_order());
    assert_eq!(k, 1);
    assert_eq!(
        reason,
        KeepReason::Revision {
            kept: "Rev 1".into(),
            other: String::new()
        }
    );
}

#[test]
fn container_rule() {
    let a = unit(0, "PS", "Crash Bash.cue", UnitKind::DiscSet, 5);
    let b = unit(1, "PS", "Crash Bash.pbp", UnitKind::Image, 10);
    let order = ContainerOrder::new("anbernic-stock", &["chd", "pbp", "cue"]);
    let mut cb = cand(1, &b);
    cb.relation = Relation::Format;
    let (k, reason, _) = pick(&[cand(0, &a), cb], &all_rules(), &order);
    assert_eq!(k, 1);
    assert_eq!(
        reason,
        KeepReason::Container {
            kept: "pbp".into(),
            other: "cue".into(),
            profile: "anbernic-stock".into()
        }
    );
}

#[test]
fn cover_rule_decides_when_only_one_has_art() {
    let a = unit(0, "GB", "Tetris.gb", UnitKind::Single, 5);
    let b = unit(1, "GB", "Tetris (1).gb", UnitKind::Single, 10);
    let mut cb = cand(1, &b);
    cb.has_cover = true;
    let (k, reason, _) = pick(&[cand(0, &a), cb.clone()], &all_rules(), &no_order());
    assert_eq!((k, reason), (1, KeepReason::HasCover));
    // Both with art: the cover says nothing, the tie decides.
    let mut ca = cand(0, &a);
    ca.has_cover = true;
    let (k, reason, tie) = pick(&[ca, cb], &all_rules(), &no_order());
    assert_eq!((k, reason, tie), (0, KeepReason::Tie, true));
}

#[test]
fn catalogue_name_only_among_proven_copies() {
    // Three identical Game Boy files; only one is named as the catalogue
    // names the game, and it is neither the oldest nor the first path.
    let a = unit(
        0,
        "GB",
        "Mega Man - Dr. Wily's Revenge (U).gb",
        UnitKind::Single,
        1,
    );
    let b = unit(
        1,
        "GB",
        "Mega Man - Dr. Wily's Revenge (USA).gb",
        UnitKind::Single,
        9,
    );
    let c = unit(2, "GB", "Mega Man (1).gb", UnitKind::Single, 2);
    let dat = "Mega Man - Dr. Wily's Revenge (USA)";
    let cs: Vec<Candidate> = [&a, &b, &c]
        .iter()
        .enumerate()
        .map(|(i, u)| Candidate {
            dat_name: Some(dat),
            ..cand(i, u)
        })
        .collect();
    let (k, reason, _) = pick(&cs, &all_rules(), &no_order());
    assert_eq!(k, 1);
    assert_eq!(reason, KeepReason::CatalogueName { name: dat.into() });

    // Between other versions the name never weighs.
    let mut cs = cs;
    cs[1].relation = Relation::Version;
    cs[1].dat_name = Some(dat);
    let (k, _, _) = pick(&cs[..2], &all_rules(), &no_order());
    assert_eq!(
        k, 0,
        "oldest wins: the catalogue name is not used for versions"
    );
}

#[test]
fn tie_is_oldest_then_path_and_flagged() {
    let a = unit(0, "GB", "b.gb", UnitKind::Single, 10);
    let b = unit(1, "GB", "a.gb", UnitKind::Single, 5);
    let (k, reason, tie) = pick(&[cand(0, &a), cand(1, &b)], &all_rules(), &no_order());
    assert_eq!((k, reason, tie), (1, KeepReason::Tie, true));
    // Same time: path order.
    let b = unit(1, "GB", "a.gb", UnitKind::Single, 10);
    let (k, _, tie) = pick(&[cand(0, &a), cand(1, &b)], &all_rules(), &no_order());
    assert_eq!((k, tie), (1, true));
    // Input order does not matter.
    let (k, _, _) = pick(&[cand(1, &b), cand(0, &a)], &all_rules(), &no_order());
    assert_eq!(k, 1);
}

// --- The plan's named cases ---

#[test]
fn tetris_v11_stays_over_v10_with_1g1r() {
    let a = unit(0, "GB", "Tetris (JUE) (V1.0) [!].gb", UnitKind::Single, 1);
    let b = unit(1, "GB", "Tetris (World) (Rev 1).gb", UnitKind::Single, 9);
    let v10 = parse_tags("Tetris (Japan) (En)");
    let v11 = parse_tags("Tetris (World) (Rev 1)");
    let mut ca = cand(0, &a);
    ca.tags = Some(&v10);
    ca.dat_name = Some("Tetris (Japan) (En)");
    let mut cb = cand(1, &b);
    cb.tags = Some(&v11);
    cb.dat_name = Some("Tetris (World) (Rev 1)");
    cb.relation = Relation::Version;
    let (k, reason, runner, tie) = choose(
        &[ca, cb],
        &all_rules(),
        &default_regions(),
        &no_order(),
        None,
        None,
    );
    assert_eq!(k, 1);
    assert_eq!(
        reason,
        KeepReason::Region {
            kept: Region::World,
            other: Region::Japan
        }
    );
    assert_eq!(runner.map(|r| r.0), Some(0));
    assert!(!tie);
    // With the region rule off, the revision agrees.
    let rules: Vec<RuleToggle> = all_rules()
        .into_iter()
        .map(|t| RuleToggle {
            on: t.rule != KeepRule::Region,
            ..t
        })
        .collect();
    let mut ca = cand(0, &a);
    ca.tags = Some(&v10);
    let mut cb = cand(1, &b);
    cb.tags = Some(&v11);
    cb.relation = Relation::Version;
    let (k, reason, _) = pick(&[ca, cb], &rules, &no_order());
    assert_eq!(k, 1);
    assert!(matches!(reason, KeepReason::Revision { .. }), "{reason:?}");
}

#[test]
fn headerless_snes_wins_verified_but_headerless_nes_never_does() {
    // SNES: a 512-byte copier header is not part of the game.
    let a = unit(0, "SNES", "Zelda.smc", UnitKind::Single, 1);
    let b = unit(1, "SNES", "Zelda.sfc", UnitKind::Single, 9);
    let mut ca = cand(0, &a);
    ca.headered_functional = Some(false);
    let (k, reason, _) = pick(&[ca, cand(1, &b)], &all_rules(), &no_order());
    assert_eq!(k, 1);
    assert_eq!(
        reason,
        KeepReason::Verified {
            kept: DumpQuality::Headerless,
            other: DumpQuality::Headered
        }
    );
    // NES: the iNES header is what emulators read; the headerless copy
    // never wins, whatever its age.
    let a = unit(0, "NES", "Zelda.nes", UnitKind::Single, 9);
    let b = unit(1, "NES", "Zelda (raw).nes", UnitKind::Single, 1);
    let mut ca = cand(0, &a);
    ca.headered_functional = Some(true);
    let (k, reason, _) = pick(&[ca, cand(1, &b)], &all_rules(), &no_order());
    assert_eq!(k, 0);
    assert_eq!(
        reason,
        KeepReason::Verified {
            kept: DumpQuality::Headered,
            other: DumpQuality::Headerless
        }
    );
}

#[test]
fn pbp_wins_container_on_anbernic_and_cue_on_es_de() {
    let s = DedupeSettings::default();
    let cue = unit(0, "PS", "Crash Bash.cue", UnitKind::DiscSet, 1);
    let pbp = unit(1, "PS", "Crash Bash.PBP", UnitKind::Image, 9);
    let mut cp = cand(1, &pbp);
    cp.relation = Relation::Format;
    let cands = [cand(0, &cue), cp];

    let anbernic = ContainerOrder::resolve(
        &s.container_order,
        "anbernic-stock",
        "ps",
        ContainerClass::Disc,
    );
    let (k, reason, _) = pick(&cands, &all_rules(), &anbernic);
    assert_eq!(k, 1);
    assert_eq!(
        reason,
        KeepReason::Container {
            kept: "pbp".into(),
            other: "cue".into(),
            profile: "anbernic-stock".into()
        }
    );

    let esde = ContainerOrder::resolve(&s.container_order, "esde", "ps", ContainerClass::Disc);
    let (k, reason, _) = pick(&cands, &all_rules(), &esde);
    assert_eq!(k, 0);
    assert_eq!(
        reason,
        KeepReason::Container {
            kept: "cue".into(),
            other: "pbp".into(),
            profile: "esde".into()
        }
    );
}

#[test]
fn arcade_container_order_is_zip_only() {
    let s = DedupeSettings::default();
    let order = ContainerOrder::resolve(&s.container_order, "esde", "mame", ContainerClass::Arcade);
    assert_eq!(order.order, vec!["zip".to_string()]);
    let seven = unit(0, "MAME", "ffight.7z", UnitKind::Single, 1);
    let zip = unit(1, "MAME", "ffight.zip", UnitKind::Arcade, 9);
    let (k, reason, _) = pick(&[cand(0, &seven), cand(1, &zip)], &all_rules(), &order);
    assert_eq!(k, 1);
    assert_eq!(
        reason,
        KeepReason::Container {
            kept: "zip".into(),
            other: "7z".into(),
            profile: "esde".into()
        }
    );
    // Cartridges have no order by default: zip and raw are the same.
    let cart = ContainerOrder::resolve(&s.container_order, "esde", "gb", ContainerClass::Cart);
    assert!(cart.order.is_empty());
    let raw = unit(0, "GB", "Tetris.gb", UnitKind::Single, 1);
    let zipped = unit(1, "GB", "Tetris.zip", UnitKind::Zip, 9);
    assert_eq!(container_name(&raw), "gb");
    assert_eq!(container_name(&zipped), "zip");
    let (k, reason, _) = pick(&[cand(0, &raw), cand(1, &zipped)], &all_rules(), &cart);
    assert_eq!((k, reason), (0, KeepReason::Tie));
    // "zip > raw" when the user asks for it.
    let space = ContainerOrder::new("esde", &["zip", "raw"]);
    let (k, reason, _) = pick(&[cand(0, &raw), cand(1, &zipped)], &all_rules(), &space);
    assert_eq!(k, 1);
    assert_eq!(
        reason,
        KeepReason::Container {
            kept: "zip".into(),
            other: "raw".into(),
            profile: "esde".into()
        }
    );
}

#[test]
fn rule_order_changes_the_winner_and_disabled_rule_is_skipped() {
    let cue = unit(0, "PS", "Crash Bash.cue", UnitKind::DiscSet, 1);
    let pbp = unit(1, "PS", "Crash Bash.pbp", UnitKind::Image, 9);
    let order = ContainerOrder::new("anbernic-stock", &["pbp", "cue"]);
    let mut cc = cand(0, &cue);
    cc.has_cover = true;
    let mut cp = cand(1, &pbp);
    cp.relation = Relation::Format;
    let cands = [cc, cp];

    // Default: container before cover.
    assert_eq!(pick(&cands, &all_rules(), &order).0, 1);

    // Cover moved above container.
    let mut rules = all_rules();
    let cover = rules
        .iter()
        .position(|t| t.rule == KeepRule::Cover)
        .unwrap();
    let t = rules.remove(cover);
    let container = rules
        .iter()
        .position(|t| t.rule == KeepRule::Container)
        .unwrap();
    rules.insert(container, t);
    let (k, reason, _) = pick(&cands, &rules, &order);
    assert_eq!((k, reason), (0, KeepReason::HasCover));

    // Container turned off.
    let rules: Vec<RuleToggle> = all_rules()
        .into_iter()
        .map(|t| RuleToggle {
            on: t.rule != KeepRule::Container,
            ..t
        })
        .collect();
    let (k, reason, _) = pick(&cands, &rules, &order);
    assert_eq!((k, reason), (0, KeepReason::HasCover));
}

#[test]
fn user_choice_and_forced_keeper_win() {
    let a = unit(0, "GB", "Tetris.gb", UnitKind::Single, 1);
    let b = unit(1, "GB", "Tetris (1).gb", UnitKind::Single, 9);
    let saves = [save("GB/Tetris.srm", 3)];
    let mut ca = cand(0, &a);
    ca.saves = &saves;
    let cands = [ca, cand(1, &b)];
    let regions = default_regions();

    let (k, reason, runner, tie) =
        choose(&cands, &all_rules(), &regions, &no_order(), Some(1), None);
    assert_eq!((k, reason, tie), (1, KeepReason::UserChoice, false));
    assert_eq!(runner.map(|r| r.0), Some(0));

    // A forced keeper beats even the user's pick.
    let (k, reason, _, _) = choose(
        &cands,
        &all_rules(),
        &regions,
        &no_order(),
        Some(1),
        Some(0),
    );
    assert_eq!((k, reason), (0, KeepReason::UserChoice));

    // A pick that is not a member is ignored.
    let (k, reason, _, _) = choose(&cands, &all_rules(), &regions, &no_order(), Some(7), None);
    assert_eq!(k, 0);
    assert!(matches!(reason, KeepReason::HasSave { .. }));
}

#[test]
fn remembered_pick_drops_when_the_other_copy_gains_a_save() {
    let a = unit(0, "GB", "Tetris.gb", UnitKind::Single, 1);
    let b = unit(1, "GB", "Tetris (1).gb", UnitKind::Single, 9);
    // The pick was made when no copy had a save.
    let then = [cand(0, &a), cand(1, &b)];
    let fp = saves_fingerprint(&then);
    assert!(remembered_pick_holds(&then, 1, &fp));
    assert!(!remembered_pick_holds(&then, 5, &fp), "not a member");

    // Now the copy that was not picked has a save.
    let saves = [save("GB/Tetris.srm", 3)];
    let mut ca = cand(0, &a);
    ca.saves = &saves;
    let now = [ca, cand(1, &b)];
    assert_ne!(saves_fingerprint(&now), fp);
    assert!(!remembered_pick_holds(&now, 1, &fp));
    let user = remembered_pick_holds(&now, 1, &fp).then_some(1);
    let (k, reason, _, _) = choose(
        &now,
        &all_rules(),
        &default_regions(),
        &no_order(),
        user,
        None,
    );
    assert_eq!(k, 0);
    assert!(matches!(reason, KeepReason::HasSave { .. }));

    // A save the picked copy already had, unchanged, keeps the pick.
    let saves = [save("GB/Tetris (1).srm", 4)];
    let mut cb = cand(1, &b);
    cb.saves = &saves;
    let with = [cand(0, &a), cb];
    let fp = saves_fingerprint(&with);
    assert!(remembered_pick_holds(&with, 1, &fp));
    // ... but not once that save changed.
    let changed = [save("GB/Tetris (1).srm", 5)];
    let mut cb = cand(1, &b);
    cb.saves = &changed;
    assert!(!remembered_pick_holds(&[cand(0, &a), cb], 1, &fp));
}

#[test]
fn keeper_is_never_the_shortest_name_by_default() {
    // Identical zips; the "(1)" copy is older, so it stays, as a tie.
    let short = unit(0, "SFC", "Tetris 2.zip", UnitKind::Zip, 9);
    let long = unit(1, "SFC", "Tetris 2 (1).zip", UnitKind::Zip, 1);
    let (k, reason, tie) = pick(
        &[cand(0, &short), cand(1, &long)],
        &all_rules(),
        &no_order(),
    );
    assert_eq!((k, reason, tie), (1, KeepReason::Tie, true));
}

#[test]
fn runner_up_is_the_closest_copy_and_its_rule() {
    let a = unit(0, "GB", "Tetris.gb", UnitKind::Single, 1);
    let b = unit(1, "GB", "Tetris (1).gb", UnitKind::Single, 2);
    let c = unit(2, "GB", "Tetris (2).gb", UnitKind::Single, 3);
    let saves = [save("GB/Tetris (2).srm", 1)];
    let mut cc = cand(2, &c);
    cc.saves = &saves;
    cc.has_cover = true;
    let mut cb = cand(1, &b);
    cb.has_cover = true;
    let (k, reason, runner, tie) = choose(
        &[cand(0, &a), cb, cc],
        &all_rules(),
        &default_regions(),
        &no_order(),
        None,
        None,
    );
    assert_eq!(k, 2);
    assert!(matches!(reason, KeepReason::HasSave { .. }));
    // Copy 1 beats copy 0 on the cover: it is the closest.
    assert_eq!(
        runner,
        Some((
            1,
            KeepReason::HasSave {
                path: PathBuf::from("GB/Tetris (2).srm")
            }
        ))
    );
    assert!(!tie);
}

#[test]
fn names_never_choose_outside_the_catalogue_rule() {
    // Identical copies, no catalogue: the file names alone never decide
    // (only the tie's path order, after the modified time).
    let a = unit(0, "GB", "Tetris (USA) [!].gb", UnitKind::Single, 9);
    let b = unit(1, "GB", "zzz.gb", UnitKind::Single, 1);
    let (k, reason, _) = pick(&[cand(0, &a), cand(1, &b)], &all_rules(), &no_order());
    assert_eq!((k, reason), (1, KeepReason::Tie));
}

// --- Game list ---

fn system_dir(xml: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let sys = dir.path().join("GB");
    fs::create_dir_all(&sys).unwrap();
    fs::write(sys.join("gamelist.xml"), xml).unwrap();
    (dir, sys)
}

#[test]
fn played_comes_from_the_game_list() {
    let (_d, sys) = system_dir(
        r#"<?xml version="1.0"?>
<gameList>
  <game><path>./Tetris.gb</path><name>Tetris</name><playcount>4</playcount>
    <lastplayed>20260102T101010</lastplayed></game>
  <game><path>./Sub/Mario &amp; Yoshi.gb</path><favorite>true</favorite></game>
  <game><path>./Never.gb</path><name>Never</name></game>
</gameList>"#,
    );
    assert_eq!(
        played_from_gamelist(&sys, Path::new("Tetris.gb")),
        Some(Played {
            playcount: 4,
            lastplayed: Some("20260102T101010".into()),
            favorite: false
        })
    );
    assert_eq!(
        played_from_gamelist(&sys, Path::new("Sub/Mario & Yoshi.gb")),
        Some(Played {
            playcount: 0,
            lastplayed: None,
            favorite: true
        })
    );
    // Case-insensitive, as ES reads it.
    assert!(played_from_gamelist(&sys, Path::new("tetris.GB")).is_some());
    assert_eq!(played_from_gamelist(&sys, Path::new("Never.gb")), None);
    assert_eq!(played_from_gamelist(&sys, Path::new("Missing.gb")), None);
    assert_eq!(
        played_from_gamelist(&sys.join("nope"), Path::new("Tetris.gb")),
        None
    );
}

#[test]
fn hostile_game_list_is_no_evidence() {
    let (_d, sys) = system_dir("<!DOCTYPE x [<!ENTITY a \"b\">]><gameList><game><path>./T.gb</path><playcount>1</playcount></game></gameList>");
    assert_eq!(played_from_gamelist(&sys, Path::new("T.gb")), None);
    let (_d, sys) = system_dir(
        "<gameList><game><path>./T.gb</path><playcount>lots</playcount></game></gameList>",
    );
    assert_eq!(played_from_gamelist(&sys, Path::new("T.gb")), None);
}

#[test]
fn region_order_skips_the_rom_entry_and_unknown_words() {
    assert_eq!(
        region_order(&["rom", "World", "usa", "mars", "japan"].map(String::from)),
        vec![Region::World, Region::Usa, Region::Japan]
    );
}

#[test]
fn file_name_tags_never_choose_between_proven_copies() {
    // Same bytes under two names: region and dump tags read from the file
    // names are names, so only the tie decides (spec §1, §6.1 `region` is
    // level 2 only).
    let a = unit(0, "GB", "Tetris (J) [b1].gb", UnitKind::Single, 1);
    let b = unit(1, "GB", "Tetris (U) [!].gb", UnitKind::Single, 9);
    let jp = parse_tags("Tetris (J) [b1]");
    let us = parse_tags("Tetris (U) [!]");
    for rel in [Relation::Exact, Relation::Format] {
        let mut ca = cand(0, &a);
        ca.tags = Some(&jp);
        let mut cb = cand(1, &b);
        cb.tags = Some(&us);
        cb.relation = rel;
        let (k, reason, tie) = pick(&[ca, cb], &all_rules(), &no_order());
        assert_eq!((k, reason, tie), (0, KeepReason::Tie, true), "{rel:?}");
    }
}

#[test]
fn saves_fingerprint_survives_a_new_copy_joining() {
    // The pick is remembered by content (spec §9.3): member ids may be
    // renumbered when another copy joins the group.
    let a = unit(0, "GB", "Tetris.gb", UnitKind::Single, 1);
    let b = unit(1, "GB", "Tetris (1).gb", UnitKind::Single, 9);
    let n = unit(2, "GB", "Tetris (2).gb", UnitKind::Single, 5);
    let saves = [save("GB/Tetris (1).srm", 4)];
    let mut cb = cand(1, &b);
    cb.saves = &saves;
    let fp = saves_fingerprint(&[cand(0, &a), cb]);
    let mut cb = cand(2, &b);
    cb.saves = &saves;
    let now = [cand(0, &n), cand(1, &a), cb];
    assert_eq!(saves_fingerprint(&now), fp);
    assert!(remembered_pick_holds(&now, 2, &fp));
}
