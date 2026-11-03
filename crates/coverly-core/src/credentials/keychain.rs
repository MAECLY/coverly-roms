//! The OS credential store (macOS Keychain, Windows Credential Manager,
//! Secret Service) behind `CredentialStore`. Values never leave Rust.
//!
//! Every call blocks (a Keychain prompt blocks for as long as somebody looks
//! at it), so callers run on the blocking pool. Pattern from Versorium's
//! `secrets` module.

use super::{CredentialError, CredentialKey, CredentialStore};

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

/// Maps a keyring error to a secret-free `CredentialError`, deciding by the
/// platform's error code (never by message text).
pub fn classify(e: &keyring::Error) -> CredentialError {
    match e {
        keyring::Error::NoDefaultStore => CredentialError::Unavailable,
        // `BadEncoding` and `BadDataFormat` carry the stored bytes (a secret):
        // never format them, and never use `{:?}` on a keyring error anywhere
        // (logs too).
        keyring::Error::BadEncoding(_) | keyring::Error::BadDataFormat(..) => {
            CredentialError::Failed("the stored value is not text".into())
        }
        keyring::Error::NoStorageAccess(b) => {
            by_code(b.as_ref()).unwrap_or(CredentialError::Locked)
        }
        keyring::Error::PlatformFailure(b) => {
            by_code(b.as_ref()).unwrap_or_else(|| CredentialError::Failed(b.to_string()))
        }
        other => CredentialError::Failed(other.to_string()),
    }
}

type PlatformErr = dyn std::error::Error + Send + Sync + 'static;

/// macOS: the `OSStatus` of a Security framework error.
#[cfg(target_os = "macos")]
fn by_code(e: &PlatformErr) -> Option<CredentialError> {
    let code = e.downcast_ref::<security_framework::base::Error>()?.code();
    Some(match code {
        // errSecInteractionNotAllowed: the keychain is locked.
        -25308 => CredentialError::Locked,
        // userCanceledErr, errSecAuthFailed, write permission, errSecReadOnly,
        // errSecInvalidOwnerEdit.
        -128 | -25293 | -61 | -25292 | -25244 => CredentialError::Denied,
        // errSecNotAvailable, errSecNoSuchKeychain, errSecInvalidKeychain.
        -25291 | -25294 | -25295 => CredentialError::Unavailable,
        _ => return None,
    })
}

/// Windows: the Win32 error code. `windows-native-keyring-store` 1.1.0 boxes
/// it as `PlatformError(u32)` in a private module, so the type cannot be
/// named; the code is read from that tuple struct's derived `Debug` form
/// (a number, never a secret).
#[cfg(windows)]
fn by_code(e: &PlatformErr) -> Option<CredentialError> {
    const ERROR_ACCESS_DENIED: u32 = 5;
    const ERROR_NO_SUCH_LOGON_SESSION: u32 = 1312;
    let debug = format!("{e:?}");
    let code: u32 = debug
        .strip_prefix("PlatformError(")?
        .strip_suffix(')')?
        .parse()
        .ok()?;
    Some(match code {
        ERROR_NO_SUCH_LOGON_SESSION => CredentialError::Unavailable,
        ERROR_ACCESS_DENIED => CredentialError::Denied,
        _ => return None,
    })
}

/// Linux: the Secret Service error variant.
#[cfg(target_os = "linux")]
fn by_code(e: &PlatformErr) -> Option<CredentialError> {
    use secret_service::Error as Ss;
    Some(match e.downcast_ref::<Ss>()? {
        Ss::Locked => CredentialError::Locked,
        Ss::Prompt | Ss::PromptDisconnected => CredentialError::Denied,
        Ss::Unavailable | Ss::Zbus(_) | Ss::ZbusFdo(_) | Ss::NoResult => {
            CredentialError::Unavailable
        }
        _ => return None,
    })
}

#[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
fn by_code(_: &PlatformErr) -> Option<CredentialError> {
    None
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

    #[test]
    fn the_service_name_is_pinned() {
        assert_eq!(SERVICE, "com.maecly.coverlyroms");
    }

    #[test]
    fn denied_never_carries_detail() {
        assert_eq!(
            CredentialError::Denied.to_string(),
            "access to the credential store was denied"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_codes_are_classified() {
        use security_framework::base::Error as SfError;
        let boxed = |code: i32| keyring::Error::NoStorageAccess(Box::new(SfError::from_code(code)));
        let plat = |code: i32| keyring::Error::PlatformFailure(Box::new(SfError::from_code(code)));
        assert_eq!(classify(&plat(-25308)), CredentialError::Locked);
        for c in [-128, -25293] {
            assert_eq!(classify(&plat(c)), CredentialError::Denied, "{c}");
        }
        for c in [-61, -25292, -25244] {
            assert_eq!(classify(&boxed(c)), CredentialError::Denied, "{c}");
        }
        for c in [-25291, -25294, -25295] {
            assert_eq!(classify(&boxed(c)), CredentialError::Unavailable, "{c}");
        }
        assert!(matches!(classify(&plat(-1)), CredentialError::Failed(_)));
    }

    /// `windows-native-keyring-store` 1.1.0 keeps `PlatformError` in a
    /// private module, so the test boxes a value of the same shape: the
    /// classifier reads the code from its derived `Debug` form.
    #[cfg(windows)]
    #[test]
    fn windows_codes_are_classified() {
        #[derive(Debug)]
        struct PlatformError(pub u32);
        impl std::fmt::Display for PlatformError {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(f, "Windows error code {}", self.0)
            }
        }
        impl std::error::Error for PlatformError {}
        let e = |code: u32| keyring::Error::PlatformFailure(Box::new(PlatformError(code)));
        assert_eq!(
            classify(&keyring::Error::NoStorageAccess(Box::new(PlatformError(
                1312
            )))),
            CredentialError::Unavailable
        );
        assert_eq!(classify(&e(5)), CredentialError::Denied);
        assert!(matches!(classify(&e(87)), CredentialError::Failed(_)));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_errors_are_classified() {
        use secret_service::Error as Ss;
        let e = |x: Ss| keyring::Error::NoStorageAccess(Box::new(x));
        assert_eq!(classify(&e(Ss::Locked)), CredentialError::Locked);
        assert_eq!(classify(&e(Ss::Prompt)), CredentialError::Denied);
        assert_eq!(
            classify(&keyring::Error::PlatformFailure(Box::new(
                Ss::PromptDisconnected
            ))),
            CredentialError::Denied
        );
        assert_eq!(classify(&e(Ss::Unavailable)), CredentialError::Unavailable);
        assert_eq!(classify(&e(Ss::NoResult)), CredentialError::Unavailable);
        assert!(matches!(
            classify(&keyring::Error::PlatformFailure(Box::new(Ss::Crypto("x")))),
            CredentialError::Failed(_)
        ));
    }

    /// Touches the real OS keychain: run by hand only.
    #[test]
    #[ignore = "real OS keychain"]
    fn real_keychain_round_trip() {
        let store = KeychainStore::with_service("com.maecly.coverlyroms.test");
        super::super::store_secret(&store, CredentialKey::ScreenscraperUser, "m5-test").unwrap();
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
