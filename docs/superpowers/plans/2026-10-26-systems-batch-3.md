# Missing Systems, Batch 3 (Current Generation: PS4, PS5, Xbox One, Xbox Series) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking. The UI tasks (Tasks 15, 17, 18) also REQUIRE frontend-design:frontend-design: follow `docs/08-brand/brand.md` and spec §9.1, use the `src/components/ui` primitives and critique the changed screens with screenshots before committing.

**Goal:** Coverly knows the four systems of `TODO.md` §2 «Tanda 3» (127 → 131 systems): PS4 and PS5 as first-class systems (decrypted app folders, PKG headers, ZArchive, PS5 images and app archives, overlays, DLC), Xbox One early (GDK and ERA folders, XVC GUIDs) and Xbox Series catalogued only by the Scarlett rule, marked «Sin emulador». Every reader reads only plaintext metadata inside batch 2's budgets, never a key, licence or encrypted entry, and no copy names an emulator. Embedded icons become the first cover source for these four systems, and Organizar asks before writing 10 GB or more.

**Architecture:** Everything reuses batch 2: `consoles/` readers over `Budgeted`, `ConsoleId` riding on `Sniff.console` into the memoised facts (`FACTS_VERSION` 8), `identify_dir`/`DirProbe` folder games (one `ItemKind::Folder` item, one `RomEntry`), updates/DLC listed after their base (`updates_after_bases`, `note_updates`), `sources/embedded.rs`, title-id dedupe keys. New pieces: one `sony::probe_sce_app` replacing the Vita probe (lists `sce_sys` once for PS4/PS5/Vita), `xbox::probe_gdk`/`probe_era`, a probe outcome that leaves a folder (`FolderMatch.leave`), CNT/FIH/XVD/UFS2/exFAT header hits, a tree-based `.zar` classifier shared by three systems, shadPS4 sibling overlays that follow their base (name and action), a Batocera `DLC/<TITLE_ID>/` path, an `embedded_first` system flag, and one `organize::large_transfer` function used by the plan warning, the IPC apply gate (`confirmed_bytes`, `IpcError::LargeTransfer`), the CLI (`--yes-large`) and a confirmation dialog.

**Tech Stack:** Rust 1.99 (workspace MSRV 1.93), `serde`, `serde_json`, `quick-xml` 0.38, `zip`, `ruzstd` (via the batch 2 ZArchive reader), `insta`, `filetime`, `tempfile`; React 19 + TypeScript + Tailwind 3.4 + zustand, Vitest + Testing Library; `pnpm system-icons` against RetroArch Assets at `d9f969054dc7fbb6fa89519036d2b971e0855b51`. **No new crate, no new npm package** (spec D3).

**Spec:** [`docs/superpowers/specs/2026-10-26-systems-batch-3.md`](../specs/2026-10-26-systems-batch-3.md). It is binding: decisions D1–D28 (§4), files (§5), folders (§6), entries (§7), identification (§8), UX (§9), copy (§10, word for word), edge cases (§12), blocked list (§13), tests (§14) and sources (§16). This plan turns it into tasks and records its deviations below. The batch 2 plan [`2026-10-26-systems-batch-2.md`](2026-10-26-systems-batch-2.md) is the pattern for every harness, builder and snapshot reused here.

## Global Constraints

- **Worktree and branch.** Work only in `/Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-systems-3`, branch `feat/systems-batch-3`, which tracks `origin/feat/systems-batch-3`. Agent shells reset their cwd, so every shell command starts with `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-systems-3 &&`. Never switch branches and never touch `main`.
- **Git rules (mandatory).** An earlier run lost a whole branch because it was never pushed.
  - Conventional, granular commits in English (`feat(core): …`, `test(core): …`, `feat(ui): …`, `feat(app): …`, `feat(cli): …`, `build(icons): …`, `docs: …`).
  - **No** `Co-Authored-By` line, no "Generated with" line, no mention of Claude or AI anywhere (commits, PR, code comments). This owner rule overrides any harness attribution.
  - Run `git push` **immediately after every commit**. Every task ends with `git commit … && git push && git status -sb`; the first line must read `## feat/systems-batch-3...origin/feat/systems-batch-3` with no `[ahead N]`.
  - Never force-push. To update with `main`: `git fetch origin && git merge origin/main`, resolve conflicts by hand (other workflows change `locales/*.json`, `src/components/*` and `TODO.md`; resolve locale files key by key, never by taking one side), run the gates, commit the merge, push.
- **Do not edit** `docs/TRACKER.md` or `TODO.md` on this branch (they are updated on `main` after merge).
- **Scope.** Only Tanda 3 (spec D1). **Do not implement TODO 2.1** (per-game folders with `DLC/`/`Updates/` subfolders, Mods/Cheats/Extras): updates and DLC keep Tanda 2's flat placement after their base. The Batocera `ps4/DLC/<TITLE_ID>/` path (D16) is a frontend's required path, not 2.1. No PS4/PS5/Xbox One disc images, no extraction or install of a PKG, no sync.
- **Legal/security rule (spec D4), no exceptions.** No reader takes a key parameter or derives one; nothing decrypts. `.rif`, `act.dat`, `work.bin`, `*.rap`, `*.zrif`, `*.tik`, `*.cetk`, `*.dkey` and the new `*.cik` are recognised **by name before any open** and left `KeyMaterial`. `sce_sys/license.dat` moves with its folder and is never opened. A PKG entry or `param.json` flagged encrypted is never read; the PS5 store PKG entry table is never read. `eboot.bin`, `.exe` and `resources.pri` are never opened (presence by name only). **No emulator is named, linked, recommended or downloaded** in any new locale value, note, CLI text or PR text for these systems (fake PS5/Xbox emulator repos spread malware); notes state facts about the file only. A Vitest copy guard enforces it (Task 18).
- **Huge folders: headers only, within budget (spec D24).** Per item ≤ 4 MiB read, ≤ 1 MiB decoded, ≤ 16 seeks; folder probes use `symlink_metadata` on named paths and at most one `read_dir` of one named child (≤ 256 entries) plus one probe file ≤ 64 KiB. The tree summary (≤ 100 000 entries, depth 16, metadata only) is the only pass over a big folder. Every offset and length read from a file is checked against the file length and the cap before use; every multiplication is checked. Cloud placeholders are never read.
- **Never guess a folder name.** Every `Named` cell comes from spec §6 (which cites §16). A cell that cannot be traced is `None` (Absent) and is listed in the PR. LaunchBox and Pegasus have no profile and get no names.
- **«Verify» rules.** Each item marked «verify» in the spec (PSF keys `APP_TYPE`/`PUBTOOLVER` vs `PSP2_SYSTEM_VER`, D5; PKG entry layout and encrypted bit, D9; UFS2 superblock offset, D10) is checked against its cited source during its task (LibOrbisPkg `ParamSfo.cs`/`PkgReader.cs`/`MetaEntry`, Vita3K SFO reader, FreeBSD `ufs/ffs/fs.h`). If it cannot be verified, apply the spec's fallback (D5: `CUSA` prefix only; D9: header-only verdict; D10: extension only), say so in a code comment and add `BLOCKED: <rule> — <why>` to the commit body. Task 19 collects them for the PR.
- **TDD.** Write the failing test, run it and watch it fail for the expected reason, implement, watch it pass. Each "Run" step names the exact command.
- **Gates (same as CI).** Before every commit that touches code or tests:
  - **G-RUST:** `cargo fmt --all --check && cargo clippy --workspace --all-targets --all-features -- -D warnings && scripts/dev/clippy-windows.sh && cargo test --workspace --no-fail-fast`
  - **G-WEB** (any task touching `src/`, `locales/`, `scripts/icons/`, `design/` or `src-tauri/`): `pnpm check-all && pnpm build`
  - Docs-only commits run `pnpm check-all`.
- **Windows CI leg.** Fixture mtimes pinned (`coverly_core::dedupe::testing::pin_mtime` or `filetime::set_file_mtime` to a fixed time, never "now"). Paths compared by component (`Path::components`, `file_name`, `Path::join`), never by slash text; snapshot paths go through the existing `/` redaction. Far offsets are tested through the in-memory `SparseAt`, never `set_len` files. Big folder sizes (10 GB, 48 GB) are faked through the walk's `MetaProbe::file_len` hook, never written. Unix-only helpers go in `#[cfg(unix)] mod unix_tests`. No new folder name is reserved (`CON`, `AUX`…) or ends in a dot or a space; `Microsoft Xbox Series X|S` is a display name only, never a path component.
- **Real data is read-only.** Never write to `~/Documents/ROMS`, `/Volumes/TinyExt/*` or `/Volumes/System`, never create `._*` there; only `ls`, `stat`, `head -c` into a copy and Coverly **plan-only** runs may touch them. Copies go under `/Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-systems-3/target/systems-sample/` (`cp -RXp` for trees, `head -c 1048576 <file> > …` for a head), `find target/systems-sample -name '._*' | wc -l` must print `0`, and the folder is deleted afterwards (`rm -rf target/systems-sample`). Real heads are never committed.
- **Fixtures are synthetic** bytes, XML and JSON built by builders in `consoles/testing.rs` (no copyrighted data, no keys). Real game names may appear as file names over synthetic content.
- **No UI strings in the core.** Reasons, notes and warnings are enums plus data; the UI and CLI render them. Every UI string lives in `locales/en/ui.json` **and** `locales/es/ui.json` with the same keys and placeholders, nested (a key is never both a string and an object). Copy is spec §10 word for word (Spanish: sentence case, no exclamation marks, exact counts, codes and sizes in mono).
- **Brand.** Only existing tokens from `design/tokens.json` (`pnpm tokens:check` green), `src/components/ui` primitives, `SystemIcon` for every system mark. No `--cr-warning` or tag for «Sin emulador» or «Ningún emulador lo abre así» (spec §9.1). «Xbox» and «PlayStation» appear only as the systems' display names (`TRADEMARKS.md`).
- **Icons.** Only RetroArch Assets XMB Monochrome (CC BY 4.0) at `d9f969054dc7fbb6fa89519036d2b971e0855b51`, generated with `pnpm system-icons`. Never hand-edit generated files. PS5 uses the Slot mount fallback.
- **Secrets are never read or printed:** the minisign key, `~/.coverly-release/`, any `.env*`, `.vercel/`.
- **Servers and E2E.** The web UI runs on port **1467** only: `pnpm dev --port 1467 --strictPort` (mock backend). Stop it by port only: `lsof -ti tcp:1467 -sTCP:LISTEN | xargs -r kill`. E2E uses Chrome DevTools MCP `new_page` with `isolatedContext: "systems-batch-3"`; fall back to Playwright MCP; if both profiles are locked, install `playwright-core` in `/private/tmp/claude-501/-Users-miguelangelesparzacalero-Documents-GitHub/35b7c020-f2ae-412f-b757-99c24a9a498b/scratchpad/systems-batch-3` and drive Chrome for Testing from there.
- **Blocked items never block the batch.** Anything needing an owner decision (spec §13, or a «verify» rule that fails) is implemented around, recorded as a `BLOCKED:` line in the task's commit body and collected into the PR (Task 19).

## Deviations from the spec (decided in this plan)

