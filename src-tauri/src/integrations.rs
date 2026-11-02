//! The «Integraciones» settings tab on the Rust side (integrations spec
//! D6, D9–D13, D21, D29): one view per working integration of the
//! registry, the connection test, «Olvidar credenciales» and the help
//! link. Values never leave Rust: the view carries presence flags, a
//! state and the quota numbers ScreenScraper reported, nothing else.
//!
//! Health (what the last test or job said) and the last quota live in
//! memory only (§12.4): a restart starts every card at `saved`.

use std::collections::{BTreeMap, BTreeSet};

use coverly_core::credentials::{CredentialError, CredentialKey, DevCredentials};
use coverly_core::integrations::{
    self, Health, Integration, KeychainKind, Reason, SeenQuota, SsQuota, State, StateInput,
};
use coverly_core::settings::Settings;
use coverly_core::sources::{KeyCheck, SourceTrouble, TroubleKind};
use serde::Serialize;
use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt;

use crate::error::IpcError;
use crate::ipc::blocking;
use crate::state::{now_unix, AppState};

/// One credential field of a card: stored or not (never the value).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FieldView {
    pub key: CredentialKey,
    pub set: bool,
}

/// One card of the «Integraciones» tab.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct IntegrationView {
    pub id: String,
    pub enabled: bool,
    /// This build can use it (ScreenScraper needs developer credentials).
    pub built: bool,
    pub fields: Vec<FieldView>,
    pub state: State,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<Reason>,
    /// When the last test or job said what `state` shows (unix seconds).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checked_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quota: Option<SsQuota>,
    /// When the quota was last seen, or seen used up (unix seconds).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quota_at: Option<i64>,
}

/// What «Probar conexión» found.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TestResult {
    pub verdict: KeyCheck,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quota: Option<SsQuota>,
}

/// The in-memory health of the integrations, by integration id.
#[derive(Clone, Debug, Default)]
pub struct HealthBook {
    health: BTreeMap<String, Health>,
    /// The last ScreenScraper quota with numbers.
    quota: Option<SeenQuota>,
    /// When a job was told the daily quota is used up, by integration id
    /// (the service does not always send numbers with that answer).
    exhausted: BTreeMap<String, i64>,
}

impl HealthBook {
    fn forget(&mut self, id: &str) {
        self.health.remove(id);
        self.exhausted.remove(id);
        if id == "screenscraper" {
            self.quota = None;
        }
    }

    fn note_quota(&mut self, quota: SsQuota, at: i64) {
        self.quota = Some(SeenQuota { quota, at });
    }
}

/// Why the keychain could not be read, without its message (D16).
pub fn keychain_kind(e: &CredentialError) -> KeychainKind {
    match e {
        CredentialError::Unavailable => KeychainKind::Unavailable,
        CredentialError::Locked => KeychainKind::Locked,
        CredentialError::Denied => KeychainKind::Denied,
        CredentialError::Failed(_) | CredentialError::BadValue => KeychainKind::Failed,
    }
}

/// Whether the user has `entry` turned on: its source in the Fuentes
/// order, the Switch catalogue switch, or always (a link card).
fn enabled(entry: &Integration, s: &Settings) -> bool {
    if let Some(source) = &entry.source {
        return s.sources.order.iter().any(|e| &e.id == source && e.enabled);
    }
    match entry.id.as_str() {
        "switch-catalog" => s.catalogs.switch.enabled,
        _ => true,
    }
}

/// Whether this build can use `entry`.
fn built(entry: &Integration) -> bool {
    entry.id != "screenscraper" || DevCredentials::from_build().is_some()
}

/// The working entry `id`, or a bad request.
fn working(id: &str) -> Result<&'static Integration, IpcError> {
    integrations::working()
        .find(|e| e.id == id)
        .ok_or_else(|| IpcError::BadRequest(format!("`{id}` is not a working integration")))
}

