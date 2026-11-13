import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import { setBackend } from '@/ipc/backend';
import { MOCK_ADDONS } from '@/ipc/mock/addonViewsFixture';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import { createMockBackend, MOCK_LIBRARIES } from '@/ipc/mock/mockBackend';
import type { JobView } from '@/ipc/types';
import { applyFilters } from '@/lib/filters';

import { useFilterStore } from './filterStore';
import { useJobStore } from './jobStore';
import { resetLibraryStore, useLibraryStore } from './libraryStore';
import { useVisibleEntries } from './useVisibleEntries';
import { useViewStore } from './viewStore';

describe('useVisibleEntries', () => {
  beforeEach(async () => {
    setBackend(createMockBackend({ delayMs: 0, now: FIXTURE_NOW }));
    resetLibraryStore();
    await useLibraryStore.getState().open(MOCK_LIBRARIES[1].path);
  });
  afterEach(() => {
    act(() => {
      useViewStore.getState().setPrefs({ groupBy: 'none' });
      useJobStore.setState({ job: null });
    });
    setBackend(null);
  });

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

  it('ungrouped: one section and the sorted order', () => {
    const { result } = renderHook(() => useVisibleEntries());
    expect(result.current.sections).toEqual([
      { key: 'all', label: '', start: 0, count: 240 },
    ]);
  });

  it('grouped by system: visible follows the sections', () => {
    const { result } = renderHook(() => useVisibleEntries());
    act(() => useViewStore.getState().setPrefs({ groupBy: 'system' }));
    const { visible, sections } = result.current;
    const systems = useLibraryStore.getState().view!.systems;
    expect(sections.length).toBeGreaterThan(1);
    expect(sections.reduce((n, s) => n + s.count, 0)).toBe(visible.length);
    for (const s of sections) {
      const ids = new Set(
        visible.slice(s.start, s.start + s.count).map((e) => e.system),
      );
      expect(ids.size).toBe(1);
      const index = [...ids][0]!;
      expect(s.label).toBe(systems.find((x) => x.index === index)!.name);
    }
  });

  it('keeps art groups frozen while a fill runs', () => {
    const { result } = renderHook(() => useVisibleEntries());
    act(() => useViewStore.getState().setPrefs({ groupBy: 'art' }));
    const before = result.current.sections.map((s) => [s.key, s.count]);
    expect(before.length).toBeGreaterThan(1);
    act(() => useJobStore.setState({ job: { state: 'applying' } as JobView }));
    const coverAll = () => {
      const view = useLibraryStore.getState().view!;
      useLibraryStore.setState({
        view: {
          ...view,
          entries: view.entries.map((e) => ({
            ...e,
            art: { cover: 'present', screenshot: 'present', title: 'present' },
          })),
        },
      });
    };
    act(coverAll);
    expect(result.current.sections.map((s) => [s.key, s.count])).toEqual(
      before,
    );
    act(() => useJobStore.setState({ job: null }));
    expect(result.current.sections.map((s) => [s.key, s.count])).toEqual([
      ['art:complete', 240],
    ]);
  });
});

describe('useVisibleEntries on games (D1, D4, D13)', () => {
  beforeEach(async () => {
    setBackend(
      createMockBackend({ delayMs: 0, now: FIXTURE_NOW, storage: null }),
    );
    resetLibraryStore();
    await useLibraryStore.getState().open(MOCK_ADDONS);
  });
  afterEach(() => {
    act(() => useViewStore.getState().setPrefs({ groupBy: 'none' }));
    setBackend(null);
  });

  const entryNamed = (name: string) =>
    useLibraryStore.getState().view!.entries.find((e) => e.name === name)!;

  it('shows games: revisions folded, orphans apart, counts', () => {
    const { result } = renderHook(() => useVisibleEntries());
    const { visible, visibleGames, games, orphans, counts } = result.current;
    const names = visible.map((e) => e.name);
    expect(names).toContain('Star Courier (USA) (Rev 1)');
    expect(names).not.toContain('Star Courier (USA)');
    expect(names).not.toContain('Tidewalker (v1.3.0)');
    expect(visibleGames.map((g) => g.id)).toEqual(visible.map((e) => e.id));
    expect(games).toHaveLength(visible.length);
    expect(orphans).toHaveLength(2);
    expect(counts).toMatchObject({ games: 9, demos: 2, orphans: 2 });
    expect(result.current.shownCounts).toEqual(counts);
  });

  it('a game matches the search by the name of its DLC', () => {
    const { result } = renderHook(() => useVisibleEntries());
    act(() => useFilterStore.getState().setQuery('lighthouse keeper'));
    expect(result.current.visible.map((e) => e.name)).toEqual([
      'Lantern Harbor (USA)',
    ]);
    expect(result.current.shownCounts.games).toBe(1);
    expect(result.current.shownCounts.updates).toBe(2);
  });

  it('sorting reorders games, never their updates and DLC', () => {
    const { result } = renderHook(() => useVisibleEntries());
    const before = result.current.visibleGames.map((g) =>
      g.children.map((c) => c.index),
    );
    act(() => useViewStore.getState().setSort({ key: 'name', dir: 'desc' }));
    const sorted = result.current.visible.map((e) => e.name);
    expect(sorted).toEqual(
      [...sorted].sort((a, b) =>
        b.localeCompare(a, undefined, { sensitivity: 'base', numeric: true }),
      ),
    );
    const after = result.current.visibleGames.map((g) =>
      g.children.map((c) => c.index),
    );
    expect([...after].sort()).toEqual([...before].sort());
  });

  it('the orphans scope lists only them and falls back to all', () => {
    const { result } = renderHook(() => useVisibleEntries());
    act(() => useViewStore.getState().setScope({ kind: 'orphans' }));
    expect(result.current.visible.map((e) => e.orphan)).toEqual([
      'update',
      'dlc',
    ]);
    // Organizar placed them: the library comes back without orphans.
    act(() => {
      const view = useLibraryStore.getState().view!;
      useLibraryStore.setState({
        view: {
          ...view,
          entries: view.entries.filter((e) => e.orphan === undefined),
        },
      });
    });
    expect(useViewStore.getState().scope).toEqual({ kind: 'all' });
    expect(result.current.visible).toHaveLength(result.current.games.length);
  });

  it('follows the revision default the user chose', async () => {
    const { result } = renderHook(() => useVisibleEntries());
    const untagged = entryNamed('Star Courier (USA)');
    await act(() => useLibraryStore.getState().setDefaultRevision(untagged.id));
    const names = result.current.visible.map((e) => e.name);
    expect(names).toContain('Star Courier (USA)');
    expect(names).not.toContain('Star Courier (USA) (Rev 1)');
  });
});
