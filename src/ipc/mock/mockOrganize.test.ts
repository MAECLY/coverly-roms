import { afterEach, describe, expect, it } from 'vitest';

import type {
  OrganizeItem,
  OrganizeJobView,
  OrganizeJobState,
  OrganizeProgressEvent,
} from '../types';
import { FIXTURE_NOW } from './fixture';
import {
  createMockBackend,
  MOCK_LIBRARIES,
  type MockBackend,
  type MockOptions,
} from './mockBackend';
import {
  MOCK_DOWNLOADS,
  MOCK_MIXED,
  MOCK_PICK_KEY,
  MOCK_SAMPLE,
  readMockPick,
} from './mockOrganize';

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
      rom_like: 20,
      partial_downloads: 3,
      sorted_folders: [],
      writable: true,
    });
    const { plan } = await b.organizePlan({ source: MOCK_DOWNLOADS });
    expect(plan.counts).toMatchObject({
      auto: 19,
      review: 0,
      roms: 19,
      systems: 7,
      left: 4,
      junk: 1,
    });
    expect(
      new Set(plan.items.flatMap((i) => (i.folder ? [i.folder] : []))),
    ).toEqual(
      new Set(['GBA', 'GB', 'GBC', 'SFC', 'PS', 'DREAMCAST', 'SATURN']),
    );
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
      'not-a-rom',
      'partial-download',
      'partial-download',
      'partial-download',
    ]);
    // The names mislead; the content settles it, as on the real sample.
    const folderOf = (name: string) =>
      plan.items.find((i) => i.sources[0] === name)?.folder;
    expect(folderOf('Tetris 2.zip')).toBe('SFC');
    expect(folderOf('Tetris 2 (1).zip')).toBe('GB');
    expect(folderOf('Tetris Attack (1).zip')).toBe('GB');
    const perFolder: Record<string, number> = {};
    for (const i of plan.items)
      if (i.folder) perFolder[i.folder] = (perFolder[i.folder] ?? 0) + 1;
    expect(perFolder).toEqual({
      GBA: 6,
      GB: 6,
      GBC: 2,
      SFC: 2,
      PS: 1,
      DREAMCAST: 1,
      SATURN: 1,
    });
    for (const i of plan.items.filter((i) => i.action === 'auto')) {
      expect(i.target).toMatch(/^ROMS\/(GBA|GB|GBC|SFC|PS|DREAMCAST|SATURN)\//);
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
      new Set(['gba', 'gb', 'gbc', 'snes', 'psx', 'dreamcast', 'saturn']),
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
    expect(apply.map((e) => e.done)).toEqual([0, 4, 8, 12, 16, 19]);
    expect(view.state).toBe('done');
    expect(view.report).toMatchObject({
      moved: 16,
      copied: 0,
      extracted: 3,
      failed: 0,
    });
    expect(view.job).toEqual(expect.any(String));
    expect(view).toMatchObject({
      target_root: MOCK_DOWNLOADS,
      layout: 'anbernic-stock',
    });

    const lib = await b.openLibrary(MOCK_DOWNLOADS, 'anbernic-stock');
    // 16 cartridges and 4 sheets (Dreamcast brings a .gdi and a .cue).
    expect(lib.entries).toHaveLength(20);
    expect(lib.profile.id).toBe('anbernic-stock');
    expect(lib.systems.map((s) => s.folder)).toEqual([
      'DREAMCAST',
      'GB',
      'GBA',
      'GBC',
      'PS',
      'SATURN',
      'SFC',
    ]);
    expect(lib.entries[0]?.rel_path).toMatch(/^ROMS\/DREAMCAST\//);

    const history = await b.jobHistory();
    expect(history[0]).toMatchObject({
      job: view.job,
      kind: 'organize',
      entries: 19,
      complete: true,
      undone: false,
      undoable: true,
    });
    // What is left loose: the web page, the partial downloads and the
    // archive the Trash refused.
    expect((await b.probeFolder(MOCK_DOWNLOADS)).sorted_folders).toEqual([
      'DREAMCAST',
      'GB',
      'GBA',
      'GBC',
      'PS',
      'SATURN',
      'SFC',
    ]);

    const undo = await b.organizeUndo(view.job!, false);
    expect(undo.later_jobs).toEqual([]);
    expect(undo.report).toHaveLength(1);
    // 16 moved back and the 2 archives brought back from the Trash.
    expect(undo.report[0]?.restored).toHaveLength(18);
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
    expect(view.report).toMatchObject({ moved: 0, copied: 16, extracted: 3 });
    expect(
      view.report?.items.filter((o) => o.status === 'extracted'),
    ).toHaveLength(3);
    expect((await b.probeFolder(MOCK_DOWNLOADS)).rom_like).toBe(20);
    expect((await b.openLibrary('/Volumes/SD')).entries).toHaveLength(20);
    const undo = await b.organizeUndo(view.job!, false);
    // The 16 copies and the 80 extracted files.
    expect(undo.report[0]?.removed).toHaveLength(96);
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
    await expect(
      b.organizeApply(plan_id, [{ item: 99, choice: 'accept' }]),
    ).rejects.toMatchObject({ code: 'bad-request' });
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
    const lib = await b.openLibrary(MOCK_LIBRARIES[1].path);
    const done = ended(b);
    await b.organizeApply(plan_id, []);
    // While it runs: no fill, and no undo (Rust's manual-write guard).
    await expect(
      b.startFill(lib.generation, {
        entry_ids: null,
        media: ['cover'],
        strategy: 'rom-region',
        regions: ['rom'],
        overwrite: 'never',
      }),
    ).rejects.toMatchObject({ code: 'job-running' });
    await expect(b.organizeUndo('organize-0001', false)).rejects.toMatchObject({
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
    // The first six: five cartridges and the PlayStation disc.
    expect(p.report).toMatchObject({
      moved: 5,
      extracted: 1,
      paused: 'volume',
      pause_reason: 'disk-full',
    });
    expect(p.report?.resumable_items).toHaveLength(13);
    expect((await b.jobHistory())[0]).toMatchObject({
      job: p.job,
      complete: false,
    });
    const done = ended(b, 'done');
    const resumed = await b.organizeResume(p.job!);
    expect(resumed.state).toBe('applying');
    const d = await done;
    expect(d.job).toBe(p.job);
    expect(d.report).toMatchObject({ moved: 11, extracted: 2 });
    expect((await b.openLibrary(MOCK_DOWNLOADS)).entries).toHaveLength(20);
    await expect(b.organizeResume('nope')).rejects.toMatchObject({
      code: 'no-plan',
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
    expect(saved.items).toHaveLength(23);
    await expect(c.organizeExportPlan(999)).rejects.toMatchObject({
      code: 'no-plan',
    });
  });

  it('drops the kept plans once an apply or an undo runs, like Rust', async () => {
    const b = backend();
    const view = await applied(b);
    // The pile changed: the plan applied (and any other) is gone.
    await expect(b.organizeApply(view.plan_id, [])).rejects.toMatchObject({
      code: 'no-plan',
    });
    await expect(b.organizeExportPlan(view.plan_id)).rejects.toMatchObject({
      code: 'no-plan',
    });
    const again = await b.organizePlan({ source: MOCK_MIXED });
    await b.organizeUndo(view.job!, false);
    await expect(b.organizeApply(again.plan_id, [])).rejects.toMatchObject({
      code: 'no-plan',
    });
  });

  it('resumes only an unfinished job, with the Rust codes', async () => {
    const b = backend();
    const view = await applied(b);
    await expect(b.organizeResume('nope')).rejects.toMatchObject({
      code: 'no-plan',
    });
    await expect(b.organizeResume(view.job!)).rejects.toMatchObject({
      code: 'bad-request',
    });
  });

  it('counts a later organise into the same tree as a later job', async () => {
    const b = backend();
    const first = await b.organizePlan({
      source: MOCK_DOWNLOADS,
      target: '/Volumes/SD',
      mode: 'copy',
    });
    let done = ended(b);
    await b.organizeApply(first.plan_id, []);
    const one = await done;
    const second = await b.organizePlan({
      source: MOCK_MIXED,
      target: '/Volumes/SD',
      mode: 'copy',
    });
    done = ended(b);
    await b.organizeApply(second.plan_id, []);
    const two = await done;

    await expect(b.organizeUndo(one.job!, false)).rejects.toMatchObject({
      code: 'later-jobs',
      detail: two.job,
    });
    const undo = await b.organizeUndo(one.job!, true);
    expect(undo.later_jobs.map((j) => j.job)).toEqual([two.job]);
    expect(undo.report.map((r) => r.job)).toEqual([two.job, one.job]);
    expect((await b.jobHistory()).every((h) => h.undone)).toBe(true);
    await expect(b.openLibrary('/Volumes/SD')).rejects.toMatchObject({
      code: 'not-found',
    });
  });

  it('refuses an undo while a fill is paused', async () => {
    const b = backend({ pauseAfter: 3 });
    const view = await applied(b);
    const lib = await b.openLibrary(MOCK_DOWNLOADS, 'anbernic-stock');
    const planned = new Promise<number>((resolve) => {
      void b.onJobChanged((v) => {
        if (v.state === 'planned') resolve(v.id);
      });
    });
    await b.startFill(lib.generation, {
      entry_ids: null,
      media: ['cover'],
      strategy: 'rom-region',
      regions: ['rom', 'world', 'usa', 'europe', 'japan'],
      overwrite: 'never',
    });
    const id = await planned;
    const paused = new Promise<void>((resolve) => {
      void b.onJobChanged((v) => {
        if (v.state === 'paused') resolve();
      });
    });
    await b.runFill(id);
    await paused;
    await expect(b.organizeUndo(view.job!, true)).rejects.toMatchObject({
      code: 'job-running',
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

describe('mock organise: the folder changes between scans', () => {
  const names = (plan: { items: { sources: string[] }[] }) =>
    plan.items.map((i) => i.sources[0]!);

  it('keeps the first scan until asked to read the folder again', async () => {
    const b = backend();
    const first = await b.organizePlan({ source: MOCK_MIXED });
    expect(first.plan.items).toHaveLength(10);

    b.organizeTouch(MOCK_MIXED, {
      add: ['Wario Land 4 (USA).gba', 'Zelda DX (USA).gbc'],
      remove: ['mystery.rom'],
    });

    // Like Rust: a plain re-plan reuses the kept survey.
    const kept = await b.organizePlan({ source: MOCK_MIXED });
    expect(names(kept.plan)).toEqual(names(first.plan));

    const seen: OrganizeProgressEvent[] = [];
    await b.onOrganizeProgress((e) => seen.push(e));
    const fresh = await b.organizePlan({ source: MOCK_MIXED, rescan: true });
    expect(seen.some((e) => e.plan_id === fresh.plan_id)).toBe(true);
    const now = names(fresh.plan);
    expect(now).toHaveLength(11);
    expect(now).toContain('Wario Land 4 (USA).gba');
    expect(now).toContain('Zelda DX (USA).gbc');
    expect(now).not.toContain('mystery.rom');
    // An added file is identified by its extension and filed.
    const wario = fresh.plan.items.find(
      (i) => i.sources[0] === 'Wario Land 4 (USA).gba',
    )!;
    expect(wario).toMatchObject({ action: 'auto', system_id: 'gba' });

    // Later re-plans keep the fresh scan.
    const after = await b.organizePlan({ source: MOCK_MIXED, names: 'keep' });
    expect(names(after.plan)).toEqual(now);
  });

  it('adds and removes by count, and a folder can go missing', async () => {
    const b = backend();
    const first = await b.organizePlan({ source: MOCK_MIXED });
    b.organizeTouch(MOCK_MIXED, { add: 2, remove: 1 });
    const fresh = await b.organizePlan({ source: MOCK_MIXED, rescan: true });
    expect(fresh.plan.items).toHaveLength(first.plan.items.length + 1);
    expect((await b.probeFolder(MOCK_MIXED)).rom_like).toBeGreaterThan(0);

    b.organizeTouch(MOCK_MIXED, { gone: true });
    await expect(
      b.organizePlan({ source: MOCK_MIXED, rescan: true }),
    ).rejects.toMatchObject({ code: 'not-found' });
    b.organizeTouch(MOCK_MIXED, { gone: false });
    const back = await b.organizePlan({ source: MOCK_MIXED, rescan: true });
    expect(back.plan.items).toHaveLength(fresh.plan.items.length);
  });

  it('files a copy once its original is gone', async () => {
    const b = backend();
    await b.organizePlan({ source: MOCK_MIXED });
    b.organizeTouch(MOCK_MIXED, { remove: ['Super Mario Land (World).gb'] });
    const fresh = await b.organizePlan({ source: MOCK_MIXED, rescan: true });
    const copy = fresh.plan.items.find(
      (i) => i.sources[0] === 'Super Mario Land (World) (1).gb',
    )!;
    expect(copy).toMatchObject({ action: 'auto', system_id: 'gb' });
  });

  it('refuses a folder it does not know', () => {
    const b = backend();
    expect(() => b.organizeTouch('/nowhere', { add: 1 })).toThrow();
  });
});

describe('mock folder pick', () => {
  afterEach(() => {
    window.history.replaceState(null, '', '/');
    window.localStorage.removeItem(MOCK_PICK_KEY);
  });

  it('reads ?mockPick= aliases and absolute paths, then storage, else nothing', () => {
    expect(readMockPick()).toBeNull();
    window.history.replaceState(null, '', '/?mockPick=downloads');
    expect(readMockPick()).toBe(MOCK_SAMPLE);
    window.history.replaceState(null, '', '/?mockPick=loose');
    expect(readMockPick()).toBe(MOCK_DOWNLOADS);
    window.history.replaceState(null, '', '/?mockPick=mixed');
    expect(readMockPick()).toBe(MOCK_MIXED);
    window.history.replaceState(null, '', '/?mockPick=relative/path');
    expect(readMockPick()).toBeNull();
    window.localStorage.setItem(MOCK_PICK_KEY, '/Users/demo/Other');
    expect(readMockPick()).toBe('/Users/demo/Other');
  });

  it('open folder picks the asked pile, which has no known layout', async () => {
    window.history.replaceState(null, '', '/?mockPick=loose');
    const b = backend();
    const path = await b.pickFolder();
    expect(path).toBe(MOCK_DOWNLOADS);
    expect(await backend({ folder: MOCK_MIXED }).pickFolder()).toBe(MOCK_MIXED);
    window.history.replaceState(null, '', '/');
    expect(await backend().pickFolder()).toBe(MOCK_LIBRARIES[0].path);
  });
});

describe('mock organise: the sample as it is now', () => {
  it('opens as a library and plans only the loose files', async () => {
    const b = backend();
    const lib = await b.openLibrary(MOCK_SAMPLE);
    expect(lib.profile.id).toBe('anbernic-stock');
    expect(lib.systems.map((s) => [s.folder, s.roms])).toEqual([
      ['GB', 6],
      ['GBA', 146],
      ['GBC', 2],
      ['SFC', 2],
    ]);
    // 5 GBA zips, the patch, the web page, the .rar and 3 disc archives.
    expect(lib.loose_files).toBe(11);
    const probe = await b.probeFolder(MOCK_SAMPLE);
    expect(probe.sorted_folders).toEqual(['GB', 'GBA', 'GBC', 'SFC']);

    const { plan } = await b.organizePlan({
      source: MOCK_SAMPLE,
      layout: 'anbernic-stock',
      mode: 'move',
    });
    expect(plan.counts).toMatchObject({ auto: 8, review: 0, left: 3 });
    expect(plan.sorted_folders).toEqual([
      ['ROMS/GB', 'gb'],
      ['ROMS/GBA', 'gba'],
      ['ROMS/GBC', 'gbc'],
      ['ROMS/SFC', 'snes'],
    ]);
    // Nothing already in ROMS/ is part of the plan.
    expect(plan.items.every((i) => !i.sources[0]!.startsWith('ROMS/'))).toBe(
      true,
    );
    const by = (name: string) => plan.items.find((i) => i.sources[0] === name)!;
    expect(
      by('Pokemon Ultra Violet (1.22) LSA (Fire Red Hack).zip'),
    ).toMatchObject({
      action: 'auto',
      target: 'ROMS/GBA/Pokemon Ultra Violet (1.22) LSA (Fire Red Hack).zip',
      rename: null,
      method: 'header',
    });
    expect(by('Pokemon - Leaf Green Version (U) (V1.1) (1).zip').target).toBe(
      'ROMS/GBA/Pokemon - Leaf Green Version (U) (V1.1) (1).zip',
    );
    for (const [name, folder] of [
      ['Next Tetris, The.zip', 'PS'],
      ['Next Tetris, The_ On-Line Edition.zip', 'DREAMCAST'],
      ['Tetris Plus.zip', 'SATURN'],
    ] as const) {
      expect(by(name)).toMatchObject({ folder, transfer: { kind: 'extract' } });
    }
    const left = Object.fromEntries(
      plan.items.flatMap((i) =>
        i.action === 'left' ? [[i.sources[0], i.reason]] : [],
      ),
    );
    expect(left).toEqual({
      '3-D Tetris (USA).zip': { kind: 'not-a-rom', what: 'web-page' },
      'Pokemon Jupiter - 6.04 (Ruby Hack) (1).zip': {
        kind: 'not-a-rom',
        what: 'patch',
      },
      'Tetris Collection.rar': {
        kind: 'unsupported-archive',
        format: 'rar',
        need: 'unrar',
      },
    });
  });

  it('apply adds to the sorted folders and undo leaves them as they were', async () => {
    const b = backend();
    const { plan_id } = await b.organizePlan({
      source: MOCK_SAMPLE,
      layout: 'anbernic-stock',
      mode: 'move',
    });
    const done = ended(b);
    await b.organizeApply(plan_id, []);
    const view = await done;
    expect(view.report).toMatchObject({ moved: 5, extracted: 3, failed: 0 });
    const after = await b.openLibrary(MOCK_SAMPLE);
    expect(after.systems.map((s) => [s.folder, s.roms])).toEqual([
      ['DREAMCAST', 2],
      ['GB', 6],
      ['GBA', 151],
      ['GBC', 2],
      ['PS', 1],
      ['SATURN', 1],
      ['SFC', 2],
    ]);
    // The patch, the web page and the .rar stay loose, and so does
    // Tetris Plus.zip: the mock's Trash refuses an archive that big.
    expect(after.loose_files).toBe(4);

    await b.organizeUndo(view.job!, false);
    const back = await b.openLibrary(MOCK_SAMPLE);
    expect(back.systems.map((s) => [s.folder, s.roms])).toEqual([
      ['GB', 6],
      ['GBA', 146],
      ['GBC', 2],
      ['SFC', 2],
    ]);
    expect(back.loose_files).toBe(11);
  });
});

describe('mock organise: archives and disc games', () => {
  const CASES = [
    {
      name: 'Next Tetris, The.zip',
      size: 355_281_551,
      system: 'psx',
      folder: 'PS',
      serial: 'SLUS-00862',
      files: 12,
      bytes: 405_749_493,
      target: 'ROMS/PS/Next Tetris, The (USA).cue',
      deferred: false,
      twin: false,
    },
    {
      name: 'Next Tetris, The_ On-Line Edition.zip',
      size: 381_421_246,
      system: 'dreamcast',
      folder: 'DREAMCAST',
      serial: 'T-40214N',
      files: 16,
      bytes: 1_191_067_563,
      target: 'ROMS/DREAMCAST/Next Tetris, The - On-Line Edition (USA).gdi',
      deferred: true,
      twin: true,
    },
    {
      name: 'Tetris Plus.zip',
      size: 478_731_748,
      system: 'saturn',
      folder: 'SATURN',
      serial: 'T-5704H',
      files: 52,
      bytes: 591_881_790,
      target: 'ROMS/SATURN/Tetris Plus (USA).cue',
      deferred: true,
      twin: false,
    },
  ] as const;

  const extractOf = (item: OrganizeItem) => {
    if (item.transfer.kind !== 'extract') throw new Error('not an extract');
    return item.transfer;
  };

  it('downloads_has_three_disc_archives_with_real_numbers', async () => {
    const b = backend();
    const { plan } = await b.organizePlan({ source: MOCK_DOWNLOADS });
    for (const c of CASES) {
      const it = plan.items.find((i) => i.sources[0] === c.name)!;
      expect(it, c.name).toMatchObject({
        action: 'auto',
        kind: 'archive',
        size: c.size,
        system_id: c.system,
        folder: c.folder,
        method: 'header',
        target: c.target,
      });
      expect(it.disc?.serials).toEqual([c.serial]);
      const t = extractOf(it);
      expect(t.entries).toHaveLength(c.files);
      expect(t.bytes).toBe(c.bytes);
      expect(t.entries.reduce((n, e) => n + e.size, 0)).toBe(c.bytes);
      expect(t.archive_after).toBe('trash');
      expect(t.source).toEqual({ kind: 'builtin' });
      expect(t.m3u).toBeNull();
      for (const e of t.entries) {
        expect(e.target.startsWith(`ROMS/${c.folder}/`)).toBe(true);
        // Flat: the archive's game folder is dropped.
        expect(e.target.split('/')).toHaveLength(3);
        expect(e.crc32).toEqual(expect.any(Number));
      }
      expect(it.notes).toContainEqual({
        kind: 'container-mismatch',
        ext: 'zip',
        actual: '7z',
      });
      expect(t.sheet_check).toBe(c.deferred ? 'deferred' : 'read');
      expect(it.notes.some((n) => n.kind === 'sheet-not-read-yet')).toBe(
        c.deferred,
      );
      expect(it.notes.some((n) => n.kind === 'twin-sheets')).toBe(c.twin);
    }
    const dc = plan.items.find((i) => i.sources[0] === CASES[1].name)!;
    expect(dc.notes).toContainEqual({
      kind: 'twin-sheets',
      kept: ['gdi', 'cue'],
    });
    // The saved web page stays a web page.
    expect(
      plan.items.find((i) => i.sources[0] === '3-D Tetris (USA).zip'),
    ).toMatchObject({
      action: 'left',
      reason: { kind: 'not-a-rom', what: 'web-page' },
      transfer: { kind: 'as-is' },
    });
    // The cartridge zips move as they are.
    for (const i of plan.items.filter((i) => i.folder === 'GBA'))
      expect(i.transfer).toEqual({ kind: 'as-is' });
    expect(plan.counts).toMatchObject({ auto: 19, roms: 19, systems: 7 });

    // One folder per game on ES-DE: menus list it once, so no twin note.
    const esde = await b.organizePlan({
      source: MOCK_DOWNLOADS,
      layout: 'esde',
    });
    const dcEsde = esde.plan.items.find((i) => i.sources[0] === CASES[1].name)!;
    expect(dcEsde.target).toBe(
      'ROMs/dreamcast/Next Tetris, The - On-Line Edition (USA)/Next Tetris, The - On-Line Edition (USA).gdi',
    );
    expect(dcEsde.notes.some((n) => n.kind === 'twin-sheets')).toBe(false);
    // Copy mode never touches the archive.
    const copy = await b.organizePlan({ source: MOCK_DOWNLOADS, mode: 'copy' });
    for (const c of CASES) {
      const it = copy.plan.items.find((i) => i.sources[0] === c.name)!;
      expect(extractOf(it).archive_after).toBe('keep');
    }
  });

  it('rar_without_tool_left_with_need', async () => {
    const b = backend();
    b.organizeTouch(MOCK_DOWNLOADS, { add: ['Tetris DX (World).rar'] });
    const { plan } = await b.organizePlan({ source: MOCK_DOWNLOADS });
    expect(
      plan.items.find((i) => i.sources[0] === 'Tetris DX (World).rar'),
    ).toMatchObject({
      action: 'left',
      kind: 'archive',
      reason: { kind: 'unsupported-archive', format: 'rar', need: 'unrar' },
    });
  });

  it('tar_gz_cartridge_extracts', async () => {
    const b = backend();
    b.organizeTouch(MOCK_DOWNLOADS, { add: ['Tetris Worlds (Europe).tar.gz'] });
    const { plan } = await b.organizePlan({ source: MOCK_DOWNLOADS });
    const it = plan.items.find(
      (i) => i.sources[0] === 'Tetris Worlds (Europe).tar.gz',
    )!;
    expect(it).toMatchObject({
      action: 'auto',
      kind: 'archive',
      system_id: 'gba',
      folder: 'GBA',
      target: 'ROMS/GBA/Tetris Worlds (Europe).gba',
      disc: null,
    });
    const t = extractOf(it);
    expect(t.entries).toEqual([
      {
        index: 0,
        name: 'Tetris Worlds (Europe).gba',
        size: 4_194_304,
        crc32: expect.any(Number),
        target: 'ROMS/GBA/Tetris Worlds (Europe).gba',
      },
    ]);
    expect(t).toMatchObject({ bytes: 4_194_304, archive_after: 'trash' });

    const done = ended(b);
    const { plan_id } = await b.organizePlan({ source: MOCK_DOWNLOADS });
    await b.organizeApply(plan_id, []);
    const view = await done;
    expect(view.report?.items).toContainEqual({
      item: it.id,
      status: 'extracted',
      targets: [`${MOCK_DOWNLOADS}/ROMS/GBA/Tetris Worlds (Europe).gba`],
    });
    const lib = await b.openLibrary(MOCK_DOWNLOADS, 'anbernic-stock');
    expect(lib.entries.map((e) => e.rel_path)).toContain(
      'ROMS/GBA/Tetris Worlds (Europe).gba',
    );
  });

  it('extract_progress_is_byte_based', async () => {
    const b = backend();
    const events: OrganizeProgressEvent[] = [];
    await b.onOrganizeProgress((e) => events.push(e));
    const { plan_id, plan } = await b.organizePlan({ source: MOCK_DOWNLOADS });
    const done = ended(b);
    await b.organizeApply(plan_id, []);
    await done;
    const total = plan.counts.auto;
    for (const c of CASES) {
      const item = plan.items.find((i) => i.sources[0] === c.name)!;
      const mine = events.flatMap((e) =>
        e.phase === 'extract' && e.item === item.id ? [e] : [],
      );
      expect(mine.length).toBeGreaterThan(2);
      expect(mine[0]!.bytes_done).toBe(0);
      expect(mine.at(-1)!.bytes_done).toBe(c.bytes);
      for (const [n, e] of mine.entries()) {
        expect(e).toMatchObject({ plan_id, total, bytes_total: c.bytes });
        if (n > 0)
          expect(e.bytes_done).toBeGreaterThan(mine[n - 1]!.bytes_done);
      }
      // `done` counts the items before this one.
      expect(new Set(mine.map((e) => e.done)).size).toBe(1);
    }
  });

  it('apply_reports_extracted_and_one_extracted_kept', async () => {
    const b = backend();
    const { plan } = await b.organizePlan({ source: MOCK_DOWNLOADS });
    const view = await applied(b);
    expect(view.state).toBe('done');
    expect(view.report).toMatchObject({ moved: 16, extracted: 3, failed: 0 });
    const outcome = (name: string) => {
      const id = plan.items.find((i) => i.sources[0] === name)!.id;
      return view.report!.items.find((o) => o.item === id)!;
    };
    expect(outcome(CASES[0].name).status).toBe('extracted');
    expect(outcome(CASES[1].name).status).toBe('extracted');
    // The mock's Trash refuses archives over 450 MB: the archive stays.
    expect(outcome(CASES[2].name).status).toBe('extracted-kept');
    for (const c of CASES) {
      const o = outcome(c.name);
      expect(o.targets).toHaveLength(c.files);
      expect(o.targets).toContain(`${MOCK_DOWNLOADS}/${c.target}`);
    }
    // Trashed archives leave the pile; the kept one stays.
    const left = await b.organizePlan({ source: MOCK_DOWNLOADS, rescan: true });
    const names = left.plan.items.map((i) => i.sources[0]);
    expect(names).not.toContain(CASES[0].name);
    expect(names).not.toContain(CASES[1].name);
    expect(names).toContain(CASES[2].name);
    // Its files are already there: planned again, it collides.
    expect(
      left.plan.items.find((i) => i.sources[0] === CASES[2].name),
    ).toMatchObject({ action: 'left', reason: { kind: 'collision-set' } });

    const lib = await b.openLibrary(MOCK_DOWNLOADS, 'anbernic-stock');
    expect(lib.systems.map((s) => s.folder)).toEqual([
      'DREAMCAST',
      'GB',
      'GBA',
      'GBC',
      'PS',
      'SATURN',
      'SFC',
    ]);
    // The library lists the sheets, not the tracks.
    expect(
      lib.entries
        .filter((e) => /^ROMS\/(PS|DREAMCAST|SATURN)\//.test(e.rel_path))
        .map((e) => e.rel_path)
        .sort(),
    ).toEqual([
      'ROMS/DREAMCAST/Next Tetris, The - On-Line Edition (USA).cue',
      'ROMS/DREAMCAST/Next Tetris, The - On-Line Edition (USA).gdi',
      'ROMS/PS/Next Tetris, The (USA).cue',
      'ROMS/SATURN/Tetris Plus (USA).cue',
    ]);
  });

  it('undo_keeps_extracted_files_when_the_archive_place_is_taken', async () => {
    const b = backend();
    const { plan } = await b.organizePlan({ source: MOCK_DOWNLOADS });
    const gba = plan.items.find((i) => i.folder === 'GBA')!;
    const view = await applied(b);
    // Something else now sits where the PS1 archive and a GBA ROM were.
    b.organizeTouch(MOCK_DOWNLOADS, {
      add: [CASES[0].name, gba.sources[0]!],
    });
    const r = (await b.organizeUndo(view.job!, false)).report[0]!;
    // Like Rust: the trashed archive cannot come back, so its extracted
    // files stay; the moved ROM stays at its target.
    expect(r.archive_lost).toEqual([`${MOCK_DOWNLOADS}/${CASES[0].name}`]);
    expect(r.restored).not.toContain(`${MOCK_DOWNLOADS}/${CASES[0].name}`);
    expect(r.removed.some((x) => x.includes('/ROMS/PS/'))).toBe(false);
    expect(r.skipped_occupied).toEqual([`${MOCK_DOWNLOADS}/${gba.target!}`]);
    expect(r.restored).not.toContain(`${MOCK_DOWNLOADS}/${gba.sources[0]!}`);
    expect(r.restored).toHaveLength(16);
    expect(r.dirs_removed).not.toContain(`${MOCK_DOWNLOADS}/ROMS/PS`);
    expect(r.dirs_removed).not.toContain(`${MOCK_DOWNLOADS}/ROMS`);
    const lib = await b.openLibrary(MOCK_DOWNLOADS, 'anbernic-stock');
    expect(lib.entries.map((e) => e.rel_path).sort()).toEqual(
      [CASES[0].target, gba.target!].sort(),
    );
  });

  it('cancel_mid_extract_rolls_the_item_back', async () => {
    const b = backend();
    const { plan_id, plan } = await b.organizePlan({ source: MOCK_DOWNLOADS });
    const dc = plan.items.find((i) => i.sources[0] === CASES[1].name)!;
    // Cancelled once the Dreamcast decode is under way.
    await b.onOrganizeProgress((e) => {
      if (e.phase === 'extract' && e.item === dc.id && e.bytes_done > 0)
        void b.organizeCancelApply();
    });
    const done = ended(b);
    await b.organizeApply(plan_id, []);
    const view = await done;
    expect(view.state).toBe('cancelled');
    expect(view.report?.items.find((o) => o.item === dc.id)).toEqual({
      item: dc.id,
      status: 'skipped',
      reason: 'cancelled',
      targets: [],
    });
    // Nothing of it reached the target, and the archive is still loose.
    const lib = await b.openLibrary(MOCK_DOWNLOADS, 'anbernic-stock');
    expect(
      lib.entries.some((e) => e.rel_path.startsWith('ROMS/DREAMCAST/')),
    ).toBe(false);
    const again = await b.organizePlan({
      source: MOCK_DOWNLOADS,
      rescan: true,
    });
    expect(
      again.plan.items.find((i) => i.sources[0] === CASES[1].name),
    ).toMatchObject({ action: 'auto', transfer: { kind: 'extract' } });
    // Undo removes only what was written before the cancel.
    const undo = await b.organizeUndo(view.job!, false);
    expect(
      undo.report[0]!.removed.some((r) => r.includes('/ROMS/DREAMCAST/')),
    ).toBe(false);
  });

  it('undo_reports_archive_restored', async () => {
    const b = backend();
    const view = await applied(b);
    const undo = await b.organizeUndo(view.job!, false);
    const r = undo.report[0]!;
    expect(r.restored).toHaveLength(18);
    expect(r.restored).toContain(`${MOCK_DOWNLOADS}/${CASES[0].name}`);
    expect(r.restored).toContain(`${MOCK_DOWNLOADS}/${CASES[1].name}`);
    // Never sent to the Trash, so nothing to bring back.
    expect(r.restored).not.toContain(`${MOCK_DOWNLOADS}/${CASES[2].name}`);
    expect(r.removed).toHaveLength(12 + 16 + 52);
    expect(r.removed).toContain(`${MOCK_DOWNLOADS}/${CASES[1].target}`);
    expect(r.archive_lost).toEqual([]);
    expect(r.dirs_removed).toContain(`${MOCK_DOWNLOADS}/ROMS/SATURN`);
    const again = await b.organizePlan({
      source: MOCK_DOWNLOADS,
      rescan: true,
    });
    for (const c of CASES) {
      expect(
        again.plan.items.find((i) => i.sources[0] === c.name),
      ).toMatchObject({ action: 'auto', transfer: { kind: 'extract' } });
    }
  });
});
