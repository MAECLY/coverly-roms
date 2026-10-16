//! Source availability and "Test key" (M6). Values never leave Rust: the
//! answers are presence flags and a `KeyCheck`.

use coverly_core::credentials::{CredentialKey, DevCredentials};
use std::time::Duration;

use coverly_core::net::{NetConfig, PoliteHttp, UreqHttp};
use coverly_core::settings::KNOWN_SOURCES;
use coverly_core::sources::screenscraper::{check_account, SsConfig};
use coverly_core::sources::{steamgriddb, KeyCheck};
use serde::Serialize;

use crate::error::IpcError;
use crate::state::AppState;

/// The longest a "Test key" request waits: the settings timeout is for
/// slow downloads, not for a button the user is watching.
pub const KEY_CHECK_TIMEOUT: Duration = Duration::from_secs(10);

/// The network settings of a key check: one try, a short timeout.
pub fn key_check_net(net: &NetConfig) -> NetConfig {
    NetConfig {
        retries: 0,
        timeout: net.timeout.min(KEY_CHECK_TIMEOUT),
        ..net.clone()
    }
}

/// One source as the Sources settings show it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SourceInfoView {
    pub id: String,
    /// This build can use it (ScreenScraper needs developer credentials).
    pub built: bool,
    /// Credentials it needs before it can run.
    pub needs: Vec<CredentialKey>,
    /// Built and every needed credential stored.
    pub ready: bool,
}

impl AppState {
    /// Every known source, whether this build has it and whether the
    /// credentials it needs are stored and usable: the same keys a fill
    /// and "Test key" send (presence only, never values).
    pub fn sources_info(&self) -> Vec<SourceInfoView> {
        let keys = self.source_keys();
        let usable = |k: &CredentialKey| match k {
            CredentialKey::SteamgriddbApiKey => keys.steamgriddb.is_some(),
            CredentialKey::ScreenscraperUser | CredentialKey::ScreenscraperPassword => {
                keys.screenscraper_user.is_some()
            }
        };
        KNOWN_SOURCES
            .iter()
            .map(|&id| {
                let (built, needs) = match id {
                    "steamgriddb" => (true, vec![CredentialKey::SteamgriddbApiKey]),
                    "screenscraper" => (DevCredentials::from_build().is_some(), vec![]),
                    _ => (true, vec![]),
                };
                let ready = built && needs.iter().all(usable);
                SourceInfoView {
                    id: id.to_string(),
                    built,
                    needs,
                    ready,
                }
            })
            .collect()
    }

    /// One cheap authenticated request with the stored key ("Test key").
    /// Only the verdict comes back; the key is never returned or logged.
    pub fn test_source_key(&self, source: &str) -> Result<KeyCheck, IpcError> {
        let cfg = self.fill_config();
        let net = key_check_net(&cfg.net);
        let http = PoliteHttp::new(UreqHttp::new(&net), &net);
        let check = match source {
            "steamgriddb" => steamgriddb::check_key(
                &http,
                &cfg.endpoints.steamgriddb,
                cfg.keys.steamgriddb.as_ref(),
            ),
            "screenscraper" => match &cfg.keys.screenscraper_dev {
                None => KeyCheck::Unavailable,
                Some(dev) => {
                    let ss = SsConfig {
                        base: cfg.endpoints.screenscraper.clone(),
                        ..SsConfig::default()
                    };
                    check_account(&http, &ss, dev, cfg.keys.screenscraper_user.as_ref())
                }
            },
            _ => {
                return Err(IpcError::BadRequest(
                    "this source has no key to test".into(),
                ))
            }
        };
        log::info!("source key checked: {source} -> {check:?}");
        Ok(check)
    }
}
