//! Credentials (spec §11, D30): a closed set of keys, values never shown.

use coverly_core::credentials::*;

#[test]
fn keys_are_a_closed_set() {
    assert_eq!(
        CredentialKey::parse("steamgriddb-api-key"),
        Some(CredentialKey::SteamgriddbApiKey)
    );
    assert_eq!(
        CredentialKey::parse("screenscraper-user"),
        Some(CredentialKey::ScreenscraperUser)
    );
    assert_eq!(
        CredentialKey::parse("screenscraper-password"),
        Some(CredentialKey::ScreenscraperPassword)
    );
    for bad in ["", "github", "../x", "STEAMGRIDDB-API-KEY"] {
        assert_eq!(CredentialKey::parse(bad), None, "{bad}");
    }
    let accounts: std::collections::BTreeSet<_> =
        CredentialKey::ALL.iter().map(|k| k.account()).collect();
    assert_eq!(accounts.len(), 3);
}

#[test]
fn memory_store_sets_clears_and_reports_presence_only() {
    let store = MemoryStore::default();
    store_secret(&store, CredentialKey::SteamgriddbApiKey, "  sk-123  ").unwrap();
    assert_eq!(
        store
            .get(CredentialKey::SteamgriddbApiKey)
            .unwrap()
            .as_deref(),
        Some("sk-123")
    );
    let st = status(&store);
    assert!(st.available);
    assert!(st.set[&CredentialKey::SteamgriddbApiKey]);
    assert!(!st.set[&CredentialKey::ScreenscraperUser]);
    let json = serde_json::to_string(&st).unwrap();
    assert!(!json.contains("sk-123"), "{json}");
    assert!(json.contains("\"steamgriddb-api-key\":true"), "{json}");
    assert!(!format!("{store:?}").contains("sk-123"));
    store_secret(&store, CredentialKey::SteamgriddbApiKey, "   ").unwrap();
    assert_eq!(
        store.get(CredentialKey::SteamgriddbApiKey).unwrap(),
        None,
        "empty clears"
    );
    store.clear(CredentialKey::SteamgriddbApiKey).unwrap();
}

#[test]
fn bad_values_are_refused() {
    let store = MemoryStore::default();
    for bad in ["a\u{0}b".to_string(), "x".repeat(MAX_SECRET_CHARS + 1)] {
        assert_eq!(
            store_secret(&store, CredentialKey::ScreenscraperPassword, &bad),
            Err(CredentialError::BadValue)
        );
    }
}

#[test]
fn an_unavailable_store_says_so() {
    let store = MemoryStore::unavailable();
    let st = status(&store);
    assert!(!st.available);
    assert!(st.set.values().all(|v| !v));
    assert_eq!(
        store_secret(&store, CredentialKey::ScreenscraperUser, "me"),
        Err(CredentialError::Unavailable)
    );
}
