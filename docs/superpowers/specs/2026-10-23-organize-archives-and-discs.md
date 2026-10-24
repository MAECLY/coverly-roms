# Coverly ROMs — Organise: Archives of Every Kind and Disc Games (Design Spec)

> Status: **Draft for review** · Date: 2026-10-23 · Owner: Miguel Angel Esparza Calero (MAECLY)
> Extends [`2026-10-17-organize-loose-roms-design.md`](2026-10-17-organize-loose-roms-design.md) (the "organise spec": §7.2 archives, §7.3 sets, §9 safety, §10 journal) and supplies the archive and disc readers that [`2026-10-18-duplicates-design.md`](2026-10-18-duplicates-design.md) phase B plugs into (`dedupe/source.rs`: `EntryListing`, `DiscProbe`).
> Branch: `feat/organize-disc-archives`. Working tracker: [`docs/TRACKER.md`](../../TRACKER.md).
> This document replaces organise spec §7.2 rows for `.7z`, `.rar` and "others", and amends "Archives are never extracted" (§7.2): disc games in archives and cartridge ROMs in containers the device cannot load are now extracted on apply.

## 1. Purpose and the real case

The owner's download pile `~/Documents/ROMs-Downloaded` (read only for this work) shows today's organiser is wrong for disc games:

| File (as named) | What it really is (`7zz l`, `7zz t` "Everything is Ok") | Today | Wanted |
|---|---|---|---|
| `Next Tetris, The.zip` (355,281,551 B) | **7z**, LZMA:26, 12 blocks (not solid); `Next Tetris, The (USA)/` with `.cue` + 11 `.bin` = 405,749,493 B. Track 01: ISO 9660 `PLAYSTATION`, "Licensed by Sony Computer Entertainment America", `SYSTEM.CNF` `BOOT = cdrom:\SLUS_008.62;1` | *Archivo comprimido dañado* | PS1 (`psx`), extract 12 files to `ROMS/PS/` |
| `Next Tetris, The_ On-Line Edition.zip` (381,421,246 B) | **7z**, LZMA2:24, **solid** (1 block); `.cue` + `.gdi` + 14 tracks = 1,191,067,563 B; track 03 IP.BIN `SEGA SEGAKATANA`, product `T-40214N` | *dañado* | Dreamcast, extract 16 files to `ROMS/DREAMCAST/` |
| `Tetris Plus.zip` (478,731,748 B) | **7z**, LZMA2:24, **solid**; `.cue` + 51 tracks = 591,881,790 B; track 01 IP.BIN `SEGA SEGASATURN`, product `T-5704H` | *dañado* | Saturn, extract 52 files to `ROMS/SATURN/` |
| `3-D Tetris (USA).zip` (5,900 B) | HTML page | *No es una ROM (página web)* | unchanged (correct) |
| cartridge zips (GB/GBA/GBC/SFC, top level and the already-sorted `ROMS/` tree) | real zips | moved as-is / already sorted | unchanged |

Root cause: `organize/inspect.rs::container_for()` decides by **extension first**, so a `.zip` whose magic is 7z goes down the zip path, `ZipArchive::new` fails, and the item is `DamagedArchive`. Even with the right container, 7z is "name and extension only" and nothing is ever extracted, so a disc game in any archive can never be organised.

## 2. Owner decisions

| # | Decision | Consequence in this spec |
|---|---|---|
| D1 | **Detect containers by magic**; extension is only a fallback. An extension/magic mismatch is a **note**, never "damaged". Damaged only when the integrity listing fails. | §3; `ItemNote::ContainerMismatch`; `container_for` rewritten |
| D2 | Support basically every common archive/compressor: zip (zip64; stored/deflate/deflate64/bzip2/lzma/xz/zstd/ppmd), 7z (LZMA/LZMA2/BCJ/PPMd…), RAR v4/v5, tar, gzip (`.gz`/`.tgz`), bzip2, xz, zstd, lz4, and single-file compressed ROMs (`game.gba.gz`). Pure Rust preferred, licences checked against AGPL-3.0-only. RAR: `unrar` crate licence evaluated (rejected, §5.2) → optional external tool on `PATH` (`unrar`/`7zz`/`7z`/`bsdtar`) with a clear "install X to open .rar" note, never failing the whole plan. | §4, §5 |
| D3 | **Cartridge ROMs inside any container** are identified like zipped ones. Moved as-is when the layout's emulators accept that container (zip, usually 7z), else the ROM is extracted. | §6.1, §8.3 |
| D4 | **Disc games in any archive**: identify the system from the disc headers; on apply extract `.cue`/`.gdi` + tracks into the system folder (per-profile layout: flat or per-game subfolder + `.m3u` where the frontend needs it); never rewrite source bytes; free-space and FAT32 4 GiB checks; extract to a temp file in the target folder, then verified no-clobber rename; CRC of every track checked against the archive; one journal entry per file; move mode removes the archive only after everything is verified, copy mode keeps it; resume after interruption; undo removes what was extracted and restores the archive if it was moved. The dry run shows **Extraer** with sizes. | §7, §8, §9 |

Derived decisions (D-numbers continue the tracker's O-D list as **A-D1…**):

- **A-D1** "Removing" an archive in move mode means **sending it to the OS Trash**, never `remove_file`: a 7z cannot be rebuilt byte-identically, so the Trash is what makes "undo restores the archive" true. No Trash on that volume → the archive stays and the item reports *Extraído; el comprimido se queda porque este disco no tiene papelera* (copy semantics), never a hard delete.
- **A-D2** Archive entries are never extracted by an external tool to disk. Tools only **stream to stdout**; Coverly writes every byte itself through the same temp → verify → no-clobber path. A tool can therefore never zip-slip, follow a symlink or overwrite.
- **A-D3** One reader trait for every format (§4.2). The organiser, `coverly organize`, and dedupe phase B (`EntryListing`, `DiscProbe`) all go through it; no caller opens `zip::ZipArchive` directly any more (today `inspect.rs` and `dedupe/source.rs::ZipListing` both do).
- **A-D4** Arcade sets (`fbneo`, `mame`, `cps*`, `neogeo`, `naomi`, `atomiswave`, `hbmame`, `pgm2`) are **never extracted**: the core loads the set by archive stem. A set in a container the core cannot load (RAR, tar.gz) stays with *Repack it as .zip*.
- **A-D5** Archives inside archives are listed, not opened (no recursion). An item whose only payload is a nested archive stays as *Archivo dentro de otro archivo; extráelo antes*.

## 3. Container detection (D1)

### 3.1 Magic table

`identify/sniff.rs::ContainerMagic` grows to the full table; `detect_container(head: &[u8], len: u64) -> Option<ContainerMagic>` reads only the 128 KiB prefix already read by `inspect_file`.

| Format | Magic (offset) | Extra validation before trusting it | Fallback extensions |
|---|---|---|---|
| zip | `PK\x03\x04` (0), `PK\x05\x06` (0, empty), `PK\x07\x08` (0, spanned marker) | none (the reader validates the EOCD) | `zip`, `zipx` |
| zip with prefix (SFX, junk) | EOCD `PK\x05\x06` in the last 64 KiB + 22 | EOCD offsets inside the file | as above |
| 7z | `37 7A BC AF 27 1C` (0) | version major 0 | `7z` |
| RAR 4 | `52 61 72 21 1A 07 00` (0) | — | `rar`, `r00`, `cbr` |
| RAR 5 | `52 61 72 21 1A 07 01 00` (0) | — | `rar` |
| gzip | `1F 8B 08` (0) | FLG reserved bits zero | `gz`, `tgz`, `gzip` |
| bzip2 | `BZh` + `1`–`9` (0) **and** `31 41 59 26 53 59` (4) or `17 72 45 38 50 90` (empty) | both parts (plain text can start `BZh9`) | `bz2`, `tbz`, `tbz2` |
| xz | `FD 37 7A 58 5A 00` (0) | stream-header CRC32 | `xz`, `txz` |
| zstd | `28 B5 2F FD` (0); skippable `5?2A4D18` | frame header descriptor reserved bit zero | `zst`, `tzst` |
| lz4 frame | `04 22 4D 18` (0); legacy `02 21 4C 18` | FLG version bits `01` | `lz4` |
| tar | `ustar\0` + `00` or `ustar  \0` (257) | header checksum (148) valid | `tar` (v7 tar has no magic: extension + checksum only) |
| LZMA alone | — (no magic) | props byte < 225, dict size ≤ cap, sizes plausible | `lzma` only |
| CHD | `MComprHD` (0) | — | `chd` (a disc image, not an archive: unchanged) |
| recognised, not opened | `MSCF` cab, `60 EA` arj, `-lh?-` (2) lzh/lha, `1F 9D` Unix compress, ECM `ECM\0` | — | → `Unsupported { format }` with its name |

### 3.2 Rules

1. Magic wins. `container_for(ext, magic)` becomes `magic.map(Container::from).or_else(|| by_extension(ext))`. The test `extension_decides_before_magic` is replaced by `magic_decides_before_extension` (a 7z named `.zip` → `SevenZip`).
2. Magic ≠ extension and both name archive formats (`.zip` holding 7z, `.rar` holding zip) → `ItemNote::ContainerMismatch { ext: "zip", actual: SevenZip }`, shown as *Es un archivo 7z con extensión .zip* and never a `LeftReason`. The file keeps its name on move (renaming it to `.7z` is a later opt-in, open question Q3) **unless** the profile then cannot load it (§8.3), in which case it is extracted like any other unaccepted container.
3. No magic, archive extension: try the format's reader anyway (an SFX or prefixed zip is found by its EOCD). Text/HTML → `NotARom` (today's behaviour, kept: `3-D Tetris (USA).zip`). Otherwise `DamagedArchive`.
4. **Damaged** only when the format's integrity listing fails (§4.3: zip central directory, 7z header with CRC, RAR listing exit status, tar header walk, compressor frame/stream header). Running out of budget is **never** damaged (§4.4).
5. A raw ROM that is really a zip (`game.bin` with `PK\x03\x04`) keeps today's handling (magic → zip).

