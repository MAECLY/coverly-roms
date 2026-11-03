//! The integrations registry (spec D2–D4): every service Coverly talks to
//! on the user's behalf, in one file, `integrations/integrations.json`.
//! It feeds the «Integraciones» settings tab, the settings search, the
//! notices on other screens and `coverly integrations`.
//!
//! Working entries are the cards of the tab; planned ones only appear in
//! the «Próximamente» list and never take fields, a test or a keychain
//! key (YAGNI). [`problems`] is the coverage test: every integration has a
//! credit, every credential key belongs to exactly one working entry,
//! every endpoint that needs credentials is covered by a working entry and
//! every help link is an official HTTPS page of the service.

use std::collections::HashSet;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::credentials::CredentialKey;
use crate::credits::Credit;
use crate::net::hosts::Endpoint;
use crate::sources::KeyCheck;

/// The registry file, compiled in.
pub const INTEGRATIONS_JSON: &str = include_str!("../../../integrations/integrations.json");

/// What the user hands Coverly for the service.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    ApiKey,
    Account,
    Dataset,
    Link,
}

/// What the integration is about (groups the «Próximamente» list).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IGroup {
    Art,
    Names,
    Achievements,
    Catalogs,
    Updates,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    Working,
    Planned,
}

/// The kind of input a field (or a planned service's need) is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Input {
    User,
    Password,
    ApiKey,
    ClientId,
    Secret,
}

/// Which connection test «Probar conexión» runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TestKind {
    Steamgriddb,
    Screenscraper,
}

/// One credential field of a working integration (a keychain entry).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Field {
    pub key: CredentialKey,
    pub input: Input,
    pub required: bool,
}

/// One registry entry (spec D3).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Integration {
    pub id: String,
    /// Display name; for `switch-catalog` and `updates` only the Spanish
    /// fallback (the UI reads `integrations.service.<id>.name`).
    pub name: String,
    pub kind: Kind,
    pub group: IGroup,
    pub status: Status,
    #[serde(default)]
    pub fields: Vec<Field>,
    #[serde(default)]
    pub test: Option<TestKind>,
    /// The service's official account or API page.
    #[serde(default)]
    pub help_url: Option<String>,
    /// Ids in `credits/credits.json`.
    #[serde(default)]
    pub credits: Vec<String>,
    /// Art source id (`settings::KNOWN_SOURCES`) this integration unlocks.
    #[serde(default)]
    pub source: Option<String>,
    /// Settings path of its place elsewhere (Fuentes order, General).
    #[serde(default)]
    pub setting: Option<String>,
    /// Features it powers (`fill`, `organize-identify`, …).
    #[serde(default)]
    pub powers: Vec<String>,
    /// Anchor of its roadmap section (planned entries).
    #[serde(default)]
    pub roadmap: Option<String>,
    /// What a planned service will ask for (display only).
    #[serde(default)]
    pub needs: Vec<Input>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IntegrationsFile {
    entries: Vec<Integration>,
}

impl Integration {
    /// Whether the user has it turned on: its source in the Fuentes
    /// order, the Switch catalogue switch, or always (a link card).
    pub fn enabled(&self, s: &crate::settings::Settings) -> bool {
        if let Some(source) = &self.source {
            return s.sources.order.iter().any(|e| &e.id == source && e.enabled);
        }
        match self.id.as_str() {
            "switch-catalog" => s.catalogs.switch.enabled,
            _ => true,
        }
    }

    /// Whether this build can use it (ScreenScraper needs the developer
    /// credentials compiled in).
    pub fn built(&self) -> bool {
        self.id != "screenscraper" || crate::credentials::DevCredentials::from_build().is_some()
    }
}

/// Parses a registry file.
pub fn parse(text: &str) -> Result<Vec<Integration>, serde_json::Error> {
    serde_json::from_str::<IntegrationsFile>(text).map(|f| f.entries)
}

/// Every entry of the compiled-in registry, in page order.
pub fn all() -> &'static [Integration] {
    static ENTRIES: OnceLock<Vec<Integration>> = OnceLock::new();
    ENTRIES.get_or_init(|| {
        parse(INTEGRATIONS_JSON).expect("integrations/integrations.json is valid (tested)")
    })
}

/// The entry with this id.
pub fn get(id: &str) -> Option<&'static Integration> {
    all().iter().find(|e| e.id == id)
}

