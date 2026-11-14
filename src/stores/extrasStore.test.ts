import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { setBackend, type Backend } from '@/ipc/backend';
import { BackendError } from '@/ipc/errors';
import { MOCK_EXTRAS } from '@/ipc/mock/extrasFixture';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import { createMockBackend, type MockBackend } from '@/ipc/mock/mockBackend';
import type {
  CategoryRef,
  ExtraFileView,
  ExtrasView,
  ExtrasWriteView,
  LibraryView,
} from '@/ipc/types';

import { onAddEnd, useExtrasStore } from './extrasStore';
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

function file(index: number, path: string): ExtraFileView {
  return {
    index,
    category: SOUNDTRACK,
    path,
    name: path.split('/').pop()!,
    kind: 'audio',
    provenance: 'user',
    format: null,
    tags: [],
    size: 1,
    link: false,
    playable: 'inline',
  };
}

function view(
  entry_id: number,
  rev: number,
  files: ExtraFileView[] = [],
): ExtrasView {
  return {
    entry_id,
    rev,
    game_folder: true,
    index: { state: 'ok' },
    writable: { kind: 'yes' },
    categories: [],
    files,
    missing: [],
    capped: false,
    system: 'ps3',
    manual_inside: false,
    trash: true,
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

  it('undoes a job of a game that is no longer open without showing it', async () => {
    const tide = id('Tidewalker');
    const star = id('Star Courier (USA)');
    await store().open(lib.generation, tide);
    const a = await store().setTags(fileIndex('02 Coral Depths.wav'), ['jefe']);
    await store().open(lib.generation, star);
    const shown = store().listing;
    const u = await store().undoJob(tide, a.job!);
    expect(u.job).toBe(a.job);
    expect(store().lastJob[tide]).toBeUndefined();
    // The open game's listing stays on screen.
    expect(store().listing).toBe(shown);
    expect(store().pending).toBeNull();
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

  it('an add finishing for another game keeps the open game loading its listing', async () => {
    const { backend, calls } = deferredBackend();
    setBackend(backend);
    const first = store().open(1, 10);
    await vi.waitFor(() => expect(calls).toHaveLength(1));
    const other: ExtrasWriteView = {
      job: 'job-11',
      listing: view(11, 1),
      notes: [],
    };
    store().applyWrite(other);
    calls[0]!.resolve(view(10, 1));
    await first;
    expect(store().listing?.entry_id).toBe(10);
    expect(store().loading).toBe(false);
    expect(store().lastJob[11]).toBe('job-11');
  });

  it('a failed write does not leave a refresh in flight loading forever', async () => {
    const { backend, calls } = deferredBackend();
    (backend as unknown as Record<string, unknown>).extrasSetTags = () =>
      Promise.reject(new BackendError('io', 'disk gone'));
    setBackend(backend);
    const first = store().open(1, 10);
    await vi.waitFor(() => expect(calls).toHaveLength(1));
    calls[0]!.resolve(view(10, 1, [file(0, 'Soundtrack/a.flac')]));
    await first;
    const again = store().refresh();
    await vi.waitFor(() => expect(calls).toHaveLength(2));
    await expect(store().setTags(0, ['x'])).rejects.toMatchObject({
      code: 'io',
    });
    calls[1]!.resolve(view(10, 2, [file(0, 'Soundtrack/a.flac')]));
    await again;
    expect(store().listing?.rev).toBe(2);
    expect(store().loading).toBe(false);
  });

  it('a listing that answers before a write still loses to the write', async () => {
    const { backend, calls } = deferredBackend();
    const writes: ((w: ExtrasWriteView) => void)[] = [];
    (backend as unknown as Record<string, unknown>).extrasSetTags = () =>
      new Promise<ExtrasWriteView>((resolve) => writes.push(resolve));
    setBackend(backend);
    const first = store().open(1, 10);
    await vi.waitFor(() => expect(calls).toHaveLength(1));
    calls[0]!.resolve(view(10, 1, [file(0, 'Soundtrack/a.flac')]));
    await first;
    const again = store().refresh();
    await vi.waitFor(() => expect(calls).toHaveLength(2));
    const tagged = store().setTags(0, ['x']);
    await vi.waitFor(() => expect(writes).toHaveLength(1));
    writes[0]!({ job: 'j', listing: view(10, 3), notes: [] });
    await tagged;
    calls[1]!.resolve(view(10, 2));
    await again;
    expect(store().listing?.rev).toBe(3);
    expect(store().loading).toBe(false);
  });

  it('a new listing keeps the selected files by path, not by stale index', async () => {
    const { backend, calls } = deferredBackend();
    setBackend(backend);
    const first = store().open(1, 10);
    await vi.waitFor(() => expect(calls).toHaveLength(1));
    calls[0]!.resolve(
      view(10, 1, [
        file(0, 'Soundtrack/a.flac'),
        file(1, 'Soundtrack/b.flac'),
        file(2, 'Soundtrack/c.flac'),
      ]),
    );
    await first;
    store().select(1, 'only');
    store().select(2, 'toggle');
    // a.flac went away outside the app: the others got new handles.
    const again = store().refresh();
    await vi.waitFor(() => expect(calls).toHaveLength(2));
    calls[1]!.resolve(
      view(10, 2, [
        file(0, 'Soundtrack/b.flac'),
        file(1, 'Soundtrack/c.flac'),
        file(2, 'Soundtrack/d.flac'),
      ]),
    );
    await again;
    expect([...store().selection].sort()).toEqual([0, 1]);
    expect(store().anchor).toBe(1);
  });

  describe('adds (D13)', () => {
    async function withPick(pick: 'two' | 'many') {
      b = createMockBackend({
        delayMs: 0,
        now: FIXTURE_NOW,
        storage: null,
        pauseAfter: null,
        extrasPick: pick,
      });
      setBackend(b);
      lib = await b.openLibrary(MOCK_EXTRAS);
      store().reset();
      await store().open(lib.generation, id('Tidewalker'));
      const plan = await b.extrasAddPick(lib.generation, id('Tidewalker'));
      return plan!;
    }

    it('a short add answers its write and shows the new listing', async () => {
      const plan = await withPick('two');
      const before = store().listing!.files.length;
      const started = await store().addCommit(
        plan.plan,
        plan.files.map((f) => ({ index: f.index, category: SOUNDTRACK })),
      );
      expect(started.kind).toBe('done');
      expect(store().listing!.files.length).toBe(before + 2);
      expect(store().lastJob[id('Tidewalker')]).toBeDefined();
      expect(store().addRun).toBeNull();
    });

    it('a long add runs in the background with progress, then ends', async () => {
      const plan = await withPick('many');
      const ends: unknown[] = [];
      const off = onAddEnd((e) => ends.push(e));
      const before = store().listing!.files.length;
      const started = await store().addCommit(
        plan.plan,
        plan.files.map((f) => ({ index: f.index, category: VIDEOS })),
      );
      expect(started.kind).toBe('background');
      expect(store().addRun).toMatchObject({
        entryId: id('Tidewalker'),
        n: 60,
      });
      await vi.waitFor(() => expect(store().addRun?.i ?? 0).toBeGreaterThan(0));
      await vi.waitFor(() => expect(ends).toHaveLength(1));
      off();
      expect(store().addRun).toBeNull();
      expect(store().listing!.files.length).toBe(before + 60);
      expect(store().lastJob[id('Tidewalker')]).toBeDefined();
    });

    it('cancel stops after the current file and keeps what was copied', async () => {
      const plan = await withPick('many');
      const ends: { end: { kind: string } }[] = [];
      const off = onAddEnd((e) => ends.push(e));
      const before = store().listing!.files.length;
      await store().addCommit(
        plan.plan,
        plan.files.map((f) => ({ index: f.index, category: VIDEOS })),
      );
      await store().cancelAdd();
      expect(store().addRun?.cancelling).toBe(true);
      await vi.waitFor(() => expect(ends).toHaveLength(1));
      off();
      const added = store().listing!.files.length - before;
      expect(added).toBeGreaterThan(0);
      expect(added).toBeLessThan(60);
    });

    it('adds an OS drop by token into one category', async () => {
      await withPick('two');
      const token = b.emitDrop({
        names: ['Boss Theme.flac', 'prod.keys'],
        position: { x: 0, y: 0 },
      });
      const started = await store().addDropped(token, SOUNDTRACK);
      if (started.kind !== 'done') throw new Error('expected done');
      expect(started.write.notes).toContainEqual({
        kind: 'keys-skipped',
        n: 1,
      });
      expect(
        store().listing!.files.some((f) => f.name === 'Boss Theme.flac'),
      ).toBe(true);
    });
  });
});
