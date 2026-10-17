import { describe, expect, it } from 'vitest';

import type {
  OrganizeJobView,
  OrganizeJobState,
  OrganizeProgressEvent,
} from '../types';
import { FIXTURE_NOW } from './fixture';
import {
  createMockBackend,
  type MockBackend,
  type MockOptions,
} from './mockBackend';
import { MOCK_DOWNLOADS, MOCK_MIXED } from './mockOrganize';

function backend(options: MockOptions = {}): MockBackend {
  return createMockBackend({
    delayMs: 0,
    now: FIXTURE_NOW,
    storage: null,
    pauseAfter: null,
    ...options,
  });
}

/** The next organise job view that is not `applying` (register before acting). */
function ended(
  b: MockBackend,
  state?: OrganizeJobState,
): Promise<OrganizeJobView> {
  return new Promise((resolve) => {
    void b.onOrganizeChanged((v) => {
      if (v.state !== 'applying' && (!state || v.state === state)) resolve(v);
    });
  });
}

async function applied(b: MockBackend, source = MOCK_DOWNLOADS) {
  const { plan_id } = await b.organizePlan({ source });
  const done = ended(b);
  const started = await b.organizeApply(plan_id, []);
  expect(started.state).toBe('applying');
  return done;
}

describe('mock organise backend', () => {
  it('plans the downloads pile like the real sample', async () => {
    const b = backend();
    await expect(b.openLibrary(MOCK_DOWNLOADS)).rejects.toMatchObject({
      code: 'no-profile',
    });
    const probe = await b.probeFolder(MOCK_DOWNLOADS);
    expect(probe).toMatchObject({
      rom_like: 17,
      partial_downloads: 3,
      sorted_folders: [],
      writable: true,
    });
    const { plan } = await b.organizePlan({ source: MOCK_DOWNLOADS });
    expect(plan.counts).toMatchObject({
      auto: 16,
      review: 0,
      roms: 16,
      systems: 4,
      left: 4,
      junk: 1,
    });
    expect(
      new Set(plan.items.flatMap((i) => (i.folder ? [i.folder] : []))),
    ).toEqual(new Set(['GBA', 'GB', 'GBC', 'SFC']));
    expect(plan).toMatchObject({
      source_root: MOCK_DOWNLOADS,
      target_root: MOCK_DOWNLOADS,
      roms_dir: `${MOCK_DOWNLOADS}/ROMS`,
      layout: 'anbernic-stock',
      mode: 'move',
      names: 'keep',
    });
    const reasons = plan.items.flatMap((i) =>
      i.action === 'left' ? [i.reason.kind] : [],
    );
    expect(reasons.sort()).toEqual([
      'duplicate',
      'partial-download',
      'partial-download',
      'partial-download',
    ]);
    for (const i of plan.items.filter((i) => i.action === 'auto')) {
      expect(i.target).toMatch(/^ROMS\/(GBA|GB|GBC|SFC)\//);
      expect(i.why.length).toBeGreaterThan(0);
      expect(i.method).not.toBeNull();
    }
  });

  it('reports plan progress and honours the layout, target and names', async () => {
    const b = backend();
    const events: OrganizeProgressEvent[] = [];
    await b.onOrganizeProgress((e) => events.push(e));
    const { plan_id, plan } = await b.organizePlan({
      source: MOCK_DOWNLOADS,
      layout: 'esde',
      target: '/Volumes/SD',
      mode: 'copy',
      names: 'catalogue',
    });
    expect(events.some((e) => e.plan_id === plan_id)).toBe(true);
    expect(events.at(-1)).toMatchObject({ phase: 'identify' });
    expect(plan.roms_dir).toBe('/Volumes/SD/ROMs');
    expect(plan.mode).toBe('copy');
    expect(new Set(plan.items.flatMap((i) => i.folder ?? []))).toEqual(
      new Set(['gba', 'gb', 'gbc', 'snes']),
    );
    const renamed = plan.items.find((i) => i.rename?.kind === 'catalogue');
    expect(renamed?.catalogue_name).toEqual(expect.any(String));
    expect(renamed?.target).toBe(
      `ROMs/${renamed?.folder}/${renamed?.catalogue_name}.zip`,
    );

    const flat = await b.organizePlan({ source: MOCK_DOWNLOADS, flat: true });
    expect(flat.plan.roms_dir).toBe(MOCK_DOWNLOADS);
    expect(flat.plan.items.find((i) => i.action === 'auto')?.target).toMatch(
      /^(GBA|GB|GBC|SFC)\//,
    );
    await expect(
      b.organizePlan({ source: MOCK_DOWNLOADS, layout: 'nope' }),
    ).rejects.toMatchObject({ code: 'unknown-profile' });
    await expect(b.organizePlan({ source: '/nowhere' })).rejects.toMatchObject({
      code: 'not-found',
    });
    await expect(b.probeFolder('/nowhere')).rejects.toMatchObject({
      code: 'not-found',
    });
  });

  it('applies with progress, then opens with the profile and undoes', async () => {
    const b = backend();
    const progress: OrganizeProgressEvent[] = [];
    await b.onOrganizeProgress((e) => progress.push(e));
    const view = await applied(b);
    const apply = progress.filter((e) => e.phase === 'apply');
    expect(apply.map((e) => e.done)).toEqual([0, 4, 8, 12, 16]);
    expect(view.state).toBe('done');
    expect(view.report).toMatchObject({ moved: 16, copied: 0, failed: 0 });
    expect(view.job).toEqual(expect.any(String));
    expect(view).toMatchObject({
      target_root: MOCK_DOWNLOADS,
      layout: 'anbernic-stock',
    });

    const lib = await b.openLibrary(MOCK_DOWNLOADS, 'anbernic-stock');
    expect(lib.entries).toHaveLength(16);
    expect(lib.profile.id).toBe('anbernic-stock');
    expect(lib.systems.map((s) => s.folder)).toEqual([
      'GB',
      'GBA',
      'GBC',
      'SFC',
    ]);
    expect(lib.entries[0]?.rel_path).toMatch(/^ROMS\/GB\//);

    const history = await b.jobHistory();
    expect(history[0]).toMatchObject({
      job: view.job,
      kind: 'organize',
      entries: 16,
      complete: true,
      undone: false,
      undoable: true,
    });
    // What is left loose: the duplicate, the partial downloads.
    expect((await b.probeFolder(MOCK_DOWNLOADS)).sorted_folders).toEqual([
      'GB',
      'GBA',
      'GBC',
      'SFC',
    ]);

    const undo = await b.organizeUndo(view.job!, false);
    expect(undo.later_jobs).toEqual([]);
    expect(undo.report).toHaveLength(1);
    expect(undo.report[0]?.restored).toHaveLength(16);
    expect(undo.report[0]?.dirs_removed).toContain(`${MOCK_DOWNLOADS}/ROMS`);
    await expect(b.openLibrary(MOCK_DOWNLOADS)).rejects.toMatchObject({
      code: 'no-profile',
    });
    expect((await b.jobHistory())[0]).toMatchObject({ undone: true });
    await expect(b.organizeUndo(view.job!, false)).rejects.toMatchObject({
      code: 'nothing-to-undo',
    });
  });

  it('copies leave the pile in place and undo removes the copies', async () => {
    const b = backend();
    const { plan_id } = await b.organizePlan({
      source: MOCK_DOWNLOADS,
      target: '/Volumes/SD',
      mode: 'copy',
    });
    const done = ended(b);
    await b.organizeApply(plan_id, []);
    const view = await done;
    expect(view.report).toMatchObject({ moved: 0, copied: 16 });
    expect((await b.probeFolder(MOCK_DOWNLOADS)).rom_like).toBe(17);
    expect((await b.openLibrary('/Volumes/SD')).entries).toHaveLength(16);
    const undo = await b.organizeUndo(view.job!, false);
    expect(undo.report[0]?.removed).toHaveLength(16);
    expect(undo.report[0]?.restored).toEqual([]);
  });

  it('settles review items with decisions', async () => {
    const b = backend();
    const { plan } = await b.organizePlan({ source: MOCK_MIXED });
    expect(plan.counts).toMatchObject({ review: 2 });
    const left = plan.items.flatMap((i) =>
      i.action === 'left' ? [i.reason.kind] : [],
    );
    expect(left.sort()).toEqual([
      'duplicate',
      'partial-download',
      'partial-download',
      'unknown',
    ]);
    const [first, second] = plan.items.filter((i) => i.action === 'review');
    expect(first!.alternatives.length).toBeGreaterThan(0);
    const unknown = plan.items.find(
      (i) => i.action === 'left' && i.reason.kind === 'unknown',
    )!;

    const decided = await b.organizePlan({ source: MOCK_MIXED }, [
      { item: first!.id, choice: 'assign', system: 'gbc' },
      { item: second!.id, choice: 'accept' },
      { item: unknown.id, choice: 'assign', system: 'megadrive' },
    ]);
    const a = decided.plan.items[first!.id]!;
    expect(a).toMatchObject({
      action: 'auto',
      system_id: 'gbc',
      folder: 'GBC',
      method: 'user',
      confidence: 1,
    });
    expect(a.why[0]).toMatchObject({ kind: 'user', system: 'gbc' });
    expect(decided.plan.items[second!.id]).toMatchObject({ action: 'auto' });
    expect(decided.plan.items[unknown.id]).toMatchObject({
      action: 'auto',
      folder: 'MD',
    });
    expect(decided.plan.counts.review).toBe(0);

    const left2 = await b.organizePlan({ source: MOCK_MIXED }, [
      { item: first!.id, choice: 'leave' },
    ]);
    expect(left2.plan.items[first!.id]).toMatchObject({
      action: 'left',
      reason: { kind: 'unknown' },
    });
    await expect(
      b.organizePlan({ source: MOCK_MIXED }, [{ item: 99, choice: 'accept' }]),
    ).rejects.toMatchObject({ code: 'bad-request' });
    await expect(
      b.organizePlan({ source: MOCK_MIXED }, [
        { item: first!.id, choice: 'assign', system: 'nope' },
      ]),
    ).rejects.toMatchObject({ code: 'bad-request' });
  });

  it('skips undecided review items and applies the decided ones', async () => {
    const b = backend();
    const { plan_id, plan } = await b.organizePlan({ source: MOCK_MIXED });
    const review = plan.items.filter((i) => i.action === 'review');
    const done = ended(b);
    await b.organizeApply(plan_id, [
      { item: review[0]!.id, choice: 'assign', system: 'gbc' },
    ]);
    const view = await done;
    expect(view.report?.moved).toBe(plan.counts.auto + 1);
    expect(view.report?.items).toContainEqual({
      item: review[1]!.id,
      status: 'skipped',
      reason: 'not-decided',
      targets: [],
    });
    await expect(b.organizeApply(999, [])).rejects.toMatchObject({
      code: 'no-plan',
    });
    await expect(
      b.organizeApply(plan_id, [{ item: 99, choice: 'accept' }]),
    ).rejects.toMatchObject({ code: 'bad-request' });
  });

  it('refuses undo when a later fill exists unless asked', async () => {
    const b = backend();
    const view = await applied(b);
    const lib = await b.openLibrary(MOCK_DOWNLOADS, 'anbernic-stock');
    await b.setEntryArtFromFile(lib.generation, 0, 'cover');
    const fills = (await b.jobHistory()).filter((h) => h.kind === 'fill');
    expect(fills).toHaveLength(1);

    await expect(b.organizeUndo(view.job!, false)).rejects.toMatchObject({
      code: 'later-jobs',
      detail: fills[0]!.job,
    });
    const undo = await b.organizeUndo(view.job!, true);
    expect(undo.later_jobs.map((j) => j.job)).toEqual([fills[0]!.job]);
    expect(undo.report.map((r) => r.job)).toEqual([fills[0]!.job, view.job]);
    const history = await b.jobHistory();
    expect(history.every((h) => h.undone)).toBe(true);
  });

  it('keeps organise applies and fill jobs apart', async () => {
    const b = backend();
    const { plan_id } = await b.organizePlan({ source: MOCK_DOWNLOADS });
    const done = ended(b);
    await b.organizeApply(plan_id, []);
    await expect(b.organizeApply(plan_id, [])).rejects.toMatchObject({
      code: 'job-running',
    });
    await b.organizeCancelApply();
    const view = await done;
    expect(view.state).toBe('cancelled');
    expect(view.report?.items.some((i) => i.status === 'skipped')).toBe(true);
  });

  it('pauses on a full disk and resumes into the same job', async () => {
    const b = backend({ organizePauseAfter: 6 });
    const paused = ended(b, 'paused');
    const { plan_id } = await b.organizePlan({ source: MOCK_DOWNLOADS });
    await b.organizeApply(plan_id, []);
    const p = await paused;
    expect(p.report).toMatchObject({
      moved: 6,
      paused: 'volume',
      pause_reason: 'disk-full',
    });
    expect(p.report?.resumable_items).toHaveLength(10);
    expect((await b.jobHistory())[0]).toMatchObject({
      job: p.job,
      complete: false,
    });
    const done = ended(b, 'done');
    const resumed = await b.organizeResume(p.job!);
    expect(resumed.state).toBe('applying');
    const d = await done;
    expect(d.job).toBe(p.job);
    expect(d.report?.moved).toBe(10);
    expect((await b.openLibrary(MOCK_DOWNLOADS)).entries).toHaveLength(16);
    await expect(b.organizeResume('nope')).rejects.toMatchObject({
      code: 'no-job',
    });
  });

  it('cancels a plan in progress and exports a plan', async () => {
    const b = backend();
    let id: number | null = null;
    await b.onOrganizeProgress((e) => {
      if (id === null) {
        id = e.plan_id;
        void b.organizeCancel(e.plan_id);
      }
    });
    await expect(
      b.organizePlan({ source: MOCK_DOWNLOADS }),
    ).rejects.toMatchObject({ code: 'cancelled' });

    const c = backend();
    const { plan_id } = await c.organizePlan({ source: MOCK_DOWNLOADS });
    expect(await c.organizeExportPlan(plan_id)).toBe(
      'coverly-organize-plan.json',
    );
    const saved = JSON.parse(c.exports.at(-1)!.text) as {
      items: unknown[];
    };
    expect(saved.items).toHaveLength(20);
    await expect(c.organizeExportPlan(999)).rejects.toMatchObject({
      code: 'no-plan',
    });
  });

  it('escapes names in the art of the organised library', async () => {
    const b = backend();
    await applied(b, MOCK_MIXED);
    const lib = await b.openLibrary(MOCK_MIXED);
    const hostile = lib.entries.find((e) => e.name.includes('<'))!;
    expect(hostile.art.cover).toBe('missing');
    await b.setEntryArtFromFile(lib.generation, hostile.id, 'cover');
    const art = await b.readArt(lib.generation, hostile.id, 'cover');
    const svg = new TextDecoder().decode(art.bytes);
    expect(svg).toContain('&lt;img src=x');
    expect(svg).not.toContain('<img');
  });
});
