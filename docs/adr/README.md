# Architecture Decision Records (ADRs)

> **TL;DR** — One file per architectural decision of Coverly ROMs, in **Michael Nygard's ADR format**. ADRs are append-only: once accepted, an ADR is never rewritten; a later ADR **supersedes** it if the decision changes.

## Index

| ID   | Title | Status | Date |
| ---- | ----- | ------ | ---- |
| 0001 | [Product name: Coverly ROMs](./0001-product-name.md) | Accepted | 2026-10-15 |
| 0002 | [Stack: Tauri 2 + Rust + React (TypeScript)](./0002-stack-tauri-rust-react.md) | Accepted | 2026-10-15 |
| 0003 | [Cargo workspace: core library, CLI and app](./0003-core-cli-app-workspace.md) | Accepted | 2026-10-15 |
| 0004 | [Licence: AGPL-3.0-only with CLA and trademark policy](./0004-license-agpl.md) | Accepted | 2026-10-15 |
| 0005 | [libretro-thumbnails is the primary art source](./0005-libretro-primary-source.md) | Accepted | 2026-10-15 |
| 0006 | [Identify ROMs by hash first, then by name](./0006-hash-then-name-identification.md) | Accepted | 2026-10-15 |
| 0007 | [Bulk art selection through pluggable strategies](./0007-pluggable-strategies.md) | Accepted | 2026-10-15 |
| 0008 | [Updater on public GitHub Releases, signed with minisign](./0008-updater-public-releases.md) | Accepted | 2026-10-15 |
| 0009 | [Two-phase fill with a SQLite cache and a content-addressed art store](./0009-two-phase-fill-with-sqlite-cache.md) | Accepted | 2026-10-16 |
| 0010 | [Safe writes, with backups and journals in the app data dir](./0010-safe-writes-backups-and-journal.md) | Accepted | 2026-10-16 |
| 0011 | [Release pipeline and the update key](./0011-release-pipeline-and-update-key.md) | Accepted | 2026-10-16 |

ADRs 0001–0004 are implemented (milestone M0). ADRs 0005–0007, 0009 and 0010 are implemented in M2 (identification, libretro-thumbnails, strategies, two-phase fill, safe writes); the SteamGridDB votes of ADR-0007's Popularity arrive with the M6 sources. ADR-0008 and ADR-0011 are implemented in M7 (release pipeline, signed updates for the app and the CLI); the real signing key, the public repository and the first published release are the owner's steps in [RELEASING.md](../../RELEASING.md).

## Format

Each ADR has these sections, following [*Documenting Architecture Decisions*](https://cognitect.com/blog/2011/11/15/documenting-architecture-decisions):

1. **Title** — four-digit ID and a short noun phrase.
2. **Status** — `Proposed`, `Accepted`, `Superseded by ADR-XXXX`, `Deprecated` or `Rejected`.
3. **Date** — ISO 8601 date the decision last changed.
4. **Context** — the forces and constraints, without the decision.
5. **Decision** — what we do, in active voice.
6. **Consequences** — what becomes easier and what becomes harder.

## Conventions

- IDs are sequential and zero-padded (`0001`, `0002`, …) and never reused.
- File names are `NNNN-kebab-case-title.md`.
- The source of each decision is the design spec ([§2 Locked decisions](../superpowers/specs/2026-10-15-coverly-roms-design.md#2-locked-decisions) and [§16 Research-driven refinements](../superpowers/specs/2026-10-15-coverly-roms-design.md#16-research-driven-refinements-2026-10-15)); the evidence behind them is in [`docs/02-research/`](../02-research/README.md).
