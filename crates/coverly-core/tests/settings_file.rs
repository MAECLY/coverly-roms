//! Spec §9 settings: defaults, persistence, hostile files, validation, patches.

use std::fs;
use std::path::{Path, PathBuf};

use coverly_core::identify::MatchMethod;
use coverly_core::matching::StrategyKind;
use coverly_core::profiles::{builtin_profiles, Media};
use coverly_core::settings::*;
use serde_json::json;

fn known() -> Vec<String> {
    builtin_profiles().into_iter().map(|p| p.id).collect()
}

fn files(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn write(dir: &Path, text: &str) -> PathBuf {
    let path = dir.join(SETTINGS_FILE);
    fs::write(&path, text).unwrap();
    path
}

#[test]
fn defaults_follow_the_spec() {
    let s = Settings::default();
    assert_eq!(s.version, SETTINGS_VERSION);
    assert_eq!(s.general.language, LanguageChoice::Auto);
    assert_eq!(s.general.theme, ThemeChoice::System);
    assert!(s.general.check_updates_on_launch);
    assert_eq!(s.general.update_channel, UpdateChannel::Stable);
    assert_eq!(s.library.default_profile, None);
    assert_eq!(
        s.library.ignored_dirs,
        ["bios", "saves", "states", "Shortcuts"]
    );
    assert!(s.library.ignored_exts.contains(&"txt".to_string()));
    assert!(!s.library.include_hidden && !s.library.follow_symlinks);
    assert_eq!(s.art.media, Media::ALL);
    assert_eq!(s.art.single_slot_media, Media::Cover);
    assert_eq!(s.art.output_format, OutputFormat::Profile);
    assert!(s.art.keep_aspect);
    assert_eq!(s.art.jpeg_quality, 90);
    assert_eq!(s.art.overwrite, OverwriteSetting::Never);
    assert_eq!(s.matching.strategy, StrategyKind::RomRegion);
    assert_eq!(
        s.matching.regions,
        ["rom", "world", "usa", "europe", "japan"]
    );
    assert_eq!(s.matching.languages, ["en"]);
    assert_eq!(s.matching.auto_threshold, 0.90);
    assert_eq!(s.matching.review_threshold, 0.65);
    assert!(s.matching.fuzzy && s.matching.arcade_translation);
    assert_eq!(s.matching.match_method, MatchMethod::HashThenName);
    assert_eq!(s.matching.max_hash_mb, 64);
    let sources: Vec<(&str, bool)> = s
        .sources
        .order
        .iter()
        .map(|e| (e.id.as_str(), e.enabled))
        .collect();
    assert_eq!(
        sources,
        [
            ("libretro", true),
            ("steamgriddb", false),
            ("screenscraper", false),
            ("embedded", true)
        ]
    );
    assert_eq!(
        (
            s.network.concurrency,
            s.network.timeout_secs,
            s.network.retries
        ),
        (4, 30, 3)
    );
    assert_eq!(s.network.proxy, ProxyChoice::System);
    assert_eq!(s.network.cache_max_mb, 2048);
    assert_eq!(s.network.index_refresh_hours, 48);
    assert!(s.safety.backup_before_overwrite && s.safety.dry_run_default);
    assert!(s.safety.clean_apple_double);
    assert_eq!(s.safety.backup_dir, None);
    assert_eq!(s.safety.undo_depth, 10);
    assert_eq!(s.advanced.log_level, LogLevel::Info);
    assert!(s.validate(&known()).is_empty());
}

#[test]
fn missing_file_gives_defaults_and_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (s, report) = load_settings(dir.path(), &known());
    assert_eq!(s, Settings::default());
    assert_eq!(report, LoadReport::default());
    assert!(files(dir.path()).is_empty());
}

#[test]
fn save_then_load_round_trips_without_temp_files() {
    let dir = tempfile::tempdir().unwrap();
    let nested = dir.path().join("a/b");
    let mut s = Settings::default();
    s.general.theme = ThemeChoice::Dark;
    s.network.timeout_secs = 90;
    s.art.max_size.insert(
        "onion".into(),
        MaxSize {
            width: 300,
            height: 400,
        },
    );
    save_settings(&nested, &s).unwrap();
    assert_eq!(files(&nested), [SETTINGS_FILE]);
    let (back, report) = load_settings(&nested, &known());
    assert_eq!(back, s);
    assert_eq!(report.notice, None);
}

#[test]
fn garbage_is_set_aside_and_defaults_are_used() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "{ not json");
    let (s, report) = load_settings(dir.path(), &known());
    assert_eq!(s, Settings::default());
    assert_eq!(report.notice, Some(LoadNotice::ResetCorrupt));
    let backup = report.backup.expect("the bad file is kept");
    assert_eq!(fs::read_to_string(&backup).unwrap(), "{ not json");
    assert!(backup
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with("settings.json.bad-"));
    assert!(!dir.path().join(SETTINGS_FILE).exists());
}

#[test]
fn a_json_array_is_corrupt_too() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "[1,2,3]");
    let (s, report) = load_settings(dir.path(), &known());
    assert_eq!(s, Settings::default());
    assert_eq!(report.notice, Some(LoadNotice::ResetCorrupt));
}

#[test]
fn a_wrong_type_resets_only_that_field_and_keeps_a_backup() {
    let dir = tempfile::tempdir().unwrap();
    let text =
        r#"{"version":2,"network":{"timeout_secs":"soon","retries":5},"general":{"theme":"dark"}}"#;
    write(dir.path(), text);
    let (s, report) = load_settings(dir.path(), &known());
    assert_eq!(s.network.timeout_secs, 30);
    assert_eq!(s.network.retries, 5);
    assert_eq!(s.general.theme, ThemeChoice::Dark);
    assert_eq!(report.notice, Some(LoadNotice::Fixed));
    assert_eq!(report.fixed, ["network.timeout_secs"]);
    let backup = report.backup.expect("a copy of what was read");
    assert_eq!(fs::read_to_string(backup).unwrap(), text);
    // The file itself is untouched until the user saves.
    assert_eq!(
        fs::read_to_string(dir.path().join(SETTINGS_FILE)).unwrap(),
        text
    );
}

#[test]
fn out_of_range_values_fall_back_to_defaults() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        r#"{"network":{"concurrency":99},"art":{"jpeg_quality":0},"safety":{"undo_depth":3}}"#,
    );
    let (s, report) = load_settings(dir.path(), &known());
    assert_eq!(s.network.concurrency, 4);
    assert_eq!(s.art.jpeg_quality, 90);
    assert_eq!(s.safety.undo_depth, 3);
    assert_eq!(report.notice, Some(LoadNotice::Fixed));
    assert!(report.fixed.contains(&"network.concurrency".to_string()));
    assert!(report.fixed.contains(&"art.jpeg_quality".to_string()));
}

#[test]
fn thresholds_out_of_order_are_both_reset() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        r#"{"matching":{"auto_threshold":0.5,"review_threshold":0.9}}"#,
    );
    let (s, _) = load_settings(dir.path(), &known());
    assert_eq!(s.matching.auto_threshold, 0.90);
    assert_eq!(s.matching.review_threshold, 0.65);
}

#[test]
fn a_newer_file_keeps_what_it_understands() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        r#"{"version":9,"general":{"theme":"light","hologram":true},"telemetry":{"on":true}}"#,
    );
    let (s, report) = load_settings(dir.path(), &known());
    assert_eq!(s.general.theme, ThemeChoice::Light);
    assert_eq!(s.version, SETTINGS_VERSION);
    assert_eq!(report.notice, Some(LoadNotice::FutureVersion));
    assert!(report.backup.is_some());
}

