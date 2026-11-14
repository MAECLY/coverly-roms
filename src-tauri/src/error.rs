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
    /// The game has no update or DLC at this index (part C D2).
    #[error("unknown child {0}")]
    UnknownChild(u32),
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
    #[error("access to the keychain was denied")]
    KeychainDenied,
    #[error("keychain error: {0}")]
    KeychainFailed(String),
    #[error("the update server could not be reached: {0}")]
    UpdateOffline(String),
    #[error("the update failed its signature check and was not installed: {0}")]
    UpdateSignature(String),
    /// The extras named by a handle are no longer what that listing showed
    /// (an older `rev`, or files changed on disk): list again (part D D22).
    #[error("the extras changed; list them again")]
    ExtrasChanged,
    /// The game's `extras.json` was written by a newer Coverly: view only
    /// (part D D5).
    #[error("extras.json was written by a newer Coverly")]
    ExtrasNewer,
    /// A name or tags the extras rules refuse; `why` is a kebab-case
    /// reason the webview renders (part D D1, D20).
    #[error("refused: {why}")]
    ExtrasRefused { why: String },
    #[error("no update is published for this channel and system: {0}")]
    UpdateUnavailable(String),
    #[error("this build has no update key")]
    UpdateNoKey,
    #[error("the update could not be installed: {0}")]
    UpdateFailed(String),
    #[error("check for updates first")]
    UpdateNotReady,
    #[error("cancelled")]
    Cancelled,
    #[error("that organise plan is no longer kept; plan again")]
    NoPlan,
    #[error("newer jobs wrote under this folder: {0}")]
    LaterJobs(String),
    /// Detail: `"<needed>/<free>"` bytes.
    #[error("not enough space (needed/free bytes): {0}")]
    NoSpace(String),
    #[error("that duplicates plan is no longer kept; look for duplicates again")]
    NoDedupePlan,
    /// A group must keep one copy that does not move (a conflict has no
    /// keeper until you pick one; some copies may never stay).
    #[error("this group needs a copy that stays: {0}")]
    KeeperRequired(String),
    #[error("the duplicates folder is inside or around a system folder: {0}")]
    QuarantineInSystemFolder(String),
    /// The decided plan writes at least 10 GB to the target, or has huge
    /// items, and the request did not confirm every huge item and that
    /// many bytes (systems batch 3 D23, adjustments D16): the numbers the
    /// questions show. `huge` are asked one by one, `rest` is what every
    /// other item writes.
    #[error("would write {bytes} bytes to {volume}")]
    LargeTransfer {
        bytes: u64,
        items: usize,
        copies: bool,
        free: Option<u64>,
        volume: String,
        huge: Vec<coverly_core::organize::apply::HugeItem>,
        rest: u64,
    },
}

impl From<coverly_core::organize::LargeTransfer> for IpcError {
    fn from(t: coverly_core::organize::LargeTransfer) -> Self {
        IpcError::LargeTransfer {
            bytes: t.bytes,
            items: t.items,
            copies: t.copies,
            free: t.free,
            volume: t.volume,
            huge: t.huge,
            rest: t.rest,
        }
    }
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
            other @ (W::NotAFile(_)
            | W::NotEmpty(_)
            | W::CrossDevice(_)
            | W::Journal(_)
            | W::Io(_)) => IpcError::Io(other.to_string()),
        }
    }
}

