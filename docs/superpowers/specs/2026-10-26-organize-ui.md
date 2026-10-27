# Coverly ROMs — Organise: the Extraction Progress Stage (Design Spec)

> Status: **Draft, self-reviewed 2026-10-26 (§13)** · Date: 2026-10-26 · Owner: Miguel Angel Esparza Calero (MAECLY)
> Roadmap item: `TODO.md` §1 «Mejora de la interfaz de «Organizar por sistema»». Builds on [`2026-10-23-organize-archives-and-discs.md`](2026-10-23-organize-archives-and-discs.md) (§9.1 apply of an `Extract` item, phases A–C) and the brand ([`docs/08-brand/brand.md`](../../08-brand/brand.md) §2 the Slot, §5 type, §6 motion, §7 voice).
> Branch: `feat/organize-extract-progress`. `docs/TRACKER.md` and `TODO.md` are updated on `main` after merge, not on this branch.

## 1. Purpose

Applying a plan that extracts disc games takes minutes. The owner's pile (`discFixture.ts` mirrors it) holds three: `Next Tetris, The.zip` (PS1, 12 files, 406 MB), `Next Tetris, The_ On-Line Edition.zip` (Dreamcast, solid, 16 files, 1.19 GB) and `Tetris Plus.zip` (Saturn, solid, 52 files, 592 MB). Today the whole apply is a slot bar plus one line:

> Extrayendo Next Tetris, The.zip: 0 de 1 archivos…

That line is wrong as well as plain: `{done} of {total}` counts **plan items** (`OrganizeProgress.done/total`), not files in the archive, so a 52-file Saturn game reads «0 de 1 archivos» for its whole extraction. Nothing tells the user how fast it goes, how long is left, or that each file is checked.

This spec replaces that block with an illustrated **extraction stage** in the style of the landing (hero shelf and `#seguro` demo), a byte-based readout with speed and time left, separate game and track counters, and a per-track list that ticks as each file's CRC is checked. The scan, cover-fill and duplicates progress get the same readout so the app speaks about progress one way.

## 2. The owner's asks (verbatim from `TODO.md` §1)

- [ ] Animación de progreso al extraer en «Organizar por sistema». Ahora solo muestra la carpeta y una línea de texto («Extrayendo Next Tetris, The.zip: 0 de 1 archivos…»), y se queda corta para extracciones que duran minutos. Propuesta:
  - Un movimiento propio, en la línea de la landing «The Lean-To»: por ejemplo, el comprimido que se abre y las pistas que salen hacia la carpeta del sistema, con su `SystemIcon`.
  - Barra por bytes (`OrganizePhase::Extract { bytes_done, bytes_total }` ya llega del núcleo), velocidad y tiempo restante.
  - Lista de las pistas del juego en curso, con su CRC verificado (✓) a medida que terminan.
  - Respetar `prefers-reduced-motion` y usar los tokens de `design/tokens.json`.
- [ ] El contador «0 de 1 archivos» cuenta elementos del plan, no archivos dentro del comprimido, y se lee mal. Separar «juego 1 de 3» de «pista 4 de 12».
- [ ] Revisar otras pantallas de progreso que también están demasiado simples (relleno de portadas, duplicados, escaneo) para darles el mismo tratamiento.

Workflow brief additions: the owner strongly prefers **illustrated** animation (drawn, like the hero shelf and the `#seguro` mini demo) over screenshots or carousels; the browser mock must simulate a long multi-track extraction so it can be watched and E2E-tested.

## 3. What exists today (read before changing)

