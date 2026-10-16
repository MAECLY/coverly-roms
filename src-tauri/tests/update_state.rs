//! Update policy (M7): startup gating, key, channel, phases, fill-job gating.

use std::sync::{Arc, Mutex};

use coverly_core::settings::{GeneralSettings, UpdateChannel};
use coverly_core::update::{BETA_ENDPOINT, STABLE_ENDPOINT};
use coverly_roms_lib::error::IpcError;
use coverly_roms_lib::update::*;

#[derive(Default)]
struct Calls {
    checks: Vec<String>,
    downloads: usize,
    installs: usize,
}

struct Fake {
    calls: Arc<Mutex<Calls>>,
    offer: Option<UpdateOfferView>,
    download_error: Option<IpcError>,
}

impl UpdateDriver for Fake {
    fn check(&self, endpoint: &str, pubkey: &str) -> Result<Option<UpdateOfferView>, IpcError> {
        assert_eq!(pubkey, "KEY");
        self.calls.lock().unwrap().checks.push(endpoint.to_string());
        Ok(self.offer.clone())
    }
    fn download(&self, on_progress: &mut dyn FnMut(u64, Option<u64>)) -> Result<(), IpcError> {
        self.calls.lock().unwrap().downloads += 1;
        on_progress(5, Some(10));
        on_progress(10, Some(10));
        self.download_error.clone().map_or(Ok(()), Err)
    }
    fn install(&self) -> Result<UpdateInstallView, IpcError> {
        self.calls.lock().unwrap().installs += 1;
        Ok(UpdateInstallView {
            restart_required: true,
        })
    }
}

fn offer() -> UpdateOfferView {
    UpdateOfferView {
        version: "9.9.9".into(),
        notes: Some("New".into()),
        date: None,
    }
}

fn manager(
    offer: Option<UpdateOfferView>,
    key: Option<&str>,
    err: Option<IpcError>,
) -> (UpdateManager, Arc<Mutex<Calls>>) {
    let calls = Arc::new(Mutex::new(Calls::default()));
    let fake = Fake {
        calls: calls.clone(),
        offer,
        download_error: err,
    };
    (
        UpdateManager::new(Box::new(fake), key.map(String::from), None),
        calls,
    )
}

fn general(on: bool, channel: UpdateChannel) -> GeneralSettings {
    GeneralSettings {
        check_updates_on_launch: on,
        update_channel: channel,
        ..GeneralSettings::default()
    }
}

#[test]
fn startup_check_with_the_setting_off_never_reaches_the_network() {
    let (m, calls) = manager(Some(offer()), Some("KEY"), None);
    let got = m
        .check(
            CheckTrigger::Startup,
            &general(false, UpdateChannel::Stable),
            "0.0.1",
        )
        .unwrap();
    assert_eq!(got, UpdateCheckView::Skipped);
    assert!(calls.lock().unwrap().checks.is_empty());
    // A manual check still works with the setting off.
    m.check(
        CheckTrigger::Manual,
        &general(false, UpdateChannel::Stable),
        "0.0.1",
    )
    .unwrap();
    assert_eq!(calls.lock().unwrap().checks, [STABLE_ENDPOINT]);
}

#[test]
fn the_channel_picks_the_manifest() {
    let (m, calls) = manager(None, Some("KEY"), None);
    let got = m
        .check(
            CheckTrigger::Startup,
            &general(true, UpdateChannel::Beta),
            "0.0.1",
        )
        .unwrap();
    assert_eq!(
        got,
        UpdateCheckView::UpToDate {
            current: "0.0.1".into(),
            latest: "0.0.1".into()
        }
    );
    assert_eq!(calls.lock().unwrap().checks, [BETA_ENDPOINT]);
}

#[test]
fn without_a_key_nothing_is_checked() {
    let (m, calls) = manager(Some(offer()), None, None);
    assert_eq!(
        m.check(
            CheckTrigger::Manual,
            &general(true, UpdateChannel::Stable),
            "0.0.1"
        ),
        Err(IpcError::UpdateNoKey)
    );
    assert!(calls.lock().unwrap().checks.is_empty());
}

