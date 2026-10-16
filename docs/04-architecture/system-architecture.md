# System Architecture

> **TL;DR.** One Cargo workspace: a pure Rust library (`coverly-core`) holds every rule about systems, layouts, scanning and art; the `coverly` CLI and the Tauri app (`coverly-roms`) are thin layers over it. M1 built the read-only scan and audit. M2 adds identification, the libretro-thumbnails source, strategies, plans and safe writes, which together are `coverly fill` and `coverly undo`. Details: [identification](./identification.md), [art sources](./art-sources.md), [fill pipeline](./fill-pipeline.md). Decisions: [ADR-0002](../adr/0002-stack-tauri-rust-react.md), [ADR-0003](../adr/0003-core-cli-app-workspace.md). Source: spec [§6–§7](../superpowers/specs/2026-10-15-coverly-roms-design.md#6-architecture-approach-a), refined by [§16](../superpowers/specs/2026-10-15-coverly-roms-design.md#16-research-driven-refinements-2026-10-15).

## Containers

```
┌───────────────────────────────┐     ┌─────────────────────────────┐
│ Tauri app  (src-tauri + src/) │     │ coverly CLI (crates/        │
│ React 19 + TS + Tailwind +    │     │ coverly-cli, clap)          │
│ zustand, i18n en/es           │     │ audit · fill · undo ·       │
│ IPC commands + events         │     │ profiles · systems          │
│                               │     │ table / JSON / CSV output   │
└──────────────┬────────────────┘     └──────────────┬──────────────┘
               │  Rust calls (no Tauri types cross)   │
               └──────────────────┬───────────────────┘
                                  ▼
               ┌──────────────────────────────────────┐
               │ coverly-core (library, no UI)        │
               │ systems · profiles · scan · art ·    │
               │ audit · identify · cache · net ·     │
               │ sources · gather · matching · plan · │
               │ image · write · jobs                 │
               │ (M5+: settings)                      │
               └──────────────────┬───────────────────┘
                                  ▼
          ROM library on disk (SD card, ES-DE, RetroArch, folder)
          thumbnails.libretro.com + raw.githubusercontent.com (M2)
          SteamGridDB, ScreenScraper (M6)
```

Rules:

- `coverly-core` **never depends on Tauri** and has no UI strings. Network code lives only in `net` (HTTPS only, one global limiter) and is reached through the `Http` trait.
- The app and the CLI only parse input, call the core and present results.
- Long work runs on worker threads and reports progress through a `ProgressSink` trait (`progress.rs`; Tauri events in the app from M3/M4, stderr in the CLI).
- Everything that touches network or disk is behind a trait so tests can fake it.

## Workspace layout

```
coverly-roms/
├─ Cargo.toml                 workspace (members: crates/*, src-tauri)
├─ crates/
│  ├─ coverly-core/src/
│  │  ├─ systems/             systems.toml (embedded) + loader + alias resolution   [M1]
│  │  ├─ profiles/            built-in profiles, path templates, auto-detect        [M1]
│  │  ├─ scan/                walker, ROM classification, multi-file sets            [M1]
│  │  ├─ art/                 art status per media, orphan art                      [M1]
│  │  ├─ audit/               report model, JSON (serde) and CSV                    [M1]
│  │  ├─ paths.rs             app directory identity (qualifier, org, app)          [M1]
│  │  ├─ identify/            CRC32/MD5/SHA1, header skip, DAT index, arcade map    [M2]
│  │  ├─ sources/             trait ArtSource: libretro [M2]; SteamGridDB, ScreenScraper [M6]
│  │  ├─ gather/              targets, identify + candidates into the cache         [M2]
│  │  ├─ matching/            regions, strategies (Priority, ROM region, Popularity) [M2]
│  │  ├─ plan/ jobs/          plan (auto/review/none), apply, art store, pause      [M2]
│  │  ├─ image/               decode with limits, resize, re-encode                 [M2]
│  │  ├─ write/               atomic writes, backups, journal, undo, FAT-safe names [M2]
│  │  ├─ cache/               SQLite: quick ids, DAT rows, indexes, candidates      [M2]
│  │  ├─ net/                 HTTPS client, global limiter, retries, proxy, mock    [M2]
│  │  ├─ progress.rs time.rs  progress events, time helpers                         [M2]
│  │  └─ settings/            typed settings, defaults, migration                   [M5]
│  └─ coverly-cli/            binary `coverly`
├─ src-tauri/                 Tauri 2 app, IPC commands (today: app_info)
├─ src/                       React UI
├─ locales/{en,es}/ui.json    UI strings
└─ docs/
```

## Domain model (as built in M1)

| Type | Module | Meaning |
|---|---|---|
| `System`, `SystemDb` | `systems` | A platform with a canonical id (`snes`), folder aliases (`SFC`, `SNES`…) and ROM extensions. Built-in from `systems.toml`, extendable with a user TOML. Aliases and extensions match case-insensitively. |
| `Profile`, `MediaSlot`, `Media` | `profiles` | A layout: `roms_root` (`ROMS`, `Roms`, `roms`…), art folder names, whether art mirrors subfolders, and one slot per media (`cover`, `screenshot`, `title`) as a path template (`{system_dir}/{rel_dir}/Imgs/{stem}.png`), format and max size. Eight built-ins: anbernic-stock, garlicos, onion, muos, minui, esde, batocera, retroarch. |
| `Detection` | `profiles::detect` | Scored guess of the profile from marker folders/files (`anbernic/`, `.tmp_update/`, `MUOS/info/catalogue/`, `.system/`, `downloaded_media/`, `thumbnails/…/Named_*`, `<SYS>/Imgs`, `<SYS>/.media`, `<SYS>/images`, `<SYS>/gamelist.xml`), with human-readable evidence. |
| `LibraryScan`, `SystemScan`, `RomEntry`, `ScanWarning` | `scan` | System folders (known or unknown) and one entry per game: primary file, companions (tracks of a `.cue`/`.gdi`, discs of a `.m3u`), stem, size, mtime. |
| `ArtState`, `RomArt`, `SystemArt` | `art` | Per ROM and media: `present(path)`, `missing` or `unsupported` (the profile has no slot); plus orphan images no ROM uses. |
| `AuditReport`, `Totals`, `SystemReport`, `EntryReport` | `audit` | Serialisable summary: totals, per-system missing counts, per-entry art states, warnings. |

## Data flow

### Audit (M1, implemented)

1. **Choose profile** — `--profile <id>` or `auto`: `detect()` scores the built-ins; ties and the evidence are reported. No detection and no `--profile` → exit code 4.
2. **Scan** — `scan_library()` lists the children of `root/roms_root`, resolves each folder to a system by alias, and walks the known ones. Unknown folders (`APPS`, `PORTS`, `JAVA`…) are listed as unknown and skipped. Inside a system, files with one of its extensions are ROMs; art folders, `bios`, `saves`, `states`, `Shortcuts` and ignored extensions (`png`, `srm`, `xml`…) are skipped. Hidden entries and AppleDouble `._*` twins are never counted. Multi-file sets collapse into one entry. Unreadable folders and broken symlinks become warnings; the scan never aborts.
3. **Resolve art** — `resolve_art()` renders each media slot for each ROM and looks the file up in a per-directory index built with one `read_dir` per art folder (no `exists()` per ROM and extension). Folder names, stems and extensions match case-insensitively and in Unicode NFC; another image extension (`.jpg` where the profile writes `.png`) counts as present; Batocera postfixes are understood (`-image`/`-thumb` for covers, `-titleshot` for titles), and other scraper media of a game (marquee, fanart, wheel…) is not reported as orphan. Images that match no ROM are orphans.
4. **Report** — `build_report()` produces an `AuditReport`; the CLI prints a table, JSON or CSV (optionally `--missing-only`, optionally to `--output`).

The audit is **read-only**: no code path in `scan`, `art` or `audit` opens a file for writing.

### Fill (M2, implemented)

Two phases, after Skyscraper (spec §16.1, [ADR-0009](../adr/0009-two-phase-fill-with-sqlite-cache.md)). The full flow, thresholds, writer guarantees and the end-to-end evidence are in [fill-pipeline.md](./fill-pipeline.md):

1. **Gather** — identify each selected ROM (hash → DAT, then name, arcade translation; [ADR-0006](../adr/0006-hash-then-name-identification.md)), fetch candidates from enabled sources ([ADR-0005](../adr/0005-libretro-primary-source.md)) into a content-addressed art cache.
2. **Choose** — a strategy picks one candidate per ROM and media ([ADR-0007](../adr/0007-pluggable-strategies.md)); the result is a plan: `auto`, `review` or `none`, shown as a dry run.
3. **Apply** — download (through the art store) → decode and re-encode per profile → back up existing art if overwriting → atomic write (temp + rename) inside the library root → journal for undo ([ADR-0010](../adr/0010-safe-writes-backups-and-journal.md)) → re-audit with `coverly audit`.

## Performance (M1 evidence)

Audit of the reference SD (Anbernic stock, FAT `msdos` volume in a USB reader, 22,607 files under `ROMS/`, 7,473 ROMs in 52 known systems), release build on macOS:

| Run | Wall time |
|---|---|
| First run of the session | 1.69 s |
| Repeated runs (OS cache warm) | 0.17–0.43 s |

At this speed a scan cache is not needed for auditing, so the SQLite scan cache (tracker M1-05) moved to M2, where hashing makes caching worthwhile.

## Testing

- **Core:** unit tests per module on fixture libraries generated in temp dirs (case variants, `._*` twins, multi-file sets, unknown folders, unreadable dirs, broken symlinks); `insta` snapshots for JSON and CSV reports.
- **Network:** never real in tests. `coverly_core::net::testing::MockServer` (std `TcpListener`, feature `test-util`) serves Apache listings, DATs, images, 404/403/429 and garbage.
- **CLI:** `assert_cmd` integration tests for output formats and exit codes (0 ok, 2 usage, 3 bad root, 4 no profile, 5 paused).
- **End to end:** M2 ran against the real servers on a copy of the SD and on a FAT32 disk image (see [fill-pipeline.md](./fill-pipeline.md#end-to-end-evidence)).
- **App:** Vitest + Testing Library.
- **CI:** `ci.yml` runs fmt, clippy and the workspace tests on Linux, Windows and macOS ([CI/CD](../07-build-and-release/ci-cd.md)).