- **P-1: one harness for the batch.** `crates/coverly-core/tests/systems_batch3.rs` (Task 2) is table-driven like `systems_batch2.rs` (`SYSTEMS`, `TARGETS`, `CONSOLE_HEADERS`, cover lookup). Each system task appends rows.
- **P-2: the probe's leave outcome is an enum.** Spec D18 adds `FolderMatch.not_a_rom: Option<NotARom>` and separately wants an unreadable `MicrosoftGame.config` to leave the folder `Unreadable`. One field covers both without `consoles` depending on `organize`: `FolderMatch.leave: Option<FolderLeave>` with `FolderLeave::{NotARom(NotARom), Unreadable}` (`NotARom` is `identify::sniff::NotARom`, already a core type). The walk turns it into one left item, not descended.
- **P-3: `GameOverlay { of }` carries the base folder name from the walk.** The walk cannot know the base's title; it writes the base folder name, and the planner replaces it with the base's title (`base_title`) when the base is an item of the plan.
- **P-4: siblings follow their base in the planner, after decisions.** D11 (e) says a review decision on the base applies to its siblings in `decided_plan`. `State::decided_plan` calls `plan_from_survey(…, decisions)`, which applies decisions before targets; the sibling pass (`organize::target::overlay_pairs` + `siblings_follow`) runs right after the decisions loop, so the IPC path, the CLI and tests share it without touching `src-tauri`.
- **P-5: `embedded_first` reorders ranking only.** `gather` already asks every enabled source for candidates (`gather/mod.rs`, `for &src in sources`), so the only order that matters is `RankContext.sources` in `plan/mod.rs`. A helper `fill::source_order(ids, system) -> Vec<String>` moves `embedded` first when the system has the flag and `embedded` is in `ids`; it is used only there (manual search, `Filler::search`, keeps the user's order: spec D21 is about fill).
- **P-6: research listings.** `tests/fixtures/research/thumbnails-root.html` (2026-10-31) already holds `Sony - PlayStation 4/`; it is not refreshed unless a test proves a folder missing. `retroarch-assets-monochrome.txt` already holds the three icon files. `libretro-database-dats.txt` needs no change (no DAT for the four systems).

## Review Focus

1. **Keys, licences and emulators (D4).** `x.cik`, `*.rif` and `act.dat` are left before any open; `sce_sys/license.dat` is never opened (open log); an encrypted PKG entry `0x1000` and the FIH entry table are never touched (`RangeLog`); `eboot.bin` is never opened (open log); the locale copy guard finds no emulator name or URL. Tests: Task 1 `cik_is_key_material`, Task 4 `eboot_and_license_are_never_opened`, Task 5 `encrypted_param_sfo_entry_is_never_read`, Task 6 `fih_entry_table_is_never_read`, Task 18 `locale_values_name_no_emulator`.
2. **Budgets (D24).** Every reader stays within 4 MiB / 1 MiB / 16 seeks on its largest fixture; an adversarial PKG entry count (`0x8000_0000`) stops at 64 entries; `param.json` and manifests over 64 KiB are not parsed; a GUID-named file is peeked for 0x208 bytes only.
3. **No silent reclassification.** The class snapshot diff equals spec §7 exactly (`zar` Shared on three systems, `pkg` weak on four, `exfat` weak on ps5, `xvc xvd msixvc` Shared on two); a Vita folder with `PUBTOOLINFO` stays Vita; `.wua` stays Wii U; `.zar` without a decisive tree goes to review (stated behaviour change); a GDK `PC` build never becomes a console system; nothing but `TargetDeviceFamily="Scarlett"` makes an Xbox Series game.
4. **Siblings never split from their base (D11).** Name derived from the base's final name (collision and Batocera suffix carry over, a taken sibling name keeps the group together), action follows the base (left, review, not chosen), case of the suffix kept, `-mods` left without descent. Tests: Task 9.
5. **The size gate is on the decided plan (D23).** A review choice that crosses 10 GB opens the dialog; a smaller `confirmed_bytes` is refused; same-volume moves never count; resume never asks; the CLI writes nothing without `--yes-large`. Tests: Tasks 14 and 15.
6. **Windows.** Pinned mtimes, component path comparisons, `\` manifest paths split by component with `..`/root/prefix dropped, `SparseAt` for far offsets, `scripts/dev/clippy-windows.sh` on every task.

---

## File structure

```
crates/coverly-core/
├─ src/consoles/
│  ├─ mod.rs        ConsoleId.{content_id,region,product_id,parent_id}; FolderMatch.leave + FolderLeave (P-2);
│  │                  IdSource::{ParamJson,MicrosoftGameConfig,AppxManifest}; KEY_EXTS += cik; DIR_PROBES order;
│  │                  base_title_id ps4/ps5/xboxone/xboxseries; region_from_content_id
│  ├─ sony.rs       probe_sce_app (replaces probe_vita_app); PS4 PSF signals and kinds; .ps4 launcher; from_vpk PS4 reject
│  ├─ orbis.rs      (new) CNT PKG (PS4), FIH PKG (PS5), param.json parser, UFS2/exFAT checks
│  ├─ zarchive.rs   classify(tree) → Xbox360 | Ps4 | Ps5 | Ps4Dlc | Ambiguous
│  ├─ xbox.rs       zar without the magic-only fallback; probe_gdk, probe_era, MRT logo resolution; XVD header
│  └─ testing.rs    builders: ps4_sfo, ps4_app, ps5_param_json, ps5_app, cnt_pkg, fih_pkg, xvd_head, gdk_config,
│                     era_manifest, gdk_folder, era_folder, ufs2_image, exfat_image
├─ src/systems/{systems.toml,mod.rs}   four entries; embedded_first, no_emulator; zar Shared; pkg/exfat weak
├─ src/identify/{sniff.rs,hash.rs}     TEXT_NATIVE_EXTS += ps4; NotARom::ConsoleSystem; NAME_ONLY_EXTS += 8
├─ src/organize/walk.rs                FolderMatch.leave; shadPS4 user dir, sys_modules; overlay siblings;
│                                        zero-byte exception; GUID-named candidates
├─ src/organize/{plan.rs,target.rs,evidence.rs,inspect.rs,apply.rs}
│                                      LeftReason::GameOverlay; ItemNote::{ExtractFirst,NoEmulator,ProEnhanced,DlcFolder};
│                                        PlanWarning::LargeTransfer; OrganizeItem.content_id; FACTS_VERSION 8;
│                                        overlay pairs and sibling pass; Batocera DLC path; PS5 app archives;
│                                        large_transfer()
├─ src/profiles/{builtin.rs,custom.rs,mod.rs}  §6 rows; ps4 suffix (Batocera); dlc_folder map; ps5 container override
├─ src/dedupe/{keys.rs,keeper.rs}      TITLE_ID_SYSTEMS + 4; content-id and Xbox keys; region fallback
├─ src/sources/embedded.rs, src/fill/mod.rs, src/plan/mod.rs   SYSTEMS + 4; source_order (P-5)
└─ tests/
   ├─ systems_batch3.rs (new)          harness (P-1)
   ├─ consoles_current_gen.rs (new)    folder probes and file readers through walk/inspect
   ├─ organize_overlays.rs (new)       D11 siblings, D16 Batocera DLC
   ├─ organize_large_transfer.rs (new) D23
   └─ organize_folder_games.rs, organize_walk.rs, organize_evidence.rs, dedupe_keys.rs, dedupe_keeper.rs,
      fill_sources.rs, sniff.rs, systems_classes.rs, custom_profiles.rs, profile_folders.rs  updated
crates/coverly-cli/src/organize.rs     text for every new variant; --yes-large
src-tauri/src/{ipc.rs,state.rs,error.rs}   SystemInfo.no_emulator; organize_apply(…, confirmed_bytes); IpcError::LargeTransfer
src-tauri/tests/{organize_state.rs,contract.rs}   gate tests; contract members
src/ipc/{types.ts,backend.ts,tauriBackend.ts,contract.sample.json,contract.test.ts}
src/ipc/mock/{mockOrganize.ts,mockBackend.ts,fixture.ts} (+ tests)   ?mockPick=current; large-transfer; no_emulator
src/stores/organizeStore.ts (+ test)   pending confirmation, confirmApply
src/lib/organize.ts (+ test)           copy for reasons, notes, why lines, ID_FILES
src/screens/OrganizeScreen.tsx, src/components/organize/LargeTransferDialog.tsx (new, + test)
src/components/organize/SystemGroups.tsx, src/components/library/views/SectionHeaderRow.tsx (+ tests)
src/components/system/systemFallbacks.ts, scripts/icons/systemIconsLib.mjs, generated icons
src/i18n/localeGuard.test.ts (new)     copy guard and shape test
locales/{es,en}/ui.json, THIRD-PARTY-NOTICES.md, CHANGELOG.md
```

---

## Task 1: Core types: new `ConsoleId` fields, probe leave outcome, reasons, notes, warning, facts v8 (Rust, CLI text, TS types)

No behaviour change: types, serde, text and contract only.

**Files:**
- Modify: `crates/coverly-core/src/consoles/mod.rs`, `crates/coverly-core/src/identify/sniff.rs` (`NotARom::ConsoleSystem`), `crates/coverly-core/src/organize/plan.rs` (`LeftReason::GameOverlay`, four `ItemNote`s, `PlanWarning::LargeTransfer`, `OrganizeItem.content_id`, `FACTS_VERSION = 8`), `crates/coverly-cli/src/organize.rs` (`left_text`, note and warning text), `src-tauri/src/ipc.rs` (exhaustive matches, if any), `src/ipc/types.ts`, `src/ipc/contract.sample.json`, `src/ipc/contract.test.ts`, `src-tauri/tests/contract.rs`
- Test: unit tests in `consoles/mod.rs`, `organize/plan.rs`; `crates/coverly-cli` text tests; `contract.test.ts`

**Interfaces (produced):**

```rust
// consoles/mod.rs — appended to ConsoleId (all serde default + skip when None)
/// PS4/PS5 36-character content id; XVC ContentId GUID (spec §8.0).
#[serde(default, skip_serializing_if = "Option::is_none")]
pub content_id: Option<String>,
/// From the first letter of the content id (D8).
#[serde(default, skip_serializing_if = "Option::is_none")]
pub region: Option<crate::identify::names::Region>,
/// GDK StoreId, ERA Identity Name, XVC ProductId GUID (D14, D15).
#[serde(default, skip_serializing_if = "Option::is_none")]
pub product_id: Option<String>,
/// GDK DLC: its single AllowedProduct (D15).
#[serde(default, skip_serializing_if = "Option::is_none")]
pub parent_id: Option<String>,

pub enum IdSource { /* existing */ ParamJson, MicrosoftGameConfig, AppxManifest }

/// Why a probe that recognised a folder leaves it in place (plan P-2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FolderLeave {
    NotARom(crate::identify::sniff::NotARom),
    /// The probe file exists but cannot be read (Windows ACLs).
    Unreadable,
}
pub struct FolderMatch { pub id: ConsoleId, pub probe: PathBuf, pub leave: Option<FolderLeave> }

const KEY_EXTS: &[&str] = &["rap", "rif", "dkey", "zrif", "tik", "cetk", "cik"];

/// shadPS4 launcher `GameListUtils::GetRegion(content_id[0])` (D8).
pub fn region_from_content_id(content_id: &str) -> Option<Region> {
    match content_id.as_bytes().first()? {
        b'U' => Some(Region::Usa),
        b'E' => Some(Region::Europe),
        b'J' => Some(Region::Japan),
        b'H' => Some(Region::Asia),
        b'I' => Some(Region::World),
        _ => None,
    }
}
```

```rust
// identify/sniff.rs
pub enum NotARom { /* existing */ ConsoleSystem }   // serde "console-system"

// organize/plan.rs
pub enum LeftReason { /* existing */
    /// A shadPS4 `-mods`/`-UPDATE`/`-patch` folder beside its base that
    /// cannot move alone (D11 d, e): `of` is the base's title, or its
    /// folder name when the base is not an item (plan P-3).
    GameOverlay { of: String },
}
pub enum ItemNote { /* existing */
    /// A PS4 PKG: catalogued, has to be extracted to play (D9).
    ExtractFirst,
    /// A PS5 store PKG, `.exfat`, `.ffpfs`, `.ffpfsc`, or an XVC (D10, D19).
    NoEmulator,
    /// PS5 `attribute3` bit 22 (KytyPS5 `title_supports_trinity`).
    ProEnhanced,
    /// Batocera DLC path (D16): `layout` is the profile's display name.
    DlcFolder { layout: String },
}
pub enum PlanWarning { /* existing */
    /// The items that move by default write at least LARGE_TRANSFER bytes (D23).
    LargeTransfer { bytes: u64, items: usize },
}
// OrganizeItem, after title_id:
/// The console content id (PKG header, param.sfo, param.json, XVC), for the
/// row's «why» line (spec §10).
#[serde(default, skip_serializing_if = "Option::is_none")]
pub content_id: Option<String>,

const FACTS_VERSION: u32 = 8;
```

```ts
// src/ipc/types.ts additions
export type NotARom = /* existing */ | 'console-system';
// LeftReason union: | { kind: 'game-overlay'; of: string }
// ItemNote union:   | { kind: 'extract-first' } | { kind: 'no-emulator' } | { kind: 'pro-enhanced' }
//                   | { kind: 'dlc-folder'; layout: string }
// PlanWarning union:| { kind: 'large-transfer'; bytes: number; items: number }
// IdSource:         | 'param-json' | 'microsoft-game-config' | 'appx-manifest'
// OrganizeItem:     content_id?: string;
```

- [ ] **Step 1: Failing tests.**
  - `consoles::tests::cik_is_key_material`: `is_key_material("x.cik")`, `"X.CIK"` true; `"license.dat"` false (spec D4: not listed).
  - `consoles::tests::console_id_new_fields_serde`: a `ConsoleId` with all four fields round-trips through JSON; one without them serialises without the keys (snapshot the JSON string) and an old JSON without them deserialises with `None`s.
  - `consoles::tests::region_letters`: `U E J H I` → `Usa Europe Japan Asia World`; `K`, `X`, `""` → `None`.
  - `organize::plan::tests::new_variants_serde`: `LeftReason::GameOverlay { of: "Bloodborne".into() }` → `{"kind":"game-overlay","of":"Bloodborne"}`; `NotARom::ConsoleSystem` → `"console-system"`; each new note and `PlanWarning::LargeTransfer` round-trips; `OrganizeItem` without `content_id` omits the key.
  - CLI: `left_text(&LeftReason::GameOverlay { of: "Bloodborne".into() })` is `mods or patch folder of Bloodborne: left where it is`; `left_text(NotARom ConsoleSystem)` contains `console system file`; note texts `this package does not open as is: it has to be extracted first`, `no emulator opens it in this form`, `enhanced for PS5 Pro`, `goes to the DLC folder, where Batocera looks for downloadable content`; warning `{bytes} will be written to the target; you will be asked to confirm`.
  - `contract.test.ts` and `src-tauri/tests/contract.rs`: the sample JSON gains one item of each new member (`game-overlay`, `console-system`, four notes, `large-transfer` warning, `content_id`, the three `IdSource` values) and both sides parse it.
- [ ] **Step 2: Run** `cargo test -p coverly-core --lib consoles:: organize::plan::tests && cargo test -p coverly-cli && cargo test -p coverly-roms --test contract && pnpm vitest run src/ipc/contract.test.ts` — expect compile failures and missing variants.
- [ ] **Step 3: Implement** the types above. `ConsoleId::with` sets the four fields to `None`. Every `FolderMatch { id, probe }` literal in `sony.rs`, `wiiu.rs`, `xbox.rs`, `nx.rs`, tests and `testing.rs` gains `leave: None` (`grep -rn "FolderMatch {" crates/coverly-core/src crates/coverly-core/tests`). Exhaustive matches over `LeftReason`, `ItemNote`, `PlanWarning`, `NotARom`, `IdSource` in core, CLI and `src-tauri` get arms (compile errors list them). TS: extend the unions. Where `tsc` demands exhaustive switches in `src/lib/organize.ts`, add the real arms now with the §10 keys below (Task 18 adds their tests and the rest of the UI), so no placeholder copy ships:
  - `organize.note.extractFirst`, `organize.note.noEmulator`, `organize.note.proEnhanced`, `organize.note.dlcFolder`, `organize.left.gameOverlay`, `organize.left.consoleSystem`, `organize.why.contentId` (es/en exactly as spec §10).
- [ ] **Step 4: Run** the Step 2 command, then G-RUST and G-WEB.
- [ ] **Step 5: Commit and push:** `git add -A crates src-tauri src locales && git commit -m "feat(core): add current-generation console fields, reasons, notes and facts v8" && git push && git status -sb`

---

## Task 2: Systems: four entries, `embedded_first` and `no_emulator`, `zar` Shared, harness and class snapshot; `SystemInfo.no_emulator`

**Files:**
- Modify: `crates/coverly-core/src/systems/systems.toml`, `crates/coverly-core/src/systems/mod.rs` (`RawSystem.embedded_first`, `RawSystem.no_emulator`, `System.embedded_first: bool`, `System.no_emulator: bool`, lenient user merge like `libretro_also`), `crates/coverly-core/src/identify/sniff.rs` (`TEXT_NATIVE_EXTS` += `ps4`, now 18), `crates/coverly-core/src/identify/hash.rs` (`NAME_ONLY_EXTS` += `ps4 ffpkg ffpfs ffpfsc exfat xvc xvd msixvc`, now 35), `src-tauri/src/ipc.rs` (`SystemInfo.no_emulator`), `src/ipc/types.ts` (`SystemInfo.no_emulator: boolean`), `src/ipc/contract.sample.json`, `src/ipc/mock/fixture.ts` (`no_emulator: false` everywhere, `true` for `xboxseries` in the mock system list)
- Create: `crates/coverly-core/tests/systems_batch3.rs`
- Test: `tests/systems_classes.rs` (snapshot update), `tests/systems_batch3.rs`, `src-tauri/tests/contract.rs`

**TOML rows** (spec §7; comments carry the sources):

```toml
[[system]]
id = "ps4"
name = "Sony PlayStation 4"
family = "console"
aliases = ["PS4", "playstation4"]
# Decrypted `CUSAxxxxx/` app folders are folder games (D5); `.ps4` is the
# ES-DE «game serial» file (D17); `.zar` is shared with Xbox 360 and PS5
# (D12); `pkg` is weak: magic decides (D9).
extensions = ["ps4", "zar"]
weak_extensions = ["pkg"]
libretro = "Sony - PlayStation 4"
screenscraper = 60
embedded_first = true

[[system]]
id = "ps5"
name = "Sony PlayStation 5"
family = "console"
aliases = ["PS5", "playstation5"]
# `PPSAxxxxx/` app folders (D6); `.ffpkg` UFS2 images play, `.ffpfs`/
# `.ffpfsc` are console-side images (D10); `.exfat` needs a `PPSA` token
# or the folder (weak).
extensions = ["ffpkg", "ffpfs", "ffpfsc", "zar"]
weak_extensions = ["pkg", "exfat"]
screenscraper = 284
embedded_first = true

