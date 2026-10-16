# Identification

> **TL;DR.** Before looking for art, `coverly fill` works out which game each ROM is. Non-arcade ROMs are hashed (CRC32, MD5 and SHA1 in one pass, with copier headers skipped) and looked up in the libretro-database DATs, which are downloaded on demand into the cache. Arcade sets are looked up by short name in the MAME/FBNeo DATs. When neither works, the ROM is identified by its normalised file name. Every identity records its **method** (`hash`, `arcade-dat`, `name` or `unknown`) and a **confidence**. Decision: [ADR-0006](../adr/0006-hash-then-name-identification.md). Spec: [§16.2–§16.7](../superpowers/specs/2026-10-15-coverly-roms-design.md#16-research-driven-refinements-2026-10-15). Code: `crates/coverly-core/src/identify/`.

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
| `lynx` | Atari Lynx | `LYNX` at offset 0 | 64 bytes |
| `a7800` | Atari 7800 | `ATARI7800` at offset 1 | 128 bytes |
| `plain` (default) | everything else | — | — |

- **Quick-id memo.** The result is stored in the SQLite cache under `path + size + mtime (ns)`. A second run, or a re-run after a pause, does not hash again unless the file changed.
- An unreadable file is identified by name instead of failing the job.

## DATs

`identify::dat::DatStore` downloads DATs in clrmamepro format from `https://raw.githubusercontent.com/libretro/libretro-database/master/<path>`. The DATs are fetched at runtime and **never bundled** (spec §17 q3).

- **Paths per system.** A system's `dats = [...]` list in `systems.toml` replaces the default. The default is `metadat/no-intro/<libretro>.dat`, or `metadat/redump/<libretro>.dat` for disc systems, where `<libretro>` is the system's thumbnail folder name. Two systems name their DAT differently and set it explicitly: Atari 8-bit (`metadat/no-intro/Atari - 8-bit Family.dat`) and Game & Watch (`dat/Handheld Electronic Game.dat`). The M2 end-to-end run checked every path with HEAD requests (see [fill-pipeline.md](./fill-pipeline.md#end-to-end-evidence)). DOS, EasyRPG and ScummVM have no usable content DAT: their default path returns 404, which is remembered, and they match by name. Paths with `..`, absolute paths or backslashes are refused, in overrides and in derived names.
- **Download on demand.** A DAT is fetched the first time a ROM of that system needs it. Its rows (game, ROM name, size, CRC, MD5, SHA1) go into SQLite, so each lookup is one indexed query. Bodies are capped at 64 MiB, and an empty DAT is refused.
- **Freshness.** After **30 days** a DAT is revalidated with its ETag (a 304 only refreshes the timestamp). A **404 is remembered** for the same TTL. Any other failed refresh falls back to the older rows. With no older rows, the failure is remembered for the rest of the process, so a broken DAT is not downloaded once per ROM. Rate limits and going offline are never remembered: they pause the phase, and the next run tries again.
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

## Output

`identify::Identity { parsed, canonical, method, confidence, hash }`. `canonical` is the DAT or arcade name when there is one, and it is what [art sources](./art-sources.md) try first. A `hash`/`arcade-dat` identity has confidence 1.0. A `name` identity has 0.0 here: its final confidence comes from how well the art matched (see [fill-pipeline.md](./fill-pipeline.md#plan-classes-and-thresholds)).

## Deviation: 7z

Spec §16.2 asks for the inner CRC of single-file `.7z` archives. Reading 7z headers needs a heavy dependency, so in M2 `.7z` ROMs are identified **by name** (`HashOutcome::NameOnly`). Zip, the common case on handheld SDs, is covered. Tracked as decision D13.
