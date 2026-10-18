use std::fs;
use std::path::Path;

use coverly_core::cache::{Cache, ContentRow, DatRow, StoredChoice, CACHE_FILE, SCHEMA_VERSION};
use coverly_core::identify::dat::DatGame;
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
        serial: None,
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
        serial: None,
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
    assert_eq!(version, 3);
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
                row("Other (Europe)", "Other (Europe).bin", 300, "33333333"),
                row("Game (USA)", "Game (USA).cue", 100, "11111111"),
                row("Game (USA)", "Game (USA) (Track 1).bin", 200, "22222222"),
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
    // Ordered by stem then game, so fuzzy ties resolve the same way on
    // every run whatever order the rows were inserted in.
    let stems = cache.dat_stems("psx.dat").unwrap();
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

/// The v2 DDL, copied from `cache/mod.rs` as it shipped in schema v2.
const V2_SCHEMA_FOR_TEST: &str = "
CREATE INDEX IF NOT EXISTS dat_roms_crc_any ON dat_roms (crc32);
CREATE INDEX IF NOT EXISTS dat_roms_game ON dat_roms (dat, game);
CREATE TABLE IF NOT EXISTS organize_ids (path TEXT PRIMARY KEY, size INTEGER NOT NULL, mtime_ns INTEGER NOT NULL, facts TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS organize_choices (crc32 TEXT NOT NULL, size INTEGER NOT NULL, system_id TEXT NOT NULL, PRIMARY KEY (crc32, size));
";

fn user_version(path: &Path) -> i32 {
    rusqlite::Connection::open(path)
        .unwrap()
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap()
}

#[test]
fn migrates_v2_to_v3_keeping_rows() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join(CACHE_FILE);
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(V1_SCHEMA_FOR_TEST).unwrap();
        conn.execute_batch(V2_SCHEMA_FOR_TEST).unwrap();
        conn.pragma_update(None, "user_version", 2).unwrap();
        conn.execute(
            "INSERT INTO dat_roms VALUES ('d.dat','Tetris (World)','t.gb','t',32768,'46DF91AD',NULL,NULL)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO organize_ids VALUES ('/pile/a.zip', 1, 2, '{}')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO organize_choices VALUES ('ABCDEF01', 100, 'gbc')",
            [],
        )
        .unwrap();
    }
    let cache = Cache::open(&path).unwrap();
    let hits = cache.dat_by_crc("d.dat", "46DF91AD").unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].serial, None);
    assert_eq!(
        cache
            .organize_facts(Path::new("/pile/a.zip"), 1, 2)
            .unwrap()
            .as_deref(),
        Some("{}")
    );
    assert_eq!(
        cache.organize_choice("ABCDEF01", 100).unwrap().as_deref(),
        Some("gbc")
    );
    // The new tables work on the migrated database.
    cache.put_content_row(&content("/lib/a.gb")).unwrap();
    drop(cache);
    assert!(!dir_names(tmp.path())
        .iter()
        .any(|n| n.contains(".corrupt-")));
    assert_eq!(user_version(&path), 3);
}

#[test]
fn fresh_database_is_v3() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join(CACHE_FILE);
    drop(Cache::open(&path).unwrap());
    assert_eq!(SCHEMA_VERSION, 3);
    assert_eq!(user_version(&path), 3);
    let conn = rusqlite::Connection::open(&path).unwrap();
    for table in ["content_keys", "content_key_index", "dedupe_choices"] {
        let n: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1, "{table}");
    }
    for column in ["serial", "region", "cloneof", "isbios"] {
        let n: i64 = conn
            .query_row(
                "SELECT count(*) FROM pragma_table_info('dat_roms') WHERE name = ?1",
                [column],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1, "{column}");
    }
}

