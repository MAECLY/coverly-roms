//! Errors returned by IPC commands. They cross the bridge as
//! `{ "code": "<kebab-case>", "detail": … }` (no `detail` for unit
//! variants); the UI shows the i18n key `errors.<code>`. Codes are stable:
//! never rename one.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, thiserror::Error)]
#[serde(tag = "code", content = "detail", rename_all = "kebab-case")]
pub enum IpcError {
    #[error("folder not found: {0}")]
    NotFound(String),
    #[error("not a folder: {0}")]
    NotAFolder(String),
    #[error("cannot read {0}")]
    Unreadable(String),
    #[error("no layout profile detected in {0}")]
    NoProfile(String),
    #[error("unknown layout profile `{0}`")]
    UnknownProfile(String),
    #[error("no library is open")]
    NoLibrary,
    #[error("the library was closed or reopened")]
    StaleLibrary,
    #[error("another library was opened meanwhile")]
    Superseded,
    #[error("unknown entry {0}")]
    UnknownEntry(u32),
    #[error("no {0} image for this entry")]
    ArtMissing(String),
    #[error("art outside the library: {0}")]
    OutsideLibrary(String),
    #[error("image too large: {0} bytes")]
    ArtTooLarge(u64),
    #[error("{0}")]
    Io(String),
    #[error("cannot save settings: {0}")]
    Settings(String),
    #[error("a fill job is running")]
    JobRunning,
    #[error("there is no fill job")]
    NoJob,
    #[error("the fill job was replaced by a newer one")]
    StaleJob,
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("nothing to undo")]
    NothingToUndo,
    #[error("network: {0}")]
    Network(String),
    #[error("not a usable image: {0}")]
    BadImage(String),
    #[error("write denied: {0}")]
    WriteDenied(String),
    #[error("the disk is full")]
    DiskFull,
    #[error("file name not valid on FAT/exFAT: {0}")]
    NameInvalid(String),
    #[error("the library volume is gone: {0}")]
    VolumeGone(String),
    #[error("there is no credential store (keychain) on this system")]
    KeychainUnavailable,
    #[error("the keychain is locked")]
    KeychainLocked,
    #[error("keychain error: {0}")]
    KeychainFailed(String),
    #[error("the update server could not be reached: {0}")]
    UpdateOffline(String),
    #[error("the update failed its signature check and was not installed: {0}")]
    UpdateSignature(String),
    #[error("no update is published for this channel and system: {0}")]
    UpdateUnavailable(String),
    #[error("this build has no update key")]
    UpdateNoKey,
    #[error("the update could not be installed: {0}")]
    UpdateFailed(String),
    #[error("check for updates first")]
    UpdateNotReady,
}

impl From<coverly_core::write::WriteError> for IpcError {
    fn from(e: coverly_core::write::WriteError) -> Self {
        use coverly_core::write::WriteError as W;
        match e {
            W::OutsideRoot(p) => IpcError::OutsideLibrary(p.display().to_string()),
            W::NameInvalid(n) => IpcError::NameInvalid(n),
            W::Denied(p) => IpcError::WriteDenied(p.display().to_string()),
            W::DiskFull => IpcError::DiskFull,
            W::NothingToUndo => IpcError::NothingToUndo,
            other @ (W::NotAFile(_) | W::Journal(_) | W::Io(_)) => IpcError::Io(other.to_string()),
        }
    }
}

impl From<coverly_core::fill::ArtError> for IpcError {
    fn from(e: coverly_core::fill::ArtError) -> Self {
        use coverly_core::fill::ArtError as A;
        match e {
            A::NoSlot | A::UnknownSource(_) | A::BadQuery => IpcError::BadRequest(e.to_string()),
            A::Source(m) | A::Paused(m) => IpcError::Network(m),
            A::Image(i) => IpcError::BadImage(i.to_string()),
            A::Write(w) => w.into(),
        }
    }
}

/// Never carries a stored value: only what went wrong.
impl From<coverly_core::credentials::CredentialError> for IpcError {
    fn from(e: coverly_core::credentials::CredentialError) -> Self {
        use coverly_core::credentials::CredentialError as C;
        match e {
            C::Unavailable => IpcError::KeychainUnavailable,
            C::Locked => IpcError::KeychainLocked,
            C::Failed(m) => IpcError::KeychainFailed(m),
            C::BadValue => {
                IpcError::BadRequest("the value is too long or holds control characters".into())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use coverly_core::write::WriteError;

    #[test]
    fn write_errors_keep_their_meaning() {
        assert_eq!(IpcError::from(WriteError::DiskFull), IpcError::DiskFull);
        assert_eq!(
            IpcError::from(WriteError::NothingToUndo),
            IpcError::NothingToUndo
        );
        assert!(matches!(
            IpcError::from(WriteError::Denied("/sd/x.png".into())),
            IpcError::WriteDenied(_)
        ));
        assert!(matches!(
            IpcError::from(coverly_core::fill::ArtError::BadQuery),
            IpcError::BadRequest(_)
        ));
        use coverly_core::credentials::CredentialError as C;
        assert_eq!(
            IpcError::from(C::Unavailable),
            IpcError::KeychainUnavailable
        );
        assert_eq!(IpcError::from(C::Locked), IpcError::KeychainLocked);
        assert!(matches!(
            IpcError::from(C::BadValue),
            IpcError::BadRequest(_)
        ));
        assert_eq!(
            serde_json::to_value(IpcError::from(C::Failed("denied".into()))).unwrap(),
            serde_json::json!({ "code": "keychain-failed", "detail": "denied" })
        );
        let json = serde_json::to_value(IpcError::JobRunning).unwrap();
        assert_eq!(json, serde_json::json!({ "code": "job-running" }));
    }
}
