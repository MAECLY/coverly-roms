# Desktop App (Tauri shell, IPC and UI state)

> **TL;DR.** The desktop app is a thin Tauri 2 shell over `coverly-core`. Opening a library scans it once into a compact `LibraryView` that the webview browses entirely in memory (filters, facets, sort, folder tree are pure TypeScript). The backend keeps the paths: the webview asks for art, identification and "reveal" by `(generation, entry id)`, never by path. Fills run as one backend job at a time (dry run → review → apply, pausable and resumable); only the entries a job touched are re-scanned and pushed to the webview. Settings (M5) are one typed `settings.json` edited from a Settings screen, applied live (theme, language, log level) or on the next open/job; see [settings.md](./settings.md). In a plain browser the same UI runs on a deterministic mock backend. Spec: [§6, §7, §8, §9, §11](../superpowers/specs/2026-10-15-coverly-roms-design.md). Code: `src-tauri/src/`, `src/ipc/`, `src/stores/`, `src/lib/`, `src/components/library/`, `src/components/settings/`.

## Layers

| Layer | Where | Role |
|---|---|---|
| Core | `crates/coverly-core` | `scan_library_with_progress`, `fit_to_root`, `resolve_art`, `settings::LibrarySettings`, `Identifier` |
| Library layer | `src-tauri/src/library/` | `open_session` → `(Session, LibraryView)`; `read_art_bytes`, `entry_path`; `identify_entry` |
| App state | `src-tauri/src/state.rs`, `prefs.rs`, `profiles.rs` | generation counter, the open `Session`, the settings (`settings.json`, migrated from `library.json`), custom profiles, the credential store, SQLite cache, the `JobManager`, drawer and search candidates, undo/history/exports |
| Platform | `src-tauri/src/keychain.rs`, `logging.rs` | OS keychain `CredentialStore`; the file logger whose level follows the settings |
| Jobs | `src-tauri/src/jobs/` | `JobManager` (one job), worker threads, `JobEvents`, wire views (`jobs/view.rs`) |
| Commands | `src-tauri/src/ipc.rs` | one-liners over the two layers above; blocking work on the blocking pool |
| Backend adapter | `src/ipc/` | `Backend` interface; `tauriBackend` (invoke/listen/dialog/webview) or `mockBackend` |
| Stores | `src/stores/` | zustand: library, filters, view, selection, identity, job, settings, nav |
| Pure logic | `src/lib/` | search folding, filters + facets, scopes, sort, folder tree, keyboard, formatting, art URL cache |

## IPC contract

JS argument names are camelCase; payload fields are Rust snake_case. The shapes are pinned by `src/ipc/contract.sample.json`, which both `src-tauri/tests/contract.rs` and `src/ipc/contract.test.ts` check.

