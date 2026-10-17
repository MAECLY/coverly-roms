# Organise Loose ROMs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Picking a plain folder that is just a pile of ROMs stops being a dead end ("No se reconoce ningún formato en …"). Coverly identifies every game in the pile and its system (hash → header magic → extension → name, each with a confidence and a "why"), proposes one folder per system following a layout profile (Anbernic stock `ROMS/<SYS>/` by default, any built-in or custom profile by choice), lets the user settle doubtful items, applies the move or copy safely (dry run first, never overwrite, journal, undo, resume) and opens the result as a normal library where scan, audit and "fill covers" work unchanged. The same planner runs from the CLI (`coverly organize`) and from the app (start-screen offer, proposal screen, library "Organise loose files…").

**Architecture:** `coverly-core` gains `identify::sniff` (pure header-magic checks over a bounded `ReadAt`), `identify::evidence` (shared `Evidence`/`IdLevel` types) and a new `organize` module: `walk` (read-only walk + classification), `inspect` (raw file / bounded zip entry / 7z-CHD-RAR magic / disc sets), `evidence` (identification ladder over a `Catalogue` of cached DATs + `combine`), `target` (folder naming per profile, target paths, collisions, duplicates, companions), `plan` (`plan_organize`, `probe_folder`, plan types, decisions) and `apply` (journaled move/copy with re-validation, pause and resume). `write` gains `rename_noclobber`, `Writer::{move_file, copy_file, ensure_dir}`, fingerprints, new journal ops and their undo, kind-aware pruning. The cache moves to schema v2 (CRC index, memo of per-file facts, remembered user choices). `profiles` gains `FolderNaming`, host folders, arcade order and a weak `system-folders` detection rule. The CLI adds `coverly organize`; `src-tauri` adds an `organize` module (plan store keyed by `plan_id`, progress events, apply as a job behind the `JobManager` manual guard, undo with the later-jobs check); the React app adds an offer panel on the start screen, an `organizeStore` and an `OrganizeScreen`, wired through the existing `Backend` interface with a deterministic mock.

**Tech Stack:** Rust 1.90 (edition 2021), `zip` 9 (feature `deflate-flate2` turned on; `flate2` 1.1 moves from dev to normal dependency — both already in `Cargo.lock`), `walkdir`, `crc32fast`, `rusqlite`; new **direct** dependencies only on crates already in `Cargo.lock`: `libc` 0.2 (unix) and `windows-sys` 0.61 (windows) for no-clobber renames and volume queries. No `rayon`, no `proptest`: bounded `std::thread::scope` workers and a deterministic xorshift "random bytes" test. Tauri 2.12, React 19 + TypeScript 6 (strict, `exactOptionalPropertyTypes`, `noUncheckedIndexedAccess`) + Tailwind 3 + zustand 5. Tests: `cargo test`, `insta` JSON snapshots, `assert_cmd`, Vitest + Testing Library. Mock HTTP: `coverly_core::net::testing::MockServer`. E2E: Chrome DevTools MCP on `pnpm dev` (mock backend).

**Spec:** [`docs/superpowers/specs/2026-10-17-organize-loose-roms-design.md`](../specs/2026-10-17-organize-loose-roms-design.md) (all sections; §16 milestones O1–O10 map to the task groups below) on top of the master spec [`2026-10-15-coverly-roms-design.md`](../specs/2026-10-15-coverly-roms-design.md) (§10 safety, §16.10 safe writes). Tracker: `docs/TRACKER.md`. Architecture docs to update: `docs/04-architecture/identification.md`, `fill-pipeline.md`, `desktop-app.md`, new `organize.md`.

## Global Constraints

- Work only in the worktree `/Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize`, branch `feat/organize-loose-roms`. Never switch branches, never push, never touch `main`. Every shell command starts with `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize &&`.
- TDD: write the failing test, run it and watch it fail, implement, watch it pass.
- After **every** task run all gates and fix anything red before committing: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `pnpm lint`, `pnpm typecheck`, `pnpm format:check`, `pnpm test`, `pnpm build`. (Rust-only tasks may skip the `pnpm` gates when no file under `src/`, `locales/` or `src-tauri/` changed; UI tasks run everything.)
- Commits: conventional (`feat(core): …`, `test(core): …`, `docs: …`), granular, English. **Never** add `Co-Authored-By`, "Generated with", or any mention of Claude/AI. Do not push.
- `rust-version = "1.90"`, `edition = "2021"` stay (no let-chains). No crate that is not already in `Cargo.lock`; `cargo build --offline` must keep working.
- The core never depends on Tauri and holds no UI strings: reasons and evidence are enums/ids plus data; the UI and the CLI render them.
- **Real data is read-only.** Never write to `/Volumes/System`, `/Users/miguelangelesparzacalero/Documents/ROMS` or `/Users/miguelangelesparzacalero/Documents/ROMs-Downloaded`, and never create `._*` files there. Write tests use `tempfile` dirs. Manual apply runs use a copy under `target/organize-sample/` made with `cp -X` (no xattrs, so no AppleDouble files).
- Tests never hit the real network (Rust: `MockServer` on 127.0.0.1; UI: the mock backend). The real-sample test is `#[ignore]`, gated by `COVERLY_SAMPLE_DIR`, plan only (read-only).
- Fixtures are synthetic and tiny: no copyrighted ROM bytes in the repo; header fixtures contain only the bytes a check reads, checksums computed by helpers.
- Every UI string goes through i18n in `locales/en/ui.json` **and** `locales/es/ui.json` (same keys; `src/i18n/parity.test.ts`). Calm archivist voice, exact counts, no exclamation marks.
- UI markup stays simple and uses the existing classes (`section-title`, `hint`, `primary-button`, `ghost-button`, `icon-button`, `error-banner`, `recent-list`, plain `<select>`, `<details>`, `<ul>`): another branch is replacing raw controls with `src/components/ui` primitives and must merge cleanly. No new UI library.
- File names are hostile: React text only (never `dangerouslySetInnerHTML`), CLI output through `table::terminal_safe`, JSON exports as data.
- IPC: JS argument names camelCase, payload fields snake_case, errors `{ code, detail? }` shown as `errors.<code>`; codes are only added, never renamed. `no-profile` stays as is: the UI reacts to it. The webview never sends a plan back: only `plan_id` + decisions.

## Review Focus

1. **Never overwrite, never lose a file.** Target exists (exact or case/NFC twin), target created between plan and apply, two sources to one target, `rename` that would silently replace on Unix/Windows, cross-volume move whose copy is corrupt, source delete that fails after a verified copy. Expected: `(2)` names or skip with a reason, source kept until the copy is verified, never a replaced file. Tests: Task 17 (`rename_noclobber_refuses_an_existing_target`, `refuses_a_case_twin`, `falls_back_to_link_when_exclusive_rename_is_unsupported`), Task 19 (`copy_verifies_by_rereading_and_keeps_the_source_on_mismatch`, `a_failed_source_delete_is_reported_as_copied`), Task 21 (`a_target_created_after_planning_gets_a_numbered_name`, `a_set_collision_skips_the_whole_set`).
2. **Undo puts everything back, even after a crash or later jobs.** Crash between copy and source delete, target modified after the move, source path re-occupied, later fill job wrote art under the organised tree, journal from before this change. Tests: Task 20 (`undo_after_a_crash_between_copy_and_delete_removes_the_copy`, `undo_keeps_a_moved_file_that_was_changed`, `undo_refuses_when_later_jobs_wrote_under_the_target`, `an_old_fill_journal_still_parses_and_undoes`), Task 20 (`organise_journals_survive_thirty_fill_jobs`).
3. **Wrong system with high confidence.** GBC-only cart placed in `GB/`, Sega CD / 32X / Pico image taken as Mega Drive, PS2 disc taken as PS1, NDS taken as GBA, an HTML page named `.zip`, random `.bin`/`.iso` in a broad folder proposed for moving. Expected: vetoes and the "content support" rule hold; random bytes never reach Auto. Tests: Task 5, 6, 8 (`random_bytes_never_reach_high_confidence`), Task 12 (`gbc_only_header_never_goes_to_gb`, `extension_and_name_alone_never_reach_auto`), Task 16 (`generic_files_in_a_broad_folder_are_never_proposed`).
4. **Hostile archives.** Zip bombs, 10 000-entry zips, encrypted entries, truncated zips, `.zip` that is HTML, 7z with a bad magic. Expected: bounded reads (128 KiB prefix, 8 MiB stream cap), a reason per file, never a crash or a hang. Tests: Task 9 (`a_zip_bomb_entry_is_read_bounded`, `a_truncated_zip_is_damaged`, `an_html_page_named_zip_is_not_a_rom`).
5. **Files still arriving / not really there.** `.crdownload`, Firefox `x.zip` + `x.zip.part`, mtime within 60 s, iCloud dataless placeholders, zero-byte files. Expected: left in place with the reason, never read (placeholders), never moved. Tests: Task 11 (`partial_downloads_are_left_with_the_reason`, `a_file_with_a_part_sibling_is_still_downloading`, `cloud_only_files_are_identified_by_name_only`).
6. **10 000 files.** Plan without full-file hashing when headers decide; apply without O(n²) directory scans; UI renders capped groups. Tests: Task 16 (`ten_thousand_gba_headers_plan_without_hashing`, ignored perf gate), Task 21 (`apply_uses_a_directory_index`), Task 32 (`renders_at_most_200_rows_per_group`).

## Deviations from the spec / task brief (decided in this plan)

- **O-D1 — Journal paths stay absolute.** Existing journal entries store absolute `target` paths (`Writer::write`). Organise entries do the same for `target` and `from`; the header gains `source_root` and `kind`, so the spec's relative example is a display choice, not the on-disk format. Undo checks `ensure_within(root|source_root, …)` exactly as before.
- **O-D2 — No new crates beyond the lockfile.** Parallel identification uses `std::thread::scope` with an atomic work index (4 workers on internal disks, 2 on removable/network volumes), not `rayon`. The "random bytes" property test is a deterministic xorshift loop, not `proptest`. `libc` (unix) and `windows-sys` 0.61 (windows) become direct dependencies — both already resolved in `Cargo.lock`.
- **O-D3 — Same-volume test on Windows** compares the path prefix (drive letter or UNC share), because `MetadataExt::volume_serial_number` is unstable; `rename_noclobber` still falls back to copy on `ERROR_NOT_SAME_DEVICE` (17).
- **O-D4 — Sniff takes the extension.** `sniff(src, len, ext)` resolves family ties that only the extension can settle (SMS vs Game Gear, WonderSwan footer only checked for `.ws`/`.wsc`), so `combine` never needs family special-cases. The WonderSwan footer check also requires `len % 65536 == 0` (a single `0xEA` byte would otherwise make 1 in 256 random files a WonderSwan ROM).
- **O-D5 — "Decisive" negative sniff** applies to systems whose signature every real dump carries: `gb`, `gbc`, `gba`, `nds`, `n64`, `nes`, `fds`, `sega32x`. SNES, Mega Drive, SMS and the disc systems have legitimate dumps without the checked bytes, so a failed sniff there is "no evidence", not negative evidence. Markdown named `.md` is caught by the text rule.
- **O-D6 — The planner memoises facts, not verdicts.** `organize_ids` stores per-file *facts* (container, inner names, CRCs, sniff results), keyed by path + size + mtime; DAT lookups and `combine` re-run on every plan (cheap SQL), so a DAT downloaded later or a threshold change is honoured without re-reading files.
- **O-D7 — Files that are not ROM-like** (no known ROM/disc/archive extension and not a companion: `.dmg`, `.exe`, `.docx`…) are counted as `other_files` and listed only on demand, not as *unknown*: a broad folder must not drown the review in personal files. *Unknown* stays for ROM-like files Coverly could not place.
- **O-D8 — Broad roots** (home, `Desktop`, `Documents`, `Downloads`, `/`, a volume root, `C:\`): besides the warning, items without content evidence (no DAT hit and no header ≥ 0.85) are *unknown* instead of *review*.
- **O-D9 — Online lookup** (spec §6.5) is an optional extra level implemented with ScreenScraper `jeuInfos` by CRC (Task 24), only when that source is available in the build, enabled in settings, and the plan is not offline; it only touches items still in review/unknown, at most 3 candidate systems each, capped at 0.80. libretro and SteamGridDB have no lookup-by-hash, so they add nothing here.
- **O-D10 — Device folder lists.** "This layout has no folder for {system}" needs to know which folders a device shows. Only `anbernic-stock` ships a list (the reference `~/Documents/ROMS` folder names, `Profile.known_folders`); other profiles leave it empty and never show the hint.
- **O-D11 — Open questions of spec §17** get these defaults (all changeable later without format changes): nesting `<picked>/ROMS/<SYS>` (Q1), *Names: keep* stays the default (Q2), dual-mode GB/GBC without a DAT hit → `gbc` (Q3), no 7z crate (Q4), `fbneo` first for Anbernic stock (Q5), BIOS files stay in place (Q6).

---

## File structure

```
crates/coverly-core/
├─ Cargo.toml                         zip features ["deflate-flate2"]; flate2 → [dependencies];
│                                     libc (cfg unix), windows-sys 0.61 (cfg windows)
├─ src/lib.rs                         + pub mod organize
├─ src/scan/mod.rs                    pub(crate) mod classify / multifile; partial downloads skipped
├─ src/scan/classify.rs               + PARTIAL_DOWNLOAD_EXTS, is_partial_download, is_zone_identifier,
│                                       is_bios_name, COMPANION/DOC exts
├─ src/systems/mod.rs                 + ExtClass, extension_class, systems_for_extension, system_for_dat
├─ src/profiles/mod.rs                + FolderNaming, Profile.{system_folder, host_folders, arcade_order,
│                                       known_folders}; pub mod naming
├─ src/profiles/naming.rs             system_folder(), FolderChoice, validate_folder_override
├─ src/profiles/builtin.rs            naming per profile, Anbernic host folders + known folders
├─ src/profiles/custom.rs             ProfileFile.{system_folder, folder_names}
├─ src/profiles/detect.rs             weak `system-folders` rule
├─ src/identify/mod.rs                + pub mod evidence; pub mod sniff
├─ src/identify/evidence.rs           IdLevel, Evidence
├─ src/identify/sniff.rs              ReadAt (+ SliceAt, FileAt, StreamAt), Sniff, sniff()
├─ src/cache/mod.rs                   schema v2: dat_roms_crc_any index, organize_ids, organize_choices;
│                                       dat_by_crc_any, dat_stems, cached_dats, facts/choices accessors
├─ src/settings/model.rs              + OrganizeSettings (settings.organize)
├─ src/settings/validate.rs           + organize ranges
├─ src/sources/screenscraper.rs       + identify_by_hash (Task 24)
├─ src/write/mod.rs                   Header.{source_root, kind}, Entry.{sha1: Option, fp}, Op::{Moved, Copied,
│                                       DirCreated, OverrideMoved, Unknown}, Fingerprint, JobKind,
│                                       Writer::{begin_with, move_file, copy_file, ensure_dir, journal_override},
│                                       MoveOutcome, Collision, undo of new ops, later_jobs, kind-aware prune
├─ src/write/noclobber.rs             rename_noclobber, same_volume, retry_sharing_violation
├─ src/organize/mod.rs                pub API re-exports; OrganizeOptions, TransferMode, NameMode
├─ src/organize/plan.rs               OrganizePlan, OrganizeItem, ItemKind, OrganizeAction, LeftReason,
│                                       Counts, Decision, plan_organize, probe_folder, FolderProbe
├─ src/organize/walk.rs               walk(), Candidate, Walked, MetaProbe (cloud-only), is_broad_root
├─ src/organize/inspect.rs            inspect_file(), Inspection, Payload, Container, ArchiveIssue, disc sets
├─ src/organize/evidence.rs           Catalogue trait, DatCatalogue, Facts, gather_evidence(), combine()
├─ src/organize/target.rs             TargetLayout, folder for system, target paths, collisions, duplicates,
│                                       companions, FAT32 / path-length rules, DirIndex
├─ src/organize/apply.rs              apply_organize(), resume_organize(), OrganizeReport, persisted plan
└─ tests/{sniff.rs, organize_inspect.rs, organize_evidence.rs, organize_walk.rs, organize_target.rs,
          organize_plan.rs, organize_apply.rs, organize_undo.rs, noclobber.rs, organize_real_sample.rs,
          common/roms.rs (synthetic ROM builders)}
crates/coverly-cli/
├─ src/main.rs                        + Organize subcommand
├─ src/organize.rs                    run(): plan / apply / resume, table + json
└─ tests/cli_organize.rs
src-tauri/
├─ src/lib.rs                         + organize commands
├─ src/ipc.rs                         probe_folder, organize_plan, organize_cancel, organize_apply,
│                                       organize_resume, organize_undo, organize_export_plan; events
├─ src/organize/mod.rs                OrganizeManager (plans by id, one apply at a time), progress throttle
├─ src/organize/view.rs               wire types (FolderProbeView, OrganizePlanView, OrganizeItemView, …)
├─ src/jobs/view.rs                   JobHistoryView.kind
├─ src/error.rs                       + no-plan, later-jobs
└─ tests/{organize_state.rs, contract.rs}
src/
├─ ipc/{types.ts, backend.ts, tauriBackend.ts, errors.ts, contract.sample.json, contract.requests.json,
│       contract.test.ts}
├─ ipc/mock/{mockOrganize.ts, mockOrganize.test.ts, mockBackend.ts}
├─ stores/{organizeStore.ts, organizeStore.test.ts, libraryStore.ts (open(path, profileId?)), navStore.ts}
├─ lib/organize.ts (+ .test.ts)       grouping, buckets, reason → i18n key, confidence label
├─ components/organize/{OrganizeOffer, OptionsBar, SystemGroups, ReviewBuckets, LeftInPlace,
│                       OrganizeResult}.tsx (+ tests)
├─ screens/{StartScreen.tsx, OrganizeScreen.tsx (+ test), LibraryScreen.tsx}, App.tsx
├─ components/library/Toolbar.tsx     "Organise loose files…"
└─ components/settings/LibrarySection.tsx   Organise subsection
locales/{en,es}/ui.json
README.md, README.es.md, docs/04-architecture/{organize.md, identification.md, desktop-app.md}, docs/TRACKER.md,
CHANGELOG.md
```

Synthetic ROM builders shared by the core tests live in `crates/coverly-core/tests/common/roms.rs` (`mod roms;` from `tests/common/mod.rs`) and, for unit tests inside `identify/sniff.rs`, in a `#[cfg(test)] pub(crate) mod fixtures` of the same file. The test-side copy re-exports the in-crate one through `coverly_core::identify::sniff::fixtures` behind the existing `test-util` feature, so there is one implementation.

---

## Group O1 — Foundations

### Task 1: Core — partial downloads, BIOS names and Windows stream files in `classify`

**Files:**
- Modify: `crates/coverly-core/src/scan/mod.rs:10-11` (`mod classify; mod multifile;` → `pub(crate) mod …`)
- Modify: `crates/coverly-core/src/scan/classify.rs`
- Test: `crates/coverly-core/src/scan/classify.rs` (unit), `crates/coverly-core/tests/scan_fixtures.rs` (append)

**Interfaces:**
- Produces (all `pub(crate)` in `crate::scan::classify`):
  - `const PARTIAL_DOWNLOAD_EXTS: [&str; 10]`
  - `fn is_partial_download(name: &str) -> bool`
  - `fn is_zone_identifier(name: &str) -> bool`
  - `fn is_bios_name(name: &str) -> bool`
  - `const COMPANION_EXTS: [&str; 12]` (default of `settings.organize.companion_exts`)
  - `is_skipped_entry` also skips partial downloads and `:Zone.Identifier` files (the scan reuses it).

- [ ] **Step 1: Write the failing unit tests** (append to `classify.rs` `mod tests`)

```rust
#[test]
fn partial_downloads_by_suffix_any_case() {
    for name in [
        "Tetris.zip.crdownload", "Tetris.zip.part", "x.PARTIAL", "x.download",
        "x.opdownload", "x.tmp", "x.!ut", "x.!qb", "x.zip.aria2", "x.filepart",
    ] {
        assert!(is_partial_download(name), "{name}");
    }
    for name in ["Tetris.zip", "Party.gba", "partial.gb", "download.nes"] {
        assert!(!is_partial_download(name), "{name}");
    }
}

#[test]
fn zone_identifier_streams() {
    assert!(is_zone_identifier("Tetris.zip:Zone.Identifier"));
    assert!(is_zone_identifier("a.gba:zone.identifier"));
    assert!(!is_zone_identifier("Zone.Identifier.gba"));
}

#[test]
fn bios_names_exact_and_by_prefix() {
    for name in [
        "gba_bios.bin", "GBA_BIOS.BIN", "bios7.bin", "bios9.bin", "firmware.bin",
        "scph1001.bin", "SCPH5501.BIN", "neogeo.zip", "syscard3.pce", "disksys.rom",
        "dc_boot.bin", "dc_flash.bin", "bios_CD_U.bin", "sega_101.bin", "mpr-17933.bin",
        "kick34005.A500.rom",
    ] {
        assert!(is_bios_name(name), "{name}");
    }
    for name in ["Tetris.gb", "scph.gba", "kickoff.zip", "mpr-17933.zip", "biosphere.gba"] {
        assert!(!is_bios_name(name), "{name}");
    }
}

#[test]
fn the_scan_skips_partial_downloads_and_streams() {
    let opts = ScanOptions::default();
    assert!(is_skipped_entry("Tetris.zip.crdownload", &opts));
    assert!(is_skipped_entry("Tetris.zip:Zone.Identifier", &opts));
    assert!(!is_skipped_entry("Tetris.zip", &opts));
}
```

- [ ] **Step 2: Run to see them fail**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize && cargo test -p coverly-core --lib scan::classify`
Expected: compile errors (`is_partial_download` not found).

- [ ] **Step 3: Implement** (in `classify.rs`, below `JUNK_FILES`)

```rust
/// Suffixes of files a browser or download manager is still writing
/// (Chrome, Firefox, Safari, Opera, µTorrent, qBittorrent, aria2, WinSCP).
pub(crate) const PARTIAL_DOWNLOAD_EXTS: [&str; 10] = [
    "crdownload", "part", "partial", "download", "opdownload", "tmp", "!ut", "!qb", "aria2",
    "filepart",
];

/// Extensions that follow a ROM with the same stem when it moves
/// (default of `settings.organize.companion_exts`; `state*` matches
/// `state`, `state1`, `state.auto`…).
pub(crate) const COMPANION_EXTS: [&str; 12] = [
    "sav", "srm", "rtc", "state*", "cht", "ips", "bps", "ups", "txt", "nfo", "png", "jpg",
];

/// BIOS and firmware files, by exact lowercase name.
const BIOS_NAMES: [&str; 18] = [
    "gba_bios.bin", "gb_bios.bin", "gbc_bios.bin", "sgb_bios.bin", "bios7.bin", "bios9.bin",
    "firmware.bin", "neogeo.zip", "disksys.rom", "dc_boot.bin", "dc_flash.bin", "sega_101.bin",
    "lynxboot.img", "5200.rom", "7800 bios (u).rom", "bios.min", "pgm.zip", "naomi.zip",
];

/// BIOS families by lowercase prefix and the extension they must have.
const BIOS_PREFIXES: [(&str, &str); 5] = [
    ("scph", "bin"),
    ("syscard", "pce"),
    ("bios_cd_", "bin"),
    ("mpr-", "bin"),
    ("kick", "rom"),
];

/// Whether `name` is a file a download is still writing.
pub(crate) fn is_partial_download(name: &str) -> bool {
    let lower = name.to_lowercase();
    PARTIAL_DOWNLOAD_EXTS
        .iter()
        .any(|ext| lower.len() > ext.len() + 1 && lower.ends_with(&format!(".{ext}")))
}

/// Windows `Zone.Identifier` alternate stream copied as a regular file
/// (through WSL or exFAT): `name.zip:Zone.Identifier`.
pub(crate) fn is_zone_identifier(name: &str) -> bool {
    name.to_lowercase().ends_with(":zone.identifier")
}

/// Whether `name` is a BIOS or firmware file (left in place by organise:
/// moving it into a game folder makes it a fake game).
pub(crate) fn is_bios_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    if BIOS_NAMES.contains(&lower.as_str()) {
        return true;
    }
    let ext = lower.rsplit_once('.').map_or("", |(_, e)| e);
    BIOS_PREFIXES
        .iter()
        .any(|(prefix, want)| lower.starts_with(prefix) && ext == *want)
}
```

Extend `is_skipped_entry`:

```rust
pub(crate) fn is_skipped_entry(name: &str, opts: &ScanOptions) -> bool {
    is_appledouble(name)
        || JUNK_FILES.iter().any(|j| name.eq_ignore_ascii_case(j))
        || is_partial_download(name)
        || is_zone_identifier(name)
        || (!opts.include_hidden && is_hidden(name))
}
```

In `scan/mod.rs` change `mod classify;` / `mod multifile;` to `pub(crate) mod classify;` / `pub(crate) mod multifile;`.

- [ ] **Step 4: Append an integration test** to `crates/coverly-core/tests/scan_fixtures.rs`

```rust
#[test]
fn partial_downloads_are_not_roms() {
    let tmp = tempfile::tempdir().unwrap();
    let gba = tmp.path().join("ROMS/GBA");
    std::fs::create_dir_all(&gba).unwrap();
    std::fs::write(gba.join("Done.gba"), b"x").unwrap();
    std::fs::write(gba.join("Half.gba.crdownload"), b"x").unwrap();
    let profile = coverly_core::profiles::builtin_profiles()
        .into_iter()
        .find(|p| p.id == "anbernic-stock")
        .unwrap();
    let scan = coverly_core::scan::scan_library(
        tmp.path(),
        &profile,
        &coverly_core::systems::SystemDb::builtin(),
        &coverly_core::scan::ScanOptions::default(),
    );
    let names: Vec<_> = scan.systems.iter().flat_map(|s| &s.roms).map(|r| r.stem.clone()).collect();
    assert_eq!(names, ["Done"]);
}
```

(Check the exact `scan_library` argument list at `scan/mod.rs:137` and match it.)

- [ ] **Step 5: Run the tests and all gates**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize && cargo test -p coverly-core scan && cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize && git add crates/coverly-core && git commit -m "feat(core): recognise partial downloads, BIOS files and zone streams"
```

---

### Task 2: Core — extension classes, systems by extension, system of a DAT

**Files:**
- Modify: `crates/coverly-core/src/systems/mod.rs`
- Test: `crates/coverly-core/src/systems/mod.rs` (unit)

**Interfaces:**
- Produces:
  ```rust
  pub const CONTAINER_EXTS: [&str; 2] = ["zip", "7z"];
  pub const GENERIC_EXTS: [&str; 6] = ["bin", "iso", "img", "rom", "dat", "chd"];
  #[derive(Clone, Copy, Debug, PartialEq, Eq)]
  pub enum ExtClass { Container, Generic, Unique, Shared, Unknown }
  impl SystemDb {
      pub fn systems_for_extension(&self, ext: &str) -> Vec<&System>; // lowercased, containers → empty
      pub fn extension_class(&self, ext: &str) -> ExtClass;
      pub fn system_for_dat(&self, dat: &str) -> Option<&System>;
      pub fn is_rom_like_extension(&self, ext: &str) -> bool;  // any system ext, container, generic, cue/gdi/m3u
  }
  ```

- [ ] **Step 1: Write the failing tests** (in `systems/mod.rs` tests)

```rust
#[test]
fn extension_classes_follow_the_database() {
    let db = SystemDb::builtin();
    assert_eq!(db.extension_class("GBA"), ExtClass::Unique);
    assert_eq!(db.extension_class("sfc"), ExtClass::Unique);
    assert_eq!(db.extension_class("gb"), ExtClass::Shared, "gb and gbc list it");
    assert_eq!(db.extension_class("wsc"), ExtClass::Shared);
    assert_eq!(db.extension_class("cue"), ExtClass::Shared);
    assert_eq!(db.extension_class("zip"), ExtClass::Container);
    assert_eq!(db.extension_class("7z"), ExtClass::Container);
    for generic in ["bin", "iso", "img", "rom", "dat", "chd"] {
        assert_eq!(db.extension_class(generic), ExtClass::Generic, "{generic}");
    }
    assert_eq!(db.extension_class("docx"), ExtClass::Unknown);
    assert_eq!(db.extension_class(""), ExtClass::Unknown);
}

#[test]
fn systems_for_an_extension() {
    let db = SystemDb::builtin();
    let ids = |ext: &str| -> Vec<String> {
        db.systems_for_extension(ext).iter().map(|s| s.id.clone()).collect()
    };
    assert_eq!(ids("gba"), ["gba"]);
    let mut gb = ids("gb");
    gb.sort();
    assert_eq!(gb, ["gb", "gbc"]);
    assert!(ids("zip").is_empty(), "containers name no system");
}

#[test]
fn the_system_of_a_dat() {
    let db = SystemDb::builtin();
    let gba = db.get("gba").unwrap();
    let dat = crate::identify::dat::dat_sources(gba).into_iter().next().unwrap();
    assert_eq!(db.system_for_dat(&dat).map(|s| s.id.as_str()), Some("gba"));
    assert!(db.system_for_dat("metadat/no-intro/Nope.dat").is_none());
}

#[test]
fn rom_like_extensions() {
    let db = SystemDb::builtin();
    for ext in ["gba", "zip", "7z", "bin", "cue", "m3u", "gdi", "chd", "NDS"] {
        assert!(db.is_rom_like_extension(ext), "{ext}");
    }
    for ext in ["txt", "jpg", "docx", "dmg", ""] {
        assert!(!db.is_rom_like_extension(ext), "{ext}");
    }
}
```

- [ ] **Step 2: Run to see them fail**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize && cargo test -p coverly-core --lib systems`
Expected: compile errors.

- [ ] **Step 3: Implement** (after `impl SystemDb`'s `all`)

```rust
/// Extensions that wrap a ROM: never evidence by themselves.
pub const CONTAINER_EXTS: [&str; 2] = ["zip", "7z"];
/// Extensions many non-ROM files use: no system candidate without
/// content evidence (spec §6.4).
pub const GENERIC_EXTS: [&str; 6] = ["bin", "iso", "img", "rom", "dat", "chd"];
/// Disc/list formats that are ROM-like even when no system lists them.
const LIST_EXTS: [&str; 3] = ["cue", "gdi", "m3u"];

/// How much an extension says about the system (spec §6.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExtClass {
    /// `zip`, `7z`: look inside.
    Container,
    /// `bin`, `iso`, `img`, `rom`, `dat`, `chd`: says nothing alone.
    Generic,
    /// Listed by exactly one system.
    Unique,
    /// Listed by several systems.
    Shared,
    /// Listed by none.
    Unknown,
}

impl SystemDb {
    /// Systems listing `ext` (any case, no dot); empty for containers.
    pub fn systems_for_extension(&self, ext: &str) -> Vec<&System> {
        let ext = ext.trim().to_lowercase();
        if ext.is_empty() || CONTAINER_EXTS.contains(&ext.as_str()) {
            return Vec::new();
        }
        self.systems
            .iter()
            .filter(|s| s.extensions.iter().any(|e| *e == ext))
            .collect()
    }

    /// What `ext` says about the system.
    pub fn extension_class(&self, ext: &str) -> ExtClass {
        let lower = ext.trim().to_lowercase();
        if CONTAINER_EXTS.contains(&lower.as_str()) {
            return ExtClass::Container;
        }
        if GENERIC_EXTS.contains(&lower.as_str()) {
            return ExtClass::Generic;
        }
        match self.systems_for_extension(&lower).len() {
            0 => ExtClass::Unknown,
            1 => ExtClass::Unique,
            _ => ExtClass::Shared,
        }
    }

    /// The system whose DAT list (see `dat_sources`) contains `dat`.
    pub fn system_for_dat(&self, dat: &str) -> Option<&System> {
        self.systems
            .iter()
            .find(|s| crate::identify::dat::dat_sources(s).iter().any(|d| d == dat))
    }

    /// Whether a file with extension `ext` may be a ROM, a disc image or
    /// an archive of one (organise candidates).
    pub fn is_rom_like_extension(&self, ext: &str) -> bool {
        let lower = ext.trim().to_lowercase();
        !lower.is_empty()
            && (LIST_EXTS.contains(&lower.as_str())
                || self.extension_class(&lower) != ExtClass::Unknown)
    }
}
```

- [ ] **Step 4: Run tests and gates** — `cargo test -p coverly-core --lib systems` PASS, then the Rust gates.

- [ ] **Step 5: Commit**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize && git add crates/coverly-core/src/systems && git commit -m "feat(core): classify extensions and map DATs back to systems"
```

---

### Task 3: Core — folder naming per profile (`FolderNaming`, host folders, arcade order, device folders)

**Files:**
- Modify: `crates/coverly-core/src/profiles/mod.rs` (fields, `pub mod naming;`, re-exports)
- Create: `crates/coverly-core/src/profiles/naming.rs`
- Modify: `crates/coverly-core/src/profiles/builtin.rs` (`profile()` helper, Anbernic family settings)
- Modify: `crates/coverly-core/src/profiles/custom.rs` (`ProfileFile.system_folder`, `folder_names`; `to_profile` / `from_profile`)
- Modify every `Profile { … }` literal outside `builtin.rs` (find with `grep -rn "fixed_format:" crates src-tauri`; add `..Default`-style fields explicitly)
- Test: `crates/coverly-core/tests/profile_naming.rs`

