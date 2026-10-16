# ADR-0006: Identify ROMs by hash first, then by name

## Status

Accepted (implemented in M2; 7z inner CRC deferred — see [identification.md](../04-architecture/identification.md#deviation-7z))

## Date

2026-10-15

## Context

Art sources are keyed by canonical game names (No-Intro, Redump, MAME), while ROMs on users' cards carry any name: GoodTools codes, translations, numbered prefixes (`051 Hudson's Adventure Island.nes` on the reference SD), or arcade short names. Matching on file names alone gives wrong art for sequels and regional variants; matching on hashes alone fails for hacks and for formats with no stable hash. Skyscraper and Skraper both combine the two ([`skyscraper-architecture.md`](../02-research/skyscraper-architecture.md), [`skraper.md`](../02-research/skraper.md)).

## Decision

Identification runs in order and records the **method** and a **confidence** (0–1):

1. **Hash** — CRC32, MD5 and SHA1 in one streaming pass, with header-skip rules per system and a maximum file size; for single-file zip/7z the inner CRC is read from the archive header without decompressing. Multi-file formats (`cue`, `gdi`, `m3u`…) are identified by name instead. The hash is looked up in libretro-database DATs.
2. **Name** — normalisation in the Skyscraper style (strip brackets and revision suffixes, `_` → space, `, The` → front, numeral equivalence, a retry without subtitle) and a Levenshtein score on Unicode characters; candidates with a different sequel numeral or year are rejected.
3. **Arcade** — short name → description through MAME/FBNeo DATs.

The method is a user setting: `hash then name` (default), `hash only` or `name only`. Hashes are computed only when the method needs them and memoised by `path + size + mtime`. Extensions are compared case-insensitively.

## Consequences

### Positive

- Exact matches for clean dumps whatever their file name; sensible fallbacks for everything else.
- The confidence drives the plan: auto-apply above a threshold, review queue between thresholds.

### Negative

- Hashing large disc images over USB is slow; the size limit and the memo mitigate it.
- DATs must be downloaded and indexed before identification.

### Risks / Open Questions

- DAT licence terms (spec §17, question 3).
