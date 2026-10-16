# M2 — Identification, Art Sources and Fill Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `coverly fill` identifies the ROMs of a library (content hash → libretro-database DAT, arcade short name → MAME/FBNeo description, or normalised name), gathers art candidates from libretro-thumbnails into a local SQLite cache, picks one per ROM and media with a strategy (Priority / ROM region / Popularity), shows a dry-run plan (auto / review / none), and on `--apply` downloads, re-encodes and writes the art atomically with backups, a journal and `coverly undo`.

**Architecture:** New `coverly-core` modules, each behind a small typed interface: `identify` (hash, names, DAT, identifier), `cache` (SQLite), `net` (ureq + global limiter + retries, plus a std-only mock server for tests), `sources` (`ArtSource` trait + `LibretroSource`), `gather` (two-phase: identify + candidates into the cache), `matching` (strategies), `plan`, `image`, `write` (atomic writer, backups, journal, undo) and `jobs` (apply a plan with progress, pause and cancel). The CLI composes them: scan → art status → select targets → gather → plan → (apply). Network and disk sit behind traits and injectable base URLs so every test runs offline against a local mock server.

**Tech Stack:** Rust 1.90+ (edition 2021); new crates: `crc32fast 1.5`, `md-5 0.10`, `sha1 0.10`, `zip 4` (no default features — central directory only), `rusqlite` (feature `bundled`), `directories 6`, `ureq 3.4` (rustls, as in Pluryprint), `percent-encoding 2.3`, `image 0.25` (no default features; `png`, `jpeg`, `webp`, `gif`, `bmp`). Tests: `tempfile`, `assert_cmd`, `predicates`, `insta`, and the crate's own `test-util` feature (std `TcpListener` mock HTTP server).

**Spec:** `docs/superpowers/specs/2026-10-15-coverly-roms-design.md` — read §4 (v1 CLI), §5 (domain), §7 (flow), §9 (Matching / Network / Safety), §10 (errors), §11 (security) and §16 refinements 1–7, 9, 10 (they win on conflict). Decision **D9** in `docs/TRACKER.md` defines Popularity. Evidence: `docs/02-research/libretro-thumbnails.md`, `skyscraper-architecture.md`, `skraper.md`, `layouts.md`. ADRs 0005–0007.

## Global Constraints

- Work on branch `feat/m2-fill`; never switch branches, never push, never touch `main`. Every shell command starts with `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms &&`.
- TDD: write the failing test, run it and see it fail, implement, see it pass.
- After every task: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` (and `pnpm lint && pnpm typecheck && pnpm test` only if the frontend is touched — it is not in this plan).
- Commits: conventional, granular, English. **Never** add `Co-Authored-By` or any Claude attribution.
- `rust-version = "1.90"`, `edition = "2021"` stay as they are. If a crate version needs a newer compiler, pin the newest version whose `rust-version` (see `cargo info <crate>@<ver>`) is ≤ 1.90.
- The core never depends on Tauri and has no UI strings.
- Unit and integration tests **never touch the real network**: use `coverly_core::net::testing::MockServer` (Task 4) and fixtures. Only Task 14 talks to the real servers.
- The real SD at `/Volumes/System` is **strictly read-only**: never create, modify or delete anything under `/Volumes`. Real write tests use a COPY in `/private/tmp/claude-501/-Users-miguelangelesparzacalero-Documents-GitHub/35b7c020-f2ae-412f-b757-99c24a9a498b/scratchpad/sd-copy/`.
- HTTPS only (spec §11); plain `http://` is accepted only for loopback hosts (`127.0.0.1`, `localhost`, `[::1]`) so tests can use the mock server. No telemetry. User-Agent `CoverlyROMs/<version>`.
- Downloaded images are always decoded and re-encoded, never copied raw (spec §11).
- Treat file names and downloaded data as hostile: path traversal in names (`..`, `/`, `\`, `%2F`), decompression bombs and huge images, non-image bytes, oversized bodies.
- Never write outside the library root; writes are atomic (temp file + rename); FAT/exFAT-safe names; never create `._*` AppleDouble files (no xattrs, no `fs::copy` on macOS — it copies xattrs).
- Defaults (spec §9/§16): dry-run ON, only missing art, overwrite `never`, matching method `hash-then-name`, region order `rom,world,usa,europe,japan`, auto-accept threshold 0.90, review threshold 0.65, fuzzy min score 65, max file size to hash 64 MiB, 4 concurrent requests (clamped to 1–6), libretro index TTL 2 days, retries 3 with exponential backoff.
- libretro (spec §16.5): system list from the **live** `https://thumbnails.libretro.com/` root listing; Apache index per system and type; sanitise `& * / : \` " < > ? \ |` → `_`; strip `.png` before scoring; case-insensitive keys; fallback host `https://raw.githubusercontent.com/libretro-thumbnails/<repo>/master/…`; one global limiter.
- libretro-database DATs are downloaded at runtime into the app cache dir, never bundled (spec §17 q3): base `https://raw.githubusercontent.com/libretro/libretro-database/master`.
- Anbernic stock / Garlic / Onion covers: PNG, max 250×360, folder `Imgs` (already in the built-in profiles).

## Review Focus

1. **Hostile names from the network** — an Apache listing href such as `..%2F..%2Fevil.png`, `a%2Fb.png`, `%00.png` or a DAT game name with `/` or `\` must never become a path component: hrefs that decode to anything with `/`, `\`, `..` or control characters are dropped, and the written file name always comes from the ROM stem through the profile template, then `ensure_within(root, …)`. Tests: Task 6 (`apache_index_drops_traversal_and_control_names`) and Task 11 (`refuses_targets_outside_the_root`, `refuses_symlink_escape`).
2. **Garbage behind a 200** — an HTML error page, a truncated PNG or a 9000-pixel-wide image served as `image/png` must fail that one item with `decode` and fall back to the next candidate; the job continues. Tests: Task 10 (`rejects_non_images`, `rejects_oversized_dimensions`) and Task 12 (`bad_image_falls_back_to_next_candidate`).
3. **Interrupted job** — a 429, going offline (spec §10: offline pauses like a rate limit), a full or read-only volume in the middle of `--apply`, or a cancel, must stop scheduling new items, keep the journal of what was written, let `coverly undo` remove exactly those files (even when run from another working directory: the root is journaled as an absolute path), and let a re-run fill only what is still missing. Tests: Task 7 (`going_offline_pauses_gathering`), Task 11 (`begin_requires_an_absolute_root`), Task 12 (`rate_limit_pauses_and_journal_allows_undo`, `going_offline_pauses_the_job`).
4. **Corrupt or foreign cache database** — a garbage `coverly.sqlite3` or one from a newer schema must be set aside and recreated, never block a fill. Test: Task 3 (`corrupt_database_is_set_aside_and_recreated`).
5. **Art that appears between plan and apply, or exists with another case** — with overwrite `never`, an existing `Imgs/zelda.png` blocks writing `Imgs/Zelda.png` (FAT is case-insensitive) and a file the user changed after a job is not deleted by undo. Tests: Task 11 (`never_policy_skips_case_insensitive_twin`, `undo_keeps_files_changed_after_the_job`) and Task 12 (`target_created_after_planning_is_skipped`).

## Deviations from the spec (decided in this plan)

- **7z inner CRC** (§16.2) is deferred: `.7z` ROMs are identified by name (`HashOutcome::NameOnly`). Reading 7z headers needs a heavy dependency; zip (the common case on handheld SDs) is covered.
- **§9 settings** the core needs are typed option structs with spec defaults (`NetConfig`, `IdentifyOptions`, `GatherOptions`, `PlanOptions`, `ApplyOptions`); persistence and the settings UI stay in M5.
- **Backups and journals** live in the app data dir (`<data>/backups/<job>/…`, `<data>/jobs/<job>.jsonl`), not on the SD card, so the handheld never sees extra folders. ADR-0010 records this.
- **CLI review items**: per spec §7.7, `coverly fill --apply` also writes `review` items unless `--auto-only` is given; the dry-run summary always shows how many are `review`.
- **Popularity consensus (D9 part b)** in M2 is measured inside libretro only: `consensus` = how many libretro media listings (`Named_Boxarts`, `Named_Snaps`, `Named_Titles`) contain the same variant file. Cross-source consensus and SteamGridDB votes (D9 part a) arrive with the M6 sources; `Candidate::votes` / `consensus` are already in place, so the strategy does not change.
- **gamelist.xml merge** for the Batocera/Knulli profile (spec §4 "out", §16.8) is not part of M2: `coverly fill` writes the images and prints a warning that `gamelist.xml` is not updated yet when the profile is `batocera`. Tracked as a new tracker row (M4/M5).
- **Offline = pause** (spec §10): a transport failure (`NetError::Offline`) after retries pauses gather and apply exactly like a 403/429, instead of failing every remaining item one by one.

---

## File structure

```
crates/coverly-core/
├─ Cargo.toml                         + deps, feature `test-util`, self dev-dep
├─ src/lib.rs                         + pub mod cache, gather, identify, image, jobs, matching, net, plan, progress, sources, write; mod time
├─ src/paths.rs                       + AppDirs (cache/data dirs, overrides)
├─ src/time.rs                        rfc3339_utc (moved from audit), compact_utc, now_unix
├─ src/systems/{mod.rs,systems.toml}  + `header`, `dats` fields
├─ src/art/mod.rs                     + pub fn target_path
├─ src/identify/mod.rs                Identity, IdMethod, MatchMethod, IdentifyOptions, Identifier
├─ src/identify/hash.rs               HeaderRule, Digests, RomHashes, HashOutcome, hash_rom
├─ src/identify/names.rs              Region, ParsedName, parse_name, compare_key, score, compatible, numbering, sanitise_libretro
├─ src/identify/dat.rs                clrmamepro parser, dat_sources, DatStore (download-on-demand), arcade map
├─ src/cache/mod.rs                   Cache (rusqlite), schema v1, quick ids, kv JSON, DAT rows, art files
├─ src/net/mod.rs                     Http trait, HttpRequest/Response, NetError, NetConfig, UreqHttp, PoliteHttp
├─ src/net/limiter.rs                 global Limiter (concurrency + min interval)
├─ src/net/testing.rs                 MockServer + libretro/DAT fixtures (feature `test-util`)
├─ src/sources/mod.rs                 ArtSource trait, ArtQuery, Candidate, MatchKind, SourceError
├─ src/sources/libretro.rs            LibretroSource, Apache index parsing, URL building, fallback host
├─ src/progress.rs                    ProgressSink, ProgressEvent, Phase, NoProgress
├─ src/gather/mod.rs                  TargetFilter, Scope, select_targets, GatherOptions, gather → Gathered
├─ src/matching/mod.rs                RegionPref, StrategyKind, RankContext, Strategy, Priority, RomRegion, Popularity
├─ src/plan/mod.rs                    PlanOptions, Plan, PlanItem, PlanAction, Counts, build_plan
├─ src/image/mod.rs                   ImageLimits, Fit, OutputSpec, process_image
├─ src/write/mod.rs                   safe_file_name, ensure_within, Writer, Journal, undo_last, list_jobs
└─ src/jobs/mod.rs                    ArtStore, ApplyOptions, apply_plan → JobReport
crates/coverly-core/tests/            identify_hash.rs, names_golden.rs, cache.rs, net.rs, dat.rs, libretro.rs,
                                      gather.rs, strategies.rs, plan.rs, image_pipeline.rs, writer.rs, apply.rs
crates/coverly-cli/src/{main.rs,fill.rs,undo.rs,profile.rs}   (profile.rs: choose_profile/check_root moved from audit.rs)
crates/coverly-cli/tests/cli_fill.rs
docs/04-architecture/{identification.md,art-sources.md,fill-pipeline.md}, docs/adr/{0009,0010}-*.md
```

---

### Task 1: Content hashing with header skip, zip CRC and size cap

**Files:**
- Modify: `Cargo.toml` (workspace deps), `crates/coverly-core/Cargo.toml`, `crates/coverly-core/src/lib.rs`, `crates/coverly-core/src/systems/mod.rs`, `crates/coverly-core/src/systems/systems.toml`
- Create: `crates/coverly-core/src/identify/mod.rs`, `crates/coverly-core/src/identify/hash.rs`
- Test: `crates/coverly-core/tests/identify_hash.rs`, unit tests in `systems/mod.rs`

**Interfaces:**
- Consumes: `systems::{System, SystemDb, SystemsError}` (M1).
- Produces:

```rust
// coverly_core::identify::hash
pub const DEFAULT_HASH_MAX_BYTES: u64 = 64 * 1024 * 1024;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HeaderRule { #[default] Plain, Nes, Fds, SnesCopier, Lynx, A7800 }
impl HeaderRule {
    pub fn parse(s: &str) -> Option<HeaderRule>;               // "plain" | "nes" | "fds" | "snes-copier" | "lynx" | "a7800"
    pub fn header_len(self, head: &[u8], file_len: u64) -> usize;
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Digests { pub crc32: String, pub md5: String, pub sha1: String, pub size: u64 } // upper-case hex
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RomHashes { pub full: Digests, pub headerless: Option<Digests> }
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum HashOutcome {
    Content(RomHashes),
    ZipEntry { crc32: String, size: u64, inner_name: String },
    TooLarge { size: u64 },
    NameOnly,
}
pub fn hash_rom(path: &Path, ext: &str, rule: HeaderRule, max_bytes: u64) -> io::Result<HashOutcome>;
// coverly_core::systems::System gains:  pub header: HeaderRule
// SystemsError gains: BadHeader { id: String, value: String }
```

- [ ] **Step 1: Add dependencies**

Workspace `Cargo.toml`, `[workspace.dependencies]` (append):

```toml
crc32fast = "1.5"
md-5 = "0.10"
sha1 = "0.10"
zip = { version = "4", default-features = false }
```

`crates/coverly-core/Cargo.toml` `[dependencies]` (append): `crc32fast.workspace = true`, `md-5.workspace = true`, `sha1.workspace = true`, `zip.workspace = true`. In `lib.rs` add `pub mod identify;`. Create `identify/mod.rs`:

```rust
//! ROM identification: content hashes, names, DAT lookups (spec §16.2–§16.7).

pub mod hash;
```

- [ ] **Step 2: Write the failing tests**

`crates/coverly-core/tests/identify_hash.rs`:

```rust
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use coverly_core::identify::hash::{hash_rom, HashOutcome, HeaderRule, DEFAULT_HASH_MAX_BYTES};
use tempfile::TempDir;

/// The standard check input and its digests.
const CHECK: &[u8] = b"123456789";
const CRC: &str = "CBF43926";
const MD5: &str = "25F9E794323B453885F5181F1B624D0B";
const SHA1: &str = "F7C3BC1D808E04732ADF679965CCC34CA7AE3441";

fn file(dir: &TempDir, name: &str, bytes: &[u8]) -> PathBuf {
    let p = dir.path().join(name);
    fs::write(&p, bytes).unwrap();
    p
}

fn content(outcome: HashOutcome) -> coverly_core::identify::hash::RomHashes {
    match outcome {
        HashOutcome::Content(h) => h,
        other => panic!("expected content hashes, got {other:?}"),
    }
}

#[test]
fn hashes_a_plain_file_in_one_pass() {
    let dir = TempDir::new().unwrap();
    let p = file(&dir, "Game.gba", CHECK);
    let h = content(hash_rom(&p, "gba", HeaderRule::Plain, DEFAULT_HASH_MAX_BYTES).unwrap());
    assert_eq!(h.full.crc32, CRC);
    assert_eq!(h.full.md5, MD5);
    assert_eq!(h.full.sha1, SHA1);
    assert_eq!(h.full.size, 9);
    assert_eq!(h.headerless, None);
}

#[test]
fn skips_the_ines_header_and_keeps_the_full_digest() {
    let dir = TempDir::new().unwrap();
    let mut bytes = b"NES\x1a".to_vec();
    bytes.extend([0u8; 12]);
    bytes.extend(CHECK);
    let p = file(&dir, "Game.nes", &bytes);
    let h = content(hash_rom(&p, "nes", HeaderRule::Nes, DEFAULT_HASH_MAX_BYTES).unwrap());
    let body = h.headerless.expect("headerless digest");
    assert_eq!((body.crc32.as_str(), body.size), (CRC, 9));
    assert_eq!(h.full.size, 25);
    assert_ne!(h.full.crc32, CRC);
}

#[test]
fn a_rule_without_its_magic_skips_nothing() {
    let dir = TempDir::new().unwrap();
    let p = file(&dir, "Game.nes", CHECK);
    let h = content(hash_rom(&p, "nes", HeaderRule::Nes, DEFAULT_HASH_MAX_BYTES).unwrap());
    assert_eq!(h.headerless, None);
}

#[test]
fn header_rules_detect_their_headers() {
    let payload = vec![7u8; 1024];
    let crc = format!("{:08X}", crc32fast::hash(&payload));
    let mut lynx = b"LYNX".to_vec();
    lynx.extend([0u8; 60]);
    let mut a7800 = vec![1u8];
    a7800.extend(b"ATARI7800");
    a7800.extend([0u8; 118]);
    let mut fds = b"FDS\x1a".to_vec();
    fds.extend([0u8; 12]);
    let copier = vec![0u8; 512];
    for (rule, header) in [
        (HeaderRule::Lynx, lynx),
        (HeaderRule::A7800, a7800),
        (HeaderRule::Fds, fds),
        (HeaderRule::SnesCopier, copier),
    ] {
        let dir = TempDir::new().unwrap();
        let mut bytes = header.clone();
        bytes.extend(&payload);
        let p = file(&dir, "rom.bin", &bytes);
        let h = content(hash_rom(&p, "bin", rule, DEFAULT_HASH_MAX_BYTES).unwrap());
        let body = h.headerless.unwrap_or_else(|| panic!("{rule:?}: no headerless digest"));
        assert_eq!(body.crc32, crc, "{rule:?}");
        assert_eq!(body.size, 1024, "{rule:?}");
    }
}

#[test]
fn single_file_zip_uses_the_central_directory_crc() {
    let dir = TempDir::new().unwrap();
    let p = dir.path().join("Game.zip");
    let mut zip = zip::ZipWriter::new(fs::File::create(&p).unwrap());
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored);
    zip.start_file("Game (USA).gba", opts).unwrap();
    zip.write_all(CHECK).unwrap();
    zip.finish().unwrap();
    assert_eq!(
        hash_rom(&p, "ZIP", HeaderRule::Plain, 1).unwrap(),
        HashOutcome::ZipEntry {
            crc32: CRC.into(),
            size: 9,
            inner_name: "Game (USA).gba".into()
        },
        "zip CRC is read even above the size cap: nothing is decompressed"
    );
}

#[test]
fn multi_file_zips_and_disc_formats_are_identified_by_name() {
    let dir = TempDir::new().unwrap();
    let p = dir.path().join("set.zip");
    let mut zip = zip::ZipWriter::new(fs::File::create(&p).unwrap());
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored);
    for name in ["a.bin", "b.bin"] {
        zip.start_file(name, opts).unwrap();
        zip.write_all(CHECK).unwrap();
    }
    zip.finish().unwrap();
    assert_eq!(hash_rom(&p, "zip", HeaderRule::Plain, DEFAULT_HASH_MAX_BYTES).unwrap(), HashOutcome::NameOnly);
    for ext in ["cue", "CUE", "gdi", "m3u", "chd", "7z", "pbp"] {
        let q = file(&dir, &format!("game.{ext}"), CHECK);
        assert_eq!(hash_rom(&q, ext, HeaderRule::Plain, DEFAULT_HASH_MAX_BYTES).unwrap(), HashOutcome::NameOnly, "{ext}");
    }
}

#[test]
fn files_over_the_cap_are_not_read() {
    let dir = TempDir::new().unwrap();
    let p = file(&dir, "big.gba", CHECK);
    assert_eq!(hash_rom(&p, "gba", HeaderRule::Plain, 4).unwrap(), HashOutcome::TooLarge { size: 9 });
}

#[test]
fn a_corrupt_zip_is_hashed_as_plain_content() {
    let dir = TempDir::new().unwrap();
    let p = file(&dir, "bad.zip", CHECK);
    let h = content(hash_rom(&p, "zip", HeaderRule::Plain, DEFAULT_HASH_MAX_BYTES).unwrap());
    assert_eq!(h.full.crc32, CRC);
}
```

Add `zip.workspace = true` and `crc32fast.workspace = true` are regular deps, so the test can use them. Unit tests in `systems/mod.rs` `mod tests`:

```rust
#[test]
fn builtin_header_rules() {
    use crate::identify::hash::HeaderRule;
    let db = SystemDb::builtin();
    for (id, rule) in [
        ("nes", HeaderRule::Nes),
        ("fds", HeaderRule::Fds),
        ("snes", HeaderRule::SnesCopier),
        ("lynx", HeaderRule::Lynx),
        ("a7800", HeaderRule::A7800),
        ("gba", HeaderRule::Plain),
    ] {
        assert_eq!(db.get(id).unwrap().header, rule, "{id}");
    }
}

#[test]
fn unknown_header_rule_is_an_error() {
    let err = SystemDb::builtin()
        .with_overrides("[[system]]\nid = \"gba\"\nheader = \"bogus\"\n")
        .unwrap_err();
    assert!(matches!(err, SystemsError::BadHeader { .. }), "{err}");
}
```

- [ ] **Step 3: Run the tests to see them fail**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && cargo test -p coverly-core --test identify_hash && cargo test -p coverly-core --lib systems`
Expected: FAIL to compile (`identify::hash` and `System::header` do not exist).

- [ ] **Step 4: Implement `identify/hash.rs`**

```rust
//! Content hashing of ROM files (spec §16.2): CRC32, MD5 and SHA1 in one
//! streaming pass, with an optional copier/emulator header skipped for a
//! second set of digests, and the inner CRC of a single-file zip read from
//! the central directory without decompressing. Read-only.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use sha1::Sha1;

/// Files larger than this are not hashed by default (spec §9 "max file size to hash").
pub const DEFAULT_HASH_MAX_BYTES: u64 = 64 * 1024 * 1024;
/// Formats identified by name: multi-file sets, compressed discs, 7z (deferred).
const NAME_ONLY_EXTS: [&str; 9] = ["cue", "gdi", "m3u", "ccd", "chd", "7z", "pbp", "cso", "rvz"];
const CHUNK: usize = 64 * 1024;
/// A zip with more entries than this is not a single ROM; do not walk it.
const MAX_ZIP_ENTRIES: usize = 4096;

/// Header present in some dumps but not in the DAT's headerless entries.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HeaderRule {
    /// No header.
    #[default]
    Plain,
    /// iNES: `NES\x1a`, 16 bytes.
    Nes,
    /// fwNES: `FDS\x1a`, 16 bytes.
    Fds,
    /// SNES copier header: 512 bytes when `len % 1024 == 512`.
    SnesCopier,
    /// Lynx: `LYNX`, 64 bytes.
    Lynx,
    /// Atari 7800: `ATARI7800` at offset 1, 128 bytes.
    A7800,
}

impl HeaderRule {
    /// Parses a `systems.toml` value.
    pub fn parse(s: &str) -> Option<HeaderRule> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "" | "plain" => HeaderRule::Plain,
            "nes" => HeaderRule::Nes,
            "fds" => HeaderRule::Fds,
            "snes-copier" => HeaderRule::SnesCopier,
            "lynx" => HeaderRule::Lynx,
            "a7800" => HeaderRule::A7800,
            _ => return None,
        })
    }

    /// Length of the header at the start of a file whose first bytes are
    /// `head` and whose size is `file_len`; 0 when absent.
    pub fn header_len(self, head: &[u8], file_len: u64) -> usize {
        match self {
            HeaderRule::Nes if head.starts_with(b"NES\x1a") => 16,
            HeaderRule::Fds if head.starts_with(b"FDS\x1a") => 16,
            HeaderRule::Lynx if head.starts_with(b"LYNX") => 64,
            HeaderRule::A7800 if head.get(1..10) == Some(b"ATARI7800".as_slice()) => 128,
            HeaderRule::SnesCopier if file_len % 1024 == 512 => 512,
            _ => 0,
        }
    }
}

/// Digests of a byte stream, upper-case hex.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Digests {
    pub crc32: String,
    pub md5: String,
    pub sha1: String,
    pub size: u64,
}

/// Digests of the whole file and, when a header was found, of the rest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RomHashes {
    pub full: Digests,
    pub headerless: Option<Digests>,
}

/// What hashing a ROM produced.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum HashOutcome {
    /// The file contents were hashed.
    Content(RomHashes),
    /// A zip holding one file: its CRC from the central directory.
    ZipEntry { crc32: String, size: u64, inner_name: String },
    /// Larger than the cap: identify by name.
    TooLarge { size: u64 },
    /// A format identified by name.
    NameOnly,
}

/// Hashes `path` (extension `ext`, any case) with `rule`, reading nothing
/// when the format is name-only and only the central directory of a zip.
pub fn hash_rom(path: &Path, ext: &str, rule: HeaderRule, max_bytes: u64) -> io::Result<HashOutcome> {
    let ext = ext.to_ascii_lowercase();
    if NAME_ONLY_EXTS.contains(&ext.as_str()) {
        return Ok(HashOutcome::NameOnly);
    }
    let mut file = File::open(path)?;
    let size = file.metadata()?.len();
    if ext == "zip" {
        if let Some(outcome) = zip_outcome(&mut file) {
            return Ok(outcome);
        }
        file.seek(SeekFrom::Start(0))?;
    }
    if size > max_bytes {
        return Ok(HashOutcome::TooLarge { size });
    }
    stream(file, size, rule).map(HashOutcome::Content)
}

/// `None` when the file is not a readable zip (then it is hashed as is).
fn zip_outcome(file: &mut File) -> Option<HashOutcome> {
    let mut archive = zip::ZipArchive::new(&mut *file).ok()?;
    if archive.len() > MAX_ZIP_ENTRIES {
        return Some(HashOutcome::NameOnly);
    }
    let mut found = None;
    for i in 0..archive.len() {
        let Ok(entry) = archive.by_index_raw(i) else {
            return Some(HashOutcome::NameOnly);
        };
        if entry.is_dir() {
            continue;
        }
        if found.is_some() {
            return Some(HashOutcome::NameOnly);
        }
        found = Some((entry.crc32(), entry.size(), entry.name().to_string()));
    }
    Some(match found {
        Some((crc, size, inner_name)) => HashOutcome::ZipEntry { crc32: format!("{crc:08X}"), size, inner_name },
        None => HashOutcome::NameOnly,
    })
}

struct Hasher {
    crc: crc32fast::Hasher,
    md5: Md5,
    sha1: Sha1,
    size: u64,
}

impl Hasher {
    fn new() -> Hasher {
        Hasher { crc: crc32fast::Hasher::new(), md5: Md5::new(), sha1: Sha1::new(), size: 0 }
    }
    fn update(&mut self, bytes: &[u8]) {
        self.crc.update(bytes);
        self.md5.update(bytes);
        self.sha1.update(bytes);
        self.size += bytes.len() as u64;
    }
    fn finish(self) -> Digests {
        Digests {
            crc32: format!("{:08X}", self.crc.finalize()),
            md5: hex_upper(&self.md5.finalize()),
            sha1: hex_upper(&self.sha1.finalize()),
            size: self.size,
        }
    }
}

pub(crate) fn hex_upper(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

/// One pass: every byte feeds `full`, bytes after the header feed `body`.
fn stream(mut file: File, size: u64, rule: HeaderRule) -> io::Result<RomHashes> {
    let mut buf = vec![0u8; CHUNK];
    let mut full = Hasher::new();
    let mut body = Hasher::new();
    let mut header = None;
    let mut to_skip = 0usize;
    loop {
        let n = read_full(&mut file, &mut buf)?;
        if n == 0 {
            break;
        }
        let chunk = &buf[..n];
        if header.is_none() {
            let len = rule.header_len(chunk, size);
            header = Some(len);
            to_skip = len;
        }
        full.update(chunk);
        let skip = to_skip.min(n);
        to_skip -= skip;
        body.update(&chunk[skip..]);
    }
    let header = header.unwrap_or(0) as u64;
    Ok(RomHashes {
        full: full.finish(),
        headerless: (header > 0 && size > header).then(|| body.finish()),
    })
}

/// Reads until `buf` is full or EOF, so the first chunk holds the header.
fn read_full(file: &mut File, buf: &mut [u8]) -> io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        match file.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(filled)
}
```

Note: `HeaderRule::A7800` compares `head.get(1..10)` with `Some(b"ATARI7800".as_slice())`.

- [ ] **Step 5: Add `header` to systems**

In `systems/mod.rs`: `System` gains `/// Header rule for content hashing. pub header: HeaderRule,` (import `crate::identify::hash::HeaderRule`); `RawSystem` gains `header: Option<String>`; `SystemsError` gains

```rust
/// A `header` value is not a known rule.
#[error("system `{id}`: unknown header rule `{value}` (expected plain, nes, fds, snes-copier, lynx or a7800)")]
BadHeader { id: String, value: String },
```

In `merge`, new systems start with `header: HeaderRule::Plain`; when `raw.header` is `Some(v)`, set `system.header = HeaderRule::parse(&v).ok_or(SystemsError::BadHeader { id: id.clone(), value: v })?` (parse before taking `&mut self.systems[index]` to satisfy the borrow checker). In `systems.toml` add `header = "nes"` to `nes`, `header = "fds"` to `fds`, `header = "snes-copier"` to `snes`, `header = "lynx"` to `lynx`, `header = "a7800"` to `a7800`, and document `header` in the file's comment block. Fix any `System { … }` literal in tests by adding `header: HeaderRule::Plain`.

- [ ] **Step 6: Run the tests to see them pass**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && cargo test -p coverly-core --test identify_hash && cargo test -p coverly-core --lib systems`
Expected: PASS. Then the three global checks (fmt, clippy, workspace tests).

- [ ] **Step 7: Commit**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && git add Cargo.toml Cargo.lock crates/coverly-core && git commit -m "feat(core): hash ROM contents with header skip and zip CRC"
```

---

### Task 2: Name parsing, regions and fuzzy scoring

**Files:**
- Create: `crates/coverly-core/src/identify/names.rs`
- Modify: `crates/coverly-core/src/identify/mod.rs` (`pub mod names;`)
- Test: `crates/coverly-core/tests/names_golden.rs`

**Interfaces:**
- Consumes: `unicode-normalization` (already a dependency).
- Produces:

```rust
// coverly_core::identify::names
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Region { World, Usa, Europe, Japan, Asia, Australia, Brazil, Canada, China, France,
                  Germany, HongKong, Italy, Korea, Netherlands, Spain, Sweden, Taiwan, UnitedKingdom }
impl Region {
    pub fn from_no_intro(word: &str) -> Option<Region>;   // "USA", "Europe", "Hong Kong"…
    pub fn from_goodtools(code: char) -> Option<Region>;  // 'U','E','J','W','A','B','C','F','G','H','I','K','S'
    pub fn parse(s: &str) -> Option<Region>;              // CLI/config: no-intro word, or "us","eu","jp","uk"
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedName {
    pub title: String,        // brackets removed, "_"→" ", ", The" moved to the front, numbering stripped
    pub regions: Vec<Region>, // in name order, deduplicated
    pub year: Option<u16>,    // "(1983)"
    pub numeral: u32,         // sequel number after roman→arabic, default 1
    pub key: String,          // compare_key(title)
    pub short_key: String,    // compare_key(without_subtitle(title))
}
pub fn parse_name(stem: &str, strip_numbering: bool) -> ParsedName;
pub fn compare_key(title: &str) -> String;
pub fn without_subtitle(title: &str) -> String;
pub fn score(a: &str, b: &str) -> u8;                    // 100*(L-lev)/L on chars
pub fn compatible(rom: &ParsedName, candidate: &ParsedName) -> bool; // numeral + year guards
pub fn strip_numbering_prefix(stem: &str) -> &str;       // "001 Golden Sun" → "Golden Sun"
pub fn detect_numbering<'a>(stems: impl IntoIterator<Item = &'a str>) -> bool;
pub fn sanitise_libretro(name: &str) -> String;          // & * / : ` " < > ? \ | → _
pub fn libretro_key(file_name: &str) -> String;          // strip ".png" (any case), NFC, lowercase
```

- [ ] **Step 1: Write the failing golden tests**

`crates/coverly-core/tests/names_golden.rs`:

```rust
use coverly_core::identify::names::*;
use Region::*;

#[test]
fn parses_real_world_names() {
    // (stem, strip_numbering, title, regions, year, numeral, key)
    let cases: &[(&str, bool, &str, &[Region], Option<u16>, u32, &str)] = &[
        ("Legend of Zelda, The - A Link to the Past (USA)", false,
         "The Legend of Zelda - A Link to the Past", &[Usa], None, 1, "legend of zelda a link to the past"),
        ("Super Mario World (USA, Europe) (Rev 1)", false, "Super Mario World", &[Usa, Europe], None, 1, "super mario world"),
        ("Street Fighter II' Turbo (J) [!]", false, "Street Fighter II' Turbo", &[Japan], None, 2, "street fighter 2 turbo"),
        ("Contra III - The Alien Wars (JUE)", false, "Contra III - The Alien Wars", &[Japan, Usa, Europe], None, 3, "contra 3 the alien wars"),
        ("001 Golden Sun", true, "Golden Sun", &[], None, 1, "golden sun"),
        ("Pokémon - Ruby Version (USA, Europe)", false, "Pokémon - Ruby Version", &[Usa, Europe], None, 1, "pokemon ruby version"),
        ("Adventures of Batman & Robin, The (USA)", false, "The Adventures of Batman & Robin", &[Usa], None, 1, "adventures of batman and robin"),
        ("Q-Bert's Qubes (USA) (1983) (Parker Brothers) [h]", false, "Q-Bert's Qubes", &[Usa], Some(1983), 1, "q berts qubes"),
        ("Mega_Man_X_(U)", false, "Mega Man X", &[Usa], None, 10, "mega man 10"),
        ("Beta Game (Beta) (Unl)", false, "Beta Game", &[], None, 1, "beta game"),
        // Lower-casing `İ` changes its UTF-8 length: byte offsets from a
        // lower-cased copy must never index the original (hostile names must not panic).
        ("İstanbul Racer, The (Turkey)", false, "The İstanbul Racer", &[], None, 1, "istanbul racer"),
    ];
    for (stem, strip, title, regions, year, numeral, key) in cases {
        let p = parse_name(stem, *strip);
        assert_eq!(p.title, *title, "{stem}");
        assert_eq!(p.regions, *regions, "{stem}");
        assert_eq!(p.year, *year, "{stem}");
        assert_eq!(p.numeral, *numeral, "{stem}");
        assert_eq!(p.key, *key, "{stem}");
    }
}

#[test]
fn short_key_drops_the_subtitle() {
    assert_eq!(parse_name("Golden Sun - The Lost Age (USA)", false).short_key, "golden sun");
    assert_eq!(parse_name("Metroid: Zero Mission", false).short_key, "metroid");
}

#[test]
fn guards_reject_other_sequels_and_years() {
    let p = |s| parse_name(s, false);
    assert!(!compatible(&p("Street Fighter II (USA)"), &p("Street Fighter III (USA)")));
    assert!(!compatible(&p("Final Fantasy III (J)"), &p("Final Fantasy VI (USA)")));
    assert!(!compatible(&p("Contra (USA)"), &p("Contra III - The Alien Wars (USA)")));
    assert!(!compatible(&p("Q-Bert (1983)"), &p("Q-Bert (1999)")));
    assert!(compatible(&p("Contra (USA)"), &p("Contra Force (USA)")));
    assert!(compatible(&p("Q-Bert"), &p("Q-Bert (1999)")));
}

#[test]
fn score_counts_unicode_characters_not_bytes() {
    assert_eq!(score("super mario world", "super mario world"), 100);
    assert_eq!(score("aé", "ae"), 50); // bytes would give 33
    assert_eq!(score("ドラクエ", "ドラクエx"), 80); // bytes would give 93
    assert_eq!(score("", ""), 100);
    assert_eq!(score("abc", ""), 0);
}

#[test]
fn numbering_prefixes() {
    assert_eq!(strip_numbering_prefix("001 Golden Sun"), "Golden Sun");
    assert_eq!(strip_numbering_prefix("12) Foo"), "Foo");
    assert_eq!(strip_numbering_prefix("7. Bar"), "Bar");
    assert_eq!(strip_numbering_prefix("1942"), "1942");
    assert_eq!(strip_numbering_prefix("Zelda"), "Zelda");
    assert!(detect_numbering(["001 A", "002 B", "003 C", "Zelda"]));
    assert!(!detect_numbering(["1942", "Zelda", "Mario"]));
    assert!(!detect_numbering(["001 A", "002 B"])); // too few to tell
}

#[test]
fn libretro_sanitising_and_keys() {
    assert_eq!(sanitise_libretro("A&B*C/D:E`F\"G<H>I?J\\K|L"), "A_B_C_D_E_F_G_H_I_J_K_L");
    assert_eq!(sanitise_libretro("Batman & Robin"), "Batman _ Robin");
    assert_eq!(libretro_key("Super Mario World (USA).PNG"), "super mario world (usa)");
    assert_eq!(libretro_key("Cafe\u{301}.png"), "café"); // NFD input → NFC key
}

#[test]
fn region_words_for_config() {
    assert_eq!(Region::parse("usa"), Some(Usa));
    assert_eq!(Region::parse("EU"), Some(Europe));
    assert_eq!(Region::parse("jp"), Some(Japan));
    assert_eq!(Region::parse("World"), Some(World));
    assert_eq!(Region::parse("mars"), None);
}
```

- [ ] **Step 2: Run to see it fail**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && cargo test -p coverly-core --test names_golden`
Expected: FAIL to compile (`identify::names` missing).

- [ ] **Step 3: Implement `identify/names.rs`**

```rust
//! ROM and art name parsing, normalisation and fuzzy scoring
//! (spec §16.4 and §16.7, after Skyscraper's getCompareTitle /
//! getUrlQueryName, but on Unicode characters).

use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Region {
    World, Usa, Europe, Japan, Asia, Australia, Brazil, Canada, China, France,
    Germany, HongKong, Italy, Korea, Netherlands, Spain, Sweden, Taiwan, UnitedKingdom,
}

impl Region {
    pub fn from_no_intro(word: &str) -> Option<Region> {
        use Region::*;
        Some(match word.trim().to_lowercase().as_str() {
            "world" => World, "usa" => Usa, "europe" => Europe, "japan" => Japan,
            "asia" => Asia, "australia" => Australia, "brazil" => Brazil, "canada" => Canada,
            "china" => China, "france" => France, "germany" => Germany, "hong kong" => HongKong,
            "italy" => Italy, "korea" => Korea, "netherlands" => Netherlands, "spain" => Spain,
            "sweden" => Sweden, "taiwan" => Taiwan, "uk" | "united kingdom" => UnitedKingdom,
            _ => return None,
        })
    }

    pub fn from_goodtools(code: char) -> Option<Region> {
        use Region::*;
        Some(match code {
            'W' => World, 'U' => Usa, 'E' => Europe, 'J' => Japan, 'A' => Australia,
            'B' => Brazil, 'C' => China, 'F' => France, 'G' => Germany, 'H' => Netherlands,
            'I' => Italy, 'K' => Korea, 'S' => Spain,
            _ => return None,
        })
    }

    pub fn parse(s: &str) -> Option<Region> {
        match s.trim().to_lowercase().as_str() {
            "us" => Some(Region::Usa),
            "eu" => Some(Region::Europe),
            "jp" | "jpn" => Some(Region::Japan),
            other => Region::from_no_intro(other),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedName {
    pub title: String,
    pub regions: Vec<Region>,
    pub year: Option<u16>,
    pub numeral: u32,
    pub key: String,
    pub short_key: String,
}

pub fn parse_name(stem: &str, strip_numbering: bool) -> ParsedName {
    let nfc: String = stem.nfc().collect();
    let base = if strip_numbering { strip_numbering_prefix(&nfc) } else { nfc.as_str() };
    let (outside, groups) = split_groups(&base.replace('_', " "));
    let mut regions: Vec<Region> = Vec::new();
    let mut year = None;
    for group in &groups {
        if let Some(found) = region_group(group) {
            for r in found {
                if !regions.contains(&r) {
                    regions.push(r);
                }
            }
        } else if let Some(y) = year_group(group) {
            year.get_or_insert(y);
        }
    }
    let title = tidy_title(&outside);
    let key = compare_key(&title);
    let short_key = compare_key(&without_subtitle(&title));
    let numeral = numeral_of(&key);
    ParsedName { title, regions, year, numeral, key, short_key }
}

/// Text outside `()`, `[]` and `{}`, and the contents of top-level `()`
/// groups. An unclosed bracket is kept as text.
fn split_groups(s: &str) -> (String, Vec<String>) {
    let mut outside = String::new();
    let mut groups = Vec::new();
    let mut stack: Vec<char> = Vec::new();
    let mut current = String::new();
    let mut pending = String::new(); // raw text of the open group, for unclosed brackets
    for c in s.chars() {
        match c {
            '(' | '[' | '{' => {
                stack.push(c);
                pending.push(c);
                if stack.len() > 1 { current.push(c); }
            }
            ')' | ']' | '}' if !stack.is_empty() => {
                let open = stack.pop().unwrap();
                pending.push(c);
                if stack.is_empty() {
                    if open == '(' { groups.push(current.trim().to_string()); }
                    current.clear();
                    pending.clear();
                } else {
                    current.push(c);
                }
            }
            _ if !stack.is_empty() => { current.push(c); pending.push(c); }
            _ => outside.push(c),
        }
    }
    outside.push_str(&pending);
    (outside, groups)
}

fn region_group(group: &str) -> Option<Vec<Region>> {
    let parts: Vec<&str> = group.split(',').map(str::trim).collect();
    if let Some(all) = parts.iter().map(|p| Region::from_no_intro(p)).collect::<Option<Vec<_>>>() {
        return Some(all);
    }
    let g = group.trim();
    if (1..=4).contains(&g.len()) && g.chars().all(|c| c.is_ascii_uppercase()) {
        return g.chars().map(Region::from_goodtools).collect();
    }
    None
}

fn year_group(group: &str) -> Option<u16> {
    let g = group.trim();
    (g.len() == 4 && g.chars().all(|c| c.is_ascii_digit()))
        .then(|| g.parse::<u16>().ok())
        .flatten()
        .filter(|y| (1970..=2099).contains(y))
}

/// Collapses spaces, trims separators and moves ", The" to the front.
fn tidy_title(raw: &str) -> String {
    let collapsed = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut title = collapsed.trim_matches(|c: char| c == '-' || c == ' ' || c == ',').to_string();
    // ASCII lower-casing keeps byte offsets identical, so `i` indexes `title` safely.
    let lower = title.to_ascii_lowercase();
    if let Some(i) = lower.find(", the") {
        let after = &title[i + 5..];
        if after.is_empty() || after.starts_with(" -") || after.starts_with(':') {
            title = format!("The {}{}", &title[..i], after);
        }
    }
    title
}

/// Case-, accent- and punctuation-insensitive key with roman numerals as
/// arabic numbers and a leading "the" dropped.
pub fn compare_key(title: &str) -> String {
    let folded: String = title
        .nfd()
        .filter(|c| !('\u{0300}'..='\u{036F}').contains(c))
        .nfc()
        .collect::<String>()
        .to_lowercase()
        .replace('&', " and ");
    let mut cleaned = String::with_capacity(folded.len());
    for c in folded.chars() {
        if c.is_alphanumeric() {
            cleaned.push(c);
        } else if c != '\'' && c != '\u{2019}' {
            cleaned.push(' ');
        }
    }
    let mut tokens: Vec<String> = cleaned.split_whitespace().map(str::to_string).collect();
    if tokens.len() > 1 && tokens[0] == "the" {
        tokens.remove(0);
    }
    let last = tokens.len().saturating_sub(1);
    for (i, tok) in tokens.iter_mut().enumerate() {
        if let Some(n) = roman(tok, i == last && i > 0) {
            *tok = n.to_string();
        }
    }
    tokens.join(" ")
}

/// II..XX anywhere; single-letter V and X only as the last word.
fn roman(tok: &str, single_ok: bool) -> Option<u32> {
    const R: [(&str, u32); 19] = [
        ("ii", 2), ("iii", 3), ("iv", 4), ("v", 5), ("vi", 6), ("vii", 7), ("viii", 8),
        ("ix", 9), ("x", 10), ("xi", 11), ("xii", 12), ("xiii", 13), ("xiv", 14), ("xv", 15),
        ("xvi", 16), ("xvii", 17), ("xviii", 18), ("xix", 19), ("xx", 20),
    ];
    let n = R.iter().find(|(r, _)| *r == tok)?.1;
    (tok.len() > 1 || single_ok).then_some(n)
}

/// The last 1–2 digit number after the first word; 1 when none.
fn numeral_of(key: &str) -> u32 {
    key.split(' ')
        .skip(1)
        .filter(|t| (1..=2).contains(&t.len()) && t.chars().all(|c| c.is_ascii_digit()))
        .filter_map(|t| t.parse().ok())
        .last()
        .unwrap_or(1)
}

pub fn without_subtitle(title: &str) -> String {
    let cut = [": ", " - ", " ~ "].iter().filter_map(|sep| title.find(sep)).min();
    match cut {
        Some(i) if title[..i].trim().chars().count() > 3 => title[..i].trim().to_string(),
        _ => title.to_string(),
    }
}

pub fn score(a: &str, b: &str) -> u8 {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let l = a.len().max(b.len());
    if l == 0 {
        return 100;
    }
    (100 * (l - levenshtein(&a, &b)) / l) as u8
}

fn levenshtein(a: &[char], b: &[char]) -> usize {
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            cur[j + 1] = (prev[j + 1] + 1).min(cur[j] + 1).min(prev[j] + usize::from(ca != cb));
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

pub fn compatible(rom: &ParsedName, candidate: &ParsedName) -> bool {
    rom.numeral == candidate.numeral
        && match (rom.year, candidate.year) {
            (Some(a), Some(b)) => a == b,
            _ => true,
        }
}

/// Strips `NNN `, `NN) `, `N. ` or `N - ` (1–4 digits) when text follows.
pub fn strip_numbering_prefix(stem: &str) -> &str {
    let digits = stem.chars().take_while(|c| c.is_ascii_digit()).count();
    if !(1..=4).contains(&digits) {
        return stem;
    }
    let rest = &stem[digits..];
    let rest = rest
        .strip_prefix(')')
        .or_else(|| rest.strip_prefix('.'))
        .or_else(|| rest.trim_start().strip_prefix("- "))
        .unwrap_or(rest);
    if !rest.starts_with(' ') && rest.len() == stem.len() - digits {
        return stem; // "1942", "10yard"
    }
    let rest = rest.trim_start();
    if rest.is_empty() { stem } else { rest }
}

/// Whether a folder numbers its ROMs: at least 3 stems and 60 % of them.
pub fn detect_numbering<'a>(stems: impl IntoIterator<Item = &'a str>) -> bool {
    let (mut total, mut numbered) = (0usize, 0usize);
    for s in stems {
        total += 1;
        if strip_numbering_prefix(s) != s {
            numbered += 1;
        }
    }
    numbered >= 3 && numbered * 10 >= total * 6
}

pub fn sanitise_libretro(name: &str) -> String {
    name.chars().map(|c| if "&*/:`\"<>?\\|".contains(c) { '_' } else { c }).collect()
}

