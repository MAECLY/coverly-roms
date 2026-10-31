# Missing Systems, Batch 1 (Retro Without New Formats) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking. The UI task (Task 16) also REQUIRES frontend-design:frontend-design: follow `docs/08-brand/brand.md`, use the `src/components/ui` primitives and critique the changed screen with screenshots before committing.

**Goal:** Coverly knows the 64 retro systems of `TODO.md` §2 «Tanda 1» (54 → 118 systems). Each one has a `systems.toml` entry with sourced folder aliases, extensions, libretro-thumbnails folder, libretro-database DATs and ScreenScraper id. Each built-in profile has the folder its frontend really uses, or says plainly that the device has none. PS2, 3DO and Sega Pico stop being `Unsupported`. Headers and DAT hashes identify what they can. Ambiguous extensions (`.bin`, `.rom`, `.dsk`, `.tap`, `.d88`, `.hex`…) never assign a system by extension alone. Folder-per-game ports follow a ScummVM-style rule. Each system gets a RetroArch silhouette or the Slot mount. The organise «left in place» picker groups the 118 systems by family.

**Architecture:** Data first. `systems.toml` gains three fields: `family`, `weak_extensions` (accepted inside the system's folder or with its header or hash, never counted by `extension_class`) and `scoped_aliases` (resolved only under a profile). `Profile` gains `folders: BTreeMap<id, ProfileFolder::{Named, Absent}>`, filled in `builtin.rs` from one per-system table (`SYSTEM_FOLDERS`, five frontend columns). `SystemDb::resolve_folder_in(Some(profile), name)` replaces the global lookup wherever a layout is known. Identification adds sniff rules (`identify/sniff.rs`) and disc rules (`disc/`). `organize/evidence.rs` learns the weak-extension and «weak owner proven by header» rules. `organize/walk.rs` learns the folder-game rule. The front end gets `SystemInfo.family`, a grouped `<optgroup>` picker, the regenerated icon module and a `?mockPick=retro` pile. Each system family lands as one task: its TOML rows, its folder-table rows, its rules and its tests.

**Tech Stack:** Rust 1.99 (workspace MSRV 1.93), `serde`, `toml`, `insta` JSON snapshots, `filetime`, `tempfile`; React 19 + TypeScript + Tailwind 3.4 + zustand, Vitest + Testing Library; `node scripts/icons/systems.mjs` (`pnpm system-icons`) against RetroArch Assets at the pinned commit `d9f969054dc7fbb6fa89519036d2b971e0855b51`. No new crate, no new npm package.

**Spec:** [`docs/superpowers/specs/2026-10-26-systems-batch-1.md`](../specs/2026-10-26-systems-batch-1.md). It is binding: decisions D1–D27 (§4), the folder table (§6.1), the system table (§7), the identification rules (§8), the copy (§10), the edge cases (§12), the blocked list (§13) and the sources (§16). The research it records was done on 2026-10-31 and re-checked in its §18. This plan does not repeat that research. It commits the evidence as fixtures (Task 3) so tests can check every name against it. Roadmap brief: `TODO.md` §2 «Tanda 1». Builds on [`2026-10-23-organize-archives-and-discs.md`](2026-10-23-organize-archives-and-discs.md) and [`2026-10-18-system-icons-and-views.md`](2026-10-18-system-icons-and-views.md).

## Global Constraints

- **Worktree and branch.** Work only in `/Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-systems-1`, branch `feat/systems-batch-1`, which tracks `origin/feat/systems-batch-1`. Agent shells reset their cwd, so every shell command starts with `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-systems-1 &&`. Never switch branches and never touch `main`.
- **Git rules (mandatory).** An earlier run lost a whole branch because it was never pushed.
  - Use conventional, granular commits in English (`feat(core): …`, `test(core): …`, `feat(ui): …`, `build(icons): …`, `docs: …`).
  - **No** `Co-Authored-By` line, no "Generated with" line, and no mention of Claude or AI anywhere. This owner rule overrides any harness attribution.
  - Run `git push` **immediately after every commit**. Every task ends with `git commit … && git push && git status -sb`, and the first line must read `## feat/systems-batch-1...origin/feat/systems-batch-1` with no `[ahead N]`.
  - Never force-push.
  - To update with `main`, run `git fetch origin && git merge origin/main`. Resolve conflicts by hand: other workflows change `locales/*.json`, `src/components/*` and `TODO.md` in parallel. Resolve locale files key by key, never by taking one side. Then run the gates, commit the merge and push.
- **Do not edit** `docs/TRACKER.md` or `TODO.md` on this branch. They are updated on `main` after the merge.
- **Scope.** Only Tanda 1. Tanda 2 (GameCube, Wii, Wii U, 3DS, Switch, Vita, PS3, Xbox, Xbox 360, DSi) and Tanda 3 (PS4, PS5, Xbox One/Series) are separate later runs. Do not add their ids, aliases or icons.
- **Never guess a folder name.** Every `Named` folder comes from spec §6.1, which cites §16. If a task finds a cell it cannot trace to the pinned source, it makes that cell `None` (Absent) and lists it in the PR. A frontend without a folder for a system leaves that profile without it.
- **Ambiguous extensions.** `bin`, `iso`, `img`, `rom`, `dat`, `chd` stay Generic. A Shared extension (`.dsk`, `.tap`, `.d88`, `.2hd`, `.hdm`, `.sna`, `.ccc`, `.cas`, `.hdf`, `.fdi`, `.dim`) gives only 0.50 (review). A weak extension (D3, D4) never assigns a system on its own. Only a folder alias, a header or a DAT hash can assign one.
- **TDD.** Write the failing test, run it and watch it fail for the expected reason, implement, and watch it pass. Each "Run" step names the exact command.
- **Gates (same as CI).** Before every commit that touches code or tests, run:
  - **G-RUST:** `cargo fmt --all --check && cargo clippy --workspace --all-targets --all-features -- -D warnings && scripts/dev/clippy-windows.sh && cargo test --workspace --no-fail-fast`
  - **G-WEB** (any task touching `src/`, `locales/`, `scripts/icons/` or `src-tauri/`): `pnpm check-all && pnpm build`
  - Docs-only commits run `pnpm check-all`.
- **Windows CI leg.**
  - Fixture mtimes are pinned: use `coverly_core::dedupe::testing::pin_mtime`, or `filetime::set_file_mtime` to a fixed time. Never use "now".
  - Compare paths by component (`Path::components`, `file_name`, `Path::join`), never by slash text. Snapshot paths go through the existing `/` normalising redaction.
  - Unix-only helpers go in `#[cfg(unix)] mod unix_tests`.
  - No new folder name is a Windows reserved name, and none ends in a dot or a space (Task 2 test).
- **Real data is read-only.** Never write to `~/Documents/ROMS`, `/Volumes/TinyExt/*` or `/Volumes/System`, and never create `._*` files there. Only `ls`, `stat` and Coverly **plan-only** runs may touch them.
  - A write test on real-like data works on a copy under `/Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-systems-1/target/systems-sample/`, made with `cp -RXp`, checked with `find target/systems-sample -name '._*' | wc -l` (must print `0`) and deleted afterwards with `rm -rf target/systems-sample`.
  - Test fixtures are synthetic bytes built in the test, with no copyrighted data. Real game names may appear as file names over synthetic content.
- **No UI strings in the core.** Reasons are enums plus data, and the UI and CLI render them. Every UI string lives in `locales/en/ui.json` **and** `locales/es/ui.json` with the same keys and placeholders (`src/i18n/parity.test.ts`). Spanish copy: sentence case, no exclamation marks, exact counts, numbers in mono. The copy is spec §10, word for word.
- **Brand.** Only existing tokens from `design/tokens.json` (`pnpm tokens:check` stays green). Use the `src/components/ui` primitives, and `SystemIcon` for every system mark.
- **Icons.** Only RetroArch Assets XMB Monochrome (CC BY 4.0) at commit `d9f969054dc7fbb6fa89519036d2b971e0855b51`, generated with `pnpm system-icons`. Never hand-edit the generated files. Logos and wordmarks use the Slot mount (`TRADEMARKS.md`).
- **Secrets are never read or printed:** the minisign key, `~/.coverly-release/`, any `.env*`, `.vercel/`.
- **Servers and E2E.**
  - The web UI runs on port 1465 only: `pnpm dev --port 1465 --strictPort` (mock backend). Stop it by port only: `lsof -ti tcp:1465 -sTCP:LISTEN | xargs -r kill`.
  - E2E uses Chrome DevTools MCP `new_page` with `isolatedContext: "systems-batch-1"`. Fall back to Playwright MCP. If both profiles are locked, install `playwright-core` in `/private/tmp/claude-501/-Users-miguelangelesparzacalero-Documents-GitHub/35b7c020-f2ae-412f-b757-99c24a9a498b/scratchpad/systems-batch-1` and drive Chrome for Testing from there.
- **Blocked items never block the batch.** Anything that needs an owner decision (spec §13, or a «verify» rule of §8 that cannot be verified) is implemented around, recorded in the task's commit body and collected into the PR's «Blocked / needs the owner» section (Task 17).

## Deviations from the spec (decided in this plan)

- **P-1: one folder table per system, not per profile.** Spec §5.1 says "`const` arrays of `(id, Option<&str>)`" per profile. This plan uses one table, `SYSTEM_FOLDERS: &[(&str, [Option<&str>; 5])]`, with the columns `[esde, batocera, muos, onion, minui]`. GarlicOS reads the Onion column (D9). Anbernic stock is Absent for every row (D11), and RetroArch has no entries (it keeps `Id`). Each family task appends its rows next to its TOML rows, so one diff shows a system's whole folder story. The behaviour is the same as the spec's.
- **P-2: the `.hdf`/`.fdi`/`.dim` header rules land with Computers II (Task 9), not Computers III.** The class change happens when `x68000` (`.hdf`, `.dim`) and `pc98` (`.fdi`) list those extensions. The rules that keep loose Amiga and Atari ST disks automatic must ship in the same commit (spec §15 item 8 wants exactly that pairing).
- **P-3: an Absent system with a host keeps the host folder without the note.** D5 says Absent gives `on_device = Some(false)`. D23 adds hosts only where the system is Absent. So `system_folder` applies the order «user override → Named → host → naming rule». The Absent «no folder on device» mark applies only when no host takes the system: the host folder exists on the device by definition (it is in `ANBERNIC_FOLDERS` or in the Onion package list).
- **P-4: one batch harness test file.** Step 5 of the brief (header, target folder, cover lookup, plan snapshot) is satisfied by a table-driven integration test `crates/coverly-core/tests/systems_batch1.rs`, created in Task 3. Each family task adds its rows to its tables. The three plan snapshots over the full §9.3 pile are written once all systems exist (Task 14), so they are reviewed once rather than rewritten in every task.
- **P-5: the research evidence is committed as fixtures** (Task 3): the live libretro-thumbnails root listing, the libretro-database `.dat` path list at `2c93952f`, and the RetroArch Assets monochrome file list at `d9f96905`. Tests assert every §7 name against them, so a typo fails offline.

## Review Focus

1. **Silent reclassification of existing libraries.** No pre-batch extension changes class except D3's list (`.dsk`, `.dim`, `.hdf`, `.fdi` become Shared; `.sgx` and `.bs` move to their new systems). No existing folder alias changes owner except `msx2` (MSX → MSX2). Anbernic `.sgx`/`.bs`/MSX2/XE files still go to `PCE`/`SFC`/`MSX`/`A800` (D23). Tests: Task 1 `extension_classes` snapshot (its diff over the batch must equal D3 exactly), Task 2 `anbernic_hosts_keep_old_destinations`, Task 9 `loose_amiga_and_st_disks_stay_automatic`, Task 10 `msx1_still_msx_and_msx2_moves`.
2. **Folder names traced to sources.** Every `Named` cell resolves back to its system under its profile, every first alias and Named folder is a valid Windows name, and global aliases are unique. Scoped aliases (`doom`, `dragon`, `pet`, `x1`…) never resolve without a profile, so a `Dragon/` folder in Downloads is still organised and profile detection is unchanged. Tests: Task 2 `named_folders_resolve_back`, `scoped_aliases_need_a_profile`, `pico_is_sega_pico_only_on_batocera`.
3. **Ambiguous files go to review.** A loose `.bin`, `.rom`, `.dsk`, `.hex`, `.wasm`, `.lua`, `.p` or `.0` is never Auto by extension. A weak owner wins only with a ≥ 0.95 header (D24). A `.wad` without `IWAD`/`PWAD` drops to 0.60. Zip-based packages are never extracted (D25). Tests: Task 3 `weak_extension_alone_is_never_evidence`, `package_exts_are_raw_items`; Task 12 `wad_without_magic_is_not_auto`.
4. **Disc systems.** PS2/3DO/PC-FX/CD-i/CD32/CDTV/Jaguar CD become `Known`, the PS2 serial reaches the DAT ladder, `.zso` is name-only, `multi_file` blocks archives as-is, and facts memoised as `Unsupported` are re-read (`FACTS_VERSION` 6). Tests: Task 4.
5. **Windows.** Pinned mtimes, component path comparisons, reserved names, digit-only extensions (`.0`, `.81`). `scripts/dev/clippy-windows.sh` runs on every task.

---

## File structure

```
crates/coverly-core/
├─ src/systems/systems.toml          + family on all 54; weak/scoped fields; 64 new [[system]] (Tasks 4–13)
├─ src/systems/mod.rs                Family, weak_extensions, scoped_aliases, accepts_extension,
│                                      systems_accepting, resolve_folder (unscoped), resolve_folder_in
├─ src/profiles/mod.rs               ProfileFolder, Profile.folders, Profile::folder_owner
├─ src/profiles/builtin.rs           SYSTEM_FOLDERS table, D23 hosts, folders per built-in
├─ src/profiles/naming.rs            system_folder order (P-3), default_name/validate under profile
├─ src/profiles/custom.rs            clone fills folder_names; to_profile → folders; preview under profile
├─ src/scan/mod.rs, scan/classify.rs resolve_folder_in(Some(profile)); accepts_extension
├─ src/organize/walk.rs              WalkOptions.profile; folder-game rule (Task 12)
├─ src/organize/plan.rs              LeftReason::FolderGame; FACTS_VERSION 6; quarantine/probe under profile
├─ src/organize/evidence.rs          ext~ weak evidence; D24 contradiction
├─ src/organize/inspect.rs           PACKAGE_EXTS (D25); MAME CHD companions (Task 13)
├─ src/identify/sniff.rs             new hits; PS2/Pico hits; DECISIVE; TEXT_NATIVE_EXTS
├─ src/identify/hash.rs              NAME_ONLY_EXTS += zso
├─ src/disc/{mod,iso9660,signatures}.rs  Known(ps2/3do/pcfx/cdi/amigacd32/cdtv/jaguarcd)
├─ src/disc/testing.rs               builders for PC-FX, CD-i, CD32, CDTV, Jaguar CD heads
├─ src/dedupe/units.rs               folder-game dirs out of scope like ports
├─ src/settings/validate.rs          folder overrides validated under the profile
└─ tests/
   ├─ systems_batch1.rs              (new) table-driven harness: load, target, header, cover (P-4)
   ├─ systems_classes.rs             (new) extension-class snapshot
   ├─ fixtures/research/             (new) thumbnails-root.html, libretro-database-dats.txt,
   │                                   retroarch-assets-monochrome.txt (P-5)
   ├─ fixtures/dat/                  (new) trimmed tosec / dat/ / homebrew DATs
   └─ sniff.rs, disc_identify.rs, organize_*.rs, dedupe_units.rs   updated assertions
crates/coverly-cli/src/dedupe.rs     resolve_folder_in
src-tauri/src/ipc.rs                 SystemInfo.family
src/ipc/types.ts, contract.sample.json, contract.test.ts   family; LeftReason folder-game
src/lib/systems.ts (+ .test.ts)      (new) systemOptionGroups
src/lib/organize.ts                  copy for folder-game, extension-weak
src/components/organize/LeftInPlace.tsx (+ test)  grouped picker
src/components/system/systemFallbacks.ts          FALLBACK_ABBR, WORDMARK_IDS
src/components/system/systemIcons.generated.ts, src/assets/system-icons/*   regenerated
scripts/icons/systemIconsLib.mjs     new SYSTEM_ICON_SOURCES rows
src/ipc/mock/mockOrganize.ts, fixture.ts          ?mockPick=retro, shelves fixture
locales/{es,en}/ui.json              spec §10 keys
THIRD-PARTY-NOTICES.md               icon count and ids
CHANGELOG.md                         Unreleased: systems, behaviour changes (spec §12)
```

---

## Task 1: Core: `family`, `weak_extensions`, `scoped_aliases` and the extension-class snapshot

No new system yet. Behaviour is unchanged, but every existing system gets a family.

**Files:**
- Modify: `crates/coverly-core/src/systems/mod.rs`
- Modify: `crates/coverly-core/src/systems/systems.toml` (header comment; `family` on the 54 entries)
- Modify: `crates/coverly-core/src/scan/classify.rs` (`accepts_extension`; test helper literal)
- Create: `crates/coverly-core/tests/systems_classes.rs`, snapshot `tests/snapshots/systems_classes__extension_classes.snap`

**Interfaces (produced):**
```rust
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Family { #[default] Console, Handheld, Computer, Arcade, Engine }
impl Family {
    pub const ALL: [Family; 5];
    pub fn parse(s: &str) -> Option<Family>;
    pub fn as_str(self) -> &'static str;
}
// System additions
pub family: Family,
pub weak_extensions: Vec<String>,
pub scoped_aliases: Vec<String>,
impl System {
    /// Strong or weak: may this system's folder hold a file with `ext`?
    pub fn accepts_extension(&self, ext: &str) -> bool;
    pub fn is_scoped_alias(&self, name: &str) -> bool;
}
impl SystemDb {
    /// Strong and weak owners of `ext` (folder, header and hash paths).
    pub fn systems_accepting(&self, ext: &str) -> Vec<&System>;
    /// Built-in invariants; `Err` lists every violation (used by a test,
    /// never at runtime, so user files stay lenient).
    pub fn check_builtin_invariants(&self) -> Result<(), Vec<String>>;
}
```

- [ ] **Step 0: Sync.** `git fetch origin && git merge origin/main && git push && git status -sb`
- [ ] **Step 1: Write the failing unit tests** in `systems/mod.rs` `mod tests`:

```rust
#[test]
fn every_builtin_system_has_a_family() {
    let db = SystemDb::builtin();
    let get = |id: &str| db.get(id).unwrap().family;
    assert_eq!(get("snes"), Family::Console);
    assert_eq!(get("gba"), Family::Handheld);
    assert_eq!(get("c64"), Family::Computer);
    assert_eq!(get("mame"), Family::Arcade);
    assert_eq!(get("scummvm"), Family::Engine);
    db.check_builtin_invariants().unwrap();
}

#[test]
fn weak_extensions_are_accepted_but_not_classed() {
    let db = SystemDb::builtin()
        .with_overrides(
            r#"
            [[system]]
            id = "zz"
            name = "ZZ"
            aliases = ["ZZ"]
            extensions = ["zzq"]
            weak_extensions = ["st"]
            "#,
        )
        .unwrap();
    let zz = db.get("zz").unwrap();
    assert!(zz.accepts_extension("ST") && zz.accepts_extension(".zzq"));
    assert_eq!(db.extension_class("st"), ExtClass::Unique); // still atarist only
    let ids: Vec<_> = db.systems_accepting("st").iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["atarist", "zz"]);
    assert_eq!(db.systems_for_extension("st").len(), 1);
}

#[test]
fn user_files_are_lenient_about_the_new_fields() {
    // A strong extension in a user file removes the same weak one; an
    // unknown family falls back to the default; neither is an error.
    let db = SystemDb::builtin()
        .with_overrides(
            r#"
            [[system]]
            id = "zz"
            name = "ZZ"
            family = "spaceship"
            weak_extensions = ["zzq"]
            [[system]]
            id = "zz"
            extensions = ["zzq"]
            "#,
        )
        .unwrap();
    let zz = db.get("zz").unwrap();
    assert_eq!(zz.family, Family::Console);
    assert!(zz.weak_extensions.is_empty() && zz.extensions == ["zzq"]);
    let arcade = SystemDb::builtin()
        .with_overrides("[[system]]\nid = \"yy\"\nname = \"YY\"\narcade = true\n")
        .unwrap();
    assert_eq!(arcade.get("yy").unwrap().family, Family::Arcade);
}

#[test]
fn scoped_aliases_do_not_resolve_without_a_profile() {
    let db = SystemDb::builtin()
        .with_overrides(
            r#"
            [[system]]
            id = "zz"
            name = "ZZ"
            aliases = ["ZZBOX", "Dragon"]
            scoped_aliases = ["dragon"]
            "#,
        )
        .unwrap();
    assert_eq!(db.resolve_folder("zzbox").map(|s| s.id.as_str()), Some("zz"));
    assert!(db.resolve_folder("DRAGON").is_none());
}
```

  Also update `override_new_system_is_appended` (≈ line 670). It adds `tic80`, which becomes built-in in Task 11. Rename its id to `zz-tic` and its aliases to `["ZZTIC", "zztic"]` now, so Task 11 does not break it.
- [ ] **Step 2: Run** `cargo test -p coverly-core --lib systems::` and expect compile errors (`Family`, `accepts_extension`… missing).
- [ ] **Step 3: Implement** in `systems/mod.rs`:
  - The `Family` enum above, with `parse` (case-insensitive: `console|handheld|computer|arcade|engine`).
  - `RawSystem` gains `family: Option<String>`, `weak_extensions: Option<Vec<String>>`, `scoped_aliases: Option<Vec<String>>`. Keep `deny_unknown_fields`: the new fields are known now.
  - `merge`:
    - New systems start with `family: if raw.arcade == Some(true) { Family::Arcade } else { Family::Console }`, empty `weak_extensions` and empty `scoped_aliases`.
    - `family`: `Some(v)` → `Family::parse(&v).unwrap_or(<default above>)`. An unknown value is ignored with `tracing::warn!` if the crate already depends on `tracing`. Otherwise it is ignored silently, with a comment that says so. Check `grep -n '^tracing' crates/coverly-core/Cargo.toml`, and do not add a crate.
    - `weak_extensions` are normalised like `extensions` and appended, minus any that are strong for this system.
    - After the strong extensions are appended: `system.weak_extensions.retain(|w| !system.extensions.contains(w))` (the user wins).
    - `scoped_aliases` are lowercased and appended. When the user claims an alias, the matching scoped entries are removed from the other systems too (beside the existing `other.aliases.retain`).
  - `System::is_scoped_alias(name)` is `self.scoped_aliases.iter().any(|a| a.eq_ignore_ascii_case(name))`.
  - `System::accepts_extension(ext)` normalises `ext`, then checks `extensions` and `weak_extensions`.
  - `resolve_folder` skips an alias when `s.is_scoped_alias(alias)`.
  - `systems_accepting` is like `systems_for_extension`, but uses `accepts_extension`.
  - `check_builtin_invariants` collects these violations:
    - a weak extension that is also strong for the same system;
    - a scoped alias that is not one of the system's aliases;
    - an alias (case-insensitive) claimed by two systems;
    - a weak extension that is a container (`zip`, `7z`).
  - Header comment of `systems.toml`: document `family`, `weak_extensions` and `scoped_aliases` in the style of the existing lines.
  - Add `family = "…"` to the 54 entries:
    - `handheld`: gb, gbc, gba, gamegear, gw, lynx, nds, ngp, ngpc, pokemini, psp, virtualboy, wonderswan, wonderswancolor.
    - `computer`: a800, amiga, atarist, c64, dos, msx, vic20.
    - `arcade`: every `arcade = true` entry (atomiswave, cps1, cps2, cps3, fbneo, hbmame, mame, naomi, neogeo, pgm2, varcade).
    - `engine`: easyrpg, ons, openbor, pico8, scummvm.
    - `console`: the rest.
  - `scan/classify.rs:170`: `system.extensions.iter().any(|e| e == ext)` becomes `system.accepts_extension(ext)`. The test helper `system()` gains `family: Family::Console, weak_extensions: Vec::new(), scoped_aliases: Vec::new()`. Fix every other `System { … }` literal that `cargo build --workspace --all-targets` reports.
- [ ] **Step 4: Write the snapshot test** `crates/coverly-core/tests/systems_classes.rs`:

```rust
//! Pins the class of every extension the built-in database lists, with
//! its strong and weak owners. A batch that changes one must show it in
//! this snapshot's diff (spec D3, §14.1).

use std::collections::BTreeSet;

use coverly_core::systems::SystemDb;

#[test]
fn extension_classes() {
    let db = SystemDb::builtin();
    let exts: BTreeSet<String> = db
        .all()
        .iter()
        .flat_map(|s| s.extensions.iter().chain(&s.weak_extensions).cloned())
        .collect();
    let rows: Vec<serde_json::Value> = exts
        .iter()
        .map(|e| {
            serde_json::json!({
                "ext": e,
                "class": format!("{:?}", db.extension_class(e)),
                "strong": db.systems_for_extension(e).iter().map(|s| &s.id).collect::<Vec<_>>(),
                "accepting": db.systems_accepting(e).iter().map(|s| &s.id).collect::<Vec<_>>(),
            })
        })
        .collect();
    insta::assert_json_snapshot!(rows);
}
```

- [ ] **Step 5: Run** `cargo test -p coverly-core --lib systems:: && INSTA_UPDATE=always cargo test -p coverly-core --test systems_classes`. Read the new `.snap` and check that every extension of today's `systems.toml` is there with its current class. Then run `cargo insta pending-snapshots` (or `git status`) and expect nothing left pending.
- [ ] **Step 6: G-RUST, G-WEB** (`src-tauri` builds against `System`).
- [ ] **Step 7: Commit and push:** `git add -A crates && git commit -m "feat(core): add system families, weak extensions and profile-scoped aliases" && git push && git status -sb`

---

## Task 2: Core: profile folder tables and profile-scoped folder resolution

**Files:**
- Modify: `crates/coverly-core/src/profiles/mod.rs`, `profiles/builtin.rs`, `profiles/naming.rs`, `profiles/custom.rs`
- Modify: `crates/coverly-core/src/systems/mod.rs` (`resolve_folder_in`)
- Modify (callers, spec §3 «Folder resolution»): `src/scan/mod.rs:248`, `src/organize/walk.rs:161` (`WalkOptions` gains `profile: Option<&'a Profile>`; its constructors in `organize/plan.rs` pass the target profile), `src/organize/plan.rs:2148` (quarantine) and `:2695` (probe), `src/profiles/custom.rs:418` (preview), `src/settings/validate.rs:278, 338`, `crates/coverly-cli/src/dedupe.rs:206`. **Unchanged** (profile-less by definition, D22): `profiles/detect.rs:90`, `profiles/fit.rs:35`.
- Modify: every `Profile { … }` literal without `..` that the build reports (`src-tauri/tests/profiles_state.rs`, `src-tauri/tests/organize_state.rs`, test helpers in `crates/coverly-core/tests/*` if any). Add `folders: BTreeMap::new()`.
- Create: `crates/coverly-core/tests/profile_folders.rs`

**Interfaces (produced):**
```rust
// profiles/mod.rs
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProfileFolder {
    /// The frontend's folder for the system (spec §6.1).
    Named(String),
    /// The frontend has no folder for it: organise uses the naming rule
    /// and marks the folder as not on the device (D5).
    Absent,
}
// Profile addition
pub folders: BTreeMap<String, ProfileFolder>,
impl Profile {
    /// The system id whose `Named` folder is `name` (case-insensitive).
    pub fn folder_owner(&self, name: &str) -> Option<&str>;
}
// systems/mod.rs
impl SystemDb {
    /// `profile`'s Named folders first, then every alias (scoped ones
    /// included); `None` profile = `resolve_folder`.
    pub fn resolve_folder_in(&self, profile: Option<&Profile>, name: &str) -> Option<&System>;
}
// profiles/builtin.rs
/// Columns of [`SYSTEM_FOLDERS`].
const ESDE: usize = 0; const BATOCERA: usize = 1; const MUOS: usize = 2;
const ONION: usize = 3; const MINUI: usize = 4;
/// Spec §6.1: each Tanda 1 system's folder per frontend (None = Absent).
/// GarlicOS reads the Onion column (D9); Anbernic stock is Absent for all
/// (D11); RetroArch keeps `Id` and has no entries.
const SYSTEM_FOLDERS: &[(&str, [Option<&str>; 5])] = &[];
fn folders(column: Option<usize>) -> BTreeMap<String, ProfileFolder>;
```

- [ ] **Step 0: Sync** (as Task 1).
- [ ] **Step 1: Write the failing tests** `crates/coverly-core/tests/profile_folders.rs`. The tests iterate whatever the table holds, so they keep checking as Tasks 4–13 add rows:

```rust
use std::collections::BTreeMap;

use coverly_core::profiles::naming::{system_folder, validate_folder_override};
use coverly_core::profiles::{builtin_profiles, ProfileFolder};
use coverly_core::systems::SystemDb;

const RESERVED: [&str; 22] = [
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

fn windows_safe(name: &str) -> bool {
    !name.is_empty()
        && !name.ends_with(['.', ' '])
        && !RESERVED.contains(&name.to_ascii_lowercase().as_str())
        && !name.contains(['<', '>', ':', '"', '/', '\\', '|', '?', '*'])
}

#[test]
fn named_folders_resolve_back_and_absent_ones_are_marked() {
    let db = SystemDb::builtin();
    let none = BTreeMap::new();
    for p in builtin_profiles() {
        for (id, f) in &p.folders {
            let system = db.get(id).unwrap_or_else(|| panic!("{}: unknown id {id}", p.id));
            let choice = system_folder(&p, system, &db, &none);
            match f {
                ProfileFolder::Named(n) => {
                    assert_eq!(&choice.folder, n, "{} {id}", p.id);
                    assert_eq!(
                        db.resolve_folder_in(Some(&p), n).map(|s| s.id.as_str()),
                        Some(id.as_str()),
                        "{} {n}", p.id
                    );
                    assert!(validate_folder_override(&db, &p, id, n).is_ok());
                    assert!(windows_safe(n), "{n}");
                }
                ProfileFolder::Absent if choice.host.is_none() => {
                    assert_eq!(choice.on_device, Some(false), "{} {id}", p.id)
                }
                ProfileFolder::Absent => assert_ne!(choice.on_device, Some(false)),
            }
        }
    }
}

#[test]
fn first_aliases_are_windows_safe() {
    for s in SystemDb::builtin().all() {
        assert!(windows_safe(&s.aliases[0]), "{}", s.id);
    }
}

#[test]
fn retroarch_has_no_table_and_anbernic_is_all_absent() {
    let ps = builtin_profiles();
    let ra = ps.iter().find(|p| p.id == "retroarch").unwrap();
    assert!(ra.folders.is_empty());
    let anb = ps.iter().find(|p| p.id == "anbernic-stock").unwrap();
    assert!(anb.folders.values().all(|f| *f == ProfileFolder::Absent));
}

#[test]
fn existing_systems_keep_their_folders() {
    // No pre-batch system is in the tables (D5): same names as before.
    let db = SystemDb::builtin();
    let none = BTreeMap::new();
    for p in builtin_profiles() {
        for id in ["snes", "psx", "gba", "msx", "pcengine", "a800"] {
            assert!(!p.folders.contains_key(id), "{} {id}", p.id);
            let _ = system_folder(&p, db.get(id).unwrap(), &db, &none);
        }
    }
}
```

  In `tests/custom_profiles.rs`, add `clone_copies_named_folders`. Build `ProfileFile::from_profile(&batocera)`, then assert that `folder_names` holds exactly the `Named` entries of `batocera.folders` (none yet, so the map is empty, and the test becomes meaningful from Task 4 on). Assert that `to_profile()` returns `folders` with the same Named entries and no Absent, and that the TOML round-trips.
- [ ] **Step 2: Run** `cargo test -p coverly-core --test profile_folders --test custom_profiles` and expect compile errors.
- [ ] **Step 3: Implement.**
  - `Profile.folders` and `ProfileFolder`. `folder_owner` scans `Named` values with `eq_ignore_ascii_case`.
  - `builtin.rs`:
    - `fn folders(column: Option<usize>)`: `Some(c)` maps each row to `Named(cell)` or `Absent`; `None` (Anbernic) maps every row to `Absent`.
    - `imgs_profile(id, …)` takes the column: anbernic-stock `None`, garlicos and onion `Some(ONION)`.
    - `profile(…)` sets `folders(Some(MUOS))`, `Some(MINUI)`, `Some(ESDE)` or `Some(BATOCERA)` by id, and `BTreeMap::new()` for retroarch.
    - Keep `SYSTEM_FOLDERS` empty for now.
  - `resolve_folder_in`:

```rust
pub fn resolve_folder_in(&self, profile: Option<&Profile>, name: &str) -> Option<&System> {
    let Some(profile) = profile else {
        return self.resolve_folder(name);
    };
    if name.is_empty() {
        return None;
    }
    if let Some(s) = profile.folder_owner(name).and_then(|id| self.get(id)) {
        return Some(s);
    }
    self.systems
        .iter()
        .find(|s| s.aliases.iter().any(|a| a.eq_ignore_ascii_case(name)))
}
```

  - `naming.rs` `system_folder`, order per P-3:

```rust
let own = profile.folders.get(&system.id);
let host = match own {
    Some(ProfileFolder::Named(_)) => None,
    _ => profile.host_folders.get(&system.id).and_then(|h| db.get(h)).filter(|h| h.id != system.id),
};
let owner = host.unwrap_or(system);
let user = overrides
    .get(&system.id)
    .map(|f| f.trim().to_string())
    .filter(|f| validate_folder_override(db, profile, &system.id, f).is_ok());
let folder = user
    .clone()
    .or_else(|| match own {
        Some(ProfileFolder::Named(n)) => Some(n.clone()),
        _ => None,
    })
    .unwrap_or_else(|| default_name(profile, owner, db));
let on_device = if !profile.known_folders.is_empty() {
    Some(profile.known_folders.iter().any(|k| k.eq_ignore_ascii_case(&folder)))
} else if matches!(own, Some(ProfileFolder::Absent)) && host.is_none() && user.is_none() {
    // The user chose the name: no «no folder on device» note (spec §12).
    Some(false)
} else {
    None
};
```

  - `default_name(profile, system, db)` takes the profile. For `Id`, it checks `db.resolve_folder_in(Some(profile), &system.id)`.
  - `validate_folder_override` resolves with `db.resolve_folder_in(Some(profile), folder.trim())`.
  - `custom.rs`:
    - `from_profile` fills `folder_names` from `Named` entries.
    - `to_profile` maps `folder_names` to `folders` (`Named`). Remove the comment at ≈ line 190 that says custom names are merged only under settings; keep that merge.
    - `preview_paths` uses `resolve_folder_in(Some(&profile), …)`.
    - `validate_profile`'s `folder_names` check validates against the profile being validated.
  - Callers: pass the active profile at every site listed under **Files**.
    - `settings/validate.rs:278` keeps `resolve_folder` when no built-in matches (custom profile, as its comment says). The `:338` quarantine check accepts a folder that resolves under **any** built-in (`builtins.iter().any(|p| db.resolve_folder_in(Some(p), name).is_some()) || db.resolve_folder(name).is_some()`).
- [ ] **Step 4: Run** `cargo test -p coverly-core --test profile_folders --test custom_profiles --test profile_naming --test organize_walk --test scan_fixtures` and expect them to pass. Then G-RUST and G-WEB.
- [ ] **Step 5: Commit and push:** `git add -A crates src-tauri && git commit -m "feat(core): per-profile folder tables and profile-scoped folder resolution" && git push && git status -sb`

---

## Task 3: Core: identification plumbing, research fixtures and the batch harness

**Files:**
- Modify: `crates/coverly-core/src/organize/evidence.rs` (weak evidence, D24)
- Modify: `crates/coverly-core/src/organize/inspect.rs` (`PACKAGE_EXTS`, D25)
- Modify: `crates/coverly-core/src/identify/sniff.rs` (`TEXT_NATIVE_EXTS` += `hex`, `nx`, `lua`, `game`, D26)
- Modify: `crates/coverly-core/src/organize/plan.rs` (`FACTS_VERSION` 5 → 6, D27)
- Create: `crates/coverly-core/tests/fixtures/research/thumbnails-root.html`, `libretro-database-dats.txt`, `retroarch-assets-monochrome.txt`, `README.md` (where, when and at which commit each was fetched)
- Create: `crates/coverly-core/tests/systems_batch1.rs`

- [ ] **Step 0: Sync.**
- [ ] **Step 1: Capture the research fixtures** (P-5). Use public endpoints only, with no credentials:

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-systems-1 && R=crates/coverly-core/tests/fixtures/research && mkdir -p $R && \
curl -fsS https://thumbnails.libretro.com/ -o $R/thumbnails-root.html && \
gh api 'repos/libretro/libretro-database/git/trees/2c93952f3cdf02837d2b34112aea5ee844d42b8b?recursive=1' \
  --jq '.tree[] | select(.path | endswith(".dat")) | .path' | sort > $R/libretro-database-dats.txt && \
gh api 'repos/libretro/retroarch-assets/git/trees/d9f969054dc7fbb6fa89519036d2b971e0855b51?recursive=1' \
  --jq '.tree[] | select(.path | startswith("src/xmb/monochrome/")) | .path' | sort > $R/retroarch-assets-monochrome.txt && \
wc -l $R/*.txt && grep -c 'href=' $R/thumbnails-root.html
```

  Expected: the root listing has 127 folders (spec §7), and the trees are not truncated (`gh api … --jq .truncated` prints `false`; if it prints `true`, fetch the `metadat/` and `dat/` subtrees separately). Write `README.md` with the three URLs, the date and the commits.
- [ ] **Step 2: Write the failing tests.**
  - `organize_evidence.rs`:

```rust
#[test]
fn weak_extension_alone_is_never_evidence() {
    let db = SystemDb::builtin()
        .with_overrides("[[system]]\nid = \"zz\"\nname = \"ZZ\"\nweak_extensions = [\"zzq\"]\n")
        .unwrap();
    let ev = extension_evidence(&db, "zzq", &Sniff { read: 4096, ..Sniff::default() });
    assert!(ev.is_empty());
}

#[test]
fn weak_owner_header_contradicts_the_strong_extension() {
    // D24: `.st` is atarist's; a ≥ 0.95 header for a system that lists
    // `st` as weak drops atarist's claim to 0.60 with detail `st!`.
    let db = SystemDb::builtin()
        .with_overrides("[[system]]\nid = \"zz\"\nname = \"ZZ\"\nweak_extensions = [\"st\"]\n")
        .unwrap();
    let mut sniff = Sniff { read: 4096, ..Sniff::default() };
    sniff.hits.push(Evidence::new(IdLevel::Header, "zz", 0.95, "ZZ MAGIC"));
    let ev = extension_evidence(&db, "st", &sniff);
    let st = ev.iter().find(|e| e.system == "atarist").unwrap();
    assert_eq!((st.weight, st.detail.as_str()), (0.60, "st!"));
}
```

  - `organize_inspect.rs` `package_exts_are_raw_items`: a zip named `Game.arduboy` holding `info.json` + `game.hex`, and one named `Game.rpk` holding `layout.xml` + `game.bin`. `inspect` returns one raw item each, `Container::Raw` and no extracted entries.
  - `sniff.rs`: `text_native_game_files_are_not_text_roms`. An Intel HEX head `b":100000000C9435000C945D000C945D000C945D0024\n"` with ext `hex` and a BASIC source with ext `nx` both give `not_a_rom == None`.
  - `organize_plan.rs` `facts_of_version_5_are_read_again`: store a facts row with `v = 5`, then check that the planner recomputes it. Use the existing facts cache test pattern near the `FACTS_VERSION` uses, and assert on `stored.v == 6` after a plan.
  - `systems_batch1.rs` harness, empty tables for now:

```rust
//! Table-driven checks for the systems of batch 1 (plan P-4). Each family
//! task appends rows; every check runs over every row.

mod common;

use std::collections::{BTreeMap, BTreeSet};

use coverly_core::identify::sniff::{sniff, SliceAt};
use coverly_core::profiles::builtin_profiles;
use coverly_core::profiles::naming::system_folder;
use coverly_core::sources::libretro::parse_apache_dirs;
use coverly_core::systems::{Family, SystemDb};

/// (id, family, multi_file)
const SYSTEMS: &[(&str, Family, bool)] = &[];
/// (profile, id, expected folder, on device): spot checks of spec §6.1.
const TARGETS: &[(&str, &str, &str, Option<bool>)] = &[];
/// (id, ext, head builder, expected system or None): header rules (§8.1).
type Head = fn() -> Vec<u8>;
const HEADERS: &[(&str, &str, Head, Option<&str>)] = &[];

fn research(name: &str) -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/research").join(name),
    )
    .unwrap()
}

#[test]
fn batch_systems_load_with_family_and_disc_flag() {
    let db = SystemDb::builtin();
    for (id, family, multi) in SYSTEMS {
        let s = db.get(id).unwrap_or_else(|| panic!("missing {id}"));
        assert_eq!((s.family, s.multi_file), (*family, *multi), "{id}");
    }
}

#[test]
fn libretro_names_exist_on_the_thumbnail_server() {
    let dirs: BTreeSet<String> = parse_apache_dirs(&research("thumbnails-root.html")).into_iter().collect();
    for s in SystemDb::builtin().all() {
        if let Some(name) = &s.libretro {
            assert!(dirs.contains(name), "{}: `{name}` not on thumbnails.libretro.com", s.id);
        }
    }
}

#[test]
fn dat_paths_exist_in_the_pinned_database() {
    let paths: BTreeSet<String> = research("libretro-database-dats.txt").lines().map(String::from).collect();
    let db = SystemDb::builtin();
    for (id, _, _) in SYSTEMS {
        for dat in coverly_core::identify::dat::dat_sources(db.get(id).unwrap()) {
            assert!(paths.contains(&dat), "{id}: {dat}");
        }
    }
}

#[test]
fn organise_targets_match_the_research_table() {
    let db = SystemDb::builtin();
    let ps = builtin_profiles();
    let none = BTreeMap::new();
    for (profile, id, folder, on_device) in TARGETS {
        let p = ps.iter().find(|p| &p.id == profile).unwrap();
        let c = system_folder(p, db.get(id).unwrap(), &db, &none);
        assert_eq!((c.folder.as_str(), c.on_device), (*folder, *on_device), "{profile} {id}");
    }
}

#[test]
fn header_rules() {
    for (id, ext, head, want) in HEADERS {
        let bytes = head();
        let s = sniff(&mut SliceAt(&bytes), bytes.len() as u64, ext);
        let best = s.hits.iter().max_by(|a, b| a.weight.total_cmp(&b.weight)).map(|h| h.system.as_str());
        assert_eq!(best, *want, "{id} .{ext}");
    }
}
```

  Put the cover lookup in the same file. `cover_lookup_finds_batch_systems` uses the `MockServer` + `apache_listing` pattern of `tests/libretro.rs` `gba_server`. It serves the committed root listing at `/`, and for the first `SYSTEMS` row with a `libretro` name it serves `/<encode_segment(name)>/Named_Boxarts/` holding `Test Game (World).png`. Then `LibretroSource::server_system` and a `Media::Cover` query for `Test Game (World)` must return a `MatchKind::Exact` hit. While `SYSTEMS` is empty, the test returns early, with a comment.
- [ ] **Step 3: Run** `cargo test -p coverly-core --test organize_evidence --test organize_inspect --test sniff --test organize_plan --test systems_batch1` and expect the new tests to fail.
- [ ] **Step 4: Implement.**
  - `extension_evidence`:
    - (a) Strong owners as today.
    - (b) D24: for a strong owner `s`, when `sniff.hits` has a hit `h` with `h.weight >= 0.95`, `h.system != s.id`, and `db.get(&h.system)` lists `ext` in `weak_extensions`, the owner gets `0.60`, `format!("{ext}!")`.
    - (c) Weak owners add **nothing** here. Their 0.50 `ext~` support is added by the planner only when a folder alias or a header already names that system: in `organize/evidence.rs` where folder and header evidence are combined, push `Evidence::new(IdLevel::Extension, id, 0.50, format!("{ext}~"))` for a weak owner that already has non-extension evidence.
    - Unit test `weak_extension_supports_a_header`.
  - `inspect.rs`: `pub const PACKAGE_EXTS: [&str; 5] = ["arduboy", "rpk", "solarus", "lutro", "uef"];`. In `container_for`, return `Container::Raw` for those extensions before looking at the magic. Header sniffs that need the inner files (Tasks 8, 11, 12) read them through `archive::open_archive` on the same bytes, never through extraction.
  - `TEXT_NATIVE_EXTS`: grow the array from 10 to 14 entries with `hex`, `nx`, `lua`, `game`.
  - `FACTS_VERSION = 6`, with a doc line: «6: PS2, 3DO and Pico verdicts became systems (systems batch 1)».
- [ ] **Step 5: Run** the tests from Step 3 (pass), then G-RUST.
- [ ] **Step 6: Commit and push.** Use two commits, each pushed:
  - `git add crates/coverly-core/tests/fixtures/research && git commit -m "test(core): commit the libretro and icon research listings for systems batch 1" && git push`
  - `git add -A crates && git commit -m "feat(core): weak-extension evidence, raw game packages, text-native game files" && git push && git status -sb`

---

## Task 4: Systems: disc family (PS2, 3DO, PC-FX, CD-i, CD32, CDTV, Jaguar CD, FM Towns)

**Files:**
- Modify: `systems/systems.toml` (append under a `# Batch 1: disc systems` comment at the end of the file)
- Modify: `profiles/builtin.rs` (`SYSTEM_FOLDERS` rows)
- Modify: `identify/sniff.rs` (PS2 hit), `identify/hash.rs` (`NAME_ONLY_EXTS` += `zso`, 9 → 10)
- Modify: `disc/mod.rs`, `disc/iso9660.rs` (`PS2` const), `disc/signatures.rs` (PC-FX, CD-i, CD32/CDTV, Jaguar CD), `disc/testing.rs` (builders)
- Modify: `organize/inspect.rs:242` stays as is for `Unsupported` (kept for future systems)
- Modify tests: `tests/disc_identify.rs:116-149, 627`, `tests/sniff.rs:547, 607`, `tests/organize_inspect.rs:1670`, `tests/systems_batch1.rs`, `tests/organize_target.rs`, `tests/identify_hash.rs`

**TOML rows** (spec §7; `zip`/`7z` are not listed for disc systems):

```toml
[[system]]
id = "ps2"
name = "Sony PlayStation 2"
family = "console"
aliases = ["PS2", "playstation2"]
extensions = ["zso", "iso", "chd", "cue", "bin", "m3u"]
weak_extensions = ["cso"]
libretro = "Sony - PlayStation 2"
multi_file = true
screenscraper = 58

[[system]]
id = "3do"
name = "3DO Interactive Multiplayer"
family = "console"
aliases = ["3DO", "3d0"]
extensions = ["iso", "cue", "bin", "chd"]
libretro = "The 3DO Company - 3DO"
multi_file = true
screenscraper = 29

[[system]]
id = "pcfx"
name = "NEC PC-FX"
family = "console"
aliases = ["PCFX", "pc-fx"]
extensions = ["cue", "bin", "chd", "ccd"]
libretro = "NEC - PC-FX"
multi_file = true
screenscraper = 72

[[system]]
id = "cdi"
name = "Philips CD-i"
family = "console"
aliases = ["CDI", "cdimono1", "cd-i"]
scoped_aliases = ["cdi"]
extensions = ["chd", "cue", "bin", "iso"]
libretro = "Philips - CD-i"
multi_file = true
screenscraper = 133

[[system]]
id = "amigacd32"
name = "Commodore Amiga CD32"
family = "console"
aliases = ["CD32", "amigacd32"]
extensions = ["cue", "bin", "iso", "chd"]
libretro = "Commodore - CD32"
multi_file = true
screenscraper = 130

[[system]]
id = "cdtv"
name = "Commodore CDTV"
family = "console"
aliases = ["CDTV", "amigacdtv"]
extensions = ["cue", "bin", "iso", "chd"]
libretro = "Commodore - CDTV"
multi_file = true
screenscraper = 129

[[system]]
id = "jaguarcd"
name = "Atari Jaguar CD"
family = "console"
aliases = ["JAGUARCD", "atarijaguarcd"]
extensions = ["cue", "bin", "chd"]
weak_extensions = ["cdi"]
multi_file = true
dats = ["metadat/redump/Atari - Jaguar CD.dat"]
screenscraper = 171

[[system]]
id = "fmtowns"
name = "Fujitsu FM Towns"
family = "computer"
aliases = ["FMTOWNS", "fm-towns"]
extensions = ["cue", "bin", "chd", "iso", "d88"]
multi_file = true
screenscraper = 253
```

**Folder rows** (`[esde, batocera, muos, onion, minui]`, spec §6.1):

```rust
    ("ps2", [Some("ps2"), Some("ps2"), None, None, None]),
    ("3do", [Some("3do"), Some("3do"), Some("3do"), None, None]),
    ("pcfx", [Some("pcfx"), Some("pcfx"), Some("pcfx"), None, None]),
    ("cdi", [Some("cdimono1"), Some("cdi"), Some("cdi"), None, None]),
    ("amigacd32", [Some("amigacd32"), Some("amigacd32"), None, None, None]),
    ("cdtv", [Some("cdtv"), Some("amigacdtv"), None, None, None]),
    ("jaguarcd", [Some("atarijaguarcd"), Some("jaguarcd"), None, None, None]),
    ("fmtowns", [Some("fmtowns"), Some("fmtowns"), None, None, None]),
```

- [ ] **Step 0: Sync.**
- [ ] **Step 1: Rewrite the old assertions to the new verdicts, then watch them fail.**
  - `tests/disc_identify.rs`: every `DiscSystem::Unsupported("PlayStation 2")` becomes `DiscSystem::Known("ps2")`, and `Unsupported("3DO")` becomes `Known("3do")`. Keep each test's serial assertions, and add `assert_eq!(disc.serials, ["SLUS-20062"])` where the builder writes `BOOT2 = cdrom0:\SLUS_200.62;1`.
  - `tests/sniff.rs:547, 607`: `ps2.unsupported` becomes a hit: `assert!(ps2.hits.iter().any(|h| h.system == "ps2" && h.weight >= 0.95))` with `BOOT2`, and `0.80` for the large `PLAYSTATION` volume without `SYSTEM.CNF`. `assert!(ps2.unsupported.is_none())`.
  - `tests/organize_inspect.rs:1670`: the PS2 ISO case now expects a `ps2` placement, not `LeftReason::UnsupportedSystem`.
  - New tests in `disc_identify.rs`, using builders added to `disc/testing.rs` (synthetic sectors only):
    - `pcfx_wins_over_pc_engine_cd`: sector 0 `PC-FX:Hu_CD-ROM` **and** the PCE text at sector 1 → `Known("pcfx")`.
    - `cdi_disc_label`: sector 16 with record type `0x01`, `CD-I `, version `0x01` → `Known("cdi")`.
    - `cd32_and_cdtv_by_root_file`: an ISO 9660 root holding `CD32.TM` → `Known("amigacd32")`, and `CDTV.TM` → `Known("cdtv")`.
    - `jaguar_cd_session_two_header`: the first sector of the session-2 data track holds `ATARI APPROVED DATA HEADER ATRI ` → `Known("jaguarcd")`; the byte-swapped form too. A single-session CUE gives `None`.
    - `opera_is_3do`.
  - `tests/identify_hash.rs` `zso_is_name_only`.
  - `tests/systems_batch1.rs`: append the 8 rows to `SYSTEMS` (`Family::Console`, `true`, except `fmtowns`, which is `Family::Computer`, `true`) and these `TARGETS`: `("esde","ps2","ps2",None)`, `("onion","ps2","PS2",Some(false))`, `("anbernic-stock","ps2","PS2",Some(false))`, `("batocera","cdtv","amigacdtv",None)`, `("esde","cdi","cdimono1",None)`, `("muos","3do","3do",None)`, `("minui","pcfx","PCFX",Some(false))`.
  - `tests/organize_target.rs` `ps2_archive_is_extracted_never_moved_as_is`: a zip holding an ISO with the PS2 head → `Transfer::Extract` on every built-in.
- [ ] **Step 2: Run** `cargo test -p coverly-core --test disc_identify --test sniff --test organize_inspect --test identify_hash --test systems_batch1 --test organize_target` and expect failures.
- [ ] **Step 3: Implement.**
  - Append the TOML and the folder rows above.
  - `disc/iso9660.rs:391`: `const PS2: DiscSystem = DiscSystem::Known("ps2");` (update the doc comment at :323).
  - `disc/mod.rs`:
    - Opera → `Known("3do")`.
    - Check order in `identify_with`: Opera, Sega, **PC-FX** (`signatures::is_pcfx(&sector0)`), PC Engine CD, **CD-i** (`signatures::is_cdi(src)?`, sector 16), PlayStation, PVD, then UMD, **CD32/CDTV** (`signatures::amiga_cd(src, &pvd)?` → `Some("amigacd32" | "cdtv")`), Neo Geo CD.
    - Jaguar CD needs the session-2 track: `organize/inspect.rs` calls `signatures::is_jaguar_cd(first_sector_of_session_2)` when the sheet has `REM SESSION 02`. Otherwise the disc is identified by folder and name only (§8.2).
    - Confidences: PC-FX 0.95, CD-i 0.90, CD32/CDTV 0.90, Jaguar CD 0.95. Use details from a fixed vocabulary: `"PC-FX:Hu_CD-ROM"`, `"CD-I disc label"`, `"CD32.TM"`, `"CDTV.TM"`, `"ATARI APPROVED DATA HEADER"`.
  - `identify/sniff.rs:367-370`: `s.hit("ps2", 0.95, "SYSTEM.CNF BOOT2")` and `_ if len > CD_MAX => s.hit("ps2", 0.80, "PLAYSTATION > CD")`.
  - D24 then makes a PS2 `.cso` contradict `psp`, through the CISO reader (spec §12): test `ps2_cso_contradicts_psp` in `organize_evidence.rs`.
  - `NAME_ONLY_EXTS`: add `"zso"`.
  - FM Towns has no signature (§8.2): folder and name only, with no DAT (`dats` absent, `libretro` absent).
  - **Verify rules** marked «verify» in spec §8.2 (CD-i label, CD32/CDTV files) against the cited emulator source (MAME `cdi` driver / FS-UAE `cdtv`/`cd32` detection) before keeping them. If a rule cannot be confirmed, drop it, keep folder + hash, and write `BLOCKED: <rule> unverified` in the commit body.
  - Onion `extlist` check: no row here has an Onion folder, so there is nothing to compare.
- [ ] **Step 4: Run** the Step 2 tests (pass). Also `INSTA_UPDATE=always cargo test -p coverly-core --test systems_classes`. The diff may only add the new extensions (`zso`, `ccd` gains `pcfx`, `cue`/`m3u` gain owners, `cso` accepting gains `ps2`). Review it line by line, then G-RUST.
- [ ] **Step 5: Commit and push:** `git add -A crates && git commit -m "feat(core): add PS2, 3DO, PC-FX, CD-i, CD32, CDTV, Jaguar CD and FM Towns" && git push && git status -sb`

---

## Task 5: Systems: early consoles (ColecoVision, Intellivision, Vectrex, Odyssey², Channel F, Arcadia, SCV, Jaguar)

**Files:** `systems/systems.toml`, `profiles/builtin.rs`, `identify/sniff.rs` (`vectrex` joins `DECISIVE`, so the array grows), `tests/systems_batch1.rs`, `tests/sniff.rs`, `tests/organize_evidence.rs`

**TOML rows:**

```toml
[[system]]
id = "colecovision"
name = "ColecoVision"
family = "console"
aliases = ["COLECO", "colecovision"]
extensions = ["col", "rom", "bin", "zip", "7z"]
libretro = "Coleco - ColecoVision"
screenscraper = 48

[[system]]
id = "intellivision"
name = "Mattel Intellivision"
family = "console"
aliases = ["INTELLIVISION", "intv"]
extensions = ["int", "bin", "rom", "zip", "7z"]
libretro = "Mattel - Intellivision"
dats = ["metadat/no-intro/Mattel - Intellivision.dat", "metadat/homebrew/Mattel - Intellivision.dat"]
screenscraper = 115

[[system]]
id = "vectrex"
name = "GCE Vectrex"
family = "console"
aliases = ["VECTREX", "gcevectrex"]
extensions = ["vec", "bin", "zip", "7z"]
weak_extensions = ["gam"]
libretro = "GCE - Vectrex"
dats = ["metadat/no-intro/GCE - Vectrex.dat", "metadat/homebrew/GCE - Vectrex.dat"]
screenscraper = 102

[[system]]
id = "odyssey2"
name = "Magnavox Odyssey² / Videopac"
family = "console"
aliases = ["ODYSSEY", "odyssey2", "videopac", "o2em"]
extensions = ["bin", "zip", "7z"]
libretro = "Magnavox - Odyssey2"
screenscraper = 104

[[system]]
id = "channelf"
name = "Fairchild Channel F"
family = "console"
aliases = ["FAIRCHILD", "channelf"]
extensions = ["chf", "bin", "zip", "7z"]
libretro = "Fairchild - Channel F"
screenscraper = 80

[[system]]
id = "arcadia"
name = "Emerson Arcadia 2001"
family = "console"
aliases = ["ARCADIA", "arcadia2001"]
scoped_aliases = ["arcadia"]
extensions = ["bin", "zip", "7z"]
libretro = "Emerson - Arcadia 2001"
screenscraper = 94

[[system]]
id = "scv"
name = "Epoch Super Cassette Vision"
family = "console"
aliases = ["SCV", "supercassettevision"]
extensions = ["bin", "rom", "zip", "7z"]
weak_extensions = ["0"]
libretro = "Epoch - Super Cassette Vision"
screenscraper = 67

[[system]]
id = "jaguar"
name = "Atari Jaguar"
family = "console"
aliases = ["JAGUAR", "atarijaguar"]
extensions = ["j64", "jag", "rom", "bin", "zip", "7z"]
weak_extensions = ["abs", "cof"]
libretro = "Atari - Jaguar"
screenscraper = 27
```

**Folder rows:**

```rust
    ("colecovision", [Some("colecovision"), Some("colecovision"), Some("colecovision"), Some("COLECO"), Some("COLECO")]),
    ("intellivision", [Some("intellivision"), Some("intellivision"), Some("intellivision"), Some("INTELLIVISION"), None]),
    ("vectrex", [Some("vectrex"), Some("vectrex"), Some("vectrex"), Some("VECTREX"), None]),
    ("odyssey2", [Some("odyssey2"), Some("odyssey2"), Some("odyssey2"), Some("ODYSSEY"), None]),
    ("channelf", [Some("channelf"), Some("channelf"), Some("channelf"), Some("FAIRCHILD"), None]),
    ("arcadia", [Some("arcadia"), Some("arcadia"), None, None, None]),
    ("scv", [Some("scv"), Some("scv"), Some("scv"), None, None]),
    ("jaguar", [Some("atarijaguar"), Some("jaguar"), Some("jaguar"), None, None]),
```

- [ ] **Step 0: Sync.**
- [ ] **Step 1: Failing tests.**
  - `tests/sniff.rs`, with builders added to `identify::sniff::testing`:
    - `vectrex_gce_header`: `g GCE` at 0 → `vectrex` 0.95; the same bytes with ext `vec` and without the magic → no hit.
    - `colecovision_test_bytes_are_weak`: `AA 55` at 0 → `colecovision` 0.60, never 0.95.
    - `jaguar_abs_and_cof`: `60 1B` and `01 50` heads with ext `abs`/`cof` → `jaguar` 0.50.
  - `tests/organize_evidence.rs` `loose_bin_of_an_early_console_is_never_auto`: `Game.bin` with no header goes to review or is left; `Game.col` → colecovision (unique 0.85 plus no contradiction → Auto, as for any unique extension); `Game.rom` → nothing.
  - `systems_batch1.rs`:
    - `SYSTEMS` rows: all `Family::Console`, `false`.
    - `TARGETS`: `("esde","jaguar","atarijaguar",None)`, `("onion","colecovision","COLECO",None)`, `("minui","colecovision","COLECO",None)`, `("onion","channelf","FAIRCHILD",None)`, `("onion","odyssey2","ODYSSEY",None)`, `("batocera","odyssey2","odyssey2",None)`, `("muos","scv","scv",None)`, `("onion","arcadia","ARCADIA",Some(false))`.
    - `HEADERS`: the vectrex, coleco and jaguar heads.
    - A test that the Knulli alias `o2em` and the ES-DE `videopac` resolve to `odyssey2` without a profile.
- [ ] **Step 2: Run** `cargo test -p coverly-core --test sniff --test organize_evidence --test systems_batch1` and expect failures.
- [ ] **Step 3: Implement.**
  - TOML and the folder rows above.
  - Sniff rules in `cartridges()`, before the generic fallbacks and after the existing Sega/Nintendo checks:
    - `starts(head, 0, b"g GCE")` → `s.hit("vectrex", 0.95, "g GCE")`.
    - `head[..2] == [0xAA, 0x55] || [0x55, 0xAA]` with `ext` in `col|rom|bin` → `s.hit("colecovision", 0.60, "AA55")`.
    - `ext == "abs" && head[..2] == [0x60, 0x1B]` → jaguar 0.50 `DRI ABS`; `ext == "cof" && head[..2] == [0x01, 0x50]` → jaguar 0.50 `COFF`.
  - `DECISIVE` gains `"vectrex"`.
  - Verify the Vectrex magic against the libretro `vecx` core source (`cart` loader) and record the file and line in the commit body.
  - Onion `extlist` check: compare `COLECO`, `INTELLIVISION`, `VECTREX`, `ODYSSEY`, `FAIRCHILD` and `SUPERVISION` (the last in Task 7) package `config.json` `extlist` at Onion `07505ea5` with the TOML. Put every mismatch in the commit body, under «Onion extlist».
- [ ] **Step 4: Run** the Step 2 tests, the snapshot (expect only new extensions in the diff, and `bin`/`rom` stay Generic), then G-RUST.
- [ ] **Step 5: Commit and push:** `git add -A crates && git commit -m "feat(core): add ColecoVision, Intellivision, Vectrex, Odyssey2, Channel F, Arcadia, SCV and Jaguar" && git push && git status -sb`

---

## Task 6: Systems: Sega, NEC and Nintendo add-ons (SG-1000, Pico, SuperGrafx, 64DD, Satellaview, Sufami Turbo) + Famicom check

**Files:** `systems/systems.toml` (also `pcengine`: `sgx` moves from `extensions` to `weak_extensions`, and `pce`… stays; `snes`: `bs` moves to `weak_extensions`), `profiles/builtin.rs` (rows + `ANBERNIC_HOSTS` gains `("supergrafx","pcengine")`, `("satellaview","snes")`), `identify/sniff.rs` (Pico and Sufami hits; `DECISIVE` += `pico`, `sufami`), tests.

**TOML rows:**

```toml
[[system]]
id = "sg1000"
name = "Sega SG-1000 / SC-3000"
family = "console"
aliases = ["SEGASGONE", "sg1000", "sg-1000", "sc3000"]
extensions = ["sg", "sc", "bin", "zip", "7z"]
libretro = "Sega - SG-1000"
screenscraper = 109

[[system]]
id = "pico"
name = "Sega Pico"
family = "console"
aliases = ["SEGAPICO"]
extensions = ["bin", "zip", "7z"]
weak_extensions = ["md"]
libretro = "Sega - PICO"
screenscraper = 250

[[system]]
id = "supergrafx"
name = "PC Engine SuperGrafx"
family = "console"
aliases = ["SGFX", "supergrafx"]
extensions = ["sgx", "zip", "7z"]
weak_extensions = ["pce"]
libretro = "NEC - PC Engine SuperGrafx"
screenscraper = 105

[[system]]
id = "n64dd"
name = "Nintendo 64DD"
family = "console"
aliases = ["N64DD", "64dd"]
extensions = ["ndd", "zip", "7z"]
libretro = "Nintendo - Nintendo 64DD"
screenscraper = 122

[[system]]
id = "satellaview"
name = "Nintendo Satellaview"
family = "console"
aliases = ["SATELLAVIEW", "bsx", "bs-x"]
extensions = ["bs", "zip", "7z"]
weak_extensions = ["sfc", "smc"]
libretro = "Nintendo - Satellaview"
screenscraper = 107

[[system]]
id = "sufami"
name = "Bandai Sufami Turbo"
family = "console"
aliases = ["SUFAMI", "sufamiturbo"]
extensions = ["zip", "7z"]
weak_extensions = ["st", "sfc", "smc"]
libretro = "Nintendo - Sufami Turbo"
screenscraper = 108
```

**Folder rows:**

```rust
    ("sg1000", [Some("sg-1000"), Some("sg1000"), Some("sg1000"), Some("SEGASGONE"), Some("SG1000")]),
    ("pico", [None, Some("pico"), Some("segapico"), None, None]),
    ("supergrafx", [Some("supergrafx"), Some("supergrafx"), Some("supergrafx"), Some("SGFX"), None]),
    ("n64dd", [Some("n64dd"), Some("n64dd"), Some("n64dd"), None, None]),
    ("satellaview", [Some("satellaview"), Some("satellaview"), Some("satellaview"), Some("SATELLAVIEW"), None]),
    ("sufami", [Some("sufami"), Some("sufami"), Some("sufami"), Some("SUFAMI"), None]),
```

- [ ] **Step 0: Sync.**
- [ ] **Step 1: Failing tests.**
  - `tests/sniff.rs:341`: `pico.unsupported` becomes `assert!(pico.hits.iter().any(|h| h.system == "pico" && h.weight >= 0.95))`.
  - New `sufami_magic`: `BANDAI SFC-ADX` at 0 → `sufami` 0.95; `SFC-ADX BACKUP` (the BIOS) → no hit.
  - `tests/organize_evidence.rs`:
    - `pico_md_beats_megadrive` (D24): `.md` + `SEGA PICO` → pico Auto, with megadrive at 0.60 `md!`.
    - `sufami_st_beats_atarist`: `.st` + the magic → sufami Auto.
    - `plain_st_stays_atarist`.
    - `sgx_now_supergrafx`: a loose `.sgx` → supergrafx, unique 0.85.
  - `tests/profile_folders.rs`:
    - `anbernic_hosts_keep_old_destinations`: on anbernic-stock `supergrafx` → `PCE` with `host == Some("pcengine")` and `on_device == Some(true)`; `satellaview` → `SFC`.
    - `pico_is_sega_pico_only_on_batocera`: `resolve_folder_in(Some(batocera), "pico")` → `pico`; `resolve_folder_in(Some(onion), "PICO")` → `pico8`; `resolve_folder("pico")` → `pico8`. A custom Batocera clone with `folder_names.pico = "pico"` passes `validate_profile`.
  - `systems_batch1.rs` rows:
    - `SYSTEMS`: all Console.
    - `TARGETS`: `("esde","sg1000","sg-1000",None)`, `("onion","sg1000","SEGASGONE",None)`, `("minui","sg1000","SG1000",None)`, `("batocera","pico","pico",None)`, `("muos","pico","segapico",None)`, `("esde","pico","SEGAPICO",Some(false))`.

      ES-DE uses `Id`, and `pico` (the id) under ES-DE resolves to pico8 (global alias), so the naming rule falls back to the first alias `SEGAPICO`. Check this against `default_name` and fix the expectation if the code gives otherwise: the rule is the spec's (D6), not the test's.
    - `("onion","supergrafx","SGFX",None)`, `("anbernic-stock","supergrafx","PCE",Some(true))`.
  - **Famicom check (D17):** `famicom_and_fc_stay_nes`: `resolve_folder("famicom")` and `resolve_folder("FC")` → `nes`; the BS-X alias `bsx` → `satellaview`. No new entry.
- [ ] **Step 2: Run** `cargo test -p coverly-core --test sniff --test organize_evidence --test profile_folders --test systems_batch1` and expect failures.
- [ ] **Step 3: Implement.**
  - TOML and rows. Move `sgx` and `bs` as stated.
  - `sega()`: the Pico branch becomes `s.hit("pico", 0.95, "SEGA PICO")` (keep it before the Mega Drive rule).
  - Sufami: `if starts(head, 0, b"BANDAI SFC-ADX") && !starts(head, 0x10, b"SFC-ADX BACKUP")` → `s.hit("sufami", 0.95, "BANDAI SFC-ADX")`, checked before the SNES map test. Verify against Snes9x `memmap.cpp` (`is_SufamiTurbo_Cart`) and cite it in the commit body.
  - `DECISIVE` += `pico`, `sufami`.
  - Hosts for `supergrafx`/`satellaview` only on anbernic-stock (D23). For minui, check `LoveRetro/NextUI` at `5291344b` for an `SGFX`/`SATELLAVIEW` pak. If none exists, nothing changes and the system stays Absent. Write the result in the commit body.
  - Onion `extlist`: compare `SEGASGONE`, `SGFX`, `SATELLAVIEW`, `SUFAMI` and list the mismatches. `SUFAMI` lacks `.st`; it is a known owner ask (§13).
- [ ] **Step 4: Run** the Step 2 tests and the snapshot. The diff must show `sgx` Unique → owner `supergrafx` and `bs` Unique → owner `satellaview`; both still `accepting` the old system. Then G-RUST.
- [ ] **Step 5: Commit and push:** `git add -A crates && git commit -m "feat(core): add SG-1000, Pico, SuperGrafx, 64DD, Satellaview and Sufami Turbo" && git push && git status -sb`

---

## Task 7: Systems: handhelds (Supervision, Game.com, Mega Duck)

**Files:** `systems/systems.toml`, `profiles/builtin.rs`, `tests/systems_batch1.rs`, `tests/organize_evidence.rs`

**TOML rows:**

```toml
[[system]]
id = "supervision"
name = "Watara Supervision"
family = "handheld"
aliases = ["SUPERVISION"]
extensions = ["sv", "bin", "zip", "7z"]
libretro = "Watara - Supervision"
screenscraper = 207

[[system]]
id = "gamecom"
name = "Tiger Game.com"
family = "handheld"
aliases = ["GAMECOM"]
extensions = ["tgc", "bin", "zip", "7z"]
libretro = "Tiger - Game.com"
screenscraper = 121

[[system]]
id = "megaduck"
name = "Mega Duck / Cougar Boy"
family = "handheld"
aliases = ["MEGADUCK", "cougarboy"]
extensions = ["bin", "zip", "7z"]
screenscraper = 90
```

**Folder rows:**

```rust
    ("supervision", [Some("supervision"), Some("supervision"), Some("supervision"), Some("SUPERVISION"), None]),
    ("gamecom", [Some("gamecom"), Some("gamecom"), None, None, None]),
    ("megaduck", [Some("megaduck"), Some("megaduck"), Some("megaduck"), Some("MEGADUCK"), None]),
```

- [ ] **Step 0: Sync.**
- [ ] **Step 1: Failing tests.**
  - `systems_batch1.rs`:
    - `SYSTEMS`: all `Family::Handheld`.
    - `TARGETS`: `("onion","megaduck","MEGADUCK",None)`, `("onion","supervision","SUPERVISION",None)`, `("muos","gamecom","gamecom",Some(false))`, `("esde","gamecom","gamecom",None)`.
  - `organize_evidence.rs` `mega_duck_bin_needs_its_folder`: a loose `.bin` → nothing; inside `MEGADUCK/` (already sorted, so it is not walked) the scan lists it under megaduck (`scan_fixtures` style test).
  - Mega Duck has no libretro folder and no DAT: assert that `libretro` and `dat_sources` are empty and that `screenscraper == Some(90)` (covers come from ScreenScraper only, §11 step 6).
- [ ] **Step 2: Run** `cargo test -p coverly-core --test systems_batch1 --test organize_evidence --test scan_fixtures` and expect failures.
- [ ] **Step 3: Implement** the TOML and rows. There are no headers (§8.1). Onion `extlist` check for `SUPERVISION` and `MEGADUCK`.
- [ ] **Step 4: Run** the tests and the snapshot (`tgc`, `sv` are new and unique), then G-RUST.
- [ ] **Step 5: Commit and push:** `git add -A crates && git commit -m "feat(core): add Watara Supervision, Game.com and Mega Duck" && git push && git status -sb`

---

## Task 8: Systems: computers I (ZX Spectrum, ZX81, Amstrad CPC, BBC Micro, Thomson, Dragon, CoCo, TI-99/4A)

**Files:** `systems/systems.toml`, `profiles/builtin.rs`, `identify/sniff.rs` (TZX, CPC DSK, UEF), `organize/inspect.rs` (RPK `layout.xml` probe through the archive layer), `tests/fixtures/dat/` (trimmed TOSEC and `dat/` files), tests.

**TOML rows:**

```toml
[[system]]
id = "zxspectrum"
name = "Sinclair ZX Spectrum"
family = "computer"
aliases = ["ZXS", "zxspectrum", "spectrum"]
extensions = ["tzx", "z80", "rzx", "scl", "trd", "szx", "tap", "dsk", "sna", "zip", "7z"]
libretro = "Sinclair - ZX Spectrum"
dats = ["metadat/tosec/Sinclair - ZX Spectrum.dat", "metadat/no-intro/Sinclair - ZX Spectrum +3.dat", "dat/Sinclair - ZX Spectrum.dat"]
screenscraper = 76

[[system]]
id = "zx81"
name = "Sinclair ZX81"
family = "computer"
aliases = ["ZX81"]
extensions = ["81", "zip", "7z"]
weak_extensions = ["p", "tzx"]
libretro = "Sinclair - ZX 81"
dats = ["dat/Sinclair - ZX 81.dat"]
screenscraper = 77

[[system]]
id = "amstradcpc"
name = "Amstrad CPC"
family = "computer"
aliases = ["CPC", "amstradcpc"]
extensions = ["cdt", "cpr", "kcr", "dsk", "sna", "m3u", "zip", "7z"]
libretro = "Amstrad - CPC"
dats = ["metadat/tosec/Amstrad - CPC.dat", "dat/Amstrad - CPC.dat"]
screenscraper = 65

[[system]]
id = "bbcmicro"
name = "Acorn BBC Micro"
family = "computer"
aliases = ["BBCMICRO", "bbc"]
scoped_aliases = ["bbc"]
extensions = ["ssd", "dsd", "uef", "zip", "7z"]
screenscraper = 37

[[system]]
id = "thomson"
name = "Thomson MO/TO"
family = "computer"
aliases = ["THOMSON", "moto", "to8", "thomsonmoto", "theodore"]
scoped_aliases = ["moto", "theodore"]
extensions = ["k7", "m7", "sap", "rom", "zip", "7z"]
weak_extensions = ["fd"]
libretro = "Thomson - MOTO"
dats = ["metadat/tosec/Thomson - MOTO.dat"]
screenscraper = 141

[[system]]
id = "dragon"
name = "Dragon 32/64"
family = "computer"
aliases = ["DRAGON", "dragon32", "dragon64"]
scoped_aliases = ["dragon", "dragon32", "dragon64"]
extensions = ["cas", "dsk", "ccc", "zip", "7z"]
screenscraper = 91

[[system]]
id = "coco"
name = "Tandy TRS-80 Color Computer"
family = "computer"
aliases = ["COCO", "trs80coco"]
scoped_aliases = ["coco"]
extensions = ["cas", "dsk", "ccc", "zip", "7z"]
screenscraper = 144

[[system]]
id = "ti99"
name = "Texas Instruments TI-99/4A"
family = "computer"
aliases = ["TI99", "ti994a"]
extensions = ["rpk", "ctg", "bin", "zip", "7z"]
screenscraper = 205
```

**Folder rows:**

```rust
    ("zxspectrum", [Some("zxspectrum"), Some("zxspectrum"), Some("spectrum"), Some("ZXS"), None]),
    ("zx81", [Some("zx81"), Some("zx81"), Some("zx81"), None, None]),
    ("amstradcpc", [Some("amstradcpc"), Some("amstradcpc"), Some("amstradcpc"), Some("CPC"), Some("CPC")]),
    ("bbcmicro", [Some("bbcmicro"), Some("bbcmicro"), None, None, None]),
    ("thomson", [Some("moto"), Some("thomson"), Some("thomson"), None, None]),
    ("dragon", [Some("dragon32"), Some("dragon64"), Some("dragon"), None, None]),
    ("coco", [Some("coco"), Some("coco"), Some("coco"), None, None]),
    ("ti99", [Some("ti99"), Some("ti99"), None, None, None]),
```

- [ ] **Step 0: Sync.**
- [ ] **Step 1: Failing tests.**
  - `tests/sniff.rs`:
    - `tzx_magic`: `ZXTape!\x1A` with `.tzx` → zxspectrum 0.90; the same magic with `.cdt` → amstradcpc 0.90.
    - `cpc_dsk_magic_is_a_review`: `MV - CPC` / `EXTENDED CPC DSK File` with `.dsk` → amstradcpc 0.50 **and** zxspectrum 0.50.
    - `uef_magic`: `UEF File!` → bbcmicro 0.95. A gzip `.uef` is a package (D25): its inner head is read through the archive layer.
  - `tests/organize_evidence.rs`:
    - `loose_dsk_goes_to_review`: the CPC magic lists Amstrad CPC first, then ZX Spectrum. Without it, every `.dsk` owner is a candidate and nothing is Auto.
    - `loose_tap_stays_review`: `.tap` is already Shared (c64, vic20) and still is.
  - `tests/organize_inspect.rs` `rpk_with_layout_xml_is_ti99`: a zip named `.rpk` holding `layout.xml` → one raw item with `ti99` 0.90.
  - `tests/dat.rs` `parses_trimmed_tosec_and_dat_folder`: parse `tests/fixtures/dat/tosec-sample.dat` and `dat-sample.dat` (20 lines each, copied from the pinned files' **structure** with invented game names and CRCs). Rows and CRCs come out.
  - `tests/profile_folders.rs` `scoped_aliases_need_a_profile`: `resolve_folder("Dragon")`, `("PET")`, `("coco")`, `("moto")` → `None` (PET lands in Task 10; add it to this test then). With `Some(batocera)`: `"dragon64"` → dragon. `profiles::detect` on a temp dir holding `Dragon/` and `coco/` does not count them (use `detect` with a tempdir as `tests/profile_fit.rs` does).
  - `tests/organize_walk.rs` `dragon_folder_in_downloads_is_still_walked`: with no profile match, a pile folder `Dragon/` holding `game.cas` is walked; with the target profile set to batocera, `dragon64/` is «already sorted».
  - `systems_batch1.rs`:
    - `SYSTEMS`: all `Family::Computer`.
    - `TARGETS`: `("muos","zxspectrum","spectrum",None)`, `("onion","zxspectrum","ZXS",None)`, `("onion","amstradcpc","CPC",None)`, `("minui","amstradcpc","CPC",None)`, `("esde","thomson","moto",None)`, `("esde","dragon","dragon32",None)`, `("batocera","dragon","dragon64",None)`, `("muos","bbcmicro","bbcmicro",Some(false))`.
    - `HEADERS` for TZX, CPC DSK and UEF.
- [ ] **Step 2: Run** `cargo test -p coverly-core --test sniff --test organize_evidence --test organize_inspect --test dat --test profile_folders --test organize_walk --test systems_batch1` and expect failures.
- [ ] **Step 3: Implement.**
  - TOML and rows.
  - Sniff rules in `cartridges()`:
    - `ZXTape!\x1A` → `zxspectrum` or `amstradcpc` by ext (`cdt` → CPC), 0.90.
    - `MV - CPC` / `EXTENDED CPC DSK File` → both at 0.50, detail `CPC DSK`.
    - `UEF File!` → bbcmicro 0.95.
  - The RPK / UEF probes go in `inspect.rs` for `PACKAGE_EXTS` items: open with `archive::open_archive` on the item's bytes, list names (RPK: `layout.xml` → `ti99` 0.90) or read the decompressed head (gzip UEF → `UEF File!`). Bounded by the existing budgets, with nothing written.
  - `.dsk` becomes Shared (D3) automatically; the snapshot diff shows it.
  - Onion `extlist` check for `ZXS` and `CPC`.
- [ ] **Step 4: Run** the tests. Snapshot diff: `dsk` Unique (msx) → Shared (msx, zxspectrum, amstradcpc, dragon, coco), and `sna`, `ccc` new Shared. Review it, then G-RUST.
- [ ] **Step 5: Commit and push:** `git add -A crates && git commit -m "feat(core): add ZX Spectrum, ZX81, Amstrad CPC, BBC Micro, Thomson, Dragon, CoCo and TI-99/4A" && git push && git status -sb`

---

## Task 9: Systems: computers II (X68000, X1, PC-98, PC-88) + `.hdf`/`.fdi`/`.dim` headers (P-2)

**Files:** `systems/systems.toml`, `profiles/builtin.rs`, `identify/sniff.rs` (T98 NHD, Amiga RDB/DOS, X68000 SCSI, FDI 2.0, FastCopy DIM), tests.

**TOML rows:**

```toml
[[system]]
id = "x68000"
name = "Sharp X68000"
family = "computer"
aliases = ["X68000", "x68k"]
extensions = ["xdf", "dim", "hdf", "hdm", "2hd", "m3u", "zip", "7z"]
libretro = "Sharp - X68000"
dats = ["metadat/no-intro/Sharp - X68000.dat", "metadat/tosec/Sharp - X68000.dat"]
screenscraper = 79

[[system]]
id = "x1"
name = "Sharp X1"
family = "computer"
aliases = ["X1"]
scoped_aliases = ["x1"]
extensions = ["dx1", "2hd", "tap", "d88", "zip", "7z"]
weak_extensions = ["2d"]
libretro = "Sharp - X1"
dats = ["metadat/no-intro/Sharp - X1.dat", "metadat/tosec/Sharp - X1.dat"]
screenscraper = 220

[[system]]
id = "pc98"
name = "NEC PC-98"
family = "computer"
aliases = ["PC98", "pc9800"]
extensions = ["hdi", "nhd", "d88", "fdi", "hdm", "m3u", "zip", "7z"]
libretro = "NEC - PC-98"
dats = ["metadat/tosec/NEC - PC-98.dat", "metadat/redump/NEC - PC-98.dat"]
screenscraper = 208

[[system]]
id = "pc88"
name = "NEC PC-88"
family = "computer"
aliases = ["PC88", "pc8800", "pc8000"]
extensions = ["t88", "cmt", "d88", "zip", "7z"]
libretro = "NEC - PC-88"
dats = ["metadat/tosec/NEC - PC-88.dat"]
screenscraper = 221
```

**Folder rows:**

```rust
    ("x68000", [Some("x68000"), Some("x68000"), Some("x68000"), None, None]),
    ("x1", [Some("x1"), Some("x1"), Some("x1"), None, None]),
    ("pc98", [Some("pc98"), Some("pc98"), Some("pc98"), None, None]),
    ("pc88", [Some("pc88"), Some("pc88"), Some("pc8000"), None, None]),
```

- [ ] **Step 0: Sync.**
- [ ] **Step 1: Failing tests.**
  - `tests/sniff.rs`:
    - `nhd_magic`: `T98HDDIMAGE.R0` → pc98 0.95.
    - `hdf_amiga_or_x68000`: `RDSK` at 0 → amiga 0.90; `DOS\0` → amiga 0.90; `X68SCSI1` → x68000 0.90.
    - `fdi_amiga`: `Formatted Disk Image file` → amiga 0.90.
    - `dim_atari_st`: `42 42` at 0 → atarist 0.85.
  - `tests/organize_evidence.rs` `loose_amiga_and_st_disks_stay_automatic`:
    - `Game.hdf` with `RDSK` → amiga Auto;
    - `Game.fdi` with the FDI 2.0 text → amiga Auto;
    - `Game.dim` with `42 42` → atarist Auto;
    - the same files without a header → review, listing amiga/x68000, amiga/pc98 and atarist/x68000.
  - `systems_batch1.rs`:
    - `SYSTEMS`: Computer.
    - `TARGETS`: `("muos","pc88","pc8000",None)`, `("esde","x68000","x68000",None)`, `("onion","x68000","X68000",Some(false))`.
    - `HEADERS` for the rules above.
  - `profile_folders.rs`: the scoped `X1` folder → `resolve_folder("x1")` is `None`, and `resolve_folder_in(Some(esde), "x1")` is `x1`.
- [ ] **Step 2: Run** `cargo test -p coverly-core --test sniff --test organize_evidence --test systems_batch1 --test profile_folders` and expect failures.
- [ ] **Step 3: Implement.**
  - TOML and rows, plus the sniff rules (in `cartridges()`, keyed by ext, so a rule never fires on another extension).
  - Verify each «verify» rule against its cited spec or emulator (Amiga RDB: `RDSK` block id; X68000: `X68SCSI1` in px68k/MAME `x68k_hdd`; FDI 2.0: Disk2FDI documentation; FastCopy DIM: Hatari `dim.c`). A rule that cannot be confirmed is dropped, and the commit body records `BLOCKED: … unverified`. Without it, those files go to review, which is the safe side.
- [ ] **Step 4: Run** the tests. Snapshot diff: `hdf`, `fdi`, `dim` → Shared; `d88`, `2hd`, `hdm` new Shared. Review it, then G-RUST.
- [ ] **Step 5: Commit and push:** `git add -A crates && git commit -m "feat(core): add X68000, X1, PC-98 and PC-88; keep Amiga and ST disks automatic by header" && git push && git status -sb`

---

## Task 10: Systems: computers III (Plus/4, PET, C128, Atari XE, MSX2, MSX turbo R, Apple II)

**Files:** `systems/systems.toml` (also `msx`: remove alias `msx2`), `profiles/builtin.rs` (rows; hosts: anbernic-stock `("msx2","msx")`, `("atarixe","a800")`; garlicos and onion `("msx2","msx")`; minui only if a NextUI pak exists, checked here), `identify/sniff.rs` (WOZ, 2IMG, PRG load address tie-break), tests.

**TOML rows** (`atarixe` and every row here come after `a800`/`msx` because they are appended at the end of the file. That keeps `Atari - 8-bit Family.dat` mapping to `a800`, spec §8.3):

```toml
[[system]]
id = "plus4"
name = "Commodore Plus/4"
family = "computer"
aliases = ["PLUS4", "cplus4", "commodoreplus4"]
extensions = ["zip", "7z"]
weak_extensions = ["prg", "d64", "t64", "tap", "crt"]
libretro = "Commodore - Plus-4"
screenscraper = 99

[[system]]
id = "pet"
name = "Commodore PET"
family = "computer"
aliases = ["PET", "commodorepet"]
scoped_aliases = ["pet"]
extensions = ["zip", "7z"]
weak_extensions = ["prg", "d64", "t64", "tap"]
libretro = "Commodore - PET"
dats = ["metadat/tosec/Commodore - PET.dat"]

[[system]]
id = "c128"
name = "Commodore 128"
family = "computer"
aliases = ["C128", "commodore128"]
extensions = ["zip", "7z"]
weak_extensions = ["prg", "d64", "d71", "d81", "t64", "tap", "crt"]

[[system]]
id = "atarixe"
name = "Atari XE / XEGS"
family = "computer"
aliases = ["ATARIXE", "xegs"]
extensions = ["zip", "7z"]
weak_extensions = ["atr", "xex", "xfd", "car", "rom", "bin"]
libretro = "Atari - 8-bit"
dats = ["metadat/no-intro/Atari - 8-bit Family.dat"]
screenscraper = 43

[[system]]
id = "msx2"
name = "MSX2"
family = "computer"
aliases = ["MSX2"]
extensions = ["rom", "dsk", "cas", "zip", "7z"]
weak_extensions = ["mx2"]
libretro = "Microsoft - MSX2"
screenscraper = 116

[[system]]
id = "msxturbor"
name = "MSX turbo R"
family = "computer"
aliases = ["MSXTURBOR", "turbor"]
scoped_aliases = ["turbor"]
extensions = ["rom", "dsk", "zip", "7z"]
screenscraper = 118

[[system]]
id = "apple2"
name = "Apple II"
family = "computer"
aliases = ["APPLE2", "appleii"]
extensions = ["woz", "2mg", "nib", "dsk", "zip", "7z"]
weak_extensions = ["do", "po"]
dats = ["metadat/tosec/Apple - II.dat"]
screenscraper = 86
```

**Folder rows:**

```rust
    ("plus4", [Some("plus4"), Some("cplus4"), Some("plus4"), None, Some("PLUS4")]),
    ("pet", [None, Some("pet"), Some("pet"), None, Some("PET")]),
    ("c128", [None, Some("c128"), Some("c128"), None, Some("C128")]),
    ("atarixe", [Some("atarixe"), Some("xegs"), None, None, None]),
    ("msx2", [Some("msx2"), Some("msx2"), Some("msx2"), None, None]),
    ("msxturbor", [Some("msxturbor"), Some("msxturbor"), None, None, None]),
    ("apple2", [Some("apple2"), Some("apple2"), None, None, None]),
```

- [ ] **Step 0: Sync.**
- [ ] **Step 1: Failing tests.**
  - `tests/sniff.rs`:
    - `woz_and_2img`: `WOZ2`/`WOZ1`/`2IMG` → apple2 0.95.
    - `prg_load_address_is_a_tiebreak_only`: `01 10` → plus4 0.40, `01 1C` → c128 0.40, `01 04` → pet 0.40, `01 08` → c64 0.40, all under the 0.50 review line.
  - `tests/systems_batch1.rs` `msx1_still_msx_and_msx2_moves`: `resolve_folder("msx1")` → msx, `("MSX2")` → msx2, `("MSX")` → msx.
  - `profile_folders.rs`:
    - anbernic `msx2` → `MSX` (host msx) and `atarixe` → `A800`;
    - onion `msx2` → `MSX`;
    - batocera `atarixe` → `xegs`;
    - esde `atarixe` → `atarixe`;
    - batocera `plus4` → `cplus4`;
    - minui `plus4` → `PLUS4`, `pet` → `PET`, `c128` → `C128`;
    - `resolve_folder("PET")` → `None` (scoped).
  - `tests/dat.rs` `a800_keeps_its_dat`: `system_for_dat("metadat/no-intro/Atari - 8-bit Family.dat")` → `a800`, and a test asserts `atarixe` is never chosen by hash.
  - `systems_batch1.rs`:
    - `SYSTEMS`: Computer.
    - `TARGETS`: the profile-folder cases above that are not hosts.
    - `HEADERS` for WOZ and 2IMG.
- [ ] **Step 2: Run** `cargo test -p coverly-core --test sniff --test systems_batch1 --test profile_folders --test dat` and expect failures.
- [ ] **Step 3: Implement** the TOML, the rows, the alias move, the hosts and the sniff rules. The PRG tie-break runs only for `ext == "prg"`, and its weight stays under the auto line. Verify the WOZ and 2IMG magics against the Applesauce WOZ spec and the 2IMG spec, and cite them. Onion has no folder for any of these; the `MSX` package `extlist` is compared for the `msx2` host.
- [ ] **Step 4: Run** the tests and the snapshot (expect no class change for `prg`/`d64`/`t64`/`tap`/`crt`: weak only; `dsk` gains msx2, msxturbor, apple2; `mx2` accepting gains msx2). Also run `cargo test -p coverly-roms` for the `list_systems` snapshot in `src-tauri/src/lib.rs` and review its diff (new names only). Then G-RUST.
- [ ] **Step 5: Commit and push:** `git add -A crates src-tauri && git commit -m "feat(core): add Plus/4, PET, C128, Atari XE, MSX2, MSX turbo R and Apple II" && git push && git status -sb`

---

## Task 11: Systems: fantasy consoles (TIC-80, WASM-4, Arduboy, Uzebox, LowRes NX, Vircon32)

**Files:** `systems/systems.toml`, `profiles/builtin.rs`, `identify/sniff.rs` (`UZEBOX`, `V32-CART`; `DECISIVE` += `uzebox`, `vircon32`), `organize/inspect.rs` (Arduboy `info.json` probe), tests.

**TOML rows:**

```toml
[[system]]
id = "tic80"
name = "TIC-80"
family = "engine"
aliases = ["TIC", "tic80"]
extensions = ["tic"]
libretro = "TIC-80"
dats = ["dat/TIC-80.dat"]
screenscraper = 222

[[system]]
id = "wasm4"
name = "WASM-4"
family = "engine"
aliases = ["WASM4"]
weak_extensions = ["wasm"]
libretro = "WASM-4"
dats = ["dat/WASM-4.dat"]
screenscraper = 262

[[system]]
id = "arduboy"
name = "Arduboy"
family = "handheld"
aliases = ["ARDUBOY"]
extensions = ["arduboy", "zip", "7z"]
weak_extensions = ["hex"]
libretro = "Arduboy Inc - Arduboy"
screenscraper = 263

[[system]]
id = "uzebox"
name = "Uzebox"
family = "console"
aliases = ["UZEBOX"]
extensions = ["uze", "zip", "7z"]
weak_extensions = ["hex"]
dats = ["dat/Uzebox.dat"]
screenscraper = 216

[[system]]
id = "lowresnx"
name = "LowRes NX"
family = "engine"
aliases = ["LOWRESNX"]
extensions = ["nx"]
libretro = "LowRes NX"
dats = ["dat/LowRes NX.dat"]
screenscraper = 244

[[system]]
id = "vircon32"
name = "Vircon32"
family = "engine"
aliases = ["VIRCON32"]
extensions = ["v32"]
libretro = "Vircon32"
dats = ["dat/Vircon32.dat"]
screenscraper = 272
```

**Folder rows:**

```rust
    ("tic80", [Some("tic80"), Some("tic80"), Some("tic80"), Some("TIC"), None]),
    ("wasm4", [Some("wasm4"), Some("wasm4"), Some("wasm4"), None, None]),
    ("arduboy", [Some("arduboy"), Some("arduboy"), Some("arduboy"), None, None]),
    ("uzebox", [Some("uzebox"), Some("uzebox"), Some("uzebox"), None, None]),
    ("lowresnx", [Some("lowresnx"), Some("lowresnx"), Some("lowresnx"), None, None]),
    ("vircon32", [Some("vircon32"), Some("vircon32"), Some("vircon32"), None, None]),
```

- [ ] **Step 0: Sync.**
- [ ] **Step 1: Failing tests.**
  - `tests/sniff.rs`:
    - `uzebox_header`: `UZEBOX` → uzebox 0.95.
    - `vircon32_header`: `V32-CART` → vircon32 0.95.
    - `wasm_magic_proves_nothing`: `\0asm` with `.wasm` → no hit.
  - `tests/organize_evidence.rs` `loose_wasm_hex_lua_are_unknown`: `game.wasm`, `firmware.hex` and `main.lua` on a desktop pile → left unknown. Inside `WASM4/` the scan lists `game.wasm` under wasm4.
  - `tests/organize_inspect.rs` `arduboy_package_is_raw`: an `.arduboy` zip with `info.json` → one raw item, arduboy 0.90.
  - `systems_batch1.rs`:
    - `SYSTEMS`: `tic80`, `wasm4`, `lowresnx`, `vircon32` → `Family::Engine`; `arduboy` → `Handheld`; `uzebox` → `Console`.
    - `TARGETS`: `("onion","tic80","TIC",None)`, `("esde","wasm4","wasm4",None)`, `("minui","tic80","TIC",Some(false))`.
- [ ] **Step 2: Run** `cargo test -p coverly-core --test sniff --test organize_evidence --test organize_inspect --test systems_batch1` and expect failures.
- [ ] **Step 3: Implement** the TOML, the rows, the sniff rules (verify `UZEBOX` against the Uzebox `uze` header doc and `V32-CART` against the Vircon32 cartridge spec, and cite both) and the Arduboy probe. Onion `TIC` `extlist` lists Thomson extensions (spec §6.1 note): record it as found, do not fix it.
- [ ] **Step 4: Run** the tests, the snapshot (`hex` is weak in two systems and must not appear as a class) and G-RUST.
- [ ] **Step 5: Commit and push:** `git add -A crates && git commit -m "feat(core): add TIC-80, WASM-4, Arduboy, Uzebox, LowRes NX and Vircon32" && git push && git status -sb`

---

## Task 12: Systems: ports and engines + the folder-game rule (D15)

**Files:** `systems/systems.toml`, `profiles/builtin.rs`, `identify/sniff.rs` (`IWAD`/`PWAD` → doom, `DECISIVE` += `doom`), `organize/walk.rs` (folder-game detection), `organize/plan.rs` (`LeftReason::FolderGame { system: String }`), `dedupe/units.rs` (folder-game dirs out of scope), `crates/coverly-cli/src/organize.rs` (render the new reason), tests.

**TOML rows:**

```toml
[[system]]
id = "doom"
name = "Doom (PrBoom)"
family = "engine"
aliases = ["PRBOOM", "doom"]
scoped_aliases = ["doom"]
extensions = ["wad"]
libretro = "DOOM"
dats = ["dat/DOOM.dat"]
screenscraper = 135

[[system]]
id = "quake"
name = "Quake"
family = "engine"
aliases = ["QUAKE", "tyrquake"]
scoped_aliases = ["quake"]
weak_extensions = ["pak"]
libretro = "Quake"
dats = ["dat/Quake.dat"]
screenscraper = 135

[[system]]
id = "cavestory"
name = "Cave Story (NXEngine)"
family = "engine"
aliases = ["CAVESTORY", "nxengine"]
libretro = "Cave Story"
dats = ["dat/Cave Story.dat"]

[[system]]
id = "wolf3d"
name = "Wolfenstein 3D (ECWolf)"
family = "engine"
aliases = ["ECWOLF", "wolf3d", "wolfenstein3d"]
libretro = "Wolfenstein 3D"
dats = ["dat/Wolfenstein 3D.dat"]

[[system]]
id = "mrboom"
name = "Mr. Boom"
family = "engine"
aliases = ["MRBOOM"]
scoped_aliases = ["mrboom"]
libretro = "MrBoom"
dats = ["dat/MrBoom.dat"]

[[system]]
id = "dinothawr"
name = "Dinothawr"
family = "engine"
aliases = ["DINOTHAWR"]
weak_extensions = ["game"]
libretro = "Dinothawr"
dats = ["dat/Dinothawr.dat"]

[[system]]
id = "lutro"
name = "Lutro"
family = "engine"
aliases = ["LUTRO"]
scoped_aliases = ["lutro"]
extensions = ["lutro"]
weak_extensions = ["lua"]
libretro = "Lutro"
dats = ["dat/Lutro.dat"]
screenscraper = 206

[[system]]
id = "solarus"
name = "Solarus"
family = "engine"
aliases = ["SOLARUS"]
scoped_aliases = ["solarus"]
extensions = ["solarus"]
screenscraper = 223

[[system]]
id = "flashback"
name = "Flashback (REminiscence)"
family = "engine"
aliases = ["REMINISCENCE", "flashback"]
scoped_aliases = ["flashback"]
libretro = "Flashback"
dats = ["dat/Flashback.dat"]

[[system]]
id = "xrick"
name = "Rick Dangerous (XRick)"
family = "engine"
aliases = ["XRICK", "rickdangerous"]
scoped_aliases = ["xrick"]
libretro = "Rick Dangerous"
dats = ["dat/Rick Dangerous.dat"]
```

**Folder rows:**

```rust
    ("doom", [Some("doom"), Some("prboom"), Some("doom"), None, Some("PRBOOM")]),
    ("quake", [Some("quake"), Some("quake"), Some("quake"), None, None]),
    ("cavestory", [None, Some("cavestory"), Some("cavestory"), None, None]),
    ("wolf3d", [None, Some("ecwolf"), Some("wolfenstein3d"), None, None]),
    ("mrboom", [None, Some("mrboom"), None, None, None]),
    ("dinothawr", [None, None, None, None, None]),
    ("lutro", [Some("lutro"), Some("lutro"), None, None, None]),
    ("solarus", [Some("solarus"), Some("solarus"), None, None, None]),
    ("flashback", [None, Some("reminiscence"), None, None, None]),
    ("xrick", [None, Some("xrick"), None, None, None]),
```

**Interfaces (produced):**
```rust
// organize/plan.rs, LeftReason
/// A game that is a folder (Doom WAD, Solarus, Lutro, ScummVM): left as
/// one item; move the whole folder to the system's folder (D15).
FolderGame { system: String },
// organize/walk.rs
/// Systems whose games are folders, and the proof a file gives.
pub const FOLDER_GAME_SYSTEMS: [&str; 4] = ["doom", "solarus", "lutro", "scummvm"];
```

- [ ] **Step 0: Sync.**
- [ ] **Step 1: Failing tests.**
  - `tests/sniff.rs`:
    - `wad_magic`: `IWAD`/`PWAD` → doom 0.95.
    - `pak_alone_is_not_quake`: `PACK` with `.pak` → no quake hit (OpenBOR uses the same magic).
  - `tests/organize_evidence.rs` `wad_without_magic_is_not_auto`: `.wad` without the magic → doom 0.60 (`wad!`), never Auto.
  - `tests/organize_walk.rs`:
    - `freedoom_folder_is_one_item`: `Freedoom Phase 1/freedoom1.wad` (IWAD) + `README.txt` → one item with `LeftReason::FolderGame { system: "doom" }`, and `README.txt` is not a separate item.
    - `cave_story_folder_files_are_unknown`: `Cave Story/Doukutsu.exe` + `data/` → the files are left unknown (no rule invented).
    - `scummvm_launcher_keeps_its_folder`: `Monkey/monkey.scummvm` + `MONKEY.000` → one `FolderGame { system: "scummvm" }`.
  - `tests/dedupe_units.rs` `folder_game_dirs_are_out_of_scope`: `ROMS/prboom/…` and `ROMS/scummvm/…` are skipped like `PORTS/`.
  - `systems_batch1.rs`:
    - `SYSTEMS`: Engine, `false`.
    - `TARGETS`: `("batocera","doom","prboom",None)`, `("minui","doom","PRBOOM",None)`, `("batocera","wolf3d","ecwolf",None)`, `("muos","wolf3d","wolfenstein3d",None)`, `("batocera","flashback","reminiscence",None)`, `("esde","dinothawr","dinothawr",Some(false))`.
  - `profile_folders.rs`: `resolve_folder("doom")`, `("quake")`, `("flashback")`, `("Solarus")` → `None`; `resolve_folder("prboom")` → doom (distinctive, global).
- [ ] **Step 2: Run** `cargo test -p coverly-core --test sniff --test organize_evidence --test organize_walk --test dedupe_units --test systems_batch1 --test profile_folders` and expect failures.
- [ ] **Step 3: Implement.**
  - TOML and rows.
  - `IWAD`/`PWAD` hit. `DECISIVE` += `doom`.
  - Walk: when a pile folder (not a system folder) **directly** holds a file proven for a `FOLDER_GAME_SYSTEMS` system, emit one item for the folder with `LeftReason::FolderGame` and skip its contents:
    - doom: `IWAD`/`PWAD` read from the first 4 bytes through the existing `MetaProbe`;
    - solarus: `.solarus` package;
    - lutro: `.lutro` package, or `main.lua` beside `conf.lua`;
    - scummvm: `.scummvm`/`.svm`.

    The folder's target (for the UI's `{folder}`) is `system_folder(profile, system, …)`.
  - Dedupe: extend the out-of-scope check with every alias of `FOLDER_GAME_SYSTEMS` (keep `OUT_OF_SCOPE_DIRS` as is and add a second check that reads the aliases from `SystemDb`).
  - CLI: render `FolderGame` as `"{system} game in its own folder: move the whole folder to {folder}"` through the existing reason renderer (CLI English only, like the other reasons).
  - Serde: `LeftReason` is `kebab-case` tagged, so the variant serialises as `folder-game`. Add it to `src/ipc/types.ts` in Task 16.
- [ ] **Step 4: Run** the tests, the snapshot and G-RUST. `src-tauri` must build: add the variant to any exhaustive `match` there.
- [ ] **Step 5: Commit and push:** `git add -A crates src-tauri && git commit -m "feat(core): add port and engine systems and keep folder games whole" && git push && git status -sb`

---

## Task 13: Systems: extra arcade (Model 2, Model 3, NAOMI 2, Triforce) + MAME CHD companions

**Files:** `systems/systems.toml` (the rows are appended after `mame`, by file order, spec §8.3), `profiles/builtin.rs`, `organize/inspect.rs` or `organize/plan.rs` (CHD companion grouping, wherever companions are grouped today: find the call with `grep -n "companion" crates/coverly-core/src/organize/*.rs`), tests.

**TOML rows:**

```toml
[[system]]
id = "model2"
name = "Sega Model 2"
family = "arcade"
aliases = ["MODEL2"]
extensions = ["zip", "7z"]
libretro = "MAME"
arcade = true
dats = ["metadat/mame/MAME.dat"]
screenscraper = 75

[[system]]
id = "model3"
name = "Sega Model 3"
family = "arcade"
aliases = ["MODEL3"]
extensions = ["zip", "7z"]
libretro = "MAME"
arcade = true
dats = ["metadat/mame/MAME.dat"]
screenscraper = 75

[[system]]
id = "naomi2"
name = "Sega NAOMI 2"
family = "arcade"
aliases = ["NAOMI2"]
extensions = ["zip", "7z", "chd"]
libretro = "Sega - Naomi 2"
arcade = true
dats = ["metadat/redump/Sega - Naomi 2.dat", "metadat/mame/MAME.dat"]

[[system]]
id = "triforce"
name = "Namco Sega Nintendo Triforce"
family = "arcade"
aliases = ["TRIFORCE"]
weak_extensions = ["iso"]
arcade = true
```

**Folder rows:**

```rust
    ("model2", [Some("model2"), Some("model2"), None, None, None]),
    ("model3", [Some("model3"), Some("model3"), None, None, None]),
    ("naomi2", [Some("naomi2"), Some("naomi2"), None, None, None]),
    ("triforce", [Some("triforce"), Some("triforce"), None, None, None]),
```

- [ ] **Step 0: Sync.**
- [ ] **Step 1: Failing tests.**
  - `tests/dat.rs`:
    - `mame_dat_still_maps_to_mame`: `system_for_dat("metadat/mame/MAME.dat")` → `mame` (not model2/model3).
    - `naomi2_never_wins_mame_dat`.
  - `tests/organize_plan.rs` `loose_model2_set_goes_to_mame`: a loose `daytona.zip` matching a MAME DAT row → `mame`.
  - `tests/organize_walk.rs` `mame_chd_folder_moves_with_its_set`: `sfiii.zip` + `sfiii/sfiii.chd` → one item whose companions include `sfiii/sfiii.chd`. On apply in a temp target, both land under the arcade folder: `target/MAME/sfiii.zip` and `target/MAME/sfiii/sfiii.chd`, compared by components.
  - `systems_batch1.rs`:
    - `SYSTEMS`: Arcade.
    - `TARGETS`: `("esde","model2","model2",None)`, `("batocera","triforce","triforce",None)`, `("onion","naomi2","NAOMI2",Some(false))`.
    - `model2` and `model3` have `libretro = "MAME"`, and the cover lookup row uses the MAME folder.
- [ ] **Step 2: Run** `cargo test -p coverly-core --test dat --test organize_plan --test organize_walk --test systems_batch1` and expect failures.
- [ ] **Step 3: Implement** the TOML (after `mame`, by appending), the rows and the CHD companion rule: a sibling folder named like an arcade set's stem that holds only `.chd` files is a companion of that set. System 16/18/24/32, Konami and Cave get **no** system (D16). Taito Type X and Cave CV1000 are owner asks (§13), recorded for the PR.
- [ ] **Step 4: Run** the tests, the snapshot and G-RUST.
- [ ] **Step 5: Commit and push:** `git add -A crates && git commit -m "feat(core): add Model 2, Model 3, NAOMI 2 and Triforce; move MAME CHD folders with their set" && git push && git status -sb`

---

## Task 14: Core: batch plan snapshots over the retro pile, and existing snapshot review

**Files:** `crates/coverly-core/tests/organize_plan.rs`, new snapshots `organize_plan__systems_batch_1__{esde,batocera,onion}.snap`, any existing `.snap` whose diff the batch causes.

- [ ] **Step 0: Sync.**
- [ ] **Step 1: Write the test.** `systems_batch_1_pile_plans` builds the spec §9.3 pile in a `tempfile` dir with synthetic bytes. Pin every mtime with `pin_mtime(path, 1)`, and pass `recent` in the past so nothing is «recently modified».

```rust
fn retro_pile(dir: &Path) {
    put(&dir.join("Gran Turismo 4 (Europe).iso"), &fx::ps2_iso("SCES_517.19"));
    put(&dir.join("Tempest 2000 (World).j64"), &[0u8; 4096]);
    put(&dir.join("Manic Miner (1983)(Bug-Byte).tzx"), &fx::tzx());
    put(&dir.join("Arkanoid (1987).dsk"), &fx::cpc_dsk());
    put(&dir.join("Sailor Moon (Japan).st"), &fx::sufami());
    put(&dir.join("Prince of Persia.st"), &fx::zeros(737_280));
    put(&dir.join("DOOM.WAD"), &fx::wad(b"IWAD"));
    put(&dir.join("Freedoom Phase 1/freedoom1.wad"), &fx::wad(b"IWAD"));
    put(&dir.join("Freedoom Phase 1/README.txt"), b"Freedoom\n");
    put(&dir.join("Cave Story/Doukutsu.exe"), b"MZ\x90\x00");
    put(&dir.join("Cave Story/data/Stage/0.pxm"), &[1u8; 64]);
    zip(&dir.join("sfiii.zip"), "sfiii.bin", &[2u8; 1024]);
    put(&dir.join("sfiii/sfiii.chd"), b"MComprHD");
    put(&dir.join("game.bin"), &[3u8; 512]);
    for e in walkdir_files(dir) { pin_mtime(&e, 1); }
}

#[test]
fn systems_batch_1_pile_plans() {
    for layout in ["esde", "batocera", "onion"] {
        let world = World::offline();
        let dir = tempfile::tempdir().unwrap();
        retro_pile(dir.path());
        let plan = plan(&world, dir.path(), defaults(layout));
        insta::assert_json_snapshot!(format!("systems_batch_1__{layout}"), redact(&plan));
    }
}
```

  Use the existing `fx` builders and add `ps2_iso`, `tzx`, `cpc_dsk`, `sufami`, `wad` to `identify::sniff::testing` or `disc::testing` if earlier tasks did not. Reuse `walkdir_files` if it exists; otherwise write a 6-line recursive `read_dir` helper.
- [ ] **Step 2: Run** `INSTA_UPDATE=always cargo test -p coverly-core --test organize_plan systems_batch_1` and **read every line** of the three snapshots against spec §9.3 and §14.3 step 1:
  - PS2 Auto with serial `SCES-51719`;
  - Jaguar and ZX Spectrum groups;
  - `Arkanoid` in review (CPC first);
  - Sailor Moon → sufami, Prince of Persia → atarist;
  - `DOOM.WAD` → doom; Freedoom → `folder-game`; Cave Story files unknown; `sfiii` with its CHD companion;
  - `game.bin` left;
  - Onion: `NoFolderOnDevice` notes for PS2/Jaguar.

  Any mismatch is a bug in Tasks 4–13: fix it there (a separate `fix(core):` commit) and do not accept the snapshot.
- [ ] **Step 3: Review existing snapshot diffs.** Run `cargo test --workspace --no-fail-fast 2>&1 | grep -E "snapshot|FAILED"`. For each changed `.snap` (for example plan snapshots with `.dsk`, `.sgx`, `.bs`, `.hdf` candidates, or `list_systems`), explain the diff in the commit body in one line each. Accept with `cargo insta accept` only after that.
- [ ] **Step 4: Run the real sample in plan-only mode** (read-only). Run `cargo test -p coverly-core --test organize_real_sample -- --ignored` if it is mounted. If not, the test skips with a message. Record the result in the commit body. Nothing is written to `/Volumes/TinyExt` or `~/Documents/ROMS`.
- [ ] **Step 5: G-RUST, G-WEB.**
- [ ] **Step 6: Commit and push:** `git add -A crates src-tauri && git commit -m "test(core): plan snapshots of the systems batch 1 retro pile" && git push && git status -sb`

---

## Task 15: Icons: silhouettes, audit, regenerate, notices

**Files:** `scripts/icons/systemIconsLib.mjs` (`SYSTEM_ICON_SOURCES`), `src/components/system/systemFallbacks.ts` (`FALLBACK_ABBR`, `WORDMARK_IDS`), generated: `src/components/system/systemIcons.generated.ts`, `src/assets/system-icons/*` (SVGs, README, contact sheet), `THIRD-PARTY-NOTICES.md`, tests `src/components/system/systemIcons.test.ts`, `SystemIcon.test.tsx`.

- [ ] **Step 0: Sync:** `git fetch origin && git merge origin/main`. Resolve `src/components/*` conflicts by hand, run G-WEB, commit the merge if there was one, and push.
- [ ] **Step 1: Failing tests.**
  - `SystemIcon.test.tsx`: `ps2` renders an `svg` (silhouette); `wasm4` renders the Slot mount with the text `W4`; `model2` renders `MDL2`.
  - `systemIcons.test.ts`: every id in `SYSTEM_ICON_SOURCES` exists in the system list that `src/ipc/mock/fixture.ts` `FIXTURE_SYSTEMS` mirrors, and every `FALLBACK_ABBR` value is ≤ 4 characters.
- [ ] **Step 2: Run** `pnpm vitest run src/components/system` and expect failures.
- [ ] **Step 3: Add the sources.**
  - Add one `SYSTEM_ICON_SOURCES` row per spec §9.2 silhouette, in the existing style (`file`, `depicts`, `audit`). Check each `file` against `crates/coverly-core/tests/fixtures/research/retroarch-assets-monochrome.txt` (the pinned list); a file missing there is not added.
  - Then run `pnpm system-icons`, open the generated contact sheet `src/assets/system-icons/contact-sheet.*` with the Read tool, and audit each new icon (D20): a device or a neutral pictogram stays; a logo, wordmark or game title art goes to `WORDMARK_IDS`, and its row is removed from `SYSTEM_ICON_SOURCES`.
  - Write the audit line for each one (what it shows). Candidate logos: DOOM, Quake, Cave Story, Rick Dangerous, Flashback, TIC-80, Uzebox, Arduboy, MrBoom, Dinothawr, Lutro.
  - Add `FALLBACK_ABBR` rows exactly as spec §9.2 lists them (`msxturbor: 'TR'`, `c128: 'C128'`, `dragon: 'DRGN'`, `coco: 'COCO'`, `wasm4: 'W4'`, `lowresnx: 'LRNX'`, `vircon32: 'V32'`, `wolf3d: 'W3D'`, `solarus: 'SOL'`, `model2: 'MDL2'`, `model3: 'MDL3'`, `naomi2: 'NAO2'`, `triforce: 'TRI'`, plus those for any icon the audit moved out).
  - Rerun `pnpm system-icons` and check that `git status` shows only generated files and the two sources.
- [ ] **Step 4: Notices.** In `THIRD-PARTY-NOTICES.md`, update the file count and the id list of the RetroArch Assets section. The licence paragraph, the commit and the changes-made text stay word for word.
- [ ] **Step 5: Run** `pnpm vitest run src/components/system`, then G-WEB.
- [ ] **Step 6: Commit and push:**
  - `git add scripts/icons src/components/system src/assets/system-icons THIRD-PARTY-NOTICES.md && git commit -m "build(icons): add silhouettes and slot labels for systems batch 1" && git push && git status -sb`

---

## Task 16: UI: `SystemInfo.family`, grouped picker, copy, mock retro pile

**REQUIRED:** frontend-design:frontend-design. Brand: `docs/08-brand/brand.md`, spec §9.1.

**Files:**
- Modify: `src-tauri/src/ipc.rs` (`SystemInfo.family`), its `list_systems` snapshot
- Modify: `src/ipc/types.ts` (`SystemFamily`, `SystemInfo.family`, `LeftReason` `folder-game`), `src/ipc/contract.sample.json`, `src/ipc/contract.test.ts`
- Create: `src/lib/systems.ts`, `src/lib/systems.test.ts`
- Modify: `src/components/organize/LeftInPlace.tsx`, `LeftInPlace.test.tsx`; `ReviewBuckets.test.tsx` (assert unchanged)
- Modify: `src/lib/organize.ts` (`folder-game`, `extension-weak` why line), its test
- Modify: `locales/es/ui.json`, `locales/en/ui.json` (spec §10 keys)
- Modify: `src/ipc/mock/mockOrganize.ts` (`?mockPick=retro`), `src/ipc/mock/fixture.ts` (`FIXTURE_SYSTEMS` with `family`; shelves gain `PS2`, `ZXS`, `WASM4`), `mockOrganize.test.ts`

**Interfaces (produced):**
```ts
export type SystemFamily = 'console' | 'handheld' | 'computer' | 'arcade' | 'engine';
export interface SystemInfo { id: string; name: string; family: SystemFamily }
export const FAMILY_ORDER: readonly SystemFamily[] =
  ['console', 'handheld', 'computer', 'arcade', 'engine'];
export interface SystemOptionGroup { family: SystemFamily; label: string; systems: SystemInfo[] }
export function systemOptionGroups(
  systems: readonly SystemInfo[], t: (k: string) => string, locale: string,
): SystemOptionGroup[];
```

- [ ] **Step 0: Sync** with `origin/main` (locales and components conflict often: resolve key by key). G-WEB, push.
- [ ] **Step 1: Failing tests.**
  - `src/lib/systems.test.ts`:

```ts
import { describe, expect, it } from 'vitest';

import { systemOptionGroups } from './systems';

const t = (k: string) => ({ 'systems.family.console': 'Consolas', 'systems.family.computer': 'Ordenadores', 'systems.family.engine': 'Motores y consolas de fantasía' })[k] ?? k;

describe('systemOptionGroups', () => {
  it('keeps the family order, sorts by locale and drops empty groups', () => {
    const g = systemOptionGroups(
      [
        { id: 'b', name: 'Bravo', family: 'console' },
        { id: 'a', name: 'Ámbar', family: 'console' },
        { id: 'z', name: 'ZX Spectrum', family: 'computer' },
        { id: 't', name: 'TIC-80', family: 'engine' },
      ],
      t,
      'es',
    );
    expect(g.map((x) => x.family)).toEqual(['console', 'computer', 'engine']);
    expect(g[0].systems.map((s) => s.name)).toEqual(['Ámbar', 'Bravo']);
    expect(g[0].label).toBe('Consolas');
  });
});
```

  - `LeftInPlace.test.tsx`: the system select has `optgroup` elements labelled «Consolas», «Portátiles», «Ordenadores», «Recreativas», «Motores y consolas de fantasía» (in that order, those present in the fixture). The placeholder «Elige un sistema…» is outside any group and disabled. Typing `zx` with `userEvent.keyboard` on the focused select selects ZX Spectrum.
  - `ReviewBuckets.test.tsx`: unchanged flat candidates, «Dejar» last.
  - `organize.test.ts`: `folder-game` renders «Juego de Doom (PrBoom) en su propia carpeta: muévela entera a prboom» with the folder in the mono path style; `ext~` evidence renders `organize.why.extensionWeak`.
  - `contract.test.ts`: `family` present on every `SystemInfo`.
  - `mockOrganize.test.ts`: `?mockPick=retro` resolves to the retro pile and its plan holds the §9.3 items.
- [ ] **Step 2: Run** `pnpm vitest run src/lib/systems.test.ts src/components/organize src/lib/organize.test.ts src/ipc` and expect failures.
- [ ] **Step 3: Implement.**
  - `ipc.rs`: `SystemInfo { id, name, family: String }` with `family: s.family.as_str().into()`; update the `list_systems` snapshot after reading its diff.
  - `systems.ts`:

```ts
export function systemOptionGroups(systems, t, locale) {
  const collator = new Intl.Collator(locale, { sensitivity: 'base' });
  return FAMILY_ORDER.map((family) => ({
    family,
    label: t(`systems.family.${family}`),
    systems: systems.filter((s) => s.family === family).sort((a, b) => collator.compare(a.name, b.name)),
  })).filter((g) => g.systems.length > 0);
}
```

  - `LeftInPlace.tsx`: keep the `NativeSelect` primitive. Children become the placeholder `<option value="" disabled>` followed by one `<optgroup label={g.label}>` per group, with no icons in options and no counts in labels (spec §9.1).
  - Locale keys, exactly spec §10 (es and en): `systems.family.*`, `organize.why.extensionWeak`, `organize.left.folderGame`, in their sorted place.
  - Mock: `MOCK_RETRO` pile and `readMockPick` `v === 'retro'`. Update the doc comment to `downloads|loose|mixed|retro`. Mirror the §9.3 items and verdicts from the Task 14 snapshot (Onion notes included when the mock profile is Onion).
- [ ] **Step 4: Visual check (frontend-design).**
  - Start `pnpm dev --port 1465 --strictPort` in the background and open `http://localhost:1465/?mockPick=retro` in Chrome DevTools MCP (`isolatedContext: "systems-batch-1"`).
  - Screenshot the «Se quedan donde están» picker open, in light and dark (`document.documentElement.dataset.theme`), and critique it against spec §9.1: groups, sentence case, no new colours, left aligned.
  - Fix and re-shoot until it passes. Stop the server: `lsof -ti tcp:1465 -sTCP:LISTEN | xargs -r kill`.
- [ ] **Step 5: Run** G-WEB and `cargo test -p coverly-roms` (the `list_systems` snapshot), then G-RUST.
- [ ] **Step 6: Commit and push.** Use two commits, each pushed:
  - `git add src-tauri src/ipc && git commit -m "feat(app): send each system's family to the UI" && git push`
  - `git add src locales && git commit -m "feat(ui): group the system picker by family and add the retro mock pile" && git push && git status -sb`

---

## Task 17: Verification, E2E, changelog and PR

**Files:** `CHANGELOG.md` (Unreleased), `docs/superpowers/specs/2026-10-26-systems-batch-1.md` (status line only: «Implemented on `feat/systems-batch-1`»), screenshots under the scratchpad (not committed).

- [ ] **Step 0: Sync** with `origin/main`. Resolve conflicts by hand, run all gates, commit the merge and push.
- [ ] **Step 1: Changelog.** Under `## [Unreleased]`, add:
  - **Added**: 64 systems, by family;
  - **Changed**: the behaviour changes of spec §12, each in one line: `.hdf`/`.fdi`/`.dim` without a header go to review; `MSX2/` folders are MSX2, with art paths moving once; PS2/3DO/Pico are organised; old custom profiles keep rule-based names until re-cloned.

  English, plain words.
- [ ] **Step 2: Full gates**, all six, from a clean tree. Paste the last line of each into the PR evidence:

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-systems-1 && pnpm check-all && pnpm build && cargo fmt --all --check && cargo clippy --workspace --all-targets --all-features -- -D warnings && scripts/dev/clippy-windows.sh && cargo test --workspace --no-fail-fast
```

- [ ] **Step 3: CLI check against a copy** (read-only real data):

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-systems-1 && cargo run -q -p coverly-cli -- systems list | wc -l && \
mkdir -p target/systems-sample && cp -RXp crates/coverly-core/tests/fixtures/research target/systems-sample/ && \
find target/systems-sample -name '._*' | wc -l && rm -rf target/systems-sample
```

  `systems list` (or the CLI's equivalent; check `coverly --help`) shows 118 systems. If the owner's real pile is mounted, run `coverly organize plan` (plan-only) on it and record the counts.
- [ ] **Step 4: Browser E2E** (spec §14.3).
  - Start `pnpm dev --port 1465 --strictPort` in the background.
  - Chrome DevTools MCP `new_page` `http://localhost:1465/?mockPick=retro` with `isolatedContext: "systems-batch-1"`. Fall back to Playwright MCP, then to `playwright-core` in the scratchpad with Chrome for Testing.
  1. Organise offer → plan. Check the PS2 group with its silhouette, the why line with `SCES-51719`, the ZX Spectrum and Jaguar groups, Sailor Moon under Sufami Turbo and Prince of Persia under Atari ST.
  2. Review, keyboard only. Tab to `Arkanoid (1987).dsk`'s select, use the arrows to «Amstrad CPC», then apply.
  3. Left in place. Check `Freedoom Phase 1` with the folder-game line, and `Cave Story` and `game.bin` unknown. In the picker: Tab, type «zx», and check that ZX Spectrum under «Ordenadores» is selected. `take_snapshot` shows the group labels in the accessibility tree.
  4. Switch the profile to Onion. The PS2 item shows «Onion OS no tiene carpeta para Sony PlayStation 2; se crea PS2.»
  5. Library shelves. Check the `PS2` silhouette and the `WASM4` mount in light and dark, with `prefers-reduced-motion: reduce` emulated, at 1280 px and at the narrowest supported width.
  - Check `list_console_messages` (no errors).
  - Save screenshots to `/private/tmp/claude-501/-Users-miguelangelesparzacalero-Documents-GitHub/35b7c020-f2ae-412f-b757-99c24a9a498b/scratchpad/systems-batch-1/e2e/`.
  - Stop the server: `lsof -ti tcp:1465 -sTCP:LISTEN | xargs -r kill`.
- [ ] **Step 5: Commit and push:** `git add CHANGELOG.md docs/superpowers/specs/2026-10-26-systems-batch-1.md && git commit -m "docs: changelog for systems batch 1" && git push && git status -sb`
- [ ] **Step 6: Open the PR** in English, with no Claude or AI mention and no generated-by line:

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-systems-1 && gh pr create --base main --head feat/systems-batch-1 \
  --title "feat: add 64 retro systems (systems batch 1)" --body-file /private/tmp/claude-501/-Users-miguelangelesparzacalero-Documents-GitHub/35b7c020-f2ae-412f-b757-99c24a9a498b/scratchpad/systems-batch-1/pr-body.md
```

  The body (written to the scratchpad, not the repo) has these sections:
  - Summary, by family.
  - Behaviour changes (spec §12).
  - Folder sources (spec §16 table).
  - **Blocked / needs the owner:**
    - Taito Type X;
    - Cave CV1000;
    - ScreenScraper ids for C128, PET, NAOMI 2 and Triforce;
    - `doom`/`quake` using 135;
    - the ES-DE `{system_folder}` media path and the muOS catalogue name;
    - D23 hosts vs new folders;
    - Onion `extlist` mismatches collected from Tasks 5–11;
    - refreshing old custom profiles;
    - every `BLOCKED:` line from the task commit bodies (`git log origin/main..HEAD --format=%b | grep BLOCKED`).
  - Gates evidence (Step 2).
  - E2E evidence with screenshots: upload them as PR comment images, or describe each one when upload is not possible.

  Post the local gate evidence as a PR comment too (GitHub Actions is blocked by billing; the gates were run locally).
- [ ] **Step 7: End state:** `git status -sb` shows `## feat/systems-batch-1...origin/feat/systems-batch-1` with nothing ahead, and `gh pr view --json url,state` prints the PR.

---

## Spec coverage check

| Spec | Task |
|---|---|
| D1 scope, D2 ids | 4–13 |
| D3 weak/strong + class snapshot | 1, 4–13 (snapshot diff each) |
| D4 weak short extensions | 1, 3, 5, 8, 11, 12 |
| D5 folder tables, Absent note | 2, 4–13 |
| D6 profile-scoped resolution, `pico`, `msx2` | 2, 6, 10 |
| D7 Knulli aliases | 5 (`o2em`), 8 (`bbc`), 12 (`tyrquake`) |
| D8 first aliases, D9 Garlic = Onion, D10 NextUI, D11 Anbernic, D12 muOS | 2, 4–13 |
| D13 disc layout, `multi_file` | 4 |
| D14 PS2/3DO ids, `zso` | 4 |
| D15 folder games | 12 |
| D16 extra arcade, CHD companions | 13 |
| D17 Famicom | 6 |
| D18/D21 families and grouped picker | 1, 16 |
| D19/D20 icons and audit | 15 |
| D22 scoped aliases | 1, 2, 8, 9, 10, 12 |
| D23 hosts | 6, 10 |
| D24 weak owner proven by header | 3, 6 |
| D25 packages | 3, 8, 11, 12 |
| D26 text-native | 3 |
| D27 facts version | 3 |
| §8.1 / §8.2 rules | 4–13 |
| §9.3 mock, §10 copy | 16 |
| §12 edge cases | 3–14 |
| §13 blocked list | 17 (PR) |
| §14 tests | every task; 14 (snapshots); 17 (E2E) |
