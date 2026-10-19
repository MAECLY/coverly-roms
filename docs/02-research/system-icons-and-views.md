# System icons and library views: research for the next library iteration

> Research for branch `feat/system-icons-views` (2026-10-18). Three strands: an icon-set survey with licence and trademark checks, a survey of view modes in file managers and ROM managers, and an audit of this repo. Rows marked (K) come from general product knowledge and were not confirmed against a source. This is research, not legal advice.

## Summary

- **System icons:** use the **RetroArch Assets XMB "Monochrome" theme** (`libretro/retroarch-assets`, `src/xmb/monochrome/*.svg`, CC BY 4.0). It has single-colour hardware silhouettes, not maker logos. It covers 46 of Coverly's 54 systems directly, and 3 more through a proxy. Vendor it at a pinned commit and recolour it to `currentColor`. For the rest (wordmark-style icons, missing systems, user-defined systems), use a Coverly **Slot glyph plus an abbreviation**, or a `lucide-react` glyph (already a dependency).
- **Views:** grow from Grid and List to **eight views**: Icons (with a size slider), List, Details, Tiles, Content, Columns, Gallery, Shelves. Add settings that apply to every view: Group by, Sort, Compact density, a docked Inspector, art type, and Cmd/Ctrl+1–8 to switch. Skip wheel and carousel modes, which are for launchers on a TV, not for managing a library.
- **Repo:** system names appear in 5 library places, 3 report and fill places, and 7 organise places. All library places can reach `system_id`. Reports and fill only have the folder, so they need a lookup. In organise, the only change is inserting the component in `SystemGroups.tsx`, to keep the merge with `feat/organize-disc-archives` easy. `Layout` lives in `src/stores/viewStore.ts` and is saved under the localStorage key `coverly.layout`. It needs a list of accepted values so that values saved by older versions still load.

---

## Part A: System icons

### A.1 Recommendation

- **Primary set: RetroArch Assets, XMB Monochrome.**
  - **Licence:** CC BY 4.0 for the whole repo (`COPYING`). The FSF lists CC BY 4.0 as compatible with all versions of the GNU GPL, so it can ship inside an AGPL-3.0 app if Coverly gives attribution and says what it changed.
  - **Style:** solid white single-colour SVG silhouettes of the hardware or the controller. They were made white on purpose so that code can recolour them (commit "Change monochrome to white, for recoloring in code", 2019), so they can be themed with `currentColor`.
  - **Coverage:** 46 of the 54 systems directly, plus 3 through a proxy.
  - **Upkeep:** the repo is active. The survey saw a last commit of 2026-10-01 in one place and 2026-08-01 in another, so check the date before pinning. Pin to `d9f969054dc7fbb6fa89519036d2b971e0855b51`.
  - **Precedent:** RomM (AGPL-3.0) ships the same libretro icons, with an `ATTRIBUTIONS` file that credits them under CC BY 4.0.
- **Fallback: a Coverly Slot glyph plus an abbreviation**, drawn in-house on the brand's slot motif (`design/icon/`). The abbreviation is in Atkinson Hyperlegible Mono. Use it for:
  - systems the set does not cover;
  - icons in the set that are really wordmarks (CPS, MAME, DOS, ScummVM, RPG Maker);
  - systems that users define themselves.

  Where a meaningful glyph reads better, use `lucide-react` (`package.json`, v1.59.0, ISC).
- **Rejected:**
  - **Maker logos** (trademark risk): Simple Icons and Font Awesome Brands.
  - **Non-commercial terms** (incompatible with AGPL): Custom Brand Icons (`cbi`) and the ES-DE, Carbon and ckau-book themes.
  - **No licence:** HVR88, whose icons are also wordmarks.
  - **No SVG:** KyleBing, which is PNG only and illustrated.

### A.2 Candidate matrix

