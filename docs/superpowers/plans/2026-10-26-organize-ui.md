# Organise: the Extraction Progress Stage — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking. UI tasks (Group D and E) also REQUIRE frontend-design:frontend-design: follow `docs/08-brand/brand.md`, use the `src/components/ui` primitives, and critique every changed screen with screenshots (light and dark) before committing.

**Goal:** Replace the one-line extraction progress of «Organizar por sistema» («Extrayendo Next Tetris, The.zip: 0 de 1 archivos…») with an illustrated extraction stage in the landing's style: a storage box opens, its tracks leave as 3:4 sleeves along a dashed route and drop into the system's folder with its `SystemIcon`. Under it sit a byte-based slot bar with speed and time left, «Juego n de N» kept apart from «Pista k de M», and a track list that ticks ✓ as each file's CRC is checked. The scan, cover-fill and duplicates progress get the same readout. Reduced motion, forced colours, brand tokens, es/en. The browser mock simulates a long multi-track extraction so it can be watched and E2E-tested.

**Architecture:** The core counts staged entries in phase A (`ExtractTick { bytes, entries_done }`) and `OrganizePhase::Extract` carries `entries_done` / `entries_total`; the 100 ms coalescing stays, plus one all-staged edge. Tauri's throttle and wire, the contract sample and the CLI follow. In the webview, pure modules derive everything (`lib/rate.ts`, `lib/extractProgress.ts`), `i18n/rich.tsx` puts numbers in mono, and `components/progress/*` draws the stage (inline SVG + CSS keyframes opted in under `prefers-reduced-motion: no-preference`), the readout and the track list. `ApplyProgress` replaces the applying block of `OrganizeScreen`. The mock paces extraction by elapsed time (≈ 25 MB/s, `?mockExtract=`).

**Tech Stack:** Rust 1.99 (workspace MSRV 1.93), Tauri 2, React 19 + TypeScript + Tailwind 3.4 + zustand, Radix Progress, Vitest + Testing Library, Chrome DevTools MCP for E2E.

**Spec:** [`docs/superpowers/specs/2026-10-26-organize-ui.md`](../specs/2026-10-26-organize-ui.md) (all sections; decisions D1–D15 are binding unless a deviation below says otherwise). Builds on [`2026-10-23-organize-archives-and-discs.md`](2026-10-23-organize-archives-and-discs.md). Brand: [`docs/08-brand/brand.md`](../../08-brand/brand.md). Landing reference (read-only, never edit): `/Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-landing/site/index.html` (`#seguro`, hero), `assets/site.css` (`hr-march` l. 2265, `fx-fly` l. 1513, opt-in motion at l. 99, 791, 1579, 2166), `assets/art/kit.svg` (`#folder`, `#ftab`), `assets/shelf.js`, `assets/views.js`, `assets/motion.js`.

## Global Constraints

- **Worktree.** Work only in `/Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize-ui`, branch `feat/organize-extract-progress`, which tracks `origin/feat/organize-extract-progress`. Every shell command starts with `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize-ui &&` (agent shells reset their cwd). Never switch branches, never touch `main`.
- **Push after every commit (mandatory).** An earlier run lost a whole branch because it was never pushed. Every task ends with `git commit …`, then `git push`, then `git status -sb`; its first line must read `## feat/organize-extract-progress...origin/feat/organize-extract-progress` with **no** `[ahead N]`. Never batch commits; never end an agent with unpushed work. **Never force-push.**
- **Commit messages.** Conventional (`feat(core): …`, `test(core): …`, `feat(app): …`, `feat(cli): …`, `feat(ui): …`, `feat(mock): …`, `fix(…): …`, `docs: …`), granular, English. **No** `Co-Authored-By` trailer, no "Generated with", no mention of Claude or AI anywhere (owner rule; overrides any harness attribution).
- **Updating with main.** Other workflows change `locales/*.json`, `src/components/*` and `TODO.md` in parallel. To update: `git fetch origin && git merge origin/main` (never rebase), resolve conflicts keeping both sides' keys, run the gates, commit the merge, push. Do it at the start of Task 8 (locales), Task 17 (duplicates) and Task 18.
- **Do not edit** `docs/TRACKER.md` or `TODO.md` on this branch (they are updated on `main` after merge).
- **TDD.** Write the failing test, run it and watch it fail for the expected reason, implement, watch it pass. Each "Run" step names the exact command.
- **Gates (same as CI).** Before every commit run the gates the task touches; Task 18 runs all of them:
  - **G-RUST:** `cargo fmt --all --check && cargo clippy --workspace --all-targets --all-features -- -D warnings && scripts/dev/clippy-windows.sh && cargo test --workspace --no-fail-fast`.
  - **G-WEB:** `pnpm check-all && pnpm build` (`check-all` = `tokens:check`, `lint`, `format:check`, `typecheck`, `test`).
- **Windows hygiene.** Fixture mtimes are pinned; paths are compared by component (`Path::components`, `PathBuf` equality), never by slash text. Unix-only tests go in `#[cfg(unix)]` modules. No `std::os::unix` outside `cfg(unix)`.
- **Real data is read-only.** Never write to `~/Documents/ROMS`, `/Volumes/TinyExt/*` or `/Volumes/System`. Write tests use `tempfile` dirs. The one manual real-archive check (Task 18) works on copies made with `cp -RXp` under `/Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms-organize-ui/target/organize-ui-sample/`, deleted afterwards, with no `._*` files.
- **No UI strings in the core.** Every UI string lives in `locales/en/ui.json` **and** `locales/es/ui.json` with the same keys and placeholders (`src/i18n/parity.test.ts`). Spanish: sentence case, no exclamation marks, exact counts, numbers in mono.
- **Tokens only.** Colours come from `var(--cr-…)` roles or Tailwind `cr-*` classes; no hex, no `rgb()` literals, no blend modes. Durations and easings from `--cr-duration-*` / `--cr-ease-*` (zeroed under reduced motion by `src/styles/tokens.css` l. 589). Never `--cr-ease-spring` on apply.
- **Servers and E2E.** `pnpm dev --port 1462 --strictPort` (mock backend). Stop it **by port only**: `lsof -ti tcp:1462 -sTCP:LISTEN | xargs -r kill`. E2E: Chrome DevTools MCP `new_page` with `isolatedContext: "organize-ui"`; fall back to Playwright MCP; if both profiles are locked, `pnpm add -D` is **not** used — install `playwright-core` in `/private/tmp/claude-501/-Users-miguelangelesparzacalero-Documents-GitHub/35b7c020-f2ae-412f-b757-99c24a9a498b/scratchpad/organize-ui` (`npm init -y && npm i playwright-core`) and drive Chrome for Testing from there.
- **Secrets** (minisign key, `~/.coverly-release`, `.env*`, `.vercel`) are never read or printed.

## Deviations from the spec (decided in this plan)

- **AP-1: CLI wording.** The CLI has no per-game index without re-deriving the plan's applicable order, so it prints `extract item 19/40 files 4/12` (plan item, then entries) instead of D13's `extract 2/3 tracks 4/12`. Same pacing rules as D13.
- **AP-2: System label on the folder front.** At the stage's scale the landing's `#ftab` is 9 user units tall, too small for a 20 px icon. The `SystemIcon` and the `k/M` count sit on a Manila label plate on the folder's front; the tab stays Manila and flashes on each landing. Same roles as §6.1.
- **AP-3: Scan sentence.** To keep the scan's `role="status"` quiet (§6.5), the sentence is `start.scanningStart` («Leyendo la carpeta…») for the whole scan; the folder counter moves to the readout as `start.progress.folders`. `start.scanning` is removed if nothing else uses it.
- **AP-4: Queue reducer is pure.** The sleeve queue of D10 is a pure reducer (`stageReducer`, exported and unit-tested) plus a 1.5 s safety timer that lands a sleeve whose `animationend` never arrives (throttled webview).

## Review focus

1. **Truthful numbers.** `entries_done` is cumulative, monotone, ends at `entries_total`, and the ✓ rows are the first `k` by archive index (D1, D2). The game counter is derived from plan order and `applicable()` (D6), never from events seen. No ETA when the data cannot support one (D3, D12).
2. **Motion discipline.** Under `prefers-reduced-motion: reduce`, `document.getAnimations()` on the stage is empty; at most 3 sleeves fly; only the current sleeve's march loops (§6.1, §6.6).
3. **IPC volume.** The core coalesces to ≤ 10 events/s even with hundreds of tiny entries; the Tauri throttle lets entry changes through without flooding (D1, R5).
4. **Merge safety.** Locale keys in one contiguous block per namespace; duplicates screen touched in one hunk, last, right after merging `origin/main` (D11, R16).

---

## File structure

```
crates/coverly-core/src/organize/extract.rs     ExtractTick; progress gets (bytes, entries_done)
crates/coverly-core/src/organize/plan.rs        OrganizePhase::Extract + entries_done, entries_total
crates/coverly-core/src/organize/apply.rs       report(bytes, entries); all-staged edge
crates/coverly-core/tests/organize_extract_apply.rs   order, edge, coalescing tests
crates/coverly-core/tests/organize_extract_resume.rs  adopted entries counted
crates/coverly-core/tests/archive_external.rs   real 7-Zip streams ascending (skips without one)
crates/coverly-cli/src/organize.rs              progress_text, ProgressGate
src-tauri/src/organize/mod.rs                   Throttle: entries_done change is an edge
src-tauri/src/organize/view.rs                  ProgressWire + entries_done, entries_total
src-tauri/tests/contract.rs, organize_state.rs  new fields
src/ipc/types.ts, src/ipc/contract.sample.json, src/ipc/contract.test.ts
src/ipc/mock/mockOrganize.ts (+ test)           per-entry, time-paced extraction, readMockExtract
src/ipc/mock/mockBackend.ts, src/ipc/backend.ts extractRate option, ?mockExtract
src/lib/rate.ts (+ test)                        samples, rate, ETA keys, formatRate
src/lib/extractProgress.ts (+ test)             extractView, gameIndex, targetDir, middleTruncate
src/i18n/rich.tsx (+ test)                      useRichT: values in mono spans
src/lib/useReducedMotion.ts (+ test)            matchMedia subscription
src/hooks/useRate.ts (+ test)                   rate + ETA, ≤ 2 renders/s
src/components/progress/progress.css (+ test)   tokens only, opt-in keyframes, forced colours
src/components/progress/stageArt.ts             shared path data and geometry
src/components/progress/WorkStage.tsx           SVG frame + HTML overlay
src/components/progress/ExtractStage.tsx (+ test)  box, lid, sleeves, queue reducer
src/components/progress/ScanStage.tsx (+ test)  folders under a reading line, slot row
src/components/progress/TrackList.tsx (+ test)
src/components/progress/WorkReadout.tsx (+ test)
src/components/organize/ApplyProgress.tsx (+ test)
src/screens/OrganizeScreen.tsx (+ test)         uses ApplyProgress; old helpers removed
src/components/ScanProgressView.tsx (+ test)
src/components/fill/JobsPanel.tsx (+ test)
src/screens/DuplicatesScreen.tsx (+ test)
locales/{es,en}/ui.json                         keys of spec §7
```

---

