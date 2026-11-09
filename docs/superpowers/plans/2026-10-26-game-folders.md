# The Game as a Folder (Part A): Base, Updates/, DLC/, Demos, Versions and Revisions — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking. The UI tasks (Tasks 18 and 19) also REQUIRE frontend-design:frontend-design: follow `docs/08-brand/brand.md` and spec §9.1, use the `src/components/ui` primitives and critique the changed screens with screenshots before committing.

**Goal:** `TODO.md` §2.1 part A. With the new profile option «carpeta por juego con `DLC/` y `Updates/`» (on for the new built-in `coverly-library` profile, off for every handheld/frontend profile), Organizar moves a modern game's base, its updates and its DLC into `<System>/<Game>/` (`Updates/<Game> (vX).<ext>`, `DLC/<Game> - <DLC name>.<ext>`) with dry run and full undo; demos become their own `<Game> (Demo)` game linked by name; versions and regions come from plaintext headers; detection of base/update/DLC/demo/system titles covers every system of the research table; licences are recognised by name only and travel with their file only by an unambiguous same-volume rename; firmware, shader caches and emulator configs are «not a game»; retro revisions are versions of one game in the scan model and Duplicados. The scan, the pile walk and Duplicados **always** respect existing game folders (never loose games, never duplicates of the base), whatever the option says.

