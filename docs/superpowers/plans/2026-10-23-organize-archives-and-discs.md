# Organise: Archives of Every Kind and Disc Games — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking. UI tasks (Group H) also REQUIRE frontend-design:frontend-design: follow the brand rules in `docs/08-brand/brand.md`, use the `src/components/ui` primitives, and critique each changed screen with screenshots before committing.

**Goal:** Coverly opens every common archive and compressor (zip/zip64 with every common method, 7z, RAR 4/5 through an optional external tool, tar, gz, bz2, xz, zstd, lz4, lzma, and single-file compressed ROMs) and recognises the container **by magic**. A wrong extension becomes a note, never "damaged". Cartridge ROMs in any container are identified like zipped ones. They are moved as they are when the profile's emulators load that container, and extracted when they do not. Disc games in any archive are identified from their disc headers (PS1, PS2, Saturn, Dreamcast, Sega CD, PC Engine CD, Neo Geo CD, 3DO). On apply, Coverly extracts the `.cue`/`.gdi` and its tracks into the system folder, using the profile's disc layout. Extraction is journaled per file, CRC-verified, free-space and FAT32 checked, resumable and undoable. The security requirements S1–S8 are designed in, and each one has a named test.

**Architecture:** Two new modules in `coverly-core`. `archive/` holds one `ArchiveReader` trait over every format: `detect`, `guard`, `names`, `budget`, `verify`, `lzma`, `zip`, `sevenz`, `tar`, `single`, `external`, `cache`, `testing`. `disc/` holds `SectorSource`, ISO 9660, Sega IP.BIN, the other system signatures, serials and fingerprints. `organize/inspect.rs` delegates every container to `archive::open_archive` and every disc to `disc::DiscReader`. `organize/plan.rs` gains `Transfer::Extract`, and `profiles` gains `disc_layout` and `accepts_containers`. `organize/apply.rs` commits Extract items in four phases (temps → validate → rename → archive). The journal (`write/mod.rs`) gains `Extracted`, `RolledBack`, `ArchiveTrash`, `ArchiveTrashed` and `ArchiveKept`. `write/trash.rs` is the Trash backend shared with dedupe phase B. The CLI, the Tauri views, the TS wire types, the mock, the UI and i18n follow. Dedupe phase B later implements `dedupe::source::{EntryListing, DiscProbe}` on top of `archive` + `disc`. This plan ships the seams for that (Task 22), not the adapters.

**Tech Stack:** Rust (workspace MSRV raised 1.90 → 1.93 by this plan, toolchain 1.99.1), edition 2021, `zip` 9 (features `deflate-flate2`, `deflate64`, `bzip2` only), `sevenz-rust2`, `lzma-rust2`, `ppmd-rust`, `ruzstd`, `lz4_flex`, `tar` 0.4, `bzip2` (with `libbz2-rs-sys`), `flate2`, `crc32fast`, `objc2-foundation` (macOS Trash), `windows-sys` (Windows Trash). Tests: `cargo test`, `insta` JSON snapshots, `assert_cmd`, Vitest + Testing Library. E2E: Chrome DevTools MCP on the Vite dev server with the mock backend.

**Spec:** [`docs/superpowers/specs/2026-10-23-organize-archives-and-discs.md`](../specs/2026-10-23-organize-archives-and-discs.md) (all sections; §13 S1–S8 are binding). It builds on [`2026-10-17-organize-loose-roms-design.md`](../specs/2026-10-17-organize-loose-roms-design.md) and feeds [`2026-10-18-duplicates-design.md`](../specs/2026-10-18-duplicates-design.md) phase B (plan [`2026-10-18-duplicates.md`](2026-10-18-duplicates.md), Task 25). Brand: [`docs/08-brand/brand.md`](../../08-brand/brand.md). Tracker: `docs/TRACKER.md`.

## Global Constraints

- **Worktree.** Work only in `/Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-discs`, branch `feat/organize-disc-archives`, which tracks `origin/feat/organize-disc-archives`. Every shell command starts with `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-discs &&`. Never switch branches and never touch `main`.
- **Push after every commit (mandatory).** A previous run lost all its work because nothing was pushed. Every task ends with the same three commands: `git commit …`, then `git push`, then `git status -sb`. The first line must read `## feat/organize-disc-archives...origin/feat/organize-disc-archives` with **no** `[ahead N]`. Never batch commits, and never end an agent with unpushed commits.
- **Commit messages.** Conventional (`feat(core): …`, `test(core): …`, `fix(core): …`, `build: …`, `feat(cli): …`, `feat(app): …`, `feat(ui): …`, `docs: …`), granular, in English. **No** `Co-Authored-By` trailer, no "Generated with", and no mention of Claude or AI anywhere (owner rule; it overrides any harness attribution).
- **TDD.** Write the failing test, run it and watch it fail for the expected reason, implement the change, then watch it pass. Each "Run" step names the exact command.
- **Gates.** Every task runs the gates before its commit and fixes anything red.
  - **G-RUST:** `cargo fmt --all -- --check && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace`, plus `scripts/check-zip-features.sh` and `scripts/check-licenses.sh` (both from Task 1). From Task 1 on, also run `scripts/dev/clippy-windows.sh`. It runs clippy for `x86_64-pc-windows-msvc`, a target that is installed. If a C build script cannot cross-compile, the script falls back to `cargo check` of `coverly-core` with `--target` and says so. Paste its last line into the commit-evidence note in `docs/TRACKER.md`.
  - **G-WEB** (any task touching `src/`, `locales/` or `src-tauri/`): `pnpm lint && pnpm typecheck && pnpm format:check && pnpm test && pnpm build && pnpm tokens:check`.
- **Real data is read-only.** The owner moved the download pile. The real case now lives at **`/Volumes/TinyExt/ROMs-Downloaded`** (it was `~/Documents/ROMs-Downloaded` in the spec; see deviation AP-1). Never write to `/Volumes/TinyExt/ROMs-Downloaded`, `/Users/miguelangelesparzacalero/Documents/ROMs-Downloaded`, `/Users/miguelangelesparzacalero/Documents/ROMS` or `/Volumes/System`, and never create `._*` files there. Only `7zz l`, `7zz t`, `ls`, `stat` and Coverly **plan-only** runs may touch them. Write tests use `tempfile` dirs. Manual apply runs use a copy under `/Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-discs/target/disc-sample/`, made with `cp -X` and deleted afterwards (`rm -rf target/disc-sample`).
- **Fixtures** are synthetic and generated in tests (crafted bytes, encoders from dev-dependencies), with no copyrighted bytes. The real names of spec §1 may appear as file names over synthetic content.
- **Dependencies.** The only new crates are those named in Task 1 (spec §5.1). Pin each one to a release published at least two weeks before it is added (check with `cargo info <crate>`). Each crate's licence must be in the allowlist of `scripts/check-licenses.sh` (MIT, Apache-2.0, BSD-2/3-Clause, ISC, Zlib, CC0-1.0, MIT-0, Unicode-3.0, BSL-1.0, AGPL-3.0-only for our own crates). `unrar`/`unrar_sys` are **forbidden** (spec §5.2), and the licence script fails if either one appears. `cargo fetch` runs once, in Task 1. After that, `cargo build --offline` must keep working.
- **Decoder discipline (S1, S3).** Every decoder runs on the calling thread. `lzma-rust2` `mt`, `sevenz-rust2` multithreading and `zip` features `lzma`, `xz`, `ppmd` and `zstd` stay **off**. The workspace keeps `panic = "unwind"`. LZMA decoders are only built through `archive::lzma::open_decoder` (enforced by `clippy.toml` `disallowed-methods`).
- **Windows hygiene** (Rust 1.99, `clippy -D warnings`). Unix-only tests and their helpers go in `#[cfg(unix)] mod unix_tests { … }`, and Windows-only ones in `#[cfg(windows)] mod windows_tests { … }`. A shared helper file never has items that one OS leaves unused. There is no `std::os::unix` outside `cfg(unix)`. Mtime comparisons tolerate 100 ns ticks (Windows) and 2 s (FAT). Snapshot paths are normalised to `/` before `insta` (the existing `tests/common` helper). Fake tools build as `.exe` on Windows.
- **No UI strings in the core.** Reasons and notes are enums plus data, and the UI and the CLI render them. Every UI string goes through `locales/en/ui.json` **and** `locales/es/ui.json` with the same keys and placeholders (`src/i18n/parity.test.ts`). Spanish copy follows spec §11.2 exactly. No exclamation marks, exact counts, and numbers in mono.
- **Hostile names.** React text only, CLI output through `table::terminal_safe`, and JSON exports as data.
- **IPC.** JS argument names are camelCase, payload fields snake_case, and errors `{ code, detail? }`. Codes are only ever added. No new commands (spec §12), only new fields and variants with serde defaults.
- **Servers and E2E.** Start the web UI with `pnpm dev --port 1455 --strictPort` (mock backend). Stop it **by port only**: `lsof -ti tcp:1455 | xargs kill`. E2E uses Chrome DevTools MCP `new_page` with `isolatedContext: "disc-archives"`. If the Chrome profile is locked, fall back to Playwright MCP.

## Deviations from the spec (decided in this plan)

