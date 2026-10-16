use coverly_core::cache::Cache;
use coverly_core::identify::dat::{
    dat_sources, encode_path, parse_clrmamepro, DatStatus, DatStore,
};
use coverly_core::identify::hash::{Digests, HashOutcome, RomHashes};
use coverly_core::net::testing::{MockResponse, MockServer};
use coverly_core::net::{NetConfig, PoliteHttp, UreqHttp};
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
    });
    assert_eq!(
        store.lookup(&dats, &headered).unwrap().as_deref(),
        Some("Golden Test (USA)")
    );
    let zipped = HashOutcome::ZipEntry {
        crc32: "22222222".into(),
        size: 3,
        inner_name: "q.nes".into(),
    };
    assert_eq!(
        store.lookup(&dats, &zipped).unwrap().as_deref(),
        Some("Quote \"Test\" (Japan)")
    );
    let wrong_size = HashOutcome::ZipEntry {
        crc32: "22222222".into(),
        size: 4,
        inner_name: "q.nes".into(),
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