| Source | Coverage of the 54 | Style | Format | Licence | Trademark risk | Verdict |
|---|---|---|---|---|---|---|
| **RetroArch Assets `src/xmb/monochrome`** (849 files, 488 system icons) | 46 direct + 3 proxy | Mono hardware or pad silhouettes; a few wordmarks | SVG, white fill, viewBox 64 or 256 | CC BY 4.0 | Low: generic device shapes used beside the name (nominative). Avoid the wordmarks | **Primary** |
| RetroArch `automatic` / `daite` / `flatux` | Adds Atomiswave, NAOMI, HBMAME | Colour raster | PNG | CC BY 4.0 | Low/medium | Cannot be themed; skip |
| Simple Icons (`simple-icons`, 3,466) | atari, commodore, sega, playstation family | Brand **logos** | SVG | CC0, but the project tells users to follow each brand's guidelines | **High** | Reject |
| Font Awesome Brands (`fa7-brands`) | playstation only | Logo | SVG | CC BY 4.0 | High | Reject |
| Material Design Icons (`mdi`) | game boy, playstation, switch/wii/wiiu, gamepads | Mixed logos and devices | SVG | Apache 2.0 | Medium | Not needed |
| Custom Brand Icons (`cbi`) | nes, snes, n64, dreamcast, saturn, genesis, atari, gamecube | Devices and logos | SVG | **CC BY-NC-SA 4.0** | n/a | **Incompatible** |
| Game-icons.net | Generic consoles and controllers only | Silhouettes | SVG | CC BY 3.0 | Low | Not needed |
| Arcticons | Emulator app icons, not systems | Line | SVG | CC BY-SA 4.0 | n/a | Wrong subject |
| Streamline, Solar, Hugeicons, Tabler, teenyicons | One-offs | Line or solid | SVG | Varies (CC BY 4.0, MIT) | Low/medium | Too few, inconsistent |
| Lucide (already a dependency), Phosphor | Generic: gamepad-2, joystick, monitor, terminal, disc, cpu, book-open | Line, `currentColor` | SVG | ISC / MIT | None | **Fallback glyphs** |
| HVR88/Monochrome-Gaming-Logos (620+) | Broad | Mono **wordmarks** | SVG + PNG | **None.** README warns about fair use and commercial use | High | Reject |
| KyleBing/retro-game-console-icons (~104) | Broad | Illustrated | **PNG only** | CC BY 4.0 | Low/medium | Reject |
| ES-DE `system-logos` | Broad | Wordmarks | SVG | **None** | High | Reject |
| ES-DE Slate/Modern themes | Broad | Logos and art | SVG/PNG | Custom; "PROHIBITED: Commercial distribution" | High | **Incompatible** |
| Batocera/RetroPie Carbon, ckau-book, KNULLI themes (Art Book Next) | Broad | Logos and console photos | Mixed | Carbon: custom non-commercial licence. ckau-book: CC BY-NC-SA 4.0. Dan Patrick logos: credited, no licence | High | **Incompatible** |
| Playnite, LaunchBox community packs | Broad | Logos | Mixed | Uploaders disclaim ownership; "never sell" | High | Reject |
| RomM `frontend/assets/platforms` (285) | Broad | Mixed | SVG/ICO | The libretro subset is CC BY 4.0; the rest has no stated licence | Mixed | Precedent only |

### A.3 Map from Coverly id to icon

**Do not derive the map from the `libretro` field in `systems.toml`.** That field is the playlist name, and it matches the icon file names for 49 of the 51 systems that have one. The match still fails in three ways:

- `a800` is "Atari - 8-bit" in the field, but the file is "Atari - 8-bit Family".
- `naomi` has no SVG.
- cps1, cps2, cps3, hbmame, pgm2 and varcade all point to "MAME", and gw points to "Handheld Electronic Game".

Keep an explicit id → file map in the frontend.

**Direct silhouettes (41).** Each Coverly id below maps to a RetroArch Monochrome file.

| Coverly id | Monochrome file |
|---|---|
| a2600 | Atari - 2600 |
| a5200 | Atari - 5200 |
| a7800 | Atari - 7800 |
| a800 | Atari - 8-bit Family |
| atarist | Atari - ST |
| lynx | Atari - Lynx |
| amiga | Commodore - Amiga |
| c64 | Commodore - 64 |
| vic20 | Commodore - VIC-20 |
| msx | Microsoft - MSX |
| nes | Nintendo - Nintendo Entertainment System |
| fds | Nintendo - Family Computer Disk System |
| snes | Nintendo - Super Nintendo Entertainment System |
| n64 | Nintendo - Nintendo 64 |
| gb | Nintendo - Game Boy |
| gbc | Nintendo - Game Boy Color |
| gba | Nintendo - Game Boy Advance |
| nds | Nintendo - Nintendo DS |
| gw | Nintendo - Game & Watch |
| virtualboy | Nintendo - Virtual Boy |
| pokemini | Nintendo - Pokemon Mini |
| mastersystem | Sega - Master System - Mark III |
| megadrive | Sega - Mega Drive - Genesis |
| segacd | Sega - Mega-CD - Sega CD |
| sega32x | Sega - 32X |
| saturn | Sega - Saturn |
| dreamcast | Sega - Dreamcast |
| gamegear | Sega - Game Gear |
| neogeo | SNK - Neo Geo |
| neocd | SNK - Neo Geo CD |
| ngp | SNK - Neo Geo Pocket |
| ngpc | SNK - Neo Geo Pocket Color |
| pcengine | NEC - PC Engine - TurboGrafx 16 |
| pcenginecd | NEC - PC Engine CD - TurboGrafx-CD |
| psx | Sony - PlayStation |
| psp | Sony - PlayStation Portable |
| wonderswan | Bandai - WonderSwan |
| wonderswancolor | Bandai - WonderSwan Color |
| fbneo | FBNeo - Arcade Games (neutral arcade-cabinet silhouette) |
| varcade | FBNeo - Arcade Games (same cabinet) |
| pgm2 | FBNeo - Arcade Games (same cabinet) |