#[test]
fn huge_files_are_treated_as_corrupt() {
    let dir = tempfile::tempdir().unwrap();
    let big = " ".repeat(MAX_SETTINGS_BYTES as usize + 1);
    write(dir.path(), &big);
    let (s, report) = load_settings(dir.path(), &known());
    assert_eq!(s, Settings::default());
    assert_eq!(report.notice, Some(LoadNotice::ResetCorrupt));
}

#[test]
fn an_unreadable_settings_path_is_set_aside_not_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join(SETTINGS_FILE)).unwrap();
    let (s, report) = load_settings(dir.path(), &known());
    assert_eq!(s, Settings::default());
    assert_eq!(report.notice, Some(LoadNotice::ResetCorrupt));
    assert!(report.backup.expect("moved aside").is_dir());
    save_settings(dir.path(), &s).unwrap();
    assert!(dir.path().join(SETTINGS_FILE).is_file());
}

#[test]
fn a_deleted_default_profile_falls_back_to_auto() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), r#"{"library":{"default_profile":"gone"}}"#);
    let (s, report) = load_settings(dir.path(), &known());
    assert_eq!(s.library.default_profile, None);
    assert_eq!(report.fixed, ["library.default_profile"]);
}

#[test]
fn legacy_library_json_is_migrated_and_kept() {
    let dir = tempfile::tempdir().unwrap();
    let legacy = r#"{"version":1,"ignored_dirs":["bios","Junk"],"recent":[
        {"path":"/sd","name":"sd","profile_id":"onion","roms":12,"opened_at":5},
        {"path":7}
    ]}"#;
    fs::write(dir.path().join(LEGACY_LIBRARY_FILE), legacy).unwrap();
    let (s, report) = load_settings(dir.path(), &known());
    assert_eq!(report.notice, Some(LoadNotice::Migrated));
    assert_eq!(s.library.ignored_dirs, ["bios", "Junk"]);
    assert_eq!(s.library.recent.len(), 1, "the broken recent is dropped");
    assert_eq!(s.library.recent[0].profile_id, "onion");
    assert!(dir.path().join(SETTINGS_FILE).exists(), "migrated once");
    assert_eq!(
        fs::read_to_string(dir.path().join(LEGACY_LIBRARY_FILE)).unwrap(),
        legacy,
        "the old file stays as the backup"
    );
    let (again, report) = load_settings(dir.path(), &known());
    assert_eq!(again, s);
    assert_eq!(report.notice, None);
}

#[test]
fn read_settings_has_no_side_effects() {
    let dir = tempfile::tempdir().unwrap();
    let path = write(dir.path(), "garbage");
    assert_eq!(read_settings(&path, &known()), Settings::default());
    assert_eq!(files(dir.path()), [SETTINGS_FILE]);
    assert_eq!(
        read_settings(&dir.path().join("none.json"), &known()),
        Settings::default()
    );
}

fn codes(errors: &[FieldError]) -> Vec<(String, FieldErrorCode)> {
    errors.iter().map(|e| (e.field.clone(), e.code)).collect()
}

#[test]
fn validation_reports_each_field_with_its_range() {
    let mut s = Settings::default();
    s.network.timeout_secs = 1;
    s.network.retries = 11;
    s.art.jpeg_quality = 101;
    s.matching.max_hash_mb = 0;
    s.network.index_refresh_hours = 0;
    s.safety.undo_depth = 0;
    let errors = s.validate(&known());
    let timeout = errors
        .iter()
        .find(|e| e.field == "network.timeout_secs")
        .unwrap();
    assert_eq!(timeout.code, FieldErrorCode::Range);
    assert_eq!((timeout.min, timeout.max), (Some(5.0), Some(300.0)));
    for field in [
        "network.retries",
        "art.jpeg_quality",
        "matching.max_hash_mb",
        "network.index_refresh_hours",
        "safety.undo_depth",
    ] {
        assert!(
            errors.iter().any(|e| e.field == field),
            "{field}: {errors:?}"
        );
    }
}

#[test]
fn cross_field_and_list_rules() {
    let mut s = Settings::default();
    s.matching.review_threshold = 0.95;
    s.matching.regions = vec!["rom".into(), "mars".into()];
    s.matching.languages = vec!["english".into()];
    s.art.media.clear();
    s.art.max_size.insert(
        "nope".into(),
        MaxSize {
            width: 300,
            height: 400,
        },
    );
    s.art.max_size.insert(
        "onion".into(),
        MaxSize {
            width: 8,
            height: 400,
        },
    );
    s.network.proxy = ProxyChoice::Manual;
    s.safety.backup_before_overwrite = false;
    s.safety.backup_dir = Some("relative/dir".into());
    s.advanced.custom_dir = Some("also/relative".into());
    s.library.default_profile = Some("nope".into());
    let got = codes(&s.validate(&known()));
    for want in [
        ("matching.review_threshold", FieldErrorCode::Order),
        ("matching.regions", FieldErrorCode::Invalid),
        ("matching.languages", FieldErrorCode::Invalid),
        ("art.media", FieldErrorCode::Required),
        ("art.max_size.nope", FieldErrorCode::Unknown),
        ("art.max_size.onion.width", FieldErrorCode::Range),
        ("network.proxy_url", FieldErrorCode::Required),
        ("safety.backup_before_overwrite", FieldErrorCode::Invalid),
        ("safety.backup_dir", FieldErrorCode::NotAbsolute),
        ("advanced.custom_dir", FieldErrorCode::NotAbsolute),
        ("library.default_profile", FieldErrorCode::Unknown),
    ] {
        assert!(
            got.contains(&(want.0.to_string(), want.1)),
            "missing {want:?} in {got:?}"
        );
    }
    let mut dup = Settings::default();
    dup.matching.regions = vec!["usa".into(), "USA".into()];
    assert_eq!(
        codes(&dup.validate(&known())),
        [("matching.regions".to_string(), FieldErrorCode::Duplicate)]
    );
    let mut bad_url = Settings::default();
    bad_url.network.proxy = ProxyChoice::Manual;
    bad_url.network.proxy_url = "socks5://x:1".into();
    assert_eq!(
        codes(&bad_url.validate(&known())),
        [("network.proxy_url".to_string(), FieldErrorCode::Invalid)]
    );
}

#[test]
fn normalise_cleans_lists_and_keeps_unavailable_sources_off() {
    let mut s = Settings::default();
    s.library.set_ignored_dirs(vec![
        " bios ".into(),
        "BIOS".into(),
        "a/b".into(),
        "..".into(),
        "".into(),
    ]);
    s.library.set_ignored_exts(vec![
        ".PNG".into(),
        "png".into(),
        " txt ".into(),
        "a b".into(),
        "".into(),
    ]);
    s.sources.order = vec![
        SourceEntry {
            id: "screenscraper".into(),
            enabled: true,
        },
        SourceEntry {
            id: "evil".into(),
            enabled: true,
        },
        SourceEntry {
            id: "libretro".into(),
            enabled: true,
        },
        SourceEntry {
            id: "libretro".into(),
            enabled: false,
        },
    ];
    s.art.media = vec![Media::Title, Media::Cover, Media::Title];
    s.matching.regions = vec![" USA ".into(), "rom".into()];
    s.normalise();
    assert_eq!(s.library.ignored_dirs, ["bios"]);
    assert_eq!(s.library.ignored_exts, ["png", "txt"]);
    let order: Vec<(&str, bool)> = s
        .sources
        .order
        .iter()
        .map(|e| (e.id.as_str(), e.enabled))
        .collect();
    // ScreenScraper stays on only in a build with developer credentials.
    let ss = available_sources().contains(&"screenscraper");
    assert_eq!(
        order,
        [
            ("screenscraper", ss),
            ("libretro", true),
            ("steamgriddb", false),
            ("embedded", false)
        ]
    );
    assert_eq!(s.art.media, [Media::Cover, Media::Title]);
    assert_eq!(s.matching.regions, ["usa", "rom"]);
}

