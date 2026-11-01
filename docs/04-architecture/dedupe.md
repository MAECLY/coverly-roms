# Duplicates (the same game stored twice in one system)

> **TL;DR.** Coverly finds the same game stored more than once **inside one system**, at three levels: (1) the exact same game data, whatever the name, container, copier header or compression; (2) another version of the same game (revision, region, beta), from the catalogue, report-only unless "one ROM per game" (1G1R) is on; (3) the same disc in another format (cue/bin, iso, chd, pbp, cso), from the disc serial plus a volume fingerprint. Content decides, never the file name. One copy stays per group, picked by ordered rules with a one-line reason. The other copies are **set aside** into `_Duplicados/` inside the library (relative paths kept), or sent to the OS Trash on request, together with their saves and art. Every move is journaled, resumable and undoable. Nothing is ever deleted. It runs as a library action (`DuplicatesScreen`), as a step of [Organise](./organize.md), and as `coverly dedupe` (a dry run unless `--apply`). Spec: [`2026-10-18-duplicates-design.md`](../superpowers/specs/2026-10-18-duplicates-design.md). Plan: [`2026-10-18-duplicates.md`](../superpowers/plans/2026-10-18-duplicates.md) (decisions DD-1 to DD-13). Code: `crates/coverly-core/src/dedupe/`, `crates/coverly-cli/src/dedupe.rs`, `src-tauri/src/dedupe/`, `src/screens/DuplicatesScreen.tsx`, `src/components/dedupe/`, `src/stores/dedupeStore.ts`.

## Pipeline

```text
library root ──► scan (system folders only, quarantine skipped)
   │
   ▼
units::units_from_scan     units: single ROMs and sets (cue+bins, gdi, ccd, m3u+discs, arcade zips);
   │                       exclusions: folders, BIOS, not a ROM, links, partial, unreadable, quarantine
   ▼
keys                       level 1: size buckets → 128 KiB prefix CRC → payload SHA-1 (colliders only)
   │                       level 2: title_key of the DAT game name (identify::names::parse_tags)
   │                       level 3: serials + volume fingerprint (ReaderDiscProbe)
   ▼
group                      connected components per system, weakest-link relation, report-only reasons
   ▼
saves · keeper · companions   real saves and conflicts → ordered rules → KeepReason; travelling files
   ▼
plan::plan_dedupe ──► DedupePlan (read-only; JSON report, app view)
   ▼  decisions (keeper picks, skipped groups and systems)
apply::apply_dedupe ──► Writer journal (JobKind::Dedupe) ──► undo through the organise undo
```

- **Phases** (`DedupePhase`, the progress events and the planning screen): `listing` (metadata only) → `sizes` (size buckets and the cache) → `fingerprints` (headers, prefix CRCs, full hashes of colliders) → `serials` (disc reads) → `catalogues` (DAT lookups and downloads, at most `dedupe.max_dat_downloads`).
- **Planning only reads.** `plan_dedupe(env, opts)` writes nothing under the library: only cache rows. It can be cancelled between units. Partial results are dropped and the cache is kept.
- **`regroup(plan, opts)`** re-groups an existing plan when an option changes (levels, 1G1R, rules) without reading a file again. `set_keeper` and `skip_group` change one group. The webview never sends a plan back, only the plan id and the decisions.

## Units and exclusions (`dedupe/units.rs`)

A **unit** is what is compared: a single ROM (raw file or single-ROM archive of any format) or a **set** that is never split (cue with its tracks, gdi with its tracks, `.ccd/.img/.sub`, an arcade zip, an `.m3u` with its discs, a multi-disc `.pbp`). Sets are built as a reference graph: two units that reference the same file are merged, so a shared track never moves away from a keeper that needs it.

`Exclusion` lists what is never a unit, and why:

| Exclusion | What |
|---|---|
| `folder` | under `PORTS/`, `BIOS/`, `APPS/` (`dedupe.exclude_systems`), an art folder or a save folder |
| `bios` | a BIOS or firmware file wherever it is (System.dat list, DAT `isbios`, arcade BIOS/device sets): byte-identical PS BIOS twins each serve their own core |
| `not-a-rom` | empty, or the bytes contradict the extension (a web page saved as `.zip`) |
| `symlink` | a symlink, or a unit with a file a symlink points to |
| `hard-link-twin` | another path of a file already taken: moving it frees nothing |
| `partial`, `unreadable` | a download still being written; a read error (reported, never fatal) |
| `multi-game-archive` | an archive that holds several games |
| `quarantine` | inside the quarantine folder (also skipped by the scan and the organise walk) |