pub fn libretro_key(file_name: &str) -> String {
    let lower = file_name.nfc().collect::<String>().to_lowercase();
    lower.strip_suffix(".png").map(str::to_string).unwrap_or(lower)
}
```

Check `strip_numbering_prefix("12) Foo")`: digits=2, rest=") Foo" → strip ')' → " Foo" → starts with ' ' → "Foo". `"7. Bar"` → ". Bar" → " Bar" → "Bar". `"1942"` → rest "" → `rest.len() == stem.len()-digits` and not starting with ' ' → stem. `"001 Golden Sun"` → rest " Golden Sun" → "Golden Sun". If any golden case fails, fix the implementation, not the expectation (expectations encode the spec).

- [ ] **Step 4: Run to see it pass**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && cargo test -p coverly-core --test names_golden`
Expected: PASS. Then the three global checks.

- [ ] **Step 5: Commit**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && git add crates/coverly-core && git commit -m "feat(core): parse ROM names and regions and score fuzzy matches"
```

---

### Task 3: SQLite cache and app directories (M1-05)

**Files:**
- Modify: `Cargo.toml`, `crates/coverly-core/Cargo.toml`, `crates/coverly-core/src/lib.rs`, `crates/coverly-core/src/paths.rs`
- Create: `crates/coverly-core/src/cache/mod.rs`, `crates/coverly-core/src/time.rs`
- Modify: `crates/coverly-core/src/audit/mod.rs` (use `crate::time::rfc3339_utc`)
- Test: `crates/coverly-core/tests/cache.rs`

**Interfaces:**
- Consumes: `identify::hash::HashOutcome` (Task 1).
- Produces:

```rust
// coverly_core::cache
pub const SCHEMA_VERSION: i32 = 1;
pub const CACHE_FILE: &str = "coverly.sqlite3";
#[derive(Debug, thiserror::Error)]
pub enum CacheError { #[error("cache database: {0}")] Sql(#[from] rusqlite::Error),
                      #[error("cache data: {0}")] Json(#[from] serde_json::Error),
                      #[error("cache folder: {0}")] Io(#[from] std::io::Error) }
pub struct Stored<T> { pub value: T, pub fetched_at: i64, pub etag: Option<String> }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatRow { pub game: String, pub rom_name: String, pub size: Option<u64>,
                    pub crc32: Option<String>, pub md5: Option<String>, pub sha1: Option<String> }
pub struct Cache { /* Mutex<rusqlite::Connection> */ }       // Send + Sync
impl Cache {
    pub fn open(path: &Path) -> Result<Cache, CacheError>;   // creates parents; corrupt/newer → set aside + recreate
    pub fn in_memory() -> Result<Cache, CacheError>;
    pub fn quick_id(&self, path: &Path, size: u64, mtime_ns: i64) -> Result<Option<HashOutcome>, CacheError>;
    pub fn put_quick_id(&self, path: &Path, size: u64, mtime_ns: i64, outcome: &HashOutcome) -> Result<(), CacheError>;
    pub fn get_json<T: DeserializeOwned>(&self, ns: &str, key: &str) -> Result<Option<Stored<T>>, CacheError>;
    pub fn put_json<T: Serialize>(&self, ns: &str, key: &str, value: &T, etag: Option<&str>) -> Result<(), CacheError>;
    pub fn touch_json(&self, ns: &str, key: &str) -> Result<(), CacheError>; // fetched_at = now (HTTP 304)
    pub fn replace_dat(&self, dat: &str, rows: &[DatRow]) -> Result<(), CacheError>;  // one transaction
    pub fn dat_by_crc(&self, dat: &str, crc32: &str) -> Result<Vec<DatRow>, CacheError>; // ordered by game
    pub fn dat_by_rom_stem(&self, dat: &str, stem: &str) -> Result<Vec<DatRow>, CacheError>; // case-insensitive
    pub fn put_art_file(&self, url: &str, sha1: &str, len: u64) -> Result<(), CacheError>;
    pub fn art_file(&self, url: &str) -> Result<Option<(String, u64)>, CacheError>;
}
pub fn mtime_ns(t: SystemTime) -> i64;
// coverly_core::paths
pub struct AppDirs { pub cache: PathBuf, pub data: PathBuf }
impl AppDirs { pub fn resolve(cache: Option<&Path>, data: Option<&Path>) -> Option<AppDirs>; }
// crate::time (pub(crate))
pub(crate) fn rfc3339_utc(t: SystemTime) -> String;  // moved from audit
pub(crate) fn compact_utc(t: SystemTime) -> String;  // "20261016T030000Z"
pub fn now_unix() -> i64;                            // re-exported as coverly_core::cache::now_unix
```

- [ ] **Step 1: Dependencies**

Workspace: `rusqlite = { version = "0.37", features = ["bundled"] }` (use the newest 0.x whose `rust-version` ≤ 1.90; check with `cargo info rusqlite@<ver>`), `directories = "6"`. Core deps: `rusqlite.workspace = true`, `directories.workspace = true`, `serde_json.workspace = true` (move from dev-deps to deps). `lib.rs`: `pub mod cache; mod time;`.

- [ ] **Step 2: Write the failing tests**

`crates/coverly-core/tests/cache.rs`:

```rust
use std::fs;
use std::path::Path;

use coverly_core::cache::{Cache, DatRow, CACHE_FILE};
use coverly_core::identify::hash::HashOutcome;
use coverly_core::paths::AppDirs;
use tempfile::TempDir;

fn zip_outcome() -> HashOutcome {
    HashOutcome::ZipEntry { crc32: "CBF43926".into(), size: 9, inner_name: "a.gba".into() }
}

#[test]
fn quick_ids_are_reused_only_for_the_same_size_and_mtime() {
    let cache = Cache::in_memory().unwrap();
    let p = Path::new("/lib/ROMS/GBA/a.zip");
    cache.put_quick_id(p, 9, 1_000, &zip_outcome()).unwrap();
    assert_eq!(cache.quick_id(p, 9, 1_000).unwrap(), Some(zip_outcome()));
    assert_eq!(cache.quick_id(p, 10, 1_000).unwrap(), None);
    assert_eq!(cache.quick_id(p, 9, 2_000).unwrap(), None);
}

#[test]
fn json_values_round_trip_with_etag() {
    let cache = Cache::in_memory().unwrap();
    cache.put_json("libretro-index", "GBA/Named_Boxarts", &vec!["a.png", "b.png"], Some("\"e1\"")).unwrap();
    let got = cache.get_json::<Vec<String>>("libretro-index", "GBA/Named_Boxarts").unwrap().unwrap();
    assert_eq!(got.value, ["a.png", "b.png"]);
    assert_eq!(got.etag.as_deref(), Some("\"e1\""));
    assert!(got.fetched_at > 1_700_000_000);
    assert!(cache.get_json::<Vec<String>>("libretro-index", "other").unwrap().is_none());
}

#[test]
fn dat_rows_are_replaced_and_found_by_crc_and_stem() {
    let cache = Cache::in_memory().unwrap();
    let row = |game: &str, rom: &str, crc: &str| DatRow {
        game: game.into(), rom_name: rom.into(), size: Some(9),
        crc32: Some(crc.into()), md5: None, sha1: None,
    };
    cache.replace_dat("mame", &[row("Old", "old.zip", "00000000")]).unwrap();
    cache.replace_dat("mame", &[row("10-Yard Fight (World, set 1)", "10yard.zip", "8F401426")]).unwrap();
    assert!(cache.dat_by_crc("mame", "00000000").unwrap().is_empty());
    assert_eq!(cache.dat_by_crc("mame", "8f401426").unwrap()[0].game, "10-Yard Fight (World, set 1)");
    assert_eq!(cache.dat_by_rom_stem("mame", "10YARD").unwrap().len(), 1);
}

#[test]
fn data_persists_across_reopen() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("nested").join(CACHE_FILE);
    Cache::open(&path).unwrap().put_art_file("https://x/a.png", "ABC", 12).unwrap();
    assert_eq!(Cache::open(&path).unwrap().art_file("https://x/a.png").unwrap(), Some(("ABC".into(), 12)));
}

#[test]
fn corrupt_database_is_set_aside_and_recreated() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(CACHE_FILE);
    fs::write(&path, b"this is not a sqlite database at all, just garbage bytes").unwrap();
    let cache = Cache::open(&path).unwrap();
    cache.put_json("ns", "k", &1u8, None).unwrap();
    let names: Vec<String> = fs::read_dir(dir.path()).unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert!(names.iter().any(|n| n.starts_with("coverly.sqlite3.corrupt-")), "{names:?}");
}

#[test]
fn newer_schema_is_set_aside_too() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(CACHE_FILE);
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.pragma_update(None, "user_version", 99).unwrap();
    drop(conn);
    Cache::open(&path).unwrap().put_json("ns", "k", &1u8, None).unwrap();
}

#[test]
fn cache_is_shareable_between_threads() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Cache>();
}

#[test]
fn app_dirs_honour_overrides() {
    let dirs = AppDirs::resolve(Some(Path::new("/c")), Some(Path::new("/d"))).unwrap();
    assert_eq!((dirs.cache.as_path(), dirs.data.as_path()), (Path::new("/c"), Path::new("/d")));
}
```

(`rusqlite` is a regular dependency, so the test may use it.)

- [ ] **Step 3: Run to see it fail**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && cargo test -p coverly-core --test cache`
Expected: FAIL to compile.

- [ ] **Step 4: Implement**

`time.rs`: move `rfc3339_utc` and `civil_from_days` (and their unit tests) out of `audit/mod.rs` unchanged, make `rfc3339_utc` `pub(crate)`, and add:

```rust
/// `20261016T030000Z`: sortable, file-name safe.
pub(crate) fn compact_utc(time: SystemTime) -> String {
    rfc3339_utc(time).replace(['-', ':'], "")
}

/// Seconds since the Unix epoch, now.
pub fn now_unix() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}
```

(`rfc3339_utc` yields `2026-10-16T03:00:00Z` → `20261016T030000Z`.) `audit/mod.rs` now calls `crate::time::rfc3339_utc`.

`paths.rs` (append):

```rust
/// Where the app keeps its cache (indexes, DATs, downloaded art) and its
/// data (job journals, backups).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppDirs {
    pub cache: std::path::PathBuf,
    pub data: std::path::PathBuf,
}

impl AppDirs {
    /// The platform dirs for `(QUALIFIER, ORG, APP)`, each replaced by its
    /// override when given. `None` only when no home dir is known and an
    /// override is missing.
    pub fn resolve(cache: Option<&std::path::Path>, data: Option<&std::path::Path>) -> Option<AppDirs> {
        let project = directories::ProjectDirs::from(QUALIFIER, ORG, APP);
        let cache = match cache {
            Some(p) => p.to_path_buf(),
            None => project.as_ref()?.cache_dir().to_path_buf(),
        };
        let data = match data {
            Some(p) => p.to_path_buf(),
            None => project.as_ref()?.data_dir().to_path_buf(),
        };
        Some(AppDirs { cache, data })
    }
}
```

`cache/mod.rs`:

```rust
//! Local SQLite cache (spec §6 `cache/`, §16.1): quick-id memo
//! (`path + size + mtime → hashes`), source indexes and gathered candidates
//! as JSON values with ETag and fetch time, DAT rows, downloaded art files.
//! It is only a cache: a corrupt or newer database is set aside and rebuilt.

use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};

use crate::identify::hash::HashOutcome;
pub use crate::time::now_unix;

pub const SCHEMA_VERSION: i32 = 1;
pub const CACHE_FILE: &str = "coverly.sqlite3";

const SCHEMA_V1: &str = "
CREATE TABLE quick_ids (path TEXT PRIMARY KEY, size INTEGER NOT NULL, mtime_ns INTEGER NOT NULL, outcome TEXT NOT NULL);
CREATE TABLE kv (ns TEXT NOT NULL, key TEXT NOT NULL, value TEXT NOT NULL, etag TEXT, fetched_at INTEGER NOT NULL, PRIMARY KEY (ns, key));
CREATE TABLE dat_roms (dat TEXT NOT NULL, game TEXT NOT NULL, rom_name TEXT NOT NULL, rom_stem TEXT NOT NULL,
                       size INTEGER, crc32 TEXT, md5 TEXT, sha1 TEXT);
CREATE INDEX dat_roms_crc ON dat_roms (dat, crc32);
CREATE INDEX dat_roms_stem ON dat_roms (dat, rom_stem);
CREATE TABLE art_files (url TEXT PRIMARY KEY, sha1 TEXT NOT NULL, len INTEGER NOT NULL, fetched_at INTEGER NOT NULL);
";

#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("cache database: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("cache data: {0}")]
    Json(#[from] serde_json::Error),
    #[error("cache folder: {0}")]
    Io(#[from] std::io::Error),
}

pub struct Stored<T> {
    pub value: T,
    pub fetched_at: i64,
    pub etag: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatRow {
    pub game: String,
    pub rom_name: String,
    pub size: Option<u64>,
    pub crc32: Option<String>,
    pub md5: Option<String>,
    pub sha1: Option<String>,
}

pub struct Cache {
    conn: Mutex<Connection>,
}

impl Cache {
    pub fn open(path: &Path) -> Result<Cache, CacheError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        match Connection::open(path).map_err(CacheError::from).and_then(prepare) {
            Ok(conn) => Ok(Cache { conn: Mutex::new(conn) }),
            // Another process (app + CLI) holds the lock: the database is fine, do not move it.
            Err(CacheError::Sql(rusqlite::Error::SqliteFailure(e, msg)))
                if matches!(e.code, rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked) =>
            {
                Err(CacheError::Sql(rusqlite::Error::SqliteFailure(e, msg)))
            }
            Err(_) => {
                let aside = path.with_file_name(format!(
                    "{}.corrupt-{}",
                    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                    now_unix()
                ));
                std::fs::rename(path, &aside)?;
                for suffix in ["-wal", "-shm"] {
                    let side = path.with_file_name(format!("{}{suffix}", path.file_name().unwrap().to_string_lossy()));
                    let _ = std::fs::remove_file(side);
                }
                Ok(Cache { conn: Mutex::new(prepare(Connection::open(path)?)?) })
            }
        }
    }

    pub fn in_memory() -> Result<Cache, CacheError> {
        Ok(Cache { conn: Mutex::new(prepare(Connection::open_in_memory()?)?) })
    }

    fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn quick_id(&self, path: &Path, size: u64, mtime_ns: i64) -> Result<Option<HashOutcome>, CacheError> {
        let row: Option<String> = self.conn().query_row(
            "SELECT outcome FROM quick_ids WHERE path = ?1 AND size = ?2 AND mtime_ns = ?3",
            params![path.to_string_lossy(), size as i64, mtime_ns],
            |r| r.get(0),
        ).optional()?;
        Ok(row.map(|json| serde_json::from_str(&json)).transpose()?)
    }

    pub fn put_quick_id(&self, path: &Path, size: u64, mtime_ns: i64, outcome: &HashOutcome) -> Result<(), CacheError> {
        self.conn().execute(
            "INSERT OR REPLACE INTO quick_ids (path, size, mtime_ns, outcome) VALUES (?1, ?2, ?3, ?4)",
            params![path.to_string_lossy(), size as i64, mtime_ns, serde_json::to_string(outcome)?],
        )?;
        Ok(())
    }

    pub fn get_json<T: DeserializeOwned>(&self, ns: &str, key: &str) -> Result<Option<Stored<T>>, CacheError> {
        let row: Option<(String, Option<String>, i64)> = self.conn().query_row(
            "SELECT value, etag, fetched_at FROM kv WHERE ns = ?1 AND key = ?2",
            params![ns, key],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        ).optional()?;
        match row {
            None => Ok(None),
            // A value that no longer deserialises (older format) is a miss.
            Some((json, etag, fetched_at)) => Ok(serde_json::from_str(&json).ok().map(|value| Stored { value, fetched_at, etag })),
        }
    }

    pub fn put_json<T: Serialize>(&self, ns: &str, key: &str, value: &T, etag: Option<&str>) -> Result<(), CacheError> {
        self.conn().execute(
            "INSERT OR REPLACE INTO kv (ns, key, value, etag, fetched_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![ns, key, serde_json::to_string(value)?, etag, now_unix()],
        )?;
        Ok(())
    }

    pub fn touch_json(&self, ns: &str, key: &str) -> Result<(), CacheError> {
        self.conn().execute("UPDATE kv SET fetched_at = ?3 WHERE ns = ?1 AND key = ?2", params![ns, key, now_unix()])?;
        Ok(())
    }

    pub fn replace_dat(&self, dat: &str, rows: &[DatRow]) -> Result<(), CacheError> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM dat_roms WHERE dat = ?1", params![dat])?;
        {
            let mut insert = tx.prepare(
                "INSERT INTO dat_roms (dat, game, rom_name, rom_stem, size, crc32, md5, sha1) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )?;
            for r in rows {
                let stem = r.rom_name.rsplit_once('.').map(|(s, _)| s).unwrap_or(&r.rom_name).to_lowercase();
                insert.execute(params![dat, r.game, r.rom_name, stem, r.size.map(|s| s as i64),
                    r.crc32.as_deref().map(str::to_uppercase), r.md5, r.sha1])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn dat_by_crc(&self, dat: &str, crc32: &str) -> Result<Vec<DatRow>, CacheError> {
        self.dat_query("crc32 = ?2", dat, &crc32.to_uppercase())
    }

    pub fn dat_by_rom_stem(&self, dat: &str, stem: &str) -> Result<Vec<DatRow>, CacheError> {
        self.dat_query("rom_stem = ?2", dat, &stem.to_lowercase())
    }

    fn dat_query(&self, filter: &str, dat: &str, value: &str) -> Result<Vec<DatRow>, CacheError> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT game, rom_name, size, crc32, md5, sha1 FROM dat_roms WHERE dat = ?1 AND {filter} ORDER BY game, rom_name"
        ))?;
        let rows = stmt.query_map(params![dat, value], |r| Ok(DatRow {
            game: r.get(0)?, rom_name: r.get(1)?, size: r.get::<_, Option<i64>>(2)?.map(|s| s as u64),
            crc32: r.get(3)?, md5: r.get(4)?, sha1: r.get(5)?,
        }))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn put_art_file(&self, url: &str, sha1: &str, len: u64) -> Result<(), CacheError> {
        self.conn().execute(
            "INSERT OR REPLACE INTO art_files (url, sha1, len, fetched_at) VALUES (?1, ?2, ?3, ?4)",
            params![url, sha1, len as i64, now_unix()],
        )?;
        Ok(())
    }

    pub fn art_file(&self, url: &str) -> Result<Option<(String, u64)>, CacheError> {
        Ok(self.conn().query_row("SELECT sha1, len FROM art_files WHERE url = ?1", params![url],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64))).optional()?)
    }
}

/// Applies pragmas and migrations; fails on a database it cannot use.
fn prepare(conn: Connection) -> Result<Connection, CacheError> {
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    let version: i32 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    match version {
        0 => {
            conn.execute_batch(SCHEMA_V1)?;
            conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        }
        SCHEMA_VERSION => {}
        _ => return Err(CacheError::Sql(rusqlite::Error::InvalidQuery)),
    }
    // Touch a table so a garbage file fails here, not on first use.
    conn.query_row("SELECT count(*) FROM kv", [], |r| r.get::<_, i64>(0))?;
    Ok(conn)
}

pub fn mtime_ns(t: SystemTime) -> i64 {
    t.duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as i64).unwrap_or(0)
}
```

(Journal mode stays the default `DELETE`: WAL side files on an exFAT card or network share are a risk, and the cache lives in the app cache dir anyway.)

- [ ] **Step 5: Run to see it pass**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && cargo test -p coverly-core --test cache && cargo test -p coverly-core --lib`
Expected: PASS (audit tests still pass after moving `rfc3339_utc`). Global checks.

- [ ] **Step 6: Commit**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && git add Cargo.toml Cargo.lock crates/coverly-core && git commit -m "feat(core): add SQLite cache for quick ids, indexes and DAT rows"
```

---

### Task 4: HTTP client, global limiter, retries and mock server

**Files:**
- Modify: `Cargo.toml`, `crates/coverly-core/Cargo.toml` (deps, `[features] test-util = []`, self dev-dep), `crates/coverly-core/src/lib.rs`
- Create: `crates/coverly-core/src/net/mod.rs`, `crates/coverly-core/src/net/limiter.rs`, `crates/coverly-core/src/net/testing.rs`
- Test: `crates/coverly-core/tests/net.rs`

**Interfaces:**
- Produces:

```rust
// coverly_core::net
pub const USER_AGENT: &str = concat!("CoverlyROMs/", env!("CARGO_PKG_VERSION"));
pub const DEFAULT_MAX_BYTES: u64 = 32 * 1024 * 1024;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpRequest { pub url: String, pub if_none_match: Option<String>, pub max_bytes: u64 }
impl HttpRequest { pub fn get(url: impl Into<String>) -> Self; pub fn max_bytes(self, n: u64) -> Self;
                   pub fn if_none_match(self, etag: Option<String>) -> Self; }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpResponse { pub status: u16, pub etag: Option<String>, pub last_modified: Option<String>,
                          pub content_type: Option<String>, pub retry_after: Option<Duration>, pub body: Vec<u8> }
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NetError {
    #[error("offline or unreachable: {0}")] Offline(String),
    #[error("timed out")] Timeout,
    #[error("rate limited by {host} (HTTP {status})")] RateLimited { host: String, status: u16 },
    #[error("HTTP {status} for {url}")] Status { status: u16, url: String },
    #[error("response larger than {limit} bytes")] TooLarge { limit: u64 },
    #[error("refused non-HTTPS URL {0}")] Insecure(String),
}
pub trait Http: Send + Sync { fn get(&self, req: &HttpRequest) -> Result<HttpResponse, NetError>; }
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProxyMode { Env, Off, Manual(String) }
#[derive(Clone, Debug)]
pub struct NetConfig { pub concurrency: usize, pub min_interval: Duration, pub timeout: Duration,
                       pub retries: u32, pub backoff_base: Duration, pub proxy: ProxyMode }
impl Default for NetConfig  // 4, 150 ms, 30 s, 3, 500 ms, Env
pub struct UreqHttp;              impl UreqHttp { pub fn new(cfg: &NetConfig) -> UreqHttp }   impl Http
pub struct PoliteHttp<H: Http>;   impl<H: Http> PoliteHttp<H> { pub fn new(inner: H, cfg: &NetConfig) -> Self;
                                      pub fn with_limiter(inner: H, limiter: Arc<Limiter>, cfg: &NetConfig) -> Self }  impl Http
pub fn proxy_from(lookup: impl Fn(&str) -> Option<String>) -> Option<ureq::Proxy>;
pub fn host_of(url: &str) -> String;
// coverly_core::net::limiter (re-exported as net::Limiter)
pub struct Limiter;  impl Limiter { pub fn new(concurrency: usize, min_interval: Duration) -> Arc<Limiter>;
                                    pub fn acquire(&self) -> Permit<'_> }   // Permit releases on drop
// coverly_core::net::testing  (cfg(any(test, feature = "test-util")))
pub struct MockServer;  pub struct MockResponse { pub status: u16, pub headers: Vec<(String, String)>, pub body: Vec<u8> }
pub struct RecordedRequest { pub path: String, pub headers: Vec<(String, String)> }
impl RecordedRequest { pub fn header(&self, name: &str) -> Option<&str> }
impl MockResponse { pub fn ok(body: impl Into<Vec<u8>>) -> Self; pub fn status(code: u16) -> Self;
                    pub fn header(self, k: &str, v: &str) -> Self }
impl MockServer { pub fn start() -> MockServer; pub fn base(&self) -> String; pub fn url(&self, path: &str) -> String;
                  pub fn mock(&self, path: &str, resp: MockResponse);           // repeats forever
                  pub fn mock_seq(&self, path: &str, seq: Vec<MockResponse>);   // in order, last repeats
                  pub fn requests(&self) -> Vec<RecordedRequest>; pub fn hits(&self, path: &str) -> usize }
```

