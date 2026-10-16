//! In-app updates (M7, ADR-0011): the policy lives here and is tested with a
//! fake driver; `PluginDriver` is the thin `tauri-plugin-updater` binding.
//! The webview never talks to the plugin (no `updater:` permission): it calls
//! these commands, and the endpoint and key are always ours.

use std::sync::Mutex;
use std::time::Duration;

use coverly_core::settings::GeneralSettings;
use serde::{Deserialize, Serialize};

use crate::error::IpcError;

/// `plugins.updater.pubkey` in tauri.conf.json; the real key is set at runtime.
pub const CONF_PUBKEY_MARKER: &str = "set at runtime from crates/coverly-core/updater.pub";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CheckTrigger {
    Startup,
    Manual,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct UpdateOfferView {
    pub version: String,
    pub notes: Option<String>,
    pub date: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum UpdateCheckView {
    /// Startup check with "check on start" off: nothing was requested.
    Skipped,
    UpToDate {
        current: String,
        latest: String,
    },
    Available {
        current: String,
        offer: UpdateOfferView,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct UpdateProgressEvent {
    pub downloaded: u64,
    pub total: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct UpdateInstallView {
    /// macOS/Linux: the new version runs after a restart. Windows never gets
    /// here (the installer ends this process).
    pub restart_required: bool,
}

pub trait UpdateDriver: Send + Sync {
    fn check(&self, endpoint: &str, pubkey: &str) -> Result<Option<UpdateOfferView>, IpcError>;
    fn download(&self, on_progress: &mut dyn FnMut(u64, Option<u64>)) -> Result<(), IpcError>;
    fn install(&self) -> Result<UpdateInstallView, IpcError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Idle,
    Available,
    Downloading,
    Downloaded,
    Installed,
}

pub struct UpdateManager {
    driver: Box<dyn UpdateDriver>,
    pubkey: Option<String>,
    endpoint_override: Option<String>,
    phase: Mutex<Phase>,
}

impl UpdateManager {
    pub fn new(
        driver: Box<dyn UpdateDriver>,
        pubkey: Option<String>,
        endpoint_override: Option<String>,
    ) -> Self {
        UpdateManager {
            driver,
            pubkey,
            endpoint_override,
            phase: Mutex::new(Phase::Idle),
        }
    }

    fn phase(&self) -> std::sync::MutexGuard<'_, Phase> {
        self.phase.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn check(
        &self,
        trigger: CheckTrigger,
        general: &GeneralSettings,
        current: &str,
    ) -> Result<UpdateCheckView, IpcError> {
        if trigger == CheckTrigger::Startup && !general.check_updates_on_launch {
            return Ok(UpdateCheckView::Skipped);
        }
        let pubkey = self.pubkey.as_deref().ok_or(IpcError::UpdateNoKey)?;
        if matches!(
            *self.phase(),
            Phase::Downloading | Phase::Downloaded | Phase::Installed
        ) {
            // An update is already on its way: a new check would drop it.
            return Err(IpcError::UpdateFailed(
                "an update is already being installed".into(),
            ));
        }
        let endpoint = coverly_core::update::resolve_endpoint(
            self.endpoint_override.as_deref(),
            general.update_channel,
        )
        .map_err(|e| IpcError::UpdateFailed(e.to_string()))?;
        let found = self.driver.check(&endpoint, pubkey)?;
        *self.phase() = if found.is_some() {
            Phase::Available
        } else {
            Phase::Idle
        };
        Ok(match found {
            Some(offer) => UpdateCheckView::Available {
                current: current.to_string(),
                offer,
            },
            None => UpdateCheckView::UpToDate {
                current: current.to_string(),
                latest: current.to_string(),
            },
        })
    }

    pub fn download(&self, on_progress: &mut dyn FnMut(u64, Option<u64>)) -> Result<(), IpcError> {
        {
            let mut p = self.phase();
            if *p != Phase::Available {
                return Err(IpcError::UpdateNotReady);
            }
            *p = Phase::Downloading;
        }
        let result = self.driver.download(on_progress);
        *self.phase() = if result.is_ok() {
            Phase::Downloaded
        } else {
            Phase::Idle
        };
        result
    }

    pub fn install(&self, job_running: bool) -> Result<UpdateInstallView, IpcError> {
        if *self.phase() != Phase::Downloaded {
            return Err(IpcError::UpdateNotReady);
        }
        if job_running {
            return Err(IpcError::JobRunning);
        }
        let view = self.driver.install()?;
        *self.phase() = Phase::Installed;
        Ok(view)
    }

    pub fn can_restart(&self, job_running: bool) -> Result<(), IpcError> {
        if *self.phase() != Phase::Installed {
            return Err(IpcError::UpdateNotReady);
        }
        if job_running {
            return Err(IpcError::JobRunning);
        }
        Ok(())
    }
}

/// Whether the current fill job must finish before an update installs or the
/// app restarts: running, or paused with items still to apply.
pub fn job_blocks_update(state: Option<crate::jobs::view::JobState>) -> bool {
    use crate::jobs::view::JobState as S;
    matches!(state, Some(S::Gathering | S::Applying | S::Paused))
}

/// Every way the plugin can fail, in the IPC vocabulary.
///
/// The plugin reports any non-2xx manifest answer (404 and 5xx alike) as
/// `ReleaseNotFound` without the status, so a server outage reads as
/// update-unavailable, and its UI copy says so ("none is published yet, or
/// the server could not provide it"). A 200 that is not JSON is a reqwest
/// decode error: update-offline.
pub fn classify(err: &tauri_plugin_updater::Error) -> IpcError {
    use tauri_plugin_updater::Error as E;
    match err {
        E::Minisign(_)
        | E::Base64(_)
        | E::SignatureUtf8(_)
        | E::SignedVersionMismatch { .. }
        | E::MissingSignedVersion => IpcError::UpdateSignature(err.to_string()),
        E::Reqwest(_) | E::Network(_) => IpcError::UpdateOffline(err.to_string()),
        E::ReleaseNotFound | E::Serialization(_) | E::TargetNotFound(_) | E::TargetsNotFound(_) => {
            IpcError::UpdateUnavailable(err.to_string())
        }
        other => IpcError::UpdateFailed(other.to_string()),
    }
}

/// Total bound on the manifest request. The plugin has no timeout by
/// default, so a server that accepts and never answers would hang the check.
pub const CHECK_TIMEOUT: Duration = Duration::from_secs(30);
/// Total bound on the artifact download (larger: it is tens of MiB).
pub const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(20 * 60);
/// Release notes longer than this are cut before they reach the webview.
pub const MAX_NOTES_BYTES: usize = 64 * 1024;

/// The offer the webview sees, with the notes capped at `MAX_NOTES_BYTES`
/// (cut on a char boundary).
pub fn offer_view(version: &str, notes: Option<&str>, raw: &serde_json::Value) -> UpdateOfferView {
    let notes = notes.map(|n| {
        if n.len() <= MAX_NOTES_BYTES {
            return n.to_string();
        }
        let mut end = MAX_NOTES_BYTES;
        while !n.is_char_boundary(end) {
            end -= 1;
        }
        n[..end].to_string()
    });
    UpdateOfferView {
        version: version.to_string(),
        notes,
        date: raw
            .get("pub_date")
            .and_then(|d| d.as_str())
            .map(String::from),
    }
}

/// The real driver: `tauri-plugin-updater`, blocking on its futures from the
/// blocking pool (the IPC commands already run there).
pub struct PluginDriver<R: tauri::Runtime> {
    app: tauri::AppHandle<R>,
    check_timeout: Duration,
    download_timeout: Duration,
    pending: Mutex<Option<tauri_plugin_updater::Update>>,
    bytes: Mutex<Option<Vec<u8>>>,
}

impl<R: tauri::Runtime> PluginDriver<R> {
    pub fn new(app: tauri::AppHandle<R>) -> Self {
        PluginDriver {
            app,
            check_timeout: CHECK_TIMEOUT,
            download_timeout: DOWNLOAD_TIMEOUT,
            pending: Mutex::new(None),
            bytes: Mutex::new(None),
        }
    }

    /// Other bounds than `CHECK_TIMEOUT` / `DOWNLOAD_TIMEOUT` (tests).
    pub fn with_timeouts(mut self, check: Duration, download: Duration) -> Self {
        self.check_timeout = check;
        self.download_timeout = download;
        self
    }
}

impl<R: tauri::Runtime> UpdateDriver for PluginDriver<R> {
    fn check(&self, endpoint: &str, pubkey: &str) -> Result<Option<UpdateOfferView>, IpcError> {
        use tauri_plugin_updater::UpdaterExt;
        let url = tauri::Url::parse(endpoint).map_err(|e| IpcError::UpdateFailed(e.to_string()))?;
        let updater = self
            .app
            .updater_builder()
            .pubkey(pubkey)
            .timeout(self.check_timeout)
            .endpoints(vec![url])
            .and_then(|b| b.build())
            .map_err(|e| classify(&e))?;
        let found = tauri::async_runtime::block_on(updater.check()).map_err(|e| classify(&e))?;
        let view = found
            .as_ref()
            .map(|u| offer_view(&u.version, u.body.as_deref(), &u.raw_json));
        *self.pending.lock().unwrap_or_else(|e| e.into_inner()) = found;
        *self.bytes.lock().unwrap_or_else(|e| e.into_inner()) = None;
        Ok(view)
    }

    fn download(&self, on_progress: &mut dyn FnMut(u64, Option<u64>)) -> Result<(), IpcError> {
        let mut update = self
            .pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .ok_or(IpcError::UpdateNotReady)?;
        // The check's bound is too tight for the artifact itself.
        update.timeout = Some(self.download_timeout);
        let mut done = 0u64;
        // `download` verifies the signature before returning any byte.
        let bytes = tauri::async_runtime::block_on(update.download(
            |chunk, total| {
                done += chunk as u64;
                on_progress(done, total);
            },
            || {},
        ))
        .map_err(|e| classify(&e))?;
        *self.bytes.lock().unwrap_or_else(|e| e.into_inner()) = Some(bytes);
        Ok(())
    }

    fn install(&self) -> Result<UpdateInstallView, IpcError> {
        let update = self
            .pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .ok_or(IpcError::UpdateNotReady)?;
        let bytes = self
            .bytes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
            .ok_or(IpcError::UpdateNotReady)?;
        update.install(bytes).map_err(|e| classify(&e))?;
        Ok(UpdateInstallView {
            restart_required: !cfg!(windows),
        })
    }
}
