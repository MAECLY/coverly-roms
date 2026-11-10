# Mods/ and Cheats/ Linked to Their Game, and Per-Emulator Translators (Part B) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking. The UI tasks (Tasks 22 and 23) also REQUIRE frontend-design:frontend-design: follow `docs/08-brand/brand.md` and spec §9.1, use the `src/components/ui` primitives and critique the changed screens with screenshots before committing.

**Goal:** `TODO.md` §2.1 part B. Translation patches, hacks, cheats, emulator patches, mods and texture packs are detected **without keys**, linked to exactly one game by the key their format carries (source CRC32, serial, game id, title id, build id, ROM name), and placed by Organizar with the `coverly-library` profile into the game's `Mods/` or `Cheats/` (created only when something goes in), with dry run, undo and every existing safety rule. Unlinked ones stay where they are and are reported; nothing is deleted. A pure `translate` module (master layout in → target plan out, one translator per emulator or frontend) is used by Organizar when an emulator is chosen for a system, and later by block 4 (sync). Also fixed here, test-first: `organize --apply` no longer pauses with `volume-gone` when the target folder does not exist yet; a region-less demo links to the base of the right region; a bare region word already in a name is not doubled; the CLI no longer lists a licence that moves with its game as «staying»; the review selects get an `id` and a `name`. Minimal UI: addons folded under their game, `Mods/`/`Cheats/` in the folder preview, a per-system «Emulador» select and a translated preview.

**Architecture:** Two new pure modules and thin glue. `crates/coverly-core/src/addons/` sniffs bytes, names, listings and capped text into `(AddonKind, AddonFormat, AddonKey)` and resolves keys against candidate games (`link::resolve`). `crates/coverly-core/src/translate/` maps a `MasterGame` to a `TranslatedGame` with no I/O. Organizar glue lives in a new `organize/addons.rs` (walk hook, link pass, master placement) and `organize/translate_plan.rs` (builds `MasterGame`s from the plan, calls `translate`, turns placements into targets), so `plan.rs` and `walk.rs` only gain calls, model types and a few lines. The journal gains two optional header fields (`created_roots`, `extra_roots`); no new op. The webview receives `GameLink.addon`, new notes/reasons and `OrganizePlan.translated`; it sends only `OrganizeRequest.emulators`.

**Tech Stack:** Rust 1.99 (workspace MSRV 1.93), `serde`, `serde_json`, `toml`, `crc32fast` (already a dependency of `identify/hash.rs`), `unicode-normalization`, `insta`, `filetime`, `tempfile`; React 19 + TypeScript + Tailwind 3.4 + zustand, Vitest + Testing Library. **No new crate and no new npm package.**

