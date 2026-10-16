//! Source credentials (spec §11, D30). Values live in a `CredentialStore`
//! (the OS keychain in the app, memory in tests); the UI only ever learns
//! whether a key is set.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
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
    /// How many times a value was asked for (tests count keychain trips).
    reads: AtomicUsize,
}

impl MemoryStore {
    /// A store that reports no credential store on this system.
    pub fn unavailable() -> MemoryStore {
        MemoryStore {
            unavailable: true,
            ..MemoryStore::default()
        }
    }

    /// How many `get` calls this store answered (or refused).
    pub fn reads(&self) -> usize {
        self.reads.load(Ordering::Relaxed)
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
        self.reads.fetch_add(1, Ordering::Relaxed);
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

/// Environment variables the CLI reads keys from (it has no keychain).
pub const SGDB_KEY_ENV: &str = "COVERLY_SGDB_KEY";
pub const SS_USER_ENV: &str = "COVERLY_SS_USER";
pub const SS_PASSWORD_ENV: &str = "COVERLY_SS_PASSWORD";

/// Every credential a fill may use, read once when the job starts (the
/// app: keychain; the CLI: environment). Workers never touch the keychain.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceKeys {
    pub steamgriddb: Option<Secret>,
    pub screenscraper_user: Option<UserCredentials>,
    pub screenscraper_dev: Option<DevCredentials>,
}

/// A stored or typed value worth sending: trimmed, not empty, not too
/// long, no control characters (anything else counts as not set).
fn usable(v: Option<String>) -> Option<Secret> {
    let v = v?;
    let v = v.trim();
    (!v.is_empty() && v.chars().count() <= MAX_SECRET_CHARS && !v.chars().any(char::is_control))
        .then(|| Secret::new(v))
}

impl SourceKeys {
    /// The keys in `store`; a store that fails counts as empty.
    pub fn from_store(store: &dyn CredentialStore) -> SourceKeys {
        SourceKeys::read_store(store).0
    }

    /// [`Self::from_store`] and whether every read succeeded (a locked or
    /// failing store gives empty keys that are not worth remembering).
    pub fn read_store(store: &dyn CredentialStore) -> (SourceKeys, bool) {
        let mut complete = true;
        let mut get = |k| match store.get(k) {
            Ok(v) => usable(v),
            Err(_) => {
                complete = false;
                None
            }
        };
        let sgdb = get(CredentialKey::SteamgriddbApiKey);
        let user = get(CredentialKey::ScreenscraperUser);
        let password = get(CredentialKey::ScreenscraperPassword);
        (SourceKeys::assemble(sgdb, user, password), complete)
    }

    /// The keys in the environment (`lookup` is `std::env::var` in the CLI).
    pub fn from_env(lookup: impl Fn(&str) -> Option<String>) -> SourceKeys {
        SourceKeys::assemble(
            usable(lookup(SGDB_KEY_ENV)),
            usable(lookup(SS_USER_ENV)),
            usable(lookup(SS_PASSWORD_ENV)),
        )
    }

    fn assemble(
        sgdb: Option<Secret>,
        user: Option<Secret>,
        password: Option<Secret>,
    ) -> SourceKeys {
        SourceKeys {
            steamgriddb: sgdb,
            screenscraper_user: match (user, password) {
                (Some(user), Some(password)) => Some(UserCredentials { user, password }),
                _ => None,
            },
            screenscraper_dev: DevCredentials::from_build(),
        }
    }
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

    #[test]
    fn source_keys_come_from_the_store_or_the_environment_without_blanks() {
        let store = MemoryStore::default();
        store.set(CredentialKey::SteamgriddbApiKey, "sk").unwrap();
        store.set(CredentialKey::ScreenscraperUser, "me").unwrap();
        let k = SourceKeys::from_store(&store);
        assert_eq!(k.steamgriddb, Some(Secret::new("sk")));
        assert_eq!(
            k.screenscraper_user, None,
            "user without password is not an account"
        );
        assert_eq!(
            SourceKeys::from_store(&MemoryStore::unavailable()).steamgriddb,
            None
        );
        let env = |n: &str| match n {
            "COVERLY_SGDB_KEY" => Some("  sk2 ".to_string()),
            "COVERLY_SS_USER" => Some("".into()),
            _ => None,
        };
        let k = SourceKeys::from_env(env);
        assert_eq!(k.steamgriddb, Some(Secret::new("sk2")));
        assert_eq!(k.screenscraper_user, None);
        assert!(!format!("{k:?}").contains("sk2"));
    }

    #[test]
    fn a_full_account_is_kept_and_odd_values_are_dropped() {
        let env = |n: &str| match n {
            SS_USER_ENV => Some("me".to_string()),
            SS_PASSWORD_ENV => Some("pw".to_string()),
            SGDB_KEY_ENV => Some("bad\nkey".to_string()),
            _ => None,
        };
        let k = SourceKeys::from_env(env);
        assert_eq!(
            k.steamgriddb, None,
            "a key with a control character is dropped"
        );
        let user = k.screenscraper_user.clone().unwrap();
        assert_eq!((user.user.expose(), user.password.expose()), ("me", "pw"));
        assert!(!format!("{k:?}").contains("pw"));
    }
}
