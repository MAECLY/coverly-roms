# Third-party notices

Coverly ROMs is licensed under the GNU Affero General Public License v3.0 (see
`LICENSE`). It links third-party code into its binaries and its webview. Their
licences require their notices to travel with the program, so this file will
ship inside every application bundle, together with `LICENSE` and the licence
texts in `LICENSES/`, not only in the repository.

The notices for Rust crates and npm packages are added here as dependencies are
added, and will be generated from `Cargo.lock` and `pnpm-lock.yaml` before the
first release. `docs/09-legal/license.md` explains the project's own licence.

## Landing page

The website in `site/` (not part of the application bundles) self-hosts one
font file, `site/assets/fonts/archivo-display-800.woff2`: Archivo, Copyright
2020 The Archivo Project Authors (https://github.com/Omnibus-Type/Archivo),
licensed under the SIL Open Font License 1.1. It is a modified version
(instanced at width 72 and weight 800 and subset to Latin-1), served under
the family name "Archivo Display". The full licence is in
`site/assets/fonts/OFL.txt`.

## Art and metadata sources

Art sources are fetched at the user's request and are not distributed with
Coverly ROMs: libretro-thumbnails (`thumbnails.libretro.com`), and, when the
user enables them, ScreenScraper, SteamGridDB and TheGamesDB. Name and
identification data from libretro-database is likewise downloaded at runtime,
not bundled. Coverly ROMs ships no ROMs, no BIOS files and no game art. The
game art belongs to each game's developers and publishers; each source's own
terms apply to what the user downloads from it. `docs/09-legal/` documents
those terms.