**Spec:** [`docs/superpowers/specs/2026-10-26-mods-cheats-translators.md`](../specs/2026-10-26-mods-cheats-translators.md). It is binding: decisions D1–D25 (§4), files (§5), formats (§6), examples (§7), behaviour (§8), UX (§9), copy (§10, word for word), edge cases (§12), owner questions (§13, implemented with the safe default), tests (§14). This plan turns it into tasks and records its deviations below. Part A ([`2026-10-26-game-folders.md`](2026-10-26-game-folders.md), PR #45) is the base: `FixedFolder`, `classify_listing`, `GameLink`/`GameRole`, licences by name, `coverly-library`, the `game_folders` option, `library_bases.rs`. Do not redo any of it.

## Global Constraints

- **Worktree and branch.** Work only in `/Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-mods`, branch `feat/mods-cheats-translators`, which tracks `origin/feat/mods-cheats-translators`. Agent shells reset their cwd, so every shell command starts with `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-mods &&`. Never switch branches and never touch `main`.
- **Git rules (mandatory).** An earlier run lost a whole branch because it was never pushed.
  - Conventional, granular commits in English (`fix(core): …`, `feat(core): …`, `test(core): …`, `feat(cli): …`, `feat(ipc): …`, `feat(ui): …`, `fix(ui): …`, `docs: …`).
  - **No** `Co-Authored-By` line, no "Generated with" line, no mention of Claude or AI anywhere (commits, PR, code comments). This owner rule overrides any harness attribution.
  - Run `git push` **immediately after every commit**. Every task ends with `git commit … && git push && git status -sb`; the first line must read `## feat/mods-cheats-translators...origin/feat/mods-cheats-translators` with no `[ahead N]`.
  - Never force-push. To update with `main`: `git fetch origin && git merge origin/main`, resolve conflicts by hand (other workflows change `locales/*.json`, `src/components/*` and `TODO.md` in parallel; resolve locale files **key by key**, never by taking one side), run the gates, commit the merge, push.
- **Do not edit** `docs/TRACKER.md` or `TODO.md` on this branch (they are updated on `main` after merge).
- **Scope: part B only.** Do **not** build the six-views redesign or addon rows in the library views (part C), the Extras CRUD, official extras, `extras.json` or a media viewer (part D), sync (block 4), a settings UI for emulator data folders, applying patches to make patched ROMs, installing into emulators, or merging into an emulator's config files. Organizar never creates `Extras/` or `Saves/`.
- **Keys rule, no exceptions.** Never read, include, download or decrypt console keys or licences; never link or recommend emulators, ROM or key sites (emulator names are product names that say where a file goes, nothing more). `consoles::is_key_material(name)` is checked **before** any addon sniff opens a file; a mod folder holding key material by name is not moved (`LeftReason::KeyMaterial`) and the key file is never opened (D18). Every test with a `.tik`, `.rap`, `prod.keys` or `title.keys` in a fixture runs under `consoles::start_open_log()` and asserts the key path is never in `take_open_log()`. Translators never extract, decrypt or install: one that would need it says so (`ExtractFirst`, `InstallInEmulator`, `NotSupported`).
- **Bytes read are bounded (D2–D4).** Patches: the first 64 bytes, the last 12 (BPS/UPS) and at most 4 KiB of an xdelta header; never the whole patch. Text (cheats, `rules.txt`, `hires.txt`, readmes): at most 1 MiB (readmes 64 KiB), UTF-8 lossy, never executed. Source-CRC hashing only for same-system candidates whose size equals the declared source size (or +16 iNES, +512 copier), ≤ 512 MiB each, under the plan's existing hash budget.
- **Installed users first (D12, D16, Q11).** With no `emulators` in the request, every built-in profile's existing snapshot stays byte-identical except for **new** addon rows and notes; a profile default emulator runs `TranslateScope::Conservative`, which never changes where a base, update or DLC goes and never renames a patch for soft-patching. Same-stem `.cht`/`.ips` keep travelling as companions unless game folders are on (Q13). New serde fields are `#[serde(default)]` and skipped when empty, so a paused job saved by an older build still resumes. No `SETTINGS_VERSION` bump.
- **Safety rules unchanged.** Dry-run plan first; apply re-validates fingerprints; exclusive renames, no overwrite; same-volume moves, otherwise copy + verify + remove; volume-gone pause/resume; undo removes only empty (or litter-only) folders it created. An addon whose name is taken in its fixed folder is `AddonNameTaken` (Review), never ` (2)` (D11).
- **Untrusted text never becomes a path unchecked (D14).** Every key that becomes a path component (title id, serial, game id, build id, folder or file name from a listing or manifest) is validated against its format (`[0-9A-F]{16}`, serial/game id patterns) or `safe_file_name`, must be exactly one `Component::Normal`, never `..`, absolute or carrying a separator; a key that fails is `AddonKey::None` (`UnlinkedWhy::NoKey`), never sanitised into a path.
- **TDD.** Write the failing test, run it and watch it fail for the expected reason, implement, watch it pass. Each "Run" step names the exact command.
- **Gates (same as CI).** Before every commit that touches code or tests:
  - **G-RUST:** `cargo fmt --all --check && cargo clippy --workspace --all-targets --all-features -- -D warnings && scripts/dev/clippy-windows.sh && cargo test --workspace --no-fail-fast`
  - **G-WEB** (any task touching `src/`, `locales/`, `design/` or `src-tauri/`): `pnpm check-all && pnpm build`
  - Docs-only commits run `pnpm check-all`.
- **Windows CI leg.** Fixture mtimes pinned (`coverly_core::dedupe::testing::pin_mtime` or `filetime::set_file_mtime` to a fixed time, never "now"). Paths compared by component (`Path::components`, `file_name`, `Path::join`), never by slash text; snapshot paths go through the existing `/` redaction (`parts(..).join("/")`). Unix-only checks (`st_dev`) go in `#[cfg(unix)]` tests; the Windows drive and UNC rules of D19 are recognised **by text** so their pure tests run on every OS. No fixture folder name is reserved (`CON`, `AUX`…) or ends in a dot or space, except in the safe-name tests that assert the rewrite.
- **Real data is read-only.** Never write to `~/Documents/ROMS`, `/Volumes/TinyExt/*` or `/Volumes/System`; only `ls`, `stat` and Coverly **plan-only** runs may touch them. Write tests only on copies made with `cp -RXp` under `/Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-mods/target/mods-sample/`; `find target/mods-sample -name '._*' | wc -l` must print `0`; delete the folder afterwards.
- **Fixtures are synthetic** (builders in `consoles/testing.rs` and new `addons/testing.rs`, no copyrighted data, no keys; real game names only as file names over synthetic bytes).
- **No UI strings in the core.** Reasons, notes, kinds and translate notes are enums plus data; UI and CLI render them. Every UI string lives in `locales/en/ui.json` **and** `locales/es/ui.json` with the same keys and placeholders (parity test). Copy is spec §10 word for word (Spanish: sentence case, no exclamation marks, exact counts; numbers, keys, versions and paths in mono by the renderer).
- **Brand.** Only existing tokens from `design/tokens.json` (`pnpm tokens:check` green, no new token), `src/components/ui` primitives, `SystemIcon` for system marks. No warning colour for addon or translate notes (§9.1).
- **Secrets are never read or printed:** the minisign key, `~/.coverly-release/`, any `.env*`, `.vercel/`.
- **Servers and E2E.** The web UI runs on port **1471** only: `pnpm dev --port 1471 --strictPort` (mock backend). Stop it by port only: `lsof -ti tcp:1471 -sTCP:LISTEN | xargs -r kill`. E2E uses Chrome DevTools MCP `new_page` with `isolatedContext: "mods-cheats-translators"`; fall back to Playwright MCP; if both profiles are locked, install `playwright-core` in `/private/tmp/claude-501/-Users-miguelangelesparzacalero-Documents-GitHub/35b7c020-f2ae-412f-b757-99c24a9a498b/scratchpad/mods-cheats-translators` and drive Chrome for Testing from there.
- **Owner decisions never block.** Every §13 question (Q1–Q14) is implemented with its safe default. Anything new that needs Miguel is implemented around, recorded as a `NEEDS-OWNER: <topic> — <default taken>` line in the task's commit body and collected into the PR (Task 24).

## Deviations from the spec (decided in this plan)

- **P-1: organize glue in two new files.** `plan.rs` is ~5 000 lines and `walk.rs` ~1 000. Addon work in Organizar goes into `organize/addons.rs` (walk hook `addon_candidate`, `link_addons` pass, `addon_targets` placement) and translator work into `organize/translate_plan.rs`. `walk.rs`, `plan.rs` and `target.rs` only gain the model types the spec lists, the calls and the `-mods` overlay change. Behaviour is the spec's.
- **P-2: xdelta3 app header field order.** xdelta3 writes its app header as `<target name>/<target compression>/<source name>/<source compression>` (`main_set_appheader`), so the **source** ROM name is the third `/`-separated field; spec D2 phrases it «`source//target/` form». `sniff_patch` takes field 3 (index 2) when four fields are present, else no key; only the final path component of that field is kept (§12 «xdelta app header with a path»).
- **P-3: `roots_state` in its own module.** `organize/roots.rs` holds `TargetAnchor`, `RootsState`, the `RootProbe` trait (`OsRootProbe` for real runs, a fake in tests), `anchor_of` and `roots_state`. Windows drive (`E:\`, `E:`) and UNC share roots (`\\server\share`) are recognised **by text** (`is_volume_root`) so the pure test runs on every OS (spec §14.1 asks for that).
- **P-4: resume never creates a target root.** A resumed job's writer already began, so its target existed; if it is missing at resume it vanished: `CreateTarget` at resume is treated as `VolumeGone` (spec D19 «resume re-runs the same check»; this is the safe reading — the header cannot be rewritten to add new `created_roots`).
- **P-5: `JobMeta` carries the new header fields.** `JobMeta { kind, source_root, created_roots: Vec<PathBuf>, extra_roots: Vec<PathBuf> }`; every existing `JobMeta { .. }` literal gains `..JobMeta::default()`. `Writer` keeps `extra_roots` (canonicalised at begin) for `ensure_within_any`.
- **P-6: `LibraryBase.region`.** `library_bases.rs` stores the name-tag region on each `LibraryBase` and `keys: HashMap<String, Vec<LibraryBase>>`; `by_title_key` keeps its signature (first by the D20 order) for existing callers and `by_title_key_all` returns every candidate. One shared `pick_by_region(candidates, want)` serves plan and library demos (D20).
- **P-7: CLI staying list as a function.** `render_table`'s `left` list is extracted into `fn staying(plan) -> Vec<(&OrganizeItem, &LeftReason)>` so D22 is unit-tested without rendering the whole table.

## Review Focus

1. **Keys rule.** No addon sniff opens key material; a mod folder with a `.tik` is left as `KeyMaterial` and the `.tik` never opened (Task 9 `a_mod_folder_with_a_key_is_left_and_the_key_never_opened`, Task 13 open-log assertions on apply and undo). Translators never extract/decrypt/install (Task 15 `translate_never_touches_the_filesystem`).
2. **No wrong links.** Exactly one game or none (D6): `Several`, `Many`, `NoKey`, `Budget`, `Partial` all stay put. `RomName` needs the same region set; a 3-character Dolphin key with two regions present is `Several`. Tests in Task 10.
3. **Installed users.** Every built-in profile with no `emulators` plans the batch 2/3 and part A snapshot piles to the same targets (Task 18 `profile_defaults_keep_every_existing_target`). Same-stem companions unchanged with game folders off.
4. **D19 safety.** A missing or unmounted volume is never «created» on the system disk: mount parent, filesystem/drive/UNC root, anchor path mismatch, `st_dev` mismatch, `target_anchor: None` → `VolumeGone`. Creation is one `create_dir` per level, journaled in the header, undone innermost first only when empty (Tasks 1, 2).
5. **Path safety.** Crafted ids (`..`, `a/b`, `C:`, NUL, 300 chars) never become components (Task 15 `crafted_keys_never_become_paths`).
6. **Undo.** Addons back first, then children, then bases, then created `Mods/`/`Cheats/`/`-UPDATE`/`addcont`/target roots when empty; emulator-data writes inside `extra_roots` undo; an older journal without the new fields still undoes (Tasks 2, 13, 17).
7. **Windows.** Pinned mtimes, component comparisons, deeper paths counted for `PathTooLong`, `scripts/dev/clippy-windows.sh` on every task.

---

## File structure

```
crates/coverly-core/
├─ src/addons/ (new)
│  ├─ mod.rs        AddonKind, AddonFormat, AddonKey, AddonLink, LinkVia, HeaderFit, UnlinkedWhy; fixed_folder(); key validators
│  ├─ patch.rs      sniff_patch(head, tail12, xdelta_app) (D2), varint, IPS chain names
│  ├─ cheat.rs      sniff_cheat(name, parent_names, text) (D3), global databases
│  ├─ mods.rs       sniff_mod_dir(name, listing, small_files) (D4), rules.txt / hires.txt readers, MSU-1 sets
│  ├─ link.rs       Candidates, GameRef, resolve() (D5, D6, D8), readme search
│  └─ testing.rs    bps()/ups()/ips()/xdelta()/ppf()/aps_n64() byte builders (cfg test-util)
├─ src/translate/ (new, pure: no std::fs, no std::time)
│  ├─ mod.rs        MasterGame, MasterEntry, MasterAddon, EntryForm, GameIds, TranslateTarget, FrontendMode, TranslateScope,
│  │                  TranslatedGame, Placement, Destination, TranslateNote, NotSupportedWhy, EmulatorId (+ systems(), display_name()),
│  │                  translate(), path-safe join helpers
│  ├─ shadps4.rs rpcs3.rs azahar.rs cemu.rs vita3k.rs switch.rs retroarch.rs duckstation.rs pcsx2.rs dolphin.rs ppsspp.rs
│  │  xenia.rs flycast.rs mesen.rs frontend.rs
├─ src/lib.rs                    pub mod addons; pub mod translate
├─ src/identify/names.rs         bare_region (D21)
├─ src/organize/
│  ├─ roots.rs (new)             TargetAnchor, RootsState, RootProbe, OsRootProbe, anchor_of, roots_state, is_volume_root (D19, P-3)
│  ├─ addons.rs (new)            addon_candidate (walk hook), link_addons (plan pass), addon_targets (D11–D13) (P-1)
│  ├─ translate_plan.rs (new)    master_games(plan) → translate → targets; TranslatedGameView (D16)
│  ├─ apply.rs                   prepare_roots before check_space/Writer::begin_with; created_roots; extra_roots (D17, D19)
│  ├─ plan.rs                    GameRole::{Mod,Cheat}; GameLink.addon; ItemNote::{ModOf,PatchOf,CheatOf,HeaderMismatch,PointEmulatorHere};
│  │                               LeftReason::{AddonUnlinked,AddonNameTaken,AddonNoPlace,AddonForEmulator,NotSupported,PatchNotApplied};
│  │                               OrganizeOptions.{emulators,emulator_data}; OrganizePlan.{translated,target_anchor}; demo region (D20);
│  │                               FACTS_VERSION 11
│  ├─ library_bases.rs           LibraryBase.region; by_title_key_all; pick_by_region; serial / game id / stem maps (D9, D20, P-6)
│  ├─ target.rs                  bare_region in game_folder_name (D21); fixed-folder targets for addons and same-stem companions
│  └─ walk.rs                    addon candidates; `-mods` overlay → addon when its base is planned (D4); pile Mods/Cheats entries (D13)
├─ src/write/{mod.rs,undo_organize.rs}  header created_roots / extra_roots; ensure_within_any; undo of both (D17, D19)
├─ src/profiles/{mod.rs,builtin.rs,custom.rs}  Profile.emulators + defaults + TOML validation (D16)
└─ tests/
   ├─ organize_missing_target.rs (new)  D19 end to end
   ├─ addons_detect.rs (new)            patch / cheat / mod tables on synthetic files
   ├─ organize_addons.rs (new)          link, place, apply + undo, keys, installed users
   ├─ translate.rs (new)                one table per translator, path safety, no-fs grep
   └─ organize_game_folders.rs, custom_profiles.rs  updated (D20, D16)
crates/coverly-cli/src/{organize.rs,main.rs}     D22 staying list; --emulator, --emulator-data; addon and translate texts (D24)
src-tauri/src/organize/{view.rs,mod.rs}, src-tauri/tests/contract.rs   OrganizeRequest.emulators; survey_key ignores it
src/ipc/{types.ts,organizeWire.ts,contract.sample.json,contract.requests.json,contract.test.ts}
src/ipc/mock/{modsFixture.ts (new),mockOrganize.ts,mockOrganize.test.ts}   ?mockPick=mods
src/lib/organize.ts (+ test)                      fold mod/cheat; addonText; preview Mods/Cheats; translatedPreview; note/left keys
src/components/organize/{ReviewBuckets.tsx,SystemGroups.tsx,ItemName.tsx,GameFolderPreview.tsx} (+ tests)
src/stores/organize*.ts                           per-session emulator choice
locales/{es,en}/ui.json, CHANGELOG.md
```

---

## Task 1: `roots_state` and the plan-time target anchor (D19, pure, test-first)

**Files:**
- Create: `crates/coverly-core/src/organize/roots.rs`
- Modify: `crates/coverly-core/src/organize/mod.rs` (`pub mod roots;`), `crates/coverly-core/src/organize/plan.rs` (`OrganizePlan.target_anchor`, set in `plan_from_survey`)
- Test: unit tests in `roots.rs`

**Interfaces (produced):**

```rust
// organize/roots.rs
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetAnchor {
    /// Nearest existing ancestor of a target that did not exist at plan time (absolute, as given).
    pub path: PathBuf,
    /// Unix `st_dev` of `path`; `None` on Windows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dev: Option<u64>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RootsState { Ready, CreateTarget { missing: Vec<PathBuf> /* outermost first */ }, VolumeGone }
pub trait RootProbe { fn is_dir(&self, p: &Path) -> bool; fn exists(&self, p: &Path) -> bool; fn dev(&self, p: &Path) -> Option<u64>; }
pub struct OsRootProbe;
pub fn is_volume_root(path: &Path) -> bool;
pub fn anchor_of(target: &Path, probe: &dyn RootProbe) -> Option<TargetAnchor>;
pub fn roots_state(source: &Path, target: &Path, anchor: Option<&TargetAnchor>,
                   mount_parents: &[PathBuf], probe: &dyn RootProbe) -> RootsState;
// plan.rs, OrganizePlan
#[serde(default, skip_serializing_if = "Option::is_none")]
pub target_anchor: Option<crate::organize::roots::TargetAnchor>,
```

- [ ] **Step 1: Write the failing tests** at the bottom of `roots.rs` (create the file with only the type stubs and `todo!()` bodies first so it compiles):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// Directories that exist, with their device number.
    struct Fake(Vec<(PathBuf, u64)>);
    impl RootProbe for Fake {
        fn is_dir(&self, p: &Path) -> bool { self.0.iter().any(|(d, _)| d.components().eq(p.components())) }
        fn exists(&self, p: &Path) -> bool { self.is_dir(p) }
        fn dev(&self, p: &Path) -> Option<u64> {
            self.0.iter().find(|(d, _)| d.components().eq(p.components())).map(|(_, v)| *v)
        }
    }
    fn pb(parts: &[&str]) -> PathBuf {
        let mut p = PathBuf::from(std::path::MAIN_SEPARATOR_STR);
        for c in parts { p.push(c); }
        p
    }
    fn world() -> Fake {
        Fake(vec![(pb(&[]), 1), (pb(&["home"]), 1), (pb(&["home", "m"]), 1), (pb(&["pile"]), 1),
                  (pb(&["Volumes"]), 1), (pb(&["mnt"]), 1), (pb(&["mnt", "usb"]), 1)])
    }
    fn anchor(parts: &[&str], dev: u64) -> TargetAnchor { TargetAnchor { path: pb(parts), dev: Some(dev) } }

    #[test]
    fn an_existing_target_is_ready() {
        let f = world();
        assert_eq!(roots_state(&pb(&["pile"]), &pb(&["home", "m"]), None, &[], &f), RootsState::Ready);
    }

    #[test]
    fn a_missing_source_is_a_vanished_volume() {
        let f = world();
        assert_eq!(roots_state(&pb(&["gone"]), &pb(&["home", "m"]), None, &[], &f), RootsState::VolumeGone);
    }

    #[test]
    fn missing_levels_under_the_recorded_anchor_are_created_outermost_first() {
        let f = world();
        let t = pb(&["home", "m", "new", "sub"]);
        assert_eq!(anchor_of(&t, &f), Some(anchor(&["home", "m"], 1)));
        assert_eq!(
            roots_state(&pb(&["pile"]), &t, Some(&anchor(&["home", "m"], 1)), &[], &f),
            RootsState::CreateTarget { missing: vec![pb(&["home", "m", "new"]), t.clone()] }
        );
    }

    #[test]
    fn a_target_that_existed_when_planned_is_never_recreated() {
        let f = world();
        let t = pb(&["home", "m", "new"]);
        assert_eq!(roots_state(&pb(&["pile"]), &t, None, &[], &f), RootsState::VolumeGone);
    }

    #[test]
    fn an_unplugged_drive_under_a_mount_parent_is_gone() {
        let f = world();
        let t = pb(&["Volumes", "USB", "ROMS"]);
        let a = anchor_of(&t, &f).unwrap();
        assert_eq!(roots_state(&pb(&["pile"]), &t, Some(&a), &[pb(&["Volumes"])], &f), RootsState::VolumeGone);
    }

    #[test]
    fn a_different_nearest_ancestor_or_device_is_gone() {
        let f = world();
        let t = pb(&["mnt", "usb", "ROMS"]);
        // Planned while mounted: anchor /mnt/usb on device 7; now /mnt/usb is an empty folder on device 1.
        assert_eq!(roots_state(&pb(&["pile"]), &t, Some(&anchor(&["mnt", "usb"], 7)), &[], &f), RootsState::VolumeGone);
        // Planned with /mnt/usb/ROMS/x missing below an anchor that is not the one found now.
        assert_eq!(roots_state(&pb(&["pile"]), &t, Some(&anchor(&["mnt"], 1)), &[], &f), RootsState::VolumeGone);
    }

    #[test]
    fn a_filesystem_root_as_nearest_ancestor_is_gone() {
        let f = world();
        let t = pb(&["nothing", "here"]);
        assert_eq!(roots_state(&pb(&["pile"]), &t, Some(&anchor(&[], 1)), &[], &f), RootsState::VolumeGone);
    }

    #[test]
    fn windows_drive_and_unc_roots_are_volume_roots_on_every_os() {
        for root in [r"E:\", "E:", "e:/", r"\\server\share", r"\\server\share\", "//server/share"] {
            assert!(is_volume_root(Path::new(root)), "{root}");
        }
        for not in [r"E:\ROMS", r"\\server\share\ROMS"] {
            assert!(!is_volume_root(Path::new(not)), "{not}");
        }
        let f = world();
        // A missing drive: nothing above `E:\ROMS` exists in the fake → gone.
        let t = PathBuf::from(r"E:\ROMS");
        assert_eq!(roots_state(&pb(&["pile"]), &t, Some(&TargetAnchor { path: PathBuf::from(r"E:\"), dev: None }), &[], &f),
                   RootsState::VolumeGone);
    }
}
```

- [ ] **Step 2: Run** `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-mods && cargo test -p coverly-core --lib organize::roots` — expect panics at `todo!()`.

- [ ] **Step 3: Implement** `roots.rs`:

```rust
//! Whether an organise job's roots are there, and which missing target
//! folders apply may create (mods/cheats/translators spec D19).

use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

pub struct OsRootProbe;

impl RootProbe for OsRootProbe {
    fn is_dir(&self, p: &Path) -> bool { p.is_dir() }
    fn exists(&self, p: &Path) -> bool { p.symlink_metadata().is_ok() }
    #[cfg(unix)]
    fn dev(&self, p: &Path) -> Option<u64> {
        use std::os::unix::fs::MetadataExt;
        std::fs::metadata(p).ok().map(|m| m.dev())
    }
    #[cfg(not(unix))]
    fn dev(&self, _: &Path) -> Option<u64> { None }
}

fn same(a: &Path, b: &Path) -> bool { a.components().eq(b.components()) }

/// A filesystem root, a Windows drive root (`E:\`, `E:`) or a UNC share
/// root (`\\server\share`); the Windows forms by text so the rule is the
/// same (and tested) on every OS.
pub fn is_volume_root(path: &Path) -> bool {
    if path.parent().is_none() { return true; }
    let text = path.to_string_lossy();
    let t = text.trim_end_matches(['\\', '/']);
    let b = t.as_bytes();
    if b.len() == 2 && b[0].is_ascii_alphabetic() && b[1] == b':' { return true; }
    if let Some(rest) = t.strip_prefix(r"\\").or_else(|| t.strip_prefix("//")) {
        return rest.split(['\\', '/']).filter(|s| !s.is_empty()).count() <= 2;
    }
    false
}

fn parent_of(p: &Path) -> Option<&Path> { p.parent().filter(|q| !q.as_os_str().is_empty()) }

pub fn anchor_of(target: &Path, probe: &dyn RootProbe) -> Option<TargetAnchor> {
    if probe.is_dir(target) { return None; }
    let mut cur = parent_of(target);
    while let Some(p) = cur {
        if probe.exists(p) {
            return probe.is_dir(p).then(|| TargetAnchor { path: p.to_path_buf(), dev: probe.dev(p) });
        }
        cur = parent_of(p);
    }
    None
}

pub fn roots_state(source: &Path, target: &Path, anchor: Option<&TargetAnchor>,
                   mount_parents: &[PathBuf], probe: &dyn RootProbe) -> RootsState {
    if !probe.is_dir(source) { return RootsState::VolumeGone; }
    if probe.is_dir(target) { return RootsState::Ready; }
    let Some(anchor) = anchor else { return RootsState::VolumeGone };
    let mut missing = Vec::new();
    let mut cur = Some(target);
    let found = loop {
        let Some(p) = cur else { break None };
        if probe.exists(p) { break Some(p); }
        if is_volume_root(p) { break None; }
        missing.push(p.to_path_buf());
        cur = parent_of(p);
    };
    let Some(found) = found else { return RootsState::VolumeGone };
    if !probe.is_dir(found) || !same(found, &anchor.path) || probe.dev(found) != anchor.dev
        || is_volume_root(found) || mount_parents.iter().any(|m| same(m, found)) {
        return RootsState::VolumeGone;
    }
    missing.reverse();
    RootsState::CreateTarget { missing }
}
```

  In `plan.rs`, add the field to `OrganizePlan` (doc: «Set when the target did not exist at plan time (D19)») and in `plan_from_survey` set `target_anchor: roots::anchor_of(&layout.target_root, &roots::OsRootProbe)`. Every `OrganizePlan { .. }` literal in the workspace (grep `OrganizePlan {` in `crates`, `src-tauri`) gains `target_anchor: None`.

- [ ] **Step 4: Run** `cargo test -p coverly-core --lib organize::roots` (green), then G-RUST.
- [ ] **Step 5: Commit + push.**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-mods && git add crates/coverly-core/src/organize src-tauri crates/coverly-cli && git commit -m "fix(core): tell a missing target folder from a vanished volume" -m "Pure roots_state with a plan-time anchor (nearest existing ancestor and its device); a mount parent, a filesystem, drive or UNC root, another anchor or another device is still volume-gone." && git push && git status -sb
```

---

## Task 2: Apply creates the missing target folder, journaled and undone (D19, test-first)

**Files:**
- Create: `crates/coverly-core/tests/organize_missing_target.rs`
- Modify: `crates/coverly-core/src/organize/apply.rs` (`prepare_roots`), `crates/coverly-core/src/write/mod.rs` (`JobMeta.created_roots`, `Line::Header.created_roots`, `JobInfo`/journal reader), `crates/coverly-core/src/write/undo_organize.rs` (remove created roots last), every `JobMeta { .. }` literal (`..JobMeta::default()`)

**Interfaces:** `JobMeta { kind, source_root, created_roots: Vec<PathBuf>, extra_roots: Vec<PathBuf> }` (P-5; `extra_roots` is used in Task 17, add it now as an empty field). Header:

```rust
Header {
    job: String, root: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")] source_root: Option<PathBuf>,
    #[serde(default)] kind: JobKind,
    created_at: String,
    /// Target folders apply created before the job began (D19), outermost first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")] created_roots: Vec<PathBuf>,
    /// Emulator data folders this job may write into (D17).
    #[serde(default, skip_serializing_if = "Vec::is_empty")] extra_roots: Vec<PathBuf>,
},
```

- [ ] **Step 1: Write the failing test** `tests/organize_missing_target.rs` (copy `NoCloud`, `put`, `plan_of`-style env and `apply_in` from `tests/organize_game_folders.rs` lines 19–33, 269–284 and 1221–1234):

```rust
//! `organize --apply` into a folder that does not exist yet (mods/cheats/
//! translators spec D19, `TODO.md` «Fallos conocidos»).

#[test]
fn apply_creates_a_missing_target_and_undo_removes_it() {
    let tmp = tempfile::tempdir().unwrap();
    let pile = tmp.path().join("pile");
    put(&pile, "Tetris (World).gb", &gb_rom(), 1); // synthetic GB header from `consoles::testing` / tests/common/roms.rs
    let target = tmp.path().join("new").join("sub");
    assert!(!target.exists());
    let plan = plan_of(&pile, &options("anbernic-stock", &target));
    assert_eq!(plan.target_anchor.as_ref().map(|a| a.path.clone()), Some(tmp.path().to_path_buf()));

    let data = tmp.path().join("data");
    let report = apply_in(&plan, &data);
    assert_eq!(report.paused, None, "{report:?}");
    assert_eq!(report.moved, 1);
    assert!(target.is_dir());

    let job = report.job.clone().unwrap();
    let header: serde_json::Value = serde_json::from_str(
        fs::read_to_string(data.join("jobs").join(format!("{job}.jsonl"))).unwrap().lines().next().unwrap(),
    ).unwrap();
    let created: Vec<Vec<String>> = header["created_roots"].as_array().unwrap().iter()
        .map(|v| parts(Path::new(v.as_str().unwrap()))).collect();
    assert_eq!(created, vec![parts(&tmp.path().join("new")), parts(&target)]);

    undo_job(&data, &job).unwrap();
    assert!(pile.join("Tetris (World).gb").is_file());
    assert!(!tmp.path().join("new").exists(), "created folders removed innermost first");
}

#[test]
fn undo_keeps_a_created_target_the_user_added_to() {
    // same setup; after apply, put(&target, "mine.txt", b"x", 9); undo → target and new/ kept, mine.txt untouched.
}

#[test]
fn a_target_gone_since_planning_still_pauses() {
    // target exists at plan time (fs::create_dir_all), removed before apply → report.paused == Some(PauseKind::VolumeGone),
    // nothing created, no job.
}

#[test]
fn an_old_header_without_created_roots_still_undoes() {
    // Hand-written journal header with no created_roots / extra_roots (shape of tests/organize_undo.rs `hand_journal`) → undo works.
}
```

  Write the three bodies in full in the same style (no placeholders in the committed test).

- [ ] **Step 2: Run** `cargo test -p coverly-core --test organize_missing_target` — the first test fails with `paused: Some(VolumeGone)`: the known bug, reproduced.

- [ ] **Step 3: Implement.**
  - `write/mod.rs`: add the two fields to `JobMeta` and `Line::Header`; `begin_with` writes them; canonicalise each `extra_roots` entry (`fs::canonicalize`, error → `WriteError::OutsideRoot`) — used in Task 17. Expose `pub fn job_header_roots(data_dir, job) -> Result<(Vec<PathBuf>, Vec<PathBuf>), WriteError>` for undo. Fix every `JobMeta { … }` literal with `..JobMeta::default()` (grep in `crates/`, `src-tauri/`).
  - `apply.rs`:

```rust
/// Makes the target root exist when D19 allows it: the folders created,
/// outermost first, for the journal header; `Err` pauses as volume gone.
fn prepare_roots(plan: &OrganizePlan, resumed: bool) -> Result<Vec<PathBuf>, ItemErrorKind> {
    use super::roots::{roots_state, OsRootProbe, RootsState};
    match roots_state(&plan.source_root, &plan.target_root, plan.target_anchor.as_ref(),
                      &crate::volumes::mount_parents(), &OsRootProbe) {
        RootsState::Ready => Ok(Vec::new()),
        RootsState::VolumeGone => Err(ItemErrorKind::VolumeGone),
        RootsState::CreateTarget { .. } if resumed => Err(ItemErrorKind::VolumeGone), // P-4
        RootsState::CreateTarget { missing } => {
            let mut made = Vec::new();
            for dir in missing {
                match fs::create_dir(&dir) { // one level at a time: a race is seen
                    Ok(()) => made.push(dir),
                    Err(e) if e.kind() == io::ErrorKind::AlreadyExists && dir.is_dir() => {}
                    Err(_) => { for d in made.iter().rev() { let _ = fs::remove_dir(d); } return Err(ItemErrorKind::VolumeGone); }
                }
            }
            Ok(made)
        }
    }
}
```

    In `apply_organize` replace `if let Some(reason) = roots_gone(plan)` with `let created = match prepare_roots(plan, false) { Ok(c) => c, Err(reason) => { pause(&mut report, reason, &pending); return Ok(report); } };` before `check_space`, and pass `created_roots: created.clone()` in `JobMeta`; if `check_space` or `Writer::begin_with` fails, remove `created` innermost first (only empty) before returning the error. In `resume_organize` use `prepare_roots(&plan, true)`. The mid-run `roots_gone` checks stay as they are.
  - `undo_organize.rs`: after every entry and every `DirCreated` removal, read the header's `created_roots`, keep only paths equal to the header `root` or one of its ancestors (`root.starts_with(p)` by components), and for each innermost first call `remove_created_root(dir, report)` — same body as `remove_created_dir` (~line 701) but without `within_either` (the path was validated against the header root above).
- [ ] **Step 4: Run** `cargo test -p coverly-core --test organize_missing_target --test organize_undo --test organize_apply --test journal_resume` (green), then G-RUST.
- [ ] **Step 5: Commit + push.**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-mods && git add crates src-tauri && git commit -m "fix(core): create a missing target folder instead of pausing as volume gone" -m "Apply creates the missing levels one at a time when the plan-time anchor and device still hold, records them as created_roots in the journal header and undo removes them innermost first when empty. Resume never creates a root." && git push && git status -sb
```

---

## Task 3: A region-less demo links to the base of its region (D20, test-first)

**Files:**
- Modify: `crates/coverly-core/src/organize/library_bases.rs` (`LibraryBase.region`, `keys: HashMap<String, Vec<LibraryBase>>`, `by_title_key_all`, `pick_by_region`), `crates/coverly-core/src/organize/plan.rs` (`link_demos_and_orphans` ~2655: `plan_bases` keeps every base)
- Test: `crates/coverly-core/tests/organize_game_folders.rs` (new tests after `a_demo_finds_its_base_in_the_library_by_name`), unit tests for `pick_by_region` in `library_bases.rs`

**Interfaces:**

```rust
/// D20: the demo's own region; else World, USA, Europe, Japan; else the rest by name (bytes).
pub fn pick_by_region<'a, T>(cands: &'a [T], want: Option<Region>,
                            region: impl Fn(&T) -> Option<Region>, name: impl Fn(&T) -> &str) -> Option<&'a T>;
impl LibraryIndex { pub fn by_title_key_all(&self, key: &str) -> &[LibraryBase]; }
```

- [ ] **Step 1: Write the failing tests.**

```rust
// library_bases.rs tests
#[test]
fn pick_by_region_prefers_the_match_then_world_usa_europe_japan() {
    use Region::*;
    let c = |v: &[(&'static str, Option<Region>)]| v.to_vec();
    let pick = |v: &[(&'static str, Option<Region>)], w| pick_by_region(v, w, |x| x.1, |x| x.0).map(|x| x.0);
    let bases = c(&[("Game (Japan)", Some(Japan)), ("Game (USA)", Some(Usa)), ("Game (Europe)", Some(Europe))]);
    assert_eq!(pick(&bases, Some(Europe)), Some("Game (Europe)"));
    assert_eq!(pick(&bases, None), Some("Game (USA)"));
    assert_eq!(pick(&bases, Some(Korea)), Some("Game (USA)"));
    assert_eq!(pick(&c(&[("Game (Japan)", Some(Japan)), ("Game (World)", Some(World))]), None), Some("Game (World)"));
    assert_eq!(pick(&c(&[("Game (Spain)", Some(Spain)), ("Game (France)", Some(France))]), None), Some("Game (France)"));
    assert_eq!(pick(&c(&[]), None), None);
}
```

```rust
// tests/organize_game_folders.rs
#[test]
fn a_region_less_demo_links_to_the_usa_base_not_the_first_by_name() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, lib) = (tmp.path().join("pile"), tmp.path().join("lib"));
    // "(Japan)" sorts before "(USA)": the old code took Japan.
    put(&pile, "Pokemon Y (Japan).cia", &ct::cia(0x0004_0000_0005_5E00, true, false).head_bytes(), 1);
    put(&pile, "Pokemon Y (USA).cia", &ct::cia(0x0004_0000_0005_5D00, true, false).head_bytes(), 2);
    // A 3DS demo by header: no region (a child's product code is not trusted, part A D10).
    put(&pile, "Pokemon Y.cia", &ct::cia_with_code(0x0004_0002_0005_5D00, "CTR-P-EK2A", false, false).head_bytes(), 3);
    fs::create_dir_all(&lib).unwrap();
    let plan = plan_of(&pile, &options("coverly-library", &lib));
    let demo = item(&plan, "Pokemon Y.cia");
    assert_eq!(demo.game.as_ref().and_then(|g| g.demo_of.as_deref()), Some("Pokemon Y (USA)"));
}

#[test]
fn a_demo_with_a_region_links_to_the_base_of_that_region() {
    // Resistance (Japan).pkg (JP9000-BCJS…), Resistance (USA).pkg (UP9000-BCUS…), Resistance (Europe) (Demo).pkg (EP9000-NPEA…)
    // → demo_of == "Resistance (Europe)" absent → falls back by preference: add "Resistance (Europe).pkg" (EP9000-BCES…) and expect it.
}

#[test]
fn a_library_demo_lookup_uses_the_same_order() {
    // lib/3DS/Pokemon Y (Japan)/ and lib/3DS/Pokemon Y (USA)/ game folders; pile only has the region-less demo → demo_of "Pokemon Y (USA)".
}
```

  Write the second and third bodies in full with `ps3_pkg` and `fs::create_dir_all` + `put` as in `a_demo_finds_its_base_in_the_library_by_name`.

- [ ] **Step 2: Run** `cargo test -p coverly-core --test organize_game_folders region` — the first test fails with `Some("Pokemon Y (Japan)")`.
- [ ] **Step 3: Implement.** In `library_bases.rs`, `LibraryBase` gains `pub region: Option<Region>` (`parse_tags(&shown).regions` when exactly one); `keys` becomes `HashMap<String, Vec<LibraryBase>>` (`entry(key).or_default().push(..)`); `by_title_key(key)` = `pick_by_region(self.by_title_key_all(key), None, |b| b.region, |b| b.title.as_deref().unwrap_or(""))`. In `plan.rs` change `plan_bases` to `HashMap<(String, String), Vec<(String, Option<Region>)>>` (base region = header region of the same system, else the single name-tag region) and resolve each demo with `pick_by_region(cands, demo_region, |c| c.1, |c| &c.0)`, where `demo_region` = header region else single name-tag region of the demo stem; the library fallback becomes `by_title_key_all` + `pick_by_region` with the same `demo_region`.
- [ ] **Step 4: Run** `cargo test -p coverly-core --test organize_game_folders && cargo test -p coverly-core --lib organize::library_bases`, then G-RUST (existing snapshots must not change: none had two regions of one demo's base).
- [ ] **Step 5: Commit + push.** `git commit -m "fix(core): link a region-less demo to the base of its region"` (+ push, status).

---

## Task 4: Do not add a region a name already ends with (D21, test-first)

**Files:**
- Modify: `crates/coverly-core/src/identify/names.rs` (`bare_region`), `crates/coverly-core/src/organize/target.rs` (`game_folder_name`), `crates/coverly-core/src/organize/plan.rs` (`demo_folder_stem`)
- Test: `names.rs` tests, `target.rs` tests

- [ ] **Step 1: Write the failing tests.**

```rust
// identify/names.rs tests
#[test]
fn bare_region_reads_only_a_last_word_that_is_not_the_whole_title() {
    use Region::*;
    for (stem, want) in [
        ("New Super Mario Bros 2 USA", Some(Usa)),
        ("Game usa", Some(Usa)),
        ("Game - Europe", Some(Europe)),
        ("Game EUR", Some(Europe)),
        ("Game JPN", Some(Japan)),
        ("Game JP.", Some(Japan)),
        ("Game KOR", Some(Korea)),
        ("Game CHN", Some(China)),
        ("USA Racer", None),          // not the last word
        ("Among Us", None),           // `US` is not an alias
        ("Game US", None),
        ("Super Mario World", None),  // `World` is a title word
        ("USA", None),                // the title itself
        ("Game", None),
    ] {
        assert_eq!(bare_region(stem), want, "{stem}");
    }
}
```

```rust
// organize/target.rs tests
#[test]
fn a_bare_region_word_is_not_doubled_in_a_game_folder_name() {
    let tmp = tempfile::tempdir().unwrap();
    let name = |s: &str, r| game_folder_name(s, r, tmp.path(), &mut DirIndex::new()).unwrap().0;
    assert_eq!(name("New Super Mario Bros 2 USA", Some(Region::Usa)), "New Super Mario Bros 2 USA");
    assert_eq!(name("Game USA", Some(Region::Europe)), "Game USA (Europe)");
    assert_eq!(name("USA Racer", Some(Region::Usa)), "USA Racer (USA)");
    assert_eq!(name("Super Mario World", Some(Region::Usa)), "Super Mario World (USA)");
}
```

  And in `plan.rs` tests: `demo_folder_stem("Game USA", Some(Region::Usa)) == ("Game USA (Demo)".into(), None)`.

- [ ] **Step 2: Run** `cargo test -p coverly-core --lib bare_region a_bare_region demo_folder` — fail (missing fn / doubled name).
- [ ] **Step 3: Implement.**

```rust
/// The region a stem already ends with as a bare word (D21): the last
/// whitespace-separated word, trimmed of `-_.,`, when it is a one-word
/// No-Intro region other than `World`, or `EU EUR JP JPN JAP KOR CHN`;
/// never `US` (`Among Us`), and never when it is the only word.
pub fn bare_region(stem: &str) -> Option<Region> {
    let words: Vec<&str> = stem.split_whitespace().collect();
    let last = words.last()?.trim_matches(|c| matches!(c, '-' | '_' | '.' | ','));
    let others = words[..words.len() - 1].iter().filter(|w| w.chars().any(char::is_alphanumeric)).count();
    if others == 0 || last.is_empty() { return None; }
    let alias = match last.to_ascii_uppercase().as_str() {
        "EU" | "EUR" => Some(Region::Europe),
        "JP" | "JPN" | "JAP" => Some(Region::Japan),
        "KOR" => Some(Region::Korea),
        "CHN" => Some(Region::China),
        _ => None,
    };
    alias.or_else(|| Region::from_no_intro(last).filter(|r| *r != Region::World && !r.no_intro_word().contains(' ')))
}
```

  `game_folder_name` and `demo_folder_stem`: the `Some(r) if parse_tags(stem).regions.is_empty()` arm gains `&& bare_region(stem) != Some(r)`.
- [ ] **Step 4: Run** the tests above, then G-RUST.
- [ ] **Step 5: Commit + push.** `git commit -m "fix(core): do not add a region a name already ends with"`.

---

## Task 5: The CLI lists a licence that moves with its game under its game (D22, P-7)

**Files:**
- Modify: `crates/coverly-cli/src/organize.rs` (`fn staying`, `render_table` ~1787, the moving row's extras ~1447)
- Test: `crates/coverly-cli/src/organize.rs` `mod tests`

- [ ] **Step 1: Write the failing test.**

```rust
#[test]
fn a_licence_that_moves_with_its_game_is_not_staying() {
    use coverly_core::organize::{Companion, OrganizeAction};
    let p = std::path::Path::new;
    let mut game = test_item(0, "Resistance.pkg"); // existing test builder in this module; else build OrganizeItem like the other tests here
    game.action = OrganizeAction::Auto;
    game.target = Some(p("PS3").join("Resistance").join("Resistance.pkg"));
    game.companions.push(Companion { source: p("BLUS30001.rap").into(), target: p("PS3").join("Resistance").join("BLUS30001.rap"), licence: true });
    let mut rap = test_item(1, "BLUS30001.rap");
    rap.action = OrganizeAction::Left { reason: LeftReason::LicenceOf { of: p("Resistance.pkg").into() } };
    let mut key = test_item(2, "prod.keys");
    key.action = OrganizeAction::Left { reason: LeftReason::KeyMaterial };
    let plan = plan_with_items(vec![game, rap, key]);
    let names: Vec<String> = staying(&plan).iter().map(|(i, _)| source_label(i)).collect();
    assert_eq!(names, vec!["prod.keys".to_string()]);
    assert!(render_table(&plan, &SystemDb::builtin(), &OrganizeOptions::default(), true)
        .contains("licence BLUS30001.rap goes with it"));
}
```

  If the module has no `test_item`/`plan_with_items` builders, add them at the top of `mod tests` (an `OrganizeItem` with `sources: vec![name.into()]` and defaults, an `OrganizePlan` built from the existing test plan constructor in this module).
- [ ] **Step 2: Run** `cargo test -p coverly-cli a_licence_that_moves` — fails (`staying` missing).
- [ ] **Step 3: Implement.** `fn staying(plan) -> Vec<(&OrganizeItem, &LeftReason)>`: every `Left` item, minus those whose `sources[0]` equals a `Companion { licence: true }` source of an item whose action is `Auto` with a target (or a set-aside item that moves). `render_table` uses it. On the moving row, after «with N companion file(s)», add one `licence {name} goes with it` per licence companion. Update `crates/coverly-cli/tests/cli_organize.rs` expectations if a fixture there has a licence.
- [ ] **Step 4: Run** `cargo test -p coverly-cli`, then G-RUST.
- [ ] **Step 5: Commit + push.** `git commit -m "fix(cli): list a licence that moves with its game under its game"`.

---

## Task 6: The review system selects get an id and a name (D23)

**Files:**
- Modify: `src/components/organize/ReviewBuckets.tsx` (both `NativeSelect`s), `src/components/organize/ReviewBuckets.test.tsx`

- [ ] **Step 1: Write the failing test** in `ReviewBuckets.test.tsx`:

```tsx
it('gives every system select a unique id and the name "system"', async () => {
  renderBuckets();
  await openAll();
  const selects = screen.getAllByRole('combobox');
  expect(selects.length).toBeGreaterThan(2);
  const ids = selects.map((s) => s.id);
  for (const s of selects) {
    expect(s.id).not.toBe('');
    expect(s).toHaveAttribute('name', 'system');
  }
  expect(new Set(ids).size).toBe(ids.length);
  // Accessible names are unchanged.
  expect(screen.getByRole('combobox', { name: /Pokemon Pinball\.zip/ })).toBeInTheDocument();
});
```

  (Adjust the last query to the existing `organize.review.systemFor` English text with the file name.)
- [ ] **Step 2: Run** `pnpm vitest run src/components/organize/ReviewBuckets.test.tsx` — fails (empty id).
- [ ] **Step 3: Implement.** Bucket select: `id={`${id}-system`}` `name="system"` (the bucket's `useId()` value `id` already exists). Row select: `id={`review-system-${item.id}`}` `name="system"`. Keep `aria-labelledby` / `aria-label`.
- [ ] **Step 4: Run** the test, then G-WEB.
- [ ] **Step 5: Commit + push.** `git commit -m "fix(ui): give the review system selects an id and a name"`.

---

## Task 7: Addon model and translation patch detection (D1, D2, P-2)

**Files:**
- Create: `crates/coverly-core/src/addons/mod.rs`, `crates/coverly-core/src/addons/patch.rs`, `crates/coverly-core/src/addons/testing.rs` (`#[cfg(any(test, feature = "test-util"))]`)
- Modify: `crates/coverly-core/src/lib.rs` (`pub mod addons;`)
- Test: unit tests in `patch.rs`; `crates/coverly-core/tests/addons_detect.rs` (new) for the file-level reader

**Interfaces (produced):**

```rust
// addons/mod.rs
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AddonKind { Patch, Cheat, Mod }
impl AddonKind { pub fn fixed_folder(self) -> FixedFolder { match self { Self::Patch | Self::Mod => FixedFolder::Mods, Self::Cheat => FixedFolder::Cheats } } }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AddonFormat {
    // patches (D2)
    Bps, Ups, Ips, Xdelta, Ppf, Aps,
    // cheats and emulator patches (D3)
    RetroArchCht, FceuxCht, DuckStationCht, Pnach, DolphinIni, PpssppIni, CtrCheats, SwitchCheats, Pchtxt,
    Rpcs3Patch, Shadps4Cheat, Shadps4Patch, XeniaPatch, Vita3kPatch, MelonDsCheats, MgbaCheats, DesmumeCheats, CodeText,
    // mods (D4)
    TexturePack, DolphinGraphicMod, Riivolution, LayeredFs, SwitchMod, ExefsPatches, CemuGraphicPack, VitaRepatch,
    MesenHdPack, Msu1, XeniaPlugin, N64HiresTextures, Shadps4Mods,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum AddonKey {
    SourceCrc { crc32: u32, size: u64 }, SourceName { name: String }, RomName { name: String },
    Serial { serial: String }, GameId { id: String }, TitleId { id: String }, TitleIds { ids: Vec<String> },
    SerialVersion { serial: String, version: String }, BuildId { build_id: String, title_id: Option<String> },
    N64Id { id: String }, N64Name { name: String }, Many { count: usize }, None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LinkVia { SourceCrc, Dat, Name, Readme, Key, Folder }
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HeaderFit { Matches, NeedsHeaderless, NeedsHeadered }
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum UnlinkedWhy { NoMatch, Several { count: usize }, Many { count: usize }, NoKey, Budget, Partial }
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonLink {
    pub kind: AddonKind, pub format: AddonFormat,
    /// «SLUS-00594», «0004000000055D00», «CRC32 1A2B3C4D»: data, rendered in mono.
    pub key_text: String,
    pub via: LinkVia,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header: Option<HeaderFit>,
}
/// Key validators (D14): a key becomes a path component only through these.
pub fn valid_title_id16(s: &str) -> Option<String>;    // [0-9A-Fa-f]{16} → upper
pub fn valid_sony_serial(s: &str) -> Option<String>;   // SLUS-00594, SLUS_005.94, BLUS30001, CUSA07023, NPUA80001, PCSE00001 → normalised
pub fn valid_game_id(s: &str) -> Option<String>;       // [A-Z0-9]{6} or [A-Z0-9]{3}
pub fn valid_build_id(s: &str) -> Option<String>;      // [0-9A-Fa-f]{16..64}, even length → upper
pub fn one_component(name: &str) -> Option<&str>;      // safe_file_name(name) == name and exactly one Component::Normal
```

```rust
// addons/patch.rs
/// D2: by magic first, extension second. `head` = first ≤ 64 bytes, `tail12` = last 12 (BPS/UPS),
/// `xdelta_app` = at most 4 KiB from offset 4. Never panics on short input.
pub fn sniff_patch(head: &[u8], tail12: Option<[u8; 12]>, xdelta_app: Option<&[u8]>) -> Option<(AddonFormat, AddonKey)>;
/// `.ips`, `.ips1` … `.ips9` (RetroArch chain), lower-case.
pub fn ips_chain_index(name: &str) -> Option<u8>;
/// Reads the bounded bytes of one file and sniffs it (the only fs-touching fn here).
pub fn read_patch(path: &Path) -> io::Result<Option<(AddonFormat, AddonKey)>>;
```

- [ ] **Step 1: Write the byte builders** in `addons/testing.rs`:

```rust
/// beat varint (BPS/UPS): 7 bits per byte, high bit ends, +1 carry per byte.
pub fn varint(mut n: u64) -> Vec<u8> {
    let mut out = Vec::new();
    loop { let x = (n & 0x7f) as u8; n >>= 7; if n == 0 { out.push(0x80 | x); return out; } out.push(x); n -= 1; }
}
pub fn bps(source_size: u64, source_crc: u32) -> Vec<u8> {
    let mut b = b"BPS1".to_vec();
    b.extend(varint(source_size)); b.extend(varint(source_size)); b.extend(varint(0));
    b.extend([0u8; 8]); // a dummy action stream
    b.extend(source_crc.to_le_bytes()); b.extend(0x1111_1111u32.to_le_bytes()); b.extend(0x2222_2222u32.to_le_bytes());
    b
}
pub fn ups(source_size: u64, source_crc: u32) -> Vec<u8> { /* "UPS1" + varint(src) + varint(dst) + 8 dummy + same 12-byte footer */ }
pub fn ips() -> Vec<u8> { b"PATCH\x00\x00\x10\x00\x01\xAAEOF".to_vec() }
pub fn xdelta(app: Option<&str>) -> Vec<u8> {
    let mut b = vec![0xD6, 0xC3, 0xC4, 0x00];
    match app { None => b.push(0), Some(a) => { b.push(0x04); b.extend(vcdiff_int(a.len() as u64)); b.extend(a.as_bytes()); } }
    b.extend([0u8; 16]); b
}
pub fn vcdiff_int(n: u64) -> Vec<u8> { /* big-endian base-128, continuation bit on every byte but the last */ }
pub fn ppf(version: u8) -> Vec<u8> { let mut b = format!("PPF{version}0").into_bytes(); b.extend([0u8; 60]); b }
pub fn aps_n64(cart_id: &str) -> Vec<u8> {
    // APS10, type 1 (N64), encoding 0, 50-byte description, image format, cart id (3 bytes) at offset 58, CRC 8, pad 5, size 4.
    let mut b = b"APS10".to_vec(); b.push(1); b.push(0); b.extend([b' '; 50]); b.push(0);
    b.extend(cart_id.as_bytes()); b.extend([0u8; 8 + 5 + 4]); b
}
```

- [ ] **Step 2: Write the failing tests** in `patch.rs`:

```rust
#[test]
fn bps_and_ups_give_the_source_crc_and_size() {
    let b = t::bps(1_048_576, 0x2D20_6BF7);
    let tail: [u8; 12] = b[b.len() - 12..].try_into().unwrap();
    assert_eq!(sniff_patch(&b[..b.len().min(64)], Some(tail), None),
               Some((AddonFormat::Bps, AddonKey::SourceCrc { crc32: 0x2D20_6BF7, size: 1_048_576 })));
    let u = t::ups(40_976, 0x1A2B_3C4D);
    let tail: [u8; 12] = u[u.len() - 12..].try_into().unwrap();
    assert_eq!(sniff_patch(&u[..u.len().min(64)], Some(tail), None),
               Some((AddonFormat::Ups, AddonKey::SourceCrc { crc32: 0x1A2B_3C4D, size: 40_976 })));
}
#[test]
fn varint_edges_round_trip() { for n in [0, 1, 127, 128, 16_511, 16_512, u32::MAX as u64, 1 << 40] { assert_eq!(read_varint(&t::varint(n)), Some((n, t::varint(n).len()))); } }
#[test]
fn ips_ppf_aps_and_keyless_forms() {
    assert_eq!(sniff_patch(&t::ips(), None, None), Some((AddonFormat::Ips, AddonKey::None)));
    for v in [1, 2, 3] { assert_eq!(sniff_patch(&t::ppf(v), None, None), Some((AddonFormat::Ppf, AddonKey::None))); }
    assert_eq!(sniff_patch(&t::aps_n64("NSM"), None, None), Some((AddonFormat::Aps, AddonKey::N64Id { id: "NSM".into() })));
    assert_eq!(sniff_patch(b"APS1\x00rest", None, None), Some((AddonFormat::Aps, AddonKey::None)));
}
#[test]
fn xdelta_takes_the_source_name_from_the_third_app_header_field() {
    let x = t::xdelta(Some("patched.iso//Game (USA).iso/"));
    assert_eq!(sniff_patch(&x[..4], None, Some(&x[4..])),
               Some((AddonFormat::Xdelta, AddonKey::SourceName { name: "Game (USA).iso".into() })));
    let x = t::xdelta(Some("out.iso//../../etc/Game (USA).iso/"));
    assert_eq!(sniff_patch(&x[..4], None, Some(&x[4..])),
               Some((AddonFormat::Xdelta, AddonKey::SourceName { name: "Game (USA).iso".into() })));
    let x = t::xdelta(None);
    assert_eq!(sniff_patch(&x[..4], None, Some(&x[4..])), Some((AddonFormat::Xdelta, AddonKey::None)));
}
#[test]
fn short_or_garbage_input_never_panics() {
    for len in 0..80 {
        let b = t::bps(1, 1);
        let s = &b[..len.min(b.len())];
        let _ = sniff_patch(s, None, None);
        let _ = sniff_patch(s, s.get(..12).and_then(|x| x.try_into().ok()), Some(s));
    }
    assert_eq!(sniff_patch(b"BPS1", None, None), Some((AddonFormat::Bps, AddonKey::None)));
    assert_eq!(sniff_patch(b"not a patch", None, None), None);
}
#[test]
fn ips_chain_names() {
    assert_eq!(ips_chain_index("Game.ips"), Some(0));
    assert_eq!(ips_chain_index("Game.IPS3"), Some(3));
    assert_eq!(ips_chain_index("Game.ips10"), None);
    assert_eq!(ips_chain_index("Game.bps"), None);
}
```

  In `tests/addons_detect.rs`: a renamed `.ips` that holds BPS bytes is BPS; an `.ips` holding text is `None`; `read_patch` on a 5 MB file reads at most 64 + 12 bytes (assert via `consoles::start_open_log` count and a `ReadCounter` wrapper if available, else check the function only seeks — document the bound with a comment and a size-independent timing-free test: a sparse 4 GiB file via `File::set_len` on the temp dir plans instantly).

- [ ] **Step 3: Run** `cargo test -p coverly-core --lib addons::patch` — fail (unimplemented).
- [ ] **Step 4: Implement** `patch.rs`: `read_varint(&[u8]) -> Option<(u64, usize)>` (beat: `data += (x & 0x7f) * shift; if x & 0x80 != 0 { break }; shift <<= 7; data += shift;`, checked arithmetic, ≤ 10 bytes); BPS/UPS key only when the header varint parses **and** `tail12` is present (`crc32 = u32::from_le_bytes(tail12[0..4])`); xdelta: indicator byte at offset 4 (`app[0]`), skip secondary compressor id (bit 0x01) and code table (bit 0x02, `vcdiff_int` length + bytes), then app header when bit 0x04 (cap 4 KiB, UTF-8 lossy), split on `/`, take field index 2 when ≥ 3 fields, keep the text after the last `/` or `\`, `None` if empty or `..`. APS N64 id = bytes 58..61 when `head.len() >= 61` and they are ASCII alphanumeric. `read_patch`: `File::open`, read 64, if BPS/UPS `seek(End(-12))` and read 12, if xdelta read up to 4 KiB from offset 4. Callers check `is_key_material` before calling (documented on the fn).
- [ ] **Step 5: Run** the tests, then G-RUST.
- [ ] **Step 6: Commit + push.** `git commit -m "feat(core): recognise translation patches and their source keys"`.

---

## Task 8: Cheats and emulator patches by name and content (D3)

**Files:**
- Create: `crates/coverly-core/src/addons/cheat.rs`
- Test: unit tests in `cheat.rs`

**Interfaces:**

```rust
/// D3. `name` = file name; `parents` = up to two parent folder names, nearest first; `text` = capped UTF-8 lossy (≤ 1 MiB).
/// `Some((AddonFormat::…, key))` for a cheat file; `None` for anything else.
pub fn sniff_cheat(name: &str, parents: &[&str], text: &str) -> Option<(AddonFormat, AddonKey)>;
/// `usrcheat.dat`, `cheat.db`, `mupencheat.txt`, `CHEATS_JSON.txt`, `patch_config.yml` (case-insensitive): system data (D7).
pub fn is_global_cheat_db(name: &str) -> bool;
/// Name patterns cheap enough to test before reading any text (walk pre-filter).
pub fn may_be_cheat(name: &str) -> bool;
```

- [ ] **Step 1: Write the failing table test** (one row per D3 format; synthetic text):

```rust
#[test]
fn every_cheat_format_gives_its_key() {
    use AddonFormat as F;
    let k = |s: &str| AddonKey::Serial { serial: s.into() };
    let rows: Vec<(&str, &[&str], &str, Option<(F, AddonKey)>)> = vec![
        ("Chrono Trigger (USA).cht", &[], "cheats = 2\ncheat0_desc = \"x\"\n", Some((F::RetroArchCht, AddonKey::RomName { name: "Chrono Trigger (USA)".into() }))),
        ("Contra (USA).cht", &[], "0x0032:09\n0x0033:09\n", Some((F::FceuxCht, AddonKey::RomName { name: "Contra (USA)".into() }))),
        ("SLUS-00594.cht", &[], "[Infinite HP]\nType = Gameshark\n8009C6E4 03E7\n", Some((F::DuckStationCht, k("SLUS-00594")))),
        ("SLUS-00594-3A1B2C3D4E5F6A7B.cht", &[], "[X]\nType = Gameshark\n", Some((F::DuckStationCht, k("SLUS-00594")))),
        ("SLUS-20946_A0E8A6B4.pnach", &[], "gametitle=Game\npatch=1,EE,0,word,0\n", Some((F::Pnach, k("SLUS-20946")))),
        ("A0E8A6B4.pnach", &[], "gametitle=Kingdom Hearts (USA)\n", Some((F::Pnach, AddonKey::RomName { name: "Kingdom Hearts (USA)".into() }))),
        ("C0FFEE00.pnach", &[], "patch=1,EE,0,word,0\n", Some((F::Pnach, AddonKey::None))),
        ("GALE01.ini", &[], "[Gecko]\n$Infinite\n04000000 00000000\n", Some((F::DolphinIni, AddonKey::GameId { id: "GALE01".into() }))),
        ("RMC.ini", &[], "[ActionReplay]\n$x\n", Some((F::DolphinIni, AddonKey::GameId { id: "RMC".into() }))),
        ("GALE01.ini", &[], "[Core]\nCPUThread = True\n", None),
        ("ULUS10041.ini", &[], "_S ULUS-10041\n_C0 Infinite\n_L 0x2000\n", Some((F::PpssppIni, AddonKey::GameId { id: "ULUS10041".into() }))),
        ("0004000000055D00.txt", &[], "[Infinite money]\nD3000000 00000000\n", Some((F::CtrCheats, AddonKey::TitleId { id: "0004000000055D00".into() }))),
        ("A1B2C3D4E5F60718.txt", &["cheats", "0100000000010000"], "[Moon jump]\n04000000 00000000\n",
            Some((F::SwitchCheats, AddonKey::BuildId { build_id: "A1B2C3D4E5F60718".into(), title_id: Some("0100000000010000".into()) }))),
        ("A1B2C3D4E5F60718.txt", &[], "{Master}\n04000000 00000000\n",
            Some((F::SwitchCheats, AddonKey::BuildId { build_id: "A1B2C3D4E5F60718".into(), title_id: None }))),
        ("60fps.pchtxt", &["0100000000010000"], "@nsobid-A1B2C3D4E5F60718\n@enabled\n", Some((F::Pchtxt,
            AddonKey::BuildId { build_id: "A1B2C3D4E5F60718".into(), title_id: Some("0100000000010000".into()) }))),
        ("BLUS30001_patch.yml", &[], "Version: 1.2\n", Some((F::Rpcs3Patch, AddonKey::TitleId { id: "BLUS30001".into() }))),
        ("patch.yml", &[], "PPU-abc:\n  \"x\":\n    Games:\n      \"G\":\n        BLUS30001: [ 01.00 ]\n", Some((F::Rpcs3Patch, AddonKey::TitleId { id: "BLUS30001".into() }))),
        ("patch.yml", &[], "BLUS30001: x\nBLES00001: y\nNPUA80001: z\n", Some((F::Rpcs3Patch, AddonKey::Many { count: 3 }))),
        ("CUSA07023_01.03.json", &[], "{\"name\":\"x\"}", Some((F::Shadps4Cheat, AddonKey::SerialVersion { serial: "CUSA07023".into(), version: "01.03".into() }))),
        ("CUSA07023_01.03_2.json", &[], "{}", Some((F::Shadps4Cheat, AddonKey::SerialVersion { serial: "CUSA07023".into(), version: "01.03".into() }))),
        ("Bloodborne.xml", &[], "<Patch><TitleID><ID>CUSA00900</ID></TitleID></Patch>", Some((F::Shadps4Patch, AddonKey::TitleId { id: "CUSA00900".into() }))),
        ("415607D1 - Halo 3.patch.toml", &[], "title_id = \"415607D1\"\n", Some((F::XeniaPatch, AddonKey::TitleId { id: "415607D1".into() }))),
        ("PCSE00001_60fps.txt", &[], "[PCSE00001,eboot.bin]\n@0x1234 0x00\n", Some((F::Vita3kPatch, AddonKey::TitleId { id: "PCSE00001".into() }))),
        ("patchlist.txt", &[], "[PCSE00001,eboot.bin]\n[PCSB00002,eboot.bin]\n", Some((F::Vita3kPatch, AddonKey::Many { count: 2 }))),
        ("Mario Kart DS (USA).mch", &[], "", Some((F::MelonDsCheats, AddonKey::RomName { name: "Mario Kart DS (USA)".into() }))),
        ("Pokemon Emerald (USA).cheats", &[], "", Some((F::MgbaCheats, AddonKey::RomName { name: "Pokemon Emerald (USA)".into() }))),
        ("Game.dct", &[], "", Some((F::DesmumeCheats, AddonKey::RomName { name: "Game".into() }))),
        ("Super Mario Bros. (World).txt", &[], "SXIOPO\n# lives\nAATOZE\n", Some((F::CodeText, AddonKey::RomName { name: "Super Mario Bros. (World)".into() }))),
        ("readme.txt", &[], "This is a translation patch.\n", None),
        ("../../x_patch.yml", &[], "", None), // crafted name: not one component → not a key
    ];
    for (name, parents, text, want) in rows { assert_eq!(sniff_cheat(name, parents, text), want, "{name}"); }
}
#[test]
fn global_databases_are_system_data() {
    for n in ["usrcheat.dat", "CHEAT.DB", "mupencheat.txt", "CHEATS_JSON.txt", "patch_config.yml"] { assert!(is_global_cheat_db(n), "{n}"); }
    assert!(!is_global_cheat_db("patch.yml"));
}
```

  Note on `CodeText`: `sniff_cheat` returns it whenever every non-comment line is a Game Genie (6/8 letters of `APZLGITYEOXUKSVN`, or `XXXX-XXXX`), Action Replay (`XXXXXXXX YYYYYYYY` / `XXXXXXXX:YY`) or GameShark code; the link step (Task 10) only links it when the stem equals a ROM stem (D3).
- [ ] **Step 2: Run** `cargo test -p coverly-core --lib addons::cheat` — fail.
- [ ] **Step 3: Implement** `cheat.rs` with small private matchers per format (no regex crate: hand-written char checks), serial patterns `[BN][LCP][AEJUK][SD]\d{5}` and `NP[A-Z]{2}\d{5}` for RPCS3 line scans (distinct serials counted), `CUSA\d{5}` / `PCSE\d{5}` style for shadPS4/Vita3K, 8-hex Xenia title ids, 16-hex 3DS/Switch ids; every key through the `addons::valid_*` validators; `one_component(name)` first.
- [ ] **Step 4: Run** the tests, then G-RUST.
- [ ] **Step 5: Commit + push.** `git commit -m "feat(core): recognise cheats and emulator patches by name and content"`.

---

## Task 9: Mod folders and texture packs, key material first (D4, D18)

**Files:**
- Create: `crates/coverly-core/src/addons/mods.rs`
- Test: unit tests in `mods.rs`; `crates/coverly-core/tests/addons_detect.rs` (on-disk shapes, keys rule)

**Interfaces:**

```rust
/// One directory as the walk sees it: its name, its parent names (nearest first),
/// its direct entries (name, is_dir) and, for listed sub-folders, their entries one level down.
pub struct DirView<'a> { pub name: &'a str, pub parents: &'a [&'a str], pub entries: &'a [(String, bool)],
                         pub children: &'a [(String, Vec<(String, bool)>)] }
/// Capped small text files the sniff asked for (`rules.txt`, `hires.txt`, `metadata.json`, `plugins.toml`, `riivolution/*.xml`).
pub type SmallFiles<'a> = &'a dyn Fn(&str /* path relative to the dir */) -> Option<String>;
pub enum ModVerdict { Mod { format: AddonFormat, kind: AddonKind, key: AddonKey }, HoldsKeys, NotAMod }
pub fn sniff_mod_dir(dir: &DirView<'_>, small: SmallFiles<'_>) -> ModVerdict;
/// MSU-1 sets in one listing: `<name>.msu` + `<name>-<N>.pcm` (+ `<name>.bml`); stray `.pcm`s are `Partial`.
pub fn msu1_sets(names: &[String]) -> (Vec<Msu1Set>, Vec<String>);
pub struct Msu1Set { pub name: String, pub files: Vec<String> }
/// `rules.txt` `[Definition]` `titleIds = a,b` (spaces, case) and `name`/`path`.
pub fn cemu_rules(text: &str) -> Option<(Vec<String>, bool /* cheat-named */)>;
```

- [ ] **Step 1: Write the failing tests** — one row per D4 shape from a synthetic `DirView`:
  - `SLUS-00594/` with `replacements/` → `TexturePack` `Serial("SLUS-00594")`; also as `textures/SLUS-00594/` (parent `textures`).
  - `GALE01/` with `tex1_64x64_abc_0.png` → `TexturePack` `GameId`; `GraphicMods/Bloom/metadata.json` naming `GALE01` → `DolphinGraphicMod`.
  - Riivolution pack root with `riivolution/x.xml` (`<id game="RMC">`) and the folder it names → `Riivolution` `GameId("RMC")`; an `.xml` alone → handled in Task 12 as `Partial`.
  - `ULUS10041/textures.ini` → `TexturePack` `GameId`.
  - `0004000000055D00/` with `romfs/` → `LayeredFs` `TitleId`; with `tex1_*.png` under `load/textures/` → `TexturePack`.
  - `0100000000010000/` with `romfs/` (under `atmosphere/contents/`) → `SwitchMod` `TitleId`; with only `cheats/` → handled by Task 8 per file (the folder is not a mod when it holds only `cheats/`; its files are cheats).
  - `exefs_patches/MyPatch/` with `A1B2…ips` → `ExefsPatches` `BuildId { title_id: None }`.
  - Cemu pack with `rules.txt` `titleIds = 00050000101C9400, 00050000101C9500` → `CemuGraphicPack` `TitleIds`, kind `Mod`; with `name = "Infinite Health Cheat"` → kind `Cheat`.
  - `rePatch/PCSE00001/` → `VitaRepatch` `TitleId`.
  - `Super Mario Bros. 3 (USA)/hires.txt` (`<ver>106`) → `MesenHdPack` `RomName`.
  - `plugins/4D5307E6/plugins.toml` → `XeniaPlugin` `TitleId`.
  - `SUPER MARIO 64/` holding `SUPER MARIO 64#4A3B2C1D#0#2_all.png` → `N64HiresTextures` `N64Name`.
  - `CUSA00900-mods/` → `Shadps4Mods` `TitleId("CUSA00900")`.
  - A `USRDIR/`, `dvdroot_ps4/` or `Content/` mirror with no key → `NotAMod`.
  - Any shape with a `.tik`, `title.keys` or `prod.keys` entry → `HoldsKeys` (checked **before** reading any small file: the `small` closure in the test panics if called).
  - Crafted folder names (`..`, `a/b`) → `NotAMod`.
  - `msu1_sets`: `Game.msu`, `Game-1.pcm`, `Game-2.pcm`, `Game.bml`, `Other-1.pcm` → one set of four, stray `Other-1.pcm`.

```rust
#[test]
fn a_mod_folder_with_a_key_is_holds_keys_and_reads_nothing() {
    let entries = vec![("romfs".to_string(), true), ("title.tik".to_string(), false)];
    let dir = DirView { name: "0100000000010000", parents: &["contents", "atmosphere"], entries: &entries, children: &[] };
    let small: SmallFiles<'_> = &|p| panic!("read {p}");
    assert!(matches!(sniff_mod_dir(&dir, small), ModVerdict::HoldsKeys));
}
```

  `tests/addons_detect.rs`: build the Switch shape on disk with a `.tik`, run the on-disk wrapper `mods::sniff_mod_dir_on_disk(path)` under `start_open_log()`, assert `HoldsKeys` and that `take_open_log()` contains no path ending in `.tik`.
- [ ] **Step 2: Run** `cargo test -p coverly-core --lib addons::mods && cargo test -p coverly-core --test addons_detect` — fail.
- [ ] **Step 3: Implement** `mods.rs` (pure sniff + `sniff_mod_dir_on_disk(path) -> io::Result<ModVerdict>` that lists one level, plus one more for sub-folders the shapes need, checks `is_key_material` on every listed name first, then reads only the small files asked for, ≤ 1 MiB each).
- [ ] **Step 4: Run** the tests, then G-RUST.
- [ ] **Step 5: Commit + push.** `git commit -m "feat(core): recognise mod folders and texture packs"`.

---

## Task 10: Link an addon to one game (D5, D6, D8, pure)

**Files:**
- Create: `crates/coverly-core/src/addons/link.rs`
- Test: unit tests in `link.rs`

**Interfaces:**

```rust
/// A game an addon may belong to: a plan item or a library base.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GameRef { Item(usize), Library(PathBuf) }
#[derive(Clone, Debug, Default)]
pub struct Candidate {
    pub game: GameRef, pub system: String, pub file_name: String, pub stem: String,
    pub title_key: String, pub regions: Vec<Region>, pub catalogue_name: Option<String>,
    pub size: u64, pub crc_full: Option<u32>, pub crc_headerless: Option<u32>, pub has_header: bool,
    pub serials: Vec<String>, pub game_id: Option<String>, pub title_id: Option<String>,
    pub n64_id: Option<String>, pub n64_name: Option<String>, pub keeper: bool,
}
pub struct Candidates<'a> { pub list: &'a [Candidate], pub dat_by_crc: &'a dyn Fn(u32) -> Vec<(String /* game name */, u64 /* size */)>,
                            pub budget_out: bool }
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resolution { One { game: GameRef, via: LinkVia, header: Option<HeaderFit> }, Several(usize), None, Budget }
pub fn resolve(key: &AddonKey, system: Option<&str>, own_stem: &str, c: &Candidates<'_>) -> Resolution;
/// Readme fallback for keyless patches: exactly one candidate whose file name, No-Intro name or `CRC… XXXXXXXX` appears.
pub fn resolve_by_readme(readme: &str, c: &Candidates<'_>) -> Resolution;
/// Source sizes worth hashing for a SourceCrc key: size, size + 16 (iNES), size + 512 (copier).
pub fn wanted_sizes(source_size: u64) -> [u64; 3];
```

- [ ] **Step 1: Write the failing tests** (table): `SourceCrc` full match → `One(via SourceCrc, Matches)`; headerless match on a headered ROM → `NeedsHeaderless`; full match on a headered ROM whose DAT entry is headerless → `NeedsHeadered`; DAT-only (no ROM CRC computed) whose DAT name equals a candidate's catalogue name and size → `via Dat`; two identical dumps with one `keeper` → the keeper; two without keeper → `Several(2)`; `RomName` equal stem → One; equal title key with different regions → `None` (`Chrono Trigger (USA).cht` vs `Chrono Trigger (Japan).sfc`); `Serial` after `normalise_serial` (`SLUS_005.94` ≡ `SLUS-00594`); `GameId` 3-char with two regions present → `Several(2)`; `TitleIds` with only the US base present → One; `Many(400)` → `Several(400)`-style `None` result reported as `UnlinkedWhy::Many` by the caller (resolve returns `Resolution::None`; test the caller mapping in Task 12); `BuildId { title_id: None }` → `None`; `budget_out` and no match → `Budget`; readme with one candidate name → `One(via Readme)`; with two → `Several(2)`; `wanted_sizes(1_048_576) == [1_048_576, 1_048_592, 1_049_088]`. System filter: a candidate of another system never matches.
- [ ] **Step 2: Run** `cargo test -p coverly-core --lib addons::link` — fail.
- [ ] **Step 3: Implement** `link.rs` exactly in the D5 order (plan candidates come first in `list`, library after; the caller passes them in that order and `resolve` takes the first **tier** with any hit, then applies D6: one game → `One`, several → keeper or `Several`).
- [ ] **Step 4: Run** the tests, then G-RUST.
- [ ] **Step 5: Commit + push.** `git commit -m "feat(core): link mods, patches and cheats to one game"`.

---

## Task 11: Plan model for addons and translators (types only, no behaviour change)

**Files:**
- Modify: `crates/coverly-core/src/organize/plan.rs`, `crates/coverly-cli/src/organize.rs` (exhaustive texts), `src-tauri/src/organize/view.rs` (only if it matches exhaustively), `src/ipc/types.ts`, `src/ipc/organizeWire.ts`, `src/ipc/contract.sample.json`, `src/ipc/contract.test.ts`, `src-tauri/tests/contract.rs`, `src/lib/organize.ts` (exhaustive `switch` placeholders returning the generic text until Task 22)
- Test: `plan.rs` unit tests (serde shapes, `FACTS_VERSION`), CLI text tests, `contract.test.ts`

**Interfaces (produced):**

```rust
pub enum GameRole { Base, Update, Dlc, Demo, Mod, Cheat }                         // + serde "mod", "cheat"
pub struct GameLink { …, #[serde(default, skip_serializing_if = "Option::is_none")] pub addon: Option<AddonLink> }
pub enum ItemNote { …,
    ModOf { of: String }, PatchOf { of: String }, CheatOf { of: String },
    HeaderMismatch { needs: HeaderFit },
    PointEmulatorHere { emulator: String, folder: String } }
pub enum LeftReason { …,
    AddonUnlinked { kind: AddonKind, format: AddonFormat, why: UnlinkedWhy },
    AddonNameTaken { at: PathBuf }, AddonNoPlace { kind: AddonKind },
    AddonForEmulator { emulator: String, path: PathBuf },
    NotSupported { emulator: String, why: NotSupportedWhy },
    PatchNotApplied { emulator: String } }
pub struct OrganizeOptions { …, pub emulators: BTreeMap<String, EmulatorChoice>, pub emulator_data: BTreeMap<EmulatorId, PathBuf> }
pub struct OrganizePlan { …, #[serde(default, skip_serializing_if = "Vec::is_empty")] pub translated: Vec<TranslatedGameView> }
const FACTS_VERSION: u32 = 11;
```

  `EmulatorId`, `EmulatorChoice` (`#[serde(untagged)]`-free: an enum `{ Emulator(EmulatorId), None }` with a custom kebab-case string serde: `"shadps4"` / `"none"`), `NotSupportedWhy { Rpcs3Mods, VitaRepatch, SeveralPatches, OlderUpdate, ConfigExists }` and `TranslatedGameView { system, base: Option<usize>, emulator: EmulatorId, placements: Vec<PlacementView { from: PathBuf, to: PlacementTo { System(PathBuf), EmulatorData(PathBuf), Library }, note: Option<TranslateNote> }> }` are declared in `translate/mod.rs` now (types only, Task 14 fills the module) and re-exported from `organize`.

- [ ] **Step 1: Write the failing tests:** serde round trips for every new variant (JSON shape pinned with `serde_json::json!`), `GameLink` without `addon` deserialises from a part A plan JSON, `OrganizePlan` without `translated`/`target_anchor` deserialises (old saved plan), `assert_eq!(FACTS_VERSION, 11)` (update the existing pinning test), CLI `left_text`/`note_text` for each new variant (English, D24 wording: `"its game is unknown: stays here (Translation patch, IPS)"`, `"fits 2 games: stays here"`, `"holds patches for 400 games: stays here"`, `"a file with that name is already in its folder"`, `"this layout has no folder for its game's mods: stays here"`, `"goes in DuckStation's folder: cheats/SLUS-00594.cht (pass --emulator-data)"`, `"RPCS3 does not use it like this: no mods folder"`, `"RetroArch would apply it when you play: pass --emulator snes=retroarch"`), `contract.test.ts` round trip with the new sample fields.
- [ ] **Step 2: Run** `cargo test --workspace organize::plan && pnpm vitest run src/ipc/contract.test.ts` — fail.
- [ ] **Step 3: Implement** the types, defaults and texts; TS: `GameRole` adds `'mod' | 'cheat'`, `AddonLink`, `AddonKind`, `AddonFormat` (string union), `UnlinkedWhy`, `HeaderFit`, new `ItemNote`/`LeftReason` members, `EmulatorId`, `EmulatorChoice = EmulatorId | 'none'`, `OrganizePlan.translated?: TranslatedGameView[]`, `OrganizeRequest.emulators?: Record<string, EmulatorChoice>`; unknown future kinds keep rendering the generic text.
- [ ] **Step 4: Run** G-RUST and G-WEB (every snapshot unchanged: no behaviour yet).
- [ ] **Step 5: Commit + push.** `git commit -m "feat(core): plan model for addons and emulator layouts"`.

---

## Task 12: The walk finds addon candidates (D2–D4, D7, D13, D18)

**Files:**
- Create: `crates/coverly-core/src/organize/addons.rs` (P-1: `addon_candidate`, `AddonCandidate`)
- Modify: `crates/coverly-core/src/organize/walk.rs` (`Walked.addons: Vec<AddonCandidate>`; files by magic/pattern; recognised mod folders not descended; pile game folder `Mods/`/`Cheats/` entries (D13); `-mods` overlay becomes a candidate keyed to its base; key material first), `crates/coverly-core/src/organize/plan.rs` (memoised addon verdicts in `Facts`, `FACTS_VERSION` 11 already)
- Test: `crates/coverly-core/tests/organize_addons.rs` (new: survey-level tests), `crates/coverly-core/tests/organize_walk.rs`, `crates/coverly-core/tests/organize_overlays.rs` (updated `-mods` expectation only when its base is planned)

**Interfaces:**

```rust
pub struct AddonCandidate {
    pub path: PathBuf, pub is_dir: bool, pub kind: AddonKind, pub format: AddonFormat, pub key: AddonKey,
    /// D13: filed by the user in a pile game folder's `Mods/` or `Cheats/`: the base's path.
    pub filed_under: Option<PathBuf>,
    /// MSU-1: the other files of the set (relative paths).
    pub members: Vec<PathBuf>,
    /// The readme beside a keyless patch (≤ 64 KiB), if any.
    pub readme: Option<PathBuf>,
}
pub fn addon_candidate(path: &Path, meta: &fs::Metadata, parents: &[&str]) -> Option<AddonCandidate>;
```

- [ ] **Step 1: Write the failing tests** in `tests/organize_addons.rs` (helpers copied from `organize_game_folders.rs` as in Task 2; `addons::testing` builders):
  - `a_bps_beside_a_rom_is_an_addon_candidate_not_other_files`: pile `Chrono Trigger (USA).sfc` (synthetic 1 MiB) + `Chrono Trigger (USA) [T-Es].bps` → `survey(...)` lists the patch in `walked.addons` with `SourceCrc`, not in `other_files`.
  - `a_same_stem_patch_is_still_a_companion`: `Game.ips` beside `Game.sfc` stays a companion (unchanged).
  - `a_mod_folder_is_one_candidate_and_not_descended`: `0004000000055D00/romfs/a.bin` → one `LayeredFs` candidate, no ROM candidate from inside.
  - `a_pile_game_folder_s_mods_and_cheats_entries_are_filed_addons`: the part A `pokemon_pile` → `Mods/m.ips` is an addon with `filed_under = Pokemon Y/Pokemon Y.cia`; `Extras/` and `Saves/` still `GameFolderContent`.
  - `the_shadps4_mods_overlay_is_an_addon_when_its_base_is_here`: `CUSA00900/` (extracted, `ct::` PS4 param builder) + `CUSA00900-mods/` → addon candidate `Shadps4Mods`; with no base → still `LeftReason::GameOverlay` (organize_overlays snapshot unchanged).
  - `global_cheat_databases_are_emulator_config`: `usrcheat.dat` → `NotARom::EmulatorConfig` left.
  - `a_mod_folder_with_a_key_is_left_and_the_key_never_opened`: `atmosphere/contents/0100000000010000/{romfs/x, title.tik}` under `start_open_log()` → left `KeyMaterial`, no `.tik` in `take_open_log()`.
  - `walking_2000_text_files_and_500_patches_stays_within_budget`: synthetic counts; assert planning finishes and no addon read exceeds its cap (count via the open log: each patch opened once).
- [ ] **Step 2: Run** `cargo test -p coverly-core --test organize_addons` — fail.
- [ ] **Step 3: Implement.** In `walk.rs`, in the file branch before «other files»: `is_key_material` → as today; `cheat::is_global_cheat_db` → left `EmulatorConfig`; patch extensions or `may_be_cheat` (and not a same-stem companion, which the existing companion pass keeps) → `addons::addon_candidate`. In `filter_entry` for directories: before `identify_dir_on_disk`, call `mods::sniff_mod_dir_on_disk` only for folder names that can be a key (16-hex, serial, game id, `-mods` suffix, `rePatch`/`reAddcont` parents, `hires.txt` present, `rules.txt` present, `exefs_patches`) — never for every folder; `Mod` → push candidate, `return false`; `HoldsKeys` → left `KeyMaterial`, `return false`. D13: the existing `FixedSeen::Content { folder: Mods | Cheats, of }` branch lists the folder's direct entries and pushes one candidate per entry with `filed_under` (entries that are key material → `KeyMaterial`, never opened); `Extras`/`Saves` unchanged. `-mods` overlay: emit a candidate with `filed_under = <base dir>` instead of `GameOverlay` (the plan turns it back into `GameOverlay` when the base is not planned and not in the library — Task 13). Memoise addon verdicts in `Facts` (size + mtime keyed).
- [ ] **Step 4: Run** `cargo test -p coverly-core --test organize_addons --test organize_walk --test organize_overlays --test organize_game_folders`, then G-RUST (snapshot changes allowed only where a former «other file» patch now shows as an addon row; review each `.snap.new` and accept only those).
- [ ] **Step 5: Commit + push.** `git commit -m "feat(core): find mods, patches and cheats while walking a pile"`.

---

## Task 13: Link addons in the plan and place them in Mods/ and Cheats/ (D5–D13)

**Files:**
- Modify: `crates/coverly-core/src/organize/addons.rs` (`link_addons`, `addon_targets`), `crates/coverly-core/src/organize/plan.rs` (call `link_addons` after `link_demos_and_orphans`; addon items get `ItemKind::{File, Folder, Set, Archive}` and `GameLink { role: Mod | Cheat, addon }`; `ItemNote::{ModOf, PatchOf, CheatOf}` for library bases), `crates/coverly-core/src/organize/library_bases.rs` (serial, game id, stem/title-key + region maps, D9), `crates/coverly-core/src/organize/target.rs` (addon targets; same-stem companions of a game-folder base go to `Cheats/`/`Mods/` when game folders are on, Q13), `crates/coverly-core/src/dedupe/units.rs` (addon items never units)
- Test: `crates/coverly-core/tests/organize_addons.rs`

- [ ] **Step 1: Write the failing tests:**
  - `coverly_library_places_mods_and_cheats_in_the_game_folder` (insta snapshot via `rows(&plan)`): 3DS base `Pokemon Y (World).cia` + `0004000000055D00/romfs/` + `0004000000055D00.txt` → `3DS/Pokemon Y (World)/Mods/0004000000055D00/`, `…/Cheats/0004000000055D00.txt`; `GameLink.role` `Mod`/`Cheat`, `of` = base id, `addon.key_text == "0004000000055D00"`, `via: Key`.
  - `no_mods_folder_without_mods`: a base with only cheats creates `Cheats/` and never `Mods/` (check targets and, after apply, the tree).
  - `a_ps4_cheat_json_goes_to_cheats`: `CUSA07023/` base + `CUSA07023_01.03.json` → `PS4/<Game>/Cheats/CUSA07023_01.03.json`.
  - `the_shadps4_mods_overlay_follows_its_base`: → `PS4/<Game>/Mods/CUSA00900-mods/`.
  - `filed_pile_mods_move_with_their_base` (D13): `Pokemon Y/Mods/m.ips` → `3DS/Pokemon Y/Mods/m.ips`, `via: Folder`.
  - `a_retro_patch_has_no_place_in_coverly_library` (Q10): `Chrono Trigger (USA).sfc` + `.bps` with matching CRC → patch left `AddonNoPlace { kind: Patch }`, the link still shown (`game.addon.via == SourceCrc`).
  - `a_same_name_in_cheats_is_taken_not_numbered`: library `3DS/Pokemon Y (World)/Cheats/0004000000055D00.txt` with other bytes → `AddonNameTaken` (Review); same bytes → `AlreadyPresent`.
  - `an_addon_already_in_place_is_no_item`: organise a library in place (no target) with `Mods/` content → no addon item.
  - `unlinked_addons_stay_with_their_reason`: `random.ips` (no ROM, no readme) → `AddonUnlinked { why: NoMatch }`; RPCS3 `patch.yml` with 3 serials → `why: Many { count: 3 }`; Cemu pack listing US and EU ids with both bases → `Several { count: 2 }`; a lone `.pcm` → `Partial`; a Switch `<BuildId>.txt` alone → `NoKey`.
  - `a_library_base_receives_its_addon_by_serial` (D9): `lib/PS3/Resistance (USA)/Resistance (USA).pkg` (game folder) + pile `BLUS30001_patch.yml` → `PS3/Resistance (USA)/Cheats/BLUS30001_patch.yml` with `ItemNote::CheatOf { of: "Resistance (USA)" }`.
  - `a_bps_hashes_only_same_size_candidates`: a 1.4 GB sparse GameCube image and a BPS for a 1 MiB SNES source → the hash hook counts zero GameCube hashes.
  - `same_stem_companions_go_to_fixed_folders_with_game_folders_on` (Q13): `Game.cht` + `Game.iso` (PSP, a game-folder system) → `PSP/Game/Cheats/Game.cht`; with `--no-game-folders` → companion beside the ROM, byte-identical to today's snapshot.
  - `addons_are_never_dedupe_units`: two identical cheats in two games' `Cheats/` are not a duplicate group.
  - `apply_and_undo_round_trip_with_addons` under `start_open_log()` with a `.rap` beside a PS3 base: apply → every addon in place, `Mods/`/`Cheats/` created; undo → every path back, created folders gone, no key opened.
- [ ] **Step 2: Run** `cargo test -p coverly-core --test organize_addons` — fail.
- [ ] **Step 3: Implement.** `link_addons(items, entries, walked.addons, libraries, env, limits, layout)`: build `Candidate`s from plan bases (same system; ROM hashes only for `wanted_sizes` matches, ≤ 512 MiB, under `limits.plan_head_budget`) then from `library_bases` maps for systems with unresolved addons; `filed_under` → `One { via: Folder }` directly; `resolve`, then keyless patches `resolve_by_readme`, then name fallback; `Resolution` → item: `One` → `OrganizeAction::Auto` with `GameLink { role, addon: Some(AddonLink { … }), of }`, `Several(n)` → `AddonUnlinked { why: Several { count: n } }`, `AddonKey::Many { count }` → `why: Many { count }`, `None`/`Budget` → `NoMatch`/`NoKey`/`Budget`. `-mods` overlay with no base anywhere → back to `LeftReason::GameOverlay { of }`. `addon_targets` (after `game_children_targets`): base folder known (planned or `LibraryBase.in_game_folder`) → `<folder>/<Mods|Cheats>/<own name>` (reuse an existing fixed folder's spelling via `DirIndex`), path-length check, `AlreadyPresent` / `AddonNameTaken`; base flat or system not in `GAME_FOLDER_SYSTEMS` → `AddonNoPlace` (Task 18 may still place it via a translator). `dedupe/units.rs`: skip items with `game.role ∈ {Mod, Cheat}`.
- [ ] **Step 4: Run** `cargo test -p coverly-core --test organize_addons --test organize_game_folders --test dedupe_game_folders --test organize_plan`, then G-RUST; review `.snap.new` files: only new addon rows may appear in existing snapshots.
- [ ] **Step 5: Commit + push.** `git commit -m "feat(core): place mods and cheats in the game folder"` (body: `NEEDS-OWNER: Q1, Q10, Q13 — defaults taken as in the spec`).

---

## Task 14: The pure translate module and its safety (D14)

**Files:**
- Create: `crates/coverly-core/src/translate/mod.rs` (fill the Task 11 type stubs), `crates/coverly-core/src/translate/frontend.rs`
- Modify: `crates/coverly-core/src/lib.rs` (`pub mod translate;`)
- Test: `crates/coverly-core/tests/translate.rs` (new)

**Interfaces (produced):**

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EmulatorId { Shadps4, Rpcs3, Azahar, Cemu, Vita3k, Ryujinx, YuzuFamily, RetroArch, DuckStation, Pcsx2, Dolphin, Ppsspp, Xenia, Flycast, Mesen }
impl EmulatorId { pub fn systems(self) -> &'static [&'static str]; pub fn display_name(self) -> &'static str; pub fn for_system(system: &str) -> Vec<EmulatorId>; }
pub enum EntryForm { Package, Extracted, Image, File }
pub struct GameIds { pub title_id: Option<String>, pub serial: Option<String>, pub game_id: Option<String> }
pub struct MasterEntry { pub path: PathBuf, pub is_dir: bool, pub form: EntryForm, pub title_id: Option<String>, pub version: Option<String>, pub newest: bool }
pub struct MasterAddon { pub path: PathBuf, pub is_dir: bool, pub kind: AddonKind, pub format: AddonFormat, pub key: AddonKey, pub header: Option<HeaderFit> }
pub struct MasterGame { pub system: String, pub title: String, pub ids: GameIds, pub base: MasterEntry,
                        pub updates: Vec<MasterEntry>, pub dlc: Vec<MasterEntry>, pub mods: Vec<MasterAddon>, pub cheats: Vec<MasterAddon>,
                        /// Existing names in the emulator data folder that must not be overwritten (Dolphin/PPSSPP ini, D15).
                        pub taken_in_emulator: Vec<PathBuf> }
pub enum FrontendMode { Plain, DirAsGame }
pub enum TranslateScope { Conservative, Full }
pub struct TranslateTarget { pub emulator: Option<EmulatorId>, pub frontend: FrontendMode, pub game_folders: bool, pub scope: TranslateScope }
pub enum Destination { System(PathBuf), EmulatorData { emulator: EmulatorId, path: PathBuf }, Library }
pub enum TranslateNote { InstallInEmulator { emulator: EmulatorId }, ExtractFirst, NotSupported { emulator: EmulatorId, why: NotSupportedWhy },
                         HeaderMismatch { needs: HeaderFit }, PointEmulatorHere { emulator: EmulatorId, folder: PathBuf }, PatchNotApplied { emulator: EmulatorId } }
pub struct Placement { pub from: PathBuf, pub to: Destination, pub note: Option<TranslateNote> }
pub struct TranslatedGame { pub entries: Vec<Placement> }
pub fn translate(game: &MasterGame, to: &TranslateTarget) -> TranslatedGame;
/// Master layout (no emulator): base → `<title>/<base name>`, updates → `<title>/Updates/…`, … (identity for coverly-library).
pub fn master_layout(game: &MasterGame) -> TranslatedGame;
/// Joins validated components only; `None` when any is not one `Component::Normal` (D14).
pub fn safe_join(parts: &[&str]) -> Option<PathBuf>;
```

- [ ] **Step 1: Write the failing tests** in `tests/translate.rs`:

```rust
#[test]
fn translate_never_touches_the_filesystem() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("translate");
    for entry in fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let text = fs::read_to_string(&path).unwrap();
        for banned in ["std::fs", "std::time", "SystemTime", "Instant::", "std::env", "File::"] {
            assert!(!text.contains(banned), "{} uses {banned}", path.display());
        }
    }
}
#[test]
fn crafted_keys_never_become_paths() {
    for bad in ["..", "a/b", r"a\b", "C:", "/abs", "", "x\0y", &"A".repeat(300)] {
        assert_eq!(safe_join(&["mods", bad]), None, "{bad:?}");
    }
    assert_eq!(safe_join(&["load", "mods", "0004000000055D00"]).map(|p| p.components().count()), Some(3));
}
#[test]
fn no_emulator_keeps_the_master_layout() {
    let g = fixtures::pokemon_y(); // MasterGame builder in tests/translate.rs: base + update + mod + cheat
    let t = TranslateTarget { emulator: None, frontend: FrontendMode::Plain, game_folders: true, scope: TranslateScope::Full };
    assert_eq!(translate(&g, &t), master_layout(&g));
}
#[test]
fn es_de_dir_as_game_names_the_folder_after_the_base_file() {
    // D15 frontend: game_folders on, base `Game.pkg` → System("Game.pkg/Game.pkg"), update → System("Game.pkg/Updates/…").
    // Conservative scope → DirAsGame never applied (master/today placement).
}
```

- [ ] **Step 2: Run** `cargo test -p coverly-core --test translate` — fail.
- [ ] **Step 3: Implement** `mod.rs` (`translate` dispatches to `<emulator>::translate(game, to)` — stubs returning `master_layout` until Tasks 15–16 — then `frontend::apply(…)` when `frontend == DirAsGame && game_folders && scope == Full`), `safe_join` (each part: `one_component`, ≤ 255 bytes, no NUL), `EmulatorId::systems()` from D16's table (`retroarch` = every retro system id of `SystemDb::builtin()` that is not a modern console; list them as a `const` slice and test it against the db).
- [ ] **Step 4: Run** the tests, then G-RUST.
- [ ] **Step 5: Commit + push.** `git commit -m "feat(core): add the pure translate module and its path safety"`.

---

## Task 15: Translators for shadPS4, RPCS3, Azahar, Cemu and Vita3K (D15)

**Files:**
- Create: `crates/coverly-core/src/translate/{shadps4.rs,rpcs3.rs,azahar.rs,cemu.rs,vita3k.rs}`
- Test: `crates/coverly-core/tests/translate.rs` (one table per translator)

- [ ] **Step 1: Write the failing tables** (paths by component; `sys(&["ps4","CUSA00900-UPDATE"])`-style helpers):

```rust
#[test]
fn shadps4() {
    let g = fixtures::bloodborne(); // base Extracted CUSA00900/, updates: v1.09 Extracted (newest), v1.08 Extracted; dlc Extracted; mod CUSA00900-mods/; cheat CUSA00900_01.09.json
    let full = TranslateTarget { emulator: Some(EmulatorId::Shadps4), frontend: FrontendMode::Plain, game_folders: false, scope: TranslateScope::Full };
    let t = translate(&g, &full);
    assert_eq!(dest(&t, "CUSA00900"), Destination::System(pb(&["CUSA00900"])));
    assert_eq!(dest(&t, "v1.09"), Destination::System(pb(&["CUSA00900-UPDATE"])));
    assert_eq!(placement(&t, "v1.08").to, Destination::Library);
    assert_eq!(placement(&t, "v1.08").note, Some(TranslateNote::NotSupported { emulator: EmulatorId::Shadps4, why: NotSupportedWhy::OlderUpdate }));
    assert_eq!(dest(&t, "dlc"), Destination::System(pb(&["addcont", "CUSA00900", "dlc"])));
    assert!(matches!(placement(&t, "dlc").note, Some(TranslateNote::PointEmulatorHere { .. })));
    assert_eq!(dest(&t, "CUSA00900-mods"), Destination::System(pb(&["CUSA00900-mods"])));
    assert_eq!(dest(&t, "CUSA00900_01.09.json"), Destination::EmulatorData { emulator: EmulatorId::Shadps4, path: pb(&["cheats", "CUSA00900_01.09.json"]) });
    // A package base or update: ExtractFirst, placed as without a translator.
    let t = translate(&fixtures::bloodborne_pkg(), &full);
    assert_eq!(placement(&t, "Bloodborne.pkg").note, Some(TranslateNote::ExtractFirst));
    // Conservative: base/update/DLC unchanged vs master (no addcont, no -UPDATE rewrite beyond the existing pile shape), cheats still placed.
}
#[test] fn rpcs3() { /* updates + DLC → Library + InstallInEmulator(Rpcs3); mods → NotSupported(Rpcs3Mods); BLUS30001_patch.yml → EmulatorData(patches/…) */ }
#[test] fn azahar() { /* updates + DLC → Library + InstallInEmulator; <TitleID>/ mod → EmulatorData(load/mods/<TitleID>/); texture pack → load/textures/<TitleID>/; cheats → cheats/<TitleID>.txt */ }
#[test] fn cemu() { /* updates + DLC → Library + InstallInEmulator; graphic packs (mod and cheat) → EmulatorData(graphicPacks/<pack>/) */ }
#[test] fn vita3k() { /* updates + DLC → Library + InstallInEmulator; rePatch → NotSupported(VitaRepatch); <TITLEID>*.txt → EmulatorData(patch/<name>) */ }
```

  Write every row in full with the same helpers.
- [ ] **Step 2: Run** `cargo test -p coverly-core --test translate shadps4 rpcs3 azahar cemu vita3k` — fail.
- [ ] **Step 3: Implement** the five files, each `pub(super) fn translate(g: &MasterGame, to: &TranslateTarget) -> TranslatedGame`, every path through `safe_join` (a failing key → the addon stays `Library` with `NotSupported`-free note `None` and the caller reports `NoKey`).
- [ ] **Step 4: Run** the tests, then G-RUST.
- [ ] **Step 5: Commit + push.** `git commit -m "feat(core): translators for shadPS4, RPCS3, Azahar, Cemu and Vita3K"` (body: `NEEDS-OWNER: Q3 — shadPS4 -mods overlay used`).

---

## Task 16: Translators for Switch, RetroArch and the disc-era emulators (D15)

**Files:**
- Create: `crates/coverly-core/src/translate/{switch.rs,retroarch.rs,duckstation.rs,pcsx2.rs,dolphin.rs,ppsspp.rs,xenia.rs,flycast.rs,mesen.rs}`
- Test: `crates/coverly-core/tests/translate.rs`

- [ ] **Step 1: Write the failing tables:**
  - Ryujinx / yuzu-family (Full): base `System(Zelda.nsp)`; update `System(Updates/Zelda (v131072).nsp)` + `PointEmulatorHere { folder: "Updates" }`; DLC `System(DLC/…)`; mod `0100…/romfs/` → Ryujinx `EmulatorData(mods/contents/0100…/Coverly mod/romfs…)`, yuzu `EmulatorData(load/0100…/Coverly mod/…)`; a user-named mod folder `Zelda 60fps/0100…/` keeps `Zelda 60fps` as the mod level; cheats `<BuildId>.txt` with title id → `…/0100…/cheats/<BuildId>.txt`. Conservative: base/update/DLC as master, mods/cheats placed (Q8, Q11).
  - RetroArch: one BPS `Matches` → `System(<rom dir>/<rom stem>.bps)` (Full) / `Library` + `PatchNotApplied` (Conservative); `NeedsHeaderless` → `Library` + `HeaderMismatch`; two patches for one ROM → both `Library` + `NotSupported(SeveralPatches)`; IPS chain `.ips`, `.ips1` keeps its order; `.cht` → `EmulatorData(cheats/<No-Intro system name>/<rom stem>.cht)` (system name from a const map, e.g. `snes` → `Nintendo - Super Nintendo Entertainment System`); MSU-1 set → `System(<rom stem>.msu)`, `-N.pcm` renamed to the ROM stem (Full only); other mods → `Library`.
  - DuckStation: textures → `EmulatorData(textures/<SERIAL>/…)`, cheats → `cheats/<name>`, a `.ppf` with the image's stem → `System(<image stem>.ppf)`.
  - PCSX2: textures → `textures/<SERIAL>/…`, `.pnach` → `cheats/<name>`.
  - Dolphin: textures → `Load/Textures/<GAMEID>/…`, Riivolution → `Load/Riivolution/…`, ini → `GameSettings/<GAMEID>.ini`, and when `taken_in_emulator` has it → `Library` + `NotSupported(ConfigExists)` (Q5).
  - PPSSPP: `PSP/TEXTURES/<GAMEID>/…`, `PSP/Cheats/<GAMEID>.ini` (same never-merge rule).
  - Xenia: `patches/<name>`, plugins → `plugins/<TITLEID>/…`. Flycast: `textures/<GAMEID>/…`. Mesen: `HdPacks/<rom stem>/…`.
- [ ] **Step 2: Run** `cargo test -p coverly-core --test translate` — fail.
- [ ] **Step 3: Implement** the nine files.
- [ ] **Step 4: Run** the tests, then G-RUST.
- [ ] **Step 5: Commit + push.** `git commit -m "feat(core): translators for Switch, RetroArch and disc-era emulators"` (body: `NEEDS-OWNER: Q5, Q6, Q8 — defaults taken`).

---

## Task 17: Emulator data folders as journal roots (D17)

**Files:**
- Modify: `crates/coverly-core/src/write/mod.rs` (`Writer.extra_roots`, `ensure_within_any(&self, target)`), `crates/coverly-core/src/write/undo_organize.rs` (`within_either` also accepts header `extra_roots`)
- Test: `crates/coverly-core/tests/organize_undo.rs`, `crates/coverly-core/tests/writer.rs`

- [ ] **Step 1: Write the failing tests:** `ensure_within_any` accepts a path under `root` or an extra root and refuses one in neither (and refuses each root itself); a `Moved` entry into an extra root undoes back to the source; a header with `extra_roots` parses with the old struct shape (a local copy of the pre-change `Line` enum in the test, `serde_json::from_str` succeeds, field ignored); an extra root that is a symlink is canonicalised once at begin and a later swap of the link does not redirect writes (`#[cfg(unix)]`).
- [ ] **Step 2: Run** `cargo test -p coverly-core --test writer --test organize_undo` — fail.
- [ ] **Step 3: Implement.** `begin_with` canonicalises `meta.extra_roots` (missing → the D19 rule is applied by the caller in Task 18 before begin); every organise op that checks `ensure_within(&self.root, target)` checks `self.ensure_within_any(target)`; undo's `within_either` tries root, source root, then each header extra root.
- [ ] **Step 4: Run** the tests, then G-RUST.
- [ ] **Step 5: Commit + push.** `git commit -m "feat(core): journal emulator data folders as extra roots"`.

---

## Task 18: Organizar uses translators for a device (D16, profiles)

**Files:**
- Create: `crates/coverly-core/src/organize/translate_plan.rs`
- Modify: `crates/coverly-core/src/profiles/{mod.rs,builtin.rs,custom.rs}` (`Profile.emulators: BTreeMap<String, EmulatorId>`, defaults, TOML key `emulators` with validation; every `Profile { .. }` literal gains the field), `crates/coverly-core/src/organize/plan.rs` (call `translate_plan::apply` after `addon_targets`; fill `OrganizePlan.translated`), `crates/coverly-core/src/organize/apply.rs` (emulator-data roots: D19 rule, `extra_roots` in `JobMeta`)
- Test: `crates/coverly-core/tests/organize_addons.rs`, `crates/coverly-core/tests/custom_profiles.rs`, `crates/coverly-core/tests/profile_folders.rs`

- [ ] **Step 1: Write the failing tests:**
  - `profile_defaults_keep_every_existing_target`: for every built-in profile id, plan the batch 2/3 and part A snapshot piles (reuse their builders: `tests/systems_batch2.rs`, `systems_batch3.rs`, `organize_game_folders.rs`) with no `emulators` → for every item that existed before, the same `target` and `action` as the committed snapshots (load the `.snap` files' rows and compare, ignoring new addon rows and new notes).
  - `choosing_shadps4_uses_the_full_translator`: ES-DE, `emulators: {ps4: shadps4}` → `ps4/CUSA00900-UPDATE/`, DLC under `ps4/addcont/CUSA00900/…` with `PointEmulatorHere`, `OrganizePlan.translated` has one view with those placements (snapshot).
  - `rpcs3_keeps_the_update_beside_its_base_and_says_install`: ES-DE default → PS3 update item gains `ItemNote::InstallFirst { emulator: "RPCS3" }`, target unchanged.
  - `emulator_data_given_vs_not`: DuckStation `SLUS-00594.cht` → without `emulator_data` `AddonForEmulator { emulator: "DuckStation", path: cheats/SLUS-00594.cht }`; with `emulator_data: {duckstation: <tmp>/ds}` → target under that root, apply journals `extra_roots`, undo moves it back.
  - `none_returns_the_master_layout`: `emulators: {switch: none}` with game folders on → master layout.
  - `a_linked_retro_patch_under_the_retroarch_default_is_patch_not_applied` and, with `emulators: {snes: retroarch}`, beside the ROM.
  - Profiles: `esde().emulators["ps4"] == Shadps4`, `coverly-library` empty, handhelds `retroarch` for their retro systems only; custom TOML `emulators = { ps4 = "rpcs3" }` → validation error naming `emulators.ps4`; unknown system / unknown emulator → errors; an older-build-style TOML without the key loads.
  - `emulator_data_inside_the_source_or_holding_keys_is_refused`: `emulator_data` equal to the target root, inside the source root, or holding `prod.keys` at its top → plan error (`OrganizeError::EmulatorData`), nothing planned.
- [ ] **Step 2: Run** `cargo test -p coverly-core --test organize_addons --test custom_profiles` — fail.
- [ ] **Step 3: Implement.** `translate_plan::apply(items, plan_ctx, layout, opts)`: for each system with an emulator (run choice, else profile default; `none` → skip), group master-shaped items (a base with linked children or addons; addon-only groups whose base is in the library), build `MasterGame` (paths relative to the system dir; `taken_in_emulator` from a one-level listing of the emulator data folder when given), scope = `Full` if `opts.emulators` names the system or game folders are on, else `Conservative`; map placements: `System(p)` → target via the existing collision/safe-name/FAT32/path-length rules; `Library` → keep the placement already computed + `InstallFirst`/note; `EmulatorData` → under `opts.emulator_data[emu]` or `AddonForEmulator`; `ExtractFirst`/`NotSupported`/`PatchNotApplied` → the matching `LeftReason`/`ItemNote`. Existing pile shapes (shadPS4 `-UPDATE` siblings, Batocera `DLC/{title_id}`, `.ps3` suffix) keep their code: groups they already shaped are passed to the translator only for addons in `Conservative`. Apply: emulator-data roots go through `prepare_roots`-style creation (D19 with their own anchor recorded at plan time) and into `JobMeta.extra_roots`.
- [ ] **Step 4: Run** `cargo test -p coverly-core`, then G-RUST; review every `.snap.new` (only new addon rows and notes allowed in pre-existing snapshots).
- [ ] **Step 5: Commit + push.** `git commit -m "feat(core): use translators when organising for a device"` (body: `NEEDS-OWNER: Q2, Q9, Q11, Q12 — defaults taken`).

---

## Task 19: CLI flags and texts (D24)

**Files:**
- Modify: `crates/coverly-cli/src/main.rs` (`--emulator <system>=<id|none>` repeatable, `--emulator-data <id>=<path>` repeatable, absolute only), `crates/coverly-cli/src/organize.rs` (addons under their base in the table: `    + Mod  0004000000055D00/  → Mods/  (linked by its title ID)`; translate notes; `--help` text)
- Test: `crates/coverly-cli/tests/cli_organize.rs`, `crates/coverly-cli/src/organize.rs` tests

- [ ] **Step 1: Write the failing tests:** flag parsing (`ps4=shadps4`, `ps4=none`, unknown system → error listing valid ids, unknown emulator → error, emulator that does not run the system → error, relative `--emulator-data` → error); plan text of a pile with a 3DS mod and cheat shows them under the base with kind and key; a `--dry-run` with `--emulator ps4=shadps4` prints `CUSA00900-UPDATE`; `coverly organize --apply` into `<tmp>/new/sub` (assert_cmd) completes and `coverly undo` removes `new/` (D19 at the CLI level).
- [ ] **Step 2: Run** `cargo test -p coverly-cli` — fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the tests, then G-RUST.
- [ ] **Step 5: Commit + push.** `git commit -m "feat(cli): emulator flags and addon texts"`.

---

## Task 20: Carry addon links and translated layouts to the app (IPC)

**Files:**
- Modify: `src-tauri/src/organize/view.rs` (`OrganizeRequest.emulators: Option<BTreeMap<String, EmulatorChoice>>`, `#[serde(default)]`; **no** `emulator_data`), `src-tauri/src/organize/mod.rs` (map to `OrganizeOptions.emulators`; `survey_key` resets `emulators` and `emulator_data` so a choice re-plans without a new survey), `src-tauri/tests/contract.rs`, `src/ipc/contract.requests.json`, `src/ipc/contract.sample.json`, `src/ipc/organizeWire.ts`, `src/ipc/types.ts`
- Test: `src-tauri` unit test next to `mode_names_and_game_folders_do_not_need_a_new_survey`, `src-tauri/tests/contract.rs`, `src/ipc/contract.test.ts`

- [ ] **Step 1: Write the failing tests:** `emulators_do_not_need_a_new_survey`; a request JSON with `"emulators": {"ps4": "shadps4", "switch": "none"}` deserialises; a request with `"emulator_data"` is rejected or ignored (assert the field is not on the struct: `deny_unknown_fields` is not used, so assert the mapped options have empty `emulator_data`); the sample plan with `game.addon`, the new notes/reasons and `translated` round-trips in Rust and TS.
- [ ] **Step 2: Run** `cargo test -p coverly-roms && pnpm vitest run src/ipc` (use the src-tauri crate name from `src-tauri/Cargo.toml`) — fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** G-RUST and G-WEB.
- [ ] **Step 5: Commit + push.** `git commit -m "feat(ipc): carry addon links and translated layouts to the app"`.

---

## Task 21: Mods mock pile (`?mockPick=mods`)

**Files:**
- Create: `src/ipc/mock/modsFixture.ts`
- Modify: `src/ipc/mock/mockOrganize.ts` (pick `mods`; honour `emulators` in the request: re-derive placements from fixture tables), `src/ipc/mock/mockOrganize.test.ts`

- [ ] **Step 1: Write the failing test:** `?mockPick=mods` plan contains every §9.3 case (SNES BPS CRC + IPS readme; headered NES with `NeedsHeaderless`; two IPS for one ROM; PS1 `SLUS-00594.cht` + `textures/SLUS-00594/`; 3DS mod folder + `<TitleID>.txt`; Switch base + update + `atmosphere/contents/<id>/romfs/` + `cheats/<build id>.txt`; PS4 extracted base + update folder + `CUSA…_01.03.json`; PS3 PKG update with `InstallFirst`; Cemu pack `Several`; RPCS3 `patch.yml` `Many`; unlinked `.ips`; pile game folder `Mods/` + `Cheats/`), all ids unique, every `game.of` points at an existing base; requesting `{ps4: 'shadps4'}` yields a `translated` view with `CUSA…-UPDATE`; `{switch: 'none'}` yields the master layout. Synthetic names and sizes only.
- [ ] **Step 2: Run** `pnpm vitest run src/ipc/mock` — fail.
- [ ] **Step 3: Implement** (document the pick in the `?mockPick=` comment of `mockOrganize.ts`).
- [ ] **Step 4: Run** G-WEB.
- [ ] **Step 5: Commit + push.** `git commit -m "test(ui): mods mock pile"`.

---

## Task 22: Fold mods and cheats under their game, and the copy (§9, §10)

**REQUIRED SUB-SKILL:** frontend-design:frontend-design.

**Files:**
- Modify: `src/lib/organize.ts` (`updatesUnderBases` folds `mod`/`cheat` after updates and DLC, Mods then Cheats; `addonText(link): KeyedText` («Parche BPS · CRC32 1A2B3C4D» from `organize.addon.*` + format/key data); `addonViaKey`; `gameFolderPreview` adds `Mods/` and `Cheats/` sections; `noteKey`/`leftReasonKey` for every new variant; `translatedPreview(view, cap = PREVIEW_NAMES)` → `{ system: string[], emulatorData: string[], library: string[], more: {…} }`), `src/lib/organize.test.ts`, `src/components/organize/ItemName.tsx` (addon second line: proof sentence, key in mono), `src/components/organize/SystemGroups.tsx` (addon child rows), `src/components/organize/GameFolderPreview.tsx` (`Mods/`, `Cheats/`), `locales/{es,en}/ui.json` (every §10 key, word for word; merge key by key)
- Test: `src/lib/organize.test.ts`, `src/components/organize/SystemGroups.test.tsx`, `src/components/organize/GameFolderPreview.test.tsx`, the i18n parity test

- [ ] **Step 1: Write the failing tests:** folding order (base, updates, DLC, mods, cheats) by `game.of`; an addon whose `of` is absent is not folded; `addonText` for BPS/CRC, DuckStation/serial, Azahar/title id; `addonViaKey` for each `LinkVia`; `gameFolderPreview` lists «Mods (1)» and «Trucos (1)» only when present; `translatedPreview` sections and the 20 + «y {count} más» cap; a part A plan (no `addon`, no `translated`) renders as before; `SystemGroups` renders «Mod de Pokemon Y» with `0004000000055D00` in a `font-mono` element and «Enlazado por su title ID»-style proof as text; left reasons render the §10 sentences («Encaja con 2 juegos; se queda donde está»).
- [ ] **Step 2: Run** `pnpm vitest run src/lib/organize.test.ts src/components/organize` — fail.
- [ ] **Step 3: Implement** with existing tokens only (`--cr-surface`, `--cr-border`, `--cr-text`, `--cr-text-secondary`), no badges/pills, mono only for data, `overflow-wrap:anywhere` on keys and paths.
- [ ] **Step 4: Run** G-WEB; screenshots of the fold at 1440 and 390 px, light and dark, from `pnpm dev --port 1471 --strictPort` on `/?mockPick=mods` (critique per frontend-design; stop the server by port).
- [ ] **Step 5: Commit + push.** `git commit -m "feat(ui): fold mods and cheats under their game"`.

---

## Task 23: Per-system emulator choice and the translated preview (§9.1, §9.2)

**REQUIRED SUB-SKILL:** frontend-design:frontend-design.

**Files:**
- Modify: `src/components/organize/SystemGroups.tsx` (group header «Emulador» `NativeSelect size="sm"`, `id` from `useId()` + `emulator-<system>`, `name="emulator"`, visible `<label htmlFor>`, `aria-describedby` → the group's system name; options «Predeterminado: {emulator}» (sends nothing), each emulator of the system, «Ninguno (estructura de la biblioteca)»; shown only for systems with translators; disabled while planning; stable group key so focus stays), `src/components/organize/GameFolderPreview.tsx` (emulator mode: «Ver cómo queda para {emulator}», nested `ul` lists «En la carpeta del sistema», «En la carpeta de {emulator}» (+ «Coverly no la toca si no le dices dónde está»), «Se queda en la biblioteca» (+ «Instálalo en el emulador»), `aria-label` «Carpeta de {title} para {emulator}», built only while open), the Organizar store (per-session `emulators` map sent in `OrganizeRequest.emulators`, not saved in settings, Q14), `src/lib/organize.ts` (`emulatorsFor(system)`, `defaultEmulator(profile, system)` from the plan or a TS table mirroring D16)
- Test: `src/components/organize/SystemGroups.test.tsx`, `GameFolderPreview.test.tsx`, the store test

- [ ] **Step 1: Write the failing tests:** the select appears only for systems with translators; it has an id, `name="emulator"` and a visible label; «Predeterminado: shadPS4» sends no `emulators` entry; picking shadPS4 sends `{ps4: 'shadps4'}`; «Ninguno» sends `{ps4: 'none'}`; after the re-plan the select keeps focus; the emulator preview shows the three lists, the cap and the unknown-folder sentence; reduced motion removes the collapsible transition.
- [ ] **Step 2: Run** `pnpm vitest run src/components/organize src/stores` — fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** G-WEB; screenshots (1440/390, light/dark, ES-DE layout, PS4 set to shadPS4) and critique.
- [ ] **Step 5: Commit + push.** `git commit -m "feat(ui): choose the emulator per system and preview its layout"`.

---

## Task 24: Real verification, E2E and the PR

**Files:**
- Modify: `CHANGELOG.md` (Unreleased: added/fixed lines in the existing style; no `TODO.md`, no `docs/TRACKER.md`)
- Evidence (not committed): screenshots and logs in `/private/tmp/claude-501/-Users-miguelangelesparzacalero-Documents-GitHub/35b7c020-f2ae-412f-b757-99c24a9a498b/scratchpad/mods-cheats-translators/`

- [ ] **Step 1: Update with main.** `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-mods && git fetch origin && git merge origin/main` — resolve conflicts by hand (locale files key by key), commit the merge if any, `git push`.
- [ ] **Step 2: All gates** from a clean tree: `pnpm check-all && pnpm build && cargo fmt --all --check && cargo clippy --workspace --all-targets --all-features -- -D warnings && scripts/dev/clippy-windows.sh && cargo test --workspace --no-fail-fast`. Every one green; record the tail of each output.
- [ ] **Step 3: Real-data check (read-only sources).**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-mods && mkdir -p target/mods-sample && \
  cp -RXp ~/Documents/ROMS/<two or three small system folders> target/mods-sample/ && \
  find target/mods-sample -name '._*' | wc -l   # must print 0
cargo run -p coverly-cli -- organize target/mods-sample --layout coverly-library --dry-run
cargo run -p coverly-cli -- organize target/mods-sample --layout esde --dry-run
cargo run -p coverly-cli -- organize target/mods-sample --layout esde --emulator ps4=shadps4 --dry-run
# D19: apply into a folder that does not exist yet, then undo
cargo run -p coverly-cli -- organize target/mods-sample --layout anbernic-stock --target target/mods-sample-out/new/sub --apply
cargo run -p coverly-cli -- undo
test ! -e target/mods-sample-out/new && echo "created folders removed"
rm -rf target/mods-sample target/mods-sample-out
```

  Pick the folders with `ls ~/Documents/ROMS` (read-only); never pass `~/Documents/ROMS`, `/Volumes/TinyExt/*` or `/Volumes/System` to a writing command. Note what the plan found (counts of addons linked/unlinked) for the PR.
- [ ] **Step 4: E2E (spec §14.3).** `pnpm dev --port 1471 --strictPort` in the background; Chrome DevTools MCP `new_page` with `isolatedContext: "mods-cheats-translators"`:
  1. `/?mockPick=mods`, layout «Coverly»: the 3DS base shows its mod and cheats folded with proof lines and keys in mono; «Ver cómo queda la carpeta» lists `Mods/` and `Trucos`; the unlinked `.ips`, `patch.yml` (`Many`) and the Cemu pack (`Several`) in «Se quedan donde están»; screenshots 1440 and 390 px, light and dark.
  2. Layout ES-DE: the PS4 «Emulador» select reads «Predeterminado: shadPS4» and targets match the plan before part B; pick shadPS4: «Ver cómo queda para shadPS4» with `CUSA…-UPDATE` and the addcont note; PS3 → RPCS3: the update says it is installed in the emulator; Switch → «Ninguno»: master layout returns.
  3. SNES: with the default, the linked BPS says RetroArch would apply it; choose RetroArch: beside the ROM; the header note shows; two patches for one ROM show the RetroArch reason.
  4. Review bucket: no form-field issue for the «Elige un sistema…» selects in the DevTools issues panel; keyboard-only pass over the emulator select and the disclosure; `prefers-reduced-motion: reduce` emulated; `list_console_messages` shows no errors.
  Fallback: Playwright MCP; if both profiles are locked, `cd /private/tmp/claude-501/-Users-miguelangelesparzacalero-Documents-GitHub/35b7c020-f2ae-412f-b757-99c24a9a498b/scratchpad/mods-cheats-translators && npm init -y && npm i playwright-core` and drive Chrome for Testing. Stop the server: `lsof -ti tcp:1471 -sTCP:LISTEN | xargs -r kill`.
- [ ] **Step 5: Commit + push** the changelog: `git commit -m "docs: changelog for mods, cheats and emulator translators"` && `git push && git status -sb`.
- [ ] **Step 6: Open the PR** (English, no attribution lines, no mention of Claude or AI): `gh pr create --base main --head feat/mods-cheats-translators --title "Mods/ and Cheats/ linked to their game, and per-emulator translators (2.1 part B)"` with a body listing: what ships (D1–D24), the fixes (volume-gone, demo region, bare region, CLI licences, select ids), gates with their results, E2E evidence (screenshots described), the real-data plan counts, and **Owner decisions taken by default**: Q1–Q14 from spec §13 plus every `NEEDS-OWNER` line from the commit bodies (`git log origin/main..HEAD --format=%b | grep NEEDS-OWNER`). Add a PR comment with the local gate evidence (Actions is billing-blocked; see the CI-local rule).
- [ ] **Step 7: Final state.** `git status -sb` shows `## feat/mods-cheats-translators...origin/feat/mods-cheats-translators` with nothing ahead.
