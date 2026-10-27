# Settings Tabs: Library, Organise and Duplicates — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking. UI tasks also REQUIRE frontend-design:frontend-design: follow `docs/08-brand/brand.md`, use the `src/components/ui` primitives, and critique each changed screen with screenshots before committing.

**Goal:** Settings › Library keeps only the scan options and the recent libraries. Organise and Duplicates get their own tabs, with the common options visible and the advanced ones folded under «Opciones avanzadas». Long help moves behind «Más información». A settings search (⌘F on macOS, Ctrl+F elsewhere, `/` outside text fields) finds any setting and lands focus on it. The Duplicates and Organise screens link straight to their tab and come back with their plan intact. The tab list is grouped so future tabs (collections, devices, saves, RetroAchievements) fit without being built. Presentation only: `settings.json`, its schema, keys, defaults and validation do not change, and no Rust code changes.

**Architecture:** `navStore` gains two tab ids (`organize`, `duplicates`), `SETTINGS_GROUPS`, `settingsFrom` (where Back returns), `openFolds` (session-only fold state), `focusSetting` (the control to land on) and `resetNavStore()`. `Field.tsx` gains `MoreInfo` and `AdvancedFold` (a `Collapsible` with `forceMount`). `LibrarySection.tsx` is split into `LibrarySection` (scan + recents), `OrganizeSection` (new) and `DuplicatesSection` (new, wraps `DedupeSettings` split into common + advanced). `SettingsScreen` gets a grouped nav, the arrival effect (focus + manila mark), and `SettingsSearch` over a static registry (`registry.ts`) with a pure matcher (`search.ts`). `SettingsLink` opens a tab from a screen; `dedupeStore` / `organizeStore` keep a settings stamp so the screen can say «the settings changed, look again».

**Tech Stack:** React 19, TypeScript, Tailwind 3.4, zustand, Radix (via `src/components/ui`), lucide-react, Vitest + Testing Library + user-event. E2E: Chrome DevTools MCP on the Vite dev server with the mock backend.

**Spec:** [`docs/superpowers/specs/2026-10-26-settings-ui.md`](../specs/2026-10-26-settings-ui.md) (all sections; decisions D1–D15 are binding unless a deviation below says otherwise). Roadmap brief: `TODO.md` §1.6. Brand: [`docs/08-brand/brand.md`](../../08-brand/brand.md).

## Global Constraints

- **Worktree.** Work only in `/Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-settings-ui`, branch `feat/settings-tabs`, which tracks `origin/feat/settings-tabs`. Every shell command starts with `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-settings-ui &&`. Never switch branches and never touch `main`.
- **Merge main first.** Every task starts with `git fetch origin && git merge origin/main` (no rebase, never force-push). Other workflows change `locales/*.json`, `src/components/*`, `src/screens/DuplicatesScreen.tsx` (`feat/dedupe-screen-polish`) and `TODO.md` in parallel: resolve conflicts keeping both sides, rerun the gates, and push the merge commit at once.
- **Push after every commit (mandatory).** An earlier run lost a whole branch because it was never pushed. Every task ends with `git commit …`, then `git push`, then `git status -sb`; the first line must read `## feat/settings-tabs...origin/feat/settings-tabs` with **no** `[ahead N]`. Never end an agent with unpushed commits.
- **Commit messages.** Conventional (`feat(ui): …`, `refactor(ui): …`, `test(ui): …`, `fix(ui): …`, `docs: …`), granular, in English. **No** `Co-Authored-By` trailer, no "Generated with", and no mention of Claude or AI anywhere (owner rule; it overrides any harness attribution).
- **Do not edit** `docs/TRACKER.md` or `TODO.md` on this branch (they are updated on `main` after merge).
- **Presentation only.** No change to `crates/**`, `src-tauri/**`, `src/ipc/types.ts` settings types, the mock's settings defaults or validation. Each moved control keeps its `settingId`, its `name` attribute and its exact `update()` patch. Tests that assert saved values keep their assertions byte for byte.
- **TDD.** Write the failing test, run it and watch it fail for the expected reason, implement, watch it pass. Each "Run" step names the exact command.
- **Gates** (same as CI). Every task runs **G-WEB** before its commit: `pnpm check-all && pnpm build` (`check-all` = `tokens:check`, `lint`, `format:check`, `typecheck`, `test`). The first task and the last task also run **G-RUST**: `cargo fmt --all --check && cargo clippy --workspace --all-targets --all-features -- -D warnings && scripts/dev/clippy-windows.sh && cargo test --workspace --no-fail-fast` (a guard that nothing in the settings schema moved).
- **Copy.** Every UI string goes through `locales/en/ui.json` **and** `locales/es/ui.json` with the same keys and placeholders (`src/i18n/parity.test.ts`). Spanish: sentence case, no exclamation marks, exact counts, numbers in mono; tab names keep their capital inside a sentence («Cambiar en Coincidencias»). English uses the catalogue's British spelling (`Organise`). Before removing a key, `git grep -n '<key>'` across `src/` must show no other user.
- **Brand.** No new tokens (`pnpm tokens:check`). Archivo only for the screen title; Atkinson Hyperlegible Next for labels/nav; Atkinson Hyperlegible Mono only for data (paths, ids, extension lists, counts). No cards, no all-caps labels, no nav icons (spec §6.2).
- **Real data is read-only.** Never write to `~/Documents/ROMS`, `/Volumes/TinyExt/*` or `/Volumes/System`. Any manual write test uses a copy made with `cp -RXp` under `/Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-settings-ui/target/`, deleted afterwards, with no `._*` files. (This plan needs none: the E2E runs on the mock backend.)
- **Windows CI.** Paths in tests are compared by component, not by slash text; fixture mtimes are pinned. This plan adds no filesystem fixtures; keep it that way.
- **Servers and E2E.** Start the web UI with `pnpm dev --port 1464 --strictPort` (mock backend). Stop it **by port only**: `lsof -ti tcp:1464 -sTCP:LISTEN | xargs -r kill`. E2E uses Chrome DevTools MCP `new_page` with `isolatedContext: "settings-ui"`; fall back to Playwright MCP; if both profiles are locked, install `playwright-core` in `/private/tmp/claude-501/-Users-miguelangelesparzacalero-Documents-GitHub/35b7c020-f2ae-412f-b757-99c24a9a498b/scratchpad/settings-ui` and drive Chrome for Testing.
- **Secrets** (minisign key, `~/.coverly-release`, `.env*`, `.vercel`) are never read or printed.

## Deviations from the spec (decided in this plan)

- **SP-1: list anchors on the `<ol>`.** The keeper-rules list gets `id="setting-dedupe-keep-rules"` and the Matching region list `id="setting-matching-regions"`, each `tabIndex={-1}` and already labelled by its heading. The spec put the region anchor on the first row's control, but that row's only controls are «Move up» (disabled on the first row) and «Move down», so focus would land on an arbitrary button. The container-order sub-group gets `id="setting-dedupe-container-order"` (`role="group"`, `tabIndex={-1}`).
- **SP-2: fold count outside the string.** `settings.advancedOptions` is «Opciones avanzadas» / "Advanced options" with no placeholder; the trigger appends ` (n)` in a mono span. The same plain label is reused in search breadcrumbs (`{section} › {fold}`), where a count would be noise.
- **SP-3: folds open from the DOM, not the registry.** The arrival effect finds the target element (folded content is `forceMount`ed, so it exists while hidden), reads the nearest `[data-advanced-fold]`, and opens it. `openSettings(section, { focus })` therefore needs no registry lookup, and the registry's `fold` field is used only for the breadcrumb and checked by the registry test.
- **SP-4: two action entries.** The action radios are two registry entries (`setting-dedupe-action-quarantine`, `setting-dedupe-action-trash`) with their own labels, so «papelera» lands on the Trash radio itself.

## Review Focus

1. **No data change.** Every moved control saves the same patch as before; `Sections.test.tsx` assertions on `saved()` are unchanged; no file under `crates/`, `src-tauri/` or the mock's settings defaults is in the diff (`git diff origin/main --stat`).
2. **Round trip through Settings keeps the plan.** Opening Settings from Duplicates/Organise and coming back never calls `reset()` (only `back()` does), the plan and decisions survive, focus returns to the link, and the alert appears only when a plan-read setting changed (§8.6).
3. **Focus and keyboard.** Arrival focus is not stolen by the `<h1>` effect; ⌘F only on macOS, Ctrl+F elsewhere, macOS Ctrl+F untouched in text fields; `/` only outside text fields; Escape semantics; arrow keys across groups and in the narrow row (orientation follows layout); the nav stays a valid `tablist` (no `role="group"` inside).
4. **Folded content.** Drafts and ids survive closing a fold (`forceMount`), a field error opens its fold, and a closed fold with errors says so.
5. **Registry completeness.** The registry test fails if any `setting-` control in any tab lacks an entry, or an entry has no element.

---

## File structure