**Scope:** every key starts with the unit's system id, from its system folder (library) or from the organise identification (pile). No group spans two systems. This is enforced in the grouping function, not filtered in the UI.

## Keys

**Names never group.** No key reads a file stem, an archive name or an inner entry name. A name can only split a group (arcade set names), downgrade it to report-only (modification tags) or break the final tie (`catalogue_name`, then path order).

| Level | Unit | Key | Acts |
|---|---|---|---|
| 1 exact | raw ROM with a header rule (NES, FDS, SNES copier, **SMS/GG copier**, Lynx, A7800) | system + SHA-1 and size of the headerless payload | yes; differing **functional** headers (NES, Lynx, A7800) are report-only (`header-differs`) |
| 1 exact | raw ROM, no header rule | system + SHA-1 + size; N64 `.v64`/`.n64` rewritten to big-endian first (`n64-order`) | yes |
| 1 exact | single-ROM archive (zip, 7z, rar, tar, gz, bz2, xz, zst, lz4) | the listing's CRC + size, then the payload SHA-1 of a bounded decode | once confirmed; CRC-only is `unconfirmed` |
| 1 exact | arcade / multi-entry archive | sorted multiset of (CRC, size) over all entries | yes, among set names that are not valid drivers (see below) |
| 1 exact | disc set (cue/gdi/ccd) | ordered per-track SHA-1 + track layout; names and `REM` lines ignored | yes |
| 1 exact | CHD | the v3/v4/v5 header `sha1` (data + metadata), header-only read | yes; same `rawsha1`, other `sha1` → `chd-metadata-differs` |
| 1 exact | PBP, CSO, ISO, RVZ | SHA-1 + size, hashed only when the size collides | yes |
| 2 version | units with a catalogue hit by hash | `title_key` of the **DAT game name** + disc number | only with `one_game_one_rom` (else `level-two-off`) |
| 3 format | disc units | ordered serial list + disc number, plus the volume fingerprint | yes when the fingerprints agree; otherwise `build-differs` |

- **Bucketing keeps it cheap.** Per system: bucket by payload size; inside a bucket of two or more, a 128 KiB prefix CRC; the full SHA-1 only for units still colliding. A singleton is never hashed. Archive units bucket on the listing's CRC for free. The 64 MiB `max_hash_mb` cap does not apply to a unit already in a colliding bucket.
- **Reuse.** Raw ROMs reuse the library scan's quick-id memo (`HashOutcome` v2 adds the payload SHA-1 in the same pass), so a file the scan has hashed costs no extra read.
- **Arcade: the name is the driver.** Two byte-identical zips under two **valid** set names (`ffight.zip`, `ffightu.zip`) run different drivers and never group. A group forms only among stems that are not valid set names, plus at most one that is (the forced keeper). Without an arcade DAT, arcade groups are `arcade-no-dat`.
- **Soft patches** (same-stem `.ips/.bps/.ups/.xdelta/.ppf`) are part of the key: a patched and an unpatched copy are different games.
- **Level 2** (`identify::names::parse_tags`): strips known groups (regions, languages, `(Rev N)`, `(V1.1)`, `[!] [a] [b] [o]`, dates, `(Beta)`, `(Proto)`…), keeps the disc number, every unknown group and the modification class (`[h]`, `[t]`, `[T+…]`, `[f]`, `[p]`, `(Unl)`). BIOS entries never join.
- **Level 3**: a serial names a product, not a build, so a match also needs the **volume fingerprint** (PVD volume space size and dates, plus the SHA-1 of `SYSTEM.CNF`, `IP.BIN` or the UMD `PARAM.SFO`). A modification tag in any member's name, a partial set (`partial-set`) or serials the Redump DAT maps to different titles (`serial-conflict`) make the group report-only. A unit whose disc number is unknown takes part only when the DAT says the serial has one disc.
- **Big groups.** A group of more than 50 members (a backup folder copied into itself) is collapsed and report-only (`big-group`) until it is opened and confirmed.

## Groups (`dedupe/group.rs`)

