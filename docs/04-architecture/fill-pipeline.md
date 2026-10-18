# Fill Pipeline

> **TL;DR.** `coverly fill` runs scan → art status → targets → **gather** → **plan** → (with `--apply`) **apply** → re-audit. Gathering identifies ROMs and collects art candidates from every enabled source (libretro, SteamGridDB, ScreenScraper when built), in the user's order, merged into a SQLite cache. Planning picks one candidate per ROM and media with a strategy, records **why** it won (`PickReason`) and classes it `auto`, `review` or `none`; that is the dry run, and it writes nothing. Applying downloads through a content-addressed art store, re-encodes for the profile and writes atomically inside the library root. Replaced files are backed up and every write is journaled, so `coverly undo` can reverse a job. A rate limit or going offline pauses the job; it never fails item by item. A source whose key, account or quota is refused is skipped for the run with one warning. In the app the same pipeline runs as a job (`coverly_core::fill::FillEnv`), with review decisions between plan and apply. Decisions: [ADR-0007](../adr/0007-pluggable-strategies.md), [ADR-0009](../adr/0009-two-phase-fill-with-sqlite-cache.md), [ADR-0010](../adr/0010-safe-writes-backups-and-journal.md). Spec: [§7, §9, §10, §11, §16.1, §16.10](../superpowers/specs/2026-10-15-coverly-roms-design.md). Decisions D9, D33–D36 in the [tracker](../TRACKER.md). Related: [identification](./identification.md), [art sources](./art-sources.md).

## Flow

```
coverly fill ROOT [--system …] [--media …] [--apply]
 │
 ├─ choose_profile / check_root      profiles::detect (or --profile)
 ├─ scan_library + resolve_art       read-only (M1)
 ├─ select_targets                   missing art only (--scope missing), known systems, media the profile can store
 │
 ├─ gather  ───────────────────────────────────────────────── phase 1 (no library writes)
 │    for each target ROM:
 │      Identifier::identify ── quick-id memo ─▶ hash_rom ─▶ DatStore (download DAT on demand)
 │      for each media, each enabled source (user order):
 │        skipped for this run (refused key / quota)? ─▶ next source
 │        cached candidate list?  ── yes ─▶ use it
 │                                └─ no ──▶ source.candidates (libretro listing, SteamGridDB, ScreenScraper)
 │        Auth / MissingKey / Exhausted ─▶ skip the source for the run, one warning
 │      merge_candidates (dedupe, cross-source consensus D34)
 │      rate limit (429; 403 from libretro) / offline / timeout ─▶ PAUSE (partial result, exit 5)
 │
 ├─ build_plan                       strategy ranks; explain → PickReason; auto / review / none
 │    dry run: print the table (or --report json) and stop
 │
 └─ apply_plan (--apply) ──────────────────────────────────── phase 2
      Writer::begin (journal jobs/<job>.jsonl)
      for each auto (and review unless --auto-only) item, N workers:
        ArtStore hit? ─ yes ─▶ bytes                         (no download)
                      └ no ─▶ source.fetch (primary, then fallback URL) ─▶ ArtStore::put
        process_image (decode with limits, fit, re-encode)
          └ decode error / 404 ─▶ next candidate (alternatives)
        Writer::write (overwrite policy, backup, temp + rename, journal entry)
      pause / full or read-only volume / cancel ─▶ stop scheduling, finish journal
      ─▶ JobReport; prune journals (keep 10); "Undo with: coverly undo"
```

## Plan classes and thresholds

A candidate's **confidence** depends on how it matched: `canonical` (DAT/arcade name) and `name-exact` give 1.0, `title-exact` 0.95, and `fuzzy` its score / 100 (at most 0.94).

| Class | Rule (spec §9 defaults) | Written by `--apply` |
|---|---|---|
| `auto` | confidence ≥ **0.90** | yes |
| `review` | 0.65 ≤ confidence < 0.90 | yes, unless `--auto-only` (spec §7.7, decision D14) |
| `none` | no candidate, below 0.65, or a source error | never |

