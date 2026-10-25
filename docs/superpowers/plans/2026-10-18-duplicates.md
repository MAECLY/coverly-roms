# Duplicates Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. UI tasks (Group D and Group F) also REQUIRE frontend-design:frontend-design: follow the design brief in this plan ("UI design brief") and critique each screen with screenshots before committing.

**Goal:** Coverly finds the same game stored more than once inside one system, at three levels: (1) exact copy, whatever the name, container, copier header or compression; (2) same game, other version, from the catalogue (revision, region, beta), opt-in and report-only by default; (3) same disc game, other format, from the disc serial plus a volume fingerprint (cue/bin vs chd vs pbp). It picks one copy to keep per group with ordered, configurable rules and gives a one-line reason. It then sets the other copies aside in a quarantine folder outside `ROMS/`, together with their saves and art. The move is journaled and undoable, and nothing is ever deleted. Sending to the OS Trash is opt-in. All of this is a step of Organise, a library action (`DuplicatesScreen`) and `coverly dedupe` (dry run unless `--apply`).

**Architecture:** A new read-only planner in `coverly-core` (`dedupe/`): `units` (what is compared, exclusions, sets), `keys` (level 1 content keys with size bucketing, level 2 `title_key`, level 3 serial + volume fingerprint), `group` (connected components per system, weakest-link relation, report-only reasons), `saves`, `keeper` (ordered rules → `KeepReason`), `companions`, `plan` (`plan_dedupe`), and later `apply` (journaled moves to the quarantine or the Trash, revalidation, pause, resume). Archive listings and disc identity come through two narrow traits that dedupe owns (`EntryListing`, `DiscProbe`). Phase A builds and tests them with fakes. Phase B implements them on top of `feat/organize-disc-archives`. Supporting changes: `identify/names.rs::parse_tags`, the DAT parser keeps `serial`/`region`/`cloneof`/`isbios`, cache schema v3 (`content_keys`, `dedupe_choices`, DAT columns), `HashOutcome` v2, `settings.dedupe`, journal `JobKind::Dedupe` + `Op::Trashed`. The CLI adds `coverly dedupe`. `src-tauri` adds a `dedupe` module (plan store, progress events, apply behind the job guard, undo through `organize_undo`). The React app adds `DuplicatesScreen`, `components/dedupe/*`, `dedupeStore`, a Toolbar entry, a section in the Organise proposal, a Jobs kind and a settings block.

**Tech Stack:** Rust 1.90 (1.93 once `feat/organize-disc-archives` raises the MSRV), edition 2021, `rusqlite`, `zip`, `crc32fast`, `sha1`, `flate2` (already deps). There is one new dependency, `trash` 5.x, in Phase B only and behind a trait (deviation DD-4). Tauri 2.12, React 19 + TypeScript 6 strict + Tailwind 3 + zustand 5, `src/components/ui` primitives (Radix-based), `lucide-react` 1.59. Tests: `cargo test`, `insta` JSON snapshots, `assert_cmd`, Vitest + Testing Library. E2E: Chrome DevTools MCP on `pnpm dev` (mock backend).

**Spec:** [`docs/superpowers/specs/2026-10-18-duplicates-design.md`](../specs/2026-10-18-duplicates-design.md) (all sections). Its §17 milestones D1–D11 map to the task groups below. It builds on [`2026-10-17-organize-loose-roms-design.md`](../specs/2026-10-17-organize-loose-roms-design.md) and the archive spec on branch `feat/organize-disc-archives` (`docs/superpowers/specs/2026-10-18-organize-disc-archives.md`). Brand: [`docs/08-brand/brand.md`](../../08-brand/brand.md). Tracker: `docs/TRACKER.md`.

## Global Constraints

- Work only in the worktree `/Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-dedupe`, branch `feat/organize-dedupe`. Never switch branches, never push, never touch `main`. Every shell command starts with `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-dedupe &&`.
- **Phase gate.** Tasks marked **BLOCKED** start only after `feat/organize-disc-archives` is merged into `main` and this branch is rebased on it (`git rebase main`, then all gates green). Tasks marked **light** may start now but will need a textual merge on rebase. Tasks marked **none** never touch a file the archive branch changes.
- TDD: write the failing test, run it and watch it fail, implement, watch it pass.
- After **every** task, run all gates and fix anything red before committing: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `pnpm lint`, `pnpm typecheck`, `pnpm format:check`, `pnpm test`, `pnpm build`. Rust-only tasks may skip the `pnpm` gates when no file under `src/`, `locales/` or `src-tauri/` changed. UI tasks run everything.
- Commits: conventional (`feat(core): …`, `test(core): …`, `feat(ui): …`, `docs: …`), granular, English. **Never** add `Co-Authored-By`, "Generated with", or any mention of Claude/AI. Do not push.
- No crate that is not already in `Cargo.lock`, except `trash` (DD-4). `cargo build --offline` must keep working after the one `cargo fetch` of Task 22.
- The core never depends on Tauri and holds no UI strings. Reasons are enums plus data, and the UI and the CLI render them.
- **Real data is read-only.** Never write to `/Users/miguelangelesparzacalero/Documents/ROMS`, `/Users/miguelangelesparzacalero/Documents/ROMs-Downloaded` or `/Volumes/*`, and never create `._*` files there. Write tests use `tempfile` dirs. Manual apply runs use a copy under `target/dedupe-sample/` made with `cp -X`.
- Fixtures are synthetic and tiny, with no copyrighted ROM bytes. The real names from spec §15 may appear as file names in tests, over synthetic bytes.
- **Names never group.** No key reads `RomEntry.stem`, a zip name or an inner entry name. A name may only split or downgrade a group (arcade set names, modification tags) and break the final tie. Every task that adds a key adds a "names never group" test.
- Every UI string goes through i18n in `locales/en/ui.json` **and** `locales/es/ui.json` (same keys; `src/i18n/parity.test.ts`). Spanish uses **apartar**, never eliminar or borrar. No exclamation marks, exact counts, numbers in mono.
- File names are hostile: React text only, CLI output through `table::terminal_safe`, JSON exports as data.
- IPC: JS argument names camelCase, payload fields snake_case, errors `{ code, detail? }`, codes only added. The webview never sends a plan back, only `plan_id` + decisions (keeper picks, skipped groups).

## Review Focus

1. **Two different games grouped.** Same name in two systems (`SFC/Tetris 2.zip` vs `GB/Tetris 2 (1).zip`), identical arcade zips under two valid set names (`ffight.zip`/`ffightu.zip`), soft-patched vs clean, PS BIOS twins, discs 1 and 2 of one game (`Driver 2.img` SLUS-01161 vs `Driver 2.bin` SLUS-01318), a translation that shares a serial, NES headers with different mappers. Expected: no group, or a report-only group. Tests: Task 8 (`bios_twins_are_never_units`), Task 9 (`names_never_group`, `two_valid_arcade_set_names_are_not_duplicates`, `soft_patch_splits_identical_roms`, `nes_headers_that_differ_are_report_only`), Task 10 (`different_serials_never_group`, `same_serial_other_build_is_report_only`, `modification_tag_vetoes_level_three`), Task 11 (`no_group_spans_two_systems`).
2. **The copy you play loses its save.** Exact copies with different saves, a remembered pick vs a new save, shared-stem saves, per-core save folders, a blank PS memory card, saves off the card. Tests: Task 12 (`blank_ps_card_is_no_save`, `different_real_saves_make_a_conflict`), Task 13 (`remembered_pick_drops_when_the_other_copy_gains_a_save`), Task 14 (`shared_stem_save_stays_with_the_keeper`).
3. **Something gets lost on apply.** Keeper changed after the plan, collision in the quarantine, cross-volume copy corrupt, FAT32 over 4 GiB, a set moved by halves, undo after a later job, trashed files. Tests: Task 23 (`keeper_changed_skips_the_group`, `set_moves_all_or_nothing`, `fat32_destination_skips_before_any_copy`), Task 24 (`undo_restores_rom_companions_art_and_gamelist`, `undo_lists_trashed_files_for_manual_restore`).
4. **Scale.** 50 000 files, power-of-two cartridge sizes, a warm run with no reads, a pathological 3 000-member group. Tests: Task 15 (`warm_run_hashes_nothing`, `ten_thousand_files_hash_only_colliders` ignored perf gate, `huge_group_is_collapsed_and_report_only`), Task 20 (`system_section_windows_over_200_groups`).
5. **UI traps.** A tray radio mistaken for "remove this", counts that grow silently, "free space" that is not freed, a confirm missing on an unusual plan. Tests: Task 19 (`tray_radio_has_a_visible_keep_label`, `changing_keeper_announces_and_offers_undo`), Task 20 (`option_change_shows_the_delta`), Task 30 (`bulk_confirm_above_500_files`, `same_volume_done_state_says_the_card_is_as_full`).

## Deviations from the spec (decided in this plan)

- **DD-1: the archive contract is narrower than spec §12.4.** The archive branch exposes `organize::archive::ArchiveSource { list() -> Listing; stream(order) }` and puts disc evidence into sniff `detail` strings. It has no `ArchiveReader`, no `DiscId` and no `read_disc_id`. Dedupe therefore owns two narrow traits in `dedupe/source.rs`: `EntryListing` (entry sizes + CRCs + bounded open) and `DiscProbe` (`DiscId { serials, disc_no, volume, source }`). Phase A implements `EntryListing` for zip with the existing `hash_rom` / `organize::inspect` code and tests `DiscProbe` with a fake. Phase B adds `ArchiveEntryListing` over `ArchiveSource` and `IsoDiscProbe` (Task 25), which reuses the archive branch's PVD/`SYSTEM.CNF` sniff helpers and adds the boot-file SHA1, PVD timestamps, `IP.BIN` disc number and PBP `PSISO` reading.
- **DD-2: CHD.** Level 1 uses the CHD v3/v4/v5 header `sha1`/`rawsha1`, a header-only read with no decompression. Level 3 for CHD needs hunk decompression, which has no crate in the lockfile, so it is **deferred**. A CHD only takes part in level 3 if a later reader provides a `DiscId`. The summary says "Otros formatos: {n} CHD sin leer por dentro". This matches spec §15.3: 24 CHDs gave no serial.
- **DD-3: PBP serial.** `IsoDiscProbe` reads the PBP `DATA.PSAR` `PSISO` index and inflates only the blocks that hold sector 16, the root directory and `SYSTEM.CNF` (`flate2`, a bounded 1 MiB output). It never reads the outer EBOOT `PARAM.SFO` for the fingerprint (spec §5.4).
- **DD-4 (superseded by archive plan AP-3, 2026-10-24): use `write::trash` from `feat/organize-disc-archives`, no `trash` crate.** The archive branch ships `crates/coverly-core/src/write/trash.rs` (`TrashBackend` with `trash(path) -> Option<location>`, `find(volume, name, fp)`, `available(path)`; `OsTrash` for macOS `NSFileManager trashItemAtURL:resultingItemURL:`, Windows Recycle Bin, Linux freedesktop; `FakeTrash` for tests), because organise's undo needs the exact Trash location, which the `trash` crate does not return on macOS. Phase B uses that module: Task 22 adds no dependency and no `dedupe/trash.rs`, `THIRD-PARTY-NOTICES.md` needs no `trash` entry, and "Put Back" in Finder keeps working because the file goes through `NSFileManager`. The original text follows for history. **DD-4 (original): `trash` is a new dependency.** The owner's decision (optional OS Trash) cannot be met with lockfile crates. Task 22 adds `trash` 5.x (MIT) behind `dedupe::trash::TrashBackend`, records it in `THIRD-PARTY-NOTICES.md`, and runs `cargo fetch` once. On macOS it uses the Finder method so "Put Back" works. `dedupe.undo.trashed` shows "(Volver a poner)" only when `cfg!(target_os = "macos")` (spec open question 8).
- **DD-5: per-profile data stays out of `profiles/`.** The archive branch edits `profiles/{mod,builtin}.rs`. The container order defaults (spec §6.3) live in `settings.dedupe.container_order`. The off-card `save_locations` (spec §8.2) are a static table in `dedupe/saves.rs`, keyed by profile id: `muos` → internal, `anbernic-stock` → none, all others → none. Moving them into `Profile` is a follow-up.
- **DD-6: `OrganizeAction::SetAside` instead of a new `ItemAction`.** The organise plan has `OrganizeAction { Auto, Review, Left }`. Dedupe adds `SetAside { keeper: PathBuf, relation: Relation, reason: KeepReason, dest: PathBuf }`. `LeftReason::Duplicate { of }` stays deserialisable for old plans but is no longer produced. `LeftReason::InLibrary { path, relation }` is new.
- **DD-7: UI uses the `src/components/ui` primitives.** They exist now: `RadioGroup`, `ToggleGroup`, `Collapsible`, `Badge`, `Alert`, `AlertDialog`, `Progress`, `EmptyState`, `Kbd`, `DropdownMenu`, `Checkbox`, `Switch`. The older organise rule ("plain markup only") no longer applies. The design pass revised three spec details (see "UI design brief", revisions R1–R3).
- **DD-8: open questions of spec §18 get these defaults.** All can change later without changing the format. Q1 `has_save` above everything automatic. Q2 no P/C download. Q3 translations and hacks in their own class. Q4 `_Duplicados`/`_Duplicates` fixed at first run. Q5 accept `~/Documents/ROMS _Duplicados`. Q6 zip vs raw has no preference. Q7 the Organise step defaults to "Apartar". Q8 per DD-4. Q9 the quarantine stays on the card, with the honest wording. Q10 accept level 3 false negatives. Q11 keep 500 files / 25 % and re-tune in Task 33.

