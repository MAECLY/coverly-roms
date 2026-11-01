# Missing Systems, Batch 2 (Big Discs and Handhelds With Their Own Format) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking. The UI task (Task 18) also REQUIRES frontend-design:frontend-design: follow `docs/08-brand/brand.md`, use the `src/components/ui` primitives and critique the changed screens with screenshots before committing.

**Goal:** Coverly knows the nine systems of `TODO.md` §2 «Tanda 2» (118 → 127 systems): GameCube, Wii (with WiiWare WAD), Wii U, 3DS, Switch, PS Vita, PS3, Xbox and Xbox 360, and `nds` learns DSi/DSiWare without a separate system. Each system has a header reader that reads **only plaintext metadata**, inside a fixed budget, and never touches a console key. Games that are folders (PS3 JB, Vita apps, Wii U loadiine, Xbox `default.xbe`, Xbox 360 GOD/XBLA, split NSP) move as one item and appear as one game in the library. Embedded plaintext icons become a last-resort cover source. Each built-in profile gets the folder its frontend really uses.

**Architecture:** A new module `crates/coverly-core/src/consoles/` holds one pure reader per format family over a budget-counting `ReadAt` (`Budgeted`), each returning a `ConsoleId` (system, serial, title id, title, version, kind, disc number, encryption flag, embedded-art locators). `identify/sniff.rs` calls `consoles::identify` once, between the disc rules and the cartridge rules, so raw files and archive streams share one integration point; the `ConsoleId` rides on `Sniff.console` into the memoised facts (`FACTS_VERSION` 7). `organize/walk.rs` learns key-material files, emulator-data folders and named-sub-path folder probes; a probed folder becomes a new `ItemKind::Folder` item whose tree is measured from metadata only. `write/` gains a directory move (`rename_dir_noclobber`, `Op::DirMoved`, verified per-file copy across volumes, undo, resume). `scan/` stops at a folder game and emits one `RomEntry`. `sources/embedded.rs` reads icons through the same budgeted readers. The front end gets the new enum members, copy, folder rows with their size, the `embedded` source row, nine silhouettes and a `?mockPick=modern` pile.

**Tech Stack:** Rust 1.99 (workspace MSRV 1.93), `serde`, `toml`, `quick-xml` 0.38, `ruzstd` 0.9 (decoder; encoder as dev-dependency, already used by `tests/archive_tar.rs`), `flate2` (zlib-rs), `zip` 9, `image` 0.25 (+ `tga` feature), `insta`, `filetime`, `tempfile`; React 19 + TypeScript + Tailwind 3.4 + zustand, Vitest + Testing Library; `pnpm system-icons` against RetroArch Assets at `d9f969054dc7fbb6fa89519036d2b971e0855b51`. **No new crate, no new npm package.**

**Spec:** [`docs/superpowers/specs/2026-10-26-systems-batch-2.md`](../specs/2026-10-26-systems-batch-2.md). It is binding: decisions D1–D28 (§4), files (§5), folders (§6), entries (§7), readers and budgets (§8), UX (§9), copy (§10), edge cases (§12), blocked list (§13), tests (§14) and sources (§16). This plan does not repeat its research; it turns it into tasks and records where it deviates (below). Batch 1 plan [`2026-10-26-systems-batch-1.md`](2026-10-26-systems-batch-1.md) is the pattern for every table, harness and snapshot reused here.

## Global Constraints

- **Worktree and branch.** Work only in `/Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-systems-2`, branch `feat/systems-batch-2`, which tracks `origin/feat/systems-batch-2`. Agent shells reset their cwd, so every shell command starts with `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-systems-2 &&`. Never switch branches and never touch `main`.
- **Git rules (mandatory).** An earlier run lost a whole branch because it was never pushed.
  - Conventional, granular commits in English (`feat(core): …`, `test(core): …`, `feat(ui): …`, `build(icons): …`, `docs: …`).
  - **No** `Co-Authored-By` line, no "Generated with" line, no mention of Claude or AI anywhere (commits, PR, code comments). This owner rule overrides any harness attribution.
  - Run `git push` **immediately after every commit**. Every task ends with `git commit … && git push && git status -sb`; the first line must read `## feat/systems-batch-2...origin/feat/systems-batch-2` with no `[ahead N]`.
  - Never force-push. To update with `main`: `git fetch origin && git merge origin/main`, resolve conflicts by hand (other workflows change `locales/*.json`, `src/components/*` and `TODO.md`; resolve locale files key by key, never by taking one side), run the gates, commit the merge, push.
