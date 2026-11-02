//! The credits of everything Coverly uses that is not its own (adjustments
//! D25, D26, D30): one file, `credits/credits.json`, read here for
//! `open_credit` and the coverage tests, by the front end for the
//! «Agradecimientos» tab and by the README script.
//!
//! The tests make a new network endpoint, art source, catalogue or DAT
//! origin fail until it has a credit entry, and check the credits the
//! brief names (ScreenScraper, RetroArch Assets, GameTDB,
//! libretro-database, titledb) with their exact licence.

use std::collections::HashSet;
use std::sync::OnceLock;

use serde::Deserialize;

/// The credits file, compiled in.
pub const CREDITS_JSON: &str = include_str!("../../../credits/credits.json");

/// A text in both UI languages.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalText {
    pub es: String,
    pub en: String,
}

/// A name: a proper name (`MIT`, `ScreenScraper`) or, when there is none,
/// a phrase in both languages.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum Label {
    Plain(String),
    Local(LocalText),
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct License {
    #[serde(default)]
    pub spdx: Option<String>,
    pub name: Label,
    #[serde(default)]
    pub url: Option<String>,
}

/// The tab's groups, in their display order (`TODO.md` §7).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Group {
    ArtSources,
    Databases,
    Docs,
    Conventions,
    Libraries,
    FontsIcons,
}

impl Group {
    pub const ALL: [Group; 6] = [
        Group::ArtSources,
        Group::Databases,
        Group::Docs,
        Group::Conventions,
        Group::Libraries,
        Group::FontsIcons,
    ];
}

/// One credit entry (spec D26).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Credit {
    pub id: String,
    pub group: Group,
    pub name: Label,
    /// Official site or repository. Absent for emulators (`TODO.md` §7
    /// forbids linking them) and for sources without a page of their own.
    #[serde(default)]
    pub url: Option<String>,
    #[serde(rename = "use")]
    pub use_: LocalText,
    pub license: License,
    /// The exact credit text, required when `required`.
    #[serde(default)]
    pub credit: Option<LocalText>,
    pub required: bool,
    /// Network prefixes (`net::hosts::ENDPOINTS`) this entry covers.
    #[serde(default)]
    pub endpoints: Vec<String>,
    /// Datasets (catalogues, DAT origins) this entry covers.
    #[serde(default)]
    pub datasets: Vec<String>,
    /// Art source ids (`settings::KNOWN_SOURCES`) this entry covers.
    #[serde(default)]
    pub sources: Vec<String>,
}

/// Which link of an entry to open (`open_credit`, spec D27): on the wire
/// `"site"` or `"license"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CreditLink {
    Site,
    License,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreditsFile {
    #[serde(rename = "$schema", default)]
    _schema: Option<String>,
    entries: Vec<Credit>,
}

/// Parses a credits file.
pub fn parse(text: &str) -> Result<Vec<Credit>, serde_json::Error> {
    serde_json::from_str::<CreditsFile>(text).map(|f| f.entries)
}

/// Every credit entry of the compiled-in file.
pub fn all() -> &'static [Credit] {
    static CREDITS: OnceLock<Vec<Credit>> = OnceLock::new();
    CREDITS.get_or_init(|| parse(CREDITS_JSON).expect("credits/credits.json is valid (tested)"))
}

/// The entry with this id.
pub fn credit(id: &str) -> Option<&'static Credit> {
    all().iter().find(|c| c.id == id)
}

/// The URL `which` of the entry `id`, if the entry exists and has it.
pub fn link(id: &str, which: CreditLink) -> Option<&'static str> {
    let c = credit(id)?;
    match which {
        CreditLink::Site => c.url.as_deref(),
        CreditLink::License => c.license.url.as_deref(),
    }
}

/// The catalogue datasets Coverly downloads (adjustments D2–D9), each of
/// which needs a credit entry.
pub const CATALOG_DATASETS: [&str; 4] = ["titledb", "titledb-ncas", "nlib", "eshop-icons"];