#[test]
fn with_patch_applies_a_valid_change() {
    let s = Settings::default();
    let next = s
        .with_patch(
            &json!({"network": {"timeout_secs": 60}, "general": {"language": "es"}}),
            &known(),
        )
        .unwrap();
    assert_eq!(next.network.timeout_secs, 60);
    assert_eq!(next.general.language, LanguageChoice::Es);
    assert_eq!(next.network.retries, s.network.retries);
    let cleared = next
        .with_patch(&json!({"safety": {"backup_dir": null}}), &known())
        .unwrap();
    assert_eq!(cleared.safety.backup_dir, None);
}

#[test]
fn with_patch_names_the_field_of_a_type_error_and_of_a_range_error() {
    let s = Settings::default();
    let errors = s
        .with_patch(
            &json!({"network": {"timeout_secs": "x", "retries": 2}}),
            &known(),
        )
        .unwrap_err();
    assert_eq!(
        codes(&errors),
        [("network.timeout_secs".to_string(), FieldErrorCode::Invalid)]
    );
    let errors = s
        .with_patch(&json!({"network": {"concurrency": 7}}), &known())
        .unwrap_err();
    assert_eq!(
        codes(&errors),
        [("network.concurrency".to_string(), FieldErrorCode::Range)]
    );
}

#[test]
fn with_patch_refuses_a_change_that_only_enables_an_unavailable_source() {
    // Normalising turns sources this build cannot use off; a patch whose
    // only enabled source is one of them would leave none enabled.
    if available_sources().contains(&"screenscraper") {
        eprintln!("skipped: this build carries ScreenScraper dev credentials");
        return;
    }
    let errors = Settings::default()
        .with_patch(
            &json!({"sources": {"order": [
                {"id": "screenscraper", "enabled": true},
                {"id": "libretro", "enabled": false}
            ]}}),
            &known(),
        )
        .unwrap_err();
    assert_eq!(
        codes(&errors),
        [("sources.order".to_string(), FieldErrorCode::Required)]
    );
}

#[test]
fn remembers_recent_libraries_most_recent_first() {
    let mut s = Settings::default();
    for i in 0..12 {
        s.remember(RecentLibrary {
            path: format!("/lib{i}").into(),
            name: format!("lib{i}"),
            profile_id: "onion".into(),
            roms: i,
            opened_at: i64::from(i),
        });
    }
    assert_eq!(s.library.recent.len(), MAX_RECENT);
    assert_eq!(s.library.recent[0].path, PathBuf::from("/lib11"));
    s.remember(RecentLibrary {
        path: "/lib5".into(),
        name: "lib5".into(),
        profile_id: "onion".into(),
        roms: 99,
        opened_at: 99,
    });
    assert_eq!(s.library.recent[0].roms, 99);
    assert_eq!(
        s.library
            .recent
            .iter()
            .filter(|r| r.path == Path::new("/lib5"))
            .count(),
        1
    );
    s.forget(Path::new("/lib5"));
    assert!(s
        .library
        .recent
        .iter()
        .all(|r| r.path != Path::new("/lib5")));
}

#[test]
fn ranges_table_is_the_validation_source() {
    let fields: Vec<&str> = RANGES.iter().map(|r| r.field).collect();
    assert!(fields.contains(&"network.timeout_secs"));
    let json = serde_json::to_value(RANGES[0]).unwrap();
    assert!(json.get("field").is_some() && json.get("min").is_some());
}

const RECENT: &str = r#"{"path":"/sd","name":"sd","profile_id":"onion","roms":12,"opened_at":5}"#;

#[test]
fn a_manual_proxy_without_url_resets_only_the_proxy() {
    for network in [
        r#"{"proxy":"manual","timeout_secs":90}"#,
        r#"{"proxy":"manual","proxy_url":"socks5://x:1","timeout_secs":90}"#,
    ] {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            &format!(
                r#"{{"general":{{"theme":"dark"}},"network":{network},"library":{{"recent":[{RECENT}]}}}}"#
            ),
        );
        let (s, report) = load_settings(dir.path(), &known());
        assert_eq!(s.general.theme, ThemeChoice::Dark, "{network}");
        assert_eq!(s.network.timeout_secs, 90, "{network}");
        assert_eq!(s.library.recent.len(), 1, "{network}");
        assert_eq!(s.network.proxy, ProxyChoice::System, "{network}");
        assert_eq!(s.network.proxy_url, "", "{network}");
        assert!(s.validate(&known()).is_empty());
        assert_eq!(report.notice, Some(LoadNotice::Fixed));
        assert!(report.fixed.contains(&"network.proxy".to_string()));
        assert!(report.fixed.contains(&"network.proxy_url".to_string()));
    }
}

#[test]
fn loading_cleans_what_it_can_instead_of_resetting() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        r#"{"matching":{"languages":["ES"," fr"],"regions":["USA","usa","rom"]},
            "sources":{"order":[{"id":"hologram-db","enabled":true},
                                {"id":"screenscraper","enabled":true},
                                {"id":"libretro","enabled":true}]}}"#,
    );
    let (s, _) = load_settings(dir.path(), &known());
    assert_eq!(s.matching.languages, ["es", "fr"]);
    assert_eq!(s.matching.regions, ["usa", "rom"]);
    let ids: Vec<&str> = s.sources.order.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(
        &ids[..2],
        ["screenscraper", "libretro"],
        "the user's order is kept"
    );
}

#[test]
fn bad_list_and_map_entries_are_dropped_one_by_one() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        &format!(
            r#"{{"library":{{"recent":[{RECENT},{{"path":7}}]}},
                "art":{{"max_size":{{"onion":{{"width":300,"height":400}},
                                    "garlic":{{"width":1,"height":400}},
                                    "gone":{{"width":300,"height":400}},
                                    "minui":"big"}}}}}}"#
        ),
    );
    let (s, report) = load_settings(dir.path(), &known());
    assert_eq!(s.library.recent.len(), 1);
    assert_eq!(s.library.recent[0].profile_id, "onion");
    assert_eq!(
        s.art.max_size.keys().collect::<Vec<_>>(),
        ["onion"],
        "only the broken entries are dropped"
    );
    assert!(report.fixed.contains(&"library.recent".to_string()));
    assert!(report.fixed.contains(&"art.max_size".to_string()));
}

#[test]
fn concurrent_saves_never_fail_or_leave_temp_files() {
    let dir = tempfile::tempdir().unwrap();
    std::thread::scope(|scope| {
        for t in 0..8u32 {
            let dir = dir.path();
            scope.spawn(move || {
                for i in 0..50u32 {
                    let mut s = Settings::default();
                    s.network.timeout_secs = 5 + t * 30 + i % 30;
                    save_settings(dir, &s).expect("every save succeeds");
                }
            });
        }
    });
    assert_eq!(files(dir.path()), [SETTINGS_FILE]);
    let (_, report) = load_settings(dir.path(), &known());
    assert_eq!(report.notice, None);
}