**Interfaces:**
- Produces:
  ```rust
  #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
  #[serde(rename_all = "kebab-case")]
  pub enum FolderNaming { #[default] FirstAlias, Id }
  // Profile gains:
  pub system_folder: FolderNaming,
  pub host_folders: BTreeMap<String, String>,   // system id → host system id (ngpc → ngp)
  pub arcade_order: Vec<String>,                 // system ids, preferred first
  pub known_folders: Vec<String>,                // folders the device shows; empty = unknown
  // profiles::naming:
  pub struct FolderChoice { pub folder: String, pub host: Option<String>, pub on_device: Option<bool> }
  pub fn system_folder(profile: &Profile, system: &System, db: &SystemDb,
                       overrides: &BTreeMap<String, String>) -> FolderChoice;
  pub fn validate_folder_override(db: &SystemDb, profile: &Profile, system_id: &str, folder: &str)
      -> Result<(), String>;
  ```

- [ ] **Step 1: Write the failing tests** — `crates/coverly-core/tests/profile_naming.rs`

```rust
//! Folder names organise creates, per layout (spec §8.1).

use std::collections::BTreeMap;

use coverly_core::profiles::naming::{system_folder, validate_folder_override};
use coverly_core::profiles::{builtin_profiles, FolderNaming, Profile};
use coverly_core::systems::SystemDb;

fn profile(id: &str) -> Profile {
    builtin_profiles().into_iter().find(|p| p.id == id).unwrap()
}

fn folder(profile_id: &str, system_id: &str) -> String {
    let db = SystemDb::builtin();
    let p = profile(profile_id);
    system_folder(&p, db.get(system_id).unwrap(), &db, &BTreeMap::new()).folder
}

#[test]
fn anbernic_family_uses_the_sd_card_names() {
    for id in ["anbernic-stock", "garlicos", "onion", "minui"] {
        assert_eq!(profile(id).system_folder, FolderNaming::FirstAlias, "{id}");
    }
    assert_eq!(folder("anbernic-stock", "gba"), "GBA");
    assert_eq!(folder("anbernic-stock", "snes"), "SFC");
    assert_eq!(folder("anbernic-stock", "nes"), "FC");
    assert_eq!(folder("anbernic-stock", "megadrive"), "MD");
    assert_eq!(folder("anbernic-stock", "psx"), "PS");
}

#[test]
fn frontends_use_ids() {
    for id in ["esde", "batocera", "muos", "retroarch"] {
        assert_eq!(profile(id).system_folder, FolderNaming::Id, "{id}");
    }
    assert_eq!(folder("esde", "snes"), "snes");
    assert_eq!(folder("batocera", "megadrive"), "megadrive");
}

#[test]
fn colour_handhelds_share_their_host_folder_on_anbernic() {
    let db = SystemDb::builtin();
    let p = profile("anbernic-stock");
    let ngpc = system_folder(&p, db.get("ngpc").unwrap(), &db, &BTreeMap::new());
    assert_eq!(ngpc.folder, "NGP");
    assert_eq!(ngpc.host.as_deref(), Some("ngp"));
    assert_eq!(folder("anbernic-stock", "wonderswancolor"), "WS");
    assert_eq!(folder("esde", "ngpc"), "ngpc", "frontends keep their own folder");
}

#[test]
fn every_profile_and_system_folder_resolves_back() {
    let db = SystemDb::builtin();
    for p in builtin_profiles() {
        for s in db.all() {
            let choice = system_folder(&p, s, &db, &BTreeMap::new());
            let back = db.resolve_folder(&choice.folder).map(|r| r.id.clone());
            let want = choice.host.clone().unwrap_or_else(|| s.id.clone());
            assert_eq!(back, Some(want), "{} × {} → {}", p.id, s.id, choice.folder);
        }
    }
}

#[test]
fn user_overrides_win_and_must_resolve() {
    let db = SystemDb::builtin();
    let p = profile("minui");
    let mut map = BTreeMap::new();
    map.insert("gba".to_string(), "gameboyadvance".to_string());
    assert_eq!(system_folder(&p, db.get("gba").unwrap(), &db, &map).folder, "gameboyadvance");
    assert!(validate_folder_override(&db, &p, "gba", "gameboyadvance").is_ok());
    assert!(validate_folder_override(&db, &p, "gba", "Advance Stuff").is_err());
    assert!(validate_folder_override(&db, &p, "gba", "SFC").is_err(), "resolves to snes");
}

#[test]
fn a_system_the_device_does_not_show_is_flagged() {
    let db = SystemDb::builtin();
    let p = profile("anbernic-stock");
    assert_eq!(system_folder(&p, db.get("gba").unwrap(), &db, &BTreeMap::new()).on_device, Some(true));
    let mut map = BTreeMap::new();
    map.insert("gba".to_string(), "gameboyadvance".to_string());
    assert_eq!(system_folder(&p, db.get("gba").unwrap(), &db, &map).on_device, Some(false));
    let esde = profile("esde");
    assert_eq!(system_folder(&esde, db.get("gba").unwrap(), &db, &BTreeMap::new()).on_device, None);
}

#[test]
fn arcade_order_defaults() {
    assert_eq!(profile("anbernic-stock").arcade_order[..2], ["fbneo".to_string(), "mame".to_string()]);
    assert_eq!(profile("esde").arcade_order, ["fbneo", "mame"]);
}
```

- [ ] **Step 2: Run to see them fail**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize && cargo test -p coverly-core --test profile_naming`
Expected: compile errors.

- [ ] **Step 3: Implement**

`profiles/mod.rs`: add `pub mod naming;`, `use std::collections::BTreeMap;`, the `FolderNaming` enum above and the four `Profile` fields with doc comments:

```rust
    /// How organise names system folders (spec §8.1).
    pub system_folder: FolderNaming,
    /// Systems the device keeps in another system's folder (Anbernic:
    /// `ngpc → ngp`, `wonderswancolor → wonderswan`).
    pub host_folders: BTreeMap<String, String>,
    /// Arcade systems, preferred first, when a set is in several DATs.
    pub arcade_order: Vec<String>,
    /// Folders the device shows, when known (empty = unknown).
    pub known_folders: Vec<String>,
```

`builtin.rs`: `profile()` sets `system_folder: FolderNaming::Id`, `host_folders: BTreeMap::new()`, `arcade_order: vec!["fbneo".into(), "mame".into()]`, `known_folders: Vec::new()`. `imgs_profile()` and the MinUI profile switch to `FirstAlias`; `imgs_profile` also sets:

```rust
const ANBERNIC_HOSTS: [(&str, &str); 2] = [("ngpc", "ngp"), ("wonderswancolor", "wonderswan")];
const ANBERNIC_ARCADE: [&str; 6] = ["fbneo", "mame", "cps1", "cps2", "cps3", "neogeo"];
/// Folder names of the stock Anbernic card (reference `~/Documents/ROMS`).
const ANBERNIC_FOLDERS: [&str; 55] = [
    "A2600", "A5200", "A7800", "A800", "AMIGA", "APPS", "ATARIST", "ATOMISWAVE", "C64", "CPS1",
    "CPS2", "CPS3", "DOS", "DREAMCAST", "EASYRPG", "FBNEO", "FC", "FDS", "GB", "GBA", "GBC", "GG",
    "GW", "HBMAME", "JAVA", "LYNX", "MAME", "MD", "MDCD", "MSX", "N64", "NAOMI", "NDS", "NEOCD",
    "NEOGEO", "NGP", "ONS", "OPENBOR", "PCE", "PCECD", "PGM2", "PICO", "POKE", "PORTS", "PS",
    "PSP", "SATURN", "SCUMMVM", "SEGA32X", "SFC", "SMS", "VARCADE", "VB", "VIC20", "WS",
];
```

(The list is exactly `ls /Users/miguelangelesparzacalero/Documents/ROMS` as of 2026-10-17, read-only.) `known_folders` is set **only** for `anbernic-stock`; `garlicos`/`onion` get the hosts and arcade order but no folder list (O-D10).

`naming.rs`:

```rust
//! Folder names organise creates for each system (spec §8.1).

use std::collections::BTreeMap;

use super::{FolderNaming, Profile};
use crate::systems::{System, SystemDb};

/// The folder a system's ROMs go to under a layout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FolderChoice {
    /// Folder name, e.g. `GBA`, `snes`.
    pub folder: String,
    /// The system that owns the folder when it is shared (`ngp` for NGPC on
    /// Anbernic); `None` when the folder is the system's own.
    pub host: Option<String>,
    /// Whether the device shows this folder; `None` when the layout does
    /// not list its folders.
    pub on_device: Option<bool>,
}

/// The folder for `system` under `profile`, honouring `overrides`
/// (`settings.organize.folder_names[profile.id]`).
pub fn system_folder(
    profile: &Profile,
    system: &System,
    db: &SystemDb,
    overrides: &BTreeMap<String, String>,
) -> FolderChoice {
    let host = profile
        .host_folders
        .get(&system.id)
        .and_then(|h| db.get(h))
        .filter(|h| h.id != system.id);
    let owner = host.unwrap_or(system);
    let folder = overrides
        .get(&system.id)
        .cloned()
        .unwrap_or_else(|| default_name(profile.system_folder, owner, db));
    let on_device = (!profile.known_folders.is_empty()).then(|| {
        profile
            .known_folders
            .iter()
            .any(|k| k.eq_ignore_ascii_case(&folder))
    });
    FolderChoice {
        folder,
        host: host.map(|h| h.id.clone()),
        on_device,
    }
}

/// `Id` falls back to the first alias when the id itself is not an alias
/// (the folder must resolve back, or the scan would not see it).
fn default_name(rule: FolderNaming, system: &System, db: &SystemDb) -> String {
    let first = system.aliases.first().cloned().unwrap_or_else(|| system.id.clone());
    match rule {
        FolderNaming::FirstAlias => first,
        FolderNaming::Id => {
            let resolves = db.resolve_folder(&system.id).is_some_and(|s| s.id == system.id);
            if resolves { system.id.clone() } else { first }
        }
    }
}

/// A user folder name for `system_id` must resolve back to that system or
/// to its host under `profile`.
pub fn validate_folder_override(
    db: &SystemDb,
    profile: &Profile,
    system_id: &str,
    folder: &str,
) -> Result<(), String> {
    let wanted = profile.host_folders.get(system_id).map_or(system_id, String::as_str);
    match db.resolve_folder(folder.trim()) {
        Some(s) if s.id == wanted => Ok(()),
        Some(s) => Err(format!("`{folder}` is the folder of {} ({})", s.name, s.id)),
        None => Err(format!("`{folder}` is not a known system folder name")),
    }
}
```

`custom.rs`: `ProfileFile` gains

```rust
    /// How organise names system folders (`first-alias` | `id`).
    #[serde(default)]
    pub system_folder: FolderNaming,
    /// Per-system folder names (system id → folder).
    #[serde(default)]
    pub folder_names: BTreeMap<String, String>,
```

`to_profile` copies `system_folder`, sets `host_folders` empty, `arcade_order` `["fbneo","mame"]`, `known_folders` empty; the custom `folder_names` are merged under the user settings map by the planner (Task 15). `from_profile` copies `system_folder`. `to_toml` writes `system_folder` only when not the default. Add `system_folder`/`folder_names` to `validate_profile`: every `folder_names` entry passes `validate_folder_override` against `SystemDb::builtin()` (field error code `invalid`, path `folder_names.<id>`).

- [ ] **Step 4: Fix the remaining literals**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize && grep -rn "fixed_format:" crates src-tauri` and add the four fields to every literal found (tests use `FolderNaming::default()`, empty maps/vecs).

- [ ] **Step 5: Run tests and gates**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize && cargo test -p coverly-core --test profile_naming --test custom_profiles && cargo test --workspace`
Expected: PASS. If `every_profile_and_system_folder_resolves_back` names a system whose first alias does not resolve (a user-overridden alias list), fix the data in `systems.toml`, not the test.

- [ ] **Step 6: Commit**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize && git add crates src-tauri && git commit -m "feat(core): name system folders per layout profile"
```

---

## Group O2 — Header magic

### Task 4: Core — `Evidence`, `ReadAt`, `Sniff`, text/web pages, NES/FDS/N64/Lynx/A7800, containers

**Files:**
- Create: `crates/coverly-core/src/identify/evidence.rs`
- Create: `crates/coverly-core/src/identify/sniff.rs`
- Modify: `crates/coverly-core/src/identify/mod.rs` (`pub mod evidence; pub mod sniff;`)
- Modify: `crates/coverly-core/Cargo.toml` (`[features] test-util` also exposes `sniff::fixtures`)
- Test: `crates/coverly-core/tests/sniff.rs`

**Interfaces:**
- Produces:
  ```rust
  // identify::evidence
  #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
  #[serde(rename_all = "kebab-case")]
  pub enum IdLevel { Hash, Header, Extension, Name, Online, User }
  #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
  pub struct Evidence { pub kind: IdLevel, pub system: String, pub weight: f32, pub detail: String }
  impl Evidence { pub fn new(kind: IdLevel, system: &str, weight: f32, detail: impl Into<String>) -> Self }
  // identify::sniff
  pub const PREFIX: usize = 128 * 1024;
  pub const STREAM_CAP: u64 = 8 * 1024 * 1024;
  pub trait ReadAt { fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> io::Result<usize>; }
  pub struct SliceAt<'a>(pub &'a [u8]);
  pub struct FileAt(pub std::fs::File);
  pub struct StreamAt<R: Read> { .. }  // StreamAt::new(reader, cap)
  #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
  #[serde(rename_all = "kebab-case")]
  pub enum NotARom { WebPage, Text }
  #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
  #[serde(rename_all = "kebab-case")]
  pub enum ContainerMagic { Zip, SevenZip, Chd, Rar }
  #[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
  pub struct Sniff {
      pub hits: Vec<Evidence>,          // kind = Header
      pub excluded: Vec<String>,        // hard vetoes (e.g. "gb" for a GBC-only cart)
      pub unsupported: Option<String>,  // "Sega Pico", "PlayStation 2"
      pub not_a_rom: Option<NotARom>,
      pub container: Option<ContainerMagic>,
      pub read: usize,                  // bytes of prefix actually read (0 = unreadable/empty)
  }
  pub fn sniff(src: &mut dyn ReadAt, len: u64, ext: &str) -> Sniff;
  pub const DECISIVE: [&str; 8] = ["gb", "gbc", "gba", "nds", "n64", "nes", "fds", "sega32x"];
  ```

- [ ] **Step 1: Write the failing tests** — `crates/coverly-core/tests/sniff.rs`

```rust
//! Header magic per system (spec §6.3). Fixtures hold only the bytes a
//! check reads; everything else is zero.

use coverly_core::identify::sniff::fixtures as fx;
use coverly_core::identify::sniff::{sniff, ContainerMagic, NotARom, SliceAt, Sniff};

fn run(bytes: &[u8], ext: &str) -> Sniff {
    sniff(&mut SliceAt(bytes), bytes.len() as u64, ext)
}

fn best(s: &Sniff) -> Option<(String, f32)> {
    s.hits
        .iter()
        .max_by(|a, b| a.weight.total_cmp(&b.weight))
        .map(|e| (e.system.clone(), e.weight))
}

#[test]
fn nes_ines_and_unif() {
    assert_eq!(best(&run(&fx::nes(), "nes")), Some(("nes".into(), 0.95)));
    assert_eq!(best(&run(&fx::unif(), "unf")), Some(("nes".into(), 0.95)));
    let mut near = fx::nes();
    near[3] = 0x1B;
    assert_eq!(best(&run(&near, "nes")), None);
}

#[test]
fn fds_header_and_headerless() {
    assert_eq!(best(&run(&fx::fds(true), "fds")), Some(("fds".into(), 0.95)));
    assert_eq!(best(&run(&fx::fds(false), "fds")), Some(("fds".into(), 0.95)));
}

#[test]
fn n64_in_every_byte_order() {
    for order in [fx::N64Order::Z64, fx::N64Order::V64, fx::N64Order::N64] {
        assert_eq!(best(&run(&fx::n64(order), "bin")), Some(("n64".into(), 0.95)));
    }
}

#[test]
fn lynx_and_7800() {
    assert_eq!(best(&run(&fx::lynx(), "lnx")), Some(("lynx".into(), 0.95)));
    assert_eq!(best(&run(&fx::a7800(), "a78")), Some(("a7800".into(), 0.95)));
}

#[test]
fn web_pages_and_text_are_not_roms() {
    let html = fx::html_page();
    let s = run(&html, "zip");
    assert_eq!(s.not_a_rom, Some(NotARom::WebPage));
    assert!(s.hits.is_empty());
    assert_eq!(run(b"# Notes\n\nSome markdown text.\n", "md").not_a_rom, Some(NotARom::Text));
    assert_eq!(run(b"{\"a\":1}", "bin").not_a_rom, Some(NotARom::WebPage));
    assert_eq!(run(&[0u8; 1024], "bin").not_a_rom, None, "zeros are not text");
}

#[test]
fn containers_are_recognised_not_read() {
    assert_eq!(run(b"PK\x03\x04rest", "zip").container, Some(ContainerMagic::Zip));
    assert_eq!(run(&[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C, 0, 4], "7z").container, Some(ContainerMagic::SevenZip));
    assert_eq!(run(b"MComprHD\0\0\0\x7c", "chd").container, Some(ContainerMagic::Chd));
    assert_eq!(run(b"Rar!\x1a\x07\x01\x00", "rar").container, Some(ContainerMagic::Rar));
}

#[test]
fn empty_and_short_inputs_never_panic() {
    for len in [0usize, 1, 3, 16, 0x103, 0x14F] {
        let s = run(&vec![0u8; len], "gb");
        assert!(s.hits.is_empty(), "len {len}");
    }
}
```

- [ ] **Step 2: Run to see them fail**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize && cargo test -p coverly-core --test sniff`
Expected: compile errors.

- [ ] **Step 3: Implement `evidence.rs`**

```rust
//! Pieces of evidence about which system a file belongs to (spec §6.1).

use serde::{Deserialize, Serialize};

/// Where a piece of evidence comes from, strongest first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IdLevel {
    /// CRC/SHA1 found in a DAT.
    Hash,
    /// Header magic.
    Header,
    /// File extension.
    Extension,
    /// Name, tags, fuzzy title.
    Name,
    /// Online lookup (ScreenScraper by CRC), only when configured.
    Online,
    /// Chosen by the user.
    User,
}

/// One clue: `system` with `weight` in 0..=1, `detail` is data for the
/// UI (a CRC and game name, `CGB 0xC0`, an extension) — never a sentence.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Evidence {
    pub kind: IdLevel,
    pub system: String,
    pub weight: f32,
    pub detail: String,
}

impl Evidence {
    pub fn new(kind: IdLevel, system: &str, weight: f32, detail: impl Into<String>) -> Evidence {
        Evidence { kind, system: system.to_string(), weight, detail: detail.into() }
    }
}
```

- [ ] **Step 4: Implement the `sniff.rs` skeleton and the first checks**

```rust
//! Header magic per system (spec §6.3): pure checks over bounded reads.
//! Reads a 128 KiB prefix plus a few targeted offsets; never the whole
//! file. Checks run most specific first inside a family.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};

use serde::{Deserialize, Serialize};

use super::evidence::{Evidence, IdLevel};

/// Bytes read from the start of every candidate.
pub const PREFIX: usize = 128 * 1024;
/// Largest offset reachable by stream-skipping a deflated zip entry.
pub const STREAM_CAP: u64 = 8 * 1024 * 1024;
/// Systems every real dump of which carries the checked signature: a
/// failed sniff is negative evidence for them (O-D5).
pub const DECISIVE: [&str; 8] = ["gb", "gbc", "gba", "nds", "n64", "nes", "fds", "sega32x"];

/// Bounded reads at given offsets.
pub trait ReadAt {
    /// Reads up to `buf.len()` bytes at `offset`; fewer at the end.
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> io::Result<usize>;
}

/// In-memory bytes (tests, already-read prefixes).
pub struct SliceAt<'a>(pub &'a [u8]);

impl ReadAt for SliceAt<'_> {
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> io::Result<usize> {
        let Ok(start) = usize::try_from(offset) else { return Ok(0) };
        if start >= self.0.len() {
            return Ok(0);
        }
        let n = buf.len().min(self.0.len() - start);
        buf[..n].copy_from_slice(&self.0[start..start + n]);
        Ok(n)
    }
}

/// A raw file: seeks.
pub struct FileAt(pub File);

impl ReadAt for FileAt {
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> io::Result<usize> {
        self.0.seek(SeekFrom::Start(offset))?;
        let mut n = 0;
        while n < buf.len() {
            match self.0.read(&mut buf[n..])? {
                0 => break,
                k => n += k,
            }
        }
        Ok(n)
    }
}

/// A forward-only stream (deflated zip entry): bytes are kept as they are
/// inflated, up to `cap`; offsets beyond the cap read as end of file.
pub struct StreamAt<R: Read> {
    reader: R,
    seen: Vec<u8>,
    cap: u64,
    eof: bool,
}

impl<R: Read> StreamAt<R> {
    pub fn new(reader: R, cap: u64) -> StreamAt<R> {
        StreamAt { reader, seen: Vec::new(), cap, eof: false }
    }

    fn fill_to(&mut self, end: u64) -> io::Result<()> {
        let end = end.min(self.cap);
        let mut chunk = [0u8; 64 * 1024];
        while !self.eof && (self.seen.len() as u64) < end {
            let want = ((end - self.seen.len() as u64) as usize).min(chunk.len());
            match self.reader.read(&mut chunk[..want])? {
                0 => self.eof = true,
                n => self.seen.extend_from_slice(&chunk[..n]),
            }
        }
        Ok(())
    }
}

impl<R: Read> ReadAt for StreamAt<R> {
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> io::Result<usize> {
        self.fill_to(offset.saturating_add(buf.len() as u64))?;
        SliceAt(&self.seen).read_at(offset, buf)
    }
}

/// What a file says it is not.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NotARom {
    WebPage,
    Text,
}

/// Container formats recognised by their magic (contents not read here).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ContainerMagic {
    Zip,
    SevenZip,
    Chd,
    Rar,
}

/// Result of [`sniff`].
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Sniff {
    pub hits: Vec<Evidence>,
    pub excluded: Vec<String>,
    pub unsupported: Option<String>,
    pub not_a_rom: Option<NotARom>,
    pub container: Option<ContainerMagic>,
    pub read: usize,
}

impl Sniff {
    fn hit(&mut self, system: &str, weight: f32, detail: impl Into<String>) {
        self.hits.push(Evidence::new(IdLevel::Header, system, weight, detail));
    }
}

/// Reads at `offset` until `buf` is full or the source ends.
fn read_exact_at(src: &mut dyn ReadAt, offset: u64, buf: &mut [u8]) -> usize {
    src.read_at(offset, buf).unwrap_or(0)
}

/// `n` bytes at `offset` of `head`, if present.
fn at<'a>(head: &'a [u8], offset: usize, n: usize) -> Option<&'a [u8]> {
    head.get(offset..offset.checked_add(n)?)
}

fn starts(head: &[u8], offset: usize, magic: &[u8]) -> bool {
    at(head, offset, magic.len()) == Some(magic)
}

/// Header magic of the file behind `src` (`len` bytes, extension `ext`).
pub fn sniff(src: &mut dyn ReadAt, len: u64, ext: &str) -> Sniff {
    let ext = ext.to_ascii_lowercase();
    let mut head = vec![0u8; PREFIX.min(usize::try_from(len).unwrap_or(PREFIX))];
    let n = read_exact_at(src, 0, &mut head);
    head.truncate(n);
    let mut s = Sniff { read: n, ..Sniff::default() };
    if n == 0 {
        return s;
    }
    if let Some(kind) = text_kind(&head) {
        s.not_a_rom = Some(kind);
        return s;
    }
    if let Some(c) = container(&head) {
        s.container = Some(c);
        return s;
    }
    // Disc images first (sector 16), then cartridges, most specific first.
    discs(src, &head, len, &mut s);
    if s.hits.is_empty() && s.unsupported.is_none() {
        cartridges(src, &head, len, &ext, &mut s);
    }
    s
}

/// Printable text (no NUL/control bytes but tab/CR/LF in the first 512
/// bytes, valid UTF-8): a web page when it starts like one.
fn text_kind(head: &[u8]) -> Option<NotARom> {
    let probe = &head[..head.len().min(512)];
    let text = match std::str::from_utf8(probe) {
        Ok(t) => t,
        // A multi-byte character cut at byte 512 is still text.
        Err(e) if e.error_len().is_none() => std::str::from_utf8(&probe[..e.valid_up_to()]).ok()?,
        Err(_) => return None,
    };
    if text.is_empty() || text.chars().any(|c| c.is_control() && !matches!(c, '\t' | '\r' | '\n')) {
        return None;
    }
    let start = text.trim_start_matches('\u{feff}').trim_start().to_ascii_lowercase();
    let web = ["<!doctype", "<html", "<?xml", "<head", "<body", "{", "["];
    Some(if web.iter().any(|w| start.starts_with(w)) { NotARom::WebPage } else { NotARom::Text })
}

fn container(head: &[u8]) -> Option<ContainerMagic> {
    if starts(head, 0, b"PK\x03\x04") || starts(head, 0, b"PK\x05\x06") {
        Some(ContainerMagic::Zip)
    } else if starts(head, 0, &[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C]) {
        Some(ContainerMagic::SevenZip)
    } else if starts(head, 0, b"MComprHD") {
        Some(ContainerMagic::Chd)
    } else if starts(head, 0, b"Rar!\x1a\x07") {
        Some(ContainerMagic::Rar)
    } else {
        None
    }
}

/// Disc images (Task 8 fills this in).
fn discs(_src: &mut dyn ReadAt, _head: &[u8], _len: u64, _s: &mut Sniff) {}

fn cartridges(src: &mut dyn ReadAt, head: &[u8], len: u64, ext: &str, s: &mut Sniff) {
    if starts(head, 0, b"FDS\x1a")
        || starts(head, 0, b"\x01*NINTENDO-HVC*")
        || starts(head, 16, b"\x01*NINTENDO-HVC*")
    {
        s.hit("fds", 0.95, "FDS");
        return;
    }
    if starts(head, 0, b"NES\x1a") {
        s.hit("nes", 0.95, "iNES");
        return;
    }
    if starts(head, 0, b"UNIF") {
        s.hit("nes", 0.95, "UNIF");
        return;
    }
    for (magic, order) in [
        ([0x80, 0x37, 0x12, 0x40], "z64"),
        ([0x37, 0x80, 0x40, 0x12], "v64"),
        ([0x40, 0x12, 0x37, 0x80], "n64"),
    ] {
        if starts(head, 0, &magic) {
            s.hit("n64", 0.95, order);
            return;
        }
    }
    if starts(head, 0, b"LYNX") {
        s.hit("lynx", 0.95, "LYNX");
        return;
    }
    if starts(head, 1, b"ATARI7800") {
        s.hit("a7800", 0.95, "ATARI7800");
        return;
    }
    // Tasks 5–7 add Nintendo handhelds, Sega, SNES, SNK and Bandai here.
    let _ = (src, len, ext);
}
```

Fixtures (bottom of `sniff.rs`):

```rust
/// Synthetic headers for tests: only the bytes the checks read.
#[cfg(any(test, feature = "test-util"))]
pub mod fixtures {
    pub fn zeros(len: usize) -> Vec<u8> { vec![0u8; len] }

    pub fn nes() -> Vec<u8> { let mut b = zeros(16 + 16384); b[..4].copy_from_slice(b"NES\x1a"); b }
    pub fn unif() -> Vec<u8> { let mut b = zeros(64); b[..4].copy_from_slice(b"UNIF"); b }
    pub fn fds(with_header: bool) -> Vec<u8> {
        let mut b = zeros(65500 + 16);
        if with_header { b[..4].copy_from_slice(b"FDS\x1a"); b[16..31].copy_from_slice(b"\x01*NINTENDO-HVC*"); }
        else { b[..15].copy_from_slice(b"\x01*NINTENDO-HVC*"); }
        b
    }
    #[derive(Clone, Copy)]
    pub enum N64Order { Z64, V64, N64 }
    pub fn n64(order: N64Order) -> Vec<u8> {
        let mut b = zeros(4096);
        b[..4].copy_from_slice(match order {
            N64Order::Z64 => &[0x80, 0x37, 0x12, 0x40],
            N64Order::V64 => &[0x37, 0x80, 0x40, 0x12],
            N64Order::N64 => &[0x40, 0x12, 0x37, 0x80],
        });
        b
    }
    pub fn lynx() -> Vec<u8> { let mut b = zeros(1024); b[..4].copy_from_slice(b"LYNX"); b }
    pub fn a7800() -> Vec<u8> { let mut b = zeros(1024); b[1..10].copy_from_slice(b"ATARI7800"); b }
    pub fn html_page() -> Vec<u8> {
        b"<!DOCTYPE html>\n<html><head><title>Download</title></head><body>Not found</body></html>\n".to_vec()
    }
}
```

`Cargo.toml` needs nothing new (`test-util` already exists); the `cfg(any(test, feature = "test-util"))` makes `fixtures` reachable from integration tests through the dev-dependency on itself with `test-util`.

- [ ] **Step 5: Run tests and gates** — `cargo test -p coverly-core --test sniff` PASS; Rust gates.

- [ ] **Step 6: Commit**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize && git add crates/coverly-core && git commit -m "feat(core): sniff header magic with bounded reads (NES, FDS, N64, Lynx, 7800)"
```

---

### Task 5: Core — sniff Game Boy / Game Boy Color, Game Boy Advance, Nintendo DS

**Files:**
- Modify: `crates/coverly-core/src/identify/sniff.rs` (`cartridges`, fixtures)
- Test: `crates/coverly-core/tests/sniff.rs` (append)

**Interfaces:**
- Produces: `fixtures::{gb(cgb: u8), gba(), nds()}`; `fn gb_header_checksum(head) -> u8`, `fn gba_complement(head) -> u8`, `pub(crate) fn crc16_modbus(data: &[u8]) -> u16` (private helpers in `sniff.rs`, the CRC one also used by fixtures).

- [ ] **Step 1: Write the failing tests** (append to `tests/sniff.rs`)

```rust
#[test]
fn game_boy_by_cgb_flag() {
    assert_eq!(best(&run(&fx::gb(0x00), "gb")), Some(("gb".into(), 0.95)));
    let gbc_only = run(&fx::gb(0xC0), "gb");
    assert_eq!(best(&gbc_only), Some(("gbc".into(), 0.95)));
    assert_eq!(gbc_only.excluded, ["gb"], "a GBC-only cart never goes to GB");
    let dual = run(&fx::gb(0x80), "gb");
    assert_eq!(best(&dual), Some(("gbc".into(), 0.85)));
    assert!(dual.excluded.is_empty());
}

#[test]
fn game_boy_needs_the_header_checksum() {
    let mut bad = fx::gb(0x00);
    bad[0x14D] ^= 0xFF;
    assert_eq!(best(&run(&bad, "gb")), None);
    let mut no_logo = fx::gb(0x00);
    no_logo[0x104] = 0;
    assert_eq!(best(&run(&no_logo, "gb")), None);
}

#[test]
fn gba_by_fixed_byte_and_complement() {
    assert_eq!(best(&run(&fx::gba(), "gba")), Some(("gba".into(), 0.95)));
    let mut bad = fx::gba();
    bad[0xBD] ^= 0x01;
    assert_eq!(best(&run(&bad, "gba")), None, "homebrew without gbafix");
    let mut no_fixed = fx::gba();
    no_fixed[0xB2] = 0;
    assert_eq!(best(&run(&no_fixed, "gba")), None);
}

#[test]
fn nds_by_two_crcs_and_never_as_gba() {
    let nds = fx::nds();
    assert_eq!(best(&run(&nds, "nds")), Some(("nds".into(), 0.95)));
    let mut hybrid = fx::nds();
    // An NDS image that also looks like a GBA header stays NDS.
    hybrid[0xB2] = 0x96;
    assert_eq!(best(&run(&hybrid, "nds")).unwrap().0, "nds");
    let mut bad = fx::nds();
    bad[0x15E] ^= 0xFF;
    assert_eq!(best(&run(&bad, "nds")), None);
}
```

(NDS before GBA: the hybrid test changes a byte covered by the NDS header CRC, so the fixture helper `fx::nds_with(|b| b[0xB2] = 0x96)` must recompute the CRC; write `nds_with(edit: impl FnOnce(&mut [u8])) -> Vec<u8>` and use it here.)

- [ ] **Step 2: Run to see them fail** — `cargo test -p coverly-core --test sniff` → FAIL (`fx::gb` missing).

- [ ] **Step 3: Implement** (insert at the top of `cartridges`, before FDS)