/// Coverly's own endpoints (the updater), the only ones without a credit.
pub const OWN_ENDPOINTS: &[&str] = &[crate::net::hosts::MAECLY_GITHUB];

/// The emulators the research cites: credited by name, use and licence,
/// never linked (`TODO.md` §7).
pub const EMULATORS: &[&str] = &[
    "dolphin",
    "ppsspp",
    "rpcs3",
    "vita3k",
    "cemu",
    "xenia",
    "xemu",
    "azahar",
    "duckstation",
    "shadps4",
    "cxbx-reloaded",
    "decaf-emu",
];

/// Hosts and repositories no entry may link to: ROM, key and emulator
/// sites (`TODO.md` §7), matched by host (without `www.`) and path prefix
/// on a segment boundary.
pub const DENYLIST: &[&str] = &[
    // Emulators cited by the research: name, use and licence, never a link.
    "dolphin-emu.org",
    "github.com/dolphin-emu",
    "ppsspp.org",
    "github.com/hrydgard/ppsspp",
    "rpcs3.net",
    "github.com/rpcs3",
    "vita3k.org",
    "github.com/vita3k",
    "cemu.info",
    "github.com/cemu-project",
    "xenia.jp",
    "github.com/xenia-project",
    "github.com/xenia-canary",
    "xemu.app",
    "github.com/xemu-project",
    "azahar-emu.org",
    "github.com/azahar-emu",
    "duckstation.org",
    "github.com/stenzek",
    "shadps4.net",
    "github.com/shadps4-emu",
    "cxbx-reloaded.co.uk",
    "github.com/cxbx-reloaded",
    "github.com/decaf-emu",
    "citra-emu.org",
    "yuzu-emu.org",
    "ryujinx.org",
    "github.com/ryujinx",
    "pcsx2.net",
    "github.com/pcsx2",
    "mgba.io",
    "melonds.kuribo64.net",
    "mamedev.org",
    "neo-source.com",
    // ROM, firmware and key sites.
    "archive.org",
    "myrient.erista.me",
    "hshop.erista.me",
    "vimm.net",
    "nopaystation.com",
    "emuparadise.me",
    "coolrom.com",
    "romsfun.com",
    "cdromance.org",
    "wowroms.com",
    "romspure.cc",
    "nsw2u.com",
    "nswdb.com",
];

/// What is wrong with the credits as a whole: the D26 shape rules and the
/// D30 coverage (a) endpoints, (b) art sources and catalogues, (c) DAT
/// origins, (d) the required credits. Empty when all is well.
pub fn problems(entries: &[Credit]) -> Vec<String> {
    let mut out = schema_problems(entries);
    out.extend(coverage_problems(entries));
    out
}

