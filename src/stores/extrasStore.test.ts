import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { setBackend, type Backend } from '@/ipc/backend';
import { BackendError } from '@/ipc/errors';
import { MOCK_EXTRAS } from '@/ipc/mock/extrasFixture';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import { createMockBackend, type MockBackend } from '@/ipc/mock/mockBackend';
import type { CategoryRef, ExtrasView, LibraryView } from '@/ipc/types';

import { useExtrasStore } from './extrasStore';
import { currentTrack, usePlayerStore } from './playerStore';

const SOUNDTRACK: CategoryRef = { kind: 'fixed', name: 'soundtrack' };
const VIDEOS: CategoryRef = { kind: 'fixed', name: 'videos' };

const store = () => useExtrasStore.getState();

let b: MockBackend;
let lib: LibraryView;
const id = (name: string) => {
  const e = lib.entries.find((x) => x.name === name);
  if (!e) throw new Error(name);
  return e.id;
};
const fileIndex = (name: string) => {
  const f = store().listing!.files.find((x) => x.name === name);
  if (!f) throw new Error(name);
  return f.index;
};
const soundtrack = () =>
  store()
    .listing!.files.filter(
      (f) => f.category.kind === 'fixed' && f.category.name === 'soundtrack',
    )
    .map((f) => f.index);

/** A controllable listing backend: each call waits for `answer`. */
function deferredBackend() {
  const calls: {
    entryId: number;
    resolve: (v: ExtrasView) => void;
    reject: (e: unknown) => void;
  }[] = [];
  const backend = {
    extrasList: (_g: number, entryId: number) =>
      new Promise<ExtrasView>((resolve, reject) =>
        calls.push({ entryId, resolve, reject }),
      ),
  } as unknown as Backend;
  return { backend, calls };
}

function view(entry_id: number, rev: number): ExtrasView {
  return {
    entry_id,
    rev,
    game_folder: true,
    index: { state: 'ok' },
    writable: { kind: 'yes' },
    categories: [],
    files: [],
    missing: [],
    capped: false,
    system: 'ps3',
    manual_inside: false,
  };
}