## Overlap map with `feat/organize-disc-archives`

| File | Archive branch changes | Dedupe tasks | Marker |
|---|---|---|---|
| `crates/coverly-core/src/organize/inspect.rs` | `container_for` magic-first, zip code moved to `organize/archive/zip.rs`, `Inspection.disc` | 26 | BLOCKED |
| `crates/coverly-core/src/organize/{plan,target,apply,evidence,walk}.rs` | ExtractPlan, new LeftReasons, disc layout, extract apply | 23, 26 | BLOCKED |
| `crates/coverly-core/src/organize/archive/*` (new there) | `ArchiveSource` | 25 (adapter) | BLOCKED |
| `crates/coverly-core/src/identify/sniff.rs` | disc rows, product codes, container magic | 25 | BLOCKED |
| `crates/coverly-core/src/write/mod.rs` | `Op::{ExtractTemp, ExtractSource, Extracted, Retired, RetiredPurged}`, prune exemption | 21 | BLOCKED |
| `crates/coverly-core/Cargo.toml`, `Cargo.lock`, MSRV 1.93 | new crates | 22 | BLOCKED |
| `crates/coverly-core/src/scan/multifile.rs` | `.gdi`/`.cue` twin rule | 8 reads it only | none (read-only use) |
| `crates/coverly-cli/src/organize.rs` | `--extract`, `--purge-retired` | 26 | BLOCKED |
| `src-tauri/src/ipc.rs`, `src-tauri/src/lib.rs` | `organize_purge_retired` | 28 | BLOCKED (small textual merge) |
| `src/components/organize/*`, `src/lib/organize.ts`, `src/ipc/types.ts`, mock organise | extract column, notes | 29, 31 | BLOCKED |
| `locales/{es,en}/ui.json` | organise extract strings | 19, 20, 30, 31 | light (insert the `dedupe` block alphabetically, never at the end of the file) |

Everything else in this plan is new files or files the archive branch does not touch.

---

## UI design brief (frontend-design pass)

**Subject.** A tidy-up tool inside a cover-art manager for handheld emulation libraries. The audience is a person holding a 128 GB SD card with 20 000 files. Their job is to see, in one pass, which copy of each game stays and where the others go, and to trust it enough to press one button.

**Palette (existing brand tokens only, no new colours).**

| Name | Light | Dark | Role here |
|---|---|---|---|
| Shelf paper `--cr-surface` | `#fbfbf9` | `#1a1e1d` | the system panel, the shelf |
| Tray well `--cr-bg` | `#f2f3ef` | `#121514` | the recessed tray under each keeper |
| Hairline `--cr-border` | `#d3d7cf` | `#2d3331` | group separators, the shelf rule |
| Ink `--cr-text` | `#1b1e1b` | `#eceeea` | names, sentences |
| Buckram `--cr-primary` | `#0e5e6f` | `#5db6c4` | shelf cap, checked radio, focus, primary button only |
| Warning `--cr-warning` | `#9f4b0c` | `#f0a35a` | conflicts, with an icon and a word |

Manila is never used: it means "Coverly applied art, still undoable". Red appears only for a real error.

**Type.** Archivo wdth 75 / 800 at 24px for the screen title, and at 32px for the summary counts only (`type.app.display`, brand "summary counts"). Atkinson Hyperlegible Next 13/1.45 for everything else. Atkinson Hyperlegible Mono 12/1.5 with `tabular-nums` for file names, paths, sizes and serials. No all-caps labels and no eyebrows. System triggers read as sentences.

**Layout concept.** One continuous panel per system, groups separated by hairlines. Each group is a *shelf* (the keeper, carrying the brand shelf rule: 1px border with a 3px Buckram cap on the left) above a *tray* (the extras, recessed `--cr-bg` well inset 24px). Every tray row carries its destination in mono. Sizes sit right-aligned in a fixed mono column, so the freed bytes line up down the page. Everything is left-aligned.

```
Duplicados                                                  (Archivo 24)
/Volumes/SD/ROMS                                                  (mono)

89 copias de más en 88 juegos                               (Archivo 32)
Quitarías 1,53 GB de ROMS/. Es una propuesta: todavía no se ha movido nada.

[Copias exactas 89] [Otros formatos 4] [Otras versiones 37]   +37 archivos
Qué hacer con las copias  (•) Apartar en _Duplicados/  Cambiar…   ( ) Papelera

▾ Master System — 23 juegos, 23 copias                               7,7 MB
 ─────────────────────────────────────────────────────────────────────────
 Ecco the Dolphin                                 [⧉ Copia exacta]
 ▌(•) Ecco the Dolphin.sms                              512 KB  Se queda
 ▌    Se queda: tiene portada; la otra no.
 ┊ ┌ tray ───────────────────────────────────────────────────────────┐
 ┊ │ ( ) Quedarme con esta   Adventures of Dolphins.sms     512 KB   │
 ┊ │     → _Duplicados/SMS/Adventures of Dolphins.sms   + 1 portada  │
 ┊ └─────────────────────────────────────────────────────────────────┘
 [ ] No tocar este juego
═══════════════════════════════════════════════════════════════ sticky ═
[Apartar 89 archivos]  Exportar informe  Cancelar   Fuera de ROMS/ 1,53 GB
```

**Principles.**
1. The shelf/tray reading is the one memorable element. Everything around it is quiet: neutrals, hairlines, no cards, no shadows, no gradients.
2. Motion only answers the user. Changing the keeper slides the shelf cap to the new row and drops the old keeper into the tray (`duration.base`, `easing.standard`, 0 ms under reduced motion). There is no entrance animation.
3. Every count is exact and says where bytes leave ("Fuera de ROMS/"), never "liberar" on its own.
4. A state is never colour alone. Relation and conflict carry an icon and a word, and the keeper is the checked radio, not the cap colour.

**Review against generic defaults, and what changed.**
- *Checked:* not the SaaS card kit (groups are rows in one panel, not rounded cards), not cream + terracotta (brand neutrals + Buckram), no mono for small labels except data, no "→" appended to buttons (the "→" appears only before a destination path, where it means "goes to").
- **R1 (revised from spec §11.3):** the spec showed the confidence number ("1.00", "0.97") on every group. That is engineering vocabulary. The number moves into the badge tooltip. Only below 1.00 does the badge itself change, to the word "Sin confirmar" or "Otro formato" with its icon.
- **R2 (revised):** the spec's system trigger `SMS  Master System  23 juegos · 23 copias · 7,7 MB` used middle-dot meta strings and a bare id. It becomes a sentence, "Master System: 23 juegos, 23 copias", with the bytes in the right-aligned size column, so system totals line up with row sizes. The system id stays as a `title` and in the export.
- **R3 (revised):** the spec used Unicode glyphs (⧉ ◫ ⟲ ?) for relation icons. Those render inconsistently in WebKit vs Chromium webviews. Use `lucide-react` icons: `Copy` (exact), `Disc3` (other format), `History` (other version), `CircleHelp` (not confirmed), `TriangleAlert` (conflict). Size 14, `aria-hidden`, always next to the word.
- *Kept from the spec:* the summary sentence carries the numbers. Archivo 32 is applied to the first sentence only, because the count is the screen's answer.

---

## File structure

```
crates/coverly-core/
├─ src/lib.rs                          + pub mod dedupe
├─ src/identify/names.rs               + parse_tags, NameTags, Revision, ReleaseClass, DumpFlags, ModClass
├─ src/identify/dat.rs                 parse_game/parse_rom keep serial, region, cloneof, isbios
├─ src/identify/hash.rs                HeaderRule::SmsCopier, N64 byte order, HashOutcome v2 (payload sha1)
├─ src/systems/systems.toml            sms/gg header = "sms-copier"
├─ src/cache/mod.rs                    schema v3: dat_roms.{serial,region,cloneof,isbios}, content_keys,
│                                        content_key_index, dedupe_choices; accessors
├─ src/settings/{model,validate}.rs    + DedupeSettings (settings.dedupe)
├─ src/dedupe/mod.rs                   public types, re-exports
├─ src/dedupe/source.rs                EntryListing, DiscProbe, DiscId, VolumeFingerprint, ZipListing
├─ src/dedupe/testing.rs               FakeDiscProbe, FakeListing, synthetic builders (cfg test-util)
├─ src/dedupe/units.rs                 Unit, UnitKind, build_units (library + organise facts), exclusions
├─ src/dedupe/keys.rs                  level 1 keys + bucketing, title_key, level 3 keys
├─ src/dedupe/group.rs                 union-find per system, weakest link, ReportOnly reasons
├─ src/dedupe/saves.rs                 save detection, blank check, conflicts, SAVE_LOCATIONS
├─ src/dedupe/keeper.rs                rules, KeepReason, container order
├─ src/dedupe/companions.rs            same-stem, art slots, shared stems, AppleDouble, gamelist entries
├─ src/dedupe/plan.rs                  plan_dedupe, DedupeProgress, phases, cache use, remembered choices
├─ src/dedupe/disc.rs                  IsoDiscProbe (Phase B)
├─ src/dedupe/archive.rs               ArchiveEntryListing over organize::archive::ArchiveSource (Phase B)
├─ src/dedupe/trash.rs                 TrashBackend, OsTrash (Phase B)
├─ src/dedupe/apply.rs                 apply_dedupe, resume_dedupe, quarantine paths, preflight (Phase B)
├─ src/write/mod.rs                    JobKind::Dedupe, Op::Trashed, Header.quarantine, prune, header_kind (Phase B)
├─ src/write/undo_organize.rs          Trashed → manual-restore report (Phase B)
├─ src/organize/{plan,target,walk,apply}.rs   SetAside, InLibrary, mark_duplicates removed (Phase B)
├─ src/scan/mod.rs                     quarantine folder skipped
└─ tests/{dedupe_units.rs, dedupe_keys.rs, dedupe_group.rs, dedupe_saves.rs, dedupe_keeper.rs,
          dedupe_companions.rs, dedupe_plan.rs, dedupe_apply.rs, dedupe_undo.rs, dedupe_disc.rs,
          dedupe_real_sample.rs, names_tags.rs}
crates/coverly-cli/
├─ src/main.rs                         + Dedupe subcommand
├─ src/dedupe.rs                       run(): plan / apply / resume, table + json
├─ src/undo.rs                         kind_name Dedupe, trashed lines
└─ tests/cli_dedupe.rs
src-tauri/
├─ src/dedupe/{mod.rs, view.rs}        DedupeManager, wire types, events
├─ src/ipc.rs, src/lib.rs              dedupe_* commands
└─ tests/{dedupe_state.rs, contract.rs}
src/
├─ ipc/dedupeTypes.ts                  wire types (re-exported from types.ts in Phase B)
├─ ipc/mock/mockDedupe.ts (+ test)     deterministic fixture backend
├─ lib/dedupe.ts (+ test)              reason → i18n, relation meta, byte totals, deltas
├─ stores/dedupeStore.ts (+ test)
├─ components/dedupe/{DedupeOptions, SystemSection, DupGroup, KeeperRow, TrayRow, ConflictGroup,
│                     UnchangedList, DedupeResult, dedupe.css}.tsx (+ tests)
├─ screens/DuplicatesScreen.tsx (+ test), App.tsx, stores/navStore.ts
├─ components/library/Toolbar.tsx, components/fill/JobsPanel.tsx
├─ components/organize/{OptionsBar, OrganizeResult}.tsx, screens/OrganizeScreen.tsx
└─ components/settings/{LibrarySection.tsx, DedupeSettings.tsx}
locales/{en,es}/ui.json
docs/04-architecture/dedupe.md (new), organize.md, identification.md, desktop-app.md, settings.md,
README.md, README.es.md, CHANGELOG.md, docs/TRACKER.md
```

Synthetic builders live in `crates/coverly-core/src/dedupe/testing.rs` behind `cfg(any(test, feature = "test-util"))` and are reused by integration tests through `coverly_core::dedupe::testing`. They include `rom(bytes)`, `nes(header, prg)`, `smc(copier, body)`, `zip_one(name, bytes, method)`, `arcade_zip(entries)`, `cue_bin(tracks, layout)`, `chd_header(sha1, rawsha1)`, `ps_card_blank()`, `library(tree)`.

---

## Phase A — can start now

## Group A1: Name tags, DAT fields, cache, hashes, settings (spec D1–D2)

### Task 1: Core: `identify::names::parse_tags`

**Overlap:** none.

**Files:**
- Modify: `crates/coverly-core/src/identify/names.rs`
- Test: `crates/coverly-core/tests/names_tags.rs`

