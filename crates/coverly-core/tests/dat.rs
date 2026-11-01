use coverly_core::cache::Cache;
use coverly_core::identify::dat::{
    dat_sources, encode_path, normalise_serial, parse_clrmamepro, DatStatus, DatStore,
};
use coverly_core::identify::hash::{Digests, HashOutcome, RomHashes};
use coverly_core::net::testing::{HangingServer, MockResponse, MockServer};
use coverly_core::net::{NetConfig, NetError, PoliteHttp, UreqHttp};
use coverly_core::systems::SystemDb;

const NES_DAT: &str = r#"clrmamepro (
	name "Nintendo - Nintendo Entertainment System"
	version "2026.10.07"
)

game (
	name "Golden Test (USA)"
	region "USA"
	rom ( name "Golden Test (USA).nes" size 25 crc 11111111 md5 AA sha1 BB )
)
game (
	name "Golden Test (USA)"
	rom ( name "Golden Test (USA).unh" size 9 crc CBF43926 md5 25F9E794323B453885F5181F1B624D0B sha1 F7C3BC1D808E04732ADF679965CCC34CA7AE3441 )
)
game (
	name "Quote \"Test\" (Japan)"
	rom ( name "q.nes" size 3 crc 22222222 )
)
"#;

const MAME_DAT: &str = "clrmamepro (\n\tname \"MAME\"\n\tversion 2017-02-14\n)\n\ngame (\n\tname \"10-Yard Fight (World, set 1)\"\n\tyear \"1983\"\n\trom ( name 10yard.zip size 62708 crc 8F401426 md5 040dc0827e184b8da9b91499f96c1fce sha1 8b15a2e54e9fdded3d61205bac6535ccc7b41271 )\n)\n";

fn http() -> PoliteHttp<UreqHttp> {
    let cfg = NetConfig {
        retries: 0,
        ..NetConfig::default()
    };
    PoliteHttp::new(UreqHttp::new(&cfg), &cfg)
}

fn digests(crc: &str, size: u64) -> Digests {
    Digests {
        crc32: crc.into(),
        md5: String::new(),
        sha1: String::new(),
        size,
    }
}

#[test]
fn parses_quoted_unquoted_and_escaped_values() {
    let games = parse_clrmamepro(NES_DAT).unwrap();
    assert_eq!(games.len(), 3);
    assert_eq!(games[1].roms[0].crc32.as_deref(), Some("CBF43926"));
    assert_eq!(games[1].roms[0].size, Some(9));
    assert_eq!(games[2].name, "Quote \"Test\" (Japan)");
    let mame = parse_clrmamepro(MAME_DAT).unwrap();
    assert_eq!(mame[0].roms[0].rom_name, "10yard.zip");
}

#[test]
fn unbalanced_dat_is_an_error_with_a_line() {
    let err = parse_clrmamepro("game (\n name \"x\"\n rom ( name y ").unwrap_err();
    assert!(err.to_string().contains("line"), "{err}");
}

#[test]
fn dat_sources_per_system() {
    let db = SystemDb::builtin();
    assert_eq!(
        dat_sources(db.get("gba").unwrap()),
        ["metadat/no-intro/Nintendo - Game Boy Advance.dat"]
    );
    assert_eq!(
        dat_sources(db.get("psx").unwrap()),
        ["metadat/redump/Sony - PlayStation.dat"]
    );
    assert_eq!(
        dat_sources(db.get("mame").unwrap()),
        ["metadat/mame/MAME.dat"]
    );
    assert_eq!(
        dat_sources(db.get("fbneo").unwrap()),
        ["metadat/fbneo-split/FBNeo - Arcade Games.dat"]
    );
    // libretro-database names these DATs differently from the thumbnail
    // folders (checked with HEAD requests against the live repository).
    assert_eq!(
        dat_sources(db.get("a800").unwrap()),
        ["metadat/no-intro/Atari - 8-bit Family.dat"]
    );
    assert_eq!(
        dat_sources(db.get("gw").unwrap()),
        ["dat/Handheld Electronic Game.dat"]
    );
    let custom = db
        .with_overrides("[[system]]\nid = \"gba\"\ndats = [\"dat/Custom.dat\"]\n")
        .unwrap();
    assert_eq!(dat_sources(custom.get("gba").unwrap()), ["dat/Custom.dat"]);
    assert_eq!(
        encode_path("metadat/no-intro/Nintendo - Game Boy Advance.dat"),
        "metadat/no-intro/Nintendo%20-%20Game%20Boy%20Advance.dat"
    );
}

