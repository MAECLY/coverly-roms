# Organise (loose ROMs by system)

> **TL;DR.** Picking a folder that follows no known layout — a downloads folder full of zips — used to end in "No se reconoce ningún formato". Coverly now offers to **organise** it: every file is identified (hash → header → extension → name, plus an optional online lookup), given a system, a confidence and a "why", and a **plan** proposes one folder per system in a layout profile (by default Anbernic stock: `ROMS/GBA/`, `ROMS/GB/`, `ROMS/SFC/`…). Planning is read-only. Applying moves or copies the files through the journaled writer, never replaces anything, can pause and resume, and is undone like a fill job. The result opens as a normal library, so scan and fill work unchanged. Containers are recognised by their **magic**, not their extension: a 7z named `.zip` is a 7z with a note, never "damaged". Every common archive and compressor opens (zip, 7z, RAR through an optional external program, tar, gz, bz2, xz, zstd, lz4, lzma). Disc games inside an archive are identified from their disc headers (PlayStation, Saturn, Dreamcast, Sega CD, PC Engine CD, Neo Geo CD) and **extracted** on apply, verified track by track, with the archive sent to the Trash only after everything is committed. Spec: [`2026-10-17-organize-loose-roms-design.md`](../superpowers/specs/2026-10-17-organize-loose-roms-design.md), extended by [`2026-10-23-organize-archives-and-discs.md`](../superpowers/specs/2026-10-23-organize-archives-and-discs.md) (archives and discs). Plans: [`2026-10-17-organize-loose-roms.md`](../superpowers/plans/2026-10-17-organize-loose-roms.md), [`2026-10-23-organize-archives-and-discs.md`](../superpowers/plans/2026-10-23-organize-archives-and-discs.md). Code: `crates/coverly-core/src/organize/` (`extract.rs` for extraction), `archive/`, `disc/`, `identify/sniff.rs`, `write/{noclobber,transfer,trash,undo_organize}.rs`, `crates/coverly-cli/src/organize.rs`, `src-tauri/src/organize/`, `src/screens/OrganizeScreen.tsx`, `src/components/organize/`, `src/stores/organizeStore.ts`.

## Pipeline

