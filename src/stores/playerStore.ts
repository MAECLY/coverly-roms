/**
 * The mini player (extras spec D19): a queue of one category's audio, in
 * list order, the track playing and where it is. The one `<audio>` lives
 * in the app-level `PlayerHost`, which reads this store and reports back
 * with `progress` and `ended`; the panel only renders controls. Tracks
 * are named by the game and their path inside `Extras/`, and carry the
 * `(rev, index)` handle of the listing they came from, which
 * `syncListing` keeps current (a trashed track is skipped).
 */
import { create } from 'zustand';

import { sameCategory } from '@/lib/extras';
import type { ExtraFileView, ExtrasView } from '@/ipc/types';

/** Seconds into a track after which «Pista anterior» restarts it. */
export const RESTART_AFTER = 3;

export interface Track {
  generation: number;
  entryId: number;
  /** The game's title, shown in the player. */
  game: string;
  rev: number;
  index: number;
  /** `/`-joined inside `Extras/`: what names the track across listings. */
  path: string;
  name: string;
}

/** The game a queue is started for. */
export interface PlayerGame {
  generation: number;
  title: string;
}

interface PlayerState {
  queue: Track[];
  at: number;
  playing: boolean;
  /** Seconds. */
  position: number;
  duration: number;
  /** Bumped by every seek the host must apply to its element. */
  seekNonce: number;
  /** Queues the audio of `index`'s category and plays `index`. */
  playFrom(listing: ExtrasView, index: number, game: PlayerGame): void;
  play(): void;
  pause(): void;
  toggle(): void;
  next(): void;
  previous(): void;
  seek(seconds: number): void;
  stop(): void;
  /** The host's time update. */
  progress(position: number, duration: number): void;
  /** The host's `ended` event: the next track, or stop at the end. */
  ended(): void;
  /** A video opened: only one thing plays at a time. */
  pauseForVideo(): void;
  /** The library `generation` closed (any library when omitted). */
  stopOnLibraryClose(generation?: number): void;
  /** A new listing of a game: handles follow, gone tracks are skipped. */
  syncListing(listing: ExtrasView): void;
}

const IDLE = {
  queue: [] as Track[],
  at: 0,
  playing: false,
  position: 0,
  duration: 0,
};

function plays(f: ExtraFileView): boolean {
  return f.kind === 'audio' && f.playable === 'inline';
}

/** The track playing (or paused), if any. */
export function currentTrack(s: Pick<PlayerState, 'queue' | 'at'>) {
  return s.queue[s.at] ?? null;
}

export const usePlayerStore = create<PlayerState>()((set, get) => ({
  ...IDLE,
  seekNonce: 0,

  playFrom(listing, index, game) {
    const start = listing.files.find((f) => f.index === index);
    if (!start || !plays(start)) return;
    const queue = listing.files
      .filter((f) => plays(f) && sameCategory(f.category, start.category))
      .map((f): Track => ({
        generation: game.generation,
        entryId: listing.entry_id,
        game: game.title,
        rev: listing.rev,
        index: f.index,
        path: f.path,
        name: f.name,
      }));
    // From the top, also when the host already holds this file: the
    // seek tells it to rewind.
    set({
      queue,
      at: queue.findIndex((t) => t.index === index),
      playing: true,
      position: 0,
      duration: 0,
      seekNonce: get().seekNonce + 1,
    });
  },

  play() {
    if (get().queue.length > 0) set({ playing: true });
  },

  pause() {
    set({ playing: false });
  },

  toggle() {
    if (get().playing) get().pause();
    else get().play();
  },

  next() {
    const { queue, at } = get();
    if (at + 1 < queue.length) set({ at: at + 1, position: 0, duration: 0 });
    else set({ playing: false });
  },

  previous() {
    const { at, position, seekNonce } = get();
    if (position > RESTART_AFTER || at === 0)
      set({ position: 0, seekNonce: seekNonce + 1 });
    else set({ at: at - 1, position: 0, duration: 0 });
  },

  seek(seconds) {
    const { duration, seekNonce } = get();
    const max = duration > 0 ? duration : Number.POSITIVE_INFINITY;
    set({
      position: Math.min(Math.max(0, seconds), max),
      seekNonce: seekNonce + 1,
    });
  },

  stop() {
    set(IDLE);
  },

  progress(position, duration) {
    set({ position, duration });
  },

  ended() {
    const { queue, at } = get();
    if (at + 1 < queue.length)
      set({ at: at + 1, position: 0, duration: 0, playing: true });
    else set({ playing: false, position: 0 });
  },

  pauseForVideo() {
    set({ playing: false });
  },

  stopOnLibraryClose(generation) {
    const track = currentTrack(get());
    if (!track) return;
    if (generation === undefined || track.generation === generation)
      get().stop();
  },

  syncListing(listing) {
    const { queue, at } = get();
    const current = queue[at];
    if (!current || current.entryId !== listing.entry_id) return;
    const byPath = new Map(listing.files.map((f) => [f.path, f]));
    const kept: Track[] = [];
    // The first surviving track at or after the current one.
    let nextAt = -1;
    queue.forEach((t, i) => {
      const f = byPath.get(t.path);
      if (!f || !plays(f)) return;
      if (i >= at && nextAt < 0) nextAt = kept.length;
      kept.push({ ...t, rev: listing.rev, index: f.index, name: f.name });
    });
    if (kept.length === 0) {
      get().stop();
      return;
    }
    const still = byPath.get(current.path);
    if (still && plays(still)) {
      set({ queue: kept, at: nextAt });
      return;
    }
    // Gone: the next surviving track plays; with none after it the
    // queue ends, as it does after its last track.
    set(
      nextAt >= 0
        ? { queue: kept, at: nextAt, position: 0, duration: 0 }
        : {
            queue: kept,
            at: kept.length - 1,
            playing: false,
            position: 0,
            duration: 0,
          },
    );
  },
}));