#[test]
fn downloads_once_then_looks_up_headerless_and_zip_crcs() {
    let server = MockServer::start();
    let path = "metadat/no-intro/Nintendo - Nintendo Entertainment System.dat";
    server.serve_dat(path, NES_DAT);
    let (http, cache) = (http(), Cache::in_memory().unwrap());
    let store = DatStore::new(&http, &cache, server.base());
    let dats = vec![path.to_string()];
    assert_eq!(store.ensure(path).unwrap(), DatStatus::Ready);
    assert_eq!(store.ensure(path).unwrap(), DatStatus::Ready);
    assert_eq!(server.hits(&format!("/{}", encode_path(path))), 1);

    let headered = HashOutcome::Content(RomHashes {
        full: digests("99999999", 25),
        headerless: Some(digests("CBF43926", 9)),
        payload_sha1: None,
    });
    assert_eq!(
        store.lookup(&dats, &headered).unwrap().as_deref(),
        Some("Golden Test (USA)")
    );
    let zipped = HashOutcome::ZipEntry {
        crc32: "22222222".into(),
        size: 3,
        inner_name: "q.nes".into(),
        payload_sha1: None,
    };
    assert_eq!(
        store.lookup(&dats, &zipped).unwrap().as_deref(),
        Some("Quote \"Test\" (Japan)")
    );
    let wrong_size = HashOutcome::ZipEntry {
        crc32: "22222222".into(),
        size: 4,
        inner_name: "q.nes".into(),
        payload_sha1: None,
    };
    assert_eq!(store.lookup(&dats, &wrong_size).unwrap(), None);
    assert_eq!(store.lookup(&dats, &HashOutcome::NameOnly).unwrap(), None);
}

#[test]
fn a_sha1_mismatch_rejects_a_crc_collision() {
    let server = MockServer::start();
    let path = "metadat/no-intro/X.dat";
    server.serve_dat(path, NES_DAT);
    let (http, cache) = (http(), Cache::in_memory().unwrap());
    let store = DatStore::new(&http, &cache, server.base());
    let mut d = digests("CBF43926", 9);
    d.sha1 = "0000000000000000000000000000000000000000".into();
    let outcome = HashOutcome::Content(RomHashes {
        full: d,
        headerless: None,
        payload_sha1: None,
    });
    assert_eq!(store.lookup(&[path.to_string()], &outcome).unwrap(), None);
}

#[test]
fn missing_dat_is_remembered_and_arcade_names_resolve() {
    let server = MockServer::start();
    server.serve_dat("metadat/mame/MAME.dat", MAME_DAT);
    let (http, cache) = (http(), Cache::in_memory().unwrap());
    let store = DatStore::new(&http, &cache, server.base());
    assert_eq!(
        store.ensure("metadat/redump/Nope.dat").unwrap(),
        DatStatus::Missing
    );
    let fresh = DatStore::new(&http, &cache, server.base());
    assert_eq!(
        fresh.ensure("metadat/redump/Nope.dat").unwrap(),
        DatStatus::Missing
    );
    assert_eq!(
        server.hits("/metadat/redump/Nope.dat"),
        1,
        "404 is cached for the TTL"
    );
    let dats = vec!["metadat/mame/MAME.dat".to_string()];
    assert_eq!(
        store
            .arcade_description(&dats, "10YARD")
            .unwrap()
            .as_deref(),
        Some("10-Yard Fight (World, set 1)")
    );
    assert_eq!(store.arcade_description(&dats, "nothing").unwrap(), None);
}

#[test]
fn offline_with_stale_rows_uses_them() {
    let cache = Cache::in_memory().unwrap();
    let path = "metadat/no-intro/X.dat";
    {
        let server = MockServer::start();
        server.serve_dat(path, NES_DAT);
        let http = http();
        DatStore::new(&http, &cache, server.base())
            .ensure(path)
            .unwrap();
    } // server gone
    let http = http();
    let store = DatStore::new(&http, &cache, "http://127.0.0.1:9").with_ttl_secs(0);
    assert_eq!(store.ensure(path).unwrap(), DatStatus::Stale);
    let zipped = HashOutcome::ZipEntry {
        crc32: "22222222".into(),
        size: 3,
        inner_name: "q".into(),
        payload_sha1: None,
    };
    assert!(store
        .lookup(&[path.to_string()], &zipped)
        .unwrap()
        .is_some());
}

