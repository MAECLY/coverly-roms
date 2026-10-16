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
    store.set(CredentialKey::SteamgriddbApiKey, "sk").unwrap();
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
    store
        .set(CredentialKey::SteamgriddbApiKey, "sk-SECRET")
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