| Layer | Where | What it carries for an extraction |
|---|---|---|
| Core event | `crates/coverly-core/src/organize/plan.rs` `OrganizePhase::Extract { item, bytes_done, bytes_total }`, `OrganizeProgress { phase, done, total }` | Item id, bytes of the decode pass (a solid block's unwanted entries included), plan-item counters. First event `bytes_done == 0`, last `== bytes_total`. |
| Core emitter | `organize/apply.rs` `Applier::extract` (≈ l. 1685) | `report(0)`, then `on_bytes` throttled to `PROGRESS_EVERY` (100 ms), then `report(bytes_total)` after phase C. |
| Phase A | `organize/extract.rs` `extract_adopting` → `Run::entry` | Streams wanted entries **in ascending archive index** (`wanted.sort_unstable()`), writes each to a temp, re-reads it and checks its CRC, then pushes a `StagedEntry` (l. 1127 and 1212). `ExtractRequest::progress: &dyn Fn(u64)` gets bytes only. |
| Tauri | `src-tauri/src/organize/mod.rs` `Throttle::progress` (l. 400), `organize/view.rs` `ProgressWire` | Throttles to 100 ms per phase; an extract **edge** (`bytes_done == 0` or `>= bytes_total`) always goes out. Wire adds optional `item`, `bytes_done`, `bytes_total`. |
| TS | `src/ipc/types.ts` `OrganizeProgressEvent`; `organizeStore.progress` keeps the latest event only | |
| UI | `src/screens/OrganizeScreen.tsx` `applyingText`, `barOf` (l. 82–126), block at l. 445 | `Progress slots={SLOTS}` by bytes when extracting, by items otherwise; one `role="status"` line; Cancel. |
| Mock | `src/ipc/mock/mockOrganize.ts` apply loop (l. 1267+) | `EXTRACT_CHUNK` = 128 MB per 30 ms tick: Tetris Plus is 5 events, over in 0.15 s. No per-entry data. |
| Other screens | `ScanProgressView.tsx`, `fill/JobsPanel.tsx` (l. 271), `DuplicatesScreen.tsx` (l. 259, 332) | Bar + one counter line each, worded differently. |

There is **no per-entry signal** anywhere above phase A, so «pista 4 de 12» and the ✓ list cannot be derived honestly from bytes (a solid block decodes unwanted bytes, and sizes do not map to time). §5.1 adds it.

## 4. Decisions

Open questions and the default chosen. Each can be overturned by the owner without changing the rest of the design.

| # | Question | Decision | Why |
|---|---|---|---|
| D1 | Derive the track counter in the UI from bytes, or add it to the core? | **Add `entries_done` and `entries_total` to `OrganizePhase::Extract`**, counted when an entry is staged (written, re-read, CRC equal). | Exact and cheap (one counter in phase A). Bytes cannot tell which track is done in a solid block. The ✓ then means what it says: the file's CRC was checked. |
| D2 | Which entries are ✓ for `entries_done = k`? | **The first `k` plan entries in ascending archive `index`**. The UI sorts `transfer.entries` by `index`. | Phase A streams `wanted` sorted ascending and stages in that order, adopted entries (resume) included. A Rust test pins this order (§11.1) so the UI's assumption cannot drift silently. |
| D3 | Time left: for the current game or the whole apply? | **Current game only**, plus «juego n de N». | A later item's `bytes_total` is only known once its archive is opened (it includes the rest of each solid block, `decode_total`), so a job-wide ETA would be a guess presented as data. Brand §7.6: numbers are never invented. |
| D4 | How is speed measured? | **Sliding window of the last 5 s of `(t, bytes_done)` samples**, kept across items of one apply (a new item resets the samples, not the last rate). Shown after ≥ 2 s of samples; hidden while paused. | Smooths the 100 ms bursty events and the decoder's stalls between solid blocks; carrying the rate over avoids «Calculando…» at every game. |
| D5 | ETA wording and rounding | `< 60 s` → «Queda menos de un minuto»; `< 60 min` → whole minutes rounded **up** («Quedan unos 3 minutos»); else hours and minutes. Always «unos/about». | It is an estimate, and says so; rounding up never promises too soon. |
| D6 | «Juego n de N»: what is a game? | **N = plan items with `transfer.kind === 'extract'` and `action !== 'left'`**; n = 1 + number of distinct extract items already seen in this apply (a resume starts from the job view's finished extracts). Moves keep their existing «Moviendo 12 de 40». | The owner's three disc games read «Juego 2 de 3», not «Elemento 19 de 40» (cartridge moves interleave). |
| D7 | «Pista» or «archivo»? | **«Pista» when the item is a disc set (`item.disc != null`), «archivo» otherwise.** A single-entry extraction (a `.tar.gz` cartridge) shows no file counter and no list. | Owner's word for discs; a `.cue` or a cartridge is not a track. «Archivo 1 de 1» is noise. |
| D8 | ✓ when the plan has no CRC (`crc32: null`, tar/gz) | Same ✓, tooltip and screen-reader text «Tamaño comprobado» instead of «CRC comprobado». | Phase A still re-reads and checks size; the text stays true. |
| D9 | Animation technique | **Inline SVG + CSS keyframes driven by React state classes**, no animation library. Sleeves travel by **nested transforms** (outer `translateX` linear, inner `translateY` ease-out/ease-in) along an arc, not `offset-path`. | Same toolset as the landing; no dependency; nested transforms behave the same in WKWebView, WebView2 and WebKitGTK (CSS `offset-path` support in older WebKitGTK is not guaranteed). |
| D10 | Animation is truthful? | **Data first, animation after**: counters, bar and list update on the event; the drawing replays what happened, capped (≤ 3 sleeves in flight; older ones land instantly). | A burst of 40 small `.bin` verifying in one second must not turn into 16 s of choreography lagging behind the numbers. |
| D11 | Duplicates screen | **Readout only** (`WorkReadout`, §6.4), no illustration, touched in one block. | `TODO.md` §1.5 redesigns that screen in a parallel branch; a light touch keeps merges small. Its stage is that item's call. |
| D12 | Cover fill and scan | **Readout + a small stage variant** (§6.5). Fill: rate in covers/min and ETA by items. Scan: no ETA (its total grows as folders are found). | Consistent language without inventing a time the data cannot support. |
| D13 | CLI | `progress_line` prints `extract 2/3 tracks 4/12` (stderr, terminal only). | Same data, one line; no new flags. |
| D14 | Mock pacing | Browser mock extracts at a simulated **≈ 25 MB/s with ±20 % jitter** (Tetris Plus ≈ 24 s), `?mockExtract=fast` restores the old 128 MB chunks; Vitest passes its own pacing through `createMockBackend` options. | Long enough to watch and to E2E the ticks and ETA; tests stay fast and deterministic. |

## 5. Architecture and files touched

### 5.1 Rust

**`crates/coverly-core/src/organize/plan.rs`**

```rust
Extract {
    item: usize,
    bytes_done: u64,
    bytes_total: u64,
    /// Entries of the plan item staged so far (written, re-read, CRC
    /// equal), in ascending archive index; adopted entries count.
    entries_done: usize,
    /// The plan item's entries (`Transfer::Extract::entries.len()`; the
    /// generated playlist is not one).
    entries_total: usize,
},
```

**`crates/coverly-core/src/organize/extract.rs`**

- `ExtractRequest::progress: &'a dyn Fn(ExtractTick)` with `pub struct ExtractTick { pub bytes: u64, pub entries_done: usize }`.
- `Run::entry` calls it with the running byte count (as now) and the current `staged.len()`; right after each `staged.push` (both the written and the adopted path) it calls it once more, **unthrottled**.

**`crates/coverly-core/src/organize/apply.rs`** — `Applier::extract`: `report(bytes, entries_done)`; `on_bytes` keeps the 100 ms throttle for byte-only ticks and lets a tick whose `entries_done` changed through at once. First event `(0, 0)`, last `(bytes_total, entries_total)` after phase C. `resume_extract` reports adopted entries the same way.

**`src-tauri/src/organize/mod.rs`** — `Throttle::progress`: the extract edge also fires when `entries_done` differs from the last sent extract event's.

**`src-tauri/src/organize/view.rs`** — `ProgressWire` gains `entries_done` and `entries_total` (`skip_serializing_if = "Option::is_none"`, extract only). The wire test at l. 233 grows both fields.

**`crates/coverly-cli/src/organize.rs`** — `progress_line`: `extract {done}/{total} tracks {entries_done}/{entries_total}` (D13); its test grows a case.

No journal, plan or apply-order change: this is reporting only. `OrganizePhase` derives `Serialize`/`Deserialize` for internal use; no persisted file stores it (checked: the journal stores `TransferRecord`, not phases).

### 5.2 TypeScript

| File | Change |
|---|---|
| `src/ipc/types.ts` | `OrganizeProgressEvent` extract branch: `entries_done?: number; entries_total?: number` (optional: an older wire or the old mock leaves them out). |
| `src/ipc/contract.sample.json` | `organize_progress_extract` gains both fields (checked from both sides: `src/ipc/contract.test.ts` and `src-tauri/tests/contract.rs`). |
| `src/lib/rate.ts` (new) | Pure: `pushSample`, `rateOf(samples, now)` (bytes/s over 5 s window), `etaSeconds(rest, rate)`, `formatEta(t, locale, seconds)` (D5), `formatRate(locale, bytesPerSecond)` (decimal units, like `formatDecimalSize`). |
| `src/lib/extractProgress.ts` (new) | Pure: `extractView(plan, event, seen)` → `{ name, system, targetDir, noun: 'track' \| 'file', game: {n, total}, entries: {done, total} \| null, rows: TrackRow[], phase: 'decoding' \| 'checking' }`. `rows` = entries sorted by `index`, state `done` / `current` / `waiting`; `checking` when `entries_done === entries_total` and the item has not ended (phase B). |
| `src/lib/useReducedMotion.ts` (new) | `matchMedia('(prefers-reduced-motion: reduce)')` subscription. (`DupGroup.tsx` has a private copy; left alone, §1.5 can adopt this one.) |
| `src/hooks/useRate.ts` (new) | Keeps the sample ring in a ref per `plan_id`; returns `{ rate, eta }` and re-renders at most 2×/s. |
| `src/components/progress/WorkStage.tsx` (new) | The SVG frame shared by the variants: shelf rule, route, target folder with its tab and an optional `SystemIcon`. |
| `src/components/progress/ExtractStage.tsx` (new) | The archive box, lid, sleeves and their queue (D10). |
| `src/components/progress/TrackList.tsx` (new) | The per-track list (§6.3). |
| `src/components/progress/WorkReadout.tsx` (new) | Bar + amounts + rate + ETA + counters + action slot (§6.4). |
| `src/components/progress/progress.css` (new) | Stage styles and keyframes, token variables only. |
| `src/screens/OrganizeScreen.tsx` | The applying block becomes `<ApplyProgress>` (stage + readout + list); `applyingText` and `barOf` move into it; the old `organize.extracting` key goes. |
| `src/components/ScanProgressView.tsx` | Scan stage variant + `WorkReadout` (§6.5). |
| `src/components/fill/JobsPanel.tsx` | `WorkReadout` with rate and ETA by items. |
| `src/screens/DuplicatesScreen.tsx` | `WorkReadout` in its two progress blocks only (D11). |
| `src/ipc/mock/mockOrganize.ts` | Per-entry extraction (§9). |
| `locales/{es,en}/ui.json` | Keys of §7. |

## 6. UX and UI design

### 6.1 Design plan (frontend-design pass), checked against generic defaults

| Axis | Plan | The generic default it replaces |
|---|---|---|
| **Concept** | *Filing a box away*: a storage box opens, its tracks leave one by one as 3:4 sleeves (the Slot), travel a dashed route and drop into the system's Manila folder. Same vocabulary as the hero (slots, shelf rule, Buckram spine, Manila tab) and `#seguro` (dashed route from card to folder, `kit.svg#folder`/`#ftab`). | Spinner + percentage; an indeterminate stripe; a file-copy dialog with flying paper icons; confetti at the end. |
| **Tokens** | Colour roles only: box `--cr-surface-raised` + `--cr-border-strong`; route and waiting sleeves `--cr-slot-missing` (dash 3 3 on the route, 4 3 on sleeves, 1.5 px `border-width.slot`); travelling sleeve `--cr-slot-filled` outline; landed sleeve gets a 3 px `--cr-slot-applied` spine; folder `--cr-border` front, `--cr-manila` tab, `--cr-manila-fg` ink for the `SystemIcon` and count; ✓ `--cr-slot-applied`; paused stage `--cr-warning-fill` (bar tone). No raw hex, no blend modes (brand §2 implementation rules), both themes from the same roles. | Hard-coded blues and greens, gradient bars, drop-shadow glows. |
| **Type** | 13 px Atkinson for sentences (`text-ui`), **mono 12 px tabular** for every number, size, rate, file name and path (brand §5, §7.6); game name in mono inside the sentence. No Archivo (all text < 24 px). | Bold percentage in a display face; proportional digits that jitter as they tick. |
| **Layout** | One card in the content column. Row 1: stage SVG (viewBox 360 × 112, scales to the column, max 480 px wide). Row 2: the sentence and target path. Row 3: slot bar by bytes. Row 4: amounts · rate · ETA (left) and counters (right). Row 5: Cancel. Track list: right column at ≥ 900 px container width (`@container`), below and collapsible otherwise, max 8 rows tall then scrolls. | Centered modal with a bar; a full-width log of lines. |
| **Motion** | «Ink settles» (brand §6): `ink` and `enter`/`exit` easings, `slow`/`slower` durations; **no `spring`** (apply is banned from it). Beats in §6.2. Everything runs off token durations, so `prefers-reduced-motion: reduce` zeroes them; travel and march keyframes are also removed under that query (§6.6). | Bouncy easing; looping decorative animation unrelated to data. |
| **Restraint** | Only one thing loops (the current sleeve's dash march, the hero's `hr-march`); everything else moves once per real event. The stage pauses when the window is hidden. | Constant motion that competes with reading the numbers. |

### 6.2 The extraction stage (beats)

```
 ┌ box ┐  lid                    ┌──┐ tab: SystemIcon · 18/52
 │▯▯▯▯▯│ ╱  ▯⋯⋯⋯⋯⋯⋯⋯⋯⋯⋯⋯⋯⋯⋯⋯⋯⋯⋯⋯╲  │  │
 └─────┘   route (dashed)        └▮▮▮┘ folder front (sleeves drop behind it)
 ───────────────────────────────────── shelf rule, 3 px Buckram cap
```

| Beat | Trigger (data) | Drawing | Timing |
|---|---|---|---|
| Box in | first event of an item (`bytes_done == 0`) | Box slides in from the left over the shelf rule; the folder's tab cross-fades to the item's `SystemIcon` (`<SystemIcon systemId={item.system_id} />`, which already draws the fallback for an unknown id). | `slower`, `enter` |
| Lid opens | same | Lid rotates −14° about its back hinge. Inside, up to 5 dashed sleeves stand for the waiting tracks (5 = cap; the count is in the tab, not drawn). | `slow`, `ink`, 120 ms after box in |
| Current sleeve | `entries_done < entries_total` | The next sleeve rises half out of the lid; its dashed outline marches (stroke-dashoffset loop, the only loop). | 0.8 s per march cycle, `standard` |
| Verified | `entries_done` grew by k | For each of up to 3 (D10): outline turns solid (`fast`), the sleeve travels the arc to the folder mouth (`slower`, outer X linear, inner Y `enter` then `exit`), drops behind the folder front, a 3 px Buckram spine draws on it, the Manila tab flashes once (`base`, brand §6 Manila flash) and the tab count updates. The rest of k land instantly. | ≈ 640 ms per sleeve, 140 ms stagger (`--g`, as `#seguro`) |
| Checking | `entries_done == entries_total`, item not ended | Lid closes; a check stroke draws on the folder front (`slow`, `ink`). The sentence reads «Comprobando los archivos de …» (phase B, sheets and names). | — |
| Box out | next item starts or the apply ends | Empty box slides out right and fades (`slow`, `exit`); the folder keeps its stacked spines until the next box arrives. | — |
| Moves | `phase: 'apply'` | Box replaced by a short stack of three solid sleeves; one sleeve travels per `done` change, at most one per 400 ms; the folder shows no system icon (an apply batch mixes systems). | as Verified |
| Paused | job paused (disk full, volume gone) | March stops, the bar takes `tone="warning"`; the stage holds its frame. | instant |
| Cancelled | user cancel | Sleeves in flight return to the box; lid closes. | `slow`, `exit` |

### 6.3 Track list

An `<ol aria-label="Pistas de Tetris Plus.zip">` of the item's entries in archive order. Each row: `SlotMark` (`missing` waiting, `missing` + march class current, `applied` done with a ✓ glyph in `--cr-slot-applied`), the file name in mono with **middle truncation** (keeps `(Track 18).bin`; full name in `title`), the size in mono decimal units, and a visually hidden state («En espera», «Escribiendo», «CRC comprobado» / «Tamaño comprobado»). The current row is scrolled into view (`block: 'nearest'`, `behavior: 'auto'` under reduced motion) unless the user scrolled the list in the last 5 s. Hidden for single-entry items (D7). Collapsed by default below 900 px with a disclosure «Ver las 52 pistas».

### 6.4 `WorkReadout` (shared by every progress screen)

```
[▮▮▮▮▮▮▮▮▯▯▯▯▯▯▯▯]                                  ← Progress slots, value by bytes or items
312,4 MB de 591,9 MB · 24,1 MB/s · Quedan unos 12 s     Juego 2 de 3 · Pista 18 de 52
[Cancelar]
```

Props: `bar {value, max, tone}`, `amount?` (pre-formatted «312,4 MB de 591,9 MB» or «Carpeta 12 de 40»), `rate?`, `eta?` (`null` → «Calculando el tiempo…» while < 2 s of samples, omitted when not applicable), `counters` (strings), `action?`. The bar keeps `aria-label` and gets `aria-valuetext` = the amount. The sentence above the readout stays the single `role="status"` region, announcing only game changes, pause, and the end (not every tick).

### 6.5 The other progress screens

| Screen | Stage | Readout |
|---|---|---|
| **Scan** (`ScanProgressView`) | `WorkStage` variant: folders (small `kit`-style folder glyphs) pass under a reading line, and each 25 ROMs found add one solid slot to a row on the shelf rule (max 12 slots, then the row holds and only the count grows). | «Carpeta 12 de 40» · «312 ROMs encontradas» · current folder in mono. No ETA (D12). Indeterminate until the first folder reports, as now. |
| **Cover fill** (`JobsPanel`) | none in this item (the panel is a side sheet; the slot bar already is the brand's fill motion). | `jobs.progress` counter as now + rate «18 por minuto» and ETA by items once `done ≥ 5`; the current label (game) in mono. |
| **Duplicates apply** (`DuplicatesScreen`) | none (D11). | Its counter + ETA by items once `done ≥ 5`. |

### 6.6 Reduced motion

`tokens.css` already sets every `--cr-duration-*` to `0ms` under `prefers-reduced-motion: reduce`. In addition `progress.css` removes the march and travel keyframes inside `@media (prefers-reduced-motion: reduce)`, and `ExtractStage` (via `useReducedMotion`) skips the sleeve queue: verified sleeves appear directly in the folder, the lid is drawn open while decoding and closed while checking. The stage still renders, statically, because it carries meaning (box → folder, the system icon, the count). Changing the setting mid-extraction takes effect on the next render; sleeves in flight land at once.

## 7. Copy (es / en)

Sentence case, no exclamation marks, exact counts, numbers in mono (the components wrap `{…}` values in `<span class="mono tabular-nums">`). Sizes and rates use `formatDecimalSize` (locale decimal separator: «312,4 MB» / «312.4 MB»).

| Key | es | en |
|---|---|---|
| `organize.progress.extracting` | Extrayendo {name} en {folder} | Extracting {name} into {folder} |
| `organize.progress.checking` | Comprobando los archivos de {name}… | Checking the files of {name}… |
| `organize.progress.game` | Juego {n} de {total} | Game {n} of {total} |
| `organize.progress.track` | Pista {n} de {total} | Track {n} of {total} |
| `organize.progress.file` | Archivo {n} de {total} | File {n} of {total} |
| `organize.progress.amount` | {done} de {total} | {done} of {total} |
| `organize.progress.rate` | {rate}/s | {rate}/s |
| `organize.progress.left` | Quedan unos {time} | About {time} left |
| `organize.progress.leftUnderMinute` | Queda menos de un minuto | Less than a minute left |
| `organize.progress.measuring` | Calculando el tiempo… | Working out the time… |
| `organize.progress.tracksLabel` | Pistas de {name} | Tracks of {name} |
| `organize.progress.filesLabel` | Archivos de {name} | Files of {name} |
| `organize.progress.showTracks` | Ver las {count} pistas | Show the {count} tracks |
| `organize.progress.showFiles` | Ver los {count} archivos | Show the {count} files |
| `organize.progress.waiting` | En espera | Waiting |
| `organize.progress.writing` | Escribiendo | Writing |
| `organize.progress.crcChecked` | CRC comprobado | CRC checked |
| `organize.progress.sizeChecked` | Tamaño comprobado | Size checked |
| `organize.progress.cancelNote` | Si cancelas, se borra lo extraído de este juego y el comprimido queda como estaba. Los juegos ya extraídos se quedan. | If you cancel, what was extracted of this game is removed and the archive stays as it was. Games already extracted stay. |
| `organize.progress.stageLabel` | Extracción de {name} hacia la carpeta de {system} | Extraction of {name} into the {system} folder |
| `start.progress.folders` | Carpeta {done} de {total} | Folder {done} of {total} |
| `progress.perMinute` | {n} por minuto | {n} per minute |

Kept: `organize.applying` / `copying` / `organizing` / `starting` (moves). Removed: `organize.extracting` («… {done} de {total} archivos…»), whose only caller is `applyingText`. Time values come from `Intl.NumberFormat(locale, { style: 'unit', unit: 'minute' | 'second' | 'hour', unitDisplay: 'long' })` («3 minutos», «12 segundos»; «1 h 20 min» with `unitDisplay: 'short'` above an hour). The parity test covers every new key in both locales.

## 8. Data flow

```
phase A  Run::entry ──(bytes, staged.len())──▶ ExtractRequest::progress (ExtractTick)
                                                    │  byte ticks ≤ 1 per 100 ms; entry ticks immediate
apply.rs Applier::extract ──▶ OrganizeProgress { Extract { item, bytes_*, entries_* }, done, total }
                                                    │
Tauri    Throttle::progress (edges: bytes 0 / total, entries_done changed) ──▶ ProgressWire ──▶ "organize-progress"
                                                    │
TS       tauriBackend.onOrganizeProgress ──▶ organizeStore.progress (latest only)
                                                    │
UI       ApplyProgress: extractView(plan, progress, seenItems)   useRate(progress) ──▶ { rate, eta }
            ├─ ExtractStage (animates the delta of entries_done; queue ≤ 3)
            ├─ WorkReadout (bar by bytes, amount, rate, ETA, «Juego n de N · Pista k+1 de M»)
            └─ TrackList (first k rows ✓ in archive order)
```

The track counter shows the track **being written**: `min(entries_done + 1, entries_total)`; when all are done the counter reads «Pista 52 de 52» and the sentence switches to checking. Because `entries_done` is cumulative, a dropped or coalesced event only skips a frame, never a ✓. `seenItems` is a `Set<number>` kept in the component per `plan_id` (D6); the store stays as is.

## 9. Mock

`mockOrganize.ts` apply loop for an extract item:

- Walk `transfer.entries` sorted by `index`; for each, advance `bytes_done` in chunks of `rate × tick` (D14: 25 MB/s × 30 ms ≈ 0.75 MB, jitter ±20 % from a seeded PRNG so runs are reproducible), emitting `entries_done` unchanged; at the entry's end emit `entries_done + 1` immediately. A solid archive (`discFixture` marks it) adds the bytes of unwanted entries before the first wanted one, so the bar moves ahead of the ✓ as in Rust.
- First event `(0, 0)`, last `(bytes_total, entries_total)`; cancel between chunks rolls back as now.
- `?mockExtract=fast|slow` (default `slow` in the browser; `createMockBackend({ extractRate })` in tests, defaulting to the old fast chunks so existing tests keep their speed).
- Pause: the existing `organizePauseAfter` option also works mid-item for the paused-stage check.

## 10. Edge cases and failure modes

| Case | Behaviour |
|---|---|
| Event without `entries_*` (older wire, old mock) | No track counter, no list, no sleeves; bar, rate, ETA and game counter as normal. |
| `bytes_total == 0` | Bar indeterminate (`value={null}`); no rate or ETA. |
| `bytes_done > bytes_total` or going backwards within one item | Clamped; backwards resets the samples (D4). |
| Solid archive: bytes advance with no ✓ for a while | Correct: the decoder is going through other games' entries. The current sleeve keeps marching. |
| Resume with adopted entries | They tick ✓ quickly at the start (they are CRC-checked by phase A); the game counter starts from the job view's finished extracts. |
| Plan entry without CRC | ✓ with «Tamaño comprobado» (D8). |
| Extraction fails (CRC mismatch, decode error, set incomplete, changed since plan) | No per-entry failure event exists: the stage stops at its last frame and the item's result appears in `OrganizeResult` as today, which already says nothing of that game was left in the library. The list does not show a ✗ it cannot attribute. |
| Cancel mid-item | Sleeves return, lid closes; `cancelNote` is shown under the Cancel button while extracting. |
| Paused (disk full, volume gone, tool gone) | Stage frozen, bar warning tone, rate and ETA hidden, existing pause UI. |
| 100+ entries, or 40 tiny entries verified in one event | List scrolls (no virtualisation needed under ~500 rows); animation queue capped (D10). |
| Very long or markup-like file names | Rendered as text (React), middle-truncated, full name in `title`. |
| Unknown system id or no icon | `SystemIcon` with `systemId` null or unknown draws its fallback; `stageLabel` uses the system's display name or the id. |
| Window hidden or minimised | Queue drained instantly on `visibilitychange`; nothing animates off screen. |
| Narrow window (< 900 px) | List below, collapsed; stage scales with the viewBox; no horizontal scroll. |
| Dark theme / high contrast | Roles only; slot-missing passes WCAG 1.4.11 in both themes (brand §2). |
| Very fast extraction (< 1 s) | Readout shows no rate (D4) and the stage plays at most the capped sleeves. |
| External tool (unrar, 7-Zip, bsdtar) with coarse byte reports | Rate drops toward 0 while the bar stalls; ETA falls back to «Calculando el tiempo…» if no bytes for 10 s rather than showing hours. |

## 11. Test plan

Gates (as CI): `pnpm check-all`, `pnpm build`, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `scripts/dev/clippy-windows.sh`, `cargo test --workspace --no-fail-fast`. Fixture archives are built under the worktree's `target/` with pinned mtimes; paths are compared by component, never by slash text (Windows CI). Nothing writes to the owner's ROM folders or volumes.

### 11.1 Rust

- `organize_extract_apply.rs`: the existing extract-progress test (l. 1614) also asserts `entries_done` is monotone, starts at 0, ends at `entries_total`, `entries_total == entries.len()`, and that every value from 0 to `entries_total` is seen (entry ticks are unthrottled).
- New: a solid multi-entry 7z fixture with plan entries listed **out of** archive order: the k-th entry tick happens after the entry with the k-th smallest `index` is staged (pins D2).
- New: resume with two adopted entries reports them in `entries_done`.
- `src-tauri/src/organize/mod.rs`: `Throttle` emits an extract event whose `entries_done` changed within 100 ms of the previous one, and still drops a byte-only one.
- `src-tauri/src/organize/view.rs`: wire JSON has `entries_done`/`entries_total` for extract, and neither for other phases.
- `crates/coverly-cli/src/organize.rs`: `progress_line` text for an extract event.

### 11.2 Vitest

- `rate.test.ts`: window maths, reset on backwards bytes, carry-over across items, `< 2 s` → no rate, ETA rounding thresholds (59 s, 60 s, 61 s, 59 min, 61 min) in es and en, 10 s stall fallback.
- `extractProgress.test.ts`: rows sorted by `index`; states for k = 0, mid, all; noun track vs file; single-entry → `entries: null`; game counter for 3 extract items among 40 (moves interleaved), and on resume; events without `entries_*`.
- `ExtractStage.test.tsx`: renders the item's `SystemIcon` and `stageLabel`; a jump of 10 in `entries_done` puts ≤ 3 sleeves in flight; with `matchMedia` reduced → no element with a travel class.
- `TrackList.test.tsx`: accessible names and hidden state texts, `title` with the full name, CRC vs size text, collapsed disclosure below the breakpoint.
- `WorkReadout.test.tsx`: `aria-valuetext`, measuring text, omitted ETA.
- `OrganizeScreen.test.tsx`: during a mock apply of `downloads`, the status reads «Juego 1 de 3» and «Pista n de 12», never «de 1 archivos»; the old key is gone.
- `ScanProgressView.test.tsx`, `JobsPanel.test.tsx`, `DuplicatesScreen.test.tsx`: new readout texts.
- `mockOrganize.test.ts`: extract events carry monotone `entries_done` ending at `entries_total`, first/last edges, `?mockExtract` parsing, cancel mid-item.
- Locale parity test (existing) covers §7; `pnpm tokens:check` covers token use.

### 11.3 E2E (browser mock)

`pnpm dev --port 1462 --strictPort`; Chrome DevTools MCP with `isolatedContext: 'organize-ui'` (fallback Playwright MCP, then `playwright-core` driving Chrome for Testing from the session scratchpad). Stop the server by port only.

1. `/?mockPick=downloads` → open, Organise, simulate, apply. Assert the stage is visible with the Saturn/Dreamcast/PS1 icon of the current game, «Juego n de 3», «Pista k de 52» growing, ✓ rows growing, rate and «Quedan unos …» present after 2 s. Screenshots at 1280 × 800 and 760 × 800, light and dark, es and en.
2. Emulate `prefers-reduced-motion: reduce`: `document.getAnimations()` scoped to the stage is empty after a tick; counters still advance.
3. Cancel during `Tetris Plus.zip`: the result reports that game not extracted and earlier games extracted; no console errors.
4. Scan screen and the fill jobs panel show the new readout (screenshots).
5. Console free of errors and React warnings throughout.

## 12. Out of scope

- A job-wide ETA (D3) and per-entry failure attribution (needs a new core event; revisit if the owner asks).
- The duplicates screen's own illustration (`TODO.md` §1.5).
- Any change to what apply does, the journal, undo or resume.
- Landing changes (read-only reference).

## 13. Self-review

- **Placeholders**: none; every key, file and field is named. Speeds, windows and caps have values (5 s, 2 s, 3 sleeves, 25 MB/s, 900 px, 8 rows, 10 s).
- **Contradictions checked**: D3 (ETA per game) vs §6.4 readout — the readout shows only the game's ETA; §6.5 ETAs are by items, where totals are known. D7 (no list for one entry) vs §6.3 — consistent. D10 (data first) vs §6.2 beats — beats are triggered by data deltas, capped. D11 (duplicates readout only) vs §6.5 — consistent. Reduced motion: tokens zero durations (brand §6) and §6.6 also removes loops, which zero durations alone would not stop (an infinite 0 ms animation still runs); stated.
- **Assumptions to verify in implementation**: phase A stages in ascending archive index for every backend, including external tools (D2, pinned by a test); `OrganizePhase` is not persisted anywhere (checked for the journal; re-check `jobs/`); `src-tauri/tests/contract.rs` builds `organize_progress_extract` from the Rust wire (update it with the sample).