```text
probe_folder (names + stat only, ≤ 5 000 entries)          ← start-screen offer
   │
walk      organize/walk.rs      read-only; no symlinks, one volume, depth 4;
   │                            skips, partial downloads, BIOS, cloud-only,
   │                            recently modified, already-sorted folders
inspect   organize/inspect.rs   raw file | any archive (archive::open_archive, by magic) | disc sets
   │      identify/sniff.rs     header magic over a 128 KiB prefix + targeted reads
   │      disc/                 disc headers of loose sets and of disc archives (DiscReader)
ladder    organize/evidence.rs  extension → zip CRCs → header → DAT by CRC → raw hash
   │                            (only when it can change the outcome) → names → arcade sets
   │                            → online (optional)
combine   organize/evidence.rs  scores, margin, content support, vetoes → auto | review | unknown
   │
target    organize/target.rs    folder per layout, target paths, safe names, collisions,
   │                            duplicates, companions, sets, extract targets, FAT32 limits
plan      organize/plan.rs      OrganizePlan (items with Transfer::AsIs | Extract, counts,
   │                            warnings, free space); user decisions re-plan from the kept
   │                            survey without reading files again
apply     organize/apply.rs     re-validate → ensure_dir / move_file / copy_file (write-ahead
   │      organize/extract.rs   journal); Extract items: phases A–D → pause / resume → report
undo      write/undo_organize.rs  reverse order: restore trashed archives, delete extracted
                                files and copies, move back, remove empty folders
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
| `damaged-archive`, `unsupported-archive` (with the `format` and the program it `need`s), `multi-game-archive`, `encrypted-archive`, `archive-needs-memory`, `archive-too-big`, `multi-volume-archive`, `unsafe-archive-entry`, `nested-archive`, `repack-arcade` | see [Archives](#archives) |
| `set-unsafe-name`, `path-too-long`, `folder-full`, `unsafe-target` | extraction targets (see [Extract](#extract-disc-games-and-unloadable-containers)) |
| `unsupported-system` | a signature of a system Coverly does not organise (Sega Pico, PlayStation 2) |
| `unknown` | ROM-like, but nothing reached the review threshold (still assignable) |
| `duplicate`, `already-present` | same CRC + size as another item, or already in the target folder |
| `already-sorted`, `symlink`, `unreadable` | as named |
| `set-incomplete`, `set-outside-folder`, `shared-track`, `collision-set` | disc sets (see [Sets](#sets-and-companions)) |
| `ambiguous-companion` | a save/patch whose stem matches several moved ROMs |
| `too-big-for-fat` | ≥ 4 GiB on a FAT32 target |

Junk (`.DS_Store`, `Thumbs.db`, `desktop.ini`, AppleDouble `._*`, `*:Zone.Identifier`) is counted and not listed. Files that are not ROM-like at all (`.dmg`, `.exe`, `.docx`…) are counted as `other_files` and listed only on demand (O-D7).

## Archives

Planning never extracts anything to disk. `organize::inspect` opens every container through `archive::open_archive_with(path, limits, tools)` and reads only what a check needs, within a **64 MiB decode budget per archive**.

### Container detection (magic first)

`archive::detect::detect_container(head, len, tail)` reads the 128 KiB prefix and the last 64 KiB + 22 bytes (a zip's end-of-central-directory, so a prefixed or self-extracting zip is found). `organize::inspect::container_for(ext, magic)` = `magic.or_else(by_extension)`: the extension is only a fallback.

| Format | Magic | Extra validation |
|---|---|---|
| zip, zip64 | `PK\x03\x04`, empty `PK\x05\x06`, EOCD in the tail | EOCD offsets inside the file (zip64 needs its locator) |
| 7z | `37 7A BC AF 27 1C` | major version 0 |
| RAR 4 / RAR 5 | `Rar!\x1A\x07\x00` / `Rar!\x1A\x07\x01\x00` | — |
| gzip, bzip2, xz, zstd, lz4 | `1F 8B 08`, `BZh1–9` + block magic, `FD 37 7A 58 5A 00`, `28 B5 2F FD` (+ skippable frames), `04 22 4D 18` | reserved bits, stream-header CRC, version bits |
| tar | `ustar` at 257, or a valid header checksum (v7 tar) | checksum |
| LZMA alone | none (`.lzma` only) | props < 225, dictionary under the cap |
| CHD | `MComprHD` | a disc image, not an archive: unchanged |
| cab, arj, lzh, Unix compress, ECM | their magic | recognised, never opened → `unsupported-archive` with the format's name |

Rules:

- An extension that names another archive format than the magic (`.zip` holding a 7z) is the note *It is a 7z archive named .zip* (`ItemNote::ContainerMismatch`), **never** a reason to leave it. The file keeps its name.
- **Damaged** only when the format's integrity listing fails: zip central directory, 7z header with its CRC, the external tool's listing and exit status, the tar header walk, a compressor's frame/stream header. A spent budget, a size cap or a missing program is never "damaged".
- No magic but an archive extension: the format's reader is tried anyway; text or HTML → `not-a-rom` (`3-D Tetris (USA).zip` is a saved web page); otherwise `damaged-archive`.
- **Multi-volume** sets (`.7z.001`, `.partN.rar`, `.rNN`, `.zNN`, the zip spanned marker, the RAR volume flag) become **one** left item `multi-volume-archive` with all its parts, so later parts are never moved apart from the first.
- A compressor holding a tar (`.tgz`, `.tar.xz`, or a first decoded block that is a tar header) opens as `Tar { outer }`; a single compressed ROM (`game.gba.gz`) is one virtual entry named from gzip's `FNAME` (validated) or the file stem.

### Archive reader (`crates/coverly-core/src/archive/`)

```text
open.rs      open_archive: magic → backend, multi-volume, external-tool choice
mod.rs       ArchiveReader trait, Entry, Format, ArchiveError, Limits, file_entries
detect.rs    detect_container, by_extension, multi-volume name patterns
guard.rs     guarded(): catch_unwind → Damaged; GuardedArchive / GuardedRead (S1)
names.rs     RelPath: zip-slip, absolute, `..`, device names, control chars, NFC
budget.rs    Budget (decoded bytes, wall time, cancel), MemoryGate (512 MiB global)
verify.rs    VerifyingReader: size + CRC32 at EOF, never yields the overrun byte
lzma.rs      LzmaParams / Lzma2Params / xz block walker: one memory formula (S2, S3)
zstd.rs      frame-by-frame zstd with a window check per frame
zip.rs       zip/zip64 over `zip`; LZMA, XZ, PPMd, zstd decoded by us from the raw entry
sevenz.rs    bounded, fail-closed 7z header walker (S4, S5) + `sevenz-rust2` for folders
tar.rs       our own 512-byte block walk (pax/GNU records capped), capped trailer (S6)
single.rs    gz, bz2, xz, zst, lz4, lzma → one virtual entry
external.rs  unrar / 7zz / 7z / bsdtar: discovery, process hygiene, strict listings (S7),
             name or verified stream reads (S8)
