//! Source keys for the CLI (integrations spec D18, D19): the environment
//! first (`COVERLY_SGDB_KEY`, `COVERLY_SS_USER`, `COVERLY_SS_PASSWORD`),
//! then the keys the app saved in the OS keychain (same entries), read
//! lazily: only for an enabled source whose variable is empty (both
//! ScreenScraper variables, as the account is one pair), and only
//! when the run talks to the default endpoints, so a saved key never
//! reaches a host the user did not choose for it. Every keychain call has
//! a 5 s cap; a keychain that fails, times out or is off counts as empty
//! with one note. `COVERLY_NO_KEYCHAIN=1` turns the keychain off (every
//! CLI test sets it).

use std::collections::BTreeMap;
use std::sync::{mpsc, Arc};
use std::time::Duration;

use coverly_core::credentials::{
    CredentialError, CredentialKey, CredentialStore, SourceKeys, SGDB_KEY_ENV, SS_PASSWORD_ENV,
    SS_USER_ENV,
};

pub const NO_KEYCHAIN_ENV: &str = "COVERLY_NO_KEYCHAIN";
pub const KEYCHAIN_TIMEOUT: Duration = Duration::from_secs(5);

/// The OS keychain behind the 5 s cap; `None` when `COVERLY_NO_KEYCHAIN`
/// is set (to anything but `0`).
pub fn cli_store() -> Option<Arc<dyn CredentialStore>> {
    store_for(|name| std::env::var(name).ok())
}

fn store_for(env: impl Fn(&str) -> Option<String>) -> Option<Arc<dyn CredentialStore>> {
    if keychain_off(&env) {
        return None;
    }
    let keychain = Arc::new(coverly_core::credentials::keychain::KeychainStore::new());
    Some(Arc::new(TimedStore::new(keychain, KEYCHAIN_TIMEOUT)))
}

fn keychain_off(env: &impl Fn(&str) -> Option<String>) -> bool {
    env(NO_KEYCHAIN_ENV).is_some_and(|v| !matches!(v.trim(), "" | "0"))
}

/// Runs each call on a helper thread; a call that does not answer within
/// `timeout` is `Unavailable` (a Secret Service that never replies, or a
/// macOS prompt nobody sees in a cron job, must not hang a script).
pub struct TimedStore {
    inner: Arc<dyn CredentialStore>,
    timeout: Duration,
}

impl TimedStore {
    pub fn new(inner: Arc<dyn CredentialStore>, timeout: Duration) -> TimedStore {
        TimedStore { inner, timeout }
    }

    fn run<T: Send + 'static>(
        &self,
        call: impl FnOnce(&dyn CredentialStore) -> T + Send + 'static,
    ) -> Option<T> {
        let (tx, rx) = mpsc::channel();
        let inner = Arc::clone(&self.inner);
        std::thread::Builder::new()
            .name("keychain".into())
            .spawn(move || {
                let _ = tx.send(call(inner.as_ref()));
            })
            .ok()?;
        rx.recv_timeout(self.timeout).ok()
    }
}

impl CredentialStore for TimedStore {
    fn available(&self) -> bool {
        self.run(|s| s.available()).unwrap_or(false)
    }
    fn get(&self, key: CredentialKey) -> Result<Option<String>, CredentialError> {
        self.run(move |s| s.get(key))
            .unwrap_or(Err(CredentialError::Unavailable))
    }
    fn set(&self, key: CredentialKey, value: &str) -> Result<(), CredentialError> {
        let value = value.to_string();
        self.run(move |s| s.set(key, &value))
            .unwrap_or(Err(CredentialError::Unavailable))
    }
    fn clear(&self, key: CredentialKey) -> Result<(), CredentialError> {
        self.run(move |s| s.clear(key))
            .unwrap_or(Err(CredentialError::Unavailable))
    }
}

/// Which keys a run may need.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyPlan {
    /// SteamGridDB is enabled for this run.
    pub steamgriddb: bool,
    /// ScreenScraper is enabled for this run (and this build has it).
    pub screenscraper: bool,
    /// No `--…-url` override points an enabled keyed source elsewhere.
    pub default_endpoints: bool,
}

/// One word for a keychain failure in a note (never a value).
fn failure_word(e: &CredentialError) -> &'static str {
    match e {
        CredentialError::Unavailable => "unavailable",
        CredentialError::Locked => "locked",
        CredentialError::Denied => "denied",
        CredentialError::Failed(_) | CredentialError::BadValue => "failed",
    }
}

fn is_empty(v: &Option<String>) -> bool {
    v.as_deref().is_none_or(|v| v.trim().is_empty())
}

