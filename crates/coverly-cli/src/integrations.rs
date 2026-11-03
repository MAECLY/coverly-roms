//! `coverly integrations`: the accounts and keys of online services, from
//! the same registry and the same keychain entries as the app's
//! «Integraciones» tab (integrations spec D18–D20).
//!
//! No command takes a secret as an argument: `set` reads it from stdin
//! (no echo on a terminal) or from the variable `--from-env` names, and a
//! stray positional after the id is bad usage. Outputs say `set` /
//! `not set`, never a value, and `list` never prints `network.proxy_url`.
//!
//! Exit codes (the table in `main.rs`): 0 success; 1 the test said
//! `invalid`, `missing` or `unavailable`, or a keychain error (also «the
//! keychain is off»); 2 bad usage, an unknown or planned id, a service
//! without credentials or test, or `forget` without a terminal or
//! `--yes`; 5 the test said `offline` or `rate-limited`.

use std::io::{self, BufRead, IsTerminal, Read, Write};

use clap::{Args, Subcommand, ValueEnum};
use coverly_core::credentials::{
    store_secret, CredentialError, CredentialKey, CredentialStore, DevCredentials, SGDB_KEY_ENV,
    SS_PASSWORD_ENV, SS_USER_ENV,
};
use coverly_core::integrations::{self, Field, Input, Integration, SsQuota, Status, TestKind};
use coverly_core::net::{key_check_net, PoliteHttp, UreqHttp};
use coverly_core::settings::Settings;
use coverly_core::sources::screenscraper::{check_account, SsConfig, SCREENSCRAPER_BASE};
use coverly_core::sources::steamgriddb::{self, STEAMGRIDDB_BASE};
use coverly_core::sources::KeyCheck;
use coverly_core::term::EchoOff;
use serde_json::{json, Value};

use crate::keys::{cli_store, failure_word, resolve_keys, KeyPlan, NO_KEYCHAIN_ENV};
use crate::{print_stdout, Failure};

/// The longest value `set` reads (the keychain refuses more than 512
/// characters anyway; this only bounds the read).
const MAX_READ_BYTES: u64 = 4096;

#[derive(Debug, Args)]
pub struct IntegrationsArgs {
    #[command(subcommand)]
    pub command: IntegrationsCommand,
}

#[derive(Debug, Subcommand)]
pub enum IntegrationsCommand {
    /// Every integration, whether it is on and which credentials are set
    /// (never their values).
    List {
        #[arg(long)]
        json: bool,
    },
    /// One authenticated request with the environment or saved
    /// credentials (saved ones only towards the service's own address).
    Test {
        id: String,
        #[arg(long)]
        json: bool,
        #[arg(long, hide = true)]
        steamgriddb_url: Option<String>,
        #[arg(long, hide = true)]
        screenscraper_url: Option<String>,
        #[arg(hide = true)]
        stray: Vec<String>,
    },
    /// Save a credential in the system keychain, read from stdin (no echo
    /// on a terminal) or from the variable --from-env names.
    Set {
        id: String,
        /// Which credential, for a service with more than one.
        #[arg(long)]
        field: Option<FieldArg>,
        /// Read the value from this environment variable instead of stdin.
        #[arg(long, value_name = "VAR")]
        from_env: Option<String>,
        /// Caught here so a secret typed after the id is refused without
        /// clap repeating it on stderr.
        #[arg(hide = true)]
        stray: Vec<String>,
    },
    /// Delete the integration's credentials from the keychain.
    Forget {
        id: String,
        /// Do not ask (needed without a terminal).
        #[arg(long)]
        yes: bool,
        #[arg(hide = true)]
        stray: Vec<String>,
    },
}

/// A credential field, by the kind of input it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum FieldArg {
    User,
    Password,
    ApiKey,
}

impl FieldArg {
    fn input(self) -> Input {
        match self {
            FieldArg::User => Input::User,
            FieldArg::Password => Input::Password,
            FieldArg::ApiKey => Input::ApiKey,
        }
    }
}

