# Component audit and migration plan

Every control in the app is a raw HTML element styled by hand-written classes
in `src/styles/globals.css` (one 1695-line file). Checkboxes, radios, selects
and progress bars render with the OS look, focus styling is inconsistent, and
several fields do not link their hint or error. This page lists every
occurrence, maps it to a shadcn-style primitive in `src/components/ui`, and
orders the migration so each step is a reviewable PR that keeps the tests
green.

Line numbers refer to the tree at the time of the audit (2026-10-17). Grep
before editing; they drift.

## Summary

Across 30 non-test `.tsx` files:

| Control | Count |
|---|---|
| `<button>` | 104 (102 `type="button"`, 2 `type="submit"`) |
| `<input>` | 38: 17 checkbox, 8 radio, 7 text, 2 search, 2 date, 1 number, 1 password |
| `<select>` | 10 |
| `<textarea>` | 1 |
| `<progress>` / `<meter>` | 4 / 1 |
| `<details>` | 1 |
| `<fieldset>` / `<label>` | 16 / 33 |
| Custom `<Modal>` usages | 7 (+ 3 inline confirms) |
| Hand-rolled tablists | 3 |
| Segmented control | 1 |
| Banner styles | 4 (`error-banner`, `notice-banner`, `update-banner`, `pause-banner`) |
| Badges / pills | 4 kinds + ~6 plain-text states |
| Native `title=` tooltips | 28 |

There is no Radix, cva, clsx, tailwind-merge or icon library today. Tailwind
is barely used: `theme.extend` is empty and the TSX has a handful of layout
utilities.

### Cross-cutting accessibility problems

1. **Focus ring.** Only `.art-ws__slot`, `.art-ws__candidate`, `.art-ws__btn`,
   `.art-ws__link`, `.art-ws__more`, `.drawer-reveal`, `.art-ws__input`,
   `.cover-grid` and `.entry-table` have a custom `:focus-visible`
   (`globals.css:294`, `:811-818`, `:995`). Every other button, input, select,
   checkbox, radio, tab and list row falls back to the WebKit default ring,
   which ignores the accent. One `focus-visible:ring-2 ring-ring
   ring-offset-2 ring-offset-background` in the primitives fixes all of them.
2. **Destructive confirms look safe.** `ProfilesSection.tsx:145` (delete
   profile), `AdvancedSection.tsx:114` (reset all settings) and
   `NetworkSection.tsx:157` (clear cache) use `primary-button`.
3. **Unlinked hints and errors.** `Field.tsx:96` (NumberSetting) is the only
   field wiring `aria-describedby`; the 12 other `<Field>` usages with a
   `hint` or `error` do not.
4. **Modal.** No inert background, no scroll lock, no portal, no
   `aria-describedby`, initial focus on the container, window-level Escape
   listener that stacks with `LibraryScreen.tsx:82` and `ReviewQueue.tsx:121`,
   and `role="dialog"` on destructive confirms.
5. **Icon-only buttons** (↑ ↓ × text glyphs) have no tooltip, and native
   `title` is never shown to keyboard users.
6. **Unnamed group:** the remove-art confirm at `ArtWorkspace.tsx:639` is
   `role="group"` with no accessible name.
7. **Disabled state** is only `opacity: .5`, with nothing saying why.
8. **Hard-coded status colours** fail contrast: white on `#16a34a`
   (`.art-ws__inuse`) is 3.3:1; `#dc2626` on the dark surface is 3.45:1.

## Migration plan

Columns: primitive, occurrences, shadcn target, accessibility fixes, tests at
risk. Order is by dependency first, then value against risk.

| # | Primitive | Count | Risk |
|---|---|---|---|
| 0 | Tokens + `cn()` + Tailwind mapping | — | Low |
| 1 | Button | 104 | Low |
| 2 | Dialog + AlertDialog | 7 + 3 | **High** |
| 3 | Input / Textarea / Label / Field | 13 + 1 + 33 | Low |
| 4 | Switch / Checkbox | 17 | Medium |
| 5 | RadioGroup / ToggleGroup | 8 + 1 + candidate pickers | Medium |
| 6 | Tabs | 3 | Medium |
| 7 | Select (styled native first) | 10 | High if Radix |
| 8 | Alert + Sonner | ~25 banners, 6 transient | Medium |
| 9 | Badge / SlotMark | 4 kinds + ~6 states | Low |
| 10 | Progress | 4 + 1 meter | Low |
| 11 | Tooltip | 28 `title` (~12 must change) | Low |
| 12 | Card / Table / Skeleton / Collapsible / ScrollArea / Separator / EmptyState | ~20 | Low |
| 13 | DropdownMenu / Command (new) | — | Low |

