# Changelog

All notable changes to Coverly ROMs are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html). Commits follow
[Conventional Commits](https://www.conventionalcommits.org/); a release moves
the notes below from `[Unreleased]` to a version heading (see RELEASING.md).

## [Unreleased]

### Added

- One folder per game for modern consoles: Organise moves a game, its
  updates and its DLC together into `<System>/<Game>/`, with the updates in
  `Updates/` and the DLC in `DLC/`. Only the folders that get content are
  created. The new «Coverly» library layout has it on; every other layout
  has a switch for it in Settings → Library. The plan shows each game with
  its updates and DLC beneath it and a preview of the folder it will make,
  and the whole run can be undone.
- Demos become games of their own, in a `<Game> (Demo)` folder, and are
  linked to their game by name.
- Versions and regions in names: updates are named with the version read
  from the file (3DS, Wii U, Switch when in clear, PS3, PS4, Vita, PSP,
  Xbox 360), and the region read from the title or product code is added to
  the game's name. Two regions of one game get two folders.
- Updates, DLC, demos and system titles are now recognised on more
  consoles: 3DS, Switch, Wii U, Wii and WiiWare, PS3, Vita, PSP, PS4, PS5,
  Xbox 360, Xbox One and Series. Anything unclear stays unknown and is left
  where it is.
- Retro revisions such as `(Rev 1)` or `(v1.1)` are grouped as versions of
  one game in the library and in Duplicates, where the newest is the
  suggested keeper. On disk they stay as they are.
- The library scan, Organise and Duplicates respect a game's `Updates/`,
  `DLC/`, `Mods/`, `Cheats/`, `Extras/` and `Saves/` folders: their files
  are never moved out as loose games nor reported as copies of the game.
- `coverly organize --game-folders` and `--no-game-folders` override the
  layout's setting for one run.
- Mods, translation patches and cheats are recognised and linked to their
  game without opening any key: patches (`.ips`, `.bps`, `.ups`,
  `.xdelta`, `.ppf`) by the CRC32 of the ROM they were made for, their
  name or their readme, minding copier headers; cheats by the game's name,
  serial or title ID (`.cht`, `.pnach`, Dolphin and PPSSPP `.ini`, Azahar,
  Switch, RPCS3, shadPS4 and Xenia files); texture packs, LayeredFS and
  Atmosphère mods, Cemu graphic packs, rePatch folders, Mesen HD packs and
  MSU-1 sets by their folder. With the «Coverly» layout Organise puts them
  in the game's `Mods/` or `Cheats/` folder, created only when something
  goes in, and the run can be undone. One that fits no game, or several,
  stays where it is and the plan says why; nothing is ever deleted.
- Emulator layouts: Organise can lay a system out for its emulator
  (shadPS4, RPCS3, Azahar, Cemu, Vita3K, Ryujinx and yuzu, RetroArch,
  DuckStation, PCSX2, Dolphin, PPSSPP, Xenia, Flycast, Mesen) and for
  frontends such as ES-DE. Each system group has an «Emulator» choice and a
  preview of how the game will look for it. Updates and DLC that an
  emulator installs stay in the library with a note to install them there;
  nothing is ever extracted, decrypted or installed. A patch the emulator
  cannot use as it is, or one made for a ROM with a different header, says
  why in the preview.
- `coverly organize --emulator SYSTEM=EMULATOR` lays one system out for an
  emulator (`none` keeps the library's own layout), and
  `--emulator-data EMULATOR=DIR` names the emulator's own folder, where its
  mods, cheats and patches go; without it they stay where they are and the
  plan says where they belong.

### Changed

- A licence file beside a game (`.rap`, `.rif`, `.tik`, `.edat` and
  similar) may move with it, by a rename on the same drive only, when its
  name matches that one game. It is never opened, read or copied; when it
  is unclear which game it belongs to, it stays where it is.
- `keys.txt`, `otp.bin`, `ticket.db`, `eeprom.bin` and `.edat` files are now
  treated as key material and never opened. Files with a fixed key name
  (`prod.keys`, `seeddb.bin`, `work.bin`, `keys.txt` and the like) belong to
  no single game and always stay where they are.
- Firmware packages, shader caches and emulator settings files are shown as
  «not a game» and left where they are.
- Duplicates never removes a game whose folder holds updates or DLC; it only
  reports it.

### Fixed

- `organize --apply` into a folder that does not exist yet creates it when
  its drive is there, instead of stopping with «volume gone»; a drive that
  really is missing still stops the run before anything is written, and
  undo removes the folders it created when they are empty.
- A demo with no region in its name links to the game whose region
  matches, else the World or USA one, not the first by name.
- A name that already ends in a region word (`New Super Mario Bros 2 USA`)
  is no longer given the region twice.
- `coverly organize` no longer lists a licence that moves with its game
  under «Staying where they are».
- Organise's «Choose a system…» selects have an id and a name, so the
  browser no longer reports them as unlabelled form fields.

## [0.4.0] - 2026-11-03

### Added

- Settings → Integrations: one card per service Coverly talks to on your
  behalf (SteamGridDB, ScreenScraper, the Switch name catalogue and a link to
  Updates). Each card says what the service is for and what it unlocks, shows
  its status (not set up, saved, connected, error with the reason and what to
  do, daily quota used up, off), and has «Test connection» (works, wrong
  credentials, offline, service down), «How to get the key» on the service's
  own account page, turn off without deleting, «Forget credentials» with a
  confirmation in the page, a note of what is sent, and links to its credit
  and its place in the Sources order. Key fields are masked, can be shown
  while you type and are never filled with the saved value. Services planned
  for later are listed under «Coming later». The settings search finds every
  service and field.
- ScreenScraper quota on its card: requests used today of the daily maximum
  and the threads allowed, when the service reports them.
- Keychain messages per system: a keychain that is locked, unavailable or
  refuses access (permission denied) says what to do on macOS, Windows or
  Linux. A connection test on a keychain that cannot be read reports the
  keychain error instead of «not set up».
- Fill, Organise and their summaries point to the right card when an
  account is missing, a key is refused, a quota is used up or the Switch
  catalogue is off.
- `coverly integrations list|test|set|forget`. `set` reads the secret from
  stdin (without echo on a terminal) or from the variable `--from-env`
  names, never from an argument; no output prints a value. `fill` and
  `organize` read the keys the app saved in the keychain when the matching
  `COVERLY_*` variable is not set, only towards the service's own address;
  `COVERLY_NO_KEYCHAIN=1` turns that off. `fill` names
  `coverly integrations set steamgriddb` when the key is missing, and
  `catalog switch` names the Integrations tab when the catalogue is off.
- One list of integrations, shared by the app and the CLI, feeds the tab,
  the settings search, the notices and `coverly integrations`. Tests fail
  when a service has no credit, asks for a key the list does not know, or
  links its help anywhere but its own site.

### Changed

- Settings → Sources keeps only the order of the sources, with «Set up in
  Integrations» on the ones that need an account.
- The Switch name catalogue (on/off, icons and the fallback address) moved
  from Sources to Integrations. Your setting is kept.
- Keys and accounts saved before keep working: they are read from the same
  keychain entries.
- Settings, their export and their import never carry a key or a password,
  only whether one is saved; a test fills every key and checks.
- «See the plan» under «Coming later» opens the public roadmap page
  (coverly-roms.maecly.com/roadmap/) instead of the repository's TODO.md.
- The README, in English and Spanish, documents Integrations, the
  `coverly integrations` command and how the keychain is used, names the
  Switch catalogue in the data FAQ, and links the English features and
  roadmap pages under /en/.

### Fixed

- Typing a secret at the CLI's hidden prompt always gives the terminal its
  echo back, even when a signal ends the prompt; a value typed after the
  id is refused without being echoed.
- A link from another screen to a settings tab moves the focus to that tab,
  and Back from Settings or from a card returns to where you left (the
  fill wizard reopens on its step).

## [0.3.0] - 2026-11-02

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
- Nine more systems (127 in all): GameCube, Wii and WiiWare, Wii U, 3DS,
  Switch, PS Vita, PS3, Xbox and Xbox 360. Each one is identified from the
  plain header of its own format (disc header, NCSD/NCCH, XCI and PFS0
  tables, PARAM.SFO, XBE, XEX, STFS), reading a few kilobytes and never the
  whole game. Encrypted parts are never read: a Switch title id comes from
  the file table or the file name.
- Nintendo DS knows DSi-enhanced games and DSiWare from their header,
  without a separate system.
- A game that is a folder (PS3 game folders, PS Vita apps, Wii U game
  folders, Xbox and Xbox 360 game folders, split Switch files) moves as one
  item, shows its size in the plan, appears as one game in the library and
  is never split. Moves across drives copy and check every file before the
  original is removed, and can be undone.
- Updates and downloadable content are listed right under their game, and
  duplicates of the same game by title id are reported with the newer
  version suggested as the one to keep.
- Covers: the icon some games carry inside their files (PS3, PS Vita,
  Wii U, 3DS, Xbox 360) is used when no other source has a cover.
- Four current-generation systems (131 in all): PS4, PS5, Xbox One and
  Xbox Series. PS4 and PS5 game folders are identified from `sce_sys/`
  (`param.sfo`, `param.json`) and PS4 packages from their plain header;
  Xbox One and Xbox Series folders from `MicrosoftGame.config` or
  `AppxManifest.xml`. Nothing encrypted, no key and no licence is ever
  read. Xbox Series is catalogued only and marked «no emulator»; a game
  counts as Xbox Series only when its configuration says it targets that
  console. Updates, DLC and patch folders are listed under their game.
- Covers for PS4, PS5, Xbox One and Xbox Series try the icon the game
  carries first, then the other sources.
- Organise asks for confirmation, with the exact size, before writing
  10 GB or more.
- Optional Switch name catalogue from blawar/titledb (MIT), off until you
  turn it on in Settings → Sources. It downloads one region and language
  (by the app language, with a US English fallback), keeps only the title
  id, name, publisher, release date and region, refreshes at most once a
  week and is never shipped with the app. A file without `[0100…]` in its
  name is named from its NCA ids. Switch icons from the catalogue are
  opt-in and kept in a local cache; a second online source can be set as a
  fallback; with no network the file name is used.
- A Switch or 3DS game whose name cannot be read without console keys is
  labelled «Encrypted» instead of guessed. Coverly never reads, asks for,
  downloads or ships console keys.
- Loose executables and title metadata are classified from their plain
  headers (Switch `.nro`/`.nso`/`main.npdm`/`.nca`, GameCube and Wii
  `.dol`, Xbox `.xbe`, Xbox 360 `.xex`, Wii U `.rpx`/`.rpl`/`.tmd`,
  PS Vita, PS3 and PSP executables and backups, 3DS `.3dsx`): homebrew,
  game, update, DLC, or «part of a game» that is never moved on its own.
- Each item that writes 10 GB or more asks «Yes / Yes to all / No», with
  its size and the free space left on the target, in the app and in
  `coverly organize`; `--skip-large` leaves those items where they are
  without asking.
- Folder games inside zip and 7z archives (PS Vita, PS3, PS4, Wii U and
  others) are recognised from the archive listing, without extracting.
  PS Vita archives stay compressed; the others are extracted after you
  confirm, and PS4 is told apart from PS Vita.
- Loose multi-disc games are grouped into one set with an `.m3u` when the
  disc headers or the DAT hashes agree, with a note when a disc is missing.
- «Credits» settings tab: every cover and metadata source, database,
  technical reference, library and font Coverly uses, with its licence,
  link and the credit it asks for, searchable, and the full third-party
  licence texts viewable in the app.
- `coverly catalog switch status|update|remove`.
- Organise shows extraction as it happens: the archive opens and each
  track goes into its system folder, the current game's tracks are listed
  as each one is written and its CRC checked, and a bar by size shows the
  speed and the time left, with separate «Game 2 of 4» and «Track 5 of 16»
  counters instead of «0 of 1 files». Scanning, cover fill and duplicates
  show the same readout where it fits; nothing moves with reduced motion.
  `coverly organize` prints a line per extracted file, at most once a
  second.
- Duplicates screen: every copy, conflicting save and the `_Duplicados/`
  folder can be shown in Finder, Explorer or the file manager; each copy
  shows its region, revision, languages and dump tags, its cover, size,
  format, date, whether it has a save and the rule that kept it. Keys: ↑/↓
  and 1–9 pick the copy, J/K and Tab move between games, X skips a game,
  ⌘↩ / Ctrl+Enter applies.

### Changed

- Duplicates of the same game by title id and version, or by the same
  folder, proven by their content, are set aside or sent to the Trash (as
  the duplicates setting says) and can be undone, instead of only being
  reported.
- Extracting a folder game from an archive always asks first.
- Extractions to the same drive check the free space before writing.
- The app bundle ships the `LICENSES/` texts and the font licences.
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
- Console key and licence files (`prod.keys`, `title.keys`, `.rap`,
  `.rif`, loose tickets and similar) are left untouched: Coverly never opens,
  moves or asks for them. Emulator data folders (for example `ux0`) are left
  in place too.
- macOS `.pkg` installers and 3D Studio `.3ds` models are reported as not
  ROMs instead of an unknown system.
- Settings move to version 3: the new embedded-icon cover source is turned
  on once for existing users, after the other sources.
- `.zar` files without a readable Xbox 360 executable go to review instead
  of being filed as Xbox 360.
- `coverly organize --apply` needs `--yes-large` to write 10 GB or more;
  without it nothing is written.
- Empty files no longer hold back PS4, PS5, Xbox One and Xbox Series
  folder games.
- `.cik` files are treated as key material and left untouched.
- A Wii U `.wua` stays over a Wii U game folder with the same title id: a
  `.wua` can bundle the update and DLC the folder may lack. Your own pick,
  a save, «played» and «in the library» still decide first.
- The third-party notices name every source and component whose licence
  asks for a credit, drop an art source Coverly never used, and point to
  the full licence list generated from the lock files. The README's
  acknowledgements are generated from the same credits list as the app.

### Fixed

- RetroArch now shows covers, screenshots and title screens of games whose
  name has `&`, `*`, `:`, `` ` ``, `"`, `<`, `>`, `?`, `\` or `|`
  (for example «Mario & Luigi»): Coverly names the thumbnail with those
  characters replaced by `_`, as RetroArch looks for it. Thumbnails written
  before under the full name are left where they are and are not reported
  as orphans (when a duplicate is set aside, its old thumbnail goes with
  it); the next fill writes the right file next to them. Other profiles are
  unchanged.
- ScreenScraper media listed without a region, or under a region Coverly
  has no name for (such as ScreenScraper's own `ss`), are no longer dropped:
  they are offered after every region of your order.

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