/// The working entries, in page order.
pub fn working() -> impl Iterator<Item = &'static Integration> {
    all().iter().filter(|e| e.status == Status::Working)
}

/// The working integration that unlocks the art source `source`.
pub fn for_source(source: &str) -> Option<&'static Integration> {
    working().find(|e| e.source.as_deref() == Some(source))
}

/// The feature ids an integration may power (spec D3): what the notices
/// and the card's «what it unlocks» line key on.
pub const POWERS: &[&str] = &["fill", "organize-identify", "organize-names", "updates"];

/// What is wrong with the registry against the credits and the network
/// endpoints. Empty when all is well.
pub fn problems(
    entries: &[Integration],
    credits: &[Credit],
    endpoints: &[Endpoint],
) -> Vec<String> {
    let mut out = Vec::new();
    let mut ids = HashSet::new();
    for e in entries {
        let id = e.id.as_str();
        if !ids.insert(id) {
            out.push(format!("{id}: id used twice"));
        }
        if id.is_empty()
            || !id
                .chars()
                .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
        {
            out.push(format!("{id}: id must be kebab-case"));
        }
        for c in &e.credits {
            if !credits.iter().any(|k| &k.id == c) {
                out.push(format!("{id}: unknown credit {c}"));
            }
        }
        for p in &e.powers {
            if !POWERS.contains(&p.as_str()) {
                out.push(format!("{id}: unknown power {p}"));
            }
        }
        match e.status {
            Status::Working => {
                if e.kind != Kind::Link && e.credits.is_empty() {
                    out.push(format!("{id}: needs a credit"));
                }
                if !e.needs.is_empty() {
                    out.push(format!("{id}: working entries take fields, not needs"));
                }
                if e.test.is_some() && e.fields.is_empty() {
                    out.push(format!("{id}: a test needs fields"));
                }
            }
            Status::Planned => {
                if !e.fields.is_empty() || e.test.is_some() {
                    out.push(format!("{id}: planned entries take no fields or test"));
                }
                if e.roadmap.is_none() {
                    out.push(format!("{id}: planned entries need a roadmap anchor"));
                }
            }
        }
        match &e.help_url {
            Some(url) => out.extend(help_url_problem(e, url, credits)),
            // «Cómo conseguir la clave» needs somewhere official to go.
            None if !e.fields.is_empty() => out.push(format!("{id}: needs a help_url")),
            None => {}
        }
    }
    for key in CredentialKey::ALL {
        let owners: Vec<&str> = entries
            .iter()
            .filter(|e| e.status == Status::Working && e.fields.iter().any(|f| f.key == key))
            .map(|e| e.id.as_str())
            .collect();
        let name = key.account();
        match owners.len() {
            0 => out.push(format!("{name}: no working entry")),
            1 => {}
            _ => out.push(format!("{name}: owned twice ({})", owners.join(", "))),
        }
    }
    for ep in endpoints.iter().filter(|ep| ep.credentials) {
        let Some(credit) = ep.credit else {
            out.push(format!(
                "{}: needs credentials but has no credit to find its integration",
                ep.prefix
            ));
            continue;
        };
        // Covered only by a working entry the user can type credentials in.
        let covered = entries
            .iter()
            .filter(|e| e.status == Status::Working && !e.fields.is_empty())
            .any(|e| e.credits.iter().any(|c| c == credit));
        if !covered {
            out.push(format!(
                "{}: needs credentials but credit {credit} is in no working integration",
                ep.prefix
            ));
        }
    }
    out
}

/// Why `url` may not be the help link of `e`, if it may not: it must be a
/// linkable HTTPS URL (the credits rules, with their denylist) on the host
/// of one of the entry's credits. Datasets and links take none.
fn help_url_problem(e: &Integration, url: &str, credits: &[Credit]) -> Option<String> {
    let id = e.id.as_str();
    if matches!(e.kind, Kind::Dataset | Kind::Link) {
        return Some(format!("{id}: a {:?} entry takes no help_url", e.kind));
    }
    if let Some(why) = crate::credits::url_problem(url) {
        return Some(format!("{id}: help_url {url}: {why}"));
    }
    let host = host_of(url);
    let official = e
        .credits
        .iter()
        .filter_map(|c| credits.iter().find(|k| &k.id == c))
        .filter_map(|k| k.url.as_deref())
        .any(|u| host_of(u) == host);
    (!official).then(|| format!("{id}: help_url host {host} is not the host of its credit"))
}