cache.rs     8 archives' listings + head bytes (≤ 4 MiB each, ≤ 32 MiB), never a decoder
testing.rs   (feature test-util) crafted headers, mutation engine, fake tools
```

`ArchiveReader` gives the validated listing (`entries()`), `open_entry(i, budget)` for heads and hashes, and `stream_entries(wanted, budget, sink)`, which decodes each 7z folder, RAR solid group or tar stream **once** in storage order (the extractor uses it). `listing_complete()` is false only when a listing was cut by its budget (a big `.tar.xz`, a gzip whose size is unknown): such an item is never extracted. `file_entries()` (files only, archive order) is the stable index dedupe phase B uses.

`Limits` (`archive/mod.rs`): 10,000 entries; 16 MiB per header, central directory or tool listing; 16 GiB per entry and 64 GiB per archive (declared); **256 MiB per decoder** and **512 MiB** of decoder memory across all planner threads (`MemoryGate`, one request per 7z folder for the sum of its coders); 64 MiB decoded per archive for heads at plan time, 256 MiB for compressed tar listings, 1 MiB tar trailer allowance, 20 s per archive at plan time. Apply has no byte or time budget.

Errors map into the plan (`plan::left_reason_for`): `Damaged` → `damaged-archive`; `Unsupported { format, need }` → `unsupported-archive` (*To open RAR archives, install unrar*); `Encrypted` → `encrypted-archive`; `NeedsTooMuchMemory` → `archive-needs-memory { mib }`; `TooManyEntries`/`TooLarge` → `archive-too-big`; `MultiVolume` → `multi-volume-archive`; `UnsafeEntry` → `unsafe-archive-entry` only when that entry is needed; `BudgetSpent` → **Review** with the note *Couldn't be read in full during the dry run*, never left. For dedupe, `ArchiveError` converts to `io::Error` (`InvalidData`, `Unsupported`, `OutOfMemory`, `FileTooLarge`, and `QuotaExceeded`/`TimedOut` for a spent budget, never `Interrupted`, which read loops retry; `archive::is_budget_spent` recognises it).

Crates (all permissive, pure Rust, checked by `scripts/check-licenses.sh`; see `THIRD-PARTY-NOTICES.md`): `zip` (features `deflate-flate2`, `deflate64`, `bzip2` only; `scripts/check-zip-features.sh` fails if anything turns on `lzma`/`xz`/`ppmd`/`zstd`), `sevenz-rust2`, `lzma-rust2`, `ppmd-rust`, `ruzstd`, `lz4_flex`, `tar`, `bzip2` + `libbz2-rs-sys`, `flate2` + `zlib-rs`. LZMA decoders are built only through `archive::lzma::open_decoder` (`clippy.toml` `disallowed-methods`), always single-threaded.

### RAR and external programs

The `unrar`/`unrar_sys` crates are **refused** (RARLAB's UnRAR licence is not free and cannot ship inside an AGPL-3.0-only binary; the licence script fails if they appear). RAR, and 7z coders `sevenz-rust2` lacks, open through a program already on the computer, run as a separate process: `unrar` (≥ 5) → `7zz` → `7z` → `bsdtar` (libarchive ≥ 3.6 for RAR5; macOS ships `/usr/bin/bsdtar`). Without one, the item is left as *To open RAR archives, install unrar* and the plan carries on. `settings.organize.external_tools` (`auto` | `off`; Settings → Organise → *External programs for RAR*, which lists what was found) and `coverly organize --no-external-tools` turn them off.

Discovery takes absolute `PATH` entries that are neither the current directory nor inside the source or target, plus fixed locations (`/opt/homebrew/bin`, `/usr/local/bin`, `/usr/bin`; `%ProgramFiles%\7-Zip`, `%ProgramFiles%\WinRAR`), and parses each version banner once. Every call (`run_tool`): an absolute path to a regular executable (`.bat`/`.cmd`/`.ps1` refused), no shell, `--` before the archive, an empty temp working directory, a **cleared environment** (fixed `PATH`, UTF-8 locale, `HOME` = that temp dir, `-cfg-` for unrar), null stdin, stdout and stderr drained on their own threads, a timeout that kills the whole process tree (Unix process group; Windows Job object, child created suspended). The tool that listed an archive is recorded in the plan (`ExtractSource::Tool { tool, version }`); apply uses the same one or skips the item (`tool-gone`). bsdtar output is never parsed as text: it transcodes the archive to a pax tar that our tar walker reads.

### What is inside

**Cartridges** follow the zip rules for **every** format: one ROM entry (readmes and OS metadata ignored) is identified by its stored CRC (zip, 7z, RAR; tar and compressors get a CRC from a bounded decode up to 64 MiB), its inner name and a 128 KiB header; several ROMs of one system are one item; entries of different systems or more than 8 ROMs → `multi-game-archive`; arcade sets by set name + entry CRCs (an arcade set in another container → `repack-arcade`, never extracted); an archive inside an archive → `nested-archive`.

**Disc archives** hold at least one `.cue`, `.gdi`, `.ccd`, `.iso`, `.img`, `.chd`, `.cdi`, `.pbp` or `.cso` (a lone `.m3u` does not count: Amiga and C64 floppies ship playlists). Then:

1. Sheets (`.cue`/`.gdi`/`.m3u` ≤ 64 KiB) are parsed with the `scan/multifile.rs` parsers when they are reachable within the head budget; references resolve inside the archive (case- and NFC-insensitive); absolute or `..` → `set-outside-folder`, missing → `set-incomplete`, a track of two discs → `shared-track`.
2. A sheet stored **last in a solid block** (7-Zip sorts by extension, so the `.cue`/`.gdi` come after 1.19 GB of tracks) is not decoded at plan time: the set is **inferred** from the listing by the Redump pattern `<stem> (Track NN).bin` (or `trackNN.bin` beside a `.gdi`), contiguous from 01, with nothing else in the sheet's folder. The item carries `SheetCheck::Deferred` and the note *The .cue sheet is checked while extracting*; the sheet is checked on apply before anything is committed. A listing that fits no pattern → Review, never Extract.
3. A `.cue` and a `.gdi` of the same tracks are one disc (Dreamcast dumps ship both; note *Has a .gdi and a .cue for the same tracks* in flat layouts). Discs of one game are one multi-disc item; discs of several games → `multi-game-archive`.
4. The primary data track is read into a window of at most 4 MiB through `open_entry` and identified by `disc::DiscReader` (below); the serial goes into `OrganizeItem.disc` (shown as `SLUS-00862`) and into dedupe's `DiscId`.

## Disc identification (`crates/coverly-core/src/disc/`)

`DiscReader::identify_with(src, image_len, fetch)` over a `SectorSource` (a loose image `FileSource`, or a decoded archive window `WindowSource`); the sector layout (2048, raw 2352 Mode 1/2, 2336) is detected from the sync pattern. Every read is bounded (PVD at LBA 16, root directory ≤ 64 records and 8 sectors, `SYSTEM.CNF` ≤ 4 KiB, IP.BIN = the track's first 32 KiB). Order: 3DO → Sega IP.BIN → PC Engine CD → PlayStation → PSP → Neo Geo CD.

| System | Signature | Serial | Confidence |
|---|---|---|---|
| PlayStation (`psx`) | PVD `PLAYSTATION` + root `SYSTEM.CNF` `BOOT = cdrom:\SLUS_008.62;1`; the Sony licence text in raw sector 4 is extra evidence only | from `BOOT`, `SLUS_008.62` → `SLUS-00862` | 0.95 with `BOOT`, 0.80 PVD only |
| PlayStation 2 | `SYSTEM.CNF` `BOOT2`, or a PlayStation volume larger than a CD | — | `unsupported-system` |
| Sega Saturn (`saturn`) | IP.BIN `SEGA SEGASATURN` | product at 0x20 (`T-5704H`) | 0.95 |
| Dreamcast (`dreamcast`) | IP.BIN `SEGA SEGAKATANA` on track 03 (the high-density area); a low-density track 01 never identifies | product at 0x40, `T40214N` → `T-40214N` | 0.95 |
| Sega CD (`segacd`) | `SEGADISCSYSTEM` | `GM …` at 0x180 | 0.95, never `megadrive` |
| PC Engine CD (`pcenginecd`) | `PC Engine CD-ROM SYSTEM` at 0x20 of sector 1 | — | 0.90 |
| Neo Geo CD (`neocd`) | `IPL.TXT` in the root, no `SYSTEM.CNF`, no Sega/NEC signature | — | 0.85 |
| 3DO | Opera volume header | — | `unsupported-system` |
| PSP (`psp`) | `PSP GAME` / `CISO` | `UMD_DATA.BIN` | 0.85 |

When `SYSTEM.CNF` lies past the 4 MiB window (the real case: LBA 2089, after a 4 MiB `PSX.BSE`), the reader asks `fetch` for exactly that file's extent, charged to the same 64 MiB budget, and finishes at 0.95. `DiscIdentity::to_dedupe_id()` gives dedupe's `DiscId` (serials, disc number, `VolumeFingerprint` = PVD size and dates + SHA-1 of `SYSTEM.CNF` or IP.BIN); serial normalisation lives in `disc::serial`.

## Extract (disc games and unloadable containers)

`plan::plan_transfer` gives every item a `Transfer`:

- `AsIs`: move or copy the file (loose ROMs, and cartridge archives the layout loads as they are).
- `Extract { entries, bytes, archive_after, source, sheet_check, m3u }`: a **disc archive** always (the whole set); a **cartridge** when the layout cannot load its container (`Profile::accepts_as_is(system, ContainerKind)`): all built-in profiles load zip and 7z as they are, except MinUI (zip only); `psp` never loads an archive; rar, tar.*, gz, bz2, xz, zst and lz4 are always extracted. Custom profiles set `accepts_containers` and `container_overrides` in their TOML.
- `archive_after` is `Trash` only in move mode **and** when every file entry is extracted or is OS junk (`.DS_Store`, `._*`, `Thumbs.db`, `desktop.ini`, `__MACOSX/`). A save, readme or second game inside forces `Keep` with the note *The archive stays: it has {count} files that are not extracted*. Copy mode always keeps the archive.

**Disc layout per profile** (`Profile.disc_layout`): `Flat` (Anbernic stock, GarlicOS, Onion, muOS; Batocera and RetroArch also generate `<title>.m3u` for multi-disc games): sheet and tracks straight into `PS/`. `GameFolder` (MinUI, ES-DE): `psx/<stem>/` with an `.m3u` named like the folder for multi-disc games. The archive's top folder is dropped; set members are **never renamed** (the sheets reference them): a name `safe_file_name` would change → `set-unsafe-name`; a path over the Windows limit (also for the longer temp name) → `path-too-long`; any collision with a file on disk, another member (`name_key`: NFC + lower case) or another item of the plan → `collision-set`. Generated `.m3u` files are UTF-8, `\n`, one relative sheet per line, never written into the source.

**Checks at plan time**: free space = Σ every extracted entry and `.m3u` rounded up to the target's cluster size, + 1 % + 64 MiB, against the space available **to the user** (`f_bavail`, Windows caller quota); a move on the same volume credits nothing for the archive (the Trash frees nothing). `PlanWarning::NoSpace { needed, free }` shows both numbers. FAT32: any extracted entry ≥ 4 GiB → `too-big-for-fat`; a FAT32 folder over 65,000 directory slots (`ceil(units/13) + 1` per long name) → `folder-full`. The dry run shows **Extract · 12 files · 405.7 MB** with the target folder and the sticky summary *Extracting: 3 games (2.19 GB)* (decimal units, as the CLI).

### Apply an Extract item (`organize/extract.rs`)

After the usual re-validation (archive size + mtime, the same external tool) and a **per-item free-space re-check**, an item is committed in four phases so that no final file appears until the whole item is known good:

| Phase | What happens | On failure |
|---|---|---|
| **A — decode to temps** | each target folder below the root must be a real directory (`symlink_metadata`; no symlink, junction or reparse point → `unsafe-target`); missing ones created one level at a time (`dir-created`); leftover job temps cleaned; one `stream_entries` pass; per entry an `extracted` **intent** line (no `fp`, synced), then a `create_new` + `O_NOFOLLOW` temp `.coverly-<job>-<n>.tmp` **in the target folder**, written through `VerifyingReader` (size + CRC vs the listing), `sync_all`, **re-read from the medium** (`F_NOCACHE` / `POSIX_FADV_DONTNEED`) and CRC-checked | disk full (`ENOSPC`, `EDQUOT`, Windows 112/39) → the item is rolled back and the **job pauses**; other errors fail the item |
| **B — validate the whole item** | the decode ended cleanly (gzip/xz/zstd/lz4 trailer within the S6 allowance, the tool's exit status 0); every sheet parses and every `FILE`/track line resolves to the extracted set, exactly or with the plan's case-insensitive + NFC fallback (`SheetCheck::Deferred` sets confirmed here); final names still free; archive unchanged since the plan | **rollback**: this item's temps deleted, a `rolled-back` line per intent, archive untouched |
| **C — commit** | each temp renamed with `rename_noclobber_indexed`, then the **completion** line (same `extracted` op with `fp` = size + mtime + CRC); the `.m3u` through the same temp → rename path (`created`) | a rename that fails rolls the item back (finals deleted after their `fp` check) |
| **D — the archive** | move mode with `ArchiveAfter::Trash` only: `archive-trash` intent with the archive's `fp`, re-check, `write::trash` sends it to the OS Trash, `archive-trashed { trash }` with the location the OS returned | no Trash on that volume, refused or changed → `archive-kept`; the item reports *Extracted; the archive stays…* (`ItemStatus::ExtractedKept`) |

Cancel is checked between entries and **every 16 MiB inside an entry**; cancelling in A or B rolls the item back and pauses. A solid 7z folder is decoded exactly once per item (single-threaded, about 30–80 MB/s); progress events (`phase: "extract"`, `bytes_done`/`bytes_total`) are byte-based. The journal is **append-only**: a later line supersedes an earlier intent.

**Trash** (`write/trash.rs`, `TrashBackend`, shared with dedupe phase B): macOS `NSFileManager trashItemAtURL:resultingItemURL:` (`objc2-foundation`), exact location, local writable volumes only; Windows `SHFileOperationW` with `FOF_ALLOWUNDO` on fixed drives only, location found afterwards in `$Recycle.Bin\<SID>\$I…`; Linux the freedesktop Trash spec written by us (`.trashinfo` before the rename). A removable card usually has no usable Trash, so its archive is kept.

**Resume**: an Extract item is done only when every entry has a completion line, its `.m3u` exists and (move + Trash) its last archive line is `archive-trashed` or `archive-kept`. Intents without completion → temps deleted, the item restarts at phase A. A crash in phase C → completed finals re-verified by `fp` and kept, finals matching an intent's size + full CRC adopted, the rest re-extracted (adopted entries are decoded to `io::sink` and CRC-checked, never rewritten). An `archive-trash` intent with no follow-up → trash again, or find it in the Trash by name + `fp`. An archive changed since the plan → finals of the item rolled back, `changed-since-plan`.

**Undo** (idempotent, per item, newest first): archives first, by their last line: `archive-trashed` → put back from the recorded location (or `TrashBackend::find` by name + `fp`) with `rename_noclobber`; if the archive cannot be restored (Trash emptied, `from` occupied by another file) the item's extracted files **stay**, because they are the only copy of the game, and `UndoReport.archive_lost` lists it (*The archive couldn't be recovered from the Trash; the extracted files stay*). Then extracted files: deleted when they match their `fp` (FAT 2 s and Windows 100 ns mtime tolerance, else CRC), modified ones kept (`skipped_modified`); an intent without completion is deleted only if its size and a full CRC re-read equal the listing; a rolled-back intent is never touched. A copy-mode extraction, or an `archive-kept` item, only deletes the extracted files.

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
| `extracted { from, entry, size, crc32, temp }` | intent (no `fp`) before the temp is created; completion (same op with `fp` = size + mtime + CRC) after the rename | completion matches `fp` → deleted; intent only → deleted if size + full CRC equal the listing; else kept |
| `rolled-back { temp }` | phase B/C failure: that intent was undone | the intent is never touched |
| `archive-trash { from }` | intent before the Trash call, `fp` = the archive's | resolved by searching the Trash by name + `fp` |
| `archive-trashed { from, trash }` | after the Trash call, with the location (or `null`) | put back first; not restorable → `archive_lost`, the extracted files stay |
| `archive-kept { from }` | the archive stayed (copy mode never writes it) | nothing |
| `created` (`.m3u`) | a generated playlist | deleted if unchanged |

```json
{"type":"entry","target":"/abs/card/ROMS/PS/Next Tetris, The (USA) (Track 01).bin","op":{"kind":"extracted","from":"/abs/ROMs-Downloaded/Next Tetris, The.zip","entry":"Next Tetris, The (USA)/Next Tetris, The (USA) (Track 01).bin","size":…,"crc32":…,"temp":".coverly-<job>-3.tmp"},"item":4}
{"type":"entry","target":"/abs/card/ROMS/PS/Next Tetris, The (USA) (Track 01).bin","op":{"kind":"extracted",…},"fp":{"size":…,"mtime_ns":…,"crc32":…},"item":4}
{"type":"entry","target":"/abs/ROMs-Downloaded/Next Tetris, The.zip","op":{"kind":"archive-trash","from":"/abs/ROMs-Downloaded/Next Tetris, The.zip"},"fp":{…},"item":4}
{"type":"entry","target":"/abs/ROMs-Downloaded/Next Tetris, The.zip","op":{"kind":"archive-trashed","from":"…","trash":"/Users/…/.Trash/Next Tetris, The.zip"},"item":4}
```

Builds older than the archive work read these ops as `Unknown` and refuse to undo the job. `prune_jobs_at` never removes a journal with an `archive-trash` intent that has no follow-up line.

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
                 [--offline] [--no-external-tools] [--dry-run | --apply] [--resume JOB_ID]
                 [--report table|json] [--output FILE] [--data-dir DIR] [--cache-dir DIR]
coverly undo [--job ID [--include-later]] [--list] [--data-dir DIR]
coverly systems                     # system ids for --assign
```

