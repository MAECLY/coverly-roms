# Settings tabs: Library, Organise and Duplicates

Status: design spec, 2026-10-26, revised after an adversarial review the
same day (§12). Branch `feat/settings-tabs`. Roadmap item
**1.6. Reordenar los ajustes de Biblioteca** (`TODO.md`).

This is a presentation change. The settings file (`settings.json`), its
schema, its keys, its defaults and its validation stay exactly as they are.
No Rust code changes. Front-end state that is never persisted changes:
`src/stores/navStore.ts` (two tab ids, the screen Settings was opened
from, open folds, the setting to focus) and a settings stamp in
`dedupeStore` / `organizeStore` (D12). Nothing new is written to
`settings.json`, `localStorage` or any other storage.

---

## 1. Goals

1. Settings › Library holds only what belongs there: how a library is
   scanned and the list of recent libraries.
2. Organise and Duplicates each get their own tab, with the common options
   at the top and the advanced ones folded.
3. Each option in these three tabs has one short help line. Longer
   explanations sit behind a «Más información» toggle.
4. People can find any setting by typing (⌘F on macOS, Ctrl+F on Windows
   and Linux, inside Settings).
5. The Duplicates and Organise screens link straight to their settings tab
   and come back to where they were, with their plan intact.
6. The tab list has room for the roadmap's future tabs (collections,
   devices, saves, RetroAchievements) without building any of them.

Non-goals: new settings, changes to defaults, a new settings file format,
any change to the Duplicates screen's own layout (that is
`feat/dedupe-screen-polish`), rewriting the help text of the other tabs
(Arte, Coincidencias, Fuentes…; a follow-up if the owner wants it).

## 2. The owner's asks (from `TODO.md` §1.6, verbatim)

> Configuración › Biblioteca (`src/components/settings/LibrarySection.tsx` +
> `DedupeSettings.tsx`, unas 770 líneas) mezcla tres cosas distintas en una
> sola pestaña y se hace larga:
>
> - **Escaneo**: perfil por defecto, seguir enlaces simbólicos, archivos
>   ocultos, carpetas y extensiones ignoradas, bibliotecas recientes.
> - **Organizar**: modo (mover / copiar / automático), disposición por
>   defecto, nombres (catálogo / conservar), anidar en `ROMS/`, programas
>   externos para RAR, descargar catálogos.
> - **Duplicados**: acción (cuarentena / Papelera), carpeta de cuarentena,
>   niveles, una ROM por juego, reglas para elegir la copia que se queda y
>   su orden, orden de regiones, orden de contenedores por sistema, sistemas
>   excluidos, paso dentro de Organizar.
>
> Tareas:
>
> - Separar en pestañas propias: **Biblioteca** (solo escaneo y bibliotecas
>   recientes), **Organizar** y **Duplicados**. Valorar si «Coincidencias»
>   (regiones) absorbe el orden de regiones de duplicados para no tenerlo
>   dos veces.
> - Dentro de cada pestaña, lo habitual arriba y lo avanzado plegado
>   («Opciones avanzadas»): por ejemplo, en Duplicados dejar a la vista la
>   acción y la carpeta, y plegar niveles, reglas, contenedores y
>   exclusiones.
> - Revisar textos de ayuda: uno corto por opción y el detalle en un «Más
>   información», en vez de párrafos largos.
> - Buscador de ajustes (⌘/Ctrl+F dentro de Configuración) y enlaces
>   directos desde cada pantalla («Ajustes de Duplicados» desde la pantalla
>   de duplicados, «Ajustes de Organizar» desde Organizar).
> - Preparar el sitio para lo que viene del roadmap (Biblioteca con
>   colecciones, Dispositivos, Partidas, RetroAchievements): cada uno con su
>   pestaña, no apilado en Biblioteca.
> - Mantener los tests de `Sections.test.tsx` y la paridad i18n; E2E de la
>   navegación por pestañas con teclado.

Each ask maps to: tabs → D2, D3, §5; folds → D4–D6, §5; help → D7, §7;
search → D8–D10; screen links → D11–D13; future tabs → D3; tests → §10.

## 3. Decisions

Open questions and the default chosen for each.