Each `auto`/`review` item keeps up to 4 ranked **alternatives**. Apply uses them when the chosen image fails.

## Strategies and D9

A strategy only **orders** candidates (`matching::Strategy::rank`). Eligible candidates are those of the strongest match kind whose score is within 5 points of the best. Within them, a retail release always beats a beta, prototype, demo, sample, hack, pirate, kiosk or debug variant (`(Beta)`, `[h…]`…). After that, each strategy applies its own order:

| Strategy | Order |
|---|---|
| `priority` | source order, then the region order without the ROM's own regions |
| `rom-region` (default) | the ROM's own regions (`(USA, Europe)`, `(J)`), then the region order (`rom,world,usa,europe,japan`), then source order |
| `popularity` | exact matches as one tier (D33), then net `votes` (SteamGridDB), match kind, `consensus`, source order and region order (Priority) |

Every tie ends on source, name and URL, so a plan never depends on input order.

**Unreleased variants never win as `auto` (M2-11, D22).** `matching::is_unreleased_variant` recognises bracketed beta, proto, demo, sample, hack, pirate, kiosk, debug and `(Unl)` tags and GoodTools `[h…]`. When the best tier holds only such a variant, `build_plan` moves the first retail candidate that reaches the review threshold to the front, and a non-retail first pick that is neither a canonical nor an exact file-name match, for a ROM that is not itself that kind of variant, is classed `review` with `note: unreleased-variant`, never `auto`. Real cases (M4 run): `057 Final Fantasy IV` → `Final Fantasy IV Advance (Europe) (En,Fr,De,Es,It)` in review, `054 Contra Advance` → `Contra Advance (USA) (Beta)` in review.

**Popularity (D9, D33, D34).** Spec §17 Q5 is resolved by D9: votes where they exist, consensus otherwise, then Priority.

| Step | Popularity order (D33) |
|---|---|
| 1 | Match tier: every **exact** kind (`canonical`, `name-exact`, `title-exact`) is one tier; `fuzzy` is a lower tier. So a voted SteamGridDB grid of the same title can beat libretro's exact file name, but never a fuzzy one (an ambiguous SteamGridDB title is fuzzy 85, see [art-sources.md](./art-sources.md#steamgriddb-m6-01-d35)). |
| 2 | Release before beta/proto/hack (D22) |
| 3 | Net votes, `up − down` (a missing `votes` counts 0) |
| 4 | Match kind (`canonical` > `name-exact` > `title-exact`) |
| 5 | `consensus` |
| 6 | Source order, then region order (Priority) |
| 7 | Preferred language (D28), then name and URL |

Without votes this is exactly the M2 behaviour. Priority and RomRegion are unchanged by M6.

`consensus` (D34) = 1 + the source's other media listings that carry the same file (libretro `Named_Boxarts`/`Named_Snaps`/`Named_Titles`, M2) **+ the number of other sources offering the same parsed title in an overlapping region**, each source counted once. It is measured only under Popularity. SteamGridDB grids have no region, so they never add or receive consensus; they compete by votes.