/// The official account or API page of integration `id` (never a URL the
/// webview sends).
pub fn help_url(id: &str) -> Result<&'static str, IpcError> {
    integrations::get(id)
        .and_then(|e| e.help_url.as_deref())
        .ok_or_else(|| IpcError::BadRequest(format!("`{id}` has no help page")))
}

/// The log line of a connection test: the id and the verdict, never a value.
fn checked_line(id: &str, verdict: KeyCheck) -> String {
    format!("integration checked: {id} -> {verdict:?}")
}

/// The log line of «Olvidar credenciales».
fn forgotten_line(id: &str) -> String {
    format!("credentials forgotten: {id}")
}

/// A synthetic used-up quota, so [`integrations::state`] gives `quota`
/// when a job saw the quota exhausted without numbers.
fn used_up_at(at: i64) -> SeenQuota {
    SeenQuota {
        quota: SsQuota {
            requests_today: 1,
            max_requests_per_day: 1,
            max_threads: 0,
        },
        at,
    }
}

fn view_of(
    entry: &Integration,
    settings: &Settings,
    presence: &Result<Vec<(CredentialKey, bool)>, KeychainKind>,
    book: &HealthBook,
    now: i64,
) -> IntegrationView {
    let enabled = enabled(entry, settings);
    let built = built(entry);
    // An entry without fields never reads the keychain, so it never fails on it.
    let presence: Result<&[(CredentialKey, bool)], KeychainKind> = if entry.fields.is_empty() {
        Ok(&[])
    } else {
        presence.as_ref().map(Vec::as_slice).map_err(|k| *k)
    };
    let fields = entry
        .fields
        .iter()
        .map(|f| FieldView {
            key: f.key,
            set: presence.is_ok_and(|p| p.iter().any(|&(k, set)| k == f.key && set)),
        })
        .collect();
    let health = book.health.get(&entry.id).copied();
    let exhausted = book.exhausted.get(&entry.id).copied();
    let numbers = (entry.id == "screenscraper")
        .then_some(book.quota)
        .flatten();
    // Numbers that say «used up» win; else a job's «exhausted» without numbers.
    let quota = match (numbers, exhausted) {
        (Some(q), Some(at)) if at > q.at => Some(used_up_at(at)),
        (Some(q), _) => Some(q),
        (None, Some(at)) => Some(used_up_at(at)),
        (None, None) => None,
    };
    let (state, reason) = integrations::state(&StateInput {
        entry,
        // A build that cannot use it cannot turn it on either: «not in
        // this build» is the truth, not «off» (every official build today).
        enabled: enabled || !built,
        built,
        presence,
        health,
        quota,
        now,
    });
    IntegrationView {
        id: entry.id.clone(),
        enabled,
        built,
        fields,
        state,
        reason,
        checked_at: health.map(|h| h.at),
        quota: numbers.map(|q| q.quota),
        quota_at: match (numbers.map(|q| q.at), exhausted) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        },
    }
}

impl AppState {
    /// Every working integration as its card shows it: presence from the
    /// cached keychain read (no network, D9), settings and health.
    pub fn integrations_view(&self) -> Vec<IntegrationView> {
        let settings = self.settings();
        let presence = self.key_presence().map_err(|e| keychain_kind(&e));
        let book = self.integration_book().clone();
        let now = now_unix();
        integrations::working()
            .map(|e| view_of(e, &settings, &presence, &book, now))
            .collect()
    }

    /// «Probar conexión»: one cheap authenticated request with the stored
    /// keys. The verdict and any quota become the card's health.
    pub fn test_integration(&self, id: &str) -> Result<TestResult, IpcError> {
        let entry = working(id)?;
        let source = match (entry.test, &entry.source) {
            (Some(_), Some(source)) => source.as_str(),
            _ => {
                return Err(IpcError::BadRequest(format!(
                    "`{id}` has no connection test"
                )))
            }
        };
        let (verdict, quota) = self.check_source(source)?;
        let now = now_unix();
        {
            let mut book = self.integration_book();
            book.health
                .insert(id.to_string(), Health { verdict, at: now });
            if let Some(q) = quota {
                book.note_quota(q, now);
                book.exhausted.remove(id);
            }
        }
        log::info!("{}", checked_line(id, verdict));
        Ok(TestResult { verdict, quota })
    }

