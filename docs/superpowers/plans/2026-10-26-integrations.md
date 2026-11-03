# Settings Tab: Integraciones (accounts, keys and services) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking. UI tasks (9–12) also REQUIRE frontend-design:frontend-design: follow `docs/08-brand/brand.md` and spec §6, use the `src/components/ui` primitives and the tokens in `design/tokens.json` (no new tokens), and critique each changed screen with screenshots before committing.

**Goal:** A new Settings tab, **Integraciones** / "Integrations", holds every service Coverly talks to on the user's behalf: one card per working integration (SteamGridDB, ScreenScraper, Catálogo de Switch, a link card to Actualizaciones) with what it is for, a visible status, masked fields that never refill, «Probar conexión», «Cómo conseguir la clave», ScreenScraper quota, enable/disable, «Olvidar credenciales» with an in-page confirm, a privacy note and links to Agradecimientos and Fuentes. Planned services appear only as a compact «Próximamente» list. One registry (`integrations/integrations.json`) feeds the tab, the settings search, the notices on other screens and a new CLI `coverly integrations list|test|set|forget`. Keychain errors say what to do per OS. Stored credentials keep their keychain names, so nothing has to be entered again.

**Architecture:** `integrations/integrations.json` is compiled into Rust (`coverly_core::integrations`, parse + `problems()` + pure `state()`) and imported by TS (`src/lib/integrations.ts`). `KeychainStore` moves from `src-tauri` into `coverly_core::credentials::keychain` behind a `keychain` feature so the CLI shares it; `classify` decides by platform error code. `check_account` returns ScreenScraper quota; `gather` records structured `SourceTrouble`s beside its unchanged warning strings; `AppState` keeps an in-memory integration health map and last quota. New IPC: `integrations_view`, `test_integration`, `forget_integration`, `open_integration_help`; `AppInfo.platform`; `LinkId::Roadmap`. The UI adds `IntegrationsSection`, `IntegrationCard`, `SecretField`, `SwitchCatalogGroup` (moved out of Fuentes), `IntegrationLink`, `IntegrationNotice`; Fuentes keeps only the order and «Configurar en Integraciones».

**Tech Stack:** Rust 2021 (coverly-core, coverly-cli with clap + assert_cmd, src-tauri on Tauri 2), `keyring` 4.2 (`apple-native-keyring-store` 1.0.2 / `security-framework` 3.7, `windows-native-keyring-store` 1.1.0, `zbus-secret-service-keyring-store` 1.0.1 / `secret-service` 5.2 — versions from `Cargo.lock`), React 19, TypeScript, Tailwind 3.4, zustand, Radix via `src/components/ui`, lucide-react, Vitest + Testing Library + user-event. E2E: Chrome DevTools MCP on the Vite dev server with the mock backend.

**Spec:** [`docs/superpowers/specs/2026-10-26-integrations.md`](../specs/2026-10-26-integrations.md) — decisions D1–D29, copy §7, test plan §10 and owner decisions §12 are binding unless a deviation below says otherwise. Roadmap brief: `TODO.md` §8 (owner-approved, Miguel, 2026-11-02). Brand: [`docs/08-brand/brand.md`](../../08-brand/brand.md).

## Global Constraints

