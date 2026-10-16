# M4 — Bulk Fill in the App Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** From the desktop app, the user fills missing art in bulk: a wizard (scope → media → strategy + region order → overwrite policy → dry-run summary) builds a plan, a keyboard-driven review queue settles the doubtful matches, a job applies it with live progress, pauses on rate limit / offline / full or read-only volume and resumes, can be cancelled, reports per-item errors, and the last job can be undone. The detail drawer can change art (pick a candidate), set it from a local file and remove it (undoable). Audit and job reports export to CSV/JSON. Hacks, betas, prototypes and unlicensed variants never win as `auto` (M2-11). Everything also runs in a plain browser on the mock backend so the whole flow is E2E-testable in Chrome.

**Architecture:** `coverly-core` gains a `fill` facade (`FillEnv::plan` / `apply` / `candidate_image`, `write_local_image`, `remove_art`) shared by the CLI and the app, review decisions on `Plan`, structured pause kinds, journal resume (`Writer::resume`, one undo per job) and a journaled `remove`. `src-tauri` gains a `jobs` module: a `JobManager` holding at most one job (gather thread → `planned` → apply thread → `done`/`paused`/`cancelled`), emitting `fill-job-progress` / `fill-job-changed` events and refreshing only the touched entries of the open `Session` (`library-entries-updated`). The webview never sends a path: candidates are addressed by `(job, item, candidate)`, entries by `(generation, id)`, and the "from file" and "save report" dialogs are opened from Rust. The frontend adds a `jobStore`, a wizard, a review queue, a jobs panel, drawer actions and a reports panel, all through the existing `Backend` interface (Tauri adapter + a deterministic mock with fake progress, review items, failures and a rate-limit pause).

**Tech Stack:** Rust 1.90 (edition 2021), Tauri 2.12, `tauri-plugin-dialog` 2.8.3 (Rust API `DialogExt`: `blocking_pick_file`, `blocking_save_file`), `serde_json` (moves from dev to normal dependency of `src-tauri`), `coverly-core` feature `test-util` for `src-tauri` tests (mock HTTP server). React 19 + TypeScript 6 (strict, `exactOptionalPropertyTypes`, `noUncheckedIndexedAccess`) + Tailwind 3 + zustand 5. Tests: `cargo test`, Vitest 5 + Testing Library + jsdom. E2E: Chrome DevTools MCP against `pnpm dev`; a real run of the Rust job path against a copy of two systems.

**Spec:** `docs/superpowers/specs/2026-10-15-coverly-roms-design.md` — §7 steps 3–9 (select, identify, fetch, choose, review, apply, re-scan affected entries + report), **§8 Fill wizard, Review queue, Jobs, Reports** and the drawer actions (change, from file, remove), §9 (Art: overwrite policy; Matching: strategy, region order; Safety: backup before overwrite, dry run by default, undo history depth), **§10 per-item errors** (`no-art`, `network`, `decode`, `write-denied`, `disk-full`, `name-invalid`), pause/resume, §11, §12, §15 (M4 exit: "Fill missing covers on the SD copy from the UI") and §16 (refinements win; §16.10 safety). Tracker rows M4-01..M4-04, open row **M2-11**, decisions D1–D19 in `docs/TRACKER.md`. Architecture: `docs/04-architecture/fill-pipeline.md`, `docs/04-architecture/desktop-app.md`. Pluryprint (`/Users/miguelangelesparzacalero/Documents/GitHub/pluryprint`) is a read-only reference for patterns.

## Global Constraints

- Branch `feat/m4-fill-app`; never switch branches, never push, never touch `main`. Every shell command starts with `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms &&`.
- TDD: write the failing test, run it and see it fail, implement, see it pass.
- After **every** task run all gates and fix anything red before committing: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `pnpm lint`, `pnpm typecheck`, `pnpm format:check`, `pnpm test`, `pnpm build`.
- Commits: conventional, granular, English. **Never** add `Co-Authored-By` or any Claude attribution. Do not push.
- `rust-version = "1.90"`, `edition = "2021"` stay.
- The core never depends on Tauri and holds no UI strings (spec §6). `src-tauri` stays thin: commands are one-liners over `state`/`jobs`/`library` functions that are testable without a Tauri runtime.
- Tests never hit the real network: Rust uses `coverly_core::net::testing::{MockServer, HangingServer}` on 127.0.0.1; the UI uses the mock backend. Only the ignored `real_fill` test of Task 13 talks to the real servers, and only when run by hand.
- Real ROM data `/Users/miguelangelesparzacalero/Documents/ROMS` is **strictly read-only**. Never create, modify or delete anything under `/Volumes`. Any real write uses a COPY under `/private/tmp/claude-501/-Users-miguelangelesparzacalero-Documents-GitHub/35b7c020-f2ae-412f-b757-99c24a9a498b/scratchpad` (`$SC` below).
- Every UI string goes through i18n in `locales/en/ui.json` **and** `locales/es/ui.json` (same keys; `src/i18n/parity.test.ts` enforces it).
- File names and downloaded data are hostile: React text only (never `dangerouslySetInnerHTML`), escaped in mock SVG, every downloaded or picked image decoded with limits and re-encoded (`process_image`), CSV fields formula-neutralised (`'` prefix for `= + - @ \t \r`).
- The webview never sends a library path. Art writes are addressed by `(generation, entryId, media)` or `(job, item, candidate)`; the local-image picker and the report save dialog run in Rust (`tauri_plugin_dialog::DialogExt`), so no new JS capability is granted (`capabilities/default.json` stays `core:default` + `dialog:allow-open`).
- One fill job at a time. The app's Undo is the newest not-undone journal **of the open library** (`list_jobs` + `undo_job`; never a journal of another library or SD), refused with `job-running` while a job gathers, applies **or is paused** (a paused job resumes into its journal). A worker thread never leaves a job stuck in `gathering`/`applying`: a panic ends the job as `failed`.
- IPC: JS argument names camelCase (`entryId`), payload fields snake_case (`entry_ids`), errors `{ code, detail? }` shown as `errors.<code>`; codes are stable.

## Review Focus

1. **Network or volume trouble in the middle of a job** — a 429 / going offline / the SD becoming read-only while applying must pause (banner with the reason and a Resume button), not fail every item; Resume must finish into the **same** journal so one Undo reverts both runs. Tests: Task 2 (`resume_appends_to_the_same_journal_and_one_undo_reverts_both`, `a_paused_job_resumes_into_the_same_journal`), Task 5 (`resume_after_a_rate_limit_finishes_the_same_journal`), Task 11 (`shows_the_pause_reason_and_resumes`).
2. **Double start / double click on Run** — a second `start_fill` or a `run_fill` on a job that is already applying must be refused (`job-running` / `bad-request`), never two writers. Tests: Task 5 (`a_second_job_is_refused_while_one_runs`, `run_twice_is_refused_and_decisions_need_a_planned_job`), Task 9 (`run_button_is_disabled_while_running`).
3. **Hostile names in new surfaces** — progress labels, review cards, error lists and exported CSV with `<img onerror>`, RTL overrides or `=HYPERLINK(...)`: text only, CSV neutralised. Tests: Task 7 (`job_report_csv_neutralises_formulas`, `audit_export_neutralises_formulas`), Task 8 (`mock_csv_neutralises_formulas`), Task 10 (`renders_hostile_names_as_text`).
4. **The library is reopened (new generation) while a job runs or after it** — `start_fill` with an old generation is `stale-library`; `library-entries-updated` for another generation is ignored by the UI; the job keeps its own session. Tests: Task 6 (`start_with_a_stale_generation_is_refused`), Task 9 (`ignores_entry_updates_for_another_generation`).
5. **Undo after the user touched the art** — a file the user replaced or re-added after the job (or after a removal) is kept and reported as kept; removed art comes back byte for byte. Tests: Task 2 (`undo_restores_removed_art`, `undo_keeps_art_the_user_added_after_a_removal`), Task 11 (`undo_reports_kept_files`).
6. **Undo or a crash at the wrong moment** — Undo while a job is paused is refused (resume would append to a journal that was just undone), Undo never reverts a journal of another library than the open one, and a panicking worker ends the job as `failed` instead of blocking new jobs, Undo and the drawer forever. Tests: Task 5 (`a_panicking_worker_fails_the_job_instead_of_leaving_it_busy`), Task 6 (`undo_is_refused_while_a_job_is_paused`, `undo_only_touches_the_open_library`), Task 8 (mock pause test refuses undo).

## Deviations from the spec / task brief (decided in this plan)

- **Overwrite policy has two choices in M4**: "only missing art" (`never`) and "replace existing art, with backup" (`backup`, scope `all`). Spec §9's "ask" needs per-file prompts and moves to M5 settings.
- **The app writes only `auto` items plus the review items the user accepted or re-picked** (undecided review items are skipped as `skipped-review`). The CLI keeps D14 (writes review items unless `--auto-only`). New decision **D20**.
- **Resume reopens the same journal** (`Writer::resume`), so a paused-and-resumed job is one job for Undo and history. Resuming a gather pause re-runs the dry run (cached candidates make it cheap) and clears decisions. New decision **D21**.
- **M2-11 fix in the plan, not only in the strategies**: the strategies already rank a release above a variant within the best tier; when the best tier holds only a variant (the real cases), `build_plan` moves the first retail candidate that reaches the review threshold to the front and never classes a non-retail pick as `auto` unless it is a canonical/exact file-name match or the ROM itself is that kind of variant (`PlanNote::UnreleasedVariant`). `(Unl)` counts as a variant. New decision **D22**.
- **"Search…" in the drawer stays disabled** (search by typed title needs a manual identity override that must not change the target file name) → new tracker row M4-05 for M5.
- **Scope "selection"** is the selected entry (M3 has single selection); "current filter" sends the ids of the visible entries; "whole library" sends `null`.
- **Job report export covers the current/last job of this app session**; the history lists every journal (id, date, files, complete/undone) but per-item reports of older sessions are not kept (journals hold targets only).
- **Undo in the app is per library**: "Undo last job" reverts the newest not-undone journal whose root is the open library (the CLI `coverly undo` keeps undoing the newest journal of any library). Part of D21.
- **Dialogs from Rust**: "From file…" and both "Export…" actions open native dialogs in Rust and return the chosen file name (or `null` when cancelled). The mock downloads exports through a Blob URL and "picks" a built-in image. New decision **D23**.
- **M4 exit criterion** ("fill missing covers on the SD copy from the UI"): the UI is proven end to end in Chrome on the mock, and the exact Rust job path the UI drives (`AppState::start_fill` → `run` → `undo_last_job`) runs against a copy of two real systems with the real servers (Task 13). A by-hand GUI run in the Tauri window stays as the manual check for Miguel, like M3.
- **Still open after M4**: M2-10 (Batocera `gamelist.xml`; the wizard shows a warning for the `batocera` profile, like the CLI), M2-12 (Anbernic max size), M3-07 (identity filter; identity stays on demand) → tracker notes in Task 13.

---

## File structure

```
crates/coverly-core/
├─ src/lib.rs                     + pub mod fill; mod csv_util
├─ src/progress.rs                + PauseKind
├─ src/net/mod.rs                 + NetError::pause_kind
├─ src/matching/mod.rs            + is_unreleased_variant (pub), bracket groups, (Unl)
├─ src/plan/mod.rs                + PlanNote, PlanItem.note, Plan.pause_kind, Decision, Plan::{decide, subset}, NoneReason::Skipped
├─ src/gather/mod.rs              + Gathered.pause_kind
├─ src/jobs/mod.rs                + ApplyOptions.resume_job, JobReport.{pause_kind, resumable_items, merge_resumed},
│                                   ItemErrorKind::as_str, write_report_csv
├─ src/write/mod.rs               + Writer::{resume, remove}, Op::Removed, RemoveOutcome, undo of removals
├─ src/csv_util.rs                quote / write_row / media_name (moved out of audit/csv.rs)
├─ src/fill/mod.rs                Endpoints, FillSettings, FillEnv::{plan, apply, candidate_image},
│                                 write_local_image, remove_art, ArtError, KEEP_JOBS
└─ tests/{variants.rs, journal_resume.rs, fill_env.rs, job_report.rs} + apply.rs (appended tests)
crates/coverly-cli/src/fill.rs    run() on FillEnv; review note; NoneReason::Skipped
src-tauri/
├─ Cargo.toml                     serde_json → [dependencies]; dev: coverly-core with test-util
├─ src/lib.rs                     FillConfig, new commands
├─ src/error.rs                   + job-running, no-job, stale-job, bad-request, nothing-to-undo, network,
│                                   bad-image, write-denied, disk-full, name-invalid; From<WriteError>, From<ArtError>
├─ src/state.rs                   AppState::new(settings, FillConfig); jobs; picks; refresh_entries; fill/undo/history/
│                                   drawer/export methods
├─ src/ipc.rs                     job, drawer and export commands; TauriEvents
├─ src/library/mod.rs             Session.{profile, systems}, SessionEntry.system, Clone
├─ src/library/refresh.rs         subset_scan, refresh_entries, EntryArtUpdate, ids_for_art_paths, entry_index
├─ src/library/art.rs             inside_root → pub(crate)
├─ src/jobs/mod.rs                FillConfig, JobEvents, JobManager, with_env
├─ src/jobs/view.rs               FillRequest, JobView, JobItemView, CandidateView, … (wire types)
└─ tests/{common/fill.rs, refresh.rs, fill_job.rs, fill_state.rs, drawer_actions.rs, real_fill.rs} + contract.rs, app_state.rs
src/
├─ ipc/types.ts, backend.ts, tauriBackend.ts, errors.ts, contract.sample.json, contract.test.ts
├─ ipc/mock/mockJobs.ts           deterministic fake jobs, drawer actions, exports
├─ ipc/mock/mockBackend.ts        composes mockJobs
├─ lib/{fill.ts, csv.ts, audit.ts} + artCache.ts (dropArtUrls)
├─ hooks/useImageUrl.ts           blob URL of an ArtImage loader
├─ stores/{jobStore.ts} + libraryStore.ts (applyArtUpdates), viewStore.ts (overlay)
├─ components/Modal.tsx
├─ components/fill/{FillWizard, ReviewQueue, JobsPanel, CandidateImage}.tsx
├─ components/reports/ReportsPanel.tsx
├─ components/library/{Toolbar, DetailDrawer}.tsx, screens/LibraryScreen.tsx
└─ styles/globals.css
locales/{en,es}/ui.json
docs/04-architecture/{desktop-app.md, fill-pipeline.md}, docs/TRACKER.md
```

---
### Task 1: Core — hacks, betas, prototypes and unlicensed variants never win as `auto` (M2-11)

**Files:**
- Modify: `crates/coverly-core/src/matching/mod.rs` (private `unreleased` → public `is_unreleased_variant` with bracket groups and `(Unl)`)
- Modify: `crates/coverly-core/src/plan/mod.rs` (`PlanNote`, `PlanItem.note`, `choose` takes the ROM stem)
- Modify: `crates/coverly-cli/src/fill.rs` (review line shows the note)
- Test: `crates/coverly-core/tests/variants.rs`

**Interfaces:**
- Consumes: `matching::{strategy_for, RankContext}`, `plan::{build_plan, confidence, PlanOptions}`, `gather::{GatherItem, GatherTargetSummary, Gathered}` (existing).
- Produces:
  - `pub fn coverly_core::matching::is_unreleased_variant(name: &str) -> bool`
  - `#[serde(rename_all = "kebab-case")] pub enum coverly_core::plan::PlanNote { UnreleasedVariant }` (`Clone, Copy, Debug, PartialEq, Eq, Serialize`)
  - `PlanItem.note: Option<PlanNote>` (`#[serde(skip_serializing_if = "Option::is_none")]`)

- [ ] **Step 1: Write the failing tests with the real M2 candidate lists**

`crates/coverly-core/tests/variants.rs`:

```rust
//! M2-11: the libretro candidates of the M2 end-to-end dry run (cache
//! namespace `candidates-2`, 2026-10-16) for two real GBA ROMs of the SD.

mod common;

use std::collections::BTreeMap;

use coverly_core::art::resolve_art;
use coverly_core::gather::{
    select_targets, GatherItem, GatherTargetSummary, Gathered, Scope, TargetFilter,
};
use coverly_core::identify::names::{parse_name, Region};
use coverly_core::identify::{IdMethod, Identity};
use coverly_core::matching::is_unreleased_variant;
use coverly_core::plan::{build_plan, Plan, PlanAction, PlanItem, PlanNote, PlanOptions};
use coverly_core::profiles::Media;
use coverly_core::scan::{scan_library, ScanOptions};
use coverly_core::sources::{Candidate, MatchKind};
use coverly_core::systems::SystemDb;

fn cand(name: &str, regions: &[Region], kind: MatchKind, score: u8) -> Candidate {
    Candidate {
        source: "libretro".into(),
        media: Media::Cover,
        name: name.into(),
        url: format!("https://x/{name}.png"),
        fallback_url: None,
        regions: regions.to_vec(),
        kind,
        score,
        votes: None,
        consensus: 0,
    }
}

fn real_candidates(stem: &str) -> Vec<Candidate> {
    use MatchKind::*;
    use Region::*;
    match stem {
        "057 Final Fantasy IV" => vec![
            cand("Final Fantasy IV - Sound Restoration hack [Hack by Bregalad]", &[], Fuzzy, 90),
            cand("Final Fantasy IV Advance (Europe) (En,Fr,De,Es,It)", &[Europe], Fuzzy, 65),
            cand("Final Fantasy IV Advance (Japan)", &[Japan], Fuzzy, 65),
            cand("Final Fantasy IV Advance (Japan) (Rev 1)", &[Japan], Fuzzy, 65),
            cand(
                "Final Fantasy IV Advance (Japan) (Rev 1) (Virtual Console)",
                &[Japan],
                Fuzzy,
                65,
            ),
        ],
        "054 Contra Advance" => vec![cand("Contra Advance (USA) (Beta)", &[Usa], TitleExact, 95)],
        "070 Zelda (Beta)" => vec![cand("Zelda (USA) (Beta)", &[Usa], TitleExact, 95)],
        "071 Dumped" => vec![cand("Dumped (USA) (Proto)", &[Usa], Canonical, 100)],
        "072 Golden Sun" => vec![
            cand("Golden Sun (USA) (Beta)", &[Usa], TitleExact, 95),
            cand("Golden Sun (USA, Europe)", &[Usa, Europe], TitleExact, 95),
        ],
        _ => vec![],
    }
}

/// The plan of a GBA folder holding `stems`, with the candidates above.
fn plan_for(stems: &[&str]) -> (tempfile::TempDir, Plan) {
    let dir = tempfile::TempDir::new().unwrap();
    let r = dir.path();
    common::mkdir(r, "anbernic");
    for s in stems {
        common::write(r, &format!("ROMS/GBA/{s}.gba"), s);
    }
    let profile = common::profile("anbernic-stock");
    let db = SystemDb::builtin();
    let scan = scan_library(r, &profile, &db, &ScanOptions::default());
    let art = resolve_art(&scan, &profile, &db, &[Media::Cover]);
    let targets = select_targets(
        &art,
        &TargetFilter {
            systems: vec![],
            media: vec![Media::Cover],
            scope: Scope::Missing,
        },
    );
    let items = targets
        .iter()
        .map(|t| GatherItem {
            target: GatherTargetSummary {
                system_id: t.system_id.clone(),
                folder: t.folder.clone(),
                system_dir: t.system_dir.clone(),
                rel_dir: t.rom.rel_dir.clone(),
                path: t.rom.path.clone(),
                stem: t.rom.stem.clone(),
                media: t.media.clone(),
            },
            identity: Identity {
                canonical: None,
                method: IdMethod::Name,
                confidence: 0.0,
                parsed: parse_name(&t.rom.stem, true),
                hash: None,
            },
            candidates: BTreeMap::from([(Media::Cover, real_candidates(&t.rom.stem))]),
            errors: BTreeMap::new(),
        })
        .collect();
    let gathered = Gathered {
        items,
        ..Gathered::default()
    };
    let plan = build_plan(r, &profile, &db, &gathered, &PlanOptions::default());
    (dir, plan)
}

fn item<'a>(plan: &'a Plan, stem: &str) -> &'a PlanItem {
    plan.items.iter().find(|i| i.stem == stem).unwrap()
}

#[test]
fn final_fantasy_iv_never_takes_the_sound_restoration_hack_as_auto() {
    let (_dir, plan) = plan_for(&["057 Final Fantasy IV"]);
    let it = item(&plan, "057 Final Fantasy IV");
    let PlanAction::Review {
        chosen,
        alternatives,
        ..
    } = &it.action
    else {
        panic!("{:?}", it.action)
    };
    // The retail GBA game is Final Fantasy IV Advance; the hack stays an option.
    assert_eq!(chosen.name, "Final Fantasy IV Advance (Europe) (En,Fr,De,Es,It)");
    assert!(alternatives
        .iter()
        .any(|c| c.name.contains("Sound Restoration hack")));
    assert_eq!(it.note, Some(PlanNote::UnreleasedVariant));
    assert_eq!(plan.totals.auto, 0);
}

#[test]
fn contra_advance_beta_goes_to_review() {
    let (_dir, plan) = plan_for(&["054 Contra Advance"]);
    let it = item(&plan, "054 Contra Advance");
    let PlanAction::Review { chosen, .. } = &it.action else {
        panic!("{:?}", it.action)
    };
    assert_eq!(chosen.name, "Contra Advance (USA) (Beta)");
    assert_eq!(it.note, Some(PlanNote::UnreleasedVariant));
}

#[test]
fn a_beta_rom_may_take_beta_art_automatically() {
    let (_dir, plan) = plan_for(&["070 Zelda (Beta)"]);
    let it = item(&plan, "070 Zelda (Beta)");
    assert!(matches!(it.action, PlanAction::Auto { .. }), "{:?}", it.action);
    assert_eq!(it.note, None);
}

#[test]
fn a_canonical_match_to_a_prototype_stays_auto() {
    let (_dir, plan) = plan_for(&["071 Dumped"]);
    assert!(matches!(
        item(&plan, "071 Dumped").action,
        PlanAction::Auto { .. }
    ));
}

#[test]
fn a_release_in_the_best_tier_still_wins_as_auto() {
    let (_dir, plan) = plan_for(&["072 Golden Sun"]);
    let it = item(&plan, "072 Golden Sun");
    let PlanAction::Auto { chosen, .. } = &it.action else {
        panic!("{:?}", it.action)
    };
    assert_eq!(chosen.name, "Golden Sun (USA, Europe)");
    assert_eq!(it.note, None);
}

#[test]
fn variant_tags_are_recognised() {
    for name in [
        "Contra Advance (USA) (Beta)",
        "Zelda (USA) (Proto 2)",
        "Tetris (Demo)",
        "Game (USA) (Sample)",
        "Final Fantasy IV - Sound Restoration hack [Hack by Bregalad]",
        "Sonic [h1]",
        "Sonic [h]",
        "Mario (Sound Restoration Hack)",
        "Action 52 (USA) (Unl)",
        "Game (Unlicensed)",
        "Pirates (Pirate)",
        "Game (Kiosk)",
        "Game (Debug Version)",
    ] {
        assert!(is_unreleased_variant(name), "{name}");
    }
    for name in [
        "Golden Sun (USA, Europe)",
        ".hack--Infection (USA)",
        "Hackers (USA)",
        "Demolition Man (USA)",
        "Shadow Hearts (Rev 1)",
        "Betrayal at Krondor",
        "Sonic [!]",
        "Unclosed (Beta",
    ] {
        assert!(!is_unreleased_variant(name), "{name}");
    }
}
```

- [ ] **Step 2: Run the tests to see them fail**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && cargo test -p coverly-core --test variants`
Expected: FAIL to compile — `is_unreleased_variant` and `PlanNote` do not exist.

- [ ] **Step 3: Implement the variant test in `matching`**

In `crates/coverly-core/src/matching/mod.rs`, replace the private `fn unreleased(c: &Candidate) -> bool { … }` with:

```rust
/// Words that mark a non-retail variant inside `(…)` or `[…]` (No-Intro
/// and GoodTools tags).
const VARIANT_WORDS: [&str; 11] = [
    "beta",
    "proto",
    "prototype",
    "demo",
    "sample",
    "hack",
    "pirate",
    "kiosk",
    "debug",
    "unl",
    "unlicensed",
];

/// Whether `name` is not a retail release: a beta, prototype, demo,
/// sample, hack, pirate, kiosk, debug or unlicensed variant — `(Beta)`,
/// `(Proto 2)`, `(Unl)`, GoodTools `[h1]`, `[Hack by …]`,
/// `(Sound Restoration Hack)`. Only bracketed tags count, so titles such
/// as `.hack` or `Hackers` are releases.
pub fn is_unreleased_variant(name: &str) -> bool {
    let lower = name.to_lowercase();
    bracket_groups(&lower).any(|g| {
        let g = g.trim();
        let goodtools_hack =
            g == "h" || (g.starts_with('h') && g.as_bytes().get(1).is_some_and(u8::is_ascii_digit));
        goodtools_hack
            || g
                .split(|c: char| !c.is_alphanumeric())
                .any(|w| VARIANT_WORDS.contains(&w))
    })
}

/// The text inside each closed `(…)` and `[…]` of `s` (not nested).
fn bracket_groups(s: &str) -> impl Iterator<Item = &str> {
    let mut rest = s;
    std::iter::from_fn(move || loop {
        let open = rest.find(['(', '['])?;
        let close = if rest.as_bytes()[open] == b'(' { ')' } else { ']' };
        let after = &rest[open + 1..];
        match after.find(close) {
            Some(end) => {
                rest = &after[end + 1..];
                return Some(&after[..end]);
            }
            // Unclosed: not a tag; look further on.
            None => rest = after,
        }
    })
}
```

and in `rank_with` replace both `unreleased(&cands[a])` / `unreleased(&cands[b])` with `is_unreleased_variant(&cands[a].name)` / `is_unreleased_variant(&cands[b].name)`.

- [ ] **Step 4: Implement the plan rule**

In `crates/coverly-core/src/plan/mod.rs`:

1. Imports: `use crate::matching::{default_region_order, is_unreleased_variant, strategy_for, RankContext, RegionPref, StrategyKind};`
2. After `NoneReason`, add:

```rust
/// Why an item needs a look although its match may be strong.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PlanNote {
    /// The best image is a beta, prototype, demo, hack or unlicensed
    /// variant while the ROM is not (M2-11, D22): never written as `auto`.
    UnreleasedVariant,
}
```

3. In `PlanItem`, after `replaces_existing`, add:

```rust
    /// Why this item is in review although its confidence may be high.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<PlanNote>,
```

4. In `build_plan`, the per-media `action` becomes a pair. Replace `let action = match &target { … };` and the `items.push(PlanItem { … })` with:

```rust
            let (action, note) = match &target {
                None => (
                    PlanAction::None {
                        reason: NoneReason::NoTarget,
                    },
                    None,
                ),
                Some(_) if cands.is_empty() => (
                    match g.errors.get(&media) {
                        Some(message) => PlanAction::None {
                            reason: NoneReason::SourceError {
                                message: message.clone(),
                            },
                        },
                        None => PlanAction::None {
                            reason: NoneReason::NoCandidates,
                        },
                    },
                    None,
                ),
                Some(_) => {
                    let ctx = RankContext {
                        rom_regions: &g.identity.parsed.regions,
                        regions: &opts.regions,
                        sources: &opts.sources,
                    };
                    let order = strategy.rank(&ctx, cands);
                    choose(cands, &order, opts, &t.stem)
                }
            };
```

and in the `PlanItem { … }` literal add `note,` after `replaces_existing: …,`.

5. Replace `fn choose` with:

```rust
/// Whether `c` is a non-retail variant matched only by title or fuzzily
/// for a ROM that is not itself such a variant (M2-11).
fn variant_needs_review(c: &Candidate, rom_stem: &str) -> bool {
    is_unreleased_variant(&c.name)
        && !matches!(c.kind, MatchKind::Canonical | MatchKind::NameExact)
        && !is_unreleased_variant(rom_stem)
}

/// Picks the first of `order` (falling back to the first candidate if the
/// strategy returned nothing) and classifies it by the thresholds. A
/// non-retail first pick is replaced by the first retail candidate that
/// reaches the review threshold, and is never `auto` (D22).
fn choose(
    cands: &[Candidate],
    order: &[usize],
    opts: &PlanOptions,
    rom_stem: &str,
) -> (PlanAction, Option<PlanNote>) {
    let mut order: Vec<usize> = order.iter().copied().filter(|&i| i < cands.len()).collect();
    if order.is_empty() {
        order = (0..cands.len()).collect();
    }
    let mut note = None;
    if variant_needs_review(&cands[order[0]], rom_stem) {
        note = Some(PlanNote::UnreleasedVariant);
        if let Some(pos) = order.iter().position(|&i| {
            !is_unreleased_variant(&cands[i].name) && confidence(&cands[i]) >= opts.review_threshold
        }) {
            let release = order.remove(pos);
            order.insert(0, release);
        }
    }
    let chosen = cands[order[0]].clone();
    let alternatives: Vec<Candidate> = order[1..]
        .iter()
        .take(opts.max_alternatives)
        .map(|&i| cands[i].clone())
        .collect();
    let c = confidence(&chosen);
    if c >= opts.auto_threshold && note.is_none() {
        (
            PlanAction::Auto {
                chosen,
                confidence: c,
                alternatives,
            },
            None,
        )
    } else if c >= opts.review_threshold {
        (
            PlanAction::Review {
                chosen,
                confidence: c,
                alternatives,
            },
            note,
        )
    } else {
        (
            PlanAction::None {
                reason: NoneReason::BelowThreshold { best: c },
            },
            None,
        )
    }
}
```

- [ ] **Step 5: Show the note in the CLI review list**

In `crates/coverly-cli/src/fill.rs`, `render_table`, replace the `PlanAction::Review { chosen, .. } => review.push(format!(…)),` arm with:

```rust
            PlanAction::Review { chosen, .. } => review.push(format!(
                "{}  -> {}  {}{}",
                item_label(item),
                chosen.name,
                kind_name(chosen.kind, chosen.score),
                if item.note.is_some() {
                    "  (beta/prototype/hack variant)"
                } else {
                    ""
                }
            )),
```

- [ ] **Step 6: Run the tests**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && cargo test -p coverly-core --test variants --test strategies --test plan && cargo test -p coverly-cli`
Expected: PASS (the existing `prototypes_and_betas_never_beat_a_release` and the CLI snapshot tests unchanged).

- [ ] **Step 7: Gates and commit**

Run all gates (Global Constraints). Then:

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && git add crates/coverly-core/src/matching/mod.rs crates/coverly-core/src/plan/mod.rs crates/coverly-core/tests/variants.rs crates/coverly-cli/src/fill.rs && git commit -m "fix(core): never pick hacks, betas or unlicensed variants as auto art"
```

---

### Task 2: Core — pause kinds, resumable journals and journaled art removal

**Files:**
- Modify: `crates/coverly-core/src/progress.rs` (`PauseKind`)
- Modify: `crates/coverly-core/src/net/mod.rs` (`NetError::pause_kind`)
- Modify: `crates/coverly-core/src/gather/mod.rs` (`Gathered.pause_kind`)
- Modify: `crates/coverly-core/src/plan/mod.rs` (`Plan.pause_kind`, `Plan::subset`, private `Plan::recount`)
- Modify: `crates/coverly-core/src/jobs/mod.rs` (`ApplyOptions.resume_job`, `JobReport.pause_kind`, `resumable_items`, `merge_resumed`, `recount`)
- Modify: `crates/coverly-core/src/write/mod.rs` (`Writer::resume`, `Writer::remove`, `RemoveOutcome`, `Op::Removed`, `WriteError::NotAFile`, undo of removals)
- Test: `crates/coverly-core/tests/journal_resume.rs`, `crates/coverly-core/tests/apply.rs` (appended tests)

**Interfaces:**
- Consumes: `Writer`, `undo_last`, `list_jobs`, `apply_plan` (existing).
- Produces:
  - `#[serde(rename_all = "kebab-case")] pub enum coverly_core::progress::PauseKind { RateLimit, Offline, Volume }` (`Clone, Copy, Debug, PartialEq, Eq, Serialize`)
  - `pub fn NetError::pause_kind(&self) -> Option<PauseKind>`
  - `Gathered.pause_kind`, `Plan.pause_kind`, `JobReport.pause_kind`: `Option<PauseKind>` (skipped in JSON when `None`)
  - `pub fn Plan::subset(&self, indices: &[usize]) -> Plan`
  - `ApplyOptions.resume_job: Option<String>` (default `None`)
  - `pub fn JobReport::resumable_items(&self) -> Vec<usize>`; `pub fn JobReport::merge_resumed(&mut self, indices: &[usize], other: JobReport)`
  - `pub fn Writer::resume(root: &Path, data_dir: &Path, job: &str) -> Result<Writer, WriteError>`
  - `pub fn Writer::remove(&mut self, target: &Path) -> Result<RemoveOutcome, WriteError>`
  - `#[serde(tag = "outcome", rename_all = "kebab-case")] pub enum RemoveOutcome { Removed { path: PathBuf, backup: PathBuf }, Missing }`
  - `WriteError::NotAFile(PathBuf)`

- [ ] **Step 1: Write the failing journal tests**

`crates/coverly-core/tests/journal_resume.rs`:

```rust
use std::fs;
use std::path::{Path, PathBuf};

use coverly_core::write::{list_jobs, undo_last, OverwritePolicy, RemoveOutcome, WriteError, Writer};
use tempfile::TempDir;

fn setup() -> (TempDir, TempDir, PathBuf) {
    let lib = TempDir::new().unwrap();
    let data = TempDir::new().unwrap();
    let root = lib.path().to_path_buf();
    fs::create_dir_all(root.join("ROMS/GBA/Imgs")).unwrap();
    (lib, data, root)
}

fn img(root: &Path, name: &str) -> PathBuf {
    root.join("ROMS/GBA/Imgs").join(name)
}

#[test]
fn resume_appends_to_the_same_journal_and_one_undo_reverts_both() {
    let (_lib, data, root) = setup();
    let mut w = Writer::begin(&root, data.path()).unwrap();
    w.write(&img(&root, "a.png"), b"a", OverwritePolicy::Never).unwrap();
    let job = w.finish().unwrap();
    let mut w = Writer::resume(&root, data.path(), &job).unwrap();
    assert_eq!(w.job_id(), job);
    w.write(&img(&root, "b.png"), b"b", OverwritePolicy::Never).unwrap();
    w.finish().unwrap();
    let jobs = list_jobs(data.path()).unwrap();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].entries, 2);
    assert!(jobs[0].complete);
    let undo = undo_last(data.path()).unwrap();
    assert_eq!(undo.removed.len(), 2);
    assert!(!img(&root, "a.png").exists() && !img(&root, "b.png").exists());
}

#[test]
fn resume_refuses_another_library_and_unknown_jobs() {
    let (_lib, data, root) = setup();
    let other = TempDir::new().unwrap();
    let job = Writer::begin(&root, data.path()).unwrap().finish().unwrap();
    assert!(matches!(
        Writer::resume(other.path(), data.path(), &job),
        Err(WriteError::Journal(_))
    ));
    assert!(Writer::resume(&root, data.path(), "../evil").is_err());
    assert!(Writer::resume(&root, data.path(), "20990101T000000Z-0000-0000").is_err());
}

#[test]
fn resume_cuts_a_torn_last_line() {
    let (_lib, data, root) = setup();
    let mut w = Writer::begin(&root, data.path()).unwrap();
    w.write(&img(&root, "a.png"), b"a", OverwritePolicy::Never).unwrap();
    let job = w.job_id().to_string();
    drop(w); // a crash: no `done` line
    let journal = data.path().join("jobs").join(format!("{job}.jsonl"));
    let mut text = fs::read_to_string(&journal).unwrap();
    text.push_str("{\"type\":\"entry\",\"tar");
    fs::write(&journal, text).unwrap();
    let mut w = Writer::resume(&root, data.path(), &job).unwrap();
    w.write(&img(&root, "b.png"), b"b", OverwritePolicy::Never).unwrap();
    w.finish().unwrap();
    assert_eq!(undo_last(data.path()).unwrap().removed.len(), 2);
}

#[test]
fn undo_restores_removed_art() {
    let (_lib, data, root) = setup();
    fs::write(img(&root, "a.png"), b"original").unwrap();
    let mut w = Writer::begin(&root, data.path()).unwrap();
    let out = w.remove(&img(&root, "a.png")).unwrap();
    let RemoveOutcome::Removed { backup, .. } = out else {
        panic!("{out:?}")
    };
    assert_eq!(fs::read(&backup).unwrap(), b"original");
    w.finish().unwrap();
    assert!(!img(&root, "a.png").exists());
    let undo = undo_last(data.path()).unwrap();
    assert_eq!(undo.restored.len(), 1);
    assert_eq!(fs::read(img(&root, "a.png")).unwrap(), b"original");
}

#[test]
fn undo_keeps_art_the_user_added_after_a_removal() {
    let (_lib, data, root) = setup();
    fs::write(img(&root, "a.png"), b"original").unwrap();
    let mut w = Writer::begin(&root, data.path()).unwrap();
    w.remove(&img(&root, "a.png")).unwrap();
    w.finish().unwrap();
    fs::write(img(&root, "a.png"), b"user art").unwrap();
    let undo = undo_last(data.path()).unwrap();
    assert_eq!(undo.skipped_modified.len(), 1);
    assert_eq!(fs::read(img(&root, "a.png")).unwrap(), b"user art");
}

#[test]
fn remove_skips_missing_files_and_never_removes_folders_or_outside_paths() {
    let (_lib, data, root) = setup();
    let outside = TempDir::new().unwrap();
    fs::write(outside.path().join("x.png"), b"x").unwrap();
    fs::create_dir(img(&root, "dir.png")).unwrap();
    let mut w = Writer::begin(&root, data.path()).unwrap();
    assert_eq!(
        w.remove(&img(&root, "none.png")).unwrap(),
        RemoveOutcome::Missing
    );
    assert!(matches!(
        w.remove(&img(&root, "dir.png")),
        Err(WriteError::NotAFile(_))
    ));
    assert!(matches!(
        w.remove(&outside.path().join("x.png")),
        Err(WriteError::OutsideRoot(_))
    ));
    assert!(img(&root, "dir.png").is_dir() && outside.path().join("x.png").exists());
}
```

- [ ] **Step 2: Append the failing apply tests**

In `crates/coverly-core/tests/apply.rs` change the import `use coverly_core::progress::NoProgress;` to `use coverly_core::progress::{NoProgress, PauseKind};` and `use coverly_core::write::undo_last;` to `use coverly_core::write::{list_jobs, undo_last};`, then append:

```rust
#[test]
fn a_rate_limited_gather_reports_its_pause_kind() {
    let w = world();
    w.server
        .mock("/", MockResponse::status(429).header("Retry-After", "0"));
    let p = plan(&w);
    assert!(p.paused.is_some());
    assert_eq!(p.pause_kind, Some(PauseKind::RateLimit));
}

#[test]
fn a_paused_job_resumes_into_the_same_journal() {
    let w = world();
    let p = plan(&w);
    let path = "/Nintendo%20-%20Game%20Boy%20Advance/Named_Boxarts/Golden%20Adventure%20(USA).png";
    w.server
        .mock(path, MockResponse::status(429).header("Retry-After", "0"));
    let opts = ApplyOptions {
        concurrency: 1,
        ..ApplyOptions::default()
    };
    let mut r = apply(&w, &p, &opts, false);
    assert_eq!(r.pause_kind, Some(PauseKind::RateLimit));
    let todo = r.resumable_items();
    assert_eq!(todo, vec![1, 2, 3]);
    w.server
        .mock(path, MockResponse::ok(b"<html>oops</html>".to_vec()));
    let rest = p.subset(&todo);
    assert_eq!(rest.items.len(), 3);
    let again = apply(
        &w,
        &rest,
        &ApplyOptions {
            resume_job: r.job_id.clone(),
            ..opts
        },
        false,
    );
    assert_eq!(again.job_id, r.job_id);
    r.merge_resumed(&todo, again);
    assert_eq!(r.paused, None);
    assert_eq!(r.pause_kind, None);
    assert!(matches!(status(&r, "002"), ItemStatus::Filled { .. }));
    assert_eq!(r.filled, 3, "{r:?}");
    assert_eq!(list_jobs(w.data.path()).unwrap().len(), 1);
    assert_eq!(undo_last(w.data.path()).unwrap().removed.len(), 3);
}

#[test]
fn going_offline_reports_the_offline_kind() {
    let w = world();
    let mut p = plan(&w);
    for item in &mut p.items {
        if let PlanAction::Auto {
            chosen,
            alternatives,
            ..
        }
        | PlanAction::Review {
            chosen,
            alternatives,
            ..
        } = &mut item.action
        {
            for c in std::iter::once(chosen).chain(alternatives.iter_mut()) {
                c.url = "http://127.0.0.1:9/gone.png".into();
                c.fallback_url = None;
            }
        }
    }
    let r = apply(
        &w,
        &p,
        &ApplyOptions {
            concurrency: 1,
            ..ApplyOptions::default()
        },
        false,
    );
    assert_eq!(r.pause_kind, Some(PauseKind::Offline));
}

#[cfg(unix)]
#[test]
fn a_read_only_volume_reports_the_volume_kind() {
    use std::os::unix::fs::PermissionsExt;
    let w = world();
    let p = plan(&w);
    let imgs = w.lib.path().join("ROMS/GBA/Imgs");
    fs::create_dir_all(&imgs).unwrap();
    fs::set_permissions(&imgs, fs::Permissions::from_mode(0o555)).unwrap();
    let r = apply(
        &w,
        &p,
        &ApplyOptions {
            concurrency: 1,
            ..ApplyOptions::default()
        },
        false,
    );
    fs::set_permissions(&imgs, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(r.pause_kind, Some(PauseKind::Volume));
    // The item that met the read-only folder is retried on resume.
    assert!(r.resumable_items().contains(&0));
}
```

(Plan order is `001 Golden Sun`, `002 Golden Adventure`, `003 Missing Game`, `004 Gldn Rsh`: with one worker, 001 is written, 002 meets the 429 and the rest are not attempted. On resume 002 falls back from the HTML page to the Japanese box, 003 is `no-art` and 004 (review, included by default) is written.)

- [ ] **Step 3: Run the tests to see them fail**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && cargo test -p coverly-core --test journal_resume --test apply`
Expected: FAIL to compile — `Writer::resume`, `Writer::remove`, `RemoveOutcome`, `PauseKind`, `pause_kind`, `subset`, `resumable_items`, `merge_resumed`, `resume_job` do not exist.

- [ ] **Step 4: Pause kinds**

`crates/coverly-core/src/progress.rs`, after `Phase`:

```rust
/// Why a phase or a job paused (spec §10). The app shows a reason and a
/// Resume button.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PauseKind {
    /// HTTP 403 / 429 from a source.
    RateLimit,
    /// Offline, unreachable or timed out after the retries.
    Offline,
    /// The library volume is full, read-only or gone.
    Volume,
}
```

`crates/coverly-core/src/net/mod.rs`, inside `impl NetError` after `pauses`:

```rust
    /// The kind of pause this error causes, if it pauses (see [`pauses`](Self::pauses)).
    pub fn pause_kind(&self) -> Option<crate::progress::PauseKind> {
        use crate::progress::PauseKind;
        match self {
            NetError::RateLimited { .. } => Some(PauseKind::RateLimit),
            NetError::Offline(_) | NetError::Timeout => Some(PauseKind::Offline),
            _ => None,
        }
    }
```

`crates/coverly-core/src/gather/mod.rs`:
- import `use crate::progress::{PauseKind, Phase, ProgressEvent, ProgressSink};`
- `Gathered` gains, after `paused`:

```rust
    /// The kind of `paused`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pause_kind: Option<PauseKind>,
```

- in `gather`, the identification arm becomes:

```rust
            Err(DatError::Net(e)) if e.pauses() => {
                out.paused = Some(e.to_string());
                out.pause_kind = e.pause_kind();
                break;
            }
```

- `let mut paused = None;` holds `(String, Option<PauseKind>)`: the source arm becomes `paused = Some((e.to_string(), e.pause_kind()));`, the "paused: {reason}" block becomes `if let Some((reason, _)) = &paused {`, and the end of the loop becomes:

```rust
        out.items.push(item);
        if let Some((reason, kind)) = paused {
            out.paused = Some(reason);
            out.pause_kind = kind;
            break;
        }
```

`crates/coverly-core/src/plan/mod.rs`:
- import `use crate::progress::PauseKind;`
- `Plan` gains, after `paused`:

```rust
    /// The kind of `paused`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pause_kind: Option<PauseKind>,
```

- `build_plan` sets `pause_kind: gathered.pause_kind,`.
- add:

```rust
impl Plan {
    /// A plan with only `indices` (in that order; out-of-range ones are
    /// skipped) and recomputed counts: what a resumed job still has to do.
    pub fn subset(&self, indices: &[usize]) -> Plan {
        let mut plan = Plan {
            items: indices
                .iter()
                .filter_map(|&n| self.items.get(n).cloned())
                .collect(),
            ..self.clone()
        };
        plan.recount();
        plan
    }

    fn recount(&mut self) {
        let mut totals = Counts::default();
        let mut per_system = BTreeMap::<String, Counts>::new();
        for item in &self.items {
            totals.add(&item.action);
            per_system
                .entry(item.folder.clone())
                .or_default()
                .add(&item.action);
        }
        self.totals = totals;
        self.per_system = per_system;
    }
}
```

- [ ] **Step 5: Resumable apply**

`crates/coverly-core/src/jobs/mod.rs`:
- import `use crate::progress::{PauseKind, Phase, ProgressEvent, ProgressSink};`
- `ApplyOptions` gains `pub resume_job: Option<String>,` (doc: "Continue this paused job's journal instead of starting a new one (D21)."), and `resume_job: None` in `Default`.
- `JobReport` gains, after `paused`:

```rust
    /// The kind of `paused`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pause_kind: Option<PauseKind>,
```

- in `apply_plan` the writer becomes:

```rust
        let started = match &opts.resume_job {
            Some(job) => Writer::resume(&plan.root, data_dir, job)?,
            None => Writer::begin(&plan.root, data_dir)?,
        };
        let writer = Mutex::new(started);
```

- `let stop: Mutex<Option<(String, PauseKind)>> = Mutex::new(None);` and after the workers:

```rust
        let stopped = stop.into_inner().unwrap_or_else(|p| p.into_inner());
        report.paused = stopped.as_ref().map(|(why, _)| why.clone());
        report.pause_kind = stopped.map(|(_, kind)| kind);
```

- the final loop only pushes items, then recounts:

```rust
    for (item, st) in plan.items.iter().zip(results) {
        report.items.push(ItemResult {
            folder: item.folder.clone(),
            rel_path: item.rel_path.clone(),
            media: item.media,
            target: item.target.clone(),
            status: st.unwrap_or(ItemStatus::NotAttempted),
        });
    }
    report.recount();
    Ok(report)
```

- `Ctx.stop: &'a Mutex<Option<(String, PauseKind)>>`, and:

```rust
    fn pause(&self, why: String, kind: PauseKind) {
        lock(self.stop).get_or_insert((why, kind));
    }
```

  with the call sites `self.pause(e.to_string(), e.pause_kind().unwrap_or(PauseKind::Offline));` (fetch) and `self.pause(format!("the library volume is full or read-only: {e}"), PauseKind::Volume);` (write).
- add:

```rust
impl JobReport {
    /// Items a resumed job must still try: never attempted, or stopped by
    /// a full or read-only volume.
    pub fn resumable_items(&self) -> Vec<usize> {
        self.items
            .iter()
            .enumerate()
            .filter(|(_, i)| {
                matches!(
                    i.status,
                    ItemStatus::NotAttempted
                        | ItemStatus::Failed {
                            kind: ItemErrorKind::WriteDenied | ItemErrorKind::DiskFull,
                            ..
                        }
                )
            })
            .map(|(n, _)| n)
            .collect()
    }

    /// Puts the outcomes of a resumed run (`other.items[k]` is item
    /// `indices[k]` of this report) into this report and recounts.
    pub fn merge_resumed(&mut self, indices: &[usize], other: JobReport) {
        for (&n, item) in indices.iter().zip(other.items) {
            if let Some(slot) = self.items.get_mut(n) {
                *slot = item;
            }
        }
        self.paused = other.paused;
        self.pause_kind = other.pause_kind;
        self.cancelled = other.cancelled;
        if self.job_id.is_none() {
            self.job_id = other.job_id;
        }
        self.recount();
    }

    fn recount(&mut self) {
        let (mut filled, mut replaced, mut skipped, mut no_art, mut failed) = (0, 0, 0, 0, 0);
        for item in &self.items {
            match &item.status {
                ItemStatus::Filled { .. } => filled += 1,
                ItemStatus::Replaced { .. } => replaced += 1,
                ItemStatus::Failed {
                    kind: ItemErrorKind::NoArt,
                    ..
                } => no_art += 1,
                ItemStatus::Failed { .. } => failed += 1,
                _ => skipped += 1,
            }
        }
        self.filled = filled;
        self.replaced = replaced;
        self.skipped = skipped;
        self.no_art = no_art;
        self.failed = failed;
    }
}
```

- [ ] **Step 6: Journal resume and removal**

`crates/coverly-core/src/write/mod.rs`:
- `WriteError` gains `#[error("not a regular file: {0}")] NotAFile(PathBuf),`.
- `Op` gains `Removed { backup: PathBuf },`.
- after `WriteOutcome`:

```rust
/// Result of one [`Writer::remove`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub enum RemoveOutcome {
    Removed { path: PathBuf, backup: PathBuf },
    /// Nothing at the target.
    Missing,
}
```

- in `impl Writer`, after `begin`:

```rust
    /// Continues the journal of job `job` (paused earlier), so one undo
    /// reverts every run of it (D21). The journal must exist, not be
    /// undone and belong to `root`. A torn last line is cut off first.
    pub fn resume(root: &Path, data_dir: &Path, job: &str) -> Result<Writer, WriteError> {
        if !valid_job_id(job) {
            return Err(WriteError::Journal(format!("not a job id: {job}")));
        }
        let path = data_dir.join("jobs").join(format!("{job}.jsonl"));
        let text = fs::read_to_string(&path).map_err(|e| map_io(e, &path))?;
        if !text.is_empty() && !text.ends_with('\n') {
            let keep = text.rfind('\n').map_or(0, |i| i + 1);
            let file = OpenOptions::new()
                .write(true)
                .open(&path)
                .map_err(|e| map_io(e, &path))?;
            file.set_len(keep as u64).map_err(|e| map_io(e, &path))?;
            file.sync_all()?;
        }
        let (header, entries, _) = parse(&path)?;
        let Line::Header {
            root: journal_root, ..
        } = header
        else {
            unreachable!("parse returns a header first")
        };
        if journal_root != root {
            return Err(WriteError::Journal(format!(
                "{} belongs to {}",
                path.display(),
                journal_root.display()
            )));
        }
        let journal = OpenOptions::new()
            .append(true)
            .open(&path)
            .map_err(|e| map_io(e, &path))?;
        Ok(Writer {
            root: root.to_path_buf(),
            job: job.to_string(),
            backups: data_dir.join("backups").join(job),
            journal,
            counter: u32::try_from(entries.len()).unwrap_or(u32::MAX),
        })
    }
```

- after `write`:

```rust
    /// Deletes the art file `target`: backed up first and journaled before
    /// the delete, so undo restores it. A missing target is
    /// [`RemoveOutcome::Missing`]; folders and symlinks are never removed.
    pub fn remove(&mut self, target: &Path) -> Result<RemoveOutcome, WriteError> {
        let target = ensure_within(&self.root, target)?;
        let meta = match fs::symlink_metadata(&target) {
            Ok(meta) => meta,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(RemoveOutcome::Missing),
            Err(e) => return Err(map_io(e, &target)),
        };
        if !meta.file_type().is_file() {
            return Err(WriteError::NotAFile(target));
        }
        let bytes = fs::read(&target).map_err(|e| map_io(e, &target))?;
        let backup = self.back_up(&target)?;
        self.append(&Line::Entry {
            target: target.clone(),
            op: Op::Removed {
                backup: backup.clone(),
            },
            sha1: sha1_hex(&bytes),
        })?;
        fs::remove_file(&target).map_err(|e| map_io(e, &target))?;
        Ok(RemoveOutcome::Removed {
            path: target,
            backup,
        })
    }
```

- in `undo_journal`, the `match (op, current)` becomes (the two new arms before the generic SHA-1 arm):

```rust
        match (op, current) {
            // Already gone: nothing to remove.
            (Op::Created, None) => {}
            (Op::Replaced { .. }, None) => report.skipped_modified.push(target),
            (Op::Removed { backup }, None) => {
                let backup = ensure_within(&backups, &backup)?;
                let old = fs::read(&backup).map_err(|e| map_io(e, &backup))?;
                if let Some(parent) = target.parent() {
                    remove_apple_doubles(&create_dirs(parent)?);
                }
                let tmp = format!(".coverly-undo-{}-{n}.tmp", journal.job);
                atomic_write(&target, &old, &tmp)?;
                report.restored.push(target);
            }
            // The delete never happened (crash between journal and delete).
            (Op::Removed { .. }, Some(bytes)) if sha1_hex(&bytes) == sha1 => {}
            (Op::Removed { .. }, Some(_)) => report.skipped_modified.push(target),
            (_, Some(bytes)) if sha1_hex(&bytes) != sha1 => report.skipped_modified.push(target),
            (Op::Created, Some(_)) => {
                fs::remove_file(&target).map_err(|e| map_io(e, &target))?;
                report.removed.push(target);
            }
            (Op::Replaced { backup }, Some(_)) => {
                let backup = ensure_within(&backups, &backup)?;
                let old = fs::read(&backup).map_err(|e| map_io(e, &backup))?;
                let tmp = format!(".coverly-undo-{}-{n}.tmp", journal.job);
                atomic_write(&target, &old, &tmp)?;
                report.restored.push(target);
            }
        }
```

- [ ] **Step 7: Run the tests**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && cargo test -p coverly-core && cargo test -p coverly-cli`
Expected: PASS (CLI JSON snapshots unchanged: `pause_kind` is skipped when `None`).

- [ ] **Step 8: Gates and commit**

Run all gates. Then:

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && git add crates/coverly-core && git commit -m "feat(core): resumable job journals, pause kinds and journaled art removal"
```

---
### Task 3: Core — the `fill` facade, review decisions, and the CLI on top of it

**Files:**
- Create: `crates/coverly-core/src/fill/mod.rs`
- Modify: `crates/coverly-core/src/lib.rs` (`pub mod fill;`)
- Modify: `crates/coverly-core/src/plan/mod.rs` (`Decision`, `Plan::decide`, `NoneReason::Skipped`)
- Modify: `crates/coverly-cli/src/fill.rs` (`run` uses `FillEnv`; `none_reason` handles `Skipped`; `KEEP_JOBS` and the `prune_jobs` call move into the core)
- Test: `crates/coverly-core/tests/fill_env.rs`

**Interfaces:**
- Consumes: Task 2 (`ApplyOptions.resume_job`, `Writer::remove`, `RemoveOutcome`), `gather`, `build_plan`, `apply_plan`, `ArtStore`, `LibretroSource`, `DatStore`, `Identifier`, `process_image`.
- Produces:
  - `pub const coverly_core::fill::KEEP_JOBS: usize = 10;`
  - `pub struct Endpoints { pub libretro: String, pub libretro_raw: String, pub database: String }` (`Clone, Debug, PartialEq, Eq, Default` = the real servers)
  - `pub struct FillSettings { pub systems: Vec<String>, pub media: Vec<Media>, pub scope: Scope, pub strategy: StrategyKind, pub regions: Vec<RegionPref>, pub match_method: MatchMethod, pub refresh: bool }` (`Clone, Debug, Default` = cover, missing, rom-region, default regions, hash-then-name)
  - `pub struct FillEnv<'a> { pub cache: &'a Cache, pub http: &'a dyn Http, pub endpoints: &'a Endpoints, pub cache_dir: &'a Path, pub data_dir: &'a Path }` with
    - `pub fn plan(&self, root: &Path, profile: &Profile, systems: &SystemDb, art: &[SystemArt], settings: &FillSettings, progress: &dyn ProgressSink, cancel: &AtomicBool) -> Plan`
    - `pub fn apply(&self, plan: &Plan, profile: &Profile, opts: &ApplyOptions, progress: &dyn ProgressSink, cancel: &AtomicBool) -> Result<JobReport, WriteError>` (prunes to `KEEP_JOBS`)
    - `pub fn candidate_image(&self, profile: &Profile, c: &Candidate) -> Result<ProcessedImage, ArtError>`
  - `pub fn write_local_image(root: &Path, profile: &Profile, media: Media, target: &Path, bytes: &[u8], data_dir: &Path) -> Result<(Option<String>, WriteOutcome), ArtError>`
  - `pub fn remove_art(root: &Path, target: &Path, data_dir: &Path) -> Result<(Option<String>, RemoveOutcome), WriteError>`
  - The job id is `None` when nothing was written or removed: a refused or no-op manual write leaves **no** journal (otherwise `undo_last` undoes the empty phantom instead of the real last job, and failed clicks prune real journals and their backups). `Writer::{has_entries, discard}` support this.
  - `pub enum ArtError { NoSlot, UnknownSource(String), Source(String), Paused(String), Image(ImageError), Write(WriteError) }` (`Debug, thiserror::Error`)
  - `#[serde(tag = "choice", rename_all = "kebab-case")] pub enum coverly_core::plan::Decision { Accept, Pick { candidate: usize }, Skip }` (`Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize`); `candidate` 0 is the chosen image, `n ≥ 1` is `alternatives[n - 1]`
  - `pub fn Plan::decide(&mut self, decisions: &BTreeMap<usize, Decision>)`
  - `NoneReason::Skipped` (`{"reason":"skipped"}`)

- [ ] **Step 1: Write the failing tests**

`crates/coverly-core/tests/fill_env.rs`:

```rust
mod common;

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use coverly_core::art::resolve_art;
use coverly_core::cache::Cache;
use coverly_core::fill::{remove_art, write_local_image, ArtError, Endpoints, FillEnv, FillSettings};
use coverly_core::image::probe_dimensions;
use coverly_core::jobs::{ApplyOptions, ItemStatus};
use coverly_core::net::testing::{png_bytes, MockServer};
use coverly_core::net::{NetConfig, PoliteHttp, UreqHttp};
use coverly_core::plan::{Decision, NoneReason, Plan, PlanAction};
use coverly_core::profiles::Media;
use coverly_core::progress::NoProgress;
use coverly_core::scan::{scan_library, ScanOptions};
use coverly_core::systems::SystemDb;
use coverly_core::write::{undo_last, RemoveOutcome, WriteOutcome};
use tempfile::TempDir;

const GBA: &str = "Nintendo - Game Boy Advance";
const BOX: &str = "Named_Boxarts";
const SUN: &str = "Golden Sun";
const RUSH: &str = "Gldn Rsh";

struct World {
    lib: TempDir,
    cache_dir: TempDir,
    data: TempDir,
    server: MockServer,
    http: PoliteHttp<UreqHttp>,
    cache: Cache,
    endpoints: Endpoints,
}

fn world() -> World {
    let lib = TempDir::new().unwrap();
    let r = lib.path();
    common::mkdir(r, "anbernic");
    common::write(r, &format!("ROMS/GBA/{SUN}.gba"), "golden-sun-bytes");
    common::write(r, &format!("ROMS/GBA/{RUSH}.gba"), "rush");
    let server = MockServer::start();
    server.serve_libretro_root(&[GBA]);
    server.serve_libretro_index(
        GBA,
        BOX,
        &[
            "Golden Sun (USA, Europe).png",
            "Golden Sun (Japan).png",
            "Golden Rush (USA).png",
        ],
    );
    server.serve_libretro_image(GBA, BOX, "Golden Sun (USA, Europe).png", png_bytes(1000, 1400));
    server.serve_libretro_image(GBA, BOX, "Golden Sun (Japan).png", png_bytes(500, 700));
    server.serve_libretro_image(GBA, BOX, "Golden Rush (USA).png", png_bytes(200, 200));
    let cfg = NetConfig {
        retries: 0,
        ..NetConfig::default()
    };
    World {
        endpoints: Endpoints {
            libretro: server.base(),
            libretro_raw: server.url("/raw"),
            database: server.base(),
        },
        http: PoliteHttp::new(UreqHttp::new(&cfg), &cfg),
        cache: Cache::in_memory().unwrap(),
        lib,
        cache_dir: TempDir::new().unwrap(),
        data: TempDir::new().unwrap(),
        server,
    }
}

impl World {
    fn env(&self) -> FillEnv<'_> {
        FillEnv {
            cache: &self.cache,
            http: &self.http,
            endpoints: &self.endpoints,
            cache_dir: self.cache_dir.path(),
            data_dir: self.data.path(),
        }
    }

    fn plan(&self) -> Plan {
        let profile = common::profile("anbernic-stock");
        let db = SystemDb::builtin();
        let scan = scan_library(self.lib.path(), &profile, &db, &ScanOptions::default());
        let art = resolve_art(&scan, &profile, &db, &[Media::Cover]);
        self.env().plan(
            self.lib.path(),
            &profile,
            &db,
            &art,
            &FillSettings::default(),
            &NoProgress,
            &AtomicBool::new(false),
        )
    }

    fn img(&self, stem: &str) -> PathBuf {
        self.lib.path().join(format!("ROMS/GBA/Imgs/{stem}.png"))
    }
}

fn at(plan: &Plan, stem: &str) -> usize {
    plan.items.iter().position(|i| i.stem == stem).unwrap()
}

#[test]
fn plan_is_a_dry_run_with_auto_and_review_items() {
    let w = world();
    let p = w.plan();
    assert!(matches!(p.items[at(&p, SUN)].action, PlanAction::Auto { .. }));
    assert!(matches!(p.items[at(&p, RUSH)].action, PlanAction::Review { .. }));
    assert!(!w.lib.path().join("ROMS/GBA/Imgs").exists());
}

#[test]
fn decisions_turn_items_into_auto_or_skipped() {
    let w = world();
    let mut p = w.plan();
    let (sun, rush) = (at(&p, SUN), at(&p, RUSH));
    p.decide(&BTreeMap::from([
        (rush, Decision::Accept),
        (sun, Decision::Pick { candidate: 1 }),
    ]));
    assert!(matches!(p.items[rush].action, PlanAction::Auto { .. }));
    let PlanAction::Auto {
        chosen,
        alternatives,
        ..
    } = &p.items[sun].action
    else {
        panic!("{:?}", p.items[sun].action)
    };
    assert_eq!(chosen.name, "Golden Sun (Japan)");
    assert!(alternatives.is_empty(), "a picked image never falls back to another");
    assert_eq!(p.totals.auto, 2);

    let mut skipped = w.plan();
    skipped.decide(&BTreeMap::from([(rush, Decision::Skip), (99, Decision::Accept)]));
    assert!(matches!(
        skipped.items[rush].action,
        PlanAction::None {
            reason: NoneReason::Skipped
        }
    ));
    assert_eq!(skipped.totals.none, 1);
}

#[test]
fn apply_writes_the_picked_candidate() {
    let w = world();
    let mut p = w.plan();
    let (sun, rush) = (at(&p, SUN), at(&p, RUSH));
    p.decide(&BTreeMap::from([
        (sun, Decision::Pick { candidate: 1 }),
        (rush, Decision::Skip),
    ]));
    let r = w
        .env()
        .apply(
            &p,
            &common::profile("anbernic-stock"),
            &ApplyOptions {
                include_review: false,
                ..ApplyOptions::default()
            },
            &NoProgress,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert!(
        matches!(&r.items[sun].status, ItemStatus::Filled { name, .. } if name == "Golden Sun (Japan)")
    );
    assert_eq!(r.items[rush].status, ItemStatus::SkippedNoMatch);
    let (wd, ht) = probe_dimensions(&fs::read(w.img(SUN)).unwrap()).unwrap();
    assert!(wd <= 250 && ht <= 360, "{wd}x{ht}");
}

#[test]
fn candidate_image_is_re_encoded_for_the_slot_and_downloaded_once() {
    let w = world();
    let p = w.plan();
    let PlanAction::Auto { chosen, .. } = &p.items[at(&p, SUN)].action else {
        panic!()
    };
    let profile = common::profile("anbernic-stock");
    let img = w.env().candidate_image(&profile, chosen).unwrap();
    assert!(img.width <= 250 && img.height <= 360);
    assert_eq!(&img.bytes[..8], b"\x89PNG\r\n\x1a\n");
    w.env().candidate_image(&profile, chosen).unwrap();
    assert_eq!(
        w.server.hits(
            "/Nintendo%20-%20Game%20Boy%20Advance/Named_Boxarts/Golden%20Sun%20(USA%2C%20Europe).png"
        ),
        1
    );
}

#[test]
fn a_local_image_is_re_encoded_backed_up_and_undoable() {
    let w = world();
    let target = w.img(SUN);
    common::write(w.lib.path(), &format!("ROMS/GBA/Imgs/{SUN}.png"), "old art");
    let (job, outcome) = write_local_image(
        w.lib.path(),
        &common::profile("anbernic-stock"),
        Media::Cover,
        &target,
        &png_bytes(800, 800),
        w.data.path(),
    )
    .unwrap();
    assert!(job.is_some());
    assert!(matches!(outcome, WriteOutcome::Replaced { .. }), "{outcome:?}");
    let (wd, ht) = probe_dimensions(&fs::read(&target).unwrap()).unwrap();
    assert!(wd <= 250 && ht <= 360);
    undo_last(w.data.path()).unwrap();
    assert_eq!(fs::read_to_string(&target).unwrap(), "old art");
}

#[test]
fn a_local_file_that_is_not_an_image_is_refused_before_writing() {
    let w = world();
    let target = w.img(SUN);
    let r = write_local_image(
        w.lib.path(),
        &common::profile("anbernic-stock"),
        Media::Cover,
        &target,
        b"<html>not an image</html>",
        w.data.path(),
    );
    assert!(matches!(r, Err(ArtError::Image(_))), "{r:?}");
    assert!(!target.exists());
    assert!(matches!(
        write_local_image(
            w.lib.path(),
            &common::profile("anbernic-stock"),
            Media::Screenshot,
            &target,
            &png_bytes(10, 10),
            w.data.path(),
        ),
        Err(ArtError::NoSlot)
    ));
}

#[test]
fn removed_art_is_backed_up_and_undo_brings_it_back() {
    let w = world();
    let target = w.img(SUN);
    common::write(w.lib.path(), &format!("ROMS/GBA/Imgs/{SUN}.png"), "old art");
    let (_, out) = remove_art(w.lib.path(), &target, w.data.path()).unwrap();
    assert!(matches!(out, RemoveOutcome::Removed { .. }));
    assert!(!target.exists());
    undo_last(w.data.path()).unwrap();
    assert_eq!(fs::read_to_string(&target).unwrap(), "old art");
}
```

(The stems are **not numbered**: title matching does not strip a numbered prefix yet (tracker M2-13), so `001 Golden Sun` would be a fuzzy `review` (score 71) even with a DAT, and `002 Gldn Rsh` would get no candidates. `Golden Sun` has no DAT on the mock server, so it is matched by title: `Golden Sun (USA, Europe)` and `(Japan)` are both `title-exact`, `rom-region` with no ROM region puts USA/Europe first, so candidate 1 is the Japanese box. `Gldn Rsh` is a fuzzy match to `Golden Rush (USA)` (score 72), a `review` item. The as-built file also tests that a refused or no-op `write_local_image`/`remove_art` leaves `list_jobs` unchanged and `undo_last` still reverts the real job, and that a cancel is `report.cancelled` but leaves no mark on the plan.)

- [ ] **Step 2: Run the tests to see them fail**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && cargo test -p coverly-core --test fill_env`
Expected: FAIL to compile — module `coverly_core::fill`, `Decision`, `Plan::decide`, `NoneReason::Skipped` do not exist.

- [ ] **Step 3: Review decisions on the plan**

In `crates/coverly-core/src/plan/mod.rs`:
- import `use serde::{Deserialize, Serialize};`
- `NoneReason` gains `/// The user skipped it in the review queue. Skipped,`
- add:

```rust
/// What the user decided for one item in the review queue.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "choice", rename_all = "kebab-case")]
pub enum Decision {
    /// Write the chosen image (falling back to the alternatives).
    Accept,
    /// Write this image only: 0 is the chosen one, `n` is `alternatives[n - 1]`.
    Pick { candidate: usize },
    /// Write nothing.
    Skip,
}
```

- inside `impl Plan` (added in Task 2) add:

```rust
    /// Applies review decisions: an accepted or picked item becomes `auto`
    /// (with the confidence of its image), a skipped one `none`
    /// ([`NoneReason::Skipped`]). Unknown indices, `none` items and picks
    /// past the last alternative are ignored.
    pub fn decide(&mut self, decisions: &BTreeMap<usize, Decision>) {
        for (&n, decision) in decisions {
            let Some(item) = self.items.get_mut(n) else {
                continue;
            };
            let (chosen, alternatives) = match &item.action {
                PlanAction::Auto {
                    chosen,
                    alternatives,
                    ..
                }
                | PlanAction::Review {
                    chosen,
                    alternatives,
                    ..
                } => (chosen.clone(), alternatives.clone()),
                PlanAction::None { .. } => continue,
            };
            item.action = match *decision {
                Decision::Accept => PlanAction::Auto {
                    confidence: confidence(&chosen),
                    chosen,
                    alternatives,
                },
                Decision::Pick { candidate } => {
                    let Some(c) = std::iter::once(&chosen)
                        .chain(alternatives.iter())
                        .nth(candidate)
                    else {
                        continue;
                    };
                    PlanAction::Auto {
                        confidence: confidence(c),
                        chosen: c.clone(),
                        alternatives: Vec::new(),
                    }
                }
                Decision::Skip => PlanAction::None {
                    reason: NoneReason::Skipped,
                },
            };
            item.note = None;
        }
        self.recount();
    }
```

- [ ] **Step 4: The `fill` facade**

`crates/coverly-core/src/fill/mod.rs`:

```rust
//! A whole fill behind one door (spec §7 steps 3–9), shared by
//! `coverly fill` and the desktop app: phase one + plan (the dry run),
//! phase two (apply, then prune old journals), the preview of one
//! candidate, and the manual writes of the detail drawer (a local image,
//! removing art). Every write is journaled, so `undo` reverts it.

use std::path::Path;
use std::sync::atomic::AtomicBool;

use crate::art::SystemArt;
use crate::cache::Cache;
use crate::gather::{gather, select_targets, GatherOptions, Scope, TargetFilter};
use crate::identify::dat::{DatStore, DATABASE_BASE};
use crate::identify::{Identifier, IdentifyOptions, MatchMethod};
use crate::image::{process_image, Fit, ImageError, ImageLimits, OutputSpec, ProcessedImage};
use crate::jobs::{apply_plan, ApplyOptions, ArtStore, JobReport};
use crate::matching::{default_region_order, RegionPref, StrategyKind};
use crate::net::Http;
use crate::plan::{build_plan, Plan, PlanOptions};
use crate::profiles::{Media, Profile};
use crate::progress::ProgressSink;
use crate::sources::libretro::{LibretroConfig, LibretroSource, LIBRETRO_BASE, LIBRETRO_RAW_BASE};
use crate::sources::{ArtSource, Candidate, SourceError};
use crate::systems::SystemDb;
use crate::write::{prune_jobs, OverwritePolicy, RemoveOutcome, WriteError, WriteOutcome, Writer};

/// Journals (and their backups) kept for undo (spec §9 undo history depth).
pub const KEEP_JOBS: usize = 10;

/// Where the sources and DATs live (tests point them at a mock server).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoints {
    pub libretro: String,
    pub libretro_raw: String,
    pub database: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Endpoints {
            libretro: LIBRETRO_BASE.to_string(),
            libretro_raw: LIBRETRO_RAW_BASE.to_string(),
            database: DATABASE_BASE.to_string(),
        }
    }
}

/// What one fill does (spec §9 defaults).
#[derive(Clone, Debug)]
pub struct FillSettings {
    /// System folders or ids (empty = all).
    pub systems: Vec<String>,
    pub media: Vec<Media>,
    pub scope: Scope,
    pub strategy: StrategyKind,
    pub regions: Vec<RegionPref>,
    pub match_method: MatchMethod,
    /// Ignore cached candidate lists and indexes.
    pub refresh: bool,
}

impl Default for FillSettings {
    fn default() -> Self {
        FillSettings {
            systems: Vec::new(),
            media: vec![Media::Cover],
            scope: Scope::Missing,
            strategy: StrategyKind::RomRegion,
            regions: default_region_order(),
            match_method: MatchMethod::HashThenName,
            refresh: false,
        }
    }
}

/// Why a candidate preview or a manual write failed.
#[derive(Debug, thiserror::Error)]
pub enum ArtError {
    #[error("this layout cannot store that media")]
    NoSlot,
    #[error("unknown image source `{0}`")]
    UnknownSource(String),
    #[error("{0}")]
    Source(String),
    #[error("{0}")]
    Paused(String),
    #[error(transparent)]
    Image(#[from] ImageError),
    #[error(transparent)]
    Write(#[from] WriteError),
}

impl From<SourceError> for ArtError {
    fn from(e: SourceError) -> Self {
        match e {
            SourceError::Net(n) if n.pauses() => ArtError::Paused(n.to_string()),
            other => ArtError::Source(other.to_string()),
        }
    }
}

/// What a fill needs from its host.
pub struct FillEnv<'a> {
    pub cache: &'a Cache,
    pub http: &'a dyn Http,
    pub endpoints: &'a Endpoints,
    /// Holds the art store (`<cache dir>/art`).
    pub cache_dir: &'a Path,
    /// Holds journals and backups.
    pub data_dir: &'a Path,
}

fn spec_for(profile: &Profile, media: Media) -> Result<OutputSpec, ArtError> {
    let slot = profile.slot(media).ok_or(ArtError::NoSlot)?;
    Ok(OutputSpec::for_slot(slot, Fit::Contain, 90))
}

impl FillEnv<'_> {
    fn libretro(&self, refresh: bool) -> LibretroSource<'_> {
        let mut cfg = LibretroConfig {
            base: self.endpoints.libretro.clone(),
            raw_base: self.endpoints.libretro_raw.clone(),
            ..LibretroConfig::default()
        };
        if refresh {
            cfg.ttl_secs = 0;
        }
        LibretroSource::new(self.http, self.cache, cfg)
    }

    /// Phase one and the plan of `art` (the dry run). Never writes to the
    /// library. A pause gives a partial plan (`plan.paused`). A cancel
    /// does not mark the plan: the caller checks its own `cancel` flag.
    #[allow(clippy::too_many_arguments)]
    pub fn plan(
        &self,
        root: &Path,
        profile: &Profile,
        systems: &SystemDb,
        art: &[SystemArt],
        settings: &FillSettings,
        progress: &dyn ProgressSink,
        cancel: &AtomicBool,
    ) -> Plan {
        let targets = select_targets(
            art,
            &TargetFilter {
                systems: settings.systems.clone(),
                media: settings.media.clone(),
                scope: settings.scope,
            },
        );
        let dats = DatStore::new(self.http, self.cache, self.endpoints.database.clone());
        let identifier = Identifier::new(
            self.cache,
            &dats,
            IdentifyOptions {
                method: settings.match_method,
                ..IdentifyOptions::default()
            },
        );
        let libretro = self.libretro(settings.refresh);
        let sources: Vec<&dyn ArtSource> = vec![&libretro];
        let gopts = GatherOptions {
            consensus: settings.strategy == StrategyKind::Popularity,
            refresh: settings.refresh,
            ..GatherOptions::default()
        };
        let gathered = gather(
            &targets,
            systems,
            &sources,
            &identifier,
            self.cache,
            &gopts,
            progress,
            cancel,
        );
        build_plan(
            root,
            profile,
            systems,
            &gathered,
            &PlanOptions {
                strategy: settings.strategy,
                regions: settings.regions.clone(),
                ..PlanOptions::default()
            },
        )
    }

    /// Phase two: applies `plan`, then keeps the newest [`KEEP_JOBS`] journals.
    pub fn apply(
        &self,
        plan: &Plan,
        profile: &Profile,
        opts: &ApplyOptions,
        progress: &dyn ProgressSink,
        cancel: &AtomicBool,
    ) -> Result<JobReport, WriteError> {
        let libretro = self.libretro(false);
        let sources: Vec<&dyn ArtSource> = vec![&libretro];
        let store = ArtStore::new(self.cache_dir, self.cache);
        let report = apply_plan(
            plan,
            profile,
            &sources,
            &store,
            self.data_dir,
            opts,
            progress,
            cancel,
        )?;
        // Pruning only frees space: failing to prune is not an error.
        let _ = prune_jobs(self.data_dir, KEEP_JOBS);
        Ok(report)
    }

    /// The image of `c` as it would be written (decoded with limits and
    /// re-encoded for its media's slot), through the art store: a preview
    /// downloads once and the later apply reuses it.
    pub fn candidate_image(
        &self,
        profile: &Profile,
        c: &Candidate,
    ) -> Result<ProcessedImage, ArtError> {
        let spec = spec_for(profile, c.media)?;
        let libretro = self.libretro(false);
        if c.source != libretro.id() {
            return Err(ArtError::UnknownSource(c.source.clone()));
        }
        let store = ArtStore::new(self.cache_dir, self.cache);
        let (bytes, cached) = match store.get(&c.url) {
            Some(bytes) => (bytes, true),
            None => (libretro.fetch(c)?, false),
        };
        let img = process_image(&bytes, &spec, &ImageLimits::default())?;
        if !cached {
            let _ = store.put(&c.url, &bytes);
        }
        Ok(img)
    }
}

/// Writes `bytes` (a local image the user picked: untrusted) as the
/// `media` art at `target`, in a journaled job of its own. The job id is
/// `None` when nothing was written: a refused or no-op write leaves no job.
pub fn write_local_image(
    root: &Path,
    profile: &Profile,
    media: Media,
    target: &Path,
    bytes: &[u8],
    data_dir: &Path,
) -> Result<(Option<String>, WriteOutcome), ArtError> {
    let spec = spec_for(profile, media)?;
    let img = process_image(bytes, &spec, &ImageLimits::default())?;
    precheck(root, target)?; // ensure_within + FAT-safe name, before any journal
    let mut writer = Writer::begin(root, data_dir)?;
    let outcome = writer.write(target, &img.bytes, OverwritePolicy::Backup);
    let job = close(writer, data_dir)?;
    Ok((job, outcome?))
}

/// Removes the art file `target` (backed up, journaled) in a job of its own.
pub fn remove_art(
    root: &Path,
    target: &Path,
    data_dir: &Path,
) -> Result<(Option<String>, RemoveOutcome), WriteError> {
    ensure_within(root, target)?;
    let mut writer = Writer::begin(root, data_dir)?;
    let outcome = writer.remove(target);
    let job = close(writer, data_dir)?;
    Ok((job, outcome?))
}

/// Finishes (then prunes) a job that changed the library; discards
/// (journal + backups deleted) one that did not.
fn close(writer: Writer, data_dir: &Path) -> Result<Option<String>, WriteError> {
    if !writer.has_entries() {
        writer.discard()?;
        return Ok(None);
    }
    let job = writer.finish()?;
    let _ = prune_jobs(data_dir, KEEP_JOBS);
    Ok(Some(job))
}
```

Add `pub mod fill;` to `crates/coverly-core/src/lib.rs` (alphabetical, after `pub mod cache;`… keep the list sorted: `art, audit, cache, fill, gather, …`).

- [ ] **Step 5: Run the core tests**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && cargo test -p coverly-core --test fill_env`
Expected: PASS.

- [ ] **Step 6: Move `coverly fill` onto the facade**

In `crates/coverly-cli/src/fill.rs`:
- Imports become:

```rust
use coverly_core::art::resolve_art;
use coverly_core::cache::{Cache, CACHE_FILE};
use coverly_core::fill::{Endpoints, FillEnv, FillSettings};
use coverly_core::gather::Scope;
use coverly_core::identify::dat::DATABASE_BASE;
use coverly_core::identify::MatchMethod;
use coverly_core::jobs::{ApplyOptions, ItemErrorKind, ItemStatus, JobReport};
use coverly_core::matching::{parse_region_order, StrategyKind};
use coverly_core::net::{NetConfig, PoliteHttp, UreqHttp};
use coverly_core::paths::AppDirs;
use coverly_core::plan::{NoneReason, Plan, PlanAction, PlanItem};
use coverly_core::profiles::Media;
use coverly_core::progress::{Phase, ProgressEvent, ProgressSink};
use coverly_core::scan::{scan_library, ScanOptions};
use coverly_core::sources::libretro::{LIBRETRO_BASE, LIBRETRO_RAW_BASE};
use coverly_core::sources::MatchKind;
use coverly_core::systems::SystemDb;
use coverly_core::write::OverwritePolicy;
```

- Delete `const KEEP_JOBS: usize = 10;`.
- In `run`, replace everything from `let db = SystemDb::builtin();` to the end of the function with:

```rust
    let db = SystemDb::builtin();
    let scan = scan_library(&root, profile, &db, &ScanOptions::default());
    let art = resolve_art(&scan, profile, &db, &media);

    let dirs =
        AppDirs::resolve(args.cache_dir.as_deref(), args.data_dir.as_deref()).ok_or_else(|| {
            fail("cannot find the app cache/data folders; pass --cache-dir and --data-dir")
        })?;
    std::fs::create_dir_all(&dirs.cache).map_err(|e| {
        fail(format!(
            "cannot create the cache folder {}: {e}",
            dirs.cache.display()
        ))
    })?;
    let cache = Cache::open(&dirs.cache.join(CACHE_FILE)).map_err(fail)?;
    let net = NetConfig {
        concurrency: args.concurrency,
        ..NetConfig::default()
    };
    let http = PoliteHttp::new(UreqHttp::new(&net), &net);
    let endpoints = Endpoints {
        libretro: args
            .libretro_url
            .clone()
            .unwrap_or_else(|| LIBRETRO_BASE.into()),
        libretro_raw: args
            .libretro_raw_url
            .clone()
            .unwrap_or_else(|| LIBRETRO_RAW_BASE.into()),
        database: args
            .database_url
            .clone()
            .unwrap_or_else(|| DATABASE_BASE.into()),
    };
    let env = FillEnv {
        cache: &cache,
        http: &http,
        endpoints: &endpoints,
        cache_dir: &dirs.cache,
        data_dir: &dirs.data,
    };
    let settings = FillSettings {
        systems: args.system.clone(),
        media: media.clone(),
        scope: args.scope.into(),
        strategy: args.strategy.into(),
        regions,
        match_method: args.match_method.into(),
        refresh: args.refresh,
    };
    let cancel = AtomicBool::new(false);
    let progress = StderrProgress::new();
    let plan = env.plan(&root, profile, &db, &art, &settings, &progress, &cancel);
    for w in &plan.warnings {
        eprintln!("{}", table::terminal_safe(&format!("warning: {w}")));
    }
    if let Some(reason) = &plan.paused {
        emit(args, &chosen, &plan, None)?;
        return Err(Failure {
            code: 5,
            message: format!(
                "{reason}; nothing was written. Run the same command later to continue."
            ),
        });
    }
    if !args.apply {
        return emit(args, &chosen, &plan, None);
    }

    let aopts = ApplyOptions {
        overwrite: args.overwrite.into(),
        include_review: !args.auto_only,
        concurrency: args.concurrency,
        ..ApplyOptions::default()
    };
    let report = env
        .apply(&plan, profile, &aopts, &progress, &cancel)
        .map_err(fail)?;
    emit(args, &chosen, &plan, Some(&report))?;
    match &report.paused {
        Some(reason) => Err(Failure {
            code: 5,
            message: format!(
                "{reason}; the job was paused. Run the same command later to continue, or `coverly undo`."
            ),
        }),
        None => Ok(()),
    }
}
```

- `none_reason` gains the arm `NoneReason::Skipped => "skipped in review".into(),`.

- [ ] **Step 7: Run every Rust test**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && cargo test --workspace`
Expected: PASS (the `cli_fill` tests prove the CLI behaves as before).

- [ ] **Step 8: Gates and commit**

Run all gates. Then:

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && git add crates/coverly-core crates/coverly-cli && git commit -m "feat(core): add the fill facade and review decisions shared by the CLI and the app"
```

---
### Task 4: App groundwork — session layout, re-scan of touched entries, job wire types and error codes

**Files:**
- Modify: `src-tauri/Cargo.toml` (`serde_json` to `[dependencies]`; dev-dependency `coverly-core` with `test-util`)
- Modify: `src-tauri/src/library/mod.rs` (`SessionSystem`, `Session.{profile, systems}`, `SessionEntry.system`, `Clone`; `pub mod refresh;`)
- Create: `src-tauri/src/library/refresh.rs`
- Modify: `src-tauri/src/library/art.rs` (`inside_root` → `pub(crate)`)
- Create: `src-tauri/src/jobs/mod.rs` (`FillConfig`; `pub mod view;`), `src-tauri/src/jobs/view.rs`
- Modify: `src-tauri/src/lib.rs` (`pub mod jobs;`, `AppState::new(settings, FillConfig)`)
- Modify: `src-tauri/src/state.rs` (`fill: Arc<FillConfig>`, `refresh_entries`, `cache()` uses `fill.cache_dir`)
- Modify: `src-tauri/src/error.rs` (new codes, `From<WriteError>`, `From<ArtError>`)
- Modify: `src/ipc/contract.sample.json`, `src-tauri/tests/contract.rs`; Create: `src/ipc/contract.requests.json`
- Modify: `src-tauri/tests/app_state.rs` (constructor)
- Test: `src-tauri/tests/refresh.rs`, unit tests in `src-tauri/src/error.rs`

**Interfaces:**
- Consumes: Task 1 `PlanNote`; Task 2 `PauseKind`, `UndoReport`; Task 3 `Endpoints`, `ArtError`, `Decision`; `resolve_art`, `LibraryScan`, `SystemScan`.
- Produces:
  - `pub struct SessionSystem { pub dir: PathBuf, pub folder: String, pub system_id: Option<String> }`; `Session { …, pub profile: Profile, pub systems: Vec<SessionSystem>, … }`; `SessionEntry { pub system: u16, … }` (all `Clone, Debug`)
  - `library::refresh::{EntryArtUpdate { id: u32, art: ArtMarks }, subset_scan(&Session, &[u32]) -> (LibraryScan, Vec<u32>), refresh_entries(&Session, &[u32], &SystemDb) -> (Session, Vec<EntryArtUpdate>), ids_for_art_paths(&Session, &[PathBuf]) -> Vec<u32>, entry_index(&Session) -> HashMap<PathBuf, u32>, rom_path(&Session, folder: &str, rel_path: &Path) -> Option<PathBuf>}`
  - `jobs::FillConfig { pub cache_dir: PathBuf, pub data_dir: PathBuf, pub endpoints: Endpoints, pub net: NetConfig }` + `FillConfig::new(cache_dir, data_dir)`
  - `AppState::new(settings_path: PathBuf, fill: FillConfig) -> AppState`; `AppState::refresh_entries(&self, generation: u64, ids: &[u32], systems: &SystemDb) -> Result<Vec<EntryArtUpdate>, IpcError>`; `AppState::fill_config(&self) -> Arc<FillConfig>`
  - `jobs::view::{FillRequest, JobState, JobProgressView, PauseView, SystemCountsView, PlanSummaryView, JobTotalsView, JobView, CandidateView, ActionKind, NoneKind, OutcomeKind, ItemOutcomeView, JobItemView, ItemFilter, DecisionIn, JobProgressEvent, EntriesUpdatedEvent, UndoView, JobHistoryView, ExportFormat, candidates_of, candidate_views}` (exact fields below)
  - `IpcError::{JobRunning, NoJob, StaleJob, BadRequest(String), NothingToUndo, Network(String), BadImage(String), WriteDenied(String), DiskFull, NameInvalid(String)}` → codes `job-running`, `no-job`, `stale-job`, `bad-request`, `nothing-to-undo`, `network`, `bad-image`, `write-denied`, `disk-full`, `name-invalid`

- [ ] **Step 1: Dependencies**

`src-tauri/Cargo.toml`: in `[dependencies]` add `serde_json = { workspace = true }`; `[dev-dependencies]` becomes:

```toml
[dev-dependencies]
coverly-core = { path = "../crates/coverly-core", features = ["test-util"] }
tempfile = { workspace = true }
```

- [ ] **Step 2: Write the failing refresh tests**

`src-tauri/tests/refresh.rs`:

```rust
mod common;

use std::path::PathBuf;

use common::{anbernic_tree, open, write};
use coverly_core::profiles::Media;
use coverly_core::settings::LIBRARY_SETTINGS_FILE;
use coverly_core::systems::SystemDb;
use coverly_roms_lib::error::IpcError;
use coverly_roms_lib::jobs::FillConfig;
use coverly_roms_lib::library::art::read_art_bytes;
use coverly_roms_lib::library::refresh::{ids_for_art_paths, refresh_entries, subset_scan};
use coverly_roms_lib::library::view::ArtMark;
use coverly_roms_lib::state::AppState;
use tempfile::TempDir;

#[test]
fn refresh_reads_only_the_touched_entries() {
    let tree = anbernic_tree(&["FC", "GBA"], 3, 0);
    let (session, view) = open(tree.path(), None).unwrap();
    let id = view
        .entries
        .iter()
        .find(|e| e.rel_path == "ROMS/GBA/Game 00001 (USA).gba")
        .unwrap()
        .id;
    write(tree.path(), "ROMS/GBA/Imgs/Game 00001 (USA).png", b"png");
    let (next, updates) = refresh_entries(&session, &[id, 9999], &SystemDb::builtin());
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].id, id);
    assert_eq!(updates[0].art.cover, ArtMark::Present);
    assert!(next.entries[id as usize].art.contains_key(&Media::Cover));
    assert_eq!(next.entries.len(), session.entries.len());
    assert!(session.entries[id as usize].art.is_empty(), "the old session is untouched");
}

#[test]
fn subset_scan_groups_entries_by_system_in_session_order() {
    let tree = anbernic_tree(&["FC", "GBA"], 3, 0);
    let (session, _) = open(tree.path(), None).unwrap();
    // FC holds ids 0..=2, GBA 3..=5.
    let (scan, order) = subset_scan(&session, &[5, 0, 5, 4, 77]);
    assert_eq!(order, vec![0, 4, 5]);
    assert_eq!(scan.systems.len(), 2);
    assert_eq!(scan.systems[0].folder_name, "FC");
    assert_eq!(scan.systems[0].roms.len(), 1);
    assert_eq!(scan.systems[1].roms.len(), 2);
    assert_eq!(scan.profile_id, "anbernic-stock");
}

#[test]
fn art_paths_map_back_to_their_entries() {
    let tree = anbernic_tree(&["FC", "GBA"], 3, 0);
    let (session, _) = open(tree.path(), None).unwrap();
    let ids = ids_for_art_paths(
        &session,
        &[
            tree.path().join("ROMS/GBA/Imgs/Game 00001 (USA).png"),
            PathBuf::from("/elsewhere/Other.png"),
        ],
    );
    // Same stem in both systems: refreshing one extra entry is harmless.
    assert_eq!(ids, vec![1, 4]);
}

#[test]
fn app_state_refreshes_the_open_generation_only() {
    let tree = anbernic_tree(&["GBA"], 2, 0);
    let config = TempDir::new().unwrap();
    let st = AppState::new(
        config.path().join(LIBRARY_SETTINGS_FILE),
        FillConfig::new(config.path().join("cache"), config.path().join("data")),
    );
    let db = SystemDb::builtin();
    let view = st.open(tree.path(), None, &db, &mut |_| {}).unwrap();
    write(
        tree.path(),
        "ROMS/GBA/Imgs/Game 00000 (USA).png",
        b"\x89PNG\r\n\x1a\nrest",
    );
    let updates = st.refresh_entries(view.generation, &[0], &db).unwrap();
    assert_eq!(updates[0].art.cover, ArtMark::Present);
    let session = st.session(view.generation).unwrap();
    assert!(read_art_bytes(&session, 0, Media::Cover)
        .unwrap()
        .starts_with(b"\x89PNG"));
    assert_eq!(
        st.refresh_entries(view.generation + 1, &[0], &db),
        Err(IpcError::StaleLibrary)
    );
}
```

In `src-tauri/tests/app_state.rs` add `use coverly_roms_lib::jobs::FillConfig;` and replace both constructor calls:

```rust
fn state(config: &TempDir) -> AppState {
    AppState::new(
        config.path().join(LIBRARY_SETTINGS_FILE),
        FillConfig::new(config.path().join("cache"), config.path().join("data")),
    )
}
```

and `let st = AppState::new(settings_path, config.path().join("cache"));` → `let st = AppState::new(settings_path, FillConfig::new(config.path().join("cache"), config.path().join("data")));`.

- [ ] **Step 3: Run them to see them fail**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && cargo test -p coverly-roms --test refresh --test app_state`
Expected: FAIL to compile — `refresh`, `jobs::FillConfig`, the new `AppState::new` do not exist.

- [ ] **Step 4: Session layout**

`src-tauri/src/library/mod.rs`:
- `pub mod refresh;` next to `pub mod art;`.
- types:

```rust
/// One folder of the ROMs root, as the backend needs it later.
#[derive(Clone, Debug)]
pub struct SessionSystem {
    pub dir: PathBuf,
    /// Folder name as on disk.
    pub folder: String,
    pub system_id: Option<String>,
}

/// Backend-only data of one entry (same index as its [`EntryView`]).
#[derive(Clone, Debug)]
pub struct SessionEntry {
    /// Index in [`Session::systems`].
    pub system: u16,
    pub system_id: String,
    pub rom: RomEntry,
    /// Art files that exist, per media.
    pub art: BTreeMap<Media, PathBuf>,
}

/// The library currently open in the app.
#[derive(Clone, Debug)]
pub struct Session {
    pub generation: u64,
    /// The folder as picked.
    pub root: PathBuf,
    /// `root` canonicalised: nothing outside it is ever read.
    pub root_canon: PathBuf,
    /// The profile fitted to `root` (what the scan used).
    pub profile: Profile,
    pub systems: Vec<SessionSystem>,
    pub entries: Vec<SessionEntry>,
}
```

- in `open_session`: `let mut session_systems = Vec::with_capacity(art.len());`; in the system loop after `system_views.push(…)` add

```rust
        session_systems.push(SessionSystem {
            dir: system_art.scan.dir.clone(),
            folder: system_art.scan.folder_name.clone(),
            system_id: system_art.scan.system_id.clone(),
        });
```

  the `SessionEntry` literal becomes

```rust
            entries.push(SessionEntry {
                system: index,
                system_id: system_art.scan.system_id.clone().unwrap_or_default(),
                rom: rom.clone(),
                art: refresh::present_paths(&rom_art.art),
            });
```

  and the `Session` literal gains `profile: profile.clone(), systems: session_systems,` (the `ProfileView` above is built before; `profile` is still owned).
- `src-tauri/src/library/art.rs`: `fn inside_root` → `pub(crate) fn inside_root`.

- [ ] **Step 5: Re-scan of touched entries**

`src-tauri/src/library/refresh.rs`:

```rust
//! After a write (fill, drawer action, undo) only the touched entries are
//! resolved again (spec §7 step 9): a scan holding just those ROMs goes
//! through the core's `resolve_art`, which reads only their art folders.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use coverly_core::art::{resolve_art, ArtState};
use coverly_core::profiles::Media;
use coverly_core::scan::{LibraryScan, SystemScan};
use coverly_core::systems::SystemDb;
use serde::Serialize;

use super::view::ArtMarks;
use super::Session;

/// New art marks of one entry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EntryArtUpdate {
    pub id: u32,
    pub art: ArtMarks,
}

/// The art files that exist, per media.
pub(crate) fn present_paths(states: &BTreeMap<Media, ArtState>) -> BTreeMap<Media, PathBuf> {
    states
        .iter()
        .filter_map(|(media, state)| match state {
            ArtState::Present { path } => Some((*media, path.clone())),
            _ => None,
        })
        .collect()
}

/// A scan of only `ids` (unknown ids dropped, duplicates merged), grouped
/// by system in session order, and the ids in the order of its ROMs.
pub fn subset_scan(session: &Session, ids: &[u32]) -> (LibraryScan, Vec<u32>) {
    let mut wanted: Vec<u32> = ids
        .iter()
        .copied()
        .filter(|&id| (id as usize) < session.entries.len())
        .collect();
    wanted.sort_unstable();
    wanted.dedup();
    let mut systems: Vec<SystemScan> = Vec::new();
    let mut last: Option<u16> = None;
    for &id in &wanted {
        let entry = &session.entries[id as usize];
        if last != Some(entry.system) {
            let sys = &session.systems[entry.system as usize];
            systems.push(SystemScan {
                dir: sys.dir.clone(),
                folder_name: sys.folder.clone(),
                system_id: sys.system_id.clone(),
                roms: Vec::new(),
            });
            last = Some(entry.system);
        }
        systems
            .last_mut()
            .expect("a system was pushed above")
            .roms
            .push(entry.rom.clone());
    }
    let scan = LibraryScan {
        root: session.root.clone(),
        profile_id: session.profile.id.clone(),
        systems,
        warnings: Vec::new(),
    };
    (scan, wanted)
}

/// Resolves the art of `ids` again from disk: a new session with their art
/// paths updated (the old one is untouched) and the new marks.
pub fn refresh_entries(
    session: &Session,
    ids: &[u32],
    systems: &SystemDb,
) -> (Session, Vec<EntryArtUpdate>) {
    let (scan, order) = subset_scan(session, ids);
    let art = resolve_art(&scan, &session.profile, systems, &Media::ALL);
    let mut next = session.clone();
    let mut updates = Vec::with_capacity(order.len());
    for (id, rom_art) in order.into_iter().zip(art.iter().flat_map(|s| s.roms.iter())) {
        next.entries[id as usize].art = present_paths(&rom_art.art);
        updates.push(EntryArtUpdate {
            id,
            art: ArtMarks::from_states(&rom_art.art),
        });
    }
    (next, updates)
}

/// Entries whose art could be one of `paths` (file stem equal to the ROM
/// stem, or a Batocera `<stem>-image` style name), ignoring case. Used
/// after an undo, which reports paths only; an extra entry is harmless.
pub fn ids_for_art_paths(session: &Session, paths: &[PathBuf]) -> Vec<u32> {
    let stems: Vec<String> = paths
        .iter()
        .filter_map(|p| p.file_stem())
        .map(|s| s.to_string_lossy().to_lowercase())
        .collect();
    session
        .entries
        .iter()
        .enumerate()
        .filter(|(_, e)| {
            let stem = e.rom.stem.to_lowercase();
            stems.iter().any(|s| {
                s == &stem
                    || s
                        .strip_prefix(stem.as_str())
                        .is_some_and(|rest| rest.starts_with('-'))
            })
        })
        .map(|(i, _)| i as u32)
        .collect()
}

/// Entry id of each ROM's primary file.
pub fn entry_index(session: &Session) -> HashMap<PathBuf, u32> {
    session
        .entries
        .iter()
        .enumerate()
        .map(|(i, e)| (e.rom.path.clone(), i as u32))
        .collect()
}

/// The primary file of a plan item (`folder` + path relative to it).
pub fn rom_path(session: &Session, folder: &str, rel_path: &Path) -> Option<PathBuf> {
    session
        .systems
        .iter()
        .find(|s| s.folder == folder)
        .map(|s| s.dir.join(rel_path))
}
```

- [ ] **Step 6: `FillConfig` and the app state**

`src-tauri/src/jobs/mod.rs`:

```rust
//! Fill jobs of the desktop app (spec §7 steps 3–9, §8 Jobs).

pub mod view;

use std::path::PathBuf;

use coverly_core::fill::Endpoints;
use coverly_core::net::NetConfig;

/// Where fills keep their cache and journals, and how they reach the network.
#[derive(Clone, Debug)]
pub struct FillConfig {
    pub cache_dir: PathBuf,
    pub data_dir: PathBuf,
    pub endpoints: Endpoints,
    pub net: NetConfig,
}

impl FillConfig {
    /// The real servers with the default network settings.
    pub fn new(cache_dir: PathBuf, data_dir: PathBuf) -> FillConfig {
        FillConfig {
            cache_dir,
            data_dir,
            endpoints: Endpoints::default(),
            net: NetConfig::default(),
        }
    }
}
```

`src-tauri/src/lib.rs`: `pub mod jobs;`; the setup closure becomes:

```rust
        .setup(|app| {
            let settings_path = config_dir()
                .ok_or("cannot find the app config folder")?
                .join(LIBRARY_SETTINGS_FILE);
            let dirs = AppDirs::resolve(None, None).ok_or("cannot find the app data folders")?;
            app.manage(state::AppState::new(
                settings_path,
                jobs::FillConfig::new(dirs.cache, dirs.data),
            ));
            Ok(())
        })
```

`src-tauri/src/state.rs`:
- fields: replace `cache_dir: PathBuf` with `fill: Arc<FillConfig>`; `new(settings_path: PathBuf, fill: FillConfig)` stores `fill: Arc::new(fill)`.
- `cache()` uses `self.fill.cache_dir`.
- add:

```rust
    /// The fill settings shared with job threads.
    pub fn fill_config(&self) -> Arc<FillConfig> {
        Arc::clone(&self.fill)
    }

    /// Resolves the art of `ids` again and installs the updated session,
    /// if `generation` is still the open library.
    pub fn refresh_entries(
        &self,
        generation: u64,
        ids: &[u32],
        systems: &SystemDb,
    ) -> Result<Vec<EntryArtUpdate>, IpcError> {
        let mut slot = self.session.lock().unwrap_or_else(PoisonError::into_inner);
        let current = match slot.as_ref() {
            None => return Err(IpcError::NoLibrary),
            Some(s) if s.generation != generation => return Err(IpcError::StaleLibrary),
            Some(s) => Arc::clone(s),
        };
        let (next, updates) = refresh_entries(&current, ids, systems);
        *slot = Some(Arc::new(next));
        Ok(updates)
    }
```

  with imports `use crate::jobs::FillConfig;` and `use crate::library::refresh::{refresh_entries, EntryArtUpdate};`.

- [ ] **Step 7: Error codes**

`src-tauri/src/error.rs` — add the variants after `Settings`:

```rust
    #[error("a fill job is running")]
    JobRunning,
    #[error("there is no fill job")]
    NoJob,
    #[error("the fill job was replaced by a newer one")]
    StaleJob,
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("nothing to undo")]
    NothingToUndo,
    #[error("network: {0}")]
    Network(String),
    #[error("not a usable image: {0}")]
    BadImage(String),
    #[error("write denied: {0}")]
    WriteDenied(String),
    #[error("the disk is full")]
    DiskFull,
    #[error("file name not valid on FAT/exFAT: {0}")]
    NameInvalid(String),
```

and after the enum:

```rust
impl From<coverly_core::write::WriteError> for IpcError {
    fn from(e: coverly_core::write::WriteError) -> Self {
        use coverly_core::write::WriteError as W;
        match e {
            W::OutsideRoot(p) => IpcError::OutsideLibrary(p.display().to_string()),
            W::NameInvalid(n) => IpcError::NameInvalid(n),
            W::Denied(p) => IpcError::WriteDenied(p.display().to_string()),
            W::DiskFull => IpcError::DiskFull,
            W::NothingToUndo => IpcError::NothingToUndo,
            other @ (W::NotAFile(_) | W::Journal(_) | W::Io(_)) => IpcError::Io(other.to_string()),
        }
    }
}

impl From<coverly_core::fill::ArtError> for IpcError {
    fn from(e: coverly_core::fill::ArtError) -> Self {
        use coverly_core::fill::ArtError as A;
        match e {
            A::NoSlot | A::UnknownSource(_) => IpcError::BadRequest(e.to_string()),
            A::Source(m) | A::Paused(m) => IpcError::Network(m),
            A::Image(i) => IpcError::BadImage(i.to_string()),
            A::Write(w) => w.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use coverly_core::write::WriteError;

    #[test]
    fn write_errors_keep_their_meaning() {
        assert_eq!(IpcError::from(WriteError::DiskFull), IpcError::DiskFull);
        assert_eq!(
            IpcError::from(WriteError::NothingToUndo),
            IpcError::NothingToUndo
        );
        assert!(matches!(
            IpcError::from(WriteError::Denied("/sd/x.png".into())),
            IpcError::WriteDenied(_)
        ));
        let json = serde_json::to_value(IpcError::JobRunning).unwrap();
        assert_eq!(json, serde_json::json!({ "code": "job-running" }));
    }
}
```

- [ ] **Step 8: Job wire types**

`src-tauri/src/jobs/view.rs`:

```rust
//! Wire types of fill jobs, drawer actions and exports, mirrored by
//! `src/ipc/types.ts` and pinned by `tests/contract.rs`. Paths never
//! cross: items are addressed by index and entries by id.

use coverly_core::identify::names::Region;
use coverly_core::jobs::{ItemErrorKind, ItemStatus, JobReport};
use coverly_core::matching::StrategyKind;
use coverly_core::plan::{
    confidence, Counts, Decision, NoneReason, Plan, PlanAction, PlanItem, PlanNote,
};
use coverly_core::profiles::Media;
use coverly_core::progress::{PauseKind, Phase};
use coverly_core::sources::{Candidate, MatchKind};
use coverly_core::write::{JobSummary, OverwritePolicy, UndoReport};
use serde::{Deserialize, Serialize};

use crate::library::refresh::EntryArtUpdate;

/// What the wizard asks for.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct FillRequest {
    /// The selection or the current filter; `None` = the whole library.
    pub entry_ids: Option<Vec<u32>>,
    pub media: Vec<Media>,
    pub strategy: StrategyKind,
    /// `rom`, `world`, `usa`, … (spec §9 region order).
    pub regions: Vec<String>,
    /// `never` = only missing art; `backup` = replace existing art too.
    pub overwrite: OverwritePolicy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum JobState {
    Gathering,
    Planned,
    Applying,
    Paused,
    Cancelled,
    Done,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct JobProgressView {
    pub phase: Phase,
    pub done: usize,
    pub total: usize,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PauseView {
    pub phase: Phase,
    pub kind: PauseKind,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SystemCountsView {
    pub folder: String,
    pub auto: usize,
    pub review: usize,
    pub none: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PlanSummaryView {
    pub totals: Counts,
    pub per_system: Vec<SystemCountsView>,
    pub warnings: Vec<String>,
}

impl PlanSummaryView {
    pub fn of(plan: &Plan) -> PlanSummaryView {
        PlanSummaryView {
            totals: plan.totals,
            per_system: plan
                .per_system
                .iter()
                .map(|(folder, c)| SystemCountsView {
                    folder: folder.clone(),
                    auto: c.auto,
                    review: c.review,
                    none: c.none,
                })
                .collect(),
            warnings: plan.warnings.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct JobTotalsView {
    pub filled: usize,
    pub replaced: usize,
    pub skipped: usize,
    pub no_art: usize,
    pub failed: usize,
}

impl JobTotalsView {
    pub fn of(r: &JobReport) -> JobTotalsView {
        JobTotalsView {
            filled: r.filled,
            replaced: r.replaced,
            skipped: r.skipped,
            no_art: r.no_art,
            failed: r.failed,
        }
    }
}

/// The current job, as the webview sees it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct JobView {
    pub id: u64,
    /// The library generation the job belongs to.
    pub generation: u64,
    pub state: JobState,
    pub progress: Option<JobProgressView>,
    pub paused: Option<PauseView>,
    pub plan: Option<PlanSummaryView>,
    /// Review decisions recorded so far.
    pub decided: usize,
    /// The journal id, once something was written.
    pub journal: Option<String>,
    pub result: Option<JobTotalsView>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CandidateView {
    /// 0 = the chosen image, `n` = alternative `n - 1` (see `Decision::Pick`).
    pub index: u32,
    pub source: String,
    pub name: String,
    pub regions: Vec<Region>,
    pub kind: MatchKind,
    pub score: u8,
    pub confidence: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ActionKind {
    Auto,
    Review,
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NoneKind {
    NoCandidates,
    BelowThreshold,
    SourceError,
    NoTarget,
    Skipped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum OutcomeKind {
    Filled,
    Replaced,
    SkippedExists,
    SkippedReview,
    SkippedNoMatch,
    NotAttempted,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ItemOutcomeView {
    pub status: OutcomeKind,
    pub error: Option<ItemErrorKind>,
    pub message: Option<String>,
}

impl ItemOutcomeView {
    pub fn of(status: &ItemStatus) -> ItemOutcomeView {
        let simple = |status| ItemOutcomeView {
            status,
            error: None,
            message: None,
        };
        match status {
            ItemStatus::Filled { .. } => simple(OutcomeKind::Filled),
            ItemStatus::Replaced { .. } => simple(OutcomeKind::Replaced),
            ItemStatus::SkippedExists => simple(OutcomeKind::SkippedExists),
            ItemStatus::SkippedReview => simple(OutcomeKind::SkippedReview),
            ItemStatus::SkippedNoMatch => simple(OutcomeKind::SkippedNoMatch),
            ItemStatus::NotAttempted => simple(OutcomeKind::NotAttempted),
            ItemStatus::Failed { kind, message } => ItemOutcomeView {
                status: OutcomeKind::Failed,
                error: Some(*kind),
                message: Some(message.clone()),
            },
        }
    }
}

/// One plan item: one ROM and one media.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct JobItemView {
    /// Index in the plan (what decisions and candidate art name).
    pub index: u32,
    /// The library entry, when it is still in the open session.
    pub entry_id: Option<u32>,
    pub folder: String,
    /// The ROM file name without extension.
    pub name: String,
    pub media: Media,
    pub action: ActionKind,
    pub confidence: Option<f32>,
    pub note: Option<PlanNote>,
    pub none_reason: Option<NoneKind>,
    pub candidates: Vec<CandidateView>,
    pub replaces_existing: bool,
    pub decision: Option<Decision>,
    pub outcome: Option<ItemOutcomeView>,
}

/// The chosen image then the alternatives (empty for `none`).
pub fn candidates_of(item: &PlanItem) -> Vec<&Candidate> {
    match &item.action {
        PlanAction::Auto {
            chosen,
            alternatives,
            ..
        }
        | PlanAction::Review {
            chosen,
            alternatives,
            ..
        } => std::iter::once(chosen).chain(alternatives.iter()).collect(),
        PlanAction::None { .. } => Vec::new(),
    }
}

pub fn candidate_views(item: &PlanItem) -> Vec<CandidateView> {
    candidates_of(item)
        .into_iter()
        .enumerate()
        .map(|(i, c)| CandidateView {
            index: i as u32,
            source: c.source.clone(),
            name: c.name.clone(),
            regions: c.regions.clone(),
            kind: c.kind,
            score: c.score,
            confidence: confidence(c),
        })
        .collect()
}

impl JobItemView {
    pub fn new(
        index: usize,
        item: &PlanItem,
        entry_id: Option<u32>,
        decision: Option<Decision>,
        outcome: Option<&ItemStatus>,
    ) -> JobItemView {
        let (action, confidence, none_reason) = match &item.action {
            PlanAction::Auto { confidence, .. } => (ActionKind::Auto, Some(*confidence), None),
            PlanAction::Review { confidence, .. } => (ActionKind::Review, Some(*confidence), None),
            PlanAction::None { reason } => (
                ActionKind::None,
                None,
                Some(match reason {
                    NoneReason::NoCandidates => NoneKind::NoCandidates,
                    NoneReason::BelowThreshold { .. } => NoneKind::BelowThreshold,
                    NoneReason::SourceError { .. } => NoneKind::SourceError,
                    NoneReason::NoTarget => NoneKind::NoTarget,
                    NoneReason::Skipped => NoneKind::Skipped,
                }),
            ),
        };
        JobItemView {
            index: index as u32,
            entry_id,
            folder: item.folder.clone(),
            name: item.stem.clone(),
            media: item.media,
            action,
            confidence,
            note: item.note,
            none_reason,
            candidates: candidate_views(item),
            replaces_existing: item.replaces_existing,
            decision,
            outcome: outcome.map(ItemOutcomeView::of),
        }
    }
}

/// Which items `fill_job_items` returns.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ItemFilter {
    All,
    Auto,
    Review,
    Failed,
}

/// One review decision from the webview: `{ "index": 3, "choice": "pick", "candidate": 1 }`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub struct DecisionIn {
    pub index: u32,
    #[serde(flatten)]
    pub decision: Decision,
}

/// Payload of `fill-job-progress`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct JobProgressEvent {
    pub job: u64,
    pub phase: Phase,
    pub done: usize,
    pub total: usize,
    pub label: String,
}

/// Payload of `library-entries-updated`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EntriesUpdatedEvent {
    pub generation: u64,
    pub entries: Vec<EntryArtUpdate>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct UndoView {
    pub job: String,
    pub removed: usize,
    pub restored: usize,
    /// Files the user changed after the job: left as they are.
    pub kept: usize,
    pub entries: Vec<EntryArtUpdate>,
}

impl UndoView {
    pub fn new(r: &UndoReport, entries: Vec<EntryArtUpdate>) -> UndoView {
        UndoView {
            job: r.job.clone(),
            removed: r.removed.len(),
            restored: r.restored.len(),
            kept: r.skipped_modified.len(),
            entries,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct JobHistoryView {
    pub job: String,
    pub root: String,
    pub created_at: String,
    pub entries: usize,
    pub complete: bool,
    pub undone: bool,
}

impl From<JobSummary> for JobHistoryView {
    fn from(s: JobSummary) -> Self {
        JobHistoryView {
            job: s.job,
            root: s.root.display().to_string(),
            created_at: s.created_at,
            entries: s.entries,
            complete: s.complete,
            undone: s.undone,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    Csv,
    Json,
}
```

- [ ] **Step 9: Extend the shared contract (failing first)**

`src/ipc/contract.sample.json`: replace the last entry `"error": { "code": "not-found", "detail": "/Volumes/SD" }` with:

```json
  "job_view": {
    "id": 4,
    "generation": 3,
    "state": "paused",
    "progress": { "phase": "apply", "done": 2, "total": 5, "label": "Zelda (USA)" },
    "paused": {
      "phase": "apply",
      "kind": "rate-limit",
      "detail": "rate limited by thumbnails.libretro.com (HTTP 429)"
    },
    "plan": {
      "totals": { "auto": 3, "review": 1, "none": 1 },
      "per_system": [{ "folder": "SFC", "auto": 3, "review": 1, "none": 1 }],
      "warnings": []
    },
    "decided": 1,
    "journal": "20261016T050401Z-1bfb-0000",
    "result": { "filled": 2, "replaced": 0, "skipped": 2, "no_art": 0, "failed": 1 },
    "error": null
  },
  "job_item": {
    "index": 0,
    "entry_id": 0,
    "folder": "SFC",
    "name": "Zelda (USA)",
    "media": "cover",
    "action": "review",
    "confidence": 0.5,
    "note": "unreleased-variant",
    "none_reason": null,
    "candidates": [
      {
        "index": 0,
        "source": "libretro",
        "name": "Zelda (USA) (Beta)",
        "regions": ["usa"],
        "kind": "title-exact",
        "score": 95,
        "confidence": 0.5
      }
    ],
    "replaces_existing": false,
    "decision": { "choice": "pick", "candidate": 0 },
    "outcome": { "status": "failed", "error": "write-denied", "message": "write denied" }
  },
  "job_progress": { "job": 4, "phase": "gather", "done": 1, "total": 5, "label": "Zelda (USA)" },
  "entries_updated": {
    "generation": 3,
    "entries": [
      {
        "id": 0,
        "art": { "cover": "present", "screenshot": "unsupported", "title": "unsupported" }
      }
    ]
  },
  "undo": {
    "job": "20261016T050401Z-1bfb-0000",
    "removed": 2,
    "restored": 1,
    "kept": 0,
    "entries": []
  },
  "history_item": {
    "job": "20261016T050401Z-1bfb-0000",
    "root": "/Volumes/SD",
    "created_at": "2026-10-16T05:04:01Z",
    "entries": 3,
    "complete": true,
    "undone": false
  },
  "error": { "code": "not-found", "detail": "/Volumes/SD" }
```

`src/ipc/contract.requests.json` (what the webview sends):

```json
{
  "fill_request": {
    "entry_ids": [0, 2],
    "media": ["cover"],
    "strategy": "rom-region",
    "regions": ["rom", "world", "usa", "europe", "japan"],
    "overwrite": "never"
  },
  "fill_request_all": {
    "entry_ids": null,
    "media": ["cover", "title"],
    "strategy": "priority",
    "regions": ["usa"],
    "overwrite": "backup"
  },
  "decisions": [
    { "index": 0, "choice": "accept" },
    { "index": 1, "choice": "pick", "candidate": 2 },
    { "index": 2, "choice": "skip" }
  ],
  "item_filters": ["all", "auto", "review", "failed"],
  "export_formats": ["csv", "json"]
}
```

`src-tauri/tests/contract.rs`:
- imports add:

```rust
use coverly_core::jobs::ItemErrorKind;
use coverly_core::matching::StrategyKind;
use coverly_core::plan::{Counts, Decision, PlanNote};
use coverly_core::progress::{PauseKind, Phase};
use coverly_core::sources::MatchKind;
use coverly_core::write::OverwritePolicy;
use coverly_roms_lib::jobs::view::{
    ActionKind, CandidateView, DecisionIn, EntriesUpdatedEvent, ExportFormat, FillRequest,
    ItemFilter, ItemOutcomeView, JobHistoryView, JobItemView, JobProgressEvent,
    JobProgressView, JobState, JobTotalsView, JobView, OutcomeKind, PauseView, PlanSummaryView,
    SystemCountsView, UndoView,
};
use coverly_roms_lib::library::refresh::EntryArtUpdate;
```

- in the `json!({ … })` of `rust_serialises_the_shared_sample`, replace `"error": IpcError::NotFound("/Volumes/SD".into()),` with:

```rust
        "job_view": JobView {
            id: 4,
            generation: 3,
            state: JobState::Paused,
            progress: Some(JobProgressView { phase: Phase::Apply, done: 2, total: 5, label: "Zelda (USA)".into() }),
            paused: Some(PauseView {
                phase: Phase::Apply,
                kind: PauseKind::RateLimit,
                detail: "rate limited by thumbnails.libretro.com (HTTP 429)".into(),
            }),
            plan: Some(PlanSummaryView {
                totals: Counts { auto: 3, review: 1, none: 1 },
                per_system: vec![SystemCountsView { folder: "SFC".into(), auto: 3, review: 1, none: 1 }],
                warnings: vec![],
            }),
            decided: 1,
            journal: Some("20261016T050401Z-1bfb-0000".into()),
            result: Some(JobTotalsView { filled: 2, replaced: 0, skipped: 2, no_art: 0, failed: 1 }),
            error: None,
        },
        "job_item": JobItemView {
            index: 0,
            entry_id: Some(0),
            folder: "SFC".into(),
            name: "Zelda (USA)".into(),
            media: Media::Cover,
            action: ActionKind::Review,
            confidence: Some(0.5),
            note: Some(PlanNote::UnreleasedVariant),
            none_reason: None,
            candidates: vec![CandidateView {
                index: 0,
                source: "libretro".into(),
                name: "Zelda (USA) (Beta)".into(),
                regions: vec![Region::Usa],
                kind: MatchKind::TitleExact,
                score: 95,
                confidence: 0.5,
            }],
            replaces_existing: false,
            decision: Some(Decision::Pick { candidate: 0 }),
            outcome: Some(ItemOutcomeView {
                status: OutcomeKind::Failed,
                error: Some(ItemErrorKind::WriteDenied),
                message: Some("write denied".into()),
            }),
        },
        "job_progress": JobProgressEvent { job: 4, phase: Phase::Gather, done: 1, total: 5, label: "Zelda (USA)".into() },
        "entries_updated": EntriesUpdatedEvent {
            generation: 3,
            entries: vec![EntryArtUpdate {
                id: 0,
                art: ArtMarks { cover: ArtMark::Present, screenshot: ArtMark::Unsupported, title: ArtMark::Unsupported },
            }],
        },
        "undo": UndoView { job: "20261016T050401Z-1bfb-0000".into(), removed: 2, restored: 1, kept: 0, entries: vec![] },
        "history_item": JobHistoryView {
            job: "20261016T050401Z-1bfb-0000".into(),
            root: "/Volumes/SD".into(),
            created_at: "2026-10-16T05:04:01Z".into(),
            entries: 3,
            complete: true,
            undone: false,
        },
        "error": IpcError::NotFound("/Volumes/SD".into()),
```

- append:

```rust
#[test]
fn rust_reads_the_shared_requests() {
    let req: Value =
        serde_json::from_str(include_str!("../../src/ipc/contract.requests.json")).unwrap();
    let one: FillRequest = serde_json::from_value(req["fill_request"].clone()).unwrap();
    assert_eq!(one.entry_ids, Some(vec![0, 2]));
    assert_eq!(one.strategy, StrategyKind::RomRegion);
    assert_eq!(one.overwrite, OverwritePolicy::Never);
    let all: FillRequest = serde_json::from_value(req["fill_request_all"].clone()).unwrap();
    assert_eq!(all.entry_ids, None);
    assert_eq!(all.media, vec![Media::Cover, Media::Title]);
    assert_eq!(all.overwrite, OverwritePolicy::Backup);
    let decisions: Vec<DecisionIn> = serde_json::from_value(req["decisions"].clone()).unwrap();
    assert_eq!(
        decisions,
        vec![
            DecisionIn { index: 0, decision: Decision::Accept },
            DecisionIn { index: 1, decision: Decision::Pick { candidate: 2 } },
            DecisionIn { index: 2, decision: Decision::Skip },
        ]
    );
    let filters: Vec<ItemFilter> = serde_json::from_value(req["item_filters"].clone()).unwrap();
    assert_eq!(filters.len(), 4);
    let formats: Vec<ExportFormat> =
        serde_json::from_value(req["export_formats"].clone()).unwrap();
    assert_eq!(formats, vec![ExportFormat::Csv, ExportFormat::Json]);
}
```

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && cargo test -p coverly-roms --test contract`
Expected: PASS once Steps 4–8 are in (it fails to compile before Step 8).

- [ ] **Step 10: Run all app tests**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && cargo test -p coverly-roms && pnpm test -- src/ipc/contract.test.ts`
Expected: PASS (the TS contract test still checks only the M3 keys; Task 8 adds the new ones).

- [ ] **Step 11: Gates and commit**

Run all gates. Then:

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && git add src-tauri src/ipc/contract.sample.json src/ipc/contract.requests.json Cargo.lock && git commit -m "feat(app): add the session layout, entry refresh and fill job wire types"
```

---
### Task 5: App — the job manager (dry run, decisions, run, pause/resume, cancel, items, candidate previews)

**Files:**
- Modify: `src-tauri/src/jobs/mod.rs` (`JobEvents`, `JobManager`, `with_env`, worker threads, progress bridge)
- Create: `src-tauri/tests/common/fill.rs`; Modify: `src-tauri/tests/common/mod.rs` (`pub mod fill;`)
- Test: `src-tauri/tests/fill_job.rs`

**Interfaces:**
- Consumes: Task 3 `FillEnv`, `FillSettings`, `Decision`, `Plan::{decide, subset}`; Task 2 `JobReport::{resumable_items, merge_resumed}`, `ApplyOptions.resume_job`, `PauseKind`; Task 4 `FillConfig`, `Session`, `subset_scan`, `entry_index`, `rom_path`, wire types, error codes.
- Produces:
  - `pub trait JobEvents: Send + Sync { fn progress(&self, event: JobProgressEvent); fn changed(&self, view: JobView); fn written(&self, generation: u64, ids: Vec<u32>); }` — `written` is called before the final `changed`
  - `pub fn with_env<T>(cfg: &FillConfig, f: impl FnOnce(&FillEnv<'_>) -> Result<T, IpcError>) -> Result<T, IpcError>`
  - `#[derive(Default)] pub struct JobManager` with
    - `pub fn busy(&self) -> bool`
    - `pub fn current(&self) -> Option<JobView>`
    - `pub fn start(&self, cfg: Arc<FillConfig>, session: Arc<Session>, req: &FillRequest, events: Arc<dyn JobEvents>) -> Result<JobView, IpcError>`
    - `pub fn items(&self, job: u64, filter: ItemFilter) -> Result<Vec<JobItemView>, IpcError>`
    - `pub fn decide(&self, job: u64, decisions: &[DecisionIn]) -> Result<JobView, IpcError>`
    - `pub fn run(&self, cfg: Arc<FillConfig>, job: u64, events: Arc<dyn JobEvents>) -> Result<JobView, IpcError>`
    - `pub fn resume(&self, cfg: Arc<FillConfig>, job: u64, events: Arc<dyn JobEvents>) -> Result<JobView, IpcError>`
    - `pub fn cancel(&self, job: u64) -> Result<JobView, IpcError>`
    - `pub fn candidate_image(&self, cfg: &FillConfig, job: u64, index: u32, candidate: u32) -> Result<Vec<u8>, IpcError>`
    - `pub fn report(&self, job: u64) -> Result<JobReport, IpcError>`
  - test helpers `common::fill::{fill_world, request, wait_for, Recorder, FillWorld, GBA, BOX}`

State machine: `gathering → planned | paused(gather) | cancelled | failed`; `planned → applying (run) | cancelled`; `applying → done | paused(apply) | cancelled | failed`; `paused → gathering | applying (resume) | cancelled`. A new `start` is refused (`job-running`) only while gathering/applying; otherwise it replaces the job (its journal stays for undo) and older ids become `stale-job`.

- [ ] **Step 1: Test helpers**

`src-tauri/tests/common/mod.rs`: add `pub mod fill;` below the `#![allow(dead_code)]` line.

`src-tauri/tests/common/fill.rs`:

```rust
//! A mock libretro server and a small GBA library for fill job tests.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use coverly_core::fill::Endpoints;
use coverly_core::matching::StrategyKind;
use coverly_core::net::testing::{png_bytes, MockServer};
use coverly_core::net::NetConfig;
use coverly_core::profiles::Media;
use coverly_core::write::OverwritePolicy;
use coverly_roms_lib::jobs::view::{FillRequest, JobProgressEvent, JobState, JobView};
use coverly_roms_lib::jobs::{FillConfig, JobEvents};
use tempfile::TempDir;

pub const GBA: &str = "Nintendo - Game Boy Advance";
pub const BOX: &str = "Named_Boxarts";
/// The USA box of `002 Golden Adventure` (an HTML page: it falls back to Japan).
pub const ADVENTURE_USA: &str =
    "/Nintendo%20-%20Game%20Boy%20Advance/Named_Boxarts/Golden%20Adventure%20(USA).png";

pub struct FillWorld {
    pub lib: TempDir,
    pub dirs: TempDir,
    pub server: MockServer,
    pub cfg: Arc<FillConfig>,
}

// CAUTION (Task 3 review): title matching does not strip numbered
// prefixes yet (tracker M2-13): `001 Golden Sun` is a fuzzy `review`, not
// `auto`, and `004 Gldn Rsh` gets no candidates. If these stems fail as
// written, drop the numbers (or add a DAT hit per ROM) and re-derive the
// plan order and entry ids below; do not copy them blindly.
/// `ROMS/GBA`: `001 Golden Sun` (auto), `002 Golden Adventure` (auto, its
/// USA box is an HTML page, the Japanese one works), `003 Missing Game`
/// (auto, no image on either host: `no-art`), `004 Gldn Rsh` (review).
/// Entry ids are 0..=3 in that order.
pub fn fill_world() -> FillWorld {
    let lib = TempDir::new().unwrap();
    let r = lib.path();
    std::fs::create_dir_all(r.join("anbernic")).unwrap();
    for (name, bytes) in [
        ("001 Golden Sun.gba", "golden-sun-bytes"),
        ("002 Golden Adventure.gba", "adv"),
        ("003 Missing Game.gba", "missing"),
        ("004 Gldn Rsh.gba", "rush"),
    ] {
        super::write(r, &format!("ROMS/GBA/{name}"), bytes.as_bytes());
    }
    let server = MockServer::start();
    server.serve_libretro_root(&[GBA]);
    server.serve_libretro_index(
        GBA,
        BOX,
        &[
            "Golden Sun (USA, Europe).png",
            "Golden Adventure (USA).png",
            "Golden Adventure (Japan).png",
            "Missing Game (USA).png",
            "Golden Rush (USA).png",
        ],
    );
    server.serve_libretro_image(GBA, BOX, "Golden Sun (USA, Europe).png", png_bytes(1000, 1400));
    server.serve_libretro_image(
        GBA,
        BOX,
        "Golden Adventure (USA).png",
        b"<html>oops</html>".to_vec(),
    );
    server.serve_libretro_image(GBA, BOX, "Golden Adventure (Japan).png", png_bytes(300, 300));
    server.serve_libretro_image(GBA, BOX, "Golden Rush (USA).png", png_bytes(200, 200));
    let dirs = TempDir::new().unwrap();
    let cfg = FillConfig {
        cache_dir: dirs.path().join("cache"),
        data_dir: dirs.path().join("data"),
        endpoints: Endpoints {
            libretro: server.base(),
            libretro_raw: server.url("/raw"),
            database: server.base(),
        },
        net: NetConfig {
            retries: 0,
            ..NetConfig::default()
        },
    };
    FillWorld {
        lib,
        dirs,
        server,
        cfg: Arc::new(cfg),
    }
}

/// Covers for `entry_ids` (or everything), default strategy, only missing.
pub fn request(entry_ids: Option<Vec<u32>>) -> FillRequest {
    FillRequest {
        entry_ids,
        media: vec![Media::Cover],
        strategy: StrategyKind::RomRegion,
        regions: ["rom", "world", "usa", "europe", "japan"]
            .map(String::from)
            .to_vec(),
        overwrite: OverwritePolicy::Never,
    }
}

/// Records every event.
#[derive(Default)]
pub struct Recorder {
    pub progress: Mutex<Vec<JobProgressEvent>>,
    pub changed: Mutex<Vec<JobView>>,
    pub written: Mutex<Vec<(u64, Vec<u32>)>>,
}

impl JobEvents for Recorder {
    fn progress(&self, event: JobProgressEvent) {
        self.progress.lock().unwrap().push(event);
    }
    fn changed(&self, view: JobView) {
        self.changed.lock().unwrap().push(view);
    }
    fn written(&self, generation: u64, ids: Vec<u32>) {
        self.written.lock().unwrap().push((generation, ids));
    }
}

/// Waits (at most 20 s) until the last `changed` event is in `state`.
pub fn wait_for(rec: &Recorder, state: JobState) -> JobView {
    let start = Instant::now();
    loop {
        if let Some(v) = rec.changed.lock().unwrap().last() {
            if v.state == state {
                return v.clone();
            }
        }
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "timed out waiting for {state:?}: {:?}",
            rec.changed.lock().unwrap().last()
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}
```

- [ ] **Step 2: Write the failing job tests**

`src-tauri/tests/fill_job.rs`:

```rust
mod common;

use std::sync::Arc;
use std::time::Duration;

use common::fill::{fill_world, request, wait_for, Recorder, ADVENTURE_USA};
use common::open;
use coverly_core::fill::Endpoints;
use coverly_core::jobs::ItemErrorKind;
use coverly_core::net::testing::{HangingServer, MockResponse};
use coverly_core::net::NetConfig;
use coverly_core::plan::Decision;
use coverly_core::profiles::Media;
use coverly_core::progress::{PauseKind, Phase};
use coverly_core::write::{list_jobs, undo_last};
use coverly_roms_lib::error::IpcError;
use coverly_roms_lib::jobs::view::{
    ActionKind, DecisionIn, ItemFilter, JobProgressEvent, JobState, JobView, OutcomeKind,
};
use coverly_roms_lib::jobs::{FillConfig, JobEvents, JobManager};
use coverly_roms_lib::library::Session;

fn session(w: &common::fill::FillWorld) -> Arc<Session> {
    Arc::new(open(w.lib.path(), None).unwrap().0)
}

fn planned(
    w: &common::fill::FillWorld,
    jobs: &JobManager,
    rec: &Arc<Recorder>,
    ids: Option<Vec<u32>>,
) -> u64 {
    let v = jobs
        .start(w.cfg.clone(), session(w), &request(ids), rec.clone())
        .unwrap();
    assert_eq!(v.state, JobState::Gathering);
    wait_for(rec, JobState::Planned).id
}

fn cover(w: &common::fill::FillWorld, stem: &str) -> std::path::PathBuf {
    w.lib.path().join(format!("ROMS/GBA/Imgs/{stem}.png"))
}

#[test]
fn dry_run_classifies_items_and_writes_nothing() {
    let w = fill_world();
    let (jobs, rec) = (JobManager::default(), Arc::new(Recorder::default()));
    let id = planned(&w, &jobs, &rec, None);
    let v = jobs.current().unwrap();
    let plan = v.plan.unwrap();
    assert_eq!(
        (plan.totals.auto, plan.totals.review, plan.totals.none),
        (3, 1, 0)
    );
    assert_eq!(plan.per_system[0].folder, "GBA");
    assert!(!w.lib.path().join("ROMS/GBA/Imgs").exists());
    assert!(rec
        .progress
        .lock()
        .unwrap()
        .iter()
        .any(|p| p.phase == Phase::Gather && p.job == id));
    let review = jobs.items(id, ItemFilter::Review).unwrap();
    assert_eq!(review.len(), 1);
    assert_eq!(review[0].name, "004 Gldn Rsh");
    assert_eq!(review[0].entry_id, Some(3));
    assert_eq!(review[0].action, ActionKind::Review);
    assert!(!review[0].candidates.is_empty());
    assert_eq!(jobs.items(id, ItemFilter::All).unwrap().len(), 4);
}

#[test]
fn a_selection_only_plans_those_entries() {
    let w = fill_world();
    let (jobs, rec) = (JobManager::default(), Arc::new(Recorder::default()));
    let id = planned(&w, &jobs, &rec, Some(vec![0]));
    let all = jobs.items(id, ItemFilter::All).unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].entry_id, Some(0));
}

#[test]
fn review_decisions_feed_the_apply_and_touched_entries_are_reported() {
    let w = fill_world();
    let (jobs, rec) = (JobManager::default(), Arc::new(Recorder::default()));
    let id = planned(&w, &jobs, &rec, None);
    let review = jobs.items(id, ItemFilter::Review).unwrap();
    let v = jobs
        .decide(
            id,
            &[DecisionIn {
                index: review[0].index,
                decision: Decision::Pick { candidate: 0 },
            }],
        )
        .unwrap();
    assert_eq!(v.decided, 1);
    jobs.run(w.cfg.clone(), id, rec.clone()).unwrap();
    let done = wait_for(&rec, JobState::Done);
    let result = done.result.unwrap();
    assert_eq!((result.filled, result.no_art, result.failed), (3, 1, 0));
    assert!(cover(&w, "004 Gldn Rsh").exists());
    let written = rec.written.lock().unwrap().clone();
    assert_eq!(written.len(), 1);
    let (generation, mut ids) = written[0].clone();
    ids.sort_unstable();
    assert_eq!((generation, ids), (7, vec![0, 1, 3]));
    let failed = jobs.items(id, ItemFilter::Failed).unwrap();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].name, "003 Missing Game");
    let outcome = failed[0].outcome.clone().unwrap();
    assert_eq!(outcome.status, OutcomeKind::Failed);
    assert_eq!(outcome.error, Some(ItemErrorKind::NoArt));
}

#[test]
fn undecided_review_items_are_not_written() {
    let w = fill_world();
    let (jobs, rec) = (JobManager::default(), Arc::new(Recorder::default()));
    let id = planned(&w, &jobs, &rec, None);
    jobs.run(w.cfg.clone(), id, rec.clone()).unwrap();
    assert_eq!(wait_for(&rec, JobState::Done).result.unwrap().filled, 2);
    assert!(!cover(&w, "004 Gldn Rsh").exists());
    let review = jobs.items(id, ItemFilter::Review).unwrap();
    assert_eq!(
        review[0].outcome.as_ref().unwrap().status,
        OutcomeKind::SkippedReview
    );
}

#[test]
fn a_skipped_item_is_not_written() {
    let w = fill_world();
    let (jobs, rec) = (JobManager::default(), Arc::new(Recorder::default()));
    let id = planned(&w, &jobs, &rec, None);
    jobs.decide(
        id,
        &[DecisionIn {
            index: 0,
            decision: Decision::Skip,
        }],
    )
    .unwrap();
    jobs.run(w.cfg.clone(), id, rec.clone()).unwrap();
    wait_for(&rec, JobState::Done);
    assert!(!cover(&w, "001 Golden Sun").exists());
    assert!(cover(&w, "002 Golden Adventure").exists());
}

#[test]
fn resume_after_a_rate_limit_finishes_the_same_journal() {
    let w = fill_world();
    let (jobs, rec) = (JobManager::default(), Arc::new(Recorder::default()));
    let id = planned(&w, &jobs, &rec, None);
    w.server.mock(
        ADVENTURE_USA,
        MockResponse::status(429).header("Retry-After", "0"),
    );
    // One worker: 001 is written, then 002 meets the rate limit.
    let serial = Arc::new(FillConfig {
        net: NetConfig {
            concurrency: 1,
            retries: 0,
            ..NetConfig::default()
        },
        ..(*w.cfg).clone()
    });
    jobs.run(serial.clone(), id, rec.clone()).unwrap();
    let paused = wait_for(&rec, JobState::Paused);
    let why = paused.paused.clone().unwrap();
    assert_eq!((why.phase, why.kind), (Phase::Apply, PauseKind::RateLimit));
    let journal = paused.journal.clone().unwrap();
    w.server
        .mock(ADVENTURE_USA, MockResponse::ok(b"<html>oops</html>".to_vec()));
    jobs.resume(serial, id, rec.clone()).unwrap();
    let done = wait_for(&rec, JobState::Done);
    assert_eq!(done.journal, Some(journal));
    assert_eq!(done.result.unwrap().filled, 2);
    assert_eq!(list_jobs(&w.cfg.data_dir).unwrap().len(), 1);
    assert_eq!(undo_last(&w.cfg.data_dir).unwrap().removed.len(), 2);
}

#[test]
fn a_second_job_is_refused_while_one_runs() {
    let w = fill_world();
    let (jobs, rec) = (JobManager::default(), Arc::new(Recorder::default()));
    let hang = HangingServer::start();
    let slow = Arc::new(FillConfig {
        endpoints: Endpoints {
            libretro: hang.base(),
            libretro_raw: hang.base(),
            database: hang.base(),
        },
        net: NetConfig {
            retries: 0,
            timeout: Duration::from_millis(500),
            ..NetConfig::default()
        },
        ..(*w.cfg).clone()
    });
    let s = session(&w);
    let first = jobs
        .start(slow.clone(), s.clone(), &request(None), rec.clone())
        .unwrap();
    assert_eq!(
        jobs.start(slow, s.clone(), &request(None), rec.clone()),
        Err(IpcError::JobRunning)
    );
    assert!(jobs.busy());
    jobs.cancel(first.id).unwrap();
    assert_eq!(wait_for(&rec, JobState::Cancelled).id, first.id);
    assert!(!jobs.busy());
    // A stopped job is replaced; its id is then stale.
    jobs.start(w.cfg.clone(), s, &request(None), rec.clone())
        .unwrap();
    assert_eq!(jobs.cancel(first.id), Err(IpcError::StaleJob));
    wait_for(&rec, JobState::Planned);
}

#[test]
fn run_twice_is_refused_and_decisions_need_a_planned_job() {
    let w = fill_world();
    let (jobs, rec) = (JobManager::default(), Arc::new(Recorder::default()));
    let id = planned(&w, &jobs, &rec, None);
    jobs.run(w.cfg.clone(), id, rec.clone()).unwrap();
    assert!(matches!(
        jobs.run(w.cfg.clone(), id, rec.clone()),
        Err(IpcError::BadRequest(_))
    ));
    assert!(matches!(
        jobs.decide(
            id,
            &[DecisionIn {
                index: 0,
                decision: Decision::Accept
            }]
        ),
        Err(IpcError::BadRequest(_))
    ));
    wait_for(&rec, JobState::Done);
}

#[test]
fn bad_requests_are_refused_before_any_work() {
    let w = fill_world();
    let (jobs, rec) = (JobManager::default(), Arc::new(Recorder::default()));
    let mut screenshots = request(None);
    screenshots.media = vec![Media::Screenshot];
    assert!(matches!(
        jobs.start(w.cfg.clone(), session(&w), &screenshots, rec.clone()),
        Err(IpcError::BadRequest(_))
    ));
    let mut mars = request(None);
    mars.regions = vec!["mars".into()];
    assert!(matches!(
        jobs.start(w.cfg.clone(), session(&w), &mars, rec.clone()),
        Err(IpcError::BadRequest(_))
    ));
    assert_eq!(jobs.current(), None);
    let id = planned(&w, &jobs, &rec, None);
    assert!(matches!(
        jobs.decide(
            id,
            &[DecisionIn {
                index: 99,
                decision: Decision::Accept
            }]
        ),
        Err(IpcError::BadRequest(_))
    ));
}

#[test]
fn candidate_images_are_png_previews() {
    let w = fill_world();
    let (jobs, rec) = (JobManager::default(), Arc::new(Recorder::default()));
    let id = planned(&w, &jobs, &rec, None);
    let bytes = jobs.candidate_image(&w.cfg, id, 0, 0).unwrap();
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    assert!(matches!(
        jobs.candidate_image(&w.cfg, id, 0, 9),
        Err(IpcError::BadRequest(_))
    ));
}

/// Events whose progress hook panics, as a bug in a worker would.
struct Panicking(Recorder);

impl JobEvents for Panicking {
    fn progress(&self, _: JobProgressEvent) {
        panic!("boom");
    }
    fn changed(&self, view: JobView) {
        self.0.changed(view);
    }
    fn written(&self, generation: u64, ids: Vec<u32>) {
        self.0.written(generation, ids);
    }
}

#[test]
fn a_panicking_worker_fails_the_job_instead_of_leaving_it_busy() {
    let w = fill_world();
    let jobs = JobManager::default();
    let ev = Arc::new(Panicking(Recorder::default()));
    jobs.start(w.cfg.clone(), session(&w), &request(None), ev.clone())
        .unwrap();
    let failed = wait_for(&ev.0, JobState::Failed);
    assert!(failed.error.is_some());
    assert!(!jobs.busy());
    // The manager still works: a new job plans normally.
    let rec = Arc::new(Recorder::default());
    planned(&w, &jobs, &rec, None);
}

#[test]
fn cancelling_a_planned_job_ends_it() {
    let w = fill_world();
    let (jobs, rec) = (JobManager::default(), Arc::new(Recorder::default()));
    let id = planned(&w, &jobs, &rec, None);
    assert_eq!(jobs.cancel(id).unwrap().state, JobState::Cancelled);
    assert!(matches!(
        jobs.run(w.cfg.clone(), id, rec.clone()),
        Err(IpcError::BadRequest(_))
    ));
}
```

- [ ] **Step 3: Run them to see them fail**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && cargo test -p coverly-roms --test fill_job`
Expected: FAIL to compile — `JobManager`, `JobEvents` do not exist.

- [ ] **Step 4: Implement the job manager**

Replace `src-tauri/src/jobs/mod.rs` with (keeping `FillConfig` from Task 4):

```rust
//! Fill jobs of the desktop app (spec §7 steps 3–9, §8 Jobs). At most one
//! job exists at a time: the dry run gathers on a worker thread
//! (`gathering` → `planned`), review decisions are recorded on the job,
//! `run` applies the decided plan on another thread (`applying` → `done`);
//! a rate limit, going offline or a full/read-only volume pauses it, and
//! resume continues into the same journal (D21); cancel stops it. The
//! webview follows it through [`JobEvents`].

pub mod view;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use coverly_core::art::resolve_art;
use coverly_core::cache::{Cache, CACHE_FILE};
use coverly_core::fill::{Endpoints, FillEnv, FillSettings};
use coverly_core::gather::Scope;
use coverly_core::jobs::{ApplyOptions, ItemStatus, JobReport};
use coverly_core::matching::{default_region_order, parse_region_order};
use coverly_core::net::{NetConfig, PoliteHttp, UreqHttp};
use coverly_core::plan::{Decision, Plan, PlanAction};
use coverly_core::profiles::Media;
use coverly_core::progress::{PauseKind, Phase, ProgressEvent, ProgressSink};
use coverly_core::systems::SystemDb;
use coverly_core::write::OverwritePolicy;

use crate::error::IpcError;
use crate::library::refresh::{entry_index, rom_path, subset_scan};
use crate::library::Session;
use view::{
    candidates_of, DecisionIn, FillRequest, ItemFilter, JobItemView, JobProgressEvent,
    JobProgressView, JobState, JobTotalsView, JobView, PauseView, PlanSummaryView,
};

/// Where fills keep their cache and journals, and how they reach the network.
#[derive(Clone, Debug)]
pub struct FillConfig {
    pub cache_dir: PathBuf,
    pub data_dir: PathBuf,
    pub endpoints: Endpoints,
    pub net: NetConfig,
}

impl FillConfig {
    /// The real servers with the default network settings.
    pub fn new(cache_dir: PathBuf, data_dir: PathBuf) -> FillConfig {
        FillConfig {
            cache_dir,
            data_dir,
            endpoints: Endpoints::default(),
            net: NetConfig::default(),
        }
    }
}

/// Where a job reports to (Tauri events in the app, a recorder in tests).
pub trait JobEvents: Send + Sync {
    /// Throttled progress (at most ~20 a second; first and last always).
    fn progress(&self, event: JobProgressEvent);
    /// The job changed state.
    fn changed(&self, view: JobView);
    /// Art was written for `ids` of library `generation` (sent before the
    /// final `changed`).
    fn written(&self, generation: u64, ids: Vec<u32>);
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Runs `f` with a fill environment on `cfg`: its own cache connection
/// and HTTP client.
pub fn with_env<T>(
    cfg: &FillConfig,
    f: impl FnOnce(&FillEnv<'_>) -> Result<T, IpcError>,
) -> Result<T, IpcError> {
    let cache =
        Cache::open(&cfg.cache_dir.join(CACHE_FILE)).map_err(|e| IpcError::Io(e.to_string()))?;
    let http = PoliteHttp::new(UreqHttp::new(&cfg.net), &cfg.net);
    let env = FillEnv {
        cache: &cache,
        http: &http,
        endpoints: &cfg.endpoints,
        cache_dir: &cfg.cache_dir,
        data_dir: &cfg.data_dir,
    };
    f(&env)
}

struct Job {
    id: u64,
    session: Arc<Session>,
    /// `None` = the whole library.
    entry_filter: Option<Vec<u32>>,
    settings: FillSettings,
    overwrite: OverwritePolicy,
    cancel: AtomicBool,
    data: Mutex<JobData>,
}

struct JobData {
    state: JobState,
    progress: Option<JobProgressView>,
    paused: Option<PauseView>,
    /// The plan as gathered (decisions are applied when it runs).
    plan: Option<Plan>,
    /// Entry id of each plan item.
    entry_ids: Vec<Option<u32>>,
    decisions: BTreeMap<usize, Decision>,
    /// Outcomes, aligned with `plan.items`.
    report: Option<JobReport>,
    error: Option<String>,
}

impl JobData {
    fn new() -> JobData {
        JobData {
            state: JobState::Gathering,
            progress: None,
            paused: None,
            plan: None,
            entry_ids: Vec::new(),
            decisions: BTreeMap::new(),
            report: None,
            error: None,
        }
    }

    /// The plan with the review decisions applied.
    fn decided_plan(&self) -> Option<Plan> {
        let mut plan = self.plan.clone()?;
        plan.decide(&self.decisions);
        Some(plan)
    }
}

impl Job {
    fn running(&self) -> bool {
        matches!(
            lock(&self.data).state,
            JobState::Gathering | JobState::Applying
        )
    }

    fn view(&self) -> JobView {
        let d = lock(&self.data);
        JobView {
            id: self.id,
            generation: self.session.generation,
            state: d.state,
            progress: d.progress.clone(),
            paused: d.paused.clone(),
            plan: d.plan.as_ref().map(PlanSummaryView::of),
            decided: d.decisions.len(),
            journal: d.report.as_ref().and_then(|r| r.job_id.clone()),
            result: d.report.as_ref().map(JobTotalsView::of),
            error: d.error.clone(),
        }
    }
}

/// Core progress → job state + throttled events.
struct Bridge<'a> {
    job: &'a Job,
    events: &'a dyn JobEvents,
    last: Mutex<Option<Instant>>,
}

impl<'a> Bridge<'a> {
    fn new(job: &'a Job, events: &'a dyn JobEvents) -> Bridge<'a> {
        Bridge {
            job,
            events,
            last: Mutex::new(None),
        }
    }
}

impl ProgressSink for Bridge<'_> {
    fn event(&self, e: ProgressEvent) {
        let (phase, done, total, label) = match e {
            ProgressEvent::Started { phase, total } => (phase, 0, total, String::new()),
            ProgressEvent::Item {
                phase,
                done,
                total,
                label,
            } => (phase, done, total, label),
            ProgressEvent::Warning(_) | ProgressEvent::Finished { .. } => return,
        };
        lock(&self.job.data).progress = Some(JobProgressView {
            phase,
            done,
            total,
            label: label.clone(),
        });
        let mut last = lock(&self.last);
        let due = done == 0
            || done + 1 >= total
            || last.is_none_or(|t| t.elapsed() >= Duration::from_millis(50));
        if due {
            *last = Some(Instant::now());
            drop(last);
            self.events.progress(JobProgressEvent {
                job: self.job.id,
                phase,
                done,
                total,
                label,
            });
        }
    }
}

fn settings_for(session: &Session, req: &FillRequest) -> Result<FillSettings, IpcError> {
    let mut media: Vec<Media> = Vec::new();
    for &m in &req.media {
        if session.profile.slot(m).is_some() && !media.contains(&m) {
            media.push(m);
        }
    }
    if media.is_empty() {
        return Err(IpcError::BadRequest(
            "this layout cannot store the chosen art".into(),
        ));
    }
    let mut regions = parse_region_order(&req.regions.join(",")).map_err(IpcError::BadRequest)?;
    if regions.is_empty() {
        regions = default_region_order();
    }
    Ok(FillSettings {
        media,
        scope: if req.overwrite == OverwritePolicy::Backup {
            Scope::All
        } else {
            Scope::Missing
        },
        strategy: req.strategy,
        regions,
        ..FillSettings::default()
    })
}

fn pause_view(phase: Phase, why: Option<&String>, kind: Option<PauseKind>) -> Option<PauseView> {
    why.map(|detail| PauseView {
        phase,
        kind: kind.unwrap_or(PauseKind::Offline),
        detail: detail.clone(),
    })
}

/// Runs a job phase; a panic becomes an error, so the job ends `failed`
/// instead of staying `gathering`/`applying` (and `busy`) forever.
fn guarded<T>(f: impl FnOnce() -> Result<T, IpcError>) -> Result<T, IpcError> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f))
        .unwrap_or_else(|_| Err(IpcError::Io("the fill job stopped unexpectedly".into())))
}

fn spawn_gather(job: Arc<Job>, cfg: Arc<FillConfig>, events: Arc<dyn JobEvents>) {
    std::thread::spawn(move || {
        let planned = guarded(|| with_env(&cfg, |env| {
            let db = SystemDb::builtin();
            let session = &job.session;
            let ids: Vec<u32> = match &job.entry_filter {
                Some(ids) => ids.clone(),
                None => (0..session.entries.len() as u32).collect(),
            };
            let (scan, _) = subset_scan(session, &ids);
            let art = resolve_art(&scan, &session.profile, &db, &job.settings.media);
            let root =
                std::path::absolute(&session.root).map_err(|e| IpcError::Io(e.to_string()))?;
            let sink = Bridge::new(&job, events.as_ref());
            Ok(env.plan(
                &root,
                &session.profile,
                &db,
                &art,
                &job.settings,
                &sink,
                &job.cancel,
            ))
        }));
        {
            let mut d = lock(&job.data);
            d.progress = None;
            match planned {
                Err(e) => {
                    d.state = JobState::Failed;
                    d.error = Some(e.to_string());
                }
                Ok(plan) => {
                    let index = entry_index(&job.session);
                    d.entry_ids = plan
                        .items
                        .iter()
                        .map(|i| {
                            rom_path(&job.session, &i.folder, &i.rel_path)
                                .and_then(|p| index.get(&p).copied())
                        })
                        .collect();
                    d.decisions.clear();
                    d.report = None;
                    d.paused = pause_view(Phase::Gather, plan.paused.as_ref(), plan.pause_kind);
                    d.state = if job.cancel.load(Ordering::SeqCst) {
                        JobState::Cancelled
                    } else if d.paused.is_some() {
                        JobState::Paused
                    } else {
                        JobState::Planned
                    };
                    d.plan = Some(plan);
                }
            }
        }
        events.changed(job.view());
    });
}

/// Applies `plan`. `resumed`: the original indices of `plan.items` and
/// the journal to continue.
fn spawn_apply(
    job: Arc<Job>,
    cfg: Arc<FillConfig>,
    events: Arc<dyn JobEvents>,
    plan: Plan,
    resumed: Option<(Vec<usize>, String)>,
) {
    std::thread::spawn(move || {
        let applied = guarded(|| with_env(&cfg, |env| {
            let opts = ApplyOptions {
                overwrite: job.overwrite,
                // Only auto items and accepted/picked review items (D20).
                include_review: false,
                concurrency: cfg.net.concurrency.clamp(1, 6),
                resume_job: resumed.as_ref().map(|(_, journal)| journal.clone()),
                ..ApplyOptions::default()
            };
            let sink = Bridge::new(&job, events.as_ref());
            env.apply(&plan, &job.session.profile, &opts, &sink, &job.cancel)
                .map_err(IpcError::from)
        }));
        let mut touched = Vec::new();
        {
            let mut d = lock(&job.data);
            d.progress = None;
            match applied {
                Err(e) => {
                    d.state = JobState::Failed;
                    d.error = Some(e.to_string());
                }
                Ok(report) => {
                    for (k, item) in report.items.iter().enumerate() {
                        if matches!(
                            item.status,
                            ItemStatus::Filled { .. } | ItemStatus::Replaced { .. }
                        ) {
                            let n = resumed.as_ref().map_or(k, |(indices, _)| indices[k]);
                            if let Some(Some(id)) = d.entry_ids.get(n) {
                                touched.push(*id);
                            }
                        }
                    }
                    let merged = match (resumed, d.report.take()) {
                        (Some((indices, _)), Some(mut first)) => {
                            first.merge_resumed(&indices, report);
                            first
                        }
                        _ => report,
                    };
                    d.paused = pause_view(Phase::Apply, merged.paused.as_ref(), merged.pause_kind);
                    d.state = if merged.cancelled {
                        JobState::Cancelled
                    } else if d.paused.is_some() {
                        JobState::Paused
                    } else {
                        JobState::Done
                    };
                    d.report = Some(merged);
                }
            }
        }
        if !touched.is_empty() {
            events.written(job.session.generation, touched);
        }
        events.changed(job.view());
    });
}

/// The one fill job of the app.
#[derive(Default)]
pub struct JobManager {
    seq: AtomicU64,
    current: Mutex<Option<Arc<Job>>>,
}

impl JobManager {
    /// Whether the job is gathering or applying.
    pub fn busy(&self) -> bool {
        lock(&self.current).as_ref().is_some_and(|j| j.running())
    }

    pub fn current(&self) -> Option<JobView> {
        lock(&self.current).as_ref().map(|j| j.view())
    }

    fn job(&self, id: u64) -> Result<Arc<Job>, IpcError> {
        match lock(&self.current).as_ref() {
            None => Err(IpcError::NoJob),
            Some(j) if j.id != id => Err(IpcError::StaleJob),
            Some(j) => Ok(Arc::clone(j)),
        }
    }

    /// Starts the dry run of `req` over `session`. Refused while another
    /// job gathers or applies; a stopped job is replaced.
    pub fn start(
        &self,
        cfg: Arc<FillConfig>,
        session: Arc<Session>,
        req: &FillRequest,
        events: Arc<dyn JobEvents>,
    ) -> Result<JobView, IpcError> {
        let settings = settings_for(&session, req)?;
        let job = {
            let mut slot = lock(&self.current);
            if slot.as_ref().is_some_and(|j| j.running()) {
                return Err(IpcError::JobRunning);
            }
            let job = Arc::new(Job {
                id: self.seq.fetch_add(1, Ordering::SeqCst) + 1,
                session,
                entry_filter: req.entry_ids.clone(),
                settings,
                overwrite: req.overwrite,
                cancel: AtomicBool::new(false),
                data: Mutex::new(JobData::new()),
            });
            *slot = Some(Arc::clone(&job));
            job
        };
        spawn_gather(Arc::clone(&job), cfg, events);
        Ok(job.view())
    }

    pub fn items(&self, job: u64, filter: ItemFilter) -> Result<Vec<JobItemView>, IpcError> {
        let job = self.job(job)?;
        let d = lock(&job.data);
        let Some(plan) = &d.plan else {
            return Ok(Vec::new());
        };
        let outcome = |n: usize| {
            d.report
                .as_ref()
                .and_then(|r| r.items.get(n))
                .map(|r| &r.status)
        };
        Ok(plan
            .items
            .iter()
            .enumerate()
            .filter(|(n, item)| match filter {
                ItemFilter::All => true,
                ItemFilter::Auto => matches!(item.action, PlanAction::Auto { .. }),
                ItemFilter::Review => matches!(item.action, PlanAction::Review { .. }),
                ItemFilter::Failed => matches!(outcome(*n), Some(ItemStatus::Failed { .. })),
            })
            .map(|(n, item)| {
                JobItemView::new(
                    n,
                    item,
                    d.entry_ids.get(n).copied().flatten(),
                    d.decisions.get(&n).copied(),
                    outcome(n),
                )
            })
            .collect())
    }

    /// Records review decisions (only before the job runs).
    pub fn decide(&self, job: u64, decisions: &[DecisionIn]) -> Result<JobView, IpcError> {
        let job = self.job(job)?;
        {
            let mut d = lock(&job.data);
            if d.state != JobState::Planned {
                return Err(IpcError::BadRequest(
                    "decisions are taken before the job runs".into(),
                ));
            }
            let n = d.plan.as_ref().map_or(0, |p| p.items.len());
            if let Some(bad) = decisions.iter().find(|x| x.index as usize >= n) {
                return Err(IpcError::BadRequest(format!("no item {}", bad.index)));
            }
            for x in decisions {
                d.decisions.insert(x.index as usize, x.decision);
            }
        }
        Ok(job.view())
    }

    /// Applies the decided plan on a worker thread.
    pub fn run(
        &self,
        cfg: Arc<FillConfig>,
        job: u64,
        events: Arc<dyn JobEvents>,
    ) -> Result<JobView, IpcError> {
        let job = self.job(job)?;
        let plan = {
            let mut d = lock(&job.data);
            if d.state != JobState::Planned {
                return Err(IpcError::BadRequest("the job is not ready to run".into()));
            }
            let plan = d.decided_plan().ok_or(IpcError::NoJob)?;
            d.state = JobState::Applying;
            plan
        };
        spawn_apply(Arc::clone(&job), cfg, events, plan, None);
        Ok(job.view())
    }

    /// Continues a paused job: the dry run again, or the items not done yet
    /// into the same journal.
    pub fn resume(
        &self,
        cfg: Arc<FillConfig>,
        job: u64,
        events: Arc<dyn JobEvents>,
    ) -> Result<JobView, IpcError> {
        let job = self.job(job)?;
        let mut d = lock(&job.data);
        if d.state != JobState::Paused {
            return Err(IpcError::BadRequest("the job is not paused".into()));
        }
        let phase = d.paused.take().map_or(Phase::Apply, |p| p.phase);
        if phase == Phase::Gather {
            d.state = JobState::Gathering;
            drop(d);
            spawn_gather(Arc::clone(&job), cfg, events);
        } else {
            let plan = d.decided_plan().ok_or(IpcError::NoJob)?;
            let report = d.report.as_ref().ok_or(IpcError::NoJob)?;
            let indices = report.resumable_items();
            let (todo, resumed) = match report.job_id.clone() {
                Some(journal) => (plan.subset(&indices), Some((indices, journal))),
                None => (plan, None),
            };
            d.state = JobState::Applying;
            drop(d);
            spawn_apply(Arc::clone(&job), cfg, events, todo, resumed);
        }
        Ok(job.view())
    }

    /// Stops a running job (its thread ends it as `cancelled`) or ends a
    /// planned or paused one.
    pub fn cancel(&self, job: u64) -> Result<JobView, IpcError> {
        let job = self.job(job)?;
        job.cancel.store(true, Ordering::SeqCst);
        {
            let mut d = lock(&job.data);
            if matches!(d.state, JobState::Planned | JobState::Paused) {
                d.state = JobState::Cancelled;
                d.paused = None;
            }
        }
        Ok(job.view())
    }

    /// Candidate `candidate` of item `index`, re-encoded as it would be written.
    pub fn candidate_image(
        &self,
        cfg: &FillConfig,
        job: u64,
        index: u32,
        candidate: u32,
    ) -> Result<Vec<u8>, IpcError> {
        let job = self.job(job)?;
        let c = {
            let d = lock(&job.data);
            let item = d
                .plan
                .as_ref()
                .and_then(|p| p.items.get(index as usize))
                .ok_or_else(|| IpcError::BadRequest(format!("no item {index}")))?;
            candidates_of(item)
                .get(candidate as usize)
                .map(|c| (*c).clone())
                .ok_or_else(|| IpcError::BadRequest(format!("no candidate {candidate}")))?
        };
        with_env(cfg, |env| {
            Ok(env.candidate_image(&job.session.profile, &c)?.bytes)
        })
    }

    /// The outcomes so far (for exports).
    pub fn report(&self, job: u64) -> Result<JobReport, IpcError> {
        let job = self.job(job)?;
        let d = lock(&job.data);
        d.report
            .clone()
            .ok_or_else(|| IpcError::BadRequest("the job has not run yet".into()))
    }
}
```

- [ ] **Step 5: Run the tests**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && cargo test -p coverly-roms --test fill_job`
Expected: PASS (the hanging-server test takes about 0.5 s; the panicking-worker test prints the `boom` panic message, which is expected).

- [ ] **Step 6: Gates and commit**

Run all gates. Then:

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && git add src-tauri && git commit -m "feat(app): run fill jobs with progress, review decisions, pause, resume and cancel"
```

---
### Task 6: App — job commands, events, undo of the last job and history

**Files:**
- Modify: `src-tauri/src/state.rs` (`jobs: JobManager`, `jobs()`, `start_fill`, `undo_last_job`, `job_history`)
- Modify: `src-tauri/src/ipc.rs` (`TauriEvents`, event names, job commands)
- Modify: `src-tauri/src/lib.rs` (register the commands)
- Test: `src-tauri/tests/fill_state.rs`

**Interfaces:**
- Consumes: Task 5 `JobManager`, `JobEvents`; Task 4 `refresh_entries`, `ids_for_art_paths`, `UndoView`, `JobHistoryView`, `EntriesUpdatedEvent`; core `undo_last`, `list_jobs`.
- Produces:
  - `AppState::jobs(&self) -> &JobManager`
  - `AppState::start_fill(&self, generation: u64, req: &FillRequest, events: Arc<dyn JobEvents>) -> Result<JobView, IpcError>`
  - `AppState::undo_last_job(&self, generation: u64) -> Result<UndoView, IpcError>` (the newest not-undone journal whose root is the open library; `job-running` while a job gathers, applies or is paused; `nothing-to-undo` when this library has none; refreshes the entries whose art it touched)
  - `AppState::job_history(&self) -> Result<Vec<JobHistoryView>, IpcError>`
  - Commands (JS name → args → result): `start_fill` (`generation`, `request`) → `JobView`; `fill_job` → `JobView | null`; `fill_job_items` (`job`, `filter`) → `JobItemView[]`; `decide_fill` (`job`, `decisions`) → `JobView`; `run_fill` (`job`) → `JobView`; `resume_fill` (`job`) → `JobView`; `cancel_fill` (`job`) → `JobView`; `candidate_art` (`job`, `index`, `candidate`) → bytes; `undo_last_job` (`generation`) → `UndoView`; `job_history` → `JobHistoryView[]`
  - Events: `fill-job-progress` (`JobProgressEvent`), `fill-job-changed` (`JobView`), `library-entries-updated` (`EntriesUpdatedEvent`)

- [ ] **Step 1: Write the failing state tests**

`src-tauri/tests/fill_state.rs`:

```rust
mod common;

use std::sync::Arc;
use std::time::Duration;

use common::fill::{fill_world, request, wait_for, FillWorld, Recorder, ADVENTURE_USA};
use coverly_core::fill::Endpoints;
use coverly_core::net::testing::{HangingServer, MockResponse};
use coverly_core::net::NetConfig;
use coverly_core::profiles::Media;
use coverly_core::settings::LIBRARY_SETTINGS_FILE;
use coverly_core::systems::SystemDb;
use coverly_roms_lib::error::IpcError;
use coverly_roms_lib::jobs::view::{JobProgressEvent, JobState, JobView};
use coverly_roms_lib::jobs::{FillConfig, JobEvents};
use coverly_roms_lib::library::art::read_art_bytes;
use coverly_roms_lib::library::view::ArtMark;
use coverly_roms_lib::state::AppState;

/// Events that refresh the state the way the Tauri ones do.
struct Refreshing {
    state: Arc<AppState>,
    rec: Recorder,
}

impl JobEvents for Refreshing {
    fn progress(&self, event: JobProgressEvent) {
        self.rec.progress(event);
    }
    fn changed(&self, view: JobView) {
        self.rec.changed(view);
    }
    fn written(&self, generation: u64, ids: Vec<u32>) {
        let _ = self
            .state
            .refresh_entries(generation, &ids, &SystemDb::builtin());
        self.rec.written(generation, ids);
    }
}

fn opened(w: &FillWorld, cfg: FillConfig) -> (Arc<AppState>, Arc<Refreshing>, u64) {
    let st = Arc::new(AppState::new(
        w.dirs.path().join(LIBRARY_SETTINGS_FILE),
        cfg,
    ));
    let view = st
        .open(w.lib.path(), None, &SystemDb::builtin(), &mut |_| {})
        .unwrap();
    let ev = Arc::new(Refreshing {
        state: st.clone(),
        rec: Recorder::default(),
    });
    (st, ev, view.generation)
}

#[test]
fn start_with_a_stale_generation_is_refused() {
    let w = fill_world();
    let (st, ev, generation) = opened(&w, (*w.cfg).clone());
    assert_eq!(
        st.start_fill(generation + 1, &request(None), ev),
        Err(IpcError::StaleLibrary)
    );
}

#[test]
fn a_finished_job_updates_the_library_and_undo_restores_it() {
    let w = fill_world();
    let (st, ev, generation) = opened(&w, (*w.cfg).clone());
    let id = st
        .start_fill(generation, &request(None), ev.clone())
        .unwrap()
        .id;
    wait_for(&ev.rec, JobState::Planned);
    st.jobs().run(st.fill_config(), id, ev.clone()).unwrap();
    wait_for(&ev.rec, JobState::Done);
    let session = st.session(generation).unwrap();
    assert!(read_art_bytes(&session, 0, Media::Cover).is_ok());

    let history = st.job_history().unwrap();
    assert_eq!(history.len(), 1);
    assert!(history[0].complete && !history[0].undone);
    assert_eq!(history[0].entries, 2);

    let undo = st.undo_last_job(generation).unwrap();
    assert_eq!((undo.removed, undo.restored, undo.kept), (2, 0, 0));
    let mut ids: Vec<u32> = undo.entries.iter().map(|e| e.id).collect();
    ids.sort_unstable();
    assert_eq!(ids, vec![0, 1]);
    assert!(undo.entries.iter().all(|e| e.art.cover == ArtMark::Missing));
    assert_eq!(
        read_art_bytes(&st.session(generation).unwrap(), 0, Media::Cover),
        Err(IpcError::ArtMissing("cover".into()))
    );
    assert!(st.job_history().unwrap()[0].undone);
    assert_eq!(st.undo_last_job(generation), Err(IpcError::NothingToUndo));
}

#[test]
fn undo_is_refused_while_a_job_runs() {
    let w = fill_world();
    let hang = HangingServer::start();
    let slow = FillConfig {
        endpoints: Endpoints {
            libretro: hang.base(),
            libretro_raw: hang.base(),
            database: hang.base(),
        },
        net: NetConfig {
            retries: 0,
            timeout: Duration::from_millis(500),
            ..NetConfig::default()
        },
        ..(*w.cfg).clone()
    };
    let (st, ev, generation) = opened(&w, slow);
    let id = st
        .start_fill(generation, &request(None), ev.clone())
        .unwrap()
        .id;
    assert_eq!(st.undo_last_job(generation), Err(IpcError::JobRunning));
    st.jobs().cancel(id).unwrap();
    wait_for(&ev.rec, JobState::Cancelled);
}

#[test]
fn undo_is_refused_while_a_job_is_paused() {
    let w = fill_world();
    let serial = FillConfig {
        net: NetConfig {
            concurrency: 1,
            retries: 0,
            ..NetConfig::default()
        },
        ..(*w.cfg).clone()
    };
    let (st, ev, generation) = opened(&w, serial);
    let id = st
        .start_fill(generation, &request(None), ev.clone())
        .unwrap()
        .id;
    wait_for(&ev.rec, JobState::Planned);
    w.server.mock(
        ADVENTURE_USA,
        MockResponse::status(429).header("Retry-After", "0"),
    );
    st.jobs().run(st.fill_config(), id, ev.clone()).unwrap();
    wait_for(&ev.rec, JobState::Paused);
    // Resume appends to this journal: undoing it now would break the resume.
    assert_eq!(st.undo_last_job(generation), Err(IpcError::JobRunning));
    st.jobs().cancel(id).unwrap();
    assert_eq!(st.undo_last_job(generation).unwrap().removed, 1);
}

#[test]
fn undo_only_touches_the_open_library() {
    let w = fill_world();
    let (st, ev, generation) = opened(&w, (*w.cfg).clone());
    let id = st
        .start_fill(generation, &request(None), ev.clone())
        .unwrap()
        .id;
    wait_for(&ev.rec, JobState::Planned);
    st.jobs().run(st.fill_config(), id, ev.clone()).unwrap();
    wait_for(&ev.rec, JobState::Done);
    let cover = w.lib.path().join("ROMS/GBA/Imgs/001 Golden Sun.png");
    assert!(cover.exists());
    // Another library is opened: the first library's job is not its to undo.
    let other = common::anbernic_tree(&["GBA"], 2, 0);
    let view = st
        .open(other.path(), None, &SystemDb::builtin(), &mut |_| {})
        .unwrap();
    assert_eq!(
        st.undo_last_job(view.generation),
        Err(IpcError::NothingToUndo)
    );
    assert!(cover.exists());
    assert!(!st.job_history().unwrap()[0].undone);
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && cargo test -p coverly-roms --test fill_state`
Expected: FAIL to compile — `start_fill`, `jobs`, `undo_last_job`, `job_history` do not exist.

- [ ] **Step 3: State methods**

`src-tauri/src/state.rs`:
- field `jobs: JobManager` (initialised with `JobManager::default()` in `new`).
- imports: add `use crate::jobs::view::{FillRequest, JobHistoryView, JobState, JobView, UndoView};` and `use coverly_core::write::{list_jobs, undo_job};`; the Task 4 imports become `use crate::jobs::{FillConfig, JobEvents, JobManager};` and `use crate::library::refresh::{ids_for_art_paths, refresh_entries, EntryArtUpdate};`.
- methods:

```rust
    pub fn jobs(&self) -> &JobManager {
        &self.jobs
    }

    /// Starts the dry run of a fill on the open library `generation`.
    pub fn start_fill(
        &self,
        generation: u64,
        req: &FillRequest,
        events: Arc<dyn JobEvents>,
    ) -> Result<JobView, IpcError> {
        let session = self.session(generation)?;
        self.jobs.start(self.fill_config(), session, req, events)
    }

    /// Undoes the newest not-undone journal of the open library
    /// `generation` (fill, drawer change, removal) and refreshes the
    /// entries it touched. Refused while a job gathers, applies or is
    /// paused (a paused job resumes into its journal); journals of other
    /// libraries are never touched. Journals keep `absolute(session.root)`
    /// (the root of every app write, Tasks 5 and 7).
    pub fn undo_last_job(&self, generation: u64) -> Result<UndoView, IpcError> {
        let paused = self
            .jobs
            .current()
            .is_some_and(|j| j.state == JobState::Paused);
        if self.jobs.busy() || paused {
            return Err(IpcError::JobRunning);
        }
        let session = self.session(generation)?;
        let root = std::path::absolute(&session.root).map_err(|e| IpcError::Io(e.to_string()))?;
        let job = list_jobs(&self.fill.data_dir)?
            .into_iter()
            .find(|j| !j.undone && j.root == root)
            .ok_or(IpcError::NothingToUndo)?;
        let report = undo_job(&self.fill.data_dir, &job.job)?;
        let paths: Vec<PathBuf> = report
            .removed
            .iter()
            .chain(&report.restored)
            .cloned()
            .collect();
        let ids = ids_for_art_paths(&session, &paths);
        // The undo is done; a library reopened meanwhile only loses the refresh.
        let entries = self
            .refresh_entries(generation, &ids, &SystemDb::builtin())
            .unwrap_or_default();
        Ok(UndoView::new(&report, entries))
    }

    /// Every journal kept, newest first.
    pub fn job_history(&self) -> Result<Vec<JobHistoryView>, IpcError> {
        Ok(list_jobs(&self.fill.data_dir)?
            .into_iter()
            .map(JobHistoryView::from)
            .collect())
    }
```

- [ ] **Step 4: Commands and events**

`src-tauri/src/ipc.rs` — imports:

```rust
use std::sync::Arc;

use crate::jobs::view::{
    DecisionIn, EntriesUpdatedEvent, FillRequest, ItemFilter, JobHistoryView, JobItemView,
    JobProgressEvent, JobView, UndoView,
};
use crate::jobs::JobEvents;
```

then:

```rust
/// Event carrying `JobProgressEvent` (throttled).
pub const JOB_PROGRESS_EVENT: &str = "fill-job-progress";
/// Event carrying the `JobView` after every state change.
pub const JOB_CHANGED_EVENT: &str = "fill-job-changed";
/// Event carrying `EntriesUpdatedEvent` after art was written.
pub const ENTRIES_UPDATED_EVENT: &str = "library-entries-updated";

/// Job events as Tauri events; written art refreshes the open library.
struct TauriEvents(AppHandle);

impl JobEvents for TauriEvents {
    fn progress(&self, event: JobProgressEvent) {
        let _ = self.0.emit(JOB_PROGRESS_EVENT, event);
    }
    fn changed(&self, view: JobView) {
        let _ = self.0.emit(JOB_CHANGED_EVENT, view);
    }
    fn written(&self, generation: u64, ids: Vec<u32>) {
        let state = self.0.state::<AppState>();
        if let Ok(entries) = state.refresh_entries(generation, &ids, &SystemDb::builtin()) {
            let _ = self.0.emit(
                ENTRIES_UPDATED_EVENT,
                EntriesUpdatedEvent {
                    generation,
                    entries,
                },
            );
        }
    }
}

fn job_events(app: &AppHandle) -> Arc<dyn JobEvents> {
    Arc::new(TauriEvents(app.clone()))
}

/// Starts the dry run of a fill (progress and the result arrive as events).
#[tauri::command]
pub async fn start_fill(
    app: AppHandle,
    generation: u64,
    request: FillRequest,
) -> Result<JobView, IpcError> {
    let events = job_events(&app);
    blocking(move || {
        app.state::<AppState>()
            .start_fill(generation, &request, events)
    })
    .await
}

#[tauri::command]
pub fn fill_job(state: State<'_, AppState>) -> Option<JobView> {
    state.jobs().current()
}

#[tauri::command]
pub async fn fill_job_items(
    app: AppHandle,
    job: u64,
    filter: ItemFilter,
) -> Result<Vec<JobItemView>, IpcError> {
    blocking(move || app.state::<AppState>().jobs().items(job, filter)).await
}

#[tauri::command]
pub fn decide_fill(
    state: State<'_, AppState>,
    job: u64,
    decisions: Vec<DecisionIn>,
) -> Result<JobView, IpcError> {
    state.jobs().decide(job, &decisions)
}

#[tauri::command]
pub fn run_fill(app: AppHandle, state: State<'_, AppState>, job: u64) -> Result<JobView, IpcError> {
    state.jobs().run(state.fill_config(), job, job_events(&app))
}

#[tauri::command]
pub fn resume_fill(
    app: AppHandle,
    state: State<'_, AppState>,
    job: u64,
) -> Result<JobView, IpcError> {
    state.jobs().resume(state.fill_config(), job, job_events(&app))
}

#[tauri::command]
pub fn cancel_fill(state: State<'_, AppState>, job: u64) -> Result<JobView, IpcError> {
    state.jobs().cancel(job)
}

/// A candidate image of a plan item, re-encoded as it would be written.
#[tauri::command]
pub async fn candidate_art(
    app: AppHandle,
    job: u64,
    index: u32,
    candidate: u32,
) -> Result<tauri::ipc::Response, IpcError> {
    blocking(move || {
        let state = app.state::<AppState>();
        state
            .jobs()
            .candidate_image(&state.fill_config(), job, index, candidate)
            .map(tauri::ipc::Response::new)
    })
    .await
}

#[tauri::command]
pub async fn undo_last_job(app: AppHandle, generation: u64) -> Result<UndoView, IpcError> {
    blocking(move || app.state::<AppState>().undo_last_job(generation)).await
}

#[tauri::command]
pub async fn job_history(app: AppHandle) -> Result<Vec<JobHistoryView>, IpcError> {
    blocking(move || app.state::<AppState>().job_history()).await
}
```

`src-tauri/src/lib.rs` — `generate_handler!` gains, after `ipc::reveal_entry,`:

```rust
            ipc::start_fill,
            ipc::fill_job,
            ipc::fill_job_items,
            ipc::decide_fill,
            ipc::run_fill,
            ipc::resume_fill,
            ipc::cancel_fill,
            ipc::candidate_art,
            ipc::undo_last_job,
            ipc::job_history,
```

- [ ] **Step 5: Run the tests**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && cargo test -p coverly-roms`
Expected: PASS (`config_security` unchanged: no new capability).

- [ ] **Step 6: Gates and commit**

Run all gates. Then:

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && git add src-tauri && git commit -m "feat(app): expose fill jobs, undo of the last job and job history over IPC"
```

---

### Task 7: App — drawer actions (change, from file, remove) and report exports

**Files:**
- Create: `crates/coverly-core/src/csv_util.rs`; Modify: `crates/coverly-core/src/audit/csv.rs`, `crates/coverly-core/src/lib.rs` (`mod csv_util;`)
- Modify: `crates/coverly-core/src/jobs/mod.rs` (`ItemErrorKind::as_str`, `write_report_csv`)
- Modify: `src-tauri/src/library/art.rs` (`media_name` → `pub(crate)`)
- Modify: `src-tauri/src/state.rs` (`picks`, drawer and export methods)
- Modify: `src-tauri/src/ipc.rs`, `src-tauri/src/lib.rs` (commands)
- Test: `crates/coverly-core/tests/job_report.rs`, `src-tauri/tests/drawer_actions.rs`

**Interfaces:**
- Consumes: Task 3 `FillEnv`, `write_local_image`, `remove_art`, `Decision`; Task 4 `subset_scan`, `refresh_entries`, `inside_root`, `candidate_views`, `candidates_of`, `ExportFormat`; Task 5 `with_env`, `JobManager::report`; core `build_report`, `write_csv`, `target_path`.
- Produces:
  - `pub fn coverly_core::jobs::write_report_csv<W: Write>(report: &JobReport, w: W) -> io::Result<()>` — columns `folder,rel_path,media,status,source,name,error,message`
  - `pub fn ItemErrorKind::as_str(self) -> &'static str`
  - `AppState::entry_candidates(&self, generation: u64, id: u32, media: Media) -> Result<Vec<CandidateView>, IpcError>`
  - `AppState::entry_candidate_art(&self, generation: u64, id: u32, media: Media, candidate: u32) -> Result<Vec<u8>, IpcError>`
  - `AppState::set_entry_art(&self, generation: u64, id: u32, media: Media, candidate: u32) -> Result<Vec<EntryArtUpdate>, IpcError>`
  - `AppState::set_entry_art_from_file(&self, generation: u64, id: u32, media: Media, file: &Path) -> Result<Vec<EntryArtUpdate>, IpcError>`
  - `AppState::remove_entry_art(&self, generation: u64, id: u32, media: Media) -> Result<Vec<EntryArtUpdate>, IpcError>`
  - `AppState::export_audit(&self, generation: u64, format: ExportFormat, dest: &Path) -> Result<(), IpcError>`
  - `AppState::export_job_report(&self, job: u64, format: ExportFormat, dest: &Path) -> Result<(), IpcError>`
  - Commands: `entry_candidates` (`generation`, `entryId`, `media`) → `CandidateView[]`; `entry_candidate_art` (`generation`, `entryId`, `media`, `candidate`) → bytes; `set_entry_art` (… `candidate`) → `EntryArtUpdate[]`; `set_entry_art_from_file` (`generation`, `entryId`, `media`; native open dialog in Rust) → `EntryArtUpdate[] | null`; `remove_entry_art` (`generation`, `entryId`, `media`) → `EntryArtUpdate[]`; `export_audit` (`generation`, `format`; native save dialog) → file name `| null`; `export_job_report` (`job`, `format`) → file name `| null`

Every drawer write is refused with `job-running` while a job gathers or applies, is journaled (one undo each) and replaces existing art with a backup.

- [ ] **Step 1: Write the failing core report test**

`crates/coverly-core/tests/job_report.rs`:

```rust
use std::path::PathBuf;

use coverly_core::jobs::{write_report_csv, ItemErrorKind, ItemResult, ItemStatus, JobReport};
use coverly_core::profiles::Media;

#[test]
fn job_report_csv_neutralises_formulas() {
    let report = JobReport {
        items: vec![
            ItemResult {
                folder: "GBA".into(),
                rel_path: PathBuf::from("=HYPERLINK(\"http://x\").gba"),
                media: Media::Cover,
                target: PathBuf::from("/sd/x.png"),
                status: ItemStatus::Filled {
                    source: "libretro".into(),
                    name: "Golden Sun (USA, Europe)".into(),
                },
            },
            ItemResult {
                folder: "GBA".into(),
                rel_path: PathBuf::from("b.gba"),
                media: Media::Cover,
                target: PathBuf::from("/sd/b.png"),
                status: ItemStatus::Failed {
                    kind: ItemErrorKind::Decode,
                    message: "-cmd".into(),
                },
            },
        ],
        ..JobReport::default()
    };
    let mut out = Vec::new();
    write_report_csv(&report, &mut out).unwrap();
    assert_eq!(
        String::from_utf8(out).unwrap(),
        "folder,rel_path,media,status,source,name,error,message\n\
         GBA,\"'=HYPERLINK(\"\"http://x\"\").gba\",cover,filled,libretro,\"Golden Sun (USA, Europe)\",,\n\
         GBA,b.gba,cover,failed,,,decode,'-cmd\n"
    );
}

#[test]
fn error_kinds_use_their_wire_names() {
    for kind in [
        ItemErrorKind::NoArt,
        ItemErrorKind::Network,
        ItemErrorKind::Decode,
        ItemErrorKind::WriteDenied,
        ItemErrorKind::DiskFull,
        ItemErrorKind::NameInvalid,
        ItemErrorKind::OutsideRoot,
        ItemErrorKind::Io,
    ] {
        assert_eq!(
            serde_json::to_value(kind).unwrap().as_str(),
            Some(kind.as_str())
        );
    }
}
```

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && cargo test -p coverly-core --test job_report`
Expected: FAIL to compile — `write_report_csv`, `as_str` do not exist.

- [ ] **Step 2: Shared CSV helpers and the job report CSV**

Create `crates/coverly-core/src/csv_util.rs` with the module doc `//! CSV output shared by the audit and job reports: RFC 4180 quoting, \`\n\` line ends, spreadsheet formulas neutralised.` and `use std::io::{self, Write};` + `use crate::profiles::Media;`, then **move** `fn write_row`, `fn quote`, `fn media_name` and the whole `#[cfg(test)] mod tests` block from `crates/coverly-core/src/audit/csv.rs` into it unchanged, making the three functions `pub(crate)`. In `audit/csv.rs` add `use crate::csv_util::{media_name, write_row};` and drop the now-unused imports. In `lib.rs` add `mod csv_util;`.

In `crates/coverly-core/src/jobs/mod.rs` add `use std::io::Write;` and `use crate::csv_util::{media_name, write_row};`, then:

```rust
impl ItemErrorKind {
    /// The stable name used on the wire and in reports (`no-art`, …).
    pub fn as_str(self) -> &'static str {
        match self {
            ItemErrorKind::NoArt => "no-art",
            ItemErrorKind::Network => "network",
            ItemErrorKind::Decode => "decode",
            ItemErrorKind::WriteDenied => "write-denied",
            ItemErrorKind::DiskFull => "disk-full",
            ItemErrorKind::NameInvalid => "name-invalid",
            ItemErrorKind::OutsideRoot => "outside-root",
            ItemErrorKind::Io => "io",
        }
    }
}

/// Writes one row per item of `report`:
/// `folder,rel_path,media,status,source,name,error,message`, quoted and
/// formula-neutralised like the audit CSV (names are hostile).
pub fn write_report_csv<W: Write>(report: &JobReport, mut w: W) -> io::Result<()> {
    let header = [
        "folder", "rel_path", "media", "status", "source", "name", "error", "message",
    ]
    .map(String::from);
    write_row(&mut w, &header)?;
    for item in &report.items {
        let (status, source, name, error, message) = match &item.status {
            ItemStatus::Filled { source, name } => ("filled", source.as_str(), name.as_str(), "", ""),
            ItemStatus::Replaced { source, name, .. } => {
                ("replaced", source.as_str(), name.as_str(), "", "")
            }
            ItemStatus::SkippedExists => ("skipped-exists", "", "", "", ""),
            ItemStatus::SkippedReview => ("skipped-review", "", "", "", ""),
            ItemStatus::SkippedNoMatch => ("skipped-no-match", "", "", "", ""),
            ItemStatus::NotAttempted => ("not-attempted", "", "", "", ""),
            ItemStatus::Failed { kind, message } => ("failed", "", "", kind.as_str(), message.as_str()),
        };
        let row = [
            item.folder.clone(),
            item.rel_path.to_string_lossy().into_owned(),
            media_name(item.media),
            status.to_string(),
            source.to_string(),
            name.to_string(),
            error.to_string(),
            message.to_string(),
        ];
        write_row(&mut w, &row)?;
    }
    w.flush()
}
```

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && cargo test -p coverly-core`
Expected: PASS (audit snapshots unchanged).

- [ ] **Step 3: Write the failing drawer and export tests**

`src-tauri/tests/drawer_actions.rs`:

```rust
mod common;

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use common::fill::{fill_world, request, wait_for, FillWorld, Recorder};
use common::write;
use coverly_core::image::probe_dimensions;
use coverly_core::net::testing::png_bytes;
use coverly_core::profiles::Media;
use coverly_core::settings::LIBRARY_SETTINGS_FILE;
use coverly_core::systems::SystemDb;
use coverly_roms_lib::error::IpcError;
use coverly_roms_lib::jobs::view::{ExportFormat, JobState};
use coverly_roms_lib::library::view::ArtMark;
use coverly_roms_lib::state::AppState;

fn opened(w: &FillWorld) -> (AppState, u64) {
    let st = AppState::new(
        w.dirs.path().join(LIBRARY_SETTINGS_FILE),
        (*w.cfg).clone(),
    );
    let view = st
        .open(w.lib.path(), None, &SystemDb::builtin(), &mut |_| {})
        .unwrap();
    (st, view.generation)
}

fn cover(w: &FillWorld, stem: &str) -> PathBuf {
    w.lib.path().join(format!("ROMS/GBA/Imgs/{stem}.png"))
}

#[test]
fn change_art_writes_the_picked_candidate_and_undo_removes_it() {
    let w = fill_world();
    let (st, generation) = opened(&w);
    let cands = st.entry_candidates(generation, 1, Media::Cover).unwrap();
    let japan = cands
        .iter()
        .find(|c| c.name == "Golden Adventure (Japan)")
        .unwrap()
        .index;
    let preview = st
        .entry_candidate_art(generation, 1, Media::Cover, japan)
        .unwrap();
    assert_eq!(&preview[..8], b"\x89PNG\r\n\x1a\n");
    let updates = st.set_entry_art(generation, 1, Media::Cover, japan).unwrap();
    assert_eq!(updates.len(), 1);
    assert_eq!((updates[0].id, updates[0].art.cover), (1, ArtMark::Present));
    assert!(cover(&w, "002 Golden Adventure").exists());
    assert_eq!(st.undo_last_job(generation).unwrap().removed, 1);
    assert!(!cover(&w, "002 Golden Adventure").exists());
}

#[test]
fn set_art_needs_the_candidates_of_that_entry() {
    let w = fill_world();
    let (st, generation) = opened(&w);
    assert!(matches!(
        st.set_entry_art(generation, 1, Media::Cover, 0),
        Err(IpcError::BadRequest(_))
    ));
    st.entry_candidates(generation, 0, Media::Cover).unwrap();
    assert!(matches!(
        st.set_entry_art(generation, 1, Media::Cover, 0),
        Err(IpcError::BadRequest(_))
    ));
    assert!(matches!(
        st.set_entry_art(generation, 0, Media::Cover, 42),
        Err(IpcError::BadRequest(_))
    ));
}

#[test]
fn a_local_image_replaces_the_art_with_a_backup() {
    let w = fill_world();
    write(w.lib.path(), "ROMS/GBA/Imgs/001 Golden Sun.png", b"old art");
    let (st, generation) = opened(&w);
    let file = w.dirs.path().join("pick.png");
    fs::write(&file, png_bytes(800, 800)).unwrap();
    let updates = st
        .set_entry_art_from_file(generation, 0, Media::Cover, &file)
        .unwrap();
    assert_eq!(updates[0].art.cover, ArtMark::Present);
    let (wd, ht) = probe_dimensions(&fs::read(cover(&w, "001 Golden Sun")).unwrap()).unwrap();
    assert!(wd <= 250 && ht <= 360, "{wd}x{ht}");
    assert_eq!(st.undo_last_job(generation).unwrap().restored, 1);
    assert_eq!(fs::read(cover(&w, "001 Golden Sun")).unwrap(), b"old art");
}

#[test]
fn a_file_that_is_not_an_image_is_refused() {
    let w = fill_world();
    let (st, generation) = opened(&w);
    let file = w.dirs.path().join("evil.png");
    fs::write(&file, b"<html><script>alert(1)</script></html>").unwrap();
    assert!(matches!(
        st.set_entry_art_from_file(generation, 0, Media::Cover, &file),
        Err(IpcError::BadImage(_))
    ));
    assert!(!cover(&w, "001 Golden Sun").exists());
}

#[test]
fn removing_art_backs_it_up_and_undo_restores_it() {
    let w = fill_world();
    write(w.lib.path(), "ROMS/GBA/Imgs/001 Golden Sun.png", b"old art");
    let (st, generation) = opened(&w);
    let updates = st.remove_entry_art(generation, 0, Media::Cover).unwrap();
    assert_eq!(updates[0].art.cover, ArtMark::Missing);
    assert!(!cover(&w, "001 Golden Sun").exists());
    let undo = st.undo_last_job(generation).unwrap();
    assert_eq!(undo.restored, 1);
    assert_eq!(undo.entries[0].art.cover, ArtMark::Present);
    assert_eq!(fs::read(cover(&w, "001 Golden Sun")).unwrap(), b"old art");
    assert_eq!(
        st.remove_entry_art(generation, 1, Media::Cover),
        Err(IpcError::ArtMissing("cover".into()))
    );
}

#[test]
fn audit_export_neutralises_formulas() {
    let w = fill_world();
    write(w.lib.path(), "ROMS/GBA/=HYPERLINK(1).gba", b"x");
    let (st, generation) = opened(&w);
    let csv = w.dirs.path().join("audit.csv");
    st.export_audit(generation, ExportFormat::Csv, &csv).unwrap();
    let text = fs::read_to_string(&csv).unwrap();
    assert!(text.starts_with("system,folder,rel_path,stem,size,modified,cover"));
    assert!(text.contains("'=HYPERLINK(1)"), "{text}");
    let json = w.dirs.path().join("audit.json");
    st.export_audit(generation, ExportFormat::Json, &json).unwrap();
    let v: serde_json::Value = serde_json::from_str(&fs::read_to_string(&json).unwrap()).unwrap();
    assert_eq!(v["totals"]["roms"], 5);
}

#[test]
fn job_report_export_lists_every_item() {
    let w = fill_world();
    let (st, generation) = opened(&w);
    let rec = Arc::new(Recorder::default());
    let id = st
        .start_fill(generation, &request(None), rec.clone())
        .unwrap()
        .id;
    wait_for(&rec, JobState::Planned);
    assert!(matches!(
        st.export_job_report(id, ExportFormat::Csv, &w.dirs.path().join("early.csv")),
        Err(IpcError::BadRequest(_))
    ));
    st.jobs().run(st.fill_config(), id, rec.clone()).unwrap();
    wait_for(&rec, JobState::Done);
    let csv = w.dirs.path().join("job.csv");
    st.export_job_report(id, ExportFormat::Csv, &csv).unwrap();
    let text = fs::read_to_string(&csv).unwrap();
    assert_eq!(text.lines().count(), 5);
    assert!(
        text.contains("GBA,003 Missing Game.gba,cover,failed,,,no-art,"),
        "{text}"
    );
    let json = w.dirs.path().join("job.json");
    st.export_job_report(id, ExportFormat::Json, &json).unwrap();
    let v: serde_json::Value = serde_json::from_str(&fs::read_to_string(&json).unwrap()).unwrap();
    assert_eq!(v["filled"], 2);
}
```

Add `serde_json = { workspace = true }` back to `[dev-dependencies]` of `src-tauri/Cargo.toml` only if the compiler asks for it (it is a normal dependency since Task 4, so tests can use it).

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && cargo test -p coverly-roms --test drawer_actions`
Expected: FAIL to compile — the drawer and export methods do not exist.

- [ ] **Step 4: Drawer and export methods**

`src-tauri/src/library/art.rs`: `fn media_name` → `pub(crate) fn media_name`.

`src-tauri/src/state.rs` — imports:

```rust
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, BufWriter, Read, Write};
use std::sync::atomic::AtomicBool;

use coverly_core::art::{resolve_art, target_path};
use coverly_core::audit::{build_report, write_csv};
use coverly_core::fill::{remove_art, write_local_image, FillSettings};
use coverly_core::gather::Scope;
use coverly_core::jobs::{write_report_csv, ApplyOptions, ItemErrorKind, ItemStatus};
use coverly_core::plan::{Decision, Plan};
use coverly_core::profiles::Media;
use coverly_core::progress::NoProgress;
use coverly_core::write::OverwritePolicy;

use crate::jobs::view::{candidate_views, candidates_of, CandidateView, ExportFormat};
use crate::jobs::with_env;
use crate::library::art::{inside_root, media_name, MAX_ART_BYTES};
```

and extend the existing imports: `crate::jobs::view::{…}` (Task 6) with the names above instead of a second `use` line, `crate::library::refresh::{…}` with `subset_scan`; `Session`, `Arc`, `Path`, `PathBuf` and `SystemDb` are already imported. Merge `use std::…` lines with the existing ones (`fs` is new).

field `picks: Mutex<Option<EntryPick>>` (`Mutex::new(None)` in `new`), and:

```rust
/// The candidates last listed in the drawer, so a pick names one of them.
struct EntryPick {
    generation: u64,
    id: u32,
    media: Media,
    plan: Plan,
}

fn item_error(kind: ItemErrorKind, message: &str) -> IpcError {
    let m = message.to_string();
    match kind {
        ItemErrorKind::NoArt | ItemErrorKind::Network => IpcError::Network(m),
        ItemErrorKind::Decode => IpcError::BadImage(m),
        ItemErrorKind::WriteDenied => IpcError::WriteDenied(m),
        ItemErrorKind::DiskFull => IpcError::DiskFull,
        ItemErrorKind::NameInvalid => IpcError::NameInvalid(m),
        ItemErrorKind::OutsideRoot => IpcError::OutsideLibrary(m),
        ItemErrorKind::Io => IpcError::Io(m),
    }
}

fn absolute_root(session: &Session) -> Result<PathBuf, IpcError> {
    std::path::absolute(&session.root).map_err(|e| IpcError::Io(e.to_string()))
}

/// Where the profile stores `media` for entry `id`.
fn art_target(session: &Session, id: u32, media: Media) -> Result<PathBuf, IpcError> {
    let entry = &session.entries[id as usize];
    let sys = &session.systems[entry.system as usize];
    let db = SystemDb::builtin();
    let system = db.get(&entry.system_id).ok_or(IpcError::UnknownEntry(id))?;
    target_path(
        &absolute_root(session)?,
        &session.profile,
        &sys.dir,
        system,
        &entry.rom.rel_dir,
        &entry.rom.stem,
        media,
    )
    .ok_or_else(|| IpcError::BadRequest("this layout cannot store that media".into()))
}

/// A picked file, at most [`MAX_ART_BYTES`].
fn read_capped(file: &Path) -> Result<Vec<u8>, IpcError> {
    let shown = file.display().to_string();
    let meta = fs::metadata(file).map_err(|_| IpcError::Unreadable(shown.clone()))?;
    if !meta.is_file() {
        return Err(IpcError::BadImage(shown));
    }
    if meta.len() > MAX_ART_BYTES {
        return Err(IpcError::ArtTooLarge(meta.len()));
    }
    let mut bytes = Vec::new();
    File::open(file)
        .and_then(|f| f.take(MAX_ART_BYTES + 1).read_to_end(&mut bytes))
        .map_err(|e| IpcError::Io(e.to_string()))?;
    if bytes.len() as u64 > MAX_ART_BYTES {
        return Err(IpcError::ArtTooLarge(bytes.len() as u64));
    }
    Ok(bytes)
}

fn write_export(
    dest: &Path,
    f: impl FnOnce(&mut BufWriter<File>) -> io::Result<()>,
) -> Result<(), IpcError> {
    let mut w = BufWriter::new(File::create(dest).map_err(|e| IpcError::Io(e.to_string()))?);
    f(&mut w)
        .and_then(|()| w.flush())
        .map_err(|e| IpcError::Io(e.to_string()))
}
```

and in `impl AppState`:

```rust
    /// The open session, if entry `id` exists and no job is writing.
    fn writable_entry(&self, generation: u64, id: u32) -> Result<Arc<Session>, IpcError> {
        if self.jobs.busy() {
            return Err(IpcError::JobRunning);
        }
        let session = self.session(generation)?;
        if session.entries.get(id as usize).is_none() {
            return Err(IpcError::UnknownEntry(id));
        }
        Ok(session)
    }

    fn picked(&self, generation: u64, id: u32, media: Media) -> Result<Plan, IpcError> {
        let slot = self.picks.lock().unwrap_or_else(PoisonError::into_inner);
        match slot.as_ref() {
            Some(p) if p.generation == generation && p.id == id && p.media == media => {
                Ok(p.plan.clone())
            }
            _ => Err(IpcError::BadRequest("list the candidates first".into())),
        }
    }

    /// Every candidate for `media` of entry `id`, best first (the drawer's
    /// "Change…"). Existing art does not hide candidates.
    pub fn entry_candidates(
        &self,
        generation: u64,
        id: u32,
        media: Media,
    ) -> Result<Vec<CandidateView>, IpcError> {
        let session = self.writable_entry(generation, id)?;
        if session.profile.slot(media).is_none() {
            return Err(IpcError::BadRequest(
                "this layout cannot store that media".into(),
            ));
        }
        let plan = with_env(&self.fill, |env| {
            let db = SystemDb::builtin();
            let (scan, _) = subset_scan(&session, &[id]);
            let art = resolve_art(&scan, &session.profile, &db, &[media]);
            let settings = FillSettings {
                media: vec![media],
                scope: Scope::All,
                ..FillSettings::default()
            };
            Ok(env.plan(
                &absolute_root(&session)?,
                &session.profile,
                &db,
                &art,
                &settings,
                &NoProgress,
                &AtomicBool::new(false),
            ))
        })?;
        if let Some(why) = &plan.paused {
            return Err(IpcError::Network(why.clone()));
        }
        let candidates = plan.items.first().map(candidate_views).unwrap_or_default();
        *self.picks.lock().unwrap_or_else(PoisonError::into_inner) = Some(EntryPick {
            generation,
            id,
            media,
            plan,
        });
        Ok(candidates)
    }

    pub fn entry_candidate_art(
        &self,
        generation: u64,
        id: u32,
        media: Media,
        candidate: u32,
    ) -> Result<Vec<u8>, IpcError> {
        let session = self.session(generation)?;
        let plan = self.picked(generation, id, media)?;
        let c = plan
            .items
            .first()
            .and_then(|item| candidates_of(item).get(candidate as usize).map(|c| (*c).clone()))
            .ok_or_else(|| IpcError::BadRequest(format!("no candidate {candidate}")))?;
        with_env(&self.fill, |env| {
            Ok(env.candidate_image(&session.profile, &c)?.bytes)
        })
    }

    /// Writes candidate `candidate` of the listed ones (existing art backed up).
    pub fn set_entry_art(
        &self,
        generation: u64,
        id: u32,
        media: Media,
        candidate: u32,
    ) -> Result<Vec<EntryArtUpdate>, IpcError> {
        let session = self.writable_entry(generation, id)?;
        let mut plan = self.picked(generation, id, media)?;
        let known = plan.items.first().map_or(0, |item| candidates_of(item).len());
        if candidate as usize >= known {
            return Err(IpcError::BadRequest(format!("no candidate {candidate}")));
        }
        plan.decide(&BTreeMap::from([(
            0,
            Decision::Pick {
                candidate: candidate as usize,
            },
        )]));
        let opts = ApplyOptions {
            overwrite: OverwritePolicy::Backup,
            include_review: false,
            concurrency: 1,
            ..ApplyOptions::default()
        };
        let report = with_env(&self.fill, |env| {
            env.apply(&plan, &session.profile, &opts, &NoProgress, &AtomicBool::new(false))
                .map_err(IpcError::from)
        })?;
        if let Some(ItemStatus::Failed { kind, message }) = report.items.first().map(|i| &i.status) {
            return Err(item_error(*kind, message));
        }
        if let Some(why) = report.paused {
            return Err(IpcError::Network(why));
        }
        self.refresh_entries(generation, &[id], &SystemDb::builtin())
    }

    /// Writes the image file the user picked (decoded and re-encoded).
    pub fn set_entry_art_from_file(
        &self,
        generation: u64,
        id: u32,
        media: Media,
        file: &Path,
    ) -> Result<Vec<EntryArtUpdate>, IpcError> {
        let session = self.writable_entry(generation, id)?;
        let target = art_target(&session, id, media)?;
        let bytes = read_capped(file)?;
        write_local_image(
            &absolute_root(&session)?,
            &session.profile,
            media,
            &target,
            &bytes,
            &self.fill.data_dir,
        )?;
        self.refresh_entries(generation, &[id], &SystemDb::builtin())
    }

    /// Removes the art file of `media` (backed up, undoable).
    pub fn remove_entry_art(
        &self,
        generation: u64,
        id: u32,
        media: Media,
    ) -> Result<Vec<EntryArtUpdate>, IpcError> {
        let session = self.writable_entry(generation, id)?;
        let path = session.entries[id as usize]
            .art
            .get(&media)
            .cloned()
            .ok_or_else(|| IpcError::ArtMissing(media_name(media)))?;
        inside_root(&session, &path)?;
        remove_art(&absolute_root(&session)?, &path, &self.fill.data_dir)?;
        self.refresh_entries(generation, &[id], &SystemDb::builtin())
    }

    /// Writes the audit of the open library (read again from disk) to `dest`.
    pub fn export_audit(
        &self,
        generation: u64,
        format: ExportFormat,
        dest: &Path,
    ) -> Result<(), IpcError> {
        let session = self.session(generation)?;
        let media: Vec<Media> = Media::ALL
            .into_iter()
            .filter(|m| session.profile.slot(*m).is_some())
            .collect();
        let report = build_report(
            &session.root,
            &session.profile,
            &SystemDb::builtin(),
            &self.settings().scan_options(),
            &media,
        );
        write_export(dest, |w| match format {
            ExportFormat::Csv => write_csv(&report, w, false),
            ExportFormat::Json => serde_json::to_writer_pretty(w, &report).map_err(io::Error::other),
        })
    }

    /// Writes the item outcomes of job `job` to `dest`.
    pub fn export_job_report(
        &self,
        job: u64,
        format: ExportFormat,
        dest: &Path,
    ) -> Result<(), IpcError> {
        let report = self.jobs.report(job)?;
        write_export(dest, |w| match format {
            ExportFormat::Csv => write_report_csv(&report, w),
            ExportFormat::Json => serde_json::to_writer_pretty(w, &report).map_err(io::Error::other),
        })
    }
```

- [ ] **Step 5: Commands with native dialogs**

`src-tauri/src/ipc.rs` — imports `use tauri_plugin_dialog::DialogExt;`, `use crate::jobs::view::{CandidateView, ExportFormat};`, `use crate::library::refresh::EntryArtUpdate;`, then:

```rust
const IMAGE_FILTER: &str = "PNG / JPEG / WebP / GIF / BMP";
const IMAGE_EXTS: [&str; 6] = ["png", "jpg", "jpeg", "webp", "gif", "bmp"];

fn ext(format: ExportFormat) -> &'static str {
    match format {
        ExportFormat::Csv => "csv",
        ExportFormat::Json => "json",
    }
}

/// Asks where to save `default_name`; `None` when the user cancels.
fn save_path(app: &AppHandle, default_name: &str, format: ExportFormat) -> Result<Option<PathBuf>, IpcError> {
    let Some(file) = app
        .dialog()
        .file()
        .set_file_name(default_name)
        .add_filter(ext(format).to_uppercase(), &[ext(format)])
        .blocking_save_file()
    else {
        return Ok(None);
    };
    file.into_path()
        .map(Some)
        .map_err(|e| IpcError::Io(e.to_string()))
}

fn shown_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[tauri::command]
pub async fn entry_candidates(
    app: AppHandle,
    generation: u64,
    entry_id: u32,
    media: Media,
) -> Result<Vec<CandidateView>, IpcError> {
    blocking(move || app.state::<AppState>().entry_candidates(generation, entry_id, media)).await
}

#[tauri::command]
pub async fn entry_candidate_art(
    app: AppHandle,
    generation: u64,
    entry_id: u32,
    media: Media,
    candidate: u32,
) -> Result<tauri::ipc::Response, IpcError> {
    blocking(move || {
        app.state::<AppState>()
            .entry_candidate_art(generation, entry_id, media, candidate)
            .map(tauri::ipc::Response::new)
    })
    .await
}

#[tauri::command]
pub async fn set_entry_art(
    app: AppHandle,
    generation: u64,
    entry_id: u32,
    media: Media,
    candidate: u32,
) -> Result<Vec<EntryArtUpdate>, IpcError> {
    blocking(move || {
        app.state::<AppState>()
            .set_entry_art(generation, entry_id, media, candidate)
    })
    .await
}

/// Opens the native image picker; `None` when the user cancels.
#[tauri::command]
pub async fn set_entry_art_from_file(
    app: AppHandle,
    generation: u64,
    entry_id: u32,
    media: Media,
) -> Result<Option<Vec<EntryArtUpdate>>, IpcError> {
    blocking(move || {
        let Some(file) = app
            .dialog()
            .file()
            .add_filter(IMAGE_FILTER, &IMAGE_EXTS)
            .blocking_pick_file()
        else {
            return Ok(None);
        };
        let path = file.into_path().map_err(|e| IpcError::Io(e.to_string()))?;
        app.state::<AppState>()
            .set_entry_art_from_file(generation, entry_id, media, &path)
            .map(Some)
    })
    .await
}

#[tauri::command]
pub async fn remove_entry_art(
    app: AppHandle,
    generation: u64,
    entry_id: u32,
    media: Media,
) -> Result<Vec<EntryArtUpdate>, IpcError> {
    blocking(move || app.state::<AppState>().remove_entry_art(generation, entry_id, media)).await
}

/// Saves the audit through the native save dialog; returns the file name.
#[tauri::command]
pub async fn export_audit(
    app: AppHandle,
    generation: u64,
    format: ExportFormat,
) -> Result<Option<String>, IpcError> {
    blocking(move || {
        let name = format!("coverly-audit.{}", ext(format));
        let Some(path) = save_path(&app, &name, format)? else {
            return Ok(None);
        };
        app.state::<AppState>()
            .export_audit(generation, format, &path)?;
        Ok(Some(shown_name(&path)))
    })
    .await
}

#[tauri::command]
pub async fn export_job_report(
    app: AppHandle,
    job: u64,
    format: ExportFormat,
) -> Result<Option<String>, IpcError> {
    blocking(move || {
        let name = format!("coverly-job-{job}.{}", ext(format));
        let Some(path) = save_path(&app, &name, format)? else {
            return Ok(None);
        };
        app.state::<AppState>()
            .export_job_report(job, format, &path)?;
        Ok(Some(shown_name(&path)))
    })
    .await
}
```

(`use std::path::{Path, PathBuf};` replaces `use std::path::Path;`.) Register in `lib.rs` after `ipc::job_history,`:

```rust
            ipc::entry_candidates,
            ipc::entry_candidate_art,
            ipc::set_entry_art,
            ipc::set_entry_art_from_file,
            ipc::remove_entry_art,
            ipc::export_audit,
            ipc::export_job_report,
```

- [ ] **Step 6: Run the tests**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && cargo test --workspace`
Expected: PASS.

- [ ] **Step 7: Gates and commit**

Run all gates. Then commit in two steps:

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && git add crates/coverly-core && git commit -m "feat(core): export job reports as formula-safe CSV"
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && git add src-tauri && git commit -m "feat(app): change, set from file and remove art from the drawer; export reports"
```

---
### Task 8: Frontend IPC for jobs — wire types, Tauri adapter, error codes and the mock backend parity

**Files:**
- Modify: `src/ipc/types.ts`, `src/ipc/backend.ts`, `src/ipc/tauriBackend.ts`, `src/ipc/errors.ts`
- Modify: `src/ipc/contract.test.ts`, `src/ipc/tauriBackend.test.ts`
- Create: `src/lib/csv.ts`, `src/lib/csv.test.ts`
- Create: `src/ipc/mock/svg.ts`, `src/ipc/mock/mockJobs.ts`, `src/ipc/mock/mockJobs.test.ts`
- Modify: `src/ipc/mock/mockBackend.ts` (compose the jobs; `escapeXml` re-exported from `svg.ts`)
- Modify: `locales/en/ui.json`, `locales/es/ui.json` (`errors.*` for the new codes)

**Interfaces:**
- Consumes: the Rust commands and events of Tasks 6–7 and the shapes pinned in `src/ipc/contract.sample.json` / `contract.requests.json` (Task 4).
- Produces (TypeScript, used by Tasks 9–12):
  - types: `StrategyKind`, `STRATEGIES`, `OverwritePolicy`, `RegionPref`, `FillRequest`, `JobState`, `Phase`, `PauseKind`, `JobProgress`, `PauseView`, `PlanCounts`, `SystemCounts`, `PlanSummary`, `JobTotals`, `JobView`, `MatchKind`, `CandidateView`, `ActionKind`, `NoneKind`, `Decision`, `DecisionIn`, `ItemErrorKind`, `OutcomeKind`, `ItemOutcome`, `JobItemView`, `ItemFilter`, `JobProgressEvent`, `EntryArtUpdate`, `EntriesUpdated`, `UndoView`, `JobHistoryItem`, `ExportFormat`
  - `Backend` gains: `startFill(generation, request)`, `fillJob()`, `fillJobItems(job, filter)`, `decideFill(job, decisions)`, `runFill(job)`, `resumeFill(job)`, `cancelFill(job)`, `candidateArt(job, index, candidate): Promise<ArtImage>`, `undoLastJob(generation)`, `jobHistory()`, `onJobProgress(cb)`, `onJobChanged(cb)`, `onEntriesUpdated(cb)`, `entryCandidates(generation, entryId, media)`, `entryCandidateArt(generation, entryId, media, candidate): Promise<ArtImage>`, `setEntryArt(generation, entryId, media, candidate)`, `setEntryArtFromFile(generation, entryId, media): Promise<EntryArtUpdate[] | null>`, `removeEntryArt(generation, entryId, media)`, `exportAudit(generation, format): Promise<string | null>`, `exportJobReport(job, format): Promise<string | null>`
  - `src/lib/csv.ts`: `csvField(value: string): string`, `csvRow(fields: readonly (string | number)[]): string`, `csvText(rows: readonly (readonly (string | number)[])[]): string`
  - mock: `mockClass(id: number, media: Media): ActionKind`, `mockFailure(id: number): ItemErrorKind | null`, `mockTitle(name: string): string`, `MOCK_PAUSE_DETAIL`; `MockOptions` gains `pauseAfter?: number | null` (default 25: the first apply run of each job pauses once with a rate limit after that many items; `null` never) and `pick?: 'image' | 'cancel' | 'not-image'` (what "From file…" picks; default `'image'`); `MockBackend` gains `readonly exports: MockExport[]` (`{ name, text }`)

Mock rules (deterministic; undo follows Rust: refused while a job runs or is paused, only journals of the open library): an entry/media is `auto` when `(id + mediaIndex) % 10 < 7`, `review` when `7` or `8` (`8` = an unreleased-variant note), `none` when `9`; an `auto` item of an entry with `id % 13 === 0` fails on apply with `['no-art', 'network', 'decode', 'name-invalid'][id % 4]`; undecided review items are skipped; each write is journaled (`mock-0001`…) so undo restores the previous marks.

- [ ] **Step 1: Write the failing tests**

`src/lib/csv.test.ts`:

```ts
import { describe, expect, it } from 'vitest';

import { csvField, csvRow, csvText } from './csv';

describe('csv', () => {
  it('mock_csv_neutralises_formulas', () => {
    expect(csvField('=HYPERLINK("http://x")')).toBe(
      '"\'=HYPERLINK(""http://x"")"',
    );
    expect(csvField('+1')).toBe("'+1");
    expect(csvField('-cmd')).toBe("'-cmd");
    expect(csvField('@SUM(A1)')).toBe("'@SUM(A1)");
    expect(csvField('\tTab')).toBe("'\tTab");
    expect(csvField('Mega Man X')).toBe('Mega Man X');
  });

  it('quotes only when needed', () => {
    expect(csvField('Tom, Jerry')).toBe('"Tom, Jerry"');
    expect(csvField('two\nlines')).toBe('"two\nlines"');
    expect(csvField(' padded')).toBe('" padded"');
    expect(csvRow(['a', 1, '<img src=x>'])).toBe('a,1,<img src=x>');
    expect(csvText([['a', 'b'], [1, '=x']])).toBe("a,b\n1,'=x\n");
  });
});
```

Append to `src/ipc/contract.test.ts` (imports at the top, keys and checks inside the existing structure):

```ts
import requests from './contract.requests.json';
import type {
  CandidateView,
  DecisionIn,
  EntriesUpdated,
  FillRequest,
  JobHistoryItem,
  JobItemView,
  JobProgressEvent,
  JobView,
  UndoView,
} from './types';

const JOB_KEYS = [
  'id',
  'generation',
  'state',
  'progress',
  'paused',
  'plan',
  'decided',
  'journal',
  'result',
  'error',
] as const;
const ITEM_KEYS = [
  'index',
  'entry_id',
  'folder',
  'name',
  'media',
  'action',
  'confidence',
  'note',
  'none_reason',
  'candidates',
  'replaces_existing',
  'decision',
  'outcome',
] as const;
const CANDIDATE_KEYS = [
  'index',
  'source',
  'name',
  'regions',
  'kind',
  'score',
  'confidence',
] as const;
const JOB_PROGRESS_KEYS = ['job', 'phase', 'done', 'total', 'label'] as const;
const UPDATED_KEYS = ['generation', 'entries'] as const;
const UNDO_KEYS = ['job', 'removed', 'restored', 'kept', 'entries'] as const;
const HISTORY_KEYS = [
  'job',
  'root',
  'created_at',
  'entries',
  'complete',
  'undone',
] as const;
const REQUEST_KEYS = [
  'entry_ids',
  'media',
  'strategy',
  'regions',
  'overwrite',
] as const;
```

and add to the `checks` array:

```ts
  ['JobView', JOB_KEYS satisfies AllKeys<JobView, typeof JOB_KEYS>, sample.job_view],
  ['JobItemView', ITEM_KEYS satisfies AllKeys<JobItemView, typeof ITEM_KEYS>, sample.job_item],
  [
    'CandidateView',
    CANDIDATE_KEYS satisfies AllKeys<CandidateView, typeof CANDIDATE_KEYS>,
    sample.job_item.candidates[0]!,
  ],
  [
    'JobProgressEvent',
    JOB_PROGRESS_KEYS satisfies AllKeys<JobProgressEvent, typeof JOB_PROGRESS_KEYS>,
    sample.job_progress,
  ],
  [
    'EntriesUpdated',
    UPDATED_KEYS satisfies AllKeys<EntriesUpdated, typeof UPDATED_KEYS>,
    sample.entries_updated,
  ],
  ['UndoView', UNDO_KEYS satisfies AllKeys<UndoView, typeof UNDO_KEYS>, sample.undo],
  [
    'JobHistoryItem',
    HISTORY_KEYS satisfies AllKeys<JobHistoryItem, typeof HISTORY_KEYS>,
    sample.history_item,
  ],
  [
    'FillRequest',
    REQUEST_KEYS satisfies AllKeys<FillRequest, typeof REQUEST_KEYS>,
    requests.fill_request,
  ],
```

and a test:

```ts
  it('job payloads use the Rust spellings', () => {
    expect(sample.job_view.paused.kind).toBe('rate-limit');
    expect(sample.job_item.decision).toEqual({ choice: 'pick', candidate: 0 });
    expect(sample.job_item.outcome.error).toBe('write-denied');
    const decisions = requests.decisions as DecisionIn[];
    expect(decisions[1]).toEqual({ index: 1, choice: 'pick', candidate: 2 });
    expect(requests.item_filters).toEqual(['all', 'auto', 'review', 'failed']);
  });
```

Append to `src/ipc/tauriBackend.test.ts` (inside the `describe`; import `JOB_CHANGED_EVENT, ENTRIES_UPDATED_EVENT` from `./tauriBackend`):

```ts
  it('starts a fill with the request object and forwards job events', async () => {
    const calls: [string, unknown][] = [];
    mockIPC((cmd, args) => {
      calls.push([cmd, args]);
      return sample.job_view;
    });
    const b = createTauriBackend();
    const request = {
      entry_ids: null,
      media: ['cover'] as const,
      strategy: 'rom-region' as const,
      regions: ['rom', 'usa'] as const,
      overwrite: 'never' as const,
    };
    await b.startFill(3, {
      ...request,
      media: [...request.media],
      regions: [...request.regions],
    });
    await b.decideFill(4, [{ index: 0, choice: 'accept' }]);
    await b.fillJobItems(4, 'review');
    expect(calls).toEqual([
      ['start_fill', { generation: 3, request: { ...request, media: ['cover'], regions: ['rom', 'usa'] } }],
      ['decide_fill', { job: 4, decisions: [{ index: 0, choice: 'accept' }] }],
      ['fill_job_items', { job: 4, filter: 'review' }],
    ]);
    const seen: unknown[] = [];
    await b.onJobChanged((v) => seen.push(v));
    await b.onEntriesUpdated((u) => seen.push(u));
    events.handlers.get(JOB_CHANGED_EVENT)!({ payload: sample.job_view });
    events.handlers.get(ENTRIES_UPDATED_EVENT)!({ payload: sample.entries_updated });
    expect(seen).toEqual([sample.job_view, sample.entries_updated]);
  });

  it('sniffs candidate art and passes drawer arguments in camelCase', async () => {
    const calls: [string, unknown][] = [];
    mockIPC((cmd, args) => {
      calls.push([cmd, args]);
      return cmd.endsWith('_art') ? PNG.buffer : null;
    });
    const b = createTauriBackend();
    expect((await b.candidateArt(4, 2, 1)).mime).toBe('image/png');
    expect((await b.entryCandidateArt(3, 9, 'cover', 0)).mime).toBe('image/png');
    await expect(b.setEntryArtFromFile(3, 9, 'cover')).resolves.toBeNull();
    expect(calls).toEqual([
      ['candidate_art', { job: 4, index: 2, candidate: 1 }],
      ['entry_candidate_art', { generation: 3, entryId: 9, media: 'cover', candidate: 0 }],
      ['set_entry_art_from_file', { generation: 3, entryId: 9, media: 'cover' }],
    ]);
  });
```

`src/ipc/mock/mockJobs.test.ts`:

```ts
import { describe, expect, it } from 'vitest';

import type { FillRequest, JobState, JobView } from '../types';
import { FIXTURE_NOW } from './fixture';
import {
  createMockBackend,
  MOCK_LIBRARIES,
  type MockBackend,
  type MockOptions,
} from './mockBackend';
import { MOCK_PAUSE_DETAIL, mockClass, mockFailure } from './mockJobs';

const SMALL = MOCK_LIBRARIES[1].path;
const REQUEST: FillRequest = {
  entry_ids: null,
  media: ['cover'],
  strategy: 'rom-region',
  regions: ['rom', 'world', 'usa', 'europe', 'japan'],
  overwrite: 'never',
};

async function opened(options: MockOptions = {}) {
  const b = createMockBackend({
    delayMs: 0,
    now: FIXTURE_NOW,
    pauseAfter: null,
    ...options,
  });
  const view = await b.openLibrary(SMALL);
  return { b, view };
}

/** The next job view in `state` (register before the action). */
function next(b: MockBackend, state: JobState): Promise<JobView> {
  return new Promise((resolve) => {
    void b.onJobChanged((v) => {
      if (v.state === state) resolve(v);
    });
  });
}

async function planned(b: MockBackend, generation: number, request = REQUEST) {
  const done = next(b, 'planned');
  const job = await b.startFill(generation, request);
  expect(job.state).toBe('gathering');
  return done;
}

describe('mock fill jobs', () => {
  it('plans every missing cover with deterministic auto, review and none', async () => {
    const { b, view } = await opened();
    const progress: number[] = [];
    await b.onJobProgress((p) => progress.push(p.done));
    const v = await planned(b, view.generation);
    const missing = view.entries.filter((e) => e.art.cover === 'missing');
    const count = (c: string) =>
      missing.filter((e) => mockClass(e.id, 'cover') === c).length;
    expect(v.plan?.totals).toEqual({
      auto: count('auto'),
      review: count('review'),
      none: count('none'),
    });
    expect(progress[0]).toBe(0);
    const items = await b.fillJobItems(v.id, 'all');
    expect(items).toHaveLength(missing.length);
  });

  it('plans only the given entries', async () => {
    const { b, view } = await opened();
    const ids = view.entries
      .filter((e) => e.art.cover === 'missing')
      .slice(0, 3)
      .map((e) => e.id);
    const v = await planned(b, view.generation, {
      ...REQUEST,
      entry_ids: [...ids, ids[0]!, 99_999],
    });
    const items = await b.fillJobItems(v.id, 'all');
    expect(items.map((i) => i.entry_id)).toEqual(ids);
  });

  it('writes auto and decided review items, lists failures and updates the library', async () => {
    const { b, view } = await opened();
    const updated: number[] = [];
    await b.onEntriesUpdated((u) => updated.push(...u.entries.map((e) => e.id)));
    const v = await planned(b, view.generation);
    const review = await b.fillJobItems(v.id, 'review');
    expect(review.length).toBeGreaterThan(1);
    const picked = review[0]!;
    await b.decideFill(v.id, [
      { index: picked.index, choice: 'pick', candidate: 1 },
      { index: review[1]!.index, choice: 'skip' },
    ]);
    const done = next(b, 'done');
    await b.runFill(v.id);
    const result = (await done).result!;
    const autos = await b.fillJobItems(v.id, 'auto');
    const failing = autos.filter((i) => mockFailure(i.entry_id!) !== null).length;
    expect(result.filled).toBe(autos.length - failing + 1);
    expect(result.no_art + result.failed).toBe(failing);
    expect(await b.fillJobItems(v.id, 'failed')).toHaveLength(failing);
    expect(updated).toContain(picked.entry_id);
    const art = await b.readArt(view.generation, picked.entry_id!, 'cover');
    expect(art.mime).toBe('image/svg+xml');
    const skipped = (await b.fillJobItems(v.id, 'review')).find(
      (i) => i.index === review[1]!.index,
    );
    expect(skipped?.outcome?.status).toBe('skipped-no-match');
  });

  it('pauses once with a rate limit and resumes into the same journal', async () => {
    const { b, view } = await opened({ pauseAfter: 3 });
    const v = await planned(b, view.generation);
    const paused = next(b, 'paused');
    await b.runFill(v.id);
    const p = await paused;
    expect(p.paused).toEqual({
      phase: 'apply',
      kind: 'rate-limit',
      detail: MOCK_PAUSE_DETAIL,
    });
    expect(p.journal).not.toBeNull();
    await expect(b.undoLastJob(view.generation)).rejects.toMatchObject({
      code: 'job-running',
    });
    const done = next(b, 'done');
    await b.resumeFill(v.id);
    expect((await done).journal).toBe(p.journal);
    expect(await b.jobHistory()).toHaveLength(1);
  });

  it('undo restores the marks and history shows it undone', async () => {
    const { b, view } = await opened();
    const v = await planned(b, view.generation);
    const done = next(b, 'done');
    await b.runFill(v.id);
    const result = (await done).result!;
    const undo = await b.undoLastJob(view.generation);
    expect(undo.removed).toBe(result.filled);
    expect(undo.entries.every((e) => e.art.cover === 'missing')).toBe(true);
    expect((await b.jobHistory())[0]?.undone).toBe(true);
    await expect(b.undoLastJob(view.generation)).rejects.toMatchObject({
      code: 'nothing-to-undo',
    });
  });

  it('refuses a second job while one runs, stale libraries and bad requests', async () => {
    const { b, view } = await opened();
    const done = next(b, 'planned');
    await b.startFill(view.generation, REQUEST);
    await expect(b.startFill(view.generation, REQUEST)).rejects.toMatchObject({
      code: 'job-running',
    });
    await expect(b.undoLastJob(view.generation)).rejects.toMatchObject({
      code: 'job-running',
    });
    const v = await done;
    await expect(
      b.startFill(view.generation + 1, REQUEST),
    ).rejects.toMatchObject({ code: 'stale-library' });
    await expect(
      b.startFill(view.generation, { ...REQUEST, regions: ['mars' as 'usa'] }),
    ).rejects.toMatchObject({ code: 'bad-request' });
    await expect(b.resumeFill(v.id)).rejects.toMatchObject({
      code: 'bad-request',
    });
    await expect(b.runFill(v.id + 1)).rejects.toMatchObject({
      code: 'stale-job',
    });
  });

  it('changes, sets from file and removes art from the drawer', async () => {
    const { b, view } = await opened();
    const e = view.entries.find((x) => x.art.cover === 'missing')!;
    await expect(
      b.setEntryArt(view.generation, e.id, 'cover', 0),
    ).rejects.toMatchObject({ code: 'bad-request' });
    const cands = await b.entryCandidates(view.generation, e.id, 'cover');
    expect(cands).toHaveLength(3);
    const art = await b.entryCandidateArt(view.generation, e.id, 'cover', 1);
    expect(art.mime).toBe('image/svg+xml');
    const [set] = await b.setEntryArt(view.generation, e.id, 'cover', 1);
    expect(set?.art.cover).toBe('present');
    const [removed] = await b.removeEntryArt(view.generation, e.id, 'cover');
    expect(removed?.art.cover).toBe('missing');
    await expect(
      b.removeEntryArt(view.generation, e.id, 'cover'),
    ).rejects.toMatchObject({ code: 'art-missing' });
    const undo = await b.undoLastJob(view.generation);
    expect(undo.restored).toBe(1);
    expect(undo.entries[0]?.art.cover).toBe('present');
    expect(
      await b.setEntryArtFromFile(view.generation, e.id, 'title'),
    ).not.toBeNull();
    const cancel = await opened({ pick: 'cancel' });
    await expect(
      cancel.b.setEntryArtFromFile(cancel.view.generation, e.id, 'cover'),
    ).resolves.toBeNull();
    const bad = await opened({ pick: 'not-image' });
    await expect(
      bad.b.setEntryArtFromFile(bad.view.generation, e.id, 'cover'),
    ).rejects.toMatchObject({ code: 'bad-image' });
  });

  it('exports the audit and the job report as formula-safe text', async () => {
    const { b, view } = await opened();
    expect(await b.exportAudit(view.generation, 'csv')).toBe(
      'coverly-audit.csv',
    );
    const audit = b.exports[0]!.text;
    expect(audit.split('\n')[0]).toBe(
      'system,folder,rel_path,stem,size,modified,cover,screenshot,title',
    );
    expect(audit.split('\n')).toHaveLength(view.entries.length + 2);
    const v = await planned(b, view.generation);
    await expect(b.exportJobReport(v.id, 'csv')).rejects.toMatchObject({
      code: 'bad-request',
    });
    const done = next(b, 'done');
    await b.runFill(v.id);
    await done;
    expect(await b.exportJobReport(v.id, 'json')).toBe(
      `coverly-job-${v.id}.json`,
    );
    expect(JSON.parse(b.exports[1]!.text)).toHaveProperty('items');
  });
});
```

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && pnpm test -- src/lib/csv.test.ts src/ipc`
Expected: FAIL — `csv.ts`, `mockJobs.ts` and the new types/methods do not exist.

- [ ] **Step 2: Wire types**

Append to `src/ipc/types.ts`:

```ts
/* Fill jobs — mirror of src-tauri/src/jobs/view.rs. */

export type StrategyKind = 'priority' | 'rom-region' | 'popularity';
export const STRATEGIES: readonly StrategyKind[] = [
  'rom-region',
  'priority',
  'popularity',
];
export type OverwritePolicy = 'never' | 'backup';
/** `rom` = the ROM's own regions. */
export type RegionPref = 'rom' | Region;

export interface FillRequest {
  /** Selection or current filter; null = the whole library. */
  entry_ids: number[] | null;
  media: Media[];
  strategy: StrategyKind;
  regions: RegionPref[];
  overwrite: OverwritePolicy;
}

export type JobState =
  | 'gathering'
  | 'planned'
  | 'applying'
  | 'paused'
  | 'cancelled'
  | 'done'
  | 'failed';
export type Phase = 'gather' | 'apply';
export type PauseKind = 'rate-limit' | 'offline' | 'volume';

export interface JobProgress {
  phase: Phase;
  done: number;
  total: number;
  label: string;
}

export interface PauseView {
  phase: Phase;
  kind: PauseKind;
  detail: string;
}

export interface PlanCounts {
  auto: number;
  review: number;
  none: number;
}

export interface SystemCounts extends PlanCounts {
  folder: string;
}

export interface PlanSummary {
  totals: PlanCounts;
  per_system: SystemCounts[];
  warnings: string[];
}

export interface JobTotals {
  filled: number;
  replaced: number;
  skipped: number;
  no_art: number;
  failed: number;
}

export interface JobView {
  id: number;
  generation: number;
  state: JobState;
  progress: JobProgress | null;
  paused: PauseView | null;
  plan: PlanSummary | null;
  decided: number;
  journal: string | null;
  result: JobTotals | null;
  error: string | null;
}

export type MatchKind = 'fuzzy' | 'title-exact' | 'name-exact' | 'canonical';

export interface CandidateView {
  /** 0 = the chosen image; n = alternative n - 1. */
  index: number;
  source: string;
  name: string;
  regions: Region[];
  kind: MatchKind;
  score: number;
  confidence: number;
}

export type ActionKind = 'auto' | 'review' | 'none';
export type NoneKind =
  | 'no-candidates'
  | 'below-threshold'
  | 'source-error'
  | 'no-target'
  | 'skipped';
export type Decision =
  | { choice: 'accept' }
  | { choice: 'skip' }
  | { choice: 'pick'; candidate: number };
export type DecisionIn = Decision & { index: number };
export type ItemErrorKind =
  | 'no-art'
  | 'network'
  | 'decode'
  | 'write-denied'
  | 'disk-full'
  | 'name-invalid'
  | 'outside-root'
  | 'io';
export type OutcomeKind =
  | 'filled'
  | 'replaced'
  | 'skipped-exists'
  | 'skipped-review'
  | 'skipped-no-match'
  | 'not-attempted'
  | 'failed';

export interface ItemOutcome {
  status: OutcomeKind;
  error: ItemErrorKind | null;
  message: string | null;
}

export interface JobItemView {
  index: number;
  entry_id: number | null;
  folder: string;
  name: string;
  media: Media;
  action: ActionKind;
  confidence: number | null;
  note: 'unreleased-variant' | null;
  none_reason: NoneKind | null;
  candidates: CandidateView[];
  replaces_existing: boolean;
  decision: Decision | null;
  outcome: ItemOutcome | null;
}

export type ItemFilter = 'all' | 'auto' | 'review' | 'failed';

export interface JobProgressEvent {
  job: number;
  phase: Phase;
  done: number;
  total: number;
  label: string;
}

export interface EntryArtUpdate {
  id: number;
  art: ArtMarks;
}

export interface EntriesUpdated {
  generation: number;
  entries: EntryArtUpdate[];
}

export interface UndoView {
  job: string;
  removed: number;
  restored: number;
  kept: number;
  entries: EntryArtUpdate[];
}

export interface JobHistoryItem {
  job: string;
  root: string;
  created_at: string;
  entries: number;
  complete: boolean;
  undone: boolean;
}

export type ExportFormat = 'csv' | 'json';
```

- [ ] **Step 3: Backend interface, Tauri adapter and error codes**

`src/ipc/backend.ts` — import the new types and extend `Backend`:

```ts
  startFill(generation: number, request: FillRequest): Promise<JobView>;
  fillJob(): Promise<JobView | null>;
  fillJobItems(job: number, filter: ItemFilter): Promise<JobItemView[]>;
  decideFill(job: number, decisions: DecisionIn[]): Promise<JobView>;
  runFill(job: number): Promise<JobView>;
  resumeFill(job: number): Promise<JobView>;
  cancelFill(job: number): Promise<JobView>;
  candidateArt(job: number, index: number, candidate: number): Promise<ArtImage>;
  undoLastJob(generation: number): Promise<UndoView>;
  jobHistory(): Promise<JobHistoryItem[]>;
  onJobProgress(cb: (e: JobProgressEvent) => void): Promise<() => void>;
  onJobChanged(cb: (v: JobView) => void): Promise<() => void>;
  onEntriesUpdated(cb: (e: EntriesUpdated) => void): Promise<() => void>;
  entryCandidates(generation: number, entryId: number, media: Media): Promise<CandidateView[]>;
  entryCandidateArt(
    generation: number,
    entryId: number,
    media: Media,
    candidate: number,
  ): Promise<ArtImage>;
  setEntryArt(
    generation: number,
    entryId: number,
    media: Media,
    candidate: number,
  ): Promise<EntryArtUpdate[]>;
  /** Opens a native image picker; null when the user cancels. */
  setEntryArtFromFile(
    generation: number,
    entryId: number,
    media: Media,
  ): Promise<EntryArtUpdate[] | null>;
  removeEntryArt(generation: number, entryId: number, media: Media): Promise<EntryArtUpdate[]>;
  /** Saves through a native dialog; the file name, or null when cancelled. */
  exportAudit(generation: number, format: ExportFormat): Promise<string | null>;
  exportJobReport(job: number, format: ExportFormat): Promise<string | null>;
```

`src/ipc/tauriBackend.ts`:

```ts
export const JOB_PROGRESS_EVENT = 'fill-job-progress';
export const JOB_CHANGED_EVENT = 'fill-job-changed';
export const ENTRIES_UPDATED_EVENT = 'library-entries-updated';

/** A command returning image bytes; only PNG/JPEG/WebP/GIF become images. */
async function image(
  cmd: string,
  args: Record<string, unknown>,
): Promise<ArtImage> {
  const buffer = await call<ArrayBuffer>(cmd, args);
  const bytes = new Uint8Array(buffer);
  const mime = sniffImageMime(bytes);
  if (!mime) throw new BackendError('art-unreadable');
  return { bytes, mime };
}
```

`readArt` becomes `readArt: (generation, entryId, media) => image('read_art', { generation, entryId, media }),` and the object gains:

```ts
    startFill: (generation, request) =>
      call<JobView>('start_fill', { generation, request }),
    fillJob: () => call<JobView | null>('fill_job'),
    fillJobItems: (job, filter) =>
      call<JobItemView[]>('fill_job_items', { job, filter }),
    decideFill: (job, decisions) =>
      call<JobView>('decide_fill', { job, decisions }),
    runFill: (job) => call<JobView>('run_fill', { job }),
    resumeFill: (job) => call<JobView>('resume_fill', { job }),
    cancelFill: (job) => call<JobView>('cancel_fill', { job }),
    candidateArt: (job, index, candidate) =>
      image('candidate_art', { job, index, candidate }),
    undoLastJob: (generation) => call<UndoView>('undo_last_job', { generation }),
    jobHistory: () => call<JobHistoryItem[]>('job_history'),
    onJobProgress: (cb) =>
      listen<JobProgressEvent>(JOB_PROGRESS_EVENT, (e) => cb(e.payload)),
    onJobChanged: (cb) => listen<JobView>(JOB_CHANGED_EVENT, (e) => cb(e.payload)),
    onEntriesUpdated: (cb) =>
      listen<EntriesUpdated>(ENTRIES_UPDATED_EVENT, (e) => cb(e.payload)),
    entryCandidates: (generation, entryId, media) =>
      call<CandidateView[]>('entry_candidates', { generation, entryId, media }),
    entryCandidateArt: (generation, entryId, media, candidate) =>
      image('entry_candidate_art', { generation, entryId, media, candidate }),
    setEntryArt: (generation, entryId, media, candidate) =>
      call<EntryArtUpdate[]>('set_entry_art', {
        generation,
        entryId,
        media,
        candidate,
      }),
    setEntryArtFromFile: (generation, entryId, media) =>
      call<EntryArtUpdate[] | null>('set_entry_art_from_file', {
        generation,
        entryId,
        media,
      }),
    removeEntryArt: (generation, entryId, media) =>
      call<EntryArtUpdate[]>('remove_entry_art', { generation, entryId, media }),
    exportAudit: (generation, format) =>
      call<string | null>('export_audit', { generation, format }),
    exportJobReport: (job, format) =>
      call<string | null>('export_job_report', { job, format }),
```

`src/ipc/errors.ts` — `ERROR_CODES` gains, before `'art-unreadable'`: `'job-running'`, `'no-job'`, `'stale-job'`, `'bad-request'`, `'nothing-to-undo'`, `'network'`, `'bad-image'`, `'write-denied'`, `'disk-full'`, `'name-invalid'`, `'volume-gone'` (added in the Task 7 review: a drawer change whose SD is pulled mid-write).

`locales/en/ui.json` → `errors` gains:

```json
    "job-running": "A fill job is running. Wait for it or cancel it first.",
    "no-job": "There is no fill job.",
    "stale-job": "That fill job was replaced by a newer one.",
    "bad-request": "This cannot be done: {detail}",
    "nothing-to-undo": "There is nothing to undo.",
    "network": "The art source could not be reached: {detail}",
    "bad-image": "This file is not a usable image: {detail}",
    "write-denied": "Coverly ROMs cannot write to the library (read-only or no permission): {detail}",
    "disk-full": "The library disk is full.",
    "name-invalid": "This file name is not valid on SD cards: {detail}",
    "volume-gone": "The library volume was removed or is no longer reachable: {detail}"
```

`locales/es/ui.json` → `errors` gains:

```json
    "job-running": "Hay un trabajo de relleno en curso. Espera a que termine o cancélalo.",
    "no-job": "No hay ningún trabajo de relleno.",
    "stale-job": "Ese trabajo de relleno se ha sustituido por otro más reciente.",
    "bad-request": "No se puede hacer: {detail}",
    "nothing-to-undo": "No hay nada que deshacer.",
    "network": "No se ha podido contactar con la fuente de arte: {detail}",
    "bad-image": "Este archivo no es una imagen válida: {detail}",
    "write-denied": "Coverly ROMs no puede escribir en la biblioteca (solo lectura o sin permiso): {detail}",
    "disk-full": "El disco de la biblioteca está lleno.",
    "name-invalid": "Este nombre de archivo no es válido en tarjetas SD: {detail}",
    "volume-gone": "El volumen de la biblioteca se ha retirado o ya no está accesible: {detail}"
```

- [ ] **Step 4: CSV helper**

`src/lib/csv.ts`:

```ts
/**
 * CSV text like the core's writer: RFC 4180 quoting, `\n` line ends, and
 * fields a spreadsheet would run as a formula prefixed with `'` (ROM names
 * are hostile; OWASP CSV injection guidance).
 */
const FORMULA = /^[=+\-@\t\r]/;

export function csvField(value: string): string {
  const v = FORMULA.test(value) ? `'${value}` : value;
  return /[",\n\r]/.test(v) || v.startsWith(' ') || v.endsWith(' ')
    ? `"${v.replace(/"/g, '""')}"`
    : v;
}

export function csvRow(fields: readonly (string | number)[]): string {
  return fields.map((f) => csvField(String(f))).join(',');
}

export function csvText(
  rows: readonly (readonly (string | number)[])[],
): string {
  return rows.map(csvRow).join('\n') + '\n';
}
```

- [ ] **Step 5: Mock jobs**

`src/ipc/mock/svg.ts`:

```ts
const XML: Record<string, string> = {
  '&': '&amp;',
  '<': '&lt;',
  '>': '&gt;',
  '"': '&quot;',
  "'": '&apos;',
};

export function escapeXml(s: string): string {
  return s.replace(/[&<>"']/g, (c) => XML[c] ?? c);
}

/** A labelled placeholder image (the label is escaped). */
export function labelSvg(label: string, wide: boolean, hue: number): string {
  const [w, h] = wide ? [320, 240] : [250, 360];
  const text = escapeXml(label.length > 40 ? `${label.slice(0, 39)}…` : label);
  return (
    `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}" viewBox="0 0 ${w} ${h}">` +
    `<rect width="100%" height="100%" fill="hsl(${hue} 40% 35%)"/>` +
    `<text x="50%" y="50%" fill="#fff" font-family="system-ui" font-size="15" text-anchor="middle">${text}</text>` +
    `</svg>`
  );
}
```

In `mockBackend.ts` delete the local `XML` table and `escapeXml`, and add `import { escapeXml } from './svg';` plus `export { escapeXml } from './svg';` (tests import it from `./mockBackend`).

`src/ipc/mock/mockJobs.ts`:

```ts
/**
 * Fake fill jobs for the browser mock (`pnpm dev`, tests): deterministic
 * plans, review items, failures and one rate-limit pause per job, drawer
 * writes and exports, so the whole fill flow runs end to end in Chrome.
 */
import { csvText } from '@/lib/csv';

import type { Backend } from '../backend';
import { BackendError } from '../errors';
import {
  MEDIA,
  REGIONS,
  type ActionKind,
  type ArtImage,
  type ArtMark,
  type CandidateView,
  type Decision,
  type EntriesUpdated,
  type EntryArtUpdate,
  type EntryView,
  type ItemErrorKind,
  type JobItemView,
  type JobProgressEvent,
  type JobTotals,
  type JobView,
  type LibraryView,
  type Media,
  type Phase,
  type PlanSummary,
  type Region,
} from '../types';
import { labelSvg } from './svg';

export const MOCK_PAUSE_DETAIL =
  'rate limited by thumbnails.libretro.com (HTTP 429)';
const FAIL_KINDS: readonly ItemErrorKind[] = [
  'no-art',
  'network',
  'decode',
  'name-invalid',
];

/** auto (7 in 10), review (2 in 10) or none, per entry and media. */
export function mockClass(id: number, media: Media): ActionKind {
  const k = (id + MEDIA.indexOf(media)) % 10;
  return k < 7 ? 'auto' : k < 9 ? 'review' : 'none';
}

/** How an auto item of entry `id` fails when applied, if it does. */
export function mockFailure(id: number): ItemErrorKind | null {
  return id % 13 === 0 ? (FAIL_KINDS[id % FAIL_KINDS.length] ?? 'io') : null;
}

/** The name without `(…)` and `[…]` tags. */
export function mockTitle(name: string): string {
  return name.replace(/\s*[([][^)\]]*[)\]]/g, '').trim() || name;
}

function cand(
  index: number,
  name: string,
  regions: Region[],
  kind: CandidateView['kind'],
  score: number,
): CandidateView {
  const confidence =
    kind === 'canonical' || kind === 'name-exact'
      ? 1
      : kind === 'title-exact'
        ? 0.95
        : Math.min(score, 94) / 100;
  return { index, source: 'libretro', name, regions, kind, score, confidence };
}

function candidatesFor(e: EntryView, media: Media) {
  const t = mockTitle(e.name);
  const action = mockClass(e.id, media);
  const variant = (e.id + MEDIA.indexOf(media)) % 10 === 8;
  const candidates: CandidateView[] =
    action === 'auto'
      ? [
          cand(0, `${t} (USA)`, ['usa'], 'canonical', 100),
          cand(1, `${t} (Europe)`, ['europe'], 'title-exact', 95),
        ]
      : action === 'none'
        ? []
        : variant
          ? [
              cand(0, `${t} (USA) (Beta)`, ['usa'], 'title-exact', 95),
              cand(1, `${t} (Japan)`, ['japan'], 'fuzzy', 72),
            ]
          : [
              cand(0, `${t} (Europe)`, ['europe'], 'fuzzy', 82),
              cand(1, `${t} (Japan)`, ['japan'], 'fuzzy', 78),
              cand(2, `${t} (USA) (Beta)`, ['usa'], 'title-exact', 95),
            ];
  return {
    action,
    candidates,
    note: action === 'review' && variant ? ('unreleased-variant' as const) : null,
  };
}

function iso(secs: number): string {
  return new Date(secs * 1000).toISOString().replace(/\.\d{3}Z$/, 'Z');
}

function svgImage(label: string, media: Media, hue: number): ArtImage {
  return {
    bytes: new TextEncoder().encode(labelSvg(label, media !== 'cover', hue)),
    mime: 'image/svg+xml',
  };
}

export interface MockExport {
  name: string;
  text: string;
}

export interface MockJobsHost {
  /** The open library, with the latest art marks. */
  current(): LibraryView | null;
  /** Replaces the art marks of some entries of the open library. */
  patch(updates: readonly EntryArtUpdate[]): void;
  /** One fake unit of work (a macrotask, so callers see `gathering`). */
  tick(): Promise<void>;
  now: number;
}

export interface MockJobsOptions {
  pauseAfter: number | null;
  pick: 'image' | 'cancel' | 'not-image';
}

type JobsApi = Pick<
  Backend,
  | 'startFill'
  | 'fillJob'
  | 'fillJobItems'
  | 'decideFill'
  | 'runFill'
  | 'resumeFill'
  | 'cancelFill'
  | 'candidateArt'
  | 'undoLastJob'
  | 'jobHistory'
  | 'onJobProgress'
  | 'onJobChanged'
  | 'onEntriesUpdated'
  | 'entryCandidates'
  | 'entryCandidateArt'
  | 'setEntryArt'
  | 'setEntryArtFromFile'
  | 'removeEntryArt'
  | 'exportAudit'
  | 'exportJobReport'
> & { readonly exports: MockExport[] };

interface Journal {
  job: string;
  createdAt: number;
  root: string;
  undone: boolean;
  complete: boolean;
  changes: { id: number; media: Media; before: ArtMark }[];
}

interface Job {
  view: JobView;
  items: JobItemView[];
  root: string;
  cancel: boolean;
  pausedOnce: boolean;
  journal: Journal | null;
}

export function createMockJobs(
  host: MockJobsHost,
  opts: MockJobsOptions,
): JobsApi {
  const progress = new Set<(e: JobProgressEvent) => void>();
  const changed = new Set<(v: JobView) => void>();
  const updated = new Set<(e: EntriesUpdated) => void>();
  const journals: Journal[] = [];
  const exports: MockExport[] = [];
  let seq = 0;
  let job: Job | null = null;
  let picks: {
    generation: number;
    id: number;
    media: Media;
    candidates: CandidateView[];
  } | null = null;

  const running = () =>
    job !== null &&
    (job.view.state === 'gathering' || job.view.state === 'applying');
  const snapshot = (j: Job): JobView => ({
    ...j.view,
    decided: j.items.filter((i) => i.decision !== null).length,
  });
  const emitChanged = (j: Job) => {
    const v = snapshot(j);
    changed.forEach((cb) => cb(v));
  };
  const emitProgress = (
    j: Job,
    phase: Phase,
    done: number,
    total: number,
    label: string,
  ) => {
    j.view.progress = { phase, done, total, label };
    progress.forEach((cb) => cb({ job: j.view.id, phase, done, total, label }));
  };

  function library(generation: number): LibraryView {
    const view = host.current();
    if (!view) throw new BackendError('no-library');
    if (view.generation !== generation) throw new BackendError('stale-library');
    return view;
  }
  function entry(generation: number, id: number): EntryView {
    const e = library(generation).entries[id];
    if (!e) throw new BackendError('unknown-entry', String(id));
    return e;
  }
  function fresh(id: number): EntryView | undefined {
    return host.current()?.entries[id];
  }
  function current(id: number): Job {
    if (!job) throw new BackendError('no-job');
    if (job.view.id !== id) throw new BackendError('stale-job');
    return job;
  }
  function notRunning() {
    if (running()) throw new BackendError('job-running');
  }
  function newJournal(root: string): Journal {
    const n = journals.length + 1;
    const j: Journal = {
      job: `mock-${String(n).padStart(4, '0')}`,
      createdAt: host.now + n * 60,
      root,
      undone: false,
      complete: false,
      changes: [],
    };
    journals.push(j);
    return j;
  }
  /** Sets one mark, journaled; returns the entry's new marks. */
  function write(
    journal: Journal,
    id: number,
    media: Media,
    mark: ArtMark,
  ): EntryArtUpdate | null {
    const e = fresh(id);
    if (!e) return null;
    journal.changes.push({ id, media, before: e.art[media] });
    const update = { id, art: { ...e.art, [media]: mark } };
    host.patch([update]);
    return update;
  }
  /** One update per entry (the last one wins). */
  function merged(updates: (EntryArtUpdate | null)[]): EntryArtUpdate[] {
    const byId = new Map<number, EntryArtUpdate>();
    for (const u of updates) if (u) byId.set(u.id, u);
    return [...byId.values()];
  }
  function summary(items: JobItemView[]): PlanSummary {
    const totals = { auto: 0, review: 0, none: 0 };
    const bySystem = new Map<string, { auto: number; review: number; none: number }>();
    for (const i of items) {
      totals[i.action] += 1;
      const s = bySystem.get(i.folder) ?? { auto: 0, review: 0, none: 0 };
      s[i.action] += 1;
      bySystem.set(i.folder, s);
    }
    const per_system = [...bySystem.entries()]
      .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
      .map(([folder, c]) => ({ folder, ...c }));
    return { totals, per_system, warnings: [] };
  }
  function totals(items: JobItemView[]): JobTotals {
    const t = { filled: 0, replaced: 0, skipped: 0, no_art: 0, failed: 0 };
    for (const i of items) {
      const o = i.outcome;
      if (o?.status === 'filled') t.filled += 1;
      else if (o?.status === 'replaced') t.replaced += 1;
      else if (o?.status === 'failed' && o.error === 'no-art') t.no_art += 1;
      else if (o?.status === 'failed') t.failed += 1;
      else t.skipped += 1;
    }
    return t;
  }
  const wanted = (i: JobItemView) =>
    i.decision?.choice !== 'skip' &&
    (i.action === 'auto' || (i.action === 'review' && i.decision !== null));

  async function gather(j: Job) {
    const total = j.items.length;
    const step = Math.max(25, Math.ceil(total / 40));
    emitProgress(j, 'gather', 0, total, '');
    for (let n = 0; n < total && !j.cancel; n += step) {
      emitProgress(j, 'gather', n, total, j.items[n]?.name ?? '');
      await host.tick();
    }
    await host.tick();
    j.view.progress = null;
    j.view.plan = summary(j.items);
    j.view.state = j.cancel ? 'cancelled' : 'planned';
    emitChanged(j);
  }

  async function apply(j: Job) {
    const todo = j.items.filter(
      (i) => i.outcome === null || i.outcome.status === 'not-attempted',
    );
    for (const i of todo) {
      if (!wanted(i)) {
        i.outcome = {
          status:
            i.action === 'review' && i.decision === null
              ? 'skipped-review'
              : 'skipped-no-match',
          error: null,
          message: null,
        };
      }
    }
    const work = todo.filter(wanted);
    if (work.length > 0) j.journal ??= newJournal(j.root);
    const step = Math.max(10, Math.ceil(work.length / 40));
    const updates: (EntryArtUpdate | null)[] = [];
    let ran = 0;
    for (const [n, i] of work.entries()) {
      if (j.cancel) break;
      if (opts.pauseAfter !== null && !j.pausedOnce && ran === opts.pauseAfter) {
        j.pausedOnce = true;
        j.view.paused = {
          phase: 'apply',
          kind: 'rate-limit',
          detail: MOCK_PAUSE_DETAIL,
        };
        break;
      }
      if (n % step === 0) {
        emitProgress(j, 'apply', n, work.length, i.name);
        await host.tick();
      }
      ran += 1;
      const fail =
        i.action === 'auto' && i.decision === null && i.entry_id !== null
          ? mockFailure(i.entry_id)
          : null;
      if (fail) {
        i.outcome = { status: 'failed', error: fail, message: `mock ${fail}: ${i.name}` };
        continue;
      }
      if (i.entry_id === null || !j.journal) continue;
      updates.push(write(j.journal, i.entry_id, i.media, 'present'));
      i.outcome = {
        status: i.replaces_existing ? 'replaced' : 'filled',
        error: null,
        message: null,
      };
    }
    for (const i of work) {
      i.outcome ??= { status: 'not-attempted', error: null, message: null };
    }
    await host.tick();
    if (j.journal && !j.view.paused) j.journal.complete = true;
    j.view.journal = j.journal?.job ?? null;
    j.view.result = totals(j.items);
    j.view.progress = null;
    j.view.state = j.cancel ? 'cancelled' : j.view.paused ? 'paused' : 'done';
    const entries = merged(updates);
    if (entries.length > 0) {
      const ev = { generation: j.view.generation, entries };
      updated.forEach((cb) => cb(ev));
    }
    emitChanged(j);
  }

  function picked(generation: number, id: number, media: Media): CandidateView[] {
    if (
      !picks ||
      picks.generation !== generation ||
      picks.id !== id ||
      picks.media !== media
    ) {
      throw new BackendError('bad-request', 'list the candidates first');
    }
    return picks.candidates;
  }

  function record(name: string, text: string, mime: string): string {
    exports.push({ name, text });
    if (typeof document !== 'undefined' && !/jsdom/i.test(navigator.userAgent)) {
      const url = URL.createObjectURL(new Blob([text], { type: mime }));
      const a = document.createElement('a');
      a.href = url;
      a.download = name;
      a.click();
      setTimeout(() => URL.revokeObjectURL(url), 1000);
    }
    return name;
  }

  const listen = <T>(set: Set<(v: T) => void>, cb: (v: T) => void) => {
    set.add(cb);
    return Promise.resolve(() => {
      set.delete(cb);
    });
  };

  return {
    exports,
    async startFill(generation, request) {
      const view = library(generation);
      notRunning();
      const media = MEDIA.filter(
        (m) => request.media.includes(m) && view.profile.media.includes(m),
      );
      if (media.length === 0) {
        throw new BackendError('bad-request', 'this layout cannot store the chosen art');
      }
      const bad = request.regions.find(
        (r) => r !== 'rom' && !REGIONS.includes(r),
      );
      if (bad !== undefined) {
        throw new BackendError('bad-request', `unknown region \`${String(bad)}\``);
      }
      const ids = [
        ...new Set(request.entry_ids ?? view.entries.map((e) => e.id)),
      ].sort((a, b) => a - b);
      const items: JobItemView[] = [];
      for (const id of ids) {
        const e = view.entries[id];
        if (!e) continue;
        for (const m of media) {
          const mark = e.art[m];
          const replace = request.overwrite === 'backup' && mark === 'present';
          if (mark !== 'missing' && !replace) continue;
          const c = candidatesFor(e, m);
          items.push({
            index: items.length,
            entry_id: e.id,
            folder: view.systems[e.system]?.folder ?? '',
            name: e.name,
            media: m,
            action: c.action,
            confidence: c.candidates[0]?.confidence ?? null,
            note: c.note,
            none_reason: c.action === 'none' ? 'no-candidates' : null,
            candidates: c.candidates,
            replaces_existing: mark === 'present',
            decision: null,
            outcome: null,
          });
        }
      }
      seq += 1;
      const j: Job = {
        view: {
          id: seq,
          generation,
          state: 'gathering',
          progress: null,
          paused: null,
          plan: null,
          decided: 0,
          journal: null,
          result: null,
          error: null,
        },
        items,
        root: view.root,
        cancel: false,
        pausedOnce: false,
        journal: null,
      };
      job = j;
      void gather(j);
      return snapshot(j);
    },
    fillJob: () => Promise.resolve(job ? snapshot(job) : null),
    async fillJobItems(id, filter) {
      const j = current(id);
      return j.items
        .filter((i) =>
          filter === 'all'
            ? true
            : filter === 'failed'
              ? i.outcome?.status === 'failed'
              : i.action === filter,
        )
        .map((i) => ({ ...i }));
    },
    async decideFill(id, decisions) {
      const j = current(id);
      if (j.view.state !== 'planned') {
        throw new BackendError('bad-request', 'decisions are taken before the job runs');
      }
      const bad = decisions.find((d) => !j.items[d.index]);
      if (bad) throw new BackendError('bad-request', `no item ${bad.index}`);
      for (const d of decisions) {
        const { index, ...decision } = d;
        j.items[index]!.decision = decision as Decision;
      }
      return snapshot(j);
    },
    async runFill(id) {
      const j = current(id);
      if (j.view.state !== 'planned') {
        throw new BackendError('bad-request', 'the job is not ready to run');
      }
      j.view.state = 'applying';
      void apply(j);
      return snapshot(j);
    },
    async resumeFill(id) {
      const j = current(id);
      if (j.view.state !== 'paused') {
        throw new BackendError('bad-request', 'the job is not paused');
      }
      j.view.paused = null;
      j.view.state = 'applying';
      void apply(j);
      return snapshot(j);
    },
    async cancelFill(id) {
      const j = current(id);
      j.cancel = true;
      if (j.view.state === 'planned' || j.view.state === 'paused') {
        j.view.state = 'cancelled';
        j.view.paused = null;
        emitChanged(j);
      }
      return snapshot(j);
    },
    async candidateArt(id, index, candidate) {
      const c = current(id).items[index]?.candidates[candidate];
      if (!c) throw new BackendError('bad-request', `no candidate ${candidate}`);
      await host.tick();
      return svgImage(c.name, current(id).items[index]!.media, (index * 53 + candidate * 97) % 360);
    },
    async undoLastJob(generation) {
      notRunning();
      // Like Rust: a paused job resumes into its journal; only this library's journals.
      if (job?.view.state === 'paused') throw new BackendError('job-running');
      const view = library(generation);
      const journal = [...journals]
        .reverse()
        .find((j) => !j.undone && j.root === view.root);
      if (!journal) throw new BackendError('nothing-to-undo');
      journal.undone = true;
      const updates: (EntryArtUpdate | null)[] = [];
      let removed = 0;
      let restored = 0;
      for (const c of [...journal.changes].reverse()) {
        if (c.before === 'present') restored += 1;
        else removed += 1;
        const e = fresh(c.id);
        if (e) {
          const update = { id: c.id, art: { ...e.art, [c.media]: c.before } };
          host.patch([update]);
          updates.push(update);
        }
      }
      return { job: journal.job, removed, restored, kept: 0, entries: merged(updates) };
    },
    jobHistory: () =>
      Promise.resolve(
        [...journals].reverse().map((j) => ({
          job: j.job,
          root: j.root,
          created_at: iso(j.createdAt),
          entries: j.changes.length,
          complete: j.complete,
          undone: j.undone,
        })),
      ),
    onJobProgress: (cb) => listen(progress, cb),
    onJobChanged: (cb) => listen(changed, cb),
    onEntriesUpdated: (cb) => listen(updated, cb),
    async entryCandidates(generation, id, media) {
      notRunning();
      const e = entry(generation, id);
      if (!library(generation).profile.media.includes(media)) {
        throw new BackendError('bad-request', 'this layout cannot store that media');
      }
      const t = mockTitle(e.name);
      const candidates = [
        cand(0, `${t} (USA)`, ['usa'], 'canonical', 100),
        cand(1, `${t} (Europe)`, ['europe'], 'title-exact', 95),
        cand(2, `${t} (Japan)`, ['japan'], 'fuzzy', 80),
      ];
      picks = { generation, id, media, candidates };
      await host.tick();
      return candidates.map((c) => ({ ...c }));
    },
    async entryCandidateArt(generation, id, media, candidate) {
      entry(generation, id);
      const c = picked(generation, id, media)[candidate];
      if (!c) throw new BackendError('bad-request', `no candidate ${candidate}`);
      return svgImage(c.name, media, (id * 31 + candidate * 97) % 360);
    },
    async setEntryArt(generation, id, media, candidate) {
      notRunning();
      const view = library(generation);
      entry(generation, id);
      if (!picked(generation, id, media)[candidate]) {
        throw new BackendError('bad-request', `no candidate ${candidate}`);
      }
      const journal = newJournal(view.root);
      journal.complete = true;
      return merged([write(journal, id, media, 'present')]);
    },
    async setEntryArtFromFile(generation, id, media) {
      notRunning();
      const view = library(generation);
      entry(generation, id);
      if (opts.pick === 'cancel') return null;
      if (opts.pick === 'not-image') {
        throw new BackendError('bad-image', 'not an image');
      }
      const journal = newJournal(view.root);
      journal.complete = true;
      return merged([write(journal, id, media, 'present')]);
    },
    async removeEntryArt(generation, id, media) {
      notRunning();
      const view = library(generation);
      if (entry(generation, id).art[media] !== 'present') {
        throw new BackendError('art-missing', media);
      }
      const journal = newJournal(view.root);
      journal.complete = true;
      return merged([write(journal, id, media, 'missing')]);
    },
    async exportAudit(generation, format) {
      const view = library(generation);
      const media = view.profile.media;
      if (format === 'csv') {
        const rows = [
          ['system', 'folder', 'rel_path', 'stem', 'size', 'modified', ...media],
          ...view.entries.map((e) => {
            const s = view.systems[e.system];
            return [
              s?.system_id ?? '',
              s?.folder ?? '',
              e.rel_path,
              e.name,
              e.size,
              iso(e.mtime),
              ...media.map((m) => e.art[m]),
            ];
          }),
        ];
        return record('coverly-audit.csv', csvText(rows), 'text/csv');
      }
      const json = {
        root: view.root,
        profile_id: view.profile.id,
        media,
        entries: view.entries.map((e) => ({ rel_path: e.rel_path, art: e.art })),
      };
      return record('coverly-audit.json', JSON.stringify(json, null, 2), 'application/json');
    },
    async exportJobReport(id, format) {
      const j = current(id);
      if (!j.view.result) throw new BackendError('bad-request', 'the job has not run yet');
      const name = `coverly-job-${id}.${format}`;
      if (format === 'json') {
        return record(name, JSON.stringify({ ...j.view.result, items: j.items }, null, 2), 'application/json');
      }
      const rows = [
        ['folder', 'rel_path', 'media', 'status', 'source', 'name', 'error', 'message'],
        ...j.items.map((i) => {
          const wrote = i.outcome?.status === 'filled' || i.outcome?.status === 'replaced';
          return [
            i.folder,
            i.name,
            i.media,
            i.outcome?.status ?? 'not-attempted',
            wrote ? 'libretro' : '',
            wrote ? (i.candidates[0]?.name ?? '') : '',
            i.outcome?.error ?? '',
            i.outcome?.message ?? '',
          ];
        }),
      ];
      return record(name, csvText(rows), 'text/csv');
    },
  };
}
```

In `src/ipc/mock/mockBackend.ts`:
- `MockOptions` gains:

```ts
  /** The first apply of each job pauses (rate limit) after this many items; null = never (default 25). */
  pauseAfter?: number | null;
  /** What "From file…" picks (default: an image). */
  pick?: 'image' | 'cancel' | 'not-image';
```

- `MockBackend` gains `readonly exports: MockExport[];` (import `createMockJobs, type MockExport` from `./mockJobs`).
- inside `createMockBackend`, after `let current: LibraryView | null = null;`:

```ts
  const jobs = createMockJobs(
    {
      current: () => current,
      patch(updates) {
        if (!current) return;
        const byId = new Map(updates.map((u) => [u.id, u.art]));
        current = {
          ...current,
          entries: current.entries.map((e) => {
            const art = byId.get(e.id);
            return art ? { ...e, art } : e;
          }),
        };
      },
      tick: () => new Promise<void>((r) => setTimeout(r, delayMs)),
      now,
    },
    {
      pauseAfter: opts.pauseAfter === undefined ? 25 : opts.pauseAfter,
      pick: opts.pick ?? 'image',
    },
  );
```

- the returned object starts with `...jobs,` (before `kind: 'mock'`).

- [ ] **Step 6: Run the tests**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && pnpm test && pnpm typecheck`
Expected: PASS (existing mock tests still pass; `errors.test.ts` checks every new code has both translations).

- [ ] **Step 7: Gates and commit**

Run all gates. Then:

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && git add src locales && git commit -m "feat(ipc): add fill job, drawer and export calls with a deterministic mock"
```

---
### Task 9: Fill wizard — scope, media, strategy + region order, overwrite policy, dry-run summary, Run (M4-01)

**Files:**
- Create: `src/stores/jobStore.ts`, `src/lib/fill.ts`, `src/lib/fill.test.ts`, `src/lib/focusTrap.ts`
- Create: `src/components/Modal.tsx`, `src/hooks/useImageUrl.ts`, `src/components/fill/CandidateImage.tsx`, `src/components/fill/FillWizard.tsx`, `src/components/fill/FillWizard.test.tsx`
- Create: `src/stores/libraryStore.updates.test.ts`, `src/lib/artCache.drop.test.ts`
- Modify: `src/stores/libraryStore.ts` (`applyArtUpdates`), `src/lib/artCache.ts` (`dropArtUrls`), `src/stores/viewStore.ts` (`overlay`), `src/components/LibrarySettingsDialog.tsx` (uses `focusTrap`), `src/components/library/Toolbar.tsx` (Fill button live), `src/screens/LibraryScreen.tsx` (job events, overlay), `src/screens/LibraryScreen.test.tsx`, `src/styles/globals.css`, `locales/{en,es}/ui.json`

**Interfaces:**
- Consumes: Task 8 `Backend` job methods and types; `MockOptions.pauseAfter`; `mockClass`, `mockFailure`; `generateLibrary`, `FIXTURE_NOW`.
- Produces:
  - `useJobStore` (zustand): state `{ job: JobView | null; progress: JobProgressEvent | null; error: BackendError | null; history: JobHistoryItem[]; lastUndo: UndoView | null; pending: boolean }`; actions `connect(): Promise<() => void>`, `start(generation, request): Promise<boolean>`, `decide(decisions): Promise<boolean>`, `run(): Promise<boolean>`, `resume(): Promise<boolean>`, `cancel(): Promise<boolean>`, `undoLast(generation): Promise<boolean>`, `loadHistory(): Promise<void>`, `exportReport(format): Promise<string | null>`, `clearError()`, `reset()`; helper `isRunning(job: JobView | null): boolean`
  - `useLibraryStore.getState().applyArtUpdates(generation: number, updates: readonly EntryArtUpdate[]): void` (ignores another generation)
  - `dropArtUrls(generation: number, ids: readonly number[]): void`
  - `useViewStore`: `overlay: Overlay` (`'none' | 'wizard' | 'review' | 'jobs' | 'reports'`), `setOverlay(o)`, `closeOverlay()`
  - `src/lib/fill.ts`: `type FillScope = 'selection' | 'filter' | 'library'`, `DEFAULT_REGION_ORDER`, `scopeIds(scope, selectedId, visible): number[] | null`, `moveItem<T>(list, i, dir): T[]`, `buildFillRequest(o: FillChoices): FillRequest`
  - `<Modal labelledBy onClose className? children />`, `trapTab(ev, root)` in `src/lib/focusTrap.ts`
  - `type ImageSource = { kind: 'job'; job; index; candidate } | { kind: 'entry'; generation; entryId; media; candidate }`, `useImageUrl(source): { url: string | null; failed: boolean }`, `<CandidateImage source label className? />`
  - `<FillWizard view visible selectedId onClose />`

- [ ] **Step 1: Write the failing tests**

`src/lib/fill.test.ts`:

```ts
import { describe, expect, it } from 'vitest';

import { generateLibrary, FIXTURE_NOW } from '@/ipc/mock/fixture';

import { buildFillRequest, DEFAULT_REGION_ORDER, moveItem, scopeIds } from './fill';

const view = generateLibrary({
  seed: 3,
  count: 12,
  root: '/mock/x',
  generation: 1,
  now: FIXTURE_NOW,
});

describe('fill choices', () => {
  it('turns a scope into entry ids', () => {
    const visible = view.entries.slice(0, 4);
    expect(scopeIds('selection', 7, visible)).toEqual([7]);
    expect(scopeIds('selection', null, visible)).toEqual([]);
    expect(scopeIds('filter', null, visible)).toEqual([0, 1, 2, 3]);
    expect(scopeIds('library', 7, visible)).toBeNull();
  });

  it('moves a region up and down within bounds', () => {
    expect(moveItem(DEFAULT_REGION_ORDER, 2, -1)).toEqual([
      'rom',
      'usa',
      'world',
      'europe',
      'japan',
    ]);
    expect(moveItem(DEFAULT_REGION_ORDER, 0, -1)).toEqual([
      ...DEFAULT_REGION_ORDER,
    ]);
    expect(moveItem(DEFAULT_REGION_ORDER, 4, 1)).toEqual([
      ...DEFAULT_REGION_ORDER,
    ]);
  });

  it('builds the request the backend expects', () => {
    expect(
      buildFillRequest({
        scope: 'library',
        selectedId: null,
        visible: view.entries,
        media: ['cover'],
        strategy: 'priority',
        regions: ['usa', 'rom'],
        overwrite: 'backup',
      }),
    ).toEqual({
      entry_ids: null,
      media: ['cover'],
      strategy: 'priority',
      regions: ['usa', 'rom'],
      overwrite: 'backup',
    });
  });
});
```

`src/stores/libraryStore.updates.test.ts`:

```ts
import { afterEach, describe, expect, it } from 'vitest';

import { setBackend } from '@/ipc/backend';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import { createMockBackend, MOCK_LIBRARIES } from '@/ipc/mock/mockBackend';

import { resetLibraryStore, useLibraryStore } from './libraryStore';

describe('library art updates', () => {
  afterEach(() => setBackend(null));

  it('ignores_entry_updates_for_another_generation', async () => {
    setBackend(createMockBackend({ delayMs: 0, now: FIXTURE_NOW }));
    resetLibraryStore();
    await useLibraryStore.getState().open(MOCK_LIBRARIES[1].path);
    const view = useLibraryStore.getState().view!;
    const e = view.entries.find((x) => x.art.cover === 'missing')!;
    const art = { ...e.art, cover: 'present' as const };
    useLibraryStore
      .getState()
      .applyArtUpdates(view.generation + 1, [{ id: e.id, art }]);
    expect(useLibraryStore.getState().view).toBe(view);
    useLibraryStore.getState().applyArtUpdates(view.generation, [{ id: e.id, art }]);
    const next = useLibraryStore.getState().view!;
    expect(next.entries[e.id]!.art.cover).toBe('present');
    const other = view.entries.find((x) => x.id !== e.id)!;
    expect(next.entries[other.id]).toBe(other);
  });
});
```

`src/lib/artCache.drop.test.ts`:

```ts
import { afterEach, describe, expect, it } from 'vitest';

import { setBackend } from '@/ipc/backend';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import { createMockBackend, MOCK_LIBRARIES } from '@/ipc/mock/mockBackend';

import { artKey, clearArtCache, dropArtUrls, loadArtUrl, peekArtUrl } from './artCache';

describe('dropping art urls', () => {
  afterEach(() => {
    clearArtCache();
    setBackend(null);
  });

  it('forgets the images of updated entries only', async () => {
    const b = createMockBackend({ delayMs: 0, now: FIXTURE_NOW });
    setBackend(b);
    const view = await b.openLibrary(MOCK_LIBRARIES[1].path);
    const [a, c] = view.entries.filter((e) => e.art.cover === 'present');
    await loadArtUrl(view.generation, a!.id, 'cover');
    await loadArtUrl(view.generation, c!.id, 'cover');
    dropArtUrls(view.generation, [a!.id]);
    expect(peekArtUrl(artKey(view.generation, a!.id, 'cover'))).toBeNull();
    expect(peekArtUrl(artKey(view.generation, c!.id, 'cover'))).not.toBeNull();
  });
});
```

`src/components/fill/FillWizard.test.tsx`:

```tsx
import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import App from '@/App';
import { I18nProvider } from '@/i18n/I18nProvider';
import { setBackend } from '@/ipc/backend';
import { FIXTURE_NOW, generateLibrary } from '@/ipc/mock/fixture';
import {
  createMockBackend,
  MOCK_LIBRARIES,
  type MockOptions,
} from '@/ipc/mock/mockBackend';
import { mockClass, mockFailure } from '@/ipc/mock/mockJobs';
import { useFilterStore } from '@/stores/filterStore';
import { useJobStore } from '@/stores/jobStore';
import { resetLibraryStore, useLibraryStore } from '@/stores/libraryStore';
import { stubLayout } from '@/test/layout';

import { FillWizard } from './FillWizard';

async function setup(opts: MockOptions = {}, locale: 'en' | 'es' = 'en') {
  const backend = createMockBackend({
    delayMs: 0,
    now: FIXTURE_NOW,
    pauseAfter: null,
    ...opts,
  });
  setBackend(backend);
  resetLibraryStore();
  useJobStore.getState().reset();
  await useLibraryStore.getState().open(MOCK_LIBRARIES[1].path);
  render(
    <I18nProvider locale={locale}>
      <App />
    </I18nProvider>,
  );
  return { backend, view: useLibraryStore.getState().view! };
}

async function toPreview(dialog: HTMLElement, scope: RegExp) {
  await userEvent.click(within(dialog).getByRole('radio', { name: scope }));
  for (let i = 0; i < 3; i += 1) {
    await userEvent.click(within(dialog).getByRole('button', { name: 'Next' }));
  }
  await userEvent.click(
    within(dialog).getByRole('button', { name: 'Preview (dry run)' }),
  );
}

describe('Fill wizard', () => {
  let restore: () => void;
  beforeEach(() => {
    restore = stubLayout(1280, 800);
  });
  afterEach(() => {
    restore();
    setBackend(null);
  });

  it('walks the wizard to a dry-run summary and runs it', async () => {
    const { view } = await setup();
    await userEvent.click(
      screen.getByRole('button', { name: 'Fill missing art' }),
    );
    const dialog = screen.getByRole('dialog', { name: 'Fill missing art' });
    await userEvent.click(
      within(dialog).getByRole('radio', { name: /Whole library/ }),
    );
    await userEvent.click(within(dialog).getByRole('button', { name: 'Next' }));
    expect(within(dialog).getByRole('checkbox', { name: 'Cover' })).toBeChecked();
    await userEvent.click(within(dialog).getByRole('button', { name: 'Next' }));
    expect(
      within(dialog).getByRole('radio', { name: /ROM region/ }),
    ).toBeChecked();
    await userEvent.click(
      within(dialog).getByRole('button', { name: 'Move USA up' }),
    );
    await userEvent.click(within(dialog).getByRole('button', { name: 'Next' }));
    expect(
      within(dialog).getByRole('radio', { name: 'Only missing art' }),
    ).toBeChecked();
    await userEvent.click(
      within(dialog).getByRole('button', { name: 'Preview (dry run)' }),
    );
    const table = await within(dialog).findByRole('table', { name: 'Dry run' });
    const missing = view.entries.filter((e) => e.art.cover === 'missing');
    const auto = missing.filter((e) => mockClass(e.id, 'cover') === 'auto');
    expect(
      within(table).getByRole('row', { name: new RegExp(`^Total ${auto.length} `) }),
    ).toBeInTheDocument();
    await userEvent.click(
      within(dialog).getByRole('button', { name: `Run (${auto.length})` }),
    );
    await waitFor(() =>
      expect(useJobStore.getState().job?.state).toBe('done'),
    );
    const written = auto.filter((e) => mockFailure(e.id) === null).length;
    expect(
      await screen.findByText(`${missing.length - written} without cover`),
    ).toBeInTheDocument();
  });

  it('sends the visible entries for the current filter', async () => {
    const { backend, view } = await setup();
    const spy = vi.spyOn(backend, 'startFill');
    act(() => useFilterStore.getState().setArt('missing-cover'));
    await userEvent.click(
      screen.getByRole('button', { name: 'Fill missing art' }),
    );
    const dialog = screen.getByRole('dialog', { name: 'Fill missing art' });
    await toPreview(dialog, /Current filter/);
    await waitFor(() => expect(spy).toHaveBeenCalled());
    const ids = [...(spy.mock.calls[0]![1].entry_ids ?? [])].sort((a, b) => a - b);
    expect(ids).toEqual(
      view.entries.filter((e) => e.art.cover === 'missing').map((e) => e.id),
    );
  });

  it('run_button_is_disabled_while_running', async () => {
    await setup({ delayMs: 40 });
    await userEvent.click(
      screen.getByRole('button', { name: 'Fill missing art' }),
    );
    const dialog = screen.getByRole('dialog', { name: 'Fill missing art' });
    await toPreview(dialog, /Whole library/);
    expect(within(dialog).getByRole('status')).toHaveTextContent(
      'Looking for art',
    );
    expect(
      within(dialog).queryByRole('button', { name: /^Run/ }),
    ).not.toBeInTheDocument();
    const run = await within(dialog).findByRole(
      'button',
      { name: /^Run/ },
      { timeout: 5000 },
    );
    expect(run).toBeEnabled();
  });

  it('warns that Batocera gamelists are not updated', () => {
    const view = generateLibrary({
      seed: 3,
      count: 12,
      root: '/mock/bato',
      generation: 1,
      now: FIXTURE_NOW,
    });
    render(
      <I18nProvider locale="en">
        <FillWizard
          view={{ ...view, profile: { ...view.profile, id: 'batocera' } }}
          visible={view.entries}
          selectedId={null}
          onClose={() => undefined}
        />
      </I18nProvider>,
    );
    expect(screen.getByText(/gamelist\.xml/)).toBeInTheDocument();
  });

  it('speaks Spanish', async () => {
    await setup({}, 'es');
    await userEvent.click(
      screen.getByRole('button', { name: 'Rellenar arte que falta' }),
    );
    const dialog = screen.getByRole('dialog', {
      name: 'Rellenar arte que falta',
    });
    expect(
      within(dialog).getByRole('button', { name: 'Siguiente' }),
    ).toBeInTheDocument();
    expect(within(dialog).getByText('Qué rellenar')).toBeInTheDocument();
  });
});
```

In `src/screens/LibraryScreen.test.tsx` replace the test `keeps Fill missing art disabled until M4` with:

```tsx
  it('enables Fill missing art', async () => {
    await openLibrary();
    expect(
      screen.getByRole('button', { name: 'Fill missing art' }),
    ).toBeEnabled();
  });
```

and in `speaks Spanish` change the last `.toBeDisabled()` to `.toBeEnabled()`.

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && pnpm test -- src/lib/fill.test.ts src/stores src/lib/artCache.drop.test.ts src/components/fill src/screens/LibraryScreen.test.tsx`
Expected: FAIL — modules and the wizard do not exist; the Fill button is disabled.

- [ ] **Step 2: Stores, cache and helpers**

`src/lib/artCache.ts` — add (and `import { MEDIA, type Media } from '@/ipc/types';`):

```ts
/** Forgets (and revokes) the images of `ids`: their art was rewritten. */
export function dropArtUrls(generation: number, ids: readonly number[]): void {
  for (const id of ids) {
    for (const media of MEDIA) {
      const key = artKey(generation, id, media);
      const url = urls.get(key);
      if (url !== undefined) {
        URL.revokeObjectURL(url);
        urls.delete(key);
      }
      inflight.delete(key);
    }
  }
}
```

`src/stores/libraryStore.ts` — `LibraryState` gains `applyArtUpdates(generation: number, updates: readonly EntryArtUpdate[]): void;` (import `EntryArtUpdate`, `dropArtUrls`) and the store gains:

```ts
  applyArtUpdates(generation, updates) {
    const view = get().view;
    if (!view || view.generation !== generation || updates.length === 0) return;
    const byId = new Map(updates.map((u) => [u.id, u.art]));
    const entries = view.entries.map((e) => {
      const art = byId.get(e.id);
      return art ? { ...e, art } : e;
    });
    dropArtUrls(generation, [...byId.keys()]);
    set({ view: { ...view, entries } });
  },
```

`src/stores/viewStore.ts`:

```ts
export type Overlay = 'none' | 'wizard' | 'review' | 'jobs' | 'reports';
```

`ViewState` gains `overlay: Overlay; setOverlay(overlay: Overlay): void; closeOverlay(): void;`; the store gains `overlay: 'none', setOverlay: (overlay) => set({ overlay }), closeOverlay: () => set({ overlay: 'none' }),` and `reset` also sets `overlay: 'none'`.

`src/lib/fill.ts`:

```ts
import type {
  EntryView,
  FillRequest,
  Media,
  OverwritePolicy,
  RegionPref,
  StrategyKind,
} from '@/ipc/types';

/** What the wizard fills: the selected ROM, the visible ones, or all. */
export type FillScope = 'selection' | 'filter' | 'library';

/** Spec §9: the ROM's own region, then World, USA, Europe, Japan. */
export const DEFAULT_REGION_ORDER: readonly RegionPref[] = [
  'rom',
  'world',
  'usa',
  'europe',
  'japan',
];

/** Entry ids of a scope; `null` = the whole library. */
export function scopeIds(
  scope: FillScope,
  selectedId: number | null,
  visible: readonly EntryView[],
): number[] | null {
  switch (scope) {
    case 'selection':
      return selectedId === null ? [] : [selectedId];
    case 'filter':
      return visible.map((e) => e.id);
    case 'library':
      return null;
  }
}

/** `list` with item `i` moved one place up (-1) or down (+1). */
export function moveItem<T>(list: readonly T[], i: number, dir: -1 | 1): T[] {
  const out = [...list];
  const j = i + dir;
  const a = out[i];
  const b = out[j];
  if (a === undefined || b === undefined) return out;
  out[i] = b;
  out[j] = a;
  return out;
}

export interface FillChoices {
  scope: FillScope;
  selectedId: number | null;
  visible: readonly EntryView[];
  media: readonly Media[];
  strategy: StrategyKind;
  regions: readonly RegionPref[];
  overwrite: OverwritePolicy;
}

export function buildFillRequest(o: FillChoices): FillRequest {
  return {
    entry_ids: scopeIds(o.scope, o.selectedId, o.visible),
    media: [...o.media],
    strategy: o.strategy,
    regions: [...o.regions],
    overwrite: o.overwrite,
  };
}
```

`src/stores/jobStore.ts`:

```ts
/**
 * The fill job as the UI follows it. The backend owns the job (one at a
 * time); this store mirrors it from events and command results, and keeps
 * the job history and the last undo.
 */
import { create } from 'zustand';

import { getBackend, type Backend } from '@/ipc/backend';
import { BackendError, toBackendError } from '@/ipc/errors';
import type {
  DecisionIn,
  ExportFormat,
  FillRequest,
  JobHistoryItem,
  JobProgressEvent,
  JobView,
  UndoView,
} from '@/ipc/types';

import { useLibraryStore } from './libraryStore';

export function isRunning(job: JobView | null): boolean {
  return job?.state === 'gathering' || job?.state === 'applying';
}

interface JobStore {
  job: JobView | null;
  progress: JobProgressEvent | null;
  error: BackendError | null;
  history: JobHistoryItem[];
  lastUndo: UndoView | null;
  /** A command is in flight. */
  pending: boolean;
  /** Subscribes to job events; returns the unsubscribe. */
  connect(): Promise<() => void>;
  start(generation: number, request: FillRequest): Promise<boolean>;
  decide(decisions: DecisionIn[]): Promise<boolean>;
  run(): Promise<boolean>;
  resume(): Promise<boolean>;
  cancel(): Promise<boolean>;
  undoLast(generation: number): Promise<boolean>;
  loadHistory(): Promise<void>;
  exportReport(format: ExportFormat): Promise<string | null>;
  clearError(): void;
  reset(): void;
}

const initial = {
  job: null,
  progress: null,
  error: null,
  history: [],
  lastUndo: null,
  pending: false,
};

/** Bumped by every `changed` event: a command result older than an event is dropped. */
let events = 0;

export const useJobStore = create<JobStore>()((set, get) => {
  function accept(view: JobView) {
    const cur = get().job;
    if (cur && view.id < cur.id) return;
    set({ job: view, progress: isRunning(view) ? get().progress : null });
  }
  function onChanged(view: JobView) {
    events += 1;
    accept(view);
  }
  function jobId(job: JobView | null): number {
    if (!job) throw new BackendError('no-job');
    return job.id;
  }
  async function act(
    f: (b: Backend, job: JobView | null) => Promise<JobView>,
  ): Promise<boolean> {
    const seen = events;
    set({ pending: true, error: null });
    try {
      const b = await getBackend();
      const view = await f(b, get().job);
      if (events === seen || get().job?.id !== view.id) accept(view);
      return true;
    } catch (e) {
      set({ error: toBackendError(e) });
      return false;
    } finally {
      set({ pending: false });
    }
  }
  return {
    ...initial,
    async connect() {
      const b = await getBackend();
      const offs = await Promise.all([
        b.onJobProgress((p) => {
          const cur = get().job;
          if (!cur || p.job === cur.id) set({ progress: p });
        }),
        b.onJobChanged(onChanged),
        b.onEntriesUpdated((u) =>
          useLibraryStore.getState().applyArtUpdates(u.generation, u.entries),
        ),
      ]);
      const job = await b.fillJob();
      if (job) accept(job);
      return () => offs.forEach((off) => off());
    },
    start(generation, request) {
      set({ progress: null, lastUndo: null });
      return act((b) => b.startFill(generation, request));
    },
    decide: (decisions) => act((b, job) => b.decideFill(jobId(job), decisions)),
    run: () => act((b, job) => b.runFill(jobId(job))),
    resume: () => act((b, job) => b.resumeFill(jobId(job))),
    cancel: () => act((b, job) => b.cancelFill(jobId(job))),
    async undoLast(generation) {
      set({ pending: true, error: null });
      try {
        const b = await getBackend();
        const undo = await b.undoLastJob(generation);
        useLibraryStore.getState().applyArtUpdates(generation, undo.entries);
        set({ lastUndo: undo, history: await b.jobHistory() });
        return true;
      } catch (e) {
        set({ error: toBackendError(e) });
        return false;
      } finally {
        set({ pending: false });
      }
    },
    async loadHistory() {
      try {
        const b = await getBackend();
        set({ history: await b.jobHistory() });
      } catch (e) {
        set({ error: toBackendError(e) });
      }
    },
    async exportReport(format) {
      const job = get().job;
      if (!job) return null;
      try {
        const b = await getBackend();
        return await b.exportJobReport(job.id, format);
      } catch (e) {
        set({ error: toBackendError(e) });
        return null;
      }
    },
    clearError: () => set({ error: null }),
    reset: () => set(initial),
  };
});
```

- [ ] **Step 3: Modal, focus trap and candidate images**

`src/lib/focusTrap.ts` — move `FOCUSABLE` and `trapTab` out of `src/components/LibrarySettingsDialog.tsx` unchanged, exporting `trapTab`; the dialog imports it from `@/lib/focusTrap`.

`src/components/Modal.tsx`:

```tsx
import { useEffect, useRef, type ReactNode } from 'react';

import { trapTab } from '@/lib/focusTrap';

/** A modal dialog: focus moves in and back, Tab stays inside, Escape closes. */
export function Modal({
  labelledBy,
  onClose,
  className = '',
  children,
}: {
  labelledBy: string;
  onClose(): void;
  className?: string;
  children: ReactNode;
}) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const opener =
      document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null;
    ref.current?.focus();
    return () => opener?.focus();
  }, []);
  useEffect(() => {
    const onKey = (ev: KeyboardEvent) => {
      if (ev.key === 'Escape') onClose();
      else if (ev.key === 'Tab') trapTab(ev, ref.current);
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose]);
  return (
    <div className="modal-backdrop">
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby={labelledBy}
        className={`modal ${className}`}
        ref={ref}
        tabIndex={-1}
      >
        {children}
      </div>
    </div>
  );
}
```

`src/hooks/useImageUrl.ts`:

```ts
import { useEffect, useState } from 'react';

import { getBackend } from '@/ipc/backend';
import type { Media } from '@/ipc/types';

/** A candidate image: of a plan item, or of the drawer's candidate list. */
export type ImageSource =
  | { kind: 'job'; job: number; index: number; candidate: number }
  | {
      kind: 'entry';
      generation: number;
      entryId: number;
      media: Media;
      candidate: number;
    };

export function sourceKey(s: ImageSource): string {
  return s.kind === 'job'
    ? `job:${s.job}:${s.index}:${s.candidate}`
    : `entry:${s.generation}:${s.entryId}:${s.media}:${s.candidate}`;
}

/** A blob URL of the candidate image (revoked when it changes or unmounts). */
export function useImageUrl(source: ImageSource): {
  url: string | null;
  failed: boolean;
} {
  const key = sourceKey(source);
  const [state, setState] = useState<{
    key: string;
    url: string | null;
    failed: boolean;
  } | null>(null);
  useEffect(() => {
    let alive = true;
    let url: string | null = null;
    void (async () => {
      try {
        const b = await getBackend();
        const img =
          source.kind === 'job'
            ? await b.candidateArt(source.job, source.index, source.candidate)
            : await b.entryCandidateArt(
                source.generation,
                source.entryId,
                source.media,
                source.candidate,
              );
        url = URL.createObjectURL(new Blob([img.bytes], { type: img.mime }));
        if (alive) setState({ key, url, failed: false });
        else URL.revokeObjectURL(url);
      } catch {
        if (alive) setState({ key, url: null, failed: true });
      }
    })();
    return () => {
      alive = false;
      if (url) URL.revokeObjectURL(url);
    };
    // `key` identifies `source` completely.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);
  return state?.key === key
    ? { url: state.url, failed: state.failed }
    : { url: null, failed: false };
}
```

`src/components/fill/CandidateImage.tsx`:

```tsx
import { useT } from '@/i18n/useT';
import { useImageUrl, type ImageSource } from '@/hooks/useImageUrl';

/** A candidate image; `label` (a hostile name) is only ever an attribute or text. */
export function CandidateImage({
  source,
  label,
  className = '',
}: {
  source: ImageSource;
  label: string;
  className?: string;
}) {
  const t = useT();
  const { url, failed } = useImageUrl(source);
  if (url) {
    return (
      <img
        src={url}
        alt={label}
        draggable={false}
        className={`art-thumb ${className}`}
      />
    );
  }
  return (
    <div
      role="img"
      aria-label={label}
      className={`art-thumb art-placeholder ${className}`}
    >
      {failed ? t('fill.imageFailed') : ''}
    </div>
  );
}
```

- [ ] **Step 4: The wizard**

`src/components/fill/FillWizard.tsx`:

```tsx
import { useContext, useEffect, useMemo, useState } from 'react';

import { Modal } from '@/components/Modal';
import { LocaleContext } from '@/i18n/core';
import { useT } from '@/i18n/useT';
import { getBackend } from '@/ipc/backend';
import { errorKey } from '@/ipc/errors';
import {
  STRATEGIES,
  type EntryView,
  type JobItemView,
  type JobView,
  type LibraryView,
  type Media,
  type OverwritePolicy,
  type RegionPref,
  type StrategyKind,
} from '@/ipc/types';
import {
  buildFillRequest,
  DEFAULT_REGION_ORDER,
  moveItem,
  type FillScope,
} from '@/lib/fill';
import { useJobStore } from '@/stores/jobStore';
import { useViewStore } from '@/stores/viewStore';

import { CandidateImage } from './CandidateImage';

type Step = 'scope' | 'media' | 'strategy' | 'overwrite' | 'summary';
const STEPS: readonly Step[] = ['scope', 'media', 'strategy', 'overwrite', 'summary'];
const SCOPES: readonly FillScope[] = ['selection', 'filter', 'library'];
const OVERWRITES: readonly OverwritePolicy[] = ['never', 'backup'];

/** A job the summary step can show: a dry run of this library still in play. */
function inPlay(job: JobView | null, generation: number): boolean {
  return (
    job !== null &&
    job.generation === generation &&
    (job.state === 'gathering' ||
      job.state === 'planned' ||
      (job.state === 'paused' && job.paused?.phase === 'gather'))
  );
}

function Samples({ job }: { job: JobView }) {
  const t = useT();
  const [items, setItems] = useState<JobItemView[]>([]);
  useEffect(() => {
    let alive = true;
    void getBackend()
      .then((b) => b.fillJobItems(job.id, 'auto'))
      .then(
        (list) => alive && setItems(list.slice(0, 4)),
        () => alive && setItems([]),
      );
    return () => {
      alive = false;
    };
  }, [job.id]);
  if (items.length === 0) return null;
  return (
    <section aria-label={t('fill.samples')}>
      <h3 className="section-title">{t('fill.samples')}</h3>
      <ul className="sample-grid">
        {items.map((i) => (
          <li key={i.index}>
            <CandidateImage
              source={{ kind: 'job', job: job.id, index: i.index, candidate: 0 }}
              label={i.candidates[0]?.name ?? i.name}
            />
            <span className="truncate" title={i.name}>
              {i.name}
            </span>
          </li>
        ))}
      </ul>
    </section>
  );
}

export function FillWizard({
  view,
  visible,
  selectedId,
  onClose,
}: {
  view: LibraryView;
  visible: readonly EntryView[];
  selectedId: number | null;
  onClose(): void;
}) {
  const t = useT();
  const locale = useContext(LocaleContext);
  const n = useMemo(() => new Intl.NumberFormat(locale), [locale]);
  const job = useJobStore((s) => s.job);
  const progress = useJobStore((s) => s.progress);
  const error = useJobStore((s) => s.error);
  const pending = useJobStore((s) => s.pending);
  const start = useJobStore((s) => s.start);
  const run = useJobStore((s) => s.run);
  const cancel = useJobStore((s) => s.cancel);
  const resume = useJobStore((s) => s.resume);
  const setOverlay = useViewStore((s) => s.setOverlay);
  const [step, setStep] = useState<Step>(() =>
    inPlay(useJobStore.getState().job, view.generation) ? 'summary' : 'scope',
  );
  const [scope, setScope] = useState<FillScope>(
    selectedId !== null
      ? 'selection'
      : visible.length < view.entries.length
        ? 'filter'
        : 'library',
  );
  const [media, setMedia] = useState<Media[]>(view.profile.media.slice(0, 1));
  const [strategy, setStrategy] = useState<StrategyKind>('rom-region');
  const [regions, setRegions] = useState<RegionPref[]>([...DEFAULT_REGION_ORDER]);
  const [overwrite, setOverwrite] = useState<OverwritePolicy>('never');
  const index = STEPS.indexOf(step);
  const regionName = (r: RegionPref) =>
    r === 'rom' ? t('fill.regionRom') : t(`regions.${r}`);
  const counts: Record<FillScope, number> = {
    selection: selectedId === null ? 0 : 1,
    filter: visible.length,
    library: view.entries.length,
  };

  async function preview() {
    setStep('summary');
    await start(
      view.generation,
      buildFillRequest({
        scope,
        selectedId,
        visible,
        media,
        strategy,
        regions,
        overwrite,
      }),
    );
  }

  async function runJob() {
    if (await run()) setOverlay('none');
  }

  async function discard() {
    await cancel();
    onClose();
  }

  let body;
  if (step === 'scope') {
    body = (
      <fieldset className="wizard-fieldset">
        <legend className="section-title">{t('fill.scope.title')}</legend>
        {SCOPES.map((s) => (
          <label key={s} className="wizard-choice">
            <input
              type="radio"
              name="fill-scope"
              checked={scope === s}
              disabled={counts[s] === 0}
              onChange={() => setScope(s)}
            />
            {t(`fill.scope.${s}`, { n: n.format(counts[s]) })}
          </label>
        ))}
      </fieldset>
    );
  } else if (step === 'media') {
    body = (
      <fieldset className="wizard-fieldset">
        <legend className="section-title">{t('fill.media.title')}</legend>
        {view.profile.media.map((m) => (
          <label key={m} className="wizard-choice">
            <input
              type="checkbox"
              name={`fill-media-${m}`}
              checked={media.includes(m)}
              onChange={() =>
                setMedia((list) =>
                  list.includes(m) ? list.filter((x) => x !== m) : [...list, m],
                )
              }
            />
            {t(`art.media.${m}`)}
          </label>
        ))}
      </fieldset>
    );
  } else if (step === 'strategy') {
    body = (
      <>
        <fieldset className="wizard-fieldset">
          <legend className="section-title">{t('fill.strategy.title')}</legend>
          {STRATEGIES.map((s) => (
            <label key={s} className="wizard-choice">
              <input
                type="radio"
                name="fill-strategy"
                checked={strategy === s}
                onChange={() => setStrategy(s)}
              />
              <span>
                {t(`fill.strategy.${s}`)}
                <span className="hint wizard-hint">
                  {t(`fill.strategyHint.${s}`)}
                </span>
              </span>
            </label>
          ))}
        </fieldset>
        <h3 className="section-title" id="fill-regions">
          {t('fill.regions')}
        </h3>
        <ol className="region-order" aria-labelledby="fill-regions">
          {regions.map((r, i) => (
            <li key={r}>
              <span>{regionName(r)}</span>
              <button
                type="button"
                className="icon-button"
                disabled={i === 0}
                aria-label={t('fill.moveUp', { region: regionName(r) })}
                onClick={() => setRegions((l) => moveItem(l, i, -1))}
              >
                ↑
              </button>
              <button
                type="button"
                className="icon-button"
                disabled={i === regions.length - 1}
                aria-label={t('fill.moveDown', { region: regionName(r) })}
                onClick={() => setRegions((l) => moveItem(l, i, 1))}
              >
                ↓
              </button>
            </li>
          ))}
        </ol>
      </>
    );
  } else if (step === 'overwrite') {
    body = (
      <fieldset className="wizard-fieldset">
        <legend className="section-title">{t('fill.overwrite.title')}</legend>
        {OVERWRITES.map((o) => (
          <label key={o} className="wizard-choice">
            <input
              type="radio"
              name="fill-overwrite"
              checked={overwrite === o}
              onChange={() => setOverwrite(o)}
            />
            {t(`fill.overwrite.${o}`)}
          </label>
        ))}
        <p className="hint">{t('fill.overwrite.backupHint')}</p>
      </fieldset>
    );
  } else {
    body = renderSummary();
  }

  /** The summary step (a plain render function: `Samples` must not remount on every progress event). */
  function renderSummary() {
    if (!job || job.generation !== view.generation) return null;
    if (job.state === 'gathering') {
      const p = progress?.job === job.id ? progress : null;
      return (
        <div className="progress-row">
          <p role="status">
            {t('fill.gathering', {
              done: n.format(p?.done ?? 0),
              total: n.format(p?.total ?? 0),
            })}
          </p>
          <progress
            max={p?.total || 1}
            value={p?.done ?? 0}
            aria-label={t('fill.summary')}
          />
          {p?.label ? (
            <p className="hint truncate" title={p.label}>
              {p.label}
            </p>
          ) : null}
        </div>
      );
    }
    if (job.state === 'paused' && job.paused) {
      return (
        <div role="alert" className="pause-banner">
          <p>{t(`jobs.pause.${job.paused.kind}`)}</p>
          <p className="hint">{t('fill.pausedHint')}</p>
          <button
            type="button"
            className="primary-button"
            disabled={pending}
            onClick={() => void resume()}
          >
            {t('jobs.resume')}
          </button>
        </div>
      );
    }
    if (job.state === 'failed') {
      return (
        <p role="alert" className="error-banner">
          {t('fill.failed', { detail: job.error ?? '' })}
        </p>
      );
    }
    if (!job.plan) return null;
    const { totals, per_system } = job.plan;
    if (totals.auto + totals.review + totals.none === 0) {
      return <p>{t('fill.nothingToDo')}</p>;
    }
    return (
      <>
        <p className="hint">{t('fill.summaryHint')}</p>
        <table className="summary-table">
          <caption className="sr-only">{t('fill.summary')}</caption>
          <thead>
            <tr>
              <th scope="col">{t('fill.system')}</th>
              <th scope="col">{t('fill.auto')}</th>
              <th scope="col">{t('fill.review')}</th>
              <th scope="col">{t('fill.none')}</th>
            </tr>
          </thead>
          <tbody>
            {per_system.map((s) => (
              <tr key={s.folder}>
                <th scope="row">{s.folder}</th>
                <td>{n.format(s.auto)}</td>
                <td>{n.format(s.review)}</td>
                <td>{n.format(s.none)}</td>
              </tr>
            ))}
            <tr>
              <th scope="row">{t('fill.total')}</th>
              <td>{n.format(totals.auto)}</td>
              <td>{n.format(totals.review)}</td>
              <td>{n.format(totals.none)}</td>
            </tr>
          </tbody>
        </table>
        <Samples job={job} />
      </>
    );
  }

  const planned = job !== null && job.generation === view.generation && job.state === 'planned';
  return (
    <Modal labelledBy="fill-title" onClose={onClose} className="wizard">
      <h2 id="fill-title" className="section-title">
        {t('fill.title')}
      </h2>
      <p className="hint">
        {t('fill.stepOf', { n: index + 1, total: STEPS.length })}
      </p>
      {view.profile.id === 'batocera' && <p className="hint">{t('fill.batocera')}</p>}
      {body}
      {error && (
        <p role="alert" className="error-banner">
          {t(errorKey(error.code), { detail: error.detail ?? '' })}
        </p>
      )}
      <div className="modal-actions">
        {step !== 'summary' && index > 0 && (
          <button
            type="button"
            className="ghost-button"
            onClick={() => setStep(STEPS[index - 1] ?? 'scope')}
          >
            {t('fill.back')}
          </button>
        )}
        {step === 'summary' && planned && (
          <button type="button" className="ghost-button" onClick={() => void discard()}>
            {t('fill.discard')}
          </button>
        )}
        <button type="button" className="ghost-button" onClick={onClose}>
          {t('fill.close')}
        </button>
        {index < STEPS.indexOf('overwrite') && (
          <button
            type="button"
            className="primary-button"
            disabled={step === 'media' && media.length === 0}
            onClick={() => setStep(STEPS[index + 1] ?? 'summary')}
          >
            {t('fill.next')}
          </button>
        )}
        {step === 'overwrite' && (
          <button
            type="button"
            className="primary-button"
            disabled={pending || media.length === 0}
            onClick={() => void preview()}
          >
            {t('fill.preview')}
          </button>
        )}
        {planned && job.plan && (
          <button
            type="button"
            className="primary-button"
            disabled={pending}
            onClick={() => void runJob()}
          >
            {t('fill.run', { n: n.format(job.plan.totals.auto + job.decided) })}
          </button>
        )}
      </div>
    </Modal>
  );
}
```

(The table is named "Dry run" by its `<caption>`.)

- [ ] **Step 5: Toolbar and library screen**

`src/components/library/Toolbar.tsx`: props gain `onFill(): void`; replace the disabled `<span title=…><button … disabled>` block and the `fill-hint` span with:

```tsx
      <button type="button" className="primary-button" onClick={onFill}>
        {t('toolbar.fill')}
      </button>
```

`src/screens/LibraryScreen.tsx`:
- imports `FillWizard`, `useJobStore`.
- `const overlay = useViewStore((s) => s.overlay); const setOverlay = useViewStore((s) => s.setOverlay); const closeOverlay = useViewStore((s) => s.closeOverlay);`
- job events for as long as the library screen lives:

```tsx
  useEffect(() => {
    let alive = true;
    let off: (() => void) | undefined;
    void useJobStore
      .getState()
      .connect()
      .then((unsubscribe) => {
        if (alive) off = unsubscribe;
        else unsubscribe();
      });
    return () => {
      alive = false;
      off?.();
    };
  }, []);
```

- the keyboard handler returns early when `settingsOpen || overlay !== 'none'` (add `overlay` to its deps).
- `<Toolbar … onFill={() => setOverlay('wizard')} />`
- before `{settingsOpen && …}`:

```tsx
      {overlay === 'wizard' && (
        <FillWizard
          view={view}
          visible={visible}
          selectedId={selectedId}
          onClose={closeOverlay}
        />
      )}
```

- [ ] **Step 6: Strings and styles**

`locales/en/ui.json`: remove `toolbar.fillHint`; add

```json
  "fill": {
    "title": "Fill missing art",
    "stepOf": "Step {n} of {total}",
    "next": "Next",
    "back": "Back",
    "close": "Close",
    "scope": {
      "title": "What to fill",
      "selection": "Selected ROM ({n})",
      "filter": "Current filter ({n} ROMs)",
      "library": "Whole library ({n} ROMs)"
    },
    "media": { "title": "Which art" },
    "strategy": {
      "title": "How to choose the image",
      "rom-region": "ROM region",
      "priority": "Source priority",
      "popularity": "Most common variant"
    },
    "strategyHint": {
      "rom-region": "Prefers the ROM's own region, then the order below.",
      "priority": "Prefers the first source, then the order below.",
      "popularity": "Prefers the variant that has box, screenshot and title, then source priority."
    },
    "regions": "Region order",
    "regionRom": "The ROM's own region",
    "moveUp": "Move {region} up",
    "moveDown": "Move {region} down",
    "overwrite": {
      "title": "Existing art",
      "never": "Only missing art",
      "backup": "Also replace existing art (with a backup)",
      "backupHint": "Replaced images are kept in the app's data folder and come back with Undo."
    },
    "batocera": "Batocera and Knulli read art from gamelist.xml, which Coverly ROMs does not update yet: new images may not show until it does.",
    "preview": "Preview (dry run)",
    "gathering": "Looking for art… {done} of {total}",
    "summary": "Dry run",
    "summaryHint": "Nothing has been written yet.",
    "system": "System",
    "auto": "Automatic",
    "review": "To review",
    "none": "No art",
    "total": "Total",
    "samples": "Sample images",
    "imageFailed": "Image not available",
    "nothingToDo": "Nothing to fill here.",
    "run": "Run ({n})",
    "discard": "Discard the plan",
    "failed": "The dry run failed: {detail}",
    "pausedHint": "The dry run paused. Resume it when the connection is back."
  },
  "jobs": {
    "resume": "Resume",
    "pause": {
      "rate-limit": "Paused: the art server is limiting requests. Wait a few minutes and resume.",
      "offline": "Paused: no connection to the art server. Resume when you are back online.",
      "volume": "Paused: the SD card is full, read-only or was removed. Fix it and resume."
    }
  }
```

`locales/es/ui.json`: remove `toolbar.fillHint`; add

```json
  "fill": {
    "title": "Rellenar arte que falta",
    "stepOf": "Paso {n} de {total}",
    "next": "Siguiente",
    "back": "Atrás",
    "close": "Cerrar",
    "scope": {
      "title": "Qué rellenar",
      "selection": "ROM seleccionada ({n})",
      "filter": "Filtro actual ({n} ROMs)",
      "library": "Toda la biblioteca ({n} ROMs)"
    },
    "media": { "title": "Qué arte" },
    "strategy": {
      "title": "Cómo elegir la imagen",
      "rom-region": "Región de la ROM",
      "priority": "Prioridad de fuentes",
      "popularity": "Variante más común"
    },
    "strategyHint": {
      "rom-region": "Prefiere la región de la propia ROM y después el orden de abajo.",
      "priority": "Prefiere la primera fuente y después el orden de abajo.",
      "popularity": "Prefiere la variante que tiene caja, captura y título; después, la prioridad de fuentes."
    },
    "regions": "Orden de regiones",
    "regionRom": "La región de la propia ROM",
    "moveUp": "Subir {region}",
    "moveDown": "Bajar {region}",
    "overwrite": {
      "title": "Arte existente",
      "never": "Solo el arte que falta",
      "backup": "Sustituir también el arte existente (con copia de seguridad)",
      "backupHint": "Las imágenes sustituidas se guardan en la carpeta de datos de la app y vuelven con Deshacer."
    },
    "batocera": "Batocera y Knulli leen el arte de gamelist.xml, que Coverly ROMs aún no actualiza: puede que las imágenes nuevas no se vean hasta entonces.",
    "preview": "Vista previa (simulación)",
    "gathering": "Buscando arte… {done} de {total}",
    "summary": "Simulación",
    "summaryHint": "Todavía no se ha escrito nada.",
    "system": "Sistema",
    "auto": "Automáticas",
    "review": "Por revisar",
    "none": "Sin arte",
    "total": "Total",
    "samples": "Imágenes de muestra",
    "imageFailed": "Imagen no disponible",
    "nothingToDo": "No hay nada que rellenar aquí.",
    "run": "Ejecutar ({n})",
    "discard": "Descartar el plan",
    "failed": "La simulación ha fallado: {detail}",
    "pausedHint": "La simulación se ha pausado. Reanúdala cuando vuelva la conexión."
  },
  "jobs": {
    "resume": "Reanudar",
    "pause": {
      "rate-limit": "En pausa: el servidor de arte está limitando las peticiones. Espera unos minutos y reanuda.",
      "offline": "En pausa: no hay conexión con el servidor de arte. Reanuda cuando vuelvas a tener conexión.",
      "volume": "En pausa: la tarjeta SD está llena, en solo lectura o se ha extraído. Corrígelo y reanuda."
    }
  }
```

`src/styles/globals.css` (append):

```css
.wizard {
  width: min(40rem, calc(100vw - 2rem));
  max-height: calc(100vh - 4rem);
  overflow-y: auto;
}

.wizard-fieldset {
  display: grid;
  gap: 0.5rem;
  margin: 0.75rem 0;
  border: 0;
  padding: 0;
}

.wizard-choice {
  display: flex;
  gap: 0.5rem;
  align-items: flex-start;
}

.wizard-hint {
  display: block;
}

.region-order {
  display: grid;
  gap: 0.25rem;
  margin: 0.5rem 0;
  padding-left: 1.25rem;
}

.region-order li {
  display: flex;
  gap: 0.5rem;
  align-items: center;
}

.region-order li > span {
  flex: 1;
}

.summary-table {
  width: 100%;
  margin: 0.75rem 0;
  border-collapse: collapse;
  font-variant-numeric: tabular-nums;
}

.summary-table th,
.summary-table td {
  padding: 0.25rem 0.5rem;
  text-align: right;
  border-bottom: 1px solid var(--cr-border, rgb(127 127 127 / 0.25));
}

.summary-table th[scope='row'],
.summary-table thead th:first-child {
  text-align: left;
}

.sample-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(7rem, 1fr));
  gap: 0.75rem;
  margin: 0.5rem 0 0;
  padding: 0;
  list-style: none;
}

.sample-grid li {
  display: grid;
  gap: 0.25rem;
  min-width: 0;
}

.progress-row progress {
  width: 100%;
}

.pause-banner {
  display: grid;
  gap: 0.5rem;
  padding: 0.75rem;
  border-radius: 0.5rem;
  background: rgb(234 179 8 / 0.15);
}
```

(If `--cr-border` is not defined in `globals.css`, the fallback colour applies.)

- [ ] **Step 7: Run the tests**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && pnpm test && pnpm lint && pnpm typecheck`
Expected: PASS.

- [ ] **Step 8: Gates and commit**

Run all gates. Then:

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && git add src locales && git commit -m "feat(ui): add the fill wizard with a dry-run summary and run"
```

---
### Task 10: Review queue — current art vs candidates, keyboard-driven decisions (M4-02)

**Files:**
- Create: `src/components/fill/ReviewQueue.tsx`, `src/components/fill/ReviewQueue.test.tsx`
- Modify: `src/components/fill/FillWizard.tsx` ("Review (n)" button), `src/screens/LibraryScreen.tsx` (overlay `review`), `src/styles/globals.css`, `locales/{en,es}/ui.json`

**Interfaces:**
- Consumes: Task 9 `useJobStore` (`job`, `decide`, `error`), `Modal`, `CandidateImage`, `useViewStore.setOverlay`; Task 8 `fillJobItems(job, 'review')`; `ArtThumb`.
- Produces: `<ReviewQueue view onClose />` — loads the job's review items; per item shows the current art next to every candidate (source, regions, score); decisions go to the backend immediately (`decide`), so they feed the run. Keys: `←`/`→` (or `k`/`j`) move, `1`–`9` select a candidate, `Enter` accepts the selected one (`accept` for candidate 0, else `pick`), `s` skips, `a` accepts every undecided item, `Esc` goes back to the wizard.

- [ ] **Step 1: Write the failing tests**

`src/components/fill/ReviewQueue.test.tsx`:

```tsx
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import App from '@/App';
import { I18nProvider } from '@/i18n/I18nProvider';
import { setBackend } from '@/ipc/backend';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import { createMockBackend, MOCK_LIBRARIES } from '@/ipc/mock/mockBackend';
import { mockClass, mockFailure } from '@/ipc/mock/mockJobs';
import type { JobItemView } from '@/ipc/types';
import { DEFAULT_REGION_ORDER } from '@/lib/fill';
import { useJobStore } from '@/stores/jobStore';
import { resetLibraryStore, useLibraryStore } from '@/stores/libraryStore';
import { useViewStore } from '@/stores/viewStore';
import { stubLayout } from '@/test/layout';

async function planned(locale: 'en' | 'es' = 'en') {
  const backend = createMockBackend({
    delayMs: 0,
    now: FIXTURE_NOW,
    pauseAfter: null,
  });
  setBackend(backend);
  resetLibraryStore();
  useJobStore.getState().reset();
  await useLibraryStore.getState().open(MOCK_LIBRARIES[1].path);
  render(
    <I18nProvider locale={locale}>
      <App />
    </I18nProvider>,
  );
  const view = useLibraryStore.getState().view!;
  await useJobStore.getState().start(view.generation, {
    entry_ids: null,
    media: ['cover'],
    strategy: 'rom-region',
    regions: [...DEFAULT_REGION_ORDER],
    overwrite: 'never',
  });
  await waitFor(() => expect(useJobStore.getState().job?.state).toBe('planned'));
  const review = view.entries.filter(
    (e) => e.art.cover === 'missing' && mockClass(e.id, 'cover') === 'review',
  );
  return { backend, view, review };
}

function openQueue(name = 'Review doubtful matches') {
  act(() => useViewStore.getState().setOverlay('review'));
  return screen.findByRole('dialog', { name });
}

describe('Review queue', () => {
  let restore: () => void;
  beforeEach(() => {
    restore = stubLayout(1280, 800);
  });
  afterEach(() => {
    restore();
    setBackend(null);
  });

  it('shows the current art and the candidates side by side', async () => {
    const { review } = await planned();
    const dialog = await openQueue();
    const first = review[0]!;
    expect(
      await within(dialog).findByRole('heading', { level: 3, name: first.name }),
    ).toBeInTheDocument();
    expect(within(dialog).getByText('Current art')).toBeInTheDocument();
    const group = within(dialog).getByRole('group', { name: 'Candidates' });
    const buttons = within(group).getAllByRole('button');
    expect(buttons.length).toBeGreaterThanOrEqual(2);
    expect(buttons[0]).toHaveAttribute('aria-pressed', 'true');
    expect(within(group).getByText(/^1\. libretro · /)).toBeInTheDocument();
    expect(
      within(dialog).getByText(`1 of ${review.length} · 0 decided`),
    ).toBeInTheDocument();
  });

  it('is keyboard driven: digits pick, Enter accepts, S skips, A accepts the rest', async () => {
    const { backend, review } = await planned();
    const spy = vi.spyOn(backend, 'decideFill');
    const dialog = await openQueue();
    await within(dialog).findByRole('heading', { level: 3, name: review[0]!.name });
    const items = await backend.fillJobItems(useJobStore.getState().job!.id, 'review');
    fireEvent.keyDown(window, { key: '2' });
    const group = within(dialog).getByRole('group', { name: 'Candidates' });
    expect(within(group).getAllByRole('button')[1]).toHaveAttribute('aria-pressed', 'true');
    fireEvent.keyDown(window, { key: 'Enter' });
    await waitFor(() =>
      expect(spy).toHaveBeenLastCalledWith(expect.any(Number), [
        { choice: 'pick', candidate: 1, index: items[0]!.index },
      ]),
    );
    expect(
      await within(dialog).findByRole('heading', { level: 3, name: review[1]!.name }),
    ).toBeInTheDocument();
    fireEvent.keyDown(window, { key: 's' });
    await waitFor(() =>
      expect(spy).toHaveBeenLastCalledWith(expect.any(Number), [
        { choice: 'skip', index: items[1]!.index },
      ]),
    );
    fireEvent.keyDown(window, { key: 'a' });
    await waitFor(() =>
      expect(useJobStore.getState().job?.decided).toBe(review.length),
    );
  });

  it('decisions feed the run', async () => {
    const { view, review } = await planned();
    const dialog = await openQueue();
    await within(dialog).findByRole('heading', { level: 3, name: review[0]!.name });
    await userEvent.click(
      within(dialog).getByRole('button', { name: 'Accept all remaining' }),
    );
    await waitFor(() =>
      expect(useJobStore.getState().job?.decided).toBe(review.length),
    );
    await userEvent.click(within(dialog).getByRole('button', { name: 'Done' }));
    const wizard = await screen.findByRole('dialog', { name: 'Fill missing art' });
    const auto = view.entries.filter(
      (e) => e.art.cover === 'missing' && mockClass(e.id, 'cover') === 'auto',
    );
    await userEvent.click(
      within(wizard).getByRole('button', {
        name: `Run (${auto.length + review.length})`,
      }),
    );
    await waitFor(() => expect(useJobStore.getState().job?.state).toBe('done'));
    const written = auto.filter((e) => mockFailure(e.id) === null).length;
    expect(useJobStore.getState().job?.result?.filled).toBe(written + review.length);
  });

  it('renders_hostile_names_as_text', async () => {
    const { backend } = await planned();
    const hostile = '<img src=x onerror=alert(1)>';
    const item: JobItemView = {
      index: 0,
      entry_id: null,
      folder: 'SFC',
      name: hostile,
      media: 'cover',
      action: 'review',
      confidence: 0.8,
      note: 'unreleased-variant',
      none_reason: null,
      candidates: [
        {
          index: 0,
          source: 'libretro',
          name: `${hostile} (USA) (Beta)`,
          regions: ['usa'],
          kind: 'title-exact',
          score: 95,
          confidence: 0.95,
        },
      ],
      replaces_existing: false,
      decision: null,
      outcome: null,
    };
    vi.spyOn(backend, 'fillJobItems').mockResolvedValue([item]);
    const dialog = await openQueue();
    expect(
      await within(dialog).findByRole('heading', { level: 3, name: hostile }),
    ).toBeInTheDocument();
    expect(within(dialog).getByText(`${hostile} (USA) (Beta)`)).toBeInTheDocument();
    expect(
      within(dialog).getByText(/beta, prototype, hack or unlicensed/),
    ).toBeInTheDocument();
    expect(document.querySelector('img[src="x"]')).toBeNull();
    expect(document.querySelector('[onerror]')).toBeNull();
  });

  it('speaks Spanish', async () => {
    await planned('es');
    const dialog = await openQueue('Revisar coincidencias dudosas');
    expect(
      within(dialog).getByRole('button', { name: 'Aceptar el resto' }),
    ).toBeInTheDocument();
  });
});
```

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && pnpm test -- src/components/fill/ReviewQueue.test.tsx`
Expected: FAIL — `ReviewQueue` does not exist and the `review` overlay renders nothing.

- [ ] **Step 2: The review queue**

`src/components/fill/ReviewQueue.tsx`:

```tsx
import { useContext, useEffect, useMemo, useState } from 'react';

import { ArtThumb } from '@/components/library/ArtThumb';
import { Modal } from '@/components/Modal';
import { LocaleContext } from '@/i18n/core';
import { useT } from '@/i18n/useT';
import { getBackend } from '@/ipc/backend';
import { errorKey, toBackendError, type BackendError } from '@/ipc/errors';
import type { Decision, DecisionIn, JobItemView, LibraryView } from '@/ipc/types';
import { useJobStore } from '@/stores/jobStore';

import { CandidateImage } from './CandidateImage';

export function ReviewQueue({
  view,
  onClose,
}: {
  view: LibraryView;
  onClose(): void;
}) {
  const t = useT();
  const locale = useContext(LocaleContext);
  const n = useMemo(() => new Intl.NumberFormat(locale), [locale]);
  const job = useJobStore((s) => s.job);
  const decide = useJobStore((s) => s.decide);
  const storeError = useJobStore((s) => s.error);
  const [items, setItems] = useState<JobItemView[] | null>(null);
  const [loadError, setLoadError] = useState<BackendError | null>(null);
  const [pos, setPos] = useState(0);
  const [selected, setSelected] = useState<Record<number, number>>({});
  const [decisions, setDecisions] = useState<Record<number, Decision>>({});
  const jobId = job?.id ?? null;
  const editable = job?.state === 'planned';

  useEffect(() => {
    if (jobId === null) return;
    let alive = true;
    void getBackend()
      .then((b) => b.fillJobItems(jobId, 'review'))
      .then(
        (list) => {
          if (!alive) return;
          setItems(list);
          setDecisions(
            Object.fromEntries(
              list.flatMap((i) => (i.decision ? [[i.index, i.decision]] : [])),
            ),
          );
        },
        (e: unknown) => alive && setLoadError(toBackendError(e)),
      );
    return () => {
      alive = false;
    };
  }, [jobId]);

  const item = items?.[pos] ?? null;
  const chosen = item ? (selected[item.index] ?? 0) : 0;
  const decided = Object.keys(decisions).length;

  function move(delta: number) {
    setPos((p) => Math.max(0, Math.min((items?.length ?? 1) - 1, p + delta)));
  }

  async function send(list: DecisionIn[]): Promise<boolean> {
    if (list.length === 0) return true;
    if (!(await decide(list))) return false;
    setDecisions((d) => {
      const next = { ...d };
      for (const { index, ...decision } of list) next[index] = decision as Decision;
      return next;
    });
    return true;
  }

  async function accept() {
    if (!item) return;
    const d: Decision =
      chosen === 0 ? { choice: 'accept' } : { choice: 'pick', candidate: chosen };
    if (await send([{ ...d, index: item.index }])) move(1);
  }

  async function skip() {
    if (!item) return;
    if (await send([{ choice: 'skip', index: item.index }])) move(1);
  }

  async function acceptAll() {
    if (!items) return;
    await send(
      items
        .filter((i) => !decisions[i.index])
        .map((i) => ({ choice: 'accept' as const, index: i.index })),
    );
  }

  function pick(candidate: number) {
    if (!item || candidate >= item.candidates.length) return;
    setSelected((s) => ({ ...s, [item.index]: candidate }));
  }

  // Re-subscribed on every render so the keys always act on the shown item.
  useEffect(() => {
    function onKey(ev: KeyboardEvent) {
      if (ev.metaKey || ev.ctrlKey || ev.altKey) return;
      const key = ev.key.length === 1 ? ev.key.toLowerCase() : ev.key;
      if (key === 'ArrowRight' || key === 'ArrowDown' || key === 'j') {
        ev.preventDefault();
        move(1);
      } else if (key === 'ArrowLeft' || key === 'ArrowUp' || key === 'k') {
        ev.preventDefault();
        move(-1);
      } else if (!editable) {
        return;
      } else if (key === 'Enter') {
        ev.preventDefault();
        void accept();
      } else if (key === 's') {
        void skip();
      } else if (key === 'a') {
        void acceptAll();
      } else if (/^[1-9]$/.test(key)) {
        pick(Number(key) - 1);
      }
    }
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });

  const entry =
    item?.entry_id !== null && item?.entry_id !== undefined
      ? (view.entries[item.entry_id] ?? null)
      : null;
  const status = (d: Decision | undefined) =>
    !d ? t('review.undecided') : d.choice === 'skip' ? t('review.skipped') : t('review.accepted');
  const error = loadError ?? storeError;

  return (
    <Modal labelledBy="review-title" onClose={onClose} className="review">
      <h2 id="review-title" className="section-title">
        {t('review.title')}
      </h2>
      {items === null && !loadError && <p role="status">{t('review.loading')}</p>}
      {items !== null && items.length === 0 && <p>{t('review.empty')}</p>}
      {item && items && (
        <>
          <p className="hint">
            {t('review.position', {
              n: n.format(pos + 1),
              total: n.format(items.length),
              decided: n.format(decided),
            })}
          </p>
          <h3 className="review-name truncate" title={item.name}>
            {item.name}
          </h3>
          <p className="hint">
            {item.folder} · {t(`art.media.${item.media}`)} ·{' '}
            {status(decisions[item.index])}
          </p>
          {item.note === 'unreleased-variant' && (
            <p className="hint review-note">{t('review.variantNote')}</p>
          )}
          <div className="review-compare">
            <figure className="review-current">
              {entry ? (
                <ArtThumb generation={view.generation} entry={entry} media={item.media} />
              ) : (
                <div className="art-thumb art-placeholder" aria-hidden="true" />
              )}
              <figcaption>{t('review.current')}</figcaption>
            </figure>
            <div role="group" aria-label={t('review.candidates')} className="candidate-grid">
              {item.candidates.map((c) => (
                <button
                  key={c.index}
                  type="button"
                  className="candidate"
                  aria-pressed={chosen === c.index}
                  onClick={() => pick(c.index)}
                >
                  {jobId !== null && (
                    <CandidateImage
                      source={{ kind: 'job', job: jobId, index: item.index, candidate: c.index }}
                      label={c.name}
                    />
                  )}
                  <span className="truncate" title={c.name}>
                    {c.name}
                  </span>
                  <span className="hint">
                    {t('review.meta', {
                      key: c.index + 1,
                      source: c.source,
                      regions: c.regions.map((r) => t(`regions.${r}`)).join(', ') || '—',
                      score: Math.round(c.confidence * 100),
                    })}
                  </span>
                </button>
              ))}
            </div>
          </div>
        </>
      )}
      {error && (
        <p role="alert" className="error-banner">
          {t(errorKey(error.code), { detail: error.detail ?? '' })}
        </p>
      )}
      <p className="hint">{t('review.keys')}</p>
      <div className="modal-actions">
        <button type="button" className="ghost-button" disabled={pos === 0} onClick={() => move(-1)}>
          {t('review.previous')}
        </button>
        <button
          type="button"
          className="ghost-button"
          disabled={!items || pos >= items.length - 1}
          onClick={() => move(1)}
        >
          {t('review.next')}
        </button>
        <button type="button" className="ghost-button" disabled={!editable || !item} onClick={() => void skip()}>
          {t('review.skip')}
        </button>
        <button type="button" className="ghost-button" disabled={!editable || !items} onClick={() => void acceptAll()}>
          {t('review.acceptAll')}
        </button>
        <button type="button" className="primary-button" disabled={!editable || !item} onClick={() => void accept()}>
          {t('review.accept')}
        </button>
        <button type="button" className="ghost-button" onClick={onClose}>
          {t('review.done')}
        </button>
      </div>
    </Modal>
  );
}
```

- [ ] **Step 3: Wire it in**

`src/components/fill/FillWizard.tsx` — in the action row, before the Run button:

```tsx
        {planned && job.plan && job.plan.totals.review > 0 && (
          <button
            type="button"
            className="ghost-button"
            onClick={() => setOverlay('review')}
          >
            {t('review.open', { n: n.format(job.plan.totals.review) })}
          </button>
        )}
```

`src/screens/LibraryScreen.tsx` — import `ReviewQueue`; after the wizard overlay:

```tsx
      {overlay === 'review' && (
        <ReviewQueue view={view} onClose={() => setOverlay('wizard')} />
      )}
```

`locales/en/ui.json` add:

```json
  "review": {
    "title": "Review doubtful matches",
    "open": "Review ({n})",
    "loading": "Loading…",
    "empty": "Nothing to review.",
    "position": "{n} of {total} · {decided} decided",
    "current": "Current art",
    "candidates": "Candidates",
    "meta": "{key}. {source} · {regions} · {score}%",
    "variantNote": "The best image is a beta, prototype, hack or unlicensed variant: check it before writing.",
    "undecided": "Not decided",
    "accepted": "Will be written",
    "skipped": "Skipped",
    "keys": "Keys: ← → move · 1–9 choose · Enter accept · S skip · A accept all remaining · Esc back",
    "previous": "Previous",
    "next": "Next",
    "accept": "Accept",
    "skip": "Skip",
    "acceptAll": "Accept all remaining",
    "done": "Done"
  }
```

`locales/es/ui.json` add:

```json
  "review": {
    "title": "Revisar coincidencias dudosas",
    "open": "Revisar ({n})",
    "loading": "Cargando…",
    "empty": "No hay nada que revisar.",
    "position": "{n} de {total} · {decided} decididas",
    "current": "Arte actual",
    "candidates": "Candidatas",
    "meta": "{key}. {source} · {regions} · {score} %",
    "variantNote": "La mejor imagen es una beta, un prototipo, un hack o una variante sin licencia: revísala antes de escribirla.",
    "undecided": "Sin decidir",
    "accepted": "Se escribirá",
    "skipped": "Omitida",
    "keys": "Teclas: ← → moverse · 1–9 elegir · Intro aceptar · S omitir · A aceptar el resto · Esc volver",
    "previous": "Anterior",
    "next": "Siguiente",
    "accept": "Aceptar",
    "skip": "Omitir",
    "acceptAll": "Aceptar el resto",
    "done": "Hecho"
  }
```

`src/styles/globals.css` (append):

```css
.review {
  width: min(56rem, calc(100vw - 2rem));
  max-height: calc(100vh - 4rem);
  overflow-y: auto;
}

.review-name {
  margin: 0.25rem 0;
  font-size: 1.1rem;
  font-weight: 600;
  unicode-bidi: plaintext;
}

.review-note {
  color: rgb(202 138 4);
}

.review-compare {
  display: grid;
  grid-template-columns: 10rem 1fr;
  gap: 1rem;
  margin: 0.75rem 0;
}

.review-current {
  margin: 0;
}

.candidate-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(9rem, 1fr));
  gap: 0.75rem;
}

.candidate {
  display: grid;
  gap: 0.25rem;
  min-width: 0;
  padding: 0.5rem;
  border: 2px solid transparent;
  border-radius: 0.5rem;
  background: none;
  text-align: left;
  unicode-bidi: plaintext;
}

.candidate[aria-pressed='true'] {
  border-color: var(--cr-accent, rgb(59 130 246));
}
```

- [ ] **Step 4: Run the tests**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && pnpm test && pnpm lint && pnpm typecheck`
Expected: PASS.

- [ ] **Step 5: Gates and commit**

Run all gates. Then:

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && git add src locales && git commit -m "feat(ui): add the keyboard-driven review queue whose decisions feed the run"
```

---
### Task 11: Jobs panel — progress, pause banner + resume, cancel, per-item errors, undo with confirmation, history (M4-03)

**Files:**
- Create: `src/components/fill/JobsPanel.tsx`, `src/components/fill/JobsPanel.test.tsx`
- Modify: `src/components/library/Toolbar.tsx` (Jobs button with state), `src/components/fill/FillWizard.tsx` (Run opens the panel), `src/screens/LibraryScreen.tsx` (overlay `jobs`), `src/styles/globals.css`, `locales/{en,es}/ui.json`

**Interfaces:**
- Consumes: Task 9 `useJobStore` (`job`, `progress`, `history`, `lastUndo`, `error`, `pending`, `resume`, `cancel`, `undoLast`, `loadHistory`, `exportReport`), `isRunning`, `Modal`; Task 8 `fillJobItems(job, 'failed')`.
- Produces: `<JobsPanel view onClose />`; Toolbar prop `onJobs(): void`; the wizard's Run now opens the `jobs` overlay.

- [ ] **Step 1: Write the failing tests**

`src/components/fill/JobsPanel.test.tsx`:

```tsx
import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import App from '@/App';
import { I18nProvider } from '@/i18n/I18nProvider';
import { setBackend } from '@/ipc/backend';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import {
  createMockBackend,
  MOCK_LIBRARIES,
  type MockOptions,
} from '@/ipc/mock/mockBackend';
import { mockClass, mockFailure } from '@/ipc/mock/mockJobs';
import { DEFAULT_REGION_ORDER } from '@/lib/fill';
import { useJobStore } from '@/stores/jobStore';
import { resetLibraryStore, useLibraryStore } from '@/stores/libraryStore';
import { stubLayout } from '@/test/layout';

async function running(opts: MockOptions = {}, locale: 'en' | 'es' = 'en') {
  const backend = createMockBackend({
    delayMs: 0,
    now: FIXTURE_NOW,
    pauseAfter: null,
    ...opts,
  });
  setBackend(backend);
  resetLibraryStore();
  useJobStore.getState().reset();
  await useLibraryStore.getState().open(MOCK_LIBRARIES[1].path);
  render(
    <I18nProvider locale={locale}>
      <App />
    </I18nProvider>,
  );
  const view = useLibraryStore.getState().view!;
  await useJobStore.getState().start(view.generation, {
    entry_ids: null,
    media: ['cover'],
    strategy: 'rom-region',
    regions: [...DEFAULT_REGION_ORDER],
    overwrite: 'never',
  });
  await waitFor(() => expect(useJobStore.getState().job?.state).toBe('planned'));
  await act(() => useJobStore.getState().run());
  return { backend, view };
}

async function openPanel(name = 'Jobs') {
  await userEvent.click(screen.getByRole('button', { name: new RegExp(`^${name}`) }));
  return screen.findByRole('dialog', { name });
}

describe('Jobs panel', () => {
  let restore: () => void;
  beforeEach(() => {
    restore = stubLayout(1280, 800);
  });
  afterEach(() => {
    restore();
    setBackend(null);
  });

  it('shows_the_pause_reason_and_resumes', async () => {
    await running({ pauseAfter: 3 });
    await waitFor(() => expect(useJobStore.getState().job?.state).toBe('paused'));
    const dialog = await openPanel();
    const banner = within(dialog).getByRole('alert');
    expect(banner).toHaveTextContent(
      'Paused: the art server is limiting requests.',
    );
    await userEvent.click(within(banner).getByRole('button', { name: 'Resume' }));
    await waitFor(() => expect(useJobStore.getState().job?.state).toBe('done'));
    expect(await within(dialog).findByText('Finished')).toBeInTheDocument();
  });

  it('lists per-item errors with their reasons', async () => {
    const { view } = await running();
    await waitFor(() => expect(useJobStore.getState().job?.state).toBe('done'));
    const dialog = await openPanel();
    const failing = view.entries.filter(
      (e) =>
        e.art.cover === 'missing' &&
        mockClass(e.id, 'cover') === 'auto' &&
        mockFailure(e.id) !== null,
    );
    expect(failing.length).toBeGreaterThan(0);
    const list = await within(dialog).findByRole('list', {
      name: `Problems (${failing.length})`,
    });
    expect(within(list).getAllByRole('listitem')).toHaveLength(failing.length);
    const first = failing[0]!;
    const reason = {
      'no-art': 'No art found',
      network: 'Network error',
      decode: 'Not a usable image',
      'name-invalid': 'Name not valid on SD cards',
    }[mockFailure(first.id)!]!;
    expect(within(list).getAllByText(new RegExp(reason)).length).toBeGreaterThan(0);
  });

  it('undo asks for confirmation and restores the library', async () => {
    const { view } = await running();
    await waitFor(() => expect(useJobStore.getState().job?.state).toBe('done'));
    const missing = view.entries.filter((e) => e.art.cover === 'missing').length;
    const dialog = await openPanel();
    await userEvent.click(
      within(dialog).getByRole('button', { name: 'Undo last job' }),
    );
    expect(within(dialog).getByText(/Undo the last job\?/)).toBeInTheDocument();
    await userEvent.click(within(dialog).getByRole('button', { name: 'Undo' }));
    expect(await within(dialog).findByText(/^Undone: /)).toBeInTheDocument();
    expect(await screen.findByText(`${missing} without cover`)).toBeInTheDocument();
    expect(within(dialog).getByText('Undone')).toBeInTheDocument();
  });

  it('undo_reports_kept_files', async () => {
    const { backend } = await running();
    await waitFor(() => expect(useJobStore.getState().job?.state).toBe('done'));
    vi.spyOn(backend, 'undoLastJob').mockResolvedValue({
      job: 'mock-0001',
      removed: 2,
      restored: 1,
      kept: 1,
      entries: [],
    });
    const dialog = await openPanel();
    await userEvent.click(
      within(dialog).getByRole('button', { name: 'Undo last job' }),
    );
    await userEvent.click(within(dialog).getByRole('button', { name: 'Undo' }));
    expect(
      await within(dialog).findByText(
        'Undone: 2 removed, 1 restored, 1 kept because you changed them.',
      ),
    ).toBeInTheDocument();
  });

  it('cancels a running job', async () => {
    await running({ delayMs: 40 });
    const dialog = await openPanel();
    await userEvent.click(within(dialog).getByRole('button', { name: 'Cancel' }));
    await waitFor(() =>
      expect(useJobStore.getState().job?.state).toBe('cancelled'),
    );
    expect(await within(dialog).findByText('Cancelled')).toBeInTheDocument();
  });

  it('keeps a history of jobs', async () => {
    await running();
    await waitFor(() => expect(useJobStore.getState().job?.state).toBe('done'));
    const dialog = await openPanel();
    const history = await within(dialog).findByRole('list', { name: 'History' });
    expect(within(history).getAllByRole('listitem')).toHaveLength(1);
    expect(within(history).getByText('Complete')).toBeInTheDocument();
  });

  it('speaks Spanish', async () => {
    await running({}, 'es');
    await waitFor(() => expect(useJobStore.getState().job?.state).toBe('done'));
    const dialog = await openPanel('Trabajos');
    expect(
      within(dialog).getByRole('button', { name: 'Deshacer el último trabajo' }),
    ).toBeInTheDocument();
  });
});
```

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && pnpm test -- src/components/fill/JobsPanel.test.tsx`
Expected: FAIL — no Jobs button, no panel.

- [ ] **Step 2: The panel**

`src/components/fill/JobsPanel.tsx`:

```tsx
import { Fragment, useContext, useEffect, useMemo, useState } from 'react';

import { Modal } from '@/components/Modal';
import { LocaleContext } from '@/i18n/core';
import { useT } from '@/i18n/useT';
import { getBackend } from '@/ipc/backend';
import { errorKey } from '@/ipc/errors';
import type { ExportFormat, JobItemView, JobTotals, LibraryView } from '@/ipc/types';
import { isRunning, useJobStore } from '@/stores/jobStore';

const TOTALS: readonly (keyof JobTotals)[] = [
  'filled',
  'replaced',
  'skipped',
  'no_art',
  'failed',
];

export function JobsPanel({
  view,
  onClose,
}: {
  view: LibraryView;
  onClose(): void;
}) {
  const t = useT();
  const locale = useContext(LocaleContext);
  const n = useMemo(() => new Intl.NumberFormat(locale), [locale]);
  const dates = useMemo(
    () => new Intl.DateTimeFormat(locale, { dateStyle: 'medium', timeStyle: 'short' }),
    [locale],
  );
  const job = useJobStore((s) => s.job);
  const progress = useJobStore((s) => s.progress);
  const history = useJobStore((s) => s.history);
  const lastUndo = useJobStore((s) => s.lastUndo);
  const error = useJobStore((s) => s.error);
  const pending = useJobStore((s) => s.pending);
  const resume = useJobStore((s) => s.resume);
  const cancel = useJobStore((s) => s.cancel);
  const undoLast = useJobStore((s) => s.undoLast);
  const loadHistory = useJobStore((s) => s.loadHistory);
  const exportReport = useJobStore((s) => s.exportReport);
  const [confirming, setConfirming] = useState(false);
  const [failed, setFailed] = useState<{ job: number; items: JobItemView[] } | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const running = isRunning(job);
  const jobId = job?.id ?? null;
  const state = job?.state ?? null;
  const finished = state === 'done' || state === 'paused' || state === 'cancelled';

  useEffect(() => {
    void loadHistory();
  }, [loadHistory, state]);

  useEffect(() => {
    if (jobId === null || !finished) return;
    let alive = true;
    void getBackend()
      .then((b) => b.fillJobItems(jobId, 'failed'))
      .then(
        (items) => alive && setFailed({ job: jobId, items }),
        () => alive && setFailed({ job: jobId, items: [] }),
      );
    return () => {
      alive = false;
    };
  }, [jobId, finished]);

  const errors = finished && failed?.job === jobId ? failed.items : [];
  const p = progress && job && progress.job === job.id ? progress : null;
  // Rust refuses while paused and only undoes journals of the open library.
  const canUndo =
    !running &&
    job?.state !== 'paused' &&
    history.some((h) => !h.undone && h.root === view.root);

  async function exportJob(format: ExportFormat) {
    const name = await exportReport(format);
    if (name) setSaved(name);
  }

  async function undo() {
    setConfirming(false);
    await undoLast(view.generation);
  }

  return (
    <Modal labelledBy="jobs-title" onClose={onClose} className="jobs">
      <h2 id="jobs-title" className="section-title">
        {t('jobs.title')}
      </h2>
      {job ? (
        <section aria-labelledby="jobs-current">
          <h3 id="jobs-current" className="section-title">
            {t('jobs.current')}
          </h3>
          <p>{t(`jobs.state.${job.state}`)}</p>
          {running && (
            <div className="progress-row">
              <progress
                max={p?.total || 1}
                value={p?.done ?? 0}
                aria-label={t('jobs.progressLabel')}
              />
              {p && (
                <p>
                  {t('jobs.progress', {
                    phase: t(`jobs.phase.${p.phase}`),
                    done: n.format(Math.min(p.done + 1, p.total)),
                    total: n.format(p.total),
                  })}
                </p>
              )}
              {p?.label ? (
                <p className="hint truncate" title={p.label}>
                  {p.label}
                </p>
              ) : null}
              <button
                type="button"
                className="ghost-button"
                disabled={pending}
                onClick={() => void cancel()}
              >
                {t('jobs.cancel')}
              </button>
            </div>
          )}
          {job.state === 'paused' && job.paused && (
            <div role="alert" className="pause-banner">
              <p>{t(`jobs.pause.${job.paused.kind}`)}</p>
              <p className="hint truncate" title={job.paused.detail}>
                {job.paused.detail}
              </p>
              <div className="modal-actions">
                <button
                  type="button"
                  className="ghost-button"
                  disabled={pending}
                  onClick={() => void cancel()}
                >
                  {t('jobs.cancel')}
                </button>
                <button
                  type="button"
                  className="primary-button"
                  disabled={pending}
                  onClick={() => void resume()}
                >
                  {t('jobs.resume')}
                </button>
              </div>
            </div>
          )}
          {job.error && (
            <p role="alert" className="error-banner">
              {t('jobs.failed', { detail: job.error })}
            </p>
          )}
          {job.result && (
            <dl className="jobs-totals">
              {TOTALS.map((k) => (
                <Fragment key={k}>
                  <dt>{t(`jobs.totals.${k}`)}</dt>
                  <dd>{n.format(job.result?.[k] ?? 0)}</dd>
                </Fragment>
              ))}
            </dl>
          )}
          {errors.length > 0 && (
            <ul
              className="job-errors"
              aria-label={t('jobs.errors', { n: n.format(errors.length) })}
            >
              {errors.map((i) => (
                <li key={i.index}>
                  <span className="truncate" title={i.name}>
                    {i.name}
                  </span>
                  <span className="hint" title={i.outcome?.message ?? ''}>
                    {t(`art.media.${i.media}`)} ·{' '}
                    {t(`jobs.error.${i.outcome?.error ?? 'io'}`)}
                  </span>
                </li>
              ))}
            </ul>
          )}
          {job.result && (
            <div className="modal-actions">
              <button type="button" className="ghost-button" onClick={() => void exportJob('csv')}>
                {t('jobs.exportCsv')}
              </button>
              <button type="button" className="ghost-button" onClick={() => void exportJob('json')}>
                {t('jobs.exportJson')}
              </button>
            </div>
          )}
          {saved && <p className="hint">{t('reports.saved', { name: saved })}</p>}
        </section>
      ) : (
        <p>{t('jobs.none')}</p>
      )}
      <section aria-labelledby="jobs-undo">
        <h3 id="jobs-undo" className="section-title">
          {t('jobs.undoTitle')}
        </h3>
        {confirming ? (
          <div className="undo-confirm">
            <p>{t('jobs.undoConfirm')}</p>
            <div className="modal-actions">
              <button type="button" className="ghost-button" onClick={() => setConfirming(false)}>
                {t('jobs.undoNo')}
              </button>
              <button
                type="button"
                className="primary-button"
                disabled={pending}
                onClick={() => void undo()}
              >
                {t('jobs.undoYes')}
              </button>
            </div>
          </div>
        ) : (
          <button
            type="button"
            className="ghost-button"
            disabled={!canUndo || pending}
            onClick={() => setConfirming(true)}
          >
            {t('jobs.undo')}
          </button>
        )}
        {lastUndo && (
          <p role="status">
            {t('jobs.undone', {
              removed: n.format(lastUndo.removed),
              restored: n.format(lastUndo.restored),
              kept: n.format(lastUndo.kept),
            })}
          </p>
        )}
      </section>
      <section aria-labelledby="jobs-history">
        <h3 id="jobs-history" className="section-title">
          {t('jobs.history')}
        </h3>
        {history.length === 0 ? (
          <p className="hint">{t('jobs.noHistory')}</p>
        ) : (
          <ul className="history-list" aria-labelledby="jobs-history">
            {history.map((h) => (
              <li key={h.job}>
                <span>{dates.format(new Date(h.created_at))}</span>
                <span>{t('jobs.files', { n: n.format(h.entries) })}</span>
                <span className="hint">
                  {h.undone
                    ? t('jobs.undoneBadge')
                    : h.complete
                      ? t('jobs.complete')
                      : t('jobs.incomplete')}
                </span>
              </li>
            ))}
          </ul>
        )}
      </section>
      {error && (
        <p role="alert" className="error-banner">
          {t(errorKey(error.code), { detail: error.detail ?? '' })}
        </p>
      )}
      <div className="modal-actions">
        <button type="button" className="ghost-button" onClick={onClose}>
          {t('jobs.close')}
        </button>
      </div>
    </Modal>
  );
}
```

- [ ] **Step 3: Wire it in**

`src/components/library/Toolbar.tsx` — props gain `onJobs(): void`; `const job = useJobStore((s) => s.job);` and after the Fill button:

```tsx
      <button type="button" className="ghost-button" onClick={onJobs}>
        {isRunning(job)
          ? t('toolbar.jobsRunning')
          : job?.state === 'paused'
            ? t('toolbar.jobsPaused')
            : t('toolbar.jobs')}
      </button>
```

`src/components/fill/FillWizard.tsx` — `runJob` becomes `if (await run()) setOverlay('jobs');`.

`src/screens/LibraryScreen.tsx` — import `JobsPanel`; `<Toolbar … onJobs={() => setOverlay('jobs')} />`; after the review overlay:

```tsx
      {overlay === 'jobs' && <JobsPanel view={view} onClose={closeOverlay} />}
```

`locales/en/ui.json`: `toolbar` gains `"jobs": "Jobs", "jobsRunning": "Jobs (running)", "jobsPaused": "Jobs (paused)"`; the `jobs` object (Task 9) gains:

```json
    "title": "Jobs",
    "current": "Current job",
    "none": "No fill job yet.",
    "state": {
      "gathering": "Looking for art (dry run)…",
      "planned": "Dry run ready: nothing written yet.",
      "applying": "Writing art…",
      "paused": "Paused",
      "cancelled": "Cancelled",
      "done": "Finished",
      "failed": "Failed"
    },
    "phase": { "gather": "Looking for art", "apply": "Writing art" },
    "progress": "{phase}: {done} of {total}",
    "progressLabel": "Job progress",
    "cancel": "Cancel",
    "failed": "The job failed: {detail}",
    "totals": {
      "filled": "Filled",
      "replaced": "Replaced",
      "skipped": "Skipped",
      "no_art": "No art found",
      "failed": "Failed"
    },
    "errors": "Problems ({n})",
    "error": {
      "no-art": "No art found",
      "network": "Network error",
      "decode": "Not a usable image",
      "write-denied": "Write denied",
      "disk-full": "Disk full",
      "name-invalid": "Name not valid on SD cards",
      "outside-root": "Outside the library",
      "io": "Disk error"
    },
    "exportCsv": "Export report (CSV)",
    "exportJson": "Export report (JSON)",
    "undoTitle": "Undo",
    "undo": "Undo last job",
    "undoConfirm": "Undo the last job? Written images are removed and replaced ones restored; images you changed since are kept.",
    "undoYes": "Undo",
    "undoNo": "Keep",
    "undone": "Undone: {removed} removed, {restored} restored, {kept} kept because you changed them.",
    "history": "History",
    "noHistory": "No jobs yet.",
    "files": "{n} files",
    "complete": "Complete",
    "incomplete": "Stopped early",
    "undoneBadge": "Undone",
    "close": "Close"
```

and a new `"reports": { "saved": "Saved as {name}." }`.

`locales/es/ui.json`: `toolbar` gains `"jobs": "Trabajos", "jobsRunning": "Trabajos (en curso)", "jobsPaused": "Trabajos (en pausa)"`; `jobs` gains:

```json
    "title": "Trabajos",
    "current": "Trabajo actual",
    "none": "Todavía no hay ningún trabajo de relleno.",
    "state": {
      "gathering": "Buscando arte (simulación)…",
      "planned": "Simulación lista: todavía no se ha escrito nada.",
      "applying": "Escribiendo arte…",
      "paused": "En pausa",
      "cancelled": "Cancelado",
      "done": "Terminado",
      "failed": "Ha fallado"
    },
    "phase": { "gather": "Buscando arte", "apply": "Escribiendo arte" },
    "progress": "{phase}: {done} de {total}",
    "progressLabel": "Progreso del trabajo",
    "cancel": "Cancelar",
    "failed": "El trabajo ha fallado: {detail}",
    "totals": {
      "filled": "Rellenadas",
      "replaced": "Sustituidas",
      "skipped": "Omitidas",
      "no_art": "Sin arte",
      "failed": "Fallidas"
    },
    "errors": "Problemas ({n})",
    "error": {
      "no-art": "No se ha encontrado arte",
      "network": "Error de red",
      "decode": "No es una imagen válida",
      "write-denied": "Escritura denegada",
      "disk-full": "Disco lleno",
      "name-invalid": "Nombre no válido en tarjetas SD",
      "outside-root": "Fuera de la biblioteca",
      "io": "Error de disco"
    },
    "exportCsv": "Exportar informe (CSV)",
    "exportJson": "Exportar informe (JSON)",
    "undoTitle": "Deshacer",
    "undo": "Deshacer el último trabajo",
    "undoConfirm": "¿Deshacer el último trabajo? Se quitan las imágenes escritas y se restauran las sustituidas; las que hayas cambiado después se conservan.",
    "undoYes": "Deshacer",
    "undoNo": "Conservar",
    "undone": "Deshecho: {removed} quitadas, {restored} restauradas, {kept} conservadas porque las cambiaste.",
    "history": "Historial",
    "noHistory": "Todavía no hay trabajos.",
    "files": "{n} archivos",
    "complete": "Completo",
    "incomplete": "Detenido antes de terminar",
    "undoneBadge": "Deshecho",
    "close": "Cerrar"
```

and `"reports": { "saved": "Guardado como {name}." }`.

`src/styles/globals.css` (append):

```css
.jobs {
  width: min(44rem, calc(100vw - 2rem));
  max-height: calc(100vh - 4rem);
  overflow-y: auto;
}

.jobs section + section {
  margin-top: 1rem;
}

.jobs-totals {
  display: grid;
  grid-template-columns: max-content max-content;
  gap: 0.25rem 1rem;
  font-variant-numeric: tabular-nums;
}

.job-errors,
.history-list {
  display: grid;
  gap: 0.25rem;
  max-height: 12rem;
  margin: 0.5rem 0;
  padding: 0;
  overflow-y: auto;
  list-style: none;
}

.job-errors li,
.history-list li {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto auto;
  gap: 0.75rem;
  unicode-bidi: plaintext;
}
```

- [ ] **Step 4: Run the tests**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && pnpm test && pnpm lint && pnpm typecheck`
Expected: PASS (the wizard tests still pass: Run now opens the panel instead of closing).

- [ ] **Step 5: Gates and commit**

Run all gates. Then:

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && git add src locales && git commit -m "feat(ui): add the jobs panel with progress, pause and resume, errors, undo and history"
```

---
### Task 12: Drawer actions (change, from file, remove) and the reports panel (M4-04)

**Files:**
- Modify: `src/components/library/DetailDrawer.tsx` (module-level `DrawerActions`), `src/components/library/DetailDrawer.test.tsx`
- Create: `src/components/library/DrawerActions.test.tsx`
- Create: `src/lib/audit.ts`, `src/lib/audit.test.ts`, `src/components/reports/ReportsPanel.tsx`, `src/components/reports/ReportsPanel.test.tsx`
- Modify: `src/components/library/Toolbar.tsx` (Reports button), `src/screens/LibraryScreen.tsx` (overlay `reports`), `src/styles/globals.css`, `locales/{en,es}/ui.json`

**Interfaces:**
- Consumes: Task 8 `entryCandidates`, `entryCandidateArt` (through `CandidateImage` with `{ kind: 'entry', … }`), `setEntryArt`, `setEntryArtFromFile`, `removeEntryArt`, `exportAudit`; Task 9 `useLibraryStore.applyArtUpdates`, `useJobStore` (`job`, `exportReport`), `isRunning`, `Modal`.
- Produces:
  - `auditSummary(view: LibraryView): AuditSummary` with `AuditRow { folder: string; name: string; roms: number; missing: Partial<Record<Media, number>>; complete: number }` and `AuditSummary { media: Media[]; systems: AuditRow[]; total: AuditRow }` (systems without ROMs are left out)
  - `<ReportsPanel view onClose />`; Toolbar prop `onReports(): void`
  - Drawer: a media selector (when the layout has several), "Change…" lists candidates and writes the clicked one, "From file…" opens the picker, "Remove" asks first; all three are disabled while a job runs; "Search…" stays disabled (M4-05).

- [ ] **Step 1: Write the failing tests**

`src/lib/audit.test.ts`:

```ts
import { describe, expect, it } from 'vitest';

import { FIXTURE_NOW, generateLibrary } from '@/ipc/mock/fixture';

import { auditSummary } from './audit';

describe('audit summary', () => {
  it('counts ROMs, missing art and complete ROMs per system', () => {
    const view = generateLibrary({
      seed: 7,
      count: 240,
      root: '/mock/a',
      generation: 1,
      now: FIXTURE_NOW,
    });
    const s = auditSummary(view);
    expect(s.media).toEqual(view.profile.media);
    expect(s.total.roms).toBe(view.entries.length);
    expect(s.systems.reduce((n, r) => n + r.roms, 0)).toBe(view.entries.length);
    const sys = view.systems.findIndex((x) => x.roms > 0);
    const mine = view.entries.filter((e) => e.system === sys);
    const row = s.systems.find((r) => r.folder === view.systems[sys]!.folder)!;
    expect(row.roms).toBe(mine.length);
    expect(row.missing.cover).toBe(mine.filter((e) => e.art.cover === 'missing').length);
    expect(row.complete).toBe(
      mine.filter((e) => view.profile.media.every((m) => e.art[m] !== 'missing')).length,
    );
    expect(s.systems.every((r) => r.roms > 0)).toBe(true);
  });
});
```

`src/components/library/DrawerActions.test.tsx`:

```tsx
import { act, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import App from '@/App';
import { I18nProvider } from '@/i18n/I18nProvider';
import { setBackend } from '@/ipc/backend';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import {
  createMockBackend,
  MOCK_LIBRARIES,
  type MockOptions,
} from '@/ipc/mock/mockBackend';
import type { EntryView } from '@/ipc/types';
import { useJobStore } from '@/stores/jobStore';
import { resetLibraryStore, useLibraryStore } from '@/stores/libraryStore';
import { useSelectionStore } from '@/stores/selectionStore';
import { useViewStore } from '@/stores/viewStore';
import { stubLayout } from '@/test/layout';

async function setup(opts: MockOptions = {}) {
  setBackend(createMockBackend({ delayMs: 0, now: FIXTURE_NOW, ...opts }));
  resetLibraryStore();
  useJobStore.getState().reset();
  await useLibraryStore.getState().open(MOCK_LIBRARIES[1].path);
  render(
    <I18nProvider locale="en">
      <App />
    </I18nProvider>,
  );
  return useLibraryStore.getState().view!;
}

function show(entry: EntryView) {
  act(() => {
    useSelectionStore.getState().select(entry.id);
    useViewStore.getState().openDrawer();
  });
  return screen.getByRole('complementary', { name: entry.name });
}

const cover = (id: number) =>
  useLibraryStore.getState().view!.entries[id]!.art.cover;

describe('Drawer art actions', () => {
  let restore: () => void;
  beforeEach(() => {
    restore = stubLayout(1280, 800);
  });
  afterEach(() => {
    restore();
    setBackend(null);
  });

  it('changes the art by picking a candidate', async () => {
    const view = await setup();
    const entry = view.entries.find((e) => e.art.cover === 'missing')!;
    const drawer = show(entry);
    await userEvent.click(within(drawer).getByRole('button', { name: 'Change…' }));
    const group = await within(drawer).findByRole('group', { name: 'Candidates' });
    const buttons = within(group).getAllByRole('button');
    expect(buttons).toHaveLength(3);
    await userEvent.click(buttons[1]!);
    expect(
      await within(drawer).findByText('Art updated. Undo it from Jobs if needed.'),
    ).toBeInTheDocument();
    expect(cover(entry.id)).toBe('present');
    expect(
      within(drawer).queryByRole('group', { name: 'Candidates' }),
    ).not.toBeInTheDocument();
  });

  it('sets the art from a file and removes it after confirmation', async () => {
    const view = await setup();
    const entry = view.entries.find((e) => e.art.cover === 'missing')!;
    const drawer = show(entry);
    expect(within(drawer).getByRole('button', { name: 'Remove' })).toBeDisabled();
    await userEvent.click(within(drawer).getByRole('button', { name: 'From file…' }));
    expect(await within(drawer).findByText(/^Art updated/)).toBeInTheDocument();
    expect(cover(entry.id)).toBe('present');
    await userEvent.click(within(drawer).getByRole('button', { name: 'Remove' }));
    expect(within(drawer).getByText(/It is backed up/)).toBeInTheDocument();
    await userEvent.click(within(drawer).getByRole('button', { name: 'Remove image' }));
    await within(drawer).findByText(/^Art updated/);
    expect(cover(entry.id)).toBe('missing');
  });

  it('does nothing when the file picker is cancelled', async () => {
    const view = await setup({ pick: 'cancel' });
    const entry = view.entries.find((e) => e.art.cover === 'missing')!;
    const drawer = show(entry);
    await userEvent.click(within(drawer).getByRole('button', { name: 'From file…' }));
    expect(cover(entry.id)).toBe('missing');
    expect(within(drawer).queryByRole('alert')).not.toBeInTheDocument();
  });

  it('explains a file that is not an image', async () => {
    const view = await setup({ pick: 'not-image' });
    const drawer = show(view.entries[0]!);
    await userEvent.click(within(drawer).getByRole('button', { name: 'From file…' }));
    expect(await within(drawer).findByRole('alert')).toHaveTextContent(
      'This file is not a usable image: not an image',
    );
  });

  it('disables art changes while a job runs', async () => {
    const view = await setup();
    act(() =>
      useJobStore.setState({
        job: {
          id: 1,
          generation: view.generation,
          state: 'applying',
          progress: null,
          paused: null,
          plan: null,
          decided: 0,
          journal: null,
          result: null,
          error: null,
        },
      }),
    );
    const drawer = show(view.entries[0]!);
    for (const name of ['Change…', 'From file…', 'Remove']) {
      expect(within(drawer).getByRole('button', { name })).toBeDisabled();
    }
    expect(
      within(drawer).getByText(/A fill job is running/),
    ).toBeInTheDocument();
  });
});
```

In `src/components/library/DetailDrawer.test.tsx` replace the test `reveals the file and keeps the M4 actions disabled` with:

```tsx
  it('reveals the file; searching by title stays for later', async () => {
    const view = await setup();
    const entry = view.entries[5]!;
    const drawer = show(entry);
    expect(within(drawer).getByRole('button', { name: 'Search…' })).toBeDisabled();
    expect(within(drawer).getByRole('button', { name: 'Change…' })).toBeEnabled();
    expect(within(drawer).getByRole('button', { name: 'From file…' })).toBeEnabled();
    expect(within(drawer).getByRole('button', { name: 'Remove' })).toHaveProperty(
      'disabled',
      entry.art.cover !== 'present',
    );
    await userEvent.click(
      within(drawer).getByRole('button', {
        name: /Reveal in Finder|Show in Explorer|Show in folder/,
      }),
    );
    expect(backend.revealed).toEqual([entry.id]);
  });
```

and in its `speaks Spanish` change `.toBeDisabled()` to `.toBeEnabled()`.

`src/components/reports/ReportsPanel.test.tsx`:

```tsx
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { act } from 'react';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import App from '@/App';
import { I18nProvider } from '@/i18n/I18nProvider';
import { setBackend } from '@/ipc/backend';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import {
  createMockBackend,
  MOCK_LIBRARIES,
  type MockBackend,
} from '@/ipc/mock/mockBackend';
import { auditSummary } from '@/lib/audit';
import { DEFAULT_REGION_ORDER } from '@/lib/fill';
import { useJobStore } from '@/stores/jobStore';
import { resetLibraryStore, useLibraryStore } from '@/stores/libraryStore';
import { stubLayout } from '@/test/layout';

let backend: MockBackend;

async function setup(locale: 'en' | 'es' = 'en') {
  backend = createMockBackend({ delayMs: 0, now: FIXTURE_NOW, pauseAfter: null });
  setBackend(backend);
  resetLibraryStore();
  useJobStore.getState().reset();
  await useLibraryStore.getState().open(MOCK_LIBRARIES[1].path);
  render(
    <I18nProvider locale={locale}>
      <App />
    </I18nProvider>,
  );
  return useLibraryStore.getState().view!;
}

const escapeRe = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

describe('Reports panel', () => {
  let restore: () => void;
  beforeEach(() => {
    restore = stubLayout(1280, 800);
  });
  afterEach(() => {
    restore();
    setBackend(null);
  });

  it('summarises the audit per system and exports it', async () => {
    const view = await setup();
    await userEvent.click(screen.getByRole('button', { name: 'Reports' }));
    const dialog = await screen.findByRole('dialog', { name: 'Reports' });
    const table = within(dialog).getByRole('table', { name: 'Art audit' });
    const first = auditSummary(view).systems[0]!;
    expect(
      within(table).getByRole('row', {
        name: new RegExp(`^${escapeRe(first.name)} ${first.roms} `),
      }),
    ).toBeInTheDocument();
    expect(
      within(table).getByRole('row', {
        name: new RegExp(`^Total ${view.entries.length} `),
      }),
    ).toBeInTheDocument();
    expect(within(dialog).getByText('No job has run yet.')).toBeInTheDocument();
    await userEvent.click(
      within(dialog).getByRole('button', { name: 'Export audit (CSV)' }),
    );
    expect(
      await within(dialog).findByText('Saved as coverly-audit.csv.'),
    ).toBeInTheDocument();
    expect(backend.exports[0]?.name).toBe('coverly-audit.csv');
  });

  it('exports the last job report', async () => {
    const view = await setup();
    await useJobStore.getState().start(view.generation, {
      entry_ids: null,
      media: ['cover'],
      strategy: 'rom-region',
      regions: [...DEFAULT_REGION_ORDER],
      overwrite: 'never',
    });
    await waitFor(() => expect(useJobStore.getState().job?.state).toBe('planned'));
    await act(() => useJobStore.getState().run());
    await waitFor(() => expect(useJobStore.getState().job?.state).toBe('done'));
    await userEvent.click(screen.getByRole('button', { name: 'Reports' }));
    const dialog = await screen.findByRole('dialog', { name: 'Reports' });
    await userEvent.click(
      within(dialog).getByRole('button', { name: 'Export report (JSON)' }),
    );
    const id = useJobStore.getState().job!.id;
    expect(
      await within(dialog).findByText(`Saved as coverly-job-${id}.json.`),
    ).toBeInTheDocument();
  });

  it('speaks Spanish', async () => {
    await setup('es');
    await userEvent.click(screen.getByRole('button', { name: 'Informes' }));
    const dialog = await screen.findByRole('dialog', { name: 'Informes' });
    expect(
      within(dialog).getByRole('button', { name: 'Exportar auditoría (CSV)' }),
    ).toBeInTheDocument();
  });
});
```

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && pnpm test -- src/lib/audit.test.ts src/components/library src/components/reports`
Expected: FAIL — `audit.ts`, `ReportsPanel`, the drawer actions do not exist.

- [ ] **Step 2: Audit summary**

`src/lib/audit.ts`:

```ts
import type { LibraryView, Media } from '@/ipc/types';

export interface AuditRow {
  folder: string;
  name: string;
  roms: number;
  /** ROMs missing each media the layout can store. */
  missing: Partial<Record<Media, number>>;
  /** ROMs missing none of them. */
  complete: number;
}

export interface AuditSummary {
  media: Media[];
  systems: AuditRow[];
  total: AuditRow;
}

/** The audit per system of an open library (spec §8 Reports). */
export function auditSummary(view: LibraryView): AuditSummary {
  const media = [...view.profile.media];
  const zero = () =>
    Object.fromEntries(media.map((m) => [m, 0])) as Partial<Record<Media, number>>;
  const rows: AuditRow[] = view.systems.map((s) => ({
    folder: s.folder,
    name: s.name,
    roms: 0,
    missing: zero(),
    complete: 0,
  }));
  const total: AuditRow = { folder: '', name: '', roms: 0, missing: zero(), complete: 0 };
  for (const e of view.entries) {
    const row = rows[e.system];
    if (!row) continue;
    let complete = true;
    for (const m of media) {
      if (e.art[m] === 'missing') {
        row.missing[m] = (row.missing[m] ?? 0) + 1;
        total.missing[m] = (total.missing[m] ?? 0) + 1;
        complete = false;
      }
    }
    row.roms += 1;
    total.roms += 1;
    if (complete) {
      row.complete += 1;
      total.complete += 1;
    }
  }
  return { media, systems: rows.filter((r) => r.roms > 0), total };
}
```

- [ ] **Step 3: Drawer actions**

In `src/components/library/DetailDrawer.tsx`:
- delete `LATER_ACTIONS` and the `drawer-later` hint;
- imports: `useState` (already), `type Backend` from `@/ipc/backend`, `CandidateImage` from `@/components/fill/CandidateImage`, `useJobStore, isRunning` from `@/stores/jobStore`, `useLibraryStore` from `@/stores/libraryStore`, types `CandidateView`, `EntryArtUpdate`, `Media`;
- add at module level:

```tsx
function DrawerActions({ view, entry }: { view: LibraryView; entry: EntryView }) {
  const t = useT();
  const job = useJobStore((s) => s.job);
  const applyArtUpdates = useLibraryStore((s) => s.applyArtUpdates);
  const writable = view.profile.media;
  const [media, setMedia] = useState<Media>(writable[0] ?? 'cover');
  const [candidates, setCandidates] = useState<CandidateView[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [confirmRemove, setConfirmRemove] = useState(false);
  const [updated, setUpdated] = useState(false);
  const [error, setError] = useState<BackendError | null>(null);
  const busy = isRunning(job);

  async function attempt(
    f: (b: Backend) => Promise<EntryArtUpdate[] | null>,
  ): Promise<boolean> {
    setError(null);
    setUpdated(false);
    try {
      const b = await getBackend();
      const updates = await f(b);
      if (!updates) return false;
      applyArtUpdates(view.generation, updates);
      setUpdated(true);
      return true;
    } catch (e) {
      setError(toBackendError(e));
      return false;
    }
  }

  async function listCandidates() {
    setError(null);
    setUpdated(false);
    setLoading(true);
    setCandidates(null);
    try {
      const b = await getBackend();
      setCandidates(await b.entryCandidates(view.generation, entry.id, media));
    } catch (e) {
      setError(toBackendError(e));
    } finally {
      setLoading(false);
    }
  }

  async function choose(candidate: number) {
    if (await attempt((b) => b.setEntryArt(view.generation, entry.id, media, candidate))) {
      setCandidates(null);
    }
  }

  async function remove() {
    setConfirmRemove(false);
    await attempt((b) => b.removeEntryArt(view.generation, entry.id, media));
  }

  return (
    <>
      {writable.length > 1 && (
        <label className="field-label">
          {t('drawer.mediaLabel')}{' '}
          <select
            name="drawer-media"
            className="select"
            value={media}
            onChange={(e) => {
              setMedia(e.target.value as Media);
              setCandidates(null);
              setConfirmRemove(false);
            }}
          >
            {writable.map((m) => (
              <option key={m} value={m}>
                {t(`art.media.${m}`)}
              </option>
            ))}
          </select>
        </label>
      )}
      <div className="drawer-actions">
        <button
          type="button"
          className="ghost-button"
          disabled={busy || loading}
          onClick={() => void listCandidates()}
        >
          {t('drawer.change')}
        </button>
        <button
          type="button"
          className="ghost-button"
          disabled
          aria-describedby="drawer-search-later"
        >
          {t('drawer.search')}
        </button>
        <button
          type="button"
          className="ghost-button"
          disabled={busy}
          onClick={() =>
            void attempt((b) => b.setEntryArtFromFile(view.generation, entry.id, media))
          }
        >
          {t('drawer.fromFile')}
        </button>
        <button
          type="button"
          className="ghost-button"
          disabled={busy || entry.art[media] !== 'present'}
          onClick={() => setConfirmRemove(true)}
        >
          {t('drawer.remove')}
        </button>
      </div>
      <p id="drawer-search-later" className="hint">
        {t('drawer.searchLater')}
      </p>
      {busy && <p className="hint">{t('drawer.busy')}</p>}
      {confirmRemove && (
        <div className="undo-confirm">
          <p>{t('drawer.removeConfirm')}</p>
          <div className="modal-actions">
            <button type="button" className="ghost-button" onClick={() => setConfirmRemove(false)}>
              {t('drawer.removeNo')}
            </button>
            <button type="button" className="primary-button" onClick={() => void remove()}>
              {t('drawer.removeYes')}
            </button>
          </div>
        </div>
      )}
      {loading && <p role="status">{t('drawer.loadingCandidates')}</p>}
      {candidates &&
        (candidates.length === 0 ? (
          <p>{t('drawer.noCandidates')}</p>
        ) : (
          <div
            role="group"
            aria-label={t('drawer.candidates')}
            className="candidate-grid drawer-candidates"
          >
            {candidates.map((c) => (
              <button
                key={c.index}
                type="button"
                className="candidate"
                onClick={() => void choose(c.index)}
              >
                <CandidateImage
                  source={{
                    kind: 'entry',
                    generation: view.generation,
                    entryId: entry.id,
                    media,
                    candidate: c.index,
                  }}
                  label={c.name}
                />
                <span className="truncate" title={c.name}>
                  {c.name}
                </span>
                <span className="hint">
                  {t('review.meta', {
                    key: c.index + 1,
                    source: c.source,
                    regions: c.regions.map((r) => t(`regions.${r}`)).join(', ') || '—',
                    score: Math.round(c.confidence * 100),
                  })}
                </span>
              </button>
            ))}
          </div>
        ))}
      {updated && (
        <p role="status" className="hint">
          {t('drawer.updated')}
        </p>
      )}
      {error && (
        <p role="alert" className="error-banner">
          {t(errorKey(error.code), { detail: error.detail ?? '' })}
        </p>
      )}
    </>
  );
}
```

- the actions section of `DetailDrawer` becomes:

```tsx
      <section className="drawer-section">
        <h3 className="section-title">{t('drawer.actions')}</h3>
        <DrawerActions key={entry.id} view={view} entry={entry} />
        <div className="drawer-actions">
          <button type="button" className="ghost-button" onClick={() => void reveal()}>
            {t(revealLabel())}
          </button>
        </div>
        {revealError && (
          <p role="alert" className="error-banner">
            {t(errorKey(revealError.code), { detail: revealError.detail ?? '' })}
          </p>
        )}
      </section>
```

- [ ] **Step 4: Reports panel**

`src/components/reports/ReportsPanel.tsx`:

```tsx
import { useContext, useMemo, useState } from 'react';

import { Modal } from '@/components/Modal';
import { LocaleContext } from '@/i18n/core';
import { useT } from '@/i18n/useT';
import { getBackend } from '@/ipc/backend';
import { errorKey, toBackendError, type BackendError } from '@/ipc/errors';
import type { ExportFormat, LibraryView } from '@/ipc/types';
import { auditSummary, type AuditRow } from '@/lib/audit';
import { useJobStore } from '@/stores/jobStore';

export function ReportsPanel({
  view,
  onClose,
}: {
  view: LibraryView;
  onClose(): void;
}) {
  const t = useT();
  const locale = useContext(LocaleContext);
  const n = useMemo(() => new Intl.NumberFormat(locale), [locale]);
  const job = useJobStore((s) => s.job);
  const exportReport = useJobStore((s) => s.exportReport);
  const summary = useMemo(() => auditSummary(view), [view]);
  const [saved, setSaved] = useState<string | null>(null);
  const [error, setError] = useState<BackendError | null>(null);

  async function exportAudit(format: ExportFormat) {
    setError(null);
    try {
      const b = await getBackend();
      const name = await b.exportAudit(view.generation, format);
      if (name) setSaved(name);
    } catch (e) {
      setError(toBackendError(e));
    }
  }

  async function exportJob(format: ExportFormat) {
    const name = await exportReport(format);
    if (name) setSaved(name);
  }

  const cells = (r: AuditRow) => (
    <>
      <td>{n.format(r.roms)}</td>
      {summary.media.map((m) => (
        <td key={m}>{n.format(r.missing[m] ?? 0)}</td>
      ))}
      <td>{n.format(r.complete)}</td>
    </>
  );

  return (
    <Modal labelledBy="reports-title" onClose={onClose} className="reports">
      <h2 id="reports-title" className="section-title">
        {t('reports.title')}
      </h2>
      <h3 className="section-title">{t('reports.audit')}</h3>
      <table className="summary-table">
        <caption className="sr-only">{t('reports.audit')}</caption>
        <thead>
          <tr>
            <th scope="col">{t('reports.system')}</th>
            <th scope="col">{t('reports.roms')}</th>
            {summary.media.map((m) => (
              <th key={m} scope="col">
                {t(`art.none.${m}`)}
              </th>
            ))}
            <th scope="col">{t('reports.complete')}</th>
          </tr>
        </thead>
        <tbody>
          {summary.systems.map((r) => (
            <tr key={r.folder}>
              <th scope="row" title={r.folder}>
                {r.name}
              </th>
              {cells(r)}
            </tr>
          ))}
          <tr>
            <th scope="row">{t('reports.total')}</th>
            {cells(summary.total)}
          </tr>
        </tbody>
      </table>
      <div className="modal-actions">
        <button type="button" className="ghost-button" onClick={() => void exportAudit('csv')}>
          {t('reports.exportAuditCsv')}
        </button>
        <button type="button" className="ghost-button" onClick={() => void exportAudit('json')}>
          {t('reports.exportAuditJson')}
        </button>
      </div>
      <h3 className="section-title">{t('reports.jobReport')}</h3>
      {job?.result ? (
        <div className="modal-actions">
          <button type="button" className="ghost-button" onClick={() => void exportJob('csv')}>
            {t('jobs.exportCsv')}
          </button>
          <button type="button" className="ghost-button" onClick={() => void exportJob('json')}>
            {t('jobs.exportJson')}
          </button>
        </div>
      ) : (
        <p className="hint">{t('reports.noJob')}</p>
      )}
      {saved && <p role="status">{t('reports.saved', { name: saved })}</p>}
      {error && (
        <p role="alert" className="error-banner">
          {t(errorKey(error.code), { detail: error.detail ?? '' })}
        </p>
      )}
      <div className="modal-actions">
        <button type="button" className="ghost-button" onClick={onClose}>
          {t('reports.close')}
        </button>
      </div>
    </Modal>
  );
}
```

- [ ] **Step 5: Wire it in, strings, styles**

`src/components/library/Toolbar.tsx` — props gain `onReports(): void`; after the Jobs button:

```tsx
      <button type="button" className="ghost-button" onClick={onReports}>
        {t('toolbar.reports')}
      </button>
```

`src/screens/LibraryScreen.tsx` — import `ReportsPanel`; `<Toolbar … onReports={() => setOverlay('reports')} />`; after the jobs overlay:

```tsx
      {overlay === 'reports' && <ReportsPanel view={view} onClose={closeOverlay} />}
```

`locales/en/ui.json`: `toolbar.reports` = `"Reports"`; in `drawer` remove `laterHint` and add

```json
    "searchLater": "Searching by title arrives in a later version.",
    "mediaLabel": "Art to change",
    "candidates": "Candidates",
    "loadingCandidates": "Looking for images…",
    "noCandidates": "No images found for this ROM.",
    "updated": "Art updated. Undo it from Jobs if needed.",
    "removeConfirm": "Remove this image? It is backed up and Undo brings it back.",
    "removeYes": "Remove image",
    "removeNo": "Keep",
    "busy": "A fill job is running: art changes wait until it ends."
```

and `reports` (with `saved` from Task 11) gains

```json
    "title": "Reports",
    "audit": "Art audit",
    "system": "System",
    "roms": "ROMs",
    "complete": "Complete",
    "total": "Total",
    "exportAuditCsv": "Export audit (CSV)",
    "exportAuditJson": "Export audit (JSON)",
    "jobReport": "Last job report",
    "noJob": "No job has run yet.",
    "close": "Close"
```

`locales/es/ui.json`: `toolbar.reports` = `"Informes"`; in `drawer` remove `laterHint` and add

```json
    "searchLater": "Buscar por título llegará en una versión posterior.",
    "mediaLabel": "Arte que cambiar",
    "candidates": "Candidatas",
    "loadingCandidates": "Buscando imágenes…",
    "noCandidates": "No se han encontrado imágenes para esta ROM.",
    "updated": "Arte actualizado. Puedes deshacerlo desde Trabajos.",
    "removeConfirm": "¿Quitar esta imagen? Se guarda una copia y Deshacer la recupera.",
    "removeYes": "Quitar imagen",
    "removeNo": "Conservar",
    "busy": "Hay un trabajo de relleno en curso: los cambios de arte esperan a que termine."
```

and `reports` gains

```json
    "title": "Informes",
    "audit": "Auditoría de arte",
    "system": "Sistema",
    "roms": "ROMs",
    "complete": "Completas",
    "total": "Total",
    "exportAuditCsv": "Exportar auditoría (CSV)",
    "exportAuditJson": "Exportar auditoría (JSON)",
    "jobReport": "Informe del último trabajo",
    "noJob": "Todavía no se ha ejecutado ningún trabajo.",
    "close": "Cerrar"
```

`src/styles/globals.css` (append):

```css
.reports {
  width: min(48rem, calc(100vw - 2rem));
  max-height: calc(100vh - 4rem);
  overflow-y: auto;
}

.drawer-candidates {
  grid-template-columns: repeat(auto-fill, minmax(6.5rem, 1fr));
  margin-top: 0.5rem;
}

.undo-confirm {
  margin-top: 0.5rem;
  padding: 0.75rem;
  border-radius: 0.5rem;
  background: rgb(127 127 127 / 0.12);
}
```

- [ ] **Step 6: Run the tests**

Run: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && pnpm test && pnpm lint && pnpm typecheck`
Expected: PASS.

- [ ] **Step 7: Gates and commit**

Run all gates. Then:

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && git add src locales && git commit -m "feat(ui): add drawer art actions and the reports panel with exports"
```

---
### Task 13: End-to-end — Chrome (mock), a real fill through the app's job path on a copy, docs and tracker

**Files:**
- Create: `src-tauri/tests/real_fill.rs` (ignored test, run by hand)
- Modify: `docs/04-architecture/desktop-app.md`, `docs/04-architecture/fill-pipeline.md`, `docs/TRACKER.md`
- Evidence (not versioned): `$SC/m4-e2e/` with `$SC` = `/private/tmp/claude-501/-Users-miguelangelesparzacalero-Documents-GitHub/35b7c020-f2ae-412f-b757-99c24a9a498b/scratchpad`

**Interfaces:**
- Consumes: everything above; `AppState::{open, start_fill, jobs, fill_config, undo_last_job, session}`, `common::fill::Recorder`, `JobEvents`, `probe_dimensions`, `sha1_hex`.
- Produces: the M4 evidence and documentation; no new production code unless the E2E finds a bug (then: failing test first, fix, its own `fix(…)` commit, and a line in «Evidencia M4»).

- [ ] **Step 1: The real-run test (ignored)**

`src-tauri/tests/real_fill.rs`:

```rust
//! The M4 exit check through the app's job path (not the GUI) on a COPY of
//! real systems, with the real art servers. Run by hand:
//!
//! ```text
//! COVERLY_FILL_COPY=$SC/m4-copy COVERLY_FILL_WORK=$SC/m4-work \
//!   cargo test -p coverly-roms --release --test real_fill -- --ignored --nocapture
//! ```
//!
//! Writes only inside the copy and the work folder.

mod common;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use common::fill::Recorder;
use coverly_core::image::probe_dimensions;
use coverly_core::matching::StrategyKind;
use coverly_core::profiles::Media;
use coverly_core::settings::LIBRARY_SETTINGS_FILE;
use coverly_core::systems::SystemDb;
use coverly_core::write::{sha1_hex, OverwritePolicy};
use coverly_roms_lib::jobs::view::{
    ActionKind, FillRequest, ItemFilter, JobProgressEvent, JobState, JobView,
};
use coverly_roms_lib::jobs::{FillConfig, JobEvents};
use coverly_roms_lib::library::view::ArtMark;
use coverly_roms_lib::state::AppState;

struct Refreshing {
    state: Arc<AppState>,
    rec: Recorder,
}

impl JobEvents for Refreshing {
    fn progress(&self, event: JobProgressEvent) {
        self.rec.progress(event);
    }
    fn changed(&self, view: JobView) {
        self.rec.changed(view);
    }
    fn written(&self, generation: u64, ids: Vec<u32>) {
        let _ = self
            .state
            .refresh_entries(generation, &ids, &SystemDb::builtin());
        self.rec.written(generation, ids);
    }
}

/// SHA-1 of every file under `root`, by relative path.
fn tree(root: &Path) -> BTreeMap<PathBuf, String> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<PathBuf, String>) {
        for e in fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(root, &p, out);
            } else {
                let rel = p.strip_prefix(root).unwrap().to_path_buf();
                out.insert(rel, sha1_hex(&fs::read(&p).unwrap()));
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

fn wait(rec: &Recorder, state: JobState, limit: Duration) -> JobView {
    let start = Instant::now();
    loop {
        if let Some(v) = rec.changed.lock().unwrap().last() {
            if v.state == state {
                return v.clone();
            }
            assert!(
                !matches!(v.state, JobState::Paused | JobState::Failed),
                "the job stopped: {v:?}"
            );
        }
        assert!(start.elapsed() < limit, "timed out waiting for {state:?}");
        std::thread::sleep(Duration::from_millis(200));
    }
}

#[test]
#[ignore = "real network and a copy of real ROMs: run by hand (M4 Task 13)"]
fn fills_a_copy_auto_only_and_undo_restores_it() {
    let copy = PathBuf::from(std::env::var("COVERLY_FILL_COPY").expect("COVERLY_FILL_COPY"));
    let work = PathBuf::from(std::env::var("COVERLY_FILL_WORK").expect("COVERLY_FILL_WORK"));
    let home = PathBuf::from(std::env::var("HOME").unwrap_or_default());
    for forbidden in [PathBuf::from("/Volumes"), home.join("Documents/ROMS")] {
        assert!(
            !copy.starts_with(&forbidden) && !work.starts_with(&forbidden),
            "refusing to write under {}",
            forbidden.display()
        );
    }
    let before = tree(&copy);
    let st = Arc::new(AppState::new(
        work.join(LIBRARY_SETTINGS_FILE),
        FillConfig::new(work.join("cache"), work.join("data")),
    ));
    let view = st
        .open(&copy, None, &SystemDb::builtin(), &mut |_| {})
        .unwrap();
    let missing_before = view
        .entries
        .iter()
        .filter(|e| e.art.cover == ArtMark::Missing)
        .count();
    println!(
        "opened {} ({}): {} ROMs, {missing_before} without cover",
        view.root,
        view.profile.id,
        view.entries.len()
    );
    let ev = Arc::new(Refreshing {
        state: st.clone(),
        rec: Recorder::default(),
    });

    let t0 = Instant::now();
    let request = FillRequest {
        entry_ids: None,
        media: vec![Media::Cover],
        strategy: StrategyKind::RomRegion,
        regions: ["rom", "world", "usa", "europe", "japan"]
            .map(String::from)
            .to_vec(),
        overwrite: OverwritePolicy::Never,
    };
    let id = st.start_fill(view.generation, &request, ev.clone()).unwrap().id;
    let planned = wait(&ev.rec, JobState::Planned, Duration::from_secs(900));
    let plan = planned.plan.clone().unwrap();
    println!(
        "dry run {:.1} s: auto {} review {} none {}",
        t0.elapsed().as_secs_f64(),
        plan.totals.auto,
        plan.totals.review,
        plan.totals.none
    );
    for s in &plan.per_system {
        println!("  {}: auto {} review {} none {}", s.folder, s.auto, s.review, s.none);
    }
    assert_eq!(tree(&copy), before, "the dry run writes nothing");
    for item in st.jobs().items(id, ItemFilter::All).unwrap() {
        if item.name.ends_with("Final Fantasy IV") || item.name.ends_with("Contra Advance") {
            println!(
                "  M2-11 {}: {:?} {:?} -> {:?}",
                item.name,
                item.action,
                item.note,
                item.candidates.first().map(|c| c.name.clone())
            );
            assert_ne!(item.action, ActionKind::Auto, "{} must not be auto", item.name);
        }
    }

    let t1 = Instant::now();
    st.jobs().run(st.fill_config(), id, ev.clone()).unwrap();
    let done = wait(&ev.rec, JobState::Done, Duration::from_secs(1800));
    let result = done.result.unwrap();
    println!(
        "apply (auto only) {:.1} s: filled {} replaced {} skipped {} no-art {} failed {} journal {:?}",
        t1.elapsed().as_secs_f64(),
        result.filled,
        result.replaced,
        result.skipped,
        result.no_art,
        result.failed,
        done.journal
    );
    let after = tree(&copy);
    let written: Vec<&PathBuf> = after.keys().filter(|p| !before.contains_key(*p)).collect();
    assert_eq!(written.len(), result.filled, "one new file per filled item");
    for p in &written {
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        assert!(!name.starts_with("._") && !name.starts_with(".coverly-"), "{name}");
        let bytes = fs::read(copy.join(p)).unwrap();
        assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"), "{} is not a PNG", p.display());
        let (w, h) = probe_dimensions(&bytes).unwrap();
        assert!(w <= 250 && h <= 360, "{} is {w}x{h}", p.display());
    }
    let session = st.session(view.generation).unwrap();
    let missing_after = session
        .entries
        .iter()
        .filter(|e| !e.art.contains_key(&Media::Cover))
        .count();
    println!("without cover: {missing_before} -> {missing_after}");
    assert_eq!(missing_after, missing_before - result.filled);

    let undo = st.undo_last_job(view.generation).unwrap();
    println!(
        "undo {}: removed {} restored {} kept {}",
        undo.job, undo.removed, undo.restored, undo.kept
    );
    assert_eq!(tree(&copy), before, "undo restores the copy byte for byte");
}
```

Run (no network, it is ignored): `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && cargo test -p coverly-roms --test real_fill`
Expected: `1 ignored`. Commit:

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && git add src-tauri/tests/real_fill.rs && git commit -m "test(app): add the by-hand real fill through the job path"
```

- [ ] **Step 2: Real run on a copy of two systems (read-only source)**

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && SC=/private/tmp/claude-501/-Users-miguelangelesparzacalero-Documents-GitHub/35b7c020-f2ae-412f-b757-99c24a9a498b/scratchpad && rm -rf "$SC/m4-copy" "$SC/m4-work" && mkdir -p "$SC/m4-copy/anbernic" "$SC/m4-copy/ROMS/GBA" "$SC/m4-copy/ROMS/FC" "$SC/m4-work" "$SC/m4-e2e" && touch "$SC/m4-snapshot" && sleep 1 \
 && ls "$HOME/Documents/ROMS/GBA" | grep -v '^Imgs$' | head -60 | while IFS= read -r f; do cp -X "$HOME/Documents/ROMS/GBA/$f" "$SC/m4-copy/ROMS/GBA/"; done \
 && cp -RX "$HOME/Documents/ROMS/GBA/Imgs" "$SC/m4-copy/ROMS/GBA/" \
 && ls "$HOME/Documents/ROMS/FC" | grep -v '^Imgs$' | head -150 | while IFS= read -r f; do cp -X "$HOME/Documents/ROMS/FC/$f" "$SC/m4-copy/ROMS/FC/"; done \
 && cp -RX "$HOME/Documents/ROMS/FC/Imgs" "$SC/m4-copy/ROMS/FC/" \
 && du -sh "$SC/m4-copy" && find "$SC/m4-copy" -type f | wc -l
```

(60 GBA files include `054 Contra Advance.gba` and `057 Final Fantasy IV.gba`, the M2-11 cases.) Then:

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && SC=/private/tmp/claude-501/-Users-miguelangelesparzacalero-Documents-GitHub/35b7c020-f2ae-412f-b757-99c24a9a498b/scratchpad && COVERLY_FILL_COPY="$SC/m4-copy" COVERLY_FILL_WORK="$SC/m4-work" cargo test -p coverly-roms --release --test real_fill -- --ignored --nocapture 2>&1 | tee "$SC/m4-e2e/real-fill.log"
```

Expected: PASS, with the dry-run counts per system, `M2-11 … Review Some(UnreleasedVariant)` lines for both real cases, every written file a PNG ≤ 250×360, "without cover: X -> X − filled", and the undo leaving the copy identical. Then prove the source was not touched:

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && SC=/private/tmp/claude-501/-Users-miguelangelesparzacalero-Documents-GitHub/35b7c020-f2ae-412f-b757-99c24a9a498b/scratchpad && find "$HOME/Documents/ROMS/GBA" "$HOME/Documents/ROMS/FC" -cnewer "$SC/m4-snapshot" | wc -l | tee "$SC/m4-e2e/source-untouched.txt"
```

Expected: `0`. Nothing under `/Volumes` is used.

- [ ] **Step 3: Chrome E2E on the mock (`pnpm dev`)**

Start the dev server in the background: `cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && pnpm dev` (port 1431). Load the browser tools with `ToolSearch` (`select:mcp__chrome-devtools__new_page,mcp__chrome-devtools__navigate_page,mcp__chrome-devtools__take_snapshot,mcp__chrome-devtools__take_screenshot,mcp__chrome-devtools__click,mcp__chrome-devtools__press_key,mcp__chrome-devtools__fill,mcp__chrome-devtools__wait_for,mcp__chrome-devtools__evaluate_script,mcp__chrome-devtools__list_console_messages,mcp__chrome-devtools__list_network_requests,mcp__chrome-devtools__resize_page`, or the `mcp__plugin_chrome-devtools-mcp_chrome-devtools__*` names if those are the ones available). Screenshots go to `$SC/m4-e2e/`; if a path outside the allowed roots is refused, save into `target/m4-e2e/` (git-ignored) and `mv` them to `$SC/m4-e2e/` afterwards. Viewport 1440×900.

Walk this script, taking a snapshot before each click and checking the expected result; record each check in `$SC/m4-e2e/e2e-notes.md`:

1. `http://localhost:1431` → "Open folder…" → the 12k fixture opens (`01-library.png`). Note the status bar "N without cover".
2. "Fill missing art" → step 1 of 5; choose "Whole library (12,000 ROMs)"; Next; check Cover and Title; Next; "ROM region" selected; "Move USA up"; Next; "Only missing art"; "Preview (dry run)" → the gathering status with a progress bar (`02-gathering.png`), then the summary table per system with the Total row and 4 sample images (`03-summary.png`).
3. "Review (n)" → the review queue: the current art next to 2–3 candidates with source, region and score (`04-review.png`). Press `2`, `Enter` (the counter shows 1 decided and the next ROM), `s`, then `a` (all decided). Esc → back to the summary; "Run (n)" now counts the accepted review items.
4. "Run (n)" → the jobs panel shows the apply progress (`05-applying.png`), then the pause banner "Paused: the art server is limiting requests…" with Resume (the mock pauses after 25 items) (`06-paused.png`); Resume → "Finished" with the totals and the "Problems (n)" list naming no-art / network / decode / name-invalid reasons (`07-done.png`).
5. Close the panel: the status bar "without cover" count dropped by the filled covers; filled tiles show their new art.
6. Jobs → "Export report (CSV)" and "(JSON)" → "Saved as coverly-job-1.csv." / ".json." (Chrome downloads them).
7. Jobs → "Undo last job" → the confirmation text → "Undo" → "Undone: … removed, 0 restored, 0 kept…", history shows "Undone", the status bar count is back to step 1's value (`08-undone.png`).
8. Pick a ROM without cover → drawer → "Change…" → 3 candidates (`09-drawer-candidates.png`) → click the second → "Art updated…" and the cover appears; "From file…" → updated; "Remove" → the confirmation → "Remove image" → the cover is gone; Jobs → Undo → the cover is back.
9. "Reports" → audit table per system with Total (`10-reports.png`); "Export audit (CSV)" → "Saved as coverly-audit.csv.".
10. Hostile names: search `<img` → open the drawer of `<img src=x onerror=alert(1)>` → "Change…": candidate names are text; `evaluate_script` `() => document.querySelectorAll('img[src="x"], [onerror]').length` → `0`.
11. Language → Español: the wizard ("Rellenar arte que falta", "Paso 1 de 5"), the review queue ("Revisar coincidencias dudosas") and the jobs panel ("Trabajos", "Deshacer el último trabajo") in Spanish (`11-es-wizard.png`, `12-es-jobs.png`).
12. `list_console_messages` → 0 errors and 0 warnings (save `console.txt`); `list_network_requests` → all 200 (save `network.txt`).

Any bug found: write the failing test, fix it, commit `fix(…)`, re-run the step, note it in the evidence.

- [ ] **Step 4: Architecture docs**

`docs/04-architecture/desktop-app.md`:
- TL;DR: add one sentence: "Fills run as one backend job at a time (dry run → review → apply, pausable and resumable); only the entries a job touched are re-scanned and pushed to the webview."
- Layers table: add `Jobs | src-tauri/src/jobs/ | JobManager (one job), worker threads, JobEvents, wire views` and stores row `jobStore`.
- IPC table: add the rows of Tasks 6–7 (`start_fill`, `fill_job`, `fill_job_items`, `decide_fill`, `run_fill`, `resume_fill`, `cancel_fill`, `candidate_art`, `undo_last_job`, `job_history`, `entry_candidates`, `entry_candidate_art`, `set_entry_art`, `set_entry_art_from_file`, `remove_entry_art`, `export_audit`, `export_job_report`) with their arguments, results and error codes (`job-running`, `no-job`, `stale-job`, `bad-request`, `nothing-to-undo`, `network`, `bad-image`, `write-denied`, `disk-full`, `name-invalid`, `volume-gone`), and the events `fill-job-progress`, `fill-job-changed`, `library-entries-updated`.
- New section "Fill jobs": the state machine (copy it from Task 5), one job at a time, throttled progress (~20/s), `written` before `changed`, decisions recorded before Run, undecided review items skipped (D20), resume into the same journal (D21), undo = newest not-undone journal of the open library, refused while a job gathers, applies or is paused; a panicking worker ends the job `failed`; "Re-scan of touched entries" (`subset_scan` + `resolve_art`, `refresh_entries` installs a new `Session` for the same generation).
- Security: add "The image picker and the save dialog run in Rust (`DialogExt`); the webview never sends a path. Picked files are read up to 20 MiB and always decoded and re-encoded."
- Mock adapter: add the job rules (Task 8 "Mock rules"), `pauseAfter`, `pick`, exports through Blob downloads.

`docs/04-architecture/fill-pipeline.md`:
- TL;DR: add "In the app the same pipeline runs as a job (`coverly_core::fill::FillEnv`), with review decisions between plan and apply."
- Strategies and D9: add the M2-11 rule (D22): `is_unreleased_variant` (bracketed beta/proto/demo/sample/hack/pirate/kiosk/debug/unl tags, GoodTools `[h…]`), a non-retail first pick that is neither canonical nor an exact file name, for a ROM that is not itself such a variant, is never `auto`; the first retail candidate that reaches the review threshold goes first, `note: unreleased-variant`. Real cases: `057 Final Fantasy IV` → `Final Fantasy IV Advance (Europe)…` in review, `054 Contra Advance` → `(Beta)` in review.
- Writer guarantees: journal line `{"type":"entry","target":"…","op":{"kind":"removed","backup":"…"},"sha1":"…"}`; undo of a removal restores the backup unless the user put a file there.
- Pause and resume: the `pause_kind` field (`rate-limit`, `offline`, `volume`); "Resume = run the same command again" stays for the CLI; the app resumes the not-attempted (and volume-failed) items into the same journal (`Writer::resume`, a torn last line is cut off).
- New section "In the app" with the decisions table (`accept`, `pick`, `skip`) and the drawer writes (`write_local_image`, `remove_art`), each a journaled job of its own.
- "End-to-end evidence": add the M4 numbers from Step 2.

- [ ] **Step 5: Tracker**

`docs/TRACKER.md`:
- "Estado actual": fecha, fase "M4 implementado en `feat/m4-fill-app` (pendiente de PR)", siguiente paso "PR M4 + verificación independiente".
- Decisions: add
  - `| D20 | En la app se escriben solo los ítems auto y los de revisión aceptados/elegidos; los de revisión sin decidir se omiten (la CLI mantiene D14) | 2026-10-16 | Claude (plan M4) |`
  - `| D21 | Reanudar un job pausado continúa el mismo journal (Writer::resume): un único Deshacer revierte ambas tandas. En la app, Deshacer actúa solo sobre journals de la biblioteca abierta y se rechaza mientras un job está pausado | 2026-10-16 | Claude (plan M4) |`
  - `| D22 | M2-11: una variante beta/proto/demo/hack/sin licencia nunca es auto salvo coincidencia canónica o de nombre exacto, o si la ROM es esa variante; se antepone la primera release ≥ umbral de revisión | 2026-10-16 | Claude (plan M4) |`
  - `| D23 | Los diálogos de «Desde archivo…» y de exportar se abren desde Rust: la webview nunca envía rutas y no hay capacidades nuevas | 2026-10-16 | Claude (plan M4) |`
- M2-11 → ✅ with the Task 1 commit and the real-run lines; M2-10, M2-12 stay ⬜ (note "aviso en el asistente para batocera" on M2-10).
- M3-07 note: "sigue diferido (identidad bajo demanda) → M5".
- M4 rows: M4-01 (Tasks 3, 5, 6, 9), M4-02 (Task 10), M4-03 (Tasks 2, 5, 6, 11), M4-04 (Tasks 7, 12) → ✅ with commit hashes and "Evidencia M4"; add `| M4-05 | Buscar por título en el panel (spec §8) | ⏸️ | Diferido a M5: necesita una identidad manual que no cambie el nombre del destino |`.
- New section "### Evidencia M4 — relleno en la app (fecha)": the real run (copy: files, size; dry-run counts per system and time; M2-11 lines; apply counts, time, PNG checks; without-cover before/after; undo identical; `find -cnewer` → 0; nothing under `/Volumes`), the Chrome E2E (screenshots list, console/network results, bugs found and their fix commits), test counts (`cargo test --workspace`, `pnpm test`) and the gates.
- Bitácora: one line for M4.

Commit:

```bash
cd /Users/miguelangelesparzacalero/Documents/GitHub/coverly-roms && git add docs && git commit -m "docs: record M4 fill-in-app evidence and the fill job architecture"
```

- [ ] **Step 6: Final gates**

Run every gate once more on the final tree and paste the summary lines (test counts) into «Evidencia M4» if they changed; amend nothing — a follow-up `docs(tracker): …` commit if needed.

---

## Self-review notes

- **Plan review (2026-10-16):** tightened job safety — worker phases are `guarded` (a panic ends the job `failed`, never `busy` forever); the app's Undo is scoped to the open library (`list_jobs` + `undo_job`) and refused while a job is paused (Rust, mock and the Jobs panel's `canUndo` agree); a vacuous early `return` in the Jobs panel test became an assertion. Known, accepted: `prune_jobs` (keep 10) could drop a paused job's journal after 10 more drawer writes — resume then ends the job `failed` with the I/O error, nothing is written.

- **Spec coverage:** §7.3 select (wizard scope: selection / filter / library), §7.4–7.6 identify, fetch, choose (core `FillEnv::plan`, strategy + region order), §7.7 review (Task 10), §7.8 apply with backup and journal (Tasks 2, 3, 5), §7.9 re-scan of affected entries + report (Tasks 4, 6, 11, 12); §8 Fill wizard (Task 9), Review queue (Task 10; "search" deferred, M4-05), Jobs (Task 11), Reports (Task 12), drawer change / from file / remove (Tasks 7, 12); §10 per-item errors, pause on rate limit/offline/volume with resume (Tasks 2, 5, 11); §11 every image decoded and re-encoded, no paths from the webview (Tasks 3, 7); M2-11 (Task 1); exit criterion "fill missing covers on the SD copy from the UI" → mock E2E of the UI + the real run of the same job path the UI drives (Task 13).
- **Types used across tasks:** `FillRequest`, `JobView`, `JobItemView`, `CandidateView`, `DecisionIn` (`{ index, choice, candidate? }`), `EntryArtUpdate`, `UndoView`, `JobHistoryView`/`JobHistoryItem`, `ExportFormat` are defined once in Rust (Task 4) and mirrored once in TS (Task 8), pinned by `contract.sample.json` / `contract.requests.json`.