**Architecture:** Everything reuses what exists. A new `game_folder` module owns the fixed folder names and the one-listing «is this a game folder» test, shared by the walk, the scan and the dedupe units. The planner keeps its order (survey → identify → `updates_after_bases` → `note_updates` → `assign_targets_in` → sibling passes → dedupe step) and gains: a `GameLink` per item written while linking; game folder `subdirs` for bases computed before `assign_targets_in` (the hook Batocera's DLC folder already uses); a `game_children_targets` pass after it, shaped like `sibling_targets`, that names children from their base's final folder; a licence pass after targets. Apply and undo need no new journal op (`DirCreated`, `Moved`). The scan attaches `Updates/`/`DLC/` files to the base `RomEntry` as `children`. Dedupe builds role-tagged units and gains two removal guards. UI: Organizar folds by `game.of` and shows a collapsed folder preview; Ajustes → Biblioteca has one switch.

**Tech Stack:** Rust 1.99 (workspace MSRV 1.93), `serde`, `serde_json`, `toml`, `unicode-normalization`, `insta`, `filetime`, `tempfile`; React 19 + TypeScript + Tailwind 3.4 + zustand, Vitest + Testing Library. **No new crate and no new npm package.**

**Spec:** [`docs/superpowers/specs/2026-10-26-game-folders.md`](../specs/2026-10-26-game-folders.md). It is binding: decisions D1–D25 (§4), files (§5), profiles (§6), naming examples (§7), behaviour (§8), UX (§9), copy (§10, word for word), edge cases (§12), owner questions (§13, implemented with the safe default), tests (§14). This plan turns it into tasks and records its deviations below. The systems batch 2 and 3 plans ([`2026-10-26-systems-batch-2.md`](2026-10-26-systems-batch-2.md), [`2026-10-26-systems-batch-3.md`](2026-10-26-systems-batch-3.md)) are the pattern for builders, harnesses, snapshots and the sibling pass reused here.

## Global Constraints

- **Worktree and branch.** Work only in `/Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-game-folders`, branch `feat/game-folders`, which tracks `origin/feat/game-folders`. Agent shells reset their cwd, so every shell command starts with `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-game-folders &&`. Never switch branches and never touch `main`.
- **Git rules (mandatory).** An earlier run lost a whole branch because it was never pushed.
  - Conventional, granular commits in English (`feat(core): …`, `test(core): …`, `feat(ui): …`, `feat(app): …`, `feat(cli): …`, `docs: …`).
  - **No** `Co-Authored-By` line, no "Generated with" line, no mention of Claude or AI anywhere (commits, PR, code comments). This owner rule overrides any harness attribution.
  - Run `git push` **immediately after every commit**. Every task ends with `git commit … && git push && git status -sb`; the first line must read `## feat/game-folders...origin/feat/game-folders` with no `[ahead N]`.
  - Never force-push. To update with `main`: `git fetch origin && git merge origin/main`, resolve conflicts by hand (other workflows change `locales/*.json`, `src/components/*` and `TODO.md` in parallel; resolve locale files **key by key**, never by taking one side), run the gates, commit the merge, push.
- **Do not edit** `docs/TRACKER.md` or `TODO.md` on this branch (they are updated on `main` after merge).
- **Scope: part A only.** Do **not** build the six-views redesign or art slots for children (part C), Mods/ and Cheats/ detection or association, per-emulator translators for shadPS4/RPCS3/Azahar/Cemu/Vita3K/Switch/ES-DE (part B), Extras/ CRUD, `extras.json` or a media viewer (part D), saves (block 5), «reagrupar biblioteca» (Q1) or an online «newest available» check (Q10). Organizar never creates `Mods/`, `Cheats/`, `Extras/` or `Saves/` (D4).
- **Keys rule, no exceptions.** Never read, include, download or decrypt console keys or licences. `.rap .rif .zrif .dkey .tik .cetk .cik .edat`, `work.bin`, `ticket.db`, `seeddb.bin`, `title.tik`/`*.tik`, `keys.txt`, `otp.bin`, `prod.keys`, `title.keys`, `dev.keys`, `aes_keys.txt`, `act.dat`, `eeprom.bin` are recognised **by name or extension only**, before any open; nothing in their code path opens, hashes, sniffs or stats beyond `symlink_metadata`. A licence moves only by an exclusive rename on the same volume in move mode, else it stays (D17). Identification stays keyless: plaintext ids, categories and content types only. Every test with a licence beside a game runs under `consoles::start_open_log()` and asserts the licence path is never in `take_open_log()`.
- **Installed users first.** With the option off and any existing profile, every existing plan snapshot stays byte-identical (the snapshot tests prove it). The built-in id is `coverly-library`; a custom profile file with that id is kept as `coverly-library-custom` (D2). No `SETTINGS_VERSION` bump; new serde fields are defaulted and skipped when empty.
- **Safety rules unchanged (D21).** Dry-run plan first; apply re-validates fingerprints; exclusive renames, no overwrite; same-volume moves, otherwise copy + verify + remove; volume-gone pause/resume; undo removes only empty folders it created.
- **Untrusted header text.** Any header-derived path component (`<DLC name>`, orphan title, version text) goes through `consoles::clean_title` then the existing safe-name rule; a test asserts a `TITLE` of `../../x` stays below `<System>/<Game>/DLC/`.
- **TDD.** Write the failing test, run it and watch it fail for the expected reason, implement, watch it pass. Each "Run" step names the exact command.
- **Gates (same as CI).** Before every commit that touches code or tests:
  - **G-RUST:** `cargo fmt --all --check && cargo clippy --workspace --all-targets --all-features -- -D warnings && scripts/dev/clippy-windows.sh && cargo test --workspace --no-fail-fast`
  - **G-WEB** (any task touching `src/`, `locales/`, `design/` or `src-tauri/`): `pnpm check-all && pnpm build`
  - Docs-only commits run `pnpm check-all`.
- **Windows CI leg.** Fixture mtimes pinned (`coverly_core::dedupe::testing::pin_mtime` or `filetime::set_file_mtime` to a fixed time, never "now"). Paths compared by component (`Path::components`, `file_name`, `Path::join`), never by slash text; snapshot paths go through the existing `/` redaction. Unix-only helpers go in `#[cfg(unix)] mod unix_tests`. The `DLC/` + `dlc/` test runs only when the temp volume is case-sensitive (probe it first, `return` otherwise). No fixture folder name is reserved (`CON`, `AUX`…) or ends in a dot or space, except in the safe-name tests that assert the rewrite.
- **Real data is read-only.** Never write to `~/Documents/ROMS`, `/Volumes/TinyExt/*` or `/Volumes/System`; only `ls`, `stat` and Coverly **plan-only** runs may touch them. Write tests only on copies made with `cp -RXp` under `/Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-game-folders/target/game-folders-sample/`; `find target/game-folders-sample -name '._*' | wc -l` must print `0`; delete the folder afterwards.
- **Fixtures are synthetic** (builders in `consoles/testing.rs`, no copyrighted data, no keys; real game names only as file names over synthetic bytes).
- **No UI strings in the core.** Reasons, notes and roles are enums plus data; UI and CLI render them. Every UI string lives in `locales/en/ui.json` **and** `locales/es/ui.json` with the same keys and placeholders (parity test). Copy is spec §10 word for word (Spanish: sentence case, no exclamation marks, exact counts; numbers, versions and paths in mono by the renderer).
- **Brand.** Only existing tokens from `design/tokens.json` (`pnpm tokens:check` green), `src/components/ui` primitives, `SystemIcon` for system marks. No warning colour for game folders.
- **Secrets are never read or printed:** the minisign key, `~/.coverly-release/`, any `.env*`, `.vercel/`.
- **Servers and E2E.** The web UI runs on port **1470** only: `pnpm dev --port 1470 --strictPort` (mock backend). Stop it by port only: `lsof -ti tcp:1470 -sTCP:LISTEN | xargs -r kill`. E2E uses Chrome DevTools MCP `new_page` with `isolatedContext: "game-folders"`; fall back to Playwright MCP; if both profiles are locked, install `playwright-core` in `/private/tmp/claude-501/-Users-miguelangelesparzacalero-Documents-GitHub/35b7c020-f2ae-412f-b757-99c24a9a498b/scratchpad/game-folders` and drive Chrome for Testing from there.
- **Owner decisions never block.** Every §13 question is implemented with its safe default. Anything new that needs Miguel is implemented around, recorded as a `NEEDS-OWNER: <topic> — <default taken>` line in the task's commit body and collected into the PR (Task 20). A rule in the spec marked «medium confidence» (3DS TMD bit split, 3DS product-code region letters) keeps its documented fallback (raw value, no region) if its source cannot be confirmed in `docs/02-research/`; say so in a code comment and add `BLOCKED: <rule> — <why>` to the commit body.

## Deviations from the spec (decided in this plan)

- **P-1: system titles are a `TitleKind` variant.** Spec D14 says «→ `ConsoleSystem`» and §5.1 names `is_console_system_title(system, id)`. One new variant `TitleKind::System` (serde `system`) carries it through readers, facts and dedupe without a side channel; `Sniff::add_console` maps a single verdict of kind `System` to `NotARom::ConsoleSystem` exactly as it does today for `XVD_SYSTEM_DETAIL`/`TMD_SYSTEM_DETAIL`. `is_console_system_title` is kept as the helper the kind functions call. TS `TitleKind` gains `'system'`.
- **P-2: one `game_folder` module.** `crates/coverly-core/src/game_folder.rs` holds `FixedFolder` (re-exported as `consoles::FixedFolder`, as §5.1 names it), `DirShape` and `classify_listing(names, is_dir, is_rom_like) -> DirShape` used by `organize/walk.rs`, `scan/mod.rs` and `dedupe/units.rs`, so «what is a game folder» (D3, §8.1) has one definition and one test table.
- **P-3: base folders through `subdirs`, children after targets.** `<Game>` for a base is computed before `assign_targets_in` and passed as its `subdirs` entry (the hook Batocera's DLC path uses); the base's own file is then named `<Game>.<ext>` by a `stem_override` on the item. Children get their targets in a new `target::game_children_targets` pass after `assign_targets_in` (like `sibling_targets`), from the base's **final** folder, so a `(2)` collision on the folder carries over to every child. A child whose base is left is left with the same reason (§12 row 1).
- **P-4: library lookup for orphans and demos in one helper.** D12 (demo base by name in the target) and D13 (orphan base by `base_title_id` in the target) share `organize/library_bases.rs`: one listing of `<roms>/<System>/` per system that has an orphan or a demo, top-level entries only, header reads through the memoised dedupe facts (size + mtime), under the existing plan budget; budget exhausted → base not found (Review), never guessed.
- **P-5: the folder preview is built in TS from the plan.** `gameFolderPreview(items, base)` derives the tree from `item.target` paths and `game.of`; the core sends no tree. «ya existe» comes from a new `GameLink.folder_exists: bool` set by the planner from `DirIndex::existing` (one more field than D16 lists).

## Review Focus

1. **Keys rule.** Licences are never opened, hashed or copied: open-log assertions in Task 7 (`licence_names_are_never_opened`), Task 12 (`licence_follows_by_rename_only`, `licence_stays_in_copy_mode`, `licence_rename_exdev_leaves_it`), Task 13 (apply + undo). Global key files never match an item.
2. **Respect game folders always.** Option off, library detected as MinUI, or any profile: `Updates/`/`DLC/` files are never top-level scan entries or loose pile games; `Mods/`/`Cheats/`/`Extras/`/`Saves/` are never descended, scanned or units — but only inside a real game folder (one base at the root). `SNES/Hacks/Mods/*.sfc` and Batocera `ps4/DLC/CUSA…/` keep today's behaviour. Tests: Tasks 2, 8, 14, 15.
3. **No data loss.** Undo restores every path and removes only empty created folders; a `Saves/` added after the run keeps `<Game>/`. Duplicados never removes a base with children/fixed folders (report only) and refuses any plan removing every member of a group; the keeper rule order is untouched. Tests: Tasks 13, 15.
4. **Installed users.** Option off → existing snapshots unchanged; `coverly-library.toml` custom profile kept as `coverly-library-custom` with settings references rewritten; downgrade path (map ignored, ids unknown) documented. Tests: Tasks 7, 10.
5. **Untrusted names and paths.** `../../x` titles, trailing dots/spaces, `CON`, NFC/NFD reuse, path-length shortening of the DLC part first and never of the folder. Tests: Task 10.
6. **Windows.** Pinned mtimes, component comparisons, deeper path counted for `PathTooLong`, `scripts/dev/clippy-windows.sh` on every task.

---

## File structure

```
crates/coverly-core/
├─ src/game_folder.rs (new)        FixedFolder, DirShape, classify_listing (P-2)
├─ src/lib.rs                      pub mod game_folder
├─ src/consoles/
│  ├─ mod.rs        TitleKind::System (P-1); is_console_system_title; kind_3ds/kind_switch/kind_wii;
│  │                  base_title_id arms wii, psp; display_version; region_from_product_code,
│  │                  region_from_game_id, region_from_sony_serial (+ K → Korea in region_from_content_id);
│  │                  KEY_NAMES/KEY_EXTS additions; licence_matches; pub use crate::game_folder::FixedFolder
│  ├─ ctr.rs, wiiu.rs, nintendo_disc.rs, wad.rs   region; 3DS/Wii system titles
│  ├─ sony.rs, loose.rs, orbis.rs                 region from content id / serial; Vita ac → Dlc; PSP PG → Update
│  ├─ nx.rs                                       DataPatch, Delta, system titles
│  ├─ xbox.rs                                     STFS demo 0x00080000, version 0x358, media id 0x354; XVD updater → System
│  └─ testing.rs                                  builders: wad_head(title_id, version), psp_pbp(category, disc_id, ver),
│                                                   stfs_head(ctype, title_id, media_id, version), ctr product-code variants
├─ src/identify/{names.rs,sniff.rs}               Region::no_intro_word; NotARom::{EmulatorCache, EmulatorConfig}; System → ConsoleSystem
├─ src/scan/{mod.rs,classify.rs}                  RomEntry.{children, role, version_group, revision}; not_a_game()
├─ src/organize/
│  ├─ walk.rs        fixed folders in a pile (role hints, GameFolderContent); .PUP names; not_a_game
│  ├─ plan.rs        GameLink, GameRole, ChildRole; LeftReason::GameFolderContent; ItemNote::{DemoOf, BaseMissing};
│  │                   RenameReason::GameFolder; Companion.licence; link pass writes game; FACTS_VERSION 10
│  ├─ target.rs      TargetLayout.game_folders; game folder subdirs; stem_override; game_children_targets;
│  │                   demo folders; licence companions; deep path-length checks
│  ├─ library_bases.rs (new)  P-4
│  └─ apply.rs       licence companions rename-only; creation/removal ordering asserted
├─ src/profiles/{mod.rs,builtin.rs,custom.rs,detect.rs}  Profile.game_folders, uses_game_folders, coverly-library, migration guard
├─ src/settings/{model.rs,patch.rs,validate.rs,load.rs}  OrganizeSettings.game_folders map, normalise, reference rewrite
├─ src/dedupe/{units.rs,keys.rs,keeper.rs,apply.rs,plan.rs} roles, 360 media id, guards, FACTS_VERSION 4
└─ tests/
   ├─ game_folder.rs (new)                classify_listing table
   ├─ consoles_updates.rs (new)           kinds, regions, versions per system (table harness)
   ├─ organize_game_folders.rs (new)      plan, demos, orphans, licences, apply + undo
   ├─ scan_game_folders.rs (new)          children, orphans, skipped folders, revisions
   ├─ dedupe_game_folders.rs (new)        roles, versions, guards
   └─ custom_profiles.rs, profile_folders.rs, organize_walk.rs, organize_plan.rs, consoles_keys.rs, no_keys.rs  updated
crates/coverly-cli/src/{organize.rs,main.rs}       --game-folders / --no-game-folders; text for every new variant; version column
src-tauri/src/{ipc.rs,state.rs}, src-tauri/tests/{contract.rs,organize_state.rs}  resolved option in plan; settings patch
src/ipc/{types.ts,contract.sample.json,contract.test.ts}
src/ipc/mock/{mockOrganize.ts,gameFoldersFixture.ts (new),mockSettings.ts,fixture.ts} (+ tests)  ?mockPick=gamefolders; coverly-library
src/lib/organize.ts (+ test)                     fold by game.of; gameFolderPreview; noteText; fixedFolderLabel
src/components/organize/{SystemGroups.tsx,ItemName.tsx,GameFolderPreview.tsx (new)} (+ tests)
src/components/settings/{LibrarySection.tsx,registry.ts,Sections.test.tsx}
locales/{es,en}/ui.json, CHANGELOG.md
```

---

## Task 1: Core types, no behaviour change (enums, fields, facts, CLI text, TS types, contract)

**Files:**
- Create: `crates/coverly-core/src/game_folder.rs` (types only here; logic in Task 2)
- Modify: `crates/coverly-core/src/lib.rs`, `crates/coverly-core/src/consoles/mod.rs` (`TitleKind::System`, `pub use crate::game_folder::FixedFolder`), `crates/coverly-core/src/identify/sniff.rs` (`NotARom::{EmulatorCache, EmulatorConfig}`; `add_console` maps a lone `System` verdict to `ConsoleSystem`), `crates/coverly-core/src/organize/plan.rs`, `crates/coverly-cli/src/organize.rs` (exhaustive text), `src-tauri/src/ipc.rs` (exhaustive matches, if any), `src/ipc/types.ts`, `src/ipc/contract.sample.json`, `src/ipc/contract.test.ts`, `src-tauri/tests/contract.rs`, `src/lib/organize.ts` (exhaustive `switch` placeholders returning today's generic text until Task 18)
- Test: unit tests in `game_folder.rs`, `plan.rs`, `sniff.rs`; CLI text tests; `contract.test.ts`

**Interfaces (produced):**

```rust
// game_folder.rs
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FixedFolder { Updates, Dlc, Mods, Cheats, Extras, Saves }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ChildRole { Update, Dlc }

// organize/plan.rs
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GameRole { Base, Update, Dlc, Demo }

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameLink {
    pub role: GameRole,
    /// Display form (D11), never the raw header value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<crate::identify::names::Region>,
    /// The base item's id in this plan.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub of: Option<usize>,
    /// The game folder, relative to the target root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<PathBuf>,
    /// The game folder was already on disk (P-5).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub folder_exists: bool,
    /// A demo's base title, linked by name only (D12).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub demo_of: Option<String>,
    /// Highest version present for this base (D24).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub newest_here: bool,
}
// OrganizeItem: #[serde(default, skip_serializing_if = "Option::is_none")] pub game: Option<GameLink>,
// LeftReason::GameFolderContent { folder: FixedFolder, of: String }
// ItemNote::DemoOf { of: String }, ItemNote::BaseMissing
// RenameReason::GameFolder { from: String }
// Companion: #[serde(default, skip_serializing_if = "std::ops::Not::not")] pub licence: bool,
// FACTS_VERSION 9 → 10 (the unit test pinning 9 now pins 10)
```

```ts
// src/ipc/types.ts
export type TitleKind = 'base' | 'update' | 'dlc' | 'demo' | 'homebrew' | 'system' | 'unknown';
export type FixedFolder = 'updates' | 'dlc' | 'mods' | 'cheats' | 'extras' | 'saves';
export type GameRole = 'base' | 'update' | 'dlc' | 'demo';
export interface GameLink {
  role: GameRole; version?: string; region?: Region; of?: number;
  folder?: string; folder_exists?: boolean; demo_of?: string; newest_here?: boolean;
}
// OrganizeItem.game?: GameLink; LeftReason { kind: 'game-folder-content'; folder: FixedFolder; of: string };
// NotARom += 'emulator-cache' | 'emulator-config'; ItemNote { kind: 'demo-of'; of: string } | { kind: 'base-missing' };
// RenameReason { kind: 'game-folder'; from: string }; Companion.licence?: boolean
```

- [ ] **Step 1: Failing tests.** Serde round trips for every new variant and field (kebab tags; `game` absent when `None`; `licence` absent when `false`); a plan JSON from before (no `game`, no `licence`) still deserialises; `TitleKind::System` serialises `"system"`; `Sniff::add_console(vec![id_with_kind(System)])` sets `not_a_rom = Some(ConsoleSystem)` and records no hit; `FACTS_VERSION == 10`. CLI: `left_text(GameFolderContent{Mods,"Pokemon Y"})` and the note/rename texts are non-empty English. `contract.test.ts`: the sample holds one item with `game`, one `game-folder-content` left reason, one licence companion.
- [ ] **Step 2: Run** `cargo test -p coverly-core --lib -- game_folder:: organize::plan:: identify::sniff:: && cargo test -p coverly-cli && cargo test -p coverly-roms --test contract && pnpm vitest run src/ipc/contract.test.ts` — expect failures.
- [ ] **Step 3: Implement** the types, the CLI texts (`"kept in place: the {folder} folder of {of} stays for now"`, `"demo of {of}"`, `"its game is not here or in the library"`, `"renamed for its game folder (was {from})"`, `"emulator cache"`, `"emulator settings"`), the TS types and sample, and placeholder arms in `src/lib/organize.ts` so `pnpm typecheck` passes.
- [ ] **Step 4: Run** Step 2, then G-RUST and G-WEB.
- [ ] **Step 5: Commit and push:** `git add -A crates src-tauri src && git commit -m "feat(core): add game folder, game link and system title types" && git push && git status -sb`

---

## Task 2: What a game folder is (`FixedFolder` names, `classify_listing`)

Spec D3, §8.1.

**Files:**
- Modify: `crates/coverly-core/src/game_folder.rs`
- Create: `crates/coverly-core/tests/game_folder.rs`

**Interfaces (produced):**

```rust
impl FixedFolder {
    pub const ALL: [FixedFolder; 6] = [Self::Updates, Self::Dlc, Self::Mods, Self::Cheats, Self::Extras, Self::Saves];
    /// Exact on-disk name: `Updates`, `DLC`, `Mods`, `Cheats`, `Extras`, `Saves`.
    pub fn dir_name(self) -> &'static str;
    /// Case-insensitive and exact: `updates`, `dlc` match; `Update`, `DLCs`, `Updates (old)` do not.
    pub fn from_dir_name(name: &str) -> Option<FixedFolder>;
    pub fn child_role(self) -> Option<ChildRole>; // Updates → Update, Dlc → Dlc, others None
}

/// One directory's shape from one listing (no file opened).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DirShape {
    /// Exactly one base at the root. `fixed` holds every fixed folder found,
    /// with its on-disk spelling(s), sorted by bytes (D3: `DLC/` and `dlc/` both count).
    GameFolder { base: GameBase, fixed: Vec<(FixedFolder, String)> },
    /// Fixed folders but zero or several bases: not a game folder (D3).
    Loose { fixed: Vec<(FixedFolder, String)> },
    /// No fixed folder at all.
    Plain,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GameBase { SelfIsFolderGame, File(String), Sheet(String) }

/// `entries`: (name, is_dir) of one listing; `rom_like(name)` says whether a
/// root file is a ROM-like candidate (extension known to the system db, not
/// a companion, not key material); `sheet(name)` whether it is a cue/gdi/ccd/m3u;
/// `folder_game`: a probe already recognised the directory itself.
pub fn classify_listing(
    entries: &[(String, bool)],
    rom_like: &dyn Fn(&str) -> bool,
    sheet: &dyn Fn(&str) -> bool,
    folder_game: bool,
) -> DirShape;

/// True when `dir`'s parent is a system folder and `dir` itself is named
/// like a fixed folder (Batocera `ps4/DLC/`): never a game's (D3).
pub fn is_system_level_fixed(dir_name: &str, parent_is_system: bool) -> bool;
```

A sheet set (one `.cue` plus its tracks, or one `.m3u` with its discs) counts as one base: when a sheet is present, files referenced by sheets are not counted again (the caller passes `rom_like` that already excludes track extensions when a sheet exists; document it on the function).

- [ ] **Step 1: Failing tests** (`tests/game_folder.rs`, table-driven): names (`Updates`, `UPDATES`, `dlc` match; `Update`, `DLCs`, `Updates (old)`, `Saves ` no match); shapes: `[Pokemon.cia, Updates/]` → GameFolder File; `[Game.cue, Game (Track 1).bin, Game (Track 2).bin, DLC/]` → GameFolder Sheet; folder game + `Updates/` → GameFolder SelfIsFolderGame; `[a.sfc, b.sfc, Mods/]` → Loose; `[Mods/]` → Loose; `[a.sfc]` → Plain; `[Game.cia, DLC/, dlc/]` → fixed lists both spellings, `DLC` first by bytes; `is_system_level_fixed("DLC", true)`.
- [ ] **Step 2: Run** `cargo test -p coverly-core --test game_folder` — expect failures.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** Step 2, G-RUST.
- [ ] **Step 5: Commit and push:** `git add -A crates && git commit -m "feat(core): name the fixed game folders" && git push && git status -sb`

---

## Task 3: Regions from product codes, game ids and Sony ids; `Region::no_intro_word`

Spec D10.

**Files:**
- Modify: `crates/coverly-core/src/identify/names.rs` (`Region::no_intro_word`), `crates/coverly-core/src/consoles/mod.rs` (three helpers, `K → Korea` in `region_from_content_id`), `ctr.rs`, `wiiu.rs`, `nintendo_disc.rs`, `wad.rs`, `sony.rs`, `loose.rs` (PBP), `orbis.rs` (unchanged path, uses the extended helper), `consoles/testing.rs` (product-code variants)
- Create: `crates/coverly-core/tests/consoles_updates.rs` (table harness reused by Tasks 4–6)

**Interfaces (produced):**

```rust
impl Region { /// Inverse of `from_no_intro`: `Usa` → "USA", `Europe` → "Europe", `Japan` → "Japan"…
    pub fn no_intro_word(self) -> &'static str; }

/// 3DS `CTR-P-AXXE` / Wii U `WUP-P-AXXP`: last letter of the 4-char game code.
pub fn region_from_product_code(code: &str) -> Option<Region>;
/// GameCube/Wii disc id or WAD game code: 4th character.
pub fn region_from_game_id(id: &str) -> Option<Region>;
/// PS3/PSP serials `[BN][LCP]xx…` / `U[LC]xx…`: third letter. Vita `PCSx` → None.
pub fn region_from_sony_serial(serial: &str) -> Option<Region>;
```

Letter table (shared by the first two): `E` Usa, `P` Europe, `J` Japan, `K` Korea, `W` Taiwan, `C` China, `A` World, `D` Germany, `F` France, `S` Spain, `I` Italy, `U` Australia, `H` Netherlands; anything else `None`. Sony serial third letter: `U` Usa, `E` Europe, `J` Japan, `A` Asia, `K` Korea, `H` HongKong (or `Asia` if `Region` has no Hong Kong value — check `Region` and pick the existing variant; record which in the commit body). A letter table not in `docs/02-research/` cites its source in a code comment or stays `None` (D10).

Reader wiring: `ctr.rs` and `wiiu.rs` set `id.region` from the product code of **base** titles only (never a child's own product code, D10); `nintendo_disc.rs` from the disc id; `wad.rs` from the game code; `sony.rs` PKG header content id (`+0x30`) and PSF `CONTENT_ID` through `region_from_content_id`, else the serial for `ps3`/`psp`; `loose.rs` PBP `DISC_ID` for `psp`. Switch, Xbox 360, Xbox One/Series: none.

- [ ] **Step 1: Failing tests.** `names.rs`: `no_intro_word` round trip for every `Region` value through `from_no_intro`. `consoles/mod.rs`: the three helpers over the tables (including `CTR-P-EK2A` → World, `WUP-P-ARDP` → Europe, `RMCE01` → Usa, `BLUS30001` → Usa, `BLES00001` → Europe, `ULJM05000` → Japan, `PCSE00123` → None, `K` content id → Korea). `tests/consoles_updates.rs`: a `REGIONS` table runs each builder through `consoles::identify` and checks `region` (3DS base, 3DS update → None, Wii U base, GameCube disc, WAD, PS3 PKG, PSP PBP, Vita PSF without content id → None).
- [ ] **Step 2: Run** `cargo test -p coverly-core --lib -- identify::names consoles:: && cargo test -p coverly-core --test consoles_updates` — expect failures.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** Step 2, G-RUST.
- [ ] **Step 5: Commit and push:** `git add -A crates && git commit -m "feat(core): read regions from product codes, game ids and Sony ids" && git push && git status -sb`

---

## Task 4: Nintendo kinds: 3DS and Switch system titles, Switch DataPatch and Delta, Wii WADs

Spec D14 (a), (b), (c), (d).

**Files:**
- Modify: `crates/coverly-core/src/consoles/mod.rs` (`is_console_system_title`, `kind_3ds`, `kind_switch`, `kind_wii`, `base_title_id` arm `wii`), `nx.rs`, `wad.rs`, `consoles/testing.rs` (`wad_head(title_id, version)`), `crates/coverly-core/tests/consoles_updates.rs`, `crates/coverly-core/tests/adjustments.rs` (rows whose kind changes, if any: read the diff)

**Rules:**

```rust
pub fn is_console_system_title(system: &str, title_id: u64) -> bool {
    match system {
        "3ds" => high(title_id) & 0x10 != 0,               // 00040010, 0004001B, 00040030, 0004009B, 000400DB, 00040130, 00040138
        "switch" => title_id < 0x0100_0000_0001_0000,
        "wii" => matches!(high(title_id), 0x0000_0001 | 0x0001_0002 | 0x0001_0008),
        _ => false,
    }
}
// kind_3ds: system check first → TitleKind::System, then today's arms.
// kind_switch: system → System; low 0x1FFF == 0 → Base; 0x800 → Update; 0xC00 → Update (Delta);
//   x & 0x1000 != 0 (includes DataPatch 0x1801..=0x1FD0) → Dlc. switch_base unchanged (gives
//   ((id − 0x800) − 0x1000) & !0xFFF for a DataPatch: assert it in a test).
// kind_wii: system → System; 00010001 → Base; 00010005 → Dlc; 00010004 → Unknown.
// base_title_id "wii": the 4-character game code (low 32 bits as ASCII), first letter upper-cased
//   (DLC 00010005-wF4E → "WF4E"); a disc id's first 4 chars are compared with it by the linker.
```

A disc game's `base_title_id` for `wii` is the first four characters of its disc id upper-cased, so a WAD DLC links to a disc in `updates_after_bases` (D14 d). Two WADs with one title id are versions, not base + update: both stay `Base`; the TMD version goes into `ConsoleId.version` (raw u16 decimal).

- [ ] **Step 1: Failing tests** in `consoles/mod.rs` (`kind_3ds_system_titles_are_system`, `kind_switch_datapatch_and_delta`, `switch_system_titles`, `base_title_id_wii_wad_dlc_meets_disc_and_channel`) and a `KINDS` table in `tests/consoles_updates.rs` through the readers (`nx.rs` with a `cnmt.xml` in clear for a DataPatch; `wad.rs` DLC, channel, system channel; 3DS CIA of `0004001B…` → `Sniff.not_a_rom == Some(ConsoleSystem)`).
- [ ] **Step 2: Run** `cargo test -p coverly-core --lib consoles:: && cargo test -p coverly-core --test consoles_updates --test adjustments` — expect failures.
- [ ] **Step 3: Implement.** Read any `adjustments.rs`/`consoles_loose.rs` diff: only rows of system titles may change, to `System`.
- [ ] **Step 4: Run** Step 2, G-RUST.
- [ ] **Step 5: Commit and push:** `git add -A crates && git commit -m "feat(core): classify Nintendo system titles, Switch data patches and Wii DLC" && git push && git status -sb`

---

## Task 5: Sony and Xbox kinds: Vita DLC, PSP updates, Xbox 360 demos/versions/media id, XVD updaters

Spec D14 (e)–(j), D11 (raw storage).

**Files:**
- Modify: `crates/coverly-core/src/consoles/sony.rs` (`("psvita", Some("ac")) => Dlc`), `loose.rs` (PBP: `CATEGORY PG` → `Update`; a file named `PBOOT.PBP` → `Update`; `UG`/`EG` → Base; `DISC_VERSION`/`APP_VER` into `version`), `consoles/mod.rs` (`base_title_id` arm `psp` = `DISC_ID`/title id upper-cased), `xbox.rs` (STFS `0x0008_0000` → `Demo`; `be32(h, 0x358)` stored as `format!("{:08X}")` in `version`; media id `be32(h, 0x354)` into a new `ConsoleId.media_id: Option<String>` `{:08X}`, serde default skip; XVD content types `0x0B`/`0x0C` → `System`), `consoles/testing.rs` (`stfs_head(ctype, title_id, media_id, version)`, `psp_pbp(category, disc_id, version)`), `tests/consoles_updates.rs`
- PSP PKG stays `Unknown` (encrypted `PARAM.SFO`, D14 g). PS3 and PS4/PS5 unchanged except that a licence-only `AL` DLC PKG is still read as a DLC PKG (D14 h): add a fixture row that pins it.

- [ ] **Step 1: Failing tests** (`KINDS` rows): Vita PSF `ac` → Dlc; PSP PBP `PG` → Update with `base_title_id == "ULUS10041"`; `PBOOT.PBP` named file → Update; STFS demo → Demo; STFS version `0x20000C00` → `version == "20000C00"`, `media_id == "4D5307E6"`; XVD `0x0B` → `Sniff.not_a_rom == ConsoleSystem`; PS4 `AL` PKG → Dlc. `dedupe::keeper::version_order("xbox360", "20000C00")` still parses.
- [ ] **Step 2: Run** `cargo test -p coverly-core --test consoles_updates --test consoles_current_gen && cargo test -p coverly-core --lib consoles::` — expect failures.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** Step 2, G-RUST.
- [ ] **Step 5: Commit and push:** `git add -A crates && git commit -m "feat(core): detect Vita DLC, PSP updates, Xbox 360 demos and versions" && git push && git status -sb`

---

## Task 6: Display versions

Spec D11.

**Files:**
- Modify: `crates/coverly-core/src/consoles/mod.rs` (`display_version`), `tests/consoles_updates.rs`

```rust
/// Human form of a raw `ConsoleId.version` for names and the UI (D11);
/// comparison stays `dedupe::keeper::version_order` over the raw value.
pub fn display_version(system: &str, raw: &str) -> Option<String>;
// 3ds: u16 → major.minor.micro from 6/6/4 bits (1040 → "1.1.0") — medium confidence: if not in
//      docs/02-research, keep the raw decimal and add BLOCKED to the commit body.
// wiiu: raw decimal ("32"); switch: raw decimal ("131072");
// ps3/psvita/ps4/psp: as written ("01.05"); ps5: as written ("01.000.000");
// xbox360: hex u32 → major.minor.build.qfe from 4/4/16/8 bits ("20000C00" → "2.0.12.0");
// xboxone/xboxseries: four dot-separated u16 as stored;
// gamecube/wii disc: "Rev N" when N > 0, None when 0. Empty or unparsable → None.
```

- [ ] **Step 1: Failing test** `display_version_per_system` (table above, plus `None` cases).
- [ ] **Step 2: Run** `cargo test -p coverly-core --lib consoles::tests::display_version` — expect failure.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** Step 2, G-RUST.
- [ ] **Step 5: Commit and push:** `git add -A crates && git commit -m "feat(core): show console versions in their community form" && git push && git status -sb`

---

## Task 7: Licences, firmware, shader caches and emulator configs by name

Spec D17 (names only), D18.

**Files:**
- Modify: `crates/coverly-core/src/consoles/mod.rs` (`KEY_NAMES` += `keys.txt`, `otp.bin`, `ticket.db`, `eeprom.bin`; `KEY_EXTS` += `edat`; doc comment updated: `keys.txt` now listed on the owner's brief; `licence_matches`), `crates/coverly-core/src/scan/classify.rs` (`not_a_game`), `crates/coverly-core/src/organize/walk.rs` (call `not_a_game` before the size check; drop new key names from probe listings), `crates/coverly-core/tests/consoles_keys.rs`, `tests/no_keys.rs`, `tests/organize_walk.rs`, `locales/{es,en}/ui.json` (`organize.left.keyMaterial`, `organize.left.emulatorCache`, `organize.left.emulatorConfig` per spec §10)

**Interfaces (produced):**

```rust
/// Keys an item offers for licence matching (D17): its content id, its
/// title id, the first 16 hex characters of a Switch rights id, its stem.
pub struct LicenceKeys<'a> { pub content_id: Option<&'a str>, pub title_id: Option<&'a str>, pub stem: &'a str }
/// Whether licence file `name` belongs to the item (case-insensitive stem
/// equality). Never touches the file system. Global key files never match.
pub fn licence_matches(name: &str, item: &LicenceKeys) -> bool;

/// Firmware packages, shader caches and per-game emulator configs (D18), by
/// name and parent folder names only.
pub fn not_a_game(path: &Path) -> Option<NotARom>;
// PS3UPDAT.PUP, PS4UPDATE.PUP, PS5UPDATE.PUP, PSP2UPDAT.PUP → ConsoleSystem (never EBOOT.PBP);
// any ancestor named shadercache | shader_cache | ShaderCache | GLCache | vulkan_pipeline_cache
//   (case-insensitive) or ext shadercache | pipelinecache → EmulatorCache;
// ext opt | rmp | slangp | glslp | cgp; GameSettings/*.ini; gameProfiles/*.ini;
//   config/custom_configs/*.yml; *_ppsspp.ini; custom/*.ini → EmulatorConfig.
```

- [ ] **Step 1: Failing tests.** `consoles_keys.rs`: `is_key_material_new_names` (`KEYS.TXT`, `otp.bin`, `ticket.db`, `eeprom.bin`, `Game.edat`); `licence_matches_by_name_only` (`BLUS30001.rap` vs content id `UP9000-BLUS30001_00-…` → match by title id; `Game.rap` vs stem `Game`; `prod.keys` never; `0100ABCD12340000.tik` vs Switch title id). `licence_names_are_never_opened`: walk a temp pile with every licence name under `start_open_log`, assert none in `take_open_log()` and each left `KeyMaterial`. `organize_walk.rs`: `PS3UPDAT.PUP` → `NotARom::ConsoleSystem`; `shadercache/x.bin` → `EmulatorCache`; `Game.opt` → `EmulatorConfig`; a PSP `EBOOT.PBP` still identified as a game. A test pins that no folder probe sees the new key names in its listing.
- [ ] **Step 2: Run** `cargo test -p coverly-core --test consoles_keys --test no_keys --test organize_walk && cargo test -p coverly-core --lib scan::classify` — expect failures.
- [ ] **Step 3: Implement**, update the two locale files (`keyMaterial` text changes to «Archivo de claves o licencia: Coverly no lo lee y lo deja aquí» / «Key or licence file: Coverly never reads it and leaves it here»; new `emulatorCache`/`emulatorConfig`).
- [ ] **Step 4: Run** Step 2, G-RUST, G-WEB.
- [ ] **Step 5: Commit and push:** `git add -A crates locales && git commit -m "feat(core): recognise licences, firmware, shader caches and emulator configs by name" && git push && git status -sb`

---

## Task 8: Respect fixed folders when walking a pile

Spec D7, §8.1.

**Files:**
- Modify: `crates/coverly-core/src/organize/walk.rs` (before the console probes: `classify_listing` on each pile directory's existing `read_dir`; `Updates/`/`DLC/` walked with `role_hint` and `base_dir`; `Mods/`/`Cheats/`/`Extras/`/`Saves/` of a `GameFolder` not descended, one left row each), `crates/coverly-core/src/organize/plan.rs` (`SurveyEntry.role_hint: Option<ChildRole>`, `SurveyEntry.game_dir: Option<PathBuf>`; a header kind wins over the hint: `effective_kind(entry)` = header kind if not `Unknown`, else the hint), `tests/organize_walk.rs`, `tests/organize_game_folders.rs` (create)

`LeftReason::GameFolderContent { folder, of }`: `of` is the base's stem (file base) or the folder name (folder game). For a `Loose` shape (no single base) the `Updates/`/`DLC/` hints still apply and `Mods/`-like folders are walked as today (§8.1). A fixed folder directly under a pile root that is itself a system folder of the target is unaffected (target system folders are `sorted` and never descended already).

- [ ] **Step 1: Failing tests.** Pile `Pokemon/` = `[Pokemon.cia, Updates/u.cia, DLC/d.cia, Mods/m.ips, Saves/s.sav, Extras/ost.cia]` → `u`/`d` candidates with hints, three `GameFolderContent` rows (`Mods`, `Saves`, `Extras`) and nothing from inside them (no `NotARom::Patch` row for `m.ips`); `Hacks/` = `[a.sfc, b.sfc, Mods/x.sfc]` → `x.sfc` is a candidate as today; a 3DS update header inside `DLC/` stays `Update` (header wins); unknown file in `DLC/` gets role `Dlc`; the same with the option off (walk ignores the option).
- [ ] **Step 2: Run** `cargo test -p coverly-core --test organize_walk --test organize_game_folders` — expect failures.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** Step 2, G-RUST (existing walk snapshots must not change: read any diff).
- [ ] **Step 5: Commit and push:** `git add -A crates && git commit -m "feat(core): leave fixed game folders alone when walking a pile" && git push && git status -sb`

---

## Task 9: Profile option, the Coverly library profile and the settings override

Spec D1, D2, §6.

**Files:**
- Modify: `crates/coverly-core/src/profiles/mod.rs` (`Profile.game_folders: bool`, serde default; `uses_game_folders(&self, &OrganizeSettings) -> bool`), `profiles/builtin.rs` (`coverly-library` + `game_folders: false` on every other built-in), `profiles/custom.rs` (TOML key `game_folders`, written only when `true`; «duplicar perfil» copies it; migration guard), `profiles/detect.rs` (`rules_for("coverly-library")` → no markers), `settings/model.rs` (`OrganizeSettings.game_folders: BTreeMap<String, bool>`, `#[serde(default, skip_serializing_if = "BTreeMap::is_empty")]`), `settings/patch.rs` (`organize.game_folders` patch: set one key, `null` removes it), `settings/validate.rs`/`load.rs` (normalise drops unknown profile ids; reference rewrite for the migration guard), `crates/coverly-core/tests/custom_profiles.rs`, `tests/profile_folders.rs`, `src-tauri/tests/contract.rs`, `src/ipc/contract.sample.json` (built-in list gains `coverly-library`)

```rust
// builtin.rs
Profile {
    id: "coverly-library".into(),
    name: "Coverly".into(),
    roms_root: "ROMS".into(),
    art_dir_names: vec![".media".into()],
    // slots: {system_dir}/{rel_dir}/.media/{stem}.png, -screenshot, -title
    system_folder: /* same FolderNaming as anbernic-stock */,
    host_folders: BTreeMap::new(),               // no device merges (Q12)
    disc_layout: DiscLayout::GameFolder { m3u_for_multi: true },
    accepts_containers: vec![ContainerKind::Zip, ContainerKind::SevenZ],
    game_folders: true,
    ..
}
```

Migration guard (`ProfileStore::load`): a custom file whose id equals `coverly-library` is loaded as `coverly-library-custom` (name unchanged) with one warning, and the normalise pass rewrites `organize.default_layout`, `library.default_profile`, keys of `organize.folder_names`, `organize.arcade_order` and `organize.game_folders` from `coverly-library` to `coverly-library-custom` **only when** that custom profile was renamed in this load. Other built-in ids keep today's «dropped with warning» behaviour.

- [ ] **Step 1: Failing tests.** `profile_folders.rs`: `coverly-library` values (no host merges, `PS4`/`3DS`/`SFC` folders like `anbernic-stock`, `GameFolder` disc layout, `game_folders` on); every other built-in off. `custom_profiles.rs`: TOML round trip (`game_folders = true` read; written only when true); `coverly-library.toml` custom kept as `coverly-library-custom` and a settings file with `default_layout = "coverly-library"` rewritten to it; `uses_game_folders` precedence (override true/false over the profile value); normalise drops `game_folders["nope"]`; `detect()` never returns `coverly-library`. Contract: built-ins list includes it.
- [ ] **Step 2: Run** `cargo test -p coverly-core --test custom_profiles --test profile_folders --test profile_fit && cargo test -p coverly-core --lib settings:: profiles:: && cargo test -p coverly-roms --test contract` — expect failures.
- [ ] **Step 3: Implement.** Record `NEEDS-OWNER: Q5, Q7, Q11, Q12 — defaults of spec §13` in the commit body.
- [ ] **Step 4: Run** Step 2, G-RUST, G-WEB (sample changed).
- [ ] **Step 5: Commit and push:** `git add -A crates src-tauri src && git commit -m "feat(core): add the game folders option and the Coverly library profile" && git push && git status -sb`

---

## Task 10: Plan bases, updates and DLC into game folders (link pass, targets, names)

Spec D5, D6, D16, D20, D21 (path length), D24 (newest here), P-3.

**Files:**
- Modify: `crates/coverly-core/src/organize/plan.rs` (`OrganizeOptions.game_folders: Option<bool>` — `None` = the profile's resolved value; link pass writes `item.game` for bases and children from `updates_after_bases`/`note_updates`; `effective_kind` from Task 8; game folder `subdirs` before `assign_targets_in`; `game_children_targets` after `sibling_targets`; `newest_here`), `crates/coverly-core/src/organize/target.rs` (`TargetLayout.game_folders: bool`; `GAME_FOLDER_SYSTEMS`; `game_folder_name`; `stem_override`; `game_children_targets`; deep `check_lengths`), `crates/coverly-core/tests/organize_game_folders.rs`, `tests/organize_target.rs` (layout literal gains the field), `tests/organize_plan.rs`

```rust
/// Systems whose games get a folder when the option is on (D5, D14).
pub const GAME_FOLDER_SYSTEMS: &[&str] = &[
    "3ds", "switch", "wiiu", "wii", "ps3", "psvita", "psp", "ps4", "ps5", "xbox360", "xboxone", "xboxseries",
];

/// `<Game>`: the base's final stem (NameMode applied), plus ` (<Region>)` in
/// No-Intro words when the stem has no region tag (D10); then safe-name,
/// trailing dot/space trim, NFC; a taken name gets ` (2)` via `DirIndex`
/// (an existing directory is reused, a file or symlink is a collision, D20).
fn game_folder_name(stem: &str, region: Option<Region>, layout: &TargetLayout) -> String;

/// After `assign_targets_in`: every child whose `game.of` base was placed in
/// a game folder gets `<folder>/Updates/<Game> (v<display>).<ext>` or
/// `<folder>/DLC/<Game> - <DLC name>.<ext>` (`RenameReason::GameFolder { from }`),
/// `folder` and `folder_exists` copied from the base; a left base leaves its
/// children with the same reason (§12). Fixed folder spelling: an existing
/// case-variant is reused (first by bytes), never a third created; a file named
/// `Updates` → the child is left `UnsafeTarget`. Inside `Updates/`, same size and
/// fingerprint → `AlreadyPresent`, else ` (2)` and Review (D20).
pub fn game_children_targets(items: &mut [OrganizeItem], layout: &TargetLayout, index: &mut DirIndex, same_content: &dyn Fn(&Path, &OrganizeItem) -> bool);
```

`<DLC name>` (D6): the DLC's own header title when it differs from the base title, else the file stem with the base title and every `(…)`/`[…]` group stripped when anything is left, else `DLC <title id>`; always `clean_title` + safe-name. Path length: shorten the `<DLC name>`, then the repeated `<Game>` prefix in the file name, never the folder; still too long → `PathTooLong`. A folder game keeps its own folder as the game folder: base → `<System>/<Game>/` (renamed only with catalogue names), its update/DLC folders → `<Game>/Updates/<folder name>/` and `<Game>/DLC/<folder name>/`. Retro systems (not in `GAME_FOLDER_SYSTEMS`) and the option off: no change. Profiles with their own DLC rule (Batocera `dlc_folder`, shadPS4 siblings) keep it.

- [ ] **Step 1: Failing tests** (`organize_game_folders.rs`, layout `coverly-library`, names «conservar», mtimes pinned): 3DS base `Pokemon Y.cia` (`CTR-P-EK2A`) + update `0004000E00055D00.cia` (TMD 1040) + two DLC → targets of spec §7 compared by components, `game.of` = base id, `game.version == "1.1.0"` (or raw if Task 6 kept raw), `RenameReason::GameFolder`; `insta` snapshot of the whole plan; **option off → the existing flat snapshot unchanged**; ES-DE/Anbernic profile → flat; Switch update `[v131072]` → `Updates/Zelda (v131072).nsp` (no region); PS3 JB folder with an update folder → nested under `Updates/`; existing `<Game>/` reused with its casing; a file named `<Game>` → `<Game> (2)/` and its children follow; case-different `updates/` reused; DLC `TITLE` `../../x` stays below `DLC/`; stem ending in `.` and title `CON` get the safe forms; NFC/NFD names reuse one folder; Windows path limit (`set_windows_paths_override(Some(true))`) shortens the DLC part first; Wii DLC `wF4E` WAD folds under disc `WF4E01`; a 360 `Demo` with the base's title id is **not** folded; two bases titled «Tetris» with different title ids → two folders, children stay with their own base; base left `TooBigForFat` → its update left too; `newest_here` on the highest of two updates.
- [ ] **Step 2: Run** `cargo test -p coverly-core --test organize_game_folders --test organize_target --test organize_plan --test organize_overlays` — expect failures.
- [ ] **Step 3: Implement.** Record `NEEDS-OWNER: Q2, Q3, Q4, Q8 — defaults of spec §13` in the commit body.
- [ ] **Step 4: Run** Step 2, G-RUST (every pre-existing snapshot unchanged: read the diff).
- [ ] **Step 5: Commit and push:** `git add -A crates && git commit -m "feat(core): plan bases, updates and DLC into game folders" && git push && git status -sb`

---

## Task 11: Demos as their own game, orphans, and the library lookup

Spec D12, D13, P-4.

**Files:**
- Create: `crates/coverly-core/src/organize/library_bases.rs`
- Modify: `crates/coverly-core/src/organize/mod.rs`, `plan.rs` (demo linking by `parse_tags(stem).title_key`, `ItemNote::DemoOf`, `GameLink { role: Demo, demo_of }`; orphan detection, `ItemNote::BaseMissing`, Review), `target.rs` (demo folder `<Name> (Demo)/<Name> (Demo).<ext>`, ` (Demo)` added only when no `Demo`/`Trial` tag), `tests/organize_game_folders.rs`

```rust
pub struct LibraryBase { pub path: PathBuf, pub in_game_folder: bool, pub title: Option<String> }
/// Top-level entries of `<roms>/<System>/` for the systems that have an
/// orphan or a demo in this plan: game folders by name, files by
/// `base_title_id` through the memoised dedupe facts (size + mtime keyed),
/// within `budget`; `None` from a lookup after the budget ran out.
pub fn library_bases(system_dir: &Path, system: &str, db: &SystemDb, budget: &mut PlanBudget) -> LibraryIndex;
impl LibraryIndex {
    pub fn by_title_id(&self, base_id: &str) -> Option<&LibraryBase>;
    pub fn by_title_key(&self, key: &str) -> Option<&LibraryBase>;
}
```

Orphan rules (D13, §12): base found as a game folder → child joins `<that folder>/Updates|DLC/…` (Auto); base found flat (`3DS/Pokemon Y.cia`) → child goes flat beside it as today, Auto, no `BaseMissing`, no folder created (D19); base not found, trusted title known (own header title, catalogue name) → `<System>/<Title>/Updates|DLC/…`, **Review**, `BaseMissing`; no title → flat, Review, `BaseMissing`. Demo: own folder always (option on); `DemoOf` when a base with the same `title_key` is in the plan or the library; never under the base's folder.

- [ ] **Step 1: Failing tests.** Demo by header (3DS `00040002`) and by `(Demo)` tag (PS3 PKG) → `PS3/Resistance (USA) (Demo)/…`, `DemoOf` when `Resistance (USA)` is in the plan; `(Kiosk Demo)` keeps its name; orphan with title → Review + folder; orphan without title → flat + Review; target already holds `3DS/Pokemon Y (World)/` → orphan joins it, Auto; target holds flat `3DS/Pokemon Y.cia` → update flat, Auto, no note; budget exhausted (tiny budget) → Review; a library folder with 2 000 files reads headers only for systems with orphans (count reads through the facts hook).
- [ ] **Step 2: Run** `cargo test -p coverly-core --test organize_game_folders` — expect failures.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** Step 2, G-RUST.
- [ ] **Step 5: Commit and push:** `git add -A crates && git commit -m "feat(core): plan demos as their own games and place orphan updates" && git push && git status -sb`

---

## Task 12: A licence moves with its game only when unambiguous

Spec D17 (companions and apply).

**Files:**
- Modify: `crates/coverly-core/src/organize/target.rs` (`licence_companions` after targets: move mode + same volume + exactly one matching item in the same source directory → `Companion { licence: true }` into the item's folder beside its file; else the licence keeps `LeftReason::KeyMaterial`), `organize/plan.rs` (call order: after `game_children_targets`, before the dedupe step), `organize/apply.rs` (a licence companion only by exclusive rename; any failure — `EXDEV`, name taken, permission — leaves it in place with its row and does not undo the base; no hash, no fingerprint beyond `symlink_metadata`, no comparison with an existing target), `tests/organize_game_folders.rs`

- [ ] **Step 1: Failing tests** (all under `start_open_log`, asserting no licence path is opened in plan **or** apply): `licence_follows_by_rename_only` (`BLUS30001.rap` beside `Resistance.pkg`, move mode, same volume → `licence: true`, moved, undone); `licence_stays_in_copy_mode`; `licence_stays_when_ambiguous` (two items match); `prod.keys` beside a game → stays; `licence_rename_exdev_leaves_it` (inject the rename failure through the existing apply fault hook used by `organize_apply.rs`; if none exists, add a `#[cfg(feature = "test-util")]` hook) → licence stays, base moved, row says so; an existing file at the licence target is never opened or compared.
- [ ] **Step 2: Run** `cargo test -p coverly-core --features test-util --test organize_game_folders --test organize_apply` — expect failures.
- [ ] **Step 3: Implement.** Record `NEEDS-OWNER: Q6, Q9 — defaults of spec §13` in the commit body.
- [ ] **Step 4: Run** Step 2, G-RUST.
- [ ] **Step 5: Commit and push:** `git add -A crates && git commit -m "feat(core): move a licence with its game when unambiguous" && git push && git status -sb`

---

## Task 13: Apply and undo round trip for game folders

Spec D4, D21, §8.3.

**Files:**
- Modify: `crates/coverly-core/src/organize/apply.rs` (ordering: `DirCreated` for `<Game>/` before `Updates/`/`DLC/`, before any child move; assert no `Mods/`/`Cheats/`/`Extras/`/`Saves/` creation), `crates/coverly-core/src/write/undo_organize.rs` (children back first, then base, then empty `DLC/`, `Updates/`, `<Game>/`; a non-empty folder is kept and reported with the existing «carpeta no vacía» outcome), `tests/organize_game_folders.rs`, `tests/organize_undo.rs`

- [ ] **Step 1: Failing tests.** Apply the Task 10 plan in a temp dir (move mode) → every path by components; journal holds exactly `DirCreated` for the folders used (no DLC folder when there is no DLC); undo restores every source path and removes the created folders; a user file dropped into `<Game>/Saves/` after apply → undo keeps `<Game>/` and `Saves/`, reports it; copy mode round trip; volume-gone mid-apply (existing harness) → resume finishes the children, undo reverses what moved; option toggled between plan and apply → apply follows the plan.
- [ ] **Step 2: Run** `cargo test -p coverly-core --features test-util --test organize_game_folders --test organize_undo --test journal_resume` — expect failures (ordering) or passes; if all pass already, keep the tests and note it in the commit body.
- [ ] **Step 3: Implement** any ordering fix.
- [ ] **Step 4: Run** Step 2, G-RUST.
- [ ] **Step 5: Commit and push:** `git add -A crates && git commit -m "test(core): apply and undo game folders end to end" && git push && git status -sb` (use `feat(core): …` if code changed).

---

## Task 14: The scan attaches updates and DLC to their game; revision groups

Spec D8, D15 (model), D5 (folder size without fixed folders).

**Files:**
- Modify: `crates/coverly-core/src/scan/mod.rs` (`scan_system`'s `filter_entry`: reuse the listing `is_console_folder` makes — pass it in — and `classify_listing`; skip `Mods/Cheats/Extras/Saves` of a `GameFolder`; files under `Updates/`/`DLC/` become `children` of the root base; orphans stay top-level with `role`; `folder_entry` sums the tree without fixed folders; `version_group`/`revision` from `parse_tags`), `crates/coverly-core/tests/scan_game_folders.rs` (create), `tests/scan_fixtures.rs` (snapshots: read any diff)

```rust
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChildEntry { pub path: PathBuf, pub role: ChildRole, pub size: u64, pub modified: SystemTime }
// RomEntry gains (all serde default, skipped when empty/None):
pub children: Vec<ChildEntry>,
pub role: Option<ChildRole>,
/// Same system + title_key with revision tags stripped (region kept) (D15).
pub version_group: Option<String>,
/// `(Rev 1)`, `(Rev A)`, `(v1.1)` as written.
pub revision: Option<String>,
```

No header is read for children in the scan. Fill, gamelists, audit and the library views keep using top-level entries only (assert counts in a test). Batocera `ps4/DLC/` (system-level fixed folder) is unchanged.

- [ ] **Step 1: Failing tests** (`scan_game_folders.rs`): `3DS/Pokemon Y (World)/` with base + `Updates/` + `DLC/` + `Mods/` + `Saves/` → one entry, two children, nothing from `Mods`/`Saves`; orphan folder (no base) → children top-level with `role`; `SNES/Hacks/Mods/x.sfc` scanned; Batocera `ps4/DLC/CUSA00001/` unchanged; library opened with the MinUI profile and with the option off → same result; PS3 JB folder game with `Updates/` inside → size excludes it; `Sonic (USA, Europe).md` and `(Rev 1)` → same `version_group`, `revision == Some("Rev 1")`; total game count excludes children.
- [ ] **Step 2: Run** `cargo test -p coverly-core --test scan_game_folders --test scan_fixtures --test gamelist --test audit_report` — expect failures.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** Step 2, G-RUST.
- [ ] **Step 5: Commit and push:** `git add -A crates && git commit -m "feat(core): attach updates and DLC to their game in the scan" && git push && git status -sb`

---

## Task 15: Duplicados keeps roles and revisions apart, with two removal guards

Spec D9, D11 (media id), D15 (guard), D22.

**Files:**
- Modify: `crates/coverly-core/src/dedupe/units.rs` (units from top-level entries and their `children`, `Unit.role: Option<ChildRole>`; fixed `Mods/…/Saves/` never units; folder units exclude fixed folders from files and fingerprint), `dedupe/keys.rs` (exact and format links require equal roles; a child without header kind gets `ExactKey::Title { kind }` from its role; the 360 `VersionKey` includes `media_id`; `FACTS_VERSION` 3 → 4), `dedupe/plan.rs` (has-children guard: a base whose game folder holds children or any fixed folder is never a removal; the group is reported with `DedupeNote::HasChildren` and no action), `dedupe/apply.rs` (never-all guard: refuse any group action that would remove every member), `dedupe/keeper.rs` (unchanged rule order; `version_order` over raw), `src/ipc/types.ts` + `contract.sample.json` (`has-children` note), `locales/{es,en}/ui.json` (`dedupe.note.hasChildren` — confirm the exact branch in `locales/es/ui.json`), `crates/coverly-core/tests/dedupe_game_folders.rs` (create)

- [ ] **Step 1: Failing tests.** Update never paired with its base even with equal bytes; two updates of one base → level 2 versions, higher kept; two 360 TUs with different media ids → not grouped; `(Rev 1)` vs untagged with default rules → keeper `(Rev 1)`; untagged with a save → kept (rule order respected); a crafted plan removing both → refused by apply; a base with `Updates/` in its folder chosen against → report only with the note; folder unit excludes `Updates/`/`DLC/`; files under `Extras/` are not units; `FACTS_VERSION == 4`.
- [ ] **Step 2: Run** `cargo test -p coverly-core --test dedupe_game_folders --test dedupe_keys --test dedupe_keeper --test dedupe_units --test dedupe_apply --test dedupe_plan` — expect failures.
- [ ] **Step 3: Implement.** Record `NEEDS-OWNER: Q13 — report only` in the commit body.
- [ ] **Step 4: Run** Step 2, G-RUST, G-WEB.
- [ ] **Step 5: Commit and push:** `git add -A crates src locales && git commit -m "feat(core): keep roles and revisions apart in duplicates" && git push && git status -sb`

---

## Task 16: CLI flags and text

Spec D23.

**Files:**
- Modify: `crates/coverly-cli/src/organize.rs` (`OrganizeArgs`: `--game-folders` / `--no-game-folders`, mutually exclusive, → `OrganizeOptions.game_folders`; version column for children in the plan table; texts already added in Task 1 now used), `crates/coverly-cli/src/main.rs` (help), `crates/coverly-cli/tests/*` (existing organize CLI test file)

- [ ] **Step 1: Failing tests.** `--game-folders` with `--layout anbernic-stock` nests a 3DS update; `--no-game-folders` with `--layout coverly-library` keeps it flat; both flags → clap error; the table shows `v1.1.0` (or raw) for the update row.
- [ ] **Step 2: Run** `cargo test -p coverly-cli` — expect failures.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** Step 2, G-RUST.
- [ ] **Step 5: Commit and push:** `git add -A crates && git commit -m "feat(cli): game folders flags and texts" && git push && git status -sb`

---

## Task 17: IPC carries the option and the game links to the app

**Files:**
- Modify: `src-tauri/src/state.rs` (`organize_plan` resolves `game_folders` from the run's layout via `Profile::uses_game_folders(&settings.organize)` unless the request overrides it), `src-tauri/src/ipc.rs` (request field `game_folders?: boolean`; settings patch accepts `organize.game_folders`), `src-tauri/tests/organize_state.rs`, `src-tauri/tests/contract.rs`, `src/ipc/types.ts` (`OrganizeSettings.game_folders: Record<string, boolean>`, `Profile.game_folders: boolean`), `src/ipc/backend.ts`, `src/ipc/tauriBackend.ts`, `src/ipc/contract.sample.json`, `src/ipc/contract.test.ts`

- [ ] **Step 1: Failing tests.** `organize_state.rs`: layout `coverly-library` → nested plan; same with `settings.organize.game_folders["coverly-library"] = false` → flat; layout `anbernic-stock` with override true → nested; a settings patch sets and clears the key. Contract round trip of the new fields.
- [ ] **Step 2: Run** `cargo test -p coverly-roms --test organize_state --test contract && pnpm vitest run src/ipc` — expect failures.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** Step 2, G-RUST, G-WEB.
- [ ] **Step 5: Commit and push:** `git add -A src-tauri src && git commit -m "feat(app): carry game links and the game folders option to the app" && git push && git status -sb`

---

## Task 18: Organizar folds children by link and previews the game folder; mock pile

Spec §9.1, §9.3, §10, P-5. REQUIRES frontend-design:frontend-design.

**Files:**
- Create: `src/components/organize/GameFolderPreview.tsx` (+ `GameFolderPreview.test.tsx`), `src/ipc/mock/gameFoldersFixture.ts`
- Modify: `src/lib/organize.ts` (+ test: `updatesUnderBases` folds by `game.of`, falls back to the name match when no item has `game`; `gameFolderPreview(items, base)`; `noteText` for `demo-of`, `base-missing`; `left` texts for `game-folder-content`, `emulator-cache`, `emulator-config`; `fixedFolderLabel(folder)`; rename text `organize.renamedForFolder`), `src/components/organize/SystemGroups.tsx`, `ItemName.tsx` (+ `SystemGroups.test.tsx`: version in mono with «versión {version}» accessible text, `la más nueva aquí` after it, target shortened to `…/Updates/<name>` with the full path in `title` and the accessible name), `src/ipc/mock/mockOrganize.ts` (`?mockPick=gamefolders`; respects the layout and the option, folds by `game.of` also when off), `src/ipc/mock/mockSettings.ts`/`fixture.ts` (built-in `coverly-library` «Coverly», `game_folders` map), mock tests, `locales/{es,en}/ui.json` (`gameFolder.*`, `organize.gameFolder.*`, `organize.version`, `organize.note.demoOf`, `organize.note.baseMissing`, `organize.renamedForFolder`, `organize.left.gameFolderContent`, `organize.newestHere`, `organize.licenceStays`, `organize.companion.licence`: spec §10 word for word)

Preview (spec §9.1): a disclosure button «Ver cómo queda la carpeta» / «Ocultar la carpeta» (`aria-expanded`, ≥ 24 × 24 px hit area, keeps focus on close) in the base row's detail area; content is a nested `ul` with `aria-label` «Carpeta de {title}», a single left spine `border-l` at the `border-width.spine` token in `--cr-border` on `--cr-surface`; fixed folders as «Actualizaciones (1)» in the UI face, the on-disk name once per preview in the accessible description («en disco: Updates/»); names and paths in mono with `overflow-wrap:anywhere`; at most 20 children per folder then «y {count} más»; built only when open; «ya existe» in `--cr-text-secondary` when `game.folder_exists`; open state in `sessionStorage` keyed by the base's source path, every access in `try/catch`, collapsed when storage throws; motion uses the existing collapsible (`--cr-duration-fast`, `--cr-ease-standard`), none under `prefers-reduced-motion`.

Mock pile `gamefolders` (synthetic names and sizes, spec §9.3): 3DS base + update + two DLC (one named by id), PS4 base + update + DLC, PS3 base PKG with `BLUS30001.rap` (licence companion), PS3 `(Demo)`, a game with 25 DLC, a Switch DataPatch, an orphan Wii U update (Review, `base-missing`), `Sonic the Hedgehog (USA, Europe)` and `(Rev 1)`, a pile game folder with `Updates/`, `DLC/`, `Mods/`, `Saves/` (two `game-folder-content` rows), `PS3UPDAT.PUP` (`console-system`), a `shadercache/` file (`emulator-cache`), `prod.keys` (key material).

- [ ] **Step 1: Failing tests** (Vitest): folding by `game.of` with two bases titled «Tetris»; legacy plan folds by name; preview shape, counts, the 20 cap, «ya existe»; list semantics and `aria-expanded`; storage throwing → collapsed, no crash; child row version text; mock `gamefolders` plan shape with layout `coverly-library` and with `anbernic-stock` (flat targets, still folded); i18n parity.
- [ ] **Step 2: Run** `pnpm vitest run src/lib/organize.test.ts src/components/organize src/ipc/mock src/i18n` — expect failures.
- [ ] **Step 3: Implement.** Screenshot critique (frontend-design) at 1440 and 390 px, light and dark, before committing; the default «file-manager tree with icons and pills» is rejected (spec §9.1).
- [ ] **Step 4: Run** Step 2, G-WEB.
- [ ] **Step 5: Commit and push:** `git add -A src locales && git commit -m "feat(ui): fold children by link and preview the game folder" && git push && git status -sb`

---

## Task 19: Ajustes → Biblioteca switch

Spec §9.1, §9.2, §10.

**Files:**
- Modify: `src/components/settings/LibrarySection.tsx` (one `SwitchSetting` right after «Perfil por defecto»: label `settings.library.gameFolders`, hint `settings.library.gameFoldersHint` with `{profile}` = the display name of `organize.default_layout`, `more` = `settings.library.gameFoldersDetails`; id `settingId('organize.game_folders')`; value `settings.organize.game_folders[default_layout] ?? profile.game_folders`; writing a value equal to the profile default removes the map entry), `src/components/settings/registry.ts` (`SETTINGS_INDEX` entry: section `library`, keywords «DLC actualizaciones updates carpeta»), `src/components/settings/Sections.test.tsx`, `registry.test.tsx` (must pass), `locales/{es,en}/ui.json` (`settings.library.gameFolders*`)

- [ ] **Step 1: Failing tests.** With default layout `coverly-library` the switch is on and the hint names «Coverly»; with `anbernic-stock` it is off and names «Anbernic stock»; toggling writes `organize.game_folders.anbernic-stock = true`; toggling back removes the key; the settings search finds it by «DLC»; `registry.test.tsx` green.
- [ ] **Step 2: Run** `pnpm vitest run src/components/settings` — expect failures.
- [ ] **Step 3: Implement.** Screenshot critique at 1440 and 390 px, light and dark.
- [ ] **Step 4: Run** Step 2, G-WEB.
- [ ] **Step 5: Commit and push:** `git add -A src locales && git commit -m "feat(ui): game folders switch in library settings" && git push && git status -sb`

---

## Task 20: Verification, real-data check, E2E, changelog and PR

**Files:** `CHANGELOG.md` (Unreleased), `docs/superpowers/specs/2026-10-26-game-folders.md` (status line only: «Implemented on `feat/game-folders`»); screenshots and the PR body under the scratchpad (not committed).

- [ ] **Step 0: Sync** with `origin/main`: `git fetch origin && git merge origin/main`; resolve conflicts by hand (locale files key by key; `src/components/*` by reading both sides; never touch `TODO.md`/`docs/TRACKER.md` content of this branch), run all gates, commit the merge, `git push`.
- [ ] **Step 1: Changelog.** Under `## [Unreleased]`: **Added** — one folder per game with `Updates/` and `DLC/` for modern consoles (the new «Coverly» library layout, and a switch for any layout), demos as their own games, versions and regions in names, detection of updates/DLC/demos/system titles on more consoles, retro revisions grouped as versions, `--game-folders`/`--no-game-folders`. **Changed** — licence files may move with their game by a same-volume rename only (never opened); `keys.txt`, `otp.bin`, `ticket.db`, `eeprom.bin` and `.edat` are key material; firmware packages, shader caches and emulator settings are shown as «not a game»; Duplicados never removes a game whose folder holds updates or DLC. English, plain words.
- [ ] **Step 2: Full gates** from a clean tree; keep the last line of each for the PR evidence:

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-game-folders && pnpm check-all && pnpm build && cargo fmt --all --check && cargo clippy --workspace --all-targets --all-features -- -D warnings && scripts/dev/clippy-windows.sh && cargo test --workspace --no-fail-fast
```

- [ ] **Step 3: Real-data check, read-only.**
  - `mkdir -p target/game-folders-sample && cp -RXp ~/Documents/ROMS/3DS ~/Documents/ROMS/PS3 target/game-folders-sample/` (only folders that exist; pick at most three modern-system folders by `ls ~/Documents/ROMS`; if a folder is larger than 20 GB, copy a subset of its files with `cp -Xp`), `find target/game-folders-sample -name '._*' | wc -l` prints `0`.
  - Plan only: `cargo run -q -p coverly-cli -- organize target/game-folders-sample --layout coverly-library --target target/game-folders-sample-out` (no `--apply`; check `--help` for the exact flags). Record per system: bases, updates, DLC, demos, orphans, left reasons; every unexpected row goes into the PR.
  - A second plan with `--no-game-folders` to show the flat layout is unchanged.
  - `rm -rf target/game-folders-sample target/game-folders-sample-out`. Never write to `~/Documents/ROMS`, `/Volumes/TinyExt/*` or `/Volumes/System`.
- [ ] **Step 4: Browser E2E** (spec §14.3). Start `pnpm dev --port 1470 --strictPort` in the background. Chrome DevTools MCP `new_page` `http://localhost:1470/?mockPick=gamefolders` with `isolatedContext: "game-folders"`; fall back to Playwright MCP, then `playwright-core` + Chrome for Testing in `/private/tmp/claude-501/-Users-miguelangelesparzacalero-Documents-GitHub/35b7c020-f2ae-412f-b757-99c24a9a498b/scratchpad/game-folders`.
  1. Layout «Coverly»: the 3DS base shows its update and DLC folded beneath with versions in mono; open «Ver cómo queda la carpeta»; screenshots at 1440 and 390 px, light and dark.
  2. Rows «La carpeta Mods de … se queda donde está por ahora» and the `Saves` one; `prod.keys` left; the PS3 PKG row says its licence goes with it; the 25-DLC game's preview shows 20 and «y 5 más»; the orphan Wii U update in review with «Su juego no está aquí ni en la biblioteca; revisa dónde va»; the demo row «Demo de Resistance (USA)».
  3. Ajustes → Organizar: «Perfil de estructura por defecto» = Coverly; Ajustes → Biblioteca: the switch is on and names Coverly; set Anbernic stock in Organizar; the switch is off and names Anbernic stock; turn it on, re-plan: the Anbernic plan nests; the settings search finds the switch by «DLC».
  4. `prefers-reduced-motion: reduce` emulated: the disclosure opens without transition; keyboard-only pass over the switch and the disclosure (`take_snapshot` shows `aria-expanded`); `list_console_messages`: no errors.
  - Screenshots to `/private/tmp/claude-501/-Users-miguelangelesparzacalero-Documents-GitHub/35b7c020-f2ae-412f-b757-99c24a9a498b/scratchpad/game-folders/e2e/`. Stop the server: `lsof -ti tcp:1470 -sTCP:LISTEN | xargs -r kill`.
- [ ] **Step 5: Commit and push:** `git add CHANGELOG.md docs/superpowers/specs/2026-10-26-game-folders.md && git commit -m "docs: changelog for game folders, part A" && git push && git status -sb`
- [ ] **Step 6: Open the PR** in English, no Claude/AI mention, no generated-by line:

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-game-folders && gh pr create --base main --head feat/game-folders \
  --title "feat: one folder per game with updates and DLC (game folders, part A)" \
  --body-file /private/tmp/claude-501/-Users-miguelangelesparzacalero-Documents-GitHub/35b7c020-f2ae-412f-b757-99c24a9a498b/scratchpad/game-folders/pr-body.md
```

  The body (scratchpad, not the repo) has: summary; the layout tree of spec §1; what each reader now reads in clear (kinds, versions, regions per system); the keys rule and the tests that guard it; installed-user notes (option off unchanged, `coverly-library` id and migration, downgrade); behaviour changes (licence rename, new key names, «not a game» rows, Duplicados guard); **Needs the owner** (spec §13 Q1–Q13 with the default taken, plan P-1–P-5, every `NEEDS-OWNER:` line: `git log origin/main..HEAD --format=%b | grep -E 'NEEDS-OWNER|BLOCKED'`); out of scope (parts B, C, D); gates evidence (Step 2); real-data counts (Step 3); E2E evidence with screenshots (uploaded as PR comment images, or described when upload is not possible). Post the local gate evidence as a PR comment too (GitHub Actions is blocked by billing; gates were run locally).
- [ ] **Step 7: End state:** `git status -sb` shows `## feat/game-folders...origin/feat/game-folders` with nothing ahead, and `gh pr view --json url,state` prints the PR.

---

## Spec coverage check

| Spec | Task |
|---|---|
| D1 option, override, respect always | 9 (option), 8/14/15 (respect regardless), 17 (IPC) |
| D2 `coverly-library`, migration guard | 9 |
| D3 fixed names, game folder definition | 2 |
| D4 only used folders | 10, 13 |
| D5 target paths, folder games, retro unchanged | 10 |
| D6 names, untrusted text, path length | 10 |
| D7 walk | 8 |
| D8 scan | 14 |
| D9 Duplicados roles and removal guard | 15 |
| D10 region | 3, 10 (name) |
| D11 versions (raw vs display, media id) | 5, 6, 15 |
| D12 demos | 5 (360 demo), 11 |
| D13 orphans | 11 |
| D14 detection (a)–(k) | 4, 5 |
| D15 revisions | 14 (model), 15 (guard) |
| D16 `GameLink` | 1, 10 |
| D17 licences | 7 (names), 12 (companions, apply) |
| D18 not a game | 1 (types), 7 |
| D19 no restructure | 10, 11 (flat base case), 20 (PR, Q1) |
| D20 existing names | 10 |
| D21 safety | 10, 12, 13 |
| D22 facts versions | 1 (organize 10), 15 (dedupe 4) |
| D23 CLI | 1 (texts), 16 |
| D24 newest here | 10 (core), 18 (UI) |
| D25 related variants out | 20 (PR) |
| §9 UX, §9.3 mock, §10 copy | 7, 15, 18, 19 |
| §12 edge cases | 8–15 |
| §13 owner questions | 9, 10, 12, 15, 20 |
| §14 tests | every task; 20 (E2E, real data) |
