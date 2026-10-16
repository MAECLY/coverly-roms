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
            ("screenscraper", false)
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
            id: "steamgriddb".into(),
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
    assert_eq!(
        order,
        [
            ("steamgriddb", false),
            ("libretro", true),
            ("screenscraper", false)
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