```rust
    // NDS before GBA: NDS cartridges embed a GBA-style area.
    if let Some(h) = at(head, 0, 0x160) {
        let logo_crc = u16::from_le_bytes([h[0x15C], h[0x15D]]);
        let header_crc = u16::from_le_bytes([h[0x15E], h[0x15F]]);
        if logo_crc == 0xCF56 && crc16_modbus(&h[..0x15E]) == header_crc {
            s.hit("nds", 0.95, "NDS header");
            return;
        }
    }
    if let Some(h) = at(head, 0, 0xC0) {
        if h[0xB2] == 0x96 && gba_complement(h) == h[0xBD] {
            let logo = if starts(h, 0x04, &[0x24, 0xFF, 0xAE, 0x51]) { "fixed 0x96, logo" } else { "fixed 0x96" };
            s.hit("gba", 0.95, logo);
            return;
        }
    }
    const GB_LOGO: [u8; 8] = [0xCE, 0xED, 0x66, 0x66, 0xCC, 0x0D, 0x00, 0x0B];
    if let Some(h) = at(head, 0, 0x150) {
        if h[0x104..0x10C] == GB_LOGO && gb_header_checksum(h) == h[0x14D] {
            match h[0x143] {
                0xC0 => {
                    s.hit("gbc", 0.95, "CGB 0xC0");
                    s.excluded.push("gb".into());
                }
                0x80 => s.hit("gbc", 0.85, "CGB 0x80"),
                _ => s.hit("gb", 0.95, "DMG"),
            }
            return;
        }
    }
```

Helpers:

```rust
/// Game Boy header checksum over 0x134..=0x14C.
fn gb_header_checksum(h: &[u8]) -> u8 {
    h[0x134..=0x14C].iter().fold(0u8, |x, b| x.wrapping_sub(*b).wrapping_sub(1))
}

/// GBA complement check over 0xA0..=0xBC.
fn gba_complement(h: &[u8]) -> u8 {
    h[0xA0..=0xBC].iter().fold(0u8, |c, b| c.wrapping_sub(*b)).wrapping_sub(0x19)
}

/// CRC-16/MODBUS (init 0xFFFF, reflected poly 0xA001), as the NDS header uses.
pub(crate) fn crc16_modbus(data: &[u8]) -> u16 {
    let mut crc = 0xFFFFu16;
    for &b in data {
        crc ^= u16::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xA001 } else { crc >> 1 };
        }
    }
    crc
}
```

Fixtures:

```rust
    pub fn gb(cgb: u8) -> Vec<u8> {
        let mut b = zeros(32 * 1024);
        b[0x104..0x10C].copy_from_slice(&[0xCE, 0xED, 0x66, 0x66, 0xCC, 0x0D, 0x00, 0x0B]);
        b[0x134..0x13C].copy_from_slice(b"SYNTHGB\0");
        b[0x143] = cgb;
        b[0x14D] = b[0x134..=0x14C].iter().fold(0u8, |x, v| x.wrapping_sub(*v).wrapping_sub(1));
        b
    }
    pub fn gba() -> Vec<u8> {
        let mut b = zeros(64 * 1024);
        b[0x04..0x08].copy_from_slice(&[0x24, 0xFF, 0xAE, 0x51]);
        b[0xA0..0xAC].copy_from_slice(b"SYNTHGBA\0\0\0\0");
        b[0xB2] = 0x96;
        b[0xBD] = b[0xA0..=0xBC].iter().fold(0u8, |c, v| c.wrapping_sub(*v)).wrapping_sub(0x19);
        b
    }
    pub fn nds_with(edit: impl FnOnce(&mut [u8])) -> Vec<u8> {
        let mut b = zeros(64 * 1024);
        b[..12].copy_from_slice(b"SYNTHNDS\0\0\0\0");
        b[0x15C..0x15E].copy_from_slice(&0xCF56u16.to_le_bytes());
        edit(&mut b);
        let crc = super::crc16_modbus(&b[..0x15E]);
        b[0x15E..0x160].copy_from_slice(&crc.to_le_bytes());
        b
    }
    pub fn nds() -> Vec<u8> { nds_with(|_| {}) }
```

- [ ] **Step 4: Run tests and gates** — PASS.

- [ ] **Step 5: Commit**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize && git add crates/coverly-core && git commit -m "feat(core): sniff Game Boy, Game Boy Color, GBA and DS headers"
```

---

### Task 6: Core — sniff the Sega family (Sega CD, 32X, Pico veto, Mega Drive, SMD, Master System / Game Gear)

**Files:**
- Modify: `crates/coverly-core/src/identify/sniff.rs`
- Test: `crates/coverly-core/tests/sniff.rs` (append)

**Interfaces:**
- Produces fixtures: `md()`, `smd_interleaved()`, `sega32x()`, `pico()`, `segacd_iso(raw: bool)`, `sms_gg(offset: usize, region: u8)`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn mega_drive_and_interleaved_smd() {
    assert_eq!(best(&run(&fx::md(), "md")), Some(("megadrive".into(), 0.90)));
    assert_eq!(best(&run(&fx::smd_interleaved(), "smd")), Some(("megadrive".into(), 0.90)));
}

#[test]
fn sega_cd_32x_and_pico_never_come_out_as_mega_drive() {
    assert_eq!(best(&run(&fx::segacd_iso(false), "iso")).unwrap().0, "segacd");
    assert_eq!(best(&run(&fx::segacd_iso(true), "bin")).unwrap().0, "segacd");
    assert_eq!(best(&run(&fx::sega32x(), "32x")), Some(("sega32x".into(), 0.95)));
    let pico = run(&fx::pico(), "md");
    assert!(pico.hits.is_empty());
    assert_eq!(pico.unsupported.as_deref(), Some("Sega Pico"));
}

#[test]
fn master_system_and_game_gear() {
    // TMR SEGA at any of the three offsets → SMS family.
    for offset in [0x7FF0usize, 0x3FF0, 0x1FF0] {
        let sms = run(&fx::sms_gg(offset, 0x4C), "sms");
        assert_eq!(best(&sms), Some(("mastersystem".into(), 0.90)), "{offset:#x}");
    }
    // The extension decides even against the region code.
    let gg_with_sms_region = run(&fx::sms_gg(0x7FF0, 0x4C), "gg");
    assert_eq!(best(&gg_with_sms_region), Some(("gamegear".into(), 0.90)));
    // Without a deciding extension the region is only a tiebreak.
    let gg_region = run(&fx::sms_gg(0x7FF0, 0x6C), "bin");
    assert_eq!(best(&gg_region), Some(("gamegear".into(), 0.70)));
}
```

- [ ] **Step 2: Run to see them fail.**

- [ ] **Step 3: Implement** (in `cartridges`, after the NES/N64/Lynx/7800 checks and before the GBA check, which stays last; Sega CD also lives in `discs` for ISO/raw images)

```rust
    // Sega family: most specific first (spec §6.3).
    if starts(head, 0, b"SEGADISCSYSTEM") || starts(head, 0x10, b"SEGADISCSYSTEM") {
        s.hit("segacd", 0.95, "SEGADISCSYSTEM");
        return;
    }
    let sega_at_100 = starts(head, 0x100, b"SEGA") || starts(head, 0x101, b"SEGA");
    if starts(head, 0x100, b"SEGA 32X")
        || (sega_at_100 && head.get(..0x400.min(head.len())).is_some_and(|h| contains(h, b"MARS")))
    {
        s.hit("sega32x", 0.95, "SEGA 32X");
        return;
    }
    if starts(head, 0x100, b"SEGA PICO") {
        s.unsupported = Some("Sega Pico".into());
        return;
    }
    if sega_at_100 {
        s.hit("megadrive", 0.90, "SEGA at 0x100");
        return;
    }
    if len % 16384 == 512 && at(head, 8, 2) == Some(&[0xAA, 0xBB][..]) {
        s.hit("megadrive", 0.90, "SMD interleaved");
        return;
    }
    for offset in [0x7FF0usize, 0x3FF0, 0x1FF0] {
        if starts(head, offset, b"TMR SEGA") {
            let region = head.get(offset + 0xF).map_or(0, |b| b >> 4);
            match ext {
                "gg" => s.hit("gamegear", 0.90, "TMR SEGA"),
                "sms" => s.hit("mastersystem", 0.90, "TMR SEGA"),
                _ => {
                    let (system, other) = if (5..=7).contains(&region) {
                        ("gamegear", "mastersystem")
                    } else {
                        ("mastersystem", "gamegear")
                    };
                    s.hit(system, 0.70, format!("TMR SEGA, region {region}"));
                    s.hit(other, 0.60, "TMR SEGA");
                }
            }
            return;
        }
    }
```

```rust
fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}
```

Fixtures:

```rust
    pub fn md() -> Vec<u8> { let mut b = zeros(128 * 1024); b[0x100..0x10F].copy_from_slice(b"SEGA MEGA DRIVE"); b }
    pub fn smd_interleaved() -> Vec<u8> { let mut b = zeros(16384 * 8 + 512); b[8] = 0xAA; b[9] = 0xBB; b }
    pub fn sega32x() -> Vec<u8> { let mut b = zeros(128 * 1024); b[0x100..0x108].copy_from_slice(b"SEGA 32X"); b }
    pub fn pico() -> Vec<u8> { let mut b = zeros(128 * 1024); b[0x100..0x109].copy_from_slice(b"SEGA PICO"); b }
    pub fn segacd_iso(raw: bool) -> Vec<u8> {
        let mut b = zeros(64 * 1024);
        let at = if raw { 0x10 } else { 0 };
        b[at..at + 14].copy_from_slice(b"SEGADISCSYSTEM");
        b
    }
    pub fn sms_gg(offset: usize, region_byte: u8) -> Vec<u8> {
        let mut b = zeros((offset + 0x10).max(32 * 1024));
        b[offset..offset + 8].copy_from_slice(b"TMR SEGA");
        b[offset + 0xF] = region_byte;
        b
    }
```

- [ ] **Step 4: Run tests and gates.**

- [ ] **Step 5: Commit**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize && git add crates/coverly-core && git commit -m "feat(core): sniff the Sega family with Pico and Sega CD vetoes"
```

---

### Task 7: Core — sniff SNES (LoROM/HiROM/ExHiROM, copier header), Neo Geo Pocket, WonderSwan

**Files:**
- Modify: `crates/coverly-core/src/identify/sniff.rs`
- Test: `crates/coverly-core/tests/sniff.rs` (append)

**Interfaces:**
- Produces fixtures: `snes(map: SnesMap, copier: bool)` (`SnesMap::{Lo, Hi, ExHi}`), `ngp(color: bool)`, `ws(color: bool)`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn snes_lorom_hirom_exhirom_with_and_without_copier_header() {
    use fx::SnesMap::*;
    for (map, copier) in [(Lo, false), (Lo, true), (Hi, false), (Hi, true), (ExHi, false)] {
        let rom = fx::snes(map, copier);
        assert_eq!(best(&run(&rom, "sfc")), Some(("snes".into(), 0.95)), "{map:?} copier={copier}");
    }
}

#[test]
fn snes_with_a_broken_checksum_but_valid_map_and_title_is_weaker() {
    let mut rom = fx::snes(fx::SnesMap::Lo, false);
    rom[0x7FC0 + 0x1C] ^= 0xFF;
    assert_eq!(best(&run(&rom, "sfc")), Some(("snes".into(), 0.85)));
}

#[test]
fn neo_geo_pocket_and_colour() {
    assert_eq!(best(&run(&fx::ngp(false), "ngp")), Some(("ngp".into(), 0.90)));
    assert_eq!(best(&run(&fx::ngp(true), "ngc")), Some(("ngpc".into(), 0.90)));
}

#[test]
fn wonderswan_footer_only_for_its_extensions() {
    assert_eq!(best(&run(&fx::ws(false), "ws")), Some(("wonderswan".into(), 0.85)));
    assert_eq!(best(&run(&fx::ws(true), "wsc")), Some(("wonderswancolor".into(), 0.85)));
    assert_eq!(best(&run(&fx::ws(false), "bin")), None, "0xEA alone is too common");
}
```

The ExHiROM case needs `len > 0x40FFE0`: the fixture is 4 MiB + 64 KiB of zeros (allocated in the test only).

- [ ] **Step 2: Run to see them fail.**

- [ ] **Step 3: Implement** (end of `cartridges`, before the GBA check, which stays last)

```rust
    if let Some(weight) = snes_header(src, head, len) {
        s.hit("snes", weight, if weight >= 0.95 { "internal header" } else { "map mode + title" });
        return;
    }
    if starts(head, 0, b"COPYRIGHT BY SNK CORPORATION") || starts(head, 0, b" LICENSED BY SNK CORPORATION") {
        let color = head.get(0x23) == Some(&0x10);
        s.hit(if color { "ngpc" } else { "ngp" }, 0.90, "SNK");
        return;
    }
    if matches!(ext, "ws" | "wsc") && len >= 10 && len % 65536 == 0 {
        let mut foot = [0u8; 10];
        if read_exact_at(src, len - 10, &mut foot) == 10 && foot[0] == 0xEA {
            let color = foot[1] & 1 == 1;
            s.hit(if color { "wonderswancolor" } else { "wonderswan" }, 0.85, "footer");
        }
    }
```

```rust
const SNES_MAPS: [u8; 8] = [0x20, 0x21, 0x23, 0x25, 0x30, 0x31, 0x32, 0x35];

/// SNES internal header at LoROM/HiROM/ExHiROM, after a 512-byte copier
/// header when `len % 1024 == 512`.
fn snes_header(src: &mut dyn ReadAt, head: &[u8], len: u64) -> Option<f32> {
    let base: u64 = if len % 1024 == 512 { 512 } else { 0 };
    let mut weak = None;
    for offset in [0x7FC0u64, 0xFFC0, 0x40_FFC0] {
        let pos = base + offset;
        if pos + 0x20 > len {
            continue;
        }
        let mut h = [0u8; 0x20];
        let got = match usize::try_from(pos).ok().and_then(|p| at(head, p, 0x20)) {
            Some(slice) => { h.copy_from_slice(slice); 0x20 }
            None => read_exact_at(src, pos, &mut h),
        };
        if got < 0x20 || !SNES_MAPS.contains(&h[0x15]) {
            continue;
        }
        let complement = u16::from_le_bytes([h[0x1C], h[0x1D]]);
        let checksum = u16::from_le_bytes([h[0x1E], h[0x1F]]);
        if checksum ^ complement == 0xFFFF {
            return Some(0.95);
        }
        let title_ok = h[..21].iter().all(|b| (0x20..0x7F).contains(b)) && h[..21].iter().any(|b| *b != b' ');
        if title_ok {
            weak = Some(0.85);
        }
    }
    weak
}
```

Fixtures:

```rust
    #[derive(Clone, Copy, Debug)]
    pub enum SnesMap { Lo, Hi, ExHi }
    pub fn snes(map: SnesMap, copier: bool) -> Vec<u8> {
        let (offset, len, mode) = match map {
            SnesMap::Lo => (0x7FC0usize, 0x8_0000usize, 0x20u8),
            SnesMap::Hi => (0xFFC0, 0x10_0000, 0x21),
            SnesMap::ExHi => (0x40_FFC0, 0x41_0000, 0x25),
        };
        let base = if copier { 512 } else { 0 };
        let mut b = zeros(len + base);
        let h = &mut b[base + offset..base + offset + 0x20];
        h[..21].copy_from_slice(b"SYNTH SNES           ");
        h[0x15] = mode;
        h[0x1C..0x1E].copy_from_slice(&0x5A5Au16.to_le_bytes());
        h[0x1E..0x20].copy_from_slice(&(0x5A5Au16 ^ 0xFFFF).to_le_bytes());
        b
    }
    pub fn ngp(color: bool) -> Vec<u8> {
        let mut b = zeros(64 * 1024);
        b[..28].copy_from_slice(b"COPYRIGHT BY SNK CORPORATION");
        if color { b[0x23] = 0x10; }
        b
    }
    pub fn ws(color: bool) -> Vec<u8> {
        let mut b = zeros(128 * 1024);
        let n = b.len();
        b[n - 10] = 0xEA;
        b[n - 9] = u8::from(color);
        b
    }
```

> **Review fix (verified on real dumps):** the WonderSwan footer is 16 bytes: 0xEA at `len - 16`, colour flag at `len - 9` (the fixture above is corrected in code). `SNES_MAPS` also holds 0x22 and 0x3A (SPC7110); a checksum pair (not 0xFFFF/0x0000) is strong under a valid map mode *or* a ROM-size byte in 0x08..=0x0D; and a GBA header *with its logo* is checked before SNES (only the logo-less GBA signature stays last).

- [ ] **Step 4: Run tests and gates.**

- [ ] **Step 5: Commit**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize && git add crates/coverly-core && git commit -m "feat(core): sniff SNES, Neo Geo Pocket and WonderSwan headers"
```

---

### Task 8: Core — sniff disc images (PS1 vs PS2 by `SYSTEM.CNF`, Saturn, Dreamcast, PSP, CSO) and the random-bytes guard

**Files:**
- Modify: `crates/coverly-core/src/identify/sniff.rs` (`discs`)
- Test: `crates/coverly-core/tests/sniff.rs` (append)

**Interfaces:**
- Produces fixtures: `psx_iso(raw: bool, boot2: bool)`, `saturn()`, `dreamcast_track()`, `psp_iso()`, `cso()`; `fn xorshift_bytes(seed: u64, len: usize) -> Vec<u8>`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn playstation_by_system_cnf() {
    for raw in [false, true] {
        let ps1 = run(&fx::psx_iso(raw, false), if raw { "bin" } else { "iso" });
        assert_eq!(best(&ps1), Some(("psx".into(), 0.95)), "raw={raw}");
        let ps2 = run(&fx::psx_iso(raw, true), "iso");
        assert!(ps2.hits.is_empty(), "PS2 is never psx");
        assert_eq!(ps2.unsupported.as_deref(), Some("PlayStation 2"));
    }
}

#[test]
fn playstation_without_system_cnf_is_family_only() {
    let mut iso = fx::psx_iso(false, false);
    fx::drop_system_cnf(&mut iso);
    assert_eq!(best(&run(&iso, "iso")), Some(("psx".into(), 0.80)));
}

#[test]
fn saturn_dreamcast_psp() {
    assert_eq!(best(&run(&fx::saturn(), "iso")), Some(("saturn".into(), 0.90)));
    assert_eq!(best(&run(&fx::dreamcast_track(), "bin")), Some(("dreamcast".into(), 0.90)));
    assert_eq!(best(&run(&fx::psp_iso(), "iso")), Some(("psp".into(), 0.85)));
    assert_eq!(best(&run(&fx::cso(), "cso")), Some(("psp".into(), 0.85)));
}

#[test]
fn random_bytes_never_reach_high_confidence() {
    for seed in 1..=400u64 {
        let bytes = fx::xorshift_bytes(seed, 64 * 1024 + (seed as usize % 7) * 512);
        for ext in ["bin", "gb", "gba", "sfc", "md", "iso", "ws"] {
            let s = run(&bytes, ext);
            assert!(
                s.hits.iter().all(|e| e.weight < 0.95),
                "seed {seed} ext {ext}: {:?}",
                s.hits
            );
        }
    }
}
```

- [ ] **Step 2: Run to see them fail.**

- [ ] **Step 3: Implement `discs`**

```rust
/// ISO 9660 primary volume descriptor at sector 16, as a 2048-byte ISO or
/// a raw 2352-byte Mode 2 image (user data at +24 of each sector).
#[derive(Clone, Copy)]
struct IsoLayout { sector: u64, data: u64 }
const ISO: IsoLayout = IsoLayout { sector: 2048, data: 0 };
const RAW: IsoLayout = IsoLayout { sector: 2352, data: 24 };

impl IsoLayout {
    fn offset(self, lba: u64, within: u64) -> u64 { lba * self.sector + self.data + within }
}

fn discs(src: &mut dyn ReadAt, head: &[u8], len: u64, s: &mut Sniff) {
    for (magic, system, detail) in [
        (&b"SEGA SEGASATURN"[..], "saturn", "SEGA SEGASATURN"),
        (&b"SEGA SEGAKATANA"[..], "dreamcast", "SEGA SEGAKATANA"),
    ] {
        if starts(head, 0, magic) || starts(head, 0x10, magic) {
            s.hit(system, 0.90, detail);
            return;
        }
    }
    if starts(head, 0, b"CISO") {
        s.hit("psp", 0.85, "CSO");
        return;
    }
    for layout in [ISO, RAW] {
        let Some(pvd) = read_block(src, head, layout.offset(16, 0), 2048, len) else { continue };
        if &pvd[1..6] != b"CD001" {
            continue;
        }
        if &pvd[40..48] == b"PSP GAME" {
            s.hit("psp", 0.85, "PSP GAME");
            return;
        }
        if &pvd[8..19] != b"PLAYSTATION" {
            return; // another ISO 9660 disc: nothing to say
        }
        match system_cnf(src, head, layout, &pvd, len) {
            Some(text) if text.contains("BOOT2") => s.unsupported = Some("PlayStation 2".into()),
            Some(text) if text.contains("BOOT") => s.hit("psx", 0.95, "SYSTEM.CNF BOOT"),
            _ => s.hit("psx", 0.80, "PLAYSTATION"),
        }
        return;
    }
}

/// `n` bytes at `offset`, from the prefix when it covers them.
fn read_block(src: &mut dyn ReadAt, head: &[u8], offset: u64, n: usize, len: u64) -> Option<Vec<u8>> {
    if offset + n as u64 > len {
        return None;
    }
    if let Some(slice) = usize::try_from(offset).ok().and_then(|o| at(head, o, n)) {
        return Some(slice.to_vec());
    }
    let mut buf = vec![0u8; n];
    (read_exact_at(src, offset, &mut buf) == n).then_some(buf)
}

/// Text of `SYSTEM.CNF` from the root directory (one directory read, at
/// most 2 sectors, file capped at 2 KiB).
fn system_cnf(src: &mut dyn ReadAt, head: &[u8], layout: IsoLayout, pvd: &[u8], len: u64) -> Option<String> {
    let root = &pvd[156..156 + 34];
    let lba = u64::from(u32::from_le_bytes(root[2..6].try_into().ok()?));
    let size = u32::from_le_bytes(root[10..14].try_into().ok()?).min(4096) as usize;
    let mut dir = Vec::new();
    for i in 0..size.div_ceil(2048) as u64 {
        dir.extend(read_block(src, head, layout.offset(lba + i, 0), 2048, len)?);
    }
    let mut pos = 0;
    while pos + 33 < dir.len() {
        let rec_len = dir[pos] as usize;
        if rec_len == 0 {
            // Records never cross a sector: skip to the next one.
            pos = (pos / 2048 + 1) * 2048;
            continue;
        }
        let name_len = dir[pos + 32] as usize;
        let name = dir.get(pos + 33..pos + 33 + name_len)?;
        if name.eq_ignore_ascii_case(b"SYSTEM.CNF;1") {
            let file_lba = u64::from(u32::from_le_bytes(dir[pos + 2..pos + 6].try_into().ok()?));
            let file_len = (u32::from_le_bytes(dir[pos + 10..pos + 14].try_into().ok()?) as usize).min(2048);
            let bytes = read_block(src, head, layout.offset(file_lba, 0), file_len, len)?;
            return Some(String::from_utf8_lossy(&bytes).to_ascii_uppercase());
        }
        pos += rec_len;
    }
    None
}
```

Fixtures (`psx_iso` writes a PVD at sector 16 with `CD001`, `PLAYSTATION` at +8, a root record at +156 pointing to LBA 20, a directory with one `SYSTEM.CNF;1` record pointing to LBA 21 holding `BOOT = cdrom:\SLUS_000.01;1` or `BOOT2 = cdrom0:\SLUS_200.01;1`; for `raw` every sector is 2352 bytes with user data at +24; `drop_system_cnf` renames the record to `README.TXT;1`; `saturn()` = `SEGA SEGASATURN` at 0; `dreamcast_track()` = `SEGA SEGAKATANA` at 0; `psp_iso()` = PVD with `PSP GAME` at +40; `cso()` = `CISO` at 0; `xorshift_bytes(seed, len)` = xorshift64* stream). Write each as a small function following the layout above; keep images ≤ 24 sectors.

- [ ] **Step 4: Run tests and gates.** If `random_bytes_never_reach_high_confidence` fails, tighten the offending check (never loosen the test).

- [ ] **Step 5: Commit**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize && git add crates/coverly-core && git commit -m "feat(core): sniff disc images and guard against random bytes"
```

---

## Group O3 — Archives and disc sets

### Task 9: Core — `organize::inspect`: raw files, bounded zip entries, 7z/CHD/RAR, verdicts

**Files:**
- Modify: `crates/coverly-core/Cargo.toml` (`zip = { workspace = true, features = ["deflate-flate2"] }`; move `flate2.workspace = true` from `[dev-dependencies]` to `[dependencies]`)
- Modify: `crates/coverly-core/src/lib.rs` (`pub mod organize;`)
- Create: `crates/coverly-core/src/organize/mod.rs` (`pub mod inspect;` for now)
- Create: `crates/coverly-core/src/organize/inspect.rs`
- Create: `crates/coverly-core/tests/common/roms.rs` (zip builders), register in `tests/common/mod.rs`
- Test: `crates/coverly-core/tests/organize_inspect.rs`

**Interfaces:**
- Consumes: `identify::sniff::{sniff, FileAt, StreamAt, SliceAt, Sniff, NotARom, ContainerMagic, STREAM_CAP}`, `SystemDb::{extension_class, is_rom_like_extension}`, `scan::classify::extension`.
- Produces:
  ```rust
  #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)] #[serde(rename_all = "kebab-case")]
  pub enum Container { Raw, Zip, SevenZip, Chd, Rar }
  #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
  pub struct Payload { pub name: String, pub ext: String, pub size: u64, pub crc32: Option<String>, pub sniff: Sniff }
  #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
  pub struct ZipEntryInfo { pub name: String, pub crc32: String, pub size: u64 }
  #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)] #[serde(tag = "kind", rename_all = "kebab-case")]
  pub enum ArchiveIssue { Damaged, Unsupported, NotARom { what: NotARom }, TooManyRoms { count: usize } }
  #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
  pub struct Inspection {
      pub container: Container,
      pub payloads: Vec<Payload>,          // ROM-like contents, sniffed (≤ 8)
      pub entries: Vec<ZipEntryInfo>,      // every file entry of a zip (arcade confirmation)
      pub issue: Option<ArchiveIssue>,
      pub notes: Vec<String>,              // "encrypted", "unsupported compression" (data, not prose)
  }
  pub const MAX_ZIP_ENTRIES: usize = 10_000;
  pub const MAX_ROM_ENTRIES: usize = 8;
  pub fn inspect_file(path: &Path, len: u64, db: &SystemDb) -> io::Result<Inspection>;
  pub fn inspect_set_primary(track: &Path) -> io::Result<Sniff>;   // first data track of a cue/gdi set
  ```

- [ ] **Step 1: Write the zip builders** — `tests/common/roms.rs`

```rust
//! Synthetic zips for organise tests (no real ROM data).

use std::fs::File;
use std::io::Write;
use std::path::Path;

use zip::write::SimpleFileOptions;
use zip::CompressionMethod;

/// Writes a zip at `path` with `(name, bytes, deflate)` entries.
pub fn zip_file(path: &Path, entries: &[(&str, &[u8], bool)]) {
    let mut zip = zip::ZipWriter::new(File::create(path).unwrap());
    for (name, bytes, deflate) in entries {
        let method = if *deflate { CompressionMethod::Deflated } else { CompressionMethod::Stored };
        zip.start_file(*name, SimpleFileOptions::default().compression_method(method)).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
}
```

(`zip` 9 writes deflate only with a deflate feature; the core now enables `deflate-flate2`, and the dev-dependency on itself inherits it.)

- [ ] **Step 2: Write the failing tests** — `tests/organize_inspect.rs`

```rust
mod common;

use coverly_core::identify::sniff::fixtures as fx;
use coverly_core::organize::inspect::{inspect_file, ArchiveIssue, Container};
use coverly_core::identify::sniff::NotARom;
use coverly_core::systems::SystemDb;
use common::roms::zip_file;

fn inspect(path: &std::path::Path) -> coverly_core::organize::inspect::Inspection {
    let len = std::fs::metadata(path).unwrap().len();
    inspect_file(path, len, &SystemDb::builtin()).unwrap()
}

#[test]
fn a_raw_file_is_one_sniffed_payload() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("Game.gba");
    std::fs::write(&p, fx::gba()).unwrap();
    let i = inspect(&p);
    assert_eq!(i.container, Container::Raw);
    assert_eq!(i.payloads.len(), 1);
    assert_eq!(i.payloads[0].crc32, None, "raw files are not hashed here");
    assert_eq!(i.payloads[0].sniff.hits[0].system, "gba");
}

#[test]
fn single_entry_zips_stored_and_deflated() {
    let tmp = tempfile::tempdir().unwrap();
    for deflate in [false, true] {
        let p = tmp.path().join(format!("t{deflate}.zip"));
        let rom = fx::gb(0xC0);
        zip_file(&p, &[("Tetris DX.gbc", &rom, deflate)]);
        let i = inspect(&p);
        assert_eq!(i.container, Container::Zip);
        let pay = &i.payloads[0];
        assert_eq!(pay.ext, "gbc");
        assert_eq!(pay.crc32.as_deref(), Some(format!("{:08X}", crc32fast::hash(&rom)).as_str()));
        assert_eq!(pay.sniff.hits[0].system, "gbc");
    }
}

#[test]
fn readme_entries_are_not_payloads() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("g.zip");
    zip_file(&p, &[("readme.txt", b"hello", true), ("dir/Game.gba", &fx::gba(), true)]);
    let i = inspect(&p);
    assert_eq!(i.payloads.len(), 1);
    assert_eq!(i.payloads[0].name, "dir/Game.gba");
    assert_eq!(i.entries.len(), 2);
}

#[test]
fn an_html_page_named_zip_is_not_a_rom() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("3-D Tetris (USA).zip");
    std::fs::write(&p, fx::html_page()).unwrap();
    assert_eq!(inspect(&p).issue, Some(ArchiveIssue::NotARom { what: NotARom::WebPage }));
}

#[test]
fn a_truncated_zip_is_damaged() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("t.zip");
    zip_file(&p, &[("a.gba", &fx::gba(), true)]);
    let bytes = std::fs::read(&p).unwrap();
    std::fs::write(&p, &bytes[..bytes.len() / 2]).unwrap();
    assert_eq!(inspect(&p).issue, Some(ArchiveIssue::Damaged));
}

#[test]
fn a_zip_bomb_entry_is_read_bounded() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("bomb.zip");
    let zeros = vec![0u8; 64 * 1024 * 1024]; // 64 MiB of zeros deflate to ~64 KiB
    zip_file(&p, &[("big.sfc", &zeros, true)]);
    let started = std::time::Instant::now();
    let i = inspect(&p);
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
    assert!(i.payloads[0].sniff.hits.is_empty());
}

#[test]
fn too_many_roms_is_a_collection() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("collection.zip");
    let rom = fx::nes();
    let names: Vec<String> = (0..20).map(|i| format!("g{i}.nes")).collect();
    let entries: Vec<(&str, &[u8], bool)> = names.iter().map(|n| (n.as_str(), rom.as_slice(), false)).collect();
    zip_file(&p, &entries);
    assert_eq!(inspect(&p).issue, Some(ArchiveIssue::TooManyRoms { count: 20 }));
}

#[test]
fn seven_zip_chd_and_rar_are_recognised_only() {
    let tmp = tempfile::tempdir().unwrap();
    let cases: [(&str, &[u8], Container, Option<ArchiveIssue>); 5] = [
        ("a.7z", &[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C, 0, 4], Container::SevenZip, None),
        ("b.7z", b"not a 7z at all", Container::SevenZip, Some(ArchiveIssue::Damaged)),
        ("c.chd", b"MComprHD\0\0\0\x7c", Container::Chd, None),
        ("d.chd", b"garbage!", Container::Chd, Some(ArchiveIssue::Damaged)),
        ("e.rar", b"Rar!\x1a\x07\x01\x00", Container::Rar, Some(ArchiveIssue::Unsupported)),
    ];
    for (name, bytes, container, issue) in cases {
        let p = tmp.path().join(name);
        std::fs::write(&p, bytes).unwrap();
        let i = inspect(&p);
        assert_eq!((i.container, i.issue), (container, issue), "{name}");
        assert!(i.payloads.is_empty());
    }
}
```

- [ ] **Step 3: Run to see them fail**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize && cargo test -p coverly-core --test organize_inspect`
Expected: compile errors.

- [ ] **Step 4: Implement `inspect.rs`**

