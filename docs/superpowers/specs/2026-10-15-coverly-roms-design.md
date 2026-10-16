# Coverly ROMs — Design Spec

> Status: **Draft for review** · Date: 2026-10-15 · Owner: Miguel Angel Esparza Calero (MAECLY)
> This is the master design document. Every other doc in `docs/` details a part of it.
> Working tracker (tasks + evidence): [`docs/TRACKER.md`](../../TRACKER.md).

## 1. Purpose

Coverly ROMs is a free, public, cross-platform desktop app (Windows, Linux, macOS) that:

1. **Audits** a ROM library in any folder layout (SD card of a handheld, EmulationStation/ES-DE, RetroArch, a plain folder).
2. **Detects** which ROMs have or lack each kind of art: **cover (box art), screenshot, title screen**.
3. **Adds** the missing art, by default from **libretro-thumbnails** (free, no account), and optionally from secondary sources.
4. Does it **in bulk**, choosing the art automatically with a user-selected strategy, with review of doubtful matches.
5. **Browses** the library the way it is organised on disk (folder tree like Finder/Explorer), by system, by date, with filters (has/missing art, system, extension, region…).
6. **Updates itself** from GitHub Releases, like Pluryprint.

Why it exists: the reference tools are Windows-only (Skraper, ARRM, Retro CardMaker) or need heavy setup (Skyscraper). The macOS-native tools found (mini-scraper, ROMCover, BoxArt) are young, single-purpose, and have gaps (unrecognised folders such as `NDS`/`A2600`, weak arcade matching, no audit/browser). Coverly ROMs aims to be the polished, safe, multi-platform option. Evidence: `docs/02-research/`.

### What the user said vs. what we assume

| Said by the user | Assumed (confirmed in spec review) |
|---|---|
| Audit any kind of ROM; check has/no art; add art from the big free library; bulk; Win/Linux/Mac; easy updates by publishing the repo; browser by folder/console/date with filters; settings | Layout **profiles** (Anbernic stock, Garlic, Onion, muOS, MinUI/NextUI, ES-DE/Batocera, RetroArch, Custom) |
| Public use, same licence and trademark scheme as Pluryprint/Versorium | Never overwrite existing art unless asked; backup before writing |
| Stack: React like Pluryprint | No ROMs and no art are bundled or redistributed by us |
| Bulk strategies: configurable priority, ROM region, real popularity — user picks | libretro does **not** have everything (arcade is weak) → secondary sources are optional |
| Media v1: cover + screenshot + title | Docs in English (as in Pluryprint); tracker in Spanish |
| UI languages: Spanish + English | |
| Architecture: approach A (shared Rust core + Tauri app + CLI) | |

## 2. Locked decisions

| Item | Choice | ADR |
|---|---|---|
| Product name | **Coverly ROMs** (repo `MAECLY/coverly-roms`) | ADR-0001 |
| Stack | **Tauri 2 + Rust + React/TypeScript + Tailwind + zustand** (same as Pluryprint) | ADR-0002 |
| Architecture | **Cargo workspace: `coverly-core` lib + `coverly` CLI + Tauri app** | ADR-0003 |
| Licence | **AGPL-3.0-only**, §7(e) no trademark rights, contributions under CLA | ADR-0004 |
| Primary art source | **libretro-thumbnails** (`thumbnails.libretro.com`) | ADR-0005 |
| ROM identification | **Hash (CRC32) → libretro-database DATs**, then **fuzzy name match**, arcade short-name translation | ADR-0006 |
| Bulk selection | **Pluggable strategies**: Priority, ROM region, Popularity | ADR-0007 |
| Auto-update | **tauri-plugin-updater**, minisign, `latest.json` on GitHub Releases, stable + beta channels | ADR-0008 |
| Release signing | No OS certificates initially (as Pluryprint ADR-0011) | ADR-0008 |
| Repo visibility | Private now → **public before the first release** (updater needs public assets) | ADR-0008 |
| UI languages | English (default) + Spanish | — |
| Docs language | English; `docs/TRACKER.md` in Spanish | — |

## 3. Users

- **Handheld owner** (primary): SD card from Anbernic/Miyoo/TrimUI/etc., wants every game to show its cover. Not technical. Uses macOS or Windows.
- **Frontend curator**: ES-DE / Batocera / RetroArch user with large libraries, wants covers + screenshots + titles, likes bulk and CLI.
- **Power user / scripter**: runs `coverly audit` / `coverly fill` headless.

