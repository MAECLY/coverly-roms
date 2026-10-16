# ADR-0007: Bulk art selection through pluggable strategies

## Status

Accepted (implemented in M2)

## Date

2026-10-15

## Context

For one ROM and one media type, sources usually offer several images: regions (USA, Europe, Japan), sources (libretro, SteamGridDB…), community variants. In bulk the app must pick one without asking, and users disagree on what "best" means: some want a fixed source and region order, some want the art of the ROM's own region, some want the most popular image.

## Decision

The choice is a **`Strategy`** trait that returns one candidate per ROM and media. Three strategies ship, selectable in settings and in the fill wizard:

- **Priority** — a configurable order of sources and regions.
- **ROM region** — prefers the region parsed from the ROM name (No-Intro `(USA, Europe)` and GoodTools `(U)(E)(J)(JUE)`), then the configured order. Default region order: `ROM's own region > World > USA > Europe > Japan`.
- **Popularity** — SteamGridDB votes where they exist; for consoles, the variant most common across sources and regions; otherwise it falls back to Priority (decision D9 in the tracker, which answers spec §17 question 5).

Ranking is deterministic (no alphabetical tie-breaks as in mini-scraper). Strategies only choose; they never write. Their output feeds a plan (auto / review / none) that is shown as a dry run before anything is written.

## Consequences

### Positive

- New strategies are added without touching the scan, sources or writers.
- Each strategy is tested in isolation with fixed candidate lists.

### Negative

- Three strategies triple the matching cases to test and document.

### Risks / Open Questions

- Popularity has weak signal for console games; the consensus heuristic must be validated on real libraries.
