# Organise (loose ROMs by system)

> **TL;DR.** Picking a folder that follows no known layout — a downloads folder full of zips — used to end in "No se reconoce ningún formato". Coverly now offers to **organise** it: every file is identified (hash → header → extension → name, plus an optional online lookup), given a system, a confidence and a "why", and a **plan** proposes one folder per system in a layout profile (by default Anbernic stock: `ROMS/GBA/`, `ROMS/GB/`, `ROMS/SFC/`…). Planning is read-only. Applying moves or copies the files through the journaled writer, never replaces anything, can pause and resume, and is undone like a fill job. The result opens as a normal library, so scan and fill work unchanged. Spec: [`2026-10-17-organize-loose-roms-design.md`](../superpowers/specs/2026-10-17-organize-loose-roms-design.md). Plan: [`2026-10-17-organize-loose-roms.md`](../superpowers/plans/2026-10-17-organize-loose-roms.md). Code: `crates/coverly-core/src/organize/`, `identify/sniff.rs`, `write/{noclobber,transfer,undo_organize}.rs`, `crates/coverly-cli/src/organize.rs`, `src-tauri/src/organize/`, `src/screens/OrganizeScreen.tsx`, `src/components/organize/`, `src/stores/organizeStore.ts`.

## Pipeline

```text
probe_folder (names + stat only, ≤ 5 000 entries)          ← start-screen offer
   │
walk      organize/walk.rs      read-only; no symlinks, one volume, depth 4;
   │                            skips, partial downloads, BIOS, cloud-only,
   │                            recently modified, already-sorted folders
inspect   organize/inspect.rs   raw file | zip entries (bounded) | 7z/CHD/RAR magic | disc sets
   │      identify/sniff.rs     header magic over a 128 KiB prefix + targeted reads
ladder    organize/evidence.rs  extension → zip CRCs → header → DAT by CRC → raw hash
   │                            (only when it can change the outcome) → names → arcade sets
   │                            → online (optional)
combine   organize/evidence.rs  scores, margin, content support, vetoes → auto | review | unknown
   │
target    organize/target.rs    folder per layout, target paths, safe names, collisions,
   │                            duplicates, companions, sets, FAT32 limit
plan      organize/plan.rs      OrganizePlan (items, counts, warnings); user decisions re-plan
   │                            from the kept survey without reading files again
apply     organize/apply.rs     re-validate → ensure_dir / move_file / copy_file (write-ahead
   │                            journal) → pause / resume → report
undo      write/undo_organize.rs  reverse order: move back, delete copies, remove empty folders
```

`survey` (walk + inspect + ladder for every candidate) is the expensive part; `plan_from_survey` turns a survey plus options and decisions into a plan and does no I/O beyond `stat`, which is why changing the review answers on screen is instant. The app keeps the survey per source and options, so changing only the names or the mode re-plans without surveying again; changing anything else (layout, target, *Directly here*, offline, an edited custom layout, online lookup becoming available) surveys again.

### Workers and progress

Identification runs on `std::thread::scope` workers with an atomic work index (O-D2): 4 workers, 2 when the source is on a removable or network volume. Progress (`OrganizeProgress { phase, done, total }`, phases `walk`, `identify`, `dats`, `hash`, `apply`) is throttled to one event per 100 ms, with phase changes and ends always sent. Planning and applying are cancellable between files.

### Memo

`organize_ids` (cache schema v2) stores per-file **facts** — container, inner names and CRCs, sniff results, raw CRCs once hashed — keyed by path + size + mtime (O-D6). DAT lookups and `combine` run again on every plan, so a DAT downloaded later or a new threshold is honoured without reading the files again.

## Walk and skips

`organize::walk::walk` reads names and metadata only.

- **Never followed:** symlinks, junctions and other reparse points (*symlink*), other mounted volumes, hidden folders, the ignored folders of the scan settings (`bios`, `saves`, `states`, …), the layout's art folders, and the target tree when it is inside the source. Folders deeper than 4 levels are reported (`organize.deepFolders`), not dropped silently.
- **Already sorted:** a folder that resolves to a system (`SystemDb::resolve_folder`, any case) is not walked; it is reported once as *already in a system folder*.
- **Left in place** (each with a reason, never moved):