Success = the user opens their SD card, sees at a glance what is missing, clicks one button, reviews a handful of doubtful matches, and every game has art — without breaking anything on the card.

## 4. Scope

### v1 (in)
- Open a library root; auto-suggest a layout profile; user can confirm/override.
- Scan: systems, ROM entries (single file, multi-file sets `.cue/.bin`, `.m3u`, `.gdi`, folder games), art status per media type.
- Browser: folder tree view, by-system view, by-date view, flat list; grid/list; filters and search; ROM detail panel.
- Identify + match against libretro-thumbnails; arcade name translation; review queue.
- Bulk apply with strategy, overwrite policy, dry-run plan, backup, undo of last job.
- Manual per-ROM: pick another candidate, search by title, set local image file, remove art.
- Reports: export audit as CSV/JSON.
- Settings (§9), i18n en/es, light/dark theme.
- Secondary sources: **SteamGridDB** (needed for the Popularity strategy; user API key). ScreenScraper is designed for (trait) but ships in v1.1 (needs developer credentials — research item R-05).
- CLI: `coverly audit`, `coverly fill`, `coverly profiles`, `coverly systems`, `coverly update check|install`.
- Auto-update (stable/beta), release pipeline for Win/Linux/macOS (universal).

### Out of v1 (later)
- "Mix" composite images (box + screenshot + logo) à la Skraper; videos; marquees/wheels/logos.
- gamelist.xml metadata editing (descriptions, ratings) — v1 only **merges image paths** for ES-DE/Batocera profiles.
- ROM renaming/verification/deduplication.
- On-device (handheld) builds.

## 5. Domain model

| Concept | Meaning |
|---|---|
| **Library** | A root folder chosen by the user (e.g. `/Volumes/System/ROMS`). Remembered in recent libraries. |
| **Profile** | A layout definition: how to find system folders, which files are ROMs, and **where each media type lives** (path template, file name rule, format, max size). Built-in + user-defined (TOML). |
| **System** | A platform (canonical id, e.g. `snes`) with folder aliases (`SFC`, `SNES`, `snes`, `Super Nintendo`…), ROM extensions, libretro playlist name (`Nintendo - Super Nintendo Entertainment System`), DAT names, header-skip rules. Shipped as `systems.toml`, user-extendable. |
| **RomEntry** | One game as seen by the user: primary file + companion files, stem, size, mtime, system, region tags parsed from name. |
| **ArtStatus** | Per media type: `present(path)`, `missing`, `orphan` (art without a ROM). |
| **Identity** | Result of identification: canonical name (No-Intro/Redump/MAME), method (`hash`, `name-exact`, `name-fuzzy`, `arcade-dat`, `manual`), confidence 0–1. |
| **Candidate** | An image from a source for a media type: source, URL, canonical name, region, score, popularity (if known). |
| **Strategy** | Function choosing one candidate per ROM+media: `Priority`, `RomRegion`, `Popularity`. |
| **Plan** | Ordered list of actions (download+write, skip, needs-review) produced before any write. |
| **Job** | Execution of a plan with progress, cancel, resume, and a journal for undo. |

### Layout profiles (built-in v1; exact paths verified in `docs/02-research/layouts.md`)

| Profile | Cover | Screenshot | Title |
|---|---|---|---|
| Anbernic stock | `<SYS>/Imgs/<stem>.png` | — (single image slot) | — |
| GarlicOS | `<SYS>/Imgs/<stem>.png` | — | — |
| Onion OS | `<SYS>/Imgs/<stem>.png` | — | — |
| muOS | `MUOS/info/catalogue/<System>/box/<stem>.png` | `…/preview/<stem>.png` | — |
| MinUI / NextUI | `<SYS>/.media/<stem>.png` | — | — |
| ES-DE | `downloaded_media/<sys>/covers/<stem>.png` | `…/screenshots/` | `…/titlescreens/` |
| Batocera / Knulli / EmulationStation | `<sys>/images/<stem>-image.png` + gamelist.xml | `-screenshot` | `-titlescreen` |
| RetroArch | `thumbnails/<Playlist>/Named_Boxarts/<label>.png` | `Named_Snaps` | `Named_Titles` |
| Custom | user templates with `{root} {system_dir} {stem} {name} {media}` | | |