**Interfaces (produced):**
```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Revision { pub major: u16, pub minor: u16, pub letter: u8 }   // V1.1 → (1,1,0); Rev 1 → (1,0,0); Rev A → (0,0,b'A'); v1.02 → (1,2,0)
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReleaseClass { Retail, Beta, Proto, Demo, Sample }               // Ord = preference order
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DumpFlags { pub verified: bool, pub bad: bool, pub overdump: bool, pub alternate: bool }
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModClass { Original, Hack, Trainer, Translation, Fixed, Pirate, Unlicensed }
pub struct NameTags {
    pub title_key: String,          // compare_key of the title with known groups stripped, unknown groups kept
    pub revision: Option<Revision>,
    pub revision_text: Option<String>,   // as written, for KeepReason::Revision
    pub class: ReleaseClass,
    pub flags: DumpFlags,
    pub modification: ModClass,
    pub disc: Option<u8>,
    pub regions: Vec<Region>,
    pub unknown_groups: Vec<String>,
}
pub fn parse_tags(name: &str) -> NameTags;
pub fn has_modification_tag(name: &str) -> bool;  // [T+…] [h…] Hack Translated Traducido Undub (Patched)
```

- [ ] **Step 1: Write the failing tests** in `tests/names_tags.rs` (table-driven):

```rust
#[test] fn revisions_normalise() { /* "Tetris (JUE) (V1.1) [!]" → (1,1,0) "V1.1"; "X (World) (Rev 1)" → (1,0,0); "(Rev A)" → letter A; "(v1.02)" → (1,2,0) */ }
#[test] fn tetris_v10_and_world_rev1_share_a_title_key() { /* "Tetris (Japan) (En)" and "Tetris (World) (Rev 1)" → "tetris" */ }
#[test] fn disc_number_is_kept_in_the_key() { /* "(Disc 1)" vs "(Disc 2)" → different title_key, disc Some(1)/Some(2) */ }
#[test] fn unknown_groups_stay_in_the_key() { /* "Hippo Teeth (Sporty Time & Fun)" vs "(Mini Time & Fun)" → different */ }
#[test] fn known_groups_are_stripped() { /* (SGB Enhanced) (GB Compatible) (Rumble Version) (Virtual Console) dates (Beta) (Proto 2) (Demo) (Sample) (Kiosk) */ }
#[test] fn goodtools_modification_flags_set_the_class_and_stay_in_the_key() { /* [h1] → Hack, [t1] → Trainer, [T+Eng] → Translation, [f1] → Fixed, [p1] → Pirate; key differs from the clean name */ }
#[test] fn dump_flags() { /* [!] verified, [b1] bad, [o] overdump, [a] alternate; key unchanged */ }
#[test] fn release_classes_order() { /* Retail < Beta < Proto < Demo < Sample */ }
#[test] fn translated_en_is_its_own_class() { /* "Heracles no Eikou (Japan)" vs "(Translated En)" → different key */ }
#[test] fn modification_tag_detector() { /* "[T+Spa]", "Undub", "(Patched)", "Traducido" → true; "Tetris (USA)" → false */ }
```

- [ ] **Step 2: Run to see them fail:** `cargo test -p coverly-core --test names_tags` (it fails because `parse_tags` is missing).
- [ ] **Step 3: Implement.** Tokenise `(...)` and `[...]` groups with the existing group scanner used by `parse_name`. Map known groups with `Region::from_no_intro` / `from_goodtools` and static tables for classes and flags. Build the key as `compare_key(title + " " + kept_groups.join(" "))`, where kept groups are disc, unknown groups and the modification class. `ParsedName` is unchanged.
- [ ] **Step 4: Run tests and gates.**
- [ ] **Step 5: Commit:** `git add crates/coverly-core && git commit -m "feat(core): parse revision, class and modification tags from catalogue names"`

### Task 2: Core: DAT parser keeps `serial`, `region`, `cloneof`, `isbios`

**Overlap:** none.

**Files:**
- Modify: `crates/coverly-core/src/identify/dat.rs` (`DatGame`, `parse_game`, `parse_rom`)
- Modify: `crates/coverly-core/src/cache/mod.rs` (`DatRow` gains `serial: Option<String>`; `DatGame` gains `cloneof`, `region`, `is_bios`)
- Test: `crates/coverly-core/tests/dat.rs` (append)

- [ ] **Step 1: Failing tests:** `parses_serial_from_rom_line_and_game_block` (Redump-style `serial "SLUS-00543"`), `parses_cloneof_and_region`, `isbios_yes_and_bios_name_mark_bios`, `old_dats_without_fields_still_parse`, `serial_is_normalised` (`SLUS_005.43` → `SLUS-00543`, via `pub fn normalise_serial(&str) -> Option<String>` in `dat.rs`).
- [ ] **Step 2:** `cargo test -p coverly-core --test dat` fails.
- [ ] **Step 3: Implement.** Unknown keys stay ignored. `normalise_serial` uppercases, maps `_` and the first `.` removal to the `AAAA-NNNNN` form, and accepts Sega `T-5704H`, `MK-81009` and `HDR-0001` unchanged.
- [ ] **Step 4:** Tests and gates.
- [ ] **Step 5:** `git commit -m "feat(core): keep serial, region, clone and BIOS fields from DATs"`

### Task 3: Core: cache schema v3

**Overlap:** none.

**Files:**
- Modify: `crates/coverly-core/src/cache/mod.rs` (`SCHEMA_VERSION = 3`, `SCHEMA_V3`, migration arms `0`, `1`, `2`)
- Test: `crates/coverly-core/tests/cache.rs` (append)

**Interfaces (produced, on `Cache`):**
```rust
pub struct ContentRow { pub volume_uuid: String, pub path: PathBuf, pub size: u64, pub mtime_ns: i64,
                        pub file_id: Option<u64>, pub keys: ContentKeys, pub serials: Option<DiscIdCached> }
pub fn content_row(&self, volume_uuid: &str, path: &Path, size: u64, mtime_ns: i64) -> Result<Option<ContentRow>, CacheError>;
pub fn content_by_file_id(&self, volume_uuid: &str, file_id: u64) -> Result<Option<ContentRow>, CacheError>;
pub fn put_content_row(&self, row: &ContentRow) -> Result<(), CacheError>;
pub fn rekey_content(&self, volume_uuid: &str, from: &Path, to: &Path) -> Result<(), CacheError>;
pub fn dat_by_serial(&self, dat: &str, serial: &str) -> Result<Vec<DatRow>, CacheError>;
pub fn dedupe_choice(&self, system: &str, keeper_key: &str) -> Result<Option<StoredChoice>, CacheError>;
pub fn put_dedupe_choice(&self, c: &StoredChoice) -> Result<(), CacheError>;
pub fn clear_dedupe_choices(&self, systems: &[String]) -> Result<usize, CacheError>;
```
`ContentKeys` and `DiscIdCached` are defined in `dedupe/mod.rs` (Task 6). Until then this task stores `keys_json: String`, and Task 6 adds the typed wrappers. Do not introduce a module cycle: the cache stores JSON, and `dedupe` (de)serialises it.

SQL (v3):
```sql
ALTER TABLE dat_roms ADD COLUMN serial TEXT; ALTER TABLE dat_roms ADD COLUMN region TEXT;
ALTER TABLE dat_roms ADD COLUMN cloneof TEXT; ALTER TABLE dat_roms ADD COLUMN isbios INTEGER NOT NULL DEFAULT 0;
CREATE INDEX IF NOT EXISTS dat_roms_serial ON dat_roms (dat, serial);
CREATE TABLE IF NOT EXISTS content_keys (volume_uuid TEXT NOT NULL, path TEXT NOT NULL, size INTEGER NOT NULL,
  mtime_ns INTEGER NOT NULL, file_id INTEGER, keys_json TEXT NOT NULL, serials_json TEXT, volume_fp TEXT,
  PRIMARY KEY (volume_uuid, path));
CREATE INDEX IF NOT EXISTS content_keys_file_id ON content_keys (volume_uuid, file_id);
CREATE TABLE IF NOT EXISTS content_key_index (key TEXT NOT NULL, volume_uuid TEXT NOT NULL, path TEXT NOT NULL,
  PRIMARY KEY (key, volume_uuid, path));
CREATE TABLE IF NOT EXISTS dedupe_choices (system TEXT NOT NULL, keeper_key TEXT NOT NULL, signature TEXT NOT NULL,
  saves_fp TEXT NOT NULL, chosen_at INTEGER NOT NULL, PRIMARY KEY (system, keeper_key));
```

- [ ] **Step 1: Failing tests:** `migrates_v2_to_v3_keeping_rows`, `fresh_database_is_v3`, `content_row_round_trip`, `rekey_moves_the_row_and_its_index`, `dat_by_serial_uses_the_index`, `choices_round_trip_and_clear_by_system`, `a_v3_database_opened_by_v2_code_is_refused_cleanly` (the existing "newer schema" error path).
- [ ] **Step 2:** `cargo test -p coverly-core --test cache` fails.
- [ ] **Step 3:** Implement. The `dev_t` is never stored. `volume_uuid` comes from `crate::volumes` (existing module; add `pub fn volume_uuid(path: &Path) -> Option<String>` if missing, with fallback `"path:<mount point>"`).
- [ ] **Step 4:** Tests and gates.
- [ ] **Step 5:** `git commit -m "feat(core): cache schema v3 for content keys, serials and keeper choices"`

### Task 4: Core: `HashOutcome` v2, SMS/GG copier header, N64 byte order

**Overlap:** none.

**Files:**
- Modify: `crates/coverly-core/src/identify/hash.rs`, `crates/coverly-core/src/systems/systems.toml` (`sms`, `gg`: `header = "sms-copier"`), `crates/coverly-core/src/systems/mod.rs` (parse)
- Test: `crates/coverly-core/tests/identify_hash.rs` (append)

**Interfaces:**
```rust
pub enum HeaderRule { /* … */ SmsCopier }            // 512 bytes when len % 16384 == 512
pub struct RomHashes { pub full: Digests, pub headerless: Option<Digests>, pub payload_sha1: Option<String> } // v2
pub enum HashOutcome {
    Content(RomHashes),
    ZipEntry { crc32: String, size: u64, inner_name: String, payload_sha1: Option<String> }, // v2: set when decoded within budget
    TooLarge { size: u64 }, NameOnly,
}
pub fn n64_to_z64(head: &[u8]) -> Option<N64Order>;  // .v64 byte-swapped, .n64 little-endian
pub const HASH_OUTCOME_VERSION: u32 = 2;               // stored in the quick_ids JSON; v1 rows are re-hashed lazily
```

- [ ] **Step 1: Failing tests:** `sms_copier_header_is_skipped`, `n64_three_orders_share_payload_sha1`, `single_entry_zip_payload_sha1_matches_raw_file` (stored and deflate), `zip_entry_over_budget_has_no_payload_sha1`, `v1_quick_id_rows_are_treated_as_missing`, `headerless_sha1_is_computed_in_the_same_pass` (count reads with a wrapper reader: one pass).
- [ ] **Step 2:** `cargo test -p coverly-core --test identify_hash` fails.
- [ ] **Step 3:** Implement. `payload_sha1` = SHA1 of headerless bytes, N64-normalised. The zip payload decode uses the existing bounded zip reader (`STREAM_CAP`) and runs only when the caller asks (`HashMode::WithPayload`), so fill/scan cost is unchanged.
- [ ] **Step 4:** Tests and gates (the existing `identify_hash` and `fill` tests must stay green).
- [ ] **Step 5:** `git commit -m "feat(core): payload hashes for zips and N64, SMS/GG copier header"`

### Task 5: Core: `settings.dedupe`

**Overlap:** none.

**Files:**
- Modify: `crates/coverly-core/src/settings/model.rs`, `crates/coverly-core/src/settings/validate.rs`, `crates/coverly-core/src/settings/patch.rs` (if the patch list enumerates groups)
- Test: `crates/coverly-core/tests/settings_file.rs` (append)

**Interfaces:**
```rust
#[serde(rename_all = "kebab-case")] pub enum DedupeLevel { Exact, Format, Version }
#[serde(rename_all = "kebab-case")] pub enum DedupeActionSetting { Quarantine, Trash }
#[serde(rename_all = "snake_case")] pub enum KeepRule { HasSave, Played, InLibrary, Verified, Retail, Region, Revision, Container, Cover, CatalogueName }
pub struct RuleToggle { pub rule: KeepRule, pub on: bool }
pub struct DedupeSettings {
    pub levels: Vec<DedupeLevel>,                       // default [exact, format, version]; version acts only with one_game_one_rom
    pub one_game_one_rom: bool,                         // false
    pub action: DedupeActionSetting,                    // quarantine
    pub quarantine_dir: Option<PathBuf>,                // None = next to the ROMs folder
    pub quarantine_name: Option<String>,                // fixed at first run from the app language
    pub keep_rules: Vec<RuleToggle>,                    // spec §6.1 order, all on
    pub container_order: BTreeMap<String, BTreeMap<String, Vec<String>>>, // profile|* → system|disc|cart|* → order; DD-5 defaults
    pub exclude_systems: Vec<String>,                   // ["ports", "bios", "apps"]
    pub max_dat_downloads: u32,                         // 12
    pub organize_step: OrganizeDedupeMode,              // set-aside | report | off; default set-aside
}
```