#[test]
fn download_and_install_follow_a_check() {
    let (m, calls) = manager(Some(offer()), Some("KEY"), None);
    assert_eq!(m.download(&mut |_, _| {}), Err(IpcError::UpdateNotReady));
    assert_eq!(m.install(false), Err(IpcError::UpdateNotReady));
    let got = m
        .check(
            CheckTrigger::Manual,
            &general(true, UpdateChannel::Stable),
            "0.0.1",
        )
        .unwrap();
    assert_eq!(
        got,
        UpdateCheckView::Available {
            current: "0.0.1".into(),
            offer: offer()
        }
    );
    let mut seen = Vec::new();
    m.download(&mut |d, t| seen.push((d, t))).unwrap();
    assert_eq!(seen, [(5, Some(10)), (10, Some(10))]);
    assert_eq!(
        m.install(false),
        Ok(UpdateInstallView {
            restart_required: true
        })
    );
    assert_eq!(calls.lock().unwrap().installs, 1);
    assert_eq!(m.can_restart(false), Ok(()));
}

#[test]
fn install_and_restart_wait_for_the_job() {
    let (m, calls) = manager(Some(offer()), Some("KEY"), None);
    m.check(
        CheckTrigger::Manual,
        &general(true, UpdateChannel::Stable),
        "0.0.1",
    )
    .unwrap();
    m.download(&mut |_, _| {}).unwrap();
    assert_eq!(m.install(true), Err(IpcError::JobRunning));
    assert_eq!(
        calls.lock().unwrap().installs,
        0,
        "a running job is never interrupted"
    );
    m.install(false).unwrap();
    assert_eq!(m.can_restart(true), Err(IpcError::JobRunning));
    assert_eq!(m.can_restart(false), Ok(()));
}

#[test]
fn a_paused_job_holds_the_update_like_a_running_one() {
    use coverly_roms_lib::jobs::view::JobState as S;
    // A paused job (offline, rate limit, volume gone) still has items to
    // apply: installing (Windows quits) or restarting would interrupt it.
    for s in [S::Gathering, S::Applying, S::Paused] {
        assert!(job_blocks_update(Some(s)), "{s:?}");
    }
    for s in [S::Planned, S::Cancelled, S::Done, S::Failed] {
        assert!(!job_blocks_update(Some(s)), "{s:?}");
    }
    assert!(!job_blocks_update(None));
}

#[test]
fn a_rejected_download_can_be_retried_after_a_new_check() {
    let bad = IpcError::UpdateSignature("bad".into());
    let (m, _) = manager(Some(offer()), Some("KEY"), Some(bad.clone()));
    m.check(
        CheckTrigger::Manual,
        &general(true, UpdateChannel::Stable),
        "0.0.1",
    )
    .unwrap();
    assert_eq!(m.download(&mut |_, _| {}), Err(bad));
    assert_eq!(m.install(false), Err(IpcError::UpdateNotReady));
    assert_eq!(m.can_restart(false), Err(IpcError::UpdateNotReady));
}

#[test]
fn plugin_errors_keep_their_meaning() {
    use tauri_plugin_updater::Error as E;
    for e in [E::SignatureUtf8("x".into()), E::MissingSignedVersion] {
        assert!(
            matches!(classify(&e), IpcError::UpdateSignature(_)),
            "{e:?}"
        );
    }
    assert!(matches!(
        classify(&E::Network("503".into())),
        IpcError::UpdateOffline(_)
    ));
    assert!(matches!(
        classify(&E::ReleaseNotFound),
        IpcError::UpdateUnavailable(_)
    ));
    assert!(matches!(
        classify(&E::TargetNotFound("x".into())),
        IpcError::UpdateUnavailable(_)
    ));
    assert!(matches!(
        classify(&E::EmptyEndpoints),
        IpcError::UpdateFailed(_)
    ));
    assert_eq!(
        serde_json::to_value(IpcError::UpdateNoKey).unwrap(),
        serde_json::json!({ "code": "update-no-key" })
    );
}

#[test]
fn shipped_config_signs_updates_and_keeps_the_key_in_one_place() {
    let conf: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
    let updater = &conf["plugins"]["updater"];
    assert_eq!(
        updater["pubkey"], CONF_PUBKEY_MARKER,
        "the key lives in crates/coverly-core/updater.pub"
    );
    assert_eq!(updater["endpoints"][0], STABLE_ENDPOINT);
    assert_eq!(updater["requireSignedVersion"], true);
    assert!(updater.get("dangerousInsecureTransportProtocol").is_none());
    assert!(updater.get("allowDowngrades").is_none());
    assert_eq!(conf["bundle"]["createUpdaterArtifacts"], true);
    let parsed: tauri_plugin_updater::Config = serde_json::from_value(updater.clone()).unwrap();
    assert!(parsed.require_signed_version);
    let caps = include_str!("../capabilities/default.json");
    assert!(
        !caps.contains("updater:"),
        "updates are driven from Rust, never by the webview"
    );
}
