import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import { setBackend } from '@/ipc/backend';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import { createMockBackend, MOCK_LIBRARIES } from '@/ipc/mock/mockBackend';
import { applyFilters } from '@/lib/filters';

import { useFilterStore } from './filterStore';
import { resetLibraryStore, useLibraryStore } from './libraryStore';
import { useVisibleEntries } from './useVisibleEntries';
import { useViewStore } from './viewStore';

describe('useVisibleEntries', () => {
  beforeEach(async () => {
    setBackend(createMockBackend({ delayMs: 0, now: FIXTURE_NOW }));
    resetLibraryStore();
    await useLibraryStore.getState().open(MOCK_LIBRARIES[1].path);
  });
  afterEach(() => setBackend(null));

  it('applies scope, filters and sort together', () => {
    const { result } = renderHook(() => useVisibleEntries());
    expect(result.current.visible).toHaveLength(240);
    act(() => {
      useViewStore.getState().setScope({ kind: 'system', system: 3 });
      useFilterStore.getState().setArt('missing-cover');
    });
    const { view, keys } = useLibraryStore.getState();
    const scoped = view!.entries.filter((e) => e.system === 3);
    const expected = applyFilters(
      scoped,
      keys,
      useFilterStore.getState().filters,
    );
    expect(result.current.scoped).toHaveLength(scoped.length);
    expect(result.current.visible.map((e) => e.id)).toEqual(
      expected.map((e) => e.id),
    );
    expect(result.current.facets.systems.get(3)).toBe(expected.length);
  });
});