#[test]
fn loading_the_same_fixed_file_again_keeps_one_backup() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), r#"{"network":{"concurrency":99}}"#);
    let (_, first) = load_settings(dir.path(), &known());
    for _ in 0..4 {
        let (_, again) = load_settings(dir.path(), &known());
        assert_eq!(again.backup, first.backup);
    }
    assert_eq!(files(dir.path()).len(), 2, "{:?}", files(dir.path()));
    // A different content gets its own copy.
    write(dir.path(), r#"{"network":{"concurrency":98}}"#);
    let (_, other) = load_settings(dir.path(), &known());
    assert_ne!(other.backup, first.backup);
    assert_eq!(files(dir.path()).len(), 3);
}

#[test]
fn a_set_aside_settings_file_is_not_replaced_by_the_legacy_one() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join(LEGACY_LIBRARY_FILE),
        r#"{"version":1,"ignored_dirs":["Old"]}"#,
    )
    .unwrap();
    write(dir.path(), "garbage");
    let (_, report) = load_settings(dir.path(), &known());
    assert_eq!(report.notice, Some(LoadNotice::ResetCorrupt));
    let (s, report) = load_settings(dir.path(), &known());
    assert_eq!(s, Settings::default());
    assert_eq!(report.notice, None, "settings existed: no migration");
}

#[test]
fn with_patch_refuses_a_patch_that_is_not_an_object() {
    let mut s = Settings::default();
    s.general.theme = ThemeChoice::Dark;
    for patch in [json!([1]), json!([]), json!(null), json!(7), json!("x")] {
        let errors = s.with_patch(&patch, &known()).unwrap_err();
        assert_eq!(
            codes(&errors),
            [(String::new(), FieldErrorCode::Invalid)],
            "{patch}"
        );
    }
    let errors = s
        .with_patch(
            &json!({"network": [], "general": {"theme": "light"}}),
            &known(),
        )
        .unwrap_err();
    assert_eq!(
        codes(&errors),
        [("network".to_string(), FieldErrorCode::Invalid)]
    );
    assert!(s.with_patch(&json!({}), &known()).is_ok());
}

#[test]
fn a_utf8_bom_is_not_corruption() {
    let dir = tempfile::tempdir().unwrap();
    let text = "\u{feff}{\"general\":{\"theme\":\"dark\"}}";
    write(dir.path(), text);
    let (s, report) = load_settings(dir.path(), &known());
    assert_eq!(s.general.theme, ThemeChoice::Dark);
    assert_eq!(report.notice, None, "a BOM is not a problem");
    assert_eq!(
        files(dir.path()),
        [SETTINGS_FILE],
        "nothing set aside or copied"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join(SETTINGS_FILE)).unwrap(),
        text
    );
    let (read, problem) = read_settings_checked(&dir.path().join(SETTINGS_FILE), &known());
    assert_eq!(read.general.theme, ThemeChoice::Dark);
    assert_eq!(problem, None);
}

#[test]
fn a_broken_legacy_file_is_set_aside_once() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join(LEGACY_LIBRARY_FILE), "garbage").unwrap();
    let (s, report) = load_settings(dir.path(), &known());
    assert_eq!(s, Settings::default());
    assert_eq!(report.notice, Some(LoadNotice::ResetCorrupt));
    let backup = report.backup.expect("the old file is kept aside");
    assert!(backup
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with("library.json.bad-"));
    assert_eq!(fs::read_to_string(&backup).unwrap(), "garbage");
    assert!(!dir.path().join(LEGACY_LIBRARY_FILE).exists());
    assert!(dir.path().join(SETTINGS_FILE).exists(), "defaults saved");
    let (_, again) = load_settings(dir.path(), &known());
    assert_eq!(again.notice, None, "no warning on the next start");
}

#[test]
fn read_settings_checked_names_the_problem_without_touching_disk() {
    let dir = tempfile::tempdir().unwrap();
    let path = write(dir.path(), "{ not json");
    assert_eq!(
        read_settings_checked(&path, &known()),
        (Settings::default(), Some(ReadProblem::NotSettings))
    );
    assert_eq!(files(dir.path()), [SETTINGS_FILE]);
    let missing = dir.path().join("none.json");
    assert_eq!(
        read_settings_checked(&missing, &known()),
        (Settings::default(), None)
    );
    let folder = dir.path().join("folder.json");
    fs::create_dir(&folder).unwrap();
    assert_eq!(
        read_settings_checked(&folder, &known()).1,
        Some(ReadProblem::Unreadable)
    );
}

#[test]
fn a_settings_file_without_organize_loads_with_defaults() {
    let s: Settings = serde_json::from_str(r#"{"version":1}"#).unwrap();
    let o = &s.organize;
    assert_eq!(o.default_layout, "anbernic-stock");
    assert!(o.nest_in_roms_root);
    assert_eq!(o.mode, OrganizeModeSetting::Auto);
    assert_eq!(o.names, coverly_core::organize::NameMode::Keep);
    assert!(o.download_dats);
    assert_eq!(o.auto_threshold, 0.85);
    assert_eq!(o.review_threshold, 0.50);
    assert!(o.companion_exts.contains(&"state*".to_string()));
    assert!(o.folder_names.is_empty());
    assert!(o.arcade_order.is_empty());
    assert_eq!(o.recent_seconds, 60);
    assert_eq!(o.keep_organize_journals_days, 90);
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), r#"{"version":2,"general":{"theme":"dark"}}"#);
    let (s, report) = load_settings(dir.path(), &known());
    assert_eq!(s.organize, OrganizeSettings::default());
    assert!(report.fixed.is_empty(), "{:?}", report.fixed);
}

#[test]
fn organize_values_are_validated() {
    let mut s = Settings::default();
    s.organize.auto_threshold = 1.5;
    s.organize.recent_seconds = 4000;
    s.organize.keep_organize_journals_days = 0;
    s.organize.default_layout = "nope".into();
    s.organize.folder_names.insert(
        "anbernic-stock".into(),
        [("gba".into(), "SFC".into())].into(),
    );
    s.organize
        .folder_names
        .insert("ghost".into(), [("gba".into(), "GBA".into())].into());
    s.organize
        .arcade_order
        .insert("ghost".into(), vec!["fbneo".into()]);
    let got = codes(&s.validate(&known()));
    for want in [
        ("organize.auto_threshold", FieldErrorCode::Range),
        ("organize.recent_seconds", FieldErrorCode::Range),
        (
            "organize.keep_organize_journals_days",
            FieldErrorCode::Range,
        ),
        ("organize.default_layout", FieldErrorCode::Unknown),
        (
            "organize.folder_names.anbernic-stock.gba",
            FieldErrorCode::Invalid,
        ),
        ("organize.folder_names.ghost", FieldErrorCode::Unknown),
        ("organize.arcade_order.ghost", FieldErrorCode::Unknown),
    ] {
        assert!(
            got.contains(&(want.0.to_string(), want.1)),
            "missing {want:?} in {got:?}"
        );
    }

    let mut s = Settings::default();
    s.organize.auto_threshold = 0.6;
    s.organize.review_threshold = 0.7;
    s.organize.folder_names.insert(
        "anbernic-stock".into(),
        [("gba".into(), "gba".into())].into(),
    );
    s.organize
        .arcade_order
        .insert("anbernic-stock".into(), vec!["mame".into(), "nope".into()]);
    let got = codes(&s.validate(&known()));
    assert!(got.contains(&(
        "organize.review_threshold".to_string(),
        FieldErrorCode::Order
    )));
    assert!(got.contains(&(
        "organize.arcade_order.anbernic-stock".to_string(),
        FieldErrorCode::Invalid
    )));
    assert!(
        !got.iter()
            .any(|(f, _)| f.starts_with("organize.folder_names")),
        "a folder that resolves back is fine: {got:?}"
    );
    // Thresholds share the 0..=1 range.
    let fields: Vec<&str> = RANGES.iter().map(|r| r.field).collect();
    for f in [
        "organize.auto_threshold",
        "organize.review_threshold",
        "organize.recent_seconds",
        "organize.keep_organize_journals_days",
    ] {
        assert!(fields.contains(&f), "{f}");
    }
}

