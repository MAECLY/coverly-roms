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
        if let Some(url) = &e.help_url {
            out.extend(help_url_problem(e, url, credits));
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
        let credit = ep.credit.unwrap_or("none");
        let covered = entries
            .iter()
            .filter(|e| e.status == Status::Working)
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
}