```rust
//! Opening a candidate (spec §7.2): raw file, zip entries (central
//! directory CRCs + bounded inflate of ROM-like entries), container magic
//! for 7z/CHD/RAR. Never extracts, never inflates more than STREAM_CAP.

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::identify::sniff::{sniff, ContainerMagic, FileAt, NotARom, SliceAt, Sniff, StreamAt, PREFIX, STREAM_CAP};
use crate::scan::classify::extension;
use crate::systems::{ExtClass, SystemDb};

pub const MAX_ZIP_ENTRIES: usize = 10_000;
pub const MAX_ROM_ENTRIES: usize = 8;

// … types from the Interfaces block …

pub fn inspect_file(path: &Path, len: u64, db: &SystemDb) -> io::Result<Inspection> {
    let ext = extension(path);
    let mut file = File::open(path)?;
    let mut head = vec![0u8; PREFIX.min(usize::try_from(len).unwrap_or(PREFIX))];
    let n = read_prefix(&mut file, &mut head)?;
    head.truncate(n);
    let first = sniff(&mut SliceAt(&head), head.len() as u64, &ext);
    let mut out = Inspection::new(container_for(&ext, first.container));
    match out.container {
        Container::Zip => inspect_zip(&mut file, &first, db, &mut out),
        Container::SevenZip => out.issue = (first.container != Some(ContainerMagic::SevenZip)).then_some(ArchiveIssue::Damaged),
        Container::Chd => out.issue = (first.container != Some(ContainerMagic::Chd)).then_some(ArchiveIssue::Damaged),
        Container::Rar => out.issue = Some(ArchiveIssue::Unsupported),
        Container::Raw => {
            let sniffed = if len <= head.len() as u64 { first } else { sniff(&mut FileAt(file), len, &ext) };
            if let Some(what) = sniffed.not_a_rom {
                out.issue = Some(ArchiveIssue::NotARom { what });
            }
            out.payloads.push(Payload { name: file_name(path), ext, size: len, crc32: None, sniff: sniffed });
        }
    }
    Ok(out)
}

/// Reads the start of `file` into `buf` (fewer bytes at end of file).
fn read_prefix(file: &mut File, buf: &mut [u8]) -> io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match file.read(&mut buf[n..])? {
            0 => break,
            k => n += k,
        }
    }
    Ok(n)
}

fn file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

impl Inspection {
    fn new(container: Container) -> Inspection {
        Inspection { container, payloads: Vec::new(), entries: Vec::new(), issue: None, notes: Vec::new() }
    }
}

/// By extension first (a `.zip` that is HTML is still judged as a zip
/// that failed), else by magic (a ROM named `.bin` that is really a zip).
fn container_for(ext: &str, magic: Option<ContainerMagic>) -> Container {
    match (ext, magic) {
        ("zip", _) | (_, Some(ContainerMagic::Zip)) => Container::Zip,
        ("7z", _) | (_, Some(ContainerMagic::SevenZip)) => Container::SevenZip,
        ("chd", _) | (_, Some(ContainerMagic::Chd)) => Container::Chd,
        ("rar", _) | (_, Some(ContainerMagic::Rar)) => Container::Rar,
        _ => Container::Raw,
    }
}

fn inspect_zip(file: &mut File, first: &Sniff, db: &SystemDb, out: &mut Inspection) {
    if first.container != Some(ContainerMagic::Zip) {
        out.issue = Some(match first.not_a_rom {
            Some(what) => ArchiveIssue::NotARom { what },
            None => ArchiveIssue::Damaged,
        });
        return;
    }
    let Ok(mut archive) = zip::ZipArchive::new(&mut *file) else {
        out.issue = Some(ArchiveIssue::Damaged);
        return;
    };
    if archive.len() > MAX_ZIP_ENTRIES {
        out.issue = Some(ArchiveIssue::Damaged);
        return;
    }
    let mut rom_indices = Vec::new();
    for i in 0..archive.len() {
        let Ok(entry) = archive.by_index_raw(i) else {
            out.issue = Some(ArchiveIssue::Damaged);
            return;
        };
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_string();
        out.entries.push(ZipEntryInfo { name: name.clone(), crc32: format!("{:08X}", entry.crc32()), size: entry.size() });
        let inner_ext = extension(Path::new(&name));
        let class = db.extension_class(&inner_ext);
        if matches!(class, ExtClass::Unique | ExtClass::Shared) {
            rom_indices.push(i);
        }
    }
    if rom_indices.len() > MAX_ROM_ENTRIES {
        out.issue = Some(ArchiveIssue::TooManyRoms { count: rom_indices.len() });
        return;
    }
    for i in rom_indices {
        let info = out.entries.iter().find(|e| Some(e.name.as_str()) == archive.name_for_index(i)).cloned();
        let Some(info) = info else { continue };
        let ext = extension(Path::new(&info.name));
        let sniffed = match archive.by_index(i) {
            Ok(entry) => sniff(&mut StreamAt::new(entry.take(STREAM_CAP), STREAM_CAP), info.size, &ext),
            Err(e) => {
                out.notes.push(if e.to_string().contains("ncrypt") { "encrypted".into() } else { "unsupported-compression".into() });
                Sniff::default()
            }
        };
        out.payloads.push(Payload { name: info.name, ext, size: info.size, crc32: Some(info.crc32), sniff: sniffed });
    }
}
```

Notes for the implementer:
- `zip` 9: check the exact names `by_index_raw`, `name_for_index`, `ZipFile::encrypted()`; prefer `entry.encrypted()` over string matching when available, and record `"encrypted"` before trying `by_index`.
- Arcade-shaped zips (several entries, none with a Unique/Shared console extension) end with empty `payloads` and full `entries`: the ladder (Task 14) handles them.
- `inspect_set_primary(track)` = `sniff(&mut FileAt(File::open(track)?), len, ext)`.
- *As built:* a zip whose **only** file entry has a generic extension (`bin`, `iso`, `img`, `rom`, `dat`) also yields that entry as a payload (Mega Drive `.bin` and PS1 `.iso` zips); next to other entries a generic name stays arcade data and is not counted toward `TooManyRoms`. Encrypted entries are detected with `ZipFile::encrypted()` from the raw pass (CRC kept, not sniffed, note `"encrypted"`); `UnsupportedArchive`/`CompressionMethodNotSupported` give `"unsupported-compression"`; any other entry error, a bad central-directory record or more than `MAX_ZIP_ENTRIES` entries makes the zip `Damaged` with `entries`/`payloads` cleared. `ZipFile::name()` is fallible in `zip` 9: a non-UTF-8 name falls back to a lossy `name_raw()`.

- [ ] **Step 5: Run tests and gates.**

- [ ] **Step 6: Commit**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize && git add Cargo.lock crates/coverly-core && git commit -m "feat(core): inspect raw files and archives with bounded reads"
```

---

### Task 10: Core — cache schema v2 (CRC index, game index, organise facts and choices)

**Files:**
- Modify: `crates/coverly-core/src/cache/mod.rs`
- Test: `crates/coverly-core/tests/cache.rs` (append)

**Interfaces:**
- Produces:
  ```rust
  pub const SCHEMA_VERSION: i32 = 2;
  impl Cache {
      pub fn dat_by_crc_any(&self, crc32: &str, size: u64) -> Result<Vec<(String, DatRow)>, CacheError>;
      pub fn dat_rows_of_game(&self, dat: &str, game: &str) -> Result<Vec<DatRow>, CacheError>;
      pub fn dat_stems(&self, dat: &str) -> Result<Vec<(String, String)>, CacheError>; // (rom_stem, game)
      pub fn cached_dats(&self) -> Result<Vec<String>, CacheError>;
      pub fn organize_facts(&self, path: &Path, size: u64, mtime_ns: i64) -> Result<Option<String>, CacheError>;
      pub fn put_organize_facts(&self, path: &Path, size: u64, mtime_ns: i64, json: &str) -> Result<(), CacheError>;
      pub fn organize_choice(&self, crc32: &str, size: u64) -> Result<Option<String>, CacheError>;
      pub fn put_organize_choice(&self, crc32: &str, size: u64, system_id: &str) -> Result<(), CacheError>;
  }
  ```

- [ ] **Step 1: Write the failing tests** (append to `tests/cache.rs`)

```rust
#[test]
fn a_v1_database_migrates_to_v2_and_keeps_its_rows() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("c.sqlite3");
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(V1_SCHEMA_FOR_TEST).unwrap(); // copy of SCHEMA_V1
        conn.pragma_update(None, "user_version", 1).unwrap();
        conn.execute("INSERT INTO dat_roms VALUES ('d.dat','Tetris (World)','t.gb','t',32768,'46DF91AD',NULL,NULL)", []).unwrap();
    }
    let cache = coverly_core::cache::Cache::open(&path).unwrap();
    let hits = cache.dat_by_crc_any("46df91ad", 32768).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].0, "d.dat");
    assert_eq!(cache.cached_dats().unwrap(), ["d.dat"]);
}

#[test]
fn crc_any_respects_size_and_case() {
    let cache = coverly_core::cache::Cache::in_memory().unwrap();
    cache.replace_dat("gb.dat", &[row("Tetris (World)", "Tetris.gb", 32768, "46DF91AD")]).unwrap();
    cache.replace_dat("gbc.dat", &[row("Other", "o.gbc", 65536, "46DF91AD")]).unwrap();
    let hits = cache.dat_by_crc_any("46df91ad", 32768).unwrap();
    assert_eq!(hits.iter().map(|h| h.0.as_str()).collect::<Vec<_>>(), ["gb.dat"]);
}

#[test]
fn organise_facts_are_keyed_by_size_and_mtime() {
    let cache = coverly_core::cache::Cache::in_memory().unwrap();
    let p = std::path::Path::new("/pile/a.zip");
    cache.put_organize_facts(p, 10, 5, "{\"x\":1}").unwrap();
    assert_eq!(cache.organize_facts(p, 10, 5).unwrap().as_deref(), Some("{\"x\":1}"));
    assert_eq!(cache.organize_facts(p, 10, 6).unwrap(), None);
}

#[test]
fn user_choices_are_remembered_by_content() {
    let cache = coverly_core::cache::Cache::in_memory().unwrap();
    cache.put_organize_choice("ABCDEF01", 100, "gbc").unwrap();
    assert_eq!(cache.organize_choice("abcdef01", 100).unwrap().as_deref(), Some("gbc"));
    assert_eq!(cache.organize_choice("ABCDEF01", 101).unwrap(), None);
}
```

(`row(...)` builds a `DatRow`; `V1_SCHEMA_FOR_TEST` is a `const &str` in the test file with the v1 DDL copied from `cache/mod.rs`.)

- [ ] **Step 2: Run to see them fail.**

- [ ] **Step 3: Implement**

```rust
pub const SCHEMA_VERSION: i32 = 2;

