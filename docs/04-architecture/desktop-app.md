# Desktop App (Tauri shell, IPC and UI state)

> **TL;DR.** The desktop app is a thin Tauri 2 shell over `coverly-core`. Opening a library scans it once into a compact `LibraryView` that the webview browses entirely in memory (filters, facets, sort, folder tree are pure TypeScript). The backend keeps the paths: the webview asks for art, identification and "reveal" by `(generation, entry id)`, never by path. In a plain browser the same UI runs on a deterministic mock backend. Spec: [§6, §7, §8, §9 Library, §11](../superpowers/specs/2026-10-15-coverly-roms-design.md). Code: `src-tauri/src/`, `src/ipc/`, `src/stores/`, `src/lib/`, `src/components/library/`.

## Layers

| Layer | Where | Role |
|---|---|---|
| Core | `crates/coverly-core` | `scan_library_with_progress`, `fit_to_root`, `resolve_art`, `settings::LibrarySettings`, `Identifier` |
| Library layer | `src-tauri/src/library/` | `open_session` → `(Session, LibraryView)`; `read_art_bytes`, `entry_path`; `identify_entry` |
| App state | `src-tauri/src/state.rs` | generation counter, the open `Session`, `library.json` (ignored folders, recents), SQLite cache |
| Commands | `src-tauri/src/ipc.rs` | one-liners over the two layers above; blocking work on the blocking pool |
| Backend adapter | `src/ipc/` | `Backend` interface; `tauriBackend` (invoke/listen/dialog/webview) or `mockBackend` |
| Stores | `src/stores/` | zustand: library, filters, view, selection, identity, locale |
| Pure logic | `src/lib/` | search folding, filters + facets, scopes, sort, folder tree, keyboard, formatting, art URL cache |

## IPC contract

JS argument names are camelCase; payload fields are Rust snake_case. The shapes are pinned by `src/ipc/contract.sample.json`, which both `src-tauri/tests/contract.rs` and `src/ipc/contract.test.ts` check.

| Command | Arguments | Returns | Errors |
|---|---|---|---|
| `open_library` | `path`, `profileId` (null = detect) | `LibraryView` | `not-found`, `not-a-folder`, `unreadable`, `no-profile`, `unknown-profile`, `superseded` |
| `recent_libraries` | — | `RecentView[]` (with `available`) | — |
| `forget_recent_library` | `path` | `RecentView[]` | `settings` |
| `library_settings` | — | `{ ignored_dirs }` | — |
| `set_library_settings` | `ignoredDirs` | `{ ignored_dirs }` (normalised) | `settings` |
| `read_art` | `generation`, `entryId`, `media` | raw bytes (`ArrayBuffer`) | `no-library`, `stale-library`, `unknown-entry`, `art-missing`, `outside-library`, `art-too-large` |
| `identify_entry` | `generation`, `entryId` | `IdentityView` (`offline` when DATs were unreachable and uncached) | `no-library`, `stale-library`, `unknown-entry`, `io` |
| `reveal_entry` | `generation`, `entryId` | `null` | `no-library`, `stale-library`, `unknown-entry`, `outside-library`, `io` |
| `app_info` | — | `AppInfo` | — |

Event `library-scan-progress` → `{ generation, done, total, folder, roms }`: once before the first system folder, then after each.

Errors are `{ "code": "<kebab-case>", "detail"?: … }`; the UI shows `errors.<code>` from `locales/{en,es}/ui.json` (`src/ipc/errors.ts`). Codes are stable.

## Generations

Every `open_library` takes a new generation. A newer open supersedes an older one still scanning (`superseded`; the UI ignores it), and calls that name an older generation get `stale-library`. A failed open keeps the library that was already open.

## Security

- The webview never sends a path except the folder the user picked. Art and reveal resolve paths from the scan, canonicalise them and refuse anything outside the canonical library root (symlinks included) — `outside-library`. Art over 20 MiB is refused.
- No asset protocol, no `fs` plugin; capabilities are `core:default` + `dialog:allow-open`; the opener plugin is used from Rust only. CSP `img-src 'self' data: blob:`. Pinned by `src-tauri/tests/config_security.rs`.
- File names are hostile: React text only, `unicode-bidi: plaintext`, truncated with `title`; the mock's SVG escapes them. Only PNG/JPEG/WebP/GIF bytes become images (`sniffImageMime`), never SVG from disk.

## Stores

| Store | State |
|---|---|
| `libraryStore` | `status` (idle/opening/ready/error), `view`, search `keys`, `path`, `progress`, `error`, `recents`; `open()` ignores stale results |
| `filterStore` | query, art status, systems, extensions, regions, date range, size range |
| `viewStore` | sidebar scope (all/system/folder/date), grid/list (remembered), sort, drawer, filter panel |
| `selectionStore` | selected entry id |
| `identityStore` | identify results per `(generation, id)` |
| `localeStore` | Auto/English/Español (localStorage until M5 settings) |

`useVisibleEntries()` derives scope → filters → sort plus the facet counts with memoised pure functions; the filter input is deferred (`useDeferredValue`) so typing stays responsive on 12k entries.

## Mock adapter

`getBackend()` picks Tauri when `window.__TAURI_INTERNALS__` exists and lazily imports `src/ipc/mock/mockBackend.ts` otherwise (its own chunk). The mock serves seeded libraries from `generateLibrary` (mulberry32): `/mock/Fixture SD (12k)` (12,000 entries, 18 systems, mixed cover/screenshot/title, hostile names, dates over 3 years), `/mock/Small library` (240), `/mock/Empty library` (0), an unavailable `/Volumes/EJECTED` recent, and `not-found` for anything else. `pickFolder()` opens the 12k fixture; dropping a folder named `X` opens `/mock/X`. Art is an escaped SVG per entry; identify answers hash for `id % 3 == 0`, offline for `id % 3 == 2`.

## Performance

| Measure | Result |
|---|---|
| Real library `~/Documents/ROMS` via `open_session` (release, cold / warm) | 367 ms / 140 ms (scan 176 / 90 ms), 7,473 entries in 34 systems (55 folders), 3,080 without cover, 1,777,055 bytes of JSON, 56 progress events |
| Generated 12,000-ROM tree (release) | 151 ms (2,913,204 bytes of JSON) |
| 12k fixture in Chrome: grid cells mounted while scrolling | 48 max (budget 120); table rows 42 max (budget 60) |
| 12k fixture in Chrome: frame time while scrolling 120 × 600 px (avg / p95) | production build 16.6–19.4 / 20–30 ms; `pnpm dev` build 22–32 / 42–58 ms (React development mode) |

The real open is far below the 3 s threshold for streaming entries per system, so entries keep arriving in one `LibraryView` (D17). The scroll benchmark jumps 600 px per frame, so every frame mounts a fresh set of tiles and starts their art loads; normal wheel scrolling is lighter. The frame budget (avg ≤ 20 ms, p95 ≤ 50 ms) holds for the production bundle the app ships; the development build pays React's dev-mode checks.

Evidence (screenshots, logs, console and network dumps) is listed in the tracker under "Evidencia M3".