| Reason | When |
|---|---|
| `partial-download` | `.crdownload`, `.part`, `.partial`, `.download`, `.opdownload`, `.tmp`, `.!ut`, `.!qb`, `.aria2`, `.filepart`, Safari `.download` bundles, zero-byte files, a file with a `<name>.part` sibling |
| `recently-modified` | modified within `settings.organize.recent_seconds` (60 s) at plan time |
| `cloud-only` | macOS dataless files (`SF_DATALESS`), Windows `OFFLINE` / `RECALL_ON_DATA_ACCESS`: identified by name and extension only, never Auto |
| `bios` | BIOS and firmware names (`gba_bios.bin`, `bios7.bin`, `scph*.bin`, `neogeo.zip`, `syscard*.pce`, `kick<digits>…rom`, …; list in `scan/classify.rs`) |
| `not-a-rom` | the first 512 bytes are text; `web-page` when it opens like HTML/XML/JSON |
| `damaged-archive`, `unsupported-archive`, `multi-game-archive` | see [Archives](#archives) |
| `unsupported-system` | a signature of a system Coverly does not organise (Sega Pico, PlayStation 2) |
| `unknown` | ROM-like, but nothing reached the review threshold (still assignable) |
| `duplicate`, `already-present` | same CRC + size as another item, or already in the target folder |
| `already-sorted`, `symlink`, `unreadable` | as named |
| `set-incomplete`, `set-outside-folder`, `shared-track`, `collision-set` | disc sets (see [Sets](#sets-and-companions)) |
| `ambiguous-companion` | a save/patch whose stem matches several moved ROMs |
| `too-big-for-fat` | ≥ 4 GiB on a FAT32 target |

Junk (`.DS_Store`, `Thumbs.db`, `desktop.ini`, AppleDouble `._*`, `*:Zone.Identifier`) is counted and not listed. Files that are not ROM-like at all (`.dmg`, `.exe`, `.docx`…) are counted as `other_files` and listed only on demand (O-D7).

## Archives

`organize::inspect` never extracts and never decompresses a whole entry.

| Case | Handling |
|---|---|
| Zip, one ROM entry (plus readmes/NFOs, OS metadata ignored) | identified by that entry: central-directory CRC + size, inner name, 128 KiB header read |
| Zip, several ROM entries of one system | each entry identified on its own; all agree → moved, with the note *holds {n} ROMs; the console loads the first* |
| Zip, entries of different systems, or more than 8 ROM entries | `multi-game-archive` (extract it first) |
| Zip with more than 10 000 entries or a central directory over 16 MiB | `damaged-archive` (bounded parse) |
| `.zip` that is not a zip | text → `not-a-rom`; otherwise `damaged-archive` |
| Encrypted entry or unsupported compression | CRC level still works; header skipped; note `encrypted` |
| `.7z`, `.chd` | magic checked, contents not read: name and extension only, never Auto; bad magic → `damaged-archive` |
| `.rar` and others | `unsupported-archive` |

The central-directory CRC is what the zip claims; it is not verified against the data. Deflated entries are inflated as a stream only up to the offset a check needs, capped at 8 MiB (enough for an SNES ExHiROM header at 0x40FFC0), which also defuses zip bombs.

## Identification ladder

The *precedence* is hash > header > extension > name; the *evaluation order* is by cost, and expensive levels run only when they can change the outcome:

1. **Extension** (`extension_evidence`): unique to one system → 0.85; shared by several → 0.50 each; generic (`bin`, `iso`, `img`, `rom`, `dat`, …) → nothing on its own. A unique extension of a system with a decisive signature whose bytes do not carry it drops to **0.60** (detail `ext!`, note *the extension says X but the contents don't look like X*). `zip`/`7z` are containers, never evidence.
2. **Header** (`identify::sniff`): see [identification.md](./identification.md#header-magic-organise) for the table; 0.70–0.95, plus vetoes.
3. **DAT by CRC** (`dat_level`): zip CRCs at once, raw files only after hashing. A hit in a DAT of a candidate system → **1.00**; no candidate hit but a hit in any cached DAT (`dat_roms(crc32)` index) → **0.97**. Several systems hit (the same CRC in GB and GBC catalogues) → kept to those the header names. Missing DATs of candidate systems are downloaded through `DatStore` unless offline or `download_dats = false`, at most **4 downloads per plan**; the rest are reported (`catalogue-missing`).
4. **Raw hash** (`hash_rom`, 64 MiB cap): only when levels 1–3 leave the file below Auto, or *Names: catalogue* needs the DAT name, or a duplicate check needs it (another candidate with the same size). A plan of thousands of raw files with valid headers hashes none of them.
5. **Names** (`parse_name` + `strip_numbering_prefix`, outer and inner stems): tags `(SGB Enhanced)` → gb/gbc, `(GB Compatible)` → gbc, GBA scene groups (`Venom`, `Paracox`, `BatMan`, `Independent`, `Rising Sun`, …) → gba, all at **0.60**; fuzzy title against the DAT stems of the candidate systems only: score ≥ 95 → **0.80**, ≥ 85 → **0.70**. Names never reach Auto on their own.
6. **Arcade sets** (multi-entry zips): the zip stem is looked up as a set in the cached arcade DATs; ≥ 75 % of the set's ROM entries present by CRC → confirmed (1.00 for the first system in the layout's `arcade_order`, 0.99 for the others, listed as alternatives); name only → **0.70** (review).
7. **Online** (optional, O-D9): ScreenScraper `jeuInfos` by CRC, only when the build has developer credentials, the user enabled ScreenScraper and set an account, and the plan is not offline; only for items still in review or unknown, at most 3 candidate systems each, **0.80**.

A **user choice** (`method = user`) is remembered by content (CRC32 + size) in `organize_choices`, so the same file is not asked about twice.

### Combine

`organize::evidence::combine`, pure:

- Per system: `score = max(weights) + 0.03 × (independent agreeing levels − 1)`, capped at 1.00. Conflicting levels subtract nothing; they are listed.
- **Auto** when `score ≥ auto_threshold` (**0.85**), the lead over the runner-up is **≥ 0.10**, and the system has **content support**: a DAT hit, header evidence ≥ 0.85, or a unique extension (≥ 0.85) of a system without a decisive signature whose bytes were read. A user choice is Auto unless a hard veto excludes it.
- **Review** when `score ≥ review_threshold` (**0.50**) but not Auto. On a broad root (home, Desktop, Documents, Downloads, a volume root, `/`, `C:\`) a review item without content support becomes *unknown* (O-D8).
- **Unknown** otherwise. Candidates and alternatives are systems scoring **≥ 0.40**, best first.
- **Hard vetoes**: a GBC-only header (CGB `0xC0`) excludes `gb`; `not-a-rom` and an unsupported system stop the ladder before combining.
- The "why" keeps at most 4 pieces of evidence for the chosen system, user choice first, then by precedence.

Thresholds are `settings.organize.auto_threshold` / `review_threshold`, separate from the art-matching thresholds.

## Target layout

- **Folder per profile** (`Profile.system_folder: FolderNaming`): `FirstAlias` (the system's first alias in `systems.toml`, i.e. the Anbernic SD name: `GBA`, `SFC`, `FC`, `MD`, `PS`) for `anbernic-stock`, `garlicos`, `onion` and `minui`; `Id` (`gba`, `snes`, `nes`, `megadrive`, `psx`) for `esde`, `batocera`, `muos` and `retroarch`. Custom profiles may declare `system_folder` and their own map.
- **Overrides**: `settings.organize.folder_names[<profile>][<system>]`. An override that does not resolve back to its system is ignored.
- **Host folders**: Anbernic stock, GarlicOS and Onion keep `ngpc` in the NGP folder and `wonderswancolor` in the WS folder (the devices have no NGPC/WSC folder).
- **Invariant**, tested for every built-in profile × system: the folder name resolves back through `SystemDb::resolve_folder` to the system or its host, so the scan sees what organise created.
- **Device folders** (O-D10): only `anbernic-stock` lists the folders its card shows (`Profile.known_folders`, the reference `~/Documents/ROMS`); a system outside that list gets the note *{layout} has no folder for {system}; {folder} will be created*.
- **Paths**: target root `T` = the picked folder or another one; ROMs folder `R = T/<roms_root>` (or `T` with *Directly here* / `--flat`); item target `R/<folder>/<name>`. An existing folder under any casing is reused. Names go through `safe_file_name` on every platform; a path that would exceed 259 UTF-16 units gets a shorter stem; on FAT32 targets files ≥ 4 GiB stay in place. Every renamed target carries its reason (`collision`, `safe-name`, `path-length`, `catalogue`).
- **Collisions** inside the plan and against the disk are resolved at plan time with the next free ` (n)` name, so the dry run shows final names. Same content already in the target folder → `already-present`.

## Sets and companions

- **Disc sets** (`scan::multifile::group_sets`: `.cue` → tracks, `.gdi` → tracks, `.m3u` → discs) are one item, identified from the primary data track, and move together keeping their relative layout. A missing or partial member, a member outside the set's folder, a track shared by two sets, or a member colliding at the target leaves the **whole set** in place. Disc sets are never catalogue-renamed (their text names the members).
- **Companions**: files with the exact stem of a moved ROM and an extension in `settings.organize.companion_exts` (`sav`, `srm`, `rtc`, `state*`, `cht`, `ips`, `bps`, `ups`, `txt`, `nfo`, `png`, `jpg`) follow it into the same folder (images are not put into `Imgs/`; that is the fill flow's job). With *Names: catalogue* they are renamed to the new stem; if any of them would collide, the ROM keeps its name too.
- **Duplicates**: same CRC32 + size twice in the pile → the first (shortest name, then alphabetical) is planned, the others stay as `duplicate` of it. Browser suffixes (` (1)`) are never treated as duplicates by name; nothing is deleted.

## Apply

`organize::apply::apply_organize(plan, env)`:

1. Review items without the user's answer are reported `skipped: not-decided`; only Auto items with a target are applied.
2. Source or target root gone → paused (`volume-gone`) before anything is written.
3. **Free space**: a copy, or a move across volumes, needs the items' bytes + 1 % free on the target volume, else `no-space` with both numbers and nothing written.
4. The writer starts an organise journal (`kind: organize`, `source_root`), and the plan is saved to `<data>/jobs/<job>.plan.json` **before the first action** — that file is what makes resume possible after a restart.
5. Every item is **re-validated** (each source still has the planned size and mtime, else `changed-since-plan` / `source-missing`), then moved or copied with `Writer::ensure_dir` / `move_file` / `copy_file`. One directory index per run (`DirIndex`, one `read_dir` per folder) replaces per-file folder listings.
6. Manual identities (`identity-overrides.json`) keyed by a moved path follow the file (`override-moved`).
7. A full, read-only or vanished volume pauses the job (`disk-full`, `write-denied`, `volume-gone`) with the not-attempted items as `resumable_items`. Other per-item errors fail that item only.

`resume_organize(job)` loads the saved plan, counts the items the journal shows as done, finishes items whose ROM moved but whose set members or companions did not, deletes the job's leftover `.coverly-<job>-*.tmp` files, and applies the rest into the **same journal**: one undo reverts every run.

### Safety rules

- **Dry run first.** Planning only reads. The CLI is a dry run unless `--apply` (or the "dry run by default" setting is off); the app's proposal is the dry run.
- **Never overwrite.** All renames go through `write::rename_noclobber`: `renamex_np(RENAME_EXCL)` on macOS, `renameat2(RENAME_NOREPLACE)` on Linux, `MoveFileExW` without `MOVEFILE_REPLACE_EXISTING` on Windows; where unsupported, `hard_link` + delete; where links are unsupported too (FAT/exFAT), a last case/NFC twin check and a plain rename (the remaining race is documented). A collision found at that point gets the next ` (n)` name. A set whose member collides is skipped whole (`collision-set`).
- **Same volume** = same `st_dev` on Unix, same drive letter or UNC share on Windows (O-D3). A move across volumes, or an `EXDEV` / `ERROR_NOT_SAME_DEVICE` from `rename`, becomes a streamed copy → `sync_all` → **re-read and CRC-check of the target** → delete the source. If the source cannot be deleted, the item is `copied-not-removed`, journaled as a copy.
- **Streamed copies** (1 MiB buffer) into a `create_new` temp file next to the target, then `rename_noclobber`. Never `fs::copy` (it carries xattrs, which become `._*` files on FAT). The source's mtime is kept; AppleDouble sidecars of created files and folders are removed.
- **Fingerprints** (`size`, `mtime_ns`, `crc32` for copies) are read from the target **after** the operation, because FAT stores mtimes with 2 s resolution.
- **Windows sharing violations** (32, 33) are retried 3 times (200/500/1000 ms).
- **Roots**: targets stay inside `T`, sources inside the source root; the app refuses an apply when its job data folder lies inside the folder being organised.
- **Case-insensitive names everywhere**: comparisons use NFC + lowercase on every volume; `GBA/` is never created next to `gba/`; a rename that only changes case is a no-op.
- **One writer at a time** in the app: apply and resume take the `JobManager` manual-write guard (refused with `job-running` while a fill gathers, applies or is paused).

## Journal ops

Organise extends the JSON-lines journal of [fill-pipeline.md](./fill-pipeline.md#writer-guarantees) compatibly. Paths are **absolute** (O-D1):

```json
{"type":"header","job":"20261017T101500Z-4242-0000","root":"/abs/ROMs-Downloaded","source_root":"/abs/ROMs-Downloaded","kind":"organize","created_at":"2026-10-17T10:15:00Z"}
{"type":"entry","target":"/abs/ROMs-Downloaded/ROMS","op":{"kind":"dir-created"}}
{"type":"entry","target":"/abs/ROMs-Downloaded/ROMS/GBA","op":{"kind":"dir-created"}}
{"type":"entry","target":"/abs/ROMs-Downloaded/ROMS/GBA/Tetris Worlds (USA).zip","op":{"kind":"moved","from":"/abs/ROMs-Downloaded/Tetris Worlds (USA).zip","via":"rename"},"fp":{"size":1873412,"mtime_ns":1760…},"item":3}
{"type":"entry","target":"/abs/SD/ROMS/SFC/Tetris 2.zip","op":{"kind":"copied","from":"/abs/ROMs-Downloaded/Tetris 2.zip"},"fp":{"size":…,"mtime_ns":…,"crc32":…},"item":7}
{"type":"done"}
```

| Op | Written | Undo |
|---|---|---|
| `dir-created` | by `ensure_dir`, only for folders it created | removed when empty (`.DS_Store` / `._*` litter ignored) |
| `moved { from, via: rename\|copy }` | before the move (write-ahead) | target matches `fp` and `from` is free → moved back with `rename_noclobber` (streamed back across volumes); target missing → nothing; target changed → `skipped_modified`; `from` taken → if it is the same file (size + CRC: the crash window of a cross-volume move) the target copy is deleted, else `skipped_occupied` |
| `copied { from }` | before the copy | target matches `fp` → deleted; else kept |
| `override-moved { from, to }` | when a manual identity followed a moved ROM | rekeyed back |

- The header's `kind` (`fill` default) and `source_root` (default `root`) are optional, and `sha1` became optional too: journals written before organise parse and undo unchanged.
- An unknown op kind (a journal from a newer Coverly) makes that job **not undoable** in `list_jobs` (`undoable: false`) instead of breaking the listing.
- Undo of a `moved` entry needs the source root present; otherwise it is refused (`write-denied` on that root) instead of reporting every file as failed.
- **Later jobs**: `write::later_jobs(job)` lists newer, not-undone jobs whose root or source root overlaps the job's root (a fill that wrote `ROMS/GBA/Imgs/…`, a later organise of the organised tree). The app (`organize_undo`) and `coverly undo --job` refuse with those jobs unless asked to include them, then undo them newest first (`undo_jobs_in_order`).
- **Pruning** (at the end of every fill run, `prune_jobs_at`): fill and organise journals are counted separately, the newest `safety.undo_depth` of each kind kept; organise journals are also kept while younger than `settings.organize.keep_organize_journals_days` (90). Pruning removes the journal with its `.plan.json` and backups.

## Cache schema v2

`crates/coverly-core/src/cache/mod.rs`, `PRAGMA user_version = 2`, an additive migration from v1:

| Object | Purpose |
|---|---|
| index `dat_roms_crc_any (crc32)` | CRC lookup across every cached DAT (`dat_by_crc_any`), for files without a candidate system |
| index `dat_roms_game (dat, game)` | rows of one arcade set (`dat_rows_of_game`) |
| table `organize_ids (path, size, mtime_ns, facts)` | memoised per-file facts (O-D6) |
| table `organize_choices (crc32, size, system_id)` | remembered user choices |

## Entry points

- **Start screen**: on `no-profile`, `probe_folder` counts ROM-like files (same depth as the walk, stops after 5 000 entries). With ROM-like files it shows the offer (*Organise this folder…*); a folder that only holds system folders gets *Open as organised by system* with a layout picker; broad roots get a warning line.
- **Library screen**: when the scan found ROM-like files outside the system folders (`LibraryView.loose_files`), *Organise loose files…* opens the same screen with source = target = the library root and the library's layout locked.
- **Detection fallback**: `profiles::detect_with_fallback` suggests `settings.organize.default_layout` (reason `system-folders`) for a folder organised by system with no layout markers — a `roms` child (any case) holding at least one filled system folder, or the root holding at least two. The UI shows it as a suggestion.
- **After apply**: *Open library* calls `open_library(target, layout)` with the layout explicitly, so no detection is needed.
- **CLI**: `coverly organize` (below).

The screen, commands and events are described in [desktop-app.md](./desktop-app.md#organise).

## CLI

```text
coverly organize <SOURCE> [--layout PROFILE_ID] [--target DIR] [--flat] [--copy | --move]
                 [--names keep|catalogue] [--assign FILE=SYSTEM_ID]… [--include-review]
                 [--offline] [--dry-run | --apply] [--resume JOB_ID]
                 [--report table|json] [--output FILE] [--data-dir DIR] [--cache-dir DIR]
coverly undo [--job ID [--include-later]] [--list] [--data-dir DIR]
coverly systems                     # system ids for --assign
```

- Dry run by default; the table groups items by system with target folder, confidence and why, then *To review* and *Left in place*, and ends with "Dry run: nothing was written. Re-run with --apply to organise {n} files."
- Review items are applied only with `--assign` (split on the last `=`, `FILE` relative to the source) or `--include-review` (best candidate). An `--assign` on an item that stays in place is reported, not dropped silently.
- `--resume` continues an interrupted job from its saved plan and refuses the plan options (`--layout`, `--target`, `--flat`, `--copy`/`--move`, `--names`, `--assign`, `--include-review`, `--apply`, `--dry-run`).
- Exit codes: 0 ok (also with items left in place), 1 I/O or job error (including not enough space), 2 usage (unknown layout or system id), 3 source not found or not a folder, 5 paused (prints the `--resume` and `coverly undo --job` commands). Exit 4 never applies: organise needs no detection.

## Tests

Synthetic fixtures only (no ROM data is committed): `identify/sniff.rs` unit tests (one positive and one near miss per signature, deterministic random-bytes guard), and the integration suites `crates/coverly-core/tests/organize_{inspect,walk,evidence,target,plan,apply,undo}.rs`, `crates/coverly-cli/tests/cli_organize.rs`, the IPC contract (`src-tauri/tests/contract.rs`, `src/ipc/contract.test.ts`) and the Vitest suites of `organizeStore`, `OrganizeScreen`, `StartScreen` and `src/components/organize/`. The read-only run against the owner's real sample and the E2E on the mock are recorded in the [tracker](../TRACKER.md) (O10).