/// Bad usage for anything after the id, without repeating it: it may be
/// a secret typed in the wrong place.
fn no_stray(stray: &[String]) -> Result<(), Failure> {
    if stray.is_empty() {
        return Ok(());
    }
    Err(Failure::usage(
        "unexpected argument after the integration id (not shown); a value is read from stdin \
         or --from-env, never as an argument",
    ))
}

pub fn run(args: &IntegrationsArgs) -> Result<(), Failure> {
    match &args.command {
        IntegrationsCommand::List { json } => {
            let settings = crate::settings::load_cli_settings().settings;
            let store = cli_store();
            if *json {
                let text = serde_json::to_string_pretty(&list_json(store.as_deref(), &settings))
                    .map_err(|e| failed(e.to_string()))?;
                print_stdout(&format!("{text}\n"))
            } else {
                print_stdout(&list_text(store.as_deref(), &settings))
            }
        }
        IntegrationsCommand::Test {
            id,
            json,
            steamgriddb_url,
            screenscraper_url,
            stray,
        } => no_stray(stray).and_then(|()| {
            run_test(
                id,
                *json,
                steamgriddb_url.as_deref(),
                screenscraper_url.as_deref(),
            )
        }),
        IntegrationsCommand::Set {
            id,
            field,
            from_env,
            stray,
        } => no_stray(stray).and_then(|()| run_set(id, *field, from_env.as_deref())),
        IntegrationsCommand::Forget { id, yes, stray } => {
            no_stray(stray).and_then(|()| run_forget(id, *yes))
        }
    }
}

fn failed(message: impl Into<String>) -> Failure {
    Failure {
        code: 1,
        message: message.into(),
    }
}

fn keychain_off() -> Failure {
    failed(format!(
        "the keychain is off ({NO_KEYCHAIN_ENV}); nothing was changed"
    ))
}

/// What to do about a keychain error on this machine (spec D16, the
/// CLI's words for `errors.keychainOs.*`). Never holds a value.
pub fn keychain_message(e: &CredentialError) -> String {
    keychain_message_on(e, std::env::consts::OS)
}

fn keychain_message_on(e: &CredentialError, os: &str) -> String {
    let advice = match (e, os) {
        (CredentialError::Locked, "macos") => {
            "the macOS keychain is locked; unlock it in Keychain Access, or over SSH run \
             `security unlock-keychain` first"
        }
        (CredentialError::Locked, "linux") => {
            "the keyring is locked; unlock it in Passwords and Keys or KWalletManager and try again"
        }
        (CredentialError::Locked, _) => "the keychain is locked; unlock it and try again",
        (CredentialError::Denied, "macos") => {
            "macOS did not allow access to the keychain; try again and choose «Allow» in the prompt"
        }
        (CredentialError::Denied, "windows") => {
            "Windows denied access to Credential Manager; check that your account has no policy \
             restrictions"
        }
        (CredentialError::Denied, "linux") => {
            "the secret service denied access; accept the unlock prompt (on Flatpak, allow \
             org.freedesktop.secrets)"
        }
        (CredentialError::Denied, _) => "the system did not allow access to the keychain",
        (CredentialError::Unavailable, "macos") => {
            "the login keychain cannot be found; check it in Keychain Access"
        }
        (CredentialError::Unavailable, "windows") => {
            "Credential Manager is not available in this session (a service, a scheduled task or \
             SSH without a loaded profile); run coverly from your own Windows session, or use the \
             environment variables"
        }
        (CredentialError::Unavailable, "linux") => {
            "no secret service is running, or it has no default keyring (SSH, headless, a \
             sandbox); start GNOME Keyring or KWallet, or use the environment variables"
        }
        (CredentialError::Unavailable, _) => {
            "this system has no credential store (keychain) coverly can use"
        }
        (CredentialError::BadValue, _) => {
            "the value is not valid: it holds control characters or is longer than 512 characters"
        }
        (CredentialError::Failed(detail), _) => {
            return format!("the keychain refused the request: {detail}")
        }
    };
    advice.to_string()
}

