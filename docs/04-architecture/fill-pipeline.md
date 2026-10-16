# Fill Pipeline

> **TL;DR.** `coverly fill` runs scan → art status → targets → **gather** → **plan** → (with `--apply`) **apply** → re-audit. Gathering identifies ROMs and collects art candidates into a SQLite cache. Planning picks one candidate per ROM and media with a strategy and classes it `auto`, `review` or `none`; that is the dry run, and it writes nothing. Applying downloads through a content-addressed art store, re-encodes for the profile and writes atomically inside the library root. Replaced files are backed up and every write is journaled, so `coverly undo` can reverse a job. A rate limit or going offline pauses the job; it never fails item by item. Decisions: [ADR-0007](../adr/0007-pluggable-strategies.md), [ADR-0009](../adr/0009-two-phase-fill-with-sqlite-cache.md), [ADR-0010](../adr/0010-safe-writes-backups-and-journal.md). Spec: [§7, §9, §10, §11, §16.1, §16.10](../superpowers/specs/2026-10-15-coverly-roms-design.md). Related: [identification](./identification.md), [art sources](./art-sources.md).

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
 │      for each media, each source:
 │        cached candidate list?  ── yes ─▶ use it
 │                                └─ no ──▶ LibretroSource::candidates (listing from cache / HTTP)
 │      403/429/offline/timeout ─▶ PAUSE (partial result, exit 5)
 │
 ├─ build_plan                       strategy ranks; auto / review / none
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
| `popularity` | `votes` (SteamGridDB, M6), then `consensus`, then Priority |

Every tie ends on source, name and URL, so a plan never depends on input order.

**Popularity (D9).** Part (b), consensus, is measured inside libretro in M2. With `--strategy popularity`, gathering counts how many libretro media listings (`Named_Boxarts`, `Named_Snaps`, `Named_Titles`) contain the same variant file. A variant with box, snap and title is more "canonical" than one with a lone box. Part (a), SteamGridDB votes, and cross-source consensus arrive with the M6 sources. The `votes` field already exists, so the strategy will not change (decision D15).

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
{"type":"done"}
```

  The root is stored as an **absolute** path, so `undo` works from any working directory. A torn last line (crash) is ignored, and an unfinished job can still be undone.
- **Undo** (`coverly undo [--job ID] [--list]`) walks the entries in reverse. A `created` file is removed only if its SHA1 still matches. A `replaced` file is restored from its backup, atomically, only if its SHA1 still matches. A file the user changed or deleted after the job is **kept** and reported. Job ids are validated before use, and backup paths must stay inside the job's backup folder. The journal is renamed `<job>.undone.jsonl`. The newest 10 journals and their backups are kept.

## Apply

`jobs::apply_plan` runs the wanted items in plan order with `--concurrency` workers:

- **Art store.** Downloads land in `<cache>/art/<sha1[0..2]>/<sha1>`, keyed by URL in SQLite and verified by SHA1 on read. Re-applying after an undo, or with another profile, downloads nothing.
- **Fallbacks.** A 404, a decode failure (garbage behind a 200, a truncated PNG, a 9000 px image) or a fetch error moves to the next alternative. Only when all of them fail does the item fail, with `no-art`, `network` or `decode`.
- **Per-item errors** (spec §10): `no-art`, `network`, `decode`, `write-denied`, `disk-full`, `name-invalid`, `outside-root`, `io`. One failed item never stops the job.

## Pause and resume

| Event | Gather | Apply |
|---|---|---|
| 403 / 429 (after retries for 429) | stop, partial plan, exit 5, nothing written | stop scheduling, finish the journal, exit 5 |
| Offline or timeout after retries (D15) | same as a rate limit | same as a rate limit |
| Volume full or read-only, write denied | — | stop, journal kept |
| Cancel (`AtomicBool`, used by the app in M4) | — | stop, journal kept, `cancelled: true` |

**Resume = run the same command again.** Already-written art is no longer missing, so it is not a target. Quick ids, DATs, listings and candidate lists come from the cache, and art already downloaded comes from the art store. `coverly undo` removes exactly what the interrupted job wrote. In the CLI, Ctrl-C ends the process. Every journal entry is fsynced as it is written, so the job can still be undone.

## CLI reference

```
coverly fill <ROOT> [--profile ID|auto] [--system GBA,FC] [--media cover,screenshot,title]
             [--strategy priority|rom-region|popularity] [--regions rom,world,usa,europe,japan]
             [--match hash-then-name|hash-only|name-only] [--scope missing|all]
             [--dry-run | --apply [--auto-only]] [--overwrite never|backup]
             [--report table|json] [--output FILE] [--concurrency 1-6] [--refresh]
             [--cache-dir DIR] [--data-dir DIR]
coverly undo [--job ID] [--list] [--data-dir DIR]
```

- Dry run is the default. `--scope all` replaces existing art and needs `--overwrite backup`.
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
