# Duplicates Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. UI tasks (Group D and Group F) also REQUIRE frontend-design:frontend-design: follow the design brief in this plan ("UI design brief") and critique each screen with screenshots before committing.

**Goal:** Coverly finds the same game stored more than once inside one system, at three levels: (1) exact copy, whatever the name, container, copier header or compression; (2) same game, other version, from the catalogue (revision, region, beta), opt-in and report-only by default; (3) same disc game, other format, from the disc serial plus a volume fingerprint (cue/bin vs chd vs pbp). It picks one copy to keep per group with ordered, configurable rules and gives a one-line reason. It then sets the other copies aside in a quarantine folder `_Duplicados/` inside the library (outside the system folders, relative paths kept; DD-11), together with their saves and art. The move is journaled and undoable, and nothing is ever deleted. Sending to the OS Trash is opt-in. All of this is a step of Organise, a library action (`DuplicatesScreen`) and `coverly dedupe` (dry run unless `--apply`).

**Architecture:** A new read-only planner in `coverly-core` (`dedupe/`): `units` (what is compared, exclusions, sets), `keys` (level 1 content keys with size bucketing, level 2 `title_key`, level 3 serial + volume fingerprint), `group` (connected components per system, weakest-link relation, report-only reasons), `saves`, `keeper` (ordered rules → `KeepReason`), `companions`, `plan` (`plan_dedupe`), and later `apply` (journaled moves to the quarantine or the Trash, revalidation, pause, resume). Archive listings and disc identity come through two narrow traits that dedupe owns (`EntryListing`, `DiscProbe`). Phase A builds and tests them with fakes. Phase B implements them over `archive::open_archive` and `disc::DiscReader`, merged into `main` with PR #24. Supporting changes: `identify/names.rs::parse_tags`, the DAT parser keeps `serial`/`region`/`cloneof`/`isbios`, cache schema v3 (`content_keys`, `dedupe_choices`, DAT columns), `HashOutcome` v2, `settings.dedupe`, journal `JobKind::Dedupe` (the Trash reuses the existing `write::trash` backend and its journal ops, DD-9). The CLI adds `coverly dedupe`. `src-tauri` adds a `dedupe` module (plan store, progress events, apply behind the job guard, undo through `organize_undo`). The React app adds `DuplicatesScreen`, `components/dedupe/*`, `dedupeStore`, a Toolbar entry, a section in the Organise proposal, a Jobs kind and a settings block.

**Tech Stack:** Rust 1.93 (MSRV raised by the archive work), edition 2021, `rusqlite`, `zip`, `crc32fast`, `sha1`, `flate2` (already deps). No new dependency: the optional Trash uses `write::trash` from `main` (DD-4 superseded by AP-3). Tauri 2.12, React 19 + TypeScript 6 strict + Tailwind 3 + zustand 5, `src/components/ui` primitives (Radix-based), `lucide-react` 1.59. Tests: `cargo test`, `insta` JSON snapshots, `assert_cmd`, Vitest + Testing Library. E2E: Chrome DevTools MCP on `pnpm dev` (mock backend).

**Spec:** [`docs/superpowers/specs/2026-10-18-duplicates-design.md`](../specs/2026-10-18-duplicates-design.md) (all sections). Its §17 milestones D1–D11 map to the task groups below. It builds on [`2026-10-17-organize-loose-roms-design.md`](../specs/2026-10-17-organize-loose-roms-design.md) and the archive spec on branch `feat/organize-disc-archives` (`docs/superpowers/specs/2026-10-18-organize-disc-archives.md`). Brand: [`docs/08-brand/brand.md`](../../08-brand/brand.md). Tracker: `docs/TRACKER.md`.

## Global Constraints

> Phase B (Tasks 21–33) works on branch `feat/dedupe-apply` and **pushes after every commit**; its own rules in "Phase B working rules" below override the branch, push, crate and real-data lines here.

- Work only in the worktree `/Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-dedupe`, branch `feat/organize-dedupe`. Never switch branches, never push, never touch `main`. Every shell command starts with `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-dedupe &&`.
- **Phase gate.** Tasks marked **BLOCKED** start only after `feat/organize-disc-archives` is merged into `main` and this branch is rebased on it (`git rebase main`, then all gates green). Tasks marked **light** may start now but will need a textual merge on rebase. Tasks marked **none** never touch a file the archive branch changes.
- TDD: write the failing test, run it and watch it fail, implement, watch it pass.
- After **every** task, run all gates and fix anything red before committing: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `pnpm lint`, `pnpm typecheck`, `pnpm format:check`, `pnpm test`, `pnpm build`. Rust-only tasks may skip the `pnpm` gates when no file under `src/`, `locales/` or `src-tauri/` changed. UI tasks run everything.
- Commits: conventional (`feat(core): …`, `test(core): …`, `feat(ui): …`, `docs: …`), granular, English. **Never** add `Co-Authored-By`, "Generated with", or any mention of Claude/AI. Do not push.
- No crate that is not already in `Cargo.lock` (DD-4 superseded: no `trash` crate). `cargo build --offline` must keep working.
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
3. **Something gets lost on apply.** Keeper changed after the plan, collision in the quarantine, cross-volume copy corrupt, FAT32 over 4 GiB, a set moved by halves, undo after a later job, trashed files. Tests: Task 22 (`trashed_rom_and_companion_come_back_on_undo`), Task 23 (`keeper_changed_skips_the_group`, `set_moves_all_or_nothing`, `fat32_destination_skips_before_any_copy`), Task 24 (`undo_restores_rom_companions_art_and_gamelist`, `undo_brings_trashed_copies_back_from_the_exact_place`, `undo_reports_a_copy_the_trash_lost`).
4. **Scale.** 50 000 files, power-of-two cartridge sizes, a warm run with no reads, a pathological 3 000-member group. Tests: Task 15 (`warm_run_hashes_nothing`, `ten_thousand_files_hash_only_colliders` ignored perf gate, `huge_group_is_collapsed_and_report_only`), Task 20 (`system_section_windows_over_200_groups`).
5. **UI traps.** A tray radio mistaken for "remove this", counts that grow silently, "free space" that is not freed, a confirm missing on an unusual plan. Tests: Task 19 (`tray_radio_has_a_visible_keep_label`, `changing_keeper_announces_and_offers_undo`), Task 20 (`option_change_shows_the_delta`), Task 30 (`bulk_confirm_above_500_files`, `same_volume_done_state_says_the_card_is_as_full`).

## Deviations from the spec (decided in this plan)