## Group A — Core, wire and CLI

### Task 1: Core: phase A reports staged entries (`ExtractTick`)

**Files:**
- Modify: `crates/coverly-core/src/organize/extract.rs` (l. 90–106 `ExtractRequest`, `Run::entry` l. 1021–1136, `Run::sink_entry` l. 1151–1221)
- Modify: `crates/coverly-core/src/organize/apply.rs` (l. 1700–1708, `on_bytes`)
- Modify: `crates/coverly-core/tests/organize_extract_apply.rs` (closures at l. 122, 397, 520, 1171)
- Test: `crates/coverly-core/tests/organize_extract_apply.rs`, `crates/coverly-core/tests/archive_external.rs`

- [ ] **Step 1: Write the failing tests.** In `organize_extract_apply.rs` (top level, next to `cancel_inside_64mib_entry_returns_within_16mib`):

```rust
/// D2: at every tick, the staged entries are the first `entries_done` of
/// the plan entries by archive index, whatever order the plan lists them.
#[test]
fn staged_entries_are_a_prefix_by_archive_index() {
    let env = env();
    let files: Vec<(String, Vec<u8>)> = (1..=5)
        .map(|i| (format!("Game (Track {i}).bin"), vec![i as u8; 300_000 * i]))
        .collect();
    let refs: Vec<(&str, &[u8])> = files.iter().map(|(n, d)| (n.as_str(), d.as_slice())).collect();
    let (archive, mut reader) = write_archive(
        &env,
        "Game.7z",
        &sevenz(&refs, SevenZShape::solid_lzma2_sheets_last()),
    );
    let mut entries = plan_entries(reader.as_ref(), "PS/Game");
    entries.reverse(); // the plan lists them out of archive order
    let mut writer = env.writer();
    let cancel = Arc::new(AtomicBool::new(false));
    let ticks: Mutex<Vec<ExtractTick>> = Mutex::default();
    let progress = |t: ExtractTick| ticks.lock().unwrap().push(t);
    let done = extract_to_temps(
        &mut writer,
        reader.as_mut(),
        &ExtractRequest { item: 0, archive: &archive, entries: &entries, cancel: &cancel, progress: &progress },
    )
    .unwrap();
    let ticks = ticks.into_inner().unwrap();
    let staged: Vec<usize> = done.staged.iter().map(|s| s.entry.index).collect();
    let mut by_index: Vec<usize> = entries.iter().map(|e| e.index).collect();
    by_index.sort_unstable();
    assert_eq!(staged, by_index, "staged in ascending archive index");
    assert_eq!(ticks.first().unwrap().entries_done, 0);
    assert_eq!(ticks.last().unwrap().entries_done, entries.len());
    assert!(ticks.windows(2).all(|w| w[0].entries_done <= w[1].entries_done));
    assert!(ticks.windows(2).all(|w| w[0].bytes <= w[1].bytes));
    // Every count from 1 to 5 is reported right after its entry is staged.
    for k in 1..=entries.len() {
        assert!(ticks.iter().any(|t| t.entries_done == k), "no tick for {k}");
    }
}
```

In `archive_external.rs`, inside the module that holds `real_seven_zip_reads_by_name_and_stream`, add `real_seven_zip_streams_in_ascending_index` with the same discovery/skip prologue (`eprintln!("skipped: no 7-Zip found"); return;`), an archive of four files made with `-ms=on` (solid), `wanted = vec![3, 0, 2, 1]` mapped through `index_of`, and a `stream_entries` sink that pushes `e.index`; assert the pushed list is sorted ascending.

- [ ] **Step 2: Run, watch it fail to compile** (`ExtractTick` does not exist):
  `cargo test -p coverly-core --test organize_extract_apply staged_entries_are_a_prefix 2>&1 | tail -5`

- [ ] **Step 3: Implement.** In `extract.rs`:

```rust
/// What phase A has done for one item so far.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExtractTick {
    /// Bytes decoded (out of [`decode_total`]).
    pub bytes: u64,
    /// Entries staged: written, re-read and CRC-equal (adopted ones
    /// included). Always the first `entries_done` plan entries by archive
    /// index, since every backend streams the wanted entries ascending.
    pub entries_done: usize,
}
```

Change `ExtractRequest::progress` to `pub progress: &'a dyn Fn(ExtractTick),` (update its doc). In both `Run::entry` and `Run::sink_entry` replace `(self.req.progress)(before.saturating_add(in_entry));` with

```rust
(self.req.progress)(ExtractTick {
    bytes: before.saturating_add(in_entry),
    entries_done: self.staged.len(),
});
```

and right after each `self.staged.push(StagedEntry { … });` add

```rust
(self.req.progress)(ExtractTick {
    bytes: before.saturating_add(in_entry),
    entries_done: self.staged.len(),
});
```

(`before` and `in_entry` are `u64`, still in scope.) Export `ExtractTick` wherever `ExtractRequest` is re-exported (`organize/mod.rs`). In `apply.rs` keep the behaviour for now: `let on_bytes = |tick: ExtractTick| { if last.get().elapsed() >= PROGRESS_EVERY { last.set(Instant::now()); report(tick.bytes); } };`. Update the test closures: `&|_| {}` stays valid; l. 397 becomes `let progress = |t: ExtractTick| { let bytes = t.bytes; … }`; l. 520 the same.

- [ ] **Step 4: Run** `cargo test -p coverly-core --test organize_extract_apply && cargo test -p coverly-core --test archive_external real_seven_zip` — all pass (the external test prints `skipped` without 7-Zip).
- [ ] **Step 5: Gates** G-RUST.
- [ ] **Step 6: Commit and push.**
  `git add -A && git commit -m "feat(core): report staged entries during extraction phase A" && git push && git status -sb`

### Task 2: Core: `OrganizePhase::Extract` carries entry counts

**Files:**
- Modify: `crates/coverly-core/src/organize/plan.rs` (l. 630–638)
- Modify: `crates/coverly-core/src/organize/apply.rs` (l. 1686–1737)
- Modify (compile only, `..` or new fields): `crates/coverly-cli/src/organize.rs` (l. 256), `src-tauri/src/organize/mod.rs` (l. 412 and tests l. 496+), `src-tauri/src/organize/view.rs` (l. 130–134, 231), `src-tauri/tests/contract.rs` (l. 575), `src-tauri/tests/organize_state.rs` (l. 1058)
- Test: `crates/coverly-core/tests/organize_extract_apply.rs`, `crates/coverly-core/tests/organize_extract_resume.rs`

- [ ] **Step 1: Failing tests.**
  1. Extend `apply_phases::progress_is_byte_based_over_folder_unpack_size` (l. 1589): destructure `entries_done, entries_total` too, collect `(bytes_done, bytes_total, entries_done, entries_total)`, and assert: every `entries_total == entries.len()` (the playlist is not counted), first `entries_done == 0`, last `== entries_total`, monotone, and an event with `entries_done == entries_total && bytes_done < bytes_total` **or** one before the final event with `entries_done == entries_total` exists (the all-staged edge).
  2. New in `apply_phases`: `progress_coalesces_two_hundred_tiny_entries` — a stored zip of 200 entries of 1 KiB (`zip_with` with `ZipMethod::Stored`), one item, `Instant` taken before and after `apply_full`; assert `events <= elapsed_ms / 100 + 3` and that the last event has `entries_done == 200`.
  3. In `organize_extract_resume.rs`: `resume_reports_adopted_entries_in_the_count` — reuse the set-up of `crash_after_5_of_12_resume_finishes_no_double_files` but give `resume` a progress closure that records `entries_done` (write a local `resume_with(env, job, trash, progress)` beside `resume`); assert the last event has `entries_done == 12` and the counts are monotone.
- [ ] **Step 2: Run** `cargo test -p coverly-core --test organize_extract_apply progress_ 2>&1 | tail -5` — fails to compile (no such fields).
- [ ] **Step 3: Implement.** `plan.rs`:

```rust
Extract {
    item: usize,
    bytes_done: u64,
    bytes_total: u64,
    /// Entries of the item staged so far (written, re-read, CRC equal),
    /// in ascending archive index; adopted entries count.
    entries_done: usize,
    /// The item's entries (`Transfer::Extract::entries.len()`; a
    /// generated playlist is not one).
    entries_total: usize,
},
```

`apply.rs` (`Applier::extract`):

```rust
let entries_total = entries.len();
let report = |bytes_done: u64, entries_done: usize| {
    progress(OrganizeProgress {
        phase: OrganizePhase::Extract { item: item_id, bytes_done, bytes_total, entries_done, entries_total },
        done,
        total,
    })
};
report(0, 0);
let last = std::cell::Cell::new(Instant::now());
let all_staged = std::cell::Cell::new(false);
let on_tick = |tick: ExtractTick| {
    // The count rides the 100 ms cadence; only the moment every entry is
    // staged goes out at once (phase B starts and no tick follows it).
    let edge = tick.entries_done == entries_total && !all_staged.replace(true);
    if edge || last.get().elapsed() >= PROGRESS_EVERY {
        last.set(Instant::now());
        report(tick.bytes, tick.entries_done);
    }
};
```

pass `progress: &on_tick`, and after phase C `report(bytes_total, entries_total);`. Fix every other site: CLI `OrganizePhase::Extract { .. }` already compiles; `view.rs` and `mod.rs` patterns add `..` for now (Task 3 uses the fields); test constructors add `entries_done: 0, entries_total: 1` (contract) or the right values.
- [ ] **Step 4: Run** `cargo test --workspace --no-fail-fast 2>&1 | grep -E "test result|FAILED|panicked" | tail -20` — all pass.
- [ ] **Step 5: Gates** G-RUST.
- [ ] **Step 6: Commit and push.** `git commit -am "feat(core): carry staged entry counts in extract progress" && git push && git status -sb`

### Task 3: Tauri: wire fields and an entry-change edge in the throttle

**Files:**
- Modify: `src-tauri/src/organize/view.rs` (`ProgressWire`, `From`, test l. 225–250)
- Modify: `src-tauri/src/organize/mod.rs` (`Throttle::progress` l. 402–436, tests)
- Modify: `src-tauri/tests/contract.rs` (l. 575), `src/ipc/contract.sample.json` (l. 1082), `src/ipc/types.ts` (l. 1033–1046), `src/ipc/contract.test.ts` (`EXTRACT_PROGRESS_KEYS`, l. 929–937)

- [ ] **Step 1: Failing tests.**
  - `mod.rs` tests: `an_entry_change_goes_out_within_the_window` — send `(bytes 0, e 0)`, then within the same millisecond `(bytes 10, e 1)`, `(bytes 11, e 1)`; assert three sends would be wrong: exactly the first two are sent (the byte-only third is dropped).
  - `view.rs` wire test: the extract JSON now has `"entries_done": 4, "entries_total": 12`; add a check that an `Apply` event's JSON has neither key.
  - `contract.test.ts`: the sample assertion grows `progress.entries_done, progress.entries_total` → `[3, 104_857_600, 405_749_493, 4, 12]`.
- [ ] **Step 2: Run** `cargo test -p coverly-roms --lib organize 2>&1 | tail -5` (fails), `pnpm vitest run src/ipc/contract.test.ts` (fails).
- [ ] **Step 3: Implement.**