**In the set, but a wordmark or logo, so use the fallback instead (6 entries):**

| Coverly id | What the set has | Use instead |
|---|---|---|
| cps1, cps2, cps3 | "CP System" wordmark | Cabinet with a CPS1/2/3 badge, or Slot with "CPS1", "CPS2", "CPS3" |
| mame, hbmame | MAME wordmark | Cabinet |
| dos | "DOS" lettermark | Lucide `terminal` or `monitor` |
| scummvm | ScummVM "S" logo | Slot with "SVM", or Lucide `mouse-pointer-click` |
| easyrpg | RPG Maker logo | Slot with "RPG", or Lucide `swords` / `scroll` |

**No SVG in the set (5):**

| Coverly id | Use |
|---|---|
| atomiswave, naomi | Cabinet. Only colour PNGs exist in the set |
| ons | Lucide `book-open` (visual novels) or Slot with "ONS" |
| openbor | Slot with "BOR" |
| pico8 | Slot with "P8". PICO-8 is Lexaloffle's mark; do not copy their logo |

**Near-duplicate shapes:** some pairs and groups share nearly the same shape:

- ngp and ngpc;
- wonderswan and wonderswancolor;
- pcengine and pcenginecd;
- megadrive, segacd and sega32x, which all use the Mega Drive pad.

This is acceptable because the name always sits next to the icon. Optionally, add a small "CD" or "C" pip in a corner.

### A.4 Facts that matter for the build

- **Fills are not uniform.** Most icons use `fill="#ffffff"`. `a800` uses `fill="#fff"`. CPS, DOS, MAME and Neo Geo use `style="fill:#ffffff"`, and DOS also has `fill:none`. The build must turn every fill that is not `none` into `currentColor`.
- **The viewBoxes differ** (`0 128 64 64` and `0 0 256 256`). Keep each icon's own viewBox and set the size in CSS, or normalise them with svgo.
- **No `<text>` or `<image>` elements.** Every icon is made of paths only.
- **Size:** the 47 sample icons are 212 KB raw and 87 KB after `svgo@3.3.2 --multipass`, about 38 KB gzipped. The median icon is about 1.5 KB. The largest are DOS (8.2 KB) and CPS (6.8 KB), and both are replaced by the fallback. Bundle all icons up front; lazy loading would not save enough to be worth it.

### A.5 How to bundle

1. **Vendor the icons with a script; do not fetch them at runtime.** Add `scripts/icons/systems.mjs`, run by `pnpm system-icons`. The script does the following:
   - It downloads the files from the pinned commit, using a `SYSTEM_ICON_SOURCES` map from id to file.
   - It runs svgo (multipass and `removeDimensions`; fills become `currentColor`; ids and styles are stripped).
   - It writes `src/assets/system-icons/<id>.svg`.
   - It generates `src/components/system/systemIcons.generated.ts`, typed as `Record<SystemId, { viewBox: string; body: string }>`.
   - It writes `src/assets/system-icons/README.md` with the source, the commit and the changes made.
2. **Add `<SystemIcon systemId size? title? className? />`.** It renders inline `<svg fill="currentColor" aria-hidden data-system-icon>`, so it follows the text colour in the light and dark Buckram & Manila themes. Its default size is 1em, and it sits beside the name in a `gap-1.5` flex wrapper.
   - An id that is unknown, unmapped or `null` gets the Slot fallback.
   - The abbreviations live in a frontend map, not in `systems.toml`, so that this branch does not touch core while `feat/organize-disc-archives` is open. Suggested abbreviations: `ONS`, `BOR`, `P8`, `SVM`, `RPG`, `DOS`, `CPS1`, `CPS2`, `CPS3`, `MAME`, `HB`, `PGM2`, `AW`, `NAO`. For a user-defined system, use the first 3–4 letters of its id in upper case.
   - When the id is `null`, the `arcade` and `multi_file` flags can pick a family glyph (arcade, disc, handheld, home console or computer) instead of a generic one.
