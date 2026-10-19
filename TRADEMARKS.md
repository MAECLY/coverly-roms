# TRADEMARKS

**Coverly ROMs**, the two-part wordmark (`Coverly` + `ROMs`), and the **Coverly
mark** belong to MAECLY, the name under which the founder, Miguel Angel Esparza
Calero, publishes his software.

The mark is the Lean-To icon: a solid cover leaning against an empty slot on a
shelf rule. Its master SVGs live in `design/icon/` (see `design/icon/README.md`).

- The code is licensed under the GNU Affero General Public License v3.0 (see
  `LICENSE`). As an additional term under its section 7(e), the licence grants
  no rights under trademark law to these names and marks beyond reasonable and
  customary use in describing where the work came from — for example, "a fork of
  Coverly ROMs".
- Forks and derivatives **must** change the product name, wordmark and mark before
  distributing. The name "Coverly ROMs" may not be used to market a fork as if it
  were the original product. `docs/09-legal/trademark.md` has the full policy.

## What a fork has to replace

The name, identifiers and mark live in these places in this repository (some of
them are created as the app is built; the list names where each one will live):

- `src-tauri/tauri.conf.json` - `productName` and the window `title`
  ("Coverly ROMs"), the bundle `identifier` (`com.maecly.coverlyroms`), and the
  updater `pubkey` and `endpoints`. A fork that keeps Coverly ROMs' public key
  cannot publish updates its own builds will accept.
- `crates/coverly-core/src/paths.rs` - the `ProjectDirs` triple (`com`,
  `Maecly`, `CoverlyROMs`) that names every config, data, cache and log folder.
  A fork that keeps it shares Coverly ROMs' settings and art cache on the same
  machine.
- `package.json` - the npm package name `coverly-roms`.
- `Cargo.toml` files - the crates `coverly-core`, `coverly-cli` (binary
  `coverly`) and the app crate `coverly-roms`.
- `locales/en/ui.json` and `locales/es/ui.json` - `app.name`, the product name
  in the interface copy.
- `design/icon/` - the master SVGs of the mark. `pnpm icons` rasterises them
  into `src-tauri/icons/` (the desktop icon set) and `public/favicon.png`, and,
  with `--site`, the landing icons of
  [MAECLY/coverly-roms-landing](https://github.com/MAECLY/coverly-roms-landing) (`favicon.ico`, `assets/favicon.svg`,
  `assets/apple-touch-icon.png`, `assets/icon-*.png`).
- `index.html` - the page `<title>`.

The repository is `github.com/MAECLY/coverly-roms`.

## Other names

The app and its docs mention products, systems and services it works with or
reads files for. Those names belong to their owners, and their use here only
describes compatibility: Tauri, React, libretro, RetroArch, EmulationStation,
ES-DE, Batocera, Knulli, muOS, Onion, GarlicOS, MinUI, NextUI, Anbernic, Miyoo,
TrimUI, ScreenScraper, SteamGridDB, TheGamesDB, Skraper, Skyscraper, Nintendo,
Pokémon, Sega, Sony, PlayStation, SNK, Neo Geo, Atari, NEC, Bandai,
WonderSwan, Commodore, Capcom, CP System, MSX, IGS, PGM, Lexaloffle, PICO-8,
ScummVM, MAME, FinalBurn Neo, Apple, macOS, Finder, Microsoft, Windows,
Explorer, Linux, GitHub, and similar marks. WonderSwan is a trademark of Bandai, and MSX of MSX
Licensing Corporation (not Microsoft). Game titles, box art and screenshots
belong to their respective publishers and developers.

System icons are simplified silhouettes of compatible hardware, shown next to
the system's name only to identify it. They are not the makers' logos and
imply no endorsement. Licence notices for the third-party code compiled into
the app, and for the icons, are in `THIRD-PARTY-NOTICES.md`.

Copyright © 2026 Miguel Angel Esparza Calero (MAECLY).