    /// «Olvidar credenciales»: clears every field of `id` (all attempted,
    /// the first error returned after), drops its health, and gives the
    /// fresh view.
    pub fn forget_integration(&self, id: &str) -> Result<Vec<IntegrationView>, IpcError> {
        let entry = working(id)?;
        if entry.fields.is_empty() {
            return Err(IpcError::BadRequest(format!("`{id}` has no credentials")));
        }
        if !self.credentials().available() {
            return Err(IpcError::KeychainUnavailable);
        }
        let mut first_error = None;
        for f in &entry.fields {
            if let Err(e) = self.credentials().clear(f.key) {
                first_error.get_or_insert(e);
            }
        }
        self.forget_keys();
        self.integration_book().forget(id);
        if let Some(e) = first_error {
            return Err(e.into());
        }
        log::info!("{}", forgotten_line(id));
        Ok(self.integrations_view())
    }

    /// What a job found about its sources (D29): `Auth` → error/invalid,
    /// `Exhausted` → quota, a source that answered → connected;
    /// `MissingKey` changes nothing (presence already says it).
    pub fn note_troubles(&self, troubles: &[SourceTrouble], answered: &BTreeSet<String>) {
        let now = now_unix();
        let mut book = self.integration_book();
        for source in answered {
            if let Some(e) = integrations::for_source(source) {
                book.health.insert(
                    e.id.clone(),
                    Health {
                        verdict: KeyCheck::Ok,
                        at: now,
                    },
                );
                let exhausted_now = troubles
                    .iter()
                    .any(|t| &t.source == source && t.kind == TroubleKind::Exhausted);
                if !exhausted_now {
                    book.exhausted.remove(&e.id);
                }
            }
        }
        for t in troubles {
            let Some(e) = integrations::for_source(&t.source) else {
                continue;
            };
            match t.kind {
                TroubleKind::MissingKey => {}
                TroubleKind::Auth => {
                    book.health.insert(
                        e.id.clone(),
                        Health {
                            verdict: KeyCheck::Invalid,
                            at: now,
                        },
                    );
                }
                TroubleKind::Exhausted => {
                    book.exhausted.insert(e.id.clone(), now);
                }
            }
        }
    }

    /// The ScreenScraper quota a run reported.
    pub fn note_ss_quota(&self, quota: SsQuota) {
        self.integration_book().note_quota(quota, now_unix());
    }
}

/// Every working integration as its card shows it.
#[tauri::command]
pub async fn integrations_view(app: AppHandle) -> Result<Vec<IntegrationView>, IpcError> {
    blocking(move || Ok(app.state::<AppState>().integrations_view())).await
}

/// «Probar conexión» on integration `id`; never returns a value.
#[tauri::command]
pub async fn test_integration(app: AppHandle, id: String) -> Result<TestResult, IpcError> {
    blocking(move || app.state::<AppState>().test_integration(&id)).await
}

/// «Olvidar credenciales» on integration `id`.
#[tauri::command]
pub async fn forget_integration(
    app: AppHandle,
    id: String,
) -> Result<Vec<IntegrationView>, IpcError> {
    blocking(move || app.state::<AppState>().forget_integration(&id)).await
}