## 4. Archive reader architecture

### 4.1 Module layout

```
crates/coverly-core/src/archive/
  mod.rs        public API: open_archive, ArchiveReader, Entry, ArchiveError, Budget, Limits
  detect.rs     detect_container (§3), Format, Compressor
  guard.rs      guarded(): catch_unwind → ArchiveError::Damaged (S1)
  names.rs      entry-path validation: RelPath (zip-slip, absolute, `..`, device names…)
  budget.rs     Budget (decode bytes, decoder memory, wall time), MemoryGate (global)
  verify.rs     VerifyingReader: size + CRC32 at EOF, `take(size + 1)` overrun check
  zip.rs        zip/zip64 over the `zip` crate; LZMA/XZ/PPMd/zstd decoded by us from the raw stream (S2, S3)
  sevenz.rs     bounded 7z header walker (S4, S5) + `sevenz-rust2` for folder decoding
  tar.rs        tar over `tar`, optionally inside a compressor; capped trailer drain (S6)
  single.rs     single-file compressors: gz, bz2, xz, zst, lz4, lzma → one virtual entry
  external.rs   unrar / 7zz / 7z / bsdtar: discovery, strict listing parser (S7), stream reads (S8)
  lzma.rs       LzmaParams, Lzma2Params, xz block walker: one mem formula for check and decoder (S2, S3)
  testing.rs    (feature test-util) fixture builders: crafted headers, garbled archives, fake tools
crates/coverly-core/src/disc/
  mod.rs        DiscReader: identify a disc from a SectorSource; serials, fingerprint
  source.rs     SectorSource over a raw file, an archive entry stream (decoded window), a cue/gdi set
  iso9660.rs    PVD + root directory + file lookup (bounded: depth 1, 64 records)
  sega.rs       IP.BIN parsing (Saturn, Dreamcast, Sega CD)
  sheets.rs     cue/gdi/m3u parsing reused from scan/multifile.rs (made pub(crate) helpers)
```

`organize/inspect.rs` keeps its role (open a candidate, produce an `Inspection`) but delegates every container to `archive::open_archive` and every disc to `disc::DiscReader`. `identify/sniff.rs::discs()` keeps the head-magic checks; the serial and fingerprint reading moves to `disc/`.

### 4.2 One trait over formats

```rust
pub enum Format { Zip, SevenZip, Rar { v5: bool }, Tar { outer: Option<Compressor> }, Single(Compressor) }
pub enum Compressor { Gzip, Bzip2, Xz, Zstd, Lz4, LzmaAlone }

pub enum EntryKind { File, Dir, Symlink, Hardlink, Other }

pub struct Entry {
    pub index: usize,            // position in the archive's own order
    pub name: RelPath,           // validated, '/'-separated, NFC (§13 extras)
    pub raw_name: Vec<u8>,       // bytes as stored, for diagnostics only
    pub kind: EntryKind,
    pub size: u64,               // declared uncompressed size
    pub packed: Option<u64>,
    pub crc32: Option<u32>,      // stored CRC when the format has one
    pub encrypted: bool,
    pub method: Method,          // Stored, Deflate, Lzma{dict}, Ppmd{mem}, …
    pub solid_block: Option<u32>,// 7z folder / RAR solid group; None = independent
    pub mtime: Option<SystemTime>,
}

pub trait ArchiveReader: Send {
    fn format(&self) -> Format;
    /// The validated listing (§4.3). Never empty for an Ok reader.
    fn entries(&self) -> &[Entry];
    /// A reader of one entry's decoded bytes, wrapped in `VerifyingReader`
    /// (errors at EOF when size or CRC differ from the listing). Bytes
    /// decoded to reach it (solid prefix, tar skip) are charged to `budget`.
    fn open_entry<'a>(&'a mut self, index: usize, budget: &'a mut Budget)
        -> Result<Box<dyn Read + 'a>, ArchiveError>;
    /// Streams the `wanted` entries in storage order with one decode pass
    /// (one pass per 7z folder / RAR solid group / tar stream). The
    /// extractor uses this; `open_entry` is for heads and hashes.
    fn stream_entries(&mut self, wanted: &[usize], budget: &mut Budget,
        sink: &mut dyn FnMut(&Entry, &mut dyn Read) -> Result<(), ArchiveError>)
        -> Result<(), ArchiveError>;
}

pub fn open_archive(path: &Path, limits: &Limits) -> Result<Option<Box<dyn ArchiveReader>>, ArchiveError>;
```

`open_archive` returns `Ok(None)` for a file that is no container by magic (and no archive extension); `Err` for a container that cannot be used. Every call into a format backend, including every `Read::read` of a returned stream, runs inside `guard::guarded` (S1).

```rust
pub enum ArchiveError {
    Damaged { detail: String },                       // integrity listing failed, a decoder panicked, CRC/size mismatch
    Unsupported { format: String, need: Option<Tool> },// RAR without a tool, LZH, unknown coder
    Encrypted,                                        // header or every payload encrypted
    NeedsTooMuchMemory { needed: u64, cap: u64 },     // S2, S3
    BudgetSpent { what: BudgetKind },                 // decode bytes, wall time: not damage
    TooManyEntries { count: u64 },
    TooLarge { declared: u64 },
    UnsafeEntry { name: String, why: UnsafeWhy },     // zip-slip, device name, symlink… (§13 extras)
    Io(io::Error),
}
```

