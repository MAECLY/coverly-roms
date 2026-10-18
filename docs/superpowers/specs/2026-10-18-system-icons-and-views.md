# System icons and library views

Status: design spec, 2026-10-18. Branch `feat/system-icons-views`.
Builds on the research in
[`docs/02-research/system-icons-and-views.md`](../../02-research/system-icons-and-views.md).

The owner asked for two things:

1. Wherever the app shows a system's name (for example "Game Boy"), show
   that system's icon beside it, on the left or the right as the place
   suits. Choose an icon library that serves this, and implement it.
2. Offer more library views than Grid and List: the range that macOS
   Finder and Windows Explorer have, plus the views of other ROM managers.

Part A covers the icons and Part B the views. Part C is the test plan.

---

## 0. Design plan and review against the brief

Coverly ROMs is a collector's shelf, not a launcher (brand §1). Its job is
finding empty art slots and filling them. Both features are judged against
that.

- **Colour.** Nothing new. The Buckram & Manila tokens stay as they are:
  `--cr-text-secondary` for icons at rest, `--cr-primary` only for
  selection and focus, `--cr-slot-missing` for dashed empty mounts, and
  Manila only for the applied-this-run tab.
- **Type.** Archivo for view titles and group headers, Atkinson
  Hyperlegible for body and metadata, and Atkinson Hyperlegible Mono only
  for the fallback abbreviation inside the icon. Sizes follow the app's
  current scale; this spec adds no new type sizes.
- **Layout.** Every view is a different way of looking at the same ordered
  list of entries: same selection, same sort, same filters, same inspector.
  Content is left-aligned and starts on the leading edge.
- **Principles.**
  1. Icons label things; they do not decorate them. Each icon is quiet and
     monochrome and always sits next to a text name, except where there is
     no room for one.
  2. A view earns its place only if it shows something no other view
     shows: more items, more metadata, the hierarchy, or the art itself.
  3. The Slot is the one bold element. The new **Mounts** view (B.4.5)
     shows a row's three art slots (cover, title, screenshot) side by side,
     so missing art is visible at a glance. The other views stay quiet.

**What changed after reviewing this plan.** The first draft had nine views,
including a CoverFlow-style carousel and a horizontal Explorer "List".
Both were dropped (§B.3): carousels are a TV-launcher pattern and show
about five items per screen, and a list that scrolls sideways fights the
trackpad and screen readers at 12,000 rows. The first draft also had an
Explorer-style "Content" view, which only repeated Details with a larger
thumbnail. It became **Mounts**, the one view tied to Coverly's actual
job, which no ROM manager offers.

---

# Part A. System icons

## A.1 Library choice

**Primary set: RetroArch Assets, XMB "Monochrome" theme**
(`libretro/retroarch-assets`, `src/xmb/monochrome/*.svg`), vendored at
commit `d9f969054dc7fbb6fa89519036d2b971e0855b51`.

| Question | Answer |
|---|---|
| Licence | CC BY 4.0 (repo `COPYING`). The FSF lists CC BY 4.0 as compatible with every GPL version, so it can ship in an AGPL-3.0 app with attribution and a note of changes. |
| Style | Solid, single-colour silhouettes of the console or its controller, drawn in white so code can recolour them. They work with `currentColor` in both themes. |
| Coverage | 41 of the 54 systems use their icon directly. 13 use the fallback (§A.4). |
| Precedent | RomM (AGPL-3.0) ships the same icons with a CC BY 4.0 attribution file. |
| Weight | About 87 KB after SVGO (about 38 KB gzipped) for every icon in the set. Bundled eagerly; no lazy loading. |

**Fallback glyphs:** `lucide-react` (already a dependency, ISC), used only
for the unknown-folder icon (§A.4).

**Rejected sets** (details in the research doc):

- Simple Icons and Font Awesome Brands: these are the makers' logos, the
  kind of trademark use to avoid.
- Custom Brand Icons, the ES-DE themes, RetroPie Carbon and ckau-book:
  non-commercial licences, which AGPL-3.0 cannot accept.
- HVR88 and the ES-DE `system-logos`: no licence, and they are wordmarks.
- KyleBing: PNG only and illustrated, so the icons cannot be themed.

### Trademark rationale

The CC licence covers copyright only; it grants no trademark rights
(CC BY 4.0 §2(b)(2)). The test that matters is nominative use, and
Coverly meets each of its three parts:

1. **The system cannot be named without its name.** The name is always
   shown as plain text.
2. **Use only as much as is needed.** A generic 16px silhouette of the
   hardware is used, never the maker's logo or wordmark. The
   monochrome icons that are wordmarks (CPS I/II/III, MAME, DOS, ScummVM,
   RPG Maker) are replaced by the fallback.
3. **No implied endorsement.** `TRADEMARKS.md` already disclaims it. This
   work extends that list and adds a paragraph about icons.

Some device shapes could be protected as trade dress; the Game Boy outline
is an example. Small icons next to a text label, used to navigate a
library, are the same widespread practice as RetroArch and RomM. If the
owner ever wants no exposure at all, any single icon can be switched to the
fallback by removing one entry from the source map. This is design
reasoning, not legal advice.

**Brand fit.** Brand §1 bans console shapes from the **identity** (the app
icon, the logo, the landing art), and brand §3 says "system names appear
only as library data inside the UI". System icons follow the same rule:
they are library data. They never appear next to the Coverly mark, on the
landing page, or in marketing images.

### Legal files to update

- **`THIRD-PARTY-NOTICES.md`**
  - Add a "System icons" section: RetroArch Assets, XMB Monochrome theme,
    © libretro contributors, CC BY 4.0, source URL and commit. Changes
    made: recoloured to `currentColor`, optimised with SVGO, viewBoxes
    normalised.
  - Add lucide-react (ISC), which is missing today.