Profiles with one image slot (Anbernic/Garlic/Onion/MinUI) expose a setting **"which media goes in the slot"** (default cover).

## 6. Architecture (approach A)

```
coverly-roms/
├─ Cargo.toml                (workspace)
├─ crates/
│  ├─ coverly-core/          (library, no UI, fully unit-tested)
│  │  ├─ systems/            systems.toml + loader + alias resolution
│  │  ├─ profiles/           built-in profiles + TOML loader + auto-detect
│  │  ├─ scan/               walker, ROM classification, multi-file sets, art status
│  │  ├─ identify/           hashing (crc32, header skip), DAT index, arcade map, name parse/normalise
│  │  ├─ sources/            trait ArtSource; libretro, steamgriddb, (screenscraper v1.1)
│  │  ├─ matching/           scoring, regions, strategies
│  │  ├─ plan/ jobs/         plan building, execution, journal, undo, resume
│  │  ├─ image/              decode, resize, encode png/jpg, size limits
│  │  ├─ write/              atomic writes, backups, FAT/exFAT-safe names, AppleDouble handling
│  │  ├─ cache/              SQLite (rusqlite): scan cache, source indexes, http cache metadata
│  │  ├─ net/                HTTP client (ureq/rustls like Pluryprint), retries, rate limit, proxy
│  │  └─ settings/           typed settings + defaults + migration
│  └─ coverly-cli/           binary `coverly` (clap)
├─ src-tauri/                Tauri app: IPC commands + events over coverly-core, updater
├─ src/                      React UI (TS, Tailwind, zustand, i18n)
├─ locales/{en,es}/
└─ docs/
```

Rules: the core never depends on Tauri; the app and CLI are thin. Long work runs on worker threads and reports progress through a `ProgressSink` trait (Tauri events in the app, a progress bar in the CLI). Everything that touches network or disk is behind a trait so it can be faked in tests.

## 7. Data flow

1. **Open library** → profile auto-detect (marker folders/files: `anbernic/`, `Imgs/`, `MUOS/`, `.media`, `gamelist.xml`, `downloaded_media/`, `thumbnails/`) → user confirms.
2. **Scan** (parallel walk, cached by path+size+mtime) → RomEntries + ArtStatus → browser fills progressively.
3. **Select** (e.g. filter "missing cover", one system, or all).
4. **Identify** → hash (if enabled and file ≤ max size) → DAT lookup; else name normalisation; arcade → MAME/FBNeo DAT translation.
5. **Fetch candidates** from enabled sources (libretro index per system is downloaded once and cached; image HEAD/GET only for chosen items).
6. **Choose** with the selected strategy → Plan: `auto` (confidence ≥ high threshold), `review` (between thresholds), `none`.
7. **Review** doubtful items in a side-by-side screen (or skip in CLI with `--auto-only`).
8. **Apply**: download → resize/encode per profile → backup existing (if overwrite) → atomic write → journal.
9. **Re-scan affected entries** → report (filled / skipped / failed / no art found).

## 8. UI

- **Start**: recent libraries, "Open folder…", drag-and-drop of a folder/SD, detected removable volumes.
- **Library**: left sidebar (views: Folders · Systems · Recent/Date · All), toolbar (search, filters, grid/list toggle, "Fill missing art" primary button), main area (virtualised grid of covers or table with columns name, system, size, modified, cover/screenshot/title status icons), right detail drawer (art previews per media, identity + confidence, candidates, actions: change, search, from file, remove, reveal in Finder/Explorer).
- **Filters**: art status per media (missing any / cover / screenshot / title / has all), system, extension, region, identification method/confidence, date range, size.
- **Fill wizard**: scope (selection / filter / all) → media types → strategy + region order → overwrite policy → dry-run summary (counts per system, sample previews) → Run.
- **Review queue**: per ROM, candidates side by side with source/region/score; accept, choose other, search, skip.
- **Jobs**: progress, per-item errors, cancel, resume, undo last job.
- **Reports**: audit summary per system, export CSV/JSON.
- **Settings**, **About/Updates** (as Pluryprint).

## 9. Settings