```rust
// view.rs ProgressWire
#[serde(skip_serializing_if = "Option::is_none")]
entries_done: Option<usize>,
#[serde(skip_serializing_if = "Option::is_none")]
entries_total: Option<usize>,
// From: OrganizePhase::Extract { item, bytes_done, bytes_total, entries_done, entries_total }
//   => (PhaseName::Extract, Some((item, bytes_done, bytes_total, entries_done, entries_total)))
```

```rust
// mod.rs Throttle::progress, inside `Some((phase, at))`
let extract_edge = match (phase, p.phase) {
    (_, OrganizePhase::Extract { bytes_done, bytes_total, .. })
        if bytes_done == 0 || bytes_done >= bytes_total => true,
    // A ✓ is never hidden behind a long solid-block stall; the core
    // already coalesces these to ≤ 10/s.
    (
        OrganizePhase::Extract { entries_done: a, .. },
        OrganizePhase::Extract { entries_done: b, .. },
    ) => a != b,
    _ => false,
};
```

Sample (`contract.sample.json`): add `"entries_done": 4, "entries_total": 12` to `organize_progress_extract` (keys sorted as the file has them) and mirror it in `contract.rs`. `types.ts` extract branch gains:

```ts
/** Entries staged so far, first in archive order (absent on an older wire). */
entries_done?: number;
/** The item's entries (the generated playlist is not one). */
entries_total?: number;
```

and `EXTRACT_PROGRESS_KEYS` lists both.
- [ ] **Step 4: Run** `cargo test -p coverly-roms && pnpm vitest run src/ipc` — pass.
- [ ] **Step 5: Gates** G-RUST and G-WEB.
- [ ] **Step 6: Commit and push.** `git commit -am "feat(app): send extract entry counts and never throttle a new entry away" && git push && git status -sb`

### Task 4: CLI: one extract line per entry change, at most one per second

**Files:** Modify `crates/coverly-cli/src/organize.rs` (l. 230–262, 324–329, test l. 1277).

- [ ] **Step 1: Failing tests** (in the existing `tests` module):

```rust
#[test]
fn progress_text_names_the_item_and_its_files() {
    let p = OrganizeProgress {
        phase: OrganizePhase::Extract { item: 3, bytes_done: 5, bytes_total: 10, entries_done: 4, entries_total: 12 },
        done: 18,
        total: 40,
    };
    assert_eq!(progress_text(p).as_deref(), Some("extract item 19/40 files 4/12"));
    let walk = OrganizeProgress { phase: OrganizePhase::Walk, done: 0, total: 0 };
    assert_eq!(progress_text(walk).as_deref(), Some("walk"));
}

#[test]
fn extract_lines_follow_entries_not_bytes() {
    let gate = ProgressGate::default();
    let t0 = Instant::now();
    let x = |b, e| OrganizeProgress {
        phase: OrganizePhase::Extract { item: 1, bytes_done: b, bytes_total: 100, entries_done: e, entries_total: 3 },
        done: 0,
        total: 1,
    };
    assert!(gate.due(x(0, 0), t0), "first event");
    assert!(!gate.due(x(10, 0), t0 + Duration::from_millis(100)), "byte tick");
    assert!(gate.due(x(20, 1), t0 + Duration::from_millis(1100)), "entry change");
    assert!(!gate.due(x(30, 2), t0 + Duration::from_millis(1200)), "within a second");
    assert!(gate.due(x(100, 3), t0 + Duration::from_millis(1300)), "last event");
}
```

Keep `progress_is_printed_sparingly`, switching it to `ProgressGate::default().due(p, Instant::now())`.
- [ ] **Step 2: Run** `cargo test -p coverly-cli progress 2>&1 | tail -5` — fails.
- [ ] **Step 3: Implement.** Replace `progress_due`/`progress_line` with:

```rust
/// Decides which progress events get a terminal line: phase starts and
/// ends and every 25th file; for an extraction, its first and last event
/// and each change of its staged entries, at most once a second.
#[derive(Default)]
struct ProgressGate {
    last: Mutex<Option<(usize, usize, Instant)>>,
}

impl ProgressGate {
    fn due(&self, p: OrganizeProgress, now: Instant) -> bool {
        let OrganizePhase::Extract { item, bytes_done, bytes_total, entries_done, .. } = p.phase else {
            return p.done == 0 || p.done == p.total || p.done.is_multiple_of(25);
        };
        let mut last = self.last.lock().unwrap_or_else(PoisonError::into_inner);
        let edge = bytes_done == 0 || bytes_done >= bytes_total;
        let changed = match *last {
            Some((i, e, at)) => i != item || (e != entries_done && now.duration_since(at) >= Duration::from_secs(1)),
            None => true,
        };
        if edge || changed {
            *last = Some((item, entries_done, now));
            return true;
        }
        false
    }
}

/// `identify 40/631`, or `extract item 19/40 files 4/12`.
fn progress_text(p: OrganizeProgress) -> Option<String> {
    let phase = match p.phase {
        OrganizePhase::Walk => "walk",
        OrganizePhase::Identify => "identify",
        OrganizePhase::Dats => "catalogues",
        OrganizePhase::Hash => "hash",
        OrganizePhase::Apply => "apply",
        OrganizePhase::Extract { entries_done, entries_total, .. } => {
            return Some(format!(
                "extract item {}/{} files {entries_done}/{entries_total}",
                p.done + 1,
                p.total
            ));
        }
    };
    Some(if p.total == 0 { phase.to_string() } else { format!("{phase} {}/{}", p.done, p.total) })
}
```

and in `run`: `let gate = ProgressGate::default(); let progress = move |p| if tty && gate.due(p, Instant::now()) { if let Some(line) = progress_text(p) { eprintln!("{line}"); } };` (the closure must stay `Sync`; `Mutex` keeps it so).
- [ ] **Step 4: Run** `cargo test -p coverly-cli` — pass.
- [ ] **Step 5: Gates** G-RUST.
- [ ] **Step 6: Commit and push.** `git commit -am "feat(cli): print extraction progress per staged entry" && git push && git status -sb`

---

## Group B — Mock

### Task 5: Mock: per-entry, time-paced extraction (`?mockExtract`)

**Files:**
- Modify: `src/ipc/mock/mockOrganize.ts` (constants l. 115, options l. 587, apply loop l. 1302–1324; new `readMockExtract`)
- Modify: `src/ipc/mock/mockBackend.ts` (`MockOptions`, `createMockOrganize` call l. 306), `src/ipc/backend.ts` (l. 297)
- Test: `src/ipc/mock/mockOrganize.test.ts`

- [ ] **Step 1: Failing tests** in `mockOrganize.test.ts`:

```ts
describe('extract pacing', () => {
  it('emits monotone entries_done ending at entries_total, first and last edges', async () => {
    const b = createMockBackend({ delayMs: 0, storage: null, folder: MOCK_DOWNLOADS });
    const events: OrganizeProgressEvent[] = [];
    await b.onOrganizeProgress((e) => events.push(e));
    // plan + apply MOCK_DOWNLOADS as the existing apply tests do (helper `planAndApply`)
    await planAndApply(b, MOCK_DOWNLOADS);
    const byItem = new Map<number, OrganizeProgressEvent[]>();
    for (const e of events) if (e.phase === 'extract') byItem.set(e.item, [...(byItem.get(e.item) ?? []), e]);
    expect(byItem.size).toBe(3);
    for (const list of byItem.values()) {
      const counts = list.map((e) => (e.phase === 'extract' ? e.entries_done! : -1));
      const total = list[0]!.phase === 'extract' ? list[0]!.entries_total! : 0;
      expect(counts[0]).toBe(0);
      expect(counts.at(-1)).toBe(total);
      expect(counts.every((c, i) => i === 0 || c >= counts[i - 1]!)).toBe(true);
      for (let k = 1; k <= total; k += 1) expect(counts).toContain(k); // fast mode: one event per entry
    }
  });

  it.each([
    ['', 25],
    ['?mockExtract=slow', 25],
    ['?mockExtract=fast', 'fast'],
    ['?mockExtract=80', 80],
    ['?mockExtract=-3', 25],
    ['?mockExtract=abc', 25],
  ])('reads %s', (search, want) => {
    window.history.replaceState(null, '', `/${search}`);
    expect(readMockExtract()).toEqual(want);
    window.history.replaceState(null, '', '/');
  });

  it('a rate run paces by elapsed time and cancels mid-item', async () => {
    // extractRate 2000 MB/s with delayMs 1: Tetris Plus takes a few ticks;
    // cancel after the first entry of the first extract item, then assert
    // the job view says cancelled and no extracted file of that game exists.
  });
});
```

(Write `planAndApply` from the steps the existing apply tests in this file already use; reuse, do not duplicate, if such a helper exists.)
- [ ] **Step 2: Run** `pnpm vitest run src/ipc/mock/mockOrganize.test.ts` — fails.
- [ ] **Step 3: Implement.**

```ts
/** Simulated extraction speed: 'fast' (old 128 MB chunks, one event per
 * entry; tests) or megabytes per second (browser default 25, D14). */
export type MockExtractRate = 'fast' | number;
export const MOCK_EXTRACT_SLOW = 25;
/** Phase B (sheets, names) after the last entry, rate mode only. */
const CHECK_PAUSE_MS = 600;

/** `?mockExtract=fast|slow|<MB/s>`; anything else is the slow default. */
export function readMockExtract(): MockExtractRate {
  try {
    const v = new URLSearchParams(window.location.search).get('mockExtract');
    if (v === 'fast') return 'fast';
    const n = Number(v);
    return v && v !== 'slow' && Number.isFinite(n) && n > 0 ? n : MOCK_EXTRACT_SLOW;
  } catch {
    return MOCK_EXTRACT_SLOW;
  }
}

/** Seeded PRNG (mulberry32): reproducible ±20 % jitter. */
function jitter(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return 0.8 + 0.4 * (((t ^ (t >>> 14)) >>> 0) / 4294967296);
  };
}
```

`MockOrganizeOptions` gains `extractRate?: MockExtractRate` (default `'fast'`). Replace the extract block of the apply loop with a helper `extractItem(at, t, own)` that walks `[...t.entries].sort((a, b) => a.index - b.index)`:

```ts
const entriesTotal = t.entries.length;
const send = (bytes: number, entries: number) =>
  emitProgress({ ...at, bytes_done: Math.min(bytes, t.bytes), entries_done: entries, entries_total: entriesTotal });
send(0, 0);
let bytes = 0;
const rand = jitter(item.id + 1);
for (const [k, entry] of sorted.entries()) {
  const end = bytes + entry.size;
  if (rate === 'fast') {
    for (let b = bytes + EXTRACT_CHUNK; b < end; b += EXTRACT_CHUNK) {
      await host.tick();
      if (own.cancel) return false;
      send(b, k);
    }
  } else {
    let last = performance.now();
    while (bytes < end) {
      await host.tick();
      if (own.cancel) return false;
      const now = performance.now();
      bytes = Math.min(end, bytes + (rate * 1e6 * (now - last) / 1000) * rand());
      last = now;
      if (bytes < end) send(bytes, k);
    }
  }
  bytes = end;
  await host.tick();
  if (own.cancel) return false;
  send(bytes, k + 1);
}
if (rate !== 'fast') await new Promise((r) => setTimeout(r, CHECK_PAUSE_MS));
send(t.bytes, entriesTotal);
return true;
```