- Dry run by default; the table groups items by system with target folder, confidence and why, then *To review* and *Left in place*, and ends with "Dry run: nothing was written. Re-run with --apply to organise {n} files."
- An Extract item reads `extract 12 files 405.7 MB → ROMS/PS/`; the summary adds `Extracting: N games (bytes)` and the result `Extracted: N`. `--no-external-tools` plans, applies and resumes without unrar/7-Zip/bsdtar. `coverly undo` lists `archive restored:` and `archive lost:` lines.
- Review items are applied only with `--assign` (split on the last `=`, `FILE` relative to the source) or `--include-review` (best candidate). An `--assign` on an item that stays in place is reported, not dropped silently.
- `--resume` continues an interrupted job from its saved plan and refuses the plan options (`--layout`, `--target`, `--flat`, `--copy`/`--move`, `--names`, `--assign`, `--include-review`, `--apply`, `--dry-run`).
- Exit codes: 0 ok (also with items left in place), 1 I/O or job error (including not enough space), 2 usage (unknown layout or system id), 3 source not found or not a folder, 5 paused (prints the `--resume` and `coverly undo --job` commands). Exit 4 never applies: organise needs no detection.

## Hostile archives (S1–S8)

Archives come from the internet, so every reader assumes the bytes are crafted. Each rule has named tests (plan cross-check table).

