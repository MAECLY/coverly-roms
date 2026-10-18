//! Network, cache and safety settings.

mod common;

use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};

use coverly_core::cache::{cache_usage, clear_cache, Cache, CACHE_FILE};
use coverly_core::jobs::{trim_art_store, ArtStore};
use coverly_core::net::ProxyMode;
use coverly_core::settings::{ProxyChoice, Settings};
use coverly_core::write::{clean_apple_doubles, on_removable_volume};

const APPLE_DOUBLE: [u8; 8] = [0x00, 0x05, 0x16, 0x07, 0x00, 0x02, 0x00, 0x00];

#[test]
fn net_config_follows_the_network_group() {
    let mut s = Settings::default();
    s.network.concurrency = 2;
    s.network.timeout_secs = 12;
    s.network.retries = 0;
    let n = s.net_config();
    assert_eq!(n.concurrency, 2);
    assert_eq!(n.timeout, Duration::from_secs(12));
    assert_eq!(n.retries, 0);
    assert_eq!(
        n.proxy,
        ProxyMode::Env,
        "system reads the environment (D26)"
    );
    s.network.proxy = ProxyChoice::Env;
    assert_eq!(s.net_config().proxy, ProxyMode::Env);
    s.network.proxy = ProxyChoice::Manual;
    s.network.proxy_url = "http://proxy.local:3128".into();
    assert_eq!(
        s.net_config().proxy,
        ProxyMode::Manual("http://proxy.local:3128".into())
    );
}

#[test]
fn apply_options_follow_art_network_and_safety() {
    let mut s = Settings::default();
    s.art.keep_aspect = false;
    s.art.jpeg_quality = 70;
    s.network.concurrency = 3;
    s.network.cache_max_mb = 100;
    s.safety.undo_depth = 4;
    s.organize.keep_organize_journals_days = 365;
    let o = s.apply_options(Path::new("/Users/me/sd-copy"));
    assert_eq!(o.fit, coverly_core::image::Fit::Stretch);
    assert_eq!(o.jpeg_quality, 70);
    assert_eq!(o.concurrency, 3);
    assert_eq!(o.keep_jobs, 4);
    assert_eq!(o.cache_max_bytes, Some(100 * 1024 * 1024));
    assert!(!o.clean_apple_double, "not a removable volume");
    let m = s.manual_options();
    assert_eq!((m.jpeg_quality, m.keep_jobs), (70, 4));
    assert_eq!((o.keep_organize_days, m.keep_organize_days), (365, 365));
}

#[test]
fn removable_volumes_are_recognised_by_their_mount_point() {
    if cfg!(target_os = "macos") {
        assert!(on_removable_volume(Path::new("/Volumes/SD/ROMS")));
        assert!(!on_removable_volume(Path::new("/Users/me/ROMS")));
    }
    if cfg!(target_os = "linux") {
        assert!(on_removable_volume(Path::new("/media/me/SD")));
        assert!(on_removable_volume(Path::new("/run/media/me/SD")));
        assert!(!on_removable_volume(Path::new("/home/me/ROMS")));
    }
}

#[test]
fn data_dir_defaults_to_the_app_folder() {
    let mut s = Settings::default();
    assert_eq!(s.data_dir(Path::new("/app/data")), Path::new("/app/data"));
    s.safety.backup_dir = Some("/backups/coverly".into());
    assert_eq!(
        s.data_dir(Path::new("/app/data")),
        Path::new("/backups/coverly")
    );
}

#[test]
fn scan_settings_and_the_single_slot_ask_for_a_reopen() {
    let a = Settings::default();
    let mut b = a.clone();
    b.network.timeout_secs = 60;
    b.general.theme = coverly_core::settings::ThemeChoice::Dark;
    assert!(!a.needs_reopen(&b));
    for change in [
        |s: &mut Settings| s.library.include_hidden = true,
        |s: &mut Settings| s.library.ignored_dirs.push("x".into()),
        |s: &mut Settings| s.art.single_slot_media = coverly_core::profiles::Media::Title,
        |s: &mut Settings| s.art.output_format = coverly_core::settings::OutputFormat::Jpg,
    ] {
        let mut c = a.clone();
        change(&mut c);
        assert!(a.needs_reopen(&c));
    }
}

#[test]
fn clean_apple_doubles_removes_only_real_appledouble_files() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    fs::write(d.join("._a.png"), APPLE_DOUBLE).unwrap();
    fs::write(d.join("._notes.txt"), b"plain text that starts with ._").unwrap();
    fs::write(d.join("a.png"), b"png").unwrap();
    fs::create_dir(d.join("._folder")).unwrap();
    assert_eq!(clean_apple_doubles(d), 1);
    assert!(!d.join("._a.png").exists());
    assert!(
        d.join("._notes.txt").exists() && d.join("a.png").exists() && d.join("._folder").is_dir()
    );
    assert_eq!(clean_apple_doubles(&d.join("missing")), 0);
}

#[test]
fn trim_art_store_drops_the_oldest_files_first() {
    let cache_dir = tempfile::tempdir().unwrap();
    let art = cache_dir.path().join("art");
    let mut paths = Vec::new();
    for (i, size) in [300usize, 300, 300].into_iter().enumerate() {
        let sha = format!("{:040x}", i + 1);
        let path = art.join(&sha[..2]).join(&sha);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, vec![0u8; size]).unwrap();
        let t = SystemTime::now() - Duration::from_secs(1000 - i as u64 * 100);
        filetime::set_file_mtime(&path, filetime::FileTime::from_system_time(t)).unwrap();
        paths.push(path);
    }
    let removed = trim_art_store(cache_dir.path(), 650).unwrap();
    assert_eq!(removed, 300);
    assert!(!paths[0].exists(), "the oldest goes first");
    assert!(paths[1].exists() && paths[2].exists());
    assert_eq!(trim_art_store(cache_dir.path(), 10_000).unwrap(), 0);
    assert_eq!(
        trim_art_store(&cache_dir.path().join("none"), 1).unwrap(),
        0
    );
}

#[test]
fn a_trimmed_url_is_a_cache_miss() {
    let cache_dir = tempfile::tempdir().unwrap();
    let cache = Cache::open(&cache_dir.path().join(CACHE_FILE)).unwrap();
    let store = ArtStore::new(cache_dir.path(), &cache);
    store.put("https://x/a.png", b"image-a").unwrap();
    assert!(store.get("https://x/a.png").is_some());
    trim_art_store(cache_dir.path(), 0).unwrap();
    assert!(store.get("https://x/a.png").is_none());
}

#[test]
fn usage_and_clear_cover_the_database_and_the_art_store() {
    let cache_dir = tempfile::tempdir().unwrap();
    {
        let cache = Cache::open(&cache_dir.path().join(CACHE_FILE)).unwrap();
        ArtStore::new(cache_dir.path(), &cache)
            .put("https://x/a.png", &[7u8; 4096])
            .unwrap();
    }
    let used = cache_usage(cache_dir.path());
    assert!(used >= 4096, "{used}");
    let freed = clear_cache(cache_dir.path()).unwrap();
    assert!(freed >= 4096);
    assert_eq!(cache_usage(cache_dir.path()), 0);
    // A fresh cache opens again after a clear.
    Cache::open(&cache_dir.path().join(CACHE_FILE)).unwrap();
    // Nothing to clear is fine.
    clear_cache(&cache_dir.path().join("none")).unwrap();
}
