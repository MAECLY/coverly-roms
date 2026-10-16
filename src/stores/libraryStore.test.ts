import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import { setBackend, type Backend } from '@/ipc/backend';
import { FIXTURE_NOW, generateLibrary } from '@/ipc/mock/fixture';
import { createMockBackend, MOCK_LIBRARIES } from '@/ipc/mock/mockBackend';
import type { LibraryView, ScanProgress } from '@/ipc/types';

import { useFilterStore } from './filterStore';
import { resetLibraryStore, useLibraryStore } from './libraryStore';
import { useSelectionStore } from './selectionStore';
import { useViewStore } from './viewStore';

const SMALL = MOCK_LIBRARIES[1].path;

describe('library store', () => {
  beforeEach(() => {
    setBackend(createMockBackend({ delayMs: 0, now: FIXTURE_NOW }));
    resetLibraryStore();
  });
  afterEach(() => setBackend(null));

  it('opens a library, indexes it and resets the view', async () => {
    useFilterStore.getState().setQuery('zelda');
    useSelectionStore.getState().select(3);
    useViewStore.getState().setScope({ kind: 'system', system: 2 });
    await useLibraryStore.getState().open(SMALL);
    const s = useLibraryStore.getState();
    expect(s.status).toBe('ready');
    expect(s.path).toBe(SMALL);
    expect(s.view?.entries).toHaveLength(240);
    expect(s.keys).toHaveLength(240);
    expect(s.recents[0]?.path).toBe(SMALL);
    expect(useFilterStore.getState().filters.query).toBe('');
    expect(useSelectionStore.getState().selectedId).toBeNull();
    expect(useViewStore.getState().scope).toEqual({ kind: 'all' });
  });

  it('reports a translated-ready error and stays on Start', async () => {
    await useLibraryStore.getState().open('/mock/Broken');
    const s = useLibraryStore.getState();
    expect(s.status).toBe('error');
    expect(s.error?.code).toBe('not-found');
    expect(s.view).toBeNull();
  });

  it('ignores_a_stale_open_result', async () => {
    const resolvers = new Map<string, (v: LibraryView) => void>();
    const mock = createMockBackend({ delayMs: 0, now: FIXTURE_NOW });
    const slow: Backend = {
      ...mock,
      openLibrary: (path) =>
        new Promise<LibraryView>((resolve) => resolvers.set(path, resolve)),
    };
    setBackend(slow);
    const first = useLibraryStore.getState().open('/a');
    const second = useLibraryStore.getState().open('/b');
    await new Promise((r) => setTimeout(r, 0));
    const view = (name: string, generation: number) =>
      generateLibrary({
        seed: 1,
        count: 3,
        root: `/${name}`,
        generation,
        now: FIXTURE_NOW,
      });
    resolvers.get('/b')!(view('b', 2));
    resolvers.get('/a')!(view('a', 1));
    await Promise.all([first, second]);
    expect(useLibraryStore.getState().view?.name).toBe('b');
    expect(useLibraryStore.getState().path).toBe('/b');
  });

  it('stale_scan_progress_does_not_leak', async () => {
    const mock = createMockBackend({ delayMs: 0, now: FIXTURE_NOW });
    const listeners = new Set<(p: ScanProgress) => void>();
    const emit = (p: ScanProgress) => listeners.forEach((cb) => cb(p));
    const hung: Backend = {
      ...mock,
      onScanProgress: (cb) => {
        listeners.add(cb);
        return Promise.resolve(() => listeners.delete(cb));
      },
      openLibrary: () => new Promise<LibraryView>(() => undefined),
    };
    setBackend(hung);
    void useLibraryStore.getState().open('/a');
    void useLibraryStore.getState().open('/b');
    await new Promise((r) => setTimeout(r, 0));
    const progress = (generation: number, folder: string): ScanProgress => ({
      generation,
      done: 1,
      total: 2,
      folder,
      roms: 0,
    });
    emit(progress(2, 'NEW'));
    emit(progress(1, 'OLD'));
    expect(useLibraryStore.getState().progress?.folder).toBe('NEW');
  });

  it('old_scan_progress_after_a_reopen_is_ignored', async () => {
    const mock = createMockBackend({ delayMs: 0, now: FIXTURE_NOW });
    const listeners = new Set<(p: ScanProgress) => void>();
    const emit = (p: ScanProgress) => listeners.forEach((cb) => cb(p));
    const hung: Backend = {
      ...mock,
      onScanProgress: (cb) => {
        listeners.add(cb);
        return Promise.resolve(() => listeners.delete(cb));
      },
      openLibrary: () => new Promise<LibraryView>(() => undefined),
    };
    setBackend(hung);
    const progress = (generation: number, done: number, folder: string) => ({
      generation,
      done,
      total: 55,
      folder,
      roms: 0,
    });
    void useLibraryStore.getState().open('/a');
    await new Promise((r) => setTimeout(r, 0));
    emit(progress(1, 5, 'OLD-A'));
    void useLibraryStore.getState().open('/b');
    await new Promise((r) => setTimeout(r, 0));
    // The old scan keeps going before the new one reports anything.
    emit(progress(1, 6, 'OLD-A'));
    expect(useLibraryStore.getState().progress).toBeNull();
    emit(progress(2, 0, ''));
    emit(progress(1, 7, 'OLD-A'));
    expect(useLibraryStore.getState().progress).toMatchObject({
      generation: 2,
      done: 0,
    });
  });

  it('a_failing_progress_listener_becomes_an_error', async () => {
    const mock = createMockBackend({ delayMs: 0, now: FIXTURE_NOW });
    setBackend({
      ...mock,
      onScanProgress: () => Promise.reject(new Error('listen failed')),
    });
    await expect(
      useLibraryStore.getState().open(SMALL),
    ).resolves.toBeUndefined();
    const s = useLibraryStore.getState();
    expect(s.status).toBe('error');
    expect(s.error?.code).toBe('unknown');
    expect(s.openingPath).toBeNull();
  });

  it('closes back to idle', async () => {
    await useLibraryStore.getState().open(SMALL);
    useLibraryStore.getState().close();
    expect(useLibraryStore.getState().status).toBe('idle');
    expect(useLibraryStore.getState().view).toBeNull();
  });
});