/// The host of an `https://` URL, lowercase and without `www.`.
fn host_of(url: &str) -> String {
    let rest = url.strip_prefix("https://").unwrap_or(url);
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.to_ascii_lowercase();
    host.strip_prefix("www.").map(str::to_owned).unwrap_or(host)
}

/// ScreenScraper's quota as its `ssuserInfos` answer gives it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SsQuota {
    pub requests_today: u64,
    pub max_requests_per_day: u64,
    pub max_threads: u64,
}

/// What a notice on another screen says about an integration (spec D27).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Problem {
    /// A required key or account is not stored.
    Missing,
    /// The service refused the stored credentials.
    Rejected,
    /// The daily quota is used up.
    Quota,
    /// Turned off by the user.
    Off,
}

/// A missing or refused service that changes what a screen shows (D27,
/// D28); the screen links it to the card of `integration`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notice {
    pub integration: String,
    pub problem: Problem,
}

/// An integration card's state (spec D6). Precedence, strongest first:
/// `off` > `unavailable` > `not-configured` > `quota` > `error` >
/// `connected` > `saved`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum State {
    /// Disabled by the user.
    Off,
    /// This build cannot use it (ScreenScraper without developer credentials).
    Unavailable,
    /// A required field is not stored.
    NotConfigured,
    /// Stored, not checked this session.
    Saved,
    /// The last check or job this session succeeded.
    Connected,
    /// The last check or job failed, or the keychain could not be read.
    Error,
    /// The daily quota is used up.
    Quota,
}

/// What the last check or job said, and when (unix seconds).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Health {
    pub verdict: KeyCheck,
    pub at: i64,
}

/// The last quota seen, and when (unix seconds).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeenQuota {
    pub quota: SsQuota,
    pub at: i64,
}

/// Why a card is in `error` or `not-configured`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "detail", rename_all = "kebab-case")]
pub enum Reason {
    /// The last check or job said this.
    Check(KeyCheck),
    /// The keychain could not be read (D16).
    Keychain(KeychainKind),
    /// This required field is not stored.
    Missing(Input),
}

/// Why the keychain could not be read (D16), without its message.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum KeychainKind {
    Unavailable,
    Locked,
    Denied,
    Failed,
}

/// Everything [`state`] looks at; no I/O, so callers gather it first.
#[derive(Clone, Copy, Debug)]
pub struct StateInput<'a> {
    pub entry: &'a Integration,
    pub enabled: bool,
    pub built: bool,
    /// Per field: stored or not; `Err` = the keychain could not be read.
    pub presence: Result<&'a [(CredentialKey, bool)], KeychainKind>,
    pub health: Option<Health>,
    pub quota: Option<SeenQuota>,
    pub now: i64,
}

/// How long a used-up quota counts: ScreenScraper resets it daily and we
/// do not guess its timezone, so a day after it was seen the card falls
/// back to what it was.
pub const QUOTA_TTL_SECS: i64 = 24 * 60 * 60;

/// The card state of one integration and, for `error` and
/// `not-configured`, why (spec D6).
pub fn state(i: &StateInput) -> (State, Option<Reason>) {
    if !i.enabled {
        return (State::Off, None);
    }
    if !i.built {
        return (State::Unavailable, None);
    }
    let presence = match i.presence {
        Ok(p) => p,
        // «we could not look» is not «nothing stored».
        Err(kind) => return (State::Error, Some(Reason::Keychain(kind))),
    };
    let stored = |key: CredentialKey| presence.iter().any(|&(k, set)| k == key && set);
    if let Some(f) = i.entry.fields.iter().find(|f| f.required && !stored(f.key)) {
        return (State::NotConfigured, Some(Reason::Missing(f.input)));
    }
    if i.quota.is_some_and(|q| quota_used_up(q, i.now)) {
        return (State::Quota, None);
    }
    match i.health.map(|h| h.verdict) {
        Some(KeyCheck::Ok) => (State::Connected, None),
        // Presence already says whether a key is missing.
        Some(KeyCheck::Missing) | None => (State::Saved, None),
        Some(v) => (State::Error, Some(Reason::Check(v))),
    }
}