Mapping into the plan (`LeftReason`, `organize/plan.rs`): `Damaged` → `DamagedArchive`; `Unsupported` → `UnsupportedArchive { format, need }` (new fields, serde default for old plans); `Encrypted` → **new** `EncryptedArchive`; `NeedsTooMuchMemory` → **new** `ArchiveNeedsMemory { mib }`; `TooManyEntries`/`TooLarge` → `DamagedArchive` (bounded parse, as today's 10,000-entry rule); `UnsafeEntry` → **new** `UnsafeArchiveEntry { name, why }` only when that entry is needed (an unsafe readme next to a good ROM is just not extracted); `BudgetSpent` at plan time → the item goes to **Review** with name/extension evidence and `ItemNote::ArchiveNotFullyRead`, never Left.

### 4.3 Integrity listing per format

| Format | Listing = | Damaged when |
|---|---|---|
| zip | today's `central_directory_claim` precheck + `ZipArchive::new` + `by_index_raw` for every entry | EOCD/CD unreadable, an entry header unreadable, CD claims > 10,000 entries or > 16 MiB |
| 7z | our bounded header walker (S4, S5), header CRC checked | any walker failure (fail closed) |
| RAR | external tool listing, strict parser (S7), exit status 0 | tool exits non-zero on list, parser rejects |
| tar (+ compressor) | header walk with checksum per header, decoded through the compressor up to the listing budget | bad checksum, truncated header, compressor error before the end marker |
| single-file compressor | stream/frame header (+ xz index/footer; gzip FNAME) | header invalid |

A **full integrity test** (decode everything, check every CRC) is not done at plan time; it happens on apply, per extracted file, before anything is committed (§9).

### 4.4 Budgets and limits

`Limits` (constants in `archive/mod.rs`, test-overridable):

| Limit | Value | Why |
|---|---|---|
| `MAX_ENTRIES` | 10,000 | organise's existing zip rule; dedupe's `ZipListing` keeps its own 4,096 policy on top |
| `MAX_HEADER` | 16 MiB | 7z header (encoded and decoded), zip CD, tool listing output |
| `MAX_ENTRY_SIZE` | 16 GiB | larger than any disc (DVD-9 8.5 GB) |
| `MAX_TOTAL_SIZE` | 64 GiB | sum of declared sizes of one archive |
| `DECODER_MEM_CAP` | 256 MiB per decoder | covers 7-Zip Ultra (LZMA2 64 MiB dict ≈ 64 MiB + 1 MiB), PPMd up to 256 MiB, zstd window ≤ 128 MiB (`--long=27`) |
| `MEMORY_GATE` | 512 MiB across threads | a global semaphore; a worker waits for its decoder's memory, so parallel planning never holds more than 512 MiB of decoder state |
| plan head budget | 64 MiB decoded per archive | enough to reach track 03 of the solid Dreamcast archive (3.2 + 2.1 MB in) and read 4 MiB of it |
| plan listing budget (tar in compressor) | 256 MiB decoded | big `.tar.xz` listings stop with `BudgetSpent` → Review |
| tar trailer allowance | 1 MiB after the end marker | S6 |
| external tool | list 30 s / 16 MiB stdout; stream 60 s + 1 s per 10 MiB | S7 |
| wall time per archive at plan time | 20 s | `BudgetSpent { what: Time }` |

`Budget` is a value passed by `&mut` (no globals); every byte a decoder produces, including bytes skipped to reach an entry in a solid block and the trailer drain, is charged. A reader whose budget is spent returns `BudgetSpent`, which callers treat as *not read*, never as *damaged*.

### 4.5 Reader cache and concurrency

Planning opens one archive several times (listing, head of the primary track, dedupe's `head` and `payload_sha1`). `archive::cache` keeps up to 8 open readers keyed by `(path, size, mtime_ns)`; a changed file invalidates its entry. Decoding happens on the existing planner worker pool; `MEMORY_GATE` bounds memory, the per-archive wall time bounds latency.

## 5. Crates and licences

### 5.1 Chosen

All licences below are compatible with distributing Coverly under AGPL-3.0-only (permissive or public-domain-equivalent). `cargo deny check licenses` gains these entries; versions are pinned to a release at least two weeks old when the dependency is added.

| Need | Crate (checked on crates.io 2026-10-23) | Licence | Pure Rust | Notes |
|---|---|---|---|---|
| zip, zip64 | `zip` 9.x (already a dependency) | MIT | yes | features: `deflate-flate2` (+ `flate2/zlib-rs`), `deflate64`, `bzip2`, `lzma`, `xz`, `ppmd`. We **do not** let `zip` decode LZMA/XZ/PPMd/zstd entries: we read them raw (`by_index_raw`) and decode with our capped decoders (S2, S3). MSRV 1.88 |
| deflate64 | `deflate64` 0.1 | MIT | yes | via `zip` |
| bzip2 | `bzip2` 0.6 with `libbz2-rs-sys` backend | MIT OR Apache-2.0; backend `bzip2-1.0.6` (BSD-style) | yes | decoder memory ≤ 3.6 MiB by design |
| LZMA, LZMA2, xz | `lzma-rust2` 0.2x | Apache-2.0 | yes | used by `zip` and `sevenz-rust2`; our own `LzmaParams`/xz block walker drive it with the checked dict (S2, S3) |
| PPMd (zip method 98 = Ppmd8, 7z = Ppmd7) | `ppmd-rust` 1.x | CC0-1.0 OR MIT-0 | yes | allocates the model up front: cap check before construction |
| 7z | `sevenz-rust2` 0.2x | Apache-2.0 | yes | LZMA, LZMA2, BCJ (x86/ARM/ARM64/PPC/SPARC/IA64), BCJ2, Delta, PPMd, BZip2, Deflate, AES (we refuse encrypted). Only ever given archives our walker validated (S5). **MSRV 1.93**: the workspace `rust-version` moves 1.90 → 1.93 (CI already runs 1.99), or the newest release with MSRV ≤ 1.90 is pinned; decided in the plan |
| tar | `tar` 0.4 | MIT OR Apache-2.0 | yes | we never call `unpack`; entries are read as streams |
| gzip | `flate2` 1.1 (already a dependency) | MIT OR Apache-2.0 | yes (zlib-rs) | `MultiGzDecoder` (concatenated members) |
| zstd | `ruzstd` 0.9 | MIT | yes | decoder only; window checked against `DECODER_MEM_CAP` from the frame header before decoding. The C `zstd` crate (BSD-3-Clause) is **not** used: `zip`'s `zstd` feature stays off and zip method 93 goes through `ruzstd` from the raw stream |
| lz4 | `lz4_flex` 0.14 (`frame` feature) | MIT | yes | block max 4 MiB |
| CRC32 | `crc32fast` (already) | MIT OR Apache-2.0 | yes | |

### 5.2 RAR

- **`unrar` / `unrar_sys` crates**: the Rust wrapper is MIT OR Apache-2.0, but `unrar_sys` compiles RARLAB's UnRAR source, whose licence forbids using it to re-create the RAR compression algorithm and is not an open-source licence (FSF and Debian treat UnRAR as non-free). Shipping it inside an AGPL-3.0-only binary would add a non-free, GPL-incompatible component. **Rejected.**
- **Pure-Rust RAR crates** (`rar` 0.4 and similar) are incomplete for RAR5 and solid archives. **Rejected** for now; revisit if one passes a RAR5 + solid + multi-volume test corpus.
- **External tool** (chosen): RAR is opened through a program already on the user's machine, invoked as a separate process (no linking, so no licence interaction). Preference order: `unrar` (≥ 5.0) → `7zz` → `7z` (p7zip with the RAR codec) → `bsdtar` (libarchive ≥ 3.6 for RAR5). macOS always has `/usr/bin/bsdtar`, so most Mac users can open RAR4/RAR5 with nothing installed. Missing or unable tool → `Unsupported { format: "RAR", need: Some(Tool::Unrar) }` for that item only; the plan never fails because of it.
- The same external path is a **fallback** for 7z coders `sevenz-rust2` lacks (e.g. ARM64 filter in older releases, Rar5 coders in 7z): if `7zz`/`7z` is present, such an archive is streamed through it; else `Unsupported { format: "7z (<coder>)", need: Some(Tool::SevenZip) }`.

Discovery (`external.rs`): absolute paths only. Search `PATH` entries that are absolute and are neither the current directory, nor inside the source or target roots; plus fixed locations because a GUI app launched from Finder/Explorer does not inherit the shell `PATH`: macOS `/opt/homebrew/bin`, `/usr/local/bin`, `/usr/bin`; Linux `/usr/bin`, `/usr/local/bin`; Windows `%ProgramFiles%\7-Zip\7z.exe`, `%ProgramFiles%\WinRAR\UnRAR.exe` (+ `%ProgramFiles(x86)%`). Never `CreateProcess` with a bare name (Windows searches the current directory first). Each tool's version banner is parsed once per run; a tool too old for the archive (unrar < 5 with RAR5) is skipped. Settings: `organize.external_tools: "auto" | "off"` (default `auto`), shown in Configuración → Organizar with what was found.

## 6. Identifying what is inside

### 6.1 Cartridges (D3)

Exactly today's zip rules (organise spec §7.2: one ROM-like entry, readme companions, generic-extension "lone" rule, multi-ROM of one system, collections > 8 ROMs, arcade sets by set name + entry CRCs), now for **every** format through `entries()`:

- CRC level: the listing's stored CRC (zip, 7z, RAR via tool). Formats without one (tar, gz without full decode) get the CRC from a bounded decode when the entry is ≤ 64 MiB (all cartridges), else name/extension only.
- Header level: `open_entry(i).take(STREAM_CAP)` → `sniff()` as today.
- Single-file compressors: one virtual entry named from gzip `FNAME` (validated like any entry name) else the archive stem (`game.gba.gz` → `game.gba`). xz/zstd/lz4 frames declare the size when the encoder wrote it; otherwise the bounded decode measures it.

### 6.2 Disc games (D4)

An archive is a **disc archive** when its file entries contain at least one `.cue`, `.gdi`, `.m3u`, `.ccd`, `.iso`, `.img`, `.chd`, `.cdi`, `.pbp` or `.cso`. Then:

1. Parse every `.cue`/`.gdi`/`.m3u` entry ≤ 64 KiB (text, decoded, bounded) with the `scan/multifile.rs` parsers; resolve references **inside the archive's namespace** (relative to the sheet's directory; case-insensitive + NFC fallback, as on disk). A reference that is absolute, contains `..`, or is missing → `SetIncomplete`/`SetOutsideFolder` as for loose sets. A track referenced by two sheets of **different** discs → `SharedTrack`. A `.cue` and a `.gdi` listing the **same** tracks are one disc (Dreamcast dumps ship both, §8.2).
2. One plan item per archive (kind `Set`, `source: Archive`). An archive with several discs of **one** game (`Disc 1`/`Disc 2` sheets, same title stem minus disc tags) is one multi-disc item; discs of different games → `MultiGameArchive { count }`.
3. The primary data track (first `MODE1/MODE2` track of a cue, track 03 of a gdi, the image itself for iso/img) is read through `open_entry` into a `SectorSource` window of at most 4 MiB within the 64 MiB plan head budget, and `DiscReader` identifies it (§7).
4. Evidence: disc header hit (level `Header`, confidence per §7 table) + sheet extension evidence + name signals, combined by `organize/evidence.rs` as for loose sets. A disc whose primary track could not be reached within budget is name/extension only → Review.
5. The serial and volume fingerprint go into the item (`OrganizeItem.disc: Option<DiscId>`) for the UI ("SLUS-00862") and for dedupe level 3.

## 7. Disc identification

`disc::DiscReader::identify(src: &mut dyn SectorSource) -> Option<DiscIdentity>`; sector layout auto-detected (2048 ISO, 2352 raw MODE1/MODE2 with sync `00 FF×10 00`, 2336). All reads bounded (PVD at LBA 16, root directory ≤ 64 records, `SYSTEM.CNF` ≤ 4 KiB, IP.BIN = first 32 KiB of the track).

| System (id) | Where | Signature | Serial | Confidence | Notes |
|---|---|---|---|---|---|
| PlayStation (`psx`) | track 1, PVD (LBA 16) + root `SYSTEM.CNF` | system id `PLAYSTATION`; `SYSTEM.CNF` `BOOT = cdrom:\XXXX_NNN.NN;1`; licence text "Licensed by Sony Computer Entertainment" (raw sector 4) as extra evidence | `BOOT` file name, `SLUS_008.62` → `SLUS-00862` | 0.95 with `BOOT`; 0.80 PVD only | today's `sniff::discs` rule kept |
| PlayStation 2 | same | `SYSTEM.CNF` `BOOT2`, or `PLAYSTATION` on a > CD-size image | `BOOT2` file name | — | `UnsupportedSystem { name: "PlayStation 2" }` (no `ps2` system yet), as today |
| Sega Saturn (`saturn`) | track 1 sector 0 (IP.BIN) | `SEGA SEGASATURN ` at 0x00 (0x10 raw) | product number 0x20, 10 chars (`T-5704H`) | 0.95 | |
| Dreamcast (`dreamcast`) | gdi track 03 / cue HD-area data track, sector 0 | `SEGA SEGAKATANA ` at 0x00 | product number 0x40, 10 chars (`T-40214N`); disc `n/m` at 0x2B | 0.95 | low-density track 01 alone is not enough (no IP.BIN) |
| Sega CD (`segacd`) | track 1 sector 0 | `SEGADISCSYSTEM  ` at 0x00 (0x10 raw); system type `SEGA MEGA DRIVE`/`SEGA GENESIS` at 0x100 | serial at 0x180 (`GM XXXXXXXX-XX`) | 0.95 | must never come out as `megadrive` (existing test) |
| PC Engine CD (`pcenginecd`) | first data track (usually track 02), sector 1 | `PC Engine CD-ROM SYSTEM` at 0x20 | none in the disc | 0.90 | serial from DAT by CRC when cached |
| Neo Geo CD (`neocd`) | track 1 ISO 9660 root | `IPL.TXT` in root, no `SYSTEM.CNF`, no Sega/NEC signature | none | 0.85 | Review unless a DAT/name agrees |
| 3DO | track 1 sector 0 | Opera volume header: `01` + `5A×5` + `01` | none | — | `UnsupportedSystem { name: "3DO" }` (no system in `systems.toml`) |
| PSP (`psp`) | iso/cso | today's `PSP GAME` / `CISO` rules | `UMD_DATA.BIN` / `PARAM.SFO` `DISC_ID` | as today | |

`DiscIdentity { system, serials, disc_no, volume: VolumeFingerprint, source: DiscIdSource }` maps 1:1 onto `dedupe::DiscId` (§10).

## 8. Plan: actions and target layout

### 8.1 New per-item transfer

`OrganizeItem` gains `transfer: Transfer` (serde default `Move`/`Copy` from the plan mode, so old plans parse):

```rust
pub enum Transfer {
    AsIs,                                   // move/copy the file (today)
    Extract { entries: Vec<ExtractEntry>, bytes: u64, archive_after: ArchiveAfter },
}
pub struct ExtractEntry { pub index: usize, pub name: RelPath, pub size: u64, pub crc32: Option<u32>, pub target: PathBuf }
pub enum ArchiveAfter { Keep /* copy mode */, Trash /* move mode, A-D1 */ }
```

The dry run shows the action per item: *Mover*, *Copiar* or **Extraer · 12 archivos · 405,7 MB** with the target folder; the sticky summary adds *Se extraen: {count} juegos ({bytes})*.

### 8.2 Disc layout per profile

New `Profile` field `disc_layout: DiscLayout` (custom profiles may set it in TOML; default `Flat`).

| Profile(s) | `DiscLayout` | Single disc | Multi-disc | Justification |
|---|---|---|---|---|
| `anbernic-stock`, `garlicos`, `onion` | `Flat` | `PS/<cue>` + tracks directly in `PS/` | each disc's sheet flat, no generated `.m3u` | The owner's card kept PS games flat (`PS/Driver 2.img`, `Colony Wars [Disc1of2]`, dedupe spec §1); Onion's PS extension list (`iso, cue, img, mdf, pbp, toc, cbn, m3u, ccd, chd`, layouts research row 4) hides `.bin`, so tracks do not show as games; art stays `PS/Imgs/<stem>.png` with no per-game subfolder. Redump track names embed the title, so flat folders do not collide across games |
| `muos` | `Flat` | flat | flat | muOS ignores art subfolders (`box/` flat, research row 11); content lists by extension |
| `minui` | `GameFolder { m3u_for_multi: true }` | `PS/<stem>/<stem>.cue` + tracks | `PS/<stem>/` with all discs + `<stem>.m3u` named like the folder | MinUI rule (research row 10): a folder named like its `.cue`, or holding an `.m3u` named like the folder, is one game and launches directly |
| `esde` | `GameFolder { m3u_for_multi: true }` | `psx/<stem>/` | `psx/<stem>/` + `<stem>.m3u` | ES-DE lists `.bin` as a PlayStation extension, so flat tracks would show as eleven games; one folder per game keeps the list readable. "Directories interpreted as files" (`<stem>.m3u/`) is a later option, needs a scan change (Q2) |
| `batocera`, `retroarch` | `Flat` | flat | flat + generated `<stem>.m3u` | Batocera/RetroArch take cue/gdi flat; the `.m3u` makes disc swapping work in the core |

Rules for every layout:
- Paths inside the archive are relative to the **sheet's directory**; the archive's top folder (`Next Tetris, The (USA)/`) is dropped in `Flat` and replaced by `<stem>/` in `GameFolder`. Sub-paths a sheet references (`Disc 1/track.bin`) are kept below that.
- `<stem>` is the primary sheet's stem (for the real case, identical to the archive's inner folder).
- Names are never changed for set members (the sheets reference them). A member name that `safe_file_name` would change, or a path over the 259-unit Windows limit, or any member colliding at the target → the whole item stays (`SetUnsafeName { name }` **new**, `PathTooLong` **new**, `CollisionSet`).
- A Dreamcast archive with both `.cue` and `.gdi` for the same tracks: **both are extracted** (no source bytes are dropped, D4). In `Flat` layouts the plan adds `ItemNote::TwinSheets { kept: ["gdi","cue"] }` (*Trae .gdi y .cue de las mismas pistas; algunos menús mostrarán el juego dos veces*). Q1 asks the owner whether to put the redundant `.cue` in a `_extra/` subfolder instead.
- Generated `.m3u` files are new files (journaled `Created`), UTF-8 without BOM, `\n`, one relative sheet path per line. Never written into the source.
- Never rewrite a sheet. If a sheet's `FILE` lines do not resolve after extraction exactly as they did inside the archive, the item is not applied.

