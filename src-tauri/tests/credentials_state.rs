use std::sync::Arc;

use coverly_core::credentials::{CredentialKey, MemoryStore};
use coverly_roms_lib::error::IpcError;
use coverly_roms_lib::jobs::FillConfig;
use coverly_roms_lib::state::AppState;
use tempfile::TempDir;

fn state(dir: &TempDir, store: MemoryStore) -> AppState {
    AppState::new(
        dir.path().join("config"),
        FillConfig::new(dir.path().join("cache"), dir.path().join("data")),
    )
    .with_credentials(Arc::new(store))
}

#[test]
fn credential_values_never_come_back() {
    let dir = TempDir::new().unwrap();
    let st = state(&dir, MemoryStore::default());
    let status = st
        .set_credential("steamgriddb-api-key", "sk-SECRET-123")
        .unwrap();
    assert!(status.available && status.set[&CredentialKey::SteamgriddbApiKey]);
    let wire = serde_json::to_string(&status).unwrap();
    assert!(!wire.contains("SECRET"));
    assert!(!serde_json::to_string(&st.settings_view())
        .unwrap()
        .contains("SECRET"));
    st.update_settings(&serde_json::json!({"general": {"theme": "dark"}}))
        .unwrap();
    let file = std::fs::read_to_string(dir.path().join("config/settings.json")).unwrap();
    assert!(!file.contains("SECRET"), "never in the settings file");
    let cleared = st.clear_credential("steamgriddb-api-key").unwrap();
    assert!(!cleared.set[&CredentialKey::SteamgriddbApiKey]);
}

#[test]
fn unknown_keys_bad_values_and_a_missing_keychain_are_errors() {
    let dir = TempDir::new().unwrap();
    let st = state(&dir, MemoryStore::default());
    assert!(matches!(
        st.set_credential("github-token", "x"),
        Err(IpcError::BadRequest(_))
    ));
    assert!(matches!(
        st.set_credential("screenscraper-user", "a\u{0}b"),
        Err(IpcError::BadRequest(_))
    ));
    let none = state(&TempDir::new().unwrap(), MemoryStore::unavailable());
    assert!(!none.credential_status().available);
    assert_eq!(
        none.set_credential("screenscraper-user", "me").unwrap_err(),
        IpcError::KeychainUnavailable
    );
    assert_eq!(
        serde_json::to_value(IpcError::KeychainLocked).unwrap(),
        serde_json::json!({"code": "keychain-locked"})
    );
}