| # | Question | Decision | Why |
|---|----------|----------|-----|
| D1 | Does Matching absorb the duplicates region order? | **Yes, it already does.** `dedupe` has no region list: the `region` keeper rule reads `settings.matching.regions` through `region_order()` (`crates/coverly-core/src/dedupe/keeper.rs`), which drops the `rom` entry and anything that is not a region. The Duplicados tab shows that order read-only, with the same translated names Coincidencias uses (`regions.<id>`), e.g. «Mundo, EE. UU., Europa, Japón», plus «Cambiar en Coincidencias», which switches tab and focuses the region list. A second line says the same order also picks the art («También decide qué arte se elige.»). | One source of truth, already true in the data. The readout replaces today's «Usa el orden de Coincidencias», which says where the order lives but not what it is. The shared-effect line stops a user from reordering regions for duplicates without knowing the art fill changes too. |
| D2 | Tab ids | Add `organize` and `duplicates` to `SettingsSection`. `library` stays and keeps its meaning (scan + recents). | Existing calls (`openSettings('library')` in tests, `openSettings('about')` from `UpdateBanner`) still land on a valid tab. `organize` matches the existing `settings.organize.*` copy; `duplicates` matches `Screen = 'duplicates'`. |
| D3 | Tab order and grouping | Today there are **ten** flat tabs (`general, library, art, matching, sources, network, safety, advanced, profiles, about`); this makes twelve. New order: **General** first and ungrouped; **Colección** (Biblioteca, Organizar, Duplicados, Perfiles); **Portadas** (Arte, Coincidencias, Fuentes); **Aplicación** (Red, Seguridad, Avanzado); **Acerca de** last and ungrouped. `SETTINGS_SECTIONS` keeps being the single ordered list (arrow-key order); a new `SETTINGS_GROUPS` maps each section to its group or none. | General (theme, language) is what most people open Settings for and stays the first tab and the first one shown, so the start of the list does not move for installed users. Future tabs (Colecciones, Dispositivos, Partidas, RetroAchievements) slot into Colección; sixteen tabs still fit at 12rem. |
| D3a | Which tab opens | Unchanged: `openSettings()` with no section reopens the last tab of this session (`section ?? s.section`); `general` the first time. | Existing behaviour and tests. (The first draft said "default `general`, unchanged"; it is "last tab, `general` at start".) |
| D4 | Fold label | «Opciones avanzadas» / "Advanced options", as the owner wrote. | The owner's words. The «Avanzado» tab (settings file) is a different thing; its place under Aplicación keeps them apart. |
| D5 | Fold state | Closed by default; stays open for the session once opened (kept in `navStore.openFolds: string[]`, not in `settings.json` and not in browser storage). Search and deep links open the fold that holds their target. A fold whose fields have an error (`errors[path]`) opens itself when the error appears; if the user closes it again, the trigger shows a danger dot and its accessible name ends in «, con errores». The fold content is rendered with `forceMount` (Radix then sets `hidden` while closed), so a draft typed in a folded field and its DOM id survive closing the fold. | Never hide a field the user is fixing or looking for. Without `forceMount`, Radix unmounts closed content and a refused draft (the `ListSetting` / `NumberSetting` local state) would be lost. Not persisting keeps `settings.json` unchanged. |
| D6 | What is common vs advanced | See §5. Rule: an option is common if a first-time user changes it in their first week, or if it decides whether the app goes online or deletes files; lists, orders and exclusions are advanced. | The owner's own example for Duplicados follows this rule. The network/delete clause keeps «Descargar catálogos» and the Trash choice in view. |
| D7 | «Más información» | A text toggle after the short hint that reveals one paragraph below it (`aria-expanded`, `aria-controls`). It sits after the `FieldDescription`, not inside it, so the control's `aria-describedby` reads only the short hint. Not a tooltip, not a dialog. | Tooltips are hover-only and cut off long text; inline keeps reading order and works with a screen reader. |
| D8 | Search scope | A static registry of every setting (§4). Matches label, short hint and keywords, accent- and case-insensitive, in the current UI language plus every other bundled locale as a fallback (`translate(locale, key)` from `src/i18n/core.ts`; both catalogues are already bundled). Placeholders (`{folder}`) are stripped before matching. Results are settings, shown in the current language. | A DOM crawl would miss inactive tabs. A registry is testable and fast (about 45 entries). |
| D9 | ⌘F / Ctrl+F | Inside Settings it focuses (and selects) the search field. On macOS only ⌘F counts; on Windows and Linux only Ctrl+F (macOS Ctrl+F is "forward one character" inside text fields and must keep working). Shift and Caps Lock do not matter (`ev.key.toLowerCase() === 'f'`, or `ev.code === 'KeyF'` for non-Latin layouts); Alt/AltGr and `ev.isComposing` (IME) are ignored. `/` does the same when focus is not in a text field (`isTyping`), like the library screen. Escape in the field with text clears it; Escape in an empty field returns focus to the active tab. | Same pattern as `LibraryScreen.tsx`, minus its macOS Ctrl+F clash. |
| D10 | Choosing a result | Clears the query, switches tab, opens the fold that holds it, scrolls it into view, moves focus to its control and marks it for 1.2 s (§6.4). | The user asked "where is X"; the answer is X with focus on it. |
| D11 | Deep-link API | `openSettings(section?)` keeps its signature. It records `settingsFrom` (the screen it was called from) unless the current screen is already `settings`; `closeSettings()` returns there and clears it. One optional export for focus targets: `openSettings(section, { focus: settingId })`. Screens call `useNavStore.getState().openSettings('duplicates')`. | Small surface for the parallel Duplicates screen workflow: one call they already know. Not overwriting `settingsFrom` from inside Settings avoids a Back that returns to Settings. |
| D12 | Coming back to Duplicates / Organise after a settings change | See §8.6: only settings the next plan actually reads count. If one changed, the screen shows an info alert with the screen's existing re-plan action (Duplicates: `dedupe.replan` «Volver a buscar»; Organise: `organize.refresh.button` «Volver a analizar»). No automatic re-plan. | A scan can take minutes on a large library; the user decides. Reusing the existing buttons avoids a second name for the same action. |
| D13 | Deep link while busy | «Ajustes de Duplicados» / «Ajustes de Organizar» is disabled while applying, like the Back button (`status === 'applying'`). | Leaving mid-apply would unmount the progress view. |
| D14 | Narrow windows (≤ 40rem) | Nav becomes one horizontal row that scrolls inside itself (not the page); group labels hide, groups keep a wider gap. The selected tab is scrolled into view. The Radix `Tabs` `orientation` follows the layout (`matchMedia('(max-width: 40rem)')`), so ←/→ work in the row and ↑/↓ in the column. Search stays at the top, full width. | The current narrow layout wraps the tabs into several lines, and a vertical tablist shown as a row only answers ↑/↓. |
| D15 | Where «Al organizar» (`dedupe.organize_step`) lives | On Organizar (visible, §5.2), with a one-line pointer and link from Duplicados. One control, one place. | It changes what an Organise run does; the brief lists it under Duplicados, so Duplicados keeps the pointer and search finds it under both words. |

