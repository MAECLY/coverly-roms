# Updates, DLC, Demos and Revisions in the Six Views, the Game Panel and the Filters; 3DS Region by Product Code (Part C) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking. Every UI task (Tasks 16–26) also REQUIRES frontend-design:frontend-design: follow `docs/08-brand/brand.md`, spec §8.1 and the approved mockup, use the `src/components/ui` primitives and critique the changed screens with screenshots before committing.

**Goal:** `TODO.md` §2.1 part C. A game is one object everywhere: its updates, DLC and retro revisions live inside it and never count as games; demos are games of their own, linked to the full game, and they do count. The approved design (2026-11-09) ships in the six views (Portadas summary under the cover and a dot at small sizes, Galería DLC tray, Detalles treegrid with Versión and Contenido and a ghost row for a missing update, Columnas typed groups, Huecos de arte child rows, Estantes edges behind the box with «Al lado»), the game panel (installed vs newest, versions, revisions with a persisted default, DLC with thumbnails, related demo, read-only Mods/Cheats/Extras/Saves counts), four content filters, the «Complementos sin juego base» entry, a status bar that counts **juegos**, art read in the clear for updates/DLC/demos into child slots with their origin, and the 3DS region (and country) from the product code, from a table verified against 3dbrew, GameTDB and WiiBrew.

**Architecture:** Rust owns every fact and every ordering; the webview owns only the folding into games. Core: a verified pure `consoles/ctr_region.rs` feeds `ConsoleId.region`; the scan gains fixed-folder counts and a pure post-pass (`scan/links.rs`) that links demos and orphan add-ons to their game by title key; a pure `catalog/latest.rs` holds the 3.1 seam (`LatestVersions`, `NoTitleList`) and the in-library version facts (`installed`, `newer_elsewhere`, both through `dedupe::keeper::version_order`); `settings` stores revision defaults keyed by library root; `art/children.rs` owns child art slots (`ChildMedia { Icon, Banner }`, `.art.json` origin) written only through the journaled writer, read only from what `sources/embedded.rs` finds in the clear. Tauri adds optional fields to `EntryView` (unchanged shape for a plain retro ROM) and four id-only commands (`game_details`, `set_default_revision`, `read_child_art`, `read_child_art_bytes`). Webview: a pure `src/lib/games.ts` (`buildGames`) runs in `useVisibleEntries` before scope, filters, sort and grouping, so all six views, filters, status bar and fill see the same `Game[]`.

**Tech Stack:** Rust 1.99 (workspace MSRV 1.93), `serde`, `serde_json`, `insta`, `filetime`, `tempfile`, `image` (already used by `sources/embedded.rs`); React 19 + TypeScript + Tailwind 3.4 + zustand, Vitest + Testing Library, Radix primitives already in `src/components/ui`. **No new crate and no new npm package.**

**Spec:** [`docs/superpowers/specs/2026-10-26-addon-views.md`](../specs/2026-10-26-addon-views.md). It is binding: decisions D1–D19 (§4), the verified 3DS table (§5), files (§6), data flow (§7), UX (§8), copy (§9, word for word), the 3.1 seam (§10), edge cases (§11), tests (§12), owner questions (§14, implemented with the safe default). Visual source of truth: the approved mockup `https://claude.ai/artifact/UPUWX45c9Mj5B1o5hgFrTg`, local copy `/private/tmp/claude-501/-Users-miguelangelesparzacalero-Documents-GitHub/35b7c020-f2ae-412f-b757-99c24a9a498b/scratchpad/views-21c/index.html` with `design-notes.md` and `research-1..3.md` beside it. Reproduce its hierarchy, copy and interactions with the app's tokens and primitives; import none of its CSS, fonts or ad-hoc colours. Part A ([`2026-10-26-game-folders.md`](2026-10-26-game-folders.md), PR #45: `FixedFolder`, `RomEntry.children`, `RomEntry.role`, `version_group`, `revision`, demos D12, region D10, `library_bases.rs` `has_demo_tag`/`demo_free_key`/`pick_by_region`) and part B ([`2026-10-26-mods-cheats-translators.md`](2026-10-26-mods-cheats-translators.md), PR #46: `Mods/`/`Cheats/` filled and linked, `translate/*`) are merged: build on them, redo none of them.

## Global Constraints

- **Worktree and branch.** Work only in `/Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-views`, branch `feat/addon-views`, which tracks `origin/feat/addon-views`. Agent shells reset their cwd, so every shell command starts with `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-views &&`. Never switch branches and never touch `main`.
- **Git rules (mandatory).** An earlier run lost a whole branch because it was never pushed.
  - Conventional, granular commits in English (`feat(core): …`, `fix(core): …`, `test(core): …`, `feat(tauri): …`, `feat(ipc): …`, `feat(ui): …`, `feat(organize): …`, `feat(cli): …`, `test(ui): …`, `docs: …`).
  - **No** `Co-Authored-By` line, no «Generated with» line, no mention of Claude or AI anywhere (commits, PR, code comments). This owner rule overrides any harness attribution.
  - Run `git push` **immediately after every commit**. Every task ends with `git commit … && git push && git status -sb`; the first line must read `## feat/addon-views...origin/feat/addon-views` with no `[ahead N]`.
  - Never force-push. To update with `main`: `git fetch origin && git merge origin/main`, resolve conflicts by hand (other workflows change `locales/*.json`, `src/components/*` and `TODO.md` in parallel; resolve locale files **key by key**, never by taking one side), run the gates, commit the merge, push.
- **Do not edit** `docs/TRACKER.md` or `TODO.md` on this branch (they are updated on `main` after merge).
- **Scope: part C only.** Not here: the Extras CRUD, `extras.json`, the media viewer (part D); reading saves (block 5: placeholder and read-only counts only); device version and sync (block 4); an offline title list with versions (3.1: seam only, `NoTitleList`); automatic network art for children (manual search only); moving orphans from their list (Organizar does it). Do not redo part A or B.
- **Keys rule, no exceptions.** Never read, include, download or decrypt console keys, tickets or licences. Art only from what is in the clear. `consoles::is_key_material(name)` is checked **before** any child is opened (`game_details`, `read_child_art`, Organizar thumbnails). Switch child art is none (NCAs are encrypted). Every test with a `.tik`, `.rap`, `prod.keys` or `title.keys` in a fixture runs under `consoles::start_open_log()` and asserts the key path is never in `take_open_log()`.
- **Offline details.** `game_details` never calls `catalog/nlib.rs` or any network client; it receives no client at all (asserted by its signature and a test).
- **Ids, never paths, from the webview.** Every new command takes `entry_id` and, for children, `child_index`; Tauri resolves the path from the session, canonicalises it and checks it is inside the library root, exactly as `library/art.rs` `read_art_bytes` does. Unknown id → `UnknownEntry`; unknown child → new `UnknownChild`.
- **One ordering.** «Newest» is decided only in Rust with `dedupe::keeper::version_order(system, v)` (hex for `xbox360`), after stripping a leading `v`/`V`. Equal or unparseable versions → no newest, no warning. `src/lib/versions.ts` sorts for display only and says so in its doc comment.
- **Counting.** Juegos = games that are not orphans; demos count. Updates, DLC, non-default revisions and orphans never count. Status bar copy is spec §9 word for word.
- **Installed users first (D19).** A plain retro entry serialises exactly as today (all new `EntryView` fields `skip_serializing_if`). New `viewPrefs` fields go through `sanitisePrefs` with defaults; new Details columns are inserted once (`contentColumns`). Settings: new field `#[serde(default)]`, no `SETTINGS_VERSION` bump. Libraries with no children, revisions or demos look as today except «juegos» for «ROMs».
- **Nothing overwritten, everything undoable.** Child art goes through the existing writer (`write::ensure_within`, journal, backup of an existing file, undo). An existing slot file is never overwritten without the usual backup; a case-insensitive collision is skipped as «exists».
- **TDD.** Write the failing test, run it and watch it fail for the expected reason, implement, watch it pass. Each «Run» step names the exact command.
- **Gates (same as CI).** Before every commit that touches code or tests:
  - **G-RUST:** `cargo fmt --all --check && cargo clippy --workspace --all-targets --all-features -- -D warnings && scripts/dev/clippy-windows.sh && cargo test --workspace --no-fail-fast`
  - **G-WEB** (any task touching `src/`, `locales/`, `design/` or `src-tauri/`): `pnpm check-all && pnpm build`
  - Docs-only commits run `pnpm check-all`.
- **Windows CI leg.** Fixture mtimes pinned (`coverly_core::dedupe::testing::pin_mtime` or `filetime::set_file_mtime` to a fixed time, never «now»). Paths compared by component (`Path::components`, `file_name`, `Path::join`), never by slash text; wire `rel_path` strings are built with the existing `rel_string`. Child slot path length is checked in UTF-16 units against `WINDOWS_MAX_PATH` on **every** OS. No fixture name is reserved (`CON`, `AUX`…) or ends in a dot or space.
- **Real data is read-only.** Never write to `~/Documents/ROMS`, `/Volumes/TinyExt/*` or `/Volumes/System`; only `ls`, `stat`, the app opened read-only and Coverly **plan-only** runs may touch them. Write tests only on copies made with `cp -RXp` under `/Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-views/target/addon-sample/`; `find target/addon-sample -name '._*' | wc -l` must print `0`; delete the folder afterwards.
- **Fixtures are synthetic** (builders in `consoles/testing.rs`; no copyrighted data, no keys; real game names only as file names over synthetic bytes). The mock library uses the mockup's invented games (Lantern Harbor, Tidewalker, Neon Rhythm…).
- **No UI strings in the core.** Notes, verdicts and kinds are enums plus data; UI and CLI render them. Every UI string lives in `locales/en/ui.json` **and** `locales/es/ui.json` with the same keys and placeholders (parity test). Copy is spec §9 word for word (Spanish: sentence case, no exclamation marks, exact counts; numbers, versions, ids and paths in mono by the renderer; numbers through `Intl.NumberFormat`, lists through `Intl.ListFormat`).
- **Brand.** Only existing tokens from `design/tokens.json` (`pnpm tokens:check` green, **no new token**), `src/components/ui` primitives, `SystemIcon` for system marks. Warning tone only through the semantic `--cr-warning` / `--cr-warning-bg` and always with «↑» and a word; never `--cr-warning-11`. Manila only for «Nuevo». New CSS only as component classes in `src/styles/globals.css` built on `--cr-*` tokens.
- **Accessibility.** Every count and state also in text (`sr-only` phrase from `spokenSummary`), never colour or icon only; visible chips/dots/arrows `aria-hidden`; visible label text contained in the accessible name; keyboard in the treegrid (APG); motion only per D17 and zeroed under reduced motion; forced-colours fallbacks.
- **Secrets are never read or printed:** the minisign key, `~/.coverly-release/`, any `.env*`, `.vercel/`.
- **Servers and E2E.** The web UI runs on port **1472** only: `pnpm dev --port 1472 --strictPort` (mock backend). Stop it by port only: `lsof -ti tcp:1472 -sTCP:LISTEN | xargs -r kill`. E2E uses Chrome DevTools MCP `new_page` with `isolatedContext: "addon-views"`; fall back to Playwright MCP; if both profiles are locked, install `playwright-core` in `/private/tmp/claude-501/-Users-miguelangelesparzacalero-Documents-GitHub/35b7c020-f2ae-412f-b757-99c24a9a498b/scratchpad/addon-views` and drive Chrome for Testing from there.
- **Owner decisions never block.** Every §14 question (Q1–Q11) is implemented with its safe default. Anything new that needs Miguel is implemented around, recorded as a `NEEDS-OWNER: <topic> — <default taken>` line in the task's commit body and collected into the PR (Task 28).

## Deviations from the spec (decided in this plan)