**Why this image (`PickReason`).** `matching::explain` names the rule that put the first candidate before the second, and every plan item stores it as `reason` (also on the app's chosen `CandidateView`, localised in the review queue and the drawer):

| `why` | Meaning |
|---|---|
| `only` | the only candidate |
| `better-match` | a stronger match kind or score |
| `release` | a release beat a beta/proto/demo/hack (D22) |
| `votes { score, next }` | more net SteamGridDB votes (Popularity) |
| `consensus { count, next }` | more listings, sources and regions agree on it (Popularity) |
| `source-order { source }` | earlier in the user's source order |
| `region { rom }` | earlier in the region order; `rom` = it is the ROM's own region |
| `language` | carries the preferred language (D28) |
| `tie` | equal on every rule: first by name, so the plan is stable |
| `user` | picked by hand (review queue `pick`, drawer) |

## Gathering from several sources (M6)

- **Which sources.** The enabled entries of Settings → Sources, in their order (`--sources libretro,steamgriddb` overrides it in the CLI), kept to `available_sources()`: libretro and SteamGridDB always, ScreenScraper only in a build with developer credentials. Settings from another build (ScreenScraper on, an unknown id, a duplicate) load and normalise instead of failing.
- **Keys snapshot.** `FillEnv.keys` is a `SourceKeys` read once when the job starts (keychain in the app, environment in the CLI) and kept for every resume of that job. Workers never read the keychain; a key never reaches a plan, a journal, a report or the webview.
- **Merge.** `gather::merge_candidates` drops repeats of one image (same source and URL, the strongest kept), adds cross-source consensus under Popularity (D34) and sorts strongest first. Each candidate keeps its `source` and `origin`. The candidate-list cache namespace was bumped for M6 (`candidates-3`).
- **Source-wide skip.** `Auth`, `MissingKey` and `Exhausted` (`SourceError::is_source_wide`) stop that source for the rest of the run with **one** warning in `plan.warnings` (e.g. `steamgriddb: no API key stored; skipped`), and the run goes on with the other sources. This is never a pause: a pause would come back on every resume. A 429 or going offline from any source still pauses the whole phase.
- **Apply.** `FillEnv::apply`, the drawer and "Search by title" fetch through the source a candidate came from; search asks every enabled source and dedupes by `(source, url)`.

## Image pipeline

`image::process_image(bytes, OutputSpec, ImageLimits)`. Downloaded bytes are **always decoded and re-encoded**, never copied raw (spec §11); re-encoding also drops metadata.

| Limit | Default |
|---|---|
| Input size | 20 MiB |
| Format | sniffed from the bytes: PNG, JPEG, WebP, GIF, BMP only (an HTML error page behind a 200 is `decode`) |
| Dimensions | read from the header **before** decoding: at most 8192 px per side |
| Decoder allocation | 256 MiB |

The image is fitted into the profile slot's box (`Contain` keeps the aspect ratio; `Pad` centres it on a canvas of exactly that size) and encoded in the slot's format. Anbernic stock, Garlic and Onion covers are PNG with at most 250×360 px.

## Writer guarantees

`write::Writer`, used by every library write:

- **Root containment.** `ensure_within(root, target)`. Lexically, there is no `..` and no other root. On disk, the deepest existing ancestor must resolve inside the canonical root, and the target must not be a symlink. Escapes are refused (`outside-root`).
- **FAT/exFAT names.** `safe_file_name` replaces control characters and `< > : " / \ | ? *` with `_`, drops trailing spaces and dots, prefixes DOS device names (`CON`, `LPT1`…) and fits 255 bytes. The writer refuses a target whose name is not already safe.
- **Atomic.** The bytes go to `.coverly-<job>-<n>.tmp` next to the target, then fsync, then rename. A temp file never survives an error.
- **Case-insensitive skip.** FAT is case-insensitive. With overwrite `never`, an existing `Imgs/zelda.png` blocks writing `Imgs/Zelda.png`, and a file that appeared between plan and apply is skipped. With `backup`, the twin is replaced, and folders and symlinks are never replaced.
- **No `._*` files.** The writer never sets extended attributes and never uses `fs::copy` (it copies xattrs on macOS). macOS still tags every file and folder a process creates with `com.apple.provenance`. User space cannot remove that tag (`xattr -d` and `xattr -c` leave it), and on FAT/exFAT it becomes a `._<name>` AppleDouble file. The writer therefore **deletes the `._` sidecar of every file and folder it created**, and of the temp file. It never deletes a sidecar that existed before the write. Regression note: the M2 end-to-end run on a FAT32 image found 12 `._*` files for 12 written PNGs before this fix, and 0 after it (commit `90874a2`).
- **Backups** of replaced files go to the **app data dir**, not the card: `<data>/backups/<job>/<path relative to the root>`. A file written twice in one job keeps its original backup.
- **Journal.** `<data>/jobs/<job>.jsonl`, one JSON line per event, flushed and fsynced per entry:

```json
{"type":"header","job":"20261016T050401Z-1bfb-0000","root":"/abs/library","created_at":"2026-10-16T05:04:01Z"}
{"type":"entry","target":"/abs/library/ROMS/FC/Imgs/054 Soccer.png","op":{"kind":"created"},"sha1":"F2EA…"}
{"type":"entry","target":"/abs/…/Imgs/001 Golden Sun.png","op":{"kind":"replaced","backup":"/abs/data/backups/<job>/ROMS/GBA/Imgs/001 Golden Sun.png"},"sha1":"…"}
{"type":"entry","target":"/abs/…/Imgs/057 Final Fantasy IV.png","op":{"kind":"removed","backup":"/abs/data/backups/<job>/ROMS/GBA/Imgs/057 Final Fantasy IV.png"},"sha1":"…"}
{"type":"done"}
```

  The root is stored as an **absolute** path, so `undo` works from any working directory. A torn last line (crash) is ignored, and an unfinished job can still be undone.
- **Job kinds.** The header carries an optional `kind`: `fill` (the default, so journals written before organise jobs existed read as fill) or `organize`. Organise journals also carry `source_root`, a fingerprint `fp` (`size`, `mtime_ns`, `crc32` for copies) instead of `sha1` (now optional), the plan item index, and the ops `moved`, `copied`, `dir-created` and `override-moved` ([organize.md](./organize.md#journal-ops)). A line with an op this build does not know (a journal from a newer Coverly) makes the job `undoable: false` in `list_jobs` instead of failing the listing. `JobSummary` (and the app's job history) exposes `kind` and `undoable`.
- **Undo** (`coverly undo [--job ID] [--list]`) walks the entries in reverse. A `created` file is removed only if its SHA1 still matches. A `replaced` file is restored from its backup, atomically, only if its SHA1 still matches. A `removed` file (drawer "Remove", M4) is restored from its backup unless the user put a file there since. A file the user changed or deleted after the job is **kept** and reported. Job ids are validated before use, and backup paths must stay inside the job's backup folder. The journal is renamed `<job>.undone.jsonl`.
- **Later jobs.** `write::later_jobs(job)` lists the newer, not-undone jobs whose root or source root overlaps the job's root. `coverly undo --job ID` refuses while there are any (undoing an organise job under a fill that added covers to the organised folders would orphan that art) unless `--include-later`, which undoes them too, newest first (`undo_jobs_in_order`); the app's organise undo does the same with *Undo all in order*.
- **Pruning** (`prune_jobs_at`, at the end of every fill run): fill and organise journals are counted **separately**, the newest `safety.undo_depth` (default 10) of each kind are kept, and organise journals are also kept while younger than `settings.organize.keep_organize_journals_days` (default 90; the job id gives the age). A few fill runs therefore never delete the only way to put hundreds of moved ROMs back. A pruned job loses its journal, its `backups/<job>` folder and its `jobs/<job>.plan.json`.

## Apply

`jobs::apply_plan` runs the wanted items in plan order with `--concurrency` workers:

- **Art store.** Downloads land in `<cache>/art/<sha1[0..2]>/<sha1>`, keyed by URL in SQLite and verified by SHA1 on read. Re-applying after an undo, or with another profile, downloads nothing.
- **Fallbacks.** A 404, a decode failure (garbage behind a 200, a truncated PNG, a 9000 px image) or a fetch error moves to the next alternative. Only when all of them fail does the item fail, with `no-art`, `network` or `decode`.
- **Per-item errors** (spec §10): `no-art`, `network`, `decode`, `write-denied`, `disk-full`, `name-invalid`, `outside-root`, `volume-gone`, `io`. One failed item never stops the job.

## Pause and resume

| Event | Gather | Apply |
|---|---|---|
| 403 / 429 (after retries for 429) | stop, partial plan, exit 5, nothing written | stop scheduling, finish the journal, exit 5 |
| Offline or timeout after retries (D15) | same as a rate limit | same as a rate limit |
| Volume full or read-only, write denied | — | stop, journal kept |
| Volume ejected (library root gone) | — | stop before downloading more, journal kept, the item is resumable |
| Cancel (`AtomicBool`, used by the app in M4) | — | stop, journal kept, `cancelled: true` |

The plan and the job report carry a `pause_kind` (`rate-limit`, `offline`, `volume`) so a UI can say why it stopped.

**Resume = run the same command again.** Already-written art is no longer missing, so it is not a target. Quick ids, DATs, listings and candidate lists come from the cache, and art already downloaded comes from the art store. `coverly undo` removes exactly what the interrupted job wrote. In the CLI, Ctrl-C ends the process. Every journal entry is fsynced as it is written, so the job can still be undone.

**Resume in the app.** The app does not re-plan after an apply pause: it resumes the not-attempted (and volume-failed) items of the same plan into the **same journal** (`Writer::resume`; a torn last line is cut off first), so a paused and resumed job is one job for Undo and history (D21). Resuming a gather pause re-runs the dry run, which is cheap from the cache, and clears the decisions.

## In the app

`FillEnv::plan` builds the plan of the chosen entries; the review queue records decisions on it (`Plan::decide`) before `FillEnv::apply` writes `Plan::subset` of the wanted items:

| Decision | Effect |
|---|---|
| `accept` | write the chosen image (falling back to the alternatives) |
| `pick` (`candidate`) | write that image only: 0 is the chosen one, `n` is alternative `n − 1` |
| `skip` | write nothing |

The app writes `auto` items plus the accepted or picked review items; undecided review items are skipped as `skipped-review` (D20). The CLI keeps D14 (review items are written unless `--auto-only`).

The detail drawer writes one file at a time, each as a journaled job of its own (so Undo reverts it): "Change…" writes a candidate through the same apply path; "From file…" uses `write_local_image` (the file is read up to 20 MiB, decoded and re-encoded for the slot); "Remove" uses `remove_art`, which backs the file up and journals a `removed` op. A drawer action that changes nothing leaves no journal.

## Settings, gamelist.xml and manual matches (M5)

The pipeline takes its options from the settings ([settings.md](./settings.md)). The core never reads the file itself:

- **Settings → options.** `scan_options()` (ignored folders/extensions, hidden files, symlinks), `fill_settings()` (media, strategy, regions, languages, thresholds, fuzzy, arcade translation, match method, max hash size), `art_profile()` (single-slot media, output format, per-profile max size), `fit()` (keep aspect, JPEG quality), `net_config()` (concurrency, timeout, retries, proxy), `apply_options()` (`._*` cleanup, undo depth) and `data_dir()` (the backup location). The thresholds of [Plan classes](#plan-classes-and-thresholds) are the setting values (defaults 0.90/0.65). In the app, each job keeps the settings snapshot it started with, for its run and every resume.
- **Match method.** `hash-then-name` (default), `hash-only` (no name fallback) or `name-only` (hashing off). ROMs over "max file size to hash" are identified by name.
- **Language tie-break (D28).** Among candidates the strategy ranks **equal** (same match kind, score and regions), one whose No-Intro language group (`(En,Fr,De)`) contains the first preferred language wins. It never overrides a real strategy difference.
- **Manual identities first (D32).** Identification checks `identity-overrides.json` before hash and name (`IdMethod::Manual`, confidence 1.0), so a ROM matched by hand keeps that match in later fills.
- **gamelist.xml (D31).** After the apply phase, on a profile with `gamelist = true` (Batocera/Knulli), `gamelist::merge_into` merges the written art paths into each touched system's `gamelist.xml` through the same `Writer` (backup + journal), so Undo restores it byte for byte. A file that cannot be merged safely is left untouched with a warning in the report. The mapping and refusals are in [settings.md](./settings.md#gamelistxml-merge-gamelist-d31-m2-10).
- **`._*` cleanup (D27).** With the setting on and the library on a removable volume (`/Volumes/…`, `/media`, `/run/media`, `/mnt`), the AppleDouble files (`._*` starting with `00 05 16 07`) in the folders the job wrote to are deleted after the job (`write::clean_apple_doubles`). This is not journaled.
- **Art store trimming.** After a job, `trim_art_store(cache_dir, cache_max_mb)` deletes the oldest downloaded images until the store fits the cache limit. A trimmed image is a cache miss next time. Clear cache empties it (refused while a job runs).
- **Search by title (M4-05).** `FillEnv::search_title` returns candidates for a typed title (cleaned by `clean_query`: not empty, at most 120 characters, no control characters). `FillEnv::apply_candidate` writes the picked one at the ROM's **own** target path (never renamed, never the candidate's name) as a journaled job of its own, and the app stores the manual identity.
- **Dry run by default.** CLI: `coverly fill` without `--apply`/`--dry-run` follows the setting. App: the wizard always plans first. With the setting off, it starts the run by itself when the plan has nothing to review.

## CLI reference

```
coverly fill <ROOT> [--profile ID|auto] [--system GBA,FC] [--media cover,screenshot,title]
             [--strategy priority|rom-region|popularity] [--regions rom,world,usa,europe,japan]
             [--match hash-then-name|hash-only|name-only] [--scope missing|all]
             [--dry-run | --apply [--auto-only]] [--overwrite never|backup]
             [--report table|json] [--output FILE] [--concurrency 1-6] [--refresh]
             [--sources libretro,steamgriddb] [--cache-dir DIR] [--data-dir DIR]
coverly undo [--job ID [--include-later]] [--list] [--data-dir DIR]
```

`coverly organize` (loose ROMs into one folder per system) is documented in [organize.md](./organize.md#cli); `coverly undo` undoes its jobs too.

- Dry run is the default. `--scope all` replaces existing art and needs `--overwrite backup`.
- `--sources` overrides the order and enabled set of the sources setting. The CLI has no keychain: keys come from `COVERLY_SGDB_KEY`, `COVERLY_SS_USER` and `COVERLY_SS_PASSWORD`. Hidden `--steamgriddb-url` / `--screenscraper-url` point the sources at a mock in tests.
- `--cache-dir` / `--data-dir` override the OS app dirs (`directories`: `com.Maecly.CoverlyROMs`).
- With the `batocera` profile, fill prints a warning that `gamelist.xml` is not updated yet (tracker row M4/M5).

| Exit code | Meaning |
|---|---|
| 0 | done (dry run shown, or job finished) |
| 1 | error (I/O, cache, journal) |
| 2 | bad usage |
| 3 | bad library root |
| 4 | no profile detected and none given |
| 5 | paused (rate limit, offline, timeout): run the same command later, or `coverly undo` |

## End-to-end evidence

M2 Task 14, run on 2026-10-16 with release binaries against the real servers. The source was a copy of the user's SD backup (`~/Documents/ROMS`), placed in the session scratchpad. `/Volumes` was never written to. Full numbers are in the [tracker](../TRACKER.md) («Evidencia M2»).

- **Copy:** GBA (first 40 files + 22 covers) and FC (first 150 files + 41 covers): 253 files, 475 MB, 189 ROMs, 126 missing covers.
- **DAT path check (HEAD):** every explicit path and the GBA/NES no-intro paths return 200. The check found two wrong derived paths (Atari 8-bit, Game & Watch), fixed in `e2e16f8`.
- **Dry run (cold cache, 5.7 s):** `Tom &amp; Jerry 3` showed up as a fuzzy review match. The cause was HTML entities in Apache hrefs, fixed in `0bd4eab` with a test. After the fix: FC 96 auto / 5 review / 8 none, GBA 13 / 3 / 1, 109 / 8 / 9 in total. FC: 74 by hash, 35 by name. GBA: 1 by hash, 16 by name, because the GBA ROMs on the card are trimmed and do not match No-Intro.
- **Apply `--auto-only`:** 109 filled, 0 failed, 17 skipped (review + none). 17.5 s with downloads; 1.0 s again from the art store. All 109 files are `PNG image data`, 0 exceed 250×360, there are no temp leftovers, and missing covers went from 126 to 17 (126 − 109).
- **FAT32 image** mounted at a scratchpad folder (`-nobrowse -mountpoint`): 12 written, **0 new `._*` files** after the sidecar fix (12 before it), and 0 left after undo.
- **Undo** restored the original tree: the `shasum` diff of the whole copy is empty.
- **Backup round trip** (`--scope all --overwrite backup`, GBA): 13 filled, 19 replaced, 19 backups in the data dir; undo removed 13 and restored 19, and the diff is empty.
- **Source untouched:** `find ~/Documents/ROMS/{GBA,FC} -newer snapshot` → 0.

M4 Task 13, run on 2026-10-16 through the app's job path (`AppState::open` → `start_fill` → `run` → `undo_last_job`, the code the UI drives; ignored test `src-tauri/tests/real_fill.rs`, release build, real servers). Full numbers in the tracker («Evidencia M4»).

- **Copy:** GBA (first 60 files + 22 covers) and FC (first 150 files + 41 covers): 273 files, 684 MB, 209 ROMs (anbernic-stock), 146 without cover.
- **Dry run (cold cache, 5.9 s):** auto 122 / review 14 / none 10 (FC 95 / 6 / 8, GBA 27 / 8 / 2). Nothing written.
- **M2-11:** `054 Contra Advance` → review, `unreleased-variant`, `Contra Advance (USA) (Beta)`; `057 Final Fantasy IV` → review, `unreleased-variant`, `Final Fantasy IV Advance (Europe) (En,Fr,De,Es,It)`.
- **Apply (auto only, no decisions):** 19.3 s, 122 filled, 0 replaced, 24 skipped, 0 no-art, 0 failed; every new file a PNG ≤ 250×360, no `._*` or `.coverly-*` leftovers; the session's covers went from 146 missing to 24 (146 − 122) without reopening the library.
- **Undo** (`undo_last_job`): 122 removed, 0 restored, 0 kept; the SHA-1 tree of the copy is identical to before.
- **Source untouched:** `find ~/Documents/ROMS/{GBA,FC} -cnewer snapshot` → 0; nothing under `/Volumes`.

M6 Task 14, run on 2026-10-16 with the release CLI against the real libretro servers and **no SteamGridDB key**. Full numbers in the tracker («Evidencia M6»).

- **Copy:** `ROMS/GG` (107 ROMs, 40 covers) + `anbernic/` → 41 MB, anbernic-stock, 67 without cover.
- **Dry run** (`--sources libretro,steamgriddb --strategy popularity`, `COVERLY_SGDB_KEY` unset, cold cache, 2.1 s): one warning `steamgriddb: no API key stored; skipped`, no pause; auto 66 / review 1 / none 0, every pick from libretro; reasons `only` 32, `better-match` 19, `release` 6, `region` 3, `consensus` 3, `tie` 3, `language` 1 — never `votes`.
- **Apply `--auto-only`:** 66 filled, 0 failed, 1 skipped (10.9 s); 66 new files, no `._*` or `.coverly-*` leftovers.
- **Undo:** 66 removed, 0 restored; `diff -r` against the untouched copy and against `~/Documents/ROMS/GG` is empty.
- **Source untouched:** `find ~/Documents/ROMS -cnewer snapshot` → 0; nothing under `/Volumes`.