const SCHEMA_V2: &str = "
CREATE INDEX IF NOT EXISTS dat_roms_crc_any ON dat_roms (crc32);
CREATE INDEX IF NOT EXISTS dat_roms_game ON dat_roms (dat, game);
CREATE TABLE IF NOT EXISTS organize_ids (path TEXT PRIMARY KEY, size INTEGER NOT NULL, mtime_ns INTEGER NOT NULL, facts TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS organize_choices (crc32 TEXT NOT NULL, size INTEGER NOT NULL, system_id TEXT NOT NULL, PRIMARY KEY (crc32, size));
";
```

`prepare`:

```rust
    match version {
        0 => {
            conn.execute_batch(SCHEMA_V1)?;
            conn.execute_batch(SCHEMA_V2)?;
        }
        1 => conn.execute_batch(SCHEMA_V2)?,
        SCHEMA_VERSION => {}
        _ => return Err(CacheError::Sql(rusqlite::Error::InvalidQuery)),
    }
    conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
```

CRCs are stored upper-case by `replace_dat` (check; if not, normalise in queries with `upper(?1)`); every new query upper-cases its CRC argument. `dat_by_crc_any` = `SELECT dat, game, rom_name, size, crc32, md5, sha1 FROM dat_roms WHERE crc32 = ?1 AND (size IS NULL OR size = ?2)`. `cached_dats` = `SELECT DISTINCT dat FROM dat_roms ORDER BY dat`. `dat_stems` = `SELECT rom_stem, game FROM dat_roms WHERE dat = ?1`. The facts/choices accessors follow `quick_id` / `put_quick_id`.

- [ ] **Step 4: Run tests and gates** (`cargo test -p coverly-core --test cache` and the whole workspace: the v1 tests must still pass).

- [ ] **Step 5: Commit**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize && git add crates/coverly-core && git commit -m "feat(core): cache schema v2 for organise facts, choices and CRC lookups"
```

---

## Group O4 — Walk, evidence ladder, combine

### Task 11: Core — organise types and the read-only walk

**Files:**
- Create: `crates/coverly-core/src/organize/plan.rs` (types only in this task)
- Create: `crates/coverly-core/src/organize/walk.rs`
- Modify: `crates/coverly-core/src/organize/mod.rs` (`pub mod plan; pub mod walk;` + re-exports)
- Modify: `crates/coverly-core/src/scan/mod.rs` (`find_dir_ignoring_case` stays `pub(crate)`; used by walk/target)
- Test: `crates/coverly-core/tests/organize_walk.rs`

**Interfaces:**
- Produces (`organize::plan`, all `Serialize + Deserialize`, kebab-case):
  ```rust
  pub enum TransferMode { Move, Copy }                       // Default: Move
  pub enum NameMode { Keep, Catalogue }                      // Default: Keep
  pub enum ItemKind { File, Archive, Set }
  #[serde(tag = "kind")]
  pub enum LeftReason {
      PartialDownload, RecentlyModified, CloudOnly, NotARom { what: NotARom }, Bios,
      DamagedArchive, UnsupportedArchive, MultiGameArchive { count: usize },
      UnsupportedSystem { name: String }, Unknown, Duplicate { of: PathBuf },
      AlreadyPresent { at: PathBuf }, AlreadySorted { folder: String }, Symlink,
      SetIncomplete { missing: Vec<String> }, SetOutsideFolder, SharedTrack, AmbiguousCompanion,
      TooBigForFat, Unreadable, CollisionSet,
  }
  ```
- Produces (`organize::walk`):
  ```rust
  pub trait MetaProbe: Sync { fn cloud_only(&self, path: &Path, meta: &fs::Metadata) -> bool; }
  pub struct OsMetaProbe;                     // macOS SF_DATALESS, Windows RECALL_ON_DATA_ACCESS|OFFLINE
  pub struct WalkCandidate { pub path: PathBuf, pub ext: String, pub size: u64, pub mtime_ns: i64, pub cloud_only: bool }
  pub struct Walked {
      pub candidates: Vec<WalkCandidate>,
      pub left: Vec<(PathBuf, LeftReason)>,
      pub sorted_folders: Vec<(PathBuf, String)>,   // folder, system id
      pub deep_folders: Vec<PathBuf>,
      pub companions: Vec<PathBuf>,                 // docs/saves/images, decided by the planner
      pub other_files: Vec<PathBuf>,                // not ROM-like (O-D7)
      pub junk: usize,
  }
  pub struct WalkOptions<'a> {
      pub skip_dirs: Vec<PathBuf>,      // the target tree when it is inside the source
      pub roms_root: Option<PathBuf>,   // <T>/<roms_root> when inside the source: its system dirs are sorted
      pub art_dirs: &'a [String],
      pub ignored_dirs: &'a [String],
      pub now: SystemTime,
      pub recent: Duration,
      pub probe: &'a dyn MetaProbe,
      pub max_depth: usize,             // 4
  }
  pub fn walk(source: &Path, db: &SystemDb, opts: &WalkOptions) -> io::Result<Walked>;
  pub fn is_broad_root(path: &Path) -> bool;
  ```

- [ ] **Step 1: Write the failing tests** — `tests/organize_walk.rs`

```rust
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use coverly_core::organize::plan::LeftReason;
use coverly_core::organize::walk::{walk, MetaProbe, WalkOptions, Walked};
use coverly_core::systems::SystemDb;

struct NoCloud;
impl MetaProbe for NoCloud { fn cloud_only(&self, _: &Path, _: &fs::Metadata) -> bool { false } }
struct CloudNamed(&'static str);
impl MetaProbe for CloudNamed {
    fn cloud_only(&self, p: &Path, _: &fs::Metadata) -> bool { p.file_name().unwrap() == self.0 }
}

fn old(path: &Path) {
    let t = filetime::FileTime::from_system_time(SystemTime::now() - Duration::from_secs(3600));
    filetime::set_file_mtime(path, t).unwrap();
}

fn put(root: &Path, rel: &str, bytes: &[u8]) -> PathBuf {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(&p, bytes).unwrap();
    old(&p);
    p
}

fn run(root: &Path, probe: &dyn MetaProbe) -> Walked {
    let opts = WalkOptions {
        skip_dirs: vec![], roms_root: None, art_dirs: &[], ignored_dirs: &["bios".into(), "saves".into()],
        now: SystemTime::now(), recent: Duration::from_secs(60), probe, max_depth: 4,
    };
    walk(root, &SystemDb::builtin(), &opts).unwrap()
}

fn reason_of(w: &Walked, name: &str) -> Option<LeftReason> {
    w.left.iter().find(|(p, _)| p.file_name().unwrap() == name).map(|(_, r)| r.clone())
}

#[test]
fn partial_downloads_are_left_with_the_reason() {
    let tmp = tempfile::tempdir().unwrap();
    put(tmp.path(), "Tetris.zip.crdownload", b"x");
    put(tmp.path(), "Empty.gba", b"");
    let w = run(tmp.path(), &NoCloud);
    assert_eq!(reason_of(&w, "Tetris.zip.crdownload"), Some(LeftReason::PartialDownload));
    assert_eq!(reason_of(&w, "Empty.gba"), Some(LeftReason::PartialDownload), "zero bytes");
    assert!(w.candidates.is_empty());
}

#[test]
fn a_file_with_a_part_sibling_is_still_downloading() {
    let tmp = tempfile::tempdir().unwrap();
    put(tmp.path(), "x.zip", b"PK\x03\x04");
    put(tmp.path(), "x.zip.part", b"");
    let w = run(tmp.path(), &NoCloud);
    assert_eq!(reason_of(&w, "x.zip"), Some(LeftReason::PartialDownload));
}

#[test]
fn recently_modified_files_wait() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("fresh.gba");
    fs::write(&p, b"x").unwrap(); // mtime = now
    let w = run(tmp.path(), &NoCloud);
    assert_eq!(reason_of(&w, "fresh.gba"), Some(LeftReason::RecentlyModified));
}

#[test]
fn cloud_only_files_are_identified_by_name_only() {
    let tmp = tempfile::tempdir().unwrap();
    put(tmp.path(), "far.gba", b"x");
    let w = run(tmp.path(), &CloudNamed("far.gba"));
    assert_eq!(w.candidates.len(), 1);
    assert!(w.candidates[0].cloud_only);
}

#[test]
fn junk_bios_and_symlinks() {
    let tmp = tempfile::tempdir().unwrap();
    put(tmp.path(), ".DS_Store", b"x");
    put(tmp.path(), "._Game.gba", b"x");
    put(tmp.path(), "Game.gba:Zone.Identifier", b"x");
    put(tmp.path(), "gba_bios.bin", b"x");
    #[cfg(unix)]
    std::os::unix::fs::symlink(tmp.path().join("gba_bios.bin"), tmp.path().join("link.gba")).unwrap();
    let w = run(tmp.path(), &NoCloud);
    assert_eq!(w.junk, 3);
    assert_eq!(reason_of(&w, "gba_bios.bin"), Some(LeftReason::Bios));
    #[cfg(unix)]
    assert_eq!(reason_of(&w, "link.gba"), Some(LeftReason::Symlink));
}

#[test]
fn system_folders_are_not_walked_and_deep_folders_are_reported() {
    let tmp = tempfile::tempdir().unwrap();
    put(tmp.path(), "gba/Already.gba", b"x");
    put(tmp.path(), "GBA/Too.gba", b"x"); // same folder on case-insensitive volumes
    put(tmp.path(), "a/b/c/d/e/Deep.gba", b"x");
    put(tmp.path(), "a/Loose.gba", b"x");
    let w = run(tmp.path(), &NoCloud);
    assert!(w.sorted_folders.iter().all(|(_, id)| id == "gba"));
    assert!(!w.sorted_folders.is_empty());
    assert_eq!(w.candidates.iter().map(|c| c.path.file_name().unwrap().to_owned()).collect::<Vec<_>>(), ["Loose.gba"]);
    assert_eq!(w.deep_folders, [tmp.path().join("a/b/c/d")]);
}

#[test]
fn documents_are_companions_and_personal_files_are_other() {
    let tmp = tempfile::tempdir().unwrap();
    put(tmp.path(), "Game.gba", b"x");
    put(tmp.path(), "Game.sav", b"x");
    put(tmp.path(), "notes.txt", b"x");
    put(tmp.path(), "setup.dmg", b"x");
    let w = run(tmp.path(), &NoCloud);
    assert_eq!(w.candidates.len(), 1);
    assert_eq!(w.companions.len(), 2);
    assert_eq!(w.other_files.len(), 1);
}

#[test]
fn broad_roots() {
    let home = directories::BaseDirs::new().unwrap().home_dir().to_path_buf();
    assert!(coverly_core::organize::walk::is_broad_root(&home));
    assert!(coverly_core::organize::walk::is_broad_root(&home.join("Downloads")));
    assert!(coverly_core::organize::walk::is_broad_root(Path::new("/")));
    assert!(!coverly_core::organize::walk::is_broad_root(&home.join("Downloads/roms")));
}
```

- [ ] **Step 2: Run to see them fail.**

- [ ] **Step 3: Implement `walk.rs`**

Algorithm (write it as one function plus small helpers):

1. `WalkDir::new(source).follow_links(false).same_file_system(true).min_depth(1).max_depth(opts.max_depth)` with `filter_entry` that refuses: hidden dirs (`classify::is_hidden`), `ignored_dirs`/`art_dirs` (case-insensitive), any path in `skip_dirs`, and **directories that resolve to a system** (`db.resolve_folder(name)`), which are recorded in `sorted_folders` before being refused. Inside `opts.roms_root` the same rule applies (its children are system dirs), so loose files directly in `ROMS/` are still candidates.
2. Directories at depth `max_depth` that have any child → `deep_folders`.
3. For each file entry (`file_type()` from walkdir, **not** followed):
   - symlink (or Windows reparse point: `meta.file_attributes() & 0x400`) → `left(Symlink)`;
   - `is_appledouble || JUNK_FILES || is_zone_identifier` → `junk += 1`;
   - `is_partial_download(name)` or `len == 0` or a sibling `<name>.part` exists (check with one `HashSet` of names per directory, built lazily) → `left(PartialDownload)`;
   - `is_bios_name(name)` → `left(Bios)`;
   - `now - mtime < recent` → `left(RecentlyModified)`;
   - ROM-like extension (`db.is_rom_like_extension(ext)`) → `candidates` with `cloud_only = probe.cloud_only(path, &meta)`;
   - extension in `COMPANION_EXTS` (or `state*`) or `ScanOptions::default().ignored_exts` → `companions`;
   - else → `other_files`.
4. Walk errors (permission denied on a subfolder) are skipped; an unreadable `source` itself is the `io::Error` returned.
5. Sort every list by path for deterministic plans.

`OsMetaProbe::cloud_only`:

```rust
impl MetaProbe for OsMetaProbe {
    #[allow(unused_variables)]
    fn cloud_only(&self, path: &Path, meta: &fs::Metadata) -> bool {
        #[cfg(target_os = "macos")]
        {
            use std::os::macos::fs::MetadataExt;
            const SF_DATALESS: u32 = 0x4000_0000;
            return meta.st_flags() & SF_DATALESS != 0;
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            const OFFLINE: u32 = 0x1000;
            const RECALL_ON_DATA_ACCESS: u32 = 0x40_0000;
            return meta.file_attributes() & (OFFLINE | RECALL_ON_DATA_ACCESS) != 0;
        }
        #[allow(unreachable_code)]
        false
    }
}
```

`is_broad_root(path)`: true for `/`, a Windows drive root (`C:\`), a direct child of `/Volumes`, `/media/<user>/<x>` or `/run/media/<user>/<x>`, the home dir, and `home/{Desktop,Documents,Downloads}` (compare with `std::path::absolute` + case-insensitive on macOS/Windows). Uses `directories::BaseDirs`.

- [ ] **Step 4: Run tests and gates.**

- [ ] **Step 5: Commit**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize && git add crates/coverly-core && git commit -m "feat(core): walk a ROM pile read-only and classify what stays put"
```

---

### Task 12: Core — `combine`: scores, margin, content support, vetoes, thresholds

**Files:**
- Create: `crates/coverly-core/src/organize/evidence.rs` (pure part)
- Modify: `crates/coverly-core/src/organize/mod.rs`
- Test: `crates/coverly-core/tests/organize_evidence.rs`

**Interfaces:**
- Produces:
  ```rust
  #[derive(Clone, Copy, Debug, PartialEq)]
  pub struct Thresholds { pub auto: f32, pub review: f32 }        // Default: 0.85 / 0.50
  #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)] #[serde(rename_all = "kebab-case")]
  pub enum Verdict { Auto, Review, Unknown }
  #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
  pub struct Combined {
      pub verdict: Verdict,
      pub system: Option<String>,
      pub confidence: f32,
      pub method: Option<IdLevel>,
      pub why: Vec<Evidence>,               // strongest first, ≤ 4
      pub alternatives: Vec<(String, f32)>, // other systems ≥ 0.40, ordered
  }
  pub struct CombineInput<'a> {
      pub evidence: &'a [Evidence],
      pub excluded: &'a [String],
      pub content_read: bool,  // the sniff read bytes (not cloud-only, not a 7z/CHD)
      pub broad_root: bool,
      pub thresholds: Thresholds,
  }
  pub fn combine(input: &CombineInput) -> Combined;
  pub fn extension_evidence(db: &SystemDb, ext: &str, sniff: &Sniff) -> Vec<Evidence>;   // unique 0.85 / shared 0.50 / negative 0.60
  ```

- [ ] **Step 1: Write the failing table-driven tests** — `tests/organize_evidence.rs`

```rust
use coverly_core::identify::evidence::{Evidence, IdLevel::*};
use coverly_core::identify::sniff::{fixtures as fx, sniff, SliceAt};
use coverly_core::organize::evidence::{combine, extension_evidence, CombineInput, Thresholds, Verdict};
use coverly_core::systems::SystemDb;

fn ev(kind: coverly_core::identify::evidence::IdLevel, system: &str, w: f32) -> Evidence {
    Evidence::new(kind, system, w, "")
}

fn decide(evidence: &[Evidence], excluded: &[&str]) -> (Verdict, Option<String>) {
    let excluded: Vec<String> = excluded.iter().map(|s| s.to_string()).collect();
    let c = combine(&CombineInput {
        evidence, excluded: &excluded, content_read: true, broad_root: false, thresholds: Thresholds::default(),
    });
    (c.verdict, c.system)
}

#[test]
fn a_dat_hit_wins() {
    assert_eq!(decide(&[ev(Hash, "gb", 1.0), ev(Extension, "gb", 0.5), ev(Extension, "gbc", 0.5)], &[]),
               (Verdict::Auto, Some("gb".into())));
}

#[test]
fn gb_or_gbc_by_the_cgb_flag() {
    assert_eq!(decide(&[ev(Header, "gb", 0.95), ev(Extension, "gb", 0.5), ev(Extension, "gbc", 0.5)], &[]),
               (Verdict::Auto, Some("gb".into())));
    assert_eq!(decide(&[ev(Header, "gbc", 0.85), ev(Extension, "gb", 0.5), ev(Extension, "gbc", 0.5)], &[]),
               (Verdict::Auto, Some("gbc".into())), "dual-mode → GBC, GB as alternative");
}

#[test]
fn gbc_only_header_never_goes_to_gb() {
    let (v, s) = decide(&[ev(Header, "gbc", 0.95), ev(Hash, "gb", 1.0)], &["gb"]);
    assert_eq!((v, s.as_deref()), (Verdict::Auto, Some("gbc")));
}

#[test]
fn a_shared_extension_without_header_needs_review() {
    assert_eq!(decide(&[ev(Extension, "gb", 0.5), ev(Extension, "gbc", 0.5)], &[]).0, Verdict::Review);
}

#[test]
fn extension_and_name_alone_never_reach_auto() {
    // A unique extension of a system with a signature, no header hit: not content support.
    let c = combine(&CombineInput {
        evidence: &[ev(Extension, "gba", 0.85), ev(Name, "gba", 0.80)], excluded: &[], content_read: false,
        broad_root: false, thresholds: Thresholds::default(),
    });
    assert_eq!(c.verdict, Verdict::Review);
}

#[test]
fn a_unique_extension_of_a_system_without_signature_is_enough() {
    let (v, s) = decide(&[ev(Extension, "pcengine", 0.85)], &[]);
    assert_eq!((v, s.as_deref()), (Verdict::Auto, Some("pcengine")));
}

#[test]
fn a_contradicting_sniff_downgrades_a_unique_extension() {
    let db = SystemDb::builtin();
    let bytes = fx::zeros(64 * 1024); // no GBA header
    let s = sniff(&mut SliceAt(&bytes), bytes.len() as u64, "gba");
    let e = extension_evidence(&db, "gba", &s);
    assert_eq!(e.len(), 1);
    assert!((e[0].weight - 0.60).abs() < 1e-6);
    assert_eq!(decide(&e, &[]).0, Verdict::Review);
}

#[test]
fn the_margin_rule() {
    assert_eq!(decide(&[ev(Header, "mastersystem", 0.90), ev(Header, "gamegear", 0.85)], &[]).0, Verdict::Review);
}

#[test]
fn two_catalogues_for_one_crc_need_review() {
    assert_eq!(decide(&[ev(Hash, "gb", 1.0), ev(Hash, "gbc", 1.0)], &[]).0, Verdict::Review);
}

#[test]
fn nothing_above_half_is_unknown() {
    assert_eq!(decide(&[ev(Name, "gba", 0.40)], &[]).0, Verdict::Unknown);
    assert_eq!(decide(&[], &[]).0, Verdict::Unknown);
}

#[test]
fn broad_roots_drop_reviews_without_content_evidence() {
    let c = combine(&CombineInput {
        evidence: &[ev(Extension, "gb", 0.5), ev(Extension, "gbc", 0.5)], excluded: &[], content_read: true,
        broad_root: true, thresholds: Thresholds::default(),
    });
    assert_eq!(c.verdict, Verdict::Unknown);
}

#[test]
fn agreement_adds_a_small_bonus_and_why_is_strongest_first() {
    let c = combine(&CombineInput {
        evidence: &[ev(Extension, "gba", 0.85), ev(Header, "gba", 0.95), ev(Hash, "gba", 1.0)],
        excluded: &[], content_read: true, broad_root: false, thresholds: Thresholds::default(),
    });
    assert_eq!(c.confidence, 1.0);
    assert_eq!(c.method, Some(Hash));
    assert_eq!(c.why[0].kind, Hash);
}
```

- [ ] **Step 2: Run to see them fail.**

- [ ] **Step 3: Implement**

```rust
impl Default for Thresholds {
    fn default() -> Self { Thresholds { auto: 0.85, review: 0.50 } }
}

const BONUS: f32 = 0.03;
const MARGIN: f32 = 0.10;
const CANDIDATE: f32 = 0.40;

pub fn combine(input: &CombineInput) -> Combined {
    use std::collections::BTreeMap;
    let mut by_system: BTreeMap<&str, Vec<&Evidence>> = BTreeMap::new();
    for e in input.evidence.iter().filter(|e| !input.excluded.iter().any(|x| *x == e.system)) {
        by_system.entry(e.system.as_str()).or_default().push(e);
    }
    let mut scored: Vec<(&str, f32, bool)> = by_system
        .iter()
        .map(|(system, evs)| {
            let max = evs.iter().map(|e| e.weight).fold(0.0f32, f32::max);
            let mut levels: Vec<IdLevel> = evs.iter().map(|e| e.kind).collect();
            levels.sort();
            levels.dedup();
            let score = (max + BONUS * (levels.len() as f32 - 1.0)).min(1.0);
            (*system, score, has_content_support(system, evs, input.content_read))
        })
        .collect();
    scored.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(b.0)));
    let Some(&(best, score, content)) = scored.first() else {
        return Combined::unknown();
    };
    let second = scored.get(1).map_or(0.0, |s| s.1);
    let verdict = if score >= input.thresholds.auto && score - second >= MARGIN && content {
        Verdict::Auto
    } else if score >= input.thresholds.review && !(input.broad_root && !content) {
        Verdict::Review
    } else {
        Verdict::Unknown
    };
    let mut why: Vec<Evidence> = by_system[best].iter().map(|e| (*e).clone()).collect();
    why.sort_by(|a, b| a.kind.cmp(&b.kind).then(b.weight.total_cmp(&a.weight)));
    why.truncate(4);
    Combined {
        verdict,
        system: (verdict != Verdict::Unknown || score >= CANDIDATE).then(|| best.to_string()),
        confidence: score,
        method: why.first().map(|e| e.kind),
        why,
        alternatives: scored.iter().skip(1).filter(|s| s.1 >= CANDIDATE).map(|s| (s.0.to_string(), s.1)).collect(),
    }
}

/// A DAT hit, header evidence ≥ 0.85, a user choice, or a unique extension
/// of a system without a decisive signature whose contents were read.
fn has_content_support(system: &str, evs: &[&Evidence], content_read: bool) -> bool {
    evs.iter().any(|e| match e.kind {
        IdLevel::Hash | IdLevel::User => true,
        IdLevel::Header => e.weight >= 0.85,
        IdLevel::Extension => content_read && e.weight >= 0.85 && !DECISIVE.contains(&system),
        IdLevel::Name | IdLevel::Online => false,
    })
}

pub fn extension_evidence(db: &SystemDb, ext: &str, sniff: &Sniff) -> Vec<Evidence> {
    let class = db.extension_class(ext);
    let weight = match class {
        ExtClass::Unique => 0.85,
        ExtClass::Shared => 0.50,
        _ => return Vec::new(),
    };
    let read_enough = sniff.read >= 0x200 && sniff.not_a_rom.is_none();
    db.systems_for_extension(ext)
        .into_iter()
        .map(|s| {
            let contradicted = class == ExtClass::Unique
                && read_enough
                && DECISIVE.contains(&s.id.as_str())
                && !sniff.hits.iter().any(|h| h.system == s.id);
            let w = if contradicted { 0.60 } else { weight };
            let detail = if contradicted { format!("{ext}!") } else { ext.to_string() };
            Evidence::new(IdLevel::Extension, &s.id, w, detail)
        })
        .collect()
}
```

(The `detail` suffix `!` marks "the extension says X but the contents don't look like X"; Task 16 turns it into `ItemNote::ExtensionContradicted`.) `Combined::unknown()` is a small constructor. `gbc` unique `.gbc` + header GB-DMG: `.gbc` is unique to gbc? It is listed by `gbc` only → unique → `gbc` 0.85; header says `gb` 0.95 → gb wins with margin 0.10 → Auto `gb` (harmless direction, spec §6.3).

- [ ] **Step 4: Run tests and gates.**

- [ ] **Step 5: Commit**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize && git add crates/coverly-core && git commit -m "feat(core): combine identification evidence with margins and vetoes"
```

---

### Task 13: Core — `Catalogue` over cached DATs (CRC hits, stems, arcade sets, download cap, offline)

**Files:**
- Modify: `crates/coverly-core/src/organize/evidence.rs`
- Test: `crates/coverly-core/tests/organize_evidence.rs` (append)

**Interfaces:**
- Produces:
  ```rust
  #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
  pub struct CatalogueHit { pub system: String, pub game: String, pub rom_name: String }
  pub trait Catalogue: Sync {
      fn hits(&self, systems: &[String], crc32: &str, size: u64) -> Vec<CatalogueHit>;
      fn any_hits(&self, crc32: &str, size: u64) -> Vec<CatalogueHit>;
      fn stems(&self, system: &str) -> Vec<(String, String)>;        // (rom stem, game)
      fn stem_systems(&self) -> Vec<String>;                          // systems with a cached DAT
      fn set_roms(&self, system: &str, set: &str) -> Option<(String, Vec<(String, u64)>)>; // game, (crc, size)
      fn unconsulted(&self) -> Vec<String>;                           // systems whose DAT was not available
  }
  pub struct DatCatalogue<'a> { /* db, cache, store: Option<&DatStore>, cap, state */ }
  impl<'a> DatCatalogue<'a> {
      pub fn new(db: &'a SystemDb, cache: &'a Cache, store: Option<&'a DatStore<'a>>, max_downloads: usize) -> Self;
  }
  pub const MAX_DAT_DOWNLOADS: usize = 4;
  ```

- [ ] **Step 1: Write the failing tests**

```rust
use coverly_core::cache::{Cache, DatRow};
use coverly_core::identify::dat::{dat_sources, DatStore};
use coverly_core::net::testing::MockServer;
use coverly_core::organize::evidence::{Catalogue, DatCatalogue};

fn row(game: &str, rom: &str, size: u64, crc: &str) -> DatRow {
    DatRow { game: game.into(), rom_name: rom.into(), size: Some(size), crc32: Some(crc.into()), md5: None, sha1: None }
}

#[test]
fn offline_uses_cached_dats_only_and_reports_the_rest() {
    let db = SystemDb::builtin();
    let cache = Cache::in_memory().unwrap();
    let gb_dat = dat_sources(db.get("gb").unwrap()).remove(0);
    cache.replace_dat(&gb_dat, &[row("Tetris (World)", "Tetris (World).gb", 32768, "46DF91AD")]).unwrap();
    let cat = DatCatalogue::new(&db, &cache, None, 4);
    let hits = cat.hits(&["gb".into(), "gbc".into()], "46df91ad", 32768);
    assert_eq!(hits[0].system, "gb");
    assert_eq!(hits[0].game, "Tetris (World)");
    assert_eq!(cat.unconsulted(), ["gbc"]);
}

#[test]
fn missing_dats_are_downloaded_up_to_the_cap() {
    let db = SystemDb::builtin();
    let cache = Cache::in_memory().unwrap();
    let server = MockServer::start();
    for id in ["gb", "gbc", "gba", "snes", "nes"] {
        for dat in dat_sources(db.get(id).unwrap()) {
            server.serve_dat(&dat, "game (\n name \"X\"\n rom ( name \"x\" size 1 crc 00000001 )\n)\n");
        }
    }
    let http = server.http();
    let store = DatStore::new(&http, &cache, server.url(""));
    let cat = DatCatalogue::new(&db, &cache, Some(&store), 2);
    let systems: Vec<String> = ["gb", "gbc", "gba", "snes", "nes"].iter().map(|s| s.to_string()).collect();
    let _ = cat.hits(&systems, "00000001", 1);
    assert_eq!(cat.unconsulted().len(), 3, "only two downloads per plan");
}

#[test]
fn arcade_sets_by_name_with_their_roms() {
    let db = SystemDb::builtin();
    let cache = Cache::in_memory().unwrap();
    let fb = dat_sources(db.get("fbneo").unwrap()).remove(0);
    cache.replace_dat(&fb, &[row("sf2", "sf2-1.bin", 4, "11111111"), row("sf2", "sf2-2.bin", 4, "22222222")]).unwrap();
    let cat = DatCatalogue::new(&db, &cache, None, 0);
    let (game, roms) = cat.set_roms("fbneo", "SF2").unwrap();
    assert_eq!(game, "sf2");
    assert_eq!(roms.len(), 2);
}
```

(Check `MockServer::{http, url}` names in `net/testing.rs` and the DAT text format accepted by `parse_clrmamepro`; adapt the helper names, not the assertions. For arcade DATs, check how `parse_clrmamepro`/`replace_dat` fill `game` and `rom_stem` and look the set up by whichever column holds the set's short name — `arcade_description` already does this via `dat_by_rom_stem`; `set_roms` returns all rows of that game through `dat_rows_of_game`.)

- [ ] **Step 2: Run to see them fail.**

- [ ] **Step 3: Implement `DatCatalogue`**

State (behind `Mutex`): `ready: HashMap<String /*dat*/, bool>`, `downloads: usize`, `unconsulted: BTreeSet<String /*system*/>`. `ensure_system(id)`:
for each `dat` in `dat_sources(system)`: if `ready` has it → use; else if the cache already has rows (`cached_dats()` snapshot taken in `new`) → ready; else if `store` is `Some` and `downloads < cap` → `downloads += 1; store.ensure(dat)` (`Ready | Stale` → ready, `Missing` or `Err` → not); else not ready. A system with no ready DAT is added to `unconsulted`. `hits` = for each system, for each ready dat, `cache.dat_by_crc(dat, crc)` filtered by size (`r.size.is_none_or(|s| s == size)`), mapped to `CatalogueHit`. `any_hits` = `cache.dat_by_crc_any` mapped with `db.system_for_dat`. `stems(system)` = union over ready DATs of `cache.dat_stems`. `stem_systems` = systems of `cached_dats()`. All errors degrade to "no hit" (logged with `log::warn!` only in the app; the core returns empty).

- [ ] **Step 4: Run tests and gates.**

- [ ] **Step 5: Commit**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize && git add crates/coverly-core && git commit -m "feat(core): consult cached and on-demand DATs for organise"
```

---

### Task 14: Core — the identification ladder per file (`identify_candidate`)

**Files:**
- Modify: `crates/coverly-core/src/organize/evidence.rs`
- Test: `crates/coverly-core/tests/organize_evidence.rs` (append)

**Interfaces:**
- Consumes: `inspect::{Inspection, Payload, Container, ArchiveIssue}`, `Catalogue`, `combine`, `identify::hash::{hash_rom, HashOutcome}`, `identify::names::{parse_name, strip_numbering_prefix, score}`.
- Produces:
  ```rust
  /// What is known about one candidate before any DAT is consulted:
  /// memoised in `organize_ids` (O-D6).
  #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
  pub struct Facts { pub inspection: Inspection, pub raw_crcs: Vec<String>, pub hashed: bool }
  #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
  pub struct Identification {
      pub combined: Combined,
      pub left: Option<LeftReason>,           // decided before combining (not a ROM, damaged, …)
      pub catalogue_name: Option<String>,     // DAT game name of the deciding hit
      pub content_key: Option<(String, u64)>, // CRC32 + size for duplicates and remembered choices
      pub rom_count: usize,                   // ROM entries in a zip (multi-ROM note)
      pub notes: Vec<ItemNote>,
  }
  pub struct LadderEnv<'a> {
      pub db: &'a SystemDb, pub catalogue: &'a dyn Catalogue, pub cache: &'a Cache,
      pub thresholds: Thresholds, pub broad_root: bool, pub need_hash: bool,
      pub arcade_order: &'a [String], pub online: Option<&'a dyn OnlineLookup>,
  }
  pub trait OnlineLookup: Sync {                 // implemented in Task 24
      fn lookup(&self, systems: &[String], file_name: &str, crc32: &str, size: u64) -> Vec<(String, String)>;
  }
  pub fn identify_candidate(path: &Path, outer_name: &str, size: u64, facts: &mut Facts, cloud_only: bool,
                            env: &LadderEnv) -> Identification;
  pub fn scene_group_hint(name: &str) -> Option<&'static str>;  // "gba" for Venom/Paracox/…
  ```
  `ItemNote` is defined in `plan.rs` (Task 11 types, extend there): `MultiRomZip { count }`, `NoFolderOnDevice { folder }`, `ExtensionContradicted { ext }`, `CatalogueMissing { system }`, `Encrypted`.

- [ ] **Step 1: Write the failing tests** (a `FakeCatalogue` in the test file implements `Catalogue` from a `Vec<(system, crc, size, game)>` and stems)

```rust
struct FakeCatalogue { rows: Vec<(&'static str, String, u64, &'static str)>, stems: Vec<(&'static str, &'static str)> }
impl Catalogue for FakeCatalogue { /* filter rows / stems; set_roms None; unconsulted empty */ }

fn identify(path: &Path, cat: &FakeCatalogue) -> Identification {
    let db = SystemDb::builtin();
    let cache = Cache::in_memory().unwrap();
    let len = std::fs::metadata(path).unwrap().len();
    let mut facts = Facts { inspection: inspect_file(path, len, &db).unwrap(), raw_crcs: vec![], hashed: false };
    let env = LadderEnv { db: &db, catalogue: cat, cache: &cache, thresholds: Thresholds::default(), broad_root: false,
                          need_hash: false, arcade_order: &["fbneo".into(), "mame".into()], online: None };
    identify_candidate(path, path.file_name().unwrap().to_str().unwrap(), len, &mut facts, false, &env)
}

#[test]
fn scene_named_gba_zip_is_decided_by_crc_and_header() {
    let tmp = tempfile::tempdir().unwrap();
    let rom = fx::gba();
    let crc = format!("{:08X}", crc32fast::hash(&rom));
    let p = tmp.path().join("1490 - Sonic Advance 3 (U)(Venom).zip");
    zip_file(&p, &[("1490 - Sonic Advance 3 (U)(Venom).gba", &rom, true)]);
    let id = identify(&p, &FakeCatalogue { rows: vec![("gba", crc, rom.len() as u64, "Sonic Advance 3 (USA)")], stems: vec![] });
    assert_eq!(id.combined.verdict, Verdict::Auto);
    assert_eq!(id.combined.system.as_deref(), Some("gba"));
    assert_eq!(id.combined.method, Some(IdLevel::Hash));
    assert_eq!(id.catalogue_name.as_deref(), Some("Sonic Advance 3 (USA)"));
}

#[test]
fn without_any_dat_the_header_still_decides() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("Tetris DX.zip");
    zip_file(&p, &[("Tetris DX.gbc", &fx::gb(0xC0), true)]);
    let id = identify(&p, &FakeCatalogue { rows: vec![], stems: vec![] });
    assert_eq!((id.combined.verdict, id.combined.system.as_deref()), (Verdict::Auto, Some("gbc")));
}

#[test]
fn same_title_different_systems() {
    let tmp = tempfile::tempdir().unwrap();
    let snes = tmp.path().join("Tetris 2.zip");
    zip_file(&snes, &[("Tetris 2.sfc", &fx::snes(fx::SnesMap::Lo, false), true)]);
    let gb = tmp.path().join("Tetris 2 (1).zip");
    zip_file(&gb, &[("Tetris 2.gb", &fx::gb(0x00), true)]);
    let none = FakeCatalogue { rows: vec![], stems: vec![] };
    assert_eq!(identify(&snes, &none).combined.system.as_deref(), Some("snes"));
    assert_eq!(identify(&gb, &none).combined.system.as_deref(), Some("gb"));
}

#[test]
fn a_web_page_and_a_collection_are_left() {
    let tmp = tempfile::tempdir().unwrap();
    let html = tmp.path().join("3-D Tetris (USA).zip");
    std::fs::write(&html, fx::html_page()).unwrap();
    assert!(matches!(identify(&html, &FakeCatalogue { rows: vec![], stems: vec![] }).left,
                     Some(LeftReason::NotARom { what: NotARom::WebPage })));
    let mixed = tmp.path().join("mixed.zip");
    zip_file(&mixed, &[("a.gb", &fx::gb(0), false), ("b.gba", &fx::gba(), false)]);
    assert!(matches!(identify(&mixed, &FakeCatalogue { rows: vec![], stems: vec![] }).left,
                     Some(LeftReason::MultiGameArchive { count: 2 })));
}

#[test]
fn two_versions_of_one_game_in_one_zip_are_allowed_with_a_note() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("Tetris (JUE).zip");
    zip_file(&p, &[("Tetris (V1.0).gb", &fx::gb(0), false), ("Tetris (V1.1).gb", &fx::gb(0), false)]);
    let id = identify(&p, &FakeCatalogue { rows: vec![], stems: vec![] });
    assert_eq!(id.combined.system.as_deref(), Some("gb"));
    assert!(id.notes.contains(&ItemNote::MultiRomZip { count: 2 }));
}

#[test]
fn a_seven_zip_is_named_only_and_never_auto() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("Tetris (World).7z");
    std::fs::write(&p, [0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C, 0, 4]).unwrap();
    let cat = FakeCatalogue { rows: vec![], stems: vec![("gb", "Tetris (World)")] };
    let id = identify(&p, &cat);
    assert_eq!(id.combined.verdict, Verdict::Review);
    assert_eq!(id.combined.system.as_deref(), Some("gb"));
}

#[test]
fn raw_files_are_hashed_only_when_needed() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("Game.gba");
    std::fs::write(&p, fx::gba()).unwrap();
    let id = identify(&p, &FakeCatalogue { rows: vec![], stems: vec![] });
    assert_eq!(id.combined.verdict, Verdict::Auto, "header decides");
    assert_eq!(id.content_key, None, "no hashing for an Auto raw file");
}

#[test]
fn scene_groups_hint_gba() {
    assert_eq!(scene_group_hint("0319 - Crash Bandicoot XS (E)(Paracox)"), Some("gba"));
    assert_eq!(scene_group_hint("Pokemon Verde Hoja (S)(Rising Sun)"), Some("gba"));
    assert_eq!(scene_group_hint("Tetris (World)"), None);
}

#[test]
fn a_remembered_choice_settles_the_same_content() {
    // cache.put_organize_choice(crc, size, "gbc") then identify a zip with that CRC and only a `.gb` ext
    // → verdict Auto, method User.
}
```

(Write the last test fully; it needs `LadderEnv.cache` to be a real in-memory cache with the choice stored.)

- [ ] **Step 2: Run to see them fail.**

- [ ] **Step 3: Implement `identify_candidate`** in this order (each step adds to `evidence: Vec<Evidence>` and `excluded`):

1. **Pre-checks from the inspection**: `issue` → `left`: `NotARom` → `NotARom{what}`, `Damaged` → `DamagedArchive`, `Unsupported` → `UnsupportedArchive`, `TooManyRoms` → `MultiGameArchive{count}`; a payload `sniff.unsupported` → `UnsupportedSystem{name}`; a raw payload `sniff.not_a_rom` → `NotARom`. Return early with `Combined::unknown()`.
2. **Remembered choice**: if the content key (single payload with CRC) has `cache.organize_choice` → `Evidence(User, system, 1.0, "remembered")`.
3. **Per payload** (zip: each ROM entry; raw: the file): `extension_evidence(db, &payload.ext, &payload.sniff)`, header hits, `excluded`. For zips with several payloads, identify each payload separately (`combine` on its own evidence) and require them to agree on one system: disagree → `left = MultiGameArchive{count}`; agree → keep the union of evidence and `notes.push(MultiRomZip{count})` when `count > 1`.
4. **Name signals** on the outer stem and inner stem (`strip_numbering_prefix` then `parse_name`): tags `(SGB Enhanced)` → `gb`, `gbc` 0.60; `(GB Compatible)` → `gbc` 0.60; `scene_group_hint` → 0.60. For `Container::SevenZip | Chd` the outer name's extension-less stem also feeds step 7.
5. **DAT by CRC** (zip payload CRCs, or `facts.raw_crcs` when already hashed): candidates = systems with any evidence so far; `catalogue.hits(candidates, crc, size)` → `Hash 1.00`, detail `"{crc} {game}"`; if no hit, `catalogue.any_hits(crc, size)` → `Hash 0.97`. When several systems hit and header evidence names some of them, keep only those (spec §6.2). The deciding hit's game becomes `catalogue_name`.
6. **Raw hashing** (only `Container::Raw`, not cloud-only, not already `hashed`): when `combine` so far is not `Auto`, or `env.need_hash` (catalogue names / duplicate bucket): `hash_rom(path, ext, system.header rule of the best candidate (or Plain), DEFAULT_HASH_MAX_BYTES)`; store `raw_crcs` (full + headerless), `hashed = true`, repeat step 5. Set `content_key` from the full digest.
7. **Fuzzy name** when still not `Auto`: systems = candidates, or `catalogue.stem_systems()` when there are none; for each `(stem, game)` compute `names::score(key, stem_key)`; best ≥ 95 → `Name 0.80`, ≥ 85 → `Name 0.70`, detail = the game.
8. **Arcade** (zip with no payloads but `entries`): for each `system` in `env.arcade_order`: `catalogue.set_roms(system, zip stem)`; confirmed when ≥ 75 % of the set's ROM CRCs are among the zip entries → `Hash 1.00`; name only → `Name 0.70`. The first confirmed system in order wins; later confirmed ones become alternatives with `Hash 0.99`. No set anywhere → `Unknown`.
9. **Online** (`env.online`, Task 24) when still not `Auto` and a CRC is known: hits → `Online 0.80`.
10. `combine(...)` with `content_read = !cloud_only && payload sniff read > 0 && container is Raw|Zip`.
11. `ItemNote::ExtensionContradicted` when an extension evidence detail ends with `!`; `CatalogueMissing{system}` for the chosen system when it is in `catalogue.unconsulted()`.

`scene_group_hint`: case-insensitive search for one of `["Venom", "Paracox", "BatMan", "Independent", "Rising Sun", "Eurasia", "Trashman", "Mode7", "Megaroms", "Suxxors", "Menace", "LightForce"]` inside parentheses.

- [ ] **Step 4: Run tests and gates.**

- [ ] **Step 5: Commit**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize && git add crates/coverly-core && git commit -m "feat(core): identify each candidate with the hash-header-extension-name ladder"
```

---

## Group O5 — Plan

### Task 15: Core — `organize::target`: folders, target paths, collisions, duplicates, companions, sets

**Files:**
- Create: `crates/coverly-core/src/organize/target.rs`
- Modify: `crates/coverly-core/src/organize/plan.rs` (item types)
- Modify: `crates/coverly-core/src/write/mod.rs` (`find_twin` → `pub(crate)`; new `pub(crate) fn name_key(&str) -> String` = NFC + lowercase)
- Test: `crates/coverly-core/tests/organize_target.rs`

**Interfaces:**
- Produces (in `plan.rs`):
  ```rust
  #[serde(tag = "kind", rename_all = "kebab-case")]
  pub enum RenameReason { Collision { name: String }, SafeName { from: String }, PathLength, Catalogue { from: String } }
  pub struct Companion { pub source: PathBuf, pub target: PathBuf }   // both relative (source to S, target to T)
  pub struct SourceFp { pub size: u64, pub mtime_ns: i64 }
  #[serde(tag = "action", rename_all = "kebab-case")]
  pub enum OrganizeAction { Auto, Review, Left { reason: LeftReason } }
  pub struct OrganizeItem {
      pub id: usize,
      pub sources: Vec<PathBuf>,          // relative to the source root; primary first
      pub kind: ItemKind,
      pub size: u64,
      pub fps: Vec<SourceFp>,             // one per source, for re-validation at apply
      pub target: Option<PathBuf>,        // relative to the target root (primary)
      pub set_targets: Vec<PathBuf>,      // other members of a set, same order as sources[1..]
      pub system_id: Option<String>,
      pub folder: Option<String>,
      pub confidence: f32,
      pub method: Option<IdLevel>,
      pub why: Vec<Evidence>,
      pub alternatives: Vec<(String, f32)>,
      pub rename: Option<RenameReason>,
      pub notes: Vec<ItemNote>,
      pub companions: Vec<Companion>,
      pub catalogue_name: Option<String>,
      #[serde(flatten)] pub action: OrganizeAction,
  }
  ```
- Produces (in `target.rs`):
  ```rust
  pub struct TargetLayout {
      pub target_root: PathBuf, pub roms_dir: PathBuf,       // absolute; roms_dir = T/roms_root or T
      pub profile: Profile, pub overrides: BTreeMap<String, String>,
      pub fat32: bool, pub windows_paths: bool, pub names: NameMode, pub companion_exts: Vec<String>,
  }
  pub struct DirIndex { /* HashMap<PathBuf, HashSet<String>> of name keys */ }
  impl DirIndex { pub fn new() -> Self; pub fn taken(&mut self, dir: &Path, name: &str) -> bool; pub fn claim(&mut self, dir: &Path, name: &str); pub fn read_dirs(&self) -> usize; }
  pub fn folder_dir(layout: &TargetLayout, db: &SystemDb, system_id: &str) -> (PathBuf, FolderChoice); // reuses an existing dir in any casing
  pub fn assign_targets(items: &mut [OrganizeItem], layout: &TargetLayout, db: &SystemDb, source_root: &Path,
                        companions: &[PathBuf], index: &mut DirIndex, same_content: &dyn Fn(&Path, &OrganizeItem) -> bool);
  pub fn mark_duplicates(items: &mut [OrganizeItem], keys: &[Option<(String, u64)>]);
  pub fn file_system_is_fat32(path: &Path) -> bool;
  ```

- [ ] **Step 1: Write the failing tests** — `tests/organize_target.rs` (build `OrganizeItem`s with a helper `auto(id, source, system)`; layout = Anbernic stock under a tempdir)

```rust
#[test]
fn targets_follow_the_layout_and_reuse_existing_folders_in_any_case() {
    // T/ROMS/gba exists → GBA item goes to ROMS/gba/…, GB item to ROMS/GB/…
}

#[test]
fn flat_layout_puts_system_folders_directly_in_the_target() { /* roms_dir = T → GBA/x.gba */ }

#[test]
fn two_sources_to_one_target_get_numbered_names() {
    // "Tetris.gb" in two subfolders, both gb → "ROMS/GB/Tetris.gb" and "ROMS/GB/Tetris (2).gb"
}

#[test]
fn an_existing_different_file_gets_a_numbered_name_and_the_same_file_is_already_present() {
    // existing ROMS/GB/Tetris.gb with other bytes → (2); same bytes → Left AlreadyPresent { at }
}

#[test]
fn case_twins_collide() {
    // existing ROMS/GB/tetris.GB → planned "Tetris.gb" becomes "Tetris (2).gb"
}

#[test]
fn unsafe_names_are_made_fat_safe_with_a_reason() {
    // source "Zelda: Link?.gb" (unix only) → "Zelda_ Link_.gb", rename SafeName { from }
}

#[test]
fn duplicates_by_content_keep_the_shortest_name() {
    // keys equal for "Tetris 2 (1).zip" and "Tetris 2.zip" → "(1)" left Duplicate { of: "Tetris 2.zip" }
}

#[test]
fn companions_follow_and_are_renamed_with_catalogue_names() {
    // Game.gba + Game.sav, names=Catalogue, catalogue_name "Game (USA)" → Game (USA).gba + Game (USA).sav
}

#[test]
fn a_companion_of_two_roms_in_different_folders_is_ambiguous() {
    // Tetris.gb (gb) + Tetris.gbc (gbc) + Tetris.sav → sav not attached; reported as AmbiguousCompanion in `left`
}

#[test]
fn a_set_moves_together_and_a_set_collision_skips_the_whole_set() {
    // Game.cue + Game.bin (psx) → ROMS/PS/Game.cue + ROMS/PS/Game.bin; with ROMS/PS/Game.bin existing (different)
    // → Left CollisionSet, no target
}

#[test]
fn too_big_for_fat32() {
    // layout.fat32 = true, item.size = 4 GiB → Left TooBigForFat
}

#[test]
fn long_windows_paths_are_shortened() {
    // windows_paths = true, a 250-char stem → target path ≤ 259 UTF-16 units, rename PathLength
}
```

Write each test body fully (the comments say exactly what to build and assert).

- [ ] **Step 2: Run to see them fail.**

- [ ] **Step 3: Implement**

Rules, in order, for every item whose action is `Auto` or `Review` with a chosen system (review items get targets too, so the dry run can show where they would go once confirmed):
1. `(dir, choice) = folder_dir(layout, db, system)`: `R/<choice.folder>` unless `find_dir_ignoring_case(R, folder)` finds an existing one (if both `gba/` and `GBA/` exist, the one equal to `choice.folder` wins). `choice.on_device == Some(false)` → `ItemNote::NoFolderOnDevice{folder}`.
2. Name: original file name, or with `NameMode::Catalogue` and a `catalogue_name` (not for sets) `"{catalogue_name}.{ext}"` (`RenameReason::Catalogue`); then `safe_file_name` (`SafeName` when changed); then on `windows_paths` the stem is cut until `T/…/name` ≤ 259 UTF-16 units (`PathLength`).
3. FAT32: `layout.fat32 && size >= 4 GiB` → `Left(TooBigForFat)`.
4. Collision against `DirIndex` (existing entries of `dir` + names claimed by earlier items): existing file with `same_content` → `Left(AlreadyPresent{at})`; else for single files next free `"{stem} ({n}).{ext}"` (`RenameReason::Collision`), checked again against the index; for sets any member collision → `Left(CollisionSet)`.
5. Companions: for each path in `companions` with the same parent dir and the same stem (`name_key` comparison) and an extension matching `companion_exts` (`state*` glob) → attached; renamed to the new stem when the ROM was renamed; a companion whose new name collides makes the whole item keep its original name (redo step 2 with `Keep`). A companion matching ROMs that go to different folders is reported through `left` as `AmbiguousCompanion` (returned in a separate `Vec<(PathBuf, LeftReason)>`).
6. Claim every final name in the index.

`mark_duplicates(items, keys)`: group items by `Some(key)`; within a group sort by `(file name length, file name)`; the first keeps its action; others become `Left(Duplicate{of: first.sources[0]})`.

`file_system_is_fat32(path)`: macOS `libc::statfs` → `f_fstypename == "msdos"`; Linux: longest mount point prefix of `path` in `/proc/self/mountinfo` with fs type `vfat`; Windows: `GetVolumeInformationW` file system name `FAT32`. Anything else / any error → `false`.

- [ ] **Step 4: Run tests and gates.**

- [ ] **Step 5: Commit**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize && git add crates/coverly-core && git commit -m "feat(core): plan organise targets without collisions"
```

---

### Task 16: Core — `plan_organize`, `survey`, `probe_folder`, decisions, memo, workers, progress

**Files:**
- Modify: `crates/coverly-core/src/organize/plan.rs`
- Modify: `crates/coverly-core/src/organize/mod.rs` (re-exports: `plan_organize`, `survey`, `plan_from_survey`, `probe_folder`, `OrganizeOptions`, `OrganizeEnv`, `OrganizeProgress`, `OrganizePhase`, `Decision`, `Choice`, `OrganizeError`)
- Create: `crates/coverly-core/tests/organize_plan.rs` (+ `insta` snapshots under `tests/snapshots/`)

**Interfaces:**
- Produces:
  ```rust
  pub struct OrganizeOptions {
      pub layout: String, pub target: Option<PathBuf>, pub flat: bool, pub mode: Option<TransferMode>,
      pub names: NameMode, pub offline: bool, pub thresholds: Thresholds,
      pub companion_exts: Vec<String>, pub folder_names: BTreeMap<String, String>,
      pub recent_secs: u64, pub arcade_order: Option<Vec<String>>, pub download_dats: bool,
  }
  impl OrganizeOptions { pub fn from_settings(s: &Settings, layout: Option<&str>) -> OrganizeOptions; }
  pub struct OrganizeEnv<'a> {
      pub db: &'a SystemDb, pub profiles: &'a [Profile], pub cache: &'a Cache,
      pub dats: Option<&'a DatStore<'a>>, pub probe: &'a dyn MetaProbe,
      pub online: Option<&'a dyn OnlineLookup>, pub now: SystemTime,
  }
  #[derive(Serialize)] #[serde(rename_all = "kebab-case")]
  pub enum OrganizePhase { Walk, Identify, Dats, Hash }
  #[derive(Serialize)] pub struct OrganizeProgress { pub phase: OrganizePhase, pub done: usize, pub total: usize }
  #[derive(Debug, thiserror::Error)]
  pub enum OrganizeError { #[error("source not found: {0}")] SourceMissing(PathBuf),
      #[error("cannot read {0}: {1}")] Unreadable(PathBuf, String), #[error("unknown layout `{0}`")] UnknownLayout(String),
      #[error("unknown system `{0}`")] UnknownSystem(String), #[error("cancelled")] Cancelled,
      #[error("item {0} is not in the plan")] UnknownItem(usize) }
  pub struct Survey { /* source_root, target_root, walked lists, per-candidate Identification + Facts, broad_root */ }
  pub fn survey(source: &Path, opts: &OrganizeOptions, env: &OrganizeEnv, progress: &(dyn Fn(OrganizeProgress) + Sync),
                cancel: &AtomicBool) -> Result<Survey, OrganizeError>;
  pub fn plan_from_survey(survey: &Survey, opts: &OrganizeOptions, decisions: &[Decision], env: &OrganizeEnv)
      -> Result<OrganizePlan, OrganizeError>;
  pub fn plan_organize(source: &Path, opts: &OrganizeOptions, env: &OrganizeEnv,
                       progress: &(dyn Fn(OrganizeProgress) + Sync), cancel: &AtomicBool) -> Result<OrganizePlan, OrganizeError>;
  #[derive(Clone, Debug, Serialize, Deserialize)]
  pub struct Decision { pub item: usize, #[serde(flatten)] pub choice: Choice }
  #[derive(Clone, Debug, Serialize, Deserialize)] #[serde(tag = "choice", rename_all = "kebab-case")]
  pub enum Choice { Accept, Assign { system: String }, Leave }
  #[derive(Clone, Debug, Default, Serialize, Deserialize)]
  pub struct Counts { pub roms: usize, pub systems: usize, pub auto: usize, pub review: usize, pub left: usize,
                      pub junk: usize, pub other_files: usize, pub deep_folders: usize, pub bytes: u64 }
  #[derive(Clone, Debug, Serialize, Deserialize)]
  pub struct OrganizePlan {
      pub source_root: PathBuf, pub target_root: PathBuf, pub roms_dir: PathBuf, pub layout: String,
      pub mode: TransferMode, pub names: NameMode, pub items: Vec<OrganizeItem>, pub counts: Counts,
      pub sorted_folders: Vec<(PathBuf, String)>, pub deep_folders: Vec<PathBuf>, pub other_files: Vec<PathBuf>,
      pub warnings: Vec<PlanWarning>, pub broad_root: bool, pub fat32: bool, pub move_allowed: bool,
      pub target_detected: Option<String>, pub created_at: String,
  }
  #[serde(tag = "kind", rename_all = "kebab-case")]
  pub enum PlanWarning { CatalogueMissing { system: String }, TargetLayoutMismatch { profile: String },
                         MoveUnavailable, SortedOnly }
  pub struct FolderProbe { pub rom_like: usize, pub partial_downloads: usize, pub cloud_only: usize,
                           pub sorted_folders: Vec<String>, pub writable: bool, pub broad_root: bool, pub truncated: bool }
  pub fn probe_folder(path: &Path, db: &SystemDb, probe: &dyn MetaProbe) -> io::Result<FolderProbe>;
  pub const PROBE_LIMIT: usize = 5_000;
  ```

- [ ] **Step 1: Write the failing integration tests** — `tests/organize_plan.rs`

```rust
//! Planner end to end on a pile shaped like the owner's sample, with
//! synthetic contents and a mock DAT server (spec §6.7, §15).

mod common;

use std::sync::atomic::AtomicBool;

fn sample_pile(dir: &Path) -> Vec<(&'static str, Vec<u8>)> {
    // name → inner ROM bytes; mirrors ~/Documents/ROMs-Downloaded by shape only.
    let gba = |tag: u8| { let mut b = fx::gba(); b[0xA0] = tag; fix_gba(&mut b); b };
    let gb = |tag: u8| { let mut b = fx::gb(0x00); b[0x134] = tag; fix_gb(&mut b); b };
    let pile = vec![
        ("0319 - Crash Bandicoot XS (E)(Paracox).zip", "0319 - Crash Bandicoot XS (E)(Paracox).gba", gba(1)),
        ("1490 - Sonic Advance 3 (U)(Venom).zip", "1490 - Sonic Advance 3 (U)(Venom).gba", gba(2)),
        ("1792 - Tomb Raider - The Prophecy (U)(BatMan).zip", "1792 - Tomb Raider.gba", gba(3)),
        ("Fire Emblem (E)(Independent).zip", "1575 - Fire Emblem (E)(Independent).gba", gba(4)),
        ("Pokemon Verde Hoja (S)(Rising Sun).zip", "Pokemon Verde Hoja (S).gba", gba(5)),
        ("Tetris Worlds (USA).zip", "Tetris Worlds (USA).gba", gba(6)),
        ("Tetris (JUE) (V1.0) [!].zip", "Tetris (JUE) (V1.0) [!].gb", gb(1)),
        ("Tetris (JUE) (V1.1) [!].zip", "Tetris (JUE) (V1.1) [!].gb", gb(2)),
        ("Tetris 2 (1).zip", "Tetris 2 (USA).gb", gb(3)),
        ("Tetris Attack (1).zip", "Tetris Attack (USA).gb", gb(4)),
        ("Tetris Blast.zip", "Tetris Blast (USA).gb", gb(5)),
        ("Tetris Plus (1).zip", "Tetris Plus (USA).gb", gb(6)),
        ("Magical Tetris Challenge (USA).zip", "Magical Tetris Challenge (USA).gbc", fx::gb(0xC0)),
        ("Tetris DX.zip", "Tetris DX (World).gb", fx::gb(0x80)),
        ("Tetris 2.zip", "Tetris 2 (USA).sfc", fx::snes(fx::SnesMap::Lo, false)),
        ("Tetris Attack.zip", "Tetris Attack (USA).sfc", fx::snes(fx::SnesMap::Hi, false)),
    ];
    // write zips; plus "3-D Tetris (USA).zip" as HTML and ".DS_Store"; set mtimes one hour back
}

#[test]
fn the_sample_shape_plans_gba_gb_gbc_sfc_and_asks_nothing() {
    // offline, no DATs: headers decide every valid zip
    let plan = plan(&dir, OrganizeOptions { offline: true, ..defaults("anbernic-stock") });
    let per: BTreeMap<String, usize> = count_by_folder(&plan);
    assert_eq!(per, BTreeMap::from([("GB".into(), 6), ("GBA".into(), 6), ("GBC".into(), 2), ("SFC".into(), 2)]));
    assert_eq!(plan.counts.review, 0);
    assert_eq!(left_reason(&plan, "3-D Tetris (USA).zip"), Some(LeftReason::NotARom { what: NotARom::WebPage }));
    assert_eq!(plan.counts.junk, 1);
    assert!(plan.items.iter().filter_map(|i| i.target.as_ref()).all(|t| t.starts_with("ROMS")));
    insta::assert_json_snapshot!("sample_offline", redact(&plan));
}

#[test]
fn with_a_mock_dat_server_hits_come_from_the_catalogue() {
    // serve GB/GBA/SNES DATs with the synthetic CRCs; GBC DAT served too → method hash for every item
}

#[test]
fn folder_names_follow_the_layout() {
    for (layout, gba) in [("anbernic-stock", "ROMS/GBA"), ("esde", "ROMs/gba"), ("batocera", "roms/gba"), ("muos", "ROMS/gba")] {
        // assert the GBA items' target parent
    }
}

#[test]
fn decisions_assign_leave_and_accept() {
    // a `.gb` file with no header (zeros) → review gb/gbc; Assign{gbc} → Auto-like target in GBC, method User;
    // Leave → Left Unknown; Assign{"nope"} → Err(UnknownSystem)
}

#[test]
fn generic_files_in_a_broad_folder_are_never_proposed() {
    // installer.iso (zeros), firmware.bin (random), photos.zip (jpg inside) under a dir; broad_root forced
    // through OrganizeOptions? (no: through a pile under a fake home) → counts.review == 0, counts.auto == 0
}

#[test]
fn already_sorted_only_folder_warns() {
    // pile with only GBA/a.gba → warnings contains SortedOnly, items empty
}

#[test]
fn replanning_after_an_option_change_reads_no_file() {
    // survey once; plan_from_survey with names=Catalogue and flat=true → same items, different targets;
    // a counting MetaProbe/ReadAt is not needed: assert facts come from the cache (organize_facts rows == n)
}

#[test]
fn cancel_stops_planning() {
    let cancel = AtomicBool::new(true);
    assert!(matches!(plan_organize(&dir, &opts, &env, &|_| {}, &cancel), Err(OrganizeError::Cancelled)));
}

#[test]
fn probe_counts_names_only() {
    // sample pile (16 valid zips + the HTML one) + 1 .crdownload + a GBA/ folder → rom_like 17
    // (the HTML zip is ROM-like by name), partial_downloads 1, sorted_folders ["GBA"], writable true, truncated false
}

#[test]
#[ignore = "perf gate: run with --ignored in CI-local gates"]
fn ten_thousand_gba_headers_plan_without_hashing() {
    // 10 000 raw 64 KiB GBA files with distinct tags → plan < 30 s, every item Auto by header,
    // `organize::plan::HASH_CALLS` (a test-util AtomicUsize counter incremented by the ladder) == 0
}
```

Write every test body fully following its comment; helpers `fix_gba`/`fix_gb` recompute checksums (expose them from `fixtures` as `gba_tagged(tag)` / `gb_tagged(cgb, tag)` instead of fixing in the test). `redact` replaces absolute paths and `created_at` with fixed strings before snapshotting.

- [ ] **Step 2: Run to see them fail.**

- [ ] **Step 3: Implement**

`survey`:
1. `source` must be a directory (`SourceMissing` / `Unreadable`). Resolve the layout: `env.profiles` by id → `UnknownLayout`. Target root `T = opts.target.unwrap_or(source)` (absolute via `std::path::absolute`), `roms_dir = if opts.flat || profile.roms_root.is_empty() { T } else { find_dir_ignoring_case(T, roms_root).unwrap_or(T.join(roms_root)) }`.
2. `walk(source, …)` with `skip_dirs` = `[T]` when `T` is strictly inside `source` and is not `source`; `roms_root = Some(roms_dir)` when `roms_dir` is inside `source`. Progress `Walk`.
3. Disc sets: `scan::multifile::group_sets(candidates of the same dir, &mut warnings)` turns cue/gdi/m3u + members into one candidate; missing members → `Left(SetIncomplete{missing})`; members referenced by `..` or absolute paths → `Left(SetOutsideFolder)`; a member in two sets → both `Left(SharedTrack)`. The set's primary data track is sniffed with `inspect_set_primary`.
4. Identify candidates on `workers = if write::on_removable_volume(source) { 2 } else { 4 }` scoped threads pulling indices from an `AtomicUsize`; each: facts from `cache.organize_facts(path, size, mtime_ns)` (deserialised) or `inspect_file` + `put_organize_facts`; then `identify_candidate`. Cloud-only candidates skip `inspect_file` (name/extension only: a synthetic `Inspection { container: Raw, payloads: [Payload { sniff: Sniff::default(), … }] }`). Check `cancel` between items → `Cancelled`. Progress `Identify` at most every 100 ms (`Instant` in a `Mutex`).
5. Duplicate bucket: candidates whose size equals another candidate's size and whose content key is still unknown are hashed (`need_hash`), so `mark_duplicates` can decide.

`plan_from_survey`: build `OrganizeItem`s from the survey (Auto/Review/Left from the identification), apply `decisions` (`Assign` → `Evidence(User, system, 1.0)`, action Auto, `method = User`, confidence 1.0, remember with `cache.put_organize_choice` when a content key exists; `Leave` → `Left(Unknown)`; `Accept` → Auto with the best candidate; unknown ids → `UnknownItem`, unknown system → `UnknownSystem`), `mark_duplicates`, `assign_targets` with a fresh `DirIndex`, set `mode = opts.mode.unwrap_or(if same_volume(source, T) { Move } else { Copy })` and `move_allowed = source writable` (`libc::access(W_OK)` on unix, readonly attribute on Windows) → when not allowed `mode = Copy` + `PlanWarning::MoveUnavailable`. `target_detected = detect(T).first()` when it differs from `opts.layout` → `TargetLayoutMismatch`. `counts` from the items. Items with no target that are not Left and plan with zero candidates but sorted folders → `SortedOnly`.

`probe_folder`: same walk rules with `max_depth = 4`, names and `symlink_metadata` only (no file reads), stop after `PROBE_LIMIT` entries (`truncated = true`); `rom_like` = ROM-like extensions (partial downloads excluded), `writable` as above, `broad_root = is_broad_root(path)`.

`OrganizeOptions::from_settings` reads `settings.organize` (Task 23 adds the group; until then the function uses the hard defaults and Task 23 rewires it — write it against `Default` now and update it there).

- [ ] **Step 4: Run tests and gates** (`cargo insta review` to accept the first snapshot after checking it by eye: GBA 6, GB 6, GBC 2, SFC 2, review 0).

- [ ] **Step 5: Commit**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize && git add crates/coverly-core && git commit -m "feat(core): plan organising a ROM pile by system"
```

---

## Group O6 — Safe apply, journal, undo

### Task 17: Core — `rename_noclobber`, same-volume test, sharing-violation retries, `EXDEV` mapping

**Files:**
- Modify: `crates/coverly-core/Cargo.toml`:
  ```toml
  [target.'cfg(unix)'.dependencies]
  libc = "0.2"
  [target.'cfg(windows)'.dependencies]
  windows-sys = { version = "0.61", features = ["Win32_Foundation", "Win32_Storage_FileSystem"] }
  ```
  (both versions are already in `Cargo.lock`; run `cargo build --offline` to prove it)
- Create: `crates/coverly-core/src/write/noclobber.rs`
- Modify: `crates/coverly-core/src/write/mod.rs` (`mod noclobber; pub use noclobber::{rename_noclobber, same_volume};`, `WriteError::CrossDevice`, `map_io` arms)
- Modify: `src-tauri/src/error.rs` (`From<WriteError>` gets the new variant → `IpcError::Io`)
- Test: `crates/coverly-core/tests/noclobber.rs`, unit tests in `noclobber.rs`

**Interfaces:**
- Produces:
  ```rust
  pub fn rename_noclobber(from: &Path, to: &Path) -> io::Result<()>;     // ErrorKind::AlreadyExists when `to` (or a twin on a case-insensitive volume) exists
  #[doc(hidden)] pub fn rename_noclobber_using(from: &Path, to: &Path,
      exclusive: &dyn Fn(&Path, &Path) -> io::Result<()>, link: &dyn Fn(&Path, &Path) -> io::Result<()>) -> io::Result<()>;
  pub fn same_volume(a: &Path, b: &Path) -> bool;
  pub(crate) fn with_sharing_retries<T>(f: impl FnMut() -> io::Result<T>) -> io::Result<T>;
  pub(crate) fn is_cross_device(e: &io::Error) -> bool;                    // EXDEV 18 / ERROR_NOT_SAME_DEVICE 17
  WriteError::CrossDevice(PathBuf)
  ```

- [ ] **Step 1: Write the failing tests** — `tests/noclobber.rs`

```rust
use std::fs;
use std::io::ErrorKind;

use coverly_core::write::{rename_noclobber, rename_noclobber_using, same_volume};

#[test]
fn rename_noclobber_moves_when_the_target_is_free() {
    let tmp = tempfile::tempdir().unwrap();
    let (a, b) = (tmp.path().join("a.gb"), tmp.path().join("b.gb"));
    fs::write(&a, b"A").unwrap();
    rename_noclobber(&a, &b).unwrap();
    assert!(!a.exists());
    assert_eq!(fs::read(&b).unwrap(), b"A");
}

#[test]
fn rename_noclobber_refuses_an_existing_target() {
    let tmp = tempfile::tempdir().unwrap();
    let (a, b) = (tmp.path().join("a.gb"), tmp.path().join("b.gb"));
    fs::write(&a, b"A").unwrap();
    fs::write(&b, b"B").unwrap();
    let err = rename_noclobber(&a, &b).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::AlreadyExists);
    assert_eq!(fs::read(&b).unwrap(), b"B", "never replaced");
    assert_eq!(fs::read(&a).unwrap(), b"A");
}

