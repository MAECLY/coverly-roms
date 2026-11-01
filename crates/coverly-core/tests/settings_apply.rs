//! Settings reach the scan, the plan and the art output.

mod common;

use std::collections::BTreeMap;
use std::path::Path;

use coverly_core::credentials::DevCredentials;
use coverly_core::gather::{GatherItem, GatherTargetSummary, Gathered};
use coverly_core::identify::names::{languages, parse_name, Region};
use coverly_core::identify::{IdMethod, Identity, MatchMethod};
use coverly_core::image::{process_image, Fit, ImageLimits, OutputSpec};
use coverly_core::matching::StrategyKind;
use coverly_core::net::testing::png_bytes;
use coverly_core::plan::{build_plan, PlanAction, PlanOptions};
use coverly_core::profiles::{ImageFormat, Media};
use coverly_core::scan::scan_library;
use coverly_core::settings::{
    available_sources, available_sources_with, MaxSize, OutputFormat, OverwriteSetting, Settings,
};
use coverly_core::sources::{Candidate, MatchKind, Votes};
use coverly_core::systems::SystemDb;
use coverly_core::write::OverwritePolicy;

#[test]
fn scan_options_follow_the_library_group() {
    let lib = tempfile::tempdir().unwrap();
    let r = lib.path();
    common::mkdir(r, "anbernic");
    common::write(r, "ROMS/GBA/Keep.gba", "a");
    common::write(r, "ROMS/GBA/Junk/Skip.gba", "b");
    common::write(r, "ROMS/GBA/.hidden.gba", "c");
    common::write(r, "ROMS/GBA/Odd.zzz", "d");
    let mut s = Settings::default();
    s.library.set_ignored_dirs(vec!["junk".into()]);
    s.library.set_ignored_exts(vec!["gba".into()]);
    s.library.include_hidden = true;
    s.library.follow_symlinks = true;
    let opts = s.scan_options();
    assert!(opts.include_hidden && opts.follow_symlinks);
    assert_eq!(opts.ignored_dirs, ["junk"]);
    assert_eq!(opts.ignored_exts, ["gba"]);

    let mut s = Settings::default();
    s.library.set_ignored_dirs(vec!["junk".into()]);
    let scan = scan_library(
        r,
        &common::profile("anbernic-stock"),
        &SystemDb::builtin(),
        &s.scan_options(),
    );
    let stems: Vec<String> = scan
        .systems
        .iter()
        .flat_map(|sys| sys.roms.iter().map(|rom| rom.stem.clone()))
        .collect();
    assert_eq!(
        stems,
        ["Keep"],
        "Junk/ is ignored, hidden files are not scanned"
    );
}

#[test]
fn fill_settings_map_the_matching_group() {
    let mut s = Settings::default();
    s.matching.strategy = StrategyKind::Priority;
    s.matching.regions = vec!["japan".into(), "rom".into()];
    s.matching.languages = vec!["es".into()];
    s.matching.auto_threshold = 0.95;
    s.matching.review_threshold = 0.5;
    s.matching.fuzzy = false;
    s.matching.arcade_translation = false;
    s.matching.match_method = MatchMethod::NameOnly;
    s.matching.max_hash_mb = 8;
    s.art.media = vec![Media::Cover, Media::Title];
    s.network.index_refresh_hours = 6;
    let f = s.fill_settings();
    assert_eq!(f.strategy, StrategyKind::Priority);
    assert_eq!(f.regions.len(), 2);
    assert_eq!(f.languages, ["es"]);
    assert_eq!((f.auto_threshold, f.review_threshold), (0.95, 0.5));
    assert!(!f.fuzzy && !f.arcade_translation);
    assert_eq!(f.match_method, MatchMethod::NameOnly);
    assert_eq!(f.hash_max_bytes, 8 * 1024 * 1024);
    assert_eq!(f.media, [Media::Cover, Media::Title]);
    assert_eq!(f.sources, ["libretro", "embedded"]);
    assert_eq!(f.index_ttl_secs, 6 * 3600);
}