/// The kebab-case wire name of a serialisable reason enum.
fn reason<T: Serialize>(r: &T) -> String {
    serde_json::to_value(r)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

/// The `extras-refused` reasons of viewing, opening and adding a file
/// (writes refuse with the core's name and tag reasons, D20): kebab-case,
/// rendered by the webview, never prose.
pub mod extras_refusal {
    /// A key or licence file: never opened, played or shown.
    pub const KEY_FILE: &str = "key-file";
    /// A symlink: listed, never followed or served.
    pub const LINK: &str = "link";
    /// Not played or shown in the app.
    pub const NOT_MEDIA: &str = "not-media";
    /// Not a text file the viewer shows.
    pub const NOT_TEXT: &str = "not-text";
    /// Only revealed in the file manager, never opened with another app.
    pub const REVEAL_ONLY: &str = "reveal-only";
    /// An OS drop already added, or too old.
    pub const DROP_EXPIRED: &str = "drop-expired";
    /// Every reason above but [`LINK`] (a write's reason too).
    pub const ACCESS: [&str; 5] = [KEY_FILE, NOT_MEDIA, NOT_TEXT, REVEAL_ONLY, DROP_EXPIRED];
}

/// An Extras write refused or failed (part D D1, D5, D8, D20, D22): names
/// and tags reach the webview as kebab-case reasons, never as prose.
impl From<coverly_core::extras::ops::ExtrasError> for IpcError {
    fn from(e: coverly_core::extras::ops::ExtrasError) -> Self {
        use coverly_core::extras::ops::ExtrasError as E;
        let refused = |why: &str| IpcError::ExtrasRefused { why: why.into() };
        match e {
            E::IndexNewer => IpcError::ExtrasNewer,
            E::Changed => IpcError::ExtrasChanged,
            E::IndexUnreadable => refused("index-unreadable"),
            E::Name(r) => IpcError::ExtrasRefused { why: reason(&r) },
            E::Tags(r) => IpcError::ExtrasRefused {
                why: format!("tags-{}", reason(&r)),
            },
            E::NoTrash => refused("no-trash"),
            E::TooMany => refused("too-many"),
            E::NoSpace { needed, free } => IpcError::NoSpace(format!("{needed}/{free}")),
            E::Link(_) => refused(extras_refusal::LINK),
            E::OutsideExtras(p) => IpcError::OutsideLibrary(p.display().to_string()),
            E::Cancelled => IpcError::Cancelled,
            E::Write(w) => w.into(),
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
            C::Denied => IpcError::KeychainDenied,
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
            IpcError::from(WriteError::CrossDevice("/sd/x.gba".into())),
            IpcError::Io(_)
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
        assert_eq!(IpcError::from(C::Denied), IpcError::KeychainDenied);
        assert_eq!(
            serde_json::to_value(IpcError::from(C::Denied)).unwrap(),
            serde_json::json!({ "code": "keychain-denied" })
        );
        assert!(matches!(
            IpcError::from(C::BadValue),
            IpcError::BadRequest(_)
        ));
        assert_eq!(
            serde_json::to_value(IpcError::from(C::Failed("denied".into()))).unwrap(),
            serde_json::json!({ "code": "keychain-failed", "detail": "denied" })
        );
        assert_eq!(
            serde_json::to_value(IpcError::UnknownChild(3)).unwrap(),
            serde_json::json!({ "code": "unknown-child", "detail": 3 })
        );
        assert_eq!(
            serde_json::to_value(IpcError::ExtrasChanged).unwrap(),
            serde_json::json!({ "code": "extras-changed" })
        );
        assert_eq!(
            serde_json::to_value(IpcError::ExtrasNewer).unwrap(),
            serde_json::json!({ "code": "extras-newer" })
        );
        assert_eq!(
            serde_json::to_value(IpcError::ExtrasRefused {
                why: "taken".into()
            })
            .unwrap(),
            serde_json::json!({ "code": "extras-refused", "detail": { "why": "taken" } })
        );
        {
            use coverly_core::extras::ops::ExtrasError as E;
            use coverly_core::extras::{NameRefusal, TagRefusal};
            let why = |e: E| match IpcError::from(e) {
                IpcError::ExtrasRefused { why } => why,
                other => panic!("{other:?}"),
            };
            assert_eq!(why(E::Name(NameRefusal::PathTooLong)), "path-too-long");
            assert_eq!(why(E::Name(NameRefusal::Taken)), "taken");
            assert_eq!(why(E::Tags(TagRefusal::TooLong)), "tags-too-long");
            assert_eq!(why(E::NoTrash), "no-trash");
            assert_eq!(why(E::IndexUnreadable), "index-unreadable");
            assert_eq!(IpcError::from(E::Changed), IpcError::ExtrasChanged);
            assert_eq!(IpcError::from(E::IndexNewer), IpcError::ExtrasNewer);
            assert_eq!(
                IpcError::from(E::NoSpace {
                    needed: 8192,
                    free: 4096
                }),
                IpcError::NoSpace("8192/4096".into())
            );
        }
        let json = serde_json::to_value(IpcError::JobRunning).unwrap();
        assert_eq!(json, serde_json::json!({ "code": "job-running" }));
        assert_eq!(
            serde_json::to_value(IpcError::NoPlan).unwrap(),
            serde_json::json!({ "code": "no-plan" })
        );
        assert_eq!(
            serde_json::to_value(IpcError::Cancelled).unwrap(),
            serde_json::json!({ "code": "cancelled" })
        );
        assert_eq!(
            serde_json::to_value(IpcError::NoSpace("2000/1000".into())).unwrap(),
            serde_json::json!({ "code": "no-space", "detail": "2000/1000" })
        );
        assert_eq!(
            serde_json::to_value(IpcError::LaterJobs("a, b".into())).unwrap(),
            serde_json::json!({ "code": "later-jobs", "detail": "a, b" })
        );
        assert_eq!(
            serde_json::to_value(IpcError::NoDedupePlan).unwrap(),
            serde_json::json!({ "code": "no-dedupe-plan" })
        );
        assert_eq!(
            serde_json::to_value(IpcError::KeeperRequired("group 4".into())).unwrap(),
            serde_json::json!({ "code": "keeper-required", "detail": "group 4" })
        );
        assert_eq!(
            serde_json::to_value(IpcError::QuarantineInSystemFolder("/sd/ROMS/GB".into())).unwrap(),
            serde_json::json!({ "code": "quarantine-in-system-folder", "detail": "/sd/ROMS/GB" })
        );
        let large = IpcError::LargeTransfer {
            bytes: 187_420_000_000,
            items: 3,
            copies: true,
            free: None,
            volume: "TinyExt".into(),
            huge: Vec::new(),
            rest: 187_420_000_000,
        };
        assert_eq!(
            large.to_string(),
            "would write 187420000000 bytes to TinyExt"
        );
        assert_eq!(
            serde_json::to_value(large).unwrap(),
            serde_json::json!({
                "code": "large-transfer",
                "detail": {
                    "bytes": 187_420_000_000u64,
                    "items": 3,
                    "copies": true,
                    "free": null,
                    "volume": "TinyExt",
                    "huge": [],
                    "rest": 187_420_000_000u64,
                }
            })
        );
    }
}
