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

/// A secret value (API key, password). `Debug` never shows it and it has
/// no `Display` or `Serialize`, so it cannot reach a log, a journal or the
/// webview by accident; `expose` is the one way to read it.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Secret {
        Secret(value.into())
    }
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(***)")
    }
}

/// ScreenScraper developer credentials, baked in at build time from
/// `COVERLY_SS_DEVID` / `COVERLY_SS_DEVPASSWORD` (never in git).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DevCredentials {
    pub id: Secret,
    pub password: Secret,
}

impl DevCredentials {
    /// `None` = ScreenScraper is not available in this build.
    pub fn from_build() -> Option<DevCredentials> {
        DevCredentials::from_values(
            option_env!("COVERLY_SS_DEVID"),
            option_env!("COVERLY_SS_DEVPASSWORD"),
        )
    }

    /// Both values present and non-empty after trimming, else `None`.
    pub fn from_values(id: Option<&str>, password: Option<&str>) -> Option<DevCredentials> {
        let id = id.map(str::trim).filter(|s| !s.is_empty())?;
        let password = password.map(str::trim).filter(|s| !s.is_empty())?;
        Some(DevCredentials {
            id: Secret::new(id),
            password: Secret::new(password),
        })
    }
}

/// The user's own ScreenScraper account (keychain); sent only when both
/// are stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserCredentials {
    pub user: Secret,
    pub password: Secret,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_secret_never_prints_its_value() {
        let s = Secret::new("sk-123");
        assert_eq!(format!("{s:?}"), "Secret(***)");
        assert_eq!(s.expose(), "sk-123");
    }

    #[test]
    fn dev_credentials_need_both_values_and_never_print_them() {
        let d = DevCredentials::from_values(Some(" dev "), Some("pw")).unwrap();
        assert_eq!(d.id.expose(), "dev");
        assert_eq!(
            format!("{d:?}"),
            "DevCredentials { id: Secret(***), password: Secret(***) }"
        );
        assert_eq!(DevCredentials::from_values(Some(""), Some("pw")), None);
        assert_eq!(DevCredentials::from_values(None, Some("pw")), None);
    }
}
