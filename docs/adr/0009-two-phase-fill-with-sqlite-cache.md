# ADR-0009: Two-phase fill with a SQLite cache and a content-addressed art store

## Status

Accepted (implemented in M2)

## Date

2026-10-16

## Context

Filling a library is slow and depends on the network. Each ROM is hashed, its DAT may need downloading, a source listing has to be fetched and matched, and only then can an image be downloaded and written. If one loop did all of this and wrote as it went, a rate limit halfway through would leave a half-written card with no reviewable plan, and every re-run would repeat the hashing and listing work. Skyscraper splits the same work into a *gather* phase into a local cache and a *generate* phase from that cache ([`skyscraper-architecture.md`](../02-research/skyscraper-architecture.md)); spec §16.1 adopts the split. The tracker had already moved the SQLite scan cache (M1-05) to M2 for hashing (decision D11).

## Decision

- **Phase 1 — gather** identifies the selected ROMs and collects art candidates from every source into a **local SQLite cache** (`coverly.sqlite3` in the OS cache dir, or `--cache-dir`). It writes nothing to the library. It downloads only DATs and listings. The cache holds:
  - the quick-id memo (`path + size + mtime → hashes`);
  - DAT rows, with ETag and TTL (30 days);
  - parsed source listings, with ETag and TTL (2 days);
  - the gathered candidate lists, keyed by system, ROM stem, identity and options.
- **Plan** — a strategy turns the gathered candidates into a plan (`auto` / `review` / `none`), which is the dry run.
- **Phase 2 — apply** downloads the chosen images through a **content-addressed art store** (`<cache>/art/<sha1[0..2]>/<sha1>`, indexed by URL, verified by SHA1 on read), re-encodes them and writes them through the safe writer ([ADR-0010](./0010-safe-writes-backups-and-journal.md)).
- The cache is **only a cache**. A corrupt database, or one from a newer schema, is set aside and recreated, and the fill continues. A busy or locked database is left alone. When parsing changes, the cache namespaces get a new version (`libretro-index-2`), so stale parsed data is never revalidated by a 304.

## Consequences

### Positive

- The dry run shows the full plan before anything is written. The app's review queue (M4) reads the same plan.
- Re-applying is cheap and works offline: after an undo, with another profile, or after a pause, no ROM is hashed again and no image is downloaded again. In the M2 end-to-end run, a repeat apply of 109 covers took 1.0 s instead of 17.5 s.
- A pause (rate limit, offline) during gather writes nothing, and the next run continues from the cache.
- The cache can be deleted at any time. The only cost is time.

### Negative

- The cache grows with downloaded art (60 MB for about 190 ROMs in the end-to-end run). Size limits and a "clear cache" control are M5 settings.
- Candidate lists are cached for 2 days, so new art on the server appears only after that, or with `--refresh`.

### Risks / Open Questions

- Paths in the quick-id memo are absolute. Moving a library re-hashes it once, which is correct but slow on very large cards.