- [ ] **Step 1: Failing tests:** `defaults_match_the_spec`, `old_settings_file_without_dedupe_loads_defaults`, `duplicate_rule_is_rejected`, `quarantine_inside_a_system_folder_is_rejected` (validation needs the library root, so validate the relative shape here and the absolute check in Task 23), `container_order_default_for_anbernic_puts_chd_first`.
- [ ] **Step 2:** `cargo test -p coverly-core --test settings_file` fails.
- [ ] **Step 3:** Implement with `#[serde(default)]` on the group. The `SETTINGS_VERSION` is unchanged (additive).
- [ ] **Step 4:** Tests and gates (`src/lib/settingsRules.ts` mirrors validation in Task 31).
- [ ] **Step 5:** `git commit -m "feat(core): duplicates settings with keeper rules and container order"`

## Group A2: Detection core (spec D3–D5)

### Task 6: Core: `dedupe` module types

**Overlap:** none (`lib.rs` is not touched by the archive branch).

**Files:**
- Create: `crates/coverly-core/src/dedupe/mod.rs`
- Modify: `crates/coverly-core/src/lib.rs` (`pub mod dedupe;`)
- Test: unit tests in `dedupe/mod.rs` (serde round trips)

**Interfaces (produced):**
```rust
#[serde(rename_all = "kebab-case")] pub enum Relation { Exact, Format, Version }   // Ord: Exact < Format < Version (weakest = max)
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ReportWhy { LevelTwoOff, Unconfirmed, HeaderDiffers, BuildDiffers, PartialSet, SerialConflict,
                     Modified, ArcadeNoDat, BigGroup { members: usize }, UserSkipped, ChdMetadataDiffers }
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum GroupAction { Act, ReportOnly { why: ReportWhy }, Conflict { saves: Vec<SaveInfo> } }
pub use keeper::KeepReason;  // enum per spec §6.2, plus Played { playcount }
pub struct Member { pub id: usize, pub unit: UnitRef, pub relation: Relation, pub size: u64,
                    pub travels: Vec<Travel>, pub stays: Vec<SharedStay>, pub dest: Option<PathBuf>,
                    pub catalogue_name: Option<String>, pub serials: Vec<String>, pub confidence: f32 }
pub struct DupGroup { pub id: u64 /* stable: group_id(system, sorted content keys) */, pub system: String,
                      pub title: String, pub keeper: usize, pub reason: KeepReason, pub runner_up: Option<(usize, KeepReason)>,
                      pub members: Vec<Member>, pub action: GroupAction, pub tie: bool }
pub struct DedupePlan { pub version: u32 /* 1 */, pub root: PathBuf, pub roms_dir: PathBuf, pub quarantine: PathBuf,
                        pub same_volume: bool, pub removable: bool, pub profile_id: String,
                        pub groups: Vec<DupGroup>, pub unchanged: Vec<u64>, pub not_roms: Vec<PathBuf>,
                        pub unreadable: Vec<PathBuf>, pub no_catalogue: Vec<String>, pub chd_unread: usize,
                        pub off_card_saves: Vec<String>, pub totals: Totals }
pub struct Totals { pub groups: usize, pub extras: usize, pub bytes: u64, pub conflicts: usize, pub by_system: BTreeMap<String, (usize, usize, u64)> }
pub struct DedupeOptions { pub levels: Vec<DedupeLevel>, pub one_game_one_rom: bool, pub rules: Vec<RuleToggle>,
                           pub container_order: …, pub systems: Option<Vec<String>>, pub offline: bool,
                           pub quarantine: QuarantineChoice, pub forced_keepers: Vec<PathBuf>, pub reset_choices: bool }
```

- [ ] **Step 1: Failing test:** `plan_json_round_trips` and `relation_weakest_link_order`.
- [ ] **Step 2:** `cargo test -p coverly-core dedupe::` fails.
- [ ] **Step 3:** Implement the types and empty submodules (`mod units; mod keys; …` with `todo!`-free stubs that compile). Add `#[non_exhaustive]` on the public enums that will grow.
- [ ] **Step 4:** Tests and gates.
- [ ] **Step 5:** `git commit -m "feat(core): duplicates plan types"`

### Task 7: Core: `EntryListing` and `DiscProbe` traits, zip listing, fakes

**Overlap:** none (new files; the zip listing calls `identify::hash` and the existing `organize::inspect` public functions read-only).

**Files:**
- Create: `crates/coverly-core/src/dedupe/source.rs`, `crates/coverly-core/src/dedupe/testing.rs`
- Test: `crates/coverly-core/tests/dedupe_units.rs` (start)

**Interfaces:**
```rust
pub struct ListedEntry { pub name: String, pub size: u64, pub crc32: Option<u32> }
pub trait EntryListing: Send + Sync {
    fn list(&self, path: &Path) -> io::Result<Option<Vec<ListedEntry>>>;               // None = not an archive this lister reads
    fn payload_sha1(&self, path: &Path, entry: usize, rule: HeaderRule, budget: u64) -> io::Result<Option<String>>; // None = over budget
}
pub struct ZipListing;   // zip only, via the zip crate; used until Task 25
pub struct VolumeFingerprint { pub space_size: u32, pub created: [u8; 17], pub modified: [u8; 17], pub boot_sha1: String }
pub enum DiscIdSource { SystemCnf, ParamSfo, IpBin, UmdData, Chd, M3uPosition }
pub struct DiscId { pub serials: Vec<String>, pub disc_no: Option<u8>, pub volume: Option<VolumeFingerprint>, pub source: DiscIdSource }
pub trait DiscProbe: Send + Sync { fn probe(&self, path: &Path, system: &str) -> io::Result<Option<DiscId>>; }
pub struct NoDiscProbe;  // returns None: level 3 disabled with the "needs disc reading" note
```

- [ ] **Step 1: Failing tests:** `zip_listing_reads_central_directory_without_decoding`, `zip_payload_sha1_respects_budget`, `no_disc_probe_disables_level_three`, `fake_disc_probe_returns_scripted_ids`.
- [ ] **Step 2:** `cargo test -p coverly-core --test dedupe_units` fails.
- [ ] **Step 3:** Implement. Put the fakes in `testing.rs` behind `cfg(any(test, feature = "test-util"))`.
- [ ] **Step 4:** Tests and gates.
- [ ] **Step 5:** `git commit -m "feat(core): archive listing and disc probe seams for duplicates"`

### Task 8: Core: units, exclusions, sets, links, BIOS, not-a-ROM

**Overlap:** none. It reads `scan::LibraryScan` and `scan/multifile.rs` output and changes neither.

**Files:**
- Create: `crates/coverly-core/src/dedupe/units.rs`
- Test: `crates/coverly-core/tests/dedupe_units.rs`

**Interfaces:**
```rust
pub enum UnitKind { Single, Zip, Arcade, DiscSet, Chd, Image /* pbp cso rvz iso */, MultiDisc }
pub struct Unit { pub id: usize, pub system: String, pub primary: PathBuf, pub files: Vec<PathBuf>, pub kind: UnitKind,
                  pub size: u64, pub payload_size: Option<u64>, pub mtime_ns: i64, pub file_id: Option<u64>,
                  pub patches: Vec<PathBuf>, pub rel_dir: PathBuf, pub stem: String, pub ext: String, pub in_library: bool }
pub struct UnitSet { pub units: Vec<Unit>, pub excluded: Vec<(PathBuf, Exclusion)> }
pub enum Exclusion { Folder, Bios, NotARom, Symlink, HardLinkTwin, Partial, Unreadable, MultiGameArchive, Quarantine }
pub fn units_from_scan(scan: &LibraryScan, db: &SystemDb, opts: &UnitOptions) -> UnitSet;
pub fn units_from_facts(facts: &[(PathBuf, organize::Facts)], system_of: &dyn Fn(usize) -> Option<String>) -> UnitSet; // used in Task 26
```

- [ ] **Step 1: Failing tests:**
```rust
#[test] fn ports_bios_apps_art_and_saves_folders_are_excluded() {}
#[test] fn bios_twins_are_never_units() { /* PS/scph1001.bin and PS/scph5501.bin, same synthetic bytes → Bios */ }
#[test] fn bios_by_dat_isbios_and_arcade_bios_sets() { /* neogeo.zip, pgm.zip in FBNEO → Bios */ }
#[test] fn zero_byte_and_html_named_zip_are_not_roms() {}
#[test] fn symlink_is_not_a_unit_and_hard_links_are_one() {}
#[test] fn cue_with_tracks_is_one_unit_and_tracks_are_not_units() {}
#[test] fn two_cues_over_one_bin_merge_into_one_unit() {}
#[test] fn m3u_and_loose_cue_of_the_same_disc_merge() {}
#[test] fn soft_patches_attach_to_their_rom() { /* Game.sfc + Game.ips → patches = [Game.ips] */ }
#[test] fn quarantine_folder_is_never_walked() { /* <root>/_Duplicados/GB/x.gb excluded; also when roms_root = "" */ }
#[test] fn apple_double_and_hidden_files_are_skipped() {}
```
- [ ] **Step 2:** `cargo test -p coverly-core --test dedupe_units` fails.
- [ ] **Step 3:** Implement. Reuse `scan::classify` predicates (`is_bios_name`, hidden, AppleDouble) and `RomEntry.companions` for sets. Build a reference graph (m3u → cue → bin) and merge components. The libretro `System.dat` firmware names come from the existing BIOS list in `scan::classify`. Extend that list only if a spec §5.1 name is missing, and if so add it in this task with a test.
- [ ] **Step 4:** Tests and gates.
- [ ] **Step 5:** `git commit -m "feat(core): duplicates units with sets, exclusions and BIOS guard"`

### Task 9: Core: level 1 keys and bucketing

**Overlap:** none.

**Files:**
- Create: `crates/coverly-core/src/dedupe/keys.rs`
- Test: `crates/coverly-core/tests/dedupe_keys.rs`

**Interfaces:**
```rust
pub enum ExactKey { Payload { sha1: String, size: u64 }, ZipCrc { crc32: u32, size: u64 } /* unconfirmed */,
                    Arcade { entries: Vec<(u32, u64)> }, DiscTracks { tracks: Vec<String>, layout: String },
                    Chd { sha1: String }, ChdRaw { rawsha1: String } }
pub struct KeyedUnit { pub unit: usize, pub exact: Option<ExactKey>, pub confirmed: bool, pub patch_set: Vec<String>,
                       pub header: Option<Vec<u8>>, pub dat_game: Option<(String, String)> }
pub fn level_one(units: &[Unit], ctx: &KeyCtx, progress: &dyn Fn(Phase, usize, usize), cancel: &AtomicBool) -> Vec<KeyedUnit>;
pub fn parse_chd_header(head: &[u8]) -> Option<(u32 /*version*/, Option<String> /*sha1*/, Option<String> /*rawsha1*/)>;
pub fn track_layout(cue_text: &str) -> String;   // track count, mode, INDEX/PREGAP/POSTGAP; names, comments, REM ignored
```
`KeyCtx` carries the cache, the `quick_ids` reuse, the `EntryListing`, the header rules per system, `max_hash_mb` and the volume UUID.

- [ ] **Step 1: Failing tests:**
```rust
#[test] fn nes_with_and_without_ines_header_share_a_key() {}
#[test] fn nes_headers_that_differ_are_report_only() { /* same PRG/CHR, mapper 1 vs 4 → keys equal, header bytes kept for Task 11 */ }
#[test] fn smc_copier_and_sfc_share_a_key() {}
#[test] fn sms_512_header_shares_a_key() {}
#[test] fn n64_orders_share_a_key() {}
#[test] fn raw_and_single_entry_zip_share_a_key_after_confirmation() {}
#[test] fn zip_over_budget_is_unconfirmed() {}
#[test] fn arcade_zips_with_renamed_entries_share_a_key_and_one_changed_entry_does_not() {}
#[test] fn cue_text_differences_do_not_matter_but_index_differences_do() {}
#[test] fn chd_sha1_and_rawsha1_from_header() {}
#[test] fn soft_patch_splits_identical_roms() { /* Game.sfc+Game.ips vs Game (1).sfc → different; both with equal .ips → same */ }
#[test] fn names_never_group() { /* Tetris 2.zip (SFC bytes) vs Tetris 2 (1).zip (GB bytes) → different; same bytes, "(1)" → same */ }
#[test] fn singleton_size_buckets_are_never_read() { /* counting reader: 0 reads */ }
#[test] fn bucket_key_is_payload_size_not_file_size() { /* 512+N smc vs N sfc meet in one bucket */ }
#[test] fn quick_ids_v2_rows_are_reused_without_reading() {}
#[test] fn colliding_bucket_ignores_max_hash_mb() {}
```
- [ ] **Step 2:** `cargo test -p coverly-core --test dedupe_keys` fails.
- [ ] **Step 3:** Implement the spec §5.2 table and the work order of §10: payload-size buckets → zip CRC (free) → 128 KiB prefix CRC (`sniff::PREFIX`) → full SHA1. Work runs on `std::thread::scope` workers (4, or 2 on removable/network volumes, using the organise helper). Write every computed key to `content_keys` (Task 3).
- [ ] **Step 4:** Tests and gates.
- [ ] **Step 5:** `git commit -m "feat(core): exact-copy keys across containers, headers and byte orders"`