- **DD-1: the archive contract is narrower than spec §12.4.** (Phase B status: the merged work names these `archive::ArchiveReader`/`open_archive` and `disc::DiscReader`/`DiscIdentity::to_dedupe_id`; Task 25 adapts them, see "What `main` gives Phase B".) The archive branch exposes `organize::archive::ArchiveSource { list() -> Listing; stream(order) }` and puts disc evidence into sniff `detail` strings. It has no `ArchiveReader`, no `DiscId` and no `read_disc_id`. Dedupe therefore owns two narrow traits in `dedupe/source.rs`: `EntryListing` (entry sizes + CRCs + bounded open) and `DiscProbe` (`DiscId { serials, disc_no, volume, source }`). Phase A implements `EntryListing` for zip with the existing `hash_rom` / `organize::inspect` code and tests `DiscProbe` with a fake. Phase B adds `ArchiveEntryListing` over `ArchiveSource` and `IsoDiscProbe` (Task 25), which reuses the archive branch's PVD/`SYSTEM.CNF` sniff helpers and adds the boot-file SHA1, PVD timestamps, `IP.BIN` disc number and PBP `PSISO` reading.
- **DD-2: CHD.** Level 1 uses the CHD v3/v4/v5 header `sha1`/`rawsha1`, a header-only read with no decompression. Level 3 for CHD needs hunk decompression, which has no crate in the lockfile, so it is **deferred**. A CHD only takes part in level 3 if a later reader provides a `DiscId`. The summary says "Otros formatos: {n} CHD sin leer por dentro". This matches spec §15.3: 24 CHDs gave no serial.
- **DD-3: PBP serial.** `IsoDiscProbe` reads the PBP `DATA.PSAR` `PSISO` index and inflates only the blocks that hold sector 16, the root directory and `SYSTEM.CNF` (`flate2`, a bounded 1 MiB output). It never reads the outer EBOOT `PARAM.SFO` for the fingerprint (spec §5.4).
- **DD-4 (superseded by archive plan AP-3, 2026-10-24): use `write::trash` from `feat/organize-disc-archives`, no `trash` crate.** The archive branch ships `crates/coverly-core/src/write/trash.rs` (`TrashBackend` with `trash(path) -> Option<location>`, `find(volume, name, fp)`, `available(path)`; `OsTrash` for macOS `NSFileManager trashItemAtURL:resultingItemURL:`, Windows Recycle Bin, Linux freedesktop; `FakeTrash` for tests), because organise's undo needs the exact Trash location, which the `trash` crate does not return on macOS. Phase B uses that module: Task 22 adds no dependency and no `dedupe/trash.rs`, `THIRD-PARTY-NOTICES.md` needs no `trash` entry, and "Put Back" in Finder keeps working because the file goes through `NSFileManager`. The original text follows for history. **DD-4 (original): `trash` is a new dependency.** The owner's decision (optional OS Trash) cannot be met with lockfile crates. Task 22 adds `trash` 5.x (MIT) behind `dedupe::trash::TrashBackend`, records it in `THIRD-PARTY-NOTICES.md`, and runs `cargo fetch` once. On macOS it uses the Finder method so "Put Back" works. `dedupe.undo.trashed` shows "(Volver a poner)" only when `cfg!(target_os = "macos")` (spec open question 8).
- **DD-5: per-profile data stays out of `profiles/`.** The archive branch edits `profiles/{mod,builtin}.rs`. The container order defaults (spec §6.3) live in `settings.dedupe.container_order`. The off-card `save_locations` (spec §8.2) are a static table in `dedupe/saves.rs`, keyed by profile id: `muos` → internal, `anbernic-stock` → none, all others → none. Moving them into `Profile` is a follow-up.
- **DD-6: `OrganizeAction::SetAside` instead of a new `ItemAction`.** The organise plan has `OrganizeAction { Auto, Review, Left }`. Dedupe adds `SetAside { keeper: PathBuf, relation: Relation, reason: KeepReason, dest: PathBuf }`. `LeftReason::Duplicate { of }` stays deserialisable for old plans but is no longer produced. `LeftReason::InLibrary { path, relation }` is new.
- **DD-7: UI uses the `src/components/ui` primitives.** They exist now: `RadioGroup`, `ToggleGroup`, `Collapsible`, `Badge`, `Alert`, `AlertDialog`, `Progress`, `EmptyState`, `Kbd`, `DropdownMenu`, `Checkbox`, `Switch`. The older organise rule ("plain markup only") no longer applies. The design pass revised three spec details (see "UI design brief", revisions R1–R3).
- **DD-8: open questions of spec §18 get these defaults.** All can change later without changing the format. Q1 `has_save` above everything automatic. Q2 no P/C download. Q3 translations and hacks in their own class. Q4 `_Duplicados`/`_Duplicates` fixed at first run. Q5 superseded by DD-11 (always inside the library: `~/Documents/ROMS/_Duplicados`). Q6 zip vs raw has no preference. Q7 the Organise step defaults to "Apartar". Q8 per DD-4/AP-3 and DD-9 (undo brings trashed copies back by itself; Put Back also works in Finder). Q9 the quarantine stays on the card, with the honest wording. Q10 accept level 3 false negatives. Q11 keep 500 files / 25 % and re-tune in Task 33.

## Overlap map with `feat/organize-disc-archives`