#[test]
fn normalise_cleans_companion_extensions() {
    let mut s = Settings::default();
    s.organize.companion_exts = vec![" SAV ".into(), "".into(), ".Srm".into(), "sav".into()];
    s.normalise();
    assert_eq!(s.organize.companion_exts, ["sav", "srm"]);
}

#[test]
fn bad_organize_values_in_a_file_are_fixed_one_by_one() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        r#"{"organize":{"auto_threshold":0.4,"review_threshold":0.9,"default_layout":"nope",
            "folder_names":{"anbernic-stock":{"gba":"SFC","snes":"SFC"}},"mode":"copy"}}"#,
    );
    let (s, report) = load_settings(dir.path(), &known());
    assert_eq!(s.organize.auto_threshold, 0.85);
    assert_eq!(s.organize.review_threshold, 0.50);
    assert_eq!(s.organize.default_layout, "anbernic-stock");
    assert_eq!(s.organize.mode, OrganizeModeSetting::Copy);
    let names = &s.organize.folder_names["anbernic-stock"];
    assert_eq!(names.get("snes").map(String::as_str), Some("SFC"));
    assert!(!names.contains_key("gba"));
    assert!(report.fixed.contains(&"organize.folder_names".to_string()));
    assert!(report
        .fixed
        .contains(&"organize.default_layout".to_string()));
}

#[test]
fn built_in_layouts_are_known_without_a_profile_list() {
    let mut s = Settings::default();
    s.organize.default_layout = "esde".into();
    s.organize
        .arcade_order
        .insert("esde".into(), vec!["fbneo".into()]);
    assert!(s.validate(&[]).is_empty(), "{:?}", s.validate(&[]));
}

#[test]
fn defaults_match_the_spec() {
    let d = Settings::default().dedupe;
    assert_eq!(
        d.levels,
        [
            DedupeLevel::Exact,
            DedupeLevel::Format,
            DedupeLevel::Version
        ]
    );
    assert!(!d.one_game_one_rom);
    assert_eq!(d.action, DedupeActionSetting::Quarantine);
    assert_eq!(d.quarantine_dir, None);
    assert_eq!(d.quarantine_name, None);
    let rules: Vec<KeepRule> = d.keep_rules.iter().map(|t| t.rule).collect();
    assert_eq!(rules, KeepRule::ALL);
    assert_eq!(
        rules,
        [
            KeepRule::HasSave,
            KeepRule::Played,
            KeepRule::InLibrary,
            KeepRule::Verified,
            KeepRule::Retail,
            KeepRule::Region,
            KeepRule::Revision,
            KeepRule::Container,
            KeepRule::Cover,
            KeepRule::CatalogueName,
        ]
    );
    assert!(d.keep_rules.iter().all(|t| t.on));
    assert_eq!(d.exclude_systems, ["ports", "bios", "apps"]);
    assert_eq!(d.max_dat_downloads, 12);
    assert_eq!(d.organize_step, OrganizeDedupeMode::SetAside);
    assert!(Settings::default().validate(&known()).is_empty());
    // Wire spelling.
    let v = serde_json::to_value(&d).unwrap();
    assert_eq!(v["levels"], json!(["exact", "format", "version"]));
    assert_eq!(v["action"], json!("quarantine"));
    assert_eq!(
        v["keep_rules"][0],
        json!({ "rule": "has_save", "on": true })
    );
    assert_eq!(v["keep_rules"][9]["rule"], json!("catalogue_name"));
    assert_eq!(v["organize_step"], json!("set-aside"));
    assert_eq!(
        serde_json::to_value(DedupeActionSetting::Trash).unwrap(),
        json!("trash")
    );
    let range = RANGES
        .iter()
        .find(|r| r.field == "dedupe.max_dat_downloads")
        .unwrap();
    assert_eq!((range.min, range.max), (0.0, 100.0));
}

#[test]
fn old_settings_file_without_dedupe_loads_defaults() {
    let s: Settings = serde_json::from_str(r#"{"version":2}"#).unwrap();
    assert_eq!(s.dedupe, DedupeSettings::default());
    let s: Settings =
        serde_json::from_str(r#"{"version":2,"dedupe":{"one_game_one_rom":true}}"#).unwrap();
    assert!(s.dedupe.one_game_one_rom);
    assert_eq!(s.dedupe.keep_rules, DedupeSettings::default().keep_rules);
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        r#"{"version":2,"general":{"theme":"dark"},"organize":{"mode":"copy"}}"#,
    );
    let (s, report) = load_settings(dir.path(), &known());
    assert_eq!(s.dedupe, DedupeSettings::default());
    assert_eq!(s.version, SETTINGS_VERSION);
    assert!(report.fixed.is_empty(), "{:?}", report.fixed);
}

#[test]
fn duplicate_rule_is_rejected() {
    let mut s = Settings::default();
    s.dedupe.keep_rules[1] = RuleToggle {
        rule: KeepRule::HasSave,
        on: false,
    };
    let got = codes(&s.validate(&known()));
    assert!(
        got.contains(&("dedupe.keep_rules".to_string(), FieldErrorCode::Duplicate)),
        "{got:?}"
    );
    let patch = json!({ "dedupe": { "keep_rules": [
        { "rule": "region", "on": true },
        { "rule": "region", "on": false }
    ] } });
    let err = Settings::default()
        .with_patch(&patch, &known())
        .unwrap_err();
    assert!(err
        .iter()
        .any(|e| e.field == "dedupe.keep_rules" && e.code == FieldErrorCode::Duplicate));
    // Two levels the same, an unknown rule.
    let mut s = Settings::default();
    s.dedupe.levels = vec![DedupeLevel::Exact, DedupeLevel::Exact];
    let got = codes(&s.validate(&known()));
    assert!(got.contains(&("dedupe.levels".to_string(), FieldErrorCode::Duplicate)));
    let patch = json!({ "dedupe": { "keep_rules": [{ "rule": "user", "on": true }] } });
    let err = Settings::default()
        .with_patch(&patch, &known())
        .unwrap_err();
    assert_eq!(err[0].field, "dedupe.keep_rules");
    // A rule left out of a reordered list comes back at the end, on.
    let patch = json!({ "dedupe": { "keep_rules": [
        { "rule": "cover", "on": false },
        { "rule": "has_save", "on": true }
    ] } });
    let s = Settings::default().with_patch(&patch, &known()).unwrap();
    let rules: Vec<KeepRule> = s.dedupe.keep_rules.iter().map(|t| t.rule).collect();
    assert_eq!(rules.len(), KeepRule::ALL.len());
    assert_eq!(
        &rules[..3],
        [KeepRule::Cover, KeepRule::HasSave, KeepRule::Played]
    );
    assert!(!s.dedupe.keep_rules[0].on);
    assert!(s.dedupe.keep_rules[2..].iter().all(|t| t.on));
    // A duplicate in a file is fixed back to the default order.
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        r#"{"dedupe":{"keep_rules":[{"rule":"cover","on":true},{"rule":"cover","on":false}],"max_dat_downloads":500}}"#,
    );
    let (s, report) = load_settings(dir.path(), &known());
    assert_eq!(s.dedupe.keep_rules, DedupeSettings::default().keep_rules);
    assert_eq!(s.dedupe.max_dat_downloads, 12);
    assert!(report.fixed.contains(&"dedupe.keep_rules".to_string()));
    assert!(report
        .fixed
        .contains(&"dedupe.max_dat_downloads".to_string()));
}