### 8.3 Cartridges: as-is or extract (D3)

`Profile` gains `accepts_containers: Vec<Format>` (per profile, with per-system overrides `container_overrides: BTreeMap<system, Vec<Format>>`):

| Profile | zip | 7z | others (rar, tar.*, gz, bz2, xz, zst, lz4) |
|---|---|---|---|
| `anbernic-stock`, `garlicos`, `onion`, `muos`, `batocera`, `esde`, `retroarch` | as-is | as-is | extract |
| `minui` | as-is | extract | extract |
| system overrides (all profiles) | `nds`, `psp`, `n64`: zip as-is only where the research confirms the core reads it; disc systems never "as-is" for archives | | |

Extracting a cartridge: the single ROM entry lands as `<SYS>/<entry base name>`; companions in the archive (readme, `.sav`) are not extracted (they remain in the archive, which in move mode goes to the Trash with the item, and is restorable by undo). Multi-ROM-of-one-system archives that need extraction → each ROM is extracted (note *contiene {n} ROMs*). The per-profile values are confirmed by a research task (R1) before implementation; until confirmed the table above is the default.

### 8.4 Checks at plan time

- **Free space** (§9 of organise spec, extended): sum of extracted bytes + 1 % + the largest single entry (the temp file exists next to the final while being verified only for the entry in flight, so the extra is one entry, not double) must be free on the target volume; `organize.noSpace` with exact numbers.
- **FAT32**: any extracted entry ≥ 4 GiB → whole item `TooBigForFat`. A cartridge archive kept as-is is checked by its own size, as today.
- **Path length / safe names / collisions** per §8.2.
- **Memory**: an archive whose decoder needs more than the cap → `ArchiveNeedsMemory { mib }` (*Necesita {mib} MB de memoria para descomprimirse*).

