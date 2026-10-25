import { describe, expect, it } from 'vitest';

import type {
  DedupeJobState,
  DedupeJobView,
  DedupePlanView,
  DedupeProgressEvent,
} from '../types';
import { dedupeFixture } from './dedupeFixture';
import { FIXTURE_NOW } from './fixture';
import {
  createMockBackend,
  MOCK_LIBRARIES,
  type MockBackend,
  type MockOptions,
} from './mockBackend';
import { MOCK_DEDUPE_EXPORT } from './mockDedupe';

const ROOT = MOCK_LIBRARIES[0].path;

function backend(options: MockOptions = {}): MockBackend {
  return createMockBackend({
    delayMs: 0,
    now: FIXTURE_NOW,
    storage: null,
    pauseAfter: null,
    ...options,
  });
}

/** The next duplicates job view that is not `applying` (register first). */
function ended(b: MockBackend, state?: DedupeJobState): Promise<DedupeJobView> {
  return new Promise((resolve) => {
    void b.onDedupeChanged((v) => {
      if (v.state !== 'applying' && (!state || v.state === state)) resolve(v);
    });
  });
}

async function applied(b: MockBackend, plan: DedupePlanView) {
  const done = ended(b);
  const started = await b.dedupeApply(plan.plan_id);
  expect(started.state).toBe('applying');
  return done;
}

const acting = (p: DedupePlanView) =>
  p.groups.filter((g) => g.action.kind === 'act');