#[test]
fn refuses_a_case_twin() {
    let tmp = tempfile::tempdir().unwrap();
    let a = tmp.path().join("a.gb");
    fs::write(&a, b"A").unwrap();
    fs::write(tmp.path().join("Tetris.gb"), b"T").unwrap();
    let err = rename_noclobber(&a, &tmp.path().join("TETRIS.GB")).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::AlreadyExists);
    assert_eq!(fs::read(tmp.path().join("Tetris.gb")).unwrap(), b"T");
}

#[test]
fn falls_back_to_link_when_exclusive_rename_is_unsupported() {
    let tmp = tempfile::tempdir().unwrap();
    let (a, b) = (tmp.path().join("a"), tmp.path().join("b"));
    fs::write(&a, b"A").unwrap();
    let unsupported = |_: &std::path::Path, _: &std::path::Path| Err(std::io::Error::from_raw_os_error(UNSUPPORTED));
    rename_noclobber_using(&a, &b, &unsupported, &|f, t| fs::hard_link(f, t)).unwrap();
    assert!(!a.exists() && b.exists());
    fs::write(&a, b"A2").unwrap();
    let err = rename_noclobber_using(&a, &b, &unsupported, &|f, t| fs::hard_link(f, t)).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::AlreadyExists);
}

#[test]
fn falls_back_to_a_checked_rename_without_links() {
    // exclusive and link both unsupported (FAT): twin check then rename; an existing twin → AlreadyExists
}

#[test]
fn a_tempdir_is_on_the_same_volume_as_itself() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(same_volume(tmp.path(), &tmp.path().join("x/y")));
}

#[cfg(unix)]
const UNSUPPORTED: i32 = libc::ENOTSUP;
#[cfg(windows)]
const UNSUPPORTED: i32 = 50; // ERROR_NOT_SUPPORTED
```

(`libc` is a dev-dependency through the normal dependency on unix; on windows the constant is literal.)

- [ ] **Step 2: Run to see them fail.**

- [ ] **Step 3: Implement `noclobber.rs`**

```rust
//! Renames that never replace an existing file (spec §9): `std::fs::rename`
//! silently replaces on Unix and on Windows.

use std::ffi::CString;
use std::io::{self, ErrorKind};
use std::path::Path;

pub fn rename_noclobber(from: &Path, to: &Path) -> io::Result<()> {
    rename_noclobber_using(from, to, &exclusive_rename, &|f, t| std::fs::hard_link(f, t))
}

#[doc(hidden)]
pub fn rename_noclobber_using(
    from: &Path,
    to: &Path,
    exclusive: &dyn Fn(&Path, &Path) -> io::Result<()>,
    link: &dyn Fn(&Path, &Path) -> io::Result<()>,
) -> io::Result<()> {
    // A twin under another case/NFC spelling counts as taken on every
    // volume (the library may later be copied to FAT).
    if twin_exists(to)? {
        return Err(io::Error::from(ErrorKind::AlreadyExists));
    }
    match exclusive(from, to) {
        Ok(()) => return Ok(()),
        Err(e) if !is_unsupported(&e) => return Err(e),
        Err(_) => {}
    }
    match link(from, to) {
        Ok(()) => return std::fs::remove_file(from).or_else(|e| {
            // Linked but the source stays: undo the link, never leave two names.
            let _ = std::fs::remove_file(to);
            Err(e)
        }),
        Err(e) if e.kind() == ErrorKind::AlreadyExists => return Err(e),
        Err(e) if !is_unsupported(&e) && e.kind() != ErrorKind::PermissionDenied => return Err(e),
        Err(_) => {}
    }
    // FAT/exFAT: no exclusive rename, no hard links. Last check, then
    // rename; the remaining race is documented in the spec (§9).
    if twin_exists(to)? {
        return Err(io::Error::from(ErrorKind::AlreadyExists));
    }
    std::fs::rename(from, to)
}

fn twin_exists(to: &Path) -> io::Result<bool> {
    let (Some(dir), Some(name)) = (to.parent(), to.file_name().and_then(|n| n.to_str())) else {
        return Ok(false);
    };
    match super::find_twin(dir, name) {
        Ok(found) => Ok(found.is_some()),
        Err(super::WriteError::Io(e)) if e.kind() == ErrorKind::NotFound => Ok(false),
        Err(e) => Err(io::Error::other(e.to_string())),
    }
}

fn is_unsupported(e: &io::Error) -> bool {
    if e.kind() == ErrorKind::Unsupported {
        return true;
    }
    match e.raw_os_error() {
        #[cfg(unix)]
        Some(code) => [libc::EINVAL, libc::ENOSYS, libc::ENOTSUP, libc::EOPNOTSUPP].contains(&code),
        #[cfg(windows)]
        Some(code) => code == 50 || code == 1, // ERROR_NOT_SUPPORTED, ERROR_INVALID_FUNCTION
        _ => false,
    }
}

#[cfg(target_os = "macos")]
fn exclusive_rename(from: &Path, to: &Path) -> io::Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let (f, t) = (CString::new(from.as_os_str().as_bytes())?, CString::new(to.as_os_str().as_bytes())?);
    // SAFETY: two valid NUL-terminated paths.
    let rc = unsafe { libc::renamex_np(f.as_ptr(), t.as_ptr(), libc::RENAME_EXCL) };
    if rc == 0 { Ok(()) } else { Err(io::Error::last_os_error()) }
}

#[cfg(target_os = "linux")]
fn exclusive_rename(from: &Path, to: &Path) -> io::Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let (f, t) = (CString::new(from.as_os_str().as_bytes())?, CString::new(to.as_os_str().as_bytes())?);
    // SAFETY: two valid NUL-terminated paths relative to the cwd fd.
    let rc = unsafe {
        libc::renameat2(libc::AT_FDCWD, f.as_ptr(), libc::AT_FDCWD, t.as_ptr(), libc::RENAME_NOREPLACE)
    };
    if rc == 0 { Ok(()) } else { Err(io::Error::last_os_error()) }
}

#[cfg(windows)]
fn exclusive_rename(from: &Path, to: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::MoveFileExW;
    let wide = |p: &Path| p.as_os_str().encode_wide().chain(Some(0)).collect::<Vec<u16>>();
    let (f, t) = (wide(from), wide(to));
    // Flags 0: no MOVEFILE_REPLACE_EXISTING, no MOVEFILE_COPY_ALLOWED.
    // SAFETY: two valid NUL-terminated UTF-16 paths.
    let ok = unsafe { MoveFileExW(f.as_ptr(), t.as_ptr(), 0) };
    if ok != 0 { Ok(()) } else { Err(io::Error::last_os_error()) }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
fn exclusive_rename(_: &Path, _: &Path) -> io::Result<()> {
    Err(io::Error::from(ErrorKind::Unsupported))
}

/// Whether `a` and `b` (or their nearest existing ancestors) are on one volume.
pub fn same_volume(a: &Path, b: &Path) -> bool {
    let existing = |p: &Path| p.ancestors().find(|q| q.exists()).map(Path::to_path_buf);
    let (Some(a), Some(b)) = (existing(a), existing(b)) else { return false };
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        matches!((std::fs::metadata(&a), std::fs::metadata(&b)), (Ok(x), Ok(y)) if x.dev() == y.dev())
    }
    #[cfg(windows)]
    {
        // O-D3: compare drive letter / UNC share.
        let prefix = |p: &Path| std::path::absolute(p).ok().and_then(|p| p.components().next().map(|c| c.as_os_str().to_ascii_lowercase()));
        prefix(&a).is_some() && prefix(&a) == prefix(&b)
    }
}

/// Windows sharing/lock violations (antivirus, indexer): 3 retries with
/// 200/500/1000 ms back-off. Other systems call `f` once.
pub(crate) fn with_sharing_retries<T>(mut f: impl FnMut() -> io::Result<T>) -> io::Result<T> {
    #[cfg(windows)]
    for wait in [200u64, 500, 1000] {
        match f() {
            Err(e) if matches!(e.raw_os_error(), Some(32 | 33)) => std::thread::sleep(std::time::Duration::from_millis(wait)),
            other => return other,
        }
    }
    f()
}

pub(crate) fn is_cross_device(e: &io::Error) -> bool {
    e.kind() == ErrorKind::CrossesDevices || matches!(e.raw_os_error(), Some(18) if cfg!(unix)) || matches!(e.raw_os_error(), Some(17) if cfg!(windows))
}
```

`find_twin` becomes `pub(crate)` (Task 15 did it; check). `map_io` gains the `EXDEV`/17 arm returning `WriteError::CrossDevice(path)`. Check the `libc` docs for `renamex_np`/`RENAME_EXCL` availability on the pinned `libc` version (`cargo doc -p libc --offline`); if missing, call it through `extern "C" { fn renamex_np(...) -> c_int; }` with `RENAME_EXCL = 0x4`.

- [ ] **Step 4: Run tests and gates** (`cargo build --offline` too).

- [ ] **Step 5: Commit**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize && git add Cargo.lock crates src-tauri/src/error.rs && git commit -m "feat(core): rename without ever replacing an existing file"
```

---

### Task 18: Core — journal format: job kind, source root, fingerprints, new ops, forward compatibility

**Files:**
- Modify: `crates/coverly-core/src/write/mod.rs`
- Test: `crates/coverly-core/tests/writer.rs` (append), `crates/coverly-core/tests/fixtures/journals/fill-v1.jsonl` (an old journal, written by hand from the current format)

**Interfaces:**
- Produces:
  ```rust
  #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)] #[serde(rename_all = "kebab-case")]
  pub enum JobKind { #[default] Fill, Organize }
  #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
  pub struct Fingerprint { pub size: u64, pub mtime_ns: i64, #[serde(default, skip_serializing_if = "Option::is_none")] pub crc32: Option<u32> }
  #[serde(rename_all = "kebab-case")] pub enum Via { Rename, Copy }
  // Line::Header gains: #[serde(default)] source_root: Option<PathBuf>, #[serde(default)] kind: JobKind
  // Line::Entry: sha1: Option<String> (#[serde(default, skip_serializing_if = "Option::is_none")]),
  //              #[serde(default, skip_serializing_if = "Option::is_none")] fp: Option<Fingerprint>,
  //              #[serde(default, skip_serializing_if = "Option::is_none")] item: Option<usize>
  // Op gains: Moved { from: PathBuf, via: Via }, Copied { from: PathBuf }, DirCreated,
  //           OverrideMoved { from: PathBuf, to: PathBuf }, #[serde(other)] Unknown
  pub struct JobMeta { pub kind: JobKind, pub source_root: Option<PathBuf> }
  impl Writer { pub fn begin_with(root: &Path, data_dir: &Path, meta: JobMeta) -> Result<Writer, WriteError>; }
  // JobSummary gains: pub kind: JobKind, pub source_root: Option<PathBuf>, pub undoable: bool
  ```

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn an_old_fill_journal_still_parses_and_undoes() {
    // copy tests/fixtures/journals/fill-v1.jsonl into <data>/jobs/, rewrite its root to a tempdir holding the
    // created file with the journaled sha1 → list_jobs shows kind Fill, undoable; undo_last removes the file
}

#[test]
fn an_organise_header_round_trips() {
    let tmp = tempfile::tempdir().unwrap();
    let (root, src, data) = (tmp.path().join("T"), tmp.path().join("S"), tmp.path().join("d"));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&src).unwrap();
    let w = Writer::begin_with(&root, &data, JobMeta { kind: JobKind::Organize, source_root: Some(src.clone()) }).unwrap();
    let job = w.finish().unwrap();
    let s = list_jobs(&data).unwrap().into_iter().find(|j| j.job == job).unwrap();
    assert_eq!((s.kind, s.source_root.as_deref()), (JobKind::Organize, Some(src.as_path())));
}

#[test]
fn a_journal_from_a_newer_version_is_listed_as_not_undoable() {
    // header + {"type":"entry","target":"/x","op":{"kind":"teleported"}} → list_jobs ok, undoable false;
    // undo_job → Err(Journal) mentioning "newer"
}
```

- [ ] **Step 2: Run to see them fail.**

- [ ] **Step 3: Implement** the type changes above. `Writer::begin` becomes `begin_with(root, data_dir, JobMeta { kind: Fill, source_root: None })`. Every existing `Line::Entry { sha1 }` construction wraps in `Some(..)`; `undo_journal`'s existing arms compare `sha1.as_deref()` (an entry of a fill op without `sha1` is `skipped_modified`). `list_jobs` sets `undoable = !entries.iter().any(|e| matches!(e, Line::Entry { op: Op::Unknown, .. }))`; `undo_journal` refuses such a journal with `WriteError::Journal("written by a newer Coverly; update to undo it")` before touching anything. Add `Writer::append_entry(&mut self, target, op, fp, item)` (private) used by Task 19.

- [ ] **Step 4: Run tests and gates** (`cargo test -p coverly-core --test writer --test journal_resume --test apply` — every existing journal test must still pass).

- [ ] **Step 5: Commit**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize && git add crates/coverly-core && git commit -m "feat(core): extend job journals for organise jobs, compatibly"
```

---

### Task 19: Core — `Writer::{ensure_dir, move_file, copy_file}`: streamed copies, verification, fingerprints

**Files:**
- Modify: `crates/coverly-core/src/write/mod.rs`
- Test: `crates/coverly-core/tests/organize_apply.rs` (writer-level tests first)

**Interfaces:**
- Produces:
  ```rust
  #[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Collision { Rename, Skip }
  #[derive(Clone, Debug, PartialEq, Eq, Serialize)] #[serde(tag = "outcome", rename_all = "kebab-case")]
  pub enum MoveOutcome {
      Moved { to: PathBuf, renamed: bool },
      Copied { to: PathBuf, renamed: bool },
      CopiedNotRemoved { to: PathBuf, renamed: bool },
      SkippedExists { existing: PathBuf },
  }
  impl Writer {
      pub fn ensure_dir(&mut self, dir: &Path) -> Result<(), WriteError>;
      pub fn move_file(&mut self, from: &Path, to: &Path, collision: Collision, item: Option<usize>) -> Result<MoveOutcome, WriteError>;
      pub fn copy_file(&mut self, from: &Path, to: &Path, collision: Collision, item: Option<usize>) -> Result<MoveOutcome, WriteError>;
      pub fn clean_temps(&self, dirs: &[PathBuf]);   // .coverly-<job>-*.tmp of this job
  }
  #[doc(hidden)] pub fn set_copy_fault(f: Option<fn(&Path) -> Option<io::Error>>);  // test-util: inject verify/delete faults
  ```

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn move_in_place_renames_and_journals_before_acting() { /* S==T; ROMS/GBA created; journal has dir-created ×2 then moved */ }

#[test]
fn copy_keeps_the_source_and_the_mtime() {
    // copy_file → target bytes equal, source untouched, target mtime == source mtime (±2 s for FAT)
}

#[test]
fn copy_verifies_by_rereading_and_keeps_the_source_on_mismatch() {
    // set_copy_fault(Some(|_| Some(corrupt))) for the verify step → Err(WriteError::Io(..)), no target left,
    // source intact, journal has the intent entry only (undo is a no-op)
}

#[test]
fn a_failed_source_delete_is_reported_as_copied() {
    // cross-volume move simulated: set_copy_fault injects EXDEV on rename and PermissionDenied on delete →
    // Ok(CopiedNotRemoved), journal op `copied`
}

#[test]
fn an_existing_target_is_renamed_or_skipped() {
    // Collision::Rename → "Tetris (2).gb"; Collision::Skip → SkippedExists; existing bytes untouched
}

#[test]
fn a_case_only_rename_is_a_noop() { /* from "x/tetris.gb" to "x/Tetris.gb" → Moved { renamed: false }, no rename attempted */ }

#[test]
fn no_appledouble_or_temp_files_are_left() { /* after a copy: no "._*" and no ".coverly-*.tmp" in the target dir */ }
```

- [ ] **Step 2: Run to see them fail.**

- [ ] **Step 3: Implement**

`ensure_dir(dir)`: `ensure_within(&self.root, dir)`; `create_dirs(dir)` gives the folders that did not exist (outermost last → reverse them); for each, **append `DirCreated` before** creating it (write-ahead), then `fs::create_dir`; `remove_apple_doubles` on them.

`move_file(from, to, collision, item)`:
1. `from` within `source_root` (`ensure_within`), `to` within `root`; `from` must be a regular file (`NotAFile`).
2. Same parent and `name_key` equal → return `Moved { to: from, renamed: false }` (no-op, nothing journaled).
3. `self.ensure_dir(parent)`.
4. Final name: if `find_twin(parent, name)` → `Collision::Skip` → `SkippedExists`; `Rename` → next free `"{stem} ({n}).{ext}"`.
5. If `same_volume(from, parent)`: fingerprint from the source `symlink_metadata` (size, mtime_ns; rename keeps both), append `Entry { target, op: Moved { from, via: Rename }, fp, item }` **then** `with_sharing_retries(|| rename_noclobber(from, target))`. `AlreadyExists` (a file appeared meanwhile) → pick the next free name, append a new entry, retry (max 100). Cross-device error → step 6.
6. Otherwise `stream_copy(from, target)` (below) with intent entry `Moved { via: Copy }`, `fp: None` appended **before** copying, and the completed entry (same target, `fp: Some(after)`) appended after the verified copy; then `fs::remove_file(from)` with sharing retries; failure → append `Copied { from }` with the same `fp` and return `CopiedNotRemoved`.

`copy_file`: steps 1–4, then 6 without the delete, op `Copied { from }`.

`stream_copy(from, to) -> Result<Fingerprint, WriteError>`: temp `.coverly-<job>-<counter>.tmp` in `to`'s dir via `OpenOptions::create_new`; `io::copy` through a `BufReader::with_capacity(1 << 20)` wrapper that updates a `crc32fast::Hasher`; `sync_all`; `File::set_modified(source mtime)`; `rename_noclobber(tmp, to)` (on `AlreadyExists` the caller picks the next name); re-read `to` in 1 MiB chunks computing CRC; mismatch → delete `to`, return `WriteError::Io(InvalidData "copy verification failed")`; `fp` = `stat(to)` after `set_modified` + the CRC. Never `fs::copy`. On any error the temp is removed and its AppleDouble sidecar too.

`set_copy_fault` is a `#[cfg(feature = "test-util")]` static `Mutex<Option<fn(&Path) -> Option<io::Error>>>` consulted at three points (`"rename"`, `"verify"`, `"delete"` encoded in the path's extension by the test — simplest: the fault fn receives a stage enum; adapt the signature to `fn(CopyStage, &Path) -> Option<io::Error>`).

- [ ] **Step 4: Run tests and gates.**

- [ ] **Step 5: Commit**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize && git add crates/coverly-core && git commit -m "feat(core): move and copy ROMs with a write-ahead journal and verified copies"
```

---

### Task 20: Core — undo for moves/copies/folders, later-jobs check, kind-aware pruning

**Files:**
- Modify: `crates/coverly-core/src/write/mod.rs`
- Test: `crates/coverly-core/tests/organize_undo.rs`

**Interfaces:**
- Produces:
  ```rust
  // UndoReport gains: pub skipped_occupied: Vec<PathBuf>, pub dirs_removed: Vec<PathBuf>
  pub fn later_jobs(data_dir: &Path, job: &str) -> Result<Vec<JobSummary>, WriteError>;  // newer, not undone, root inside job's root
  pub fn undo_jobs_in_order(data_dir: &Path, jobs: &[String]) -> Result<Vec<UndoReport>, WriteError>; // newest first
  pub fn prune_jobs(data_dir: &Path, keep: usize) -> Result<(), WriteError>;            // now kind-aware
  pub fn prune_jobs_at(data_dir: &Path, keep: usize, keep_organize_days: u32, now: SystemTime) -> Result<(), WriteError>;
  ```

- [ ] **Step 1: Write the failing tests** — `tests/organize_undo.rs`

```rust
#[test]
fn undo_moves_back_and_removes_created_folders() { /* move 2 files into ROMS/GB; undo → sources back, ROMS/GB and ROMS gone */ }

#[test]
fn undo_keeps_a_moved_file_that_was_changed() { /* append bytes to the target after the move → skipped_modified, target stays */ }

#[test]
fn undo_does_not_overwrite_a_reoccupied_source() { /* new file at `from` with other bytes → skipped_occupied, both kept */ }

#[test]
fn undo_after_a_crash_between_copy_and_delete_removes_the_copy() {
    // hand-write a journal: header(kind organize) + intent entry Moved{via: copy}, fp None, and leave both
    // files with equal bytes → undo deletes the target, restored has it, source intact
}

#[test]
fn undo_of_a_copy_deletes_only_an_unchanged_target() {}

#[test]
fn a_folder_with_macos_litter_is_still_removed() { /* ROMS/GB holds .DS_Store and ._x after undo → removed */ }

#[test]
fn a_folder_with_user_files_is_kept() {}

#[test]
fn undo_needs_the_source_volume() { /* source_root missing → Err(WriteError::Denied or a new VolumeGone) and nothing changed */ }

#[test]
fn undo_refuses_when_later_jobs_wrote_under_the_target() {
    // organise job O into T, then a fill job F with root T → later_jobs(O) == [F];
    // undo_jobs_in_order([F, O]) undoes both, newest first
}