/// The quota was used up when last seen, less than a day ago.
fn quota_used_up(q: SeenQuota, now: i64) -> bool {
    q.quota.max_requests_per_day > 0
        && q.quota.requests_today >= q.quota.max_requests_per_day
        && now - q.at < QUOTA_TTL_SECS
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credits::{self, Credit};
    use crate::net::hosts::{Endpoint, ENDPOINTS};

    fn shipped() -> (Vec<Integration>, Vec<Credit>) {
        (all().to_vec(), credits::all().to_vec())
    }

    #[test]
    fn the_shipped_registry_has_no_problems() {
        let (i, c) = shipped();
        assert_eq!(problems(&i, &c, ENDPOINTS), Vec::<String>::new());
    }

    #[test]
    fn a_working_entry_without_a_known_credit_fails() {
        let (mut i, c) = shipped();
        i[0].credits = vec!["no-such-credit".into()];
        let p = problems(&i, &c, ENDPOINTS);
        assert!(
            p.iter()
                .any(|m| m.starts_with("steamgriddb:") && m.contains("no-such-credit")),
            "{p:?}"
        );
        i[0].credits.clear();
        assert!(problems(&i, &c, ENDPOINTS)
            .iter()
            .any(|m| m.contains("steamgriddb: needs a credit")));
    }

    #[test]
    fn the_updater_link_needs_no_credit() {
        let (i, c) = shipped();
        let u = i.iter().find(|e| e.id == "updates").unwrap();
        assert!(u.credits.is_empty());
        assert!(problems(&i, &c, ENDPOINTS).is_empty());
    }

    #[test]
    fn a_credentialed_endpoint_missing_from_the_registry_fails() {
        let (i, c) = shipped();
        let mut eps = ENDPOINTS.to_vec();
        eps.push(Endpoint {
            prefix: "https://api.example.org/",
            credit: Some("tauri"),
            credentials: true,
        });
        let p = problems(&i, &c, &eps);
        assert!(
            p.iter()
                .any(|m| m.contains("https://api.example.org/") && m.contains("tauri")),
            "{p:?}"
        );
    }

    #[test]
    fn every_credential_key_belongs_to_exactly_one_working_entry() {
        let (mut i, c) = shipped();
        let ss = i.iter().position(|e| e.id == "screenscraper").unwrap();
        i[ss].fields.pop();
        assert!(problems(&i, &c, ENDPOINTS)
            .iter()
            .any(|m| m.contains("screenscraper-password") && m.contains("no working entry")));
        let (mut i, c) = shipped();
        let sg = i[0].fields[0].clone();
        i[1].fields.push(sg);
        assert!(problems(&i, &c, ENDPOINTS)
            .iter()
            .any(|m| m.contains("steamgriddb-api-key") && m.contains("twice")));
    }

    #[test]
    fn planned_entries_carry_no_fields_and_no_test() {
        let (mut i, c) = shipped();
        let igdb = i.iter().position(|e| e.id == "igdb").unwrap();
        i[igdb].test = Some(TestKind::Steamgriddb);
        assert!(problems(&i, &c, ENDPOINTS)
            .iter()
            .any(|m| m.starts_with("igdb:") && m.contains("planned")));
    }

    #[test]
    fn help_urls_must_be_official_https_pages() {
        for (url, why) in [
            (
                "http://www.steamgriddb.com/profile/preferences/api",
                "not https",
            ),
            ("https://www.steamgriddb.com/api?x=1", "query"),
            ("https://steamgriddb.example.com/api", "host"),
        ] {
            let (mut i, c) = shipped();
            i[0].help_url = Some(url.into());
            let p = problems(&i, &c, ENDPOINTS);
            assert!(
                p.iter()
                    .any(|m| m.starts_with("steamgriddb:") && m.contains(why)),
                "{url}: {p:?}"
            );
        }
    }

    #[test]
    fn ids_are_unique_kebab_case() {
        let (mut i, c) = shipped();
        i[1].id = "SteamGridDB".into();
        assert!(problems(&i, &c, ENDPOINTS)
            .iter()
            .any(|m| m.contains("kebab-case")));
        let (mut i, c) = shipped();
        i[1].id = "steamgriddb".into();
        assert!(problems(&i, &c, ENDPOINTS)
            .iter()
            .any(|m| m.contains("used twice")));
    }

    #[test]
    fn a_credentialed_endpoint_covered_only_by_an_entry_without_fields_fails() {
        // titledb belongs to the Switch catalogue, which takes no credentials:
        // if its endpoint ever needed a key there would be nowhere to type it.
        let (i, c) = shipped();
        let eps: Vec<Endpoint> = ENDPOINTS
            .iter()
            .map(|ep| Endpoint {
                credentials: ep.credentials || ep.credit == Some("titledb"),
                ..*ep
            })
            .collect();
        let p = problems(&i, &c, &eps);
        assert!(
            p.iter()
                .any(|m| m.contains("titledb") && m.contains("needs credentials")),
            "{p:?}"
        );
    }

    #[test]
    fn a_credentialed_endpoint_without_a_credit_fails() {
        let (i, c) = shipped();
        let mut eps = ENDPOINTS.to_vec();
        eps.push(Endpoint {
            prefix: "https://api.example.org/",
            credit: None,
            credentials: true,
        });
        let p = problems(&i, &c, &eps);
        assert!(
            p.iter()
                .any(|m| m.starts_with("https://api.example.org/:") && m.contains("no credit")),
            "{p:?}"
        );
    }

    #[test]
    fn an_entry_with_fields_needs_a_help_url() {
        let (mut i, c) = shipped();
        i[0].help_url = None;
        let p = problems(&i, &c, ENDPOINTS);
        assert!(
            p.iter()
                .any(|m| m.starts_with("steamgriddb:") && m.contains("needs a help_url")),
            "{p:?}"
        );
    }

    #[test]
    fn datasets_and_links_take_no_help_url() {
        for id in ["switch-catalog", "updates"] {
            let (mut i, c) = shipped();
            let at = i.iter().position(|e| e.id == id).unwrap();
            i[at].help_url = Some("https://www.steamgriddb.com/".into());
            let p = problems(&i, &c, ENDPOINTS);
            assert!(
                p.iter()
                    .any(|m| m.starts_with(&format!("{id}:")) && m.contains("takes no help_url")),
                "{id}: {p:?}"
            );
        }
    }

    #[test]
    fn planned_entries_need_a_roadmap_and_working_ones_take_no_needs() {
        let (mut i, c) = shipped();
        let rawg = i.iter().position(|e| e.id == "rawg").unwrap();
        i[rawg].roadmap = None;
        i[0].needs = vec![Input::ApiKey];
        let p = problems(&i, &c, ENDPOINTS);
        assert!(
            p.iter()
                .any(|m| m.starts_with("rawg:") && m.contains("roadmap")),
            "{p:?}"
        );
        assert!(
            p.iter()
                .any(|m| m.starts_with("steamgriddb:") && m.contains("not needs")),
            "{p:?}"
        );
    }

    #[test]
    fn a_test_needs_fields() {
        let (mut i, c) = shipped();
        let sw = i.iter().position(|e| e.id == "switch-catalog").unwrap();
        i[sw].test = Some(TestKind::Screenscraper);
        assert!(problems(&i, &c, ENDPOINTS)
            .iter()
            .any(|m| m.starts_with("switch-catalog:") && m.contains("a test needs fields")));
    }

    #[test]
    fn powers_are_known_feature_ids() {
        let (mut i, c) = shipped();
        i[0].powers.push("fil".into());
        let p = problems(&i, &c, ENDPOINTS);
        assert!(
            p.iter()
                .any(|m| m.starts_with("steamgriddb:") && m.contains("unknown power fil")),
            "{p:?}"
        );
    }

    #[test]
    fn the_file_refuses_unknown_fields_and_keychain_keys() {
        assert!(parse(r#"{ "entries": [], "extra": 1 }"#).is_err());
        let entry = |extra: &str| {
            format!(
                r#"{{ "entries": [{{ "id": "x", "name": "X", "kind": "api-key", "group": "art", "status": "working"{extra} }}] }}"#
            )
        };
        assert!(parse(&entry("")).is_ok());
        assert!(parse(&entry(r#", "colour": "red""#)).is_err());
        assert!(parse(&entry(
            r#", "fields": [{ "key": "igdb-secret", "input": "secret", "required": true }]"#
        ))
        .is_err());
    }

    #[test]
    fn get_finds_entries_by_id() {
        assert_eq!(get("screenscraper").map(|e| e.kind), Some(Kind::Account));
        assert_eq!(get("igdb").map(|e| e.status), Some(Status::Planned));
        assert!(get("nope").is_none());
        assert!(working().all(|e| e.status == Status::Working));
    }

    #[test]
    fn sources_map_to_their_integration() {
        assert_eq!(
            for_source("steamgriddb").map(|e| e.id.as_str()),
            Some("steamgriddb")
        );
        assert_eq!(
            for_source("screenscraper").map(|e| e.id.as_str()),
            Some("screenscraper")
        );
        assert!(for_source("libretro").is_none());
        for e in working() {
            if let Some(s) = &e.source {
                assert!(
                    crate::settings::KNOWN_SOURCES.contains(&s.as_str()),
                    "{}: unknown source {s}",
                    e.id
                );
            }
        }
    }

    #[test]
    fn state_follows_the_precedence_of_d6() {
        let ss = get("screenscraper").unwrap();
        let both: &[(CredentialKey, bool)] = &[
            (CredentialKey::ScreenscraperUser, true),
            (CredentialKey::ScreenscraperPassword, true),
        ];
        let half: &[(CredentialKey, bool)] = &[
            (CredentialKey::ScreenscraperUser, true),
            (CredentialKey::ScreenscraperPassword, false),
        ];
        let now = 1_800_000_000;
        let used = SeenQuota {
            quota: SsQuota {
                requests_today: 20_000,
                max_requests_per_day: 20_000,
                max_threads: 1,
            },
            at: now - 60,
        };
        let quota = Some(used);
        let bad = Some(Health {
            verdict: KeyCheck::Invalid,
            at: now - 5,
        });
        let ok = Some(Health {
            verdict: KeyCheck::Ok,
            at: now - 5,
        });
        let base = |p| StateInput {
            entry: ss,
            enabled: true,
            built: true,
            presence: p,
            health: None,
            quota: None,
            now,
        };
        let s = |i: StateInput| state(&i).0;
        assert_eq!(
            s(StateInput {
                enabled: false,
                built: false,
                ..base(Ok(half))
            }),
            State::Off
        );
        assert_eq!(
            s(StateInput {
                built: false,
                ..base(Ok(half))
            }),
            State::Unavailable
        );
        assert_eq!(
            s(StateInput {
                quota,
                ..base(Ok(half))
            }),
            State::NotConfigured
        );
        assert_eq!(
            s(StateInput {
                quota,
                health: bad,
                ..base(Ok(both))
            }),
            State::Quota
        );
        assert_eq!(
            s(StateInput {
                health: bad,
                ..base(Ok(both))
            }),
            State::Error
        );
        assert_eq!(
            s(StateInput {
                health: ok,
                ..base(Ok(both))
            }),
            State::Connected
        );
        assert_eq!(s(base(Ok(both))), State::Saved);
        // «we could not look» is not «nothing stored»
        assert_eq!(
            state(&base(Err(KeychainKind::Locked))),
            (State::Error, Some(Reason::Keychain(KeychainKind::Locked)))
        );
        // the missing field is named
        assert_eq!(
            state(&base(Ok(half))).1,
            Some(Reason::Missing(Input::Password))
        );
        // quota expires 24 h after it was seen
        let old = Some(SeenQuota {
            at: now - QUOTA_TTL_SECS - 1,
            ..used
        });
        assert_eq!(
            s(StateInput {
                quota: old,
                ..base(Ok(both))
            }),
            State::Saved
        );
        // a quota not used up is no state
        let fine = Some(SeenQuota {
            quota: SsQuota {
                requests_today: 3,
                ..used.quota
            },
            ..used
        });
        assert_eq!(
            s(StateInput {
                quota: fine,
                ..base(Ok(both))
            }),
            State::Saved
        );
        // a quota with no known maximum is no state either
        let unknown = Some(SeenQuota {
            quota: SsQuota {
                max_requests_per_day: 0,
                ..used.quota
            },
            ..used
        });
        assert_eq!(
            s(StateInput {
                quota: unknown,
                ..base(Ok(both))
            }),
            State::Saved
        );
        // rate-limited from a check is an error with that reason, not «quota» (D9)
        let rl = Some(Health {
            verdict: KeyCheck::RateLimited,
            at: now,
        });
        assert_eq!(
            state(&StateInput {
                health: rl,
                ..base(Ok(both))
            }),
            (State::Error, Some(Reason::Check(KeyCheck::RateLimited)))
        );
        // every failing verdict carries itself as the reason
        for v in [KeyCheck::Invalid, KeyCheck::Offline, KeyCheck::Unavailable] {
            let h = Some(Health {
                verdict: v,
                at: now,
            });
            assert_eq!(
                state(&StateInput {
                    health: h,
                    ..base(Ok(both))
                }),
                (State::Error, Some(Reason::Check(v)))
            );
        }
        // `missing` from a check is ignored: presence already says it
        let m = Some(Health {
            verdict: KeyCheck::Missing,
            at: now,
        });
        assert_eq!(
            state(&StateInput {
                health: m,
                ..base(Ok(both))
            }),
            (State::Saved, None)
        );
    }

    #[test]
    fn off_and_unavailable_win_over_an_unreadable_keychain_and_quota_over_connected() {
        let ss = get("screenscraper").unwrap();
        let both: &[(CredentialKey, bool)] = &[
            (CredentialKey::ScreenscraperUser, true),
            (CredentialKey::ScreenscraperPassword, true),
        ];
        let now = 1_800_000_000;
        let used = SsQuota {
            requests_today: 20_000,
            max_requests_per_day: 20_000,
            max_threads: 1,
        };
        let base = StateInput {
            entry: ss,
            enabled: true,
            built: true,
            presence: Err(KeychainKind::Denied),
            health: None,
            quota: None,
            now,
        };
        // A disabled or unbuildable integration never reads the keychain.
        assert_eq!(
            state(&StateInput {
                enabled: false,
                ..base
            }),
            (State::Off, None)
        );
        assert_eq!(
            state(&StateInput {
                built: false,
                ..base
            }),
            (State::Unavailable, None)
        );
        // A used-up quota outranks a check that worked.
        let fresh = Some(SeenQuota {
            quota: used,
            at: now - 1,
        });
        let ok = Some(Health {
            verdict: KeyCheck::Ok,
            at: now,
        });
        assert_eq!(
            state(&StateInput {
                presence: Ok(both),
                quota: fresh,
                health: ok,
                ..base
            }),
            (State::Quota, None)
        );
        // The TTL is exclusive: exactly a day later the quota no longer counts.
        let edge = |at| StateInput {
            presence: Ok(both),
            quota: Some(SeenQuota { quota: used, at }),
            ..base
        };
        assert_eq!(state(&edge(now - QUOTA_TTL_SECS + 1)).0, State::Quota);
        assert_eq!(state(&edge(now - QUOTA_TTL_SECS)).0, State::Saved);
    }

    #[test]
    fn a_memory_store_with_todays_keys_reads_as_saved() {
        use crate::credentials::{CredentialStore, MemoryStore};
        let store = MemoryStore::default();
        for k in CredentialKey::ALL {
            store.set(k, "x").unwrap();
        }
        for e in working().filter(|e| !e.fields.is_empty()) {
            let presence: Vec<_> = e
                .fields
                .iter()
                .map(|f| (f.key, store.get(f.key).unwrap().is_some()))
                .collect();
            let i = StateInput {
                entry: e,
                enabled: true,
                built: true,
                presence: Ok(&presence),
                health: None,
                quota: None,
                now: 0,
            };
            assert_eq!(state(&i).0, State::Saved, "{}", e.id);
        }
    }

    #[test]
    fn states_and_reasons_serialise_as_the_ui_expects() {
        let j = |v: serde_json::Value| v.to_string();
        assert_eq!(
            j(serde_json::to_value(State::NotConfigured).unwrap()),
            r#""not-configured""#
        );
        assert_eq!(
            j(serde_json::to_value(Reason::Check(KeyCheck::RateLimited)).unwrap()),
            r#"{"detail":"rate-limited","kind":"check"}"#
        );
        assert_eq!(
            j(serde_json::to_value(Reason::Keychain(KeychainKind::Denied)).unwrap()),
            r#"{"detail":"denied","kind":"keychain"}"#
        );
        assert_eq!(
            j(serde_json::to_value(Reason::Missing(Input::ApiKey)).unwrap()),
            r#"{"detail":"api-key","kind":"missing"}"#
        );
    }
}
