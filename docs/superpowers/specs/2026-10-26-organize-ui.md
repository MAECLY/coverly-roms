# Coverly ROMs — Organise: the Extraction Progress Stage (Design Spec)

> Status: **Draft, self-reviewed and adversarially reviewed 2026-10-26 (§13, §14)** · Date: 2026-10-26 · Owner: Miguel Angel Esparza Calero (MAECLY)
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
| Core emitter | `organize/apply.rs` `Applier::extract` (≈ l. 1685) | `report(0)`, then `on_bytes` throttled to `PROGRESS_EVERY` (100 ms), then `report(bytes_total)` after phase C. Nothing is reported between the last byte tick of phase A and the end of phase C (phase B, renames, playlist). |
| Apply order | `organize/apply.rs` l. 335 (`apply`) and l. 419 (`resume`) | Items run in **plan order**, filtered by `applicable()` (l. 425: `action == auto && target != None`, or a set-aside item with an as-is transfer). A resume runs the same order minus the finished items and counts those in `done_before`. |
| Phase A | `organize/extract.rs` `extract_adopting` → `Run::entry` | Asks the backend for the wanted entries sorted ascending (`wanted.sort_unstable()`); every backend's `stream_entries` hands them out in ascending index (`zip.rs` l. 1050, `external.rs` l. 1949/1980, the 7z and tar readers in storage order, which is index order). Each entry is written to a temp, re-read, and its re-read CRC compared with the decoded one (and the decoded one with the listing's when the format stores one, `VerifyingReader`), then a `StagedEntry` is pushed (l. 1127 and 1212). `ExtractRequest::progress: &dyn Fn(u64)` gets bytes only. |
| Tauri | `src-tauri/src/organize/mod.rs` `Throttle::progress` (l. 402), `organize/view.rs` `ProgressWire` | Throttles to 100 ms per phase against the last **sent** event; an extract **edge** (`bytes_done == 0` or `>= bytes_total`) always goes out. Wire adds optional `item`, `bytes_done`, `bytes_total`. `OrganizeJobView.progress` carries the latest event too (a window that mounts mid-apply starts from it). |
| TS | `src/ipc/types.ts` `OrganizeProgressEvent`; `organizeStore.progress` keeps the latest event only | `organizeStore.plan` is **`null`** after `resumeJob` (a job resumed from the history: `reset()` clears it) and before a plan exists; `applyingText` already falls back to `organize.organizing` and `#id` names. |
| Store states | `organizeStore` `accept()` | A **paused** job leaves `applying` (`status: 'paused'`) and the screen shows `OrganizeResult` with Resume; there is no paused frame inside the applying block. Cancel during apply calls `organizeCancelApply()` and stays `applying` until the job view says `cancelled`; there is no «cancelling» state today. |
| UI | `src/screens/OrganizeScreen.tsx` `applyingText`, `barOf` (l. 82–126), block at l. 445 | `Progress slots={SLOTS}` by bytes when extracting, by items otherwise; one `role="status"` line; Cancel. |
| Mock | `src/ipc/mock/mockOrganize.ts` apply loop (l. 1267+) | `EXTRACT_CHUNK` = 128 MB per `host.tick()` (`setTimeout(delayMs)`): Tetris Plus is 5 events, over in a fraction of a second. No per-entry data. Pauses (`organizePauseAfter`) happen **between** items only, and end the apply. |
| i18n | `src/i18n/core.ts` | `t()` returns a **plain string** with `{name}` interpolation; there is **no plural support** and no way to wrap a value in an element. |
| Other screens | `ScanProgressView.tsx`, `fill/JobsPanel.tsx` (l. 271), `DuplicatesScreen.tsx` (l. 259, 332) | Bar + one counter line each, worded differently. The scan view wraps **everything** in `role="status"` (every tick is announced). The fill panel is a **dialog** with a plain (non-slot) bar. |

There is **no per-entry signal** anywhere above phase A, so «pista 4 de 12» and the ✓ list cannot be derived honestly from bytes (a solid block decodes unwanted bytes, and sizes do not map to time). §5.1 adds it.

## 4. Decisions

Open questions and the default chosen. Each can be overturned by the owner without changing the rest of the design.

