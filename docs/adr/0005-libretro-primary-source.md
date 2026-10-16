# ADR-0005: libretro-thumbnails is the primary art source

## Status

Accepted (implementation: milestone M2)

## Date

2026-10-15

## Context

Users want "the big free library" of art without creating an account. The candidates were libretro-thumbnails, ScreenScraper, TheGamesDB and SteamGridDB ([`libretro-thumbnails.md`](../02-research/libretro-thumbnails.md), [`screenscraper-api.md`](../02-research/screenscraper-api.md)). libretro-thumbnails is free, needs no credentials, covers covers (`Named_Boxarts`), screenshots (`Named_Snaps`) and title screens (`Named_Titles`) for most consoles, and is keyed by No-Intro/Redump names. Its weak spot is arcade, which is keyed by long descriptions rather than MAME short names. ScreenScraper needs developer credentials and strict rate limits; SteamGridDB needs a user API key and mostly covers PC titles.

## Decision

**libretro-thumbnails is the default and primary source.** Secondary sources are optional plug-ins behind an `ArtSource` trait: SteamGridDB in v1 (user API key), ScreenScraper in v1.1, TheGamesDB considered for its hash lookup.

How we use libretro (spec §16.5):

- The system list comes from the live `thumbnails.libretro.com` root listing, whose names differ from the GitHub repositories.
- The Apache index per system and media type is parsed and cached on disk with a TTL of about two days.
- Names are sanitised the libretro way (`& * / : \` " < > ? \ |` → `_`) and compared case-insensitively.
- Fallback host: `raw.githubusercontent.com/libretro-thumbnails/<repo>/master/…`.
- One global token bucket, 4–6 concurrent requests, backoff on errors.
- Arcade short names are translated to descriptions through libretro-database `metadat/mame` and `metadat/fbneo-split` (spec §16.6).

## Consequences

### Positive

- Works out of the box with no account, which is what the primary user (a handheld owner) needs.
- One index download per system answers every lookup; images are fetched only for chosen items.

### Negative

- Coverage is uneven, mainly for arcade and recent systems, so some ROMs end with "no art found".
- We depend on the layout and naming of a third-party server we do not control.

### Risks / Open Questions

- Index format or host changes need the fallback host and a quick release.
