import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { waitFor } from '@testing-library/react';

import { setBackend, type Backend } from '@/ipc/backend';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import { createMockBackend, type MockBackend } from '@/ipc/mock/mockBackend';
import { MOCK_DOWNLOADS, MOCK_MIXED } from '@/ipc/mock/mockOrganize';
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
    expect(s.plan?.counts).toMatchObject({ auto: 16, review: 0 });
    expect(s.progress).toBeNull();
    expect(s.error).toBeNull();
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
    expect(s.job?.report?.moved).toBe(16);
    expect(s.job?.job).toBeTruthy();
    await store().openResult();
    expect(open).toHaveBeenCalledWith(MOCK_DOWNLOADS, 'anbernic-stock');
    expect(useLibraryStore.getState().status).toBe('ready');
    expect(useLibraryStore.getState().view?.entries).toHaveLength(16);
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
    expect((await b.probeFolder(MOCK_DOWNLOADS)).rom_like).toBe(17);
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
});