```
src/stores/navStore.ts                 + 'organize' | 'duplicates', SETTINGS_GROUPS, settingsFrom,
                                         openFolds/setFold, focusSetting/clearFocusSetting,
                                         openSettings(section, { focus }), resetNavStore()
src/stores/navStore.test.ts            (new)
src/App.tsx                            focus restore after Back from Settings to any screen
src/lib/keyboard.ts                    + isMac()
src/lib/settingsStamp.ts               (new) dedupeStamp(), organizeStamp()
src/lib/settingsStamp.test.ts          (new)
src/screens/SettingsScreen.tsx         grouped nav, orientation by width, arrival effect,
                                         search field + results, ⌘F / Ctrl+F / '/'
src/screens/SettingsScreen.test.tsx    tab order, arrows, arrival, search, shortcuts
src/components/settings/
  Field.tsx                            + MoreInfo, AdvancedFold, MonoIn, `more` prop,
                                         data-setting-row on Field / SwitchSetting
  Field.test.tsx                       (new)
  ListSetting.tsx                      (new) moved out of LibrarySection, unchanged
  LibrarySection.tsx                   scan options + recent libraries only
  OrganizeSection.tsx                  (new) moved from LibrarySection + organize_step
  DuplicatesSection.tsx                (new) DedupeCommon + fold(DedupeAdvanced)
  DedupeSettings.tsx                   split: DedupeCommon, DedupeAdvanced, RegionReadout
  MatchingSection.tsx                  id/tabIndex anchor on the region list (SP-1)
  registry.ts                          (new) SETTINGS_INDEX
  registry.test.tsx                    (new)
  search.ts                            (new) foldText, searchSettings (pure)
  search.test.ts                       (new)
  SettingsSearch.tsx                   (new) combobox + listbox + live count
  SettingsLink.tsx                     (new) «Ajustes de …» button for screens
  Sections.test.tsx                    library:* tests move to organize:/duplicates:
src/stores/dedupeStore.ts              + settingsStamp; replan() clears it
src/stores/organizeStore.ts            + settingsStamp; refresh() clears it
src/screens/DuplicatesScreen.tsx       + SettingsLink in header + changed alert
src/screens/OrganizeScreen.tsx         + SettingsLink in header + changed alert
src/screens/settingsLinks.test.tsx     (new) round trips through <Root />
src/styles/globals.css                 nav groups, narrow row, search, arrival mark
locales/{es,en}/ui.json                keys of spec §7 (+ SP-2), old keys removed when unused
README.md, README.es.md, CHANGELOG.md, docs/04-architecture/desktop-app.md,
docs/04-architecture/settings.md       tab names and «Settings → Library» wording
```

Tests that today call `useNavStore.getState().closeSettings()` to reset (`src/screens/StartScreen.test.tsx`, `src/screens/SettingsScreen.test.tsx`, `src/screens/LibraryScreen.test.tsx`, `src/components/AppFooter.test.tsx`, `src/components/settings/ProfilesSection.test.tsx`, `src/components/settings/Sections.test.tsx`, `src/components/update/UpdatePanel.test.tsx`) switch to `resetNavStore()` in Task 1.

---

## Task 1: Nav store: where Back returns, folds, focus target, reset

**Files:**
- Modify: `src/stores/navStore.ts`
- Modify: `src/App.tsx`
- Create: `src/stores/navStore.test.ts`
- Modify: the seven test files listed above (`closeSettings()` in `beforeEach` → `resetNavStore()`)

**Interfaces (produced):**
```ts
export interface OpenSettingsOptions {
  /** DOM id of the control to land on (settingId(path) or a list anchor). */
  focus?: string;
}
// NavState additions
settingsFrom: Exclude<Screen, 'settings'> | null;
openFolds: readonly string[];
focusSetting: string | null;
openSettings(section?: SettingsSection, opts?: OpenSettingsOptions): void;
setFold(id: string, open: boolean): void;
clearFocusSetting(): void;
export function resetNavStore(): void;
```

- [ ] **Step 0: Sync.** `git fetch origin && git merge origin/main && git push`
- [ ] **Step 1: Write the failing tests** in `src/stores/navStore.test.ts`:

```ts
import { beforeEach, describe, expect, it } from 'vitest';

import { resetNavStore, useNavStore } from './navStore';

const nav = () => useNavStore.getState();

describe('navStore settings round trip', () => {
  beforeEach(() => resetNavStore());

  it('records the screen Settings was opened from and returns there', () => {
    nav().openDuplicates('/Volumes/SD');
    nav().openSettings('library');
    expect(nav().settingsFrom).toBe('duplicates');
    nav().closeSettings();
    expect(nav().screen).toBe('duplicates');
    expect(nav().settingsFrom).toBeNull();
    expect(nav().duplicatesRoot).toBe('/Volumes/SD');
  });

  it('does not overwrite settingsFrom from inside Settings', () => {
    nav().openOrganize('/Users/demo/Downloads');
    nav().openSettings('library');
    nav().openSettings('matching');
    expect(nav().settingsFrom).toBe('organize');
    expect(nav().section).toBe('matching');
  });

  it('falls back to main when the screen has nothing to show', () => {
    nav().openDuplicates('/Volumes/SD');
    nav().openSettings();
    useNavStore.setState({ duplicatesRoot: null });
    nav().closeSettings();
    expect(nav().screen).toBe('main');
  });

  it('keeps the last tab, ignores unknown ids and sets the focus target', () => {
    nav().openSettings('art');
    nav().closeSettings();
    nav().openSettings();
    expect(nav().section).toBe('art');
    nav().setSection('nope' as never);
    expect(nav().section).toBe('art');
    nav().openSettings('library', { focus: 'setting-library-include-hidden' });
    expect(nav().focusSetting).toBe('setting-library-include-hidden');
    nav().clearFocusSetting();
    expect(nav().focusSetting).toBeNull();
  });

  it('keeps folds open for the session, not across a reset', () => {
    nav().setFold('library-advanced', true);
    nav().setFold('library-advanced', true);
    expect(nav().openFolds).toEqual(['library-advanced']);
    nav().closeSettings();
    expect(nav().openFolds).toEqual(['library-advanced']);
    nav().setFold('library-advanced', false);
    expect(nav().openFolds).toEqual([]);
    nav().setFold('x', true);
    resetNavStore();
    expect(nav().openFolds).toEqual([]);
  });
});
```

- [ ] **Step 2: Run** `pnpm vitest run src/stores/navStore.test.ts` — fails (`resetNavStore` not exported).
- [ ] **Step 3: Implement** in `src/stores/navStore.ts`:

```ts
const INITIAL = {
  screen: 'main' as Screen,
  section: 'general' as SettingsSection,
  organizeSource: null as string | null,
  organizeLaunch: null as OrganizeLaunch | null,
  duplicatesRoot: null as string | null,
  settingsFrom: null as Exclude<Screen, 'settings'> | null,
  openFolds: [] as readonly string[],
  focusSetting: null as string | null,
};

const known = (s: SettingsSection | undefined): s is SettingsSection =>
  s !== undefined && SETTINGS_SECTIONS.includes(s);

export const useNavStore = create<NavState>()((set) => ({
  ...INITIAL,
  openSettings: (section, opts) =>
    set((s) => ({
      screen: 'settings',
      section: known(section) ? section : s.section,
      // From inside Settings (a search result, a cross-tab link) Back
      // still returns to the screen that opened Settings.
      settingsFrom: s.screen === 'settings' ? s.settingsFrom : s.screen,
      focusSetting: opts?.focus ?? null,
    })),
  closeSettings: () =>
    set((s) => {
      const from = s.settingsFrom;
      const back: Screen =
        (from === 'duplicates' && !s.duplicatesRoot) ||
        (from === 'organize' && !s.organizeSource)
          ? 'main'
          : (from ?? 'main');
      return { screen: back, settingsFrom: null, focusSetting: null };
    }),
  setSection: (section) => {
    if (known(section)) set({ section });
  },
  setFold: (id, open) =>
    set((s) => {
      const has = s.openFolds.includes(id);
      if (open === has) return s;
      return {
        openFolds: open
          ? [...s.openFolds, id]
          : s.openFolds.filter((f) => f !== id),
      };
    }),
  clearFocusSetting: () => set({ focusSetting: null }),
  // …openOrganize / closeOrganize / openDuplicates / closeDuplicates unchanged
}));

/** Tests: back to the start screen with nothing remembered. */
export function resetNavStore(): void {
  useNavStore.setState({ ...INITIAL });
}
```

  In `src/App.tsx` change the focus-restore condition from `screen === 'main'` to `screen !== 'settings'` (the comment becomes "Back from Settings: focus returns to the control that opened it, on whichever screen that was"). Replace `useNavStore.getState().closeSettings()` in the seven `beforeEach` blocks with `resetNavStore()` (import it from `@/stores/navStore`).
- [ ] **Step 4: Run** `pnpm vitest run src/stores/navStore.test.ts` (passes), then G-WEB and G-RUST.
- [ ] **Step 5: Commit and push:** `git add src && git commit -m "feat(ui): remember where Settings was opened from, open folds and a focus target" && git push && git status -sb`