1. Level 1 clusters collapse exact copies into one copy cluster.
2. Clusters that share a level 3 key join.
3. With 1G1R (or reporting of level 2), clusters join on the level 2 key.
4. A group is one connected component inside one system. Each member's relation to the keeper is the **weakest link** on the path (`exact` < `format` < `version`).
5. One-member groups are dropped.

`GroupAction` is `act`, `report-only { why: ReportWhy }` or `conflict`. `ReportWhy`: `level-two-off`, `unconfirmed`, `header-differs`, `build-differs`, `partial-set`, `serial-conflict`, `modified`, `arcade-no-dat`, `big-group`, `user-skipped`, `chd-metadata-differs`, `title-id` (same console title id, contents not compared; systems batch 2 P-6), `folder-game` (a folder game is in the group, never set aside; D28).

## Saves and conflicts (`dedupe/saves.rs`)

- A **save** is a companion with a save extension (`.sav .srm .rtc .eep .sra .fla .mpk .dsv .mcd .mcr .state*`) and the unit's stem, in the same folder or in the profile's save folders, per-core subfolders included.
- A **blank** save (all `0x00`, all `0xFF`, a PS1 memory card with the `MC` header and no used frames) counts as no save.
- Saves keyed by serial or title (DuckStation `memcards/<serial>.mcd`, PPSSPP `SAVEDATA`) are shared by every member and always stay.
- Off-card saves (`SAVE_LOCATIONS`, keyed by profile id, DD-5: `muos` keeps saves on internal storage) cannot be seen, so the screen shows a note once per system and `played` is the only evidence.
- Two or more members with **different real saves** make a **conflict**: no keeper, no destination, not counted, each save listed with size, date and "Open folder". Byte-identical saves are not a conflict.

## Keeper rules (`dedupe/keeper.rs`)

Rules compare members pairwise **in order**. The first rule that tells two members apart decides. All rules can be reordered or switched off (`dedupe.keep_rules`, `--keep-rules`) except the user's pick (always first) and the tie-breaker (always last).

| Rule | Prefers | `KeepReason` |
|---|---|---|
| user | the copy you picked (this session, or remembered) | `user-choice` |
| `has_save` | the copy with a real save or state | `has-save { path }` |
| `played` | the copy the frontend's `gamelist.xml` says you played | `played { playcount }` |
| `in_library` | a copy already in a system folder over a pile copy (organise step) | `in-library` |
| `verified` | DAT hit, good dump over `[b]`/`[o]`, headerless over a copier header (never a headerless NES/Lynx/A7800) | `verified { kept, other }` |
| `retail` | retail > beta > proto > demo/sample (level 2) | `retail { other }` |
| `region` | first match in the Matching region order (level 2) | `region { kept, other }` |
| `revision` | the highest revision (level 2) | `revision { kept, other }` |
| `container` | earliest in the container order of the profile (`dedupe.container_order`) | `container { kept, other }` |
| `cover` | has art in one of the profile's slots | `has-cover` |
| `catalogue_name` | the stem equals the DAT game name, only among proven copies (levels 1 and 3) | `catalogue-name { name }` |
| tie | oldest modified time, then path order | `tie` |

- **Container order defaults** (spec §6.3): handheld profiles (`anbernic-stock`, `garlicos`, `onion`, `muos`, `minui`) `chd > pbp > cue > iso > img > bin` for discs; frontends (`esde`, `batocera`, `retroarch`) `chd > cue > pbp > iso > img > bin`; arcade `zip` only; cartridges no preference (raw = zip).
- **A remembered pick never outranks a new save.** It is dropped, with a note, when a non-keeper gained a real save the keeper lacks or any member's save changed since the pick.
- **Remembered picks** live in `dedupe_choices` (cache schema v3), keyed by system + the keeper's content key, with the group signature and a saves fingerprint. A pick follows a renamed file through the file id on Unix. **On Windows a pick is remembered by path** (`file_id: None`, std has no file id there): renaming or moving that copy falls back to the rules. The keeper never moves, so apply never invalidates a pick. "Apply the rules again" in the screen and `--reset-choices` clear them.

## Companions and art (`dedupe/companions.rs`)

