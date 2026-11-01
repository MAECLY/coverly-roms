# Adjustments after Systems Batches 1–3 (adjustments 1, 2, 4, 5, 6 and 7) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking. The UI tasks (Tasks 18, 19, 20, 21 and the viewer of Task 22) also REQUIRE frontend-design:frontend-design: follow `docs/08-brand/brand.md` and spec §9, use the `src/components/ui` primitives and app tokens only, and critique the changed screens with screenshots (light and dark, 1280 and 760 px) before committing.

**Goal:** Make the 131 systems trustworthy on real piles without any key: Switch names from the optional, on-demand `blawar/titledb` catalogue (NCA ids through `ncas.json`, opt-in eShop icons, optional `api.nlib.cc` fallback), unresolved Switch/3DS titles labelled «Cifrado»; loose executables and title metadata classified by their plaintext headers (homebrew, game, update, DLC, part of a game); a per-item «Sí / Sí a todo / No» question for huge items in the app and the CLI; title-id and folder-tree duplicates acting through the existing duplicates system with undo; folder games inside zip/7z found by the existing probes over a virtual listing, and loose multi-disc games grouped with an `.m3u`; and a searchable «Agradecimientos» settings tab fed by one `credits/credits.json`, with build-time third-party notices bundled and viewable in the app, and the licence gaps of `credits-inventory.md` §6 fixed.

**Architecture:** Everything extends what batches 1–3 built. Readers stay in `consoles/` over `Budgeted` (D24 budgets); a new `consoles/loose.rs` covers only the formats no reader owns, and the existing readers (`wiiu.rs`, `nx.rs`, `xbox.rs`, `ctr.rs`) are extended in place so nothing shadows them. The catalogue is a new `catalog/` module over its **own** `catalog.sqlite` (never the shared cache, whose `SCHEMA_VERSION` stays 3), fed by a streaming whitelist reducer behind a new `Http::get_stream`; identification asks it through a `TitleNames` trait carried on `OrganizeEnv`. Every network URL moves into one `net/hosts.rs` registry that `UreqHttp` enforces and the credits test checks. Folder probes move from direct disk calls to a `ListingFs` trait (`DiskFs` unchanged behaviour, `ArchiveFs` over the entries inspection already listed). The huge-item gate is one core function (`organize::gate`) used by IPC and CLI; «No» is a real `Choice::Decline`. Credits are one JSON file read by the front end, `coverly-core` (`include_str!`), the README script and the landing (by copy, after merge).

**Tech Stack:** Rust (workspace MSRV 1.93), `serde`/`serde_json` (streaming `Deserializer` + visitor), `rusqlite`, `ureq` 3, `unicode-normalization`, `sha1`, `zip`, `sevenz-rust2`, `insta`, `filetime`, `tempfile`; React 19 + TypeScript + Tailwind 3.4 + zustand, Vitest + Testing Library, `@tanstack/react-virtual` (already a dependency); `cargo-about` 0.9.2 (developer machine only, never in CI or release jobs). **No new crate and no new npm runtime package.**

**Spec:** [`docs/superpowers/specs/2026-10-26-adjustments.md`](../specs/2026-10-26-adjustments.md). It is binding: decisions D1–D33 (§4), files (§5), Switch flow (§6), catalogue lifecycle (§7), classification tables (§8), UX (§9), copy (§10, word for word), data flow (§11), edge cases (§12), owner decisions Q1–Q18 (§13), tests (§14) and the adversarial findings R1–R19 (§18). This plan turns it into tasks and records its deviations below. The batch 2/3 plans ([`2026-10-26-systems-batch-2.md`](2026-10-26-systems-batch-2.md), [`2026-10-26-systems-batch-3.md`](2026-10-26-systems-batch-3.md)) are the pattern for builders, harnesses and snapshots reused here.

## Global Constraints

- **Worktree and branch.** Work only in `/Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-adjustments`, branch `feat/adjustments`, which tracks `origin/feat/adjustments`. Agent shells reset their cwd, so every shell command starts with `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-adjustments &&`. Never switch branches and never touch `main`.
- **Git rules (mandatory).** An earlier run lost a whole branch because it was never pushed.
  - Conventional, granular commits in English (`feat(core): …`, `test(core): …`, `refactor(core): …`, `feat(cli): …`, `feat(ipc): …`, `feat(ui): …`, `feat(credits): …`, `build: …`, `docs: …`).
  - **No** `Co-Authored-By` line, no "Generated with" line, no mention of Claude or AI anywhere (commits, PR, code comments). This owner rule overrides any harness attribution.
  - Run `git push` **immediately after every commit**. Every task ends with `git commit … && git push && git status -sb`; the first line must read `## feat/adjustments...origin/feat/adjustments` with no `[ahead N]`.
  - Never force-push, never rebase. To update with `main`: `git fetch origin && git merge origin/main`, resolve conflicts by hand (other workflows change `locales/*.json`, `src/components/*` and `TODO.md` in parallel; resolve locale files key by key, never by taking one side), run the gates, commit the merge, push.
- **Do not edit** `docs/TRACKER.md` or `TODO.md` on this branch (they are updated on `main` after merge).
- **Scope.** Adjustments 1, 2, 4, 5, 6 and 7 only. **Not here:** adjustment 3 (frontend media folders, `TODO.md` §6), launchers `.psvita`/`.psn`/serial files (`TODO.md` §6 «Lanzadores»), every `TODO.md` §7.1 source except `blawar/titledb`, and any user-keys mode («Claves aportadas por el usuario», parked).
- **Keys rule (spec D1), no exceptions.** No code path reads, requests, bundles, downloads, links or logs console keys (`prod.keys`, `title.keys`, `keys.txt`, `aes_keys.txt`, `seeddb.bin`, `boot9`…). Key files stay left by name before any open (`is_key_material`). Nothing opens a `.tik`, `.cert`, `.nca` or `.ncz` body, nor the XCI 0x190 gamecard info. The catalogue never stores `key`, `rightsId`, `cnmts`, `cheats`, and an entry with a non-null `key` abandons the import (D5, Q11). No log line, error or status echoes catalogue content. The UI never says where keys come from. `tests/no_keys.rs` (Task 7) enforces the text part.
- **Catalogue rule (D2–D10).** Never bundled in the repo, a fixture or the installer: fixtures are **synthetic** JSON of a few entries. Downloaded only while `catalogs.switch.enabled` is on, never under CLI `--offline` or a paused network, never for a plan with no Switch item. No raw copy on disk: the body is reduced while streaming.
- **Links rule (D26).** Official site or repo only, HTTPS, no query strings; never a ROM, key or emulator site or repo (emulator entries carry name, use and licence but **no `url`**). The front end never opens a URL it built; links go through `open_credit(id, which)`.
- **Huge folders: headers only, within budget (batch 2 D24).** Per item ≤ 4 MiB read, ≤ 1 MiB decoded, ≤ 16 seeks; every offset and length read from a file is checked against the file length and the cap; every multiplication is checked. Archive folder-game detection reuses the listing inspection already made (no second open).
- **TDD.** Write the failing test, run it and watch it fail for the expected reason, implement, watch it pass. Each "Run" step names the exact command.
- **Gates (same as CI).** Before every commit that touches code or tests:
  - **G-RUST:** `cargo fmt --all --check && cargo clippy --workspace --all-targets --all-features -- -D warnings && scripts/dev/clippy-windows.sh && cargo test --workspace --no-fail-fast`
  - **G-WEB** (any task touching `src/`, `locales/`, `credits/`, `scripts/`, `design/`, `LICENSES/` or `src-tauri/`): `pnpm check-all && pnpm build`
  - Docs-only commits run `pnpm check-all`.
- **Windows CI leg.** Fixture mtimes pinned (`coverly_core::dedupe::testing::pin_mtime` or `filetime::set_file_mtime` to a fixed time, never "now"). Paths compared by component (`Path::components`, `file_name`, `Path::join`), never by slash text; snapshot paths go through the existing `/` redaction. Archive fixtures are written with both `/` and `\` separators where the spec says so. Far offsets go through the in-memory `SparseAt`, never `set_len` files. Big sizes (10 GB, 40 GB) are faked through `MetaProbe::file_len` and the `set_large_transfer_override` / `COVERLY_TEST_LARGE_TRANSFER` hooks, never written. Unix-only helpers go in `#[cfg(unix)] mod unix_tests`. The CLI prompt is tested through an injected `Prompt`, never a pty.
- **Real data is read-only.** Never write to `~/Documents/ROMS`, `/Volumes/TinyExt/*` (including `/Volumes/TinyExt/ROMs-Downloaded` and `/Volumes/TinyExt/Roms`) or `/Volumes/System`, never create `._*` there; only `ls`, `stat`, `head -c` into a copy and Coverly **plan-only** runs may touch them. Write tests use copies made with `cp -RXp` under `/Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-adjustments/target/adjust-sample/`; `find target/adjust-sample -name '._*' | wc -l` must print `0`; the folder is deleted afterwards (`rm -rf target/adjust-sample`). Real heads are never committed.
- **Fixtures are synthetic** bytes, XML and JSON built by builders in `consoles/testing.rs` (no copyrighted data, no keys, no catalogue copy). Real game names may appear as file names over synthetic content.
- **No UI strings in the core.** Reasons, notes and warnings are enums plus data; the UI and CLI render them. Every UI string lives in `locales/en/ui.json` **and** `locales/es/ui.json` with the same keys and placeholders, nested (a key is never both a string and an object; `settings.sections` stays a string, tab names live under `settings.section.*`). Copy is spec §10 word for word (Spanish: sentence case, no exclamation marks, exact counts, numbers, sizes and dates in mono through `useRichT`). Credit-entry strings live in `credits/credits.json` with the same rules.
- **Brand.** Only existing tokens from `design/tokens.json` (`pnpm tokens:check` green), `src/components/ui` primitives, `SystemIcon` for every system mark. Mono only for numbers (and the licence text inside the notices viewer). No Manila outside «you can still undo». No new colour, no new animation.
- **Secrets are never read or printed:** the minisign key, `~/.coverly-release/`, any `.env*`, `.vercel/`.
- **Servers and E2E.** The web UI runs on port **1468** only: `pnpm dev --port 1468 --strictPort` (mock backend). Stop it by port only: `lsof -ti tcp:1468 -sTCP:LISTEN | xargs -r kill`. E2E uses Chrome DevTools MCP `new_page` with `isolatedContext: "adjustments"`; fall back to Playwright MCP; if both profiles are locked, install `playwright-core` in `/private/tmp/claude-501/-Users-miguelangelesparzacalero-Documents-GitHub/35b7c020-f2ae-412f-b757-99c24a9a498b/scratchpad/adjustments` and drive Chrome for Testing from there.
- **Owner decisions never block.** Every spec §13 item is implemented with its default. Anything new needing the owner, or a «verify» step that fails (nlib response shape, Task 5; GameTDB link wording, Task 16; devDependency banners, Task 22), is implemented around, recorded as a `BLOCKED: <rule> — <why>` line in the task's commit body and collected into the PR (Task 24).

## Deviations from the spec (decided in this plan)