## Task 2: Settings primitives: «Más información», advanced fold, ListSetting

**Files:**
- Modify: `src/components/settings/Field.tsx`
- Create: `src/components/settings/ListSetting.tsx` (moved verbatim from `LibrarySection.tsx`; `LibrarySection` imports it)
- Create: `src/components/settings/Field.test.tsx`
- Modify: `locales/en/ui.json`, `locales/es/ui.json`

**Interfaces (produced):**
```ts
export function MonoIn(props: { text: string; value: ReactNode }): JSX.Element; // splits `text` at '\u0000'
export function MoreInfo(props: { id: string; children: ReactNode }): JSX.Element;
export function AdvancedFold(props: {
  id: string;              // 'library-advanced' | 'organize-advanced' | 'duplicates-advanced'
  count: number;           // registry entries inside (shown in mono)
  errorPaths: readonly string[]; // settings paths whose errors[path] (or path.*) live inside
  children: ReactNode;
}): JSX.Element;
// Field and SwitchSetting gain `more?: ReactNode` (rendered as <MoreInfo> after the hint)
// and `data-setting-row` on their wrapper (the arrival mark's target, Task 3).
```

Keys (both locales; SP-2):

| Key | es | en |
|-----|----|----|
| `settings.advancedOptions` | Opciones avanzadas | Advanced options |
| `settings.advancedHasErrors` | {label}, con errores | {label}, with errors |
| `settings.moreInfo` | Más información | More information |
| `settings.lessInfo` | Menos información | Less information |

- [ ] **Step 0: Sync.** `git fetch origin && git merge origin/main && git push`
- [ ] **Step 1: Write the failing tests** in `Field.test.tsx` (render inside `I18nProvider`; `resetSettingsStore()` + `resetNavStore()` in `beforeEach`; seed `useSettingsStore.setState({ errors: {} })`):
  - `more_info_toggles`: a `SwitchSetting` with `hint="Short."` and `more="Long."`. The button «More information» has `aria-expanded="false"`, its `aria-controls` element is `hidden`; click → `aria-expanded="true"`, text «Less information», paragraph visible. The switch's accessible description is exactly «Short.» (the long text is not in `aria-describedby`).
  - `advanced_fold_is_closed_and_counts`: `<AdvancedFold id="t" count={2} errorPaths={['library.ignored_dirs']}>` with a `ListSetting` inside. Trigger name «Advanced options (2)», `aria-expanded="false"`; the textbox is not accessible by role (hidden) but `document.getElementById('setting-library-ignored-dirs')` exists. The `(2)` span has class `font-mono`.
  - `advanced_fold_keeps_a_draft_when_closed`: open, type `bios` in the textbox (no blur), close via the trigger, reopen → the textbox still holds `bios`.
  - `advanced_fold_opens_on_error_and_flags_when_closed`: `act(() => useSettingsStore.setState({ errors: { 'library.ignored_dirs': { code: 'invalid' } } }))` → fold opens (`useNavStore.getState().openFolds` contains `t`); close it → trigger accessible name «Advanced options (2), with errors» and a `[data-fold-error]` dot is rendered.
- [ ] **Step 2: Run** `pnpm vitest run src/components/settings/Field.test.tsx` (fails: exports missing).
- [ ] **Step 3: Implement** in `Field.tsx`:

```tsx
const MARK = '\u0000';

/** A translated line with one value in the data face (brand §5). */
export function MonoIn({ text, value }: { text: string; value: ReactNode }) {
  const [before = '', after = ''] = text.split(MARK);
  return (
    <>
      {before}
      <span className="font-mono">{value}</span>
      {after}
    </>
  );
}

/** The long help behind a text toggle, after the short hint (D7): the
 * control's aria-describedby keeps reading only the short one. */
export function MoreInfo({ id, children }: { id: string; children: ReactNode }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const panel = `${id}-more`;
  return (
    <div className="grid gap-1">
      <Button
        variant="link"
        size="sm"
        className="h-auto justify-self-start p-0"
        aria-expanded={open}
        aria-controls={panel}
        onClick={() => setOpen((o) => !o)}
      >
        {t(open ? 'settings.lessInfo' : 'settings.moreInfo')}
      </Button>
      <p id={panel} hidden={!open} className={hintText}>
        {children}
      </p>
    </div>
  );
}

/** «Opciones avanzadas»: closed by default, open for the session once
 * opened (navStore.openFolds), content kept mounted so drafts and ids
 * survive (D5). Opens itself when a field inside gets an error. */
export function AdvancedFold({ id, count, errorPaths, children }: {
  id: string;
  count: number;
  errorPaths: readonly string[];
  children: ReactNode;
}) {
  const t = useT();
  const open = useNavStore((s) => s.openFolds.includes(id));
  const setFold = useNavStore((s) => s.setFold);
  const hasError = useSettingsStore((s) =>
    Object.keys(s.errors).some((k) =>
      errorPaths.some((p) => k === p || k.startsWith(`${p}.`)),
    ),
  );
  const had = useRef(false);
  useEffect(() => {
    if (hasError && !had.current) setFold(id, true);
    had.current = hasError;
  }, [hasError, id, setFold]);
  const label = t('settings.advancedOptions');
  const name = `${label} (${count})`;
  return (
    <Collapsible
      open={open}
      onOpenChange={(o) => setFold(id, o)}
      data-advanced-fold={id}
      className="grid gap-4"
    >
      <CollapsibleTrigger asChild>
        <Button
          variant="ghost"
          className="group justify-self-start px-1"
          aria-label={
            hasError && !open ? t('settings.advancedHasErrors', { label: name }) : name
          }
        >
          <ChevronRight
            aria-hidden
            className="transition-transform duration-fast ease-standard group-data-[state=open]:rotate-90 motion-reduce:transition-none"
          />
          {label} <span className="font-mono">({count})</span>
          {hasError && !open && (
            <span data-fold-error aria-hidden className="size-1.5 rounded-full bg-cr-danger" />
          )}
        </Button>
      </CollapsibleTrigger>
      <CollapsibleContent forceMount className="grid gap-4">
        {children}
      </CollapsibleContent>
    </Collapsible>
  );
}
```

  Radix `CollapsibleContent` with `forceMount` sets `hidden` while closed; confirm in the test (`toHaveAttribute('hidden')`). Add `more?: ReactNode` to `Field` and `SwitchSetting`, rendered as `{more && <MoreInfo id={id}>{more}</MoreInfo>}` after `FieldErrorText` / `FieldDescription`, and `data-setting-row=""` on both wrappers. Move `ListSetting` to `ListSetting.tsx` (no behaviour change; it accepts and passes through `more`).
- [ ] **Step 4: Run** the test file (passes), then G-WEB.
- [ ] **Step 5: Commit and push:** `git add src locales && git commit -m "feat(ui): add the advanced options fold and the more information toggle for settings" && git push && git status -sb`

## Task 3: Arrival: focus the target setting, open its fold, mark it

**Files:**
- Modify: `src/screens/SettingsScreen.tsx`
- Modify: `src/styles/globals.css`
- Modify: `src/screens/SettingsScreen.test.tsx`

- [ ] **Step 0: Sync.** `git fetch origin && git merge origin/main && git push`
- [ ] **Step 1: Write the failing tests** in `SettingsScreen.test.tsx`:
  - `lands_on_a_setting_and_marks_it`: `act(() => useNavStore.getState().openSettings('library', { focus: 'setting-library-include-hidden' }))` → the «Include hidden files» switch has focus (not the `<h1>`), its `[data-setting-row]` has `data-arrived="true"`; with `vi.useFakeTimers()` advance 1200 ms → attribute gone; `focusSetting` is `null`.
  - `opens_the_fold_that_holds_the_target`: wrap Library's ignored-folders `ListSetting` in an `AdvancedFold id="library-advanced"` in the test via the real component once Task 6 lands — for now, assert with `openSettings('matching', { focus: 'setting-matching-regions' })` that the region `<ol>` (SP-1) has focus.
  - `missing_target_focuses_the_tab`: `openSettings('art', { focus: 'setting-nope' })` → the «Art» tab has focus, `focusSetting` is `null`.
  - The existing «moves focus into the screen and back to the Settings button» still passes (no target → `<h1>`).
- [ ] **Step 2: Run** `pnpm vitest run src/screens/SettingsScreen.test.tsx` (new tests fail).
- [ ] **Step 3: Implement.** In `MatchingSection.tsx` add `id="setting-matching-regions" tabIndex={-1}` to the region `<ol>` (SP-1; no `name` changes). In `SettingsScreen.tsx`:

```tsx
const focusSetting = useNavStore((s) => s.focusSetting);
const openFolds = useNavStore((s) => s.openFolds);
// The screen replaces the one that opened it: focus starts at its title,
// unless it was opened to land on one setting.
useEffect(() => {
  if (useNavStore.getState().focusSetting === null) heading.current?.focus();
}, []);
useEffect(() => {
  if (!focusSetting) return;
  if (status !== 'ready' && section !== 'about' && status !== 'error') return;
  const nav = useNavStore.getState();
  const el = document.getElementById(focusSetting);
  if (!el) {
    nav.clearFocusSetting();
    document.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]')?.focus();
    return;
  }
  // Folded content is mounted but hidden (SP-3): open it, land next render.
  const fold = el.closest<HTMLElement>('[data-advanced-fold]')?.dataset.advancedFold;
  if (fold && !nav.openFolds.includes(fold)) {
    nav.setFold(fold, true);
    return;
  }
  arrive(el);
  nav.clearFocusSetting();
}, [focusSetting, section, status, openFolds]);
```

  With a module-level helper:

```ts
function arrive(el: HTMLElement) {
  const row = el.closest<HTMLElement>('[data-setting-row]') ?? el;
  const reduce = window.matchMedia?.('(prefers-reduced-motion: reduce)').matches;
  row.scrollIntoView({ block: 'center', behavior: reduce ? 'auto' : 'smooth' });
  el.focus({ preventScroll: true });
  row.dataset.arrived = 'true';
  window.setTimeout(() => delete row.dataset.arrived, 1200);
}
```

  CSS (`globals.css`, after `.settings-group`):

```css
/* Arrival mark (spec §6.1): a manila bar and tint, once. */
[data-setting-row] {
  border-radius: var(--cr-radius-sm);
  transition:
    background-color var(--cr-duration-slower) var(--cr-ease-standard),
    box-shadow var(--cr-duration-slower) var(--cr-ease-standard);
}
[data-setting-row][data-arrived='true'] {
  box-shadow: inset 3px 0 0 var(--cr-manila);
  background-color: color-mix(in srgb, var(--cr-manila) 14%, transparent);
  transition: none;
}
@media (prefers-reduced-motion: reduce) {
  [data-setting-row] { transition: none; }
}
@media (forced-colors: active) {
  [data-setting-row][data-arrived='true'] {
    outline: 2px solid Highlight;
    box-shadow: none;
    background-color: transparent;
  }
}
```

  Add `data-setting-row` also to the levels group, the action `<fieldset>`, the rules `<ol>`'s wrapper and the region list wrapper so lists get the mark. Check text contrast on the tint in light and dark (screenshot in Task 15).
- [ ] **Step 4: Run** the test file (passes), then G-WEB.
- [ ] **Step 5: Commit and push:** `git add src && git commit -m "feat(ui): land on a setting with focus and a brief manila mark" && git push && git status -sb`

## Task 4: Organizar tab

**Files:**
- Create: `src/components/settings/OrganizeSection.tsx`
- Modify: `src/components/settings/LibrarySection.tsx` (remove `OrganizeSettings`, `ExternalToolsHint`; profiles list loading stays for the default profile)
- Modify: `src/components/settings/DedupeSettings.tsx` (remove the «Al organizar» field; it moves here, D15)
- Modify: `src/stores/navStore.ts` (`'organize'` after `'library'` in `SettingsSection` and `SETTINGS_SECTIONS`)
- Modify: `src/screens/SettingsScreen.tsx` (`SECTION_COMPONENTS.organize = OrganizeSection`)
- Modify: `src/components/settings/Sections.test.tsx`, `src/screens/SettingsScreen.test.tsx`
- Modify: `locales/{es,en}/ui.json`: add `settings.section.organize` (Organizar / Organise); remove `settings.organize.title` if `git grep` shows no other user

Content (spec §5.2). Visible, in this order: mode, default layout, names, nest in ROMs, download game catalogues, «Al organizar» (`dedupe.organize_step`, same `name="dedupe-organize-step"`, same patch). `<AdvancedFold id="organize-advanced" count={1} errorPaths={['organize.external_tools']}>`: external RAR programs (`ExternalToolsHint`, rewritten with `MonoIn`), then `settings.organize.advancedHint`. No `<fieldset>`/`<legend>` (the tab names the panel). `OrganizeSection` loads profiles with the same effect `LibrarySection` uses.

- [ ] **Step 0: Sync.** `git fetch origin && git merge origin/main && git push`
- [ ] **Step 1: Migrate the tests first.** In `Sections.test.tsx` rename `library: the organise defaults show and save` → `organize: the organise defaults show and save`, `library: external_tools_toggle_saves_off` → `organize: …`, `library: found_tools_listed_in_mono` → `organize: …`, `library: no_tools_shows_install_hint` → `organize: …`. Each uses `const panel = await open('organize')` and `within(panel)` instead of `getByRole('group', { name: 'Organise' })`. The three external-tools tests first click `within(panel).getByRole('button', { name: 'Advanced options (1)' })`. Add `«When organising» saves report` to the organise test (moved from the duplicates test, same `saved()` assertion on `dedupe.organize_step`). Add `library: shows no organise control` (`queryByRole('combobox', { name: 'Mode' })` is null on the Library panel). Update the tab-order test in `SettingsScreen.test.tsx` to insert `'Organise'` after `'Library'`, and the arrow-key test (`ArrowUp` twice from Library now reaches … compute from the new order; keep Home/End assertions).
- [ ] **Step 2: Run** `pnpm vitest run src/components/settings/Sections.test.tsx src/screens/SettingsScreen.test.tsx` (fails: no `organize` tab).
- [ ] **Step 3: Implement** as described. `ExternalToolsHint` becomes:

```tsx
function ExternalToolsHint({ found }: { found: FoundTool[] }) {
  const t = useT();
  if (found.length === 0) return <>{t('settings.organize.externalToolsNone')}</>;
  const list = found.map((f) => `${toolName(f.tool)} ${f.version} (${f.path})`).join(', ');
  return (
    <MonoIn
      text={t('settings.organize.externalToolsFound', { tools: '\u0000' })}
      value={<span className="[overflow-wrap:anywhere]">{list}</span>}
    />
  );
}
```

  Keep the inner span carrying both `font-mono` and `[overflow-wrap:anywhere]` on one element (the test checks both classes on the text node's element): render `MonoIn`'s value span with `className="font-mono [overflow-wrap:anywhere]"` by giving `MonoIn` an optional `className` prop.
- [ ] **Step 4: Run** the same tests (pass), then G-WEB.
- [ ] **Step 5: Commit and push:** `git add src locales && git commit -m "feat(ui): move the organise settings to their own tab" && git push && git status -sb`

## Task 5: Duplicados tab, common options on top

**Files:**
- Create: `src/components/settings/DuplicatesSection.tsx`
- Modify: `src/components/settings/DedupeSettings.tsx` → exports `DedupeCommon` and `DedupeAdvanced` (shared local state for drafts stays per component; the quarantine draft lives in `DedupeCommon`, container-order and exclude drafts in `DedupeAdvanced`)
- Modify: `src/components/settings/LibrarySection.tsx` (remove `<DedupeSettings>`)
- Modify: `src/stores/navStore.ts` (`'duplicates'` after `'organize'`), `src/screens/SettingsScreen.tsx` (`SECTION_COMPONENTS.duplicates`)
- Modify: `src/components/settings/Sections.test.tsx`, `src/screens/SettingsScreen.test.tsx`
- Modify: `locales/{es,en}/ui.json`

Content (spec §5.3). Visible: action radios (with the Trash hint), quarantine folder + «Elegir…» (short hint `quarantineHint`, `more` = `settings.dedupe.quarantineDetails`), one ROM per game (`oneGameOneRomHint` short, `more` = `oneGameOneRomDetails`). Then `<AdvancedFold id="duplicates-advanced" count={…} errorPaths={['dedupe.levels','dedupe.keep_rules','dedupe.container_order','dedupe.exclude_systems']}>` holding, in order: `settings.dedupe.defaultsNote`; levels; rules (hint `rulesHint` short, `more` = `rulesDetails`; `id="setting-dedupe-keep-rules" tabIndex={-1}` on the `<ol>`, SP-1); format order per profile as a plain sub-group (`<div role="group" id="setting-dedupe-container-order" tabIndex={-1} aria-labelledby=…>` with an `<h3 className={sectionTitle}>`, no inner `Collapsible`); systems left out; the pointer line `settings.dedupe.organizeStepWhere` + link button `settings.dedupe.organizeStepLink` → `openSettings('organize', { focus: settingId('dedupe.organize_step') })`. The `count` is the number of registry entries in the fold (Task 9's test checks it; until then use the number of levels + 3).

Keys (both locales; texts from spec §7):

| Key | es | en |
|-----|----|----|
| `settings.section.duplicates` | Duplicados | Duplicates |
| `settings.dedupe.defaultsNote` | La pantalla de duplicados empieza con estas opciones; las de aquí abajo se aplican al volver a buscar. | The duplicates screen starts with these options; the ones below apply when you look again. |
| `settings.dedupe.organizeStepWhere` | Qué hacer al organizar se elige en Organizar. | What happens when organising is set in Organise. |
| `settings.dedupe.organizeStepLink` | Ir a Organizar | Go to Organise |
| `settings.dedupe.quarantineDetails` | No puede estar dentro de la carpeta de ROMs ni de un sistema: la consola listaría las copias. | It can't be inside the ROMs folder or a system folder: the device would list the copies. |
| `settings.dedupe.oneGameOneRomHint` (changed) | Deja una sola versión de cada juego. | Keeps one version of each game. |
| `settings.dedupe.oneGameOneRomDetails` | Aparta también las otras versiones del mismo juego (otra región o revisión). | Also sets aside the other versions of the same game (another region or revision). |
| `settings.dedupe.rulesHint` (changed) | Usa las flechas para ordenarlas. | Use the arrows to reorder them. |
| `settings.dedupe.rulesDetails` | Se miran en este orden; la primera regla que decide, decide. | Weighed in this order; the first rule that decides, decides. |

  Remove `settings.dedupe.title` if `git grep` shows no other user.

- [ ] **Step 0: Sync.** `git fetch origin && git merge origin/main && git push`
- [ ] **Step 1: Migrate the tests first.** `library: the duplicates block shows and saves` → `duplicates: the duplicates tab shows and saves`: `const panel = await open('duplicates')`; click «One ROM per game», «Send to the Trash», check the quarantine placeholder (visible); then open «Advanced options (n)» and toggle «Other versions», edit «Systems to leave out»; the `saved()` assertion keeps `levels`, `one_game_one_rom`, `action`, `exclude_systems` (drop `organize_step`, now in Task 4's test). The region-link assertion moves to Task 7. `library: settings_reorder_rules_announces` → `duplicates: …` (open the fold first; same assertions). `library: settings_rejects_quarantine_inside_a_system_folder` → `duplicates: …` (visible; the description regex `/can't be inside/` now matches the error, not the hint: keep it). New: `duplicates: the fold shows what applies when` (defaultsNote visible after opening), `duplicates: Go to Organise lands on When organising` (click → section `organize`, the select has focus), `library: shows no duplicates control`. Tab order in `SettingsScreen.test.tsx`: `'Duplicates'` after `'Organise'`.
- [ ] **Step 2: Run** `pnpm vitest run src/components/settings src/screens/SettingsScreen.test.tsx` (fails).
- [ ] **Step 3: Implement** the split and `DuplicatesSection`:

```tsx
export function DuplicatesSection() {
  const profiles = useProfiles(); // the shared listProfiles effect, extracted to Field.tsx or a hook file
  return (
    <div className="settings-group">
      <DedupeCommon />
      <AdvancedFold
        id="duplicates-advanced"
        count={DUPLICATES_ADVANCED_COUNT}
        errorPaths={DUPLICATES_ADVANCED_PATHS}
      >
        <DedupeAdvanced profiles={profiles} />
      </AdvancedFold>
    </div>
  );
}
```

  Extract the profile-loading effect used by Library, Organise and Duplicates into `useProfiles()` (in `src/components/settings/useProfiles.ts`) so the three tabs do not repeat it.
- [ ] **Step 4: Run** the same tests (pass), then G-WEB.
- [ ] **Step 5: Commit and push:** `git add src locales && git commit -m "feat(ui): move the duplicates settings to their own tab with advanced options folded" && git push && git status -sb`

## Task 6: Biblioteca tab: scan options and recents only

**Files:**
- Modify: `src/components/settings/LibrarySection.tsx`
- Modify: `src/screens/SettingsScreen.test.tsx`, `src/components/settings/Sections.test.tsx`
- Modify: `locales/{es,en}/ui.json`

Content (spec §5.1). Visible: default layout profile, include hidden files, follow symbolic links (short hint + `more`). Then `<AdvancedFold id="library-advanced" count={2} errorPaths={['library.ignored_dirs','library.ignored_exts']}>`: ignored folders, ignored extensions (short hint + `more`). Then «Bibliotecas recientes».

| Key | es | en |
|-----|----|----|
| `settings.library.followSymlinksHint` (changed) | Las carpetas enlazadas se leen como si estuvieran dentro. | Linked folders are read as if they were inside. |
| `settings.library.followSymlinksDetails` | Nunca se escribe arte fuera de la carpeta de la biblioteca: las ROMs de una carpeta enlazada que apunta a otro sitio se muestran, pero su arte se omite con un error. | Art is never written outside the library folder: ROMs in a linked folder that points elsewhere are listed, but their art is skipped with an error. |
| `settings.library.ignoredExtsHint` (changed) | Separadas por comas, sin punto. | Comma-separated, without dots. |
| `settings.library.ignoredExtsDetails` | Los archivos con estas extensiones nunca se toman como ROMs. | Files with these extensions are never taken as ROMs. |

- [ ] **Step 0: Sync.** `git fetch origin && git merge origin/main && git push`
- [ ] **Step 1: Write/update the tests.** `edits the ignored folders and the default profile`: click «Advanced options (2)» before finding «Ignored folders»; the saved assertions are unchanged. `offers to reopen after a scan setting changes`: unchanged, must pass. New `library: the advanced fold opens on an ignored-folder error` (type a value the mock refuses, e.g. an empty line list rejected by validation — use whatever `library.ignored_dirs` error the mock produces; if it produces none, inject with `useSettingsStore.setState({ errors: { 'library.ignored_dirs': { code: 'invalid' } } })` while the fold is closed, and assert it opens). New `library: follow symlinks has a short hint and more information`.
- [ ] **Step 2: Run** `pnpm vitest run src/screens/SettingsScreen.test.tsx src/components/settings/Sections.test.tsx` (fails).
- [ ] **Step 3: Implement.** Reorder and wrap as above; the panel is ≤ 150 lines after the moves.
- [ ] **Step 4: Run** (passes), then G-WEB.
- [ ] **Step 5: Commit and push:** `git add src locales && git commit -m "feat(ui): keep only the scan options and recent libraries in the library tab" && git push && git status -sb`

## Task 7: Region order shown in Duplicados, edited in Coincidencias (D1)

**Files:**
- Modify: `src/components/settings/DedupeSettings.tsx` (`RegionReadout` under the `region` rule; remove the `regionOrder` link button)
- Modify: `src/components/settings/Sections.test.tsx`
- Modify: `locales/{es,en}/ui.json` (add `regionsNow`, `regionsNone`, `regionsShared`, `regionsChange`; remove `settings.dedupe.regionOrder` after `git grep`)

```tsx
function RegionReadout() {
  const t = useT();
  const regions = useSettingsStore((s) => s.settings!.matching.regions);
  const named = regions.filter((r) => r !== 'rom').map((r) => t(`regions.${r}`));
  return (
    <div className="grid gap-1 pl-1">
      <p className={hintText}>
        {named.length === 0
          ? t('settings.dedupe.regionsNone')
          : t('settings.dedupe.regionsNow', { regions: named.join(', ') })}
      </p>
      <p className={hintText}>{t('settings.dedupe.regionsShared')}</p>
      <Button
        variant="link"
        size="sm"
        className="h-auto justify-self-start p-0"
        onClick={() =>
          useNavStore.getState().openSettings('matching', { focus: 'setting-matching-regions' })
        }
      >
        {t('settings.dedupe.regionsChange')}
      </Button>
    </div>
  );
}
```

- [ ] **Step 0: Sync.** `git fetch origin && git merge origin/main && git push`
- [ ] **Step 1: Write the failing tests:** `duplicates: shows the Matching region order and links to it` (open the fold; text «Regions, in order: World, USA, Europe, Japan» — compute the expected string from `DEFAULT_REGION_ORDER` minus `rom` through the English `regions.*` names; click «Change in Matching» → section `matching`, the region list has focus); `duplicates: only the ROM region says the rule decides nothing` (`backend` seeded with `matching.regions: ['rom']` through `update`, then the `regionsNone` line).
- [ ] **Step 2: Run** `pnpm vitest run src/components/settings/Sections.test.tsx` (fails).
- [ ] **Step 3: Implement** as above.
- [ ] **Step 4: Run** (passes), then G-WEB.
- [ ] **Step 5: Commit and push:** `git add src locales && git commit -m "feat(ui): show the matching region order in the duplicates tab and link to it" && git push && git status -sb`

## Task 8: Grouped tab list and narrow layout

**Files:**
- Modify: `src/stores/navStore.ts` (final order + `SETTINGS_GROUPS`)
- Modify: `src/screens/SettingsScreen.tsx`
- Modify: `src/styles/globals.css`
- Modify: `src/screens/SettingsScreen.test.tsx`
- Modify: `locales/{es,en}/ui.json` (`settings.group.collection` Colección / Collection, `settings.group.covers` Portadas / Covers, `settings.group.app` Aplicación / App)

```ts
export const SETTINGS_SECTIONS: readonly SettingsSection[] = [
  'general',
  'library', 'organize', 'duplicates', 'profiles',
  'art', 'matching', 'sources',
  'network', 'safety', 'advanced',
  'about',
];
export type SettingsGroup = 'collection' | 'covers' | 'app';
/** Visual groups of the tab list (D3); future tabs (collections, devices,
 * saves, RetroAchievements) join 'collection'. */
export const SETTINGS_GROUPS: Readonly<Record<SettingsSection, SettingsGroup | null>> = {
  general: null,
  library: 'collection', organize: 'collection', duplicates: 'collection', profiles: 'collection',
  art: 'covers', matching: 'covers', sources: 'covers',
  network: 'app', safety: 'app', advanced: 'app',
  about: null,
};
```

  Nav rendering: one `TabsList`; before the first tab of each group render `<span aria-hidden id={`settings-group-${g}`} className="settings-nav__group">{t(`settings.group.${g}`)}</span>` (not focusable, not a `role="group"`), and give that first tab `aria-describedby={`settings-group-${g}`}`. Orientation follows the layout:

```ts
const narrow = useMediaQuery('(max-width: 40rem)'); // small hook over matchMedia + 'change'
<Tabs orientation={narrow ? 'horizontal' : 'vertical'} …>
```

  When `section` changes, scroll the selected tab into view (`block: 'nearest', inline: 'nearest'`). CSS: `.settings-nav__group { font: caption, medium weight, --cr-text-secondary; margin: 0.75rem 0.75rem 0.25rem; }`; under `max-width: 40rem` the tablist is `flex-direction: row; flex-wrap: nowrap; overflow-x: auto; overscroll-behavior-x: contain`, group labels `display: none`, and the first tab of each group gets `margin-left: 0.75rem`.

- [ ] **Step 0: Sync.** `git fetch origin && git merge origin/main && git push`
- [ ] **Step 1: Write the failing tests:** tab order is the 12 names above (General first, About last); `ArrowDown` from General reaches Library (the group label is skipped), `ArrowUp` from General wraps to About, `End` → About, `Home` → General; the Library tab's accessible description is «Collection», Organise has none; group labels are `aria-hidden`; no element with `role="group"` inside the tablist. Narrow: `stubLayout(375, 800)` plus a `matchMedia` stub returning `matches: true` for `(max-width: 40rem)` → tablist `aria-orientation="horizontal"` and `ArrowRight` moves.
- [ ] **Step 2: Run** `pnpm vitest run src/screens/SettingsScreen.test.tsx` (fails).
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** (passes), then G-WEB.
- [ ] **Step 5: Commit and push:** `git add src locales && git commit -m "feat(ui): group the settings tabs and scroll them in a row on narrow windows" && git push && git status -sb`

## Task 9: Settings registry and its completeness test

**Files:**
- Create: `src/components/settings/registry.ts`
- Create: `src/components/settings/registry.test.tsx`
- Modify: `locales/{es,en}/ui.json` (`settings.keywords.*`)

```ts
import type { SettingsSection } from '@/stores/navStore';

export interface SettingEntry {
  /** DOM id to focus: settingId(path) of the control, or a list anchor (SP-1). */
  id: string;
  section: SettingsSection;
  /** The AdvancedFold id that holds it, if any. */
  fold?: 'library-advanced' | 'organize-advanced' | 'duplicates-advanced';
  /** i18n keys. */
  label: string;
  hint?: string;
  keywords?: string;
  /** The id is a prefix: dynamic rows (container order per profile/kind). */
  family?: true;
}

export const SETTINGS_INDEX: readonly SettingEntry[] = [
  // Biblioteca
  { id: 'setting-library-default-profile', section: 'library', label: 'settings.library.defaultProfile' },
  { id: 'setting-library-include-hidden', section: 'library', label: 'settings.library.includeHidden', keywords: 'settings.keywords.hidden' },
  { id: 'setting-library-follow-symlinks', section: 'library', label: 'settings.library.followSymlinks', hint: 'settings.library.followSymlinksHint', keywords: 'settings.keywords.symlinks' },
  { id: 'setting-library-ignored-dirs', section: 'library', fold: 'library-advanced', label: 'settings.library.ignoredDirs', hint: 'settings.library.ignoredDirsHint' },
  { id: 'setting-library-ignored-exts', section: 'library', fold: 'library-advanced', label: 'settings.library.ignoredExts', hint: 'settings.library.ignoredExtsHint' },
  // Organizar
  { id: 'setting-organize-mode', section: 'organize', label: 'settings.organize.mode' },
  { id: 'setting-organize-default-layout', section: 'organize', label: 'settings.organize.defaultLayout' },
  { id: 'setting-organize-names', section: 'organize', label: 'settings.organize.names' },
  { id: 'setting-organize-nest-in-roms-root', section: 'organize', label: 'settings.organize.nestInRoms' },
  { id: 'setting-organize-download-dats', section: 'organize', label: 'settings.organize.downloadDats' },
  { id: 'setting-dedupe-organize-step', section: 'organize', label: 'settings.dedupe.organizeStep', keywords: 'settings.keywords.duplicates' },
  { id: 'setting-organize-external-tools', section: 'organize', fold: 'organize-advanced', label: 'settings.organize.externalTools', keywords: 'settings.keywords.rar' },
  // Duplicados
  { id: 'setting-dedupe-action-quarantine', section: 'duplicates', label: 'settings.dedupe.actionQuarantine', keywords: 'settings.keywords.quarantine' },
  { id: 'setting-dedupe-action-trash', section: 'duplicates', label: 'settings.dedupe.actionTrash', hint: 'settings.dedupe.actionTrashHint', keywords: 'settings.keywords.trash' },
  { id: 'setting-dedupe-quarantine-dir', section: 'duplicates', label: 'settings.dedupe.quarantine', keywords: 'settings.keywords.quarantine' },
  { id: 'setting-dedupe-one-game-one-rom', section: 'duplicates', label: 'settings.dedupe.oneGameOneRom', hint: 'settings.dedupe.oneGameOneRomHint' },
  { id: 'setting-dedupe-levels-exact', section: 'duplicates', fold: 'duplicates-advanced', label: 'dedupe.level.exact' },
  // …one per DEDUPE_LEVELS entry
  { id: 'setting-dedupe-keep-rules', section: 'duplicates', fold: 'duplicates-advanced', label: 'settings.dedupe.rules', keywords: 'settings.keywords.region' },
  { id: 'setting-dedupe-container-order', section: 'duplicates', fold: 'duplicates-advanced', label: 'settings.dedupe.containerOrder', family: true },
  { id: 'setting-dedupe-exclude-systems', section: 'duplicates', fold: 'duplicates-advanced', label: 'settings.dedupe.exclude', hint: 'settings.dedupe.excludeHint' },
  // Coincidencias
  { id: 'setting-matching-regions', section: 'matching', label: 'settings.matching.regions', keywords: 'settings.keywords.region' },
  // General, Perfiles, Arte, Coincidencias, Fuentes, Red, Seguridad, Avanzado:
  // one entry per `setting-` control, label = the key that control's label uses.
];

/** Entries inside a fold (the trigger's count). */
export const foldCount = (fold: NonNullable<SettingEntry['fold']>) =>
  SETTINGS_INDEX.filter((e) => e.fold === fold).length;
```

  Use `foldCount(…)` for the `count` of the three `AdvancedFold`s (replacing Task 4–6 literals; the `(n)` in the migrated tests then follows the registry: compute it in the tests with `foldCount` too).

  Keywords (`settings.keywords.<name>`, comma-separated, both locales; spec §7): `rar` (unrar, 7-Zip, 7z, rar), `trash` es «papelera, borrar, eliminar, trash» / en "trash, bin, delete, papelera", `quarantine` (cuarentena, apartados, quarantine), `region` es «región, país, idioma» / en "region, country, language", `symlinks` (symlink, alias, acceso directo / shortcut), `hidden` (dotfiles, punto / dot), `duplicates` (copias, repetidos / copies, duplicates).

- [ ] **Step 0: Sync.** `git fetch origin && git merge origin/main && git push`
- [ ] **Step 1: Write the failing test** `registry.test.tsx`: render `<Root />` with the mock backend, settings seeded so `dedupe.container_order` has at least one row (the mock default already does; assert it), and for each `SETTINGS_SECTIONS` value (except `about`): `act(() => useNavStore.getState().openSettings(s))`, open every fold (`setFold` for the three ids), collect `[id^="setting-"]` whose id does not end in `-description`, `-error` or `-more`. Assert: (a) every collected id is an entry's `id` with `section === s`, or starts with a `family` entry's id of that section; (b) every entry with `section === s` exists in the DOM (`getElementById`); (c) an element inside `[data-advanced-fold=F]` has `fold === F` in its entry, and one outside has no `fold`; (d) every `label`/`hint`/`keywords` key resolves in both locales (`translate(l, key) !== key`). On failure print the missing ids so the remaining entries can be filled from the output.
- [ ] **Step 2: Run** `pnpm vitest run src/components/settings/registry.test.tsx` (fails, listing the other tabs' ids).
- [ ] **Step 3: Fill** the remaining entries from the failure list, taking each label key from the component that renders that control. Re-run until green.
- [ ] **Step 4: Run** G-WEB.
- [ ] **Step 5: Commit and push:** `git add src locales && git commit -m "feat(ui): index every setting for search with a completeness test" && git push && git status -sb`

## Task 10: Search matcher (pure)

**Files:**
- Create: `src/components/settings/search.ts`
- Create: `src/components/settings/search.test.ts`

```ts
import { translate, type Locale } from '@/i18n/core';

import { SETTINGS_INDEX, type SettingEntry } from './registry';

const LOCALES: readonly Locale[] = ['es', 'en'];

/** Accent- and case-insensitive text, placeholders removed (D8). */
export function foldText(s: string): string {
  return s
    .replace(/\{\w+\}/g, ' ')
    .normalize('NFD')
    .replace(/\p{M}+/gu, '')
    .toLowerCase()
    .replace(/\s+/g, ' ')
    .trim();
}

interface Folded { label: string; hint: string; keywords: string[] }
const cache = new Map<Locale, Map<SettingEntry, Folded>>();

function folded(locale: Locale, e: SettingEntry): Folded { /* memoised per locale */ }

function score(f: Folded, q: string, words: string[]): number {
  if (f.label.startsWith(q)) return 40;
  const labelWords = f.label.split(/[^\p{L}\p{N}]+/u);
  if (words.every((w) => labelWords.some((lw) => lw.startsWith(w)))) return 30;
  if (f.label.includes(q)) return 25;
  if (words.every((w) => f.hint.includes(w))) return 20;
  if (words.every((w) => f.keywords.some((k) => k.startsWith(w)))) return 10;
  return 0;
}

/** Settings that match `query`, best first; other locales count a bit less. */
export function searchSettings(
  query: string,
  locale: Locale,
  index: readonly SettingEntry[] = SETTINGS_INDEX,
): SettingEntry[] {
  const q = foldText(query);
  if (!q) return [];
  const words = q.split(' ');
  const order = [locale, ...LOCALES.filter((l) => l !== locale)];
  return index
    .map((e, i) => {
      const best = Math.max(
        ...order.map((l, k) => {
          const s = score(folded(l, e), q, words);
          return s === 0 ? 0 : s - (k === 0 ? 0 : 5);
        }),
      );
      return { e, i, best };
    })
    .filter((x) => x.best > 0)
    .sort((a, b) => b.best - a.best || a.i - b.i)
    .map((x) => x.e);
}
```

- [ ] **Step 0: Sync.** `git fetch origin && git merge origin/main && git push`
- [ ] **Step 1: Write the failing tests:** `foldText('Organización {count}') === 'organizacion'`; «organizacion» finds an entry labelled «Organización»; label prefix beats label word beats hint beats keyword (a four-entry index of hand-made entries using real keys); `searchSettings('', 'es')` is `[]`; `searchSettings('trash', 'es')` returns the Trash entry first (English label, other-locale fallback); `searchSettings('rar', 'es')` returns exactly one entry, `setting-organize-external-tools`; `searchSettings('papelera', 'en')` finds the Trash entry through keywords.
- [ ] **Step 2: Run** `pnpm vitest run src/components/settings/search.test.ts` (fails).
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** (passes), then G-WEB.
- [ ] **Step 5: Commit and push:** `git add src && git commit -m "feat(ui): match settings by label, hint and keywords in any bundled language" && git push && git status -sb`

## Task 11: Search field, results and ⌘F / Ctrl+F

**Files:**
- Create: `src/components/settings/SettingsSearch.tsx`
- Modify: `src/screens/SettingsScreen.tsx`, `src/lib/keyboard.ts` (`isMac()`; `modKeyLabel` uses it)
- Modify: `src/styles/globals.css`, `src/screens/SettingsScreen.test.tsx`
- Modify: `locales/{es,en}/ui.json`: `settings.search.label`, `.placeholder`, `.clear`, `.countOne`, `.count`, `.none`, `.where` (spec §7)

Behaviour (spec D8–D10, §6.3, §6.5):
- `SettingsSearch` sits at the top of `.settings-nav`, full width. `Input` with `role="combobox"`, `aria-label={t('settings.search.label')}`, `aria-expanded={query !== ''}`, `aria-controls="settings-results"`, `aria-autocomplete="list"`, `aria-activedescendant={results.length ? `settings-result-${active}` : undefined}`; leading `Search` icon; `Kbd` hint (`modKeyLabel()` + `F`) when empty; a clear button (`settings.search.clear`) when not.
- The query lives in `SettingsScreen` state. With a query, the active `TabsContent` gets the `hidden` class and a results region replaces it: a polite live line with the count (updated 300 ms after typing stops: `countOne` / `count` with the number in `MonoIn`, or `none` with the query as React text), then `<ul role="listbox" id="settings-results">` of `<li role="option" id="settings-result-i" aria-selected>` items: label on line one, `t(`settings.section.${e.section}`)` or `t('settings.search.where', { section, fold: t('settings.advancedOptions') })` on line two.
- Keys in the field: ↓/↑ move `active` (wrapping), Enter chooses, Escape with text clears (`preventDefault`), Escape when empty focuses the selected tab.
- Choosing: `setQuery('')`, then `useNavStore.getState().openSettings(entry.section, { focus: entry.id })` (keeps `settingsFrom`, Task 1; the arrival effect of Task 3 opens the fold and focuses).
- Shortcut in `SettingsScreen` (bubbling, like `LibraryScreen`):

```ts
useEffect(() => {
  function onKey(ev: KeyboardEvent) {
    if (ev.defaultPrevented || ev.isComposing || ev.altKey) return;
    if (document.querySelector('[role="dialog"][data-state="open"],[role="alertdialog"][data-state="open"]')) return;
    const f = ev.key.toLowerCase() === 'f' || ev.code === 'KeyF';
    const find = f && (isMac() ? ev.metaKey && !ev.ctrlKey : ev.ctrlKey && !ev.metaKey);
    const slash = ev.key === '/' && !ev.metaKey && !ev.ctrlKey && !isTyping(ev.target);
    if (!find && !slash) return;
    ev.preventDefault();
    searchRef.current?.focus();
    searchRef.current?.select();
  }
  window.addEventListener('keydown', onKey);
  return () => window.removeEventListener('keydown', onKey);
}, []);
```

- [ ] **Step 0: Sync.** `git fetch origin && git merge origin/main && git push`
- [ ] **Step 1: Write the failing tests** (`SettingsScreen.test.tsx`; stub `navigator.platform` with `vi.spyOn(navigator, 'platform', 'get')`): `⌘F focuses search on mac`; `Ctrl+F on mac does nothing` (focus stays in a text field and the event is not prevented); `Ctrl+F focuses search on Windows`; `Shift+⌘F still works`; `/ focuses search outside a text field, types a slash inside one`; `typing rar lists one result and Enter lands on the external tools switch` (section `organize`, fold `organize-advanced` open, the switch has focus, the query is empty); `Escape clears, Escape again focuses the selected tab`; `ArrowDown moves the active option` (`aria-activedescendant`); `the count is announced after typing stops` (fake timers, 300 ms; «1 setting»); `no match says so with the query` («No setting matches "zzz".»); `papelera in English finds Send to the Trash`.
- [ ] **Step 2: Run** `pnpm vitest run src/screens/SettingsScreen.test.tsx` (fails).
- [ ] **Step 3: Implement.** CSS: `.settings-search { margin: 0 0.25rem 0.75rem; }`, result items as plain rows (`padding: 0.5rem 0.75rem`, active row `--cr-surface-sunken`), match weight not colour; no card. Narrow: the search stays above the tab row, full width.
- [ ] **Step 4: Run** (passes), then G-WEB.
- [ ] **Step 5: Commit and push:** `git add src locales && git commit -m "feat(ui): search settings with Cmd+F or Ctrl+F and jump to the result" && git push && git status -sb`

## Task 12: Duplicates screen: settings link and «settings changed» alert

**Files:**
- Create: `src/components/settings/SettingsLink.tsx`
- Create: `src/lib/settingsStamp.ts`, `src/lib/settingsStamp.test.ts`
- Modify: `src/stores/dedupeStore.ts` (`settingsStamp: string | null`, initial `null`; `replan()` sets it to `null` first)
- Modify: `src/screens/DuplicatesScreen.tsx` (header link + alert only; nothing in the groups area — coordinate with `feat/dedupe-screen-polish`)
- Create: `src/screens/settingsLinks.test.tsx`
- Modify: `locales/{es,en}/ui.json`: `dedupe.settingsLink` (Ajustes de Duplicados / Duplicates settings), `dedupe.settingsChanged` (spec §7)

```ts
// src/lib/settingsStamp.ts
import type { Settings } from '@/ipc/types';

/** What a duplicates re-plan reads from the settings (spec §8.6). */
export const dedupeStamp = (s: Settings) =>
  JSON.stringify([
    s.dedupe.keep_rules,
    s.dedupe.container_order,
    s.dedupe.exclude_systems,
    s.dedupe.quarantine_name,
    s.matching.regions,
  ]);

/** What an organise re-scan falls back to (spec §8.6). */
export const organizeStamp = (s: Settings) =>
  JSON.stringify([s.organize, s.dedupe.organize_step]);
```

```tsx
// src/components/settings/SettingsLink.tsx
export function SettingsLink({ section, label, disabled, onOpen }: {
  section: SettingsSection;
  label: string;
  disabled?: boolean;
  /** Runs before Settings opens (the screen stamps its settings). */
  onOpen?: () => void;
}) {
  return (
    <Button
      size="sm"
      variant="ghost"
      className="ml-auto"
      data-settings-opener
      disabled={disabled ?? false}
      onClick={() => {
        onOpen?.();
        useNavStore.getState().openSettings(section);
      }}
    >
      <Settings aria-hidden />
      {label}
    </Button>
  );
}
```

  In `DuplicatesScreen` header, after the `<h1>`: `<SettingsLink section="duplicates" label={t('dedupe.settingsLink')} disabled={applying} onOpen={() => { const s = useSettingsStore.getState().settings; if (s) useDedupeStore.setState({ settingsStamp: dedupeStamp(s) }); }} />`. In the alert area (with the other top alerts), when `canReplan && stamp !== null && settings && dedupeStamp(settings) !== stamp`: `<Alert variant="info" action={<Button size="sm" variant="primary" onClick={replan}>{t('dedupe.replan')}</Button>}>{t('dedupe.settingsChanged')}</Alert>`. `back()` already calls `reset()`, which clears the stamp.

- [ ] **Step 0: Sync.** `git fetch origin && git merge origin/main && git push` (expect conflicts in `DuplicatesScreen.tsx` from the polish branch; keep both).
- [ ] **Step 1: Write the failing tests.** `settingsStamp.test.ts`: the stamp changes with `keep_rules` and `matching.regions`, not with `dedupe.action`, `levels`, `one_game_one_rom` or `quarantine_dir`. `settingsLinks.test.tsx` (render `<Root />`, mock backend, open the mock library, `openDuplicates(root)`, wait for the plan): `opens the duplicates tab and comes back with the plan` (click «Duplicates settings» → tab «Duplicates» selected; Back → the Duplicates screen, `useDedupeStore.getState().plan` is the same object, the link has focus); `a changed keeper rule shows the alert and Look again re-plans` (move a rule in the fold, Back, alert visible; click «Look again» → status `planning`, alert gone); `a changed default action shows no alert`; `the link is disabled while applying` (`useDedupeStore.setState({ status: 'applying' })`).
- [ ] **Step 2: Run** `pnpm vitest run src/lib/settingsStamp.test.ts src/screens/settingsLinks.test.tsx` (fails).
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** (passes), then `pnpm vitest run src/screens/DuplicatesScreen.test.tsx` (unchanged tests still pass), then G-WEB.
- [ ] **Step 5: Commit and push:** `git add src locales && git commit -m "feat(ui): link the duplicates screen to its settings and offer to look again after a change" && git push && git status -sb`

## Task 13: Organise screen: settings link and «settings changed» alert

**Files:**
- Modify: `src/stores/organizeStore.ts` (`settingsStamp`, initial `null`; `refresh()` clears it)
- Modify: `src/screens/OrganizeScreen.tsx` (header link + alert)
- Modify: `src/screens/settingsLinks.test.tsx`
- Modify: `locales/{es,en}/ui.json`: `organize.settingsLink` (Ajustes de Organizar / Organise settings), `organize.settingsChanged` (spec §7)

  Same shape as Task 12: `<SettingsLink section="organize" label={t('organize.settingsLink')} disabled={status === 'applying'} onOpen={…organizeStamp…} />` in the header; the alert's button is the existing `refresh` with `t('organize.refresh.button')`, shown when the stamp differs and `status === 'ready'`.

- [ ] **Step 0: Sync.** `git fetch origin && git merge origin/main && git push`
- [ ] **Step 1: Write the failing tests** in `settingsLinks.test.tsx` (`openOrganize(MOCK_DOWNLOADS)`, wait for the plan, make one decision): `opens the organise tab and comes back with the decisions` (Back → Organise screen, the decision is still in `useOrganizeStore`, the link has focus); `a changed names setting shows the alert and Scan again keeps the decisions`; `the link is disabled while applying`.
- [ ] **Step 2: Run** `pnpm vitest run src/screens/settingsLinks.test.tsx` (fails).
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** (passes), then `pnpm vitest run src/screens/OrganizeScreen.test.tsx`, then G-WEB.
- [ ] **Step 5: Commit and push:** `git add src locales && git commit -m "feat(ui): link the organise screen to its settings and offer to scan again after a change" && git push && git status -sb`

## Task 14: Docs and changelog

**Files:**
- Modify: `README.md` (line ~139 tab list: General, Library, Organise, Duplicates, Profiles, Art, Matching, Sources, Network, Safety, Advanced, About; any «Settings → Library» for organise/duplicates), `README.es.md` (same in Spanish)
- Modify: `docs/04-architecture/desktop-app.md` (§Duplicates wording, `navStore` row: `settingsFrom`, `openFolds`, `focusSetting`; Settings paragraph: groups, search, folds, screen links)
- Modify: `docs/04-architecture/settings.md` («Duplicates block» → «Duplicates tab»; a short note that the UI is grouped and the file is unchanged)
- Modify: `CHANGELOG.md` (`[Unreleased]` › Changed: «Settings: Organise and Duplicates have their own tabs; advanced options are folded; search with ⌘F / Ctrl+F; the Duplicates and Organise screens link to their settings.»)

- [ ] **Step 0: Sync.** `git fetch origin && git merge origin/main && git push`
- [ ] **Step 1:** `git grep -n "Settings → Library\|Ajustes → Biblioteca\|Configuración › Biblioteca" -- README* docs CHANGELOG.md` and fix each hit that refers to organise or duplicates options (leave `docs/TRACKER.md` and `TODO.md` alone).
- [ ] **Step 2: Run** `pnpm format:check` (Prettier covers Markdown) and G-WEB.
- [ ] **Step 3: Commit and push:** `git add README.md README.es.md docs/04-architecture CHANGELOG.md && git commit -m "docs: describe the new settings tabs, search and screen links" && git push && git status -sb`

## Task 15: Verification and E2E

**Files:** none expected; any fix found here gets its own `fix(ui): …` commit (with a test) before this task finishes.

- [ ] **Step 0: Sync.** `git fetch origin && git merge origin/main && git push`
- [ ] **Step 1: Full gates.** G-WEB and G-RUST, all green. `git diff origin/main --stat -- crates src-tauri src/ipc/types.ts src/ipc/mock` must show no settings schema/default change (paste the empty output into the PR evidence).
- [ ] **Step 2: Start the mock UI.** `pnpm dev --port 1464 --strictPort` (background). Chrome DevTools MCP `new_page` with `isolatedContext: "settings-ui"` at `http://localhost:1464/` (fallbacks per Global Constraints).
- [ ] **Step 3: E2E checklist** (spec §10), screenshots of each step saved under the scratchpad, not the repo:
  1. Open Settings from the start screen; Tab into the nav; walk all 12 tabs with ↓ checking each panel; End/Home. Run a Lighthouse accessibility audit (or axe via `evaluate_script`) of the nav: no `aria-required-children` / `aria-required-parent` errors.
  2. Library shows only default profile, hidden files, symlinks (+ «Más información»), «Opciones avanzadas (2)», recents. Open the fold, edit ignored extensions, reload: value kept and fold closed again (session state, not persisted).
  3. ⌘F, type «papelera», Enter: Duplicados tab, focus on the Trash radio, manila mark visible, gone after ~1.2 s.
  4. Open the mock library, «Buscar duplicados», wait for the plan, «Ajustes de Duplicados», move a keeper rule, Back: Duplicates screen with the same plan, the changed alert, focus on the link; «Volver a buscar» re-plans. Repeat changing only the default action: no alert.
  5. `?mockPick=downloads`: organise the pile, decide one item, «Ajustes de Organizar», change Nombres, Back: alert; «Volver a analizar» keeps the decision.
  6. Narrow (375×800, `resize_page`): nav row scrolls inside itself, ←/→ move tabs, no horizontal page scroll (`document.documentElement.scrollWidth <= innerWidth`), search works.
  7. `emulate` reduced motion: fold, scroll and mark without animation. Forced colours: mark visible as an outline.
  8. Spanish and English, light and dark: screenshots of Biblioteca, Organizar, Duplicados (fold open and closed) and a search result list. Critique them against `docs/08-brand/brand.md` and spec §6.2; fix anything off.
- [ ] **Step 4: Stop the server by port:** `lsof -ti tcp:1464 -sTCP:LISTEN | xargs -r kill`.
- [ ] **Step 5: Windows note.** WebView2's own Ctrl+F find bar must be suppressed by `preventDefault()`; this cannot be checked on the mock in Chrome. Record it as a manual check for the owner on the next Windows build in the PR description.
- [ ] **Step 6: Final state.** `git status -sb` shows `## feat/settings-tabs...origin/feat/settings-tabs` with nothing ahead and a clean tree.