/// Opens the official account or API page of integration `id`.
#[tauri::command]
pub fn open_integration_help(app: AppHandle, id: String) -> Result<(), IpcError> {
    let url = help_url(&id)?;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| IpcError::Io(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use coverly_core::credentials::{CredentialStore, MemoryStore};
    use coverly_core::net::testing::{MockResponse, MockServer};
    use coverly_core::sources::steamgriddb::search_path;
    use std::sync::atomic::{AtomicBool, Ordering};

    use crate::jobs::FillConfig;

    fn app(
        store: Arc<dyn CredentialStore>,
        server: Option<&MockServer>,
    ) -> (AppState, tempfile::TempDir) {
        let dir = tempfile::TempDir::new().unwrap();
        let mut base = FillConfig::new(dir.path().join("cache"), dir.path().join("data"));
        if let Some(server) = server {
            base.endpoints.steamgriddb = server.url("/sgdb");
        }
        base.net.retries = 0;
        let st = AppState::new(dir.path().join("config"), base).with_credentials(store);
        (st, dir)
    }

    fn card<'a>(view: &'a [IntegrationView], id: &str) -> &'a IntegrationView {
        view.iter().find(|v| v.id == id).unwrap()
    }

    fn enable(st: &AppState, source: &str) {
        st.update_settings(&serde_json::json!({
            "sources": { "order": st.settings().sources.order.iter().map(|e| {
                serde_json::json!({ "id": e.id, "enabled": e.enabled || e.id == source })
            }).collect::<Vec<_>>() }
        }))
        .unwrap();
    }

    fn full_store() -> Arc<MemoryStore> {
        let store = Arc::new(MemoryStore::default());
        store
            .set(CredentialKey::SteamgriddbApiKey, "sk-STORED")
            .unwrap();
        store
            .set(CredentialKey::ScreenscraperUser, "me-STORED")
            .unwrap();
        store
            .set(CredentialKey::ScreenscraperPassword, "pw-STORED")
            .unwrap();
        store
    }

    #[test]
    fn stored_credentials_under_todays_names_show_up_without_values() {
        let (st, _d) = app(full_store(), None);
        enable(&st, "steamgriddb");
        enable(&st, "screenscraper");
        let view = st.integrations_view();
        let sgdb = card(&view, "steamgriddb");
        assert_eq!(sgdb.state, State::Saved);
        assert!(sgdb.fields[0].set);
        let ss = card(&view, "screenscraper");
        assert!(ss.fields.iter().all(|f| f.set));
        if DevCredentials::from_build().is_none() {
            assert_eq!(ss.state, State::Unavailable);
            assert!(!ss.built);
        }
        let json = serde_json::to_string(&view).unwrap();
        assert!(!json.contains("STORED"), "{json}");
        let ids: Vec<&str> = view.iter().map(|v| v.id.as_str()).collect();
        assert_eq!(
            ids,
            ["steamgriddb", "screenscraper", "switch-catalog", "updates"]
        );
        assert_eq!(card(&view, "switch-catalog").state, State::Off);
        assert_eq!(card(&view, "updates").state, State::Saved);
    }

    #[test]
    fn the_view_reads_the_keychain_once_until_a_credential_changes() {
        let store = Arc::new(MemoryStore::default());
        let (st, _d) = app(store.clone(), None);
        let _ = st.integrations_view();
        let _ = st.integrations_view();
        assert!(store.reads() <= CredentialKey::ALL.len());
        let before = store.reads();
        st.set_credential("steamgriddb-api-key", "sk").unwrap();
        let view = st.integrations_view();
        assert!(store.reads() > before, "a save invalidates the cache");
        assert!(card(&view, "steamgriddb").fields[0].set);
    }

    #[test]
    fn a_failing_keychain_is_an_error_never_not_configured() {
        let (st, _d) = app(Arc::new(MemoryStore::unavailable()), None);
        enable(&st, "steamgriddb");
        let view = st.integrations_view();
        let sgdb = card(&view, "steamgriddb");
        assert_eq!(sgdb.state, State::Error);
        assert_eq!(
            sgdb.reason,
            Some(Reason::Keychain(KeychainKind::Unavailable))
        );
        assert_eq!(
            serde_json::to_value(sgdb.reason).unwrap(),
            serde_json::json!({ "kind": "keychain", "detail": "unavailable" })
        );
        // No fields, no keychain: the link card is fine.
        assert_eq!(card(&view, "updates").state, State::Saved);
    }

    /// Refuses the first clear of the ScreenScraper user.
    struct FailOnce {
        inner: MemoryStore,
        failed: AtomicBool,
    }

    impl CredentialStore for FailOnce {
        fn available(&self) -> bool {
            true
        }
        fn get(&self, key: CredentialKey) -> Result<Option<String>, CredentialError> {
            self.inner.get(key)
        }
        fn set(&self, key: CredentialKey, value: &str) -> Result<(), CredentialError> {
            self.inner.set(key, value)
        }
        fn clear(&self, key: CredentialKey) -> Result<(), CredentialError> {
            if key == CredentialKey::ScreenscraperUser && !self.failed.swap(true, Ordering::SeqCst)
            {
                return Err(CredentialError::Denied);
            }
            self.inner.clear(key)
        }
    }

    #[test]
    fn forget_clears_every_field_even_when_one_fails() {
        let store = Arc::new(FailOnce {
            inner: MemoryStore::default(),
            failed: AtomicBool::new(false),
        });
        store.set(CredentialKey::ScreenscraperUser, "me").unwrap();
        store
            .set(CredentialKey::ScreenscraperPassword, "pw")
            .unwrap();
        let (st, _d) = app(store.clone(), None);
        assert_eq!(
            st.forget_integration("screenscraper").unwrap_err(),
            IpcError::KeychainDenied
        );
        assert_eq!(
            store.get(CredentialKey::ScreenscraperPassword).unwrap(),
            None,
            "the password was still cleared"
        );
        let view = st.forget_integration("screenscraper").unwrap();
        assert!(card(&view, "screenscraper").fields.iter().all(|f| !f.set));
        for id in ["igdb", "nope", "switch-catalog", "updates"] {
            assert!(
                matches!(st.forget_integration(id), Err(IpcError::BadRequest(_))),
                "{id}"
            );
        }
    }

    #[test]
    fn a_connection_test_becomes_the_cards_health() {
        let server = MockServer::start();
        let store = Arc::new(MemoryStore::default());
        store
            .set(CredentialKey::SteamgriddbApiKey, "sk-SECRET")
            .unwrap();
        let (st, _d) = app(store, Some(&server));
        enable(&st, "steamgriddb");
        server.serve_json(
            &format!("/sgdb{}", search_path("mario")),
            r#"{"success":true,"data":[]}"#,
        );
        let ok = st.test_integration("steamgriddb").unwrap();
        assert_eq!(
            ok,
            TestResult {
                verdict: KeyCheck::Ok,
                quota: None
            }
        );
        assert!(!serde_json::to_string(&ok).unwrap().contains("SECRET"));
        let view = st.integrations_view();
        assert_eq!(card(&view, "steamgriddb").state, State::Connected);
        assert!(card(&view, "steamgriddb").checked_at.is_some());

        server.mock(
            &format!("/sgdb{}", search_path("mario")),
            MockResponse::status(401),
        );
        assert_eq!(
            st.test_integration("steamgriddb").unwrap().verdict,
            KeyCheck::Invalid
        );
        let view = st.integrations_view();
        let sgdb = card(&view, "steamgriddb");
        assert_eq!(sgdb.state, State::Error);
        assert_eq!(sgdb.reason, Some(Reason::Check(KeyCheck::Invalid)));

        for id in ["nope", "igdb", "switch-catalog", "updates"] {
            assert!(
                matches!(st.test_integration(id), Err(IpcError::BadRequest(_))),
                "{id}"
            );
        }
    }

    #[test]
    fn job_troubles_update_the_health() {
        let (st, _d) = app(full_store(), None);
        enable(&st, "steamgriddb");
        enable(&st, "screenscraper");
        let trouble = |source: &str, kind| SourceTrouble {
            source: source.into(),
            kind,
        };
        // MissingKey changes nothing.
        st.note_troubles(
            &[trouble("steamgriddb", TroubleKind::MissingKey)],
            &BTreeSet::new(),
        );
        assert_eq!(
            card(&st.integrations_view(), "steamgriddb").state,
            State::Saved
        );
        // Answered → connected.
        st.note_troubles(&[], &BTreeSet::from(["steamgriddb".to_string()]));
        assert_eq!(
            card(&st.integrations_view(), "steamgriddb").state,
            State::Connected
        );
        // Auth → error/invalid.
        st.note_troubles(
            &[trouble("steamgriddb", TroubleKind::Auth)],
            &BTreeSet::new(),
        );
        let view = st.integrations_view();
        assert_eq!(card(&view, "steamgriddb").state, State::Error);
        assert_eq!(
            card(&view, "steamgriddb").reason,
            Some(Reason::Check(KeyCheck::Invalid))
        );
        // Exhausted → quota (in a build that can use ScreenScraper), with
        // the time it was seen and no made-up numbers.
        st.note_troubles(
            &[trouble("screenscraper", TroubleKind::Exhausted)],
            &BTreeSet::new(),
        );
        let ss = card(&st.integrations_view(), "screenscraper").clone();
        assert!(ss.quota_at.is_some());
        assert_eq!(ss.quota, None);
        if DevCredentials::from_build().is_some() {
            assert_eq!(ss.state, State::Quota);
        }
        // Real numbers when a run reported them.
        let q = SsQuota {
            requests_today: 20000,
            max_requests_per_day: 20000,
            max_threads: 1,
        };
        st.note_ss_quota(q);
        let ss = card(&st.integrations_view(), "screenscraper").clone();
        assert_eq!(ss.quota, Some(q));
        // A source that is not an integration is ignored.
        st.note_troubles(&[trouble("libretro", TroubleKind::Auth)], &BTreeSet::new());
    }

    #[test]
    fn the_quota_state_is_computed_from_the_book() {
        let book = HealthBook {
            exhausted: BTreeMap::from([("screenscraper".to_string(), 100)]),
            ..HealthBook::default()
        };
        let entry = integrations::get("screenscraper").unwrap();
        let presence = Ok(vec![
            (CredentialKey::ScreenscraperUser, true),
            (CredentialKey::ScreenscraperPassword, true),
        ]);
        let mut s = Settings::default();
        for e in &mut s.sources.order {
            e.enabled = true;
        }
        let v = view_of(entry, &s, &presence, &book, 200);
        if built(entry) {
            assert_eq!(v.state, State::Quota);
        } else {
            assert_eq!(v.state, State::Unavailable);
        }
        assert_eq!(v.quota_at, Some(100));
    }

    #[test]
    fn help_links_are_the_registry_ones_only() {
        assert_eq!(
            help_url("steamgriddb").unwrap(),
            "https://www.steamgriddb.com/profile/preferences/api"
        );
        assert!(help_url("screenscraper")
            .unwrap()
            .starts_with("https://www.screenscraper.fr/"));
        for id in ["updates", "nope", "https://evil.example"] {
            assert!(matches!(help_url(id), Err(IpcError::BadRequest(_))), "{id}");
        }
    }

    #[test]
    fn log_lines_name_the_integration_and_the_verdict_only() {
        assert_eq!(
            checked_line("steamgriddb", KeyCheck::Invalid),
            "integration checked: steamgriddb -> Invalid"
        );
        assert_eq!(
            forgotten_line("screenscraper"),
            "credentials forgotten: screenscraper"
        );
    }

    #[test]
    fn keychain_errors_map_to_their_kind_without_text() {
        assert_eq!(
            keychain_kind(&CredentialError::Locked),
            KeychainKind::Locked
        );
        assert_eq!(
            keychain_kind(&CredentialError::Denied),
            KeychainKind::Denied
        );
        assert_eq!(
            keychain_kind(&CredentialError::Failed("x".into())),
            KeychainKind::Failed
        );
    }
}
