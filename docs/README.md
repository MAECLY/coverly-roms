# Coverly ROMs — Documentation

> Free, cross-platform desktop app and CLI that audits a ROM library in any folder layout, shows which games lack cover, screenshot or title art, and fills the gaps in bulk from free sources.

This directory holds the documentation of the **Coverly ROMs** project: research, architecture, build and release, legal information and Architecture Decision Records (ADRs). The master design is the [design spec](./superpowers/specs/2026-10-15-coverly-roms-design.md); every other page details a part of it.

---

## Quick start by audience

| You are… | Read first |
|---|---|
| A user wanting to know what this is | [Project README](../README.md) → [Design spec §1–§4](./superpowers/specs/2026-10-15-coverly-roms-design.md) |
| A contributor / developer | [Contributing](../CONTRIBUTING.md) → [System Architecture](./04-architecture/system-architecture.md) → [CI / CD](./07-build-and-release/ci-cd.md) |
| A reviewer / decision-maker | [System Architecture](./04-architecture/system-architecture.md) → [ADRs](./adr/README.md) → [Tracker](./TRACKER.md) |
| Curious about prior art | [Research index](./02-research/README.md) → [Tool landscape](./02-research/tool-landscape.md) |

---

## Locked project decisions

| Item | Choice | ADR |
|---|---|---|
| Product name | **Coverly ROMs** (`MAECLY/coverly-roms`) | [ADR-0001](./adr/0001-product-name.md) |
| Application stack | **Tauri 2 + Rust + React (TypeScript) + Tailwind + zustand** | [ADR-0002](./adr/0002-stack-tauri-rust-react.md) |
| Architecture | **Cargo workspace: `coverly-core` + `coverly` CLI + Tauri app** | [ADR-0003](./adr/0003-core-cli-app-workspace.md) |
| Licence | **AGPL-3.0-only**, §7(e) no trademark rights, contributions under a CLA | [ADR-0004](./adr/0004-license-agpl.md) |
| Primary art source | **libretro-thumbnails** | [ADR-0005](./adr/0005-libretro-primary-source.md) |
| ROM identification | **Hash → DATs, then name**, arcade short-name translation | [ADR-0006](./adr/0006-hash-then-name-identification.md) |
| Bulk selection | **Pluggable strategies**: Priority, ROM region, Popularity | [ADR-0007](./adr/0007-pluggable-strategies.md) |
| Auto-update | **tauri-plugin-updater**, minisign, public GitHub Releases, stable + beta | [ADR-0008](./adr/0008-updater-public-releases.md) |
| UI languages | English (default) + Spanish | — |
| Documentation language | English (the tracker is in Spanish) | — |

---

## Table of contents

### Design and planning

- [Design spec](./superpowers/specs/2026-10-15-coverly-roms-design.md) — purpose, scope, domain model, architecture, UI, settings, milestones
- [M0 + M1 implementation plan](./superpowers/plans/2026-10-15-m0-m1-foundation-and-audit.md) — foundation and audit, task by task
- [Tracker](./TRACKER.md) — task status with evidence (Spanish)

### 02 — Research

- [Research index](./02-research/README.md) — verified dossiers and claim counts
- [Tool landscape](./02-research/tool-landscape.md) — existing ROM art tools on macOS, Windows and Linux
- [Skraper](./02-research/skraper.md) — how the leading Windows scraper works
- [Skyscraper architecture](./02-research/skyscraper-architecture.md) and [output/config](./02-research/skyscraper-output-config.md)
- [libretro-thumbnails](./02-research/libretro-thumbnails.md) — repo layout, server, naming and sanitisation
- [ScreenScraper API](./02-research/screenscraper-api.md) — with TheGamesDB and SteamGridDB
- [Layouts](./02-research/layouts.md) — where each firmware and frontend expects art
- [Desktop stack and updater](./02-research/desktop-stack-updater.md) — Tauri 2 and GitHub Releases updates

### 04 — Architecture

- [System Architecture](./04-architecture/system-architecture.md) — containers, modules, domain model, data flow, performance

### 07 — Build & Release

- [CI / CD](./07-build-and-release/ci-cd.md) — GitHub Actions on Linux, Windows and macOS

### 09 — Legal

- [Licence](./09-legal/license.md) — what the AGPL-3.0 and the CLA mean in practice
- [Trademark](./09-legal/trademark.md) — name and logo usage policy

### ADRs

- [ADR Index](./adr/README.md)
- [0001 — Product name: Coverly ROMs](./adr/0001-product-name.md)
- [0002 — Stack: Tauri 2 + Rust + React](./adr/0002-stack-tauri-rust-react.md)
- [0003 — Cargo workspace: core, CLI and app](./adr/0003-core-cli-app-workspace.md)
- [0004 — Licence: AGPL-3.0-only](./adr/0004-license-agpl.md)
- [0005 — libretro-thumbnails is the primary art source](./adr/0005-libretro-primary-source.md)
- [0006 — Identify ROMs by hash first, then by name](./adr/0006-hash-then-name-identification.md)
- [0007 — Bulk art selection through pluggable strategies](./adr/0007-pluggable-strategies.md)
- [0008 — Updater on public GitHub Releases](./adr/0008-updater-public-releases.md)

---

## How this documentation is organised

Numbered folders give the reading order (`02-research`, `04-architecture`, `07-build-and-release`, `09-legal`); missing numbers (`01`, `03`, `05`, `06`, `08`) are filled as the project grows. Cross-references are relative links, so the docs work on disk and on GitHub. ADRs follow the Michael Nygard template (`Status`, `Context`, `Decision`, `Consequences`); to change a decision, **add a new ADR** that supersedes the old one.

## Status

**Pre-alpha · M1 (scan and audit) delivered.** `coverly audit` reports missing art on real libraries (validated read-only on an Anbernic SD with 7,473 ROMs). Nothing writes to a library yet; identification, art download and filling start in M2. The [tracker](./TRACKER.md) is the source of truth for what is done.

## Licence of this documentation

The documentation in this folder is released under the same licence as the code: **[AGPL-3.0](../LICENSE)**.
