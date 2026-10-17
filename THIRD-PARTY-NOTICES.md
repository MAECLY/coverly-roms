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

The application (`src/assets/fonts/`, bundled into every build) and the
website (`site/assets/fonts/`, not part of the application bundles) self-host
the same three typefaces, each licensed under the SIL Open Font License 1.1:

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
files in both folders.

## Art and metadata sources

Art sources are fetched at the user's request and are not distributed with
Coverly ROMs: libretro-thumbnails (`thumbnails.libretro.com`), and, when the
user enables them, ScreenScraper, SteamGridDB and TheGamesDB. Name and
identification data from libretro-database is likewise downloaded at runtime,
not bundled. Coverly ROMs ships no ROMs, no BIOS files and no game art. The
game art belongs to each game's developers and publishers; each source's own
terms apply to what the user downloads from it. `docs/09-legal/` documents
those terms.