- [ ] **Step 1: Dependencies and feature**

Workspace: `ureq = "3.4"`, `percent-encoding = "2.3"`. Core `[dependencies]`: `ureq.workspace = true`, `percent-encoding.workspace = true`. Core:

```toml
[features]
# Mock HTTP server and fixtures for tests of this crate and of the CLI.
test-util = []

[dev-dependencies]
coverly-core = { path = ".", features = ["test-util"] }
```

(If cargo rejects the self dev-dependency, instead add `required-features = ["test-util"]` `[[test]]` entries for the network tests and run them with `--features test-util` in CI; record the deviation.) `lib.rs`: `pub mod net;`.

- [ ] **Step 2: Write the failing tests**

`crates/coverly-core/tests/net.rs`:

```rust
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use coverly_core::net::testing::{MockResponse, MockServer};
use coverly_core::net::{proxy_from, Http, HttpRequest, Limiter, NetConfig, NetError, PoliteHttp, UreqHttp, USER_AGENT};

fn cfg() -> NetConfig {
    NetConfig { retries: 2, backoff_base: Duration::from_millis(1), min_interval: Duration::ZERO,
                timeout: Duration::from_secs(5), ..NetConfig::default() }
}

fn client() -> PoliteHttp<UreqHttp> {
    let c = cfg();
    PoliteHttp::new(UreqHttp::new(&c), &c)
}

#[test]
fn sends_the_user_agent_and_returns_body_and_etag() {
    let server = MockServer::start();
    server.mock("/a", MockResponse::ok("hello").header("ETag", "\"x1\""));
    let resp = client().get(&HttpRequest::get(server.url("/a"))).unwrap();
    assert_eq!((resp.status, resp.body.as_slice(), resp.etag.as_deref()), (200, b"hello".as_slice(), Some("\"x1\"")));
    assert!(USER_AGENT.starts_with("CoverlyROMs/"));
    assert_eq!(server.requests()[0].header("user-agent"), Some(USER_AGENT));
}

#[test]
fn conditional_get_sends_if_none_match_and_returns_304() {
    let server = MockServer::start();
    server.mock("/i", MockResponse::status(304));
    let resp = client().get(&HttpRequest::get(server.url("/i")).if_none_match(Some("\"x1\"".into()))).unwrap();
    assert_eq!(resp.status, 304);
    assert_eq!(server.requests()[0].header("if-none-match"), Some("\"x1\""));
}

#[test]
fn retries_server_errors_then_succeeds() {
    let server = MockServer::start();
    server.mock_seq("/r", vec![MockResponse::status(503), MockResponse::status(500), MockResponse::ok("fine")]);
    assert_eq!(client().get(&HttpRequest::get(server.url("/r"))).unwrap().body, b"fine");
    assert_eq!(server.hits("/r"), 3);
}

#[test]
fn persistent_429_becomes_rate_limited() {
    let server = MockServer::start();
    server.mock("/l", MockResponse::status(429).header("Retry-After", "0"));
    let err = client().get(&HttpRequest::get(server.url("/l"))).unwrap_err();
    assert!(matches!(err, NetError::RateLimited { status: 429, .. }), "{err}");
    assert_eq!(server.hits("/l"), 3);
}

#[test]
fn forbidden_is_rate_limited_without_retry() {
    let server = MockServer::start();
    server.mock("/f", MockResponse::status(403));
    assert!(matches!(client().get(&HttpRequest::get(server.url("/f"))), Err(NetError::RateLimited { status: 403, .. })));
    assert_eq!(server.hits("/f"), 1);
}

#[test]
fn not_found_is_a_response() {
    let server = MockServer::start();
    assert_eq!(client().get(&HttpRequest::get(server.url("/missing"))).unwrap().status, 404);
}

#[test]
fn bodies_over_the_limit_are_refused() {
    let server = MockServer::start();
    server.mock("/big", MockResponse::ok(vec![0u8; 4096]));
    let err = client().get(&HttpRequest::get(server.url("/big")).max_bytes(1024)).unwrap_err();
    assert_eq!(err, NetError::TooLarge { limit: 1024 });
}

#[test]
fn plain_http_to_a_remote_host_is_refused_before_connecting() {
    let err = UreqHttp::new(&cfg()).get(&HttpRequest::get("http://example.com/x")).unwrap_err();
    assert!(matches!(err, NetError::Insecure(_)));
    let err = UreqHttp::new(&cfg()).get(&HttpRequest::get("http://127.0.0.1@example.com/x")).unwrap_err();
    assert!(matches!(err, NetError::Insecure(_)), "userinfo tricks are refused");
}

#[test]
fn a_closed_port_is_offline() {
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let err = client().get(&HttpRequest::get(format!("http://127.0.0.1:{port}/x"))).unwrap_err();
    assert!(matches!(err, NetError::Offline(_)), "{err}");
}

#[test]
fn limiter_caps_concurrency() {
    let limiter = Limiter::new(2, Duration::ZERO);
    let active = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    std::thread::scope(|s| {
        for _ in 0..6 {
            let (limiter, active, peak) = (limiter.clone(), active.clone(), peak.clone());
            s.spawn(move || {
                let _permit = limiter.acquire();
                let now = active.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(now, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(20));
                active.fetch_sub(1, Ordering::SeqCst);
            });
        }
    });
    assert!(peak.load(Ordering::SeqCst) <= 2);
}

#[test]
fn limiter_spaces_request_starts() {
    let limiter = Limiter::new(4, Duration::from_millis(50));
    let start = Instant::now();
    for _ in 0..3 {
        drop(limiter.acquire());
    }
    assert!(start.elapsed() >= Duration::from_millis(100));
}

#[test]
fn concurrency_is_clamped_to_one_to_six() {
    assert_eq!(Limiter::new(0, Duration::ZERO).capacity(), 1);
    assert_eq!(Limiter::new(50, Duration::ZERO).capacity(), 6);
}

#[test]
fn proxy_from_environment_values() {
    assert!(proxy_from(|n| (n == "HTTPS_PROXY").then(|| "http://proxy:8080".to_string())).is_some());
    assert!(proxy_from(|_| Some("socks5://proxy:1080".to_string())).is_none());
    assert!(proxy_from(|_| Some("  ".to_string())).is_none());
}
```

(Add `pub fn capacity(&self) -> usize` to `Limiter` for the clamp test.)

- [ ] **Step 3: Run to see it fail**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && cargo test -p coverly-core --test net`
Expected: FAIL to compile.

- [ ] **Step 4: Implement `net/limiter.rs`**

```rust
//! One limiter shared by every request of a job (spec §16.5): at most
//! `concurrency` requests in flight (1–6) and at least `min_interval`
//! between two request starts.

use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

pub struct Limiter {
    state: Mutex<State>,
    freed: Condvar,
    capacity: usize,
    min_interval: Duration,
}

struct State {
    in_flight: usize,
    next_start: Instant,
}

pub struct Permit<'a> {
    limiter: &'a Limiter,
}

impl Limiter {
    pub fn new(concurrency: usize, min_interval: Duration) -> Arc<Limiter> {
        Arc::new(Limiter {
            state: Mutex::new(State { in_flight: 0, next_start: Instant::now() }),
            freed: Condvar::new(),
            capacity: concurrency.clamp(1, 6),
            min_interval,
        })
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn acquire(&self) -> Permit<'_> {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        while state.in_flight >= self.capacity {
            state = self.freed.wait(state).unwrap_or_else(|p| p.into_inner());
        }
        state.in_flight += 1;
        let now = Instant::now();
        let start = state.next_start.max(now);
        state.next_start = start + self.min_interval;
        drop(state);
        if start > now {
            std::thread::sleep(start - now);
        }
        Permit { limiter: self }
    }
}

impl Drop for Permit<'_> {
    fn drop(&mut self) {
        let mut state = self.limiter.state.lock().unwrap_or_else(|p| p.into_inner());
        state.in_flight -= 1;
        self.limiter.freed.notify_one();
    }
}
```

- [ ] **Step 5: Implement `net/mod.rs`**

```rust
//! HTTP for art sources and DATs (spec §6 `net/`, §9 Network, §10, §11):
//! blocking ureq 3 with rustls (as Pluryprint), HTTPS only (loopback HTTP
//! for tests), proxy from the environment, `CoverlyROMs/<ver>` User-Agent,
//! a global limiter, retries with exponential backoff and Retry-After, and
//! body size caps. Error statuses come back as responses; only transport
//! failures, exhausted retries and rate limits are errors.

mod limiter;
#[cfg(any(test, feature = "test-util"))]
pub mod testing;

use std::sync::Arc;
use std::time::Duration;

pub use limiter::{Limiter, Permit};

pub const USER_AGENT: &str = concat!("CoverlyROMs/", env!("CARGO_PKG_VERSION"));
pub const DEFAULT_MAX_BYTES: u64 = 32 * 1024 * 1024;
const PROXY_ENV: [&str; 4] = ["HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy"];
const MAX_RETRY_AFTER: Duration = Duration::from_secs(60);
const MAX_BACKOFF: Duration = Duration::from_secs(30);

// HttpRequest / HttpResponse / NetError / Http / ProxyMode / NetConfig exactly as in Interfaces.

impl HttpRequest {
    pub fn get(url: impl Into<String>) -> HttpRequest {
        HttpRequest { url: url.into(), if_none_match: None, max_bytes: DEFAULT_MAX_BYTES }
    }
    pub fn max_bytes(mut self, n: u64) -> HttpRequest { self.max_bytes = n; self }
    pub fn if_none_match(mut self, etag: Option<String>) -> HttpRequest { self.if_none_match = etag; self }
}

impl Default for NetConfig {
    fn default() -> NetConfig {
        NetConfig { concurrency: 4, min_interval: Duration::from_millis(150), timeout: Duration::from_secs(30),
                    retries: 3, backoff_base: Duration::from_millis(500), proxy: ProxyMode::Env }
    }
}

/// Same rule as Pluryprint's updater: first parseable HTTP(S) CONNECT proxy.
pub fn proxy_from(lookup: impl Fn(&str) -> Option<String>) -> Option<ureq::Proxy> {
    PROXY_ENV.iter().find_map(|name| {
        let value = lookup(name)?;
        let value = value.trim();
        if value.is_empty() {
            return None;
        }
        let proxy = ureq::Proxy::new(value).ok()?;
        matches!(proxy.protocol(), ureq::ProxyProtocol::Http | ureq::ProxyProtocol::Https).then_some(proxy)
    })
}

enum UrlKind { Https, Loopback, Refused }

/// The authority's host, lower-case; empty when the URL has none.
pub fn host_of(url: &str) -> String {
    let rest = url.split_once("://").map(|(_, r)| r).unwrap_or("");
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = if authority.starts_with('[') {
        authority.split(']').next().map(|h| format!("{h}]")).unwrap_or_default()
    } else {
        authority.split(':').next().unwrap_or("").to_string()
    };
    host.to_lowercase()
}

fn url_kind(url: &str) -> UrlKind {
    let lower = url.to_ascii_lowercase();
    let authority = lower.split_once("://").map(|(_, r)| r.split(['/', '?', '#']).next().unwrap_or("")).unwrap_or("");
    if authority.contains('@') || authority.is_empty() {
        return UrlKind::Refused;
    }
    if lower.starts_with("https://") {
        return UrlKind::Https;
    }
    if lower.starts_with("http://") && matches!(host_of(url).as_str(), "127.0.0.1" | "localhost" | "[::1]") {
        return UrlKind::Loopback;
    }
    UrlKind::Refused
}

pub struct UreqHttp {
    secure: ureq::Agent,
    loopback: ureq::Agent,
}

impl UreqHttp {
    pub fn new(cfg: &NetConfig) -> UreqHttp {
        let proxy = match &cfg.proxy {
            ProxyMode::Env => proxy_from(|n| std::env::var(n).ok()),
            ProxyMode::Off => None,
            ProxyMode::Manual(url) => ureq::Proxy::new(url).ok(),
        };
        let build = |https_only: bool, proxy: Option<ureq::Proxy>| -> ureq::Agent {
            ureq::Agent::config_builder()
                .timeout_global(Some(cfg.timeout))
                .http_status_as_error(false)
                .https_only(https_only)
                .max_redirects(5)
                .user_agent(USER_AGENT)
                .proxy(proxy)
                .build()
                .into()
        };
        UreqHttp { secure: build(true, proxy), loopback: build(false, None) }
    }
}

impl Http for UreqHttp {
    fn get(&self, req: &HttpRequest) -> Result<HttpResponse, NetError> {
        let agent = match url_kind(&req.url) {
            UrlKind::Https => &self.secure,
            UrlKind::Loopback => &self.loopback,
            UrlKind::Refused => return Err(NetError::Insecure(req.url.clone())),
        };
        let mut builder = agent.get(&req.url);
        if let Some(tag) = &req.if_none_match {
            builder = builder.header("If-None-Match", tag);
        }
        let mut resp = builder.call().map_err(transport)?;
        let status = resp.status().as_u16();
        let header = |name: &str| resp.headers().get(name).and_then(|v| v.to_str().ok()).map(str::to_string);
        let (etag, last_modified, content_type) = (header("etag"), header("last-modified"), header("content-type"));
        let retry_after = header("retry-after").and_then(|v| v.trim().parse::<u64>().ok()).map(Duration::from_secs);
        let body = if status == 304 {
            Vec::new()
        } else {
            resp.body_mut().with_config().limit(req.max_bytes).read_to_vec().map_err(|e| match e {
                ureq::Error::BodyExceedsLimit(_) => NetError::TooLarge { limit: req.max_bytes },
                other => transport(other),
            })?
        };
        Ok(HttpResponse { status, etag, last_modified, content_type, retry_after, body })
    }
}

fn transport(e: ureq::Error) -> NetError {
    match e {
        ureq::Error::Timeout(_) => NetError::Timeout,
        other => NetError::Offline(other.to_string()),
    }
}

pub struct PoliteHttp<H: Http> {
    inner: H,
    limiter: Arc<Limiter>,
    retries: u32,
    backoff_base: Duration,
}

impl<H: Http> PoliteHttp<H> {
    pub fn new(inner: H, cfg: &NetConfig) -> Self {
        Self::with_limiter(inner, Limiter::new(cfg.concurrency, cfg.min_interval), cfg)
    }
    pub fn with_limiter(inner: H, limiter: Arc<Limiter>, cfg: &NetConfig) -> Self {
        PoliteHttp { inner, limiter, retries: cfg.retries, backoff_base: cfg.backoff_base }
    }
    fn backoff(&self, attempt: u32, retry_after: Option<Duration>) -> Duration {
        retry_after.map(|d| d.min(MAX_RETRY_AFTER))
            .unwrap_or_else(|| (self.backoff_base * 2u32.saturating_pow(attempt)).min(MAX_BACKOFF))
    }
}

impl<H: Http> Http for PoliteHttp<H> {
    fn get(&self, req: &HttpRequest) -> Result<HttpResponse, NetError> {
        let mut attempt = 0;
        loop {
            let result = {
                let _permit = self.limiter.acquire();
                self.inner.get(req)
            };
            let wait = match &result {
                Ok(r) if r.status == 403 => {
                    return Err(NetError::RateLimited { host: host_of(&req.url), status: 403 });
                }
                Ok(r) if r.status == 429 || r.status >= 500 => r.retry_after,
                Err(NetError::Offline(_) | NetError::Timeout) => None,
                _ => return result,
            };
            if attempt >= self.retries {
                return match result {
                    Ok(r) if r.status == 429 => Err(NetError::RateLimited { host: host_of(&req.url), status: 429 }),
                    Ok(r) => Err(NetError::Status { status: r.status, url: req.url.clone() }),
                    Err(e) => Err(e),
                };
            }
            std::thread::sleep(self.backoff(attempt, wait));
            attempt += 1;
        }
    }
}
```

- [ ] **Step 6: Implement `net/testing.rs` (MockServer)**

```rust
//! A tiny HTTP/1.1 server on 127.0.0.1 for tests (feature `test-util`):
//! canned responses per request path (the raw, percent-encoded target),
//! `Connection: close`, request log. Unknown paths answer 404.

use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub struct MockResponse { pub status: u16, pub headers: Vec<(String, String)>, pub body: Vec<u8> }

impl MockResponse {
    pub fn ok(body: impl Into<Vec<u8>>) -> MockResponse { MockResponse { status: 200, headers: Vec::new(), body: body.into() } }
    pub fn status(code: u16) -> MockResponse { MockResponse { status: code, headers: Vec::new(), body: Vec::new() } }
    pub fn header(mut self, k: &str, v: &str) -> MockResponse { self.headers.push((k.into(), v.into())); self }
}

#[derive(Clone, Debug)]
pub struct RecordedRequest { pub path: String, pub headers: Vec<(String, String)> }

impl RecordedRequest {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

type Routes = Arc<Mutex<HashMap<String, VecDeque<MockResponse>>>>;

pub struct MockServer {
    addr: SocketAddr,
    routes: Routes,
    log: Arc<Mutex<Vec<RecordedRequest>>>,
    stop: Arc<AtomicBool>,
}

impl MockServer {
    pub fn start() -> MockServer {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        let addr = listener.local_addr().unwrap();
        let routes: Routes = Arc::default();
        let log: Arc<Mutex<Vec<RecordedRequest>>> = Arc::default();
        let stop = Arc::new(AtomicBool::new(false));
        let (r, l, s) = (routes.clone(), log.clone(), stop.clone());
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                if s.load(Ordering::SeqCst) { break; }
                let Ok(stream) = stream else { continue };
                let (r, l) = (r.clone(), l.clone());
                std::thread::spawn(move || { let _ = serve(stream, &r, &l); });
            }
        });
        MockServer { addr, routes, log, stop }
    }
    pub fn base(&self) -> String { format!("http://{}", self.addr) }
    pub fn url(&self, path: &str) -> String { format!("{}{}", self.base(), path) }
    pub fn mock(&self, path: &str, resp: MockResponse) { self.mock_seq(path, vec![resp]); }
    pub fn mock_seq(&self, path: &str, seq: Vec<MockResponse>) {
        self.routes.lock().unwrap().insert(path.to_string(), seq.into());
    }
    pub fn requests(&self) -> Vec<RecordedRequest> { self.log.lock().unwrap().clone() }
    pub fn hits(&self, path: &str) -> usize { self.requests().iter().filter(|r| r.path == path).count() }
}

impl Drop for MockServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.addr); // unblock accept
    }
}

fn serve(stream: TcpStream, routes: &Routes, log: &Arc<Mutex<Vec<RecordedRequest>>>) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
    let mut headers = Vec::new();
    loop {
        let mut h = String::new();
        if reader.read_line(&mut h)? == 0 || h == "\r\n" || headers.len() > 100 { break; }
        if let Some((k, v)) = h.trim_end().split_once(':') { headers.push((k.trim().to_string(), v.trim().to_string())); }
    }
    log.lock().unwrap().push(RecordedRequest { path: path.clone(), headers });
    let resp = {
        let mut map = routes.lock().unwrap();
        match map.get_mut(&path) {
            Some(q) if q.len() > 1 => q.pop_front().unwrap(),
            Some(q) => q.front().cloned().unwrap_or_else(|| MockResponse::status(404)),
            None => MockResponse::status(404),
        }
    };
    let mut out = stream;
    write!(out, "HTTP/1.1 {} X\r\nContent-Length: {}\r\nConnection: close\r\n", resp.status, resp.body.len())?;
    for (k, v) in &resp.headers { write!(out, "{k}: {v}\r\n")?; }
    out.write_all(b"\r\n")?;
    out.write_all(&resp.body)?;
    out.flush()
}
```

Task 6 adds libretro/DAT fixture helpers to this file.

- [ ] **Step 7: Run to see it pass**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && cargo test -p coverly-core --test net`
Expected: PASS. Global checks (clippy may ask for `#[must_use]` on `Permit` — add it).

- [ ] **Step 8: Commit**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && git add Cargo.toml Cargo.lock crates/coverly-core && git commit -m "feat(core): add polite HTTPS client with global limiter, retries and mock server"
```

---
### Task 5: libretro-database DAT index and arcade short-name map

**Files:**
- Create: `crates/coverly-core/src/identify/dat.rs`
- Modify: `crates/coverly-core/src/identify/mod.rs` (`pub mod dat;`), `crates/coverly-core/src/systems/{mod.rs,systems.toml}` (`dats` field), `crates/coverly-core/src/net/testing.rs` (`serve_dat`)
- Test: `crates/coverly-core/tests/dat.rs`

**Interfaces:**
- Consumes: `Cache`, `DatRow` (Task 3); `Http`, `HttpRequest`, `NetError` (Task 4); `RomHashes`, `HashOutcome` (Task 1); `System` (M1).
- Produces:

```rust
// coverly_core::identify::dat
pub const DATABASE_BASE: &str = "https://raw.githubusercontent.com/libretro/libretro-database/master";
pub const DAT_TTL_SECS: i64 = 30 * 24 * 3600;
pub const MAX_DAT_BYTES: u64 = 64 * 1024 * 1024;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatGame { pub name: String, pub roms: Vec<DatRow> }        // DatRow from cache; DatRow.game == name
#[derive(Debug, thiserror::Error)]
pub enum DatError { #[error("DAT syntax error on line {line}: {message}")] Parse { line: usize, message: String },
                    #[error(transparent)] Net(#[from] NetError), #[error(transparent)] Cache(#[from] CacheError) }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DatStatus { Ready, Missing, Stale }   // Stale: refresh failed, older rows used
pub fn parse_clrmamepro(text: &str) -> Result<Vec<DatGame>, DatError>;
pub fn dat_sources(system: &System) -> Vec<String>;   // repo-relative paths, e.g. "metadat/no-intro/Nintendo - Game Boy Advance.dat"
pub fn encode_path(path: &str) -> String;             // each segment percent-encoded
pub struct DatStore<'a> { /* http, cache, base, ttl, memo */ }
impl<'a> DatStore<'a> {
    pub fn new(http: &'a dyn Http, cache: &'a Cache, base: impl Into<String>) -> DatStore<'a>;
    pub fn ensure(&self, dat: &str) -> Result<DatStatus, DatError>;               // once per process per DAT
    pub fn lookup(&self, dats: &[String], outcome: &HashOutcome) -> Result<Option<String>, DatError>; // game name
    pub fn arcade_description(&self, dats: &[String], short_name: &str) -> Result<Option<String>, DatError>;
}
// coverly_core::systems::System gains: pub dats: Vec<String>   (explicit `dats = [...]` in TOML; empty = derived)
// coverly_core::net::testing gains: impl MockServer { pub fn serve_dat(&self, repo_path: &str, text: &str) }
```

- [ ] **Step 1: Write the failing tests**

`crates/coverly-core/tests/dat.rs`:

```rust
use coverly_core::cache::Cache;
use coverly_core::identify::dat::{dat_sources, encode_path, parse_clrmamepro, DatStatus, DatStore};
use coverly_core::identify::hash::{Digests, HashOutcome, RomHashes};
use coverly_core::net::testing::{MockResponse, MockServer};
use coverly_core::net::{NetConfig, PoliteHttp, UreqHttp};
use coverly_core::systems::SystemDb;

const NES_DAT: &str = r#"clrmamepro (
	name "Nintendo - Nintendo Entertainment System"
	version "2026.10.07"
)

game (
	name "Golden Test (USA)"
	region "USA"
	rom ( name "Golden Test (USA).nes" size 25 crc 11111111 md5 AA sha1 BB )
)
game (
	name "Golden Test (USA)"
	rom ( name "Golden Test (USA).unh" size 9 crc CBF43926 md5 25F9E794323B453885F5181F1B624D0B sha1 F7C3BC1D808E04732ADF679965CCC34CA7AE3441 )
)
game (
	name "Quote \"Test\" (Japan)"
	rom ( name "q.nes" size 3 crc 22222222 )
)
"#;

const MAME_DAT: &str = "clrmamepro (\n\tname \"MAME\"\n\tversion 2017-02-14\n)\n\ngame (\n\tname \"10-Yard Fight (World, set 1)\"\n\tyear \"1983\"\n\trom ( name 10yard.zip size 62708 crc 8F401426 md5 040dc0827e184b8da9b91499f96c1fce sha1 8b15a2e54e9fdded3d61205bac6535ccc7b41271 )\n)\n";

fn http() -> PoliteHttp<UreqHttp> {
    let cfg = NetConfig { retries: 0, ..NetConfig::default() };
    PoliteHttp::new(UreqHttp::new(&cfg), &cfg)
}

fn digests(crc: &str, size: u64) -> Digests {
    Digests { crc32: crc.into(), md5: String::new(), sha1: String::new(), size }
}

#[test]
fn parses_quoted_unquoted_and_escaped_values() {
    let games = parse_clrmamepro(NES_DAT).unwrap();
    assert_eq!(games.len(), 3);
    assert_eq!(games[1].roms[0].crc32.as_deref(), Some("CBF43926"));
    assert_eq!(games[1].roms[0].size, Some(9));
    assert_eq!(games[2].name, "Quote \"Test\" (Japan)");
    let mame = parse_clrmamepro(MAME_DAT).unwrap();
    assert_eq!(mame[0].roms[0].rom_name, "10yard.zip");
}

#[test]
fn unbalanced_dat_is_an_error_with_a_line() {
    let err = parse_clrmamepro("game (\n name \"x\"\n rom ( name y ").unwrap_err();
    assert!(err.to_string().contains("line"), "{err}");
}

#[test]
fn dat_sources_per_system() {
    let db = SystemDb::builtin();
    assert_eq!(dat_sources(db.get("gba").unwrap()), ["metadat/no-intro/Nintendo - Game Boy Advance.dat"]);
    assert_eq!(dat_sources(db.get("psx").unwrap()), ["metadat/redump/Sony - PlayStation.dat"]);
    assert_eq!(dat_sources(db.get("mame").unwrap()), ["metadat/mame/MAME.dat"]);
    assert_eq!(dat_sources(db.get("fbneo").unwrap()), ["metadat/fbneo-split/FBNeo - Arcade Games.dat"]);
    let custom = db.with_overrides("[[system]]\nid = \"gba\"\ndats = [\"dat/Custom.dat\"]\n").unwrap();
    assert_eq!(dat_sources(custom.get("gba").unwrap()), ["dat/Custom.dat"]);
    assert_eq!(encode_path("metadat/no-intro/Nintendo - Game Boy Advance.dat"),
               "metadat/no-intro/Nintendo%20-%20Game%20Boy%20Advance.dat");
}

#[test]
fn downloads_once_then_looks_up_headerless_and_zip_crcs() {
    let server = MockServer::start();
    let path = "metadat/no-intro/Nintendo - Nintendo Entertainment System.dat";
    server.serve_dat(path, NES_DAT);
    let (http, cache) = (http(), Cache::in_memory().unwrap());
    let store = DatStore::new(&http, &cache, server.base());
    let dats = vec![path.to_string()];
    assert_eq!(store.ensure(path).unwrap(), DatStatus::Ready);
    assert_eq!(store.ensure(path).unwrap(), DatStatus::Ready);
    assert_eq!(server.hits(&format!("/{}", encode_path(path))), 1);

    let headered = HashOutcome::Content(RomHashes { full: digests("99999999", 25), headerless: Some(digests("CBF43926", 9)) });
    assert_eq!(store.lookup(&dats, &headered).unwrap().as_deref(), Some("Golden Test (USA)"));
    let zipped = HashOutcome::ZipEntry { crc32: "22222222".into(), size: 3, inner_name: "q.nes".into() };
    assert_eq!(store.lookup(&dats, &zipped).unwrap().as_deref(), Some("Quote \"Test\" (Japan)"));
    let wrong_size = HashOutcome::ZipEntry { crc32: "22222222".into(), size: 4, inner_name: "q.nes".into() };
    assert_eq!(store.lookup(&dats, &wrong_size).unwrap(), None);
    assert_eq!(store.lookup(&dats, &HashOutcome::NameOnly).unwrap(), None);
}

#[test]
fn a_sha1_mismatch_rejects_a_crc_collision() {
    let server = MockServer::start();
    let path = "metadat/no-intro/X.dat";
    server.serve_dat(path, NES_DAT);
    let (http, cache) = (http(), Cache::in_memory().unwrap());
    let store = DatStore::new(&http, &cache, server.base());
    let mut d = digests("CBF43926", 9);
    d.sha1 = "0000000000000000000000000000000000000000".into();
    let outcome = HashOutcome::Content(RomHashes { full: d, headerless: None });
    assert_eq!(store.lookup(&[path.to_string()], &outcome).unwrap(), None);
}

#[test]
fn missing_dat_is_remembered_and_arcade_names_resolve() {
    let server = MockServer::start();
    server.serve_dat("metadat/mame/MAME.dat", MAME_DAT);
    let (http, cache) = (http(), Cache::in_memory().unwrap());
    let store = DatStore::new(&http, &cache, server.base());
    assert_eq!(store.ensure("metadat/redump/Nope.dat").unwrap(), DatStatus::Missing);
    let fresh = DatStore::new(&http, &cache, server.base());
    assert_eq!(fresh.ensure("metadat/redump/Nope.dat").unwrap(), DatStatus::Missing);
    assert_eq!(server.hits("/metadat/redump/Nope.dat"), 1, "404 is cached for the TTL");
    let dats = vec!["metadat/mame/MAME.dat".to_string()];
    assert_eq!(store.arcade_description(&dats, "10YARD").unwrap().as_deref(), Some("10-Yard Fight (World, set 1)"));
    assert_eq!(store.arcade_description(&dats, "nothing").unwrap(), None);
}

#[test]
fn offline_with_stale_rows_uses_them() {
    let cache = Cache::in_memory().unwrap();
    let path = "metadat/no-intro/X.dat";
    {
        let server = MockServer::start();
        server.serve_dat(path, NES_DAT);
        let http = http();
        DatStore::new(&http, &cache, server.base()).ensure(path).unwrap();
    } // server gone
    let http = http();
    let store = DatStore::new(&http, &cache, "http://127.0.0.1:9").with_ttl_secs(0);
    assert_eq!(store.ensure(path).unwrap(), DatStatus::Stale);
    let zipped = HashOutcome::ZipEntry { crc32: "22222222".into(), size: 3, inner_name: "q".into() };
    assert!(store.lookup(&[path.to_string()], &zipped).unwrap().is_some());
}

#[test]
fn oversized_dat_is_refused() {
    let server = MockServer::start();
    server.mock("/big.dat", MockResponse::ok(vec![b' '; 2048]));
    let (http, cache) = (http(), Cache::in_memory().unwrap());
    let store = DatStore::new(&http, &cache, server.base()).with_max_bytes(1024);
    assert!(store.ensure("big.dat").is_err());
}
```

(Also add `pub fn with_ttl_secs(self, secs: i64) -> Self` and `pub fn with_max_bytes(self, n: u64) -> Self` to `DatStore`.)

- [ ] **Step 2: Run to see it fail**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && cargo test -p coverly-core --test dat`
Expected: FAIL to compile.

- [ ] **Step 3: Add `dats` to systems**

`System` gains `/// libretro-database files (repo-relative) to identify ROMs with; empty = derived from libretro name. pub dats: Vec<String>`; `RawSystem` gains `dats: Option<Vec<String>>` (when given it **replaces** the list). In `systems.toml`: `mame` → `dats = ["metadat/mame/MAME.dat"]`; `fbneo` → `dats = ["metadat/fbneo-split/FBNeo - Arcade Games.dat"]`; `cps1`, `cps2`, `cps3`, `neogeo` → `dats = ["metadat/fbneo-split/FBNeo - Arcade Games.dat", "metadat/mame/MAME.dat"]`; `hbmame` → `dats = ["dat/HBMAME.dat"]`; document the key in the header comment.

- [ ] **Step 4: Implement `identify/dat.rs`**

