# Third-party notices

Coverly ROMs is licensed under the GNU Affero General Public License v3.0 (see
`LICENSE`). It links third-party code into its binaries and its webview. Their
licences require their notices to travel with the program, so this file will
ship inside every application bundle, together with `LICENSE` and the licence
texts in `LICENSES/`, not only in the repository.

The notices for Rust crates and npm packages are added here as dependencies are
added, and will be generated from `Cargo.lock` and `pnpm-lock.yaml` before the
first release. `docs/09-legal/license.md` explains the project's own licence.

## Fonts

The application (`src/assets/fonts/`, bundled into every build) self-hosts
three typefaces, each licensed under the SIL Open Font License 1.1 (the
website, in [MAECLY/coverly-roms-landing](https://github.com/MAECLY/coverly-roms-landing), ships the same fonts with
its own notices):

- **Archivo**, Copyright 2020 The Archivo Project Authors
  (https://github.com/Omnibus-Type/Archivo). Licence: `OFL-Archivo.txt`.
- **Atkinson Hyperlegible Next**, Copyright 2020-2024 The Atkinson
  Hyperlegible Next Project Authors
  (https://github.com/googlefonts/atkinson-hyperlegible-next). Licence:
  `OFL-AtkinsonHyperlegibleNext.txt`.
- **Atkinson Hyperlegible Mono**, Copyright 2020-2024 The Atkinson
  Hyperlegible Mono Project Authors
  (https://github.com/googlefonts/atkinson-hyperlegible-next-mono). Licence:
  `OFL-AtkinsonHyperlegibleMono.txt`.

The shipped `.woff2` files are modified versions of the variable fonts from
the google/fonts repository: their axes are limited (Archivo wdth 75-112 and
wght 600-800, Atkinson Hyperlegible Next wght 400-700, Atkinson Hyperlegible
Mono wght 400-600), they are subset to Latin, Latin-1 Supplement and Latin
Extended-A, hinting is removed, and they are converted to WOFF2. They keep
their original family names, which the OFL allows because none of these
fonts declares a Reserved Font Name. Each licence file sits next to the font
files.

## System icons

The small hardware silhouettes shown beside system names are bundled into
every build (`src/components/system/systemIcons.generated.ts`, with reviewable
copies in `src/assets/system-icons/`). They are adapted from:

- **RetroArch Assets**, XMB "Monochrome" theme (`src/xmb/monochrome/`),
  © libretro contributors (https://github.com/libretro/retroarch-assets), at
  commit `d9f969054dc7fbb6fa89519036d2b971e0855b51`. Licence: Creative Commons
  Attribution 4.0 International (CC BY 4.0), full text in
  `LICENSES/CC-BY-4.0.txt` (https://creativecommons.org/licenses/by/4.0/).

Changes made: every fill and stroke other than `none` is recoloured to
`currentColor`; the files are optimised with SVGO (metadata, editor data, ids,
classes and styles removed, coordinates rounded to three decimals); full-canvas
clip paths are removed; viewBoxes are synthesised from `width`/`height` where
the source had none, then cropped to the drawing. The drawn shapes are
otherwise unchanged. Icons in the theme that are a maker's logo or wordmark
are not included. The licence grants no trademark rights; see `TRADEMARKS.md`.
`src/assets/system-icons/README.md` maps each file to its source file and
records its trademark audit.

The 86 files, named by system id: `3do`, `3ds`, `a2600`, `a5200`, `a7800`,
`a800`, `amiga`, `amigacd32`, `apple2`, `arcadia`, `arduboy`, `atarist`,
`atarixe`, `bbcmicro`, `c64`, `cdi`, `colecovision`, `dreamcast`, `fbneo`,
`fds`, `fmtowns`, `gamecom`, `gamecube`, `gamegear`, `gb`, `gba`, `gbc`, `gw`,
`jaguar`, `jaguarcd`, `lynx`, `mastersystem`, `megadrive`, `megaduck`, `msx`,
`msx2`, `n64`, `n64dd`, `nds`, `neocd`, `neogeo`, `nes`, `ngp`, `ngpc`,
`odyssey2`, `pc88`, `pc98`, `pcengine`, `pcenginecd`, `pcfx`, `pet`, `pgm2`,
`pico`, `plus4`, `pokemini`, `ps2`, `ps3`, `psp`, `psvita`, `psx`,
`satellaview`, `saturn`, `scv`, `sega32x`, `segacd`, `sg1000`, `snes`,
`sufami`, `supergrafx`, `supervision`, `switch`, `thomson`, `ti99`, `varcade`,
`vic20`, `virtualboy`, `wii`, `wiiu`, `wonderswan`, `wonderswancolor`, `x1`,
`x68000`, `xbox`, `xbox360`, `zx81`, `zxspectrum`.

## Interface icons

- **lucide-react** 1.59.0, Copyright (c) 2026 Lucide Icons and Contributors
  (https://github.com/lucide-icons/lucide). Licence: ISC; some icons derive
  from Feather, Copyright (c) 2013-present Cole Bemis, under the MIT License.
  Both texts are in `LICENSES/ISC-lucide-react.txt`.

## Archive and compression crates

The core library (`crates/coverly-core`, linked into the app and the `coverly`
CLI) opens archives and compressed ROMs with these Rust crates. All are
permissive; `scripts/check-licenses.sh` checks every crate in `Cargo.lock`
against the allowed licences and refuses the UnRAR crates (RAR archives are
opened through an optional external tool the user installs, never linked).

- **zip** 9.0.3 (zip-rs contributors, https://github.com/zip-rs/zip2). MIT.
- **deflate64** 0.1.12 (anatawa12, https://github.com/anatawa12/deflate64-rs).
  MIT.
- **sevenz-rust2** 0.23.0 (https://github.com/hasenbanck/sevenz-rust).
  Apache-2.0.
- **lzma-rust2** 0.21.0 (https://github.com/hasenbanck/lzma-rust2). Apache-2.0.
- **ppmd-rust** 1.5.0 (https://github.com/hasenbanck/ppmd-rust). CC0-1.0 OR
  MIT-0.
- **ruzstd** 0.9.0 (Moritz Borcherding, https://github.com/KillingSpark/zstd-rs).
  MIT.
- **twox-hash** 2.1.5 (Jake Goulding, https://github.com/shepmaster/twox-hash),
  used by ruzstd and lz4_flex. MIT.
- **lz4_flex** 0.14.0 (Pascal Seitz, Arthur Silva and contributors,
  https://github.com/pseitz/lz4_flex). MIT.
- **tar** 0.4.46 (Alex Crichton and contributors,
  https://github.com/composefs/tar-rs). MIT OR Apache-2.0.
- **bzip2** 0.6.1 (https://github.com/trifectatechfoundation/bzip2-rs). MIT OR
  Apache-2.0.
- **libbz2-rs-sys** 0.2.5, a Rust port of bzip2 1.0.6
  (https://github.com/trifectatechfoundation/libbzip2-rs). bzip2-1.0.6 licence
  (BSD-style), Copyright (C) 1996-2010 Julian R Seward and the libbzip2-rs
  contributors.
- **flate2** 1.1 with the **zlib-rs** backend
  (https://github.com/rust-lang/flate2-rs,
  https://github.com/trifectatechfoundation/zlib-rs). MIT OR Apache-2.0;
  zlib-rs is under the Zlib licence.
- **objc2-foundation** 0.3.2 (https://github.com/madsmtm/objc2), macOS only,
  for moving archives to the Trash. MIT.
- **windows-sys** (https://github.com/microsoft/windows-rs), Windows only,
  already used for file system calls; it also gives the Recycle Bin, process
  Job object and ToolHelp calls of the archive work. MIT OR Apache-2.0.

External archive programs (`unrar`, 7-Zip's `7zz`/`7z`, `bsdtar`) are not
part of Coverly ROMs: they are found on the user's computer, run as separate
processes and never linked or bundled.

## Art and metadata sources

Art sources are fetched at the user's request and are not distributed with
Coverly ROMs: libretro-thumbnails (`thumbnails.libretro.com`), and, when the
user enables them, ScreenScraper, SteamGridDB and TheGamesDB. Name and
identification data from libretro-database is likewise downloaded at runtime,
not bundled. Coverly ROMs ships no ROMs, no BIOS files and no game art. The
game art belongs to each game's developers and publishers; each source's own
terms apply to what the user downloads from it. `docs/09-legal/` documents
those terms.
