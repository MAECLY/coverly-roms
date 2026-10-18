# Coverly ROMs — Duplicates (Design Spec)

> Status: **Draft for review** · Date: 2026-10-18 · Owner: Miguel Angel Esparza Calero (MAECLY)
> Revised after an adversarial review (false positives, data loss, UX traps, 50k-file scale): functional headers, arcade set names, soft patches, BIOS by hash, overlapping sets, level 3 volume fingerprint, off-card saves, pile-side quarantine, honest space wording, cross-volume preflight, volume-UUID cache keys, bulk confirm.
> Extends [`2026-10-17-organize-loose-roms-design.md`](2026-10-17-organize-loose-roms-design.md) (its §2 listed "deleting duplicates" and "1G1R curation" as non-goals; this spec brings them in, without ever deleting).
> Branch: `feat/organize-dedupe`. Depends on the archive and disc work in `feat/organize-disc-archives` (§7.4). Spec and plan only: no code in this branch until the plan is approved.

## 1. Purpose

Real libraries collect the same game more than once: a browser re-download (`Tetris 2 (1).zip`), the same ROM under a translated or misspelled name (`Caesar's Palace Advance.gba` = `Golden Nugget Casino.gba`), a copy with a copier header and one without, a PS disc as `.bin` and again as `.PBP`, or two revisions of a cartridge. The extras waste space and push the real game down a long list on a small screen.

Coverly must be able to:

1. **Find** duplicates inside each system, at three levels, with a confidence and a plain reason. Content decides, never the file name.
2. **Pick one copy to keep** per group with ordered, configurable rules, and say in one line why that copy won.
3. **Set the others aside** in a quarantine folder outside `ROMS/`, carrying their saves and art, journaled and undoable. The OS Trash is an opt-in alternative. Nothing is ever deleted.
4. Do this as **a step of Organise** (the pile against itself and against the library) and as **a library action** (an existing library against itself), plus `coverly dedupe` in the CLI.

### Owner's decisions (approved) vs. what this spec assumes

| Decided by the owner | Assumed here (confirm in review) |
|---|---|
| 3 levels: exact copy · same game, other version · same disc game, other format | Levels chain into one group per game; each extra carries its own relation to the keeper (§5.4) |
| Keeper rules: region order, verified dump, latest revision, preferred container per device, already has a cover | Two more rules ranked above them: **has a real save** and **user's own pick**; one tie-breaker: **named like the catalogue** (§6) |
| Default: move extras to a quarantine outside `ROMS/`, journaled, undoable, with saves and art | Default folder is `_Duplicados/` next to the ROMs folder on the same volume (§7.1) |
| OS Trash optional, off by default, never hard delete | Trash undo is "restore by hand" and says so before you choose it (§7.2) |
| Level 2 (1G1R) opt-in, report-only by default | Level 2 also needs both copies identified by the catalogue; unknown dumps never join a level 2 group |
| Exact copies with different saves: warn, touch nothing | A blank memory card or an all-`0xFF` save counts as "no save" (§8.2) |
| Never decide by file name alone | File names are never a grouping key at any level. A name may only **split or downgrade** a group (arcade set names §5.2, hack/translation tags §5.4), never create or upgrade one; otherwise names appear only as display and as the last tie-breaker among copies proven to be the same game |
| Organise step + library action + `coverly dedupe` (dry run unless `--apply`) | Library action opens its own screen, `navStore.screen = 'duplicates'` (§10) |

## 2. Goals and non-goals

### Goals
- Read-only **planner** in `coverly-core` (`dedupe/`), shared by the app, the Organise planner and the CLI.
- Three detection levels with exact keys and confidence (§5). Exact copies survive container (zip vs raw), copier headers, N64 byte order and CHD compression.
- **Only within one system.** Arcade core folders (`FBNEO`, `MAME`, `CPS1`…) are separate systems and are never compared with each other.
- Ordered, configurable keeper rules with a typed reason per group (§6), and a per-group override that the planner remembers.
- Safe apply: plan first, revalidate every file (keeper too) before moving, never overwrite, journal + undo, pause on volume loss, resume.
- Saves, save states, cheats, patches and art travel with the extra; shared saves stay with the keeper (§8).
- Incremental: a second run over an unchanged library costs `stat` calls only (§9).
- Scales to the owner's library (91 GB, 22,605 files, 55 systems) in about a minute on SSD for a first pass.

### Non-goals (this iteration)
- **Deleting.** No code path unlinks a ROM without a journaled copy or the OS Trash.
- **Renaming** the keeper to its catalogue name. The group shows the catalogue name as a hint (§11.4); renaming stays in Organise.
- **Merging** arcade sets (split/merged/non-merged) or rebuilding them. An arcade zip is compared as a whole set of entries.
- **Converting** formats (bin/cue → CHD). Level 3 chooses among formats you already have.
- **Cross-system** dedupe, including GB vs GBC dual-mode carts in two folders. Different folder, different system, never a group.
- Folder games (`PORTS`, DOS, ScummVM), `BIOS`, `APPS`, and anything inside art or save folders.
- Downloading a parent/clone database. Level 2 uses catalogue names now; a No-Intro P/C source is a later upgrade (§5.3, open question 2).

## 3. Users and success

- **Handheld owner tidying a card** (primary): opens the library, presses **Duplicados**, reads "89 copias de más en 88 juegos. Quitarías 1,53 GB de ROMS/.", checks two groups, presses **Apartar 89 archivos**, gets `_Duplicados/` next to `ROMS/` with the same folder shape inside, and can undo in one click.
- **Organising a pile into an existing card**: the Organise proposal says "6 ya están en tu biblioteca" and sets those aside instead of copying them in.
- **Curator who wants 1G1R**: turns on "Apartar también otras versiones", reorders regions in Matching, applies.
- **Scripter**: `coverly dedupe /Volumes/SD --report json` in a cron job, `--apply` when happy.

Success on the owner's real library `~/Documents/ROMS` (§15): the dry run finds **88 exact groups / 89 extras / 1,529,567,750 bytes** in 9 systems, plus the level 3 PS groups, shows **zero** cross-system groups (584 would appear if arcade folders were compared), never pairs `Driver 2.img` with `Driver 2.bin` or `Colony Wars [Disc1of2]` with `[Disc2of2]`, and treats the blank `Crash Bash.srm` as no save.

## 4. Where it lives

| Entry | Scope | Result |
|---|---|---|
| **Organise step** | Pile items against each other and against the target system folders | Duplicates show in the proposal as their own section; applying them is part of the organise job (`JobKind::Organize`) |
| **Library action** | One library (all systems, or the systems you pick) against itself | `DuplicatesScreen`, job `JobKind::Dedupe` |
| **CLI** | `coverly dedupe <ROOT>` | Table or JSON report; `--apply` writes a `Dedupe` job |

The Organise step replaces today's `mark_duplicates` ("shortest file name wins", `organize/target.rs:700`) with the keeper rules, and fixes the content key so a headered and a headerless dump match (§5.2). Pile items that are already in the target system folder become `LeftReason::InLibrary{path, relation}` and are offered to the pile-side quarantine (`<pile root>/_Duplicados/`, §7.1), never copied in or onto the card.

## 5. Detection

### 5.1 Units and scope

The planner works on **units**, not files:

- A **single ROM** (raw file or single-entry zip).
- A **set**: a `.cue` with its tracks, a `.gdi` with its tracks, a `.ccd/.img/.sub` triple, an arcade zip (all entries), a multi-disc `.m3u` with its discs, a multi-disc `.pbp`. A set is never split, and a track that belongs to a set is never a unit on its own.
- **Excluded** before grouping: anything under `PORTS/`, `BIOS/`, `APPS/`, art folders (`art_dir_names`), `Saves/` folders, the quarantine folder itself, files the walk classes as BIOS (e.g. `PS/Grandia.bios`), partial downloads, unreadable files (reported, never fatal), and multi-game archives.
- **BIOS anywhere**, not only by folder: files whose hash or name is in the libretro `System.dat` firmware list (`scph1001.bin`, `scph5501.bin`, `scph7003.bin`, `disksys.rom`, `gba_bios.bin`, `bios7.bin`…), DAT entries with `isbios="yes"` or a `[BIOS]` name, and arcade BIOS/device sets (`neogeo.zip`, `pgm.zip`, `qsound.zip`…). Several PS BIOS files are byte-identical under different names and each core asks for its own name, so "setting aside the copy" breaks a core. They are never units.
- **Not a ROM**: zero-byte files and files whose magic contradicts their extension (an HTML error page saved as `.zip`, §15.5). Identical junk would otherwise form large "exact" groups. Reported as "No es una ROM", never grouped. AppleDouble `._*`, `.DS_Store` and hidden files are already skipped by `scan/classify.rs`; the dedupe walk uses the same predicates.
- **Links**: a symlinked file is not a unit (its target may be the keeper, and moving the target would break it); it is reported. Two paths with the same file id (hard links) are one unit, since moving one frees nothing.
- **Overlapping sets**: sets are built as a reference graph (m3u → cue → bin). Two units that reference the same file (two `.cue` sheets over one `.bin`, an m3u and a loose cue listing the same disc) are merged into one unit before grouping, so a shared track can never move away from a keeper that needs it.

**Scope key**: every key below starts with the unit's **system id**, taken from its system folder (library) or from the organise identification (pile). Two units in different systems are never compared. This is a hard rule in the grouping function, not a filter in the UI.

### 5.2 Level 1 — exact copy (`Relation::Exact`)

Same game data, whatever the name, container, header or compression.