fn schema_problems(entries: &[Credit]) -> Vec<String> {
    let mut out = Vec::new();
    let mut ids = HashSet::new();
    let mut covered: HashSet<(&str, &str)> = HashSet::new();
    for c in entries {
        let id = c.id.as_str();
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
        let mut texts = vec![("use", &c.use_)];
        if let Label::Local(t) = &c.name {
            texts.push(("name", t));
        }
        if let Some(credit) = &c.credit {
            texts.push(("credit", credit));
        }
        if let Label::Local(t) = &c.license.name {
            texts.push(("license.name", t));
        }
        for (field, t) in texts {
            if t.es.trim().is_empty() || t.en.trim().is_empty() {
                out.push(format!("{id}: {field} needs es and en"));
            }
        }
        for (field, label) in [("name", &c.name), ("license.name", &c.license.name)] {
            if matches!(label, Label::Plain(n) if n.trim().is_empty()) {
                out.push(format!("{id}: empty {field}"));
            }
        }
        if c.required && c.credit.is_none() {
            out.push(format!("{id}: required without a credit text"));
        }
        for (field, url) in [("url", &c.url), ("license.url", &c.license.url)] {
            if let Some(url) = url {
                if let Some(why) = url_problem(url) {
                    out.push(format!("{id}: {field} {url}: {why}"));
                }
            }
        }
        for e in &c.endpoints {
            if !e.starts_with("https://") || !e.ends_with('/') {
                out.push(format!(
                    "{id}: endpoint {e} must be an https:// prefix ending in /"
                ));
            }
        }
        for (kind, list) in [
            ("endpoint", &c.endpoints),
            ("dataset", &c.datasets),
            ("source", &c.sources),
        ] {
            for item in list {
                if !covered.insert((kind, item.as_str())) {
                    out.push(format!("{id}: {kind} {item} is credited twice"));
                }
            }
        }
    }
    for e in EMULATORS {
        match entries.iter().find(|c| c.id == *e) {
            None => out.push(format!("{e}: emulator credit missing")),
            Some(c) if c.url.is_some() => out.push(format!("{e}: emulators carry no link")),
            Some(_) => {}
        }
    }
    for g in Group::ALL {
        if !entries.iter().any(|c| c.group == g) {
            out.push(format!("group {g:?} has no entry"));
        }
    }
    out
}

/// Why `url` may not be linked, if it may not.
pub(crate) fn url_problem(url: &str) -> Option<&'static str> {
    let Some(rest) = url.strip_prefix("https://") else {
        return Some("not https");
    };
    if url.contains('?') || url.contains('#') {
        return Some("has a query or fragment");
    }
    if url.chars().any(|c| c.is_whitespace() || c.is_control()) || rest.contains('@') {
        return Some("not a plain URL");
    }
    let lower = rest.to_ascii_lowercase();
    let lower = lower.strip_prefix("www.").unwrap_or(&lower);
    let host = lower.split('/').next().unwrap_or("");
    if host.is_empty() {
        return Some("no host");
    }
    let denied = DENYLIST.iter().any(|d| {
        let (d_host, d_path) = d.split_once('/').unwrap_or((d, ""));
        let host_hit = host == d_host || host.ends_with(&format!(".{d_host}"));
        if !host_hit {
            return false;
        }
        if d_path.is_empty() {
            return true;
        }
        let path = &lower[host.len()..];
        let want = format!("/{d_path}");
        path.strip_prefix(&want)
            .is_some_and(|tail| tail.is_empty() || tail.starts_with('/'))
    });
    denied.then_some("on the denylist (ROM, key or emulator site)")
}

fn coverage_problems(entries: &[Credit]) -> Vec<String> {
    let mut out = Vec::new();
    let by_id = |id: &str| entries.iter().find(|c| c.id == id);

    // (a) Endpoints.
    out.extend(endpoint_problems(entries, crate::net::hosts::ENDPOINTS));

    // (b) Art sources and catalogues.
    for s in crate::settings::KNOWN_SOURCES {
        if !entries.iter().any(|c| c.sources.iter().any(|x| x == s)) {
            out.push(format!("source {s}: no credit entry lists it"));
        }
    }
    let mut datasets: Vec<String> = CATALOG_DATASETS.iter().map(|d| d.to_string()).collect();
    // (c) Every DAT origin of the built-in system database.
    datasets.extend(dat_origins());
    for d in &datasets {
        if !entries.iter().any(|c| c.datasets.iter().any(|x| x == d)) {
            out.push(format!("dataset {d}: no credit entry lists it"));
        }
    }

    // (d) The credits the brief names, with their exact licence.
    for (id, spdx, needs_url) in [
        ("screenscraper", Some("CC-BY-NC-SA-4.0"), false),
        ("retroarch-assets", Some("CC-BY-4.0"), false),
        ("gametdb", None, true),
        ("libretro-database", Some("CC-BY-SA-4.0"), false),
        ("titledb", Some("MIT"), false),
    ] {
        let Some(c) = by_id(id) else {
            out.push(format!("{id}: required credit missing"));
            continue;
        };
        if spdx.is_some() && c.license.spdx.as_deref() != spdx {
            out.push(format!(
                "{id}: licence must be {}",
                spdx.unwrap_or_default()
            ));
        }
        if !c.required || c.credit.is_none() {
            out.push(format!("{id}: must be required with a credit text"));
        }
        if needs_url && c.url.is_none() {
            out.push(format!("{id}: needs its link"));
        }
    }
    out
}

