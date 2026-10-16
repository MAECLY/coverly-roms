import { afterEach, describe, expect, it } from 'vitest';
import { waitFor } from '@testing-library/react';

import { setBackend } from '@/ipc/backend';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import { createMockBackend, MOCK_LIBRARIES } from '@/ipc/mock/mockBackend';
import type { JobState, JobView } from '@/ipc/types';
import { DEFAULT_REGION_ORDER } from '@/lib/fill';

import { blocksUpdate, useJobStore } from './jobStore';
import { resetLibraryStore, useLibraryStore } from './libraryStore';

describe('job store', () => {
  afterEach(() => {
    setBackend(null);
    useJobStore.getState().reset();
  });

  // React StrictMode mounts, unmounts and mounts again in dev: the first
  // connection is dropped after the second one is made.
  it('keeps_job_events_after_a_superseded_connection_is_dropped', async () => {
    setBackend(createMockBackend({ delayMs: 0, now: FIXTURE_NOW }));
    resetLibraryStore();
    useJobStore.getState().reset();
    await useLibraryStore.getState().open(MOCK_LIBRARIES[1].path);
    const first = await useJobStore.getState().connect();
    const second = await useJobStore.getState().connect();
    first();
    const view = useLibraryStore.getState().view!;
    await useJobStore.getState().start(view.generation, {
      entry_ids: null,
      media: ['cover'],
      strategy: 'rom-region',
      regions: [...DEFAULT_REGION_ORDER],
      overwrite: 'never',
    });
    await waitFor(() =>
      expect(useJobStore.getState().job?.state).toBe('planned'),
    );
    second();
  });
});

describe('blocksUpdate', () => {
  it('holds an update for a running or paused job only (Rust job_blocks_update)', () => {
    const at = (state: JobState) => ({ id: 1, state }) as unknown as JobView;
    expect(blocksUpdate(null)).toBe(false);
    const held = (
      [
        'gathering',
        'planned',
        'applying',
        'paused',
        'cancelled',
        'done',
        'failed',
      ] as const
    ).filter((st) => blocksUpdate(at(st)));
    expect(held).toEqual(['gathering', 'applying', 'paused']);
  });
});