#[test]
fn overwrite_ask_is_never_inside_a_job() {
    let mut s = Settings::default();
    for (setting, policy) in [
        (OverwriteSetting::Never, OverwritePolicy::Never),
        (OverwriteSetting::Ask, OverwritePolicy::Never),
        (OverwriteSetting::Backup, OverwritePolicy::Backup),
    ] {
        s.art.overwrite = setting;
        assert_eq!(s.overwrite_policy(), policy);
    }
}

#[test]
fn languages_are_read_from_the_no_intro_group() {
    assert_eq!(
        languages("Final Fantasy IV Advance (Europe) (En,Fr,De,Es,It)"),
        ["en", "fr", "de", "es", "it"]
    );
    assert_eq!(languages("Pokemon (Spain) (Es)"), ["es"]);
    assert!(languages("Zelda (USA)").is_empty());
    assert!(languages("Game (Rev 1) (Beta)").is_empty());
    assert!(
        languages("Odd (En,Martian)").is_empty(),
        "every token must be a language"
    );
}

fn cand(name: &str) -> Candidate {
    Candidate {
        source: "libretro".into(),
        media: Media::Cover,
        name: name.into(),
        url: format!("https://x/{name}.png"),
        fallback_url: None,
        regions: vec![Region::Europe],
        kind: MatchKind::TitleExact,
        score: 100,
        votes: None,
        consensus: 0,
        origin: None,
    }
}

fn one_rom_plan(root: &Path, cands: Vec<Candidate>, opts: &PlanOptions) -> Option<String> {
    let profile = common::profile("anbernic-stock");
    let system_dir = root.join("ROMS/GBA");
    let gathered = Gathered {
        items: vec![GatherItem {
            target: GatherTargetSummary {
                system_id: "gba".into(),
                folder: "GBA".into(),
                system_dir: system_dir.clone(),
                rel_dir: "".into(),
                path: system_dir.join("Final Fantasy.gba"),
                stem: "Final Fantasy".into(),
                media: vec![Media::Cover],
            },
            identity: Identity {
                canonical: None,
                method: IdMethod::Name,
                confidence: 0.0,
                parsed: parse_name("Final Fantasy", false),
                hash: None,
            },
            candidates: BTreeMap::from([(Media::Cover, cands)]),
            errors: BTreeMap::new(),
        }],
        ..Gathered::default()
    };
    let plan = build_plan(root, &profile, &SystemDb::builtin(), &gathered, opts);
    match &plan.items[0].action {
        PlanAction::Auto { chosen, .. } | PlanAction::Review { chosen, .. } => {
            Some(chosen.name.clone())
        }
        PlanAction::None { .. } => None,
    }
}

#[test]
fn the_preferred_language_breaks_a_strategy_tie() {
    let root = tempfile::tempdir().unwrap();
    let cands = || {
        vec![
            cand("Final Fantasy (Europe) (De,Fr)"),
            cand("Final Fantasy (Europe) (Es,It)"),
        ]
    };
    let none = PlanOptions::default();
    assert_eq!(
        one_rom_plan(root.path(), cands(), &none).unwrap(),
        "Final Fantasy (Europe) (De,Fr)"
    );
    let italian = PlanOptions {
        languages: vec!["it".into()],
        ..PlanOptions::default()
    };
    assert_eq!(
        one_rom_plan(root.path(), cands(), &italian).unwrap(),
        "Final Fantasy (Europe) (Es,It)"
    );
    let german_first = PlanOptions {
        languages: vec!["ja".into(), "de".into()],
        ..PlanOptions::default()
    };
    assert_eq!(
        one_rom_plan(root.path(), cands(), &german_first).unwrap(),
        "Final Fantasy (Europe) (De,Fr)"
    );
}

#[test]
fn language_never_beats_a_stronger_match() {
    let root = tempfile::tempdir().unwrap();
    let mut weaker = cand("Final Fantasy (Europe) (Es,It)");
    weaker.kind = MatchKind::Fuzzy;
    weaker.score = 90;
    let cands = vec![cand("Final Fantasy (Europe) (De,Fr)"), weaker];
    let italian = PlanOptions {
        languages: vec!["it".into()],
        ..PlanOptions::default()
    };
    assert_eq!(
        one_rom_plan(root.path(), cands, &italian).unwrap(),
        "Final Fantasy (Europe) (De,Fr)"
    );
}