(`t.bytes` can exceed the sum of entry sizes; the last event fills the bar, as in Rust.) `mockBackend.ts`: `MockOptions.extractRate?: MockExtractRate` passed through; `backend.ts`: `createMockBackend({ extractRate: readMockExtract() })` (import `readMockExtract` from the mock chunk only, inside `createBackend`). Update the file's header comment (l. 27–33).
- [ ] **Step 4: Run** `pnpm vitest run src/ipc/mock src/screens/OrganizeScreen.test.tsx` — pass (screen tests use the fast default).
- [ ] **Step 5: Gates** G-WEB.
- [ ] **Step 6: Commit and push.** `git commit -am "feat(mock): simulate a long per-track extraction paced by time" && git push && git status -sb`

---

## Group C — Pure logic and hooks

### Task 6: `lib/rate.ts`: samples, rate, time-left wording

**Files:** Create `src/lib/rate.ts`, `src/lib/rate.test.ts`.

- [ ] **Step 1: Failing tests** (`rate.test.ts`):

```ts
import { describe, expect, it } from 'vitest';
import { etaKey, etaSeconds, formatRate, initialRate, pushSample, rateOf } from './rate';

const feed = (pts: [number, number][], key: number | string = 1) =>
  pts.reduce((s, [t, v]) => pushSample(s, key, v, t), initialRate());

describe('rate', () => {
  it('needs 2 s of samples', () => {
    expect(rateOf(feed([[0, 0], [1500, 15]]), 1500)).toBeNull();
    expect(rateOf(feed([[0, 0], [2000, 20]]), 2000)).toBeCloseTo(10);
  });
  it('keeps a 5 s window', () => {
    const s = feed([[0, 0], [1000, 100], [6000, 150], [8000, 170]]);
    expect(rateOf(s, 8000)).toBeCloseTo((170 - 100) / 7, 0); // samples older than 5 s dropped, newest anchor kept
  });
  it('resets samples when bytes go backwards and on a new item, keeping the last rate', () => {
    let s = feed([[0, 0], [3000, 30]]);
    s = pushSample(s, 2, 0, 3100);
    expect(rateOf(s, 3100)).toBeCloseTo(10);
    s = pushSample(s, 2, 5, 3200);
    s = pushSample(s, 2, 1, 3300);
    expect(s.samples).toHaveLength(1);
  });
  it('falls back to measuring after a 10 s stall', () => {
    const s = feed([[0, 0], [3000, 30], [13_500, 30]]);
    expect(rateOf(s, 13_500)).toBeNull();
  });
  it.each([
    [59, 'organize.progress.leftUnderMinute', undefined],
    [60, 'organize.progress.leftUnderMinute', undefined],
    [61, 'organize.progress.leftMinutes', { n: 2 }],
    [119, 'organize.progress.leftMinutes', { n: 2 }],
    [59 * 60, 'organize.progress.leftMinutes', { n: 59 }],
    [3600, 'organize.progress.leftOverHour', undefined],
    [3660, 'organize.progress.leftOverHour', undefined],
    [2 * 3600 + 59 * 60, 'organize.progress.leftOverHours', { n: 2 }],
  ])('eta %i s', (s, key, vars) => expect(etaKey(s)).toEqual({ key, vars }));
  it('etaSeconds is null without a rate', () => {
    expect(etaSeconds(100, null)).toBeNull();
    expect(etaSeconds(100, 10)).toBe(10);
  });
  it('formats decimal rates per locale', () => {
    expect(formatRate('es', 24_100_000)).toBe('24,1 MB');
    expect(formatRate('en', 24_100_000)).toBe('24.1 MB');
  });
});
```

Also assert, over `etaKey` for every second in 0…4 h rendered through `translate('es'|'en', key, vars)`, that no output matches `/unos 1 |1 horas|about 1 minutes|1 hours/`.
- [ ] **Step 2: Run** `pnpm vitest run src/lib/rate.test.ts` — fails (no module).
- [ ] **Step 3: Implement** `rate.ts`:

```ts
import { formatDecimalSize } from './format';

export const WINDOW_MS = 5_000;
export const MIN_SPAN_MS = 2_000;
export const STALL_MS = 10_000;

export interface Sample { t: number; v: number }
export interface RateState {
  key: number | string | null;
  samples: Sample[];
  /** Carried across items of one run (D4). */
  lastRate: number | null;
  /** When the value last grew. */
  movedAt: number | null;
}

export const initialRate = (): RateState => ({ key: null, samples: [], lastRate: null, movedAt: null });

function spanRate(samples: Sample[]): number | null {
  const first = samples[0];
  const last = samples.at(-1);
  if (!first || !last || last.t - first.t < MIN_SPAN_MS) return null;
  return ((last.v - first.v) * 1000) / (last.t - first.t);
}

/** Adds a sample `v` at monotonic time `t` (ms) for `key` (an item, a job). */
export function pushSample(s: RateState, key: number | string, v: number, t: number): RateState {
  const prev = s.samples.at(-1);
  if (s.key !== key || !prev || v < prev.v) {
    return { key, samples: [{ t, v }], lastRate: s.lastRate, movedAt: t };
  }
  const kept = s.samples.filter((x) => x.t >= t - WINDOW_MS);
  // Keep the newest sample older than the window as the anchor.
  const anchor = s.samples.filter((x) => x.t < t - WINDOW_MS).at(-1);
  const samples = [...(anchor ? [anchor] : []), ...kept, { t, v }];
  const rate = spanRate(samples);
  return { key, samples, lastRate: rate ?? s.lastRate, movedAt: v > prev.v ? t : s.movedAt };
}

/** Units per second, or null while measuring or stalled. */
export function rateOf(s: RateState, now: number): number | null {
  if (s.movedAt !== null && now - s.movedAt >= STALL_MS) return null;
  return spanRate(s.samples) ?? s.lastRate;
}

export const etaSeconds = (rest: number, rate: number | null): number | null =>
  rate && rate > 0 ? Math.max(0, rest) / rate : null;

/** D5: one key per wording, never «unos 1 minuto». */
export function etaKey(seconds: number): { key: string; vars: { n: number } | undefined } {
  if (seconds <= 60) return { key: 'organize.progress.leftUnderMinute', vars: undefined };
  if (seconds < 3600) return { key: 'organize.progress.leftMinutes', vars: { n: Math.ceil(seconds / 60) } };
  const h = Math.floor(seconds / 3600);
  return h === 1
    ? { key: 'organize.progress.leftOverHour', vars: undefined }
    : { key: 'organize.progress.leftOverHours', vars: { n: h } };
}

export const formatRate = (locale: string, perSecond: number) => formatDecimalSize(perSecond, locale);
```

(Adjust the window test's expected value to what the anchor rule yields; the rule — newest sample older than the window kept as anchor — is the contract.)
- [ ] **Step 4: Run** the test — pass.
- [ ] **Step 5: Gates** G-WEB (the locale keys used by the wording test come in Task 8; until then the wording test uses `etaKey` only and the translate-based sweep is added in Task 8, Step 3).
- [ ] **Step 6: Commit and push.** `git add -A && git commit -m "feat(ui): measure transfer rate and word the time left" && git push && git status -sb`

### Task 7: `lib/extractProgress.ts`: what the extraction screen shows

**Files:** Create `src/lib/extractProgress.ts`, `src/lib/extractProgress.test.ts`.

- [ ] **Step 1: Failing tests.** Build plans with the existing helpers in `src/test/organizeFixtures.ts` (or the mock's `MOCK_DOWNLOADS` plan through `createMockBackend({ delayMs: 0, storage: null })`). Cases (each an `it`):
  - rows sorted by `index` when `transfer.entries` is shuffled; states for `entries_done` 0 → first row `current`, rest `waiting`; mid (4 of 12) → 4 `done`, row 5 `current`; all → all `done` and `phase: 'checking'` (when `bytes_done < bytes_total`), `phase: 'decoding'` otherwise mid-item.
  - `noun: 'track'` when `item.disc` is set, `'file'` otherwise; a single-entry item → `entries: null`, `rows: []`.
  - game counter: 3 extract items among 40 with moves interleaved, one review item and one set-aside item excluded → the 2nd extract item reads `{ n: 2, total: 3 }`, also when it is the first event the view ever sees.
  - `plan === null` → `name: '#7'`, `systemId: null`, `game: null`, `rows: []`, `entries` from the event.
  - event without `entries_*` → `entries: null`, `rows: []`.
  - `targetDir`: root `C:\Roms`, target `PS/Game/Game (Track 1).bin` → `C:\Roms\PS\Game`; root `/Volumes/SD/Roms` → `/Volumes/SD/Roms/PS/Game`.
  - `middleTruncate('Tetris Plus (Japan) (Track 18).bin', 24)` keeps the end `(Track 18).bin` and contains `…`; short names untouched.
- [ ] **Step 2: Run** `pnpm vitest run src/lib/extractProgress.test.ts` — fails.
- [ ] **Step 3: Implement.**

```ts
import type { ExtractEntry, OrganizeItem, OrganizePlan, OrganizeProgressEvent } from '@/ipc/types';

export type RowState = 'done' | 'current' | 'waiting';
export interface TrackRow { index: number; name: string; full: string; size: number; checked: 'crc' | 'size'; state: RowState }
export interface ExtractView {
  itemId: number;
  name: string;
  systemId: string | null;
  targetDir: string | null;
  noun: 'track' | 'file';
  game: { n: number; total: number } | null;
  entries: { done: number; total: number } | null;
  rows: TrackRow[];
  phase: 'decoding' | 'checking';
  bytes: { done: number; total: number };
}

const lastSegment = (p: string) => p.slice(Math.max(p.lastIndexOf('/'), p.lastIndexOf('\\')) + 1);

/** D6: the applicable extract items in plan (= apply) order. */
const gameIndexCache = new WeakMap<OrganizePlan, number[]>();
export function gameIndex(plan: OrganizePlan): number[] {
  let ids = gameIndexCache.get(plan);
  if (!ids) {
    ids = plan.items
      .filter((i) => i.action === 'auto' && i.target !== null && i.transfer.kind === 'extract')
      .map((i) => i.id);
    gameIndexCache.set(plan, ids);
  }
  return ids;
}

/** The target folder with the root's own separator. */
export function targetDir(root: string, target: string): string {
  const sep = root.includes('\\') ? '\\' : '/';
  const parts = target.split(/[\\/]/).slice(0, -1);
  return [root.replace(/[\\/]+$/, ''), ...parts].join(sep);
}

export function middleTruncate(name: string, max = 36): string {
  if (name.length <= max) return name;
  const tail = Math.ceil((max - 1) * 0.6);
  return `${name.slice(0, max - 1 - tail)}…${name.slice(-tail)}`;
}

export function extractView(
  plan: OrganizePlan | null,
  e: Extract<OrganizeProgressEvent, { phase: 'extract' }>,
): ExtractView {
  const item: OrganizeItem | undefined = plan?.items.find((i) => i.id === e.item);
  const transfer = item?.transfer.kind === 'extract' ? item.transfer : null;
  const total = e.entries_total;
  const done = e.entries_done;
  const counted = total !== undefined && done !== undefined && total > 1;
  const sorted: ExtractEntry[] = transfer ? [...transfer.entries].sort((a, b) => a.index - b.index) : [];
  const rows: TrackRow[] =
    counted && sorted.length === total
      ? sorted.map((x, k) => ({
          index: x.index,
          name: lastSegment(x.name),
          full: x.name,
          size: x.size,
          checked: x.crc32 === null ? 'size' : 'crc',
          state: k < done ? 'done' : k === done ? 'current' : 'waiting',
        }))
      : [];
  const games = plan ? gameIndex(plan) : [];
  const pos = games.indexOf(e.item);
  const first = sorted[0];
  return {
    itemId: e.item,
    name: item?.sources[0] ? lastSegment(item.sources[0]) : `#${e.item}`,
    systemId: item?.system_id ?? null,
    targetDir: plan && first ? targetDir(plan.target_root, first.target) : null,
    noun: item?.disc ? 'track' : 'file',
    game: plan && pos >= 0 ? { n: pos + 1, total: games.length } : null,
    entries: counted ? { done, total } : null,
    rows,
    phase: counted && done >= total && e.bytes_done < e.bytes_total ? 'checking' : 'decoding',
    bytes: { done: Math.min(e.bytes_done, e.bytes_total), total: e.bytes_total },
  };
}
```

- [ ] **Step 4: Run** the test — pass.
- [ ] **Step 5: Gates** G-WEB.
- [ ] **Step 6: Commit and push.** `git add -A && git commit -m "feat(ui): derive the extraction view from the plan and the event" && git push && git status -sb`

### Task 8: i18n: `useRichT` and the progress copy (es/en)

**Files:** Create `src/i18n/rich.tsx`, `src/i18n/rich.test.tsx`. Modify `locales/es/ui.json`, `locales/en/ui.json`, `src/lib/rate.test.ts`.

- [ ] **Step 0: Update with main** (locales change in parallel): `git fetch origin && git merge origin/main`; resolve, run G-WEB, commit the merge if any, `git push`.
- [ ] **Step 1: Failing tests** (`rich.test.tsx`): render `<I18nProvider locale="es">` around a probe that calls `tr('organize.progress.track', { n: 4, total: 12 })`; assert the text is «Pista 4 de 12» and that `4` and `12` are each in a `span.mono.tabular-nums`; literal text is not wrapped; an unknown `{x}` stays literal; `tr(key, vars, ['name'])` wraps only `name`.
- [ ] **Step 2: Run** `pnpm vitest run src/i18n` — fails.
- [ ] **Step 3: Implement.**

```tsx
import { Fragment, useCallback, useContext, type ReactNode } from 'react';
import { LocaleContext, translate, type TranslateVars } from './core';