- **Worktree.** Work only in `/Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-integrations`, branch `feat/integrations-tab`, which tracks `origin/feat/integrations-tab`. Every shell command starts with `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-integrations &&`. Never switch branches and never touch `main`.
- **Merge main first.** Every task starts with `git fetch origin && git merge origin/main` (no rebase, never force-push). Other workflows change `locales/*.json`, `src/components/*` and `TODO.md` in parallel: resolve conflicts keeping both sides (locale files: keep every key from both sides, then re-sort nothing — keep the file's existing order), rerun the gates, and push the merge commit at once.
- **Push after every commit (mandatory).** An earlier run lost a whole branch because it was never pushed. Every task ends with `git commit …`, then `git push`, then `git status -sb`; the first line must read `## feat/integrations-tab...origin/feat/integrations-tab` with **no** `[ahead N]`. Never end an agent with unpushed commits.
- **Commit messages.** Conventional (`feat(core): …`, `feat(app): …`, `feat(ui): …`, `feat(cli): …`, `refactor(…): …`, `test(…): …`, `fix(…): …`, `docs: …`), granular, in English. **No** `Co-Authored-By` trailer, no "Generated with", and no mention of Claude or AI anywhere (owner rule; it overrides any harness attribution).
- **Do not edit** `docs/TRACKER.md` or `TODO.md` on this branch (they are updated on `main` after merge).
- **Secrets never leave Rust.** No IPC answer, log line, CLI output, error message, `Debug` print or test snapshot may contain a credential value. Values only travel webview → `set_credential` and keychain → request. New log lines say `set` / `unset` / a `KeyCheck`. Never `{:?}` a `keyring::Error`.
- **No new keychain keys.** `CredentialKey` stays `steamgriddb-api-key`, `screenscraper-user`, `screenscraper-password`; `SERVICE` stays `com.maecly.coverlyroms` (migration, D5/D21). Planned services get registry entries with `needs` (display text) and no `fields`.
- **Official pages only.** `help_url` is the service's own account/API page; never ROM, key or emulator sites (the D4 test enforces host = credit host and the credits denylist). No official logos (D25): service name in text plus a lucide icon.
- **No network on tab open.** Opening Settings sends nothing to anyone; «Probar conexión» is explicit (D9).
- **TDD.** Write the failing test, run it and watch it fail for the expected reason, implement, watch it pass. Each "Run" step names the exact command.
- **Gates** (same as CI).
  - **G-WEB**: `pnpm check-all && pnpm build` (`check-all` = `tokens:check`, `notices:check`, `credits:check`, `lint`, `format:check`, `typecheck`, `test`).
  - **G-RUST**: `cargo fmt --all --check && cargo clippy --workspace --all-targets --all-features -- -D warnings && scripts/dev/clippy-windows.sh && cargo test --workspace --no-fail-fast`.
  - Rust tasks run G-RUST before the commit; UI tasks run G-WEB; tasks that touch both run both. The last task runs everything.
- **Copy.** Every UI string goes through `locales/en/ui.json` **and** `locales/es/ui.json` with the same keys and placeholders (`src/i18n/parity.test.ts`). Spanish: sentence case, no exclamation marks, exact counts, numbers in mono; tab names keep their capital inside a sentence («Configurar en Integraciones»). English uses British spelling (`Organise`, `catalogue`, `licence`). Numbers through `Intl.NumberFormat(locale)`, relative time through the existing `formatRelative`. Before removing a key, `git grep -n '<key>' -- src` must show no other user.
- **Brand.** No new tokens (`pnpm tokens:check`). `sectionTitle` (Atkinson Hyperlegible Next) for group titles; Atkinson Hyperlegible Mono only for real data (quota numbers, the fallback URL, typed keys). Status tags: existing `Badge` variants (§6.2), word + icon always, never colour alone. Single column of rows, no card shadows, no gradients, no grid of tiles.
- **Real data is read-only.** Never write to `~/Documents/ROMS`, `/Volumes/TinyExt/*` or `/Volumes/System`. Any manual write test uses a copy made with `cp -RXp` under `/Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-integrations/target/`, deleted afterwards, with no `._*` files.
- **Windows CI.** Paths in tests are compared by component (`Path::components`), never by slash text; any filesystem fixture pins its mtime (`filetime::set_file_mtime`). CLI `set` trims a trailing `\r\n`. Windows-only code is checked by `scripts/dev/clippy-windows.sh`.
- **Keychain in tests.** No test touches the real OS keychain: unit tests use `MemoryStore`; every CLI `assert_cmd` test (new and existing) sets `COVERLY_NO_KEYCHAIN=1` through one helper (Task 13). The real-keychain round trip stays `#[ignore]` with its own service `com.maecly.coverlyroms.test`.
- **Servers and E2E.** Start the web UI with `pnpm dev --port 1469 --strictPort` (mock backend; the configured port 1431 is not used). Stop it **by port only**: `lsof -ti tcp:1469 -sTCP:LISTEN | xargs -r kill`. E2E uses Chrome DevTools MCP `new_page` with `isolatedContext: "integrations"`; fall back to Playwright MCP; if both profiles are locked, install `playwright-core` in `/private/tmp/claude-501/-Users-miguelangelesparzacalero-Documents-GitHub/35b7c020-f2ae-412f-b757-99c24a9a498b/scratchpad/integrations` and drive Chrome for Testing.
- **Secrets on disk** (minisign key, `~/.coverly-release`, `.env*`, `.vercel`) are never read or printed.
- **Owner decisions** (spec §12.1–§12.8) are implemented with their default and listed, with the alternatives, in the PR description and the final result. Nothing blocks on them.

## Deviations from the spec (decided in this plan)

- **IP-1: catalogue card state is mapped in TS.** `integrations_view` returns the Switch catalogue entry with `enabled` only (`state: "off"` or `"saved"`); the card's real state comes from the existing `CatalogStatus` through a pure `catalogCardState(status)` in `src/lib/integrations.ts` (D23 mapping). `SwitchCatalogStatusLine`, the polling and the module-level `inflight` tracker already live in the UI; duplicating the mapping in Rust would give two answers.
- **IP-2: organise notices are computed at plan time, not from troubles.** Organise does not run `gather`, so it has no `SourceTrouble`s. `src-tauri` builds the organise plan's `integration_notices` from the same inputs `online_wanted` uses (ScreenScraper enabled; account stored; quota state from health) plus "pile has Switch items and `catalogs.switch.enabled` is off" (D28). Fill notices come from `SourceTrouble` as specified.
- **IP-3: one CLI test helper.** Instead of editing each `Command::cargo_bin("coverly")` call by hand, `crates/coverly-cli/tests/common/mod.rs` gains `pub fn coverly() -> Command` that sets `COVERLY_SETTINGS=""` and `COVERLY_NO_KEYCHAIN=1`; every test file switches to it (mechanical `sed`, reviewed in the diff). A guard test greps the tests folder and fails if `cargo_bin("coverly")` appears outside the helper.

## Review Focus

1. **Secrets never surface.** No value in any IPC payload, DOM node, log line, CLI stdout/stderr, error `Display`/`Debug`, or `Settings` serialisation (D8, D17, D20). Inputs empty after save/error/forget; «Mostrar» shows only typed text.
2. **Migration and compatibility.** Same `SERVICE` and account names (pinned by test); old IPC commands (`credential_status`, `set_credential`, `clear_credential`, `sources_info`, `test_source_key`) keep their names and payloads; `settings.json` round-trips unchanged; no setting added or renamed.
3. **Keychain classification by code, per OS** (D16): the table in the spec is the test; no message-text matching; `BadEncoding`/`BadDataFormat` never formatted.
4. **CLI safety** (D18–D20): secrets only from stdin/env; a positional secret is a usage error; keychain values only towards default endpoints; 5 s cap; exit codes 0/1/2/5 as in D20; every CLI test has `COVERLY_NO_KEYCHAIN=1`.
5. **Notices are true** (D28): shown only where the missing service changes the result on screen; nothing in Duplicados; `catalogue-missing` (DATs) is not linked to the Switch card.
6. **Search and deep links** (D14, D15): closed card bodies stay mounted (`hidden`), search lands on cards and moved Switch settings, the registry completeness test covers the renamed ids.

---

## File structure

```
integrations/integrations.json                  (new) the registry (D2, D3)
integrations/README.md                          (new) how to add a service (one entry + credit + copy)
crates/coverly-core/src/integrations.rs         (new) Integration, parse, problems(), state(), SsQuota, Health
crates/coverly-core/src/lib.rs                  pub mod integrations
crates/coverly-core/src/credentials.rs          + CredentialError::Denied; pinned names test
crates/coverly-core/src/credentials/keychain.rs (new, moved) KeychainStore + classify by code (feature `keychain`)
crates/coverly-core/src/credits.rs              url_problem → pub(crate)
crates/coverly-core/src/net/hosts.rs            Endpoint.credentials: bool
crates/coverly-core/src/sources/screenscraper.rs check_account → AccountCheck { verdict, quota }; last_quota()
crates/coverly-core/src/sources/mod.rs          SourceTrouble, TroubleKind
crates/coverly-core/src/gather/mod.rs           Gathered.troubles, Gathered.answered
crates/coverly-core/src/plan/mod.rs             PlanSummary.troubles (serde default, skip if empty)
crates/coverly-core/Cargo.toml                  feature keychain; target deps for classify
crates/coverly-cli/Cargo.toml                   coverly-core features ["keychain"]
crates/coverly-cli/src/integrations.rs          (new) list/test/set/forget
crates/coverly-cli/src/keys.rs                  (new) cli_store(), TimedStore, resolve_keys()
crates/coverly-cli/src/{main,fill,organize,catalog}.rs  subcommand, keychain keys, hints, exit-code doc
crates/coverly-cli/tests/common/mod.rs          (new) coverly() helper (IP-3)
crates/coverly-cli/tests/cli_integrations.rs    (new)
crates/coverly-cli/tests/*.rs                   switch to coverly()
src-tauri/Cargo.toml                            coverly-core features ["keychain"]; drop direct keyring
src-tauri/src/keychain.rs                       removed (re-export from core in lib.rs)
src-tauri/src/integrations.rs                   (new) integrations_view, test_integration, forget, help, health
src-tauri/src/{ipc,lib,state,error,prefs,sources}.rs  commands, AppInfo.platform, keychain-denied, LinkId::Roadmap, needs ← registry
src/lib/integrations.ts                         (new) typed loader + catalogCardState + searchable text
src/lib/integrations.test.ts                    (new)
tsconfig.json, package.json                     include/format "integrations"
src/ipc/{types,backend,tauriBackend,errors,appInfo}.ts  IntegrationView, SsQuota, IntegrationNotice, calls, keychain-denied, platform
src/ipc/mock/mockSettings.ts (+ .test.ts)       integrations calls; ?mockIntegrations= ?mockKeychain= ?mockPlatform=
src/ipc/contract.test.ts                        new calls
src/components/settings/
  SecretField.tsx (+ .test.tsx)                 (new) extracted from CredentialRow
  IntegrationStatusTag.tsx                      (new)
  IntegrationLink.tsx, IntegrationNotice.tsx (+ tests)  (new)
  IntegrationCard.tsx (+ .test.tsx)             (new)
  IntegrationsSection.tsx (+ .test.tsx)         (new)
  SwitchCatalogGroup.tsx                        (new, moved from SourcesSection)
  SourcesSection.tsx (+ .test.tsx)              order only + Configurar
  registry.ts (+ registry.test.tsx, search.test.ts)  integration entries; renamed Switch ids; fold id
  keychainMessage.ts (+ .test.ts)               (new) OS-specific key with fallback
src/stores/navStore.ts                          'integrations' in SETTINGS_SECTIONS / SETTINGS_GROUPS
src/screens/SettingsScreen.tsx                  SECTION_COMPONENTS entry
src/components/fill/FillWizard.tsx              popularityNoKey + IntegrationLink; summary notices
src/screens/OrganizeScreen.tsx                  integration notices
locales/{es,en}/ui.json                         spec §7 keys; moved/removed keys (D7); fill.popularityNoKey reworded
docs/04-architecture/{settings,desktop-app,art-sources,fill-pipeline}.md, README.md, README.es.md, CHANGELOG.md
```

---

## Task 1: Integrations registry file and core module

**Files:**
- Create: `integrations/integrations.json`, `integrations/README.md`, `crates/coverly-core/src/integrations.rs`
- Modify: `crates/coverly-core/src/lib.rs`, `crates/coverly-core/src/credits.rs` (`url_problem` → `pub(crate)`), `crates/coverly-core/src/net/hosts.rs` (`Endpoint.credentials`), `crates/coverly-core/src/credentials.rs` (pin test)

**Interfaces (produced):**
```rust
pub const INTEGRATIONS_JSON: &str = include_str!("../../../integrations/integrations.json");

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind { ApiKey, Account, Dataset, Link }
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IGroup { Art, Names, Achievements, Catalogs, Updates }
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status { Working, Planned }
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Input { User, Password, ApiKey, ClientId, Secret }
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TestKind { Steamgriddb, Screenscraper }

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Field { pub key: CredentialKey, pub input: Input, pub required: bool }

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Integration {
    pub id: String,
    pub name: String,
    pub kind: Kind,
    pub group: IGroup,
    pub status: Status,
    #[serde(default)] pub fields: Vec<Field>,
    #[serde(default)] pub test: Option<TestKind>,
    #[serde(default)] pub help_url: Option<String>,
    #[serde(default)] pub credits: Vec<String>,
    #[serde(default)] pub source: Option<String>,
    #[serde(default)] pub setting: Option<String>,
    #[serde(default)] pub powers: Vec<String>,
    #[serde(default)] pub roadmap: Option<String>,
    #[serde(default)] pub needs: Vec<Input>,
}

pub fn all() -> &'static [Integration];             // OnceLock, parsed once; panics only on a broken build file (tested)
pub fn get(id: &str) -> Option<&'static Integration>;
pub fn working() -> impl Iterator<Item = &'static Integration>;
pub fn for_source(source: &str) -> Option<&'static Integration>;
pub fn problems(entries: &[Integration], credits: &[Credit], endpoints: &[Endpoint]) -> Vec<String>;
```

- [ ] **Step 0: Sync.** `git fetch origin && git merge origin/main && git push`
- [ ] **Step 1: Write the registry** `integrations/integrations.json` (2-space indent, Prettier-clean). Working entries in page order, then planned ones:

```json
{
  "entries": [
    {
      "id": "steamgriddb",
      "name": "SteamGridDB",
      "kind": "api-key",
      "group": "art",
      "status": "working",
      "fields": [{ "key": "steamgriddb-api-key", "input": "api-key", "required": true }],
      "test": "steamgriddb",
      "help_url": "https://www.steamgriddb.com/profile/preferences/api",
      "credits": ["steamgriddb"],
      "source": "steamgriddb",
      "setting": "sources.order.steamgriddb",
      "powers": ["fill"]
    },
    {
      "id": "screenscraper",
      "name": "ScreenScraper",
      "kind": "account",
      "group": "art",
      "status": "working",
      "fields": [
        { "key": "screenscraper-user", "input": "user", "required": true },
        { "key": "screenscraper-password", "input": "password", "required": true }
      ],
      "test": "screenscraper",
      "help_url": "https://www.screenscraper.fr/membreinscription.php",
      "credits": ["screenscraper"],
      "source": "screenscraper",
      "setting": "sources.order.screenscraper",
      "powers": ["fill", "organize-identify"]
    },
    {
      "id": "switch-catalog",
      "name": "Catálogo de Switch",
      "kind": "dataset",
      "group": "catalogs",
      "status": "working",
      "credits": ["titledb", "nlib", "nintendo-eshop-icons"],
      "setting": "catalogs.switch.enabled",
      "powers": ["organize-names"]
    },
    {
      "id": "updates",
      "name": "Actualizaciones",
      "kind": "link",
      "group": "updates",
      "status": "working",
      "setting": "general.update_channel",
      "powers": ["updates"]
    },
    { "id": "igdb", "name": "IGDB", "kind": "api-key", "group": "art", "status": "planned", "needs": ["client-id", "secret"], "roadmap": "31-fuentes-de-nombres-y-arte-por-title-id" },
    { "id": "retroachievements", "name": "RetroAchievements", "kind": "account", "group": "achievements", "status": "planned", "needs": ["user", "api-key"], "roadmap": "retroachievements" },
    { "id": "thegamesdb", "name": "TheGamesDB", "kind": "api-key", "group": "names", "status": "planned", "needs": ["api-key"], "roadmap": "31-fuentes-de-nombres-y-arte-por-title-id" },
    { "id": "rawg", "name": "RAWG", "kind": "api-key", "group": "names", "status": "planned", "needs": ["api-key"], "roadmap": "31-fuentes-de-nombres-y-arte-por-title-id" },
    { "id": "microsoft-store", "name": "Microsoft Store", "kind": "dataset", "group": "art", "status": "planned", "roadmap": "31-fuentes-de-nombres-y-arte-por-title-id" },
    { "id": "gametdb", "name": "GameTDB", "kind": "dataset", "group": "art", "status": "planned", "roadmap": "31-fuentes-de-nombres-y-arte-por-title-id" },
    { "id": "wikidata", "name": "Wikidata", "kind": "dataset", "group": "names", "status": "planned", "roadmap": "31-fuentes-de-nombres-y-arte-por-title-id" }
  ]
}
```

  (Prettier will reflow the one-line planned entries; run `pnpm exec prettier --write integrations/integrations.json` after Step 7 adds the folder to the format globs. Before committing, open both `help_url`s in a browser by hand: if one has moved, use the credit's home page (`https://www.steamgriddb.com/` / `https://www.screenscraper.fr/`) and note it for the PR — spec D10, §12.2. The `name` of `switch-catalog` and `updates` is the Spanish fallback only; the UI reads `integrations.service.<id>.name` from locales.)

- [ ] **Step 2: Write the failing tests** at the bottom of `crates/coverly-core/src/integrations.rs` (module body still `todo!()`), plus one in `credentials.rs`:

```rust
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
        assert!(p.iter().any(|m| m.starts_with("steamgriddb:") && m.contains("no-such-credit")), "{p:?}");
        i[0].credits.clear();
        assert!(problems(&i, &c, ENDPOINTS).iter().any(|m| m.contains("steamgriddb: needs a credit")));
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
        eps.push(Endpoint { prefix: "https://api.example.org/", credit: Some("tauri"), credentials: true });
        let p = problems(&i, &c, &eps);
        assert!(p.iter().any(|m| m.contains("https://api.example.org/") && m.contains("tauri")), "{p:?}");
    }

    #[test]
    fn every_credential_key_belongs_to_exactly_one_working_entry() {
        let (mut i, c) = shipped();
        let ss = i.iter().position(|e| e.id == "screenscraper").unwrap();
        i[ss].fields.pop();
        assert!(problems(&i, &c, ENDPOINTS).iter().any(|m| m.contains("screenscraper-password") && m.contains("no working entry")));
        let (mut i, c) = shipped();
        let sg = i[0].fields[0].clone();
        i[1].fields.push(sg);
        assert!(problems(&i, &c, ENDPOINTS).iter().any(|m| m.contains("steamgriddb-api-key") && m.contains("twice")));
    }

    #[test]
    fn planned_entries_carry_no_fields_and_no_test() {
        let (mut i, c) = shipped();
        let igdb = i.iter().position(|e| e.id == "igdb").unwrap();
        i[igdb].test = Some(TestKind::Steamgriddb);
        assert!(problems(&i, &c, ENDPOINTS).iter().any(|m| m.starts_with("igdb:") && m.contains("planned")));
    }

    #[test]
    fn help_urls_must_be_official_https_pages() {
        for (url, why) in [
            ("http://www.steamgriddb.com/profile/preferences/api", "not https"),
            ("https://www.steamgriddb.com/api?x=1", "query"),
            ("https://steamgriddb.example.com/api", "host"),
        ] {
            let (mut i, c) = shipped();
            i[0].help_url = Some(url.into());
            let p = problems(&i, &c, ENDPOINTS);
            assert!(p.iter().any(|m| m.starts_with("steamgriddb:") && m.contains(why)), "{url}: {p:?}");
        }
    }

    #[test]
    fn ids_are_unique_kebab_case() {
        let (mut i, c) = shipped();
        i[1].id = "SteamGridDB".into();
        assert!(problems(&i, &c, ENDPOINTS).iter().any(|m| m.contains("kebab-case")));
        let (mut i, c) = shipped();
        i[1].id = "steamgriddb".into();
        assert!(problems(&i, &c, ENDPOINTS).iter().any(|m| m.contains("used twice")));
    }

    #[test]
    fn sources_map_to_their_integration() {
        assert_eq!(for_source("steamgriddb").map(|e| e.id.as_str()), Some("steamgriddb"));
        assert_eq!(for_source("screenscraper").map(|e| e.id.as_str()), Some("screenscraper"));
        assert!(for_source("libretro").is_none());
        for e in working() {
            if let Some(s) = &e.source {
                assert!(crate::settings::KNOWN_SOURCES.contains(&s.as_str()), "{}: unknown source {s}", e.id);
            }
        }
    }
}
```

  In `credentials.rs` tests:

```rust
#[test]
fn keychain_names_are_pinned_so_stored_keys_keep_working() {
    let names: Vec<&str> = CredentialKey::ALL.iter().map(|k| k.account()).collect();
    assert_eq!(names, ["steamgriddb-api-key", "screenscraper-user", "screenscraper-password"]);
}
```

  (The `SERVICE` pin moves with the keychain in Task 3.)
- [ ] **Step 3: Run** `cargo test -p coverly-core integrations:: credentials::tests::keychain_names` — fails to compile / panics at `todo!()`.
- [ ] **Step 4: Implement.**
  - `net/hosts.rs`: add `pub credentials: bool` to `Endpoint`; `true` for the `STEAMGRIDDB` and `SCREENSCRAPER` entries, `false` elsewhere (`STEAMGRIDDB_CDN` is false: images need no key). Derive `Clone, Copy, Debug` on `Endpoint` if missing (the test clones the slice).
  - `credits.rs`: `pub(crate) fn url_problem`; add `pub fn all() -> &'static [Credit]` if the module does not expose one already (reuse its `OnceLock` parse).
  - `integrations.rs`: module doc (spec D2–D4), types above, `all()` with `OnceLock<Vec<Integration>>` parsing `{ "entries": [...] }` (`#[serde(deny_unknown_fields)]` wrapper struct), `get`, `working`, `for_source`, and `problems()`:
    1. ids unique and kebab-case (same rule as credits' `schema_problems`);
    2. every id in `credits` exists in `credits`; a working entry other than `kind: link` has ≥ 1 credit (`"{id}: needs a credit"`);
    3. planned: `fields.is_empty() && test.is_none()` (`"{id}: planned entries take no fields or test"`), `roadmap.is_some()`;
    4. working: `needs.is_empty()`; `test.is_some()` ⇒ `fields` non-empty;
    5. every `CredentialKey::ALL` owned by exactly one working entry (`"{key}: no working entry"` / `"{key}: owned twice (…)"`);
    6. every endpoint with `credentials: true` has `credit` in some working entry's `credits` (`"{prefix}: needs credentials but credit {credit} is in no working integration"`);
    7. `help_url`: `credits::url_problem` (message carries its reason: "not https", "has a query or fragment", …) and host without `www.` equals the host without `www.` of one of the entry's credits' `url` (`"{id}: help_url host … is not the host of its credit"`); `dataset`/`link` have none.
  - `lib.rs`: `pub mod integrations;`.
  - `integrations/README.md`: five lines — add an entry here, its credit in `credits/credits.json`, its copy under `integrations.service.<id>.*` in both locales; planned entries take `needs` and `roadmap`, never `fields`; run `cargo test -p coverly-core integrations` and `pnpm test src/lib/integrations`.
- [ ] **Step 5: Run** `cargo test -p coverly-core integrations:: credentials::` and `cargo test -p coverly-core credits::` (the `no_url_literal_outside_the_registry` test must stay green: URLs live in JSON, not `.rs`). Pass.
- [ ] **Step 6:** G-RUST.
- [ ] **Step 7: Commit and push:** `git add integrations crates/coverly-core && git commit -m "feat(core): add the integrations registry and its coverage tests" && git push && git status -sb`

---

## Task 2: Integration status model (pure) and TS loader

**Files:**
- Modify: `crates/coverly-core/src/integrations.rs`
- Create: `src/lib/integrations.ts`, `src/lib/integrations.test.ts`
- Modify: `tsconfig.json` (`"include": ["src", "locales", "credits", "integrations"]`), `package.json` (`format:check` and `format` globs gain `"integrations/**/*.json"`)

**Interfaces (produced):**
```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SsQuota { pub requests_today: u64, pub max_requests_per_day: u64, pub max_threads: u64 }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum State { Off, Unavailable, NotConfigured, Saved, Connected, Error, Quota }

/// What the last check or job said, and when (unix seconds).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Health { pub verdict: KeyCheck, pub at: i64 }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeenQuota { pub quota: SsQuota, pub at: i64 }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "detail", rename_all = "kebab-case")]
pub enum Reason { Check(KeyCheck), Keychain(KeychainKind), Missing(Input) }
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum KeychainKind { Unavailable, Locked, Denied, Failed }

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
pub const QUOTA_TTL_SECS: i64 = 24 * 60 * 60;
pub fn state(i: &StateInput) -> (State, Option<Reason>);
```

```ts
export type IntegrationId = 'steamgriddb' | 'screenscraper' | 'switch-catalog' | 'updates' | string;
export interface Integration { id: string; name: string; kind: 'api-key'|'account'|'dataset'|'link'; group: 'art'|'names'|'achievements'|'catalogs'|'updates'; status: 'working'|'planned'; fields: { key: CredentialKey; input: FieldInput; required: boolean }[]; test: 'steamgriddb'|'screenscraper'|null; help_url: string|null; credits: string[]; source: SourceId|null; setting: string|null; powers: string[]; roadmap: string|null; needs: FieldInput[] }
export const INTEGRATIONS: readonly Integration[];
export const WORKING: readonly Integration[];
export const PLANNED: readonly Integration[];
export function integration(id: string): Integration | undefined;
export function integrationForSource(source: SourceId): Integration | undefined;
export function integrationDomId(id: string): string; // `integration-${id}`
export function catalogCardState(s: CatalogStatus | null): CardState; // IP-1
```

- [ ] **Step 0: Sync.** `git fetch origin && git merge origin/main && git push`
- [ ] **Step 1: Write the failing Rust tests** (`integrations::tests`): a table over the D6 precedence, one row per adjacent pair plus the keychain rule:

```rust
#[test]
fn state_follows_the_precedence_of_d6() {
    let ss = get("screenscraper").unwrap();
    let both = [(CredentialKey::ScreenscraperUser, true), (CredentialKey::ScreenscraperPassword, true)];
    let half = [(CredentialKey::ScreenscraperUser, true), (CredentialKey::ScreenscraperPassword, false)];
    let now = 1_800_000_000;
    let quota = Some(SeenQuota { quota: SsQuota { requests_today: 20_000, max_requests_per_day: 20_000, max_threads: 1 }, at: now - 60 });
    let bad = Some(Health { verdict: KeyCheck::Invalid, at: now - 5 });
    let ok = Some(Health { verdict: KeyCheck::Ok, at: now - 5 });
    let base = |p| StateInput { entry: ss, enabled: true, built: true, presence: p, health: None, quota: None, now };
    let s = |i: StateInput| state(&i).0;
    assert_eq!(s(StateInput { enabled: false, built: false, ..base(Ok(&half)) }), State::Off);
    assert_eq!(s(StateInput { built: false, ..base(Ok(&half)) }), State::Unavailable);
    assert_eq!(s(StateInput { quota, ..base(Ok(&half)) }), State::NotConfigured);
    assert_eq!(s(StateInput { quota, health: bad, ..base(Ok(&both)) }), State::Quota);
    assert_eq!(s(StateInput { health: bad, ..base(Ok(&both)) }), State::Error);
    assert_eq!(s(StateInput { health: ok, ..base(Ok(&both)) }), State::Connected);
    assert_eq!(s(base(Ok(&both))), State::Saved);
    // «we could not look» is not «nothing stored»
    assert_eq!(state(&base(Err(KeychainKind::Locked))), (State::Error, Some(Reason::Keychain(KeychainKind::Locked))));
    // the missing field is named
    assert_eq!(state(&base(Ok(&half))).1, Some(Reason::Missing(Input::Password)));
    // quota expires 24 h after it was seen
    let old = Some(SeenQuota { at: now - QUOTA_TTL_SECS - 1, ..quota.unwrap() });
    assert_eq!(s(StateInput { quota: old, ..base(Ok(&both)) }), State::Saved);
    // a quota not used up is no state
    let fine = Some(SeenQuota { quota: SsQuota { requests_today: 3, ..quota.unwrap().quota }, ..quota.unwrap() });
    assert_eq!(s(StateInput { quota: fine, ..base(Ok(&both)) }), State::Saved);
    // rate-limited from a check is an error with that reason, not «quota» (D9)
    let rl = Some(Health { verdict: KeyCheck::RateLimited, at: now });
    assert_eq!(state(&StateInput { health: rl, ..base(Ok(&both)) }), (State::Error, Some(Reason::Check(KeyCheck::RateLimited))));
}

#[test]
fn a_memory_store_with_todays_keys_reads_as_saved() {
    use crate::credentials::{CredentialStore, MemoryStore};
    let store = MemoryStore::default();
    for k in CredentialKey::ALL { store.set(k, "x").unwrap(); }
    for e in working().filter(|e| !e.fields.is_empty()) {
        let presence: Vec<_> = e.fields.iter().map(|f| (f.key, store.get(f.key).unwrap().is_some())).collect();
        let i = StateInput { entry: e, enabled: true, built: true, presence: Ok(&presence), health: None, quota: None, now: 0 };
        assert_eq!(state(&i).0, State::Saved, "{}", e.id);
    }
}
```

  `Health` with `KeyCheck::Ok`/`Invalid`/`Offline`/`Unavailable`/`RateLimited` → `Connected` / `Error(reason)`; `KeyCheck::Missing` → ignored (presence already says it). Quota = `max > 0 && today >= max && now - at < QUOTA_TTL_SECS`.
- [ ] **Step 2: Write the failing TS tests** `src/lib/integrations.test.ts`:
  - `WORKING.map((e) => e.id)` equals `['steamgriddb', 'screenscraper', 'switch-catalog', 'updates']`; `PLANNED` has IGDB, RetroAchievements, TheGamesDB, RAWG, Microsoft Store, GameTDB, Wikidata and none has `fields.length > 0`.
  - every working entry has `integrations.service.<id>.use` and `.keywords` in both locales (import `locales/es/ui.json` / `locales/en/ui.json` as the parity test does; `privacy` too except `updates`… — `updates.privacy` exists per §7.4, so: every working entry has `use`, `privacy`, `keywords`; `steps` for entries with `help_url`; `unlocks` for every working entry but `updates`). (These locale keys land in Task 7; mark the locale assertions `it.todo` now and flip them in Task 7 — note it in the test name.)
  - every `credits[]` id exists in `CREDITS` (`src/lib/credits.ts`).
  - `integrationForSource('steamgriddb')?.id === 'steamgriddb'`, `integrationForSource('libretro') === undefined`.
  - `catalogCardState`: `off`→`off`; `pending`→`saved` (label «Pendiente»); `downloading`→`saved` (progress); `ready`→`connected`; `failed-old`, `failed-none`, `rejected`, `remove-failed`→`error`; `null`→`saved`.
- [ ] **Step 3: Run** `cargo test -p coverly-core integrations::` and `pnpm vitest run src/lib/integrations` — both fail.
- [ ] **Step 4: Implement** `state()` and the TS loader (`import raw from '../../integrations/integrations.json'`, normalising absent arrays/nulls; type the JSON through a narrow cast as `src/lib/credits.ts` does). Update `tsconfig.json` and both Prettier globs in `package.json` (`format` and `format:check`). Run `pnpm exec prettier --write integrations/integrations.json`.
- [ ] **Step 5: Run** both commands again — pass. Then G-RUST and G-WEB.
- [ ] **Step 6: Commit and push:** `git add crates/coverly-core src/lib integrations tsconfig.json package.json && git commit -m "feat(core): compute integration status and load the registry in the UI" && git push && git status -sb`

---

## Task 3: Keychain in core, `Denied`, classification by platform code

**Files:**
- Create: `crates/coverly-core/src/credentials/keychain.rs` (move `credentials.rs` → `credentials/mod.rs` with `git mv` so history follows)
- Modify: `crates/coverly-core/Cargo.toml`, `crates/coverly-core/src/credentials/mod.rs`, `src-tauri/Cargo.toml`, `src-tauri/src/lib.rs`, `src-tauri/src/error.rs`, `src-tauri/src/ipc.rs` (`AppInfo.platform`), `src/ipc/appInfo.ts`, `src/ipc/errors.ts`
- Delete: `src-tauri/src/keychain.rs`

**Interfaces (produced):**
```rust
// coverly_core::credentials
pub enum CredentialError { Unavailable, Locked, Denied, Failed(String), BadValue }
#[cfg(feature = "keychain")] pub mod keychain;   // KeychainStore, SERVICE, classify
// keychain.rs
pub const SERVICE: &str = "com.maecly.coverlyroms";
pub fn classify(e: &keyring::Error) -> CredentialError;
// src-tauri
pub struct AppInfo { pub name: String, pub version: String, pub core_version: String, pub platform: &'static str }
IpcError::KeychainDenied  // serialises as { "code": "keychain-denied" }
```

- [ ] **Step 0: Sync.** `git fetch origin && git merge origin/main && git push`
- [ ] **Step 1: Cargo.** In `crates/coverly-core/Cargo.toml`:

```toml
[features]
keychain = ["dep:keyring", "dep:security-framework", "dep:windows-native-keyring-store", "dep:secret-service"]

[dependencies]
keyring = { workspace = true, optional = true }

[target.'cfg(target_os = "macos")'.dependencies]
security-framework = { version = "3.7", optional = true }
[target.'cfg(windows)'.dependencies]
windows-native-keyring-store = { version = "1.1", optional = true }
[target.'cfg(target_os = "linux")'.dependencies]
secret-service = { version = "5.2", optional = true, default-features = false }
```

  Merge into the existing `[features]` and target tables (do not duplicate headers). Check with `cargo tree -p coverly-core -F keychain -d` that no second version of these crates appears; if `secret-service` needs a runtime feature to compile, copy exactly the features `zbus-secret-service-keyring-store` 1.0.1 enables (`cargo tree -e features -i secret-service`). In `src-tauri/Cargo.toml` replace `keyring = { workspace = true }` with `coverly-core = { …, features = ["keychain"] }` on the existing `coverly-core` line.
- [ ] **Step 2: Write the failing tests** in `keychain.rs` (moved tests kept, plus):

```rust
#[test]
fn the_service_name_is_pinned() {
    assert_eq!(SERVICE, "com.maecly.coverlyroms");
}

#[cfg(target_os = "macos")]
#[test]
fn macos_codes_are_classified() {
    use security_framework::base::Error as SfError;
    let boxed = |code: i32| keyring::Error::NoStorageAccess(Box::new(SfError::from_code(code)));
    let plat = |code: i32| keyring::Error::PlatformFailure(Box::new(SfError::from_code(code)));
    assert_eq!(classify(&plat(-25308)), CredentialError::Locked);
    for c in [-128, -25293] { assert_eq!(classify(&plat(c)), CredentialError::Denied, "{c}"); }
    for c in [-61, -25292, -25244] { assert_eq!(classify(&boxed(c)), CredentialError::Denied, "{c}"); }
    for c in [-25291, -25294, -25295] { assert_eq!(classify(&boxed(c)), CredentialError::Unavailable, "{c}"); }
    assert!(matches!(classify(&plat(-1)), CredentialError::Failed(_)));
}

#[cfg(windows)]
#[test]
fn windows_codes_are_classified() {
    use windows_native_keyring_store::PlatformError; // check the exact public path in 1.1.0 (`utils::PlatformError`, re-exported or not)
    let e = |code: u32| keyring::Error::PlatformFailure(Box::new(PlatformError(code)));
    assert_eq!(classify(&keyring::Error::NoStorageAccess(Box::new(PlatformError(1312)))), CredentialError::Unavailable);
    assert_eq!(classify(&e(5)), CredentialError::Denied);
    assert!(matches!(classify(&e(87)), CredentialError::Failed(_)));
}

#[cfg(target_os = "linux")]
#[test]
fn linux_errors_are_classified() {
    use secret_service::Error as Ss;
    let e = |x: Ss| keyring::Error::NoStorageAccess(Box::new(x));
    assert_eq!(classify(&e(Ss::Locked)), CredentialError::Locked);
    assert_eq!(classify(&e(Ss::Prompt)), CredentialError::Denied);
    assert_eq!(classify(&e(Ss::Unavailable)), CredentialError::Unavailable);
    assert_eq!(classify(&e(Ss::NoResult)), CredentialError::Unavailable);
}
```

  If the platform error is not public (Windows `PlatformError` lives in `utils`), the downcast uses whatever type the store boxes; read `~/.cargo/registry/src/*/windows-native-keyring-store-1.1.0/src/{lib,utils}.rs` first and adapt the test (fall back to `std::error::Error::source` chain + `raw_os_error` only if no type is reachable; say so in the PR). Error messages: the existing "no secret bytes" assertions stay.
  In `src-tauri/src/error.rs` tests: `IpcError::from(C::Denied) == IpcError::KeychainDenied` and it serialises to `{ "code": "keychain-denied" }`. In `src-tauri/src/lib.rs`: `app_info().platform` is one of `macos|windows|linux` and equals `std::env::consts::OS` on those three; the snake_case JSON test gains `"platform"`.
- [ ] **Step 3: Run** `cargo test -p coverly-core --features keychain keychain::` and `cargo test -p coverly-roms error:: app_info` (use the real package name from `src-tauri/Cargo.toml`) — fail.
- [ ] **Step 4: Implement.**
  - `git mv crates/coverly-core/src/credentials.rs crates/coverly-core/src/credentials/mod.rs`; `git mv src-tauri/src/keychain.rs crates/coverly-core/src/credentials/keychain.rs`; fix imports (`super::{CredentialError, CredentialKey, CredentialStore}`); `#[cfg(feature = "keychain")] pub mod keychain;` in `mod.rs`; add `#[error("access to the credential store was denied")] Denied`.
  - `classify`: `NoEntry` is never passed in; `NoDefaultStore` → `Unavailable`; `BadEncoding`/`BadDataFormat` → `Failed("the stored value is not text")` (never formatted); `NoStorageAccess(b) | PlatformFailure(b)` → `by_code(b.as_ref())` per OS:

```rust
#[cfg(target_os = "macos")]
fn by_code(e: &(dyn std::error::Error + Send + Sync + 'static)) -> Option<CredentialError> {
    let code = e.downcast_ref::<security_framework::base::Error>()?.code();
    Some(match code {
        -25308 => CredentialError::Locked,
        -128 | -25293 | -61 | -25292 | -25244 => CredentialError::Denied,
        -25291 | -25294 | -25295 => CredentialError::Unavailable,
        _ => return None,
    })
}
```

    Windows: `1312 → Unavailable`, `5 → Denied`. Linux: `Locked → Locked`, `Prompt | PromptDisconnected → Denied` (match the real variant names of 5.2), `Unavailable | Zbus(_) | ZbusFdo(_) | NoResult → Unavailable`. `None` → `Failed(e.to_string())` for `PlatformFailure`, `Locked` for an unrecognised `NoStorageAccess` (today's behaviour). Other variants → `Failed(other.to_string())`.
  - `src-tauri/src/lib.rs`: `pub use coverly_core::credentials::keychain;` so `keychain::KeychainStore::new()` keeps compiling; remove `pub mod keychain`.
  - `error.rs`: `#[error("access to the keychain was denied")] KeychainDenied` + `From` arm.
  - `ipc.rs`: `platform: std::env::consts::OS` (`&'static str`; for an OS other than the three the UI falls back to the generic copy). `src/ipc/appInfo.ts`: `platform: string`. `src/ipc/errors.ts`: add `'keychain-denied'` to the code list; locale keys arrive in Task 7 — add `errors.keychain-denied` to both locales now («El sistema no dio permiso para usar el llavero.» / "The system did not allow access to the keychain.") so parity stays green.
- [ ] **Step 5: Run** the Step 3 commands — pass. `cargo test --workspace` (src-tauri still uses `keychain::KeychainStore`).
- [ ] **Step 6:** G-RUST and G-WEB.
- [ ] **Step 7: Commit and push** (two commits): `git add -A crates/coverly-core/src/credentials* crates/coverly-core/Cargo.toml src-tauri Cargo.lock && git commit -m "refactor(core): move the keychain store into core behind a feature" && git push`, then the classify/Denied/platform changes: `git add -A crates src-tauri src/ipc locales && git commit -m "feat(core): classify keychain errors by platform code and add denied" && git push && git status -sb`. (If the move and the classify are already entangled in one diff, one commit `feat(core): …` is acceptable; never leave either unpushed.)

---

## Task 4: ScreenScraper quota and structured source troubles

**Files:**
- Modify: `crates/coverly-core/src/sources/screenscraper.rs`, `crates/coverly-core/src/sources/mod.rs`, `crates/coverly-core/src/gather/mod.rs`, `crates/coverly-core/src/plan/mod.rs`, `src-tauri/src/sources.rs` (caller of `check_account`)

**Interfaces (produced):**
```rust
// screenscraper.rs
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AccountCheck { pub verdict: KeyCheck, pub quota: Option<SsQuota> }
pub fn check_account(http, cfg, dev, user) -> AccountCheck;
impl ScreenScraperSource<'_> { pub fn last_quota(&self) -> Option<SsQuota>; }
fn quota_of(user: &Value) -> Option<SsQuota>;   // shared by note_user and check_account

// sources/mod.rs
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TroubleKind { MissingKey, Auth, Exhausted }
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceTrouble { pub source: String, pub kind: TroubleKind }
impl SourceError { pub fn trouble(&self) -> Option<TroubleKind>; }

// gather
pub struct Gathered { …, #[serde(default, skip_serializing_if = "Vec::is_empty")] pub troubles: Vec<SourceTrouble>,
                      #[serde(default, skip_serializing_if = "BTreeSet::is_empty")] pub answered: BTreeSet<String> }
// plan::PlanSummary gains the same two fields (copied from gathered)
```

- [ ] **Step 0: Sync.** `git fetch origin && git merge origin/main && git push`
- [ ] **Step 1: Write the failing tests.**
  - `screenscraper.rs`, with `coverly_core::net::testing::MockServer` serving `ssuserInfos.php`:
    - body `{"response":{"ssuser":{"requeststoday":"1234","maxrequestsperday":"20000","maxthreads":"1"}}}` → `AccountCheck { verdict: Ok, quota: Some(SsQuota { requests_today: 1234, max_requests_per_day: 20000, max_threads: 1 }) }`;
    - 2xx body `not json` → `Ok`, `None`; 2xx JSON without `maxrequestsperday` → `Ok`, `None`;
    - 401 / 423 → `verdict: RateLimited` (today's mapping, unchanged), `quota: None`;
    - no user → `Missing` with no request made (server hit count 0).
  - `screenscraper.rs`: errors never carry credentials — for a `Status` (500), an `Offline` (closed port) and an unregistered host, `format!("{e} {e:?}")` of the returned `SourceError` contains neither `sspassword`, `ssid`, `devpassword`, the test user nor the test password.
  - `screenscraper.rs`: after a run's JSON answer with `ssuser`, `last_quota()` returns the numbers; before any answer, `None`.
  - `gather/mod.rs`: with fake sources returning `MissingKey`, `Auth`, `Exhausted` once each, `troubles` holds one `SourceTrouble` per source with the matching kind (deduplicated like `skipped`), `warnings` is byte-identical to a golden vector captured from today's code (write the golden from `main` before changing gather), and a source that returned `Ok` at least once is in `answered`.
- [ ] **Step 2: Run** `cargo test -p coverly-core screenscraper:: gather::` — fail.
- [ ] **Step 3: Implement.** `quota_of` reads `requeststoday`, `maxrequestsperday`, `maxthreads` with the existing `num` helper (all three present → `Some`); `note_user` calls it and stores the result in a new `last_quota: Mutex<Option<SsQuota>>` besides its existing threads/exhausted logic (behaviour unchanged). `check_account` parses `response.ssuser` from the 2xx body (`serde_json::from_slice`, errors → `None`). In gather, where `e.is_source_wide()` pushes the warning, also push `SourceTrouble { source: src.id().into(), kind }` when `skipped` is first inserted; record `answered.insert(src.id())` on `Ok(found)`. Copy both into `PlanSummary` where `warnings: gathered.warnings.clone()` is set. `src-tauri/src/sources.rs::test_source_key` uses `.verdict` (keeps returning `KeyCheck`; Task 5 adds the quota path).
- [ ] **Step 4: Run** the same command — pass; `cargo test --workspace` (plan JSON snapshots, if any, must not change because both fields skip when empty).
- [ ] **Step 5:** G-RUST.
- [ ] **Step 6: Commit and push:** `git add crates src-tauri && git commit -m "feat(core): report ScreenScraper quota and structured source troubles" && git push && git status -sb`

---

## Task 5: App back end — integrations view, test, forget, help, health

**Files:**
- Create: `src-tauri/src/integrations.rs`
- Modify: `src-tauri/src/{lib,ipc,state,sources,prefs}.rs`, `crates/coverly-core/src/settings/*` (test only)

**Interfaces (produced):**
```rust
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FieldView { pub key: CredentialKey, pub set: bool }
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct IntegrationView {
    pub id: String, pub enabled: bool, pub built: bool, pub fields: Vec<FieldView>,
    pub state: State, #[serde(skip_serializing_if = "Option::is_none")] pub reason: Option<Reason>,
    #[serde(skip_serializing_if = "Option::is_none")] pub checked_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")] pub quota: Option<SsQuota>,
    #[serde(skip_serializing_if = "Option::is_none")] pub quota_at: Option<i64>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TestResult { pub verdict: KeyCheck, #[serde(skip_serializing_if = "Option::is_none")] pub quota: Option<SsQuota> }

impl AppState {
    pub fn integrations_view(&self) -> Vec<IntegrationView>;
    pub fn test_integration(&self, id: &str) -> Result<TestResult, IpcError>;
    pub fn forget_integration(&self, id: &str) -> Result<Vec<IntegrationView>, IpcError>;
    pub fn note_troubles(&self, troubles: &[SourceTrouble], answered: &BTreeSet<String>);
}
#[tauri::command] pub async fn integrations_view(app) -> Result<Vec<IntegrationView>, IpcError>;
#[tauri::command] pub async fn test_integration(app, id: String) -> Result<TestResult, IpcError>;
#[tauri::command] pub async fn forget_integration(app, id: String) -> Result<Vec<IntegrationView>, IpcError>;
#[tauri::command] pub fn open_integration_help(app, id: String) -> Result<(), IpcError>;
// prefs.rs
LinkId::Roadmap  // "https://github.com/MAECLY/coverly-roms/blob/main/TODO.md"
```

- [ ] **Step 0: Sync.** `git fetch origin && git merge origin/main && git push`
- [ ] **Step 1: Write the failing tests** (`src-tauri/src/integrations.rs`, `AppState::for_tests()` or the constructor the other state tests use, with `MemoryStore`):
  - Migration (D21): a store with the three keys under today's names → view of `steamgriddb` is `saved` with `fields[0].set`, `screenscraper` is `unavailable` in a build without dev credentials (`DevCredentials::from_build()` is `None` in tests) with both fields `set`; JSON of the whole view contains neither stored value.
  - The view reads presence through the key cache: two `integrations_view()` calls make at most three `MemoryStore::reads()` in total; a `set_credential` invalidates (next view re-reads).
  - Keychain fails (`MemoryStore::unavailable()`) → every credentialed card `error` with `reason: { kind: "keychain", detail: "unavailable" }`, never `not-configured`.
  - `forget_integration("screenscraper")` clears both keys (even when the first clear fails on a store that refuses one key — use a small `FailOnce` test store); planned or unknown id → `BadRequest`; `"switch-catalog"` → `BadRequest` (no credentials).
  - `test_integration` against a `MockServer` endpoint for SteamGridDB (`ok`, 401 → `invalid`) updates health: the next view is `connected` / `error` with `checked_at`. Unknown id → `BadRequest`.
  - `note_troubles`: `Auth` → `error/invalid`; `Exhausted` → `quota` state for ScreenScraper with `quota_at`; `answered` → `connected`; `MissingKey` → unchanged.
  - `sources_info`: for every `KNOWN_SOURCES` id, `needs` equals the registry's `fields[].key` of `integrations::for_source(id)` (empty when none) — ScreenScraper now lists its two keys; `ready` stays `built && needs all usable`.
  - `prefs.rs`: `LinkId::Roadmap` in the existing list test; serialises as `"roadmap"`.
  - Settings carry no secrets (D17), in `crates/coverly-core/src/settings` tests: put `"sk-LEAK"` into a `MemoryStore` under every key, serialise a fully populated `Settings` (every optional set; `network.proxy_url` = `https://proxy.example:8080`), assert the JSON contains no `CredentialKey::account()` name and no `sk-LEAK`; the test names `network.proxy_url` as the one exempt field in a comment pointing at §12.5.
  - Log lines: `test_integration` logs `integration checked: <id> -> <verdict>`; `forget_integration` logs `credentials forgotten: <id>`; a test with a capturing logger (or reading the formatted string the function builds) shows no value.
- [ ] **Step 2: Run** `cargo test -p <src-tauri package> integrations:: sources:: prefs::` and `cargo test -p coverly-core settings::` — fail.
- [ ] **Step 3: Implement.**
  - `AppState` gains `health: Mutex<BTreeMap<String, Health>>` and `ss_quota: Mutex<Option<SeenQuota>>` (memory only, §12.4). Presence: reuse `source_keys()` for usable values plus `credential_status()`-style presence for the per-field `set` flags, cached alongside `keys` (one `read_store` pass fills both; invalidated by `forget_keys()`). A failing read maps `CredentialError` → `KeychainKind`.
  - `enabled`: `sources.order[id].enabled` for entries with `source`; `catalogs.switch.enabled` for `switch-catalog`; `true` for `updates`. `built`: `DevCredentials::from_build().is_some()` for ScreenScraper, else `true`.
  - `test_integration`: maps `TestKind` to the existing code in `sources.rs` (`steamgriddb::check_key` / `check_account`), returns `quota` from `AccountCheck`, stores `Health` and `SeenQuota` (`now` from `crate::time` or `coverly_core::time` helper used elsewhere).
  - `forget_integration`: clears every `fields[].key`, all attempted, `forget_keys()` once, returns the fresh view; on partial failure returns the first error after attempting all.
  - `open_integration_help`: `integrations::get(&id)` → `help_url` → `app.opener().open_url` (like `open_credit`); none → `BadRequest`.
  - `note_troubles` is called where the fill job finishes (find the place `PlanSummary` / job result is stored in `state.rs`); organise identify calls it with ScreenScraper's `last_quota()`/exhausted state when the source was used.
  - `sources_info` `needs` ← registry.
  - Register the four commands in `lib.rs`'s `generate_handler!`; add `LinkId::Roadmap` + URL in `prefs.rs` (the only file besides `net/hosts.rs` allowed to hold project URLs).
- [ ] **Step 4: Run** the Step 2 commands — pass.
- [ ] **Step 5:** G-RUST.
- [ ] **Step 6: Commit and push:** `git add src-tauri crates && git commit -m "feat(app): integrations view, connection test, forget and help commands" && git push && git status -sb`

---

## Task 6: IPC types, Tauri backend and mock

**Files:**
- Modify: `src/ipc/types.ts`, `src/ipc/backend.ts`, `src/ipc/tauriBackend.ts` (+ `.test.ts`), `src/ipc/mock/mockSettings.ts` (+ `.test.ts`), `src/ipc/mock/mockJobs.ts` / `mockOrganize.ts` (only to add `troubles`/`integration_notices` fields to the fixtures they build), `src/ipc/contract.test.ts`

**Interfaces (produced):**
```ts
export type IntegrationState = 'off' | 'unavailable' | 'not-configured' | 'saved' | 'connected' | 'error' | 'quota';
export type KeychainKind = 'unavailable' | 'locked' | 'denied' | 'failed';
export type FieldInput = 'user' | 'password' | 'api-key' | 'client-id' | 'secret';
export type IntegrationReason =
  | { kind: 'check'; detail: KeyCheck } | { kind: 'keychain'; detail: KeychainKind } | { kind: 'missing'; detail: FieldInput };
export interface SsQuota { requests_today: number; max_requests_per_day: number; max_threads: number }
export interface IntegrationView {
  id: string; enabled: boolean; built: boolean; fields: { key: CredentialKey; set: boolean }[];
  state: IntegrationState; reason?: IntegrationReason; checked_at?: number; quota?: SsQuota; quota_at?: number;
}
export interface IntegrationTestResult { verdict: KeyCheck; quota?: SsQuota }
export type IntegrationProblem = 'missing' | 'rejected' | 'quota' | 'off';
export interface IntegrationNotice { integration: string; problem: IntegrationProblem }
export interface SourceTrouble { source: SourceId; kind: 'missing-key' | 'auth' | 'exhausted' }
// PlanSummary: troubles?: SourceTrouble[]; OrganizePlan: integration_notices?: IntegrationNotice[]
// Backend
integrationsView(): Promise<IntegrationView[]>;
testIntegration(id: string): Promise<IntegrationTestResult>;
forgetIntegration(id: string): Promise<IntegrationView[]>;
openIntegrationHelp(id: string): Promise<void>;
// LinkId gains 'roadmap'
```

- [ ] **Step 0: Sync.** `git fetch origin && git merge origin/main && git push`
- [ ] **Step 1: Write the failing tests.**
  - `tauriBackend.test.ts`: each new method calls `integrations_view` / `test_integration {id}` / `forget_integration {id}` / `open_integration_help {id}`.
  - `contract.test.ts`: the mock implements the four calls; their results have the shapes above.
  - `mockSettings.test.ts`, URL switches read from `window.location.search` once at backend creation (same pattern as `?mockPick=`):
    - default: SteamGridDB `not-configured` until `setCredential('steamgriddb-api-key', 'x')`, then `saved`; after `testIntegration('steamgriddb')` → `connected` (`ok`), with a key starting `bad` → `error` + `reason {check, invalid}`; a key starting `offline` → `offline`; ScreenScraper `unavailable` (no dev credentials, the state of every official build).
    - `?mockIntegrations=empty`: nothing stored, catalogue off. `saved`: all three keys stored. `error`: SteamGridDB stored + rejected. `quota`: dev build, ScreenScraper stored, `state: 'quota'`, `quota: {requests_today: 20000, max_requests_per_day: 20000, max_threads: 1}`, `quota_at` now. `nodev`: same as default (explicit). `dev`: ScreenScraper built; `testIntegration('screenscraper')` → `ok` with quota `{ requests_today: 1234, max_requests_per_day: 20000, max_threads: 1 }`.
    - `?mockKeychain=locked|denied|unavailable`: `credentialStatus().available === false` for `unavailable`; `setCredential` rejects with `keychain-<kind>`; the view's credentialed cards are `error` with `reason {keychain, <kind>}`.
    - `?mockPlatform=macos|windows|linux` → `appInfo().platform`.
    - `forgetIntegration('screenscraper')` clears both keys; `forgetIntegration('igdb')` rejects `bad-request`.
    - No mock state ever stores a credential value (existing rule): after `setCredential(k, 'sk-SECRET')`, `JSON.stringify(localStorage)` has no `sk-SECRET`.
- [ ] **Step 2: Run** `pnpm vitest run src/ipc` — fail.
- [ ] **Step 3: Implement** types, backend interface, Tauri calls and mock (the mock computes state with a TS port of the D6 precedence kept in `mockSettings.ts`, tested above; it is test scaffolding, not shipped logic). Update the module doc comment of `mockSettings.ts` with the new switches.
- [ ] **Step 4: Run** `pnpm vitest run src/ipc` — pass. G-WEB.
- [ ] **Step 5: Commit and push:** `git add src/ipc && git commit -m "feat(ipc): integrations calls, types and mock states" && git push && git status -sb`

---

## Task 7: Copy, keychain messages per OS

**Files:**
- Modify: `locales/es/ui.json`, `locales/en/ui.json`, `src/lib/integrations.test.ts` (flip the `it.todo`s from Task 2)
- Create: `src/components/settings/keychainMessage.ts`, `src/components/settings/keychainMessage.test.ts`

**Interfaces (produced):**
```ts
/** `errors.keychain-<kind>.<os>` when it exists in the catalogue, else `errors.keychain-<kind>`. */
export function keychainMessageKey(kind: KeychainKind, platform: string, has: (key: string) => boolean): string;
```

- [ ] **Step 0: Sync.** `git fetch origin && git merge origin/main && git push`
- [ ] **Step 1: Write the failing tests.** `keychainMessage.test.ts`: `locked` + `macos` → `errors.keychain-locked.macos`; `locked` + `windows` → `errors.keychain-locked` (no Windows locked copy); `denied` + `linux` → `errors.keychain-denied.linux`; `unavailable` + `freebsd` → `errors.keychain-unavailable`; `failed` + any → `errors.keychain-failed`. In `src/lib/integrations.test.ts` flip the locale `it.todo`s to real tests.
- [ ] **Step 2: Run** `pnpm vitest run src/components/settings/keychainMessage src/lib/integrations src/i18n` — fail.
- [ ] **Step 3: Add the copy** exactly as spec §7.1–§7.4 (es and en), plus:
  - `integrations.service.<id>.name` for `switch-catalog` («Catálogo de Switch» / "Switch catalogue") and `updates` («Actualizaciones de Coverly» / "Coverly updates"); planned: `integrations.service.<id>.use` one line each («Arte, metadatos e ids externos» / "Art, metadata and external ids"; «Logros por juego» / "Achievements per game"; «Fanart, logos y fichas» / "Fanart, logos and game details"; «Fondos y fichas, solo para ver» / "Backgrounds and details, view only"; Microsoft Store / GameTDB / Wikidata «Arte y nombres por id» / "Art and names by id").
  - `integrations.field.<input>.label` for `user` («Usuario» / "User name"), `password` («Contraseña» / "Password"), `api-key` («Clave de API» / "API key"), `client-id` («Client ID» / "Client ID"), `secret` («Secret» / "Secret"); `.saved` per §7.1 (gendered); `integrations.needs.<input>` for the Próximamente lines («client ID y secret de Twitch» is built from `needs` joined with the locale's list format — use `Intl.ListFormat`, not hand-made «y»).
  - `integrations.state.<state>` tags and `integrations.stateLine.*` extra lines per §6.3 (including `off.fill`, `off.organize`, `missing.api-key`, `missing.account`, `missing.password`, `missing.user`, `saved` «Sin comprobar en esta sesión.», `connected` «Comprobado {time}.», `quota` line with `{used}`/`{max}`), catalogue labels `integrations.state.pending` / `downloading` / `ready`.
  - `integrations.check.<keycheck>` (§7.2), `integrations.catalog.offHint` (D12 copy), `integrations.badValue` (§9 copy), `integrations.saving` («Guardando…» / "Saving…"), `integrations.missingAccount` / `integrations.missingKey` («Sin cuenta» / «Sin clave»), `integrations.updates.channel` («Canal: {channel}» / "Channel: {channel}"), `integrations.updates.change` («Cambiar en General» / "Change in General"), `integrations.updates.checkOnLaunch` («Buscar al abrir, en General» / "Check on launch, in General"), `integrations.notice.*` (§7.4 end), `integrations.organize.switchOff` («Los nombres de Switch salen del archivo; activa el catálogo de Switch para usar los oficiales.» / "Switch names come from the file; turn on the Switch catalogue to use the official ones."), `integrations.keywords.planned` («próximamente, igdb, retroachievements, thegamesdb, rawg, microsoft store, gametdb, wikidata, twitch» / same in en).
  - `errors.keychain-<kind>.<os>` per §7.3. These are nested under `errors` as `"keychain-locked": …` already exists as a string: put the OS variants as flat keys `"keychain-locked.macos"` only if the i18n loader supports dotted flat keys; otherwise (check `src/i18n/core.ts`) use `errors.keychainOs.locked.macos` and adapt `keychainMessageKey` — the test names the real keys.
  - Reword `fill.popularityNoKey`: «SteamGridDB no está listo, así que no se usan votos.» / "SteamGridDB is not ready, so votes are not used." (the link is added in Task 12).
  - Keep Spanish rules: sentence case, no «!», numbers as placeholders (rendered mono by the component).
- [ ] **Step 4: Run** the Step 2 command — pass. G-WEB.
- [ ] **Step 5: Commit and push:** `git add locales src && git commit -m "feat(i18n): copy for the Integrations tab and keychain messages per OS" && git push && git status -sb`

---

## Task 8: SecretField, status tag, IntegrationLink and IntegrationNotice

**Files:**
- Create: `src/components/settings/SecretField.tsx` (+ `.test.tsx`), `IntegrationStatusTag.tsx`, `IntegrationLink.tsx`, `IntegrationNotice.tsx` (+ `IntegrationNotice.test.tsx`)
- Modify: `src/components/settings/SourcesSection.tsx` (`CredentialRow` uses `SecretField` for now; removed in Task 11), `src/dev/UiGallery.tsx` (tags + notice samples)

**Interfaces (produced):**
```tsx
export function SecretField(props: {
  k: CredentialKey; input: FieldInput; set: boolean; disabled: boolean;
  /** Called with the typed value; resolves when stored. The field empties itself after. */
  onSave(value: string): Promise<void>;
  describedBy?: string;
}): JSX.Element;
export function IntegrationStatusTag(props: { state: IntegrationState | CardState; label?: string }): JSX.Element;
export function IntegrationLink(props: { id: string; children?: React.ReactNode; variant?: 'link' | 'button' }): JSX.Element;
export function IntegrationNotice(props: { notice: IntegrationNotice }): JSX.Element;
```

- [ ] **Step 0: Sync.** `git fetch origin && git merge origin/main && git push`
- [ ] **Step 1: Write the failing tests.**
  - `SecretField`: `type=password`, `autoComplete="off"`, `spellCheck={false}`, `name` is `integration-<key>` (not `password`/`username`); «Mostrar» has `aria-pressed` + `aria-controls` and flips `type` to `text`; Save disabled while empty; after a resolved save the input is empty, `type` back to `password`, and «Guardada en el llavero» / «Guardado en el llavero» shows per input gender with a check icon; after a rejected save the input is empty and the error text is shown; `set` never puts any value in the input (`input.value === ''` with `set: true`); while saving the button says «Guardando…» and is disabled.
  - `IntegrationStatusTag`: each state renders its word and an icon (`svg[aria-hidden]`); dashed ones carry `border-dashed`; no state is colour-only (text content non-empty).
  - `IntegrationLink`: click calls `openSettings('integrations', { focus: 'integration-<id>' })`, has `data-settings-opener`; unknown or planned id renders nothing.
  - `IntegrationNotice`: `{steamgriddb, missing}` → «SteamGridDB sin clave» + «Configurar»; `{screenscraper, missing}` → «ScreenScraper sin cuenta»; `rejected`, `quota`, `off` per §7.4; it is a `role="note"` `Alert` with variant `warning`.
- [ ] **Step 2: Run** `pnpm vitest run src/components/settings/SecretField src/components/settings/IntegrationNotice` — fail.
- [ ] **Step 3: Implement** (frontend-design: tags use `Badge` variants `outline` + `border-dashed border-cr-border-strong`, `tag` (Manila), `primary`, `warning`, `danger`; icons lucide `CircleDashed`, `Check`, `KeyRound`, `TriangleAlert`, `CircleSlash`, `Gauge`). Extract `SecretField` from `CredentialRow` keeping its generation-counter logic in the caller.
- [ ] **Step 4: Run** the same command — pass; existing `SourcesSection.test.tsx` still passes. G-WEB.
- [ ] **Step 5: Commit and push:** `git add src && git commit -m "feat(ui): secret field, status tag and integration links" && git push && git status -sb`

---

## Task 9: IntegrationCard

**Files:**
- Create: `src/components/settings/IntegrationCard.tsx`, `src/components/settings/IntegrationCard.test.tsx`
- Modify: `src/styles/globals.css` (only if a utility cannot express the dashed tag or the row; prefer Tailwind classes)

**Interfaces (produced):**
```tsx
export function IntegrationCard(props: {
  entry: Integration;            // from the registry
  view: IntegrationView;         // from integrationsView()
  /** Position in sources.order and its length, for «Fuentes: posición n de m». */
  order?: { n: number; total: number };
  platform: string;
  onView(next: IntegrationView[]): void;   // forget/save refresh
  children?: React.ReactNode;    // the Switch catalogue body (Task 10)
}): JSX.Element;
```

Structure (D14, §6.4): `<article data-setting-row data-advanced-fold="integration-<id>">` → `<h3><button id="integration-<id>" aria-expanded aria-controls="integration-<id>-body" aria-describedby="integration-<id>-use">` with icon by kind (`KeyRound`/`UserRound`/`Database`/`RefreshCw`), name, `IntegrationStatusTag`, chevron; `<p id="integration-<id>-use">`; body `<div id="integration-<id>-body" hidden={!open}>` always mounted (no Radix unmount; use `Collapsible` with `forceMount` like `AdvancedFold`). Open state from `navStore.openFolds` (`setFold`). Body order: «Activado» switch (`settingId` of the entry's `setting`, same patch Fuentes writes; disabled until usable, D12; the `sources.order` field error shown next to it), order line, fields (`SecretField` per `fields`), action row (Guardar is per field inside `SecretField`; «Probar conexión», «Olvidar credenciales»), test result line (`role="status"` live region mounted empty), quota line (mono, `Intl.NumberFormat`), «Cómo conseguir la clave/cuenta» + steps (button → `openIntegrationHelp(id)`, `ExternalLink` icon), «Qué se envía» + privacy line, «Crédito en Agradecimientos» (`openSettings('credits', { focus: 'credit-<creditId>' })` — check the credits tab's DOM id pattern in `CreditsSection.tsx`).

- [ ] **Step 0: Sync.** `git fetch origin && git merge origin/main && git push`
- [ ] **Step 1: Write the failing tests** (mock backend via `getBackend` test helper used in `SourcesSection.test.tsx`):
  - Header accessible name is «SteamGridDB, Sin configurar»; heading level 3; `aria-expanded` toggles; closed body is in the DOM but `hidden` and its controls are not tabbable.
  - Saving a key: the view refreshes to «Guardada», the input is empty and `document.body.innerHTML` does not contain the typed value.
  - «Probar conexión» for each of the six `KeyCheck`s shows the §7.2 copy in the live region (the region exists before the first message); a stale answer after a save is dropped (generation counter); during the test the button shows `Spinner` + «Probando…».
  - ScreenScraper `unavailable`: fields editable, test button disabled with the D22 reason as its description.
  - Quota: `{1234, 20000, 1}` renders «Hoy: 1234 de 20.000 peticiones» and «Hilos: 1» in es, «Today: 1,234 of 20,000 requests» in en, inside `font-mono` spans; «visto hace 2 minutos» from `formatRelative`; no quota line without numbers.
  - Forget: click → a `role="group"` labelled by «¿Olvidar las credenciales de ScreenScraper? …» replaces the action row, focus on «Cancelar»; Escape restores the row with focus on «Olvidar credenciales» and stops propagation (a parent `keydown` spy is not called); confirm calls `forgetIntegration('screenscraper')` and the tag becomes «Sin configurar»; `vi.spyOn(window, 'confirm')` never called.
  - Toggle writes `sources.order` with only that entry's `enabled` flipped (same patch as Fuentes); disabled when not usable; turning the last enabled source off shows the validation error next to the switch.
  - Keychain error state: reason `keychain/locked` on `macos` shows `errors.keychain-locked.macos` copy.
  - Help button calls `openIntegrationHelp('steamgriddb')` (never `openUrl` with a URL).
- [ ] **Step 2: Run** `pnpm vitest run src/components/settings/IntegrationCard` — fail.
- [ ] **Step 3: Implement** with frontend-design (spec §6.1–§6.5: single column row, `rounded-lg`, `bg-cr-surface` closed / `bg-cr-surface-raised` open, `border-cr-border`, no shadow; wrapping flex so it fits at 560 px and 200 % zoom; motion only `animate-collapsible-*` with `motion-reduce:animate-none`). Card starts open when state is `error`/`quota` (D26) — set via `setFold` on first render only if the user has not toggled it this session.
- [ ] **Step 4: Run** — pass. Screenshot the card in the gallery (light/dark) and critique against spec §6.2 before committing. G-WEB.
- [ ] **Step 5: Commit and push:** `git add src && git commit -m "feat(ui): integration card with status, fields, test and forget" && git push && git status -sb`

---

## Task 10: Integraciones tab, Switch catalogue card, Próximamente

**Files:**
- Create: `src/components/settings/IntegrationsSection.tsx` (+ `.test.tsx`), `src/components/settings/SwitchCatalogGroup.tsx` (moved verbatim from `SourcesSection.tsx`, lines `CATALOG_DOWNLOAD_BYTES` … end of `SwitchCatalogGroup`)
- Modify: `src/stores/navStore.ts`, `src/stores/navStore.test.ts`, `src/screens/SettingsScreen.tsx`, `src/screens/SettingsScreen.test.tsx`, `src/components/settings/SourcesSection.tsx` (import from the new file; still rendered there until Task 11), `src/components/settings/registry.ts` (`SettingsFold` gains `integrations-switch-advanced` and `integration-${string}`)

**Interfaces (produced):**
```ts
// navStore
SettingsSection |= 'integrations'; SETTINGS_SECTIONS: [... 'sources', 'integrations', 'network', ...]; SETTINGS_GROUPS.integrations = 'covers'
```

- [ ] **Step 0: Sync.** `git fetch origin && git merge origin/main && git push`
- [ ] **Step 1: Write the failing tests.**
  - `navStore.test.ts` / `settingsNav` test: `integrations` follows `sources`, group `covers`, label «Integraciones».
  - `IntegrationsSection.test.tsx`:
    - one card per working entry in order SteamGridDB, ScreenScraper (group «Portadas y arte», `h2`), Catálogo de Switch («Catálogos»), Actualizaciones («Actualizaciones»); no «Nombres y metadatos» / «Logros» headings.
    - Próximamente lists the seven planned names with their needs (built with `Intl.ListFormat`) and use line, contains **no** `input`, `switch` or `button` except «Ver el plan», which calls `openLink('roadmap')`.
    - footer links: «Proxy y límites en Red» → `openSettings('network')`, «Orden de fuentes en Fuentes» → `openSettings('sources')`, «Créditos en Agradecimientos» → `openSettings('credits')`.
    - empty state (`?mockIntegrations=empty`-equivalent mock): the D6.6 sentence and «Configurar SteamGridDB» which opens and focuses `integration-steamgriddb`; every card open; no ScreenScraper sentence in a build without dev credentials, present with `dev`.
    - keychain unavailable: one OS-specific `Alert` at the top, fields disabled, switches still enabled.
    - Switch catalogue card: status line from `SwitchCatalogStatusLine`, actions «Descargar» / «Buscar actualización» / «Borrar catálogo», icons switch, and a nested fold `integrations-switch-advanced` with the nlib fallback switch and URL (mono); its tag follows `catalogCardState`; the D12 off-hint text; ids `setting-integrations-switch-catalog`, `-icons`, `-fallback`, `-fallback-url`; toggling writes `catalogs.switch.enabled` exactly as before (copy the existing `SourcesSection.test.tsx` catalogue assertions and point them here).
    - Actualizaciones card: «Canal: Estable», «Cambiar en General» → `openSettings('general', { focus: 'setting-general-update-channel' })`, «Ver versiones» → `openLink('releases')`; no switch, no credit link.
    - Deep link: `openSettings('integrations', { focus: 'integration-screenscraper' })` opens that card (fold) and focuses its header (`SettingsScreen` arrival effect, no new code).
    - Opening the tab triggers no `testIntegration` call (spy).
  - `SettingsScreen.test.tsx`: the tab renders through `SECTION_COMPONENTS`; while settings load it shows the same loading state as other non-standalone tabs.
- [ ] **Step 2: Run** `pnpm vitest run src/components/settings/IntegrationsSection src/screens/SettingsScreen src/stores` — fail.
- [ ] **Step 3: Implement.** `IntegrationsSection` loads `integrationsView()`, `sourcesInfo()`, `appInfo()` once (`alive` guard like `SourcesSection`), groups `WORKING` by `group` in the order art → names → achievements → catalogs → updates, skipping empty groups; renders `IntegrationCard` per entry (Switch catalogue passes `SwitchCatalogGroup` body as children, with the old ids renamed `sources.switch_catalog*` → `integrations.switch_catalog*` in `settingId(...)` and fold `sources-advanced` → `integrations-switch-advanced`); Próximamente as a `<ul>` with `id="integrations-planned"` `tabIndex={-1}` (search anchor). Add the section to `navStore` and `SECTION_COMPONENTS`.
- [ ] **Step 4: Run** — pass. Run the app in the browser (`pnpm dev --port 1469 --strictPort`, `?mockIntegrations=empty|saved|quota`), screenshot light/dark at 1280 and 900 px, critique against §6.1/§6.2 (frontend-design), fix, stop the server by port. G-WEB.
- [ ] **Step 5: Commit and push:** `git add src && git commit -m "feat(ui): Integrations settings tab with the Switch catalogue and coming-later list" && git push && git status -sb`

---

## Task 11: Fuentes keeps the order; search finds integrations

**Files:**
- Modify: `src/components/settings/SourcesSection.tsx` (+ `.test.tsx`), `src/components/settings/registry.ts` (+ `registry.test.tsx`, `search.test.ts`), `locales/{es,en}/ui.json` (D7 moves/removals)

- [ ] **Step 0: Sync.** `git fetch origin && git merge origin/main && git push`
- [ ] **Step 1: Write the failing tests.**
  - `SourcesSection.test.tsx`: no `input[type=password]`, no «Cuentas» heading, no Switch catalogue controls; SteamGridDB and ScreenScraper rows show «Configurar en Integraciones» (and only those rows), which opens `integrations` focused on `integration-<id>`; the hint reads «Sin clave» (SteamGridDB) / «Sin cuenta» (ScreenScraper) instead of `settings.sources.needsKey`; the «usable» rule unchanged (SteamGridDB switch disabled without a key, an enabled source can be turned off); ScreenScraper without dev credentials keeps `settings.sources.notBuilt`.
  - `registry.test.tsx`: completeness passes with the Integraciones tab mounted (every `setting-` / `integration-` control has an entry and vice versa); the old `setting-sources-switch-catalog*` ids are gone.
  - `search.test.ts`: in es and en, «api key» → `integration-steamgriddb` first; «contraseña» / "password" → `integration-screenscraper`; «token» → SteamGridDB; «igdb» → `integrations-planned`; «screenscraper» → `integration-screenscraper`; «titledb» → `setting-integrations-switch-catalog`; «nlib» → the fallback URL entry (fold breadcrumb «Integraciones › Opciones avanzadas»).
- [ ] **Step 2: Run** `pnpm vitest run src/components/settings` — fail.
- [ ] **Step 3: Implement.**
  - `SourcesSection`: delete `CredentialRow`, the Cuentas block, the keychain alerts and `<SwitchCatalogGroup />`; readiness still from `credentialStatus()` + `sourcesInfo()`; add `IntegrationLink` («Configurar en Integraciones», `variant="link"`) on rows where `integrationForSource(e.id)?.fields.length`.
  - `registry.ts`: `SETTINGS_INDEX` entries — one per working integration with fields or a card: `{ id: 'integration-<id>', section: 'integrations', label: 'integrations.service.<id>.name' (or the plain name string for SteamGridDB/ScreenScraper as `LocalText`), hint: 'integrations.service.<id>.use', keywords: 'integrations.service.<id>.keywords' }`; the four Switch settings renamed and moved to `section: 'integrations'`, the two folded ones `fold: 'integrations-switch-advanced'`; `{ id: 'integrations-planned', section: 'integrations', label: 'integrations.group.planned', keywords: 'integrations.keywords.planned' }`. Generate the integration entries from `WORKING` (no hand-kept list), so a new working entry is indexed with no code.
  - Locales: move `settings.sources.switchCatalog.*` → `integrations.switchCatalog.*` (update every `t()` user in `SwitchCatalogGroup.tsx`/`SwitchCatalogStatus.tsx`); delete `settings.sources.credential*`, `settings.sources.check.*`, `stored`, `notStored`, `save*`, `remove*`, `show`, `testKey*`, `testing`, `needsKey` after `git grep -n` shows no user; add `settings.sources.configureIn` if `integrations.configureIn` is not reused.
- [ ] **Step 4: Run** `pnpm vitest run src` — pass. G-WEB.
- [ ] **Step 5: Commit and push** (two commits): `git add src/components/settings/SourcesSection* locales && git commit -m "refactor(ui): Sources keeps the order and links to Integrations" && git push`, then `git add src/components/settings/registry* src/components/settings/search* && git commit -m "feat(ui): settings search finds integrations by service and field" && git push && git status -sb`

---

## Task 12: Notices where an account is missing or a quota is used up

**Files:**
- Modify: `src/components/fill/FillWizard.tsx` (+ `.test.tsx`), the fill summary component that renders `PlanSummary.warnings` (find with `git grep -n "summary.warnings\|plan.warnings" src/components/fill src/screens`), `src/screens/OrganizeScreen.tsx` (+ `.test.tsx`), `src-tauri/src/state.rs` (organise plan `integration_notices`, IP-2; fill summary troubles → notices; `note_troubles` call), `src/lib/integrations.ts` (`noticesFromTroubles`)

**Interfaces (produced):**
```ts
export function noticesFromTroubles(t: SourceTrouble[] | undefined): IntegrationNotice[]; // missing-key→missing, auth→rejected, exhausted→quota; mapped through integrationForSource; deduped
```
```rust
// organise plan view (src-tauri): pub integration_notices: Vec<IntegrationNotice>  (serde default, skip if empty)
```

- [ ] **Step 0: Sync.** `git fetch origin && git merge origin/main && git push`
- [ ] **Step 1: Write the failing tests.**
  - `FillWizard.test.tsx`: with the popularity strategy and SteamGridDB not ready, the existing alert shows the reworded `fill.popularityNoKey` plus a «Configurar» `IntegrationLink` to `integration-steamgriddb`; its condition is unchanged (not shown for other strategies or when ready).
  - Fill summary: a plan with `troubles: [{source:'steamgriddb', kind:'missing-key'}]` shows «SteamGridDB sin clave · Configurar»; `exhausted` on ScreenScraper → «Cuota de ScreenScraper agotada por hoy»; no troubles → no notice; warning strings still rendered as before.
  - `src/lib/integrations.test.ts`: `noticesFromTroubles` mapping and dedupe; a trouble for `libretro` (no integration) is dropped.
  - `OrganizeScreen.test.tsx`: `integration_notices: [{integration:'screenscraper', problem:'missing'}]` renders the notice with the link; `{integration:'switch-catalog', problem:'off'}` renders `integrations.organize.switchOff` + «Configurar»; a `catalogue-missing` plan warning renders **without** any link to the Switch card; Back from Settings returns to the plan with focus on the link (`data-settings-opener`).
  - Rust (`state.rs` tests): organise plan notices — ScreenScraper enabled + no account → `missing`; enabled + quota state → `quota`; disabled → none; unavailable build → none (it cannot be enabled); Switch items in the pile + catalogue off → `switch-catalog/off`; no Switch items → none. After a fill job with `Auth` on SteamGridDB, `integrations_view` reports `error/invalid` (D29).
  - No change in `DuplicatesScreen` (assert in the PR: `git diff origin/main -- src/screens/DuplicatesScreen.tsx` is empty; spec D28, §12.6).
- [ ] **Step 2: Run** `pnpm vitest run src/components/fill src/screens/OrganizeScreen src/lib/integrations` and `cargo test -p <src-tauri package> state::` — fail.
- [ ] **Step 3: Implement.** Organise plan notices in `src-tauri` (IP-2; Switch items = plan items whose system id is `switch`); wire `note_troubles` at fill job end and organise identify end; UI renders `IntegrationNotice` above the existing warnings list. Update `src/ipc/mock/mockOrganize.ts` / `mockJobs.ts` so `?mockPick=downloads` (Switch items present, catalogue off by default) shows the Switch notice, and a fill without a SteamGridDB key shows the missing notice when SteamGridDB is enabled — keep existing fixture tests green.
- [ ] **Step 4: Run** — pass. G-WEB and G-RUST.
- [ ] **Step 5: Commit and push:** `git add src src-tauri && git commit -m "feat(ui): link missing accounts and used-up quotas to their integration" && git push && git status -sb`

---

## Task 13: CLI reads the keychain safely; every CLI test opts out

**Files:**
- Create: `crates/coverly-cli/src/keys.rs`, `crates/coverly-cli/tests/common/mod.rs`
- Modify: `crates/coverly-cli/Cargo.toml` (`coverly-core = { …, features = ["keychain"] }`), `crates/coverly-cli/src/{main,fill,organize,catalog}.rs`, every file in `crates/coverly-cli/tests/`

**Interfaces (produced):**
```rust
pub const NO_KEYCHAIN_ENV: &str = "COVERLY_NO_KEYCHAIN";
pub const KEYCHAIN_TIMEOUT: Duration = Duration::from_secs(5);
/// None when COVERLY_NO_KEYCHAIN=1.
pub fn cli_store() -> Option<Arc<dyn CredentialStore>>;
/// Runs each call on a helper thread; a timeout is `Unavailable`.
pub struct TimedStore { inner: Arc<dyn CredentialStore>, timeout: Duration }
pub struct KeyPlan { pub steamgriddb: bool, pub screenscraper: bool, pub default_endpoints: bool }
/// Env first; keychain only for enabled sources with an empty env var and only towards default endpoints (D19).
pub fn resolve_keys(env: impl Fn(&str) -> Option<String>, store: Option<&dyn CredentialStore>, plan: KeyPlan, note: &mut dyn FnMut(&str)) -> SourceKeys;
```
```rust
// tests/common/mod.rs
pub fn coverly() -> assert_cmd::Command; // cargo_bin("coverly") + COVERLY_SETTINGS="" + COVERLY_NO_KEYCHAIN=1
```

- [ ] **Step 0: Sync.** `git fetch origin && git merge origin/main && git push`
- [ ] **Step 1: Switch existing tests to the helper (IP-3).** Create `tests/common/mod.rs`; in every `tests/cli_*.rs` add `mod common; use common::coverly;` and replace `Command::cargo_bin("coverly").unwrap()` with `coverly()` (`sed -i '' 's/Command::cargo_bin("coverly")\.unwrap()/coverly()/; s/Command::cargo_bin("coverly")/coverly_bin()/' …` then fix by hand any variant the sed misses; drop now-duplicate `.env("COVERLY_SETTINGS", "")` only where it repeats the helper's value — tests that set a real settings path keep their own `.env`, which overrides). Add a guard test in `tests/cli_version.rs`: read every `tests/*.rs` with `std::fs`, fail if `cargo_bin("coverly")` appears outside `common/mod.rs`. Run `cargo test -p coverly-cli` — all existing tests pass. Commit and push: `git add crates/coverly-cli/tests && git commit -m "test(cli): run every CLI test with the keychain turned off" && git push`.
- [ ] **Step 2: Write the failing unit tests** in `keys.rs` (`MemoryStore`, no real keychain):
  - env set → env used, store not read (`reads() == 0`);
  - env empty, source enabled, default endpoints → store value used;
  - env empty, `default_endpoints: false` (an overridden `--steamgriddb-url`) → no key and `reads() == 0`, one note «saved keys are not sent to a custom endpoint» (D19);
  - source disabled → store not read;
  - store `unavailable` → empty keys with one note `keychain: unavailable`;
  - `TimedStore` over a store whose `get` sleeps 200 ms with a 50 ms timeout → `Err(Unavailable)` within ~100 ms;
  - `cli_store()` is `None` with `COVERLY_NO_KEYCHAIN=1` (test via an injected env lookup, not by mutating the process env).
- [ ] **Step 3: Run** `cargo test -p coverly-cli keys::` — fail.
- [ ] **Step 4: Implement** and wire: `fill.rs` and `organize.rs` replace `SourceKeys::from_env(...)` with `resolve_keys(...)` (`default_endpoints` = the matching `--…-url` override is `None`; enabled from settings / `--sources`); notes go to stderr prefixed `note: `. Rewrite hints: `fill` `after_help` → «SteamGridDB key: COVERLY_SGDB_KEY or `coverly integrations set steamgriddb` (saved in the system keychain).»; the unknown-source message keeps naming `COVERLY_SGDB_KEY` and adds the `integrations set` hint; `catalog.rs` `ENABLE_HINT` → «turn on «Catálogo de Switch» in the app's Integrations settings (`catalogs.switch.enabled: true` in the settings file)». Update the module docs of `fill.rs` («the CLI has no keychain» is now wrong).
- [ ] **Step 5: Run** `cargo test -p coverly-cli` — pass (existing `cli_fill`/`cli_catalog` assertions on hint text updated in the same commit). G-RUST.
- [ ] **Step 6: Commit and push:** `git add crates/coverly-cli Cargo.lock && git commit -m "feat(cli): read saved keys from the keychain after the environment" && git push && git status -sb`

---

## Task 14: `coverly integrations list|test|set|forget`

**Files:**
- Create: `crates/coverly-cli/src/integrations.rs`, `crates/coverly-cli/tests/cli_integrations.rs`
- Modify: `crates/coverly-cli/src/main.rs` (subcommand + exit-code doc)

**Interfaces (produced):**
```rust
#[derive(Debug, clap::Args)]
pub struct IntegrationsArgs { #[command(subcommand)] pub command: IntegrationsCommand }
#[derive(Debug, clap::Subcommand)]
pub enum IntegrationsCommand {
    /// Every integration, whether it is on and which credentials are set (never their values).
    List { #[arg(long)] json: bool },
    /// One authenticated request with the saved or environment credentials.
    Test { id: String, #[arg(long)] json: bool, #[arg(long, hide = true)] steamgriddb_url: Option<String>, #[arg(long, hide = true)] screenscraper_url: Option<String> },
    /// Save a credential, read from stdin (no echo on a terminal) or from --from-env.
    Set { id: String, #[arg(long)] field: Option<FieldArg>, #[arg(long, value_name = "VAR")] from_env: Option<String> },
    /// Delete the integration's credentials from the keychain.
    Forget { id: String, #[arg(long)] yes: bool },
}
pub fn run(args: &IntegrationsArgs) -> Result<(), Failure>;
// testable core:
pub fn list_text(store: Option<&dyn CredentialStore>, settings: &Settings) -> String;
pub fn set_value(store: &dyn CredentialStore, id: &str, field: Option<FieldArg>, read: &mut dyn BufRead, env: &dyn Fn(&str) -> Option<String>, from_env: Option<&str>) -> Result<(), Failure>;
pub fn exit_for(check: KeyCheck) -> u8; // Ok 0; Invalid|Missing|Unavailable 1; Offline|RateLimited 5
```

- [ ] **Step 0: Sync.** `git fetch origin && git merge origin/main && git push`
- [ ] **Step 1: Write the failing tests.**
  - Unit (`MemoryStore`): `set_value` from a reader `"sk-123\r\n"` stores `sk-123` (CRLF trimmed); from `--from-env VAR` with `VAR=sk-9`; empty stdin → exit 1 «no value read from stdin»; `--from-env UNSET` → exit 1 naming `UNSET` only; ScreenScraper without `--field` → usage (2) «choose --field user or --field password»; `steamgriddb --field password` → usage; planned (`igdb`) or unknown id → usage (2); `switch-catalog`/`updates` → usage «has no credentials». `list_text` shows `steamgriddb  on  api-key: set`, `screenscraper  off  user: set, password: not set`, `switch-catalog  off`, `updates`, a `keychain: off|available|unavailable` line, and a «coming later: IGDB, RetroAchievements, …» line; never the value; never `network.proxy_url` even when it holds `user:pass@`. `--json` shape `{ keychain, integrations: [{ id, enabled, fields: [{ key, set }] }], planned: [ids] }`. `exit_for` table.
  - `assert_cmd` (`common::coverly()`, so `COVERLY_NO_KEYCHAIN=1`): `integrations list` exits 0 and prints `keychain: off`; `integrations set steamgriddb` with stdin `sk-123` exits 1 («the keychain is off») and stderr does not contain `sk-123`; `integrations set steamgriddb sk-123` exits 2 (stray positional); `integrations test igdb` exits 2; `integrations test steamgriddb --steamgriddb-url <MockServer>` with `COVERLY_SGDB_KEY=sk-test` → 0 and prints `ok` (server answers 200), → 1 `invalid` (401), → 5 `offline` (closed port); stdout/stderr never contain `sk-test`; `forget steamgriddb` on a non-TTY without `--yes` → 2.
- [ ] **Step 2: Run** `cargo test -p coverly-cli integrations` — fail.
- [ ] **Step 3: Implement.** No-echo read on a TTY: unix `libc::tcgetattr`/`tcsetattr` clearing `ECHO` (restore on drop, also on error); Windows `windows-sys` `GetConsoleMode`/`SetConsoleMode` clearing `ENABLE_ECHO_INPUT` (both crates are already workspace deps; add the `Win32_System_Console` feature to the CLI's `windows-sys` if missing). Prompt «Value for steamgriddb api-key (input hidden): » on stderr only when stdin is a TTY (`std::io::IsTerminal`). `forget` asks «Forget the screenscraper credentials? [y/N] » on a TTY. `test` uses `resolve_keys` with the endpoint overrides (keychain values only to defaults) and the same `key_check_net` limits as the app (one try, ≤10 s; duplicate the small function or move it to core `net` — prefer moving `key_check_net` into `coverly_core::net` and re-using it from `src-tauri/src/sources.rs`). Main: `Command::Integrations(integrations::IntegrationsArgs)` with doc «Manage the accounts and keys of online services (values are never printed).»; extend the exit-code doc comment with the D20 meanings.
- [ ] **Step 4: Run** `cargo test -p coverly-cli` — pass. G-RUST.
- [ ] **Step 5: Commit and push:** `git add crates && git commit -m "feat(cli): integrations list, test, set and forget" && git push && git status -sb`

---

## Task 15: Docs and changelog

**Files:**
- Modify: `docs/04-architecture/settings.md`, `docs/04-architecture/desktop-app.md`, `docs/04-architecture/art-sources.md`, `docs/04-architecture/fill-pipeline.md`, `README.md`, `README.es.md`, `CHANGELOG.md`

- [ ] **Step 0: Sync.** `git fetch origin && git merge origin/main && git push`
- [ ] **Step 1:** `git grep -n "Settings → Sources\|Ajustes → Fuentes\|no keychain\|no tiene llavero\|la CLI no\|Cuentas" -- docs README.md README.es.md CHANGELOG.md` and fix every hit that is now wrong: accounts live in Settings › Integrations; the CLI reads `COVERLY_*` env vars first, then the keychain (default endpoints only), and `COVERLY_NO_KEYCHAIN=1` turns it off; document `coverly integrations list|test|set|forget` with a `set` example reading stdin (`printf %s "$KEY" | coverly integrations set steamgriddb`) — never a key on the command line; `settings.md` gets the tab, the registry file and the «add a service» recipe; `desktop-app.md` the four new commands and `AppInfo.platform`. `CHANGELOG.md` «Unreleased»: Added (Integraciones tab, CLI integrations, per-OS keychain messages, ScreenScraper quota), Changed (Fuentes keeps the order; Switch catalogue moved), no Removed for users.
- [ ] **Step 2:** `pnpm format:check` (Markdown under `src/**` only; docs are not in the glob — keep their style by hand). G-WEB.
- [ ] **Step 3: Commit and push:** `git add docs README.md README.es.md CHANGELOG.md && git commit -m "docs: document Integrations, the CLI integrations command and keychain use" && git push && git status -sb`

---

## Task 16: Real verification and browser E2E

**Files:** none committed except fixes found here (each fix its own `fix(…)` commit + push). Screenshots stay in `/private/tmp/claude-501/-Users-miguelangelesparzacalero-Documents-GitHub/35b7c020-f2ae-412f-b757-99c24a9a498b/scratchpad/integrations/` and go into the PR description, not the repo.

- [ ] **Step 1: Update with main.** `git fetch origin && git merge origin/main`. Resolve conflicts in `locales/*.json` (keep both sides' keys), `src/components/*` and anything else; never take a side wholesale. Commit the merge (`git commit --no-edit`) and `git push && git status -sb`.
- [ ] **Step 2: Full gates**, all green, outputs kept for the PR evidence comment (Actions is blocked by billing; local gates are the CI):
  `pnpm check-all && pnpm build && cargo fmt --all --check && cargo clippy --workspace --all-targets --all-features -- -D warnings && scripts/dev/clippy-windows.sh && cargo test --workspace --no-fail-fast`
- [ ] **Step 3: Secret sweeps.**
  - `git diff origin/main -- . ':!*.md' | grep -nE 'sk-[A-Za-z0-9]{8,}|password\s*=\s*"[^"]{4,}'` prints only test fixtures (`sk-test`, `sk-123`, `sk-LEAK`, `sk-SECRET`).
  - `cargo run -q -p coverly-cli -- integrations list` and `--json` on this Mac (real keychain read-only; it may show a macOS prompt — choose «Deny» once to see the `Denied` message, then «Allow»): output shows `set`/`not set` only. Do **not** run `set` or `forget` against the real keychain; if a manual round trip is wanted, run the ignored test with its test service: `cargo test -p coverly-core --features keychain -- --ignored real_keychain_round_trip`.
  - `COVERLY_NO_KEYCHAIN=1 cargo run -q -p coverly-cli -- integrations set steamgriddb sk-1` exits 2; `printf 'sk-1\n' | COVERLY_NO_KEYCHAIN=1 cargo run -q -p coverly-cli -- integrations set steamgriddb` exits 1 and prints no `sk-1`.
- [ ] **Step 4: Read-only check on real data.** `mkdir -p target/integrations-sample && cp -RXp ~/Documents/ROMS/<one small system folder with Switch titles, picked with ls> target/integrations-sample/`; `find target/integrations-sample -name '._*'` prints nothing. Run `COVERLY_NO_KEYCHAIN=1 cargo run -q -p coverly-cli -- organize target/integrations-sample --report json > /private/tmp/claude-501/-Users-miguelangelesparzacalero-Documents-GitHub/35b7c020-f2ae-412f-b757-99c24a9a498b/scratchpad/integrations/organize.json` (dry run; real flags from `coverly organize --help`) and check stderr/notes mention `coverly integrations` only where the catalogue is off. Then `rm -rf target/integrations-sample`.
- [ ] **Step 5: Start the mock UI.** `lsof -ti tcp:1469 -sTCP:LISTEN | xargs -r kill; pnpm dev --port 1469 --strictPort` (background).
- [ ] **Step 6: E2E** with Chrome DevTools MCP, `new_page` with `isolatedContext: "integrations"` (fallbacks per Global Constraints), spec §10:
  1. `http://localhost:1469/?mockIntegrations=empty`: open Settings › Integraciones; empty-state note; «Configurar SteamGridDB» opens and focuses the card.
  2. Type a key, «Mostrar», «Guardar»: tag «Guardada», field empty; `evaluate_script(() => document.body.innerHTML.includes('<typed key>'))` is `false`.
  3. «Probar conexión» with a key → «Funciona»; save a key starting `bad` → «Credenciales incorrectas»; a key starting `offline` → «Sin red»; tag follows.
  4. `?mockIntegrations=quota`: ScreenScraper open by default, «Hoy: 20.000 de 20.000 peticiones», «Hilos: 1», tag «Cuota agotada».
  5. Forget: confirm row, Escape (focus back on «Olvidar credenciales»), then confirm; tag «Sin configurar»; `list_console_messages` shows no `window.confirm`.
  6. Settings search «contraseña» → ScreenScraper card focused; «igdb» → Próximamente; «nlib» → fallback URL inside the opened fold.
  7. Fuentes → «Configurar en Integraciones» → card; Back returns to Fuentes.
  8. Library (`?mockPick=mixed`) → Rellenar with popularity strategy → notice «Configurar» → card → Back returns to the wizard. Organise with `?mockPick=downloads` → Switch catalogue notice → card → Back keeps the plan.
  9. Keyboard only through one card (Tab order: toggle, fields + Mostrar, actions, links); screenshots light and dark at 1280 px, 900 px and 200 % zoom (`emulate` / `resize_page`); once with `prefers-reduced-motion: reduce`; once with `forced-colors: active` (tags readable from their words).
  10. `?mockIntegrations=nodev`: ScreenScraper «No disponible», test disabled with its reason, no ScreenScraper sentence in the empty state.
  11. `?mockKeychain=locked&mockPlatform=macos` → macOS locked copy; `locked&mockPlatform=windows` → generic locked copy; `denied&mockPlatform=linux` → Linux copy.
  12. `lighthouse_audit` (accessibility) on the tab; `list_console_messages` has no errors; `list_network_requests` while opening the tab shows no request beyond the dev server (no network on tab open).
- [ ] **Step 7: Stop the server** `lsof -ti tcp:1469 -sTCP:LISTEN | xargs -r kill`.
- [ ] **Step 8: Manual checks list for the PR** (the mock cannot cover them): real macOS keychain prompt after an update (ad-hoc signing, §12.1); Windows Credential Manager from a normal session and from a scheduled task (1312 copy); Linux without a secret service (headless/SSH) and with a locked GNOME Keyring; CLI over SSH on macOS (`security unlock-keychain` hint); both «Cómo conseguir la clave» pages open the right official page; Spanish OS tool names checked on a Spanish system («Acceso a Llaveros», «Administrador de credenciales», «Contraseñas y claves»).
- [ ] **Step 9: PR text inputs** (the PR is opened by the orchestrator): list of owner decisions §12.1–§12.8 with defaults and alternatives, deviations IP-1–IP-3, gate outputs, E2E screenshots.
- [ ] **Step 10: Final state.** `git status -sb` shows `## feat/integrations-tab...origin/feat/integrations-tab` with nothing ahead and a clean tree.

---

## Spec coverage

| Spec item | Task |
|---|---|
| D1 tab id, place, group | 10 |
| D2 registry file in Rust and TS | 1, 2 |
| D3 registry shape, `sources_info` needs ← registry | 1, 5 |
| D4 registry tests (credits, credentialed endpoints, keys, help URLs) | 1 |
| D5 `CredentialKey` and `SERVICE` pinned | 1, 3 |
| D6 status model | 2, 5 |
| D7 what stays in Fuentes; locale moves | 11 |
| D8 secret fields | 8, 9 |
| D9 «Probar conexión» outcomes | 5, 9 |
| D10 «Cómo conseguir la clave» by id | 1, 5, 9 |
| D11 ScreenScraper quota | 4, 5, 9 |
| D12 enable/disable, catalogue off behaviour | 9, 10 |
| D13 «Olvidar credenciales» in-page confirm | 5, 9 |
| D14 deep links, mounted bodies | 9, 10 |
| D15 search | 11 |
| D16 keychain errors per OS | 3, 7 |
| D17 no secrets in settings, logs, errors | 4, 5, 16 |
| D18 CLI keychain, `COVERLY_NO_KEYCHAIN`, 5 s cap | 3, 13 |
| D19 CLI key precedence, default endpoints only | 13, 14 |
| D20 CLI shape and exit codes | 14 |
| D21 migration, cached presence | 1, 5 |
| D22 ScreenScraper without dev credentials | 5, 6, 9, 10 |
| D23 Switch catalogue card (IP-1) | 2, 10 |
| D24 Actualizaciones link card, `LinkId::Roadmap` | 5, 10 |
| D25 no logos | 8, 9 |
| D26 which cards start open | 9, 10 |
| D27 `IntegrationNotice`, `SourceTrouble` | 4, 8, 12 |
| D28 where notices appear (IP-2; none in Duplicados) | 12, 13 |
| D29 status after jobs | 5, 12 |
| §6 design, §6.4 a11y, §6.5 narrow windows, §6.6 empty state | 9, 10, 16 |
| §7 copy | 7 (+ 11 moves) |
| §10 tests and E2E | every task; 16 |
| §12 owner decisions | Global Constraints; 16 (PR) |