## 4. Architecture and files

```
src/stores/navStore.ts                 + 'organize' | 'duplicates' sections,
                                         SETTINGS_GROUPS, settingsFrom,
                                         openFolds, focusSetting,
                                         resetNavStore() for tests
src/screens/SettingsScreen.tsx         grouped nav, search field, ⌘F/`/`,
                                         result list, focus-on-arrival
src/components/settings/
  LibrarySection.tsx                   scan options + recent libraries only
  OrganizeSection.tsx       (new)      moved from LibrarySection
  DuplicatesSection.tsx     (new)      wraps the split DedupeSettings
  DedupeSettings.tsx                   split into common + advanced parts;
                                         region readout + link (D1)
  ListSetting.tsx           (new)      moved out of LibrarySection
  Field.tsx                            + MoreInfo, + AdvancedFold
  registry.ts               (new)      SETTINGS_INDEX: id → tab, fold, keys
  search.ts                 (new)      normalise + match + rank (pure)
  SettingsSearch.tsx        (new)      combobox field + listbox of results
  SettingsLink.tsx          (new)      «Ajustes de …» button for screens
src/stores/dedupeStore.ts              + settingsStamp (D12)
src/stores/organizeStore.ts            + settingsStamp (D12)
src/screens/DuplicatesScreen.tsx       + SettingsLink in the header + alert
src/screens/OrganizeScreen.tsx         + SettingsLink in the header + alert
src/App.tsx                            focus restore for any settingsFrom
src/styles/globals.css                 .settings-nav groups, search, mark
locales/{es,en}/ui.json                new keys (§7); old keys kept where
                                         reused, removed where unused
README.md, README.es.md,               tab list and «Settings → Library»
docs/04-architecture/desktop-app.md,     wording (README line 139;
docs/04-architecture/settings.md,        desktop-app.md §Duplicates,
CHANGELOG.md (Unreleased › Changed)      navStore row, Settings paragraph;
                                         settings.md «Duplicates block»)
```

`SECTION_COMPONENTS` gains `organize: OrganizeSection` and
`duplicates: DuplicatesSection`. It is already
`Record<SettingsSection, …>`, so a new tab without a panel fails to compile.

`registry.ts` entry:

```ts
interface SettingEntry {
  id: string;          // the DOM id to focus: settingId(path) of the control
  section: SettingsSection;
  fold?: string;       // the AdvancedFold id that holds it, if any
  label: string;       // i18n key
  hint?: string;       // i18n key (the short one)
  keywords?: string;   // i18n key with extra search words, comma-separated
}
```

What the review found about ids (`settingId` replaces every run of
non-alphanumerics with `-`, so `dedupe.keep_rules` → `setting-dedupe-keep-rules`):

- `settingId(path)` is the control's own id (`FieldControl` puts it on the
  `<input>` / `<select>` / `<button role=switch>`), so focusing `#id` focuses
  the control. The `Field` primitive also creates `${id}-description` and
  `${id}-error`; those are not settings.
- Families are dynamic: `dedupe.levels.<level>`, the two action radios
  `dedupe.action.<quarantine|trash>`, one container-order row per
  `profile/kind` present in the user's settings (a `*` profile becomes
  `setting-dedupe-container-order---<kind>`). The registry has one entry per
  family pointing at a stable anchor: the levels group, the checked radio
  (Radix roving focus lands there anyway), the container-order group.
- Some targets have no `setting-` id today: the keeper rule switches
  (`aria-label` only), the rules list, and the Matching region list
  (`#settings-regions` heading). They get anchors —
  `setting-dedupe-keep-rules` on the `<ol>` (`tabIndex={-1}`) and focus on
  the first region row's control for `matching.regions` — without changing
  any `name` attribute the tests query.

The data path does not change: each control still calls
`useSettingsStore.update(patch)` with the same patch shapes as today. Moving
a control between components does not change its `settingId`, its `name`
or its patch, so saved values, field errors (`errors[path]`) and tests that
query by name keep working.

## 5. Content of each tab

No per-tab `<h2>` is added: as today, the tab panel is named by its tab.
The `<legend>`s «Organizar» and «Duplicados» go (they would repeat the tab
name); inner sub-groups keep `sectionTitle` headings.

### 5.1 Biblioteca (library)

Visible: default layout profile, include hidden files, follow symbolic
links (short hint + «Más información»). Then «Bibliotecas recientes» with
«Olvidar».

Advanced fold: ignored folders, ignored extensions.

The existing «Reabre la biblioteca para aplicar estos cambios» alert keeps
firing for these fields, folded or not.

### 5.2 Organizar (organize)