3. **Add `<SystemLabel systemId name />`** (the icon plus the name). It turns each place a system name appears into a one-line change.
4. **Accessibility.** The icon is decorative (`aria-hidden`) because the name is always next to it, so existing text assertions in tests still pass. Where the icon stands alone, such as a compact badge, pass `title`; the icon then gets `role="img"` and an `aria-label`.
5. **Legal files:**
   - **`THIRD-PARTY-NOTICES.md`:** add a section that says: "System icons: RetroArch Assets, XMB Monochrome theme, © libretro contributors (https://github.com/libretro/retroarch-assets), CC BY 4.0. Modified: recoloured to currentColor, optimised with SVGO, some viewBoxes normalised. Source commit d9f9690…". Also add `lucide-react` (ISC), which is missing.
   - **`LICENSES/CC-BY-4.0.txt`:** add the licence text.
   - **`TRADEMARKS.md` "Other names":** say that system silhouettes identify compatible hardware and imply no endorsement. Add Capcom, Commodore, Bandai/WonderSwan, Microsoft (MSX), IGS, Lexaloffle/PICO-8 and ScummVM, which the list does not include.
   - **`docs/09-legal/trademark.md`:** extend the nominative-use text to the icons. The rule is a hardware silhouette plus the name, never a maker's logo.
6. **Tests:**
   - Every id in `systems.toml` resolves to an icon or a fallback abbreviation.
   - No generated body contains `#fff`, `<text` or `<image`.
   - A render test covers the fallback.
   - E2E checks that `svg[data-system-icon]` appears next to system names in the sidebar, the filter panel, the list and the detail view.

### A.6 Trademark rationale

- CC licences do not grant trademark rights (CC BY 4.0, Section 2(b)(2)). Clearing the marks is Coverly's job, whatever licence the icons carry.
- Nominative use is usually judged on three points, and Coverly meets each one:
  1. **The system cannot be identified without its name.** Coverly shows the name as plain text.
  2. **Use only as much of the mark as needed.** A 16 px generic device silhouette is not the maker's logo.
  3. **No implied sponsorship.** `TRADEMARKS.md` already disclaims it.
- Commentators say plain words are safer than stylised logos. That is why this plan excludes Simple Icons, Font Awesome, the ES-DE logos and HVR88, and why it replaces the wordmark-style Monochrome icons.
- **Remaining risk:** a device outline can count as trade dress (for example, the Game Boy shape). The icons here are 16 px, sit next to a text label and are not used as branding. RetroArch and RomM use icons the same way. If the owner wants no exposure at all, the fallback can replace any single icon.
- **Brand note:** `docs/08-brand/brand.md:28` rules out gamepads, D-pads, cartridges and Game Boy shapes in the app's own identity. That rule covers the logo, not UI that refers to hardware. The silhouettes must still stay quiet: monochrome, `currentColor`, never in brand accent colours. The pattern to follow is `src/components/brand/Logo.tsx`, an inline SVG in `currentColor`.

---

## Part B: Library views

### B.1 macOS Finder

| Mode | Layout | Keys | Density | Notes for 12k items |
|---|---|---|---|---|
| Icon | Thumbnail grid with an icon-size slider (16–512 px), grid spacing, text size, label position (bottom or right), and an optional "show item info" second line | Arrows move in 2D; typing jumps to an item; Cmd+1 (K) | Low to very high | A variable column count fits the row-of-N virtualiser that CoverGrid already uses |
| List | Rows with resizable columns. Control-click the header to show or hide columns; click a header to sort and again to reverse; folders expand inline | Up/Down; Left/Right collapse and expand; Cmd+2 (K) | High | Fixed row height. Group by adds sticky section headers |
| Column (Miller) | One pane per level, cascading. The last pane previews the selection | Left/Right change level; type to select; Tab and Shift-Tab; Cmd+3 | Medium | Virtualise each pane. For Coverly: System → group (region, letter or folder) → entry |
| Gallery | Large preview, a thumbnail strip below it, a metadata sidebar | Left/Right; Space for Quick Look; Cmd+4 | Low | Horizontal virtualiser; one large image loaded at a time |
| Group by | Available in every view: Kind, Size, Date. In icon and gallery views, groups become rows that can collapse | n/a | n/a | Grouping is a setting separate from the view mode |

### B.2 Windows 11 File Explorer

| Mode | Notes |
|---|---|
| Extra large, Large, Medium, Small icons | Thumbnail grids at four fixed sizes. Ctrl+Shift+1…4; Ctrl+mouse wheel steps between sizes |
| List | Small icons laid out by column (top to bottom, then wrap); scrolls horizontally; very dense. Ctrl+Shift+5 |
| Details | Columns the user picks, sortable, resizable and groupable. Computer Hope calls it the most useful view. Ctrl+Shift+6 |
| Tiles | Medium icon plus 2–3 lines: name, type and size. Ctrl+Shift+7 |
| Content | One wide row per item with a thumbnail and metadata across the row. Ctrl+Shift+8 |
| Compact view | A toggle that tightens spacing in every view. The default is padded for touch |
| Preview pane (Alt+P) and Details pane (Alt+Shift+P) | A resizable right-hand pane that shows either the preview or the metadata, not both |
| Group by and Sort by | Any column can be the grouping key; group headers collapse (K) |
| View memory per folder | Each folder remembers its own view |

### B.3 ROM managers and frontends