### Task 10: Core: level 2 and level 3 keys

**Overlap:** none (level 3 runs against the `DiscProbe` trait; the real probe comes in Task 25).

**Files:**
- Modify: `crates/coverly-core/src/dedupe/keys.rs`
- Test: `crates/coverly-core/tests/dedupe_keys.rs` (append)

**Interfaces:**
```rust
pub struct VersionKey { pub system: String, pub title_key: String, pub disc: Option<u8>, pub tags: NameTags }
pub struct FormatKey { pub system: String, pub serials: Vec<String>, pub disc_no: Option<u8>, pub volume: Option<VolumeFingerprint> }
pub fn level_two(keyed: &[KeyedUnit], dats: &dyn DatLookup) -> Vec<Option<VersionKey>>;   // only DAT hits by hash
pub fn level_three(units: &[Unit], probe: &dyn DiscProbe, dats: &dyn DatLookup) -> Vec<Option<FormatKey>>;
```

- [ ] **Step 1: Failing tests:** `tetris_v10_and_v11_share_a_version_key` (mock DAT rows `Tetris (Japan) (En)` CRC `63F9407D`, `Tetris (World) (Rev 1)` `46DF91AD`), `unknown_dump_never_gets_a_version_key`, `disc_1_and_2_never_share`, `translated_and_original_never_share`, `pbp_and_bin_same_serial_and_fingerprint_share_a_format_key`, `different_serials_never_group` (`SLUS-01161` vs `SLUS-01318`), `same_serial_other_build_is_report_only`, `modification_tag_vetoes_level_three`, `multi_disc_lists_must_match_and_a_lone_disc_is_partial`, `saturn_without_disc_no_takes_no_part_unless_dat_says_single_disc`, `serial_mapped_to_two_titles_is_serial_conflict`.
- [ ] **Step 2:** `cargo test -p coverly-core --test dedupe_keys` fails.
- [ ] **Step 3:** Implement as in spec §5.3–§5.4. `DatLookup` is a small trait over `Cache::dat_by_crc_any` / `dat_by_serial`, with a fake in `testing.rs`.
- [ ] **Step 4:** Tests and gates.
- [ ] **Step 5:** `git commit -m "feat(core): catalogue-version and disc-serial keys for duplicates"`

### Task 11: Core: grouping, weakest link, report-only reasons

**Overlap:** none.

**Files:**
- Create: `crates/coverly-core/src/dedupe/group.rs`
- Test: `crates/coverly-core/tests/dedupe_group.rs`

**Interfaces:** `pub fn build_groups(units: &[Unit], l1: &[KeyedUnit], l2: &[Option<VersionKey>], l3: &[Option<FormatKey>], opts: &GroupOpts) -> Vec<RawGroup>` with `RawGroup { system, members: Vec<(usize, Relation)>, action: GroupAction }` (keeper chosen in Task 13; relation recomputed to the keeper there).

- [ ] **Step 1: Failing tests:**
```rust
#[test] fn no_group_spans_two_systems() { /* GB vs GBC identical; FBNEO vs MAME identical; xorshift over 200 random system pairs */ }
#[test] fn crash_bash_three_members_two_relations() { /* bin = bin 1 (exact), PBP same serial → one group of 3 */ }
#[test] fn level_two_joins_only_when_enabled() {}
#[test] fn unconfirmed_zip_group_is_report_only() {}
#[test] fn header_differs_group_is_report_only_unless_one_matches_the_header_db() {}
#[test] fn two_valid_arcade_set_names_are_not_duplicates() { /* ffight.zip vs ffightu.zip with mock arcade DAT */ }
#[test] fn arcade_copy_of_a_valid_set_groups_with_a_forced_keeper() { /* ffight.zip vs ffight (1).zip */ }
#[test] fn arcade_without_dat_is_report_only() {}
#[test] fn chd_same_raw_different_sha1_is_report_only() {}
#[test] fn huge_group_is_collapsed_and_report_only() { /* 60 members → BigGroup */ }
#[test] fn singletons_are_dropped() {}
```
- [ ] **Step 2:** `cargo test -p coverly-core --test dedupe_group` fails.
- [ ] **Step 3:** Implement a union-find per system (the system is the first component of every key, asserted with `debug_assert!` and a hard runtime check that drops any cross-system edge). Order: copy clusters, then level 3 joins, then level 2 joins. A group's action is the strictest of its links (any report-only link makes the group report-only).
- [ ] **Step 4:** Tests and gates.
- [ ] **Step 5:** `git commit -m "feat(core): group duplicates per system with report-only guards"`

### Task 12: Core: saves, blank detection, conflicts

**Overlap:** none.

**Files:**
- Create: `crates/coverly-core/src/dedupe/saves.rs`
- Test: `crates/coverly-core/tests/dedupe_saves.rs`

**Interfaces:**
```rust
pub struct SaveInfo { pub path: PathBuf, pub size: u64, pub mtime_ns: i64, pub sha1: String, pub keyed_by: SaveKey /* Stem | Serial | Set */ }
pub fn saves_for(unit: &Unit, profile_id: &str, root: &Path, serials: &[String]) -> Vec<SaveInfo>; // same folder, profile save dirs, saves/<core>/, states/<core>/
pub fn is_blank(bytes_reader: impl Read, len: u64, ext: &str) -> io::Result<bool>;   // all 0x00 / all 0xFF / PS1 "MC" card with no used frames
pub fn conflict(members: &[(usize, Vec<SaveInfo>)]) -> Option<Vec<SaveInfo>>;        // two+ members with real saves of different sha1
pub const SAVE_LOCATIONS: &[(&str /*profile*/, OffCard)];                             // DD-5
```

- [ ] **Step 1: Failing tests:** `all_zero_and_all_ff_are_blank`, `blank_ps_card_is_no_save` (synthetic 128 KiB `MC` card, 193 non-zero bytes, empty directory frames), `different_real_saves_make_a_conflict`, `equal_real_saves_are_not_a_conflict`, `retroarch_sort_by_core_saves_are_found` (`saves/Snes9x/<stem>.srm`, `states/Snes9x/<stem>.state1`), `duckstation_memcard_by_serial_is_shared`, `muos_reports_off_card_saves`.
- [ ] **Step 2:** `cargo test -p coverly-core --test dedupe_saves` fails.
- [ ] **Step 3:** Implement. Save extensions are spec §8.2 plus `organize.companion_exts` entries that begin with `state` or are in the save list.
- [ ] **Step 4:** Tests and gates.
- [ ] **Step 5:** `git commit -m "feat(core): detect real saves and save conflicts between copies"`

### Task 13: Core: keeper rules and `KeepReason`

**Overlap:** none.

**Files:**
- Create: `crates/coverly-core/src/dedupe/keeper.rs`
- Test: `crates/coverly-core/tests/dedupe_keeper.rs`

**Interfaces:**
```rust
pub enum KeepReason { UserChoice, HasSave { path: PathBuf }, Played { playcount: u32 }, InLibrary,
    Verified { kept: DumpQuality, other: DumpQuality }, Retail { other: ReleaseClass },
    Region { kept: Region, other: Region }, Revision { kept: String, other: String },
    Container { kept: String, other: String, profile: String }, HasCover, CatalogueName { name: String }, Tie }
pub struct Candidate<'a> { pub member: usize, pub unit: &'a Unit, pub saves: &'a [SaveInfo], pub played: Option<Played>,
                           pub tags: Option<&'a NameTags>, pub dat_name: Option<&'a str>, pub has_cover: bool,
                           pub relation: Relation, pub headered_functional: Option<bool> }
pub fn choose(cands: &[Candidate], rules: &[RuleToggle], regions: &[Region], order: &ContainerOrder,
              user: Option<usize>, forced: Option<usize>) -> (usize, KeepReason, Option<(usize, KeepReason)>, bool /*tie*/);
pub fn played_from_gamelist(system_dir: &Path, rel: &Path) -> Option<Played>; // <playcount>, <lastplayed>, <favorite>
```

- [ ] **Step 1: Failing tests:** one per rule, with the expected `KeepReason` values, plus:
```rust
#[test] fn tetris_v11_stays_over_v10_with_1g1r() { /* Region World vs Japan decides; revision agrees */ }
#[test] fn headerless_snes_wins_verified_but_headerless_nes_never_does() {}
#[test] fn pbp_wins_container_on_anbernic_and_cue_on_es_de() {}
#[test] fn arcade_container_order_is_zip_only() {}
#[test] fn cover_rule_decides_when_only_one_has_art() {}
#[test] fn catalogue_name_only_among_proven_copies() { /* Mega Man - Dr. Wily's Revenge (USA).gb wins the three-way */ }
#[test] fn tie_is_oldest_then_path_and_flagged() {}
#[test] fn rule_order_changes_the_winner_and_disabled_rule_is_skipped() {}
#[test] fn user_choice_and_forced_keeper_win() {}
#[test] fn remembered_pick_drops_when_the_other_copy_gains_a_save() {}
#[test] fn keeper_is_never_the_shortest_name_by_default() { /* Tetris 2.zip vs Tetris 2 (1).zip identical, older mtime on "(1)" → "(1)" stays with Tie */ }
```
- [ ] **Step 2:** `cargo test -p coverly-core --test dedupe_keeper` fails.
- [ ] **Step 3:** Implement a pairwise comparator in rule order. `user` is always first and `tie` always last. `runner_up` is the best non-keeper together with the rule that separated it from the keeper. Region order comes from `settings.matching.regions` (`matching::parse_region_order`), skipping the `rom` entry.
- [ ] **Step 4:** Tests and gates.
- [ ] **Step 5:** `git commit -m "feat(core): choose the copy to keep with ordered rules and a typed reason"`

### Task 14: Core: companions, art, shared stems, gamelist entries

**Overlap:** none (uses `art::target_path`, `gamelist`, read-only).

**Files:**
- Create: `crates/coverly-core/src/dedupe/companions.rs`
- Test: `crates/coverly-core/tests/dedupe_companions.rs`

**Interfaces:** `pub enum TravelKind { Save, State, Cheat, Patch, Text, Art(Media), AppleDouble }`, `pub struct Travel { pub from: PathBuf, pub kind: TravelKind }`, `pub struct SharedStay { pub path: PathBuf, pub with: usize }`, `pub fn companions_for(group: &RawGroup, keeper: usize, units: &[Unit], ctx: &CompanionCtx) -> Vec<(Vec<Travel>, Vec<SharedStay>, Option<GamelistEntry>, ArtLeaves)>`.

- [ ] **Step 1: Failing tests:** `shared_stem_save_stays_with_the_keeper` (Crash Bash shape: PBP kept, `Crash Bash.srm` stays), `extra_with_its_own_stem_carries_save_and_art` (`GB/Imgs/Tetris (JUE) (V1.0) [!].png`), `art_leaving_with_the_extra_is_flagged_when_keeper_has_none`, `apple_double_twins_travel`, `gamelist_entry_found_for_the_extra`, `companion_case_insensitive_every_dot`.
- [ ] **Step 2:** `cargo test -p coverly-core --test dedupe_companions` fails.
- [ ] **Step 3:** Implement with `organize::target::match_companions` (make it `pub(crate)` if needed; it lives in `target.rs`, so **do not edit it**: if it is private, copy the 20-line matcher into `companions.rs` and add a `// TODO(merge): use organize::target::match_companions` note, removed in Task 26).
- [ ] **Step 4:** Tests and gates.
- [ ] **Step 5:** `git commit -m "feat(core): saves, art and game-list entries that travel with a set-aside copy"`

### Task 15: Core: `plan_dedupe`, progress, cache, remembered choices, quarantine path

**Overlap:** none.

**Files:**
- Create: `crates/coverly-core/src/dedupe/plan.rs`
- Test: `crates/coverly-core/tests/dedupe_plan.rs`, snapshots in `crates/coverly-core/tests/snapshots/`

**Interfaces:**
```rust
pub enum DedupePhase { Listing, Sizes, Fingerprints, Serials, Catalogues }
pub struct DedupeProgress { pub phase: DedupePhase, pub done: usize, pub total: usize, pub current: Option<PathBuf> }
pub struct DedupeEnv<'a> { pub cache: &'a Cache, pub db: &'a SystemDb, pub profile: &'a Profile, pub settings: &'a Settings,
    pub listing: &'a dyn EntryListing, pub discs: &'a dyn DiscProbe, pub dats: Option<&'a DatStore<'a>>,
    pub progress: &'a dyn Fn(DedupeProgress), pub cancel: &'a AtomicBool }
pub fn plan_dedupe(scan: &LibraryScan, opts: &DedupeOptions, env: &DedupeEnv) -> Result<DedupePlan, DedupeError>;
pub fn default_quarantine(root: &Path, roms_root: &str, name: &str) -> QuarantineSpot; // spec §7.1 table and fallback
pub fn regroup(plan: &DedupePlan, opts: &DedupeOptions) -> DedupePlan;                 // option change without re-hash (from cached keys)
pub fn set_keeper(plan: &mut DedupePlan, group: u64, member: usize, cache: &Cache) -> Result<(), DedupeError>;
pub fn skip_group(plan: &mut DedupePlan, group: u64, skip: bool);
```