#[test]
fn oversized_dat_is_refused() {
    let server = MockServer::start();
    server.mock("/big.dat", MockResponse::ok(vec![b' '; 2048]));
    let (http, cache) = (http(), Cache::in_memory().unwrap());
    let store = DatStore::new(&http, &cache, server.base()).with_max_bytes(1024);
    assert!(store.ensure("big.dat").is_err());
}

const HTML: &str = "<!doctype html><html><body>Sign in to continue</body></html>";

/// Fills `cache` with `NES_DAT` rows for `path`, then drops the server.
fn seed(cache: &Cache, path: &str) {
    let server = MockServer::start();
    server.serve_dat(path, NES_DAT);
    let http = http();
    assert_eq!(
        DatStore::new(&http, cache, server.base())
            .ensure(path)
            .unwrap(),
        DatStatus::Ready
    );
}

#[test]
fn a_failed_refresh_is_remembered_for_the_process() {
    let zipped = HashOutcome::ZipEntry {
        crc32: "22222222".into(),
        size: 3,
        inner_name: "q".into(),
        payload_sha1: None,
    };
    for (name, resp) in [
        ("500", MockResponse::status(500)),
        ("html", MockResponse::ok(HTML.as_bytes().to_vec())),
        ("empty", MockResponse::ok(Vec::new())),
        ("big", MockResponse::ok(vec![b' '; 2048])),
    ] {
        let server = MockServer::start();
        let path = format!("metadat/no-intro/{name}.dat");
        server.mock(&format!("/{}", encode_path(&path)), resp);
        let (http, cache) = (http(), Cache::in_memory().unwrap());
        let store = DatStore::new(&http, &cache, server.base()).with_max_bytes(1024);
        let dats = vec![path.clone()];
        assert!(store.ensure(&path).is_err(), "{name}");
        assert!(store.lookup(&dats, &zipped).is_err(), "{name}");
        assert!(store.arcade_description(&dats, "x").is_err(), "{name}");
        assert_eq!(
            server.hits(&format!("/{}", encode_path(&path))),
            1,
            "{name}: one download per process"
        );
    }
}

#[test]
fn rate_limits_and_offline_are_not_remembered() {
    let server = MockServer::start();
    let path = "metadat/no-intro/X.dat";
    server.mock(
        &format!("/{}", encode_path(path)),
        MockResponse::status(429),
    );
    let (http, cache) = (http(), Cache::in_memory().unwrap());
    let store = DatStore::new(&http, &cache, server.base());
    assert!(store.ensure(path).is_err());
    server.serve_dat(path, NES_DAT);
    assert_eq!(
        store.ensure(path).unwrap(),
        DatStatus::Ready,
        "resume works"
    );
    assert_eq!(server.hits(&format!("/{}", encode_path(path))), 2);
}

#[test]
fn any_refresh_failure_with_cached_rows_is_stale() {
    let path = "metadat/no-intro/X.dat";
    for (name, resp) in [
        ("429", MockResponse::status(429)),
        ("500", MockResponse::status(500)),
        ("html", MockResponse::ok(HTML.as_bytes().to_vec())),
        ("empty", MockResponse::ok(Vec::new())),
        ("big", MockResponse::ok(vec![b' '; 2048])),
    ] {
        let cache = Cache::in_memory().unwrap();
        seed(&cache, path);
        let server = MockServer::start();
        server.mock(&format!("/{}", encode_path(path)), resp);
        let http = http();
        let store = DatStore::new(&http, &cache, server.base())
            .with_ttl_secs(0)
            .with_max_bytes(1024);
        assert_eq!(store.ensure(path).unwrap(), DatStatus::Stale, "{name}");
        assert_eq!(
            cache.dat_by_crc(path, "22222222").unwrap().len(),
            1,
            "{name}: old rows kept"
        );
    }
}