> Historical: the archive branch is merged (PR #24). The BLOCKED markers in Phase A no longer gate anything; Phase B tasks name the files they touch.

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
├─ src/dedupe/source.rs                EntryListing, DiscProbe, DiscId, VolumeFingerprint, DatLookup (ZipListing removed in Task 25)
├─ src/dedupe/testing.rs               FakeDiscProbe, FakeListing, synthetic builders (cfg test-util)
├─ src/dedupe/units.rs                 Unit, UnitKind, build_units (library + organise facts), exclusions
├─ src/dedupe/keys.rs                  level 1 keys + bucketing, title_key, level 3 keys
├─ src/dedupe/group.rs                 union-find per system, weakest link, ReportOnly reasons
├─ src/dedupe/saves.rs                 save detection, blank check, conflicts, SAVE_LOCATIONS
├─ src/dedupe/keeper.rs                rules, KeepReason, container order
├─ src/dedupe/companions.rs            same-stem, art slots, shared stems, AppleDouble, gamelist entries
├─ src/dedupe/plan.rs                  plan_dedupe, DedupeProgress, phases, cache use, remembered choices
├─ src/dedupe/disc.rs                  ReaderDiscProbe over disc::DiscReader, sheets, m3u (Phase B, Task 25)
├─ src/dedupe/archive.rs               ArchiveEntryListing over archive::open_archive_with + archive::cache (Phase B, Task 25)
├─ src/disc/{pbp,cso}.rs               PBP PSISO and CSO sector sources (Phase B, Task 25)
├─ src/dedupe/apply.rs                 trash_file over write::trash, apply_dedupe, resume_dedupe, preflight (Phase B, Tasks 22–23)
├─ src/write/mod.rs                    JobKind::Dedupe, header_kind, prune (Phase B, Task 21; no new op, no header field)
├─ src/organize/{plan,target,walk,apply}.rs   SetAside, InLibrary, mark_duplicates removed (Phase B)
├─ src/scan/mod.rs                     quarantine folder skipped
└─ tests/{dedupe_units.rs, dedupe_keys.rs, dedupe_group.rs, dedupe_saves.rs, dedupe_keeper.rs,
          dedupe_companions.rs, dedupe_plan.rs, dedupe_apply.rs, dedupe_undo.rs, dedupe_disc.rs, archive_dedupe_contract.rs,
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

## Phase B — unblocked (archive and disc work merged, PR #24, `eb25460`)

Refreshed on 2026-10-25 against `main` at `fb94b20`. The tasks below replace the Phase B text written before the merge. They reuse what `main` now has (table below) instead of building it again.

### Phase B working rules (override the Global Constraints above where they differ)

- Worktree `/Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-dedupe`, branch `feat/dedupe-apply` (from `main`, tracks `origin/feat/dedupe-apply`). Every shell command starts with `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-dedupe &&`.
- **Push after every commit:** `git commit -m "…" && git push`. A branch that only exists locally can be lost. Every task ends with `git status -sb` showing nothing ahead of `origin/feat/dedupe-apply`.
- Conventional, granular, English commits. No `Co-Authored-By`, no "Generated with", no mention of Claude or AI.
- Gates after every task: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `scripts/dev/clippy-windows.sh` (must print `windows: clippy ok`), `cargo test --workspace`. When `Cargo.toml`/`Cargo.lock` change, also `scripts/check-licenses.sh` and `scripts/check-zip-features.sh`. When `src/`, `locales/` or `src-tauri/` change, also `pnpm lint`, `pnpm typecheck`, `pnpm format:check`, `pnpm test`, `pnpm build`.
- No new crate (DD-4 / AP-3). MSRV is 1.93.
- **Pinned mtimes.** On Windows, files written in one clock tick share an mtime. Every fixture whose test depends on "changed since plan", on mtime order or on a fingerprint sets its mtime with `filetime::set_file_mtime` (the `pin_mtime` helper of `crates/coverly-core/tests/dedupe_plan.rs`; move it to `coverly_core::dedupe::testing::pin_mtime` in Task 21 so the new test files share it).
- **Real data is read-only:** `/Users/miguelangelesparzacalero/Documents/ROMS`, `/Volumes/TinyExt/ROMs-Downloaded`, `/Volumes/System`. Never write there, never create `._*` files there. Write tests use `tempfile` dirs. Manual write runs only touch copies made with `cp -RXp` under `/Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-dedupe/target/`, deleted afterwards.
- Dev server only on port 1456 (`pnpm dev --port 1456 --strictPort`), stopped by port only: `lsof -ti tcp:1456 -sTCP:LISTEN | xargs -r kill`.
- E2E with Chrome DevTools MCP (`isolatedContext: 'dedupe'`). Fall back to Playwright MCP when the Chrome profile is locked.
- UI tasks load frontend-design:frontend-design and follow the "UI design brief" above and the existing tokens (`--cr-*`), primitives in `src/components/ui`, `SystemIcon` for system marks.
- TDD in every task: write the failing test, run it and watch it fail, implement, watch it pass.

### What `main` gives Phase B (read these before Task 21)

| Need | Reuse | Where |
|---|---|---|
| Archive listing, heads, CRC-checked decode | `archive::open_archive_with(path, limits, tools)`, `archive::file_entries` (dedupe's `entry: usize` index space), `ArchiveReader::open_entry(index, &mut Budget)`, `archive::cache::Cache::{listing, head, forget}` and the process-wide `Cache::global()`, `Budget::plan_head`, `Limits` (`plan_head_budget` = 64 MiB, `PLAN_WALL_TIME` = 20 s, `external_tools`), `is_budget_spent(&e)` → `Ok(None)`, `From<ArchiveError> for io::Error` (`OutOfMemory`, `InvalidData`, …) | `crates/coverly-core/src/archive/{mod,cache,budget}.rs` |
| External tools (RAR, 7z coders) | `ToolsOnDemand::new(&[library_root, quarantine])`: found on first need, never from a `PATH` entry inside those roots. Only for `Rar`/`SevenZip` and only when `limits.external_tools` (the `may_need_tool` rule in `organize/inspect.rs`) | `archive/external.rs` |
| Disc identity | `disc::DiscReader::identify_with(src, image_len, fetch)`, `DiscIdentity::to_dedupe_id()`, `disc::{FileSource, WindowSource, source::MAX_WINDOW}`, `disc::{parse_sheet, primary_track, SheetKind, sheets::MAX_SHEET_BYTES}`, `normalise_serial`. `VolumeFingerprint` and `DiscIdSource` are dedupe's own types, re-exported by `disc` | `crates/coverly-core/src/disc/*` |
| The adapters, as a template | `TestArchiveListing: EntryListing` and `TestDiscProbe: DiscProbe` with 7 tests (index space, CRC check, head budget, the caller's budget, serial + fingerprint, sheet → primary track, memory cap) | `crates/coverly-core/tests/archive_dedupe_contract.rs` |
| Trash | `write::trash::{TrashBackend, OsTrash, FakeTrash, TrashError, process_trash}`: `trash(path) -> Option<location>`, `find(volume_of, name, fp)`, `available(path)`. macOS `NSFileManager trashItemAtURL` (exact location, Finder "Put Back" works); Windows Recycle Bin of the current user only, fixed drives only; Linux freedesktop. `process_trash()` honours `COVERLY_TEST_FAKE_TRASH` in `test-util` builds | `write/trash.rs` |
| Trash journal ops | `Writer::{archive_trash_intent, archive_trashed, archive_kept}` → `Op::{ArchiveTrash, ArchiveTrashed, ArchiveKept}`. Undo (`undo_organize::restore_archives`) brings each file back from its exact Trash location, or finds it by name and fingerprint, and lists what it cannot in `UndoReport.archive_lost`. `prune_jobs_at` never prunes a journal with an unresolved trash intent. The ops accept any regular file inside the source root, not only archives | `write/mod.rs`, `write/undo_organize.rs` |
| Moves, journal, resume | `Writer::{begin_with(root, data_dir, JobMeta { kind, source_root }), resume, ensure_dir, move_file, move_file_indexed, write(…, OverwritePolicy::Backup), create_new_file, clean_temps}`, `write::{job_transfers, job_archive_steps, later_jobs, undo_job_with, undo_jobs_in_order_with}`, `write::transfer::{Collision, set_copy_fault, CopyStage}`, `write::noclobber::same_volume`, `organize::apply::{free_space, user_available, cluster_size, set_free_space_override, ApplyError}`, `organize::target::{DirIndex, file_system_is_fat32, set_fat32_override}`, `jobs::ItemErrorKind` | `write/*`, `organize/*`, `jobs/mod.rs` |

### Deviations decided in this refresh

- **DD-4 → AP-3 applied.** No `trash` crate, no `dedupe/trash.rs`, no `THIRD-PARTY-NOTICES.md` entry. The old Task 22 ("`trash` dependency behind `TrashBackend`") is dropped; the new Task 22 proves the reuse of `write::trash`.
- **DD-9: no `Op::Trashed`.** A copy sent to the Trash is journaled with the existing ops: `ArchiveTrash` (intent, with fingerprint) before the call, `ArchiveTrashed { trash }` after it, `ArchiveKept` when the volume has no Trash or the Trash refuses. Undo puts it back by itself, from the exact place. Only a copy the Trash no longer holds is reported (`archive_lost`). Older builds already know these ops, so no "unknown op" path is needed.
- **DD-10: no `Header.quarantine`.** The quarantine is recorded in `jobs/<job>.plan.json` (the saved plan, as organise does). The header only gains `kind: "dedupe"`.
- **DD-11: the quarantine is inside the library (owner, Phase B).** The default `_Duplicados/` always sits under the library root and keeps relative paths: `<root>/_Duplicados/<system folder>/<rel_dir>/<file>`. For `roms_root = "ROMS"` that is unchanged (`/Volumes/SD/_Duplicados`). For a folder opened as system folders (`roms_root = ""`) it becomes `<root>/_Duplicados` (it was `<parent>/ROMS _Duplicados`), which answers spec Q5. The writer root is then always the library root. Walks, scans and later runs skip it (`Exclusion::Quarantine`, Task 26 for organise). A custom folder (`--quarantine`, `dedupe.quarantine_dir`) may be anywhere except inside a system folder.
- **DD-12: organise apply is not refactored.** Dedupe apply is its own small run in `dedupe/apply.rs` over `Writer` and the public organise helpers above. The organise `Run` stays private.
- **DD-13: `ZipListing` is deleted in Task 25.** `units.rs`, `keys.rs`, the CLI and the tests use `dedupe::archive::ArchiveEntryListing`, so 7z, rar, tar and single-compressor units get entry sizes, CRCs and heads like zips (until now they were name-only and never grouped).
- **Windows, kept:** a remembered pick is keyed by path on Windows (`file_id: None`; std has no file id there). The keeper never moves, so apply never invalidates it. A copy renamed outside Coverly falls back to the rules (spec §9.3).

## Group B1: Journal, Trash, apply, undo (spec D6)

### Task 21: Core: `JobKind::Dedupe`, header kind, pruning, undo text

**Files:**
- Modify: `crates/coverly-core/src/write/mod.rs` (`JobKind::Dedupe`, serde `"dedupe"`; `header_kind` returns it; `prune_jobs_at` counts dedupe journals on their own counter with the organise rule: the newest `keep` plus every one younger than `KEEP_ORGANIZE_DAYS`)
- Modify: `crates/coverly-core/src/dedupe/testing.rs` (`pub fn pin_mtime(path, n)`, moved from `tests/dedupe_plan.rs`, which then imports it)
- Modify: `crates/coverly-cli/src/undo.rs` (`kind_name` → `"dedupe"`; a dedupe report uses `organize_text`; each `archive_lost` path of a dedupe job prints `not found in the Trash: {path}` through `table::terminal_safe`)
- Modify: every exhaustive `match` on `JobKind` the compiler points at (`src-tauri/src/jobs/view.rs` maps it to `"dedupe"`; the TS side follows in Task 29)
- Test: `crates/coverly-core/tests/writer.rs`, `crates/coverly-core/tests/organize_undo.rs`, `crates/coverly-cli/tests/cli_dedupe.rs` (append to each)

- [ ] **Step 1: Failing tests:**
```rust
// crates/coverly-core/tests/writer.rs
#[test] fn dedupe_job_lists_with_kind_dedupe() {}        // begin_with(JobMeta { kind: Dedupe, .. }); header line has "kind":"dedupe"; list_jobs → JobKind::Dedupe
#[test] fn dedupe_jobs_survive_thirty_fill_jobs() {}     // 30 fill journals + 1 dedupe 10 days old, prune_jobs_at(keep 20, 90, now) → dedupe kept
#[test] fn dedupe_and_organise_are_counted_apart() {}    // keep 1, both older than 90 days → the newest organise and the newest dedupe both kept
#[test] fn an_older_reader_never_undoes_a_dedupe_journal() {} // header into a copy of the pre-dedupe JobKind → Err: an older build lists it unreadable, never half-undoes it
// crates/coverly-core/tests/organize_undo.rs
#[test] fn dedupe_job_undo_moves_files_back() {}          // Moved + DirCreated in a JobKind::Dedupe journal → undo_job_with(&FakeTrash) restores and removes the folder
// crates/coverly-cli/tests/cli_dedupe.rs
#[test] fn undo_list_names_dedupe_jobs() {}               // journal written with the core into a temp data dir; `coverly undo --list --data-dir DIR` shows "dedupe"
#[test] fn undo_text_of_a_dedupe_job_names_lost_trash_items() {}
```
- [ ] **Step 2:** `cargo test -p coverly-core --test writer --test organize_undo && cargo test -p coverly-cli --test cli_dedupe` fails (no `JobKind::Dedupe`).
- [ ] **Step 3:** Implement. Organise and dedupe journals never count against the fill `keep`.
- [ ] **Step 4:** Gates (Rust only: no `src/` change).
- [ ] **Step 5:** `git commit -m "feat(core): journal duplicates jobs as their own kind" && git push`, then `git status -sb`.

### Task 22: Core: send set-aside copies to the Trash through `write::trash` (no new crate)

Proves that the Trash backend and its journal ops, built for archives, carry a set-aside ROM and its companions, before apply builds on them.

**Files:**
- Create: `crates/coverly-core/src/dedupe/apply.rs` with, for now, only:
  ```rust
  pub enum TrashStep { Trashed { location: Option<PathBuf> }, Kept { why: TrashKept }, Gone }
  pub enum TrashKept { Unavailable, Refused }
  /// archive_trash_intent → trash.trash(path) → archive_trashed(path, location);
  /// Unavailable/Refused → archive_kept (file stays); Missing → Gone. Io → Err.
  #[doc(hidden)] pub fn trash_file(w: &mut Writer, trash: &dyn TrashBackend, path: &Path, item: Option<usize>) -> Result<TrashStep, WriteError>;
  ```
- Modify: `crates/coverly-core/src/dedupe/mod.rs` (`pub mod apply`)
- Modify: `crates/coverly-core/src/write/mod.rs` (doc comments only: the three trash ops and `archive_trash_intent`/`archive_trashed`/`archive_kept` take "a file: an archive organise extracted, or a copy dedupe sets aside". No rename, no behaviour change unless a test below fails)
- Test: `crates/coverly-core/tests/dedupe_apply.rs` (new; `trash_file` is `pub` and `#[doc(hidden)]` so the integration test reaches it)

- [ ] **Step 1: Failing tests** (tempdirs, `FakeTrash::new(tmp/trash)`, pinned mtimes):
```rust
#[test] fn trashed_rom_and_companion_come_back_on_undo() {}   // GB/Tetris (1).gb + GB/Tetris (1).sav → trash → undo_job_with(&fake) → both back, byte-identical, in report.restored
#[test] fn trash_intent_is_journaled_before_the_call() {}     // FakeTrash::fail_after(0): journal has ArchiveTrash with fp, then ArchiveKept; file still in place
#[test] fn volume_without_trash_keeps_the_file() {}            // FakeTrash::unavailable → TrashStep::Kept { Unavailable }, file in place, trash dir empty
#[test] fn trash_emptied_is_reported_lost_not_an_error() {}    // empty the fake trash, undo → archive_lost = [rom], the companion still restored
#[test] fn unresolved_trash_intent_keeps_the_dedupe_journal() {} // intent line only (journal fault after it) → prune_jobs_at(keep 0) keeps the journal
```
- [ ] **Step 2:** `cargo test -p coverly-core --test dedupe_apply` fails (no `dedupe::apply`).
- [ ] **Step 3:** Implement `trash_file` over the existing Writer methods. Nothing in `write/trash.rs` changes.
- [ ] **Step 4:** Gates, including `scripts/dev/clippy-windows.sh` (the Windows Trash code is compiled there).
- [ ] **Step 5:** `git commit -m "feat(core): send set-aside copies to the Trash through the journaled trash backend" && git push`, then `git status -sb`.

### Task 23: Core: `dedupe::apply`, quarantine moves, preflight, pause and resume

**Files:**
- Modify: `crates/coverly-core/src/dedupe/plan.rs` (`default_quarantine` per DD-11: always `<root>/<name>`, `fallback` false, `writer_root` = root; `quarantine_dir` refuses a custom folder inside a system folder with `DedupeError::QuarantineInSystemFolder`)
- Modify: `crates/coverly-core/src/dedupe/apply.rs` (the run), `crates/coverly-core/src/dedupe/mod.rs` (re-exports, `DedupeDecisions`)
- Modify: `crates/coverly-core/src/cache/mod.rs` only if no accessor rekeys a `content_keys` row to a new path (add `rekey_content(volume_uuid, from, to)`)
- Test: `crates/coverly-core/tests/dedupe_apply.rs`, `crates/coverly-core/tests/dedupe_plan.rs` (update `quarantine_next_to_roms_folder_and_fallbacks` → `default_quarantine_is_inside_the_library`)

**Interfaces:**
```rust
pub struct DedupeDecisions { pub keepers: Vec<(u64 /*group*/, usize /*member*/)>, pub skipped_groups: Vec<u64>, pub skipped_systems: Vec<String> }
pub enum DedupeItemStatus { Quarantined { to: PathBuf }, Trashed { location: Option<PathBuf> }, Skipped { reason: DedupeSkip }, Failed { kind: ItemErrorKind, detail: String } }
pub enum DedupeSkip { KeeperChanged, ChangedSincePlan, SourceMissing, DestinationLimit { why: DestLimit }, TrashUnavailable, Conflict, ReportOnly, Excluded, Cancelled }
pub enum DestLimit { Fat32Size, BadName, PathTooLong, CaseCollision }
pub enum DedupeApplyNote { GamelistNotUpdated { system: String }, ArtLeaves { keeper: PathBuf } }
pub struct DedupeOutcome { pub group: u64, pub member: usize, pub status: DedupeItemStatus, pub files: Vec<PathBuf> }
pub struct DedupeReport { pub job: Option<String>, pub outcomes: Vec<DedupeOutcome>, pub bytes_out: u64, pub art_left: Vec<PathBuf>,
                          pub paused: Option<ItemErrorKind>, pub resumable: Vec<(u64, usize)>, pub notes: Vec<DedupeApplyNote> }
pub struct DedupeApplyEnv<'a> { pub data_dir: &'a Path, pub trash: &'a dyn TrashBackend, pub cache: &'a Cache,
                                pub progress: &'a (dyn Fn(usize, usize) + Sync), pub cancel: &'a AtomicBool }
pub fn apply_dedupe(plan: &DedupePlan, decisions: &DedupeDecisions, env: &DedupeApplyEnv) -> Result<DedupeReport, ApplyError>; // organize::apply::ApplyError
pub fn resume_dedupe(job: &str, env: &DedupeApplyEnv) -> Result<DedupeReport, ApplyError>;
pub fn load_dedupe_plan(data_dir: &Path, job: &str) -> Result<DedupePlan, ApplyError>;
```

**What the run does (spec §7–§9):**
1. Applies the decisions to a copy of the plan (`set_keeper`, `skip_group`, skipped systems) and checks, before writing anything, that every acting group keeps exactly one member that is not moving. A decision set that would move every member is an error.
2. `Writer::begin_with(root, data_dir, JobMeta { kind: JobKind::Dedupe, source_root })`: `root` = the library root for the default quarantine and for the Trash; for a custom quarantine on another volume, `root` = the quarantine and `source_root` = the library root. Saves `jobs/<job>.plan.json` (decisions applied) before the first move.
3. Preflight once per destination folder, before any copy: `user_available` against the bytes that will copy across volumes (`ApplyError::NoSpace`); FAT32 4 GiB limit, names FAT/exFAT/NTFS reject, path length, case-only collisions (`DirIndex`) → `DestinationLimit`. For the Trash, `trash.available(path)` once per volume → `TrashUnavailable` for every extra on it.
4. Per group, in plan order: revalidate the keeper (exists, same size and mtime) → `KeeperChanged` skips the whole group; revalidate every file of each extra → `ChangedSincePlan` / `SourceMissing`. A set (cue + bins, gdi, m3u + discs) is checked whole first and moves all-or-nothing: a write error in the middle moves its already-moved files back in the same run.
5. Quarantine: `ensure_dir` + `move_file_indexed` with `Collision::Rename` (`" (2)"`) into `<quarantine>/<system folder>/<rel_dir>/`. Travelling companions and art follow into the same relative places, with their `._` AppleDouble twins. Shared-stem companions stay. A failed companion is reported and never fails the item. The `gamelist.xml` entry is removed through `Writer::write(…, OverwritePolicy::Backup)` when the gamelist is inside the writer root; otherwise `GamelistNotUpdated`.
6. Trash: `trash_file` (Task 22) for the extra and each travelling companion and art file.
7. Writes `<quarantine>/<job>.txt` with `create_new_file` (journaled `Created`): one line per extra, `<keeper rel path> <- <extra rel path> (<relation>, <keep reason>)` with kebab-case enum names as data. Quarantine action only.
8. Moved files' `content_keys` rows follow them to the quarantine path. `dedupe_choices` are untouched: the keeper never moves, so a pick remembered by path (Windows) stays valid.
9. Errors map like organise's `on_error`: `DiskFull`, `Denied` and a vanished root pause (`paused`, `resumable`); other errors fail the item and the run goes on. Cancel is checked between groups, never inside a set. Progress is throttled to 100 ms like organise.
10. `resume_dedupe` reloads the plan, reads `job_transfers` and `job_archive_steps`, skips what the journal shows done, revalidates the rest and finishes the same job.

- [ ] **Step 1: Failing tests** (tempdirs, pinned mtimes, `FakeTrash`):
```rust
// tests/dedupe_plan.rs
#[test] fn default_quarantine_is_inside_the_library() {} // "ROMS" → <root>/_Duplicados; "" → <root>/_Duplicados (was <parent>/ROMS _Duplicados); writer_root = root
#[test] fn custom_quarantine_inside_a_system_folder_is_refused() {}
// tests/dedupe_apply.rs
#[test] fn quarantine_path_mirrors_rel_dir_and_art_follows() {}
#[test] fn gamelist_entry_removed_with_backup() {}
#[test] fn gamelist_outside_the_writer_root_is_noted_not_written() {}
#[test] fn collision_in_quarantine_gets_numbered() {}
#[test] fn keeper_changed_skips_the_group() {}
#[test] fn changed_extra_is_left_alone() {}
#[test] fn set_moves_all_or_nothing() {}                  // a missing bin skips the set before any move; set_copy_fault(Rename → error) on the 2nd bin moves the 1st back
#[test] fn conflict_and_report_only_groups_never_move() {}
#[test] fn never_moves_every_member() {}                  // decisions trying to skip the keeper → error before any write; tree unchanged
#[test] fn cross_volume_copies_verify_then_delete() {}    // set_copy_fault(CopyStage::Rename → EXDEV) → Via::Copy, verified, source gone
#[test] fn fat32_destination_skips_before_any_copy() {}   // set_fat32_override(Some(true)) + a 4.2 GiB sparse file (set_len) → DestinationLimit { Fat32Size }, nothing copied
#[test] fn bad_name_and_case_collision_on_exfat() {}
#[test] fn disk_full_pauses_and_resume_finishes() {}      // set_free_space_override / copy fault DiskFull → paused; resume_dedupe → all set aside, one job
#[test] fn cancel_between_groups_leaves_a_resumable_job() {}
#[test] fn writes_the_job_list_file_in_the_quarantine() {}
#[test] fn trash_action_sets_aside_through_the_backend_and_undo_brings_it_back() {}
#[test] fn trash_unavailable_on_the_volume_skips_with_a_reason() {}
#[test] fn content_cache_rows_follow_the_moved_files() {}
#[test] fn remembered_choice_by_path_survives_apply() {}  // a dedupe_choices row with file_id None (Windows shape) → next plan still UserChoice
#[test] fn apply_writes_nothing_outside_the_writer_root() {} // tree hash of a sibling folder before/after
```
- [ ] **Step 2:** `cargo test -p coverly-core --test dedupe_apply --test dedupe_plan` fails.
- [ ] **Step 3:** Implement DD-11 first and commit it alone: `git commit -m "feat(core): keep the default duplicates quarantine inside the library" && git push`. Then the run.
- [ ] **Step 4:** Gates, all organise apply/undo tests green, `scripts/dev/clippy-windows.sh` clean.
- [ ] **Step 5:** `git commit -m "feat(core): set aside duplicate copies with a journal, preflight and resume" && git push`, then `git status -sb`.

### Task 24: Core: undo of dedupe jobs

**Files:**
- Test: `crates/coverly-core/tests/dedupe_undo.rs` (new)
- Modify (only if a test needs it): `crates/coverly-core/src/write/undo_organize.rs`, `crates/coverly-core/src/write/mod.rs`

Undo goes through the existing `undo_job_with(data_dir, job, trash)` / `undo_jobs_in_order_with` (the organise path: per op, not per kind). Nothing dedupe-specific should be needed.

- [ ] **Step 1: Tests:**
```rust
#[test] fn undo_restores_rom_companions_art_and_gamelist() {}
#[test] fn undo_refuses_with_a_later_job_unless_include_later() {} // later_jobs → refusal; undo_jobs_in_order_with both
#[test] fn undo_keeps_a_file_that_took_the_original_place() {}  // skipped_occupied
#[test] fn undo_brings_trashed_copies_back_from_the_exact_place() {} // FakeTrash; location recorded → no lookup
#[test] fn undo_finds_a_trashed_copy_whose_location_line_is_missing() {} // crash after the Trash call: find(name, fp)
#[test] fn undo_reports_a_copy_the_trash_lost() {}               // archive_lost, the rest restored, job marked undone
#[test] fn undo_after_crash_mid_copy_removes_the_partial_copy() {}
#[test] fn undo_removes_the_job_list_file_and_empty_quarantine_folders() {}
#[test] fn undo_is_idempotent_after_an_interrupted_undo() {}
```
- [ ] **Step 2:** `cargo test -p coverly-core --test dedupe_undo`. A test that passes on the first run must be shown to fail by temporarily breaking the code path it guards (say which in the commit body).
- [ ] **Step 3:** Implement the gaps.
- [ ] **Step 4:** Gates.
- [ ] **Step 5:** `git commit -m "test(core): undo puts set-aside copies, saves, art and game lists back" && git push`, then `git status -sb`.

## Group B2: Real archive and disc readers (spec D4 completion)

### Task 25: Core: `ArchiveEntryListing` and `ReaderDiscProbe` over `archive::open_archive` and `disc::DiscReader`; `ZipListing` removed

The contract test already holds both adapters as test code. This task promotes them to `coverly-core`, points the contract test at the real types, and adds what the contract left out (m3u, PBP, CSO, tools, the plan's caches).

**Files:**
- Create: `crates/coverly-core/src/dedupe/archive.rs`:
  ```rust
  pub struct ArchiveEntryListing { limits: Limits, cache: &'static archive::cache::Cache /* Cache::global() */, tools: Option<Arc<ToolsOnDemand>> }
  impl ArchiveEntryListing {
      pub fn new(limits: Limits) -> Self;                                  // no external tools
      pub fn with_tools(limits: Limits, roots_to_avoid: &[&Path]) -> Self; // ToolsOnDemand over the library root and the quarantine
  }
  impl EntryListing for ArchiveEntryListing { /* list: Cache::listing, file entries only (EntryKind::File, archive order);
      head: Cache::head under Budget::plan_head; payload_sha1: open_entry(entry.index) under the caller's budget
      (Limits { plan_head_budget: budget, .. }), hash_stream, CRC checked by the reader; is_budget_spent → Ok(None) */ }
  ```
  Tools are passed to `open_archive_with` only for `Rar`/`SevenZip` and only when `limits.external_tools`, so a zip never triggers discovery.
- Create: `crates/coverly-core/src/dedupe/disc.rs`: `pub struct ReaderDiscProbe { listing: ArchiveEntryListing }`, `impl DiscProbe` = the contract's `TestDiscProbe` (raw image through `FileSource` with `image_len`; `.cue`/`.gdi` → `parse_sheet` + `primary_track`; inside an archive the sheet's primary track by name, case-insensitive, in the sheet's folder, else every entry in order; `WindowSource` heads ≤ `MAX_WINDOW`; a `SYSTEM.CNF` past the window fetched through `identify_with`'s `fetch`, charged to the plan head budget; the answer only when `DiscSystem::Known(id) == system`), plus:
  - `.m3u`: each listed disc probed in order → `DiscId { serials: in order, disc_no: None, volume: disc 1's when every disc has one, source: M3uPosition }`; any disc unreadable → `None`.
  - `.pbp` (DD-3) and `.cso` (PSP): read through the new sector sources below.
  - `.chd` (DD-2): `None`.
- Create: `crates/coverly-core/src/disc/pbp.rs` (PBP `DATA.PSAR` `PSISO` index → a `SectorSource` that inflates only the 16-sector blocks it is asked for, ≤ 1 MiB inflated in all; `flate2`; never the outer EBOOT `PARAM.SFO`) and `crates/coverly-core/src/disc/cso.rs` (CISO v1 index → `SectorSource`, deflate per block, same cap); `disc/testing.rs` gains `pbp_of(iso)` and `cso_of(iso)` builders.
- Modify: `crates/coverly-core/src/dedupe/source.rs` (delete `ZipListing` and its `MAX_ENTRIES`; module docs point at `dedupe::archive` / `dedupe::disc`; `NoDiscProbe` stays for tests and callers that read no discs)
- Modify: `crates/coverly-core/src/dedupe/units.rs` (`zip_kind` → `archive_kind(path, system, db, listing: &dyn EntryListing)`: any container `listing.list` reads; a single-ROM archive of any format is `UnitKind::Zip`, serde name kept for plan v1; `units_from_scan` takes the listing through `UnitOptions` or a parameter), `crates/coverly-core/src/dedupe/plan.rs` (passes `env.listing` down)
- Modify: `crates/coverly-cli/src/dedupe.rs` (`ArchiveEntryListing::with_tools(limits, &[root, quarantine])` and `ReaderDiscProbe` instead of `ZipListing`/`NoDiscProbe`; `--no-external-tools` like organise)
- Modify: `crates/coverly-core/tests/archive_dedupe_contract.rs` (delete `TestArchiveListing`/`TestDiscProbe`; every test runs against `ArchiveEntryListing`/`ReaderDiscProbe`; the `ZipListing` equality line becomes an explicit expected listing), `crates/coverly-core/tests/dedupe_keys.rs`, `tests/dedupe_units.rs`, `tests/dedupe_plan.rs`, `tests/dedupe_real_sample.rs` (no `ZipListing`)
- Test: `crates/coverly-core/tests/dedupe_disc.rs` (new)

- [ ] **Step 1: Failing tests:**
  - The contract test with its adapters deleted (it no longer compiles: the real types do not exist yet).
  - `tests/dedupe_disc.rs` (synthetic images from `disc::testing`): `ps1_bin_serial_and_fingerprint`, `iso_2048_and_bin_2352_of_the_same_disc_share_fingerprint`, `system_cnf_past_the_window_is_fetched_within_the_budget`, `pbp_reads_inner_system_cnf_not_outer_param_sfo`, `cso_psp_disc_id`, `saturn_disc_no_from_device_field`, `m3u_lists_serials_in_order`, `m3u_with_an_unreadable_disc_is_none`, `chd_returns_none`, `reads_are_bounded` (counting source: ≤ 64 KiB raw + 1 MiB inflated per PBP/CSO), `probe_of_another_system_is_none`.
  - `tests/dedupe_units.rs`: `seven_zip_and_tar_gz_units_have_entry_crcs`, `rar_without_tools_is_unreadable_not_fatal`, `tools_are_never_discovered_for_a_zip` (a `ToolsOnDemand` with a search that panics if used), `names_never_group` over 7z twins with different names.
  - `tests/dedupe_keys.rs`: `raw_vs_single_entry_7z_is_an_exact_copy`.
- [ ] **Step 2:** `cargo test -p coverly-core --test archive_dedupe_contract --test dedupe_disc --test dedupe_units --test dedupe_keys` fails.
- [ ] **Step 3:** Implement in this order, each its own commit followed by `git push`:
  1. `feat(core): read duplicates inside every archive format through the archive reader` (`dedupe/archive.rs`, `units.rs`, contract test listing half, `ZipListing` removed, tests migrated).
  2. `feat(core): read disc serials and volume fingerprints for format duplicates` (`dedupe/disc.rs`, m3u, contract test probe half, CLI wired).
  3. `feat(core): read PBP and CSO disc images for format duplicates` (`disc/pbp.rs`, `disc/cso.rs`, builders, tests).
- [ ] **Step 4:** Gates, then the ignored real-sample test (read-only): `COVERLY_LIBRARY_DIR=/Users/miguelangelesparzacalero/Documents/ROMS COVERLY_SAMPLE_DIR=/Volumes/TinyExt/ROMs-Downloaded cargo test -p coverly-core --release --test dedupe_real_sample -- --ignored --nocapture`, extended with the spec §15.3 PS groups (Crash Bash, Gran Turismo, Tekken 3 / Iron fist 3; record whether the Tekken pair is `BuildDiffers`). Check afterwards that nothing changed: `find /Users/miguelangelesparzacalero/Documents/ROMS /Volumes/TinyExt/ROMs-Downloaded -newer <stamp> | wc -l` = 0 and no `._*` new files.
- [ ] **Step 5:** `git commit -m "test(core): real-library duplicates across formats (ignored, read-only)" && git push`, then `git status -sb`.

## Group B3: Organise integration (spec D7)

### Task 26: Core: duplicates as an Organise step

**Files:**
- Modify: `crates/coverly-core/src/organize/plan.rs` (`OrganizeAction::SetAside { keeper, relation, reason, dest }` (DD-6), `LeftReason::InLibrary { path, relation }`, `Counts.{set_aside, in_library}`; call `dedupe::group_units` instead of `mark_duplicates` (now near line 1590))
- Modify: `crates/coverly-core/src/organize/target.rs` (remove `mark_duplicates`; make `match_companions` `pub(crate)` and drop dedupe's copy of it)
- Modify: `crates/coverly-core/src/organize/walk.rs` (skip the pile-side and the library-side quarantine)
- Modify: `crates/coverly-core/src/organize/apply.rs` (`SetAside` items move to `<pile root>/_Duplicados/<rel_dir>/` as an instant rename on the pile's volume, same journal, outcomes `Quarantined`/`Trashed`; trash through the `ApplyEnv.trash` already there and Task 22's `trash_file`; `applicable()` keeps requiring `Transfer::AsIs`, so a set-aside archive is never extracted first)
- Modify: `crates/coverly-core/src/scan/mod.rs` (library scans skip the quarantine folder)
- Modify: `crates/coverly-cli/src/organize.rs` (`--duplicates set-aside|report|off`, a column)
- Modify: `crates/coverly-core/tests/organize_target.rs` (drop the `mark_duplicates` test; replaced below)
- Test: `crates/coverly-core/tests/organize_plan.rs`, `crates/coverly-core/tests/organize_apply.rs`, `crates/coverly-cli/tests/cli_organize.rs` (append)

Pile units come from organise `Facts` (`units_from_facts`). Inner CRCs of zip, 7z, rar, tar and single-compressor archives come from the organise `Inspection` the plan already holds (no second read); disc identity from `OrganizeItem.disc` (`DiscId`, already filled by the merged disc work). Library units come from a scan of only the target system folders the plan touches.

- [ ] **Step 1: Failing tests:** `pile_item_equal_to_a_library_rom_under_another_name_is_in_library_and_set_aside`, `pile_7z_equal_to_a_library_zip_is_in_library` (inner CRCs from `Inspection`), `pile_disc_archive_equal_to_a_library_cue_set_by_serial_is_a_format_duplicate`, `pile_pair_headered_and_headerless_is_one_group` (regression for `raw_crcs.first()`), `keeper_follows_rules_not_the_shortest_name`, `pile_extras_stay_on_the_pile_volume`, `set_aside_archive_is_never_extracted`, `report_mode_moves_nothing_extra`, `off_mode_matches_old_behaviour_minus_shortest_name`, `walk_and_scan_skip_quarantine`, `old_plan_json_with_duplicate_left_reason_still_loads` (`tests/fixtures/organize/plan-v1.json`), `cli_organize_duplicates_flag`, `undo_of_an_organise_job_with_set_aside_items`.
- [ ] **Step 2:** `cargo test -p coverly-core --test organize_plan --test organize_apply && cargo test -p coverly-cli --test cli_organize` fails.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Gates (every organise test green, including the extract apply/resume/undo suites).
- [ ] **Step 5:** `git commit -m "feat(core): organise sets aside copies already in the library instead of keeping the shortest name" && git push`, then `git status -sb`.

## Group B4: CLI apply (spec D8, part 2)

### Task 27: CLI: `--apply`, `--quarantine`, `--trash`, `--resume`, exit codes, undo

**Files:**
- Modify: `crates/coverly-cli/src/dedupe.rs` (remove the Task 17 "available after the archive work lands" stub; build `DedupeApplyEnv` with `process_trash()` and `Cache`; print the report through `table::terminal_safe`), `crates/coverly-cli/src/undo.rs`
- Test: `crates/coverly-cli/tests/cli_dedupe.rs` (append)

- [ ] **Step 1: Failing tests** (`assert_cmd`, tempdir library, pinned mtimes): `apply_creates_the_quarantine_inside_the_library_and_a_job`, `quarantine_flag_on_another_dir`, `quarantine_flag_inside_a_system_folder_is_exit_2`, `trash_uses_the_fake_trash` (env `COVERLY_TEST_FAKE_TRASH=<tmp>`, the core's `test-util` hook; the CLI dev-dependency enables the feature), `trash_on_a_volume_without_trash_reports_skips_and_exits_0`, `disk_full_exits_5_and_resume_finishes`, `undo_restores_a_dedupe_job` (also the trashed variant: files back from the fake trash), `apply_without_changes_creates_no_job`, `dry_run_is_still_the_default`.
- [ ] **Step 2:** `cargo test -p coverly-cli --test cli_dedupe` fails.
- [ ] **Step 3:** Implement with the exit codes of spec §14 (0 ok, conflicts included; 1 I/O or job error; 2 bad usage; 3 root not found; 4 no profile; 5 paused). `--quarantine` and `--trash` stay mutually exclusive (clap `conflicts_with`).
- [ ] **Step 4:** Gates.
- [ ] **Step 5:** `git commit -m "feat(cli): apply, resume and undo duplicates jobs" && git push`, then `git status -sb`.

## Group B5: App (spec D9–D10)

### Task 28: Tauri: `DedupeManager`, IPC, events, contract

**Files:**
- Create: `src-tauri/src/dedupe/mod.rs`, `src-tauri/src/dedupe/view.rs`, `src-tauri/tests/dedupe_state.rs`
- Modify: `src-tauri/src/ipc.rs`, `src-tauri/src/lib.rs`, `src-tauri/src/state.rs`, `src-tauri/src/error.rs` (`no-dedupe-plan`, `keeper-required`, `quarantine-in-system-folder`), `src-tauri/src/jobs/view.rs` (kind `dedupe`), `src-tauri/tests/contract.rs`, `src/ipc/contract.sample.json`, `src/ipc/contract.requests.json`

**Commands:** `dedupe_plan(root, opts) -> DedupePlanView` (emits `dedupe-progress`), `dedupe_cancel(plan_id)`, `dedupe_regroup(plan_id, opts)`, `dedupe_set_keeper(plan_id, group, member)`, `dedupe_skip_group(plan_id, group, skip)`, `dedupe_skip_system(plan_id, system, skip)`, `dedupe_reset_choices(root, systems?)`, `dedupe_apply(plan_id) -> DedupeJobView` (emits `dedupe-progress`, `dedupe-changed`), `dedupe_cancel_apply()`, `dedupe_resume(job)`, `dedupe_export_plan(plan_id) -> path?`, `reveal_path(path)` (reuse if present). Undo goes through the existing `organize_undo`, which already calls `undo_job_with(…, &OsTrash)`. The plan view carries `trash_available: bool` (`OsTrash.available(root)`: false on a removable card on Windows, on a FAT volume without `.Trashes`), so the screen can disable the Trash option with a reason.

The planner env uses `ArchiveEntryListing::with_tools(limits, &[root, quarantine])` honouring the existing organise setting for external tools (`state.rs` `found_tools()` and `SettingsView::with_tools` stay the single source), and `ReaderDiscProbe`.

- [ ] **Step 1: Failing tests:** `plan_store_keeps_four_plans`, `apply_is_guarded_by_the_job_manager` (no dedupe apply while an organise or fill job runs), `set_keeper_persists_choice`, `regroup_does_not_rehash` (counting env), `progress_is_throttled`, `trash_available_is_reported`, `undo_of_a_dedupe_job_goes_through_organize_undo`, `contract_samples_match_views`.
- [ ] **Step 2:** `cargo test -p coverly-roms --test dedupe_state --test contract` fails.
- [ ] **Step 3:** Implement on the `OrganizeManager` pattern (`KEPT_PLANS`, `Throttle`, `spawn_apply`).
- [ ] **Step 4:** All gates.
- [ ] **Step 5:** `git commit -m "feat(app): duplicates commands, progress events and job wiring" && git push`, then `git status -sb`.

### Task 29: Frontend IPC: backend methods, Tauri adapter, mock backend

**Files:**
- Modify: `src/ipc/types.ts` (`export * from './dedupeTypes'`; `JobKind = 'fill' | 'organize' | 'dedupe'`), `src/ipc/dedupeTypes.ts` (`trash_available`, apply/report views; organise `set-aside`/`in-library` wire types), `src/ipc/backend.ts`, `src/ipc/tauriBackend.ts`, `src/ipc/mock/mockBackend.ts`
- Create: `src/ipc/mock/mockDedupe.ts`, `src/ipc/mock/mockDedupe.test.ts`
- Test: `src/ipc/contract.test.ts`, `src/ipc/tauriBackend.test.ts`

- [ ] **Step 1: Failing tests:** `mock_plan_emits_phases_in_order`, `mock_set_keeper_moves_the_old_keeper_to_the_tray`, `mock_apply_then_undo_restores_counts`, `mock_trash_undo_brings_files_back` (no manual-restore list: DD-9), `tauri_adapter_uses_camelCase_args`, `contract_round_trip`.
- [ ] **Step 2:** `pnpm vitest run src/ipc` fails.
- [ ] **Step 3:** Implement. The mock is deterministic and uses `src/ipc/mock/dedupeFixture.ts` (Task 18) and, for archives and discs, the shapes of `src/ipc/mock/discFixture.ts`/`sampleFixture.ts`. Expose `window.mockDedupe*` hooks for E2E, like `mockOrganizeTouch`.
- [ ] **Step 4:** All gates.
- [ ] **Step 5:** `git commit -m "feat(ui): duplicates backend methods and mock" && git push`, then `git status -sb`.

### Task 30: Frontend: `dedupeStore`, navigation, `DuplicatesScreen` (shelf and tray)

**Files:**
- Create: `src/stores/dedupeStore.ts` (+ test), `src/screens/DuplicatesScreen.tsx` (+ test)
- Modify: `src/stores/navStore.ts` (`Screen = 'main' | 'settings' | 'organize' | 'duplicates'`), `src/App.tsx`, `locales/{es,en}/ui.json` (same keys; `src/i18n/parity.test.ts`)

**States:** `idle | planning | ready | applying | done | error`. Planning shows the phase sequence (Listando, Tamaños, Huellas, Números de serie, Catálogos) with checks. Ready shows the summary sentence (Archivo 32 on the first clause), the options strip (`DedupeOptions`), the conflicts alert, the systems (`SystemSection` → `DupGroup`: the keeper on the shelf, the copies in the tray, Task 19–20 components unchanged), "Sin cambios" and a sticky bar. Applying uses `<Progress slots={16}>` and Cancel. Done shows `DedupeResult`. The empty state is `EmptyState` ("Este estante está en orden."). Error says what failed, then "No se ha movido nada." and offers a retry. The Trash option is disabled with its reason when `trash_available` is false. The Trash confirm (`dedupe.trash.confirmBody`, revised in spec §11.5) says that undo brings the files back from the Trash and that emptying the Trash makes that impossible (DD-9, no "restore by hand" text).

- [ ] **Step 1: Failing tests:** `store_plan_set_keeper_skip_apply_undo` (on the mock), `cmd_r_replans`, `sticky_bar_counts_files_and_bytes_out_of_roms`, `primary_disabled_with_reason_when_zero`, `bulk_confirm_above_500_files` (and above 25 % of bytes, a confirmed big group, or a quarantine on another volume), `trash_confirm_names_count_bytes_and_says_undo_brings_them_back`, `trash_option_disabled_when_the_volume_has_no_trash`, `same_volume_done_state_says_the_card_is_as_full`, `export_report_writes_json`, `off_card_saves_note_once_per_system`, `chd_unread_note`, `empty_state_lists_systems_without_catalogue`, `error_state_says_nothing_moved`.
- [ ] **Step 2:** `pnpm vitest run src/stores/dedupeStore.test.ts src/screens/DuplicatesScreen.test.tsx` fails.
- [ ] **Step 3:** Implement. Reuse the `OrganizeScreen` header and sticky-bar markup (copy the classes, do not import organise components). Screenshot critique on `pnpm dev --port 1456 --strictPort` (mock) in light/dark at 720/1280 px, with Chrome DevTools MCP (`isolatedContext: 'dedupe'`); record the changes in the commit body. Stop the server with `lsof -ti tcp:1456 -sTCP:LISTEN | xargs -r kill`.
- [ ] **Step 4:** All gates.
- [ ] **Step 5:** `git commit -m "feat(ui): duplicates screen with planning, review, apply and result" && git push`, then `git status -sb`.

### Task 31: Frontend: entry points: Toolbar, Organise section, Jobs panel, settings

**Files:**
- Modify: `src/components/library/Toolbar.tsx` (outline button "Buscar duplicados" / "Duplicados {count}", icon-only below 1180px, lucide `Copy`)
- Modify: `src/components/organize/OptionsBar.tsx` (picker Duplicados: Apartar · Solo informar · No buscar), `src/screens/OrganizeScreen.tsx` (summary "Duplicados: {n}", "Ya en tu biblioteca: {n}", a section rendering `DupGroup` between `ReviewBuckets` and `LeftInPlace`, capped at 200 rows; primary "Organizar {n} archivos y apartar {m}"; an archive that would be extracted and is a duplicate shows as set aside, not extracted), `src/components/organize/OrganizeResult.tsx` (outcomes `quarantined`/`trashed`), `src/lib/organize.ts`
- Modify: `src/components/fill/JobsPanel.tsx` (label "Duplicados", Resume/Undo through the organise branches, the fill-undo filter excludes dedupe; an undo report with `archive_lost` items lists them with `dedupe.undo.notInTrash`, "No estaba en la Papelera: {path}")
- Create: `src/components/settings/DedupeSettings.tsx`. Modify `src/components/settings/LibrarySection.tsx` to mount it under the organise block: three level switches, "Una ROM por juego", action radio (Apartar / Papelera), quarantine path + "Elegir…" (default shown as `<biblioteca>/_Duplicados`), a reorderable rule list on the `MatchingSection` pattern with `ReorderAnnouncer`, the region rule reading "Usa el orden de Coincidencias" with a link, a collapsed container order editor, and exclusions.
- Modify: `src/lib/settingsRules.ts` (+ test): no duplicate rules; a custom quarantine never inside a system folder
- Tests: the matching `*.test.tsx` files

- [ ] **Step 1: Failing tests:** `toolbar_shows_find_then_count`, `organize_options_picker_defaults_to_set_aside`, `organize_apply_label_counts_set_aside`, `organize_section_reuses_dup_group`, `jobs_panel_dedupe_undo_uses_organize_undo`, `jobs_panel_lists_copies_the_trash_lost`, `settings_reorder_rules_announces`, `settings_rejects_quarantine_inside_a_system_folder`.
- [ ] **Step 2:** `pnpm vitest run src/components src/screens src/lib` fails.
- [ ] **Step 3:** Implement and do a screenshot critique of the Organise proposal with duplicates (port 1456, Chrome DevTools MCP).
- [ ] **Step 4:** All gates.
- [ ] **Step 5:** `git commit -m "feat(ui): duplicates in the toolbar, organise proposal, jobs and settings" && git push`, then `git status -sb`.

## Group G: Docs and verification (spec D11)

### Task 32: Docs: architecture, README, changelog, tracker

**Files:**
- Create: `docs/04-architecture/dedupe.md` (pipeline, keys table, keeper rules, safety rules, journal ops (the reused trash ops, DD-9), the archive/disc adapters over `archive::`/`disc::`, the quarantine inside the library (DD-11), performance budget)
- Modify: `docs/04-architecture/{organize.md, identification.md, desktop-app.md, settings.md}`, `README.md`, `README.es.md` (feature bullet + `coverly dedupe` usage), `CHANGELOG.md` (Unreleased: Added), `docs/TRACKER.md` (duplicates phase B rows with evidence)

- [ ] **Step 1:** Write the docs. Every command in a README must run as written on the synthetic fixture (copy-paste check).
- [ ] **Step 2:** `pnpm format:check` and a link check (`grep -o '](\.[^)]*)'` on the changed files, resolving each path).
- [ ] **Step 3:** `git commit -m "docs: duplicates architecture, usage and tracker" && git push`, then `git status -sb`.

### Task 33: Verification: real sample, apply on a copy, E2E

**Overlap:** none (no source changes unless a bug is found; any fix gets its own TDD task inserted before this one, with its own commit and push).

- [ ] **Step 1: Real dry runs (read-only).** `cargo run -p coverly-cli -- dedupe /Users/miguelangelesparzacalero/Documents/ROMS --report json --output target/dedupe-evidence/library.json`, and the same for `/Volumes/TinyExt/ROMs-Downloaded` with `--one-game-one-rom` (still a dry run). Before and after, record tree hashes of both folders (`find … -type f -print0 | xargs -0 stat -f '%N %z %m' | shasum`) and assert they are equal; check no `._*` file appeared. Compare with spec §15, now including level 3 across 7z/zip/cue/pbp. List the acting vs found totals and the FC header groups.
- [ ] **Step 2: Apply on a copy.** `mkdir -p target/dedupe-sample/ROMS && cp -RXp /Users/miguelangelesparzacalero/Documents/ROMS/{SMS,GB,PS} target/dedupe-sample/ROMS/`, then `coverly dedupe target/dedupe-sample --apply`. Check `target/dedupe-sample/_Duplicados/` (inside the library, relative paths kept), the `.txt` list, saves and art. `coverly undo --job <job>`, then compare SHA1s before/after (byte-identical). Then `--trash` with the real OS Trash on that copy only, then `coverly undo --job <job>`: the files come back from the Trash by themselves (macOS `NSFileManager` location); nothing is left in the Trash from the run. Delete `target/dedupe-sample` afterwards.
- [ ] **Step 3: Tune the bulk-confirm thresholds** (DD-8 Q11) from Step 1's numbers. If they change, add a test in Task 30's file and a `fix(ui): …` commit, pushed.
- [ ] **Step 4: E2E on the mock** with Chrome DevTools MCP (`isolatedContext: 'dedupe'`; Playwright MCP if the profile is locked) on `pnpm dev --port 1456 --strictPort`. Flow: library → Duplicados → plan → change a keeper (check the live region) → "No tocar este juego" → enable Otras versiones (delta shown) → apply → done → Deshacer; the Trash variant; the Organise proposal with duplicates (a 7z already in the library shows as set aside); keyboard only (Tab, arrows, X). Screenshots in light/dark at 720px and 1280px saved to `docs/assets/dedupe/`; a Lighthouse accessibility audit on the screen. Stop the server by port.
- [ ] **Step 5:** Update `docs/TRACKER.md` with the evidence paths and numbers. `git commit -m "docs(tracker): record duplicates verification evidence" && git push`, then `git status -sb`.


---

## Self-review checklist (done while writing this plan)

- Every spec section maps to a task: §5.1→8, §5.2→4/9, §5.3→1/10, §5.4→10/25, §5.5→11, §6→13 (+5 settings), §7.1→15/23, §7.2→22/23 (write::trash), §7.3→11, §8→12/23, §9.1→14, §9.2→21/22/24, §9.3→3/13/15, §10→3/9/15, §11→18–20/30/31, §12→all core, §13→5/31, §14→17/27, §15→16/25/33, §16→per task, §17 D1–D11→groups.
- Spec §12.4 contract mismatch with the archive branch resolved by DD-1. CHD level 3 explicitly deferred (DD-2). No new crate (DD-4 superseded by AP-3). Phase B refreshed on 2026-10-25 against `main` (DD-9 to DD-13).
- Every BLOCKED task names the overlapping files. Phase A (Tasks 1–20) touches no file the archive branch edits, except the locale JSON (light).
- No task decides by file name. The "names never group" tests exist at keys (9), group (11) and keeper (13) level.
- The real data paths appear only in read-only tests and in the copy-based manual apply.