```rust
//! libretro-database DATs (clrmamepro format), downloaded on demand into
//! the cache (spec §16.6, §17 q3: never bundled). Rows are stored in SQLite
//! so a lookup is one indexed query; a DAT is re-checked after 30 days with
//! a conditional GET; a 404 is remembered for the same TTL.

use std::collections::HashMap;
use std::sync::Mutex;

use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};

use crate::cache::{now_unix, Cache, CacheError, DatRow};
use crate::identify::hash::HashOutcome;
use crate::net::{Http, HttpRequest, NetError};
use crate::systems::System;

pub const DATABASE_BASE: &str = "https://raw.githubusercontent.com/libretro/libretro-database/master";
pub const DAT_TTL_SECS: i64 = 30 * 24 * 3600;
pub const MAX_DAT_BYTES: u64 = 64 * 1024 * 1024;
const NS: &str = "dat-meta";
/// Unreserved characters stay; everything else in a segment is escaped.
pub(crate) const SEGMENT: &AsciiSet = &NON_ALPHANUMERIC.remove(b'-').remove(b'.').remove(b'_').remove(b'~').remove(b'(').remove(b')');

// DatGame, DatError, DatStatus as in Interfaces.

#[derive(serde::Serialize, serde::Deserialize)]
struct DatMeta { found: bool }

pub fn encode_path(path: &str) -> String {
    path.split('/').map(|s| utf8_percent_encode(s, SEGMENT).to_string()).collect::<Vec<_>>().join("/")
}

pub fn dat_sources(system: &System) -> Vec<String> {
    if !system.dats.is_empty() {
        return system.dats.clone();
    }
    match (&system.libretro, system.arcade) {
        (Some(name), false) if system.multi_file => vec![format!("metadat/redump/{name}.dat")],
        (Some(name), false) => vec![format!("metadat/no-intro/{name}.dat")],
        _ => Vec::new(),
    }
}

#[derive(Debug, PartialEq)]
enum Tok { Open, Close, Word(String) }

fn tokenize(text: &str) -> Result<Vec<(Tok, usize)>, DatError> {
    let mut out = Vec::new();
    let mut chars = text.chars().peekable();
    let mut line = 1;
    while let Some(c) = chars.next() {
        match c {
            '\n' => line += 1,
            c if c.is_whitespace() => {}
            '(' => out.push((Tok::Open, line)),
            ')' => out.push((Tok::Close, line)),
            '"' => {
                let mut s = String::new();
                loop {
                    match chars.next() {
                        None => return Err(DatError::Parse { line, message: "unterminated string".into() }),
                        Some('\\') => { if let Some(n) = chars.next() { s.push(n) } }
                        Some('"') => break,
                        Some('\n') => { line += 1; s.push('\n') }
                        Some(o) => s.push(o),
                    }
                }
                out.push((Tok::Word(s), line));
            }
            _ => {
                let mut s = String::from(c);
                while let Some(&n) = chars.peek() {
                    if n.is_whitespace() || n == '(' || n == ')' { break; }
                    s.push(n);
                    chars.next();
                }
                out.push((Tok::Word(s), line));
            }
        }
    }
    Ok(out)
}

/// Parses `game ( … rom ( … ) … )` blocks; other top-level blocks
/// (`clrmamepro`, `resource`, `machine` without roms) are skipped.
pub fn parse_clrmamepro(text: &str) -> Result<Vec<DatGame>, DatError> {
    let toks = tokenize(text)?;
    let mut games = Vec::new();
    let mut i = 0;
    while i < toks.len() {
        let (Tok::Word(head), line) = &toks[i] else {
            return Err(DatError::Parse { line: toks[i].1, message: "expected a block name".into() });
        };
        if toks.get(i + 1).map(|t| &t.0) != Some(&Tok::Open) {
            return Err(DatError::Parse { line: *line, message: format!("expected `(` after `{head}`") });
        }
        let end = block_end(&toks, i + 1)?;
        if head == "game" || head == "machine" {
            games.push(parse_game(&toks[i + 2..end])?);
        }
        i = end + 1;
    }
    Ok(games)
}

/// Index of the `)` closing the `(` at `open`.
fn block_end(toks: &[(Tok, usize)], open: usize) -> Result<usize, DatError> {
    let mut depth = 0;
    for (j, (t, _)) in toks.iter().enumerate().skip(open) {
        match t {
            Tok::Open => depth += 1,
            Tok::Close => { depth -= 1; if depth == 0 { return Ok(j); } }
            Tok::Word(_) => {}
        }
    }
    Err(DatError::Parse { line: toks.last().map(|t| t.1).unwrap_or(1), message: "unbalanced parentheses".into() })
}

fn parse_game(body: &[(Tok, usize)]) -> Result<DatGame, DatError> {
    let mut name = String::new();
    let mut roms = Vec::new();
    let mut i = 0;
    while i < body.len() {
        match (&body[i].0, body.get(i + 1).map(|t| &t.0)) {
            (Tok::Word(k), Some(Tok::Open)) => {
                let end = block_end(body, i + 1)?;
                if k == "rom" {
                    roms.push(parse_rom(&body[i + 2..end]));
                }
                i = end + 1;
            }
            (Tok::Word(k), Some(Tok::Word(v))) => {
                if k == "name" { name = v.clone(); }
                i += 2;
            }
            _ => i += 1,
        }
    }
    for r in &mut roms { r.game = name.clone(); }
    Ok(DatGame { name, roms })
}

fn parse_rom(body: &[(Tok, usize)]) -> DatRow {
    let mut row = DatRow { game: String::new(), rom_name: String::new(), size: None, crc32: None, md5: None, sha1: None };
    for pair in body.windows(2) {
        if let (Tok::Word(k), Tok::Word(v)) = (&pair[0].0, &pair[1].0) {
            match k.as_str() {
                "name" if row.rom_name.is_empty() => row.rom_name = v.clone(),
                "size" => row.size = v.parse().ok(),
                "crc" => row.crc32 = Some(v.to_uppercase()),
                "md5" => row.md5 = Some(v.to_uppercase()),
                "sha1" => row.sha1 = Some(v.to_uppercase()),
                _ => {}
            }
        }
    }
    row
}

pub struct DatStore<'a> {
    http: &'a dyn Http,
    cache: &'a Cache,
    base: String,
    ttl: i64,
    max_bytes: u64,
    memo: Mutex<HashMap<String, DatStatus>>,
}

impl<'a> DatStore<'a> {
    pub fn new(http: &'a dyn Http, cache: &'a Cache, base: impl Into<String>) -> DatStore<'a> {
        DatStore { http, cache, base: base.into().trim_end_matches('/').to_string(), ttl: DAT_TTL_SECS,
                   max_bytes: MAX_DAT_BYTES, memo: Mutex::default() }
    }
    pub fn with_ttl_secs(mut self, secs: i64) -> Self { self.ttl = secs; self }
    pub fn with_max_bytes(mut self, n: u64) -> Self { self.max_bytes = n; self }

    pub fn ensure(&self, dat: &str) -> Result<DatStatus, DatError> {
        if let Some(s) = self.memo.lock().unwrap().get(dat) {
            return Ok(*s);
        }
        let status = self.refresh(dat)?;
        self.memo.lock().unwrap().insert(dat.to_string(), status);
        Ok(status)
    }

    fn refresh(&self, dat: &str) -> Result<DatStatus, DatError> {
        let meta = self.cache.get_json::<DatMeta>(NS, dat)?;
        if let Some(m) = &meta {
            if now_unix() - m.fetched_at < self.ttl {
                return Ok(if m.value.found { DatStatus::Ready } else { DatStatus::Missing });
            }
        }
        let url = format!("{}/{}", self.base, encode_path(dat));
        let etag = meta.as_ref().filter(|m| m.value.found).and_then(|m| m.etag.clone());
        let resp = match self.http.get(&HttpRequest::get(url.clone()).max_bytes(self.max_bytes).if_none_match(etag)) {
            Ok(r) => r,
            Err(NetError::Offline(_) | NetError::Timeout | NetError::Status { .. }) if meta.as_ref().is_some_and(|m| m.value.found) => {
                return Ok(DatStatus::Stale);
            }
            Err(e) => return Err(e.into()),
        };
        match resp.status {
            304 => { self.cache.touch_json(NS, dat)?; Ok(DatStatus::Ready) }
            404 => { self.cache.put_json(NS, dat, &DatMeta { found: false }, None)?; Ok(DatStatus::Missing) }
            200..=299 => {
                let text = String::from_utf8_lossy(&resp.body);
                let rows: Vec<DatRow> = parse_clrmamepro(&text)?.into_iter().flat_map(|g| g.roms).collect();
                self.cache.replace_dat(dat, &rows)?;
                self.cache.put_json(NS, dat, &DatMeta { found: true }, resp.etag.as_deref())?;
                Ok(DatStatus::Ready)
            }
            status => Err(NetError::Status { status, url }.into()),
        }
    }

    /// The DAT game whose ROM matches `outcome`: zip CRC + size, or the
    /// headerless then the full digests (CRC + size, and SHA1 when both
    /// sides have one). Ties resolve to the first game by name.
    pub fn lookup(&self, dats: &[String], outcome: &HashOutcome) -> Result<Option<String>, DatError> {
        let probes: Vec<(&str, u64, Option<&str>)> = match outcome {
            HashOutcome::ZipEntry { crc32, size, .. } => vec![(crc32.as_str(), *size, None)],
            HashOutcome::Content(h) => h.headerless.iter().chain(std::iter::once(&h.full))
                .map(|d| (d.crc32.as_str(), d.size, Some(d.sha1.as_str()).filter(|s| !s.is_empty())))
                .collect(),
            _ => return Ok(None),
        };
        for dat in dats {
            if self.ensure(dat)? == DatStatus::Missing { continue; }
            for (crc, size, sha1) in &probes {
                let hit = self.cache.dat_by_crc(dat, crc)?.into_iter().find(|r| {
                    r.size.is_none_or(|s| s == *size)
                        && match (sha1, &r.sha1) { (Some(a), Some(b)) => a.eq_ignore_ascii_case(b), _ => true }
                });
                if let Some(row) = hit { return Ok(Some(row.game)); }
            }
        }
        Ok(None)
    }

    pub fn arcade_description(&self, dats: &[String], short_name: &str) -> Result<Option<String>, DatError> {
        for dat in dats {
            if self.ensure(dat)? == DatStatus::Missing { continue; }
            if let Some(row) = self.cache.dat_by_rom_stem(dat, short_name)?.into_iter().next() {
                return Ok(Some(row.game));
            }
        }
        Ok(None)
    }
}
```

(`Option::is_none_or` needs Rust 1.82 — fine for 1.90.) Add to `net/testing.rs`:

```rust
impl MockServer {
    /// Serves `text` at the libretro-database path `repo_path` (percent-encoded like `DatStore` requests it).
    pub fn serve_dat(&self, repo_path: &str, text: &str) {
        self.mock(&format!("/{}", crate::identify::dat::encode_path(repo_path)), MockResponse::ok(text.as_bytes().to_vec()));
    }
}
```

- [ ] **Step 5: Run to see it pass**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && cargo test -p coverly-core --test dat && cargo test -p coverly-core --lib systems`
Expected: PASS. Global checks.

- [ ] **Step 6: Commit**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && git add crates/coverly-core && git commit -m "feat(core): index libretro-database DATs on demand with arcade short names"
```

---

### Task 6: `ArtSource` trait and the libretro-thumbnails source

**Files:**
- Create: `crates/coverly-core/src/sources/mod.rs`, `crates/coverly-core/src/sources/libretro.rs`
- Modify: `crates/coverly-core/src/lib.rs` (`pub mod sources;`), `crates/coverly-core/src/net/testing.rs` (libretro fixtures + `png_bytes`)
- Test: `crates/coverly-core/tests/libretro.rs`

**Interfaces:**
- Consumes: `names::{ParsedName, parse_name, compatible, score, sanitise_libretro, libretro_key, Region}` (Task 2), `Cache` (Task 3), `Http`, `HttpRequest`, `NetError` (Task 4), `dat::SEGMENT` (Task 5), `Media`, `System`.
- Produces:

```rust
// coverly_core::sources
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MatchKind { Fuzzy, TitleExact, NameExact, Canonical }   // weakest → strongest
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Candidate {
    pub source: String, pub media: Media, pub name: String /* file name without .png */,
    pub url: String, pub fallback_url: Option<String>, pub regions: Vec<Region>,
    pub kind: MatchKind, pub score: u8, pub votes: Option<u32>, pub consensus: u8,
}
pub struct ArtQuery<'a> {
    pub system: &'a System, pub media: Media,
    pub canonical: Option<&'a str>,   // DAT / arcade name, when identified
    pub rom: &'a ParsedName,          // parsed ROM stem (numbering already stripped)
    pub stem: &'a str,                // raw stem, for the exact-file-name try
    pub by_name: bool,                // false for MatchMethod::HashOnly
    pub fuzzy: bool, pub min_score: u8, pub max_candidates: usize,
}
#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    #[error("{system} is not available on {source}")] SystemUnavailable { source: String, system: String },
    #[error(transparent)] Net(#[from] NetError),
    #[error(transparent)] Cache(#[from] CacheError),
    #[error("no image at {0}")] NotFound(String),
}
pub trait ArtSource: Send + Sync {
    fn id(&self) -> &str;
    fn candidates(&self, q: &ArtQuery) -> Result<Vec<Candidate>, SourceError>;  // sorted: kind desc, score desc, name asc
    fn has_name(&self, system: &System, media: Media, name: &str) -> Result<bool, SourceError>;
    fn fetch(&self, c: &Candidate) -> Result<Vec<u8>, SourceError>;
}
// coverly_core::sources::libretro
pub const LIBRETRO_BASE: &str = "https://thumbnails.libretro.com";
pub const LIBRETRO_RAW_BASE: &str = "https://raw.githubusercontent.com/libretro-thumbnails";
pub const INDEX_TTL_SECS: i64 = 2 * 24 * 3600;
pub const MAX_INDEX_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_IMAGE_BYTES: u64 = 16 * 1024 * 1024;
pub fn named_dir(media: Media) -> &'static str;          // Named_Boxarts | Named_Snaps | Named_Titles
pub fn raw_repo_name(server_system: &str) -> String;     // spaces→_, plus the two known renames
pub fn encode_segment(s: &str) -> String;
pub fn parse_apache_files(html: &str) -> Vec<String>;    // decoded *.png names, hostile ones dropped
pub fn parse_apache_dirs(html: &str) -> Vec<String>;     // decoded dir names without "/", no "stats"
pub struct LibretroConfig { pub base: String, pub raw_base: String, pub ttl_secs: i64 }  // Default = constants
pub struct LibretroSource<'a>;
impl<'a> LibretroSource<'a> { pub fn new(http: &'a dyn Http, cache: &'a Cache, cfg: LibretroConfig) -> Self;
                              pub fn server_system(&self, system: &System) -> Result<String, SourceError>; }
impl ArtSource for LibretroSource<'_>   // id() == "libretro"
// coverly_core::net::testing gains:
impl MockServer {
    pub fn serve_libretro_root(&self, systems: &[&str]);
    pub fn serve_libretro_index(&self, system: &str, media_dir: &str, files: &[&str]);  // HTML listing only
    pub fn serve_libretro_image(&self, system: &str, media_dir: &str, file: &str, body: Vec<u8>);
}
pub fn apache_listing(title: &str, hrefs: &[String]) -> String;
pub fn png_bytes(width: u32, height: u32) -> Vec<u8>;   // solid-colour PNG (needs the `image` crate: Task 6 adds it — see Step 1)
```

- [ ] **Step 1: Dependency for test images**

`png_bytes` needs an encoder; add the image crate now (Task 10 uses it too): workspace `image = { version = "0.25", default-features = false, features = ["png", "jpeg", "webp", "gif", "bmp"] }`, core `image.workspace = true`.

- [ ] **Step 2: Write the failing tests**

`crates/coverly-core/tests/libretro.rs`:

```rust
use coverly_core::cache::Cache;
use coverly_core::identify::names::parse_name;
use coverly_core::net::testing::{apache_listing, png_bytes, MockResponse, MockServer};
use coverly_core::net::{NetConfig, PoliteHttp, UreqHttp};
use coverly_core::profiles::Media;
use coverly_core::sources::libretro::{encode_segment, parse_apache_dirs, parse_apache_files, raw_repo_name, LibretroConfig, LibretroSource};
use coverly_core::sources::{ArtQuery, ArtSource, MatchKind, SourceError};
use coverly_core::systems::SystemDb;

const GBA: &str = "Nintendo - Game Boy Advance";

fn http() -> PoliteHttp<UreqHttp> {
    let cfg = NetConfig { retries: 0, ..NetConfig::default() };
    PoliteHttp::new(UreqHttp::new(&cfg), &cfg)
}

fn cfg(server: &MockServer) -> LibretroConfig {
    LibretroConfig { base: server.base(), raw_base: server.url("/raw"), ..LibretroConfig::default() }
}

fn gba_server() -> MockServer {
    let server = MockServer::start();
    server.serve_libretro_root(&[GBA, "MAME"]);
    server.serve_libretro_index(GBA, "Named_Boxarts", &[
        "Golden Sun (USA, Europe).png",
        "Golden Sun (Japan).png",
        "Golden Sun - The Lost Age (USA, Europe).png",
        "Street Fighter Alpha 3 (USA).png",
        "Batman _ Robin_ X (USA).png",
    ]);
    server
}

fn query<'a>(system: &'a coverly_core::systems::System, rom: &'a coverly_core::identify::names::ParsedName,
             stem: &'a str, canonical: Option<&'a str>) -> ArtQuery<'a> {
    ArtQuery { system, media: Media::Cover, canonical, rom, stem, by_name: true, fuzzy: true, min_score: 65, max_candidates: 5 }
}

#[test]
fn apache_index_parsing_keeps_only_safe_png_names() {
    let html = apache_listing("x", &[
        "?C=N;O=D".into(), "/Nintendo%20-%20Game%20Boy%20Advance/".into(),
        "Golden%20Sun%20(USA,%20Europe).png".into(), "Zelda%20(USA).PNG".into(), "readme.txt".into(),
    ]);
    assert_eq!(parse_apache_files(&html), ["Golden Sun (USA, Europe).png", "Zelda (USA).PNG"]);
}

#[test]
fn apache_index_drops_traversal_and_control_names() {
    let html = apache_listing("x", &[
        "..%2F..%2Fevil.png".into(), "a%2Fb.png".into(), "a%5Cb.png".into(), "%00x.png".into(),
        "..".into(), "ok.png".into(), "%E2%80%AEgnp.exe.png".into(),
    ]);
    assert_eq!(parse_apache_files(&html), ["ok.png"]);
}

#[test]
fn root_listing_lists_system_folders() {
    let html = apache_listing("/", &["?C=M;O=A".into(), "Atari%20-%208-bit/".into(), "MAME/".into(), "stats/".into()]);
    assert_eq!(parse_apache_dirs(&html), ["Atari - 8-bit", "MAME"]);
}

#[test]
fn raw_repo_names_and_url_segments() {
    assert_eq!(raw_repo_name(GBA), "Nintendo_-_Game_Boy_Advance");
    assert_eq!(raw_repo_name("Atari - 8-bit"), "Atari_-_8-bit_Family");
    assert_eq!(raw_repo_name("NEC - PC-8001 - PC-8801"), "NEC_-_PC-88");
    assert_eq!(encode_segment("Golden Sun (USA, Europe).png"), "Golden%20Sun%20(USA%2C%20Europe).png");
}

#[test]
fn matching_order_canonical_name_title_fuzzy() {
    let server = gba_server();
    let (http, cache) = (http(), Cache::in_memory().unwrap());
    let src = LibretroSource::new(&http, &cache, cfg(&server));
    let db = SystemDb::builtin();
    let gba = db.get("gba").unwrap();

    let rom = parse_name("001 Golden Sun", true);
    let c = src.candidates(&query(gba, &rom, "001 Golden Sun", Some("Golden Sun (USA, Europe)"))).unwrap();
    assert_eq!((c[0].kind, c[0].name.as_str(), c[0].score), (MatchKind::Canonical, "Golden Sun (USA, Europe)", 100));
    assert!(c.iter().any(|x| x.kind == MatchKind::TitleExact && x.name == "Golden Sun (Japan)"));
    assert!(c.iter().all(|x| !x.name.contains("Lost Age")), "a sequel subtitle is not the same title");

    let rom = parse_name("Golden Sun (USA, Europe)", false);
    let c = src.candidates(&query(gba, &rom, "Golden Sun (USA, Europe)", None)).unwrap();
    assert_eq!(c[0].kind, MatchKind::NameExact);

    let rom = parse_name("001 Golden Sun", true);
    let c = src.candidates(&query(gba, &rom, "001 Golden Sun", None)).unwrap();
    assert_eq!(c.iter().filter(|x| x.kind == MatchKind::TitleExact).count(), 2);

    let rom = parse_name("Goldn Sun", false);
    let c = src.candidates(&query(gba, &rom, "Goldn Sun", None)).unwrap();
    assert_eq!(c[0].kind, MatchKind::Fuzzy);
    assert!(c[0].score >= 65 && c[0].score < 95, "{}", c[0].score);

    let rom = parse_name("Street Fighter Alpha 2", false);
    assert!(src.candidates(&query(gba, &rom, "Street Fighter Alpha 2", None)).unwrap().is_empty(), "numeral guard");

    let rom = parse_name("Goldn Sun", false);
    let mut q = query(gba, &rom, "Goldn Sun", None);
    q.by_name = false;
    assert!(src.candidates(&q).unwrap().is_empty(), "hash-only never matches by name");
}

#[test]
fn canonical_names_are_sanitised_like_libretro() {
    let server = gba_server();
    let (http, cache) = (http(), Cache::in_memory().unwrap());
    let src = LibretroSource::new(&http, &cache, cfg(&server));
    let db = SystemDb::builtin();
    let rom = parse_name("x", false);
    let c = src.candidates(&query(db.get("gba").unwrap(), &rom, "x", Some("Batman & Robin: X (USA)"))).unwrap();
    assert_eq!(c[0].name, "Batman _ Robin_ X (USA)");
    assert_eq!(c[0].url, server.url("/Nintendo%20-%20Game%20Boy%20Advance/Named_Boxarts/Batman%20_%20Robin_%20X%20(USA).png"));
}

#[test]
fn index_is_cached_and_revalidated_with_etag() {
    let server = gba_server();
    let (http, cache) = (http(), Cache::in_memory().unwrap());
    let db = SystemDb::builtin();
    let gba = db.get("gba").unwrap();
    let rom = parse_name("Golden Sun", false);
    let index_path = "/Nintendo%20-%20Game%20Boy%20Advance/Named_Boxarts/";
    LibretroSource::new(&http, &cache, cfg(&server)).candidates(&query(gba, &rom, "Golden Sun", None)).unwrap();
    LibretroSource::new(&http, &cache, cfg(&server)).candidates(&query(gba, &rom, "Golden Sun", None)).unwrap();
    assert_eq!(server.hits(index_path), 1, "second source instance reads the SQLite copy");
    assert_eq!(server.hits("/"), 1);

    server.mock(index_path, MockResponse::status(304));
    let expired = LibretroConfig { ttl_secs: 0, ..cfg(&server) };
    let c = LibretroSource::new(&http, &cache, expired).candidates(&query(gba, &rom, "Golden Sun", None)).unwrap();
    assert!(!c.is_empty(), "304 keeps the cached listing");
    let last = server.requests().into_iter().filter(|r| r.path == index_path).last().unwrap();
    assert_eq!(last.header("if-none-match"), Some("\"listing-v1\""));
}

#[test]
fn unknown_systems_are_unavailable() {
    let server = gba_server();
    let (http, cache) = (http(), Cache::in_memory().unwrap());
    let src = LibretroSource::new(&http, &cache, cfg(&server));
    let db = SystemDb::builtin();
    let rom = parse_name("x", false);
    let err = src.candidates(&query(db.get("snes").unwrap(), &rom, "x", None)).unwrap_err();
    assert!(matches!(err, SourceError::SystemUnavailable { .. }), "{err}");
}

#[test]
fn fetch_falls_back_to_the_raw_github_host() {
    let server = gba_server();
    server.mock("/raw/Nintendo_-_Game_Boy_Advance/master/Named_Boxarts/Golden%20Sun%20(Japan).png",
                MockResponse::ok(png_bytes(10, 10)));
    let (http, cache) = (http(), Cache::in_memory().unwrap());
    let src = LibretroSource::new(&http, &cache, cfg(&server));
    let db = SystemDb::builtin();
    let rom = parse_name("Golden Sun", false);
    let c = src.candidates(&query(db.get("gba").unwrap(), &rom, "Golden Sun", None)).unwrap();
    let japan = c.iter().find(|x| x.name == "Golden Sun (Japan)").unwrap();
    assert_eq!(src.fetch(japan).unwrap(), png_bytes(10, 10));
    let usa = c.iter().find(|x| x.name == "Golden Sun (USA, Europe)").unwrap();
    assert!(matches!(src.fetch(usa), Err(SourceError::NotFound(_))));
}

#[test]
fn has_name_checks_other_media_indexes() {
    let server = gba_server();
    server.serve_libretro_index(GBA, "Named_Snaps", &["Golden Sun (Japan).png"]);
    let (http, cache) = (http(), Cache::in_memory().unwrap());
    let src = LibretroSource::new(&http, &cache, cfg(&server));
    let db = SystemDb::builtin();
    let gba = db.get("gba").unwrap();
    assert!(src.has_name(gba, Media::Screenshot, "golden sun (japan)").unwrap());
    assert!(!src.has_name(gba, Media::Screenshot, "golden sun (usa, europe)").unwrap());
    assert!(!src.has_name(gba, Media::Title, "golden sun (japan)").unwrap(), "missing index (404) = no names");
}
```

`serve_libretro_index` sets `ETag: "listing-v1"` on the listing response.

- [ ] **Step 3: Run to see it fail**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && cargo test -p coverly-core --test libretro`
Expected: FAIL to compile.

- [ ] **Step 4: Implement `sources/mod.rs`**

The types and trait exactly as in Interfaces, with doc comments, plus:

```rust
pub mod libretro;

/// Orders candidates: strongest kind, then score, then name (deterministic).
pub fn sort_candidates(c: &mut [Candidate]) {
    c.sort_by(|a, b| b.kind.cmp(&a.kind).then(b.score.cmp(&a.score)).then_with(|| a.name.cmp(&b.name)));
}
```

- [ ] **Step 5: Implement `sources/libretro.rs`**

```rust
//! libretro-thumbnails (spec §16.5, ADR-0005): the system list comes from
//! the live server root, each `<System>/<Named_Type>/` Apache listing is
//! cached in SQLite for two days and revalidated with its ETag; names are
//! matched case-insensitively after the libretro sanitising; images come
//! from thumbnails.libretro.com with raw.githubusercontent.com as fallback.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use percent_encoding::{percent_decode_str, utf8_percent_encode};
use unicode_normalization::UnicodeNormalization;

use super::{sort_candidates, ArtQuery, ArtSource, Candidate, MatchKind, SourceError};
use crate::cache::{now_unix, Cache};
use crate::identify::dat::SEGMENT;
use crate::identify::names::{compatible, libretro_key, parse_name, sanitise_libretro, score, ParsedName};
use crate::net::{Http, HttpRequest};
use crate::profiles::Media;
use crate::systems::System;

pub const LIBRETRO_BASE: &str = "https://thumbnails.libretro.com";
pub const LIBRETRO_RAW_BASE: &str = "https://raw.githubusercontent.com/libretro-thumbnails";
pub const INDEX_TTL_SECS: i64 = 2 * 24 * 3600;
pub const MAX_INDEX_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_IMAGE_BYTES: u64 = 16 * 1024 * 1024;
const NS_ROOT: &str = "libretro-root";
const NS_INDEX: &str = "libretro-index";
/// Server folders whose GitHub repo has another name (research, correction 2).
const RAW_RENAMES: [(&str, &str); 2] = [("Atari - 8-bit", "Atari_-_8-bit_Family"), ("NEC - PC-8001 - PC-8801", "NEC_-_PC-88")];

pub fn named_dir(media: Media) -> &'static str {
    match media { Media::Cover => "Named_Boxarts", Media::Screenshot => "Named_Snaps", Media::Title => "Named_Titles" }
}

pub fn raw_repo_name(server_system: &str) -> String {
    RAW_RENAMES.iter().find(|(s, _)| *s == server_system).map(|(_, r)| r.to_string())
        .unwrap_or_else(|| server_system.replace(' ', "_"))
}

pub fn encode_segment(s: &str) -> String { utf8_percent_encode(s, SEGMENT).to_string() }

/// Every `href="…"` value, in order.
fn hrefs(html: &str) -> impl Iterator<Item = &str> {
    html.split("href=\"").skip(1).filter_map(|rest| rest.split('"').next())
}

/// Decodes an href into a safe single path segment, or `None`.
fn safe_name(href: &str) -> Option<String> {
    if href.starts_with('?') || href.starts_with('/') || href.contains("://") {
        return None;
    }
    let decoded = percent_decode_str(href).decode_utf8().ok()?;
    let name: String = decoded.nfc().collect();
    // A single segment cannot traverse without a separator, so titles with
    // "..." (e.g. "Wait... What (USA).png") stay valid.
    let bad = name.is_empty() || name == "." || name == ".."
        || name.contains(['/', '\\']) || name.chars().any(|c| c.is_control() || ('\u{202A}'..='\u{202E}').contains(&c) || ('\u{2066}'..='\u{2069}').contains(&c));
    (!bad).then_some(name)
}

pub fn parse_apache_files(html: &str) -> Vec<String> {
    hrefs(html).filter_map(safe_name).filter(|n| n.to_lowercase().ends_with(".png")).collect()
}

pub fn parse_apache_dirs(html: &str) -> Vec<String> {
    hrefs(html)
        .filter_map(|h| h.strip_suffix('/'))
        .filter_map(safe_name)
        .filter(|n| n != "stats")
        .collect()
}

#[derive(Clone, Debug)]
pub struct LibretroConfig { pub base: String, pub raw_base: String, pub ttl_secs: i64 }

impl Default for LibretroConfig {
    fn default() -> Self { LibretroConfig { base: LIBRETRO_BASE.into(), raw_base: LIBRETRO_RAW_BASE.into(), ttl_secs: INDEX_TTL_SECS } }
}

/// One parsed listing.
struct Entry { file: String, key: String, parsed: ParsedName }
struct Index { entries: Vec<Entry>, by_key: HashMap<String, usize> }

pub struct LibretroSource<'a> {
    http: &'a dyn Http,
    cache: &'a Cache,
    cfg: LibretroConfig,
    root: Mutex<Option<Arc<Vec<String>>>>,
    indexes: Mutex<HashMap<(String, Media), Arc<Index>>>,
}

impl<'a> LibretroSource<'a> {
    pub fn new(http: &'a dyn Http, cache: &'a Cache, mut cfg: LibretroConfig) -> Self {
        cfg.base = cfg.base.trim_end_matches('/').to_string();
        cfg.raw_base = cfg.raw_base.trim_end_matches('/').to_string();
        LibretroSource { http, cache, cfg, root: Mutex::default(), indexes: Mutex::default() }
    }

    /// GET with the SQLite copy: fresh → cached; stale → conditional GET
    /// (304 refreshes, 200 replaces, 404 → empty list); network error with
    /// a stale copy → the stale copy.
    fn listing(&self, ns: &str, key: &str, url: &str, dirs: bool) -> Result<Vec<String>, SourceError> {
        let stored = self.cache.get_json::<Vec<String>>(ns, key)?;
        if let Some(s) = &stored {
            if now_unix() - s.fetched_at < self.cfg.ttl_secs {
                return Ok(s.value.clone());
            }
        }
        let req = HttpRequest::get(url).max_bytes(MAX_INDEX_BYTES).if_none_match(stored.as_ref().and_then(|s| s.etag.clone()));
        let resp = match self.http.get(&req) {
            Ok(r) => r,
            Err(e @ crate::net::NetError::RateLimited { .. }) => return Err(e.into()),
            Err(e) => return stored.map(|s| s.value).ok_or(e.into()),
        };
        match resp.status {
            304 if stored.is_some() => { self.cache.touch_json(ns, key)?; Ok(stored.unwrap().value) }
            404 => { self.cache.put_json(ns, key, &Vec::<String>::new(), None)?; Ok(Vec::new()) }
            200..=299 => {
                let html = String::from_utf8_lossy(&resp.body);
                let names = if dirs { parse_apache_dirs(&html) } else { parse_apache_files(&html) };
                self.cache.put_json(ns, key, &names, resp.etag.as_deref())?;
                Ok(names)
            }
            status => Err(crate::net::NetError::Status { status, url: url.to_string() }.into()),
        }
    }

    fn root(&self) -> Result<Arc<Vec<String>>, SourceError> {
        let mut root = self.root.lock().unwrap();
        if let Some(r) = root.as_ref() { return Ok(r.clone()); }
        let list = Arc::new(self.listing(NS_ROOT, "root", &format!("{}/", self.cfg.base), true)?);
        *root = Some(list.clone());
        Ok(list)
    }

    /// The server folder for `system`, matched case-insensitively against the live root.
    pub fn server_system(&self, system: &System) -> Result<String, SourceError> {
        let unavailable = || SourceError::SystemUnavailable { source: "libretro".into(), system: system.name.clone() };
        let wanted = system.libretro.as_deref().ok_or_else(unavailable)?.to_lowercase();
        self.root()?.iter().find(|s| s.to_lowercase() == wanted).cloned().ok_or_else(unavailable)
    }

    fn index(&self, server: &str, media: Media) -> Result<Arc<Index>, SourceError> {
        let key = (server.to_string(), media);
        if let Some(i) = self.indexes.lock().unwrap().get(&key) { return Ok(i.clone()); }
        let url = format!("{}/{}/{}/", self.cfg.base, encode_segment(server), named_dir(media));
        let files = self.listing(NS_INDEX, &format!("{server}/{}", named_dir(media)), &url, false)?;
        let mut entries = Vec::with_capacity(files.len());
        let mut by_key = HashMap::new();
        for file in files {
            let key = libretro_key(&file);
            let parsed = parse_name(&key_display(&file), false);
            by_key.entry(key.clone()).or_insert(entries.len());
            entries.push(Entry { file, key, parsed });
        }
        let index = Arc::new(Index { entries, by_key });
        self.indexes.lock().unwrap().insert(key, index.clone());
        Ok(index)
    }

    fn candidate(&self, server: &str, media: Media, e: &Entry, kind: MatchKind, score: u8) -> Candidate {
        let dir = named_dir(media);
        Candidate {
            source: "libretro".into(), media,
            name: key_display(&e.file),
            url: format!("{}/{}/{dir}/{}", self.cfg.base, encode_segment(server), encode_segment(&e.file)),
            fallback_url: Some(format!("{}/{}/master/{dir}/{}", self.cfg.raw_base, raw_repo_name(server), encode_segment(&e.file))),
            regions: e.parsed.regions.clone(), kind, score, votes: None, consensus: 0,
        }
    }
}

/// File name without its `.png` extension (any case), as shown to users.
fn key_display(file: &str) -> String {
    // `get` instead of indexing: never slice inside a multi-byte character.
    match file.len().checked_sub(4).and_then(|i| Some((i, file.get(i..)?))) {
        Some((i, ext)) if i > 0 && ext.eq_ignore_ascii_case(".png") => file[..i].to_string(),
        _ => file.to_string(),
    }
}

