import { beforeEach, describe, expect, it } from 'vitest';

import type {
  CategoryRef,
  ExtraFileView,
  ExtrasView,
  Playable,
} from '@/ipc/types';

import { currentTrack, RESTART_AFTER, usePlayerStore } from './playerStore';

const SOUNDTRACK: CategoryRef = { kind: 'fixed', name: 'soundtrack' };
const REMIXES: CategoryRef = { kind: 'user', name: 'Remixes' };

function f(
  index: number,
  path: string,
  category: CategoryRef = SOUNDTRACK,
  playable: Playable = 'inline',
): ExtraFileView {
  const name = path.split('/').pop()!;
  return {
    index,
    category,
    path,
    name,
    kind: /\.(flac|wav|mp3|spc)$/.test(name) ? 'audio' : 'image',
    provenance: 'user',
    format: null,
    tags: [],
    size: 1,
    link: false,
    playable,
  };
}

function listing(files: ExtraFileView[], rev = 1, entry_id = 7): ExtrasView {
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

const TRACKS = [
  f(0, 'Soundtrack/01 Opening.flac'),
  f(1, 'Soundtrack/02 Boss.wav'),
  f(2, 'Soundtrack/cover.png'),
  f(3, 'Soundtrack/03 Ending.flac'),
  f(4, 'Remixes/Boss (remix).flac', REMIXES),
  f(5, 'Soundtrack/04 Rip.spc', SOUNDTRACK, 'external'),
];
const GAME = { generation: 3, title: 'Tidewalker' };

const store = () => usePlayerStore.getState();
const names = () => store().queue.map((t) => t.name);

describe('playerStore', () => {
  beforeEach(() => store().stop());

  it('queues the category it starts from, audio played inline, in list order', () => {
    store().playFrom(listing(TRACKS), 1, GAME);
    expect(names()).toEqual([
      '01 Opening.flac',
      '02 Boss.wav',
      '03 Ending.flac',
    ]);
    expect(currentTrack(store())).toMatchObject({
      name: '02 Boss.wav',
      entryId: 7,
      generation: 3,
      game: 'Tidewalker',
      rev: 1,
      index: 1,
    });
    expect(store().playing).toBe(true);
  });

  it('does not start on a file it cannot play', () => {
    store().playFrom(listing(TRACKS), 5, GAME);
    expect(store().queue).toEqual([]);
    expect(store().playing).toBe(false);
  });

  it('goes next and previous; the end of the queue stops playing', () => {
    store().playFrom(listing(TRACKS), 0, GAME);
    store().next();
    expect(currentTrack(store())?.name).toBe('02 Boss.wav');
    store().next();
    expect(currentTrack(store())?.name).toBe('03 Ending.flac');
    store().next();
    expect(currentTrack(store())?.name).toBe('03 Ending.flac');
    expect(store().playing).toBe(false);
    store().previous();
    expect(currentTrack(store())?.name).toBe('02 Boss.wav');
    store().previous();
    store().previous();
    expect(currentTrack(store())?.name).toBe('01 Opening.flac');
  });

  it('a track that ends plays the next one', () => {
    store().playFrom(listing(TRACKS), 0, GAME);
    store().ended();
    expect(currentTrack(store())?.name).toBe('02 Boss.wav');
    expect(store().playing).toBe(true);
  });

  it('previous restarts a track played for a while', () => {
    store().playFrom(listing(TRACKS), 1, GAME);
    store().progress(RESTART_AFTER + 1, 120);
    const nonce = store().seekNonce;
    store().previous();
    expect(currentTrack(store())?.name).toBe('02 Boss.wav');
    expect(store().position).toBe(0);
    expect(store().seekNonce).toBe(nonce + 1);
  });

  it('plays, pauses, toggles and seeks within the track', () => {
    store().playFrom(listing(TRACKS), 0, GAME);
    store().pause();
    expect(store().playing).toBe(false);
    store().toggle();
    expect(store().playing).toBe(true);
    store().progress(10, 100);
    store().seek(250);
    expect(store().position).toBe(100);
    store().seek(-3);
    expect(store().position).toBe(0);
  });

  it('pauses for a video and keeps the queue', () => {
    store().playFrom(listing(TRACKS), 0, GAME);
    store().pauseForVideo();
    expect(store().playing).toBe(false);
    expect(store().queue).toHaveLength(3);
  });

  it('stops when its library closes, not another one', () => {
    store().playFrom(listing(TRACKS), 0, GAME);
    store().stopOnLibraryClose(4);
    expect(store().queue).toHaveLength(3);
    store().stopOnLibraryClose(3);
    expect(store().queue).toEqual([]);
    expect(store().playing).toBe(false);
    store().playFrom(listing(TRACKS), 0, GAME);
    store().stopOnLibraryClose();
    expect(store().queue).toEqual([]);
  });

  it('skips a trashed track and follows moved handles', () => {
    store().playFrom(listing(TRACKS), 1, GAME);
    // Boss went to the Trash; the others got new handles in rev 2.
    store().syncListing(
      listing(
        [f(0, 'Soundtrack/01 Opening.flac'), f(1, 'Soundtrack/03 Ending.flac')],
        2,
      ),
    );
    expect(names()).toEqual(['01 Opening.flac', '03 Ending.flac']);
    expect(currentTrack(store())).toMatchObject({
      name: '03 Ending.flac',
      rev: 2,
      index: 1,
    });
    expect(store().playing).toBe(true);
    expect(store().position).toBe(0);
  });

  it('a trashed last track moves back; an emptied queue stops', () => {
    store().playFrom(listing(TRACKS), 3, GAME);
    store().syncListing(
      listing(
        [f(0, 'Soundtrack/01 Opening.flac'), f(1, 'Soundtrack/02 Boss.wav')],
        2,
      ),
    );
    expect(currentTrack(store())?.name).toBe('02 Boss.wav');
    // Nothing came after it: the queue ends, as at its last track.
    expect(store().playing).toBe(false);
    expect(store().position).toBe(0);
    store().syncListing(listing([], 3));
    expect(store().queue).toEqual([]);
    expect(store().playing).toBe(false);
  });

  it('keeps the current track and position when it survives', () => {
    store().playFrom(listing(TRACKS), 1, GAME);
    store().progress(42, 120);
    store().syncListing(
      listing(
        TRACKS.map((x) => ({ ...x, index: x.index + 10 })),
        2,
      ),
    );
    expect(currentTrack(store())).toMatchObject({
      name: '02 Boss.wav',
      index: 11,
    });
    expect(store().position).toBe(42);
  });

  it('skips the current track when it no longer plays inline', () => {
    store().playFrom(listing(TRACKS), 1, GAME);
    store().progress(42, 120);
    store().syncListing(
      listing(
        [
          f(0, 'Soundtrack/01 Opening.flac'),
          f(1, 'Soundtrack/02 Boss.wav', SOUNDTRACK, 'external'),
          f(2, 'Soundtrack/03 Ending.flac'),
        ],
        2,
      ),
    );
    expect(names()).toEqual(['01 Opening.flac', '03 Ending.flac']);
    expect(currentTrack(store())?.name).toBe('03 Ending.flac');
    expect(store().position).toBe(0);
  });

  it('ignores another game listing', () => {
    store().playFrom(listing(TRACKS), 1, GAME);
    store().syncListing(listing([], 2, 8));
    expect(store().queue).toHaveLength(3);
  });
});