| Group | Settings |
|---|---|
| General | language (auto/en/es), theme (system/light/dark), check updates on launch, update channel (stable/beta) |
| Library | default profile, ignored folders (e.g. `Imgs`, `.git`, `bios`, `saves`), ignored extensions, include hidden files, follow symlinks, recent libraries list |
| Art | media types enabled (cover/screenshot/title), media for single-slot profiles, output format (png/jpg), max width/height per profile, keep aspect, JPEG quality, overwrite policy (never / ask / always with backup) |
| Matching | strategy (Priority / ROM region / Popularity), region priority order, language preference, auto-accept threshold, review threshold, fuzzy matching on/off, arcade name translation on/off, hashing on/off, max file size to hash |
| Sources | enable/disable + order (libretro, SteamGridDB, ScreenScraper), API keys/credentials stored in the OS keychain |
| Network | concurrent downloads, timeout, retries, proxy (system/env/manual), cache size + clear cache, index refresh interval |
| Safety | backup before overwrite, backup location, dry-run by default, clean macOS `._*` AppleDouble files on removable volumes, undo history depth |
| Advanced | log level, open logs folder, reset settings, custom profiles/systems folder |

## 10. Error handling

- One item failing never stops a job; errors are collected per item with a reason (`no-art`, `network`, `decode`, `write-denied`, `disk-full`, `name-invalid`).
- Offline / rate-limited (403/429): pause the job with a clear message and resume button.
- Read-only or ejected volume: stop safely, keep journal, allow resume.
- File systems: FAT32/exFAT-safe names (no `:*?"<>|`, trailing dots/spaces), Windows long paths, case-insensitive collisions.
- macOS writes `._*` files on exFAT/FAT SD cards: never count them as ROMs or art; optional cleanup.
- Never write outside the library root; writes are atomic (temp file + rename).

## 11. Security & privacy

No telemetry. Only requests: art sources, update manifest. Credentials in the OS keychain. HTTPS only. Updates verified with minisign. Downloaded images are decoded and re-encoded (never copied raw), which also strips metadata.

## 12. Testing

- **Core**: unit tests per module; fixture library trees per profile (generated in tests, tiny fake ROMs); golden tests for name normalisation and matching (real-world names list); mock HTTP server for sources; property tests for file-name sanitising.
- **CLI**: snapshot tests of `audit` output.
- **App**: Vitest + Testing Library for UI; WDIO e2e like Pluryprint.
- **CI**: GitHub Actions matrix (ubuntu, windows, macos): fmt, clippy, tests, frontend lint/test, build.

## 13. Release & updates

Copied and adapted from Pluryprint: `release.yml` (tauri-action matrix → `.dmg` universal, `.msi`/NSIS, `.AppImage`/`.deb`), `release-channels.yml` (beta), updater endpoint `https://github.com/MAECLY/coverly-roms/releases/latest/download/latest.json`, new minisign keypair (private key in repo secrets, never committed), conventional commits + CHANGELOG. macOS unsigned at first → README explains "Open Anyway".

## 14. Legal

AGPL-3.0-only + TRADEMARKS.md (name **Coverly ROMs**, wordmark, mark — to design) + CLA.md + CONTRIBUTING, CODE_OF_CONDUCT, SECURITY, THIRD-PARTY-NOTICES, `LICENSES/`. The app downloads art from third-party sources at the user's request; it does not bundle ROMs or art. Source terms (libretro-thumbnails, libretro-database, SteamGridDB, ScreenScraper) documented in `docs/09-legal/`.

## 15. Milestones

| M | Goal | Exit criterion |
|---|---|---|
| M0 | Repo foundation: docs, legal, scaffold workspace + Tauri + React, CI green | `cargo test` + `pnpm test` + CI pass on 3 OS |
| M1 | Core scan + audit + `coverly audit` | Audit of the real SD (`/Volumes/System/ROMS`) matches a manual count |
| M2 | Identification + libretro source + strategies + `coverly fill` (dry-run & apply) | Fill a copy of one system end-to-end with backups and undo |
| M3 | Desktop browser (views, filters, detail) | Browse the real SD smoothly (≥ 10k entries) |
| M4 | Fill wizard + review queue + jobs UI | Fill missing covers on the SD copy from the UI |
| M5 | Settings + i18n en/es + profiles editor | All §9 settings persisted and applied |
| M6 | SteamGridDB + Popularity strategy (+ ScreenScraper v1.1) | Popularity strategy picks by votes |
| M7 | Release pipeline + updater + brand + public repo + v0.1.0 | App updates itself from a published release |

## 16. Research-driven refinements (2026-10-15)

Verified in `docs/02-research/` (workflow `wf_46f73971-0a2`). These refine §5–§13 and win on conflict.