- **`LICENSES/CC-BY-4.0.txt`**: the full licence text.
- **`TRADEMARKS.md`, "Other names"**
  - Add Bandai (WonderSwan), Commodore, Capcom, Microsoft (MSX), IGS,
    Lexaloffle (PICO-8) and ScummVM.
  - Add one sentence: system icons are simplified silhouettes of
    compatible hardware, used only to identify it, and imply no
    endorsement.
- **`docs/09-legal/trademark.md`**: add a short "Third-party systems"
  section that states the rule (hardware silhouettes plus the name, never
  the makers' logos) and the fallback.

## A.2 Pipeline

The icons are vendored into the repo. The app never fetches them at run
time.

```
scripts/icons/systems.mjs          pnpm system-icons
  SYSTEM_ICON_SOURCES (id → RetroArch file, pinned commit)
  → fetch → svgo (multipass, removeDimensions, fills → currentColor,
    strip ids/styles/classes, drop fill:none paths' fill attr)
  → src/assets/system-icons/<id>.svg          (reviewable copies)
  → src/components/system/systemIcons.generated.ts
       export const SYSTEM_ICONS: Record<string, {viewBox, body}>
  → src/assets/system-icons/README.md         (source, commit, changes)
```

- The generated file is committed. The build does not need network access.
- The script fails if any output still contains `#fff`, `#ffffff`,
  `<text`, `<image`, `style=` or a `fill` other than `currentColor` or
  `none`.
- Each icon keeps its own `viewBox` (some are `0 128 64 64`, some
  `0 0 256 256`). The component scales them in CSS; the paths are never
  rewritten.

## A.3 Component API

`src/components/system/SystemIcon.tsx`

```ts
interface SystemIconProps {
  /** Canonical id from systems.toml (`SystemView.system_id`). Null means
   *  a folder Coverly did not recognise. */
  systemId: string | null;
  /** CSS length or px number. Default '1em', so it follows the text. */
  size?: number | string;
  /** When given, the icon stands for the name: role="img" + aria-label.
   *  When absent, the icon is decorative: aria-hidden="true". */
  title?: string;
  className?: string;
}
```

- The component renders an inline
  `<svg data-system-icon={id ?? 'unknown'} fill="currentColor"
  viewBox=…>` with the `body` paths. It is not a `<img>`, so it inherits
  the colour.
- It is a `memo` component with no state and no effects, so it is cheap
  in grids of 12,000 entries. The virtualiser renders only about 60
  of them at once.
- **Key on `system_id`, never on `name`.** The mock data and users' folders
  name systems differently (for example 'NES / Famicom' and 'SFC').

`src/components/system/SystemLabel.tsx` is the icon plus the name. It is
the one-line swap for most sites:

```tsx
<SystemLabel systemId={s.system_id} name={s.name} size={16} />
// <span class="system-label"><SystemIcon aria-hidden/><span>{name}</span></span>
```

- `display: inline-flex; align-items: center; gap: 0.375em; min-width: 0`.
- The name `<span>` truncates with an ellipsis. The icon never shrinks
  (`flex: none`).
- Accessible name: the visible name only, because the icon is
  `aria-hidden`. Existing text assertions in tests stay valid.

Helper: `src/lib/systemLookup.ts` resolves `folder` or `dir` to a
`SystemView`. It is used by surfaces that only carry a folder: the fill
summary and the review queue.

## A.4 Fallback

Three cases, all handled inside `SystemIcon`:

| Case | Example | Renders |
|---|---|---|
| Known id with a source icon | `gb`, `psx` | the silhouette |
| Known id without a usable icon | `cps2`, `mame`, `dos`, `pico8` | **abbreviation mount** |
| User-defined id | `x68000` | abbreviation mount with the first 3–4 letters of the id, upper-case (`X68K` if a map entry exists, else `X680`) |
| `systemId === null` (unrecognised folder) | `Misc/` | lucide `Folder` |

**The abbreviation mount** (`FALLBACK_ABBR` map in
`src/components/system/systemFallbacks.ts`) is the brand's Slot shape made
small:

```
 16px                   24px and up (photo-corners appear, brand §2)
 ┌────┐                 ◤──────◥
 │CPS2│                 │ CPS2 │
 └────┘                 ◣──────◢
```

- A 1px `currentColor` outline with radius 2px. Width fits the text; height
  is 1em.
- Text is Atkinson Hyperlegible Mono, weight 700, at 0.5em, with tabular
  figures.
- At 24px and larger, the four photo-corners from `slot-mark.tsx` are
  added, matching brand §2 (corners appear only at 24px and up).
- Abbreviations: `cps1 CPS1`, `cps2 CPS2`, `cps3 CPS3`, `mame MAME`,
  `hbmame HB`, `dos DOS`, `scummvm SVM`, `easyrpg RPG`, `ons ONS`,
  `openbor BOR`, `pico8 P8`, `atomiswave AW`, `naomi NAO`.
- `fbneo`, `varcade` and `pgm2` use the neutral FBNeo arcade-cabinet
  silhouette, because it is not a logo.

The map lives in the frontend, not in `systems.toml`, so this work does not
touch `crates/coverly-core` while `feat/organize-disc-archives` is open.

## A.5 Colour and size

- **At rest:** `color: var(--cr-text-secondary)`, so it is quieter than
  the name next to it, which uses `--cr-text`.
- **Selected row or tile:** the icon inherits the row's ink
  (`--cr-primary-subtle-fg`, or `--cr-primary-fg` on a solid selection),
  so it is never a lone colour spot.
- **Never** a status colour and never Manila. Buckram is used only through
  selection. Icons are never tinted by maker.
- **Sizes** (all in px; each is `1em` of the surrounding text unless noted):

| Context | Size |
|---|---|
| Table cells, filter checkboxes, compact rows, organise trigger | 16 |
| Cover-tile meta line (12px text) | 14 |
| Sidebar systems tab, Columns view system pane | 16 |
| Group headers, shelf headers | 20 |
| Inspector (DetailDrawer) "System" row, Gallery metadata | 20 |
| Standalone corner badge on S-size covers | 14 + `title` |
| Empty state "No games in {system}" | 48 (`size` prop) |

## A.6 Placement rules

1. **Leading (left of the name) is the default.** That covers lists,
   trees, the sidebar, filter checkboxes, table cells, group and shelf
   headers, and chips. In a column of names, the icons line up on one
   edge and can be scanned like a legend.
2. **Trailing (right of the text) when the game title leads and the system
   is a suffix.** That is the Columns view's game pane when it lists
   several systems at once, the Gallery filmstrip caption and search
   results. Here the icon is shown alone, right-aligned, with `title` set
   to the system name, so the title column stays aligned on the left.
3. **Alone with `title` when there is no room for the name.** That is the
   badge in the top-left corner of S-size covers, and Shelf items.
4. **Never inside native `<option>`s.** They cannot hold SVG. Where a
   native select picks a system, the icon of the current value sits to the
   left of the select, as an adornment.
5. **Never inside interpolated prose** (warnings, "why" notes, toasts).
   The sentence stays plain text.

## A.7 Every site to update

| # | Site | Change | Data |
|---|---|---|---|
| L1 | `CoverGrid.tsx:159` tile meta line | `SystemLabel` 14px, leading, before the region badges | `systems[e.system].system_id` |
| L2 | `EntryTable.tsx:176` system cell | `SystemLabel` 16px | same |
| L3 | `Sidebar.tsx:191` systems tab | `scopeButton` gets an `icon` slot; 16px leading | `view.systems` |
| L4 | `FilterPanel.tsx:203` systems checkboxes | `Option` gets an `icon` slot; between the checkbox and the label | same |
| L5 | `DetailDrawer.tsx:202` "System" row | `SystemLabel` 20px | same |
| L6 | New views (Part B): tiles, compact, mounts, columns, gallery, shelves, group headers | the rules above | same |
| R1 | `ReportsPanel.tsx:118` audit row header | `SystemLabel`; add `system_id` to `AuditRow` in `lib/audit.ts` | `view.systems` |
| F1 | `FillWizard.tsx:513` per-system summary | resolve folder through `systemLookup` → `SystemLabel` | folder |
| F2 | `ReviewQueue.tsx:229` item meta | leading 14px icon before `{folder}` | folder |
| O1 | `organize/SystemGroups.tsx:107` group trigger | insert `SystemIcon` 16px before the system name. **Only this insertion.** | `group.systemId` |
| O2 | `organize/SystemGroups.tsx:180` system select | adornment to the left of the `NativeSelect` (rule 4) | current value |
| O3 | `organize/ReviewBuckets.tsx:141` bucket header | a row of 16px icons, one per candidate system, before the title text, which stays as it is | `systems[]` |

Left alone on purpose:

- `ReviewBuckets` and `LeftInPlace` `<option>`s (rule 4).
- `OrganizeScreen` warnings, `lib/organize.ts` notes and the
  `ScanProgressView` line (rule 5).
- `ProfileEditor` sample input, which is not a display.
- Settings, StatusBar and StartScreen, which show no system names.

---

# Part B. Library views

## B.1 Model

Today `Layout = 'grid' | 'list'`. That becomes a view id plus view
options. Each option is stored for each library.

```ts
export type ViewId =
  | 'covers' | 'tiles' | 'details' | 'compact'
  | 'mounts' | 'columns' | 'gallery' | 'shelves';

export type CoverSize = 'xs' | 's' | 'm' | 'l' | 'xl';      // slider stops
export type GroupBy = 'none' | 'system' | 'folder' | 'region' | 'art' | 'letter';
export type ArtShown = 'cover' | 'title' | 'screenshot';    // = Media

export interface ViewPrefs {
  view: ViewId;
  coverSize: CoverSize;           // covers, tiles, shelves
  groupBy: GroupBy;               // views that group (see B.5)
  art: ArtShown;                  // covers, shelves, gallery
  columns: DetailsColumn[];       // details: visible, ordered
  inspector: boolean;             // docked inspector on/off
}
```

- **Store.** `viewStore` gains `prefs: ViewPrefs` and setters. `layout`
  stays as a derived alias for one release, so existing callers keep
  working while they move to `prefs.view`.
- **Persistence.**
  - One localStorage key, `coverly.views.v1`, holds a JSON object
    `{ last: ViewPrefs, byLibrary: Record<root, ViewPrefs> }`. At most 20
    libraries are kept; the least recently used is dropped.
  - Each read and write is wrapped in try/catch and validated against
    whitelists, so an unknown value falls back to the default.
  - **Migration:** if `coverly.views.v1` is missing, read the old
    `coverly.layout`: `'list'` → `details`, anything else → `covers`.
  - Opening a library uses `byLibrary[root]`, or `last` if that library
    has none. This is Explorer's per-folder memory, scoped to a library
    rather than a folder.
  - `reset()` (library change) keeps the prefs, as it keeps the layout
    today.
- **Shared state.** Selection, scroll anchor, sort, filters and scope stay
  outside the views. Switching view keeps the selected entry and scrolls
  it into view.
- **Grouping.** `src/lib/grouping.ts` is a pure function:
  `(entries, groupBy, systems) → Section[]` with
  `{ key, label, systemId?, start, count }`. It is memoised on
  `(generation, filters, sort, groupBy)`. Every view reads sections from
  it; no view groups entries on its own.

## B.2 The views

Eight views, shown in the switcher in this order. Shortcuts use ⌘ on macOS
and Ctrl elsewhere.

| ⌘ | View | Based on | Shows that no other view shows | Research |
|---|---|---|---|---|
| 1 | **Covers** | current CoverGrid | art at five sizes, from overview to close look | Finder Icon slider; Explorer S/M/L/XL icons; LaunchBox Boxes; Steam S/M/L |
| 2 | **Tiles** | grid variant | small cover plus 3 lines: name, system, art status | Explorer Tiles; GOG grid with info |
| 3 | **Details** | current EntryTable | every column, sortable, column picker, thumbnail column | Finder List; Explorer Details; Playnite/LaunchBox/GOG List |
| 4 | **Compact list** | new, light | most rows on screen (24px rows) | Explorer List; RetroArch list "small"; ES-DE textlist |
| 5 | **Mounts** | new, signature | the 3 art slots of each game side by side | Explorer Content, reworked for Coverly's job |
| 6 | **Columns** | new | the hierarchy: system → folder → game → preview | Finder Column (Miller) |
| 7 | **Gallery** | new | one game's art large, with metadata | Finder Gallery; Playnite Details; RetroArch Ozone thumbnail pane |
| 8 | **Shelves** | new | one row per system, for browsing across the whole library | Steam home shelves; Pegasus rows; Big Box Platform Hybrid |

## B.3 Views not included, and why

| Candidate | Why not |
|---|---|
| Carousel / Wheel / CoverFlow | A pattern for TV and controller launchers. About 5 items per screen, poor for managing 12k entries, and it needs motion that reduced-motion users switch off. Gallery covers the "look closely at one" need. |
| Explorer horizontal "List" (column-major) | Scrolls sideways, which fights the trackpad, the virtualiser and screen-reader reading order. Compact list gives the same density, vertically. |
| Explorer "Content" as is | Same information as Details with a larger thumbnail. Replaced by Mounts. |
| Finder free-arrange icons | Positions mean nothing in a ROM library and cannot be kept across rescans. |
| ES-DE "Video" view | Coverly does not manage videos. |
| Big Box platform wheel | The sidebar's Systems tab plus Shelves already does this. |

## B.4 Wireframes

Shared frame for every view. The toolbar is at the top and the status bar
at the bottom; the inspector is an optional docked pane on the right
(`inspector: true`) that reuses the DetailDrawer content.

```
┌ Sidebar ─────┬ Toolbar: [search] [Filters] [▦ ☰ ▥ ◫ | ⋯] [View ▾]   [Fill] ┐
│ Folders      │                                                │ Inspector  │
│ Systems      │                 <view body>                    │ (optional) │
│  ▣ Game Boy  │                                                │            │
│  ▣ Mega Drive│                                                │            │
└──────────────┴────────────────────────────────────────────────┴────────────┘
  Status bar: 12,048 games · 3,112 without cover
```

### B.4.1 Covers (⌘1)

The current grid. Tile size comes from a five-stop slider; the metrics
below are used by `CoverGrid` instead of the constants it has today.

| Stop | Tile width | Art (3:4) | Meta lines | Roughly how many fit at 1440×900 |
|---|---|---|---|---|
| XS | 96 | 96×128 | none, name in tooltip; system badge | 90 |
| S | 128 | 128×171 | name (1 line); system as corner badge | 45 |
| M (default) | 168 | 168×224 | name (2 lines), system + regions | 24 |
| L | 220 | 220×293 | as M | 12 |
| XL | 300 | 300×400 | as M, plus file size | 6 |

```
 ┌──────┐ ┌──────┐ ┌──────┐ ┌──────┐ ┌──────┐
 │      │ │      │ │╌╌╌╌╌╌│ │      │ │      │
 │ art  │ │ art  │ ╎ none ╎ │ art  │ │ art  │   ← missing art: dashed Slot
 │      │ │      │ │╌╌╌╌╌╌│ │      │ │      │
 └──────┘ └──────┘ └──────┘ └──────┘ └──────┘
 Super Mario  Tetris   Kirby's…  Sonic 2   Zelda…
 ▣ Game Boy·USA ▣ GB·JP  …                       ← icon leads the meta line
```

- The art shown is chosen in View options: cover (default), title screen
  or screenshot. That is the `MEDIA` set the app already has. An entry
  with no art of that type shows the dashed Slot plus its name, as LaunchBox
  falls back to text.
- ⌘+ and ⌘− (or ⌘ + scroll wheel) move one stop. ⌘0 returns to M.

### B.4.2 Tiles (⌘2)

```
 ┌────┐ Super Mario Land          ┌────┐ Tetris
 │art │ ▣ Game Boy                │art │ ▣ Game Boy
 └────┘ ■ cover ■ title ▢ shot    └────┘ ■ cover ▢ title ▢ shot
```

- The cell is 300×88 with a 64×85 cover and three text lines: name,
  `SystemLabel`, and the three art pips (filled = has art, dashed = missing,
  Buckram band = applied). Columns fill the available width.
- The same virtualised row-of-N layout as Covers, with a different cell.

### B.4.3 Details (⌘3)

The current EntryTable, with these additions:

```
 ▾ Name                      System           Size     Modified    C T S
 ─── ▣ Game Boy (412) ────────────────────────────────────────────────────  ← group row
 ▫  Kirby's Dream Land       ▣ Game Boy       256 KB   2024-03-01  ■ ▢ ▢
 ▫  Super Mario Land         ▣ Game Boy       64 KB    2023-11-20  ■ ■ ▢
```

- **Column picker:** right-click any header, or open View options →
  Columns. Columns can be shown, hidden and reordered. Name is always
  shown. Available columns:
  - **Thumb**: a 24px cover. This is the LaunchBox feature request.
  - **Name**, **System**, **Folder**, **Regions**, **Extension**,
    **Size**, **Modified**.
  - **Cover**, **Title**, **Screenshot**: the art marks.
- Columns resize by dragging the header edge, and widths are kept in
  `ViewPrefs`. Double-click the edge to fit the column to its content.
- Sorting by clicking a header works as it does today.
- When grouping is on, group rows are sticky (the shelf rule: 1px border
  with a 3px Buckram cap on the left, brand §2).

### B.4.4 Compact list (⌘4)

```
 ▣ Kirby's Dream Land                                  ■▢▢
 ▣ Super Mario Land                                    ■■▢
 ▣ Tetris                                              ■▢▢
```

- Rows are 24px (34px in Details), with a 16px system icon, the name and
  the art pips. No headers and no columns.
- This is the "how many games is that" view: about 30 rows on a laptop
  screen, against 20 in Details.

### B.4.5 Mounts (⌘5), the signature view

The view built for Coverly's job. Each row is an album page: the three art
slots of one game, side by side, each drawn in its Slot state.

```
 ┌────┐ ┌──────────┐ ┌──────────┐  Super Mario Land
 │cov │ │  title   │ ╎ ╌╌╌╌╌╌╌╌ ╎  ▣ Game Boy · USA · 64 KB
 │    │ │  screen  │ ╎ missing  ╎  SML.gb
 └────┘ └──────────┘ └╌╌╌╌╌╌╌╌╌╌┘
 ┌────┐ ┌──────────┐ ┌──────────┐  Tetris
 │cov ◥│ │  title   │ │  shot    │  ▣ Game Boy · World
 └────┘ └──────────┘ └──────────┘   ◥ = Manila tab, applied this run
```

- Fixed 112px rows: cover 3:4 (72×96) and title and screenshot 4:3
  (128×96), then the text.
- Slot states are exactly those of brand §2: dashed with corners = missing,
  solid = has art, Buckram bottom band = applied, Manila corner tab =
  applied this run. No new visual language.
- Selecting a missing slot and pressing Enter opens the ArtWorkspace on
  that media. This view leads straight to the fix.
- It uses three thumbnails per row, so it loads at most about 3 × 10
  images per screen.

### B.4.6 Columns (⌘6)

Finder's Miller columns. The hierarchy comes from data the app already
has.

```
 ┌ Systems ─────────┬ Folders ───────┬ Games ──────────────┬ Preview ─────────┐
 │ ▣ Game Boy    412│ ▸ (root)    380│ Kirby's Dream Land  │  ┌──────────┐    │
 │▶▣ Game Boy Adv 980│▶  Hacks       32│▶Super Mario Land    │  │  cover   │    │
 │ ▣ Mega Drive  1.2k│                │ Tetris              │  └──────────┘    │
 │ ▢ Misc/          7│                │                     │  ▣ Game Boy · USA │
 └──────────────────┴────────────────┴─────────────────────┴──────────────────┘
```

- Panes:
  1. Systems, in sidebar order, with a 16px icon and a count.
  2. Sub-folders inside that system (from `folderTree.ts`). This pane is
     left out when the system has no sub-folders.
  3. Games.
  4. Preview: the art for the shown media, plus the inspector fields.
- When the sidebar scope is already one system, pane 1 is hidden.
- With grouping set to `region` or `letter`, pane 2 shows those buckets
  instead of folders.
- Each pane is its own virtualised list. Only the selected path is
  rendered.
- Keyboard: ↑↓ within a pane, → or Enter on a branch goes into the next
  pane, ← goes back, and typing letters jumps to a name.
- Panes are 220px wide (the games pane is flexible). If the window is too
  narrow for all of them, the left panes scroll out of view, as in Finder.

### B.4.7 Gallery (⌘7)

```
 ┌──────────────────────────────────────────────┬ Metadata ──────────┐
 │                                              │ Super Mario Land   │
 │               large art                      │ ▣ Game Boy         │
 │          (cover / title / shot)              │ USA · 64 KB · .gb  │
 │                                              │ Cover   ■ has      │
 │                                              │ Title   ■ has      │
 ├──────────────────────────────────────────────┤ Shot    ╎ missing  │
 │ ◂ ▢ ▢ ▣ [▣] ▣ ▢ ▣ ▢ ▣ ▢ ▣ ▢ ▣ ▸   filmstrip  │ [Fill this game]   │
 └──────────────────────────────────────────────┴────────────────────┘
```

- The filmstrip is a horizontal virtualiser with 64px thumbnails. Only one
  large image loads at a time, and the large image is requested only after
  selection has stayed still for 120 ms, so fast arrowing does not queue
  up images.
- ←→ moves along the filmstrip. Space opens the full-size art (Quick Look),
  and Tab moves between the art tabs (cover, title, screenshot).
- Group boundaries show as a thin shelf-rule marker in the filmstrip, with
  the group name in a tooltip.

### B.4.8 Shelves (⌘8)

```
 ─── ▣ Game Boy · 412 ───────────────────────────────────── See all ─
 ┌──┐┌──┐┌──┐┌──┐┌──┐┌──┐┌──┐┌──┐┌──┐┌──┐ ▸
 └──┘└──┘└──┘└──┘└──┘└──┘└──┘└──┘└──┘└──┘
 ─── ▣ Mega Drive · 1,204 ────────────────────────────────── See all ─
 ┌──┐┌──┐┌──┐┌──┐┌──┐┌──┐┌──┐┌──┐┌──┐┌──┐ ▸
```

- One shelf per section. Shelves group by system by default; folder,
  region, art status or letter can be chosen instead. The header uses the
  shelf rule plus a 20px icon.
- "See all" sets the scope to that system and switches to Covers.
- Each shelf shows at most 60 items and scrolls sideways, virtualised.
  Shelves themselves are virtualised vertically.
- ↑↓ moves between shelves and keeps the column position; ←→ moves within
  a shelf.
- The default sort inside a shelf is the current sort. "Missing art first"
  is a useful choice here and is one click away in View options.

## B.5 Group-by and sort

| View | Group-by | Sort |
|---|---|---|
| Covers, Tiles, Compact, Mounts | yes: sticky section header rows | current sort within each group |
| Details | yes: sticky group rows | click headers |
| Columns | grouping sets what pane 2 shows (folder by default) | sorts the games pane |
| Gallery | filmstrip markers only | yes |
| Shelves | always grouped; `none` is shown as `system` | sorts within each shelf |

- Group keys:
  - `system`: in sidebar order, with an icon.
  - `folder`: the path relative to the system.
  - `region`: in the order of the region list.
  - `art`: "Missing cover", "Missing other art", "Complete" — the order
    that leads to work.
  - `letter`: `#`, then A–Z.
- Groups come first; within a group, the current sort applies.
- **Section header rows** are the shelf rule, the group label (with a
  `SystemIcon` when grouping by system) and a count. A group cannot be
  collapsed in v1. Collapsing changes row indices, so it is a follow-up
  with its own keyboard rules.

## B.6 View switcher UX

```
 [ ▦ | ☰ | ▥ | ◫ ]  [⋯ ▾]   [◐ View options ▾]
   1    3   6   7    more
```

- **Segmented control** (the current `SegmentedControl`, Radix
  toggle-group) with Finder's four: Covers, Details, Columns, Gallery.
  These icons are icon-only with a tooltip, using the current pattern.
- **"More views" menu** (`DropdownMenu`, `DropdownMenuRadioGroup` sharing
  the same value) with Tiles, Compact list, Mounts and Shelves.
  - When one of these is active, the trigger shows its icon and is
    pressed, so the current view is always visible.
  - Each menu item shows its shortcut (`Kbd`).
- **View options** popover (`Popover`). It shows only the controls that
  apply to the current view:
  - Size slider with five stops (`Slider`), labelled XS to XL. Covers,
    Tiles and Shelves.
  - Show art: cover / title / screenshot (`RadioGroup`).
  - Group by (`Select`).
  - Sort by (`Select` plus a direction toggle). Not shown in Details,
    where headers sort.
  - Columns (Details only): checkbox list, reordered with ↑ and ↓
    buttons. No drag-and-drop is needed in v1.
  - Inspector pane (`Switch`).
- **Narrow windows (below 1180px):** the segmented control and "More"
  collapse into one `View ▾` menu that lists all eight. This matches how
  the toolbar labels already collapse.
- **Shortcuts:**
  - ⌘1–⌘8 switch views.
  - ⌘+ / ⌘− / ⌘0 change the size.
  - ⌘I toggles the inspector.
  - Handled in `LibraryScreen`'s existing `keydown` listener. They are
    ignored while focus is in an input, a textarea or a dialog, except
    ⌘1–8, which never insert text.
  - The Tauri webview has no default zoom shortcuts, so ⌘+ and ⌘− are
    free. Check this in the desktop app.
- **Lucide icons per view:** `LayoutGrid` (Covers), `LayoutList` (Tiles),
  `Table2` (Details), `List` (Compact), `GalleryHorizontalEnd` (Mounts),
  `Columns3` (Columns), `GalleryThumbnails` (Gallery), `Library`
  (Shelves). The "more" trigger is `Ellipsis`; View options is
  `Settings2`.

## B.7 Performance at 12,000 entries

- **Never render the full list.** Every view uses `@tanstack/react-virtual`
  with fixed sizes per view and per size stop, so offsets are O(1) and need
  no measuring:
  - Grid rows of N for Covers and Tiles.
  - Fixed rows for Details (34), Compact (24) and Mounts (112).
  - Section header rows are 32 in every view.
- **Grouping is computed once.** `grouping.ts` is memoised and returns
  index ranges, not copies of the entries. Header rows go into a flat
  `RowModel[]` (`{kind:'header', section} | {kind:'items', start, end}`),
  which the virtualiser reads. Sticky headers use `rangeExtractor`.
- **Thumbnails.**
  - Use the existing `ArtThumb` lazy loading and cache.
  - Mounts and Gallery request at most what is on screen, plus one screen
    of overscan.
  - If the IPC has no thumbnail size, request the full image and scale it
    in CSS. Size buckets on the backend are a follow-up.
- **Switching view keeps work low.** Selection and sort live in the store,
  so a switch mounts one new virtualiser and scrolls to the selected index.
  Target: under 100 ms at 12k (React profiler, mock).
- **Columns view:** only the selected system's entries are filtered. Folder
  buckets come from a memoised `folderTree` for each system.

## B.8 Accessibility

| View | Roles | Keyboard |
|---|---|---|
| Covers, Tiles, Shelves | `role="grid"`, rows, `gridcell`s, `aria-activedescendant` (current pattern) | 2D arrows via `nextIndex`, Page Up/Down, Home/End, Enter opens, type-ahead |
| Details | `role="grid"` with `columnheader` + `aria-sort` (current) | as today |
| Compact, Mounts | `role="grid"`, one column; in Mounts each art slot is a `gridcell` (←→ moves between slots) | ↑↓ rows, ←→ slots in Mounts, Enter |
| Columns | each pane is a `listbox` with an `aria-label` from the pane name, `aria-activedescendant` | ↑↓, → / Enter go in, ← goes back, type-ahead |
| Gallery | filmstrip is a `listbox`, `aria-orientation="horizontal"`; the large art is an `img` with alt "{name}, {media}" | ←→, Home/End, Space |

- **Section headers** are rows with `role="rowheader"`, labelled
  "{group}, {n} games". Arrow keys skip them. ⌥↑ and ⌥↓ (Alt) jump to the
  previous or next group.
- `aria-rowcount` and `aria-rowindex` are set to the real totals, so
  screen readers report the full size despite virtualisation.
- **The switcher** uses radio semantics (toggle-group) for the four
  segments, and `menuitemradio` items in the menu. Both share one value,
  and the active state is announced.
- **View change announcement:** a polite live region says "Columns view"
  ("Vista de columnas").
- **Focus:** after a view switch, focus goes to the new view's container,
  and the selected item stays active.
- **System icons** are `aria-hidden` next to a name, and `role="img"`
  with a label when shown alone (A.3).
- **Contrast:** icons use `--cr-text-secondary`, which is 6.38:1 in light
  mode and above 3:1 in both themes, so they pass WCAG 1.4.11 for graphics.

## B.9 Reduced motion

- With `prefers-reduced-motion: reduce`:
  - size-slider changes snap with no transition;
  - Gallery changes its large image instantly (no crossfade);
  - `scrollToIndex` uses `behavior: 'auto'`;
  - the switch to a new view does not fade.
- Without it, the only motion is a 120 ms crossfade of the Gallery image
  and the existing tile hover. No load-time animation is added.

## B.10 Copy (i18n)

New keys go under `layouts.*`, because `views.*` is already used by the
sidebar tabs. The existing `toolbar.layout`, `toolbar.grid` and
`toolbar.list` are kept until the last caller moves over, then removed.
`locales/es/ui.json` and `locales/en/ui.json` change together.

| Key | es | en |
|---|---|---|
| `layouts.label` | Vista | View |
| `layouts.covers` | Portadas | Covers |
| `layouts.tiles` | Mosaico | Tiles |
| `layouts.details` | Detalles | Details |
| `layouts.compact` | Lista compacta | Compact list |
| `layouts.mounts` | Huecos de arte | Art slots |
| `layouts.columns` | Columnas | Columns |
| `layouts.gallery` | Galería | Gallery |
| `layouts.shelves` | Estantes | Shelves |
| `layouts.more` | Más vistas | More views |
| `layouts.options` | Opciones de vista | View options |
| `layouts.size` | Tamaño | Size |
| `layouts.sizeStops` | Mínimo · Pequeño · Mediano · Grande · Máximo | Smallest · Small · Medium · Large · Largest |
| `layouts.art` | Arte que se muestra | Art shown |
| `layouts.artCover` / `artTitle` / `artScreenshot` | Portada / Pantalla de título / Captura | Cover / Title screen / Screenshot |
| `layouts.groupBy` | Agrupar por | Group by |
| `layouts.group.none` / `system` / `folder` / `region` / `art` / `letter` | Sin agrupar / Sistema / Carpeta / Región / Estado del arte / Inicial | None / System / Folder / Region / Art status / First letter |
| `layouts.group.missingCover` / `missingOther` / `complete` | Sin portada / Falta otro arte / Completo | No cover / Other art missing / Complete |
| `layouts.groupCount` | {group}, {n} juegos | {group}, {n} games |
| `layouts.sortBy` | Ordenar por | Sort by |
| `layouts.columnsPicker` | Columnas visibles | Visible columns |
| `layouts.column.thumb` / `folder` / `regions` / `ext` | Miniatura / Carpeta / Regiones / Extensión | Thumbnail / Folder / Regions / Extension |
| `layouts.inspector` | Panel de detalles | Details pane |
| `layouts.seeAll` | Ver todo | See all |
| `layouts.announce` | Vista: {view} | {view} view |
| `layouts.pane.systems` / `folders` / `games` / `preview` | Sistemas / Carpetas / Juegos / Vista previa | Systems / Folders / Games / Preview |
| `layouts.slotMissing` | Falta {media} | No {media} |
| `systems.unknown` | Carpeta sin sistema reconocido | Folder with no known system |

Notes on the names:

- **Mounts** is the code id. The label shown to users is "Art slots" /
  "Huecos de arte", because it describes what the view shows. "Mounts" is
  the brand's metaphor, not a word users search for.
- "Mosaico" is Explorer's Spanish name for Tiles. "Detalles", "Columnas"
  and "Galería" match Finder and Explorer in Spanish.
- The `layouts.sizeStops` row lists five separate labels, one per stop.

---

# Part C. Test plan

The gate is `pnpm check-all`: tokens, lint, format, typecheck and vitest.

## C.1 Unit

- **`systemIcons` data**
  - Every `id` in `crates/coverly-core/src/systems/systems.toml` (parsed
    in the test) resolves either to `SYSTEM_ICONS[id]` or to
    `FALLBACK_ABBR[id]`. This is checked in both directions: no map entry
    may point at an id that does not exist.
  - No generated `body` contains `#fff`, `#ffffff`, `<text`, `<image`,
    `style=` or `fill="` with any value other than `currentColor` or
    `none`.
  - Every `viewBox` matches `^-?\d+ -?\d+ \d+ \d+$`.
- **`SystemIcon`**
  - A known id renders `svg[data-system-icon="gb"]` with `aria-hidden`.
  - With `title`, it renders `role="img"` and the accessible name.
  - A fallback id renders the abbreviation text.
  - At `size >= 24` it draws the photo-corners, and below that it does
    not.
  - `null` renders the folder glyph.
  - A user-defined id renders an abbreviation made from the id.
- **`SystemLabel`**: the accessible name equals the name. A long name
  truncates, and the icon has `flex: none`.
- **`viewStore`** (new file `viewStore.test.ts`, using
  `src/test/storage.ts`)
  - Defaults are right.
  - Migration from `coverly.layout`: `'list'` → `details`, `'grid'` →
    `covers`, and a junk value → `covers`.
  - Unknown values in `coverly.views.v1` fall back field by field.
  - Prefs are saved per library, and a new library falls back to `last`.
  - Libraries beyond 20 are dropped, least recently used first.
  - Storage that throws (stubbed) does not crash.
  - `reset()` keeps the prefs.
- **`grouping.ts`**
  - Each `GroupBy` produces the correct sections, order and counts on a
    fixture.
  - Groups come first and the sort applies inside them.
  - Unknown systems end up in their own group.
  - It is stable across calls (memoised).
  - It handles 12,000 entries in under 30 ms (perf smoke test).
- **`keyboardNav`**: header rows are skipped; ⌥↑ and ⌥↓ jump to groups;
  the Columns pane moves (← and →).
- **Each new view component** (`stubLayout(w, h)`)
  - Renders about one screen of rows for 12,000 entries, not 12,000.
  - Arrow, Home/End and Page keys move the active item; Enter calls
    `onOpen`.
  - The roles and `aria-activedescendant` from §B.8 are set.
  - Mounts: missing slots show the dashed state, and Enter on a slot opens
    the ArtWorkspace on that media.
  - Covers: each size stop produces the expected column count at a fixed
    width.
  - Details: the column picker hides and shows columns; sticky group rows
    render.

## C.2 Integration (`LibraryScreen.test.tsx`)

- Switching views:
  - the four segments and the four menu items all switch views;
  - the active view is shown pressed in the segmented control or on the
    "more" trigger;
  - the live region announces the new view.
- ⌘1–⌘8 and Ctrl+1–8 switch views, and do nothing while focus is in the
  search box (except ⌘1–8, see §B.6).
- The selection survives a switch, and the selected item is active and
  scrolled into view in the new view.
- **Persistence:** pick Columns, unmount, mount again, and Columns is
  restored. Two libraries keep separate prefs.
- **Group-by:** choosing "System" shows section headers with
  `svg[data-system-icon]` in Covers, Details and Shelves.
- At narrow widths (below 1180px) the switcher collapses into one menu.
- **Existing tests:** update the `radiogroup 'Layout'` test in
  `LibraryScreen.test.tsx:134` for the new names. The text assertions on
  system names in DetailDrawer, Reports, FillWizard, ReviewQueue and the
  organise tests should still pass unchanged, because the icons are
  `aria-hidden`. Add an icon-presence assertion to each.

## C.3 E2E (Chrome DevTools MCP, isolatedContext `views`)

Mock web app on port 1457:
`pnpm exec vite --port 1457 --strictPort`. Stop it with
`lsof -ti tcp:1457 -sTCP:LISTEN | xargs kill`.

1. Load the mock library at about 12,000 entries (the mock fixture's large
   library). For each of the eight views:
   - take a snapshot;
   - check the role tree;
   - count the rendered rows with `evaluate_script`. It must stay under
     about 200 DOM rows or tiles.
2. Icons:
   - `document.querySelectorAll('[data-system-icon]')` appears next to
     system names in the sidebar Systems tab, the Filters panel, the
     Details system column, the cover-tile meta line, the inspector,
     Reports, the fill summary and the organise group triggers;
   - each one has `aria-hidden="true"` or a `role="img"` with a label.
3. Keyboard:
   - ⌘1…⌘8 cycle through the views;
   - in Columns, → and ← walk the panes;
   - in Gallery, ←→ moves along the filmstrip;
   - in Shelves, ↑↓ keeps the column.
4. **Performance:** record a trace while switching from Covers to Details
   to Gallery to Shelves and scrolling Covers to the end. Check that no
   long task exceeds 200 ms and that the view switch takes under 100 ms of
   scripting.
5. **Persistence:** choose Mounts with size L, reload the page, and the
   same view and size come back.
6. **Reduced motion:** `emulate` with `prefers-reduced-motion: reduce`.
   The Gallery image changes with no transition (check computed
   `transition-duration`).
7. **Visual check, light and dark** (`emulate` colour scheme): take
   screenshots of every view at 1440×900 and 1024×768, plus the sidebar
   with icons. Check that:
   - icons are monochrome and follow the text colour when selected;
   - no icon shows the maker's colours;
   - the fallback abbreviations are legible at 16px;
   - the dashed Slots in Mounts pass 3:1 against their surface.
8. **Console:** no errors or React key warnings in any view.

## C.4 Desktop check (Tauri, manual)

- ⌘1–⌘8, ⌘+ and ⌘− do not trigger webview zoom or the native menu.
- Icons render in the same weight in the WebKit webview as in Chromium.

---

## D. Delivery order (granular commits)

1. `build(icons): vendor RetroArch monochrome system icons with a generator script`
2. `docs(legal): attribute system icons and extend trademark notes`
3. `feat(system): add SystemIcon and SystemLabel with Slot fallback`
4. `feat(library): show system icons in sidebar, filters, grid, table and drawer`
5. `feat(reports,fill): show system icons in audit and fill summaries`
6. `feat(organize): show system icon on system group triggers`
7. `feat(views): per-library view prefs with layout migration`
8. `feat(views): grouping model and section headers`
9. `feat(views): cover size slider and Tiles view`
10. `feat(views): Details column picker and thumbnail column`
11. `feat(views): Compact list and Art slots views`
12. `feat(views): Columns view`
13. `feat(views): Gallery view`
14. `feat(views): Shelves view`
15. `feat(toolbar): view switcher with more-views menu, options and shortcuts`
16. `test(views): store, grouping, keyboard and integration coverage`

Each commit passes `pnpm check-all` on its own.