| Unit | Key | Confidence | Acts |
|---|---|---|---|
| Raw ROM with a header rule (NES, FDS, SNES copier 512, Lynx, A7800, **SMS/GG 512** new) | `(system, sha1(headerless payload), payload size)` | 1.00 | yes |
| Raw ROM, no header rule | `(system, sha1(file), size)`; N64 normalised to big-endian z64 first | 1.00 | yes |
| Single-entry zip | candidate `(system, inner crc32, inner size)` from the central directory; confirmed by `sha1(inner payload)` (decompressed, header rule applied) | 1.00 once confirmed, 0.97 CRC-only | confirmed only |
| Raw vs zip of the same ROM | both reduce to the same payload key | 1.00 | yes |
| Arcade / multi-entry zip | `(system, sorted multiset of (crc32, size) over all entries)`; entry names ignored | 0.99 (two or more 32-bit CRCs must all agree) | yes |
| Disc set (cue/gdi/ccd) | `(system, ordered list of per-track sha1, normalised track layout)`; layout = track count, mode, `INDEX`/`PREGAP`/`POSTGAP` per track; file names, comments and `REM` lines ignored | 1.00 | yes |
| CHD | `(system, sha1)` from the CHD v4/v5 header: the combined hash of data **and** metadata (track layout), independent of compressors and hunk size; v3 uses its `sha1` | 1.00 | yes |
| CHD, same `rawsha1`, different combined `sha1` | same data, different track metadata (one may have a broken layout) | 0.97 | report-only ("Sin confirmar") |
| PBP, CSO, RVZ, ISO | `(system, sha1(file), size)`, hashed only when another unit of the same system has the same size | 1.00 | yes |
| 7z / rar / tar.gz (via the other branch's reader) | same as single-entry zip, using the reader's CRC listing | 0.97 → 1.00 after a bounded decode | confirmed only |

**Bucketing keeps it cheap.** Per system: bucket by payload size → within a bucket of two or more, compare a 128 KiB prefix CRC (`sniff::PREFIX`) → full SHA1 only for units still colliding. Zips bucket on central-directory CRC for free. A singleton bucket is never hashed. The existing `max_hash_mb` cap (64 MiB) does **not** apply to a unit that is already in a colliding size bucket; the user sees "Comparando 2 archivos de 693 MB…" instead.

**CRC-only matches** (a zip whose entry is too big to decode within the per-file budget, or an archive the reader can list but not decode) are shown as level 1 with the badge "Sin confirmar / Not confirmed" and are **report-only**.

**Functional headers are not copier headers.** The NES (iNES/NES 2.0), Lynx (`LYNX` rotation) and Atari 7800 (`ATARI7800` cart type, POKEY) headers tell the emulator how to run the game, and a wrong header gives a broken game. The payload key still groups them, but when the 16/64/128-byte headers of two members differ the group is **report-only** (`ReportOnly{HeaderDiffers}`, "Mismo juego, cabecera distinta") unless exactly one header matches the header database the identify step already uses; that member is then the only keeper candidate. A headerless NES copy is never preferred over a headered one. SNES/SMS/GG copier headers and the FDS fwNES header carry nothing a core needs and do not block.

**Arcade: the file name is the driver.** MAME and FBNeo load a set by its zip stem, so two byte-identical zips with two **valid** set names (`ffight.zip`, `ffightu.zip`; a parent and a clone that happen to share every ROM) run different drivers and are not duplicates. In arcade systems a level 1 group forms only among members whose stem is not a known set name for that system (`ffight (1).zip`, `ffight copy.zip`) plus at most one member whose stem is; that member is the forced keeper. With no arcade DAT cached, arcade groups are report-only. This is the "names may split, never join" rule of §1.

**Soft patches make a different game.** RetroArch and most frontends apply a same-stem `.ips`/`.bps`/`.ups`/`.xdelta`/`.ppf` at load time. `Game.sfc` + `Game.ips` and an unpatched `Game (1).sfc` are byte-identical files but different games. A unit's key includes the sorted SHA1s of its soft-patch companions; units with different patch sets never share a level 1 key.

A unit with a DAT hit also carries `dat_game` (system, game name). It is not part of the level 1 key: two dumps of the same DAT game with different bytes (`[b]` bad dump vs good) are level 2, not level 1.

### 5.3 Level 2 — same game, other version (`Relation::Version`)

Revisions, regions, betas and prototypes of one game. Only units with a **catalogue identity** take part: a DAT hit by hash (`method = Hash`), or a later P/C source. A unit identified only by header, extension or name never joins a level 2 group.

**Key:** `(system, title_key(dat_game), disc_no)` where `title_key` comes from a new `identify::names::parse_tags` (§12.2) applied to the **DAT game name** (never to the file name):

- Strips **known** groups: regions and languages (No-Intro words and GoodTools letters `(JUE)`), `(Rev N)` / `(Rev A)` / `(V1.1)` / `(v1.02)`, dump-quality flags `[!] [a] [b] [o]` only, `(SGB Enhanced)`, `(GB Compatible)`, `(Rumble Version)`, `(Virtual Console)`, dates, `(Beta)`, `(Proto)`, `(Demo)`, `(Sample)`, `(Kiosk)`.
- **Keeps** the disc number (`(Disc 2)` is never merged with `(Disc 1)`).
- **Keeps every unknown group** as part of the key. `Hippo Teeth (Sporty Time & Fun)` and `(Mini Time & Fun)` stay apart; an unrecognised edition tag is evidence of a different product until a P/C list says otherwise.
- **Keeps** translations, hacks and `(Unl)` / `(Pirate)` in their own class: `Heracles no Eikou (Japan)` and `(Translated En)` are not offered as one game (open question 3). GoodTools modification flags are part of that class and are **not** stripped: `[h…]` hack, `[t…]` trainer, `[T+…]`/`[T-…]` translation, `[f…]` fixed, `[p…]` pirate. They name modified programs, not worse dumps of the same one.
- Never groups DAT entries flagged as BIOS (§5.1).
- Folds case, accents, punctuation and roman numerals as `compare_key` does today.

Each unit also records `class` (retail, beta, proto, demo, sample), `revision` (parsed into a comparable `(major, minor, letter)`; GoodTools `V1.1`, No-Intro `Rev 1`, `Rev A` and `v1.02` all normalise) and `regions`.

| Evidence | Confidence |
|---|---|
| P/C source says `cloneof` (later) | 1.00 |
| Both units hit the DAT by hash and their DAT names share `title_key` | 0.90 |
| Anything else | not a level 2 group |

Level 2 is **report-only** unless 1G1R is on (`dedupe.one_game_one_rom`, `--one-game-one-rom`). Known limitation: regional retitles (`Pocket Monsters Aka` vs `Pokemon Red`) do not group until a P/C source exists.

### 5.4 Level 3 — same disc, other format (`Relation::Format`)

The same disc game as cue/bin, img, iso, chd, pbp, cso or a different PBP packaging. Container bytes differ, so only the **disc serial** can match them.

**Key:** `(system, disc serial, disc_no)`. Serials come from the disc reader in `feat/organize-disc-archives` (§7.4): `SYSTEM.CNF` `BOOT` line (PS1/PS2), `PARAM.SFO` `DISC_ID` (PBP, PSP), `IP.BIN` product number (Saturn, Sega CD, Dreamcast), `UMD_DATA.BIN`, CHD metadata or first hunks. Serials are normalised (`SLUS_005.43` → `SLUS-00543`).

Rules:
- A **multi-disc unit** (m3u, multi-disc PBP) is keyed by its ordered serial list. Two units match only when the lists are equal. A full set vs a lone disc of it is shown as "Juego incompleto / Partial set", report-only.
- Different serials are never merged, even with the same name and size class: `Driver 2.img` (SLUS-01161) and `Driver 2.bin` (SLUS-01318) are discs 1 and 2, not copies (§15).
- When the cached Redump DAT maps the serial (schema v3, §12.3) to game names with **different** `title_key`s, the group is report-only (some demo and compilation discs reuse serials).
- Without a serial (CHD that the reader cannot open, CDI), the unit only takes part in level 1.
- **`disc_no` must be known.** Saturn, Sega CD and Dreamcast reuse one product number on every disc of a game; the disc number comes from the `IP.BIN` device field (`CD-ROM2/3`, `GD-ROM1/2`), PS from `PARAM.SFO` / the m3u position. A unit whose `disc_no` is unknown takes part in level 3 only when the DAT says that serial has a single disc; otherwise level 1 only.

**A serial names a product, not a build.** Fan translations, undubs, LibCrypt/PPF-patched images and most hacks keep the original `SYSTEM.CNF`, so they share its serial; so do many reprints and revisions (`Greatest Hits`, `Platinum`, `Rev 1`). A level 3 match therefore also needs a **volume fingerprint** read through the same disc reader: the ISO 9660 primary volume descriptor's volume space size and creation/modification timestamps, plus the SHA1 of the disc's own boot file (`SYSTEM.CNF` for PS1/PS2, `IP.BIN` for Sega, `PSP_GAME/PARAM.SFO` inside a UMD image). Never the outer EBOOT `PARAM.SFO` of a PS1 PBP: converters rewrite its title and ID, so it would split real copies. These sit in the first sectors of the data track, survive any container (bin, img, iso, chd, pbp, cso) and change with a remaster or a re-mastered translation.

- Same serial list **and** same fingerprint (or every member hits the same Redump entry by hash): the group acts.
- Same serial, different or unreadable fingerprint: report-only, `ReportOnly{BuildDiffers}`, "Mismo número de serie, contenido distinto (¿otra revisión o un parche?)". With 1G1R on, it is treated as a level 2 group.
- **Names can only veto.** If any member's file name carries a modification tag (`[T+…]`, `[h…]`, `Hack`, `Translated`, `Traducido`, `Undub`, `(Patched)`, a same-stem `.ppf`), the group is report-only. A tag never makes anything act.

| Evidence | Confidence |
|---|---|
| Same serial list + same volume fingerprint, Redump DAT agrees on one game | 0.97 |
| Same serial list + same volume fingerprint, no DAT row for it | 0.90 |
| Same serial, different fingerprint; partial set; conflicting DAT names; modification tag | report-only |

Level 3 acts by default, like level 1 (owner's decision), within these limits.

### 5.5 Building groups

1. Build level 1 clusters (exact copies collapse into one **copy cluster**).
2. Join clusters that share a level 3 key.
3. If 1G1R or reporting of level 2 is on, join by level 2 key.
4. A **group** is one connected component, inside one system. Each member records its relation to the keeper: the **weakest link** on the path (Exact < Format < Version).
5. Groups with one member are dropped.

Example: `PS/Crash Bash.bin` and `Crash Bash 1.bin` are one copy cluster (same SHA1); `Crash Bash.PBP` shares serial `SCUS-94570`, so the group has three members. If the PBP is kept, both `.bin` files are "Otro formato"; if `Crash Bash.bin` is kept, `Crash Bash 1.bin` is "Copia exacta" and the PBP is "Otro formato".

## 6. Choosing the keeper

### 6.1 Rules

Rules compare members **pairwise in order**; the first rule that tells two members apart decides. Every rule can be reordered or turned off in Settings, except **user's pick** (always first) and the **tie-breaker** (always last).

| # | Rule id | Prefers | Applies to |
|---|---|---|---|
| 0 | `user` | The copy you chose for this group before (remembered by group content, §9.3) | all |
| 1 | `has_save` | The copy with a real save or state (§8.2) | all |
| 1b | `played` | The copy the frontend says you played: `gamelist.xml` `<playcount>` / `<lastplayed>` / `<favorite>` (profiles with a gamelist); a hint that a save may live off the card (§8.2) | all |
| 2 | `in_library` | A copy already in a system folder over a pile copy (Organise step only) | all |
| 3 | `verified` | DAT hash hit > unknown; good dump > `[b]`/`[o]`; for copier headers (SNES, SMS/GG) headerless > headered. Never prefers a headerless NES/Lynx/A7800 copy (§5.2). Modified programs (`[h]`/`[t]`/`[T]`/`[f]`) are never in the same group as their original, so this rule never weighs them | all |
| 4 | `retail` | Retail > beta > proto > demo/sample | level 2 |
| 5 | `region` | First match in `settings.matching.regions` (`rom` entry skipped) | level 2 (level 1/3 copies share regions; ignored when equal) |
| 6 | `revision` | Highest revision | level 2 |
| 7 | `container` | Earliest in the profile's container order (§6.3) | all |
| 8 | `cover` | Has art in at least one of the profile's slots | all |
| 9 | `catalogue_name` | File stem equals the DAT game name (normalised) — only among members proven to be the same game (level 1, or level 3 with a matching volume fingerprint); never in level 2 | levels 1 and 3 |
| 10 | `tie` | Oldest modified time, then path order. Marked as a tie in the UI | all |

`has_save` is above everything automatic because moving the copy you play away from its save is the worst outcome of a dedupe. `catalogue_name` is the only place a file name is read to choose a keeper, and only among copies already proven to be the same game.

**A remembered pick never outranks a new save.** `user` sits above `has_save` only for a pick made in this session. A remembered pick (§9.3) is dropped, and the group flagged "Tu elección anterior ya no vale: la otra copia tiene ahora una partida", when a non-keeper has a real save the keeper lacks, or any member's save changed since the pick. When you pick, in the session, a keeper that sends a real save into the quarantine, the tray row warns inline: "Su partida {save} se irá con ella a {folder}; el juego que se queda no la verá."

### 6.2 The reason

The planner returns a typed `KeepReason` with the rule and the two values that decided it, never prose:

```rust
pub enum KeepReason {
    UserChoice,
    HasSave { path: PathBuf },
    Played { playcount: u32 },
    InLibrary,
    Verified { kept: DumpQuality, other: DumpQuality },
    Retail { other: ReleaseClass },
    Region { kept: Region, other: Region },
    Revision { kept: String, other: String },   // as written: "V1.1", "Rev 1"
    Container { kept: Container, other: Container },
    HasCover,
    CatalogueName { name: String },
    Tie,
}
```

The UI localises it into one contrastive line, naming what the others had: "Se queda: región preferida (Mundo); la otra es Japón." When the runner-up lost on a later rule as well, only the deciding rule is shown; the full ranking is in the row's tooltip.

### 6.3 Container order per profile

`dedupe.container_order` maps a profile id (or `*`) and a system id (or `*`) to an ordered list. Defaults:

| Profile | Discs | Cartridges |
|---|---|---|
| `anbernic-stock`, `garlic`, `onion`, `muos`, `minui` | `chd > pbp > cue > iso > img > bin` | `raw = zip` (no preference; set `zip > raw` to save space) |
| `es-de`, `batocera`, `retroarch` | `chd > cue > pbp > iso > img > bin` | `raw = zip` |
| any, arcade systems | — | `zip` only (never prefer an unzipped set) |

"No preference" means the rule does not decide, and the next rule does.

## 7. Actions

### 7.1 Set aside (quarantine, default)

Extras move to `<quarantine>/<system folder>/<rel_dir>/<file name>`, keeping the library's own shape so a manual restore is obvious. Companions and art follow into the same relative places (`_Duplicados/GB/Imgs/Tetris (JUE) (V1.0) [!].png`).

**Default location:** `_Duplicados` (es) / `_Duplicates` (en, chosen once from the app language and then fixed in settings) **next to the ROMs folder**:

| Library | ROMs folder | Quarantine | Writer root |
|---|---|---|---|
| `/Volumes/SD` (`roms_root = ROMS`) | `/Volumes/SD/ROMS` | `/Volumes/SD/_Duplicados` | `/Volumes/SD` |
| `~/Documents/ROMS` opened as system folders (`roms_root = ""`) | `~/Documents/ROMS` | `~/Documents/ROMS _Duplicados` | `~/Documents` |

- The writer uses one root that holds both (organise spec "option B"), so ROM moves, art moves and `gamelist.xml` edits share one journal and stay instant renames on one volume.
- If the parent is not writable, is a volume list (`/Volumes`) or a system root, the quarantine falls back to `<library root>/_Duplicados` and the planner adds it to the walk's skip list and the scan's ignored folders.
- A **custom folder** (`dedupe.quarantine_dir`, `--quarantine`) may be anywhere. On another volume the writer uses root = quarantine and source root = library: moves become verified copies + delete (organise `copy_into`), the space check runs, and `gamelist.xml` edits are skipped with a note ("La lista de juegos de {system} no se ha actualizado").
- **Pile extras stay on the pile's side.** In the Organise step, a pile item that is already in the library (or loses to another pile item) moves to `<pile root>/_Duplicados/`, on the pile's volume, as an instant rename. It is never copied onto the card just to be set aside, which would spend card space on a file the user does not want there. Library extras use the library-side quarantine.
- **Same volume frees nothing on that volume.** Moving to `/Volumes/SD/_Duplicados` takes 1,53 GB out of `ROMS/` (shorter game lists) but the card is exactly as full. Every count of bytes says **where** they leave ("Quitarías 1,53 GB de ROMS/"), never "liberar" alone; the done state adds "La tarjeta sigue igual de llena hasta que vacíes {folder} o lo muevas a tu ordenador" when the quarantine shares the volume with the library and that volume is removable. The `Cambiar…` picker suggests a folder on the computer for removable libraries.
- **Cross-volume preflight.** Before the first move to a quarantine on another volume, the planner probes the destination file system and marks items that cannot land there as `SkipReason::DestinationLimit` (never a mid-copy failure): files over 4 GiB on FAT32 (PS2 ISOs, large CHDs), names with characters FAT/exFAT/NTFS reject (`: * ? " < > |`, trailing dot or space), paths over the destination's length limit, and collisions that only exist on a case-insensitive destination (`Tetris.gb` vs `tetris.gb`; `Collision::Rename` compares with the destination's case rule). Free space is checked against the sum of the items that will copy.
- **Verified copy means read back.** On a cross-volume move the copy is fsynced and re-hashed from the destination before the source is unlinked (organise `copy_into`); a mismatch removes the copy and leaves the source.
- The quarantine folder is **always** skipped by Organise walks, library scans, detection and later dedupe runs.
- Each job writes `<quarantine>/<job id>.txt` (journaled `Created`): one line per extra, `kept → set aside`, with the reason. It is for people who look at the folder without Coverly.

### 7.2 Send to the Trash (opt-in)

`dedupe.action = "trash"` / `--trash`. Off by default.

- Uses the `trash` crate (new dependency, platform API, no shelling out). Each file is journaled as a new op `Trashed { from }` with its fingerprint **before** the call.
- **Undo cannot restore automatically** on every OS. The undo report lists each trashed path with "Recupéralo desde la Papelera (Volver a poner)" / "Restore it from the Trash (Put Back)". This is said in the setting's help text and in the confirm dialog before the first trash apply.
- On a removable volume, the Trash lives on the card (`.Trashes`) and frees no space until emptied; the confirm dialog says so. If the volume has no Trash (FAT on some systems), that item fails and the file stays.
- The extra's travelling companions and art (§9.1) go to the Trash with it, each journaled as its own `Trashed`; shared-stem companions stay, as with the quarantine.
- Older builds see `Trashed` as `Unknown` and mark the job not undoable. That is the intended safe reading.

### 7.3 Report only

No file action. Applies to: level 2 without 1G1R, CRC-only matches, partial disc sets, conflicting serials, conflicts (§8.3), groups you excluded, and all of a dry run.

## 8. Safety

### 8.1 Hard rules (enforced in core, tested)

1. Never compare across systems (§5.1).
2. Never group on a file name. No key in §5 reads `RomEntry.stem`, the zip name or the inner entry name.
3. Never touch the keeper. Apply revalidates the keeper (exists, same size and mtime) **before** moving any extra of its group; if the keeper changed or vanished, the whole group is skipped (`SkipReason::KeeperChanged`).
4. Never set aside every member of a group. The planner asserts one keeper per group; apply checks it again.
5. Never split a set; move it all-or-nothing (`apply_set`).
6. Never overwrite in the quarantine: `Collision::Rename` gives `" (2)"`.
7. Never hard delete. The only unlink of a ROM is the source delete after a verified cross-volume copy.
8. Exact copies with different saves: no action for the whole group (§8.3).
9. Revalidate every extra (size + mtime) before moving; changed → `ChangedSincePlan`, left alone.
10. Everything below `PORTS/`, `BIOS/`, `APPS/`, art and save folders is out of scope; BIOS files are out of scope wherever they are (§5.1).
11. Never move a file another unit references (overlapping sets are merged, §5.1) and never move the target of a symlink.
12. Never act on a group where a file name signals a modification, functional headers differ, or a disc build differs (§5.2, §5.4): report-only.
13. Never write a quarantine copy that the destination file system cannot hold; preflight, then skip with a reason (§7.1).

### 8.2 What counts as a save

A **save** is a companion with a save extension (`.sav .srm .rtc .eep .sra .fla .mpk .dsv .mcd .mcr .state .state1… .state.auto`, plus any in `organize.companion_exts` marked as save) with the unit's stem in the same folder or in the profile's save folders, **including per-core subfolders** (`saves/<core>/<stem>.srm`, `states/<core>/<stem>.state*`, RetroArch "sort by core").

- **Saves keyed by serial or title, not stem** (DuckStation `memcards/<serial>.mcd`, PPSSPP `SAVEDATA/<serial>*`, MAME `nvram/<set>/`) are shared by every member of a level 3 group and always stay.
- **Saves that live off the scanned tree.** Some profiles keep saves on the device's internal storage (muOS internal, stock firmwares writing beside the frontend). The profile declares `save_locations`; when any is outside the library root, `has_save` cannot see them, so the screen shows once per system: "Este dispositivo puede guardar partidas fuera de la tarjeta. Si juegas a {system}, comprueba qué copia usas antes de apartar." and the `played` rule (§6.1) is the only evidence.

A save is **blank** (counts as "no save") when it is all `0x00` or all `0xFF`, or when it is a PS1 memory card with the `MC` header and no used directory frames. The owner's library has three identical blank PS cards (`Crash Bash.srm`, `Tekken 3.srm`, `Iron fist 3.srm`, md5 `36c002e5…`, 128 KiB, 193 non-zero bytes): they must not trigger a conflict and must not decide `has_save`.

### 8.3 Conflicts

A group where two or more members have **real saves with different content** is a conflict:

- Shown as its own block, warning icon, "No se tocará nada". Each save is listed with its size and modified date (mono) and an "Abrir carpeta" link, so you can decide by hand which copy you play.
- No keeper radio, no destination, not counted in the apply total.
- The CLI lists it under `conflicts` and exits 0 (a dry run that finds conflicts is still a success).
- If the saves are byte-identical, the group is not a conflict: the keeper keeps its own copy and the extra's save travels with it.

## 9. Companions, art and the journal

### 9.1 Companions

For each extra the planner collects:

- **Same-stem companions** in the same folder (`match_companions`, every dot, case-insensitive): saves, states, `.cht`, `.ips/.bps/.ups`, `.txt`, `.nfo`.
- **Art** in every profile media slot (`art::target_path`), e.g. `GB/Imgs/<stem>.png`.
- **gamelist.xml** entries when the profile has `gamelist: true`: the extra's `<game>` is removed through `Writer::write(…, OverwritePolicy::Backup)` (one `Replaced` per gamelist), restored by undo.

**Shared stems stay.** A companion whose stem also matches the keeper (or another member that stays) stays where it is. `PS/Crash Bash.srm` matches `Crash Bash.bin` and `Crash Bash.PBP`; if the PBP is kept, the save stays even though `Crash Bash.bin` moves.

Companions keep their names (dedupe never renames). A companion that fails to move is left in place and reported; it never fails the item.

**Art leaving with the extra.** When an extra takes art from a slot where the keeper has none (different stems, the keeper won on a rule above `cover`), the tray row says "La portada se va con esta copia; {keeper} se quedará sin portada" and the done state offers "Buscar portadas para {count} juegos" (a Fill run on those keepers). Dedupe itself never writes art for the keeper (hard rule 3).

**AppleDouble.** On FAT/exFAT volumes written by macOS, a `._<name>` beside the extra or its companions moves with it; it is never left orphaned and never a unit.

### 9.2 Journal and undo

- New `JobKind::Dedupe` (`write/mod.rs:221`). Header carries `root`, `source_root`, `kind`, `created_at`, plus `quarantine` (optional field).
- Ops: `Moved{from, via}`, `DirCreated`, `Created` (the `.txt` list), `Replaced{backup}` (gamelist), new `Trashed{from}`. Each with `fp` and `item`.
- `header_kind` must learn `dedupe`, and `prune_jobs_at` keeps dedupe jobs on the organise rule (`KEEP_ORGANIZE_DAYS = 90`) instead of the fill count. An older build would read the kind as `fill` and prune it early; this is acceptable only because the journal stays readable.
- `undo_last_job` (fill) skips dedupe jobs, like organise. Dedupe undo reuses `organize_undo(job, include_later)`: `later_jobs` → "Primero hay que deshacer…", `undo_jobs_in_order`, `undo_move` with fingerprint checks, `skipped_occupied` when a file has taken the original place.
- Plan saved as `jobs/<job>.plan.json`; `resume` as organise (`job_transfers`, `Writer::resume`). Pause on `DiskFull`, `Denied`, vanished root.
- CLI `undo.rs` `kind_name` gains `Dedupe`.

### 9.3 Remembered choices

`dedupe_choices` table, keyed by `(system, chosen keeper's content key)`, with the group signature (sorted member content keys) and the saves fingerprint at the time of the pick stored alongside. A later run restores your pick (`KeepReason::UserChoice`) whenever that content is still a member of the group, even after renames, moves or a new copy joining; it is dropped under the save rule of §6.1. Clearing: "Volver a aplicar las reglas" in the screen, `--reset-choices` in the CLI.

## 10. Performance and caching

- **Library path** uses the existing `quick_ids` memo (path + size + mtime → `HashOutcome`), extended with headerless SHA1 and payload SHA1 for zips (`HashOutcome` v2, old rows re-hashed lazily).
- **Content side table** `content_keys` (schema v3): `(volume_uuid, path, size, mtime_ns) → keys JSON`, plus `file_id` and a reverse index by key. `dev_t` is **not** a key: it is assigned per mount and changes every time a card is reinserted. The file id (inode) is used only as a **hint** to follow a renamed file, and only on file systems with stable ids (APFS, HFS+, ext4, NTFS file id). On FAT/exFAT the id is derived from the directory slot, changes on move and can be reused by another file, so a hit there is trusted only on the same path with the same size and mtime. A quarantine move or undo re-keys the row to the new path, as `rekey_override` does.
- **Keys from the cache never act alone across paths.** A level 1 group whose key came from a row found by file id on another path is re-confirmed (full SHA1) before apply.
- **Serials** cached in the same row (`serials`, `disc_no`).
- **Work order:** list (stat only) → size buckets → prefix CRC on colliders → SHA1 on colliders → serial reads (bounded: `SYSTEM.CNF`, `PARAM.SFO`, header blocks; never a full disc) → DAT lookups.
- **Workers:** 4, 2 on removable or network volumes (organise rule).
- **DATs:** the library action uses cached DATs and downloads missing ones up to `dedupe.max_dat_downloads` (default 12, separate from organise's 4) with progress. Offline: levels 1 and 3 still run; level 2 and the `verified`/`catalogue_name` rules are skipped and the summary says which systems had no catalogue.
- **Budget on the owner's library** (§15): ~6,200 ROM-like files under 64 MiB; only colliding buckets are hashed. The read-only scan that produced §15 took about 30 s for 91 GB with the same bucketing. Target: first run < 2 min SSD, < 10 min SD; warm run < 5 s.
- **Size buckets are not small for cartridges.** Cartridge ROMs come in power-of-two sizes, so on a 50,000-file library thousands of raw ROMs share 256 KiB, 512 KiB, 1 MiB, 2 MiB… buckets. Bucketing saves little there; what saves the reads is reuse: (1) zips use the central-directory CRC, no read; (2) raw ROMs under `max_hash_mb` reuse the CRC/SHA1 the library scan's `quick_ids` already computed (`HashOutcome` v2 adds the headerless SHA1 in the same pass, so dedupe adds **no** read for a file the scan has hashed); (3) only raw files the scan has never hashed get the 128 KiB prefix CRC. Bucket key is the **payload** size (after the header rule), not the file size.
- **Worst-case budget at 50k files** (cold cache, all raw, SD at ~40 MB/s): 50,000 × 128 KiB ≈ 6.4 GB of prefix reads ≈ 3 min, plus full reads of real colliders. The planner reports this phase with its own counter, and a warm run (no changed files) does no reads at all. Disc units need a few random sector reads each for the serial and volume fingerprint (§5.4), cached with the row.
- **Pathological groups.** A group with more than 50 members (a backup folder copied into itself, placeholder files) is shown collapsed with a folder-level summary ("GB/old/ es una copia de GB/: 3.112 archivos") and is report-only until opened and confirmed (§11.3).
- **Planning is cancellable** between units; partial results are discarded, the cache is kept.

## 11. UI and UX

### 11.1 Design direction

The duplicates screen is a **shelf and a tray**. Each group is one shelf: the copy that stays sits on the shelf, marked with the brand's shelf rule (1px `--cr-border` line with a 3px Buckram cap on the left, brand §2 "supporting motif"); the copies that will be set aside sit beneath it in a recessed **tray** (`--cr-bg` on a `--cr-surface` panel, inset 24px), each with its destination written out. You can read the whole decision at a glance: on the shelf = stays, in the tray = moves, and the line in mono says where.

That is the one memorable element. Everything else stays quiet and uses existing primitives and tokens; no new colours.

- **Colour:** neutrals carry the screen. Buckram only for the shelf cap, the selected radio, focus and the primary button. Warning amber with icon for conflicts. Red never appears except for a real error. Manila is not used (it means "Coverly applied art, still undoable"; quarantine is a different idea).
- **Slots are not reused.** A set-aside copy is not "missing art", so no dashed slot. Thumbnails, when shown, are the plain filled slot or nothing.
- **Type:** Archivo (≥ 24px) for the screen title only. Atkinson Hyperlegible Next for everything else. Atkinson Mono for file names, paths, sizes, serials and counts, with `tabular-nums`.
- **Groups are not cards.** One continuous panel per system, groups separated by hairlines; a wall of identical rounded cards would hide the shelf/tray reading.
- **Motion:** only in answer to you. Changing the keeper slides the shelf cap to the new row and moves the old keeper into the tray (`--cr-duration-base`, `easing.standard`, never spring; 0 ms with reduced motion). Apply progress is the existing `<Progress slots>`.
- **Alignment:** left-aligned everything; sizes right-aligned in a fixed mono column so freed bytes line up.

### 11.2 Organise step

In `OrganizeScreen` (`Proposal`), between `ReviewBuckets` and `LeftInPlace`:

- `OptionsBar` gains a picker **Duplicados**: "Apartar" (default) · "Solo informar" · "No buscar".
- The summary line gains "Duplicados: {n}" and "Ya en tu biblioteca: {n}".
- A **Duplicados** section renders the same `DupGroup` component as the library screen (§11.3), capped like `SystemGroups` (closed above 200 rows).
- The primary button counts set-aside files apart: "Organizar 16 archivos y apartar 3".
- `OrganizeResult` gains outcomes `quarantined` / `trashed`.

### 11.3 Library screen: `DuplicatesScreen`

Entry: Toolbar (`src/components/library/Toolbar.tsx`), an outline button **Duplicados** next to "Organizar archivos sueltos", icon-only below 1180px. After a check, the label carries the cached count ("Duplicados 89"); before the first check it reads "Buscar duplicados". `navStore.Screen` gains `'duplicates'`; `useDedupeStore` holds `planning | ready | applying | done | error`.

**Planning**

```
← Biblioteca

Duplicados                                              (Archivo 28)
/Volumes/SD/ROMS                                         (mono)

  ▢▢▢▢▢▢▢▣▣▣▣▣▣▣▣▣   Comparando huellas: 1.204 de 6.173
  Listando ✓   Tamaños ✓   Huellas …   Números de serie   Catálogos
                                                        [Cancelar]
```

Phases are a real sequence, so they are shown in order with a check when done.

**Ready (dry run)**

```
← Biblioteca                                                     ⌘R Volver a buscar
Duplicados
/Volumes/SD/ROMS

89 copias de más en 88 juegos. Quitarías 1,53 GB de ROMS/.
Es una propuesta: todavía no se ha movido nada.

┌ Qué buscar ──────────────────────────────────────────────────────────────┐
│ [Copias exactas 89] [Otros formatos 4] [Otras versiones 37 · solo informe]│   SegmentedControl, multi
│ Qué hacer con las copias:  (•) Apartar en  _Duplicados/   [Cambiar…]      │
│                            ( ) Enviar a la Papelera                       │
│ [ ] Apartar también las otras versiones (una ROM por juego)               │
└──────────────────────────────────────────────────────────────────────────┘

⚠ 1 juego tiene copias con partidas distintas. No se tocará nada en él.    [Ver]

▾ SMS  Master System                      23 juegos · 23 copias · 7,7 MB
  ─────────────────────────────────────────────────────────────────────────
  Ecco the Dolphin                                     Copia exacta   1.00
  ▌(•) Ecco the Dolphin.sms                      512 KB   Se queda
  │    Se queda: tiene portada; la otra no.
  │  ┌──────────────────────────────────────────────────────────────────┐
  │  │ ( ) Quedarme con esta  Adventures of Dolphins.sms  512 KB         │
  │  │     → _Duplicados/SMS/Adventures of Dolphins.sms                  │
  │  └──────────────────────────────────────────────────────────────────┘
  ─────────────────────────────────────────────────────────────────────────
  …
▸ GB  Game Boy                              5 juegos · 6 copias · 2,1 MB
▾ PS  PlayStation                           3 juegos · 4 copias · 1,9 GB
  ─────────────────────────────────────────────────────────────────────────
  Crash Bash  SCUS-94570                         Otro formato   0.97
  ▌(•) Crash Bash.PBP                          79,6 MB   Se queda
  │    Se queda: formato preferido en Anbernic (PBP); las otras son BIN.
  │  ┌──────────────────────────────────────────────────────────────────┐
  │  │ ( ) Quedarme con esta  Crash Bash.bin   178 MB  Otro formato     │
  │  │     → _Duplicados/PS/Crash Bash.bin                               │
  │  │     La partida Crash Bash.srm se queda con Crash Bash.PBP.        │
  │  │ ( ) Quedarme con esta  Crash Bash 1.bin 178 MB  Copia exacta     │
  │  │     → _Duplicados/PS/Crash Bash 1.bin                             │
  │  └──────────────────────────────────────────────────────────────────┘
  [ ] No tocar este juego
  ─────────────────────────────────────────────────────────────────────────

▸ Sin cambios (37)    Otras versiones, solo informe

══════════════════════════════════════════════════════════════════ sticky ═
 [Apartar 89 archivos]   Exportar informe   Cancelar          1,53 GB
```

Anatomy, top to bottom:

1. **Header**: Back, title, library root in mono, `⌘R` re-plan (reuses `OrganizeScreen` header).
2. **Summary sentence** with exact counts and bytes, then the dry-run line. Counts are in mono.
3. **Options strip**: levels as a multi-select `ToggleGroup` with live counts; the action as a `RadioGroup` with the destination path in mono and "Cambiar…" (`pickFolder()`); the 1G1R `Checkbox`. Changing an option re-groups from cache, no re-hash.
4. **Conflicts alert** (`Alert variant="warning"`), always above the groups, with "Ver" scrolling to the first conflict.
5. **Systems**: one `Collapsible` per system, trigger `SMS  Master System  23 juegos · 23 copias · 7,7 MB`. Systems with conflicts open by default; others open when total groups ≤ 50. A closed system mounts no rows; an open system with more than 200 groups renders them windowed (only the visible groups plus a margin are in the DOM), so a 50k-file library with thousands of groups stays responsive. Keyboard focus and `Tab` order follow the data, not the DOM window.
6. **Group**: title line (catalogue game name when known, else keeper stem; serial in mono for discs), relation `Badge` with icon (`Copia exacta` ⧉, `Otro formato` ◫, `Otra versión` ⟲, `Sin confirmar` ?), confidence in mono.
   - **Keeper row** on the shelf: radio checked, name, size, `Badge variant="primary"` "Se queda", and the **why line** under it.
   - **Tray** with extras: radio, name, size, relation, `→ destination` in mono, companions that travel ("+ 1 portada · + 1 partida"), and shared saves that stay ("La partida X se queda con Y").
   - **"No tocar este juego"** checkbox: moves the group to "Sin cambios".
   - Level 2 groups without 1G1R: tray rows show "Se queda también (solo informe)" and no destination; no radios until 1G1R is on.
7. **Sin cambios**: report-only and excluded groups, collapsed, with their reason.
8. **Sticky action bar** (OrganizeScreen pattern): primary verb + exact count, Export report (JSON, same schema as the CLI), Cancel, and freed bytes on the right in mono. Disabled with a reason when the count is 0.

**Change-keeper control.** Each group is a `RadioGroup` (`role="radiogroup"`, labelled by the group title): exactly one copy stays, which a radio expresses natively. **Trap to avoid:** an empty radio in a tray of "files to remove" reads like a checkbox, and people tick it to mark a file for removal, which does the opposite (it keeps that file and sends the old keeper to the tray). So every tray radio carries a visible text label, "Quedarme con esta" (`dedupe.keepThis`), and the change is confirmed in place by the moving shelf cap plus a polite live-region line "Ahora se queda {name}; {old} pasa a apartarse." with an inline "Deshacer" for 10 s. There are no per-file checkboxes for "move this one": the only per-group off switch is "No tocar este juego". Picking another row makes it the keeper, the shelf cap moves, the why line becomes "Se queda: lo has elegido tú.", and the choice is saved (§9.3). Arrow keys move within the group (native radio behaviour); `Tab` goes to the next group; `X` toggles "No tocar este juego" for the focused group. Shortcuts are listed in a `Kbd` legend under the options strip.

**Bulk actions.** Per system, in the trigger's menu (`DropdownMenu`): "No tocar {system}", "Volver a aplicar las reglas en {system}". Global, under the sticky bar's overflow: "Volver a aplicar las reglas" (drops your picks for this library), "Solo copias exactas" preset.

**Options that change the count say by how much.** Turning on "Apartar también las otras versiones" or another level shows the delta next to the control ("+37 archivos") and the primary button updates in the same frame; the change is announced in the live region. A count never grows silently at the bottom of the screen.

**Apply.** Quarantine: no confirm dialog for an ordinary plan (it is undoable in one click; brand §7 "say what will happen" is the button itself). A confirm `AlertDialog` **is** shown when the plan looks unlike a tidy-up: more than 500 files, more than 25 % of the library's ROM bytes, any pathological group (§10) the user opened and confirmed, or the quarantine on another volume (moves become copies). Its body names the count, the bytes, the largest system share ("2.940 de ellos en GB/") and the destination. Trash: `AlertDialog` with destructive action "Enviar 89 archivos a la Papelera", body naming the count, the bytes, that undo cannot restore them automatically, and the card-Trash caveat when removable. Progress: "Apartando {done} de {total}…" with `<Progress slots={16}>` and Cancel (between items).

**Done.**

```
✓ 89 archivos apartados en _Duplicados/. Deshacer está a un clic.
  Fuera de ROMS/: 1,53 GB          [Abrir _Duplicados]  [Deshacer]  [Volver a la biblioteca]
  ▸ Detalle (89)
```

`Deshacer` opens the organise undo `AlertDialog` ("Devolver 89 archivos a su sitio"). Failures turn the alert to warning: "Apartados: 87 · Sin tocar: 2", each with the reason and "No se ha movido nada más."

**Empty state** (`EmptyState`):

```
        No hay duplicados en 55 sistemas.
        Este estante está en orden.
        Otras versiones: sin catálogo para 3 sistemas (NDS, PSP, N64).   [Descargar catálogos]
```

**Error state**: `Alert variant="danger"`, what failed, then "No se ha movido nada." and a retry.

**Jobs panel** (`JobsPanel.tsx`): `JobKind = 'fill' | 'organize' | 'dedupe'`, label "Duplicados"; dedupe rows reuse the organise Resume / Undo branches; the fill undo filter excludes dedupe.

**Narrow windows** (≥ 720px app minimum): the destination line wraps under the name; size and badges move to the second line; the tray inset drops to 12px.

**Accessibility:** relation and conflict status always have an icon and a word; keeper state is the radio, not the cap colour; the why line is the radio's `aria-describedby`; focus ring per brand §6; counts announced via the existing live region when planning finishes.

### 11.4 Catalogue name hint

When the keeper's file name differs from its DAT name, the group title shows the DAT name and the keeper row adds "El catálogo lo llama {name}". No rename action here; it points to Organise's "Usar el nombre del catálogo".

### 11.5 Copy (i18n, es + en)

Keys under `dedupe.*` and `organize.dedupe.*`, every key in both locales (`parity.test.ts`). Brand §7: no exclamation marks, tú, verb + exact count, mono numbers, every error says what was not touched. Spanish uses **apartar** (set aside), never eliminar or borrar.

| Key | es | en |
|---|---|---|
| `dedupe.title` | Duplicados | Duplicates |
| `dedupe.entry.find` | Buscar duplicados | Find duplicates |
| `dedupe.entry.count` | Duplicados {count} | Duplicates {count} |
| `dedupe.phase.list` | Listando | Listing |
| `dedupe.phase.sizes` | Tamaños | Sizes |
| `dedupe.phase.hash` | Huellas | Fingerprints |
| `dedupe.phase.serials` | Números de serie | Serial numbers |
| `dedupe.phase.catalogue` | Catálogos | Catalogues |
| `dedupe.planning` | Comparando huellas: {done} de {total} | Comparing fingerprints: {done} of {total} |
| `dedupe.summary` | {extras} copias de más en {groups} juegos. Quitarías {bytes} de {folder}. | {extras} extra copies across {groups} games. {bytes} would leave {folder}. |
| `dedupe.dryRunHint` | Es una propuesta: todavía no se ha movido nada. | This is a proposal: nothing has moved yet. |
| `dedupe.level.exact` | Copias exactas | Exact copies |
| `dedupe.level.format` | Otros formatos | Other formats |
| `dedupe.level.version` | Otras versiones | Other versions |
| `dedupe.reportOnly` | solo informe | report only |
| `dedupe.action.label` | Qué hacer con las copias | What to do with the copies |
| `dedupe.action.quarantine` | Apartar en {folder} | Set aside in {folder} |
| `dedupe.action.trash` | Enviar a la Papelera | Send to the Trash |
| `dedupe.action.change` | Cambiar… | Change… |
| `dedupe.oneGameOneRom` | Apartar también las otras versiones (una ROM por juego) | Also set aside other versions (one ROM per game) |
| `dedupe.conflicts.alert` | {count} juegos tienen copias con partidas distintas. No se tocará nada en ellos. | {count} games have copies with different saves. Nothing in them will be touched. |
| `dedupe.conflicts.row` | Partida distinta: {path} | Different save: {path} |
| `dedupe.system.summary` | {groups} juegos · {extras} copias · {bytes} | {groups} games · {extras} copies · {bytes} |
| `dedupe.relation.exact` | Copia exacta | Exact copy |
| `dedupe.relation.format` | Otro formato | Other format |
| `dedupe.relation.version` | Otra versión | Other version |
| `dedupe.relation.unconfirmed` | Sin confirmar | Not confirmed |
| `dedupe.relation.partialSet` | Juego incompleto | Partial set |
| `dedupe.keeper` | Se queda | Stays |
| `dedupe.alsoStays` | Se queda también (solo informe) | Also stays (report only) |
| `dedupe.why.user` | Se queda: lo has elegido tú. | Stays: you chose it. |
| `dedupe.why.hasSave` | Se queda: tiene tu partida ({path}). | Stays: it has your save ({path}). |
| `dedupe.why.inLibrary` | Se queda: ya estaba en tu biblioteca. | Stays: it was already in your library. |
| `dedupe.why.verified` | Se queda: volcado verificado en el catálogo; {other}. | Stays: verified dump in the catalogue; {other}. |
| `dedupe.why.retail` | Se queda: versión comercial; la otra es {other}. | Stays: retail release; the other is {other}. |
| `dedupe.why.region` | Se queda: región preferida ({kept}); la otra es {other}. | Stays: preferred region ({kept}); the other is {other}. |
| `dedupe.why.revision` | Se queda: revisión más nueva ({kept}); la otra es {other}. | Stays: newer revision ({kept}); the other is {other}. |
| `dedupe.why.container` | Se queda: formato preferido en {profile} ({kept}); la otra es {other}. | Stays: preferred format on {profile} ({kept}); the other is {other}. |
| `dedupe.why.cover` | Se queda: tiene portada; la otra no. | Stays: it has a cover; the other doesn't. |
| `dedupe.why.catalogueName` | Se queda: se llama como en el catálogo. | Stays: named as in the catalogue. |
| `dedupe.why.tie` | Son idénticas; se queda la más antigua. Elige otra si prefieres. | They are identical; the oldest stays. Pick another if you prefer. |
| `dedupe.dest` | → {path} | → {path} |
| `dedupe.travels.art` | + {count} portadas | + {count} covers |
| `dedupe.travels.saves` | + {count} partidas | + {count} saves |
| `dedupe.sharedSaveStays` | La partida {save} se queda con {keeper}. | The save {save} stays with {keeper}. |
| `dedupe.catalogueCalls` | El catálogo lo llama {name} | The catalogue calls it {name} |
| `dedupe.keepThis` | Quedarme con esta | Keep this one |
| `dedupe.keeperChanged` | Ahora se queda {name}; {old} pasa a apartarse. | {name} now stays; {old} will be set aside. |
| `dedupe.why.played` | Se queda: es la que has jugado ({count} veces). | Stays: it's the one you played ({count} times). |
| `dedupe.saveLeaves` | Su partida {save} se irá con ella a {folder}; el juego que se queda no la verá. | Its save {save} goes with it to {folder}; the copy that stays won't see it. |
| `dedupe.choiceDropped` | Tu elección anterior ya no vale: la otra copia tiene ahora una partida. | Your earlier pick no longer applies: the other copy now has a save. |
| `dedupe.artLeaves` | La portada se va con esta copia; {keeper} se quedará sin portada. | The cover goes with this copy; {keeper} will have none. |
| `dedupe.offCardSaves` | Este dispositivo puede guardar partidas fuera de la tarjeta. Si juegas a {system}, comprueba qué copia usas antes de apartar. | This device may keep saves off the card. If you play {system}, check which copy you use before setting any aside. |
| `dedupe.report.headerDiffers` | Mismo juego, cabecera distinta | Same game, different header |
| `dedupe.report.buildDiffers` | Mismo número de serie, contenido distinto (¿otra revisión o un parche?) | Same serial, different content (another revision or a patch?) |
| `dedupe.report.modified` | Uno de los nombres indica un parche o traducción | One of the names marks a patch or translation |
| `dedupe.report.bigGroup` | {folder} es una copia de {other}: {count} archivos | {folder} is a copy of {other}: {count} files |
| `dedupe.notRom` | No es una ROM | Not a ROM |
| `dedupe.skip.destinationLimit` | {folder} no admite este archivo ({why}); sigue en su sitio. | {folder} can't hold this file ({why}); still in place. |
| `dedupe.delta` | +{count} archivos | +{count} files |
| `dedupe.bulk.confirmTitle` | ¿Apartar {count} archivos? | Set aside {count} files? |
| `dedupe.bulk.confirmBody` | Son {bytes}, {share} de ellos en {system}. Irán a {folder}. | That is {bytes}, {share} of them in {system}. They'll go to {folder}. |
| `dedupe.findCovers` | Buscar portadas para {count} juegos | Find covers for {count} games |
| `dedupe.skipGroup` | No tocar este juego | Leave this game alone |
| `dedupe.skipSystem` | No tocar {system} | Leave {system} alone |
| `dedupe.resetRules` | Volver a aplicar las reglas | Apply the rules again |
| `dedupe.unchanged` | Sin cambios ({count}) | Unchanged ({count}) |
| `dedupe.apply.quarantine` | Apartar {count} archivos | Set aside {count} files |
| `dedupe.apply.trash` | Enviar {count} archivos a la Papelera | Send {count} files to the Trash |
| `dedupe.apply.none` | No hay nada que apartar | Nothing to set aside |
| `dedupe.trash.confirmTitle` | ¿Enviar {count} archivos a la Papelera? | Send {count} files to the Trash? |
| `dedupe.trash.confirmBody` | Son {bytes}. Deshacer no podrá devolverlos solo: tendrás que recuperarlos desde la Papelera. | That is {bytes}. Undo can't bring them back by itself: you'll restore them from the Trash. |
| `dedupe.trash.cardNote` | En una tarjeta SD la Papelera está en la propia tarjeta; el espacio no se libera hasta vaciarla. | On an SD card the Trash is on the card itself; space is freed only when you empty it. |
| `dedupe.applying` | Apartando {done} de {total}… | Setting aside {done} of {total}… |
| `dedupe.done` | {count} archivos apartados en {folder}. Deshacer está a un clic. | Set aside {count} files in {folder}. Undo is one click away. |
| `dedupe.doneTrash` | {count} archivos enviados a la Papelera. | Sent {count} files to the Trash. |
| `dedupe.freed` | Fuera de {folder}: {bytes} | Out of {folder}: {bytes} |
| `dedupe.sameVolumeNote` | La tarjeta sigue igual de llena hasta que vacíes {quarantine} o lo muevas a tu ordenador. | The card is just as full until you empty {quarantine} or move it to your computer. |
| `dedupe.openFolder` | Abrir {folder} | Open {folder} |
| `dedupe.partial` | Apartados: {done} · Sin tocar: {skipped} | Set aside: {done} · Left alone: {skipped} |
| `dedupe.skip.keeperChanged` | El archivo que se queda ha cambiado; no se ha movido nada de este juego. | The file that stays has changed; nothing in this game was moved. |
| `dedupe.skip.changed` | Ha cambiado desde la propuesta; sigue en su sitio. | Changed since the proposal; still in place. |
| `dedupe.undo.title` | ¿Devolver {count} archivos a su sitio? | Put {count} files back? |
| `dedupe.undo.action` | Devolver {count} archivos | Put back {count} files |
| `dedupe.undo.trashed` | Recupéralo desde la Papelera (Volver a poner): {path} | Restore it from the Trash (Put Back): {path} |
| `dedupe.empty.title` | No hay duplicados en {systems} sistemas. | No duplicates in {systems} systems. |
| `dedupe.empty.body` | Este estante está en orden. | This shelf is in order. |
| `dedupe.noCatalogue` | Otras versiones: sin catálogo para {count} sistemas ({systems}). | Other versions: no catalogue for {count} systems ({systems}). |
| `dedupe.error` | No se pudo comparar {path}. No se ha movido nada. | Couldn't compare {path}. Nothing was moved. |
| `dedupe.gamelistSkipped` | La lista de juegos de {system} no se ha actualizado. | The {system} game list wasn't updated. |
| `organize.dedupe.picker` | Duplicados | Duplicates |
| `organize.dedupe.quarantine` | Apartar | Set aside |
| `organize.dedupe.report` | Solo informar | Report only |
| `organize.dedupe.off` | No buscar | Don't look |
| `organize.left.inLibrary` | Ya está en tu biblioteca como {path} | Already in your library as {path} |
| `organize.applyWithDedupe` | Organizar {count} archivos y apartar {extras} | Organise {count} files and set aside {extras} |
| `jobs.kind.dedupe` | Duplicados | Duplicates |

## 12. Architecture

### 12.1 Core: `crates/coverly-core/src/dedupe/`

| File | Role |
|---|---|
| `mod.rs` | Public types: `DedupePlan`, `DupGroup`, `Member`, `Relation`, `KeepReason`, `GroupAction` (`Act`, `ReportOnly{why}`, `Conflict{saves}`), `DedupeOptions` |
| `units.rs` | Build units from a `LibraryScan` or from organise `Facts`; set detection; exclusions (§5.1) |
| `keys.rs` | Level 1 keys with bucketing; level 2 `title_key`; level 3 serial keys |
| `group.rs` | Connected components per system; weakest-link relation |
| `keeper.rs` | Ordered rules → keeper + `KeepReason`; container order |
| `saves.rs` | Save detection, blank-card check, conflict test |
| `companions.rs` | Same-stem companions, art slots, shared-stem rule, gamelist entries |
| `plan.rs` | `plan_dedupe(scan, catalogue, cache, opts, progress, cancel) -> DedupePlan` |
| `apply.rs` | Generic move run built from organise `Run` (`revalidate`, `apply_set`, `on_error` pause, cancel); `apply_dedupe`, `resume_dedupe` |

Organise integration: `organize/plan.rs` calls `dedupe::group_units` on the pile + target-system units, replacing `mark_duplicates`; pile extras become `OrganizeItem`s with `target` in the quarantine and `ItemAction::SetAside`.

### 12.2 Name tags

`identify/names.rs` gains `parse_tags(name) -> NameTags { revision: Option<Revision>, class: ReleaseClass, flags: DumpFlags, disc: Option<u8>, unknown_groups: Vec<String> }`. `ParsedName` stays as is. Used only on DAT names for level 2 and for the `verified` / `revision` / `retail` rules.

### 12.3 Cache schema v3

- `dat_roms` + `serial TEXT`, `region TEXT`; `dat.rs::parse_game` / `parse_rom` keep `serial`, `region`, and `cloneof` when present. Index `(dat, serial)`.
- `content_keys(volume_uuid, path, size, mtime_ns, file_id, keys_json, serials_json, volume_fp)` + `content_key_index(key, volume_uuid, path)` (§10: `dev_t` is never stored as a key).
- `dedupe_choices(system, keeper_key, signature, saves_fp, chosen_at, PRIMARY KEY(system, keeper_key))`.
- Migration `2 → 3` adds columns and tables; old rows stay valid.

### 12.4 Contract with `feat/organize-disc-archives`

This spec assumes that branch lands:

```rust
pub trait ArchiveReader {
    fn entries(&self) -> io::Result<Vec<ArchiveEntry>>;          // name, size, crc32: Option<u32>
    fn open(&self, entry: usize, budget: u64) -> io::Result<Box<dyn Read>>;
}
pub struct DiscId {
    pub serials: Vec<String>,
    pub disc_no: Option<u8>,              // from IP.BIN device field, PARAM.SFO, m3u position
    pub volume: Option<VolumeFingerprint>, // PVD space size + timestamps, sha1 of the disc's own boot file (§5.4)
    pub source: DiscIdSource,
}
pub fn read_disc_id(path: &Path, system: SystemId) -> io::Result<Option<DiscId>>;
```

If it is not merged when dedupe is built: zip only through the existing reader; level 3 disabled with "Otros formatos: necesita leer discos (próxima versión)"; 7z/rar units are name-only and never grouped. If it lands without `volume`, level 3 groups are found but stay report-only (`BuildDiffers` cannot be ruled out).

### 12.5 App

- `src-tauri/src/dedupe/mod.rs`: `DedupeManager` (pattern of `OrganizeManager`), events `DEDUPE_PROGRESS_EVENT`, `DEDUPE_CHANGED_EVENT`.
- IPC: `dedupe_plan(root, opts)`, `dedupe_cancel`, `dedupe_set_keeper(group, member)`, `dedupe_skip_group(group, skip)`, `dedupe_reset_choices(root)`, `dedupe_apply(plan_id)`, `dedupe_resume(job)`; undo through `organize_undo`.
- UI: `src/screens/DuplicatesScreen.tsx`, `src/components/dedupe/{DedupeOptions,SystemSection,DupGroup,KeeperRow,TrayRow,ConflictGroup,DedupeResult}.tsx`, `src/stores/dedupeStore.ts`, `src/lib/dedupe.ts` (reason → text), types in `src/ipc/types.ts`, mock backend.

## 13. Settings

New `settings.dedupe` (`settings/model.rs`), shown as a **Duplicados** block in `LibrarySection` under the organise block:

| Key | Default | UI |
|---|---|---|
| `levels` | `["exact", "format"]` act, `"version"` report | three switches |
| `one_game_one_rom` | `false` | switch "Una ROM por juego" |
| `action` | `"quarantine"` | radio: Apartar / Papelera |
| `quarantine_dir` | `null` (= next to the ROMs folder, §7.1) | mono input + "Elegir…" (SafetySection pattern) |
| `keep_rules` | `has_save, played, in_library, verified, retail, region, revision, container, cover, catalogue_name` | reorderable list (MatchingSection pattern), each with on/off |
| `container_order` | §6.3 | per-profile list editor, collapsed |
| `exclude_systems` | `["ports", "bios", "apps"]` | multi-select |
| `max_dat_downloads` | `12` | advanced |

Region order is **not duplicated**: the `region` rule shows "Usa el orden de Coincidencias" with a link to `MatchingSection`. Validation in `settingsRules.ts`: no duplicate rules, `quarantine_dir` not inside a system folder and not inside `ROMS/`.

## 14. CLI

```
coverly dedupe <ROOT> [--profile ID] [--system GB,SFC…]
               [--apply | --dry-run]               default: dry run (also when safety.dry_run_default is false)
               [--level exact,format,version]       default: settings.dedupe.levels
               [--one-game-one-rom]
               [--quarantine DIR | --trash]
               [--keep-rules has_save,verified,region,revision,container,cover]
               [--keep PATH…]                       force a keeper (repeatable)
               [--reset-choices] [--offline]
               [--report table|json] [--output FILE]
               [--resume JOB] [--data-dir DIR] [--cache-dir DIR]
```

Dry run is the default regardless of `safety.dry_run_default`: dedupe writes only with `--apply`. Exit codes follow `main.rs`: 0 ok (conflicts included), 1 I/O or job error, 2 bad usage, 3 root not found, 4 no profile detected and none given, 5 paused. `coverly undo <JOB>` handles dedupe jobs.

Table report:

```
PS  PlayStation                                    3 games  4 copies  1.9 GB
  Crash Bash  SCUS-94570                          other format   0.97
    stays  Crash Bash.PBP        79.6 MB  preferred format on anbernic-stock (PBP)
    →      Crash Bash.bin       178.0 MB  other format  → _Duplicados/PS/Crash Bash.bin
    →      Crash Bash 1.bin     178.0 MB  exact copy    → _Duplicados/PS/Crash Bash 1.bin
           save Crash Bash.srm stays with Crash Bash.PBP
…
89 extra copies across 88 games, 1,529,567,750 bytes. Dry run: nothing moved.
```

JSON report: `DedupePlan` serialised (same schema as the app's "Exportar informe"), with `version: 1`.

## 15. Real data (read-only scan, 2026-10-18)

Source: `~/Documents/ROMS` (91 GB, 22,605 files, 55 systems) and `~/Documents/ROMs-Downloaded`. Read with a throwaway script using the same keys as §5.2 (headers removed, N64 normalised, zips by inner CRC, discs by size then SHA1, serial regex over the first 8 MB). Nothing was written to either folder. The implementation must reproduce these numbers in the `#[ignore]` real-sample test.

### 15.1 Level 1, inside one system

| System | Groups | Extras | Bytes freed |
|---|---|---|---|
| SMS | 23 | 23 | 7,734,272 |
| FC | 17 | 17 | 5,243,280 |
| SFC | 14 | 14 | 22,283,264 |
| MD | 14 | 14 | 12,058,624 |
| GBA | 10 | 10 | 53,842,560 |
| GB | 5 | 6 | 2,129,920 |
| GG | 2 | 2 | 1,048,576 |
| PS | 2 | 2 | 563,272,656 |
| DREAMCAST | 1 | 1 | 861,954,598 |
| **Total** | **88** | **89** | **1,529,567,750** |

(Bytes assume the largest member stays; the real keeper rules may change the total slightly.)

**Re-measure after this review.** The scan stripped NES iNES headers without comparing them. Under §5.2 an FC group whose two iNES headers differ is report-only, so the acting FC count may fall below 17 (and the bytes with it). The real-sample test asserts the **found** totals above and records the acting totals from a fresh dry run; any FC group that drops to report-only is listed with its two headers in the evidence.

Cases the tests must cover:

- **Only found with headers removed** (7 SFC groups): `F-1 Grand Prix Part III.smc` = `F-1 Grand Prix 3.smc`; `Slayers.smc` = `Bahamut Lagoon.smc`; `Great Battle 4.smc` = `The Great Battle 4.smc`; `Super Bomberman-Panic Bomber W.smc` = `Super Bomber Man World.smc`; plus `Spider-Man-Lethal Foes`, `F-1 Grand Prix`, `The Great Battle V`. Keeper rule `verified` prefers the headerless copy.
- **Misleading names, same bytes**: `GBA/234 Golden Nugget Casino.gba` = `786 Caesar's Palace Advance.gba`; `MD/Duck Hunt.bin` = `213 Quack Shot Starring Donald Duck.bin`; `MD/D&D Waterdeep … Maps.bin` = `241 D&D - Warriors of the Eternal Sun.bin`; `FC/189 Bugs Bunny Blowout.nes` = `327 Happy Birthday Bugs.nes`; `FC/404 Mission Impossible 1.nes` = `465 Rescue-The Embassy Mission.nes`; `DREAMCAST/Soul Calibur.cdi` = `Soulcalibur 2[J].cdi`; `PS/Ridge Racer 1.PBP` = `Ridge Racer 4.PBP` (SLUS-00797, Type 4); 21 SMS pairs with translated titles (`Ecco the Dolphin.sms` = `Adventures of Dolphins.sms`, `Cool Spot.sms` = `Fido Dido.sms`). The `catalogue_name` rule and the §11.4 hint exist for these.
- **Collection prefixes**: `GBA/799 Defender.gba` = `752 Defender.gba`; `GBA/071` = `278 MegaMan & Bass.gba`; `FC/035` = `474 Rolling Thunder.nes`; `MD/087` = `385 Arcus Odyssey.bin`. Usually a `tie`; the UI says so.
- **Tags that don't change bytes**: `GB/… (SGB Enhanced).gb` vs untagged (Iron Man X-O, James Bond 007, Hercules); `GB/Hyper Lode Runner (World) (Rev A).gb` = `(Rev 1).gb` (so a revision in a **file name** is not evidence: level 1 wins over level 2); three-way `GB/Mega Man 1.gb` = `Mega Man I.gb` = `Mega Man - Dr. Wily's Revenge (USA).gb` (`catalogue_name` keeps the last).
- **Covers inside these groups**: 62 groups no art at all, 24 with art on one of two members, 1 with art on both, 1 with art on one of three. The `cover` rule decides 25 groups.

### 15.2 Never offered

- **584 cross-system groups** of byte-identical arcade sets: FBNEO+MAME 438, FBNEO+MAME+VARCADE 67, FBNEO+VARCADE 38, CPS2+FBNEO 22, CPS1+FBNEO 11, MAME+VARCADE 7, CPS3+FBNEO 1 (e.g. `CPS1/ffight.zip` = `FBNEO/ffight.zip`). Test invariant: zero groups span two systems.
- **Same name, different content**: `SFC/Tetris 2.zip` (`Tetris 2 (USA) (Rev 1).sfc`, CRC `3BC7054A`) vs `GB/Tetris 2 (1).zip` (`Tetris 2 (USA, Europe) (SGB Enhanced).gb`, `EA19A9D9`); `SFC/Tetris Attack.zip` vs `GB/Tetris Attack (1).zip`; 337 names shared across systems with different content; 20 names inside one system with different content (`PCE/Taito Chase H.Q. (Japan)` vs `(USA)` — level 2 only if both hit the DAT; SFC `Jungle Book`, `Blues Brothers`, `King of Dragons`, `Great Circus Mystery` — different dumps, no catalogue hit, never grouped).
- **Multi-disc**: `PS/Driver 2.img` (SLUS-01161) vs `Driver 2.bin` (SLUS-01318); `Colony Wars [Disc1of2]` (SLUS-00543) vs `[Disc2of2]` (SLUS-00554); `Xenogears (Disc 1)` (SLUS-00664) vs `(Disc 2)` (SLUS-00669).
- `PS/Grandia.bios` (BIOS next to `Grandia.PBP`, SLPS-02124); everything in `PORTS/` (55 of the 63 save files found are under `PORTS/owlboy/gamedata/`).

### 15.3 Level 3 (PS)

| Serial | Members | Expected keeper (anbernic-stock) |
|---|---|---|
| SCUS-94570 | `Crash Bash.PBP` 79.6 MB · `Crash Bash.bin` = `Crash Bash 1.bin` 178 MB (same SHA1) | `Crash Bash.PBP` (container); both `.bin` set aside; blank `Crash Bash.srm` stays (shared stem) |
| SCUS-94194 | `GranTurismo1.PBP` 454 MB · `Gran Turismo.img` 693 MB | `GranTurismo1.PBP` (container) |
| SLPS-01300 | `Tekken 3.PBP` 504,223,641 B · `Iron fist 3.PBP` 504,238,976 B | `Tekken 3.PBP` (`catalogue_name`); blank `Iron fist 3.srm` travels |

These keepers hold only if the members' volume fingerprints match (§5.4). The two Tekken 3 PBPs differ by 15,335 bytes; if that is only EBOOT packaging (icon, outer `PARAM.SFO` title) the fingerprint matches and the row stands; if the disc data differs the group is report-only `BuildDiffers`. The test records which.

24 `.chd` files gave no serial from a byte scan; they need the CHD reader (§12.4) for level 3, and use `rawsha1` for level 1.

### 15.4 Level 2 (report-only by default)

`ROMs-Downloaded/ROMS/GB/Tetris (JUE) (V1.0) [!].zip` (CRC `63F9407D`, DAT `Tetris (Japan) (En)`) and `(V1.1) [!].zip` (`46DF91AD`, DAT `Tetris (World) (Rev 1)`): both `title_key = tetris`, same system. With 1G1R on, V1.1 stays (`region`: World before Japan; `revision` agrees), and V1.0 moves with `GB/Imgs/Tetris (JUE) (V1.0) [!].png`.

### 15.5 `ROMs-Downloaded`

- None of its 16 valid ROMs is already in `ROMS/` by content, so the Organise step reports "Ya en tu biblioteca: 0".
- `Tetris Plus.zip`, `Next Tetris, The.zip`, `Next Tetris, The_ On-Line Edition.zip` are 7z (magic `377ABCAF271C`), and `3-D Tetris (USA).zip` is a 5,900-byte HTML page: skipped as units until the archive reader lands, never fatal.

## 16. Testing

All fixtures are synthetic and generated in tests; no ROM data is committed.

- **Keys** (`dedupe/keys.rs`): NES with/without iNES header → same key; SNES `.smc` 512-byte copier vs `.sfc` → same; SMS/GG 512-byte header; N64 `.v64`/`.n64`/`.z64` → same; raw vs single-entry zip (stored and deflate) → same after SHA1 confirm; zip whose entry exceeds the budget → `Unconfirmed`, report-only; arcade zips with renamed entries → same; arcade zips differing in one entry → different; CHD v5 header with equal `rawsha1` and different compressors → same; cue/bin with different cue text, same tracks → same.
- **Scope**: two identical files in `GB/` and `GBC/` → no group; identical arcade zips in `FBNEO/` and `MAME/` → no group (property test over random system pairs).
- **Names never group**: `SFC/Tetris 2.zip` and `GB/Tetris 2 (1).zip` with different synthetic payloads → no group; same folder, same stem, different bytes, no DAT → no group; `Tetris 2.zip` and `Tetris 2 (1).zip` in one folder with the same bytes → exact group (the content decides, the `(1)` is irrelevant).
- **Level 2** (mock DAT): `Tetris (Japan) (En)` + `Tetris (World) (Rev 1)` → group, V1.1 stays, report-only unless 1G1R; `(Disc 1)` vs `(Disc 2)` → no group; `(Sporty Time & Fun)` vs `(Mini Time & Fun)` → no group; `(Translated En)` vs `(Japan)` → no group; one member without a DAT hit → no group; `parse_tags` table for `V1.1`, `Rev 1`, `Rev A`, `v1.02`, `[!]`, `[b1]`, `(Beta)`, `(Proto 2)`.
- **Level 3** (fake `DiscId` provider): PBP + bin same serial → group; disc 1 vs disc 2 serials → no group; m3u (2 discs) vs lone disc 1 → partial set, report-only; serial mapped to two titles in the DAT → report-only; no serial → level 1 only.
- **Keeper**: one test per rule, with the expected `KeepReason` and its values; rule order changes the winner; disabled rule skipped; `user` choice survives a rename (signature); `tie` reason when everything equal; container order per profile, `*` fallback, arcade zip-only.
- **Saves**: all-zero, all-`0xFF`, blank PS card (`MC` header, empty frames) → no save; real saves equal → not a conflict, extra's save travels; real saves different → conflict, nothing in the plan acts; shared-stem save stays with the keeper (Crash Bash shape).
- **Apply/undo** (tempdirs): quarantine path mirrors `rel_dir`; art in `Imgs/` follows; gamelist entry removed and restored; collision in the quarantine → `(2)`; keeper changed after plan → group skipped; extra changed → `ChangedSincePlan`; set moved all-or-nothing; cross-volume fallback via injected `EXDEV`; disk full → pause + resume; undo restores ROM, companions, art, gamelist; undo with a later job refuses unless `include_later`; trash op journaled, undo reports manual restore (trash backend injected); pruning keeps dedupe jobs 90 days; older journals still parse; `header_kind("dedupe")`.
- **Exclusions**: files under `PORTS/`, `BIOS/`, `Imgs/`, `Saves/`, the quarantine folder → never units; Organise walk and library scan skip the quarantine.
- **Adversarial review cases** (§5–§10): `PS/scph1001.bin` = `PS/scph5501.bin` (synthetic bytes, real names) → no group; `neogeo.zip` copies → no group; two zero-byte `.gba` and two identical HTML pages named `.zip` → "No es una ROM", no group; symlink → target pair → no unit for the link, target never moved; two `.cue` over one `.bin` → one unit; NES pair with equal PRG/CHR and different iNES mapper → report-only `HeaderDiffers`; NES headered vs headerless → headered stays; `ffight.zip` vs `ffightu.zip` identical with a mock arcade DAT → no group; `ffight.zip` vs `ffight (1).zip` → group, `ffight.zip` forced keeper; arcade with no DAT → report-only; `Game.sfc`+`Game.ips` vs `Game (1).sfc` → no group, and with equal `.ips` → group; CHD equal `rawsha1`, different combined `sha1` → report-only; cue/bin with equal tracks and a different `INDEX 01` → no level 1 group; DAT names with `[h1]`/`[T+Eng]` vs clean → no level 2 group; level 3 same serial + different PVD timestamp → `BuildDiffers`; PBP with rewritten outer `PARAM.SFO` + same disc → group; same serial with `[T+Spa]` in a name → report-only; Saturn two discs sharing a product number with `disc_no` unknown → no level 3; RetroArch `saves/<core>/<stem>.srm` found; DuckStation `memcards/<serial>.mcd` stays; remembered pick + new save on the other copy → pick dropped; FAT32 destination with a 4.2 GiB ISO → `DestinationLimit` before any copy; `Game:1.gb` to exFAT → `DestinationLimit`; `Tetris.gb` + `tetris.gb` (case-sensitive source) to a case-insensitive quarantine → `(2)`; content cache on FAT with a reused file id at another path → not trusted; `played` rule from `gamelist.xml`; >500-file plan → bulk confirm; 60-member group → collapsed report-only until confirmed; pile extra lands in `<pile>/_Duplicados`.
- **Organise step**: pile item equal to a library ROM under another name → `InLibrary`, set aside; pile pair with header/headerless → one group (regression for `raw_crcs.first()`); `mark_duplicates` removed, no "shortest name" behaviour left (test asserts keeper ≠ shortest name when rules say so).
- **Performance** (`#[ignore]`, CI-local): 10,000 synthetic files with 2% duplicates plan with `hash_rom` calls ≤ colliders; warm second run hashes 0 files; a moved file keeps its cache row (dev+inode).
- **CLI** (`assert_cmd`): dry run writes nothing (tree hash before/after, also with `safety.dry_run_default = false`); `--apply` creates the quarantine; `--trash` with an injected backend; `--report json` matches the schema; `--keep PATH` overrides; `coverly undo` restores.
- **App**: Vitest for `dedupeStore` (plan → set keeper → skip group → apply → undo), `DupGroup` radio semantics and keyboard, reason texts for every `KeepReason`, i18n parity, contract fixtures; E2E on the mock backend through Chrome DevTools MCP (screenshots light/dark, 720px and 1280px).
- **Real sample** (`#[ignore]`, `COVERLY_SAMPLE_DIR` and `COVERLY_LIBRARY_DIR`): **plan only** against both folders; asserts §15.1 totals per system, zero cross-system groups, the §15.2 "never offered" list, the §15.3 PS groups once the disc reader exists. Apply is tested on a copy under `target/` (`cp -X`), never on the originals.

## 17. Milestones (input for the implementation plan)

| # | Deliverable |
|---|---|
| D1 | `identify/names.rs::parse_tags` + tests; `dat.rs` keeps `serial`, `region`, `cloneof` |
| D2 | Cache schema v3 (`dat_roms` columns, `content_keys`, `dedupe_choices`); `HashOutcome` v2 with headerless + payload SHA1; SMS/GG header rule |
| D3 | `dedupe/units.rs` + `keys.rs`: units, exclusions, level 1 bucketing and keys (incl. CHD `rawsha1`) |
| D4 | `group.rs` + level 2 + level 3 (behind the `DiscId` contract, disabled if absent) |
| D5 | `keeper.rs`, `saves.rs`, `companions.rs`; `plan.rs` with snapshots |
| D6 | Journal: `JobKind::Dedupe`, `Trashed` op, pruning, `header_kind`; `dedupe/apply.rs` (generic run from organise), resume, undo wiring; `trash` crate behind a trait |
| D7 | Organise integration: replace `mark_duplicates`, `InLibrary`, set-aside items, quarantine skip in walk/scan |
| D8 | CLI `coverly dedupe` + `undo` kind |
| D9 | Tauri `DedupeManager`, IPC, events, mock backend, contract |
| D10 | `DuplicatesScreen`, `components/dedupe/*`, Organise section, Toolbar entry, Jobs panel kind, settings block, strings es/en |
| D11 | Real-sample dry run evidence (§15), E2E, tracker update |

## 18. Open questions

1. **Keeper rule order.** `has_save` above `verified` and `region` (proposed) — or strictly the owner's list first?
2. **Parent/clone source.** Add the No-Intro P/C XML (or Retool clone lists) as an optional download so level 2 groups regional retitles and reaches confidence 1.00?
3. **Translations and hacks in level 2.** Keep them in their own class (proposed), or let 1G1R set aside a translation when the original is present?
4. **Quarantine name.** `_Duplicados` fixed at first run from the app language (proposed) vs always `_Duplicates`.
5. **Quarantine on a broad parent.** For `~/Documents/ROMS`, the default lands in `~/Documents/ROMS _Duplicados`. Acceptable, or always inside the library root?
6. **Zip vs raw on handhelds.** Leave "no preference" (proposed) or prefer `zip` to save space on stock firmwares that load zips?
7. **Organise default.** Should the Organise step set aside duplicates by default (proposed: yes, they are pile files) or only report them?
8. **Trash Put Back.** Confirm per OS which `trash` crate backend supports Finder/Explorer "Put Back" before wording `dedupe.undo.trashed`.
9. **Card space.** A same-volume quarantine on an SD card frees no space (§7.1). Default stays on the card (instant, undoable) with the honest wording — or default to a folder on the computer for removable libraries, accepting slower copies?
10. **Level 3 strictness.** The volume fingerprint (§5.4) will turn some true copies (a PBP whose converter re-mastered the ISO) into report-only. Accept the false negatives, or allow acting on serial + Redump single-entry agreement alone when no member's name carries a modification tag?
11. **Bulk confirm thresholds.** 500 files / 25 % of ROM bytes (§11.3) are guesses; tune after the real-sample run.