#[test]
fn an_unsafe_libretro_name_derives_no_dat() {
    for bad in ["../../x", "a/b", "a\\\\b", ".."] {
        let db = SystemDb::builtin()
            .with_overrides(&format!("[[system]]\nid = \"gba\"\nlibretro = \"{bad}\"\n"))
            .unwrap();
        assert!(dat_sources(db.get("gba").unwrap()).is_empty(), "{bad}");
    }
}

#[test]
fn a_timeout_is_not_remembered_so_the_job_can_pause_and_resume() {
    let hang = HangingServer::start();
    let cfg = NetConfig {
        retries: 0,
        timeout: std::time::Duration::from_millis(300),
        ..NetConfig::default()
    };
    let http = PoliteHttp::new(UreqHttp::new(&cfg), &cfg);
    let cache = Cache::in_memory().unwrap();
    let store = DatStore::new(&http, &cache, hang.base());
    let dat = "metadat/no-intro/Nintendo - Game Boy Advance.dat";
    for _ in 0..2 {
        let err = store.ensure(dat).unwrap_err();
        assert!(
            matches!(
                err,
                coverly_core::identify::dat::DatError::Net(NetError::Timeout)
            ),
            "{err}"
        );
    }
    assert_eq!(hang.connections(), 2);
}

const REDUMP_DAT: &str = r#"clrmamepro (
	name "Sony - PlayStation"
)

game (
	name "Crash Bash (USA)"
	region "USA"
	serial "SCUS-94570"
	rom ( name "Crash Bash (USA).cue" size 85 crc AAAAAAAA )
	rom ( name "Crash Bash (USA).bin" size 1234 crc BBBBBBBB serial "SCUS_945.71" )
)
game (
	name "Driver 2 (USA) (Disc 1)"
	rom ( name "Driver 2 (USA) (Disc 1).bin" size 99 crc CCCCCCCC serial "SLUS-01161" )
)
"#;

#[test]
fn parses_serial_from_rom_line_and_game_block() {
    let games = parse_clrmamepro(REDUMP_DAT).unwrap();
    // The game block's serial fills a ROM line without one.
    assert_eq!(games[0].roms[0].serial.as_deref(), Some("SCUS-94570"));
    // A ROM line's own serial wins and is normalised.
    assert_eq!(games[0].roms[1].serial.as_deref(), Some("SCUS-94571"));
    assert_eq!(games[1].roms[0].serial.as_deref(), Some("SLUS-01161"));
}

#[test]
fn parses_cloneof_and_region() {
    let dat = r#"game (
	name "Street Fighter II (USA)"
	region "USA"
	cloneof "Street Fighter II (World)"
	rom ( name "sf2u.sfc" size 4 crc 01234567 )
)
game (
	name "Street Fighter II (World)"
	region "World"
	rom ( name "sf2.sfc" size 4 crc 89ABCDEF )
)
"#;
    let games = parse_clrmamepro(dat).unwrap();
    assert_eq!(games[0].region.as_deref(), Some("USA"));
    assert_eq!(
        games[0].cloneof.as_deref(),
        Some("Street Fighter II (World)")
    );
    assert_eq!(games[1].region.as_deref(), Some("World"));
    assert_eq!(games[1].cloneof, None);
    assert!(!games[0].is_bios && !games[1].is_bios);
}

#[test]
fn isbios_yes_and_bios_name_mark_bios() {
    let dat = r#"game (
	name "neogeo"
	isbios "yes"
	rom ( name "sp-s2.sp1" size 4 crc 9036D879 )
)
game (
	name "[BIOS] PlayStation (USA) (v3.0)"
	rom ( name "scph5501.bin" size 4 crc 8D8CB7E4 )
)
game (
	name "mslug"
	isbios "no"
	rom ( name "201-p1.p1" size 4 crc 08D8DAA5 )
)
"#;
    let games = parse_clrmamepro(dat).unwrap();
    assert!(games[0].is_bios);
    assert!(games[1].is_bios);
    assert!(!games[2].is_bios);
}

