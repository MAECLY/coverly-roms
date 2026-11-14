import { afterEach, describe, expect, it } from 'vitest';

import { BackendError } from '../errors';
import type {
  AddFinishedEvent,
  AddProgressEvent,
  CategoryRef,
  EntriesUpdated,
  ExtrasDropEvent,
  ExtrasView,
  LibraryView,
} from '../types';
import { MOCK_EXTRAS } from './extrasFixture';
import { FIXTURE_NOW } from './fixture';
import { sineWav } from './media/tone';
import {
  createMockBackend,
  MOCK_LIBRARIES,
  type MockBackend,
  type MockOptions,
} from './mockBackend';
import { readMockPick } from './mockOrganize';

const SOUNDTRACK: CategoryRef = { kind: 'fixed', name: 'soundtrack' };
const MANUALS: CategoryRef = { kind: 'fixed', name: 'manuals' };
const ARTWORK: CategoryRef = { kind: 'fixed', name: 'artwork' };

function backend(opts: MockOptions = {}): MockBackend {
  return createMockBackend({
    delayMs: 0,
    now: FIXTURE_NOW,
    storage: null,
    pauseAfter: null,
    ...opts,
  });
}

async function open(opts: MockOptions = {}) {
  const b = backend(opts);
  const lib = await b.openLibrary(MOCK_EXTRAS);
  const id = (name: string) => {
    const e = lib.entries.find((x) => x.name === name);
    if (!e) throw new Error(`no entry ${name}`);
    return e.id;
  };
  const list = (name: string) => b.extrasList(lib.generation, id(name));
  return { b, lib, gen: lib.generation, id, list };
}

const names = (v: ExtrasView, cat?: CategoryRef) =>
  v.files
    .filter((f) => !cat || JSON.stringify(f.category) === JSON.stringify(cat))
    .map((f) => f.name);

const pathIndex = (v: ExtrasView, path: string) => {
  const f = v.files.find((x) => x.path === path);
  if (!f) throw new Error(`no file at ${path}`);
  return f.index;
};

const fileIndex = (v: ExtrasView, name: string) => {
  const f = v.files.find((x) => x.name === name);
  if (!f) throw new Error(`no file ${name}`);
  return f.index;
};

async function rejection(p: Promise<unknown>): Promise<BackendError> {
  try {
    await p;
  } catch (e) {
    if (e instanceof BackendError) return e;
    throw e;
  }
  throw new Error('expected a rejection');
}

afterEach(() => {
  window.history.replaceState(null, '', '/');
});