impl ArtSource for LibretroSource<'_> {
    fn id(&self) -> &str { "libretro" }

    fn candidates(&self, q: &ArtQuery) -> Result<Vec<Candidate>, SourceError> {
        let server = self.server_system(q.system)?;
        let index = self.index(&server, q.media)?;
        let mut out: Vec<Candidate> = Vec::new();
        let mut taken = vec![false; index.entries.len()];
        let mut push = |i: usize, kind: MatchKind, score: u8, out: &mut Vec<Candidate>| {
            if !taken[i] { taken[i] = true; out.push(self.candidate(&server, q.media, &index.entries[i], kind, score)); }
        };
        // 1. canonical (DAT / arcade) name, then its other regions.
        if let Some(canon) = q.canonical {
            let key = libretro_key(&sanitise_libretro(canon));
            if let Some(&i) = index.by_key.get(&key) { push(i, MatchKind::Canonical, 100, &mut out); }
            let title = parse_name(canon, false).key;
            for (i, e) in index.entries.iter().enumerate() {
                if e.parsed.key == title { push(i, MatchKind::TitleExact, 95, &mut out); }
            }
        }
        if q.by_name {
            // 2. the ROM file name as is.
            let key = libretro_key(&sanitise_libretro(q.stem));
            if let Some(&i) = index.by_key.get(&key) { push(i, MatchKind::NameExact, 100, &mut out); }
            // 3. same title, any region.
            for (i, e) in index.entries.iter().enumerate() {
                if !q.rom.key.is_empty() && e.parsed.key == q.rom.key { push(i, MatchKind::TitleExact, 95, &mut out); }
            }
            // 4. fuzzy, only when nothing exact was found.
            if out.is_empty() && q.fuzzy {
                let mut scored: Vec<(u8, usize)> = index.entries.iter().enumerate()
                    .filter(|(_, e)| compatible(q.rom, &e.parsed))
                    .map(|(i, e)| (score(&q.rom.key, &e.parsed.key).max(score(&q.rom.short_key, &e.parsed.short_key).saturating_sub(10)), i))
                    .filter(|(s, _)| *s >= q.min_score)
                    .collect();
                scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| index.entries[a.1].file.cmp(&index.entries[b.1].file)));
                for (s, i) in scored.into_iter().take(q.max_candidates) { push(i, MatchKind::Fuzzy, s.min(94), &mut out); }
            }
        }
        sort_candidates(&mut out);
        out.truncate(q.max_candidates.max(1) + 8); // keep every exact variant, bounded
        Ok(out)
    }

    fn has_name(&self, system: &System, media: Media, name: &str) -> Result<bool, SourceError> {
        let server = self.server_system(system)?;
        Ok(self.index(&server, media)?.by_key.contains_key(&libretro_key(name)))
    }

    fn fetch(&self, c: &Candidate) -> Result<Vec<u8>, SourceError> {
        let mut last = SourceError::NotFound(c.url.clone());
        for url in std::iter::once(&c.url).chain(c.fallback_url.iter()) {
            match self.http.get(&HttpRequest::get(url.clone()).max_bytes(MAX_IMAGE_BYTES)) {
                Ok(r) if (200..300).contains(&r.status) && !r.body.is_empty() => return Ok(r.body),
                Ok(_) => last = SourceError::NotFound(url.clone()),
                Err(e @ crate::net::NetError::RateLimited { .. }) => return Err(e.into()),
                Err(e) => last = e.into(),
            }
        }
        Err(last)
    }
}
```

Notes for the implementer: the fuzzy short-key path scores the subtitle-less keys with a 10-point penalty so `Goldn Sun` beats nothing it should not; the numeral/year guard runs before scoring. The `truncate` keeps all exact variants (a title rarely has more than ~10 region files) while bounding fuzzy noise.

- [ ] **Step 6: Fixtures in `net/testing.rs`**

```rust
/// An Apache-style autoindex page with the given raw hrefs.
pub fn apache_listing(title: &str, hrefs: &[String]) -> String {
    let mut html = format!("<html><head><title>Index of {title}</title></head><body><h1>Index of {title}</h1><table>\n");
    for h in hrefs { html.push_str(&format!("<tr><td><a href=\"{h}\">{h}</a></td></tr>\n")); }
    html.push_str("</table></body></html>\n");
    html
}

/// A solid-colour RGBA PNG.
pub fn png_bytes(width: u32, height: u32) -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(width, height, image::Rgba([200, 30, 30, 255]));
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).expect("encode test png");
    out.into_inner()
}

impl MockServer {
    pub fn serve_libretro_root(&self, systems: &[&str]) {
        let mut hrefs = vec!["?C=N;O=D".to_string(), "stats/".to_string()];
        hrefs.extend(systems.iter().map(|s| format!("{}/", listing_href(s))));
        self.mock("/", MockResponse::ok(apache_listing("/", &hrefs)));
    }
    pub fn serve_libretro_index(&self, system: &str, media_dir: &str, files: &[&str]) {
        let sys = crate::sources::libretro::encode_segment(system);
        let mut hrefs = vec!["?C=N;O=D".to_string(), format!("/{sys}/")];
        hrefs.extend(files.iter().map(|f| listing_href(f)));
        self.mock(&format!("/{sys}/{media_dir}/"),
            MockResponse::ok(apache_listing(&format!("/{system}/{media_dir}"), &hrefs)).header("ETag", "\"listing-v1\""));
    }
    pub fn serve_libretro_image(&self, system: &str, media_dir: &str, file: &str, body: Vec<u8>) {
        use crate::sources::libretro::encode_segment;
        self.mock(&format!("/{}/{media_dir}/{}", encode_segment(system), encode_segment(file)),
                  MockResponse::ok(body).header("Content-Type", "image/png"));
    }
}

/// Apache's own href style: spaces as %20, commas and parentheses literal.
fn listing_href(name: &str) -> String {
    name.replace('%', "%25").replace(' ', "%20").replace('"', "%22")
}
```

- [ ] **Step 7: Run to see it pass**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && cargo test -p coverly-core --test libretro`
Expected: PASS. Global checks.

- [ ] **Step 8: Commit**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && git add Cargo.toml Cargo.lock crates/coverly-core && git commit -m "feat(core): add ArtSource trait and libretro-thumbnails source"
```

---

### Task 7: Identification and two-phase candidate gathering

**Files:**
- Modify: `crates/coverly-core/src/identify/mod.rs`, `crates/coverly-core/src/lib.rs` (`pub mod gather; pub mod progress;`)
- Create: `crates/coverly-core/src/progress.rs`, `crates/coverly-core/src/gather/mod.rs`
- Test: `crates/coverly-core/tests/gather.rs`

**Interfaces:**
- Consumes: Tasks 1–6; `art::{SystemArt, ArtState}`, `scan::RomEntry`, `systems::SystemDb`.
- Produces:

```rust
// coverly_core::identify
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MatchMethod { #[default] HashThenName, HashOnly, NameOnly }
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IdMethod { Hash, ArcadeDat, Name, Unknown }
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Identity { pub canonical: Option<String>, pub method: IdMethod, pub confidence: f32,
                      pub parsed: ParsedName, pub hash: Option<HashOutcome> }
#[derive(Clone, Debug)]
pub struct IdentifyOptions { pub method: MatchMethod, pub hash_max_bytes: u64, pub arcade_translation: bool }
impl Default for IdentifyOptions   // HashThenName, DEFAULT_HASH_MAX_BYTES, true
pub struct Identifier<'a>;
impl<'a> Identifier<'a> {
    pub fn new(cache: &'a Cache, dats: &'a DatStore<'a>, opts: IdentifyOptions) -> Self;
    pub fn identify(&self, rom: &RomEntry, system: &System, strip_numbering: bool) -> Result<Identity, DatError>;
}
// coverly_core::progress
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)] pub enum Phase { Gather, Apply }
#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum ProgressEvent { Started { phase: Phase, total: usize },
                         Item { phase: Phase, done: usize, total: usize, label: String },
                         Warning(String), Finished { phase: Phase } }