[[system]]
id = "xboxone"
name = "Microsoft Xbox One"
family = "console"
aliases = ["XBOXONE", "xb1"]
# GDK (`MicrosoftGame.config`) and ERA (`AppxManifest.xml`) folders (D18);
# XVC packages are shared with Xbox Series: the plaintext header cannot
# tell them apart (D19).
extensions = ["xvc", "xvd", "msixvc"]
screenscraper = 34
embedded_first = true

[[system]]
id = "xboxseries"
name = "Microsoft Xbox Series X|S"
family = "console"
aliases = ["XBOXSERIES", "xsx", "xboxseriesx"]
# Only `Executable/@TargetDeviceFamily="Scarlett"` makes a Series game
# (D18). No public emulator (D22). No ScreenScraper id found (§13).
extensions = ["xvc", "xvd", "msixvc"]
embedded_first = true
no_emulator = true
```

`xbox360` keeps `zar` in its list (it becomes Shared automatically). Field docs on `RawSystem.embedded_first`/`no_emulator` say a user file using them is rejected by an older Coverly (`deny_unknown_fields`), as spec §3 accepts.

**Harness** (`tests/systems_batch3.rs`, copy the shape of `systems_batch2.rs`):

```rust
const SYSTEMS: &[(&str, Family, bool)] = &[
    ("ps4", Family::Console, false),
    ("ps5", Family::Console, false),
    ("xboxone", Family::Console, false),
    ("xboxseries", Family::Console, false),
];
const FLAGS: &[(&str, bool, bool)] = &[ // (id, embedded_first, no_emulator)
    ("ps4", true, false), ("ps5", true, false), ("xboxone", true, false), ("xboxseries", true, true),
];
const TARGETS: &[(&str, &str, &str, Option<bool>)] = &[]; // Task 3 fills it
const CONSOLE_HEADERS: &[ConsoleHeader] = &[];             // Tasks 4–11 fill it
```

Checks: each id loads with its name and family; `SystemDb::builtin().systems().len() == 131`; `libretro` names are in `tests/fixtures/research/thumbnails-root.html` (only `ps4`); every alias resolves to its system through `resolve_folder`; `xboxseries` display name is never a resolved folder; `FLAGS` match.

- [ ] **Step 1: Failing tests.** The harness above; in `systems_classes.rs` the snapshot will change (accept only after reading the diff: `zar` Shared `[ps4, ps5, xbox360]`, `ffpkg ffpfs ffpfsc ps4` Unique, `xvc xvd msixvc` Shared `[xboxone, xboxseries]`, `pkg` and `exfat` never listed as a class since weak). A `systems::tests::user_file_flags_are_lenient` test: a user TOML without the two keys loads with `false`; one with them loads `true`. `src-tauri` contract: `SystemInfo` serialises `no_emulator`.
- [ ] **Step 2: Run** `cargo test -p coverly-core --test systems_batch3 --test systems_classes --lib systems:: && cargo test -p coverly-roms --test contract` — expect failures.
- [ ] **Step 3: Implement.** Add the rows, the two fields, the `HEADER_DECIDED_EXTS` check stays `["pkg"]` (validation must accept `pkg` weak on four systems and `exfat` weak on one), `TEXT_NATIVE_EXTS`, `NAME_ONLY_EXTS`. `cargo insta review` (or `INSTA_UPDATE=always` then read the `.snap` diff) — the diff must equal the list above, nothing else. `SystemInfo { id, name, family, no_emulator }` from `System.no_emulator`.
- [ ] **Step 4: Run** Step 2, G-RUST, G-WEB.
- [ ] **Step 5: Commit and push:** `git add -A crates src-tauri src && git commit -m "feat(core): add PS4, PS5, Xbox One and Xbox Series system entries" && git push && git status -sb`

---

## Task 3: Profiles: §6 folder rows, Batocera `.ps4` suffix and `dlc_folder`, PS5 archives taken as is

**Files:**
- Modify: `crates/coverly-core/src/profiles/builtin.rs` (`SYSTEM_FOLDERS` rows; Batocera `folder_game_suffix` gains `ps4 → .ps4`; Batocera `dlc_folder`), `crates/coverly-core/src/profiles/mod.rs` (`Profile.dlc_folder: BTreeMap<String, String>`; `default_container_overrides()` gains `ps5 → vec![ContainerKind::Zip, ContainerKind::SevenZip, ContainerKind::Rar]` (`crate::archive::ContainerKind`), the containers a `ps5` item keeps as is; `NO_ARCHIVE_SYSTEMS` entries keep their empty list), `crates/coverly-core/src/profiles/custom.rs` (`ProfileFile.dlc_folder` with `#[serde(default, skip_serializing_if = "BTreeMap::is_empty")]`, copied by duplicate like `folder_game_suffix`, validated: a relative template whose only placeholder is `{title_id}`, no `..`, no root)
- Test: `tests/systems_batch3.rs` (`TARGETS`), `tests/profile_folders.rs`, `tests/custom_profiles.rs`

**Rows** (spec §6; columns `[esde, batocera, muos, onion, minui]`):

```rust
    // Systems batch 3 (spec §6): ES-DE `es_systems.xml` at ef267ae6,
    // Batocera `es_systems.yml` at cc793b75 (Knulli shares the profile).
    // No frontend has a PS5 or Xbox Series system; Batocera has no Xbox One.
    ("ps4", [Some("ps4"), Some("ps4"), None, None, None]),
    ("ps5", [None, None, None, None, None]),
    ("xboxone", [Some("xboxone"), None, None, None, None]),
    ("xboxseries", [None, None, None, None, None]),
```

```rust
/// Batocera `es_systems.yml` (`ps4`): «rename game folder to end with .ps4»
/// (D16). ES-DE's PS4 entries are `.ps4` serial files, never folders.
fn batocera_folder_suffix() -> BTreeMap<String, String> {
    let mut m = ps3_folder_suffix();
    m.insert("ps4".into(), ".ps4".into());
    m
}

/// Batocera's shadPS4 launcher sets `addon_install_dir` to `roms/ps4/DLC`
/// and shadPS4 reads `<dir>/<TITLE_ID>/<content>/` (D16).
fn batocera_dlc_folder() -> BTreeMap<String, String> {
    BTreeMap::from([("ps4".to_string(), "DLC/{title_id}".to_string())])
}
```

`TARGETS` rows: `("esde","ps4","ps4",None)`, `("batocera","ps4","ps4",None)`, `("esde","xboxone","xboxone",None)`, `("batocera","xboxone","xboxone",Some(false))`, `("esde","ps5","ps5",Some(false))`, `("batocera","xboxseries","xboxseries",Some(false))`, `("muos","ps4","ps4",Some(false))`, `("onion","ps5","PS5",Some(false))` — check the Absent naming rule's output for Onion/MinUI with `system_folder` before writing the expected value (batch 1 D5 naming: upper-case first alias on Onion). The harness loops every built-in × every new system and asserts `Named` exactly where §6 has a name, `on_device == Some(false)` everywhere else.

- [ ] **Step 1: Failing tests.** Harness `TARGETS`; `profile_folders.rs`: Batocera `folder_game_suffix["ps4"] == ".ps4"`, ES-DE has no `ps4` suffix (still `ps3`); `dlc_folder` only on Batocera; `custom_profiles.rs`: duplicate of Batocera carries `dlc_folder` and the `ps4` suffix; a stored custom profile without `dlc_folder` loads with an empty map and serialises without the key; `dlc_folder` value `../x` or `/x` is rejected with a field error `dlc_folder.ps4`; `default_container_overrides()` contains `ps5`.
- [ ] **Step 2: Run** `cargo test -p coverly-core --test systems_batch3 --test profile_folders --test custom_profiles --lib profiles::` — expect failures.
- [ ] **Step 3: Implement** as above.
- [ ] **Step 4: Run** Step 2, G-RUST (profiles feed the IPC: G-WEB too if `src/ipc/types.ts` `ProfileInfo` mirrors these maps — `grep -n "folder_game_suffix" src/ipc/types.ts`; if it does, add `dlc_folder` there and in the settings UI's duplicate path tests).
- [ ] **Step 5: Commit and push:** `git add -A crates src && git commit -m "feat(core): add profile folders for the current-generation consoles" && git push && git status -sb`

---

## Task 4: PS4 and Vita folders through one `sce_sys` probe; PS4 kinds, region, `.ps4` launcher

**Files:**
- Modify: `crates/coverly-core/src/consoles/sony.rs` (`probe_sce_app` replaces `probe_vita_app`; `psf_system`; `from_psf` PS4 arm; `from_vpk` rejects PS4 signals; `launcher` gains `ps4`), `crates/coverly-core/src/consoles/mod.rs` (`DIR_PROBES` third entry; `base_title_id` arms `ps4 | ps5`), `crates/coverly-core/src/consoles/testing.rs` (`ps4_sfo`, `ps4_app`)
- Create: `crates/coverly-core/tests/consoles_current_gen.rs`
- Test: `sony.rs` unit tests, `tests/consoles_current_gen.rs`, `tests/organize_folder_games.rs`, `tests/systems_batch3.rs`

**Verify first (D5).** Open LibOrbisPkg `LibOrbisPkg/SFO/ParamSfo.cs` and Vita3K's SFO handling (`vita3k/io/src/io.cpp` or `vita3k/util/src/sfo.cpp`) and confirm: PS4 SFOs carry `APP_TYPE` (int) and `PUBTOOLVER`; Vita SFOs carry `PSP2_SYSTEM_VER`; PS3 SFOs carry `PS3_SYSTEM_VER`; categories `gd`, `gp*`, `ac` on PS4. Record the commit you read in a code comment. If a key cannot be confirmed, `is_ps4_psf` shrinks to the `CUSA` prefix and the commit body gets `BLOCKED: PS4 PSF key rule — <why>`.

**Code:**

```rust
/// The system a parsed `param.sfo` names (spec D5): PS4 when `TITLE_ID`
/// starts with `CUSA`, or when it has `APP_TYPE` or `PUBTOOLVER` and
/// neither `PSP2_SYSTEM_VER` nor `PS3_SYSTEM_VER`. `PUBTOOLINFO` is not a
/// PS4 signal (Vita SFOs carry it). A `CUSA` id with `PSP2_SYSTEM_VER` is
/// a contradiction: both systems, the caller makes it a conflict.
pub(crate) fn psf_systems(psf: &Psf) -> &'static [&'static str] {
    let cusa = psf.text("TITLE_ID").and_then(title_id).is_some_and(|t| is_ps4(&t));
    let vita = psf.text("PSP2_SYSTEM_VER").is_some() || psf.int("PSP2_SYSTEM_VER").is_some();
    let ps3 = psf.text("PS3_SYSTEM_VER").is_some();
    let ps4_keys = psf.int("APP_TYPE").is_some() || psf.text("PUBTOOLVER").is_some()
        || psf.int("PUBTOOLVER").is_some();
    match (cusa, vita) {
        (true, true) => &["ps4", "psvita"],
        (true, false) => &["ps4"],
        (false, _) if ps4_keys && !vita && !ps3 => &["ps4"],
        _ => &["psvita"],
    }
}
```

`from_psf` gains the PS4 kind arm (D7):

```rust
        ("ps4", Some("gd")) => TitleKind::Base,
        ("ps4", Some("gp" | "gpc" | "gpd" | "gpe" | "gpk" | "gpl")) => TitleKind::Update,
        ("ps4", Some("ac")) => TitleKind::Dlc,
```

then for `ps4`: `APP_TYPE == 3` → `Demo` (overrides Base); `CONTENT_ID` (36 chars, ASCII) → `content_id`, `region = region_from_content_id`; `id.serial = None` for `ps4` (no DAT; `normalise_console_serial` returns `None` for unknown systems — check, else set explicitly).

```rust
/// A decrypted PS4, PS5 or Vita app folder (spec D5, D6): `sce_sys` among
/// its names, listed **once**; `param.json` → PS5 (Task 6), `param.sfo` →
/// PS4 or Vita by [`psf_systems`]; both files, or a PSF contradiction →
/// the caller's `DirConflict` (returned as two matches: see below).
/// `eboot.bin` is only looked up by name; `sce_sys/license.dat` and
/// `sce_sys/package/work.bin` are never opened. Art: `sce_sys/icon0.png`.
pub fn probe_sce_app(l: &DirListing) -> Option<FolderMatch>
```

`identify_dir` returns `DirConflict` when two probes name different systems, but here one probe sees the contradiction. Add `pub fn probe_sce_app_all(l) -> Vec<FolderMatch>` semantics by giving `DirProbe` a sibling type: change `DIR_PROBES` to hold `ProbeFn` enum `{ One(DirProbe), Many(fn(&DirListing) -> Vec<FolderMatch>) }`, and `identify_dir` iterates every match of a `Many`. Keep `set_extra_probe`'s `DirProbe` signature (test hook) by wrapping it in `One`. `probe_sce_app` (the `One` form, used by dedupe and scan) returns the first match; `sce_matches` (the `Many` form, in `DIR_PROBES` at the Vita probe's position) returns one `FolderMatch` per system named.

Folder-name fallback (D7): with no `CATEGORY`, a folder name ending `-UPDATE`/`-patch` (ASCII case-insensitive) → `Update`, else `Unknown`. Title id fallback: when the PSF has no `TITLE_ID` and the folder name starts with `CUSA\d{5}` → that id with `IdSource::FileName`.

`launcher` gains `"ps4" => "ps4"` (spec D17: one line with `CUSA07010`, ≤ 64 bytes). `from_vpk` returns `None` when `psf_systems` names `ps4`.

`base_title_id`: `"ps3" | "psvita" | "xbox360" | "ps4" | "ps5" if !tid.is_empty() => Some(tid.to_ascii_uppercase())` (D15).

**Builders** (`testing.rs`):

```rust
/// A PS4 `param.sfo` (fmt `04 02` text, `04 04` int, as `param_sfo` writes).
pub fn ps4_sfo(title_id: &str, category: &str, content_id: &str, app_type: u32) -> Vec<u8>;
/// `<dir>/sce_sys/param.sfo`, `sce_sys/icon0.png` (8×8 PNG), empty `eboot.bin`,
/// `sce_sys/license.dat` (8 bytes); mtimes pinned.
pub fn ps4_app(dir: &Path, title_id: &str, category: &str, content_id: &str);
```

`param_sfo` today writes texts only; add an `ints: &[(&str, u32)]` variant (`param_sfo_with`) if `APP_TYPE` needs an int.