Visible: mode, default layout, names, put system folders inside the ROMs
folder, download game catalogues (it goes online, D6), and «Al organizar»
(`dedupe.organize_step`, D15).

Advanced fold: external RAR programs (with the found-tools readout, paths
in mono), and the existing pointer to the settings file for thresholds and
lists (`settings.organize.advancedHint`, unchanged).

### 5.3 Duplicados (duplicates)

Visible: what to do with the copies (set aside / Trash, with the Trash
hint), the set-aside folder with «Elegir…» (short hint + «Más
información»), one ROM per game (short hint + «Más información»).

Advanced fold: what to look for (levels), which copy stays (rules and
order, with the region readout and link, D1), format order per profile
(already a collapsible today; inside the fold it becomes a plain sub-group
so there is never a fold inside a fold), systems left out, and the
one-line pointer to «Al organizar» on Organizar.

These are the defaults of the Duplicates screen, which copies `levels`,
`one_game_one_rom`, `action` and `quarantine_dir` into its own options when
it starts; the advanced fold's rules, format order, exclusions and the
region order are read by the plan itself. The fold's intro line says so in
one sentence (`settings.dedupe.defaultsNote`), because it decides what a
«Volver a buscar» picks up (§8.6).

### 5.4 Other tabs

Untouched apart from moving into groups. Their hints are not rewritten in
this branch (non-goal, §1).

## 6. UX and UI

### 6.1 Design plan

Coverly ROMs is a collector's shelf. Settings is the back office of that
shelf: quiet, dense, legible. The one memorable thing on this screen is the
**arrival mark**: when search or a deep link takes you to a setting, its
row is briefly tabbed with the manila "applied" colour, the same colour the
library uses for art applied this run. Everything else stays still.

- **Colour.** No new tokens. Nav at rest `--cr-text-secondary`; the
  selected tab keeps today's vertical style (`--cr-surface-sunken` fill and
  the 3px Buckram cap from `TabsTrigger`); group labels
  `--cr-text-secondary`. Arrival mark `--cr-manila` as a 3px leading bar
  plus `--cr-manila` at low alpha behind the row (check text contrast on
  the tint in both themes). Under `forced-colors: active` the mark is a
  `Highlight` outline instead of a tint. Search match highlight is weight,
  not colour.
- **Type.** Archivo only for the screen title (as today); Atkinson
  Hyperlegible Next for labels, hints, nav and group labels (group labels
  in `text-caption`, sentence case, medium weight; never all caps);
  Atkinson Hyperlegible Mono only for values that are data (paths, system
  ids, extension lists, result counts).
- **Layout.** Left-aligned throughout. Side nav 12rem; panel content max
  40rem (unchanged).

```
┌ ← Atrás   Configuración ─────────────────────────────────────────┐
├──────────────┬───────────────────────────────────────────────────┤
│ [Buscar… ⌘F] │ Qué hacer con las copias                          │
│              │  (•) Apartar en una carpeta                       │
│  General     │  ( ) Enviar a la Papelera                         │
│              │ Carpeta de apartados                              │
│ Colección    │  [<biblioteca>/_Duplicados        ] [Elegir…]     │
│  Biblioteca  │  Vacía: _Duplicados dentro de la biblioteca.      │
│  Organizar   │  Más información                                  │
│ ▌Duplicados  │ [x] Una ROM por juego                             │
│  Perfiles    │                                                   │
│              │ ▸ Opciones avanzadas (5)                          │
│ Portadas     │                                                   │
│  Arte …      │                                                   │
│ Aplicación   │                                                   │
│  Red …       │                                                   │
│  Acerca de   │                                                   │
└──────────────┴───────────────────────────────────────────────────┘
```

Search with a query replaces the panel with results until cleared (the
tab panel is `hidden` meanwhile; the selected tab does not change):

```
│ [rar        ×] │ 1 ajuste                                        │
│                │ Programas externos para RAR                     │
│                │ Organizar › Opciones avanzadas                  │
```

- **Principles.** (1) A tab answers one question: how my library is read,
  how it is organised, what happens to copies. (2) Defaults on top, lists
  and orders folded. (3) Arriving somewhere is shown once, then gets out of
  the way.

### 6.2 Review against generic defaults

- First draft put each group in a card with a shadow. That is the SaaS
  card kit; the panel already sits on `--cr-surface`. Dropped: groups are
  plain headings with spacing.
- First draft used ALL-CAPS tracked group labels in the nav. Generic
  template chrome. Changed to sentence case caption weight.
- First draft added an icon to every tab. The app's icons label systems,
  not navigation (system icons spec §0). Dropped.
- Breadcrumb in results was `Organizar · Opciones avanzadas` with middle
  dots; changed to `›`, which reads as a path.
- Arrival mark first used `--cr-primary` (a focus ring look-alike, which
  would read as a second focus). Manila is the brand's "this just
  happened" colour; kept.

### 6.3 Components

- `AdvancedFold`: the existing `Collapsible` with a ghost button,
  chevron rotating 90° (`duration-fast`, `ease-standard`, none under
  `prefers-reduced-motion`), content `forceMount` (D5). Label «Opciones
  avanzadas» plus a mono count of the registry entries inside: «Opciones
  avanzadas (5)».
- `MoreInfo`: `Button variant="link" size="sm"` «Más información» /
  «Menos información» after the description; the paragraph uses `hintText`.
