# Art Sources

> **TL;DR.** Art comes from sources behind one small `ArtSource` trait. M2 ships **libretro-thumbnails**. Its system list is the live `https://thumbnails.libretro.com/` root. Each `<System>/Named_<Type>/` Apache listing is cached in SQLite for two days and revalidated with its ETag. Names from the network are treated as hostile. Images are downloaded from thumbnails.libretro.com, with `raw.githubusercontent.com` as fallback. All requests share one polite HTTPS client: a global limiter, retries with backoff, and a pause on 403/429 or going offline. Decision: [ADR-0005](../adr/0005-libretro-primary-source.md). Spec: [§9 Network, §11, §16.5](../superpowers/specs/2026-10-15-coverly-roms-design.md#16-research-driven-refinements-2026-10-15). Evidence: [`libretro-thumbnails.md`](../02-research/libretro-thumbnails.md). Code: `crates/coverly-core/src/sources/`, `crates/coverly-core/src/net/`.

## The `ArtSource` trait

```rust
pub trait ArtSource: Send + Sync {
    fn id(&self) -> &str;                                   // "libretro"
    fn candidates(&self, q: &ArtQuery) -> Result<Vec<Candidate>, SourceError>;
    fn has_name(&self, system: &System, media: Media, name: &str) -> Result<bool, SourceError>;
    fn fetch(&self, c: &Candidate) -> Result<Vec<u8>, SourceError>;  // raw bytes, never written as is
}
```

- `ArtQuery` carries the system, the media, the canonical name (DAT or arcade), the parsed ROM name, the raw stem, whether name matching is allowed (`false` under `hash-only`), and the fuzzy settings (`min_score` 65, `max_candidates`).
- A `Candidate` is one image: `source`, `media`, `name`, `url`, optional `fallback_url`, `kind` (`canonical` > `name-exact` > `title-exact` > `fuzzy`), `score` (0–100), `regions` parsed from the name, and two popularity fields: `votes` (SteamGridDB, from M6) and `consensus`.
- Sources only **find** candidates and **fetch** bytes. They never choose (that is [matching](./fill-pipeline.md#strategies-and-d9)) and never write (that is the [writer](./fill-pipeline.md#writer-guarantees)).
- `SourceError::SystemUnavailable` means the source has no folder for the system (a system the live root does not list). Gathering keeps the error on each affected item and media, and the plan shows them as `none` with that reason.

## libretro-thumbnails

### System list from the live root

`LibretroSource::server_system` matches the system's `libretro` field (case-insensitively) against the folders of the **live server root listing**. The GitHub umbrella repository names differ in places, so they are not used. `stats/` is ignored. A system that the root does not list has no libretro art.

### Apache listings and hostile names

Each media maps to a folder: cover → `Named_Boxarts`, screenshot → `Named_Snaps`, title → `Named_Titles`. The listing at `<base>/<System>/<Named_Type>/` is parsed from its `href="…"` values. Each value is decoded in this order:

1. **HTML entities** are decoded once. Apache escapes `&` in attributes, as in `href="Tom%20&amp;%20Jerry%203…"`; on the NES boxart listing alone, 278 names contain `&amp;`. Only `&amp; &lt; &gt; &quot; &apos;` and numeric references are decoded. Unknown entities stay as they are.
2. **Percent-decoding** runs once, then NFC normalisation. `%26amp;` stays a literal `&amp;`: nothing is decoded twice.
3. **Rejection.** A name is dropped if it is empty, `.` or `..`, contains `/` or `\` (also when an entity or percent-escape produced it: `..%2F..%2Fevil.png`, `..&#47;evil.png`, `a%5Cb.png`), or contains control or bidi-override characters (`%00x.png`, `%E2%80%AEgnp.exe.png`). Query links (`?C=M;O=A`), absolute links and URLs with `://` are ignored.

Names from the network are only ever used to build **URLs**, with each path segment percent-encoded. They never become file names: the written file name comes from the ROM stem through the profile template, and `write::ensure_within` checks it (see [fill-pipeline.md](./fill-pipeline.md#writer-guarantees)).

### Cache and TTL

The root and every listing are stored in SQLite as parsed name lists, with their ETag and fetch time:

- Younger than **2 days**: used without a request.
- Older: conditional GET with `If-None-Match`. A 304 refreshes the timestamp, a 200 replaces the list, and a 404 stores an empty list.
- Network failure with a stale copy: the stale copy is used. A rate limit always surfaces, because it must pause the job.
- Listings are capped at 16 MiB.
- The namespaces carry a parser version (`libretro-index-2`). When parsing changes, old parsed lists are not kept alive by 304s.

`--refresh` ignores the TTL.

### Name matching

libretro sanitises `& * / : \` " < > ? \ |` to `_` in file names (`Batman & Robin` → `Batman _ Robin.png`). Keys are the sanitised name without `.png`, lower-cased and NFC. Candidates are collected in this order, without duplicates:

| # | Try | Kind | Score |
|---|---|---|---|
| 1 | The canonical DAT/arcade name, sanitised, exact | `canonical` | 100 |
| 1b | Same normalised title as the canonical name, any region or revision | `title-exact` | 95 |
| 2 | The ROM file stem, sanitised, exact (unless `hash-only`) | `name-exact` | 100 |
| 3 | Same normalised title as the ROM, any region (unless `hash-only`) | `title-exact` | 95 |
| 4 | Fuzzy on the normalised key and the subtitle-less key (−10), only when 1–3 found nothing, only [compatible](./identification.md#guards) names, score ≥ **65** | `fuzzy` | ≤ 94 |

The list is sorted by kind, score and name, and bounded.

### URLs and the fallback host

- Primary: `https://thumbnails.libretro.com/<System>/<Named_Type>/<file>.png`, each segment percent-encoded.
- Fallback: `https://raw.githubusercontent.com/libretro-thumbnails/<repo>/master/<Named_Type>/<file>.png`. `<repo>` is the system name with spaces replaced by `_`. The renamed repositories are mapped explicitly: `Atari - 8-bit` → `Atari_-_8-bit_Family`, `NEC - PC-8001 - PC-8801` → `NEC_-_PC-88`.
- `fetch` tries the primary URL, then the fallback. A 404 on both is `not found` for that candidate, and the job [falls back to the next candidate](./fill-pipeline.md#apply). Images are capped at 16 MiB on the wire.

## Polite HTTP (`net`)

| Rule | Value |
|---|---|
| Client | ureq 3 (blocking) with rustls, as in Pluryprint |
| Scheme | **HTTPS only**. Plain `http://` only for loopback (`127.0.0.1`, `localhost`, `[::1]`), so tests can use the mock server. Loopback never follows redirects. |
| User-Agent | `CoverlyROMs/<version>`. No telemetry. |
| Proxy | `HTTPS_PROXY` / `HTTP_PROXY` from the environment. A malformed manual proxy fails closed. |
| Concurrency | **one global limiter** for all hosts: `--concurrency` 4 by default, clamped to 1–6 |
| Spacing | at least **150 ms** between two request starts |
| Timeout | 30 s |
| Retries | 3, with exponential backoff from 500 ms (capped at 30 s), on 429, 5xx, timeouts and transport errors. `Retry-After` (seconds or HTTP date) is honoured, capped at 60 s. |
| 403 | no retry: a **rate limit** at once |
| 429 after the retries | a **rate limit** |
| Offline / timeout after the retries | `NetError::Offline` / `Timeout` |
| Body caps | applied **after** gzip decoding, so a decompression bomb stops at the cap |

`NetError::pauses()` is true for a rate limit, offline and timeout. Gathering and applying then **pause the whole phase** instead of failing every remaining item (spec §10, decision D15). Nothing pausing is remembered as a permanent failure, so running the same command again continues.

## Where M6 sources plug in

SteamGridDB and ScreenScraper (M6) implement the same trait. Their candidates join libretro's in the gathered list. `PlanOptions::sources` gives the source order for Priority. SteamGridDB fills `Candidate::votes`, which [Popularity](./fill-pipeline.md#strategies-and-d9) ranks first, so the strategy code does not change.