/// The working integration `id`, or bad usage naming why not.
fn working(id: &str) -> Result<&'static Integration, Failure> {
    match integrations::get(id) {
        None => Err(Failure::usage(format!(
            "unknown integration «{id}»; `coverly integrations list` names them"
        ))),
        Some(e) if e.status == Status::Planned => Err(Failure::usage(format!(
            "«{id}» ({}) is coming later; it takes no credentials and has no test yet",
            e.name
        ))),
        Some(e) => Ok(e),
    }
}

/// The working integration `id` that takes credentials.
fn with_credentials(id: &str) -> Result<&'static Integration, Failure> {
    let e = working(id)?;
    if e.fields.is_empty() {
        return Err(Failure::usage(format!("«{id}» has no credentials")));
    }
    Ok(e)
}

fn input_word(i: Input) -> &'static str {
    match i {
        Input::User => "user",
        Input::Password => "password",
        Input::ApiKey => "api-key",
        Input::ClientId => "client-id",
        Input::Secret => "secret",
    }
}

fn choices(e: &Integration) -> String {
    e.fields
        .iter()
        .map(|f| format!("--field {}", input_word(f.input)))
        .collect::<Vec<_>>()
        .join(" or ")
}

/// The field of `id` that `set` writes.
fn target_field(id: &str, field: Option<FieldArg>) -> Result<&'static Field, Failure> {
    let e = with_credentials(id)?;
    match field {
        None if e.fields.len() == 1 => Ok(&e.fields[0]),
        None => Err(Failure::usage(format!(
            "«{id}» has more than one credential; choose {}",
            choices(e)
        ))),
        Some(f) => e
            .fields
            .iter()
            .find(|x| x.input == f.input())
            .ok_or_else(|| {
                Failure::usage(format!(
                    "«{id}» has no {} field; choose {}",
                    input_word(f.input()),
                    choices(e)
                ))
            }),
    }
}

/// Reads one line from `read` (CR/LF and surrounding spaces trimmed).
fn read_line(read: &mut dyn BufRead) -> Result<String, Failure> {
    let mut line = String::new();
    read.take(MAX_READ_BYTES)
        .read_line(&mut line)
        .map_err(|e| match e.kind() {
            io::ErrorKind::InvalidData => failed("the value read from stdin is not text"),
            _ => failed(format!("cannot read stdin: {e}")),
        })?;
    let value = line.trim().to_string();
    if value.is_empty() {
        return Err(failed("no value read from stdin"));
    }
    Ok(value)
}

/// Saves the value of one field of `id`: from the variable `from_env`
/// when given, else one line of `read`. Errors never hold the value.
pub fn set_value(
    store: &dyn CredentialStore,
    id: &str,
    field: Option<FieldArg>,
    read: &mut dyn BufRead,
    env: &dyn Fn(&str) -> Option<String>,
    from_env: Option<&str>,
) -> Result<(), Failure> {
    let target = target_field(id, field)?;
    let value = match from_env {
        Some(var) => match env(var) {
            Some(v) if !v.trim().is_empty() => v.trim().to_string(),
            _ => return Err(failed(format!("the variable {var} is not set or is empty"))),
        },
        None => read_line(read)?,
    };
    store_secret(store, target.key, &value).map_err(|e| failed(keychain_message(&e)))
}

