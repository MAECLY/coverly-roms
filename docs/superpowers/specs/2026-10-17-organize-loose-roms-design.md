# Coverly ROMs — Organise Loose ROMs (Design Spec)

> Status: **Draft for review** · Date: 2026-10-17 · Owner: Miguel Angel Esparza Calero (MAECLY)
> Extends the master spec [`2026-10-15-coverly-roms-design.md`](2026-10-15-coverly-roms-design.md) (§4 listed "ROM renaming/verification" as out of v1; this spec brings in the *organising* part only).
> Branch: `feat/organize-loose-roms`. Working tracker: [`docs/TRACKER.md`](../../TRACKER.md).

## 1. Purpose

Today, picking a plain folder of ROMs is a dead end:

> "No se reconoce ningún formato en …/Documents/ROMs-Downloaded. Elige la raíz de la SD o su carpeta de ROMs."

Many users do not have an SD card layout or a frontend tree. They have a **pile**: a downloads folder with zips, browser duplicates (`Tetris 2 (1).zip`), half-finished downloads, a stray `.DS_Store`. Coverly must be able to:

1. **Identify each game** in that pile and **which system** it belongs to, with a confidence and a plain reason ("why").
2. **Propose** a folder organisation by system that follows a layout profile — by default the owner's reference layout (Anbernic stock: `ROMS/<SYS>/`, e.g. `ROMS/GBA/`, `ROMS/SFC/`, art later in `ROMS/<SYS>/Imgs/<stem>.png`).
3. **Apply** it safely (dry run first, move or copy, never overwrite, journal, undo).
4. **Open the result as a normal library**, so scan and "fill covers" work unchanged.

### What the user said vs. what we assume

| Said by the user | Assumed (confirm in review) |
|---|---|
| Users won't always have a vendor/organised folder; it can be a custom folder full of ROMs | Only **files at the top of the picked folder and in non-system subfolders** are candidates; folders that already resolve to a system are left alone |
| Identify each game, then its system | Ladder: hash → header magic → extension → name; confidence + "why" per item |
| Propose a folder system like `~/Documents/ROMS` | Default layout = `anbernic-stock`, system folders named like the reference (`GBA`, `GB`, `GBC`, `SFC`, `FC`, `MD`…) |
| Any other system via configuration, but organisation by system is preferred | Every built-in profile can be the target; per-profile folder naming rule + user overrides |
| Then apply our app, covers, etc. | After apply, open the target with the chosen profile explicitly (no detection needed) |

## 2. Goals and non-goals

### Goals
- Turn the `no-profile` dead end into an offer: **"Organise this folder…"**, when the folder holds ROM-like files.
- Read-only **planner** in `coverly-core` (pure apart from reads), shared by app and CLI.
- Identification ladder with explicit evidence, confidence and a review list for doubtful/unknown items.
- Configurable target: layout profile, target root (in place by default), nesting (`<root>/ROMS/<SYS>` or `<root>/<SYS>`), move/copy, naming.
- Safe apply: never overwrite, never write outside the roots, streamed copies without xattrs, journal + undo, pause on volume loss, resumable.
- Multi-file games (`.cue/.bin`, `.m3u`, `.gdi`) and same-stem companions (`.sav`, `.srm`) move together.
- Works on case-insensitive file systems (APFS default, exFAT/FAT32 SD cards, NTFS).
- After organising, the normal flow (scan → audit → fill) works on the result with no extra steps.

### Non-goals (this iteration)
- **Extracting** archives or recompressing. Zips stay zips (the reference layout keeps zipped NDS/arcade sets).
- **Verifying** or repairing dumps, merging/splitting arcade sets, 1G1R curation, deleting duplicates.
- Reading **inside 7z** archives (no 7z crate in the workspace; see §7.2).
- Re-organising a library that is already in a known layout (profile-to-profile migration). Possible later on top of the same planner.
- Folder-based games (DOS folders, PS3/ScummVM directories) beyond reporting them.
- Online identification by default. Optional sources only when the user already configured them (§6.5).

## 3. Users and success

- **Handheld owner with a downloads folder** (primary): picks the folder, sees "16 ROMs in 4 systems, 1 to review, 4 left in place", clicks once, gets `ROMS/GBA`, `ROMS/GB`, `ROMS/GBC`, `ROMS/SFC`, lands in the library screen, fills covers.
- **Curator** moving a pile into an existing SD card: picks the pile, chooses the SD card as target, copies.
- **Scripter**: `coverly organize ~/Downloads/roms --apply`.

Success = on the owner's real sample (`~/Documents/ROMs-Downloaded`) the dry run proposes **GBA ×6, GB ×6, GBC ×2, SFC ×2**, flags `3-D Tetris (USA).zip` as *not a ROM (web page)*, skips partial downloads and `.DS_Store`, and asks **nothing** — every valid zip is settled by its inner entry (CRC or header).

## 4. User flow (app)

### 4.1 Entry point
1. `StartScreen.pick()` → `libraryStore.open(path)` → `open_library` → `choose_profile` → `detect()` is empty → `IpcError::NoProfile` (code `no-profile`, stable, unchanged).
2. The start screen, on `no-profile`, calls a new cheap probe `probe_folder(path)` (shallow, ≤ 2 levels, ≤ 5 000 entries, no hashing): counts ROM-like files (known ROM extensions, archives, disc images), partial downloads, folders that already resolve to systems.
3. If `rom_like > 0`, the error banner is replaced by an **offer panel**:
   - Title: "Esta carpeta no sigue ningún formato conocido" / "This folder doesn't follow a known layout".
   - Body with counts from the probe.
   - Buttons: **Organizar esta carpeta…** (primary) · **Elegir otra carpeta** (ghost).
   - If `rom_like == 0`, the existing error text stays (it is still correct).

### 4.2 Proposal screen (`navStore.screen = 'organize'`)
A single screen, built with existing classes (`section-title`, `hint`, `primary-button`, `ghost-button`, `error-banner`, plain `<select>`, `<details>`, `<ul>`). Markup stays simple so it merges with the parallel `src/components/ui` primitives branch.