| App | Modes | Worth copying |
|---|---|---|
| LaunchBox | **Boxes:** virtualised tiles with a size setting; text fallback when art is missing; group separators when sorted. **List:** a data grid with many columns, sortable and resizable. Also a details pane, a platform sidebar and a size slider (K) | Users have asked for a thumbnail column in List view. The docs advise keeping the virtualised panel |
| Big Box (TV) | Wall 1–4, CoverFlow, wheels, text list with details, Platform Hybrid | Wheel views are for lean-back browsing, not management. Hybrid (a system rail beside games) is close to Coverly's Sidebar plus grid |
| Playnite | **Details:** a compact list with a details and media panel. **Grid:** covers with a hover info panel. **List:** a table with a column picker. Group and sort controls, an Explorer filter panel | Ctrl+F, Ctrl+G and Ctrl+E for panels, F3 to edit, Ctrl/Shift-click to multi-select. Details is the split view that DetailDrawer could become |
| ES-DE | Legacy views: Basic, Detailed, Video, Grid, plus "Automatic", which chose one per system. From v2, themes built from carousel, grid and textlist components | Let the user choose which art each tile shows (cover, marquee or screenshot), with a text fallback. The docs warn that scanning media is slow on big collections |
| RetroArch | **Ozone:** sidebar, list and thumbnail pane (primary and secondary). **XMB:** a ribbon crossed with a list. **Material UI:** list views with small, medium or large thumbnails | Keyboard first: arrows, Enter, Backspace, `/`. "List with thumbnail size S/M/L" is a separate mode from the grid |
| Steam | Home shelves (one row per collection); a grid with Small, Medium, Large and Auto sizes; dynamic collections | Shelves are grouped carousels. Dynamic collections are saved filters |
| GOG Galaxy 2.0 | Grid with configurable info under each tile; a table with columns you can reorder and toggle; sort and group | A column picker plus metadata chosen per tile covers most power users |
| OpenEmu | Box-art grid with a size slider, a list, a system sidebar with icons (K) | Matches the system-icon request |
| RomM | Cover grid, dense list, server-side paging | Pages its results instead of keeping a huge DOM |
| Pegasus (gameOS) | TV rows and a grid. The user can switch the art: box, screenshot, wheel, cartridge, Steam tile. Alphabet jump | Alphabet jump rail and art-type switch |
| Skraper | Tree or list of systems → games, with scrape-status icons and a preview (K) | A list that leads with status, which is closest to Coverly's art-status purpose |

### B.4 Patterns across these apps

1. **Grouping is separate from the view mode.** Group by System, Region, Art status or Letter, on top of any view.
2. **Size is a zoom within a mode.** Offer a few named presets (XS–XL) that map to slider values. Finder, Explorer and Steam all work this way.
3. **A master-detail split.** A docked right-hand inspector that works with every view (Playnite Details, the Explorer panes, Ozone, Finder Gallery).
4. **A column picker** with sort, resize, reorder and an optional small thumbnail column.
5. **Art type per tile.** Cover, logo or screenshot, with a text fallback when art is missing.
6. **Keyboard.** Cmd+1..N or Ctrl+Shift+1..N to switch views, arrows in 2D, type-to-select, Space for preview, Enter to open.

### B.5 Recommended view set for Coverly

Coverly holds about 12k entries and is a management app, not a launcher.

| # | View | Built from | Virtualisation |
|---|---|---|---|
| 1 | **Icons** (grid with XS–XL size presets) | Existing CoverGrid; tile metrics become a size map | Rows of N (already there); thumbnails loaded at the size bucket in use |
| 2 | **List** (compact rows: small icon, name, system, status) | New, light | Fixed rows, `useVirtualizer` |
| 3 | **Details** (table with a column picker, sort, resize, optional thumbnail column and group headers) | Existing EntryTable | Fixed row height; sticky group headers through `rangeExtractor` |
| 4 | **Tiles** (cover plus 2–3 lines of metadata) | Grid variant | As in 1, with a taller fixed cell |
| 5 | **Content** (wide row: thumbnail plus metadata across) | List variant | Fixed tall rows |
| 6 | **Columns** (Miller: System → group → entries → preview) | New | Each pane virtualised; only the selected branch renders |
| 7 | **Gallery** (large preview, horizontal strip, metadata sidebar; covers CoverFlow-style browsing) | New | Horizontal virtualiser; one full-size image at a time |
| 8 | **Shelves** (one horizontal row per System) | Optional | A vertical virtualiser of rows, each row virtualised horizontally; cap each row and add a "See all" link |

**Settings for every view:**

- Group by: none, system, region, art status or letter.
- Sort.
- Compact density.
- Inspector pane (the DetailDrawer content, docked).
- Art type, where the media exists.
- Cmd/Ctrl+1–8 to switch views.

