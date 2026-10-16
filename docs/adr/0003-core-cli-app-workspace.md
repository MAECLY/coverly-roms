# ADR-0003: Cargo workspace: core library, CLI and app

## Status

Accepted

## Date

2026-10-15

## Context

The same operations (audit, fill, list profiles and systems, update) must be available from the desktop app and headless from a terminal, for power users and scripts. Logic that lives inside Tauri commands cannot be reused by a CLI and is hard to test without a window. Three shapes were weighed in the spec: (A) a shared Rust core with a Tauri app and a CLI on top, (B) the app only, (C) a CLI with a web UI. The user chose A.

## Decision

The repository is one Cargo workspace with three members:

- `crates/coverly-core` — a pure library: systems, profiles, scan, art status, audit, and later identification, sources, plans and jobs. It **never depends on Tauri** and, until M2, contains no network code.
- `crates/coverly-cli` — the `coverly` binary (clap), a thin layer that parses arguments, calls the core and formats output.
- `src-tauri` — the Tauri app (`coverly-roms`), a thin IPC layer over the core.

Long work reports progress through a trait, and everything that touches disk or network sits behind a trait so tests can fake it.

## Consequences

### Positive

- The core is unit-tested with generated fixture libraries and no UI; the CLI gets integration tests (`assert_cmd`).
- Every feature exists headless first; the app reuses it without duplication.
- M1 shipped a useful tool (`coverly audit`) before any UI work.

### Negative

- The core API must be designed for two consumers, which costs some up-front care.

### Risks / Open Questions

- The boundary must be policed in review: no Tauri types and no UI strings in `coverly-core`.