fn content(path: &str) -> ContentRow {
    ContentRow {
        volume_uuid: "VOL-1".into(),
        path: path.into(),
        size: 32_768,
        mtime_ns: 1_000,
        file_id: Some(42),
        keys_json: r#"{"l1":"sha1:AA"}"#.into(),
        serials_json: Some(r#"{"serials":["SLUS-00543"]}"#.into()),
        volume_fp: Some("fp".into()),
        index_keys: vec!["sha1:AA".into(), "crc:BB".into()],
    }
}

#[test]
fn content_row_round_trip() {
    let cache = Cache::in_memory().unwrap();
    let row = content("/lib/GB/Tetris.gb");
    cache.put_content_row(&row).unwrap();
    let p = Path::new("/lib/GB/Tetris.gb");
    let got = cache.content_row("VOL-1", p, 32_768, 1_000).unwrap();
    let mut want = row.clone();
    want.index_keys.sort();
    assert_eq!(got.as_ref(), Some(&want));
    // A stale size or mtime, another volume or another path is a miss.
    assert_eq!(cache.content_row("VOL-1", p, 32_769, 1_000).unwrap(), None);
    assert_eq!(cache.content_row("VOL-1", p, 32_768, 1_001).unwrap(), None);
    assert_eq!(cache.content_row("VOL-2", p, 32_768, 1_000).unwrap(), None);
    // The file id finds the row whatever its path; only on that volume.
    assert_eq!(cache.content_by_file_id("VOL-1", 42).unwrap(), Some(want));
    assert_eq!(cache.content_by_file_id("VOL-2", 42).unwrap(), None);
    assert_eq!(cache.content_by_file_id("VOL-1", 43).unwrap(), None);
    // Replacing the row replaces its index too.
    let mut newer = row.clone();
    newer.mtime_ns = 2_000;
    newer.index_keys = vec!["sha1:CC".into()];
    newer.serials_json = None;
    newer.file_id = None;
    cache.put_content_row(&newer).unwrap();
    assert_eq!(cache.content_row("VOL-1", p, 32_768, 1_000).unwrap(), None);
    assert_eq!(
        cache.content_row("VOL-1", p, 32_768, 2_000).unwrap(),
        Some(newer)
    );
    assert!(cache.content_by_key("sha1:AA").unwrap().is_empty());
    assert_eq!(
        cache.content_by_key("sha1:CC").unwrap(),
        [("VOL-1".to_owned(), p.to_path_buf())]
    );
}

#[test]
fn rekey_moves_the_row_and_its_index() {
    let cache = Cache::in_memory().unwrap();
    let from = Path::new("/lib/GB/Tetris (1).gb");
    let to = Path::new("/lib/_Duplicados/GB/Tetris (1).gb");
    let mut row = content(&from.to_string_lossy());
    cache.put_content_row(&row).unwrap();
    // A stale row already at the destination is replaced.
    let mut stale = content(&to.to_string_lossy());
    stale.index_keys = vec!["sha1:OLD".into()];
    cache.put_content_row(&stale).unwrap();
    cache.rekey_content("VOL-1", from, to).unwrap();
    assert_eq!(
        cache.content_row("VOL-1", from, 32_768, 1_000).unwrap(),
        None
    );
    row.path = to.to_path_buf();
    row.index_keys.sort();
    assert_eq!(
        cache.content_row("VOL-1", to, 32_768, 1_000).unwrap(),
        Some(row)
    );
    assert_eq!(
        cache.content_by_key("sha1:AA").unwrap(),
        [("VOL-1".to_owned(), to.to_path_buf())]
    );
    assert!(cache.content_by_key("sha1:OLD").unwrap().is_empty());
    // Rekeying a path with no row changes nothing.
    cache
        .rekey_content("VOL-1", Path::new("/nowhere"), from)
        .unwrap();
    assert_eq!(
        cache.content_row("VOL-1", from, 32_768, 1_000).unwrap(),
        None
    );
}

#[test]
fn dat_by_serial_uses_the_index() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join(CACHE_FILE);
    let cache = Cache::open(&path).unwrap();
    let mut a = row(
        "Driver 2 (USA) (Disc 1)",
        "Driver 2 (USA) (Disc 1).bin",
        9,
        "11111111",
    );
    a.serial = Some("SLUS-01161".into());
    let mut b = row(
        "Driver 2 (USA) (Disc 2)",
        "Driver 2 (USA) (Disc 2).bin",
        9,
        "22222222",
    );
    b.serial = Some("SLUS-01318".into());
    let c = row("No Serial", "n.bin", 9, "33333333");
    cache
        .replace_dat_games(
            "psx.dat",
            &[
                DatGame {
                    name: a.game.clone(),
                    roms: vec![a.clone()],
                    cloneof: None,
                    region: Some("USA".into()),
                    is_bios: false,
                },
                DatGame {
                    name: b.game.clone(),
                    roms: vec![b],
                    cloneof: Some("Driver 2 (USA) (Disc 1)".into()),
                    region: None,
                    is_bios: false,
                },
                DatGame {
                    name: c.game.clone(),
                    roms: vec![c],
                    cloneof: None,
                    region: None,
                    is_bios: true,
                },
            ],
        )
        .unwrap();
    assert_eq!(cache.dat_by_serial("psx.dat", "SLUS-01161").unwrap(), [a]);
    assert!(cache
        .dat_by_serial("other.dat", "SLUS-01161")
        .unwrap()
        .is_empty());
    assert!(cache
        .dat_by_serial("psx.dat", "SLUS-99999")
        .unwrap()
        .is_empty());
    // Serials come back on every lookup.
    assert_eq!(
        cache.dat_by_crc("psx.dat", "22222222").unwrap()[0]
            .serial
            .as_deref(),
        Some("SLUS-01318")
    );
    drop(cache);
    let conn = rusqlite::Connection::open(&path).unwrap();
    let mut stmt = conn
        .prepare("SELECT game, cloneof, region, isbios FROM dat_roms ORDER BY game")
        .unwrap();
    let fields: Vec<(String, Option<String>, Option<String>, bool)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        fields,
        [
            (
                "Driver 2 (USA) (Disc 1)".to_owned(),
                None,
                Some("USA".to_owned()),
                false
            ),
            (
                "Driver 2 (USA) (Disc 2)".to_owned(),
                Some("Driver 2 (USA) (Disc 1)".to_owned()),
                None,
                false
            ),
            ("No Serial".to_owned(), None, None, true),
        ]
    );
    let plan: String = conn
        .query_row(
            "EXPLAIN QUERY PLAN SELECT * FROM dat_roms WHERE dat = 'psx.dat' AND serial = 'X'",
            [],
            |r| r.get(3),
        )
        .unwrap();
    assert!(plan.contains("dat_roms_serial"), "{plan}");
}

