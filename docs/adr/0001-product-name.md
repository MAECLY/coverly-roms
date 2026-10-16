# ADR-0001: Product name: Coverly ROMs

## Status

Accepted

## Date

2026-10-15

## Context

The project needs a public name before the repository, the crates, the bundle identifier and the trademark policy are created, because all of them carry it. The name must say what the app does (art for ROM libraries), must not use a third-party mark (Nintendo, Sega, libretro, EmulationStation, Skraper…), and must work for a small family of artefacts: the desktop app, a CLI and a library.

## Decision

The product is called **Coverly ROMs**, published by MAECLY.

| Artefact | Name |
|---|---|
| Repository | `MAECLY/coverly-roms` |
| npm package / app crate | `coverly-roms` |
| Core library crate | `coverly-core` |
| CLI crate / binary | `coverly-cli` / `coverly` |
| Bundle identifier | `com.maecly.coverlyroms` |

The name, wordmark and mark are covered by [`TRADEMARKS.md`](../../TRADEMARKS.md), as for Pluryprint and Versorium.

## Consequences

### Positive

- Short, descriptive and free of third-party marks; `coverly` is a natural CLI command.
- One naming scheme across repo, crates, binary and bundle id.

### Negative

- "ROMs" in the name ties the brand to emulation; a later general-purpose art tool would need another name.

### Risks / Open Questions

- The brand mark (icon) is not designed yet (spec §17, question 1).