- [ ] **Step 1: Failing tests:**
```rust
#[test] fn library_plan_snapshot() { /* synthetic 6-system library covering every relation → insta JSON */ }
#[test] fn dry_plan_writes_nothing() { /* tree hash before/after */ }
#[test] fn quarantine_next_to_roms_folder_and_fallbacks() { /* /Volumes/SD + ROMS → /Volumes/SD/_Duplicados; roms_root "" → "<parent>/ROMS _Duplicados"; unwritable parent → <root>/_Duplicados */ }
#[test] fn destinations_mirror_rel_dir() {}
#[test] fn totals_say_bytes_leaving_roms() {}
#[test] fn conflict_groups_are_not_counted() {}
#[test] fn offline_skips_level_two_and_lists_systems_without_catalogue() {}
#[test] fn warm_run_hashes_nothing() { /* second plan: counting reader = 0 */ }
#[test] fn cancel_between_units_keeps_the_cache() {}
#[test] fn set_keeper_persists_and_survives_a_rename() {}
#[test] fn regroup_with_1g1r_adds_version_groups_without_reading() {}
#[test] #[ignore] fn ten_thousand_files_hash_only_colliders() { /* 2 % duplicates; hash_rom calls ≤ colliders; < 10 s */ }
```
- [ ] **Step 2:** `cargo test -p coverly-core --test dedupe_plan` fails.
- [ ] **Step 3:** Implement as the pipeline units → level 1 → (serials) → (DATs, downloads capped by `max_dat_downloads`) → groups → saves → keeper → companions → destinations → totals. The group id is a stable hash of the sorted member content keys. Progress is throttled by the caller.
- [ ] **Step 4:** Tests and gates.
- [ ] **Step 5:** `git commit -m "feat(core): plan duplicates for a library as a dry run"`

### Task 16: Core: real-sample plan test (read-only)

**Overlap:** none.

**Files:**
- Create: `crates/coverly-core/tests/dedupe_real_sample.rs`

- [ ] **Step 1: Write the test** as `#[ignore]`, gated by `COVERLY_LIBRARY_DIR` (`~/Documents/ROMS`) and `COVERLY_SAMPLE_DIR` (`~/Documents/ROMs-Downloaded`). Build the plan with `NoDiscProbe` (Phase A). Assert the spec §15.1 **found** totals per system (88 groups, 89 extras, 1 529 567 750 bytes found), zero cross-system groups, the §15.2 "never offered" pairs, `PS/Grandia.bios` excluded, and `Tetris (JUE) (V1.0)` vs `(V1.1)` as a report-only level 2 group with V1.1 as keeper. Print the **acting** totals and every FC `HeaderDiffers` group with its two headers, as evidence. Before and after, take a tree hash (sizes + mtimes) of both folders and assert it is unchanged.
- [ ] **Step 2: Run:** `COVERLY_LIBRARY_DIR=/Users/miguelangelesparzacalero/Documents/ROMS COVERLY_SAMPLE_DIR=/Users/miguelangelesparzacalero/Documents/ROMs-Downloaded cargo test -p coverly-core --test dedupe_real_sample -- --ignored --nocapture`. Fix the planner (never the expected numbers) until it passes. A difference explained by the spec's re-measure note (FC headers) is recorded in the test as the measured value, with a comment.
- [ ] **Step 3:** Gates.
- [ ] **Step 4:** `git commit -m "test(core): real-library duplicates dry run (ignored, read-only)"`

## Group A3: CLI dry run (spec D8, part 1)

### Task 17: CLI: `coverly dedupe` dry run

**Overlap:** none (`main.rs`, new `dedupe.rs`; the archive branch edits only `organize.rs`).

**Files:**
- Create: `crates/coverly-cli/src/dedupe.rs`, `crates/coverly-cli/tests/cli_dedupe.rs`
- Modify: `crates/coverly-cli/src/main.rs` (`Command::Dedupe(Box<dedupe::DedupeArgs>)`)

**Args:** `<ROOT> [--profile ID] [--system GB,SFC…] [--level exact,format,version] [--one-game-one-rom] [--keep-rules has_save,verified,…] [--keep PATH]… [--reset-choices] [--offline] [--report table|json] [--output FILE] [--data-dir DIR] [--cache-dir DIR]`. `--apply`, `--quarantine`, `--trash` and `--resume` are parsed but return exit 2 with "available after the archive work lands" until Task 27. Do **not** ship this stub in a release: Task 27 removes it.

- [ ] **Step 1: Failing tests** (`assert_cmd`, synthetic tempdir library): `dry_run_writes_nothing` (also with `safety.dry_run_default = false`), `table_lists_systems_groups_and_destinations`, `json_matches_the_plan_schema` (`version: 1`), `keep_path_overrides_the_keeper`, `keep_rules_reorder_changes_the_winner`, `conflicts_exit_zero`, `unknown_rule_is_exit_2`, `missing_root_is_exit_3`, `hostile_file_names_are_terminal_safe`.
- [ ] **Step 2:** `cargo test -p coverly-cli --test cli_dedupe` fails.
- [ ] **Step 3:** Implement the table format from spec §14 through `table::terminal_safe`. The JSON is `serde_json::to_writer_pretty(&plan)`.
- [ ] **Step 4:** Tests and gates.
- [ ] **Step 5:** `git commit -m "feat(cli): dry-run duplicates report"`

## Group D: UI components on fixtures (frontend-design; spec D10, part 1)

Before Task 18, load frontend-design:frontend-design and keep the "UI design brief" above open. Each component task ends with a screenshot critique on a Duplicates section added to the dev-only gallery `src/dev/UiGallery.tsx`. Check light and dark, 720px and 1280px. Note what changed after each critique in the commit body.

### Task 18: Frontend: wire types and `lib/dedupe.ts`

**Overlap:** none (new files).

**Files:**
- Create: `src/ipc/dedupeTypes.ts`, `src/lib/dedupe.ts`, `src/lib/dedupe.test.ts`, `src/ipc/mock/dedupeFixture.ts`

**Interfaces:** TS mirrors of the Task 6 types (`DedupePlanView`, `DupGroupView`, `MemberView`, `KeepReasonView` discriminated on `kind`, `RelationView`, `ReportWhyView`). The view adds `plan_id`. In `lib/dedupe.ts`: `reasonText(t, reason, ctx): string`, `relationMeta(r): { icon: LucideIcon; key: string }`, `systemSummary(groups)`, `deltaFor(plan, nextOpts): number`, `formatBytes` (from `lib/format.ts`), `sortGroups` (conflicts first, then by system, then by title with `localeCompare`).

- [ ] **Step 1: Failing tests:** `every_keep_reason_has_text_in_both_locales`, `reason_text_names_what_the_other_had` (region: "Se queda: región preferida (Mundo); la otra es Japón."), `relation_meta_uses_lucide_icons` (R3), `delta_for_enabling_versions_is_37_on_fixture`, `conflicts_sort_first`.
- [ ] **Step 2:** `pnpm vitest run src/lib/dedupe.test.ts` fails.
- [ ] **Step 3:** Implement. The fixture reproduces the spec's SMS Ecco, PS Crash Bash, GB Tetris (level 2) and one conflict group.
- [ ] **Step 4:** All gates.
- [ ] **Step 5:** `git commit -m "feat(ui): duplicates view types and reason texts"`

### Task 19: Frontend: `DupGroup`, `KeeperRow`, `TrayRow` (the shelf and the tray)

**Overlap:** light (`locales/{es,en}/ui.json`; insert the `dedupe` block alphabetically).

**Files:**
- Create: `src/components/dedupe/{DupGroup,KeeperRow,TrayRow}.tsx`, `src/components/dedupe/dedupe.css`, `src/components/dedupe/DupGroup.test.tsx`
- Modify: `locales/en/ui.json`, `locales/es/ui.json` (all `dedupe.*` keys of spec §11.5, with revisions R1–R2: `dedupe.system.summary` = "{name}: {groups} juegos, {extras} copias" / "{name}: {groups} games, {extras} copies")

**Design:** the shelf row uses `border-left: 3px solid var(--cr-primary)` over a 1px `--cr-border` top rule. That is the brand shelf rule: draw it with a pseudo-element so it can slide (`transform: translateY`) between rows. The tray is `background: var(--cr-bg)`, `margin-left: 24px` (12px under 900px), with `--cr-radius-sm`, no shadow. Sizes go in a fixed `9ch` right-aligned mono column. The destination line is mono `type.app.mono`, prefixed "→ ". Travels read "+ 1 portada, + 1 partida". The why line is `type.app.caption`, `--cr-text-muted`, and serves as `aria-describedby` of the radio.

- [ ] **Step 1: Failing tests:**
```tsx
it('renders one radiogroup labelled by the game title with exactly one checked radio')
it('tray_radio_has_a_visible_keep_label') // "Quedarme con esta" text, not an empty radio
it('changing_keeper_announces_and_offers_undo') // live region "Ahora se queda …; … pasa a apartarse." + Deshacer for 10 s (fake timers)
it('arrow keys move within the group, Tab leaves it, X toggles "No tocar este juego"')
it('level 2 without 1G1R shows "Se queda también (solo informe)" and no radios')
it('shows confidence only in the badge tooltip; below 1.00 shows "Sin confirmar"') // R1
it('save that leaves with a picked keeper warns inline') // dedupe.saveLeaves
it('art leaving shows the cover warning') // dedupe.artLeaves
it('reduced motion: no transition on the shelf cap')
it('hostile file names render as text')
```
- [ ] **Step 2:** `pnpm vitest run src/components/dedupe` fails.
- [ ] **Step 3:** Implement with `RadioGroup`, `Badge` (`variant="outline"` for relations, `primary` for "Se queda"), `Checkbox` and `Tooltip`. Screenshot critique on the gallery route, then apply "remove one accessory": if the tray needs both a border and a background to read, keep only the background.
- [ ] **Step 4:** All gates (`src/i18n/parity.test.ts` included).
- [ ] **Step 5:** `git commit -m "feat(ui): duplicate group as a shelf with a tray of copies to set aside"`

### Task 20: Frontend: `SystemSection`, `ConflictGroup`, `DedupeOptions`, `UnchangedList`, `DedupeResult`

**Overlap:** light (locales).

**Files:**
- Create: `src/components/dedupe/{SystemSection,ConflictGroup,DedupeOptions,UnchangedList,DedupeResult}.tsx` with tests

- [ ] **Step 1: Failing tests:** `system_section_trigger_is_a_sentence_with_bytes_in_the_size_column` (R2), `systems_with_conflicts_open_by_default`, `closed_system_mounts_no_rows`, `system_section_windows_over_200_groups` (DOM holds visible + margin only; keyboard focus follows data), `conflict_group_has_no_radio_and_lists_saves_with_open_folder`, `option_change_shows_the_delta` ("+37 archivos" next to the control, live announcement), `trash_option_shows_its_caveats`, `unchanged_list_shows_reasons`, `result_same_volume_says_the_card_is_as_full`, `result_partial_turns_warning_with_reasons`, `result_offers_find_covers_when_art_left`.
- [ ] **Step 2:** `pnpm vitest run src/components/dedupe` fails.
- [ ] **Step 3:** Implement with `Collapsible`, `ToggleGroup` (multi), `RadioGroup`, `Checkbox`, `Alert`, `Kbd`, `DropdownMenu` (per system: "No tocar {system}", "Volver a aplicar las reglas en {system}"). Windowing uses a small in-house window over measured row heights (no new library) and follows the existing `CoverGrid` virtualisation pattern. Screenshot critique in both themes and both widths.
- [ ] **Step 4:** All gates.
- [ ] **Step 5:** `git commit -m "feat(ui): duplicates options, system sections, conflicts and result"`

---

## Phase B — BLOCKED until `feat/organize-disc-archives` is merged

Start: `git fetch && git rebase main` (no push). Resolve conflicts in `locales/*` by keeping both blocks. Run all gates. Re-run `dedupe_real_sample` (Task 16) to confirm nothing moved under the rebase.

## Group B1: Journal, Trash, apply, undo (spec D6)

### Task 21: Core: `JobKind::Dedupe`, `Op::Trashed`, header, pruning, undo report

**Overlap:** BLOCKED (`write/mod.rs`, `write/undo_organize.rs`).

**Files:**
- Modify: `crates/coverly-core/src/write/mod.rs` (`JobKind::Dedupe`; `Op::Trashed { from: PathBuf }`; `Header.quarantine: Option<PathBuf>` with `#[serde(default)]`; `header_kind` reads `"dedupe"`; `prune_jobs_at` keeps dedupe jobs `KEEP_ORGANIZE_DAYS`; `undo_last_job` skips dedupe)
- Modify: `crates/coverly-core/src/write/undo_organize.rs` (`Trashed` → `UndoReport.manual_restore: Vec<PathBuf>`)
- Modify: `crates/coverly-cli/src/undo.rs` (`kind_name`, trashed lines)
- Test: `crates/coverly-core/tests/organize_undo.rs`, `crates/coverly-core/tests/writer.rs` (append)