#[test]
fn undo_restores_moved_identity_overrides() { /* OverrideMoved entry → identity-overrides.json key back to `from` */ }

#[test]
fn organise_journals_survive_thirty_fill_jobs() {
    // 1 organise + 30 fill journals; prune_jobs_at(keep 20, 90 days, now) → organise kept, 20 fill kept;
    // an organise journal 91 days old and beyond the newest 20 organise → removed with its .plan.json
}
```

- [ ] **Step 2: Run to see them fail.**

- [ ] **Step 3: Implement**

`undo_journal` dispatches by op **before** reading the target (fill ops keep today's code path unchanged, which reads the art file; organise ops never read whole files unless a CRC is needed):

- `Moved { from, via }` with `fp`:
  - `source_root` missing on disk → return `WriteError::Denied(source_root)` *before* touching anything (the caller maps it to a pause / "volume gone"; check this at the start of `undo_journal` for organise journals).
  - target missing → nothing (and `clean_temps` for that dir).
  - target `size != fp.size` → `skipped_modified`; `mtime == fp.mtime_ns` → match; mtime differs and `fp.crc32` is `Some` → CRC the target; equal → match; else `skipped_modified`.
  - `from` free → `create_dirs(from.parent())`, `rename_noclobber(target, from)`; cross-device → stream copy back + verify + delete target. → `restored`.
  - `from` occupied → its size + CRC equal the target's → delete target (`restored`); else `skipped_occupied`.
- `Moved`/`Copied` with `fp: None` (intent): `from` occupied and target exists with equal size + CRC → delete target (`restored`); otherwise nothing.
- `Copied { .. }` with `fp`: target matches → delete (`removed`); else `skipped_modified`.
- `DirCreated`: remove `.DS_Store` and AppleDouble files, then `fs::remove_dir` (fails when not empty → kept silently) → `dirs_removed`.
- `OverrideMoved { from, to }`: `Overrides::load_for_update(data_dir/OVERRIDES_FILE)`; move the entry keyed by `to` back to `from`; save.
- Entries are processed newest first (as today); the completed copy entry comes after its intent entry, so the intent sees the target already gone → no-op.

`later_jobs`: from `list_jobs`, jobs with id greater than `job` (ids sort by time), not undone, whose `root` starts with this job's `root` (or vice versa).

Kind-aware `prune_jobs_at`: split journals by kind; keep the newest `keep` of each kind, plus organise journals younger than `keep_organize_days`; delete the rest with their `backups/<job>` and `<job>.plan.json`. `prune_jobs(d, keep)` = `prune_jobs_at(d, keep, 90, SystemTime::now())`. The job id timestamp (`compact_utc`) gives the age; add `crate::time::parse_compact_utc` if not present.

- [ ] **Step 4: Run tests and gates.**

- [ ] **Step 5: Commit**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize && git add crates/coverly-core && git commit -m "feat(core): undo organise jobs safely and keep their journals longer"
```

---

### Task 21: Core — `organize::apply`: re-validation, free space, directory index, pause, resume, overrides

**Files:**
- Create: `crates/coverly-core/src/organize/apply.rs`
- Modify: `crates/coverly-core/src/organize/mod.rs`
- Test: `crates/coverly-core/tests/organize_apply.rs` (append)

**Interfaces:**
- Consumes: `OrganizePlan`, `Writer::{begin_with, ensure_dir, move_file, copy_file, finish, discard, resume}`, `jobs::ItemErrorKind`, `progress::PauseKind`, `DirIndex`.
- Produces:
  ```rust
  #[derive(Clone, Debug, Serialize, Deserialize)] #[serde(tag = "status", rename_all = "kebab-case")]
  pub enum ItemStatus { Moved, Copied, CopiedNotRemoved, Skipped { reason: SkipReason }, Failed { kind: ItemErrorKind, detail: String } }
  #[serde(rename_all = "kebab-case")]
  pub enum SkipReason { ChangedSincePlan, SourceMissing, AlreadyPresent, CollisionSet, NotDecided, Cancelled }
  #[derive(Clone, Debug, Serialize, Deserialize)]
  pub struct ItemOutcome { pub item: usize, #[serde(flatten)] pub status: ItemStatus, pub targets: Vec<PathBuf> }
  #[derive(Clone, Debug, Default, Serialize, Deserialize)]
  pub struct OrganizeReport { pub job: Option<String>, pub moved: usize, pub copied: usize, pub skipped: usize,
      pub failed: usize, pub items: Vec<ItemOutcome>, pub paused: Option<PauseKind>, pub resumable_items: Vec<usize> }
  pub struct ApplyEnv<'a> { pub data_dir: &'a Path, pub progress: &'a (dyn Fn(OrganizeProgress) + Sync), pub cancel: &'a AtomicBool }
  #[derive(Debug, thiserror::Error)] pub enum ApplyError {
      #[error("not enough space: {needed} bytes needed, {free} free")] NoSpace { needed: u64, free: u64 },
      #[error(transparent)] Write(#[from] WriteError), #[error("the plan of job {0} is missing")] NoPlan(String) }
  pub fn apply_organize(plan: &OrganizePlan, env: &ApplyEnv) -> Result<OrganizeReport, ApplyError>;
  pub fn resume_organize(job: &str, env: &ApplyEnv) -> Result<OrganizeReport, ApplyError>;
  pub fn free_space(path: &Path) -> Option<u64>;
  ```
  `OrganizePhase` gains `Apply`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn apply_moves_auto_items_and_leaves_review_and_left_items() {}
#[test]
fn copy_mode_into_another_root_keeps_the_pile() {}
#[test]
fn a_target_created_after_planning_gets_a_numbered_name() {}
#[test]
fn a_source_changed_after_planning_is_skipped() { /* rewrite bytes + bump mtime → Skipped ChangedSincePlan */ }
#[test]
fn a_set_collision_skips_the_whole_set() {}
#[test]
fn not_enough_space_refuses_to_start_with_numbers() { /* inject free_space via a test-util override → Err(NoSpace) and no journal */ }
#[test]
fn a_read_only_target_pauses_and_resume_finishes_into_the_same_journal() {
    // chmod 0o555 on ROMS/GB after the first item (unix) → paused Denied, resumable_items non-empty;
    // chmod back; resume_organize(job) → all done; one undo restores everything
}
#[test]
fn cancel_between_items_keeps_what_was_done_undoable() {}
#[test]
fn apply_uses_a_directory_index() { /* 300 files into one GBA folder → DirIndex::read_dirs() ≤ 3 */ }
#[test]
fn identity_overrides_follow_moved_roms() {}
#[test]
fn leftover_temps_of_the_job_are_cleaned_on_resume() {}
```

Write each body fully: build a pile in a tempdir, `plan_organize` it offline with the `anbernic-stock` layout, then apply.

- [ ] **Step 2: Run to see them fail.**

- [ ] **Step 3: Implement `apply_organize`**

1. Items to apply: `OrganizeAction::Auto` with a `target` (review items reach Auto only through decisions, Task 16).
2. Space: for `Copy` mode, or `Move` across volumes (`!same_volume(source_root, target_root)`): `needed = sum(size) + 1 %`; `free_space(target_root)` (`libc::statvfs` `f_bavail * f_frsize`; Windows `GetDiskFreeSpaceExW`; `None` → skip the check) → `NoSpace`.
3. `Writer::begin_with(target_root, data_dir, JobMeta { kind: Organize, source_root: Some(source_root) })`; write the plan to `<data>/jobs/<job>.plan.json` (pretty JSON) **before** the first action.
4. For each item (check `cancel` between items → remaining items `Skipped{Cancelled}`, finish the journal):
   - re-validate every source: exists, `size` and `mtime_ns` equal the plan's `fps` → else `Skipped{ChangedSincePlan|SourceMissing}`;
   - sets: every member target must still be free (`DirIndex`) → else `Skipped{CollisionSet}`;
   - `move_file`/`copy_file` with `Collision::Rename` for single files, `Collision::Skip` for set members (a skip of any member after others moved: move the already-moved members back through `rename_noclobber` and mark the item `Skipped{CollisionSet}`);
   - companions the same way, after the ROM;
   - identity overrides (`identify::overrides::Overrides` at `data_dir/OVERRIDES_FILE`) keyed by a moved source path are rewritten to the target and journaled as `OverrideMoved`;
   - `WriteError::{DiskFull, Denied}` or the source/target root gone → `paused = Some(PauseKind::…)` (reuse `DiskFull`, `ReadOnly`, `VolumeGone` from `progress::PauseKind`; add `VolumeGone` if missing), stop, `resumable_items` = items not attempted;
   - other errors → `Failed{kind: ItemErrorKind::from(&e), detail}` and continue.
   - progress `Apply` at most every 100 ms.
5. `writer.finish()` when not paused; a run that changed nothing → `writer.discard()` and `job: None`.
6. `DirIndex` is shared for the whole run (one `read_dir` per target folder).

`resume_organize(job)`: read `<job>.plan.json`; `Writer::resume(target_root, data_dir, job)` (extend `resume` to accept organise journals: it compares `root`); attempted items = item ids present in the journal (`Entry.item`); clean `.coverly-<job>-*.tmp` in the plan's target folders; apply the rest exactly as above (re-validated); merge counts.

- [ ] **Step 4: Run tests and gates.**

- [ ] **Step 5: Commit**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize && git add crates/coverly-core && git commit -m "feat(core): apply organise plans with pause, resume and undo"
```

---

### Task 22: Core — weak `system-folders` detection rule

**Files:**
- Modify: `crates/coverly-core/src/profiles/detect.rs`
- Modify: `crates/coverly-core/src/profiles/mod.rs` (`pub fn detect_with_fallback(root, fallback_profile) -> Vec<Detection>`)
- Test: `crates/coverly-core/src/profiles/detect.rs` (unit), existing detect tests unchanged

**Interfaces:**
- Produces: `pub fn detect_with_fallback(root: &Path, fallback: &str, db: &SystemDb) -> Vec<Detection>` — `detect(root)` when it finds anything; else, when a `roms`-named child (any casing) has ≥ 1 child folder that resolves to a system **and holds at least one file**, or the root itself has ≥ 2 such folders, one `Detection { profile_id: fallback, score: 1, evidence: ["system-folders"] }`. `fallback` must be a built-in id (else `anbernic-stock`).

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn a_freshly_organised_tree_is_suggested_with_the_fallback() {
    let tmp = tree(&["ROMS/GBA"], &["ROMS/GBA/a.gba"]);
    let found = detect_with_fallback(tmp.path(), "anbernic-stock", &SystemDb::builtin());
    assert_eq!(found[0].profile_id, "anbernic-stock");
    assert_eq!(found[0].evidence, ["system-folders"]);
    assert_eq!(found[0].score, 1);
}

#[test]
fn bare_root_needs_two_system_folders_with_files() {
    let one = tree(&["gb"], &["gb/a.gb"]);
    assert!(detect_with_fallback(one.path(), "esde", &SystemDb::builtin()).is_empty());
    let two = tree(&["gb", "GBA"], &["gb/a.gb", "GBA/b.gba"]);
    assert_eq!(detect_with_fallback(two.path(), "esde", &SystemDb::builtin())[0].profile_id, "esde");
}

#[test]
fn empty_system_folders_are_not_enough() {
    let tmp = tree(&["ROMS/GBA", "ROMS/GB"], &[]);
    assert!(detect_with_fallback(tmp.path(), "anbernic-stock", &SystemDb::builtin()).is_empty());
}

#[test]
fn markers_win_over_the_weak_rule() {
    let tmp = tree(&["anbernic", "ROMS/GBA"], &["ROMS/GBA/a.gba"]);
    let found = detect_with_fallback(tmp.path(), "esde", &SystemDb::builtin());
    assert_eq!(found[0].profile_id, "anbernic-stock");
    assert!(found.iter().all(|d| d.evidence != ["system-folders"]));
}
```

- [ ] **Step 2: Run to see them fail.** - [ ] **Step 3: Implement** (reuse `list`/`find_dir`; "holds a file" = any non-`._` regular file directly inside). Callers switch in Task 27 (app) and Task 25 (CLI `--profile auto` stays `detect` + this fallback with `settings.organize.default_layout`).

- [ ] **Step 4: Run tests and gates** (all existing `detect` tests unchanged).

- [ ] **Step 5: Commit**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize && git add crates/coverly-core && git commit -m "feat(core): suggest a layout for folders organised by system"
```

---

### Task 23: Core — `settings.organize`

**Files:**
- Modify: `crates/coverly-core/src/settings/model.rs`, `validate.rs`
- Modify: `crates/coverly-core/src/organize/plan.rs` (`OrganizeOptions::from_settings`)
- Test: `crates/coverly-core/tests/settings_file.rs`, `settings_apply.rs` (append)

**Interfaces:**
- Produces:
  ```rust
  choice!(/// Organise transfer mode (`auto` = move on the same volume, copy otherwise).
      OrganizeModeSetting { Auto, Move, Copy });
  #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)] #[serde(default)]
  pub struct OrganizeSettings {
      pub default_layout: String,                    // "anbernic-stock"
      pub nest_in_roms_root: bool,                   // true
      pub mode: OrganizeModeSetting,                 // Auto
      pub names: NameMode,                           // Keep
      pub download_dats: bool,                       // true
      pub auto_threshold: f32,                       // 0.85
      pub review_threshold: f32,                     // 0.50
      pub companion_exts: Vec<String>,               // COMPANION_EXTS
      pub folder_names: BTreeMap<String, BTreeMap<String, String>>, // profile → system → folder
      pub arcade_order: BTreeMap<String, Vec<String>>, // profile → order (empty = profile default)
      pub recent_seconds: u32,                       // 60
      pub keep_organize_journals_days: u32,          // 90
  }
  // Settings gains: pub organize: OrganizeSettings
  ```

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn a_settings_file_without_organize_loads_with_defaults() {
    let s: Settings = serde_json::from_str(r#"{"version":1}"#).unwrap();
    assert_eq!(s.organize.default_layout, "anbernic-stock");
    assert_eq!(s.organize.auto_threshold, 0.85);
}

#[test]
fn organize_values_are_validated() {
    // auto_threshold 1.5 → FieldError "organize.auto_threshold" out-of-range;
    // review_threshold > auto_threshold → "organize.review_threshold" invalid;
    // default_layout "nope" → "organize.default_layout" unknown (known profiles passed to validate);
    // folder_names {"anbernic-stock":{"gba":"SFC"}} → "organize.folder_names.anbernic-stock.gba" invalid
}

#[test]
fn options_from_settings() {
    // nest_in_roms_root false → flat; mode Copy → Some(Copy); Auto → None
}
```

- [ ] **Step 2: Run to see them fail.** - [ ] **Step 3: Implement** following the existing groups (`#[serde(default)]`, `Default` impl, `simple(...)` ranges `0.0..=1.0` for thresholds, `recent_seconds 0..=3600`, `keep_organize_journals_days 1..=3650`). `normalise` lowercases `companion_exts` and drops empty entries. Profile ids are validated the way `library.default_profile` already is.

- [ ] **Step 4: Run tests and gates** (`src/ipc/types.ts` changes in Task 29; the contract test may need the new group in `contract.sample.json` already — if `cargo test -p coverly-roms` fails on the sample, add `"organize": { …defaults… }` to the sample's settings object now).

- [ ] **Step 5: Commit**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize && git add crates src && git commit -m "feat(core): add organise settings"
```

---

### Task 24: Core — optional online lookup by CRC (ScreenScraper, only when configured)

**Files:**
- Modify: `crates/coverly-core/src/sources/screenscraper.rs` (`pub fn identify_by_hash`)
- Create: `crates/coverly-core/src/organize/online.rs` (`ScreenScraperLookup` implementing `OnlineLookup`)
- Test: `crates/coverly-core/tests/screenscraper.rs` (append, `MockServer`)

**Interfaces:**
- Produces:
  ```rust
  impl ScreenScraperSource<'_> {
      /// The game name for a ROM hash on system `sid` (cached like every SS answer).
      pub fn identify_by_hash(&self, sid: u32, file_name: &str, hash: &HashOutcome) -> Result<Option<String>, SourceError>;
  }
  pub struct ScreenScraperLookup<'a> { source: &'a ScreenScraperSource<'a>, db: &'a SystemDb }
  impl OnlineLookup for ScreenScraperLookup<'_> { /* ≤ 3 systems with a screenscraper id; ZipEntry-shaped HashOutcome */ }
  ```

- [ ] **Step 1: Write the failing tests** (reuse `tests/fixtures/screenscraper/jeu_infos_golden_sun.json`)

```rust
#[test]
fn identify_by_hash_returns_the_game_name() { /* serve jeuInfos for systemeid 12 → Some("Golden Sun") */ }
#[test]
fn the_lookup_asks_at_most_three_systems_and_never_when_quota_is_gone() {}
```

- [ ] **Step 2: Run to see them fail.** - [ ] **Step 3: Implement** `identify_by_hash` as a thin wrapper over the private `game(sid, &ArtQuery { hash: Some(hash), file_name, by_name: false, .. })` returning the first name. `ScreenScraperLookup::lookup(systems, file_name, crc, size)` builds `HashOutcome::ZipEntry { crc32, size, inner_name: file_name }`, asks each of the first 3 systems that have a `screenscraper` id, stops at the first hit and returns `(system, game)`. Errors and `Exhausted` → empty. The planner only builds this lookup when `available_sources()` contains `screenscraper`, the source is enabled in `settings.sources`, the user credentials exist, and `!opts.offline` (wired in Tasks 25 and 27).

- [ ] **Step 4: Run tests and gates.**

- [ ] **Step 5: Commit**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize && git add crates/coverly-core && git commit -m "feat(core): optional online identification by CRC for organise"
```

---

## Group O7 — CLI

### Task 25: CLI — `coverly organize` dry run (table and JSON)

**Files:**
- Create: `crates/coverly-cli/src/organize.rs`
- Modify: `crates/coverly-cli/src/main.rs` (`mod organize;`, `Command::Organize(Box<organize::OrganizeArgs>)`, exit-code doc)
- Test: `crates/coverly-cli/tests/cli_organize.rs`

**Interfaces:**
- Consumes: `coverly_core::organize::{plan_organize, OrganizeOptions, OrganizeEnv, OrganizePlan, …}`, `walk::OsMetaProbe`, `identify::dat::DatStore`, `crate::profile::check_root`, `crate::settings::{load_cli_settings, data_dir}`, `AppDirs`, `Cache`.
- Produces:
  ```rust
  #[derive(Debug, Args)]
  pub struct OrganizeArgs {
      source: PathBuf,
      #[arg(long, value_name = "PROFILE_ID")] layout: Option<String>,
      #[arg(long, value_name = "DIR")] target: Option<PathBuf>,
      #[arg(long)] flat: bool,
      #[arg(long, conflicts_with = "move_")] copy: bool,
      #[arg(long = "move")] move_: bool,
      #[arg(long, value_enum)] names: Option<NamesArg>,              // keep | catalogue
      #[arg(long = "assign", value_name = "FILE=SYSTEM_ID")] assign: Vec<String>,  // split on the LAST '='
      #[arg(long, value_name = "JOB_ID", conflicts_with_all = ["apply", "dry_run"])] resume: Option<String>,
      #[arg(long)] include_review: bool,
      #[arg(long)] offline: bool,
      #[arg(long, conflicts_with = "apply")] dry_run: bool,
      #[arg(long)] apply: bool,
      #[arg(long, value_enum, default_value_t = ReportArg::Table)] report: ReportArg,
      #[arg(long, value_name = "FILE")] output: Option<PathBuf>,
      #[arg(long, value_name = "DIR")] data_dir: Option<PathBuf>,
      #[arg(long, value_name = "DIR")] cache_dir: Option<PathBuf>,
      #[arg(long, hide = true)] database_url: Option<String>,
  }
  pub fn run(args: &OrganizeArgs) -> Result<(), Failure>;
  pub(crate) fn parse_assign(s: &str) -> Result<(PathBuf, String), Failure>; // exit 2 on a missing '='
  ```

- [ ] **Step 1: Write the failing tests** — `crates/coverly-cli/tests/cli_organize.rs`

```rust
//! `coverly organize` end to end on tempdir piles (never the real network:
//! `--offline` or a MockServer through `--database-url`).

use std::fs;
use std::path::Path;

use assert_cmd::Command;
use coverly_core::identify::sniff::fixtures as fx;
use predicates::str::contains;
use tempfile::TempDir;

/// `coverly <args…>` with private data/cache dirs and no user settings file.
fn coverly(data: &Path, cache: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::cargo_bin("coverly").unwrap();
    // SETTINGS_ENV (crates/coverly-cli/src/settings.rs) → a file that does not exist.
    cmd.env(SETTINGS_ENV, data.join("no-settings.json"));
    cmd.args(args).arg("--data-dir").arg(data).arg("--cache-dir").arg(cache);
    cmd
}

fn pile() -> TempDir {
    let tmp = TempDir::new().unwrap();
    for (name, bytes) in [("Game A.gba", fx::gba_tagged(1)), ("Game B.gb", fx::gb_tagged(0x00, 2)), ("Odd.gb", fx::zeros(32 * 1024))] {
        let p = tmp.path().join(name);
        fs::write(&p, bytes).unwrap();
        let old = filetime::FileTime::from_unix_time(1_600_000_000, 0);
        filetime::set_file_mtime(&p, old).unwrap();
    }
    fs::write(tmp.path().join("x.zip.crdownload"), b"x").unwrap();
    tmp
}

fn tree_digest(root: &Path) -> Vec<(String, u64)> { /* sorted (relative path, len) of every entry */ }

#[test]
fn dry_run_writes_nothing_and_lists_groups() {
    let (p, data, cache) = (pile(), TempDir::new().unwrap(), TempDir::new().unwrap());
    let before = tree_digest(p.path());
    coverly(data.path(), cache.path(), &["organize", "--offline"]).arg(p.path())
        .assert().success()
        .stdout(contains("ROMS/GBA/"))
        .stdout(contains("Game A.gba"))
        .stdout(contains("To review"))
        .stdout(contains("Odd.gb"))
        .stdout(contains("Staying where they are"))
        .stdout(contains("download not finished"))
        .stdout(contains("Dry run: nothing was written. Re-run with --apply"));
    assert_eq!(tree_digest(p.path()), before);
}

#[test]
fn json_report_has_mode_and_plan() {
    // --report json → {"mode":"dry-run","plan":{…},"job":null}; plan.counts.auto == 2, review == 1
}

#[test]
fn unknown_layout_and_bad_assign_are_usage_errors() {
    // --layout nope → exit 2; --assign "Odd.gb" (no '=') → exit 2; --assign Odd.gb=nope → exit 2
}

#[test]
fn a_missing_source_is_exit_3() {}

#[test]
fn assign_splits_on_the_last_equals() {
    // file "a=b.gb" (zeros) with --assign "a=b.gb=gbc" → plan shows it in GBC with "Chosen by you"
}
```

(`SETTINGS_ENV` is the variable name defined in `crates/coverly-cli/src/settings.rs`; copy its string value into a `const` in the test file. Add `filetime.workspace = true` to `crates/coverly-cli` `[dev-dependencies]` — already in `Cargo.lock`. Clap places `--data-dir`/`--cache-dir` after the source positional without trouble.)

- [ ] **Step 2: Run to see them fail**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize && cargo test -p coverly-cli --test cli_organize`
Expected: FAIL (unknown subcommand).

- [ ] **Step 3: Implement `organize.rs`**

`run`:
1. `check_root(&args.source)` (exit 3; message "library root" → reuse but the wording says "source": add `check_folder(path, what)` to `profile.rs` and keep `check_root` as a wrapper).
2. `source = std::path::absolute(&args.source)?`, `target = args.target.as_deref().map(std::path::absolute).transpose()?` (absolute, never canonical).
3. `cli = load_cli_settings()`; `opts = OrganizeOptions::from_settings(&cli.settings, args.layout.as_deref())`; `--flat`, `--copy`/`--move`, `--names`, `--offline` override; unknown layout → `Failure::usage` listing `profile_ids`.
4. Resolve dirs and `Cache` like `fill.rs:288-300`; `DatStore` only when `!opts.offline && opts.download_dats` (with `PoliteHttp` + `UreqHttp` from `s.net_config()`; `--database-url` overrides `DATABASE_BASE`).
5. `profiles = all_profiles(&cli.custom)` collected into `Vec<Profile>`.
6. `survey(...)` with a stderr progress line (only when stderr is a terminal, like `StderrProgress` in `fill.rs`), then `plan_from_survey(&survey, &opts, &decisions, &env)` where `decisions` come from `--assign` (each `FILE` relative to the source, matched against `item.sources[0]`; unknown file → exit 2 "not in the plan") and from `--include-review` (`Choice::Accept` for every review item without an `--assign`).
7. Dry run (default unless `--apply`, honouring `safety.dry_run_default` like `fill`): print the report. Apply: Task 26.
8. Table (`render_table`): header `Organise <source> → <target> (<layout>, <mode>)`; then per system folder: `ROMS/GBA/  (n)` and rows `  <source> → <target>  <High|Medium|—>  <why>`; `To review` with candidates `gb 0.50, gbc 0.50`; `Staying where they are` with reason text; `Other files: n` (count only); deep folders; warnings; final line `Dry run: nothing was written. Re-run with --apply to organise <n> files.` Reason/why texts are English strings built here from the enums (the core has no UI strings). Everything goes through `print_stdout` (terminal-safe).
9. JSON: `{"mode": "dry-run"|"apply", "plan": plan, "job": report|null}`; `--output` writes the file (like `fill::write_file`).

- [ ] **Step 4: Run tests and gates.**

- [ ] **Step 5: Commit**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize && git add crates/coverly-cli && git commit -m "feat(cli): add coverly organize with a dry-run report"
```

---

### Task 26: CLI — `--apply`, `--copy`, `--resume`, exit codes, `coverly undo` for organise jobs

**Files:**
- Modify: `crates/coverly-cli/src/organize.rs`, `crates/coverly-cli/src/undo.rs` (list shows the kind; undo of an organise job prints moved back / kept)
- Test: `crates/coverly-cli/tests/cli_organize.rs` (append)

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn apply_moves_and_undo_restores() {
    // organize --offline --apply → ROMS/GBA/Game A.gba and ROMS/GB/Game B.gb exist, Odd.gb stays;
    // `coverly undo --data-dir d` → tree_digest equals the one before apply (minus nothing)
}

#[test]
fn copy_to_another_target_keeps_the_pile() {
    // --target <other> --copy --apply → pile unchanged, <other>/ROMS/GBA/Game A.gba exists
}

#[test]
fn include_review_applies_the_best_candidate() {}

#[test]
fn undo_list_shows_organise_jobs() { /* `coverly undo --list` contains "organize" */ }

#[test]
fn a_read_only_target_is_exit_5_and_resume_finishes() {
    // unix only: target dir chmod 0o555 → exit 5 "paused"; chmod back; --resume <job> → success
}
```

- [ ] **Step 2: Run to see them fail.** - [ ] **Step 3: Implement**: `apply_organize(&plan, &ApplyEnv { data_dir, progress, cancel: &AtomicBool::new(false) })`; `ApplyError::NoSpace` → exit 1 with both numbers; `report.paused` → print the report then exit 5 with "paused: <kind>; resume with --resume <job>"; `--resume <job>` → `resume_organize`. After a successful apply print `Open it with: coverly fill <target> --profile <layout>` (the result opens with the chosen profile, spec §4.4). `undo.rs` `jobs_table` gains a `KIND` column; for organise reports print `moved back n, removed n copies, kept n changed, n folders removed`.

- [ ] **Step 4: Run tests and gates.**

- [ ] **Step 5: Commit**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize && git add crates/coverly-cli && git commit -m "feat(cli): apply, resume and undo organise jobs"
```

---

## Group O8 — Tauri commands

### Task 27: Tauri — `probe_folder`, `organize_plan` (plan store, progress events, cancel), weak detection in `open_library`

**Files:**
- Create: `src-tauri/src/organize/mod.rs` (`OrganizeManager`), `src-tauri/src/organize/view.rs`
- Modify: `src-tauri/src/lib.rs` (`pub mod organize;`, commands), `src-tauri/src/ipc.rs`, `src-tauri/src/state.rs` (`AppState.organize: OrganizeManager`), `src-tauri/src/library/mod.rs` (`choose_profile` uses `detect_with_fallback`; `ProfileView.detected` + evidence `system-folders`)
- Modify: `src-tauri/src/error.rs` (`NoPlan`, `LaterJobs(String)`, `NoSpace { needed, free }` as `IpcError::NoSpace(String)` "needed/free" detail)
- Test: `src-tauri/tests/organize_state.rs`

**Interfaces:**
- Produces (wire types, `Serialize`, snake_case fields, kebab-case enums, mirrored in `src/ipc/types.ts`):
  ```rust
  pub struct FolderProbeView { pub rom_like: usize, pub partial_downloads: usize, pub cloud_only: usize,
      pub sorted_folders: Vec<String>, pub writable: bool, pub broad_root: bool, pub truncated: bool }
  pub struct OrganizeRequest { pub source: String, pub layout: Option<String>, pub target: Option<String>,
      pub flat: Option<bool>, pub mode: Option<TransferMode>, pub names: Option<NameMode>, pub offline: Option<bool> }  // Deserialize
  pub struct OrganizePlanView { pub plan_id: u64, pub plan: OrganizePlan }   // OrganizePlan serialised as is
  pub struct OrganizeProgressEvent { pub plan_id: u64, pub phase: OrganizePhase, pub done: usize, pub total: usize }
  pub const ORGANIZE_PROGRESS_EVENT: &str = "organize-progress";
  // AppState:
  pub fn probe_folder(&self, path: &Path) -> Result<FolderProbeView, IpcError>;
  pub fn organize_plan(&self, req: &OrganizeRequest, decisions: &[Decision], events: Arc<dyn OrganizeEvents>) -> Result<OrganizePlanView, IpcError>;
  pub fn organize_cancel(&self, plan_id: u64) -> Result<(), IpcError>;
  pub fn organize_export_plan(&self, plan_id: u64) -> Result<Option<String>, IpcError>;  // save dialog in Rust
  ```
  IPC commands: `probe_folder(path)`, `organize_plan(request, decisions)` (decisions optional; re-plan reuses the cached `Survey` when `source/layout/target/offline` are unchanged — instant), `organize_cancel(planId)`, `organize_export_plan(planId)`.

- [ ] **Step 1: Write the failing tests** — `src-tauri/tests/organize_state.rs` (no Tauri runtime: `AppState::new` with tempdirs and a recording `OrganizeEvents`)

```rust
#[test]
fn probe_finds_rom_like_files_in_a_plain_folder() {}
#[test]
fn a_plan_is_stored_and_replanning_with_decisions_reuses_the_survey() {
    // first call emits progress events; second call with decisions emits none and returns a new plan_id
}
#[test]
fn cancelling_a_plan_in_progress_returns_cancelled() {}
#[test]
fn opening_an_organised_folder_without_markers_suggests_the_default_layout() {
    // ROMS/GBA/a.gba only → state.open(...) succeeds with profile anbernic-stock, detected, evidence ["system-folders"]
}
#[test]
fn a_plain_pile_still_answers_no_profile() { /* stable code for the UI */ }
```

- [ ] **Step 2: Run to see them fail** — `cargo test -p coverly-roms --test organize_state` (check the src-tauri package name in `src-tauri/Cargo.toml`).

- [ ] **Step 3: Implement**

`OrganizeManager` (behind `Mutex`): `next_id: u64`, `plans: HashMap<u64, StoredPlan { request, survey: Arc<Survey>, plan: OrganizePlan }>` (keep the last 4), `running: Option<(u64, Arc<AtomicBool>)>`. `organize_plan` builds `OrganizeEnv` from `FillConfig` (cache dir, data dir, endpoints, `PoliteHttp`), `settings.organize`, `all_profiles(custom)`; runs `survey` on the calling (blocking) thread with a 100 ms-throttled `events.progress(...)`; then `plan_from_survey`. Commands in `ipc.rs` are one-liners through `blocking(...)` like `open_library`, with a `TauriOrganizeEvents` that emits `ORGANIZE_PROGRESS_EVENT`. `library::choose_profile` calls `detect_with_fallback(root, &settings.organize.default_layout, &db)`.

- [ ] **Step 4: Run tests and gates.**