For each extra copy the plan lists what travels with it (`Travel { kind }`): same-stem companions in its folder (saves, states, `.cht`, patches, `.txt`, `.nfo`, case-insensitive, every dot), art in every profile media slot (`ROMS/GB/Imgs/<stem>.png`), and the AppleDouble `._` twins of each. A companion whose stem also matches the keeper or another staying member **stays** (`SharedStay`): `PS/Crash Bash.srm` stays when `Crash Bash.PBP` is kept and `Crash Bash.bin` moves. `gamelist.xml` entries of the extra (`GamelistEntry`) are removed on apply. Art that would leave a keeper without a cover is listed (`ArtLeaves`); the done state offers a fill for those games. Dedupe never writes art for the keeper.

## The quarantine (DD-11)

The default quarantine is **inside the library root**, never inside a system folder, and keeps relative paths: `<root>/<name>/<system folder>/<rel_dir>/<file>` (`default_quarantine`).

| Library | ROMs folder | Quarantine | Writer root |
|---|---|---|---|
| `/Volumes/SD` (`roms_root = "ROMS"`) | `/Volumes/SD/ROMS` | `/Volumes/SD/_Duplicados` | `/Volumes/SD` |
| `~/Documents/ROMS` opened as system folders (`roms_root = ""`) | `~/Documents/ROMS` | `~/Documents/ROMS/_Duplicados` | `~/Documents/ROMS` |

- **Name:** `_Duplicados` in every language (`DEFAULT_QUARANTINE_NAME`), unless `dedupe.quarantine_name` names another. Choosing `_Duplicates` from the app language at first run (spec §18 Q4, DD-8) is not built yet: nothing writes `quarantine_name` on its own. Scans, organise walks and later dedupe runs skip both names and the configured one (`ScanOptions.quarantine_names`, `Exclusion::Quarantine`).
- **One writer root** holds the ROMs, the art, `gamelist.xml` and the quarantine, so every move is an instant rename on one volume and one journal covers them.
- **A custom folder** (`dedupe.quarantine_dir`, `--quarantine DIR`) may be anywhere except inside a system folder (`DedupeError::QuarantineInSystemFolder`, IPC `quarantine-in-system-folder`, settings `dedupe.quarantine_dir: invalid`). On another volume the writer's root is the quarantine and its source root the library: moves become verified copies, the space check runs, and `gamelist.xml` is left as it is with a note (`GamelistNotUpdated`).
- **Same volume frees nothing.** Moving into `/Volumes/SD/_Duplicados` takes bytes out of `ROMS/` (shorter game lists) but the card is exactly as full. Every count says where bytes leave ("1,53 GB out of ROMS/"); the done state says the card is as full until the folder is emptied or moved.
- **Pile copies stay on the pile's side.** In the organise step a pile copy that is already in the library, or loses to another pile copy, goes to `<pile root>/_Duplicados/` on the pile's volume. It is never copied onto the card just to be set aside. A pile inside a system folder uses the library quarantine.
- Each job writes `<quarantine>/<job>.txt` (journaled `created`): one line per extra, `<keeper> <- <extra> (<relation>, <reason>)`, for people who open the folder without Coverly.

## Apply (`dedupe/apply.rs`, DD-12)

`apply_dedupe(plan, decisions, env)` is its own small run over `Writer` and the public organise helpers; the organise run is not refactored.

1. **Decisions** (`DedupeDecisions { keepers, skipped_groups, skipped_systems }`) are applied to a copy of the plan. Every acting group must keep exactly one member that does not move. A decision set that would move every member is an error before any write.
2. **Journal.** `Writer::begin_with(root, data_dir, JobMeta { kind: Dedupe, source_root })`. The plan with the decisions applied is saved as `jobs/<job>.plan.json` before the first move (the header has no quarantine field, DD-10).
3. **Preflight**, once per destination folder, before any copy: free space against the bytes that will cross volumes (`no-space`); FAT32 files over 4 GiB, names FAT/exFAT/NTFS reject, path length and case-only collisions → `destination-limit { fat32-size | bad-name | path-too-long | case-collision }`. For the Trash, `available(path)` once per volume → `trash-unavailable`.
4. **Per group, in plan order:** the keeper is revalidated (exists, same size and mtime), else the whole group is skipped (`keeper-changed`); every file of each extra is revalidated (`changed-since-plan`, `source-missing`). A set is checked whole and moves **all or nothing**: an error halfway moves its moved files back in the same run, by copy when it crossed volumes.
5. **Quarantine:** `ensure_dir` + `move_file_indexed` with `Collision::Rename` (`" (2)"`), never over a file; companions, art and `._` twins follow into the same relative places. A failed companion is reported and never fails the item. The extra's `gamelist.xml` entry is removed with `OverwritePolicy::Backup` when the gamelist is inside the writer root.
6. **Trash** (`dedupe.action = "trash"`, `--trash`): `trash_file` for the extra and each travelling file (below).
7. **Cache:** moved files' `content_keys` rows follow them to the new path. `dedupe_choices` are untouched.
8. **Errors** map like organise: a full disk, a write refused or a vanished root **pause** the job (`paused`, `resumable`); other errors fail that item and the run goes on. Cancel is checked between groups, never inside a set. Progress is throttled to 100 ms.