1. **Two-phase model** (Skyscraper): *gather* candidates into a content-addressed art cache (source, type, region, timestamp), then *apply* to the layout. Changing profile re-applies without re-downloading. Per-type source priority, newest wins as fallback.
2. **Identity key**: content hash (CRC32+MD5+SHA1 in one streaming pass); for single-file zip/7z read the inner CRC from the archive header (no decompression); filename hash for multi-file formats (`cue`, `gdi`, `m3u`…). Extensions compared **case-insensitively** (Skyscraper bug). Quick-id memo `path+size+mtime → hash`.
3. **Matching method is a setting** (Skraper): `hash then name` (default) · `hash only` · `name only`. Hash only computed when the method needs it.
4. **Name normalisation** (Skyscraper `getCompareTitle`/`getUrlQueryName`): strip brackets, rev/version suffixes, `_`→space, `, The` → front, numeral equivalence, fallback without subtitle; score `100*(L-lev)/L` on Unicode chars (not bytes); reject candidates with different sequel numeral or year.
5. **libretro specifics**: system list from the **live** `thumbnails.libretro.com` root listing (names differ from the GitHub repos); parse the Apache index per system/type, cache on disk **TTL ≈ 2 days**; sanitise `& * / : \` " < > ? \ |` → `_` (mini-scraper misses `\`, Skyscraper misses `` ` ``); remove `.png` before scoring; case-insensitive keys; deterministic region ranking (mini-scraper picks alphabetically); fallback host `raw.githubusercontent.com/libretro-thumbnails/<repo>/master/…`; **global** token bucket, 4–6 concurrent requests, backoff.
6. **Arcade**: short name → description from libretro-database `metadat/mame` and `metadat/fbneo-split` (no existing Mac tool does this).
7. **Regions**: configurable list, default `ROM's own region > World > USA > Europe > Japan`; parse both No-Intro `(USA, Europe)` and GoodTools `(U)(E)(J)(JUE)` codes.
8. **Layouts**: each profile declares whether art **mirrors subfolders**; warn on stem collisions in flat layouts (Batocera `images/`, muOS `box/`). Onion/Garlic/Anbernic defaults: **PNG, max 250×360, folder `Imgs` with capital I**. muOS reads `catalogue=` from `MUOS/info/assign/<system>/global.ini`. Onion: honour `Emu/*/config.json` `extlist`/`hidebios`; skip `PORTS/Shortcuts`. Batocera/Knulli: create/update `gamelist.xml` with backup. RetroArch: write PNG only. "Has art" check accepts png/jpg/jpeg/webp twins and Batocera `-thumb`/`-image` postfixes.
9. **Sources**: ScreenScraper API may only be used by **free** apps → OK for Coverly (AGPL, free); request devid/devpassword early; ≥1.2 s between requests, honour `maxthreads`, handle 429/430/431. **TheGamesDB** has `/v1/Games/ByGameHash` (exact match by md5/crc) → good secondary source. **SteamGridDB is PC-storefront oriented** → see open question 5.
10. **Safety**: default "only missing"; dry-run preview; backup gamelists; export "missing art" report and "retry only these".
11. **Updater**: pin Tauri 2.12.x / plugin-updater 2.14.x / tauri-action v1 (a v3 alpha holds GitHub's "Latest" flag); `createUpdaterArtifacts: true`; `updaterJsonPreferNsis: true`; releases must be in a **public** repo; public repos get free `ubuntu-22.04-arm` runners. Skraper itself ships a static-manifest updater with mandatory/optional updates and rollback — we match it with signed artifacts.
12. **Later (not v1)**: Skraper-style mix composition (XML `ImageComposition`, fallback chain Screenshot > Title > Wallpaper > Text).

## 17. Open questions

1. Brand mark (icon) design for Coverly ROMs.
2. ScreenScraper developer credentials (application process) — research R-05.
3. libretro-database DAT licences / redistribution vs download-at-runtime — research.
4. ~~libretro index method~~ → resolved: Apache listing of `thumbnails.libretro.com`, TTL 2 days (§16.5).
5. **Popularity strategy coverage**: SteamGridDB (votes) covers PC titles mostly, not consoles. Options: (a) keep it PC-only and fall back to Priority elsewhere; (b) define "popularity" for consoles as the variant most commonly used across sources/regions; (c) drop it. Needs Miguel's decision.