/** `tr(key, vars, mono?)`: like `t()`, but returns nodes and wraps the
 * chosen values (all by default) in mono tabular spans (brand §7.6). */
export function useRichT() {
  const locale = useContext(LocaleContext);
  return useCallback(
    (key: string, vars: TranslateVars = {}, mono?: readonly string[]): ReactNode => {
      const template = translate(locale, key);
      return template.split(/(\{\w+\})/g).map((part, i) => {
        const name = /^\{(\w+)\}$/.exec(part)?.[1];
        if (!name || !Object.prototype.hasOwnProperty.call(vars, name)) return <Fragment key={i}>{part}</Fragment>;
        const value = String(vars[name]);
        return !mono || mono.includes(name) ? (
          <span key={i} className="mono tabular-nums"><bdi>{value}</bdi></span>
        ) : (
          <bdi key={i}>{value}</bdi>
        );
      });
    },
    [locale],
  );
}
```

Locales — add **one contiguous block** each, at the end of the `organize` object, `"progress": { … }` with every key of spec §7 (`extracting`, `checking`, `game`, `track`, `file`, `amount`, `rate`, `leftMinutes`, `leftUnderMinute`, `leftOverHour`, `leftOverHours`, `measuring`, `cancelling`, `cancellingButton`, `tracksLabel`, `filesLabel`, `showTracks`, `showFiles`, `waiting`, `writing`, `crcChecked`, `sizeChecked`, `cancelNote`, `stageLabel`) with the exact es/en strings of the spec's table, plus `movingLabel` (es «Moviendo a {folder}», en «Moving into {folder}») for the moves stage label; at the end of `start`: `"progress": { "folders": "Carpeta {done} de {total}" }` / `"Folder {done} of {total}"`; a new top-level `"progress": { "perMinute": "{n} por minuto" }` / `"{n} per minute"`. Do **not** remove `organize.extracting` yet (Task 14). Add to `rate.test.ts` the translate sweep described in Task 6.
- [ ] **Step 4: Run** `pnpm vitest run src/i18n src/lib/rate.test.ts` — pass (parity included).
- [ ] **Step 5: Gates** G-WEB.
- [ ] **Step 6: Commit and push.** `git add -A && git commit -m "feat(ui): progress copy and rich interpolation with mono numbers" && git push && git status -sb`

### Task 9: Hooks: `useReducedMotion` and `useRate`

**Files:** Create `src/lib/useReducedMotion.ts` (+ `.test.ts`), `src/hooks/useRate.ts` (+ `.test.ts`).

- [ ] **Step 1: Failing tests.**
  - `useReducedMotion`: stub `window.matchMedia` returning an object with `matches: true` and an `addEventListener` that stores the listener; `renderHook` → `true`; flip `matches` to false and call the listener → `false`. Second case: an object with only `addListener`/`removeListener` (old WKWebView) is also subscribed and unsubscribed on unmount.
  - `useRate`: `vi.useFakeTimers()`; mock `performance.now` with a counter; `renderHook(({ v }) => useRate('a', v), …)`; rerender with values at 0, 1000, 2000, 2500 ms → after 2 s `rate` ≈ the slope; key change keeps the last rate; renders counted with a ref are ≤ 3 per second of fake time with 10 value updates per second.
- [ ] **Step 2: Run** `pnpm vitest run src/lib/useReducedMotion.test.ts src/hooks/useRate.test.ts` — fail.
- [ ] **Step 3: Implement.**

```ts
// src/lib/useReducedMotion.ts
import { useSyncExternalStore } from 'react';
const QUERY = '(prefers-reduced-motion: reduce)';
function subscribe(cb: () => void) {
  const mq = window.matchMedia?.(QUERY);
  if (!mq) return () => undefined;
  if (mq.addEventListener) { mq.addEventListener('change', cb); return () => mq.removeEventListener('change', cb); }
  mq.addListener(cb); // WKWebView on macOS 11
  return () => mq.removeListener(cb);
}
export const useReducedMotion = (): boolean =>
  useSyncExternalStore(subscribe, () => window.matchMedia?.(QUERY).matches === true, () => false);
```

```ts
// src/hooks/useRate.ts
import { useEffect, useRef, useState } from 'react';
import { initialRate, pushSample, rateOf, type RateState } from '@/lib/rate';

/** Rate of `value` per second for `key` (null: idle), re-rendering at most
 * twice a second. Timestamps are taken here, on arrival (D4). */
export function useRate(key: number | string | null, value: number | null): number | null {
  const state = useRef<RateState>(initialRate());
  const [rate, setRate] = useState<number | null>(null);
  useEffect(() => {
    if (key === null || value === null) return;
    state.current = pushSample(state.current, key, value, performance.now());
  }, [key, value]);
  useEffect(() => {
    if (key === null) return;
    const id = window.setInterval(() => setRate(rateOf(state.current, performance.now())), 500);
    return () => window.clearInterval(id);
  }, [key]);
  return rate;
}
```

- [ ] **Step 4: Run** — pass.
- [ ] **Step 5: Gates** G-WEB.
- [ ] **Step 6: Commit and push.** `git add -A && git commit -m "feat(ui): reduced-motion and transfer-rate hooks" && git push && git status -sb`

---

## Group D — Components (frontend-design pass required)

### Task 10: `WorkReadout`: bar, amounts, rate, time left, counters

**Files:** Create `src/components/progress/WorkReadout.tsx`, `WorkReadout.test.tsx`.

- [ ] **Step 1: Failing tests.** Render with `bar={{ value: 300, max: 1200, slots: 16 }}`, `amount="312,4 MB de 591,9 MB"`, `rate="24,1 MB/s"`, `eta={null}`, `counters={['Juego 2 de 3', 'Pista 18 de 52']}`, `action={<button>Cancelar</button>}` in `es`: progressbar has `aria-valuetext` equal to the amount (through `getValueLabel`); «Calculando el tiempo…» shows for `eta={null}`; with `eta={undefined}` no time text; the counters are present; the row container has `aria-live="off"`; numbers inside the amount are in `.tabular-nums`.
- [ ] **Step 2: Run** `pnpm vitest run src/components/progress/WorkReadout.test.tsx` — fails.
- [ ] **Step 3: Implement.**

```tsx
import type { ReactNode } from 'react';
import { Progress } from '@/components/ui/progress';
import { useT } from '@/i18n/useT';

export interface WorkReadoutProps {
  bar: { value: number | null; max: number; slots?: number; tone?: 'primary' | 'warning' };
  label: string;
  /** «312,4 MB de 591,9 MB» or «Carpeta 12 de 40» (also the bar's value text). */
  amount?: ReactNode;
  amountText?: string;
  rate?: ReactNode;
  /** undefined: not applicable; null: still measuring; else the wording. */
  eta?: ReactNode | null;
  counters?: ReactNode[];
  action?: ReactNode;
}

export function WorkReadout({ bar, label, amount, amountText, rate, eta, counters = [], action }: WorkReadoutProps) {
  const t = useT();
  const left = [amount, rate, eta === null ? t('organize.progress.measuring') : eta].filter((x) => x !== undefined && x !== null);
  return (
    <div className="grid w-full gap-2">
      <Progress
        {...bar}
        aria-label={label}
        {...(amountText ? { getValueLabel: () => amountText } : {})}
      />
      <div aria-live="off" className="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1 text-dense tabular-nums text-cr-text-secondary">
        <p className="m-0 flex flex-wrap gap-x-2">
          {left.map((x, i) => (
            <span key={i}>{i > 0 && <span aria-hidden> · </span>}{x}</span>
          ))}
        </p>
        {counters.length > 0 && (
          <p className="m-0 flex flex-wrap gap-x-2">
            {counters.map((c, i) => (
              <span key={i}>{i > 0 && <span aria-hidden> · </span>}{c}</span>
            ))}
          </p>
        )}
      </div>
      {action && <div>{action}</div>}
    </div>
  );
}
```

- [ ] **Step 4: Run** — pass.
- [ ] **Step 5: Gates** G-WEB.
- [ ] **Step 6: Commit and push.** `git add -A && git commit -m "feat(ui): shared work readout for progress screens" && git push && git status -sb`

### Task 11: Stage frame and styles: `progress.css`, `stageArt.ts`, `WorkStage`

**Files:** Create `src/components/progress/progress.css`, `progress.css.test.ts`, `stageArt.ts`, `WorkStage.tsx`, `WorkStage.test.tsx`.

- [ ] **Step 1: Failing tests.**
  - `progress.css.test.ts`: `readFileSync('src/components/progress/progress.css', 'utf8')`; assert no `/#[0-9a-f]{3,8}\b/i`, no `rgb(`/`hsl(`, no `mix-blend-mode`, no `--cr-ease-spring`; strip every `@media (prefers-reduced-motion: no-preference) { … }` block (balanced-brace scan) and assert the rest contains neither `@keyframes` nor `animation:`/`animation-name`; a `@media (forced-colors: active)` block exists.
  - `WorkStage.test.tsx`: renders an `svg` with `aria-hidden`, a wrapper with `role="img"` and the given `aria-label`, the shelf rule, and the `overlay` children **outside** the `svg` (assert `svg.contains(overlayEl) === false`).
