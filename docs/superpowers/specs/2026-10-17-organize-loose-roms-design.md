# Coverly ROMs — Organise Loose ROMs (Design Spec)

> Status: **Draft, adversarially reviewed** · Date: 2026-10-17 · Owner: Miguel Angel Esparza Calero (MAECLY)
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
- Second entry point for libraries that **are** detected but have loose files (stray zips at the SD root or directly in `ROMS/`): library menu "Organizar archivos sueltos…" runs the same planner with source = library root, target = same root, layout = the library's profile (§4.5).
- Scales to piles of **10 000 files** (plan in minutes on an SD card, seconds on an SSD; UI stays responsive; §6.1, §9).

### Non-goals (this iteration)
- **Extracting** archives or recompressing. Zips stay zips (the reference layout keeps zipped NDS/arcade sets).
- **Verifying** or repairing dumps, merging/splitting arcade sets, 1G1R curation, deleting duplicates.
- Reading **inside 7z** archives (no 7z crate in the workspace; see §7.2) or inside **CHD** images (no chd crate): both are identified by extension and name only.
- Re-organising a library that is already in a known layout (profile-to-profile migration). Possible later on top of the same planner.
- Folder-based games (DOS folders, PS3/ScummVM directories) beyond reporting them.
- Online identification by default. Optional sources only when the user already configured them (§6.5).

## 3. Users and success

- **Handheld owner with a downloads folder** (primary): picks the folder, sees "16 ROMs in 4 systems, 1 to review, 4 left in place", clicks once, gets `ROMS/GBA`, `ROMS/GB`, `ROMS/GBC`, `ROMS/SFC`, lands in the library screen, fills covers.
- **Curator** moving a pile into an existing SD card: picks the pile, chooses the SD card as target, copies.
- **Scripter**: `coverly organize ~/Downloads/roms --apply`.

Success = on the owner's real sample (`~/Documents/ROMs-Downloaded`) the dry run proposes **GBA ×6, GB ×6, GBC ×2, SFC ×2**, flags `3-D Tetris (USA).zip` as *not a ROM (web page)*, skips `.DS_Store` (and partial downloads when present — the `.crdownload` files seen earlier have since finished, so partial-download handling is proven by synthetic tests, not by the sample), and asks **nothing** — every valid zip is settled by its inner entry (CRC or header).

## 4. User flow (app)