**Skip:** wheel and carousel modes. They are launcher patterns for TVs and controllers, and they show too little information for management work. Gallery covers that kind of browsing.

**Performance at 12k entries:**

- Never render the whole DOM.
- Keep a fixed row height per mode so that offsets are O(1).
- Load thumbnails lazily, at the size the mode needs, with an LRU cache of object URLs.
- Compute group buckets once in a memoised selector, not on every render.
- Keep the selection in the store, outside the view components, so that switching views keeps the selection and scrolls back to the selected item.
- Optionally, save a view for each system in viewStore, keyed by system id. Explorer remembers views per folder the same way.

---

## Part C: Repo audit (HEAD 5698515)

### C.1 Systems data model

- `crates/coverly-core/src/systems/systems.toml` has 54 `[[system]]` entries. Each has these fields: `id`, `name`, `aliases`, `extensions`, `libretro`, `arcade`, `multi_file`, `header`, `dats`, `screenscraper`. `cargo run -p coverly-cli -- systems` prints them.
- `SystemView {index, folder, dir, system_id: string|null, name, roms}` is defined in `src/ipc/types.ts:72` and `src-tauri/src/library/view.rs:72`. `system_id` is `null` for unknown folders, and `name` is then the folder name. Entries refer to their system by index: `EntryView.system` → `view.systems[i]`.
- `SystemInfo {id, name}` comes from `listSystems()`. Organise uses it.
- **The mock names differ from the DB names.** `src/ipc/mock/fixture.ts:40-115` uses names such as 'NES / Famicom' and 'Mega Drive'. **Icons must key on `system_id`, never on `name`.**
- Fill (`SystemCounts {folder}`) and the review queue (`item.folder`) carry only the folder. Resolve it through `view.systems`, matching on `dir` rather than `folder` because folder names may repeat when folders are nested.

### C.2 Where system names are rendered

| # | Place | What renders | Path to the id |
|---|---|---|---|
| L1 | `library/CoverGrid.tsx:159-161` | `.cover-tile__system` in the tile's info line (fixed `TILE_H=296`, an 18 px line) | `systems[entry.system].system_id` |
| L2 | `library/EntryTable.tsx:176-178` | `cell--system` | same |
| L3 | `library/Sidebar.tsx:191-201` | Systems tab, `scopeButton()` `:108-125`; needs an icon slot | `view.systems` |
| L4 | `library/FilterPanel.tsx:203-223` | Systems checkbox `Option label` | `view.systems` |
| L5 | `library/DetailDrawer.tsx:129,202-203` | `<dt>System</dt><dd>` | same |
| R1 | `reports/ReportsPanel.tsx:118-123` | Audit table row header | **None.** Add `system_id` to `AuditRow` (`src/lib/audit.ts`) |
| F1 | `fill/FillWizard.tsx:513-516` | Summary table `<th>{s.folder}</th>` | Resolve folder → `view.systems` |
| F2 | `fill/ReviewQueue.tsx:229` | `{item.folder} · media ·` | Resolve folder |
| O1 | `organize/SystemGroups.tsx:107-109` | Group trigger, beside the Folder icon | Has `systemId`: **insert here** |
| O2, O4, O5 | `SystemGroups.tsx:180-193`, `ReviewBuckets.tsx:148,191,216-233`, `LeftInPlace.tsx:87-91` | Native `<option>` elements | They cannot hold an SVG; leave them |
| O3 | `organize/ReviewBuckets.tsx:103-113,141-145` | Bucket title built by i18n interpolation | Optionally add a row of icons before the title |
| O6, O7 | `screens/OrganizeScreen.tsx:93-94`, `lib/organize.ts:188-190,238-242` | Interpolated prose | Skip |

These show no system names: Settings, StartScreen, StatusBar, ArtWorkspace, JobsPanel and the organise result screens. `ScanProgressView` shows only a folder, inside prose.

### C.3 Current view architecture

- **`src/stores/viewStore.ts`:**
  - `Layout = 'grid' | 'list'`, saved under the localStorage key `coverly.layout`. Reads and writes are wrapped in try/catch.
  - `readLayout()` maps every value other than `'list'` to `'grid'`. New layouts need a list of accepted values, and the old values must still be accepted.
  - The same store holds `scope`, `sort` (not saved), `drawerOpen`, `filtersOpen` and `overlay`.
  - `reset()` keeps the layout.
- **`library/Toolbar.tsx:136-157`:**
  - A `SegmentedControl` (Radix toggle-group, which renders `radiogroup`/`radio`) holds two icon-only items, each wrapped in `Tip`.
  - Below `max-width:1180px` the toolbar collapses its labels (`globals.css:383-440`).
  - Eight segments will not fit. Use 3–4 segments plus a `DropdownMenu` (`src/components/ui/dropdown-menu.tsx`) for size, columns and grouping.