| # | Question | Decision | Why |
|---|---|---|---|
| D1 | Derive the track counter in the UI from bytes, or add it to the core? | **Add `entries_done` and `entries_total` to `OrganizePhase::Extract`**, counted when an entry is staged (written, re-read, CRC equal). Byte ticks carry the current `entries_done` too, so the count rides the existing 100 ms cadence; only the edges (§5.1) bypass it. | Exact and cheap (one counter in phase A). Bytes cannot tell which track is done in a solid block. The ✓ then means what it says: the file was checked. Coalescing is safe because the count is cumulative: a 500-entry archive of tiny files does not flood IPC with 500 events. |
| D2 | Which entries are ✓ for `entries_done = k`? | **The first `k` plan entries in ascending archive `index`**. The UI sorts `transfer.entries` by `index`. | Phase A asks for `wanted` sorted ascending and every backend streams in that order (§3), adopted entries (resume) included. A Rust test pins this order (§11.1), including a run through an external tool when one is installed, so the UI's assumption cannot drift silently. |
| D3 | Time left: for the current game or the whole apply? | **Current game only**, plus «juego n de N». | A later item's `bytes_total` is only known once its archive is opened (it includes the rest of each solid block, `decode_total`), so a job-wide ETA would be a guess presented as data. Brand §7.6: numbers are never invented. |
| D4 | How is speed measured? | **Sliding window of the last 5 s of `(t, bytes_done)` samples**, kept across items of one apply (a new item resets the samples, not the last rate). Shown after ≥ 2 s of samples. Timestamps are taken in the webview on arrival (`performance.now()`), not sent by the core. | Smooths the 100 ms bursty events and the decoder's stalls between solid blocks; carrying the rate over avoids «Calculando…» at every game. A monotonic clock is immune to wall-clock changes. |
| D5 | ETA wording and rounding | `≤ 60 s` → «Queda menos de un minuto»; `< 60 min` → whole minutes rounded **up** («Quedan unos 3 minutos», never fewer than 2 by construction); `≥ 60 min` → «Queda más de una hora» / «Quedan más de {n} horas» (n = whole hours, rounded down). | It is an estimate, and says so; rounding up never promises too soon. The i18n has no plurals (§3), so each wording is its own key and none can produce «unos 1 minuto» or «unos 1 h 20 min» (gender and number agreement in Spanish). |
| D6 | «Juego n de N»: what is a game? | **N = plan items that `applicable()` accepts (§3: `action === 'auto' && target !== null`) with `transfer.kind === 'extract'`; n = 1 + the position of the event's `item` among them in plan order** (= apply order). Memoised per plan. When the store has no plan (job resumed from the history) the game counter is hidden. Moves keep their existing «Moviendo 12 de 40». | The owner's three disc games read «Juego 2 de 3», not «Elemento 19 de 40» (cartridge moves interleave). Deriving n from the plan, not from events seen, stays right when the screen mounts mid-apply, after a resume, and when an event is coalesced away; the job view has no per-item results while applying, so it cannot seed a counter. |
| D7 | «Pista» or «archivo»? | **«Pista» when the item is a disc set (`item.disc != null`), «archivo» otherwise.** A single-entry extraction (a `.tar.gz` cartridge) shows no file counter and no list. The counter counts every plan entry (sheets included; the generated `.m3u` is not one). | Owner's word for discs; a cartridge is not a track. «Archivo 1 de 1» is noise. Counting the sheet keeps «Pista 12 de 12» equal to the files the user sees in the folder. |
| D8 | ✓ when the plan has no CRC (`crc32: null`, tar/gz) | Same ✓, tooltip and screen-reader text «Tamaño y copia comprobados» instead of «CRC comprobado». | Without a stored CRC phase A still checks the size and re-reads the written file against the CRC computed while decoding; «tamaño comprobado» alone would undersell it and «CRC comprobado» would claim a check against the archive that did not happen. |
| D9 | Animation technique | **Inline SVG + CSS keyframes driven by React state classes**, no animation library. Sleeves travel by **nested transforms** (outer `translateX` linear, inner `translateY` ease-out/ease-in) along an arc, not `offset-path`. The `SystemIcon` is **HTML laid over the SVG** (absolutely positioned in a wrapper with the viewBox's aspect ratio), never inside `<foreignObject>`. Folder and tab shapes are **copied as path data** from the landing's `kit.svg#folder` / `#ftab` (the app ships no `kit.svg`; no external `<use href>`). | Same toolset as the landing; no dependency; nested transforms behave the same in WKWebView, WebView2 and WebKitGTK (CSS `offset-path` support in older WebKitGTK is not guaranteed). `SystemIcon` renders an `<svg>` *or* an HTML `<span>` mount (unknown ids, fallbacks), and `foreignObject` clips and mis-transforms in WebKit. |
| D10 | Animation is truthful? | **Data first, animation after**: counters, bar and list update on the event; the drawing replays what happened, capped (≤ 3 sleeves in flight; older ones land instantly). | A burst of 40 small `.bin` verifying in one second must not turn into 16 s of choreography lagging behind the numbers. |
| D11 | Duplicates screen | **Readout only** (`WorkReadout`, §6.4) in the **apply** block (l. 259) only; the planning block with its phase list (l. 332) is left to `TODO.md` §1.5. No illustration. Done as the last commit, right after `git fetch origin && git merge origin/main`. | `TODO.md` §1.5 redesigns that screen in a parallel branch; one small hunk keeps the merge trivial. Its stage is that item's call. |
| D12 | Cover fill and scan | **Readout + a small stage variant for scan** (§6.5); fill gets the readout only. Fill: rate in covers/min and ETA by items. Scan: no ETA. | The scan's `total` is known from the start (top-level folders), but folders differ by orders of magnitude (one can hold 5 000 ROMs), so an ETA by folders would be a guess. Consistent language without inventing a time the data cannot support. |
| D13 | CLI | `progress_line` becomes a pure `progress_text(p) -> Option<String>` (testable) and prints `extract 2/3 tracks 4/12` (stderr, terminal only). `progress_due` gains extract rules: an item's first and last event and each change of `entries_done`, at most one line per second; byte-only ticks print nothing. | Today `progress_due` checks `done` (the plan item), which is constant within an item: byte ticks print **every** 100 ms when `done` is 0 or a multiple of 25 and never otherwise. Same data, one line; no new flags. |
| D14 | Mock pacing | Browser mock extracts at a simulated **≈ 25 MB/s with ±20 % jitter** (Tetris Plus ≈ 24 s, the whole `downloads` pile ≈ 90 s), `?mockExtract=fast` restores the old 128 MB chunks and `?mockExtract=<MB/s>` sets a rate (E2E uses `80` to keep runs short); Vitest passes its own pacing through `createMockBackend` options. | Long enough to watch and to E2E the ticks and ETA; tests stay fast and deterministic. |
| D15 | Cancel feedback | Pressing Cancel during apply sets a **local `cancelling` flag** in `ApplyProgress`: the button disables and reads «Cancelando…», the march stops, the sentence says «Cancelando: se deshace lo extraído de {name}…». No return-to-box animation. | The store stays `applying` until the job view says `cancelled`, then the result replaces the block (§3): any «cancel» animation would be cut off mid-flight. The flag resets when the block unmounts. |

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
- `Run::entry` and `Run::sink_entry` call it with the running byte count (as now) and the current `staged.len()`; right after each `staged.push` (both the written and the adopted path) they call it once more with the new count.

**`crates/coverly-core/src/organize/apply.rs`** — `Applier::extract`: `report(bytes, entries_done)`; `on_bytes` keeps the 100 ms throttle for every tick (byte or entry: the count rides along, D1) and lets through at once only the **all-staged edge** (`entries_done == entries_total`, first time), so the UI learns that phase B started even though no tick follows until phase C ends. First event `(0, 0)`, last `(bytes_total, entries_total)` after phase C. `resume_extract` ends in `apply_extract(item, &adopted)`, i.e. this same path, so adopted entries are reported the same way with no extra code. A resumed item whose finals are all intact goes straight to the archive's fate and reports **no** extract event (the UI shows the move line for it).

**`src-tauri/src/organize/mod.rs`** — `Throttle::progress`: the extract edge also fires when `entries_done` differs from the last **sent** extract event's (the core already coalesces to ≤ 10/s, so this cannot flood; it only stops a jittered 99 ms tick from hiding a ✓ behind a long solid-block stall). `OrganizePhase` stays `Copy` (the throttle stores it).

**`src-tauri/src/organize/view.rs`** — `ProgressWire` gains `entries_done` and `entries_total` (`skip_serializing_if = "Option::is_none"`, extract only). The wire test at l. 233 grows both fields.

**`crates/coverly-cli/src/organize.rs`** — `progress_text` / `progress_due` as D13; `progress_is_printed_sparingly` grows extract cases.

Every site that builds or destructures `OrganizePhase::Extract` must change (exhaustive patterns without `..` stop compiling): `coverly-core` `apply.rs` and `tests/organize_extract_apply.rs`; `coverly-cli` `organize.rs`; `src-tauri` `organize/mod.rs`, `organize/view.rs`, `tests/contract.rs`, `tests/organize_state.rs`.

No journal, plan or apply-order change: this is reporting only. `OrganizePhase` derives `Serialize`/`Deserialize` for internal use; no persisted file stores it (checked: the journal stores `TransferRecord`, not phases; re-check `jobs/` in implementation). The wire change is additive, so an installed app reading an old job never sees it.

### 5.2 TypeScript

| File | Change |
|---|---|
| `src/ipc/types.ts` | `OrganizeProgressEvent` extract branch: `entries_done?: number; entries_total?: number` (optional: an older wire or the old mock leaves them out). |
| `src/ipc/contract.sample.json` | `organize_progress_extract` gains both fields (checked from both sides: `src/ipc/contract.test.ts` and `src-tauri/tests/contract.rs`). |
| `src/lib/rate.ts` (new) | Pure: `pushSample`, `rateOf(samples, now)` (bytes/s over 5 s window), `etaSeconds(rest, rate)`, `etaKey(seconds)` → `{ key, vars }` (D5, one key per wording), `formatRate(locale, bytesPerSecond)` (decimal units through `formatDecimalSize`). |
| `src/lib/extractProgress.ts` (new) | Pure: `extractView(plan \| null, event)` → `{ name, systemId, targetDir, noun: 'track' \| 'file', game: {n, total} \| null, entries: {done, total} \| null, rows: TrackRow[], phase: 'decoding' \| 'checking' }`. `rows` = entries sorted by `index`, state `done` / `current` / `waiting`; `checking` when `entries_done === entries_total` and the item has not ended (phase B). `plan === null` or an item missing from it → `name` `#id`, `systemId` null, `game` null, `rows` empty; the counter still comes from the event. `game` from D6 (a memoised index of applicable extract item ids). `targetDir` joins `plan.target_root` and the item's `target` parent with the **root's own separator** (`\` when the root has one), so Windows shows one style of path. |
| `src/i18n/rich.tsx` (new) | `useRichT()` → `tr(key, vars, wrap)`: splits the template on `{name}` and returns React nodes, wrapping chosen values (numbers, sizes, names) in `<span className="mono tabular-nums">`. `t()` cannot do this (§3); the plain `t()` stays for `aria-label`/`title`. Parity is unchanged (same JSON). |
| `src/lib/useReducedMotion.ts` (new) | `matchMedia('(prefers-reduced-motion: reduce)')` subscription (`addEventListener('change')`, with the `addListener` fallback WKWebView on macOS 11 / Safari 13 needs). (`DupGroup.tsx` has a private copy; left alone, §1.5 can adopt this one.) |
| `src/hooks/useRate.ts` (new) | Keeps the sample ring in a ref per `plan_id`; returns `{ rate, eta }` and re-renders at most 2×/s. |
| `src/components/progress/WorkStage.tsx` (new) | The SVG frame shared by the variants: shelf rule, route, target folder with its tab, plus an HTML overlay slot for the `SystemIcon` (D9). |
| `src/components/progress/ExtractStage.tsx` (new) | The archive box, lid, sleeves and their queue (D10). |
| `src/components/progress/TrackList.tsx` (new) | The per-track list (§6.3). |
| `src/components/progress/WorkReadout.tsx` (new) | Bar + amounts + rate + ETA + counters + action slot (§6.4). |
| `src/components/progress/progress.css` (new) | Stage styles and keyframes, token variables only (`var(--cr-…)`; no hex, no `rgb()`), imported by `ExtractStage`. All keyframes and `animation` declarations live **inside `@media (prefers-reduced-motion: no-preference)`** (opt-in, as the landing's `site.css` does), plus a `@media (forced-colors: active)` block (§6.6). |
| `src/screens/OrganizeScreen.tsx` | The applying block becomes `<ApplyProgress>` (stage + readout + list, D15 cancelling flag); `applyingText` and `barOf` move into it; the old `organize.extracting` key goes. |
| `src/components/ScanProgressView.tsx` | Scan stage variant + `WorkReadout` (§6.5); `role="status"` moves from the wrapper to the sentence only. |
| `src/components/fill/JobsPanel.tsx` | `WorkReadout` with rate and ETA by items (keeps its plain bar). |
| `src/screens/DuplicatesScreen.tsx` | `WorkReadout` in its apply block only (D11). |
| `src/ipc/mock/mockOrganize.ts` | Per-entry extraction (§9). |
| `locales/{es,en}/ui.json` | Keys of §7. |

## 6. UX and UI design

### 6.1 Design plan (frontend-design pass), checked against generic defaults

| Axis | Plan | The generic default it replaces |
|---|---|---|
| **Concept** | *Filing a box away*: a storage box opens, its tracks leave one by one as 3:4 sleeves (the Slot), travel a dashed route and drop into the system's Manila folder. Same vocabulary as the hero (slots, shelf rule, Buckram spine, Manila tab) and `#seguro` (dashed route from card to folder, `kit.svg#folder`/`#ftab`). | Spinner + percentage; an indeterminate stripe; a file-copy dialog with flying paper icons; confetti at the end. |
| **Tokens** | Colour roles only: box `--cr-surface-raised` + `--cr-border-strong`; route and waiting sleeves `--cr-slot-missing` (dash 3 3 on the route, 4 3 on sleeves, 1.5 px `border-width.slot`); travelling sleeve `--cr-slot-filled` outline; landed sleeve gets a 3 px `--cr-slot-applied` spine; folder `--cr-border` front, `--cr-manila` tab, `--cr-manila-fg` ink for the `SystemIcon` and count; ✓ `--cr-slot-applied`; no warning tone (a pause leaves this block, §3); `forced-colors` mapping in §6.6. No raw hex, no blend modes (brand §2 implementation rules), both themes from the same roles. | Hard-coded blues and greens, gradient bars, drop-shadow glows. |
| **Type** | 13 px Atkinson for sentences (`text-ui`), **mono 12 px tabular** for every number, size, rate, file name and path (brand §5, §7.6); game name in mono inside the sentence. No Archivo (all text < 24 px). | Bold percentage in a display face; proportional digits that jitter as they tick. |
| **Layout** | One card in the content column. Row 1: stage SVG (viewBox 360 × 112, scales to the column, max 480 px wide). Row 2: the sentence and target path. Row 3: slot bar by bytes. Row 4: amounts · rate · ETA (left) and counters (right). Row 5: Cancel. Track list: a CSS grid `grid-template-columns: repeat(auto-fit, minmax(22rem, 1fr))` puts it in a right column when the card is wide enough (≈ 900 px) and below it otherwise, collapsible there, max 8 rows tall then scrolls. **No `@container`**: Tailwind 3.4 needs a plugin the app does not have, and container queries need Safari 16 while the app supports macOS 11 (`tauri.conf.json`), whose WKWebView may be older. | Centered modal with a bar; a full-width log of lines. |
| **Motion** | «Ink settles» (brand §6): `ink` and `enter`/`exit` easings, `slow`/`slower` durations; **no `spring`** (its token says never on apply). Beats in §6.2. Everything runs off token durations (`--cr-duration-*`, `--cr-ease-*`), which `tokens.css` zeroes under `prefers-reduced-motion: reduce`; keyframes are only declared under `no-preference` (§6.6). | Bouncy easing; looping decorative animation unrelated to data. |
| **Restraint** | Only one thing loops (the current sleeve's dash march, the same idea as the hero's `hr-march` dash, which there runs once); everything else moves once per real event. Nothing animates while the window is hidden. | Constant motion that competes with reading the numbers. |

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
| Box out | next item starts | Empty box slides out right and fades (`slow`, `exit`); the folder keeps its stacked spines until the next box arrives. When the apply ends the result replaces the whole block, so there is no end beat. | — |
| Moves | `phase: 'apply'` | Box replaced by a short stack of three solid sleeves; one sleeve travels per `done` change, at most one per 400 ms; the folder shows no system icon (an apply batch mixes systems). | as Verified |
| Cancelling | user pressed Cancel (D15) | March stops, queued sleeves land instantly, lid stays as it is; nothing else moves until the result replaces the block. | instant |

A pause (disk full, volume gone, tool gone) ends the applying state and shows `OrganizeResult` with Resume (§3); the stage has no paused frame. Resuming mounts a fresh stage.

### 6.3 Track list

An `<ol aria-label="Pistas de Tetris Plus.zip">` of the item's entries in archive order. Each row: `SlotMark` (`missing` waiting, `missing` + march class current, `applied` done with a ✓ glyph in `--cr-slot-applied`), the file name in mono with **middle truncation** (keeps `(Track 18).bin`; full name in `title`), the size in mono decimal units, and a visually hidden state («En espera», «Escribiendo», «CRC comprobado» / «Tamaño y copia comprobados»). The name shown is the entry's last path segment (`name` is `/`-separated on every OS; the game folder is already in the sentence). The list is **not** a live region (a 52-row list announcing each ✓ would drown the reader); the counter text carries progress. Rows are memoised by `(index, state)` so a tick re-renders at most two rows. The current row is kept visible by setting the list's own `scrollTop` (never `scrollIntoView`, which also scrolls the page and the dialog under the reader), instantly under reduced motion, unless the user scrolled the list in the last 5 s. Hidden for single-entry items (D7) and when the plan is unknown. Collapsed by default when it wraps below the stage, with a disclosure button (`aria-expanded`, `aria-controls`) «Ver las 52 pistas».

### 6.4 `WorkReadout` (shared by every progress screen)

```
[▮▮▮▮▮▮▮▮▯▯▯▯▯▯▯▯]                                  ← Progress slots, value by bytes or items
312,4 MB de 591,9 MB · 24,1 MB/s · Quedan unos 12 s     Juego 2 de 3 · Pista 18 de 52
[Cancelar]
```

Props: `bar {value, max, tone, slots?}`, `amount?` (pre-formatted «312,4 MB de 591,9 MB» or «Carpeta 12 de 40»), `rate?`, `eta?` (`null` → «Calculando el tiempo…» while < 2 s of samples, omitted when not applicable), `counters` (strings), `action?`. The bar keeps `aria-label` and gets its value text through Radix's `getValueLabel` (= the amount; Radix would otherwise announce a bare percentage). The sentence above the readout stays the single `role="status"` region; its text changes only on a new game, on checking, and on cancelling, so screen readers hear those and not every tick. The readout row is `aria-live="off"` and `aria-hidden` on nothing (it stays readable on demand). Wraps onto two lines at narrow widths; numbers are `tabular-nums` so the row does not jitter.

### 6.5 The other progress screens

| Screen | Stage | Readout |
|---|---|---|
| **Scan** (`ScanProgressView`) | `WorkStage` variant: folders (small `kit`-style folder glyphs) pass under a reading line, and each 25 ROMs found add one solid slot to a row on the shelf rule (max 12 slots, then the row holds and only the count grows). | «Carpeta 12 de 40» · «312 ROMs encontradas» · current folder in mono. No ETA (D12). Indeterminate until the first folder reports, as now. Only the sentence is `role="status"`. |
| **Cover fill** (`JobsPanel`) | none in this item (the panel is a dialog over the library; its plain bar stays). | `jobs.progress` counter as now + rate «18 por minuto» and ETA by items once `done ≥ 5` and ≥ 20 s of samples, hidden while the job is paused (rate limit, offline: `job.state`); the current label (game) in mono. |
| **Duplicates apply** (`DuplicatesScreen`) | none (D11). | Its counter + ETA by items once `done ≥ 5`. |

ETA by items reuses `rate.ts` with items instead of bytes and the same `etaKey` wordings (D5).

### 6.6 Reduced motion

`tokens.css` already sets every `--cr-duration-*` to `0ms` under `prefers-reduced-motion: reduce`. On top of that, `progress.css` declares the march and travel keyframes and every `animation:` **only inside `@media (prefers-reduced-motion: no-preference)`** (opt-in, the landing's pattern), so under `reduce` no animation object exists at all (a `0ms infinite` animation would still be live in `document.getAnimations()`); `transition` durations come from the zeroed tokens. `ExtractStage` (via `useReducedMotion`) skips the sleeve queue: verified sleeves appear directly in the folder, the lid is drawn open while decoding and closed while checking. The stage still renders, statically, because it carries meaning (box → folder, the system icon, the count). Changing the setting mid-extraction takes effect on the next render; sleeves in flight land at once. The OS settings reach the query on every target: macOS «Reducir movimiento», Windows «Efectos de animación» (WebView2), GNOME «Animaciones» (WebKitGTK).

**Hidden window.** The queue drains instantly on `visibilitychange` to hidden, and new deltas land instantly while `document.hidden`. Minimising does not set `hidden` on every webview (WKWebView reports occlusion, WebKitGTK may not), so the cap of D10 is what actually bounds the backlog; the visibility check is a bonus.

**Forced colours / high contrast** (Windows contrast themes in WebView2). `@media (forced-colors: active)` maps the stage's strokes to `CanvasText`, the travelling and done sleeves to `Highlight`, and fills to `Canvas`, so the drawing does not vanish (SVG `fill`/`stroke` from CSS variables are not forced by the browser). The ✓ in the list is a glyph plus hidden text, never colour alone (WCAG 1.4.1).

## 7. Copy (es / en)

Sentence case, no exclamation marks, exact counts, numbers in mono (the components render through `useRichT` (§5.2), which wraps `{…}` values in `<span class="mono tabular-nums">`; `t()` alone returns a string and cannot). Sizes and rates use `formatDecimalSize` (locale decimal separator: «312,4 MB» / «312.4 MB»); counts use `Intl.NumberFormat(locale)` like the scan view («1.234» / «1,234»). New keys go in one contiguous block per namespace to keep merges with the parallel branches (which also edit `locales/*.json`) to a single hunk.

| Key | es | en |
|---|---|---|
| `organize.progress.extracting` | Extrayendo {name} en {folder} | Extracting {name} into {folder} |
| `organize.progress.checking` | Comprobando los archivos de {name}… | Checking the files of {name}… |
| `organize.progress.game` | Juego {n} de {total} | Game {n} of {total} |
| `organize.progress.track` | Pista {n} de {total} | Track {n} of {total} |
| `organize.progress.file` | Archivo {n} de {total} | File {n} of {total} |
| `organize.progress.amount` | {done} de {total} | {done} of {total} |
| `organize.progress.rate` | {rate}/s | {rate}/s |
| `organize.progress.leftMinutes` | Quedan unos {n} minutos | About {n} minutes left |
| `organize.progress.leftUnderMinute` | Queda menos de un minuto | Less than a minute left |
| `organize.progress.leftOverHour` | Queda más de una hora | More than an hour left |
| `organize.progress.leftOverHours` | Quedan más de {n} horas | More than {n} hours left |
| `organize.progress.measuring` | Calculando el tiempo… | Working out the time… |
| `organize.progress.cancelling` | Cancelando: se deshace lo extraído de {name}… | Cancelling: undoing what was extracted of {name}… |
| `organize.progress.cancellingButton` | Cancelando… | Cancelling… |
| `organize.progress.tracksLabel` | Pistas de {name} | Tracks of {name} |
| `organize.progress.filesLabel` | Archivos de {name} | Files of {name} |
| `organize.progress.showTracks` | Ver las {count} pistas | Show the {count} tracks |
| `organize.progress.showFiles` | Ver los {count} archivos | Show the {count} files |
| `organize.progress.waiting` | En espera | Waiting |
| `organize.progress.writing` | Escribiendo | Writing |
| `organize.progress.crcChecked` | CRC comprobado | CRC checked |
| `organize.progress.sizeChecked` | Tamaño y copia comprobados | Size and copy checked |
| `organize.progress.cancelNote` | Si cancelas, se borra lo extraído de este juego y el comprimido queda como estaba. Los juegos ya extraídos se quedan. | If you cancel, what was extracted of this game is removed and the archive stays as it was. Games already extracted stay. |
| `organize.progress.stageLabel` | Extracción de {name} hacia la carpeta de {system} | Extraction of {name} into the {system} folder |
| `start.progress.folders` | Carpeta {done} de {total} | Folder {done} of {total} |
| `progress.perMinute` | {n} por minuto | {n} per minute |

Kept: `organize.applying` / `copying` / `organizing` / `starting` (moves). Removed: `organize.extracting` («… {done} de {total} archivos…»), whose only caller is `applyingText` (and `OrganizeScreen.test.tsx`, updated). `progress.perMinute` is a new top-level namespace (`progress`), shared by fill and duplicates. There is no plural support (§3): every wording with a number is chosen so the number is ≥ 2 or the key has no number (D5); `cancelNote` and `crcChecked` have none. The parity test covers every new key in both locales.

Spanish review notes: «Extrayendo {name} en {folder}» (not «a»), «Pista», «Juego» capitalised only at the start of their own segment in the readout; «CRC» stays as is (owner's word in `TODO.md`).

## 8. Data flow

```
phase A  Run::entry ──(bytes, staged.len())──▶ ExtractRequest::progress (ExtractTick)
                                                    │  ≤ 1 per 100 ms (count rides along); all-staged edge immediate
apply.rs Applier::extract ──▶ OrganizeProgress { Extract { item, bytes_*, entries_* }, done, total }
                                                    │
Tauri    Throttle::progress (edges: bytes 0 / total, entries_done changed) ──▶ ProgressWire ──▶ "organize-progress"
                                                    │
TS       tauriBackend.onOrganizeProgress ──▶ organizeStore.progress (latest only)
                                                    │
UI       ApplyProgress: extractView(plan | null, progress)   useRate(progress) ──▶ { rate, eta }
            ├─ ExtractStage (animates the delta of entries_done; queue ≤ 3)
            ├─ WorkReadout (bar by bytes, amount, rate, ETA, «Juego n de N · Pista k+1 de M»)
            └─ TrackList (first k rows ✓ in archive order)
```

The track counter shows the track **being written**: `min(entries_done + 1, entries_total)`; when all are done the counter reads «Pista 52 de 52» and the sentence switches to checking. Because `entries_done` is cumulative, a dropped or coalesced event only skips a frame, never a ✓. The game counter comes from the plan (D6), so nothing per apply is kept in the component besides the rate samples and the animation queue; the store stays as is. A screen that mounts mid-apply (navigating back to Organise) starts from `organizeStore.progress` and shows the right counters at once; only the rate needs its 2 s again.

## 9. Mock

`mockOrganize.ts` apply loop for an extract item:

- Walk `transfer.entries` sorted by `index`; for each, advance `bytes_done` in chunks of `rate × tick` (D14: 25 MB/s × 30 ms ≈ 0.75 MB, jitter ±20 % from a seeded PRNG so runs are reproducible), emitting `entries_done` unchanged; at the entry's end emit `entries_done + 1`. Chunks are paced by **elapsed time** (`performance.now()`), not by counting ticks, so a throttled background tab or a slow CI machine does not change the simulated speed. Every entry of the owner's three archives is wanted (one game per archive, `discFixture.ts`), so there are no unwanted solid-block bytes to simulate; the bar still runs ahead of the ✓ inside each entry, as in Rust. After the last entry, a short phase B pause (≈ 600 ms) shows the checking beat before the item's last event.
- First event `(0, 0)`, last `(bytes_total, entries_total)`; cancel between chunks rolls back as now.
- `?mockExtract=fast|slow|<MB/s>` (default `slow` in the browser; `createMockBackend({ extractRate })` in tests, defaulting to the old fast chunks so existing tests keep their speed). Fast mode still emits one event per entry so Vitest can check the counts.
- Pause: `organizePauseAfter` pauses between items only (it ends the apply, §3); nothing to add, since the stage has no paused frame.
- Mock-only code: the query parameters are read in `src/ipc/mock`, a chunk `backend.ts` (l. 297) imports only when there is no Tauri IPC, so the installed app never runs it (as `?mockPick` today). Unknown or non-positive `?mockExtract` values fall back to `slow`.

## 10. Edge cases and failure modes

| Case | Behaviour |
|---|---|
| Event without `entries_*` (older wire, old mock) | No track counter, no list, no sleeves; bar, rate, ETA and game counter as normal. |
| No plan in the store (job resumed from the history, `resumeJob`) | Name `#id`, generic folder (no `SystemIcon`, the fallback folder), no game counter, no list; bar, rate, ETA and «Pista k de M» from the event. |
| Resumed item whose finals are all intact | No extract event (§5.1): the move line shows for it, no stage beat. |
| Screen mounted mid-apply | Counters from the latest `progress` at once; rate «Calculando el tiempo…» for 2 s; the folder starts empty (no replay of earlier ✓). |
| `bytes_total == 0` | Bar indeterminate (`value={null}`); no rate or ETA. |
| `bytes_done > bytes_total` or going backwards within one item | Clamped; backwards resets the samples (D4). |
| Solid archive: bytes advance with no ✓ for a while | Correct: the decoder is going through other games' entries. The current sleeve keeps marching. |
| Resume with adopted entries | They tick ✓ quickly at the start (they are CRC-checked by phase A); the game counter starts from the job view's finished extracts. |
| Plan entry without CRC | ✓ with «Tamaño y copia comprobados» (D8). |
| Extraction fails (CRC mismatch, decode error, set incomplete, changed since plan) | No per-entry failure event exists: the stage stops at its last frame and the item's result appears in `OrganizeResult` as today, which already says nothing of that game was left in the library. The list does not show a ✗ it cannot attribute. |
| Cancel mid-item | Cancelling state (D15); `cancelNote` is shown under the Cancel button while extracting. A second press does nothing (button disabled). |
| Paused (disk full, volume gone, tool gone) | The applying block is replaced by `OrganizeResult` with Resume, as today (§3, §6.2). |
| 100+ entries, or 40 tiny entries verified in one event | List scrolls (no virtualisation needed under ~500 rows; memoised rows, §6.3); animation queue capped (D10); core coalesces ticks (D1), so IPC stays ≤ ~10 events/s. |
| Very long or markup-like file names | Rendered as text (React), middle-truncated, full name in `title`. Never through `dangerouslySetInnerHTML` (the only one stays `SystemIcon`'s trusted constant). |
| Names with right-to-left or control characters | Each name sits in its own `<bdi>` so it cannot reorder the counters around it. |
| Unknown system id or no icon | `SystemIcon` with `systemId` null or unknown draws its fallback; `stageLabel` uses the system's display name or the id. |
| Window hidden or minimised | Queue drained on `visibilitychange` where the webview reports it; the D10 cap bounds the backlog where it does not (§6.6). |
| Narrow window | List below, collapsed; stage scales with the viewBox; no horizontal scroll. |
| Dark theme / high contrast | Roles only; slot-missing passes WCAG 1.4.11 in both themes (brand §2); forced colours handled (§6.6). |
| Windows target path | Shown with the root's separator (§5.2 `targetDir`), middle-truncated, full path in `title`. |
| Very fast extraction (< 1 s) | Readout shows no rate (D4) and the stage plays at most the capped sleeves. |
| External tool (unrar, 7-Zip, bsdtar) with coarse byte reports, or the decoder waiting for memory (`Budget`) | Rate drops toward 0 while the bar stalls; ETA falls back to «Calculando el tiempo…» if no bytes for 10 s rather than showing hours. |
| Phase B / C / Trash take long (slow USB or SD card, `sync_all`) | Sentence stays on «Comprobando los archivos de …», bar at its last byte count (it can stop short of full: the rest of a solid block after the last wanted entry is never decoded, and the bar fills with the item's last event), no ETA; the lid stays closed. |

## 11. Test plan

Gates (as CI): `pnpm check-all`, `pnpm build`, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `scripts/dev/clippy-windows.sh`, `cargo test --workspace --no-fail-fast`. Fixture archives are built under the worktree's `target/` with pinned mtimes; paths are compared by component, never by slash text (Windows CI). Nothing writes to the owner's ROM folders or volumes.

### 11.1 Rust

- `organize_extract_apply.rs`: the existing extract-progress test (l. 1614) also asserts `entries_done` is monotone, starts at 0, ends at `entries_total`, `entries_total == entries.len()` (no `.m3u`), and that the all-staged edge (`entries_done == entries_total` with `bytes_done < bytes_total` or before phase C) is seen.
- New: a solid multi-entry 7z fixture with plan entries listed **out of** archive order: at every `ExtractTick`, the staged set equals the first `entries_done` entries by `index` (pins D2). The same assertion runs through the external-tool backend in `archive_external.rs` when 7-Zip is found (skipped otherwise, as the existing external tests are).
- New: resume with two adopted entries reports them in `entries_done`.
- New: 200 tiny entries produce at most ~(elapsed / 100 ms + 3) events (coalescing, D1).
- `src-tauri/src/organize/mod.rs`: `Throttle` emits an extract event whose `entries_done` changed within 100 ms of the previous sent one, and still drops a byte-only one.
- `src-tauri/src/organize/view.rs`: wire JSON has `entries_done`/`entries_total` for extract, and neither for other phases.
- `crates/coverly-cli/src/organize.rs`: `progress_text` for an extract event; `progress_due` prints an item's first event, each entry change at most once per second, and no byte-only tick.
- Fixtures are built under the worktree's `target/` with pinned mtimes; paths compared by component.

### 11.2 Vitest

- `rate.test.ts`: window maths, reset on backwards bytes, carry-over across items, `< 2 s` → no rate, ETA keys at 59 s, 60 s, 61 s, 119 s, 59 min, 60 min, 61 min, 2 h 59 min in es and en (no output contains «unos 1» or «1 horas»), 10 s stall fallback.
- `extractProgress.test.ts`: rows sorted by `index`; states for k = 0, mid, all; noun track vs file; single-entry → `entries: null`; game counter for 3 extract items among 40 (moves interleaved, review items and set-aside items excluded per `applicable`), on resume, and on a first event that is the 2nd game (mounted mid-apply); `plan === null`; events without `entries_*`; `targetDir` with a `C:\Roms` root keeps `\`.
- `rich.test.tsx`: values wrapped in `.mono`, literal text untouched, unknown `{x}` left as is (same as `t()`).
- `ExtractStage.test.tsx`: renders the item's `SystemIcon` (HTML overlay, not inside the SVG) and `stageLabel`; a jump of 10 in `entries_done` puts ≤ 3 sleeves in flight; with `matchMedia` reduced → no element with a travel class; cancelling drains the queue.
- `TrackList.test.tsx`: accessible names and hidden state texts, `title` with the full name, CRC vs size-and-copy text, disclosure `aria-expanded`, the list is not a live region, `scrollIntoView` never called.
- `WorkReadout.test.tsx`: value text through `getValueLabel`, measuring text, omitted ETA.
- `OrganizeScreen.test.tsx`: during a mock apply of `downloads`, the status reads «Juego 1 de 3» and the readout «Pista n de 12», never «de 1 archivos»; the old key is gone; Cancel switches to «Cancelando…» and disables.
- `ScanProgressView.test.tsx`, `JobsPanel.test.tsx`, `DuplicatesScreen.test.tsx`: new readout texts; the scan's `role="status"` holds only the sentence.
- `mockOrganize.test.ts`: extract events carry monotone `entries_done` ending at `entries_total`, first/last edges, `?mockExtract` parsing (`fast`, `slow`, `80`, garbage), cancel mid-item.
- `progress.css.test.ts`: the file has no hex or `rgb(` literal, and no `animation`/`@keyframes` outside a `prefers-reduced-motion: no-preference` block. (`pnpm tokens:check` only checks that the generated token files are current; it does not scan component CSS.)
- Locale parity test (existing) covers §7.

### 11.3 E2E (browser mock)

`pnpm dev --port 1462 --strictPort`; Chrome DevTools MCP with `isolatedContext: 'organize-ui'` (fallback Playwright MCP, then `playwright-core` driving Chrome for Testing from the session scratchpad). Stop the server by port only.

1. `/?mockPick=downloads` → open, Organise, simulate, apply (default `slow`). Assert the stage is visible with the Saturn/Dreamcast/PS1 icon of the current game, «Juego n de 3», «Pista k de M» growing, ✓ rows growing, rate and an ETA wording present after 2 s, and the checking sentence between games. Screenshots at 1280 × 800 and 760 × 800, light and dark, es and en.
2. Emulate `prefers-reduced-motion: reduce`: `document.getAnimations()` scoped to the stage is empty after a tick; counters still advance.
3. `?mockExtract=80`: cancel during the third game: «Cancelando…» shows, then the result reports that game not extracted and earlier games extracted; no console errors.
4. Navigate away from Organise and back mid-apply: counters correct at once (D6), no console errors.
5. Emulate `forced-colors: active`: the stage strokes are still visible (screenshot).
6. Scan screen and the fill jobs panel show the new readout (screenshots).
7. Console free of errors and React warnings throughout.

The real app is checked once on macOS with a copy of the owner's three archives made with `cp -RXp` under the worktree's `target/` (never the originals, no `._` files, deleted afterwards). WebView2 and WebKitGTK are covered by CI's build and Vitest only; the technique choices of D9 and §6.1 are what keep them safe.

## 12. Out of scope

- A job-wide ETA (D3) and per-entry failure attribution (needs a new core event; revisit if the owner asks).
- The duplicates screen's own illustration (`TODO.md` §1.5).
- Any change to what apply does, the journal, undo or resume.
- Landing changes (read-only reference).

## 13. Self-review

- **Placeholders**: none; every key, file and field is named. Speeds, windows and caps have values (5 s, 2 s, 3 sleeves, 25 MB/s, 22 rem columns, 8 rows, 10 s, 100 ms coalescing).
- **Contradictions checked**: D3 (ETA per game) vs §6.4 readout — the readout shows only the game's ETA; §6.5 ETAs are by items, where totals are known. D7 (no list for one entry) vs §6.3 — consistent. D10 (data first) vs §6.2 beats — beats are triggered by data deltas, capped. D11 (duplicates readout only) vs §6.5 — consistent. Reduced motion: tokens zero durations (brand §6) and §6.6 declares keyframes only under `no-preference`, so no loop exists under `reduce` (an infinite 0 ms animation would still run); stated.
- **Assumptions to verify in implementation**: phase A stages in ascending archive index for every backend, including external tools (D2, read in §3 and pinned by a test); `OrganizePhase` is not persisted anywhere (checked for the journal; re-check `jobs/`); `src-tauri/tests/contract.rs` builds `organize_progress_extract` from the Rust wire (update it with the sample).

## 14. Adversarial review (2026-10-26)

Checked against `TODO.md` §1, the workflow brief and the code on this branch. What was wrong or missing, and where it is fixed:

| # | Finding | Evidence | Fix |
|---|---|---|---|
| R1 | Game counter seeded from «the job view's finished extracts», which do not exist while applying; a `seenItems` set breaks when the screen mounts mid-apply. N used `action !== 'left'`, which counts review items the apply skips. | `OrganizeJobView` has only `progress` and an end `report`; `apply.rs` `applicable()` l. 425; plan order = apply order (l. 335, 419). | D6 from the plan order and `applicable()`; §8, §10. |
| R2 | `plan` can be `null` during an apply (`resumeJob` resets the store). | `organizeStore` `resumeJob` → `reset()`; `applyingText` already handles it. | §3, §5.2 `extractView(plan \| null)`, §10. |
| R3 | A paused beat (warning bar, frozen stage) and a «paused» mock check that can never be seen: a pause leaves `applying`, and the mock pauses only between items. | `accept()` maps `paused` to its own status; `mockOrganize.ts` `pauseAfter` check. | §3, §6.2, §9, §10. |
| R4 | A «cancelled» return-to-box beat that would be cut off: the result replaces the block when the job ends, and no cancelling state existed. | `cancel()` for `applying` only calls `organizeCancelApply()`. | D15, §6.2, §7 keys, tests. |
| R5 | Unthrottled per-entry ticks could flood IPC and React (hundreds of tiny entries). | `Run::entry` stages per entry; `Throttle` sends every edge. | D1 coalescing + all-staged edge, §5.1, Rust test. |
| R6 | `t()` cannot wrap numbers in mono spans and has no plurals; «Quedan unos {time}» produced «unos 1 minuto» / «unos 1 h 20 min». | `src/i18n/core.ts` `interpolate`. | `useRichT` (§5.2), D5 wordings, §7 keys. |
| R7 | `@container` layout: no Tailwind plugin, and Safari 16+ only while macOS 11 is supported. | `tauri.conf.json` `minimumSystemVersion: 11.0`; no `@container` in `src`. | §6.1 grid `auto-fit`. |
| R8 | `SystemIcon` inside an SVG stage: it can be an HTML span; `foreignObject` misbehaves in WebKit. The landing's folder art is an external `kit.svg` the app does not ship. | `SystemIcon.tsx` l. 57–125. | D9 overlay + copied path data. |
| R9 | `scrollIntoView` scrolls the page and dialog too. | — | §6.3 own `scrollTop`. |
| R10 | Reduced motion relied on removing keyframes under `reduce`; the landing opts in under `no-preference`, which also leaves no live animations. Forced colours not covered. | landing `site.css` l. 2166; `tokens.css` l. 589. | §6.6, `progress.css.test.ts`, E2E 5. |
| R11 | «Tamaño comprobado» undersold the no-CRC check (the written copy is re-read against the decoded CRC). | `extract.rs` l. 1121 `reread_crc`. | D8, §7. |
| R12 | CLI: `progress_due` checks the item counter, so extract byte ticks print every 100 ms or never; «its test» for `progress_line` did not exist. | `coverly-cli/src/organize.rs` l. 232, 1277. | D13, §11.1. |
| R13 | `pnpm tokens:check` claimed to cover token use in component CSS; it only checks generated files. | `scripts/brand/build_tokens.mjs --check`. | §11.2 new CSS test. |
| R14 | Scan ETA reason was wrong (its total is known up front). The fill panel is a dialog with a plain bar, not a side sheet with a slot bar. Scan announces every tick (`role="status"` on the wrapper). | `scan/mod.rs` l. 241; `JobsPanel.tsx` l. 247, 271; `ScanProgressView.tsx`. | D12, §5.2, §6.5. |
| R15 | Mock: no solid flag in `discFixture` and every entry is wanted; tick-counted pacing drifts in throttled tabs; the full pile at 25 MB/s is ≈ 90 s for E2E. | `discFixture.ts`; `host.tick` = `setTimeout`. | §9 time-based pacing, `?mockExtract=<MB/s>`, E2E 3. |
| R16 | Merge risk with the parallel §1.5 (duplicates) and locale changes. | Workflow brief. | D11 apply block only, last commit after merging `origin/main`; §7 contiguous key blocks. |
| R17 | Missing cases: resumed intact items emit no extract event; phase B/C on slow cards; mid-apply mount; Windows separators in the target path; RTL names; Radix overriding `aria-valuetext`. | `resume_extract`; `Progress` (Radix). | §5.1, §10, §6.4. |

Owner asks re-checked, all covered: illustrated stage (§6.2, landing vocabulary, `SystemIcon`), byte bar with speed and time left (§6.4, D3–D5), per-track list with ✓ as each check ends (§6.3, D1–D2, D8), reduced motion and tokens (§6.1, §6.6), «juego n de N» separate from «pista k de M» (D6–D7), the other progress screens (§6.5), a long multi-track mock for viewing and E2E (§9, D14), es/en (§7).
