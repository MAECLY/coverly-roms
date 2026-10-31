# Changelog

All notable changes to Coverly ROMs are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html). Commits follow
[Conventional Commits](https://www.conventionalcommits.org/); a release moves
the notes below from `[Unreleased]` to a version heading (see RELEASING.md).

## [Unreleased]

### Added

- Windows on ARM64 builds of the app and the command line.
- 64 retro systems (118 in all), each with its folder names, extensions,
  libretro thumbnails folder, libretro-database DATs and ScreenScraper id
  where one exists, and the folder each built-in profile really uses:
  - Disc consoles and computers: PlayStation 2, 3DO, PC-FX, CD-i, Amiga
    CD32, CDTV, Jaguar CD, FM Towns.
  - Early consoles: ColecoVision, Intellivision, Vectrex, Odyssey² /
    Videopac, Channel F, Arcadia 2001, Super Cassette Vision, Jaguar.
  - Add-ons: SG-1000 / SC-3000, Sega Pico, SuperGrafx, 64DD, Satellaview,
    Sufami Turbo.
  - Handhelds: Supervision, Game.com, Mega Duck / Cougar Boy.
  - Computers: ZX Spectrum, ZX81, Amstrad CPC, BBC Micro, Thomson MO/TO,
    Dragon 32/64, TRS-80 Color Computer, TI-99/4A, X68000, X1, PC-98,
    PC-88, Plus/4, PET, C128, Atari XE / XEGS, MSX2, MSX turbo R,
    Apple II.
  - Fantasy consoles: TIC-80, WASM-4, Arduboy, Uzebox, LowRes NX,
    Vircon32.
  - Ports and engines: Doom, Quake, Cave Story, Wolfenstein 3D, Mr. Boom,
    Dinothawr, Lutro, Solarus, Flashback, Rick Dangerous. A game that is
    a folder (`Freedoom Phase 1/`) moves as one item.
  - Arcade: Sega Model 2, Model 3, NAOMI 2, Triforce. A MAME set's CHD
    folder moves with its `.zip`.
- Headers and DAT hashes identify the new systems where they can. Shared
  extensions such as `.bin`, `.rom`, `.dsk`, `.tap` or `.hex` never pick a
  system on their own: only a system folder, a header or a hash does,
  otherwise the file goes to review.
- Organise: the «left in place» system picker groups systems by family
  (consoles, handhelds, computers, arcade, engines). A profile with no
  folder for a system says so and names the folder it creates.
- System icons for the new systems: RetroArch silhouettes where a neutral
  one exists, the short label mount otherwise.

### Changed

- Release files are named by system and processor
  (`Coverly-ROMs_<version>_mac_arm.dmg`, `coverly_<version>_linux_x64`, …);
  macOS ships one download per chip instead of a universal one. Installed
  copies keep updating.
- Settings: Organise and Duplicates have their own tabs; advanced options are
  folded; search with ⌘F / Ctrl+F; the Duplicates and Organise screens link
  to their settings and offer to look again after a change.
- PlayStation 2, 3DO and Sega Pico games are organised; before they were
  recognised and left in place.
- Loose `.hdf`, `.fdi` and `.dim` files without an Amiga, Atari ST or
  other known header go to review instead of being filed automatically.
- `MSX2/` folders belong to MSX2, not MSX: covers come from the MSX2
  thumbnails, so art paths for those games move once and the next fill
  downloads them again. Old files are not deleted.
- `.sgx` files go to SuperGrafx and `.bs` files to Satellaview. Anbernic
  stock keeps them in `PCE` and `SFC`, and MinUI keeps `.bs` in `SFC`;
  MSX2 and Atari XE games stay in `MSX` and `A800` where the device has
  no folder of their own.
- Custom profiles made before this version keep rule-based folder names
  for the new systems; clone the built-in profile again to get the
  frontend's real names.

## [0.2.0] - 2026-10-25

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
- Organise opens archives of every common kind, recognised by their contents:
  zip and zip64 (deflate, deflate64, bzip2, LZMA, XZ, zstd, PPMd), 7z (LZMA,
  LZMA2, BCJ, PPMd and more), tar, gzip, bzip2, xz, zstd, lz4, and single
  compressed ROMs such as `game.gba.gz`. A file whose extension names another
  format gets the note "It is a 7z archive named .zip" and is organised
  anyway.
- RAR archives open through a program already on the computer (`unrar`,
  7-Zip or `bsdtar`, which macOS includes); without one, the RAR stays with
  "To open RAR archives, install unrar" and the rest of the plan carries on.
  Settings → Organise → External programs for RAR lists what was found and
  can turn them off; `coverly organize --no-external-tools` does the same.
- Disc games inside archives are identified from the disc: PlayStation (serial
  from `SYSTEM.CNF`, e.g. `SLUS-00862`), Sega Saturn, Dreamcast, Sega CD,
  PC Engine CD and Neo Geo CD; PlayStation 2 and 3DO are named as not
  supported yet. On apply their `.cue`/`.gdi` and tracks are extracted into the
  system folder (flat, or one folder per game with an `.m3u` where the layout
  needs it), each file written to a temporary file, verified by size and CRC,
  re-read from the disk and only then renamed without overwriting. The dry
  run shows "Extract · 12 files · 405.7 MB" and "Extracting: N games".
- Cartridge ROMs in a container the layout cannot load (MinUI and 7z, RAR,
  tar, gz…) are extracted; zip and 7z stay as they are where the layout loads
  them.
- In move mode the archive goes to the Trash only after everything was
  verified, and only when nothing else is in it (a save file inside keeps
  it); copy mode never touches it. Free space (by cluster size), FAT32 4 GiB
  files and FAT32 folder capacity are checked before writing; a full disk
  rolls the game back and pauses the job. Resume after an interruption never
  rewrites finished files, and undo removes what was extracted and brings the
  archive back from the Trash, or keeps the extracted files when the Trash was
  emptied.
- Archives are read defensively: decoders run in a panic guard, with memory
  caps checked from each header before decoding, bounded headers, listings
  and trailers, safe entry paths (no zip-slip, absolute paths or device
  names), and strict parsing of external tools' output.
- New "left in place" reasons: encrypted, needs too much memory, too big to
  open safely, in several parts, unsafe entry, archive inside an archive,
  arcade set to repack, folder full on FAT32, unsafe target folder.
- Duplicates: the same game stored more than once in a system, found by its
  content and never by its name. Exact copies whatever the name, container,
  copier header (SNES, Master System, Game Gear) or N64 byte order; the same
  disc in another format (cue/bin, iso, chd, pbp, cso) by its serial and a
  fingerprint of the disc; other versions (revision, region, beta) from the
  catalogue, listed and set aside only with "One ROM per game". Identical
  arcade sets under two valid driver names, BIOS files, hacks, translations
  and soft-patched copies are never grouped.
- For each game one copy stays, picked by rules you can reorder or turn off
  (a save, played, verified dump, retail, region, revision, container, cover,
  catalogue name) and shown with its reason. You can keep another copy, leave
  a game or a system alone, and the pick is remembered. Copies with different
  saves are never touched.
- The other copies are set aside in `_Duplicados/` inside the library,
  keeping their folders, with their saves, states, art and `gamelist.xml`
  entries, plus a list of what was moved; or sent to the Trash if you choose.
  Space, FAT32 limits and names are checked before anything moves, a disc set
  moves whole or not at all, a full disk pauses the job, and resume finishes
  it. Undo puts everything back, from the Trash too; nothing is deleted.
- A Duplicates screen ("shelf and tray": the copy that stays above the
  copies that go, with where each one goes), opened from the library
  toolbar, plus duplicates jobs in the job history and a Duplicates block in
  Settings → Library.
- `coverly dedupe` finds and sets aside duplicates from the terminal (dry run
  unless `--apply`; `--trash`, `--quarantine`, `--resume`, `--keep`,
  `--one-game-one-rom`, JSON report), and `coverly undo` undoes its jobs.

### Changed

- Organise no longer keeps the shortest name of two equal files and leaves
  the other: a pile file the library already has (under any name or
  container) is set aside in the pile's `_Duplicados/` with the same keeper
  rules, and two equal pile files keep the better copy. Settings → Library
  and `coverly organize --duplicates set-aside|report|off` choose the step.

- Grid and List became Covers and Details. The saved choice carries over:
  List opens Details, Grid opens Covers.
- The narrow-window toolbar folds the views into one View menu that lists
  their shortcuts.
- The minimum supported Rust version is 1.93 (needed by `sevenz-rust2`).

### Fixed

- 7z files named `.zip` were reported as damaged archives. Containers are now
  detected by their magic bytes, the extension is only a fallback, and an
  archive is "damaged" only when its own listing fails.

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