/// D30 (a): every endpoint names an entry that lists its prefix, and every
/// listed prefix is registered for that entry. Only Coverly's own
/// endpoints ([`OWN_ENDPOINTS`]) may go without a credit, so a new
/// provider cannot slip in with `credit: None`.
fn endpoint_problems(entries: &[Credit], endpoints: &[crate::net::hosts::Endpoint]) -> Vec<String> {
    let mut out = Vec::new();
    for e in endpoints {
        let Some(credit) = e.credit else {
            if !OWN_ENDPOINTS.contains(&e.prefix) {
                out.push(format!(
                    "endpoint {}: no credit (only Coverly's own endpoints may go without)",
                    e.prefix
                ));
            }
            continue;
        };
        match entries.iter().find(|c| c.id == credit) {
            None => out.push(format!("endpoint {}: no credit entry {credit}", e.prefix)),
            Some(c) if !c.endpoints.iter().any(|p| p == e.prefix) => out.push(format!(
                "endpoint {}: credit {credit} does not list it in endpoints",
                e.prefix
            )),
            Some(_) => {}
        }
    }
    for c in entries {
        for p in &c.endpoints {
            if !endpoints
                .iter()
                .any(|e| e.prefix == p && e.credit == Some(c.id.as_str()))
            {
                out.push(format!(
                    "{}: endpoint {p} is not registered for it in net/hosts.rs",
                    c.id
                ));
            }
        }
    }
    out
}

