import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { setBackend } from '@/ipc/backend';
import { dedupeFixture } from '@/ipc/mock/dedupeFixture';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import {
  createMockBackend,
  MOCK_LIBRARIES,
  type MockBackend,
  type MockOptions,
} from '@/ipc/mock/mockBackend';
import type { DedupePlanView } from '@/ipc/types';

import { applyConfirm, useDedupeStore } from './dedupeStore';

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

const store = () => useDedupeStore.getState();
const plan = () => store().plan!;
const acting = (p: DedupePlanView) =>
  p.groups.filter((g) => g.action.kind === 'act');

async function settled(status: string) {
  await vi.waitFor(() => expect(store().status).toBe(status));
}

describe('dedupeStore', () => {
  let b: MockBackend;
  beforeEach(() => {
    b = backend();
    setBackend(b);
    store().reset();
  });
  afterEach(() => {
    store().reset();
    setBackend(null);
  });

  it('store_plan_set_keeper_skip_apply_undo', async () => {
    await store().start(ROOT);
    expect(store().status).toBe('ready');
    // Every planning phase was seen, in order, before the plan arrived.
    expect(store().phasesDone).toEqual([
      'listing',
      'sizes',
      'fingerprints',
      'serials',
      'catalogues',
    ]);
    expect(store().options).toEqual({
      levels: ['exact', 'format', 'version'],
      one_game_one_rom: false,
    });
    const extras = plan().totals.extras;
    expect(extras).toBe(89);

    // A pick: the old keeper goes to the tray, the reason is yours.
    const ecco = plan().groups.find((g) => g.title === 'Ecco the Dolphin')!;
    const other = ecco.members.find((m) => m.id !== ecco.keeper)!;
    await store().setKeeper(ecco.id, other.id);
    const after = plan().groups.find((g) => g.id === ecco.id)!;
    expect(after.keeper).toBe(other.id);
    expect(after.reason).toEqual({ kind: 'user-choice' });

    // "No tocar este juego": one copy fewer leaves, the group stays shown.
    await store().skipGroup(ecco.id, true);
    expect(store().skipped.has(ecco.id)).toBe(true);
    expect(plan().totals.extras).toBe(extras - 1);
    await store().skipGroup(ecco.id, false);
    expect(store().skipped.has(ecco.id)).toBe(false);
    expect(plan().totals.extras).toBe(extras);

    await store().apply();
    await settled('done');
    const report = store().job!.report!;
    expect(
      report.outcomes.filter((o) => o.status === 'quarantined'),
    ).toHaveLength(extras);

    await store().undo();
    expect(store().undone?.report.at(-1)!.restored.length).toBeGreaterThan(0);
    // After undo the copies are back: a new plan finds them all again.
    await store().replan();
    expect(plan().totals.extras).toBe(extras);
  });

  it('regroups when an option changes, without a new plan', async () => {
    await store().start(ROOT);
    const id = plan().plan_id;
    const spy = vi.spyOn(b, 'dedupePlan');
    await store().setOptions({ levels: ['exact'], one_game_one_rom: false });
    expect(spy).not.toHaveBeenCalled();
    expect(plan().plan_id).toBe(id);
    expect(store().options.levels).toEqual(['exact']);
    expect(
      acting(plan()).every((g) =>
        g.members.every((m) => m.relation === 'exact'),
      ),
    ).toBe(true);
  });

  it('the trash action regroups to the Trash and back', async () => {
    await store().start(ROOT);
    await store().setAction('trash');
    expect(plan().trash).toBe(true);
    await store().setAction('quarantine');
    expect(plan().trash).toBe(false);
    await store().setFolder('/Volumes/Big/Apartados');
    expect(plan().quarantine).toBe('/Volumes/Big/Apartados');
  });

  it('skipping a system skips each of its acting groups', async () => {
    await store().start(ROOT);
    const sms = acting(plan()).filter((g) => g.system === 'SMS');
    expect(sms.length).toBeGreaterThan(0);
    await store().skipSystem('SMS', true);
    for (const g of sms) expect(store().skipped.has(g.id)).toBe(true);
    expect(plan().totals.by_system.SMS).toBeUndefined();
  });

  it('cmd_r_replans: a replan makes a new plan with the same options', async () => {
    await store().start(ROOT);
    await store().setOptions({ levels: ['exact'], one_game_one_rom: false });
    const id = plan().plan_id;
    const spy = vi.spyOn(b, 'dedupePlan');
    await store().replan();
    expect(spy).toHaveBeenCalledWith(
      ROOT,
      expect.objectContaining({ levels: ['exact'] }),
    );
    expect(plan().plan_id).toBeGreaterThan(id);
  });

  it('resetting the rules forgets the picks and plans again', async () => {
    await store().start(ROOT);
    const ecco = plan().groups.find((g) => g.title === 'Ecco the Dolphin')!;
    const other = ecco.members.find((m) => m.id !== ecco.keeper)!;
    await store().setKeeper(ecco.id, other.id);
    await store().resetRules('SMS');
    expect(plan().groups.find((g) => g.id === ecco.id)!.keeper).toBe(
      ecco.keeper,
    );
  });

  it('error_state: a failed plan says so and keeps nothing', async () => {
    b.dedupeScenario({ failPlan: 'volume-gone' });
    await store().start(ROOT);
    expect(store().status).toBe('error');
    expect(store().error?.code).toBe('volume-gone');
    b.dedupeScenario({ failPlan: null });
    await store().replan();
    expect(store().status).toBe('ready');
  });

  it('cancel while planning goes back to the shown plan', async () => {
    await store().start(ROOT);
    const shown = plan().plan_id;
    const p = store().replan();
    await store().cancel();
    await p;
    expect(store().status).toBe('ready');
    expect(plan().plan_id).toBe(shown);
  });

  it('export writes the plan as JSON', async () => {
    await store().start(ROOT);
    await store().exportPlan();
    expect(store().exported).toBe('coverly-duplicates-plan.json');
    expect(JSON.parse(b.exports.at(-1)!.text).groups.length).toBeGreaterThan(0);
  });

  it('a paused apply can be resumed', async () => {
    b = backend({ dedupePauseAfter: 3 });
    setBackend(b);
    await store().start(ROOT);
    await store().apply();
    await settled('done');
    expect(store().job!.state).toBe('paused');
    await store().resume();
    await vi.waitFor(() => expect(store().job!.state).toBe('done'));
  });
});