- [ ] **Step 2: Run** `pnpm vitest run src/components/progress` — fails.
- [ ] **Step 3: Implement.**

`stageArt.ts` (geometry in viewBox units, 360 × 112):

```ts
export const VIEW = { w: 360, h: 112 } as const;
export const SHELF_Y = 100;
/** Landing kit.svg#folder and #ftab path data (viewBox 48 × 36), copied. */
export const FOLDER_PATH = 'M2 6h44a2 2 0 0 1 2 2v26a2 2 0 0 1-2 2H2a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2z';
export const FTAB_PATH = 'M2 0h14l4 4v2H0V2a2 2 0 0 1 2-2z';
export const FOLDER = { x: 262, y: 46, scale: 1.5 } as const; // 72 × 54, front from y 55 to 100
export const BOX = { x: 16, y: 54, w: 68, h: 46 } as const;
export const ROUTE = 'M 86 50 Q 174 4 262 50';
export const SLEEVE = { w: 12, h: 16 } as const; // 3:4, the Slot
/** Label plate on the folder front (AP-2), as % of the frame for the HTML overlay. */
export const PLATE = { left: (274 / 360) * 100, top: (66 / 112) * 100, width: (48 / 360) * 100 } as const;
export const pct = (v: number, of: number) => `${(v / of) * 100}%`;
```

`WorkStage.tsx`:

```tsx
import type { ReactNode } from 'react';
import { SHELF_Y, VIEW } from './stageArt';
import './progress.css';

/** The SVG frame every stage shares: shelf rule with its Buckram cap, and an
 * HTML overlay laid over the viewBox (SystemIcon never in foreignObject, D9). */
export function WorkStage({ label, children, overlay }: { label: string; children: ReactNode; overlay?: ReactNode }) {
  return (
    <div role="img" aria-label={label} className="ws relative w-full max-w-[480px]" style={{ aspectRatio: `${VIEW.w} / ${VIEW.h}` }}>
      <svg aria-hidden focusable="false" viewBox={`0 0 ${VIEW.w} ${VIEW.h}`} className="absolute inset-0 h-full w-full overflow-visible">
        <line className="ws-shelf" x1="0" x2={VIEW.w} y1={SHELF_Y} y2={SHELF_Y} />
        <line className="ws-cap" x1="0" x2="48" y1={SHELF_Y + 1.5} y2={SHELF_Y + 1.5} />
        {children}
      </svg>
      {overlay}
    </div>
  );
}
```

`progress.css` (tokens only; motion opt-in):

```css
.ws-shelf { stroke: var(--cr-border-strong); stroke-width: 1; }
.ws-cap { stroke: var(--cr-slot-applied); stroke-width: var(--cr-border-width-spine); }
.ws-route { fill: none; stroke: var(--cr-slot-missing); stroke-width: var(--cr-border-width-slot); stroke-dasharray: 3 3; }
.ws-box, .ws-lid { fill: var(--cr-surface-raised); stroke: var(--cr-border-strong); stroke-width: 1.5; }
.ws-sleeve { fill: none; stroke: var(--cr-slot-missing); stroke-width: var(--cr-border-width-slot); stroke-dasharray: 4 3; }
.ws-sleeve.is-solid { stroke: var(--cr-slot-filled); stroke-dasharray: none; fill: var(--cr-surface-raised); }
.ws-spine { fill: var(--cr-slot-applied); }
.ws-folder { fill: var(--cr-surface-sunken); stroke: var(--cr-border-strong); stroke-width: 0.7; }
.ws-tab { fill: var(--cr-manila); }
.ws-plate { color: var(--cr-manila-fg); background: var(--cr-manila); }
.ws-check { fill: none; stroke: var(--cr-slot-applied); stroke-width: 2; stroke-linecap: round; }
.ws-lid { transform-box: view-box; transform-origin: 16px 54px; transition: transform var(--cr-duration-slow) var(--cr-ease-ink) 120ms; }
.ws-lid.is-open { transform: rotate(-14deg); }

@media (prefers-reduced-motion: no-preference) {
  .ws-box-in { animation: ws-box-in var(--cr-duration-slower) var(--cr-ease-enter) both; }
  .ws-march { animation: ws-march 800ms var(--cr-ease-standard) infinite; }
  .ws-fly-x { animation: ws-fly-x 640ms linear calc(var(--g, 0) * 140ms) both; }
  .ws-fly-y { animation: ws-fly-y 640ms calc(var(--g, 0) * 140ms) both; }
  .ws-flash { animation: ws-flash var(--cr-duration-base) var(--cr-ease-standard); }
  .ws-slide { transition: transform var(--cr-duration-slow) var(--cr-ease-ink); }
  @keyframes ws-box-in { from { transform: translateX(-24px); opacity: 0; } to { transform: none; opacity: 1; } }
  @keyframes ws-march { to { stroke-dashoffset: -14; } }
  @keyframes ws-fly-x { from { transform: translateX(0); } to { transform: translateX(var(--dx)); } }
  @keyframes ws-fly-y {
    0% { transform: translateY(0); animation-timing-function: var(--cr-ease-enter); }
    45% { transform: translateY(var(--lift)); animation-timing-function: var(--cr-ease-exit); }
    100% { transform: translateY(var(--drop)); }
  }
  @keyframes ws-flash { 50% { fill: var(--cr-manila-text); } }
}

@media (forced-colors: active) {
  .ws-shelf, .ws-route, .ws-box, .ws-lid, .ws-folder, .ws-sleeve { stroke: CanvasText; }
  .ws-box, .ws-lid, .ws-folder, .ws-tab { fill: Canvas; }
  .ws-sleeve.is-solid, .ws-spine, .ws-check { stroke: Highlight; fill: Highlight; }
  .ws-plate { forced-color-adjust: auto; }
}
```

(The Buckram cap uses the `--cr-slot-applied` role, never a raw `--cr-buckram-N` primitive.)
- [ ] **Step 4: Run** — pass.
- [ ] **Step 5: Gates** G-WEB.
- [ ] **Step 6: Commit and push.** `git add -A && git commit -m "feat(ui): progress stage frame and token-only motion styles" && git push && git status -sb`

### Task 12: `ExtractStage`: the box, the lid, the sleeves and their queue

**Files:** Create `src/components/progress/ExtractStage.tsx`, `ExtractStage.test.tsx`.

- [ ] **Step 1: Failing tests.**
  - Reducer (pure): first event of item 5 with `done: 4` → `{ item: 5, seen: 4, landed: 0, flying: [] }` (no replay); `done` 4 → 14 → `flying.length === 3`, `landed === 7`; `landed` action for one id → `landed 8`, 2 flying; `drain` → 10 landed, none flying; `instant: true` → nothing flies; a new item resets.
  - Component (es): renders `SystemIcon` for `systemId="psx"` in the overlay (not inside the SVG), the `stageLabel` as the `role="img"` name, the plate count «4/12»; with `matchMedia` stubbed to reduce → no element with class `ws-fly-x` after jumping `entriesDone` 4 → 14; `cancelling` → no `ws-march` element and no flying sleeves; `phase="checking"` → lid without `is-open` and a `.ws-check` path.
- [ ] **Step 2: Run** `pnpm vitest run src/components/progress/ExtractStage.test.tsx` — fails.
- [ ] **Step 3: Implement.**