**Leave custom:** `CoverGrid` and `EntryTable` (virtualised `role=grid` with
`aria-activedescendant`, correct and covered by class-based tests),
`ArtThumb`, `CandidateImage`. Restyle them with the new tokens only. Keep
native scrollers there: `@tanstack/react-virtual` needs the real element.

### 0. Tokens, `cn()` and the Tailwind mapping

- **Files:** `src/styles/globals.css`, `tailwind.config.ts`, new
  `src/lib/utils.ts`.
- **Work:** add channel variants of the tokens, map shadcn's class names onto
  `--cr-*` (see [Setup](#setup-under-tailwind-34)), add radius, ring, success,
  warning, info, overlay and source tokens.
- **Replaces these hard-coded values:** `#16a34a` ×6 (lines 429, 867-868,
  1109, 1160, 1162), `#dc2626` ×3 (252, 433, 872; duplicates `--cr-error`
  with another red), `#0e7490` / `#7c3aed` / `#b45309` (1331-1341),
  `rgb(234 179 8 / .15)` (1260), `rgb(202 138 4)` (1277),
  `rgb(0 0 0 / .4)` (270), `rgb(127 127 127 / .12)` (1389), and the dead
  fallbacks `rgb(127 127 127 / .25)` (1228) and `rgb(59 130 246)` (1313).
- **Also:** `color-mix()` (lines 252, 1557, 1688) needs Safari 16.2; macOS
  11/12 WKWebView does not have it. Emit pre-mixed token values instead.
- **Tests at risk:** none. `src/lib/theme.test.ts` checks `.dark`, which
  shadcn also uses.

### 1. Button — 104

| Current class | Count | Target |
|---|---|---|
| `primary-button` | 18 | `variant="default"` |
| `ghost-button` | 52 | `variant="outline"` (it has a border; shadcn `ghost` does not) |
| `icon-button` | 10 | `variant="outline" size="icon"` |
| `art-ws__btn` (+ `--primary`, `--danger`) | 9 | `size="sm"` + `default` / `destructive` |
| `ghost-button danger` | 1 (`AdvancedSection.tsx:95`) | `destructive` (or an outline-destructive variant) |
| `art-ws__link` | 1 (`ArtWorkspace.tsx:448`) | `variant="link"` |
| `art-ws__more` | 1 (`ArtWorkspace.tsx:557`) | `variant="outline"`, full width |

Styling sources to delete afterwards: `.primary-button` (81-96),
`.ghost-button` (142-155), `.icon-button` (157-161), `.art-ws__btn`
(912-950), `.ghost-button.danger` (1509).

**Occurrences:**

- `Toolbar.tsx` — 56 Filters toggle (`aria-pressed` + `aria-controls`), 86
  Fill (primary), 89 Jobs (label changes running/paused), 96 Reports, 99
  Settings, 108 Close.
- `StartScreen.tsx` — 94 Settings, 109 Open folder (hero CTA → `size="lg"`),
  187 forget recent (`icon-button` "×").
- `ErrorBoundary.tsx:10` — Reload.
- `UpdateBanner.tsx` — 18, 28.
- `SettingsScreen.tsx` — 93 retry, 113 Back, 133 dismiss notice, 145 reopen
  now.
- `DetailDrawer.tsx` — 81, 183 (`art-ws__btn`), 132 reveal-in-folder (inline
  SVG + `title`), 155 close ("×").
- `ArtWorkspace.tsx` — 529 / 538 replace confirm, 642 / 649 remove confirm,
  663 From file, 672 Remove, 709 Search submit.
- `FillWizard.tsx` — 288 / 297 region ↑ ↓; 446-505 footer Back / Discard /
  Close / Next / Preview / Review / Run.
- `MatchingSection.tsx` — 69 / 78 / 87 ↑ ↓ ×. `SourcesSection.tsx` — 219 /
  228 ↑ ↓; 98 / 107 / 117 save / remove / test key.
- `JobsPanel.tsx` — 165, 182, 190 cancel / resume; 238, 245 export CSV /
  JSON; 274 / 282 undo confirm; 293 Undo; 347 Close.
- `ReviewQueue.tsx` — 253-293 Previous / Next / Skip / Accept all / Accept /
  Done.
- `ReportsPanel.tsx` — 95, 102, 113, 120, 138.
- `UpdatePanel.tsx` — 91, 100, 109, 119.
- `AboutSection.tsx:39` — external links as buttons → `variant="link"` +
  external-link icon.
- `AdvancedSection.tsx` — 53, 60 open logs / config; 95 reset; 107 / 114
  modal actions.
- `NetworkSection.tsx` — 122 clear cache; 150 / 157 confirm.
- `SafetySection.tsx:55` — choose folder. `LibrarySection.tsx:169` — forget
  recent.
- `ProfilesSection.tsx` — 64 new, 80 duplicate, 100 edit, 110 delete, 138 /
  145 delete confirm.
- `ProfileEditor.tsx` — 363 cancel, 366 Save (`type="submit"`, `aria-busy`).

**Accessibility fixes:**

- Brand focus ring on all 104.
- `variant="destructive"` on `ProfilesSection.tsx:145`,
  `AdvancedSection.tsx:114`, `NetworkSection.tsx:157`.
- Replace ↑ ↓ × glyphs with lucide `ChevronUp` / `ChevronDown` / `X`, each with
  an `aria-label` and a Tooltip; announce reorders in a live region.
- `Toolbar.tsx:56`: use `aria-expanded` (a disclosure), not `aria-pressed`.
- `ProfileEditor.tsx:366`: visible spinner slot, not only `aria-busy`.
- Disabled buttons that need a reason (e.g. ReviewQueue Accept) get a
  Tooltip.
- `ReviewQueue.tsx:251`: show the `j`/`k`/`s`/`a`/`1-9` shortcuts as `Kbd`
  chips instead of a `.hint` line.

**Tests at risk:** none expected. The primitive stays a native `<button>` with
the same text and `aria-*`, so the 241 `getByRole('button')` queries keep
passing. `LibraryScreen.test.tsx:219` uses `.closest('button')` on a sidebar
item: fine unless `asChild` is used.

### 2. Dialog and AlertDialog — replaces `src/components/Modal.tsx`

| file:line | Purpose | Target |
|---|---|---|
| `FillWizard.tsx:430` | 5-step wizard (`.wizard`) | `Dialog` + step indicator |
| `ReviewQueue.tsx:171` | candidate review (`.review`) | `Dialog` (large) |
| `JobsPanel.tsx:134` | jobs / undo / history (`.jobs`) | `Dialog` (or `Sheet side="right"`) |
| `ReportsPanel.tsx:60` | reports (`.reports`) | `Dialog` |
| `AdvancedSection.tsx:103` | reset settings | **`AlertDialog`** |
| `NetworkSection.tsx:145` | clear cache | **`AlertDialog`** |
| `ProfilesSection.tsx:130` | delete profile | **`AlertDialog`** |
| `ArtWorkspace.tsx:520-547` | inline replace confirm | `Popover`, or keep inline in a `Card` |
| `ArtWorkspace.tsx:639-658` | inline remove confirm (unnamed group) | **`AlertDialog`** |
| `JobsPanel.tsx:270-291` | undo confirm, nested in a dialog | `AlertDialog` or inline `Alert` |

Styling sources: `.modal-backdrop` (265-271, no z-index), `.modal`
(273-278), `.modal-actions` (280-285), width overrides on `.wizard` (~1178),
`.review`, `.jobs` (~1265), `.reports` (~1345). After the migration delete
`Modal.tsx` and `src/lib/focusTrap.ts` (only Modal imports `trapTab`).

**Accessibility fixes:** portal, inert and `aria-hidden` background, scroll
lock, `DialogTitle` + `DialogDescription` (`aria-describedby`), Escape scoped
to the dialog, `role="alertdialog"` for destructive confirms with focus on the
safe action, enter/exit animation that respects reduced motion, overlay from a
`--cr-overlay` token.

**Tests at risk:**

- All `getByRole('dialog')` tests: `FillWizard.test.tsx` (11×, name "Fill
  missing art"), `ReviewQueue.test.tsx`, `JobsPanel.test.tsx`,
  `ReportsPanel.test.tsx`, `ProfilesSection.test.tsx:165`,
  `Sections.test.tsx:378`, `439`, `448`.
  - The portal moves content to `body`: `screen` queries work, `within(container)`
    does not.
  - Radix hides all siblings (`hideOthers`), so `screen.getByRole` for anything
    outside an open dialog returns nothing. Audit tests that touch the
    background with a dialog open.
  - The three confirms become `alertdialog`: update
    `ProfilesSection.test.tsx:165` and `Sections.test.tsx:378`, `439`, `448`.
- `toHaveFocus` assertions (JobsPanel undo focus return, ArtWorkspace confirm
  focus, `App.tsx:39` `[data-settings-opener]`): Radix focuses the first
  focusable on open. Keep today's targets (`keepRef`, `replaceRef`, `undoRef`)
  with `onOpenAutoFocus` / `onCloseAutoFocus`.
- i18n: `dialog.tsx` ships `<span className="sr-only">Close</span>`; route it
  through `useT()` and add the key to `locales/en/ui.json` and
  `locales/es/ui.json` so `src/i18n/parity.test.ts` stays green.

### 3. Input, Textarea, Label and the Field anatomy

| file:line | Type | Fix |
|---|---|---|
| `Toolbar.tsx:46` | search | Leading search icon, `Kbd` "/" suffix (`/` and ⌘F focus it, `LibraryScreen.tsx:85`) |
| `ArtWorkspace.tsx:699` | search | Keep `sr-only` label and glyph; drop `.art-ws__input` (995) |
| `Field.tsx:96` | text, `inputMode=decimal` | Already linked; becomes the model |
| `ArtSection.tsx:70` | text, numeric, table cell | Link the `role=alert` error via `aria-describedby` |
| `MatchingSection.tsx:128` | text | Link the hint |
| `NetworkSection.tsx:92` | proxy URL | Link the error |
| `SafetySection.tsx:46` | backup dir | Commits on blur; keep |
| `AdvancedSection.tsx:79` | custom dir | Link the error |
| `ProfileEditor.tsx:178` | text ×N (`text()` helper) | Add `aria-describedby` next to `aria-invalid` |
| `SourcesSection.tsx:86` | password (API key) | Show/hide toggle (input + icon button) |
| `FilterPanel.tsx:46` | number (MB) | Keep `type="number"` |
| `FilterPanel.tsx:183`, `193` | date | Keep native for now (a Calendar adds react-day-picker) |
| `LibrarySection.tsx:37` | textarea with `text-input` | `Textarea`, monospace (`.setting textarea`, 1486) |

`Field.tsx:25-56` is a home-made Label + FormItem + FormDescription +
FormMessage. Rebuild it as shadcn's Form anatomy **without**
react-hook-form: a `Field` that provides an id through context and injects
`id`, `aria-describedby` and `aria-invalid` into its child. That fixes all 12
unlinked fields at once. Styles to delete: `.text-input`, the unused
`.textarea` (168-172).

**Tests at risk:** low. `textbox` (25), `searchbox` (11) and `spinbutton`
(`LibraryScreen.test.tsx:162`, keep `type="number"`) queries are role-based.

### 4. Switch and Checkbox — 17

| file:line | Target |
|---|---|
| `GeneralSection.tsx:53` | Switch |
| `LibrarySection.tsx:137`, `148` | Switch |
| `ArtSection.tsx:128` (media set in a fieldset) | Checkbox |
| `ArtSection.tsx:200` | Switch |
| `MatchingSection.tsx:160`, `171` | Switch |
| `SafetySection.tsx:28` (`aria-describedby="backup-locked"`), `78`, `89` | Switch |
| `SourcesSection.tsx:201` (enable source in an ordered list) | Switch |
| `ProfileEditor.tsx:241` | Switch |
| `ProfileEditor.tsx:261` (checkbox inside a `<legend>`) | Switch + `Collapsible` |
| `FillWizard.tsx:241` (media choice, `.wizard-choice` 1190) | Checkbox |
| `FilterPanel.tsx:129`, `147`, `165` (systems, extensions, regions) | Checkbox |

Rule: settings that apply immediately become `Switch` (the desktop
convention); multi-select filters and media sets stay `Checkbox`.

**Accessibility fixes:** brand colour and focus ring (today they render in
system blue: no `accent-color`); use `<Label htmlFor>` + `id` instead of
wrapping a Radix button in `<label>` (jsx-a11y `label-has-associated-control`).
Styles to delete: `label.checkbox-row` / `.radio-row` (1491-1495).

**Tests at risk:**

- `LibraryScreen.test.tsx:251` queries `input, select, textarea` and expects
  more than 5. Radix Checkbox and Radio render no `<input>` outside a
  `<form>`, so FilterPanel's 11+ controls vanish and only 5 remain. Wrap
  FilterPanel in a `<form>` (Radix then renders hidden inputs with `name`) or
  include `[role=checkbox],[role=radio]` in the assertion.
- `checkbox` (19) queries in `LibraryScreen`, `SettingsScreen`, `Sections`,
  `ProfilesSection`, `FillWizard`: `toBeChecked` works with `aria-checked`,
  but every toggle that becomes a Switch changes role to `switch`. Update
  those queries in the same PR. Tests reading `.checked` break.

### 5. RadioGroup and ToggleGroup — 8 + segmented + candidate pickers

| file:line | Target |
|---|---|
| `GeneralSection.tsx:42` (theme) | `ToggleGroup type="single"` (light / dark / system) |
| `ArtSection.tsx:226` (overwrite policy) | RadioGroup |
| `MatchingSection.tsx:50` (identification mode) | RadioGroup |
| `NetworkSection.tsx:77` (proxy mode) | RadioGroup |
| `FillWizard.tsx:223` (scope, disabled at 0) | RadioGroup |
| `FillWizard.tsx:263` (strategy + hint) | RadioGroup as "card radio" |
| `FillWizard.tsx:317` (overwrite) | RadioGroup |
| `FilterPanel.tsx:112` (art filter with counts) | RadioGroup |
| `Toolbar.tsx:67-84` (`.segmented`, Grid / List, 524) | `ToggleGroup type="single"` with LayoutGrid / List icons + Tooltips |
| `ReviewQueue.tsx:218`, `ArtWorkspace.tsx:474` (`.candidate`, `aria-pressed`, 1297-1316) | `ToggleGroup type="single"` styled as cards (arrow-key roving for free) |

**Accessibility fixes:** `role="radiogroup"` and roving focus; keep the
existing `<fieldset><legend>` as the group name.

**Tests at risk:** `radio` (25) and `group` (30) queries survive; the
`LibraryScreen.test.tsx:251` input count above; candidate pickers that change
from `aria-pressed` buttons to radio-like items change role.

### 6. Tabs — 3 hand-rolled tablists

| file:line | Current | Target |
|---|---|---|
| `SettingsScreen.tsx:156-190` | Vertical, manual roving `tabIndex`, `tabRefs` map; `.settings-tab` (1437, 1448); no focus ring | `Tabs orientation="vertical"` |
| `Sidebar.tsx:176-242` | Horizontal, 4 tabs (folders, systems, date, all); `.sidebar-tab` (543, 554); no focus ring | `Tabs` (or `ToggleGroup`) |
| `ArtWorkspace.tsx:571-631` | Media slots with thumbnails, `data-mark`, disabled tabs, `aria-describedby`; `.art-ws__slot` (777-818, 866-872) | `Tabs` with a custom `TabsTrigger` |

Deletes about 60 lines of manual roving code.

**Tests at risk:** `tab` (30) and `tabpanel` (10) queries in
`LibraryScreen`, `SettingsScreen`, `DetailDrawer`, `DrawerSearch`,
`DrawerActions` survive. Radix generates its own ids, so any assertion on
`settings-tab-*`, `settings-panel-*`, `tab-*` or `panel-*` fails. Grep the
tests for those ids first. Only the active `TabsContent` mounts (as now)
unless `forceMount`.

### 7. Select — 10

| file:line | Note |
|---|---|
| `LanguageSwitch.tsx:15` | header, `aria-label` only → Radix Select |
| `MatchingSection.tsx:99` | add-region, `aria-label` → Radix Select |
| `GeneralSection.tsx:20`, `70` | styled native |
| `LibrarySection.tsx:98` | styled native |
| `ArtSection.tsx:156`, `181` | styled native |
| `MatchingSection.tsx:185` | styled native |
| `AdvancedSection.tsx:36` (log level) | styled native |
| `ProfileEditor.tsx:281` (PNG / JPEG in `label.field-label`) | styled native |

`.select` (130-166) has no chevron and no focus ring. Plan: a hand-written
`NativeSelect` primitive (shadcn added `native-select` to its v4 registry;
under Tailwind 3 we write it with the same tokens) with a lucide chevron and
the ring, and Radix Select only where no test drives it.

**Tests at risk:** `userEvent.selectOptions(getByRole('combobox'), …)` in
`StartScreen.test.tsx:185`, `SettingsScreen.test.tsx:129`, `164`,
`Sections.test.tsx:62`, `66`, `150`, `424`. It throws on a Radix trigger
(`button role=combobox`). Keep those selects native, or rewrite the tests as
click trigger → click option.

### 8. Alert and Sonner

**Alert (banners):**

- `.error-banner` (247-254, `#dc2626` tint, `role="alert"`) in about 20
  places: `StartScreen:118`, `SettingsScreen:90`, `121`, `DetailDrawer:40`,
  `90`, `165`, `ArtWorkspace:723`, `FillWizard:380`, `440`, `JobsPanel:202`,
  `342`, `ReviewQueue:247`, `ReportsPanel:133`, and every settings section →
  `Alert variant="destructive"`.
- `.notice-banner` (1548-1562) at `SettingsScreen.tsx:126`, `143` → `Alert`
  (info) + action.
- `.update-banner` (1679-1695) at `UpdateBanner.tsx:16` → `Alert` (info).
- `.pause-banner` (1255) at `FillWizard.tsx:364`, `JobsPanel.tsx:176` →
  `Alert` (warning). Needs the new warning token.

**Sonner (transient confirmations):** `ArtWorkspace.tsx:719`
(`art-ws__status`), `ReportsPanel.tsx:131` and `JobsPanel.tsx:254` ("saved"),
`NetworkSection.tsx:130` ("freed X"), `JobsPanel.tsx:304` (undone counts),
`SourcesSection.tsx:134` (key test result).

**Tests at risk:** `alert` (13) and `status` (6). Sonner renders in a portal
and only when a `<Toaster/>` is mounted: mount it in the test helpers, or keep
inline `role=status` for the asserted ones.

### 9. Badge and SlotMark

- `CandidateMeta.tsx:70` `.source-badge[data-source]` (1318-1340, hard-coded
  source colours) → `Badge variant="outline"` with `--cr-source-*` tokens.
- `CoverGrid.tsx:164`, `172` `.cover-tile__region` (414) → `Badge`.
- `ArtStatusIcon.tsx:23` `.art-mark--present/missing/unsupported` (424-438;
  ● ○ – glyphs, `#16a34a` / `#dc2626`) → a dedicated **`SlotMark`**: solid
  slot = present, dashed slot = missing, struck slot = unsupported, from
  `--cr-slot-filled` / `--cr-slot-missing`. This is the brand metaphor; it is
  not a Badge.
- `ArtWorkspace.tsx:513` `.art-ws__inuse` → `Badge` (fixes the 3.3:1 white on
  green).
- Plain-text states → `Badge`: `JobsPanel.tsx:329-335` (undone / complete /
  incomplete), `ProfilesSection.tsx:79` (built-in), `StartScreen.tsx:180`
  (n ROMs / not connected), the Toolbar Jobs label, `.filter-count` /
  `.sidebar-count`.

**Tests at risk:** `CoverGrid.test.tsx` queries `.cover-tile`,
`.cover-tile__name`, `.cover-tile__system`, `.cover-tile__region` (×2): keep
those class names next to the Tailwind ones.

### 10. Progress — 4 + 1 meter

| file:line | Target |
|---|---|
| `ScanProgressView.tsx:28` (243) | Progress |
| `FillWizard.tsx:349` (`.progress-row progress`, 1251) | Progress |
| `JobsPanel.tsx:146` | Progress |
| `UpdatePanel.tsx:77` (`accent-color`, 1660-1663) | Progress |
| `DetailDrawer.tsx:70` (`meter`, confidence %) | Keep `meter` restyled, or Progress with a semantic colour (Radix has no meter) |

All four progress bars look different today. All have `aria-label`; Radix
gives `role="progressbar"`. **Tests at risk:** none query `progressbar` or
`meter`. `UpdatePanel.test.tsx` uses `.closest('.update-panel')`: keep the
class on the root.

### 11. Tooltip — 28 `title=`

- **Must change:** icon-only buttons — `DetailDrawer.tsx:136` (reveal, also has
  `aria-label`) and every ↑ ↓ × `icon-button` (none has a tooltip today);
  `ArtStatusIcon.tsx:26` (`role=img` + `title`).
- **Truncation helpers** (`.truncate` + `title`: StartScreen ×4, JobsPanel ×4,
  FillWizard ×2, ReviewQueue ×2, CoverGrid ×3, ScanProgressView, Toolbar `h1`,
  Sidebar, LibrarySection, ProfilesSection, ReportsPanel, ArtWorkspace,
  EntryTable): keep `title`, or add a `TruncatedText` that shows a Tooltip
  only when the text overflows.
- Needs a `TooltipProvider` in `Root.tsx`. **Tests at risk:** low.

### 12. Card, Table, Skeleton, Collapsible, ScrollArea, Separator, EmptyState

| file:line | Widget | Target |
|---|---|---|
| `StartScreen.tsx:133-152`, `165-196` | volume / recent rows (`.recent-open`, 201-235; buttons at 136, 168) | interactive `Card` + `Badge` |
| `Sidebar.tsx:122-134`, `136-172` | scope items (`aria-current`), folder tree with `tree-toggle` ▾ ▸ (576) | `Collapsible` per node + ghost `Button` + rotating `ChevronRight` |
| `EntryTable.tsx:118` | `sort-button` (478) in `aria-sort` header | ghost `Button size="sm"` + sort icon |
| `FillWizard.tsx:393`, `ReportsPanel.tsx:65`, `ArtSection.tsx` | `summary-table` (1217-1235), settings table | `Table` (styling only) |
| `ProfilesSection.tsx:71` | profile list | `Card` rows + `Badge` "built-in" |
| `DetailDrawer.tsx:127` | `aside.drawer` (676-760), non-modal | Keep non-modal; `ScrollArea` for the body. Not `Sheet`. |
| `FilterPanel.tsx:103` | `section#filter-panel` (614-650) | `Collapsible` (or `Popover` anchored to Filters) |
| `ProfileEditor.tsx:258-300` | fieldset per media slot, checkbox in legend | `Collapsible` + `Switch` |
| `ArtWorkspace.tsx:424` | `.art-ws__skeleton` ×3 | `Skeleton` |
| `UpdatePanel.tsx:84` | `<details open>` release notes | `Collapsible` + `ScrollArea` (`.update-notes`) |
| `.wizard` / `.jobs` / `.reports`, `.filter-panel`, `.sidebar-panel`, `.settings-panel` | overflow scrollers | `ScrollArea` |
| `.footer`, `.app-header`, `.toolbar`, `.drawer-section` borders | separators | `Separator` |
| `LibraryScreen.tsx:35` `Notice` (`.notice` 662-674) + `.hint` empties (`start.noVolumes`, `start.noRecent`, `jobs.none`, `jobs.noHistory`, `reports.noJob`, `review.empty`, `settings.library.recentEmpty`, `drawer.noCandidates`) | empty states | one branded **`EmptyState`** with the dashed-slot motif |

**Tests at risk:** low; keep any class a test queries.

### 13. DropdownMenu and Command (new)

- `Toolbar.tsx:39-111` has 8 controls in a wrapping row. Move Reports,
  Settings, Language and Close library into a `DropdownMenu`.
- A ⌘K `Command` palette (jump to system or folder, open Fill / Jobs /
  Settings) fits the keyboard-first design (`/`, ⌘F, `j`/`k`).
- Optional; no tests at risk.

## Setup under Tailwind 3.4

Installed today: React 19.3.1, Tailwind 3.4.19, Vitest 5.0.3 (jsdom 30.1.2),
@testing-library/react 16.3.3, user-event 14.6.8. The `@` alias exists in
`vite.config.ts` and `tsconfig.json`, preflight is on, and `src/lib/theme.ts`
sets both `.dark` and `data-theme` on `<html>`, so `darkMode: 'class'` works
as is.

### CLI or manual copy

The current shadcn CLI targets Tailwind 4. For Tailwind 3 the docs say to use
`shadcn@2.3.0` ([v3 docs](https://v3.shadcn.com/docs/installation/vite),
[legacy](https://ui.shadcn.com/docs/legacy)). `tw-animate-css` replaces
`tailwindcss-animate` only on Tailwind 4
([v4 notes](https://ui.shadcn.com/docs/tailwind-v4)), so **stay on
`tailwindcss-animate`**.

`init` rewrites `globals.css` and `tailwind.config.ts`. Prefer copying the
component sources from v3.shadcn.com into `src/components/ui/` by hand, or run
`init` on a scratch copy and merge. If the CLI is used:

```bash
pnpm dlx shadcn@2.3.0 init   # style new-york, base colour neutral, CSS variables yes
pnpm dlx shadcn@2.3.0 add button dialog alert-dialog input textarea label checkbox \
  radio-group switch select tabs toggle toggle-group tooltip progress badge separator \
  scroll-area collapsible card table skeleton alert sonner dropdown-menu popover command
```

`components.json`:

```json
{
  "style": "new-york",
  "rsc": false,
  "tsx": true,
  "tailwind": {
    "config": "tailwind.config.ts",
    "css": "src/styles/globals.css",
    "baseColor": "neutral",
    "cssVariables": true
  },
  "aliases": {
    "components": "@/components",
    "ui": "@/components/ui",
    "utils": "@/lib/utils",
    "lib": "@/lib",
    "hooks": "@/hooks"
  },
  "iconLibrary": "lucide"
}
```

### Dependencies

Runtime:

```
class-variance-authority
clsx
tailwind-merge@^2          # v2 matches Tailwind 3; v3 targets Tailwind 4
lucide-react
@radix-ui/react-slot
@radix-ui/react-dialog
@radix-ui/react-alert-dialog
@radix-ui/react-label
@radix-ui/react-checkbox
@radix-ui/react-radio-group
@radix-ui/react-switch
@radix-ui/react-select
@radix-ui/react-tabs
@radix-ui/react-toggle
@radix-ui/react-toggle-group
@radix-ui/react-tooltip
@radix-ui/react-progress
@radix-ui/react-separator
@radix-ui/react-scroll-area
@radix-ui/react-collapsible
@radix-ui/react-dropdown-menu
@radix-ui/react-popover
sonner
cmdk
```

Dev: `tailwindcss-animate`, added as an ESM import in `plugins` (the config is
`.ts`). Current Radix 1.1+/1.2+ packages declare React 19 peers. Add packages
per PR, only for the primitives that PR introduces.

### Steps

1. **`src/lib/utils.ts`:** `cn(...inputs) = twMerge(clsx(inputs))`.
2. **Channel tokens** next to the colour tokens, so opacity modifiers like
   `bg-primary/90` work (Tailwind 3 needs `<alpha-value>`):

   ```css
   --cr-bg-rgb: 247 247 245;  --cr-surface-rgb: 255 255 255;  --cr-text-rgb: 28 27 26;
   --cr-text-muted-rgb: 95 92 88;  --cr-border-rgb: 227 225 221;
   --cr-accent-rgb: 67 56 202;  --cr-error-rgb: 185 28 28;
   /* dark: 20 20 19 / 31 30 28 / 242 241 238 / 168 164 158 / 51 49 46 / 129 140 248 / 252 165 165 */
   ```

   These are today's values; the brand step replaces them with the new palette,
   ideally emitted by the token build (see `research.md`). Dark values must
   exist under `:root[data-theme='dark']`, the `prefers-color-scheme` block and
   `.dark`.
3. **`tailwind.config.ts`:** map shadcn's names onto `--cr-*` instead of
   shadcn's own HSL variables, each as `'rgb(var(--cr-…-rgb) / <alpha-value>)'`:
   `background → bg`, `foreground → text`, `card` / `popover → surface`,
   `primary → accent` (foreground `accent-text`), `muted-foreground →
   text-muted`, `border` / `input → border` (input moves to a ≥ 3:1
   `--cr-input` once it exists), `ring → accent`, `destructive → error`, plus
   `success`, `warning`, `info`, `overlay`, `source.*` and `slot.filled /
   missing`. Add `borderRadius: { lg: 'var(--cr-radius)', md:
   'calc(var(--cr-radius) - 2px)', sm: 'calc(var(--cr-radius) - 4px)' }` and
   `tailwindcss-animate`. Components then work unmodified.
4. **Lint and types:**
   - `exactOptionalPropertyTypes` and `noUncheckedIndexedAccess` are on: some
     generated components pass `undefined` to optional Radix props
     (`className`, `sideOffset`, Sonner `theme`). Run `pnpm typecheck` after
     each component and fix locally.
   - `react-refresh/only-export-components` flags `button.tsx`, `badge.tsx` and
     `toggle.tsx` (they export variants). Add an override for
     `src/components/ui/**`, or the same disable comment as `Field.tsx:9`.
   - `eslint-plugin-jsx-a11y`: use `<Label htmlFor>` rather than wrapping
     Radix controls in `<label>`.
5. **i18n:** route every built-in sr-only string (`dialog`, `sheet`,
   `command`) through `useT()` and add the keys to both
   `locales/en/ui.json` and `locales/es/ui.json`.
6. **jsdom polyfills** in `src/test/setup.ts` (ResizeObserver, `scrollTo` and
   `createObjectURL` are already stubbed):

   ```ts
   Element.prototype.hasPointerCapture ??= () => false;
   Element.prototype.setPointerCapture ??= () => {};
   Element.prototype.releasePointerCapture ??= () => {};
   Element.prototype.scrollIntoView ??= () => {};
   window.matchMedia ??= /* stub */;   // Sonner + theme
   ```

   Add a `DOMRect` stub if Popper complains.
7. **Webview targets:** Vite `build.target: 'safari13'`, macOS minimum 11.0
   (WKWebView ≈ Safari 14). Radix is fine; avoid `color-mix()` and relative
   colour syntax in the new primitives.

## First PR

1. `src/lib/utils.ts`, channel tokens, the Tailwind mapping and
   `tailwindcss-animate`.
2. `src/components/ui/button.tsx` with variants `default | outline | ghost |
   destructive | link` and sizes `sm | default | lg | icon`.
3. Codemod the 104 buttons by class: `primary-button` → `default`,
   `ghost-button` → `outline`, `icon-button` → `outline` + `icon`,
   `art-ws__btn` → `outline` + `sm`, `ghost-button danger` → `destructive`.
4. Switch the three destructive confirms to `destructive`.
5. Delete the dead CSS (81-96, 142-161, 912-950, 1509).

The DOM stays `<button>` with the same text and `aria-*`, so every
`getByRole('button')` query keeps passing. Dialog and AlertDialog follow as
their own PR, with the test changes listed above.
