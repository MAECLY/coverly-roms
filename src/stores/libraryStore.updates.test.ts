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
    useLibraryStore
      .getState()
      .applyArtUpdates(view.generation, [{ id: e.id, art }]);
    const next = useLibraryStore.getState().view!;
    expect(next.entries[e.id]!.art.cover).toBe('present');
    const other = view.entries.find((x) => x.id !== e.id)!;
    expect(next.entries[other.id]).toBe(other);
  });
});