1. **Options bar** (changing any option re-plans; identification is cached so re-planning is instant):
   - *Formato* / *Layout*: built-in profiles (`anbernic-stock` default, then Garlic, Onion, muOS, MinUI, ES-DE, Batocera, RetroArch) and custom profiles. Default from `settings.organize.default_layout`, else `settings.library.default_profile`, else `anbernic-stock`.
   - *Destino* / *Target*: "This folder" (default) or "Choose another folder…" (e.g. the SD card root).
   - *Carpetas* / *Folders*: "Inside `ROMS/`" (profile's `roms_root`, default) or "Directly here" (`roms_root = ""`, what `fit_to_root` already supports). Hidden for profiles whose `roms_root` is empty.
   - *Modo* / *Mode*: Move (default when source and target are on the same volume) or Copy (default otherwise).
   - *Nombres* / *Names*: "Keep file names" (default) or "Use the catalogue name" (No-Intro/DAT name, else inner zip name; removes browser ` (1)` suffixes).
2. **Summary line**, exact counts: "16 ROMs → 4 systems · 0 to review · 4 left in place".
3. **Grouped by system** (`<details open>` per system, target folder shown: `ROMS/GBA/`): each row shows source name → target relative path, a confidence label (Alta/High ≥ 0.95, Media/Medium ≥ 0.85, — for review), and the "why" in one line ("CRC 7B729804 in the Game Boy Advance catalogue: *Tetris Worlds (USA)*").
4. **To review** (only if any): each item shows its evidence and a `<select>` with the candidate systems (ordered by score) plus "Dejar donde está / Leave in place". Choosing a system moves the item into its group with `method = user`.
5. **Left in place**, with a reason per file: partial download, not a ROM (web page), corrupt archive, unknown, duplicate of …, already in a system folder, symlink.
6. **Actions**: "Organizar {count} archivos / Organise {count} files" (primary; count = auto + user-assigned), "Exportar plan (JSON)", "Cancelar".

The proposal **is** the dry run: nothing is written until the primary action.

### 4.3 Apply, result, open, undo
- Apply runs as a job (progress event `organize-progress`, cancel button, pause on volume loss/disk full/denied with Resume, like fill jobs). It takes the `JobManager` manual guard so it never races a fill job.
- Result panel: "Movidos: 16 · Omitidos: 0 · Errores: 0", plus the per-item list on demand and **Abrir biblioteca / Open library** (primary) and **Deshacer / Undo** (ghost).
- *Open library*: `libraryStore.open(targetRoot, profileId)` — `open(path, profileId?)` gains the optional argument the IPC already supports — and the recent-libraries entry stores `profile_id`, so reopening later also skips detection.
- *Undo*: calls `undo_job(job_id)` through a new `organize_undo` command (the generic in-library undo filters by session root; the organise screen undoes its own job explicitly). Undo is also available later from the library's job history and `coverly undo`.

### 4.4 Detection fallback (reopening an organised folder from scratch)
A freshly organised `ROMS/GBA/` has no `anbernic/` and no `Imgs/` yet, so marker detection still finds nothing. Two measures:
1. Opening with an explicit `profile_id` (above) — primary fix.
2. New weak rule in `profiles/detect.rs`: if the root (or a `roms`/`ROMS`/`Roms`/`ROMs` child) contains **≥ 2 child folders that resolve to known systems and hold ROM files**, and no marker rule scored, return one low-score detection for `settings.organize.default_layout` (default `anbernic-stock`) with reason `system-folders`. The UI shows it as a suggestion ("Parece una carpeta organizada por sistema") rather than a certainty. Existing detection tests must keep their results (the rule only fires when nothing else did).

## 5. CLI

```
coverly organize <SOURCE>
    [--layout <PROFILE_ID>]        default: settings.organize.default_layout | anbernic-stock
    [--target <DIR>]               default: <SOURCE> (in place)
    [--flat]                       system folders directly in <DIR> (roms_root = "")
    [--copy | --move]              default: move on same volume, copy across volumes
    [--names keep|catalogue]       default: keep
    [--assign <FILE>=<SYSTEM_ID>]… settle review items (repeatable)
    [--include-review]             apply review items with their best candidate
    [--offline]                    no DAT downloads, cached DATs only
    [--dry-run | --apply]          default: dry run (honours safety.dry_run_default)
    [--report table|json] [--output <FILE>] [--data-dir <DIR>] [--cache-dir <DIR>]
coverly undo [--job <ID>]          unchanged; understands organise journals
```

- Follows `fill.rs`: `check_root` (exit 3), `std::path::absolute` (not canonicalize), JSON `{"mode","plan","job"}`, table ending in "Dry run: nothing was written. Re-run with --apply…".
- Review items are **never** applied without `--assign` or `--include-review`; they stay in place and are listed. Exit codes unchanged: 0 ok (even with items left in place), 1 I/O/job error, 2 usage (unknown profile/system id in `--assign`), 3 source not found, 5 paused. Exit 4 (no profile) does not apply: organise never needs detection.
- Table output groups by system with target folder, confidence and why, then "To review" and "Left in place".

## 6. Identification

### 6.1 Pipeline per candidate file

```
walk → classify (skip / not-a-rom / rom-like) → open (raw | zip inner entry | 7z name-only | disc set)
     → collect evidence: [hash DAT hits] [header magic] [extension] [name signals]
     → combine → { system, confidence, why[], alternatives[] } → Auto | Review | LeftInPlace
```

Every piece of evidence is recorded (`Evidence { kind, system, weight, detail }`); the final "why" is the strongest one, the rest are shown on demand. Results are memoised in the existing `quick_ids` cache key style (path + size + mtime) under a new `organize_ids` table, so re-planning after an option change does no I/O beyond `stat`.

### 6.2 Level 1 — hash against DATs (confidence 1.00)
- **Inputs**: raw files hashed with `hash_rom` (CRC32/MD5/SHA1, full + headerless, 64 MiB cap); single-entry zips use the central-directory CRC32 + size (no decompression needed for this level).
- **Which DATs**: candidate systems come from levels 2–3 (extension and header), so we only consult DATs that can match. For each candidate system, `dat_sources(system)`; DATs already cached are used directly; missing DATs are downloaded through `DatStore::ensure` unless `--offline` / `settings.organize.download_dats = false`, capped at 4 DAT downloads per plan to avoid fetching the whole catalogue for one pile.
- **Unknown-system fallback**: when levels 2–3 give no candidate (e.g. a `.bin` with no recognisable header, or a multi-system zip extension), query **all cached DATs** by CRC: new `Cache::dat_by_crc_any(crc32, size) -> Vec<(dat, DatRow)>` backed by a new index on `dat_roms(crc32)` (cache schema v2, additive migration).
- **DAT → system**: reverse map built from `dat_sources` over `SystemDb::all()` (`SystemDb::system_for_dat(path)`).
- **Weights**: hit in a DAT of a candidate system = 1.00; hit outside the candidates = 0.97 (note "the extension pointed elsewhere"); hits in several systems' DATs (e.g. the same CRC in GB and GBC catalogues) → keep those consistent with header evidence; if still more than one → Review with all of them as candidates.
- **Arcade zips** (multi-entry): the zip **stem** is the set name; look it up by `rom_stem`/game name in cached arcade DATs (FBNeo, MAME). Hit = 0.90 (set names are short and can clash), several arcade systems → Review.

### 6.3 Level 2 — header magic (confidence 0.85–0.95)
New module `crates/coverly-core/src/identify/sniff.rs`: `sniff(head: &[u8], len: u64, ext: &str) -> Vec<Evidence>`. Reads a bounded prefix: 128 KiB for raw files and for the first bytes of a zip's inner entry (`zip` feature `deflate-flate2`, already a workspace dep, read with `by_index` + `take(131072)`; never fully decompressed, which also defuses zip bombs). PS1 checks read sector 16 only.

| System (`id`) | Signature | Strength |
|---|---|---|
| `nes` | `4E 45 53 1A` at 0 (iNES/NES 2.0) | 0.95 |
| `fds` | `FDS\x1A` at 0, or `\x01*NINTENDO-HVC*` at 0 / 16 | 0.95 |
| `gb` / `gbc` | logo prefix `CE ED 66 66 CC 0D 00 0B` at 0x104 **and** header checksum at 0x14D (`x = 0; for b in [0x134..0x14D) x = x - b - 1`) valid; CGB flag 0x143: `0xC0` → gbc 0.95 · `0x80` (dual) → gbc 0.85 (No-Intro and Anbernic file dual-mode carts under GBC; `.gb` extension adds gb as alternative) · otherwise → gb 0.95 | 0.85–0.95 |
| `gba` | fixed byte `0x96` at 0xB2 **and** complement check at 0xBD (`-(sum[0xA0..0xBC]) - 0x19`) valid; logo prefix `24 FF AE 51` at 0x04 adds to the reason. Do **not** key on the ARM branch at 0 (varies, e.g. `7F 00 00 EA`) | 0.95 |
| `nds` | logo CRC16 `0xCF56` at 0x15C **and** header CRC16 at 0x15E over 0x000–0x15D | 0.95 |
| `n64` | first 4 bytes `80 37 12 40` (z64), `37 80 40 12` (v64), `40 12 37 80` (n64) | 0.95 |
| `megadrive` | `SEGA` at 0x100 (`SEGA MEGA DRIVE`, `SEGA GENESIS`); interleaved SMD: 512-byte header with `0xAA 0xBB` at 8–9 | 0.90 |
| `sega32x` | `SEGA 32X` at 0x100 | 0.95 |
| `mastersystem` / `gamegear` | `TMR SEGA` at 0x7FF0, 0x3FF0 or 0x1FF0; region nibble at +0xF high bits: 3/4 → SMS, 5/6/7 → GG | 0.90 |
| `snes` | internal header at 0x7FC0 (LoROM) / 0xFFC0 (HiROM) / 0x40FFC0 (ExHiROM), +0x200 if `len % 1024 == 512` (copier header); valid when checksum (+0x1E) XOR complement (+0x1C) == 0xFFFF and map mode (+0x15) ∈ {0x20,0x21,0x23,0x25,0x30,0x31,0x32,0x35} | 0.95 (0.85 if only map mode + printable title) |
| `lynx` | `LYNX` at 0 | 0.95 |
| `a7800` | `ATARI7800` at 1 | 0.95 |
| `ngp` / `ngpc` | `COPYRIGHT BY SNK CORPORATION` or ` LICENSED BY SNK CORPORATION` at 0; byte 0x23 == 0x10 → ngpc, else ngp | 0.90 |
| `wonderswan` / `wonderswancolor` | 10-byte footer: byte `len-10` == 0xEA (jump); `len-9` bit 0 = colour | 0.85 |
| `psx` | ISO 9660 `CD001` at sector 16 (offset 0x8001 for 2048-byte, 0x9319 for raw 2352-byte `.bin`) **and** `PLAYSTATION` at system-area offset 0x8008 / raw equivalent, or `BOOT = cdrom:` in `SYSTEM.CNF` if trivially reachable | 0.90 |
| `segacd` / `saturn` / `dreamcast` | `SEGADISCSYSTEM` / `SEGA SEGASATURN` / `SEGA SEGAKATANA` at the start of the first data track | 0.90 |
| `psp` | `CD001` + `PSP GAME` volume id; `.cso` magic `CISO` | 0.85 |
| `pcengine`, `a2600`, `a5200`, `msx`, others | no reliable magic → levels 1, 3, 4 only | — |

Negative evidence: when a system with a signature is the **only** extension candidate and the sniff fails decisively (e.g. a `.gba` without `0x96` at 0xB2; a `.md` that is UTF-8 text), the extension weight drops to 0.60 → Review with reason "the extension says X but the contents don't look like X" (homebrew and hacks land here, which is correct).

Text and web pages: any candidate whose first 512 bytes are printable text starting with `<!DOCTYPE`, `<html`, `<?xml`, `{` or similar is **not a ROM** (`not-a-rom: web-page|text`), regardless of extension.

### 6.4 Level 3 — extension (confidence 0.85 unique · 0.50 shared)
- New `SystemDb::systems_for_extension(ext) -> Vec<&System>` (lowercased, cached map).
- **Unique** extensions (`gba`, `gbc`, `nes`, `sfc`, `smc`, `n64`, `z64`, `v64`, `nds`, `gg`, `sms`, `pce`, `32x`, `a26`, `lnx`, `vb`, `ngp`, `ws`, `p8`, `cdi`…) → 0.85.
- **Shared** extensions (`gb`, `bin`, `iso`, `cue`, `chd`, `ccd`, `pbp`, `wsc`, `ngc`, `d64`/`prg`/`t64`/`tap`/`crt`, `cas`, `com`, `ipf`, `m3u`) → each candidate 0.50 → needs level 1/2 to reach Auto.
- `zip` and `7z` are **containers**, never evidence by themselves; the inner entry's extension is used instead.
- The scan's `ignored_exts` are **not** applied before identification (`md` is a Mega Drive extension and also in the ignored list).

### 6.5 Level 4 — name signals (confidence ≤ 0.80)
- Outer and inner names parsed with `parse_name` (+ `strip_numbering_prefix` for scene numbers like `1490 - `).
- Fuzzy title match (`names::score`) against `rom_stem` of cached DATs **of the candidate systems only** (new `Cache::dat_stems(dat)` reuse of the `(dat, rom_stem)` index): score ≥ 95 → 0.80, ≥ 85 → 0.70.
- Tags: `(SGB Enhanced)` → gb/gbc hint, `(GB Compatible)` → gbc hint, `(Disc N)` → disc systems, GBA scene groups (`Venom`, `Paracox`, `BatMan`, `Independent`, `Rising Sun`…) → gba hint 0.60. Hints alone never reach Auto.
- Optional online lookup (ScreenScraper `jeuInfos` by CRC/name) only if the user has configured that source and is not offline; capped at 0.80.

### 6.6 Combining evidence
- Per system: `score = max(weights)` plus a small agreement bonus (+0.03 per independent agreeing level, max 1.00). Conflicting levels subtract nothing but are listed.
- `best` = highest score; `margin` = best − second.
- **Auto** when `best ≥ auto_threshold` (default **0.85**) and `margin ≥ 0.10`.
- **Review** when `best ≥ 0.50` or the margin is too small; candidates = systems ≥ 0.40, ordered.
- **Left in place: unknown** when nothing reaches 0.50.
- Thresholds live in `settings.organize` (`auto_threshold`, `review_threshold`), separate from the art-matching thresholds.
- A system chosen by the user is `method = user`, confidence 1.0, and is remembered by content (CRC32 + size) in a new `organize_choices` cache table, so the same file is not asked about twice.

### 6.7 Expected results on the real sample
| Files | Deciding evidence | Result |
|---|---|---|
| 6 × inner `.gba` (incl. `Fire Emblem (E)(Independent).zip` → `1575 - Fire Emblem….gba`) | CRC hit in GBA DAT (cached) + header 0x96/0xBD | `ROMS/GBA/`, 1.00 |
| 6 × inner `.gb` (`Tetris (JUE) (V1.0) [!]`, `Tetris 2 (1)`, `Tetris Attack (1)`…) | CRC hit in GB DAT + CGB flag 0x00 | `ROMS/GB/`, 1.00 |
| `Magical Tetris Challenge (USA).zip`, `Tetris DX.zip` | GBC DAT not cached → downloaded on demand (online) or header CGB 0xC0 / 0x80 + `.gbc` (offline, 0.95 / 0.88) | `ROMS/GBC/` |
| `Tetris 2.zip`, `Tetris Attack.zip` (inner `.sfc`) | CRC hit in SNES DAT + LoROM checksum | `ROMS/SFC/`, 1.00 |
| `3-D Tetris (USA).zip` (HTML, 5 900 B) | not a zip, text starts with `<!DOCTYPE` | left in place: *not a ROM (web page)* |
| `*.crdownload` | partial download | left in place: *download not finished* |
| `.DS_Store` | system file | ignored silently (counted, not listed) |

Same title, different systems (`Tetris 2.zip` = SNES, `Tetris 2 (1).zip` = GB) end in different folders: no collision.

## 7. Files, archives, sets

### 7.1 Walking and classification
- Walk the source with `walkdir`, **no symlink following** (symlinks are reported and left), max depth 4, ignoring `ignored_dirs` (`bios`, `saves`, `states`, `Shortcuts`), hidden dirs, and the target tree if it is inside the source.
- A subfolder that already resolves to a system (`resolve_folder`, case-insensitive) is **not** walked: reported once as "already in a system folder (`GBA/`)". If its name is not the chosen layout's name for that system (e.g. `gba` vs `GBA`), the plan does **not** rename it; on case-insensitive volumes it simply is the target folder (see §9).
- Skips (never moved, counted):
  - junk: `JUNK_FILES`, AppleDouble `._*`, dot-files (`classify::is_skipped_entry`).
  - **partial downloads** (new list in `classify.rs`, reused by scan): `.crdownload`, `.part`, `.partial`, `.download`, `.opdownload`, `.tmp`, `.!ut`, `.!qb`, `.aria2` sidecars, Safari `.download` bundles (directories), and zero-byte files.
  - non-ROM documents/images (`txt`, `nfo`, `pdf`, `jpg`, `png`…) unless a ROM with the same stem moves (then they are *companions*, see 7.4) — otherwise left in place silently.

### 7.2 Archives
| Case | Detection | Handling |
|---|---|---|
| Valid zip, 1 entry | `PK\x03\x04` / `PK\x05\x06` magic + `ZipArchive::new` OK | identify by inner entry (CRC, inner name, 128 KiB header read) |
| Valid zip, many entries, one ROM-like entry + readme/nfo | inner exts | identify by that entry |
| Valid zip, many ROM-like entries | arcade-shaped (no unique console ext) | arcade lookup by set name (6.2); else Review |
| Zip with a directory inside | allowed | as above, by entry names |
| `.zip` that is not a zip | magic mismatch / `ZipArchive::new` fails | text → *not a ROM (web page)*; else *damaged archive*; left in place |
| Encrypted or unsupported compression | `zip` error on read | CRC level still works (central directory); header level skipped; reason noted |
| `.7z` | magic `37 7A BC AF 27 1C` | **name and extension only** (no 7z crate): inner listing not read; best case Review via name signals, unless the name hits a DAT stem ≥ 95 (0.80 → Review, never Auto). Bad magic → *damaged archive*. A later iteration may add `sevenz-rust` for listings |
| `.rar` and others | — | left in place: *unsupported archive* |

Archives are **never extracted** and keep their name (unless *Names: catalogue*, which renames the `.zip` itself to `<catalogue stem>.zip`).

### 7.3 Multi-file games
- Disc sets are grouped with `scan::multifile::group_sets` (`.cue` → its `.bin`/`.wav` tracks, `.gdi` → tracks, `.m3u` → its discs and their closures). The planner makes **one plan item per set**; the set's system is identified from the primary data track (first `BINARY`/`MODE` track of the cue, track 03 of a gdi).
- All files of a set move together into `<SYS>/`, keeping their names and relative layout (a cue referencing `Disc 1/track.bin` keeps `Disc 1/`).
- If any member collides, is missing or is a partial download, the **whole set** is left in place with the reason — never renamed piecemeal (the cue text references names).
- `.m3u` playlists are moved with their discs; `.cue` files referenced by an m3u are not separate items.

### 7.4 Companions
Files sharing the exact stem of a moved ROM with an extension in `settings.organize.companion_exts` (default: `sav`, `srm`, `rtc`, `state*`, `cht`, `ips`, `bps`, `ups`, `txt`, `nfo`, `png`, `jpg`) follow the ROM into the same folder (the reference layout keeps `.sav`/`.srm` next to ROMs). Image companions are **not** moved into `Imgs/`; that is the fill flow's job.

### 7.5 Duplicates
- Same content (CRC32 + size, or SHA1 when computed) twice in the pile: the first (shortest name, then alphabetical) is planned; the others are left in place as *duplicate of `<name>`*. Nothing is deleted.
- Same content already present in the target system folder: left in place as *already in `ROMS/GB/` as `<name>`*.
- Browser suffixes (` (1)`, ` (2)`, ` - Copy`, ` copia`) are **not** treated as duplicates by name; only content decides.

## 8. Target layout

### 8.1 Folder naming per profile
New field on `Profile`: `system_folder: FolderNaming`, plus a user map.

| Profile | Rule | GBA | SNES | NES | Mega Drive | PS1 |
|---|---|---|---|---|---|---|
| anbernic-stock, garlicos, onion | `FirstAlias` | `GBA` | `SFC` | `FC` | `MD` | `PS` |
| minui | `FirstAlias` (MinUI tag folders like `Game Boy Advance (GBA)` via user map) | `GBA` | `SFC` | `FC` | `MD` | `PS` |
| esde, batocera, muos, retroarch | `Id` | `gba` | `snes` | `nes` | `megadrive` | `psx` |

- `FirstAlias` = `system.aliases[0]` (already the Anbernic SD name in `systems.toml`, matching `~/Documents/ROMS`).
- `settings.organize.folder_names: { "<profile_id>": { "<system_id>": "<folder>" } }` overrides any entry; custom profiles can declare `system_folder` and a map in their TOML.
- Invariant (tested for every built-in profile × system): the produced folder name **resolves back** to the same system via `SystemDb::resolve_folder`, otherwise the scan would not see it. A user override that does not resolve is rejected with a clear message.
- Systems without an Anbernic folder (`ngpc`, `wonderswancolor`, `pico8`…) use their first alias; the plan shows a hint when the chosen layout has no known folder for that system on the device.

### 8.2 Target paths
- Target root `T` = picked folder (default) or chosen folder. ROMs root `R = T/<profile.roms_root>` (or `T` with *Directly here*). Item target: `R/<folder>/<file name>`.
- If `R/<folder>` already exists under any casing (`find_dir_ignoring_case`), that folder is used as is.
- Names pass `safe_file_name`; a name that would change is planned with the safe name and the reason ("renamed for FAT/exFAT: `:` → `_`").
- No art folders are created by organise; the profile's art layout is created by the fill flow as usual.

### 8.3 Opening afterwards
`open_library(T, profile_id)`; `fit_to_root` handles *Directly here* (`roms_root = ""`). The organise job's journal root is `T`, so the in-library undo (which filters by session root) also sees it.

## 9. Safety and file system rules

- **Dry run first**: planning is read-only. Apply takes the plan plus decisions; every item is re-validated before acting (source still exists with the same size + mtime, else *changed since the plan*, skipped).
- **Never overwrite.** Target exists (case-insensitive + NFC twin via `find_twin`, made `pub(crate)`):
  - same content → *already present*, skipped;
  - different content, single file → rename to `<stem> (2).<ext>` (next free index, also twin-checked), reason recorded;
  - different content, multi-file set → whole set skipped.
  Collisions **inside the plan** (two sources → same target) are resolved at plan time with the same rule, so the dry run shows final names.
- **Roots**: every target passes `ensure_within(T)`, every source `ensure_within(S)` (S = source root); symlinked parents refused.
- **Move**: `fs::rename` when source and target share a volume (`st_dev` on Unix, volume serial on Windows). Cross-volume move = streamed copy → verify (size + CRC32 computed while copying) → `fsync` → delete source. `EXDEV` (errno 18) / Windows `ERROR_NOT_SAME_DEVICE` (17) from `rename` falls back to this path; `map_io` gains this mapping.
- **Copy**: streamed (`File::open` → `create_new` temp `.coverly-<job>-<n>.tmp` in the target dir → `io::copy` with a 1 MiB buffer → `sync_all` → `rename`). **Never `fs::copy`** (copies xattrs → `._*` files on FAT). Quarantine/provenance xattrs of the source are not carried over. mtime is preserved with `File::set_modified` so dates in the by-date view stay meaningful.
- **Free space**: before a copy (or cross-volume move), sum of sizes + 1 % must be free on the target volume, else apply refuses to start with exact numbers.
- **Removable volumes**: AppleDouble sidecars of created folders/files are removed, as in `Writer::write`.
- **Case-insensitive volumes**: all name comparisons use NFC + lowercase; never create `GBA/` next to `gba/`; a rename whose only change is case is a no-op (not attempted).
- **Never write** to `/Volumes/System`, the source in Copy mode, or anything outside `S` ∪ `T`.
- **Manual overrides**: `identify::overrides` entries keyed by a moved path are rewritten to the new path (best effort, logged).
- **Concurrency**: one worker per volume pair (moves are metadata ops; copies are I/O-bound and contend on SD cards). Cancel between items.

## 10. Journal format and undo

Organise reuses `write::Writer` and its JSON-lines journal (`<data>/jobs/<job>.jsonl`), extended compatibly:

```jsonc
{"type":"header","job":"20261017T101500Z-4242-1","root":"/Users/…/ROMs-Downloaded",
 "source_root":"/Users/…/ROMs-Downloaded","kind":"organize","created_at":"…"}
{"type":"entry","target":"ROMS","op":{"kind":"dir-created"}}
{"type":"entry","target":"ROMS/GBA","op":{"kind":"dir-created"}}
{"type":"entry","target":"ROMS/GBA/Tetris Worlds (USA).zip",
 "op":{"kind":"moved","from":"Tetris Worlds (USA).zip","via":"rename"},
 "fp":{"size":1873412,"mtime_ns":1760…,"crc32":"…"}}
{"type":"entry","target":"ROMS/SFC/Tetris 2.zip",
 "op":{"kind":"copied","from":"Tetris 2.zip"},"fp":{…}}
{"type":"done"}
```

- `Header` gains optional `source_root` (defaults to `root`) and `kind` (`fill` | `organize`, default `fill`); older journals parse unchanged.
- `Entry.sha1` becomes `Option<String>` (`#[serde(default)]`); new optional `fp: Fingerprint { size, mtime_ns, crc32? }` — undo of multi-GB files never re-reads whole files.
- New ops: `Moved { from, via: rename|copy }`, `Copied { from }`, `DirCreated`. `from` is relative to `source_root`.
- **Write order**: the entry is appended and flushed **before** the action (as `remove` does). Undo therefore tolerates "target missing, source present" as a no-op.
- New `Writer` methods: `move_file(from, to, collision) -> MoveOutcome`, `copy_file(from, to, collision) -> MoveOutcome`, `ensure_dir(dir)` (journals `DirCreated` only for folders it created).
- **Undo** (reverse order, inside `undo_journal`):
  - `Moved`: target exists with matching `fp` (size + mtime; CRC only if mtime differs) and `from` is free → rename back (stream back if `via: copy` / different volume), recreating `from`'s parent. Target missing → nothing. Target modified → `skipped_modified`. `from` occupied → skipped with reason.
  - `Copied`: target matches `fp` → delete it; else `skipped_modified`. Source is untouched.
  - `DirCreated`: remove only if empty (ignoring `.DS_Store`/`._*` that macOS may have added).
- `undo_last`, `undo_job`, `list_jobs`, `prune_jobs`, `coverly undo` and the app's job history work unchanged on top of this. `JobSummary` exposes `kind` so the UI can label "Organisation" jobs.
- Resume: `Writer::resume` + `resumable_items` pattern; paused organise jobs (volume gone, disk full, denied) resume from the first not-attempted item.

## 11. Architecture

```
crates/coverly-core/src/
  identify/sniff.rs        header magic → Evidence (pure, byte-slice in)
  organize/mod.rs          public API: probe_folder, plan_organize, apply_organize
  organize/walk.rs         walk + classify (skips, partial downloads, already-sorted folders)
  organize/inspect.rs      open raw / zip inner entry (bounded) / 7z magic / disc sets
  organize/evidence.rs     ladder + combine → Identification
  organize/target.rs       FolderNaming, target paths, collisions, duplicates
  organize/plan.rs         OrganizePlan, OrganizeItem, Decision, Counts (mirrors plan/mod.rs)
  organize/apply.rs        apply with Writer move/copy, progress, pause, resume
  write/mod.rs             + Moved/Copied/DirCreated ops, fingerprints, streamed copy
  systems/mod.rs           + systems_for_extension, system_for_dat
  profiles/{mod,builtin}   + system_folder (FolderNaming)
  profiles/detect.rs       + weak `system-folders` rule
  cache/mod.rs             + schema v2: index dat_roms(crc32), organize_ids, organize_choices
crates/coverly-cli/src/organize.rs
src-tauri/src/organize.rs  commands: probe_folder, organize_plan, organize_apply, organize_undo
src/screens/OrganizeScreen.tsx, src/stores/organizeStore.ts
```

Key types (serde kebab-case, mirrored in `src/ipc/types.ts` and the contract fixtures):

```rust
pub struct OrganizeOptions { layout: String, target: Option<PathBuf>, flat: bool,
    mode: Option<TransferMode /* move|copy */>, names: NameMode /* keep|catalogue */,
    offline: bool, auto_threshold: f32, review_threshold: f32 }
pub struct OrganizeItem { id: usize, sources: Vec<PathBuf>, kind: ItemKind /* file|archive|set */,
    target: Option<PathBuf>, system_id: Option<String>, folder: Option<String>,
    confidence: f32, method: IdLevel /* hash|header|extension|name|user */,
    why: Vec<Evidence>, alternatives: Vec<(String, f32)>, rename_reason: Option<String>,
    #[serde(flatten)] action: OrganizeAction /* auto | review | left{reason} */ }
pub enum LeftReason { PartialDownload, NotARom{what}, DamagedArchive, UnsupportedArchive,
    Unknown, Duplicate{of}, AlreadyPresent{at}, AlreadySorted{folder}, Symlink, SetIncomplete{missing} }
```

IPC: `probe_folder(path)`, `organize_plan(source, options)` (blocking pool, emits `organize-progress {phase: walk|identify|dats, done, total}`), `organize_apply(plan, decisions, options)` (job, emits progress; returns `OrganizeReport`), `organize_undo(job_id)`. New error code: none required; `no-profile` stays and the UI reacts to it. Each new command touches `ipc.rs`, `lib.rs`, `Backend`, `tauriBackend`, `mockBackend`, `contract.requests.json`, `contract.sample.json`.

## 12. Settings

New group **Organise** (`settings.organize`, defaults in brackets):
`default_layout` [`anbernic-stock`], `nest_in_roms_root` [true], `mode` [auto: move same volume / copy otherwise], `names` [keep], `download_dats` [true], `auto_threshold` [0.85], `review_threshold` [0.50], `companion_exts` [list in 7.4], `folder_names` [{}]. Shown in Settings → Library as a small section; the proposal screen's options override them per run.

## 13. Error handling

- One item failing never stops the job; per-item kinds reuse `ItemErrorKind` (`write-denied`, `disk-full`, `name-invalid`, `volume-gone`, `io`) plus `changed-since-plan`, `collision-set`.
- Disk full / denied / volume gone → pause with Resume, journal kept.
- Planning errors: unreadable source folder → error banner; unreadable single file → left in place (*cannot be read*).
- DAT download failure / offline → plan continues with the levels available; a warning line says which catalogues were not consulted ("Sin catálogo de Game Boy Color: decidido por la cabecera").

## 14. Copy (i18n, es + en)

Calm archivist voice: verbs, exact counts, no exclamation marks, no blame. The UI i18n has no plural rules, so counts go after a label or before a noun that reads well in both forms ("archivos: 1" style where needed). Keys under `organize.*` (plus `start.organizeOffer.*`), every key in both locales (`parity.test.ts`).

| Key | es | en |
|---|---|---|
| `start.organizeOffer.title` | Esta carpeta no sigue ningún formato conocido | This folder doesn't follow a known layout |
| `start.organizeOffer.body` | Contiene {roms} archivos que parecen ROMs. Coverly puede proponerte una carpeta por sistema; no se mueve nada hasta que lo confirmes. | It holds {roms} files that look like ROMs. Coverly can propose one folder per system; nothing moves until you confirm. |
| `start.organizeOffer.action` | Organizar esta carpeta… | Organise this folder… |
| `start.organizeOffer.other` | Elegir otra carpeta | Choose another folder |
| `organize.title` | Organizar por sistema | Organise by system |
| `organize.planning` | Identificando {done} de {total}… | Identifying {done} of {total}… |
| `organize.summary` | ROMs: {roms} · Sistemas: {systems} · Por revisar: {review} · Se quedan: {left} | ROMs: {roms} · Systems: {systems} · To review: {review} · Staying put: {left} |
| `organize.dryRunHint` | Es una propuesta. Todavía no se ha movido nada. | This is a proposal. Nothing has moved yet. |
| `organize.layout` | Formato | Layout |
| `organize.target.here` | Esta carpeta | This folder |
| `organize.target.other` | Otra carpeta… | Another folder… |
| `organize.nest.roms` | Dentro de {folder}/ | Inside {folder}/ |
| `organize.nest.flat` | Directamente aquí | Directly here |
| `organize.mode.move` | Mover | Move |
| `organize.mode.copy` | Copiar | Copy |
| `organize.names.keep` | Conservar los nombres | Keep file names |
| `organize.names.catalogue` | Usar el nombre del catálogo | Use the catalogue name |
| `organize.confidence.high` | Alta | High |
| `organize.confidence.medium` | Media | Medium |
| `organize.why.hash` | CRC {crc} en el catálogo de {system}: {name} | CRC {crc} in the {system} catalogue: {name} |
| `organize.why.header` | La cabecera es de {system} ({detail}) | The header is {system} ({detail}) |
| `organize.why.extension` | Extensión .{ext}, propia de {system} | .{ext} extension, used only by {system} |
| `organize.why.name` | El nombre se parece a {name} ({system}) | The name resembles {name} ({system}) |
| `organize.why.user` | Elegido por ti | Chosen by you |
| `organize.review.title` | Por revisar | To review |
| `organize.review.hint` | Elige el sistema de cada archivo o déjalo donde está. | Choose each file's system, or leave it where it is. |
| `organize.review.leave` | Dejar donde está | Leave in place |
| `organize.left.title` | Se quedan donde están | Staying where they are |
| `organize.left.partialDownload` | Descarga sin terminar | Download not finished |
| `organize.left.notARom` | No es una ROM ({what}) | Not a ROM ({what}) |
| `organize.left.webPage` | página web | web page |
| `organize.left.damagedArchive` | Archivo comprimido dañado | Damaged archive |
| `organize.left.unsupportedArchive` | Formato comprimido no compatible | Unsupported archive format |
| `organize.left.unknown` | Sistema desconocido | Unknown system |
| `organize.left.duplicate` | Igual que {name} | Same as {name} |
| `organize.left.alreadyPresent` | Ya está en {path} | Already in {path} |
| `organize.left.alreadySorted` | Ya está en la carpeta de sistema {folder} | Already in the {folder} system folder |
| `organize.left.setIncomplete` | Faltan archivos del juego: {missing} | Game files missing: {missing} |
| `organize.renamed` | Se guardará como {name} porque ya existe otro distinto | Will be saved as {name} because a different file already exists |
| `organize.apply` | Organizar {count} archivos | Organise {count} files |
| `organize.applying` | Moviendo {done} de {total}… | Moving {done} of {total}… |
| `organize.copying` | Copiando {done} de {total}… | Copying {done} of {total}… |
| `organize.result` | Movidos: {moved} · Copiados: {copied} · Omitidos: {skipped} · Errores: {failed} | Moved: {moved} · Copied: {copied} · Skipped: {skipped} · Errors: {failed} |
| `organize.openLibrary` | Abrir biblioteca | Open library |
| `organize.undo` | Deshacer | Undo |
| `organize.undone` | Restaurados: {restored} · Sin tocar por cambios: {modified} | Restored: {restored} · Left alone because they changed: {modified} |
| `organize.noSpace` | No hay espacio: hacen falta {needed}, quedan {free}. | Not enough space: {needed} needed, {free} free. |
| `organize.noCatalogue` | Sin catálogo de {system}: decidido por {level} | No {system} catalogue: decided by {level} |
| `organize.exportPlan` | Exportar plan (JSON) | Export plan (JSON) |
| `start.suggestSystemFolders` | Parece una carpeta organizada por sistema | Looks like a folder organised by system |

## 15. Testing

All fixtures are **synthetic and tiny**, generated in tests; no copyrighted ROM data is committed. Header fixtures contain only the bytes a check reads (zero padding elsewhere), with checksums computed by the test helpers. For GB the check uses only an 8-byte logo prefix and the header checksum, for GBA the fixed byte and complement, for NDS two CRC16s — no full logos are embedded.

- **`identify/sniff.rs` unit tests**, one positive + one near-miss per row of the §6.3 table: `fixtures::gb(cgb: u8)`, `gba()`, `nds()`, `n64(order)`, `md()`, `smd_interleaved()`, `sms_gg(region)`, `snes(map, copier: bool)`, `nes()`, `fds()`, `lynx()`, `a7800()`, `ngp(color)`, `ws(color)`, `psx_iso(raw: bool)`, `html_page()`, `text_md()`. Property test: random bytes never produce ≥ 0.95 evidence.
- **Evidence combination** (`organize/evidence.rs`): table-driven cases — DAT hit wins; GB vs GBC by CGB flag; `.gb` dual-mode → GBC with GB alternative; shared `.bin` without header → Review; negative sniff downgrades unique extension; margin rule.
- **Archives** (`organize/inspect.rs`): zips built in-test with `zip` (stored + deflate): single entry, entry + nfo, multi-entry arcade-shaped, entry in subdir, `.zip` that is HTML, truncated zip, 7z magic only, bad 7z magic, zip-bomb-shaped entry (declared 4 GiB) → bounded read.
- **Walk/classify**: partial downloads (each extension), zero-byte, `.DS_Store`, `._x.gba`, symlink, nested pile depth 5, existing `GBA/` and `gba/` folders.
- **Planner integration** (`crates/coverly-core/tests/organize_plan.rs`, `insta` JSON snapshots): a pile mirroring the real sample's *shape* (names like `1490 - Sonic Advance 3 (U)(Venom).zip`, `Tetris 2.zip` vs `Tetris 2 (1).zip`) with synthetic contents and a mock DAT server (`net::testing::serve_dat`) → GBA/GB/GBC/SFC groups; offline variant; folder naming per profile (invariant test: every built-in profile × system folder name resolves back).
- **Apply/undo** (`tests/organize_apply.rs`, tempdirs only): move in place; copy to another root; cross-volume fallback via an injected `rename` that returns `EXDEV`; collision → `(2)`; set collision → whole set skipped; same content already present; source changed since plan; disk-full and denied pause + resume; undo of each op incl. modified target and occupied source; `DirCreated` cleanup; no `._*` / `.coverly-*.tmp` left; journal from before this change still parses and undoes.
- **Detection**: weak `system-folders` rule fires only when markers don't; existing `detect` snapshots unchanged.
- **CLI** (`crates/coverly-cli/tests/cli_organize.rs`, `assert_cmd`): dry run writes nothing (tree hash before/after), `--apply`, `--copy`, `--assign`, `--report json`, `coverly undo` restores.
- **App**: Vitest for `organizeStore` (plan → decisions → apply → open with `profileId`), StartScreen offer on `no-profile` with/without ROM-like files, contract fixtures, i18n parity. E2E with the mock backend through Chrome DevTools MCP.
- **Real sample (manual, read-only)**: an `#[ignore]` test gated by `COVERLY_SAMPLE_DIR` runs **plan only** against `~/Documents/ROMs-Downloaded` and asserts the §6.7 counts. Apply is verified on a copy under `target/` (copied with `cp -X` / without xattrs), never on the original. `~/Documents/ROMS` is read only to compare folder names.

## 16. Milestones (input for the implementation plan)

| # | Deliverable |
|---|---|
| O1 | `classify` partial-download list; `SystemDb::systems_for_extension` / `system_for_dat`; `FolderNaming` on profiles + invariant test |
| O2 | `identify/sniff.rs` with the §6.3 table and synthetic fixtures |
| O3 | `organize/inspect.rs`: bounded zip inner read (`deflate-flate2`), archive verdicts, disc sets |
| O4 | Cache schema v2 (`crc32` index, `organize_ids`, `organize_choices`); evidence ladder + combine |
| O5 | `organize/plan.rs` + `target.rs`: plan, duplicates, collisions, naming; snapshots |
| O6 | Writer `move_file` / `copy_file` / `ensure_dir`, journal extensions, undo; `organize/apply.rs` |
| O7 | CLI `coverly organize` |
| O8 | Tauri commands, contract, mock backend; weak detection rule; `libraryStore.open(path, profileId?)` |
| O9 | StartScreen offer + `OrganizeScreen` + strings es/en |
| O10 | Real-sample dry run evidence, E2E, tracker update |

## 17. Open questions

1. Default nesting: `<picked>/ROMS/<SYS>` (proposed, mirrors an SD card so `ROMS/` can be dragged onto one) vs `<picked>/<SYS>` (what `~/Documents/ROMS` itself looks like when that folder is the one picked). The option exists either way; only the default is open.
2. Should *Names: catalogue* become the default once DAT coverage is wide enough? (Proposed: no; renaming is opt-in.)
3. Dual-mode GB/GBC carts without a DAT hit: GBC (No-Intro convention, proposed) or GB (runs on more devices)?
4. Add a 7z crate (`sevenz-rust`, pure Rust) to read 7z listings and CRCs in a follow-up?
