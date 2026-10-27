# Settings tabs: Library, Organise and Duplicates

Status: design spec, 2026-10-26. Branch `feat/settings-tabs`. Roadmap item
**1.6. Reordenar los ajustes de Biblioteca** (`TODO.md`).

This is a presentation change. The settings file (`settings.json`), its
schema, its keys, its defaults and its validation stay exactly as they are.
No Rust code changes. The only exception is the in-memory navigation store
(`src/stores/navStore.ts`), which gets two new tab ids and a return screen.

---

## 1. Goals

1. Settings › Library holds only what belongs there: how a library is
   scanned and the list of recent libraries.
2. Organise and Duplicates each get their own tab, with the common options
   at the top and the advanced ones folded.
3. Each option has one short help line. Longer explanations sit behind a
   «Más información» toggle.
4. People can find any setting by typing (⌘F / Ctrl+F inside Settings).
5. The Duplicates and Organise screens link straight to their settings tab
   and come back to where they were.
6. The tab list has room for the roadmap's future tabs (collections,
   devices, saves, RetroAchievements) without building any of them.

Non-goals: new settings, changes to defaults, a new settings file format,
any change to the Duplicates screen's own layout (that is
`feat/dedupe-screen-polish`).

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

## 3. Decisions

Open questions and the default chosen for each.

