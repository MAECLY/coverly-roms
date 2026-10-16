use std::fs;
use std::path::Path;

use coverly_core::cache::{Cache, DatRow, CACHE_FILE};
use coverly_core::identify::hash::HashOutcome;
use coverly_core::paths::AppDirs;
use tempfile::TempDir;

fn zip_outcome() -> HashOutcome {
    HashOutcome::ZipEntry {
        crc32: "CBF43926".into(),
        size: 9,
        inner_name: "a.gba".into(),
    }
}

fn dir_names(dir: &Path) -> Vec<String> {
    fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect()
}

#[test]
fn quick_ids_are_reused_only_for_the_same_size_and_mtime() {
    let cache = Cache::in_memory().unwrap();
    let p = Path::new("/lib/ROMS/GBA/a.zip");
    cache.put_quick_id(p, 9, 1_000, &zip_outcome()).unwrap();
    assert_eq!(cache.quick_id(p, 9, 1_000).unwrap(), Some(zip_outcome()));
    assert_eq!(cache.quick_id(p, 10, 1_000).unwrap(), None);
    assert_eq!(cache.quick_id(p, 9, 2_000).unwrap(), None);
}

#[test]
fn json_values_round_trip_with_etag() {
    let cache = Cache::in_memory().unwrap();
    cache
        .put_json(
            "libretro-index",
            "GBA/Named_Boxarts",
            &vec!["a.png", "b.png"],
            Some("\"e1\""),
        )
        .unwrap();
    let got = cache
        .get_json::<Vec<String>>("libretro-index", "GBA/Named_Boxarts")
        .unwrap()
        .unwrap();
    assert_eq!(got.value, ["a.png", "b.png"]);
    assert_eq!(got.etag.as_deref(), Some("\"e1\""));
    assert!(got.fetched_at > 1_700_000_000);
    assert!(cache
        .get_json::<Vec<String>>("libretro-index", "other")
        .unwrap()
        .is_none());
}

#[test]
fn dat_rows_are_replaced_and_found_by_crc_and_stem() {
    let cache = Cache::in_memory().unwrap();
    let row = |game: &str, rom: &str, crc: &str| DatRow {
        game: game.into(),
        rom_name: rom.into(),
        size: Some(9),
        crc32: Some(crc.into()),
        md5: None,
        sha1: None,
    };
    cache
        .replace_dat("mame", &[row("Old", "old.zip", "00000000")])
        .unwrap();
    cache
        .replace_dat(
            "mame",
            &[row(
                "10-Yard Fight (World, set 1)",
                "10yard.zip",
                "8F401426",
            )],
        )
        .unwrap();
    assert!(cache.dat_by_crc("mame", "00000000").unwrap().is_empty());
    assert_eq!(
        cache.dat_by_crc("mame", "8f401426").unwrap()[0].game,
        "10-Yard Fight (World, set 1)"
    );
    assert_eq!(cache.dat_by_rom_stem("mame", "10YARD").unwrap().len(), 1);
}

#[test]
fn data_persists_across_reopen() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("nested").join(CACHE_FILE);
    Cache::open(&path)
        .unwrap()
        .put_art_file("https://x/a.png", "ABC", 12)
        .unwrap();
    assert_eq!(
        Cache::open(&path)
            .unwrap()
            .art_file("https://x/a.png")
            .unwrap(),
        Some(("ABC".into(), 12))
    );
}

#[test]
fn corrupt_database_is_set_aside_and_recreated() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(CACHE_FILE);
    fs::write(
        &path,
        b"this is not a sqlite database at all, just garbage bytes",
    )
    .unwrap();
    let cache = Cache::open(&path).unwrap();
    cache.put_json("ns", "k", &1u8, None).unwrap();
    let names = dir_names(dir.path());
    assert!(
        names
            .iter()
            .any(|n| n.starts_with("coverly.sqlite3.corrupt-")),
        "{names:?}"
    );
}

#[test]
fn newer_schema_is_set_aside_too() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(CACHE_FILE);
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.pragma_update(None, "user_version", 99).unwrap();
    drop(conn);
    Cache::open(&path)
        .unwrap()
        .put_json("ns", "k", &1u8, None)
        .unwrap();
    let names = dir_names(dir.path());
    assert!(
        names
            .iter()
            .any(|n| n.starts_with("coverly.sqlite3.corrupt-")),
        "{names:?}"
    );
}

#[test]
fn busy_database_is_not_set_aside() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(CACHE_FILE);
    Cache::open(&path)
        .unwrap()
        .put_json("ns", "k", &1u8, None)
        .unwrap();
    // Another process (the app while the CLI runs) holds an exclusive lock.
    let other = rusqlite::Connection::open(&path).unwrap();
    other.execute_batch("BEGIN EXCLUSIVE").unwrap();
    assert!(Cache::open(&path).is_err());
    other.execute_batch("COMMIT").unwrap();
    drop(other);
    let names = dir_names(dir.path());
    assert!(!names.iter().any(|n| n.contains(".corrupt-")), "{names:?}");
    let kept = Cache::open(&path)
        .unwrap()
        .get_json::<u8>("ns", "k")
        .unwrap();
    assert_eq!(kept.map(|s| s.value), Some(1));
}

#[test]
fn cache_is_shareable_between_threads() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Cache>();
}

#[test]
fn app_dirs_honour_overrides() {
    let dirs = AppDirs::resolve(Some(Path::new("/c")), Some(Path::new("/d"))).unwrap();
    assert_eq!(
        (dirs.cache.as_path(), dirs.data.as_path()),
        (Path::new("/c"), Path::new("/d"))
    );
}