- [ ] **Step 5: Commit**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize && git add src-tauri && git commit -m "feat(app): plan organising a folder from the app backend"
```

---

### Task 28: Tauri — `organize_apply`, `organize_resume`, `organize_undo` (job guard, later jobs), history kind, contract

**Files:**
- Modify: `src-tauri/src/organize/mod.rs`, `view.rs`, `src-tauri/src/ipc.rs`, `src-tauri/src/lib.rs`, `src-tauri/src/state.rs`, `src-tauri/src/jobs/view.rs` (`JobHistoryView.kind`, `undoable`)
- Modify: `src-tauri/tests/contract.rs`, `src/ipc/contract.sample.json`, `src/ipc/contract.requests.json`
- Test: `src-tauri/tests/organize_state.rs` (append)

**Interfaces:**
- Produces:
  ```rust
  pub struct OrganizeJobView { pub job: Option<String>, pub plan_id: u64, pub state: OrganizeJobState /* applying|done|paused|cancelled|failed */,
      pub progress: Option<OrganizeProgressEvent>, pub report: Option<OrganizeReport>, pub target_root: String, pub layout: String }
  pub struct OrganizeUndoView { pub report: Vec<UndoReport>, pub later_jobs: Vec<JobHistoryView> }
  pub const ORGANIZE_CHANGED_EVENT: &str = "organize-changed";
  // AppState:
  pub fn organize_apply(&self, plan_id: u64, decisions: &[Decision], events: Arc<dyn OrganizeEvents>) -> Result<OrganizeJobView, IpcError>;
  pub fn organize_resume(&self, job: &str, events: Arc<dyn OrganizeEvents>) -> Result<OrganizeJobView, IpcError>;
  pub fn organize_cancel_apply(&self) -> Result<(), IpcError>;
  pub fn organize_undo(&self, job: &str, include_later: bool) -> Result<OrganizeUndoView, IpcError>;
  ```
  Commands: `organize_apply(planId, decisions)`, `organize_resume(job)`, `organize_cancel_apply()`, `organize_undo(job, includeLater)`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn apply_runs_behind_the_manual_guard_and_refuses_a_parallel_fill() {
    // while organize_apply runs (a slow MetaProbe/blocking ApplyEnv progress hook), start_fill → JobRunning,
    // and a second organize_apply → JobRunning
}
#[test]
fn apply_rechecks_decisions_against_the_stored_plan() { /* unknown item id → BadRequest; unknown plan → NoPlan */ }
#[test]
fn undo_refuses_when_a_later_fill_wrote_under_the_target_and_undoes_all_in_order_when_asked() {}
#[test]
fn an_interrupted_job_is_listed_as_resumable() { /* journal without done + plan.json → job_history kind organize, complete false */ }
```

- [ ] **Step 2: Run to see them fail.** - [ ] **Step 3: Implement**: `organize_apply` takes `jobs().begin_manual()?` (refuses while a fill job gathers/applies/is paused; makes fills refuse meanwhile), validates `plan_id`/decisions (`plan_from_survey` with the decisions — the backend recomputes the plan from its own survey, the UI never sends one), spawns a worker thread holding the guard that runs `apply_organize` and emits `ORGANIZE_CHANGED_EVENT`/`ORGANIZE_PROGRESS_EVENT`; a panic ends the job as `failed` (same `guarded` pattern as `jobs/mod.rs:293`). `organize_undo`: `later_jobs`; non-empty and `!include_later` → `IpcError::LaterJobs(list as detail)` plus the list in the view; else `undo_jobs_in_order`. `job_history` exposes `kind` and `undoable`. `From<ApplyError>` maps `NoSpace` → `IpcError::NoSpace`. Add every new view to `contract.rs` and the samples (both files), with `IpcError::{NoPlan, LaterJobs, NoSpace}` in the error sample list.

- [ ] **Step 4: Run tests and gates** (`cargo test -p coverly-roms`, `pnpm test -- contract`).

- [ ] **Step 5: Commit**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize && git add src-tauri src/ipc && git commit -m "feat(app): apply, resume and undo organise jobs from the app backend"
```

---

## Group O9 — UI

### Task 29: Frontend IPC — types, backend methods, Tauri adapter, mock organise backend

**Files:**
- Modify: `src/ipc/types.ts`, `src/ipc/backend.ts`, `src/ipc/tauriBackend.ts`, `src/ipc/errors.ts` (new codes), `src/ipc/contract.test.ts`
- Create: `src/ipc/mock/mockOrganize.ts`, `src/ipc/mock/mockOrganize.test.ts`
- Modify: `src/ipc/mock/mockBackend.ts` (compose `createMockOrganize`), `src/ipc/mock/fixture.ts` (a "Downloads" pile that answers `no-profile`)
- Test: `src/ipc/tauriBackend.test.ts`, `src/ipc/mock/mockOrganize.test.ts`

**Interfaces:**
- Produces (TS):
  ```ts
  export type TransferMode = 'move' | 'copy';
  export type NameMode = 'keep' | 'catalogue';
  export type IdLevel = 'hash' | 'header' | 'extension' | 'name' | 'online' | 'user';
  export interface Evidence { kind: IdLevel; system: string; weight: number; detail: string }
  export type LeftReason =
    | { kind: 'partial-download' } | { kind: 'recently-modified' } | { kind: 'cloud-only' }
    | { kind: 'not-a-rom'; what: 'web-page' | 'text' } | { kind: 'bios' } | { kind: 'damaged-archive' }
    | { kind: 'unsupported-archive' } | { kind: 'multi-game-archive'; count: number }
    | { kind: 'unsupported-system'; name: string } | { kind: 'unknown' } | { kind: 'duplicate'; of: string }
    | { kind: 'already-present'; at: string } | { kind: 'already-sorted'; folder: string } | { kind: 'symlink' }
    | { kind: 'set-incomplete'; missing: string[] } | { kind: 'set-outside-folder' } | { kind: 'shared-track' }
    | { kind: 'ambiguous-companion' } | { kind: 'too-big-for-fat' } | { kind: 'unreadable' } | { kind: 'collision-set' };
  export type OrganizeAction = { action: 'auto' } | { action: 'review' } | { action: 'left'; reason: LeftReason };
  export type OrganizeItem = OrganizeAction & {
    id: number; sources: string[]; kind: 'file' | 'archive' | 'set'; size: number; target: string | null;
    system_id: string | null; folder: string | null; confidence: number; method: IdLevel | null; why: Evidence[];
    alternatives: [string, number][]; rename: RenameReason | null; notes: ItemNote[]; companions: { source: string; target: string }[];
    catalogue_name: string | null;
  };
  export interface OrganizePlan { source_root: string; target_root: string; roms_dir: string; layout: string; mode: TransferMode;
    names: NameMode; items: OrganizeItem[]; counts: OrganizeCounts; sorted_folders: [string, string][]; deep_folders: string[];
    other_files: string[]; warnings: PlanWarning[]; broad_root: boolean; fat32: boolean; move_allowed: boolean;
    target_detected: string | null; created_at: string }
  export interface OrganizeRequest { source: string; layout?: string; target?: string; flat?: boolean; mode?: TransferMode;
    names?: NameMode; offline?: boolean }
  export type OrganizeDecision = { item: number } & ({ choice: 'accept' } | { choice: 'assign'; system: string } | { choice: 'leave' });
  export interface FolderProbe { rom_like: number; partial_downloads: number; cloud_only: number; sorted_folders: string[];
    writable: boolean; broad_root: boolean; truncated: boolean }
  // Backend gains:
  probeFolder(path: string): Promise<FolderProbe>;
  organizePlan(request: OrganizeRequest, decisions?: OrganizeDecision[]): Promise<OrganizePlanView>;
  organizeCancel(planId: number): Promise<void>;
  organizeApply(planId: number, decisions: OrganizeDecision[]): Promise<OrganizeJobView>;
  organizeResume(job: string): Promise<OrganizeJobView>;
  organizeCancelApply(): Promise<void>;
  organizeUndo(job: string, includeLater: boolean): Promise<OrganizeUndoView>;
  organizeExportPlan(planId: number): Promise<string | null>;
  onOrganizeProgress(cb: (e: OrganizeProgressEvent) => void): Promise<() => void>;
  onOrganizeChanged(cb: (e: OrganizeJobView) => void): Promise<() => void>;
  ```
  (`RenameReason`, `ItemNote`, `PlanWarning`, `OrganizeCounts`, `OrganizePlanView`, `OrganizeJobView`, `OrganizeUndoView`, `OrganizeProgressEvent` mirror Tasks 15–28 exactly; `JobHistoryItem` gains `kind: 'fill' | 'organize'` and `undoable: boolean`; `Settings` gains `organize`.)

- [ ] **Step 1: Write the failing tests**

```ts
// mockOrganize.test.ts
it('plans the downloads pile like the real sample', async () => {
  const b = createMockBackend();
  await expect(b.openLibrary('/Users/demo/Downloads/roms')).rejects.toMatchObject({ code: 'no-profile' });
  const probe = await b.probeFolder('/Users/demo/Downloads/roms');
  expect(probe.rom_like).toBe(17);
  const { plan } = await b.organizePlan({ source: '/Users/demo/Downloads/roms' });
  expect(plan.counts).toMatchObject({ auto: 16, review: 0 });
  expect(new Set(plan.items.flatMap((i) => (i.folder ? [i.folder] : [])))).toEqual(new Set(['GBA', 'GB', 'GBC', 'SFC']));
});

it('applies with progress, then opens with the profile and undoes', async () => {
  // organizeApply → onOrganizeProgress fires, final view state 'done', report.moved 16;
  // openLibrary(target, 'anbernic-stock') resolves with 16 entries; organizeUndo → restored 16
});

it('settles review items with decisions', async () => {
  // the mock "Mixed" pile has 2 review items; organizePlan(req, [{item: n, choice: 'assign', system: 'gbc'}]) → auto
});

it('refuses undo when a later fill exists unless asked', async () => {});

// tauriBackend.test.ts
it('maps organise commands to snake_case payloads', async () => {
  // organizeApply(3, decisions) → invoke('organize_apply', { planId: 3, decisions })
  // organizeUndo('j', true) → invoke('organize_undo', { job: 'j', includeLater: true })
});
```

- [ ] **Step 2: Run to see them fail** — `pnpm test -- src/ipc`.

- [ ] **Step 3: Implement** the types, backend interface, Tauri adapter (`call(...)` + `listen(...)` like the fill job methods) and a deterministic mock: two piles (`/Users/demo/Downloads/roms` with the 16 + 1 + junk shape of the real sample; `/Users/demo/Mixed` with 2 review items, 1 unknown, 1 duplicate and 2 partial downloads); progress via `setTimeout` steps of 4 items; apply turns the mock pile into a mock library (so `openLibrary(target, profileId)` returns entries in GBA/GB/GBC/SFC); undo restores the `no-profile` state. Escape names in any SVG the mock builds (reuse `svg.ts`).

- [ ] **Step 4: Run tests and all gates.**

- [ ] **Step 5: Commit**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize && git add src && git commit -m "feat(ui): organise IPC types, Tauri adapter and mock backend"
```

---

### Task 30: Frontend — `libraryStore.open(path, profileId?)`, `navStore` organise screen, `organizeStore`

**Files:**
- Modify: `src/stores/libraryStore.ts` (`open(path: string, profileId?: string)`), `src/stores/navStore.ts` (`screen: 'main' | 'settings' | 'organize'`, `openOrganize(source)`, `closeOrganize()`), `src/App.tsx` (render `OrganizeScreen` when `screen === 'organize'`)
- Create: `src/stores/organizeStore.ts`, `src/stores/organizeStore.test.ts`, `src/lib/organize.ts`, `src/lib/organize.test.ts`

**Interfaces:**
- Produces:
  ```ts
  // lib/organize.ts
  export function groupBySystem(plan: OrganizePlan): { folder: string; systemId: string; items: OrganizeItem[] }[];
  export function reviewBuckets(plan: OrganizePlan): { key: string; systems: string[]; items: OrganizeItem[] }[];
  export function leftItems(plan: OrganizePlan): OrganizeItem[];
  export function confidenceKey(c: number): 'organize.confidence.high' | 'organize.confidence.medium' | null; // ≥0.95 / ≥0.85
  export function leftReasonKey(r: LeftReason): { key: string; vars: Record<string, string | number> };
  export function whyKey(e: Evidence, systemName: (id: string) => string): { key: string; vars: Record<string, string> };
  export const ROWS_PER_GROUP = 200;
  // stores/organizeStore.ts
  interface OrganizeState {
    status: 'idle' | 'planning' | 'ready' | 'applying' | 'done' | 'paused' | 'error';
    source: string | null; request: OrganizeRequest | null; planId: number | null; plan: OrganizePlan | null;
    decisions: Record<number, OrganizeDecision>; progress: OrganizeProgressEvent | null; job: OrganizeJobView | null;
    undo: OrganizeUndoView | null; error: BackendError | null;
    start(source: string): Promise<void>; setOption(patch: Partial<OrganizeRequest>): Promise<void>;
    decide(itemIds: number[], choice: 'leave' | { system: string }): Promise<void>;
    apply(): Promise<void>; cancel(): Promise<void>; resume(): Promise<void>;
    undoJob(includeLater?: boolean): Promise<void>; openResult(): Promise<void>; reset(): void;
  }
  ```

- [ ] **Step 1: Write the failing tests**

```ts
// organizeStore.test.ts (mock backend via setBackendForTests or the existing helper used by jobStore.test.ts)
it('start → plan with progress → ready', async () => {});
it('setOption re-plans and keeps decisions', async () => {});
it('decide assigns a whole bucket in one call', async () => {});
it('apply → done → openResult opens the target with the layout profile', async () => {
  // expect libraryStore.open called with (target_root, 'anbernic-stock') and navStore back to 'main'
});
it('undo with later jobs keeps the list and undoes all when asked', async () => {});
it('an outdated plan answer is ignored (sequence guard)', async () => {});

// libraryStore.test.ts (append)
it('open passes the profile id through', async () => {});

// lib/organize.test.ts
it('buckets review items by candidate set', () => {});
it('maps every LeftReason to an i18n key that exists in both locales', () => {});
it('confidence labels', () => { expect(confidenceKey(0.97)).toBe('organize.confidence.high'); expect(confidenceKey(0.5)).toBeNull(); });
```

- [ ] **Step 2: Run to see them fail.** - [ ] **Step 3: Implement** (zustand like `jobStore.ts`; a `seq` counter guards stale answers; `decide` sends all accumulated decisions to `organizePlan(request, decisions)` so the backend recomputes targets; `openResult` = `useLibraryStore.getState().open(plan.target_root, plan.layout)` then `useNavStore.getState().closeOrganize()`).

- [ ] **Step 4: Run tests and all gates.**

- [ ] **Step 5: Commit**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize && git add src && git commit -m "feat(ui): organise store and open-with-profile"
```

---

### Task 31: Frontend — start-screen offer instead of the dead end

**Files:**
- Create: `src/components/organize/OrganizeOffer.tsx`, `src/components/organize/OrganizeOffer.test.tsx`
- Modify: `src/screens/StartScreen.tsx` (render the offer on `no-profile`), `src/screens/StartScreen.test.tsx`
- Modify: `locales/en/ui.json`, `locales/es/ui.json` (`start.organizeOffer.*`, `start.suggestSystemFolders`)

- [ ] **Step 1: Write the failing tests**

```tsx
it('offers to organise a plain pile on no-profile', async () => {
  // mock: open('/Users/demo/Downloads/roms') → no-profile; probe → rom_like 17
  // expect heading "This folder doesn't follow a known layout", text with 17, buttons
  // "Organise this folder…" (primary) and "Choose another folder"; no error banner
});
it('keeps the error text when nothing looks like a ROM', async () => {});
it('offers "Open as organised by system" with a layout select when only system folders exist', async () => {
  // probe sorted_folders ['GBA','GB'], rom_like 0 → button opens with the selected layout
});
it('warns on broad personal folders and shows 5 000+ when truncated', async () => {});
it('organise button opens the organise screen with the source', async () => {});
```

- [ ] **Step 2: Run to see them fail.**

- [ ] **Step 3: Implement**

`StartScreen`: after `status === 'error' && error.code === 'no-profile'`, call `backend.probeFolder(error.detail ?? openingPath)` once per error (effect keyed by the error object), store `probe` in local state; render `<OrganizeOffer path probe onOrganize onOpenSorted onPickOther />` instead of the banner when `probe.rom_like > 0 || probe.sorted_folders.length > 0`. The path comes from the store: add `lastPath` to `libraryStore` (set on `open`) rather than parsing `detail`.

`OrganizeOffer` (simple markup, existing classes only):

```tsx
export function OrganizeOffer({ probe, onOrganize, onOpenSorted, onPickOther }: Props) {
  const t = useT();
  const [layout, setLayout] = useState('anbernic-stock');
  const sortedOnly = probe.rom_like === 0 && probe.sorted_folders.length > 0;
  const count = probe.truncated ? `${PROBE_LIMIT_TEXT}+` : String(probe.rom_like);
  return (
    <section aria-labelledby="organize-offer-title" className="recent w-full max-w-2xl text-left">
      <h2 id="organize-offer-title" className="section-title">{t('start.organizeOffer.title')}</h2>
      {sortedOnly ? (
        <p className="hint">{t('start.suggestSystemFolders')}</p>
      ) : (
        <p>{t('start.organizeOffer.body', { roms: count })}</p>
      )}
      {probe.broad_root && <p className="hint">{t('start.organizeOffer.broadRoot')}</p>}
      {probe.truncated && <p className="hint">{t('start.organizeOffer.truncated', { count: PROBE_LIMIT_TEXT })}</p>}
      <div className="flex flex-wrap gap-3">
        {sortedOnly ? (
          <>
            <label>
              {t('organize.layout')}{' '}
              <select value={layout} onChange={(e) => setLayout(e.target.value)}>
                {/* built-in + custom profiles from settingsStore / list_profiles */}
              </select>
            </label>
            <button type="button" className="primary-button" onClick={() => onOpenSorted(layout)}>
              {t('start.organizeOffer.openSorted')}
            </button>
          </>
        ) : (
          <button type="button" className="primary-button" onClick={onOrganize}>
            {t('start.organizeOffer.action')}
          </button>
        )}
        <button type="button" className="ghost-button" onClick={onPickOther}>
          {t('start.organizeOffer.other')}
        </button>
      </div>
    </section>
  );
}
```

`PROBE_LIMIT_TEXT` = `new Intl.NumberFormat(locale).format(5000)`. Strings (both locales) exactly as spec §14 for `start.organizeOffer.{title,body,action,other,openSorted,broadRoot,truncated}`, `start.suggestSystemFolders`, `organize.layout`.

- [ ] **Step 4: Run tests and all gates** (`parity.test.ts` must pass).

- [ ] **Step 5: Commit**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize && git add src locales && git commit -m "feat(ui): offer to organise a folder that has no known layout"
```

---

### Task 32: Frontend — `OrganizeScreen`: options, summary, groups, review buckets, left in place

**Files:**
- Create: `src/screens/OrganizeScreen.tsx`, `src/screens/OrganizeScreen.test.tsx`
- Create: `src/components/organize/{OptionsBar,SystemGroups,ReviewBuckets,LeftInPlace}.tsx` (+ tests for `SystemGroups` and `ReviewBuckets`)
- Modify: `locales/en/ui.json`, `locales/es/ui.json` (`organize.*` keys of spec §14 used here)

- [ ] **Step 1: Write the failing tests**

```tsx
it('shows the summary line and the dry-run hint', async () => {
  // "ROMs: 16 · Systems: 4 · To review: 0 · Staying put: 1" and "This is a proposal. Nothing has moved yet."
});
it('groups by system with target folder, confidence and why', async () => {
  // <details> "ROMS/GBA/ (6)"; row "Tetris Worlds (USA).zip → ROMS/GBA/Tetris Worlds (USA).zip", "High",
  // why "CRC 7B729804 in the Game Boy Advance catalogue: Tetris Worlds (USA)"
});
it('renders at most 200 rows per group and a show-more button', async () => {
  // mock plan with 450 GBA items → 200 rows, button "Show 250 more"; groups closed (> 200 rows overall)
});
it('settles a review bucket with apply to all', async () => {});
it('lets the user rescue an unknown file with the system select', async () => {});
it('changing the layout re-plans and shows esde folder names', async () => {});
it('move is disabled with the reason when the source is read-only', async () => {});
it('hides the primary action when nothing would move and offers open library', async () => {});
it('renders hostile file names as text', async () => {
  // a source named '<img src=x onerror=alert(1)>.gba' appears as text, no <img> element
});
```

- [ ] **Step 2: Run to see them fail.**

- [ ] **Step 3: Implement**

`OrganizeScreen` layout: header with "Back" (`ghost-button`, `closeOrganize` + `organizeStore.reset`) and `h1.section-title` `organize.title`; `planning` → `organize.planning` with `{done}`/`{total}` and a Cancel button; `ready` → `OptionsBar`, summary `<p>` (`organize.summary`), `organize.dryRunHint` (`hint`), warnings (`organize.noCatalogue`, `organize.target.detected|mismatch`, `organize.mode.moveDisabled`, `organize.deepFolders`), `SystemGroups`, `ReviewBuckets` (only when any), `LeftInPlace`, actions (`organize.apply` primary with `{count}` = auto + decided items; `organize.exportPlan` ghost; cancel ghost). `applying|done|paused` → Task 33.

`OptionsBar`: four plain `<label><select>` controls (Layout: `list_profiles`; Target: "This folder" / "Another folder…" which calls `backend.pickFolder()`; Folders: `organize.nest.roms` with `{folder}` = the layout's `roms_root` / `organize.nest.flat`, hidden when `roms_root` is empty; Mode: move/copy, move `disabled` with `title` = reason when `!plan.move_allowed`; Names: keep/catalogue). Each `onChange` → `organizeStore.setOption(...)`.

`SystemGroups`: one `<details open={totalRows <= 200}>` per system, `<summary>` = `{folder}/ ({n})` + system name; `<ul>` of the first `visible[folder]` rows (`ROWS_PER_GROUP` steps), each row `source → target`, confidence label, `whyKey(item.why[0])`, `rename` note (`organize.renamed`), notes (`organize.multiRomZip`, `organize.noFolderOnDevice`), and a "Show {count} more" `ghost-button`.

`ReviewBuckets`: `organize.review.title` + `organize.review.hint`; per bucket `organize.review.bucket` (`{systems}` = names joined with " / ", `{count}`), a `<select>` (candidates by score + `organize.review.leave`) and `organize.review.applyAll`; inside a `<details>` the individual items with their own `<select>`.

`LeftInPlace`: `organize.left.title`, a `<ul>` with name + `leftReasonKey` text; `unknown` items get the full alphabetical system `<select>` (rescue).

All new strings from spec §14 in both locales (keys listed there), plus `organize.back`, `organize.cancel`, `organize.otherFiles` ("Other files: {count}" / "Otros archivos: {count}").

- [ ] **Step 4: Run tests and all gates.**

- [ ] **Step 5: Commit**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize && git add src locales && git commit -m "feat(ui): proposal screen for organising a folder by system"
```

---

### Task 33: Frontend — apply progress, result, open library, undo, resume; library entry point; settings section

**Files:**
- Create: `src/components/organize/OrganizeResult.tsx` (+ test)
- Modify: `src/screens/OrganizeScreen.tsx`, `src/components/library/Toolbar.tsx` (+ `LibraryScreen.tsx` wiring), `src/components/fill/JobsPanel.tsx` (history label "Organisation" for `kind: 'organize'`, Resume for interrupted organise jobs), `src/components/settings/LibrarySection.tsx` (Organise subsection), `locales/{en,es}/ui.json`
- Test: `src/screens/OrganizeScreen.test.tsx`, `src/components/library/Toolbar` tests (where they live), `src/components/settings/Sections.test.tsx`

- [ ] **Step 1: Write the failing tests**

```tsx
it('shows progress while applying and the result counts when done', async () => {
  // "Moving 4 of 16…" then "Moved: 16 · Copied: 0 · Skipped: 0 · Errors: 0"
});
it('open library opens the organised folder with the chosen layout', async () => {});
it('undo restores and reports', async () => { /* "Restored: 16 · Left alone because they changed: 0" */ });
it('undo lists later jobs and offers undo all in order', async () => {});
it('a paused apply offers resume', async () => {});
it('the library toolbar offers organise loose files when the scan saw loose ROMs', async () => {
  // LibraryView gains loose_files: number (Task 27 adds it to the view from walk-in-root);
  // button "Organise loose files…" opens the organise screen with source = library root, layout locked
});
it('settings show the organise defaults and save them', async () => {});
```

- [ ] **Step 2: Run to see them fail.**

- [ ] **Step 3: Implement**

`OrganizeResult`: counts line (`organize.result`), `organize.copiedNotRemoved` when any, a `<details>` with the per-item outcomes, **Abrir biblioteca** (primary → `openResult`) and **Deshacer** (ghost → `undoJob()`); when the backend answers `later-jobs`, show `organize.undo.laterJobs` with the job list and `organize.undo.all` (ghost → `undoJob(true)`); after undo show `organize.undone`. Paused → reason (reuse the fill pause texts) + `organize.resume`. Progress → `organize.applying` / `organize.copying` by mode.

Library entry point (spec §4.5): `LibraryView` gains `loose_files: number` (count of ROM-like files directly in the root or `roms_root`, computed in `src-tauri/src/library/mod.rs` with `probe_folder`-style listing of those two folders only — add it in this task to the Rust view, the contract and the mock). Toolbar shows `library.organizeLoose` (ghost) when `loose_files > 0`; it calls `navStore.openOrganize(root, { layout: profile.id, locked: true, mode: 'move' })`. After apply the store calls `libraryStore.open(root, profile.id)` (rescan).

Settings → Library: an "Organise" `<fieldset>` with default layout `<select>`, nest-in-ROMS checkbox, mode `<select>` (auto/move/copy), names `<select>`, download catalogues checkbox; thresholds and lists stay JSON-only (Advanced). New keys `settings.organize.*` in both locales.

- [ ] **Step 4: Run tests and all gates.**

- [ ] **Step 5: Commit**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize && git add src src-tauri locales && git commit -m "feat(ui): apply, undo and resume organising; organise loose files from a library"
```

---

## Group O10 — Docs and verification

### Task 34: Docs — README (en/es), architecture, tracker, changelog

**Files:**
- Modify: `README.md`, `README.es.md` (feature list + a short "Organise a folder of ROMs" section with the CLI example and the app flow)
- Create: `docs/04-architecture/organize.md` (pipeline diagram in text: walk → inspect → ladder → combine → target → plan → apply/journal/undo; thresholds; safety rules; journal ops; cache v2 tables)
- Modify: `docs/04-architecture/identification.md` (header magic table, evidence levels, "content support"), `docs/04-architecture/desktop-app.md` (organise screen, events, commands), `docs/04-architecture/fill-pipeline.md` (journal kinds, pruning), `docs/TRACKER.md` (rows O1–O10 with status and evidence; decisions O-D1..O-D11), `CHANGELOG.md` (Unreleased → Added/Changed)

- [ ] **Step 1: Write the docs** — every statement must match the code (open the files while writing). README example:

```bash
coverly organize ~/Downloads/roms                 # dry run: proposal by system
coverly organize ~/Downloads/roms --apply         # move into ~/Downloads/roms/ROMS/<SYS>/
coverly organize ~/Downloads/roms --target /Volumes/SD --copy --apply
coverly undo                                      # put everything back
```

Spanish README mirrors it ("Organizar una carpeta de ROMs").

- [ ] **Step 2: Check links** — Prettier ignores `docs/` and the READMEs, so check by hand: every relative link resolves (`grep -on "](\.\?[^)]*\.md)" README.md README.es.md docs/04-architecture/organize.md` and open each target), and `grep -n "organize.md" docs README*.md` lists the new page from `docs/README.md` and both READMEs.

- [ ] **Step 3: Commit**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize && git add README.md README.es.md docs CHANGELOG.md && git commit -m "docs: document organising loose ROMs by system"
```

---

### Task 35: Verification — real sample dry run (read-only), apply on a copy, E2E on the mock

**Files:**
- Create: `crates/coverly-core/tests/organize_real_sample.rs` (`#[ignore]`, gated by `COVERLY_SAMPLE_DIR`)
- Modify: `docs/TRACKER.md` (evidence)

- [ ] **Step 1: Write the ignored test**

```rust
//! Plan-only run against the owner's real pile (read-only). Run by hand:
//! COVERLY_SAMPLE_DIR=~/Documents/ROMs-Downloaded cargo test -p coverly-core --test organize_real_sample -- --ignored --nocapture

#[test]
#[ignore = "needs COVERLY_SAMPLE_DIR; plan only, never writes"]
fn the_real_sample_plans_as_the_spec_says() {
    let Ok(dir) = std::env::var("COVERLY_SAMPLE_DIR") else {
        eprintln!("COVERLY_SAMPLE_DIR not set; skipping");
        return;
    };
    let before = digest(Path::new(&dir));
    let plan = plan_offline_with_cached_dats(Path::new(&dir));   // real cache dir if present, else offline headers
    let per = count_by_folder(&plan);
    assert_eq!(per.get("GBA"), Some(&6));
    assert_eq!(per.get("GB"), Some(&6));
    assert_eq!(per.get("GBC"), Some(&2));
    assert_eq!(per.get("SFC"), Some(&2));
    assert_eq!(plan.counts.review, 0);
    assert!(matches!(left_reason(&plan, "3-D Tetris (USA).zip"), Some(LeftReason::NotARom { .. })));
    assert_eq!(digest(Path::new(&dir)), before, "read-only");
}
```

`digest` = sorted `(relative path, len, mtime)` of every entry, including hidden ones, so a stray `._*` or a moved file fails the test.

- [ ] **Step 2: Run it by hand** and paste the table into the tracker:

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize && COVERLY_SAMPLE_DIR=/Users/miguelangelesparzacalero/Documents/ROMs-Downloaded cargo test -p coverly-core --test organize_real_sample -- --ignored --nocapture
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize && cargo run -q -p coverly-cli -- organize /Users/miguelangelesparzacalero/Documents/ROMs-Downloaded
```

Expected: GBA 6, GB 6, GBC 2, SFC 2, review 0, `3-D Tetris (USA).zip` not a ROM (web page), `.DS_Store` counted as junk. If a real file lands elsewhere, investigate the evidence (`--report json`) and fix the ladder with a new synthetic test first; never weaken the assertions to match.

- [ ] **Step 3: Apply on a copy (never the original)**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize && rm -rf target/organize-sample && mkdir -p target/organize-sample && cp -X /Users/miguelangelesparzacalero/Documents/ROMs-Downloaded/*.zip target/organize-sample/ && touch -t 202601010000 target/organize-sample/*.zip
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize && cargo run -q -p coverly-cli -- organize target/organize-sample --data-dir target/organize-data --apply
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize && ls target/organize-sample/ROMS && find target/organize-sample -name '._*' | wc -l
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize && cargo run -q -p coverly-cli -- audit target/organize-sample --profile anbernic-stock
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize && cargo run -q -p coverly-cli -- undo --data-dir target/organize-data && ls target/organize-sample
```

Expected: `ROMS/{GB,GBA,GBC,SFC}`, zero `._*` files, the audit lists 16 ROMs in 4 systems (the normal flow works on the result), undo puts the 17 zips back at the top and removes `ROMS/`. Then compare against the reference names (read-only): `ls /Users/miguelangelesparzacalero/Documents/ROMS | grep -E '^(GB|GBA|GBC|SFC)$'`.

- [ ] **Step 4: E2E on the mock** (Chrome DevTools MCP against `pnpm dev`): open `/Users/demo/Downloads/roms` from the start screen → offer panel → "Organise this folder…" → proposal (GBA/GB/GBC/SFC, 0 to review) → switch layout to ES-DE and back → "Organise 16 files" → progress → result → "Open library" lands in the library screen with 16 entries → Fill wizard opens normally → back, Jobs history shows "Organisation" → Undo. Take screenshots of the offer, the proposal and the result; then repeat in Spanish (language switch) to check every string. Record the evidence (screenshot names, console clean) in `docs/TRACKER.md`.

- [ ] **Step 5: Run every gate one last time**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize && cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && cargo test -p coverly-core --test organize_plan -- --ignored && pnpm lint && pnpm typecheck && pnpm format:check && pnpm test && pnpm build
```

- [ ] **Step 6: Commit**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize && git add crates/coverly-core/tests/organize_real_sample.rs docs/TRACKER.md && git commit -m "test: verify organising on the real sample (read-only) and record the evidence"
```

---

## Self-review checklist (done while writing this plan)

- Spec coverage: §4.1 offer (Task 31), §4.2 proposal (32), §4.3 apply/result/undo/later jobs/interrupted (28, 33), §4.4 detection fallback (22, 27), §4.5 loose files in a library (33), §5 CLI (25, 26), §6.1 pipeline/memo/parallelism (14, 16), §6.2 DATs incl. arcade and download cap (13, 14), §6.3 header table (4–8), §6.4 extension classes (2, 12), §6.5 names/tags/online (14, 24), §6.6 combine (12), §6.7 sample (16, 35), §7.1 walk (11), §7.2 archives (9, 14), §7.3 sets (16), §7.4 companions (15), §7.5 duplicates (15, 16), §8 target layout incl. host folders, safe names, path length, FAT32 (3, 15), §9 safety incl. no-clobber, cross-volume verify, fingerprints, sharing retries, temps, free space, overrides, directory index (17–21), §10 journal/undo/pruning/forward compat/resume (18–21), §11 architecture (all), §12 settings (23, 33), §13 errors (21, 28), §14 strings (31–33), §15 tests (every task), §16 milestones (groups O1–O10).
- Type consistency: `Evidence`/`IdLevel` (Task 4) used by 12–16 and 29; `LeftReason` (11) extended in 15 (`SetOutsideFolder`, `CollisionSet` included from the start); `OrganizeAction` serialises flattened with `action`, matching the TS union in 29; `Decision` `{item, choice, system?}` identical in core (16), Tauri (27/28) and TS (29).
- No placeholders: every task names exact files, test names and bodies (or exact build/assert instructions for long fixture bodies), implementation code for the core algorithms, commands and commit messages.