#[test]
fn old_dats_without_fields_still_parse() {
    let games = parse_clrmamepro(MAME_DAT).unwrap();
    assert_eq!(games.len(), 1);
    assert_eq!(games[0].region, None);
    assert_eq!(games[0].cloneof, None);
    assert!(!games[0].is_bios);
    assert_eq!(games[0].roms[0].serial, None);
    let nes = parse_clrmamepro(NES_DAT).unwrap();
    assert_eq!(nes[0].region.as_deref(), Some("USA"));
    assert!(nes.iter().flat_map(|g| &g.roms).all(|r| r.serial.is_none()));
}

#[test]
fn serial_is_normalised() {
    assert_eq!(
        normalise_serial("SLUS_005.43").as_deref(),
        Some("SLUS-00543")
    );
    assert_eq!(
        normalise_serial("slus-00543").as_deref(),
        Some("SLUS-00543")
    );
    assert_eq!(normalise_serial("SLUS00543").as_deref(), Some("SLUS-00543"));
    assert_eq!(
        normalise_serial(" SCES_123.45 ").as_deref(),
        Some("SCES-12345")
    );
    assert_eq!(
        normalise_serial("SLUS-20001").as_deref(),
        Some("SLUS-20001")
    );
    // Sega product numbers stay as they are.
    assert_eq!(normalise_serial("T-5704H").as_deref(), Some("T-5704H"));
    assert_eq!(normalise_serial("MK-81009").as_deref(), Some("MK-81009"));
    assert_eq!(normalise_serial("HDR-0001").as_deref(), Some("HDR-0001"));
    assert_eq!(normalise_serial("mk-81009").as_deref(), Some("MK-81009"));
    // Garbage is not a serial.
    assert_eq!(normalise_serial(""), None);
    assert_eq!(normalise_serial("   "), None);
    assert_eq!(normalise_serial("cdrom:\\SLUS_005.43;1"), None);
}

#[test]
fn a_serial_list_keeps_its_first_real_serial() {
    let dat = r#"game (
	name "Compilation (USA)"
	serial "n/a, SLUS_011.61, SLUS-01318"
	rom ( name "Compilation (USA).bin" size 4 crc 01234567 )
)
"#;
    let games = parse_clrmamepro(dat).unwrap();
    assert_eq!(games[0].roms[0].serial.as_deref(), Some("SLUS-01161"));
}

fn fixture_dat(name: &str) -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/dat")
            .join(name),
    )
    .unwrap()
}

#[test]
fn parses_trimmed_tosec_and_dat_folder() {
    // Systems batch 1 (spec §14.1): a `metadat/tosec/` file and a `dat/`
    // file, trimmed from the structure of the files at 2c93952f (invented
    // games and checksums).
    let tosec = parse_clrmamepro(&fixture_dat("tosec-sample.dat")).unwrap();
    let rows: Vec<(&str, &str, Option<u64>, Option<&str>)> = tosec
        .iter()
        .flat_map(|g| {
            g.roms.iter().map(move |r| {
                (
                    g.name.as_str(),
                    r.rom_name.as_str(),
                    r.size,
                    r.crc32.as_deref(),
                )
            })
        })
        .collect();
    assert_eq!(
        rows,
        [
            (
                "Test Orbit",
                "Test Orbit (1986)(Coverly Test Soft).dsk",
                Some(194_816),
                Some("1A2B3C4D")
            ),
            (
                "Test Orbit",
                "Test Orbit (1986)(Coverly Test Soft)[a].dsk",
                Some(194_816),
                Some("5E6F7A8B")
            ),
            (
                "Sample Tape Run",
                "Sample Tape Run (1985)(Coverly Test Soft).cdt",
                Some(40_210),
                Some("9C8D7E6F")
            ),
        ]
    );
    let dat = parse_clrmamepro(&fixture_dat("dat-sample.dat")).unwrap();
    assert_eq!(dat.len(), 3);
    assert_eq!(dat[0].name, "Test Maze (1981)(Coverly Test Soft)");
    assert_eq!(dat[0].roms[0].crc32.as_deref(), Some("2468ACE0"));
    // The clean-cpc-db `dat/` file at 2c93952f writes some CRCs without
    // their leading zero (`611E83F`): padded to eight digits so they match.
    assert_eq!(dat[1].roms[0].crc32.as_deref(), Some("07A1B2C3"));
    // A two-sided game lists one rom per side.
    assert_eq!(dat[2].roms.len(), 2);
    assert_eq!(dat[2].roms[1].crc32.as_deref(), Some("FDB97531"));
}

