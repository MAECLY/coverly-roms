import { beforeEach, describe, expect, it } from 'vitest';

import type { ChildView, EntryView } from '@/ipc/types';

import { buildGames } from './games';
import {
  MAX_SEEN_LIBRARIES,
  MAX_SEEN_PATHS,
  SEEN_KEY,
  newChildren,
  resetSeenSession,
} from './seen';

class MemoryStorage implements Storage {
  private map = new Map<string, string>();
  get length() {
    return this.map.size;
  }
  clear() {
    this.map.clear();
  }
  getItem(key: string) {
    return this.map.get(key) ?? null;
  }
  key(i: number) {
    return [...this.map.keys()][i] ?? null;
  }
  removeItem(key: string) {
    this.map.delete(key);
  }
  setItem(key: string, value: string) {
    this.map.set(key, value);
  }
}

const throwing = {
  getItem() {
    throw new Error('blocked');
  },
  setItem() {
    throw new Error('blocked');
  },
} as unknown as Storage;

const kid = (rel_path: string): ChildView => ({
  index: 0,
  rel_path,
  name: rel_path,
  role: 'dlc',
  size: 1,
  mtime: 0,
  art: { icon: false, banner: false },
});

const library = (paths: string[]) =>
  buildGames(
    [
      {
        id: 0,
        system: 0,
        rel_path: 'G/G.pkg',
        dir: 'G',
        name: 'G',
        ext: 'pkg',
        size: 1,
        mtime: 0,
        regions: [],
        art: { cover: 'missing', screenshot: 'missing', title: 'missing' },
        children: paths.map(kid),
      } satisfies EntryView,
    ],
    {},
  ).games;

beforeEach(() => resetSeenSession());

describe('newChildren («Nuevo», D18)', () => {
  it('tags nothing on the first open and seeds the set', () => {
    const storage = new MemoryStorage();
    expect(newChildren('/lib', library(['a', 'b']), storage).size).toBe(0);
    const stored = JSON.parse(storage.getItem(SEEN_KEY)!);
    expect(stored.byRoot['/lib'].sort()).toEqual(['a', 'b']);
  });

  it('tags a path that was not there at the previous open', () => {
    const storage = new MemoryStorage();
    newChildren('/lib', library(['a']), storage);
    resetSeenSession(); // the app is opened again
    expect([...newChildren('/lib', library(['a', 'b']), storage)]).toEqual([
      'b',
    ]);
  });

  it('keeps the tags for the whole session, even after a rescan', () => {
    const storage = new MemoryStorage();
    newChildren('/lib', library(['a']), storage);
    resetSeenSession();
    newChildren('/lib', library(['a', 'b']), storage);
    expect([...newChildren('/lib', library(['a', 'b', 'c']), storage)]).toEqual(
      ['b', 'c'],
    );
  });

  it('tags a child added during the first session', () => {
    const storage = new MemoryStorage();
    newChildren('/lib', library(['a']), storage);
    expect([...newChildren('/lib', library(['a', 'z']), storage)]).toEqual([
      'z',
    ]);
  });

  it('keeps each library apart and at most 20 of them', () => {
    const storage = new MemoryStorage();
    for (let i = 0; i <= MAX_SEEN_LIBRARIES; i++)
      newChildren(`/lib${i}`, library(['a']), storage);
    const stored = JSON.parse(storage.getItem(SEEN_KEY)!);
    expect(Object.keys(stored.byRoot)).toHaveLength(MAX_SEEN_LIBRARIES);
    expect(stored.byRoot['/lib0']).toBeUndefined();
  });

  it(`is off above ${MAX_SEEN_PATHS} paths`, () => {
    const storage = new MemoryStorage();
    const many = Array.from({ length: MAX_SEEN_PATHS + 1 }, (_, i) => `p${i}`);
    newChildren('/big', library(many.slice(0, 10)), storage);
    resetSeenSession();
    expect(newChildren('/big', library(many), storage).size).toBe(0);
    const stored = JSON.parse(storage.getItem(SEEN_KEY)!);
    expect(stored.byRoot['/big']).toBeUndefined();
  });

  it('tags nothing when storage throws or is missing', () => {
    expect(newChildren('/lib', library(['a']), throwing).size).toBe(0);
    resetSeenSession();
    expect(newChildren('/lib', library(['a', 'b']), throwing).size).toBe(0);
    resetSeenSession();
    expect(newChildren('/lib', library(['a']), undefined).size).toBe(0);
  });

  it('ignores a corrupt file', () => {
    const storage = new MemoryStorage();
    storage.setItem(SEEN_KEY, '{nope');
    expect(newChildren('/lib', library(['a']), storage).size).toBe(0);
  });
});
