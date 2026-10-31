# Identification

> **TL;DR.** Before looking for art, `coverly fill` works out which game each ROM is. Non-arcade ROMs are hashed (CRC32, MD5 and SHA1 in one pass, with copier headers skipped) and looked up in the libretro-database DATs, which are downloaded on demand into the cache. Arcade sets are looked up by short name in the MAME/FBNeo DATs. When neither works, the ROM is identified by its normalised file name. Every identity records its **method** (`hash`, `arcade-dat`, `name` or `unknown`) and a **confidence**. Decision: [ADR-0006](../adr/0006-hash-then-name-identification.md). Spec: [§16.2–§16.7](../superpowers/specs/2026-10-15-coverly-roms-design.md#16-research-driven-refinements-2026-10-15). Code: `crates/coverly-core/src/identify/`. Organising a loose pile adds a second question, **which system** a file belongs to, answered with header magic (`identify::sniff`), evidence levels and content support: see [Header magic (organise)](#header-magic-organise) and [organize.md](./organize.md). Finding the same game stored twice ([dedupe.md](./dedupe.md)) reuses the hashes (payload SHA-1, `HashOutcome` v2), the DAT rows (serial, region, clone, BIOS) and a tag parser for catalogue names ([Name tags](#name-tags-duplicates)).

## Method setting

`--match` on the CLI (`IdentifyOptions::method` in the core); spec §9 default `hash-then-name`.

| Value | Arcade | Other systems | When nothing matches |
|---|---|---|---|
| `hash-then-name` (default) | short name → DAT description | hash → DAT game | the parsed file name (`name`) |
| `hash-only` | short name → DAT description | hash → DAT game | `unknown`: never matched by name, gets no art |
| `name-only` | short name → DAT description | not hashed | the parsed file name |

Arcade short-name translation can be switched off (`IdentifyOptions::arcade_translation`); the ROM is then matched by name, or stays `unknown` under `hash-only`.

## Hashing

`identify::hash::hash_rom` reads a ROM once, in 64 KiB chunks, and computes CRC32, MD5 and SHA1 together. It never writes.

- **Size cap.** Files above `hash_max_bytes` (default **64 MiB**, spec §9) are identified by name.
- **Name-only formats.** `cue`, `gdi`, `m3u`, `ccd`, `chd`, `7z`, `pbp`, `cso`, `rvz`: disc images and containers whose bytes do not match a DAT row, so they go straight to name matching.
- **Zip.** For a zip with exactly one file, the inner CRC32 and size are read from the **central directory**, without decompressing (no decompression bomb can trigger). At most 4096 entries are listed. Multi-file zips are identified by name.
- **Header rules.** A system's `header` field in `systems.toml` names a rule. When the magic matches, the header is skipped for a second set of digests, so both headered and headerless DAT rows match:

| Rule | Systems | Magic | Header length |
|---|---|---|---|
| `nes` | NES / Famicom | `NES\x1A` at offset 0 | 16 bytes |
| `fds` | Famicom Disk System | `FDS\x1A` at offset 0 | 16 bytes |
| `snes-copier` | SNES | file length % 1024 = 512 | 512 bytes |
| `sms-copier` | Master System, Game Gear | file length % 16384 = 512 | 512 bytes |
| `lynx` | Atari Lynx | `LYNX` at offset 0 | 64 bytes |
| `a7800` | Atari 7800 | `ATARI7800` at offset 1 | 128 bytes |
| `n64-order` | Nintendo 64 | boot magic in `.v64` (byte-swapped) or `.n64` (little-endian) order | none; the payload SHA-1 is taken in big-endian `.z64` order |
| `plain` (default) | everything else | — | — |

- **Payload SHA-1** (`HashOutcome` v2, `HASH_OUTCOME_VERSION = 2`). The same pass also keeps the SHA-1 of the payload: the headerless bytes, in big-endian order for N64. It is equal for the same game whatever its copier header or byte order, and it is the duplicates level 1 key. A single-entry zip gets it only in `HashMode::WithPayload` (the duplicates finder), by decoding the entry within the size cap and checking its CRC; fill and scan stay on `HashMode::Quick` (central directory only).
- **Quick-id memo.** The result is stored in the SQLite cache under `path + size + mtime (ns)`. A second run, or a re-run after a pause, does not hash again unless the file changed. Rows written under `HashOutcome` v1 are a miss and are re-hashed lazily.
- An unreadable file is identified by name instead of failing the job.

## DATs

`identify::dat::DatStore` downloads DATs in clrmamepro format from `https://raw.githubusercontent.com/libretro/libretro-database/master/<path>`. The DATs are fetched at runtime and **never bundled** (spec §17 q3).

- **Paths per system.** A system's `dats = [...]` list in `systems.toml` replaces the default. The default is `metadat/no-intro/<libretro>.dat`, or `metadat/redump/<libretro>.dat` for disc systems, where `<libretro>` is the system's thumbnail folder name. Two systems name their DAT differently and set it explicitly: Atari 8-bit (`metadat/no-intro/Atari - 8-bit Family.dat`) and Game & Watch (`dat/Handheld Electronic Game.dat`). The M2 end-to-end run checked every path with HEAD requests (see [fill-pipeline.md](./fill-pipeline.md#end-to-end-evidence)). DOS, EasyRPG and ScummVM have no usable content DAT: their default path returns 404, which is remembered, and they match by name. Paths with `..`, absolute paths or backslashes are refused, in overrides and in derived names.
- **Download on demand.** A DAT is fetched the first time a ROM of that system needs it. Its rows (game, ROM name, size, CRC, MD5, SHA1) go into SQLite, so each lookup is one indexed query. Bodies are capped at 64 MiB, and an empty DAT is refused.
- **Freshness.** After **30 days** a DAT is revalidated with its ETag (a 304 only refreshes the timestamp). A **404 is remembered** for the same TTL. Any other failed refresh falls back to the older rows. With no older rows, the failure is remembered for the rest of the process, so a broken DAT is not downloaded once per ROM. Rate limits and going offline are never remembered: they pause the phase, and the next run tries again.
- **Fields kept** (cache schema v3): besides game, ROM name, size and digests, each row keeps the DAT's `serial` (the first one, normalised; Redump discs), `region`, `cloneof` and `isbios` (`isbios "yes"` or a `[BIOS]` mark in the name). Duplicates use them for level 3 serial checks, the region rule and to keep BIOS files out of every group. The v2 → v3 migration clears every cached DAT's ETag, so each one downloads again on next use (its old rows stay as the offline fallback).
- **Lookup.** For a zip, the inner CRC and size are compared. For content hashes, the headerless digests are tried first, then the full ones: CRC + size must match, and SHA1 must also match when both sides have one. Ties resolve to the first game by name.

## Arcade

Arcade systems (`arcade = true`: MAME, FBNeo, CPS1–3, Neo Geo, HBMAME…) are **never hashed**: a set's file name *is* its identity. `DatStore::arcade_description` looks the short name (`sf2`, `mslug`) up, ignoring case, in the system's DATs: `metadat/fbneo-split/FBNeo - Arcade Games.dat` first, then `metadat/mame/MAME.dat` (or `dat/HBMAME.dat`). It returns the description, such as `Street Fighter II: The World Warrior (World 910522)`. libretro-thumbnails names arcade art by that description, so this step is required for arcade art. A hit is `arcade-dat` with confidence 1.0.

## Name normalisation

`identify::names::parse_name` turns a file stem into a `ParsedName`: a display title, regions, year, sequel numeral, a comparison `key` and a `short_key` without the subtitle. The pipeline follows Skyscraper's `getCompareTitle`, but works on Unicode characters:

1. Optionally strip a **numbering prefix**. See [Numbering](#numbering-prefixes).
2. Split on brackets: text outside `()`, `[]` and `{}` is the title; the contents of top-level `()` groups give the regions (No-Intro `(USA, Europe)`, GoodTools `(U)`, `(JUE)`) and the year. An unclosed bracket stays as text.
3. `_` → space; collapse spaces; trim separators; move `, The` to the front.
4. The key: case-, accent- and punctuation-insensitive, NFC, roman numerals II–XX as numbers (single-letter `V`/`X` only as the last word), `&` → `and`, and a leading "the" dropped.

Golden examples, from `crates/coverly-core/tests/names_golden.rs`:

| Stem | Title | Regions | Key |
|---|---|---|---|
| `Legend of Zelda, The - A Link to the Past (USA)` | The Legend of Zelda - A Link to the Past | USA | `legend of zelda a link to the past` |
| `Super Mario World (USA, Europe) (Rev 1)` | Super Mario World | USA, Europe | `super mario world` |
| `Street Fighter II' Turbo (J) [!]` | Street Fighter II' Turbo | Japan | `street fighter 2 turbo` |
| `Contra III - The Alien Wars (JUE)` | Contra III - The Alien Wars | Japan, USA, Europe | `contra 3 the alien wars` |
| `001 Golden Sun` (numbered folder) | Golden Sun | — | `golden sun` |
| `Pokémon - Ruby Version (USA, Europe)` | Pokémon - Ruby Version | USA, Europe | `pokemon ruby version` |
| `Adventures of Batman & Robin, The (USA)` | The Adventures of Batman & Robin | USA | `adventures of batman and robin` |
| `Mega_Man_X_(U)` | Mega Man X | USA | `mega man 10` |

`short_key` drops the subtitle: `Golden Sun - The Lost Age (USA)` → `golden sun`, and `Metroid: Zero Mission` → `metroid`.

### Guards

Fuzzy matching (Levenshtein on characters, 0–100, `score("aé", "ae") = 50`) only compares **compatible** names (`names::compatible`). The sequel numeral must be equal: `Street Fighter II` never matches `Street Fighter III`, and `Contra` never matches `Contra III`. Years must be equal when both names have one. `Contra` and `Contra Force` stay compatible: there the score decides. Hostile names (`../../etc/passwd`, unbalanced brackets, `İİİ, The`) never panic, and names are never used as paths. The written file name always comes from the ROM stem through the profile template.

### Numbering prefixes

Handheld packs often number ROMs (`001 Golden Sun.gba`, `12) Foo`, `7. Bar`, `3 - Baz`). `detect_numbering` decides per folder: at least 3 stems, and at least 60 % of them, carry a 1–4 digit prefix followed by text. Only then is the prefix stripped before matching, so `1942` in an unnumbered folder stays `1942`. The written art keeps the full stem (`Imgs/001 Golden Sun.png`).

### Name tags (duplicates)

`identify::names::parse_tags(name) -> NameTags` reads the tags of a **catalogue** name (the DAT game name, never a file name) for duplicates level 2 ([dedupe.md](./dedupe.md#keys)):

- `title_key`: the `compare_key` of the title plus the disc number, every unknown group and the modification tags. Known groups are stripped: regions and languages (No-Intro words, GoodTools letters such as `(JUE)`), `(Rev N)`, `(Rev A)`, `(V1.1)`, `(v1.02)`, dump flags `[!] [a] [b] [o]`, `(SGB Enhanced)`, `(GB Compatible)`, `(Rumble Version)`, `(Virtual Console)`, dates, `(Beta)`, `(Proto)`, `(Demo)`, `(Sample)`, `(Kiosk)`. `(Disc 2)` is never merged with `(Disc 1)`, and an unknown group (`(Sporty Time & Fun)`) keeps two products apart.
- `revision` (`Revision { major, minor, letter }`, comparable; `V1.1`, `Rev 1`, `Rev A` and `v1.02` all normalise) and `revision_text` as written, for the keeper's reason.
- `class` (`ReleaseClass`: retail, beta, proto, demo, sample), `flags` (`DumpFlags`: verified, bad, overdump, alternate), `modification` (`ModClass`: original, hack, trainer, translation, fixed, pirate, unlicensed: GoodTools `[h]`, `[t]`, `[T+…]`, `[f]`, `[p]` and `(Unl)` are kept, because they name modified programs), `disc`, `regions` and `unknown_groups`.

## Output

`identify::Identity { parsed, canonical, method, confidence, hash }`. `canonical` is the DAT or arcade name when there is one, and it is what [art sources](./art-sources.md) try first. A `hash`/`arcade-dat` identity has confidence 1.0. A `name` identity has 0.0 here: its final confidence comes from how well the art matched (see [fill-pipeline.md](./fill-pipeline.md#plan-classes-and-thresholds)).

## Header magic (organise)

Organising a loose pile ([organize.md](./organize.md)) also needs to know **which system** a file belongs to, not only which game. `identify::sniff::sniff(src, len, ext)` answers from the bytes: a 128 KiB prefix plus a few targeted reads (raw files seek; a deflated zip entry is inflated as a stream up to the offset, capped at 8 MiB), never the whole file. It returns header evidence, hard vetoes, an unsupported system, a not-a-ROM verdict or a container magic. The extension only settles ties the bytes cannot (Master System vs Game Gear; the WonderSwan footer is only checked for `.ws`/`.wsc`, O-D4).

Order: text first, then containers, then disc images, then cartridges, most specific first inside a family (Sega CD → 32X → Pico → Mega Drive; FDS before NES; NDS before GBA; a GBA header with its logo before SNES; a GBA header without its logo last).

| System (`id`) | Signature | Weight |
|---|---|---|
| — (not a ROM) | first 512 bytes printable UTF-8 text; `web-page` when it opens with `<!doctype`, `<html`, `<?xml`, `<head`, `<body`, `{` or `[`. Under a text-native extension (`p8`, `scummvm`, `svm`, `conf`, `bat`, `bas`, `cue`, `m3u`, `gdi`, `ccd`) only markup counts | veto |
| — (container) | `PK\x03\x04` / `PK\x05\x06` zip, `37 7A BC AF 27 1C` 7z, `MComprHD` CHD, `Rar!\x1A\x07` RAR | — |
| `saturn` / `dreamcast` | `SEGA SEGASATURN` / `SEGA SEGAKATANA` at 0 or 0x10 | 0.90 |
| `psp` | `CISO` with an uncompressed size ≤ 1.8 GB; or ISO 9660 `CD001` at sector 16 with `PSP GAME` as system or volume id | 0.85 |
| `psx` / `ps2` | `CD001` + `PLAYSTATION` system id (2048- or raw 2352-byte sectors), then `SYSTEM.CNF` from the root directory: `BOOT2` → ps2 0.95 (serial from the `BOOT2` file name, `SCES_517.19` → `SCES-51719`), `BOOT` → psx 0.95; not found → psx 0.80, or ps2 0.80 when the image is larger than any CD | 0.80–0.95 |
| `nds` | logo CRC16 `0xCF56` at 0x15C **and** header CRC16 at 0x15E over 0x000–0x15D | 0.95 |
| `gb` / `gbc` | logo prefix `CE ED 66 66 CC 0D 00 0B` at 0x104 **and** header checksum at 0x14D; CGB flag 0x143: `0xC0` → gbc 0.95 and **`gb` vetoed**, `0x80` (dual mode) → gbc 0.85, otherwise gb 0.95 | 0.85–0.95 |
| `fds` | `FDS\x1A` at 0, or `\x01*NINTENDO-HVC*` at 0 or 16 | 0.95 |
| `nes` | `NES\x1A` (iNES / NES 2.0) or `UNIF` at 0 | 0.95 |
| `n64` | `80 37 12 40` (z64), `37 80 40 12` (v64), `40 12 37 80` (n64) at 0 | 0.95 |
| `lynx` / `a7800` | `LYNX` at 0 / `ATARI7800` at 1 | 0.95 |
| `segacd` | `SEGADISCSYSTEM` at 0 or 0x10 | 0.95 |
| `sega32x` | `SEGA 32X` at 0x100, or `SEGA` at 0x100/0x101 and `MARS` in the first 0x400 bytes | 0.95 |
| `pico` | `SEGA PICO` (or ` PICO `) in the 16 bytes at 0x100 | 0.95 |
| `megadrive` | `SEGA` at 0x100 or 0x101; interleaved SMD when `len % 16384 == 512`, `AA BB` at 8–9 **and** the first block deinterleaves to `SEGA` | 0.90 |
| `mastersystem` / `gamegear` | `TMR SEGA` at 0x7FF0, 0x3FF0 or 0x1FF0: `.sms` / `.gg` → that system 0.90; otherwise the region nibble picks one at 0.70 and the other gets 0.60 | 0.60–0.90 |
| `gba` | fixed byte `0x96` at 0xB2 **and** header complement at 0xBD; with the logo prefix `24 FF AE 51` at 0x04 it is checked before SNES, without it last | 0.95 |
| `snes` | internal header at 0x7FC0 / 0xFFC0 / 0x40FFC0 (+0x200 when `len % 1024 == 512`): checksum XOR complement = `0xFFFF` with a valid map mode or ROM size → 0.95; a valid map mode under a printable title → 0.85 | 0.85–0.95 |
| `ngp` / `ngpc` | `COPYRIGHT BY SNK CORPORATION` or ` LICENSED BY SNK CORPORATION` at 0; byte 0x23 = `0x10` → ngpc | 0.90 |
| `wonderswan` / `wonderswancolor` | `.ws`/`.wsc`, `len % 65536 == 0`, footer byte `len − 16` = `0xEA`; colour bit at `len − 9` | 0.85 |
| others (PC Engine, Atari 2600/5200, MSX, …) | no reliable magic: DAT, extension and name only | — |

A deterministic random-bytes test checks that noise never produces evidence at 0.95 or more.

### Evidence levels

Every clue is an `identify::evidence::Evidence { kind, system, weight, detail }`; `detail` is data for the UI (a CRC and game name, `CGB 0xC0`, an extension), never a sentence. Levels, strongest first:

| `IdLevel` | Source | Weights |
|---|---|---|
| `hash` | CRC (zip central directory, or `hash_rom` digests) in a DAT | 1.00 in a candidate system's DAT, 0.97 in another cached DAT; arcade sets 1.00 / 0.99 confirmed, 0.70 by name |
| `header` | `sniff` | 0.60–0.95 (table above) |
| `extension` | `SystemDb::systems_for_extension` | 0.85 unique, 0.50 shared, 0.60 unique but contradicted by a decisive signature; generic extensions give none |
| `name` | tags, GBA scene groups, fuzzy title against DAT stems | 0.60 hints; 0.70 / 0.80 fuzzy |
| `online` | ScreenScraper by CRC, only when configured | 0.80 |
| `user` | the user's choice, remembered by CRC + size | 1.00 |

**Decisive signatures** (O-D5): every real dump of `gb`, `gbc`, `gba`, `nds`, `n64`, `nes`, `fds` and `sega32x` carries its signature, so a failed sniff there is negative evidence (the unique extension drops to 0.60 and the item goes to review). SNES, Mega Drive, SMS and the disc systems have legitimate dumps without the checked bytes: a failed sniff there is simply no evidence.

### Content support

A system may be placed automatically only with **content support**: a DAT hit, header evidence at 0.85 or more, a user choice, or a unique extension of a system **without** a decisive signature whose bytes were actually read (not cloud-only, not inside a 7z or CHD). Extension, name and tag bonuses alone never reach Auto, so a `.7z` or an unreadable `.gba` always asks. The full combining rules (score, margin, thresholds, vetoes) are in [organize.md](./organize.md#combine).

## Deviation: 7z

Spec §16.2 asks for the inner CRC of single-file `.7z` archives. Reading 7z headers needs a heavy dependency, so in M2 `.7z` ROMs are identified **by name** (`HashOutcome::NameOnly`). Zip, the common case on handheld SDs, is covered. Tracked as decision D13.