fn run_set(id: &str, field: Option<FieldArg>, from_env: Option<&str>) -> Result<(), Failure> {
    // Usage first: a bad id or field is the same mistake with or without
    // a keychain.
    let target = target_field(id, field)?;
    let store = cli_store().ok_or_else(keychain_off)?;
    let stdin = io::stdin();
    let typed = from_env.is_none() && stdin.is_terminal();
    let guard = if typed {
        eprint!(
            "Value for {id} {} (input hidden): ",
            input_word(target.input)
        );
        let _ = io::stderr().flush();
        // Without echo control the value would show: refuse rather than
        // print it on the screen.
        Some(EchoOff::stdin().map_err(|e| {
            failed(format!(
                "cannot hide the input on this terminal ({e}); pipe the value in or use --from-env"
            ))
        })?)
    } else {
        None
    };
    let env = |name: &str| std::env::var(name).ok();
    let result = set_value(store.as_ref(), id, field, &mut stdin.lock(), &env, from_env);
    drop(guard);
    if typed && cfg!(windows) {
        // Windows does not echo the Enter either.
        eprintln!();
    }
    result?;
    print_stdout(&format!(
        "{id} {}: saved in the system keychain\n",
        input_word(target.input)
    ))
}

/// Deletes every credential of `id` from `store`.
pub fn forget_keys(store: &dyn CredentialStore, id: &str) -> Result<(), Failure> {
    let e = with_credentials(id)?;
    for f in &e.fields {
        store
            .clear(f.key)
            .map_err(|err| failed(keychain_message(&err)))?;
    }
    Ok(())
}

fn run_forget(id: &str, yes: bool) -> Result<(), Failure> {
    with_credentials(id)?;
    let stdin = io::stdin();
    if !yes && !stdin.is_terminal() {
        return Err(Failure::usage(
            "forget asks before deleting; pass --yes to forget without a terminal",
        ));
    }
    // Before the question: no point asking when nothing can be deleted.
    let store = cli_store().ok_or_else(keychain_off)?;
    if !yes {
        eprint!("Forget the {id} credentials? [y/N] ");
        let _ = io::stderr().flush();
        let mut answer = String::new();
        stdin.lock().read_line(&mut answer)?;
        if !matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
            return print_stdout("nothing was forgotten\n");
        }
    }
    forget_keys(store.as_ref(), id)?;
    print_stdout(&format!(
        "{id}: credentials deleted from the system keychain\n"
    ))
}

/// The keychain as `list` reports it, and which keys hold a value
/// (`None`: could not be read).
struct Presence {
    keychain: &'static str,
    set: Vec<(CredentialKey, Option<bool>)>,
}

fn presence(store: Option<&dyn CredentialStore>) -> Presence {
    let Some(store) = store else {
        return Presence {
            keychain: "off",
            set: CredentialKey::ALL.iter().map(|k| (*k, None)).collect(),
        };
    };
    if !store.available() {
        return Presence {
            keychain: "unavailable",
            set: CredentialKey::ALL.iter().map(|k| (*k, None)).collect(),
        };
    }
    let mut keychain = "available";
    let mut set = Vec::new();
    for k in CredentialKey::ALL {
        if keychain != "available" {
            // One failure is enough: no more waits on a store that failed.
            set.push((k, None));
            continue;
        }
        match store.get(k) {
            Ok(v) => set.push((k, Some(v.is_some_and(|v| !v.trim().is_empty())))),
            Err(e) => {
                keychain = failure_word(&e);
                set.push((k, None));
            }
        }
    }
    Presence { keychain, set }
}

impl Presence {
    fn of(&self, key: CredentialKey) -> Option<bool> {
        self.set
            .iter()
            .find(|(k, _)| *k == key)
            .and_then(|(_, v)| *v)
    }
}

fn planned() -> impl Iterator<Item = &'static Integration> {
    integrations::all()
        .iter()
        .filter(|e| e.status == Status::Planned)
}