#[test]
fn stretch_fills_the_box_exactly_and_contain_keeps_the_aspect() {
    let src = png_bytes(100, 50);
    let spec = |fit| OutputSpec {
        format: ImageFormat::Png,
        max_size: Some((40, 40)),
        fit,
        jpeg_quality: 90,
    };
    let img = process_image(&src, &spec(Fit::Stretch), &ImageLimits::default()).unwrap();
    assert_eq!((img.width, img.height), (40, 40));
    let img = process_image(&src, &spec(Fit::Contain), &ImageLimits::default()).unwrap();
    assert_eq!((img.width, img.height), (40, 20));
    let mut s = Settings::default();
    assert_eq!(s.fit(), Fit::Contain);
    s.art.keep_aspect = false;
    assert_eq!(s.fit(), Fit::Stretch);
}

#[test]
fn art_profile_moves_the_single_slot_and_overrides_format_and_size() {
    let base = common::profile("anbernic-stock");
    let mut s = Settings::default();
    s.art.single_slot_media = Media::Screenshot;
    s.art.output_format = OutputFormat::Jpg;
    s.art.max_size.insert(
        "anbernic-stock".into(),
        MaxSize {
            width: 320,
            height: 240,
        },
    );
    let p = s.art_profile(&base);
    assert!(p.slot(Media::Cover).is_none());
    let slot = p
        .slot(Media::Screenshot)
        .expect("the one slot holds screenshots");
    assert_eq!(slot.format, ImageFormat::Jpg);
    assert!(slot.template.ends_with("{stem}.jpg"), "{}", slot.template);
    assert_eq!(slot.max_size, Some((320, 240)));
    // Idempotent: applying again changes nothing.
    assert_eq!(s.art_profile(&p), p);
    // Defaults leave a profile as it is.
    assert_eq!(Settings::default().art_profile(&base), base);
}

#[test]
fn multi_slot_layouts_keep_their_slots_and_retroarch_stays_png() {
    let mut s = Settings::default();
    s.art.single_slot_media = Media::Title;
    s.art.output_format = OutputFormat::Jpg;
    let esde = s.art_profile(&common::profile("esde"));
    assert_eq!(esde.slots.len(), 3);
    assert!(esde.slot(Media::Cover).unwrap().template.ends_with(".jpg"));
    let ra = s.art_profile(&common::profile("retroarch"));
    assert!(ra.fixed_format);
    assert_eq!(ra.slot(Media::Cover).unwrap().format, ImageFormat::Png);
    assert!(ra.slot(Media::Cover).unwrap().template.ends_with(".png"));
}

fn one_rom_action(root: &Path, cands: Vec<Candidate>, opts: &PlanOptions) -> PlanAction {
    let profile = common::profile("anbernic-stock");
    let system_dir = root.join("ROMS/GBA");
    let gathered = Gathered {
        items: vec![GatherItem {
            target: GatherTargetSummary {
                system_id: "gba".into(),
                folder: "GBA".into(),
                system_dir: system_dir.clone(),
                rel_dir: "".into(),
                path: system_dir.join("Final Fantasy.gba"),
                stem: "Final Fantasy".into(),
                media: vec![Media::Cover],
            },
            identity: Identity {
                canonical: None,
                method: IdMethod::Name,
                confidence: 0.0,
                parsed: parse_name("Final Fantasy", false),
                hash: None,
            },
            candidates: BTreeMap::from([(Media::Cover, cands)]),
            errors: BTreeMap::new(),
        }],
        ..Gathered::default()
    };
    let plan = build_plan(root, &profile, &SystemDb::builtin(), &gathered, opts);
    plan.items[0].action.clone()
}

#[test]
fn popularity_consensus_beats_the_preferred_language() {
    let root = tempfile::tempdir().unwrap();
    let mut popular = cand("Final Fantasy (Europe) (De,Fr)");
    popular.consensus = 3;
    let mut rare = cand("Final Fantasy (Europe) (Es,It)");
    rare.consensus = 1;
    let opts = PlanOptions {
        strategy: StrategyKind::Popularity,
        languages: vec!["it".into()],
        ..PlanOptions::default()
    };
    assert_eq!(
        one_rom_plan(root.path(), vec![rare, popular], &opts).unwrap(),
        "Final Fantasy (Europe) (De,Fr)"
    );
}

