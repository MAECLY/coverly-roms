mod common;

use std::sync::Arc;

use coverly_core::credentials::{CredentialKey, CredentialStore, MemoryStore};
use coverly_core::net::testing::{MockResponse, MockServer};
use coverly_core::sources::steamgriddb::search_path;
use coverly_core::sources::KeyCheck;
use coverly_roms_lib::jobs::FillConfig;
use coverly_roms_lib::state::AppState;

fn state(server: &MockServer, store: Arc<MemoryStore>) -> (AppState, tempfile::TempDir) {
    let dir = tempfile::TempDir::new().unwrap();
    let mut base = FillConfig::new(dir.path().join("cache"), dir.path().join("data"));
    base.endpoints.steamgriddb = server.url("/sgdb");
    base.net.retries = 0;
    (
        AppState::new(dir.path().join("config"), base).with_credentials(store),
        dir,
    )
}

#[test]
fn sources_info_tells_what_this_build_has_and_what_is_ready() {
    let server = MockServer::start();
    let store = Arc::new(MemoryStore::default());
    let (st, _d) = state(&server, store.clone());
    let info = st.sources_info();
    let get = |id: &str| info.iter().find(|i| i.id == id).unwrap();
    assert!(get("libretro").built && get("libretro").ready);
    assert!(get("steamgriddb").built && !get("steamgriddb").ready);
    assert_eq!(get("steamgriddb").needs, [CredentialKey::SteamgriddbApiKey]);
    st.set_credential("steamgriddb-api-key", "sk").unwrap();
    assert!(
        st.sources_info()
            .iter()
            .find(|i| i.id == "steamgriddb")
            .unwrap()
            .ready
    );
    let ss = st
        .sources_info()
        .into_iter()
        .find(|i| i.id == "screenscraper")
        .unwrap();
    assert_eq!(
        ss.built,
        coverly_core::credentials::DevCredentials::from_build().is_some()
    );
}

#[test]
fn test_source_key_never_returns_the_key() {
    let server = MockServer::start();
    let store = Arc::new(MemoryStore::default());
    let (st, _d) = state(&server, store.clone());
    assert_eq!(
        st.test_source_key("steamgriddb").unwrap(),
        KeyCheck::Missing
    );
    st.set_credential("steamgriddb-api-key", "sk-SECRET")
        .unwrap();
    server.serve_json(
        &format!("/sgdb{}", search_path("mario")),
        r#"{"success":true,"data":[]}"#,
    );
    let ok = st.test_source_key("steamgriddb").unwrap();
    assert_eq!(ok, KeyCheck::Ok);
    assert!(!serde_json::to_string(&ok).unwrap().contains("sk-SECRET"));
    assert_eq!(
        server.requests()[0].header("Authorization"),
        Some("Bearer sk-SECRET")
    );
    server.mock(
        &format!("/sgdb{}", search_path("mario")),
        MockResponse::status(401),
    );
    assert_eq!(
        st.test_source_key("steamgriddb").unwrap(),
        KeyCheck::Invalid
    );
    assert!(st.test_source_key("libretro").is_err());
    assert!(st.test_source_key("nope").is_err());
}

#[test]
fn an_unreachable_server_reads_as_offline() {
    let server = MockServer::start();
    let store = Arc::new(MemoryStore::default());
    store.set(CredentialKey::SteamgriddbApiKey, "sk").unwrap();
    let dir = tempfile::TempDir::new().unwrap();
    let mut base = FillConfig::new(dir.path().join("cache"), dir.path().join("data"));
    base.endpoints.steamgriddb = "http://127.0.0.1:1".into();
    base.net.retries = 0;
    let st = AppState::new(dir.path().join("config"), base).with_credentials(store);
    assert_eq!(
        st.test_source_key("steamgriddb").unwrap(),
        KeyCheck::Offline
    );
    drop(server);
}

#[test]
fn a_job_snapshots_the_keys_it_started_with() {
    let server = MockServer::start();
    let store = Arc::new(MemoryStore::default());
    store.set(CredentialKey::SteamgriddbApiKey, "sk").unwrap();
    let (st, _d) = state(&server, store.clone());
    let cfg = st.fill_config();
    store.clear(CredentialKey::SteamgriddbApiKey).unwrap();
    assert!(cfg.keys.steamgriddb.is_some());
    assert!(!format!("{cfg:?}").contains("\"sk\""));
}

fn sgdb_ready(st: &AppState) -> bool {
    st.sources_info()
        .into_iter()
        .find(|i| i.id == "steamgriddb")
        .unwrap()
        .ready
}

#[test]
fn a_stored_value_the_fill_would_not_send_is_not_ready() {
    let server = MockServer::start();
    for bad in ["   ".to_string(), "k".repeat(600), "a\u{7}b".to_string()] {
        let store = Arc::new(MemoryStore::default());
        store.set(CredentialKey::SteamgriddbApiKey, &bad).unwrap();
        let (st, _d) = state(&server, store);
        assert!(!sgdb_ready(&st));
        assert!(st.fill_config().keys.steamgriddb.is_none());
        assert_eq!(
            st.test_source_key("steamgriddb").unwrap(),
            KeyCheck::Missing
        );
    }
    assert!(server.requests().is_empty());
}

#[test]
fn the_keychain_is_read_once_until_a_credential_changes() {
    let server = MockServer::start();
    let store = Arc::new(MemoryStore::default());
    let (st, _d) = state(&server, store.clone());
    let per_read = CredentialKey::ALL.len();
    // Thumbnails, history and the sources list all take the same snapshot.
    for _ in 0..20 {
        let _ = st.fill_config();
    }
    let _ = st.sources_info();
    assert_eq!(store.reads(), per_read);
    st.set_credential("steamgriddb-api-key", "sk").unwrap();
    let after_save = store.reads();
    assert!(st.fill_config().keys.steamgriddb.is_some());
    assert!(sgdb_ready(&st));
    let _ = st.fill_config();
    assert_eq!(store.reads(), after_save + per_read);
    st.clear_credential("steamgriddb-api-key").unwrap();
    assert!(st.fill_config().keys.steamgriddb.is_none());
    assert!(!sgdb_ready(&st));
}

#[test]
fn a_failing_keychain_read_is_not_remembered() {
    let server = MockServer::start();
    let store = Arc::new(MemoryStore::unavailable());
    let (st, _d) = state(&server, store.clone());
    let _ = st.fill_config();
    let first = store.reads();
    assert!(first > 0);
    let _ = st.fill_config();
    assert_eq!(
        store.reads(),
        2 * first,
        "asked again: it may be unlocked now"
    );
}