/// `coverly integrations list`: one line per working integration, the
/// keychain state and the planned services. Never a value.
pub fn list_text(store: Option<&dyn CredentialStore>, settings: &Settings) -> String {
    let p = presence(store);
    let mut out = format!("keychain: {}\n", p.keychain);
    for e in integrations::working() {
        let mut line = e.id.clone();
        if e.id != "updates" {
            line.push_str(if e.enabled(settings) { "  on" } else { "  off" });
        } else {
            line.push_str("  channel in the app's General settings");
        }
        if !e.built() {
            line.push_str("  (not in this build)");
        }
        if !e.fields.is_empty() {
            let fields: Vec<String> = e
                .fields
                .iter()
                .map(|f| {
                    let state = match p.of(f.key) {
                        Some(true) => "set",
                        Some(false) => "not set",
                        None => "unknown",
                    };
                    format!("{}: {state}", input_word(f.input))
                })
                .collect();
            line.push_str("  ");
            line.push_str(&fields.join(", "));
        }
        out.push_str(&line);
        out.push('\n');
    }
    let names: Vec<&str> = planned().map(|e| e.name.as_str()).collect();
    if !names.is_empty() {
        out.push_str(&format!("coming later: {}\n", names.join(", ")));
    }
    out
}

/// `list --json`: `{ keychain, integrations: [{ id, enabled, fields:
/// [{ key, set }] }], planned: [ids] }`; `set` is `null` when the
/// keychain could not say.
pub fn list_json(store: Option<&dyn CredentialStore>, settings: &Settings) -> Value {
    let p = presence(store);
    let entries: Vec<Value> = integrations::working()
        .map(|e| {
            let fields: Vec<Value> = e
                .fields
                .iter()
                .map(|f| json!({ "key": f.key.account(), "set": p.of(f.key) }))
                .collect();
            json!({ "id": e.id, "enabled": e.enabled(settings), "fields": fields })
        })
        .collect();
    let planned: Vec<&str> = planned().map(|e| e.id.as_str()).collect();
    json!({ "keychain": p.keychain, "integrations": entries, "planned": planned })
}

/// The exit code of a connection test (spec D20).
pub fn exit_for(check: KeyCheck) -> u8 {
    match check {
        KeyCheck::Ok => 0,
        KeyCheck::Invalid | KeyCheck::Missing | KeyCheck::Unavailable => 1,
        KeyCheck::Offline | KeyCheck::RateLimited => 5,
    }
}

fn verdict_word(check: KeyCheck) -> &'static str {
    match check {
        KeyCheck::Ok => "ok",
        KeyCheck::Invalid => "invalid",
        KeyCheck::Missing => "missing",
        KeyCheck::Offline => "offline",
        KeyCheck::RateLimited => "rate-limited",
        KeyCheck::Unavailable => "unavailable",
    }
}

/// What to do after a failed test, for stderr.
fn verdict_hint(id: &str, check: KeyCheck, built: bool) -> String {
    let vars = match id {
        "screenscraper" => format!("{SS_USER_ENV} and {SS_PASSWORD_ENV}"),
        _ => SGDB_KEY_ENV.to_string(),
    };
    match check {
        KeyCheck::Ok => String::new(),
        KeyCheck::Invalid => format!(
            "the service refused the credentials; save new ones with `coverly integrations set {id}`"
        ),
        KeyCheck::Missing => format!(
            "no credentials: set {vars}, or save them with `coverly integrations set {id}`"
        ),
        KeyCheck::Unavailable if !built => {
            "this build has no ScreenScraper developer credentials".to_string()
        }
        KeyCheck::Unavailable => {
            "the service gave an answer coverly does not understand; try again later".to_string()
        }
        KeyCheck::Offline => {
            "could not reach the service; check the connection, or the proxy in the app's \
             Network settings"
                .to_string()
        }
        KeyCheck::RateLimited => {
            "the service is limiting requests or today's quota is used up; try again later"
                .to_string()
        }
    }
}