`resume_dedupe(job, env)` reloads the saved plan, reads what the journal shows done (`job_transfers`, `job_archive_steps`), revalidates the rest and finishes **the same job**. An undone job is refused.

Outcomes: `DedupeItemStatus` is `quarantined { to }`, `trashed { location }`, `skipped { reason }` or `failed { kind, detail }`. `DedupeSkip`: `keeper-changed`, `changed-since-plan`, `source-missing`, `destination-limit`, `trash-unavailable`, `conflict`, `report-only`, `excluded`, `cancelled`. `DedupeReport` adds `bytes_out`, `art_left` and notes.

## Safety rules (spec §8.1, each one tested in core)

1. Never compare across systems.
2. Never group on a file name.
3. Never touch the keeper; revalidate it before moving any extra of its group.
4. Never set aside every member of a group (asserted by the planner, checked again by apply).
5. Never split a set; move it all or nothing.
6. Never overwrite in the quarantine (`Collision::Rename`), and never delete a copy whose name a case twin holds.
7. Never hard delete. The only unlink of a ROM is the source delete after a verified cross-volume copy (copied, fsynced, re-read and checked before the source goes).
8. Exact copies with different real saves: no action for the whole group (conflict).
9. Revalidate every extra (size + mtime) before moving; changed → left alone.
10. `PORTS/`, `BIOS/`, `APPS/`, art and save folders are out of scope; BIOS files are out of scope wherever they are.
11. Never move a file another unit references, nor the target of a symlink.
12. Never act on a group where a name signals a modification, functional headers differ or a disc build differs.
13. Never write a copy the destination file system cannot hold: preflight, then skip with a reason.
14. Never write outside the writer root.

## Journal ops (DD-9)