#[test]
fn a800_keeps_its_dat() {
    // §8.3: `atarixe` lists only `a800`'s DAT and comes after it, so the
    // DAT maps to `a800` and a hash never names `atarixe`.
    let db = coverly_core::systems::SystemDb::builtin();
    let dat = "metadat/no-intro/Atari - 8-bit Family.dat";
    assert_eq!(db.system_for_dat(dat).map(|s| s.id.as_str()), Some("a800"));
    let xe = coverly_core::identify::dat::dat_sources(db.get("atarixe").unwrap());
    assert_eq!(xe, [dat]);
    for d in &xe {
        assert_ne!(
            db.system_for_dat(d).map(|s| s.id.as_str()),
            Some("atarixe"),
            "{d}"
        );
    }
}

#[test]
fn mame_dat_still_maps_to_mame() {
    // §8.3 / D16: `model2` and `model3` list only `MAME.dat` and come after
    // `mame`, so the tie keeps the DAT with `mame` and a hash never names
    // them; a Model 2/3 set found loose goes to `mame`.
    let db = SystemDb::builtin();
    let dat = "metadat/mame/MAME.dat";
    for id in ["model2", "model3"] {
        assert_eq!(dat_sources(db.get(id).unwrap()), [dat], "{id}");
    }
    assert_eq!(db.system_for_dat(dat).map(|s| s.id.as_str()), Some("mame"));
}

#[test]
fn naomi2_never_wins_mame_dat() {
    // `naomi2` lists two DATs, so the fewest-sources rule never picks it
    // for `MAME.dat`; its Redump DAT is its own.
    let db = SystemDb::builtin();
    let naomi2 = dat_sources(db.get("naomi2").unwrap());
    assert_eq!(
        naomi2,
        ["metadat/redump/Sega - Naomi 2.dat", "metadat/mame/MAME.dat"]
    );
    assert_ne!(
        db.system_for_dat("metadat/mame/MAME.dat")
            .map(|s| s.id.as_str()),
        Some("naomi2")
    );
    assert_eq!(
        db.system_for_dat("metadat/redump/Sega - Naomi 2.dat")
            .map(|s| s.id.as_str()),
        Some("naomi2")
    );
    // Triforce has no DAT: its folder claims it.
    assert!(dat_sources(db.get("triforce").unwrap()).is_empty());
}

// --- GameCube and Wii serials (systems batch 2, Task 7) ---

fn fixture(name: &str) -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/dat")
            .join(name),
    )
    .unwrap()
}

#[test]
fn redump_nintendo_serials_are_indexed_by_their_middle_key() {
    // Redump writes `RVL-RMGE-USA` and `DL-DOL-GW7P-EUR`; the disc header
    // names `RMGE01` and `GW7P69`, whose first four characters match.
    let games = parse_clrmamepro(&fixture("redump-wii.dat")).unwrap();
    assert_eq!(games[0].roms[0].serial.as_deref(), Some("RMGE"));
    assert_eq!(games[1].roms[0].serial.as_deref(), Some("GW7P"));
    // GameTDB keeps the ID6.
    let games = parse_clrmamepro(&fixture("gametdb-gamecube.dat")).unwrap();
    assert_eq!(games[0].roms[0].serial.as_deref(), Some("GM8E01"));
    // Other serials are untouched.
    let games = parse_clrmamepro(REDUMP_DAT).unwrap();
    assert_eq!(games[0].roms[0].serial.as_deref(), Some("SCUS-94570"));
}

#[test]
fn id6_finds_gametdb_and_redump_rows() {
    use coverly_core::dedupe::source::{CacheDats, DatLookup};
    let db = SystemDb::builtin();
    let cache = Cache::in_memory().unwrap();
    let server = MockServer::start();
    let gc = "dat/Nintendo - GameCube.dat";
    let wii = "metadat/redump/Nintendo - Wii.dat";
    server.serve_dat(gc, &fixture("gametdb-gamecube.dat"));
    server.serve_dat(wii, &fixture("redump-wii.dat"));
    let http = http();
    let store = DatStore::new(&http, &cache, server.base());
    assert_eq!(store.ensure(gc).unwrap(), DatStatus::Ready);
    assert_eq!(store.ensure(wii).unwrap(), DatStatus::Ready);
    let dats = CacheDats::new(&cache, &db);
    let games = |system: &str, serial: &str| -> Vec<String> {
        dats.by_serial(system, serial)
            .into_iter()
            .map(|h| h.game)
            .collect()
    };
    assert_eq!(
        games("gamecube", "GM8E01"),
        ["Test Game (USA)", "Test Game (USA) (Rev 1)"]
    );
    assert_eq!(games("wii", "RMGE01"), ["Test Wii Game (USA)"]);
    // The middle key is tried only for these two systems.
    assert!(games("psx", "RMGE01").is_empty());
}

