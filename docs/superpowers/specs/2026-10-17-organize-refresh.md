# Organise: scan the folder again

Status: design note, 2026-10-17. Extends `2026-10-17-organize-loose-roms-design.md` §4.2.

The owner's ask: on the organise proposal, a refresh button so the plan
catches files the user added to or removed from the folder after the
first scan, with its own working behaviour.

## 1. What the code does today (why a plain re-plan is not enough)

- `organizeStore.plan()` re-sends `organize_plan(request, decisions)`.
- The backend (`src-tauri/src/state.rs::organize_plan`) **reuses a kept
  survey** for the same source, options, layout and online flag
  (`OrganizeManager::survey_for`). A second `organize_plan` with the same
  request reads no file, so it would never see the change on disk.
- Item ids are positions in the plan (`id: items.len()` in
  `crates/coverly-core/src/organize/plan.rs`). Adding or removing one
  file shifts the ids, so decisions keyed by id cannot be sent as-is
  after the folder changed.

So refresh needs (a) a way to skip the kept survey and (b) decisions
re-keyed by file, not by id.

## 2. Placement and form

The button sits on the **source path line** above the options bar, at
its right end. That line names the folder, so the action that re-reads
that folder belongs to it; the options bar stays about *how* to file,
not *what* is in the pile.

```
/Users/me/Downloads/roms                          [⟳ Volver a analizar]
┌ Diseño ─┐ ┌ Destino ─┐ ┌ Carpetas ┐ ┌ Modo ┐ ┌ Nombres ┐
```

- `Button variant="outline" size="sm"`, lucide `RotateCw` icon plus label.
  - es: **Volver a analizar** · en: **Scan again**
  - No count in the label: nothing is counted yet when it is pressed
    (brand §7 rule 4 applies to the result line instead).
- Tooltip (`SimpleTooltip`): "Vuelve a leer la carpeta. Tus decisiones
  se mantienen." / "Reads the folder again. Your choices stay." followed
  by `<KbdGroup><Kbd>⌘</Kbd><Kbd>R</Kbd></KbdGroup>` (Ctrl+R off macOS).
- Shortcut **⌘R / Ctrl+R**, registered by `OrganizeScreen` on `window`
  only while the proposal is shown (same pattern as `LibraryScreen`'s
  ⌘F). It calls `preventDefault()` so the webview never reloads the
  page. Ignored while the button is disabled; ignored when focus is in a
  text input.