describe('applyConfirm', () => {
  const base = (patch: Partial<DedupePlanView> = {}): DedupePlanView => ({
    ...dedupeFixture(),
    same_volume: true,
    trash: false,
    totals: {
      groups: 1,
      extras: 3,
      bytes: 100,
      conflicts: 0,
      by_system: { GB: [1, 3, 100] },
    },
    ...patch,
  });

  it('bulk_confirm_above_500_files', () => {
    expect(applyConfirm(base(), 10_000)).toBeNull();
    const big = base({
      totals: {
        groups: 501,
        extras: 501,
        bytes: 100,
        conflicts: 0,
        by_system: { GB: [400, 400, 80], SMS: [101, 101, 20] },
      },
    });
    expect(applyConfirm(big, 10_000)).toEqual({
      kind: 'bulk',
      system: 'GB',
      share: 400,
    });
  });

  it('asks above a quarter of the library bytes', () => {
    expect(applyConfirm(base(), 400)).toBeNull();
    expect(applyConfirm(base(), 399)?.kind).toBe('bulk');
    // Unknown library size: the byte rule does not apply.
    expect(applyConfirm(base(), null)).toBeNull();
  });

  it('asks when the quarantine is on another volume', () => {
    expect(applyConfirm(base({ same_volume: false }), null)?.kind).toBe('bulk');
  });

  it('the Trash always asks, with its own dialog', () => {
    expect(applyConfirm(base({ trash: true }), null)?.kind).toBe('trash');
  });
});