### 4.1 Entry point
1. `StartScreen.pick()` → `libraryStore.open(path)` → `open_library` → `choose_profile` → `detect()` is empty → `IpcError::NoProfile` (code `no-profile`, stable, unchanged).
2. The start screen, on `no-profile`, calls a new cheap probe `probe_folder(path)` (names and `stat` only, no reads, no hashing; **same depth as the planner walk (4)** so the probe never says "nothing here" about ROMs the planner would find; stops after 5 000 entries and reports `truncated: true`): counts ROM-like files (known ROM extensions, archives, disc images), partial downloads, cloud-only placeholders, folders that already resolve to systems, and whether the folder is writable.
3. If `rom_like > 0`, the error banner is replaced by an **offer panel**:
   - Title: "Esta carpeta no sigue ningún formato conocido" / "This folder doesn't follow a known layout".
   - Body with counts from the probe ("5 000+" when truncated).
   - Buttons: **Organizar esta carpeta…** (primary) · **Elegir otra carpeta** (ghost).
   - If `rom_like == 0`, the existing error text stays (it is still correct).
   - If the probe found **only** folders that already resolve to systems (e.g. `~/Documents/ROMS` picked directly, where every ROM is already sorted), the primary button is instead **Abrir como carpeta por sistema / Open as organised by system** with a layout `<select>`: organising would move nothing, so offering it would be a dead end.
   - **Broad roots** (the home folder, `Desktop`, `Documents`, `Downloads` themselves, a system volume root, `/`, `C:\`): the offer still appears but with a warning line ("Es una carpeta personal amplia; solo se proponen archivos con pinta de ROM") and the planner ignores generic extensions without content evidence (§6.4), so personal `.iso`/`.img`/`.bin`/`.zip` files are never proposed for moving.

### 4.2 Proposal screen (`navStore.screen = 'organize'`)
A single screen, built with existing classes (`section-title`, `hint`, `primary-button`, `ghost-button`, `error-banner`, plain `<select>`, `<details>`, `<ul>`). Markup stays simple so it merges with the parallel `src/components/ui` primitives branch.

1. **Options bar** (changing any option re-plans; identification is cached so re-planning is instant):
   - *Formato* / *Layout*: built-in profiles (`anbernic-stock` default, then Garlic, Onion, muOS, MinUI, ES-DE, Batocera, RetroArch) and custom profiles. Default from `settings.organize.default_layout`, else `settings.library.default_profile`, else `anbernic-stock`.
   - *Destino* / *Target*: "This folder" (default) or "Choose another folder…" (e.g. the SD card root). When the chosen target is itself detected as a library (`detect()` finds a profile), the layout `<select>` switches to that profile and a hint says so; picking a different layout there shows a warning ("La tarjeta usa {profile}; las carpetas nuevas no se verán en ese sistema"), never a silent mix.
   - *Carpetas* / *Folders*: "Inside `ROMS/`" (profile's `roms_root`, default) or "Directly here" (`roms_root = ""`, what `fit_to_root` already supports). Hidden for profiles whose `roms_root` is empty.
   - *Modo* / *Mode*: Move (default when source and target are on the same volume) or Copy (default otherwise). **Move is disabled** (with the reason shown) when the source is not writable (read-only volume, mounted disk image, locked share): the default becomes Copy instead of an apply where every item fails.
   - *Nombres* / *Names*: "Keep file names" (default) or "Use the catalogue name" (No-Intro/DAT name, else inner zip name; removes browser ` (1)` suffixes).
2. **Summary line**, exact counts: "16 ROMs → 4 systems · 0 to review · 4 left in place".
3. **Grouped by system** (`<details>` per system, target folder shown: `ROMS/GBA/`): each row shows source name → target relative path, a confidence label (Alta/High ≥ 0.95, Media/Medium ≥ 0.85, — for review), and the "why" in one line ("CRC 7B729804 in the Game Boy Advance catalogue: *Tetris Worlds (USA)*"). **Large piles**: groups start open only when the whole plan has ≤ 200 rows; each group renders its first 200 rows and a "Mostrar {n} más / Show {n} more" button (plain list, no virtualisation library), so 10 000 rows never hit the DOM at once.
4. **To review** (only if any): each item shows its evidence and a `<select>` with the candidate systems (ordered by score) plus "Dejar donde está / Leave in place". Choosing a system moves the item into its group with `method = user`. Review items are **bucketed by candidate set** (e.g. "GB o GBC: 37"), and each bucket has one `<select>` + "Aplicar a todos / Apply to all" so 500 doubtful files are settled in a few clicks, not 500.
5. **Left in place**, with a reason per file: partial download, not a ROM (web page), corrupt archive, unknown, duplicate of …, already in a system folder, symlink, BIOS, cloud-only, recently modified, too big for FAT32, archive with several games. Items left as **unknown** keep the same `<select>` as review items (all systems, alphabetical), so the user can still rescue a file Coverly could not identify.
6. **Actions**: "Organizar {count} archivos / Organise {count} files" (primary; count = auto + user-assigned), "Exportar plan (JSON)", "Cancelar". When the count is 0 the primary action is hidden and, if the target already holds system folders, **Abrir biblioteca** is offered instead.

The proposal **is** the dry run: nothing is written until the primary action. The plan lives in the backend under a `plan_id` (in memory, and persisted to `<data>/jobs/<plan_id>.plan.json` at apply time); the UI only sends `plan_id` + decisions back, never the whole plan (smaller IPC for 10k items, nothing the UI can alter, and the persisted plan is what makes Resume possible after a restart).

### 4.3 Apply, result, open, undo
- Apply runs as a job (progress event `organize-progress`, cancel button, pause on volume loss/disk full/denied with Resume, like fill jobs). It takes the `JobManager` manual guard so it never races a fill job.
- Result panel: "Movidos: 16 · Omitidos: 0 · Errores: 0", plus the per-item list on demand and **Abrir biblioteca / Open library** (primary) and **Deshacer / Undo** (ghost).
- *Open library*: `libraryStore.open(targetRoot, profileId)` — `open(path, profileId?)` gains the optional argument the IPC already supports — and the recent-libraries entry stores `profile_id`, so reopening later also skips detection.
- *Undo*: calls `undo_job(job_id)` through a new `organize_undo` command (the generic in-library undo filters by session root; the organise screen undoes its own job explicitly). Undo is also available later from the library's job history and `coverly undo`.
- *Undo after later jobs*: if newer jobs wrote under `T` (typically a fill that created `ROMS/GBA/Imgs/…`), `organize_undo` refuses by default and lists them ("Primero deshaz: Portadas 17/10 10:32"), offering **Deshacer todo en orden / Undo all in order** (newest first). Undoing the organise job alone would move ROMs back and orphan their art and gamelist entries.
- *Interrupted apply* (app quit, crash, card pulled): the job appears in the job history as *interrupted* with **Reanudar / Resume** (from the persisted plan, §10) and **Deshacer / Undo**; nothing is left half-explained.

### 4.4 Detection fallback (reopening an organised folder from scratch)
A freshly organised `ROMS/GBA/` has no `anbernic/` and no `Imgs/` yet, so marker detection still finds nothing. Two measures:
1. Opening with an explicit `profile_id` (above) — primary fix.
2. New weak rule in `profiles/detect.rs`: if a `roms`/`ROMS`/`Roms`/`ROMs` child (any casing) contains **≥ 1** child folder, or the root itself contains **≥ 2** child folders, that resolve to known systems and hold ROM files, and no marker rule scored, return one low-score detection for `settings.organize.default_layout` (default `anbernic-stock`) with reason `system-folders`. (≥ 1 under `ROMS/` because a single-system pile — all GBA — produces exactly one folder; ≥ 2 at the bare root to avoid matching any folder that happens to contain a `gb/` subfolder.) The UI shows it as a suggestion ("Parece una carpeta organizada por sistema") rather than a certainty. Existing detection tests must keep their results (the rule only fires when nothing else did).

### 4.5 Loose files inside a detected library
The library screen shows "Organizar archivos sueltos… / Organise loose files…" when the scan reported ROM-like files that are not inside any system folder (root of the card or directly in `roms_root`). It opens the same proposal screen with source = library root, target = library root, layout = the session's profile (locked), mode = Move; the walk skips the profile's system and art folders exactly as in §7.1. After apply the library rescans.

## 5. CLI

```
coverly organize <SOURCE>
    [--layout <PROFILE_ID>]        default: settings.organize.default_layout | anbernic-stock
    [--target <DIR>]               default: <SOURCE> (in place)
    [--flat]                       system folders directly in <DIR> (roms_root = "")
    [--copy | --move]              default: move on same volume, copy across volumes
    [--names keep|catalogue]       default: keep
    [--assign <FILE>=<SYSTEM_ID>]… settle review/unknown items (repeatable; split on the LAST `=`,
                                   so names containing `=` work; FILE is relative to SOURCE)
    [--resume <JOB_ID>]            continue an interrupted organise job from its persisted plan
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

Every piece of evidence is recorded (`Evidence { kind, system, weight, detail }`, at most 4 per item sent over IPC); the final "why" is the strongest one, the rest are shown on demand. Results are memoised in the existing `quick_ids` cache key style (path + size + mtime) under a new `organize_ids` table, so re-planning after an option change does no I/O beyond `stat`.

**Precedence vs. cost order.** The ladder's *precedence* is hash > header > extension > name, but the *evaluation* order is by cost, and expensive levels run only when they can change the outcome:
1. extension + name (free) → 2. zip central directory (CRC + size of entries, a few KiB) → 3. bounded header read (≤ 128 KiB, plus the targeted offsets in §6.3) → 4. DAT lookup of CRCs already known (zip entries) → 5. **full-file hash of raw files only when** levels 1–4 leave the item below Auto, or *Names: catalogue* needs the DAT name, or a duplicate check needs it (same-size bucket, §7.5).
A pile of 10 000 raw GBA files with valid headers is therefore planned by reading ~1.3 GB of headers, not ~80 GB of contents. Raw-file hashing reuses `hash_rom` (64 MiB cap).

**Parallelism.** Identification runs on a bounded rayon pool: 4 workers when the source is on an internal SSD, 2 on removable/network volumes (random reads on SD cards and SMB degrade with more). Progress is emitted at most every 100 ms. Planning is cancellable.

### 6.2 Level 1 — hash against DATs (confidence 1.00)
- **Inputs**: raw files hashed with `hash_rom` (CRC32/MD5/SHA1, full + headerless, 64 MiB cap); single-entry zips use the central-directory CRC32 + size (no decompression needed for this level).
- **Which DATs**: candidate systems come from levels 2–3 (extension and header), so we only consult DATs that can match. For each candidate system, `dat_sources(system)`; DATs already cached are used directly; missing DATs are downloaded through `DatStore::ensure` unless `--offline` / `settings.organize.download_dats = false`, capped at 4 DAT downloads per plan to avoid fetching the whole catalogue for one pile.
- **Unknown-system fallback**: when levels 2–3 give no candidate (e.g. a `.bin` with no recognisable header, or a multi-system zip extension), query **all cached DATs** by CRC: new `Cache::dat_by_crc_any(crc32, size) -> Vec<(dat, DatRow)>` backed by a new index on `dat_roms(crc32)` (cache schema v2, additive migration).
- **DAT → system**: reverse map built from `dat_sources` over `SystemDb::all()` (`SystemDb::system_for_dat(path)`).
- **Weights**: hit in a DAT of a candidate system = 1.00; hit outside the candidates = 0.97 (note "the extension pointed elsewhere"); hits in several systems' DATs (e.g. the same CRC in GB and GBC catalogues) → keep those consistent with header evidence; if still more than one → Review with all of them as candidates.
- The central-directory CRC is what the zip *claims*; it is not verified against the data (that would need full decompression). Good enough for sorting; a corrupt zip with an intact directory is still sorted correctly and fails later in the emulator, as it would have anyway.
- **Arcade zips** (multi-entry): the zip **stem** is the set name (`sf2.zip`, `tetris.zip` — short names that clash with console titles, so a name hit alone is weak). Look it up by game name in cached arcade DATs (FBNeo, MAME, …), then **confirm with contents**: if ≥ 75 % of the set's ROM entries in the DAT appear in the zip by CRC → 1.00; name only → 0.70 (Review). Almost every FBNeo set also exists in MAME, so "several arcade DATs hit" is the normal case, not an ambiguity: the layout's **arcade preference** decides (`settings.organize.arcade_order`, default per profile: anbernic-stock `fbneo, mame, cps1, cps2, cps3, neogeo`; ES-DE/Batocera `fbneo, mame`) — the first system in that order whose DAT confirmed the set wins, the others are listed as alternatives. Neo Geo sets need `neogeo.zip`; that BIOS is never moved (§7.1).

### 6.3 Level 2 — header magic (confidence 0.85–0.95)
New module `crates/coverly-core/src/identify/sniff.rs`: `sniff(src: &mut dyn ReadAt, len: u64, ext: &str) -> Vec<Evidence>` where `ReadAt` serves bounded reads at given offsets. Reads a 128 KiB prefix for raw files and for a zip's inner entry (`zip` feature `deflate-flate2`, already a workspace dep, read with `by_index` + `take`; never fully decompressed, which also defuses zip bombs). Signatures beyond the prefix use **targeted reads**: raw files seek; deflated zip entries are stream-skipped up to the offset, capped at 8 MiB of inflated data (enough for SNES ExHiROM at 0x40FFC0 + 0x200), beyond which that check is simply skipped. PS1/ISO checks read sector 16 (and the root directory for `SYSTEM.CNF`) only.

Rows in *italics* are systems that `SystemDb` does not have (Sega Pico, PS2). Their signature is still checked because it is a **veto**: such a file must not fall through to the Mega Drive / PlayStation row. It is left in place as *unsupported system ({name})*. (The reference layout's `PICO/` folder is PICO-8, `pico8`, unrelated.)

Checks run in a fixed order and the **more specific signature wins** within a family: Sega CD (`SEGADISCSYSTEM` at 0 or 0x10) before 32X before Pico before Mega Drive (a Sega CD image and a 32X/Pico ROM also carry `SEGA` at 0x100); FDS before NES; NDS before GBA (NDS cartridges embed a GBA-style area); PS2 before PS1.

| System (`id`) | Signature | Strength |
|---|---|---|
| `nes` | `4E 45 53 1A` at 0 (iNES/NES 2.0), or `UNIF` at 0 (`.unf`) | 0.95 |
| `fds` | `FDS\x1A` at 0, or `\x01*NINTENDO-HVC*` at 0 / 16 | 0.95 |
| `gb` / `gbc` | logo prefix `CE ED 66 66 CC 0D 00 0B` at 0x104 **and** header checksum at 0x14D (`x = 0; for b in 0x134..=0x14C { x = x - b - 1 }`) valid; CGB flag 0x143: `0xC0` → gbc 0.95 · `0x80` (dual) → gbc 0.85 (No-Intro and Anbernic file dual-mode carts under GBC; `.gb` extension adds gb as alternative) · otherwise → gb 0.95. **A `0xC0` (GBC-only) cart is never placed in `gb`** whatever the extension or name says (it would not boot on a GB-only core); the reverse mistake (a GB cart in GBC) is harmless and allowed | 0.85–0.95 |
| `gba` | fixed byte `0x96` at 0xB2 **and** complement at 0xBD valid (`c = 0; for b in 0xA0..=0xBC { c = c - b }; c = c - 0x19`, wrapping u8); logo prefix `24 FF AE 51` at 0x04 adds to the reason. Do **not** key on the ARM branch at 0 (varies, e.g. `7F 00 00 EA`). Homebrew built without `gbafix` fails the complement → negative-evidence rule (Review), which is the right outcome | 0.95 |
| `nds` | logo CRC16 `0xCF56` at 0x15C **and** header CRC16 at 0x15E over 0x000–0x15D | 0.95 |
| `n64` | first 4 bytes `80 37 12 40` (z64), `37 80 40 12` (v64), `40 12 37 80` (n64) | 0.95 |
| `segacd` | `SEGADISCSYSTEM` at 0 (ISO) or 0x10 (raw 2352) — checked **before** Mega Drive | 0.95 |
| `sega32x` | `SEGA 32X` at 0x100, or `SEGA` at 0x100 **and** the 32X security code `MARS` string in the first 0x400 bytes — checked before Mega Drive | 0.95 |
| *(Sega Pico)* | `SEGA PICO` at 0x100 — checked before Mega Drive | veto |
| `megadrive` | `SEGA` at 0x100 or 0x101 (`SEGA MEGA DRIVE`, `SEGA GENESIS`, ` SEGA …`) and none of the above; interleaved SMD: `len % 16384 == 512` **and** `0xAA 0xBB` at 8–9 | 0.90 |
| `mastersystem` / `gamegear` | `TMR SEGA` at 0x7FF0, 0x3FF0 or 0x1FF0 → **SMS family** 0.90. The region nibble (+0xF high bits: 3/4 SMS, 5/6/7 GG) is **only a tiebreak** at 0.70: several GG games carry an SMS region code, and Japanese SMS games often have no `TMR SEGA` at all. A unique `.gg`/`.sms` extension therefore decides between the two; region alone never overrides it | 0.90 / 0.70 |
| `snes` | internal header at 0x7FC0 (LoROM) / 0xFFC0 (HiROM) / 0x40FFC0 (ExHiROM, targeted read), +0x200 if `len % 1024 == 512` (copier header, typical of `.smc`/`.swc`/`.fig`); valid when checksum (+0x1E) XOR complement (+0x1C) == 0xFFFF and map mode (+0x15) ∈ {0x20,0x21,0x23,0x25,0x30,0x31,0x32,0x35}. Translation patches rarely update these two fields consistently but keep them complementary, so hacks still pass. `.sfc` vs `.smc` is not a system question (both `snes`, folder `SFC`); the copier header only moves the offsets | 0.95 (0.85 if only map mode + printable title) |
| `lynx` | `LYNX` at 0 | 0.95 |
| `a7800` | `ATARI7800` at 1 | 0.95 |
| `ngp` / `ngpc` | `COPYRIGHT BY SNK CORPORATION` or ` LICENSED BY SNK CORPORATION` at 0; byte 0x23 == 0x10 → ngpc, else ngp | 0.90 |
| `wonderswan` / `wonderswancolor` | 10-byte footer: byte `len-10` == 0xEA (jump); `len-9` bit 0 = colour | 0.85 |
| `psx` / *(PS2)* | ISO 9660 `CD001` at sector 16 (offset 0x8001 for 2048-byte, 0x9319 for raw 2352-byte `.bin`) **and** `PLAYSTATION` at system-area offset 0x8008 / raw equivalent → PlayStation family 0.80 (PS2 CD-ROM discs carry the same string). `SYSTEM.CNF` in the root directory (one directory-record read) decides: `BOOT =` → psx 0.95, `BOOT2 =` → PS2 veto | 0.80–0.95 |
| `segacd` / `saturn` / `dreamcast` | `SEGADISCSYSTEM` / `SEGA SEGASATURN` / `SEGA SEGAKATANA` at the start of the first data track | 0.90 |
| `psp` | `CD001` + `PSP GAME` system id (real UMD dumps; volume id also accepted); `.cso` magic `CISO` with an uncompressed size a UMD can hold (≤ 1.8 GB; larger CSO are PS2) | 0.85 |
| `pcengine`, `a2600`, `a5200`, `msx`, others | no reliable magic → levels 1, 3, 4 only | — |
| any | CHD (`MComprHD` at 0) / 7z / RAR | container recognised, contents not read → name + extension only | — |

Negative evidence: when a system with a signature is the **only** extension candidate and the sniff fails decisively (e.g. a `.gba` without `0x96` at 0xB2; a `.md` that is UTF-8 text), the extension weight drops to 0.60 → Review with reason "the extension says X but the contents don't look like X" (homebrew and hacks land here, which is correct).

Text and web pages: any candidate whose first 512 bytes are printable text starting with `<!DOCTYPE`, `<html`, `<?xml`, `{` or similar is **not a ROM** (`not-a-rom: web-page|text`), regardless of extension.

### 6.4 Level 3 — extension (confidence 0.85 unique · 0.50 shared)
- New `SystemDb::systems_for_extension(ext) -> Vec<&System>` (lowercased, cached map).
- **Unique** extensions (`gba`, `gbc`, `nes`, `sfc`, `smc`, `n64`, `z64`, `v64`, `nds`, `gg`, `sms`, `pce`, `32x`, `a26`, `lnx`, `vb`, `ngp`, `ws`, `p8`, `cdi`…) → 0.85.
- **Shared** extensions (`gb`, `bin`, `iso`, `cue`, `chd`, `ccd`, `pbp`, `wsc`, `ngc`, `d64`/`prg`/`t64`/`tap`/`crt`, `cas`, `com`, `ipf`, `m3u`) → each candidate 0.50 → needs level 1/2 to reach Auto.
- **Generic** extensions that many non-ROM files use (`bin`, `iso`, `img`, `rom`, `dat`, `chd` without a name signal) give **no** candidate on their own: without header or DAT evidence the item is *Left in place: unknown* (still assignable by the user, §4.2), not Review. This keeps firmware, installer ISOs and disk images out of the review list when someone picks a broad folder.
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
- **Auto** when `best ≥ auto_threshold` (default **0.85**) and `margin ≥ 0.10` **and** the best system has *content* support: a DAT hit, header evidence ≥ 0.85, or a unique extension of a system that has no signature in §6.3 (sniff ran and found no contradiction). Extension + name + tag bonuses alone can never reach Auto (e.g. a `.7z` or a `.gba` that could not be read).
- **Hard vetoes** override scores: a GBC-only header excludes `gb`; a decisive negative sniff excludes that system from Auto; `not-a-rom` excludes everything.
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
| `*.crdownload` (when present; none in the sample as of 2026-10-17) | partial download | left in place: *download not finished* |
| `.DS_Store` | system file | ignored silently (counted, not listed) |

Same title, different systems (`Tetris 2.zip` = SNES, `Tetris 2 (1).zip` = GB) end in different folders: no collision.

## 7. Files, archives, sets

### 7.1 Walking and classification
- Walk the source with `walkdir`, **no symlink following** (symlinks, Windows junctions and other reparse points are reported and left), `same_file_system(true)` (never descend into another mounted volume), max depth 4, ignoring `ignored_dirs` (`bios`, `saves`, `states`, `Shortcuts`), hidden dirs, and the target tree if it is inside the source. Folders deeper than 4 are not silently dropped: the plan reports "{n} carpetas más profundas no revisadas" with their paths.
- A subfolder that already resolves to a system (`resolve_folder`, case-insensitive) is **not** walked: reported once as "already in a system folder (`GBA/`)". If its name is not the chosen layout's name for that system (e.g. `gba` vs `GBA`), the plan does **not** rename it; on case-insensitive volumes it simply is the target folder (see §9).
- Skips (never moved, counted):
  - junk: `JUNK_FILES`, AppleDouble `._*`, dot-files (`classify::is_skipped_entry`).
  - **partial downloads** (new list in `classify.rs`, reused by scan): `.crdownload`, `.part`, `.partial`, `.download`, `.opdownload`, `.tmp`, `.!ut`, `.!qb`, `.aria2` sidecars, Safari `.download` bundles (directories), and zero-byte files.
  - **Firefox-style downloads**: a file with a sibling `<name>.part` is still being written, whatever its own size.
  - **Recently modified** files (mtime within the last 60 s at plan time, or changed between plan and apply): *may still be downloading or copying*, left in place.
  - **Cloud-only placeholders**: macOS dataless files (`st_flags & SF_DATALESS`, iCloud "Optimise storage" — `~/Documents` is often in iCloud Drive) and Windows files with `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS` / `OFFLINE` (OneDrive). Reading them would trigger a download of every file in the pile; they are identified by **name/extension only**, never Auto, and the reason says *in the cloud, not downloaded*.
  - Windows `*:Zone.Identifier` stream files (appear as regular files after copies through WSL/exFAT) — junk.
  - **BIOS / firmware** files, by name (case-insensitive) and, where known, by CRC: `gba_bios.bin`, `bios7.bin`, `bios9.bin`, `firmware.bin`, `scph*.bin`, `neogeo.zip`, `syscard*.pce`, `disksys.rom`, `dc_boot.bin`, `dc_flash.bin`, `bios_CD_*.bin`, `sega_101.bin`, `mpr-*.bin`, `kick*.rom`, … (list in `classify.rs`): left in place as *BIOS (goes in the device's BIOS folder)*. Moving a BIOS into a game folder makes it a fake game and hides it from the emulator.
  - non-ROM documents/images (`txt`, `nfo`, `pdf`, `jpg`, `png`…) unless a ROM with the same stem moves (then they are *companions*, see 7.4) — otherwise left in place silently.

### 7.2 Archives
| Case | Detection | Handling |
|---|---|---|
| Valid zip, 1 entry | `PK\x03\x04` / `PK\x05\x06` magic + `ZipArchive::new` OK | identify by inner entry (CRC, inner name, 128 KiB header read) |
| Valid zip, many entries, one ROM-like entry + readme/nfo | inner exts | identify by that entry |
| Valid zip, many ROM-like entries | arcade-shaped (no unique console ext) | arcade lookup by set name + entry CRCs (6.2); else Review |
| Valid zip, several **console** ROMs of one system (e.g. GoodTools `Tetris (V1.0).gb` + `Tetris (V1.1).gb`) | every entry identified independently and all agree | moved to that system, Auto allowed, with the note *"contiene {n} ROMs; la consola cargará solo la primera"* (most cores load the first entry) |
| Valid zip, console ROMs of **different** systems, or > 8 ROM entries (a "collection" zip) | entries disagree / count | left in place as *archive with several games — extract it first* (organise never extracts) |
| Zip with > 10 000 entries or a central directory > 16 MiB | directory size | *damaged archive* (bounded parse) |
| Zip with a directory inside | allowed | as above, by entry names |
| `.zip` that is not a zip | magic mismatch / `ZipArchive::new` fails | text → *not a ROM (web page)*; else *damaged archive*; left in place |
| Encrypted or unsupported compression | `zip` error on read | CRC level still works (central directory); header level skipped; reason noted |
| `.7z` | magic `37 7A BC AF 27 1C` | **name and extension only** (no 7z crate): inner listing not read; best case Review via name signals, unless the name hits a DAT stem ≥ 95 (0.80 → Review, never Auto). Bad magic → *damaged archive*. A later iteration may add `sevenz-rust` for listings |
| `.chd` | magic `MComprHD` | container recognised, not read: name + extension only (never Auto); bad magic → *damaged archive* |
| `.rar` and others | — | left in place: *unsupported archive* |

Archives are **never extracted** and keep their name (unless *Names: catalogue*, which renames the `.zip` itself to `<catalogue stem>.zip`).

### 7.3 Multi-file games
- Disc sets are grouped with `scan::multifile::group_sets` (`.cue` → its `.bin`/`.wav` tracks, `.gdi` → tracks, `.m3u` → its discs and their closures). The planner makes **one plan item per set**; the set's system is identified from the primary data track (first `BINARY`/`MODE` track of the cue, track 03 of a gdi).
- All files of a set move together into `<SYS>/`, keeping their names and relative layout (a cue referencing `Disc 1/track.bin` keeps `Disc 1/`).
- If any member collides, is missing, is a partial download, or is referenced by an **absolute path or a path with `..`** that leaves the set's folder, the **whole set** is left in place with the reason — never renamed piecemeal (the cue text references names). A file referenced by two sets (shared `.bin`) is never moved; both sets are left with *shared track*.
- `.m3u` playlists are moved with their discs; `.cue` files referenced by an m3u are not separate items.

### 7.4 Companions
Files sharing the exact stem of a moved ROM with an extension in `settings.organize.companion_exts` (default: `sav`, `srm`, `rtc`, `state*`, `cht`, `ips`, `bps`, `ups`, `txt`, `nfo`, `png`, `jpg`) follow the ROM into the same folder (the reference layout keeps `.sav`/`.srm` next to ROMs). Image companions are **not** moved into `Imgs/`; that is the fill flow's job.
- Each companion is its own journal entry (undone individually).
- With *Names: catalogue*, companions are **renamed to the new stem** in the same item (emulators find saves by ROM stem; a renamed ROM with an old-stem `.srm` silently loses its save). If any companion's new name collides, the ROM keeps its original name too (whole item, not piecemeal). Disc sets (`.cue`/`.gdi`/`.m3u`) are never catalogue-renamed: their text references member names.
- A companion whose stem matches several moved ROMs (e.g. `Tetris.sav` with `Tetris.gb` and `Tetris.gbc` going to different folders) is left in place with *ambiguous companion*.

### 7.5 Duplicates
- Same content (CRC32 + size, or SHA1 when computed) twice in the pile: the first (shortest name, then alphabetical) is planned; the others are left in place as *duplicate of `<name>`*. Nothing is deleted.
- Raw files are only hashed for this when **another candidate has the same size** (size buckets first), so duplicate detection does not force a full read of every file (§6.1). A zip and a raw file compare by inner CRC + size.
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
- **Shared folders** (`host_folder` map per profile): some devices have one folder for a system family. Anbernic stock (and the reference `~/Documents/ROMS`) has `NGP/` and `WS/` but **no** `NGPC/` or `WSC/`; its emulators play colour carts from those folders. So for `anbernic-stock`, `garlicos`, `onion`: `ngpc → NGP`, `wonderswancolor → WS`. Without this, colour carts would land in `NGPC/`/`WSC/` folders the device never shows.
- Invariant (tested for every built-in profile × system): the produced folder name **resolves back** via `SystemDb::resolve_folder` to the same system **or to its declared host system** (`ngpc → NGP → ngp`), otherwise the scan would not see it. A user override that does not resolve is rejected with a clear message. Known limitation, unchanged by this spec: the scan labels games in a shared folder with the host system, so NGPC/WSC cover lookups use the NGP/WS catalogues (as they already do for a real Anbernic card).
- Systems with no folder on the chosen device and no host (e.g. `pico8` on a layout without it) use their first alias; the plan shows a hint ("{layout} no tiene carpeta para {system}; se crea {folder}") so the user is not surprised that the device ignores it.

### 8.2 Target paths
- Target root `T` = picked folder (default) or chosen folder. ROMs root `R = T/<profile.roms_root>` (or `T` with *Directly here*). Item target: `R/<folder>/<file name>`.
- If `R/<folder>` already exists under any casing (`find_dir_ignoring_case`), that folder is used as is.
- Names pass `safe_file_name` (already handles `<>:"/\|?*`, control characters, trailing dots/spaces, DOS device names like `CON`/`NUL`/`COM1`, 255-byte limit); a name that would change is planned with the safe name and the reason ("renamed for FAT/exFAT: `:` → `_`"). Safe names are applied on **every** platform, not only on FAT targets, so a library organised on macOS stays valid when copied to an SD card or opened on Windows.
- **Path length**: if `R/<folder>/<name>` would exceed 259 UTF-16 units on Windows (Rust's std adds the `\\?\` prefix internally, but many emulators and Explorer do not cope), the item is planned with a shortened stem (same rule as the 255-byte cut) and the reason shown.
- **FAT32 4 GiB limit**: the planner queries the target file system (`statfs` `f_fstypename` on macOS, `/proc/self/mountinfo` on Linux, `GetVolumeInformationW` on Windows). On FAT32 any item ≥ 4 GiB (large ISOs, PSP/DC images) is left in place as *too big for FAT32 (exFAT needed)* instead of failing mid-copy.
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
- **No-clobber rename** (the core of "never overwrite"): `std::fs::rename` silently **replaces** an existing target on Unix and on Windows (std uses `MOVEFILE_REPLACE_EXISTING`), so a check-then-rename has a race (and on case-insensitive volumes `tetris.zip` → `Tetris.zip` over an unseen twin). All renames go through a new `write::rename_noclobber(from, to)`: macOS `renamex_np(RENAME_EXCL)`, Linux `renameat2(RENAME_NOREPLACE)`, Windows `MoveFileExW` **without** `MOVEFILE_REPLACE_EXISTING`. Where the call is unsupported (old kernels, some FUSE/SMB mounts: `EINVAL`/`ENOSYS`/`ENOTSUP`) the fallback is `hard_link(from, to)` (fails if `to` exists) + `remove_file(from)`; where hard links are unsupported too (FAT/exFAT), a final `find_twin` check immediately before `rename`, with the remaining race documented and the item journaled first. A collision detected at this point → the item gets the next free `(n)` name, never an overwrite.
- **Move**: `rename_noclobber` when source and target share a volume (`st_dev` on Unix, volume serial on Windows). Cross-volume move = streamed copy (as Copy below) → **verify by re-reading the target** after `sync_all` (size + CRC32 of the bytes on disk, compared with the CRC computed while reading the source; the cost is one extra read, and it is the only check that catches a flaky SD card before the original is deleted) → delete source. `EXDEV` (errno 18) / Windows `ERROR_NOT_SAME_DEVICE` (17) from `rename` falls back to this path; `map_io` gains this mapping.
- If deleting the source fails after a verified copy (read-only attribute on Windows, locked file, permissions), the item is reported as *copied; original could not be removed* and journaled as `Copied` — never as a failure that invites a retry over the copy.
- **Copy**: streamed (`File::open` → `create_new` temp `.coverly-<job>-<n>.tmp` in the target dir → `io::copy` with a 1 MiB buffer → `sync_all` → `rename_noclobber`). **Never `fs::copy`** (copies xattrs → `._*` files on FAT). Quarantine/provenance xattrs and Windows `Zone.Identifier` streams of the source are not carried over. mtime is preserved with `File::set_modified` so dates in the by-date view stay meaningful.
- **Fingerprints are taken from the target after the operation** (re-`stat` after `set_modified`): FAT stores mtime with 2 s resolution and exFAT with 10 ms, so the source's `mtime_ns` would never match at undo time.
- **Windows sharing violations** (`ERROR_SHARING_VIOLATION` 32 / `ERROR_LOCK_VIOLATION` 33 — antivirus or the indexer opening a fresh file): retried 3 times with 200/500/1000 ms back-off, then the item fails with `io` and the job continues.
- **Leftover temp files**: on resume, undo and at the start of every apply, `.coverly-<job>-*.tmp` files of that job in the touched target folders are deleted (they are only ever partial copies).
- **Free space**: before a copy (or cross-volume move), sum of sizes + 1 % must be free on the target volume, else apply refuses to start with exact numbers.
- **Removable volumes**: AppleDouble sidecars of created folders/files are removed, as in `Writer::write`.
- **Case-insensitive volumes**: all name comparisons use NFC + lowercase **on every volume**, including case-sensitive ext4/APFS-CS (the library may later be copied to a FAT card; the cost is an occasional unnecessary ` (2)`); never create `GBA/` next to `gba/`; a rename whose only change is case is a no-op (not attempted). If a case-sensitive target already has both `gba/` and `GBA/`, the one whose name equals the layout's name is used and the plan says so.
- **Never write** to `/Volumes/System`, the source in Copy mode, or anything outside `S` ∪ `T`.
- **Manual overrides**: `identify::overrides` entries keyed by a moved path are rewritten to the new path (best effort, logged, journaled as `OverrideMoved` so undo restores them).
- **Concurrency**: one worker per volume pair (moves are metadata ops; copies are I/O-bound and contend on SD cards). Cancel between items.
- **Directory name index**: `find_twin` reads the whole directory per call, which is O(n²) for 10 000 files landing in one `GBA/`. Apply (and plan-time collision checks) keep a per-directory index (`HashMap<dir, HashSet<nfc-lowercase name>>`), built once with one `read_dir` and updated after each operation; `rename_noclobber` remains the final authority.
- **Journal cost**: one `fsync` of the journal per entry (journal lives in the app data dir, normally the internal disk), i.e. ~2 × 10 000 fsyncs for a big pile — acceptable (tens of seconds) and kept, since the write-ahead order is what makes undo safe.

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
  - `Moved`: target exists with matching `fp` (size + mtime; CRC only if mtime differs) and `from` is free → rename back with `rename_noclobber` (stream back + verify if `via: copy` / different volume), recreating `from`'s parent. Target missing → nothing. Target modified → `skipped_modified`. `from` occupied → if its size + CRC equal `fp` (the crash window of a cross-volume move: copy finished, source not yet deleted) the target copy is deleted and the entry counts as restored; otherwise skipped with reason.
  - `Copied`: target matches `fp` → delete it; else `skipped_modified`. Source is untouched.
  - Any op whose target is missing: also delete that job's `.coverly-<job>-*.tmp` in the target folder.
  - Undo needs `source_root` to be present for `Moved` (card plugged in); otherwise it pauses with *volume gone* like apply, instead of reporting everything as failed.
  - `OverrideMoved { from, to }`: manual-override entries rewritten by apply (§9) are journaled and rewritten back on undo.
  - `DirCreated`: remove only if empty (ignoring `.DS_Store`/`._*` that macOS may have added).
- `undo_last`, `undo_job`, `list_jobs`, `coverly undo` and the app's job history work unchanged on top of this. `JobSummary` exposes `kind` so the UI can label "Organisation" jobs.
- **Pruning**: today `prune_jobs(keep)` deletes all but the newest `keep` journals regardless of kind, so a few fill runs after an organise would silently destroy the only way to put hundreds of files back. `prune_jobs` counts `fill` and `organize` journals **separately**, and organise journals are additionally kept while younger than 90 days. The `.plan.json` is pruned with its journal.
- **Forward compatibility**: a journal line with an unknown `op` kind (written by a newer Coverly) makes that job *not undoable by this version* in `list_jobs` instead of failing the whole listing; undo refuses it with that message.
- Resume: `Writer::resume` + `resumable_items` pattern; paused or interrupted organise jobs (volume gone, disk full, denied, app quit) resume from the first not-attempted item, using `<data>/jobs/<job>.plan.json` (the plan + decisions persisted before the first action). Each resumed item is re-validated exactly like a fresh apply (§9).

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
  write/noclobber.rs       rename_noclobber (renamex_np / renameat2 / MoveFileExW), retries
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
pub enum LeftReason { PartialDownload, RecentlyModified, CloudOnly, NotARom{what}, Bios,
    DamagedArchive, UnsupportedArchive, MultiGameArchive{count}, UnsupportedSystem{name},
    Unknown, Duplicate{of}, AlreadyPresent{at}, AlreadySorted{folder}, Symlink,
    SetIncomplete{missing}, SharedTrack, AmbiguousCompanion, TooBigForFat, Unreadable }
```

IPC: `probe_folder(path)`, `organize_plan(source, options) -> { plan_id, plan }` (blocking pool, cancellable, emits `organize-progress {phase: walk|identify|dats, done, total}` at most every 100 ms), `organize_apply(plan_id, decisions)` (job; the backend re-validates against its own copy of the plan; emits progress; returns `OrganizeReport`), `organize_resume(job_id)`, `organize_undo(job_id, include_later: bool)`. New error code: none required; `no-profile` stays and the UI reacts to it. Each new command touches `ipc.rs`, `lib.rs`, `Backend`, `tauriBackend`, `mockBackend`, `contract.requests.json`, `contract.sample.json`.

## 12. Settings

New group **Organise** (`settings.organize`, defaults in brackets):
`default_layout` [`anbernic-stock`], `nest_in_roms_root` [true], `mode` [auto: move same volume / copy otherwise], `names` [keep], `download_dats` [true], `auto_threshold` [0.85], `review_threshold` [0.50], `companion_exts` [list in 7.4], `folder_names` [{}], `arcade_order` [per profile, §6.2], `recent_seconds` [60], `keep_organize_journals_days` [90]. Shown in Settings → Library as a small section; the proposal screen's options override them per run.

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
| `start.organizeOffer.openSorted` | Abrir como carpeta por sistema | Open as organised by system |
| `start.organizeOffer.broadRoot` | Es una carpeta personal amplia; solo se proponen archivos con pinta de ROM. | This is a broad personal folder; only files that look like ROMs are proposed. |
| `start.organizeOffer.truncated` | Más de {count} archivos; se revisarán todos al organizar. | More than {count} files; all of them are checked when organising. |
| `library.organizeLoose` | Organizar archivos sueltos… | Organise loose files… |
| `organize.target.detected` | El destino ya es una biblioteca {profile}; se usa ese formato. | The target is already a {profile} library; that layout is used. |
| `organize.target.mismatch` | El destino usa {profile}; las carpetas nuevas no se verán en ese sistema. | The target uses {profile}; new folders won't show up on that system. |
| `organize.mode.moveDisabled` | No se puede mover: la carpeta de origen es de solo lectura. | Can't move: the source folder is read-only. |
| `organize.showMore` | Mostrar {count} más | Show {count} more |
| `organize.review.bucket` | {systems}: {count} | {systems}: {count} |
| `organize.review.applyAll` | Aplicar a todos | Apply to all |
| `organize.deepFolders` | Carpetas más profundas no revisadas: {count} | Deeper folders not checked: {count} |
| `organize.noFolderOnDevice` | {layout} no tiene carpeta para {system}; se crea {folder}. | {layout} has no folder for {system}; {folder} will be created. |
| `organize.multiRomZip` | Contiene {count} ROMs; la consola cargará solo la primera. | Holds {count} ROMs; the console will load only the first. |
| `organize.left.recent` | Modificado hace un momento; puede que aún se esté descargando | Changed moments ago; it may still be downloading |
| `organize.left.cloudOnly` | Está en la nube, sin descargar | In the cloud, not downloaded |
| `organize.left.bios` | BIOS: va en la carpeta de BIOS del dispositivo | BIOS: belongs in the device's BIOS folder |
| `organize.left.multiGameArchive` | Comprimido con varios juegos; descomprímelo antes | Archive with several games; extract it first |
| `organize.left.unsupportedSystem` | Sistema no compatible ({name}) | Unsupported system ({name}) |
| `organize.left.sharedTrack` | Pista compartida por varios juegos | Track shared by several games |
| `organize.left.ambiguousCompanion` | Acompaña a varias ROMs; no se sabe con cuál ir | Belongs to several ROMs; unclear which to follow |
| `organize.left.tooBigForFat` | Demasiado grande para FAT32 (hace falta exFAT) | Too big for FAT32 (exFAT needed) |
| `organize.left.unreadable` | No se puede leer | Can't be read |
| `organize.copiedNotRemoved` | Copiado; el original no se pudo borrar | Copied; the original couldn't be removed |
| `organize.undo.laterJobs` | Primero hay que deshacer: {jobs} | These must be undone first: {jobs} |
| `organize.undo.all` | Deshacer todo en orden | Undo all in order |
| `organize.resume` | Reanudar | Resume |
| `organize.interrupted` | Interrumpido | Interrupted |

## 15. Testing

All fixtures are **synthetic and tiny**, generated in tests; no copyrighted ROM data is committed. Header fixtures contain only the bytes a check reads (zero padding elsewhere), with checksums computed by the test helpers. For GB the check uses only an 8-byte logo prefix and the header checksum, for GBA the fixed byte and complement, for NDS two CRC16s — no full logos are embedded.

- **`identify/sniff.rs` unit tests**, one positive + one near-miss per row of the §6.3 table: `fixtures::gb(cgb: u8)`, `gba()`, `nds()`, `n64(order)`, `md()`, `smd_interleaved()`, `sms_gg(region)`, `snes(map, copier: bool)`, `nes()`, `fds()`, `lynx()`, `a7800()`, `ngp(color)`, `ws(color)`, `psx_iso(raw: bool)`, `html_page()`, `text_md()`. Property test: random bytes never produce ≥ 0.95 evidence.
- **Evidence combination** (`organize/evidence.rs`): table-driven cases — DAT hit wins; GB vs GBC by CGB flag; `.gb` dual-mode → GBC with GB alternative; shared `.bin` without header → Review; negative sniff downgrades unique extension; margin rule.
- **Archives** (`organize/inspect.rs`): zips built in-test with `zip` (stored + deflate): single entry, entry + nfo, multi-entry arcade-shaped, entry in subdir, `.zip` that is HTML, truncated zip, 7z magic only, bad 7z magic, zip-bomb-shaped entry (declared 4 GiB) → bounded read.
- **Sniff ordering / vetoes**: Sega CD image and 32X ROM never come out as `megadrive`; Pico header → unsupported; PS2 `BOOT2` → unsupported, not `psx`; GG game with SMS region + `.gg` → `gamegear`; GBC-only header + `.gb` extension → never `gb`; SNES ExHiROM header found via targeted read inside a deflated zip entry; NDS not mistaken for GBA.
- **Archives, more**: zip with two `.gb` versions → GB + multi-ROM note; zip with `.gb` + `.gba` → multi-game archive; zip with 20 `.nes` → multi-game archive; arcade set confirmed by entry CRCs in FBNeo and MAME → `arcade_order` winner; arcade name hit without CRC confirmation → Review; CHD magic.
- **Walk/classify**: partial downloads (each extension), zero-byte, Firefox `x.zip` + `x.zip.part`, mtime < 60 s, `.DS_Store`, `._x.gba`, `x.zip:Zone.Identifier`, BIOS names, symlink, nested pile depth 5 (reported as deep folder), existing `GBA/` and `gba/` folders. Dataless/offline placeholders via an injected metadata provider (the flags cannot be produced in a tempdir).
- **Generic extensions**: a pile with `installer.iso`, `firmware.bin`, `photos.zip` (jpgs inside) → nothing in Review, all left as unknown / not a ROM.
- **Performance** (`#[ignore]`, run in CI-local gates): 10 000 synthetic 64 KiB GBA files plan in < 30 s on the dev machine with no full-file hashing (counter on `hash_rom` calls == 0) and apply-move completes with `read_dir` calls ≪ n (index counter).
- **Planner integration** (`crates/coverly-core/tests/organize_plan.rs`, `insta` JSON snapshots): a pile mirroring the real sample's *shape* (names like `1490 - Sonic Advance 3 (U)(Venom).zip`, `Tetris 2.zip` vs `Tetris 2 (1).zip`) with synthetic contents and a mock DAT server (`net::testing::serve_dat`) → GBA/GB/GBC/SFC groups; offline variant; folder naming per profile (invariant test: every built-in profile × system folder name resolves back).
- **Apply/undo** (`tests/organize_apply.rs`, tempdirs only): `rename_noclobber` refuses an existing target (exact and case-twin) on each OS, fallback path via injected `ENOTSUP`; target created between plan and apply → `(2)`, original untouched; cross-volume move whose re-read CRC mismatches (injected) keeps the source; crash after copy before source delete → undo deletes the copy; source delete fails → reported as copied; fingerprints on a 2 s-mtime fake FS; prune keeps the organise journal after 30 fill jobs; undo with a later fill job refuses unless `include_later`; companions renamed with catalogue names; move in place; copy to another root; cross-volume fallback via an injected `rename` that returns `EXDEV`; collision → `(2)`; set collision → whole set skipped; same content already present; source changed since plan; disk-full and denied pause + resume; undo of each op incl. modified target and occupied source; `DirCreated` cleanup; no `._*` / `.coverly-*.tmp` left; journal from before this change still parses and undoes.
- **Detection**: weak `system-folders` rule fires only when markers don't; existing `detect` snapshots unchanged.
- **CLI** (`crates/coverly-cli/tests/cli_organize.rs`, `assert_cmd`): dry run writes nothing (tree hash before/after), `--apply`, `--copy`, `--assign`, `--report json`, `coverly undo` restores.
- **App**: Vitest for `organizeStore` (plan → decisions → apply → open with `profileId`), StartScreen offer on `no-profile` with/without ROM-like files, contract fixtures, i18n parity. E2E with the mock backend through Chrome DevTools MCP.
- **Real sample (manual, read-only)**: an `#[ignore]` test gated by `COVERLY_SAMPLE_DIR` runs **plan only** against `/Volumes/TinyExt/ROMs-Downloaded` (it was `~/Documents/ROMs-Downloaded` when this spec was written) and asserts the §6.7 counts. Apply is verified on a copy under `target/` (copied with `cp -X` / without xattrs), never on the original. `~/Documents/ROMS` is read only to compare folder names.

## 16. Milestones (input for the implementation plan)

| # | Deliverable |
|---|---|
| O1 | `classify` partial-download list; `SystemDb::systems_for_extension` / `system_for_dat`; `FolderNaming` on profiles + invariant test |
| O2 | `identify/sniff.rs` with the §6.3 table and synthetic fixtures |
| O3 | `organize/inspect.rs`: bounded zip inner read (`deflate-flate2`), archive verdicts, disc sets |
| O4 | Cache schema v2 (`crc32` index, `organize_ids`, `organize_choices`); evidence ladder + combine |
| O5 | `organize/plan.rs` + `target.rs`: plan, duplicates, collisions, naming; snapshots |
| O6 | `rename_noclobber` per OS; Writer `move_file` / `copy_file` / `ensure_dir`, journal extensions, undo (incl. later-jobs check, overrides), kind-aware pruning, persisted plan + resume; `organize/apply.rs` with directory index |
| O7 | CLI `coverly organize` |
| O8 | Tauri commands, contract, mock backend; weak detection rule; `libraryStore.open(path, profileId?)` |
| O9 | StartScreen offer (incl. open-as-sorted, broad-root warning) + `OrganizeScreen` (capped groups, review buckets) + library "organise loose files" entry + strings es/en |
| O10 | Real-sample dry run evidence, E2E, tracker update |

## 17. Open questions

1. Default nesting: `<picked>/ROMS/<SYS>` (proposed, mirrors an SD card so `ROMS/` can be dragged onto one) vs `<picked>/<SYS>` (what `~/Documents/ROMS` itself looks like when that folder is the one picked). The option exists either way; only the default is open.
2. Should *Names: catalogue* become the default once DAT coverage is wide enough? (Proposed: no; renaming is opt-in.)
3. Dual-mode GB/GBC carts without a DAT hit: GBC (No-Intro convention, proposed) or GB (runs on more devices)?
4. Add a 7z crate (`sevenz-rust`, pure Rust) to read 7z listings and CRCs in a follow-up?
5. Default `arcade_order` for Anbernic stock: `fbneo` first (proposed; the stock FBNeo core covers most sets) or `mame`?
6. Should BIOS files found in the pile be offered a move to the profile's BIOS folder (`BIOS/` on Anbernic stock) in a later iteration? (This spec only leaves them in place.)
