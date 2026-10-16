# Art Sources

> **TL;DR.** Art comes from sources behind one small `ArtSource` trait. Three are built: **libretro-thumbnails** (M2, primary, no key), **SteamGridDB** (M6, voted portrait covers behind the user's own API key) and **ScreenScraper** (M6, v1.1, hidden unless the build carries developer credentials). libretro's system list is the live `https://thumbnails.libretro.com/` root; each `<System>/Named_<Type>/` Apache listing is cached in SQLite for two days and revalidated with its ETag. Every answer from the network is treated as hostile. All requests share one polite HTTPS client: a global limiter, retries with backoff, and a pause on a rate limit (429, or 403 from libretro/GitHub) or going offline; an API source that refuses its key is skipped for the run instead (one warning). Secrets travel as `credentials::Secret` and never reach a log, an error, the cache, a plan, a journal or the webview. Decisions: [ADR-0005](../adr/0005-libretro-primary-source.md), D9, D33–D36 in the [tracker](../TRACKER.md). Spec: [§6, §9 Sources, §10, §11, §16.5, §16.9](../superpowers/specs/2026-10-15-coverly-roms-design.md#16-research-driven-refinements-2026-10-15). Evidence: [`libretro-thumbnails.md`](../02-research/libretro-thumbnails.md), [`screenscraper-api.md`](../02-research/screenscraper-api.md). Code: `crates/coverly-core/src/sources/`, `crates/coverly-core/src/net/`, `crates/coverly-core/src/credentials.rs`.

## The `ArtSource` trait

```rust
pub trait ArtSource: Send + Sync {
    fn id(&self) -> &str;                                   // "libretro"
    fn candidates(&self, q: &ArtQuery) -> Result<Vec<Candidate>, SourceError>;
    fn has_name(&self, system: &System, media: Media, name: &str) -> Result<bool, SourceError>;
    fn fetch(&self, c: &Candidate) -> Result<Vec<u8>, SourceError>;  // raw bytes, never written as is
}
```

- `ArtQuery` carries the system, the media, the canonical name (DAT or arcade), the parsed ROM name, the raw stem, whether name matching is allowed (`false` under `hash-only`), the fuzzy settings (`min_score` 65, `max_candidates`) and, from M6, the ROM's hashes (`hash`, when it was hashed), `size` and `file_name` (ScreenScraper matches by them).
- A `Candidate` is one image: `source`, `media`, `name`, `url`, optional `fallback_url`, `kind` (`canonical` > `name-exact` > `title-exact` > `fuzzy`), `score` (0–100), `regions` parsed from the name, two popularity fields — `votes: Option<Votes { up, down }>` (SteamGridDB; `Votes::score()` = up − down) and `consensus` — and `origin`, a short provenance line (`game 1234 · grid 5678 · 600x900 alternate` for SteamGridDB, `jeu 3` for ScreenScraper, absent for libretro). A candidate list cached before M6 still reads (`votes` and `origin` default to none).
- Sources only **find** candidates and **fetch** bytes. They never choose (that is [matching](./fill-pipeline.md#strategies-and-d9)) and never write (that is the [writer](./fill-pipeline.md#writer-guarantees)).
- `SourceError` values:
  - `SystemUnavailable` — the source has no folder or id for the system (a system the live root does not list, a system without a ScreenScraper id). Gathering keeps the error on each affected item and media, and the plan shows them as `none` with that reason.
  - `Net`, `Cache`, `NotFound` — as in M2; a `Net` rate limit or offline pauses.
  - `Auth { source, status }` — the source refused the key or account (SteamGridDB 401/403, ScreenScraper 403/426), or the key cannot be sent at all (`status` 0: characters a header cannot carry).
  - `MissingKey { source }` — the source needs a key and none is stored.
  - `BadResponse { source, detail }` — the answer could not be read (not JSON, `success: false` with HTTP 200, an over-size body). That ROM gets nothing from the source; the run goes on.
  - `Exhausted { source, status }` — ScreenScraper's daily quota is used up.
- `SourceError::is_source_wide()` is true for `Auth`, `MissingKey` and `Exhausted`: the problem is the source, not this ROM, so [gathering](./fill-pipeline.md#gathering-from-several-sources-m6) asks that source no more for the rest of the run and records one warning (never a pause).
- `KeyCheck` (`ok`, `invalid`, `missing`, `rate-limited`, `offline`, `unavailable`) is the answer of "Test key" in Settings → Sources: `steamgriddb::check_key` and `screenscraper::check_account` make one cheap request and never return the key.

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

## Keys and credentials (D30, D35)

- `credentials::Secret` wraps every key and password: `Debug` prints `Secret(***)`, there is no `Display` and no `Serialize`, and `expose()` is the only way to read it (the HTTP client, when it builds the header or query).
- `SourceKeys { steamgriddb, screenscraper_user, screenscraper_dev }` is read **once per job**, with the rest of the settings snapshot: in the app from the OS keychain (`SourceKeys::from_store`), in the CLI from the environment (`SourceKeys::from_env`: `COVERLY_SGDB_KEY`, `COVERLY_SS_USER`, `COVERLY_SS_PASSWORD`). Worker threads never touch the keychain. A stored value that is empty, longer than 512 characters or holds control characters counts as not set.
- The webview only ever sees whether a key is stored (`credential_status`) and the `KeyCheck` of "Test key" (`test_source_key`); `sources_info` reports, per source, whether this build has it (`built`), which credentials it needs (`needs`) and whether it can run (`ready`) — presence only, never values.

## SteamGridDB (M6-01, D35)

| Aspect | Rule |
|---|---|
| Key | The user's own API key, from the keychain (app) or `COVERLY_SGDB_KEY` (CLI). Sent only as `Authorization: Bearer …` on API requests (`HttpRequest::bearer`), never in a URL. A key with anything but visible ASCII is refused without a request (`Auth`, status 0). Enabled without a key → `MissingKey`: one warning `steamgriddb: no API key stored; skipped` and libretro still fills. |
| Endpoints | `https://www.steamgriddb.com/api/v2`: `GET /search/autocomplete/{title}` (title as one encoded path segment) → game ids; `GET /grids/game/{id}?dimensions=600x900,342x482&types=static&nsfw=false&humor=false&epilepsy=false` → grids. |
| Scope | Covers only (portrait static grids, 600×900 preferred, then 342×482). Heroes, logos, icons, screenshots and title screens are out of scope. Searched by title for every system: it holds many console games, and when it has nothing nothing is lost. Name matching only (nothing under `hash-only`). |
| Title matching | The canonical name when the ROM was identified, else the parsed ROM name. One search result with the same normalised title → `title-exact` 95. **Several** results with that title (a remake, "Doom" 1993 and 2016) → `fuzzy` **85**: review under the default thresholds, never `auto`, never eligible against an exact libretro match. No exact title → the best [compatible](./identification.md#guards) fuzzy titles (≥ `min_score`, at most 94). Grids of at most 2 games per ROM. |
| Votes | `upvotes`/`downvotes` → `Votes { up, down }` (an answer with only a net `score` reads as that). Inside one game, grids are ordered by net votes, then dimension, then id. Grids have no region, so they never add or receive consensus (D34): they compete by votes. |
| Cache | Search answers 7 days (`sgdb-search-2`), grid lists 2 days (`sgdb-grids-2`). Keys carry the base URL and the request path with its filters, never the key. A 404 is not cached. `--refresh` ignores both TTLs. |
| Hostile answers | Bodies capped at 2 MiB; `success: false` or not JSON → `BadResponse` for that ROM; `data` parsed element by element (one malformed grid drops that grid only); vote counts that do not fit a `u32` drop the element; names and styles stripped of control and bidi characters and capped (200 / 32 characters); MIME must be PNG, JPEG or WebP; image URLs accepted only over HTTPS on `steamgriddb.com` or a subdomain with a plain `host[:port]` authority (`evil.com\.steamgriddb.com` is refused). nsfw/humor/epilepsy grids are dropped even if the server sends them. |
| Errors | 401/403 → `Auth` (source-wide: skipped for the run with one warning, **never a pause**; spec §10's "403 pauses" stays for libretro/GitHub, where it is a rate limit). 429 → retried by the polite client, then a pause. Offline/timeout → pause. |
| Images | Fetched from the CDN **without** the key (redirects and other hosts never get the `Authorization` header), capped at 16 MiB, then decoded and re-encoded like every image. |
| Test key | `check_key`: `GET /search/autocomplete/mario` with the key; 2xx → `ok`, 401/403 → `invalid`, 429 → `rate-limited`, offline → `offline`, else `unavailable`; no key → `missing` without a request. |

## ScreenScraper (M6-02, v1.1, hidden — D36)

| Aspect | Rule |
|---|---|
| Availability | Only in a build compiled with `COVERLY_SS_DEVID` and `COVERLY_SS_DEVPASSWORD` (`option_env!`, never a file in the repo). Without them `available_sources()` is libretro + SteamGridDB, settings normalise `screenscraper` to off, and the UI says "Not available in this build". The developer credentials are requested on the ScreenScraper forum (spec §17 Q2). |
| Account | Optional user account (keychain `screenscraper-user` / `screenscraper-password`, or `COVERLY_SS_USER` / `COVERLY_SS_PASSWORD` in the CLI), sent only when both are stored. It raises the thread count and daily quota. |
| Endpoints | API v2 `https://api.screenscraper.fr/api2`: `jeuInfos.php` by `crc`, `md5`, `sha1`, `romtaille`, `romnom` and `systemeid` when the ROM was hashed (`canonical` 100); `jeuRecherche.php` (`recherche=<title>`) otherwise (`title-exact` 95; nothing under `hash-only`). `softname=CoverlyROMs-<version>`, `output=json`. |
| Media | cover → `box-2D` (`flyer` for arcade systems), screenshot → `ss`, title → `sstitle`. One candidate per region the game has that media for (`wor`, `us`, `eu`, `jp`, … mapped to Coverly regions; unknown codes dropped). |
| URLs | Candidate URLs are built by Coverly as `mediaJeu.php?systemeid=…&jeuid=…&media=…` (the media type carries the region, e.g. `box-2D(us)`) **without credentials**, so nothing secret reaches the cache, a plan, a journal or the webview. `fetch` appends the credentials at request time. Bodies `NOMEDIA`, `CRCOK`, `MD5OK`, `SHA1OK` mean no image. |
| Throttle | Its own limiter on top of the polite client: at least **1.2 s** between request starts and **1 request at a time** until `ssuser.maxthreads` says more (clamped 1–6). |
| Status codes | 401/423 (API closed, for everyone or for non-members) → pause. 430/431 (daily quota), or `requeststoday ≥ maxrequestsperday` in an answer → `Exhausted`: ScreenScraper is skipped for the rest of the run with one warning and the fill goes on with the other sources (a pause would come back on every resume until the quota resets). 403/426 → `Auth` (source-wide). 429 (too many threads) → retried, then a pause. 400 → `BadResponse`. |
| Hostile answers | Bodies capped at 4 MiB; games parsed one by one; ids digits only (≤ 12 digits), region codes 2–3 lowercase letters (`us&x=1` dropped), only known media types of the parent `jeu`, names cleaned and capped. |
| Errors | Every error text is scrubbed: each credential, raw and percent-encoded, becomes `***` (a 500 or a timeout of a URL with `devpassword=` never shows it). |
| Cache | Game answers 7 days (`ss-game-1`); keys carry the base, the system and the request parameters, never a credential. |
| System ids | `screenscraper = <id>` in `systems.toml` for 39 systems (from `systemesListe.php` research; to confirm against the live list once credentials exist). Others are "not available on screenscraper". |
| Test | `check_account`: `ssuserInfos.php`; no account → `missing` without a request. |

## How the sources combine

Every enabled and usable source is asked for each ROM and media, in the user's order (Settings → Sources, or `--sources` in the CLI). Their candidates are merged (`gather::merge_candidates`: dedupe by source + URL, cross-source consensus D34) and ranked by the strategy; see [fill-pipeline.md](./fill-pipeline.md#gathering-from-several-sources-m6). The real-API checks are `#[ignore]` tests in `crates/coverly-core/tests/real_m6.rs` that skip without a key or developer credentials:

```text
COVERLY_SGDB_KEY=… cargo test -p coverly-core --test real_m6 -- --ignored --nocapture
COVERLY_SS_DEVID=… COVERLY_SS_DEVPASSWORD=… cargo test -p coverly-core --test real_m6 -- --ignored --nocapture
```