| Command | Arguments | Returns | Errors |
|---|---|---|---|
| `open_library` | `path`, `profileId` (null = detect) | `LibraryView` | `not-found`, `not-a-folder`, `unreadable`, `no-profile`, `unknown-profile`, `superseded` |
| `recent_libraries` | — | `RecentView[]` (with `available`) | — |
| `forget_recent_library` | `path` | `RecentView[]` | `settings` |
| `library_settings` | — | `{ ignored_dirs }` (kept from M3; the Settings screen uses `update_settings`) | — |
| `set_library_settings` | `ignoredDirs` | `{ ignored_dirs }` (normalised) | `settings` |
| `read_art` | `generation`, `entryId`, `media` | raw bytes (`ArrayBuffer`) | `no-library`, `stale-library`, `unknown-entry`, `art-missing`, `outside-library`, `art-too-large` |
| `identify_entry` | `generation`, `entryId` | `IdentityView` (`offline` when DATs were unreachable and uncached) | `no-library`, `stale-library`, `unknown-entry`, `io` |
| `reveal_entry` | `generation`, `entryId` | `null` | `no-library`, `stale-library`, `unknown-entry`, `outside-library`, `io` |
| `app_info` | — | `AppInfo` | — |
| `start_fill` | `generation`, `request` (`FillRequest`: `entry_ids` or null, `media`, `strategy`, `regions`, `overwrite`) | `JobView` (`gathering`) | `no-library`, `stale-library`, `job-running`, `bad-request` |
| `fill_job` | — | `JobView` or `null` | — |
| `fill_job_items` | `job`, `filter` (`all`/`auto`/`review`/`failed`) | `JobItemView[]` | `no-job`, `stale-job` |
| `decide_fill` | `job`, `decisions` (`DecisionIn[]`: `{ index, choice: accept/pick/skip, candidate? }`) | `JobView` | `no-job`, `stale-job`, `bad-request` (not planned, unknown item, not a review item, bad candidate) |
| `run_fill` | `job` | `JobView` (`applying`) | `no-job`, `stale-job`, `bad-request` (not planned), `job-running` |
| `resume_fill` | `job` | `JobView` (`gathering`/`applying`) | `no-job`, `stale-job`, `bad-request` (not paused), `job-running` |
| `cancel_fill` | `job` | `JobView` | `no-job`, `stale-job` |
| `candidate_art` | `job`, `index`, `candidate` | raw bytes (decoded, re-encoded preview) | `no-job`, `stale-job`, `bad-request`, `network`, `bad-image` |
| `undo_last_job` | `generation` | `UndoView` (`job`, `removed`, `restored`, `kept`, `entries`) | `no-library`, `stale-library`, `job-running` (gathering, applying or paused), `nothing-to-undo`, `io` |
| `job_history` | — | `JobHistoryView[]` (newest first) | `io` |
| `entry_candidates` | `generation`, `entryId`, `media` | `CandidateView[]` | `no-library`, `stale-library`, `unknown-entry`, `bad-request`, `job-running`, `network` |
| `entry_candidate_art` | `generation`, `entryId`, `media`, `candidate` | raw bytes | as above + `bad-image` |
| `set_entry_art` | `generation`, `entryId`, `media`, `candidate` | `EntryArtUpdate[]` | as above + `write-denied`, `disk-full`, `name-invalid`, `volume-gone` |
| `set_entry_art_from_file` | `generation`, `entryId`, `media` (the picker opens in Rust) | `EntryArtUpdate[]` or `null` (cancelled) | as above + `bad-image` (not an image, over 20 MiB) |
| `remove_entry_art` | `generation`, `entryId`, `media` | `EntryArtUpdate[]` | `no-library`, `stale-library`, `unknown-entry`, `art-missing`, `job-running`, `write-denied`, `volume-gone` |
| `export_audit` | `generation`, `format` (`csv`/`json`; the save dialog opens in Rust) | file name or `null` (cancelled) | `no-library`, `stale-library`, `io` |
| `export_job_report` | `job`, `format` | file name or `null` | `no-job`, `stale-job`, `bad-request` (the job has not run yet), `io` |
| `get_settings` | — | `SettingsView` (`settings`, `notice`: `migrated`/`reset-corrupt`/`fixed`/`future-version` or null, `backup`, `fixed`) | — |
| `update_settings` | `patch` (JSON merge patch of groups) | `SettingsUpdateView` (`settings`, `errors: FieldError[]`, `reopen`); emits `settings-changed` | `settings` (cannot save) |
| `reset_settings` | — | `SettingsView` (defaults, recents kept, backup made) | `settings` |
| `open_config_folder` / `open_logs_folder` | — | `null` (folder chosen in Rust) | `io` |
| `open_link` | `link` (`repo`/`license`/`notices`/`trademarks`/`releases`/`issues`) | `null` | `io` |
| `cache_usage` / `clear_cache` | — | bytes used / bytes freed | `job-running` (clear), `io` |
| `list_profiles` | — | `ProfileInfoView[]` (`profile`, `builtin`, `mirror_subdirs`) | — |
| `save_profile` | `profile` (`ProfileFile`), `replaces` (old id or null) | `ProfileSaveView` (`profiles`, `errors`; nothing saved when `errors` is not empty) | `bad-request` (`replaces` is not a custom profile), `io`, `settings` |
| `delete_profile` | `id` | `ProfileInfoView[]` | `bad-request` (built-in), `unknown-profile`, `io` |
| `preview_profile` | `profile`, `example` (`system_folder`, `rel_dir`, `file_name`) | `ProfilePreviewView` (`paths`, `errors`) | — |
| `credential_status` | — | `{ available, set: { key: bool } }` (never a value) | `keychain-*` |
| `set_credential` / `clear_credential` | `key` (closed set), `value` (set only) | `CredentialStatus` | `bad-request` (unknown key), `keychain-unavailable`, `keychain-locked`, `keychain-failed` |
| `search_entry_art` | `generation`, `entryId`, `media`, `query` | `CandidateView[]` | `no-library`, `stale-library`, `unknown-entry`, `bad-request` (empty, over 120 chars, control chars), `job-running`, `network` |
| `entry_search_art` | `generation`, `entryId`, `media`, `candidate` | raw bytes | as above + `bad-image` |
| `set_entry_art_from_search` | `generation`, `entryId`, `media`, `candidate` | `EntryArtUpdate[]` (the file keeps the ROM's name; the manual identity is stored) | as `set_entry_art` |
| `clear_identity_override` | `generation`, `entryId` | `null` | `no-library`, `stale-library`, `unknown-entry`, `io` |

Event `library-scan-progress` → `{ generation, done, total, folder, roms }`: once before the first system folder, then after each.

Job events: `fill-job-progress` → `{ job, phase (gather/apply), done, total, label }` (throttled, ~20 a second, first and last always); `fill-job-changed` → `JobView` on every state change; `library-entries-updated` → `{ generation, entries: EntryArtUpdate[] }` after a job or a drawer action wrote (the UI ignores another generation).

Settings event: `settings-changed` → `Settings` after every saved update or reset (the log level is applied at the same time).

Errors are `{ "code": "<kebab-case>", "detail"?: … }`; the UI shows `errors.<code>` from `locales/{en,es}/ui.json` (`src/ipc/errors.ts`). Codes are stable.

## Generations

Every `open_library` takes a new generation. A newer open supersedes an older one still scanning (`superseded`; the UI ignores it), and calls that name an older generation get `stale-library`. A failed open keeps the library that was already open.

## Fill jobs

State machine (`src-tauri/src/jobs/mod.rs`):

```text
gathering → planned | paused(gather) | cancelled | failed
planned   → applying (run) | cancelled
applying  → done | paused(apply) | cancelled | failed
paused    → gathering | applying (resume) | cancelled
```

- **One job at a time.** A new `start_fill` is refused (`job-running`) while a job gathers or applies; otherwise it replaces the job (its journal stays for Undo) and older ids become `stale-job`. Drawer writes take the same writer slot, so a drawer write and a job never write at once.
- **Events.** Progress is throttled (~20/s, first and last always). After each write batch the job emits `written` (the touched entries are re-scanned and pushed as `library-entries-updated`) **before** `changed`, so the grid is up to date when the panel shows the new totals.
- **Decisions** are recorded on a `planned` job before Run (`decide_fill`). The app writes the `auto` items plus the review items the user accepted or re-picked; undecided review items are skipped as `skipped-review` (D20). The CLI keeps D14.
- **Pause and resume.** A rate limit, going offline or a full/read-only/vanished volume pauses the job with a reason (`paused.kind`: `rate-limit`, `offline`, `volume`). Resume after an apply pause finishes the not-attempted items into the **same journal** (`Writer::resume`), so one Undo reverts both runs (D21); resume after a gather pause re-runs the dry run (cached candidates) and clears decisions.
- **Undo** = the newest not-undone journal **of the open library** (`list_jobs` + `undo_job`), refused with `job-running` while a job gathers, applies or is paused. Files the user changed after the job are kept and counted as `kept`.
- **Crash safety.** Worker phases are guarded: a panicking worker ends the job `failed` instead of leaving it busy forever.
- **Re-scan of touched entries.** `subset_scan` re-scans only the folders of the written entries and `resolve_art` re-resolves their art; `refresh_entries` installs a new `Session` for the **same** generation and returns `EntryArtUpdate`s (no full re-open, no new generation).

## Security

- The webview never sends a path except the folder the user picked. Art and reveal resolve paths from the scan, canonicalise them and refuse anything outside the canonical library root (symlinks included) — `outside-library`. Art over 20 MiB is refused.
- The image picker and the save dialog run in Rust (`DialogExt`); the webview never sends a path. Picked files are read up to 20 MiB and always decoded and re-encoded. Candidates are addressed by `(job, item, candidate)` or `(generation, entryId, media, candidate)`.
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
| `settingsStore` | `settings`, load `notice`/`backup`/`fixed`, per-field `errors`, `reopen`, `saveError`; `update(patch)` numbers each save and ignores answers older than one already applied |
| `navStore` | which screen shows (library/start or Settings) and the open Settings section |
| `jobStore` | the current `JobView`, progress, error, history, last undo, `pending`; mirrors `fill-job-*` events (a command result older than an event is dropped) and applies `library-entries-updated` to the library store |

`useVisibleEntries()` derives scope → filters → sort plus the facet counts with memoised pure functions; the filter input is deferred (`useDeferredValue`) so typing stays responsive on 12k entries.

## Settings screen

`src/screens/SettingsScreen.tsx`: tabs General, Library, Art, Matching, Sources, Network, Safety, Advanced, Profiles, About (`src/components/settings/`). Each field saves on change, or on blur for typed values. An invalid value keeps what was typed and shows `settings.field.<code>` next to it while the other fields still save. Scan-affecting changes with a library open show "Reopen the library to apply these changes." with "Reopen now". Theme (`<html data-theme>`) and language (`<html lang>`, `I18nProvider` + `pickLocale` for `auto`) apply live. Job events keep flowing while the screen is open. Credentials are write-only inputs. Profiles shows the built-ins read-only, with Duplicate, and an editor with a live path preview. About shows the version, the licence, the no-telemetry note and fixed links. The fill wizard takes its defaults from the settings (media, strategy, regions). With overwrite `ask` nothing is preselected. For a layout that reads `gamelist.xml`, the wizard notes that it is updated (with a backup).

## Mock adapter

`getBackend()` picks Tauri when `window.__TAURI_INTERNALS__` exists and lazily imports `src/ipc/mock/mockBackend.ts` otherwise (its own chunk). The mock serves seeded libraries from `generateLibrary` (mulberry32): `/mock/Fixture SD (12k)` (12,000 entries, 18 systems, mixed cover/screenshot/title, hostile names, dates over 3 years), `/mock/Small library` (240), `/mock/Empty library` (0), an unavailable `/Volumes/EJECTED` recent, and `not-found` for anything else. `pickFolder()` opens the 12k fixture; dropping a folder named `X` opens `/mock/X`. Art is an escaped SVG per entry; identify answers hash for `id % 3 == 0`, offline for `id % 3 == 2`.

Fill jobs in the mock (`src/ipc/mock/mockJobs.ts`, deterministic): an entry/media is `auto` when `(id + mediaIndex) % 10 < 7`, `review` when `7` or `8` (`8` = an unreleased-variant note), `none` when `9`; an `auto` item of an entry with `id % 13 === 0` fails on apply with `['no-art', 'network', 'decode', 'name-invalid'][id % 4]`; undecided review items are skipped; each write is journaled (`mock-0001`…) so Undo restores the previous marks, refused while a job runs or is paused and only for journals of the open library. Options: `pauseAfter` (the first apply of each job pauses with a rate limit after this many items; default 25, `null` = never) and `pick` (what "From file…" picks: `image`, `cancel`, `not-image`). Exports are downloaded through Blob URLs (`coverly-job-<id>.csv/json`, `coverly-audit.csv/json`). Each subscription is its own listener, like Tauri's `listen`, so React StrictMode's double connect keeps its events. Settings, custom profiles and credential presence persist in `localStorage` (`coverly.mock.settings`, `coverly.mock.profiles`, `coverly.mock.credentials` with booleans only). The validation rules are the TypeScript mirror of the Rust ones. Search by title answers three regional variants of the typed title.

## Performance

| Measure | Result |
|---|---|
| Real library `~/Documents/ROMS` via `open_session` (release, cold / warm) | 367 ms / 140 ms (scan 176 / 90 ms), 7,473 entries in 34 systems (55 folders), 3,080 without cover, 1,777,055 bytes of JSON, 56 progress events |
| Generated 12,000-ROM tree (release) | 151 ms (2,913,204 bytes of JSON) |
| 12k fixture in Chrome: grid cells mounted while scrolling | 48 max (budget 120); table rows 42 max (budget 60) |
| 12k fixture in Chrome: frame time while scrolling 120 × 600 px (avg / p95) | production build 16.6–19.4 / 20–30 ms; `pnpm dev` build 22–32 / 42–58 ms (React development mode) |

The real open is far below the 3 s threshold for streaming entries per system, so entries keep arriving in one `LibraryView` (D17). The scroll benchmark jumps 600 px per frame, so every frame mounts a fresh set of tiles and starts their art loads; normal wheel scrolling is lighter. The frame budget (avg ≤ 20 ms, p95 ≤ 50 ms) holds for the production bundle the app ships; the development build pays React's dev-mode checks.

Evidence (screenshots, logs, console and network dumps) is listed in the tracker under "Evidencia M3", "Evidencia M4" and "Evidencia M5".
