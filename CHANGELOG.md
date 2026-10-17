# Changelog

All notable changes to Coverly ROMs are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html). Commits follow
[Conventional Commits](https://www.conventionalcommits.org/); a release moves
the notes below from `[Unreleased]` to a version heading (see RELEASING.md).

## [Unreleased]

### Added

- Release pipeline (M7): signed builds for macOS (universal `.dmg`), Windows
  (`.msi` and NSIS) and Linux (`.AppImage`, `.deb`), a `latest.json` update
  manifest with stable and beta channels, and `SHA256SUMS`.
- In-app updates (M7): check on start (optional) and from Settings → About,
  download with progress, signature check before installing, restart when no
  fill job is running. `coverly update check|install` for the CLI.
- Secondary sources (M6): SteamGridDB covers with community votes behind your
  own API key, the Popularity strategy (votes, then cross-source consensus),
  "why this image" for every choice; ScreenScraper prepared for a later version.
- Settings (M5): every group of the spec (General, Library, Art, Matching,
  Sources, Network, Safety, Advanced), custom layout profiles with a live
  preview, source keys in the OS keychain, search by title with manual
  matches, and Batocera/Knulli `gamelist.xml` merging with undo.
- Fill in the app (M4): fill wizard with a dry-run preview, review queue
  driven by keyboard, jobs with pause, resume, cancel and undo, per-ROM
  actions (change, from file, remove) and CSV/JSON reports.
- Library browser (M3): folder, system, date and flat views, virtualised grid
  and table, filters and search, and a detail panel per ROM.
- Identification and fill (M2): CRC32/MD5/SHA1 identification against
  libretro-database, name matching, arcade short-name translation, the
  libretro-thumbnails source, strategies (Priority, ROM region, Popularity),
  `coverly fill` with dry run, backups, journal and `coverly undo`.
- Audit (M1): scan any layout (Anbernic stock, GarlicOS, Onion, muOS,
  MinUI/NextUI, ES-DE, Batocera/Knulli, RetroArch, custom) and `coverly audit`
  with CSV/JSON reports.
- Foundation (M0): Tauri 2 + React app, Rust workspace (core, CLI, app),
  English and Spanish UI, light and dark themes, licence and contribution
  documents.

### Changed

- Detail drawer: the cover, screenshot and title screen are now slots that
  open one workspace in place. Candidates load on their own (the first missing
  art when the drawer opens), a title search replaces them in the same spot, an
  empty slot fills with one click and replacing existing art asks right on the
  chosen image. "Show in Finder" moved next to the close button; identity and
  file facts follow the art.

### Fixed

- A `settings.json` saved with a UTF-8 byte-order mark is read normally
  instead of being set aside as corrupt.
- A broken legacy `library.json` is set aside once instead of warning on every
  start.
- The CLI warns on stderr when `settings.json` cannot be read as settings.
