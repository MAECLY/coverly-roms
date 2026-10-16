# ADR-0002: Stack: Tauri 2 + Rust + React (TypeScript)

## Status

Accepted

## Date

2026-10-15

## Context

Coverly ROMs must run on Windows, Linux and macOS, walk libraries of tens of thousands of files on slow SD cards, decode and re-encode images, and update itself. The author already maintains **Pluryprint** on Tauri 2 + Rust + React, with a working CI, release pipeline and updater that can be reused. The alternatives (Electron, a native app per OS, a web app) were compared in [`desktop-stack-updater.md`](../02-research/desktop-stack-updater.md).

## Decision

We use **Tauri 2** for the shell, **Rust** (edition 2021, MSRV 1.90) for all logic, and **React 19 + TypeScript + Tailwind 3 + zustand 5** for the UI, with Vite, Vitest, ESLint, Prettier, commitlint and husky — the same stack and tooling as Pluryprint. UI strings go only through i18n (`locales/en`, `locales/es`).

## Consequences

### Positive

- Small native bundles on three OSes; file system and image work in fast, memory-safe Rust.
- Tooling, CI and the release/updater pipeline are copied from Pluryprint instead of designed from scratch.
- Rust code is shared with a headless CLI (ADR-0003).

### Negative

- Two languages and two test runners (cargo, Vitest).
- The UI depends on each OS's web view (WebView2, WebKitGTK, WKWebView), so rendering differs slightly per platform.

### Risks / Open Questions

- Tauri 2.12.x and plugin-updater 2.14.x are pinned (spec §16.11); upgrades are deliberate.