| # | Question | Decision | Why |
|---|----------|----------|-----|
| D1 | Does Matching absorb the duplicates region order? | **Yes, it already does.** `dedupe` has no region list; the `region` keeper rule reads `matching.regions`. The Duplicates tab shows the current order read-only, as a mono readout (`eu, us, jp…`), with a link «Cambiar en Coincidencias» that switches tab and focuses the region list. | One source of truth, already true in the data. The readout removes the current guesswork («Usa el orden de Coincidencias» says where it lives but not what it is). |
| D2 | Tab ids | Add `organize` and `duplicates` to `SettingsSection`. `library` stays and keeps its meaning (scan + recents). | Existing deep links (`openSettings('library')`) still land on a valid tab. `organize` matches the existing `settings.organize.*` copy; `duplicates` matches the screen name (`Screen = 'duplicates'`). |
| D3 | Tab order and grouping | Three labelled groups in the side nav: **Colección** (Biblioteca, Organizar, Duplicados, Perfiles), **Portadas** (Arte, Coincidencias, Fuentes), **Aplicación** (General, Red, Seguridad, Avanzado, Acerca de). General moves to the top of its group but stays the first tab opened when no tab is asked for. | Twelve flat tabs today, sixteen once the roadmap lands. Groups are information (what part of the app a tab governs), not decoration. Future tabs (Colecciones, Dispositivos, Partidas, RetroAchievements) slot into Colección. |
| D3a | Default tab | `general`, unchanged. | Existing behaviour and tests. |
| D4 | Fold label | «Opciones avanzadas» / "Advanced options", as the owner wrote. | The owner's words. The «Avanzado» tab (settings file) is a different thing; its fold-free name and its place under Aplicación keep them apart. |
| D5 | Fold state | Closed by default; stays open for the session once opened (kept in `navStore`, not in `settings.json` and not in browser storage). Search and deep links open the fold that holds their target. A fold with a field error opens itself. | Never hide a field the user is fixing or looking for. Not persisting keeps `settings.json` unchanged. |
| D6 | What is common vs advanced | See §5.2. Rule: an option is common if a first-time user changes it in their first week; everything that is a list, an order or an exclusion is advanced. | The owner's own example for Duplicados follows this rule. |
| D7 | «Más información» | A text toggle after the short hint that reveals one paragraph below it (`aria-expanded`, `aria-controls`). Not a tooltip, not a dialog. | Tooltips are hover-only and cut off long text; inline keeps reading order and works with a screen reader. |
| D8 | Search scope | A static registry of every setting (id, tab, label key, hint key, extra keywords per locale). Matches label, short hint and keywords, accent- and case-insensitive, in the current UI language plus the other one as a fallback. Results are settings, not tabs. | A DOM crawl would miss closed folds and inactive tabs. A registry is testable and fast. |
| D9 | ⌘F / Ctrl+F | Inside Settings it focuses the search field (the webview's own find is not used). `/` does the same when focus is not in a text field, like the library screen. Escape in the field clears it; a second Escape leaves the field. | Same pattern as `LibraryScreen.tsx`. |
| D10 | Choosing a result | Switches tab, opens the fold that holds it, scrolls it into view, moves focus to its control and marks it for 1.2 s (see §6.4). | The user asked "where is X"; the answer is X with focus on it. |
| D11 | Deep-link API | `openSettings(section?, from?)` is unchanged for callers that pass only a section. `navStore` records the screen Settings was opened from (`settingsFrom`); `closeSettings()` returns there. No other new export. Screens call `useNavStore.getState().openSettings('duplicates')`. | Small surface for the parallel Duplicates screen workflow: one call they already know. |
| D12 | Coming back to Duplicates after a settings change | The Duplicates store keeps its plan (it is keyed on root). If any `dedupe.*` or `matching.regions` value changed while away, the screen shows an info alert «Los ajustes de duplicados han cambiado. Busca de nuevo para aplicarlos.» with «Buscar de nuevo» (the existing `replan`). No automatic replan. | A scan can take minutes on a large library; the user decides. The Organise screen already re-plans on option changes and gets the same alert for `organize.*` / `dedupe.organize_step`. |
| D13 | Deep link while busy | «Ajustes de Duplicados» / «Ajustes de Organizar» is disabled while applying, like the Back button. | Leaving mid-apply would unmount the progress view. |
| D14 | Narrow windows (≤ 40rem) | Nav becomes a horizontal scrolling row; group labels hide, groups keep a wider gap between them. Search stays at the top, full width. | The current narrow layout wraps twelve tabs into four lines. |
| D15 | Where «Al organizar» (`dedupe.organize_step`) lives | On Organizar (advanced fold), with a one-line pointer and link from Duplicados. One control, one place. | It changes what an Organise run does; the brief lists it under Duplicados, so Duplicados keeps the pointer and search finds it under both words. |

## 4. Architecture and files

```
src/stores/navStore.ts                 + 'organize' | 'duplicates' sections,
                                         SETTINGS_GROUPS, settingsFrom,
                                         openFolds (session), focusSetting
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
  SettingsSearch.tsx        (new)      field, listbox of results
src/components/settings/SettingsLink.tsx (new)
                                         «Ajustes de …» button for screens
src/screens/DuplicatesScreen.tsx       + SettingsLink + changed alert (D12)
src/screens/OrganizeScreen.tsx         + SettingsLink + changed alert
src/App.tsx                            focus restore covers settingsFrom
src/styles/globals.css                 .settings-nav groups, search, mark
locales/{es,en}/ui.json                new keys (§7); old keys kept where
                                         reused, removed where unused
```

`SECTION_COMPONENTS` gains `organize: OrganizeSection` and
`duplicates: DuplicatesSection`. The registry type is
`Record<SettingsSection, …>`, so a new tab without a panel fails to
compile.

`registry.ts` entry:

```ts
interface SettingEntry {
  id: string;          // settingId(path), the DOM id of the field
  section: SettingsSection;
  fold?: string;       // the AdvancedFold id that holds it, if any
  label: string;       // i18n key
  hint?: string;       // i18n key
  keywords?: string;   // i18n key with extra search words
}
```

The data path does not change: each control still calls
`useSettingsStore.update(patch)` with the same patch shapes as today. Moving
a control between components does not change its `settingId`, its `name`
or its patch, so saved values, field errors (`errors[path]`) and tests that
query by name keep working.

## 5. Content of each tab

### 5.1 Biblioteca (library)

Visible: default layout profile, include hidden files, follow symbolic
links. Then «Bibliotecas recientes» with Forget.

Advanced fold: ignored folders, ignored extensions.

### 5.2 Organizar (organize)

Visible: mode, default layout, names, put system folders inside the ROMs
folder.

Advanced fold: download catalogues, external RAR programs (with the found
tools readout), the duplicates step when organising (`dedupe.organize_step`,
shown here and linked from Duplicados; one control, rendered on this tab
only), and the pointer to the settings file for thresholds and lists.

`dedupe.organize_step` lives on Organizar because it changes what the
Organise run does. Duplicados shows a one-line pointer to it.

### 5.3 Duplicados (duplicates)

Visible: what to do with the copies (set aside / Trash), the set-aside
folder with «Elegir…», one ROM per game.

Advanced fold: what to look for (levels), which copy stays (rules, order,
with the region readout and link, D1), format order per profile (already a
collapsible today; inside the fold it becomes a plain sub-group), systems
left out, and the pointer to «Al organizar» on Organizar.

### 5.4 Other tabs

Untouched apart from moving into groups. Their long hints get the same
short-hint + «Más información» treatment only where a hint is longer than
one line at 40rem (follow symlinks, set-aside folder, ignored extensions,
one ROM per game, the rules hint).

## 6. UX and UI

### 6.1 Design plan

Coverly ROMs is a collector's shelf. Settings is the back office of that
shelf: quiet, dense, legible. The one memorable thing on this screen is the
**arrival mark**: when search or a deep link takes you to a setting, its
row is briefly tabbed with the manila "applied" colour, the same tab the
library uses for art applied this run. Everything else stays still.

- **Colour.** No new tokens. Nav at rest `--cr-text-secondary`, selected
  tab `--cr-primary-subtle` / `--cr-primary-subtle-fg` (as today), group
  labels `--cr-text-secondary`. Arrival mark `--cr-manila` as a 3px leading
  bar plus `--cr-manila` at low alpha behind the row. Search match
  highlight is weight, not colour.
- **Type.** Archivo for the screen title and the tab panel heading;
  Atkinson Hyperlegible Next for labels, hints, nav and group labels (group
  labels in `text-caption`, sentence case, medium weight; never all caps);
  Atkinson Hyperlegible Mono only for values that are data (paths, region
  ids, extension lists, result counts).
- **Layout.** Left-aligned throughout. Side nav 12rem; panel content max
  40rem (unchanged).

```
┌ ← Atrás   Configuración ─────────────────────────────────────────┐
├──────────────┬───────────────────────────────────────────────────┤
│ [Buscar… ⌘F] │ Duplicados                                        │
│              │                                                   │
│ Colección    │ Qué hacer con las copias                          │
│  Biblioteca  │  (•) Apartar en una carpeta                       │
│  Organizar   │  ( ) Enviar a la Papelera                         │
│ ▌Duplicados  │ Carpeta de apartados                              │
│  Perfiles    │  [<biblioteca>/_Duplicados        ] [Elegir…]     │
│              │  Vacía: _Duplicados dentro de la biblioteca.      │
│ Portadas     │  Más información                                  │
│  Arte        │ [x] Una ROM por juego                             │
│  Coincid.    │                                                   │
│  Fuentes     │ ▸ Opciones avanzadas                              │
│              │                                                   │
│ Aplicación   │                                                   │
│  General …   │                                                   │
└──────────────┴───────────────────────────────────────────────────┘
```

Search with a query replaces the panel with results until cleared:

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
  dots; changed to `›`, which reads as a path and matches the screen's
  "Configuración › Biblioteca" wording in docs.
- Arrival mark first used `--cr-primary` (a focus ring look-alike, which
  would read as a second focus). Manila is the brand's "this just
  happened" colour; kept.

### 6.3 Components

- `AdvancedFold`: the existing `Collapsible` with a ghost button,
  chevron rotating 90° (`duration-fast`, `ease-standard`, none under
  `prefers-reduced-motion`). Label «Opciones avanzadas» plus a mono count
  of the options inside: «Opciones avanzadas (5)».
- `MoreInfo`: `Button variant="link" size="sm"` «Más información» /
  «Menos información»; the paragraph uses `hintText`.
- `SettingsSearch`: `Input` with a leading search icon, `Kbd` hint
  (`modKeyLabel()` + F) when empty, clear button when not. Results are a
  `role="listbox"` driven by the field (`aria-activedescendant`); ↑/↓
  move, Enter opens, Escape clears.
- `SettingsLink`: `Button size="sm" variant="ghost"` with the `Settings`
  lucide icon, `data-settings-opener` so focus returns to it.

### 6.4 Motion

Only answers to an action:

- Fold open/close: the existing `collapsible-down/up` animation.
- Arrival mark: the bar and tint appear at once and fade out over
  `--cr-duration-slower` after 1.2 s. With reduced motion they appear and
  disappear without a fade. Nothing moves position.
- Scroll into view: `block: 'center'`, `behavior: 'smooth'` unless reduced
  motion.

### 6.5 Accessibility

- Nav stays one `tablist` (Radix vertical tabs, arrow keys, Home/End).
  Group labels are not tabs: each group is a `role="group"` with
  `aria-labelledby` its label, inside the tablist container, and roving
  focus skips the labels. If Radix cannot group triggers, the labels are
  `aria-hidden` visual separators and each trigger gets
  `aria-describedby` its group label.
- The results count is a polite live region («3 ajustes», «Ningún ajuste»).
- Arrival moves focus to the control itself (the `Switch`, `Input`,
  `select`, or the first radio), so screen readers announce it.
- Folds keep `aria-expanded`; a fold opened by search is announced by the
  focus landing inside it.

## 7. Copy

Spanish: sentence case, no exclamation marks, exact counts, numbers in
mono. Both locales get every key (parity test). Counts follow the
catalog's existing `xOne` / `x` convention (no plural engine); the number
inside a count is rendered in the mono face by splitting the translated
line around a marker, as `ExternalToolsHint` already does.

| Key | es | en |
|-----|----|----|
| `settings.section.organize` | Organizar | Organise |
| `settings.section.duplicates` | Duplicados | Duplicates |
| `settings.group.collection` | Colección | Collection |
| `settings.group.covers` | Portadas | Covers |
| `settings.group.app` | Aplicación | App |
| `settings.advancedOptions` | Opciones avanzadas ({count}) | Advanced options ({count}) |
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
| `settings.dedupe.regionsNone` | Ninguna: se usa la región de la ROM | None: the ROM's own region is used |
| `settings.dedupe.regionsChange` | Cambiar en Coincidencias | Change in Matching |
| `settings.dedupe.organizeStepWhere` | Qué hacer al organizar se elige en Organizar. | What happens when organising is set in Organise. |
| `settings.dedupe.organizeStepLink` | Ir a Organizar | Go to Organise |
| `dedupe.settingsLink` | Ajustes de duplicados | Duplicates settings |
| `organize.settingsLink` | Ajustes de organizar | Organise settings |
| `dedupe.settingsChanged` | Los ajustes de duplicados han cambiado. Busca de nuevo para aplicarlos. | The duplicates settings changed. Look again to use them. |
| `dedupe.lookAgain` | Buscar de nuevo | Look again |
| `organize.settingsChanged` | Los ajustes de organizar han cambiado. Vuelve a preparar el plan para aplicarlos. | The organise settings changed. Plan again to use them. |

Short hints replacing the long ones (the long text moves to a
`…Details` key behind «Más información»):

| Key | es (short) | en (short) |
|-----|-----------|-----------|
| `settings.library.followSymlinksHint` | Las carpetas enlazadas se leen como si estuvieran dentro. | Linked folders are read as if they were inside. |
| `settings.library.followSymlinksDetails` | Nunca se escribe arte fuera de la carpeta de la biblioteca: las ROMs de una carpeta enlazada que apunta a otro sitio se muestran, pero su arte se omite con un error. | Art is never written outside the library folder: ROMs in a linked folder that points elsewhere are listed, but their art is skipped with an error. |
| `settings.library.ignoredExtsHint` | Separadas por comas, sin punto. | Comma-separated, without dots. |
| `settings.library.ignoredExtsDetails` | Los archivos con estas extensiones nunca se toman como ROMs. | Files with these extensions are never taken as ROMs. |
| `settings.dedupe.quarantineDetails` | No puede estar dentro de la carpeta de ROMs ni de un sistema: la consola listaría las copias. | It can't be inside the ROMs folder or a system folder: the console would list the copies. |
| `settings.dedupe.rulesDetails` | Se miran en este orden; la primera regla que decide, decide. | They are checked in this order; the first rule that decides, decides. |

`settings.dedupe.rulesHint` becomes «Usa las flechas para ordenarlas.» /
"Use the arrows to reorder them." Search keywords
(`settings.keywords.*`) are added for the settings people name by another
word: RAR (unrar, 7-Zip), Papelera (trash, borrar, eliminar), cuarentena
(apartados), región (país, idioma), enlaces simbólicos (symlink, alias).

Keys left unused after the move (`settings.organize.title`,
`settings.dedupe.title` if the tab heading uses the section key,
`settings.dedupe.regionOrder`) are removed from both locales in the same
commit that stops using them.

## 8. Data flow

1. `openSettings(section, from)` sets `screen='settings'`, `section`, and
   `settingsFrom = from ?? current screen`.
2. The panel renders from `useSettingsStore.settings`, unchanged.
3. Search: the query string lives in `SettingsScreen` state. `search.ts`
   maps it over `SETTINGS_INDEX` with `t()` for both locales; results are
   memoised on `(query, locale)`.
4. Choosing a result sets `section`, adds its `fold` to `openFolds`, and
   sets `focusSetting = id`. The panel, after render, finds
   `#${id}` and focuses its control, then clears `focusSetting`.
5. Back: `closeSettings()` sets `screen = settingsFrom ?? 'main'`. `App`
   focuses `[data-settings-opener]` on the screen it returns to.
6. D12: when Duplicates/Organise opens Settings, it stores a stamp
   (`JSON.stringify` of the relevant settings slices) in its store. On
   return it compares the stamp with the current settings; a difference
   shows the alert.

## 9. Edge cases and failure modes

- **Settings not loaded / load error.** Search field still shows; results
  that point to a tab whose panel cannot render go to that tab and show
  the existing load error. About works without settings (unchanged).
- **Field error in a closed fold.** The fold opens itself (D5) and shows
  the error under the field; the fold trigger shows a danger dot while
  closed so the error is never hidden after a save from elsewhere.
- **Saving fails** (`saveError`). Unchanged alert at the top.
- **Unknown tab id** from an old caller (none exist today). `setSection`
  ignores ids not in `SETTINGS_SECTIONS`; type-checked anyway.
- **Search with no match.** Empty state line with the query in quotes,
  nothing else; Escape clears.
- **Search for a setting whose label changed language.** The other locale
  is a fallback, so "trash" finds Papelera in Spanish.
- **Accents and case.** `normalize('NFD')` and strip combining marks, so
  «organizacion» finds «Organización».
- **⌘F while a dialog or popover is open** (folder picker, select). The
  handler checks `ev.defaultPrevented` and an open Radix layer, like the
  library screen; it does nothing then.
- **⌘F in the Tauri webview.** `preventDefault` stops the webview's find
  bar on all three platforms; the handler runs in the capture phase so an
  input inside a panel does not swallow it.
- **Deep link while a scan or apply runs.** Disabled during apply (D13).
  During planning the link works; the plan keeps running in its store and
  the screen picks it up again on return.
- **Opened from Duplicates, then Back after the library was closed
  elsewhere.** Not possible: Settings has no "close library". If
  `duplicatesRoot` is null on return, `closeSettings` falls back to
  `'main'`.
- **Region readout with an empty region list.** Shows
  `settings.dedupe.regionsNone` («Ninguna: se usa la región de la ROM»).
- **Narrow window.** Results replace the panel; nav row scrolls; no
  horizontal page scroll.
- **Settings file edited by hand with values the UI does not list**
  (unknown layout id, unknown container kind). Already handled by the
  components being moved; their code moves unchanged.

## 10. Test plan

### Unit (Vitest)

- `search.test.ts`: accent/case folding; label, hint and keyword matches;
  ranking (label prefix > label word > hint > keyword); other-locale
  fallback; empty query returns nothing.
- `registry.test.tsx`: renders every tab (folds forced open) and checks
  that every `[id^="setting-"]` field in the DOM has a registry entry with
  the right `section` and `fold`, and that every registry entry exists in
  the DOM. This is the guard against a moved setting losing its search
  entry.
- `navStore.test.ts`: `openSettings` records `settingsFrom`;
  `closeSettings` returns to it; falls back to `'main'`; unknown section
  ignored.

### Component (Vitest + Testing Library)

- `Sections.test.tsx`: the `library:` organise and duplicates tests move
  to `open('organize')` / `open('duplicates')` and open the fold where the
  control is now folded. Assertions on saved values stay byte-for-byte.
  New: Library shows no organise or duplicates control.
- `SettingsScreen.test.tsx`: tab list and order updated (12 tabs, grouped);
  arrow keys move through tabs across groups; ⌘F and Ctrl+F focus search;
  `/` focuses search outside a text field; typing «rar» lists one result
  and Enter lands focus on the external tools switch inside an open fold;
  Escape clears; results count live region.
- `Field.test.tsx`: `MoreInfo` toggles `aria-expanded`; `AdvancedFold`
  opens on a field error.
- `DuplicatesScreen.test.tsx` / `OrganizeScreen.test.tsx`: the settings
  link opens the right tab; Back returns to the screen with focus on the
  link; a changed setting shows the alert and «Buscar de nuevo» replans;
  the link is disabled while applying.
- `parity.test.ts`: unchanged, must pass with the new keys.

### Rust

No Rust changes. `cargo test --workspace` must stay green as a guard that
nothing in the settings schema moved; the existing settings round-trip and
defaults snapshot tests cover this.

### E2E (browser mock, port 1464, Chrome DevTools MCP)

1. Open Settings from the start screen; Tab into the nav; walk all tabs
   with ↓, check each panel heading; End/Home.
2. Library tab shows only scan options and recents; open «Opciones
   avanzadas», edit ignored extensions, reload, value kept.
3. ⌘F, type «papelera», Enter: Duplicados tab, focus on the Trash radio,
   arrival mark visible, then gone.
4. Open a library (`?mockPick=mixed`), open Duplicates, click «Ajustes de
   duplicados», switch the action, Back: Duplicates screen with the
   changed alert; «Buscar de nuevo» replans.
5. Same from Organise with mode.
6. Narrow window (375px): nav row scrolls, no horizontal page scroll,
   search works.
7. Reduced motion emulated: fold and arrival mark without animation.
8. Spanish and English: screenshots of each new tab in light and dark.

### Gates

`pnpm check-all`, `pnpm build`, `cargo fmt --all --check`,
`cargo clippy --workspace --all-targets --all-features -- -D warnings`,
`scripts/dev/clippy-windows.sh`, `cargo test --workspace --no-fail-fast`.

## 11. Coordination with `feat/dedupe-screen-polish`

The only touch points in `DuplicatesScreen.tsx` are one `<SettingsLink
section="duplicates" />` in the folder row and one alert above the groups.
Both are self-contained components, so a merge conflict is a two-line
resolution. The other workflow can add the link itself with
`useNavStore.getState().openSettings('duplicates')`; nothing else from this
branch is needed. Merge `origin/main` before each implementation step.