## 9. Apply, undo, resume

### 9.1 Apply an `Extract` item

For each item, in plan order, after the organise spec's revalidation (archive size + mtime unchanged):

1. `ensure_dir` for the target folder(s) (journaled `DirCreated`).
2. Clean leftover `.coverly-<job>-*.tmp` in those folders (existing `clean_temps`).
3. `stream_entries(wanted)` — one decode pass per 7z folder / solid group / tar stream; an entry already journaled `Extracted` with a target matching its fingerprint (resume) is decoded and discarded, not rewritten (the cost of a solid archive).
4. Per entry, write-ahead: append `{"op":{"kind":"extracted","from":"<archive>","entry":"<rel path>","size":…,"crc32":"…"}}` with `target`, flush, then:
   - `create_new` temp `.coverly-<job>-<n>.tmp` in the **target** folder; copy the stream through `VerifyingReader` (size + CRC vs the listing; for formats without a per-entry CRC, the container's own check: gzip CRC32/ISIZE, xz check, zstd/lz4 content checksum when present; else size only, noted `ItemNote::NoStoredChecksum`); 1 MiB buffer; `sync_all`.
   - **Re-read the temp** and compare CRC32 with the decoded CRC (same as the organise cross-volume move verify: catches a flaky SD before the source goes).
   - `set_modified` from the entry mtime when present; `rename_noclobber(temp, final)`; on collision the **item** fails (sets are never renamed piecemeal) and its already-extracted files are rolled back immediately (deleted after fp check), journaled.
   - Fingerprint taken from the final file after rename (organise spec §9).
5. Generated `.m3u` (if the layout says so): written through the same temp → rename path, journaled `Created`.
6. Only when **every** entry of the item is extracted and verified: if `archive_after == Trash`, append `{"op":{"kind":"archive-trashed","from":"<archive>","fp":{…},"trash":"<location or null>"}}`, flush, then send to the Trash. Trash failure (no Trash on the volume, permissions) → the archive stays, item reported *Extraído; el comprimido se queda* (`ItemStatus::ExtractedKept`), journal entry rewritten as `archive-kept` (a no-op for undo). Copy mode never touches the archive.
7. Cancellation is honoured between entries (a partially extracted item is finished or rolled back before pausing; a solid decode pass is not resumable mid-entry).

Trash implementation: macOS `NSFileManager trashItemAtURL:resultingItemURL:` (via `objc2-foundation`, MIT/Apache) giving the exact location for undo; Windows `IFileOperation` with `FOFX_RECYCLEONDELETE` (location found via `$Recycle.Bin` metadata, else "restore from the Recycle Bin by name"); Linux freedesktop Trash spec (`$XDG_DATA_HOME/Trash` on the home volume, `$topdir/.Trash-$uid` on others) written by our own code (location exact). Shared with dedupe phase B's Papelera option; both use one `write::trash` module.

### 9.2 Undo

Per item (entries carry the item id), in reverse journal order:

1. `archive-trashed`: if the archive is back at `from` already (user restored it) and matches `fp` → fine; else restore it from the recorded Trash location with `rename_noclobber` (cross-volume: stream + verify). If it cannot be restored (Trash emptied, location unknown, `from` occupied by a different file) → **stop undoing this item**: the extracted files are the only copy of the game, so they stay, and the report says *No se pudo recuperar el comprimido de la papelera; los archivos extraídos se quedan*.
2. `extracted` / `created` (`.m3u`): target matches `fp` → delete; modified → `skipped_modified` (kept); missing → nothing. Then job temp files in that folder are cleaned.
3. `dir-created`: removed only if empty (ignoring `.DS_Store`/`._*`), as today.

Undo of a copy-mode extraction only deletes the extracted files.

### 9.3 Resume

Unchanged mechanism (`<job>.plan.json` + `resumable_items`), extended:
- An item with some `extracted` entries: verified targets (fp) are kept, missing/mismatching ones are re-extracted; the archive is trashed only after all are verified (step 6 re-checked from the journal).
- An item whose `archive-trashed` entry exists but the archive is still at `from` (crash between journal and Trash) → trash again.
- An item whose archive changed since the plan (size/mtime) → *changed since the plan*, and its partial extraction is rolled back.

### 9.4 Journal additions (`write/mod.rs` `Op`)

```rust
Extracted { from: PathBuf, entry: String, size: u64, crc32: Option<u32> },
ArchiveTrashed { from: PathBuf, trash: Option<PathBuf> },
ArchiveKept { from: PathBuf },
```
Older Coverly builds see `Unknown` and refuse to undo the job ("not undoable by this version"), as designed in organise spec §10. `JobKind` stays `organize`.

## 10. Interfaces for duplicates phase B

Phase B implements `dedupe/source.rs` on top of this branch:

```rust
/// EntryListing over every format the archive reader opens.
pub struct ArchiveListing { cache: archive::Cache, limits: Limits }

impl EntryListing for ArchiveListing {
    fn list(&self, path) -> io::Result<Option<Vec<ListedEntry>>>
        // open_archive → entries().filter(kind == File) in archive order,
        // ListedEntry { name: name.as_str(), size, crc32 }; > 4096 files → error (dedupe policy).
    fn payload_sha1(&self, path, entry, rule, budget)
        // entry = index into that filtered list; size > budget → Ok(None);
        // open_entry(..).take(size + 1) → hash_stream; VerifyingReader errors on size/CRC mismatch.
    fn head(&self, path, entry, len)
        // open_entry(..).take(len); BudgetSpent → Ok(None) (headerless), never an error.
}

/// DiscProbe over raw images, cue/gdi/m3u sets, and disc archives.
pub struct ArchiveDiscProbe { listing: ArchiveListing }
impl DiscProbe for ArchiveDiscProbe {
    fn probe(&self, path, system) -> io::Result<Option<DiscId>>  // DiscReader → DiscIdentity → DiscId
    fn reads_discs(&self) -> bool { true }
}
```

Design points that make this easy:
- The filtered file-entry order is **stable** (archive order, directories and links removed) and is the same list `list` returns, so the `entry: usize` index is unambiguous.
- `ArchiveError` converts to `io::Error` with `ErrorKind::InvalidData` (damaged, unsafe), `Unsupported` (format/tool), `OutOfMemory` (memory cap) so dedupe's "unreadable unit" path needs no new types; `BudgetSpent` becomes `Ok(None)`.
- `VolumeFingerprint` is computed by `disc::` exactly as dedupe spec §5.4 defines it: PVD volume space size + creation/modification datetimes + SHA1 of the boot file (`SYSTEM.CNF` for PS1/PS2, IP.BIN = first 32 KiB of the primary track for Sega systems, `PSP_GAME/PARAM.SFO` for UMD). Never the outer PBP `PARAM.SFO`.
- Serial normalisation (`SLUS_008.62` → `SLUS-00862`, Sega product numbers trimmed) lives in `disc::serial` and is used by both.
- `dedupe::testing` fakes keep working; `ZipListing` is deleted in phase B (its tests move to `archive/zip.rs`).

## 11. UI and copy

### 11.1 Screens (`src/components/organize/*`, brand primitives in `src/components/ui`)

- **SystemGroups** rows: an action `Badge` per row — *Mover*, *Copiar*, **Extraer** (petrol outline, `Archive` icon) followed by `12 archivos · 405,7 MB` in mono. `SystemIcon` unchanged. A disc row shows the serial in mono (`SLUS-00862`) under the name.
- **Row detail** (expand): the list of files to be extracted with sizes, the target folder, the layout note (flat / per-game folder / `.m3u`), and notes (`ContainerMismatch`, `TwinSheets`, `NoStoredChecksum`).
- **LeftInPlace**: new reasons (below); `UnsupportedArchive` with `need` shows an inline hint with the tool to install and a "Volver a analizar" button (existing re-plan, ⌘R).
- **Summary bar**: *Se extraen: {count} juegos ({bytes})* when any item extracts; the apply button keeps *Organizar {count} archivos* (count = items).
- **Progress**: *Extrayendo {name}: {done} de {total} archivos…* with the existing `<Progress slots={16}>`; byte-based progress for big tracks.
- **Result**: *Extraídos: {extracted}* added to the counts line; per-item *Extraído; el comprimido se queda* when the Trash was not available.
- **Configuración → Organizar**: "Programas externos para RAR" (auto/off) listing the tools found.
- Mock (`src/ipc/mock/mockOrganize.ts`): the downloads fixture gains the three disc archives with the real names, sizes and results of §1, one RAR without a tool, one `.tar.gz` cartridge needing extraction, and progress for extraction; contract fixtures (`contract.requests.json`, `contract.sample.json`) updated.

### 11.2 i18n (es / en), keys under `organize.*`

| Key | es | en |
|---|---|---|
| `organize.action.move` | Mover | Move |
| `organize.action.copy` | Copiar | Copy |
| `organize.action.extract` | Extraer | Extract |
| `organize.extract.detail` | {count} archivos · {bytes} | {count} files · {bytes} |
| `organize.extract.summary` | Se extraen: {count} juegos ({bytes}) | Extracting: {count} games ({bytes}) |
| `organize.extract.into` | Se extraen en {folder} | Extracted into {folder} |
| `organize.extract.layoutFlat` | Las pistas van directamente en {folder} | Tracks go straight into {folder} |
| `organize.extract.layoutFolder` | Cada juego en su carpeta: {folder} | Each game in its own folder: {folder} |
| `organize.extract.m3u` | Se crea {name} para cambiar de disco | {name} is created for disc swapping |
| `organize.extracting` | Extrayendo {name}: {done} de {total} archivos… | Extracting {name}: {done} of {total} files… |
| `organize.result.extracted` | Extraídos: {count} | Extracted: {count} |
| `organize.extract.kept` | Extraído; el comprimido se queda porque este disco no tiene papelera | Extracted; the archive stays because this disk has no Trash |
| `organize.extract.trashed` | El comprimido se ha enviado a la papelera | The archive was moved to the Trash |
| `organize.note.containerMismatch` | Es un archivo {actual} con extensión .{ext} | It is a {actual} archive named .{ext} |
| `organize.note.twinSheets` | Trae .gdi y .cue de las mismas pistas; algunos menús mostrarán el juego dos veces | Has a .gdi and a .cue for the same tracks; some menus will list the game twice |
| `organize.note.noStoredChecksum` | Este formato no guarda suma de comprobación; se comprueba el tamaño | This format stores no checksum; the size is checked |
| `organize.note.notFullyRead` | No se pudo leer entero al simular; revísalo | Couldn't be read in full during the dry run; please review it |
| `organize.why.disc` | Disco de {system}: {detail} ({serial}) | {system} disc: {detail} ({serial}) |
| `organize.left.unsupportedArchiveTool` | Para abrir archivos {format} instala {tool} | To open {format} archives, install {tool} |
| `organize.left.encryptedArchive` | Protegido con contraseña | Password protected |
| `organize.left.archiveNeedsMemory` | Necesita {mib} MB de memoria para descomprimirse | Needs {mib} MB of memory to decompress |
| `organize.left.unsafeArchiveEntry` | Contiene una ruta no segura ({name}); no se extrae | Contains an unsafe path ({name}); not extracted |
| `organize.left.setUnsafeName` | Un archivo del juego tiene un nombre que la tarjeta no admite: {name} | A game file has a name the card can't store: {name} |
| `organize.left.pathTooLong` | La ruta sería demasiado larga para Windows | The path would be too long for Windows |
| `organize.left.nestedArchive` | Archivo dentro de otro archivo; extráelo antes | Archive inside another archive; extract it first |
| `organize.left.repackArcade` | Los juegos de recreativa deben ir en .zip; vuelve a comprimirlo | Arcade sets must be .zip; repack it |
| `organize.undo.archiveLost` | No se pudo recuperar el comprimido de la papelera; los archivos extraídos se quedan | The archive couldn't be recovered from the Trash; the extracted files stay |
| `settings.organize.externalTools` | Programas externos para RAR | External programs for RAR |
| `settings.organize.externalToolsFound` | Encontrado: {tools} | Found: {tools} |
| `settings.organize.externalToolsNone` | Ninguno. Instala unrar o 7-Zip para abrir RAR. | None. Install unrar or 7-Zip to open RAR. |

Tool names are not translated (`unrar`, `7-Zip`). `parity.test.ts` enforces both locales and placeholders.

## 12. CLI, IPC

- `coverly organize` prints the action per item (`extract 12 files 405.7 MB → ROMS/PS/`), honours `--copy` (archive kept) and gains `--no-external-tools`. `--report json` includes `transfer`. `coverly undo` handles the new ops.
- IPC: no new commands; `OrganizeItem.transfer`, `OrganizeItem.disc`, new `LeftReason`/`ItemNote` variants and `OrganizeReport.extracted` are mirrored in `src/ipc/types.ts`, the mock and the contract fixtures. Progress events gain `phase: "extract"` with `bytes_done`/`bytes_total`.

## 13. Security

Every mechanism below has a named test (in `archive/*` unit tests unless stated). Fixtures are generated in tests (crafted bytes), never downloaded. The fake external tools are a small Rust `bin` target behind the `test-util` feature (`coverly-fake-tool`, told by an env var which crafted output to print), so the parser tests run on all three OSes.

| ID | Threat | Mechanism | Tests |
|---|---|---|---|
| **S1** | A third-party parser/decoder panics on hostile input (index out of bounds, arithmetic overflow, `unwrap`) and kills the planner or the app | `guard::guarded(f)` = `catch_unwind(AssertUnwindSafe(f))` around **every** backend entry point: listing, `open_entry`, `stream_entries`, and each `read()` of a returned stream (`GuardedRead`). A caught panic → `ArchiveError::Damaged { detail: "decoder panicked" }`; the reader is poisoned (later calls → `Damaged`) and evicted from the cache. A scoped panic hook suppresses the default stderr print for guarded panics only. The workspace must build with `panic = "unwind"` (a compile-time `const _: () = assert!(cfg!(panic = "unwind"))` in `guard.rs`). | `garbled_archives_never_panic`: for each format fixture (zip deflate/lzma/ppmd/zstd, 7z LZMA/LZMA2/BCJ/PPMd, tar.gz/bz2/xz/zst, gz, lz4), 2,000 deterministic seeded mutations (bit flips, truncations, length-field rewrites) → listing and full decode return `Ok` or `Err`, never unwind out; `panicking_backend_maps_to_damaged` with a fake backend that panics in `read()` |
| **S2** | Zip LZMA/XZ/PPMd (and zstd) entries declare a huge dictionary/model → multi-GB allocation | `zip.rs` never lets `zip` decode methods 14 (LZMA), 95 (XZ), 98 (PPMd), 93 (zstd). At **list** time it reads each such entry's property header from the raw stream (LZMA: 4-byte version/props-size + 5-byte props; PPMd: 2-byte order/mem/restore; XZ: stream header + first block header filter flags; zstd: frame header window) and computes `needed` with `lzma::mem_needed` / PPMd `mem` / zstd window; `needed > DECODER_MEM_CAP` → `NeedsTooMuchMemory`. At **open** the header is parsed **again** from the bytes actually fed to the decoder (no trust in list-time state, the file may have changed) and the decoder is constructed with exactly that value; xz is decoded block by block by our walker, checking each block header before its decoder is built. All decoders also acquire `needed` from `MEMORY_GATE`. | `zip_lzma_huge_dict_refused_at_list_and_open`, `zip_ppmd_256mib_plus_refused`, `zip_xz_second_block_huge_dict_refused` (first block small, second huge), `zip_zstd_window_over_cap_refused`, `entry_changed_between_list_and_open_rechecked` |
| **S3** | Using the attacker-declared unpacked size to shrink the LZMA dictionary makes the cap check and the decoder disagree | `lzma::LzmaParams::from_props(raw)` keeps `dict_raw`; `mem_needed()` uses `dict_raw` only; the decoder is built from the same `LzmaParams` value (`LzmaReader::new_with_props(.., params.dict_raw, ..)` / `new_mem_limit(.., cap_kib, ..)`), never from `min(dict, declared_size)`. Output is bounded by `take(declared + 1)` and `VerifyingReader`, so a smaller real size cannot widen anything. One type, one formula, used by zip, 7z, `.lzma` and xz. | `tiny_declared_size_huge_dict_refused` (declared 1 KiB, dict 1.5 GiB → `NeedsTooMuchMemory`, not decoded); `cap_check_and_decoder_use_same_dict` (asserts the value passed to the decoder equals the checked value, via a recording test hook); `lzma_alone_declared_size_ignored_for_memory` |
| **S4** | 7z encoded header decodes to another encoded header (recursion), or to a header bigger than the caps | `sevenz.rs` walker: next-header size ≤ `MAX_HEADER` and CRC checked; `kEncodedHeader` → parse its `StreamsInfo` with the same bounded parser (coders and dict/model sizes checked against `DECODER_MEM_CAP`, unpack size ≤ `MAX_HEADER`), decode with our capped LZMA/LZMA2 decoder, check the folder CRC, then require the first byte to be `kHeader` (0x01). Any other byte (including a second `kEncodedHeader`) → `Damaged`. `MAX_HEADER`, decoder-memory and `MAX_ENTRIES` apply to the decoded header too. | `nested_encoded_header_is_damaged`, `encoded_header_unpack_size_over_cap`, `encoded_header_dict_over_cap`, `decoded_header_with_too_many_files` |
| **S5** | Passing an unvalidated 7z header to `sevenz-rust2`, which may allocate from declared counts or loop | Fail closed: the walker must fully parse `kHeader` (`MainStreamsInfo`: pack info positions within the file, folders ≤ `MAX_ENTRIES`, coders per folder ≤ 8, bind pairs consistent, unpack sizes summing to substreams; `FilesInfo`: `numFiles ≤ MAX_ENTRIES`, every property's declared size inside the header, names UTF-16 NUL-terminated and exactly `numFiles` of them, empty-stream/empty-file/anti bit vectors sized to `numFiles`, attribute/time vectors sized as declared). Unknown property id, `kAdditionalStreamsInfo`, external flag, inconsistent count → `Damaged`. Every folder's coders and dict sizes are checked before the crate is called. Only then is the crate given **the same open file handle** (re-checked size + mtime), inside `guarded`, with output budgets. Listing comes from our walker, not from the crate. | `sevenz_declared_files_mismatch_names` (`numFiles` 1,000,000 with 1 name → `Damaged` before the crate runs, asserted via a "crate never called" counter), `sevenz_unknown_property_fails_closed`, `sevenz_pack_pos_outside_file`, `sevenz_coder_count_over_cap`, `sevenz_valid_real_shaped_archives_list` (LZMA:26 non-solid 12 blocks, LZMA2:24 solid — the §1 shapes, synthetic content) |
| **S6** | tar inside gz/bz2/xz/zst: after the tar end marker, draining the compressor to EOF (to verify its CRC) can decode gigabytes of trailer | `tar.rs` reads to the end-of-archive marker (two zero blocks), then drains at most `TRAILER_ALLOWANCE` (1 MiB) more, **charged to the decode budget**; more data → `BudgetSpent` at plan time / `Damaged { "trailing data" }` on apply; the outer stream's own check (gzip CRC/ISIZE, xz check) must pass at EOF within the allowance. | `tar_gz_huge_zero_trailer_bounded` (end marker + 4 GiB of zeros compressed to ~4 MB; finishes in < 1 s with `BudgetSpent`/`Damaged`, decoded bytes ≤ allowance + tar size); same for `.tar.xz`, `.tar.zst`, `.tar.bz2`; `tar_gz_normal_padding_ok` (GNU 10 KiB record padding passes) |
| **S7** | External tool listing (`7zz l -slt`, `unrar lt`, `bsdtar -tv`): an entry name containing `\n` forges extra records or fields | Strict parser per tool: (1) force machine output (7-Zip `-slt -ba -sccUTF-8`, unrar `lt -scul`, bsdtar `-tv` whose non-printables libarchive escapes as `\ooo`); (2) every record must contain **exactly** the key set that tool prints for that archive format, each key once, in the tool's order, no unknown or continuation lines; (3) the record count must equal the tool's own count (7-Zip summary `N files` from a second `l` run, unrar's totals line, bsdtar line count) and the sum of sizes must equal the summary total; (4) any name with a control character (U+0000–U+001F, U+007F) or that fails `RelPath` → the whole listing is rejected (`Unsupported { "unsafe names" }`, never partially trusted); (5) stdout ≤ 16 MiB, 30 s timeout, stdin null, `-p-`/no-prompt flags, argv only (no shell), `--` before the archive path. | `forged_newline_name_rejected` (fake `7zz` printing a name `a.bin\nPath = evil` → rejected), `record_key_set_mismatch_rejected`, `record_count_vs_summary_mismatch_rejected`, `size_total_mismatch_rejected`, `listing_over_cap_rejected`, `tool_timeout_killed`; fake tools are test binaries emitting crafted output |
| **S8** | unrar (and bsdtar) treat a name argument as a pattern: wildcards, case-insensitive matches and directory recursion read a different entry than intended | A name is **ambiguous** when it contains `*`, `?`, `[`, `{`, `\`, starts with `-` or `@`, has a case-insensitive (NFC) twin in the listing, or another entry starts with `name + '/'`. Unambiguous names are read with `unrar p -inul -- <archive> <name>`. Ambiguous names (and every bsdtar read) are read **by stream**: the tool prints all files in archive order (`unrar p -inul -- <archive>`, `bsdtar -xOf`), Coverly slices the stream by the listed sizes in listing order and verifies each slice's size and (when listed) CRC32, keeping only the wanted ones; any mismatch → `Damaged`. Every read result is verified against the listing (size + CRC) regardless of path. | `wildcard_name_read_by_stream`, `case_twin_read_by_stream`, `dir_prefix_twin_read_by_stream`, `dash_name_not_an_option`, `stream_slice_crc_mismatch_damaged`, `name_read_crc_verified` (fake `unrar` returning another entry's bytes for a name → `Damaged`) |

### 13.1 Extras

| Threat | Mechanism | Tests |
|---|---|---|
| Zip-slip / absolute paths / `..` | `names::RelPath::parse(raw)` accepts only relative paths: splits on `/` and `\`; rejects empty components after normalisation, `.`/`..`, a leading `/` or `\`, drive prefixes (`C:`), UNC (`\\`), NUL; extraction targets are `target_dir.join(rel)` **and** pass `ensure_within(target_root)` after joining | `zip_slip_dotdot`, `absolute_unix`, `absolute_windows_drive`, `unc_path`, `backslash_dotdot` |
| Symlink / hardlink / device entries | zip external attributes `S_IFLNK`, 7z attribute high word `S_IFLNK`, tar typeflags `1`/`2`/`3`/`4`/`6`, RAR link records → `EntryKind::Symlink/Hardlink/Other`; never extracted, never followed; needed by a set → `UnsafeArchiveEntry` | `zip_symlink_entry_not_extracted`, `tar_symlink_then_file_through_it` (symlink `d → /etc` then `d/passwd`: refused), `tar_hardlink_refused` |
| Windows device and illegal names | `RelPath` rejects components `CON`, `PRN`, `AUX`, `NUL`, `COM0–9`, `LPT0–9` (also with superscript digits and any extension, case-insensitive), trailing dot/space, `:` (ADS), and the characters `<`, `>`, `"`, pipe, `?`, `*`; applied on every OS (the card may go to a PC) | `device_names_rejected` table test |
| Huge entry counts / total size | `MAX_ENTRIES`, `MAX_ENTRY_SIZE`, `MAX_TOTAL_SIZE`, `MAX_HEADER` checked from declared counts **before** allocating per-entry vectors (`Vec::with_capacity` never from an untrusted count) | `zip_claims_one_million_entries`, `sevenz_total_size_over_cap`, `tar_many_tiny_entries_capped` |
| Decompression bombs | Every stream is `take(declared + 1)`; overrun → `Damaged`; disk use bounded by the free-space check over declared sizes | `zip_entry_inflates_past_declared_size` |
| TOCTOU between plan and apply | Revalidate size + mtime; the extractor re-parses headers on open (S2) and verifies every byte by CRC | `archive_replaced_after_plan_is_changed_since_plan` |
| Encrypted entries | Never prompt; `-p-` for tools; encrypted → `Encrypted` or `ItemNote::Encrypted` | `zip_aes_entry_note`, `sevenz_encrypted_header` |
| Windows long paths | Target length checked at plan time (259 UTF-16 units, organise spec §8.2); std's `\\?\` handling is relied on for our own writes | `long_target_left_path_too_long` |
| Windows CI hygiene (Rust 1.99, `clippy -D warnings`) | Unix-only tests and their helpers under `#[cfg(unix)]` modules (no unused imports/helpers on Windows); mtime comparisons tolerate 100 ns ticks (Windows) and 2 s (FAT); snapshot paths normalised to `/` before `insta`; fake tools built as `.exe` on Windows; no `std::os::unix` outside `cfg` | CI on macOS, Ubuntu, Windows; `cargo clippy --workspace --all-targets --all-features -D warnings` on each |

## 14. Testing

- **Unit** (`archive/*`, `disc/*`): magic table one positive + one near-miss per row (`BZh9` text, `ustar` with bad checksum); every format listed and decoded from in-test fixtures (zip methods via `zip` writer; 7z via `sevenz-rust2` writer feature in dev-dependencies only; tar/gz/bz2/xz/zst/lz4 via their encoders in dev-dependencies); every S-test above.
- **Disc** (`disc/*`): synthetic sectors per §7 row (PVD + `SYSTEM.CNF` BOOT/BOOT2, Saturn/DC/Sega CD IP.BIN with product numbers, PCE CD sector-1 string, Neo Geo CD `IPL.TXT`, 3DO Opera header), raw 2352 and 2048 layouts; serial normalisation table; fingerprint equality across bin/iso of the same synthetic disc.
- **Inspect/plan** (`tests/organize_plan.rs`, `insta`): a 7z named `.zip` with a cue + 2 tracks (PS1 synthetic) → `psx`, `Extract`, `ContainerMismatch` note; solid 7z Saturn shape; DC `.cue` + `.gdi` twins → `TwinSheets`; `game.gba.gz` → GBA extract on MinUI, as-is never (gz not accepted); a 7z cartridge → as-is on Anbernic, extract on MinUI; RAR with no tool → `UnsupportedArchive { need: unrar }`, rest of plan unaffected; per-profile disc layout snapshots (flat vs game folder + m3u); FAT32 ≥ 4 GiB track (declared size) → `TooBigForFat`.
- **Apply/undo/resume** (`tests/organize_apply.rs`, tempdirs only): extract + verify + rename; injected CRC mismatch on re-read → item rolled back, archive untouched; collision mid-set → rollback; crash (injected) after 5 of 12 entries → resume finishes, no double files; crash between `archive-trashed` journal and Trash → resume trashes; Trash unavailable (injected) → `ExtractedKept`; undo restores from the (fake) Trash and removes the extracted files; Trash emptied → extracted files kept + message; copy mode never touches the archive; journal from before this change still undoes. The Trash backend is behind a trait with a tempdir fake for tests; the real backends get one `#[ignore]` smoke test per OS.
- **Real case (manual, read-only)**: `#[ignore]` test gated by `COVERLY_SAMPLE_DIR` runs **plan only** on `~/Documents/ROMs-Downloaded` and asserts §1 (three disc archives identified with serials `SLUS-00862`, `T-40214N`, `T-5704H`; Extract with 12/16/52 files and 405,749,493 / 1,191,067,563 / 591,881,790 bytes; `3-D Tetris (USA).zip` not a ROM; cartridge results unchanged). Apply + undo are verified on a copy under `target/` made with `cp -X`, deleted afterwards; never on the original, `~/Documents/ROMS` or `/Volumes/System`.
- **App**: Vitest for the new badges, row detail, left reasons, settings tool list; i18n parity; mock E2E with Chrome DevTools MCP (`isolatedContext: "disc-archives"`, Playwright MCP if the Chrome profile is locked) on port 1455, stopping the server by port only.

## 15. Milestones (input for the implementation plan)

1. `archive/` skeleton: `detect`, `guard`, `names`, `budget`, `verify`, trait + errors; `container_for` by magic; `ContainerMismatch` note (fixes the three §1 items' "dañado" immediately, as name-only).
2. zip backend over the new trait with S2/S3 (raw LZMA/XZ/PPMd/zstd decoding); `inspect.rs` migrated; existing zip tests green.
3. 7z walker (S4, S5) + `sevenz-rust2` decoding; MSRV decision.
4. tar + single-file compressors (S6).
5. External tools: discovery, strict parsers (S7), stream reads (S8); settings.
6. `disc/`: `SectorSource`, ISO 9660, IP.BIN, serials, fingerprints; sniff migrated.
7. Planner: disc archives as set items, `Transfer::Extract`, profile `disc_layout` + `accepts_containers`, plan-time checks, new reasons/notes.
8. Apply/undo/resume for `Extract`, `write::trash`, journal ops.
9. CLI, IPC types, mock, UI, i18n.
10. Real-case verification (read-only plan; apply/undo on a copy), docs, tracker.
R1 (research, before 7): confirm per-profile container acceptance and disc layouts (Onion/GarlicOS/Anbernic stock extension lists for SATURN/DREAMCAST/MDCD/PCECD/NEOCD; MinUI 7z; ES-DE `.bin` listing) with sources in `docs/02-research/layouts.md`.

## 16. Open questions

- **Q1** Dreamcast `.cue` + `.gdi` twins in flat layouts: extract both (default, no source bytes dropped, possible double listing) or put the redundant `.cue` in `<SYS>/_extra/<stem>/`?
- **Q2** ES-DE "directories interpreted as files" (`<stem>.cue/` folders) instead of plain per-game folders: nicer lists, but the Coverly scan must learn it first.
- **Q3** Offer to rename a mismatched cartridge archive to its real extension (`.zip` → `.7z`) on move? Default no (keeps names; emulators read by magic).
- **Q4** Should a moved archive be kept in a Coverly holding folder instead of the OS Trash when the source is a removable card (the card's `.Trashes` fills the card until emptied on that computer)?