#[test]
fn popularity_votes_beat_the_preferred_language() {
    let root = tempfile::tempdir().unwrap();
    let mut popular = cand("Final Fantasy (Europe) (De,Fr)");
    popular.votes = Some(Votes { up: 10, down: 0 });
    let mut rare = cand("Final Fantasy (Europe) (Es,It)");
    rare.votes = Some(Votes { up: 2, down: 0 });
    let opts = PlanOptions {
        strategy: StrategyKind::Popularity,
        languages: vec!["it".into()],
        ..PlanOptions::default()
    };
    assert_eq!(
        one_rom_plan(root.path(), vec![rare, popular], &opts).unwrap(),
        "Final Fantasy (Europe) (De,Fr)"
    );
}

#[test]
fn source_priority_beats_the_preferred_language() {
    let root = tempfile::tempdir().unwrap();
    let first = cand("Final Fantasy (Europe) (De,Fr)");
    let mut second = cand("Final Fantasy (Europe) (Es,It)");
    second.source = "other".into();
    for strategy in [StrategyKind::Priority, StrategyKind::RomRegion] {
        let opts = PlanOptions {
            strategy,
            sources: vec!["libretro".into(), "other".into()],
            languages: vec!["it".into()],
            ..PlanOptions::default()
        };
        assert_eq!(
            one_rom_plan(root.path(), vec![second.clone(), first.clone()], &opts).unwrap(),
            "Final Fantasy (Europe) (De,Fr)",
            "{strategy:?}"
        );
    }
}

#[test]
fn a_release_stays_auto_over_a_beta_in_the_preferred_language() {
    let root = tempfile::tempdir().unwrap();
    let cands = vec![
        cand("Final Fantasy (Europe) (En,Fr) (Beta)"),
        cand("Final Fantasy (Europe)"),
    ];
    let opts = PlanOptions {
        languages: vec!["en".into()],
        ..PlanOptions::default()
    };
    match one_rom_action(root.path(), cands, &opts) {
        PlanAction::Auto { chosen, .. } => assert_eq!(chosen.name, "Final Fantasy (Europe)"),
        other => panic!("expected auto, got {other:?}"),
    }
}

#[test]
fn an_uppercase_image_extension_follows_the_output_format() {
    let mut base = common::profile("anbernic-stock");
    for slot in base.slots.values_mut() {
        slot.template = slot.template.replace(".png", ".PNG");
    }
    let mut s = Settings::default();
    s.art.output_format = OutputFormat::Jpg;
    let p = s.art_profile(&base);
    for slot in p.slots.values() {
        assert_eq!(slot.format, ImageFormat::Jpg);
        assert!(slot.template.ends_with(".jpg"), "{}", slot.template);
        assert!(!slot.template.contains(".PNG"), "{}", slot.template);
    }
    let mut jpeg = common::profile("anbernic-stock");
    for slot in jpeg.slots.values_mut() {
        slot.template = slot.template.replace(".png", ".JPEG");
        slot.format = ImageFormat::Jpg;
    }
    s.art.output_format = OutputFormat::Png;
    for slot in s.art_profile(&jpeg).slots.values() {
        assert!(slot.template.ends_with(".png"), "{}", slot.template);
    }
}

#[test]
fn steamgriddb_can_now_be_enabled_and_survives_normalise() {
    let mut s = Settings::default();
    assert_eq!(
        s.sources
            .order
            .iter()
            .filter(|e| e.enabled)
            .map(|e| e.id.as_str())
            .collect::<Vec<_>>(),
        ["libretro", "embedded"]
    );
    s.sources.order.iter_mut().for_each(|e| e.enabled = true);
    s.normalise();
    let on: Vec<&str> = s
        .sources
        .order
        .iter()
        .filter(|e| e.enabled)
        .map(|e| e.id.as_str())
        .collect();
    assert!(on.contains(&"steamgriddb"));
    assert_eq!(
        available_sources_with(false),
        ["libretro", "steamgriddb", "embedded"]
    );
    assert_eq!(
        available_sources_with(true),
        ["libretro", "steamgriddb", "screenscraper", "embedded"]
    );
}

