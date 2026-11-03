//! Persisted settings (spec §9): one struct per group with the spec
//! defaults, stored as a versioned JSON file (`settings.json`) in the app
//! config dir (see [`crate::paths::config_dir`]). Loading never fails and
//! never loses a file: unknown fields are ignored, a bad field falls back
//! to its default, a file that cannot be read as settings is set aside,
//! and a copy is kept whenever loading changed something (D24).

mod apply;
mod load;
mod model;
mod patch;
mod validate;

pub use load::{
    backup_settings, load_settings, read_settings, read_settings_checked, save_settings,
    LoadNotice, LoadReport, ReadProblem, LEGACY_LIBRARY_FILE, MAX_SETTINGS_BYTES, SETTINGS_FILE,
};
pub use model::*;
pub use validate::{FieldError, FieldErrorCode, Range, RANGES};

#[cfg(test)]
mod no_secrets_tests {
    use super::*;
    use crate::credentials::{CredentialKey, CredentialStore, MemoryStore};

    /// Settings (the file, the export and the import) never carry a
    /// credential (integrations spec D17): with every key stored and every
    /// optional setting filled, neither a value nor a keychain name shows.
    #[test]
    fn a_fully_populated_settings_file_carries_no_secret() {
        let store = MemoryStore::default();
        for key in CredentialKey::ALL {
            store.set(key, "sk-LEAK").unwrap();
        }
        let mut s = Settings::default();
        s.library.default_profile = Some("anbernic-stock".into());
        s.library.recent.push(RecentLibrary {
            path: "/Volumes/SD".into(),
            name: "SD".into(),
            profile_id: "anbernic-stock".into(),
            roms: 3,
            opened_at: 1,
        });
        s.safety.backup_dir = Some("/tmp/backup".into());
        s.advanced.custom_dir = Some("/tmp/profiles".into());
        s.dedupe.quarantine_dir = Some("/tmp/quarantine".into());
        s.dedupe.quarantine_name = Some("Duplicados".into());
        s.network.proxy = ProxyChoice::Manual;
        // The one exempt field: a proxy URL may hold `user:pass@` the user
        // typed into settings themselves (owner decision §12.5).
        s.network.proxy_url = "https://proxy.example:8080".into();
        let json = serde_json::to_string(&s).unwrap();
        assert!(!json.contains("sk-LEAK"));
        for key in CredentialKey::ALL {
            assert!(!json.contains(key.account()), "{}", key.account());
        }
    }
}