| ID | Rule | Where |
|---|---|---|
| S1 | Every listing, prefix decode and `Read::read` of a returned stream runs inside `guard::guarded` (`catch_unwind`); a panic maps to `Damaged` and poisons that reader; the panic hook stays silent only inside the guard; the workspace keeps `panic = "unwind"` (compile-time assert). `garbled_archives_never_panic` mutates 21 fixtures of every format 2,000 times each (100,000 each, 2.1 M mutations, in the ignored long run: no panic, no hang) | `guard.rs`, `tests/archive_garbled.rs` |
| S2 | zip LZMA/XZ/PPMd/zstd entries: the dictionary, model or window size is read from the entry's own property header **at list time and again at open** and checked against the 256 MiB cap → `NeedsTooMuchMemory`; xz checks every block header, zstd every frame | `zip.rs`, `lzma.rs`, `zstd.rs` |
| S3 | The LZMA dictionary is never lowered with the attacker-declared unpacked size: the cap check and the decoder use the same raw dictionary value, through the one constructor `lzma::open_decoder` | `lzma.rs`, `clippy.toml` |
| S4 | 7z encoded header: exactly one folder of one LZMA/LZMA2 coder (+ BCJ/Delta), CRC required, unpack size ≤ 16 MiB, dictionary under the cap; the decoded result must be a plain `kHeader` (a nested encoded header → `Damaged`); entry-count caps apply to it | `sevenz.rs` |
| S5 | Fail closed: anything our bounded walker cannot fully validate (unknown property, count mismatch, trailing bytes, ranges outside the file) → `Damaged`; `sevenz-rust2` only ever parses an in-memory **snapshot** of the header we validated, and its view must equal our walk | `sevenz.rs` |
| S6 | tar inside gz/bz2/xz/zst: after the end-of-archive marker at most 1 MiB is drained, charged to the budget (`BudgetSpent` at plan time, `Damaged` on apply); pax and GNU long-name records ≤ 1 MiB, parsed with checked arithmetic | `tar.rs` |
| S7 | External listings (`7zz l -slt`, `unrar lt`): only the exact key set each tool prints per record, in order; record count and total size cross-checked against the tool's own summary; a name with control characters (a newline) makes the whole listing unusable; archive size + mtime unchanged across both runs | `external.rs`, `tests/archive_external.rs` (fake tool, plan AP-5) |
| S8 | unrar/7-Zip reads by name only for unambiguous names; a name with wildcards, a leading `-`/`@`, a case or NFC twin, another entry under `name/` (or ending in `/name`) is read from the full stream instead, cut by the listed sizes, and **every** slice size- and CRC-checked | `external.rs` |