```tsx
import { useEffect, useReducer, type CSSProperties } from 'react';
import { SystemIcon } from '@/components/system/SystemIcon';
import { useReducedMotion } from '@/lib/useReducedMotion';
import { WorkStage } from './WorkStage';
import { BOX, FOLDER, FOLDER_PATH, FTAB_PATH, PLATE, ROUTE, SLEEVE } from './stageArt';

export const FLY_CAP = 3;
export interface Flight { id: number; g: number }
export interface StageState { item: number | null; seen: number; landed: number; flying: Flight[] }
export type StageAction =
  | { type: 'event'; item: number; done: number; instant: boolean; cap?: number }
  | { type: 'landed'; id: number }
  | { type: 'drain' };
export const initialStage: StageState = { item: null, seen: 0, landed: 0, flying: [] };

/** D10: data first; at most `cap` sleeves in flight, the rest land at once. */
export function stageReducer(s: StageState, a: StageAction): StageState {
  switch (a.type) {
    case 'event': {
      if (a.item !== s.item) return { item: a.item, seen: a.done, landed: 0, flying: [] };
      if (a.done <= s.seen) return s;
      const fresh = Array.from({ length: a.done - s.seen }, (_, i) => s.seen + i);
      if (a.instant) return { ...s, seen: a.done, landed: s.landed + s.flying.length + fresh.length, flying: [] };
      const room = Math.max(0, (a.cap ?? FLY_CAP) - s.flying.length);
      const fly = room > 0 ? fresh.slice(-room) : [];
      return {
        ...s,
        seen: a.done,
        landed: s.landed + fresh.length - fly.length,
        flying: [...s.flying, ...fly.map((id, i) => ({ id, g: i }))],
      };
    }
    case 'landed':
      return s.flying.some((f) => f.id === a.id)
        ? { ...s, landed: s.landed + 1, flying: s.flying.filter((f) => f.id !== a.id) }
        : s;
    case 'drain':
      return { ...s, landed: s.landed + s.flying.length, flying: [] };
  }
}

export interface ExtractStageProps {
  label: string;
  itemId: number;
  systemId: string | null;
  /** null: no per-entry data (older wire, single entry). */
  entries: { done: number; total: number } | null;
  phase: 'decoding' | 'checking';
  cancelling: boolean;
  /** 'move': a stack of solid sleeves, one per `done` change (§6.2 Moves). */
  variant?: 'extract' | 'move';
}

export function ExtractStage({ label, itemId, systemId, entries, phase, cancelling, variant = 'extract' }: ExtractStageProps) {
  const reduced = useReducedMotion();
  const [s, dispatch] = useReducer(stageReducer, initialStage);
  const done = entries?.done ?? 0;
  useEffect(() => {
    const instant = reduced || cancelling || document.hidden;
    dispatch({ type: 'event', item: itemId, done, instant, cap: variant === 'move' ? 1 : FLY_CAP });
  }, [itemId, done, reduced, cancelling, variant]);
  useEffect(() => {
    if (cancelling || reduced) dispatch({ type: 'drain' });
  }, [cancelling, reduced]);
  useEffect(() => {
    const onHide = () => document.hidden && dispatch({ type: 'drain' });
    document.addEventListener('visibilitychange', onHide);
    return () => document.removeEventListener('visibilitychange', onHide);
  }, []);
  useEffect(() => {
    // AP-4: a sleeve whose animationend never comes still lands.
    const oldest = s.flying[0];
    if (!oldest) return;
    const id = window.setTimeout(() => dispatch({ type: 'landed', id: oldest.id }), 1500);
    return () => window.clearTimeout(id);
  }, [s.flying]);

  const open = phase === 'decoding' && !cancelling;
  const waiting = entries ? Math.min(5, Math.max(0, entries.total - entries.done - 1)) : 3;
  const marching = entries !== null && phase === 'decoding' && !cancelling && entries.done < entries.total;
  return (
    <WorkStage
      label={label}
      overlay={
        <span
          className="ws-plate absolute flex items-center justify-center gap-1 rounded-sm px-1 text-[11px] leading-4"
          style={{ left: `${PLATE.left}%`, top: `${PLATE.top}%`, width: `${PLATE.width}%` }}
        >
          {variant === 'extract' && <SystemIcon systemId={systemId} size={14} />}
          {entries && <span className="mono tabular-nums">{entries.done}/{entries.total}</span>}
        </span>
      }
    >
      <path className="ws-route" d={ROUTE} />
      {/* Box (or a stack of three sleeves for moves). */}
      <g key={itemId} className="ws-box-in">
        {variant === 'extract' ? (
          <>
            {Array.from({ length: waiting }, (_, i) => (
              <rect key={i} className="ws-sleeve" x={BOX.x + 8 + i * 10} y={BOX.y - 6} width={SLEEVE.w} height={SLEEVE.h} rx="1.5" />
            ))}
            <rect className="ws-box" x={BOX.x} y={BOX.y} width={BOX.w} height={BOX.h} rx="2" />
            <rect className={`ws-lid${open ? ' is-open' : ''}`} x={BOX.x - 2} y={BOX.y - 6} width={BOX.w + 4} height="7" rx="1.5" />
            {marching && (
              <rect className="ws-sleeve ws-march" x={BOX.x + BOX.w - 18} y={BOX.y - 14} width={SLEEVE.w} height={SLEEVE.h} rx="1.5" />
            )}
          </>
        ) : (
          [0, 1, 2].map((i) => (
            <rect key={i} className="ws-sleeve is-solid" x={BOX.x + 20 + i * 4} y={BOX.y + 20 - i * 4} width={SLEEVE.w} height={SLEEVE.h} rx="1.5" />
          ))
        )}
      </g>
      {/* Landed sleeves: spines standing behind the folder front. */}
      {Array.from({ length: Math.min(s.landed, 6) }, (_, i) => (
        <g key={i}>
          <rect className="ws-sleeve is-solid" x={FOLDER.x + 8 + i * 9} y={FOLDER.y - 4} width={SLEEVE.w} height={SLEEVE.h} rx="1.5" />
          <rect className="ws-spine" x={FOLDER.x + 8 + i * 9} y={FOLDER.y - 4} width="3" height={SLEEVE.h} />
        </g>
      ))}
      {s.flying.map((f) => (
        <g
          key={f.id}
          className="ws-fly-x"
          style={{ '--g': f.g, '--dx': '226px' } as CSSProperties}
          onAnimationEnd={(e) => e.animationName === 'ws-fly-x' && dispatch({ type: 'landed', id: f.id })}
        >
          <g className="ws-fly-y" style={{ '--g': f.g, '--lift': '-36px', '--drop': '-8px' } as CSSProperties}>
            <rect className="ws-sleeve is-solid" x={BOX.x + BOX.w - 18} y={BOX.y - 14} width={SLEEVE.w} height={SLEEVE.h} rx="1.5" />
          </g>
        </g>
      ))}
      <g transform={`translate(${FOLDER.x} ${FOLDER.y}) scale(${FOLDER.scale})`}>
        <path key={s.landed} className={`ws-tab${s.landed > 0 ? ' ws-flash' : ''}`} d={FTAB_PATH} />
        <path className="ws-folder" d={FOLDER_PATH} />
      </g>
      {phase === 'checking' && (
        <path className="ws-check" d={`M ${FOLDER.x + 26} ${FOLDER.y + 40} l 6 6 l 12 -14`} />
      )}
    </WorkStage>
  );
}
```

Then run the dev server (`pnpm dev --port 1462 --strictPort` with `?mockPick=downloads&mockExtract=slow`), take screenshots in light and dark, and tune the geometry (`--dx`, lift, sleeve positions) until the sleeve leaves the box mouth, follows the route and drops behind the folder front. Critique against the landing hero and `#seguro` (frontend-design). Stop the server by port.
- [ ] **Step 4: Run** the tests — pass.
- [ ] **Step 5: Gates** G-WEB.
- [ ] **Step 6: Commit and push.** `git add -A && git commit -m "feat(ui): illustrated extraction stage with a capped sleeve queue" && git push && git status -sb`

### Task 13: `TrackList`: the tracks of the game being extracted

**Files:** Create `src/components/progress/TrackList.tsx`, `TrackList.test.tsx`.

- [ ] **Step 1: Failing tests.** With 12 rows (4 done, one current, 7 waiting; one row `checked: 'size'`), es: the `ol` has the accessible name «Pistas de Tetris Plus.zip»; it is not a live region (no `aria-live`, no `role="status"|"log"`); done rows contain «✓» and hidden «CRC comprobado», the no-CRC done row «Tamaño y copia comprobados», the current «Escribiendo», waiting «En espera»; every name cell has `title` = full entry path and is wrapped in `<bdi>`; `collapsible` mode shows a button «Ver las 12 pistas» with `aria-expanded="false"` and `aria-controls` = the list id, toggling to `true`; `Element.prototype.scrollIntoView` spy is never called while `rows` advance.
- [ ] **Step 2: Run** `pnpm vitest run src/components/progress/TrackList.test.tsx` — fails.
- [ ] **Step 3: Implement.** Memoised `Row` (`memo` keyed by `index` and `state`), `SlotMark` from `src/components/ui/slot-mark.tsx` (`missing` for waiting/current, `applied` for done; current gets `ws-march` on its outline only if `SlotMark` accepts a `className`, otherwise a small inline `rect`), name through `middleTruncate(row.name, 40)` inside `<bdi title={row.full}>`, size via `formatDecimalSize(row.size, locale)` in `mono tabular-nums`, hidden state `<span className="sr-only">`. Keep the current row visible by setting the list's own `scrollTop` (`listRef.current.scrollTop = rowEl.offsetTop - listRef.current.clientHeight / 2`), skipped when the user scrolled the list in the last 5 s (`onScroll` with an `isTrusted`/programmatic flag). Max height `max-h-[calc(8*1.75rem)] overflow-y-auto`. Props: `{ rows, noun, name, collapsible }`; labels `organize.progress.tracksLabel|filesLabel`, `showTracks|showFiles`, states `waiting|writing|crcChecked|sizeChecked`.
- [ ] **Step 4: Run** — pass.
- [ ] **Step 5: Gates** G-WEB.
- [ ] **Step 6: Commit and push.** `git add -A && git commit -m "feat(ui): per-track list that ticks as each file is checked" && git push && git status -sb`

---

## Group E — Screens

### Task 14: `ApplyProgress` in «Organizar por sistema»

**Files:**
- Create: `src/components/organize/ApplyProgress.tsx`, `ApplyProgress.test.tsx`
- Modify: `src/screens/OrganizeScreen.tsx` (remove `applyingText`, `barOf`, `SLOTS` use at l. 76–126; applying block l. 445–466), `src/screens/OrganizeScreen.test.tsx` (tests at l. 380, 448, 480), `locales/{es,en}/ui.json` (remove `organize.extracting`)

- [ ] **Step 1: Failing tests.**
  - `OrganizeScreen.test.tsx`: replace `progress_shows_extracting_name_and_counts` with `progress_shows_game_and_track_counters`: set the store to `applying` with the `MOCK_DOWNLOADS` plan and an extract event for its 2nd extract item, `entries_done: 4, entries_total: 12`, `bytes_done: 300, bytes_total: 1200` → status sentence «Extracting Next Tetris, The.zip into …» (en) and the readout shows «Game 2 of 3» and «Track 5 of 12»; no text matches `/of \d+ files…/`; bar `aria-valuenow` 300, `aria-valuemax` 1200. Update `follows extract progress events while applying` to wait for `/^Game \d of 3$/` and the final summary. Cancel test: click Cancel → button reads «Cancelling…» and is disabled; the status reads «Cancelling: undoing what was extracted of …».
  - Move phase: an `apply` event shows the existing «Moving 12 of 40» sentence and a stage with `variant="move"`.
  - `plan: null` (resumed job) with an extract event: name `#7`, no game counter, track counter from the event.
  - Parity: `organize.extracting` absent from both locales.
- [ ] **Step 2: Run** `pnpm vitest run src/screens/OrganizeScreen.test.tsx src/components/organize/ApplyProgress.test.tsx` — fails.
- [ ] **Step 3: Implement.** `ApplyProgress({ plan, progress, onCancel })`:
  - `const [cancelling, setCancelling] = useState(false)` (resets on unmount, D15).
  - Extract event → `view = useMemo(() => extractView(plan, progress), [plan, progress])`; `rate = useRate(progress ? \`${progress.plan_id}:${view?.itemId ?? 'apply'}\` : null, view ? view.bytes.done : progress?.done ?? null)` — key per item for bytes (D4: new item resets the samples, the hook keeps `lastRate`).
  - Sentence (`<p role="status" className="m-0 text-ui">`): cancelling → `tr('organize.progress.cancelling', { name }, ['name'])`; checking → `organize.progress.checking`; extract → `organize.progress.extracting` with `name` and `folder` (= `view.targetDir`, middle-truncated, `title` full) in mono; moves → the existing `organize.applying|copying|organizing` keys; no event → `organize.starting`. The sentence's text changes only on a new game, on checking and on cancelling.
  - Stage: `ExtractStage` with `label = t('organize.progress.stageLabel', { name, system })` (system display name from `systemName(view.systemId)` passed in from the screen, or the id); moves → `variant="move"`, `entries = { done: progress.done, total: progress.total }`, `label = t('organize.progress.movingLabel', { folder: plan?.target_root ?? '' })`.
  - Readout: `WorkReadout` with `bar` by bytes (`value: bytes_total ? bytes.done : null`, `slots: 16`) or by items; `amount = tr('organize.progress.amount', { done: size(done), total: size(total) })` and `amountText` = the plain `t()` form; `rate` → `tr('organize.progress.rate', { rate: formatRate(locale, rate) })` when not null; `eta`: extract only — `null` while `rate === null`, else `etaKey(etaSeconds(total - done, rate))` through `t`; hidden (undefined) while checking or `bytes_total === 0`; counters: `game` → `organize.progress.game`, `entries` → `organize.progress.track|file` with `n = min(done + 1, total)`.
  - Action: Cancel button (`variant="ghost" size="sm"`), disabled and reading `organize.progress.cancellingButton` once pressed; `onClick={() => { setCancelling(true); onCancel(); }}`; `cancelNote` as a `hint` under it while extracting.
  - Layout: a card (`rounded-lg border border-cr-border bg-cr-surface-raised p-4`), `grid gap-4 [grid-template-columns:repeat(auto-fit,minmax(22rem,1fr))]`; left column stage + sentence + readout; `TrackList` in the second column (`collapsible` when the window is narrower than the two-column breakpoint: `useElementSize` from `src/hooks/useElementSize.ts` on the card, `width < 2 * 22 * 16 + gap`).
  - `OrganizeScreen.tsx`: the applying block becomes `<ApplyProgress plan={plan} progress={progress} systemName={systemName} onCancel={() => void useOrganizeStore.getState().cancel()} />`; delete `applyingText`, `barOf` and the now-unused imports; delete `organize.extracting` from both locales.