Dedupe uses only existing ops of the [organise journal](./organize.md#journal-ops), so no new op kind exists and a build that knows organise jobs reads every line:

| Op | When |
|---|---|
| `dir-created` | a quarantine folder `ensure_dir` created |
| `moved { from, via }` | each extra, companion, art file and `._` twin set aside (write-ahead, `fp` after) |
| `created` | the `<job>.txt` list |
| `replaced { backup }` | a `gamelist.xml` edited, with its backup |
| `archive-trash { from }` | before a Trash call, with the file's fingerprint |
| `archive-trashed { from, trash }` | after it, with the exact Trash location (or `null`) |
| `archive-kept { from }` | the volume has no Trash or it refused: the file stayed |

The three trash ops were built for archives organise extracts and take any regular file inside the source root. `trash_file(writer, trash, path, item)` → `TrashStep::{Trashed { location }, Kept { why: unavailable | refused }, Gone}`.

- **Header:** `kind: "dedupe"` (`JobKind::Dedupe`). An older build cannot parse the kind: it lists the journal as unreadable and never half-undoes it.
- **Pruning:** dedupe journals are counted on their own, with the organise rule (the newest `safety.undo_depth`, plus every one younger than `organize.keep_organize_journals_days`). Fill journals never push them out, and a journal with a trash intent that has no outcome line is never pruned.
- **Undo** is the organise undo, per op, not per kind: `undo_job_with(data_dir, job, trash)` / `undo_jobs_in_order_with`, `later_jobs` refusing while newer jobs wrote under the same root, `skipped_occupied` when a file took the original place, the `.txt` list and empty quarantine folders removed, `gamelist.xml` restored from its backup. Trashed copies **come back by themselves** from their recorded Trash place, or are found by name and fingerprint when the place line is missing (a crash during the call). A copy the Trash no longer holds is listed in `UndoReport.archive_lost` ("Not in the Trash any more: {path}"); the rest is still undone. The app undoes through `organize_undo`; `coverly undo` prints `not found in the Trash:` lines.

## The Trash (`write::trash`, AP-3)

Dedupe reuses the organise Trash backend; there is no `trash` crate and no `dedupe/trash.rs`. `TrashBackend { trash(path) -> Option<location>, find(volume, name, fp), available(path) }`: `OsTrash` on macOS goes through `NSFileManager trashItemAtURL:resultingItemURL:` (exact location, Finder "Put Back" also works); on Windows the current user's Recycle Bin on fixed drives only; on Linux the freedesktop Trash. `FakeTrash` serves the tests, and `process_trash()` honours `COVERLY_TEST_FAKE_TRASH` in `test-util` builds. A removable card usually has no usable Trash: the app disables the option with that reason (`trash_available` in the plan view) and an item that still meets one is skipped with the file in place.

## Archive and disc adapters (DD-1, DD-13)

Dedupe owns two narrow traits in `dedupe/source.rs`, `EntryListing` and `DiscProbe`, and implements them over the readers merged with the archive work (PR #24). `tests/archive_dedupe_contract.rs` runs its seven contract tests against the real types.

- **`ArchiveEntryListing`** (`dedupe/archive.rs`) over `archive::open_archive_with` and the process-wide `archive::cache::Cache::global()`: `list` = `file_entries` (files only, archive order: dedupe's entry index space); `head` under `Budget::plan_head`; `payload_sha1` = `open_entry(index)` under the caller's budget, CRC checked by the reader; a spent budget is `Ok(None)` (the member stays `unconfirmed`), never an error. `with_tools(limits, &[library root, quarantine])` builds `ToolsOnDemand`, which is only consulted for RAR and 7z coders the built-in reader lacks, never for a zip, never from a `PATH` entry inside those roots, and never with `--no-external-tools` or the organise setting off. `ZipListing` is gone: 7z, RAR, tar and single-compressor units get entry sizes, CRCs and heads like zips.
- **`ReaderDiscProbe`** (`dedupe/disc.rs`) over `disc::DiscReader::identify_with`: a raw image through `FileSource`; a `.cue`/`.gdi` through `parse_sheet` + `primary_track` (inferred from the data track when the sheet lies past the head budget); inside an archive, the sheet's primary track by name, case-insensitive, in the sheet's folder; heads through `WindowSource` (≤ `MAX_WINDOW`), a `SYSTEM.CNF` past the window fetched and charged to the same plan budget. The answer counts only when the disc's system is the unit's system. `.m3u`: each disc probed in order, serials in order, any unreadable disc → none. `.pbp` (`disc/pbp.rs`, DD-3): the `DATA.PSAR` `PSISO` index, inflating only the blocks that hold sector 16, the root directory and `SYSTEM.CNF` (≤ 1 MiB inflated), never the outer EBOOT `PARAM.SFO`. `.cso` (`disc/cso.rs`): the CISO v1 index, deflate per block, same cap. `.chd` (DD-2): none, level 1 only (header SHA-1); the summary counts CHDs not read inside.
- `DiscIdentity::to_dedupe_id()` gives `DiscId { serials, disc_no, volume, source }`; `VolumeFingerprint` and `DiscIdSource` are dedupe's types, re-exported by `disc`.

## Organise step

Organise groups the pile **and** the library copies of the systems it touches with the same keys and rules (`group_units`), instead of keeping the shortest name. `settings.dedupe.organize_step` / `--duplicates set-aside|report|off`:

- `set-aside` (default): a pile copy already in the library is `OrganizeAction::SetAside { keeper, relation, reason, dest }` into `<pile root>/_Duplicados/` (DD-6) and is never extracted first. `Counts.set_aside` and `Counts.in_library` (pile copies whose group has a library copy) feed the summary.
- `report`: the copy stays where it is with `LeftReason::InLibrary { path, relation }`.
- `off`: no duplicates step.

Inner CRCs come from the organise `Inspection` (no second read), disc identity from `OrganizeItem.disc`. `in_library` beats a pile copy (a library copy stays). Old plans with `LeftReason::Duplicate` still load. See [organize.md](./organize.md#sets-and-companions).

## CLI

```text
coverly dedupe <ROOT> [--profile ID|auto] [--system IDS] [--level exact,format,version]
               [--one-game-one-rom] [--keep-rules RULES] [--keep PATH]… [--reset-choices]
               [--offline] [--no-external-tools] [--dry-run | --apply] [--quarantine DIR | --trash]
               [--resume JOB_ID] [--report table|json] [--output FILE] [--data-dir DIR] [--cache-dir DIR]
coverly undo [--list] [--job ID [--include-later]] [--data-dir DIR]
```

- A dry run unless `--apply`. The table lists each system, each group with its relation and confidence, the copy that stays with its reason, and each extra with its destination; then conflicts, report-only groups, and a summary with the exact count and bytes and where they leave.
- `--resume` refuses the options that shape a plan. `--quarantine` and `--trash` are mutually exclusive. An apply with nothing to move creates no job.
- Exit codes: 0 ok (conflicts included), 1 I/O or job error, 2 bad usage (a quarantine inside a system folder), 3 root not found, 4 no profile detected, 5 paused (prints the `--resume` and `coverly undo --job` commands).
- File names go through `table::terminal_safe`; the JSON report is the plan as data.

## App

`src-tauri/src/dedupe/` (`DedupeManager` on the `OrganizeManager` pattern): commands `dedupe_plan`, `dedupe_cancel`, `dedupe_regroup`, `dedupe_set_keeper`, `dedupe_skip_group`, `dedupe_skip_system`, `dedupe_reset_choices`, `dedupe_apply`, `dedupe_cancel_apply`, `dedupe_job`, `dedupe_resume`, `dedupe_export_plan`; events `dedupe-progress` and `dedupe-changed`; the newest four plans kept (`KEPT_PLANS`); apply behind the `JobManager` write guard; undo through `organize_undo`. The screen, store and entry points are in [desktop-app.md](./desktop-app.md#duplicates).

## Performance budget

| Case | Budget | How |
|---|---|---|
| Owner's library (~6,200 ROM-like files, 91 GB, 55 systems) | first run < 2 min on SSD, < 10 min on SD; warm run < 5 s | only colliding buckets are hashed; the read-only scan behind spec §15 took about 30 s with the same bucketing |
| Warm run, nothing changed | no file reads | `content_keys` rows (`volume_uuid + path + size + mtime`), reverse index by key; the file id is only a hint, trusted on FAT/exFAT only on the same path |
| 50,000 files, cold, all raw on SD (~40 MB/s) | ≈ 3 min of prefix reads | cartridge sizes are powers of two, so reuse matters more than buckets: archive CRCs (no read), the scan's quick-id hashes (no read), a 128 KiB prefix CRC only for files never hashed |
| Disc units | a few sector reads each | 64 MiB plan head budget and 20 s wall time per archive (`archive::Limits`); ≤ 1 MiB inflated per PBP/CSO; never a full disc |
| Workers | 4, 2 on removable or network volumes | organise rule |

`warm_run_hashes_nothing`, `huge_group_is_collapsed_and_report_only` and the perf gate `ten_thousand_files_hash_only_colliders` (`#[ignore]`, run with `cargo test --release -- --ignored`) are in `tests/dedupe_plan.rs`.

## Tests

Synthetic fixtures only, with pinned mtimes (`dedupe::testing::pin_mtime`: on Windows files written in one clock tick share an mtime). Core: `tests/dedupe_{units,keys,group,saves,keeper,companions,plan,apply,undo,disc}.rs`, `tests/archive_dedupe_contract.rs`, the organise suites (`organize_{plan,apply,undo}.rs`) and `tests/dedupe_real_sample.rs` (`#[ignore]`, read-only on the owner's library). CLI: `crates/coverly-cli/tests/cli_dedupe.rs`, `cli_organize.rs`. App: `src-tauri/tests/dedupe_state.rs`, the shared wire contract. UI: `src/ipc/mock/mockDedupe.test.ts`, `src/stores/dedupeStore.test.ts`, `src/screens/DuplicatesScreen.test.tsx`, `src/components/dedupe/*.test.tsx`. Windows clippy (`scripts/dev/clippy-windows.sh`) compiles the Windows Trash code.
