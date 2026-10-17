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

/// The v1 DDL, copied from `cache/mod.rs` as it shipped in schema v1.
const V1_SCHEMA_FOR_TEST: &str = "
CREATE TABLE quick_ids (path TEXT PRIMARY KEY, size INTEGER NOT NULL, mtime_ns INTEGER NOT NULL, outcome TEXT NOT NULL);
CREATE TABLE kv (ns TEXT NOT NULL, key TEXT NOT NULL, value TEXT NOT NULL, etag TEXT, fetched_at INTEGER NOT NULL, PRIMARY KEY (ns, key));
CREATE TABLE dat_roms (dat TEXT NOT NULL, game TEXT NOT NULL, rom_name TEXT NOT NULL, rom_stem TEXT NOT NULL,
                       size INTEGER, crc32 TEXT, md5 TEXT, sha1 TEXT);
CREATE INDEX dat_roms_crc ON dat_roms (dat, crc32);
CREATE INDEX dat_roms_stem ON dat_roms (dat, rom_stem);
CREATE TABLE art_files (url TEXT PRIMARY KEY, sha1 TEXT NOT NULL, len INTEGER NOT NULL, fetched_at INTEGER NOT NULL);
";

fn row(game: &str, rom: &str, size: u64, crc: &str) -> DatRow {
    DatRow {
        game: game.into(),
        rom_name: rom.into(),
        size: Some(size),
        crc32: Some(crc.into()),
        md5: None,
        sha1: None,
    }
}

#[test]
fn a_v1_database_migrates_to_v2_and_keeps_its_rows() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("c.sqlite3");
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(V1_SCHEMA_FOR_TEST).unwrap();
        conn.pragma_update(None, "user_version", 1).unwrap();
        conn.execute(
            "INSERT INTO dat_roms VALUES ('d.dat','Tetris (World)','t.gb','t',32768,'46DF91AD',NULL,NULL)",
            [],
        )
        .unwrap();
    }
    let cache = Cache::open(&path).unwrap();
    let hits = cache.dat_by_crc_any("46df91ad", 32768).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].0, "d.dat");
    assert_eq!(hits[0].1.game, "Tetris (World)");
    assert_eq!(cache.cached_dats().unwrap(), ["d.dat"]);
    drop(cache);
    // Migrated, not set aside, and the version is stamped.
    assert!(!dir_names(tmp.path())
        .iter()
        .any(|n| n.contains(".corrupt-")));
    let conn = rusqlite::Connection::open(&path).unwrap();
    let version: i32 = conn
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    assert_eq!(version, coverly_core::cache::SCHEMA_VERSION);
    assert_eq!(version, 2);
}

#[test]
fn a_v2_database_reopens_with_its_facts() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join(CACHE_FILE);
    let p = Path::new("/pile/a.zip");
    Cache::open(&path)
        .unwrap()
        .put_organize_facts(p, 1, 2, "{}")
        .unwrap();
    assert_eq!(
        Cache::open(&path)
            .unwrap()
            .organize_facts(p, 1, 2)
            .unwrap()
            .as_deref(),
        Some("{}")
    );
}

#[test]
fn crc_any_respects_size_and_case() {
    let cache = Cache::in_memory().unwrap();
    cache
        .replace_dat(
            "gb.dat",
            &[row("Tetris (World)", "Tetris.gb", 32768, "46DF91AD")],
        )
        .unwrap();
    cache
        .replace_dat("gbc.dat", &[row("Other", "o.gbc", 65536, "46DF91AD")])
        .unwrap();
    let hits = cache.dat_by_crc_any("46df91ad", 32768).unwrap();
    assert_eq!(
        hits.iter().map(|h| h.0.as_str()).collect::<Vec<_>>(),
        ["gb.dat"]
    );
    assert!(cache.dat_by_crc_any("46DF91AD", 1).unwrap().is_empty());
}

#[test]
fn crc_any_accepts_rows_without_a_size() {
    let cache = Cache::in_memory().unwrap();
    let mut sizeless = row("Any", "a.bin", 0, "0000ABCD");
    sizeless.size = None;
    cache.replace_dat("x.dat", &[sizeless]).unwrap();
    assert_eq!(cache.dat_by_crc_any("0000abcd", 123).unwrap().len(), 1);
}

#[test]
fn rows_of_a_game_and_stems_of_a_dat() {
    let cache = Cache::in_memory().unwrap();
    cache
        .replace_dat(
            "psx.dat",
            &[
                row("Game (USA)", "Game (USA).cue", 100, "11111111"),
                row("Game (USA)", "Game (USA) (Track 1).bin", 200, "22222222"),
                row("Other (Europe)", "Other (Europe).bin", 300, "33333333"),
            ],
        )
        .unwrap();
    let rows = cache.dat_rows_of_game("psx.dat", "Game (USA)").unwrap();
    assert_eq!(
        rows.iter().map(|r| r.rom_name.as_str()).collect::<Vec<_>>(),
        ["Game (USA) (Track 1).bin", "Game (USA).cue"]
    );
    assert!(cache
        .dat_rows_of_game("other.dat", "Game (USA)")
        .unwrap()
        .is_empty());
    let mut stems = cache.dat_stems("psx.dat").unwrap();
    stems.sort();
    assert_eq!(
        stems,
        [
            ("game (usa)".to_owned(), "Game (USA)".to_owned()),
            ("game (usa) (track 1)".to_owned(), "Game (USA)".to_owned()),
            ("other (europe)".to_owned(), "Other (Europe)".to_owned()),
        ]
    );
    assert_eq!(cache.cached_dats().unwrap(), ["psx.dat"]);
}

#[test]
fn organise_facts_are_keyed_by_size_and_mtime() {
    let cache = Cache::in_memory().unwrap();
    let p = Path::new("/pile/a.zip");
    cache.put_organize_facts(p, 10, 5, "{\"x\":1}").unwrap();
    assert_eq!(
        cache.organize_facts(p, 10, 5).unwrap().as_deref(),
        Some("{\"x\":1}")
    );
    assert_eq!(cache.organize_facts(p, 10, 6).unwrap(), None);
    assert_eq!(cache.organize_facts(p, 11, 5).unwrap(), None);
    cache.put_organize_facts(p, 10, 6, "{\"x\":2}").unwrap();
    assert_eq!(cache.organize_facts(p, 10, 5).unwrap(), None);
    assert_eq!(
        cache.organize_facts(p, 10, 6).unwrap().as_deref(),
        Some("{\"x\":2}")
    );
}

#[test]
fn user_choices_are_remembered_by_content() {
    let cache = Cache::in_memory().unwrap();
    cache.put_organize_choice("ABCDEF01", 100, "gbc").unwrap();
    assert_eq!(
        cache.organize_choice("abcdef01", 100).unwrap().as_deref(),
        Some("gbc")
    );
    assert_eq!(cache.organize_choice("ABCDEF01", 101).unwrap(), None);
    cache.put_organize_choice("abcdef01", 100, "gb").unwrap();
    assert_eq!(
        cache.organize_choice("ABCDEF01", 100).unwrap().as_deref(),
        Some("gb")
    );
}