#[test]
fn quarantine_inside_a_system_folder_is_rejected() {
    // Absolute on every platform: `/x` is relative on Windows.
    let check = |dir: &str| {
        let dir = if cfg!(windows) && dir.starts_with('/') {
            format!("C:{dir}")
        } else {
            dir.to_string()
        };
        let mut s = Settings::default();
        s.dedupe.quarantine_dir = Some(PathBuf::from(dir));
        codes(&s.validate(&known()))
            .into_iter()
            .filter(|(f, _)| f == "dedupe.quarantine_dir")
            .map(|(_, c)| c)
            .collect::<Vec<_>>()
    };
    // The absolute check against the open library is in the planner; here
    // only the shape: a system folder or a ROMs folder on the way.
    assert_eq!(
        check("/Volumes/SD/ROMS/GB/_Duplicados"),
        [FieldErrorCode::Invalid]
    );
    assert_eq!(
        check("/Volumes/SD/Roms/_Duplicados"),
        [FieldErrorCode::Invalid]
    );
    assert_eq!(
        check("/Volumes/SD/sfc/_Duplicates"),
        [FieldErrorCode::Invalid]
    );
    assert_eq!(check("_Duplicados"), [FieldErrorCode::NotAbsolute]);
    assert!(check("/Volumes/SD/_Duplicados").is_empty());
    assert!(check("/Users/me/Documents/ROMS _Duplicados").is_empty());
    assert!(check("/Users/me/Desktop/Duplicates").is_empty());
    // A card or disk label is not a ROMs or system folder.
    assert!(check("/Volumes/ROMS/_Duplicados").is_empty());
    assert!(check("/Volumes/GBA/_Duplicados").is_empty());
    assert!(check("/media/me/ROMS/_Duplicados").is_empty());
    assert!(check("/run/media/me/ROMS/_Duplicados").is_empty());
    assert!(check("/mnt/ROMS/_Duplicados").is_empty());
    assert_eq!(
        check("/Volumes/ROMS/ROMS/_Duplicados"),
        [FieldErrorCode::Invalid]
    );
    assert_eq!(
        check("/media/me/ROMS/Roms/_Duplicados"),
        [FieldErrorCode::Invalid]
    );
    // The folder name is one plain name.
    for bad in ["", "  ", "a/b", "..", "."] {
        let mut s = Settings::default();
        s.dedupe.quarantine_name = Some(bad.into());
        let got = codes(&s.validate(&known()));
        assert!(
            got.contains(&(
                "dedupe.quarantine_name".to_string(),
                FieldErrorCode::Invalid
            )),
            "{bad:?}: {got:?}"
        );
    }
    let mut s = Settings::default();
    s.dedupe.quarantine_name = Some("_Duplicados".into());
    assert!(s.validate(&known()).is_empty());
}

#[test]
fn container_order_default_for_anbernic_puts_chd_first() {
    let d = DedupeSettings::default();
    let disc = |profile: &str| d.container_order[profile]["disc"].clone();
    for p in ["anbernic-stock", "garlicos", "onion", "muos", "minui"] {
        assert_eq!(disc(p), ["chd", "pbp", "cue", "iso", "img", "bin"], "{p}");
        // Cartridges: zip or raw, no preference.
        assert!(!d.container_order[p].contains_key("cart"), "{p}");
    }
    for p in ["esde", "batocera", "retroarch"] {
        assert_eq!(disc(p), ["chd", "cue", "pbp", "iso", "img", "bin"], "{p}");
    }
    assert_eq!(d.container_order["*"]["arcade"], ["zip"]);
    assert_eq!(
        d.container_for("anbernic-stock", "psx", ContainerClass::Disc),
        Some(&["chd", "pbp", "cue", "iso", "img", "bin"].map(String::from)[..])
    );
    assert_eq!(
        d.container_for("anbernic-stock", "mame", ContainerClass::Arcade),
        Some(&["zip".to_string()][..])
    );
    assert_eq!(
        d.container_for("anbernic-stock", "gba", ContainerClass::Cart),
        None
    );
    // A system entry wins over its class, a profile over `*`.
    let mut d = d;
    d.container_order
        .entry("onion".into())
        .or_default()
        .insert("psx".into(), vec!["pbp".into(), "chd".into()]);
    assert_eq!(
        d.container_for("onion", "psx", ContainerClass::Disc),
        Some(&["pbp".to_string(), "chd".to_string()][..])
    );
    // Unknown profiles, unknown keys and repeated containers are rejected.
    let mut s = Settings::default();
    s.dedupe
        .container_order
        .insert("ghost".into(), [("disc".into(), vec!["chd".into()])].into());
    s.dedupe
        .container_order
        .insert("minui".into(), [("nope".into(), vec!["chd".into()])].into());
    s.dedupe.container_order.insert(
        "esde".into(),
        [("disc".into(), vec!["chd".into(), "CHD".into()])].into(),
    );
    let got = codes(&s.validate(&known()));
    for want in [
        ("dedupe.container_order.ghost", FieldErrorCode::Unknown),
        ("dedupe.container_order.minui", FieldErrorCode::Invalid),
        ("dedupe.container_order.esde", FieldErrorCode::Invalid),
    ] {
        assert!(
            got.contains(&(want.0.to_string(), want.1)),
            "missing {want:?} in {got:?}"
        );
    }
}

#[test]
fn normalise_cleans_dedupe_lists() {
    let mut s = Settings::default();
    s.dedupe.exclude_systems = vec![" Ports ".into(), "".into(), "ports".into(), "BIOS".into()];
    s.dedupe.quarantine_name = Some(" _Duplicados ".into());
    s.normalise();
    assert_eq!(s.dedupe.exclude_systems, ["ports", "bios"]);
    assert_eq!(s.dedupe.quarantine_name.as_deref(), Some("_Duplicados"));
}

#[test]
fn external_tools_defaults_to_auto() {
    let s = Settings::default();
    assert_eq!(s.organize.external_tools, ExternalTools::Auto);
    let json = serde_json::to_value(&s).unwrap();
    assert_eq!(json["organize"]["external_tools"], json!("auto"));
    let opts = coverly_core::organize::OrganizeOptions::from_settings(&s, None);
    assert!(opts.limits.external_tools, "auto lets external tools run");
    assert_eq!(opts.limits, coverly_core::archive::Limits::default());
}