- **Do not edit** `docs/TRACKER.md` or `TODO.md` on this branch.
- **Scope.** Only Tanda 2 (spec D1). Tanda 3 (PS4, PS5, Xbox One/Series) is not started; the PSF reader is written so it can be reused, nothing more.
- **Legal/security rule (spec D4), no exceptions.** No reader takes a key parameter or derives one; nothing decrypts. `prod.keys`, `title.keys`, `dev.keys`, `aes_keys.txt`, `seeddb.bin`, `act.dat`, loose `work.bin`, `*.rap`, `*.rif`, `*.dkey`, `*.zrif`, loose `*.tik`/`*.cetk` are recognised **by name before any open** and left with `KeyMaterial`. Inside an NSP only the `.tik` *entry name* is read from the PFS0 table, never its bytes. No catalogue or key is downloaded. Copy never says how to decrypt.
- **Headers only, within budget (spec D24, §8.6).** Per item ≤ 4 MiB read, ≤ 1 MiB decoded, ≤ 16 seeks; directory probes use `symlink_metadata` on named paths and at most one `read_dir` of one named child, capped at 256 entries. No offset or length from a file is trusted before it is checked against the file length and the cap; every multiplication is checked. Cloud placeholders are never read.
- **Never guess a folder name.** Every `Named` cell comes from spec §6, which cites §16. A cell that cannot be traced becomes `None` (Absent) and is listed in the PR.
- **Ambiguous extensions** (spec D7): `gcz rvz wia ciso` are Shared (header decides, review otherwise); `wad tgc pkg xbe nro nso nca psvimg tmd dol` are weak (only in the system's folder or with its header); `iso`, `bin` stay Generic; `3ds` joins `DECISIVE`.
- **«Verify» rules.** Each offset marked «verify» in spec §8 is checked against the cited source (Dolphin, 3dbrew, switchbrew, wiiubrew, psdevwiki, xboxdevwiki, free60, Azahar, ZArchive) during its task. If it cannot be verified, drop the rule (system by magic, name for the rest), say so in a code comment and add a line `BLOCKED: <rule> — <why>` to the commit body. Task 19 collects them for the PR.
- **TDD.** Write the failing test, run it and watch it fail for the expected reason, implement, watch it pass. Each "Run" step names the exact command.
- **Gates (same as CI).** Before every commit that touches code or tests:
  - **G-RUST:** `cargo fmt --all --check && cargo clippy --workspace --all-targets --all-features -- -D warnings && scripts/dev/clippy-windows.sh && cargo test --workspace --no-fail-fast`
  - **G-WEB** (any task touching `src/`, `locales/`, `scripts/icons/` or `src-tauri/`): `pnpm check-all && pnpm build`
  - Docs-only commits run `pnpm check-all`.
- **Windows CI leg.** Fixture mtimes pinned (`coverly_core::dedupe::testing::pin_mtime` or `filetime::set_file_mtime` to a fixed time, never "now"). Paths compared by component (`Path::components`, `file_name`, `Path::join`), never by slash text; snapshot paths go through the existing `/` redaction. Readers are tested through the in-memory `SparseAt` (Task 1), never through `set_len` files (not sparse on NTFS). Unix-only helpers go in `#[cfg(unix)] mod unix_tests`. No new folder name is reserved (`CON`, `AUX`…) or ends in a dot or a space.
- **Real data is read-only.** Never write to `~/Documents/ROMS`, `/Volumes/TinyExt/*` or `/Volumes/System`, never create `._*` there; only `ls`, `stat`, `head -c` into a copy and Coverly **plan-only** runs may touch them. A real-header check copies the file's head under `/Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-systems-2/target/systems-sample/` (`cp -RXp` for trees, `head -c 1048576 <file> > target/systems-sample/<name>` for a head), checks `find target/systems-sample -name '._*' | wc -l` prints `0`, and deletes it afterwards (`rm -rf target/systems-sample`). Real heads are never committed.
- **Fixtures are synthetic** bytes built by the builders in `consoles/testing.rs` (no copyrighted data, no keys). Real game names may appear as file names over synthetic content.
- **No UI strings in the core.** Reasons and notes are enums plus data; the UI and CLI render them. Every UI string lives in `locales/en/ui.json` **and** `locales/es/ui.json` with the same keys and placeholders. The copy is spec §10, word for word (Spanish: sentence case, no exclamation marks, exact counts, codes and sizes in mono).
- **Brand.** Only existing tokens from `design/tokens.json` (`pnpm tokens:check` green), `src/components/ui` primitives, `SystemIcon` for every system mark. «Xbox» appears only as the system's display name (`TRADEMARKS.md`).
- **Icons.** Only RetroArch Assets XMB Monochrome (CC BY 4.0) at `d9f969054dc7fbb6fa89519036d2b971e0855b51`, generated with `pnpm system-icons`. Never hand-edit generated files. Wordmarks use the Slot mount.
- **Secrets are never read or printed:** the minisign key, `~/.coverly-release/`, any `.env*`, `.vercel/`.
- **Servers and E2E.** The web UI runs on port **1466** only: `pnpm dev --port 1466 --strictPort` (mock backend). Stop it by port only: `lsof -ti tcp:1466 -sTCP:LISTEN | xargs -r kill`. E2E uses Chrome DevTools MCP `new_page` with `isolatedContext: "systems-batch-2"`; fall back to Playwright MCP; if both profiles are locked, install `playwright-core` in `/private/tmp/claude-501/-Users-miguelangelesparzacalero-Documents-GitHub/35b7c020-f2ae-412f-b757-99c24a9a498b/scratchpad/systems-batch-2` and drive Chrome for Testing from there.
- **Blocked items never block the batch.** Anything needing an owner decision (spec §13, or a «verify» rule that fails) is implemented around, recorded as a `BLOCKED:` line in the task's commit body and collected into the PR (Task 19).

## Deviations from the spec (decided in this plan)

- **P-1: one integration point for file readers.** Spec D5 splits head-only checks (sniff) from seeking readers (inspect) and adds GameCube/Wii/PS3/Xbox branches to `DiscReader`. This plan calls `consoles::identify(&mut Budgeted, ext)` **once, inside `identify::sniff::sniff`**, after `discs()` and before `cartridges()`. `sniff` already runs on `SliceAt` (heads), `FileAt` (raw files, seekable) and `StreamAt` (archive entries, capped at `STREAM_CAP`); a far offset inside an archive simply reads as end of file, which yields the magic-only verdict spec §8.6 asks for. `disc/mod.rs` is unchanged. Same behaviour, one place to audit for D4/D24.
- **P-2: the `ConsoleId` rides on `Sniff`.** `Sniff` gains `console: Option<ConsoleId>` (serde default, skipped when `None`), so it is memoised with the facts and reaches evidence, target, notes and dedupe without a parallel structure. `Sniff.serial` is set from `ConsoleId.serial` so the existing DAT serial ladder works unchanged.
- **P-3: folder games are identified at walk time.** The probe file (≤ 64 KiB, budgeted) is read by the walk's probe, so `Walked` gains `folder_games: Vec<WalkFolderGame>` carrying the `ConsoleId` and the metadata-only tree summary (D27). The planner turns each into an `ItemKind::Folder` item without going through `inspect`.
- **P-4: one batch harness.** `crates/coverly-core/tests/systems_batch2.rs` (Task 2) is table-driven like `systems_batch1.rs`: entries, targets, header rules, cover lookup. Each system task appends rows. The three plan snapshots over the §9.3 pile are written once all systems exist (Task 16).
- **P-5: research listings.** The committed `tests/fixtures/research/libretro-database-dats.txt` is pinned at `2c93952f`; every §7 DAT path is already in it (checked while writing this plan). The listing is **not** refreshed unless a test proves a path missing; the PR records that the spec's `99421ed9` pin and the fixture's `2c93952f` agree on every Tanda 2 path. The thumbnails root listing (2026-10-31, 127 folders) already has every §7 folder and no Switch folder.
- **P-6: title-id duplicates are report-only.** Spec D11 makes two copies of the same base + version duplicates with «keeper = higher version». Equal title ids do not prove equal bytes, and none of these images is hashed under the 64 MiB cap, so this plan adds an **unconfirmed** `ExactKey::Title` (report-only, like `ZipCrc`): duplicates are listed with the higher version suggested as keeper, never set aside by title id alone. Byte-equal small files (DSiWare, 3DSX) still dedupe by `Payload`. Listed in the PR as an owner decision.
- **P-7: one walk probe hook for the folder machinery.** Spec task 1b wants the folder path end to end «with a dummy probe» before any system uses it. Under the `test-util` feature, `consoles::testing::set_extra_probe(Some(fn))` adds a probe (thread-local, like `write::transfer::set_copy_fault`); tests use it with an existing system (`gb`, folder holding `GAME.PROBE`). The real probe table is empty until Task 9.

## Review Focus

1. **Keys are untouchable (D4).** No reader takes a key; key-material names are left before any open on every OS; NSP `.tik`/NCA bytes are never read. Tests: Task 1 `is_key_material_list`, Task 3 `key_material_is_never_opened` (open hook), Task 10 `nsp_ticket_and_nca_bytes_are_never_read` (range recorder).
2. **Budgets (D24).** Every reader stays within 4 MiB / 1 MiB / 16 seeks on its largest fixture and on an adversarial table (count `0x8000_0000`). Tests: `budget_*` in every reader task; Task 1 `budgeted_refuses_past_caps`.
3. **The new destructive path (D25).** Directory moves never hard-link a directory, never `fs::rename` onto a folder Coverly did not just create, never merge into an existing folder, delete sources only after every file verified, and are undoable and resumable. Tests: Task 5.
4. **No silent reclassification or new noise.** The extension-class snapshot diff over the batch equals D7 exactly; `.wad`/`.tgc` keep doom/gamecom unless a Wii/GameCube header proves otherwise; macOS `.pkg` installers and Autodesk `.3ds` models become `NotARom`, not «Sistema desconocido» rows. Tests: Task 2 snapshot, Task 3 `xar_pkg_is_an_installer`, `autodesk_3ds_is_a_model`, Task 7 `wii_wad_contradicts_doom`.
5. **Folder games stay whole** in walk, plan, apply, scan, gamelists and dedupe (one item, one `RomEntry`, never split, never set aside). Tests: Tasks 4–6, 14.
6. **Windows.** Pinned mtimes, component path comparisons, `SparseAt` for far offsets, `PathTooLong` per contained file. `scripts/dev/clippy-windows.sh` on every task.

---

## File structure

```
crates/coverly-core/
├─ src/consoles/                    (new, spec §5.1)
│  ├─ mod.rs                         ConsoleId, TitleKind, EmbeddedArt, Budgeted, identify(), identify_dir(),
│  │                                   is_key_material(), kind_* helpers, DirProbe table
│  ├─ testing.rs                     SparseAt, RangeLog, builders for every format head (cfg test/test-util)
│  ├─ nintendo_disc.rs  wad.rs       GameCube/Wii (Task 7)
│  ├─ ctr.rs                         3DS (Task 8)
│  ├─ wiiu.rs  zarchive.rs           Wii U, ZArchive (Task 9)
│  ├─ nx.rs                          Switch (Task 10)
│  ├─ psf.rs  sony.rs                PARAM.SFO, PS3, Vita (Task 11)
│  └─ xbox.rs                        Xbox, Xbox 360 (Task 12)
├─ src/lib.rs                        pub mod consoles
├─ src/systems/{systems.toml,mod.rs} libretro_also; nine entries; nds DSi; weak wad/tgc owners
├─ src/identify/sniff.rs             Sniff.console; consoles::identify call (P-1); NotARom::{Installer, Model3d};
│                                      DECISIVE += 3ds; TEXT_NATIVE_EXTS += psvita psn xbox360; NDS unit code
├─ src/identify/{dat.rs,hash.rs}     Wii/Wii U middle keys, name-prefix index; NAME_ONLY_EXTS
├─ src/disc/serial.rs                normalise_console_serial
├─ src/organize/walk.rs              key material, emulator data, folder probes, WalkFolderGame, tree summary
├─ src/organize/plan.rs              ItemKind::Folder, new LeftReason/ItemNote, FACTS_VERSION 7, folder items,
│                                      update/DLC grouping, summary counts
├─ src/organize/{target.rs,evidence.rs,inspect.rs}  folder targets + suffix; console evidence; vpk package
├─ src/organize/apply.rs             folder items through Writer::move_dir/copy_dir
├─ src/write/{noclobber.rs,transfer.rs,mod.rs,undo_organize.rs}  rename_dir_noclobber, Op::DirMoved, dir copy
├─ src/scan/mod.rs, src/gamelist.rs, src/fill/mod.rs, src/audit/  one RomEntry per folder game
├─ src/profiles/{builtin.rs,mod.rs}  rows of §6; Profile.folder_game_suffix
├─ src/dedupe/{keys.rs,units.rs}     ExactKey::Title (P-6); folder units reported
├─ src/sources/{embedded.rs,mod.rs,libretro.rs}  embedded source, ArtQuery.rom_path, libretro_also
├─ src/settings/{model.rs,load.rs}   KNOWN_SOURCES += embedded; SETTINGS_VERSION 3 migration
└─ tests/
   ├─ systems_batch2.rs              (new) harness (P-4)
   ├─ consoles_*.rs                  (new) reader integration tests through inspect/walk
   ├─ organize_folder_games.rs       (new) folder items: plan, apply, undo, resume
   └─ sniff.rs, organize_*.rs, dedupe_*.rs, scan_fixtures.rs, settings_file.rs, systems_classes.rs  updated
crates/coverly-cli/src/organize.rs   text for every new reason/note/kind
src-tauri/src/ipc.rs                 exhaustive matches, if any
Cargo.toml                           image + "tga"
src/ipc/types.ts, contract.sample.json, contract.test.ts   new enum members
src/lib/organize.ts (+ test)         copy for reasons, notes, why lines, summary
src/components/organize/{SystemGroups,ReviewBuckets,LeftInPlace}.tsx (+ tests)  folder rows, size, order
src/components/settings/SourcesSection.tsx, src/lib/settingsRules.ts, src/ipc/mock/mockSettings.ts  embedded
src/components/system/systemFallbacks.ts, scripts/icons/systemIconsLib.mjs, generated icons
src/ipc/mock/{mockOrganize.ts,fixture.ts} (+ tests)   ?mockPick=modern, shelves
locales/{es,en}/ui.json              spec §10 keys
THIRD-PARTY-NOTICES.md, CHANGELOG.md
```

---

## Task 1: Core: `consoles` module, `ConsoleId`, `Budgeted`, key material, console serials

No behaviour change yet: a pure module with its tests.

**Files:**
- Create: `crates/coverly-core/src/consoles/mod.rs`, `crates/coverly-core/src/consoles/testing.rs`
- Modify: `crates/coverly-core/src/lib.rs` (`pub mod consoles;`), `crates/coverly-core/src/disc/serial.rs` (`normalise_console_serial`), `crates/coverly-core/src/identify/sniff.rs` (`Sniff.console`)

**Interfaces (produced):**

```rust
// consoles/mod.rs
use serde::{Deserialize, Serialize};
use crate::identify::sniff::ReadAt;

pub const MAX_READ: u64 = 4 << 20;
pub const MAX_DECODED: u64 = 1 << 20;
pub const MAX_SEEKS: u32 = 16;
/// Largest metadata file read whole (`PARAM.SFO`, `meta.xml`, `cnmt.xml`).
pub const MAX_META_FILE: u64 = 64 << 10;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TitleKind { Base, Update, Dlc, Demo, #[default] Unknown }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ArtFormat { Png, Tga, Smdh }

/// Where an embedded icon is; never the bytes (spec D17).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ArtLocator {
    /// A byte range of the file itself (STFS thumbnail, CIA meta SMDH).
    Range { offset: u64, len: u64, format: ArtFormat },
    /// A zip entry (VPK `sce_sys/icon0.png`).
    ZipEntry { name: String, format: ArtFormat },
    /// A file inside a folder game, relative to the folder.
    File { rel: std::path::PathBuf, format: ArtFormat },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConsoleId {
    pub system: String,
    pub confidence: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub serial: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub title_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub version: Option<String>,
    #[serde(default)] pub kind: TitleKind,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub disc_no: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub encrypted: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")] pub art: Vec<ArtLocator>,
    /// Where the title id came from, for the «why» line (spec §10).
    #[serde(default, skip_serializing_if = "Option::is_none")] pub id_source: Option<IdSource>,
    /// Fixed vocabulary («RVZ disc header», «PFS0 ticket name»…).
    pub detail: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IdSource { ParamSfo, MetaXml, AppXml, DefaultXbe, DefaultXex, CnmtXml, TicketName, FileName, Header }

impl ConsoleId {
    /// A magic-only verdict (0.90): system known, nothing else read.
    pub fn magic(system: &str, detail: &str) -> ConsoleId;
    /// Magic plus a structure that checked out (0.95).
    pub fn proven(system: &str, detail: &str) -> ConsoleId;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadErr { Bounds, Budget, Io }

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Used { pub read: u64, pub decoded: u64, pub seeks: u32 }

/// A `ReadAt` that refuses reads past the file length or the D24 caps.
pub struct Budgeted<'a> { /* inner, len, used, next */ }
impl<'a> Budgeted<'a> {
    pub fn new(inner: &'a mut dyn ReadAt, len: u64) -> Self;
    pub fn len(&self) -> u64;
    pub fn used(&self) -> Used;
    /// Exactly `n` bytes at `offset`, or an error (never a short read).
    pub fn read(&mut self, offset: u64, n: usize) -> Result<Vec<u8>, ReadErr>;
    /// Charges `n` decoded bytes (zlib, zstd) against `MAX_DECODED`.
    pub fn decoded(&mut self, n: u64) -> Result<(), ReadErr>;
}

/// Key or licence files: never opened, hashed, uploaded or moved (D4).
pub fn is_key_material(name: &str) -> bool;

/// Every file reader of the batch, in a fixed order; `None` = not a
/// console format this batch knows. `name` is the file name (Switch
/// title-id tokens, spec D10); `""` when unknown.
pub fn identify(src: &mut Budgeted, ext: &str, name: &str) -> Option<ConsoleId>;

pub fn kind_3ds(title_id: u64) -> TitleKind;
pub fn kind_wiiu(title_id: u64) -> TitleKind;
pub fn kind_wii(title_id: u64) -> TitleKind;
pub fn kind_switch(title_id: u64) -> TitleKind;
/// `0100000000010800` → base `0100000000010000` (updates and DLC).
pub fn switch_base(title_id: u64) -> u64;

// disc/serial.rs
/// The DAT form of a console serial (spec D22): ID6/ID4 for Nintendo
/// discs, `CTR-P-XXXX` product codes, `AAAA-NNNNN` for Sony, `XX-NNN`
/// for Xbox, 8 hex for Xbox 360. `None` when `raw` is not that shape.
pub fn normalise_console_serial(system: &str, raw: &str) -> Option<String>;

// consoles/testing.rs (cfg(any(test, feature = "test-util")))
/// A sparse file in memory: declared length, bytes at given offsets, zeros elsewhere.
pub struct SparseAt { pub len: u64, pub parts: std::collections::BTreeMap<u64, Vec<u8>> }
impl SparseAt { pub fn new(len: u64) -> Self; pub fn put(&mut self, offset: u64, bytes: &[u8]) -> &mut Self; }
impl ReadAt for SparseAt { /* zeros between parts */ }
/// Records every range read through it (D4 tests).
pub struct RangeLog<'a> { pub inner: &'a mut dyn ReadAt, pub ranges: Vec<(u64, usize)> }
```

- [ ] **Step 0: Sync.** `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-systems-2 && git fetch origin && git merge origin/main && git push && git status -sb`
- [ ] **Step 1: Write the failing unit tests** in `consoles/mod.rs` `#[cfg(test)] mod tests`:

```rust
use super::testing::SparseAt;
use super::*;

#[test]
fn is_key_material_list() {
    for name in [
        "prod.keys", "TITLE.KEYS", "dev.keys", "aes_keys.txt", "seeddb.bin", "act.dat",
        "work.bin", "NPUB30024.rap", "x.rif", "game.dkey", "a.zrif", "0100.tik", "t.cetk",
    ] {
        assert!(is_key_material(name), "{name}");
    }
    for name in ["keys.txt", "prod.keys.txt", "game.iso", "title.tmd", "work.bin.bak", "ticket"] {
        assert!(!is_key_material(name), "{name}");
    }
}

#[test]
fn budgeted_refuses_past_caps() {
    let mut src = SparseAt::new(1 << 40);
    src.put(0, b"HEAD");
    let mut b = Budgeted::new(&mut src, 1 << 40);
    assert_eq!(b.read(0, 4).unwrap(), b"HEAD");
    assert_eq!(b.read((1 << 40) - 2, 4), Err(ReadErr::Bounds));
    assert_eq!(b.read(u64::MAX - 1, 4), Err(ReadErr::Bounds));
    assert_eq!(b.read(8, (MAX_READ + 1) as usize), Err(ReadErr::Budget));
    for i in 0..MAX_SEEKS {
        b.read(u64::from(i) * 4096 + 100, 1).unwrap();
    }
    assert_eq!(b.read(999_999, 1), Err(ReadErr::Budget));
    assert!(b.decoded(MAX_DECODED + 1).is_err());
    assert!(b.used().seeks <= MAX_SEEKS && b.used().read <= MAX_READ);
}

#[test]
fn title_kinds() {
    assert_eq!(kind_3ds(0x0004_0000_0003_0800), TitleKind::Base);
    assert_eq!(kind_3ds(0x0004_000E_0003_0800), TitleKind::Update);
    assert_eq!(kind_3ds(0x0004_008C_0003_0800), TitleKind::Dlc);
    assert_eq!(kind_3ds(0x0004_0002_0003_0800), TitleKind::Demo);
    assert_eq!(kind_wiiu(0x0005_000E_1010_ED00), TitleKind::Update);
    assert_eq!(kind_wii(0x0001_0001_5741_4145), TitleKind::Base);
    assert_eq!(kind_switch(0x0100_0000_0001_0000), TitleKind::Base);
    assert_eq!(kind_switch(0x0100_0000_0001_0800), TitleKind::Update);
    assert_eq!(kind_switch(0x0100_0000_0001_1001), TitleKind::Dlc);
    assert_eq!(switch_base(0x0100_0000_0001_1001), 0x0100_0000_0001_0000);
    assert_eq!(switch_base(0x0100_0000_0001_0800), 0x0100_0000_0001_0000);
}
```

  And in `disc/serial.rs` `mod tests`:

```rust
#[test]
fn console_serial_forms() {
    let n = super::normalise_console_serial;
    assert_eq!(n("gamecube", "GALE01").as_deref(), Some("GALE01"));
    assert_eq!(n("wii", "rmge01").as_deref(), Some("RMGE01"));
    assert_eq!(n("wii", "WAAE").as_deref(), Some("WAAE")); // WAD ID4
    assert_eq!(n("3ds", "CTR-P-AMKP").as_deref(), Some("CTR-P-AMKP"));
    assert_eq!(n("3ds", "CTR-P-AMKP\0\0\0\0\0\0").as_deref(), Some("CTR-P-AMKP"));
    assert_eq!(n("nds", "KJUJ").as_deref(), Some("KJUJ"));
    assert_eq!(n("ps3", "BLUS30443").as_deref(), Some("BLUS-30443"));
    assert_eq!(n("psvita", "PCSE00844").as_deref(), Some("PCSE-00844"));
    assert_eq!(n("wiiu", "WUP-P-ARPE").as_deref(), Some("ARPE"));
    assert_eq!(n("xbox", "EA-013").as_deref(), Some("EA-013"));
    assert_eq!(n("xbox360", "4d5307e6").as_deref(), Some("4D5307E6"));
    assert_eq!(n("gamecube", "GA E01"), None);
    assert_eq!(n("ps3", "BLUS3044"), None);
    // The PlayStation path is untouched.
    assert_eq!(super::normalise_serial("SLUS_008.62").as_deref(), Some("SLUS-00862"));
}
```

- [ ] **Step 2: Run** `cargo test -p coverly-core --lib consoles:: disc::serial::` and expect compile errors (module missing).
- [ ] **Step 3: Implement.**
  - `consoles/mod.rs` with the interfaces above. `Budgeted::read`: `let end = offset.checked_add(n as u64).ok_or(ReadErr::Bounds)?; if end > self.len { return Err(Bounds) }`; refuse when `used.read + n > MAX_READ`; a read whose `offset != self.next` counts one seek (the first read at 0 does not), refuse when the count would exceed `MAX_SEEKS`; read with a loop over `inner.read_at` until `n` bytes or 0 returned (0 before `n` → `Bounds`, an I/O error → `Io`); update `next = end`.
  - `is_key_material`: lower-case the name; true for the exact names `prod.keys title.keys dev.keys aes_keys.txt seeddb.bin act.dat work.bin`, or an extension in `rap rif dkey zrif tik cetk`. Doc-comment why `keys.txt` is absent (spec D4).
  - `kind_*` per spec §8.0 table: 3DS/Wii U by the high 32 bits (`0x0004_0000`/`0x0005_0000` base, `…_000E` update, `…_008C`/`…_000C` DLC, `…_0002` demo); Wii `0x0001_0001` base, `0x0001_0005` DLC; Switch `match id & 0x1FFF { 0 => Base, 0x800 => Update, x if x & 0x1000 != 0 => Dlc, _ => Unknown }`; `switch_base(id) = id & !0x1FFF`. **Verify** each against the 3dbrew «Titles», wiiubrew «Title database» and switchbrew «Title list» pages; a rule that does not match returns `Unknown` (`BLOCKED:` line).
  - `identify(src, ext, name)`: for now `None` (each system task adds its reader to the ordered list in this function: `nintendo_disc`, `wad`, `ctr`, `nx`, `wiiu`, `sony`, `xbox`; each is called only when its magic or extension can apply, so a 128 KiB head costs at most a few slice compares).
  - Text cleaning: `pub(crate) fn clean_title(raw: &str) -> Option<String>` = `crate::sources::clean_text(raw.trim_end_matches('\0').trim(), 128)` (existing `pub fn clean_text(s: &str, max_chars: usize) -> Option<String>` in `sources/mod.rs`: controls and bidi removed, `None` when empty).
  - `consoles/testing.rs`: `SparseAt` (reads return zeros between parts, `Ok(0)` past `len`), `RangeLog`. Guard with `#[cfg(any(test, feature = "test-util"))]` in `mod.rs`.
  - `disc/serial.rs` `normalise_console_serial` per the test (upper-case; strip NULs/spaces at the end; ID6 `^[A-Z0-9]{6}$` for gamecube/wii, ID4 `^[A-Z0-9]{4}$` for wii/nds/wiiu (from `WUP-[PNM]-XXXX` take the last part); `^CTR-[PNMU]-[A-Z0-9]{4}$` and `^KTR-…` for 3ds; four letters + five digits → `AAAA-NNNNN` for ps3/psvita; `^[A-Z0-9]{2}-\d{3}$` for xbox; `^[0-9A-F]{8}$` for xbox360).
  - `identify/sniff.rs`: `Sniff` gains `#[serde(default, skip_serializing_if = "Option::is_none")] pub console: Option<crate::consoles::ConsoleId>`. Nothing sets it yet.
- [ ] **Step 4: Run** `cargo test -p coverly-core --lib consoles:: disc::serial::`, then G-RUST.
- [ ] **Step 5: Commit and push:** `git add -A crates && git commit -m "feat(core): add the console header module with budgets and key-material rules" && git push && git status -sb`

---

## Task 2: Systems plumbing: `libretro_also`, the batch 2 harness and the class snapshot baseline

**Files:**
- Modify: `crates/coverly-core/src/systems/mod.rs` (`libretro_also` field, lenient merge, invariants), `crates/coverly-core/src/systems/systems.toml` (header comment)
- Modify: `crates/coverly-core/src/sources/libretro.rs` (try `libretro_also` folders after `libretro`, cover lookup only)
- Create: `crates/coverly-core/tests/systems_batch2.rs`

**Interfaces (produced):**

```rust
// systems/mod.rs, System
/// Extra libretro-thumbnails folders tried after `libretro`, for covers
/// only (spec D9). `{libretro}` in templates keeps the primary.
pub libretro_also: Vec<String>,
```

- [ ] **Step 0: Sync.**
- [ ] **Step 1: Failing tests.**
  - `systems/mod.rs`: `libretro_also_is_parsed_and_lenient` — a user override with `libretro_also = ["X - Y"]` appends; repeating an existing value does not duplicate; `libretro_also` without `libretro` is an invariant violation in `check_builtin_invariants` (built-in only).
  - `sources/libretro.rs`: `libretro_also_folder_is_tried_after_the_primary` with the existing `MockServer`: the primary listing lacks `Foo.png`, the `also` listing has it → one candidate whose URL is under the `also` folder; with both having it, the primary wins.
  - `tests/systems_batch2.rs`, modelled on `systems_batch1.rs` (copy its `research()` helper, `libretro_names_exist_on_the_thumbnail_server`, `dat_paths_exist_in_the_pinned_database`, `organise_targets_match_the_research_table`, `header_rules`, `cover_lookup_finds_batch_systems`), with **empty** tables `SYSTEMS`, `TARGETS`, `HEADERS` typed as in batch 1, plus:

```rust
/// Every `libretro_also` folder exists on the thumbnail server too.
#[test]
fn libretro_also_names_exist_on_the_thumbnail_server() {
    let dirs: BTreeSet<String> = parse_apache_dirs(&research("thumbnails-root.html")).into_iter().collect();
    for s in SystemDb::builtin().all() {
        for name in &s.libretro_also {
            assert!(dirs.contains(name), "{}: `{name}`", s.id);
        }
    }
}

/// The batch's raw-file header rules go through `sniff` like a real file
/// (P-1): (id, ext, sparse file, expected system, expected serial).
type Sparse = fn() -> coverly_core::consoles::testing::SparseAt;
const CONSOLE_HEADERS: &[(&str, &str, Sparse, Option<&str>, Option<&str>)] = &[];

#[test]
fn console_header_rules() {
    for (id, ext, file, want, serial) in CONSOLE_HEADERS {
        let mut f = file();
        let len = f.len;
        let s = sniff(&mut f, len, ext);
        let best = s.hits.iter().max_by(|a, b| a.weight.total_cmp(&b.weight)).map(|h| h.system.as_str());
        assert_eq!(best, *want, "{id} .{ext}");
        assert_eq!(s.serial.as_deref(), *serial, "{id} .{ext} serial");
    }
}
```

- [ ] **Step 2: Run** `cargo test -p coverly-core --lib systems:: sources::libretro:: && cargo test -p coverly-core --test systems_batch2` and expect compile failures (`libretro_also`).
- [ ] **Step 3: Implement.** `RawSystem.libretro_also: Option<Vec<String>>` (keep `deny_unknown_fields`); merge appends without duplicates; `System { … }` literals elsewhere (`scan/classify.rs` test helper and whatever `cargo build --workspace --all-targets` reports) get `libretro_also: Vec::new()`. In `LibretroSource::candidates`, after the primary folder's exact and fuzzy passes find nothing, run the same passes on each `libretro_also` folder in order; `has_name` checks the primary then the others. Document the field in the `systems.toml` header comment.
- [ ] **Step 4: Run** the tests, `INSTA_UPDATE=no cargo test -p coverly-core --test systems_classes` (must still pass: no extension changed), G-RUST, G-WEB (`src-tauri` builds against `System`).
- [ ] **Step 5: Commit and push:** `git add -A crates && git commit -m "feat(core): add extra libretro thumbnail folders and the batch 2 harness" && git push && git status -sb`

---

## Task 3: Core: new reasons and notes, `NotARom` installer/model, facts v7, key material and emulator data in the walk

**Files:**
- Modify: `crates/coverly-core/src/organize/plan.rs` (`LeftReason::{KeyMaterial, EmulatorData}`, `ItemNote::{Update, Dlc, EncryptedContent, InstallFirst, FolderSuffix}`, `FACTS_VERSION` 7)
- Modify: `crates/coverly-core/src/identify/sniff.rs` (`NotARom::{Installer, Model3d}`, the two magics)
- Modify: `crates/coverly-core/src/organize/walk.rs` (key material before any open; emulator-data roots)
- Modify: `crates/coverly-core/src/organize/inspect.rs`, `src/identify/hash.rs`, `src/dedupe/*` (hashing entry points), `src/fill/*` (peeking): call the open hook
- Modify: `crates/coverly-cli/src/organize.rs` (`left_text`, `note_text`), `src-tauri/src/ipc.rs` if it matches exhaustively
- Modify: `crates/coverly-core/src/consoles/mod.rs` (`open_rom`, test-util open log)
- Test: `crates/coverly-core/tests/organize_walk.rs`, `tests/sniff.rs`, `tests/organize_plan.rs`, new `tests/consoles_keys.rs`

**Interfaces (produced):**

```rust
// organize/plan.rs
pub enum LeftReason {
    // …
    /// A console key or licence file (spec D4): never read, never moved.
    KeyMaterial,
    /// Inside an emulator's own data tree (`ux0`, `dev_hdd0`, `mlc01`,
    /// `sdmc/Nintendo 3DS`, `nand`): moving it breaks the install.
    EmulatorData { folder: String },
}
pub enum ItemNote {
    // …
    /// An update of `of` (its title, or its title id when the base is not in the plan).
    Update { of: String },
    Dlc { of: String },
    /// The header says the content is encrypted; `emulator` only opens decrypted copies.
    EncryptedContent { emulator: String },
    /// A package the emulator installs (PS3/Vita PKG).
    InstallFirst { emulator: String },
    /// The folder is renamed to `name` so `layout` shows it (spec D13).
    FolderSuffix { name: String, layout: String },
}
const FACTS_VERSION: u32 = 7;

// identify/sniff.rs
pub enum NotARom { WebPage, Text, Patch, /** macOS `xar!` package */ Installer, /** Autodesk 3D Studio */ Model3d }

// consoles/mod.rs
/// Opens a candidate for reading; refuses key material (debug assert +
/// `PermissionDenied`) and, under `test-util`, logs the path.
pub fn open_rom(path: &std::path::Path) -> std::io::Result<std::fs::File>;
#[cfg(feature = "test-util")] pub fn start_open_log();
#[cfg(feature = "test-util")] pub fn take_open_log() -> Vec<std::path::PathBuf>;
```

- [ ] **Step 0: Sync.**
- [ ] **Step 1: Failing tests.**
  - `tests/sniff.rs`:
    - `xar_pkg_is_an_installer`: `b"xar!\0\x1c\0\x01"` + zeros, ext `pkg` → `not_a_rom == Some(NotARom::Installer)`, no hits; the same bytes as `.bin` → no `Installer`.
    - `autodesk_3ds_is_a_model`: `[0x4D, 0x4D, 0x10, 0, 0, 0, 0x02, 0x00]` + zeros, ext `3ds` → `Some(NotARom::Model3d)`; as `.dat` → not.
  - `tests/organize_walk.rs`:
    - `key_material_is_left_before_any_read`: pile with `prod.keys`, `title.keys`, `NPUB30024.rap`, `work.bin`, `x.tik`, `keys.txt`, `Game.iso` → the first five are `LeftReason::KeyMaterial`, `keys.txt` is in `other_files`, `Game.iso` is a candidate.
    - `emulator_data_roots_are_left`: `Pile/ux0/app/PCSB00245/eboot.bin`, `Pile/dev_hdd0/game/X/PARAM.SFO`, `Pile/mlc01/usr/title/x`, `Pile/sdmc/Nintendo 3DS/a`, `Pile/nand/system/x` → one `EmulatorData { folder }` per root (`folder` = the root's name), no candidate below them; `Pile/sdmc/other.gba` (no `Nintendo 3DS` child) is walked normally.
  - `tests/consoles_keys.rs` (`required-features = ["test-util"]` in `crates/coverly-core/Cargo.toml`, like the existing test-util tests):

```rust
//! Spec D4 / §14.1: key and licence files are never opened by the walk,
//! the plan's inspection or hashing (an open hook records every path).
mod common;
use coverly_core::consoles::{start_open_log, take_open_log};

#[test]
fn key_material_is_never_opened() {
    let world = common::World::new();
    let pile = tempfile::TempDir::new().unwrap();
    for name in ["prod.keys", "title.keys", "x.rap", "work.bin", "Game.gba"] {
        common::put(&pile.path().join(name), &[7u8; 2048]);
        coverly_core::dedupe::testing::pin_mtime(&pile.path().join(name), 1);
    }
    start_open_log();
    let plan = common::plan_offline(&world, pile.path(), "esde");
    let opened = take_open_log();
    for name in ["prod.keys", "title.keys", "x.rap", "work.bin"] {
        assert!(!opened.iter().any(|p| p.file_name().unwrap() == name), "{name} opened");
        let item = plan.items.iter().find(|i| i.sources[0].file_name().unwrap() == name).unwrap();
        assert!(matches!(&item.action, coverly_core::organize::plan::OrganizeAction::Left {
            reason: coverly_core::organize::plan::LeftReason::KeyMaterial }), "{name}");
    }
    assert!(opened.iter().any(|p| p.file_name().unwrap() == "Game.gba"), "the hook records opens");
}
```

    Adapt the helper names to what `tests/common/` provides (`grep -n "pub fn" crates/coverly-core/tests/common/*.rs`); add a `plan_offline` helper there if none exists. Do the same with a library: `scan` + `dedupe` plan over `ROMS/switch/prod.keys` and `ROMS/gba/Game.gba` records no open of `prod.keys` (dedupe hashing path).
  - `tests/organize_plan.rs` `facts_v6_rows_are_reread`: store a v6 facts row for a candidate in the cache (the existing memo helpers in that file), plan → the candidate is inspected again (the existing «inspected» counter or the open log shows it).
  - CLI: `crates/coverly-cli/src/organize.rs` tests `left_text_new_reasons` and `note_text_new_notes` assert non-empty English text for each new variant (`"key or licence file: never read or moved"`, `"inside an emulator's data folder (ux0): left as it is"`, `"update for {of}"`, `"downloadable content for {of}"`, `"encrypted copy: {emulator} only opens decrypted copies"`, `"{emulator} installs it; it does not open from the folder"`, `"renamed to {name} so {layout} shows it"`).
- [ ] **Step 2: Run** `cargo test -p coverly-core --test sniff --test organize_walk --test organize_plan && cargo test -p coverly-core --features test-util --test consoles_keys && cargo test -p coverly-cli` and expect failures.
- [ ] **Step 3: Implement.**
  - Enums and CLI text as above (serde `kebab-case`: `key-material`, `emulator-data`, `update`, `dlc`, `encrypted-content`, `install-first`, `folder-suffix`, `installer`, `model3d`).
  - Sniff: in `sniff()` before `text_kind`/`container`: `if ext == "pkg" && head.starts_with(b"xar!") { s.not_a_rom = Some(Installer); return s }`; `if ext == "3ds" && head.starts_with(&[0x4D, 0x4D]) && !starts(&head, 0x100, b"NCSD") && !starts(&head, 0x100, b"NCCH") { s.not_a_rom = Some(Model3d); return s }`. Check `container()` does not already claim `xar!` (`grep -n "xar" crates/coverly-core/src/archive/detect.rs`); if it does, place the rule before it.
  - Walk, files loop: right after the AppleDouble/junk checks and **before** `entry.metadata()` is used for anything but the type, `if is_key_material(&name) { out.left.push((path, LeftReason::KeyMaterial)); continue; }`. `folder_game_in` skips key-material names when it lists a directory.
  - Walk, `filter_entry`: before `resolve_folder_in`, `if let Some(root) = emulator_data_root(entry.path(), &name) { emulator.push((path, LeftReason::EmulatorData { folder: name.to_string() })); return false; }` where `emulator_data_root` matches (case-insensitive) `ux0`, `dev_hdd0`, `mlc01`, `nand`, and `sdmc` only when `symlink_metadata(dir.join("Nintendo 3DS"))` is a directory. Extend `out.left` with them at the end like `folder_games`.
  - `open_rom`: `if is_key_material(name) { return Err(io::Error::new(PermissionDenied, "key material")) }`, log under `test-util` (`static OPEN_LOG: Mutex<Option<Vec<PathBuf>>>`, global so rayon workers are recorded), then `File::open`. Replace `File::open` with `crate::consoles::open_rom` in: `organize/inspect.rs` (`inspect_file_with`, `inspect_set_primary`, archive opens), `organize/walk.rs` (`starts_like_a_wad`), `identify/hash.rs`, the dedupe hashing/reading entry points (`grep -n "File::open" crates/coverly-core/src/dedupe/*.rs`), and fill's ROM peeks (`grep -n "File::open" crates/coverly-core/src/fill/*.rs crates/coverly-core/src/scan/*.rs`). Do not touch opens of cache, settings, journal, art or DAT files.
  - Scan: `scan_system` skips key-material names (never a `RomEntry`), so fill and dedupe never see them.
  - `FACTS_VERSION` 7 with a comment: «7: Tanda 2 console ids on `Sniff.console` (systems batch 2 D23)».
- [ ] **Step 4: Run** the tests, existing snapshots (`cargo test -p coverly-core`; any changed snapshot is reviewed: none expected), G-RUST, G-WEB.
- [ ] **Step 5: Commit and push:** `git add -A crates src-tauri && git commit -m "feat(core): leave key files and emulator data untouched and add batch 2 reasons" && git push && git status -sb`

---

## Task 4: Folder items, plan side: probes, tree summary, D27 gates, `ItemKind::Folder`, targets and suffixes

The riskiest part of the batch starts here; no system uses it until Task 9. Tests use the P-7 hook.

**Files:**
- Modify: `crates/coverly-core/src/consoles/mod.rs` (`DirProbe`, `identify_dir`, empty `DIR_PROBES`, test-util `set_extra_probe`)
- Modify: `crates/coverly-core/src/organize/walk.rs` (`WalkFolderGame`, `TreeSummary`, `tree_summary`)
- Modify: `crates/coverly-core/src/organize/plan.rs` (`ItemKind::Folder`, folder items, tree fingerprint), `organize/target.rs` (folder target + suffix), `profiles/mod.rs` (`Profile.folder_game_suffix`), `profiles/custom.rs` (serde default, clone)
- Modify: `crates/coverly-cli/src/organize.rs` (folder kind text)
- Create: `crates/coverly-core/tests/organize_folder_games.rs`

**Interfaces (produced):**

```rust
// consoles/mod.rs
/// One named entry the walk already listed for a directory.
pub struct DirListing<'a> { pub dir: &'a Path, pub names: &'a [String] }
/// A folder probe: looks only at `names`, may `symlink_metadata` named
/// sub-paths, may `read_dir` one named child (≤ 256 entries) and read one
/// probe file (≤ MAX_META_FILE) through `Budgeted`.
pub type DirProbe = fn(&DirListing) -> Option<FolderMatch>;
pub struct FolderMatch { pub id: ConsoleId, /** relative */ pub probe: PathBuf }
/// Probes in the fixed order of spec D12; the first match wins; two
/// matches naming different systems → `Err(Conflict)` (review).
pub fn identify_dir(listing: &DirListing) -> Result<Option<FolderMatch>, DirConflict>;
pub struct DirConflict { pub systems: Vec<String> }
#[cfg(feature = "test-util")] pub fn set_extra_probe(p: Option<DirProbe>);

// organize/walk.rs
pub const TREE_MAX_ENTRIES: usize = 100_000;
pub const TREE_MAX_DEPTH: usize = 16;
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreeSummary {
    pub files: u64,
    pub bytes: u64,
    pub newest_mtime_ns: i64,
    pub largest_file: u64,
    /// Longest path below the folder, in UTF-16 units (Windows limit check).
    pub longest_rel_utf16: usize,
    pub truncated: bool,        // > TREE_MAX_ENTRIES or > TREE_MAX_DEPTH
    pub symlink: bool,
    pub partial: bool,          // `.part`, `.crdownload`, zero-byte file
    pub recent: bool,           // a file inside the `recent` window
    pub cloud_only: bool,
}
/// Metadata-only pass (`symlink_metadata`, never content).
pub fn tree_summary(dir: &Path, opts: &WalkOptions) -> TreeSummary;
pub struct WalkFolderGame { pub path: PathBuf, pub id: ConsoleId, pub probe: PathBuf, pub tree: TreeSummary }
// Walked gains:
pub folder_games: Vec<WalkFolderGame>,

// organize/plan.rs
pub enum ItemKind { File, Archive, Set, /** a game that is a folder (spec D12) */ Folder }

// profiles/mod.rs, Profile
/// Suffix a folder game of `system` must end in on this layout (`ps3` →
/// `.ps3` on ES-DE and Batocera, spec D13). Empty elsewhere.
#[serde(default)]
pub folder_game_suffix: BTreeMap<String, String>,
```

- [ ] **Step 0: Sync.**
- [ ] **Step 1: Failing tests** in `tests/organize_folder_games.rs` (`required-features = ["test-util"]`). A test probe matches a folder holding `GAME.PROBE` and returns `ConsoleId { system: "gb", serial: Some("TEST01"), title_id: Some("TEST01"), id_source: Some(IdSource::Header), … }`; every test sets it with `set_extra_probe(Some(test_probe))` and clears it at the end (a guard struct whose `Drop` clears it).

```rust
#[test]
fn probed_folder_is_one_item_and_its_contents_are_not_walked() {
    let _g = ProbeGuard::new();
    let (world, pile, target) = setup();
    put_tree(pile.path(), &[("Zelda/GAME.PROBE", 16), ("Zelda/data/a.gb", 32_768), ("Zelda/data/b.bin", 10)]);
    let p = plan(&world, pile.path(), target.path(), "esde");
    let folder: Vec<_> = p.items.iter().filter(|i| i.kind == ItemKind::Folder).collect();
    assert_eq!(folder.len(), 1);
    let item = folder[0];
    assert_eq!(item.sources, [PathBuf::from("Zelda")]);
    assert_eq!(item.size, 16 + 32_768 + 10);
    assert_eq!(item.fps.len(), 1);
    assert_eq!(item.system_id.as_deref(), Some("gb"));
    let t = item.target.as_ref().unwrap();
    assert_eq!(t.components().last().unwrap().as_os_str(), "Zelda");
    assert!(!p.items.iter().any(|i| i.sources[0].ends_with("a.gb")));
}

#[test]
fn folder_gates() {
    // One case per D27 gate, each in its own pile.
    let cases: &[(&str, &[(&str, u64)], fn(&LeftReason) -> bool)] = &[
        ("partial", &[("G/GAME.PROBE", 16), ("G/x.part", 5)], |r| matches!(r, LeftReason::PartialDownload)),
        ("zero", &[("G/GAME.PROBE", 16), ("G/empty", 0)], |r| matches!(r, LeftReason::PartialDownload)),
    ];
    // … plus: a file with mtime = now → RecentlyModified; (unix) a symlink
    // inside → Symlink; a probe answering cloud_only for one inner file →
    // CloudOnly; a > 4 GiB member on a FAT32 target via a MetaProbe/size
    // override (never a real 4 GiB file) → TooBigForFat; a nested path
    // whose target exceeds 259 UTF-16 units → PathTooLong.
}

#[test]
fn folder_suffix_is_appended_once() {
    // A test profile with folder_game_suffix {"gb": ".gbdir"}: `Zelda/` →
    // `GB/Zelda.gbdir` with ItemNote::FolderSuffix { name: "Zelda.gbdir", layout }; a
    // pile folder already named `Zelda.gbdir` gets no second suffix and no note.
}

#[test]
fn two_probes_naming_different_systems_go_to_review() { /* DirConflict → Review, both in `alternatives` */ }

#[test]
fn taken_target_folder_gets_a_number_never_a_merge() {
    // `<target>/GB/Zelda/` exists → target `GB/Zelda (2)`, RenameReason::Collision.
}
```

  Use `pin_mtime` on every file except the «recent» case; compare paths by component.
- [ ] **Step 2: Run** `cargo test -p coverly-core --features test-util --test organize_folder_games` and expect failures.
- [ ] **Step 3: Implement.**
  - Walk `filter_entry`: after `resolve_folder_in` and before `folder_game_in`, list the directory **once** (reuse the listing `folder_game_in` already does: refactor so one `read_dir` produces `names` for both), build `DirListing`, call `consoles::identify_dir`. A match → push `WalkFolderGame { path, id, probe, tree: tree_summary(path, opts) }` and return `false` (the walker never enters it). A `DirConflict` → push the folder to a new `Walked.folder_conflicts: Vec<(PathBuf, Vec<String>)>` and return `false`.
  - `tree_summary`: iterative stack of `(dir, depth)`, `fs::read_dir` + `symlink_metadata` per entry, never `File::open`; sets the flags per D27 (`is_partial_download` from `scan::classify`, zero-byte files, `is_recent`, `opts.probe.cloud_only`, symlink/reparse via `file_type().is_symlink()` or the existing `is_reparse_point`); stops and sets `truncated` beyond `TREE_MAX_ENTRIES` entries or `TREE_MAX_DEPTH`; junk files (`.DS_Store`, `._*`, `Thumbs.db`, `desktop.ini`) are not counted.
  - Plan: each `WalkFolderGame` → an `OrganizeItem { kind: Folder, sources: [rel folder], size: tree.bytes, fps: [SourceFp { size: tree.bytes, mtime_ns: tree.newest_mtime_ns }], method: Some(IdLevel::Header), confidence: id.confidence, why: [Evidence header with id.detail], … }`. Gates in this order: `truncated` → `Review` with no target change (the UI shows «más de» from `size`, spec §9.1); `symlink` → `Left Symlink`; `partial` → `PartialDownload`; `recent` → `RecentlyModified`; `cloud_only` → `CloudOnly`; FAT32 target and `largest_file > 4 GiB - 1` → `TooBigForFat`, except when `id.system == "switch"` and the folder is a split «archive bit» folder (Task 10 sets a `ConsoleId.detail` of `split NSP folder`; parts are ≤ 4 GiB by design); target path + `longest_rel_utf16` over the existing Windows limit → `PathTooLong`. Free space: add `tree.bytes` to the existing `NoSpace` sum.
  - Re-validation at apply: a folder item's fingerprint is recomputed with `tree_summary` (same function) and compared on `(bytes, newest_mtime_ns)`; a change → the existing `ChangedSincePlan` skip.
  - Target (`target.rs`): `<system folder>/<folder name>[suffix]`; the suffix from `profile.folder_game_suffix.get(system)` when the name does not already end with it (ASCII case-insensitive) → `ItemNote::FolderSuffix { name, layout: profile.name }`. Collision with an existing target folder or a planned one → existing `Collision` policy (`" (2)"` inserted **before** the suffix: `Game (2).ps3`). Never a target inside an existing folder of the same name.
  - Disc layout: never applied to folder items (they already are folders).
  - Profiles: `folder_game_suffix` empty on every built-in for now; custom profiles clone it; old custom-profile files load with an empty map (`#[serde(default)]`).
  - CLI: render `ItemKind::Folder` as `folder` in the item line, and `FolderSuffix`.
- [ ] **Step 4: Run** the tests, `cargo test -p coverly-core` (no existing snapshot should change: no built-in probe yet), G-RUST, G-WEB (`OrganizeItemKind` gains `'folder'` in Task 18; the TS build is unaffected until then because the contract sample has no folder item).
- [ ] **Step 5: Commit and push:** `git add -A crates && git commit -m "feat(core): plan games that are folders as one item with metadata-only gates" && git push && git status -sb`

---

## Task 5: Folder items, apply side: directory move, cross-volume copy, journal, undo, resume

**Files:**
- Modify: `crates/coverly-core/src/write/noclobber.rs` (`rename_dir_noclobber`, `rename_dir_noclobber_using`)
- Modify: `crates/coverly-core/src/write/transfer.rs` (`Writer::move_dir`, `Writer::copy_dir`, `DirOutcome`, byte progress)
- Modify: `crates/coverly-core/src/write/mod.rs` (`Op::DirMoved { from }`, undo and resume of it), `write/undo_organize.rs` (if undo lives there)
- Modify: `crates/coverly-core/src/organize/apply.rs` (folder items)
- Test: `crates/coverly-core/tests/noclobber.rs`, `tests/organize_folder_games.rs`, `tests/organize_undo.rs`, `tests/journal_resume.rs`

**Interfaces (produced):**

```rust
// write/noclobber.rs
/// Moves the directory `from` to `to` on one volume, never replacing or
/// merging. Exclusive rename first; where unsupported: twin check,
/// `create_dir(to)` (exclusive), then `rename(from, to)` onto that empty
/// folder this call just created. Never a hard link (`EPERM` on dirs).
pub fn rename_dir_noclobber(from: &Path, to: &Path) -> io::Result<()>;
#[doc(hidden)]
pub fn rename_dir_noclobber_using(from: &Path, to: &Path, exclusive: &dyn Fn(&Path, &Path) -> io::Result<()>) -> io::Result<()>;

// write/transfer.rs
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub enum DirOutcome {
    Moved { to: PathBuf, renamed: bool },
    Copied { to: PathBuf, renamed: bool },
    /// Every file copied and verified, but the source tree could not be removed.
    CopiedNotRemoved { to: PathBuf, renamed: bool },
    SkippedExists { existing: PathBuf },
}
impl Writer {
    pub fn move_dir(&mut self, from: &Path, to: &Path, collision: Collision, item: Option<usize>,
                    progress: &mut dyn FnMut(u64)) -> Result<DirOutcome, WriteError>;
    pub fn copy_dir(&mut self, from: &Path, to: &Path, collision: Collision, item: Option<usize>,
                    progress: &mut dyn FnMut(u64)) -> Result<DirOutcome, WriteError>;
}
// write/mod.rs, Op
/// The folder `target` came from the folder `from` by one rename.
DirMoved { from: PathBuf },
```

- [ ] **Step 0: Sync.**
- [ ] **Step 1: Failing tests.**
  - `tests/noclobber.rs`:
    - `dir_rename_refuses_an_existing_folder`: `to` exists (empty) → `AlreadyExists`, `from` intact, `to` still empty.
    - `dir_rename_fallback_creates_then_renames`: `rename_dir_noclobber_using` with `exclusive` returning `ErrorKind::Unsupported` → `to` holds the tree, `from` gone; with `to` pre-created → `AlreadyExists` and nothing moved (proves no rename onto a folder this call did not create); a case twin (`game` vs `Game`) → `AlreadyExists`.
  - `tests/organize_folder_games.rs`:
    - `same_volume_folder_move_is_one_rename_and_undoes`: apply → `<target>/GB/Zelda/data/a.gb` exists, source gone, the journal has one `dir-moved` entry (read with the existing journal reader helper); undo → source restored byte-identical, target folder gone.
    - `cross_volume_copy_is_verified_per_file`: force the cross-volume path with `set_copy_fault(Some(|stage, _| (stage == CopyStage::Rename).then(|| io::Error::from_raw_os_error(libc::EXDEV))))` (`#[cfg(unix)]`; on Windows use `ERROR_NOT_SAME_DEVICE` = 17 — check how `is_cross_device` recognises it) → every file copied with its mtime, sources deleted, source folder removed; journal has `moved { via: copy }` per file and `dir-created` per folder.
    - `verify_fault_mid_tree_deletes_nothing`: fault at `CopyStage::Verify` on the second file → error outcome, **every** source file still present, the copied files are rolled back by the existing per-file journal undo.
    - `junk_only_leftover_is_removed_other_leftover_keeps_the_source`: a `.DS_Store` left in the source → folder removed; a file that appeared during the copy (inject via the fault hook at `Delete`) → `CopiedNotRemoved`.
    - `copy_mode_keeps_the_source`.
    - `resume_after_a_crash_between_files`: run apply with a fault that aborts after the first file (`CopyStage::Verify` returning an error that the job treats as a crash, or use the journal resume helpers of `tests/journal_resume.rs`), then resume → finishes, no file copied twice (size + mtime check), sources deleted only at the end.
    - `progress_reports_bytes_inside_the_item`: the progress callback is called with increasing byte totals ending at `item.size`.
    - `fingerprint_change_skips_the_item`: touch a file inside after planning → `ChangedSincePlan`.
- [ ] **Step 2: Run** `cargo test -p coverly-core --test noclobber && cargo test -p coverly-core --features test-util --test organize_folder_games --test organize_undo --test journal_resume` and expect failures.
- [ ] **Step 3: Implement.**
  - `rename_dir_noclobber_using`: `if twin_exists(to)? { AlreadyExists }`; `match exclusive(from, to) { Ok => Ok, Err(e) if !is_unsupported(&e) => Err(e), _ => { if twin_exists(to)? { AlreadyExists }; fs::create_dir(to)?; fs::rename(from, to).inspect_err(|_| { let _ = fs::remove_dir(to); }) } }`. On Windows `MoveFileExW` with flags 0 already refuses an existing target and moves directories on one volume, so the existing `exclusive_rename` is reused as is. Document that POSIX `rename(2)` of a directory onto an **empty** directory replaces it, which is why the fallback only ever renames onto the folder it just created.
  - `move_dir`: ensure the parent with `ensure_dir`; resolve the name with the existing `next_free` logic for `Collision::Rename` (directory names: insert ` (n)` before a `.ps3`-style suffix when present, otherwise at the end); journal `Op::DirMoved { from }` **before** the rename (as other ops are written as intents), call `rename_dir_noclobber` through the `fault(CopyStage::Rename, …)` hook; on cross-device → `copy_tree(from, to, Mode::Move)`.
  - `copy_tree`: walk the source with `symlink_metadata` (no symlink is followed; Task 4's gates already left trees with links); `ensure_dir` per folder (journals `DirCreated`); each file through the existing streamed, verified `transfer(..., Mode::Copy)` path so `._*` is never written and mtime is kept, journaled per file; progress after each chunk. Move mode: only after **every** file verified, delete the sources (`with_sharing_retries(fs::remove_file)`), then remove the source folders bottom-up after deleting known junk (`.DS_Store`, `._*`, `Thumbs.db`, `desktop.ini`); a non-empty folder → `CopiedNotRemoved`.
  - Undo of `DirMoved`: `rename_dir_noclobber(target, from)`; if `from` exists → leave it and report as the existing undo conflict outcome. Resume: a `DirMoved` intent whose `target` exists and `from` does not is complete; the reverse is not started; per-file copy entries resume as today (size + mtime).
  - `organize/apply.rs`: `ItemKind::Folder` items call `move_dir`/`copy_dir` by mode; the outcome maps onto the existing item outcomes.
- [ ] **Step 4: Run** the tests, `cargo test -p coverly-core --features test-util`, G-RUST (Windows clippy matters here: `#[cfg]` around `libc` codes).
- [ ] **Step 5: Commit and push:** `git add -A crates && git commit -m "feat(core): move and copy game folders safely with undo and resume" && git push && git status -sb`

---

## Task 6: Folder games in the library: scan, gamelists, hashing, audit (D26)

**Files:**
- Modify: `crates/coverly-core/src/scan/mod.rs` (`scan_system` stops at a folder game), `src/scan/classify.rs` if the stop rule fits there
- Modify: `crates/coverly-core/src/gamelist.rs` (`<path>` of a folder entry), `src/fill/mod.rs`, `src/identify/hash.rs`, `src/audit/mod.rs` (skip opening folder entries)
- Test: `crates/coverly-core/tests/scan_fixtures.rs`, `tests/gamelist.rs`, `tests/organize_folder_games.rs`

**Interfaces (produced):**

```rust
// scan/mod.rs, RomEntry
/// The entry is a game folder (spec D26): `path` is the folder, `ext` the
/// profile suffix without the dot (`ps3`) or empty; never opened as a file.
pub is_dir: bool,
```

- [ ] **Step 0: Sync.**
- [ ] **Step 1: Failing tests** (with the P-7 probe): a library `ROMS/GB/Zelda/GAME.PROBE` + `ROMS/GB/Zelda/data/a.gb` + `ROMS/GB/Tetris.gb` → exactly two `RomEntry`s: `Zelda` (`is_dir`, `size` = tree bytes, `stem` = `Zelda`, `ext` = ``) and `Tetris`; a folder `ROMS/GB/Mario.gbdir/` with `folder_game_suffix {"gb": ".gbdir"}` and no probe file is also one entry (`stem` = `Mario`, `ext` = `gbdir`); the gamelist writes `<path>./Zelda</path>` and `<path>./Mario.gbdir</path>`; the hash step and ScreenScraper hash lookup never open a folder entry (open log from Task 3 is empty for it); the audit lists it once.
- [ ] **Step 2: Run** `cargo test -p coverly-core --features test-util --test scan_fixtures --test gamelist --test organize_folder_games` and expect failures.
- [ ] **Step 3: Implement.** In `scan_system`'s `filter_entry`, for a directory: build the `DirListing` (one `read_dir`), and if `consoles::identify_dir` matches or the name ends in `profile.folder_game_suffix.get(&system.id)`, push a folder `RomEntry` (via a `folder_entry(dir, path, tree)` built from `walk::tree_summary` with `TreeSummary` limits) and return `false`. `RomEntry` literals elsewhere get `is_dir: false`. Gamelist: folder entries keep the folder name with its suffix in `<path>`. Media naming uses `stem` (without suffix). **Verify** in ES-DE's source (`es-app/src/FileData.cpp`, directories interpreted as files) and Batocera's scraper that the media of `Game.ps3/` is named `Game`; if one names it `Game.ps3`, keep the suffix in `stem` for that profile and add a `BLOCKED:`/note line to the commit body (spec D26, §13). Hashing, peeking and archive checks: `if rom.is_dir { skip }` at each entry point (`grep -n "fn .*RomEntry\|&RomEntry" crates/coverly-core/src/{fill,identify,audit,dedupe}`).
- [ ] **Step 4: Run** the tests, `cargo test -p coverly-core` (review any scan snapshot diff: none expected), G-RUST, G-WEB.
- [ ] **Step 5: Commit and push:** `git add -A crates src-tauri && git commit -m "feat(core): list game folders as one library entry" && git push && git status -sb`

---

## Task 7: GameCube and Wii (disc formats and WiiWare WAD)

**Files:**
- Create: `crates/coverly-core/src/consoles/nintendo_disc.rs`, `crates/coverly-core/src/consoles/wad.rs`
- Modify: `consoles/mod.rs` (`identify` order), `consoles/testing.rs` (builders), `identify/sniff.rs` (call `consoles::identify`, P-1), `systems/systems.toml`, `profiles/builtin.rs`, `identify/dat.rs` (ID4 middle key), `identify/hash.rs` (`NAME_ONLY_EXTS`), `organize/evidence.rs` (console evidence contradicting `doom`/`gamecom`)
- Test: `tests/systems_batch2.rs`, `tests/sniff.rs`, `tests/organize_evidence.rs`, `tests/dat.rs`, `tests/systems_classes.rs` snapshot

**TOML rows** (spec §7; `multi_file = true`, explicit `dats`):

```toml
[[system]]
id = "gamecube"
name = "Nintendo GameCube"
family = "console"
aliases = ["GAMECUBE", "gc", "ngc", "gcn"]
# `gcz rvz wia ciso` are shared with wii: the disc header decides (D7).
# `tgc` stays gamecom's; a TGC magic proves GameCube (D24).
extensions = ["gcm", "iso", "ciso", "gcz", "rvz", "wia", "m3u"]
weak_extensions = ["tgc", "dol"]
libretro = "Nintendo - GameCube"
dats = ["dat/Nintendo - GameCube.dat", "metadat/redump/Nintendo - GameCube.dat", "metadat/homebrew/Nintendo - GameCube.dat"]
multi_file = true
screenscraper = 13

[[system]]
id = "wii"
name = "Nintendo Wii"
family = "console"
aliases = ["WII"]
# `wad` stays doom's; a Wii WAD header proves Wii (D7, D24).
extensions = ["wbfs", "iso", "ciso", "gcz", "rvz", "wia", "m3u"]
weak_extensions = ["wad", "dol"]
libretro = "Nintendo - Wii"
dats = ["dat/Nintendo - Wii.dat", "metadat/redump/Nintendo - Wii.dat", "metadat/no-intro/Nintendo - Wii (Digital).dat"]
multi_file = true
screenscraper = 16
```

  Aliases match case-insensitively, so the upper-case first alias (D8) also covers the lower-case folder names `gamecube`, `wii`, `ps3`… of spec §6; `profile_folders.rs` asserts `resolve_folder("gamecube")` and `("gc")` give gamecube. Check `iso` stays Generic in the class snapshot (it is listed by many systems already). `rvz` is already in `NAME_ONLY_EXTS`; add `gcz wia wbfs ciso`.

**Folder rows** (`SYSTEM_FOLDERS`, columns `[esde, batocera, muos, onion, minui]`):

```rust
    // Systems batch 2 (spec §6).
    ("gamecube", [Some("gc"), Some("gamecube"), None, None, None]),
    ("wii", [Some("wii"), Some("wii"), None, None, None]),
```

**Reader** (`consoles/nintendo_disc.rs`; offsets from spec §8.1, Dolphin `DiscIO`):

```rust
//! GameCube and Wii disc headers through every container (spec §8.1).
//! Only the 0x80-byte disc header is read; it is in clear on every disc.

use super::{Budgeted, ConsoleId, IdSource};

const WII_MAGIC: [u8; 4] = [0x5D, 0x1C, 0x9E, 0xA3];
const GC_MAGIC: [u8; 4] = [0xC2, 0x33, 0x9F, 0x3D];
const HEADER: usize = 0x80;

/// The disc header at the start of `h` (≥ 0x80 bytes).
pub fn disc_header(h: &[u8], detail: &str) -> Option<ConsoleId> {
    let h = h.get(..HEADER)?;
    let system = if h[0x18..0x1C] == WII_MAGIC {
        "wii"
    } else if h[0x1C..0x20] == GC_MAGIC {
        "gamecube"
    } else {
        return None;
    };
    let id6 = std::str::from_utf8(&h[..6]).ok()?;
    let serial = crate::disc::serial::normalise_console_serial(system, id6)?;
    let mut id = ConsoleId::proven(system, detail);
    id.title_id = Some(serial.clone());
    id.serial = Some(serial);
    id.id_source = Some(IdSource::Header);
    id.disc_no = Some(h[0x06].saturating_add(1)); // 0-based on disc
    id.version = Some(h[0x07].to_string());
    id.title = super::clean_title(&String::from_utf8_lossy(&h[0x20..0x60]));
    Some(id)
}

pub fn identify(b: &mut Budgeted) -> Option<ConsoleId> {
    let head = b.read(0, 0x100.min(b.len() as usize)).ok()?;
    match head.get(..4)? {
        b"CISO" => ciso(b, &head),
        [0x01, 0xC0, 0x0B, 0xB1] => gcz(b, &head),  // 0xB10BC001 LE (Dolphin CompressedBlob.h)
        b"WBFS" => wbfs(b, &head),
        b"WIA\x01" => wia(b, "WIA disc header"),
        b"RVZ\x01" => wia(b, "RVZ disc header"),
        [0xAE, 0x0F, 0x38, 0xA2] => tgc(b, &head),
        _ => disc_header(&head, "disc header"),
    }
}

/// Nintendo CISO vs PSP CSO (same magic): block size at 4 is a power of
/// two ≥ 0x8000 and the map's first 16 bytes are all 0/1.
pub fn is_nintendo_ciso(head: &[u8]) -> bool {
    let bs = head.get(4..8).map(|b| u32::from_le_bytes(b.try_into().unwrap()));
    matches!(bs, Some(bs) if bs >= 0x8000 && bs.is_power_of_two())
        && head.get(8..0x18).is_some_and(|m| m.iter().all(|&x| x <= 1))
}

fn ciso(b: &mut Budgeted, head: &[u8]) -> Option<ConsoleId> {
    if !is_nintendo_ciso(head) || head[8] != 1 { return None; }
    disc_header(&b.read(0x8000, HEADER).ok()?, "CISO disc header")
}

fn gcz(b: &mut Budgeted, head: &[u8]) -> Option<ConsoleId> {
    let u32le = |o: usize| u32::from_le_bytes(head[o..o + 4].try_into().unwrap());
    let sub_type = u32le(4);
    let block_size = u64::from(u32le(0x18));
    let blocks = u64::from(u32le(0x1C));
    // pointers (u64) then hashes (u32) follow the 0x20-byte header.
    let table = blocks.checked_mul(12)?;
    if blocks == 0 || block_size == 0 || 0x20 + table > b.len() { return None; }
    let p0 = u64::from_le_bytes(b.read(0x20, 8).ok()?.try_into().ok()?);
    let stored = p0 & (1 << 63) != 0;
    let start = 0x20u64.checked_add(table)?.checked_add(p0 & !(1 << 63))?;
    let header = if stored {
        b.read(start, HEADER).ok()?
    } else {
        let p1 = if blocks > 1 { u64::from_le_bytes(b.read(0x28, 8).ok()?.try_into().ok()?) & !(1 << 63) } else { b.len() - 0x20 - table };
        let clen = p1.checked_sub(p0 & !(1 << 63))?.min(block_size).min(super::MAX_DECODED);
        let raw = b.read(start, usize::try_from(clen).ok()?).ok()?;
        inflate_prefix(b, &raw, HEADER)?
    };
    let mut id = disc_header(&header, "GCZ disc header")?;
    let gcz_system = if sub_type == 1 { "wii" } else { "gamecube" };
    if gcz_system != id.system { id.detail = "GCZ disc header (sub-type disagrees)".into(); }
    Some(id)
}

/// Inflates zlib `raw` until `want` bytes, charging the budget.
fn inflate_prefix(b: &mut Budgeted, raw: &[u8], want: usize) -> Option<Vec<u8>> {
    use std::io::Read;
    let mut out = vec![0u8; want];
    let mut z = flate2::read::ZlibDecoder::new(raw);
    z.read_exact(&mut out).ok()?;
    b.decoded(want as u64).ok()?;
    Some(out)
}

fn wbfs(b: &mut Budgeted, head: &[u8]) -> Option<ConsoleId> {
    let shift = *head.get(8)?;
    if !(9..=12).contains(&shift) { return None; } // 512 B … 4 KiB sectors
    disc_header(&b.read(1u64 << shift, HEADER).ok()?, "WBFS disc header")
}

fn wia(b: &mut Budgeted, detail: &str) -> Option<ConsoleId> {
    let h = b.read(0, 0xD8).ok()?;
    let disc_type = u32::from_be_bytes(h[0x48..0x4C].try_into().ok()?);
    let mut id = disc_header(&h[0x58..0xD8], detail)?;
    let want = match disc_type { 1 => "gamecube", 2 => "wii", _ => id.system.as_str() };
    if want != id.system { id.detail = format!("{detail} (disc type disagrees)"); }
    Some(id)
}

fn tgc(b: &mut Budgeted, head: &[u8]) -> Option<ConsoleId> {
    // Verify against Dolphin TGCBlob.h: `tgc_header_size` u32 BE at 0x08,
    // the embedded GCM header starts there.
    let off = u64::from(u32::from_be_bytes(head.get(8..12)?.try_into().ok()?));
    if off > 0x1_0000 { return None; }
    let mut id = disc_header(&b.read(off, HEADER).ok()?, "TGC disc header")?;
    (id.system == "gamecube").then(|| { id.system = "gamecube".into(); id })
}
```

  `consoles/wad.rs`: header size u32 BE `0x20` at 0 and type `Is\0\0`/`ib\0\0` at 4; section sizes u32 BE at 0x08 (cert), 0x10 (ticket), 0x14 (TMD), 0x18 (data); `align64(x) = (x + 63) & !63`; cert at 0x40, ticket at `align64(0x40 + cert)`, TMD at `align64(ticket_off + ticket)`; read ≤ 0x2000 bytes of TMD (`tmd ≤ 0x2000` else magic-only); title id u64 BE at TMD + 0x18C; ID4 = the low 4 bytes when ASCII alphanumeric → `serial`; `title_id` = 16 hex; `kind = kind_wii(tid)`; title `None` (names come from the DAT); TMD-section CRC32 computed over the TMD bytes and returned in `detail` as `WAD TMD crc=XXXXXXXX` **only if** the No-Intro Wii (Digital) DAT match is verified on one real WAD head copied under `target/systems-sample/` (spec §8.1); otherwise drop the CRC part (`BLOCKED:` line). The ticket section is never read.

  Sniff (P-1): add `pub fn sniff_named(src, len, ext, name) -> Sniff` holding today's body, with `sniff(src, len, ext)` calling it with `""`; `inspect` calls `sniff_named` with the raw file name or the archive entry name. In it, after `discs(...)`: `if s.hits.is_empty() && s.unsupported.is_none() { if let Some(id) = crate::consoles::identify(&mut crate::consoles::Budgeted::new(src, len), &ext, name) { s.hit(&id.system, id.confidence, id.detail.clone()); s.serial = id.serial.clone(); s.console = Some(id); } }` — then cartridges only when still no hit. Add to `consoles::identify`: `nintendo_disc::identify` for any ext (the magics are specific), `wad::identify` when `ext == "wad"` or the 8-byte WAD header matches.

  Evidence: a console hit for `wii` on a `.wad` makes `doom` (strong owner) contradicted (`ItemNote::ExtensionContradicted { ext: "wad", system: Some("doom") }`) through batch 1's D24 rule (`weak owner proven by header ≥ 0.95`); same for `gamecube` on `.tgc` against `gamecom`. Confirm the rule picks this up without code (`grep -n "ExtensionContradicted" crates/coverly-core/src/organize/evidence.rs`); add only what is missing.

  DAT: in `identify/dat.rs`, when indexing serials, also index `RVL-XXXX-…` and `DOL-XXXX-…` under the 4-character middle `XXXX`; the serial lookup for `gamecube`/`wii` tries the ID6, then its first 4 characters. **Verify** with the trimmed fixtures below that the Redump serial form is `RVL-XXXX-REG`.

- [ ] **Step 0: Sync.**
- [ ] **Step 1: Failing tests.**
  - `consoles/testing.rs` builders (each returns `SparseAt`): `gc_disc(id6, magic_wii: bool)` (0x80-byte header with title `"Test Game"`), `ciso(inner)`, `psp_cso()` (header size 0x18 at 4), `gcz(inner, stored: bool)` (zlib-compresses one 0x8000 block with `flate2::write::ZlibEncoder`), `wbfs(inner)`, `wia(inner, rvz: bool, disc_type)`, `tgc(inner)`, `wii_wad(tid)`.
  - `consoles/nintendo_disc.rs` unit tests: one positive and one negative per format; `is_nintendo_ciso` both ways; disc byte 1 → `disc_no == Some(2)`; GCZ sub-type disagreeing → detail notes it; WBFS shift 30 → `None`; GCZ with `blocks = 0x8000_0000` → `None` within budget (`budget_gcz_adversarial`: `b.used().read <= MAX_READ`).
  - `consoles/wad.rs`: header, title id, ID4, kind; ticket range never read (`RangeLog`).
  - `systems_batch2.rs`: `SYSTEMS` rows `("gamecube", Family::Console, true)`, `("wii", Family::Console, true)`; `TARGETS` `("esde","gamecube","gc",None)`, `("batocera","gamecube","gamecube",None)`, `("muos","wii","wii",Some(false))`, `("onion","gamecube","GAMECUBE",Some(false))`, `("anbernic-stock","wii","WII",Some(false))`; `CONSOLE_HEADERS` rows for `iso` GC (`GM8E01`), `iso` Wii (`RMGE01`), `rvz`, `wia`, `gcz`, `ciso`, `wbfs`, `tgc` (GC) and `wad` (Wii, serial `WAAE`), plus `cso` from `psp_cso()` → `Some("psp")`-or-unchanged (assert the PSP path still answers as before).
  - `tests/organize_evidence.rs`: `wii_wad_contradicts_doom` (Wii WAD head in `.wad` → wii, note contradicts doom), `iwad_stays_doom`, `tgc_magic_contradicts_gamecom`, `gcz_without_header_is_shared_review` (zeros in `.gcz` → Review with gamecube and wii as alternatives).
  - `tests/dat.rs`: trimmed fixtures `tests/fixtures/dat/gametdb-gamecube.dat` (two `game` rows with `serial "GM8E01"`) and `redump-wii.dat` (one row `serial "RVL-RMGE-USA"`): ID6 `GM8E01` hits the first; ID6 `RMGE01` hits the Redump row through its middle key.
  - Class snapshot: `INSTA_UPDATE=always cargo test -p coverly-core --test systems_classes`, then read the diff: `gcz rvz wia ciso` Shared (gamecube, wii); `wad` strong doom + accepting wii; `tgc` strong gamecom + accepting gamecube; `wbfs gcm` Unique; `dol` weak only; `iso` Generic.
- [ ] **Step 2: Run** `cargo test -p coverly-core --lib consoles:: && cargo test -p coverly-core --test systems_batch2 --test sniff --test organize_evidence --test dat --test systems_classes` and expect failures.
- [ ] **Step 3: Implement** as above.
- [ ] **Step 4: Run** the tests and `cargo test -p coverly-core` (review every snapshot diff: plan snapshots with `.wad`/`.iso` candidates may gain or lose a header line; explain each in the commit body), G-RUST, G-WEB.
- [ ] **Step 5: Commit and push:** `git add -A crates && git commit -m "feat(core): identify GameCube and Wii discs and WiiWare WADs by their header" && git push && git status -sb`

---

## Task 8: Nintendo 3DS (NCSD, NCCH, CIA, 3DSX, Z3DS, SMDH)

**Files:**
- Create: `crates/coverly-core/src/consoles/ctr.rs`
- Modify: `consoles/mod.rs`, `consoles/testing.rs`, `identify/sniff.rs` (`DECISIVE` += `3ds`, now 15), `systems/systems.toml`, `profiles/builtin.rs`, `identify/hash.rs` (`NAME_ONLY_EXTS` += `cia zcci zcia`)
- Test: `tests/systems_batch2.rs`, `tests/sniff.rs`, `tests/organize_evidence.rs`, class snapshot

**TOML row:**

```toml
[[system]]
id = "3ds"
name = "Nintendo 3DS"
family = "handheld"
aliases = ["3DS", "n3ds", "nintendo3ds"]
# `3ds` is DECISIVE: an Autodesk `.3ds` has no NCSD magic (D7).
extensions = ["3ds", "cci", "cxi", "cia", "3dsx", "zcci", "zcxi", "zcia", "z3dsx"]
libretro = "Nintendo - Nintendo 3DS"
libretro_also = ["Nintendo - Nintendo 3DS (Digital)"]
dats = [
  "metadat/no-intro/Nintendo - Nintendo 3DS.dat",
  "metadat/no-intro/Nintendo - Nintendo 3DS (Digital).dat",
  "metadat/no-intro/Nintendo - New Nintendo 3DS.dat",
  "metadat/no-intro/Nintendo - New Nintendo 3DS (Digital).dat",
]
screenscraper = 17
```

  Check the two «New» paths in `libretro-database-dats.txt` first (`grep -n "New Nintendo 3DS" crates/coverly-core/tests/fixtures/research/libretro-database-dats.txt`); drop any that is missing and say so in the commit body.

**Folder row:** `("3ds", [Some("n3ds"), Some("3ds"), Some("3ds"), None, None]),`

**Reader** (`consoles/ctr.rs`; spec §8.2, 3dbrew):

```rust
const MEDIA: u64 = 0x200;

pub fn identify(b: &mut Budgeted, ext: &str) -> Option<ConsoleId> {
    let head = b.read(0, 0x200.min(b.len() as usize)).ok()?;
    if head.get(0x100..0x104) == Some(b"NCSD") { return ncsd(b, &head); }
    if head.get(0x100..0x104) == Some(b"NCCH") { return ncch_at(b, 0, "NCCH header"); }
    if head.get(..4) == Some(b"3DSX") { return dsx(b, &head); }
    if head.get(..4) == Some(b"Z3DS") { return z3ds(b, &head); }
    if ext == "cia" && head.get(..4) == Some(&0x2020u32.to_le_bytes()) { return cia(b, &head); }
    None
}

fn ncch_at(b: &mut Budgeted, at: u64, detail: &str) -> Option<ConsoleId> {
    let h = b.read(at, 0x200).ok()?;
    if &h[0x100..0x104] != b"NCCH" { return None; }
    let program = u64::from_le_bytes(h[0x118..0x120].try_into().ok()?);
    let code = std::str::from_utf8(&h[0x150..0x160]).ok()?;
    let mut id = ConsoleId::proven("3ds", detail);
    id.serial = crate::disc::serial::normalise_console_serial("3ds", code);
    id.title_id = Some(format!("{program:016X}"));
    id.id_source = Some(IdSource::Header);
    id.kind = super::kind_3ds(program);
    let no_crypto = h[0x188 + 7] & 0x04 != 0;
    id.encrypted = Some(!no_crypto);
    Some(id)
}

fn ncsd(b: &mut Budgeted, head: &[u8]) -> Option<ConsoleId> {
    let p0 = u64::from(u32::from_le_bytes(head[0x120..0x124].try_into().ok()?));
    let off = p0.checked_mul(MEDIA)?;
    ncch_at(b, off, "NCSD header").or_else(|| Some(ConsoleId::magic("3ds", "NCSD header")))
}
```

  `cia`: sizes at 0x08 (cert), 0x0C (ticket), 0x10 (TMD), 0x14 (meta), content size u64 at 0x18; `align64`; cert at `align64(0x2020)`, ticket after it, TMD after the ticket, content after the TMD, meta after the content. TMD: signature type u32 BE at 0 must be `0x0001_0004` (RSA-2048; header at +0x140), else magic-only; title id u64 BE at TMD+0x18C, version u16 BE at TMD+0x1DC, content count u16 BE at TMD+0x1DE, first content chunk record at TMD+0xB04 (type u16 BE at +6, bit 0 = encrypted). Not encrypted → `ncch_at(content_off)` for the product code; encrypted → TMD title id only, `encrypted: Some(true)`. Meta present (`meta_size ≥ 0x400 + 0x36C0`) → read the SMDH at meta+0x400 (`SMDH` magic): title = English long title (language 1, UTF-16LE at 0x8 + 1 × 0x200 + 0x80, 0x100 bytes; fallback Japanese 0), art `Range { offset: meta+0x400, len: 0x36C0, format: Smdh }`.
  `dsx`: header size u16 LE at 4 > 0x20 → SMDH offset/size u32 LE at 0x20/0x24 (bounded by the file) → title and art as above; confidence 0.90, no serial (homebrew).
  `z3ds`: **verify** the header in Azahar `src/common/file_derived.h` at `6e42de80` (underlying magic at 4, version u8 at 8, header size u16 at 0xA, metadata size u32 at 0xC, compressed/uncompressed u64 at 0x10/0x18) and read the underlying magic values from Azahar's writer; decode the first zstd frame after `header_size + metadata_size` with `ruzstd::decoding::StreamingDecoder` into a buffer capped at 1 MiB (`b.decoded`), then run `ncsd`/`ncch_at`/`cia`/`dsx` on a `SliceAt` of the decoded bytes. If verification fails: `Z3DS` magic → `3ds` magic-only (`BLOCKED:` line).
  SMDH icon decode (for Task 15): `pub fn smdh_icon_png(smdh: &[u8]) -> Option<image::RgbaImage>` — 48×48 RGB565 at 0x24C0, 8×8 tiles in Morton (Z) order; write it here with a unit test against a known 8×8 pattern (a tile whose pixel `i` has value `i`, checked at the de-swizzled coordinates).

- [ ] **Step 0: Sync.**
- [ ] **Step 1: Failing tests.** Builders `ncsd(code, tid, no_crypto)`, `ncch(...)`, `cia(tid, encrypted, with_meta)`, `dsx_with_smdh(title)`, `z3ds(inner)` (zstd via `ruzstd::encoding::compress_to_vec`), `smdh(title)`. Unit tests per format; `encrypted_cia_gives_title_id_only`; `cia_meta_title_and_art`; `smdh_tile_order`; budget with an adversarial partition offset (`0xFFFF_FFFF` × 0x200) → magic-only. Harness: `SYSTEMS` `("3ds", Family::Handheld, false)`; `TARGETS` `("esde","3ds","n3ds",None)`, `("muos","3ds","3ds",None)`, `("batocera","3ds","3ds",None)`, `("minui","3ds","3DS",Some(false))`; `CONSOLE_HEADERS` for `3ds` (`CTR-P-AMKP`), `cci`, `cxi`, `cia` (unencrypted: product code; encrypted: no serial), `3dsx`, `zcci`. `tests/organize_evidence.rs` `autodesk_3ds_never_auto` (`NotARom::Model3d` from Task 3) and `unknown_3ds_is_0_60` (zeros, no `4D 4D`) → not Auto. `ItemNote::EncryptedContent { emulator: "Azahar" }` on an encrypted CIA item (`tests/organize_plan.rs` small pile). Class snapshot diff: the nine extensions Unique to `3ds`.
- [ ] **Step 2: Run** `cargo test -p coverly-core --lib consoles::ctr && cargo test -p coverly-core --test systems_batch2 --test organize_evidence --test organize_plan --test systems_classes` and expect failures.
- [ ] **Step 3: Implement** as above; the plan adds `EncryptedContent { emulator: "Azahar" }` when `console.system == "3ds" && console.encrypted == Some(true)` (fixed string, never from a file).
- [ ] **Step 4: Run** tests, G-RUST, G-WEB.
- [ ] **Step 5: Commit and push:** `git add -A crates && git commit -m "feat(core): identify Nintendo 3DS images from their NCSD, NCCH and CIA headers" && git push && git status -sb`

---

## Task 9: Nintendo Wii U (folders, NUS, WUD/WUX, WUA, RPX/WUHB) and ZArchive

First real folder probes (Task 4 machinery).

**Files:**
- Create: `crates/coverly-core/src/consoles/wiiu.rs`, `crates/coverly-core/src/consoles/zarchive.rs`
- Modify: `consoles/mod.rs` (`DIR_PROBES` += `wiiu::probe_loadiine`, `wiiu::probe_nus`), `consoles/testing.rs`, `systems/systems.toml`, `profiles/builtin.rs`, `identify/dat.rs` (Wii U ID4 key), `identify/hash.rs` (`wua wux wud`)
- Test: `tests/systems_batch2.rs`, `tests/organize_folder_games.rs`, `tests/organize_walk.rs`

**TOML row:**

```toml
[[system]]
id = "wiiu"
name = "Nintendo Wii U"
family = "console"
aliases = ["WIIU"]
extensions = ["wud", "wux", "wua", "rpx", "wuhb"]
weak_extensions = ["tmd"]
libretro = "Nintendo - Wii U"
dats = ["dat/Nintendo - Wii U.dat", "metadat/no-intro/Nintendo - Wii U (Digital).dat"]
screenscraper = 18
```

**Folder row:** `("wiiu", [Some("wiiu"), Some("wiiu"), None, None, None]),`

**Reader notes** (`consoles/wiiu.rs`; spec §8.2):
- `parse_meta_xml(bytes) -> Option<ConsoleId>`: refuse input > `MAX_META_FILE`; `quick_xml::Reader::from_reader(bytes)` with `config_mut().expand_empty_elements = false`; **reject** any `Event::DocType` (no DTD, no entities) and any `&` entity other than the five predefined ones; read `title_id` (16 hex), `title_version`, `product_code` (`WUP-P-ARPE` → serial `ARPE` via `normalise_console_serial`), `longname_en` (fallback `shortname_en`, then `longname_ja`); `kind_wiiu(title_id)`; art `File { rel: "meta/iconTex.tga", format: Tga }` when `symlink_metadata` shows that file. `id_source = MetaXml`.
- `parse_app_xml`: `title_id`, `title_version` only (`id_source = AppXml`).
- `probe_loadiine(listing)`: needs `code` and (`meta` or `content`) among `listing.names` (case-insensitive); reads `meta/meta.xml` (≤ 64 KiB, budgeted, `symlink_metadata` first: a symlink → no match) else `code/app.xml`. `probe = "meta/meta.xml"` or `"code/app.xml"`.
- `probe_nus(listing)`: `title.tmd` among the names; read ≤ 0x2000 bytes; TMD signature type `0x0001_0004` → title id u64 BE at 0x18C; high word `0x0005_xxxx` → wiiu (`detail: "NUS title.tmd"`, no title), else `None` (3DS CDN / Wii NUS stay in review, spec D12). `title.tik`/`title.cetk` are never read.
- File formats: `WUX0` + `[0x2E, 0xD0, 0x99, 0x10]` at 4 → sector size u32 LE at 8 (power of two, 0x8000 expected), index table at 0x20, data at `align(0x20 + 4 × sectors, sector_size)`; read sector 0 through index[0]; product code check as WUD. WUD: **verify** on one real WUD head that it starts with `WUP-P-` (spec §8.2); if verified, serial from the first 10 bytes, else `.wud` is extension evidence only (`BLOCKED:` line). RPX: ELF magic and `e_ident[7..9] == [0xCA, 0xFE]` → 0.90 no id. WUHB `WUHB` at 0 → 0.90.
- `consoles/zarchive.rs`: **verify** `zarchivecommon.h` (MIT-0) footer layout before coding: footer = 6 × `{u64 offset, u64 size}` (compressed data, offset records, names, file tree, meta directory, meta data) + 32-byte hash + `u64 total_size` + `u32 version` (`0x61BF3A01`) + `u32 magic` (`0x169F52D6`), all big-endian, at the end of the file (0x90 bytes). Read the footer, the names section and the file tree (each ≤ 256 KiB, budgeted), list the root's directories `<16hex>_v<n>` (WUA) → one `ConsoleId` for the base (kind from its id; versions listed in `detail` as `WUA: base, update, dlc`), then decompress the block(s) holding the base's `meta/meta.xml` (64 KiB zstd blocks; offset records give each block's position: **verify** the record shape `u64 base + 16 × u16 (size − 1)`) with at most 256 KiB decoded for this file, charged through `Budgeted::decoded` (the 1 MiB D24 cap still applies). Failing verification → `.wua` by footer magic only, no id.

- [ ] **Step 0: Sync.**
- [ ] **Step 1: Failing tests.** Builders `meta_xml(title_id, product, name)`, `wiiu_folder(dir)` (on disk: `code/app.xml`, `code/game.rpx` 64 bytes, `content/x.bin`, `meta/meta.xml`, `meta/iconTex.tga` 18-byte TGA header), `nus_folder(dir, tid)`, `wux(inner)`, `zarchive(entries)` (a tiny writer in `testing.rs` that follows the verified layout with one zstd block from `ruzstd::encoding::compress_to_vec`). Unit tests: `meta_xml_fields`, `meta_xml_with_doctype_is_rejected`, `meta_xml_over_64k_is_rejected`, `app_xml_title_id`, `nus_3ds_tmd_is_not_a_folder_game`, `wux_sector0`, `zarchive_lists_titles_and_reads_meta`, `zarchive_adversarial_footer` (offsets past EOF) → magic-only. Integration (`tests/organize_folder_games.rs`): a pile `Super Mario 3D World [0005000010145D00]/` with the loadiine tree → one `Folder` item to `wiiu/Super Mario 3D World [0005000010145D00]` on ES-DE, `why` carries `title_id` `0005000010145D00` with `id_source = MetaXml`; `code/game.rpx` is **not** a separate item; a `title.tmd` folder with a 3DS id is not a folder item. Harness: `SYSTEMS` `("wiiu", Family::Console, false)`; `TARGETS` `("esde","wiiu","wiiu",None)`, `("muos","wiiu","wiiu",Some(false))`; `CONSOLE_HEADERS` `wux`, `wua`, `rpx`, `wuhb`. Library (`tests/scan_fixtures.rs`): the moved folder is one `RomEntry` (Task 6 rule with the real probe).
- [ ] **Step 2: Run** `cargo test -p coverly-core --lib consoles::wiiu consoles::zarchive && cargo test -p coverly-core --features test-util --test organize_folder_games && cargo test -p coverly-core --test systems_batch2 --test scan_fixtures --test systems_classes` and expect failures.
- [ ] **Step 3: Implement** as above; register the probes in `DIR_PROBES` in the D12 order (this task adds the two Wii U entries after the PS3/Vita slots, which Task 11 fills; keep the array order equal to spec D12 with a comment).
- [ ] **Step 4: Run** tests, G-RUST, G-WEB.
- [ ] **Step 5: Commit and push:** `git add -A crates && git commit -m "feat(core): identify Wii U folders, NUS titles, WUX and WUA archives" && git push && git status -sb`

---

## Task 10: Nintendo Switch (XCI, XCZ, NSP, NSZ, split folders) — only what is in clear

**Files:**
- Create: `crates/coverly-core/src/consoles/nx.rs`
- Modify: `consoles/mod.rs` (`DIR_PROBES` += `nx::probe_split`), `consoles/testing.rs`, `systems/systems.toml`, `profiles/builtin.rs`, `identify/hash.rs` (`nsp nsz xci xcz`), `organize/plan.rs` (update/DLC notes and ordering, summary counts)
- Test: `tests/systems_batch2.rs`, `tests/organize_plan.rs`, `tests/organize_folder_games.rs`

**TOML row:**

```toml
[[system]]
id = "switch"
name = "Nintendo Switch"
family = "console"
# `switch` is an everyday word: scoped (D8). No libretro folder and no
# DAT exist (D18): header for the system, name for the rest (D10).
aliases = ["SWITCH", "nintendoswitch", "nsw"]
scoped_aliases = ["switch"]
extensions = ["xci", "nsp", "nsz", "xcz"]
weak_extensions = ["nro", "nso", "nca"]
screenscraper = 225
```

  `is_scoped_alias` compares case-insensitively (batch 1), so `switch` scopes the first alias `SWITCH` too (spec D8). If `check_builtin_invariants` compares scoped aliases to aliases case-sensitively, make that comparison case-insensitive rather than listing both spellings.

**Folder row:** `("switch", [Some("switch"), Some("switch"), None, None, None]),` with a comment: «Batocera has no `switch` key at `a3d49969`; Knulli and RetroBat do. Named for the shared Batocera/Knulli profile (spec D19).»

**What is and isn't possible (spec §8.3), enforced by the code and its tests:**
- XCI/XCZ: `HEAD` at 0x100 → `switch` 0.95; root HFS0 at the u64 LE at 0x130 (bounded), entry names read for the detail (`secure`, `normal`, `update`); the gamecard info at 0x190 and every NCA are **never read**. Title id only from the file name.
- NSP/NSZ: `PFS0` at 0; `count` u32 LE at 4 (≤ 1024), string table size u32 at 8 (≤ 64 KiB); entries 0x18 bytes from 0x10 (`offset u64, size u64, name_off u32, _ u32`); names from the string table at `0x10 + 0x18 × count`. Title id from a `<32hex>.tik` **entry name** (`id_source = TicketName`, first 16 hex), else a `*.cnmt.xml` entry body (≤ 64 KiB; `<Id>0x…</Id>`, `<Version>`, `<Type>`; `id_source = CnmtXml`), else the file name. `.tik`, `.cert`, `.nca`, `.ncz` bodies are never read.
- File name: `\[(0100[0-9A-Fa-f]{12})\]` → title id (`id_source = FileName`), `\[v(\d+)\]` → version. Parse by hand (no regex crate): scan for `[`, check 16 hex, `]`.
- Kind from `kind_switch`; DLC/update base = `switch_base`.
- Split «archive bit» folder: a directory named `*.nsp`/`*.xci` whose names include `00` and `01` → read the `00` part's head through the same rules; `detail = "split NSP folder"` (Task 4 skips `TooBigForFat` for it).

```rust
pub fn title_id_from_name(name: &str) -> Option<(u64, Option<u32>)> {
    let bytes = name.as_bytes();
    let mut tid = None;
    let mut ver = None;
    for (i, _) in name.match_indices('[') {
        let rest = &bytes[i + 1..];
        if rest.len() >= 17 && rest[16] == b']' && rest[..16].iter().all(u8::is_ascii_hexdigit) {
            let s = std::str::from_utf8(&rest[..16]).ok()?;
            if s.starts_with("01") { tid = u64::from_str_radix(s, 16).ok(); }
        } else if rest.first() == Some(&b'v') {
            let digits: String = rest[1..].iter().take_while(|b| b.is_ascii_digit()).map(|&b| b as char).collect();
            if !digits.is_empty() && rest.get(1 + digits.len()) == Some(&b']') { ver = digits.parse().ok(); }
        }
    }
    tid.map(|t| (t, ver))
}
```

  Plan side (spec D11, §9.1): when a `switch`, `3ds`, `wiiu`, `ps3`, `psvita` or `xbox360` item has `kind` `Update`/`Dlc`, find a `Base` item of the same system in the plan whose base id matches (`switch_base`, the 3DS/Wii U low 32 bits, the Sony `TITLE_ID`, the Xbox 360 title id) → `ItemNote::Update { of: base title or base file stem }` / `Dlc { … }`; no base → `of` = the base id in hex. Items of a system group are sorted so updates/DLC follow their base. `OrganizePlan` gains `updates: usize` (rows with an `Update`/`Dlc` note) for the summary sentence (`#[serde(default)]`).

- [ ] **Step 0: Sync.**
- [ ] **Step 1: Failing tests.** Builders `xci(names)`, `nsp(entries: &[(&str, &[u8])])`, `nsz(...)`. Unit tests: `xci_is_switch_without_title_id`, `nsp_ticket_name_gives_title_id`, `nsp_cnmt_xml_gives_id_and_version`, `nsz_entries_ncz`, `file_name_tokens` (`"Mario [0100000000010000][v65536].nsp"`, no token, a 16-hex token not starting `01`, `[v]` alone), `pfs0_count_over_cap_is_magic_only`, and:

```rust
#[test]
fn nsp_ticket_and_nca_bytes_are_never_read() {
    let tik = "01000000000100000000000000000000.tik";
    let mut f = testing::nsp(&[("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.nca", &[0x55; 4096]), (tik, &[0x66; 0x2C0]), ("x.cnmt.xml", b"<ContentMeta><Id>0x0100000000010000</Id><Version>0</Version></ContentMeta>")]);
    let len = f.len;
    let (nca, tik_range) = (f.entry_range(0), f.entry_range(1));
    let mut log = testing::RangeLog { inner: &mut f, ranges: Vec::new() };
    let id = super::identify(&mut Budgeted::new(&mut log, len), "nsp", "Mario.nsp").unwrap();
    assert_eq!(id.title_id.as_deref(), Some("0100000000010000"));
    for (o, n) in &log.ranges {
        let r = *o..*o + *n as u64;
        assert!(!overlaps(&r, &nca) && !overlaps(&r, &tik_range), "read {r:?}");
    }
}
```

  (`SparseAt::entry_range(i)` is a helper the `nsp` builder records.) The file name reaches `nx` through `consoles::identify(src, ext, name)` and `sniff_named` (Task 7).
  Plan (`tests/organize_plan.rs` `switch_update_is_listed_under_its_base`): `Super Mario Odyssey [0100000000010000][v0].nsp` and `… [0100000000010800][v393216].nsp` → both in `switch/`, the update right after the base with `ItemNote::Update { of: "Super Mario Odyssey" }`, `plan.updates == 1`; an orphan DLC gets `of: "0100000000010000"`. Folder: `Game.nsp/00` + `01` → one `Folder` item, no `TooBigForFat` on a FAT32 target (part sizes faked through the metadata path, never written). Harness: `SYSTEMS` `("switch", Family::Console, false)`; `TARGETS` `("esde","switch","switch",None)`, `("batocera","switch","switch",None)`, `("muos","switch","SWITCH",Some(false))`; `scoped`: `resolve_folder("switch")` is `None`, `resolve_folder_in(esde, "switch")` is `switch`; cover lookup for switch skips libretro (no folder) and asks ScreenScraper (existing mock) by name.
- [ ] **Step 2: Run** `cargo test -p coverly-core --lib consoles::nx && cargo test -p coverly-core --test systems_batch2 --test organize_plan --test profile_folders && cargo test -p coverly-core --features test-util --test organize_folder_games` and expect failures.
- [ ] **Step 3: Implement** as above.
- [ ] **Step 4: Run** tests, G-RUST, G-WEB.
- [ ] **Step 5: Commit and push:** `git add -A crates && git commit -m "feat(core): identify Switch dumps from their clear tables and file names only" -m "No key is required, read or shipped; XCI title ids come from the file name only (spec §8.3)." && git push && git status -sb`

---

## Task 11: PARAM.SFO, PS3 and PS Vita (JB/HDD/Vita folders, PS3 ISO, PKG, VPK, launcher text)

**Files:**
- Create: `crates/coverly-core/src/consoles/psf.rs`, `crates/coverly-core/src/consoles/sony.rs`
- Modify: `consoles/mod.rs` (`DIR_PROBES` += PS3 JB, PS3 HDD, Vita app — first in D12 order), `consoles/testing.rs`, `identify/sniff.rs` (`TEXT_NATIVE_EXTS` += `psvita psn`, now 16), `organize/inspect.rs` (`PACKAGE_EXTS` += `vpk`, read `sce_sys/param.sfo` entry), `systems/systems.toml`, `profiles/builtin.rs` (+ `folder_game_suffix` `ps3 → .ps3` on esde and batocera), `identify/hash.rs` (`vpk pkg`), `organize/plan.rs` (`InstallFirst`)
- Test: `tests/systems_batch2.rs`, `tests/organize_folder_games.rs`, `tests/organize_inspect.rs`, `tests/sniff.rs`

**TOML rows:**

```toml
[[system]]
id = "psvita"
name = "Sony PlayStation Vita"
family = "handheld"
aliases = ["PSVITA", "vita"]
scoped_aliases = ["vita"]
extensions = ["vpk", "psvita"]
weak_extensions = ["psvimg", "pkg"]
libretro = "Sony - PlayStation Vita"
dats = ["metadat/no-intro/Sony - PlayStation Vita.dat", "metadat/no-intro/Sony - PlayStation Vita (PSN).dat"]
screenscraper = 62

[[system]]
id = "ps3"
name = "Sony PlayStation 3"
family = "console"
aliases = ["PS3", "playstation3"]
# `pkg` is shared with Vita and every macOS installer: magic decides (D7).
extensions = ["iso", "ps3", "psn", "m3u"]
weak_extensions = ["pkg"]
libretro = "Sony - PlayStation 3"
libretro_also = ["Sony - PlayStation 3 (Downloadable)"]
dats = ["dat/Sony - PlayStation 3.dat", "metadat/redump/Sony - PlayStation 3.dat", "metadat/no-intro/Sony - PlayStation 3 (PSN).dat"]
multi_file = true
screenscraper = 59
```

**Folder rows:** `("psvita", [Some("psvita"), Some("psvita"), None, None, None]),` `("ps3", [Some("ps3"), Some("ps3"), None, None, None]),`; `fn builtin_profiles` sets `folder_game_suffix = {"ps3": ".ps3"}` on the `esde` and `batocera` profiles only (spec D13, sources: Batocera `es_systems.yml` comment and ES-DE `es_systems.xml` `.ps3`/`.ps3dir`).

**PSF reader** (`consoles/psf.rs`; spec §8.4, psdevwiki):

```rust
pub struct Psf { pub entries: Vec<(String, PsfValue)> }
pub enum PsfValue { Text(String), Int(u32) }
impl Psf { pub fn text(&self, key: &str) -> Option<&str>; }

pub fn parse(b: &[u8]) -> Option<Psf> {
    if b.len() > super::MAX_META_FILE as usize || b.get(..4)? != b"\0PSF" { return None; }
    let u32le = |o: usize| b.get(o..o + 4).map(|x| u32::from_le_bytes(x.try_into().unwrap()));
    let key_start = u32le(8)? as usize;
    let data_start = u32le(0xC)? as usize;
    let count = u32le(0x10)? as usize;
    if count > 256 || key_start > b.len() || data_start > b.len() { return None; }
    let mut entries = Vec::with_capacity(count);
    for i in 0..count {
        let e = 0x14 + i * 16;
        let key_off = u16::from_le_bytes(b.get(e..e + 2)?.try_into().ok()?) as usize;
        let fmt = u16::from_le_bytes(b.get(e + 2..e + 4)?.try_into().ok()?);
        let len = u32le(e + 4)? as usize;
        let data_off = u32le(e + 12)? as usize;
        let key_bytes = b.get(key_start.checked_add(key_off)?..)?;
        let key = std::str::from_utf8(&key_bytes[..key_bytes.iter().position(|&c| c == 0)?]).ok()?;
        if !key.is_ascii() || len > 1024 { return None; }
        let at = data_start.checked_add(data_off)?;
        let raw = b.get(at..at.checked_add(len)?)?;
        let value = match fmt {
            0x0404 => PsfValue::Int(u32::from_le_bytes(raw.get(..4)?.try_into().ok()?)),
            0x0004 | 0x0204 => PsfValue::Text(String::from_utf8_lossy(raw).trim_end_matches('\0').to_string()),
            _ => continue,
        };
        entries.push((key.to_string(), value));
    }
    Some(Psf { entries })
}
```

**Sony reader** (`consoles/sony.rs`):
- `from_psf(psf, system, detail) -> ConsoleId`: `TITLE_ID` → `serial` via `normalise_console_serial`, `title_id` raw; `TITLE` → `clean_title`; `APP_VER`/`VERSION` → version; `CATEGORY`: PS3 `DG`/`HG` Base, `GD` Update; Vita `gd` Base, `gp` Update; `id_source = ParamSfo`.
- `probe_ps3_jb(listing)`: `PS3_GAME` among the names → one `read_dir` of it (≤ 256 entries) to find `PARAM.SFO` case-insensitively → read (≤ 64 KiB) → `from_psf(…, "ps3", "PARAM.SFO")`; art `File { rel: "PS3_GAME/ICON0.PNG", Png }` when present. `probe_ps3_hdd`: `PARAM.SFO` + `USRDIR` directory among the names. `probe_vita_app`: `sce_sys` → its `param.sfo`; art `sce_sys/icon0.png`; `sce_sys/package/work.bin` is never touched.
- PS3 ISO (`identify` for ext `iso`): the existing ISO 9660 PVD at sector 16 with volume id `PS3VOLUME` (read 2 KiB at 0x8000 through `Budgeted`; check the field at PVD+40, 32 bytes); find `PS3_GAME` in the root with the existing `disc::iso9660` helpers (`grep -n "pub fn find_root_file\|pub(crate) fn" crates/coverly-core/src/disc/iso9660.rs`; add a `find_in_dir(src, dir_extent, name)` if only root lookup exists, bounded by `MAX_ROOT_RECORDS`), then `PARAM.SFO` inside it; bytes not starting `\0PSF` (encrypted Redump region) → **verify** the sector-1 plaintext id (`PlayStation3` at 0x800, title id at 0x810) on a real Redump head copy; verified → serial from it, else magic-only (`BLOCKED:` line).
- PKG: `7F 50 4B 47` at 0; type u16 BE at 6 (`1` → ps3, `2` → psvita); content id at 0x30 (0x24 bytes); title id = `content_id[7..16]`, checked as 4 letters + 5 digits; `ItemNote::InstallFirst { emulator: "RPCS3" | "Vita3K" }` in the plan for any PKG item. `.rap`/zRIF never read.
- VPK: `inspect` treats `vpk` as a package (`PACKAGE_EXTS`): `package_hits` opens the zip read-only, finds `sce_sys/param.sfo` (≤ 64 KiB decoded, the existing zip limits), `from_psf(…, "psvita", "VPK param.sfo")`; art `ZipEntry { name: "sce_sys/icon0.png", Png }`. Never extracted.
- `.psvita`/`.psn` text-native: ≤ 64 bytes; when the trimmed text is `AAAA00000` → that title id (`detail: "launcher file"`), system by extension.
- `.psvimg`: weak, no reader (encrypted backup).

- [ ] **Step 0: Sync.**
- [ ] **Step 1: Failing tests.** Builders `param_sfo(&[(key, value)])`, `ps3_jb(dir, title_id, category)`, `vita_app(dir, title_id)`, `ps3_iso(title_id)` (use `disc/testing.rs` ISO builder with a `PS3_GAME` sub-directory), `pkg(kind, content_id)`, `vpk(path, title_id)` (zip via the `zip` crate). PSF unit tests: valid; each bound violated (count 257, key offset past end, length 1025, non-ASCII key, data offset past end) → `None`; `fmt` values. Folder integration: `Demon's Souls (BLUS-30443)/PS3_GAME/PARAM.SFO` + `USRDIR/EBOOT.BIN` → one `Folder` item to `ps3/Demon's Souls (BLUS-30443).ps3` on ES-DE and Batocera with `FolderSuffix`, to `PS3/Demon's Souls (BLUS-30443)` without suffix on Onion (Absent note); `PCSE00844/sce_sys/param.sfo` → `psvita/PCSE00844`; a probe path that is a symlink (`#[cfg(unix)]`) → no match; a folder with both `PS3_GAME/` and `default.xbe` → conflict → Review (after Task 12 adds Xbox; write the test here with the P-7 extra probe as the second system). Inspect: `NPUB30024.pkg` → ps3 with `InstallFirst { emulator: "RPCS3" }`; a VPK → psvita, the zip is never extracted (`Transfer::AsIs`); `Foo.pkg` with `xar!` still `NotARom::Installer`. Sniff: `.psvita` text `PCSB00245\n` → psvita with title id. Harness: `SYSTEMS` `("psvita", Family::Handheld, false)`, `("ps3", Family::Console, true)`; `TARGETS` `("esde","ps3","ps3",None)`, `("batocera","psvita","psvita",None)`, `("muos","ps3","ps3",Some(false))`; `CONSOLE_HEADERS` `pkg` (PS3, Vita), `iso` (PS3 ISO, serial `BLUS-30443`); `scoped`: `resolve_folder("vita")` is `None`. Library: `ps3/Game.ps3/PS3_GAME/PARAM.SFO` → one `RomEntry` with `stem` `Game` (Task 6 verification result applies). Class snapshot: `pkg` weak on ps3 and psvita, `vpk psvita ps3 psn` Unique.
- [ ] **Step 2: Run** `cargo test -p coverly-core --lib consoles::psf consoles::sony && cargo test -p coverly-core --features test-util --test organize_folder_games && cargo test -p coverly-core --test systems_batch2 --test organize_inspect --test sniff --test scan_fixtures --test systems_classes` and expect failures.
- [ ] **Step 3: Implement** as above.
- [ ] **Step 4: Run** tests, G-RUST, G-WEB.
- [ ] **Step 5: Commit and push:** `git add -A crates && git commit -m "feat(core): identify PS3 and PS Vita games from PARAM.SFO, PKG and VPK headers" && git push && git status -sb`

---

## Task 12: Xbox and Xbox 360 (XDVDFS, XBE, XEX2, STFS/GOD, ZAR)

**Files:**
- Create: `crates/coverly-core/src/consoles/xbox.rs`
- Modify: `consoles/mod.rs` (`DIR_PROBES` += `default.xbe`, `default.xex`, GOD/XBLA), `consoles/testing.rs`, `identify/sniff.rs` (`TEXT_NATIVE_EXTS` += `xbox360`, now 17), `systems/systems.toml`, `profiles/builtin.rs`, `identify/dat.rs` (`dat_by_name_prefix`), `identify/hash.rs` (`zar`)
- Test: `tests/systems_batch2.rs`, `tests/organize_folder_games.rs`, `tests/dat.rs`

**TOML rows:**

```toml
[[system]]
id = "xbox"
name = "Microsoft Xbox"
family = "console"
aliases = ["XBOX"]
extensions = ["xiso", "iso", "m3u"]
weak_extensions = ["xbe"]
libretro = "Microsoft - Xbox"
dats = ["metadat/redump/Microsoft - Xbox.dat"]
multi_file = true
screenscraper = 32

[[system]]
id = "xbox360"
name = "Microsoft Xbox 360"
family = "console"
aliases = ["XBOX360", "x360"]
extensions = ["xex", "zar", "xbox360", "iso", "m3u"]
libretro = "Microsoft - Xbox 360"
dats = ["metadat/redump/Microsoft - Xbox 360.dat", "metadat/no-intro/Microsoft - Xbox 360.dat", "metadat/no-intro/Microsoft - Xbox 360 (Digital).dat"]
multi_file = true
screenscraper = 33
```

**Folder rows:** `("xbox", [Some("xbox"), Some("xbox"), None, None, None]),` `("xbox360", [Some("xbox360"), Some("xbox360"), None, None, None]),`

**Reader** (`consoles/xbox.rs`; spec §8.5):

```rust
const XDVDFS_MAGIC: &[u8; 20] = b"MICROSOFT*XBOX*MEDIA";
/// XISO, Redump XGD1, XGD2, XGD3 partition offsets (four 2 KiB reads at most).
const PARTITIONS: [u64; 4] = [0, 0x1830_0000, 0x0FD9_0000, 0x0208_0000];
const SECTOR: u64 = 2048;

/// The XBE certificate serial: publisher letters, a dash, three digits.
pub fn xbe_serial(title_id: u32) -> Option<String> {
    let (a, b) = ((title_id >> 24) as u8, (title_id >> 16) as u8);
    (a.is_ascii_alphanumeric() && b.is_ascii_alphanumeric())
        .then(|| format!("{}{}-{:03}", a as char, b as char, title_id & 0xFFFF))
}

pub fn xbe(bytes_at: &mut dyn FnMut(u64, usize) -> Option<Vec<u8>>) -> Option<ConsoleId> {
    let h = bytes_at(0, 0x178)?;
    if &h[..4] != b"XBEH" { return None; }
    let base = u32::from_le_bytes(h[0x104..0x108].try_into().ok()?);
    let cert = u32::from_le_bytes(h[0x118..0x11C].try_into().ok()?).checked_sub(base)?;
    if cert >= 0x1_0000 { return Some(ConsoleId::magic("xbox", "XBE header")); }
    let c = bytes_at(u64::from(cert), 0xB0)?;
    let tid = u32::from_le_bytes(c[8..12].try_into().ok()?);
    let name: Vec<u16> = c[0xC..0x5C].chunks_exact(2).map(|p| u16::from_le_bytes([p[0], p[1]])).collect();
    let mut id = ConsoleId::proven("xbox", "XBE certificate");
    id.serial = xbe_serial(tid);
    id.title_id = Some(format!("{tid:08X}"));
    id.title = super::clean_title(&String::from_utf16_lossy(&name));
    id.version = Some(u32::from_le_bytes(c[0xAC..0xB0].try_into().ok()?).to_string()); // verify +0xAC
    id.id_source = Some(IdSource::DefaultXbe);
    Some(id)
}
```

  - XDVDFS: for each partition `p`, read 0x20 bytes at `p + 0x10000`; magic → root sector u32 LE at +0x14, root size u32 LE at +0x18 (≤ 64 KiB); read the root table; walk the binary tree iteratively from offset 0 (entry: left u16 LE, right u16 LE — offsets in 4-byte units —, start sector u32, size u32, attributes u8, name length u8, name), with a visited set and ≤ 2048 entries; `default.xbe` (case-insensitive) → read its first 0x178 bytes + the certificate (`xbe` with `bytes_at = |o, n| b.read(p + sector × SECTOR + o, n)`); `default.xex` → `xex2` on it. Images are `iso`/`xiso`: system from what the tree holds; neither → `xbox` magic-only when the extension is `xiso`, else nothing.
  - `xex2(bytes_at)`: `XEX2` at 0; header count u32 BE at 0x14 (≤ 64); headers `(key u32 BE, value u32 BE)` from 0x18; key `0x0004_0006` → execution info at `value` (bounded): media id, version (u32 BE at +4), base version (+8), title id (u32 BE at +0xC), platform, executable type, disc number (+0x16), disc count (+0x17). `serial = title_id` as 8 hex. `id_source = DefaultXex` (in a folder or image) or `Header` (loose). `XEX1`/`XEX%`/`XEX-` → `xbox360` magic-only.
  - STFS (`CON `, `LIVE`, `PIRS` at 0): **verify** free60 STFS offsets: content type u32 BE at 0x344, title id u32 BE at 0x360, display name UTF-16BE at 0x411 (0x80 bytes), title name at 0x1691, thumbnail size u32 BE at 0x1712, thumbnail PNG at 0x171A (≤ 16 KiB → `ArtLocator::Range { Png }`). Kind: `0x0000_7000` GOD Base, `0x000D_0000` XBLA Base, `0x000B_0000` Update, `0x0000_0002` DLC. Any offset not verified → that field dropped (`BLOCKED:` line).
  - Probes: `default.xbe` among the names → read it (≤ 64 KiB budget for header + cert) → xbox; `default.xex` → xbox360; GOD/XBLA: the folder name is 8 hex, a child with an 8-hex name exists, one `read_dir` of that child finds a 40-hex file whose first 4 bytes are `CON `/`LIVE`/`PIRS` → STFS header → xbox360 (`probe = "<ctype>/<hash>"`). 8-hex folders without the magic → no match.
  - ZAR: `zarchive` footer (Task 9) → names; a `default.xex` at the root → its first block decoded (≤ 1 MiB) → `xex2`; else magic-only.
  - DAT: `dat_by_name_prefix(system, prefix)` — an index of the first 16 characters of rom names for `xbox360` digital rows (`<titleid8><contenttype8>`), built lazily with the serial index; STFS items look up `TITLE+CTYPE`.

- [ ] **Step 0: Sync.**
- [ ] **Step 1: Failing tests.** Builders `xbe(tid, name)`, `xiso_at(partition, files: &[(&str, Vec<u8>)])` (SparseAt with the volume descriptor and a small tree; for `0x1830_0000` the SparseAt holds only the parts, so the fixture is a few KiB in memory), `xex2(tid)`, `stfs(magic, tid, ctype, thumb)`, `god_folder(dir, …)`. Unit tests: `xbe_serial(0x4541_000D) == "EA-013"`; XDVDFS at each of the four offsets; `xdvdfs_cycle_terminates` (an entry whose left points to itself); `xdvdfs_2049_entries_stop_at_cap`; XEX2 execution info; STFS fields and thumbnail locator; `eight_hex_cache_folder_is_not_god`. Folder integration: `Halo/default.xbe` → `xbox/Halo`; `58410889/000D0000/<40hex>` (LIVE) → one `Folder` item named `58410889` to `xbox360/`. Harness: `SYSTEMS` `("xbox", Family::Console, true)`, `("xbox360", Family::Console, true)`; `TARGETS` `("esde","xbox","xbox",None)`, `("batocera","xbox360","xbox360",None)`, `("onion","xbox","XBOX",Some(false))`; `CONSOLE_HEADERS` `iso` at XGD1 (serial `MS-004`), `xiso` at 0, `iso` with `default.xex` (`4D5307E6`), `xex`. `tests/dat.rs` `xbox360_name_prefix_lookup` (trimmed No-Intro digital fixture). Class snapshot: `xbe` weak, `xiso xex zar xbox360` Unique, `iso` Generic.
- [ ] **Step 2: Run** `cargo test -p coverly-core --lib consoles::xbox && cargo test -p coverly-core --features test-util --test organize_folder_games && cargo test -p coverly-core --test systems_batch2 --test dat --test systems_classes` and expect failures.
- [ ] **Step 3: Implement** as above.
- [ ] **Step 4: Run** tests, G-RUST, G-WEB.
- [ ] **Step 5: Commit and push:** `git add -A crates && git commit -m "feat(core): identify Xbox and Xbox 360 images, executables and packages" && git push && git status -sb`

---

## Task 13: Nintendo DSi and DSiWare inside `nds` (D6)

**Files:** `systems/systems.toml` (`nds`), `identify/sniff.rs` (unit code into the detail), `tests/systems_batch2.rs`, `tests/sniff.rs`, class snapshot.

**TOML change** (`nds`):

```toml
aliases = ["NDS", "nintendods", "dsi", "nintendodsi"]
extensions = ["nds", "dsi", "zip", "7z"]
libretro = "Nintendo - Nintendo DS"
libretro_also = ["Nintendo - Nintendo DSi"]
dats = ["metadat/no-intro/Nintendo - Nintendo DS.dat", "metadat/no-intro/Nintendo - Nintendo DSi.dat"]
```

  Check that `dat_sources(nds)` before this change returns exactly `metadat/no-intro/Nintendo - Nintendo DS.dat` (the derived path) so the first entry is unchanged; check that no profile folder table uses `dsi` for another system (`grep -n '"dsi"' crates/coverly-core/src/profiles/builtin.rs`).

- [ ] **Step 0: Sync.**
- [ ] **Step 1: Failing tests.** `tests/sniff.rs` `nds_unit_code_in_detail`: `fx::nds_with(|h| h[0x12] = 0x03)` → `nds` hit with detail ending ` (DSi)`; `0x02` → ` (DSi-enhanced)`; `0x00` → detail unchanged (compare with the current string); system always `nds`. Harness: `dsi_folder_resolves_to_nds` (`resolve_folder("dsi")` and `("nintendodsi")` → nds), `dsi_extension_is_nds`, `dsi_cover_folder_is_tried` (cover lookup with only the DSi listing having the name → candidate under `Nintendo - Nintendo DSi`). Class snapshot: `dsi` Unique to nds.
- [ ] **Step 2: Run** `cargo test -p coverly-core --test sniff --test systems_batch2 --test systems_classes` and expect failures.
- [ ] **Step 3: Implement** (append to the existing NDS detail string; no new hit).
- [ ] **Step 4: Run** tests, `cargo test -p coverly-core` (snapshots with an NDS detail may change only for DSi heads: none in today's fixtures), G-RUST.
- [ ] **Step 5: Commit and push:** `git add -A crates && git commit -m "feat(core): recognise DSi and DSiWare dumps as Nintendo DS" && git push && git status -sb`

---

## Task 14: Dedupe: title-id keys, kinds and versions; folder games reported only

**Files:** `crates/coverly-core/src/dedupe/keys.rs` (`ExactKey::Title`), `dedupe/units.rs` (folder entries are units), `dedupe/apply.rs` or the set-aside guard (never a folder), tests `tests/dedupe_keys.rs`, `tests/dedupe_units.rs`, `tests/dedupe_plan.rs`.

**Interfaces (produced):**

```rust
// dedupe/keys.rs, ExactKey (non_exhaustive already)
/// Same console title id, kind and version (systems batch 2 D11, plan
/// P-6). Not a byte proof: unconfirmed, report-only.
Title { title_id: String, kind: TitleKind, version: Option<String> },
// key_string: format!("l1:{system}:title:{title_id}:{kind:?}:{}", version.unwrap_or_default())
// is_confirmed: false for Title
```

- [ ] **Step 0: Sync.**
- [ ] **Step 1: Failing tests.**
  - `dedupe_keys.rs`: `title_key_is_unconfirmed`; `base_and_update_never_share_a_key`; `same_base_same_version_share_a_key`.
  - `dedupe_plan.rs` over a library `ROMS/switch/A [0100000000010000][v0].nsp`, `ROMS/switch/A copy [0100000000010000][v0].nsp`, `ROMS/switch/A [0100000000010800][v65536].nsp`, `ROMS/switch/A [0100000000010000][v131072].nsp`: the two `v0` bases are a reported group (no set-aside action even with set-aside enabled); the update is in no group; the `v131072` base is reported with the `v0` ones under level 2 with the higher version as suggested keeper (`VersionKey` gains the console version as its first ordering key when both sides have one).
  - `dedupe_units.rs` `folder_games_are_units_and_never_set_aside`: two `ROMS/ps3/Game.ps3/` and `ROMS/ps3/Game (2).ps3/` with the same `TITLE_ID` → one reported group, and the set-aside planner refuses folder units (explicit check, not by accident of file-only code).
  - Key material and emulator data never appear as units (scan already skips them: assert).
- [ ] **Step 2: Run** `cargo test -p coverly-core --test dedupe_keys --test dedupe_plan --test dedupe_units` and expect failures.
- [ ] **Step 3: Implement.** The title key is computed from the scan entry's console id: dedupe reads it through the same budgeted reader (`consoles::identify` for files, `identify_dir` for folder entries) inside the existing per-unit read budget; the file name token (`title_id_from_name`) is enough for Switch. Units whose system is not one of the nine keep their current keys.
- [ ] **Step 4: Run** tests, existing dedupe snapshots (no change expected for other systems), G-RUST.
- [ ] **Step 5: Commit and push:** `git add -A crates && git commit -m "feat(core): report console duplicates by title id, kind and version" && git push && git status -sb`

---

## Task 15: Embedded covers (`sources/embedded.rs`) and settings version 3 (Rust and TS)

**Files:**
- Create: `crates/coverly-core/src/sources/embedded.rs`
- Modify: `crates/coverly-core/src/sources/mod.rs` (`ArtQuery.rom_path`), `src/fill/mod.rs` (pass `rom_path`; build the source), `src/settings/model.rs` (`KNOWN_SOURCES`, `available_sources_with`, `SETTINGS_VERSION` 3), `src/settings/load.rs` (migration), `Cargo.toml` (`image` + `"tga"`), `THIRD-PARTY-NOTICES.md` only if the `image` feature line is listed there
- Modify (TS, same commit): `src/ipc/types.ts` (`SourceId` += `'embedded'`), `src/lib/settingsRules.ts` (`KNOWN_SOURCES`, `AVAILABLE_SOURCES`, normalise migration), `src/ipc/mock/mockSettings.ts`
- Test: `tests/fill_sources.rs`, `tests/settings_file.rs`, `src/lib/settingsRules.test.ts`

**Interfaces (produced):**

```rust
// sources/mod.rs, ArtQuery
/// The ROM (file or game folder) on disk; `None` when unknown. Only the
/// embedded source reads it, through the budgeted console readers.
pub rom_path: Option<&'a std::path::Path>,

// sources/embedded.rs
pub const ID: &str = "embedded";
pub const MAX_ART_BYTES: u64 = 2 << 20;
pub struct EmbeddedSource;
impl ArtSource for EmbeddedSource { /* Cover only; one candidate per locator; fetch re-runs the reader */ }

// settings/model.rs
pub const SETTINGS_VERSION: u32 = 3;
pub const KNOWN_SOURCES: [&str; 4] = ["libretro", "steamgriddb", "screenscraper", "embedded"];
```

- [ ] **Step 0: Sync.**
- [ ] **Step 1: Failing tests.**
  - `tests/fill_sources.rs`: `embedded_png_from_vpk_entry`, `embedded_tga_from_wiiu_folder`, `embedded_smdh_icon_becomes_png` (decoded through `ctr::smdh_icon_png`), `embedded_answers_cover_only` (`Media::Screenshot` → no candidate), `embedded_png_over_limits_has_no_candidate` (a 2048×2048 PNG header → `image::Limits` refuse), `embedded_is_last` (with a mock libretro hit, the libretro candidate ranks first), `embedded_output_is_reencoded` (the written card is the profile's PNG, not the raw bytes: compare the bytes differ and decode to the profile size).
  - `tests/settings_file.rs`: `v2_file_gains_embedded_enabled_at_the_end`; `v3_file_keeps_embedded_off`; `available_sources_with_false_contains_embedded`; an unknown source id in a v3 file is dropped (existing lenient load).
  - `src/lib/settingsRules.test.ts`: normalise of a v2 settings object appends `{ id: 'embedded', enabled: true }` last; a v3 object with it disabled keeps it.
- [ ] **Step 2: Run** `cargo test -p coverly-core --test fill_sources --test settings_file && pnpm vitest run src/lib/settingsRules.test.ts` and expect failures.
- [ ] **Step 3: Implement.**
  - `image` workspace features `["png", "jpeg", "webp", "gif", "bmp", "tga"]`.
  - `EmbeddedSource::candidates(q)`: `q.media != Cover` or `q.rom_path` is `None` → empty. Otherwise re-run the budgeted reader (file: `open_rom` + `FileAt` + `consoles::identify`; folder: `identify_dir`; VPK: the package path) and turn each `ArtLocator` into a `Candidate { source: "embedded", kind: MatchKind::Canonical, url: <encoded locator> … }` (look at `Candidate`'s fields and use the field that holds a fetch key). `fetch`: re-run the reader, read ≤ `MAX_ART_BYTES` at the locator (folder files through `symlink_metadata` + `open_rom`, refusing symlinks; zip entries through the zip reader with the existing decode limits; SMDH → `smdh_icon_png` → PNG bytes), decode with `image::ImageReader` and `Limits { max_image_width: Some(1024), max_image_height: Some(1024), max_alloc: Some(16 << 20) }`, return the encoded PNG of the decoded image (the existing pipeline re-encodes again to the profile format). Never write raw bytes.
  - Fill: build `EmbeddedSource` whenever `embedded` is enabled in the source order (no key, no network, every build); pass `rom_path: Some(&entry.path)`.
  - Settings: `KNOWN_SOURCES` += `embedded`; `available_sources_with(_)` always includes it; in `load.rs`, after parsing, `if file_version <= 2 { order.push(SourceEntry { id: "embedded".into(), enabled: true }) }` before `normalise` (so `normalise` does not append it disabled); version written as 3. Same in `settingsRules.ts` and `mockSettings.ts`.
- [ ] **Step 4: Run** tests, G-RUST, G-WEB.
- [ ] **Step 5: Commit and push:** `git add -A Cargo.toml Cargo.lock crates src THIRD-PARTY-NOTICES.md && git commit -m "feat(core): use plaintext icons inside games as a last-resort cover source" && git push && git status -sb`

---

## Task 16: Batch plan snapshots over the modern pile, and existing snapshot review

**Files:** `crates/coverly-core/tests/organize_plan.rs` (`modern_pile`, `systems_batch_2_pile_plans`), new snapshots `tests/snapshots/organize_plan__systems_batch_2__{esde,batocera,muos}.snap`.

- [ ] **Step 0: Sync.**
- [ ] **Step 1: Write the pile** (spec §9.3, synthetic bytes from `consoles::testing` builders written to disk with `put`, small sizes; the folder games' declared sizes come from their real small files — the UI's `23,41 GB` is the mock's, not this test's):

```rust
/// The modern pile of systems batch 2 (spec §9.3): synthetic heads, real
/// names, every mtime pinned in the past.
fn modern_pile(dir: &Path) {
    use coverly_core::consoles::testing as ct;
    put(&dir.join("Metroid Prime (USA).rvz"), &ct::wia_bytes(ct::gc_header("GM8E01"), true, 1));
    put(&dir.join("Resident Evil 4 (USA) (Disc 1).rvz"), &ct::wia_bytes(ct::gc_header_disc("G4BE08", 0), true, 1));
    put(&dir.join("Resident Evil 4 (USA) (Disc 2).rvz"), &ct::wia_bytes(ct::gc_header_disc("G4BE08", 1), true, 1));
    put(&dir.join("Super Mario Galaxy (Europe).wbfs"), &ct::wbfs_bytes(ct::wii_header("RMGP01")));
    put(&dir.join("Super Mario Galaxy (Europe).wbf1"), &[0u8; 1024]);
    put(&dir.join("Excitebike - World Rally (Europe) (WiiWare).wad"), &ct::wii_wad_bytes(0x0001_0001_5741_4150));
    ct::wiiu_folder(&dir.join("Super Mario 3D World [0005000010145D00]"));
    put(&dir.join("Mario Kart 7 (Europe).3ds"), &ct::ncsd_bytes("CTR-P-AMKP", 0x0004_0000_0003_0800, false));
    put(&dir.join("Pokemon Y (Europe).cia"), &ct::cia_bytes(0x0004_0000_0005_5E00, true, false));
    put(&dir.join("model.3ds"), &[0x4D, 0x4D, 0x10, 0, 0, 0, 2, 0]);
    put(&dir.join("Super Mario Odyssey [0100000000010000][v0].nsp"), &ct::nsp_bytes(&[("01000000000100000000000000000000.tik", &[0; 64])]));
    put(&dir.join("Super Mario Odyssey [0100000000010800][v393216].nsp"), &ct::nsp_bytes(&[]));
    put(&dir.join("Zelda BotW.xci"), &ct::xci_bytes());
    ct::vita_app(&dir.join("PCSE00844"), "PCSE00844");
    put(&dir.join("Persona 4 Golden.vpk"), &ct::vpk_bytes("PCSB00245"));
    ct::ps3_jb(&dir.join("Demon's Souls (BLUS-30443)"), "BLUS30443", "DG");
    put(&dir.join("NPUB30024.pkg"), &ct::pkg_bytes(1, "UP0001-NPUB30024_00-0000000000000000"));
    put(&dir.join("Halo (USA).xiso"), &ct::xiso_bytes_at0(ct::xbe_bytes(0x4D53_0004, "Halo")));
    put(&dir.join("Halo 3 (USA).xex"), &ct::xex2_bytes(0x4D53_07E6));
    ct::god_folder(&dir.join("58410889"), 0x5841_0889, 0x000D_0000);
    for k in ["prod.keys", "title.keys", "NPUB30024.rap"] { put(&dir.join(k), &[9u8; 128]); }
    put(&dir.join("ux0").join("app").join("PCSB00245").join("eboot.bin"), &[1u8; 64]);
    put(&dir.join("Foo.pkg"), b"xar!\0\x1c\0\x01");
    for path in files_below(dir) { pin_mtime(&path, 1); }
}
```

  (The Redump XGD1 offset is covered by the `SparseAt` unit tests of Task 12; on disk the pile uses an XISO at offset 0 to stay small.) Add the `*_bytes` helpers to `consoles/testing.rs` as `SparseAt` → `Vec<u8>` flattening (only for heads under a few KiB).
- [ ] **Step 2: Write the test** `systems_batch_2_pile_plans` like `systems_batch_1_pile_plans` for `esde`, `batocera`, `muos`, plus asserts that do not depend on the snapshot: key files are `KeyMaterial`; `ux0` is `EmulatorData`; `Demon's Souls` is one `Folder` item with `.ps3` on esde/batocera; the update follows its base; `model.3ds` and `Foo.pkg` are `NotARom`.
- [ ] **Step 3: Run** `INSTA_UPDATE=always cargo test -p coverly-core --test organize_plan systems_batch_2`, read the three snapshots line by line against spec §9.3 and §6 (folder per layout, notes, reasons), then `cargo insta pending-snapshots` (nothing pending).
- [ ] **Step 4: Review existing snapshots.** `git diff --stat crates/coverly-core/tests/snapshots` must list only the new files and any snapshot already explained in an earlier task's commit; re-run `cargo test -p coverly-core` clean. G-RUST.
- [ ] **Step 5: Commit and push:** `git add -A crates && git commit -m "test(core): snapshot the systems batch 2 pile under three layouts" && git push && git status -sb`

---

## Task 17: Icons: silhouettes, audit, regenerate, notices

**Files:** `scripts/icons/systemIconsLib.mjs` (`SYSTEM_ICON_SOURCES` rows), `src/components/system/systemFallbacks.ts` (`FALLBACK_ABBR`, `WORDMARK_IDS`), generated `src/components/system/systemIcons.generated.ts` and `src/assets/system-icons/*`, `THIRD-PARTY-NOTICES.md`.

- [ ] **Step 0: Sync** with `origin/main` (other workflows touch `src/components/*`): `git fetch origin && git merge origin/main`, resolve, gates, commit the merge, push.
- [ ] **Step 1: Failing test.** `src/components/system/systemIcons.test.ts` (or the existing icon test): `gamecube`, `wii`, `wiiu`, `3ds`, `switch`, `psvita`, `ps3`, `xbox`, `xbox360` each have either a generated silhouette or a `FALLBACK_ABBR` entry (≤ 4 characters) and, if a wordmark, an entry in `WORDMARK_IDS`. Run `pnpm vitest run src/components/system` → fails.
- [ ] **Step 2: Add the sources** (files from spec §9.2, all in `tests/fixtures/research/retroarch-assets-monochrome.txt`: check each with `grep -F "<name>.svg"` first):

```js
  // Systems batch 2 (spec §9.2). Audit each on the contact sheet: a
  // wordmark goes to WORDMARK_IDS and keeps the Slot mount.
  gamecube: { file: 'Nintendo - GameCube.svg', depicts: 'pad', audit: '' },
  wii: { file: 'Nintendo - Wii.svg', depicts: 'pad', audit: '' },
  wiiu: { file: 'Nintendo - Wii U.svg', depicts: 'pad', audit: '' },
  '3ds': { file: 'Nintendo - Nintendo 3DS.svg', depicts: 'handheld', audit: '' },
  switch: { file: 'Nintendo - Switch.svg', depicts: 'handheld', audit: '' },
  psvita: { file: 'Sony - PlayStation Vita.svg', depicts: 'handheld', audit: '' },
  ps3: { file: 'Sony - PlayStation 3.svg', depicts: 'pad', audit: '' },
  xbox: { file: 'Microsoft - Xbox.svg', depicts: 'pad', audit: '' },
  xbox360: { file: 'Microsoft - Xbox 360.svg', depicts: 'pad', audit: '' },
```

  Use the `depicts` values the script accepts (`grep -n "depicts" scripts/icons/systemIconsLib.mjs | head`). Run `pnpm system-icons`, open the contact sheet it writes, and fill each `audit` with one line describing what the drawing shows (as batch 1 did); a logo or wordmark, or a drawing outside the 1:2–2.5:1 ratio, moves to `WORDMARK_IDS`. `FALLBACK_ABBR`: `gamecube: 'GC'`, `wii: 'WII'`, `wiiu: 'WIIU'`, `'3ds': '3DS'`, `switch: 'NSW'`, `psvita: 'VITA'`, `ps3: 'PS3'`, `xbox: 'XBOX'`, `xbox360: 'X360'`.
- [ ] **Step 3: Regenerate** (`pnpm system-icons`), never hand-edit the output. Update `THIRD-PARTY-NOTICES.md` count and id list (licence paragraph unchanged).
- [ ] **Step 4: Run** `pnpm vitest run src/components/system`, G-WEB.
- [ ] **Step 5: Commit and push:** `git add -A scripts src THIRD-PARTY-NOTICES.md && git commit -m "build(icons): add silhouettes for the systems of batch 2" && git push && git status -sb`

---

## Task 18: UI: types, copy, folder rows with size, update order, sources row, mock modern pile

REQUIRES frontend-design:frontend-design (spec §9.1 is the design plan; critique with screenshots before committing).

**Files:** `src/ipc/types.ts`, `src/ipc/contract.sample.json`, `src/ipc/contract.test.ts`, `src/lib/organize.ts` (+ test), `src/components/organize/SystemGroups.tsx`, `ReviewBuckets.tsx`, `LeftInPlace.tsx` (+ tests), `src/components/settings/SourcesSection.tsx`, `locales/{es,en}/ui.json`, `src/ipc/mock/mockOrganize.ts` (+ test), `src/ipc/mock/fixture.ts`.

**Types (exact):**

```ts
export type SourceId = 'libretro' | 'steamgriddb' | 'screenscraper' | 'embedded';
export type OrganizeItemKind = 'file' | 'archive' | 'set' | 'folder';
// LeftReason union gains:
  | { kind: 'key-material' }
  | { kind: 'emulator-data'; folder: string }
// NotARom gains 'installer' | 'model3d'
// ItemNote union gains:
  | { kind: 'update'; of: string }
  | { kind: 'dlc'; of: string }
  | { kind: 'encrypted-content'; emulator: string }
  | { kind: 'install-first'; emulator: string }
  | { kind: 'folder-suffix'; name: string; layout: string }
// OrganizePlan gains `updates?: number`.
```

  If the Rust `ConsoleId` reaches the UI through `why` evidence only, no new type is needed; if Task 10 exposed `id_source`/`title_id` on the item (check `OrganizeItem` serialisation), mirror the field exactly.

**Copy** (spec §10, both locales, sorted into place; keys exactly): `organize.left.keyMaterial`, `organize.left.emulatorData`, `organize.left.installer`, `organize.left.model3d`, `organize.why.titleIdFile`, `organize.why.titleIdName`, `organize.why.titleIdTicket`, `organize.note.update`, `organize.note.dlc`, `organize.note.encryptedContent`, `organize.note.installFirst`, `organize.note.folderSuffix`, `organize.summary.updates`, `organize.summary.updatesOne`, `organize.item.folder`, `settings.sources.name.embedded`, `settings.sources.hint.embedded`. Check the existing key prefixes first (`grep -n '"organize.left\|"organize.note\|"organize.why' locales/es/ui.json | head`) and follow the file's real nesting (flat or nested objects) — the spec's dotted names map onto it.

- [ ] **Step 0: Sync** with `origin/main` (locales and components change in parallel): merge, resolve key by key, gates, commit the merge, push.
- [ ] **Step 1: Failing tests.**
  - `src/lib/organize.test.ts`: text for each new reason, note and why line (`titleIdFile` with `{file}` = `PARAM.SFO`; `titleIdTicket`; `titleIdName`), `{file}` only from the fixed set; `updates` summary picks `updatesOne` when 1.
  - `SystemGroups.test.tsx`: a `folder` item renders the folder glyph with `aria-hidden="true"`, a visually hidden «Carpeta de juego» before the name, and the size `23,41 GB` (es) / `23.41 GB` (en) in a mono element via `formatDecimalSize`; an update item renders right after its base; the summary line counts updates.
  - `LeftInPlace.test.tsx`: `key-material` and `emulator-data` rows render their copy with `prod.keys` / `ux0` in mono.
  - Sources section test (where `SourcesSection` is tested, e.g. `Sections.test.tsx`): the `embedded` row shows «Imagen incluida en el juego» and its hint, has no key field, and is reorderable (the existing `ReorderAnnouncer` announces its move).
  - `contract.test.ts`: the sample has one item of each new member (`'folder'`, `'embedded'`, `installer`, `model3d`, every new note and reason) and round-trips.
  - `mockOrganize.test.ts`: `?mockPick=modern` → `MOCK_MODERN`; the modern plan has the nine groups, `Demon's Souls (BLUS-30443)` is one `folder` item with size `23_410_000_000` and a `folder-suffix` note on `esde`, the Switch update follows its base with `update`, three `key-material` rows, one `emulator-data` row, `model.3ds` `not-a-rom` `model3d`; switching to `batocera` gives `switch/` with no `no-folder-on-device` note; `muos` gives the Wii `no-folder-on-device` note.
  - i18n parity and the Spanish copy check run in `pnpm check-all`.
- [ ] **Step 2: Run** `pnpm vitest run src/lib/organize.test.ts src/components/organize src/components/settings src/ipc` and expect failures.
- [ ] **Step 3: Implement.**
  - Types, sample and copy as above.
  - `SystemGroups.tsx` (and wherever items render in `ReviewBuckets.tsx`/`LeftInPlace.tsx`): for `item.kind === 'folder'`, the lucide `Folder` glyph already imported (`aria-hidden`), `<span className="sr-only">{t('organize.item.folder')}</span>` before the name, `<span className="font-mono">{formatDecimalSize(item.size, locale)}</span>` after it (check `formatDecimalSize`'s signature in `src/lib/format.ts`); a truncated tree (`size` flagged by the plan) shows «más de» per the existing pattern if one exists, otherwise just the size. Ordering: within a system group, stable sort so an item with an `update`/`dlc` note follows the item whose name matches `note.of`. Notes render with the component and tokens of the `no-folder-on-device` note. Codes (`title_id`, file names in reasons) in mono through the existing renderer for numbers and paths.
  - `SourcesSection.tsx`: the `embedded` row (name + hint), no key field.
  - Mock: `MOCK_MODERN = '/Users/demo/Modern'`, `readMockPick` maps `modern`; `ORGANIZE_SYSTEMS` gains the nine systems (names as `systems.toml`, folders as the first alias); `ABSENT_ON` for the new systems per §6 (`gamecube: ['anbernic-stock','garlicos','onion','muos','minui']`, …, `3ds` absent on all but esde/batocera/muos); the modern pile of spec §9.3 with synthetic sizes (folder games: `23_410_000_000` for Demon's Souls, `1_700_000_000` for the Wii U folder); `fixture.ts` shelves gain `GAMECUBE`, `PS3`, `SWITCH` with a few games each. Update the `readMockPick` doc comment.
- [ ] **Step 4: Design critique.** Start `pnpm dev --port 1466 --strictPort` in the background, open `http://localhost:1466/?mockPick=modern` (Chrome DevTools MCP, `isolatedContext: "systems-batch-2"`), screenshot the plan in light and dark and at the narrowest supported width, compare against spec §9.1 (no pills, no lock icon, notes as plain sentences, mono only for codes and sizes) and fix what reads off. Stop the server: `lsof -ti tcp:1466 -sTCP:LISTEN | xargs -r kill`.
- [ ] **Step 5: Run** G-WEB, then G-RUST (the contract sample is shared with Rust tests if `contract.sample.json` is read there: check `grep -rn contract.sample crates src-tauri`).
- [ ] **Step 6: Commit and push** (two commits if the diff is large: types+copy, then components+mock): `git add -A src locales && git commit -m "feat(ui): show game folders, console notes and the embedded cover source" && git push && git status -sb`

---

## Task 19: Verification, E2E, changelog and PR

**Files:** `CHANGELOG.md` (Unreleased), `docs/superpowers/specs/2026-10-26-systems-batch-2.md` (status line only: «Implemented on `feat/systems-batch-2`»), screenshots under the scratchpad (not committed).

- [ ] **Step 0: Sync** with `origin/main`; resolve conflicts by hand, run all gates, commit the merge, push.
- [ ] **Step 1: Changelog.** Under `## [Unreleased]`: **Added** — nine systems (GameCube, Wii and WiiWare, Wii U, 3DS, Switch, PS Vita, PS3, Xbox, Xbox 360), DSi/DSiWare in Nintendo DS, game folders moved as one item, embedded icons as a last-resort cover source. **Changed** — key and licence files are left untouched and never read; emulator data folders are left; macOS `.pkg` installers and 3D Studio `.3ds` files are reported as not ROMs; settings move to version 3 (the new source is enabled once for existing users). English, plain words.
- [ ] **Step 2: Full gates** from a clean tree; paste the last line of each into the PR evidence:

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-systems-2 && pnpm check-all && pnpm build && cargo fmt --all --check && cargo clippy --workspace --all-targets --all-features -- -D warnings && scripts/dev/clippy-windows.sh && cargo test --workspace --no-fail-fast
```

- [ ] **Step 3: Real-data check, read-only.**
  - `cargo run -q -p coverly-cli -- systems list | wc -l` (or the CLI's equivalent per `cargo run -q -p coverly-cli -- --help`) shows 127 systems.
  - If `~/Documents/ROMS` or `/Volumes/TinyExt/*` hold any Tanda 2 file, list them with `find <root> -maxdepth 3 \( -iname '*.rvz' -o -iname '*.wbfs' -o -iname '*.nsp' -o -iname '*.xci' -o -iname '*.3ds' -o -iname '*.cia' -o -iname '*.vpk' -o -iname '*.pkg' -o -iname 'PARAM.SFO' -o -iname 'default.xbe' \) | head -50` (read-only), then run a **plan-only** `coverly organize` on that root and record the counts per system and any unexpected row. Never apply.
  - For any «verify» rule still open, copy one real head under `target/systems-sample/` (`head -c 1048576 "<file>" > target/systems-sample/<name>`), run the reader test against it locally (not committed), record the result, `find target/systems-sample -name '._*' | wc -l` prints `0`, then `rm -rf target/systems-sample`.
- [ ] **Step 4: Browser E2E** (spec §14.3). Start `pnpm dev --port 1466 --strictPort` in the background. Chrome DevTools MCP `new_page` `http://localhost:1466/?mockPick=modern` with `isolatedContext: "systems-batch-2"`; fall back to Playwright MCP, then `playwright-core` + Chrome for Testing in `/private/tmp/claude-501/-Users-miguelangelesparzacalero-Documents-GitHub/35b7c020-f2ae-412f-b757-99c24a9a498b/scratchpad/systems-batch-2`.
  1. Organise offer → plan: the nine groups with their silhouettes (or mounts); the RE4 disc set as one item; `Demon's Souls (BLUS-30443)` one folder row with `23,41 GB` and «Se renombra a Demon's Souls (BLUS-30443).ps3 para que ES-DE lo muestre».
  2. Switch group: the base NSP with «Title ID 0100000000010000 en el nombre del ticket», its update right under it with «Actualización de Super Mario Odyssey»; the summary says «Incluye 1 actualización o contenido descargable».
  3. Left in place: `prod.keys`, `title.keys`, `NPUB30024.rap` with «Archivo de claves o licencia: Coverly no lo lee ni lo mueve»; `ux0` with the emulator-data reason; `model.3ds` «No es una ROM (modelo 3D)».
  4. Profile Batocera: Switch to `switch/` with no «no tiene carpeta» note; profile muOS: Wii shows «muOS no tiene carpeta para Nintendo Wii; se crea wii.» (check the real string of `organize.noFolderOnDevice`).
  5. Keyboard only: Tab through the review rows; `take_snapshot` shows each note as text, the hidden «Carpeta de juego» label and the size «23,41 GB» in the accessibility tree.
  6. Settings → Fuentes: the «Imagen incluida en el juego» row, reorder it with the keyboard and hear the announcer text in the snapshot.
  7. Library shelves with `GAMECUBE`, `PS3`, `SWITCH`: light and dark (`data-theme`), `prefers-reduced-motion: reduce` emulated, 1280 px and the narrowest supported width.
  - `list_console_messages`: no errors. Screenshots to `/private/tmp/claude-501/-Users-miguelangelesparzacalero-Documents-GitHub/35b7c020-f2ae-412f-b757-99c24a9a498b/scratchpad/systems-batch-2/e2e/`. Stop the server: `lsof -ti tcp:1466 -sTCP:LISTEN | xargs -r kill`.
- [ ] **Step 5: Commit and push:** `git add CHANGELOG.md docs/superpowers/specs/2026-10-26-systems-batch-2.md && git commit -m "docs: changelog for systems batch 2" && git push && git status -sb`
- [ ] **Step 6: Open the PR** in English, no Claude/AI mention, no generated-by line:

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-systems-2 && gh pr create --base main --head feat/systems-batch-2 \
  --title "feat: add GameCube, Wii, Wii U, 3DS, Switch, Vita, PS3, Xbox and Xbox 360 (systems batch 2)" \
  --body-file /private/tmp/claude-501/-Users-miguelangelesparzacalero-Documents-GitHub/35b7c020-f2ae-412f-b757-99c24a9a498b/scratchpad/systems-batch-2/pr-body.md
```

  The body (scratchpad, not the repo) has: Summary by system; what each reader reads in clear (spec §8.3 Switch table verbatim); the D4 key rule and how it is tested; the `nod`/`nx-archive`/`xex` evaluation (spec D2, §16: `nod` 1.4.4 is `MIT OR Apache-2.0`, compatible with AGPL-3.0, rejected because it embeds the Wii common keys and decrypts, and vendors C compression libraries; no new dependency); behaviour changes (§12); folder sources (§16); per-frontend extension mismatches (§6 notes: ES-DE `n3ds` lacks `.cia`, `switch` lacks `.nsz/.xcz`, `psvita` shows only `.psvita`, …); **Blocked / needs the owner**: Switch title-id catalogue (D10), GameTDB licence, Batocera vs Knulli `switch` (D19), launcher files (D14), ES-DE media folder for `gc`/`n3ds`, extracting archived big games and big folder copies (confirmation above a size), encrypted-copy note wording, setting folder duplicates aside (D28) and title-id duplicates report-only (P-6), zipped folder games, 3DS CDN / Wii NUS folders, ES-DE/Batocera media name for `.ps3` dirs (Task 6 result), and every `BLOCKED:` line (`git log origin/main..HEAD --format=%b | grep BLOCKED`); gates evidence (Step 2); E2E evidence with screenshots (uploaded as PR comment images, or described when upload is not possible). Post the local gate evidence as a PR comment too (GitHub Actions is blocked by billing; gates were run locally).
- [ ] **Step 7: End state:** `git status -sb` shows `## feat/systems-batch-2...origin/feat/systems-batch-2` with nothing ahead, and `gh pr view --json url,state` prints the PR.

---

## Spec coverage check

| Spec | Task |
|---|---|
| D1 scope, D3 ids | 7–13 |
| D2 no new crate, `nod` evaluation | 1 (module), 19 (PR) |
| D4 keys | 1 (`is_key_material`), 3 (walk, open hook), 10 (NSP ranges), 11 (`.rap`, `work.bin`), 14 (dedupe) |
| D5 readers in `consoles/` | 1, 7–12 (P-1: one sniff integration point) |
| D6 DSi | 13 |
| D7 shared/weak, `NotARom` | 3, 7, 8, 10, 11, 12; class snapshot each |
| D8 aliases (scoped `switch`, `vita`) | 10, 11 |
| D9 `libretro_also` | 2, 8, 11, 13 |
| D10 Switch title id | 10 |
| D11 kinds, updates, DLC | 1 (`kind_*`), 10 (notes, order, summary), 14 (dedupe, P-6) |
| D12 folder probes | 4 (machinery, P-7), 9, 10, 11, 12 |
| D13 `.ps3` suffix | 4 (plumbing), 11 (values) |
| D14 launcher text files | 11, 12 |
| D15 `multi_file` | 7, 11, 12 |
| D16 VPK package | 11 |
| D17 embedded covers, settings v3 | 15 |
| D18/D19 libretro and Switch folders | 2, 10 |
| D20 families, D21 icons | 7–12, 17 |
| D22 serial forms | 1, 7–12 |
| D23 facts v7 | 3 |
| D24 budgets | 1, every reader task |
| D25 directory moves | 5 |
| D26 library folder games | 6 |
| D27 folder gates | 4 |
| D28 folder duplicates reported | 14 |
| §6 folders | 7–12 rows; harness `TARGETS` |
| §9 UX, §10 copy, §9.3 mock | 16 (core pile), 18 (UI) |
| §12 edge cases | 3–16 |
| §13 blocked list | 19 (PR) |
| §14 tests | every task; 16 (snapshots); 19 (E2E) |