- `SettingsSearch`: `Input` with `role="combobox"`, `aria-expanded`,
  `aria-controls` the results `role="listbox"`, `aria-autocomplete="list"`
  and `aria-activedescendant` (ARIA 1.2 combobox). Leading search icon;
  `Kbd` hint (`modKeyLabel()` + F) when empty; clear button when not.
  ↑/↓ move, Enter opens, Escape per D9.
- `SettingsLink`: `Button size="sm" variant="ghost"` with the `Settings`
  lucide icon and `data-settings-opener`, placed in the screen header
  (`.settings-header`, after the title) on both screens. The header is
  always rendered (planning, ready, error), so focus can always return to
  it, and it is outside the groups area the parallel polish branch works
  on.

### 6.4 Motion

Only answers to an action:

- Fold open/close: the existing `collapsible-down/up` animation
  (`motion-reduce:animate-none` already).
- Arrival mark: the bar and tint appear at once and fade out over
  `--cr-duration-slower` after 1.2 s. With reduced motion they appear and
  disappear without a fade. Nothing moves position.
- Scroll into view: `block: 'center'`, `behavior: 'smooth'` unless
  `prefers-reduced-motion: reduce`.

### 6.5 Accessibility

- Nav stays one `tablist` (Radix tabs, arrow keys, Home/End across groups).
  ARIA only allows `tab` children in a `tablist`, so groups are **not**
  `role="group"` inside it. Group labels are `aria-hidden` visual
  separators (plain elements, skipped by roving focus); the first tab of
  each group gets `aria-describedby` its group label, so a screen reader
  hears «Biblioteca, pestaña, Colección» once per group, not on every tab.
  The E2E runs an axe-style check of the nav (§10).
- The results count is a polite live region («3 ajustes», «Ningún ajuste»),
  updated 300 ms after typing stops, not on every key.