#[test]
fn external_tools_off_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Settings::default();
    s.organize.external_tools = ExternalTools::Off;
    save_settings(dir.path(), &s).unwrap();
    let text = fs::read_to_string(dir.path().join(SETTINGS_FILE)).unwrap();
    assert!(text.contains(r#""external_tools": "off""#), "{text}");
    let (back, report) = load_settings(dir.path(), &known());
    assert_eq!(back.organize.external_tools, ExternalTools::Off);
    assert_eq!(report.notice, None);
    let opts = coverly_core::organize::OrganizeOptions::from_settings(&back, None);
    assert!(
        !opts.limits.external_tools,
        "off keeps external tools from running"
    );

    // A UI patch turns it off and back on; a bad value names the field.
    let off = Settings::default()
        .with_patch(&json!({"organize": {"external_tools": "off"}}), &known())
        .unwrap();
    assert_eq!(off.organize.external_tools, ExternalTools::Off);
    let on = off
        .with_patch(&json!({"organize": {"external_tools": "auto"}}), &known())
        .unwrap();
    assert_eq!(on.organize.external_tools, ExternalTools::Auto);
    let errors = Settings::default()
        .with_patch(&json!({"organize": {"external_tools": "always"}}), &known())
        .unwrap_err();
    assert_eq!(
        codes(&errors),
        [(
            "organize.external_tools".to_string(),
            FieldErrorCode::Invalid
        )]
    );
}

#[test]
fn old_settings_file_without_field_loads() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        r#"{"version":2,"organize":{"mode":"copy","recent_seconds":30}}"#,
    );
    let (s, report) = load_settings(dir.path(), &known());
    assert_eq!(s.organize.external_tools, ExternalTools::Auto);
    assert_eq!(s.organize.mode, OrganizeModeSetting::Copy);
    assert_eq!(s.organize.recent_seconds, 30);
    assert!(report.fixed.is_empty(), "{:?}", report.fixed);

    // A value this version does not know resets only that field.
    write(
        dir.path(),
        r#"{"version":2,"organize":{"external_tools":"sometimes","mode":"copy"}}"#,
    );
    let (s, report) = load_settings(dir.path(), &known());
    assert_eq!(s.organize.external_tools, ExternalTools::Auto);
    assert_eq!(s.organize.mode, OrganizeModeSetting::Copy);
    assert_eq!(report.fixed, ["organize.external_tools"]);
}

// Systems batch 2 (D17): the embedded source and settings version 3.

fn source_order(s: &Settings) -> Vec<(&str, bool)> {
    s.sources
        .order
        .iter()
        .map(|e| (e.id.as_str(), e.enabled))
        .collect()
}

#[test]
fn v2_file_gains_embedded_enabled_at_the_end() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        r#"{"version":2,"sources":{"order":[
            {"id":"steamgriddb","enabled":true},
            {"id":"libretro","enabled":true},
            {"id":"screenscraper","enabled":false}]}}"#,
    );
    let (s, report) = load_settings(dir.path(), &known());
    assert_eq!(s.version, 3);
    assert_eq!(SETTINGS_VERSION, 3);
    assert_eq!(
        source_order(&s),
        [
            ("steamgriddb", true),
            ("libretro", true),
            ("screenscraper", false),
            ("embedded", true)
        ]
    );
    assert!(report.fixed.is_empty(), "{:?}", report.fixed);
    // Saved and read again, it is a version 3 file and stays as it is.
    save_settings(dir.path(), &s).unwrap();
    let text = fs::read_to_string(dir.path().join(SETTINGS_FILE)).unwrap();
    assert!(text.contains("\"version\": 3"), "{text}");
    let (again, _) = load_settings(dir.path(), &known());
    assert_eq!(source_order(&again), source_order(&s));
}

#[test]
fn v2_file_without_sources_gets_the_defaults() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), r#"{"version":2,"general":{"theme":"dark"}}"#);
    let (s, _) = load_settings(dir.path(), &known());
    assert_eq!(source_order(&s), source_order(&Settings::default()));
    assert!(s
        .sources
        .order
        .iter()
        .any(|e| e.id == "embedded" && e.enabled));
}

#[test]
fn v3_file_keeps_embedded_off() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        r#"{"version":3,"sources":{"order":[
            {"id":"libretro","enabled":true},
            {"id":"embedded","enabled":false},
            {"id":"steamgriddb","enabled":false},
            {"id":"screenscraper","enabled":false}]}}"#,
    );
    let (s, _) = load_settings(dir.path(), &known());
    assert_eq!(
        source_order(&s),
        [
            ("libretro", true),
            ("embedded", false),
            ("steamgriddb", false),
            ("screenscraper", false)
        ]
    );
    // A version 3 file that lost the entry gets it back off (normalise).
    write(
        dir.path(),
        r#"{"version":3,"sources":{"order":[{"id":"libretro","enabled":true}]}}"#,
    );
    let (s, _) = load_settings(dir.path(), &known());
    assert!(s
        .sources
        .order
        .iter()
        .any(|e| e.id == "embedded" && !e.enabled));
}

#[test]
fn available_sources_with_false_contains_embedded() {
    assert!(available_sources_with(false).contains(&"embedded"));
    assert!(available_sources_with(true).contains(&"embedded"));
    assert!(available_sources().contains(&"embedded"));
    assert!(KNOWN_SOURCES.contains(&"embedded"));
}

#[test]
fn an_unknown_source_in_a_v3_file_is_dropped() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        r#"{"version":3,"sources":{"order":[
            {"id":"hologram","enabled":true},
            {"id":"embedded","enabled":true},
            {"id":"libretro","enabled":false}]}}"#,
    );
    let (s, _) = load_settings(dir.path(), &known());
    let ids: Vec<&str> = s.sources.order.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(
        ids,
        ["embedded", "libretro", "steamgriddb", "screenscraper"]
    );
    assert!(s.sources.order[0].enabled);
}

// ---- Switch name catalogue (adjustments D2, D4, D8, D9) -----------------

#[test]
fn the_switch_catalogue_is_off_by_default_and_a_file_without_it_loads() {
    let s = Settings::default();
    let c = &s.catalogs.switch;
    assert!(!c.enabled && !c.icons && !c.fallback);
    assert_eq!(c.refresh_days, 7);
    assert_eq!(c.fallback_url, "https://api.nlib.cc");
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), r#"{"version":3,"general":{"theme":"dark"}}"#);
    let (s, report) = load_settings(dir.path(), &known());
    assert_eq!(s.general.theme, ThemeChoice::Dark);
    assert_eq!(s.catalogs, CatalogSettings::default());
    assert_eq!(report.notice, None);
}

#[test]
fn an_unknown_catalogue_group_is_ignored() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        r#"{"version":3,"catalogs":{"other":{"enabled":true},
            "switch":{"enabled":true,"refresh_days":14,"future_field":1}}}"#,
    );
    let (s, report) = load_settings(dir.path(), &known());
    assert!(s.catalogs.switch.enabled);
    assert_eq!(s.catalogs.switch.refresh_days, 14);
    assert_eq!(report.notice, None, "{:?}", report.fixed);
}

#[test]
fn refresh_days_must_be_between_7_and_90() {
    for (days, ok) in [
        (3u16, false),
        (6, false),
        (7, true),
        (90, true),
        (91, false),
        (200, false),
    ] {
        let mut s = Settings::default();
        s.catalogs.switch.refresh_days = days;
        let errors = s.validate(&known());
        let bad = errors
            .iter()
            .find(|e| e.field == "catalogs.switch.refresh_days");
        assert_eq!(bad.is_none(), ok, "{days}: {errors:?}");
        if let Some(e) = bad {
            assert_eq!(e.code, FieldErrorCode::Range);
            assert_eq!((e.min, e.max), (Some(7.0), Some(90.0)));
        }
    }
    let errors = Settings::default()
        .with_patch(
            &json!({"catalogs": {"switch": {"refresh_days": 200}}}),
            &known(),
        )
        .unwrap_err();
    assert_eq!(
        codes(&errors),
        [(
            "catalogs.switch.refresh_days".to_string(),
            FieldErrorCode::Range
        )]
    );
}

