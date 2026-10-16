//! Source credentials (spec §11, D30). Values live in a `CredentialStore`
//! (the OS keychain in the app, memory in tests); the UI only ever learns
//! whether a key is set.

use std::collections::BTreeMap;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CredentialKey {
    SteamgriddbApiKey,
    ScreenscraperUser,
    ScreenscraperPassword,
}

impl CredentialKey {
    pub const ALL: [CredentialKey; 3] = [
        CredentialKey::SteamgriddbApiKey,
        CredentialKey::ScreenscraperUser,
        CredentialKey::ScreenscraperPassword,
    ];

    /// Account name in the store (also the wire name).
    pub fn account(self) -> &'static str {
        match self {
            CredentialKey::SteamgriddbApiKey => "steamgriddb-api-key",
            CredentialKey::ScreenscraperUser => "screenscraper-user",
            CredentialKey::ScreenscraperPassword => "screenscraper-password",
        }
    }

    /// Only the exact wire names; anything else is refused.
    pub fn parse(name: &str) -> Option<CredentialKey> {
        CredentialKey::ALL.into_iter().find(|k| k.account() == name)
    }
}

/// Errors never carry a stored value: only what went wrong.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CredentialError {
    #[error("there is no credential store on this system")]
    Unavailable,
    #[error("the credential store is locked")]
    Locked,
    #[error("credential store error: {0}")]
    Failed(String),
    #[error("the value is too long or holds control characters")]
    BadValue,
}

pub trait CredentialStore: Send + Sync {
    fn available(&self) -> bool;
    fn get(&self, key: CredentialKey) -> Result<Option<String>, CredentialError>;
    fn set(&self, key: CredentialKey, value: &str) -> Result<(), CredentialError>;
    /// Already absent is success.
    fn clear(&self, key: CredentialKey) -> Result<(), CredentialError>;
}

pub const MAX_SECRET_CHARS: usize = 512;

/// Stores `value` trimmed; an empty value clears the key.
pub fn store_secret(
    store: &dyn CredentialStore,
    key: CredentialKey,
    value: &str,
) -> Result<(), CredentialError> {
    if !store.available() {
        return Err(CredentialError::Unavailable);
    }
    let v = value.trim();
    if v.is_empty() {
        return store.clear(key);
    }
    if v.chars().count() > MAX_SECRET_CHARS || v.chars().any(char::is_control) {
        return Err(CredentialError::BadValue);
    }
    store.set(key, v)
}

/// Whether the store works and which keys hold a value (never the values).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CredentialStatus {
    pub available: bool,
    pub set: BTreeMap<CredentialKey, bool>,
}

pub fn status(store: &dyn CredentialStore) -> CredentialStatus {
    let available = store.available();
    CredentialStatus {
        available,
        set: CredentialKey::ALL
            .into_iter()
            .map(|k| (k, available && matches!(store.get(k), Ok(Some(_)))))
            .collect(),
    }
}

/// Tests and the keychain-less fallback: values in memory.
#[derive(Default)]
pub struct MemoryStore {
    values: Mutex<BTreeMap<CredentialKey, String>>,
    unavailable: bool,
}

impl MemoryStore {
    /// A store that reports no credential store on this system.
    pub fn unavailable() -> MemoryStore {
        MemoryStore {
            unavailable: true,
            ..MemoryStore::default()
        }
    }

    fn values(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, BTreeMap<CredentialKey, String>>, CredentialError> {
        if self.unavailable {
            return Err(CredentialError::Unavailable);
        }
        self.values
            .lock()
            .map_err(|_| CredentialError::Failed("poisoned".into()))
    }
}

/// Lists which keys are set, never their values.
impl std::fmt::Debug for MemoryStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let keys: Vec<CredentialKey> = self
            .values
            .lock()
            .map(|v| v.keys().copied().collect())
            .unwrap_or_default();
        f.debug_struct("MemoryStore")
            .field("keys", &keys)
            .field("unavailable", &self.unavailable)
            .finish()
    }
}

impl CredentialStore for MemoryStore {
    fn available(&self) -> bool {
        !self.unavailable
    }
    fn get(&self, key: CredentialKey) -> Result<Option<String>, CredentialError> {
        Ok(self.values()?.get(&key).cloned())
    }
    fn set(&self, key: CredentialKey, value: &str) -> Result<(), CredentialError> {
        self.values()?.insert(key, value.to_string());
        Ok(())
    }
    fn clear(&self, key: CredentialKey) -> Result<(), CredentialError> {
        self.values()?.remove(&key);
        Ok(())
    }
}