- [ ] **Step 1: Failing tests:** `header_kind_reads_dedupe`, `dedupe_jobs_survive_thirty_fill_jobs`, `fill_undo_skips_dedupe_jobs`, `trashed_op_round_trips_and_is_reported_for_manual_restore`, `older_build_reads_trashed_as_unknown_not_undoable` (deserialise into a copy of the old enum), `extract_ops_from_archive_branch_still_undo` (regression after the merge).
- [ ] **Step 2:** `cargo test -p coverly-core --test organize_undo --test writer` fails.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Tests and gates.
- [ ] **Step 5:** `git commit -m "feat(core): journal duplicates jobs and trashed files"`

### Task 22: Core: `trash` dependency behind `TrashBackend`

**Overlap:** BLOCKED (`crates/coverly-core/Cargo.toml`, `Cargo.lock`, MSRV).

**Files:**
- Modify: `crates/coverly-core/Cargo.toml` (`trash = "5"`, default features), `Cargo.lock`, `THIRD-PARTY-NOTICES.md`
- Create: `crates/coverly-core/src/dedupe/trash.rs`
- Test: unit tests with `FakeTrash`

**Interfaces:** `pub trait TrashBackend: Send + Sync { fn trash(&self, path: &Path) -> Result<(), TrashError>; fn supports_put_back(&self) -> bool; }`, `pub struct OsTrash;` (macOS `DeleteMethod::Finder`), `pub struct FakeTrash` (testing, moves into a temp "trash" dir and records calls).

- [ ] **Step 1: Failing tests:** `fake_trash_records_calls`, `volume_without_trash_is_an_error_not_a_delete` (FakeTrash configured to fail → file still present).
- [ ] **Step 2:** `cargo fetch` (one time, network) then `cargo test -p coverly-core dedupe::trash` fails.
- [ ] **Step 3:** Implement. Confirm `cargo build --offline` afterwards.
- [ ] **Step 4:** Tests and gates, plus `cargo deny check licenses` if configured (`Makefile` target), else note the MIT licence in `THIRD-PARTY-NOTICES.md`.
- [ ] **Step 5:** Two commits: `git commit -m "build(deps): add trash for the optional send-to-Trash action"` (Cargo files + notices) then `git commit -m "feat(core): trash backend for set-aside copies"`.

### Task 23: Core: `dedupe::apply`, quarantine moves, preflight, pause and resume

**Overlap:** BLOCKED (reuses `organize/apply.rs` internals that the archive branch rewrites: `revalidate`, `apply_set`, the pause/resume path).

**Files:**
- Create: `crates/coverly-core/src/dedupe/apply.rs`
- Modify: `crates/coverly-core/src/organize/apply.rs` (extract the shared run loop into `pub(crate) struct Run` if the merged code has not already; no behaviour change, organise tests stay green)
- Test: `crates/coverly-core/tests/dedupe_apply.rs`

**Interfaces:**
```rust
pub enum DedupeItemStatus { Quarantined, Trashed, Skipped { reason: DedupeSkip }, Failed { kind: ItemErrorKind, detail: String } }
pub enum DedupeSkip { KeeperChanged, ChangedSincePlan, SourceMissing, DestinationLimit { why: DestLimit }, Excluded, Conflict, Cancelled }
pub enum DestLimit { Fat32Size, BadName, PathTooLong, CaseCollision }
pub struct DedupeReport { pub job: String, pub outcomes: Vec<(u64 /*group*/, usize /*member*/, DedupeItemStatus)>, pub bytes_out: u64, pub art_left: Vec<PathBuf> }
pub struct DedupeApplyEnv<'a> { pub data_dir: &'a Path, pub trash: &'a dyn TrashBackend, pub cache: &'a Cache,
                                pub progress: &'a dyn Fn(usize, usize), pub cancel: &'a AtomicBool }
pub fn apply_dedupe(plan: &DedupePlan, decisions: &DedupeDecisions, env: &DedupeApplyEnv) -> Result<DedupeReport, ApplyError>;
pub fn resume_dedupe(job: &str, env: &DedupeApplyEnv) -> Result<DedupeReport, ApplyError>;
```

- [ ] **Step 1: Failing tests** (tempdirs):
```rust
#[test] fn quarantine_path_mirrors_rel_dir_and_art_follows() {}
#[test] fn gamelist_entry_removed_with_backup() {}
#[test] fn collision_in_quarantine_gets_numbered() {}
#[test] fn keeper_changed_skips_the_group() {}
#[test] fn changed_extra_is_left_alone() {}
#[test] fn set_moves_all_or_nothing() {}
#[test] fn conflict_and_report_only_groups_never_move() {}
#[test] fn never_moves_every_member() { /* decisions trying to skip the keeper → error before any move */ }
#[test] fn cross_volume_copies_verify_then_delete() { /* injected EXDEV */ }
#[test] fn fat32_destination_skips_before_any_copy() { /* injected fs probe: 4.2 GiB → DestinationLimit */ }
#[test] fn bad_name_and_case_collision_on_exfat() {}
#[test] fn disk_full_pauses_and_resume_finishes() {}
#[test] fn writes_the_job_list_file_in_the_quarantine() { /* <quarantine>/<job>.txt journaled Created */ }
#[test] fn trash_action_journals_before_calling_the_backend() {}
#[test] fn content_cache_rows_are_rekeyed_to_the_quarantine() {}
#[test] fn quarantine_inside_roms_or_a_system_folder_is_refused() {}
```
- [ ] **Step 2:** `cargo test -p coverly-core --test dedupe_apply` fails.
- [ ] **Step 3:** Implement per spec §7–§9. The writer root holds both the ROMs folder and the quarantine (spec §7.1 "option B"). Do the cross-volume preflight once per destination before the first move. Persist the plan as `jobs/<job>.plan.json`.
- [ ] **Step 4:** Tests and gates (all organise apply tests green).
- [ ] **Step 5:** `git commit -m "feat(core): set aside duplicate copies with a journal, preflight and resume"`

### Task 24: Core: undo of dedupe jobs

**Overlap:** BLOCKED (`write/undo_organize.rs`).

**Files:**
- Test: `crates/coverly-core/tests/dedupe_undo.rs`
- Modify (only if a test needs it): `crates/coverly-core/src/write/undo_organize.rs`

- [ ] **Step 1: Failing tests:** `undo_restores_rom_companions_art_and_gamelist`, `undo_refuses_with_a_later_job_unless_include_later`, `undo_keeps_a_file_that_took_the_original_place` (`skipped_occupied`), `undo_lists_trashed_files_for_manual_restore`, `undo_after_crash_mid_copy_removes_the_partial_copy`.
- [ ] **Step 2:** `cargo test -p coverly-core --test dedupe_undo` fails or passes. A test that passes on the first run must be shown to fail by temporarily breaking the code path it guards. Note this in the commit body.
- [ ] **Step 3:** Implement the gaps.
- [ ] **Step 4:** Tests and gates.
- [ ] **Step 5:** `git commit -m "test(core): undo puts set-aside copies, saves, art and game lists back"`

## Group B2: Real archive and disc readers (spec D4 completion)

### Task 25: Core: `IsoDiscProbe` and `ArchiveEntryListing`

**Overlap:** BLOCKED (builds on `identify/sniff.rs` disc rows and `organize/archive/*` from the archive branch; edits neither unless a helper must become `pub(crate)`).

**Files:**
- Create: `crates/coverly-core/src/dedupe/disc.rs`, `crates/coverly-core/src/dedupe/archive.rs`
- Modify (visibility only): `crates/coverly-core/src/identify/sniff.rs`, `crates/coverly-core/src/organize/archive/mod.rs`
- Test: `crates/coverly-core/tests/dedupe_disc.rs`

**What `IsoDiscProbe` reads (bounded, a few sectors per unit, cached in `content_keys.serials_json`/`volume_fp`):** for raw `.bin` (2352) / `.img` / `.iso` (2048): PVD at sector 16 (space size, creation/modification timestamps), root directory → `SYSTEM.CNF` (PS1 `BOOT`, PS2 `BOOT2`) → serial + SHA1 of `SYSTEM.CNF`. Sega: `IP.BIN` product number at 0x20, device field for `disc_no`, SHA1 of the first 0x100 bytes. PSP ISO/CSO: `UMD_DATA.BIN` + `PSP_GAME/PARAM.SFO` `DISC_ID`. PBP (DD-3): the `PSISO` index, inflating only the needed blocks. `.m3u`: per-disc probes in order. CHD (DD-2): `None` unless a later reader exists.

- [ ] **Step 1: Failing tests** (synthetic images built in `testing.rs`): `ps1_bin_serial_and_fingerprint`, `iso_2048_and_bin_2352_of_the_same_disc_share_fingerprint`, `pbp_reads_inner_system_cnf_not_outer_param_sfo`, `saturn_disc_no_from_device_field`, `psp_disc_id`, `m3u_lists_serials_in_order`, `chd_returns_none`, `reads_are_bounded` (counting reader ≤ 64 KiB raw + 1 MiB inflated), `archive_listing_lists_7z_and_tar_crcs` (via the archive branch fixtures), `unreadable_archive_is_reported_not_fatal`.
- [ ] **Step 2:** `cargo test -p coverly-core --test dedupe_disc` fails.
- [ ] **Step 3:** Implement and plug the probes into `DedupeEnv` construction in the CLI (Task 27) and Tauri (Task 28). Extend `dedupe_real_sample` with the spec §15.3 PS groups (Crash Bash, Gran Turismo, Tekken 3 / Iron fist 3), recording whether the Tekken pair is `BuildDiffers`.
- [ ] **Step 4:** Tests, gates and the ignored real-sample test.
- [ ] **Step 5:** `git commit -m "feat(core): read disc serials and volume fingerprints for format duplicates"`

## Group B3: Organise integration (spec D7)

### Task 26: Core: duplicates as an Organise step

**Overlap:** BLOCKED (`organize/plan.rs`, `organize/target.rs`, `organize/walk.rs`, `organize/apply.rs`, `organize/inspect.rs`, `crates/coverly-cli/src/organize.rs`).

**Files:**
- Modify: `crates/coverly-core/src/organize/plan.rs` (`OrganizeAction::SetAside {…}`, `LeftReason::InLibrary { path, relation }`, `Counts.{set_aside, in_library}`; call `dedupe::group_units` instead of `mark_duplicates` at the former line ~1251)
- Modify: `crates/coverly-core/src/organize/target.rs` (remove `mark_duplicates`; make `match_companions` `pub(crate)` and drop the Task 14 copy)
- Modify: `crates/coverly-core/src/organize/walk.rs` (skip the pile-side and library-side quarantine)
- Modify: `crates/coverly-core/src/organize/apply.rs` (SetAside items move to `<pile root>/_Duplicados/` as an instant rename, same journal, outcome `Quarantined`/`Trashed`)
- Modify: `crates/coverly-core/src/scan/mod.rs` (quarantine folder skipped by library scans)
- Modify: `crates/coverly-cli/src/organize.rs` (`--duplicates set-aside|report|off`, column)
- Modify: `crates/coverly-core/tests/organize_target.rs` (drop the `mark_duplicates` test; it is replaced below)
- Test: `crates/coverly-core/tests/organize_plan.rs` (append), `crates/coverly-cli/tests/cli_organize.rs` (append)

- [ ] **Step 1: Failing tests:** `pile_item_equal_to_a_library_rom_under_another_name_is_in_library_and_set_aside`, `pile_pair_headered_and_headerless_is_one_group` (regression for `raw_crcs.first()`), `keeper_follows_rules_not_the_shortest_name`, `pile_extras_stay_on_the_pile_volume`, `report_mode_moves_nothing_extra`, `off_mode_matches_old_behaviour_minus_shortest_name`, `walk_and_scan_skip_quarantine`, `old_plan_json_with_duplicate_left_reason_still_loads`, `cli_organize_duplicates_flag`.
- [ ] **Step 2:** `cargo test -p coverly-core --test organize_plan && cargo test -p coverly-cli --test cli_organize` fails.
- [ ] **Step 3:** Implement. Pile units come from organise `Facts` (`units_from_facts`), and the archive branch's `Inspection` gives the inner CRCs of 7z/rar/tar. Library units come from a scan of only the target system folders that the plan touches.
- [ ] **Step 4:** Tests and gates (every organise test green).
- [ ] **Step 5:** `git commit -m "feat(core): organise sets aside copies already in the library instead of keeping the shortest name"`

## Group B4: CLI apply (spec D8, part 2)

### Task 27: CLI: `--apply`, `--quarantine`, `--trash`, `--resume`, exit codes, undo

**Overlap:** BLOCKED (depends on Tasks 21–25).

**Files:**
- Modify: `crates/coverly-cli/src/dedupe.rs` (remove the Task 17 stub), `crates/coverly-cli/src/undo.rs`
- Test: `crates/coverly-cli/tests/cli_dedupe.rs` (append)

