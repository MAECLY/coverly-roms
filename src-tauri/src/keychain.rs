//! The OS credential store (macOS Keychain, Windows Credential Manager,
//! Secret Service) behind `CredentialStore`. Values never leave Rust.
//!
//! Every call blocks (a Keychain prompt blocks for as long as somebody looks
//! at it), so callers run on the blocking pool. Pattern from Versorium's
//! `secrets` module.

use coverly_core::credentials::{CredentialError, CredentialKey, CredentialStore};

/// The bundle identifier: entries are recognisable in Keychain Access.
pub const SERVICE: &str = "com.maecly.coverlyroms";

pub struct KeychainStore {
    service: String,
}

impl KeychainStore {
    pub fn new() -> KeychainStore {
        KeychainStore::with_service(SERVICE)
    }

    pub fn with_service(service: &str) -> KeychainStore {
        KeychainStore {
            service: service.to_string(),
        }
    }

    fn entry(&self, key: CredentialKey) -> Result<keyring::Entry, CredentialError> {
        keyring::Entry::new(&self.service, key.account()).map_err(|e| classify(&e))
    }
}

impl Default for KeychainStore {
    fn default() -> Self {
        KeychainStore::new()
    }
}

/// Maps a keyring error to a secret-free `CredentialError`.
pub(crate) fn classify(e: &keyring::Error) -> CredentialError {
    match e {
        keyring::Error::NoStorageAccess(_) => CredentialError::Locked,
        keyring::Error::NoDefaultStore => CredentialError::Unavailable,
        // `BadEncoding` and `BadDataFormat` carry the stored bytes (a secret):
        // never format them, and never use `{:?}` on a keyring error anywhere
        // (logs too).
        keyring::Error::BadEncoding(_) | keyring::Error::BadDataFormat(..) => {
            CredentialError::Failed("the stored value is not text".into())
        }
        other => CredentialError::Failed(other.to_string()),
    }
}

impl CredentialStore for KeychainStore {
    fn available(&self) -> bool {
        keyring::Entry::store_status().is_ok()
    }
    fn get(&self, key: CredentialKey) -> Result<Option<String>, CredentialError> {
        match self.entry(key)?.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(classify(&e)),
        }
    }
    fn set(&self, key: CredentialKey, value: &str) -> Result<(), CredentialError> {
        self.entry(key)?
            .set_password(value)
            .map_err(|e| classify(&e))
    }
    fn clear(&self, key: CredentialKey) -> Result<(), CredentialError> {
        match self.entry(key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(classify(&e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_are_classified() {
        assert_eq!(
            classify(&keyring::Error::NoDefaultStore),
            CredentialError::Unavailable
        );
        assert_eq!(
            classify(&keyring::Error::NoStorageAccess(Box::new(
                std::io::Error::other("locked")
            ))),
            CredentialError::Locked
        );
        assert!(matches!(
            classify(&keyring::Error::PlatformFailure(Box::new(
                std::io::Error::other("boom")
            ))),
            CredentialError::Failed(_)
        ));
        let leaked = classify(&keyring::Error::BadEncoding(b"sk-SECRET".to_vec()));
        assert!(!format!("{leaked} {leaked:?}").contains("SECRET"));
        let leaked = classify(&keyring::Error::BadDataFormat(
            b"sk-SECRET".to_vec(),
            Box::new(std::io::Error::other("bad")),
        ));
        assert!(!format!("{leaked} {leaked:?}").contains("SECRET"));
    }

    /// Touches the real OS keychain: run by hand only.
    #[test]
    #[ignore = "real OS keychain"]
    fn real_keychain_round_trip() {
        let store = KeychainStore::with_service("com.maecly.coverlyroms.test");
        coverly_core::credentials::store_secret(
            &store,
            CredentialKey::ScreenscraperUser,
            "m5-test",
        )
        .unwrap();
        assert_eq!(
            store
                .get(CredentialKey::ScreenscraperUser)
                .unwrap()
                .as_deref(),
            Some("m5-test")
        );
        store.clear(CredentialKey::ScreenscraperUser).unwrap();
        assert_eq!(store.get(CredentialKey::ScreenscraperUser).unwrap(), None);
    }
}
