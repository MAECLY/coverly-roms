# System Icons and Library Views Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wherever the app shows a system's name ("Game Boy"), a quiet monochrome hardware silhouette sits beside it (RetroArch XMB Monochrome, CC BY 4.0, vendored and recoloured to `currentColor`; a Slot-shaped abbreviation mount where no neutral silhouette exists). The library grows from two layouts (Grid, List) to six views — **Covers**, **Details**, **Columns**, **Gallery**, **Art slots** (Mounts) and **Shelves** — with per-library view prefs, Group by, cover sizes XS–XL, a Details column picker with row density, one shared keyboard model (type-ahead, Space quick look, ⌥↑/⌥↓ group jumps) and ⌘1–⌘6 / ⌘+ ⌘− ⌘0 / ⌘I shortcuts.

**Architecture:** Frontend only (no change under `crates/` or the organise core). A Node generator (`scripts/icons/systems.mjs`, pure helpers in `scripts/icons/systemIconsLib.mjs`) fetches pinned SVGs once and commits `src/components/system/systemIcons.generated.ts`; `SystemIcon` / `SystemLabel` render them inline. Views share one data path: `useVisibleEntries` → `grouping.ts` (sections as index ranges) → `rowModel.ts` (flat header/item rows per view geometry) → one `@tanstack/react-virtual` virtualiser per scroll axis. `viewStore` gains `prefs: ViewPrefs`, persisted per library by `src/lib/viewPrefs.ts` under `coverly.views.v1` with migration from `coverly.layout`. Each view is a component in `src/components/library/views/` taking `ViewProps` (today's `EntryListProps` plus `sections`, `prefs`, `onPreview`).

**Tech Stack:** React 19 + TypeScript 6 (strict, `exactOptionalPropertyTypes`, `noUncheckedIndexedAccess`), Tailwind 3, zustand 5, Radix primitives in `src/components/ui`, `@tanstack/react-virtual` 3, `lucide-react` 1.59. New **dev** dependencies only: `svgo` and `svg-path-bbox` (generator). Tests: Vitest 5 + Testing Library + `user-event`; E2E by hand with Chrome DevTools MCP on the mock web app.

**Spec:** [`docs/superpowers/specs/2026-10-18-system-icons-and-views.md`](../specs/2026-10-18-system-icons-and-views.md) (Parts A, B, C and §D delivery order). Research: [`docs/02-research/system-icons-and-views.md`](../../02-research/system-icons-and-views.md) (§A.3 id → file map, §C repo audit). Brand: `docs/08-brand/brand.md` (§1 identity, §2 Slot states, §3 voice).

## Global Constraints

- Work only in the worktree `/Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-views`, branch `feat/system-icons-views`. Never switch branches, never push, never touch `main`. Every shell command starts with `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-views &&`.
- TDD: write the failing test, run it and watch it fail, implement, watch it pass.
- After **every** task run the gate and fix anything red before committing: `pnpm check-all` (tokens:check, lint, format:check, typecheck, vitest) and `pnpm build`. No Rust file changes, so cargo gates are not needed; if a task ever touches `src-tauri/` (only Task 24's contingency), also run `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`.
- Commits: conventional (`build(icons): …`, `feat(views): …`, `test(views): …`, `docs: …`), granular, English. **Never** add `Co-Authored-By`, "Generated with", or any mention of Claude/AI. Do not push.
- Real data is read-only: never write to `/Volumes/System` or `/Users/miguelangelesparzacalero/Documents/ROMS`. Nothing in this plan needs them.
- **Merge hygiene.** `feat/organize-disc-archives` touches the organise core and organise UI. In `src/components/organize/*` make only the insertions listed in Task 8 (an import plus one element per site). Do not reformat, reorder or rename anything there. No file under `crates/` changes.
- Every UI string goes through i18n in `locales/en/ui.json` **and** `locales/es/ui.json` (same keys; `src/i18n/parity.test.ts`). New keys under `layouts.*` (`views.*` is taken by the sidebar tabs). Calm archivist voice, exact counts, no exclamation marks.
- System icons key on `SystemView.system_id`, never on `name`. Icons beside a name are `aria-hidden`; a lone icon has `title` → `role="img"` + `aria-label`. Never inside `<option>`, never inside interpolated prose (spec §A.6).
- Colours: only existing tokens (`--cr-text-secondary` at rest, inherited ink when selected, `--cr-primary` only via selection/focus, `--cr-slot-missing` for dashed mounts, Manila only for applied-this-run). No new type sizes.
- Every view: virtualised, fixed sizes per view and per size stop, the active item always rendered (`rangeExtractor`), `aria-activedescendant` pointing at an element that exists, Tab leaves the view.
- `localStorage` reads/writes wrapped in try/catch and validated against whitelists.
- File names are hostile: React text only, never `dangerouslySetInnerHTML` for entry data. (The generated icon bodies are trusted repo constants; `SystemIcon` injects them with `dangerouslySetInnerHTML` on the `<svg>` — allowed only there, with an eslint-disable comment naming this rule.)
- Mock web app for E2E on port **1457**: `pnpm exec vite --port 1457 --strictPort` (background). Stop only by port: `lsof -ti tcp:1457 -sTCP:LISTEN | xargs kill`. Chrome DevTools MCP with `isolatedContext: 'views'`. Pick the 12k fixture by setting `localStorage['coverly.mock.pick'] = '/mock/Fixture SD (12k)'` before opening.

## Review Focus

1. **Licence and trademark.** Every vendored file comes from the pinned commit `d9f969054dc7fbb6fa89519036d2b971e0855b51`; CC BY 4.0 attribution + list of changes in `THIRD-PARTY-NOTICES.md` and `src/assets/system-icons/README.md`; no maker logo or wordmark ships (CPS, MAME, DOS, ScummVM, RPG Maker → fallback); every source entry has `depicts` + `audit`. Tests: Task 2 (`every source entry is audited`, `no wordmark id is mapped to a silhouette`).
2. **Icons cannot leak colour or markup.** No `#fff`, `style=`, `<text`, `<image>`, or `fill` other than `currentColor`/`none`; viewBoxes valid and cropped; PS2/Neo Geo (no source viewBox) render. Tests: Task 1 (`normalise` unit tests), Task 2 (data invariants), Task 25 E2E contact sheet.
3. **Every system id resolves both ways.** `systems.toml` ↔ `SYSTEM_ICONS ∪ FALLBACK_ABBR`. Test: Task 2 (`every systems.toml id resolves`, `no map entry points at an unknown id`).
4. **12,000 entries stay fast.** No view renders more than ~200 rows/tiles; grouping < 30 ms; view switch < 100 ms; Art slots scroll without long tasks > 50 ms. Tests: Task 10 (`groups 12k entries under 30 ms`), each view's `renders about one screen` test, Task 25 traces.
5. **Keyboard and screen readers.** One model across views, no Tab trap (Gallery!), valid roles (`row > rowheader[aria-colspan]`), active descendant always in the DOM, shortcuts stand down behind modals. Tests: Task 12, each view task, Task 23.
6. **Prefs never break startup.** Junk/old/throwing storage → defaults; per-library LRU 20; migration from `coverly.layout`. Tests: Task 9.

## Deviations from the spec (decided in this plan)

- **V-D1 — Generator split.** The spec names one script; pure helpers (`normaliseSvg`, `synthesiseViewBox`, `cropViewBox`, `assertClean`) live in `scripts/icons/systemIconsLib.mjs` with a `.d.mts` (same pattern as `scripts/brand/build_tokens.mjs`), so Vitest (which only includes `src/**`) can test them from `src/components/system/systemIconsBuild.test.ts`. `scripts/icons/systems.mjs` does the fetching and writing. The existing `scripts/icons/generate.mjs` (app icon rasteriser) is untouched.
- **V-D2 — Script name.** `pnpm system-icons` (the spec's name); `pnpm icons` stays the app-icon command.
- **V-D3 — Docked inspector.** Today `DetailDrawer` opens on Enter/click. `prefs.inspector` makes it stay docked (opened on selection, not only on Enter) in Covers, Details, Art slots and Shelves; Enter still opens it when undocked. No new component: `DetailDrawer` gets a `docked` prop.
- **V-D4 — Quick look.** Space opens `QuickLook` (a Radix `Dialog` showing the active entry's art large for the current media, with ←→ to step). Being a Radix modal, it automatically makes the view shortcuts stand down (spec §B.6).
- **V-D5 — `layout` alias.** `viewStore.layout` stays for one release as a getter-like derived field (`'grid'` for covers, `'list'` for everything else) updated with `prefs.view`; Toolbar and LibraryScreen move to `prefs.view` in this branch, so the alias only protects code on other branches. `toolbar.grid` / `toolbar.list` keys are removed in Task 22 once no caller is left (grep proves it).
- **V-D6 — `maxPx` contingency.** The art-URL `maxPx` IPC (spec §B.7) is only implemented if Task 25's Art-slots trace fails its budget; it is then Task 24 (src-tauri `art` command + mock + `useArtUrl`). If the budget holds, Task 24 is skipped and the result recorded in the tracker.

---

## File structure

```
package.json                                  + "system-icons" script; devDeps svgo, svg-path-bbox
scripts/icons/systems.mjs                     fetch pinned SVGs → normalise → write outputs
scripts/icons/systemIconsLib.mjs (+ .d.mts)   SOURCE map, pure normalise/crop/assert helpers
src/assets/system-icons/<id>.svg              reviewable normalised copies (41)
src/assets/system-icons/README.md             source, commit, changes, per-icon audit table
src/assets/system-icons/contact-sheet.html    every icon at 14/16/20/48px (served by vite)
src/components/system/
├─ systemIcons.generated.ts                   SYSTEM_ICONS: Record<string,{viewBox,body,aspect}>
├─ systemFallbacks.ts                         FALLBACK_ABBR, abbreviationFor(id)
├─ SystemIcon.tsx (+ .test.tsx)               icon box 1.5×size, silhouette | mount | folder
├─ SystemLabel.tsx (+ .test.tsx)              icon + truncating name
├─ systemIcons.test.ts                        data invariants vs systems.toml
└─ systemIconsBuild.test.ts                   generator helpers
src/lib/systemLookup.ts (+ .test.ts)          folder/dir → SystemView
src/lib/audit.ts                              AuditRow.system_id
src/lib/viewPrefs.ts (+ .test.ts)             ViewPrefs types, defaults, parse/validate, migrate, LRU store
src/lib/grouping.ts (+ .test.ts)              (entries, groupBy, ctx) → Section[]
src/lib/rowModel.ts (+ .test.ts)              sections + columns → RowModel[]; index maps
src/lib/keyboardNav.ts (+ .test.ts)           + groupJump, createTypeAhead, nearestByLeft, paneMove
src/lib/coverSizes.ts (+ .test.ts)            COVER_SIZES metrics per stop, step()
src/lib/detailsColumns.ts (+ .test.ts)        column catalogue, defaults, reorder/toggle/resize
src/stores/viewStore.ts (+ viewStore.test.ts) prefs, setters, loadFor(root), layout alias
src/hooks/useViewShortcuts.ts (+ .test.tsx)   ⌘1–6, ⌘+/−/0, ⌘I, ctrl-wheel
src/hooks/useActiveRange.ts                   rangeExtractor that keeps active row + sticky header
src/components/library/
├─ views/types.ts                             ViewProps
├─ views/SectionHeaderRow.tsx                 shelf-rule header row (role=row > rowheader)
├─ views/QuickLook.tsx (+ .test.tsx)          Space preview dialog
├─ views/CoversView.tsx (+ .test.tsx)         was CoverGrid: size stops, art choice, groups
├─ views/DetailsView.tsx (+ .test.tsx)        was EntryTable: columns, density, thumb, groups
├─ views/ColumnPicker.tsx                     header context menu + options list
├─ views/MountsView.tsx (+ .test.tsx)         Art slots
├─ views/ColumnsView.tsx (+ .test.tsx)        Miller panes
├─ views/GalleryView.tsx (+ .test.tsx)        large art + filmstrip + metadata
├─ views/ShelvesView.tsx (+ .test.tsx)        one shelf per section
├─ ViewSwitcher.tsx (+ .test.tsx)             six segments / narrow menu
├─ ViewOptions.tsx (+ .test.tsx)              popover with per-view controls
├─ CoverGrid.tsx, EntryTable.tsx              become re-exports of CoversView / DetailsView (Task 11/14),
│                                             existing tests kept passing, then moved
├─ Sidebar.tsx, FilterPanel.tsx, DetailDrawer.tsx, Toolbar.tsx   icon sites, docked inspector, switcher
src/components/reports/ReportsPanel.tsx       R1
src/components/fill/{FillWizard,ReviewQueue}.tsx   F1, F2
src/components/organize/{SystemGroups,ReviewBuckets}.tsx  O1–O3 insertions only
src/screens/LibraryScreen.tsx (+ .test.tsx)   view switch, shortcuts, focus move, prefs per library
src/styles/globals.css                        .system-icon, .system-label, .system-mount, view classes
locales/{en,es}/ui.json                       layouts.*, systems.unknown
THIRD-PARTY-NOTICES.md, LICENSES/CC-BY-4.0.txt, TRADEMARKS.md, docs/09-legal/trademark.md
docs/04-architecture/desktop-app.md, README.md, README.es.md, CHANGELOG.md, docs/TRACKER.md
```

---

## Group I — System icons

### Task 1: Generator script and vendored icons

**Files:**
- Modify: `package.json` (script `"system-icons": "node scripts/icons/systems.mjs"`; devDeps)
- Create: `scripts/icons/systemIconsLib.mjs`, `scripts/icons/systemIconsLib.d.mts`, `scripts/icons/systems.mjs`
- Create (generated, committed): `src/assets/system-icons/*.svg`, `src/assets/system-icons/README.md`, `src/assets/system-icons/contact-sheet.html`, `src/components/system/systemIcons.generated.ts`
- Test: `src/components/system/systemIconsBuild.test.ts`

**Interfaces (produced):**

```ts
// scripts/icons/systemIconsLib.d.mts
export const COMMIT: 'd9f969054dc7fbb6fa89519036d2b971e0855b51';
export const SOURCE_BASE: string; // https://raw.githubusercontent.com/libretro/retroarch-assets/<COMMIT>/src/xmb/monochrome/
export type Depicts = 'console' | 'handheld' | 'pad' | 'cabinet' | 'computer';
export interface IconSource { file: string; depicts: Depicts; audit: string }
export const SYSTEM_ICON_SOURCES: Readonly<Record<string, IconSource>>;
export interface NormalisedIcon { viewBox: string; body: string; aspect: number }
/** svgo pass + fills → currentColor + strip ids/classes/styles. Throws on a dirty result. */
export function normaliseSvg(svg: string): string;
/** "0 0 w h" from width/height when there is no viewBox; throws when neither exists. */
export function synthesiseViewBox(svg: string): string;
/** Union bbox of every <path d> (svg-path-bbox), padded 2%, clamped to the source viewBox. */
export function cropViewBox(svg: string, sourceViewBox: string): { viewBox: string; aspect: number };
/** Throws naming the offending token: #fff, #ffffff, <text, <image, style=, fill="<other>". */
export function assertClean(id: string, body: string): void;
export function toGenerated(icons: Record<string, NormalisedIcon>): string; // TS module text
export function toContactSheet(icons: Record<string, NormalisedIcon>): string;
export function toReadme(sources: Record<string, IconSource>): string;
```

`SYSTEM_ICON_SOURCES` holds exactly the 41 entries of research §A.3 "Direct silhouettes" (file names verbatim, `.svg` appended), each with `depicts` and a one-line `audit` written while looking at the source (e.g. `"pad, plain dots, no marks"`, `"handheld, blank screen, no badge"`). `fbneo`, `varcade`, `pgm2` → `FBNeo - Arcade Games.svg`, `depicts: 'cabinet'`. If, on inspection, any silhouette carries a maker mark (face-button symbols △○×□, Dreamcast swirl, a wordmark on the shell), drop it from the map (it then falls back in Task 2) and say so in its README row.

- [ ] **Step 1: Add dev dependencies**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-views && pnpm view svgo version && pnpm view svg-path-bbox version
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-views && pnpm add -D -E svgo@<printed> svg-path-bbox@<printed>
```

Use the printed latest stable versions, pinned exactly (`-E`), and check both are MIT (`pnpm view <pkg> license`).

- [ ] **Step 2: Write failing helper tests** — `src/components/system/systemIconsBuild.test.ts`

```ts
import { describe, expect, it } from 'vitest';
import {
  assertClean,
  cropViewBox,
  normaliseSvg,
  synthesiseViewBox,
  SYSTEM_ICON_SOURCES,
} from '../../../scripts/icons/systemIconsLib.mjs';

const WHITE = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 128 64 64"><path fill="#ffffff" d="M10 150h44v20H10z"/></svg>';
const STYLED = '<svg viewBox="0 0 256 256"><path style="fill:#ffffff" d="M0 0h10v10H0z"/><path style="fill:none" d="M1 1h2v2H1z"/></svg>';
const NO_VIEWBOX = '<svg width="64" height="64"><path fill="#fff" d="M8 8h48v48H8z"/></svg>';

describe('normaliseSvg', () => {
  it('turns every non-none fill into currentColor', () => {
    const out = normaliseSvg(WHITE);
    expect(out).toContain('fill="currentColor"');
    expect(out).not.toMatch(/#fff/i);
  });
  it('turns style fills into attributes and keeps fill none', () => {
    const out = normaliseSvg(STYLED);
    expect(out).not.toContain('style=');
    expect(out).toContain('currentColor');
    expect(out).toMatch(/fill="none"/);
  });
});

describe('synthesiseViewBox', () => {
  it('builds one from width and height', () => {
    expect(synthesiseViewBox(NO_VIEWBOX)).toBe('0 0 64 64');
  });
  it('throws when there is neither', () => {
    expect(() => synthesiseViewBox('<svg><path d="M0 0h1v1z"/></svg>')).toThrow(/viewBox/);
  });
});

describe('cropViewBox', () => {
  it('crops a landscape pad to its drawing plus 2%', () => {
    const { viewBox, aspect } = cropViewBox(WHITE, '0 128 64 64');
    const [x, y, w, h] = viewBox.split(' ').map(Number);
    expect(x).toBeCloseTo(10 - 0.88, 1);
    expect(y).toBeCloseTo(150 - 0.4, 1);
    expect(w! / h!).toBeCloseTo(aspect, 3);
    expect(aspect).toBeGreaterThan(2);
  });
  it('throws on an empty glyph', () => {
    expect(() => cropViewBox('<svg viewBox="0 0 10 10"></svg>', '0 0 10 10')).toThrow(/empty/);
  });
});

describe('assertClean', () => {
  it.each(['#fff', 'style="x"', '<text', '<image', 'fill="#000"'])('rejects %s', (bad) => {
    expect(() => assertClean('gb', `<path ${bad} d="M0 0"/>`)).toThrow(/gb/);
  });
  it('accepts currentColor and none', () => {
    expect(() => assertClean('gb', '<path fill="currentColor" d="M0 0"/><path fill="none" d="M0 0"/>')).not.toThrow();
  });
});

describe('SYSTEM_ICON_SOURCES', () => {
  it('maps 41 ids, each audited', () => {
    const entries = Object.entries(SYSTEM_ICON_SOURCES);
    expect(entries).toHaveLength(41);
    for (const [id, s] of entries) {
      expect(s.file, id).toMatch(/\.svg$/);
      expect(s.audit.trim(), id).not.toBe('');
    }
  });
});
```

(`41` is the spec's count; if Step 4's audit drops an icon, update this number and the README in the same commit.)

- [ ] **Step 3: Run and watch it fail**

`cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-views && pnpm vitest run src/components/system/systemIconsBuild.test.ts` → FAIL (module not found).

- [ ] **Step 4: Implement `systemIconsLib.mjs` and `systems.mjs`**

- `normaliseSvg`: `optimize(svg, { multipass: true, plugins: [{ name: 'preset-default', params: { overrides: { removeViewBox: false } } }, 'removeDimensions', 'convertStyleToAttrs', { name: 'removeAttrs', params: { attrs: ['id', 'class', 'data-.*'] } }, 'removeStyleElement'] })`, then replace every `fill="<x>"` with `x !== 'none'` by `fill="currentColor"`; then `assertClean`.
- `cropViewBox`: collect every `d="…"` with a regex, `svgPathBbox(d)` each, union, pad 2% of the larger side, intersect with the source viewBox; format numbers with at most 3 decimals; `aspect = w / h`. Empty/outside → throw.
- `systems.mjs`: for each id, `fetch(SOURCE_BASE + encodeURIComponent(file))` (one fetch per distinct file), `synthesiseViewBox` when missing, `normaliseSvg`, `cropViewBox`, keep only the inner markup as `body`; write `src/assets/system-icons/<id>.svg` (`<svg xmlns viewBox fill="currentColor">body</svg>`), the generated TS (sorted ids, `// Generated by pnpm system-icons. Do not edit.` header, `export const SYSTEM_ICONS: Readonly<Record<string, { viewBox: string; body: string; aspect: number }>> = {…} as const;`), README (source URL, commit, licence CC BY 4.0, changes: recoloured to `currentColor`, optimised with SVGO, viewBoxes synthesised/cropped, path data unchanged; table id | file | depicts | audit) and contact sheet (static HTML, both themes via `prefers-color-scheme`, each icon at 14/16/20/48 px in a 1.5× box). Exit non-zero on any thrown check. Run Prettier on the generated TS.

- [ ] **Step 5: Run the generator and tests**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-views && pnpm system-icons && pnpm vitest run src/components/system/systemIconsBuild.test.ts
```

Expected: 41 files written, tests PASS. Open `src/assets/system-icons/contact-sheet.html` in the browser later (Task 25); now just `grep -c "data-id" src/assets/system-icons/contact-sheet.html` → 41.

- [ ] **Step 6: Gate and commit**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-views && pnpm check-all && pnpm build
git add package.json pnpm-lock.yaml scripts/icons src/assets/system-icons src/components/system/systemIcons.generated.ts src/components/system/systemIconsBuild.test.ts
git commit -m "build(icons): vendor RetroArch monochrome system icons with a generator script"
```

---

### Task 2: Fallback map and icon data invariants

**Files:**
- Create: `src/components/system/systemFallbacks.ts`
- Test: `src/components/system/systemIcons.test.ts`

**Interfaces:**

```ts
// systemFallbacks.ts
/** Abbreviations for systems with no neutral silhouette (spec §A.4). ≤ 4 chars. */
export const FALLBACK_ABBR: Readonly<Record<string, string>> = {
  cps1: 'CPS1', cps2: 'CPS2', cps3: 'CPS3', mame: 'MAME', hbmame: 'HB',
  dos: 'DOS', scummvm: 'SVM', easyrpg: 'RPG', ons: 'ONS', openbor: 'BOR',
  pico8: 'P8', atomiswave: 'AW', naomi: 'NAO',
};
/** Ids whose set icon is a wordmark/logo: never mapped to a silhouette. */
export const WORDMARK_IDS: readonly string[] = ['cps1','cps2','cps3','mame','hbmame','dos','scummvm','easyrpg'];
/** Known abbreviation, else the first 4 letters of the id upper-cased (user systems). */
export function abbreviationFor(id: string): string;
```

- [ ] **Step 1: Failing test** — `systemIcons.test.ts`

```ts
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { SYSTEM_ICON_SOURCES } from '../../../scripts/icons/systemIconsLib.mjs';
import { abbreviationFor, FALLBACK_ABBR, WORDMARK_IDS } from './systemFallbacks';
import { SYSTEM_ICONS } from './systemIcons.generated';

const toml = readFileSync(path.resolve(__dirname, '../../../crates/coverly-core/src/systems/systems.toml'), 'utf8');
const IDS = [...toml.matchAll(/^id\s*=\s*"([^"]+)"/gm)].map((m) => m[1]!);

describe('system icon data', () => {
  it('reads the 54 systems', () => expect(IDS).toHaveLength(54));
  it('every systems.toml id resolves to an icon or an abbreviation, not both', () => {
    for (const id of IDS) {
      const has = Number(id in SYSTEM_ICONS) + Number(id in FALLBACK_ABBR);
      expect(has, id).toBe(1);
    }
  });
  it('no map entry points at an unknown id', () => {
    for (const id of [...Object.keys(SYSTEM_ICONS), ...Object.keys(FALLBACK_ABBR)]) expect(IDS, id).toContain(id);
  });
  it('no wordmark id is mapped to a silhouette', () => {
    for (const id of WORDMARK_IDS) expect(SYSTEM_ICONS, id).not.toHaveProperty(id);
  });
  it('bodies are clean', () => {
    for (const [id, icon] of Object.entries(SYSTEM_ICONS)) {
      expect(icon.body, id).not.toMatch(/#fff|<text|<image|style=/i);
      for (const m of icon.body.matchAll(/fill="([^"]*)"/g)) expect(['currentColor', 'none'], id).toContain(m[1]);
    }
  });
  it('viewBoxes are valid and aspects sane', () => {
    for (const [id, icon] of Object.entries(SYSTEM_ICONS)) {
      expect(icon.viewBox, id).toMatch(/^-?[\d.]+ -?[\d.]+ [\d.]+ [\d.]+$/);
      expect(icon.aspect, id).toBeGreaterThanOrEqual(0.5);
      expect(icon.aspect, id).toBeLessThanOrEqual(2.5);
    }
  });
  it('every source entry is audited', () => {
    for (const [id, s] of Object.entries(SYSTEM_ICON_SOURCES)) {
      expect(s.depicts, id).toBeTruthy();
      expect(s.audit.length, id).toBeGreaterThan(5);
    }
  });
  it('abbreviations are 1–4 chars; user ids use their first four letters', () => {
    for (const v of Object.values(FALLBACK_ABBR)) expect(v).toMatch(/^.{1,4}$/);
    expect(abbreviationFor('x68000')).toBe('X680');
    expect(abbreviationFor('cps2')).toBe('CPS2');
  });
});
```

(If an aspect falls outside 0.5–2.5 for a real icon, widen only after looking at the contact sheet, and note it in the README.)

- [ ] **Step 2: Fail** → `pnpm vitest run src/components/system/systemIcons.test.ts` (module not found).
- [ ] **Step 3: Implement `systemFallbacks.ts`.** `abbreviationFor(id) = FALLBACK_ABBR[id] ?? id.replace(/[^a-z0-9]/gi, '').slice(0, 4).toUpperCase() || '?'`.
- [ ] **Step 4: Pass**, gate, commit: `feat(system): add fallback abbreviations and icon data checks`.

---

### Task 3: Legal notices and trademark notes

**Files:**
- Modify: `THIRD-PARTY-NOTICES.md`, `TRADEMARKS.md` ("Other names"), `docs/09-legal/trademark.md`
- Create: `LICENSES/CC-BY-4.0.txt`

- [ ] **Step 1:** Fetch the licence text verbatim: `curl -fsSL https://creativecommons.org/licenses/by/4.0/legalcode.txt -o LICENSES/CC-BY-4.0.txt` (check it starts with "Attribution 4.0 International"; else use the repo's `COPYING` at the pinned commit).
- [ ] **Step 2: `THIRD-PARTY-NOTICES.md`** — add "System icons": RetroArch Assets, XMB Monochrome theme, © libretro contributors, <https://github.com/libretro/retroarch-assets> at commit `d9f9690…`, CC BY 4.0 (`LICENSES/CC-BY-4.0.txt`), changes made (recoloured to `currentColor`, SVGO, viewBoxes synthesised and cropped; path data unchanged), list of files in `src/assets/system-icons/`. Add `lucide-react` (ISC, © Lucide Contributors, licence text). Add `svgo` / `svg-path-bbox` only if the notices file already lists dev tools (check its scope first; dev-only tools are not shipped).
- [ ] **Step 3: `TRADEMARKS.md`** — in "Other names", add Bandai (WonderSwan), Commodore, Capcom (CP System), MSX Licensing Corporation (MSX), IGS (PGM), Lexaloffle Games (PICO-8), ScummVM. Verify each holder against the current register (USPTO TESS / EUIPO eSearch) before writing; if unverifiable, name the product without a holder. Add: "System icons are simplified silhouettes of compatible hardware, shown next to the system's name only to identify it. They are not the makers' logos and imply no endorsement."
- [ ] **Step 4: `docs/09-legal/trademark.md`** — new section "Third-party systems": hardware silhouette plus the name, never a maker's logo or wordmark; no marks inside silhouettes; any icon can be switched to the abbreviation mount by removing its entry from `SYSTEM_ICON_SOURCES`; icons never appear in identity, landing or marketing (brand §1, §3). State that this is design reasoning, not legal advice.
- [ ] **Step 5:** `pnpm check-all` (format only touches src/locales; still run), commit: `docs(legal): attribute system icons and extend trademark notes`.

---

### Task 4: `SystemIcon`

**Files:**
- Create: `src/components/system/SystemIcon.tsx`
- Modify: `src/styles/globals.css` (`.system-icon`, `.system-mount`, `.system-mount--outline`)
- Modify: `locales/{en,es}/ui.json` (`systems.unknown`: "Folder with no known system" / "Carpeta sin sistema reconocido")
- Test: `src/components/system/SystemIcon.test.tsx`

**Interface:**

```ts
export interface SystemIconProps {
  systemId: string | null;
  /** Box height: px number or CSS length. Default '1em'. Width is always 1.5×. */
  size?: number | string;
  /** Lone icon: role="img" + aria-label. Absent: aria-hidden. */
  title?: string;
  className?: string;
}
export const SystemIcon: React.MemoExoticComponent<(p: SystemIconProps) => JSX.Element>;
```

Rendering:
- Silhouette: `<svg data-system-icon={id} class="system-icon" viewBox={icon.viewBox} preserveAspectRatio="xMidYMid meet" fill="currentColor" width={1.5*size} height={size} aria-hidden | role="img" aria-label>` with body injected (Global Constraints exception).
- Abbreviation mount (fallback id or user id): `<span data-system-icon={id} class="system-mount" style={{width, height}}>` with the text; numeric size < 20 → no outline, font `0.625em`; ≥ 20 → `system-mount--outline` (1px `currentColor`, radius 2px, text `0.5em`); ≥ 24 → also four photo-corner `<i>` elements styled like `slot-mark.tsx` corners. String sizes (`'1em'`) are treated as < 20.
- `null`: lucide `Folder` inside the same box, `data-system-icon="unknown"`; with `title` falls back to `t('systems.unknown')` when `title` is empty string.
- Colour: `.system-icon, .system-mount { color: var(--cr-text-secondary); flex: none; }` and `[aria-selected='true'] .system-icon, [aria-selected='true'] .system-mount, [data-state='on'] .system-icon { color: inherit; }`.

- [ ] **Step 1: Failing tests**

```tsx
import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { SystemIcon } from './SystemIcon';

const box = (el: Element) => [Number(el.getAttribute('width') ?? parseFloat((el as HTMLElement).style.width)), Number(el.getAttribute('height') ?? parseFloat((el as HTMLElement).style.height))];

describe('SystemIcon', () => {
  it('renders a decorative silhouette for a known id', () => {
    const { container } = render(<SystemIcon systemId="gb" size={16} />);
    const svg = container.querySelector('svg[data-system-icon="gb"]')!;
    expect(svg).toHaveAttribute('aria-hidden', 'true');
    expect(svg).toHaveAttribute('fill', 'currentColor');
    expect(box(svg)).toEqual([24, 16]);
  });
  it('is an image with a name when titled', () => {
    render(<SystemIcon systemId="gb" size={20} title="Game Boy" />);
    expect(screen.getByRole('img', { name: 'Game Boy' })).toHaveAttribute('data-system-icon', 'gb');
  });
  it('draws an abbreviation without outline below 20px', () => {
    const { container } = render(<SystemIcon systemId="cps2" size={16} />);
    const m = container.querySelector('[data-system-icon="cps2"]')!;
    expect(m).toHaveTextContent('CPS2');
    expect(m).not.toHaveClass('system-mount--outline');
    expect(box(m)).toEqual([24, 16]);
  });
  it('outlines from 20px and adds photo-corners from 24px', () => {
    const { container, rerender } = render(<SystemIcon systemId="cps2" size={20} />);
    expect(container.querySelector('.system-mount--outline')).not.toBeNull();
    expect(container.querySelectorAll('.system-mount__corner')).toHaveLength(0);
    rerender(<SystemIcon systemId="cps2" size={24} />);
    expect(container.querySelectorAll('.system-mount__corner')).toHaveLength(4);
  });
  it('uses the first four letters of a user-defined id', () => {
    const { container } = render(<SystemIcon systemId="x68000" size={16} />);
    expect(container.querySelector('[data-system-icon="x68000"]')).toHaveTextContent('X680');
  });
  it('draws a folder for an unrecognised folder', () => {
    const { container } = render(<SystemIcon systemId={null} size={16} />);
    expect(container.querySelector('[data-system-icon="unknown"]')).not.toBeNull();
  });
});
```

- [ ] **Step 2: Fail.** `pnpm vitest run src/components/system/SystemIcon.test.tsx`
- [ ] **Step 3: Implement**, CSS, i18n key in both locales.
- [ ] **Step 4: Pass**, gate, commit: `feat(system): add SystemIcon with Slot-shaped fallback`.

---

### Task 5: `SystemLabel` and `systemLookup`

**Files:**
- Create: `src/components/system/SystemLabel.tsx`, `src/lib/systemLookup.ts`
- Modify: `src/styles/globals.css` (`.system-label`)
- Test: `src/components/system/SystemLabel.test.tsx`, `src/lib/systemLookup.test.ts`

**Interfaces:**

```ts
export function SystemLabel(p: { systemId: string | null; name: string; size?: number; className?: string; trailing?: boolean }): JSX.Element;
// <span class="system-label"><SystemIcon aria-hidden/><span class="system-label__name">{name}</span></span>
// trailing: icon after the name (spec §A.6 rule 2) — icon stays aria-hidden; the label gets title={name}.

export function systemByFolder(systems: readonly SystemView[], folderOrDir: string): SystemView | undefined;
// exact `folder` match, then `dir` match, then case-insensitive `folder`.
```

CSS: `.system-label { display:inline-flex; align-items:center; gap:.375em; min-width:0 } .system-label__name { overflow:hidden; text-overflow:ellipsis; white-space:nowrap }`.

- [ ] **Step 1: Failing tests** — accessible name equals the visible name (`getByText('Game Boy')` and the span containing it has no other accessible text; `container.querySelector('[data-system-icon]')` has `aria-hidden`); `.system-label__name` present; `trailing` puts the icon last. `systemByFolder` cases: exact folder, dir, case-insensitive, unknown → `undefined`.
- [ ] **Step 2: Fail. Step 3: Implement. Step 4: Pass, gate, commit:** `feat(system): add SystemLabel and folder lookup`.

---

### Task 6: Icons in the sidebar and the filter panel (L3, L4)

**Files:**
- Modify: `src/components/library/Sidebar.tsx` (`scopeButton` at `:108` gains optional `icon?: ReactNode`, rendered before the label; Systems tab at `:191` passes `<SystemIcon systemId={s.system_id} size={16} />`)
- Modify: `src/components/library/FilterPanel.tsx` (`Option` at `:100` gains `icon?: ReactNode` between control and label text; systems group at `:203` passes it)
- Test: create `src/components/library/Sidebar.test.tsx`, `src/components/library/FilterPanel.test.tsx` (neither exists today)

- [ ] **Step 1: Failing tests.** Build a `LibraryView` with `generateLibrary({ seed: 7, count: 240, root: '/mock/Small library', generation: 1, now: 1_760_000_000 })` from `@/ipc/mock/fixture`. Sidebar: switch to the Systems tab (`getByRole('tab', { name: 'Systems' })`), assert every system button contains `[data-system-icon][aria-hidden="true"]` and its accessible name still starts with the system name. FilterPanel: every system checkbox's label contains `[data-system-icon]`; `getByLabelText(<system name>, { exact: false })` still finds the checkbox.
- [ ] **Step 2: Fail. Step 3: Implement. Step 4: Pass**; also run `pnpm vitest run src/screens/LibraryScreen.test.tsx` (unchanged assertions must stay green). Gate, commit: `feat(library): show system icons in the sidebar and filters`.

### Task 7: Icons in the grid, table and drawer (L1, L2, L5)

**Files:**
- Modify: `src/components/library/CoverGrid.tsx:159` — `.cover-tile__system` content → `<SystemLabel systemId={sys?.system_id ?? null} name={sys?.name ?? ''} size={16} />`
- Modify: `src/components/library/EntryTable.tsx:176` — same, in `cell--system`
- Modify: `src/components/library/DetailDrawer.tsx:203` — `<dd><SystemLabel … size={20} /></dd>`
- Modify: `src/styles/globals.css` (`.cover-tile__system` becomes `min-width:0` flex child; keep the 18px line height)
- Test: append to `CoverGrid.test.tsx`, `EntryTable.test.tsx`, `DetailDrawer.test.tsx`

- [ ] **Step 1: Failing tests** — one `it('shows the system icon beside the name')` per file asserting `[data-system-icon="<fixture id>"]` exists in the selected tile / first row / drawer system `dd`, and the existing system-name text assertion still holds.
- [ ] **Step 2–4:** fail, implement, pass. Gate, commit: `feat(library): show system icons in grid, table and drawer`.

### Task 8: Icons in reports, fill and organise (R1, F1, F2, O1–O3)

**Files:**
- Modify: `src/lib/audit.ts` (`AuditRow.system_id: string | null`, set from `s.system_id`, total row `null`); `src/lib/audit.test.ts` (assert it)
- Modify: `src/components/reports/ReportsPanel.tsx:118` — `<th scope="row" title={r.folder}><SystemLabel systemId={r.system_id} name={r.name} size={16} /></th>`
- Modify: `src/components/fill/FillWizard.tsx:513` — `const sys = systemByFolder(view.systems, s.folder)`; `<th scope="row"><SystemLabel systemId={sys?.system_id ?? null} name={s.folder} size={16} /></th>` (text stays the folder, as today)
- Modify: `src/components/fill/ReviewQueue.tsx:229` — leading `<SystemIcon systemId={systemByFolder(view.systems, item.folder)?.system_id ?? null} size={16} />` inside the `<p className="hint">` before `{item.folder}`; make the `p` `inline-flex items-center gap-1.5`
- Modify (insertions only): `src/components/organize/SystemGroups.tsx` — import line; at `:107` insert `<SystemIcon systemId={group.systemId} size={16} />` immediately before the `<span className="text-caption …">{row.systemName(group.systemId)}</span>`; at `:180` wrap nothing — insert `<SystemIcon systemId={item.system_id} size={16} />` as the sibling immediately before `<NativeSelect` (adornment, rule 4)
- Modify (insertions only): `src/components/organize/ReviewBuckets.tsx:141` — before `{title}` insert `{bucket.systems.map((id) => <SystemIcon key={id} systemId={id} size={16} />)}`
- Test: append to `ReportsPanel.test.tsx`, `FillWizard.test.tsx`, `ReviewQueue.test.tsx`, `SystemGroups.test.tsx`, `ReviewBuckets.test.tsx`: one icon-presence assertion each; existing text assertions unchanged.

- [ ] **Step 1: Failing tests. Step 2: Fail. Step 3: Implement** (check `git diff --stat src/components/organize` shows only additions). **Step 4: Pass**, gate.
- [ ] **Step 5: Commit in three:** `feat(reports): show system icons in the audit table` (audit + ReportsPanel), `feat(fill): show system icons in fill summary and review` (FillWizard + ReviewQueue), `feat(organize): show system icons on group triggers and buckets` (organise files only).

---

## Group V — Views

### Task 9: View prefs model, persistence and i18n

**Files:**
- Create: `src/lib/viewPrefs.ts`, `src/lib/viewPrefs.test.ts`
- Modify: `src/stores/viewStore.ts`; Create: `src/stores/viewStore.test.ts`
- Modify: `src/screens/LibraryScreen.tsx` (call `loadFor(view.root)` in an effect keyed on `view.root`)
- Modify: `locales/{en,es}/ui.json` — **all** `layouts.*` keys of spec §B.10 now (so later tasks only consume them); `layouts.sizeStops` as an object `{ "xs": …, "s": …, "m": …, "l": …, "xl": … }`; `layouts.group.*`, `layouts.column.*`, `layouts.pane.*` as objects.

**Interfaces:**

```ts
// viewPrefs.ts
export type ViewId = 'covers' | 'details' | 'columns' | 'gallery' | 'mounts' | 'shelves';
export const VIEW_IDS: readonly ViewId[]; // switcher order: covers, details, columns, gallery, mounts, shelves
export type CoverSize = 'xs' | 's' | 'm' | 'l' | 'xl';
export const COVER_SIZE_IDS: readonly CoverSize[];
export type GroupBy = 'none' | 'system' | 'folder' | 'region' | 'art' | 'letter';
export type Density = 'comfortable' | 'compact';
export type DetailsColumnId = 'thumb' | 'name' | 'system' | 'folder' | 'regions' | 'ext' | 'size' | 'mtime' | 'cover' | 'title' | 'screenshot';
export interface DetailsColumn { id: DetailsColumnId; width?: number }
export interface ViewPrefs { view: ViewId; coverSize: CoverSize; groupBy: GroupBy; art: Media; columns: DetailsColumn[]; density: Density; inspector: boolean }
export const DEFAULT_PREFS: ViewPrefs; // covers, m, none, cover, [name,system,size,mtime,cover,screenshot,title], comfortable, false
export const PREFS_KEY = 'coverly.views.v1';
export const LEGACY_LAYOUT_KEY = 'coverly.layout';
export const MAX_LIBRARIES = 20;
/** Field-by-field validation; unknown values → defaults; `name` always kept first-present. */
export function sanitisePrefs(raw: unknown): ViewPrefs;
export interface PrefsFile { last: ViewPrefs; byLibrary: Record<string, ViewPrefs>; order: string[] } // order = LRU, most recent last
export function readPrefsFile(storage: Storage | undefined): PrefsFile; // migration when PREFS_KEY missing
export function writePrefsFile(storage: Storage | undefined, file: PrefsFile): void; // swallow errors
export function remember(file: PrefsFile, root: string, prefs: ViewPrefs): PrefsFile; // LRU trim to 20
export function prefsFor(file: PrefsFile, root: string): ViewPrefs; // byLibrary[root] ?? last
```

```ts
// viewStore additions
prefs: ViewPrefs;
root: string | null;
/** Load the prefs of a library (Explorer's per-folder memory). */
loadFor(root: string): void;
setView(view: ViewId): void;
setPrefs(patch: Partial<ViewPrefs>): void; // sanitised, saved under root and last
/** @deprecated alias for one release (V-D5): 'grid' for covers, 'list' otherwise. */
layout: Layout;
setLayout(layout: Layout): void; // maps to setView
```

`reset()` keeps `prefs` and `root`.

- [ ] **Step 1: Failing tests** — `viewPrefs.test.ts` (with `MemoryStorage` from `src/test/storage.ts`):

```ts
it('defaults when nothing is stored', () => expect(readPrefsFile(new MemoryStorage()).last).toEqual(DEFAULT_PREFS));
it.each([['list', 'details'], ['grid', 'covers'], ['junk', 'covers']])('migrates coverly.layout=%s to %s', (old, view) => {
  const s = new MemoryStorage(); s.setItem(LEGACY_LAYOUT_KEY, old);
  expect(readPrefsFile(s).last.view).toBe(view);
});
it('falls back field by field', () => {
  expect(sanitisePrefs({ view: 'gallery', coverSize: 'huge', groupBy: 'system', art: 'x', columns: 'no', density: 'compact', inspector: 'yes' }))
    .toEqual({ ...DEFAULT_PREFS, view: 'gallery', groupBy: 'system', density: 'compact' });
});
it('drops unknown and duplicate columns and keeps name', () => {
  expect(sanitisePrefs({ columns: [{ id: 'size' }, { id: 'bogus' }, { id: 'size' }] }).columns.map((c) => c.id)).toEqual(['name', 'size']);
});
it('keeps prefs per library and falls back to last', () => {
  let f = readPrefsFile(new MemoryStorage());
  f = remember(f, '/a', { ...DEFAULT_PREFS, view: 'columns' });
  expect(prefsFor(f, '/a').view).toBe('columns');
  expect(prefsFor(f, '/b').view).toBe('columns'); // last
});
it('keeps at most 20 libraries, least recently used dropped', () => {
  let f = readPrefsFile(new MemoryStorage());
  for (let i = 0; i < 21; i++) f = remember(f, `/lib${i}`, DEFAULT_PREFS);
  expect(Object.keys(f.byLibrary)).toHaveLength(20);
  expect(f.byLibrary).not.toHaveProperty('/lib0');
});
it('survives a throwing storage', () => {
  const bad = { getItem: () => { throw new Error('x'); }, setItem: () => { throw new Error('x'); } } as unknown as Storage;
  expect(readPrefsFile(bad).last).toEqual(DEFAULT_PREFS);
  expect(() => writePrefsFile(bad, readPrefsFile(bad))).not.toThrow();
});
```

`viewStore.test.ts`: `setView('shelves')` persists under the loaded root; `loadFor('/b')` after `/a` set to columns gives columns (last) and `/a` stays separate after changing `/b`; `reset()` keeps prefs; `layout` alias tracks `prefs.view`. Use `vi.stubGlobal('localStorage', new MemoryStorage())` and re-import the store with `vi.resetModules()` in `beforeEach`.

- [ ] **Step 2: Fail. Step 3: Implement** (`viewStore` reads `readPrefsFile(globalThis.localStorage)` at creation; every setter → `remember` + `writePrefsFile`). Add the i18n keys (both files, same structure). **Step 4: Pass** (incl. `src/i18n/parity.test.ts`), gate, commit: `feat(views): per-library view prefs with layout migration`.

---

### Task 10: Grouping model and row model

**Files:**
- Create: `src/lib/grouping.ts`, `src/lib/grouping.test.ts`, `src/lib/rowModel.ts`, `src/lib/rowModel.test.ts`
- Modify: `src/stores/useVisibleEntries.ts` (adds `sections` memoised on `[visible, groupBy, systems, frozenArt]`)

**Interfaces:**

```ts
// grouping.ts
export interface Section { key: string; label: string; labelKey?: string; systemId?: string | null; start: number; count: number }
export interface GroupContext { systems: readonly SystemView[]; regionOrder: readonly Region[]; artMedia: readonly Media[] }
/** Stable-partitions `entries` by group (groups first, current order kept inside) and
 *  returns the reordered list plus sections as index ranges into it. */
export function groupEntries(entries: readonly EntryView[], by: GroupBy, ctx: GroupContext): { ordered: readonly EntryView[]; sections: readonly Section[] };
```

Rules: `none` → one section covering everything, `ordered === entries` (same reference). `system` → sidebar order (`systems` index order), `systemId` set, label = name; `folder` → `dir` relative to the system folder (`''` shown as the system name + " (root)" via `labelKey: 'layouts.group.root'` — add that key in this task if missing); `region` → first region of the entry in `REGIONS` order, entries with no region last (`labelKey: 'layouts.group.noRegion'`); `art` → `missingCover` (cover missing), `missingOther` (cover present, any other media in `artMedia` missing), `complete`; `letter` → `#` for non-letters, then A–Z (strip diacritics, `localeCompare` base). Unknown system index → its own trailing section.

```ts
// rowModel.ts
export type Row = { kind: 'header'; section: Section; sectionIndex: number } | { kind: 'items'; start: number; end: number; sectionIndex: number };
export interface RowModel { rows: readonly Row[]; rowOfIndex: Int32Array; headerRows: readonly number[] }
/** `perRow` items per row (1 for lists). With one 'none' section no header row is emitted. */
export function buildRowModel(sections: readonly Section[], perRow: number, withHeaders: boolean): RowModel;
```

`useVisibleEntries` returns `{ visible: ordered, sections, … }` so every view and `FillWizard` (which consumes `visible`) sees the grouped order. While a fill job is running (`isRunning(useJobStore.getState().job)`) and `groupBy === 'art'`, keep the section assignment computed at run start (`useRef` snapshot keyed by entry id); recompute when the run ends.

- [ ] **Step 1: Failing tests** — `grouping.test.ts`: one fixture of 12 hand-made entries over 3 systems; per `GroupBy` assert `sections.map(s => [s.key, s.count])` and that inside each section the input order is kept; `none` returns the same array reference; unknown system → last group; repeated calls with the same inputs give equal results; perf smoke:

```ts
it('groups 12k entries under 30 ms', () => {
  const view = generateLibrary({ seed: 12, count: 12_000, root: '/m', generation: 1, now: 1_760_000_000 });
  const t0 = performance.now();
  groupEntries(view.entries, 'system', ctx(view));
  expect(performance.now() - t0).toBeLessThan(30);
});
```

`rowModel.test.ts`: 2 sections of 5 and 3 items, perRow 4 → rows `[H, items 0–4, items 4–5, H, items 5–8]`; `rowOfIndex[6] === 4`; `withHeaders=false` → no header rows; single `none` section → no header.
- [ ] **Step 2–4**, gate, commit: `feat(views): grouping model and row model`.

---

### Task 11: Covers view with section headers and the always-rendered active row

**Files:**
- Create: `src/components/library/views/types.ts`, `src/components/library/views/SectionHeaderRow.tsx`, `src/hooks/useActiveRange.ts`, `src/components/library/views/CoversView.tsx`
- Modify: `src/components/library/CoverGrid.tsx` → `export { CoversView as CoverGrid } from './views/CoversView'` plus the re-exported `entryIdOf`, `TILE_W`, `TILE_H`, `GAP` (kept so existing tests and imports work); move `CoverGrid.test.tsx` to `views/CoversView.test.tsx` in this task (git mv) and extend it.
- Modify: `src/screens/LibraryScreen.tsx` (pass `sections`)
- Modify: `src/styles/globals.css` (`.section-row`: 32px, 1px `--cr-border` rule, 3px Buckram cap at the leading edge, Archivo label, count)

**Interfaces:**

```ts
// views/types.ts
export interface ViewProps extends EntryListProps {
  sections: readonly Section[];
  prefs: ViewPrefs;
  /** Space: quick look at the active entry. */
  onPreview(id: number): void;
  /** Mounts: Enter on a slot. */
  onOpenArt?(id: number, media: Media): void;
}
// useActiveRange.ts
/** rangeExtractor for TanStack Virtual that always includes `activeRow` and the
 *  sticky header row of the active section (`stickyRow`), sorted and unique. */
export function useActiveRange(activeRow: number, stickyRow: number | null): (r: Range) => number[];
// SectionHeaderRow.tsx
export function SectionHeaderRow(p: { section: Section; colSpan: number; rowIndex: number; style: CSSProperties; sticky?: boolean }): JSX.Element;
// <div role="row" aria-rowindex class="section-row"><div role="rowheader" aria-colspan={colSpan} aria-label={t('layouts.groupCount',{group,n})}>{systemId!==undefined && <SystemIcon size={20}/>}{label} <span>{n}</span></div></div>
```

CoversView = CoverGrid's current code over `buildRowModel(sections, columns, groupBy !== 'none')`; header rows have height 32, item rows `TILE_H + GAP` (`estimateSize` by row kind, fixed, no measuring); `aria-rowcount = rows.length`; arrow navigation via `nextIndex` on item indices then mapped through `rowOfIndex`, but **ArrowUp/Down move to the same column in the previous/next items row** (a partial last row in a section clamps); `rangeExtractor = useActiveRange(rowOfIndex[selectedIndex], stickyHeaderRowFor(selectedRow))`. Size metrics stay the constants until Task 13.

- [ ] **Step 1: Failing tests** (append to moved `CoversView.test.tsx`; `stubLayout(1200, 800)`):
  - `renders section headers as row > rowheader[aria-colspan]` (groupBy system): `getAllByRole('rowheader')[0]` has `aria-colspan` = column count and name `"<system>, <n> games"`, contains `[data-system-icon]`.
  - `keeps the active tile rendered after scrolling away`: select entry index 5, set `grid.scrollTop = 1_000_000`, `fireEvent.scroll(grid)`, then `document.getElementById(grid.getAttribute('aria-activedescendant')!)` is not null.
  - `ArrowDown at a partial last row of a section lands in the next section` (2 sections).
  - existing 12k virtualisation test still renders < 200 tiles.
- [ ] **Step 2–4**, also apply `useActiveRange` to `EntryTable` (same test there: `EntryTable.test.tsx` "keeps the active row rendered"). Gate, commit: `feat(views): section headers and always-rendered active item`.

---

### Task 12: Shared keyboard model — type-ahead, group jumps, quick look

**Files:**
- Modify: `src/lib/keyboardNav.ts`, `src/lib/keyboardNav.test.ts`
- Create: `src/components/library/views/QuickLook.tsx` (+ `.test.tsx`), `src/components/library/views/useViewKeys.ts`
- Modify: `CoversView.tsx`, `EntryTable.tsx` to use `useViewKeys`

**Interfaces:**

```ts
// keyboardNav.ts (additions)
/** ⌥↑/⌥↓: first item of the previous/next section; null when there is none. */
export function groupJump(dir: -1 | 1, index: number, sections: readonly Section[]): number | null;
export interface TypeAhead { push(ch: string, now: number): string } // returns the buffer
/** Buffer resets after `ms` (500) of silence. */
export function createTypeAhead(ms?: number): TypeAhead;
/** Next index whose name starts with `prefix` (case/diacritic-insensitive), searching
 *  from `from + 1` and wrapping; `from` itself only if nothing else matches. */
export function findByPrefix(names: (i: number) => string, count: number, from: number, prefix: string): number | null;
/** Shelves ↑↓: index in `targetLefts` (screen x of each candidate) nearest to `left`. */
export function nearestByLeft(left: number, targetLefts: readonly number[]): number;
/** Columns: next focused pane for ←/→ given pane count; null at the ends. */
export function paneMove(key: 'ArrowLeft' | 'ArrowRight', pane: number, panes: number, onBranch: boolean): number | null;

// useViewKeys.ts
export function useViewKeys(o: { entries; sections; selectedIndex; onSelect(id); onOpen(id); onPreview(id); scrollToIndex(i); move(key): number | null }): (ev: KeyboardEvent<HTMLElement>) => void;
// Order: Enter → onOpen; ' ' → onPreview (preventDefault); Alt+ArrowUp/Down → groupJump;
// printable single char without ctrl/meta/alt → type-ahead on titleWithoutRegions(name);
// otherwise `move(ev.key)` (view-specific geometry). Never handles Tab.
```

`QuickLook`: Radix `Dialog` (from `@/components/ui/dialog`) with title = entry name, `<img alt="{name}, {media}">` via `ArtThumb`-like `useArtUrl` at full size for `prefs.art`, ←→ step through `entries` (calls `onSelect`), Space or Esc closes; under `prefers-reduced-motion` no transition.

- [ ] **Step 1: Failing tests** — `keyboardNav.test.ts`: `groupJump` forward/back/none; type-ahead buffer `'s','u'` within 500 ms → `'su'`, after 600 ms → `'u'`; `findByPrefix` wraps and is diacritic-insensitive (`'e'` finds `"Équipe"`); `nearestByLeft(105, [0, 100, 220]) === 1`; `paneMove`. `QuickLook.test.tsx`: opens with the entry name as dialog name, ArrowRight calls `onSelect` with the next id, Escape closes. Covers test: typing `t` `e` selects the first "Te…" entry; Space opens a dialog; Tab from the grid moves focus out (`userEvent.tab()` → `document.activeElement !== grid`).
- [ ] **Step 2–4**, wire `onPreview` in `LibraryScreen` (state `previewId`), gate, commit: `feat(views): shared keyboard model with type-ahead and quick look`.

---

### Task 13: Cover sizes and art choice in Covers

**Files:**
- Create: `src/lib/coverSizes.ts` (+ `.test.ts`)
- Modify: `src/components/library/views/CoversView.tsx`, `src/styles/globals.css` (size modifier classes `.cover-tile--xs` … `--xl`)
- Create: `src/hooks/useViewShortcuts.ts` (+ `.test.tsx`) — size part only now (⌘+/⌘=/⌘−/⌘0, ctrl/meta-wheel accumulating 100px per stop with `preventDefault`, `{ passive: false }`)

**Interfaces:**

```ts
export interface CoverMetrics { tileW: number; tileH: number; artW: number; artH: number; nameLines: 0 | 1 | 2; meta: 'badge' | 'line' | 'line+size' }
export const COVER_SIZES: Readonly<Record<CoverSize, CoverMetrics>>;
// xs 96 (art 96×128, no name, badge), s 128 (128×171, 1 line, badge), m 168 (today: TILE_H 296), l 220 (220×293), xl 300 (300×400, + file size)
// tileH = border+padding 14 + artH + 6 + nameLines*16 + (meta==='badge' ? 0 : 18) (+16 for xl size line); m must equal 296.
export function stepSize(size: CoverSize, delta: -1 | 1): CoverSize; // clamps
export function columnsFor(width: number, size: CoverSize, gap?: number): number;
```

XS/S tiles show a 20px `<SystemIcon title={name}>` corner badge (rule 3) and the name only in `title`/accessible name of the gridcell (`aria-label={entry.name}`). The tile art uses `prefs.art`; missing art of that media → the dashed Slot placeholder + name (existing `art-placeholder`).

- [ ] **Step 1: Failing tests** — `coverSizes.test.ts`: `COVER_SIZES.m.tileH === 296`, `stepSize('xl', 1) === 'xl'`, `columnsFor(1200, 'm') === 6`, `columnsFor(1200,'xs') === 11`, `columnsFor(1200,'xl') === 3`. `CoversView.test.tsx`: per stop, `aria-colcount` matches `columnsFor(1200, stop)`; XS tile has `role="img"` badge named with the system; `prefs.art='title'` renders thumbs for `title`. `useViewShortcuts.test.tsx`: ⌘+ / ⌘= / ⌘− / ⌘0 call `setPrefs({ coverSize })` in covers and do nothing (no `preventDefault`) in details; ignored while focus is in an `<input type=search>`; ctrl+wheel of 250px total → two steps.
- [ ] **Step 2–4**, gate, commit: `feat(views): cover size slider metrics and art choice`.

---

### Task 14: Details — column catalogue, picker, resize, density, thumbnail, group rows

**Files:**
- Create: `src/lib/detailsColumns.ts` (+ `.test.ts`), `src/components/library/views/DetailsView.tsx`, `src/components/library/views/ColumnPicker.tsx`
- Modify: `src/components/library/EntryTable.tsx` → re-export `DetailsView as EntryTable` and `ROW_H`; `git mv EntryTable.test.tsx views/DetailsView.test.tsx` and extend
- Modify: `src/styles/globals.css` (grid-template from column widths via CSS variable `--details-cols`; `.entry-row--compact` 24px)

**Interfaces:**

```ts
export interface ColumnDef { id: DetailsColumnId; labelKey: string; sortKey?: SortKey; defaultWidth: number; minWidth: number; render(e: EntryView, ctx): ReactNode }
export const COLUMN_DEFS: Readonly<Record<DetailsColumnId, Omit<ColumnDef,'render'>>>;
export function toggleColumn(cols: DetailsColumn[], id: DetailsColumnId): DetailsColumn[]; // name never removed
export function moveColumn(cols: DetailsColumn[], id: DetailsColumnId, dir: -1 | 1): DetailsColumn[];
export function resizeColumn(cols: DetailsColumn[], id: DetailsColumnId, width: number): DetailsColumn[]; // clamps to minWidth
export function gridTemplate(cols: DetailsColumn[]): string; // e.g. "minmax(200px,1fr) 160px 90px …"
export const ROW_HEIGHT: Record<Density, number>; // comfortable 34, compact 24
```

DetailsView: header row with `columnheader`s from `prefs.columns`; sortable ones keep today's sort buttons; `onContextMenu` and Shift+F10 / ContextMenu key on a focused header open `ColumnPicker` (a `DropdownMenu` with `DropdownMenuCheckboxItem` per column plus Move left/right items); resize handle (`role="separator" aria-orientation="vertical"`, pointer drag, ←→ by 8px when focused, double-click → fit: measure the widest rendered cell text with a canvas `measureText`, clamp 60–600); Thumb column renders `ArtThumb` (24px, 20px compact) with `loading="lazy" decoding="async"`; group rows via `SectionHeaderRow` (sticky through `useActiveRange` + `position: sticky` on the active section header); `aria-rowcount` includes header rows; system cell uses `SystemLabel` 16px.

- [ ] **Step 1: Failing tests** — `detailsColumns.test.ts`: toggle hides/shows, `name` cannot be hidden, move clamps, resize clamps, `gridTemplate` order. `DetailsView.test.tsx`: right-click a header → menu lists all 11 columns; unticking "Size" removes the `Size` columnheader and calls `setPrefs` (spy); Shift+F10 on a focused header opens the same menu; compact density → rows have `height: 24px`; group rows render with `aria-colspan` = visible column count; 12k entries render < 60 rows at 800px.
- [ ] **Step 2–4**, gate, commit: `feat(views): Details column picker, thumbnail column and row density`.

---

### Task 15: Docked inspector (V-D3)

**Files:**
- Modify: `src/components/library/DetailDrawer.tsx` (prop `docked?: boolean` → no close-on-Esc capture, `aside` role kept, width token unchanged)
- Modify: `src/screens/LibraryScreen.tsx` (render the drawer when `prefs.inspector && selected && view supports inspector`, else today's `drawerOpen` rule)
- Modify: `src/hooks/useViewShortcuts.ts` (⌘I toggles `inspector` except in columns/gallery and while typing)
- Test: `DetailDrawer.test.tsx` (docked renders without the close button stealing focus), `useViewShortcuts.test.tsx` (⌘I toggles in covers; no-op in columns and gallery)

`const INSPECTOR_VIEWS: ViewId[] = ['covers','details','mounts','shelves']` exported from `viewPrefs.ts`.

- [ ] Steps 1–4, gate, commit: `feat(views): optional docked inspector`.

---

### Task 16: Art slots (Mounts) view

**Files:**
- Create: `src/components/library/views/MountsView.tsx` (+ `.test.tsx`)
- Modify: `src/styles/globals.css` (`.mount-row` 112px; slot cells 72×96 cover, 128×96 title/screenshot)
- Modify: `src/screens/LibraryScreen.tsx` (`onOpenArt(id, media)` → select + open drawer + `ArtWorkspace` on that media; reuse whatever DetailDrawer already exposes to open the workspace — add a `initialWorkspaceMedia` prop if needed)

Behaviour (spec §B.4.5): `role="grid"`, `aria-colcount=4`; each row: cell 0 = name (first in DOM, drawn right via CSS `order`), cells 1–3 = cover, title, screenshot in that order. Each slot cell shows `ArtThumb` when present, otherwise the dashed Slot (reuse `SlotMark` states: `missing`, `filled`, `applied`, `applied-run` from the entry's marks and the job store's applied-this-run set if available, else `filled`); labelled `t('art.media.<m>') + ': ' + t('layouts.slotHas')` / `t('layouts.slotMissing', { media })` — add `layouts.slotHas` ("has art" / "con arte") in both locales. Active cell = `(row, col)` state; `aria-activedescendant="entry-<id>-c<col>"`; ←→ change col, ↑↓ rows keeping col, Home/End first/last row, Enter on col 0 → `onOpen`, on col 1–3 → `onOpenArt`. Group headers as in Covers. Overscan 1 screen.

- [ ] **Step 1: Failing tests** — renders < 30 rows for 12k at 800px; missing slot has the dashed state (`[data-slot-state="missing"]`) and label "No Cover"; →→ then Enter calls `onOpenArt(id, 'title')`; Enter on name cell calls `onOpen`; ↓ keeps the column; active descendant exists after scrolling far; Tab leaves.
- [ ] **Step 2–4**, gate, commit: `feat(views): Art slots view`.

### Task 17: Columns view

**Files:**
- Create: `src/components/library/views/ColumnsView.tsx` (+ `.test.tsx`)
- Modify: `src/styles/globals.css` (`.miller` horizontal flex with `overflow-x:auto`, panes 220px, games pane `flex:1 0 260px`, preview 280px)

Behaviour (§B.4.3): local state `{ systemIndex, folderPath, pane }`. Pane 1 `listbox` "Systems" (hidden when scope is a single system) with `SystemLabel` 16 + count; pane 2 "Folders" from a memoised `buildFolderTree(entriesOfSystem)` relative to the system folder, omitted when the system has no sub-folders; pane 3 "Games" — entries of the chosen system/folder **in the current sort**, `aria-activedescendant`; pane 4 "Preview" — `ArtThumb` for `prefs.art` + `SystemLabel` 20 + facts (no docked inspector). Each list pane is its own virtualiser (row 28px). Keys per pane: ↑↓/Home/End/Page move inside; → on a branch (system/folder) enters the next pane, ← goes back (`paneMove`); Enter on a game → `onOpen`, on a branch → enter; type-ahead per pane (`createTypeAhead`). Choosing in panes does **not** change `viewStore.scope`. Selecting a game calls `onSelect` so other views keep it. On mount, if a selection exists, derive `systemIndex`/`folderPath` from it and focus the Games pane.

- [ ] **Step 1: Failing tests** — four listboxes named Systems/Folders/Games/Preview (Folders absent for a system without sub-folders); → from Systems focuses Folders/Games; ← returns; Enter on a game calls `onOpen`; clicking a system does not change `useViewStore.getState().scope`; with scope = one system the Systems pane is hidden; 12k entries → Games pane renders < 60 options; no `Inspector` switch rendered (checked in Task 22).
- [ ] **Step 2–4**, gate, commit: `feat(views): Columns view`.

### Task 18: Gallery view

**Files:**
- Create: `src/components/library/views/GalleryView.tsx` (+ `.test.tsx`)
- Modify: `src/styles/globals.css` (`.gallery` grid: large art + metadata 280px; filmstrip 88px tall, 64px thumbs; `@media (prefers-reduced-motion: no-preference) { .gallery__art img { transition: opacity 120ms } }`)

Behaviour (§B.4.4): filmstrip `role="listbox" aria-orientation="horizontal"`, options `aria-describedby` group name when they start a section; group markers `aria-hidden`; horizontal virtualiser with `useActiveRange`. Large art: `Tabs` (cover/title/screenshot; local state seeded from `prefs.art`) above an `img` alt `"{name}, {media}"`; the large URL is requested only after the selection has been stable for 120 ms (`useDebouncedValue(selectedId, 120)` — create in `src/hooks/useDebouncedValue.ts` with a test). Metadata pane: name, `SystemLabel` 20, regions · size · ext, per-media status with `SlotMark` 16, button "Fill this game" → opens FillWizard scoped to selection (`setOverlay('wizard')` after `select`). ←→ along the strip; ↑↓ do nothing; Tab order: tabs → strip → metadata (no override).

- [ ] **Step 1: Failing tests** — with fake timers, arrowing 5 times quickly requests the large art once (spy on `useArtUrl` via the mock backend's `artUrl` call count); `img` alt; ↑ does nothing; Tab from the filmstrip reaches the next control (no trap); strip renders < 40 options for 12k.
- [ ] **Step 2–4**, gate, commit: `feat(views): Gallery view`.

### Task 19: Shelves view

**Files:**
- Create: `src/components/library/views/ShelvesView.tsx` (+ `.test.tsx`)
- Modify: `src/styles/globals.css` (`.shelf` header 32px + strip height = `COVER_SIZES[size].tileH`)

Behaviour (§B.4.6): sections from `groupEntries(…, groupBy === 'none' ? 'system' : groupBy, …)` (computed here when the store's groupBy is `none`); vertical virtualiser of shelves (fixed height per size stop), each shelf a horizontal virtualiser of at most 60 items + a "See all {n}" cell when `count > 60`; header = `SectionHeaderRow` + "See all" link-button. "See all" → `setScope({ kind: 'system', system })` for system groups or the matching filter for others (folder → `setScope({kind:'folder',…})`; region/art/letter → set the filter in `filterStore` if one exists for it, else scope stays and only the view switches), then `setView('covers')`. Items: tiles from CoversView's tile renderer (extract `CoverTile` into `views/CoverTile.tsx` in this task, used by both); no system badge when grouped by system, otherwise a 20px titled badge. Keys: ←→ within a shelf; ↑↓ → `nearestByLeft` over the next shelf's rendered item lefts (`getBoundingClientRect`), scrolling that shelf if needed; End reaches "See all", Enter on it = header link. Each shelf keeps its own `scrollLeft` (map keyed by section key).

- [ ] **Step 1: Failing tests** — shelf of 100 ends with a "See all 100" cell; Enter on it sets scope and view covers; ↓ calls `nearestByLeft` (stub rects) and lands on the expected id; grouped by system → no badges; renders < 10 shelves × < 20 tiles at 1200×800 for 12k.
- [ ] **Step 2–4**, gate, commit: `feat(views): Shelves view`.

---

### Task 20: View switcher

**Files:**
- Create: `src/components/library/ViewSwitcher.tsx` (+ `.test.tsx`)
- Modify: `src/components/library/Toolbar.tsx` (replace the Grid/List `SegmentedControl` at `:136-157`)
- Modify: `src/styles/globals.css` (hairline separator between segment 4 and 5; at `max-width: 1180px` hide `.view-switcher__segments`, show `.view-switcher__menu`)

Six `SegmentedControlItem`s in `VIEW_IDS` order, `aria-label={t('layouts.<id>')}`, lucide icons `LayoutGrid`, `Table2`, `Columns3`, `GalleryThumbnails`, `GalleryHorizontalEnd`, `Library`; `Tip` content `"{name}  {mod}{n}"` with `Kbd`; the radiogroup `aria-label={t('layouts.label')}`. Narrow: `DropdownMenu` trigger `View ▾` (current view icon + `layouts.label`) with `DropdownMenuRadioGroup` and `DropdownMenuShortcut`. (Both are in the DOM; CSS shows one — the test checks both are wired to the same value.) Check each lucide icon name exists in `lucide-react@1.59.0` (`node -e "console.log(Object.keys(require('lucide-react')).filter(k=>/^(Table2|Columns3|GalleryThumbnails|GalleryHorizontalEnd|Library|Settings2)$/.test(k)))"`); substitute the nearest existing one if a name moved.

- [ ] **Step 1: Failing tests** — radiogroup "View" has 6 radios in order; clicking "Art slots" calls `setView('mounts')` and it becomes checked; the menu variant lists 6 `menuitemradio`s with shortcuts.
- [ ] **Step 2–4**, gate, commit: `feat(toolbar): six-view switcher`.

### Task 21: View options popover

**Files:**
- Create: `src/components/library/ViewOptions.tsx` (+ `.test.tsx`)
- Modify: `src/components/library/Toolbar.tsx` (trigger `Settings2` button "View options" next to the switcher)

Per-view controls (spec §B.6): Size `Slider` (5 stops, `aria-valuetext` from `layouts.sizeStops.*`) — covers, shelves; Row density `RadioGroup` — details; Art shown `RadioGroup` — covers, shelves; Group by `Select` — all but columns (shelves shows `none` as `system`); Sort by `Select` + direction toggle — all but details (reuses `viewStore.setSort`, keys from `SortKey` with existing `columns.*` labels + `library`); Columns checkbox list with ↑/↓ buttons — details (uses `toggleColumn`/`moveColumn`); Inspector `Switch` — `INSPECTOR_VIEWS` only. Also a "Missing art first" shortcut in shelves: a button that sets `groupBy: 'art'`.

- [ ] **Step 1: Failing tests** — for each view, the set of visible controls (table-driven over `VIEW_IDS`); slider change calls `setPrefs({ coverSize: 'l' })`; Columns/Gallery have no Inspector switch; Columns has no Group by.
- [ ] **Step 2–4**, gate, commit: `feat(toolbar): view options popover`.

### Task 22: Wire views, shortcuts and focus in LibraryScreen

**Files:**
- Modify: `src/screens/LibraryScreen.tsx` — `switch (prefs.view)` over the six view components with `{...list, sections, prefs, onPreview, onOpenArt}`; container `aria-label={t('layouts.announce', { view: t('layouts.'+id) })}` on each view root; after a view change (not on first mount) focus the new view's root (`useEffect` on `prefs.view` with a ref callback) and scroll the selected index into view.
- Modify: `src/hooks/useViewShortcuts.ts` — ⌘/Ctrl+1–6 → `setView(VIEW_IDS[n-1])` with `preventDefault`, working while focus is in the search box, ignored when `overlay !== 'none'`, `ev.defaultPrevented`, or `document.querySelector('[role="dialog"][data-state="open"], [role="alertdialog"][data-state="open"]')` exists (Radix modal, ArtWorkspace, QuickLook). Mount it from LibraryScreen's existing keydown effect area.
- Modify: `src/stores/viewStore.ts` — remove nothing yet; `layout` alias stays (V-D5).
- Remove `toolbar.grid`, `toolbar.list`, `toolbar.layout` keys from both locales after `grep -rn "toolbar\.\(grid\|list\|layout\)" src` returns nothing.

- [ ] **Step 1: Failing tests** in `useViewShortcuts.test.tsx`: Ctrl+3 → columns, Meta+3 → columns; works with focus in `input[type=search]`; no-op with an open Radix dialog in the DOM; ⌘+ in details not `defaultPrevented`.
- [ ] **Step 2–4**, gate, commit: `feat(views): switch views from the keyboard and move focus to the new view`.

### Task 23: LibraryScreen integration tests

**Files:**
- Modify: `src/screens/LibraryScreen.test.tsx` (replace `toggles between grid and table` at `:134`)

Tests (spec §C.2), each with `stubLayout(1440, 900)` and `MemoryStorage`:
- all six segments switch views; the active radio is checked; the focused element is the new view root whose accessible name includes the view name; `queryAllByRole('status')` count unchanged by a switch (no extra live region).
- ⌘1–⌘6 and Ctrl+1–6 switch, also from the search box; with Fill open (`click 'Fill missing art'`) Ctrl+3 does nothing; Ctrl+= in Details does not change `coverSize`.
- selection survives: select an entry in Covers, switch to Details → that row is `aria-selected` and is the active descendant; same into Columns (Games pane) and Gallery.
- persistence: choose Columns, unmount, mount → Columns; open library A (Columns) then B (set Shelves) then A → Columns.
- Group by System shows `[data-system-icon]` inside `rowheader`s in Covers, Details and Shelves.
- narrow (`stubLayout(1000, 800)` + `window.matchMedia` stub for `(max-width: 1180px)`) → menu trigger rendered and usable.
- compact density persists across remount.
- icon presence: sidebar systems, table system cell, drawer.

- [ ] Steps: write tests, run (they should pass if Tasks 9–22 are right; fix bugs found), gate, commit: `test(views): LibraryScreen integration coverage`.

### Task 24 (contingency, V-D6): Sized art thumbnails

Only if Task 25 step 4's Art-slots trace has a long task > 50 ms caused by image decode. Add `max_px: Option<u32>` to the art URL command in `src-tauri/src/ipc.rs` (resize with the `image` crate already in the lockfile — check `cargo tree -p coverly-roms -i image`), cache key includes it; mirror in `src/ipc/{types,backend,tauriBackend}.ts`, contract samples, mock (SVG sized); `ArtThumb` gets `maxPx` (Mounts 128, Thumb column 24, Gallery strip 64). Tests: contract test, `useArtUrl.test.ts` passes the size, Rust unit test for resize bounds. Run Rust gates. Commit: `perf(art): request sized thumbnails for small art slots`. If not needed, record "maxPx not needed: <trace numbers>" in `docs/TRACKER.md` in Task 26.

### Task 25: E2E with Chrome DevTools MCP (no commit unless fixes)

Start: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-views && pnpm exec vite --port 1457 --strictPort` (background). `new_page` with `isolatedContext: 'views'` at `http://localhost:1457/`; `evaluate_script` `localStorage.setItem('coverly.mock.pick','/mock/Fixture SD (12k)')`, reload, open the library from the start screen.

1. For each of the six views: `take_snapshot`; `evaluate_script` → `document.querySelectorAll('[role=row],[role=option],[role=gridcell]').length < 400` and tiles/rows < 200.
2. Icons: `[data-system-icon]` present in sidebar Systems tab, Filters, Details system column, cover meta line, inspector, Reports, fill summary (open wizard), organise group triggers (open "Organise loose files…" on a library with loose files, or `/mock/Small library`); each has `aria-hidden="true"` or `role="img"` + `aria-label`.
3. Keyboard: `press_key` Meta+1…6 cycles; with Fill dialog open Meta+3 does nothing; type-ahead in Covers, Details, Columns; Tab leaves every view (Gallery included); Columns → / ←; Gallery ←→; Shelves ↑↓.
4. Performance: `performance_start_trace`, switch Covers → Details → Gallery → Shelves, scroll Covers to the end, `performance_stop_trace`; no long task > 200 ms, switch scripting < 100 ms. Second trace scrolling Art slots top → bottom: no long task > 50 ms (else do Task 24).
5. Persistence: Shelves + size L, reload → same; Details + compact, reload → same.
6. Reduced motion: `emulate` `prefers-reduced-motion: reduce`; Gallery image computed `transition-duration` is `0s`.
7. Visual: `emulate` light and dark; `resize_page` 1440×900 and 1024×768; screenshots of every view and the sidebar; open `http://localhost:1457/src/assets/system-icons/contact-sheet.html` and screenshot (no blank PS2/Neo Geo; pads fill the width). Mounts dashed slots ≥ 3:1 (`get_css_styles` on a missing slot border vs surface).
8. `list_console_messages`: no errors, no React key warnings.

Stop: `lsof -ti tcp:1457 -sTCP:LISTEN | xargs kill`. Any bug found → fix with a failing unit test first, gate, `fix(views): …` commit.

### Task 26: Docs, changelog and tracker

**Files:**
- Modify: `docs/04-architecture/desktop-app.md` — sections "System icons" (pipeline, component, fallback, how to regenerate with `pnpm system-icons`, how to switch an icon to the fallback) and "Library views" (ViewPrefs, persistence key and migration, grouping/rowModel, keyboard model, adding a view checklist).
- Modify: `README.md`, `README.es.md` — feature bullets: system icons; six views with shortcuts.
- Modify: `CHANGELOG.md` `[Unreleased]` → `### Added` entries for icons and views; `### Changed`: Grid/List became Covers/Details, saved choice migrated.
- Modify: `docs/TRACKER.md` — new block for this branch with evidence (test counts from `pnpm test`, E2E results and trace numbers from Task 25, Task 24 decision).
- [ ] Gate, commit: `docs: document system icons and library views`.

---

## Self-review against the spec

- §A.1–A.2 → Tasks 1–3; §A.3 → Tasks 4–5; §A.4 → Tasks 2, 4; §A.5 → Task 4 CSS + size table applied in Tasks 6–8, 11, 13, 17–19; §A.6 rules → Tasks 5 (trailing), 8 (adornment, no options/prose), 13 & 19 (lone badges); §A.7 sites L1–L6, R1, F1–F2, O1–O3 → Tasks 6, 7, 8, 11–19.
- §B.1 → Tasks 9, 10; §B.2/B.4 → Tasks 11, 13, 14, 16–19; §B.5 → Tasks 10, 11, 14, 17–19, 21; §B.6 → Tasks 13, 15, 20–22; §B.7 → Tasks 10, 11 (`useActiveRange`), 16, 18, 24, 25; §B.8 → Tasks 11, 12, 14, 16–19, 22; §B.9 → Tasks 12, 18, 25; §B.10 → Task 9 (+ `layouts.slotHas`, `layouts.group.root`, `layouts.group.noRegion` added where first used).
- §C.1 → unit tests in Tasks 1–21; §C.2 → Task 23; §C.3 → Task 25; §C.4 → manual check listed in Task 26's tracker entry (⌘1–6/⌘+/− in the Tauri build: `make dev`, no webview zoom, icons render in WebKit).
- §D order kept, with legal and data checks split out and views before the switcher so each commit stays green.