- [ ] **Step 1: Failing tests.**
  - `sony.rs`: `ps4_by_cusa`; `ps4_by_app_type` (title id `ABCD12345`, `APP_TYPE 1`, no Vita key); `vita_with_pubtoolinfo_stays_vita` (`PUBTOOLINFO` + `PSP2_SYSTEM_VER`); `cusa_with_psp2_system_ver_conflicts`; kinds `gd`/`gp`/`gpc`/`ac`/`APP_TYPE 3` → Base/Update/Update/Dlc/Demo; `update_by_folder_suffix_without_category` (`CUSA00900-UPDATE`, `CUSA00900-patch`, `cusa00900-update`); region from `UP9000-…`, `EP…`, `JP…`, `HP…`, `IP…`, `KP…` (none); `.ps4` launcher `CUSA07010\n` → ps4 with title id, 65 bytes → none; `from_vpk` with a PS4 SFO → `None`.
  - `probe_sce_app_lists_sce_sys_once`: count `read_dir` calls with the existing listing hook if there is one (`grep -n "LIST_LOG\|list_log" crates/coverly-core/src/consoles`), else add a `#[cfg(any(test, feature = "test-util"))]` thread-local counter in `wiiu::list_child`.
  - `tests/consoles_current_gen.rs`: `CUSA00900/` (`ps4_app`) → one `Folder` item `ps4/CUSA00900` on ES-DE (`system_id ps4`, `title_id` `CUSA00900` from `ParamSfo`, `content_id` set), `ps4/CUSA00900.ps4` on Batocera with `FolderSuffix`; `eboot_and_license_are_never_opened` (`consoles::start_open_log()`; plan; `take_open_log()` holds `sce_sys/param.sfo` only); a Vita app (`vita_app`) still `psvita/PCSE00844`; a `CUSA` + `PSP2_SYSTEM_VER` folder → Review with alternatives `ps4`, `psvita`.
  - Harness `CONSOLE_HEADERS`: `ps4` launcher text row.
- [ ] **Step 2: Run** `cargo test -p coverly-core --lib consoles::sony consoles::tests && cargo test -p coverly-core --test consoles_current_gen --test organize_folder_games --test systems_batch3 --test sniff` — expect failures.
- [ ] **Step 3: Implement** as above. Remove `probe_vita_app` (update its tests to `probe_sce_app`); the doc comment on `DIR_PROBES` lists the new order (spec §5.1).
- [ ] **Step 4: Run** Step 2, then G-RUST.
- [ ] **Step 5: Commit and push:** `git add -A crates && git commit -m "feat(core): identify PS4 app folders and launcher files with one sce_sys probe" && git push && git status -sb`

---

## Task 5: PS4 PKG (CNT header, plaintext `param.sfo` and `icon0.png` entries) and `ExtractFirst`