- **`screens/LibraryScreen.tsx:124-128`:**
  - The screen chooses between `CoverGrid` and `EntryTable` and passes both the same `EntryListProps {entries, systems, generation, selectedId, onSelect, onOpen}`.
  - `entryIdOf` is shared from CoverGrid.
- **CoverGrid:**
  - Virtualises rows with `@tanstack/react-virtual`. Columns = `floor((width-GAP)/(TILE_W+GAP))`, with fixed constants `TILE_W=168`, `TILE_H=296` and `GAP=12`.
  - Keyboard movement comes from `nextIndex()` in `src/lib/keyboardNav.ts`.
  - ARIA uses `role=grid` with `aria-activedescendant`.
- **EntryTable:**
  - Fixed `ROW_H=34` and a sticky header.
  - Fixed columns: name, system, size, mtime, then one mark column per media type. There is no column picker and no column resizing.
- **i18n:**
  - `views.*` is already used by the Sidebar scope tabs (`ui.json:381-387`).
  - Put layout names under `toolbar.*` or a new `layouts.*` key, in both `locales/es` and `locales/en`.
- **Grouping:** none exists yet. `folderTree.ts` and `scope.ts` (`dateBucket`) can be reused.

### C.4 Tests and gates

- **Existing tests:**
  - `LibraryScreen.test.tsx:134-144`: the layout radiogroup and the grid names 'ROM covers' and 'ROM list'.
  - `CoverGrid.test.tsx`: virtualisation of 12k entries, the keyboard, the system/region line, hostile names.
  - `EntryTable.test.tsx`: 12k entries, sorting, the sticky header.
  - Text assertions on system names in the DetailDrawer, ReportsPanel, FillWizard, ReviewQueue, SystemGroups, ReviewBuckets and OrganizeScreen tests. These still pass only if the icon stays `aria-hidden`.
- **Gaps:** nothing tests saving the viewStore layout, and Sidebar and FilterPanel have no unit tests.
- **Helpers:** `src/test/layout.ts` (`stubLayout(w,h)`), `src/test/storage.ts`.
- **Gate:** `pnpm check-all`, which runs `tokens:check`, lint, `format:check`, typecheck and vitest.
- **E2E:**
  - There is no Playwright suite; E2E is run by hand with the Chrome DevTools MCP.
  - Vite is pinned to port 1431 with `strictPort` (`vite.config.ts:17`). To use port 1457, run `pnpm exec vite --port 1457`.
  - Stop the server by port: `lsof -ti tcp:1457 -sTCP:LISTEN | xargs kill`.

### C.5 What this means for the design

1. One shared `SystemIcon` and `SystemLabel`, keyed on the canonical `system_id`, `aria-hidden` and in `currentColor`, with the Slot fallback.
2. Library places L1–L5 can use `view.systems[i].system_id` directly. R1 needs `system_id` added to `AuditRow`. F1 and F2 need a lookup from folder (or `dir`) to `SystemView`. In organise, change only O1, and optionally the O3 header.
3. Widen `Layout` and check saved values against a list of accepted ones. Turn the CoverGrid metrics into a size map. Build the new views on `EntryListProps`, `entryIdOf`, `nextIndex` and the virtualiser, and keep the `role=grid` and `aria-activedescendant` pattern. Update the Toolbar to a segmented group plus a menu. Add i18n keys in both locales. Add tests for saving the layout, and virtualisation and keyboard tests for each view using `stubLayout`.

---

## Sources

### Icons and licences

1. RetroArch Assets and `COPYING` (CC BY 4.0): https://github.com/libretro/retroarch-assets, https://raw.githubusercontent.com/libretro/retroarch-assets/master/COPYING
2. The Monochrome source folder: https://github.com/libretro/retroarch-assets/tree/master/src/xmb/monochrome
3. RomM platform icons `ATTRIBUTIONS`: https://github.com/rommapp/romm/tree/master/frontend/assets/platforms
4. FSF licence list: https://www.gnu.org/licenses/license-list.html
5. Simple Icons on Iconify: https://icon-sets.iconify.design/simple-icons/. Simple Icons disclaimer: https://github.com/simple-icons/simple-icons/blob/develop/DISCLAIMER.md
6. Iconify collections API: https://api.iconify.design/collections
7. HVR88/Monochrome-Gaming-Logos: https://github.com/HVR88/Monochrome-Gaming-Logos
8. KyleBing/retro-game-console-icons: https://github.com/KyleBing/retro-game-console-icons
9. Game-icons: https://github.com/game-icons/icons. Pepicons: https://github.com/CyCraft/pepicons
10. ES-DE system-logos: https://gitlab.com/es-de/themes/system-logos. Slate: https://gitlab.com/es-de/themes/slate-es-de. Modern: https://gitlab.com/es-de/themes/modern-es-de. THEMES-DEV: https://gitlab.com/es-de/emulationstation-de/-/blob/master/THEMES-DEV.md. CREDITS: https://repo.retrodeck.net/Xargon/ES-DE/raw/branch/main/CREDITS.md
11. RetroPie es-theme-carbon: https://github.com/RetroPie/es-theme-carbon. ckau-book: https://github.com/CkauNui/ckau-book. Neutral: https://github.com/Mattersons/es-theme-neutral
12. KNULLI themes: https://knulli.org/configure/customization/themes/. Art Book Next: https://github.com/anthonycaccese/art-book-next-es
13. LaunchBox icon packs: https://forums.launchbox-app.com/files/file/4353-platform-platform-category-and-playlist-icons-pack-zipped-up/, https://forums.launchbox-app.com/files/file/3402-v2-platform-logos-professionally-redrawn-official-versions-new-bigbox-defaults/
14. Playnite: https://playnite.net/. theSVG (the RetroArch logo): https://thesvg.org/icon/retroarch