pub trait ProgressSink: Send + Sync { fn event(&self, e: ProgressEvent); }
pub struct NoProgress;  impl ProgressSink for NoProgress
// coverly_core::gather
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Scope { #[default] Missing, All }
#[derive(Clone, Debug, Default)]
pub struct TargetFilter { pub systems: Vec<String>, pub media: Vec<Media>, pub scope: Scope }
#[derive(Clone, Debug)]
pub struct GatherTarget { pub system_id: String, pub folder: String, pub system_dir: PathBuf,
                          pub rom: RomEntry, pub media: Vec<Media>, pub strip_numbering: bool }
pub fn select_targets(art: &[SystemArt], filter: &TargetFilter) -> Vec<GatherTarget>;
#[derive(Clone, Debug)]
pub struct GatherOptions { pub min_score: u8, pub fuzzy: bool, pub max_candidates: usize,
                           pub consensus: bool, pub refresh: bool, pub ttl_secs: i64 }
impl Default for GatherOptions   // 65, true, 5, false, false, INDEX_TTL_SECS
#[derive(Clone, Debug, Serialize)]
pub struct GatherItem { pub target: GatherTargetSummary, pub identity: Identity,
                        pub candidates: BTreeMap<Media, Vec<Candidate>>, pub errors: BTreeMap<Media, String> }
#[derive(Clone, Debug, Serialize)]
pub struct GatherTargetSummary { pub system_id: String, pub folder: String, pub system_dir: PathBuf,
                                 pub rel_dir: PathBuf, pub path: PathBuf, pub stem: String, pub media: Vec<Media> }
#[derive(Clone, Debug, Default, Serialize)]
pub struct Gathered { pub items: Vec<GatherItem>, pub warnings: Vec<String>, pub paused: Option<String>, pub cancelled: bool }
pub fn gather(targets: &[GatherTarget], systems: &SystemDb, sources: &[&dyn ArtSource], identifier: &Identifier,
              cache: &Cache, opts: &GatherOptions, progress: &dyn ProgressSink, cancel: &AtomicBool) -> Gathered;
```

- [ ] **Step 1: Write the failing tests**

`crates/coverly-core/tests/gather.rs`:

```rust
mod common;

use std::sync::atomic::AtomicBool;

use coverly_core::art::resolve_art;
use coverly_core::cache::Cache;
use coverly_core::gather::{gather, select_targets, GatherOptions, Scope, TargetFilter};
use coverly_core::identify::dat::DatStore;
use coverly_core::identify::{IdMethod, IdentifyOptions, Identifier, MatchMethod};
use coverly_core::net::testing::{MockResponse, MockServer};
use coverly_core::net::{NetConfig, PoliteHttp, UreqHttp};
use coverly_core::profiles::Media;
use coverly_core::progress::NoProgress;
use coverly_core::scan::{scan_library, ScanOptions};
use coverly_core::sources::libretro::{LibretroConfig, LibretroSource};
use coverly_core::sources::{ArtSource, MatchKind};
use coverly_core::systems::SystemDb;

const GBA: &str = "Nintendo - Game Boy Advance";
const GBA_DAT: &str = "metadat/no-intro/Nintendo - Game Boy Advance.dat";

/// Anbernic SD: GBA has 3 numbered ROMs (one with art), MAME one set.
fn library() -> tempfile::TempDir {
    let dir = tempfile::TempDir::new().unwrap();
    let r = dir.path();
    common::mkdir(r, "anbernic");
    common::write(r, "ROMS/GBA/001 Golden Sun.gba", "golden-sun-bytes");
    common::write(r, "ROMS/GBA/002 Goldn Adventure.gba", "unknown-bytes");
    common::write(r, "ROMS/GBA/003 Has Art.gba", "has-art");
    common::write(r, "ROMS/GBA/Imgs/003 Has Art.png", "png");
    common::write(r, "ROMS/MAME/10yard.zip", "not really a zip");
    dir
}

fn dat_text() -> String {
    let crc = format!("{:08X}", crc32fast::hash(b"golden-sun-bytes"));
    format!("clrmamepro (\n name \"GBA\"\n)\ngame (\n name \"Golden Sun (USA, Europe)\"\n rom ( name \"Golden Sun (USA, Europe).gba\" size 16 crc {crc} )\n)\n")
}

fn server() -> MockServer {
    let s = MockServer::start();
    s.serve_libretro_root(&[GBA, "MAME"]);
    s.serve_libretro_index(GBA, "Named_Boxarts", &["Golden Sun (USA, Europe).png", "Golden Sun (Japan).png", "Golden Adventure (USA).png"]);
    s.serve_libretro_index("MAME", "Named_Boxarts", &["10-Yard Fight (World, set 1).png"]);
    s.serve_dat(GBA_DAT, &dat_text());
    s.serve_dat("metadat/mame/MAME.dat", "game (\n name \"10-Yard Fight (World, set 1)\"\n rom ( name 10yard.zip size 1 crc 00000000 )\n)\n");
    s.serve_dat("metadat/fbneo-split/FBNeo - Arcade Games.dat", "");
    s
}

struct Env { http: PoliteHttp<UreqHttp>, cache: Cache }

fn env() -> Env {
    let cfg = NetConfig { retries: 0, ..NetConfig::default() };
    Env { http: PoliteHttp::new(UreqHttp::new(&cfg), &cfg), cache: Cache::in_memory().unwrap() }
}

#[test]
fn select_targets_filters_system_and_missing_media() {
    let lib = library();
    let profile = common::profile("anbernic-stock");
    let db = SystemDb::builtin();
    let scan = scan_library(lib.path(), &profile, &db, &ScanOptions::default());
    let art = resolve_art(&scan, &profile, &db, &[Media::Cover]);
    let filter = TargetFilter { systems: vec!["gba".into()], media: vec![Media::Cover], scope: Scope::Missing };
    let targets = select_targets(&art, &filter);
    let stems: Vec<_> = targets.iter().map(|t| t.rom.stem.as_str()).collect();
    assert_eq!(stems, ["001 Golden Sun", "002 Goldn Adventure"]);
    assert!(targets.iter().all(|t| t.strip_numbering && t.folder == "GBA"));
    let all = select_targets(&art, &TargetFilter { systems: vec!["GBA".into()], media: vec![Media::Cover], scope: Scope::All });
    assert_eq!(all.len(), 3);
    let screenshots = select_targets(&art, &TargetFilter { systems: vec![], media: vec![Media::Screenshot], scope: Scope::All });
    assert!(screenshots.is_empty(), "unsupported media are never targets");
}

#[test]
fn identifies_by_hash_name_and_arcade_dat_with_quick_id_memo() {
    let (lib, s, e) = (library(), server(), env());
    let dats = DatStore::new(&e.http, &e.cache, s.base());
    let db = SystemDb::builtin();
    let profile = common::profile("anbernic-stock");
    let scan = scan_library(lib.path(), &profile, &db, &ScanOptions::default());
    let rom = |folder: &str, stem: &str| scan.systems.iter().find(|x| x.folder_name == folder).unwrap()
        .roms.iter().find(|r| r.stem == stem).unwrap().clone();

    let id = Identifier::new(&e.cache, &dats, IdentifyOptions::default());
    let golden = rom("GBA", "001 Golden Sun");
    let identity = id.identify(&golden, db.get("gba").unwrap(), true).unwrap();
    assert_eq!((identity.method, identity.canonical.as_deref()), (IdMethod::Hash, Some("Golden Sun (USA, Europe)")));
    let mtime = coverly_core::cache::mtime_ns(golden.modified);
    assert!(e.cache.quick_id(&golden.path, golden.size, mtime).unwrap().is_some(), "hash memoised");

    let other = id.identify(&rom("GBA", "002 Goldn Adventure"), db.get("gba").unwrap(), true).unwrap();
    assert_eq!((other.method, other.canonical, other.parsed.title.as_str()), (IdMethod::Name, None, "Goldn Adventure"));

    let arcade = id.identify(&rom("MAME", "10yard"), db.get("mame").unwrap(), false).unwrap();
    assert_eq!((arcade.method, arcade.canonical.as_deref()), (IdMethod::ArcadeDat, Some("10-Yard Fight (World, set 1)")));

    let hash_only = Identifier::new(&e.cache, &dats, IdentifyOptions { method: MatchMethod::HashOnly, ..IdentifyOptions::default() });
    assert_eq!(hash_only.identify(&rom("GBA", "002 Goldn Adventure"), db.get("gba").unwrap(), true).unwrap().method, IdMethod::Unknown);

    let name_only = Identifier::new(&e.cache, &dats, IdentifyOptions { method: MatchMethod::NameOnly, ..IdentifyOptions::default() });
    assert_eq!(name_only.identify(&golden, db.get("gba").unwrap(), true).unwrap().method, IdMethod::Name);
}

#[test]
fn gather_stores_candidates_and_reuses_them_offline() {
    let (lib, e) = (library(), env());
    let db = SystemDb::builtin();
    let profile = common::profile("anbernic-stock");
    let scan = scan_library(lib.path(), &profile, &db, &ScanOptions::default());
    let art = resolve_art(&scan, &profile, &db, &[Media::Cover]);
    let targets = select_targets(&art, &TargetFilter { systems: vec!["GBA".into()], media: vec![Media::Cover], scope: Scope::Missing });

    let first = {
        let s = server();
        let dats = DatStore::new(&e.http, &e.cache, s.base());
        let id = Identifier::new(&e.cache, &dats, IdentifyOptions::default());
        let src = LibretroSource::new(&e.http, &e.cache, LibretroConfig { base: s.base(), raw_base: s.url("/raw"), ..Default::default() });
        let sources: Vec<&dyn ArtSource> = vec![&src];
        gather(&targets, &db, &sources, &id, &e.cache, &GatherOptions::default(), &NoProgress, &AtomicBool::new(false))
    };
    assert_eq!(first.items.len(), 2);
    let golden = &first.items[0];
    assert_eq!(golden.candidates[&Media::Cover][0].kind, MatchKind::Canonical);
    let fuzzy = &first.items[1];
    assert_eq!(fuzzy.candidates[&Media::Cover][0].name, "Golden Adventure (USA)");

    // Phase two needs no network: the server is gone, the cache answers.
    let dats = DatStore::new(&e.http, &e.cache, "http://127.0.0.1:9");
    let id = Identifier::new(&e.cache, &dats, IdentifyOptions::default());
    let src = LibretroSource::new(&e.http, &e.cache, LibretroConfig { base: "http://127.0.0.1:9".into(), raw_base: "http://127.0.0.1:9".into(), ..Default::default() });
    let sources: Vec<&dyn ArtSource> = vec![&src];
    let again = gather(&targets, &db, &sources, &id, &e.cache, &GatherOptions::default(), &NoProgress, &AtomicBool::new(false));
    assert_eq!(again.items[0].candidates, first.items[0].candidates);
    assert!(again.paused.is_none() && again.items.iter().all(|i| i.errors.is_empty()));
}

#[test]
fn rate_limit_pauses_gathering() {
    let (lib, e) = (library(), env());
    let s = server();
    s.mock("/Nintendo%20-%20Game%20Boy%20Advance/Named_Boxarts/", MockResponse::status(429).header("Retry-After", "0"));
    let db = SystemDb::builtin();
    let profile = common::profile("anbernic-stock");
    let scan = scan_library(lib.path(), &profile, &db, &ScanOptions::default());
    let art = resolve_art(&scan, &profile, &db, &[Media::Cover]);
    let targets = select_targets(&art, &TargetFilter { systems: vec!["GBA".into()], media: vec![Media::Cover], scope: Scope::Missing });
    let dats = DatStore::new(&e.http, &e.cache, s.base());
    let id = Identifier::new(&e.cache, &dats, IdentifyOptions::default());
    let src = LibretroSource::new(&e.http, &e.cache, LibretroConfig { base: s.base(), raw_base: s.url("/raw"), ..Default::default() });
    let sources: Vec<&dyn ArtSource> = vec![&src];
    let g = gather(&targets, &db, &sources, &id, &e.cache, &GatherOptions::default(), &NoProgress, &AtomicBool::new(false));
    assert!(g.paused.as_deref().unwrap_or("").contains("rate limited"), "{:?}", g.paused);
    assert!(g.items.len() < targets.len());
}

#[test]
fn going_offline_pauses_gathering() {
    let (lib, e) = (library(), env());
    let db = SystemDb::builtin();
    let profile = common::profile("anbernic-stock");
    let scan = scan_library(lib.path(), &profile, &db, &ScanOptions::default());
    let art = resolve_art(&scan, &profile, &db, &[Media::Cover]);
    let targets = select_targets(&art, &TargetFilter { systems: vec!["GBA".into()], media: vec![Media::Cover], scope: Scope::Missing });
    // Nothing cached and nothing listening: the first ROM must pause the phase,
    // not fail every ROM one by one with retries.
    let dats = DatStore::new(&e.http, &e.cache, "http://127.0.0.1:9");
    let id = Identifier::new(&e.cache, &dats, IdentifyOptions::default());
    let src = LibretroSource::new(&e.http, &e.cache, LibretroConfig { base: "http://127.0.0.1:9".into(), raw_base: "http://127.0.0.1:9".into(), ..Default::default() });
    let sources: Vec<&dyn ArtSource> = vec![&src];
    let g = gather(&targets, &db, &sources, &id, &e.cache, &GatherOptions::default(), &NoProgress, &AtomicBool::new(false));
    assert!(g.paused.as_deref().unwrap_or("").contains("offline"), "{:?}", g.paused);
    assert!(g.items.is_empty());
}

#[test]
fn consensus_counts_other_media_listings_and_is_part_of_the_cache_key() {
    let (lib, e) = (library(), env());
    let s = server();
    s.serve_libretro_index(GBA, "Named_Snaps", &["Golden Sun (Japan).png"]);
    s.serve_libretro_index(GBA, "Named_Titles", &["Golden Sun (Japan).png"]);
    let db = SystemDb::builtin();
    let profile = common::profile("anbernic-stock");
    let scan = scan_library(lib.path(), &profile, &db, &ScanOptions::default());
    let art = resolve_art(&scan, &profile, &db, &[Media::Cover]);
    let targets = select_targets(&art, &TargetFilter { systems: vec!["GBA".into()], media: vec![Media::Cover], scope: Scope::Missing });
    let dats = DatStore::new(&e.http, &e.cache, s.base());
    let id = Identifier::new(&e.cache, &dats, IdentifyOptions::default());
    let src = LibretroSource::new(&e.http, &e.cache, LibretroConfig { base: s.base(), raw_base: s.url("/raw"), ..Default::default() });
    let sources: Vec<&dyn ArtSource> = vec![&src];
    let consensus_of = |g: &coverly_core::gather::Gathered, name: &str| {
        g.items[0].candidates[&Media::Cover].iter().find(|c| c.name == name).unwrap().consensus
    };
    // First without consensus (cached), then with it: the cached run must not be reused.
    let plain = gather(&targets, &db, &sources, &id, &e.cache, &GatherOptions::default(), &NoProgress, &AtomicBool::new(false));
    assert_eq!(consensus_of(&plain, "Golden Sun (Japan)"), 0);
    let opts = GatherOptions { consensus: true, ..GatherOptions::default() };
    let g = gather(&targets, &db, &sources, &id, &e.cache, &opts, &NoProgress, &AtomicBool::new(false));
    assert_eq!(consensus_of(&g, "Golden Sun (Japan)"), 3);
    assert_eq!(consensus_of(&g, "Golden Sun (USA, Europe)"), 1);
}
```

(`crc32fast` is a regular dependency of the crate, so tests may use it.)

- [ ] **Step 2: Run to see it fail**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && cargo test -p coverly-core --test gather`
Expected: FAIL to compile.

- [ ] **Step 3: Implement `progress.rs`** — the types in Interfaces; `NoProgress::event` does nothing.

- [ ] **Step 4: Implement the identifier in `identify/mod.rs`**

```rust
pub mod dat;
pub mod hash;
pub mod names;

// MatchMethod, IdMethod, Identity, IdentifyOptions as in Interfaces.

pub struct Identifier<'a> { cache: &'a Cache, dats: &'a DatStore<'a>, opts: IdentifyOptions }

impl<'a> Identifier<'a> {
    pub fn new(cache: &'a Cache, dats: &'a DatStore<'a>, opts: IdentifyOptions) -> Self { Identifier { cache, dats, opts } }

    /// Arcade: short name → description (no hashing). Otherwise, unless
    /// name-only: hash (memoised by path + size + mtime) → DAT game. A miss
    /// falls back to the parsed name (hash-only: `Unknown`).
    pub fn identify(&self, rom: &RomEntry, system: &System, strip_numbering: bool) -> Result<Identity, DatError> {
        let parsed = names::parse_name(&rom.stem, strip_numbering);
        let dats = dat::dat_sources(system);
        if system.arcade {
            if self.opts.arcade_translation {
                if let Some(desc) = self.dats.arcade_description(&dats, &rom.stem)? {
                    return Ok(Identity { parsed: names::parse_name(&desc, false), canonical: Some(desc), method: IdMethod::ArcadeDat, confidence: 1.0, hash: None });
                }
            }
            return Ok(Identity { canonical: None, method: IdMethod::Name, confidence: 0.0, parsed, hash: None });
        }
        if self.opts.method != MatchMethod::NameOnly && !dats.is_empty() {
            let mtime = crate::cache::mtime_ns(rom.modified);
            let outcome = match self.cache.quick_id(&rom.path, rom.size, mtime)? {
                Some(o) => o,
                None => {
                    let o = hash::hash_rom(&rom.path, &rom.ext, system.header, self.opts.hash_max_bytes)
                        .unwrap_or(hash::HashOutcome::NameOnly); // unreadable file: identify by name
                    self.cache.put_quick_id(&rom.path, rom.size, mtime, &o)?;
                    o
                }
            };
            if let Some(game) = self.dats.lookup(&dats, &outcome)? {
                return Ok(Identity { parsed: names::parse_name(&game, false), canonical: Some(game), method: IdMethod::Hash, confidence: 1.0, hash: Some(outcome) });
            }
            if self.opts.method == MatchMethod::HashOnly {
                return Ok(Identity { canonical: None, method: IdMethod::Unknown, confidence: 0.0, parsed, hash: Some(outcome) });
            }
        }
        Ok(Identity { canonical: None, method: IdMethod::Name, confidence: 0.0, parsed, hash: None })
    }
}
```

`DatError` gains `#[error(transparent)] Cache(#[from] CacheError)` if not already. Size note: `RomEntry.size` includes companions, but multi-file sets are name-only, so the memo key is still stable.

- [ ] **Step 5: Implement `gather/mod.rs`**

```rust
//! Phase one of a fill (spec §7 steps 3–5, §16.1): pick the ROMs and media
//! to fill, identify each ROM and gather art candidates from every enabled
//! source into the SQLite cache. Nothing is downloaded but indexes and DATs;
//! nothing is written to the library. A rate limit pauses the phase.

const NS: &str = "candidates";

pub fn select_targets(art: &[SystemArt], filter: &TargetFilter) -> Vec<GatherTarget> {
    let wanted = |s: &SystemArt| filter.systems.is_empty() || filter.systems.iter().any(|f| {
        f.eq_ignore_ascii_case(&s.scan.folder_name) || s.scan.system_id.as_deref().is_some_and(|id| f.eq_ignore_ascii_case(id))
    });
    let mut out = Vec::new();
    for system in art.iter().filter(|s| s.scan.system_id.is_some() && wanted(s)) {
        let strip = names::detect_numbering(system.roms.iter().map(|r| r.rom.stem.as_str()));
        for rom_art in &system.roms {
            let media: Vec<Media> = filter.media.iter().copied().filter(|m| match rom_art.art.get(m) {
                Some(ArtState::Missing) => true,
                Some(ArtState::Present { .. }) => filter.scope == Scope::All,
                _ => false,
            }).collect();
            if media.is_empty() { continue; }
            out.push(GatherTarget {
                system_id: system.scan.system_id.clone().unwrap(), folder: system.scan.folder_name.clone(),
                system_dir: system.scan.dir.clone(), rom: rom_art.rom.clone(), media, strip_numbering: strip,
            });
        }
    }
    out
}

pub fn gather(targets: &[GatherTarget], systems: &SystemDb, sources: &[&dyn ArtSource], identifier: &Identifier,
              cache: &Cache, opts: &GatherOptions, progress: &dyn ProgressSink, cancel: &AtomicBool) -> Gathered {
    let mut out = Gathered::default();
    progress.event(ProgressEvent::Started { phase: Phase::Gather, total: targets.len() });
    for (n, t) in targets.iter().enumerate() {
        if cancel.load(Ordering::SeqCst) { out.cancelled = true; break; }
        let Some(system) = systems.get(&t.system_id) else { continue };
        progress.event(ProgressEvent::Item { phase: Phase::Gather, done: n, total: targets.len(), label: t.rom.stem.clone() });
        let identity = match identifier.identify(&t.rom, system, t.strip_numbering) {
            Ok(i) => i,
            // Rate limited or offline (spec §10): pause the phase.
            Err(DatError::Net(e @ (NetError::RateLimited { .. } | NetError::Offline(_)))) => {
                out.paused = Some(e.to_string()); break;
            }
            Err(e) => {
                out.warnings.push(format!("{}: identification failed: {e}", t.rom.path.display()));
                Identity { canonical: None, method: IdMethod::Name, confidence: 0.0, parsed: names::parse_name(&t.rom.stem, t.strip_numbering), hash: None }
            }
        };
        let mut item = GatherItem { target: summary(t), identity, candidates: BTreeMap::new(), errors: BTreeMap::new() };
        'media: for &media in &t.media {
            let mut all = Vec::new();
            for src in sources {
                let key = cache_key(src.id(), system, media, &item.identity, &t.rom.stem, opts);
                if !opts.refresh {
                    if let Ok(Some(s)) = cache.get_json::<Vec<Candidate>>(NS, &key) {
                        if now_unix() - s.fetched_at < opts.ttl_secs { all.extend(s.value); continue; }
                    }
                }
                let q = ArtQuery { system, media, canonical: item.identity.canonical.as_deref(), rom: &item.identity.parsed,
                                   stem: &t.rom.stem, by_name: item.identity.method != IdMethod::Unknown || item.identity.canonical.is_some(),
                                   fuzzy: opts.fuzzy, min_score: opts.min_score, max_candidates: opts.max_candidates };
                match src.candidates(&q) {
                    Ok(mut found) => {
                        if opts.consensus { add_consensus(*src, system, &mut found); }
                        let _ = cache.put_json(NS, &key, &found, None);
                        all.extend(found);
                    }
                    Err(SourceError::Net(e @ (NetError::RateLimited { .. } | NetError::Offline(_)))) => {
                        out.paused = Some(e.to_string()); // "rate limited by … (HTTP 429)" / "offline or unreachable: …"
                        out.items.push(item);
                        break 'media;
                    }
                    Err(e) => { item.errors.insert(media, e.to_string()); }
                }
            }
            sort_candidates(&mut all);
            item.candidates.insert(media, all);
        }
        if out.paused.is_some() { break; }
        out.items.push(item);
    }
    progress.event(ProgressEvent::Finished { phase: Phase::Gather });
    out
}
```

Write `summary(t) -> GatherTargetSummary` (copies fields; `path = t.rom.path`, `rel_dir = t.rom.rel_dir`), `cache_key` = `format!("{src}\u{1f}{sys}\u{1f}{media:?}\u{1f}{canon_or_stem}\u{1f}{method:?}\u{1f}{fuzzy}\u{1f}{min_score}\u{1f}{max_candidates}\u{1f}{consensus}")` (every option that changes the candidate list or its fields is part of the key, so a cached run without consensus is never reused by `--strategy popularity`) with `canon_or_stem` = canonical or `parsed.key` + `|` + stem, and `add_consensus(src, system, found)`: for each candidate, `consensus = 1 + number of other media m where src.has_name(system, m, &c.name) == Ok(true)`. Note the `break 'media` path pushes the partial item and the outer `if out.paused.is_some()` stops before pushing it twice — restructure so the item is pushed exactly once (e.g. set a `paused` local flag and push after the media loop). `GatherItem` and `Gathered` derive `Serialize` so `--report json` can include them.

- [ ] **Step 6: Run to see it pass**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && cargo test -p coverly-core --test gather`
Expected: PASS. Global checks.

- [ ] **Step 7: Commit**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && git add crates/coverly-core && git commit -m "feat(core): identify ROMs and gather art candidates into the cache"
```

---

### Task 8: Strategies — Priority, ROM region, Popularity

**Files:**
- Create: `crates/coverly-core/src/matching/mod.rs`
- Modify: `crates/coverly-core/src/lib.rs` (`pub mod matching;`)
- Test: `crates/coverly-core/tests/strategies.rs`

**Interfaces:**
- Consumes: `Candidate`, `MatchKind` (Task 6), `Region` (Task 2).
- Produces:

```rust
// coverly_core::matching
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RegionPref { RomOwn, Region(Region) }
pub fn default_region_order() -> Vec<RegionPref>;                     // rom, world, usa, europe, japan
pub fn parse_region_order(s: &str) -> Result<Vec<RegionPref>, String>; // "rom,world,usa,eu,jp"
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StrategyKind { Priority, #[default] RomRegion, Popularity }
pub struct RankContext<'a> { pub rom_regions: &'a [Region], pub regions: &'a [RegionPref], pub sources: &'a [String] }
pub trait Strategy: Send + Sync {
    fn kind(&self) -> StrategyKind;
    /// Indices of `cands`, best first; eligible (best tier) candidates first.
    fn rank(&self, ctx: &RankContext, cands: &[Candidate]) -> Vec<usize>;
}
pub fn strategy_for(kind: StrategyKind) -> Box<dyn Strategy>;
pub struct Priority; pub struct RomRegion; pub struct Popularity;
pub const ELIGIBLE_SCORE_GAP: u8 = 5;
```

- [ ] **Step 1: Write the failing tests**

`crates/coverly-core/tests/strategies.rs`:

```rust
use coverly_core::identify::names::Region::{self, *};
use coverly_core::matching::{default_region_order, parse_region_order, strategy_for, RankContext, RegionPref, StrategyKind};
use coverly_core::profiles::Media;
use coverly_core::sources::{Candidate, MatchKind};

fn c(source: &str, name: &str, regions: &[Region], kind: MatchKind, score: u8) -> Candidate {
    Candidate { source: source.into(), media: Media::Cover, name: name.into(), url: format!("https://x/{name}"),
                fallback_url: None, regions: regions.to_vec(), kind, score, votes: None, consensus: 0 }
}

fn golden() -> Vec<Candidate> {
    vec![
        c("libretro", "Golden Sun (Japan)", &[Japan], MatchKind::TitleExact, 95),
        c("libretro", "Golden Sun (USA, Europe)", &[Usa, Europe], MatchKind::TitleExact, 95),
        c("libretro", "Golden Sum (USA)", &[Usa], MatchKind::Fuzzy, 80),
    ]
}

fn pick(kind: StrategyKind, rom: &[Region], cands: &[Candidate]) -> String {
    let regions = default_region_order();
    let sources = vec!["libretro".to_string(), "other".to_string()];
    let ctx = RankContext { rom_regions: rom, regions: &regions, sources: &sources };
    cands[strategy_for(kind).rank(&ctx, cands)[0]].name.clone()
}

#[test]
fn priority_follows_the_region_order_without_the_rom_region() {
    assert_eq!(pick(StrategyKind::Priority, &[Japan], &golden()), "Golden Sun (USA, Europe)");
}

#[test]
fn rom_region_prefers_the_rom_own_region_then_the_order() {
    assert_eq!(pick(StrategyKind::RomRegion, &[Japan], &golden()), "Golden Sun (Japan)");
    assert_eq!(pick(StrategyKind::RomRegion, &[], &golden()), "Golden Sun (USA, Europe)");
}

#[test]
fn a_weaker_match_kind_never_beats_the_best_tier() {
    let cands = vec![
        c("libretro", "Golden Sun (Japan)", &[Japan], MatchKind::TitleExact, 95),
        c("libretro", "Golden Sum (USA)", &[Usa], MatchKind::Fuzzy, 94),
    ];
    for kind in [StrategyKind::Priority, StrategyKind::RomRegion, StrategyKind::Popularity] {
        assert_eq!(pick(kind, &[Usa], &cands), "Golden Sun (Japan)", "{kind:?}");
    }
}

#[test]
fn popularity_uses_votes_then_consensus_then_priority() {
    let mut cands = golden();
    cands[0].consensus = 3;
    cands[1].consensus = 1;
    assert_eq!(pick(StrategyKind::Popularity, &[], &cands), "Golden Sun (Japan)");
    cands[0].votes = Some(2);
    cands[1].votes = Some(10);
    assert_eq!(pick(StrategyKind::Popularity, &[], &cands), "Golden Sun (USA, Europe)");
    let plain = golden();
    assert_eq!(pick(StrategyKind::Popularity, &[Japan], &plain), "Golden Sun (USA, Europe)", "falls back to Priority (D9)");
}

#[test]
fn source_order_breaks_ties_under_priority() {
    let cands = vec![
        c("other", "Zelda (USA)", &[Usa], MatchKind::TitleExact, 95),
        c("libretro", "Zelda (USA)", &[Usa], MatchKind::TitleExact, 95),
    ];
    let ranked_first = pick(StrategyKind::Priority, &[], &cands);
    assert_eq!(ranked_first, "Zelda (USA)");
    let regions = default_region_order();
    let sources = vec!["libretro".to_string(), "other".to_string()];
    let ctx = RankContext { rom_regions: &[], regions: &regions, sources: &sources };
    assert_eq!(cands[strategy_for(StrategyKind::Priority).rank(&ctx, &cands)[0]].source, "libretro");
}

#[test]
fn ranking_is_deterministic_for_any_input_order() {
    let base = golden();
    let mut rev = base.clone();
    rev.reverse();
    for kind in [StrategyKind::Priority, StrategyKind::RomRegion, StrategyKind::Popularity] {
        assert_eq!(pick(kind, &[Europe], &base), pick(kind, &[Europe], &rev), "{kind:?}");
    }
}

#[test]
fn prototypes_and_betas_never_beat_a_release() {
    // By name alone "(USA) (Proto)" sorts before "(USA, Europe)"; a release must still win.
    let cands = vec![
        c("libretro", "Zelda (USA) (Proto)", &[Usa], MatchKind::TitleExact, 95),
        c("libretro", "Zelda (USA, Europe)", &[Usa, Europe], MatchKind::TitleExact, 95),
        c("libretro", "Zelda (Europe)", &[Europe], MatchKind::TitleExact, 95),
    ];
    for kind in [StrategyKind::Priority, StrategyKind::RomRegion, StrategyKind::Popularity] {
        assert_eq!(pick(kind, &[Usa], &cands), "Zelda (USA, Europe)", "{kind:?}");
    }
    let only_proto_or_europe = vec![cands[0].clone(), cands[2].clone()];
    assert_eq!(pick(StrategyKind::RomRegion, &[Usa], &only_proto_or_europe), "Zelda (Europe)");
}

#[test]
fn every_candidate_is_ranked_once() {
    let regions = default_region_order();
    let ctx = RankContext { rom_regions: &[], regions: &regions, sources: &[] };
    let mut idx = strategy_for(StrategyKind::RomRegion).rank(&ctx, &golden());
    idx.sort();
    assert_eq!(idx, [0, 1, 2]);
}

#[test]
fn region_order_parsing() {
    assert_eq!(parse_region_order("rom, world,USA,eu,jp").unwrap(), [
        RegionPref::RomOwn, RegionPref::Region(World), RegionPref::Region(Usa),
        RegionPref::Region(Europe), RegionPref::Region(Japan),
    ]);
    assert_eq!(parse_region_order("rom,world,usa,europe,japan").unwrap(), default_region_order());
    assert!(parse_region_order("usa,mars").unwrap_err().contains("mars"));
}
```

- [ ] **Step 2: Run to see it fail**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && cargo test -p coverly-core --test strategies`
Expected: FAIL to compile.

- [ ] **Step 3: Implement `matching/mod.rs`**

```rust
//! Bulk selection strategies (ADR-0007, decision D9). Strategies only
//! order candidates; they never fetch or write. Eligible candidates are
//! those of the strongest match kind whose score is within
//! `ELIGIBLE_SCORE_GAP` of the best; the strategy orders them, the rest
//! follow by kind and score. Every tie ends on source order and name, so
//! the result never depends on input order.

use std::cmp::Ordering;

pub const ELIGIBLE_SCORE_GAP: u8 = 5;

pub fn default_region_order() -> Vec<RegionPref> {
    use Region::*;
    vec![RegionPref::RomOwn, RegionPref::Region(World), RegionPref::Region(Usa), RegionPref::Region(Europe), RegionPref::Region(Japan)]
}

pub fn parse_region_order(s: &str) -> Result<Vec<RegionPref>, String> {
    s.split(',').map(str::trim).filter(|p| !p.is_empty()).map(|p| {
        if p.eq_ignore_ascii_case("rom") { Ok(RegionPref::RomOwn) }
        else { Region::parse(p).map(RegionPref::Region).ok_or_else(|| format!("unknown region `{p}`")) }
    }).collect()
}

/// Position of the candidate's best region in `order` (RomOwn expanded to
/// `rom_regions` when `expand_rom`), or `order.len()` when none matches.
fn region_rank(c: &Candidate, ctx: &RankContext, expand_rom: bool) -> usize {
    let mut expanded: Vec<Region> = Vec::new();
    for p in ctx.regions {
        match p {
            RegionPref::RomOwn if expand_rom => expanded.extend(ctx.rom_regions.iter().copied()),
            RegionPref::RomOwn => {}
            RegionPref::Region(r) => expanded.push(*r),
        }
    }
    c.regions.iter().filter_map(|r| expanded.iter().position(|x| x == r)).min().unwrap_or(expanded.len())
}

/// Whether the variant is not a retail release (No-Intro / GoodTools tags).
fn unreleased(c: &Candidate) -> bool {
    const TAGS: [&str; 9] = ["(beta", "(proto", "(demo", "(sample", "(hack", "(pirate", "(kiosk", "(debug", "[h"];
    let lower = c.name.to_lowercase();
    TAGS.iter().any(|t| lower.contains(t))
}

fn source_rank(c: &Candidate, ctx: &RankContext) -> usize {
    ctx.sources.iter().position(|s| *s == c.source).unwrap_or(ctx.sources.len())
}

fn tail(a: &Candidate, b: &Candidate, ctx: &RankContext) -> Ordering {
    b.score.cmp(&a.score).then(source_rank(a, ctx).cmp(&source_rank(b, ctx))).then_with(|| a.name.cmp(&b.name)).then_with(|| a.source.cmp(&b.source))
}

/// Splits into (eligible, rest) and sorts both: `order` for eligible,
/// kind/score/name for the rest.
fn rank_with(ctx: &RankContext, cands: &[Candidate], order: impl Fn(&Candidate, &Candidate) -> Ordering) -> Vec<usize> {
    let Some(best) = cands.iter().max_by(|a, b| a.kind.cmp(&b.kind).then(a.score.cmp(&b.score))) else { return Vec::new() };
    let eligible = |c: &Candidate| c.kind == best.kind && c.score + ELIGIBLE_SCORE_GAP >= best.score;
    let (mut good, mut rest): (Vec<usize>, Vec<usize>) = (0..cands.len()).partition(|&i| eligible(&cands[i]));
    // A release always beats a beta/proto/demo/hack of the same title, whatever the strategy.
    good.sort_by(|&a, &b| unreleased(&cands[a]).cmp(&unreleased(&cands[b]))
        .then_with(|| order(&cands[a], &cands[b])).then_with(|| tail(&cands[a], &cands[b], ctx)));
    rest.sort_by(|&a, &b| cands[b].kind.cmp(&cands[a].kind).then_with(|| tail(&cands[a], &cands[b], ctx)));
    good.extend(rest);
    good
}

impl Strategy for Priority {
    fn kind(&self) -> StrategyKind { StrategyKind::Priority }
    fn rank(&self, ctx: &RankContext, cands: &[Candidate]) -> Vec<usize> {
        rank_with(ctx, cands, |a, b| source_rank(a, ctx).cmp(&source_rank(b, ctx)).then(region_rank(a, ctx, false).cmp(&region_rank(b, ctx, false))))
    }
}

impl Strategy for RomRegion {
    fn kind(&self) -> StrategyKind { StrategyKind::RomRegion }
    fn rank(&self, ctx: &RankContext, cands: &[Candidate]) -> Vec<usize> {
        rank_with(ctx, cands, |a, b| region_rank(a, ctx, true).cmp(&region_rank(b, ctx, true)).then(source_rank(a, ctx).cmp(&source_rank(b, ctx))))
    }
}

impl Strategy for Popularity {
    fn kind(&self) -> StrategyKind { StrategyKind::Popularity }
    fn rank(&self, ctx: &RankContext, cands: &[Candidate]) -> Vec<usize> {
        rank_with(ctx, cands, |a, b| b.votes.unwrap_or(0).cmp(&a.votes.unwrap_or(0))
            .then(b.consensus.cmp(&a.consensus))
            .then(source_rank(a, ctx).cmp(&source_rank(b, ctx)))
            .then(region_rank(a, ctx, false).cmp(&region_rank(b, ctx, false))))
    }
}

pub fn strategy_for(kind: StrategyKind) -> Box<dyn Strategy> {
    match kind { StrategyKind::Priority => Box::new(Priority), StrategyKind::RomRegion => Box::new(RomRegion), StrategyKind::Popularity => Box::new(Popularity) }
}
```

Doc comment on `Popularity`: "SteamGridDB votes (`Candidate::votes`, filled by the M6 source) first; for consoles, consensus = how many media listings of the same source contain the same file (gathered with `GatherOptions::consensus`); then Priority (D9)."

- [ ] **Step 4: Run to see it pass**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && cargo test -p coverly-core --test strategies`
Expected: PASS. Global checks.

- [ ] **Step 5: Commit**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && git add crates/coverly-core && git commit -m "feat(core): add Priority, ROM region and Popularity strategies"
```

---
### Task 9: Plan building (auto / review / none) and target paths

**Files:**
- Create: `crates/coverly-core/src/plan/mod.rs`
- Modify: `crates/coverly-core/src/art/mod.rs` (`pub fn target_path`), `crates/coverly-core/src/lib.rs` (`pub mod plan;`)
- Test: `crates/coverly-core/tests/plan.rs`

**Interfaces:**
- Consumes: `Gathered`, `GatherItem`, `select_targets` (Task 7), `strategy_for`, `RankContext`, `RegionPref`, `StrategyKind` (Task 8), `Candidate`, `MatchKind` (Task 6), `IdMethod` (Task 7), `crate::time::rfc3339_utc` (Task 3).
- Produces:

```rust
// coverly_core::art
pub fn target_path(root: &Path, profile: &Profile, system_dir: &Path, system: &System,
                   rel_dir: &Path, stem: &str, media: Media) -> Option<PathBuf>;
// coverly_core::plan
#[derive(Clone, Debug)]
pub struct PlanOptions { pub strategy: StrategyKind, pub regions: Vec<RegionPref>, pub sources: Vec<String>,
                         pub auto_threshold: f32, pub review_threshold: f32, pub max_alternatives: usize }
impl Default for PlanOptions  // RomRegion, default_region_order(), ["libretro"], 0.90, 0.65, 4
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "reason", rename_all = "kebab-case")]
pub enum NoneReason { NoCandidates, BelowThreshold { best: f32 }, SourceError { message: String }, NoTarget }
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "action", rename_all = "kebab-case")]
pub enum PlanAction {
    Auto { chosen: Candidate, confidence: f32, alternatives: Vec<Candidate> },
    Review { chosen: Candidate, confidence: f32, alternatives: Vec<Candidate> },
    None { #[serde(flatten)] reason: NoneReason },
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PlanItem { pub system_id: String, pub folder: String, pub rel_path: PathBuf, pub stem: String,
                      pub media: Media, pub target: PathBuf, pub identity_method: IdMethod,
                      pub canonical: Option<String>, pub replaces_existing: bool,
                      #[serde(flatten)] pub action: PlanAction }
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Counts { pub auto: usize, pub review: usize, pub none: usize }
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Plan { pub root: PathBuf, pub profile_id: String, pub created_at: String, pub strategy: StrategyKind,
                  pub items: Vec<PlanItem>, pub totals: Counts, pub per_system: BTreeMap<String, Counts>,
                  pub warnings: Vec<String>, pub paused: Option<String> }
pub fn confidence(c: &Candidate) -> f32;   // Canonical/NameExact 1.0, TitleExact 0.95, Fuzzy score/100
pub fn build_plan(root: &Path, profile: &Profile, systems: &SystemDb, gathered: &Gathered, opts: &PlanOptions) -> Plan;
```

- [ ] **Step 1: Write the failing tests**

`crates/coverly-core/tests/plan.rs`:

```rust
mod common;

use std::collections::BTreeMap;
use std::path::PathBuf;

use coverly_core::art::{resolve_art, target_path};
use coverly_core::gather::{select_targets, GatherItem, GatherTargetSummary, Gathered, Scope, TargetFilter};
use coverly_core::identify::names::{parse_name, Region};
use coverly_core::identify::{IdMethod, Identity};
use coverly_core::plan::{build_plan, NoneReason, PlanAction, PlanOptions};
use coverly_core::profiles::Media;
use coverly_core::scan::{scan_library, ScanOptions};
use coverly_core::sources::{Candidate, MatchKind};
use coverly_core::systems::SystemDb;

fn lib() -> tempfile::TempDir {
    let dir = tempfile::TempDir::new().unwrap();
    let r = dir.path();
    common::mkdir(r, "anbernic");
    for f in ["001 Golden Sun.gba", "002 Goldn Adventure.gba", "003 Nothing.gba", "Hacks/004 Odd.gba", "005 Broken.gba"] {
        common::write(r, &format!("ROMS/GBA/{f}"), f);
    }
    dir
}

fn cand(name: &str, kind: MatchKind, score: u8) -> Candidate {
    Candidate { source: "libretro".into(), media: Media::Cover, name: name.into(), url: format!("https://x/{name}.png"),
                fallback_url: None, regions: vec![Region::Usa], kind, score, votes: None, consensus: 0 }
}

fn gathered(root: &std::path::Path) -> Gathered {
    let profile = common::profile("anbernic-stock");
    let db = SystemDb::builtin();
    let scan = scan_library(root, &profile, &db, &ScanOptions::default());
    let art = resolve_art(&scan, &profile, &db, &[Media::Cover]);
    let targets = select_targets(&art, &TargetFilter { systems: vec![], media: vec![Media::Cover], scope: Scope::Missing });
    // Scan order is rel_dir then stem: 001, 002, 003, 005, Hacks/004 — so map by stem.
    let cands_for = |stem: &str| -> Vec<Candidate> {
        match stem {
            "001 Golden Sun" => vec![cand("Golden Sun (USA, Europe)", MatchKind::Canonical, 100)],
            "002 Goldn Adventure" => vec![cand("Golden Adventure (USA)", MatchKind::Fuzzy, 70)],
            "004 Odd" => vec![cand("Something (USA)", MatchKind::Fuzzy, 60)],
            _ => vec![],
        }
    };
    let items = targets.iter().map(|t| (t, cands_for(&t.rom.stem))).map(|(t, c)| GatherItem {
        target: GatherTargetSummary { system_id: t.system_id.clone(), folder: t.folder.clone(), system_dir: t.system_dir.clone(),
            rel_dir: t.rom.rel_dir.clone(), path: t.rom.path.clone(), stem: t.rom.stem.clone(), media: t.media.clone() },
        identity: Identity { canonical: None, method: IdMethod::Name, confidence: 0.0, parsed: parse_name(&t.rom.stem, true), hash: None },
        errors: if t.rom.stem == "005 Broken" { BTreeMap::from([(Media::Cover, "HTTP 500".to_string())]) } else { BTreeMap::new() },
        candidates: BTreeMap::from([(Media::Cover, c)]),
    }).collect();
    Gathered { items, ..Gathered::default() }
}

#[test]
fn target_paths_follow_the_profile_template() {
    let root = PathBuf::from("/sd");
    let profile = common::profile("anbernic-stock");
    let db = SystemDb::builtin();
    let gba = db.get("gba").unwrap();
    let sys_dir = root.join("ROMS/GBA");
    assert_eq!(target_path(&root, &profile, &sys_dir, gba, &PathBuf::from("Hacks"), "004 Odd", Media::Cover),
               Some(root.join("ROMS/GBA/Hacks/Imgs/004 Odd.png")));
    assert_eq!(target_path(&root, &profile, &sys_dir, gba, &PathBuf::new(), "x", Media::Screenshot), None);
}

#[test]
fn classifies_items_by_thresholds() {
    let dir = lib();
    let plan = build_plan(dir.path(), &common::profile("anbernic-stock"), &SystemDb::builtin(), &gathered(dir.path()), &PlanOptions::default());
    let by_stem = |s: &str| plan.items.iter().find(|i| i.stem == s).unwrap();
    match &by_stem("001 Golden Sun").action {
        PlanAction::Auto { chosen, confidence, .. } => { assert_eq!(chosen.name, "Golden Sun (USA, Europe)"); assert_eq!(*confidence, 1.0); }
        other => panic!("{other:?}"),
    }
    assert_eq!(by_stem("001 Golden Sun").target, dir.path().join("ROMS/GBA/Imgs/001 Golden Sun.png"));
    assert!(matches!(by_stem("002 Goldn Adventure").action, PlanAction::Review { confidence, .. } if (confidence - 0.70).abs() < 1e-6));
    assert!(matches!(by_stem("003 Nothing").action, PlanAction::None { reason: NoneReason::NoCandidates }));
    assert!(matches!(by_stem("004 Odd").action, PlanAction::None { reason: NoneReason::BelowThreshold { .. } }));
    assert!(matches!(&by_stem("005 Broken").action, PlanAction::None { reason: NoneReason::SourceError { message } } if message.contains("500")));
    assert_eq!(by_stem("004 Odd").target, dir.path().join("ROMS/GBA/Hacks/Imgs/004 Odd.png"));
    let gba = plan.per_system["GBA"];
    assert_eq!((gba.auto, gba.review, gba.none), (1, 1, 3));
    assert_eq!(plan.totals, gba);
}

#[test]
fn plan_serialises_with_flat_actions() {
    let dir = lib();
    let plan = build_plan(dir.path(), &common::profile("anbernic-stock"), &SystemDb::builtin(), &gathered(dir.path()), &PlanOptions::default());
    let v = serde_json::to_value(&plan).unwrap();
    let first = &v["items"][0];
    assert_eq!(first["action"], "auto");
    assert_eq!(first["media"], "cover");
    assert_eq!(first["chosen"]["kind"], "canonical");
    assert!(v["items"].as_array().unwrap().iter().any(|i| i["reason"] == "no-candidates"));
}
```

- [ ] **Step 2: Run to see it fail**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && cargo test -p coverly-core --test plan`
Expected: FAIL to compile.

- [ ] **Step 3: `art::target_path`**

Extract the rendering in `expected()` into a private `fn render_slot(root, profile, system_dir, system, slot, rel_dir, stem) -> Option<PathBuf>` (the `{libretro}` check, the `mirror_subdirs` choice of `rel_dir`, and `render`), make `expected()` call it, and add:

```rust
/// Where the profile stores `media` for the ROM `rel_dir`/`stem` of
/// `system` (whose folder is `system_dir`), or `None` when the profile
/// cannot store that media or the template needs a libretro name the
/// system lacks. The path is rendered, not checked against the disk.
pub fn target_path(root: &Path, profile: &Profile, system_dir: &Path, system: &System,
                   rel_dir: &Path, stem: &str, media: Media) -> Option<PathBuf> {
    render_slot(root, profile, system_dir, system, profile.slot(media)?, rel_dir, stem)
}
```

All existing `art_status` tests must still pass.

- [ ] **Step 4: Implement `plan/mod.rs`**

```rust
//! The plan (spec §7 step 6): one item per ROM and media, decided before
//! anything is written. `auto` when the chosen candidate's confidence
//! reaches the auto threshold, `review` between the thresholds, `none`
//! otherwise. Shown as the dry run; applied by `jobs::apply_plan`.

pub fn confidence(c: &Candidate) -> f32 {
    match c.kind {
        MatchKind::Canonical | MatchKind::NameExact => 1.0,
        MatchKind::TitleExact => 0.95,
        MatchKind::Fuzzy => f32::from(c.score.min(94)) / 100.0,
    }
}

pub fn build_plan(root: &Path, profile: &Profile, systems: &SystemDb, gathered: &Gathered, opts: &PlanOptions) -> Plan {
    let strategy = strategy_for(opts.strategy);
    let mut items = Vec::new();
    let (mut totals, mut per_system) = (Counts::default(), BTreeMap::<String, Counts>::new());
    for g in &gathered.items {
        let t = &g.target;
        let Some(system) = systems.get(&t.system_id) else { continue };
        for &media in &t.media {
            let target = target_path(root, profile, &t.system_dir, system, &t.rel_dir, &t.stem, media);
            let cands = g.candidates.get(&media).map(Vec::as_slice).unwrap_or(&[]);
            let action = match &target {
                None => PlanAction::None { reason: NoneReason::NoTarget },
                Some(_) if cands.is_empty() => match g.errors.get(&media) {
                    Some(message) => PlanAction::None { reason: NoneReason::SourceError { message: message.clone() } },
                    None => PlanAction::None { reason: NoneReason::NoCandidates },
                },
                Some(_) => {
                    let ctx = RankContext { rom_regions: &g.identity.parsed.regions, regions: &opts.regions, sources: &opts.sources };
                    let order = strategy.rank(&ctx, cands);
                    let chosen = cands[order[0]].clone();
                    let alternatives: Vec<Candidate> = order[1..].iter().take(opts.max_alternatives).map(|&i| cands[i].clone()).collect();
                    let c = confidence(&chosen);
                    if c >= opts.auto_threshold { PlanAction::Auto { chosen, confidence: c, alternatives } }
                    else if c >= opts.review_threshold { PlanAction::Review { chosen, confidence: c, alternatives } }
                    else { PlanAction::None { reason: NoneReason::BelowThreshold { best: c } } }
                }
            };
            let counts = per_system.entry(t.folder.clone()).or_default();
            for c in [&mut totals, counts] {
                match action { PlanAction::Auto { .. } => c.auto += 1, PlanAction::Review { .. } => c.review += 1, PlanAction::None { .. } => c.none += 1 }
            }
            let rel_path = t.path.strip_prefix(&t.system_dir).map(Path::to_path_buf).unwrap_or_else(|_| t.path.clone());
            items.push(PlanItem {
                system_id: t.system_id.clone(), folder: t.folder.clone(), rel_path, stem: t.stem.clone(), media,
                replaces_existing: target.as_deref().is_some_and(Path::exists),
                target: target.unwrap_or_default(), identity_method: g.identity.method,
                canonical: g.identity.canonical.clone(), action,
            });
        }
    }
    Plan { root: root.to_path_buf(), profile_id: profile.id.clone(), created_at: crate::time::rfc3339_utc(std::time::SystemTime::now()),
           strategy: opts.strategy, items, totals, per_system, warnings: gathered.warnings.clone(), paused: gathered.paused.clone() }
}
```

(`PlanAction::None` holds `reason` flattened, so JSON items read `{"action":"none","reason":"no-candidates",…}`.)

- [ ] **Step 5: Run to see it pass**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && cargo test -p coverly-core --test plan --test art_status`
Expected: PASS. Global checks.

- [ ] **Step 6: Commit**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && git add crates/coverly-core && git commit -m "feat(core): build fill plans with auto, review and none items"
```

---

### Task 10: Image pipeline — decode with limits, resize, re-encode

**Files:**
- Create: `crates/coverly-core/src/image/mod.rs`
- Modify: `crates/coverly-core/src/lib.rs` (`pub mod image;` — refer to the external crate as `::image` inside the core)
- Test: `crates/coverly-core/tests/image_pipeline.rs`

**Interfaces:**
- Consumes: `profiles::{ImageFormat, MediaSlot}`; `net::testing::png_bytes` in tests.
- Produces:

```rust
// coverly_core::image
#[derive(Clone, Copy, Debug)]
pub struct ImageLimits { pub max_input_bytes: usize, pub max_dimension: u32, pub max_alloc: u64 }
impl Default for ImageLimits   // 20 MiB, 8192, 256 MiB
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "kebab-case")]
pub enum Fit { #[default] Contain, Pad { rgba: [u8; 4] } }
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OutputSpec { pub format: ImageFormat, pub max_size: Option<(u32, u32)>, pub fit: Fit, pub jpeg_quality: u8 }
impl OutputSpec { pub fn for_slot(slot: &MediaSlot, fit: Fit, jpeg_quality: u8) -> OutputSpec; }
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ImageError {
    #[error("download is {bytes} bytes, over the limit")] TooLarge { bytes: usize },
    #[error("not an image")] NotAnImage,
    #[error("unsupported image format {0}")] Unsupported(String),
    #[error("image is {width}x{height}, over the limit")] Dimensions { width: u32, height: u32 },
    #[error("cannot decode image: {0}")] Decode(String),
    #[error("cannot encode image: {0}")] Encode(String),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessedImage { pub bytes: Vec<u8>, pub width: u32, pub height: u32 }
pub fn process_image(bytes: &[u8], spec: &OutputSpec, limits: &ImageLimits) -> Result<ProcessedImage, ImageError>;
pub fn probe_dimensions(bytes: &[u8]) -> Option<(u32, u32)>;   // header only, for tests and E2E checks
```

- [ ] **Step 1: Write the failing tests**

`crates/coverly-core/tests/image_pipeline.rs`:

```rust
use coverly_core::image::{probe_dimensions, process_image, Fit, ImageError, ImageLimits, OutputSpec};
use coverly_core::net::testing::png_bytes;
use coverly_core::profiles::{builtin_profiles, ImageFormat, Media};

fn spec(format: ImageFormat, fit: Fit) -> OutputSpec {
    OutputSpec { format, max_size: Some((250, 360)), fit, jpeg_quality: 90 }
}

#[test]
fn shrinks_into_the_box_keeping_aspect() {
    let out = process_image(&png_bytes(1000, 500), &spec(ImageFormat::Png, Fit::Contain), &ImageLimits::default()).unwrap();
    assert_eq!((out.width, out.height), (250, 125));
    assert_eq!(probe_dimensions(&out.bytes), Some((250, 125)));
    assert!(out.bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
}

#[test]
fn never_upscales() {
    let out = process_image(&png_bytes(100, 80), &spec(ImageFormat::Png, Fit::Contain), &ImageLimits::default()).unwrap();
    assert_eq!((out.width, out.height), (100, 80));
}

#[test]
fn pad_fills_the_whole_box() {
    let out = process_image(&png_bytes(1000, 500), &spec(ImageFormat::Png, Fit::Pad { rgba: [0, 0, 0, 0] }), &ImageLimits::default()).unwrap();
    assert_eq!(probe_dimensions(&out.bytes), Some((250, 360)));
}

#[test]
fn encodes_jpeg_when_the_slot_asks() {
    let out = process_image(&png_bytes(400, 400), &spec(ImageFormat::Jpg, Fit::Contain), &ImageLimits::default()).unwrap();
    assert!(out.bytes.starts_with(&[0xFF, 0xD8]));
    assert_eq!(probe_dimensions(&out.bytes), Some((250, 250)));
}

#[test]
fn rejects_non_images() {
    let err = process_image(b"<html><body>404 Not Found</body></html>", &spec(ImageFormat::Png, Fit::Contain), &ImageLimits::default()).unwrap_err();
    assert_eq!(err, ImageError::NotAnImage);
}

#[test]
fn rejects_truncated_images() {
    let png = png_bytes(64, 64);
    let err = process_image(&png[..60], &spec(ImageFormat::Png, Fit::Contain), &ImageLimits::default()).unwrap_err();
    assert!(matches!(err, ImageError::Decode(_)), "{err:?}");
}

#[test]
fn rejects_oversized_dimensions() {
    let err = process_image(&png_bytes(9000, 1), &spec(ImageFormat::Png, Fit::Contain), &ImageLimits::default()).unwrap_err();
    assert_eq!(err, ImageError::Dimensions { width: 9000, height: 1 });
}

#[test]
fn rejects_oversized_downloads() {
    let limits = ImageLimits { max_input_bytes: 10, ..ImageLimits::default() };
    assert!(matches!(process_image(&png_bytes(8, 8), &spec(ImageFormat::Png, Fit::Contain), &limits), Err(ImageError::TooLarge { .. })));
}

#[test]
fn output_spec_from_the_anbernic_slot() {
    let p = builtin_profiles().into_iter().find(|p| p.id == "anbernic-stock").unwrap();
    let s = OutputSpec::for_slot(p.slot(Media::Cover).unwrap(), Fit::Contain, 90);
    assert_eq!((s.format, s.max_size), (ImageFormat::Png, Some((250, 360))));
}
```

- [ ] **Step 2: Run to see it fail**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && cargo test -p coverly-core --test image_pipeline`
Expected: FAIL to compile.

- [ ] **Step 3: Implement `image/mod.rs`**

```rust
//! Downloaded art → the image a profile slot expects (spec §11: always
//! decoded and re-encoded, which also drops metadata). Hostile input is
//! bounded: byte size, sniffed format (PNG, JPEG, WebP, GIF, BMP only),
//! dimensions checked from the header before decoding, allocation limit.

use std::io::Cursor;

use ::image::codecs::jpeg::JpegEncoder;
use ::image::imageops::FilterType;
use ::image::{DynamicImage, ImageFormat as Codec, ImageReader, Limits, Rgba, RgbaImage};

use crate::profiles::{ImageFormat, MediaSlot};

impl Default for ImageLimits {
    fn default() -> Self { ImageLimits { max_input_bytes: 20 * 1024 * 1024, max_dimension: 8192, max_alloc: 256 * 1024 * 1024 } }
}

impl OutputSpec {
    pub fn for_slot(slot: &MediaSlot, fit: Fit, jpeg_quality: u8) -> OutputSpec {
        OutputSpec { format: slot.format, max_size: slot.max_size, fit, jpeg_quality: jpeg_quality.clamp(1, 100) }
    }
}

fn reader(bytes: &[u8], codec: Codec, limits: &ImageLimits) -> ImageReader<Cursor<&[u8]>> {
    let mut l = Limits::default();
    l.max_image_width = Some(limits.max_dimension);
    l.max_image_height = Some(limits.max_dimension);
    l.max_alloc = Some(limits.max_alloc);
    let mut r = ImageReader::with_format(Cursor::new(bytes), codec);
    r.limits(l);
    r
}

pub fn probe_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    let codec = ::image::guess_format(bytes).ok()?;
    ImageReader::with_format(Cursor::new(bytes), codec).into_dimensions().ok()
}

pub fn process_image(bytes: &[u8], spec: &OutputSpec, limits: &ImageLimits) -> Result<ProcessedImage, ImageError> {
    if bytes.len() > limits.max_input_bytes {
        return Err(ImageError::TooLarge { bytes: bytes.len() });
    }
    let codec = ::image::guess_format(bytes).map_err(|_| ImageError::NotAnImage)?;
    if !matches!(codec, Codec::Png | Codec::Jpeg | Codec::WebP | Codec::Gif | Codec::Bmp) {
        return Err(ImageError::Unsupported(format!("{codec:?}")));
    }
    let (w, h) = ImageReader::with_format(Cursor::new(bytes), codec).into_dimensions()
        .map_err(|e| ImageError::Decode(e.to_string()))?;
    if w == 0 || h == 0 || w > limits.max_dimension || h > limits.max_dimension {
        return Err(ImageError::Dimensions { width: w, height: h });
    }
    let mut img = reader(bytes, codec, limits).decode().map_err(|e| match e {
        ::image::ImageError::Limits(_) => ImageError::Dimensions { width: w, height: h },
        other => ImageError::Decode(other.to_string()),
    })?;
    if let Some((mw, mh)) = spec.max_size {
        if img.width() > mw || img.height() > mh {
            img = img.resize(mw, mh, FilterType::Lanczos3);
        }
        if let Fit::Pad { rgba } = spec.fit {
            let mut canvas = RgbaImage::from_pixel(mw, mh, Rgba(rgba));
            let (x, y) = ((mw - img.width()) / 2, (mh - img.height()) / 2);
            ::image::imageops::overlay(&mut canvas, &img.to_rgba8(), i64::from(x), i64::from(y));
            img = DynamicImage::ImageRgba8(canvas);
        }
    }
    let mut out = Cursor::new(Vec::new());
    match spec.format {
        ImageFormat::Png => img.write_to(&mut out, Codec::Png).map_err(|e| ImageError::Encode(e.to_string()))?,
        ImageFormat::Jpg => JpegEncoder::new_with_quality(&mut out, spec.jpeg_quality)
            .encode_image(&DynamicImage::ImageRgb8(img.to_rgb8()))
            .map_err(|e| ImageError::Encode(e.to_string()))?,
    }
    Ok(ProcessedImage { width: img.width(), height: img.height(), bytes: out.into_inner() })
}
```

A truncated PNG may fail in `into_dimensions` (header cut) or in `decode`; both map to `Decode`.

- [ ] **Step 4: Run to see it pass**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && cargo test -p coverly-core --test image_pipeline`
Expected: PASS. Global checks.

- [ ] **Step 5: Commit**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && git add crates/coverly-core && git commit -m "feat(core): decode, resize and re-encode art within safe limits"
```

---

### Task 11: Safe writer — atomic writes, backups, journal and undo

**Files:**
- Create: `crates/coverly-core/src/write/mod.rs`
- Modify: `crates/coverly-core/src/lib.rs` (`pub mod write;`)
- Test: `crates/coverly-core/tests/writer.rs`

**Interfaces:**
- Consumes: `crate::time::{compact_utc, rfc3339_utc}`, `identify::hash::hex_upper`, `sha1`.
- Produces:

```rust
// coverly_core::write
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OverwritePolicy { #[default] Never, Backup }
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub enum WriteOutcome { Created { path: PathBuf }, Replaced { path: PathBuf, backup: PathBuf }, SkippedExists { existing: PathBuf } }
#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    #[error("refusing to write outside the library: {0}")] OutsideRoot(PathBuf),
    #[error("file name not valid on FAT/exFAT: {0}")] NameInvalid(String),
    #[error("write denied (read-only or no permission): {0}")] Denied(PathBuf),
    #[error("disk full")] DiskFull,
    #[error("nothing to undo")] NothingToUndo,
    #[error("job journal is damaged: {0}")] Journal(String),
    #[error(transparent)] Io(#[from] std::io::Error),
}
pub fn safe_file_name(name: &str) -> Result<String, WriteError>;
pub fn ensure_within(root: &Path, target: &Path) -> Result<PathBuf, WriteError>;
pub fn sha1_hex(bytes: &[u8]) -> String;
pub struct Writer;
impl Writer {
    pub fn begin(root: &Path, data_dir: &Path) -> Result<Writer, WriteError>;
    pub fn job_id(&self) -> &str;
    pub fn write(&mut self, target: &Path, bytes: &[u8], policy: OverwritePolicy) -> Result<WriteOutcome, WriteError>;
    pub fn finish(self) -> Result<String, WriteError>;
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct UndoReport { pub job: String, pub removed: Vec<PathBuf>, pub restored: Vec<PathBuf>, pub skipped_modified: Vec<PathBuf> }
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct JobSummary { pub job: String, pub root: PathBuf, pub created_at: String, pub entries: usize, pub complete: bool, pub undone: bool }
pub fn undo_last(data_dir: &Path) -> Result<UndoReport, WriteError>;
pub fn undo_job(data_dir: &Path, job: &str) -> Result<UndoReport, WriteError>;
pub fn list_jobs(data_dir: &Path) -> Result<Vec<JobSummary>, WriteError>;   // newest first
pub fn prune_jobs(data_dir: &Path, keep: usize) -> Result<(), WriteError>;  // undo history depth
```

Layout: journal `<data>/jobs/<job>.jsonl` (renamed `<job>.undone.jsonl` after undo), backups `<data>/backups/<job>/<path relative to the library root>`. Job id: `format!("{}-{:04x}-{:04x}", compact_utc(now), std::process::id() & 0xffff, COUNTER.fetch_add(1, SeqCst))` with a process-wide `static COUNTER: AtomicU32`, e.g. `20261016T030000Z-1a2b-0000`; ids sort by creation time (same second → counter), which `list_jobs`, `undo_last` and `prune_jobs` rely on.

- [ ] **Step 1: Write the failing tests**

`crates/coverly-core/tests/writer.rs`:

```rust
use std::fs;
use std::path::{Path, PathBuf};

use coverly_core::write::*;
use tempfile::TempDir;

fn names_under(root: &Path) -> Vec<String> {
    walkdir::WalkDir::new(root).into_iter().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect()
}

fn setup() -> (TempDir, TempDir, PathBuf) {
    let (lib, data) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let imgs = lib.path().join("ROMS/SFC/Imgs");
    fs::create_dir_all(&imgs).unwrap();
    (lib, data, imgs)
}

#[test]
fn creates_files_atomically_without_leftovers() {
    let (lib, data, imgs) = setup();
    let mut w = Writer::begin(lib.path(), data.path()).unwrap();
    let target = imgs.join("Zelda (USA).png");
    assert!(matches!(w.write(&target, b"new", OverwritePolicy::Never).unwrap(), WriteOutcome::Created { .. }));
    w.finish().unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"new");
    let names = names_under(lib.path());
    assert!(names.iter().all(|n| !n.starts_with("._") && !n.starts_with(".coverly-")), "{names:?}");
}

#[test]
fn creates_missing_art_folders_inside_the_root() {
    let (lib, data, _) = setup();
    let mut w = Writer::begin(lib.path(), data.path()).unwrap();
    let target = lib.path().join("ROMS/GBA/Imgs/001 Golden Sun.png");
    w.write(&target, b"x", OverwritePolicy::Never).unwrap();
    assert!(target.exists());
}

#[test]
fn never_policy_skips_existing_and_case_insensitive_twin() {
    let (lib, data, imgs) = setup();
    fs::write(imgs.join("zelda (usa).png"), b"mine").unwrap();
    let mut w = Writer::begin(lib.path(), data.path()).unwrap();
    let out = w.write(&imgs.join("Zelda (USA).png"), b"new", OverwritePolicy::Never).unwrap();
    assert!(matches!(out, WriteOutcome::SkippedExists { .. }));
    assert_eq!(fs::read(imgs.join("zelda (usa).png")).unwrap(), b"mine");
}

#[test]
fn backup_policy_keeps_the_old_bytes_outside_the_library() {
    let (lib, data, imgs) = setup();
    let target = imgs.join("Zelda (USA).png");
    fs::write(&target, b"old").unwrap();
    let mut w = Writer::begin(lib.path(), data.path()).unwrap();
    let job = w.job_id().to_string();
    let WriteOutcome::Replaced { backup, .. } = w.write(&target, b"new", OverwritePolicy::Backup).unwrap() else { panic!() };
    w.finish().unwrap();
    assert_eq!(backup, data.path().join("backups").join(&job).join("ROMS/SFC/Imgs/Zelda (USA).png"));
    assert_eq!(fs::read(&backup).unwrap(), b"old");
    assert_eq!(fs::read(&target).unwrap(), b"new");
}

#[test]
fn refuses_targets_outside_the_root() {
    let (lib, data, _) = setup();
    let mut w = Writer::begin(lib.path(), data.path()).unwrap();
    for bad in [lib.path().join("../evil.png"), lib.path().join("ROMS/../../evil.png"), PathBuf::from("/tmp/evil.png"), lib.path().to_path_buf()] {
        assert!(matches!(w.write(&bad, b"x", OverwritePolicy::Never), Err(WriteError::OutsideRoot(_))), "{}", bad.display());
    }
}

#[test]
fn begin_requires_an_absolute_root() {
    // The journal stores the root; a relative one would make `coverly undo`
    // from another working directory touch the wrong files.
    let data = TempDir::new().unwrap();
    assert!(matches!(Writer::begin(Path::new("relative/sd"), data.path()), Err(WriteError::OutsideRoot(_))));
}

#[test]
fn listing_or_undoing_with_no_jobs_folder() {
    let data = TempDir::new().unwrap();
    assert!(list_jobs(data.path()).unwrap().is_empty());
    assert!(matches!(undo_last(data.path()), Err(WriteError::NothingToUndo)));
}

#[cfg(unix)]
#[test]
fn refuses_symlink_escape() {
    let (lib, data, _) = setup();
    let outside = TempDir::new().unwrap();
    std::os::unix::fs::symlink(outside.path(), lib.path().join("ROMS/GBA")).unwrap();
    let mut w = Writer::begin(lib.path(), data.path()).unwrap();
    let err = w.write(&lib.path().join("ROMS/GBA/Imgs/x.png"), b"x", OverwritePolicy::Never).unwrap_err();
    assert!(matches!(err, WriteError::OutsideRoot(_)), "{err}");
    assert!(fs::read_dir(outside.path()).unwrap().next().is_none());
}

#[test]
fn unsafe_names_are_refused() {
    let (lib, data, imgs) = setup();
    let mut w = Writer::begin(lib.path(), data.path()).unwrap();
    assert!(matches!(w.write(&imgs.join("a:b.png"), b"x", OverwritePolicy::Never), Err(WriteError::NameInvalid(_))));
}

#[test]
fn fat_safe_names() {
    assert_eq!(safe_file_name("a:b?.png").unwrap(), "a_b_.png");
    assert_eq!(safe_file_name("CON.png").unwrap(), "_CON.png");
    assert_eq!(safe_file_name("name. ").unwrap(), "name");
    assert_eq!(safe_file_name("\u{7}x").unwrap(), "_x");
    assert_eq!(safe_file_name("Batman & Robin (USA).png").unwrap(), "Batman & Robin (USA).png");
    assert!(safe_file_name("").is_err());
    assert!(safe_file_name("...").is_err());
    let long = format!("{}.png", "é".repeat(200));
    let s = safe_file_name(&long).unwrap();
    assert!(s.len() <= 255 && s.ends_with(".png"), "{}", s.len());
}

#[test]
fn undo_removes_created_and_restores_replaced_files() {
    let (lib, data, imgs) = setup();
    let replaced = imgs.join("Old.png");
    fs::write(&replaced, b"old").unwrap();
    let created = imgs.join("New.png");
    let mut w = Writer::begin(lib.path(), data.path()).unwrap();
    w.write(&created, b"made", OverwritePolicy::Never).unwrap();
    w.write(&replaced, b"newer", OverwritePolicy::Backup).unwrap();
    let job = w.finish().unwrap();
    let report = undo_last(data.path()).unwrap();
    assert_eq!(report.job, job);
    assert_eq!((report.removed.len(), report.restored.len()), (1, 1));
    assert!(!created.exists());
    assert_eq!(fs::read(&replaced).unwrap(), b"old");
    assert!(list_jobs(data.path()).unwrap()[0].undone);
    assert!(matches!(undo_last(data.path()), Err(WriteError::NothingToUndo)));
}

#[test]
fn undo_keeps_files_changed_after_the_job() {
    let (lib, data, imgs) = setup();
    let created = imgs.join("New.png");
    let mut w = Writer::begin(lib.path(), data.path()).unwrap();
    w.write(&created, b"made", OverwritePolicy::Never).unwrap();
    w.finish().unwrap();
    fs::write(&created, b"user edited").unwrap();
    let report = undo_last(data.path()).unwrap();
    assert_eq!(report.skipped_modified, [created.clone()]);
    assert_eq!(fs::read(&created).unwrap(), b"user edited");
}

#[test]
fn an_unfinished_job_can_still_be_undone() {
    let (lib, data, imgs) = setup();
    let created = imgs.join("New.png");
    {
        let mut w = Writer::begin(lib.path(), data.path()).unwrap();
        w.write(&created, b"made", OverwritePolicy::Never).unwrap();
        // dropped without finish(): crash, eject, pause
    }
    assert!(!list_jobs(data.path()).unwrap()[0].complete);
    undo_last(data.path()).unwrap();
    assert!(!created.exists());
}

#[test]
fn prune_keeps_the_newest_jobs() {
    let (lib, data, imgs) = setup();
    for i in 0..3 {
        let mut w = Writer::begin(lib.path(), data.path()).unwrap();
        w.write(&imgs.join(format!("{i}.png")), b"x", OverwritePolicy::Never).unwrap();
        w.finish().unwrap();
    }
    prune_jobs(data.path(), 2).unwrap();
    assert_eq!(list_jobs(data.path()).unwrap().len(), 2);
}
```

(`walkdir` is a regular dependency.)

- [ ] **Step 2: Run to see it fail**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && cargo test -p coverly-core --test writer`
Expected: FAIL to compile.

- [ ] **Step 3: Implement `write/mod.rs`**

```rust
//! Writes into the library (spec §10, §16.10): never outside the root
//! (lexically and through symlinks), FAT/exFAT-safe names, temp file +
//! fsync + rename, an overwrite policy, backups of replaced files in the
//! app data dir, and a JSON-lines journal flushed per entry so a job can be
//! undone even after a crash. Never sets extended attributes and never uses
//! `fs::copy` (it copies xattrs on macOS, which makes `._*` files on FAT).

const RESERVED: [&str; 22] = ["CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9",
                              "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9"];

pub fn safe_file_name(name: &str) -> Result<String, WriteError> {
    let mut s: String = name.chars().map(|c| if c.is_control() || "<>:\"/\\|?*".contains(c) { '_' } else { c }).collect();
    while s.ends_with(' ') || s.ends_with('.') { s.pop(); }
    if s.is_empty() { return Err(WriteError::NameInvalid(name.to_string())); }
    let base = s.split('.').next().unwrap_or("").trim_end().to_ascii_uppercase();
    if RESERVED.contains(&base.as_str()) { s.insert(0, '_'); }
    if s.len() > 255 {
        let (stem, ext) = match s.rfind('.') { Some(i) if s.len() - i <= 10 => (s[..i].to_string(), s[i..].to_string()), _ => (s.clone(), String::new()) };
        let mut cut = 255 - ext.len();
        while !stem.is_char_boundary(cut) { cut -= 1; }
        s = format!("{}{}", &stem[..cut], ext);
    }
    Ok(s)
}

pub fn ensure_within(root: &Path, target: &Path) -> Result<PathBuf, WriteError> {
    let outside = || WriteError::OutsideRoot(target.to_path_buf());
    let rel = target.strip_prefix(root).map_err(|_| outside())?;
    if rel.as_os_str().is_empty() || rel.components().any(|c| !matches!(c, std::path::Component::Normal(_))) {
        return Err(outside());
    }
    let canon_root = root.canonicalize()?;
    let mut probe = target.parent().ok_or_else(outside)?.to_path_buf();
    while fs::symlink_metadata(&probe).is_err() {
        probe = probe.parent().ok_or_else(outside)?.to_path_buf();
    }
    if !probe.canonicalize()?.starts_with(&canon_root) { return Err(outside()); }
    if fs::symlink_metadata(target).is_ok_and(|m| m.file_type().is_symlink()) { return Err(outside()); }
    Ok(target.to_path_buf())
}
```

`Writer` (fields: `root`, `job`, `backups`, `journal: File`, `counter: u32`):
- `begin`: refuse a relative `root` with `OutsideRoot(root)` (callers pass an absolute root; the CLI uses `std::path::absolute`); create `<data>/jobs` and open `<data>/jobs/<job>.jsonl` with `create_new`, write the header line `{"type":"header","job":…,"root":…,"created_at":…}` and `sync_all`.
- `write`: `ensure_within`; refuse when `safe_file_name(name)? != name` (`NameInvalid`); `create_dir_all(parent)` (safe: the parent is under the canonical root); look in the parent for an entry whose name equals the file name ignoring case (`to_lowercase` on both, after NFC); `Never` + found → `SkippedExists`; `Backup` + found → copy its bytes (`fs::read` + `File::create` + `write_all` + `sync_all`) to `backups/<rel>` and replace **that** path (keep the on-disk spelling); write the temp file `.coverly-<job>-<n>.tmp` in the same folder with `OpenOptions::new().write(true).create_new(true)`, `write_all`, `sync_all`; `fs::rename(temp, dest)` (remove the temp on any error); append `{"type":"entry","target":…,"op":{"kind":"created"}|{"kind":"replaced","backup":…},"sha1":…}` and `sync_all`.
- I/O errors map with `fn map_io(e: io::Error, path: &Path) -> WriteError`: `ErrorKind::PermissionDenied` or (unix) raw 30 `EROFS` → `Denied(path)`; (unix) raw 28 `ENOSPC`, (windows) raw 112/39 → `DiskFull`; else `Io(e)`.
- `finish`: append `{"type":"done"}`, `sync_all`, return the job id.

Undo:
- `list_jobs`: a missing `<data>/jobs` folder means no jobs (empty list, not an error); read `<data>/jobs/*.jsonl`; a line that does not parse makes `Journal(..)` only for that job's summary (`complete: false`); `undone` = file name ends with `.undone.jsonl`; sort newest first by job id (ids sort by time).
- `undo_job`: entries in reverse order; for each, `ensure_within(header.root, target)`; current `sha1_hex(fs::read(target))` must equal the entry's sha1, else push to `skipped_modified`; `created` → `fs::remove_file`; `replaced` → read the backup and write it back through the same temp+rename routine; then rename the journal to `<job>.undone.jsonl`.
- `undo_last`: the newest job that is not undone, else `NothingToUndo`.
- `prune_jobs(keep)`: delete journals (and their `backups/<job>` folders) beyond the newest `keep`.

`sha1_hex(bytes)` = `hex_upper(&Sha1::digest(bytes))`.

- [ ] **Step 4: Run to see it pass**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && cargo test -p coverly-core --test writer`
Expected: PASS. Global checks.

- [ ] **Step 5: Commit**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && git add crates/coverly-core && git commit -m "feat(core): write art atomically with backups, journal and undo"
```

---

### Task 12: Apply a plan — art store, downloads, fallbacks, pause and cancel

**Files:**
- Create: `crates/coverly-core/src/jobs/mod.rs`
- Modify: `crates/coverly-core/src/lib.rs` (`pub mod jobs;`)
- Test: `crates/coverly-core/tests/apply.rs`

**Interfaces:**
- Consumes: `Plan`, `PlanItem`, `PlanAction` (Task 9); `ArtSource`, `Candidate`, `SourceError` (Task 6); `process_image`, `OutputSpec`, `Fit`, `ImageLimits` (Task 10); `Writer`, `OverwritePolicy`, `WriteOutcome`, `WriteError`, `sha1_hex` (Task 11); `Cache` (Task 3); `ProgressSink` (Task 7).
- Produces:

```rust
// coverly_core::jobs
pub struct ArtStore<'a>;   // content-addressed downloads: <cache dir>/art/<sha1[0..2]>/<sha1>
impl<'a> ArtStore<'a> {
    pub fn new(cache_dir: &Path, cache: &'a Cache) -> ArtStore<'a>;
    pub fn get(&self, url: &str) -> Option<Vec<u8>>;               // verified by sha1
    pub fn put(&self, url: &str, bytes: &[u8]) -> std::io::Result<()>;
}
#[derive(Clone, Debug)]
pub struct ApplyOptions { pub overwrite: OverwritePolicy, pub include_review: bool, pub concurrency: usize,
                          pub image_limits: ImageLimits, pub fit: Fit, pub jpeg_quality: u8, pub max_alternatives: usize }
impl Default for ApplyOptions   // Never, true, 4, default, Contain, 90, 2
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ItemErrorKind { NoArt, Network, Decode, WriteDenied, DiskFull, NameInvalid, OutsideRoot, Io }
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum ItemStatus { Filled { source: String, name: String }, Replaced { source: String, name: String, backup: PathBuf },
                      SkippedExists, SkippedReview, SkippedNoMatch, NotAttempted,
                      Failed { kind: ItemErrorKind, message: String } }
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ItemResult { pub folder: String, pub rel_path: PathBuf, pub media: Media, pub target: PathBuf,
                        #[serde(flatten)] pub status: ItemStatus }
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct JobReport { pub job_id: Option<String>, pub filled: usize, pub replaced: usize, pub skipped: usize,
                       pub no_art: usize, pub failed: usize, pub items: Vec<ItemResult>,
                       pub paused: Option<String>, pub cancelled: bool }
pub fn apply_plan(plan: &Plan, profile: &Profile, sources: &[&dyn ArtSource], store: &ArtStore, data_dir: &Path,
                  opts: &ApplyOptions, progress: &dyn ProgressSink, cancel: &AtomicBool) -> Result<JobReport, WriteError>;
```

Rules: items are processed in plan order by `concurrency` (1–6) scoped worker threads; the writer is behind a `Mutex`. For each `auto` (and `review` when `include_review`) item: try the chosen candidate then up to `max_alternatives` alternatives — bytes from the `ArtStore` or `source.fetch` (then `store.put`), then `process_image` with `OutputSpec::for_slot(profile.slot(media), fit, quality)`, then `writer.write`. `NotFound` / decode errors move to the next candidate (`no-art` when none worked, `decode` when the last error was a decode error); `RateLimited` **or `Offline`** (spec §10) sets `paused` and stops the queue (remaining items `not-attempted`); `DiskFull` and `Denied` stop the queue too and also set `paused` ("the library volume is full or read-only: …", §10 read-only/ejected volume: stop safely, allow resume); a cancel stops it as well. The journal is always finished (or left for undo) and the job id reported. `skipped` counts `SkippedExists`, `SkippedReview`, `SkippedNoMatch` and `NotAttempted`. No `Writer` (and no journal) is created when nothing is to be written.

- [ ] **Step 1: Write the failing tests**

`crates/coverly-core/tests/apply.rs`:

```rust
mod common;

use std::fs;
use std::sync::atomic::AtomicBool;

use coverly_core::art::resolve_art;
use coverly_core::cache::Cache;
use coverly_core::gather::{gather, select_targets, GatherOptions, Scope, TargetFilter};
use coverly_core::identify::dat::DatStore;
use coverly_core::identify::{IdentifyOptions, Identifier};
use coverly_core::image::probe_dimensions;
use coverly_core::jobs::{apply_plan, ApplyOptions, ArtStore, ItemErrorKind, ItemStatus, JobReport};
use coverly_core::net::testing::{png_bytes, MockResponse, MockServer};
use coverly_core::net::{NetConfig, PoliteHttp, UreqHttp};
use coverly_core::plan::{build_plan, Plan, PlanAction, PlanOptions};
use coverly_core::profiles::Media;
use coverly_core::progress::NoProgress;
use coverly_core::scan::{scan_library, ScanOptions};
use coverly_core::sources::libretro::{LibretroConfig, LibretroSource};
use coverly_core::sources::ArtSource;
use coverly_core::systems::SystemDb;
use coverly_core::write::undo_last;
use tempfile::TempDir;

const GBA: &str = "Nintendo - Game Boy Advance";
const BOX: &str = "Named_Boxarts";

struct World { lib: TempDir, data: TempDir, cache_dir: TempDir, http: PoliteHttp<UreqHttp>, cache: Cache, server: MockServer }

fn world() -> World {
    let lib = TempDir::new().unwrap();
    let r = lib.path();
    common::mkdir(r, "anbernic");
    common::write(r, "ROMS/GBA/001 Golden Sun.gba", "golden-sun-bytes");
    common::write(r, "ROMS/GBA/002 Golden Adventure.gba", "adv");
    common::write(r, "ROMS/GBA/003 Missing Game.gba", "missing");
    common::write(r, "ROMS/GBA/004 Gldn Rsh.gba", "rush");
    let server = MockServer::start();
    server.serve_libretro_root(&[GBA]);
    server.serve_libretro_index(GBA, BOX, &[
        "Golden Sun (USA, Europe).png", "Golden Adventure (USA).png", "Golden Adventure (Japan).png",
        "Missing Game (USA).png", "Golden Rush (USA).png",
    ]);
    let crc = format!("{:08X}", crc32fast::hash(b"golden-sun-bytes"));
    server.serve_dat("metadat/no-intro/Nintendo - Game Boy Advance.dat",
        &format!("game (\n name \"Golden Sun (USA, Europe)\"\n rom ( name \"g.gba\" size 16 crc {crc} )\n)\n"));
    server.serve_libretro_image(GBA, BOX, "Golden Sun (USA, Europe).png", png_bytes(1000, 1400));
    server.serve_libretro_image(GBA, BOX, "Golden Adventure (USA).png", b"<html>oops</html>".to_vec());
    server.serve_libretro_image(GBA, BOX, "Golden Adventure (Japan).png", png_bytes(300, 300));
    server.serve_libretro_image(GBA, BOX, "Golden Rush (USA).png", png_bytes(200, 200));
    let cfg = NetConfig { retries: 0, ..NetConfig::default() };
    World { lib, data: TempDir::new().unwrap(), cache_dir: TempDir::new().unwrap(),
            http: PoliteHttp::new(UreqHttp::new(&cfg), &cfg), cache: Cache::in_memory().unwrap(), server }
}

fn libretro(w: &World) -> LibretroSource<'_> {
    LibretroSource::new(&w.http, &w.cache, LibretroConfig { base: w.server.base(), raw_base: w.server.url("/raw"), ..Default::default() })
}

fn plan(w: &World) -> Plan {
    let profile = common::profile("anbernic-stock");
    let db = SystemDb::builtin();
    let scan = scan_library(w.lib.path(), &profile, &db, &ScanOptions::default());
    let art = resolve_art(&scan, &profile, &db, &[Media::Cover]);
    let targets = select_targets(&art, &TargetFilter { systems: vec![], media: vec![Media::Cover], scope: Scope::Missing });
    let dats = DatStore::new(&w.http, &w.cache, w.server.base());
    let id = Identifier::new(&w.cache, &dats, IdentifyOptions::default());
    let src = libretro(w);
    let sources: Vec<&dyn ArtSource> = vec![&src];
    let g = gather(&targets, &db, &sources, &id, &w.cache, &GatherOptions::default(), &NoProgress, &AtomicBool::new(false));
    build_plan(w.lib.path(), &profile, &db, &g, &PlanOptions::default())
}

fn apply(w: &World, plan: &Plan, opts: &ApplyOptions, cancel: bool) -> JobReport {
    let src = libretro(w);
    let sources: Vec<&dyn ArtSource> = vec![&src];
    let store = ArtStore::new(w.cache_dir.path(), &w.cache);
    apply_plan(plan, &common::profile("anbernic-stock"), &sources, &store, w.data.path(), opts, &NoProgress, &AtomicBool::new(cancel)).unwrap()
}

fn status<'a>(r: &'a JobReport, stem: &str) -> &'a ItemStatus {
    &r.items.iter().find(|i| i.rel_path.to_string_lossy().contains(stem)).unwrap().status
}

fn img(w: &World, stem: &str) -> std::path::PathBuf { w.lib.path().join(format!("ROMS/GBA/Imgs/{stem}.png")) }

#[test]
fn fills_resizes_and_undo_removes() {
    let w = world();
    let p = plan(&w);
    let r = apply(&w, &p, &ApplyOptions { concurrency: 1, ..ApplyOptions::default() }, false);
    assert!(matches!(status(&r, "001"), ItemStatus::Filled { name, .. } if name == "Golden Sun (USA, Europe)"));
    let (wd, ht) = probe_dimensions(&fs::read(img(&w, "001 Golden Sun")).unwrap()).unwrap();
    assert!(wd <= 250 && ht <= 360, "{wd}x{ht}");
    assert!(r.job_id.is_some());
    undo_last(w.data.path()).unwrap();
    assert!(!img(&w, "001 Golden Sun").exists());
}

#[test]
fn bad_image_falls_back_to_next_candidate() {
    let w = world();
    let r = apply(&w, &plan(&w), &ApplyOptions::default(), false);
    assert!(matches!(status(&r, "002"), ItemStatus::Filled { name, .. } if name == "Golden Adventure (Japan)"), "{:?}", status(&r, "002"));
}

#[test]
fn missing_images_on_both_hosts_are_no_art() {
    let w = world();
    let r = apply(&w, &plan(&w), &ApplyOptions::default(), false);
    assert!(matches!(status(&r, "003"), ItemStatus::Failed { kind: ItemErrorKind::NoArt, .. }));
    assert_eq!(r.no_art, 1);
}

#[test]
fn review_items_are_skipped_with_auto_only() {
    let w = world();
    let r = apply(&w, &plan(&w), &ApplyOptions { include_review: false, ..ApplyOptions::default() }, false);
    assert_eq!(status(&r, "004"), &ItemStatus::SkippedReview);
    assert!(!img(&w, "004 Gldn Rsh").exists());
}

#[test]
fn target_created_after_planning_is_skipped() {
    let w = world();
    let p = plan(&w);
    common::write(w.lib.path(), "ROMS/GBA/Imgs/001 Golden Sun.png", "user art");
    let r = apply(&w, &p, &ApplyOptions::default(), false);
    assert_eq!(status(&r, "001"), &ItemStatus::SkippedExists);
    assert_eq!(fs::read_to_string(img(&w, "001 Golden Sun")).unwrap(), "user art");
}

#[test]
fn rate_limit_pauses_and_journal_allows_undo() {
    let w = world();
    let p = plan(&w);
    w.server.mock("/Nintendo%20-%20Game%20Boy%20Advance/Named_Boxarts/Golden%20Adventure%20(USA).png",
                  MockResponse::status(429).header("Retry-After", "0"));
    let r = apply(&w, &p, &ApplyOptions { concurrency: 1, ..ApplyOptions::default() }, false);
    assert!(r.paused.is_some());
    assert!(matches!(status(&r, "001"), ItemStatus::Filled { .. }));
    assert_eq!(status(&r, "004"), &ItemStatus::NotAttempted);
    undo_last(w.data.path()).unwrap();
    assert!(!img(&w, "001 Golden Sun").exists());
}

#[test]
fn going_offline_pauses_the_job() {
    let w = world();
    let mut p = plan(&w);
    for item in &mut p.items {
        if let PlanAction::Auto { chosen, alternatives, .. } | PlanAction::Review { chosen, alternatives, .. } = &mut item.action {
            for c in std::iter::once(chosen).chain(alternatives.iter_mut()) {
                c.url = "http://127.0.0.1:9/gone.png".into();
                c.fallback_url = None;
            }
        }
    }
    let r = apply(&w, &p, &ApplyOptions { concurrency: 1, ..ApplyOptions::default() }, false);
    assert!(r.paused.as_deref().unwrap_or("").contains("offline"), "{:?}", r.paused);
    assert_eq!(r.filled, 0);
    assert!(r.items.iter().all(|i| !matches!(i.status, ItemStatus::Failed { kind: ItemErrorKind::Network, .. })),
            "offline pauses instead of failing items one by one");
}

#[test]
fn cancel_writes_nothing() {
    let w = world();
    let r = apply(&w, &plan(&w), &ApplyOptions::default(), true);
    assert!(r.cancelled);
    assert!(!w.lib.path().join("ROMS/GBA/Imgs").exists());
}

#[test]
fn art_store_avoids_downloading_twice() {
    let w = world();
    let p = plan(&w);
    let path = "/Nintendo%20-%20Game%20Boy%20Advance/Named_Boxarts/Golden%20Sun%20(USA%2C%20Europe).png";
    apply(&w, &p, &ApplyOptions::default(), false);
    undo_last(w.data.path()).unwrap();
    apply(&w, &p, &ApplyOptions::default(), false);
    assert_eq!(w.server.hits(path), 1);
    assert!(img(&w, "001 Golden Sun").exists());
}
```

(Expectations about plan classes: `001` canonical → auto; `002` title-exact USA + Japan → auto (USA chosen, Japan the alternative); `003` title-exact → auto, images 404 on both hosts; `004 Gldn Rsh` → fuzzy ≈ 72 → review.)

- [ ] **Step 2: Run to see it fail**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && cargo test -p coverly-core --test apply`
Expected: FAIL to compile.

- [ ] **Step 3: Implement `jobs/mod.rs`**

```rust
//! Phase two of a fill (spec §7 steps 8–9, §10): apply a plan. Downloads go
//! through a content-addressed art store in the cache dir (re-applying,
//! e.g. after undo or with another profile, does not download again), are
//! re-encoded for the profile slot and written through the safe writer.
//! One failing item never stops the job; a rate limit, a full or read-only
//! volume, or a cancel stop it cleanly with the journal kept.

pub struct ArtStore<'a> { dir: PathBuf, cache: &'a Cache }

impl<'a> ArtStore<'a> {
    pub fn new(cache_dir: &Path, cache: &'a Cache) -> Self { ArtStore { dir: cache_dir.join("art"), cache } }
    fn path_for(&self, sha1: &str) -> PathBuf { self.dir.join(&sha1[..2]).join(sha1) }
    pub fn get(&self, url: &str) -> Option<Vec<u8>> {
        let (sha1, _) = self.cache.art_file(url).ok()??;
        let bytes = fs::read(self.path_for(&sha1)).ok()?;
        (sha1_hex(&bytes) == sha1).then_some(bytes)
    }
    pub fn put(&self, url: &str, bytes: &[u8]) -> io::Result<()> {
        let sha1 = sha1_hex(bytes);
        let path = self.path_for(&sha1);
        if !path.exists() {
            fs::create_dir_all(path.parent().unwrap())?;
            let tmp = path.with_extension(format!("tmp{}", std::process::id()));
            fs::write(&tmp, bytes)?;
            fs::rename(&tmp, &path)?;
        }
        self.cache.put_art_file(url, &sha1, bytes.len() as u64).map_err(io::Error::other)
    }
}

/// Why the queue stopped early: rate limited, offline, or a full / read-only volume.
enum Stop { Paused(String) }

pub fn apply_plan(plan: &Plan, profile: &Profile, sources: &[&dyn ArtSource], store: &ArtStore, data_dir: &Path,
                  opts: &ApplyOptions, progress: &dyn ProgressSink, cancel: &AtomicBool) -> Result<JobReport, WriteError> {
    let todo: Vec<usize> = plan.items.iter().enumerate().filter(|(_, i)| match i.action {
        PlanAction::Auto { .. } => true,
        PlanAction::Review { .. } => opts.include_review,
        PlanAction::None { .. } => false,
    }).map(|(n, _)| n).collect();
    let mut results: Vec<Option<ItemStatus>> = plan.items.iter().map(|i| match i.action {
        PlanAction::Review { .. } if !opts.include_review => Some(ItemStatus::SkippedReview),
        PlanAction::None { .. } => Some(ItemStatus::SkippedNoMatch),
        _ => None,
    }).collect();
    let mut report = JobReport::default();
    if cancel.load(Ordering::SeqCst) {
        report.cancelled = true;
    } else if !todo.is_empty() {
        let writer = Mutex::new(Writer::begin(&plan.root, data_dir)?);
        report.job_id = Some(writer.lock().unwrap().job_id().to_string());
        let next = AtomicUsize::new(0);
        let stop: Mutex<Option<Stop>> = Mutex::new(None);
        let done = Mutex::new(Vec::<(usize, ItemStatus)>::new());
        progress.event(ProgressEvent::Started { phase: Phase::Apply, total: todo.len() });
        std::thread::scope(|s| {
            for _ in 0..opts.concurrency.clamp(1, 6) {
                s.spawn(|| loop {
                    let k = next.fetch_add(1, Ordering::SeqCst);
                    let Some(&n) = todo.get(k) else { break };
                    if cancel.load(Ordering::SeqCst) || stop.lock().unwrap().is_some() {
                        done.lock().unwrap().push((n, ItemStatus::NotAttempted));
                        continue;
                    }
                    let status = apply_item(&plan.items[n], profile, sources, store, &writer, opts, &stop);
                    progress.event(ProgressEvent::Item { phase: Phase::Apply, done: k + 1, total: todo.len(), label: plan.items[n].stem.clone() });
                    done.lock().unwrap().push((n, status));
                });
            }
        });
        for (n, st) in done.into_inner().unwrap() { results[n] = Some(st); }
        let writer = writer.into_inner().unwrap();
        writer.finish()?;
        report.cancelled = cancel.load(Ordering::SeqCst);
        if let Some(Stop::Paused(msg)) = stop.into_inner().unwrap() { report.paused = Some(msg); }
        progress.event(ProgressEvent::Finished { phase: Phase::Apply });
    }
    for (item, st) in plan.items.iter().zip(results) {
        let status = st.unwrap_or(ItemStatus::NotAttempted);
        match &status {
            ItemStatus::Filled { .. } => report.filled += 1,
            ItemStatus::Replaced { .. } => report.replaced += 1,
            ItemStatus::Failed { kind: ItemErrorKind::NoArt, .. } => report.no_art += 1,
            ItemStatus::Failed { .. } => report.failed += 1,
            _ => report.skipped += 1,
        }
        report.items.push(ItemResult { folder: item.folder.clone(), rel_path: item.rel_path.clone(), media: item.media, target: item.target.clone(), status });
    }
    Ok(report)
}
```

`apply_item`: take `(chosen, alternatives)` from the action; for each candidate in `once(chosen).chain(alternatives.iter().take(opts.max_alternatives))`: find the source by `c.source == src.id()` (none → skip); bytes = `store.get(&c.url)` or `src.fetch(c)` — `Err(SourceError::Net(e @ (NetError::RateLimited{..} | NetError::Offline(_))))` → set `stop = Paused(e.to_string())` and return `NotAttempted`; `Err(NotFound)` → `last = NoArt`, continue; other `Err` → `last = Network`, continue; on bytes `let _ = store.put(&c.url, &bytes)`; `process_image(&bytes, &spec, &opts.image_limits)` error → `last = Decode`, continue; then `writer.lock().write(&item.target, &img.bytes, opts.overwrite)`: `Created` → `Filled{source,name}`, `Replaced{backup,..}` → `Replaced{..}`, `SkippedExists` → `SkippedExists`; `Err(DiskFull)` / `Err(Denied)` → set `stop = Paused(format!("the library volume is full or read-only: {e}"))` and return `Failed{DiskFull|WriteDenied}`; `Err(NameInvalid)` → `Failed{NameInvalid}`; `Err(OutsideRoot)` → `Failed{OutsideRoot}`; other → `Failed{Io}`. After the loop return `Failed{kind: last, message}` (`message` = last error text). The spec is `OutputSpec::for_slot(profile.slot(item.media)?, opts.fit, opts.jpeg_quality)`; a missing slot is `Failed{NameInvalid}` (cannot happen for planned items).

- [ ] **Step 4: Run to see it pass**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && cargo test -p coverly-core --test apply`
Expected: PASS. Global checks.

- [ ] **Step 5: Commit**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && git add crates/coverly-core && git commit -m "feat(core): apply fill plans with art store, fallbacks, pause and cancel"
```

---
### Task 13: `coverly fill` and `coverly undo`

**Files:**
- Create: `crates/coverly-cli/src/fill.rs`, `crates/coverly-cli/src/undo.rs`, `crates/coverly-cli/src/profile.rs`
- Modify: `crates/coverly-cli/src/main.rs`, `crates/coverly-cli/src/audit.rs` (move `check_root`, `choose_profile`, `Chosen`, `detection_note`, `profile_ids` to `profile.rs`; make `MediaArg` `pub(crate)`), `crates/coverly-cli/Cargo.toml`
- Test: `crates/coverly-cli/tests/cli_fill.rs`

**Interfaces:**
- Consumes: everything above. Exit codes (extends M1): 0 ok · 1 I/O / cache / no app dirs · 2 bad usage · 3 root not found · 4 no profile detected · **5 paused (offline, rate limited, or the library volume is full / read-only)**.
- Produces (CLI surface, spec §4):

```text
coverly fill <ROOT> [--profile ID|auto] [--system FOLDER|ID[,…]] [--media cover,screenshot,title]
             [--strategy priority|rom-region|popularity] [--regions rom,world,usa,europe,japan]
             [--match hash-then-name|hash-only|name-only] [--scope missing|all]
             [--dry-run | --apply] [--auto-only] [--overwrite never|backup]
             [--report table|json] [--output FILE] [--concurrency 1..6] [--refresh]
             [--cache-dir DIR] [--data-dir DIR]
             (hidden, for tests) [--libretro-url URL] [--libretro-raw-url URL] [--database-url URL]
coverly undo [--job ID] [--list] [--data-dir DIR]
```

Rules: without `--apply` it is a dry run (nothing written, no image downloaded; indexes and DATs are fetched). `--media` defaults to every media the profile can store. `--scope all` requires `--overwrite backup` (exit 2). Unknown regions or a concurrency outside 1–6 → exit 2. `--auto-only` skips `review` items on apply. `--report json` prints `{"mode":"dry-run","plan":…}` or `{"mode":"apply","plan":…,"job":…}`. After an apply, `prune_jobs(data, 10)` keeps the last 10 jobs (spec §9 undo history depth). A paused gather or apply prints the reason and exits 5; re-running continues where it stopped (only missing art is targeted).

- [ ] **Step 1: Write the failing tests**

`crates/coverly-cli/Cargo.toml` `[dev-dependencies]`: `coverly-core = { path = "../coverly-core", features = ["test-util"] }`, `crc32fast = "1.5"` (workspace).

`crates/coverly-cli/tests/cli_fill.rs`:

```rust
use std::fs;

use assert_cmd::Command;
use coverly_core::image::probe_dimensions;
use coverly_core::net::testing::{png_bytes, MockResponse, MockServer};
use predicates::str::contains;
use tempfile::TempDir;

const GBA: &str = "Nintendo - Game Boy Advance";

struct Fixture { lib: TempDir, cache: TempDir, data: TempDir, server: MockServer }

fn fixture() -> Fixture {
    let lib = TempDir::new().unwrap();
    fs::create_dir_all(lib.path().join("anbernic")).unwrap();
    fs::create_dir_all(lib.path().join("ROMS/GBA")).unwrap();
    fs::write(lib.path().join("ROMS/GBA/001 Golden Sun.gba"), b"golden-sun-bytes").unwrap();
    let server = MockServer::start();
    server.serve_libretro_root(&[GBA]);
    server.serve_libretro_index(GBA, "Named_Boxarts", &["Golden Sun (USA, Europe).png", "Golden Sun (Japan).png"]);
    server.serve_libretro_image(GBA, "Named_Boxarts", "Golden Sun (USA, Europe).png", png_bytes(500, 700));
    let crc = format!("{:08X}", crc32fast::hash(b"golden-sun-bytes"));
    server.serve_dat("metadat/no-intro/Nintendo - Game Boy Advance.dat",
        &format!("game (\n name \"Golden Sun (USA, Europe)\"\n rom ( name \"g.gba\" size 16 crc {crc} )\n)\n"));
    Fixture { lib, cache: TempDir::new().unwrap(), data: TempDir::new().unwrap(), server }
}

fn fill(f: &Fixture) -> Command {
    let mut cmd = Command::cargo_bin("coverly").unwrap();
    cmd.arg("fill").arg(f.lib.path())
        .arg("--cache-dir").arg(f.cache.path()).arg("--data-dir").arg(f.data.path())
        .arg("--libretro-url").arg(f.server.base())
        .arg("--libretro-raw-url").arg(f.server.url("/raw"))
        .arg("--database-url").arg(f.server.base());
    cmd
}

fn cover(f: &Fixture) -> std::path::PathBuf { f.lib.path().join("ROMS/GBA/Imgs/001 Golden Sun.png") }

#[test]
fn dry_run_is_the_default_and_writes_nothing() {
    let f = fixture();
    fill(&f).assert().success().stdout(contains("Dry run")).stdout(contains("GBA"));
    assert!(!cover(&f).exists());
    assert_eq!(f.server.hits("/Nintendo%20-%20Game%20Boy%20Advance/Named_Boxarts/Golden%20Sun%20(USA%2C%20Europe).png"), 0);
}

#[test]
fn apply_writes_a_resized_png_and_undo_removes_it() {
    let f = fixture();
    fill(&f).arg("--apply").assert().success().stdout(contains("filled 1"));
    let (w, h) = probe_dimensions(&fs::read(cover(&f)).unwrap()).unwrap();
    assert!(w <= 250 && h <= 360, "{w}x{h}");
    Command::cargo_bin("coverly").unwrap().args(["undo", "--data-dir"]).arg(f.data.path())
        .assert().success().stdout(contains("removed 1"));
    assert!(!cover(&f).exists());
}

#[test]
fn json_report_contains_the_plan() {
    let f = fixture();
    let out = f.cache.path().join("plan.json");
    fill(&f).args(["--report", "json", "--output"]).arg(&out).assert().success();
    let v: serde_json::Value = serde_json::from_str(&fs::read_to_string(&out).unwrap()).unwrap();
    assert_eq!(v["mode"], "dry-run");
    assert_eq!(v["plan"]["totals"]["auto"], 1);
    assert_eq!(v["plan"]["items"][0]["chosen"]["name"], "Golden Sun (USA, Europe)");
}

#[test]
fn scope_all_requires_backups() {
    let f = fixture();
    fill(&f).args(["--scope", "all"]).assert().code(2).stderr(contains("--overwrite backup"));
}

#[test]
fn bad_regions_and_concurrency_are_usage_errors() {
    let f = fixture();
    fill(&f).args(["--regions", "usa,mars"]).assert().code(2).stderr(contains("mars"));
    fill(&f).args(["--concurrency", "9"]).assert().code(2);
}

#[test]
fn rate_limit_exits_5() {
    let f = fixture();
    f.server.mock("/Nintendo%20-%20Game%20Boy%20Advance/Named_Boxarts/", MockResponse::status(429).header("Retry-After", "0"));
    fill(&f).assert().code(5).stderr(contains("rate limited"));
}

#[test]
fn undo_with_no_jobs_fails() {
    let data = TempDir::new().unwrap();
    Command::cargo_bin("coverly").unwrap().args(["undo", "--data-dir"]).arg(data.path())
        .assert().code(1).stderr(contains("nothing to undo"));
}

#[test]
fn fill_never_touches_paths_outside_the_library() {
    let f = fixture();
    fill(&f).arg("--apply").assert().success();
    let outside: Vec<_> = fs::read_dir(f.lib.path()).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert!(outside.iter().all(|n| n == "anbernic" || n == "ROMS"), "{outside:?}");
}
```

The `rate_limit_exits_5` test relies on the CLI building `NetConfig` with `retries` honouring `Retry-After: 0`, so it stays fast.

- [ ] **Step 2: Run to see it fail**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && cargo test -p coverly-cli --test cli_fill`
Expected: FAIL (`fill` subcommand unknown).

- [ ] **Step 3: Implement**

`profile.rs`: the moved helpers, unchanged, `pub(crate)`; `audit.rs` imports them.

`main.rs`: add `mod fill; mod profile; mod undo;`, variants `Fill(fill::FillArgs)` ("Fill missing art from libretro-thumbnails (dry run unless --apply)") and `Undo(undo::UndoArgs)` ("Undo the last fill job"), dispatch to `fill::run` / `undo::run`.

`fill.rs` (outline with exact calls):

```rust
#[derive(Debug, Args)]
pub struct FillArgs {
    /// Library root (e.g. the SD card).
    root: PathBuf,
    #[arg(long, default_value = "auto", value_name = "ID|auto")] profile: String,
    /// System folders or ids to fill, comma-separated (default: all).
    #[arg(long, value_delimiter = ',')] system: Vec<String>,
    /// Media to fill (default: every media the profile can store).
    #[arg(long, value_enum, value_delimiter = ',')] media: Vec<MediaArg>,
    #[arg(long, value_enum, default_value_t = StrategyArg::RomRegion)] strategy: StrategyArg,
    #[arg(long, default_value = "rom,world,usa,europe,japan")] regions: String,
    #[arg(long = "match", value_enum, default_value_t = MatchArg::HashThenName)] match_method: MatchArg,
    #[arg(long, value_enum, default_value_t = ScopeArg::Missing)] scope: ScopeArg,
    /// Show the plan only (the default).
    #[arg(long, conflicts_with = "apply")] dry_run: bool,
    /// Download and write the art.
    #[arg(long)] apply: bool,
    /// With --apply, skip items that need review.
    #[arg(long)] auto_only: bool,
    #[arg(long, value_enum, default_value_t = OverwriteArg::Never)] overwrite: OverwriteArg,
    #[arg(long, value_enum, default_value_t = ReportArg::Table)] report: ReportArg,
    #[arg(long, value_name = "FILE")] output: Option<PathBuf>,
    #[arg(long, default_value_t = 4)] concurrency: usize,
    /// Ignore cached candidates and indexes younger than their TTL.
    #[arg(long)] refresh: bool,
    #[arg(long, value_name = "DIR")] cache_dir: Option<PathBuf>,
    #[arg(long, value_name = "DIR")] data_dir: Option<PathBuf>,
    #[arg(long, hide = true)] libretro_url: Option<String>,
    #[arg(long, hide = true)] libretro_raw_url: Option<String>,
    #[arg(long, hide = true)] database_url: Option<String>,
}

pub fn run(args: &FillArgs) -> Result<(), Failure> {
    check_root(&args.root)?;
    // Absolute (not canonical: no `\\?\` on Windows) so the journal's root and
    // targets stay valid for `coverly undo` from any working directory.
    // Every `args.root` below means this `root`.
    let root = std::path::absolute(&args.root)?;
    if args.scope == ScopeArg::All && args.overwrite != OverwriteArg::Backup {
        return Err(Failure::usage("--scope all replaces existing art and needs --overwrite backup"));
    }
    if !(1..=6).contains(&args.concurrency) { return Err(Failure::usage("--concurrency must be between 1 and 6")); }
    let regions = parse_region_order(&args.regions).map_err(Failure::usage)?;
    let chosen = choose_profile(&args.root, &args.profile)?;
    let profile = &chosen.profile;
    let media: Vec<Media> = /* args.media deduped, or Media::ALL filtered by profile.slot(m).is_some(); none left → usage error */;
    let db = SystemDb::builtin();
    let scan = scan_library(&args.root, profile, &db, &ScanOptions::default());
    let art = resolve_art(&scan, profile, &db, &media);
    let targets = select_targets(&art, &TargetFilter { systems: args.system.clone(), media: media.clone(), scope: args.scope.into() });
    let dirs = AppDirs::resolve(args.cache_dir.as_deref(), args.data_dir.as_deref())
        .ok_or_else(|| Failure { code: 1, message: "cannot find the app cache/data folders; pass --cache-dir and --data-dir".into() })?;
    let cache = Cache::open(&dirs.cache.join(CACHE_FILE)).map_err(|e| Failure { code: 1, message: e.to_string() })?;
    let net = NetConfig { concurrency: args.concurrency, ..NetConfig::default() };
    let http = PoliteHttp::new(UreqHttp::new(&net), &net);
    let dats = DatStore::new(&http, &cache, args.database_url.clone().unwrap_or_else(|| DATABASE_BASE.into()));
    let identifier = Identifier::new(&cache, &dats, IdentifyOptions { method: args.match_method.into(), ..IdentifyOptions::default() });
    let libretro = LibretroSource::new(&http, &cache, LibretroConfig {
        base: args.libretro_url.clone().unwrap_or_else(|| LIBRETRO_BASE.into()),
        raw_base: args.libretro_raw_url.clone().unwrap_or_else(|| LIBRETRO_RAW_BASE.into()),
        ..LibretroConfig::default() });
    let sources: Vec<&dyn ArtSource> = vec![&libretro];
    let strategy: StrategyKind = args.strategy.into();
    let gopts = GatherOptions { consensus: strategy == StrategyKind::Popularity, refresh: args.refresh, ..GatherOptions::default() };
    let cancel = AtomicBool::new(false);
    let progress = StderrProgress::new();   // prints "identify 40/631" lines to stderr only when stderr is a terminal
    let gathered = gather(&targets, &db, &sources, &identifier, &cache, &gopts, &progress, &cancel);
    let plan = build_plan(&args.root, profile, &db, &gathered, &PlanOptions { strategy, regions, ..PlanOptions::default() });
    for w in &plan.warnings { eprintln!("{}", terminal_safe(&format!("warning: {w}"))); }
    if let Some(reason) = &plan.paused {
        emit(args, &chosen, &plan, None)?;
        return Err(Failure { code: 5, message: format!("{reason}; nothing was written. Run the same command later to continue.") });
    }
    if !args.apply {
        return emit(args, &chosen, &plan, None);
    }
    let store = ArtStore::new(&dirs.cache, &cache);
    let aopts = ApplyOptions { overwrite: args.overwrite.into(), include_review: !args.auto_only, concurrency: args.concurrency, ..ApplyOptions::default() };
    let report = apply_plan(&plan, profile, &sources, &store, &dirs.data, &aopts, &progress, &cancel)
        .map_err(|e| Failure { code: 1, message: e.to_string() })?;
    let _ = prune_jobs(&dirs.data, 10);
    emit(args, &chosen, &plan, Some(&report))?;
    match &report.paused {
        Some(reason) => Err(Failure { code: 5, message: format!("{reason}; the job was paused. Run the same command later to continue, or `coverly undo`.") }),
        None => Ok(()),
    }
}
```

Also in `run`: when `profile.id == "batocera"`, print `warning: gamelist.xml is not updated yet; Batocera may not show the new images until it is` to stderr (deviation above). Update the exit-code list in the `main.rs` module doc comment (add `5 paused: offline, rate limited, or the library volume is full / read-only`).

`emit` writes JSON (`{"mode": "dry-run"|"apply", "plan": plan, "job": report?}`) or the table to `--output` or stdout (via `print_stdout`, terminal-safe). Table (dry run):

```text
Profile: anbernic-stock (auto-detected: …)
Dry run: nothing was written. Re-run with --apply to write the art.

FOLDER  AUTO  REVIEW  NONE
GBA        1       0     0
TOTAL      1       0     0

Review (best guess, applied unless --auto-only):
  GBA/002 Goldn Adventure.gba  cover  -> Golden Adventure (USA)  fuzzy 93
No art found:
  GBA/003 Nothing.gba  cover  no candidates
```

(list at most 20 review and 20 none lines unless `--output` is used). Apply adds: `Job <id>: filled N, replaced N, skipped N, no art N, failed N` then each failure (`GBA/… cover: decode: cannot decode image: …`) and `Undo with: coverly undo`. `StrategyArg`, `MatchArg`, `ScopeArg`, `OverwriteArg`, `ReportArg` are `clap::ValueEnum`s with `From` impls into the core enums.

`undo.rs`:

```rust
#[derive(Debug, Args)]
pub struct UndoArgs {
    /// Undo this job instead of the last one (see --list).
    #[arg(long)] job: Option<String>,
    /// List recent jobs instead of undoing.
    #[arg(long)] list: bool,
    #[arg(long, value_name = "DIR")] data_dir: Option<PathBuf>,
}

pub fn run(args: &UndoArgs) -> Result<(), Failure> {
    let dirs = AppDirs::resolve(None, args.data_dir.as_deref())
        .ok_or_else(|| Failure { code: 1, message: "cannot find the app data folder; pass --data-dir".into() })?;
    if args.list {
        // JOB  CREATED  ROOT  FILES  STATE(done|unfinished|undone)
        return print_stdout(&jobs_table(&list_jobs(&dirs.data).map_err(io_failure)?));
    }
    let report = match &args.job { Some(id) => undo_job(&dirs.data, id), None => undo_last(&dirs.data) }
        .map_err(|e| Failure { code: 1, message: e.to_string() })?;
    let mut text = format!("Undid job {}: removed {}, restored {}", report.job, report.removed.len(), report.restored.len());
    if !report.skipped_modified.is_empty() {
        text.push_str(&format!(", kept {} file(s) changed since the job:", report.skipped_modified.len()));
        for p in &report.skipped_modified { text.push_str(&format!("\n  {}", p.display())); }
    }
    text.push('\n');
    print_stdout(&text)
}
```

- [ ] **Step 4: Run to see it pass**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && cargo test -p coverly-cli`
Expected: PASS (new tests and the M1 `cli_audit` tests). Global checks.

- [ ] **Step 5: Commit**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && git add crates/coverly-cli Cargo.lock && git commit -m "feat(cli): add fill and undo commands"
```

---

### Task 14: End-to-end on a copy of the SD, docs and tracker

**Files:**
- Create: `docs/04-architecture/identification.md`, `docs/04-architecture/art-sources.md`, `docs/04-architecture/fill-pipeline.md`, `docs/adr/0009-two-phase-fill-with-sqlite-cache.md`, `docs/adr/0010-safe-writes-backups-and-journal.md`
- Modify: `docs/04-architecture/system-architecture.md`, `docs/adr/README.md`, `docs/adr/0005-libretro-primary-source.md`, `docs/adr/0006-hash-then-name-identification.md`, `docs/adr/0007-pluggable-strategies.md` (Status line only), `docs/README.md`, `docs/TRACKER.md`

Never write under `/Volumes`. All outputs go to the scratchpad. `SC=/private/tmp/claude-501/-Users-miguelangelesparzacalero-Documents-GitHub/35b7c020-f2ae-412f-b757-99c24a9a498b/scratchpad`.

- [ ] **Step 1: Copy two systems (read-only source)**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && SC=/private/tmp/claude-501/-Users-miguelangelesparzacalero-Documents-GitHub/35b7c020-f2ae-412f-b757-99c24a9a498b/scratchpad && \
test -d /Volumes/System/ROMS && rm -rf "$SC/sd-copy" "$SC/e2e" && mkdir -p "$SC/sd-copy/anbernic" "$SC/sd-copy/ROMS/GBA/Imgs" "$SC/sd-copy/ROMS/FC/Imgs" "$SC/e2e" && \
find /Volumes/System/ROMS/GBA -maxdepth 1 -type f ! -name '._*' | sort | head -40 | while IFS= read -r f; do cp -X "$f" "$SC/sd-copy/ROMS/GBA/" || exit 1; done && \
find /Volumes/System/ROMS/FC -maxdepth 1 -type f ! -name '._*' | sort | head -150 | while IFS= read -r f; do cp -X "$f" "$SC/sd-copy/ROMS/FC/" || exit 1; done && \
for s in GBA FC; do find "/Volumes/System/ROMS/$s/Imgs" -maxdepth 1 -type f ! -name '._*' | while IFS= read -r f; do cp -X "$f" "$SC/sd-copy/ROMS/$s/Imgs/" || exit 1; done; done && \
du -sh "$SC/sd-copy" && find "$SC/sd-copy" -type f | wc -l
```

(`cp -X` = no extended attributes. Only `cp` reads from `/Volumes`; nothing is written there. If the SD is not mounted, stop and report.)

- [ ] **Step 2: Baseline audit and snapshot**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && SC=/private/tmp/claude-501/-Users-miguelangelesparzacalero-Documents-GitHub/35b7c020-f2ae-412f-b757-99c24a9a498b/scratchpad && \
cargo build -p coverly-cli --release && B=target/release/coverly && \
$B audit "$SC/sd-copy" --format json --output "$SC/e2e/audit-before.json" && $B audit "$SC/sd-copy" --format table | tail -5 && \
(cd "$SC/sd-copy" && find . -type f -print0 | sort -z | xargs -0 shasum) > "$SC/e2e/snapshot-before.txt"
```

- [ ] **Step 3: Dry run against the real servers (polite: 4 concurrent, 150 ms spacing, cache in the scratchpad)**

First check (read-only HEAD requests) that every explicit `dats = [...]` path in `systems.toml` and the derived no-intro paths of GBA and FC exist in libretro-database; a 404 only degrades to name matching, so a wrong path would pass every test silently. Fix any wrong path in `systems.toml` (and the `dat_sources_per_system` test) before going on:

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && \
grep -o 'metadat/[^"]*\.dat\|dat/[^"]*\.dat' crates/coverly-core/src/systems/systems.toml | sort -u | \
  { cat; echo "metadat/no-intro/Nintendo - Game Boy Advance.dat"; echo "metadat/no-intro/Nintendo - Nintendo Entertainment System.dat"; } | \
  while IFS= read -r p; do printf '%s  %s\n' "$(curl -s -o /dev/null -w '%{http_code}' -I "https://raw.githubusercontent.com/libretro/libretro-database/master/$(python3 -I -c 'import sys,urllib.parse;print(urllib.parse.quote(sys.argv[1]))' "$p")")" "$p"; done
```

Then the dry run:

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && SC=/private/tmp/claude-501/-Users-miguelangelesparzacalero-Documents-GitHub/35b7c020-f2ae-412f-b757-99c24a9a498b/scratchpad && B=target/release/coverly && \
/usr/bin/time -p $B fill "$SC/sd-copy" --system GBA,FC --concurrency 4 --cache-dir "$SC/e2e/cache" --data-dir "$SC/e2e/data" | tee "$SC/e2e/dry-run.txt" && \
$B fill "$SC/sd-copy" --system GBA,FC --cache-dir "$SC/e2e/cache" --data-dir "$SC/e2e/data" --report json --output "$SC/e2e/plan.json" && \
(cd "$SC/sd-copy" && find . -type f -print0 | sort -z | xargs -0 shasum) | diff - "$SC/e2e/snapshot-before.txt" && echo "dry run wrote nothing"
```

Inspect `plan.json`: count items by `identity_method` (`hash` vs `name`) and by action; spot-check 10 `auto` choices (e.g. `001 Golden Sun` → a `Golden Sun (…)` box) and 10 `review` ones. If a systematic mismatch shows up (wrong sequel, wrong game), fix the matching rule with a failing golden test in `names_golden.rs` or `libretro.rs` first, then re-run.

- [ ] **Step 4: Apply (auto only), verify, re-audit**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && SC=/private/tmp/claude-501/-Users-miguelangelesparzacalero-Documents-GitHub/35b7c020-f2ae-412f-b757-99c24a9a498b/scratchpad && B=target/release/coverly && \
/usr/bin/time -p $B fill "$SC/sd-copy" --system GBA,FC --apply --auto-only --concurrency 4 --cache-dir "$SC/e2e/cache" --data-dir "$SC/e2e/data" --report json --output "$SC/e2e/apply.json" && \
python3 -I -c 'import json,sys; j=json.load(open(sys.argv[1]))["job"]; print({k:j[k] for k in ("job_id","filled","replaced","skipped","no_art","failed","paused")})' "$SC/e2e/apply.json" && \
find "$SC/sd-copy/ROMS" -path '*/Imgs/*' -newer "$SC/e2e/snapshot-before.txt" -name '*.png' -print0 | xargs -0 -n1 sips -g pixelWidth -g pixelHeight | awk '/pixelWidth/{w=$2} /pixelHeight/{h=$2; if (w>250||h>360) bad++; n++} END {print n " new PNGs, " bad+0 " over 250x360"}' && \
find "$SC/sd-copy" -name '._*' | wc -l && \
$B audit "$SC/sd-copy" --format json --output "$SC/e2e/audit-after.json" && $B audit "$SC/sd-copy" --format table | tail -5
```

Expected: `filled` > 0, `0 over 250x360`, `0` AppleDouble files, and `totals.missing.cover` in `audit-after.json` = before − filled. `file` on a few new PNGs reports `PNG image data`.

- [ ] **Step 4b: Same apply on a real FAT32 volume (AppleDouble and case-insensitivity check)**

The scratchpad is APFS, where macOS never creates `._*` files, so Step 4's `0` proves nothing about SD cards. Build a small FAT32 disk image in the scratchpad and mount it **at a scratchpad folder** (`-mountpoint`, `-nobrowse`: nothing appears under `/Volumes`):

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && SC=/private/tmp/claude-501/-Users-miguelangelesparzacalero-Documents-GitHub/35b7c020-f2ae-412f-b757-99c24a9a498b/scratchpad && B=target/release/coverly && \
hdiutil create -size 128m -fs "MS-DOS FAT32" -volname COVERLYT -layout MBRSPUD "$SC/e2e/fat.dmg" && \
mkdir -p "$SC/e2e/fat" && hdiutil attach -nobrowse -mountpoint "$SC/e2e/fat" "$SC/e2e/fat.dmg" && \
mkdir -p "$SC/e2e/fat/anbernic" "$SC/e2e/fat/ROMS/GBA/Imgs" && \
find "$SC/sd-copy/ROMS/GBA" -maxdepth 1 -type f | sort | head -15 | while IFS= read -r f; do cp -X "$f" "$SC/e2e/fat/ROMS/GBA/" || exit 1; done && \
$B fill "$SC/e2e/fat" --system GBA --apply --auto-only --cache-dir "$SC/e2e/cache" --data-dir "$SC/e2e/data-fat" | tail -3 && \
echo "AppleDouble files: $(find "$SC/e2e/fat" -name '._*' | wc -l)" && \
$B undo --data-dir "$SC/e2e/data-fat" && \
echo "left after undo: $(find "$SC/e2e/fat/ROMS/GBA/Imgs" -type f | wc -l)" && \
hdiutil detach "$SC/e2e/fat"
```

Expected: `AppleDouble files: 0`, `left after undo: 0`, and no `.coverly-*.tmp` leftovers. If `._*` files appear, the writer is leaking extended attributes (e.g. `com.apple.provenance`): fix it (write through a plain `File`, then remove any xattr with `libc::fremovexattr` on macOS only if needed) with a regression note in `fill-pipeline.md`. Always `hdiutil detach` even after a failure.

- [ ] **Step 5: Undo restores the exact original tree**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && SC=/private/tmp/claude-501/-Users-miguelangelesparzacalero-Documents-GitHub/35b7c020-f2ae-412f-b757-99c24a9a498b/scratchpad && B=target/release/coverly && \
$B undo --data-dir "$SC/e2e/data" && \
(cd "$SC/sd-copy" && find . -type f -print0 | sort -z | xargs -0 shasum) | diff - "$SC/e2e/snapshot-before.txt" && echo "undo restored the original tree"
```

- [ ] **Step 6: Backup path — replace existing GBA covers, then undo**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && SC=/private/tmp/claude-501/-Users-miguelangelesparzacalero-Documents-GitHub/35b7c020-f2ae-412f-b757-99c24a9a498b/scratchpad && B=target/release/coverly && \
$B fill "$SC/sd-copy" --system GBA --scope all --overwrite backup --apply --auto-only --cache-dir "$SC/e2e/cache" --data-dir "$SC/e2e/data" | tail -3 && \
ls "$SC/e2e/data/backups"/*/ROMS/GBA/Imgs | head && \
$B undo --data-dir "$SC/e2e/data" && \
(cd "$SC/sd-copy" && find . -type f -print0 | sort -z | xargs -0 shasum) | diff - "$SC/e2e/snapshot-before.txt" && echo "backups restored byte-for-byte"
```

(This second run downloads nothing it already has: images come from the art store.)

- [ ] **Step 7: Confirm the SD was not touched**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && find /Volumes/System/ROMS/GBA /Volumes/System/ROMS/FC -newer "/private/tmp/claude-501/-Users-miguelangelesparzacalero-Documents-GitHub/35b7c020-f2ae-412f-b757-99c24a9a498b/scratchpad/e2e/snapshot-before.txt" | wc -l
```

Expected: `0`.

- [ ] **Step 8: Write the docs**

- `docs/04-architecture/identification.md` — TL;DR; method setting (`hash-then-name` / `hash-only` / `name-only`); hashing (one pass CRC32+MD5+SHA1, header rules table nes/fds/snes-copier/lynx/a7800 with magic and length, zip central-directory CRC, name-only formats, 64 MiB cap, quick-id memo key); DATs (paths per system, download-on-demand into the cache, 30-day TTL with ETag, 404 remembered, headered + headerless lookup, SHA1 cross-check); arcade (short name → description via `metadat/mame/MAME.dat` and `metadat/fbneo-split/…`); name normalisation pipeline with the golden examples from `names_golden.rs`; guards; numbering prefixes (`001 Golden Sun`, library-level detection); deviation: 7z.
- `docs/04-architecture/art-sources.md` — `ArtSource` trait; libretro: live root, Apache listing parsing and hostile-name rules, TTL 2 days + ETag, sanitising set, match order (canonical → file name → title → fuzzy ≥ 65), URL encoding, raw.githubusercontent fallback and repo renames, global limiter (4–6, 150 ms), retries/backoff/Retry-After, 403/429 → pause; HTTPS-only rule; User-Agent; where M6 sources plug in (`votes`).
- `docs/04-architecture/fill-pipeline.md` — the flow (scan → art → targets → gather → plan → apply → re-audit) with a sequence diagram in a code block; plan classes and thresholds; strategies and D9; image pipeline limits; writer guarantees (root containment, FAT names, temp+rename, case-insensitive skip, backups location, journal format, undo rules incl. changed files); pause/resume semantics; CLI reference and exit codes; E2E evidence summary.
- `docs/adr/0009-two-phase-fill-with-sqlite-cache.md` (Accepted): gather into SQLite + content-addressed art store, apply separately; consequences (offline re-apply, cache can be deleted any time, corrupt cache rebuilt).
- `docs/adr/0010-safe-writes-backups-and-journal.md` (Accepted): atomic writes, backups and journals in the app data dir rather than on the card, undo checks SHA1, never `fs::copy`, no xattrs.
- ADR 0005/0006/0007 Status → `Accepted (implemented in M2)` (0006 adds "; 7z inner CRC deferred — see identification.md"); ADR index gains 0009/0010 and the "implemented" note; `system-architecture.md` moves identify/sources/matching/plan/jobs/image/write/cache/net from "M2+" to built and links the three new docs; `docs/README.md` index links them.

- [ ] **Step 9: Tracker (Spanish)**

In `docs/TRACKER.md`: Estado actual (fecha, fase "M2 terminado en `feat/m2-fill`", siguiente paso "PR de `feat/m2-fill` → CI → merge; después plan M3"); M1-05 ⏸️→✅ with the Task 3 commit; M2-01…M2-09 ✅ each with its commit hashes (`git log --oneline main..HEAD`); a new section «Evidencia M2 — relleno E2E sobre copia de la SD» with: the copy command and sizes, dry-run counts (auto/review/none per system, hash vs name), apply counts and time, `0 over 250x360`, `0` `._*` on the FAT32 image (Step 4b), DAT path check output (Step 3), missing before/after from both audits, undo diff empty, backup round-trip diff empty, `/Volumes` untouched (`0`), number of HTTP requests if logged; add decisions D12 (backups/journal in app data dir), D13 (7z by name for now), D14 (CLI applies review items unless `--auto-only`, per spec §7.7), D15 (offline pauses like 403/429; Popularity consensus = libretro media listings until M6); a new ⬜ row for the Batocera/Knulli `gamelist.xml` merge (deferred from M2); a Bitácora line for the day.

- [ ] **Step 10: Global checks and commits**

Run the three global checks, then:

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && git add docs/04-architecture docs/adr docs/README.md && git commit -m "docs: document identification, art sources and the fill pipeline"
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && git add docs/TRACKER.md && git commit -m "docs(tracker): record M2 completion and end-to-end fill evidence"
```

---

## Self-review

**Spec coverage.**
- §4 v1 CLI `coverly fill` (dry-run/apply/--auto-only, strategies, regions, overwrite, report) → Task 13; `coverly undo` → Tasks 11, 13.
- §5 domain: Identity → Task 7; Candidate → Task 6; Strategy → Task 8; Plan → Task 9; Job + journal → Tasks 11–12.
- §7 flow steps 3–9 → Tasks 7 (select, identify, fetch candidates), 8–9 (choose, plan), 12 (apply), 13–14 (re-scan via audit, report). Step 7 (review UI) is M4; CLI uses `--auto-only`.
- §9 Matching (method, strategy, region order, thresholds, fuzzy, arcade translation, hash cap) → `IdentifyOptions`, `GatherOptions`, `PlanOptions`; Network (concurrency, timeout, retries, proxy, index refresh) → `NetConfig`, `LibretroConfig`; Safety (backup, dry-run default, undo depth, no `._`) → Tasks 11–13. Cache size/clear and the AppleDouble *cleanup* option stay in M5 (the cache can be deleted safely; ADR-0009).
- §10 errors per item (`no-art`, `network`, `decode`, `write-denied`, `disk-full`, `name-invalid`) → `ItemErrorKind`; 403/429 pause + resume → Tasks 4, 7, 12, 13; read-only/ejected volume stops safely → Task 12; FAT names, case-insensitive collisions → Task 11; never outside root, atomic → Task 11.
- §11 HTTPS only, re-encode, no telemetry → Tasks 4, 10.
- §16.1 two-phase → Tasks 7, 12 (ADR-0009); 16.2 hashes/zip/quick-id/case-insensitive ext → Tasks 1, 3, 7; 16.3 method setting → Task 7; 16.4 normalisation → Task 2; 16.5 libretro specifics → Tasks 4, 6; 16.6 arcade → Task 5; 16.7 regions → Tasks 2, 8; 16.9 sources trait (ScreenScraper/TheGamesDB later) → Task 6; 16.10 safety defaults → Tasks 11–13.
- TRACKER M2-01 → Task 1; M2-02 → Task 5; M2-03 → Task 2; M2-04 → Tasks 4, 6; M2-05 → Task 8; M2-06 → Task 9; M2-07 → Task 10; M2-08 → Task 11; M2-09 → Task 13; M1-05 → Task 3. M15 exit criterion → Task 14.

**Type consistency.** `HashOutcome` (Task 1) is stored by `Cache::put_quick_id` (Task 3) and read by `DatStore::lookup` (Task 5) and `Identifier` (Task 7). `DatRow` lives in `cache` and is reused by `dat`. `Candidate`/`MatchKind` (Task 6) flow through `gather` → `matching::Strategy::rank` → `PlanAction` → `apply_plan`. `Region` is `identify::names::Region` everywhere; `RegionPref` wraps it. `OverwritePolicy` is `write::OverwritePolicy` in `ApplyOptions` and the CLI. `ImageFormat` is the profiles enum; the image crate is always `::image`. `IdMethod::Unknown` is produced only for `MatchMethod::HashOnly` misses and makes `ArtQuery::by_name` false.

**Placeholders.** None intended; the docs step (Task 14, Step 8) lists the exact sections each new document must contain.

---

## Execution handoff

Plan complete and saved to `docs/superpowers/plans/2026-10-16-m2-identify-and-fill.md`. Recommended execution: **subagent-driven** — 14 tasks with tightly chained interfaces (each consumes the previous task's types), and a shipped mistake in Tasks 11–12 writes to users' SD cards, so a fresh reviewer per task is worth the cost.