/// The distinct DAT origins of the built-in system database, read through
/// its API so derived defaults count: `metadat/<origin>/…` → `<origin>`,
/// `dat/…` → `dat`.
pub fn dat_origins() -> Vec<String> {
    let db = crate::systems::SystemDb::builtin();
    let mut out: Vec<String> = Vec::new();
    for system in db.all() {
        for path in crate::identify::dat::dat_sources(system) {
            let mut parts = path.split('/');
            let origin = match (parts.next(), parts.next()) {
                (Some("metadat"), Some(o)) => o.to_string(),
                (Some(first), _) => first.to_string(),
                _ => continue,
            };
            if !out.contains(&origin) {
                out.push(origin);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Component, Path, PathBuf};

    #[test]
    fn the_credits_file_parses_and_has_no_problems() {
        let entries = parse(CREDITS_JSON).expect("valid credits.json");
        let problems = problems(&entries);
        assert!(problems.is_empty(), "{problems:#?}");
    }

    #[test]
    fn a_copy_without_titledb_fails_naming_it() {
        let mut entries = parse(CREDITS_JSON).unwrap();
        entries.retain(|c| c.id != "titledb");
        let problems = problems(&entries);
        for want in [
            "dataset titledb:",
            "dataset titledb-ncas:",
            "titledb: required credit missing",
        ] {
            assert!(
                problems.iter().any(|p| p.starts_with(want)),
                "{want} not in {problems:#?}"
            );
        }
        assert!(problems
            .iter()
            .any(|p| p.contains(crate::net::hosts::TITLEDB)));
    }

    #[test]
    fn an_unlisted_source_endpoint_or_dat_origin_fails() {
        let mut entries = parse(CREDITS_JSON).unwrap();
        for c in &mut entries {
            c.sources.retain(|s| s != "embedded");
            c.datasets.retain(|d| d != "redump");
            c.endpoints.retain(|e| e != crate::net::hosts::NLIB);
        }
        let problems = problems(&entries);
        for want in [
            "source embedded:",
            "dataset redump:",
            "endpoint https://api.nlib.cc/:",
        ] {
            assert!(
                problems.iter().any(|p| p.starts_with(want)),
                "{want} not in {problems:#?}"
            );
        }
    }

    #[test]
    fn required_credits_carry_their_exact_licence() {
        let mut entries = parse(CREDITS_JSON).unwrap();
        for c in &mut entries {
            if c.id == "screenscraper" {
                c.license.spdx = Some("CC-BY-4.0".into());
            }
            if c.id == "gametdb" {
                c.url = None;
            }
        }
        let problems = problems(&entries);
        assert!(problems
            .iter()
            .any(|p| p == "screenscraper: licence must be CC-BY-NC-SA-4.0"));
        assert!(problems.iter().any(|p| p == "gametdb: needs its link"));
    }

    #[test]
    fn dat_origins_include_derived_defaults() {
        let origins = dat_origins();
        for o in ["no-intro", "redump", "mame", "fbneo-split", "tosec", "dat"] {
            assert!(origins.iter().any(|x| x == o), "{o} in {origins:?}");
        }
    }

    #[test]
    fn urls_must_be_https_plain_and_off_the_denylist() {
        assert_eq!(url_problem("https://www.screenscraper.fr/"), None);
        assert_eq!(
            url_problem("https://github.com/libretro/retroarch-assets"),
            None
        );
        assert_eq!(url_problem("https://github.com/dolphinfan/x"), None);
        for bad in [
            "http://redump.org/",
            "https://example.org/?ref=x",
            "https://example.org/#x",
            "https://user@example.org/",
            "https://dolphin-emu.org/",
            "https://www.dolphin-emu.org/download",
            "https://github.com/dolphin-emu/dolphin",
            "https://github.com/Stenzek/duckstation",
            "https://forums.archive.org/x",
            "https://vimm.net/vault",
        ] {
            assert!(url_problem(bad).is_some(), "{bad}");
        }
    }

    #[test]
    fn an_emulator_with_a_link_or_missing_fails() {
        let mut entries = parse(CREDITS_JSON).unwrap();
        for c in &mut entries {
            if c.id == "dolphin" {
                c.url = Some("https://example.org/".into());
            }
        }
        entries.retain(|c| c.id != "xemu");
        let problems = problems(&entries);
        assert!(problems
            .iter()
            .any(|p| p == "dolphin: emulators carry no link"));
        assert!(problems
            .iter()
            .any(|p| p == "xemu: emulator credit missing"));
    }

    #[test]
    fn an_endpoint_without_a_credit_must_be_coverlys_own() {
        use crate::net::hosts::{Endpoint, MAECLY_GITHUB};
        let entries = parse(CREDITS_JSON).unwrap();
        let endpoints = [
            Endpoint {
                prefix: MAECLY_GITHUB,
                credit: None,
                credentials: false,
            },
            Endpoint {
                prefix: "https://new-provider.example/",
                credit: None,
                credentials: false,
            },
        ];
        let problems = endpoint_problems(&entries, &endpoints);
        assert!(
            problems
                .iter()
                .any(|p| p.starts_with("endpoint https://new-provider.example/: no credit")),
            "{problems:#?}"
        );
        assert!(!problems.iter().any(|p| p.contains(MAECLY_GITHUB)));
    }

    #[test]
    fn lookups_and_links() {
        assert_eq!(credit("titledb").map(|c| c.group), Some(Group::Databases));
        assert!(credit("nope").is_none());
        assert_eq!(
            link("gametdb", CreditLink::Site),
            Some("https://www.gametdb.com/")
        );
        assert_eq!(
            link("screenscraper", CreditLink::License),
            Some("https://creativecommons.org/licenses/by-nc-sa/4.0/")
        );
        assert_eq!(link("nope", CreditLink::Site), None);
    }

    /// D30 (a): no source file outside `net/hosts.rs` and `prefs.rs`
    /// (`LinkId`) holds a URL, so every endpoint goes through the registry.
    #[test]
    fn no_url_literal_outside_the_registry() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut dirs: Vec<PathBuf> = vec![root.join("src-tauri").join("src")];
        for crate_dir in std::fs::read_dir(root.join("crates")).unwrap() {
            dirs.push(crate_dir.unwrap().path().join("src"));
        }
        let mut offenders = Vec::new();
        for dir in dirs {
            for entry in walkdir::WalkDir::new(&dir).sort_by_file_name() {
                let entry = entry.unwrap();
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("rs") || exempt(path) {
                    continue;
                }
                let text = std::fs::read_to_string(path).unwrap();
                for (n, line) in non_test_lines(&text) {
                    if has_url_literal(line) {
                        offenders.push(format!("{}:{}", path.display(), n + 1));
                    }
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "URL literals outside net/hosts.rs and prefs.rs (move them to the registry): {offenders:#?}"
        );
    }

    /// `net/hosts.rs`, `prefs.rs` and test-only files, compared by path
    /// component so the check holds on Windows.
    fn exempt(path: &Path) -> bool {
        let names: Vec<&str> = path
            .components()
            .filter_map(|c| match c {
                Component::Normal(s) => s.to_str(),
                _ => None,
            })
            .collect();
        let file = names.last().copied().unwrap_or("");
        let parent = names.len().checked_sub(2).map(|i| names[i]).unwrap_or("");
        (parent == "net" && file == "hosts.rs")
            || file == "prefs.rs"
            || file == "tests.rs"
            || names.contains(&"tests")
    }

    /// The lines before the file's `#[cfg(test)]` module, with their index.
    fn non_test_lines(text: &str) -> impl Iterator<Item = (usize, &str)> {
        let lines: Vec<&str> = text.lines().collect();
        let end = lines
            .iter()
            .enumerate()
            .position(|(i, l)| {
                l.trim() == "#[cfg(test)]"
                    && lines
                        .get(i + 1)
                        .is_some_and(|next| next.trim_start().starts_with("mod "))
            })
            .unwrap_or(lines.len());
        lines.into_iter().take(end).enumerate()
    }

    /// An `https://host…` in code (not a comment, not an XML namespace).
    fn has_url_literal(line: &str) -> bool {
        let code = line.trim_start();
        if code.starts_with("//") || line.contains("xmlns") {
            return false;
        }
        let mut from = 0;
        while let Some(i) = line[from..].find("https://") {
            let at = from + i;
            let before = &line[..at];
            if before.contains(" //") || before.starts_with("//") {
                return false;
            }
            let next = line[at + "https://".len()..].chars().next();
            if next.is_some_and(|c| c.is_ascii_alphanumeric()) {
                return true;
            }
            from = at + 1;
        }
        false
    }

    #[test]
    fn the_literal_scan_sees_code_but_not_comments() {
        assert!(has_url_literal(r#"const X: &str = "https://a.example/";"#));
        assert!(!has_url_literal(r#"    // see https://a.example/"#));
        assert!(!has_url_literal(r#"let x = 1; // https://a.example/"#));
        assert!(!has_url_literal(r#"if url.starts_with("https://") {"#));
        assert!(!has_url_literal(
            r#"w.attr("xmlns", "https://a.example/ns");"#
        ));
        let text = "a\n#[cfg(test)]\nmod tests {\n\"https://x.example\"\n}\n";
        assert_eq!(non_test_lines(text).count(), 1);
        assert!(exempt(Path::new("crates/coverly-core/src/net/hosts.rs")));
        assert!(!exempt(Path::new("crates/coverly-core/src/hosts.rs")));
    }
}
