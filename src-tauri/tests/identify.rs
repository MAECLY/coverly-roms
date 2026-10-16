mod common;

use common::{anbernic_tree, open, write};
use coverly_core::cache::Cache;
use coverly_core::identify::names::Region;
use coverly_core::identify::IdMethod;
use coverly_core::net::{Http, HttpRequest, HttpResponse, NetError};
use coverly_core::systems::SystemDb;
use coverly_roms_lib::error::IpcError;
use coverly_roms_lib::library::identify::identify_entry;

/// The CRC32/SHA1 of the bytes `123456789`.
const DAT: &str = "clrmamepro (\n\tname \"Test\"\n)\n\ngame (\n\tname \"Check Game (USA)\"\n\trom ( name \"Check Game (USA).gba\" size 9 crc CBF43926 sha1 F7C3BC1D808E04732ADF679965CCC34CA7AE3441 )\n)\n";

struct DatHttp;
impl Http for DatHttp {
    fn get(&self, _: &HttpRequest) -> Result<HttpResponse, NetError> {
        Ok(HttpResponse {
            status: 200,
            etag: None,
            last_modified: None,
            content_type: None,
            retry_after: None,
            body: DAT.as_bytes().to_vec(),
        })
    }
}

struct OfflineHttp;
impl Http for OfflineHttp {
    fn get(&self, _: &HttpRequest) -> Result<HttpResponse, NetError> {
        Err(NetError::Offline("test".into()))
    }
}

const BASE: &str = "https://example.invalid/db";

fn library() -> (tempfile::TempDir, coverly_roms_lib::library::Session) {
    let tree = anbernic_tree(&["GBA"], 0, 0);
    write(tree.path(), "ROMS/GBA/Some Dump (USA).gba", b"123456789");
    let (session, _) = open(tree.path(), None).unwrap();
    (tree, session)
}

#[test]
fn identifies_by_hash_when_the_dat_is_reachable() {
    let (_tree, session) = library();
    let cache = Cache::in_memory().unwrap();
    let id = identify_entry(&session, 0, &SystemDb::builtin(), &cache, &DatHttp, BASE).unwrap();
    assert_eq!(id.canonical.as_deref(), Some("Check Game (USA)"));
    assert_eq!(id.method, IdMethod::Hash);
    assert_eq!(id.confidence, 1.0);
    assert_eq!(id.regions, vec![Region::Usa]);
    assert!(!id.offline);
}

#[test]
fn offline_without_cached_dats_falls_back_to_the_name() {
    let (_tree, session) = library();
    let cache = Cache::in_memory().unwrap();
    let id = identify_entry(
        &session,
        0,
        &SystemDb::builtin(),
        &cache,
        &OfflineHttp,
        BASE,
    )
    .unwrap();
    assert_eq!(id.canonical, None);
    assert_eq!(id.method, IdMethod::Name);
    assert_eq!(id.confidence, 0.0);
    assert_eq!(id.title, "Some Dump");
    assert_eq!(id.regions, vec![Region::Usa]);
    assert!(id.offline);
}

#[test]
fn offline_with_a_cached_dat_still_identifies_by_hash() {
    let (_tree, session) = library();
    let cache = Cache::in_memory().unwrap();
    let db = SystemDb::builtin();
    identify_entry(&session, 0, &db, &cache, &DatHttp, BASE).unwrap();
    let id = identify_entry(&session, 0, &db, &cache, &OfflineHttp, BASE).unwrap();
    assert_eq!(id.method, IdMethod::Hash);
    assert!(!id.offline);
}

#[test]
fn unknown_entry_is_rejected() {
    let (_tree, session) = library();
    let cache = Cache::in_memory().unwrap();
    assert_eq!(
        identify_entry(&session, 42, &SystemDb::builtin(), &cache, &DatHttp, BASE).unwrap_err(),
        IpcError::UnknownEntry(42)
    );
}