- **P-1: catalogue reaches identification through `OrganizeEnv`.** Readers in `consoles/` stay pure (no SQLite, no network). The catalogue is consulted after `identify_all`, in the evidence step: `OrganizeEnv` gains `titles: Option<&'a dyn TitleNames>` (`catalog::TitleNames { fn title(&self, tid: u64) -> Option<CatalogTitle>; fn by_nca(&self, nca: &[u8; 16]) -> Option<(u64, u8)>; }`), `None` = no catalogue (tests, `--offline`, setting off). The library scan uses the same trait. `SwitchCatalog` implements it with a read-only connection.
- **P-2: `ensure()` is called by the caller, never by identification.** `State` (app) and the CLI call `SwitchCatalog::ensure(&http, &settings, locale, cancel, deadline)` before a plan or scan only when the setting is on, the network is not paused/offline and the walk found at least one Switch candidate (the survey reports `has_switch` from the walk's extension pass). That keeps D10's 30 s cap outside the core's pure path.
- **P-3: one harness for the batch.** `crates/coverly-core/tests/adjustments.rs` (Task 1) is table-driven like `systems_batch3.rs`: `LOOSE` rows (§8.2), `TMD_KINDS` rows (D14), `ARCHIVE_GAMES` rows (§8.4), `DISC_SETS` rows (D24). Later tasks append rows.
- **P-4: `TitleSource` is its own enum.** Spec D7 names `ConsoleId.title_source` with `Header`/`Titledb`/`Nlib`/`FileName`; it is a new `TitleSource` enum (not `IdSource`, which says where the **id** came from). `IdSource` gains `NcaCatalog`, `Tmd`, `Npdm`, `Xbe`, `Xex`, `SelfHeader` (the spec's `Self_`, renamed because `Self_` is unidiomatic; wire `self-header`), `Pbp`.
- **P-5: README block written by the credits script, landing untouched.** `scripts/credits/readme.mjs` owns the README block; the landing copy is a post-merge task in `coverly-roms-landing` (spec Q15), listed in the PR.
- **P-6: generated notices as committed text.** `LICENSES/generated/rust.txt` and `js.txt` are committed outputs of `pnpm notices`; `pnpm notices:check` only compares the normalised lock hash (pure Node). The release-workflow edit (`notices:check` before `tauri build`) is made in the workflow file if it exists in the repo; if the release workflow lives outside this repo, it goes to the PR as `BLOCKED:`.

## Review Focus

1. **Keys (D1).** No source mentions a key file outside `is_key_material` and its tests (`tests/no_keys.rs`); catalogue tables have no `key`/`rights_id` column (schema test); an entry with a non-null `key` abandons the import; XCI/NSP readers never touch a `.tik`/`.nca` body or the gamecard info (`SparseAt` that panics on those ranges). Tasks 4, 6, 7.
2. **Catalogue hygiene (D2–D10).** Off by default; never downloaded under `--offline`, paused network or a plan without Switch; conditional GET with both `If-None-Match` and `If-Modified-Since`; at most every `refresh_days`; no raw file on disk; separate `catalog.sqlite`; names sanitised; `iconUrl` host-allowlisted; nlib only with catalogue on and fallback on, upper-case 16-hex id from the parsed `u64`. Tasks 2–6, 15, 17.
3. **No shadowing, no silent reclassification (D12–D14).** `.tmd`, `.rpx`, `.xbe`, `.xex`, `.nro`/`.nso`, `.3dsx` extended in their reader; `loose.rs` runs after `sony` and before `orbis`/`xbox`; 3DS `0004000E` is Update, `0004008C` DLC; parts are left `GamePart`, never moved alone; the class snapshot diff equals the spec. Tasks 8, 9.
4. **Every byte over the threshold confirmed once (D15–D18).** Huge items asked one by one, then the aggregate only for `rest`; `Choice::Decline` → `NotConfirmed`, followers follow; the gate runs on the decided plan; `check_space` counts extraction bytes on a same-volume move; CLI prompt on stderr only, non-TTY refusal byte-for-byte unchanged. Tasks 14, 15, 18.
5. **Duplicates act only on content evidence (D19).** `Title` acts only when every id came from content (never `FileName` alone, Q14); other versions still report; `FolderTree` within one system; undo restores. Task 13.
6. **Probe refactor is behaviour-neutral (D20).** Every existing probe test runs on `DiskFs` and `ArchiveFs`; `probe_split` never matches on archives; snapshots unchanged after Task 10. Tasks 10, 11.
7. **Credits coverage (D30).** Every registered endpoint prefix, `KNOWN_SOURCES` id, catalogue and DAT origin has a credit; required credits exact; no `https://` literal outside the registry. Tasks 2, 16.
8. **Windows.** Pinned mtimes, component path comparisons, `\` archive names, `IsTerminal` on Windows consoles, `scripts/dev/clippy-windows.sh` on every task.

---

## File structure

```
credits/credits.json, credits/credits.schema.json                        (new) D25, D26
about.toml, about.hbs                                                    (new) cargo-about config/template
LICENSES/generated/{rust.txt,js.txt}                                     (new, generated, committed)
crates/coverly-core/
├─ src/consoles/
│  ├─ mod.rs       TitleKind::Homebrew; PartKind; TitleSource; ConsoleId.{part,title_source,nca_ids,disc_count};
│  │                 IdSource::{NcaCatalog,Tmd,Npdm,Xbe,Xex,SelfHeader,Pbp}; ListingFs, DiskFs, EntryMeta;
│  │                 DirListing.{fs,name}; identify_one calls loose::identify
│  ├─ loose.rs     (new) NPDM, .nca name, DOL, SCE/SELF, PBP, ~PSP PRX, .psvimg
│  ├─ wiiu.rs      tmd_id full D14 table; .rpx Homebrew, .rpl part Library; probes on ListingFs
│  ├─ nx.rs        NRO NACP title/icon; .nso part ExeFs; XCI secure names; NSP PFS0 NCA names; probe_split on ListingFs
│  ├─ xbox.rs      loose .xbe/.xex kinds; disc n/m; probes on ListingFs
│  ├─ ctr.rs       .3dsx Homebrew; NoCrypto regression test
│  ├─ sony.rs      probes on ListingFs (l.name for the CUSA fallback, l.fs.art)
│  └─ testing.rs   builders: nro, nso, npdm, dol, xbe_loose, xex_loose, rpx, tmd(sig, tid, ver), psvimg path,
│                    self_header, pbp, prx, xci_with_secure, nsp_pfs0_names
├─ src/catalog/{mod.rs,titledb.rs,ncas.rs,nlib.rs,store.rs}              (new) D2–D10
├─ src/net/{mod.rs,hosts.rs}                                             if_modified_since, get_stream, ENDPOINTS
├─ src/credits.rs                                                        (new) include_str!, Credit, tests D30
├─ src/settings/{model.rs,validate.rs,patch.rs}                          CatalogSettings
├─ src/disc/{mod.rs,sega.rs}                                             DiscIdentity.count
├─ src/organize/
│  ├─ plan.rs      Choice::Decline; LeftReason::{GamePart,NotConfirmed}; ItemNote::{Homebrew,Vita7z,MissingDisc,EncryptedName};
│  │                 OrganizeEnv.titles; FACTS_VERSION 9
│  ├─ inspect.rs   ArchiveFs; archive_folder_game (vpk_console becomes a call to it)
│  ├─ discsets.rs  (new) D24
│  ├─ target.rs, evidence.rs   archive folder-game actions (D23); disc-set placement and m3u; catalogue names (D7)
│  └─ apply.rs     HugeItem, LargeTransfer.{huge,rest}, LargeConfirm, gate(), check_space counts extraction
├─ src/dedupe/{keys.rs,group.rs,units.rs}                                acts_unconfirmed, FolderTree, FACTS_VERSION 3
└─ tests/
   ├─ adjustments.rs (new)            harness (P-3)
   ├─ catalog_titledb.rs (new)        reducer, store, refresh, ncas, nlib
   ├─ no_keys.rs (new)                D1 source scan
   ├─ consoles_loose.rs (new)         §8.2 rows through walk/inspect
   ├─ organize_archive_games.rs (new) D20–D23
   ├─ organize_discsets.rs (new)      D24
   ├─ organize_huge_items.rs (new)    D15–D17
   └─ organize_folder_games.rs, organize_large_transfer.rs, dedupe_keys.rs, dedupe_group.rs, dedupe_undo.rs, net.rs,
      settings_file.rs, systems_classes.rs, sniff.rs   updated
crates/coverly-cli/src/{organize.rs,main.rs,catalog.rs (new)}            prompt, --skip-large, `catalog switch`
crates/coverly-cli/tests/{cli_organize.rs,cli_catalog.rs (new)}
src-tauri/src/{ipc.rs,state.rs,error.rs,prefs.rs,lib.rs}                 confirm, catalog_*, open_credit
src-tauri/tauri.conf.json                                                bundle.resources (D29)
src-tauri/tests/{organize_state.rs,contract.rs,catalog_state.rs (new),bundle_resources.rs (new)}
src/ipc/{types.ts,backend.ts,tauriBackend.ts,contract.sample.json,contract.test.ts}
src/ipc/mock/{mockOrganize.ts,mockSettings.ts,mockBackend.ts,fixture.ts,adjustFixture.ts (new)} (+ tests)
src/lib/{credits.ts (new),notices.ts (new),organize.ts} (+ tests)
src/stores/{organizeStore.ts,navStore.ts} (+ tests)
src/components/organize/{HugeItemDialog.tsx (new),LargeTransferDialog.tsx,ItemName.tsx,SystemGroups.tsx,ReviewBuckets.tsx}
src/components/settings/{CreditsSection.tsx (new),NoticesDialog.tsx (new),SourcesSection.tsx,AboutSection.tsx,registry.ts,search.ts}
src/screens/{SettingsScreen.tsx,OrganizeScreen.tsx}
locales/{es,en}/ui.json
scripts/licenses/notices.mjs (new), scripts/credits/readme.mjs (new), package.json
THIRD-PARTY-NOTICES.md, docs/09-legal/license.md, README.md, README.es.md, CHANGELOG.md
```

---

## Task 1: Core types, facts v9 and the batch harness (Rust, CLI text, TS types, contract)

No behaviour change: types, serde, text and contract only.

**Files:**
- Modify: `crates/coverly-core/src/consoles/mod.rs`, `crates/coverly-core/src/organize/plan.rs` (`Choice::Decline`, `LeftReason::{GamePart, NotConfirmed}`, `ItemNote::{Homebrew, Vita7z, MissingDisc, EncryptedName}`, `FACTS_VERSION = 9`), `crates/coverly-core/src/disc/mod.rs` (`DiscIdentity.count: Option<u8>`, serde default), `crates/coverly-cli/src/organize.rs` (`left_text`, note text, exhaustive matches), `src-tauri/src/ipc.rs` (exhaustive matches, if any), `src/ipc/types.ts`, `src/ipc/contract.sample.json`, `src/ipc/contract.test.ts`, `src-tauri/tests/contract.rs`, `src/lib/organize.ts` (exhaustive switch placeholders returning existing generic copy until Task 20)
- Create: `crates/coverly-core/tests/adjustments.rs` (P-3 harness, empty tables plus one smoke row per table)
- Test: unit tests in `consoles/mod.rs` and `organize/plan.rs`; `crates/coverly-cli` text tests; `contract.test.ts`

**Interfaces (produced):**

```rust
// consoles/mod.rs
pub enum TitleKind { Base, Update, Dlc, Demo, /* new */ Homebrew, #[default] Unknown }

/// A piece that cannot play alone (spec D12/D13).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PartKind { ExeFs, Nca, Library, TitleMeta, Module, Backup }

/// Where the shown title came from (spec D7, plan P-4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TitleSource { Header, Titledb, Nlib, FileName }

pub enum IdSource { /* existing */ NcaCatalog, Tmd, Npdm, Xbe, Xex, SelfHeader, Pbp }

// appended to ConsoleId (all #[serde(default, skip_serializing_if = …)])
pub part: Option<PartKind>,
pub title_source: Option<TitleSource>,
/// Lower-case 32-hex NCA ids seen by name (loose, PFS0 table, XCI secure HFS0).
pub nca_ids: Vec<String>,
/// The `m` of a disc `n/m` (Saturn, Dreamcast, XEX), when the header says.
pub disc_count: Option<u8>,
```

```rust
// organize/plan.rs
pub enum Choice { /* existing */ Decline }          // wire "decline"; never remembered by content
pub enum LeftReason { /* existing */
    GamePart { part: PartKind, system: String, title_id: Option<String> },
    NotConfirmed,
}
pub enum ItemNote { /* existing */ Homebrew, Vita7z, MissingDisc { n: u8 }, EncryptedName }
const FACTS_VERSION: u32 = 9;
```

- [ ] **Step 1: Failing tests.** Serde round trips: `TitleKind::Homebrew` ↔ `"homebrew"`, every `PartKind`, `TitleSource`, the new `IdSource` values (`"self-header"`), `Choice::Decline` ↔ `"decline"`, `LeftReason::GamePart` JSON `{"kind":"game-part","part":"exe-fs","system":"switch","title-id":null}` (match the existing `LeftReason` tag/rename convention, check with `grep -n "serde(tag" crates/coverly-core/src/organize/plan.rs`), `NotConfirmed`, the four notes. A plan JSON saved by 0.1.x (existing fixture under `crates/coverly-core/tests/fixtures/`, `grep -rln '"facts"' crates/coverly-core/tests/fixtures | head`) still loads. Memoised facts with `v: 8` are discarded. CLI: `left_text(GamePart{…})` = `part of a game; not moved on its own`, `NotConfirmed` = `not confirmed; left where it is`. `contract.test.ts`: the sample holds one of each new value.
- [ ] **Step 2: Run** `cargo test -p coverly-core consoles:: organize::plan && cargo test -p coverly-core --test adjustments && cargo test -p coverly-cli && pnpm vitest run src/ipc/contract.test.ts` — expect failures.
- [ ] **Step 3: Implement.** Every new field `#[serde(default)]`; `ConsoleId::with` fills the new fields with `None`/empty. `Choice::Decline` in the decision loop behaves exactly like `Leave` except the item gets `LeftReason::NotConfirmed` and the decision is never stored in the remembered-choice map (`grep -n "remember" crates/coverly-core/src/organize/plan.rs`).
- [ ] **Step 4: Run** Step 2, G-RUST, G-WEB.
- [ ] **Step 5: Commit and push:** `git add -A crates src-tauri src && git commit -m "feat(core): types for homebrew, game parts, title sources and declined items" && git push && git status -sb`

---

## Task 2: Network: `If-Modified-Since`, streaming bodies and the endpoint registry

**Files:**
- Modify: `crates/coverly-core/src/net/mod.rs` (`HttpRequest.if_modified_since: Option<String>`, `Http::get_stream`, `UreqHttp` prefix guard and live-body stream with a decoded-byte cap), every provider that holds a URL literal (`grep -rn '"https://' crates/coverly-core/src src-tauri/src | grep -v '#\[cfg(test)\]'`: libretro thumbnails/database, SteamGridDB, ScreenScraper, updater)
- Create: `crates/coverly-core/src/net/hosts.rs`
- Test: `crates/coverly-core/tests/net.rs` (loopback server in `net/testing.rs`)

```rust
// net/hosts.rs
/// One allowed URL prefix and the credit entry that covers it (spec D30).
pub struct Endpoint { pub prefix: &'static str, pub credit: Option<&'static str> }

pub const ENDPOINTS: &[Endpoint] = &[
    Endpoint { prefix: "https://thumbnails.libretro.com/", credit: Some("libretro-thumbnails") },
    Endpoint { prefix: "https://raw.githubusercontent.com/libretro/libretro-database/", credit: Some("libretro-database") },
    Endpoint { prefix: "https://raw.githubusercontent.com/blawar/titledb/", credit: Some("titledb") },
    Endpoint { prefix: "https://www.steamgriddb.com/", credit: Some("steamgriddb") },
    Endpoint { prefix: "https://cdn2.steamgriddb.com/", credit: Some("steamgriddb") },
    Endpoint { prefix: "https://api.screenscraper.fr/", credit: Some("screenscraper") },
    Endpoint { prefix: "https://img-eshop.cdn.nintendo.net/", credit: Some("nintendo-eshop-icons") },
    Endpoint { prefix: "https://api.nlib.cc/", credit: Some("nlib") },
    Endpoint { prefix: "https://github.com/MAECLY/", credit: None },
];

/// Whether `url` may be fetched: a registered prefix, or one of the user's
/// own HTTPS URLs (nlib fallback, proxy) already validated by settings.
pub fn allowed(url: &str, user: &[&str]) -> bool { /* prefix match, user list exact-prefix */ }
```

```rust
// net/mod.rs
pub trait Http: Send + Sync {
    fn get(&self, req: &HttpRequest) -> Result<HttpResponse, NetError>;
    /// Streams the body to `sink`; the default wraps `get()`'s body in a
    /// cursor, so `PoliteHttp` and the fakes keep working (spec D5).
    fn get_stream(
        &self,
        req: &HttpRequest,
        sink: &mut dyn FnMut(&HttpResponse, &mut dyn std::io::Read) -> Result<(), NetError>,
    ) -> Result<HttpResponse, NetError> {
        let mut resp = self.get(req)?;
        let body = std::mem::take(&mut resp.body);
        sink(&resp, &mut std::io::Cursor::new(body))?;
        Ok(resp)
    }
}
```

The `HttpResponse` passed to `sink` has an empty `body`; status 304/404 calls `sink` with an empty reader only for 2xx (non-2xx returns without calling it). `UreqHttp::get_stream` sends `If-None-Match` and `If-Modified-Since`, reads the live body through `take(req.max_bytes + 1)` and returns `NetError::TooLarge` past the cap (check the existing variant name with `grep -n "TooLarge\|too large" crates/coverly-core/src/net/mod.rs`). `UreqHttp::get`/`get_stream` refuse any URL `hosts::allowed` rejects with a `NetError` (and `debug_assert!`). `UreqHttp` takes the user URL list at construction (`UreqHttp::with_user_urls(Vec<String>)`); `src-tauri` and the CLI pass the validated nlib fallback URL (Task 3 adds the setting; until then an empty list).

- [ ] **Step 1: Failing tests** (`tests/net.rs`): `If-Modified-Since` sent from `if_modified_since` and `Last-Modified` read back; 304 round trip; `get_stream` delivers a 3 MiB body in chunks (sink counts reads > 1) and stops at the cap with `TooLarge`; the default `get_stream` works over a fake `Http`; an unregistered prefix (`https://example.com/`) → error before any connection (loopback server sees no request); `https://raw.githubusercontent.com/other/repo/x` refused although the host is registered; a user URL passes. The existing provider tests keep passing with URLs read from `hosts.rs` constants. (The loopback server is `http://127.0.0.1`: add a `#[cfg(any(test, feature = "test-util"))]` constructor `UreqHttp::for_tests_allow_loopback()` rather than registering loopback.)
- [ ] **Step 2: Run** `cargo test -p coverly-core --test net` — expect failures.
- [ ] **Step 3: Implement.** Move every provider URL literal to a `pub const` in `hosts.rs` built from `ENDPOINTS` prefixes (e.g. `pub const LIBRETRO_THUMBS: &str = ENDPOINTS[0].prefix;` is fragile — use named `const` strings and have `ENDPOINTS` reference them).
- [ ] **Step 4: Run** Step 2, G-RUST (and G-WEB because `src-tauri` changes).
- [ ] **Step 5: Commit and push:** `git add -A crates src-tauri && git commit -m "feat(core): conditional GET with Last-Modified, streaming bodies and an endpoint registry" && git push && git status -sb`

---

## Task 3: Catalogue settings (`catalogs.switch`)

**Files:**
- Modify: `crates/coverly-core/src/settings/model.rs` (`CatalogSettings`, `SwitchCatalogSettings`, top-level `catalogs` with `#[serde(default)]`; `SETTINGS_VERSION` stays 3), `validate.rs` (`refresh_days` 7–90, `fallback_url` HTTPS-only like the proxy URL, `icons`/`fallback` require `enabled`), `patch.rs` (patch paths `catalogs.switch.*`), `src/ipc/types.ts` (`Settings.catalogs`), `src/ipc/mock/mockSettings.ts` (defaults), `src/ipc/contract.sample.json`
- Test: `crates/coverly-core/tests/settings_file.rs`, `crates/coverly-core/tests/settings_apply.rs`, `src/ipc/mock/mockSettings.test.ts`

```rust
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct SwitchCatalogSettings {
    pub enabled: bool,          // false
    pub icons: bool,            // false
    pub refresh_days: u16,      // 7
    pub fallback: bool,         // false
    pub fallback_url: String,   // "https://api.nlib.cc"
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CatalogSettings { pub switch: SwitchCatalogSettings }
```

- [ ] **Step 1: Failing tests.** A settings file without `catalogs` loads with the defaults; a file with an unknown `catalogs.other` group loads; `refresh_days: 3` and `200` rejected; `fallback_url: "http://x"` rejected with the proxy's message key; `icons: true` with `enabled: false` is normalised to `false` on save (or rejected — follow how `validate.rs` treats dependent switches, `grep -n "requires\|depends" crates/coverly-core/src/settings/validate.rs`); patch `catalogs.switch.enabled=true` round trip; a rollback build (deserialize into a struct without `catalogs`, simulated by `serde_json::from_str::<serde_json::Value>` + the old field set) ignores it.
- [ ] **Step 2: Run** `cargo test -p coverly-core --test settings_file --test settings_apply && pnpm vitest run src/ipc/mock/mockSettings.test.ts` — expect failures.
- [ ] **Step 3: Implement.** CLI `coverly settings` lists the new keys through the existing generic printer (check `crates/coverly-cli/src/settings.rs`).
- [ ] **Step 4: Run** Step 2, G-RUST, G-WEB.
- [ ] **Step 5: Commit and push:** `git add -A crates src && git commit -m "feat(core): settings for the optional Switch name catalogue" && git push && git status -sb`

---

## Task 4: Switch catalogue store and the titledb / ncas reducers

**Files:**
- Create: `crates/coverly-core/src/catalog/mod.rs` (`TitleNames`, `CatalogTitle`, `SwitchCatalog`, `CatalogStatus`), `catalog/store.rs` (`catalog.sqlite`, own `user_version` 1, `catalog_meta`, `titledb`, `ncas`, `nlib`), `catalog/titledb.rs` (streaming reducer, D5), `catalog/ncas.rs` (streaming reducer, D6), `crates/coverly-core/tests/catalog_titledb.rs`, `crates/coverly-core/tests/fixtures/catalog/{titledb-small.json,ncas-small.json}` (synthetic, ≤ 8 entries)
- Modify: `crates/coverly-core/src/lib.rs` (`pub mod catalog;`), `crates/coverly-core/src/organize/plan.rs` (`OrganizeEnv.titles: Option<&'a dyn TitleNames>`, every constructor passes `None`)

```rust
// catalog/mod.rs
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogTitle { pub name: String, pub publisher: Option<String>, pub release: Option<String>, pub region: Option<String>, pub source: TitleSource }

pub trait TitleNames: Send + Sync {
    fn title(&self, tid: u64) -> Option<CatalogTitle>;
    /// Title id and content type for an NCA id (spec D6).
    fn by_nca(&self, nca: &[u8; 16]) -> Option<(u64, u8)>;
    fn icon_url(&self, tid: u64) -> Option<String> { let _ = tid; None }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum CatalogStatus {
    Off,
    Pending,
    Downloading { done: u64, total: Option<u64> },
    Ready { count: u64, updated: String, fallback: bool },
    FailedOld { updated: String },
    FailedNone,
    Rejected,
    RemoveFailed,
}

pub struct SwitchCatalog { /* path, read-only conn (r2d2-free: one Mutex<Connection>) */ }
impl SwitchCatalog {
    pub fn open(data_dir: &Path) -> Result<SwitchCatalog, CatalogError>;
    /// Download or refresh when due (D2–D4, D10). `deadline` caps a first
    /// download inside a plan (30 s); past it returns `Pending`.
    pub fn ensure(&self, http: &dyn Http, s: &SwitchCatalogSettings, locale: &str, now: SystemTime,
                  cancel: &AtomicBool, deadline: Option<Instant>, progress: &dyn Fn(u64, Option<u64>)) -> CatalogStatus;
    pub fn status(&self, s: &SwitchCatalogSettings) -> CatalogStatus;
    /// Deletes `catalog.sqlite` and the cached icons (the handle is dropped first).
    pub fn remove(self, data_dir: &Path) -> Result<(), CatalogError>;
}
impl TitleNames for SwitchCatalog { /* indexed SELECTs */ }
```

Reducer (`titledb.rs`): `serde_json::Deserializer::from_reader(BufReader::new(r))` + a `DeserializeSeed` visitor over the top-level map. Per value, a `MapAccess` that reads only `id`, `name`, `publisher`, `releaseDate`, `region`, `iconUrl` (when `keep_icons`) and `key` (only to test `is_null`; the value is visited as `Option<IgnoredAny>`-like: deserialize into an enum `Nullness { Null, NotNull }` that never stores the content) and skips every other field as `IgnoredAny`. Rows go into `titledb_staging` in one transaction; on success `DROP TABLE titledb; ALTER TABLE titledb_staging RENAME TO titledb` inside the same transaction; on any error or a non-null `key` the transaction rolls back. Names: strip `char::is_control` and the bidi overrides U+202A–U+202E, U+2066–U+2069, NFC, cap 256 chars; skip empty. Id: exactly 16 hex → `u64`. `releaseDate` int `YYYYMMDD` → `YYYY-MM-DD` (invalid → `None`). `iconUrl` kept only when it starts with `https://img-eshop.cdn.nintendo.net/`. Errors carry only the byte offset and field name (D1). `ncas.rs`: top-level map keyed by 32-hex NCA id → `{ titleId, contentType }` (verify the field names against `docs/02-research/switch-titledb.md`; record any difference in a code comment); same staging/swap.

`ensure`: file `ES.es.json` for locale `es`, else `US.en.json`; on 404/parse failure of `ES.es.json` retry `US.en.json` (`fallback: true`); meta row stores `etag`, `last_modified`, `checked_at`, `rows`, `source_file`; not due (`now - checked_at < refresh_days`) → no request; 304 → only `checked_at`; locale change → next due refresh fetches the other file unconditionally; icons turned on since last fetch (meta `icons` flag differs) → one unconditional fetch. `ncas.json` is fetched only by `ensure_ncas()` (called by P-2's caller when the walk saw NCA ids without a title id).

- [ ] **Step 1: Failing tests** (`catalog_titledb.rs`, fake `Http` serving fixtures): 6-entry fixture (valid, no id, bad id, duplicate id, empty name, one with `rightsId`/`description`/`key: null`) → 3 rows; schema has no `key`/`rights_id` column (`PRAGMA table_info`); `releaseDate` → ISO; icon URL kept only with icons on and the allowed host; a non-null `key` → `Rejected`, old rows intact; HTML body → `FailedOld`/`FailedNone`, old rows intact; body over the cap → failed, no partial rows; 404 on `ES.es.json` → `US.en.json`, `fallback: true`; second `ensure` within 7 days sends no request; after 7 days sends `If-None-Match` and `If-Modified-Since`, 304 keeps rows; bidi/control stripped; the shared cache's `user_version` is still 3 after an import; `remove` deletes the file (on all OSes, with the read handle dropped first); `ncas` reducer and `by_nca`; nothing written under the data dir except `catalog.sqlite` (+ `-wal`/`-shm`).
- [ ] **Step 2: Run** `cargo test -p coverly-core --test catalog_titledb` — expect failures.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** Step 2, G-RUST.
- [ ] **Step 5: Commit and push:** `git add -A crates && git commit -m "feat(core): optional Switch name catalogue from blawar/titledb" && git push && git status -sb`

---

## Task 5: nlib fallback and opt-in eShop icons

**Files:**
- Create: `crates/coverly-core/src/catalog/nlib.rs`, `crates/coverly-core/tests/fixtures/catalog/nlib-0100000000010000.json` (shape captured live, values replaced by synthetic ones), `crates/coverly-core/src/catalog/icons.rs`
- Modify: `catalog/mod.rs` (`SwitchCatalog::title` consults nlib when `fallback` is on; `icon(tid)`), `catalog/store.rs` (`nlib` table, 30-day hit / 7-day miss TTL)
- Test: `crates/coverly-core/tests/catalog_titledb.rs`

- [ ] **Step 0: Verify the response shape (spec D9).** `curl -s 'https://api.nlib.cc/nx/0100000000010000?lang=en' | head -c 2000` (a public, non-game system title id; never a key, never log the body into the repo). Record the field names. If the API is unreachable or the shape differs from `name`/`publisher`/`releaseDate`, write the parser for what is observed, keep every field optional, and add `BLOCKED: nlib response shape — <what was seen>` to the commit body.
- [ ] **Step 1: Failing tests.** `GET {url}/nx/{TID}?lang=en|es` with `TID` re-formatted from the `u64` as 16 upper-case hex; `max_bytes` 64 KiB; unknown fields ignored; 404 → miss cached 7 days; hit cached 30 days; fallback off → no request; catalogue off → no request even with fallback on; a user URL `https://mirror.example/` passes the registry (Task 2 user list), `http://` never reaches here (validation). Icons: only `https://img-eshop.cdn.nintendo.net/` (redirect only to the same host), ≤ 1 MiB, JPEG/PNG decoded with the image module's caps and re-encoded to a 256 px PNG in the art cache (`grep -n "pub fn decode\|max_pixels" crates/coverly-core/src/image/*.rs`); turning icons off deletes cached icons and clears `icon_url`.
- [ ] **Step 2: Run** `cargo test -p coverly-core --test catalog_titledb` — expect failures.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** Step 2, G-RUST.
- [ ] **Step 5: Commit and push:** `git add -A crates && git commit -m "feat(core): optional nlib fallback and on-demand Switch icons" && git push && git status -sb`

---

## Task 6: Switch readers: NCA ids, NRO NACP, NSO part; catalogue names in identification

**Files:**
- Modify: `crates/coverly-core/src/consoles/nx.rs` (XCI secure-partition HFS0 header + names table only, at the root HFS0's offset; NSP/NSZ PFS0 `<32 hex>.nca|.ncz` names when no ticket/cnmt; loose `<32 hex>.nca|.ncz` by name → `part: Nca`; `.nro` `ASET` NACP title (first non-empty language) + `DisplayVersion` + icon locator `ArtLocator::Range { format: Jpeg? }` — add `ArtFormat::Jpeg` if absent; `.nso` → `part: ExeFs`; `main.npdm` is Task 9), `crates/coverly-core/src/consoles/testing.rs` (builders `nro(title, version, icon)`, `nso()`, `xci_with_secure(names)`, `nsp_pfs0(names)`), `crates/coverly-core/src/organize/evidence.rs` (D7 name priority with `env.titles`; `IdSource::NcaCatalog`; `title_source`; update/DLC names «Actualización de {name}»/«DLC de {name}» through the existing `ItemNote::Update/Dlc { of }`), `crates/coverly-core/src/organize/plan.rs` (`Survey.has_switch`, `Survey.wants_ncas` for P-2)
- Test: `crates/coverly-core/tests/adjustments.rs`, `crates/coverly-core/tests/organize_evidence.rs`

- [ ] **Step 1: Failing tests.** XCI with secure names → `nca_ids` sorted lower-case; NSP without ticket/cnmt → PFS0 NCA names; `SparseAt` that panics on any read inside the XCI 0x190 gamecard-info range or any NCA body range; loose `0123…ef.nca` → `part: Nca`, no title; NRO with NACP → `kind: Homebrew`, title, version; `.nso` → `part: ExeFs`. Evidence with a fake `TitleNames`: base by id → name, `title_source: Titledb`; update id (`0x800` bit) resolves via base id `& 0xFFFF_FFFF_FFFF_E000`; NCA-only XCI → title id from `by_nca`, `IdSource::NcaCatalog`; no catalogue → file-name title, `title_source: FileName`; `cnmt.xml` title wins over the catalogue (`Header`).
- [ ] **Step 2: Run** `cargo test -p coverly-core --test adjustments --test organize_evidence nx` — expect failures.
- [ ] **Step 3: Implement.** NCA names count only when exactly 32 hex before the extension. Catalogue names reaching a file name go through the existing target-name sanitiser (`grep -n "fn sanitize\|fn safe_name" crates/coverly-core/src/organize/target.rs`).
- [ ] **Step 4: Run** Step 2, G-RUST.
- [ ] **Step 5: Commit and push:** `git add -A crates && git commit -m "feat(core): Switch NCA ids from XCI and NSP names, NRO titles and catalogue names" && git push && git status -sb`

---

## Task 7: «Cifrado» label and the key guard test

**Files:**
- Modify: `crates/coverly-core/src/organize/evidence.rs` (`ItemNote::EncryptedName` when system ∈ {`switch`, `3ds`}, title unresolved after D7, and `encrypted == Some(true)` or a Switch NSP/XCI/NCA; never together with `EncryptedContent` on one row), `crates/coverly-core/src/consoles/ctr.rs` (`.3dsx` → `kind: Homebrew`; regression test), `crates/coverly-cli/src/organize.rs` (text `encrypted; the name cannot be read without decrypting it`)
- Create: `crates/coverly-core/tests/no_keys.rs`

`no_keys.rs`: walks `crates/*/src`, `src-tauri/src`, `src/` (by path components, skipping `target`, `node_modules`, generated icon folders), strips from a `mod tests` / `#[cfg(test)]` line to end of file for `.rs`, strips `*.test.ts(x)` files; fails naming the file when any of `prod.keys`, `title.keys`, `keys.txt`, `aes_keys.txt`, `seeddb.bin`, `boot9` appears outside the `is_key_material` name list in `consoles/mod.rs` (locate the list by its `const` name, `grep -n "KEY_EXTS\|KEY_NAMES" crates/coverly-core/src/consoles/mod.rs`); fails when a struct in `crates/coverly-core/src/catalog/` has a field named `key`, `rights_id`, `rightsId`, `title_key`, `titleKey`.

- [ ] **Step 1: Failing tests.** Evidence: unresolved NSP → `EncryptedName`; resolved encrypted 3DS keeps only `EncryptedContent`; an encrypted NCCH with an SMDH-looking ExeFS is not read (open/range log); `.3dsx` Homebrew. `no_keys.rs` passes on the tree and fails on a temp copy with an injected `prod.keys` string (test the scanner function on a temp dir).
- [ ] **Step 2: Run** `cargo test -p coverly-core --test no_keys --test organize_evidence` — expect failures.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** Step 2, G-RUST.
- [ ] **Step 5: Commit and push (two commits):** `git add crates/coverly-core/tests/no_keys.rs && git commit -m "test(core): guard against key handling" && git push && git add -A crates && git commit -m "feat(core): label unresolved Switch and 3DS titles as encrypted" && git push && git status -sb`

---

## Task 8: Header classification in the existing readers (`.tmd`, `.rpx`/`.rpl`, `.xbe`, `.xex`)

**Files:**
- Modify: `crates/coverly-core/src/consoles/wiiu.rs` (`tmd_id` learns the full D14 table for 3DS, Wii, DSi and Wii U; loose `.tmd` → `part: TitleMeta`, system by high value, `IdSource::Tmd`; `.rpx` loose → Homebrew, `.rpl` → `part: Library`, ELF OSABI `0xCA`, ABIVer `0xFE`), `crates/coverly-core/src/consoles/xbox.rs` (loose `.xbe`: Homebrew when TitleId 0 or AllowedMedia only `HARD_DISK`/`NONSECURE_HARD_DISK`; `DiscNumber`; loose `.xex`: `PATCH_*` → Update, `DLL`/`MODULE` → `part: Module`, `TITLE` → Base; `disc_no`/`disc_count`), `crates/coverly-core/src/consoles/testing.rs` (builders `tmd(sig_type, tid, version)`, `rpx(abi)`, `xbe_loose(tid, media, disc)`, `xex_loose(flags, tid, disc, count)`)
- Test: `crates/coverly-core/tests/adjustments.rs` (`TMD_KINDS`, `LOOSE` rows), `crates/coverly-core/tests/systems_classes.rs` (class snapshot)

```rust
// wiiu.rs — exact table (spec D14)
fn tmd_kind(tid: u64) -> Option<(&'static str, TmdKind)> {
    let hi = (tid >> 32) as u32;
    Some(match hi {
        0x0004_0000 | 0x0004_0001 => ("3ds", TmdKind::Title(TitleKind::Base)),
        0x0004_0002 => ("3ds", TmdKind::Title(TitleKind::Demo)),
        0x0004_000E => ("3ds", TmdKind::Title(TitleKind::Update)),
        0x0004_008C => ("3ds", TmdKind::Title(TitleKind::Dlc)),
        0x0004_0010 | 0x0004_001B | 0x0004_0030 | 0x0004_009B | 0x0004_00DB | 0x0004_0130 | 0x0004_0138 => ("3ds", TmdKind::System),
        0x0001_0000 | 0x0001_0001 | 0x0001_0004 => ("wii", TmdKind::Title(TitleKind::Base)),
        0x0001_0005 => ("wii", TmdKind::Title(TitleKind::Dlc)),
        0x0001_0002 | 0x0001_0008 | 0x0000_0001 => ("wii", TmdKind::System),
        0x0005_0000 => ("wiiu", TmdKind::Title(TitleKind::Base)),
        0x0005_0002 => ("wiiu", TmdKind::Title(TitleKind::Demo)),
        0x0005_000E => ("wiiu", TmdKind::Title(TitleKind::Update)),
        0x0005_000C => ("wiiu", TmdKind::Title(TitleKind::Dlc)),
        0x0005_0010 | 0x0005_001B | 0x0005_004E | 0x0007_0002 | 0x0007_0008 | 0x0000_0007 => ("wiiu", TmdKind::System),
        0x0003_0015 | 0x0003_0017 => ("nds", TmdKind::System),
        h if h >> 16 == 0x0003 => ("nds", TmdKind::Title(TitleKind::Base)),
        _ => return None, // Unknown high value: no guess
    })
}
```

`TmdKind::System` → `NotARom::ConsoleSystem`. The TMD header offset comes from the signature-type table (RSA-4096 `0x10000`, RSA-2048 `0x10001`, ECDSA `0x10002`, + the HMAC variants), title id at header + 0x4C, version at + 0x9C (for RSA-2048 that is the spec's 0x18C / 0x1DC); unknown signature type → not identified. The NUS folder probe keeps owning `title.tmd` inside a folder.

- [ ] **Step 1: Failing tests.** Every `TMD_KINDS` row (including `0004000E` → Update and `0004008C` → DLC, unknown high → `None`); ECDSA TMD offset; `.rpx` loose Homebrew, `.rpl` part Library, `code/*.rpx` beside `meta/` still the loadiine probe's; `.xbe` homebrew both rules, retail with siblings → review; `.xex` patch/module/title; XEX disc 2 of 2. Class snapshot diff equals spec §8.2 for these rows only.
- [ ] **Step 2: Run** `cargo test -p coverly-core --test adjustments --test systems_classes` — expect failures.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** Step 2, G-RUST; accept the snapshot only when the diff is exactly the expected rows (`cargo insta review` is not available non-interactively: inspect `*.snap.new` and move it).
- [ ] **Step 5: Commit and push:** `git add -A crates && git commit -m "feat(core): classify TMD, RPX, XBE and XEX files by their full header" && git push && git status -sb`

---

## Task 9: `consoles/loose.rs` for the formats no reader owns; game parts are left

**Files:**
- Create: `crates/coverly-core/src/consoles/loose.rs`, `crates/coverly-core/tests/consoles_loose.rs`
- Modify: `crates/coverly-core/src/consoles/mod.rs` (`identify_one`: `… .or_else(|| sony::identify(src, ext)).or_else(|| loose::identify(src, ext, name)).or_else(|| orbis::identify(…)) …`), `crates/coverly-core/src/consoles/testing.rs` (builders `npdm(program_id)`, `dol(sections, wii_word)`, `self_header(version, self_type, authid)`, `pbp(sfo_category, disc_id, psar_magic)`, `prx(modname)`), `crates/coverly-core/src/organize/plan.rs` and `evidence.rs` (any `part` → `LeftReason::GamePart`; Homebrew → system folder with `ItemNote::Homebrew`; `.psvimg`/`.psvmd` → `GamePart { Backup }` with the title id from a path segment `^[A-Z]{4}\d{5}$`; `boot.dol` under `apps/<name>/` → the folder is the item; PBP of a PS1 classic → `psx`)

Each reader per spec §8.2: NPDM `META`@0, ACI0 at AciOffset, ProgramId @+0x10 → `switch`, `part: ExeFs`, `IdSource::Npdm`; DOL by structure (each non-empty section `offset + align_up(size, 32) ≤ len`, addresses and entry in `0x8000_0000..0x8180_0000`, ≥ 1 text section; Wii when a text word matches `(w & 0xFC1F_FFFF) == 0x7C13_FBA6` within the 1 MiB decoded budget, else `gamecube`), Homebrew; SCE/SELF `SCE\0`@0, header_type 1, version `00 00 00 02` → `ps3`, `03 00 00 00` → `psvita`, Vita authid `0x2F00000000000001/2/3` → Homebrew, `eboot.bin` → `part: ExeFs`, lone `.self` APP → Homebrew, unknown version → `None`; PBP `\0PBP` + PARAM.SFO `CATEGORY` + PSAR @0x24 `PSISOIMG0000`/`NPUMDIMG` (PS1 → `psx` Base; PSN ISO → `psp` Base; PSP Base; Homebrew when `CATEGORY == "MG"` and DISC_ID not `[A-Z]{4}\d{5}`; SFO beyond budget → `psp`, Unknown, review); `~PSP`@0 `.prx` → `psp`, `part: Module`; `.psvimg`/`.psvmd` by name (never opened).

- [ ] **Step 1: Failing tests** (`consoles_loose.rs` through `walk`/`inspect` and `adjustments.rs` `LOOSE` rows): one positive and one near-miss per §8.2 row; `main.npdm` in a loose folder left `GamePart { ExeFs }` with title id; a `.dol` data file with addresses outside RAM → not a DOL; `apps/foo/boot.dol` → one folder item, Homebrew note; PBP PS1 vs PSP vs PSN vs homebrew; `.psvimg` left with the title id from `…/PCSE00123/…`; budgets on the largest fixture (4 MiB / 1 MiB / 16 seeks).
- [ ] **Step 2: Run** `cargo test -p coverly-core --test consoles_loose --test adjustments --test systems_classes` — expect failures.
- [ ] **Step 3: Implement.** CLI text for `ItemNote::Homebrew` (`homebrew`).
- [ ] **Step 4: Run** Step 2, G-RUST.
- [ ] **Step 5: Commit and push:** `git add -A crates && git commit -m "feat(core): classify loose executables and title metadata by header" && git push && git status -sb`

---

## Task 10: Folder probes over a listing abstraction (`ListingFs`, `DiskFs`)

Behaviour-neutral refactor. Every snapshot stays byte-identical.

**Files:**
- Modify: `crates/coverly-core/src/consoles/mod.rs` (`ListingFs`, `EntryMeta`, `DiskFs`; `DirListing { fs, dir, name, names }`), `sony.rs`, `wiiu.rs`, `xbox.rs`, `nx.rs` (every probe), every `DirListing { … }` constructor (`grep -rn "DirListing {" crates src-tauri`), test-only `EXTRA_PROBES` users
- Test: existing `organize_folder_games.rs`, `consoles_current_gen.rs`, `organize_walk.rs`, snapshots

```rust
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EntryMeta { pub dir: bool, pub file: bool, pub link: bool, pub len: u64, pub fat_archive_bit: Option<bool> }

pub trait ListingFs {
    /// Names of the children of `rel` (relative to the listing root), or `None`.
    fn list(&self, rel: &Path) -> Option<Vec<String>>;
    fn meta(&self, rel: &Path) -> Option<EntryMeta>;
    /// At most `cap` bytes of `rel` through `budget`.
    fn read(&self, rel: &Path, cap: u64, budget: &mut Budget) -> Option<Vec<u8>>;
    fn art(&self, rel: &Path, format: ArtFormat) -> ArtLocator;
}

pub struct DiskFs<'a> { pub root: &'a Path }   // wraps symlink_metadata / list_child / read_head unchanged

pub struct DirListing<'a> {
    pub fs: &'a dyn ListingFs,
    /// The listing root inside `fs` (`Path::new("")` for DiskFs).
    pub dir: &'a Path,
    /// The folder's own name (disk name, archive root folder name, or archive stem).
    pub name: &'a str,
    pub names: &'a [String],
}
```

Probes read `l.name` instead of `l.dir.file_name()`, build art through `l.fs.art(rel, fmt)`, read through `l.fs.read`, and `nx::probe_split` requires `meta.fat_archive_bit == Some(true)`. Check the existing budget type name (`grep -n "pub struct Budget\b\|struct Budgeted" crates/coverly-core/src/consoles/mod.rs crates/coverly-core/src/archive/*.rs`) and use it.

- [ ] **Step 1: Failing test.** A test-only `RecordingFs` that wraps `DiskFs` and records calls: every probe on the existing fixture folders goes only through the trait (no direct `std::fs` call from probe code — check with `grep -n "std::fs\|fs::" crates/coverly-core/src/consoles/{sony,wiiu,xbox,nx}.rs` after the change, only inside `DiskFs`). `l.name` gives the `CUSA…` fallback.
- [ ] **Step 2: Run** `cargo test -p coverly-core --test organize_folder_games --test consoles_current_gen --test organize_walk` — expect the new test to fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** G-RUST; no snapshot changes (`git status --porcelain crates/coverly-core/tests/snapshots` empty).
- [ ] **Step 5: Commit and push:** `git add -A crates src-tauri && git commit -m "refactor(core): run folder probes over a listing abstraction" && git push && git status -sb`

---

## Task 11: Folder games inside zip and 7z archives (`ArchiveFs`, roots, PS4/Vita, actions)

**Files:**
- Modify: `crates/coverly-core/src/organize/inspect.rs` (`ArchiveFs` built from the entries inspection already listed; `archive_folder_game(entries, archive, limits) -> Option<ArchiveFolderGame>`; `vpk_console` becomes a call to it), `crates/coverly-core/src/organize/plan.rs` / `target.rs` / `evidence.rs` (D21 roots, D22 structure-only review, D23 actions, `ItemNote::Vita7z`, `ItemNote::InstallFirst { emulator: "RPCS3" }` for PS3 HDD/PSN), `crates/coverly-core/src/consoles/sony.rs` (`sce_module/steroid.suprx` abort for Vita, review with note)
- Create: `crates/coverly-core/tests/organize_archive_games.rs`
- Test: `crates/coverly-core/tests/adjustments.rs` (`ARCHIVE_GAMES` rows), `organize_inspect.rs`, the embedded art source test reading a `ZipEntry` from `.zip` and `.7z`

```rust
pub struct ArchiveFolderGame {
    /// The root inside the archive (`""` when the markers sit at the top).
    pub root: String,
    pub matched: FolderMatch,
    /// The SFO could not be read within budget (solid 7z): structure only.
    pub structure_only: bool,
}
pub enum ArchiveGameOutcome { None, One(ArchiveFolderGame), Many(usize), Conflict(DirConflict), CaseCollision, Ps4OrVita }
```

Markers per spec §8.4 (case-insensitive), depth ≤ 3, names `\` → `/`, `unsafe_name` entries dropped, two entries equal after case folding → `CaseCollision` → review. 0 roots → today's behaviour; 1 root → `identify_dir` on the virtual listing; ≥ 2 → review «El comprimido contiene {count} juegos»; `ArchiveTooBig` unchanged. Actions: Vita zip/vpk as is to `psvita/` (7z gets `Vita7z`); PS3 JB, PS4, Wii U loadiine/NUS → `Transfer::Extract` of the root subtree into the folder a disk folder game would get, leftovers → `ArchiveKeptLeftovers`, archive kept; PS3 HDD/PSN → review, default Leave, `InstallFirst { emulator: "RPCS3" }` (existing note); Xbox/360/PS5 roots recognised and moved as is (Q16). `ArchiveFs::art` → `ArtLocator::ZipEntry { name: "<root>/<rel>" }`.

- [ ] **Step 1: Failing tests.** Every existing probe test also runs on an `ArchiveFs` built from the same tree zipped (zip with `/` and with `\` separators, 7z; mtimes pinned) and gives the same `FolderMatch` except art (`ZipEntry`); `probe_split` never matches on `ArchiveFs`; PS3 JB, PS3 HDD (review + note), PS4 (extract), Vita (as is; 7z note), Wii U loadiine and NUS (extract), two roots → review count 2, root depth 4 → ignored, `PS3_GAME/` + `ps3_game/` → review, solid 7z with SFO past 64 KiB → `Ps4OrVita` review with both alternatives, steroid → review; an `.nfo` outside the root → leftovers, archive kept; the embedded source reads `icon0.png` from the zip and the 7z; plain cartridge zips unchanged (snapshot).
- [ ] **Step 2: Run** `cargo test -p coverly-core --test organize_archive_games --test organize_inspect --test adjustments` — expect failures.
- [ ] **Step 3: Implement.** CLI text for the new review reasons.
- [ ] **Step 4: Run** Step 2, G-RUST.
- [ ] **Step 5: Commit and push:** `git add -A crates && git commit -m "feat(core): detect folder games inside zip and 7z archives" && git push && git status -sb`

---

## Task 12: Loose multi-disc sets with an undoable `.m3u`

**Files:**
- Modify: `crates/coverly-core/src/disc/mod.rs`, `disc/sega.rs` (`DiscIdentity.count` from Saturn `CD-n/m` and Dreamcast `GD-ROMn/m`), `crates/coverly-core/src/organize/plan.rs` (call `discsets::group` after identification, before dedupe), `target.rs` (placement by `DiscLayout::{Flat, GameFolder}` exactly as extracted multi-disc archives; `<title>.m3u` when `m3u_for_multi()`), `organize/apply.rs` (no-clobber m3u write, journaled, undone; copy mode too), `ItemNote::MissingDisc { n }`
- Create: `crates/coverly-core/src/organize/discsets.rs`, `crates/coverly-core/tests/organize_discsets.rs`

```rust
/// Disc tag removed for the name key (spec D24).
static DISC_TAG: &str = r"(?i)\s*\((?:Disc|Disk|CD|GD)\s+(\d+|[A-Z])(?:\s+of\s+\d+)?\)";
pub enum SetEvidence { NintendoId, SegaCount, XboxDisc, DatHash, NameOnly }
pub struct DiscSet { pub members: Vec<usize>, pub title: String, pub evidence: SetEvidence, pub missing: Vec<u8> }
pub fn group(items: &[Candidate], db: &SystemDb) -> Vec<DiscSet>;
```

(Check whether `regex` is a dependency; if not, parse the tag by hand — no new crate.)

- [ ] **Step 1: Failing tests.** GC 2-disc (same 6-char id, distinct `disc_no`), Saturn 3-disc `CD-1/3…`, Dreamcast `GD-ROM1/2`, XEX 2-disc, DAT-hash PS1 set, sibling folders `Game (Disc 1)/`, `Game (Disc 2)/`; name-only Sega CD → review «Parece un juego de varios discos»; duplicate disc number → rejected; region/Rev mismatch → rejected; header contradicting the name → rejected; missing disc 2 of 3 → note and m3u of present discs; `(Disc 0)` extra member; existing pile `.m3u` set untouched; target already has `<title>.m3u` → kept, no new one; `m3u_text` output byte-exact; undo removes the m3u (move and copy); PS2/Xbox/360 sets get no m3u; the real PS1 cue/bin layout (synthetic copy of its names) is not grouped.
- [ ] **Step 2: Run** `cargo test -p coverly-core --test organize_discsets --test disc_identify` — expect failures.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** Step 2, G-RUST.
- [ ] **Step 5: Commit and push:** `git add -A crates && git commit -m "feat(core): group loose multi-disc games" && git push && git status -sb`

---

## Task 13: Title-id and folder-tree duplicates act through the duplicates system

**Files:**
- Modify: `crates/coverly-core/src/dedupe/keys.rs` (`ExactKey::acts_unconfirmed()`, `ExactKey::FolderTree { digest }`, `FACTS_VERSION = 3`; ids from content only), `crates/coverly-core/src/dedupe/group.rs` (title groups act as exact relations at `VERSION_CONFIDENCE`, folder trees at `EXACT_CONFIDENCE`; reason ids `title-id`, `folder-tree`), `crates/coverly-core/src/dedupe/units.rs` (tree digest from the metadata-only tree summary; from `ArchiveFs` entries for archive folder games), `crates/coverly-cli/src/dedupe.rs` (reason text), `src/ipc/types.ts` (`DedupeRelation` values)
- Test: `crates/coverly-core/tests/dedupe_keys.rs`, `dedupe_group.rs`, `dedupe_apply.rs`, `dedupe_undo.rs`, `organize_plan.rs` (organize step `set-aside`)

```rust
impl ExactKey {
    /// Acts although not a byte proof (spec D19): a title key whose every
    /// member's id came from content, never from the file name alone.
    pub fn acts_unconfirmed(&self) -> bool { matches!(self, ExactKey::Title { .. }) }
}
// group.rs: a Title group acts only when all members share (title_id, kind, version, disc)
// and every member's IdSource != FileName (Q14); else ReportOnly as today.
/// SHA-1 (upper hex) of the sorted `(lower-case relative path with '/', size)` list.
pub fn folder_tree_digest(entries: &mut [(String, u64)]) -> String;
```

- [ ] **Step 1: Failing tests.** Two NSPs with the same ticket-name id and version → one group, loser set aside to quarantine; same with `dedupe.action = trash` → Trash; undo restores both modes; NSZ vs NSP same key → keeper by format preference; versions `None`/`None` act, `None`/`Some` do not group; different versions → still `ReportWhy::TitleId`; ids only from `[0100…]` file names → report-only; two `CUSA00900` folders with equal trees → act at exact confidence; a zipped PS4 game with the same digest as a library folder → «already in the library» without extracting; equal trees in different systems → never; digest stable across `\`/`/` and case; facts v2 discarded.
- [ ] **Step 2: Run** `cargo test -p coverly-core --test dedupe_keys --test dedupe_group --test dedupe_apply --test dedupe_undo --test organize_plan` — expect failures.
- [ ] **Step 3: Implement.** Commit body: `Behaviour change: title-id and folder duplicates whose ids come from the content are now set aside (or sent to the Trash) with undo instead of only reported (spec D19, Q18).`
- [ ] **Step 4: Run** Step 2, G-RUST, G-WEB (types).
- [ ] **Step 5: Commit and push:** `git add -A crates src && git commit -m "feat(core): act on title id and folder duplicates" && git push && git status -sb`

---

## Task 14: Huge items, back end: `huge`/`rest`, `LargeConfirm`, shared gate, `check_space`

**Files:**
- Modify: `crates/coverly-core/src/organize/apply.rs` (`HugeItem`, `LargeTransfer.{huge, rest}`, `LargeConfirm`, `pub fn gate(plan, confirm: Option<&LargeConfirm>) -> Result<(), LargeTransfer>`, `check_space` counts what `large_transfer` counts), `crates/coverly-core/src/organize/mod.rs` (re-exports)
- Create: `crates/coverly-core/tests/organize_huge_items.rs`
- Test: `crates/coverly-core/tests/organize_large_transfer.rs` (existing cases still pass)

```rust
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HugeItem {
    pub item: usize,            // OrganizeItem::id
    pub name: String,
    pub system: String,
    /// Bytes this item writes, its followers included (shadPS4 overlays).
    pub bytes: u64,
    pub extract: bool,
    pub followers: Vec<usize>,
}
// LargeTransfer gains:
pub huge: Vec<HugeItem>,
/// Bytes of every non-huge item.
pub rest: u64,

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct LargeConfirm { pub bytes: u64, pub items: Vec<usize> }

/// Huge = an item alone writing ≥ threshold, or an archive folder-game
/// extraction of any size (D15). `large_transfer` now returns `Some` when
/// `huge` is non-empty **or** total ≥ threshold.
pub fn gate(plan: &OrganizePlan, confirm: Option<&LargeConfirm>) -> Result<(), LargeTransfer> {
    let Some(t) = large_transfer(plan) else { return Ok(()) };
    let ok = confirm.is_some_and(|c| t.huge.iter().all(|h| c.items.contains(&h.item)) && c.bytes >= t.bytes);
    if ok { Ok(()) } else { Err(t) }
}
```

Followers come from batch 3's overlay pairing (`grep -n "fn overlay_pairs\|siblings_follow" crates/coverly-core/src/organize/target.rs`). A plan whose huge items are all declined (`Choice::Decline` on the decided plan) has no huge items, so the gate only asks the aggregate for `rest` (D16b).

- [ ] **Step 1: Failing tests.** Single 12 GB copy → huge; 9 GB item in a 15 GB plan → not huge, aggregate only (batch 3 unchanged); archive folder-game extraction of 1 GB → huge; PS4 base with two `-UPDATE` overlays → `bytes` includes them, `followers` lists them; `rest`; gate accepts full confirmation, refuses a missing item id, refuses lower `bytes`, accepts after the item is declined (decided plan), and a plan that grew is refused; `Decline` → `NotConfirmed`, followers left too; three 40 GB PS4 + 20 GB rest → three huge + `rest = 20 GB`; `check_space` refuses a same-volume 40 GB extraction on a nearly full target (`set_free_space` hook); old saved plans still load (resume never gated).
- [ ] **Step 2: Run** `cargo test -p coverly-core --test organize_huge_items --test organize_large_transfer` — expect failures.
- [ ] **Step 3: Implement.** Commit body: `Behaviour change: archive folder-game extractions always ask; same-volume extractions are now space-checked (spec D15, D17).`
- [ ] **Step 4: Run** Step 2, G-RUST.
- [ ] **Step 5: Commit and push:** `git add -A crates && git commit -m "feat(core): per-item confirmation for huge transfers" && git push && git status -sb`

---

## Task 15: CLI: per-item prompt, `--skip-large`, `coverly catalog switch`

**Files:**
- Modify: `crates/coverly-cli/src/organize.rs` (`Prompt` trait, `TtyPrompt` on stdin/stderr with `std::io::IsTerminal`, `--skip-large`, `--yes-large` conflicts with it, the D16b sequence, call `SwitchCatalog::ensure` per P-2 unless `--offline`), `crates/coverly-cli/src/main.rs` (subcommand), `crates/coverly-cli/src/dedupe.rs` (passes `titles`)
- Create: `crates/coverly-cli/src/catalog.rs`, `crates/coverly-cli/tests/cli_catalog.rs`
- Test: `crates/coverly-cli/tests/cli_organize.rs`

```rust
pub trait Prompt {
    /// `None` on EOF / Ctrl-C.
    fn ask(&mut self, question: &str) -> Option<Answer>;
}
pub enum Answer { Yes, All, No }
// question (stderr): Copy "<name>" (<size>; <free> free on <volume> after this)? [y]es / [a]ll / [n]o:
// Move/Extract variants by HugeItem.extract and plan mode; free omitted when unknown;
// "does not fit" → only [n]o accepted, says "(does not fit: <missing> short)".
```

Test entry: a hidden `#[cfg(feature = "test-util")]` env var `COVERLY_TEST_PROMPT="y,a,n"` makes the binary use a scripted `Prompt` and treat stdin/stderr as terminals, so `cli_organize.rs` runs on every OS without a pty.

- [ ] **Step 1: Failing tests.** Scripted `n,y,a`: first huge item left `not confirmed`, second applied, third and the rest applied without more questions; aggregate question for `rest` after the huge items when it reaches the threshold (`COVERLY_TEST_LARGE_TRANSFER`); EOF at the first prompt → nothing written, today's refusal text and exit code; prompts on stderr only, `--report json` stdout parses as JSON; `--yes-large` asks nothing; `--skip-large` leaves every huge item, rest still needs `--yes-large` when over the threshold; both flags → usage error (clap `conflicts_with`); non-TTY without flags → refusal byte-for-byte unchanged (compare with the batch 3 test's expected text). `coverly catalog switch status` with the switch off → `off`; `update` with the switch off → error naming `coverly settings set catalogs.switch.enabled true`; `remove`; `--offline` organize never touches the catalogue (fake HTTP counts 0).
- [ ] **Step 2: Run** `cargo test -p coverly-cli --features test-util` — expect failures (check the feature name in `crates/coverly-cli/Cargo.toml`).
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** Step 2, G-RUST.
- [ ] **Step 5: Commit and push:** `git add -A crates && git commit -m "feat(cli): ask per huge item and manage the Switch catalogue" && git push && git status -sb`

---

## Task 16: Single credits data file with coverage tests

**Files:**
- Create: `credits/credits.json`, `credits/credits.schema.json`, `crates/coverly-core/src/credits.rs`, `src/lib/credits.ts`, `src/lib/credits.test.ts`
- Modify: `crates/coverly-core/src/lib.rs` (`pub mod credits;`), `package.json` (`format`/`format:check` globs gain `"credits/**/*.json"`), `tsconfig.json` (include `credits/*.json` with `resolveJsonModule`), `vite.config.ts` only if the import needs it

Entry shape (spec D26), e.g.:

```json
{
  "id": "screenscraper",
  "group": "art-sources",
  "name": "ScreenScraper",
  "url": "https://www.screenscraper.fr/",
  "use": { "es": "Medios e identificación por hash en Organizar, con tu cuenta.", "en": "Media and hash identification in Organize, with your account." },
  "license": { "spdx": "CC-BY-NC-SA-4.0", "name": "CC BY-NC-SA 4.0", "url": "https://creativecommons.org/licenses/by-nc-sa/4.0/" },
  "credit": { "es": "Datos y medios de ScreenScraper (screenscraper.fr), CC BY-NC-SA 4.0.", "en": "Data and media from ScreenScraper (screenscraper.fr), CC BY-NC-SA 4.0." },
  "required": true,
  "endpoints": ["https://api.screenscraper.fr/"],
  "sources": ["screenscraper"]
}
```

Populate every entry of `docs/02-research/credits-inventory.md` in the six groups, with at least: ScreenScraper, libretro-thumbnails, SteamGridDB, embedded icons (in-file art), Nintendo eShop icons (© Nintendo, opt-in, local only, no `url` to a store page beyond the official host is needed — use the credit text only), titledb (MIT, «© 2019 Blake Warner»), nlib (GPL-3.0 service, `url` the `ghost-land/nlib-api` repo), libretro-database (CC BY-SA 4.0), No-Intro, Redump, MAME, FBNeo, TOSEC (courtesy credits per Q8), GameTDB (required link), the wikis (switchbrew, wiibrew, wiiubrew, 3dbrew, xboxdevwiki, psdevwiki), conventions (ES-DE, Batocera, muOS, Knulli, RetroBat, Onion…), emulator sources cited by research (Dolphin, PPSSPP, RPCS3, Vita3K, Cemu, Xenia, xemu, Azahar, DuckStation, shadPS4, Cxbx-Reloaded, decaf-emu: **no `url`**), libraries (Tauri, React, zustand, Radix, Tailwind, shadcn/ui, lucide, rusqlite/SQLite, ureq, serde, zip, sevenz-rust2, image…), fonts (Archivo, Atkinson Hyperlegible Next, Atkinson Hyperlegible Mono: OFL-1.1) and icons (RetroArch Assets XMB Monochrome CC BY 4.0 with its exact notice). Only include entries whose licence and URL can be read from the research files or the dependency's own metadata; anything unverified gets `"credit"` omitted and «Crédito de cortesía» in `use` (Q8).

`credits.rs`: `include_str!("../../../credits/credits.json")`, `Credit`, `credit(id) -> Option<&Credit>`, and the D30 tests: (a) every `hosts::ENDPOINTS` entry with a credit names an entry whose `endpoints` contains that prefix; every `https://` literal in non-test code under `crates/*/src` and `src-tauri/src` outside `net/hosts.rs`, `prefs.rs` and XML namespaces fails naming the file; (b) every `KNOWN_SOURCES` id and every catalogue id (`titledb`, `titledb-ncas`, `nlib`, `eshop-icons`) is in some entry's `sources`/`datasets`; (c) every distinct DAT origin of the built-in system DB (read through its API: `grep -n "pub fn dat_paths\|fn dats\b" crates/coverly-core/src/systems/mod.rs`) has a dataset entry; (d) ScreenScraper `CC-BY-NC-SA-4.0`, RetroArch Assets `CC-BY-4.0`, GameTDB with `url`, libretro-database `CC-BY-SA-4.0`, titledb `MIT`, each `required` with a `credit`. Schema test: URLs HTTPS, no `?`, not on the denylist (ROM/key/emulator hosts and the emulator repos listed above, by host + path prefix).

`src/lib/credits.ts`: typed import, `groupCredits(locale)`, `searchCredits(query, locale)` over name/use/licence (accent- and case-insensitive, like `src/lib/search.ts`).

- [ ] **Step 1: Failing tests.** Rust D30 (a)–(d) and the schema/denylist test; a temp copy of the JSON without the `titledb` entry fails (b) naming it. Vitest: es/en parity of every `use`/`credit`, every group non-empty, required entries present, search «retroarch» finds the icons entry, accent-insensitive search.
- [ ] **Step 2: Run** `cargo test -p coverly-core credits && pnpm vitest run src/lib/credits.test.ts` — expect failures.
- [ ] **Step 3: Implement.** If the GameTDB link wording cannot be confirmed from `credits-inventory.md`, use their site URL with the FAQ's wording and add `BLOCKED: GameTDB credit wording — <why>`.
- [ ] **Step 4: Run** Step 2, G-RUST, G-WEB.
- [ ] **Step 5: Commit and push:** `git add -A credits crates src package.json tsconfig.json && git commit -m "feat(credits): single credits data file with coverage test" && git push && git status -sb`

---

## Task 17: IPC: per-item confirmation, catalogue and credit commands

- [ ] **Step 0: Sync.** `git fetch origin && git merge origin/main`; resolve conflicts by hand (locale files key by key), run G-RUST and G-WEB, commit the merge (`git commit --no-edit`), `git push`.

**Files:**
- Modify: `src-tauri/src/ipc.rs` (`organize_apply(plan_id, decisions, confirm: Option<LargeConfirm>)` replacing `confirmed_bytes`; `catalog_status`, `catalog_update` (job with progress event `catalog-progress`, cancellable), `catalog_remove`, `catalog_icon(tid)` (returns the cached PNG through the existing thumbnail path, never a URL); `open_credit(id, which: CreditLink)`), `src-tauri/src/state.rs` (`SwitchCatalog` handle in `AppState`, dropped before remove; P-2 ensure before plan/scan; gate via `coverly_core::organize::gate`), `src-tauri/src/error.rs` (`IpcError::LargeTransfer` gains `huge`, `rest`), `src-tauri/src/lib.rs` (`invoke_handler` registrations), `src/ipc/types.ts`, `src/ipc/backend.ts`, `src/ipc/tauriBackend.ts`, `src/ipc/contract.sample.json`, `src/ipc/contract.test.ts`, `src/ipc/mock/mockBackend.ts` (stubs, filled in Tasks 18–20)
- Create: `src-tauri/tests/catalog_state.rs`
- Test: `src-tauri/tests/organize_state.rs`, `src-tauri/tests/contract.rs`

`open_credit`: looks the id up in `coverly_core::credits`, opens `url` (`which = "site"`) or `license.url` (`"license"`) with `tauri_plugin_opener` exactly as `open_link` does; unknown id or missing URL → `IpcError::BadRequest`. No new capability.

- [ ] **Step 1: Failing tests.** `organize_state.rs`: apply without `confirm` → `LargeTransfer` error JSON `{"code":"large-transfer","detail":{…,"huge":[…],"rest":…}}` and nothing written; with every huge id and `bytes` → job starts; a missing id → refused; decline decisions + confirm for the rest → applies, declined item `not-confirmed`; resume never asks. `catalog_state.rs` (fake HTTP): status off; update with switch off → `BadRequest`; update on → ready; remove → file gone; plan with no Switch item never calls ensure; paused network never calls ensure. `open_credit("screenscraper","site")` resolves the URL (test the resolver function, not the opener); `open_credit("dolphin","site")` → `BadRequest` (no url). Contract: new commands and shapes in the sample.
- [ ] **Step 2: Run** `cargo test -p coverly-roms --test organize_state --test catalog_state --test contract && pnpm vitest run src/ipc/contract.test.ts` — expect failures.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** Step 2, G-RUST, G-WEB.
- [ ] **Step 5: Commit and push:** `git add -A src-tauri src && git commit -m "feat(ipc): catalogue, credits and per-item confirmation commands" && git push && git status -sb`

---

## Task 18: UI: per-item confirmation dialog and the `huge` mock pile

**Files:**
- Create: `src/components/organize/HugeItemDialog.tsx`, `src/components/organize/HugeItemDialog.test.tsx`, `src/ipc/mock/adjustFixture.ts` (piles `huge`, `switch`, `odd`, `discs`, `dupes-title`; this task fills `huge`)
- Modify: `src/stores/organizeStore.ts` (`pendingLarge` gains a cursor over `huge`, `answerHuge(answer: 'yes' | 'all' | 'no')`, builds `decline` decisions and `LargeConfirm { bytes: rest + Σ accepted, items }` from the error's numbers only), `src/components/organize/LargeTransferDialog.tsx` (shown after the huge items only when `rest ≥ threshold`, numbers = `rest`; «Sí a todo» skips it), `src/screens/OrganizeScreen.tsx`, `src/ipc/mock/mockOrganize.ts` (`?mockPick=huge`: three PS4 folders of 38, 52 and 61 GB on a 120 GB target and one zipped PS3 disc; raise `large-transfer` with `huge`/`rest` like the backend), `locales/{es,en}/ui.json` (`organize.huge.*`, `organize.left.notConfirmed`, spec §10)
- Test: `src/stores/organizeStore.test.ts`, `src/ipc/mock/mockOrganize.test.ts`, `src/components/organize/LargeTransferDialog.test.tsx`

Dialog (spec §9.3): one `alert-dialog` instance for the whole sequence, content swapped in place; title by mode `organize.huge.titleCopy|titleMove|titleExtract` (long names wrap); body `organize.huge.body` with `{bytes}`, `{free}` (free after `rest` and items accepted so far, minus 1 %) in mono, `{volume}` plain; `bodyNoFree` when free is unknown; `organize.huge.progress` «Elemento {index} de {count}» in the accessible description; buttons «No» (initial focus each item), «Sí a todo», «Sí»; Escape = «No»; no room → `organize.huge.noRoom`, «Sí»/«Sí a todo» `aria-disabled` with the reason as description; all declined and nothing else moves → toast `organize.huge.nothing`.

- [ ] **Step 1: Failing tests.** Order of items; «No» adds a `decline` decision; «Sí a todo» accepts the current and every later item and skips the aggregate; `confirm.bytes` = `rest + Σ accepted` from the error numbers; free space decreases item by item; no room disables «Sí» with description; Escape = «No»; focus on «No» on every item; one dialog instance (same DOM node across items); aggregate dialog for `rest` after the huge items when it applies; mock `huge` raises then applies; all declined → toast.
- [ ] **Step 2: Run** `pnpm vitest run src/components/organize src/stores/organizeStore.test.ts src/ipc/mock/mockOrganize.test.ts` — expect failures.
- [ ] **Step 3: Implement** with frontend-design:frontend-design; screenshot light/dark at 1280 and 760 px and critique before committing.
- [ ] **Step 4: Run** Step 2, G-WEB.
- [ ] **Step 5: Commit and push:** `git add -A src locales && git commit -m "feat(ui): per-item confirmation dialog" && git push && git status -sb`

---

## Task 19: UI: «Catálogo de Switch» in the Sources tab

**Files:**
- Modify: `src/components/settings/SourcesSection.tsx` (group per spec §9.2: switch, status line, «Descargar ahora» / «Buscar actualización», «Borrar catálogo», icons switch disabled until on, Advanced fold with fallback switch and URL field), `src/components/settings/registry.ts` (`setting-sources-switch-catalog`, `…-icons`, `…-fallback`), `src/ipc/mock/mockSettings.ts` (catalogue status; simulated 2 s download with progress), `src/components/settings/AboutSection.tsx` (corrected `about.noTelemetry`), `locales/{es,en}/ui.json` (`settings.sources.switchCatalog.*`, `about.noTelemetry`)
- Create: `src/components/settings/SourcesSection.test.tsx` (if absent; else extend)
- Test: `src/components/settings/registry.test.tsx`, `src/components/settings/Sections.test.tsx`

- [ ] **Step 1: Failing tests.** States off, downloading (`{done} de {total}` in mono), ready, fallback (United States list), failed old/none, rejected, remove failed; icons and fallback disabled while off; fallback URL `http://` shows «Tiene que empezar por https://» (existing validation key); toggling on calls `catalogUpdate`; «Borrar catálogo» calls `catalogRemove`; registry completeness; `noTelemetry` text exact in es and en.
- [ ] **Step 2: Run** `pnpm vitest run src/components/settings` — expect failures.
- [ ] **Step 3: Implement** with frontend-design:frontend-design (same `Field` primitives as the rest of the tab, no new layout); screenshots and critique.
- [ ] **Step 4: Run** Step 2, G-WEB.
- [ ] **Step 5: Commit and push:** `git add -A src locales && git commit -m "feat(ui): Switch catalogue settings" && git push && git status -sb`

---

## Task 20: UI: «Cifrado», homebrew, parts, archive games, disc sets and title duplicates in Organizar

**Files:**
- Modify: `src/lib/organize.ts` (copy for `game-part`, `not-confirmed`, `homebrew`, `vita7z`, `missing-disc`, `encrypted-name`, review reasons `archive-games`, `ps4-or-vita`, `disc-set`, why lines `titledb`/`nca-catalog`), `src/components/organize/ItemName.tsx` («Cifrado» `Badge` with `organize.encryptedUnresolved` as accessible description and a visible detail line; never two «Cifrado» texts), `SystemGroups.tsx`, `ReviewBuckets.tsx`, `LeftInPlace.tsx`, `src/lib/dedupe.ts` + dedupe screen components (`dedupe.relation.titleId`, `dedupe.relation.folderTree`), library tile (catalogue icon through the thumbnail IPC), `src/ipc/mock/adjustFixture.ts` + `mockOrganize.ts` + `mockDedupe.ts` (piles `switch`, `odd`, `discs`, `dupes-title`), `locales/{es,en}/ui.json` (spec §10 organize/dedupe keys)
- Test: `src/lib/organize.test.ts`, `src/lib/dedupe.test.ts`, `src/components/organize/*.test.tsx`, `src/ipc/mock/mockOrganize.test.ts`, `src/ipc/mock/mockDedupe.test.ts`, locale parity test

- [ ] **Step 1: Failing tests.** Every new reason/note/why renders its exact §10 copy in es and en; the badge is text and has the description; a resolved encrypted 3DS row shows only the existing note; mock piles: `switch` (names from the mock catalogue, `[0100…]`-less XCI resolved by NCA id, one «Cifrado»), `odd` (one row per §8.2 row), `discs` (Saturn and GC sets with m3u target, Sega CD in review, missing-disc note), `dupes-title` (two NSPs same id/version, two `CUSA00900` folders equal trees → set aside, undo restores).
- [ ] **Step 2: Run** `pnpm vitest run src/lib src/components/organize src/ipc/mock` — expect failures.
- [ ] **Step 3: Implement** with frontend-design:frontend-design; screenshots and critique.
- [ ] **Step 4: Run** Step 2, G-WEB.
- [ ] **Step 5: Commit and push:** `git add -A src locales && git commit -m "feat(ui): show encrypted names, homebrew, game parts, disc sets and title duplicates" && git push && git status -sb`

---

## Task 21: UI: «Agradecimientos» settings tab

- [ ] **Step 0: Sync** with `origin/main` (as Task 17 Step 0).

**Files:**
- Create: `src/components/settings/CreditsSection.tsx`, `src/components/settings/CreditsSection.test.tsx`
- Modify: `src/stores/navStore.ts` (`SettingsSection` gains `'credits'` after `'about'`), `src/screens/SettingsScreen.tsx`, `src/components/settings/registry.ts` (`setting-credits-search`, `setting-credits-notices`, and one `credit-<id>` entry **generated from `credits.json`**), `src/components/settings/search.ts` (landing on a credit row focuses its `<article>`), `locales/{es,en}/ui.json` (`settings.section.credits`, `settings.credits.*`)
- Test: `src/components/settings/registry.test.tsx`, `src/components/settings/search.test.ts`, `src/stores/navStore` tests

Design per spec §9.1 (frontend-design pass is binding): colophon reading page, one left-aligned column (max 72 ch), search field with visible label and `aria-live` «{count} de {total}» (mono numbers), groups in `TODO.md` §7 order headed by the shelf rule (1 px border, 3 px Buckram cap) with the count in mono, rows as `<article aria-labelledby>` in a definition list (name 600, licence right-aligned plain secondary text, linked when it has a URL; one-line use; required credit as a `<blockquote>` with a 2 px `--cr-border-strong` left rule); links «Web oficial de {name}» / «Licencia de {name}» call `openCredit(id, which)` with `aria-describedby` «Se abre en el navegador»; entries without `url` show no link; footer «Ver licencias de terceros» (secondary button, opens the Task 22 viewer; a placeholder `onClick` until then is not allowed — leave the button out in this task and add it in Task 22) and «Coverly no incluye ROMs, BIOS, claves ni arte de juegos.». No cards, no all-caps eyebrows, no motion.

- [ ] **Step 1: Failing tests.** Groups in order with counts; search filters and announces the count; empty state «Nada coincide con «{query}».»; link buttons call `openCredit(id, 'site'|'license')`, never a URL; RetroArch notice text exact; entry without url has no link; ⌘/Ctrl+F settings search «ScreenScraper» lands on and focuses its article; nav still fits at 760 px (13 tabs; layout test or snapshot of the nav width class); es ↔ en switch.
- [ ] **Step 2: Run** `pnpm vitest run src/components/settings src/stores` — expect failures.
- [ ] **Step 3: Implement** with frontend-design:frontend-design; screenshots light/dark at 1280 and 760 px; critique against spec §9.1 «Review against generic defaults» before committing.
- [ ] **Step 4: Run** Step 2, G-WEB.
- [ ] **Step 5: Commit and push:** `git add -A src locales && git commit -m "feat(ui): Agradecimientos settings tab" && git push && git status -sb`

---

## Task 22: Build-time third-party notices, bundled licence texts and the notices viewer

**Files:**
- Create: `about.toml`, `about.hbs`, `scripts/licenses/notices.mjs`, `LICENSES/generated/rust.txt`, `LICENSES/generated/js.txt`, `src/lib/notices.ts` (+ test), `src/components/settings/NoticesDialog.tsx` (+ test), `src-tauri/tests/bundle_resources.rs`
- Modify: `package.json` (`notices`, `notices:check`; `check-all` gains `pnpm notices:check`), `src-tauri/tauri.conf.json` (`bundle.resources`: `"../LICENSES/": "LICENSES/"` and each `"../src/assets/fonts/OFL-*.txt": "LICENSES/fonts/OFL-*.txt"`), `src/components/settings/CreditsSection.tsx` (footer button), `src/components/settings/AboutSection.tsx` («Avisos de terceros» opens the viewer), `locales/{es,en}/ui.json` (`notices.*`, `settings.credits.notices`), the release workflow file if it is in the repo (`ls .github/workflows`; add `pnpm notices:check` before `pnpm tauri build` in the `app` job — else `BLOCKED:` per P-6)

`about.toml`: `targets = ["aarch64-apple-darwin","x86_64-apple-darwin","x86_64-pc-windows-msvc","aarch64-pc-windows-msvc","x86_64-unknown-linux-gnu","aarch64-unknown-linux-gnu"]`, `ignore-build-dependencies = true`, `ignore-dev-dependencies = true`, `accepted` = the allowlist of `scripts/check-licenses.sh` (read it and copy, including `MPL-2.0`, `CDLA-Permissive-2.0`, `ISC`, `Unicode-3.0`, `OpenSSL`/BoringSSL text for `ring` if present), workspace crates excluded. `notices.mjs`: if `cargo about --version` fails, print `install with: cargo install cargo-about --locked --version 0.9.2` and exit 1 without touching the files; else `cargo about generate --locked about.hbs > LICENSES/generated/rust.txt`; `pnpm licenses list --prod --json` plus the explicit devDependency list whose code reaches the bundle (`tailwindcss`, `tailwindcss-animate`, `vite` runtime helpers — check the built `dist/assets/*.{js,css}` banner comments after `pnpm build` and adjust the list), each package's own licence file from `node_modules` → `js.txt`. Both files start with `lock-sha256: <hex>` of the normalised lock (`Cargo.lock` without packages lacking `source`, `pnpm-lock.yaml`, CRLF → LF). `notices:check` recomputes and fails with `run pnpm notices` on mismatch (pure Node, no cargo-about).

Viewer (spec §9.1 «Notices viewer»): large dialog «Licencias de terceros», tabs «Rust ({count})» / «JavaScript ({count})» (mono counts), find field (Enter / Shift+Enter, `aria-live` «{index} de {count}»), text in Atkinson Hyperlegible Mono at caption size, one block per package rendered with `@tanstack/react-virtual`, `<mark>` only in visible blocks, debounced search; files loaded with a dynamic `?raw` import (works in the mock); focus starts on find, Escape closes, focus returns to the button; usable at 760 px.

- [ ] **Step 1: Failing tests.** `notices.test.ts`: splitting per package, counts; hash normalisation ignores CRLF and workspace-package versions (fixture locks). `NoticesDialog.test.tsx`: tabs, find highlights and jumps, count announced, focus start/return. `bundle_resources.rs`: the resolved `tauri.conf.json` maps `LICENSES/` (every `*.txt`, `README.md`, `generated/`) and every `src/assets/fonts/OFL-*.txt`, and every mapped source exists. `pnpm notices:check` fails on a temp copy with a changed lock and passes on the tree.
- [ ] **Step 2: Run** `pnpm vitest run src/lib/notices.test.ts src/components/settings/NoticesDialog.test.tsx && cargo test -p coverly-roms --test bundle_resources` — expect failures.
- [ ] **Step 3: Implement**; run `pnpm notices` (cargo-about 0.9.2 is installed on this machine) and commit the generated files; frontend-design screenshots of the viewer and critique.
- [ ] **Step 4: Run** Step 2, `pnpm notices:check`, G-RUST, G-WEB.
- [ ] **Step 5: Commit and push:** `git add -A about.toml about.hbs scripts LICENSES src src-tauri locales package.json && { [ -d .github ] && git add -A .github; true; } && git commit -m "build: generate third-party notices and bundle licence texts" && git push && git status -sb`

---

## Task 23: Docs: third-party notices, legal page and README credits

**Files:**
- Create: `scripts/credits/readme.mjs` (writes the block between `<!-- credits:start -->`/`<!-- credits:end -->` in `README.md` (en) and `README.es.md` (es) from `credits/credits.json`; `--check` exits 1 when the block differs), `scripts/credits/readme.test.mjs` or a Vitest test under `src/lib/` that imports it
- Modify: `package.json` (`credits:readme`, `credits:check`; `check-all` gains `pnpm credits:check`), `README.md`, `README.es.md` (markers + generated block), `THIRD-PARTY-NOTICES.md` (spec D31: drop TheGamesDB and the `docs/09-legal/` per-source pointer; add titledb MIT © 2019 Blake Warner, api.nlib.cc / nlib-api GPL-3.0 service contacted only with the fallback on, Nintendo eShop icons © Nintendo opt-in local cache never redistributed, GameTDB link, libretro-database CC BY-SA, shadcn/ui MIT, wry, reqwest, brotli, option-ext MPL-2.0 with its source URL, webpki-roots CDLA-Permissive-2.0, ring Apache-2.0 AND ISC (BoringSSL), ICU4X Unicode-3.0, and a pointer to `LICENSES/generated/`), `docs/09-legal/license.md` (drop TheGamesDB; name the Switch catalogue as an optional download), `LICENSES/README.md` (lists `generated/` and `fonts/`), `CHANGELOG.md` (Unreleased, Task 24 completes it)

- [ ] **Step 1: Failing test.** `credits:check` fails on the current README (no markers); the generated block lists every group and every required credit text verbatim.
- [ ] **Step 2: Run** `pnpm credits:check` — expect failure.
- [ ] **Step 3: Implement**; run `pnpm credits:readme`; `grep -n "TheGamesDB\|docs/09-legal" THIRD-PARTY-NOTICES.md` prints nothing.
- [ ] **Step 4: Run** `pnpm check-all && pnpm build`.
- [ ] **Step 5: Commit and push:** `git add -A scripts package.json README.md README.es.md THIRD-PARTY-NOTICES.md docs/09-legal LICENSES/README.md && git commit -m "docs: fix third-party notices and README credits" && git push && git status -sb`

---

## Task 24: Verification, real-data check, E2E, changelog and PR

**Files:** `CHANGELOG.md` (Unreleased), `docs/superpowers/specs/2026-10-26-adjustments.md` (status line only: «Implemented on `feat/adjustments`»); screenshots and the PR body under the scratchpad (not committed).

- [ ] **Step 0: Sync** with `origin/main`; resolve conflicts by hand (locale files key by key), run all gates, commit the merge, push.
- [ ] **Step 1: Changelog.** Under `## [Unreleased]`: **Added** — optional Switch name catalogue (blawar/titledb) with NCA lookup, opt-in Switch icons and an optional online fallback; «Encrypted» label for unresolved Switch/3DS names; header classification of loose executables and title metadata (homebrew, updates, DLC, parts of a game); per-item confirmation for huge items (app and `coverly organize`, `--skip-large`); folder games inside zip/7z; loose multi-disc sets with an `.m3u`; «Credits» settings tab with third-party licences viewable in the app; `coverly catalog switch`. **Changed** — title-id and folder duplicates proven by content are set aside or sent to the Trash with undo instead of only reported; archive folder-game extractions always ask; same-volume extractions are space-checked; the bundle now ships `LICENSES/` and the font licences. English, plain words, no emulator names.
- [ ] **Step 2: Full gates** from a clean tree; keep the last line of each for the PR evidence:

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-adjustments && pnpm check-all && pnpm build && cargo fmt --all --check && cargo clippy --workspace --all-targets --all-features -- -D warnings && scripts/dev/clippy-windows.sh && cargo test --workspace --no-fail-fast
```

- [ ] **Step 3: Real-data check, read-only.**
  - Plan-only runs (never `--apply`, never `--offline` off with the catalogue on unless deliberately testing it): `cargo run -q -p coverly-cli -- organize /Volumes/TinyExt/ROMs-Downloaded --report json > /private/tmp/claude-501/-Users-miguelangelesparzacalero-Documents-GitHub/35b7c020-f2ae-412f-b757-99c24a9a498b/scratchpad/adjustments/plan-downloaded.json` and the same for `/Volumes/TinyExt/Roms` (check the exact flags with `cargo run -q -p coverly-cli -- organize --help`). Record per-system counts, every `game-part`, `homebrew`, `encrypted-name`, archive folder game, disc set and duplicate group, and any unexpected row.
  - Write tests on copies only: `mkdir -p target/adjust-sample && cp -RXp "<Wii U loadiine folder>" "<the .wua>" "<Hatsune Miku - Project Mirai DX (USA).3ds>" "<…(USA).zip>" "<PS1 cue/bin set>" target/adjust-sample/` (paths found with `find /Volumes/TinyExt -maxdepth 4 -iname '*Mirai*' -o -iname '*.wua' 2>/dev/null | head`); `find target/adjust-sample -name '._*' | wc -l` prints `0`; run `coverly organize target/adjust-sample --target target/adjust-out --apply --yes-large` then `coverly undo`; expected: the Miku pair is one duplicate group, the PS1 set is not falsely grouped, undo restores everything; then `rm -rf target/adjust-sample target/adjust-out`.
  - Catalogue live check (optional, owner network): with a temp data dir (`COVERLY_DATA_DIR` or the CLI's equivalent; check `--help`), `coverly settings set catalogs.switch.enabled true`, `coverly catalog switch update`, `status` shows the count; confirm `catalog.sqlite` has no `key` column (`sqlite3 <path> '.schema'`); delete the temp dir.
- [ ] **Step 4: Browser E2E** (spec §14.3). Start `pnpm dev --port 1468 --strictPort` in the background. Chrome DevTools MCP `new_page` with `isolatedContext: "adjustments"`; fall back to Playwright MCP, then `playwright-core` + Chrome for Testing in `/private/tmp/claude-501/-Users-miguelangelesparzacalero-Documents-GitHub/35b7c020-f2ae-412f-b757-99c24a9a498b/scratchpad/adjustments`.
  1. `http://localhost:1468/?mockPick=huge`: apply → per-item dialog; answer «No», «Sí», then «Sí a todo»; free-space numbers decrease; the result shows one item «No confirmado; se queda donde está»; keyboard only (focus on «No», Escape = «No»).
  2. `?mockPick=switch` with the mock catalogue on: names from the catalogue, the `[0100…]`-less XCI resolved by NCA id, one row labelled «Cifrado» with its description.
  3. `?mockPick=odd`: each §8.2 row shows its system and kind or «Parte de un juego; no se mueve suelta».
  4. `?mockPick=discs`: Saturn set grouped with its m3u target, Sega CD in review, missing-disc note.
  5. `?mockPick=dupes-title`: duplicates set aside with «Mismo title ID y versión» / «Misma carpeta», undo restores.
  6. Settings → Sources: catalogue toggle, simulated download progress, icons enabled after; corrected «Sin telemetría» text in Acerca de.
  7. Settings → Agradecimientos: search «retroarch» (count announced), RetroArch notice exact; «Ver licencias de terceros» opens the viewer, find a crate name (`serde`), Enter jumps, Escape returns focus; ⌘F settings search «ScreenScraper» lands on its row; es ↔ en switch.
  8. Screenshots light/dark (`data-theme`) at 1280 and 760 px for the tab, the viewer and the per-item dialog; `prefers-reduced-motion: reduce` emulated; `list_console_messages` has no errors. Screenshots to `/private/tmp/claude-501/-Users-miguelangelesparzacalero-Documents-GitHub/35b7c020-f2ae-412f-b757-99c24a9a498b/scratchpad/adjustments/e2e/`. Stop the server: `lsof -ti tcp:1468 -sTCP:LISTEN | xargs -r kill`.
- [ ] **Step 5: Commit and push:** `git add CHANGELOG.md docs/superpowers/specs/2026-10-26-adjustments.md && git commit -m "docs: changelog for the adjustments batch" && git push && git status -sb`
- [ ] **Step 6: Open the PR** in English, no Claude/AI mention, no generated-by line, no emulator or ROM/key link:

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-adjustments && gh pr create --base main --head feat/adjustments \
  --title "feat: keyless Switch names, header classification, per-item confirmation, archive folder games and credits (adjustments batch)" \
  --body-file /private/tmp/claude-501/-Users-miguelangelesparzacalero-Documents-GitHub/35b7c020-f2ae-412f-b757-99c24a9a498b/scratchpad/adjustments/pr-body.md
```

  The body (scratchpad, not the repo) has: Summary per adjustment (1, 2, 4, 5, 6, 7); the keys rule and the tests guarding it; catalogue behaviour (off by default, what is stored, refresh, fallback, icons); behaviour changes (title/folder duplicates act, archive folder games always ask, same-volume extraction space check, `coverly organize` prompt and `--skip-large`); compliance fixes (`credits-inventory.md` §6); **Needs the owner**: every spec §13 item (Q1–Q18) with the default taken, plus plan P-1, P-2, P-6, and the post-merge landing copy (Q15); every `BLOCKED:` line (`git log origin/main..HEAD --format=%b | grep BLOCKED`); real-data results (Step 3); gates evidence (Step 2); E2E evidence with screenshots (uploaded as PR comment images, or described when upload is not possible). Post the local gate evidence as a PR comment too (GitHub Actions is blocked by billing; gates were run locally).
- [ ] **Step 7: End state:** `git status -sb` shows `## feat/adjustments...origin/feat/adjustments` with nothing ahead, and `gh pr view --json url,state` prints the PR.

---

## Spec coverage check

| Spec | Task |
|---|---|
| D1 keys | 4 (schema, `key` reject), 6 (`SparseAt` ranges), 7 (`no_keys.rs`), every task (Global Constraints) |
| D2 opt-in, D3 file choice, D4 refresh | 3 (settings), 4 (`ensure`), 15/17 (callers, P-2) |
| D5 reduction, separate `catalog.sqlite` | 2 (`get_stream`), 4 |
| D6 NCA ids, `ncas.json` | 4 (reducer), 6 (readers, `NcaCatalog`) |
| D7 name priority, `title_source` | 1 (types), 6 (evidence, P-1) |
| D8 icons | 5, 17 (`catalog_icon`), 20 (tile) |
| D9 nlib | 2 (user URL), 3 (setting), 5 |
| D10 offline | 4 (deadline), 15 (`--offline`), 17 |
| D11 «Cifrado» | 7 (core), 20 (UI) |
| D12–D14 loose classifier, `.tmd` table | 1 (types), 8 (existing readers), 9 (`loose.rs`) |
| D15–D17 huge items, gate, space | 1 (`Decline`), 14 (core), 17 (IPC), 18 (UI) |
| D18 CLI prompt | 15 |
| D19 title and folder duplicates | 13 |
| D20 `ListingFs` | 10 (disk), 11 (archive) |
| D21–D23 roots, PS4/Vita, actions | 11 |
| D24 loose multi-disc | 12 |
| D25 credits file, D26 shape | 16, 23 (README) |
| D27 `open_credit` | 17, 21 |
| D28 notices generation | 22 |
| D29 bundle | 22 |
| D30 credit test, endpoint registry | 2 (registry, guard), 16 (tests) |
| D31 notices file fixes | 23 |
| D32 RetroArch notice in UI | 16 (credit text), 21 (tab) |
| D33 facts v9 | 1 |
| §9 UX, §10 copy | 18–22 |
| §12 edge cases | 2–15 |
| §13 owner decisions | 24 (PR) |
| §14 tests | every task; 24 (real data, E2E) |