describe('mock duplicates backend', () => {
  it('mock_plan_emits_phases_in_order', async () => {
    const b = backend();
    const seen: DedupeProgressEvent[] = [];
    await b.onDedupeProgress((e) => seen.push(e));
    const plan = await b.dedupePlan(ROOT);
    const phases = seen.map((e) => e.phase).filter((p, i, a) => a[i - 1] !== p);
    expect(phases).toEqual([
      'listing',
      'sizes',
      'fingerprints',
      'serials',
      'catalogues',
    ]);
    expect(seen.every((e) => e.plan_id === plan.plan_id)).toBe(true);
    for (const phase of phases) {
      const of = seen.filter((e) => e.phase === phase);
      expect(of.at(-1)!.done).toBe(of.at(-1)!.total);
    }
  });

  it('plans the fixture under the library root, with absolute paths', async () => {
    const b = backend();
    const plan = await b.dedupePlan(ROOT);
    const fixture = dedupeFixture();
    expect(plan.root).toBe(ROOT);
    expect(plan.roms_dir).toBe(`${ROOT}/ROMS`);
    expect(plan.quarantine).toBe(`${ROOT}/_Duplicados`);
    expect(plan.totals).toEqual(fixture.totals);
    expect(plan.trash).toBe(false);
    expect(plan.trash_available).toBe(true);
    const ecco = plan.groups.find((g) => g.title === 'Ecco the Dolphin')!;
    expect(ecco.members[0]!.unit.primary).toBe(
      `${ROOT}/ROMS/SMS/Ecco the Dolphin.sms`,
    );
    expect(ecco.members[1]!.dest).toBe(
      `${ROOT}/_Duplicados/SMS/Adventures of Dolphins.sms`,
    );
  });

  it('a cancelled plan rejects with cancelled', async () => {
    const b = backend();
    let id = 0;
    await b.onDedupeProgress((e) => {
      if (id === 0) {
        id = e.plan_id;
        void b.dedupeCancel(id);
      }
    });
    await expect(b.dedupePlan(ROOT)).rejects.toMatchObject({
      code: 'cancelled',
    });
  });

  it('mock_set_keeper_moves_the_old_keeper_to_the_tray', async () => {
    const b = backend();
    const plan = await b.dedupePlan(ROOT);
    const crash = plan.groups.find((g) => g.title === 'Crash Bash')!;
    const old = crash.keeper;
    const next = await b.dedupeSetKeeper(plan.plan_id, crash.id, 2);
    const g = next.groups.find((x) => x.id === crash.id)!;
    expect(g.keeper).toBe(2);
    expect(g.reason).toEqual({ kind: 'user-choice' });
    expect(g.members[2]!.dest).toBeNull();
    // The old keeper is in the tray now, bound for the quarantine.
    expect(g.members[old]!.dest).toBe(`${ROOT}/_Duplicados/PS/Crash Bash.PBP`);
    expect(next.plan_id).toBe(plan.plan_id);
    // Remembered for the next plan of this library.
    const again = await b.dedupePlan(ROOT);
    expect(again.groups.find((x) => x.id === crash.id)!.keeper).toBe(2);
    await expect(b.dedupeResetChoices(ROOT)).resolves.toBe(1);
    const fresh = await b.dedupePlan(ROOT);
    expect(fresh.groups.find((x) => x.id === crash.id)!.keeper).toBe(0);
  });

  it('refuses a conflict keeper, an unknown member and a dropped plan', async () => {
    const b = backend();
    const plan = await b.dedupePlan(ROOT);
    const conflict = plan.groups.find((g) => g.action.kind === 'conflict')!;
    await expect(
      b.dedupeSetKeeper(plan.plan_id, conflict.id, 1),
    ).rejects.toMatchObject({ code: 'keeper-required' });
    await expect(
      b.dedupeSetKeeper(plan.plan_id, plan.groups[0]!.id, 9),
    ).rejects.toMatchObject({ code: 'bad-request' });
    await expect(b.dedupeSetKeeper(999, 1, 0)).rejects.toMatchObject({
      code: 'no-dedupe-plan',
    });
  });

  it('skips groups and systems, and takes them back', async () => {
    const b = backend();
    const plan = await b.dedupePlan(ROOT);
    const ecco = plan.groups.find((g) => g.title === 'Ecco the Dolphin')!;
    let next = await b.dedupeSkipGroup(plan.plan_id, ecco.id, true);
    let g = next.groups.find((x) => x.id === ecco.id)!;
    expect(g.action).toEqual({
      kind: 'report-only',
      why: { kind: 'user-skipped' },
    });
    expect(g.members.every((m) => m.dest === null)).toBe(true);
    expect(next.totals.extras).toBe(plan.totals.extras - 1);
    next = await b.dedupeSkipGroup(plan.plan_id, ecco.id, false);
    g = next.groups.find((x) => x.id === ecco.id)!;
    expect(g.action).toEqual({ kind: 'act' });
    expect(next.totals).toEqual(plan.totals);

    next = await b.dedupeSkipSystem(plan.plan_id, 'GBA', true);
    expect(next.totals.by_system.GBA).toBeUndefined();
    next = await b.dedupeSkipSystem(plan.plan_id, 'GBA', false);
    expect(next.totals).toEqual(plan.totals);
  });

  it('regroups with other levels without reading again', async () => {
    const b = backend();
    const seen: DedupeProgressEvent[] = [];
    const plan = await b.dedupePlan(ROOT);
    await b.onDedupeProgress((e) => seen.push(e));
    const exact = await b.dedupeRegroup(plan.plan_id, { levels: ['exact'] });
    expect(seen).toEqual([]);
    expect(exact.groups.some((g) => g.title === 'Crash Bash')).toBe(false);
    expect(exact.groups.some((g) => g.title === 'Tetris')).toBe(false);
    const one = await b.dedupeRegroup(plan.plan_id, {
      levels: ['exact', 'format', 'version'],
      one_game_one_rom: true,
    });
    // The 37 other versions act once one ROM per game is on.
    expect(acting(one).length).toBe(acting(plan).length + 37);
    const trash = await b.dedupeRegroup(plan.plan_id, {
      quarantine: { kind: 'trash' },
    });
    expect(trash.trash).toBe(true);
    expect(acting(trash).every((g) => g.members.every((m) => !m.dest))).toBe(
      true,
    );
  });

  it('mock_apply_then_undo_restores_counts', async () => {
    const b = backend();
    const plan = await b.dedupePlan(ROOT);
    const job = await applied(b, plan);
    expect(job.state).toBe('done');
    expect(job.trash).toBe(false);
    const report = job.report!;
    const moved = report.outcomes.filter((o) => o.status === 'quarantined');
    expect(moved).toHaveLength(plan.totals.extras);
    expect(report.bytes_out).toBe(plan.totals.bytes);
    // Copies of groups that do not act are reported as left alone.
    expect(
      report.outcomes.filter((o) => o.status === 'skipped').length,
    ).toBeGreaterThan(0);
    const ecco = moved.find((o) => o.files[0]!.includes('Adventures'))!;
    expect(ecco.status === 'quarantined' && ecco.to).toBe(
      `${ROOT}/_Duplicados/SMS/Adventures of Dolphins.sms`,
    );
    expect(ecco.files).toEqual([
      `${ROOT}/_Duplicados/SMS/Adventures of Dolphins.sms`,
      `${ROOT}/_Duplicados/SMS/Imgs/Adventures of Dolphins.png`,
    ]);
    // The library changed: the kept plans are gone.
    await expect(
      b.dedupeSkipGroup(plan.plan_id, plan.groups[0]!.id, true),
    ).rejects.toMatchObject({ code: 'no-dedupe-plan' });
    await expect(b.dedupeJob()).resolves.toEqual(job);

    const after = await b.dedupePlan(ROOT);
    expect(after.totals.extras).toBe(0);
    const history = await b.jobHistory();
    expect(history[0]).toMatchObject({
      job: report.job,
      kind: 'dedupe',
      root: ROOT,
      complete: true,
      undone: false,
      entries: plan.totals.extras,
    });

    const undo = await b.organizeUndo(report.job!, false);
    expect(undo.later_jobs).toEqual([]);
    expect(undo.report[0]!.restored).toContain(
      `${ROOT}/ROMS/SMS/Adventures of Dolphins.sms`,
    );
    expect(undo.report[0]!.restored).toContain(
      `${ROOT}/ROMS/SMS/Imgs/Adventures of Dolphins.png`,
    );
    expect(undo.report[0]!.dirs_removed).toContain(`${ROOT}/_Duplicados`);
    const back = await b.dedupePlan(ROOT);
    expect(back.totals).toEqual(plan.totals);
    expect((await b.jobHistory())[0]!.undone).toBe(true);
    await expect(b.organizeUndo(report.job!, false)).rejects.toMatchObject({
      code: 'nothing-to-undo',
    });
  });

  it('mock_trash_undo_brings_files_back', async () => {
    const b = backend();
    const plan = await b.dedupePlan(ROOT, { quarantine: { kind: 'trash' } });
    expect(plan.trash).toBe(true);
    const job = await applied(b, plan);
    expect(job.trash).toBe(true);
    const trashed = job.report!.outcomes.filter((o) => o.status === 'trashed');
    expect(trashed).toHaveLength(plan.totals.extras);
    const first = trashed[0]!;
    expect(first.status === 'trashed' && first.location).toMatch(/\.Trash/);
    const undo = await b.organizeUndo(job.report!.job!, false);
    const r = undo.report[0]!;
    // Undo brings every copy back from the Trash: no list to restore by hand.
    expect(r.restored).toHaveLength(
      trashed.reduce((n, o) => n + o.files.length, 0),
    );
    expect(r.archive_lost).toEqual([]);
    expect(r.skipped_modified).toEqual([]);
    expect(r.dirs_removed).toEqual([]);
    expect((await b.dedupePlan(ROOT)).totals).toEqual(plan.totals);
  });

  it('skips Trash copies on a volume without one', async () => {
    const b = backend({ dedupeTrashAvailable: false });
    const plan = await b.dedupePlan(ROOT, { quarantine: { kind: 'trash' } });
    expect(plan.trash_available).toBe(false);
    const job = await applied(b, plan);
    expect(
      job.report!.outcomes.filter(
        (o) => o.status === 'skipped' && o.reason.kind === 'trash-unavailable',
      ),
    ).toHaveLength(plan.totals.extras);
    expect(job.report!.job).toBeNull();
  });

  it('pauses on a full card and resumes into the same journal', async () => {
    const b = backend({ dedupePauseAfter: 10 });
    const plan = await b.dedupePlan(ROOT);
    const paused = await applied(b, plan);
    expect(paused.state).toBe('paused');
    const report = paused.report!;
    expect(report.paused).toBe('disk-full');
    expect(report.resumable).toHaveLength(plan.totals.extras - 10);
    const done = ended(b);
    const resumed = await b.dedupeResume(report.job!);
    expect(resumed.job).toBe(report.job);
    const end = await done;
    expect(end.state).toBe('done');
    expect(end.report!.job).toBe(report.job);
    expect((await b.dedupePlan(ROOT)).totals.extras).toBe(0);
    await expect(b.dedupeResume(report.job!)).rejects.toMatchObject({
      code: 'bad-request',
    });
  });

  it('cancels between groups', async () => {
    const b = backend({ delayMs: 1 });
    const plan = await b.dedupePlan(ROOT);
    const done = ended(b);
    await b.dedupeApply(plan.plan_id);
    await b.dedupeCancelApply();
    const end = await done;
    expect(end.state).toBe('cancelled');
    const cancelled = end.report!.outcomes.filter(
      (o) => o.status === 'skipped' && o.reason.kind === 'cancelled',
    );
    expect(cancelled.length).toBeGreaterThan(0);
    // Like Rust: what the cancel left is resumable, into the same journal.
    expect(end.report!.resumable).toEqual(
      cancelled.map((o) => [o.group, o.member]),
    );
    const job = end.report!.job!;
    expect(job).not.toBeNull();
    expect((await b.jobHistory())[0]).toMatchObject({ job, complete: false });
    const finished = ended(b);
    await b.dedupeResume(job);
    const resumed = await finished;
    expect(resumed.state).toBe('done');
    expect(resumed.report!.job).toBe(job);
    expect((await b.dedupePlan(ROOT)).totals.extras).toBe(0);
    await expect(b.dedupeResume(job)).rejects.toMatchObject({
      code: 'bad-request',
    });
    await expect(
      b.dedupeResume('20990101T000000Z-0000-0000'),
    ).rejects.toMatchObject({ code: 'no-dedupe-plan' });
    // Undo brings back what both runs set aside.
    await b.organizeUndo(job, false);
    expect((await b.dedupePlan(ROOT)).totals).toEqual(plan.totals);
  });

  it('refuses a second write while one runs', async () => {
    const b = backend({ delayMs: 1 });
    const plan = await b.dedupePlan(ROOT);
    const done = ended(b);
    await b.dedupeApply(plan.plan_id);
    const again = await b.dedupePlan(ROOT);
    await expect(b.dedupeApply(again.plan_id)).rejects.toMatchObject({
      code: 'job-running',
    });
    await done;
  });

  it('keeps four plans', async () => {
    const b = backend();
    const first = await b.dedupePlan(ROOT);
    for (let i = 0; i < 4; i++) await b.dedupePlan(ROOT);
    await expect(
      b.dedupeSkipGroup(first.plan_id, first.groups[0]!.id, true),
    ).rejects.toMatchObject({ code: 'no-dedupe-plan' });
  });

  it('exports the plan and reveals paths', async () => {
    const b = backend();
    const plan = await b.dedupePlan(ROOT);
    await expect(b.dedupeExportPlan(plan.plan_id)).resolves.toBe(
      MOCK_DEDUPE_EXPORT,
    );
    const saved = b.exports.at(-1)!;
    expect(saved.name).toBe(MOCK_DEDUPE_EXPORT);
    expect(JSON.parse(saved.text).root).toBe(ROOT);
    await b.revealPath(plan.quarantine);
    expect(b.revealedPaths).toEqual([plan.quarantine]);
  });

  it('scenarios: an empty library, a failing plan', async () => {
    const b = backend();
    b.dedupeScenario({ empty: true });
    const empty = await b.dedupePlan(ROOT);
    expect(empty.groups).toEqual([]);
    expect(empty.totals.extras).toBe(0);
    b.dedupeScenario({ empty: false, failPlan: 'volume-gone' });
    await expect(b.dedupePlan(ROOT)).rejects.toMatchObject({
      code: 'volume-gone',
    });
    b.dedupeScenario({ failPlan: null, trashAvailable: false });
    expect((await b.dedupePlan(ROOT)).trash_available).toBe(false);
  });
});