describe('mock extras library (?mockPick=extras)', () => {
  it('is what ?mockPick=extras opens', () => {
    window.history.replaceState(null, '', '/?mockPick=extras');
    expect(readMockPick()).toBe(MOCK_EXTRAS);
  });

  it('opens with Tidewalker: an official PS3 theme and three tagged tracks', async () => {
    const { list, lib, id } = await open();
    const v = await list('Tidewalker');
    expect(v.game_folder).toBe(true);
    expect(v.system).toBe('ps3');
    expect(v.index).toEqual({ state: 'ok' });
    expect(v.writable).toEqual({ kind: 'yes' });
    expect(v.manual_inside).toBe(false);
    // Six fixed slots in order, then the user's, then the loose files.
    expect(v.categories.map((c) => c.category)).toEqual([
      SOUNDTRACK,
      MANUALS,
      ARTWORK,
      { kind: 'fixed', name: 'videos' },
      { kind: 'fixed', name: 'themes' },
      { kind: 'fixed', name: 'guides' },
      { kind: 'user', name: 'Remixes' },
      { kind: 'root' },
    ]);
    expect(v.categories[1]).toMatchObject({ present: false, count: 0 });
    const theme = v.files.find((f) => f.name.endsWith('.p3t'))!;
    expect(theme).toMatchObject({
      category: { kind: 'fixed', name: 'themes' },
      path: 'Themes/Tidewalker Theme.p3t',
      kind: 'other',
      provenance: 'official',
      format: 'ps3-p3t',
      playable: 'reveal-only',
    });
    const tracks = v.files.filter(
      (f) => f.category.kind === 'fixed' && f.category.name === 'soundtrack',
    );
    expect(tracks).toHaveLength(3);
    expect(tracks.map((t) => t.name.split('.').pop())).toEqual([
      'flac',
      'wav',
      'flac',
    ]);
    expect(tracks.every((t) => t.tags.length > 0)).toBe(true);
    expect(tracks.flatMap((t) => t.tags)).toContain('tema del jefe final');
    expect(tracks.every((t) => t.playable === 'inline')).toBe(true);
    // A record whose file is gone keeps its tags.
    expect(v.missing).toEqual([
      expect.objectContaining({ index: 0, tags: ['créditos'] }),
    ]);
    // The panel count is the listing's file count.
    expect(lib.entries[id('Tidewalker')]!.addons?.extras).toBe(v.files.length);
  });

  it('lists a SNES rip as audio played by another app', async () => {
    const { list } = await open();
    const v = await list('Star Courier (USA)');
    expect(v.system).toBe('snes');
    expect(v.files[0]).toMatchObject({
      kind: 'audio',
      format: 'spc',
      provenance: 'user',
      playable: 'external',
    });
  });

  it('shows a 3DS game with no Extras/ yet and the manual note', async () => {
    const { list } = await open();
    const v = await list('Moss Garden');
    expect(v).toMatchObject({
      game_folder: true,
      manual_inside: true,
      index: { state: 'absent' },
      files: [],
      missing: [],
    });
    expect(v.categories).toHaveLength(6);
    expect(v.categories.every((c) => !c.present && c.count === 0)).toBe(true);
  });

  it('has a game with more than 200 files', async () => {
    const { list } = await open();
    expect((await list('Archive Run')).files.length).toBeGreaterThan(200);
  });

  it('answers an entry that is no game folder as read-only and empty', async () => {
    const { list } = await open();
    const v = await list('Pocket Quest (USA)');
    expect(v).toMatchObject({
      game_folder: false,
      writable: { kind: 'read-only' },
      categories: [],
      files: [],
    });
  });

  it('refuses writes on a read-only game folder', async () => {
    const { b, gen, id, list } = await open();
    const v = await list('Neon Rhythm');
    expect(v.writable).toEqual({ kind: 'read-only' });
    const e = await rejection(
      b.extrasSetTags(gen, id('Neon Rhythm'), v.rev, 0, ['x']),
    );
    expect(e.code).toBe('write-denied');
  });

  it('backs up a malformed extras.json on the first write; undo brings it back', async () => {
    const { b, gen, id, list } = await open();
    const v = await list('Lantern Harbor');
    expect(v.index.state).toBe('malformed');
    expect(v.files.every((f) => f.provenance === 'user')).toBe(true);
    const w = await b.extrasSetTags(gen, id('Lantern Harbor'), v.rev, 0, [
      'mapa',
    ]);
    expect(w.notes).toEqual([
      {
        kind: 'malformed-saved',
        file: expect.stringMatching(/^extras\.broken-\d{8}-\d{6}\.json$/),
      },
    ]);
    expect(w.listing.index).toEqual({ state: 'ok' });
    expect(w.listing.files[0]!.tags).toEqual(['mapa']);
    const u = await b.extrasUndo(gen, id('Lantern Harbor'), w.job!);
    expect(u.listing.index.state).toBe('malformed');
    expect(u.listing.files[0]!.tags).toEqual([]);
  });

  it('shows a newer extras.json and refuses every write', async () => {
    const { b, gen, id, list } = await open();
    const v = await list('Paper Skies');
    expect(v.index).toEqual({ state: 'newer', version: 2 });
    expect(v.writable).toEqual({ kind: 'newer' });
    expect(v.manual_inside).toBe(true);
    const e = await rejection(
      b.extrasTrash(gen, id('Paper Skies'), v.rev, [0]),
    );
    expect(e.code).toBe('extras-newer');
    // Viewing still works.
    const m = await b.extrasMedia(gen, id('Paper Skies'), v.rev, 0);
    expect(m.kind).toBe('audio');
  });

  it('sets files aside on the «no Trash» disk, and undo puts them back', async () => {
    const { b, gen, id, list } = await open();
    const game = id('Comet Derby');
    const v = await list('Comet Derby');
    // The listing says so before anything is trashed (D8).
    expect(v.trash).toBe(false);
    expect((await list('Tidewalker')).trash).toBe(true);
    const before = names(v);
    const w = await b.extrasTrash(gen, game, v.rev, [0]);
    expect(w.job).not.toBeNull();
    expect(w.notes).toEqual([
      { kind: 'set-aside', folder: `.coverly-trash/${w.job}` },
    ]);
    expect(names(w.listing)).toEqual(before.slice(1));
    const u = await b.extrasUndo(gen, game, w.job!);
    expect(u.restored).toBe(1);
    expect(names(u.listing)).toEqual(before);
  });

  it('refuses a write on an older listing', async () => {
    const { b, gen, id, list } = await open();
    const game = id('Tidewalker');
    const old = await list('Tidewalker');
    const fresh = await list('Tidewalker');
    expect(fresh.rev).toBeGreaterThan(old.rev);
    const e = await rejection(b.extrasSetTags(gen, game, old.rev, 0, ['x']));
    expect(e.code).toBe('extras-changed');
    // A write answers the next listing: its rev is the one to use.
    const w = await b.extrasSetTags(gen, game, fresh.rev, 0, ['x']);
    expect(w.listing.rev).toBeGreaterThan(fresh.rev);
    const again = await rejection(
      b.extrasSetTags(gen, game, fresh.rev, 0, ['y']),
    );
    expect(again.code).toBe('extras-changed');
  });

  describe('every write is a job that undo reverts', () => {
    it('rename keeps the extension and refuses a taken name', async () => {
      const { b, gen, id, list } = await open();
      const game = id('Tidewalker');
      const v = await list('Tidewalker');
      const i = fileIndex(v, '02 Coral Depths.wav');
      const w = await b.extrasRename(gen, game, v.rev, i, '02 Coral Reef');
      expect(names(w.listing)).toContain('02 Coral Reef.wav');
      // The tags follow the file.
      const renamed = w.listing.files.find(
        (f) => f.name === '02 Coral Reef.wav',
      )!;
      expect(renamed.tags).toEqual(['ambiente']);
      const taken = await rejection(
        b.extrasRename(
          gen,
          game,
          w.listing.rev,
          fileIndex(w.listing, '01 Opening Tide.flac'),
          '03 Leviathan',
        ),
      );
      expect(taken.code).toBe('extras-refused');
      expect(taken.detail).toBe('taken');
      const u = await b.extrasUndo(gen, game, w.job!);
      expect(names(u.listing)).toContain('02 Coral Depths.wav');
      expect(names(u.listing)).not.toContain('02 Coral Reef.wav');
    });

    it('refuses names that are not portable, dotted or reserved', async () => {
      const { b, gen, id, list } = await open();
      const game = id('Tidewalker');
      const v = await list('Tidewalker');
      const why = async (p: Promise<unknown>) => (await rejection(p)).detail;
      expect(await why(b.extrasRename(gen, game, v.rev, 0, 'a:b'))).toBe(
        'not-portable',
      );
      expect(await why(b.extrasRename(gen, game, v.rev, 0, '   '))).toBe(
        'empty',
      );
      expect(
        await why(b.extrasCategoryCreate(gen, game, v.rev, '.hidden')),
      ).toBe('dot');
      expect(
        await why(b.extrasCategoryCreate(gen, game, v.rev, 'extras.json')),
      ).toBe('reserved');
      expect(
        await why(b.extrasCategoryCreate(gen, game, v.rev, 'soundtrack')),
      ).toBe('taken');
      expect(
        await why(b.extrasCategoryCreate(gen, game, v.rev, 'x'.repeat(65))),
      ).toBe('too-long');
      expect(await why(b.extrasSetTags(gen, game, v.rev, 0, ['a', '']))).toBe(
        'tags-empty',
      );
      expect(
        await why(b.extrasSetTags(gen, game, v.rev, 0, ['x'.repeat(65)])),
      ).toBe('tags-too-long');
      expect(
        await why(
          b.extrasSetTags(
            gen,
            game,
            v.rev,
            0,
            Array.from({ length: 33 }, (_, n) => `t${n}`),
          ),
        ),
      ).toBe('tags-too-many');
    });

    it('move suffixes a taken name, says so and undo moves it back', async () => {
      const { b, gen, id, list } = await open();
      const game = id('Tidewalker');
      const v = await list('Tidewalker');
      const REMIXES: CategoryRef = { kind: 'user', name: 'Remixes' };
      // A loose copy of the remix lands beside it under the next number.
      const loose = pathIndex(v, '02 Coral Depths.mp3');
      const w = await b.extrasMove(gen, game, v.rev, [loose], REMIXES);
      expect(names(w.listing, REMIXES)).toEqual([
        '02 Coral Depths (2).mp3',
        '02 Coral Depths.mp3',
      ]);
      expect(w.notes).toEqual([
        { kind: 'renamed-dup', name: '02 Coral Depths (2).mp3' },
      ]);
      // A move into a fixed category that has no folder yet creates it.
      const w2 = await b.extrasMove(
        gen,
        game,
        w.listing.rev,
        [fileIndex(w.listing, 'Field Guide.pdf')],
        MANUALS,
      );
      expect(
        w2.listing.categories.find(
          (c) => c.category.kind === 'fixed' && c.category.name === 'manuals',
        ),
      ).toMatchObject({ present: true, count: 1 });
      await b.extrasUndo(gen, game, w2.job!);
      const u = await b.extrasUndo(gen, game, w.job!);
      expect(names(u.listing, REMIXES)).toEqual(['02 Coral Depths.mp3']);
      expect(names(u.listing, { kind: 'root' })).toContain(
        '02 Coral Depths.mp3',
      );
      expect(
        u.listing.categories.find(
          (c) => c.category.kind === 'fixed' && c.category.name === 'manuals',
        )?.present,
      ).toBe(false);
    });

    it('trash sends files away and undo brings them back; an undone job is gone', async () => {
      const { b, gen, id, list } = await open();
      const game = id('Tidewalker');
      const v = await list('Tidewalker');
      const w = await b.extrasTrash(gen, game, v.rev, [0, 1]);
      expect(w.notes).toEqual([]);
      expect(w.listing.files).toHaveLength(v.files.length - 2);
      const u = await b.extrasUndo(gen, game, w.job!);
      expect(u.restored).toBe(2);
      expect(names(u.listing)).toEqual(names(v));
      const twice = await rejection(b.extrasUndo(gen, game, w.job!));
      expect(twice.code).toBe('nothing-to-undo');
      // Another game's job is not this game's to undo.
      const other = await rejection(
        b.extrasUndo(gen, id('Comet Derby'), w.job!),
      );
      expect(other.code).toBe('nothing-to-undo');
    });

    it('tags are normalised, and forget drops a missing record until undone', async () => {
      const { b, gen, id, list } = await open();
      const game = id('Tidewalker');
      const v = await list('Tidewalker');
      const w = await b.extrasSetTags(gen, game, v.rev, 0, [
        '  tema del jefe final ',
        'Tema del jefe final',
        'versión orquestal',
      ]);
      expect(w.listing.files[0]!.tags).toEqual([
        'tema del jefe final',
        'versión orquestal',
      ]);
      const f = await b.extrasForget(gen, game, w.listing.rev, 0);
      expect(f.listing.missing).toEqual([]);
      const u = await b.extrasUndo(gen, game, f.job!);
      expect(u.listing.missing).toHaveLength(1);
      const t = await b.extrasUndo(gen, game, w.job!);
      expect(t.listing.files[0]!.tags).toEqual(v.files[0]!.tags);
    });

    it('creates, renames and removes own categories, each undoable', async () => {
      const { b, gen, id, list } = await open();
      const game = id('Tidewalker');
      const v = await list('Tidewalker');
      const created = await b.extrasCategoryCreate(
        gen,
        game,
        v.rev,
        ' Escaneos ',
      );
      expect(created.listing.categories).toContainEqual({
        category: { kind: 'user', name: 'Escaneos' },
        present: true,
        count: 0,
        link: false,
        sync: null,
      });
      const renamed = await b.extrasCategoryRename(
        gen,
        game,
        created.listing.rev,
        'Remixes',
        'remixes',
      );
      expect(names(renamed.listing, { kind: 'user', name: 'remixes' })).toEqual(
        ['02 Coral Depths.mp3'],
      );
      const fixed = await rejection(
        b.extrasCategoryRemove(gen, game, renamed.listing.rev, 'Soundtrack'),
      );
      expect(fixed.detail).toBe('reserved');
      const removed = await b.extrasCategoryRemove(
        gen,
        game,
        renamed.listing.rev,
        'remixes',
      );
      expect(
        removed.listing.categories.some(
          (c) => c.category.kind === 'user' && c.category.name === 'remixes',
        ),
      ).toBe(false);
      expect(removed.listing.files).toHaveLength(v.files.length - 1);
      await b.extrasUndo(gen, game, removed.job!);
      await b.extrasUndo(gen, game, renamed.job!);
      const u = await b.extrasUndo(gen, game, created.job!);
      expect(u.listing.categories.map((c) => c.category)).toEqual(
        v.categories.map((c) => c.category),
      );
      expect(names(u.listing)).toEqual(names(v));
    });

    it('the same name is no job at all', async () => {
      const { b, gen, id, list } = await open();
      const v = await list('Tidewalker');
      const i = fileIndex(v, '02 Coral Depths.wav');
      const w = await b.extrasRename(
        gen,
        id('Tidewalker'),
        v.rev,
        i,
        '02 Coral Depths',
      );
      expect(w.job).toBeNull();
    });

    it('refreshes the entry count after a write', async () => {
      const { b, gen, id, list } = await open();
      const game = id('Tidewalker');
      const seen: EntriesUpdated[] = [];
      await b.onEntriesUpdated((u) => seen.push(u));
      const v = await list('Tidewalker');
      await b.extrasTrash(gen, game, v.rev, [0]);
      expect(seen.at(-1)).toMatchObject({
        generation: gen,
        entries: [{ id: game, addons: { extras: v.files.length - 1 } }],
      });
    });

    it('lists Extras jobs in the history with their game', async () => {
      const { b, gen, id, list } = await open();
      const v = await list('Tidewalker');
      const w = await b.extrasTrash(gen, id('Tidewalker'), v.rev, [0]);
      const h = await b.jobHistory();
      expect(h[0]).toMatchObject({
        job: w.job,
        kind: 'extras',
        game: 'Tidewalker',
        undone: false,
        undoable: true,
        root: MOCK_EXTRAS,
      });
    });
  });

  describe('adding files', () => {
    it('the mock picker offers two files with a suggested category; commit copies them', async () => {
      const { b, gen, id, list } = await open();
      const game = id('Tidewalker');
      const v = await list('Tidewalker');
      const plan = (await b.extrasAddPick(gen, game))!;
      expect(plan.files).toHaveLength(2);
      expect(plan.files.every((f) => f.category === 'soundtrack')).toBe(true);
      expect(plan.files.some((f) => f.rip !== null)).toBe(true);
      expect(plan.too_many).toBe(false);
      const started = await b.extrasAddCommit(
        gen,
        game,
        v.rev,
        plan.plan,
        plan.files.map((f) => ({ index: f.index, category: SOUNDTRACK })),
      );
      if (started.kind !== 'done') throw new Error('expected done');
      const w = started.write;
      for (const f of plan.files) {
        expect(names(w.listing, SOUNDTRACK)).toContain(f.name);
      }
      expect(
        w.listing.files
          .filter((f) => plan.files.some((p) => p.name === f.name))
          .every((f) => f.provenance === 'user'),
      ).toBe(true);
      // A plan is committed once.
      const again = await rejection(
        b.extrasAddCommit(gen, game, w.listing.rev, plan.plan, []),
      );
      expect(again.code).toBe('bad-request');
      const u = await b.extrasUndo(gen, game, w.job!);
      expect(u.removed).toBe(2);
      expect(names(u.listing)).toEqual(names(v));
    });

    it('a picker cancelled answers null', async () => {
      const { b, gen, id } = await open({ extrasPick: 'cancel' });
      expect(await b.extrasAddPick(gen, id('Tidewalker'))).toBeNull();
    });

    it('too many files are refused before anything is copied', async () => {
      const { b, gen, id, list } = await open({ extrasPick: 'too-many' });
      const game = id('Tidewalker');
      const v = await list('Tidewalker');
      const plan = (await b.extrasAddPick(gen, game))!;
      expect(plan.too_many).toBe(true);
      const e = await rejection(
        b.extrasAddCommit(gen, game, v.rev, plan.plan, [
          { index: 0, category: SOUNDTRACK },
        ]),
      );
      expect(e.detail).toBe('too-many');
    });

    it('a big add runs in the background with progress, holds the library and can be cancelled', async () => {
      const { b, gen, id, list } = await open({ extrasPick: 'many' });
      const game = id('Tidewalker');
      const v = await list('Tidewalker');
      const progress: AddProgressEvent[] = [];
      const added: AddFinishedEvent[] = [];
      await b.onExtrasProgress((p) => progress.push(p));
      const done = new Promise<void>((resolve) => {
        void b.onExtrasAdded((e) => {
          added.push(e);
          resolve();
        });
      });
      const plan = (await b.extrasAddPick(gen, game))!;
      const started = await b.extrasAddCommit(
        gen,
        game,
        v.rev,
        plan.plan,
        plan.files.map((f) => ({ index: f.index, category: ARTWORK })),
      );
      expect(started).toMatchObject({
        kind: 'background',
        n: plan.files.length,
      });
      if (started.kind !== 'background') throw new Error('expected background');
      expect((await list('Tidewalker')).writable).toEqual({ kind: 'busy' });
      const busy = await rejection(b.extrasTrash(gen, game, v.rev, [0]));
      expect(busy.code).toBe('job-running');
      expect(await b.extrasAddCancel(started.add)).toBe(true);
      await done;
      expect(progress.length).toBeGreaterThan(0);
      expect(progress[0]).toMatchObject({ entry_id: game, add: started.add });
      const end = added[0]!.end;
      if (end.kind !== 'done') throw new Error('expected done');
      const cancelled = end.write.notes.find((n) => n.kind === 'cancelled');
      expect(cancelled).toBeDefined();
      expect((await list('Tidewalker')).writable).toEqual({ kind: 'yes' });
    });

    it('an OS drop is announced by token with names, sizes and position', async () => {
      const { b, gen, id, list } = await open();
      const game = id('Tidewalker');
      const drops: ExtrasDropEvent[] = [];
      await b.onExtrasDrop((d) => drops.push(d));
      const token = b.emitDrop({
        names: ['Manual.pdf', 'BLUS30001.rap', '.DS_Store'],
        position: { x: 120, y: 340 },
      });
      expect(drops).toEqual([
        {
          token,
          position: { x: 120, y: 340 },
          items: [
            { name: 'Manual.pdf', size: expect.any(Number), is_dir: false },
            { name: 'BLUS30001.rap', size: expect.any(Number), is_dir: false },
            { name: '.DS_Store', size: expect.any(Number), is_dir: false },
          ],
        },
      ]);
      const v = await list('Tidewalker');
      const started = await b.extrasAddDropped(
        gen,
        game,
        v.rev,
        token,
        MANUALS,
      );
      if (started.kind !== 'done') throw new Error('expected done');
      // Key and hidden files are never copied, and said so.
      expect(names(started.write.listing, MANUALS)).toEqual(['Manual.pdf']);
      expect(started.write.notes).toEqual([
        { kind: 'keys-skipped', n: 1 },
        { kind: 'hidden-skipped', n: 1 },
      ]);
      // A token is used once.
      const spent = await rejection(
        b.extrasAddDropped(
          gen,
          game,
          started.write.listing.rev,
          token,
          MANUALS,
        ),
      );
      expect(spent.detail).toBe('drop-expired');
    });

    it('a drop on no category answers a plan with the suggested categories', async () => {
      const { b, gen, id } = await open();
      const token = b.emitDrop({
        names: ['Tidewalker Manual.pdf', 'Poster.png', 'Ending.spc'],
        position: { x: 0, y: 0 },
      });
      const plan = await b.extrasAddPlan(gen, id('Tidewalker'), token);
      expect(plan.files.map((f) => f.category)).toEqual([
        'manuals',
        'artwork',
        'soundtrack',
      ]);
      expect(plan.files[2]!.rip).toMatchObject({ title: 'Ending' });
      expect(plan.keys).toBe(0);
    });

    it('a copy whose name is taken gets the next free number', async () => {
      const { b, gen, id, list } = await open();
      const game = id('Tidewalker');
      const v = await list('Tidewalker');
      const token = b.emitDrop({
        names: ['01 Opening Tide.flac'],
        position: { x: 0, y: 0 },
      });
      const started = await b.extrasAddDropped(
        gen,
        game,
        v.rev,
        token,
        SOUNDTRACK,
      );
      if (started.kind !== 'done') throw new Error('expected done');
      expect(started.write.notes).toEqual([
        { kind: 'renamed-dup', name: '01 Opening Tide (2).flac' },
      ]);
    });
  });

  describe('viewing', () => {
    it('serves generated media by kind and refuses packages', async () => {
      const { b, gen, id, list } = await open();
      const game = id('Tidewalker');
      const v = await list('Tidewalker');
      const media = (name: string) =>
        b.extrasMedia(gen, game, v.rev, fileIndex(v, name));
      const audio = await media('01 Opening Tide.flac');
      expect(audio.kind).toBe('audio');
      expect(audio.mime).toBe('audio/wav');
      expect(audio.url).toMatch(/^data:audio\/wav;base64,UklGR/);
      expect((await media('Key Art.png')).kind).toBe('image');
      expect((await media('Launch Trailer.webm')).kind).toBe('video');
      expect(await media('Field Guide.pdf')).toMatchObject({
        kind: 'pdf',
        mime: 'application/pdf',
      });
      const pkg = await rejection(media('Tidewalker Theme.p3t'));
      expect(pkg.detail).toBe('not-media');
      const opened = await rejection(
        b.extrasOpenExternal(
          gen,
          game,
          v.rev,
          fileIndex(v, 'Tidewalker Theme.p3t'),
        ),
      );
      expect(opened.detail).toBe('reveal-only');
      await b.extrasReveal(
        gen,
        game,
        v.rev,
        fileIndex(v, 'Tidewalker Theme.p3t'),
      );
      expect(b.extrasRevealed).toEqual(['Themes/Tidewalker Theme.p3t']);
    });

    it('reads text extras as text, never as HTML', async () => {
      const { b, gen, id, list } = await open();
      const game = id('Tidewalker');
      const v = await list('Tidewalker');
      const nfo = await b.extrasReadText(
        gen,
        game,
        v.rev,
        fileIndex(v, 'Readme.nfo'),
      );
      expect(nfo.encoding).toBe('cp437');
      expect(nfo.text).toContain('<script>');
      const notText = await rejection(
        b.extrasReadText(gen, game, v.rev, fileIndex(v, 'Key Art.png')),
      );
      expect(notText.detail).toBe('not-text');
    });
  });

  it('makes a 1 s 440 Hz sine WAV', () => {
    const wav = sineWav({ seconds: 1, hz: 440, rate: 8000 });
    const text = (a: number, b: number) =>
      String.fromCharCode(...wav.slice(a, b));
    expect(text(0, 4)).toBe('RIFF');
    expect(text(8, 12)).toBe('WAVE');
    const view = new DataView(wav.buffer);
    expect(view.getUint32(24, true)).toBe(8000);
    // 16-bit mono: two bytes per sample after the 44-byte header.
    expect(wav.length).toBe(44 + 8000 * 2);
    expect(view.getUint32(4, true)).toBe(wav.length - 8);
    // It starts at zero and swings.
    expect(view.getInt16(44, true)).toBe(0);
    const peak = Math.max(
      ...Array.from({ length: 40 }, (_, i) => view.getInt16(44 + i * 2, true)),
    );
    expect(peak).toBeGreaterThan(10_000);
  });

  it('undoes an Extras job from the jobs history without touching another open library', async () => {
    const { b, gen, id, list } = await open();
    const v = await list('Tidewalker');
    const w = await b.extrasTrash(gen, id('Tidewalker'), v.rev, [0]);
    const other = await b.openLibrary(MOCK_LIBRARIES[1].path);
    const seen: EntriesUpdated[] = [];
    await b.onEntriesUpdated((u) => seen.push(u));
    const undone = await b.organizeUndo(w.job!, false);
    expect(undone.report[0]!.restored).toHaveLength(1);
    // The other library's entry with Tidewalker's id keeps its cells.
    expect(seen).toEqual([]);
    const again = await b.openLibrary(MOCK_LIBRARIES[1].path);
    expect(again.entries[id('Tidewalker')]!.addons).toEqual(
      other.entries[id('Tidewalker')]!.addons,
    );
    // Back in the Extras library the file is there again.
    const lib = await b.openLibrary(MOCK_EXTRAS);
    const e = lib.entries.find((x) => x.name === 'Tidewalker')!;
    expect(e.addons?.extras).toBe(v.files.length);
  });

  it('a later write keeps a repaired index when an earlier job is undone', async () => {
    const { b, gen, id, list } = await open();
    const game = id('Lantern Harbor');
    const v = await list('Lantern Harbor');
    const first = await b.extrasSetTags(gen, game, v.rev, 0, ['mapa']);
    const second = await b.extrasSetTags(gen, game, first.listing.rev, 1, [
      'notas',
    ]);
    // extras.json was written again after the first job: undoing it
    // leaves the index the second job wrote (Rust keeps a file changed
    // since), so it does not read as malformed again.
    const u = await b.extrasUndo(gen, game, first.job!);
    expect(u.listing.index).toEqual({ state: 'ok' });
    const u2 = await b.extrasUndo(gen, game, second.job!);
    expect(u2.listing.index).toEqual({ state: 'ok' });
  });

  it('a commit that does not fit is refused with the bytes needed and free, and the plan is kept', async () => {
    const { b, gen, id, list } = await open({ extrasPick: 'no-space' });
    const game = id('Tidewalker');
    const v = await list('Tidewalker');
    const plan = (await b.extrasAddPick(gen, game))!;
    const choices = plan.files.map((f) => ({
      index: f.index,
      category: SOUNDTRACK,
    }));
    const e = await rejection(
      b.extrasAddCommit(gen, game, v.rev, plan.plan, choices),
    );
    expect(e.code).toBe('no-space');
    expect(e.detail).toMatch(/^\d+\/\d+$/);
    const [needed, free] = e.detail!.split('/').map(Number);
    expect(needed).toBe(plan.total);
    expect(free).toBeLessThan(needed!);
    // Still there to retry, not spent.
    const retry = await rejection(
      b.extrasAddCommit(gen, game, v.rev, plan.plan, choices),
    );
    expect(retry.code).toBe('no-space');
  });

  it('keeps the edits when the library is opened again', async () => {
    const { b, gen, id, list } = await open();
    const v = await list('Tidewalker');
    await b.extrasTrash(gen, id('Tidewalker'), v.rev, [0]);
    const lib: LibraryView = await b.openLibrary(MOCK_EXTRAS);
    const e = lib.entries.find((x) => x.name === 'Tidewalker')!;
    expect(e.addons?.extras).toBe(v.files.length - 1);
    const again = await b.extrasList(lib.generation, e.id);
    expect(again.files).toHaveLength(v.files.length - 1);
  });
});