- **P-1: the unknown region letter travels on `ConsoleId`.** `ConsoleId` gains `region_letter: Option<RegionLetterFlag { letter: char, code: String }>` (`#[serde(default, skip_serializing_if = "Option::is_none")]`), set by `ctr.rs` only when `ctr_region` answers `Unknown`. `organize/plan.rs` turns it into `ItemNote::RegionLetterUnknown { letter }` on the base or demo item. No other system sets it.
- **P-2: scan links in their own module.** The demo/orphan post-pass is a pure `scan/links.rs` (`link_titles(&mut [RomEntry])`), called once per system at the end of `scan_systems`, so `scan/mod.rs` gains only the call and the new fields (`RomEntry.is_demo: bool`, `RomEntry.demo_of: Option<usize>`, `RomEntry.orphan_of: Option<usize>`, both indexes into the same system's `roms`, and `RomEntry.fixed_counts: Option<FixedFolderCounts>`; every `RomEntry { .. }` literal in the workspace, tests included, gains the defaults). Tauri turns indexes into entry ids.
- **P-3: version facts live beside the seam.** `catalog/latest.rs` holds both the 3.1 seam and the pure in-library functions `installed_version(system, base_version, children)` and `newer_elsewhere(system, installed, orphans)`, so «newest» has one home. `ChildView.version` is parsed from the child's name by `catalog::latest::version_in_name` (`(v1.05)`, `[v131072]`, `v1.2.0` at the end), the same reader for children and bases.
- **P-4: mock library pick.** `?mockPick=addons` resolves to `MOCK_ADDONS = '/Users/demo/Addons'`; `mockBackend.openLibrary` serves `addonViewsLibrary(gen)` for that path (not an Organizar pile), so the six views can be exercised without running Organizar. A 10 000-game variant `?mockPick=addons-huge` serves the D1 performance check of the E2E.
- **P-5: game panel split in two tasks.** Head + Versión + Revisiones (Task 25) and DLC + Demo + folders + child art actions (Task 26), each independently testable.
- **P-6: «Nuevo» is the last UI piece.** `src/lib/seen.ts` lands with the model (Task 13) but the tag is rendered only where each view task lists it; if its storage throws, nothing is tagged.

## Review Focus

1. **Counting.** One game = 1 everywhere: status bar, sidebar counts, filter counts, «sin portada», select-all in Detalles. Revisions, updates, DLC and orphans never counted; demos counted and said. (`games.test.ts`, `StatusBar.test.tsx`, Task 16.)
2. **One ordering.** No TypeScript decides «newest»; `installed`/`newer_elsewhere` from Rust with `version_order` (hex on `xbox360`, `1.05` = `1.5` → none). (Task 6.)
3. **3DS table.** Every row of spec §5 tested; `T`/`U` flagged unknown, `C`/`Y`/`Z` panel-only, country letters write the country; Wii U/GameCube/Wii unchanged; behaviour changes listed in the PR. Citations in the code comment. (Tasks 1–3.)
4. **Keys and ids.** No child key file opened; no new command takes a path; `read_child_art_bytes` refuses an unknown index and a symlink escaping the root. (Tasks 9, 11.)
5. **Installed users.** A plain retro `EntryView` snapshot unchanged; saved prefs keep their columns plus the two new ones once; settings round-trip both ways. (Tasks 7, 10, 14.)
6. **Accessibility.** Treegrid roles and keys, accessible names with full phrases, no colour-only state, reduced motion, forced colours. (Tasks 18, 20, 28.)
7. **Windows.** Pinned mtimes, component comparisons, child slot path length on every OS, `scripts/dev/clippy-windows.sh` on every Rust task.

---

## File structure

```
crates/coverly-core/
├─ src/consoles/
│  ├─ ctr_region.rs (new)    ctr_region(code) -> RegionVerdict { letter, region, country, certainty } (§5, cited)
│  ├─ ctr.rs                 base_region uses ctr_region for Base and Demo; sets ConsoleId.region_letter (P-1)
│  └─ mod.rs                 pub mod ctr_region; ConsoleId.region_letter; region_from_product_code keeps WUP-/disc only
├─ src/identify/names.rs     Region::Russia (no_intro_word, from_no_intro, every exhaustive match)
├─ src/scan/
│  ├─ mod.rs                 RomEntry.{fixed_counts, is_demo, demo_of, orphan_of}; list_children skips art dirs; count_fixed()
│  └─ links.rs (new)         link_titles(): demos → full game, orphans → game, by title key, same system (P-2)
├─ src/catalog/latest.rs (new)  LatestVersions, LatestKnown, LatestSource, NoTitleList; version_in_name;
│                               installed_version; newer_elsewhere (D5, D6, P-3)
├─ src/settings/{model.rs,apply.rs}   LibraryGroup.revision_defaults keyed by root; set/prune/cap (D7)
├─ src/art/
│  ├─ children.rs (new)      ChildMedia, ChildOrigin, child_slot_path, ArtJson read/write, read_child_art (D10, D11)
│  └─ mod.rs                 claims `<stem>-icon`/`<stem>-banner` of children and orphans (D10 b)
├─ src/sources/embedded.rs   pub(crate) child_locators(path) + load_png(path, locator) for a child path (D11)
├─ src/profiles/mod.rs       Profile.child_art (default false, skipped when false) (D16)
└─ src/organize/{plan.rs,apply.rs,target.rs}
                             ItemNote::RegionLetterUnknown; FACTS_VERSION 12; child slots move with their child;
                             apply hook reads child art for coverly-library; no child art to a profile without child_art
crates/coverly-cli/src/organize.rs   prints RegionLetterUnknown
src-tauri/src/library/
├─ view.rs                   EntryView optional fields; ChildView; ChildArtMarks; FolderCountsView; DemoView; NewerView
├─ mod.rs                    builds them; revisionDefaults in LibraryView
├─ details.rs (new)          game_details (offline, facts cache, request id), GameDetailsView, RegionVerdictView
├─ art.rs                    read_child_art_bytes (session-resolved, canonicalised, MAX_ART_BYTES)
└─ (ipc.rs / lib.rs)         register set_default_revision, game_details, read_child_art, read_child_art_bytes
src/ipc/{types.ts,backend.ts,tauriBackend.ts,contract.sample.json,contract.requests.json,contract.test.ts,dedupeTypes.ts}
src/ipc/mock/{addonViewsFixture.ts (new),mockBackend.ts,addonViewsFixture.test.ts (new)}
src/lib/
├─ games.ts (new)            Game, buildGames, gameCounts, defaultRevision
├─ versions.ts (new)         compareVersions (display order only)
├─ addonSummary.ts (new)     summaryParts, spokenSummary, contentCell, versionCell
├─ seen.ts (new)             «Nuevo» seen set per library root
├─ scope.ts, search.ts, sort.ts, filters.ts, fill.ts, viewPrefs.ts, detailsColumns.ts, coverSizes.ts,
│  rowModel.ts, shelves.ts, folderTree.ts, dedupe.ts, tileLabel.ts
src/stores/{useVisibleEntries.ts,filterStore.ts,viewStore.ts,libraryStore.ts,selectionStore.ts}
src/components/library/
├─ StatusBar.tsx, Sidebar.tsx, OrphansList.tsx (new), ContentFilters.tsx (new), DetailDrawer.tsx
├─ game/{VersionSection,RevisionsSection,DlcSection,DemoSection,FixedFoldersSection,ChildThumb,RegionLine}.tsx (new)
└─ views/{CoverTile,CoversView,GalleryView,DetailsView,ColumnsView,MountsView,ShelvesView,useViewKeys}.tsx
src/components/organize/SystemGroups.tsx       child thumbnails (D15)
src/styles/globals.css                         component classes on --cr-* tokens
locales/{es,en}/ui.json, CHANGELOG.md
```

---

## Task 1: `Region::Russia` across Rust and TypeScript (D12 prerequisite)

**Files:**
- Modify: `crates/coverly-core/src/identify/names.rs` (`Region::Russia`; `from_no_intro("russia")`; `no_intro_word` → `Russia`; every other exhaustive `match` on `Region` in the workspace — find them with `cargo build` errors and `grep -rn "Region::UnitedKingdom" crates src-tauri`), `src/ipc/types.ts` (`Region` union + `REGIONS`), `src/ipc/dedupeTypes.ts`, `src/lib/dedupe.ts` and `src/lib/tileLabel.ts` (short code `RU`), `locales/{es,en}/ui.json` (`regions.russia`: `Rusia` / `Russia`)
- Test: `names.rs` unit tests, `src/lib/tileLabel.test.ts`, `src/lib/dedupe.test.ts`, the i18n parity test

- [ ] **Step 1: Write the failing tests.**

```rust
// identify/names.rs, mod tests
#[test]
fn russia_is_a_no_intro_region() {
    assert_eq!(Region::from_no_intro("Russia"), Some(Region::Russia));
    assert_eq!(Region::Russia.no_intro_word(), "Russia");
    assert_eq!(serde_json::to_string(&Region::Russia).unwrap(), "\"russia\"");
    let tags = parse_tags("Pocket Card Jockey (Russia)");
    assert_eq!(tags.regions, vec![Region::Russia]);
    assert_eq!(tags.title_key, parse_tags("Pocket Card Jockey").title_key);
}
```

```ts
// src/lib/tileLabel.test.ts
it('gives Russia the RU short code', () => {
  expect(REGION_CODE.russia).toBe('RU');
});
```

- [ ] **Step 2: Run** `cargo test -p coverly-core --lib identify::names::tests::russia_is_a_no_intro_region` and `pnpm vitest run src/lib/tileLabel.test.ts` — fail (no variant / no code).
- [ ] **Step 3: Implement** the variant (after `Netherlands`, keep alphabetical order where the enum is alphabetical), every match arm, the TS unions, the short codes and both locale keys (merge key by key).
- [ ] **Step 4: Run** G-RUST and G-WEB.
- [ ] **Step 5: Commit + push.** `git commit -m "feat(core): add Russia as a No-Intro region" && git push && git status -sb`.

---

## Task 2: Verified 3DS region table `ctr_region` (D12, §5, pure, test-first)

**Files:**
- Create: `crates/coverly-core/src/consoles/ctr_region.rs`
- Modify: `crates/coverly-core/src/consoles/mod.rs` (`pub mod ctr_region;` and re-export `RegionVerdict`, `Certainty`)
- Test: unit tests in `ctr_region.rs`

**Interfaces (produced):**

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Certainty { Certain, Likely, Unknown }

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegionVerdict {
    pub letter: char,
    pub code: String,               // the product code as read, trimmed
    pub region: Option<Region>,
    pub country: Option<Region>,
    pub certainty: Certainty,
}
impl RegionVerdict {
    /// What a name or `ConsoleId.region` may use: `country.or(region)` when Certain.
    pub fn for_name(&self) -> Option<Region>;
}
/// `None` for a malformed code or the generic `CTR-P-CTAP`.
pub fn ctr_region(code: &str) -> Option<RegionVerdict>;
```

- [ ] **Step 1: Create the file with the module comment, the types and `todo!()` bodies**, then the failing tests:

```rust
//! 3DS region (and country) from the last letter of a product code's
//! 4-character game code (`CTR-P-AXXE`, `KTR-N-AB2P`), spec part C §5.
//!
//! Verified 2026-11-09 against:
//! - 3dbrew, «Serials»: https://www.3dbrew.org/wiki/Serials
//!   (E English/US, P PAL, J Japan, K Korea, C China/Taiwan, W Taiwan/Hong
//!   Kong, Y and Z multiple regions, A region-free).
//! - GameTDB 3DS database, https://www.gametdb.com/3dstdb.zip (snapshot
//!   2026-11-09): last letter vs `<region>` of every 4-character id
//!   (E 1304 NTSC-U, P 1302 PAL, J NTSC-J, K NTSC-K, W 29 NTSC-T, A 68 none,
//!   D 33 PAL, F 83 PAL, S 10 PAL, I 11 PAL, H 2 PAL (Dutch), R 1 PAL-R,
//!   V 6 PAL, X 14 PAL, Y 32/33 PAL, Z 170/180 PAL, C 3 untagged; no id ends
//!   in T, U, B, G, L, M, N, O or Q).
//! - WiiBrew, «Title database», region codes:
//!   https://wiibrew.org/wiki/Title_database (D German-, F French-,
//!   I Italian-, S Spanish-speaking; T Korea with English; W Taiwan/HK).
//!
//! A letter not attested for 3DS is `Unknown` and flagged, never guessed.

#[cfg(test)]
mod tests {
    use super::*;
    use Certainty::*;
    use Region::*;

    #[test]
    fn every_row_of_the_verified_table() {
        let rows: &[(char, Option<Region>, Option<Region>, Certainty)] = &[
            ('E', Some(Usa), None, Certain),
            ('P', Some(Europe), None, Certain),
            ('J', Some(Japan), None, Certain),
            ('K', Some(Korea), None, Certain),
            ('W', Some(Taiwan), None, Certain),
            ('A', Some(World), None, Certain),
            ('D', Some(Europe), Some(Germany), Certain),
            ('F', Some(Europe), Some(France), Certain),
            ('S', Some(Europe), Some(Spain), Certain),
            ('I', Some(Europe), Some(Italy), Certain),
            ('H', Some(Europe), Some(Netherlands), Certain),
            ('R', Some(Europe), Some(Russia), Certain),
            ('V', Some(Europe), None, Certain),
            ('X', Some(Europe), None, Certain),
            ('Y', Some(Europe), None, Likely),
            ('Z', Some(Europe), None, Likely),
            ('C', Some(China), None, Likely),
        ];
        for &(letter, region, country, certainty) in rows {
            let code = format!("CTR-P-AXX{letter}");
            let v = ctr_region(&code).unwrap();
            assert_eq!((v.letter, v.region, v.country, v.certainty), (letter, region, country, certainty), "{code}");
        }
    }

    #[test]
    fn letters_not_attested_for_3ds_are_unknown() {
        for letter in ['T', 'U', 'B', 'G', 'L', 'M', 'N', 'O', 'Q', '1'] {
            let v = ctr_region(&format!("CTR-P-AXX{letter}")).unwrap();
            assert_eq!((v.region, v.country, v.certainty), (None, None, Unknown), "{letter}");
        }
    }

    #[test]
    fn shapes() {
        assert_eq!(ctr_region("KTR-N-AB2S").unwrap().country, Some(Spain));
        assert_eq!(ctr_region(" ctr-p-axxe ").unwrap().region, Some(Usa));
        assert_eq!(ctr_region("CTR-T-AXXP").unwrap().region, Some(Europe)); // eShop demo
        assert_eq!(ctr_region("CTR-P-CTAP"), None); // generic default of secondary NCCHs
        for bad in ["", "CTR-P-", "CTR-P-AXX", "WUP-P-AXXE", "CTR-PAXXE", "CTR-P-AXXE-X"] {
            assert_eq!(ctr_region(bad), None, "{bad}");
        }
    }

    #[test]
    fn only_certain_verdicts_name_a_region_and_the_country_wins() {
        assert_eq!(ctr_region("CTR-P-AXXS").unwrap().for_name(), Some(Spain));
        assert_eq!(ctr_region("CTR-P-AXXP").unwrap().for_name(), Some(Europe));
        assert_eq!(ctr_region("CTR-P-AXXZ").unwrap().for_name(), None);
        assert_eq!(ctr_region("CTR-P-AXXC").unwrap().for_name(), None);
        assert_eq!(ctr_region("CTR-P-AXXT").unwrap().for_name(), None);
    }
}
```

- [ ] **Step 2: Run** `cargo test -p coverly-core --lib consoles::ctr_region` — panics at `todo!()`.
- [ ] **Step 3: Implement.**

```rust
pub fn ctr_region(code: &str) -> Option<RegionVerdict> {
    let code = code.trim().to_ascii_uppercase();
    let b = code.as_bytes();
    // [CK]TR-<category>-<4-character game code>
    let ok = b.len() == 10
        && matches!(&b[..4], b"CTR-" | b"KTR-")
        && b[4].is_ascii_alphabetic()
        && b[5] == b'-'
        && b[6..].iter().all(u8::is_ascii_alphanumeric);
    if !ok || code == "CTR-P-CTAP" {
        return None;
    }
    let letter = b[9] as char;
    use {Certainty::*, Region::*};
    let (region, country, certainty) = match letter {
        'E' => (Some(Usa), None, Certain),
        'P' => (Some(Europe), None, Certain),
        'J' => (Some(Japan), None, Certain),
        'K' => (Some(Korea), None, Certain),
        'W' => (Some(Taiwan), None, Certain),
        'A' => (Some(World), None, Certain),
        'D' => (Some(Europe), Some(Germany), Certain),
        'F' => (Some(Europe), Some(France), Certain),
        'S' => (Some(Europe), Some(Spain), Certain),
        'I' => (Some(Europe), Some(Italy), Certain),
        'H' => (Some(Europe), Some(Netherlands), Certain),
        'R' => (Some(Europe), Some(Russia), Certain),
        'V' | 'X' => (Some(Europe), None, Certain),
        'Y' | 'Z' => (Some(Europe), None, Likely),
        'C' => (Some(China), None, Likely),
        _ => (None, None, Unknown),
    };
    Some(RegionVerdict { letter, code, region, country, certainty })
}

impl RegionVerdict {
    pub fn for_name(&self) -> Option<Region> {
        (self.certainty == Certainty::Certain).then(|| self.country.or(self.region)).flatten()
    }
}
```

- [ ] **Step 4: Run** the tests, then G-RUST.
- [ ] **Step 5: Commit + push.** `git commit -m "feat(core): 3DS region and country by product code, verified table"` (body: `NEEDS-OWNER: Q1 — T and U flagged unknown, C/Y/Z panel only; Q2 — country letter writes the country into names`) `&& git push && git status -sb`.

---

## Task 3: Use the verdict for 3DS bases and demos, names and the unknown-letter note (D12, P-1)

**Files:**
- Modify: `crates/coverly-core/src/consoles/mod.rs` (`ConsoleId.region_letter: Option<RegionLetterFlag>`; split test `region_from_product_codes_and_game_ids`: its `CTR-` cases move to `ctr_region`, `WUP-`/disc cases stay and keep today's table), `crates/coverly-core/src/consoles/ctr.rs` (`base_region`: `TitleKind::Base | TitleKind::Demo` → `ctr_region(serial)`; `id.region = verdict.for_name()`; `Unknown` → `id.region_letter = Some(RegionLetterFlag { letter, code })`), `crates/coverly-core/src/organize/plan.rs` (`ItemNote::RegionLetterUnknown { letter: char }` on the item whose id has `region_letter`; `FACTS_VERSION` 11 → 12 and its pin test), `crates/coverly-cli/src/organize.rs` (text «Region letter “T” not identified; no region is added to the name.»), `src/ipc/types.ts` (`ItemNote` variant), `src/lib/organize.ts` (`noteKey`), `locales/{es,en}/ui.json` (`organize.notes.regionLetterUnknown`, spec §9)
- Test: `ctr.rs` unit tests, `crates/coverly-core/tests/organize_game_folders.rs` (or the existing 3DS organise test file), `dedupe/keeper.rs` tests, CLI unit test, `src/lib/organize.test.ts`

- [ ] **Step 1: Write the failing tests.**
  - `ctr.rs`: `ncsd("CTR-P-AXXS", 0x0004000000055D00, true)` → `region == Some(Spain)`, `region_letter == None`; `"CTR-P-AXXZ"` → `region == None`, no flag; `"CTR-P-AXXT"` → `region == None`, `region_letter == Some({ letter: 'T', code: "CTR-P-AXXT" })`; a demo title id (`0x0004000200055D00`) with `"CTR-T-AXXP"` → `Some(Europe)`; an update (`0x0004000E…`) → `None` whatever its code. Use `consoles::testing::{ncsd, cia_with_code}`.
  - `consoles/mod.rs`: `region_from_product_code("WUP-P-AXXU") == Some(Australia)` and `"AXXC01"` → `Some(China)` (Wii U/disc unchanged).
  - Organise (coverly-library, game folders on): a 3DS base `Pokemon Y.3ds` with `CTR-P-EK2S` → folder `Pokemon Y (Spain)/`; with `CTR-P-EK2T` → folder `Pokemon Y/` and the item has `RegionLetterUnknown { letter: 'T' }`; `Pokemon Y (Europe).3ds` with `S` keeps `(Europe)` (name already has a region); `C` and `U` no longer add `(China)`/`(Australia)`.
  - `dedupe/keeper.rs`: a 3DS pair whose only difference is letter `U` vs `E` no longer prefers by an `Australia` region (keeper sees `None`).
  - `FACTS_VERSION` pin: `assert_eq!(FACTS_VERSION, 12)`.
  - CLI: rendering a plan with the note prints the English sentence; TS `noteKey({kind:'region-letter-unknown', letter:'T'})` → `organize.notes.regionLetterUnknown` with `{letter:'T'}`.
- [ ] **Step 2: Run** `cargo test -p coverly-core consoles:: organize_game_folders dedupe::keeper && cargo test -p coverly-cli && pnpm vitest run src/lib/organize.test.ts` — fail.
- [ ] **Step 3: Implement.** `RegionLetterFlag { letter: char, code: String }` lives in `consoles/mod.rs`; `ctr.rs` calls `ctr_region::ctr_region`. Re-run `cargo insta test --review` style checks on existing snapshots: only 3DS rows with letters `C`, `U`, `R`, `V`, `X`, `T` may change; record each changed snapshot in the commit body.
- [ ] **Step 4: Run** G-RUST and G-WEB.
- [ ] **Step 5: Commit + push.** `git commit -m "feat(core): name 3DS games by the verified region letter and flag unknown ones"` (body: `NEEDS-OWNER: Q10 — C/U stop adding China/Australia, R/V/X start adding Russia/Europe; re-plans may propose them; listed in the PR`).

---

## Task 4: Fixed-folder counts and art dirs skipped under `Updates/` and `DLC/` (D9, D10 a)

**Files:**
- Modify: `crates/coverly-core/src/scan/mod.rs` (`pub struct FixedFolderCounts { pub mods: u32, pub cheats: u32, pub extras: u32, pub saves: u32, pub capped: bool }`; `RomEntry.fixed_counts: Option<FixedFolderCounts>` set for a base at a game folder root, `None` elsewhere; `fn count_fixed(dir, opts) -> (u32, bool)`; `list_children` filter adds `is_skipped_dir(&name, &profile.art_dir_names, opts)` for directories — thread `art_dir_names` into `list_children`)
- Test: `scan/mod.rs` unit tests or `crates/coverly-core/tests/scan_game_folders.rs` (use the existing game-folder scan test file)

- [ ] **Step 1: Write the failing tests** (temp dirs, mtimes pinned):
  - `fixed_folder_counts_are_metadata_only`: `Game/Game.pkg`, `Game/Mods/a.bps`, `Game/Mods/pack/x.png`, `Game/Cheats/c.cht`, `Game/Extras/Manuals/m.pdf`, `Game/Saves/s.sav`, plus `Game/Mods/._a.bps`, `Game/Mods/.DS_Store`, `Game/Mods/Thumbs.db`, `Game/Mods/desktop.ini` → `{ mods: 2, cheats: 1, extras: 1, saves: 1, capped: false }`; the counted files are never opened (`consoles::start_open_log()` empty for them).
  - `counting_stops_at_the_cap`: a `CAP` override of 5 via a `#[cfg(test)]` const (`const FIXED_CAP: u32 = if cfg!(test) { 5 } else { 10_000 }`) with 7 files → `mods: 5, capped: true`.
  - `depth_and_symlinks`: a file at depth 5 not counted; `#[cfg(unix)]` a symlinked folder inside `Mods/` not followed even with `follow_symlinks: true`.
  - `a_media_dir_under_dlc_is_not_a_child_even_with_hidden_on`: `Game/DLC/.media/x-icon.png` and `Game/DLC/Pack.pkg` with `include_hidden: true` → `children` has only `Pack.pkg`.
  - A loose ROM and a base without a game folder → `fixed_counts == None`.
- [ ] **Step 2: Run** `cargo test -p coverly-core scan` — fail.
- [ ] **Step 3: Implement** `count_fixed` with `WalkDir::new(dir).max_depth(4).follow_links(false)`, `filter_entry` skipping `is_skipped_entry` names, counting regular files, stopping at the cap.
- [ ] **Step 4: Run** G-RUST.
- [ ] **Step 5: Commit + push.** `git commit -m "feat(core): count a game's fixed folders and skip art dirs among its children"`.

---

## Task 5: Link demos and orphan add-ons to their game in the scan (D6, D8, P-2, pure)

**Files:**
- Create: `crates/coverly-core/src/scan/links.rs`
- Modify: `crates/coverly-core/src/scan/mod.rs` (`mod links;`, `RomEntry.demo_of: Option<usize>`, `RomEntry.orphan_of: Option<usize>`, `RomEntry.is_demo: bool`; call `links::link_titles(&mut system.roms)` once per system after the walk)
- Test: unit tests in `links.rs`

**Interfaces (produced):**

```rust
/// Sets `is_demo`, `demo_of` and `orphan_of` on one system's entries.
/// Indexes point into the same slice. Never across systems.
pub(crate) fn link_titles(roms: &mut [RomEntry]);
```

Rules (spec D6, D8, §11): a demo is an entry with `organize::library_bases::has_demo_tag(stem)`; its key is `demo_free_key(stem)`; candidates are non-demo, non-orphan entries whose `parse_tags(stem).title_key` equals it; one candidate → linked; several → `pick_by_region(cands, demo_region, …)` only when a candidate has exactly the demo's region tag, else `None`. An orphan (`role.is_some()`) links by the title key of its **game folder name** (the parent of its `Updates/`/`DLC/` folder; if it is loose, its own stem) to exactly one non-orphan, non-demo entry; zero or several → `None`.

- [ ] **Step 1: Write the failing tests** with a `fn rom(stem, rel_dir, role)` builder (pinned `modified`, `path` built by `Path::join`):
  - `a_demo_links_to_its_full_game`: `Lantern Harbor (Europe)`, `Lantern Harbor (Europe) (Demo)` → demo `is_demo`, `demo_of == Some(0)`.
  - `a_demo_without_its_game_has_no_link`: `Paper Skies (Demo)` alone → `is_demo`, `demo_of == None`.
  - `two_regions_pick_the_same_region_or_none`: `X (USA)`, `X (Japan)`, `X (Japan) (Demo)` → `Some(1)`; `X (USA)`, `X (Japan)`, `X (Demo)` → `None`.
  - `an_orphan_update_links_by_its_game_folder_name`: `Tidewalker/Tidewalker.nsp` (with its own child), orphan `Tidewalker Old/Updates/Tidewalker (v1.3.0).nsp` whose folder name key does not match → none; orphan `Downloads/Tidewalker/Updates/Tidewalker (v1.3.0).nsp` (folder key `tidewalker`) → `orphan_of == Some(0)`.
  - `two_candidate_games_link_nothing`; `a_demo_is_never_an_orphan_target`.
- [ ] **Step 2: Run** `cargo test -p coverly-core --lib scan::links` — fail.
- [ ] **Step 3: Implement** (one `HashMap<String, Vec<usize>>` of title keys built once: O(n)).
- [ ] **Step 4: Run** G-RUST.
- [ ] **Step 5: Commit + push.** `git commit -m "feat(core): link demos and orphan add-ons to their game in the scan"`.

---

## Task 6: Latest-version seam and in-library version facts (D5, D6, P-3, pure)

**Files:**
- Create: `crates/coverly-core/src/catalog/latest.rs`
- Modify: `crates/coverly-core/src/catalog/mod.rs` (`pub mod latest;`)
- Test: unit tests in `latest.rs`

**Interfaces (produced):**

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LatestSource { TitleDb, GameTdb }
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LatestKnown { pub version: String, pub source: LatestSource }
/// The 3.1 seam (spec §10): an offline title list that knows the newest version.
pub trait LatestVersions: Send + Sync {
    fn latest(&self, system: &str, base_title_id: &str) -> Option<LatestKnown>;
}
/// The only implementation in part C: nothing is known.
pub struct NoTitleList;

/// `(v1.05)`, `[v131072]`, `v1.2.0` at the end of a name → `1.05`, `131072`, `1.2.0`.
pub fn version_in_name(name: &str) -> Option<String>;
/// Strict newest of `versions` by `version_order`; `None` on a tie or any unparseable one among the top.
pub fn newest<'a>(system: &str, versions: impl IntoIterator<Item = &'a str>) -> Option<&'a str>;
/// D5: the highest update version, else the base's own version, else none.
pub fn installed_version(system: &str, base: Option<&str>, updates: &[&str]) -> Option<String>;
/// D6 in-library: the newest orphan update linked to this game that is strictly newer than `installed`.
pub fn newer_elsewhere<'a>(system: &str, installed: Option<&str>, orphans: &[(usize, &'a str)]) -> Option<(usize, &'a str)>;
```

- [ ] **Step 1: Write the failing tests:**

```rust
#[test]
fn versions_in_names() {
    assert_eq!(version_in_name("Tidewalker (v1.0.2)").as_deref(), Some("1.0.2"));
    assert_eq!(version_in_name("Neon Rhythm [v131072]").as_deref(), Some("131072"));
    assert_eq!(version_in_name("Game v1.05").as_deref(), Some("1.05"));
    assert_eq!(version_in_name("Game (Europe)"), None);
    assert_eq!(version_in_name("Game (Rev 1)"), None); // revisions are not versions
}

#[test]
fn installed_is_the_newest_by_version_order() {
    assert_eq!(installed_version("switch", None, &["1.0.2", "1.0.10"]).as_deref(), Some("1.0.10"));
    assert_eq!(installed_version("ps4", Some("1.00"), &[]).as_deref(), Some("1.00"));
    assert_eq!(installed_version("ps4", None, &[]), None);
    // hex on Xbox 360
    assert_eq!(installed_version("xbox360", None, &["9", "A"]).as_deref(), Some("A"));
    // 1.05 == 1.5 → no newest, so nothing is claimed
    assert_eq!(installed_version("ps4", None, &["1.05", "1.5"]), None);
    // an unparseable one among the top → none
    assert_eq!(installed_version("ps4", None, &["1.2", "beta"]), None);
}

#[test]
fn newer_elsewhere_only_when_strictly_newer() {
    assert_eq!(newer_elsewhere("switch", Some("1.0.2"), &[(7, "1.3.0")]), Some((7, "1.3.0")));
    assert_eq!(newer_elsewhere("switch", Some("1.3.0"), &[(7, "1.3.0")]), None);
    assert_eq!(newer_elsewhere("switch", None, &[(7, "1.3.0")]), Some((7, "1.3.0")));
    assert_eq!(newer_elsewhere("switch", Some("1.0"), &[(7, "x")]), None);
}

#[test]
fn no_title_list_knows_nothing() {
    assert_eq!(NoTitleList.latest("switch", "0100000000010000"), None);
}
```

- [ ] **Step 2: Run** `cargo test -p coverly-core --lib catalog::latest` — fail.
- [ ] **Step 3: Implement** with `crate::dedupe::keeper::version_order` after `trim_start_matches(['v', 'V'])`.
- [ ] **Step 4: Run** G-RUST.
- [ ] **Step 5: Commit + push.** `git commit -m "feat(core): latest-version seam with an in-library default"` (body: `NEEDS-OWNER: Q6 — in-library comparison only until 3.1 has versions`).

---

## Task 7: Revision defaults in the library settings, keyed by root (D7)

**Files:**
- Modify: `crates/coverly-core/src/settings/model.rs` (`LibraryGroup.revision_defaults: BTreeMap<String, BTreeMap<String, String>>`, `#[serde(default, skip_serializing_if = "BTreeMap::is_empty")]`), `crates/coverly-core/src/settings/apply.rs` (`pub fn revision_default(&self, root: &Path, group: &str) -> Option<&str>`; `pub fn set_revision_default(&mut self, root: &Path, group: &str, rel_path: &str)`; `pub fn prune_revision_defaults(&mut self, root: &Path, live: &dyn Fn(&str, &str) -> bool)` — only that root's map; cap at 1 000 roots, dropping the ones whose `RecentLibrary.opened_at` is oldest, roots not in `recent` first)
- Test: `settings` unit tests and the existing settings round-trip test file

- [ ] **Step 1: Write the failing tests:**
  - `an_old_settings_file_loads_without_the_field` (the committed `SETTINGS_VERSION` 3 sample JSON) and `a_new_file_loads_in_a_struct_without_it` (deserialize into a local copy of the old `LibraryGroup` shape without `deny_unknown_fields`).
  - `set_and_read_by_root`: two roots, same group key → independent answers; `rel_path` compared by components (`"SFC/Star Courier (Rev 1).sfc"` set, `"SFC\\Star Courier (Rev 1).sfc"` read on Windows-style text → same, via a helper `same_rel(a, b)` that splits on both separators).
  - `prune_touches_only_the_open_library`: stale key in root A pruned when pruning A; B's map untouched.
  - `the_map_is_capped`: 1 001 roots → 1 000, the oldest `opened_at` gone.
  - `SETTINGS_VERSION` still 3.
- [ ] **Step 2: Run** `cargo test -p coverly-core settings` — fail.
- [ ] **Step 3: Implement.** Root keys use the same string form `RecentLibrary.path` stores.
- [ ] **Step 4: Run** G-RUST.
- [ ] **Step 5: Commit + push.** `git commit -m "feat(core): revision defaults in library settings"`.

---

## Task 8: Child art slots, origins and the art index claim (D10)

**Files:**
- Create: `crates/coverly-core/src/art/children.rs`
- Modify: `crates/coverly-core/src/art/mod.rs` (`pub mod children;`; the index claims `<stem>-icon` and `<stem>-banner` for every `RomEntry.children` stem and every orphan entry stem in the same `.media/`), `crates/coverly-core/src/profiles/mod.rs` (`Profile.child_art: bool`, `#[serde(default, skip_serializing_if = "std::ops::Not::not")]`; every `Profile { .. }` literal gains `child_art: false`)
- Test: unit tests in `children.rs`, `art/mod.rs` tests, profile serde snapshot unchanged

**Interfaces (produced):**

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ChildMedia { Icon, Banner }
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ChildOrigin { Package, Screenscraper, Steamgriddb, User }
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtJson {
    #[serde(default, skip_serializing_if = "Option::is_none")] pub icon: Option<OriginStamp>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub banner: Option<OriginStamp>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OriginStamp { pub origin: ChildOrigin, pub at: i64 }

/// `<dir of child>/.media/<stem>-icon.png`; `Err(PathTooLong)` over 259 UTF-16 units or a name over 255 bytes, on every OS.
pub fn child_slot_path(child: &Path, media: ChildMedia) -> Result<PathBuf, SlotError>;
pub fn art_json_path(child: &Path) -> PathBuf; // `<dir>/.media/<stem>.art.json`
pub fn read_art_json(child: &Path) -> ArtJson;  // missing or invalid → default
pub fn present(child: &Path) -> ChildArtMarks;  // { icon: bool, banner: bool, icon_origin, banner_origin }
```

- [ ] **Step 1: Write the failing tests:** slot paths for `Game/DLC/Pack.pkg` and `Game/Updates/Game (v1.2).pkg` compared by components; `.art.json` round trip; invalid JSON → default; a child whose slot path is 260 UTF-16 units (build the name with `"é".repeat(..)` so bytes ≠ units) → `PathTooLong`; a 256-byte file name → `PathTooLong`; two children `x.pkg` and `x.nsp` → same slot; `art::index` over `Game/DLC/.media/{Pack-icon.png,Pack-banner.png,Pack.art.json}` with `Game/DLC/Pack.pkg` as a child → no orphan image; the same with `Pack.pkg` as an **orphan** top-level entry → no orphan image; `Profile` JSON of every built-in profile byte-identical (snapshot unchanged).
- [ ] **Step 2: Run** `cargo test -p coverly-core art profiles` — fail.
- [ ] **Step 3: Implement.** `.art.json` is not an image, so the audit's image filter already ignores it; assert it.
- [ ] **Step 4: Run** G-RUST.
- [ ] **Step 5: Commit + push.** `git commit -m "feat(core): art slots for updates and DLC beside each child"`.

---

## Task 9: Read child art in the clear, journaled and undoable (D11)

**Files:**
- Modify: `crates/coverly-core/src/sources/embedded.rs` (`pub(crate) fn child_locators(path: &Path) -> Vec<ArtLocator>` reusing `locators`/`dir_art`; `pub(crate) fn child_png(path, locator) -> Result<Vec<u8>, ()>` reusing `load` + PNG encode under the existing 2 MiB/decoder limits; banner locators only for folder children: `PIC1.PNG`, `sce_sys/pic1.png`, Wii U `meta/bootTvTex.tga`), `crates/coverly-core/src/art/children.rs` (`pub fn read_child_art(children: &[&Path], writer: &mut Writer) -> ChildArtReport { written, skipped: Vec<(PathBuf, SkipWhy)> }`; `SkipWhy { KeyMaterial, NoArtInClear, Exists, PathTooLong, ReadOnly }`)
- Test: `crates/coverly-core/tests/child_art.rs` (new)

- [ ] **Step 1: Write the failing tests** (temp library, pinned mtimes):
  - `a_cia_dlc_with_a_meta_smdh_gets_its_icon`: `Game/DLC/Pack.cia` from `consoles::testing::cia(0x0004008C00055D00, false, true)` → `Game/DLC/.media/Pack-icon.png` is a PNG, `.art.json` says `package`.
  - `a_vita_zip_child_gets_icon0`: zip with `sce_sys/icon0.png` (built in the test) → icon written.
  - `a_folder_ps4_update_gets_icon_and_banner`: folder child with `sce_sys/icon0.png` and `sce_sys/pic1.png` → both.
  - `switch_children_get_nothing`: `.nsp` DLC → `NoArtInClear`, nothing written.
  - `keys_are_never_opened`: `Game/DLC/Pack.tik` passed as a child → `KeyMaterial`, under `start_open_log()` the path is absent from `take_open_log()`.
  - `an_existing_icon_is_kept`: pre-existing `Pack-icon.png` → backed up per the writer's rule (or `Exists` when the policy is keep), never silently overwritten.
  - `undo_removes_what_it_wrote`: run `write::undo` on the job → slot files and `.art.json` gone, pre-existing files restored, `.media/` removed only if it was created and is empty.
- [ ] **Step 2: Run** `cargo test -p coverly-core --test child_art` — fail.
- [ ] **Step 3: Implement.** Check `is_key_material(file_name)` before any open; writes through `Writer` with `ensure_within`.
- [ ] **Step 4: Run** G-RUST.
- [ ] **Step 5: Commit + push.** `git commit -m "feat(core): read the art updates and DLC carry in the clear"`.

---

## Task 10: Organizar keeps child art with its child and reads it for the library (D10 d, D11 a, D16)

**Files:**
- Modify: `crates/coverly-core/src/organize/plan.rs` and `organize/target.rs` (when a child is moved or renamed and both layouts are `coverly-library`-style game folders: its existing `.media/<stem>-icon.png`, `-banner.png`, `.art.json` get companion move ops to the new stem; when the target profile has `child_art == false` or no game folders, the slot files are listed in the plan as staying and never copied), `crates/coverly-core/src/organize/apply.rs` (after a child lands with `coverly-library`, call `art::children::read_child_art` for children whose slots are empty, in the same journal), `crates/coverly-core/src/write/undo_organize.rs` (undo order: child art, then children, then bases)
- Test: `crates/coverly-core/tests/organize_game_folders.rs`

- [ ] **Step 1: Write the failing tests:** renaming `DLC/Pack.cia` → `DLC/Game - Pack.cia` moves `Pack-icon.png`/`Pack.art.json` to the new stem in the same job; apply with `coverly-library` on a pile with a CIA DLC carrying a meta SMDH writes its icon; organising to ES-DE (`child_art == false`) copies no child art and lists it; undo of each restores the exact previous tree (compare file lists by components); `FACTS_VERSION` stays 12.
- [ ] **Step 2: Run** `cargo test -p coverly-core --test organize_game_folders` — fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** G-RUST; review snapshot diffs (only new companion rows allowed).
- [ ] **Step 5: Commit + push.** `git commit -m "feat(organize): child art moves with its child and is read for the library"`.

---

## Task 11: Send each game's content over the bridge (D2, D4, D5, D6)

**Files:**
- Modify: `src-tauri/src/library/view.rs` (new optional fields on `EntryView`, all `skip_serializing_if`; `ChildView`, `ChildArtMarks`, `FolderCountsView`, `DemoView`, `NewerView`; `LibraryView.revision_defaults: BTreeMap<String /* version_group */, String /* rel_path */>` for the open root), `src-tauri/src/library/mod.rs` (fill them: children sorted by Rust newest version first then name; `ChildView.version` via `version_in_name`; `installed` and `newer_elsewhere` together via `catalog::latest::version_facts` (own updates' versions, `None` for one whose name carries none, plus the orphans the scan's `orphan_of` links to this game; never `installed_version` then `newer_elsewhere`, which would warn on a game in doubt); `orphan`, `orphan_of`, `demo { of, of_name }`, `version_group`, `revision`, `version`, `addons`), `src/ipc/types.ts` (mirrors), `src/ipc/contract.sample.json` (one game with every field, one orphan, one demo, one plain retro entry unchanged), `src-tauri/tests/contract.rs`, `src/ipc/contract.test.ts`
- Test: those files plus a unit test in `library/mod.rs`

**Wire (produced):**

```ts
export interface ChildView {
  index: number; rel_path: string; name: string;
  role: 'update' | 'dlc'; version?: string; size: number; mtime: number;
  art: { icon: boolean; banner: boolean; icon_origin?: ChildOrigin; banner_origin?: ChildOrigin };
}
export type ChildOrigin = 'package' | 'screenscraper' | 'steamgriddb' | 'user';
export interface EntryView {
  /* existing fields unchanged */
  children?: ChildView[];
  orphan?: 'update' | 'dlc';
  orphan_of?: number;
  version_group?: string;
  revision?: string;
  demo?: { of?: number; of_name?: string };
  version?: string;
  installed?: string;
  newer_elsewhere?: { entry: number; version: string };
  addons?: { mods: number; cheats: number; extras: number; saves: number; capped?: true };
}
```

- [ ] **Step 1: Write the failing tests:** `a_plain_retro_entry_serialises_as_before` (serde JSON equals today's literal); a game with two updates `1.0.10`, `1.0.2` lists `1.0.10` first and `installed == "1.0.10"`; a game with `1.05`/`1.5` has no `installed`; an orphan with `orphan_of` and the game's `newer_elsewhere`; a demo with `demo.of`; contract round trip in Rust and TS.
- [ ] **Step 2: Run** `cargo test -p coverly-roms && pnpm vitest run src/ipc` — fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** G-RUST and G-WEB.
- [ ] **Step 5: Commit + push.** `git commit -m "feat(tauri): send each game's content with the library"`.

---

## Task 12: Commands `game_details`, `set_default_revision`, `read_child_art`, `read_child_art_bytes` (D3, D7, D11)

**Files:**
- Create: `src-tauri/src/library/details.rs` (`GameDetailsView { title_id?, product_code?, region?: RegionVerdictView, latest?: LatestView, children: [{ index, title_id?, kind? }] }`; reads the base's and children's headers through the budgeted console readers, memoised in the organize facts cache keyed by `(path, size, mtime)`; holds `Box<dyn LatestVersions>` = `NoTitleList` in app state; request id: the call carries `request: u32` and the webview drops answers that are not the latest)
- Modify: `src-tauri/src/library/art.rs` (`read_child_art_bytes(entry_id, child_index, slot)`: path from the session, `child_slot_path`, canonicalise, `ensure_within` root, `MAX_ART_BYTES`), `src-tauri/src/library/mod.rs` (`set_default_revision(entry_id)`: group and rel path from the session, validates membership of a group > 1, saves settings, prunes this root, returns the new `revision_defaults`; `read_child_art(entry_id)` starts a job with undo over the entry's children; disabled reason on read-only volumes), the command registry (`src-tauri/src/lib.rs`/`ipc.rs`), `src/ipc/{backend.ts,tauriBackend.ts,types.ts,contract.requests.json}`, error kind `UnknownChild` in `src/ipc/errors.ts`
- Test: unit tests in `details.rs` and `art.rs`, `src/ipc/tauriBackend.test.ts`, `contract.test.ts`

- [ ] **Step 1: Write the failing tests:**
  - `game_details_reads_the_3ds_verdict`: session over a temp library with `ncsd("CTR-P-AXXT", …)` → `region.certainty == unknown`, `letter == 'T'`, `product_code` set; `latest == None` with `NoTitleList`; a fake `LatestVersions` answering `1.3.0` → `latest == { version: "1.3.0", source: "title-db" }`.
  - `game_details_is_offline`: the function signature takes no network client (compile-level) and a test asserts `details::game_details` is callable with only the session and a `&dyn LatestVersions`.
  - `game_details_with_a_missing_file` → `Ok` with empty facts, no panic.
  - `game_details_never_opens_a_key`: a `.tik` child → not in `take_open_log()`.
  - `set_default_revision_validates_membership`: an entry with no group or a single-member group → error; a member → map updated; another library's map untouched.
  - `read_child_art_bytes_refuses_unknown_and_escaping`: unknown index → `UnknownChild`; `#[cfg(unix)]` a `.media` symlink to outside the root → refused like `read_art_bytes`.
  - TS: `backend.gameDetails(id, request)`, `setDefaultRevision(id)`, `readChildArt(id)`, `readChildArtBytes(id, index, slot)` send exactly `{ entryId, request }` / `{ entryId }` / `{ entryId, childIndex, slot }`.
- [ ] **Step 2: Run** `cargo test -p coverly-roms && pnpm vitest run src/ipc` — fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** G-RUST and G-WEB.
- [ ] **Step 5: Commit + push.** `git commit -m "feat(tauri): game details, revision default and child art commands"`.

---

## Task 13: Mock add-ons library (`?mockPick=addons`, P-4)

**Files:**
- Create: `src/ipc/mock/addonViewsFixture.ts`, `src/ipc/mock/addonViewsFixture.test.ts`
- Modify: `src/ipc/mock/mockBackend.ts` (`MOCK_ADDONS = '/Users/demo/Addons'`, `MOCK_ADDONS_HUGE`; `openLibrary` serves `addonViewsLibrary(gen)` / `addonViewsHuge(gen)`; mock `gameDetails`, `setDefaultRevision` (persisted in the mock settings so it survives a reload via `localStorage` `coverly.mock.revisions`), `readChildArt` (marks icons as `package`), `readChildArtBytes` (an SVG via `mockArtSvg` variant); document the picks in the `?mockPick=` comment of `mockOrganize.ts`)

- [ ] **Step 1: Write the failing test:** the fixture contains the mockup's library: Lantern Harbor (PS4: 2 updates `1.05`/`1.10`, 1 DLC with `package` art, a demo `Lantern Harbor (Demo)` linked, `addons` counts `{mods:1, cheats:2, extras:3, saves:0}`), Paper Skies (Demo) without its game, Tidewalker (Switch: update `1.0.2`, 1 DLC without art, `newer_elsewhere {v1.3.0}` pointing at an orphan), Neon Rhythm (3 DLC, one «Nuevo» candidate), Kite Days, Star Courier (SNES `(Rev 1)` and untagged, one `version_group`), Pebble League, Moss Garden, two orphans (one linked, one not), and a 3DS base whose `gameDetails` answers letter `T`; every `orphan_of`/`demo.of`/`newer_elsewhere.entry` points to an existing id; raw counts asserted on entries (orphans by `orphan`, demos by `demo`), since `buildGames` lands in Task 14; `?mockPick=addons-huge` gives 10 000 games and 50 000 children deterministically (seeded `mulberry32`).
- [ ] **Step 2: Run** `pnpm vitest run src/ipc/mock` — fail.
- [ ] **Step 3: Implement** (synthetic names and sizes only).
- [ ] **Step 4: Run** G-WEB.
- [ ] **Step 5: Commit + push.** `git commit -m "test(ui): add-ons mock library"`.

---

## Task 14: Game model, display versions, summaries and «Nuevo» (D1, D4, D7, D18)

**Files:**
- Create: `src/lib/games.ts`, `src/lib/versions.ts`, `src/lib/addonSummary.ts`, `src/lib/seen.ts` and their `*.test.ts`
- Modify: `locales/{es,en}/ui.json` (`content.*`, `spoken.*` keys of spec §9)

**Interfaces (produced):**

```ts
// games.ts
export interface Game {
  id: number;                 // representative entry id (the default revision's)
  entry: EntryView;           // that entry
  revisions: readonly EntryView[];  // all members of its version_group, default first; [entry] otherwise
  children: readonly ChildView[];
  updates: readonly ChildView[];
  dlc: readonly ChildView[];
  isDemo: boolean;
  demoOf: number | null;      // entry id of the full game
  demos: readonly number[];   // entry ids of demos linked to it
  installed: string | null;
  missing: { entry: number; version: string } | null; // newer_elsewhere
  orphan: 'update' | 'dlc' | null;
}
export function buildGames(entries: readonly EntryView[], revisionDefaults: Readonly<Record<string, string>>): {
  games: Game[];       // non-orphan games, in entry order
  orphans: Game[];     // orphan add-ons
  byEntry: ReadonlyMap<number, Game>;   // every revision id → its game
};
export function defaultRevision(members: readonly EntryView[], chosen?: string): EntryView;
export interface GameCounts { games: number; demos: number; updates: number; dlc: number; moreRevisions: number; orphans: number }
export function gameCounts(games: readonly Game[], orphans: readonly Game[]): GameCounts;

// versions.ts — DISPLAY ORDER ONLY; «newest» comes from Rust (`installed`, `newer_elsewhere`).
export function compareVersions(a: string | undefined, b: string | undefined): number;

// addonSummary.ts
export type SummaryPart =
  | { kind: 'dlc'; n: number } | { kind: 'rev'; n: number }
  | { kind: 'version'; v: string } | { kind: 'missing'; v: string };
export function summaryParts(game: Game): SummaryPart[];
export function spokenSummary(game: Game, systemName: string, t: TFunction, demoOfName?: string): string;

// seen.ts
export function newChildren(root: string, games: readonly Game[]): ReadonlySet<string>; // rel_paths tagged «Nuevo»
```

- [ ] **Step 1: Write the failing tests:**

```ts
// games.test.ts
const e = (over: Partial<EntryView>): EntryView => ({ id: 0, system: 0, rel_path: '', dir: '', name: '', ext: 'sfc',
  size: 1, mtime: 0, regions: [], art: NO_ART, ...over });

it('folds revisions into one game, highest Rev by default', () => {
  const a = e({ id: 0, name: 'Star Courier (USA)', rel_path: 'SFC/Star Courier (USA).sfc', version_group: 'g' });
  const b = e({ id: 1, name: 'Star Courier (USA) (Rev 1)', rel_path: 'SFC/Star Courier (USA) (Rev 1).sfc', version_group: 'g', revision: 'Rev 1' });
  const { games, byEntry } = buildGames([a, b], {});
  expect(games).toHaveLength(1);
  expect(games[0].id).toBe(1);
  expect(byEntry.get(0)).toBe(games[0]);
  expect(gameCounts(games, []).moreRevisions).toBe(1);
});

it('honours a chosen default and falls back when it is stale', () => {
  const [a, b] = [/* as above */];
  expect(buildGames([a, b], { g: 'SFC/Star Courier (USA).sfc' }).games[0].id).toBe(0);
  expect(buildGames([a, b], { g: 'SFC/gone.sfc' }).games[0].id).toBe(1);
});

it('orders Rev A < Rev B and v1.0 < v1.1, unparseable after', () => { /* defaultRevision */ });

it('counts games and demos, never updates, DLC, revisions or orphans', () => {
  // Lantern Harbor (2 updates, 1 DLC) + its demo + an orphan update
  expect(gameCounts(games, orphans)).toEqual({ games: 2, demos: 1, updates: 2, dlc: 1, moreRevisions: 0, orphans: 1 });
});

it('is memo-friendly: same input → equal output, O(n)', () => { /* 10 000 entries < 200 ms locally; no CI timing assert */ });
```

```ts
// addonSummary.test.ts
it('summary parts and the missing warning', () => {
  expect(summaryParts(tidewalker)).toEqual([{ kind: 'dlc', n: 1 }, { kind: 'missing', v: '1.3.0' }]);
  expect(summaryParts(lantern)).toEqual([{ kind: 'dlc', n: 1 }, { kind: 'version', v: '1.10' }]);
});
it('speaks the whole state in Spanish and English', () => {
  expect(spokenSummary(tidewalker, 'Switch', tEs)).toBe(
    'Tidewalker, Switch. 1 DLC, 1 actualización, la más nueva v1.0.2, falta la última actualización, v1.3.0');
});
```

```ts
// seen.test.ts: first open tags nothing and seeds; a new rel_path next time is tagged; >50 000 paths → off; storage throwing → empty set.
// versions.test.ts: natural display order ('1.0.2' < '1.0.10'); doc comment says display only.
```

- [ ] **Step 2: Run** `pnpm vitest run src/lib/games.test.ts src/lib/versions.test.ts src/lib/addonSummary.test.ts src/lib/seen.test.ts` — fail.
- [ ] **Step 3: Implement** (one pass over entries, one `Map<version_group, EntryView[]>`; spoken phrase memoised per game in a `WeakMap`).
- [ ] **Step 4: Run** G-WEB.
- [ ] **Step 5: Commit + push.** `git commit -m "feat(ui): game model, versions and accessible summaries"`.

---

## Task 15: The pipeline runs on games; scope, search, sort, fill and prefs (D1, D4, D13, D19)

**Files:**
- Modify: `src/stores/useVisibleEntries.ts` (`buildGames` memoised on `view` + `revisionDefaults`, before scope; returns `visibleGames`, `games`, `orphans`, `counts` alongside the existing fields, `visible` becomes the representative entries of `visibleGames` so unchanged consumers keep working), `src/lib/scope.ts` (`{ kind: 'orphans' }`; fallback to `all` when there are none), `src/lib/search.ts` (a game matches by its name or any child's name), `src/lib/sort.ts` (sorts games; children never reordered), `src/lib/fill.ts` (`scopeIds` expands each game to all its revision ids for `selection` and `filter`; `library` stays `null`), `src/lib/viewPrefs.ts` (`shelfAddons: 'behind' | 'beside'` default `behind`, `childSlots: boolean` default `true`, `contentColumns: boolean` default `false`), `src/lib/detailsColumns.ts` (`version`, `content` ids; `ensureContentColumns(prefs, hasContent)` inserts both after `name` once and sets `contentColumns: true`), `src/stores/libraryStore.ts` (`revisionDefaults` from `LibraryView`, updated by `setDefaultRevision`)
- Test: `useVisibleEntries` test (create if missing), `scope.test.ts`, `search.test.ts`, `fill.test.ts`, `viewPrefs` test, `detailsColumns.test.ts`

- [ ] **Step 1: Write the failing tests:** selection fill of Star Courier sends `[1, 0]`; filter fill sends every revision of every visible game; search «Isla del faro» (a DLC name) finds Lantern Harbor; `orphans` scope lists only orphans and falls back to `all` when the last disappears; `sanitisePrefs({})` gives the three defaults and `sanitisePrefs({shelfAddons:'floating'})` → `behind`; a saved `columns: ['name','size']` with content present becomes `['name','version','content','size']` once, and removing `version` later sticks.
- [ ] **Step 2: Run** `pnpm vitest run src/lib src/stores` — fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** G-WEB.
- [ ] **Step 5: Commit + push.** `git commit -m "feat(ui): the library pipeline works on games"` (body: `NEEDS-OWNER: Q7 — filling a game also fills its other revisions`).

---

## Task 16: Status bar counts games; sidebar «Todos los juegos» and «Complementos sin juego base» (D4, D13, D19 c)

**REQUIRED SUB-SKILL:** frontend-design:frontend-design.

**Files:**
- Modify: `src/components/library/StatusBar.tsx`, `src/components/library/Sidebar.tsx` (`views.allGames`; orphans entry with count, only when orphans exist), `locales/{es,en}/ui.json` (`status.games`, `gamesOne`, `gamesOf`, `demos`, `demosOne`, `inside`, `updates`, `updatesOne`, `dlc`, `revisions`, `revisionsOne`, `orphans`, `orphansOne`, `views.allGames`, `orphans.*`; remove `status.shown` and `views.allRoms` from both)
- Create: `src/components/library/OrphansList.tsx` (Detalles-style list in every view mode while scope is `orphans`: kind, name, version in mono, folder in mono, «Su juego no está en la biblioteca.» or «Es de «{name}»; está fuera de su carpeta.» + «Ver el juego»)
- Test: `src/components/library/StatusBar.test.tsx` (create), `Sidebar.test.tsx`, `OrphansList.test.tsx`

- [ ] **Step 1: Write the failing tests:** unfiltered «9 juegos, 2 de ellos demos · dentro de ellos: 5 actualizaciones, 5 DLC y 1 revisión más (no cuentan como juegos)»; filtered «4 de 9 juegos …» with the inner clause counting only shown games; each clause omitted at 0; `1 juego`; the orphans clause is a button that sets scope `orphans`; English equivalents; «sin portada» counts games by their default file; sidebar shows «Todos los juegos» and the orphans entry with its count only when orphans exist; `OrphansList` «Ver el juego» selects the game and sets scope `all`.
- [ ] **Step 2: Run** `pnpm vitest run src/components/library/StatusBar.test.tsx src/components/library/Sidebar.test.tsx src/components/library/OrphansList.test.tsx` — fail.
- [ ] **Step 3: Implement** (`Intl.ListFormat(locale, { type: 'conjunction' })` for the inner list; numbers via `Intl.NumberFormat` in mono spans).
- [ ] **Step 4: Run** G-WEB; screenshots at 1440/390, light/dark on `/?mockPick=addons` (`pnpm dev --port 1472 --strictPort`, stop by port), critique.
- [ ] **Step 5: Commit + push.** `git commit -m "feat(ui): the status bar counts games and lists add-ons without their game"` (body: `NEEDS-OWNER: Q11 — «Todas las ROMs» becomes «Todos los juegos»; Q4 — orphans list is read-only`).

---

## Task 17: Content filters «Mostrar juegos que» (D14)

**REQUIRED SUB-SKILL:** frontend-design:frontend-design.

**Files:**
- Create: `src/components/library/ContentFilters.tsx`
- Modify: `src/lib/filters.ts` (`Filters.content: { updates: boolean; missing: boolean; dlc: boolean; demos: boolean }` in `EMPTY_FILTERS`; `applyGameFilters(games, filters, latest?)` — `missing` true when `game.missing` or (seam) a known latest above `installed`; `contentFacets(games)`), `src/stores/filterStore.ts` (`toggleContent(key)`, `clear()` clears them), `FilterPanel.tsx` («Quitar filtros» clears them), the library layout (row above the views), `locales/{es,en}/ui.json` (`content.filters.*`)
- Test: `src/lib/filters.test.ts`, `src/components/library/ContentFilters.test.tsx`

- [ ] **Step 1: Write the failing tests:** AND across chips and with the panel filters; each chip shows its live count over the scope; `ToggleGroup type="multiple"` items expose `aria-pressed`; «Le falta la última actualización» has `aria-describedby` pointing at the visible hint «Compara solo dentro de tu biblioteca hasta que haya lista de títulos.»; the polite live region announces «2 juegos con estos filtros.» 500 ms after a chip press (fake timers) and never on search typing; «Ningún juego cumple estos filtros.»; the row is hidden when every count is 0; a fake `latest` makes `missing` true for a game with no orphan.
- [ ] **Step 2: Run** `pnpm vitest run src/lib/filters.test.ts src/components/library/ContentFilters.test.tsx` — fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** G-WEB; screenshots and critique.
- [ ] **Step 5: Commit + push.** `git commit -m "feat(ui): content filters for updates, DLC and demos"` (body: `NEEDS-OWNER: Q5 — chip row above the views; Q6 — in-library hint`).

---

## Task 18: Portadas — summary under the cover, dot at small sizes (§8.2, D17)

**REQUIRED SUB-SKILL:** frontend-design:frontend-design.

**Files:**
- Modify: `src/lib/coverSizes.ts` (`metrics(size, content)`: `m`/`l`/`xl` add one 18 px summary line when the library has content), `src/components/library/views/CoverTile.tsx` (summary chip line under `cover-tile__info`; `xs`/`s` dot top-right, «↑» glyph when missing; `sr-only` spoken phrase inside the `gridcell`; `title` with the phrase; chip click opens the panel at Versión), `CoversView.tsx` (passes `Game`, content flag), `src/styles/globals.css` (`.cover-tile__content`, `.addon-chip`, `.addon-chip--warning`, `.cover-tile__dot`, forced-colours rules), `locales/{es,en}/ui.json` (`content.summary.*`, `content.new`, `content.demoOf`)
- Test: `CoverTile.test.tsx`, `CoversView.test.tsx`, `coverSizes.test.ts`

- [ ] **Step 1: Write the failing tests:** Lantern Harbor shows «+1 DLC · v1.10» under the art, not inside the art element; Tidewalker shows «↑ falta v1.3.0» with the warning class and the word; narrow tile drops `+N DLC` first, never the warning; `xs` renders a dot with `aria-hidden` and the `sr-only` phrase; missing → glyph «↑» (shape change); a demo shows «Demo de Lantern Harbor» and no chip; a library without content keeps today's heights (`metrics('m', false)` equals the old value) and adds 18 px with content; the chip is not a tab stop (`tabIndex` absent); no `aria-label` on the cell (visible name preserved).
- [ ] **Step 2: Run** `pnpm vitest run src/lib/coverSizes.test.ts src/components/library/views/CoverTile.test.tsx src/components/library/views/CoversView.test.tsx` — fail.
- [ ] **Step 3: Implement** with `--cr-surface-sunken`, `--cr-text-secondary`, `--cr-warning`, `--cr-warning-bg`, `--cr-font-mono`, `--cr-manila` (Nuevo only).
- [ ] **Step 4: Run** G-WEB; screenshots of every cover size, light/dark, 1440/390; compare with the mockup's Portadas; critique.
- [ ] **Step 5: Commit + push.** `git commit -m "feat(ui): add-on summary under each cover"` (body: `NEEDS-OWNER: Q8 — chip says «↑ falta v1.3.0»; Q9 — «Nuevo» from a per-viewer seen set`).

---

## Task 19: Galería — DLC tray and facts (§8.3)

**REQUIRED SUB-SKILL:** frontend-design:frontend-design.

**Files:**
- Create: `src/components/library/game/ChildThumb.tsx` (48/24 px child thumbnail: own art via `readChildArtBytes` through `artCache`, base art dimmed with «del juego base», or the dashed missing slot)
- Modify: `src/components/library/views/GalleryView.tsx` (tray under the big cover; each thumbnail a button opening the panel at that child; facts Versión / DLC / Demo; filmstrip captions `+3`, `↑`, `2 rev.`, `demo` in mono, `aria-hidden` with the spoken phrase on the item)
- Test: `GalleryView.test.tsx`, `ChildThumb.test.tsx`

- [ ] **Step 1: Write the failing tests:** tray lists each DLC and each update with own art, captioned; a DLC without art → dashed slot with «sin arte»; Switch DLC → dimmed base art with «del juego base»; button names «Abrir Isla del faro en la ficha»; facts show «Demo: En la biblioteca» / «No está en la biblioteca»; filmstrip captions.
- [ ] **Step 2: Run** `pnpm vitest run src/components/library/views/GalleryView.test.tsx src/components/library/game` — fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** G-WEB; screenshots and critique.
- [ ] **Step 5: Commit + push.** `git commit -m "feat(ui): DLC tray in the gallery"`.

---

## Task 20: Detalles — the treegrid with Versión and Contenido (§8.4, D17, D19 b)

**REQUIRED SUB-SKILL:** frontend-design:frontend-design.

**Files:**
- Modify: `src/lib/rowModel.ts` (flatten `Game[]` with an expanded `Set<number>` into rows `{ kind: 'section' | 'game' | 'child' | 'revision' | 'demo' | 'folder' | 'ghost', level, setsize, posinset, … }`, fixed height, rebuilt only on expansion/sort/games change), `src/lib/detailsColumns.ts` (`version` cell: «v1.2», «v1.0.2 ↑ falta v1.3.0», «Rev 1», «—»; `content` cell: «2 act., 1 DLC, demo aparte», «2 revisiones»; sort Versión by `compareVersions` display order, Contenido by child count), `src/components/library/views/DetailsView.tsx` (`role="treegrid"`, `aria-rowcount` of the flattened model, row-first roving focus kept in the rendered range, `aria-level/setsize/posinset/rowindex`, `aria-expanded` on parents only; keys: ↓/↑, → expand/first child, ← parent/collapse, Home/End, Enter opens the panel (child → panel at that child); key hint line; ghost row; 390 px hides Sistema, Contenido, Tamaño), `src/stores/viewStore.ts` (session `expanded: Set<number>`, the selected game starts expanded), `src/styles/globals.css` (`.tree-row`, chevron rotation on `--cr-duration-fast`/`--cr-ease-standard`, `.tree-row--ghost` dashed with `--cr-border-width-slot`, forced colours), `locales/{es,en}/ui.json` (`content.kind.*`, `content.newest`, `content.notInLibrary`, `content.cell.*`, `content.keysHint`)
- Test: `DetailsView.test.tsx`, `rowModel.test.ts`, `detailsColumns.test.ts`

- [ ] **Step 1: Write the failing tests:** roles and every aria attribute on game, child and section rows when grouped (levels 1/2/3); each key of §8.4 moves focus or expands as specified (Testing Library `user.keyboard`); Enter on a child calls `openPanel(gameId, { child: index })`; the ghost row for Tidewalker reads «↑ v1.3.0 falta», «no está en la biblioteca», no size; child rows are excluded from the select-all count; sorting by name reorders games and keeps children under them newest first; `aria-rowcount` equals header + all flattened rows; reduced motion (`useReducedMotion` true) → no transition class; 390 px hides three columns; the key hint text.
- [ ] **Step 2: Run** `pnpm vitest run src/lib/rowModel.test.ts src/lib/detailsColumns.test.ts src/components/library/views/DetailsView.test.tsx` — fail.
- [ ] **Step 3: Implement** (expanding never animates height).
- [ ] **Step 4: Run** G-WEB; screenshots expanded/collapsed, grouped/ungrouped, light/dark, 1440/390; critique.
- [ ] **Step 5: Commit + push.** `git commit -m "feat(ui): details view as a treegrid with version and content"`.

---

## Task 21: Columnas — typed groups inside a game (§8.5)

**REQUIRED SUB-SKILL:** frontend-design:frontend-design.

**Files:**
- Modify: `src/lib/folderTree.ts` (a game folder whose base is a game with children, and a loose game with revisions, becomes a game node: «Juego base», «Actualizaciones (n)», «DLC (n)», «Revisiones (n)», «Demo» (link to the demo's own path), separator, «Trucos», «Mods», «Extras», «Partidas» with counts, not navigable; empty groups omitted; ordinary subfolders unchanged), `src/components/library/views/ColumnsView.tsx` (game column label with `+1 DLC` mono suffix; preview: art, «Archivo», «Tamaño», «Arte: Propio, incluido en el paquete» / «del juego base»)
- Test: `folderTree.test.ts`, `ColumnsView.test.tsx`

- [ ] **Step 1: Write the failing tests:** Lantern Harbor's column lists the five typed groups in order with counts; `SFC/Hacks/` still a folder; activating «Demo» jumps to the demo game's column path; → enters, ← leaves (unchanged keys); preview origin text for a DLC with `package` art.
- [ ] **Step 2: Run** `pnpm vitest run src/lib/folderTree.test.ts src/components/library/views/ColumnsView.test.tsx` — fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** G-WEB; screenshots and critique.
- [ ] **Step 5: Commit + push.** `git commit -m "feat(ui): columns view opens a game into its typed groups"`.

---

## Task 22: Huecos de arte — child slot rows and toggle (§8.6)

**REQUIRED SUB-SKILL:** frontend-design:frontend-design.

**Files:**
- Modify: `src/components/library/views/MountsView.tsx` (indented sub-rows with the L-shaped hairline for each DLC and each update with own art: Icon and Banner slots, origin tag «paquete»/«SS»/«SGDB»/«tuyo», dashed «falta», a DLC without own art shows dimmed base art «usa el del juego base»; slot actions reuse the drawer's art actions with the child as target, search manual only), the view bar toggle «Huecos del contenido (6)» (`aria-pressed`, persisted as `viewPrefs.childSlots`), `locales/{es,en}/ui.json` (`content.mounts.*`, `content.slot.*`, `content.origin.*`)
- Test: `MountsView.test.tsx`

- [ ] **Step 1: Write the failing tests:** sub-rows appear for DLC and for updates with own art only; origin tags; missing slot dashed with «falta»; toggle hides them and persists through `sanitisePrefs`; the toggle count equals the number of sub-rows; each sub-row has an accessible name «DLC Isla del faro, icono incluido en el paquete, banner falta».
- [ ] **Step 2: Run** `pnpm vitest run src/components/library/views/MountsView.test.tsx` — fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** G-WEB; screenshots and critique.
- [ ] **Step 5: Commit + push.** `git commit -m "feat(ui): art slots of updates and DLC in the slots view"`.

---

## Task 23: Estantes — DLC edges behind the box, «Al lado» option (§8.7, D17)

**REQUIRED SUB-SKILL:** frontend-design:frontend-design.

**Files:**
- Modify: `src/lib/shelves.ts` (one box per game), `src/components/library/views/ShelvesView.tsx` (up to 3 edges offset 7 px down / 9 px right behind the box, «+n» on the last; label «Con 3 complementos» and «↑ falta v1.3.0»; 3 px fan on hover/selection; segmented control «Complementos: Detrás de la caja | Al lado» with `ToggleGroup type="single"`, persisted `viewPrefs.shelfAddons`; «Al lado» shows small child icons beside the box; demos are their own box «Demo de …»), `src/styles/globals.css` (`.shelf-edge`, fan transition, forced colours `CanvasText`), `locales/{es,en}/ui.json` (`content.shelves.*`)
- Test: `ShelvesView.test.tsx`, `shelves.test.ts`

- [ ] **Step 1: Write the failing tests:** Neon Rhythm (3 DLC) renders 3 edges and no «+n»; a 5-DLC game renders 3 edges with «+2»; edges are `aria-hidden` and the box's name includes «con 3 complementos»; switching to «Al lado» persists after a re-render from prefs; reduced motion → no fan class; demos get their own box.
- [ ] **Step 2: Run** `pnpm vitest run src/lib/shelves.test.ts src/components/library/views/ShelvesView.test.tsx` — fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** G-WEB; screenshots both modes, light/dark, 1440/390; critique.
- [ ] **Step 5: Commit + push.** `git commit -m "feat(ui): shelves keep DLC behind their game's box"`.

---

## Task 24: Organizar — child thumbnails (D15)

**REQUIRED SUB-SKILL:** frontend-design:frontend-design.

**Files:**
- Modify: `src/components/organize/SystemGroups.tsx` (24 px thumbnail per child row from the existing embedded-candidate preview of the **source** file, no write; only rows in view via `IntersectionObserver`; at most 4 in flight; aborted when the plan closes; none → nothing rendered), the organize backend call used for the preview (reuse; no new command unless the existing one cannot take a plan item id — if a new one is needed, it takes the plan item id, never a path)
- Test: `SystemGroups.test.tsx`

- [ ] **Step 1: Write the failing tests:** a child row in view requests its thumbnail once; 10 rows → never more than 4 requests pending; closing the plan aborts pending ones; a child with no embedded art renders no image and no placeholder.
- [ ] **Step 2: Run** `pnpm vitest run src/components/organize/SystemGroups.test.tsx` — fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** G-WEB; screenshot of `/?mockPick=gamefolders` Organizar with thumbnails; critique.
- [ ] **Step 5: Commit + push.** `git commit -m "feat(organize): thumbnails of the art each update and DLC carries"`.

---

## Task 25: Game panel — head, Versión and Revisiones (§8.8 1–3, D5–D7)

**REQUIRED SUB-SKILL:** frontend-design:frontend-design.

**Files:**
- Create: `src/components/library/game/RegionLine.tsx`, `VersionSection.tsx`, `RevisionsSection.tsx` (+ tests)
- Modify: `src/components/library/DetailDrawer.tsx` (calls `gameDetails(id, request)` on selection, drops stale answers; sections above Arte/Identidad/Archivo with ids `panel-version`, `panel-revisions`, `panel-dlc`, `panel-demo`, `panel-folders`; `openPanel({ section, child })` scrolls with `behavior: 'auto'` under reduced motion and focuses the heading), `src/stores/selectionStore.ts` (panel target), `locales/{es,en}/ui.json` (`panel.version`, `updates*`, `newestInLibrary`, `baseVersion`, `latestOk`, `latestMissing`, `latestElsewhere`, `showIt`, `*Pill`, `revisions*`, `rev*`, `region*`)
- Test: `DetailDrawer.test.tsx`, `game/*.test.tsx`

- [ ] **Step 1: Write the failing tests:**
  - Region: «3DS, Europa, España» for `S`; «3DS, Europa (probable, según GameTDB y 3dbrew)» for `Z`; warning line «Letra de región «T» sin identificar (CTR-P-AXXT)»; no line when `game_details` fails or the name already carries a region.
  - Versión: Lantern Harbor big «v1.10» + «la más nueva de tu biblioteca», list newest first with «La más nueva» pill and the base row «Juego base»; no-updates game: «(la del juego base)» and «sin actualizaciones»; Tidewalker: «La v1.3.0 está en tu biblioteca, fuera de la carpeta del juego.» + «Ver dónde está» sets scope `orphans` and selects it; with a fake `latest` answer: «Es la última que existe, según la lista de títulos.» / «Existe la v1.3.0 y no está en tu biblioteca. Dato de la lista de títulos.»; no callout with `NoTitleList` and no orphan.
  - Revisiones: «2 archivos, un juego»; the default row has `aria-pressed="true"` «Predeterminada»; «Usar esta» (`aria-label` «Usar Rev 1 como predeterminada») calls `setDefaultRevision(id)`, the polite live region says «Rev 1 es ahora la predeterminada.», the library store's `revisionDefaults` update rebuilds the game; a settings-save error keeps the previous default; the D7 note text.
  - A stale `gameDetails` answer (selection changed) is ignored.
- [ ] **Step 2: Run** `pnpm vitest run src/components/library/DetailDrawer.test.tsx src/components/library/game` — fail.
- [ ] **Step 3: Implement** (Archivo display size only for the big version number; mono for versions, ids, sizes).
- [ ] **Step 4: Run** G-WEB; screenshots and critique against the mockup's panel.
- [ ] **Step 5: Commit + push.** `git commit -m "feat(ui): game panel version, revisions and region"`.

---

## Task 26: Game panel — DLC, related demo, Mods/Cheats/Extras/Saves, child art actions (§8.8 4–6, D8, D9, D11)

**REQUIRED SUB-SKILL:** frontend-design:frontend-design.

**Files:**
- Create: `src/components/library/game/DlcSection.tsx`, `DemoSection.tsx`, `FixedFoldersSection.tsx` (+ tests)
- Modify: `DetailDrawer.tsx` (mount them), the drawer search (child target: query `<game> <child name>`, writes on explicit pick with origin `screenscraper`/`steamgriddb`), `locales/{es,en}/ui.json` (`panel.dlcSub*`, `readArt`, `searchArt`, `relatedDemo`, `demoCard`, `demo`, `openFull`, `fullInLibrary`, `fullMissing`, `demoNote`, `folders`, `folderEmpty`, `files*`, `savesLater`, `content.origin.*`, `content.noArt`)
- Test: those files' tests, `DrawerSearch.test.tsx`

- [ ] **Step 1: Write the failing tests:** DLC rows with thumbnail (own / dimmed base / dashed), name, file in mono truncated with full path in `title`, origin text («incluido en el paquete», «arte del juego», «sin arte» in warning tone with the word), «Nuevo» tag only for `newChildren`; «Buscar arte» opens the drawer search with the child query and never writes without a pick; «Leer el arte incluido» shown only when a child lacks art and its system can carry it (not Switch), disabled with the read-only reason on a read-only library, and calls `readChildArt(id)`; Demo relacionada card «Juego aparte, 184 MB» selects the demo; on a demo: «Abrir el juego completo» or «Es una demo de Paper Skies. El juego completo no está en tu biblioteca.» plus the demo note; demo without cover shows the full game's cover dimmed with «del juego completo» (display only); folders `<details>` collapsed by default, four cells with counts or «vacía», «más de 10 000 archivos» when capped, the block-5 sentence, the folder path in mono; no write action anywhere in that group.
- [ ] **Step 2: Run** `pnpm vitest run src/components/library/game src/components/library/DrawerSearch.test.tsx` — fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** G-WEB; screenshots and critique.
- [ ] **Step 5: Commit + push.** `git commit -m "feat(ui): game panel DLC, related demo and the game's folders"` (body: `NEEDS-OWNER: Q3 — network art for DLC is manual only`).

---

## Task 27: CLI note and changelog

**Files:**
- Modify: `crates/coverly-cli/src/organize.rs` (if not already covered in Task 3, the `RegionLetterUnknown` row in `render_table` under its item, and `--help` unchanged), `CHANGELOG.md` (Unreleased: Added — add-ons inside their game in every view, game panel versions/revisions/DLC/demo, content filters, «Complementos sin juego base», child art read from packages, 3DS region and country by product code; Changed — status bar counts games, «Todos los juegos», 3DS letters `C`/`U`/`R`/`V`/`X`/`T` per the verified table)
- Test: `cargo test -p coverly-cli`

- [ ] **Step 1: Write the failing test** (if Task 3 left the table row out): plan text of a 3DS pile with letter `T` lists the note under the base.
- [ ] **Step 2: Run** `cargo test -p coverly-cli` — fail (or skip to Step 4 when Task 3 covered it; say so in the commit body).
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** G-RUST and `pnpm check-all`.
- [ ] **Step 5: Commit + push.** `git commit -m "docs: changelog for add-ons in the six views and the 3DS region"`.

---

## Task 28: Real verification, E2E and the PR

**Files:**
- Evidence (not committed): screenshots and logs in `/private/tmp/claude-501/-Users-miguelangelesparzacalero-Documents-GitHub/35b7c020-f2ae-412f-b757-99c24a9a498b/scratchpad/addon-views/`

- [ ] **Step 1: Update with main.** `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-views && git fetch origin && git merge origin/main` — resolve conflicts by hand (locale files key by key), commit the merge if any, `git push`.
- [ ] **Step 2: All gates** from a clean tree: `pnpm check-all && pnpm build && cargo fmt --all --check && cargo clippy --workspace --all-targets --all-features -- -D warnings && scripts/dev/clippy-windows.sh && cargo test --workspace --no-fail-fast`. Every one green; record the tail of each output.
- [ ] **Step 3: Real-data check (read-only sources).**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-views && ls ~/Documents/ROMS   # read-only: pick 2–3 small folders with updates/DLC (3DS, PS4 or Switch) and one retro folder with (Rev 1)
mkdir -p target/addon-sample && cp -RXp ~/Documents/ROMS/<folders> target/addon-sample/ && \
  find target/addon-sample -name '._*' | wc -l   # must print 0
cargo run -p coverly-cli -- organize target/addon-sample --layout coverly-library --dry-run   # 3DS region names and notes
cargo run -p coverly-cli -- organize target/addon-sample --layout coverly-library --apply      # child art read from packages
ls target/addon-sample/*/*/DLC/.media target/addon-sample/*/*/Updates/.media 2>/dev/null
cargo run -p coverly-cli -- audit target/addon-sample                                          # no orphan images for child art
cargo run -p coverly-cli -- undo && find target/addon-sample -name '*-icon.png' | wc -l        # 0 after undo
rm -rf target/addon-sample
```

  Never pass `~/Documents/ROMS`, `/Volumes/TinyExt/*` or `/Volumes/System` to a writing command. Note the counts (games, demos, updates, DLC, revisions, orphans, 3DS letters seen, unknown letters) for the PR.
- [ ] **Step 4: E2E (spec §12).** `pnpm dev --port 1472 --strictPort` in the background; Chrome DevTools MCP `new_page` with `isolatedContext: "addon-views"`, `/?mockPick=addons`:
  1. The six views in light and dark at 1440 and 390 px; compare each with the mockup's screenshots (`views-21c/shots/`); status bar «… juegos, … de ellos demos · dentro de ellos: …».
  2. Detalles: keyboard walk (↓ ↑ → ← Home End Enter) with `take_snapshot` showing `treegrid`, `aria-expanded`, `aria-level`; ghost row for Tidewalker.
  3. Filters: each chip, the live region text, «Quitar filtros»; numbers match the status bar.
  4. Panel: Star Courier «Usar esta» on the untagged revision, reload, the choice survives (mock persistence); Tidewalker «Ver dónde está» opens «Complementos sin juego base»; 3DS base shows «Letra de región «T» sin identificar (…)».
  5. Estantes «Al lado» persists after reload; Huecos de arte toggle.
  6. `emulate` `prefers-reduced-motion: reduce` → no fan or chevron transition (`get_css_styles` on the elements); forced colours emulated → every state still has its word; contrast of `.addon-chip--warning` text ≥ 4.5:1 in both themes (`evaluate_script` with computed colours).
  7. 390 px: `document.documentElement.scrollWidth <= innerWidth`; `list_console_messages` has no errors.
  8. `/?mockPick=addons-huge`: measure `buildGames` time with `performance.now()` around a re-open; write the number in the PR.
  Fallback: Playwright MCP; if both profiles are locked, `cd /private/tmp/claude-501/-Users-miguelangelesparzacalero-Documents-GitHub/35b7c020-f2ae-412f-b757-99c24a9a498b/scratchpad/addon-views && npm init -y && npm i playwright-core` and drive Chrome for Testing. Stop the server: `lsof -ti tcp:1472 -sTCP:LISTEN | xargs -r kill`.
- [ ] **Step 5: Open the PR** (English, no attribution lines, no mention of Claude or AI): `gh pr create --base main --head feat/addon-views --title "Updates, DLC, demos and revisions in the six views; 3DS region by product code (2.1 part C)"` with a body listing: what ships (D1–D19), the verified 3DS table with its three sources, **behaviour changes for installed users** (3DS `C`/`U`/`R`/`V`/`X`/`T`, `FACTS_VERSION` 12 re-reads headers once, `(Russia)` is now a region tag, «juegos» wording, two new Details columns inserted once), gates with their results, E2E evidence (screenshots described), the real-data counts and the 10 000-game timing, and **Owner decisions taken by default**: Q1–Q11 from spec §14 plus every `NEEDS-OWNER` line (`git log origin/main..HEAD --format=%b | grep NEEDS-OWNER`). Add a PR comment with the local gate evidence (Actions is billing-blocked; local gates are the CI).
- [ ] **Step 6: Final state.** `git status -sb` shows `## feat/addon-views...origin/feat/addon-views` with nothing ahead.