#[test]
fn normalise_turns_screenscraper_off_in_a_build_without_dev_credentials() {
    if DevCredentials::from_build().is_some() {
        eprintln!("skipped: this build carries ScreenScraper dev credentials");
        return;
    }
    let mut s = Settings::default();
    s.sources.order.iter_mut().for_each(|e| e.enabled = true);
    s.normalise();
    assert!(
        !s.sources
            .order
            .iter()
            .find(|e| e.id == "screenscraper")
            .unwrap()
            .enabled
    );
    assert!(!available_sources().contains(&"screenscraper"));
}

#[test]
fn organize_options_follow_the_organize_group() {
    use coverly_core::organize::{NameMode, OrganizeOptions, TransferMode};
    use coverly_core::settings::OrganizeModeSetting;

    let defaults = OrganizeOptions::from_settings(&Settings::default(), None);
    assert_eq!(defaults.layout, "anbernic-stock");
    assert!(!defaults.flat);
    assert_eq!(defaults.mode, None);
    assert_eq!(defaults.thresholds.auto, 0.85);
    assert_eq!(defaults.recent_secs, 60);

    let mut s = Settings::default();
    s.organize.default_layout = "esde".into();
    s.organize.nest_in_roms_root = false;
    s.organize.mode = OrganizeModeSetting::Copy;
    s.organize.names = NameMode::Catalogue;
    s.organize.download_dats = false;
    s.organize.auto_threshold = 0.9;
    s.organize.review_threshold = 0.6;
    s.organize.companion_exts = vec!["sav".into()];
    s.organize.recent_seconds = 5;
    s.organize.folder_names.insert(
        "anbernic-stock".into(),
        BTreeMap::from([("gba".to_string(), "gba".to_string())]),
    );
    s.organize
        .arcade_order
        .insert("anbernic-stock".into(), vec!["mame".into()]);
    let o = OrganizeOptions::from_settings(&s, None);
    assert_eq!(o.layout, "esde");
    assert!(o.flat);
    assert_eq!(o.mode, Some(TransferMode::Copy));
    assert_eq!(o.names, NameMode::Catalogue);
    assert!(!o.download_dats);
    assert_eq!((o.thresholds.auto, o.thresholds.review), (0.9, 0.6));
    assert_eq!(o.companion_exts, ["sav"]);
    assert_eq!(o.recent_secs, 5);
    // Overrides are per layout: es-de has none.
    assert!(o.folder_names.is_empty());
    assert_eq!(o.arcade_order, None);

    let o = OrganizeOptions::from_settings(&s, Some("anbernic-stock"));
    assert_eq!(o.layout, "anbernic-stock");
    assert_eq!(o.folder_names.get("gba").map(String::as_str), Some("gba"));
    assert_eq!(o.arcade_order, Some(vec!["mame".to_string()]));

    s.organize.mode = OrganizeModeSetting::Move;
    assert_eq!(
        OrganizeOptions::from_settings(&s, None).mode,
        Some(TransferMode::Move)
    );
}

#[test]
fn switch_icons_and_the_fallback_only_count_while_the_catalogue_is_on() {
    let mut s = Settings::default();
    let c = &s.catalogs.switch;
    assert!(!c.enabled && !c.icons_on() && !c.fallback_on());
    // Set by hand (not through a patch, which would normalise them off).
    s.catalogs.switch.icons = true;
    s.catalogs.switch.fallback = true;
    assert!(!s.catalogs.switch.icons_on() && !s.catalogs.switch.fallback_on());
    s.catalogs.switch.enabled = true;
    assert!(s.catalogs.switch.icons_on() && s.catalogs.switch.fallback_on());
    assert_eq!(
        s.catalogs.switch.refresh_secs(),
        7 * 24 * 60 * 60,
        "refresh_days in seconds"
    );
}