/// Env first; keychain only for enabled sources with an empty env var and
/// only towards default endpoints (D19). `note` gets at most one line.
pub fn resolve_keys(
    env: impl Fn(&str) -> Option<String>,
    store: Option<&dyn CredentialStore>,
    plan: KeyPlan,
    note: &mut dyn FnMut(&str),
) -> SourceKeys {
    let mut wanted: Vec<(&str, CredentialKey)> = Vec::new();
    if plan.steamgriddb {
        wanted.push((SGDB_KEY_ENV, CredentialKey::SteamgriddbApiKey));
    }
    wanted.retain(|(var, _)| is_empty(&env(var)));
    // The account is one pair: the saved password only ever goes with the
    // saved user, so either variable set keeps the keychain out of it.
    if plan.screenscraper && is_empty(&env(SS_USER_ENV)) && is_empty(&env(SS_PASSWORD_ENV)) {
        wanted.push((SS_USER_ENV, CredentialKey::ScreenscraperUser));
        wanted.push((SS_PASSWORD_ENV, CredentialKey::ScreenscraperPassword));
    }

    let mut saved: BTreeMap<&str, String> = BTreeMap::new();
    if !wanted.is_empty() {
        match store {
            _ if !plan.default_endpoints => {
                note("saved keys are not sent to a custom endpoint; only the environment is used");
            }
            None => note(&format!(
                "keychain: off ({NO_KEYCHAIN_ENV}); only the environment is used"
            )),
            Some(store) => {
                for (var, key) in wanted {
                    match store.get(key) {
                        Ok(Some(v)) => {
                            saved.insert(var, v);
                        }
                        Ok(None) => {}
                        Err(e) => {
                            // One note, and no more waits on a store that failed.
                            note(&format!(
                                "keychain: {}; saved keys are not used this run",
                                failure_word(&e)
                            ));
                            break;
                        }
                    }
                }
            }
        }
    }
    SourceKeys::from_env(|name| {
        let v = env(name);
        if is_empty(&v) {
            saved.get(name).cloned()
        } else {
            v
        }
    })
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use coverly_core::credentials::MemoryStore;

    use super::*;

    const ALL: KeyPlan = KeyPlan {
        steamgriddb: true,
        screenscraper: true,
        default_endpoints: true,
    };

    fn saved() -> MemoryStore {
        let s = MemoryStore::default();
        s.set(CredentialKey::SteamgriddbApiKey, "sk-saved").unwrap();
        s.set(CredentialKey::ScreenscraperUser, "me").unwrap();
        s.set(CredentialKey::ScreenscraperPassword, "pw-saved")
            .unwrap();
        s
    }

    fn no_env(_: &str) -> Option<String> {
        None
    }

    fn resolve(
        env: impl Fn(&str) -> Option<String>,
        store: Option<&dyn CredentialStore>,
        plan: KeyPlan,
    ) -> (SourceKeys, Vec<String>) {
        let mut notes = Vec::new();
        let keys = resolve_keys(env, store, plan, &mut |m| notes.push(m.to_string()));
        (keys, notes)
    }

    fn sgdb(keys: &SourceKeys) -> Option<&str> {
        keys.steamgriddb.as_ref().map(|s| s.expose())
    }

    #[test]
    fn an_environment_key_wins_and_the_store_is_not_read() {
        let store = saved();
        let env = |n: &str| {
            [
                (SGDB_KEY_ENV, "sk-env"),
                (SS_USER_ENV, "env-user"),
                (SS_PASSWORD_ENV, "env-pw"),
            ]
            .into_iter()
            .find(|(k, _)| *k == n)
            .map(|(_, v)| v.to_string())
        };
        let (keys, notes) = resolve(env, Some(&store), ALL);
        assert_eq!(sgdb(&keys), Some("sk-env"));
        let user = keys.screenscraper_user.unwrap();
        assert_eq!(user.user.expose(), "env-user");
        assert_eq!(store.reads(), 0);
        assert!(notes.is_empty(), "{notes:?}");
    }

    #[test]
    fn an_empty_variable_falls_back_to_the_saved_key() {
        let store = saved();
        let env = |n: &str| (n == SGDB_KEY_ENV).then(|| "  ".to_string());
        let (keys, notes) = resolve(env, Some(&store), ALL);
        assert_eq!(sgdb(&keys), Some("sk-saved"));
        let user = keys.screenscraper_user.unwrap();
        assert_eq!(user.user.expose(), "me");
        assert_eq!(user.password.expose(), "pw-saved");
        assert!(notes.is_empty(), "{notes:?}");
    }

    #[test]
    fn a_half_set_account_in_the_environment_is_not_mixed_with_the_saved_one() {
        // The saved password belongs to the saved user: it is never sent
        // with another user name (and the saved user never with another
        // password). Half an account in the environment means no account.
        let store = saved();
        for var in [SS_USER_ENV, SS_PASSWORD_ENV] {
            let env = move |n: &str| (n == var).then(|| "from-env".to_string());
            let (keys, _) = resolve(env, Some(&store), ALL);
            assert_eq!(keys.screenscraper_user, None, "{var}");
            // SteamGridDB still comes from the keychain.
            assert_eq!(sgdb(&keys), Some("sk-saved"), "{var}");
        }
        assert_eq!(store.reads(), 2);
    }

    #[test]
    fn saved_keys_never_go_to_a_custom_endpoint() {
        let store = saved();
        let plan = KeyPlan {
            default_endpoints: false,
            ..ALL
        };
        let (keys, notes) = resolve(no_env, Some(&store), plan);
        assert_eq!(keys.steamgriddb, None);
        assert_eq!(keys.screenscraper_user, None);
        assert_eq!(store.reads(), 0);
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(notes[0].contains("saved keys are not sent to a custom endpoint"));
    }

    #[test]
    fn a_disabled_source_does_not_read_the_store() {
        let store = saved();
        let plan = KeyPlan {
            steamgriddb: false,
            screenscraper: false,
            default_endpoints: true,
        };
        let (keys, notes) = resolve(no_env, Some(&store), plan);
        assert_eq!(keys.steamgriddb, None);
        assert_eq!(store.reads(), 0);
        assert!(notes.is_empty(), "{notes:?}");
    }

    #[test]
    fn only_the_enabled_source_is_read() {
        let store = saved();
        let plan = KeyPlan {
            screenscraper: false,
            ..ALL
        };
        let (keys, _) = resolve(no_env, Some(&store), plan);
        assert_eq!(sgdb(&keys), Some("sk-saved"));
        assert_eq!(keys.screenscraper_user, None);
        assert_eq!(store.reads(), 1);
    }

    #[test]
    fn an_unavailable_store_is_empty_with_one_note() {
        let store = MemoryStore::unavailable();
        let (keys, notes) = resolve(no_env, Some(&store), ALL);
        assert_eq!(keys.steamgriddb, None);
        assert_eq!(keys.screenscraper_user, None);
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(notes[0].starts_with("keychain: unavailable"), "{notes:?}");
        // It gave up after the first failure.
        assert_eq!(store.reads(), 1);
    }

    #[test]
    fn a_keychain_turned_off_is_empty_with_one_note() {
        let (keys, notes) = resolve(no_env, None, ALL);
        assert_eq!(keys.steamgriddb, None);
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(notes[0].starts_with("keychain: off"), "{notes:?}");
    }

    #[test]
    fn notes_never_hold_a_value() {
        let store = saved();
        let plan = KeyPlan {
            default_endpoints: false,
            ..ALL
        };
        let (_, notes) = resolve(no_env, Some(&store), plan);
        for n in notes {
            assert!(!n.contains("sk-saved") && !n.contains("pw-saved"), "{n}");
        }
    }

    struct Slow(Duration);

    impl CredentialStore for Slow {
        fn available(&self) -> bool {
            std::thread::sleep(self.0);
            true
        }
        fn get(&self, _: CredentialKey) -> Result<Option<String>, CredentialError> {
            std::thread::sleep(self.0);
            Ok(Some("late".into()))
        }
        fn set(&self, _: CredentialKey, _: &str) -> Result<(), CredentialError> {
            std::thread::sleep(self.0);
            Ok(())
        }
        fn clear(&self, _: CredentialKey) -> Result<(), CredentialError> {
            std::thread::sleep(self.0);
            Ok(())
        }
    }

    #[test]
    fn a_store_that_does_not_answer_times_out_as_unavailable() {
        let store = TimedStore::new(
            Arc::new(Slow(Duration::from_millis(200))),
            Duration::from_millis(50),
        );
        let started = Instant::now();
        assert_eq!(
            store.get(CredentialKey::SteamgriddbApiKey),
            Err(CredentialError::Unavailable)
        );
        assert!(started.elapsed() < Duration::from_millis(150));
        assert!(!store.available());
        assert_eq!(
            store.set(CredentialKey::SteamgriddbApiKey, "x"),
            Err(CredentialError::Unavailable)
        );
    }

    #[test]
    fn a_store_that_answers_in_time_passes_through() {
        let inner = Arc::new(saved());
        let store = TimedStore::new(inner, Duration::from_secs(5));
        assert_eq!(
            store.get(CredentialKey::SteamgriddbApiKey),
            Ok(Some("sk-saved".into()))
        );
        store.clear(CredentialKey::SteamgriddbApiKey).unwrap();
        assert_eq!(store.get(CredentialKey::SteamgriddbApiKey), Ok(None));
    }

    #[test]
    fn no_keychain_turns_the_store_off() {
        let env = |v: &'static str| move |n: &str| (n == NO_KEYCHAIN_ENV).then(|| v.to_string());
        assert!(store_for(env("1")).is_none());
        assert!(store_for(env("yes")).is_none());
        assert!(!keychain_off(&env("0")));
        assert!(!keychain_off(&env("")));
        assert!(!keychain_off(&no_env));
    }
}