- Arrival moves focus to the control itself (the `Switch`, `Input`,
  `select`, or the checked radio), so screen readers announce it.
  `SettingsScreen` today focuses its `<h1>` on mount; that effect is
  skipped when a `focusSetting` is pending, otherwise the parent's effect
  (which runs after the panel's) would steal arrival focus.
- Back from Settings: `App` restores focus to `[data-settings-opener]` on
  whichever screen `settingsFrom` names (today only `main`). `App`'s effect
  runs after the screen's own mount effects, so it wins over a screen that
  focuses its title.
- Folds keep `aria-expanded`; a fold opened by search is announced by the
  focus landing inside it.

## 7. Copy

Spanish: sentence case, no exclamation marks, exact counts, numbers in
mono. Tab names keep their capital when named inside a sentence, as the
existing copy does («se editan en Avanzado», «Ajustes → Fuentes»). Both
locales get every key (parity test). English uses the catalogue's existing
British spelling (`Organise`). Counts follow the catalogue's `xOne` / `x`
convention (no plural engine); the number inside a count is rendered in
the mono face by splitting the translated line around a marker, as
`ExternalToolsHint` already does.

| Key | es | en |
|-----|----|----|
| `settings.section.organize` | Organizar | Organise |
| `settings.section.duplicates` | Duplicados | Duplicates |
| `settings.group.collection` | Colección | Collection |
| `settings.group.covers` | Portadas | Covers |
| `settings.group.app` | Aplicación | App |
| `settings.advancedOptions` | Opciones avanzadas ({count}) | Advanced options ({count}) |
| `settings.advancedHasErrors` | {label}, con errores | {label}, with errors |
| `settings.moreInfo` | Más información | More information |
| `settings.lessInfo` | Menos información | Less information |
| `settings.search.label` | Buscar en la configuración | Search settings |
| `settings.search.placeholder` | Buscar… | Search… |
| `settings.search.clear` | Borrar la búsqueda | Clear search |
| `settings.search.countOne` | 1 ajuste | 1 setting |
| `settings.search.count` | {count} ajustes | {count} settings |
| `settings.search.none` | Ningún ajuste coincide con «{query}». | No setting matches "{query}". |
| `settings.search.where` | {section} › {fold} | {section} › {fold} |
| `settings.dedupe.regionsNow` | Regiones, en orden: {regions} | Regions, in order: {regions} |
| `settings.dedupe.regionsNone` | Solo la región de la ROM: esta regla no decide nada. | Only the ROM's region: this rule decides nothing. |
| `settings.dedupe.regionsShared` | También decide qué arte se elige. | It also decides which art is chosen. |
| `settings.dedupe.regionsChange` | Cambiar en Coincidencias | Change in Matching |
| `settings.dedupe.defaultsNote` | La pantalla de duplicados empieza con estas opciones; las de aquí abajo se aplican al volver a buscar. | The duplicates screen starts with these options; the ones below apply when you look again. |
| `settings.dedupe.organizeStepWhere` | Qué hacer al organizar se elige en Organizar. | What happens when organising is set in Organise. |
| `settings.dedupe.organizeStepLink` | Ir a Organizar | Go to Organise |
| `dedupe.settingsLink` | Ajustes de Duplicados | Duplicates settings |
| `organize.settingsLink` | Ajustes de Organizar | Organise settings |
| `dedupe.settingsChanged` | Los ajustes de duplicados han cambiado. Vuelve a buscar para aplicarlos. | The duplicates settings changed. Look again to use them. |
| `organize.settingsChanged` | Los ajustes de organizar han cambiado. Vuelve a analizar para aplicarlos a lo que no hayas elegido aquí. | The organise settings changed. Scan again to use them for what you have not chosen here. |

The alerts' buttons reuse `dedupe.replan` («Volver a buscar» / "Look
again") and `organize.refresh.button` («Volver a analizar» / "Scan
again"); no new button keys.

`regionsNow` lists `matching.regions` without `rom`, each through
`regions.<id>`, joined with «, ». `regionsNone` shows when that leaves
nothing (the list cannot be empty: validation requires at least one entry,
and it may be only `rom`).

Short hints replacing the long ones (the long text moves to a
`…Details` key behind «Más información»):

| Key | es (short) | en (short) |
|-----|-----------|-----------|
| `settings.library.followSymlinksHint` | Las carpetas enlazadas se leen como si estuvieran dentro. | Linked folders are read as if they were inside. |
| `settings.library.followSymlinksDetails` | Nunca se escribe arte fuera de la carpeta de la biblioteca: las ROMs de una carpeta enlazada que apunta a otro sitio se muestran, pero su arte se omite con un error. | Art is never written outside the library folder: ROMs in a linked folder that points elsewhere are listed, but their art is skipped with an error. |
| `settings.library.ignoredExtsHint` | Separadas por comas, sin punto. | Comma-separated, without dots. |
| `settings.library.ignoredExtsDetails` | Los archivos con estas extensiones nunca se toman como ROMs. | Files with these extensions are never taken as ROMs. |
| `settings.dedupe.quarantineDetails` | No puede estar dentro de la carpeta de ROMs ni de un sistema: la consola listaría las copias. | It can't be inside the ROMs folder or a system folder: the console would list the copies. |
| `settings.dedupe.oneGameOneRomHint` | Deja una sola versión de cada juego. | Keeps one version of each game. |
| `settings.dedupe.oneGameOneRomDetails` | Aparta también las otras versiones del mismo juego (otra región o revisión). | Also sets aside the other versions of the same game (another region or revision). |
| `settings.dedupe.rulesDetails` | Se miran en este orden; la primera regla que decide, decide. | They are checked in this order; the first rule that decides, decides. |

`settings.dedupe.quarantineDetails` repeats the text of the existing error
`settings.dedupe.quarantineInside` on purpose (one is help, the other an
error); the error key stays. `settings.dedupe.rulesHint` becomes «Usa las
flechas para ordenarlas.» / "Use the arrows to reorder them." The English
short hints are checked against the current English values when the keys
are written; the table gives the meaning, not a fixed string.

Search keywords (`settings.keywords.<entry>`, comma-separated words per
locale) for the settings people name by another word: RAR (unrar, 7-Zip,
7z), Papelera (trash, borrar, eliminar), carpeta de apartados (cuarentena,
quarantine), región (país, idioma), enlaces simbólicos (symlink, alias,
acceso directo), ocultos (dotfiles, punto), duplicados (copias, repetidos).

Keys left unused after the move (`settings.organize.title`,
`settings.dedupe.title`, `settings.dedupe.regionOrder`) are removed from
both locales in the same commit that stops using them; `git grep` each
key first, as the Duplicates screen or other components may use it.

## 8. Data flow

1. `openSettings(section, opts)` sets `screen='settings'`, `section`
   (or keeps the last one), and `settingsFrom = current screen` unless the
   current screen is `settings`. `opts.focus` sets `focusSetting` and opens
   the fold that holds it.
2. The panel renders from `useSettingsStore.settings`, unchanged.
3. Search: the query string lives in `SettingsScreen` state (cleared when
   Settings closes). `search.ts` maps it over `SETTINGS_INDEX` with
   `translate(locale, key)` for the UI locale and the others; the
   normalised strings are memoised per locale, the matches per query.
4. Choosing a result sets `section`, adds its `fold` to `openFolds`, and
   sets `focusSetting = id`. The panel, after render, finds
   `document.getElementById(id)`, scrolls, focuses it, then clears
   `focusSetting`. If the element is missing (settings not loaded, an
   empty dynamic family) focus goes to the active tab instead.
5. Back: `closeSettings()` sets `screen = settingsFrom ?? 'main'` and
   clears `settingsFrom`. If `settingsFrom` is `duplicates` but
   `duplicatesRoot` is null (or `organize` without `organizeSource`), it
   goes to `'main'`. `App` focuses `[data-settings-opener]` on the screen
   it returns to.
6. D12, what counts as a change:
   - **Duplicates.** The screen's levels, 1G1R, action and folder were
     copied from the settings at `start()` and are the user's on-screen
     choices; changing their defaults in Settings does not touch the open
     plan and shows no alert. The plan reads `dedupe.keep_rules`,
     `dedupe.container_order`, `dedupe.exclude_systems`,
     `dedupe.quarantine_name` and `matching.regions` from the settings on
     each `replan()`. The stamp is `JSON.stringify` of those slices, taken
     when the screen opens Settings; on return a difference shows the
     alert, whose button is the existing `replan`.
   - **Organise.** The request only carries what the user chose on the
     screen; every field left `None` (mode, names, layout, flat,
     duplicates) falls back to the setting, and `download_dats` /
     `external_tools` are always read at plan time. The stamp is the
     `organize` slice plus `dedupe.organize_step`. A difference shows the
     alert with the existing «Volver a analizar» (`refresh`), which keeps
     the user's decisions. `useCatalogues` re-reads `organize_step` on
     remount, so the duplicates picker default is already current.
   - Both screens unmount while Settings shows and remount on return; their
     plans live in the stores and are not reset (only `back()` resets
     them), so nothing is lost. A plan still running when Settings opened
     keeps running in its store and is shown again on return.

## 9. Edge cases and failure modes

- **Settings not loaded / load error.** Search field still shows; a
  result whose tab cannot render goes to that tab and shows the existing
  load error. About works without settings (unchanged).
- **Field error in a folded field.** The fold opens itself (D5) and shows
  the error under the field; closed again by the user, its trigger shows
  the danger dot and «, con errores».
- **A typed draft when the user switches tab.** Tab panels unmount on
  switch (`s === section && body`). Clicking another tab or a search
  result blurs the field first, which commits it (unchanged behaviour). A
  refused draft is dropped with the panel; its error stays in
  `errors[path]` and shows again when the tab is reopened, with the saved
  value. Unchanged from today.
- **Saving fails** (`saveError`). Unchanged alert at the top.
- **Unknown tab id** from an old caller (none exist; no URL scheme or
  native menu opens Settings). `setSection` ignores ids not in
  `SETTINGS_SECTIONS`; type-checked anyway.
- **Search with no match.** Empty state line with the query in quotes
  (rendered as React text, never HTML), nothing else; Escape clears.
- **Search in the other language.** "trash" finds «Enviar a la Papelera»
  in Spanish, shown in Spanish.
- **Accents and case.** `normalize('NFD')`, strip combining marks, lower
  case, so «organizacion» finds «Organización» (and «n» finds «ñ»; fine
  for search).
- **⌘F while a dialog or popover is open** (folder picker, select,
  confirm). The handler listens on `window` in the bubbling phase, like the
  library screen, and returns when `ev.defaultPrevented` or an open Radix
  modal layer exists. (The first draft said capture phase, which would run
  before Radix could mark the event.) An input inside a panel does not
  stop the event, so bubbling is enough.
- **⌘F / Ctrl+F in the Tauri webview.** WKWebView (macOS) and WebKitGTK
  (Linux) have no find bar. WebView2 (Windows) has one on Ctrl+F;
  `preventDefault()` on the keydown suppresses it as in Edge. Checked by
  hand on a Windows build before merge, because the browser-mock E2E runs
  in Chrome only. The app has no native menu with a ⌘F accelerator.
- **Deep link while a scan or apply runs.** Disabled during apply (D13).
  During planning the link works; see §8.6.
- **Region readout with only `rom`.** Shows `regionsNone`.
- **Narrow window.** Results replace the panel; the nav row scrolls inside
  itself; no horizontal page scroll at 375px.
- **Settings file edited by hand with values the UI does not list**
  (unknown layout id, unknown container kind, a profile id that no longer
  exists in `container_order`). Already handled by the components being
  moved; their code moves unchanged. The registry points at family anchors,
  so unknown rows do not break search.
- **Installed users.** Nothing persisted changes. Users who knew where an
  option was in Biblioteca find it with search; the CHANGELOG entry names
  the two new tabs.

## 10. Test plan

### Unit (Vitest)

- `search.test.ts`: accent/case folding; label, hint and keyword matches;
  placeholder stripping; ranking (label prefix > label word > hint >
  keyword); other-locale fallback; empty query returns nothing.
- `registry.test.tsx`: renders every tab (folds open) with a settings
  fixture that has container-order rows, and checks that every focusable
  element whose id starts with `setting-` (excluding `-description` and
  `-error`) is covered by a registry entry or family with the right
  `section` and `fold`, and that every registry `id` exists in the DOM.
  This is the guard against a moved setting losing its search entry.
- `navStore.test.ts`: `openSettings` records `settingsFrom`, not when
  already in Settings; `closeSettings` returns to it and clears it; falls
  back to `'main'` without a root/source; unknown section ignored.

`resetNavStore()` is added and used in `beforeEach` of every test that
today calls `closeSettings()` to reset (e.g. `Sections.test.tsx`), because
`closeSettings()` may now go to another screen.

### Component (Vitest + Testing Library)

- `Sections.test.tsx`: the five `library:` organise/duplicates tests move
  to `open('organize')` / `open('duplicates')` and open the fold where the
  control is now folded. Assertions on saved values stay byte-for-byte.
  New: Library shows no organise or duplicates control.
- `SettingsScreen.test.tsx`: tab list and order updated (12 tabs,
  General first); arrow keys move through tabs across groups (the
  "ArrowUp twice from Library reaches About" case is rewritten for the new
  order); ⌘F (mac platform stub) and Ctrl+F (other) focus search, Ctrl+F
  on mac does not; `/` focuses search outside a text field; typing «rar»
  lists one result and Enter lands focus on the external tools switch
  inside an open fold; Escape clears; results count live region. «edits
  the ignored folders» opens the Library fold first; «offers to reopen
  after a scan setting changes» still passes unchanged.
- `Field.test.tsx`: `MoreInfo` toggles `aria-expanded`; `AdvancedFold`
  opens on a field error and keeps a draft when closed.
- `DuplicatesScreen.test.tsx` / `OrganizeScreen.test.tsx`: the settings
  link opens the right tab; Back returns to the screen with the plan intact
  and focus on the link; a changed keeper rule (Duplicates) / mode
  (Organise) shows the alert and its button re-plans; a changed default
  action (Duplicates) shows no alert; the link is disabled while applying.
- `parity.test.ts`: unchanged, must pass with the new keys.

### Rust

No Rust changes. `cargo test --workspace` must stay green as a guard that
nothing in the settings schema moved; the existing settings round-trip and
defaults tests cover this.

### E2E (browser mock, port 1464, Chrome DevTools MCP)

1. Open Settings from the start screen; Tab into the nav; walk all tabs
   with ↓, check each tab's panel; End/Home. Run an accessibility audit
   of the nav (no `aria-required-children` error).
2. Library tab shows only scan options and recents; open «Opciones
   avanzadas», edit ignored extensions, reload, value kept.
3. ⌘F, type «papelera», Enter: Duplicados tab, focus on the Trash radio,
   arrival mark visible, then gone.
4. Open the mock library, click «Buscar duplicados», wait for the plan,
   click «Ajustes de Duplicados», move a keeper rule, Back: Duplicates
   screen with the same plan, the changed alert and focus on the link;
   «Volver a buscar» replans. Repeat changing only the default action: no
   alert.
5. Organise the mock downloads pile (`?mockPick=downloads`), «Ajustes de
   Organizar», change names, Back: alert; «Volver a analizar» keeps the
   decisions.
6. Narrow window (375px): nav row scrolls inside itself, ←/→ move tabs, no
   horizontal page scroll, search works.
7. Reduced motion emulated: fold, scroll and arrival mark without
   animation. Forced colours emulated: arrival mark visible.
8. Spanish and English: screenshots of each new tab in light and dark.

### Gates

`pnpm check-all`, `pnpm build`, `cargo fmt --all --check`,
`cargo clippy --workspace --all-targets --all-features -- -D warnings`,
`scripts/dev/clippy-windows.sh`, `cargo test --workspace --no-fail-fast`.

## 11. Coordination with `feat/dedupe-screen-polish`

The only touch points in `DuplicatesScreen.tsx` are one `<SettingsLink
section="duplicates" />` in the screen header (`.settings-header`, next to
the title) and one alert in the screen's existing alert area; plus a
stamp field in `dedupeStore`. All are self-contained, so a merge conflict
is a few-line resolution. The other workflow can add the link itself with
`useNavStore.getState().openSettings('duplicates')`; nothing else from this
branch is needed. The Back button must keep calling `reset()` only from
`back()`, never on unmount, or the round trip through Settings loses the
plan. Merge `origin/main` before each implementation step.

## 12. Review log (2026-10-26)

Corrections made to the first draft after checking it against `TODO.md`
and the code:

- Tab count was "twelve today"; it is ten (`SETTINGS_SECTIONS`). The
  grouped order moved General to the middle of the list; it stays first.
- "Default tab `general`" was wrong: `openSettings()` reopens the last tab.
- Groups as `role="group"` inside the `tablist` break ARIA (a tablist owns
  only tabs); the fallback became the design.
- The search field driving a listbox needs the combobox role.
- D12 assumed the Duplicates screen picks up every `dedupe.*` change on
  replan. It copies levels, 1G1R, action and folder at `start()`; only
  the plan-read keys trigger the alert now. Organise does not "re-plan on
  option changes" for settings; settings are fallbacks for what the screen
  did not set, and its re-plan action is «Volver a analizar».
- New copy «Buscar de nuevo» duplicated the existing `dedupe.replan`
  («Volver a buscar»); reused instead.
- Region readout used raw ids (`eu, us, jp`, which are not even the ids:
  `usa`, `europe`, `japan`, plus `rom` that dedupe ignores), and an "empty
  list" case that validation forbids. Now translated names, `rom` dropped,
  and the "only `rom`" case. Added that the order is shared with art.
- `SettingsScreen` focuses its `<h1>` on mount, after child effects: it
  would have stolen arrival focus.
- `closeSettings()` used by tests to reset could now land on another
  screen: `resetNavStore()`.
- `settingsFrom` set from inside Settings would make Back loop.
- Ctrl+F on macOS is a text-editing key; only ⌘F there. IME, Shift/Caps
  Lock and non-Latin layouts handled. Capture phase contradicted the
  `defaultPrevented` check; bubbling, like the library screen.
- Radix Collapsible unmounts closed content: drafts and ids lost; now
  `forceMount`.
- Registry test would have failed on `-description` / `-error` ids,
  dynamic container-order rows and controls without a `setting-` id
  (keeper rules, region list).
- «Descargar catálogos» was folded although it makes the app go online;
  visible now. `organize_step` is visible on Organizar (it was folded
  there while the brief lists it as a main Duplicados option).
- The selected-tab style is the sunken fill and Buckram cap, not
  `--cr-primary-subtle`. Panels have no heading today; none is added.
- The narrow row needs ←/→ (orientation follows the layout).
- The Duplicates link moves to the header: always rendered, out of the
  polish branch's area.
- Docs that name «Settings → Library» for organise/duplicates (README,
  `desktop-app.md`, `settings.md`) and the CHANGELOG were missing from the
  file list.
- Hints of the other tabs are out of scope (they were listed as if they
  belonged to other tabs, but were this branch's own fields).