Also: entry paths through `RelPath` (zip-slip, absolute and drive paths, UNC, `..`, Windows device names, control characters, components over 255 units/bytes); symlink, hardlink and device entries are listed but never read or extracted, and a path through a symlink of the same archive is unsafe; target folders that are symlinks or junctions → `unsafe-target`; temps `create_new` + `O_NOFOLLOW`; entry-count and total-size caps; zip bombs bounded by the budget and the verifying reader; Windows long-path checks on final and temp paths; Windows clippy runs with `-D warnings` on every change (`scripts/dev/clippy-windows.sh`).

## Tests

Synthetic fixtures only (no ROM data is committed): `identify/sniff.rs` unit tests (one positive and one near miss per signature, deterministic random-bytes guard), and the integration suites `crates/coverly-core/tests/organize_{inspect,walk,evidence,target,plan,apply,undo}.rs`, `crates/coverly-cli/tests/cli_organize.rs`, the IPC contract (`src-tauri/tests/contract.rs`, `src/ipc/contract.test.ts`) and the Vitest suites of `organizeStore`, `OrganizeScreen`, `StartScreen` and `src/components/organize/`. Archives and discs: `tests/archive_{detect,zip,sevenz,single,tar,external,garbled,dedupe_contract}.rs` (crafted headers built in `archive::testing`, encoders from dev-dependencies, the scripted `coverly-fake-tool` for 7zz/unrar/bsdtar), `tests/disc_identify.rs` (`disc::testing::IsoBuilder`), `tests/organize_extract_{apply,resume,undo}.rs` (`FakeTrash`, disk-full and crash points) and `tests/organize_real_sample.rs` (`#[ignore]`, plan only, read-only on `/Volumes/TinyExt/ROMs-Downloaded` or `COVERLY_SAMPLE_DIR`; skipped when the volume is not mounted). The read-only runs against the owner's real sample and the E2E on the mock are recorded in the [tracker](../TRACKER.md) (O10, A35, A44).