#[test]
fn the_fallback_url_must_be_plain_https() {
    for (url, code) in [
        ("http://x", Some(FieldErrorCode::Invalid)),
        ("ftp://api.nlib.cc", Some(FieldErrorCode::Invalid)),
        ("https://", Some(FieldErrorCode::Invalid)),
        ("https://api nlib.cc", Some(FieldErrorCode::Invalid)),
        ("https://api.nlib.cc?x=1", Some(FieldErrorCode::Invalid)),
        ("https://api.nlib.cc#a", Some(FieldErrorCode::Invalid)),
        ("https://user:pw@api.nlib.cc", Some(FieldErrorCode::Invalid)),
        ("", Some(FieldErrorCode::Required)),
        ("https://api.nlib.cc", None),
        ("https://mirror.example.org/titles", None),
    ] {
        let mut s = Settings::default();
        s.catalogs.switch.fallback_url = url.to_string();
        let got = s
            .validate(&known())
            .into_iter()
            .find(|e| e.field == "catalogs.switch.fallback_url")
            .map(|e| e.code);
        assert_eq!(got, code, "{url}");
    }
    // A URL the settings accept must also be one the network layer will
    // fetch from: anything `net::hosts::allowed` refuses is refused here,
    // instead of every lookup failing silently later.
    for url in [
        "https://api_nlib.cc",
        "https://api.nlib.cc:99999",
        "https://ñ.example",
        "https://api.nlib.cc/a/../b",
        "https://api.nlib.cc/%2e%2e/x",
        "https://api.nlib.cc/a%2Fb",
        "https://api.nlib.cc/a%5cb",
        "https://api.nlib.cc/\u{7f}",
    ] {
        let mut s = Settings::default();
        s.catalogs.switch.fallback_url = url.to_string();
        let got = s
            .validate(&known())
            .into_iter()
            .find(|e| e.field == "catalogs.switch.fallback_url")
            .map(|e| e.code);
        assert_eq!(got, Some(FieldErrorCode::Invalid), "{url:?}");
    }
    for url in [
        "https://api.nlib.cc",
        "https://mirror.example.org/titles",
        "https://127.0.0.1:8443",
        "https://[::1]:8443/v1",
    ] {
        let mut s = Settings::default();
        s.catalogs.switch.fallback_url = url.to_string();
        assert!(s.validate(&known()).is_empty(), "{url}");
        let lookup = format!("{url}/v1/0100000000010000");
        assert!(
            coverly_core::net::hosts::allowed(&lookup, &[url]),
            "{lookup} must be fetchable"
        );
    }
    // The same code the manual proxy URL uses, so the UI shows the same message.
    let mut s = Settings::default();
    s.network.proxy = ProxyChoice::Manual;
    s.network.proxy_url = "nope".into();
    s.catalogs.switch.fallback_url = "http://x".into();
    let errors = s.validate(&known());
    let code_of = |f: &str| errors.iter().find(|e| e.field == f).map(|e| e.code);
    assert_eq!(
        code_of("catalogs.switch.fallback_url"),
        code_of("network.proxy_url")
    );
}

#[test]
fn icons_and_fallback_only_stay_on_while_the_catalogue_is_on() {
    let s = Settings::default();
    let next = s
        .with_patch(
            &json!({"catalogs": {"switch": {"icons": true, "fallback": true}}}),
            &known(),
        )
        .unwrap();
    assert!(!next.catalogs.switch.icons && !next.catalogs.switch.fallback);
    let on = s
        .with_patch(
            &json!({"catalogs": {"switch": {"enabled": true, "icons": true, "fallback": true}}}),
            &known(),
        )
        .unwrap();
    assert!(on.catalogs.switch.enabled && on.catalogs.switch.icons && on.catalogs.switch.fallback);
    // Turning the catalogue off turns both off with it.
    let off = on
        .with_patch(
            &json!({"catalogs": {"switch": {"enabled": false}}}),
            &known(),
        )
        .unwrap();
    assert!(!off.catalogs.switch.icons && !off.catalogs.switch.fallback);
    // A file with icons on and the catalogue off loads cleaned, not reset.
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        r#"{"version":3,"catalogs":{"switch":{"icons":true,"refresh_days":30}}}"#,
    );
    let (s, _) = load_settings(dir.path(), &known());
    assert!(!s.catalogs.switch.icons);
    assert_eq!(s.catalogs.switch.refresh_days, 30);
}

#[test]
fn catalogue_patches_round_trip_and_trim_the_url() {
    let s = Settings::default();
    let next = s
        .with_patch(
            &json!({"catalogs": {"switch": {
                "enabled": true,
                "refresh_days": 30,
                "fallback_url": "  https://mirror.example.org/  "
            }}}),
            &known(),
        )
        .unwrap();
    assert!(next.catalogs.switch.enabled);
    assert_eq!(next.catalogs.switch.refresh_days, 30);
    assert_eq!(
        next.catalogs.switch.fallback_url,
        "https://mirror.example.org"
    );
    let dir = tempfile::tempdir().unwrap();
    save_settings(dir.path(), &next).unwrap();
    let (back, report) = load_settings(dir.path(), &known());
    assert_eq!(back.catalogs, next.catalogs);
    assert_eq!(report.notice, None);
    // `null` resets the group.
    let reset = next
        .with_patch(&json!({"catalogs": null}), &known())
        .unwrap();
    assert_eq!(reset.catalogs, CatalogSettings::default());
    let errors = s
        .with_patch(
            &json!({"catalogs": {"switch": {"enabled": "yes"}}}),
            &known(),
        )
        .unwrap_err();
    assert_eq!(
        codes(&errors),
        [(
            "catalogs.switch.enabled".to_string(),
            FieldErrorCode::Invalid
        )]
    );
}

#[test]
fn a_bad_catalogue_field_resets_only_that_field() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        r#"{"version":3,"catalogs":{"switch":{"enabled":true,"refresh_days":1,
            "fallback":true,"fallback_url":"http://x"}}}"#,
    );
    let (s, report) = load_settings(dir.path(), &known());
    let c = &s.catalogs.switch;
    assert!(c.enabled && c.fallback, "{c:?}");
    assert_eq!(c.refresh_days, 7);
    assert_eq!(c.fallback_url, "https://api.nlib.cc");
    assert_eq!(report.notice, Some(LoadNotice::Fixed));
    assert!(report
        .fixed
        .contains(&"catalogs.switch.refresh_days".to_string()));
    assert!(report
        .fixed
        .contains(&"catalogs.switch.fallback_url".to_string()));
}

#[test]
fn a_build_without_catalogues_ignores_the_group() {
    // A rollback build knows every group but `catalogs`: the file it reads
    // still parses, and what it writes back keeps the other groups.
    #[derive(serde::Deserialize)]
    #[allow(dead_code)]
    struct OldSettings {
        version: u32,
        general: serde_json::Value,
        network: serde_json::Value,
    }
    let mut s = Settings::default();
    s.catalogs.switch.enabled = true;
    let text = serde_json::to_string(&s).unwrap();
    let old: OldSettings = serde_json::from_str(&text).unwrap();
    assert_eq!(old.version, SETTINGS_VERSION);
    assert_eq!(SETTINGS_VERSION, 3, "the group needs no new format version");
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["catalogs"]["switch"]["enabled"], json!(true));
}
