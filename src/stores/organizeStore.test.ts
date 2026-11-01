import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { waitFor } from '@testing-library/react';

import { setBackend, type Backend } from '@/ipc/backend';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import { createMockBackend, type MockBackend } from '@/ipc/mock/mockBackend';
import {
  MOCK_DOWNLOADS,
  MOCK_MIXED,
  MOCK_MODERN,
} from '@/ipc/mock/mockOrganize';
import type {
  OrganizeDecision,
  OrganizePlanView,
  OrganizeRequest,
} from '@/ipc/types';

import { resetLibraryStore, useLibraryStore } from './libraryStore';
import { useNavStore } from './navStore';
import { useOrganizeStore } from './organizeStore';

function mock(): MockBackend {
  return createMockBackend({
    delayMs: 0,
    now: FIXTURE_NOW,
    storage: null,
    pauseAfter: null,
  });
}

const store = () => useOrganizeStore.getState();

describe('organise store', () => {
  let b: MockBackend;
  beforeEach(() => {
    b = mock();
    setBackend(b);
    resetLibraryStore();
    useOrganizeStore.getState().reset();
    useNavStore.setState({ screen: 'main' });
  });
  afterEach(() => {
    useOrganizeStore.getState().reset();
    setBackend(null);
  });

  it('start → plan with progress → ready', async () => {
    const seen: string[] = [];
    let progressed = false;
    const off = useOrganizeStore.subscribe((s) => {
      if (seen.at(-1) !== s.status) seen.push(s.status);
      if (s.progress && s.progress.phase !== 'apply') progressed = true;
    });
    await store().start(MOCK_DOWNLOADS);
    off();
    const s = store();
    expect(seen).toEqual(['planning', 'ready']);
    expect(progressed).toBe(true);
    expect(s.status).toBe('ready');
    expect(s.source).toBe(MOCK_DOWNLOADS);
    expect(s.request).toEqual({ source: MOCK_DOWNLOADS });
    expect(s.planId).not.toBeNull();
    expect(s.plan?.counts).toMatchObject({ auto: 19, review: 0 });
    expect(s.progress).toBeNull();
    expect(s.error).toBeNull();
  });

  it('a dry run never takes apply-time extract progress as its own', async () => {
    const listeners: ((p: unknown) => void)[] = [];
    vi.spyOn(b, 'onOrganizeProgress').mockImplementation(async (cb) => {
      listeners.push(cb as (p: unknown) => void);
      return () => {};
    });
    const plan = b.organizePlan.bind(b);
    let release!: () => void;
    const gate = new Promise<void>((r) => (release = r));
    vi.spyOn(b, 'organizePlan').mockImplementation(async (req, dec) => {
      await gate;
      return plan(req, dec);
    });
    const started = store().start(MOCK_DOWNLOADS);
    await waitFor(() => expect(listeners.length).toBeGreaterThan(0));
    for (const cb of listeners)
      cb({
        plan_id: 999,
        phase: 'extract',
        done: 1,
        total: 12,
        item: 0,
        bytes_done: 0,
        bytes_total: 10,
      });
    expect(store().progress).toBeNull();
    release();
    await started;
  });

  it('setOption re-plans and keeps decisions', async () => {
    await store().start(MOCK_MIXED);
    const review = store().plan!.items.filter((i) => i.action === 'review');
    expect(review).toHaveLength(2);
    await store().decide([review[0]!.id], { system: 'gbc' });
    expect(store().plan!.items[review[0]!.id]).toMatchObject({
      action: 'auto',
      system_id: 'gbc',
      method: 'user',
    });
    const before = store().planId;
    await store().setOption({ names: 'catalogue' });
    const s = store();
    expect(s.planId).not.toBe(before);
    expect(s.request).toEqual({ source: MOCK_MIXED, names: 'catalogue' });
    expect(s.plan!.names).toBe('catalogue');
    expect(s.plan!.items[review[0]!.id]).toMatchObject({
      action: 'auto',
      system_id: 'gbc',
    });
    expect(s.decisions[review[0]!.id]).toEqual({
      item: review[0]!.id,
      choice: 'assign',
      system: 'gbc',
    });
  });

  it('decide assigns a whole bucket in one call', async () => {
    const plan = vi.spyOn(b, 'organizePlan');
    await store().start(MOCK_MIXED);
    const ids = store()
      .plan!.items.filter((i) => i.action === 'review')
      .map((i) => i.id);
    plan.mockClear();
    await store().decide(ids, { system: 'gb' });
    expect(plan).toHaveBeenCalledTimes(1);
    const decisions = plan.mock.calls[0]![1] as OrganizeDecision[];
    expect(decisions).toHaveLength(2);
    for (const id of ids) {
      expect(store().plan!.items[id]).toMatchObject({
        action: 'auto',
        system_id: 'gb',
      });
    }
    // Leaving one of them replaces its earlier decision.
    await store().decide([ids[0]!], 'leave');
    expect(store().decisions[ids[0]!]).toEqual({
      item: ids[0],
      choice: 'leave',
    });
    expect(store().plan!.items[ids[0]!]!.action).toBe('left');
    expect(store().plan!.counts.review).toBe(0);
  });

  it('apply → done → openResult opens the target with the layout profile', async () => {
    const open = vi.spyOn(b, 'openLibrary');
    useNavStore.getState().openOrganize(MOCK_DOWNLOADS);
    expect(useNavStore.getState()).toMatchObject({
      screen: 'organize',
      organizeSource: MOCK_DOWNLOADS,
    });
    await store().start(MOCK_DOWNLOADS);
    await store().apply();
    await waitFor(() => expect(store().status).toBe('done'));
    const s = store();
    expect(s.job?.state).toBe('done');
    expect(s.job?.report).toMatchObject({ moved: 16, extracted: 3 });
    expect(s.job?.job).toBeTruthy();
    await store().openResult();
    expect(open).toHaveBeenCalledWith(MOCK_DOWNLOADS, 'anbernic-stock');
    expect(useLibraryStore.getState().status).toBe('ready');
    expect(useLibraryStore.getState().view?.entries).toHaveLength(20);
    expect(useNavStore.getState().screen).toBe('main');
  });

  it('undo with later jobs keeps the list and undoes all when asked', async () => {
    await store().start(MOCK_DOWNLOADS);
    await store().apply();
    await waitFor(() => expect(store().status).toBe('done'));
    const lib = await b.openLibrary(MOCK_DOWNLOADS, 'anbernic-stock');
    await b.setEntryArtFromFile(lib.generation, 0, 'cover');
    const fill = (await b.jobHistory()).find((h) => h.kind === 'fill')!;

    await store().undoJob();
    let s = store();
    expect(s.undo).toBeNull();
    expect(s.error).toMatchObject({ code: 'later-jobs', detail: fill.job });
    expect(s.status).toBe('done');

    await store().undoJob(true);
    s = store();
    expect(s.error).toBeNull();
    expect(s.undo?.later_jobs.map((j) => j.job)).toEqual([fill.job]);
    expect(s.undo?.report.map((r) => r.job)).toEqual([fill.job, s.job!.job]);
    // The pile is back: probing finds the ROMs loose again.
    expect((await b.probeFolder(MOCK_DOWNLOADS)).rom_like).toBe(20);
  });

  it('an outdated plan answer is ignored (sequence guard)', async () => {
    let releaseFirst!: () => void;
    const gate = new Promise<void>((r) => (releaseFirst = r));
    let calls = 0;
    const slow: Backend = {
      ...b,
      async organizePlan(
        req: OrganizeRequest,
        d?: OrganizeDecision[],
      ): Promise<OrganizePlanView> {
        calls += 1;
        const first = calls === 1;
        const view = await b.organizePlan(req, d);
        if (first) await gate;
        return view;
      },
    };
    setBackend(slow);
    const first = store().start(MOCK_DOWNLOADS);
    await new Promise((r) => setTimeout(r, 0));
    const second = store().setOption({ names: 'catalogue' });
    await second;
    expect(store().plan?.names).toBe('catalogue');
    releaseFirst();
    await first;
    const s = store();
    expect(s.status).toBe('ready');
    expect(s.plan?.names).toBe('catalogue');
  });

  it('a failed plan is an error, and reset goes back to idle', async () => {
    await store().start('/Users/demo/Nowhere');
    expect(store().status).toBe('error');
    expect(store().error?.code).toBe('not-found');
    store().reset();
    expect(store().status).toBe('idle');
    expect(store().plan).toBeNull();
  });

  it('cancel while applying says whether the request went through', async () => {
    useOrganizeStore.setState({ status: 'applying' });
    const cancel = vi.spyOn(b, 'organizeCancelApply').mockResolvedValue();
    await expect(store().cancel()).resolves.toBe(true);
    cancel.mockRejectedValueOnce(new Error('gone'));
    await expect(store().cancel()).resolves.toBe(false);
    expect(store().error).not.toBeNull();
  });

  it('cancel while planning drops the answer', async () => {
    const cancel = vi.spyOn(b, 'organizeCancel');
    let asked = false;
    const off = useOrganizeStore.subscribe((s) => {
      if (s.progress && !asked) {
        asked = true;
        void s.cancel();
      }
    });
    await store().start(MOCK_DOWNLOADS);
    off();
    expect(asked).toBe(true);
    expect(cancel).toHaveBeenCalled();
    expect(store().status).toBe('idle');
    expect(store().plan).toBeNull();
  });

  it('apply is refused while a newer plan is still being made', async () => {
    await store().start(MOCK_DOWNLOADS);
    const apply = vi.spyOn(b, 'organizeApply');
    let tried = false;
    const off = useOrganizeStore.subscribe((s) => {
      if (s.status === 'planning' && !tried) {
        tried = true;
        void s.apply();
      }
    });
    // The shown plan is for the old options: applying it would move files
    // with options the user no longer sees.
    await store().setOption({ names: 'catalogue' });
    off();
    expect(tried).toBe(true);
    expect(apply).not.toHaveBeenCalled();
    expect(store().status).toBe('ready');
  });

  it('cancelling a re-plan restores the options and decisions of the shown plan', async () => {
    await store().start(MOCK_MIXED);
    const shown = store().plan;
    const off = useOrganizeStore.subscribe((s) => {
      if (s.progress && s.status === 'planning') void s.cancel();
    });
    await store().setOption({ names: 'catalogue' });
    const id = store().plan!.items.find((i) => i.action === 'review')!.id;
    await store().decide([id], { system: 'gbc' });
    off();
    const s = store();
    expect(s.status).toBe('ready');
    expect(s.plan).toBe(shown);
    expect(s.request).toEqual({ source: MOCK_MIXED });
    expect(s.decisions).toEqual({});
  });

  it('options and decisions are frozen once an apply started', async () => {
    await store().start(MOCK_DOWNLOADS);
    await store().apply();
    await waitFor(() => expect(store().status).toBe('done'));
    const plan = vi.spyOn(b, 'organizePlan');
    const before = store();
    await store().setOption({ names: 'catalogue' });
    await store().decide([0], 'leave');
    expect(plan).not.toHaveBeenCalled();
    expect(store().status).toBe('done');
    expect(store().request).toEqual(before.request);
    expect(store().decisions).toEqual({});
  });

  it('start takes the options of a library and locks the layout', async () => {
    await store().start(MOCK_DOWNLOADS, {
      layout: 'esde',
      mode: 'copy',
      flat: true,
      locked: true,
    });
    const s = store();
    expect(s.status).toBe('ready');
    expect(s.locked).toBe(true);
    expect(s.request).toEqual({
      source: MOCK_DOWNLOADS,
      layout: 'esde',
      mode: 'copy',
      flat: true,
    });
    expect(s.plan?.layout).toBe('esde');
    // The layout stays; other options still change.
    await store().setOption({ layout: 'muos' });
    expect(store().request?.layout).toBe('esde');
    await store().setOption({ names: 'catalogue' });
    expect(store().request).toMatchObject({
      layout: 'esde',
      names: 'catalogue',
    });
    store().reset();
    expect(store().locked).toBe(false);
  });

  it('resumeJob resumes an interrupted job from the history', async () => {
    b = createMockBackend({
      delayMs: 0,
      now: FIXTURE_NOW,
      storage: null,
      pauseAfter: null,
      organizePauseAfter: 8,
    });
    setBackend(b);
    await store().start(MOCK_DOWNLOADS);
    await store().apply();
    await waitFor(() => expect(store().status).toBe('paused'));
    const job = store().job!.job!;
    store().reset();

    await store().resumeJob(job, MOCK_DOWNLOADS);
    expect(store().source).toBe(MOCK_DOWNLOADS);
    await waitFor(() => expect(store().status).toBe('done'));
    expect(store().job).toMatchObject({ job, state: 'done' });
    // The 11 left after the pause: 10 cartridges and the Saturn disc.
    expect(store().job?.report).toMatchObject({ moved: 10, extracted: 1 });
  });

  describe('refresh (scan again)', () => {
    const byName = (name: string) =>
      store().plan!.items.find((i) => i.sources[0] === name)!;

    it('reads the folder again and counts the new and gone files', async () => {
      await store().start(MOCK_MIXED);
      b.organizeTouch(MOCK_MIXED, {
        add: ['Wario Land 4 (USA).gba', 'Zelda DX (USA).gbc'],
        remove: ['mystery.rom'],
      });
      const plan = vi.spyOn(b, 'organizePlan');
      const pending = store().refresh();
      expect(store()).toMatchObject({ status: 'planning', refreshing: true });
      await pending;
      const s = store();
      expect(s.refreshing).toBe(false);
      expect(plan).toHaveBeenCalledTimes(1);
      expect(plan.mock.calls[0]![0]).toEqual({
        source: MOCK_MIXED,
        rescan: true,
      });
      // `rescan` is never kept: later option changes reuse the new scan.
      expect(s.request).toEqual({ source: MOCK_MIXED });
      expect(s.status).toBe('ready');
      expect(s.plan!.items).toHaveLength(11);
      expect(s.lastRefresh).toEqual({
        added: ['wario land 4 (usa).gba', 'zelda dx (usa).gbc'],
        gone: 1,
        kept: 0,
        dropped: 0,
      });
    });

    it('says nothing changed when the folder holds the same files', async () => {
      await store().start(MOCK_MIXED);
      const before = store().plan!.items.length;
      await store().refresh();
      expect(store().plan!.items).toHaveLength(before);
      expect(store().lastRefresh).toEqual({
        added: [],
        gone: 0,
        kept: 0,
        dropped: 0,
      });
    });

    it('keeps decisions by file, not by position, and drops those of gone files', async () => {
      await store().start(MOCK_MIXED);
      await store().decide([byName('Pokemon Pinball.zip').id], {
        system: 'gbc',
      });
      await store().decide([byName('Columns.bin').id], 'leave');
      // A new first file shifts every id; Columns.bin goes away.
      b.organizeTouch(MOCK_MIXED, {
        add: ['000 First (USA).gba'],
        remove: ['Columns.bin'],
      });
      const plan = vi.spyOn(b, 'organizePlan');
      await store().refresh();
      // One rescan, then one re-plan with the remapped decision.
      expect(plan).toHaveBeenCalledTimes(2);
      expect(plan.mock.calls[1]![0]).toEqual({ source: MOCK_MIXED });
      const pinball = byName('Pokemon Pinball.zip');
      expect(pinball).toMatchObject({
        action: 'auto',
        system_id: 'gbc',
        method: 'user',
      });
      expect(store().decisions).toEqual({
        [pinball.id]: { item: pinball.id, choice: 'assign', system: 'gbc' },
      });
      expect(store().lastRefresh).toMatchObject({
        added: ['000 first (usa).gba'],
        gone: 1,
        kept: 1,
        dropped: 1,
      });
    });

    it('matches files whatever their case', async () => {
      await store().start(MOCK_MIXED);
      await store().decide([byName('Pokemon Pinball.zip').id], {
        system: 'gbc',
      });
      b.organizeTouch(MOCK_MIXED, {
        add: ['POKEMON PINBALL.ZIP'],
        remove: ['Pokemon Pinball.zip'],
      });
      await store().refresh();
      expect(store().lastRefresh).toMatchObject({
        added: [],
        gone: 0,
        kept: 1,
        dropped: 0,
      });
    });

    it('cancel goes back to the shown plan with its decisions', async () => {
      await store().start(MOCK_MIXED);
      await store().decide([byName('Pokemon Pinball.zip').id], {
        system: 'gbc',
      });
      const shown = store().plan;
      const decisions = store().decisions;
      b.organizeTouch(MOCK_MIXED, { add: 3 });
      const off = useOrganizeStore.subscribe((s) => {
        if (s.progress && s.status === 'planning') void s.cancel();
      });
      await store().refresh();
      off();
      const s = store();
      expect(s.status).toBe('ready');
      expect(s.plan).toBe(shown);
      expect(s.decisions).toEqual(decisions);
      expect(s.request).toEqual({ source: MOCK_MIXED });
      expect(s.lastRefresh).toBeNull();
      expect(s.refreshing).toBe(false);
    });

    it('a folder that went away keeps the last plan, stale, and can be retried', async () => {
      await store().start(MOCK_MIXED);
      const shown = store().plan;
      b.organizeTouch(MOCK_MIXED, { gone: true });
      await store().refresh();
      expect(store().status).toBe('error');
      expect(store().error?.code).toBe('not-found');
      expect(store().plan).toBe(shown);
      expect(store().refreshing).toBe(false);
      b.organizeTouch(MOCK_MIXED, { gone: false });
      await store().refresh();
      expect(store().status).toBe('ready');
      expect(store().error).toBeNull();
    });

    it('a change of option or decision clears the summary', async () => {
      await store().start(MOCK_MIXED);
      await store().refresh();
      expect(store().lastRefresh).not.toBeNull();
      await store().setOption({ names: 'catalogue' });
      expect(store().lastRefresh).toBeNull();
      await store().refresh();
      await store().decide([byName('Columns.bin').id], 'leave');
      expect(store().lastRefresh).toBeNull();
    });

    it('does nothing while a plan runs or once an apply started', async () => {
      await store().start(MOCK_DOWNLOADS);
      const plan = vi.spyOn(b, 'organizePlan');
      const replan = store().setOption({ names: 'catalogue' });
      await store().refresh();
      await replan;
      expect(plan).toHaveBeenCalledTimes(1);
      await store().apply();
      await waitFor(() => expect(store().status).toBe('done'));
      await store().refresh();
      expect(plan).toHaveBeenCalledTimes(1);
    });
  });

  it('reveal shows a pile copy and says so when a path is outside every known folder', async () => {
    await store().start(MOCK_DOWNLOADS);
    await store().reveal(`${MOCK_DOWNLOADS}/Tetris.gb`);
    expect(store().error).toBeNull();
    expect(b.revealedPaths).toEqual([`${MOCK_DOWNLOADS}/Tetris.gb`]);
    await store().reveal('/Volumes/Elsewhere/x.gb');
    expect(store().error?.code).toBe('outside-library');
    expect(store().status).toBe('ready');
  });

  it('openOrganize keeps the launch options until it closes', () => {
    useNavStore.getState().openOrganize('/Volumes/SD', {
      layout: 'muos',
      mode: 'move',
      locked: true,
    });
    expect(useNavStore.getState()).toMatchObject({
      screen: 'organize',
      organizeSource: '/Volumes/SD',
      organizeLaunch: { layout: 'muos', mode: 'move', locked: true },
    });
    useNavStore.getState().closeOrganize();
    expect(useNavStore.getState().organizeLaunch).toBeNull();
  });

  describe('size confirmation (systems batch 3 D23)', () => {
    /** The modern pile copied: its PS3 folder alone is over 10 GB. */
    async function largeCopy() {
      useNavStore.getState().openOrganize(MOCK_MODERN);
      await store().start(MOCK_MODERN, { mode: 'copy', layout: 'esde' });
      expect(store().status).toBe('ready');
    }

    it('a large-transfer refusal goes back to the plan and holds the numbers', async () => {
      await largeCopy();
      const seen: string[] = [];
      const off = useOrganizeStore.subscribe((s) => {
        if (seen.at(-1) !== s.status) seen.push(s.status);
      });
      await store().apply();
      off();
      const s = store();
      expect(seen).toEqual(['applying', 'ready']);
      expect(s.status).toBe('ready');
      expect(s.error).toBeNull();
      expect(s.job).toBeNull();
      expect(s.submitting).toBe(false);
      expect(s.pendingLarge).toMatchObject({
        copies: true,
        free: null,
        volume: 'Modern',
      });
      expect(s.pendingLarge!.bytes).toBeGreaterThanOrEqual(10_000_000_000);
      expect(await b.jobHistory()).toEqual([]);
    });

    it('confirmApply resends once with the confirmed bytes', async () => {
      await largeCopy();
      const apply = vi.spyOn(b, 'organizeApply');
      await store().apply();
      const { bytes } = store().pendingLarge!;
      const planId = store().planId;
      const first = store().confirmApply(bytes);
      // A second click while the first call answers sends nothing.
      const second = store().confirmApply(bytes);
      expect(store().pendingLarge).toBeNull();
      await Promise.all([first, second]);
      expect(apply).toHaveBeenCalledTimes(2);
      expect(apply).toHaveBeenLastCalledWith(planId, [], bytes);
      await waitFor(() => expect(store().status).toBe('done'));
      expect(store().job?.state).toBe('done');
    });

    it('the first apply call shows as submitting until it answers', async () => {
      await largeCopy();
      const real = b.organizeApply.bind(b);
      let release!: () => void;
      const gate = new Promise<void>((r) => (release = r));
      const apply = vi
        .spyOn(b, 'organizeApply')
        .mockImplementation(async (...args) => {
          await gate;
          return real(...args);
        });
      const pending = store().apply();
      await waitFor(() => expect(store().submitting).toBe(true));
      // A second click while it answers sends nothing.
      await store().apply();
      expect(apply).toHaveBeenCalledTimes(1);
      release();
      await pending;
      expect(store().submitting).toBe(false);
      expect(store().pendingLarge).not.toBeNull();
    });

    it('dismissLarge cancels without applying; a new plan drops the pending one', async () => {
      await largeCopy();
      const apply = vi.spyOn(b, 'organizeApply');
      await store().apply();
      store().dismissLarge();
      expect(store().pendingLarge).toBeNull();
      expect(store().status).toBe('ready');
      await store().confirmApply(1);
      expect(apply).toHaveBeenCalledTimes(1);
      await store().apply();
      expect(store().pendingLarge).not.toBeNull();
      await store().setOption({ names: 'catalogue' });
      expect(store().pendingLarge).toBeNull();
    });

    it('a plan that grew after the confirmation is asked again', async () => {
      await largeCopy();
      await store().apply();
      const { bytes } = store().pendingLarge!;
      await store().confirmApply(bytes - 1);
      expect(store().status).toBe('ready');
      expect(store().pendingLarge?.bytes).toBe(bytes);
      expect(store().job).toBeNull();
    });
  });
});
