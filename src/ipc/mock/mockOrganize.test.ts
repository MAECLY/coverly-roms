import { afterEach, describe, expect, it, onTestFinished, vi } from 'vitest';

import type {
  OrganizeItem,
  OrganizeJobView,
  OrganizeJobState,
  OrganizeProgressEvent,
} from '../types';
import {
  MOCK_DISCS,
  MOCK_DUPES_TITLE,
  MOCK_HUGE,
  MOCK_HUGE_TARGET,
  MOCK_ODD,
  MOCK_SWITCH,
  MOCK_SWITCH_TITLES,
} from './adjustFixture';
import { FIXTURE_NOW } from './fixture';
import {
  createMockBackend,
  MOCK_LIBRARIES,
  type MockBackend,
  type MockOptions,
} from './mockBackend';
import { MOCK_MODS } from './modsFixture';
import {
  MOCK_CURRENT,
  MOCK_CURRENT_TARGET,
  MOCK_DOWNLOADS,
  MOCK_MIXED,
  MOCK_MODERN,
  MOCK_PICK_KEY,
  MOCK_RETRO,
  MOCK_SAMPLE,
  readMockExtract,
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
  it('sets_a_pile_copy_aside_by_default_and_reports_it_on_request', async () => {
    const b = backend();
    const { plan } = await b.organizePlan({ source: MOCK_MIXED });
    expect(plan.counts).toMatchObject({ set_aside: 1, in_library: 0 });
    const copy = plan.items.find((i) => i.action === 'set-aside')!;
    expect(copy).toMatchObject({
      sources: ['Super Mario Land (World) (1).gb'],
      keeper: 'ROMS/GB/Super Mario Land (World).gb',
      relation: 'exact',
      dest: '_Duplicados/Super Mario Land (World) (1).gb',
      target: null,
    });
    expect(plan.duplicates).toHaveLength(1);
    const group = plan.duplicates[0]!;
    expect(group.members.map((m) => m.dest)).toEqual([
      null,
      `${MOCK_MIXED}/_Duplicados/Super Mario Land (World) (1).gb`,
    ]);
    expect(plan.set_aside_trash).toBe(false);

    const report = await b.organizePlan({
      source: MOCK_MIXED,
      duplicates: 'report',
    });
    expect(report.plan.counts.set_aside).toBe(0);
    expect(
      report.plan.items.find(
        (i) => i.action === 'left' && i.reason.kind === 'in-library',
      ),
    ).toMatchObject({
      reason: { path: 'ROMS/GB/Super Mario Land (World).gb' },
    });
    const off = await b.organizePlan({ source: MOCK_MIXED, duplicates: 'off' });
    expect(off.plan.duplicates).toEqual([]);
  });

  it('applies_and_undoes_a_set_aside_copy', async () => {
    const b = backend();
    const view = await applied(b, MOCK_MIXED);
    expect(view.report).toMatchObject({ set_aside: 1 });
    expect(
      view.report?.items.find((o) => o.status === 'quarantined'),
    ).toMatchObject({
      targets: [`${MOCK_MIXED}/_Duplicados/Super Mario Land (World) (1).gb`],
    });
    const undo = await b.organizeUndo(view.job!, false);
    expect(undo.report[0]?.restored).toContain(
      `${MOCK_MIXED}/Super Mario Land (World) (1).gb`,
    );
    const again = await b.organizePlan({ source: MOCK_MIXED, rescan: true });
    expect(again.plan.counts.set_aside).toBe(1);
  });

  it('sends_set_aside_copies_to_the_trash_when_the_setting_says', async () => {
    const b = backend();
    await b.updateSettings({ dedupe: { action: 'trash' } });
    const view = await applied(b, MOCK_MIXED);
    expect(view.report?.items.some((o) => o.status === 'trashed')).toBe(true);
  });

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
    // Without the duplicates step the copy stays as a plain duplicate.
    const { plan } = await b.organizePlan({
      source: MOCK_MIXED,
      duplicates: 'off',
    });
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

    // «No» to a huge item: left as not confirmed, and the next plan
    // without the decision proposes it again (never remembered).
    const declined = await b.organizePlan({ source: MOCK_MIXED }, [
      { item: first!.id, choice: 'decline' },
    ]);
    expect(declined.plan.items[first!.id]).toMatchObject({
      action: 'left',
      reason: { kind: 'not-confirmed' },
    });
    const again = await b.organizePlan({ source: MOCK_MIXED });
    expect(again.plan.items[first!.id]!.action).not.toBe('left');
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
    window.history.replaceState(null, '', '/?mockPick=retro');
    expect(readMockPick()).toBe(MOCK_RETRO);
    window.history.replaceState(null, '', '/?mockPick=modern');
    expect(readMockPick()).toBe(MOCK_MODERN);
    window.history.replaceState(null, '', '/?mockPick=current');
    expect(readMockPick()).toBe(MOCK_CURRENT);
    window.history.replaceState(null, '', '/?mockPick=huge');
    expect(readMockPick()).toBe(MOCK_HUGE);
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
    // 5 GBA zips, the patch, the web page, the .rar, a .tar.gz and 3 disc
    // archives.
    expect(lib.loose_files).toBe(12);
    const probe = await b.probeFolder(MOCK_SAMPLE);
    expect(probe.sorted_folders).toEqual(['GB', 'GBA', 'GBC', 'SFC']);

    const { plan } = await b.organizePlan({
      source: MOCK_SAMPLE,
      layout: 'anbernic-stock',
      mode: 'move',
    });
    expect(plan.counts).toMatchObject({ auto: 9, review: 0, left: 3 });
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
      confidence: 0.98,
    });
    expect(by('Pokemon - Leaf Green Version (U) (V1.1) (1).zip').target).toBe(
      'ROMS/GBA/Pokemon - Leaf Green Version (U) (V1.1) (1).zip',
    );
    expect(by('Kirby - Nightmare in Dream Land (USA).tar.gz')).toMatchObject({
      target: 'ROMS/GBA/Kirby - Nightmare in Dream Land (USA).gba',
      transfer: { kind: 'extract', archive_after: 'trash' },
    });
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
    expect(view.report).toMatchObject({ moved: 5, extracted: 4, failed: 0 });
    const after = await b.openLibrary(MOCK_SAMPLE);
    expect(after.systems.map((s) => [s.folder, s.roms])).toEqual([
      ['DREAMCAST', 2],
      ['GB', 6],
      ['GBA', 152],
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
    expect(back.loose_files).toBe(12);
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

describe('mock organise: extract pacing', () => {
  afterEach(() => {
    window.history.replaceState(null, '', '/');
  });

  const extractEvents = (events: OrganizeProgressEvent[]) => {
    const byItem = new Map<
      number,
      Extract<OrganizeProgressEvent, { phase: 'extract' }>[]
    >();
    for (const e of events)
      if (e.phase === 'extract')
        byItem.set(e.item, [...(byItem.get(e.item) ?? []), e]);
    return byItem;
  };

  it('emits_monotone_entries_done_ending_at_entries_total', async () => {
    const b = backend();
    const events: OrganizeProgressEvent[] = [];
    await b.onOrganizeProgress((e) => events.push(e));
    const view = await applied(b);
    expect(view.state).toBe('done');
    const byItem = extractEvents(events);
    expect(byItem.size).toBe(3);
    for (const list of byItem.values()) {
      const total = list[0]!.entries_total!;
      expect(total).toBeGreaterThan(1);
      expect(list.every((e) => e.entries_total === total)).toBe(true);
      const counts = list.map((e) => e.entries_done!);
      expect(counts[0]).toBe(0);
      expect(list[0]!.bytes_done).toBe(0);
      expect(counts.at(-1)).toBe(total);
      expect(list.at(-1)!.bytes_done).toBe(list.at(-1)!.bytes_total);
      expect(counts.every((c, i) => i === 0 || c >= counts[i - 1]!)).toBe(true);
      // Fast mode: one event per entry.
      for (let k = 1; k <= total; k += 1) expect(counts).toContain(k);
    }
  });

  it.each([
    ['', 25],
    ['?mockExtract=slow', 25],
    ['?mockExtract=fast', 'fast'],
    ['?mockExtract=80', 80],
    ['?mockExtract=-3', 25],
    ['?mockExtract=0', 25],
    ['?mockExtract=abc', 25],
  ])('reads_mock_extract %s', (search, want) => {
    window.history.replaceState(null, '', `/${search}`);
    expect(readMockExtract()).toEqual(want);
  });

  it('a_rate_run_paces_by_elapsed_time_and_cancels_mid_item', async () => {
    // A fake clock: 1 ms between reads, so the bytes per tick do not depend
    // on how slow the runner is (a real 8 MB entry could end in one tick).
    let clock = 0;
    const now = vi
      .spyOn(performance, 'now')
      .mockImplementation(() => (clock += 1));
    onTestFinished(() => now.mockRestore());
    const b = backend({ delayMs: 1, extractRate: 2000 });
    const { plan_id, plan } = await b.organizePlan({ source: MOCK_DOWNLOADS });
    const events: OrganizeProgressEvent[] = [];
    let first: number | null = null;
    await b.onOrganizeProgress((e) => {
      events.push(e);
      if (e.phase !== 'extract') return;
      first ??= e.item;
      // Cancelled once the first game's first track is staged.
      if (e.item === first && e.entries_done === 1)
        void b.organizeCancelApply();
    });
    const done = ended(b);
    await b.organizeApply(plan_id, []);
    const view = await done;
    expect(view.state).toBe('cancelled');
    const mine = extractEvents(events).get(first!)!;
    // Bytes move inside an entry before its ✓ (chunks paced by time).
    expect(mine.some((e) => e.entries_done === 0 && e.bytes_done > 0)).toBe(
      true,
    );
    expect(Math.max(...mine.map((e) => e.entries_done!))).toBeLessThan(
      mine[0]!.entries_total!,
    );
    expect(view.report?.items.find((o) => o.item === first)).toEqual({
      item: first,
      status: 'skipped',
      reason: 'cancelled',
      targets: [],
    });
    const item = plan.items.find((i) => i.id === first)!;
    const lib = await b.openLibrary(MOCK_DOWNLOADS, 'anbernic-stock');
    expect(
      lib.entries.some((e) => e.rel_path.startsWith(`ROMS/${item.folder}/`)),
    ).toBe(false);
  });

  it('a_cancel_during_the_check_pause_rolls_the_item_back', async () => {
    // Rust honours a cancel until phase C: one sent while the sheets and
    // names are checked (after the last entry) leaves nothing of the game.
    const b = backend({ delayMs: 1, extractRate: 2000 });
    const { plan_id, plan } = await b.organizePlan({ source: MOCK_DOWNLOADS });
    let first: number | null = null;
    await b.onOrganizeProgress((e) => {
      if (e.phase !== 'extract') return;
      first ??= e.item;
      if (e.item === first && e.entries_done === e.entries_total)
        void b.organizeCancelApply();
    });
    const done = ended(b);
    await b.organizeApply(plan_id, []);
    const view = await done;
    expect(view.state).toBe('cancelled');
    expect(view.report?.items.find((o) => o.item === first)).toEqual({
      item: first,
      status: 'skipped',
      reason: 'cancelled',
      targets: [],
    });
    const item = plan.items.find((i) => i.id === first)!;
    const lib = await b.openLibrary(MOCK_DOWNLOADS, 'anbernic-stock');
    expect(
      lib.entries.some((e) => e.rel_path.startsWith(`ROMS/${item.folder}/`)),
    ).toBe(false);
  });
});

describe('mock organise: the systems batch 1 retro pile', () => {
  const by = (items: OrganizeItem[], name: string) =>
    items.find((i) => i.sources[0] === name)!;

  it('plans the spec 9.3 pile like the Rust snapshot on Onion', async () => {
    const { plan } = await backend().organizePlan({
      source: MOCK_RETRO,
      layout: 'onion',
    });
    const verdicts = Object.fromEntries(
      plan.items.map((i) => [i.sources[0], [i.action, i.system_id, i.folder]]),
    );
    expect(verdicts).toEqual({
      'Arkanoid (1987).dsk': ['review', 'amstradcpc', 'CPC'],
      'DOOM.WAD': ['auto', 'doom', 'PRBOOM'],
      'Freedoom Phase 1': ['left', null, null],
      'Gran Turismo 4 (Europe).iso': ['auto', 'ps2', 'PS2'],
      'Manic Miner (1983)(Bug-Byte).tzx': ['auto', 'zxspectrum', 'ZXS'],
      'Prince of Persia.st': ['auto', 'atarist', 'ATARIST'],
      'Sailor Moon (Japan).st': ['auto', 'sufami', 'SUFAMI'],
      'Tempest 2000 (World).j64': ['auto', 'jaguar', 'JAGUAR'],
      'game.bin': ['left', null, null],
      'sfiii.zip': ['auto', 'mame', 'MAME'],
    });
    expect(plan.counts).toMatchObject({
      auto: 7,
      review: 1,
      left: 2,
      other_files: 2,
    });
    expect(plan.other_files).toEqual([
      'Cave Story/Doukutsu.exe',
      'Cave Story/data/Stage/0.pxm',
    ]);
    const freedoom = by(plan.items, 'Freedoom Phase 1');
    expect(freedoom.action === 'left' && freedoom.reason).toEqual({
      kind: 'folder-game',
      system: 'doom',
      folder: 'PRBOOM',
    });
    const arkanoid = by(plan.items, 'Arkanoid (1987).dsk');
    expect(arkanoid.alternatives.map(([s]) => s).slice(0, 1)).toEqual([
      'zxspectrum',
    ]);
    const sailor = by(plan.items, 'Sailor Moon (Japan).st');
    expect(sailor.why.map((e) => e.detail)).toEqual(['BANDAI SFC-ADX', 'st~']);
    expect(sailor.notes).toEqual([
      { kind: 'extension-contradicted', ext: 'st', system: 'atarist' },
    ]);
    expect(by(plan.items, 'sfiii.zip').companions).toEqual([
      { source: 'sfiii/sfiii.chd', target: 'Roms/MAME/sfiii/sfiii.chd' },
    ]);
    // Onion has no folder for PS2, Jaguar or PrBoom (spec 6.1).
    const noFolder = plan.items
      .filter((i) => i.notes.some((n) => n.kind === 'no-folder-on-device'))
      .map((i) => i.system_id)
      .sort();
    expect(noFolder).toEqual(['doom', 'jaguar', 'ps2']);
  });

  it('names the folders of each layout and drops the Onion notes elsewhere', async () => {
    const { plan } = await backend().organizePlan({
      source: MOCK_RETRO,
      layout: 'batocera',
    });
    const freedoom = by(plan.items, 'Freedoom Phase 1');
    expect(freedoom.action === 'left' && freedoom.reason).toEqual({
      kind: 'folder-game',
      system: 'doom',
      folder: 'prboom',
    });
    expect(by(plan.items, 'Tempest 2000 (World).j64').folder).toBe('jaguar');
    expect(
      plan.items.some((i) =>
        i.notes.some((n) => n.kind === 'no-folder-on-device'),
      ),
    ).toBe(false);
  });

  it('lists the new systems with their family for the grouped picker', async () => {
    const systems = await backend().listSystems();
    for (const [id, family] of [
      ['ps2', 'console'],
      ['zxspectrum', 'computer'],
      ['wasm4', 'engine'],
      ['doom', 'engine'],
      ['mame', 'arcade'],
    ] as const)
      expect(systems.find((s) => s.id === id)?.family, id).toBe(family);
    expect(new Set(systems.map((s) => s.id)).size).toBe(systems.length);
  });
});

describe('mock organise: the systems batch 2 modern pile', () => {
  const by = (items: OrganizeItem[], name: string) =>
    items.find((i) => i.sources[0] === name)!;
  const DEMONS = "Demon's Souls (BLUS-30443)";
  const SMO = 'Super Mario Odyssey [0100000000010000][v0].nsp';
  const SMO_UPDATE = 'Super Mario Odyssey [0100000000010800][v393216].nsp';

  it('plans the spec 9.3 pile like the Rust snapshot on ES-DE', async () => {
    const { plan } = await backend().organizePlan({
      source: MOCK_MODERN,
      layout: 'esde',
    });
    const moving = plan.items.filter((i) => i.action === 'auto');
    expect(new Set(moving.map((i) => i.system_id))).toEqual(
      new Set([
        'gamecube',
        'wii',
        'wiiu',
        '3ds',
        'switch',
        'psvita',
        'ps3',
        'xbox',
        'xbox360',
      ]),
    );
    expect(plan.counts).toMatchObject({ auto: 18, review: 0, left: 6 });
    expect(plan.counts.systems).toBe(9);
    expect(plan.updates).toBe(1);

    const demons = by(plan.items, DEMONS);
    expect(demons).toMatchObject({
      kind: 'folder',
      size: 23_410_000_000,
      system_id: 'ps3',
      folder: 'ps3',
      target: `ROMs/ps3/${DEMONS}.ps3`,
      title_id: { id: 'BLUS30443', source: 'param-sfo' },
    });
    expect(demons.notes).toEqual([
      { kind: 'folder-suffix', name: `${DEMONS}.ps3`, layout: 'ES-DE' },
    ]);
    expect(
      by(plan.items, 'Super Mario 3D World [0005000010145D00]'),
    ).toMatchObject({ kind: 'folder', size: 1_700_000_000, folder: 'wiiu' });
    expect(by(plan.items, 'Metroid Prime (USA).rvz').folder).toBe('gc');
    expect(by(plan.items, 'Mario Kart 7 (Europe).3ds').folder).toBe('n3ds');

    // The update right after its base, with its note and the ids' sources.
    const ids = plan.items.map((i) => i.sources[0]);
    expect(ids.indexOf(SMO_UPDATE)).toBe(ids.indexOf(SMO) + 1);
    expect(by(plan.items, SMO).title_id).toEqual({
      id: '0100000000010000',
      source: 'ticket-name',
    });
    expect(by(plan.items, SMO_UPDATE).notes).toEqual([
      { kind: 'update', of: 'Super Mario Odyssey' },
    ]);
    expect(by(plan.items, 'Pokemon Y (Europe).cia').notes).toEqual([
      { kind: 'encrypted-content', emulator: 'Azahar' },
    ]);
    expect(by(plan.items, 'NPUB30024.pkg').notes).toEqual([
      { kind: 'install-first', emulator: 'RPCS3' },
    ]);

    const reasons = plan.items.flatMap((i) =>
      i.action === 'left' ? [[i.sources[0], i.reason] as const] : [],
    );
    expect(Object.fromEntries(reasons)).toEqual({
      'Foo.pkg': { kind: 'not-a-rom', what: 'installer' },
      'NPUB30024.rap': { kind: 'key-material' },
      'model.3ds': { kind: 'not-a-rom', what: 'model3d' },
      'prod.keys': { kind: 'key-material' },
      'title.keys': { kind: 'key-material' },
      ux0: { kind: 'emulator-data', folder: 'ux0' },
    });
    expect(
      plan.items.some((i) =>
        i.notes.some((n) => n.kind === 'no-folder-on-device'),
      ),
    ).toBe(false);
  });

  it('Batocera has a Switch folder and keeps the .ps3 suffix', async () => {
    const { plan } = await backend().organizePlan({
      source: MOCK_MODERN,
      layout: 'batocera',
    });
    const smo = by(plan.items, SMO);
    expect(smo.folder).toBe('switch');
    expect(smo.target).toBe(`roms/switch/${SMO}`);
    expect(smo.notes).toEqual([]);
    expect(by(plan.items, 'Metroid Prime (USA).rvz').folder).toBe('gamecube');
    expect(by(plan.items, DEMONS).notes).toEqual([
      {
        kind: 'folder-suffix',
        name: `${DEMONS}.ps3`,
        layout: 'Batocera / Knulli / EmulationStation',
      },
    ]);
  });

  it('muOS has no Wii folder: the core still makes it and says so', async () => {
    const { plan } = await backend().organizePlan({
      source: MOCK_MODERN,
      layout: 'muos',
    });
    const galaxy = by(plan.items, 'Super Mario Galaxy (Europe).wbfs');
    expect(galaxy.notes).toEqual([
      { kind: 'no-folder-on-device', folder: 'wii' },
    ]);
    // 3DS is the batch's only muOS folder; no suffix on muOS.
    expect(by(plan.items, 'Mario Kart 7 (Europe).3ds').notes).toEqual([]);
    expect(by(plan.items, DEMONS).target).toBe(`ROMS/ps3/${DEMONS}`);
    // The update note follows the folder note, as the core writes them.
    expect(by(plan.items, SMO_UPDATE).notes).toEqual([
      { kind: 'no-folder-on-device', folder: 'switch' },
      { kind: 'update', of: 'Super Mario Odyssey' },
    ]);
  });

  it('applies a game folder as one item', async () => {
    const b = backend();
    const { plan_id } = await b.organizePlan({
      source: MOCK_MODERN,
      layout: 'esde',
    });
    const done = ended(b);
    await b.organizeApply(plan_id, []);
    const view = await done;
    expect(view.state).toBe('done');
    expect(view.report?.moved).toBe(18);
  });
});

describe('mock organise size gate (systems batch 3 D23)', () => {
  /** What Rust's `large_transfer` sums: automatic items with a target. */
  function written(items: readonly OrganizeItem[], crosses: boolean): number {
    return items
      .filter((i) => i.action === 'auto' && i.target !== null)
      .reduce(
        (n, i) =>
          n +
          (i.transfer.kind === 'extract'
            ? i.transfer.bytes
            : crosses
              ? i.size
              : 0),
        0,
      );
  }

  it('asks before a copy of 10 GB or more, then applies with the bytes', async () => {
    const b = backend();
    const { plan_id, plan } = await b.organizePlan({
      source: MOCK_MODERN,
      layout: 'esde',
      mode: 'copy',
    });
    const bytes = written(plan.items, true);
    expect(bytes).toBeGreaterThanOrEqual(10_000_000_000);
    const items = plan.items.filter(
      (i) => i.action === 'auto' && i.target !== null,
    ).length;
    expect(plan.warnings).toContainEqual({
      kind: 'large-transfer',
      bytes,
      items,
    });
    // The PS3 folder and the XCI alone write over 10 GB: each is asked
    // on its own.
    const big = plan.items.filter(
      (i) => i.action === 'auto' && i.target !== null && i.size >= 1e10,
    );
    expect(big.map((i) => i.sources[0])).toEqual([
      "Demon's Souls (BLUS-30443)",
      'Zelda BotW.xci',
    ]);
    const hugeIds = big.map((i) => i.id);
    const refused = {
      code: 'large-transfer',
      largeTransfer: {
        bytes,
        items,
        copies: true,
        free: null,
        volume: 'Modern',
        huge: big.map((i) => ({
          item: i.id,
          name: i.sources[0],
          system: i.system_id,
          bytes: i.size,
          extract: false,
          followers: [],
        })),
        rest: bytes - big.reduce((n, i) => n + i.size, 0),
      },
    };
    await expect(b.organizeApply(plan_id, [])).rejects.toMatchObject(refused);
    await expect(
      b.organizeApply(plan_id, [], { bytes: bytes - 1, items: hugeIds }),
    ).rejects.toMatchObject(refused);
    // Every byte, but a huge item not named: still refused.
    await expect(
      b.organizeApply(plan_id, [], { bytes, items: hugeIds.slice(1) }),
    ).rejects.toMatchObject(refused);
    // Nothing was written: no job, and the plan is still there to apply.
    expect(await b.jobHistory()).toEqual([]);
    const done = ended(b);
    expect(
      (await b.organizeApply(plan_id, [], { bytes, items: hugeIds })).state,
    ).toBe('applying');
    expect((await done).state).toBe('done');
  });

  it('names the target volume and its free space for a move to another drive', async () => {
    const b = backend();
    const { plan_id, plan } = await b.organizePlan({
      source: MOCK_MODERN,
      layout: 'esde',
      target: '/Volumes/TinyExt',
      mode: 'move',
    });
    expect(plan.mode).toBe('move');
    await expect(b.organizeApply(plan_id, [])).rejects.toMatchObject({
      code: 'large-transfer',
      largeTransfer: {
        bytes: written(plan.items, true),
        copies: false,
        free: 412_100_000_000,
        volume: 'TinyExt',
      },
    });
  });

  it('never asks for a move on the same volume or a small copy', async () => {
    const b = backend();
    const moved = await b.organizePlan({ source: MOCK_MODERN, layout: 'esde' });
    expect(moved.plan.warnings.map((w) => w.kind)).not.toContain(
      'large-transfer',
    );
    const small = await b.organizePlan({
      source: MOCK_DOWNLOADS,
      mode: 'copy',
    });
    expect(small.plan.warnings.map((w) => w.kind)).not.toContain(
      'large-transfer',
    );
    const done = ended(b);
    await b.organizeApply(small.plan_id, []);
    expect((await done).state).toBe('done');
  });
});

describe('mock current-generation pile (systems batch 3 §9.3)', () => {
  const by = (items: OrganizeItem[], name: string) =>
    items.find((i) => i.sources[0] === name)!;
  const BLOODBORNE = 'UP9000-CUSA00900_00-BLOODBORNE000000';
  const XVC = '4f0b3e0a-1a2b-4c3d-8e9f-c1d2e3f4a5b6';
  const PS4_PKG = 'EP0002-CUSA04447_00-GRAVITYRUSH00000.pkg';
  const PS5_PKG = 'UP9000-PPSA01284_00-RATCHET000000000.pkg';

  afterEach(() => {
    window.history.replaceState(null, '', '/');
  });

  it('plans the pile like the Rust snapshot on ES-DE, onto TinyExt', async () => {
    const { plan } = await backend().organizePlan({
      source: MOCK_CURRENT,
      layout: 'esde',
    });
    // Another drive: the automatic mode copies, like Rust.
    expect(plan.target_root).toBe(MOCK_CURRENT_TARGET);
    expect(plan.mode).toBe('copy');
    expect(plan.counts).toMatchObject({ auto: 12, review: 1, left: 5 });
    expect(plan.counts.systems).toBe(4);
    expect(plan.updates).toBe(3);

    // The update and the DLC right after their base, then the mods folder.
    const names = plan.items.map((i) => i.sources[0]);
    expect(names.slice(0, 6)).toEqual([
      XVC,
      'CUSA00900',
      'CUSA00900-UPDATE',
      'CUSA00900-DLC-ABC',
      'CUSA00900-mods',
      PS4_PKG,
    ]);
    expect(names.indexOf('PPSA01284 DLC')).toBe(names.indexOf('PPSA01284') + 1);

    expect(by(plan.items, 'CUSA00900')).toMatchObject({
      kind: 'folder',
      size: 48_120_000_000,
      system_id: 'ps4',
      target: 'ROMs/ps4/CUSA00900',
      title_id: { id: 'CUSA00900', source: 'param-sfo' },
      content_id: BLOODBORNE,
      notes: [],
    });
    expect(by(plan.items, 'CUSA00900-UPDATE')).toMatchObject({
      size: 14_800_000_000,
      target: 'ROMs/ps4/CUSA00900-UPDATE',
      notes: [{ kind: 'update', of: 'Bloodborne' }],
    });
    expect(by(plan.items, PS4_PKG)).toMatchObject({
      size: 9_020_000_000,
      notes: [{ kind: 'extract-first' }],
      title_id: { id: 'CUSA04447', source: 'param-sfo' },
    });
    expect(by(plan.items, 'PPSA01284').notes).toEqual([
      { kind: 'pro-enhanced' },
      { kind: 'no-folder-on-device', folder: 'ps5' },
    ]);
    expect(by(plan.items, 'PPSA01284').title_id).toEqual({
      id: 'PPSA01284',
      source: 'param-json',
    });
    expect(by(plan.items, 'PPSA01284 DLC').notes).toEqual([
      { kind: 'no-folder-on-device', folder: 'ps5' },
      { kind: 'dlc', of: 'Returnal' },
    ]);
    for (const name of [PS5_PKG, 'PPSA09519.exfat'])
      expect(by(plan.items, name).notes, name).toEqual([
        { kind: 'no-emulator' },
        { kind: 'no-folder-on-device', folder: 'ps5' },
      ]);
    expect(by(plan.items, 'Forza Horizon 5')).toMatchObject({
      system_id: 'xboxseries',
      target: 'ROMs/xboxseries/Forza Horizon 5',
      size: 110_400_000_000,
      title_id: { id: '2B3C4D5E', source: 'microsoft-game-config' },
    });
    expect(by(plan.items, 'Halo 5 Guardians').target).toBe(
      'ROMs/xboxone/Halo 5 Guardians',
    );
    expect(by(plan.items, 'Sunset Overdrive').why[0]!.detail).toBe(
      'AppxManifest.xml',
    );
    // The XVC waits for a choice between Xbox One and Xbox Series.
    expect(by(plan.items, XVC)).toMatchObject({
      action: 'review',
      system_id: 'xboxone',
      alternatives: [['xboxseries', 0.95]],
      notes: [{ kind: 'no-emulator' }],
      content_id: XVC,
    });

    const reasons = plan.items.flatMap((i) =>
      i.action === 'left' ? [[i.sources[0], i.reason] as const] : [],
    );
    expect(Object.fromEntries(reasons)).toEqual({
      'CUSA00900-mods': { kind: 'game-overlay', of: 'Bloodborne' },
      'Microsoft Flight Simulator': { kind: 'not-a-rom', what: 'pc-game-data' },
      'UP9000-CUSA00900_00.rif': { kind: 'key-material' },
      shadPS4: { kind: 'emulator-data', folder: 'shadPS4' },
      'x.cik': { kind: 'key-material' },
    });
    // 187,42 GB over 12 items: Organizar asks first.
    expect(plan.warnings).toContainEqual({
      kind: 'large-transfer',
      bytes: 187_420_000_000,
      items: 12,
    });
  });

  it('Batocera renames the base, its update follows and the DLC goes to DLC/', async () => {
    const { plan } = await backend().organizePlan({
      source: MOCK_CURRENT,
      layout: 'batocera',
    });
    const layout = 'Batocera / Knulli / EmulationStation';
    expect(by(plan.items, 'CUSA00900')).toMatchObject({
      target: 'roms/ps4/CUSA00900.ps4',
      notes: [{ kind: 'folder-suffix', name: 'CUSA00900.ps4', layout }],
    });
    expect(by(plan.items, 'CUSA00900-UPDATE')).toMatchObject({
      target: 'roms/ps4/CUSA00900.ps4-UPDATE',
      notes: [{ kind: 'update', of: 'Bloodborne' }],
    });
    expect(by(plan.items, 'CUSA00900-DLC-ABC')).toMatchObject({
      target: 'roms/ps4/DLC/CUSA00900/CUSA00900-DLC-ABC',
      notes: [
        { kind: 'dlc-folder', layout },
        { kind: 'dlc', of: 'Bloodborne' },
      ],
    });
    expect(by(plan.items, 'Halo 5 Guardians').notes).toEqual([
      { kind: 'no-folder-on-device', folder: 'xboxone' },
    ]);
    expect(by(plan.items, XVC).notes).toEqual([
      { kind: 'no-emulator' },
      { kind: 'no-folder-on-device', folder: 'xboxone' },
    ]);
    // ES-DE has no suffix and no DLC folder.
    const esde = await backend().organizePlan({
      source: MOCK_CURRENT,
      layout: 'esde',
    });
    expect(by(esde.plan.items, 'CUSA00900-DLC-ABC').target).toBe(
      'ROMs/ps4/CUSA00900-DLC-ABC',
    );
  });

  it('asks before copying 187,42 GB to TinyExt, then applies with the bytes', async () => {
    const b = backend();
    const { plan_id } = await b.organizePlan({
      source: MOCK_CURRENT,
      layout: 'esde',
    });
    const { plan } = await b.organizePlan({
      source: MOCK_CURRENT,
      layout: 'esde',
    });
    const id = (name: string) => by(plan.items, name).id;
    const refused = {
      code: 'large-transfer',
      largeTransfer: {
        bytes: 187_420_000_000,
        items: 12,
        copies: true,
        free: 412_100_000_000,
        volume: 'TinyExt',
        // Bloodborne with the update that follows it, then Forza.
        huge: [
          {
            item: id('CUSA00900'),
            name: 'CUSA00900',
            system: 'ps4',
            bytes: 62_920_000_000,
            extract: false,
            followers: [id('CUSA00900-UPDATE')],
          },
          {
            item: id('Forza Horizon 5'),
            name: 'Forza Horizon 5',
            system: 'xboxseries',
            bytes: 110_400_000_000,
            extract: false,
            followers: [],
          },
        ],
        rest: 14_100_000_000,
      },
    };
    await expect(b.organizeApply(plan_id, [])).rejects.toMatchObject(refused);
    const done = ended(b);
    await b.organizeApply(plan_id, [], {
      bytes: 187_420_000_000,
      items: [id('CUSA00900'), id('Forza Horizon 5')],
    });
    expect((await done).state).toBe('done');
  });

  it('?mockSameVolume=1 keeps the pile on its drive: a move, no question', async () => {
    window.history.replaceState(
      null,
      '',
      '/?mockPick=current&mockSameVolume=1',
    );
    const b = backend();
    const { plan_id, plan } = await b.organizePlan({
      source: MOCK_CURRENT,
      layout: 'esde',
    });
    expect(plan.target_root).toBe(MOCK_CURRENT);
    expect(plan.mode).toBe('move');
    expect(plan.warnings.map((w) => w.kind)).not.toContain('large-transfer');
    const done = ended(b);
    expect((await b.organizeApply(plan_id, [])).state).toBe('applying');
    expect((await done).state).toBe('done');
  });
});

describe('mock huge pile (adjustments D15–D17, ?mockPick=huge)', () => {
  const by = (items: OrganizeItem[], name: string) =>
    items.find((i) => i.sources[0] === name)!;

  it('copies to «Juegos» and asks the PS4 folders and the zipped PS3 disc one by one', async () => {
    const b = backend();
    const { plan_id, plan } = await b.organizePlan({
      source: MOCK_HUGE,
      layout: 'esde',
    });
    expect(plan.target_root).toBe(MOCK_HUGE_TARGET);
    expect(plan.mode).toBe('copy');
    const ps3 = by(plan.items, "Demon's Souls (BLUS-30443).zip");
    expect(ps3).toMatchObject({
      action: 'auto',
      system_id: 'ps3',
      target: 'ROMs/ps3/BLUS30443',
      transfer: { kind: 'extract', folder_game: true },
    });
    const extract = ps3.transfer.kind === 'extract' ? ps3.transfer.bytes : 0;
    expect(extract).toBe(7_410_181_124);
    const err = await b.organizeApply(plan_id, []).catch((e: unknown) => e);
    expect(err).toMatchObject({
      code: 'large-transfer',
      largeTransfer: {
        copies: true,
        free: 120_000_000_000,
        volume: 'Juegos',
        // Name order, as the plan lists them.
        huge: [
          { name: 'Bloodborne', bytes: 52_400_000_000, extract: false },
          {
            name: "Demon's Souls (BLUS-30443).zip",
            bytes: extract,
            extract: true,
          },
          { name: 'Ghost of Tsushima', bytes: 61_200_000_000 },
          { name: 'The Last of Us Part II', bytes: 38_300_000_000 },
        ],
        // The two PS2 discs: under 10 GB, no aggregate question.
        rest: 6_920_000_000,
      },
    });
    expect(await b.jobHistory()).toEqual([]);
  });

  it('applies with a «No» declined and the rest confirmed from the numbers', async () => {
    const b = backend();
    const { plan_id, plan } = await b.organizePlan({
      source: MOCK_HUGE,
      layout: 'esde',
    });
    const huge = (
      (await b.organizeApply(plan_id, []).catch((e: unknown) => e)) as {
        largeTransfer: {
          huge: { item: number; bytes: number }[];
          rest: number;
        };
      }
    ).largeTransfer;
    const [blood, ...others] = huge.huge;
    const done = ended(b);
    await b.organizeApply(plan_id, [{ item: blood!.item, choice: 'decline' }], {
      bytes: huge.rest + others.reduce((n, h) => n + h.bytes, 0),
      items: others.map((h) => h.item),
    });
    const view = await done;
    expect(view.state).toBe('done');
    const bloodborne = by(plan.items, 'Bloodborne').id;
    expect(
      view.report?.items.find((o) => o.item === bloodborne),
    ).toBeUndefined();
    expect(view.report?.copied).toBe(4);
    expect(view.report?.extracted).toBe(1);
  });

  it('a declined PS4 base leaves the overlay that follows it', async () => {
    const b = backend();
    const { plan } = await b.organizePlan({
      source: MOCK_CURRENT,
      layout: 'esde',
    });
    const base = by(plan.items, 'CUSA00900').id;
    const declined = await b.organizePlan(
      { source: MOCK_CURRENT, layout: 'esde' },
      [{ item: base, choice: 'decline' }],
    );
    for (const name of ['CUSA00900', 'CUSA00900-UPDATE'])
      expect(by(declined.plan.items, name)).toMatchObject({
        action: 'left',
        reason: { kind: 'not-confirmed' },
      });
  });
});

describe('mock adjustments piles (spec §9.4)', () => {
  const by = (items: OrganizeItem[], name: string) =>
    items.find((i) => i.sources[0] === name)!;
  const notes = (i: OrganizeItem) => i.notes.map((n) => n.kind);

  it('?mockPick names every adjustments pile', () => {
    onTestFinished(() => window.history.replaceState(null, '', '/'));
    for (const [pick, path] of [
      ['switch', MOCK_SWITCH],
      ['odd', MOCK_ODD],
      ['discs', MOCK_DISCS],
      ['dupes-title', MOCK_DUPES_TITLE],
    ] as const) {
      window.history.replaceState(null, '', `/?mockPick=${pick}`);
      expect(readMockPick()).toBe(path);
    }
  });

  it('switch: catalogue names, an NCA-resolved cartridge, a part, homebrew and one «Cifrado»', async () => {
    const b = backend();
    const { plan } = await b.organizePlan({
      source: MOCK_SWITCH,
      layout: 'esde',
    });
    const zelda = by(plan.items, 'Zelda BotW [01007EF00011E000][v0].nsp');
    expect(zelda).toMatchObject({
      action: 'auto',
      system_id: 'switch',
      catalogue_name: MOCK_SWITCH_TITLES['01007EF00011E000'],
      title_source: 'titledb',
    });
    const cart = by(plan.items, 'Cartridge.xci');
    expect(cart).toMatchObject({
      catalogue_name: 'Super Mario Odyssey',
      title_id: { id: '0100000000010000', source: 'nca-catalog' },
      title_source: 'titledb',
    });
    const update = by(
      plan.items,
      'Zelda BotW Update [01007EF00011E800][v786432].nsp',
    );
    expect(update.notes).toContainEqual({
      kind: 'update',
      of: 'The Legend of Zelda Breath of the Wild',
    });
    expect(
      plan.items.find((i) => i.sources[0]!.endsWith('.nca')),
    ).toMatchObject({
      action: 'left',
      reason: { kind: 'game-part', part: 'nca', system: 'switch' },
    });
    expect(notes(by(plan.items, 'hbmenu.nro'))).toEqual(['homebrew']);
    const encrypted = plan.items.filter((i) =>
      i.notes.some((n) => n.kind === 'encrypted-name'),
    );
    expect(encrypted.map((i) => i.sources[0])).toEqual([
      '0100F2C0115B6000.nsp',
    ]);
    expect(encrypted[0]!.title_source).toBeUndefined();
  });

  it('odd: one row per loose format, homebrew moves and parts stay', async () => {
    const b = backend();
    const { plan } = await b.organizePlan({ source: MOCK_ODD, layout: 'esde' });
    expect(plan.items).toHaveLength(18);
    const homebrew = plan.items
      .filter((i) => notes(i).includes('homebrew'))
      .map((i) => [i.sources[0], i.system_id, i.action]);
    expect(homebrew).toEqual([
      ['boot.dol', 'wii', 'auto'],
      ['Checkpoint.nro', 'switch', 'auto'],
      ['FBI.3dsx', '3ds', 'auto'],
      ['Homebrew Launcher.rpx', 'wiiu', 'auto'],
      ['Swiss.dol', 'gamecube', 'auto'],
      ['VitaShell.self', 'psvita', 'auto'],
      ['XBMC4Xbox.xbe', 'xbox', 'auto'],
    ]);
    const parts = plan.items.flatMap((i) =>
      i.action === 'left' && i.reason.kind === 'game-part'
        ? [[i.sources[0], i.reason.part]]
        : [],
    );
    expect(parts).toEqual([
      ['9a3f1c2d4e5b6a7980a1b2c3d4e5f607.nca', 'nca'],
      ['eboot.bin', 'exe-fs'],
      ['kernel_util.prx', 'module'],
      ['libgui.rpl', 'library'],
      ['main.npdm', 'exe-fs'],
      ['main.nso', 'exe-fs'],
      ['PCSE00120/game.psvimg', 'backup'],
      ['title.tmd', 'title-meta'],
      ['xam.xex', 'module'],
    ]);
    expect(by(plan.items, 'Crash Bandicoot.pbp')).toMatchObject({
      action: 'auto',
      system_id: 'psx',
      title_id: { id: 'SCUS94900', source: 'pbp' },
    });
    expect(notes(by(plan.items, 'Halo 3 TU3.xex'))).toEqual(['update']);
  });

  it('discs: sets move with their m3u, the name-only set waits, a missing disc says so', async () => {
    const b = backend();
    const { plan_id, plan } = await b.organizePlan({
      source: MOCK_DISCS,
      layout: 'esde',
    });
    const saga = by(plan.items, 'Panzer Dragoon Saga (USA) (Disc 1).chd');
    expect(saga).toMatchObject({
      action: 'auto',
      kind: 'set',
      system_id: 'saturn',
      target: 'ROMs/saturn/Panzer Dragoon Saga (USA) (Disc 1).chd',
      playlist: { path: 'ROMs/saturn/Panzer Dragoon Saga (USA).m3u' },
    });
    expect(saga.sources).toHaveLength(4);
    expect(saga.set_targets).toHaveLength(3);
    expect(saga.playlist!.discs).toEqual([saga.target, ...saga.set_targets]);
    const tales = by(plan.items, 'Tales of Symphonia (USA) (Disc 1).rvz');
    expect(tales.playlist!.path).toBe('ROMs/gc/Tales of Symphonia (USA).m3u');
    expect(by(plan.items, 'Grandia (Japan) (Disc 1).chd').notes).toEqual([
      { kind: 'missing-disc', n: 2 },
    ]);
    expect(
      by(plan.items, 'Lunar - The Silver Star (USA) (Disc 1).chd'),
    ).toMatchObject({ action: 'review', notes: [{ kind: 'disc-set' }] });

    const done = ended(b);
    await b.organizeApply(plan_id, []);
    const view = await done;
    expect(view.state).toBe('done');
    const written = view.report!.items.flatMap((o) => o.targets);
    expect(written).toContain(
      `${plan.target_root}/ROMs/saturn/Panzer Dragoon Saga (USA).m3u`,
    );
    expect(written).toContain(
      `${plan.target_root}/ROMs/saturn/Panzer Dragoon Saga (USA) (Disc 4).chd`,
    );
  });

  it('dupes-title: copies matched by title id and by folder are set aside and undo brings them back', async () => {
    const b = backend();
    const { plan_id, plan } = await b.organizePlan({
      source: MOCK_DUPES_TITLE,
    });
    const aside = plan.items
      .filter((i) => i.action === 'set-aside')
      .map((i) => i.sources[0]);
    expect(aside).toEqual([
      'Bloodborne (CUSA00900)',
      'Zelda (copia) [01007EF00011E000][v0].nsp',
    ]);
    expect(plan.duplicates.map((g) => g.basis).sort()).toEqual([
      'folder-tree',
      'title-id',
    ]);
    const title = plan.duplicates.find((g) => g.basis === 'title-id')!;
    expect(title.action).toEqual({ kind: 'act' });

    // Moved on its own volume: no size question.
    const done = ended(b);
    expect((await b.organizeApply(plan_id, [])).state).toBe('applying');
    const view = await done;
    expect(view.state).toBe('done');
    expect(view.report).toMatchObject({ set_aside: 2 });
    const undo = await b.organizeUndo(view.job!, false);
    expect(undo.report[0]?.restored).toEqual(
      expect.arrayContaining([
        `${MOCK_DUPES_TITLE}/Bloodborne (CUSA00900)`,
        `${MOCK_DUPES_TITLE}/Zelda (copia) [01007EF00011E000][v0].nsp`,
      ]),
    );
    const again = await b.organizePlan({
      source: MOCK_DUPES_TITLE,
      rescan: true,
    });
    expect(again.plan.counts.set_aside).toBe(2);
  });
});

describe('mock mods pile (mods spec §9.3)', () => {
  afterEach(() => {
    window.history.replaceState(null, '', '/');
  });

  const one = (items: OrganizeItem[], name: string): OrganizeItem => {
    const found = items.find((i) => i.sources[0] === name);
    if (!found) throw new Error(`no item ${name}`);
    return found;
  };
  const SD3 = 'Seiken Densetsu 3 (Japan).sfc';
  const SD3_BPS = 'Seiken Densetsu 3 (Japan) [T-En by DeJap].bps';
  const TMZ_IPS = 'TMZ English v1.01.ips';
  const FF5 = 'Final Fantasy V (Japan).sfc';
  const FF5_IPS = ['FF5 Restored Script.ips', 'FF5 Sound Restoration.ips'];
  const NES_BPS = 'Mother (Japan) [T-En].bps';
  const MGS = 'Metal Gear Solid (USA) (Disc 1).chd';
  const POKEMON = 'Pokemon Y (World).cia';
  const ACNH = 'Animal Crossing New Horizons [01006F8002326000].nsp';
  const ACNH_UPD =
    'Animal Crossing New Horizons [01006F8002326800][v1966080].nsp';
  const ACNH_ROMFS = 'atmosphere/contents/01006F8002326000/romfs';
  const ACNH_CHEATS =
    'atmosphere/contents/01006F8002326000/cheats/7F6F4D3C2B1A0908.txt';
  const GR2 = 'CUSA04943';
  const XENO = 'Xenoblade Chronicles 3 (World)';

  it('is what ?mockPick=mods opens', () => {
    window.history.replaceState(null, '', '/?mockPick=mods');
    expect(readMockPick()).toBe(MOCK_MODS);
  });

  it('links every spec case to its game on the Coverly layout', async () => {
    const b = backend();
    const { plan } = await b.organizePlan({
      source: MOCK_MODS,
      layout: 'coverly-library',
    });
    const { items } = plan;
    // Unique ids; every `game.of` is a base of the same system.
    expect(new Set(items.map((i) => i.id)).size).toBe(items.length);
    for (const i of items.filter((i) => i.game?.of !== undefined)) {
      const base = items.find((b) => b.id === i.game!.of);
      expect(base, i.sources[0]).toBeDefined();
      expect(base!.game?.of).toBeUndefined();
      expect(base!.system_id).toBe(i.system_id);
    }
    const id = (name: string) => one(items, name).id;

    // SNES: a BPS by its source CRC32, an IPS by its readme; retro bases
    // get no game folder, so they stay with their link.
    expect(one(items, SD3_BPS)).toMatchObject({
      action: 'left',
      reason: { kind: 'addon-no-place', addon: 'patch' },
      game: {
        role: 'mod',
        of: id(SD3),
        addon: {
          kind: 'patch',
          format: 'bps',
          key_text: 'CRC32 D0176B24',
          via: 'source-crc',
        },
      },
      notes: [{ kind: 'patch-of', of: 'Seiken Densetsu 3 (Japan)' }],
    });
    expect(one(items, TMZ_IPS).game?.addon).toMatchObject({
      format: 'ips',
      via: 'readme',
    });
    // NES: a headered ROM and a patch for the headerless one.
    expect(one(items, NES_BPS)).toMatchObject({
      game: { addon: { header: 'needs-headerless' } },
    });
    expect(one(items, NES_BPS).notes).toContainEqual({
      kind: 'header-mismatch',
      needs: 'needs-headerless',
    });
    // Two IPS for one ROM.
    for (const n of FF5_IPS) expect(one(items, n).game?.of).toBe(id(FF5));
    // PS1: cheats and a texture pack folder by serial.
    expect(one(items, 'SLUS-00594.cht')).toMatchObject({
      game: { role: 'cheat', of: id(MGS), addon: { key_text: 'SLUS-00594' } },
    });
    expect(one(items, 'textures/SLUS-00594')).toMatchObject({
      kind: 'folder',
      game: { role: 'mod', of: id(MGS), addon: { format: 'texture-pack' } },
    });
    // 3DS: the mod folder in Mods/, the cheats in Cheats/.
    expect(one(items, 'load/mods/0004000000055D00')).toMatchObject({
      action: 'auto',
      kind: 'folder',
      target: 'ROMS/3DS/Pokemon Y (World)/Mods/0004000000055D00',
      game: {
        role: 'mod',
        of: id(POKEMON),
        folder: 'ROMS/3DS/Pokemon Y (World)',
        addon: { format: 'layered-fs', via: 'key' },
      },
      notes: [{ kind: 'mod-of', of: 'Pokemon Y' }],
    });
    expect(one(items, 'cheats/0004000000055D00.txt')).toMatchObject({
      target: 'ROMS/3DS/Pokemon Y (World)/Cheats/0004000000055D00.txt',
      game: { role: 'cheat', addon: { format: 'ctr-cheats' } },
    });
    // Switch: base, update, a romfs mod and cheats by build id.
    const acnh = 'ROMS/SWITCH/Animal Crossing New Horizons';
    expect(one(items, ACNH_UPD)).toMatchObject({
      target: expect.stringContaining(`${acnh}/Updates/`),
      game: { role: 'update', of: id(ACNH) },
    });
    expect(one(items, ACNH_ROMFS).target).toBe(`${acnh}/Mods/romfs`);
    expect(one(items, ACNH_CHEATS).target).toBe(
      `${acnh}/Cheats/7F6F4D3C2B1A0908.txt`,
    );
    // PS4: an extracted base is its own game folder.
    expect(one(items, GR2)).toMatchObject({
      kind: 'folder',
      target: `ROMS/PS4/${GR2}`,
      game: { role: 'base', folder: `ROMS/PS4/${GR2}` },
    });
    expect(one(items, `${GR2}-UPDATE`)).toMatchObject({
      target: `ROMS/PS4/${GR2}/Updates/${GR2}-UPDATE`,
      game: { role: 'update', of: id(GR2) },
    });
    expect(one(items, `${GR2}_01.03.json`)).toMatchObject({
      target: `ROMS/PS4/${GR2}/Cheats/${GR2}_01.03.json`,
      game: { role: 'cheat', of: id(GR2), addon: { format: 'shadps4-cheat' } },
    });
    // PS3: a PKG update of its PKG base.
    expect(one(items, 'Folklore (USA) [Update v1.02].pkg').game).toMatchObject({
      role: 'update',
      of: id('Folklore (USA).pkg'),
    });
    // Unlinked: Cemu (two bases), RPCS3 (many games), an unknown IPS.
    expect(one(items, 'graphicPacks/MK8 4K Resolution')).toMatchObject({
      action: 'left',
      reason: {
        kind: 'addon-unlinked',
        addon: 'mod',
        format: 'cemu-graphic-pack',
        why: { kind: 'several', count: 2 },
      },
    });
    expect(
      ['Mario Kart 8 (USA).wua', 'Mario Kart 8 (Europe).wua'].map(
        (n) => one(items, n).action,
      ),
    ).toEqual(['auto', 'auto']);
    expect(one(items, 'patch.yml')).toMatchObject({
      reason: { kind: 'addon-unlinked', why: { kind: 'many' } },
    });
    expect(one(items, 'Unknown Hack v2.ips')).toMatchObject({
      reason: { kind: 'addon-unlinked', why: { kind: 'no-match' } },
    });
    // A pile game folder: its Mods/ and Cheats/ follow the base.
    const xeno = `ROMS/SWITCH/${XENO}`;
    expect(one(items, `${XENO}/Mods/60fps`)).toMatchObject({
      target: `${xeno}/Mods/60fps`,
      game: { addon: { via: 'folder' } },
    });
    expect(one(items, `${XENO}/Cheats/A1B2C3D4E5F60718.txt`).target).toBe(
      `${xeno}/Cheats/A1B2C3D4E5F60718.txt`,
    );
    // The Coverly layout has no default emulator: the master layout.
    expect(plan.translated).toBeUndefined();
  });

  it('lays PS4 out for shadPS4 when the run chooses it', async () => {
    const b = backend();
    const { plan } = await b.organizePlan({
      source: MOCK_MODS,
      layout: 'coverly-library',
      emulators: { ps4: 'shadps4' },
    });
    const base = one(plan.items, GR2);
    const view = plan.translated?.find((v) => v.system === 'ps4');
    expect(view).toMatchObject({ emulator: 'shadps4', base: base.id });
    expect(view!.placements).toContainEqual({
      from: `${GR2}/Updates/${GR2}-UPDATE`,
      to: { kind: 'system', path: `${GR2}-UPDATE` },
    });
    expect(view!.placements).toContainEqual(
      expect.objectContaining({
        to: {
          kind: 'system',
          path: `addcont/${GR2}/Gravity Rush 2 Raven Pack`,
        },
        note: {
          kind: 'point-emulator-here',
          emulator: 'shadps4',
          folder: 'addcont',
        },
      }),
    );
    // The base never moves; the update goes beside it; the cheats keep
    // their game folder place (no shadPS4 folder was named).
    expect(base.target).toBe(`ROMS/PS4/${GR2}`);
    expect(one(plan.items, `${GR2}-UPDATE`).target).toBe(
      `ROMS/PS4/${GR2}-UPDATE`,
    );
    expect(one(plan.items, 'Gravity Rush 2 Raven Pack').notes).toContainEqual({
      kind: 'point-emulator-here',
      emulator: 'shadPS4',
      folder: 'addcont',
    });
    expect(one(plan.items, `${GR2}_01.03.json`).target).toBe(
      `ROMS/PS4/${GR2}/Cheats/${GR2}_01.03.json`,
    );
    // Only the chosen system is translated.
    expect(plan.translated!.every((v) => v.system === 'ps4')).toBe(true);
  });

  it('keeps PS4 DLC where it is under the ES-DE default, with no addcont note', async () => {
    // Like Rust's conservative scope: a default emulator never moves a
    // DLC and never tells the user to point it at a folder not made.
    const b = backend();
    const { plan } = await b.organizePlan({
      source: MOCK_MODS,
      layout: 'esde',
    });
    const dlc = one(plan.items, 'Gravity Rush 2 Raven Pack');
    expect(dlc.notes).not.toContainEqual(
      expect.objectContaining({ kind: 'point-emulator-here' }),
    );
    const view = plan.translated?.find((v) => v.system === 'ps4');
    for (const p of view?.placements ?? []) {
      expect(p.note?.kind).not.toBe('point-emulator-here');
      if (p.to.kind === 'system') expect(p.to.path).not.toMatch(/^addcont\//);
    }
  });

  it('follows the ES-DE defaults and returns to the master layout on none', async () => {
    const b = backend();
    const esde = await b.organizePlan({ source: MOCK_MODS, layout: 'esde' });
    const items = esde.plan.items;
    // RPCS3 installs updates: the row says so, the view too.
    const update = one(items, 'Folklore (USA) [Update v1.02].pkg');
    expect(update.notes).toContainEqual({
      kind: 'install-first',
      emulator: 'RPCS3',
    });
    expect(
      esde.plan.translated?.find((v) => v.system === 'ps3')?.placements,
    ).toContainEqual({
      from: 'Folklore (USA) [Update v1.02].pkg',
      to: { kind: 'library' },
      note: { kind: 'install-in-emulator', emulator: 'rpcs3' },
    });
    // A default emulator never renames a patch: RetroArch would apply it.
    expect(one(items, SD3_BPS)).toMatchObject({
      action: 'left',
      reason: { kind: 'patch-not-applied', emulator: 'RetroArch' },
    });
    // Like the core's conservative scope: a patch RetroArch could not use
    // even when chosen says why, not «choose RetroArch».
    for (const n of FF5_IPS)
      expect(one(items, n)).toMatchObject({
        action: 'left',
        reason: {
          kind: 'not-supported',
          emulator: 'RetroArch',
          why: 'several-patches',
        },
      });
    // Without game folders, a Switch addon goes to Ryujinx's folder,
    // which this run did not name.
    expect(one(items, ACNH_ROMFS)).toMatchObject({
      action: 'left',
      reason: {
        kind: 'addon-for-emulator',
        emulator: 'Ryujinx',
        path: 'mods/contents/01006F8002326000/romfs',
      },
    });

    const none = await b.organizePlan({
      source: MOCK_MODS,
      layout: 'esde',
      emulators: { switch: 'none' },
    });
    expect(none.plan.translated?.some((v) => v.system === 'switch')).toBe(
      false,
    );
    expect(one(none.plan.items, ACNH_ROMFS)).toMatchObject({
      action: 'left',
      reason: { kind: 'addon-no-place', addon: 'mod' },
    });
    expect(one(none.plan.items, ACNH_UPD).target).toBe(
      one(items, ACNH_UPD).target,
    );

    const ryujinx = await b.organizePlan({
      source: MOCK_MODS,
      layout: 'esde',
      emulators: { switch: 'ryujinx' },
    });
    expect(one(ryujinx.plan.items, ACNH_UPD).target).toBe(
      `ROMs/switch/Updates/${ACNH_UPD}`,
    );
  });

  it('puts a patch beside its ROM for RetroArch, and says when it cannot', async () => {
    const b = backend();
    const { plan } = await b.organizePlan({
      source: MOCK_MODS,
      layout: 'esde',
      emulators: { snes: 'retroarch', nes: 'retroarch' },
    });
    expect(one(plan.items, SD3_BPS)).toMatchObject({
      action: 'auto',
      target: 'ROMs/snes/Seiken Densetsu 3 (Japan).bps',
    });
    for (const n of FF5_IPS)
      expect(one(plan.items, n)).toMatchObject({
        action: 'left',
        reason: {
          kind: 'not-supported',
          emulator: 'RetroArch',
          why: 'several-patches',
        },
      });
    const nes = plan.translated?.find((v) => v.system === 'nes');
    expect(nes?.placements).toContainEqual({
      from: NES_BPS,
      to: { kind: 'library' },
      note: { kind: 'header-mismatch', needs: 'needs-headerless' },
    });
  });

  it('keeps the link of an addon whose game is already in the library', async () => {
    const b = backend();
    const { plan_id } = await b.organizePlan({
      source: MOCK_MODS,
      layout: 'coverly-library',
    });
    const done = ended(b);
    await b.organizeApply(plan_id, []);
    await done;
    const { plan } = await b.organizePlan({
      source: MOCK_MODS,
      layout: 'coverly-library',
      rescan: true,
    });
    // Nothing reads as moving without a target.
    expect(
      plan.items.filter((i) => i.action === 'auto' && i.target === null),
    ).toEqual([]);
    expect(plan.counts.auto).toBe(0);
    // The patch still names its game; its base has no game folder.
    const patch = one(plan.items, SD3_BPS);
    expect(patch).toMatchObject({
      action: 'left',
      reason: { kind: 'addon-no-place', addon: 'patch' },
      game: { role: 'mod', addon: { format: 'bps', via: 'source-crc' } },
    });
    expect(patch.game?.of).toBeUndefined();
    expect(patch.notes).toContainEqual({
      kind: 'patch-of',
      of: 'Seiken Densetsu 3 (Japan)',
    });
    // Unlinked ones are still reported as before.
    expect(one(plan.items, 'Unknown Hack v2.ips')).toMatchObject({
      reason: { kind: 'addon-unlinked', why: { kind: 'no-match' } },
    });
  });
});
