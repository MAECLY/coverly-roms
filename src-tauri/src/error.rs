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
}