fn choice(system: &str, keeper: &str) -> StoredChoice {
    StoredChoice {
        system: system.into(),
        keeper_key: keeper.into(),
        signature: "sha1:AA,sha1:BB".into(),
        saves_fp: "none".into(),
        chosen_at: 1_700_000_000,
    }
}

#[test]
fn choices_round_trip_and_clear_by_system() {
    let cache = Cache::in_memory().unwrap();
    cache.put_dedupe_choice(&choice("gb", "sha1:AA")).unwrap();
    cache.put_dedupe_choice(&choice("gb", "sha1:CC")).unwrap();
    cache.put_dedupe_choice(&choice("psx", "sha1:AA")).unwrap();
    cache.put_dedupe_choice(&choice("snes", "sha1:DD")).unwrap();
    assert_eq!(
        cache.dedupe_choice("gb", "sha1:AA").unwrap(),
        Some(choice("gb", "sha1:AA"))
    );
    assert_eq!(cache.dedupe_choice("gba", "sha1:AA").unwrap(), None);
    // A new pick for the same content replaces the old one.
    let mut again = choice("gb", "sha1:AA");
    again.saves_fp = "srm:1234".into();
    cache.put_dedupe_choice(&again).unwrap();
    assert_eq!(cache.dedupe_choice("gb", "sha1:AA").unwrap(), Some(again));
    assert_eq!(cache.clear_dedupe_choices(&[]).unwrap(), 0);
    assert_eq!(
        cache
            .clear_dedupe_choices(&["gb".to_owned(), "psx".to_owned()])
            .unwrap(),
        3
    );
    assert_eq!(cache.dedupe_choice("gb", "sha1:AA").unwrap(), None);
    assert_eq!(cache.dedupe_choice("psx", "sha1:AA").unwrap(), None);
    assert!(cache.dedupe_choice("snes", "sha1:DD").unwrap().is_some());
}

#[test]
fn a_v3_database_opened_by_v2_code_is_refused_cleanly() {
    // Older code sees a newer `user_version` and sets the file aside; this
    // code does the same for the version after it, never touching its rows.
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join(CACHE_FILE);
    Cache::open(&path)
        .unwrap()
        .put_json("ns", "k", &1u8, None)
        .unwrap();
    rusqlite::Connection::open(&path)
        .unwrap()
        .pragma_update(None, "user_version", SCHEMA_VERSION + 1)
        .unwrap();
    let cache = Cache::open(&path).unwrap();
    assert!(cache.get_json::<u8>("ns", "k").unwrap().is_none());
    let names = dir_names(tmp.path());
    let aside: Vec<_> = names.iter().filter(|n| n.contains(".corrupt-")).collect();
    assert_eq!(aside.len(), 1, "{names:?}");
    assert_eq!(user_version(&tmp.path().join(aside[0])), SCHEMA_VERSION + 1);
}

#[test]
fn volume_uuid_is_stable_for_one_volume() {
    let tmp = TempDir::new().unwrap();
    let a = tmp.path().join("a");
    fs::create_dir(&a).unwrap();
    let one = coverly_core::volumes::volume_uuid(&a).unwrap();
    let two = coverly_core::volumes::volume_uuid(tmp.path()).unwrap();
    assert_eq!(one, two);
    assert!(!one.is_empty());
    assert!(coverly_core::volumes::volume_uuid(Path::new("/definitely/not/here")).is_none());
}