describe('extrasStore', () => {
  beforeEach(async () => {
    b = createMockBackend({
      delayMs: 0,
      now: FIXTURE_NOW,
      storage: null,
      pauseAfter: null,
    });
    setBackend(b);
    lib = await b.openLibrary(MOCK_EXTRAS);
    store().reset();
    usePlayerStore.getState().stop();
  });
  afterEach(() => {
    store().reset();
    setBackend(null);
  });

  it('opens a game listing', async () => {
    await store().open(lib.generation, id('Tidewalker'));
    expect(store().entryId).toBe(id('Tidewalker'));
    expect(store().listing?.entry_id).toBe(id('Tidewalker'));
    expect(store().loading).toBe(false);
    expect(store().error).toBeNull();
  });

  it('drops a listing of a game that is no longer the open one (D4)', async () => {
    const { backend, calls } = deferredBackend();
    setBackend(backend);
    const first = store().open(1, 10);
    const second = store().open(1, 11);
    await vi.waitFor(() => expect(calls).toHaveLength(2));
    calls[1]!.resolve(view(11, 1));
    await second;
    calls[0]!.resolve(view(10, 1));
    await first;
    expect(store().entryId).toBe(11);
    expect(store().listing?.entry_id).toBe(11);
  });

  it('drops a stale answer for the same game by request number', async () => {
    const { backend, calls } = deferredBackend();
    setBackend(backend);
    const first = store().open(1, 10);
    const again = store().refresh();
    await vi.waitFor(() => expect(calls).toHaveLength(2));
    calls[1]!.resolve(view(10, 5));
    await again;
    calls[0]!.resolve(view(10, 4));
    await first;
    expect(store().listing?.rev).toBe(5);
  });

  it('keeps the last listing while the same game loads again', async () => {
    const { backend, calls } = deferredBackend();
    setBackend(backend);
    const first = store().open(1, 10);
    await vi.waitFor(() => expect(calls).toHaveLength(1));
    calls[0]!.resolve(view(10, 1));
    await first;
    const again = store().refresh();
    expect(store().loading).toBe(true);
    expect(store().listing?.rev).toBe(1);
    await vi.waitFor(() => expect(calls).toHaveLength(2));
    calls[1]!.resolve(view(10, 2));
    await again;
    expect(store().loading).toBe(false);
    expect(store().listing?.rev).toBe(2);
  });

  it('never shows another game while a new one loads', async () => {
    const { backend, calls } = deferredBackend();
    setBackend(backend);
    const first = store().open(1, 10);
    await vi.waitFor(() => expect(calls).toHaveLength(1));
    calls[0]!.resolve(view(10, 1));
    await first;
    void store().open(1, 11);
    expect(store().listing).toBeNull();
    expect(store().loading).toBe(true);
  });

  it('keeps a listing error for the open game', async () => {
    const { backend, calls } = deferredBackend();
    setBackend(backend);
    const first = store().open(1, 10);
    await vi.waitFor(() => expect(calls).toHaveLength(1));
    calls[0]!.reject(new BackendError('io', 'gone'));
    await first;
    expect(store().error?.code).toBe('io');
    expect(store().loading).toBe(false);
  });

  it('a write answers the next listing and records its job for undo', async () => {
    const tide = id('Tidewalker');
    await store().open(lib.generation, tide);
    const boss = fileIndex('02 Coral Depths.wav');
    const w = await store().move([boss], VIDEOS);
    expect(w.job).toBeTruthy();
    expect(store().lastJob[tide]).toBe(w.job);
    expect(store().listing).toBe(w.listing);
    const moved = store().listing!.files.find(
      (f) => f.name === '02 Coral Depths.wav',
    );
    expect(moved?.category).toEqual(VIDEOS);
    const u = await store().undo();
    expect(u?.job).toBe(w.job);
    expect(store().lastJob[tide]).toBeUndefined();
    const back = store().listing!.files.find(
      (f) => f.name === '02 Coral Depths.wav',
    );
    expect(back?.category).toEqual(SOUNDTRACK);
    // Nothing left to undo.
    expect(await store().undo()).toBeNull();
  });

  it('keeps the last job of each game apart', async () => {
    const tide = id('Tidewalker');
    const star = id('Star Courier (USA)');
    await store().open(lib.generation, tide);
    const a = await store().setTags(fileIndex('02 Coral Depths.wav'), ['jefe']);
    await store().open(lib.generation, star);
    expect(store().lastJob[tide]).toBe(a.job);
    expect(await store().undo()).toBeNull();
    await store().open(lib.generation, tide);
    expect((await store().undo())?.job).toBe(a.job);
  });

  it('refreshes on extras-changed and rethrows', async () => {
    const tide = id('Tidewalker');
    await store().open(lib.generation, tide);
    const old = store().listing!;
    // Another write moves the listing on behind the store's back.
    await b.extrasSetTags(lib.generation, tide, old.rev, old.files[0]!.index, [
      'x',
    ]);
    await expect(
      store().setTags(old.files[0]!.index, ['y']),
    ).rejects.toMatchObject({ code: 'extras-changed' });
    expect(store().listing!.rev).toBeGreaterThan(old.rev);
    expect(store().error).toBeNull();
  });

  it('selects one, toggles and ranges within a category', async () => {
    await store().open(lib.generation, id('Tidewalker'));
    const [a, c, d] = soundtrack() as [number, number, number];
    store().select(a, 'only');
    expect([...store().selection]).toEqual([a]);
    store().select(d, 'range');
    expect([...store().selection].sort()).toEqual([a, c, d].sort());
    store().select(c, 'toggle');
    expect([...store().selection].sort()).toEqual([a, d].sort());
    store().select(c, 'toggle');
    expect(store().selection.has(c)).toBe(true);
    store().clearSelection();
    expect(store().selection.size).toBe(0);
  });

  it('a range into another category selects only the clicked file', async () => {
    await store().open(lib.generation, id('Tidewalker'));
    const [a] = soundtrack() as [number];
    const theme = fileIndex('Tidewalker Theme.p3t');
    store().select(a, 'only');
    store().select(theme, 'range');
    expect([...store().selection]).toEqual([theme]);
  });

  it('a write or another game clears the selection', async () => {
    await store().open(lib.generation, id('Tidewalker'));
    const [a] = soundtrack() as [number];
    store().select(a, 'only');
    await store().setTags(a, ['uno']);
    expect(store().selection.size).toBe(0);
    store().select(soundtrack()[0]!, 'only');
    await store().open(lib.generation, id('Star Courier (USA)'));
    expect(store().selection.size).toBe(0);
  });

  it('a new listing skips a trashed track in the player', async () => {
    const tide = id('Tidewalker');
    await store().open(lib.generation, tide);
    const [first, second] = soundtrack() as [number, number];
    usePlayerStore.getState().playFrom(store().listing!, first, {
      generation: lib.generation,
      title: 'Tidewalker',
    });
    const playing = currentTrack(usePlayerStore.getState())!.name;
    await store().trash([first]);
    const now = currentTrack(usePlayerStore.getState())!;
    expect(now.name).not.toBe(playing);
    expect(now.rev).toBe(store().listing!.rev);
    expect(second).toBeDefined();
  });

  it('applies a write from an add flow for the open game only', async () => {
    const tide = id('Tidewalker');
    await store().open(lib.generation, tide);
    const w = await b.extrasSetTags(
      lib.generation,
      tide,
      store().listing!.rev,
      soundtrack()[0]!,
      ['añadido'],
    );
    store().applyWrite(w);
    expect(store().listing).toBe(w.listing);
    expect(store().lastJob[tide]).toBe(w.job);
    store().applyWrite({ ...w, job: 'other', listing: view(999, 1) });
    expect(store().listing).toBe(w.listing);
    expect(store().lastJob[999]).toBe('other');
  });
});
