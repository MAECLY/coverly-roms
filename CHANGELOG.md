# Changelog

All notable changes to Coverly ROMs are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html). Commits follow
[Conventional Commits](https://www.conventionalcommits.org/); a release moves
the notes below from `[Unreleased]` to a version heading (see RELEASING.md).

## [Unreleased]

### Added

- System icons beside every system name: sidebar, filters, the Details system
  column and group rows, covers, the inspector, Reports, the fill wizard and
  the organise screen. Neutral hardware silhouettes from RetroArch Assets'
  Monochrome theme (CC BY 4.0, attributed in THIRD-PARTY-NOTICES.md), or a
  short label in a Slot-shaped mount for systems with no neutral drawing and
  for your own systems; a folder for one Coverly did not recognise.
  `pnpm system-icons` regenerates them from a pinned commit.
- Six library views, on ⌘/Ctrl+1–6: Covers, Details, Columns (system →
  folder → game → preview), Gallery (large art with a filmstrip), Art slots
  (cover, title and screenshot side by side) and Shelves (one row per
  system). A View options popover holds what applies to the current view:
  cover size in five stops (also ⌘/Ctrl +/−/0 and ⌘/Ctrl + wheel), art
  shown, group by (system, folder, region, art status, first letter), sort,
  Details columns (pick, reorder, resize, thumbnail column) and row density,
  and a docked inspector (⌘/Ctrl+I).
- One keyboard model in every view: arrows, Home/End, Page Up/Down, Enter to
  open, type-ahead by name, ⌥↑/⌥↓ between groups, and Space for a Quick look
  that steps through games with ←/→.
- Each library remembers its view and options (the 20 most recent); a new
  library opens with the last ones used.

### Changed

- Grid and List became Covers and Details. The saved choice carries over:
  List opens Details, Grid opens Covers.
- The narrow-window toolbar folds the views into one View menu that lists
  their shortcuts.

## [0.1.0] - 2026-10-17

### Added

- Organise a folder of loose ROMs by system: picking a folder with no known
  layout offers "Organise this folder…" instead of an error. Each file is
  identified by catalogue CRC (zip entries included), header magic,
  extension and name — plus ScreenScraper by CRC when configured — with a
  confidence and a reason; doubtful files are grouped for review and
  unfinished downloads, BIOS files, web pages, damaged or multi-game
  archives and duplicates stay where they are. The proposal follows any
  layout profile (Anbernic stock by default, `ROMS/<SYS>/`), into the same
  folder or another one, moving or copying. Applying never overwrites
  (no-clobber renames, verified copies across drives), journals every step,
  pauses and resumes on a full, read-only or removed drive, and is undone
  like a fill job; "Open library" opens the result with that layout.
  Libraries with stray ROMs outside their system folders offer "Organise
  loose files…". `coverly organize` does the same from the terminal (dry run
  unless `--apply`; `--copy`, `--target`, `--assign`, `--resume`).
- `coverly undo --job ID --include-later` also undoes the newer jobs that
  wrote under the same folder, newest first.
- Organise settings in Settings → Library (default layout, ROMs folder,
  mode, names, catalogue downloads).

- Landing page at coverly-roms.maecly.com, in Spanish and English: what it
  does, the safe round trip (dry run, backups, undo), supported layouts,
  comparison, downloads per system and FAQ, with motion that stops for
  reduced motion and no third-party requests. Pages fills it with the
  latest release when one is published, and the page also reads the latest
  release in the browser, so the download links follow a new version at
  once.
- First-open help for the unsigned builds, in the landing, the README and the
  release notes: why macOS says the app is damaged and how to clear the
  quarantine flag (`xattr -rd com.apple.quarantine "/Applications/Coverly
ROMs.app"`) or use Open Anyway, SmartScreen's More info → Run anyway, and
  the Linux `.deb` dependencies.
- Brand identity: the Lean-To icon, design tokens shared by the app and the
  landing (light and dark, checked for contrast), Archivo and Atkinson
  Hyperlegible type, and UI primitives built on them across the app.

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

- Job journals record their kind (fill or organise); fill and organise
  journals are kept separately, and organise journals at least 90 days, so
  later fills never delete the way back. Undoing a job that newer jobs built
  on is refused unless they are undone too. Journals from older versions
  still undo.
- A folder already organised by system but without layout markers opens
  with a suggested layout instead of failing detection.

- Detail drawer: the cover, screenshot and title screen are now slots that
  open one workspace in place. Candidates load on their own (the first missing
  art when the drawer opens), a title search replaces them in the same spot, an
  empty slot fills with one click and moves on to the next empty one, and
  replacing existing art asks right on the chosen image. Candidates are listed
  one per row with their full name. "Show in Finder" moved next to the close button; identity and
  file facts follow the art.

### Fixed

- A `settings.json` saved with a UTF-8 byte-order mark is read normally
  instead of being set aside as corrupt.
- A broken legacy `library.json` is set aside once instead of warning on every
  start.
- The CLI warns on stderr when `settings.json` cannot be read as settings.