- [ ] **Step 4: Run** `pnpm vitest run src/screens src/components/organize src/i18n` — pass.
- [ ] **Step 5: Gates** G-WEB. Screenshots (dev server on 1462, `?mockPick=downloads`) at 1280 × 800 and 760 × 800, light and dark; critique and fix; stop the server by port.
- [ ] **Step 6: Commit and push.** `git add -A && git commit -m "feat(ui): extraction stage, readout and track list while organising" && git push && git status -sb`

### Task 15: Scan: stage variant and readout

**Files:** Create `src/components/progress/ScanStage.tsx`, `ScanStage.test.tsx`. Modify `src/components/ScanProgressView.tsx`, `ScanProgressView.test.tsx`, `locales/{es,en}/ui.json` (remove `start.scanning` if unused: `grep -rn "start.scanning'" src`).

- [ ] **Step 1: Failing tests.**
  - `ScanProgressView.test.tsx`: `role="status"` holds only the sentence «Leyendo la carpeta…» / «Reading the folder…» (assert the status element's text equals it exactly, AP-3); the readout shows «Carpeta 12 de 40» with `12` and `40` in mono, «312 ROMs encontradas», and the current folder in mono with `title`; the bar is indeterminate before the first folder (`aria-valuenow` absent).
  - `ScanStage.test.tsx`: `roms: 0` → 0 solid slots; `roms: 26` → 2; `roms: 10_000` → 12 (cap); the stage has the `role="img"` label `start.progressLabel`; under reduced motion no `.ws-slide` transition element animates (`getAnimations` is not available in jsdom: assert the class is applied only through CSS, i.e. the component renders the same DOM).
- [ ] **Step 2: Run** `pnpm vitest run src/components/ScanProgressView.test.tsx src/components/progress/ScanStage.test.tsx` — fail.
- [ ] **Step 3: Implement.** `ScanStage({ done, total, roms, label })` in a `WorkStage`: a dashed vertical reading line at x = 180 (`ws-route` class), five small folder glyphs (`FOLDER_PATH` scaled 0.5) in a `<g className="ws-slide" style={{ transform: \`translateX(${-24 * (done % 5)}px)\` }}>` passing under it, and on the shelf a row of `min(12, ceil(roms / 25))` solid sleeves (`ws-sleeve is-solid`, 12 × 16, 4 units apart) starting at x = 200. `ScanProgressView`: wrapper loses `role="status"`; sentence `<p role="status">{t('start.scanningStart')}</p>`; `ScanStage`; `WorkReadout` with `bar` as before (`slots: 16`), `amount = tr('start.progress.folders', …)` when `done > 0`, counters `[t('start.romsFound', …)]`, no rate, no ETA (D12); the path line stays.
- [ ] **Step 4: Run** — pass.
- [ ] **Step 5: Gates** G-WEB + screenshot of the scan screen (light/dark).
- [ ] **Step 6: Commit and push.** `git add -A && git commit -m "feat(ui): scan progress stage and readout" && git push && git status -sb`

### Task 16: Cover fill: readout with covers per minute and time left

**Files:** Modify `src/components/fill/JobsPanel.tsx` (l. 271–300), `src/components/fill/JobsPanel.test.tsx`.

- [ ] **Step 1: Failing tests.** With a running job at `done: 2` → no rate and no time text; feed progress `done` 0 → 30 over 60 s of fake time (`vi.useFakeTimers`, `performance.now` mocked) → «30 por minuto» and a `leftMinutes`/`leftUnderMinute` wording; a `paused` job state → neither rate nor time; the label still shows in mono.
- [ ] **Step 2: Run** `pnpm vitest run src/components/fill` — fails.
- [ ] **Step 3: Implement.** Replace the `Progress` + counter block with `WorkReadout` (plain bar, no `slots`, as before): `amount` = the existing `jobs.progress` text; `rate = useRate(running ? job.id : null, p?.done ?? null)`; show `t('progress.perMinute', { n: Math.round(rate * 60) })` and the ETA (`etaKey(etaSeconds(total - done, rate))`) only when `done >= 5`, ≥ 20 s of samples (pass a `minSpanMs` option to `useRate`, default 2000; add it to `rate.ts` `rateOf(s, now, minSpanMs)` with a test), and the job is not paused; `action` = the existing Cancel button; the label line stays.
- [ ] **Step 4: Run** — pass.
- [ ] **Step 5: Gates** G-WEB.
- [ ] **Step 6: Commit and push.** `git add -A && git commit -m "feat(ui): cover fill progress with rate and time left" && git push && git status -sb`

### Task 17: Duplicates: readout in the apply block only (last, after merging main)

**Files:** Modify `src/screens/DuplicatesScreen.tsx` (apply block l. 259–290 only), `src/screens/DuplicatesScreen.test.tsx`.

- [ ] **Step 0: Update with main.** `git fetch origin && git merge origin/main`; TODO §1.5 may have reshaped this screen: re-read the file, keep its new structure, and touch only the apply block's bar and counter (one hunk, D11). Run G-WEB, commit the merge if any, `git push`.
- [ ] **Step 1: Failing test.** During an apply (`dedupeScenario` as the existing apply tests use), after `done >= 5` and 2 s of fake time, a time-left wording shows; before that, none; the status sentence is unchanged.
- [ ] **Step 2: Run** `pnpm vitest run src/screens/DuplicatesScreen.test.tsx` — fails.
- [ ] **Step 3: Implement.** Wrap the existing `Progress` (with its `slots`) in `WorkReadout`: `bar` as now, `eta` from `useRate(job?.id ?? null, progress?.done ?? null)` once `done >= 5` (else `undefined`), `action` = the existing Cancel button. Leave the planning block (l. 332) and everything else alone.
- [ ] **Step 4: Run** — pass.
- [ ] **Step 5: Gates** G-WEB.
- [ ] **Step 6: Commit and push.** `git add -A && git commit -m "feat(ui): time left while setting duplicates aside" && git push && git status -sb`

---

## Group F — Verification

### Task 18: Full gates, browser E2E and a real-archive check

**Files:** none new (fixes go in their own `fix(…)` commits, each pushed).

- [ ] **Step 1: Update with main.** `git fetch origin && git merge origin/main`, resolve, push.
- [ ] **Step 2: All gates.**
  `pnpm check-all && pnpm build && cargo fmt --all --check && cargo clippy --workspace --all-targets --all-features -- -D warnings && scripts/dev/clippy-windows.sh && cargo test --workspace --no-fail-fast`
  Paste the last line of each into the evidence note (Step 7).
- [ ] **Step 3: Start the mock app.** `pnpm dev --port 1462 --strictPort` in the background.
- [ ] **Step 4: E2E** (Chrome DevTools MCP, `new_page` with `isolatedContext: "organize-ui"`; fallbacks per Global Constraints). Spec §11.3:
  1. `http://localhost:1462/?mockPick=downloads` → open the folder, Organise, simulate, apply. Assert (snapshots): the stage is visible with the current game's `SystemIcon` (PS1, then Dreamcast, then Saturn), «Juego n de 3» advances, «Pista k de M» grows, ✓ rows grow, a rate and a time-left wording appear after ~2 s, and «Comprobando los archivos de …» shows between games. Screenshots at 1280 × 800 and 760 × 800, light and dark, es and en.
  2. `emulate` `prefers-reduced-motion: reduce`: `evaluate_script` `document.querySelector('.ws').getAnimations({ subtree: true }).length === 0` after a tick, while the counters still advance.
  3. `?mockPick=downloads&mockExtract=80`: Cancel during the third game → «Cancelando…» shows and the button is disabled, then the result reports that game not extracted and the first two extracted.
  4. Navigate away from Organise and back mid-apply: counters correct at once (game n of 3), no console errors.
  5. `emulate` `forced-colors: active`: screenshot; the stage strokes are visible.
  6. Scan screen (open a fixture library) and the fill jobs panel: screenshots of the new readout.
  7. `list_console_messages`: no errors and no React warnings throughout.
- [ ] **Step 5: Stop the server by port.** `lsof -ti tcp:1462 -sTCP:LISTEN | xargs -r kill`.
- [ ] **Step 6: Real archives (read-only source, copies only).** Find the owner's three disc archives without writing: `find /Volumes/TinyExt/ROMs-Downloaded -maxdepth 3 -iname '*tetris*' 2>/dev/null` (skip this step, saying so, if the volume is not mounted). Copy them: `mkdir -p target/organize-ui-sample/src && cp -RXp <the three files> target/organize-ui-sample/src/`; check `find target/organize-ui-sample -name '._*'` is empty. Run the CLI through a pseudo-terminal so it prints progress: `script -q /dev/null cargo run -q -p coverly-cli -- organize target/organize-ui-sample/src --target target/organize-ui-sample/out --copy --apply --data-dir target/organize-ui-sample/data --cache-dir target/organize-ui-sample/cache 2>&1 | tail -40` — expect `extract item n/N files k/M` lines growing once per entry (≤ 1/s) and the three games extracted. Then `rm -rf target/organize-ui-sample`.
- [ ] **Step 7: Evidence.** Post the gate results, the E2E assertions and screenshot list as a comment on the branch's PR if one exists (`gh pr view --json number` → `gh pr comment`), otherwise report them in the final message. Do not edit `docs/TRACKER.md` or `TODO.md`.
- [ ] **Step 8: Final state.** `git status -sb` shows `## feat/organize-extract-progress...origin/feat/organize-extract-progress` with nothing ahead and a clean tree.

---

## Spec coverage

| Spec | Task |
|---|---|
| D1, D2, §5.1 core, §11.1 order/resume/coalescing | 1, 2 |
| §5.1 Tauri throttle and wire, contract sample | 3 |
| D13 CLI (AP-1) | 4 |
| D14, §9 mock | 5 |
| D3, D4, D5 rate and ETA | 6, 9 |
| D6, D7, D8, `extractView`, `targetDir` | 7 |
| §7 copy, `useRichT` | 8 |
| §6.6 reduced motion hook | 9 |
| §6.4 `WorkReadout` | 10 |
| D9, §6.1 tokens, §6.6 forced colours, CSS test | 11 |
| §6.2 beats, D10 queue (AP-2, AP-4) | 12 |
| §6.3 track list | 13 |
| D15, §6.2 moves, §8, §10 cases | 14 |
| §6.5 scan (AP-3), fill, duplicates (D11, D12) | 15, 16, 17 |
| §11.3 E2E, real-app check | 18 |