- While a refresh runs the button uses `loading` (spinner in place,
  `aria-busy`, disabled). The existing planning row ("Buscando
  archivos…" / "Identificando 120 de 480…" + Cancelar) is reused as the
  progress; no new progress UI.
- Disabled while `status` is `planning` (from any cause) or `applying`.
  Not rendered when the result screen is shown (`job !== null`).

The source line is otherwise plain text; the button is the only control
on it, right-aligned with `ml-auto`, and wraps under the path at narrow
widths (`flex-wrap`).

## 3. Behaviour

New store action `refresh()` in `organizeStore`:

1. Guard: `editable()` and a plan on screen (or `status === 'error'`
   with a request, see §5). Otherwise a no-op.
2. Remember the shown plan's keys: for every item, `key(item) =
   item.sources[0]` normalised (`\` to `/`, lower-cased). Map the current
   decisions from id to key: `kept[key] = decision without item`.
3. Supersede any dry run in flight exactly as `plan()` does today (bump
   `seq`, `organizeCancel(previous)`).
4. Call `organize_plan({ ...request, rescan: true }, [])`. `rescan` is a
   new optional request field; the backend skips `survey_for` when it is
   true and surveys the folder again (with progress, cancellable). It is
   **not** stored in `state.request`, so later option changes reuse the
   fresh survey as before.
5. Re-key: for each item of the new plan whose key is in `kept`, build
   `{ item: newId, ...kept[key] }`. Decisions whose key is gone are
   dropped. If any decision survived, call `organize_plan(request,
   remapped)` once more; this second call hits the fresh kept survey, so
   it reads no file and returns at once.
6. Diff the key sets of old and new plan (all items: auto, review and
   left): `added` = new keys not in old, `gone` = old keys not in new.
   Store `{ added: Set<key>, gone: number, keptDecisions: number }` as
   `lastRefresh`.
7. `cancel()` during a refresh returns to the plan that was shown, with
   its decisions, as it does for any re-plan (`shownWith`).

Matching is by the primary source path, case-insensitive (FAT32/exFAT
cards and macOS default volumes are case-insensitive; a rename that only
changes case is not a new file). A disc set keeps its decision as long
as its primary file (`sources[0]`) is still there.

Scroll and open groups: `SystemGroups` keys groups by `folder` and
`ReviewBuckets` by bucket key, and the page itself is not remounted, so
open groups and the scroll position survive the new plan without extra
work. A group that no longer exists simply disappears; a new group
starts closed like any other.

`setOption` and `decide` clear `lastRefresh` (the change summary
describes one refresh, not the plan forever).

## 4. Change summary

After a refresh, one calm `Alert variant="info"` sits under the options
bar (dismissible via `onDismiss`, `role="status"` so screen readers
hear it). The wording follows the existing summary line pattern
(label: number, no plurals machinery in the i18n layer):

| Case | Español | English |
|---|---|---|
| Changes | La carpeta cambió. Nuevos: 3 · Ya no están: 1 | The folder changed. New: 3 · Gone: 1 |
| Decisions kept (appended when > 0) | Tus decisiones se mantienen: 2 | Your choices kept: 2 |
| Decisions dropped (appended when > 0) | Decisiones de archivos que ya no están: 1 (descartadas) | Choices for files that are gone: 1 (dropped) |
| Identical | Sin cambios. La carpeta tiene los mismos archivos. | No changes. The folder holds the same files. |

Numbers in `tabular-nums`. "No changes" uses the same alert, so the
press always gets an answer.

**New rows.** Rows whose key is in `lastRefresh.added` get a small
`Badge variant="info"` reading **Nuevo / New** after the file name, and
a soft `--cr-info-bg` tint that settles to transparent once over
4 × `duration.slower` with `easing.ink` (brand "ink settles"; a single
`slow` step was too quick to notice). Under
`prefers-reduced-motion` the token durations are 0ms, so the tint is
simply absent and the badge alone marks the row. The badge stays until
the next option change, decision or refresh. Manila and Buckram are
deliberately not used: brand §1 reserves them for "you can still undo
it" and "Coverly did it".

A group or bucket holding new rows shows the count in its header
("+3"), so collapsed groups still tell the user where the new files
landed. Rows are passed an `isNew(item)` predicate from `Proposal`;
`SystemGroups`, `ReviewBuckets` and `LeftInPlace` render the badge.

## 5. Edge cases

- **Folder deleted or card unmounted.** `organize_plan` rejects with
  `not-found` (`existing_folder`). The screen shows the existing danger
  alert with the error text plus a **Reintentar / Try again** action
  (Alert `action`, offered for any failed plan) that calls `refresh()` again, and keeps the
  **Back** button working. The last plan stays visible but `stale`, so
  Apply and Export are disabled (they already key on `status !==
  'ready'`). No dead end: retry, change target, or go back. The error
  line says nothing was moved ("No se ha movido nada.").
- **After apply.** Not offered. Once a job exists the result screen is
  shown, and organising again starts from the library or start screen.
- **While a result is shown** (`job !== null`): button not rendered,
  shortcut not registered.
- **While planning from an option change**: the button is disabled; the
  user can cancel the plan in flight and then refresh.
- **Very large folders.** The first call reports walk/identify progress
  through `onOrganizeProgress` like the first scan; the row and the
  Cancel button are reused. The second (remap) call is instant because
  the survey is kept.
- **Locked mode** (loose files of an open library): same button, same
  behaviour; layout and target stay locked.

## 6. Auto-hint when the folder changes

Skipped in this iteration. There is no folder-mtime IPC, and the only
cheap existing look at a folder, `probeFolder`, counts ROM-like names up
to 5 000 entries: an add and a remove of the same number cancel out, and
it walks a whole library in locked mode on every window focus. A banner
that is sometimes wrong would undermine the calm, precise voice. If it
is wanted later, the right shape is a new `folder_stamp(path)` IPC
(directory mtime plus entry count of the source root, no recursion)
checked on `window` focus, showing a non-blocking info alert "La carpeta
cambió desde el último análisis · Volver a analizar".

## 7. Contract and mock

- `OrganizeRequest.rescan?: boolean` in `src/ipc/types.ts`, the
  Rust `OrganizeRequest` (`rescan: bool`, struct-level `#[serde(default)]`), `contract.requests.json`
  and the contract test. Excluded from the kept plan's request so an
  export or re-plan never re-surveys by accident.
- Mock (`src/ipc/mock/mockOrganize.ts`): honour `rescan`, and expose a
  dev-only helper to change a pile (`mockOrganizeTouch(path, { add,
  remove })`, reachable from `window` in mock mode) so tests and the
  browser E2E can add and remove files between scans.
- Tests: store re-keying (kept, dropped, case-insensitive), diff counts,
  "no changes", cancel restores, not-found keeps plan stale with retry;
  screen: button states, ⌘R/Ctrl+R prevents reload, badge on new rows.