**Files:**
- Create: `crates/coverly-core/src/consoles/orbis.rs` (`cnt_pkg`), registered in `consoles/mod.rs`
- Modify: `consoles/sony.rs` (`identify` dispatches `7F 43 4E 54` to `orbis::cnt_pkg`), `consoles/testing.rs` (`cnt_pkg`), `identify/sniff.rs` (head-only hit for the CNT magic so a `.pkg` outside a folder gets header evidence even from a 64 KiB head — check whether the `consoles::identify` call already covers heads, batch 2 P-1; if it does, no sniff change), `organize/evidence.rs` (`ExtractFirst` for a `ps4` item whose payload `detail == orbis::CNT_DETAIL`), `organize/plan.rs` (copy `content_id` from the item's `ConsoleId` into `OrganizeItem.content_id`, like `title_id`)
- Test: `orbis.rs` unit tests, `tests/consoles_current_gen.rs`, `tests/organize_evidence.rs`, `tests/systems_batch3.rs`

**Verify first (D9).** In LibOrbisPkg `LibOrbisPkg/PKG/PkgReader.cs` (`ReadHeader`, `ReadMetaEntry`) and `Pkg.cs` (`MetaEntry`), confirm: header big-endian; `entry_count` u32 at 0x10, `entry_table_offset` u32 at 0x18, content id 0x40 (36 bytes), content type u32 at 0x74, content flags u32 at 0x78; entry = 32 bytes (`id` u32, `name_table_offset` u32, `flags1` u32, `flags2` u32, `data_offset` u32, `data_size` u32, 8 padding), encrypted bit = `flags1 & 0x8000_0000`. Record the commit read. If the entry layout cannot be confirmed: no entry is read, header-only verdict, `BLOCKED: PS4 PKG entry table — <why>`.

**Code (`orbis.rs`):**

```rust
//! PS4 and PS5 package and image headers (spec D9, D10). Only the CNT
//! header, the entry table (PS4, ≤ 64 entries) and a plaintext
//! `param.sfo` entry are read. Encrypted entries, the PFS body, licences
//! and the PS5 entry table are never read (D4).

pub const CNT_MAGIC: [u8; 4] = [0x7F, b'C', b'N', b'T'];
pub const CNT_DETAIL: &str = "PS4 PKG header";
const CNT_HEADER: usize = 0x80;
const ENTRY_LEN: usize = 32;
const MAX_ENTRIES: u32 = 64;
const ENTRY_PARAM_SFO: u32 = 0x1000;
const ENTRY_ICON0: u32 = 0x1200;
const ENCRYPTED: u32 = 0x8000_0000;
const CT_GD: u32 = 0x1A;
const CT_AC: u32 = 0x1B;
const CT_AL: u32 = 0x1C;
const CT_DP: u32 = 0x1E;
const PATCH_FLAGS: [u32; 4] = [0x0010_0000, 0x4000_0000, 0x4100_0000, 0x6000_0000];

fn be32(h: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_be_bytes(h.get(o..o + 4)?.try_into().ok()?))
}

/// A CNT package (`7F 43 4E 54`): content id, title id (`content_id[7..16]`),
/// kind by content type and flags, and, when not flagged encrypted and
/// ≤ 64 KiB, the `param.sfo` entry through `psf::parse` (its `CATEGORY`
/// wins). System: PSF says PS4 or `CUSA` → `ps4`; `PPSA` → `ps5`; else
/// `ps4` at 0.80 (review).
pub fn cnt_pkg(b: &mut Budgeted) -> Option<ConsoleId> {
    let h = b.read(0, CNT_HEADER).ok()?;
    if h[..4] != CNT_MAGIC { return None; }
    let content_id = std::str::from_utf8(h.get(0x40..0x40 + 36)?).ok()
        .filter(|s| s.bytes().all(|c| c.is_ascii_graphic())).map(str::to_owned);
    let tid = content_id.as_deref().and_then(|c| c.get(7..16)).filter(|t| retail_id(t))
        .map(str::to_ascii_uppercase);
    let ctype = be32(&h, 0x74)?;
    let flags = be32(&h, 0x78)?;
    let header_kind = match ctype {
        CT_GD if PATCH_FLAGS.iter().any(|f| flags & f == *f) => TitleKind::Update,
        CT_GD => TitleKind::Base,
        CT_DP => TitleKind::Update,
        CT_AC | CT_AL => TitleKind::Dlc,
        _ => TitleKind::Unknown,
    };
    let entries = entry_table(b, &h).unwrap_or_default();
    let psf = entries.iter().find(|e| e.id == ENTRY_PARAM_SFO)
        .and_then(|e| plain_entry(b, e, MAX_META_FILE))
        .and_then(|bytes| psf::parse(&bytes));
    let psf_says = psf.as_ref().map(sony::psf_systems);
    let system = match (&tid, psf_says) {
        (_, Some(s)) if s.contains(&"ps4") => "ps4",
        (Some(t), _) if t.starts_with("CUSA") => "ps4",
        (Some(t), _) if t.starts_with("PPSA") => "ps5",
        _ => "ps4",
    };
    let named = psf_says.is_some_and(|s| s.contains(&"ps4"))
        || tid.as_deref().is_some_and(|t| t.starts_with("CUSA") || t.starts_with("PPSA"));
    let mut id = match &psf {
        Some(p) if system == "ps4" => sony::from_psf(p, "ps4", CNT_DETAIL),
        _ => ConsoleId::proven(system, CNT_DETAIL),
    };
    if !named {
        id.confidence = 0.80;
    }
    id.detail = CNT_DETAIL.into();
    if id.title_id.is_none() {
        id.title_id = tid;
        id.id_source = id.title_id.as_ref().map(|_| IdSource::Header);
    }
    // The PSF `CATEGORY` wins when it gave a kind (D9).
    if psf.is_none() || id.kind == TitleKind::Unknown {
        id.kind = header_kind;
    }
    id.region = content_id.as_deref().and_then(super::region_from_content_id);
    id.content_id = content_id;
    id.art.extend(entries.iter().find(|e| e.id == ENTRY_ICON0)
        .filter(|e| e.flags1 & ENCRYPTED == 0 && u64::from(e.size) <= MAX_DECODED)
        .filter(|e| u64::from(e.offset) + u64::from(e.size) <= b.len())
        .map(|e| ArtLocator::Range { offset: e.offset.into(), len: e.size.into(), format: ArtFormat::Png }));
    Some(id)
}

/// One 32-byte entry of the table (LibOrbisPkg `MetaEntry`).
struct Entry { id: u32, flags1: u32, offset: u32, size: u32 }

/// At most MAX_ENTRIES entries, whatever `entry_count` says; the table
/// range is checked against the file length first.
fn entry_table(b: &mut Budgeted, h: &[u8]) -> Option<Vec<Entry>> {
    let count = be32(h, 0x10)?.min(MAX_ENTRIES) as usize;
    let at = u64::from(be32(h, 0x18)?);
    let len = count.checked_mul(ENTRY_LEN)?;
    if at.checked_add(len as u64)? > b.len() { return None; }
    let t = b.read(at, len).ok()?;
    Some(t.chunks_exact(ENTRY_LEN).filter_map(|e| Some(Entry {
        id: be32(e, 0)?, flags1: be32(e, 8)?, offset: be32(e, 16)?, size: be32(e, 20)?,
    })).collect())
}

/// The bytes of an entry that is not flagged encrypted, ≤ `max`, inside the file.
fn plain_entry(b: &mut Budgeted, e: &Entry, max: u64) -> Option<Vec<u8>> {
    if e.flags1 & ENCRYPTED != 0 || u64::from(e.size) > max { return None; }
    if u64::from(e.offset).checked_add(e.size.into())? > b.len() { return None; }
    b.read(e.offset.into(), e.size as usize).ok()
}

```

Reuse `sony::retail_id`/`title_id` (make them `pub(super)`) and make `sony::psf_systems` `pub(super)`.

**Builder:** `pub fn cnt_pkg(content_id: &str, ctype: u32, flags: u32, entries: &[(u32, u32 /*flags1*/, Vec<u8>)]) -> SparseAt` — header at 0, entry table at 0x2A80 (a realistic offset), entry data after it.

- [ ] **Step 1: Failing tests.**
  - `orbis.rs`: content id and title id; kinds `0x1A` → Base, `0x1A`+`0x0010_0000` → Update, `0x1E` → Update, `0x1B`/`0x1C` → Dlc; PSF `CATEGORY gp` overrides type `0x1A`; `PPSA` content id → `ps5`; neither → `ps4` at 0.80; `encrypted_param_sfo_entry_is_never_read` (entry `0x1000` with `flags1 = ENCRYPTED`, wrapped in `RangeLog`: the entry's range is untouched and the verdict is header-only, `IdSource::Header`); a non-PSF `0x1000` entry → header-only; entry count `0x8000_0000` → at most 64 entries read (`Budgeted::used().read` ≤ `0x80 + 64*32 + 64 KiB`); `0x1200` → `ArtLocator::Range`; an encrypted `0x1200` → no locator; budgets on the largest fixture.
  - `tests/consoles_current_gen.rs`: `EP0002-CUSA04447_00-….pkg` in a pile → `ps4/` (as is) with `ExtractFirst`, `content_id` set, region Europe on the console id; a PS3 `7F PKG` still gets `InstallFirst { RPCS3 }`, not `ExtractFirst`; a macOS `xar!` `.pkg` is still `NotARom::Installer`.
  - Harness row: `pkg` CNT header → `ps4`.
- [ ] **Step 2: Run** `cargo test -p coverly-core --lib consoles::orbis && cargo test -p coverly-core --test consoles_current_gen --test organize_evidence --test systems_batch3` — expect failures.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** Step 2, G-RUST.
- [ ] **Step 5: Commit and push:** `git add -A crates && git commit -m "feat(core): catalogue PS4 packages from their plaintext CNT header" && git push && git status -sb`

---

## Task 6: PS5: `param.json` folders, store PKG, `.ffpkg`/`.ffpfs`/`.ffpfsc`/`.exfat`, app archives

**Files:**
- Modify: `consoles/orbis.rs` (`param_json`, `fih_pkg`, `ufs2`, `exfat_ps5`), `consoles/sony.rs` (`probe_sce_app`'s `param.json` branch), `consoles/testing.rs` (`ps5_param_json`, `ps5_app`, `fih_pkg`, `ufs2_image`, `exfat_image`), `identify/sniff.rs` (FIH magic, UFS2 at `0x1055C`, exFAT OEM name — only through `consoles::identify`, which already sees `ext` and `name`), `organize/evidence.rs` and `organize/plan.rs` folder entries (`NoEmulator` for `ps5` items whose payload detail is `FIH_DETAIL` or `EXFAT_DETAIL`, or whose extension is `ffpfs`/`ffpfsc`; `ProEnhanced` for a `ps5` item whose `ConsoleId.detail == PARAM_JSON_PRO_DETAIL` — the Pro flag rides on the fixed `detail` vocabulary, so `ConsoleId` gains no field for it), `organize/inspect.rs` (archive app check for PS5, D13)
- Test: `orbis.rs` unit tests, `tests/consoles_current_gen.rs`, `tests/organize_inspect.rs`, `tests/systems_batch3.rs`

**Verify first (D10).** FreeBSD `sys/ufs/ffs/fs.h`: `SBLOCK_UFS2 = 65536`, `fs_magic` offset `0x55C` in `struct fs`, `FS_UFS2_MAGIC = 0x19540119`. Record the revision. If unconfirmed: `.ffpkg` by extension only, `BLOCKED: UFS2 superblock offset — <why>`.

**`param.json`** (KytyPS5 `systemContent.cpp` at `893b0355`; facts only):

```rust
pub const PARAM_JSON_DETAIL: &str = "sce_sys/param.json";
pub const PARAM_JSON_PRO_DETAIL: &str = "sce_sys/param.json, PS5 Pro";
const TRINITY: i64 = 1 << 22;

/// A PS5 `param.json` (≤ MAX_META_FILE, UTF-8, BOM stripped). Fields:
/// `titleId`, `contentId`, `contentVersion` else `appVersion`,
/// `attribute3` (integer only), `localizedParameters.defaultLanguage` and
/// `<lang>.titleName` (default language, else `en-US`, else the first
/// language object). Kind: `originContentVersion`/`targetContentVersion`
/// → Update; else Base or Dlc by `has_eboot` (D6).
pub fn param_json(bytes: &[u8], has_eboot: bool) -> Option<ConsoleId> {
    if bytes.len() as u64 > MAX_META_FILE { return None; }
    let text = std::str::from_utf8(bytes).ok()?.trim_start_matches('\u{feff}');
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    let tid = v.get("titleId")?.as_str().and_then(title_id)?;
    let s = |k: &str| v.get(k).and_then(serde_json::Value::as_str);
    let pro = v.get("attribute3").and_then(serde_json::Value::as_i64).is_some_and(|a| a & TRINITY != 0);
    let mut id = ConsoleId::proven("ps5", if pro { PARAM_JSON_PRO_DETAIL } else { PARAM_JSON_DETAIL });
    id.id_source = Some(IdSource::ParamJson);
    id.content_id = s("contentId").filter(|c| c.len() == 36 && c.is_ascii()).map(str::to_owned);
    id.region = id.content_id.as_deref().and_then(super::region_from_content_id);
    id.version = s("contentVersion").or_else(|| s("appVersion")).and_then(super::clean_title);
    let langs = v.get("localizedParameters").and_then(serde_json::Value::as_object);
    id.title = langs.and_then(|l| {
        let default = l.get("defaultLanguage").and_then(serde_json::Value::as_str);
        let name = |lang: &str| l.get(lang)?.get("titleName")?.as_str();
        default.and_then(name).or_else(|| name("en-US"))
            .or_else(|| l.values().find_map(|o| o.get("titleName")?.as_str()))
    }).and_then(super::clean_title);
    id.kind = if v.get("originContentVersion").is_some() || v.get("targetContentVersion").is_some() {
        TitleKind::Update
    } else if has_eboot {
        TitleKind::Base
    } else {
        TitleKind::Dlc
    };
    id.title_id = Some(tid);
    Some(id)
}
```

`probe_sce_app` / `sce_matches`: `sce_sys` listed once; `param.json` → `param_json(read, find(l.names, "eboot.bin").is_some())` → `ps5` match; art `sce_sys/icon0.png`. Both `param.sfo` and `param.json` → two matches (conflict). Invalid/oversize JSON → no PS5 match (and no PSF → the folder is walked normally).

**Store PKG (D10):** `FIH_MAGIC = [0x7F, b'F', b'I', b'H']`; CNT offset = LE u64 at FIH+0x58, checked `< len` and `+ 0x80 <= len`; read `0x80` bytes there; must start `CNT_MAGIC`; content id (CNT+0x40), content type (CNT+0x74); `system = "ps5"`; detail `FIH_DETAIL`; **no entry table read**. `.exfat`: `ext == "exfat"`, bytes 3..11 == `b"EXFAT   "`, and `name` has a `PPSA\d{5}` token → `ps5` magic verdict, detail `EXFAT_DETAIL`, title id from the token (`IdSource::FileName`); without a token → `None` (the `ps5` folder still accepts it as weak). `.ffpkg`: 4 bytes LE at `0x1055C` == `0x19540119` → `ps5` proven, detail `UFS2_DETAIL`; else extension evidence only. `.ffpfs`/`.ffpfsc`: nothing read.

**App archives (D13):** in `organize/inspect.rs`, where an archive's entries are listed, a zip/7z/rar whose entries hold `eboot.bin` and `sce_sys/param.json` at the root or under one top folder → read the `param.json` entry (≤ 64 KiB decoded, existing archive budgets) → `orbis::param_json(bytes, true)` → payload sniff `console`; the profile's `ps5` container override (Task 3) keeps it `Transfer::AsIs`. A RAR the reader cannot open stays name-only. The same shape with `param.sfo` keeps batch 2's behaviour (no change; add a regression test).

- [ ] **Step 1: Failing tests.**
  - `orbis.rs`: `param_json` fields; default language; `en-US` fallback; first-language fallback; BOM; invalid JSON; 64 KiB + 1 → `None`; `attribute3` bit 22 → Pro detail; `attribute3` as a string → no Pro; `targetContentVersion` → Update; no eboot → Dlc. `fih_pkg`: content id, type, `fih_entry_table_is_never_read` (`RangeLog` over a fixture that has bytes where an entry table would be: only `0..0x60` and the CNT header range are touched); CNT offset past the end → `None`. `ufs2` true/false; `exfat_ps5` with/without token.
  - `tests/consoles_current_gen.rs`: `PPSA01284/` (eboot + param.json with Pro bit) → `ps5/PPSA01284` with `ProEnhanced` and the `NoFolderOnDevice` note on ES-DE; `PPSA01284 DLC/` (no eboot) → Dlc noted «Contenido descargable de …» grouped after its base; `UP9000-PPSA01284_00-….pkg` → `ps5/` with `NoEmulator`; `PPSA09519.exfat` → `ps5/` with `NoEmulator`; `game.ffpkg` with the UFS2 magic → `ps5/`, no `NoEmulator`; `x.ffpfs` → `ps5/` with `NoEmulator`; `PPSA01284.zip` holding `PPSA01284/eboot.bin` + `PPSA01284/sce_sys/param.json` → `ps5` archive item, `Transfer::AsIs`; a zip of a PS4 app folder is unchanged from batch 2.
- [ ] **Step 2: Run** `cargo test -p coverly-core --lib consoles::orbis consoles::sony && cargo test -p coverly-core --test consoles_current_gen --test organize_inspect --test systems_batch3` — expect failures.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** Step 2, G-RUST.
- [ ] **Step 5: Commit and push:** `git add -A crates && git commit -m "feat(core): identify PS5 app folders, packages, images and app archives" && git push && git status -sb`

---

## Task 7: `.zar` classified by its tree (Xbox 360, PS4, PS5) without the magic-only fallback

**Files:**
- Modify: `crates/coverly-core/src/consoles/zarchive.rs` (`classify`), `crates/coverly-core/src/consoles/xbox.rs` (`zar` drops `magic`; dispatches PS4/PS5 to the PSF/param.json readers), `crates/coverly-core/src/identify/sniff.rs` (ZArchive footer as a structure-only note, no system, if a sniff note type exists — `grep -n "ZArchive" crates/coverly-core/src/identify/sniff.rs`), `consoles/testing.rs` (reuse `zarchive(files)`)
- Test: `zarchive.rs`/`xbox.rs` unit tests, `tests/consoles_current_gen.rs`, `tests/systems_batch2.rs` (the `x360_zar` row keeps its `default.xex` verdict), `tests/systems_batch3.rs`

```rust
/// What a ZArchive's tree says (spec D12), from names only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZarKind { Xbox360, Ps4, Ps4Dlc, Ps5, Ambiguous, Nothing }

/// `default.xex` at the root → Xbox360; `eboot.bin` + `sce_sys/param.sfo`
/// → Ps4; `eboot.bin` + `sce_sys/param.json` → Ps5; `sce_sys/param.sfo`
/// without `eboot.bin` → Ps4Dlc; two of them → Ambiguous; none → Nothing.
pub fn classify(archive: &Archive) -> ZarKind;
```

`xbox::zar(b, ext)`: no footer → `None`; footer but tree unreadable or over budget → `None` (no verdict: the Shared extension sends it to review among three); `Xbox360` → today's `default.xex` path (its failure → `None`, not magic); `Ps4`/`Ps4Dlc` → `read_file(sce_sys/param.sfo, MAX_FILE_DECODED)` → `sony::from_psf(…, "ps4", "ZAR sce_sys/param.sfo")` (`Ps4Dlc` forces `Dlc` when the PSF has no category); `Ps5` → `orbis::param_json(bytes, true)`, detail `"ZAR sce_sys/param.json"`; `Ambiguous`/`Nothing` → `None`. `wiiu::wua` is untouched.

- [ ] **Step 1: Failing tests.** `classify` over five trees (360, PS4, PS4 DLC, PS5, mixed `default.xex` + `eboot.bin`); `zar_without_default_xex_has_no_verdict` (was `xbox360` magic); PS4 zar → `ps4` with title id; PS5 zar → `ps5`; over-budget tree → `None`; integration: `Gravity Rush.zar` (PS4 tree) → `ps4/` Auto; a bare `.zar` with an unreadable tree → Review with alternatives `ps4`, `ps5`, `xbox360`; a `.zar` inside `xbox360/` stays «ya ordenado»; `.wua` → `wiiu` (existing tests stay green).
- [ ] **Step 2: Run** `cargo test -p coverly-core --lib consoles::zarchive consoles::xbox consoles::wiiu && cargo test -p coverly-core --test consoles_current_gen --test systems_batch2 --test systems_batch3` — expect failures.
- [ ] **Step 3: Implement.** Commit body: `Behaviour change: a .zar without a readable default.xex goes to review instead of xbox360 (spec D12).`
- [ ] **Step 4: Run** Step 2, G-RUST.
- [ ] **Step 5: Commit and push:** `git add -A crates && git commit -m "feat(core): classify ZArchive files by their tree for Xbox 360, PS4 and PS5" && git push && git status -sb`

---

## Task 8: Walk: probe leave outcome, shadPS4 data, overlay siblings left in place, zero-byte exception

**Files:**
- Modify: `crates/coverly-core/src/organize/walk.rs`, `crates/coverly-core/src/organize/plan.rs` (`GameOverlay.of` → base title fix-up, P-3; `folder_entries` zero-byte rule per system)
- Test: `tests/organize_walk.rs`, `tests/organize_folder_games.rs`, `tests/consoles_current_gen.rs`

**Changes:**

1. `identify_dir` hit with `found.leave = Some(FolderLeave::NotARom(what))` → `out.left.push((path, LeftReason::NotARom { what }))`, not descended; `Some(FolderLeave::Unreadable)` → `LeftReason::Unreadable`, not descended. Tested here with `consoles::set_extra_probe` (a probe returning a `leave` for a folder holding `PC.PROBE`); the real users arrive in Task 10.
2. **shadPS4 user dir (D25):** `is_emulator_data_root` also returns true for a folder holding both `sys_modules/` and `game_data/` directories (`symlink_metadata`, case-insensitive names from the listing), and for a folder named `sys_modules` (any case). Reason `EmulatorData { folder: <name> }`.
3. **Overlay siblings (D11 d):** before probing, a folder named `<base><suffix>` with `suffix ∈ {-mods, -UPDATE, -patch}` (ASCII case-insensitive), whose sibling `<parent>/<base>` is a directory holding `sce_sys/` (one `symlink_metadata` each): `-mods` → left `GameOverlay { of: base }`, not descended; `-UPDATE`/`-patch` → probe first; no probe hit (no readable `param.sfo`) → left `GameOverlay { of: base }`, not descended; a probe hit → ordinary folder game (Task 9 pairs it). A `-mods` folder with no base beside it is walked normally.
4. **Zero-byte exception (D24):** `tree_summary` records `zero_bytes: bool` separately from `partial` (`partial` keeps `.part`/`.crdownload`); `folder_entries` treats `zero_bytes` as `PartialDownload` except when the folder game's system is `ps4`, `ps5`, `xboxone` or `xboxseries` (`const ZERO_BYTE_OK: [&str; 4]`). `TreeSummary` is serialised in facts: add the field with `#[serde(default)]`.
5. **P-3 fix-up** in the planner: for each item left `GameOverlay { of }` whose sibling base folder (same parent, name `of`) is an item with a `ConsoleId` title, replace `of` with `base_title(...)`.

- [ ] **Step 1: Failing tests.**
  - `organize_walk.rs`: `shadps4_user_dir_is_emulator_data` (`shadPS4/{sys_modules,game_data,addcont/CUSA00900/x}`) — one left item, nothing under it in candidates or folder games; `loose_sys_modules_is_emulator_data`; `mods_beside_base_is_left_not_descended` (`CUSA00900/` + `CUSA00900-mods/eboot.bin`, `CUSA00900-mods/x.bin`): the open log has no file under `-mods`, no candidate under it, left `GameOverlay { of: "CUSA00900" }` from the walk; `update_without_param_sfo_beside_base_is_left`; `mods_without_base_is_walked`; `probe_leave_not_a_rom_is_one_left_item` and `probe_leave_unreadable` (extra probe).
  - `organize_folder_games.rs`: `ps4_folder_with_zero_byte_file_moves` (`sce_sys/…` + empty `.ps4` marker + empty `x.dat`); `ps2_style_folder_with_zero_byte_file_is_partial` (any batch 2 folder game, e.g. `ps3_jb` with an empty file → `PartialDownload` as before); `.part` file in a PS4 folder still `PartialDownload`.
  - Plan: `game_overlay_names_the_base_title` (`CUSA00900` with `TITLE` «Bloodborne» → the `-mods` row says `of: "Bloodborne"`).
- [ ] **Step 2: Run** `cargo test -p coverly-core --test organize_walk --test organize_folder_games --test consoles_current_gen --lib organize::walk` — expect failures.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** Step 2, G-RUST.
- [ ] **Step 5: Commit and push:** `git add -A crates && git commit -m "feat(core): leave shadPS4 data and overlay folders in place, and allow empty files in console dumps" && git push && git status -sb`

---

## Task 9: Sibling overlays follow their base (name and action) and the Batocera DLC path

**Files:**
- Modify: `crates/coverly-core/src/organize/target.rs` (`overlay_pairs`, `siblings_follow`, `sibling_targets`, `dlc_target`), `crates/coverly-core/src/organize/plan.rs` (call order, P-4; `ItemNote::DlcFolder`)
- Create: `crates/coverly-core/tests/organize_overlays.rs`

**Interfaces:**

```rust
/// A shadPS4 overlay folder and its base, both items of the plan (D11):
/// same parent directory, `sibling` named `<base name><suffix>` with
/// `suffix` one of `-UPDATE`/`-patch` (ASCII case-insensitive, `suffix`
/// keeps the source's spelling), the sibling's console kind `Update`.
pub struct OverlayPair { pub base: usize, pub sibling: usize, pub suffix: String }

pub fn overlay_pairs(items: &[OrganizeItem]) -> Vec<OverlayPair>;

/// D11 (e): after the user's decisions, a sibling takes its base's action:
/// base left (any reason) → sibling `Left(GameOverlay { of: <base title> })`;
/// base in review → sibling in review with the base's system and
/// alternatives; base moved → sibling keeps its own action (Auto or
/// Review by its own evidence; a sibling Review never blocks the base).
pub fn siblings_follow(items: &mut [OrganizeItem], pairs: &[OverlayPair], titles: &dyn Fn(usize) -> String);

/// D11 (b): after targets, each sibling's target is
/// `<dir of base target>/<base final file name><suffix>`; a sibling whose
/// name is taken (on disk or by another item) leaves base and sibling
/// `CollisionSet`. A sibling with no base item keeps its own target (c).
pub fn sibling_targets(items: &mut [OrganizeItem], pairs: &[OverlayPair], layout: &TargetLayout);
```

**Batocera DLC (D16):** in target assignment, a `ps4` item of kind `Dlc` whose `ItemKind` is `Folder`, or a `.zar` file, under a profile with `dlc_folder["ps4"]`, goes to `<ps4 folder>/<dlc_folder with {title_id} = base title id>/<its own name>` (no folder suffix on DLC: only the base gets `.ps4`), note `DlcFolder { layout: profile display name }`. No title id → stays in `ps4/` with the plain DLC note. A DLC PKG stays in `ps4/` with `ExtractFirst`. Path-too-long and FAT gates apply per contained file as today.

Plan order in `plan_from_survey`: decisions loop → `let pairs = overlay_pairs(&items); siblings_follow(&mut items, &pairs, …)` → duplicates → transfers → targets (with DLC path) → `sibling_targets(&mut items, &pairs, &layout)` → notes.

- [ ] **Step 1: Failing tests** (`organize_overlays.rs`, pinned mtimes, sizes small):
  - Batocera: `CUSA00900` → `ps4/CUSA00900.ps4`, `CUSA00900-UPDATE` → `ps4/CUSA00900.ps4-UPDATE`, DLC folder (`CATEGORY ac`, title id `CUSA00900`) → `ps4/DLC/CUSA00900/<its name>` with `DlcFolder { layout: "Batocera / Knulli / EmulationStation" }`; ES-DE: `ps4/CUSA00900`, `ps4/CUSA00900-UPDATE`, DLC beside in `ps4/` with the `Dlc` note.
  - Collision: `ps4/CUSA00900` exists at the target → base `CUSA00900 (2)`, sibling `CUSA00900 (2)-UPDATE`; the sibling name also exists → both `CollisionSet`.
  - Base in review (`CUSA` + `PSP2_SYSTEM_VER`) → sibling in review with the same alternatives; base left `RecentlyModified` (touch one file inside with "now"-relative mtime via `filetime` pinned to `opts.now`) → sibling left `GameOverlay { of: "Bloodborne" }`; user `Leave` decision on the base → sibling left; user `Assign ps4` on the base → sibling moves.
  - Lower-case `CUSA00900-update` → target ends in `-update`.
  - Both `-UPDATE` and `-patch` present → both move with derived names.
  - Sibling without base → keeps its own name, note «Actualización de CUSA00900».
  - Custom profile without `dlc_folder` → DLC beside the game.
  - Batocera DLC PKG → `ps4/` with `ExtractFirst`, no `DlcFolder`.
- [ ] **Step 2: Run** `cargo test -p coverly-core --test organize_overlays --test organize_target --test organize_plan` — expect failures.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** Step 2, G-RUST.
- [ ] **Step 5: Commit and push:** `git add -A crates && git commit -m "feat(core): move shadPS4 update folders with their base and place Batocera DLC" && git push && git status -sb`

---

## Task 10: Xbox One and Series folders: GDK `MicrosoftGame.config`, ERA `AppxManifest.xml`, Scarlett rule, PC builds, logos

**Files:**
- Modify: `crates/coverly-core/src/consoles/xbox.rs` (`probe_gdk`, `probe_era`, `manifest_text`, `logo_locator`), `consoles/mod.rs` (`DIR_PROBES` += `xbox::probe_gdk`, `xbox::probe_era` after `probe_god`; `base_title_id` arms), `consoles/testing.rs` (`gdk_config`, `gdk_folder`, `era_manifest`, `era_folder`)
- Test: `xbox.rs` unit tests, `tests/consoles_current_gen.rs`, `tests/organize_evidence.rs`, `tests/systems_batch3.rs`

**Reading rules (D18, §8.4):**

```rust
/// A manifest's text: ≤ MAX_META_FILE, UTF-8 (BOM stripped) or UTF-16 LE/BE
/// with a BOM (`String::from_utf16`); anything else `None`. XML with a
/// DOCTYPE is rejected by the parser (as `wiiu::xml_fields`).
fn manifest_text(bytes: &[u8]) -> Option<String>;

/// Element/attribute walk over a manifest with quick-xml (no DTD, the five
/// XML entities only): returns the GDK fields of D18.
struct Gdk {
    name: Option<String>, version: Option<String>, title_id: Option<String>,
    store_id: Option<String>, msa_app_id: Option<String>, display_name: Option<String>,
    logos: Vec<String>,            // Square480x480Logo, StoreLogo, Square150x150Logo (in that order)
    families: Vec<String>,         // every ExecutableList/Executable/@TargetDeviceFamily
    dlc_family: Option<String>,    // <TargetDeviceFamilyForDLC> element, else attribute
    allowed_products: Vec<String>, // AllowedProducts/AllowedProduct
}
```

`probe_gdk(l)`: `MicrosoftGame.config` among the names (case-insensitive). Present but `plain_file`/read fails → `FolderMatch { leave: Some(Unreadable), id: ConsoleId::magic("xboxone", "MicrosoftGame.config") }`. Parsed:

| Families | Result |
|---|---|
| all `XboxOne` | `xboxone` Base |
| all `Scarlett` | `xboxseries` Base |
| `XboxOne` + `Scarlett` | two matches (conflict → review) — use the `Many` probe form from Task 4 |
| all `PC` | `leave: Some(NotARom(PcGameData))` |
| `PC` + one console family | that console system at confidence **0.60** (single-choice review) |
| none, DLC family `XboxOne`/`Scarlett`/`PC` | same table, kind `Dlc` |
| none, no DLC family | `xboxone` at 0.50 with alternative `xboxseries` (review) |

`XboxCompatibility` and `VirtualMachine/DlcCompatibility` are never read. `TitleId` `FFFFFFFF` (case-insensitive) → `None`; else 8 hex upper-cased → `title_id` (`IdSource::MicrosoftGameConfig`); `product_id = StoreId`; `parent_id` = the single `AllowedProduct` when exactly one; `version = Identity/@Version`; `title = DefaultDisplayName` unless `ms-resource:` (then folder name).

`probe_era(l)`: `AppxManifest.xml` whose root `Package` declares `http://schemas.microsoft.com/appx/2013/xbox/manifest` (any prefix) → `xboxone` Base; `product_id = Identity/@Name`, version `Identity/@Version`, title per D18 (`Properties/DisplayName` unless `ms-resource:`, else `VisualElements/@DisplayName` unless `ms-resource:`, else folder name), `IdSource::AppxManifest`. Without the namespace → `None`.

**Logos (D21):** `logo_locator(l, rel)`: split `rel` on `\` and `/`; reject empty, `..`, root, drive prefix, or more than 8 components; walk `symlink_metadata` component by component (a symlink → drop); exact file first; else one `read_dir` of the parent (≤ 256 entries) for the largest `<stem>.scale-<n>.png` (by `n`); → `ArtLocator::File { rel, format: Png }`. GDK: first of `Square480x480Logo`, `StoreLogo`, `Square150x150Logo` that resolves; ERA: `VisualElements/@Logo`, then `Properties/Logo`. Budget note: at most one extra listing per probe.

`base_title_id`: `xboxone | xboxseries` → `parent_id` for Dlc, `product_id` for Base (D15); ERA/XVC kinds without those → `None`.

- [ ] **Step 1: Failing tests.** `xbox.rs`: each row of the table; UTF-16 LE BOM config; UTF-16 without BOM → no hit; DOCTYPE → no hit; `XboxCompatibility="Scarlett"` + `XboxOne` executables → `xboxone`; `FFFFFFFF` ignored; one vs two `AllowedProduct`; DLC element and attribute forms; ERA with/without namespace; `ms-resource:` fallback chain; logo `..\..\secret.png`, `C:\x.png`, `\x.png` dropped; `Logo.png` stored as `Logo.scale-100.png` + `Logo.scale-200.png` → `scale-200`; a symlinked logo → dropped (`#[cfg(unix)]`). Integration: `Forza Horizon 5/` (Scarlett) → `xboxseries/`; `Halo 5 Guardians/` (XboxOne) → `xboxone/`; `Sunset Overdrive/` (ERA) → `xboxone/`; `Microsoft Flight Simulator/` (PC) → left «PC game data», not descended (open log has only the config); unreadable config (`#[cfg(unix)]` `chmod 000`, restored in a guard) → left `Unreadable`; `PC`+`XboxOne` → Review with `system_id xboxone`, no other alternative; budgets.
- [ ] **Step 2: Run** `cargo test -p coverly-core --lib consoles::xbox && cargo test -p coverly-core --test consoles_current_gen --test organize_evidence --test systems_batch3` — expect failures.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** Step 2, G-RUST.
- [ ] **Step 5: Commit and push:** `git add -A crates && git commit -m "feat(core): identify Xbox One and Series game folders from their manifests" && git push && git status -sb`

---

## Task 11: XVC/MSIXVC files and GUID-named candidates; `ConsoleSystem`; review between One and Series

**Files:**
- Modify: `crates/coverly-core/src/consoles/xbox.rs` (`xvd`), `crates/coverly-core/src/consoles/mod.rs` (`identify` unchanged order; `xbox::identify` adds the XVD check), `crates/coverly-core/src/organize/walk.rs` (GUID-named extensionless files ≥ 4 KiB → candidate with `ext = "xvc"`-like virtual ext `""` handled by inspect), `crates/coverly-core/src/organize/evidence.rs` (two equal header hits, `NoEmulator`), `consoles/testing.rs` (`xvd_head`)
- Test: `xbox.rs` unit tests, `tests/consoles_current_gen.rs`, `tests/organize_walk.rs`, `tests/organize_evidence.rs`

```rust
const XVD_MAGIC_AT: u64 = 0x200;
const XVD_MAGIC: &[u8; 8] = b"msft-xvd";
pub const XVD_DETAIL: &str = "XVD header";

/// `msft-xvd` at 0x200 (xvdtool struct + 0x200, research): VolumeFlags
/// u32 LE at 0x208, ContentId GUID at 0x220, ContentType u32 LE at 0x284,
/// ProductId GUID at 0x39C, PackageVersion 4 × u16 LE at 0x3BC. Reads
/// 0x3C4 bytes; nothing past the header (no XvcInfo). Returns one verdict
/// per system: the header cannot tell One from Series (D19).
pub fn xvd(b: &mut Budgeted) -> Option<Vec<ConsoleId>>;
```

ContentType: `0x01` → Base; `0x18 | 0x19 | 0x1A` → Dlc; `0x08 | 0x09 | 0x0A` → Unknown kind; anything else → a single verdict carrying the `ConsoleSystem` signal (`detail = XVD_SYSTEM_DETAIL`), which evidence turns into `LeftReason::NotARom { what: ConsoleSystem }`. GUIDs formatted `8-4-4-4-12` lower-case from mixed-endian bytes (Windows `GUID` layout: first three groups LE) — **check against xvdtool's `ToString` output in the research doc** before fixing the expected string.

Because `consoles::identify` returns one `ConsoleId`, add `consoles::identify_all(src, ext, name) -> Vec<ConsoleId>` used by `sniff` (one header hit per id) and keep `identify` = `identify_all(..).into_iter().next()` for dedupe/scan. Evidence: two `IdLevel::Header` hits of equal weight → `combine` cannot pass `MARGIN` → Review with `xboxone`, `xboxseries`; inside either system folder → «ya ordenado» (existing).

**Walk (D19):** a file with no extension whose name matches `^[0-9A-Fa-f]{8}-([0-9A-Fa-f]{4}-){3}[0-9A-Fa-f]{12}$`, size ≥ 4096, not key material (checked first, D4) → `WalkCandidate { ext: "".into(), .. }`; inspect peeks it through `Budgeted` with `MAX_READ` lowered to `0x208` for this candidate (assert `used().read <= 0x208` before reading the rest only when the magic matched — read the magic window `0x200..0x208` first, then the remaining header up to `0x3C4`). A GUID file without the magic → `other_files`.

- [ ] **Step 1: Failing tests.** `xvd` fields, each ContentType class, GUID formatting, truncated header → `None`, budget; `guid_file_peeks_only_the_magic_window` (`RangeLog`: a non-XVC GUID file touched only `0x200..0x208`); `guid_named_cache_blob_is_other_file`; integration: `4f0b3e0a-…-c1d2e3f4a5b6` → Review with alternatives `xboxone`, `xboxseries`, `NoEmulator` note, `content_id` = ContentId GUID; `game.xvc` under `xboxone/` → «ya ordenado»; ContentType `0x0B` → left «archivo del sistema de una consola».
- [ ] **Step 2: Run** `cargo test -p coverly-core --lib consoles::xbox && cargo test -p coverly-core --test consoles_current_gen --test organize_walk --test organize_evidence --test sniff` — expect failures.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** Step 2, G-RUST.
- [ ] **Step 5: Commit and push:** `git add -A crates && git commit -m "feat(core): catalogue Xbox XVC packages by their plaintext header" && git push && git status -sb`

---

## Task 12: Dedupe keys (content id, Xbox ids) and keeper region fallback

**Files:**
- Modify: `crates/coverly-core/src/dedupe/keys.rs` (`TITLE_ID_SYSTEMS` 9 → 13; `title_of`), `crates/coverly-core/src/dedupe/keeper.rs` (region from `ConsoleId.region` when the name has no region tag)
- Test: `tests/dedupe_keys.rs`, `tests/dedupe_keeper.rs`, `tests/dedupe_units.rs`

```rust
pub const TITLE_ID_SYSTEMS: [&str; 13] = [
    "gamecube", "wii", "wiiu", "3ds", "switch", "ps3", "psvita", "xbox", "xbox360",
    "ps4", "ps5", "xboxone", "xboxseries",
];

fn title_of(id: &ConsoleId, system: &str) -> Option<ExactKey> {
    if id.system != system { return None; }
    let key_id = match system {
        // D14: same region and DLC label → same key.
        "ps4" | "ps5" => id.content_id.clone().or_else(|| id.title_id.clone()),
        // D14: GDK TitleId (FFFFFFFF already dropped by the reader), else
        // StoreId / Identity Name; XVC ContentId.
        "xboxone" | "xboxseries" => id.title_id.clone()
            .or_else(|| id.product_id.clone())
            .or_else(|| id.content_id.clone()),
        _ => id.title_id.clone(),
    }?;
    let key_id = key_id.trim();
    if key_id.is_empty() { return None; }
    Some(ExactKey::Title { title_id: key_id.to_ascii_uppercase(), kind: id.kind, version: id.version.clone(), disc: id.disc_no })
}
```

Bump `TITLE_NS` to `dedupe-title-v2` (keys change shape for cached entries). Folder-game duplicates stay reported only (batch 2 D28) — no change needed beyond a test.

- [ ] **Step 1: Failing tests.** Same content id + kind + version → one duplicate group, reported only for folders; two DLC labels of `CUSA00900` → not duplicates; `UP…` vs `EP…` of the same `CUSA` → not duplicates; base vs its update → not duplicates; GDK `FFFFFFFF` twins with the same `StoreId` → duplicates; two XVCs with the same ContentId GUID → duplicates (files: `ExactKey::Title` stays unconfirmed, report-only, batch 2 P-6); keeper: of two PS4 PKGs with no region tag in their names, the `UP` one wins under a `Usa`-first preference.
- [ ] **Step 2: Run** `cargo test -p coverly-core --test dedupe_keys --test dedupe_keeper --test dedupe_units` — expect failures.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** Step 2, G-RUST.
- [ ] **Step 5: Commit and push:** `git add -A crates && git commit -m "feat(core): key current-generation duplicates by content id and Xbox ids" && git push && git status -sb`

---

## Task 13: Covers: embedded first for the four systems, new locators, libretro PS4, ScreenScraper ids

**Files:**
- Modify: `crates/coverly-core/src/sources/embedded.rs` (`SYSTEMS` 5 → 9; `Range` locators for PS4 PKG `0x1200`), `crates/coverly-core/src/fill/mod.rs` (`pub fn source_order(ids: &[String], system: &System) -> Vec<String>`), `crates/coverly-core/src/plan/mod.rs` (`RankContext.sources` from `source_order`), `crates/coverly-core/tests/fill_sources.rs`, `crates/coverly-core/tests/systems_batch3.rs` (cover lookup rows)
- Test: `sources::embedded` unit tests, `tests/fill_sources.rs`, `tests/systems_batch3.rs`, `tests/libretro.rs`

```rust
/// The user's source order for `system` (plan P-5): `embedded` first when
/// the system has `embedded_first` and the user enabled it; otherwise
/// `ids` unchanged. Disabling `embedded` still disables it.
pub fn source_order(ids: &[String], system: &System) -> Vec<String> {
    if !system.embedded_first || !ids.iter().any(|i| i == embedded::ID) {
        return ids.to_vec();
    }
    std::iter::once(embedded::ID.to_string())
        .chain(ids.iter().filter(|i| *i != embedded::ID).cloned())
        .collect()
}
```

- [ ] **Step 1: Failing tests.** Embedded: PS4 folder `sce_sys/icon0.png` (512×512 synthetic PNG) → a Cover candidate; PS5 folder icon; GDK `Square480x480Logo`; ERA logo; PKG `0x1200` range; MRT `scale-200` resolved by the probe (Task 10) and read as a plain `File`; no locator is produced for `pic0.png`/`pic1.png` (assert `ConsoleId.art.len() == 1` for a folder that has all three); a non-PNG range → no candidate. Fill: with sources `[libretro, screenscraper, embedded]` a PS4 entry ranks the embedded candidate first, a SNES entry keeps libretro first; with `embedded` disabled a PS4 entry gets no embedded candidate. Harness cover rows: `ps4` libretro `Sony - PlayStation 4` is in the committed listing; `ps5`, `xboxone`, `xboxseries` have no libretro name; ScreenScraper ids `60`, `284`, `34`, none.
- [ ] **Step 2: Run** `cargo test -p coverly-core --lib sources::embedded fill:: && cargo test -p coverly-core --test fill_sources --test systems_batch3 --test libretro` — expect failures.
- [ ] **Step 3: Implement.** Before committing, check the libretro root live (`curl -s https://thumbnails.libretro.com/ | grep -c "PlayStation 4"`, read-only) and `Sony - PlayStation 4/Named_Boxarts/` count; record both in the commit body (spec D21 says 20 box arts on 2026-11-01). Do not refresh the fixture unless a folder is missing (P-6).
- [ ] **Step 4: Run** Step 2, G-RUST.
- [ ] **Step 5: Commit and push:** `git add -A crates && git commit -m "feat(core): try embedded covers first for PS4, PS5 and Xbox One and Series" && git push && git status -sb`

---

## Task 14: Size confirmation, back end: `large_transfer`, plan warning, IPC gate, CLI `--yes-large`

**Files:**
- Modify: `crates/coverly-core/src/organize/apply.rs` (`LARGE_TRANSFER`, `LargeTransfer`, `large_transfer`), `crates/coverly-core/src/organize/plan.rs` (`PlanWarning::LargeTransfer` at plan time), `src-tauri/src/state.rs` (gate after `decided_plan`), `src-tauri/src/ipc.rs` (`organize_apply(…, confirmed_bytes: Option<u64>)`), `src-tauri/src/error.rs` (`IpcError::LargeTransfer`), `crates/coverly-cli/src/organize.rs` (`--yes-large`)
- Create: `crates/coverly-core/tests/organize_large_transfer.rs`
- Test: `src-tauri/tests/organize_state.rs`, `crates/coverly-cli` tests (`grep -rn "fn .*no_space" crates/coverly-cli/tests` for the existing refusal test pattern)

```rust
/// Bytes from which Organizar asks before writing (D23): 10 GB, decimal.
pub const LARGE_TRANSFER: u64 = 10_000_000_000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LargeTransfer {
    pub bytes: u64,
    pub items: usize,
    /// Copy mode (or a move across volumes, which copies then removes).
    pub copies: bool,
    pub free: Option<u64>,
    /// The mounted volume's name holding the target (longest prefix in
    /// `volumes::mounted_volumes`), else the target folder's name.
    pub volume: String,
}

/// What the plan writes to the target, when it reaches LARGE_TRANSFER
/// (D23, `check_space`'s rule): items that move in, not set aside, in copy
/// mode or across volumes, plus extracted bytes. A same-volume move is a
/// rename and counts nothing.
pub fn large_transfer(plan: &OrganizePlan) -> Option<LargeTransfer> {
    let crosses = plan.mode == TransferMode::Copy || !same_volume(&plan.source_root, &plan.target_root);
    let writing: Vec<&OrganizeItem> = plan.items.iter()
        .filter(|i| i.moves_in() && !i.is_set_aside()).collect();
    let bytes: u64 = writing.iter().map(|i| match &i.transfer {
        Transfer::Extract { bytes, .. } => *bytes,
        Transfer::AsIs if crosses => i.size,
        Transfer::AsIs => 0,
    }).fold(0u64, u64::saturating_add);
    (bytes >= LARGE_TRANSFER).then(|| LargeTransfer {
        bytes,
        items: writing.iter().filter(|i| crosses || matches!(i.transfer, Transfer::Extract { .. })).count(),
        copies: plan.mode == TransferMode::Copy,
        free: user_available(&plan.target_root),
        volume: volume_name(&plan.target_root),
    })
}
```

Check `Transfer::Extract.bytes` is the extracted total (`grep -n "bytes: u64" crates/coverly-core/src/organize/plan.rs`) and adjust. Plan time: after targets, `if let Some(t) = large_transfer(&plan) { warnings.push(PlanWarning::LargeTransfer { bytes: t.bytes, items: t.items }) }` (over default actions: Review items count as moving because `moves_in()` includes Review — the warning is informational).

Gate (`State::organize_apply`, after `decided_plan`, before `organize_guard`):

```rust
if let Some(t) = coverly_core::organize::large_transfer(&plan) {
    if confirmed_bytes.is_none_or(|c| c < t.bytes) {
        return Err(IpcError::LargeTransfer { bytes: t.bytes, items: t.items, copies: t.copies, free: t.free, volume: t.volume });
    }
}
```

`IpcError::LargeTransfer` is a struct variant: with `#[serde(tag = "code", content = "detail")]` it serialises as `{"code":"large-transfer","detail":{"bytes":…,"items":…,"copies":…,"free":…,"volume":"…"}}`; `#[error("…")]` text `would write {bytes} bytes to {volume}`. `organize_resume` is unchanged (no gate).

CLI: `#[arg(long, requires = "apply")] yes_large: bool`; after planning in apply mode and before `apply_organize`, `large_transfer(&plan)` without `--yes-large` → print `would write {bytes} bytes (≥ 10 GB) to {volume}; re-run with --yes-large to confirm; nothing was written` and return the same `Failure` exit code as the no-space refusal. `--resume` needs no flag.

- [ ] **Step 1: Failing tests.**
  - `organize_large_transfer.rs` (sizes faked through a test `MetaProbe` whose `file_len` returns 12 GB for one folder's file; `set_free_space` hook for `free`; source and target on the same tempdir volume, so force `TransferMode::Copy` for the cross-write cases and `Move` for the rename case): copy mode 12 GB → `Some` with `copies: true`, `items: 1`; move on the same volume → `None`; set-aside duplicate excluded; an extracted 11 GB archive counts in move mode; 9.99 GB → `None`; plan warning present; a review choice (`Assign`) that adds a 6 GB item to a 5 GB plan → `None` before, `Some` on the decided plan.
  - `src-tauri/tests/organize_state.rs`: apply without `confirmed_bytes` → `Err(IpcError::LargeTransfer { .. })` with the decided numbers and nothing written (target tree unchanged); with `confirmed_bytes = bytes - 1` → refused; with `bytes` → the job starts; resume never asks; the error JSON shape.
  - CLI: over 10 GB without `--yes-large` writes nothing and exits like no-space; with it applies; `--resume` runs without it.
- [ ] **Step 2: Run** `cargo test -p coverly-core --test organize_large_transfer && cargo test -p coverly-roms --test organize_state && cargo test -p coverly-cli` — expect failures.
- [ ] **Step 3: Implement.** Commit body: `Behaviour change: coverly organize --apply refuses to write 10 GB or more without --yes-large (spec D23).`
- [ ] **Step 4: Run** Step 2, G-RUST, G-WEB (IPC signature).
- [ ] **Step 5: Commit and push:** `git add -A crates src-tauri && git commit -m "feat(app): ask before Organizar writes 10 GB or more" && git push && git status -sb`

---

## Task 15: Size confirmation, front end: backend signature, mock, store flow, dialog

- [ ] **Step 0: Sync.** `git fetch origin && git merge origin/main`; resolve conflicts by hand (locale files key by key), run G-RUST and G-WEB, commit the merge (`git commit --no-edit`), `git push`.

**Files:**
- Modify: `src/ipc/backend.ts` (`organizeApply(planId: number, decisions: OrganizeDecision[], confirmedBytes?: number)`), `src/ipc/tauriBackend.ts` (`confirmedBytes ?? null` → `confirmed_bytes`), `src/ipc/types.ts` (`BackendError` code `large-transfer` with `LargeTransferDetail { bytes; items; copies; free: number | null; volume }`), `src/ipc/mock/mockOrganize.ts` (raise `large-transfer` like the backend when the decided plan writes ≥ 10 GB and `confirmedBytes` is missing or smaller), `src/stores/organizeStore.ts` (`pendingLarge: LargeTransferDetail | null`, `confirmApply(bytes)`, `dismissLarge()`), `src/screens/OrganizeScreen.tsx` (warning line; dialog when `pendingLarge`), `locales/{es,en}/ui.json` (`organize.largeTransfer.*`, `errors.large-transfer`, spec §10)
- Create: `src/components/organize/LargeTransferDialog.tsx`, `src/components/organize/LargeTransferDialog.test.tsx`
- Test: `src/stores/organizeStore.test.ts`, `src/ipc/mock/mockOrganize.test.ts`

```tsx
// LargeTransferDialog.tsx — built on src/components/ui/alert-dialog.tsx
interface Props {
  detail: LargeTransferDetail;
  onCancel(): void;
  onConfirm(bytes: number): void;
}
// Title: t(detail.copies ? 'organize.largeTransfer.title' : 'organize.largeTransfer.titleMove',
//          { bytes: <span className="font-mono">{formatDecimalSize(detail.bytes)}</span> })
// Body: body | bodyOne | bodyNoFree | bodyNoFreeOne by (items === 1, free === null),
//        {volume} plain text, {free} in mono.
// Footer: AlertDialogCancel (initial focus) «Cancelar»; AlertDialogAction
//         confirm | confirmMove with the size in mono.
// Escape and Cancel → onCancel; focus returns to the apply button (Radix
// returns focus to the trigger; pass the apply button ref as the return target).
```

Store: `apply()` catches a `BackendError` with `code === 'large-transfer'`, sets `status` back to `'ready'` and `pendingLarge = detail`; `confirmApply(bytes)` clears `pendingLarge` and calls `run(b => b.organizeApply(planId, decisions, bytes))` once (guard with a module-level `confirming` flag); a second click while the call is pending sends nothing; the apply button shows the existing busy state with `aria-busy` while the first call answers.

- [ ] **Step 1: Failing tests.** Dialog: es title «¿Copiar 187,42 GB?» and button «Copiar 187,42 GB» (`187.42 GB` en); move titles «¿Mover 187,42 GB a otro disco?»; `bodyOne` at one item; `bodyNoFree` when `free` is null; Escape and «Cancelar» call `onCancel`, never `onConfirm`; confirm calls `onConfirm(detail.bytes)`; focus starts on «Cancelar». Store: a `large-transfer` error leaves `applying`, holds `pendingLarge`; `confirmApply` resends once with `confirmedBytes`; a double call sends one request. Mock: the `current` pile (Task 18 adds it; for now a synthetic 12 GB plan) raises then applies.
- [ ] **Step 2: Run** `pnpm vitest run src/components/organize/LargeTransferDialog.test.tsx src/stores/organizeStore.test.ts src/ipc/mock/mockOrganize.test.ts` — expect failures.
- [ ] **Step 3: Implement** with frontend-design:frontend-design (spec §9.1: dialog title in the UI face at the existing dialog title size, mono only for sizes, `--cr-primary` on the confirm button only, no new token). Screenshot the dialog in light and dark at 1280 px and critique it before committing.
- [ ] **Step 4: Run** Step 2, G-WEB.
- [ ] **Step 5: Commit and push:** `git add -A src locales && git commit -m "feat(ui): confirm large Organizar transfers with the exact size" && git push && git status -sb`

---

## Task 16: Batch plan snapshots over the current-generation pile

**Files:**
- Modify: `crates/coverly-core/tests/organize_plan.rs` (or the file holding `organize_plan__systems_batch_2__*` — `grep -rln "systems_batch_2__" crates/coverly-core/tests`), `crates/coverly-core/src/organize/testing.rs` or the existing pile builder (`grep -rn "fn modern_pile\|batch_2_pile" crates/coverly-core`)
- Create: `crates/coverly-core/tests/snapshots/organize_plan__systems_batch_3__esde.snap`, `…__batocera.snap`

The pile is spec §9.3 built from the Task 4–11 builders into a `tempfile` dir (synthetic content; sizes faked through the `MetaProbe` hook: `CUSA00900` 48,12 GB, `-UPDATE` 14,80 GB, the PS4 PKG 9,02 GB, Forza 110,40 GB), pinned mtimes, copy mode so `LargeTransfer` shows:
- `CUSA00900/`, `CUSA00900-UPDATE/`, `CUSA00900-mods/`, a DLC label folder `CUSA00900-DLC-ABC/` (`CATEGORY ac`);
- `EP0002-CUSA04447_00-GRAVITYRUSH00000.pkg`, `Gravity Rush.zar`;
- `PPSA01284/`, `PPSA01284 DLC/`, `UP9000-PPSA01284_00-RATCHET000000000.pkg`, `PPSA09519.exfat`;
- `Forza Horizon 5/`, `Halo 5 Guardians/`, `Sunset Overdrive/`, `Microsoft Flight Simulator/`, `4f0b3e0a-1a2b-4c3d-8e9f-c1d2e3f4a5b6`;
- `shadPS4/{sys_modules,game_data,addcont}`, `UP9000-CUSA00900_00.rif`, `x.cik`.

- [ ] **Step 1: Write the snapshot tests** (`insta::assert_json_snapshot!` with the existing redactions) for ES-DE and Batocera; run and review every line of the new `.snap` against spec §9.1/§9.3 (targets, notes, left reasons, review alternatives, `large-transfer` warning, order: updates and DLC right after their base). Also run the batch 1 and 2 snapshot tests and confirm **no** change in them (`git diff --stat crates/coverly-core/tests/snapshots` lists only the two new files).
- [ ] **Step 2: Run** `cargo test -p coverly-core --test organize_plan` (or the file found above) — accept only after review (`cargo insta review`).
- [ ] **Step 3: Run** G-RUST.
- [ ] **Step 4: Commit and push:** `git add -A crates && git commit -m "test(core): snapshot the current-generation organise pile on ES-DE and Batocera" && git push && git status -sb`

---

## Task 17: Icons: three silhouettes, PS5 Slot mount, audit, notices

**Files:**
- Modify: `scripts/icons/systemIconsLib.mjs` (entries `ps4: 'Sony - PlayStation 4.svg'`, `xboxone: 'Microsoft - Xbox One.svg'`, `xboxseries: 'Non-Redump - Microsoft - Xbox Series X.svg'` with `depicts` and `audit` lines after looking at each file), `src/components/system/systemFallbacks.ts` (`FALLBACK_ABBR.ps5 = 'PS5'`; `WORDMARK_IDS` gains any of `ps4`/`xboxone`/`xboxseries` whose file the audit finds to be a wordmark or logo, with abbreviations `PS4`, `XB1`, `XBSX`), generated icon files (by `pnpm system-icons` only), `THIRD-PARTY-NOTICES.md` (file list), `src/components/system/SystemIcon.test.tsx`
- [ ] **Step 1: Audit.** `pnpm system-icons` in its audit/contact-sheet mode (see `node scripts/icons/systems.mjs --help`); open the contact sheet; each new file must be a device silhouette without a logo, symbol or brand mark (batch 1 D20). A file that shows a logo or wordmark goes to `WORDMARK_IDS` instead.
- [ ] **Step 2: Failing tests.** `SystemIcon` renders a silhouette (or the audited mount) for `ps4`, `xboxone`, `xboxseries`, and the Slot mount `PS5` for `ps5`.
- [ ] **Step 3: Run** `pnpm system-icons && pnpm vitest run src/components/system` — fix until green; never hand-edit generated files.
- [ ] **Step 4: Run** G-WEB.
- [ ] **Step 5: Commit and push:** `git add -A scripts src THIRD-PARTY-NOTICES.md && git commit -m "build(icons): add PS4, Xbox One and Xbox Series silhouettes and a PS5 mount" && git push && git status -sb`

---

## Task 18: UI: copy, «Sin emulador», why lines, mock `current` pile, copy guard and locale shape tests

- [ ] **Step 0: Sync.** `git fetch origin && git merge origin/main`; resolve conflicts by hand, run G-RUST and G-WEB, commit the merge, `git push`.

**Files:**
- Modify: `src/lib/organize.ts` (+ test) — copy for `game-overlay` (`organize.left.gameOverlay` with `{title}` = `of`), `console-system` (`NOT_A_ROM_KEYS['console-system'] = 'consoleSystem'`), the four notes (`dlc-folder` with `{layout}`), `why.contentId` from `OrganizeItem.content_id`, `ID_FILES` += `'param-json': 'param.json'`, `'microsoft-game-config': 'MicrosoftGame.config'`, `'appx-manifest': 'AppxManifest.xml'`; `src/components/organize/SystemGroups.tsx` (+ test) and `src/components/library/views/SectionHeaderRow.tsx` (+ test) — «Sin emulador» after the system name when `no_emulator` (caption size, secondary text, `·` separator `aria-hidden`, inside the trigger so it joins the accessible name; once per group/shelf, never per row); `src/screens/OrganizeScreen.tsx` — `large-transfer` warning line (`organize.largeTransfer.warning`, `{bytes}` in mono); `src/ipc/mock/mockOrganize.ts` (+ test) — `MOCK_CURRENT = '/Users/demo/Current'`, `?mockPick=current`, the §9.3 pile mirroring Task 16 (synthetic names and sizes, target on another volume «TinyExt» with a fixed `free` of 412,10 GB so `organizeApply` raises `large-transfer` for 187,42 GB), plus a same-volume variant for the «no dialog in move mode» E2E check (`?mockPick=current&mockSameVolume=1`); `src/ipc/mock/fixture.ts` (+ test) — shelves with `PS4` and `XBOXSERIES` entries; `locales/{es,en}/ui.json` — `systems.noEmulator` and any §10 key not added yet
- Create: `src/i18n/localeGuard.test.ts` (place it beside the existing parity test: `grep -rln "parity" src --include=*.test.ts`)

```ts
// localeGuard.test.ts
const BANNED = ['http', 'PCSX4', 'PCSX5', 'Rexbox', 'KytyPS5', 'SharpEmu', 'AnyPS5', 'Prosperity', 'shadPS4', 'WinDurango'];
it('locale values name no emulator and hold no URL', () => {
  for (const [lang, tree] of [['es', es], ['en', en]] as const)
    for (const [key, value] of leaves(tree))
      for (const word of BANNED)
        expect(value.toLowerCase().includes(word.toLowerCase()), `${lang}:${key} has ${word}`).toBe(false);
});
it('no key is both a string and an object', () => { /* walk both trees: a path is a leaf in one tree iff in the other, and never a prefix of another leaf */ });
```

Check first that no existing value contains `http` (`grep -n "http" locales/*/ui.json`); if one does (a legitimate link in another feature), scope the `http` rule to the keys this batch adds plus `organize.*` and `systems.*`, and say so in the test.

- [ ] **Step 1: Failing tests.** `organize.test.ts`: each new reason, note and why line in es/en exactly as §10; `titleIdFile` with the three new files; `SystemGroups`: «Sin emulador» only for `xboxseries`, part of the trigger's accessible name («xboxseries/ (1) Microsoft Xbox Series X|S Sin emulador»), separator `aria-hidden`; `SectionHeaderRow` likewise; `mockOrganize`: `?mockPick=current` resolves, produces the §9.3 rows and the `large-transfer` warning; the copy guard and shape tests; i18n parity and the Spanish copy check stay green.
- [ ] **Step 2: Run** `pnpm vitest run src/lib/organize.test.ts src/components/organize src/components/library/views src/ipc/mock src/i18n` — expect failures.
- [ ] **Step 3: Implement** with frontend-design:frontend-design (spec §9.1 layout and the rejected defaults: no pill, no triangle, no warning colour). Screenshot the plan screen with `?mockPick=current` in light and dark and critique it before committing.
- [ ] **Step 4: Run** Step 2, G-WEB.
- [ ] **Step 5: Commit and push:** `git add -A src locales && git commit -m "feat(ui): show current-generation consoles, their notes and the no-emulator label" && git push && git status -sb`

---

## Task 19: Verification, real-data check, E2E, changelog and PR

**Files:** `CHANGELOG.md` (Unreleased), `docs/superpowers/specs/2026-10-26-systems-batch-3.md` (status line only: «Implemented on `feat/systems-batch-3`»), screenshots and the PR body under the scratchpad (not committed).

- [ ] **Step 0: Sync** with `origin/main`; resolve conflicts by hand, run all gates, commit the merge, push.
- [ ] **Step 1: Changelog.** Under `## [Unreleased]`: **Added** — PS4, PS5, Xbox One and Xbox Series (catalogued, «no emulator»), embedded icons first for them, a confirmation before writing 10 GB or more. **Changed** — `.zar` files without a readable Xbox 360 executable go to review; `coverly organize --apply` needs `--yes-large` for 10 GB or more; empty files no longer hold back PS4/PS5/Xbox One/Series folder games; `.cik` files are treated as key material. English, plain words, no emulator names.
- [ ] **Step 2: Full gates** from a clean tree; keep the last line of each for the PR evidence:

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-systems-3 && pnpm check-all && pnpm build && cargo fmt --all --check && cargo clippy --workspace --all-targets --all-features -- -D warnings && scripts/dev/clippy-windows.sh && cargo test --workspace --no-fail-fast
```

- [ ] **Step 3: Real-data check, read-only.**
  - `cargo run -q -p coverly-cli -- systems list | wc -l` (or the CLI's equivalent per `cargo run -q -p coverly-cli -- --help`) shows 131 systems.
  - List any Tanda 3 material without writing: `find ~/Documents/ROMS /Volumes/TinyExt -maxdepth 4 \( -iname 'param.sfo' -path '*sce_sys*' -o -iname 'param.json' -o -iname '*.pkg' -o -iname '*.zar' -o -iname '*.ffpkg' -o -iname '*.exfat' -o -iname 'MicrosoftGame.config' -o -iname 'AppxManifest.xml' -o -iname '*.xvc' -o -iname '*.msixvc' \) 2>/dev/null | head -50`. If anything is found, run a **plan-only** `coverly organize <root>` (no `--apply`) and record the counts per system and any unexpected row. Never apply.
  - For a «verify» rule still open, copy one real head (`mkdir -p target/systems-sample && head -c 1048576 "<file>" > target/systems-sample/<name>`), run the reader against it in a local, uncommitted test, record the result, check `find target/systems-sample -name '._*' | wc -l` prints `0`, then `rm -rf target/systems-sample`.
- [ ] **Step 4: Browser E2E** (spec §14.3). Start `pnpm dev --port 1467 --strictPort` in the background. Chrome DevTools MCP `new_page` `http://localhost:1467/?mockPick=current` with `isolatedContext: "systems-batch-3"`; fall back to Playwright MCP, then `playwright-core` + Chrome for Testing in `/private/tmp/claude-501/-Users-miguelangelesparzacalero-Documents-GitHub/35b7c020-f2ae-412f-b757-99c24a9a498b/scratchpad/systems-batch-3`.
  1. Plan: PS4, PS5, Xbox One and Xbox Series groups with their icons; `CUSA00900` folder row with `48,12 GB`; `-UPDATE` right under it with «Actualización de Bloodborne»; the PS4 PKG with «Este paquete no se abre así: hay que extraerlo antes»; the PS5 store PKG and `.exfat` with «Ningún emulador lo abre así»; `PPSA01284` with «Mejorado para PS5 Pro»; «Content ID UP9000-CUSA00900_00-…» why line.
  2. Xbox Series heading shows «Sin emulador» (snapshot: part of the button's name); the XVC sits in review with Xbox One and Xbox Series as choices; Flight Simulator left «No es una ROM (datos de un juego de PC)».
  3. Left in place: `CUSA00900-mods` («Carpeta de mods o parches de Bloodborne: se deja donde está»), `shadPS4/` (emulator data), the `.rif` and `.cik` (key material).
  4. Layout Batocera: `CUSA00900.ps4`, `CUSA00900.ps4-UPDATE`, DLC under `ps4/DLC/CUSA00900/` with «Va a la carpeta DLC, donde Batocera / Knulli / EmulationStation busca el contenido descargable»; ES-DE: no suffix.
  5. Apply → dialog, keyboard only: Tab moves between «Cancelar» and the confirm button, `take_snapshot` shows an `alertdialog` named «¿Copiar 187,42 GB?», Escape cancels and focus is back on the apply button; Enter on the confirm button applies. `?mockPick=current&mockSameVolume=1` in move mode: no dialog.
  6. Library shelves with `PS4` and `XBOXSERIES` («Sin emulador» on the shelf header): light and dark (`data-theme`), `prefers-reduced-motion: reduce` emulated, 1280 px and the narrowest supported width.
  - `list_console_messages`: no errors. Screenshots to `/private/tmp/claude-501/-Users-miguelangelesparzacalero-Documents-GitHub/35b7c020-f2ae-412f-b757-99c24a9a498b/scratchpad/systems-batch-3/e2e/`. Stop the server: `lsof -ti tcp:1467 -sTCP:LISTEN | xargs -r kill`.
- [ ] **Step 5: Commit and push:** `git add CHANGELOG.md docs/superpowers/specs/2026-10-26-systems-batch-3.md && git commit -m "docs: changelog for systems batch 3" && git push && git status -sb`
- [ ] **Step 6: Open the PR** in English, no Claude/AI mention, no generated-by line, no emulator link:

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-systems-3 && gh pr create --base main --head feat/systems-batch-3 \
  --title "feat: add PS4, PS5, Xbox One and Xbox Series (systems batch 3)" \
  --body-file /private/tmp/claude-501/-Users-miguelangelesparzacalero-Documents-GitHub/35b7c020-f2ae-412f-b757-99c24a9a498b/scratchpad/systems-batch-3/pr-body.md
```

  The body (scratchpad, not the repo) has: Summary by system; what each reader reads in clear (spec §8.1–§8.3 tables); the D4 rule (keys, licences, encrypted entries, no emulator named) and the tests that guard it; behaviour changes (`.zar` review, `--yes-large`, zero-byte rule, `.cik`); folder sources (§6, §16) and the per-frontend mismatches (ES-DE `xboxone` placeholder lists only `.7z .zip`; no frontend has PS5 or Xbox Series; Batocera has no Xbox One; overlay names on Batocera to confirm on a real install); **Needs the owner** (spec §13, every item, plus plan P-2 and P-5); every `BLOCKED:` line (`git log origin/main..HEAD --format=%b | grep BLOCKED`); gates evidence (Step 2); E2E evidence with screenshots (uploaded as PR comment images, or described when upload is not possible). Post the local gate evidence as a PR comment too (GitHub Actions is blocked by billing; gates were run locally).
- [ ] **Step 7: End state:** `git status -sb` shows `## feat/systems-batch-3...origin/feat/systems-batch-3` with nothing ahead, and `gh pr view --json url,state` prints the PR.

---

## Spec coverage check

| Spec | Task |
|---|---|
| D1 scope, D2 ids, D3 no crate | 2 (entries), every task (no `Cargo.toml` dependency change) |
| D4 keys, licences, no emulator names | 1 (`cik`), 4 (open log), 5 (`RangeLog`), 6 (FIH), 11 (GUID peek), 18 (copy guard), 19 (PR) |
| D5 one `sce_sys` probe, PS4 PSF signals | 4 |
| D6 PS5 `param.json` | 6 |
| D7 PS4 kinds and version | 4 |
| D8 region | 1 (`region_from_content_id`), 4–6 (readers), 12 (keeper) |
| D9 PS4 PKG, `ExtractFirst` | 5 |
| D10 PS5 files, `NoEmulator` | 6 |
| D11 shadPS4 siblings | 8 (left overlays), 9 (names and actions) |
| D12 `.zar` | 2 (Shared), 7 |
| D13 PS5 app archives | 3 (override), 6 |
| D14 dedupe keys | 12 |
| D15 grouping keys | 4 (`ps4`/`ps5`), 10 (Xbox) |
| D16 Batocera suffix and DLC folder | 3 (profile maps), 9 (targets) |
| D17 `.ps4` launcher | 2 (`TEXT_NATIVE_EXTS`), 4 |
| D18 GDK, ERA, Scarlett rule, PC builds | 1 (`FolderLeave`, P-2), 8 (walk), 10 |
| D19 XVC, GUID files | 11 |
| D20 no external name source | 19 (PR, §13) |
| D21 covers | 10 (logo locators), 13 |
| D22 «Sin emulador» | 2 (`no_emulator`), 18 (UI) |
| D23 size confirmation | 14 (back end), 15 (front end) |
| D24 budgets, zero-byte exception | every reader task, 8 |
| D25 shadPS4 user dir, `sys_modules` | 8 |
| D26 visibility, D27 icons | 2, 17 |
| D28 facts v8 | 1 |
| §6 folders | 3; harness `TARGETS` |
| §9 UX, §10 copy, §9.3 mock | 15, 16 (core pile), 18 |
| §12 edge cases | 4–15 |
| §13 blocked list | 19 (PR) |
| §14 tests | every task; 16 (snapshots); 19 (E2E) |