- **AP-1: sample path.** The real case moved to `/Volumes/TinyExt/ROMs-Downloaded` (owner, 2026-10-24). The read-only sample test (Task 35) defaults to it through `COVERLY_SAMPLE_DIR`. The spec's §1 and §14 path is updated in Task 45. Because the pile is on an external volume, the sample test is `#[ignore]` and skips with a message when the volume is not mounted.
- **AP-2: MSRV 1.93.** `sevenz-rust2` needs Rust 1.93. The workspace `rust-version` moves from 1.90 to 1.93, because the local toolchain and CI already run 1.99. Dedupe plan line "1.93 once `feat/organize-disc-archives` raises the MSRV" becomes true here.
- **AP-3: Trash backend in `write/trash.rs`, no `trash` crate.** Spec §9.1 needs the exact Trash location for undo. The `trash` crate does not return it on macOS. We implement `TrashBackend` with three backends: macOS `NSFileManager trashItemAtURL:resultingItemURL:` (`objc2-foundation`, already in `Cargo.lock`), Windows `IFileOperation` (`windows-sys` features `Win32_UI_Shell`, `Win32_System_Com`) and Linux freedesktop written by us. Dedupe phase B uses this module instead of its DD-4 crate. Task 45 notes this in the dedupe plan.
- **AP-4: R1 research before the profile task.** Task 26 does the research of spec §15 R1 first and records it in `docs/02-research/layouts.md`. The spec's §8.2 and §8.3 tables are the defaults until a cited source contradicts them.
- **AP-5: fake external tools.** `coverly-fake-tool` is a `[[bin]]` of `coverly-core` with `required-features = ["test-util"]`. The S7/S8 tests are integration tests (`tests/archive_external.rs`) that find it through `env!("CARGO_BIN_EXE_coverly-fake-tool")`. It is told by `COVERLY_FAKE_TOOL_SCRIPT=<json path>` which crafted stdout, stderr and exit code to produce, and it names itself `7zz`, `unrar` or `bsdtar` by the copied file name. Tests copy it into a tempdir under those names.
- **AP-6: dedupe adapters stay in phase B.** This branch ships the seams of spec §10: `impl From<ArchiveError> for io::Error`, `archive::file_entries()` with a stable order, `disc::DiscIdentity::to_dedupe_id()` and `disc::serial`. A contract test proves an `EntryListing` and a `DiscProbe` can be written in a few lines (Task 22). `dedupe::source::ZipListing` is untouched here (phase B deletes it).
- **AP-7: open questions get the spec defaults.** Q1 extracts both `.cue` and `.gdi` with the `TwinSheets` note. Q2 uses plain per-game folders on ES-DE. Q3 never renames a mismatched archive. Q4 uses the OS Trash (the card's `.Trashes`), noted in the result.
- **AP-8: `BudgetSpent` is never `ErrorKind::Interrupted`.** `std` read loops and Coverly's own (`identify::hash::read_full`, `organize::inspect`) retry `Interrupted`, so a reader whose budget is spent would spin forever. `BudgetSpent { DecodedBytes }` maps to `QuotaExceeded` and `BudgetSpent { Time }` to `TimedOut`; dedupe phase B recognises both with `archive::is_budget_spent(&io::Error)` (a downcast, so a real disk-quota error is not mistaken for it) and returns `Ok(None)`. Test: `budget_spent_stream_ends_read_loops`.

## Review focus

1. **Data loss on apply.** No final file before phase C, a disk full mid-track, a save inside the archive, Trash unavailable, a crash between rename and journal, an interrupted undo. Tests: Tasks 31–34.
2. **Hostile archives.** S1–S8 and the §13.1 extras. Every row of spec §13 has its test names listed in the task that implements it (Tasks 5–17). The cross-check table at the end of this plan maps every S-test to a task.
3. **The real case.** Three 7z files named `.zip` become PS1/Dreamcast/Saturn Extract items with serials `SLUS-00862`, `T-40214N` and `T-5704H` and the exact byte counts. `3-D Tetris (USA).zip` stays "not a ROM", and the 13 cartridge zips do not change. Test: Task 35.
4. **Windows.** Device names, long temp paths, junctions, `.bat` tools, 100 ns mtimes and backslash snapshots. Script: `scripts/dev/clippy-windows.sh` on every task.

---

## File structure

```
Cargo.toml                                 rust-version 1.93; workspace deps for the new crates
clippy.toml                                (new) disallowed-methods: lzma-rust2 constructors
scripts/check-zip-features.sh              (new) fails if zip lzma/xz/ppmd/zstd features are on
scripts/check-licenses.sh                  (new) cargo metadata licence allowlist; forbids unrar*
scripts/dev/clippy-windows.sh              (new) clippy for x86_64-pc-windows-msvc, check fallback
THIRD-PARTY-NOTICES.md                     new crates
crates/coverly-core/
├─ Cargo.toml                              deps; [[bin]] coverly-fake-tool (test-util)
├─ src/lib.rs                              + pub mod archive; pub mod disc
├─ src/archive/mod.rs                      Format, Compressor, Entry, EntryKind, Method, ArchiveError,
│                                            Limits, ArchiveReader, open_archive, file_entries
├─ src/archive/detect.rs                   detect_container, ContainerMagic table, multi-volume names
├─ src/archive/guard.rs                    guarded, GuardedRead, Poison, panic hook (S1)
├─ src/archive/names.rs                    RelPath, UnsafeWhy, name_key, CP437
├─ src/archive/budget.rs                   Budget, BudgetKind, MemoryGate, Permit
├─ src/archive/verify.rs                   VerifyingReader (size + CRC32, take(size+1))
├─ src/archive/lzma.rs                     LzmaParams, Lzma2Params, mem_needed, open_decoder, xz walker
├─ src/archive/zip.rs                      zip backend; raw-method decoders 14/95/98/93 (S2, S3)
├─ src/archive/sevenz.rs                   bounded header walker (S4, S5) + sevenz-rust2 decode
├─ src/archive/tar.rs                      raw-mode tar (S6b), compressed tar, trailer cap (S6)
├─ src/archive/single.rs                   gz/bz2/xz/zst/lz4/lzma single-entry readers
├─ src/archive/external.rs                 tool discovery, run_tool hygiene, 7zz/unrar parsers (S7),
│                                            name/stream reads (S8), bsdtar pax transcode
├─ src/archive/cache.rs                    listing + head-bytes cache (§4.5)
├─ src/archive/testing.rs                  (test-util) crafted fixtures, mutation engine, fake tool glue
├─ src/bin/coverly-fake-tool.rs            (test-util) scripted fake 7zz/unrar/bsdtar
├─ src/disc/{mod,source,iso9660,sega,sheets,serial,signatures}.rs
├─ src/identify/sniff.rs                   ContainerMagic grows; disc serial reading moves to disc/
├─ src/scan/multifile.rs                   cue/gdi/m3u parsers re-exported through disc::sheets
├─ src/organize/inspect.rs                 delegates to archive/disc; container_for by magic
├─ src/organize/plan.rs                    Transfer, ExtractEntry, ArchiveAfter, SheetCheck, DiscId on
│                                            items, new LeftReason/ItemNote variants
├─ src/organize/target.rs                  extract targets, DiscLayout, name_key reservations, FolderFull
├─ src/organize/apply.rs                   Extract phases A–D, ExtractedKept, DiskFull pause
├─ src/organize/extract.rs                 (new) phase A/B/C engine used by apply and resume
├─ src/profiles/{mod,builtin,custom}.rs    disc_layout, accepts_containers, container_overrides
├─ src/settings/{model,validate}.rs        organize.external_tools
├─ src/progress.rs                         PauseKind::DiskFull
├─ src/write/mod.rs                        Op::{Extracted, RolledBack, ArchiveTrash, ArchiveTrashed,
│                                            ArchiveKept}; TransferRecord
├─ src/write/trash.rs                      (new) TrashBackend, OsTrash, FakeTrash (test-util)
├─ src/write/undo_organize.rs              undo of extract ops, archive restore
└─ tests/{archive_detect,archive_zip,archive_sevenz,archive_tar,archive_single,archive_external,
          archive_garbled,disc_identify,organize_inspect,organize_plan,organize_extract_apply,
          organize_extract_undo,organize_extract_resume,organize_real_sample}.rs
crates/coverly-cli/src/{organize.rs,undo.rs}, tests/cli_organize.rs
src-tauri/src/organize/{mod.rs,view.rs}, src-tauri/src/ipc.rs, src-tauri/tests/{organize_state,contract}.rs
src/ipc/{types.ts,contract.requests.json,contract.sample.json,tauriBackend.ts}
src/ipc/mock/mockOrganize.ts (+ test)
src/lib/organize.ts (+ test)
src/components/organize/{SystemGroups,LeftInPlace,OrganizeResult,OptionsBar}.tsx (+ tests)
src/components/organize/ExtractDetail.tsx (new, + test)
src/components/settings/LibrarySection.tsx (+ Sections.test.tsx)
locales/{en,es}/ui.json
docs/04-architecture/organize.md, docs/02-research/layouts.md, README.md, README.es.md,
CHANGELOG.md, docs/TRACKER.md, THIRD-PARTY-NOTICES.md
```

Fixture builders in `crates/coverly-core/src/archive/testing.rs` (`cfg(any(test, feature = "test-util"))`, re-exported as `coverly_core::archive::testing`):
`zip_with(entries: &[(name, bytes, ZipMethod)])`, `zip_raw_entry(method: u16, raw_payload)`, `lzma_props(lc, lp, pb, dict)`, `xz_stream(blocks: &[XzBlock])`, `zstd_frame(window_log, single_segment, content_size)`, `sevenz(files, SevenZShape { solid, coder, sheets_last })`, `sevenz_header_bytes(builder)` (crafted header writer for S4/S5), `tar(entries)`, `tar_gz(entries, trailer_zeros)`, `gz/bz2/xz/zst/lz4/lzma_alone(bytes)`, `mutate(seed, bytes, n) -> impl Iterator<Item = Vec<u8>>`, and `fake_tool(dir, name, script) -> PathBuf`.
Disc builders in `disc::testing`: `psx_track(raw, boot)`, `ps2_iso()`, `saturn_track(product)`, `dreamcast_track03(product, disc)`, `segacd_track()`, `pce_cd_track()`, `neocd_iso()`, `opera_3do()`, `cue_for(tracks)`, `gdi_for(tracks)`. They extend the existing `identify::sniff` test builders (`psx_iso`, `saturn`, `dreamcast_track`).

---

## Group A — Archive reader core and formats

### Task 1: Build: MSRV, new crates, licence and feature gates, Windows clippy script

**Files:**
- Modify: `Cargo.toml` (`rust-version = "1.93"`; workspace deps `sevenz-rust2`, `lzma-rust2`, `ppmd-rust`, `ruzstd`, `lz4_flex` (`frame`), `tar`, `bzip2` (`libbz2-rs-sys`, default features off), and `zip` keeps `default-features = false`)
- Modify: `crates/coverly-core/Cargo.toml`: `zip = { workspace = true, features = ["deflate-flate2", "deflate64", "bzip2"] }`, the new deps, `flate2` with `zlib-rs`, `objc2-foundation` under `cfg(target_os = "macos")`, `windows-sys` features `Win32_UI_Shell`, `Win32_System_Com`, `Win32_Foundation`, `Win32_Storage_FileSystem`. Dev-dependency `sevenz-rust2` with its `compress` feature, plus the encoder features of `lzma-rust2`, `ruzstd`, `lz4_flex` and `bzip2`.
- Create: `scripts/check-zip-features.sh`, `scripts/check-licenses.sh`, `scripts/dev/clippy-windows.sh`, `clippy.toml`
- Modify: `THIRD-PARTY-NOTICES.md`
- Test: `crates/coverly-core/tests/archive_zip.rs` (new): `zip_crate_cannot_decode_lzma`

- [ ] **Step 1: Pick versions.** For each crate, run `cargo info <crate>` and choose the newest release published on or before 2026-10-10 (two weeks old). Write the chosen version and licence into the Task 1 row of `docs/TRACKER.md`.
- [ ] **Step 2: Write the failing test.** `zip_crate_cannot_decode_lzma` builds a zip with one method-14 entry through `testing::zip_raw_entry(14, …)`, then asserts `ZipArchive::by_index(0)` returns `Err` ("unsupported compression method") and `by_index_raw(0)` returns `Ok`. Add a minimal `archive/testing.rs` with `zip_raw_entry` only, plus `pub mod archive { #[cfg(any(test, feature = "test-util"))] pub mod testing; }` in `lib.rs`.
- [ ] **Step 3: Run** `cargo test -p coverly-core --test archive_zip` (fails to compile: module missing). Then add the module and run it again: it passes once the zip features are restricted. If it passes before the restriction, an `lzma` feature is leaking in; find it with `cargo tree -e features -i zip`.
- [ ] **Step 4: Scripts.**
  - `check-zip-features.sh`: `cargo tree -e features -i zip --target all | grep -E 'zip feature "(lzma|xz|ppmd|zstd|lzma-rs|_deflate-any.*zopfli)"' && exit 1 || exit 0`, with a clear message.
  - `check-licenses.sh`: `cargo metadata --format-version 1 --locked` piped to `python3 -I` with an inline allowlist. It splits SPDX `OR`/`AND`, needs every `AND` term allowed and at least one `OR` alternative, and fails on `unrar` or `unrar_sys`.
  - `clippy-windows.sh`: `cargo clippy -p coverly-core --all-targets --all-features --target x86_64-pc-windows-msvc -- -D warnings`. If that fails with a C-toolchain error (`cc-rs`, `link.exe`), it falls back to `cargo check -p coverly-core --all-targets --all-features --target x86_64-pc-windows-msvc` and prints `windows: check-only (no C cross toolchain)`.
  - `clippy.toml`: `disallowed-methods` lists the `lzma_rust2::LzmaReader::new*`, `Lzma2Reader::new*` and `XzReader::new*` constructors, each with the reason "use archive::lzma::open_decoder (S3)".
- [ ] **Step 5: Run gates** G-RUST + `scripts/check-zip-features.sh && scripts/check-licenses.sh && scripts/dev/clippy-windows.sh` + `cargo build --offline`.
- [ ] **Step 6: Commit and push:** `git add Cargo.toml Cargo.lock clippy.toml crates/coverly-core scripts THIRD-PARTY-NOTICES.md docs/TRACKER.md && git commit -m "build: add archive decoders, raise MSRV to 1.93 and gate zip features and licences" && git push && git status -sb`

### Task 2: Core: `archive` types, errors and trait

**Files:**
- Create: `crates/coverly-core/src/archive/mod.rs`
- Modify: `crates/coverly-core/src/lib.rs` (`pub mod archive;`)
- Test: unit tests in `archive/mod.rs`

**Interfaces (produced):**
```rust
pub enum Format { Zip, SevenZip, Rar { v5: bool }, Tar { outer: Option<Compressor> }, Single(Compressor) }
pub enum Compressor { Gzip, Bzip2, Xz, Zstd, Lz4, LzmaAlone }
pub enum EntryKind { File, Dir, Symlink, Hardlink, Other }
pub enum Method { Stored, Deflate, Deflate64, Bzip2, Lzma { dict: u32 }, Lzma2 { dict: u32 }, Xz,
                  Ppmd { order: u8, mem: u32 }, Zstd { window: u64 }, Lz4, Bcj(BcjKind), Delta, Other(String) }
pub struct Entry { pub index: usize, pub name: RelPath, pub raw_name: Vec<u8>, pub kind: EntryKind, pub size: u64,
                   pub packed: Option<u64>, pub crc32: Option<u32>, pub encrypted: bool, pub method: Method,
                   pub solid_block: Option<u32>, pub mtime: Option<SystemTime>, pub unsafe_name: Option<UnsafeWhy> }
pub enum Tool { Unrar, SevenZip, Bsdtar }
pub enum BudgetKind { DecodedBytes, Time }
pub enum UnsafeWhy { Absolute, DotDot, Device, Illegal, Encoding, Link, TooLong }
pub enum ArchiveError { Damaged { detail: String }, Unsupported { format: String, need: Option<Tool> }, Encrypted,
  NeedsTooMuchMemory { needed: u64, cap: u64 }, BudgetSpent { what: BudgetKind }, TooManyEntries { count: u64 },
  TooLarge { declared: u64 }, UnsafeEntry { name: String, why: UnsafeWhy }, MultiVolume { parts: Vec<PathBuf> },
  ToolFailed { tool: Tool, status: Option<i32> }, Io(io::Error) }
pub struct Limits { pub max_entries: u64, pub max_header: u64, pub max_entry_size: u64, pub max_total_size: u64,
  pub decoder_mem_cap: u64, pub plan_head_budget: u64, pub plan_listing_budget: u64, pub tar_trailer_allowance: u64,
  pub plan_wall_time: Duration, pub external_tools: bool }   // Default = spec §4.4 values
pub trait ArchiveReader: Send { fn format(&self) -> Format; fn entries(&self) -> &[Entry];
  fn open_entry<'a>(&'a mut self, index: usize, budget: &'a mut Budget) -> Result<Box<dyn Read + 'a>, ArchiveError>;
  fn stream_entries(&mut self, wanted: &[usize], budget: &mut Budget,
      sink: &mut dyn FnMut(&Entry, &mut dyn Read) -> Result<(), ArchiveError>) -> Result<(), ArchiveError>; }
pub fn open_archive(path: &Path, limits: &Limits) -> Result<Option<Box<dyn ArchiveReader>>, ArchiveError>; // stub: Ok(None)
/// File entries only, archive order (dirs/links removed): the dedupe `entry: usize` index space (spec §10).
pub fn file_entries(r: &dyn ArchiveReader) -> impl Iterator<Item = &Entry>;
impl From<ArchiveError> for io::Error { /* InvalidData: Damaged, UnsafeEntry, ToolFailed; Unsupported: Unsupported,
  Encrypted, MultiVolume; OutOfMemory: NeedsTooMuchMemory; FileTooLarge: TooManyEntries, TooLarge;
  QuotaExceeded/TimedOut: BudgetSpent bytes/time, never Interrupted (AP-8; dedupe maps it to Ok(None)
  through is_budget_spent); Io: itself */ }
pub fn is_budget_spent(e: &io::Error) -> bool;
```

- [ ] **Step 1: Write the failing tests:** `limits_default_matches_spec` (every value in spec §4.4), `archive_error_to_io_kind` (table over every variant), `file_entries_skips_dirs_and_links_keeps_order` (a fake reader with File, Dir, File, Symlink, File → indices 0, 2, 4).
- [ ] **Step 2: Run** `cargo test -p coverly-core archive::` (fails: no module).
- [ ] **Step 3: Implement** the types. `RelPath` stays a placeholder newtype until Task 4 (`pub struct RelPath(String)`). `Budget` is a placeholder until Task 5. Doc comments name the spec sections.
- [ ] **Step 4: Run** the same command (passes), then G-RUST and the Windows script.
- [ ] **Step 5: Commit and push:** `git add crates/coverly-core/src && git commit -m "feat(core): add the archive reader trait, entry model and errors" && git push && git status -sb`

### Task 3: Core: container detection by magic (`archive/detect.rs`)

**Files:**
- Create: `crates/coverly-core/src/archive/detect.rs`
- Modify: `crates/coverly-core/src/identify/sniff.rs` (`ContainerMagic` grows to `Zip, ZipEocdOnly, SevenZip, Rar4, Rar5, Gzip, Bzip2, Xz, Zstd, Lz4, Tar, Chd, Cab, Arj, Lzh, Compress, Ecm`; the `sniff()` call site still sets `Sniff.container`)
- Test: `crates/coverly-core/tests/archive_detect.rs`

**Interfaces:**
```rust
pub fn detect_container(head: &[u8], len: u64, tail: Option<&[u8]>) -> Option<ContainerMagic>; // tail = last 64 KiB + 22 (EOCD)
pub fn by_extension(ext: &str) -> Option<ContainerMagic>;      // fallback table of spec §3.1
pub fn multi_volume_part(name: &str) -> Option<VolumePart>;    // .7z.00N, .partN.rar, .rNN, .zNN; VolumePart { first: bool, set_stem }
```

- [ ] **Step 1: Write the failing tests.** One positive and one near-miss per row of spec §3.1:
  - `zip_local_header`, `zip_empty_eocd`, `zip_spanned_marker_is_multi_volume`, `zip_with_sfx_prefix_found_by_eocd`, `eocd_offsets_outside_file_rejected`
  - `sevenz_magic`, `sevenz_major_version_nonzero_rejected`
  - `rar4_magic`, `rar5_magic`
  - `gzip_magic`, `gzip_reserved_flags_rejected`
  - `bzip2_magic_with_block_magic`, `bzip2_text_bzh9_rejected`, `bzip2_empty_stream`
  - `xz_magic_crc_checked`, `xz_bad_header_crc_rejected`
  - `zstd_magic`, `zstd_skippable_frame`, `zstd_reserved_bit_rejected`
  - `lz4_frame`, `lz4_legacy`, `lz4_bad_version_bits_rejected`
  - `tar_ustar_checksum_ok`, `tar_ustar_bad_checksum_rejected`, `v7_tar_by_extension_and_checksum`
  - `lzma_alone_only_by_extension_with_plausible_props`
  - `chd_magic`
  - `cab_arj_lzh_compress_ecm_recognised`
  - `multi_volume_name_patterns` (table)
  - `html_named_zip_is_no_container` (the `3-D Tetris (USA).zip` shape)
- [ ] **Step 2: Run** `cargo test -p coverly-core --test archive_detect` (fails).
- [ ] **Step 3: Implement.** `detect_container` reads only the 128 KiB prefix already read by `inspect_file`, plus the optional tail.
- [ ] **Step 4: Run** the tests and `cargo test -p coverly-core --test sniff` (the existing sniff tests stay green), then G-RUST and the Windows script.
- [ ] **Step 5: Commit and push:** `git add crates/coverly-core && git commit -m "feat(core): detect archive and compressor containers by magic" && git push && git status -sb`

### Task 4: Core: safe entry names (`archive/names.rs`)

**Files:**
- Create: `crates/coverly-core/src/archive/names.rs` (replaces the `RelPath` placeholder)
- Test: unit tests in `names.rs`

**Interfaces:**
```rust
pub struct RelPath(String);                       // validated, '/'-separated, NFC
impl RelPath { pub fn parse(raw: &str) -> Result<RelPath, UnsafeWhy>; pub fn as_str(&self) -> &str;
  pub fn components(&self) -> impl Iterator<Item=&str>; pub fn file_name(&self) -> &str; pub fn parent(&self) -> Option<&str> }
pub fn decode_zip_name(raw: &[u8], utf8_flag: bool, unicode_extra: Option<(u32, &[u8])>) -> Result<String, UnsafeWhy>; // CP437 default
pub fn decode_utf16_name(units: &[u16]) -> Result<String, UnsafeWhy>;                                                // unpaired surrogate → Encoding
pub fn name_key(s: &str) -> String;               // NFC + lowercase (collisions, spec §8.2)
pub fn component_fits(s: &str) -> bool;           // ≤ 255 UTF-16 units AND ≤ 255 UTF-8 bytes
```

- [ ] **Step 1: Write the failing tests:** `zip_slip_dotdot`, `absolute_unix`, `absolute_windows_drive`, `unc_path`, `backslash_dotdot`, `nul_rejected`, `empty_components_normalised`, `device_names_rejected` (a table of `CON`, `prn.txt`, `Aux.bin`, `NUL`, `COM0`–`COM9`, `COM¹`, `LPT1.cue`, `CONIN$`, `conout$`, trailing dot, trailing space, `a:b`, and each of the characters `< > " | ? *` and U+0001 → `Device`/`Illegal`), `nfc_normalised_once`, `zip_cp437_name_decoded`, `zip_unicode_extra_bad_crc_ignored`, `zip_unicode_extra_good_crc_used`, `sevenz_unpaired_surrogate_unsafe`, `name_key_folds_case_and_nfc`, `component_255_units_and_bytes`.
- [ ] **Step 2: Run** `cargo test -p coverly-core archive::names` (fails).
- [ ] **Step 3: Implement** with `unicode-normalization` (already a dependency), and a static CP437 table.
- [ ] **Step 4: Run** the tests, then G-RUST and the Windows script.
- [ ] **Step 5: Commit and push:** `git add crates/coverly-core/src/archive && git commit -m "feat(core): validate archive entry paths against zip-slip and device names" && git push && git status -sb`

### Task 5: Core: budgets, memory gate and verifying reader

**Files:**
- Create: `crates/coverly-core/src/archive/budget.rs`, `crates/coverly-core/src/archive/verify.rs`
- Test: unit tests in both

**Interfaces:**
```rust
pub struct Budget { /* decoded bytes left, deadline: Option<Instant>, cancel: Option<&AtomicBool> via Arc */ }
impl Budget { pub fn plan_head(l: &Limits) -> Budget; pub fn plan_listing(l: &Limits) -> Budget; pub fn apply() -> Budget; // no wall time
  pub fn charge(&mut self, n: u64) -> Result<(), ArchiveError>; pub fn spent(&self) -> u64; pub fn with_cancel(self, c: Arc<AtomicBool>) -> Budget }
pub struct MemoryGate; impl MemoryGate { pub fn global() -> &'static MemoryGate; pub fn acquire(&self, bytes: u64) -> Result<Permit, ArchiveError> }
pub struct Permit; // Drop releases; acquire(> cap) → NeedsTooMuchMemory; waits on a Condvar otherwise
pub struct ChargedRead<R> { inner: R, budget: &mut Budget }       // charges every byte read
pub struct VerifyingReader<R> { /* take(size+1); at EOF: size and crc32 must match else Damaged{"size"|"crc"} */ }
```

- [ ] **Step 1: Write the failing tests:** `budget_spent_is_not_damage`, `charged_read_counts_every_byte`, `apply_budget_has_no_deadline`, `gate_blocks_until_release` (two threads, gate 512 MiB, permits of 300 MiB), `gate_request_over_cap_refused`, `multi_coder_folder_acquires_sum_once`, `verifying_reader_size_short_damaged`, `verifying_reader_overrun_damaged` (`zip_entry_inflates_past_declared_size` shape), `verifying_reader_crc_mismatch_damaged`, `verifying_reader_no_crc_checks_size_only`.
- [ ] **Step 2: Run** `cargo test -p coverly-core archive::budget archive::verify` (fails).
- [ ] **Step 3: Implement.** `MEMORY_GATE` is 512 MiB, test-overridable through `#[cfg(any(test, feature = "test-util"))] pub fn with_gate_cap(...)`.
- [ ] **Step 4: Run** the tests, then G-RUST and the Windows script.
- [ ] **Step 5: Commit and push:** `git add crates/coverly-core/src/archive && git commit -m "feat(core): bound archive decoding by byte budget, memory gate and size/CRC checks" && git push && git status -sb`

### Task 6: Core: panic guard (S1)

**Files:**
- Create: `crates/coverly-core/src/archive/guard.rs`
- Modify: `src-tauri/src/main.rs` and `crates/coverly-cli/src/main.rs` (call `coverly_core::archive::guard::install_panic_hook()` once at startup; the core also calls it lazily via `Once` on first `guarded`)
- Test: unit tests in `guard.rs`

**Interfaces:**
```rust
const _: () = assert!(cfg!(panic = "unwind"));
pub fn install_panic_hook();                                     // once; suppresses print only when IN_GUARD > 0 (thread-local depth)
pub fn guarded<T>(f: impl FnOnce() -> Result<T, ArchiveError>) -> Result<T, ArchiveError>; // panic → Damaged{"decoder panicked"}
pub struct GuardedRead<R> { inner: R, poison: Arc<AtomicBool> }  // each read() inside guarded; after a panic every call → Damaged
pub struct Poison(Arc<AtomicBool>);                              // readers check it at every entry point; poisoned readers never cached
```

- [ ] **Step 1: Write the failing tests:** `panicking_backend_maps_to_damaged` (a fake `ArchiveReader` whose stream `read()` panics), `poisoned_reader_stays_damaged`, `panic_hook_suppresses_only_guarded` (spawn an unguarded thread that panics, capture the hook output through a test hook sink, and assert it was printed; a guarded panic was not), `gate_permit_released_on_panic` (acquire inside `guarded`, panic, then acquire the full cap again without blocking), `nested_guard_depth_restored`.
- [ ] **Step 2: Run** `cargo test -p coverly-core archive::guard` (fails).
- [ ] **Step 3: Implement** with `catch_unwind(AssertUnwindSafe(f))`. The thread-local depth counter uses a drop guard, so it is restored on unwind.
- [ ] **Step 4: Run** the tests, then G-RUST and the Windows script.
- [ ] **Step 5: Commit and push:** `git add crates/coverly-core/src crates/coverly-cli/src src-tauri/src && git commit -m "feat(core): run archive decoders inside a panic guard" && git push && git status -sb`

### Task 7: Core: LZMA parameters, one decoder constructor, xz walker (S2, S3)

**Files:**
- Create: `crates/coverly-core/src/archive/lzma.rs`
- Test: unit tests in `lzma.rs`

**Interfaces:**
```rust
pub struct LzmaParams { pub lc: u8, pub lp: u8, pub pb: u8, pub dict_raw: u32 }
impl LzmaParams { pub fn from_props(props: [u8; 5]) -> Result<Self, ArchiveError>; // props byte < 225
  pub fn mem_needed(&self) -> u64; }                          // dict_raw + 2 * (0x300 << (lc+lp)) + FIXED; NEVER min(dict, declared)
pub struct Lzma2Params { pub dict_raw: u32 }                  // from the 1-byte prop; lc+lp ≤ 4
pub fn check(needed: u64, cap: u64) -> Result<(), ArchiveError>;
/// The ONLY constructor of lzma-rust2 readers in the crate (clippy disallowed-methods elsewhere).
pub fn open_decoder<'a, R: Read + 'a>(kind: LzmaKind, params: &LzmaParams, declared: Option<u64>, cap: u64, src: R)
    -> Result<Box<dyn Read + 'a>, ArchiveError>;              // passes dict_raw and mem_limit = cap; records the value in a test hook
pub struct XzWalker<R>;                                       // stream header, then per block: block header filter flags → Lzma2Params
                                                              // checked before that block's decoder exists; index/footer; multi-stream
#[cfg(any(test, feature = "test-util"))] pub fn last_decoder_dict() -> Option<u32>;
```

- [ ] **Step 1: Write the failing tests:**
  - S3: `tiny_declared_size_huge_dict_refused` (declared 1 KiB, dict 1.5 GiB → `NeedsTooMuchMemory`, no decoder built), `cap_check_and_decoder_use_same_dict`, `lzma_alone_declared_size_ignored_for_memory`, `lzma_alone_unknown_size_uses_dict` (`u64::MAX`), `lzma_lc_lp_term_counted`.
  - S2: `xz_second_block_huge_dict_refused`, `xz_second_stream_huge_dict_refused`.
  - Round trips: `lzma_round_trip_small` and `xz_round_trip_two_blocks` (the encoder comes from the dev-dependency).
- [ ] **Step 2: Run** `cargo test -p coverly-core archive::lzma` (fails).
- [ ] **Step 3: Implement.** The xz walker parses the block headers itself (filter id 0x21 = LZMA2, with optional BCJ/Delta before it). Every LZMA2 chunk with a dict reset that carries new properties is checked with the same formula.
- [ ] **Step 4: Run** the tests and `cargo clippy -p coverly-core --all-targets --all-features -- -D warnings`, which proves `disallowed-methods` stays clean. Then G-RUST and the Windows script.
- [ ] **Step 5: Commit and push:** `git add crates/coverly-core/src/archive && git commit -m "feat(core): check LZMA, LZMA2 and xz dictionaries before any decoder exists" && git push && git status -sb`

### Task 8: Core: zip backend (S2 for zip methods)

**Files:**
- Create: `crates/coverly-core/src/archive/zip.rs`
- Modify: `crates/coverly-core/src/archive/testing.rs` (`zip_with`, `zip_raw_entry` for methods 14/95/98/93, a zip64 builder, symlink and AES flags)
- Test: `crates/coverly-core/tests/archive_zip.rs`

**Behaviour:**
- Listing: today's `central_directory_claim` precheck moves here from `inspect.rs`. Then `ZipArchive::new` and `by_index_raw` for every entry inside `guarded`. More than `max_entries` → `TooManyEntries`, never `Damaged`. Every per-entry vector is built with `checked_add` sums, never `with_capacity(untrusted)`.
- Kinds: external attributes `S_IFLNK` → `Symlink`; a directory flag → `Dir`.
- Encryption: an encrypted entry is `encrypted: true` with its CRC kept. If every entry is encrypted, the result is `Encrypted`.
- Methods 0/8/9/12 decode through the crate. Methods 14/95/98/93 decode by us from `by_index_raw`. List time reads the property header (LZMA 2+2+5 bytes, PPMd8 2 bytes, XZ stream + first block header, zstd frame header) and sets `Method` with dict/mem/window. Open time re-parses the bytes actually fed to the decoder, checks them again and builds through `lzma::open_decoder` / `ppmd_rust` (cap check before construction) / `ruzstd` (window check per frame).
- Every stream: `GuardedRead<VerifyingReader<ChargedRead<…>>>`.

- [ ] **Step 1: Write the failing tests:**
  - Formats: `zip_lists_stored_deflate_deflate64_bzip2`, `zip64_large_declared_sizes_listed` (crafted zip64 extra), `zip_reads_lzma_xz_ppmd_zstd_entries`.
  - S2: `zip_lzma_huge_dict_refused_at_list_and_open`, `zip_ppmd_mem_255_refused_with_lower_cap` (cap lowered to 128 MiB), `zip_xz_second_block_huge_dict_refused`, `zip_zstd_window_over_cap_refused`, `entry_changed_between_list_and_open_rechecked` (rewrite the props in the file between list and open).
  - Extras: `zip_claims_one_million_entries` → `TooManyEntries`; `zip_symlink_entry_not_extracted` (kind `Symlink`); `zip_aes_entry_note`; `zip_entry_inflates_past_declared_size` → `Damaged`; `zip_cp437_name_decoded`.
- [ ] **Step 2: Run** `cargo test -p coverly-core --test archive_zip` (fails).
- [ ] **Step 3: Implement** `ZipReader: ArchiveReader`. `stream_entries` = `open_entry` per wanted entry, in central-directory order.
- [ ] **Step 4: Run** the tests, then G-RUST, `scripts/check-zip-features.sh` and the Windows script.
- [ ] **Step 5: Commit and push:** `git add crates/coverly-core && git commit -m "feat(core): read zip and zip64 with capped LZMA, XZ, PPMd and zstd decoders" && git push && git status -sb`

### Task 9: Core: 7z bounded header walker (S4, S5)

**Files:**
- Create: `crates/coverly-core/src/archive/sevenz.rs` (walker part)
- Modify: `crates/coverly-core/src/archive/testing.rs` (`sevenz_header_bytes` crafted header writer: start header, `kHeader`/`kEncodedHeader`, pack/unpack/substreams info, `FilesInfo` properties)
- Test: `crates/coverly-core/tests/archive_sevenz.rs`

**Interfaces:**
```rust
pub(crate) struct Walked { pub folders: Vec<FolderInfo>, pub files: Vec<WalkedFile>, pub header_end: u64 }
pub(crate) struct FolderInfo { pub coders: Vec<CoderInfo>, pub unpack_size: u64, pub crc: Option<u32>, pub pack_pos: u64, pub pack_size: u64, pub mem_needed: u64 }
pub(crate) fn walk(file: &mut File, len: u64, limits: &Limits) -> Result<Walked, ArchiveError>;
```
Rules from spec §13 S4/S5:
- The start header CRC is checked. `next_header_offset + 32 + next_header_size ≤ len` is computed with `checked_add`. The next-header size must be ≤ `MAX_HEADER` and its CRC must match.
- `kEncodedHeader` must have exactly one folder, with coders LZMA/LZMA2 (+ BCJ/Delta). AES → `Encrypted`; any other coder → `Damaged`. Dicts are checked with the S3 formula, the unpack size must be ≤ `MAX_HEADER`, the pack range must lie inside the file, and the folder CRC is required. The header is decoded through `lzma::open_decoder`, and its first byte must be `0x01` (`kHeader`), else `Damaged`.
- The full `kHeader` parse caps folders and files at `MAX_ENTRIES` and coders per folder at 8. Bind pairs must be consistent, and substreams must sum to the unpack sizes. Every property size must lie inside the header. Exactly `numFiles` UTF-16 NUL-terminated names, bit vectors sized to `numFiles`, and the parse must consume the header to its last byte and end on `kEnd`. An unknown property, `kAdditionalStreamsInfo`, an external flag or anti-items → `Damaged`.
- The parse is iterative, with no recursion.

- [ ] **Step 1: Write the failing tests:**
  - S4: `nested_encoded_header_is_damaged`, `encoded_header_unpack_size_over_cap`, `encoded_header_dict_over_cap`, `encoded_header_two_folders_damaged`, `encoded_header_ppmd_coder_damaged`, `encoded_header_without_crc_damaged`, `next_header_offset_overflow_damaged`, `decoded_header_with_too_many_files` (→ `TooManyEntries`).
  - S5: `sevenz_declared_files_mismatch_names`, `sevenz_unknown_property_fails_closed`, `sevenz_trailing_bytes_after_kend_damaged`, `sevenz_pack_pos_outside_file`, `sevenz_coder_count_over_cap`.
  - Extras: `sevenz_ppmd_mem_4gib_refused`, `sevenz_total_size_over_cap` (→ `TooLarge`), `sevenz_encrypted_header` (→ `Encrypted`), `sevenz_reparse_point_not_extracted` (attribute 0x400 → `Symlink`), `sevenz_unpaired_surrogate_unsafe`.
- [ ] **Step 2: Run** `cargo test -p coverly-core --test archive_sevenz` (fails).
- [ ] **Step 3: Implement** the walker.
- [ ] **Step 4: Run** the tests, then G-RUST and the Windows script.
- [ ] **Step 5: Commit and push:** `git add crates/coverly-core && git commit -m "feat(core): walk 7z headers with fail-closed bounds before any decoding" && git push && git status -sb`

### Task 10: Core: 7z decoding over `sevenz-rust2` with crate-view cross-check (S5)

**Files:**
- Modify: `crates/coverly-core/src/archive/sevenz.rs` (`SevenZReader: ArchiveReader`)
- Modify: `crates/coverly-core/src/archive/testing.rs` (`sevenz(files, SevenZShape)` through the dev-dependency writer: LZMA:26 non-solid with 12 blocks; LZMA2:24 solid with sheets last; BCJ; PPMd)
- Test: `crates/coverly-core/tests/archive_sevenz.rs`

**Behaviour:**
- The listing comes from `walk`. Before the first decoded byte, the crate is opened on the **same** handle after a size + mtime re-check. Its parsed view (entry count, names, sizes, CRCs, folder ↔ file map) is compared with `Walked`; any difference → `Damaged`. A test counter `crate_opened()` counts the crate opens.
- Each folder acquires the **sum** of its coders' `mem_needed` from `MemoryGate` in one request.
- `open_entry` decodes the folder up to the entry and charges the skipped bytes to the budget.
- `stream_entries` makes one pass per folder in storage order and decodes unwanted entries to `io::sink` (still charged).
- Unknown coders: if `external_tools` is on and `7zz`/`7z` exists, the archive is routed to `external.rs` (Task 15). Otherwise → `Unsupported { format: "7z (<coder>)", need: Some(Tool::SevenZip) }`.

- [ ] **Step 1: Write the failing tests:** `sevenz_crate_view_differs_damaged` (a test hook swaps the header between walk and crate open), `sevenz_declared_files_mismatch_names_crate_never_called` (asserts `crate_opened() == 0`), `sevenz_valid_real_shaped_archives_list` (both §1 shapes, synthetic), `sevenz_solid_open_entry_charges_prefix` (sheet after 8 MiB of tracks with a 4 MiB head budget → `BudgetSpent`), `sevenz_stream_entries_one_pass_per_folder`, `sevenz_bcj_and_ppmd_decode`, `sevenz_crc_mismatch_damaged`.
- [ ] **Step 2: Run** `cargo test -p coverly-core --test archive_sevenz` (fails).
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the tests, then G-RUST and the Windows script.
- [ ] **Step 5: Commit and push:** `git add crates/coverly-core && git commit -m "feat(core): decode 7z folders only after the crate agrees with our header walk" && git push && git status -sb`

### Task 11: Core: single-file compressors (`archive/single.rs`)

**Files:**
- Create: `crates/coverly-core/src/archive/single.rs`
- Test: `crates/coverly-core/tests/archive_single.rs`

**Behaviour:**
- One virtual entry. Gzip uses `MultiGzDecoder` with `FNAME` validated by `RelPath`, else the archive stem (`game.gba.gz` → `game.gba`).
- bz2 uses `bzip2` with the Rust backend.
- xz goes through `XzWalker`.
- zstd uses `ruzstd`. The window comes from each frame header. With `Single_Segment` the window is the declared content size, checked against the cap. Skippable frames are drained against the budget and never allocated.
- lz4 uses the `lz4_flex` frame decoder (block max 4 MiB).
- `.lzma` alone uses `LzmaParams`.
- The size comes from the frame/footer/ISIZE when present, else from a bounded decode (≤ 64 MiB) or `None`.
- The CRC is the gzip CRC32 and is used as the entry CRC. For others the CRC comes from a bounded decode.

- [ ] **Step 1: Write the failing tests:** `gz_fname_used_and_validated`, `gz_unsafe_fname_falls_back_to_stem`, `gz_concatenated_members`, `bz2_round_trip`, `xz_round_trip`, `zst_round_trip`, `lz4_round_trip`, `lzma_alone_round_trip`, `zstd_second_frame_huge_window_refused`, `zstd_single_segment_huge_content_size_refused`, `zstd_skippable_frame_bounded`, `single_size_unknown_measured_within_budget`.
- [ ] **Step 2: Run** `cargo test -p coverly-core --test archive_single` (fails).
- [ ] **Step 3: Implement** `SingleReader: ArchiveReader`. A shared `open_compressor(c, reader, limits) -> Box<dyn Read>` is reused by tar (Task 12).
- [ ] **Step 4: Run** the tests, then G-RUST and the Windows script.
- [ ] **Step 5: Commit and push:** `git add crates/coverly-core && git commit -m "feat(core): read gz, bz2, xz, zstd, lz4 and lzma single-file ROMs" && git push && git status -sb`

### Task 12: Core: tar and compressed tar (S6, S6b)

**Files:**
- Create: `crates/coverly-core/src/archive/tar.rs`
- Test: `crates/coverly-core/tests/archive_tar.rs`

**Behaviour:**
- Raw mode: `tar::Archive::entries()?.raw(true)` and never `unpack`. The header checksum is checked on every header.
- Records: a pax `x`/`g` or GNU `L`/`K` record is read only when its declared size is ≤ 1 MiB, else `Damaged`. Pax `length key=value\n` records are parsed by us with checked arithmetic. `path`/`size`/`linkpath` apply to the next header. Metadata records are capped at ≤ 8 per entry and ≤ `MAX_ENTRIES` per archive.
- Kinds: typeflags `1`/`2`/`3`/`4`/`6` → `Hardlink`/`Symlink`/`Other`. Sparse (`S`, GNU sparse pax) and `M` → `Other`.
- Inside a compressor: walk to the end marker (two zero blocks), then drain at most `tar_trailer_allowance` (1 MiB), charged to the budget. More data → `BudgetSpent` at plan time and `Damaged { "trailing data" }` with the apply budget. The outer check (gzip CRC/ISIZE, xz check, zstd checksum) must pass at EOF within the allowance.
- Listing completeness flag: `listing_complete: bool` is true when the end marker was reached within the listing budget. Task 28 uses it to decide whether Extract is allowed.
- No stored CRC: entries get `crc32: None`; `stream_entries` computes it.

- [ ] **Step 1: Write the failing tests:**
  - S6: `tar_gz_huge_zero_trailer_bounded` (end marker + 4 GiB of zeros, compressed; finishes in < 1 s; decoded ≤ allowance + tar size), `tar_xz_huge_zero_trailer_bounded`, `tar_zst_huge_zero_trailer_bounded`, `tar_bz2_huge_zero_trailer_bounded`, `tar_gz_second_member_after_marker_bounded`, `tar_gz_normal_padding_ok` (GNU 10 KiB record).
  - S6b: `tar_pax_header_huge_size_refused`, `tar_gnu_longname_huge_refused`, `tar_pax_length_overflow_damaged`, `tar_pax_path_overrides_validated_by_relpath`, `tar_sparse_entry_other`.
  - Extras: `tar_symlink_then_file_through_it` (symlink `d → /etc` then `d/passwd`: `d/passwd` is `UnsafeEntry`/`Link`), `tar_hardlink_refused`, `tar_many_tiny_entries_capped`, `tar_listing_incomplete_when_budget_spent`, `tar_gz_bad_crc_at_eof_damaged_on_apply_budget`.
- [ ] **Step 2: Run** `cargo test -p coverly-core --test archive_tar` (fails).
- [ ] **Step 3: Implement** `TarReader: ArchiveReader`. Note: an entry under a path whose prefix is a symlink entry of the same archive gets `unsafe_name: Some(Link)`.
- [ ] **Step 4: Run** the tests, then G-RUST and the Windows script.
- [ ] **Step 5: Commit and push:** `git add crates/coverly-core && git commit -m "feat(core): read tar and compressed tar with capped metadata and trailer" && git push && git status -sb`

### Task 13: Core: fake external tool and process hygiene (`run_tool`)

**Files:**
- Create: `crates/coverly-core/src/bin/coverly-fake-tool.rs` (`[[bin]] name = "coverly-fake-tool" required-features = ["test-util"]`)
- Create: `crates/coverly-core/src/archive/external.rs` (discovery + `run_tool`)
- Modify: `crates/coverly-core/src/archive/testing.rs` (`fake_tool(dir, "7zz"|"unrar"|"bsdtar", &Script)`; on Windows the file is named `.exe`)
- Test: `crates/coverly-core/tests/archive_external.rs`

**Interfaces:**
```rust
pub struct FoundTool { pub tool: Tool, pub path: PathBuf, pub version: String, pub rar5: bool }
pub fn discover(extra_roots_to_avoid: &[&Path]) -> Vec<FoundTool>;   // absolute PATH entries not cwd/source/target + fixed per-OS locations
pub(crate) struct ToolRun { pub stdout: Vec<u8>, pub stderr: Vec<u8> /* ≤ 64 KiB */, pub status: Option<i32> }
pub(crate) fn run_tool(t: &FoundTool, args: &[&OsStr], archive: &Path, out_cap: u64, timeout: Duration,
                       sink: Option<&mut dyn Write>) -> Result<ToolRun, ArchiveError>;
```
Hygiene (spec §5.2): only an absolute regular executable (`.bat`/`.cmd`/`.ps1` refused); `Command` without a shell; `--` before the archive; the cwd is a fresh empty tempdir; `env_clear()` plus `PATH` (the fixed safe list), `LC_ALL=C.UTF-8` (`LANG` on macOS), and `HOME`/`USERPROFILE` set to the tempdir; stdin null; both pipes drained on their own threads; the timeout kills the process group (Unix `setsid`/`killpg`) or the Job object (`KILL_ON_JOB_CLOSE`) and reaps it.

- [ ] **Step 1: Write the failing tests:** `discover_ignores_relative_and_cwd_path_entries`, `discover_parses_version_banner`, `unrar_older_than_5_skipped_for_rar5`, `tool_env_is_cleared` (the fake tool prints `RAR` and `TAR_READER_OPTIONS`; both unset), `tool_cwd_is_empty_tempdir`, `tool_timeout_killed` (the fake tool spawns a sleeping grandchild; both are gone afterwards), `stderr_flood_no_deadlock` (10 MiB of stderr), `listing_over_cap_rejected` (stdout > 16 MiB), and `bat_tool_refused_on_windows` inside `#[cfg(windows)] mod windows_tests`.
- [ ] **Step 2: Run** `cargo test -p coverly-core --features test-util --test archive_external` (fails).
- [ ] **Step 3: Implement.** On Unix, `pre_exec` with `setsid` goes under `#[cfg(unix)]`. On Windows, the Job object uses `windows-sys` `Win32_System_JobObjects` (add that feature). There are no unused helpers on either OS.
- [ ] **Step 4: Run** the tests, then G-RUST and the Windows script.
- [ ] **Step 5: Commit and push:** `git add crates/coverly-core Cargo.lock && git commit -m "feat(core): find and run external archive tools in a cleared, bounded process" && git push && git status -sb`

### Task 14: Core: strict 7-Zip and unrar listing parsers (S7)

**Files:**
- Modify: `crates/coverly-core/src/archive/external.rs`
- Test: `crates/coverly-core/tests/archive_external.rs`

**Behaviour:**
- 7-Zip: `7zz l -slt -ba -sccUTF-8 -p- -- <archive>`. unrar: `unrar lt -cfg- -p- -scur -- <archive>` (the exact switches are pinned per version banner in a `const` table).
- Every record must contain **exactly** the key set the tool prints for that format, each key once, in order, with no unknown or continuation lines.
- The record count must equal the tool's own summary: a second non-`-ba` `7zz l` run gives `N files, M folders`, and the file size + mtime must be equal before the first run and after the second; unrar has its totals line. The sum of sizes must equal the summary total.
- Any name with U+0000–U+001F or U+007F–U+009F, or one that fails `RelPath`, rejects the whole listing as `Unsupported { "unsafe names" }`.
- `ToolReader: ArchiveReader` records `ExtractSource::Tool { tool, version }`.
- bsdtar is never text-parsed (Task 15).

- [ ] **Step 1: Write the failing tests:** `forged_newline_name_rejected` (`a.bin\nPath = evil`), `forged_record_with_full_key_set_caught_by_count`, `record_key_set_mismatch_rejected`, `record_count_vs_summary_mismatch_rejected`, `size_total_mismatch_rejected`, `archive_changed_between_two_list_runs_rejected`, `rar_without_tool_is_unsupported_need_unrar`, `seven_zip_listing_round_trip` (a fake tool with a real-shaped output for the Tetris PS1 shape), `unrar_listing_round_trip`.
- [ ] **Step 2: Run** `cargo test -p coverly-core --features test-util --test archive_external` (fails).
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the tests, then G-RUST and the Windows script.
- [ ] **Step 5: Commit and push:** `git add crates/coverly-core && git commit -m "feat(core): parse 7-Zip and unrar listings strictly against forged records" && git push && git status -sb`

### Task 15: Core: external reads by name, by stream, and through bsdtar's pax transcode (S8)

**Files:**
- Modify: `crates/coverly-core/src/archive/external.rs`
- Test: `crates/coverly-core/tests/archive_external.rs`

**Behaviour (spec §13 S8):**
- `is_ambiguous(name, listing)`: true when the name contains `* ? [ ] { } \ !`, starts with `-`/`@`, has an NFC case-insensitive twin, or another entry starts with `name + '/'`.
- Unambiguous names: `unrar p -inul -cfg- -p- -- <a> <name>` / `7zz e -so -spd -r- -p- -- <a> <name>`.
- Ambiguous names: a full stream (`unrar p -inul -cfg- -p- -- <a>` / `7zz e -so -spd -p- -- <a>`) is sliced by the listed file sizes in listing order. Each slice is verified (size + CRC), and only the wanted slices are kept. Any mismatch → `Damaged`. Stream mode is refused (`Unsupported { "ambiguous names" }`) when the listing has links, encrypted entries or entries without a size.
- Exit status ≠ 0 → `ToolFailed`.
- bsdtar: `bsdtar -c --format=pax -f - @<archive>` is read by the Task 12 tar reader (no slicing). Exit status is checked at EOF.

- [ ] **Step 1: Write the failing tests:** `wildcard_name_read_by_stream`, `case_twin_read_by_stream`, `dir_prefix_twin_read_by_stream`, `dash_name_not_an_option`, `bang_name_ambiguous_for_7zip`, `stream_mode_refused_with_link_entries`, `stream_slice_crc_mismatch_damaged`, `name_read_crc_verified` (a fake `unrar` returns another entry's bytes → `Damaged`), `tool_nonzero_exit_is_tool_failed`, `bsdtar_pax_transcode_names_exact`.
- [ ] **Step 2: Run** `cargo test -p coverly-core --features test-util --test archive_external` (fails).
- [ ] **Step 3: Implement** `ToolReader::open_entry` / `stream_entries`.
- [ ] **Step 4: Run** the tests, then G-RUST and the Windows script.
- [ ] **Step 5: Commit and push:** `git add crates/coverly-core && git commit -m "feat(core): read external-tool entries by unambiguous name or verified stream" && git push && git status -sb`

### Task 16: Core: `open_archive` dispatch, multi-volume, reader cache

**Files:**
- Modify: `crates/coverly-core/src/archive/mod.rs` (`open_archive`)
- Create: `crates/coverly-core/src/archive/cache.rs`
- Test: `crates/coverly-core/tests/archive_detect.rs` (dispatch cases), unit tests in `cache.rs`

**Behaviour:**
- Dispatch: magic first; extension only as a fallback (spec §3.2 rules 1–7). No magic + archive extension → try that reader; text/HTML → `Ok(None)` (the caller keeps `NotARom`).
- Multi-volume: the flag or the name pattern → `MultiVolume { parts }`, collecting the sibling parts by name.
- RAR: `external_tools` off or no tool → `Unsupported { "RAR", need: Some(Unrar) }`.
- Cache: `Cache` (≤ 8 entries keyed by `(path, size, mtime_ns)`) holds the listing, the parsed header and head bytes (≤ 4 MiB per entry, ≤ 32 MiB in total), never a decoder. Poisoned readers are not inserted.

- [ ] **Step 1: Write the failing tests:** `sevenz_named_zip_opens_as_sevenz` (the root-cause case), `zip_named_rar_opens_as_zip`, `html_named_zip_is_none`, `multi_volume_7z_001_002_one_error_with_parts`, `rar_without_tool_unsupported`, `cache_invalidated_by_mtime_change`, `cache_never_holds_decoder` (gate fully free after `open_entry` returns with the cache full), `cache_head_bytes_capped`, `declared_sizes_sum_overflow`.
- [ ] **Step 2: Run** `cargo test -p coverly-core --test archive_detect archive::cache` (fails).
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the tests, then G-RUST and the Windows script.
- [ ] **Step 5: Commit and push:** `git add crates/coverly-core && git commit -m "feat(core): open any archive by magic with multi-volume detection and a listing cache" && git push && git status -sb`

### Task 17: Core: garbled-archive mutation suite (S1 across every format)

**Files:**
- Modify: `crates/coverly-core/src/archive/testing.rs` (`mutate(seed, bytes, n)`: bit flips, truncations, length-field rewrites at known offsets)
- Create: `crates/coverly-core/tests/archive_garbled.rs`

- [ ] **Step 1: Write the test** `garbled_archives_never_panic`. For each fixture (zip deflate/lzma/ppmd/zstd, 7z LZMA/LZMA2/BCJ/PPMd, tar.gz/bz2/xz/zst, gz, lz4), it runs 2,000 seeded mutations through `open_archive` + full `stream_entries` with a 4 MiB budget. Each must return `Ok` or `Err`, and none may unwind out (the outer test fails on unwind because the call is **not** wrapped). The test also asserts the total runtime is ≤ 30 s in release-test mode, and logs per-format timing. A `#[ignore] garbled_archives_never_panic_100k` variant runs 100,000 mutations.
- [ ] **Step 2: Run** `cargo test -p coverly-core --features test-util --test archive_garbled`. Any crash or hang found is a bug: fix it in the backend (add a guard or a bound), add a regression test named after the seed in the format's test file, and re-run.
- [ ] **Step 3: Run** G-RUST and the Windows script.
- [ ] **Step 4: Commit and push:** `git add crates/coverly-core && git commit -m "test(core): mutate every archive format and assert no panic escapes" && git push && git status -sb`

### Task 18: Core: setting `organize.external_tools`

**Files:**
- Modify: `crates/coverly-core/src/settings/model.rs` (`OrganizeSettings.external_tools: ExternalTools { Auto, Off }`, serde default `Auto`), `validate.rs`, `patch.rs`
- Test: `crates/coverly-core/tests/settings_file.rs`

- [ ] **Step 1: Write the failing tests:** `external_tools_defaults_to_auto`, `external_tools_off_round_trips`, `old_settings_file_without_field_loads`.
- [ ] **Step 2: Run** `cargo test -p coverly-core --test settings_file` (fails).
- [ ] **Step 3: Implement.** `OrganizeOptions::from_settings` sets `Limits.external_tools`.
- [ ] **Step 4: Run** the tests, then G-RUST and the Windows script.
- [ ] **Step 5: Commit and push:** `git add crates/coverly-core && git commit -m "feat(core): add the organize.external_tools setting" && git push && git status -sb`

---

## Group B — Disc identification

### Task 19: Core: `disc::source` sector sources and sheet parsers

**Files:**
- Create: `crates/coverly-core/src/disc/mod.rs`, `disc/source.rs`, `disc/sheets.rs`
- Modify: `crates/coverly-core/src/scan/multifile.rs` (keep `parse_cue`/`parse_gdi`/`parse_m3u` `pub(crate)`; `disc::sheets` wraps them, adding track modes for cue and the track numbers and LBA for gdi)
- Modify: `crates/coverly-core/src/lib.rs` (`pub mod disc;`)
- Test: unit tests in `source.rs`, `sheets.rs`

**Interfaces:**
```rust
pub enum SectorLayout { Iso2048, Raw2352Mode1, Raw2352Mode2, Mode2_2336 }
pub trait SectorSource { fn layout(&self) -> SectorLayout; fn read_user(&mut self, lba: u32, out: &mut [u8; 2048]) -> io::Result<bool>;
                         fn raw_sector(&mut self, n: u32) -> io::Result<Option<Vec<u8>>>; }
pub struct WindowSource { bytes: Vec<u8> /* ≤ 4 MiB decoded head */ }  // from an archive entry
pub struct FileSource { file: File }                                  // raw file, bounded reads
pub fn detect_layout(first_sectors: &[u8]) -> SectorLayout;           // sync 00 FF×10 00, mode byte
pub struct Sheet { pub kind: SheetKind, pub files: Vec<String>, pub tracks: Vec<TrackInfo> }
pub fn parse_sheet(kind: SheetKind, text: &[u8]) -> Result<Sheet, SheetError>;  // ≤ 64 KiB, UTF-8 or Latin-1
pub fn primary_track(sheet: &Sheet) -> Option<usize>;                 // first MODE1/MODE2 of cue; track 03 of gdi
pub fn infer_redump_set(listing: &[&str], sheet_name: &str) -> Option<Vec<String>>; // "<stem> (Track NN).bin", trackNN.bin beside .gdi
```

- [ ] **Step 1: Write the failing tests:** `layout_2048_2352_2336_detected`, `window_source_out_of_range_is_none`, `cue_tracks_and_modes`, `gdi_tracks_and_track03_primary`, `m3u_lines`, `sheet_over_64k_rejected`, `infer_redump_cue_set_contiguous`, `infer_redump_gap_rejected`, `infer_gdi_trackNN`, `infer_extra_file_in_dir_rejected`.
- [ ] **Step 2: Run** `cargo test -p coverly-core disc::` (fails).
- [ ] **Step 3: Implement.** No recursion; all reads are bounded.
- [ ] **Step 4: Run** the tests and `cargo test -p coverly-core --test scan_fixtures` (still green), then G-RUST and the Windows script.
- [ ] **Step 5: Commit and push:** `git add crates/coverly-core && git commit -m "feat(core): read disc sectors from files and archive windows, parse sheets" && git push && git status -sb`

### Task 20: Core: ISO 9660, PlayStation 1/2 and serial normalisation

**Files:**
- Create: `crates/coverly-core/src/disc/iso9660.rs`, `disc/serial.rs`, `disc/testing.rs` (`cfg(any(test, feature = "test-util"))`)
- Test: `crates/coverly-core/tests/disc_identify.rs`

**Interfaces:**
```rust
pub struct Pvd { pub system_id: String, pub volume_id: String, pub space_size: u32, pub created: [u8;17], pub modified: [u8;17], pub root: DirRecord }
pub fn read_pvd(src: &mut dyn SectorSource) -> io::Result<Option<Pvd>>;               // LBA 16
pub fn find_root_file(src: &mut dyn SectorSource, pvd: &Pvd, name: &str, cap: usize) -> io::Result<Option<Vec<u8>>>; // ≤ 64 records, depth 1
pub fn normalise_serial(raw: &str) -> Option<String>;  // SLUS_008.62 → SLUS-00862; "T-5704H   " → T-5704H; T-40214N
```

- [ ] **Step 1: Write the failing tests:** `psx_boot_serial_raw_2352` (→ `psx`, `SLUS-00862`, confidence 0.95), `psx_boot_serial_iso_2048`, `psx_pvd_only_0_80`, `psx_licence_text_is_extra_evidence`, `ps2_boot2_unsupported_system`, `ps2_by_size_unsupported_system`, `serial_normalisation_table`, `root_directory_capped_at_64_records`, `fingerprint_equal_across_bin_and_iso`.
- [ ] **Step 2: Run** `cargo test -p coverly-core --features test-util --test disc_identify` (fails).
- [ ] **Step 3: Implement.** The `disc/testing.rs` builders reuse `identify::sniff` test builders where they exist.
- [ ] **Step 4: Run** the tests, then G-RUST and the Windows script.
- [ ] **Step 5: Commit and push:** `git add crates/coverly-core && git commit -m "feat(core): identify PlayStation discs from the PVD and SYSTEM.CNF" && git push && git status -sb`

### Task 21: Core: Sega IP.BIN, PC Engine CD, Neo Geo CD, 3DO, `DiscReader`

**Files:**
- Create: `crates/coverly-core/src/disc/sega.rs`, `disc/signatures.rs`
- Modify: `crates/coverly-core/src/disc/mod.rs` (`DiscReader::identify`, `DiscIdentity`, `VolumeFingerprint`)
- Modify: `crates/coverly-core/src/identify/sniff.rs` (the disc rows call `disc::` for serials; the head-magic checks stay)
- Test: `crates/coverly-core/tests/disc_identify.rs`, `crates/coverly-core/tests/sniff.rs` (existing ones green)

**Interfaces:**
```rust
pub struct DiscIdentity { pub system: DiscSystem, pub serials: Vec<String>, pub disc_no: Option<u8>,
                          pub volume: Option<VolumeFingerprint>, pub source: DiscIdSource, pub confidence: f32, pub detail: String }
pub enum DiscSystem { Known(&'static str) /* psx saturn dreamcast segacd pcenginecd neocd psp */, Unsupported(&'static str) /* "PlayStation 2", "3DO" */ }
impl DiscReader { pub fn identify(src: &mut dyn SectorSource) -> Option<DiscIdentity>; }
```
The rows come from spec §7. Saturn: `SEGA SEGASATURN ` at 0x00 (0x10 raw), product at 0x20. Dreamcast: `SEGA SEGAKATANA `, product at 0x40, disc `n/m` at 0x2B, track 03 only. Sega CD: `SEGADISCSYSTEM  ` with system type at 0x100 and serial at 0x180, never `megadrive`. PCE CD: `PC Engine CD-ROM SYSTEM` at 0x20 of sector 1 (0.90). Neo Geo CD: root `IPL.TXT` with no SYSTEM.CNF and no Sega/NEC signature (0.85). 3DO: Opera header `01 5A5A5A5A5A 01` → `Unsupported("3DO")`. The fingerprint's boot SHA1 is SYSTEM.CNF, or IP.BIN = the first 32 KiB of the primary track.

PlayStation boot file past the window (Task 20 review, real case): on `Next Tetris, The (USA)` `SYSTEM.CNF` is at LBA 2089 (≈ 4.9 MB, after a 4 MiB `PSX.BSE`), outside the 4 MiB head window. `identify_playstation` then returns `pending_boot: Some(FileExtent)` and a serial from the root boot executable's name (0.80). `DiscReader` (and the plan code of §6.2 step 3) must read `FileExtent::stored_range(layout, SYSTEM_CNF_CAP)` with a skip on a fresh `open_entry` charged to the same 64 MiB head budget, wrap it in `WindowSource::at`, `read_extent`, and call `identify_playstation_with` for the 0.95 verdict and the volume fingerprint. Budget spent → keep the 0.80 verdict.

- [ ] **Step 1: Write the failing tests:** `saturn_product_t5704h`, `dreamcast_track03_t40214n_disc_1_of_1`, `dreamcast_track01_alone_is_not_enough`, `segacd_never_megadrive`, `pce_cd_sector1`, `neocd_ipl_txt`, `three_do_unsupported`, `psp_iso_unchanged`, `identity_detail_strings_are_data`.
- [ ] **Step 2: Run** `cargo test -p coverly-core --features test-util --test disc_identify --test sniff` (fails).
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the tests, then G-RUST and the Windows script.
- [ ] **Step 5: Commit and push:** `git add crates/coverly-core && git commit -m "feat(core): identify Saturn, Dreamcast, Sega CD, PC Engine CD, Neo Geo CD and 3DO discs" && git push && git status -sb`

### Task 22: Core: seams for dedupe phase B (contract test)

**Files:**
- Modify: `crates/coverly-core/src/disc/mod.rs` (`impl DiscIdentity { pub fn to_dedupe_id(&self) -> crate::dedupe::source::DiscId }`; `VolumeFingerprint` **is** `dedupe::source::VolumeFingerprint`, re-exported, not duplicated)
- Create: `crates/coverly-core/tests/archive_dedupe_contract.rs`

- [ ] **Step 1: Write the failing test.** Inside the test file, a 30-line `TestArchiveListing: EntryListing` is written over `open_archive` + `file_entries` + `From<ArchiveError> for io::Error` (`BudgetSpent` → `Ok(None)`), plus a `TestDiscProbe: DiscProbe` over `DiscReader`. Tests: `listing_index_space_is_file_entries_in_archive_order`, `payload_sha1_verifies_crc`, `head_budget_spent_is_none`, `probe_returns_serial_and_fingerprint`, `memory_cap_maps_to_out_of_memory`. The tests prove spec §10 is implementable with no new types.
- [ ] **Step 2: Run** `cargo test -p coverly-core --features test-util --test archive_dedupe_contract` (fails until `to_dedupe_id` exists).
- [ ] **Step 3: Implement** `to_dedupe_id` and the re-export.
- [ ] **Step 4: Run** the tests, then G-RUST and the Windows script.
- [ ] **Step 5: Commit and push:** `git add crates/coverly-core && git commit -m "test(core): prove the archive and disc readers fit the dedupe seams" && git push && git status -sb`

---

## Group C — Planner: extract action

### Task 23: Core: plan types for extraction, new reasons and notes

**Files:**
- Modify: `crates/coverly-core/src/organize/plan.rs`
- Modify: `crates/coverly-core/src/organize/inspect.rs` (`Container` gains `Tar`, `Gzip`, `Bzip2`, `Xz`, `Zstd`, `Lz4`, `Lzma`, `MultiVolume`; serde kebab-case)
- Test: `crates/coverly-core/tests/organize_plan.rs` (serde back-compat)

**Interfaces (spec §4.2, §8.1, §11.2):**
```rust
pub enum Transfer { AsIs, Extract { entries: Vec<ExtractEntry>, bytes: u64, archive_after: ArchiveAfter,
                    source: ExtractSource, sheet_check: SheetCheck, m3u: Option<PathBuf> } }  // #[serde(default)] → AsIs
pub struct ExtractEntry { pub index: usize, pub name: String /* RelPath */, pub size: u64, pub crc32: Option<u32>, pub target: PathBuf }
pub enum ArchiveAfter { Keep, Trash }
pub enum ExtractSource { Builtin, Tool { tool: Tool, version: String } }
pub enum SheetCheck { Read, Deferred }
// OrganizeItem: + #[serde(default)] transfer: Transfer, + #[serde(default)] disc: Option<DiscId>
// LeftReason: UnsupportedArchive gains #[serde(default)] format: Option<String>, need: Option<Tool>;
//   new: EncryptedArchive, ArchiveNeedsMemory { mib }, ArchiveTooBig { entries, bytes }, MultiVolumeArchive { parts },
//   UnsafeArchiveEntry { name, why }, SetUnsafeName { name }, PathTooLong, NestedArchive, RepackArcade, FolderFull { folder }, UnsafeTarget
// ItemNote: new ContainerMismatch { ext, actual }, TwinSheets { kept }, NoStoredChecksum, ArchiveNotFullyRead,
//   ArchiveKeptLeftovers { count, examples }, SheetNotReadYet, MultiRomExtract { count }
```

- [ ] **Step 1: Write the failing tests:** `old_plan_json_without_transfer_parses_as_as_is` (a plan JSON fixture from `main`, committed under `tests/fixtures/organize/plan-v1.json`), `unsupported_archive_old_shape_parses`, `new_left_reasons_round_trip` (table), `new_notes_round_trip`.
- [ ] **Step 2: Run** `cargo test -p coverly-core --test organize_plan` (fails).
- [ ] **Step 3: Implement** the types only. Nothing produces them yet.
- [ ] **Step 4: Run** the tests and update any `insta` snapshot changed only by the new defaulted fields (`cargo insta review`; accept only additions). Then G-RUST and the Windows script.
- [ ] **Step 5: Commit and push:** `git add crates/coverly-core && git commit -m "feat(core): add extract transfers, disc ids and new archive reasons to the plan" && git push && git status -sb`

### Task 24: Core: `inspect.rs` delegates to the archive reader (magic first)

**Files:**
- Modify: `crates/coverly-core/src/organize/inspect.rs` (`container_for(ext, magic)` = magic first; zip code removed in favour of `archive::open_archive`; `ArchiveIssue` gains `Encrypted`, `NeedsMemory { mib }`, `TooBig { entries, bytes }`, `MultiVolume { parts }`, `Unsupported { format, need }`, `Nested`; `Inspection` gains `format: Option<Format>`, `mismatch: Option<(String, Format)>`, `listing_complete: bool`, `budget_spent: bool`)
- Modify: `crates/coverly-core/src/organize/plan.rs` (the `ArchiveIssue → LeftReason` mapping of spec §4.2; `BudgetSpent` → Review + `ArchiveNotFullyRead`)
- Test: `crates/coverly-core/src/organize/inspect.rs` unit tests, `crates/coverly-core/tests/organize_inspect.rs`

- [ ] **Step 1: Write the failing tests.** Replace `extension_decides_before_magic` with `magic_decides_before_extension` (7z named `.zip` → `SevenZip`; zip named `.bin` → `Zip`; `.zip` without magic → `Zip` fallback). In `organize_inspect.rs`:
  - `sevenz_named_zip_is_not_damaged_and_notes_mismatch`
  - `html_zip_is_not_a_rom`
  - `cartridge_in_7z_identified_by_header`
  - `cartridge_in_tar_gz_crc_from_bounded_decode`
  - `game_gba_gz_single_entry`
  - `rar_without_tool_unsupported_need_unrar`
  - `encrypted_7z_header_left_encrypted`
  - `zip_20000_entries_archive_too_big_not_damaged`
  - `nested_archive_only_payload_left_nested`
  - `budget_spent_goes_to_review_not_left`
  - `existing_zip_cases_unchanged` (all existing tests of the file)
- [ ] **Step 2: Run** `cargo test -p coverly-core --test organize_inspect inspect::` (fails).
- [ ] **Step 3: Implement.** Cartridge rules (spec §6.1) are unchanged but run over `file_entries()`. The header level uses `open_entry(i).take(STREAM_CAP)` → `sniff`.
- [ ] **Step 4: Run** `cargo test -p coverly-core` (the whole crate stays green; review any snapshot diffs and accept only the intended changes). Then G-RUST and the Windows script.
- [ ] **Step 5: Commit and push:** `git add crates/coverly-core && git commit -m "fix(core): detect organise containers by magic and open every archive format" && git push && git status -sb`

### Task 25: Core: disc archives in `inspect` (sheets, deferred sets, primary track)

**Files:**
- Modify: `crates/coverly-core/src/organize/inspect.rs` (`Inspection.disc_set: Option<DiscSetInfo { sheets, members, primary, sheet_check, identity, multi_disc, twins }>`)
- Modify: `crates/coverly-core/src/organize/evidence.rs` (disc header evidence level `Header` with the spec §7 confidence + sheet extension + name signals)
- Test: `crates/coverly-core/tests/organize_inspect.rs`

**Behaviour (spec §6.2):**
- Disc archive: a file entry with `.cue/.gdi/.m3u/.ccd/.iso/.img/.chd/.cdi/.pbp/.cso`.
- Sheets of ≤ 64 KiB are parsed when they are reachable within the head budget. References resolve inside the archive namespace (relative to the sheet's dir, case-insensitive + NFC fallback). An absolute path or `..` → `SetOutsideFolder`; a missing reference → `SetIncomplete`; a shared track → `SharedTrack`. A `.cue` + `.gdi` over the same tracks is one disc (`twins`).
- An unreachable sheet with the Redump pattern → `SheetCheck::Deferred` + `SheetNotReadYet`. Without the pattern → Review with name evidence.
- The primary track goes through `open_entry` into a `WindowSource` of ≤ 4 MiB within 64 MiB, then `DiscReader::identify`.
- Multi-disc of one game → one item; discs of several games → `MultiGameArchive`.

- [ ] **Step 1: Write the failing tests:**
  - `ps1_7z_named_zip_cue_two_tracks_identified` (→ `psx`, `SLUS-00862`, `ContainerMismatch`)
  - `saturn_solid_sheet_last_deferred_identified` (sheet after 600 MB declared / 8 MiB real with a small dictionary; plan budget spent ≤ 64 MiB, asserted via `Budget::spent`)
  - `dreamcast_cue_and_gdi_twins_one_disc`
  - `non_solid_sheet_read_at_plan_time`
  - `deferred_without_redump_pattern_is_review`
  - `sheet_dotdot_reference_outside_folder`
  - `missing_track_set_incomplete`
  - `two_disc_one_game_one_item`
  - `two_games_multi_game_archive`
  - `primary_track_beyond_budget_name_only_review`
- [ ] **Step 2: Run** `cargo test -p coverly-core --features test-util --test organize_inspect` (fails).
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the tests, then G-RUST and the Windows script.
- [ ] **Step 5: Commit and push:** `git add crates/coverly-core && git commit -m "feat(core): identify disc games inside archives from their sheets and headers" && git push && git status -sb`

### Task 26: Research + Core: profile `disc_layout`, `accepts_containers`, overrides

**Files:**
- Modify: `docs/02-research/layouts.md` (R1: the extension lists of Onion/GarlicOS/Anbernic stock for SATURN, DREAMCAST, MDCD, PCECD, NEOCD and PS; MinUI 7z support; ES-DE `.bin` listing; muOS; Batocera/RetroArch m3u, each with a source URL and the date checked)
- Modify: `crates/coverly-core/src/profiles/mod.rs` (`Profile.disc_layout: DiscLayout { Flat { m3u_for_multi: bool }, GameFolder { m3u_for_multi: bool } }`, `accepts_containers: Vec<ContainerKind>`, `container_overrides: BTreeMap<String, Vec<ContainerKind>>`, all `#[serde(default)]`), `builtin.rs` (the spec §8.2/§8.3 values, corrected by R1), `custom.rs` (TOML keys `disc_layout`, `accepts_containers`, `container_overrides`), `template.rs`
- Test: `crates/coverly-core/tests/custom_profiles.rs`, `crates/coverly-core/tests/profile_fit.rs`

- [ ] **Step 1: Research.** Use WebSearch/WebFetch against the official docs and repos for each profile. Record findings in `docs/02-research/layouts.md` under a new "Disc layouts and containers (R1, 2026-10)" section. If a source contradicts the spec table, change the default in this task and note it in the section.
- [ ] **Step 2: Write the failing tests:** `builtin_disc_layouts_match_research` (table: anbernic-stock/garlicos/onion/muos Flat without m3u; minui and esde GameFolder with m3u; batocera/retroarch Flat with m3u), `builtin_accepts_containers` (minui does not accept 7z), `disc_systems_never_accept_archives_as_is`, `custom_profile_toml_disc_layout_round_trip`, `custom_profile_without_fields_defaults_flat`.
- [ ] **Step 3: Run** `cargo test -p coverly-core --test custom_profiles --test profile_fit` (fails).
- [ ] **Step 4: Implement.**
- [ ] **Step 5: Run** the tests, then G-RUST and the Windows script.
- [ ] **Step 6: Commit and push** (two commits, each pushed):
  - `git add docs/02-research/layouts.md && git commit -m "docs(research): record disc layouts and container support per device" && git push`
  - `git add crates/coverly-core && git commit -m "feat(core): give profiles a disc layout and accepted containers" && git push && git status -sb`

### Task 27: Core: planner chooses `Transfer::Extract` and assigns extract targets

**Files:**
- Modify: `crates/coverly-core/src/organize/target.rs` (`assign_extract_targets(item, layout, dir_index, reservations)`; `name_key` reservations across the plan; generated `.m3u` path; length rules: components ≤ 255 units/bytes, the 259-unit check on the longer of the final and temp paths, depth ≤ 16)
- Modify: `crates/coverly-core/src/organize/plan.rs` (decision: disc archive → Extract; cartridge in an unaccepted container → Extract of the ROM entries; arcade sets never extract → `RepackArcade` when the container is not zip; `ArchiveAfter::Trash` only when every file entry is extracted or junk, else `Keep` + `ArchiveKeptLeftovers`; copy mode → `Keep`; `TwinSheets` note in Flat; an `UnsafeEntry` needed by the set → `UnsafeArchiveEntry`; a member name that `safe_file_name` would change → `SetUnsafeName`; Windows length → `PathTooLong`; collisions → `CollisionSet`)
- Test: `crates/coverly-core/tests/organize_plan.rs`, `crates/coverly-core/tests/organize_target.rs`

- [ ] **Step 1: Write the failing tests:**
  - `ps1_archive_extracts_flat_on_anbernic` (snapshot)
  - `ps1_archive_game_folder_on_minui` (snapshot)
  - `multi_disc_game_folder_with_m3u_on_esde` (snapshot)
  - `flat_with_m3u_on_batocera` (snapshot)
  - `dreamcast_twins_both_extracted_note_in_flat`
  - `cartridge_7z_as_is_on_anbernic_extract_on_minui`
  - `game_gba_gz_extracts_everywhere`
  - `arcade_set_in_rar_repack_arcade`
  - `sav_beside_rom_keeps_archive_with_leftovers`
  - `junk_only_leftovers_trash_archive`
  - `copy_mode_keeps_archive`
  - `two_archives_same_track_name_flat_second_collision_set`
  - `case_twins_inside_one_archive_collision_set`
  - `existing_target_file_collision_set`
  - `long_temp_name_counts` (→ `PathTooLong`)
  - `long_target_left_path_too_long`
  - `member_unsafe_name_set_unsafe_name`
  - `top_folder_dropped_in_flat_replaced_in_game_folder`
  - `extract_entries_carry_listing_crc_and_size`
- [ ] **Step 2: Run** `cargo test -p coverly-core --features test-util --test organize_plan --test organize_target` (fails).
- [ ] **Step 3: Implement.** Snapshots normalise paths to `/` (the existing helper).
- [ ] **Step 4: Run** the tests, review the new snapshots with `cargo insta review`, then G-RUST and the Windows script.
- [ ] **Step 5: Commit and push:** `git add crates/coverly-core && git commit -m "feat(core): plan extraction of disc archives and unloadable cartridge containers" && git push && git status -sb`

### Task 28: Core: plan-time checks (space, FAT32, memory, folder slots, multi-volume, listing completeness)

**Files:**
- Modify: `crates/coverly-core/src/organize/plan.rs`, `organize/target.rs` (`fat_dir_slots(dir) + planned ≤ 65,000` → `FolderFull`), `organize/apply.rs` (`free_space` gains `cluster_size(path)` and `user_available(path)`: `statvfs.f_bavail × f_frsize`, `GetDiskFreeSpaceExW` caller quota)
- Modify: `crates/coverly-core/src/organize/walk.rs` (multi-volume parts collected into one Left item; later parts never become their own items)
- Test: `crates/coverly-core/tests/organize_plan.rs`

- [ ] **Step 1: Write the failing tests:**
  - `free_space_counts_cluster_rounding_and_reserve` (the override with a 32 KiB cluster)
  - `move_same_volume_does_not_credit_archive`
  - `no_space_warning_has_exact_numbers`
  - `fat32_track_4gib_declared_too_big_for_fat`
  - `fat32_folder_slots_folder_full`
  - `archive_needs_memory_left_with_mib`
  - `multi_volume_7z_001_002_one_item_002_never_moved`
  - `truncated_tar_listing_never_extract` (Review item → approving yields `AsIs` only)
  - `plan_head_budget_asserted_64mib`
- [ ] **Step 2: Run** `cargo test -p coverly-core --features test-util --test organize_plan` (fails).
- [ ] **Step 3: Implement.** The `cluster_size`/`user_available` OS calls sit under `cfg(unix)`/`cfg(windows)` with a test override (the existing `set_free_space_override` pattern).
- [ ] **Step 4: Run** the tests, then G-RUST and the Windows script.
- [ ] **Step 5: Commit and push:** `git add crates/coverly-core && git commit -m "feat(core): check space, FAT32 limits and memory before planning an extraction" && git push && git status -sb`

---

## Group D — Apply, undo, resume

### Task 29: Core: journal ops for extraction

**Files:**
- Modify: `crates/coverly-core/src/write/mod.rs` (`Op::Extracted { from, entry, size, crc32, temp }` intent/completion like `Moved`; `Op::RolledBack { temp }`; `Op::ArchiveTrash { from }`; `Op::ArchiveTrashed { from, trash: Option<PathBuf> }`; `Op::ArchiveKept { from }`; `Writer` methods `extract_intent`, `extract_done`, `rolled_back`, `archive_trash_intent`, `archive_trashed`, `archive_kept`; `TransferRecord` learns `Extracted`; prune keeps jobs with an unresolved `ArchiveTrash`)
- Test: `crates/coverly-core/tests/writer.rs`, `crates/coverly-core/tests/journal_resume.rs`

- [ ] **Step 1: Write the failing tests:** `extracted_intent_then_completion_round_trip`, `journal_is_append_only_later_line_supersedes`, `older_build_sees_unknown_and_refuses_undo` (a JSON line with an unknown op kind → not undoable), `journal_from_before_change_still_undoes`, `job_transfers_reports_extracted`.
- [ ] **Step 2: Run** `cargo test -p coverly-core --test writer --test journal_resume` (fails).
- [ ] **Step 3: Implement.** Every write is `sync_data` before the method returns.
- [ ] **Step 4: Run** the tests, then G-RUST and the Windows script.
- [ ] **Step 5: Commit and push:** `git add crates/coverly-core && git commit -m "feat(core): journal extracted files and archive trash steps" && git push && git status -sb`

### Task 30: Core: Trash backend (`write/trash.rs`)

**Files:**
- Create: `crates/coverly-core/src/write/trash.rs`
- Test: unit tests + `#[ignore]` smoke tests per OS

**Interfaces:**
```rust
pub trait TrashBackend: Send + Sync {
    fn trash(&self, path: &Path) -> Result<Option<PathBuf>, TrashError>;   // Ok(None) = trashed, location unknown
    fn find(&self, volume_of: &Path, name: &str, fp: &Fingerprint) -> Option<PathBuf>;
    fn available(&self, path: &Path) -> bool;
}
pub struct OsTrash;      // macOS NSFileManager trashItemAtURL:resultingItemURL: (objc2-foundation); Windows IFileOperation
                         // FOFX_RECYCLEONDELETE; Linux freedesktop ($XDG_DATA_HOME/Trash or $topdir/.Trash-$uid, files + info)
#[cfg(any(test, feature = "test-util"))] pub struct FakeTrash { dir: PathBuf, unavailable: bool, fail_after: Option<usize> }
```

- [ ] **Step 1: Write the failing tests:** `fake_trash_moves_and_reports_location`, `fake_trash_unavailable`, `fake_trash_find_by_name_and_fp`, `linux_trash_writes_info_file` (`#[cfg(target_os = "linux")]`), `os_trash_smoke` (`#[ignore]`, one per OS under `cfg`; it trashes a tempfile and asserts the returned location exists).
- [ ] **Step 2: Run** `cargo test -p coverly-core write::trash` (fails).
- [ ] **Step 3: Implement.** Each OS backend is in its own `#[cfg(target_os = …)] mod`, with no shared unused helpers.
- [ ] **Step 4: Run** the tests, `cargo test -p coverly-core write::trash -- --ignored os_trash_smoke` (macOS, local), then G-RUST and the Windows script.
- [ ] **Step 5: Commit and push:** `git add crates/coverly-core Cargo.lock && git commit -m "feat(core): send files to the OS Trash and remember where they went" && git push && git status -sb`

### Task 31: Core: extraction engine, phase A (temps, verify, disk full, cancel, target symlinks)

**Files:**
- Create: `crates/coverly-core/src/organize/extract.rs`
- Modify: `crates/coverly-core/src/progress.rs` (`PauseKind::DiskFull { needed, free }`)
- Test: `crates/coverly-core/tests/organize_extract_apply.rs`

**Behaviour (spec §9.1 phase A):**
- Target dirs: every existing component below the root is `symlink_metadata`-checked and must be a real dir, not a symlink, junction or reparse point (else `UnsafeTarget`). Missing ones are made with `create_dir` one level at a time, each journaled `DirCreated`.
- Leftover `.coverly-<job>-*.tmp` files are cleaned first.
- `stream_entries(wanted)` runs once.
- Per entry: the intent line is written and synced. The temp is opened with `create_new` (+ `O_NOFOLLOW` on Unix). The data is copied through `VerifyingReader` with a 1 MiB buffer, then `sync_all`.
- Errors: `StorageFull`/`ENOSPC`/`EDQUOT`/`ERROR_DISK_FULL`/`ERROR_HANDLE_DISK_FULL` → `DiskFull` rollback + pause; other errors fail the item.
- After `sync_all` the temp is re-read (with `F_NOCACHE` on macOS, `posix_fadvise(DONTNEED)` on Linux) and its CRC compared.
- `set_modified` from the entry mtime; errors are ignored.
- Sheets of ≤ 64 KiB are kept in memory.
- Cancel is checked between entries and every 16 MiB.
- A test hook `set_write_fault(Some(Fault::StorageFullAfter(n)))` follows the existing `set_copy_fault` pattern.

- [ ] **Step 1: Write the failing tests:** `phase_a_writes_only_temps` (asserted by a hook after phase A), `crc_mismatch_on_reread_rolls_back`, `disk_full_mid_entry_rolls_back_and_pauses`, `disk_full_journal_volume_stops_before_write`, `cancel_inside_64mib_entry_returns_within_16mib`, `target_subfolder_symlink_refused` (`#[cfg(unix)] mod unix_tests`), `target_junction_refused` (`#[cfg(windows)] mod windows_tests`), `planted_symlink_at_temp_name_refused` (`#[cfg(unix)]`), `set_modified_failure_ignored`.
- [ ] **Step 2: Run** `cargo test -p coverly-core --features test-util --test organize_extract_apply` (fails).
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the tests, then G-RUST and the Windows script.
- [ ] **Step 5: Commit and push:** `git add crates/coverly-core && git commit -m "feat(core): extract archive entries to verified temps with disk-full rollback" && git push && git status -sb`

### Task 32: Core: apply phases B–D, item status, progress

**Files:**
- Modify: `crates/coverly-core/src/organize/extract.rs` (phase B validation, phase C `rename_noclobber_indexed` + completion lines + `.m3u` via temp → rename journaled `Created`, phase D archive)
- Modify: `crates/coverly-core/src/organize/apply.rs` (`Transfer::Extract` dispatch; revalidation of archive fp and tool version at item start; the per-item free-space re-check; `ItemStatus::{Extracted, ExtractedKept}`; `OrganizeReport.extracted`; `SkipReason::ToolGone`; `ApplyEnv.trash: &dyn TrashBackend`; progress `OrganizePhase::Extract { bytes_done, bytes_total }`)
- Test: `crates/coverly-core/tests/organize_extract_apply.rs`

- [ ] **Step 1: Write the failing tests:**
  - `extract_verify_rename_end_to_end` (PS1 synthetic 7z)
  - `no_final_file_before_phase_c` (hook)
  - `deferred_sheet_confirmed_at_phase_b`
  - `deferred_sheet_missing_track_rolls_back`
  - `tar_gz_bad_crc_at_eof_rolls_back`
  - `tool_exit_3_at_end_rolls_back` (fake tool, `tool_nonzero_exit_rolls_back_item`)
  - `collision_appears_between_b_and_c_rolls_back`
  - `archive_replaced_after_plan_is_changed_since_plan`
  - `archive_replaced_during_extraction_rolls_back`
  - `move_mode_trashes_archive_after_commit`
  - `trash_unavailable_extracted_kept`
  - `copy_mode_never_touches_archive`
  - `leftover_sav_archive_kept`
  - `m3u_created_journaled`
  - `progress_is_byte_based_over_folder_unpack_size`
  - `tool_gone_skips_item`
- [ ] **Step 2: Run** `cargo test -p coverly-core --features test-util --test organize_extract_apply` (fails).
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the tests, the existing `organize_apply` tests, then G-RUST and the Windows script.
- [ ] **Step 5: Commit and push:** `git add crates/coverly-core && git commit -m "feat(core): commit extracted games only after whole-item validation" && git push && git status -sb`

### Task 33: Core: undo of extract items

**Files:**
- Modify: `crates/coverly-core/src/write/undo_organize.rs`, `crates/coverly-core/src/write/mod.rs` (`UndoReport.archive_lost: Vec<PathBuf>`)
- Test: `crates/coverly-core/tests/organize_extract_undo.rs`

**Behaviour (spec §9.2):** Per item in reverse order:
1. Resolve the archive by its last line. `archive-trashed` → restore it with `rename_noclobber` (cross-volume: stream + verify). An intent without a follow-up → `find` in the Trash. If it cannot be restored, stop undoing this item and report `archive_lost`.
2. `extracted` with a completion line: delete when the fp matches (2 s FAT tolerance, 100 ns Windows), else `skipped_modified`. An intent only: delete only when the size + full CRC re-read match; delete the job temp.
3. `.m3u` as `created`; `dir-created` only when empty.
Undo is idempotent.

- [ ] **Step 1: Write the failing tests:** `undo_restores_archive_from_fake_trash_and_removes_files`, `undo_trash_emptied_keeps_extracted_files_and_reports`, `undo_copy_mode_only_deletes_extracted`, `undo_modified_extracted_file_skipped`, `undo_intent_without_completion_deletes_only_after_crc`, `interrupted_undo_run_twice_same_result`, `undo_removes_empty_created_dirs_only`.
- [ ] **Step 2: Run** `cargo test -p coverly-core --features test-util --test organize_extract_undo` (fails).
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the tests and `cargo test -p coverly-core --test organize_undo`, then G-RUST and the Windows script.
- [ ] **Step 5: Commit and push:** `git add crates/coverly-core && git commit -m "feat(core): undo extractions and restore trashed archives" && git push && git status -sb`

### Task 34: Core: resume of extract items

**Files:**
- Modify: `crates/coverly-core/src/organize/apply.rs` (`resumable_items` / `resume_organize`), `crates/coverly-core/src/organize/extract.rs` (adopt-by-CRC; `io::sink` decode for committed entries)
- Test: `crates/coverly-core/tests/organize_extract_resume.rs`

- [ ] **Step 1: Write the failing tests:** `crash_in_phase_a_restarts_item_and_deletes_temps`, `crash_after_5_of_12_resume_finishes_no_double_files`, `crash_between_rename_and_completion_adopted_by_crc`, `crash_before_trash_call_resume_trashes`, `crash_during_trash_call_found_in_trash`, `archive_changed_since_plan_rolls_back_committed`, `item_done_only_when_archive_line_final`. Crashes are injected with the `set_write_fault(Fault::AbortAfterLine(n))` test hook, which makes the extractor return early as if killed.
- [ ] **Step 2: Run** `cargo test -p coverly-core --features test-util --test organize_extract_resume` (fails).
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the tests, then G-RUST and the Windows script.
- [ ] **Step 5: Commit and push:** `git add crates/coverly-core && git commit -m "feat(core): resume interrupted extractions without rewriting finished files" && git push && git status -sb`

### Task 35: Core: real-case verification (plan read-only; apply/undo on a copy)

**Files:**
- Modify: `crates/coverly-core/tests/organize_real_sample.rs` (new `#[ignore] downloads_disc_archives_plan` gated by `COVERLY_SAMPLE_DIR`, default `/Volumes/TinyExt/ROMs-Downloaded`; skips with a message when it is missing)

- [ ] **Step 1: Write the test.** It asserts spec §1 on a **plan-only** run:
  - `Next Tetris, The.zip` → `psx`, `SLUS-00862`, Extract 12 files / 405,749,493 B, `ContainerMismatch { ext: "zip", actual: 7z }`
  - `Next Tetris, The_ On-Line Edition.zip` → `dreamcast`, `T-40214N`, 16 files / 1,191,067,563 B, `SheetNotReadYet`, `TwinSheets` (Anbernic flat)
  - `Tetris Plus.zip` → `saturn`, `T-5704H`, 52 files / 591,881,790 B, `SheetNotReadYet`
  - `3-D Tetris (USA).zip` → `NotARom`
  - cartridge zips → the same system and action as on `main` (compare with a snapshot taken on `main` first)
  - the plan decodes ≤ 64 MiB per archive
- [ ] **Step 2: Run** `cargo test -p coverly-core --features test-util --release --test organize_real_sample -- --ignored downloads_disc_archives_plan`. Before running, record `ls -l /Volumes/TinyExt/ROMs-Downloaded` and `stat -f '%m %z'` of each file. After the run, compare: nothing changed. Fix anything red at its owning task's module (with a new regression test there).
- [ ] **Step 3: Apply/undo on a copy:**
  - `mkdir -p target/disc-sample/src target/disc-sample/card/ROMS && cp -X "/Volumes/TinyExt/ROMs-Downloaded/Next Tetris, The.zip" "/Volumes/TinyExt/ROMs-Downloaded/Tetris Plus.zip" target/disc-sample/src/`
  - `cargo run -p coverly-cli -- organize target/disc-sample/src --target target/disc-sample/card --layout anbernic-stock --move --apply --data-dir target/disc-sample/data --cache-dir target/disc-sample/cache` (move mode sends the copied archives to the macOS Trash, so the restore path is exercised for real)
  - Check: `ls target/disc-sample/card/ROMS/PS target/disc-sample/card/ROMS/SATURN`, and `7zz t` of the Trash location from the journal.
  - `cargo run -p coverly-cli -- undo --data-dir target/disc-sample/data`. Check that the archives are back in `src/` and identical (`shasum` before and after), and that the card folders are empty.
  - `rm -rf target/disc-sample`. Paste the outputs into the commit body evidence and into `docs/TRACKER.md`.
- [ ] **Step 4: Commit and push:** `git add crates/coverly-core/tests docs/TRACKER.md && git commit -m "test(core): verify the real download pile plans its disc archives" && git push && git status -sb`

---

## Group E — CLI

### Task 36: CLI: `coverly organize` extract output, `--no-external-tools`, JSON, undo lines

**Files:**
- Modify: `crates/coverly-cli/src/organize.rs` (action column `extract 12 files 405.7 MB → ROMS/PS/`; `--no-external-tools`; `--report json` includes `transfer` and `disc`; summary `Extracting: N games (bytes)`; result `Extracted: N`; `ExtractedKept` line; new Left reasons rendered through `table::terminal_safe`), `crates/coverly-cli/src/undo.rs` (archive restored / archive lost lines), `crates/coverly-cli/src/main.rs` (install the panic hook)
- Test: `crates/coverly-cli/tests/cli_organize.rs`

- [ ] **Step 1: Write the failing tests:** `dry_run_shows_extract_with_sizes` (synthetic PS1 7z named `.zip`), `copy_flag_keeps_archive`, `no_external_tools_rar_left_with_hint`, `report_json_has_transfer_and_disc`, `apply_then_undo_restores_archive` (with the fake Trash enabled through the `COVERLY_TEST_FAKE_TRASH=<dir>` env, honoured only under `test-util`), `hostile_entry_name_is_terminal_safe`.
- [ ] **Step 2: Run** `cargo test -p coverly-cli --test cli_organize` (fails).
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the tests, then G-RUST and the Windows script.
- [ ] **Step 5: Commit and push:** `git add crates/coverly-cli crates/coverly-core && git commit -m "feat(cli): show and apply archive extraction in coverly organize" && git push && git status -sb`

---

## Group F — Tauri and IPC

### Task 37: Tauri: views, progress, settings and found tools

**Files:**
- Modify: `src-tauri/src/organize/view.rs` (`OrganizeItemView.transfer`, `.disc`; `OrganizeProgressEvent.phase` adds `extract` with `bytes_done`/`bytes_total` as optional fields; `OrganizeReportView.extracted`; `OrganizeUndoView.archive_lost`), `src-tauri/src/organize/mod.rs` (passes `OsTrash`, the cancel flag inside entries), `src-tauri/src/main.rs` or `lib.rs` (panic hook once), the settings view (`grep -n "struct .*SettingsView" src-tauri/src` → add `external_tools` and `external_tools_found: Vec<{ tool, version, path }>` computed by `archive::external::discover`)
- Test: `src-tauri/tests/organize_state.rs`, `src-tauri/tests/contract.rs`

- [ ] **Step 1: Write the failing tests:** `plan_view_carries_extract_transfer_and_disc`, `extract_progress_events_have_bytes`, `settings_view_lists_found_tools` (fake tool dir via a test override), `contract_sample_matches_views` (the existing contract test with updated fixtures).
- [ ] **Step 2: Run** `cargo test -p coverly-roms --test organize_state --test contract` (it fails).
- [ ] **Step 3: Implement** with no new commands. Add the new error codes only if a new failure reaches the webview (`organize.toolGone` is a skip, not an error).
- [ ] **Step 4: Run** the tests, G-RUST, the Windows script and G-WEB (contract fixtures).
- [ ] **Step 5: Commit and push:** `git add src-tauri src/ipc/contract.requests.json src/ipc/contract.sample.json && git commit -m "feat(app): expose extract transfers, disc ids and found tools to the webview" && git push && git status -sb`

### Task 38: Frontend IPC: wire types and Tauri adapter

**Files:**
- Modify: `src/ipc/types.ts` (`Transfer`, `ExtractEntry`, `ArchiveAfter`, `DiscId`, the new `LeftReason`/`ItemNote` variants, `OrganizeItem.transfer`/`disc`, `OrganizeReport.extracted`, the progress `phase: 'extract'` with `bytes_done`/`bytes_total`, settings `external_tools`/`external_tools_found`), `src/ipc/tauriBackend.ts`
- Test: `src/ipc/contract.test.ts`, `src/ipc/tauriBackend.test.ts`

- [ ] **Step 1: Write the failing tests:** contract round-trip of the new sample fields; `old_plan_without_transfer_defaults_as_is` in the adapter's normaliser.
- [ ] **Step 2: Run** `pnpm test src/ipc` (fails).
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the tests, then G-WEB.
- [ ] **Step 5: Commit and push:** `git add src/ipc && git commit -m "feat(ui): mirror extraction and disc fields in the IPC types" && git push && git status -sb`

---

## Group G — Mock backend

### Task 39: Mock: downloads fixture with the real-case shapes

**Files:**
- Modify: `src/ipc/mock/mockOrganize.ts`, `src/ipc/mock/fixture.ts` (if the downloads fixture lives there)
- Test: `src/ipc/mock/mockOrganize.test.ts`

- [ ] **Step 1: Write the failing tests:**
  - `downloads_has_three_disc_archives_with_real_numbers` (names, sizes, serials and file counts of spec §1; `ContainerMismatch` notes; `SheetNotReadYet` on the two solid ones; `TwinSheets` on Dreamcast)
  - `rar_without_tool_left_with_need`
  - `tar_gz_cartridge_extracts`
  - `extract_progress_is_byte_based`
  - `apply_reports_extracted_and_one_extracted_kept`
  - `undo_reports_archive_restored`
- [ ] **Step 2: Run** `pnpm test src/ipc/mock/mockOrganize.test.ts` (fails).
- [ ] **Step 3: Implement** a deterministic fixture.
- [ ] **Step 4: Run** the tests, then G-WEB.
- [ ] **Step 5: Commit and push:** `git add src/ipc/mock && git commit -m "feat(ui): add disc archives and extraction to the organise mock" && git push && git status -sb`

---

## Group H — UI (frontend-design)

### Task 40: UI: i18n keys and `lib/organize.ts` helpers

**Files:**
- Modify: `locales/es/ui.json`, `locales/en/ui.json` (every key of spec §11.2, inserted in alphabetical position inside the `organize`/`settings.organize` blocks, never at the end of the file)
- Modify: `src/lib/organize.ts` (`actionOf(item) → 'move'|'copy'|'extract'`, `extractDetail(item) → { count, bytes }`, `extractSummary(plan)`, `leftReasonKey(reason)` for the new reasons, `noteKey(note)`, `layoutNote(item, profile)`)
- Test: `src/lib/organize.test.ts`, `src/i18n/parity.test.ts`

- [ ] **Step 1: Write the failing tests:** `action_of_extract`, `extract_detail_counts_and_bytes`, `extract_summary_counts_games`, `every_new_left_reason_has_a_key`, `every_new_note_has_a_key`, and parity (es/en keys and placeholders equal).
- [ ] **Step 2: Run** `pnpm test src/lib/organize.test.ts src/i18n` (fails).
- [ ] **Step 3: Implement.** The Spanish copy matches spec §11.2 word for word.
- [ ] **Step 4: Run** the tests, then G-WEB.
- [ ] **Step 5: Commit and push:** `git add locales src/lib && git commit -m "feat(ui): add extraction copy and helpers for organise" && git push && git status -sb`

### Task 41: UI: `SystemGroups` action badge, serial and `ExtractDetail`

**Files:**
- Modify: `src/components/organize/SystemGroups.tsx` (an action `Badge` per row: *Mover* / *Copiar* / **Extraer** as a petrol outline with the lucide `Archive` icon, followed by `12 archivos · 405,7 MB` in mono; the serial in mono under the name for disc rows; `SystemIcon` unchanged)
- Create: `src/components/organize/ExtractDetail.tsx` (the expanded row: file list with right-aligned mono sizes, the target folder, the layout note, notes)
- Test: `src/components/organize/SystemGroups.test.tsx`, `src/components/organize/ExtractDetail.test.tsx`

- [ ] **Step 1: Write the failing tests:** `extract_badge_has_icon_and_word`, `extract_detail_in_mono_with_exact_count`, `disc_row_shows_serial`, `expand_lists_files_and_target`, `notes_render_container_mismatch_and_twin_sheets`, `badge_never_colour_only` (the accessible name contains the word).
- [ ] **Step 2: Run** `pnpm test src/components/organize` (fails).
- [ ] **Step 3: Implement** with `Badge`, `Collapsible` and the existing styles. Run the frontend-design critique: start `pnpm dev --port 1455 --strictPort` and use Chrome DevTools MCP (`new_page` with `isolatedContext: "disc-archives"`) to screenshot the Organise screen with the downloads fixture in light and dark at 1280 px and 390 px. Fix spacing, alignment and contrast. Stop the server with `lsof -ti tcp:1455 | xargs kill`.
- [ ] **Step 4: Run** the tests, then G-WEB.
- [ ] **Step 5: Commit and push:** `git add src/components/organize && git commit -m "feat(ui): show the extract action, sizes and serials in organise rows" && git push && git status -sb`

### Task 42: UI: left reasons, summary bar, progress and result

**Files:**
- Modify: `src/components/organize/LeftInPlace.tsx` (new reasons; `UnsupportedArchive` with `need` shows *Para abrir archivos {format} instala {tool}* and a "Volver a analizar" button wired to the existing re-plan, ⌘R), `src/components/organize/OptionsBar.tsx` or the sticky summary (*Se extraen: {count} juegos ({bytes})*), `src/components/organize/OrganizeResult.tsx` (*Extraídos: {count}*, per-item *Extraído; el comprimido se queda…*, undo `archiveLost` message), the progress line in `src/screens/OrganizeScreen.tsx` (*Extrayendo {name}: {done} de {total} archivos…* with `<Progress slots={16}>`, byte-based)
- Test: `LeftInPlace.test.tsx`, `OrganizeResult.test.tsx`, `src/screens/OrganizeScreen.test.tsx`

- [ ] **Step 1: Write the failing tests:** `tool_hint_and_rescan_button`, `every_new_left_reason_renders`, `summary_shows_extract_count_and_bytes`, `progress_shows_extracting_name_and_counts`, `result_counts_extracted_and_kept_note`, `undo_archive_lost_message`.
- [ ] **Step 2: Run** `pnpm test src/components/organize src/screens/OrganizeScreen.test.tsx` (fails).
- [ ] **Step 3: Implement**, then repeat the screenshot critique as in Task 41 (left list, progress, result).
- [ ] **Step 4: Run** the tests, then G-WEB.
- [ ] **Step 5: Commit and push:** `git add src && git commit -m "feat(ui): explain archive reasons and report extraction progress and results" && git push && git status -sb`

### Task 43: UI: Settings → Organizar "Programas externos para RAR"

**Files:**
- Modify: `src/components/settings/LibrarySection.tsx` (the organise block: `Switch` or `RadioGroup` auto/off; *Encontrado: {tools}* in mono, or *Ninguno. Instala unrar o 7-Zip para abrir RAR.*), `src/stores/settingsStore.ts`, `src/ipc/mock/mockSettings.ts`
- Test: `src/components/settings/Sections.test.tsx`, `src/ipc/mock/mockSettings.test.ts`

- [ ] **Step 1: Write the failing tests:** `external_tools_toggle_saves_off`, `found_tools_listed_in_mono`, `no_tools_shows_install_hint`.
- [ ] **Step 2: Run** `pnpm test src/components/settings src/ipc/mock/mockSettings.test.ts` (fails).
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the tests, then G-WEB.
- [ ] **Step 5: Commit and push:** `git add src && git commit -m "feat(ui): let users turn external RAR tools on or off and see what was found" && git push && git status -sb`

### Task 44: E2E: Organise with disc archives on the mock (Chrome DevTools MCP)

**Files:**
- Modify: `docs/TRACKER.md` (E2E evidence: screenshots under `docs/assets/e2e/disc-archives/`, committed only when they are small PNGs)

- [ ] **Step 1: Start** `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-discs && pnpm dev --port 1455 --strictPort` (background).
- [ ] **Step 2: Drive** with Chrome DevTools MCP: `new_page` `http://localhost:1455` with `isolatedContext: "disc-archives"`. If the profile is locked, use Playwright MCP `browser_navigate`.
  - Open Organise on the downloads fixture.
  - Assert the three disc rows show **Extraer**, `12 archivos · 405,7 MB` / `16 archivos · 1,19 GB` / `52 archivos · 591,9 MB` and the serials, and that `3-D Tetris (USA).zip` is in "Se quedan" as *página web*.
  - Expand the Dreamcast row and check the twin note.
  - Apply, and check the progress line and the result *Extraídos: 3*.
  - Undo.
  - Check the RAR hint and that "Volver a analizar" works.
  - `list_console_messages` must show no errors.
  - Screenshots in light, dark and 390 px.
- [ ] **Step 3: Stop** with `lsof -ti tcp:1455 | xargs kill`.
- [ ] **Step 4: Commit and push:** `git add docs && git commit -m "test(ui): record end-to-end organise run with disc archives" && git push && git status -sb`

---

## Group I — Docs and final verification

### Task 45: Docs: architecture, READMEs, changelog, tracker, spec path

**Files:**
- Modify: `docs/04-architecture/organize.md` (archive reader, disc reader, Extract phases, journal ops, S1–S8 summary)
- Modify: `README.md` and `README.es.md` (supported containers, disc games extracted, the optional RAR tools: `unrar`, 7-Zip, `bsdtar` on macOS)
- Modify: `CHANGELOG.md` (Unreleased: Added / Fixed "7z files named .zip were reported damaged")
- Modify: `THIRD-PARTY-NOTICES.md` (check it against `scripts/check-licenses.sh` output)
- Modify: `docs/TRACKER.md`
- Modify: `docs/superpowers/specs/2026-10-23-organize-archives-and-discs.md` (§1 and §14 sample path → `/Volumes/TinyExt/ROMs-Downloaded`)
- Modify: `docs/superpowers/plans/2026-10-18-duplicates.md` (DD-4 note: use `write::trash` from this branch, AP-3)

- [ ] **Step 1: Write** the docs. The Spanish README uses the same terms as the UI (*Extraer*, *comprimido*, *papelera*).
- [ ] **Step 2: Run** G-WEB `pnpm format:check` (READMEs are not covered, so also check the Markdown links with `grep -o '](\S*)' README.md | sort -u`). Spot-check the links.
- [ ] **Step 3: Commit and push:** `git add README.md README.es.md CHANGELOG.md THIRD-PARTY-NOTICES.md docs && git commit -m "docs: document archive formats, disc extraction and optional RAR tools" && git push && git status -sb`

### Task 46: Final verification

- [ ] **Step 1:** Run every gate: G-RUST, `scripts/check-zip-features.sh`, `scripts/check-licenses.sh`, `scripts/dev/clippy-windows.sh`, `cargo build --offline`, G-WEB, `cargo test -p coverly-core --features test-util --test archive_garbled` and `cargo test -p coverly-core --features test-util --release --test organize_real_sample -- --ignored` (read-only).
- [ ] **Step 2:** Run superpowers:requesting-code-review (or `/code-review high`) on `git diff main...HEAD`. Fix each confirmed finding in its own commit, with a test, pushed.
- [ ] **Step 3:** Post the evidence comment of the local gates. GitHub Actions is blocked by billing; follow `docs/TRACKER.md` "CI local" practice. Then `git status -sb` must show nothing ahead of origin.

---

## Spec → task cross-check

| Spec | Tasks |
|---|---|
| §3 detection, rules 1–7 | 3, 16, 24, 28 |
| §4.1–4.3 module layout, trait, listing | 2, 8–16 |
| §4.4 budgets | 5, 10, 12, 25, 28 |
| §4.5 cache, concurrency, solid 7z | 5, 10, 16, 32 |
| §5 crates, licences, RAR | 1, 13–15 |
| §6 identification | 24, 25 |
| §7 disc systems | 19–21 |
| §8 transfer, layout, containers, checks | 23, 26–28 |
| §9 apply, undo, resume, journal | 29–34 |
| §10 dedupe seams | 2, 22 |
| §11 UI, i18n, mock | 39–44 |
| §12 CLI, IPC | 36–38 |
| §14 testing, real case | all, 35, 44 |

| S-ID | Tests (task) |
|---|---|
| S1 | `garbled_archives_never_panic` (17); `panicking_backend_maps_to_damaged`, `poisoned_reader_stays_damaged`, `panic_hook_suppresses_only_guarded`, `gate_permit_released_on_panic` (6) |
| S2 | `zip_lzma_huge_dict_refused_at_list_and_open`, `zip_ppmd_mem_255_refused_with_lower_cap`, `zip_xz_second_block_huge_dict_refused`, `zip_zstd_window_over_cap_refused`, `entry_changed_between_list_and_open_rechecked` (8); `xz_second_stream_huge_dict_refused` (7); `zstd_second_frame_huge_window_refused`, `zstd_single_segment_huge_content_size_refused` (11); `sevenz_ppmd_mem_4gib_refused` (9); `lzma_lc_lp_term_counted` (7) |
| S3 | `tiny_declared_size_huge_dict_refused`, `cap_check_and_decoder_use_same_dict`, `lzma_alone_declared_size_ignored_for_memory`, `lzma_alone_unknown_size_uses_dict` (7) |
| S4 | `nested_encoded_header_is_damaged`, `encoded_header_unpack_size_over_cap`, `encoded_header_dict_over_cap`, `encoded_header_two_folders_damaged`, `encoded_header_ppmd_coder_damaged`, `encoded_header_without_crc_damaged`, `next_header_offset_overflow_damaged`, `decoded_header_with_too_many_files` (9) |
| S5 | `sevenz_declared_files_mismatch_names`, `sevenz_unknown_property_fails_closed`, `sevenz_trailing_bytes_after_kend_damaged`, `sevenz_pack_pos_outside_file`, `sevenz_coder_count_over_cap` (9); `sevenz_header_swapped_after_listing_damaged`, `sevenz_crate_parses_only_the_validated_header`, `sevenz_crate_parses_the_validated_encoded_header`, `sevenz_valid_real_shaped_archives_list` (10) |
| S6 / S6b | `tar_*_huge_zero_trailer_bounded`, `tar_gz_second_member_after_marker_bounded`, `tar_gz_normal_padding_ok`, `tar_pax_header_huge_size_refused`, `tar_gnu_longname_huge_refused`, `tar_pax_length_overflow_damaged`, `tar_pax_path_overrides_validated_by_relpath`, `tar_sparse_entry_other` (12) |
| S7 | `forged_newline_name_rejected`, `forged_record_with_full_key_set_caught_by_count`, `record_key_set_mismatch_rejected`, `record_count_vs_summary_mismatch_rejected`, `size_total_mismatch_rejected` (14); `listing_over_cap_rejected`, `tool_timeout_killed`, `tool_env_is_cleared`, `stderr_flood_no_deadlock`, `bat_tool_refused_on_windows` (13); `bsdtar_pax_transcode_names_exact` (15) |
| S8 | `wildcard_name_read_by_stream`, `case_twin_read_by_stream`, `dir_prefix_twin_read_by_stream`, `dash_name_not_an_option`, `bang_name_ambiguous_for_7zip`, `stream_mode_refused_with_link_entries`, `stream_slice_crc_mismatch_damaged`, `name_read_crc_verified` (15); `tool_nonzero_exit_rolls_back_item_and_exit_0_extracts` (32) |
| §13.1 extras | names (4), symlinks in archives (8, 9, 12), target-side symlinks (31), device names (4), counts and sizes (8, 9, 12, 16), bombs (5, 8), disk full (31), TOCTOU (32), encrypted (8, 9), long paths (27), Windows CI (every task: the clippy-windows script) |

## Self-review checklist (done while writing this plan)

- The order follows the owner's request: archive core + formats + S1–S8 (Tasks 1–18), disc identification (19–22), planner (23–28), apply/undo/resume (29–35), CLI (36), Tauri/IPC (37–38), mock (39), UI (40–44), docs (45), verification (46).
- Every task ends with commit, `git push` and `git status -sb`. No commit carries a co-author or an AI mention.
- Real data is only read (Task 35, Task 46), at the new path `/Volumes/TinyExt/ROMs-Downloaded`. Writes happen only under `target/disc-sample/`, which is deleted afterwards.
- Each of S1–S8 has named tests in the task that builds the mechanism, and the cross-check table lists them.
- The dedupe seams are proven by a contract test (Task 22) without moving dedupe code into this branch.