fn run_test(
    id: &str,
    as_json: bool,
    steamgriddb_url: Option<&str>,
    screenscraper_url: Option<&str>,
) -> Result<(), Failure> {
    let e = working(id)?;
    let Some(kind) = e.test else {
        return Err(Failure::usage(format!("«{id}» has no connection test")));
    };
    let settings = crate::settings::load_cli_settings().settings;
    let net = key_check_net(&settings.net_config());
    let http = PoliteHttp::new(UreqHttp::new(&net), &net);
    let dev = DevCredentials::from_build();
    let plan = match kind {
        TestKind::Steamgriddb => KeyPlan {
            steamgriddb: true,
            screenscraper: false,
            default_endpoints: steamgriddb_url.is_none(),
        },
        TestKind::Screenscraper => KeyPlan {
            steamgriddb: false,
            screenscraper: dev.is_some(),
            default_endpoints: screenscraper_url.is_none(),
        },
    };
    let keychain = cli_store();
    let keys = resolve_keys(
        |name| std::env::var(name).ok(),
        keychain.as_deref(),
        plan,
        &mut |m| eprintln!("note: {m}"),
    );
    let (verdict, quota): (KeyCheck, Option<SsQuota>) = match kind {
        TestKind::Steamgriddb => (
            steamgriddb::check_key(
                &http,
                steamgriddb_url.unwrap_or(STEAMGRIDDB_BASE),
                keys.steamgriddb.as_ref(),
            ),
            None,
        ),
        TestKind::Screenscraper => match &dev {
            None => (KeyCheck::Unavailable, None),
            Some(dev) => {
                let ss = SsConfig {
                    base: screenscraper_url.unwrap_or(SCREENSCRAPER_BASE).to_string(),
                    ..SsConfig::default()
                };
                let c = check_account(&http, &ss, dev, keys.screenscraper_user.as_ref());
                (c.verdict, c.quota)
            }
        },
    };
    let out = if as_json {
        let mut v = json!({ "id": id, "verdict": verdict_word(verdict) });
        if let Some(q) = quota {
            v["quota"] = serde_json::to_value(q).map_err(|e| failed(e.to_string()))?;
        }
        format!(
            "{}\n",
            serde_json::to_string_pretty(&v).map_err(|e| failed(e.to_string()))?
        )
    } else {
        let mut text = format!("{id}: {}\n", verdict_word(verdict));
        if let Some(q) = quota {
            text.push_str(&format!(
                "quota: {} of {} requests today, {} threads\n",
                q.requests_today, q.max_requests_per_day, q.max_threads
            ));
        }
        text
    };
    print_stdout(&out)?;
    match exit_for(verdict) {
        0 => Ok(()),
        code => Err(Failure {
            code,
            message: verdict_hint(id, verdict, dev.is_some() || id != "screenscraper"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use coverly_core::credentials::MemoryStore;

    use super::*;

    fn no_env(_: &str) -> Option<String> {
        None
    }

    fn set(
        store: &MemoryStore,
        id: &str,
        field: Option<FieldArg>,
        stdin: &str,
    ) -> Result<(), Failure> {
        set_value(
            store,
            id,
            field,
            &mut Cursor::new(stdin.as_bytes().to_vec()),
            &no_env,
            None,
        )
    }

    #[test]
    fn set_reads_one_line_and_trims_crlf() {
        let store = MemoryStore::default();
        set(&store, "steamgriddb", None, "sk-123\r\nnext line\n").unwrap();
        assert_eq!(
            store.get(CredentialKey::SteamgriddbApiKey),
            Ok(Some("sk-123".into()))
        );
    }

    #[test]
    fn set_reads_the_named_variable() {
        let store = MemoryStore::default();
        let env = |n: &str| (n == "VAR").then(|| "sk-9".to_string());
        set_value(
            &store,
            "steamgriddb",
            None,
            &mut Cursor::new(Vec::new()),
            &env,
            Some("VAR"),
        )
        .unwrap();
        assert_eq!(
            store.get(CredentialKey::SteamgriddbApiKey),
            Ok(Some("sk-9".into()))
        );
    }

    #[test]
    fn empty_stdin_is_an_error() {
        let store = MemoryStore::default();
        for stdin in ["", "\n", "   \r\n"] {
            let f = set(&store, "steamgriddb", None, stdin).unwrap_err();
            assert_eq!(f.code, 1);
            assert_eq!(f.message, "no value read from stdin");
        }
        assert_eq!(store.get(CredentialKey::SteamgriddbApiKey), Ok(None));
    }

    #[test]
    fn an_unset_variable_is_named_and_nothing_else() {
        let store = MemoryStore::default();
        let f = set_value(
            &store,
            "steamgriddb",
            None,
            &mut Cursor::new(b"sk-stdin\n".to_vec()),
            &no_env,
            Some("UNSET"),
        )
        .unwrap_err();
        assert_eq!(f.code, 1);
        assert!(f.message.contains("UNSET"), "{}", f.message);
        assert!(!f.message.contains("sk-stdin"));
    }

    #[test]
    fn screenscraper_needs_a_field() {
        let store = MemoryStore::default();
        let f = set(&store, "screenscraper", None, "me\n").unwrap_err();
        assert_eq!(f.code, 2);
        assert!(
            f.message
                .contains("choose --field user or --field password"),
            "{}",
            f.message
        );
        set(&store, "screenscraper", Some(FieldArg::User), "me\n").unwrap();
        set(&store, "screenscraper", Some(FieldArg::Password), "pw\n").unwrap();
        assert_eq!(
            store.get(CredentialKey::ScreenscraperPassword),
            Ok(Some("pw".into()))
        );
    }

    #[test]
    fn a_field_the_service_does_not_have_is_bad_usage() {
        let store = MemoryStore::default();
        let f = set(&store, "steamgriddb", Some(FieldArg::Password), "x\n").unwrap_err();
        assert_eq!(f.code, 2);
        assert!(f.message.contains("--field api-key"), "{}", f.message);
    }

    #[test]
    fn planned_unknown_and_credential_less_ids_are_bad_usage() {
        let store = MemoryStore::default();
        for id in ["igdb", "retroachievements", "nope"] {
            assert_eq!(set(&store, id, None, "x\n").unwrap_err().code, 2, "{id}");
        }
        for id in ["switch-catalog", "updates"] {
            let f = set(&store, id, None, "x\n").unwrap_err();
            assert_eq!(f.code, 2, "{id}");
            assert!(f.message.contains("has no credentials"), "{}", f.message);
        }
        assert_eq!(store.reads(), 0);
    }

    #[test]
    fn a_value_with_control_characters_is_refused_without_echo() {
        let store = MemoryStore::default();
        let f = set(&store, "steamgriddb", None, "sk\u{7}bad\n").unwrap_err();
        assert_eq!(f.code, 1);
        assert!(!f.message.contains("sk"), "{}", f.message);
    }

    #[test]
    fn an_unavailable_keychain_fails_set_with_advice() {
        let store = MemoryStore::unavailable();
        let f = set(&store, "steamgriddb", None, "sk-1\n").unwrap_err();
        assert_eq!(f.code, 1);
        assert!(!f.message.contains("sk-1"));
    }

    #[test]
    fn forget_clears_every_field() {
        let store = MemoryStore::default();
        store.set(CredentialKey::ScreenscraperUser, "me").unwrap();
        store
            .set(CredentialKey::ScreenscraperPassword, "pw")
            .unwrap();
        store.set(CredentialKey::SteamgriddbApiKey, "sk").unwrap();
        forget_keys(&store, "screenscraper").unwrap();
        assert_eq!(store.get(CredentialKey::ScreenscraperUser), Ok(None));
        assert_eq!(store.get(CredentialKey::ScreenscraperPassword), Ok(None));
        assert_eq!(
            store.get(CredentialKey::SteamgriddbApiKey),
            Ok(Some("sk".into()))
        );
        assert_eq!(forget_keys(&store, "updates").unwrap_err().code, 2);
        assert_eq!(forget_keys(&store, "igdb").unwrap_err().code, 2);
    }

    fn settings_with_secrets() -> Settings {
        let mut s = Settings::default();
        for e in &mut s.sources.order {
            e.enabled = e.id == "steamgriddb" || e.id == "libretro";
        }
        s.network.proxy_url = "http://user:proxy-pass@proxy.example:8080".into();
        s
    }

    fn saved() -> MemoryStore {
        let store = MemoryStore::default();
        store
            .set(CredentialKey::SteamgriddbApiKey, "sk-saved")
            .unwrap();
        store
            .set(CredentialKey::ScreenscraperUser, "me-saved")
            .unwrap();
        store
    }

    #[test]
    fn list_text_says_what_is_set_and_never_a_value() {
        let store = saved();
        let text = list_text(Some(&store), &settings_with_secrets());
        assert!(text.starts_with("keychain: available\n"), "{text}");
        assert!(text.contains("steamgriddb  on  api-key: set\n"), "{text}");
        assert!(text.contains("screenscraper  off"), "{text}");
        assert!(text.contains("user: set, password: not set\n"), "{text}");
        assert!(text.contains("switch-catalog  off\n"), "{text}");
        assert!(text.contains("\nupdates  "), "{text}");
        assert!(
            text.contains("coming later: IGDB, RetroAchievements"),
            "{text}"
        );
        for secret in ["sk-saved", "me-saved", "proxy-pass", "proxy.example"] {
            assert!(!text.contains(secret), "{secret} in {text}");
        }
    }

    #[test]
    fn list_text_without_a_keychain() {
        let s = Settings::default();
        let off = list_text(None, &s);
        assert!(off.starts_with("keychain: off\n"), "{off}");
        assert!(off.contains("api-key: unknown"), "{off}");
        let unavailable = list_text(Some(&MemoryStore::unavailable()), &s);
        assert!(
            unavailable.starts_with("keychain: unavailable\n"),
            "{unavailable}"
        );
    }

    #[test]
    fn list_json_has_the_documented_shape_and_no_value() {
        let store = saved();
        let v = list_json(Some(&store), &settings_with_secrets());
        assert_eq!(v["keychain"], "available");
        let sgdb = &v["integrations"][0];
        assert_eq!(sgdb["id"], "steamgriddb");
        assert_eq!(sgdb["enabled"], true);
        assert_eq!(
            sgdb["fields"],
            json!([{ "key": "steamgriddb-api-key", "set": true }])
        );
        let ss = &v["integrations"][1];
        assert_eq!(ss["enabled"], false);
        assert_eq!(
            ss["fields"][1],
            json!({ "key": "screenscraper-password", "set": false })
        );
        assert!(v["planned"].as_array().unwrap().contains(&json!("igdb")));
        let text = v.to_string();
        for secret in ["sk-saved", "me-saved", "proxy-pass"] {
            assert!(!text.contains(secret), "{secret} in {text}");
        }
        let off = list_json(None, &Settings::default());
        assert_eq!(off["keychain"], "off");
        assert_eq!(off["integrations"][0]["fields"][0]["set"], Value::Null);
    }

    #[test]
    fn exit_codes_follow_the_table() {
        assert_eq!(exit_for(KeyCheck::Ok), 0);
        for c in [KeyCheck::Invalid, KeyCheck::Missing, KeyCheck::Unavailable] {
            assert_eq!(exit_for(c), 1, "{c:?}");
        }
        for c in [KeyCheck::Offline, KeyCheck::RateLimited] {
            assert_eq!(exit_for(c), 5, "{c:?}");
        }
    }

    #[test]
    fn keychain_messages_say_what_to_do_on_each_system() {
        let locked = keychain_message_on(&CredentialError::Locked, "macos");
        assert!(locked.contains("security unlock-keychain"), "{locked}");
        let session = keychain_message_on(&CredentialError::Unavailable, "windows");
        assert!(session.contains("Credential Manager"), "{session}");
        let bus = keychain_message_on(&CredentialError::Unavailable, "linux");
        assert!(bus.contains("secret service"), "{bus}");
        for e in [
            CredentialError::Locked,
            CredentialError::Denied,
            CredentialError::Unavailable,
        ] {
            for os in ["macos", "windows", "linux", "freebsd"] {
                assert!(!keychain_message_on(&e, os).is_empty());
            }
        }
    }
}