- [ ] **Step 1: Failing tests:** `apply_creates_the_quarantine_and_a_job`, `quarantine_flag_on_another_dir`, `trash_uses_the_injected_backend` (env `COVERLY_TEST_TRASH_DIR` read only under `cfg(debug_assertions)` → `FakeTrash`), `disk_full_exits_5_and_resume_finishes`, `undo_restores_a_dedupe_job`, `apply_and_trash_together_is_exit_2`.
- [ ] **Step 2:** `cargo test -p coverly-cli --test cli_dedupe` fails.
- [ ] **Step 3:** Implement with the exit codes of spec §14.
- [ ] **Step 4:** Tests and gates.
- [ ] **Step 5:** `git commit -m "feat(cli): apply, resume and undo duplicates jobs"`

## Group B5: App (spec D9–D10)

### Task 28: Tauri: `DedupeManager`, IPC, events, contract

**Overlap:** BLOCKED (`src-tauri/src/ipc.rs`, `src-tauri/src/lib.rs`, `src-tauri/src/jobs/view.rs`).

**Files:**
- Create: `src-tauri/src/dedupe/mod.rs`, `src-tauri/src/dedupe/view.rs`, `src-tauri/tests/dedupe_state.rs`
- Modify: `src-tauri/src/ipc.rs`, `src-tauri/src/lib.rs`, `src-tauri/src/state.rs`, `src-tauri/src/error.rs` (`no-dedupe-plan`, `keeper-required`), `src-tauri/src/jobs/view.rs` (kind `dedupe`), `src-tauri/tests/contract.rs`, `src/ipc/contract.sample.json`, `src/ipc/contract.requests.json`

**Commands:** `dedupe_plan(root, opts) -> DedupePlanView` (emits `dedupe-progress`), `dedupe_cancel(plan_id)`, `dedupe_regroup(plan_id, opts)`, `dedupe_set_keeper(plan_id, group, member)`, `dedupe_skip_group(plan_id, group, skip)`, `dedupe_skip_system(plan_id, system, skip)`, `dedupe_reset_choices(root, systems?)`, `dedupe_apply(plan_id) -> DedupeJobView` (emits `dedupe-progress`, `dedupe-changed`), `dedupe_cancel_apply()`, `dedupe_resume(job)`, `dedupe_export_plan(plan_id) -> path?`, `reveal_path(path)` (reuse if present). Undo goes through `organize_undo`.

- [ ] **Step 1: Failing tests:** `plan_store_keeps_four_plans`, `apply_is_guarded_by_the_job_manager`, `set_keeper_persists_choice`, `regroup_does_not_rehash` (counting env), `progress_is_throttled`, `contract_samples_match_views`.
- [ ] **Step 2:** `cargo test -p coverly-roms --test dedupe_state --test contract` fails.
- [ ] **Step 3:** Implement on the `OrganizeManager` pattern (`KEPT_PLANS`, `Throttle`, `spawn_apply`).
- [ ] **Step 4:** All gates.
- [ ] **Step 5:** `git commit -m "feat(app): duplicates commands, progress events and job wiring"`

### Task 29: Frontend IPC: backend methods, Tauri adapter, mock backend

**Overlap:** BLOCKED (`src/ipc/types.ts`, `src/ipc/backend.ts`, `src/ipc/tauriBackend.ts`, `src/ipc/mock/mockBackend.ts`).

**Files:**
- Modify: `src/ipc/types.ts` (`export * from './dedupeTypes'`; `JobKind = 'fill' | 'organize' | 'dedupe'`), `src/ipc/backend.ts`, `src/ipc/tauriBackend.ts`, `src/ipc/mock/mockBackend.ts`
- Create: `src/ipc/mock/mockDedupe.ts`, `src/ipc/mock/mockDedupe.test.ts`
- Test: `src/ipc/contract.test.ts`, `src/ipc/tauriBackend.test.ts`

- [ ] **Step 1: Failing tests:** `mock_plan_emits_phases_in_order`, `mock_set_keeper_moves_the_old_keeper_to_the_tray`, `mock_apply_then_undo_restores_counts`, `tauri_adapter_uses_camelCase_args`, `contract_round_trip`.
- [ ] **Step 2:** `pnpm vitest run src/ipc` fails.
- [ ] **Step 3:** Implement. The mock is deterministic and uses `dedupeFixture.ts` (Task 18). Expose `window.mockDedupe*` hooks for E2E, like `mockOrganizeTouch`.
- [ ] **Step 4:** All gates.
- [ ] **Step 5:** `git commit -m "feat(ui): duplicates backend methods and mock"`

### Task 30: Frontend: `dedupeStore`, navigation, `DuplicatesScreen`

**Overlap:** BLOCKED for `src/stores/navStore.ts` and `src/App.tsx` only as a textual merge (the archive branch does not list them). Kept in Phase B because it needs Task 29.

**Files:**
- Create: `src/stores/dedupeStore.ts` (+ test), `src/screens/DuplicatesScreen.tsx` (+ test)
- Modify: `src/stores/navStore.ts` (`Screen = 'main' | 'settings' | 'organize' | 'duplicates'`), `src/App.tsx`, locales

**States:** `idle | planning | ready | applying | done | error`. Planning shows the phase sequence (Listando, Tamaños, Huellas, Números de serie, Catálogos) with checks, because it is a real sequence. Ready shows the summary sentence (Archivo 32 on the first clause), the options strip, the conflicts alert, the systems, "Sin cambios" and a sticky bar. Applying uses `<Progress slots={16}>` and Cancel. Done shows the result. The empty state is `EmptyState` ("Este estante está en orden."). Error says what failed, then "No se ha movido nada." and offers a retry.

- [ ] **Step 1: Failing tests:** `store_plan_set_keeper_skip_apply_undo` (on the mock), `cmd_r_replans`, `sticky_bar_counts_files_and_bytes_out_of_roms`, `primary_disabled_with_reason_when_zero`, `bulk_confirm_above_500_files` (and above 25 % of bytes, a confirmed big group, or the quarantine on another volume), `trash_confirm_names_count_bytes_and_manual_restore`, `same_volume_done_state_says_the_card_is_as_full`, `export_report_writes_json`, `off_card_saves_note_once_per_system`, `chd_unread_note`, `empty_state_lists_systems_without_catalogue`, `error_state_says_nothing_moved`.
- [ ] **Step 2:** `pnpm vitest run src/stores/dedupeStore.test.ts src/screens/DuplicatesScreen.test.tsx` fails.
- [ ] **Step 3:** Implement. Reuse the `OrganizeScreen` header and sticky-bar markup (copy the classes, do not import organise components). Do a screenshot critique on `pnpm dev` (mock) in light/dark at 720/1280 and record the changes in the commit body.
- [ ] **Step 4:** All gates.
- [ ] **Step 5:** `git commit -m "feat(ui): duplicates screen with planning, review, apply and result"`

### Task 31: Frontend: entry points: Toolbar, Organise section, Jobs panel, settings

**Overlap:** BLOCKED (`src/components/organize/{OptionsBar,OrganizeResult}.tsx`, `src/screens/OrganizeScreen.tsx`, `src/lib/organize.ts`).

**Files:**
- Modify: `src/components/library/Toolbar.tsx` (outline button "Buscar duplicados" / "Duplicados {count}", icon-only below 1180px, lucide `Copy`)
- Modify: `src/components/organize/OptionsBar.tsx` (picker Duplicados: Apartar · Solo informar · No buscar), `src/screens/OrganizeScreen.tsx` (summary "Duplicados: {n}", "Ya en tu biblioteca: {n}", section rendering `DupGroup` between `ReviewBuckets` and `LeftInPlace`, capped at 200 rows; primary "Organizar {n} archivos y apartar {m}"), `src/components/organize/OrganizeResult.tsx` (outcomes `quarantined`/`trashed`), `src/lib/organize.ts`
- Modify: `src/components/fill/JobsPanel.tsx` (label "Duplicados", organise Resume/Undo branches, fill-undo filter excludes dedupe)
- Create: `src/components/settings/DedupeSettings.tsx`. Modify `src/components/settings/LibrarySection.tsx` to mount it under the organise block: three level switches, "Una ROM por juego", action radio, quarantine path + "Elegir…", a reorderable rule list on the `MatchingSection` pattern with `ReorderAnnouncer`, the region rule reading "Usa el orden de Coincidencias" with a link, a collapsed container order editor, and exclusions.
- Modify: `src/lib/settingsRules.ts` (+ test): no duplicate rules, quarantine not inside `ROMS/` or a system folder
- Tests: the matching `*.test.tsx` files

- [ ] **Step 1: Failing tests:** `toolbar_shows_find_then_count`, `organize_options_picker_defaults_to_set_aside`, `organize_apply_label_counts_set_aside`, `organize_section_reuses_dup_group`, `jobs_panel_dedupe_undo_uses_organize_undo`, `settings_reorder_rules_announces`, `settings_rejects_quarantine_inside_roms`.
- [ ] **Step 2:** `pnpm vitest run src/components src/screens src/lib` fails.
- [ ] **Step 3:** Implement and do a screenshot critique of the Organise proposal with duplicates.
- [ ] **Step 4:** All gates.
- [ ] **Step 5:** `git commit -m "feat(ui): duplicates in the toolbar, organise proposal, jobs and settings"`

## Group G: Docs and verification (spec D11)

### Task 32: Docs: architecture, README, changelog, tracker

**Overlap:** light (`docs/04-architecture/organize.md`, `CHANGELOG.md`, `docs/TRACKER.md`).

**Files:**
- Create: `docs/04-architecture/dedupe.md` (pipeline, keys table, keeper rules, safety rules, journal ops, archive/disc seams, performance budget)
- Modify: `docs/04-architecture/{organize.md, identification.md, desktop-app.md, settings.md}`, `README.md`, `README.es.md` (feature bullet + `coverly dedupe` usage), `CHANGELOG.md` (Unreleased: Added), `docs/TRACKER.md` (duplicates row with evidence links)

- [ ] **Step 1:** Write the docs. Every command shown in a README must run as written on the synthetic fixture (copy-paste check).
- [ ] **Step 2:** `pnpm format:check` (Markdown under `src/` only) and a link check by `grep -o '](\.[^)]*)'` on the changed files, resolving each path.
- [ ] **Step 3:** `git commit -m "docs: duplicates architecture, usage and tracker"`

### Task 33: Verification: real sample, apply on a copy, E2E

**Overlap:** none (no source changes unless a bug is found; any fix gets its own TDD task, inserted before this one).

- [ ] **Step 1: Real dry runs (read-only).** `cargo run -p coverly-cli -- dedupe /Users/miguelangelesparzacalero/Documents/ROMS --report json --output target/dedupe-evidence/library.json` and the same for `ROMs-Downloaded` with `--one-game-one-rom` (still a dry run). Before and after, record tree hashes of both folders (`find … -type f -print0 | xargs -0 stat -f '%N %z %m' | shasum`) and assert they are equal. Compare with spec §15. List the acting vs found totals and the FC header groups.
- [ ] **Step 2: Apply on a copy.** `mkdir -p target/dedupe-sample && cp -X -R /Users/miguelangelesparzacalero/Documents/ROMS/{SMS,GB,PS} target/dedupe-sample/ROMS/`, then `coverly dedupe target/dedupe-sample --apply`. Check `_Duplicados/` shape, the `.txt` list, saves and art. `coverly undo <job>`, then compare SHA1s before/after (byte-identical). Then `--trash` with the real OS Trash on that copy only. Check the undo report lists manual restores, then empty those items from the Trash by hand. Delete `target/dedupe-sample` afterwards.
- [ ] **Step 3: Tune bulk-confirm thresholds** (DD-8 Q11) from Step 1's numbers. If they change, add a test in Task 30's file and a `fix(ui): …` commit.
- [ ] **Step 4: E2E on the mock** with Chrome DevTools MCP on `pnpm dev`. Flow: library → Duplicados → plan → change a keeper (check the live region) → "No tocar este juego" → enable Otras versiones (delta shown) → apply → done → Deshacer. Also the Organise proposal with duplicates, and keyboard only (Tab, arrows, X). Take screenshots in light/dark at 720px and 1280px, save them to `docs/assets/dedupe/`, and run a Lighthouse accessibility audit on the screen.
- [ ] **Step 5:** Update `docs/TRACKER.md` with the evidence paths and the numbers. `git commit -m "docs(tracker): record duplicates verification evidence"`

---

## Self-review checklist (done while writing this plan)

- Every spec section maps to a task: §5.1→8, §5.2→4/9, §5.3→1/10, §5.4→10/25, §5.5→11, §6→13 (+5 settings), §7.1→15/23, §7.2→22/23, §7.3→11, §8→12/23, §9.1→14, §9.2→21/24, §9.3→3/13/15, §10→3/9/15, §11→18–20/30/31, §12→all core, §13→5/31, §14→17/27, §15→16/25/33, §16→per task, §17 D1–D11→groups.
- Spec §12.4 contract mismatch with the archive branch resolved by DD-1. CHD level 3 explicitly deferred (DD-2). The only new crate is justified (DD-4).
- Every BLOCKED task names the overlapping files. Phase A (Tasks 1–20) touches no file the archive branch edits, except the locale JSON (light).
- No task decides by file name. The "names never group" tests exist at keys (9), group (11) and keeper (13) level.
- The real data paths appear only in read-only tests and in the copy-based manual apply.