### Trademark

15. Nominative use: https://en.wikipedia.org/wiki/Nominative_use
16. Michael Best, nominative fair use: https://www.michaelbest.com/insights/nominative-fair-use-the-dos-and-donts-for-using-someone-elses-trademark-102mpkz/
17. Nolo, trademark fair use: https://www.nolo.com/legal-encyclopedia/when-you-need-permission-use-trademarks.html
18. ABA Landslide, nominative use: https://www.americanbar.org/groups/intellectual_property_law/resources/landslide/archive/nominative-trademark-use-affirmative-or-negative-defense-infringement/
19. Legal aspects of emulation: https://www.digitalradical.com/legal-aspects-of-video-game-emulation/

### Views

20. Apple Finder views: https://support.apple.com/en-in/guide/mac-help/mchlp1745/mac, https://support.apple.com/en-in/guide/mac-help/mchldaafb302
21. MacMost: https://macmost.com/live-mac-finder-views-and-view-options.html, https://macmost.com/15-tips-for-using-column-view-in-the-mac-finder.html
22. iMore: https://www.imore.com/customizing-finder-window-content
23. Finder column view keyboard: https://robservatory.com/two-ways-to-navigate-column-view-folders-in-finder/, https://mac.elated.com/?p=75
24. GNOME Miller columns proposal: https://discourse.gnome.org/t/add-miller-columns-as-a-view-option/36585
25. Windows 11 Explorer layouts: https://www.elevenforum.com/t/change-folder-layout-view-in-windows-11-file-explorer.1246/, https://www.computerhope.com/issues/ch001015.htm, https://geekrewind.com/how-to-change-file-explorer-folder-layout-view-in-windows-11/, https://geekrewind.com/how-to-show-or-hide-preview-pane-in-file-explorer-in-windows-11/amp/, https://it.environment.ucdavis.edu/file-explorer, https://digitalcitizen.life/view-tab-file-explorer, https://www.anoopcnair.com/show-details-pane-in-file-explorer-windows-11/
26. LaunchBox and Big Box views: https://launchbox.featurebase.app/help/articles/9450321-big-box-views, https://feedback.launchbox-app.com/help/articles/0928688-content-views-boxes-and-list-layouts, https://launchbox.featurebase.app/help/articles/2781973-launchbox-views-reference, https://launchbox.featurebase.app/p/optional-image-column-in-list-view
27. Playnite Desktop Mode: https://api.playnite.link/docs/manual/gettingStarted/playniteDesktopMode.html
28. ES-DE THEMES-DEV: https://gitlab.com/es-de/emulationstation-de/-/raw/master/THEMES-DEV.md
29. RetroArch Ozone: https://docs.libretro.com/guides/ozone. XMB: https://docs.libretro.com/guides/xmb/. 1.8.2 notes: https://www.libretro.com/index.php/category/retroarch/page/16/
30. Steam: https://www.thegamer.com/steam-how-to-sort-your-games-into-genres-and-tags/, https://www.pcworld.com/article/397986/steams-long-awaited-library-overhaul-launches-sept-17-salvation-for-your-massive-game-collection.html, https://www.pcgamesn.com/steam/update-august-1-2023, https://steamcommunity.com/groups/SteamClientBeta/discussions/3/3338785229804306994
31. GOG Galaxy 2.0: https://www.thefpsreview.com/2019/09/13/gog-galaxy-2-0-atlas-update-brings-global-search-and-new-game-library-customizations, https://www.softpedia.com/progChangelog/GOG-Galaxy-Changelog-254202.html
32. RomM: https://selfhostyourself.com/services/romm, https://forge.light.kow.is/mirrors/romm/src/master/
33. Pegasus gameOS: https://github.com/HVR88/gameOS, https://retropie.org.uk/forum/post/201836
34. OpenEmu: https://en.wikipedia.org/wiki/OpenEmu
