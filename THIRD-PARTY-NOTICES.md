# Third-party notices

Coverly ROMs is licensed under the GNU Affero General Public License v3.0 (see
`LICENSE`). It links third-party code into its binaries and its webview. Their
licences require their notices to travel with the program, so this file ships
inside every application bundle, together with `LICENSE`, the licence texts in
`LICENSES/` and the font licences in `LICENSES/fonts/`, not only in the
repository.

The complete notices, one block per Rust crate and npm package with its full
licence text, are generated from `Cargo.lock` and `pnpm-lock.yaml` into
`LICENSES/generated/rust.txt` and `LICENSES/generated/js.txt` (`pnpm notices`;
`pnpm notices:check` fails when a lock file changed and they were not
regenerated). The app shows them in Settings → Credits → View third-party
licences. This file names the components whose licences ask for something more
than keeping that text.

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
files in the repository and is bundled as `LICENSES/fonts/OFL-*.txt`.

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

The 89 files, named by system id: `3do`, `3ds`, `a2600`, `a5200`, `a7800`,
`a800`, `amiga`, `amigacd32`, `apple2`, `arcadia`, `arduboy`, `atarist`,
`atarixe`, `bbcmicro`, `c64`, `cdi`, `colecovision`, `dreamcast`, `fbneo`,
`fds`, `fmtowns`, `gamecom`, `gamecube`, `gamegear`, `gb`, `gba`, `gbc`, `gw`,
`jaguar`, `jaguarcd`, `lynx`, `mastersystem`, `megadrive`, `megaduck`, `msx`,
`msx2`, `n64`, `n64dd`, `nds`, `neocd`, `neogeo`, `nes`, `ngp`, `ngpc`,
`odyssey2`, `pc88`, `pc98`, `pcengine`, `pcenginecd`, `pcfx`, `pet`, `pgm2`,
`pico`, `plus4`, `pokemini`, `ps2`, `ps3`, `ps4`, `psp`, `psvita`, `psx`,
`satellaview`, `saturn`, `scv`, `sega32x`, `segacd`, `sg1000`, `snes`,
`sufami`, `supergrafx`, `supervision`, `switch`, `thomson`, `ti99`, `varcade`,
`vic20`, `virtualboy`, `wii`, `wiiu`, `wonderswan`, `wonderswancolor`, `x1`,
`x68000`, `xbox`, `xbox360`, `xboxone`, `xboxseries`, `zx81`, `zxspectrum`.

## Interface icons

- **lucide-react** 1.59.0, Copyright (c) 2026 Lucide Icons and Contributors
  (https://github.com/lucide-icons/lucide). Licence: ISC; some icons derive
  from Feather, Copyright (c) 2013-present Cole Bemis, under the MIT License.
  Both texts are in `LICENSES/ISC-lucide-react.txt`.
- **shadcn/ui** (https://github.com/shadcn-ui/ui), Copyright (c) 2023 shadcn.
  The primitives in `src/components/ui/` start from its component code, adapted
  to Coverly's tokens. Licence: MIT.

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

## Other Rust crates with specific terms

All of these are in `LICENSES/generated/rust.txt` with their full texts; they
are listed here because their licences ask for more than a copy of the text, or
because they are the larger parts of the app.

- **wry** 0.57.0 (https://github.com/tauri-apps/wry), the webview layer of
  Tauri. Apache-2.0 OR MIT.
- **reqwest** 0.13.5 (https://github.com/seanmonstar/reqwest), used by Tauri for
  its HTTP and updater code. MIT OR Apache-2.0.
- **brotli** 9.0.0 and **brotli-decompressor** 6.0.1
  (https://github.com/dropbox/rust-brotli,
  https://github.com/dropbox/rust-brotli-decompressor), used by Tauri to
  compress the web assets embedded in the app. BSD-3-Clause AND MIT.
- **encoding_rs** 0.8.44 (Henri Sivonen and contributors,
  https://github.com/hsivonen/encoding_rs), used to read music rip tags
  written in Shift-JIS. (Apache-2.0 OR MIT) AND BSD-3-Clause; its index data
  comes from the WHATWG Encoding Standard, whose BSD-3-Clause notice is
  included in the generated file.
- **option-ext** 0.2.0, used through `directories` for the user folders.
  MPL-2.0. Its source code, unmodified, is available at
  https://github.com/soc/option-ext.
- **selectors** 0.38.0, used through Tauri's HTML handling.
  MPL-2.0. Its source code, unmodified, is available at
  https://github.com/servo/stylo.
- **webpki-roots** 1.0.9 (https://github.com/rustls/webpki-roots), the Mozilla
  root certificates used to check HTTPS connections. CDLA-Permissive-2.0.
- **ring** 0.17.14 (https://github.com/briansmith/ring), the cryptography under
  rustls. Apache-2.0 AND ISC; part of its code comes from BoringSSL, whose
  notices are included in the generated file.
- **ICU4X** crates (`icu_collections`, `icu_normalizer`, `icu_properties` and
  their data crates, 2.3.0, https://github.com/unicode-org/icu4x), used for
  Unicode handling of internationalised domain names. Unicode-3.0.

## Art, metadata and catalogue sources

Nothing in this section is distributed with Coverly ROMs: each source is
contacted at the user's request and what it returns stays on the user's
computer. Coverly ROMs ships no ROMs, no BIOS files, no console keys and no game
art. The game art belongs to each game's developers and publishers; each
source's own terms apply to what the user downloads from it.

- **libretro-thumbnails** (`thumbnails.libretro.com`,
  https://github.com/libretro-thumbnails/libretro-thumbnails): the default art
  source. No licence is published for the art.
- **ScreenScraper** (https://www.screenscraper.fr/), when the user enables it
  with their own account. Data and media from ScreenScraper (screenscraper.fr),
  CC BY-NC-SA 4.0.
- **SteamGridDB** (https://www.steamgriddb.com/), when the user enables it
  with their own API key.
- **libretro-database** (https://github.com/libretro/libretro-database),
  downloaded to the user's cache for identification. Identification data:
  libretro-database, © libretro contributors, CC BY-SA 4.0
  (https://creativecommons.org/licenses/by-sa/4.0/).
- **GameTDB** (https://www.gametdb.com/): the GameCube, Wii, Wii U and PS3 IDs
  that libretro-database carries come from it. Game data from GameTDB
  (https://www.gametdb.com/), where you can add new game entries and artwork.
- **titledb** (https://github.com/blawar/titledb), an optional download the
  user turns on for Switch names. MIT License, Copyright (c) 2019 Blake Warner.
  Coverly keeps only the title ID, name, publisher, release date and region of
  the downloaded file; game names and artwork are © their respective owners.
- **api.nlib.cc** (nlib-api, https://github.com/ghost-land/nlib-api), a
  GPL-3.0 service contacted only when the user turns on the online fallback for
  Switch names. Nothing of its code is shipped with Coverly ROMs.
- **Nintendo eShop icons** (`img-eshop.cdn.nintendo.net`), © Nintendo, fetched
  only when the user turns Switch icons on, kept in the user's local cache and
  never redistributed.

The full list, with what each source and reference is used for and the exact
credit each asks for, is in `credits/credits.json`, shown in the app under
Settings → Credits and in the README.