#[test]
fn wiiu_id6_finds_retail_rows_and_its_id4_eshop_rows() {
    use coverly_core::dedupe::source::{CacheDats, DatLookup};
    let db = SystemDb::builtin();
    let cache = Cache::in_memory().unwrap();
    let server = MockServer::start();
    let wiiu = "dat/Nintendo - Wii U.dat";
    server.serve_dat(wiiu, &fixture("gametdb-wiiu.dat"));
    let http = http();
    let store = DatStore::new(&http, &cache, server.base());
    assert_eq!(store.ensure(wiiu).unwrap(), DatStatus::Ready);
    let dats = CacheDats::new(&cache, &db);
    let games = |serial: &str| -> Vec<String> {
        dats.by_serial("wiiu", serial)
            .into_iter()
            .map(|h| h.game)
            .collect()
    };
    // `meta.xml` gives `WUP-P-ATWE` and company `0001`: ID6 `ATWE01`.
    assert_eq!(games("ATWE01"), ["Test Wii U Disc (USA)"]);
    // GameTDB lists eShop titles by ID4 only.
    assert_eq!(games("SNKP01"), ["Test Wii U eShop Game (Europe)"]);
    assert_eq!(games("SNKP"), ["Test Wii U eShop Game (Europe)"]);
}

// --- Xbox 360 digital rows (systems batch 2, Task 12) ---

#[test]
fn xbox360_name_prefix_lookup() {
    use coverly_core::consoles::{testing as t, xbox};
    use coverly_core::dedupe::source::{CacheDats, DatLookup};
    use coverly_core::identify::dat::dat_by_name_prefix;
    let db = SystemDb::builtin();
    let cache = Cache::in_memory().unwrap();
    let server = MockServer::start();
    let digital = "metadat/no-intro/Microsoft - Xbox 360 (Digital).dat";
    server.serve_dat(digital, &fixture("nointro-x360-digital.dat"));
    let http = http();
    let store = DatStore::new(&http, &cache, server.base());
    assert_eq!(store.ensure(digital).unwrap(), DatStatus::Ready);
    // An XBLA package's title id and content type name its row.
    let head = t::stfs(b"LIVE", 0x5841_0889, 0x000D_0000, &[]);
    let mut f = t::SparseAt::new(head.len() as u64);
    f.put(0, &head);
    let len = f.len;
    let id = xbox::stfs(&mut coverly_core::consoles::Budgeted::new(&mut f, len)).unwrap();
    let prefix = xbox::dat_name_prefix(&id).unwrap();
    assert_eq!(prefix, "58410889000D0000");
    assert_eq!(
        dat_by_name_prefix(&cache, digital, &prefix),
        ["Test Arcade Game (World)"]
    );
    assert_eq!(
        dat_by_name_prefix(&cache, digital, "58410889000b0000"),
        ["Test Arcade Game (World) (Title Update)"]
    );
    // A shorter or non-hex key matches nothing.
    assert!(dat_by_name_prefix(&cache, digital, "58410889").is_empty());
    assert!(dat_by_name_prefix(&cache, digital, "58410889000D000Z").is_empty());
    assert_eq!(
        store
            .by_name_prefix(&[digital.to_string()], &prefix)
            .unwrap()
            .as_deref(),
        Some("Test Arcade Game (World)")
    );
    let dats = CacheDats::new(&cache, &db);
    let games: Vec<String> = dats
        .by_serial("xbox360", &prefix)
        .into_iter()
        .map(|h| h.game)
        .collect();
    assert_eq!(games, ["Test Arcade Game (World)"]);
}
