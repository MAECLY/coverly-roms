import { describe, expect, it } from 'vitest';

import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import { addonViewsLibrary } from '@/ipc/mock/addonViewsFixture';

import {
  buildFolderTree,
  columnDir,
  gameGroups,
  groupFiles,
  isGameFolder,
  isGameNode,
} from './folderTree';
import { buildGames } from './games';

describe('buildFolderTree', () => {
  it('nests folders with recursive counts, sorted by name', () => {
    const tree = buildFolderTree([
      { dir: 'ROMS/SFC' },
      { dir: 'ROMS/SFC/Hacks' },
      { dir: 'ROMS/gba' },
      { dir: 'ROMS/SFC' },
      { dir: '' },
    ]);
    expect(tree.count).toBe(5);
    expect(tree.children.map((c) => c.name)).toEqual(['ROMS']);
    const roms = tree.children[0]!;
    expect(roms.count).toBe(4);
    expect(roms.children.map((c) => [c.name, c.count])).toEqual([
      ['gba', 1],
      ['SFC', 3],
    ]);
    const sfc = roms.children[1]!;
    expect(sfc.path).toBe('ROMS/SFC');
    expect(sfc.children.map((c) => [c.path, c.count])).toEqual([
      ['ROMS/SFC/Hacks', 1],
    ]);
  });
});

describe('game nodes (addon views §8.5)', () => {
  const lib = addonViewsLibrary(1, FIXTURE_NOW);
  const { games } = buildGames(lib.entries, {});
  const named = (name: string) => {
    const game = games.find((g) => g.entry.name.startsWith(name));
    if (!game) throw new Error(`no game ${name}`);
    return game;
  };

  it('opens Lantern Harbor into its typed groups, in order, with counts', () => {
    const lh = named('Lantern Harbor (USA)');
    const demo = named('Lantern Harbor (Demo)');
    expect(gameGroups(lh)).toEqual([
      { kind: 'base' },
      { kind: 'updates', count: 2 },
      { kind: 'dlc', count: 1 },
      { kind: 'demo', demo: demo.id },
      { kind: 'cheats', count: 2, separated: true },
      { kind: 'mods', count: 1 },
      { kind: 'extras', count: 3 },
    ]);
    expect(isGameNode(lh)).toBe(true);
  });

  it('omits empty groups: a loose game with revisions has base and Revisiones', () => {
    const sc = named('Star Courier');
    expect(gameGroups(sc)).toEqual([
      { kind: 'base' },
      { kind: 'revisions', count: 2 },
    ]);
    expect(isGameNode(sc)).toBe(true);
    expect(isGameFolder(sc)).toBe(false);
  });

  it('leaves a plain game a plain game', () => {
    const pl = named('Pebble League');
    expect(gameGroups(pl)).toEqual([{ kind: 'base' }]);
    expect(isGameNode(pl)).toBe(false);
    expect(isGameFolder(pl)).toBe(false);
  });

  it('lists a game folder at its parent folder, other folders unchanged', () => {
    const lh = named('Lantern Harbor (USA)');
    expect(isGameFolder(lh)).toBe(true);
    expect(columnDir('Lantern Harbor', lh)).toBe('');
    expect(columnDir('Sony/Lantern Harbor', lh)).toBe('Sony');
    const pl = named('Pebble League');
    expect(columnDir('Hacks', pl)).toBe('Hacks');
    expect(columnDir('Hacks', undefined)).toBe('Hacks');
  });

  it('leaves a folder game that is its own game folder where it is', () => {
    // A Wii U or PS3 folder game holding its own Updates/: the entry is
    // the game folder already, listed in its parent (Rust `folder_root`).
    const lh = named('Lantern Harbor (USA)');
    const folderGame = {
      ...lh,
      entry: { ...lh.entry, folder_root: true as const },
    };
    expect(isGameFolder(folderGame)).toBe(true);
    expect(columnDir('Sony', folderGame)).toBe('Sony');
    expect(columnDir('', folderGame)).toBe('');
  });

  it('keeps an ordinary sub-folder a folder beside collapsed game folders', () => {
    const lh = named('Lantern Harbor (USA)');
    const pl = named('Pebble League');
    const tree = buildFolderTree([
      { dir: columnDir('Lantern Harbor', lh) },
      { dir: columnDir('Hacks', pl) },
    ]);
    expect(tree.children.map((c) => [c.path, c.count])).toEqual([['Hacks', 1]]);
    expect(tree.count).toBe(2);
  });

  it('lists a group’s files: updates newest first, DLC, revisions default first', () => {
    const lh = named('Lantern Harbor (USA)');
    expect(groupFiles(lh, 'updates').map((f) => f.name)).toEqual([
      'Lantern Harbor (v1.10)',
      'Lantern Harbor (v1.05)',
    ]);
    expect(groupFiles(lh, 'dlc').map((f) => f.kind)).toEqual(['child']);
    expect(groupFiles(lh, 'base')).toEqual([]);
    const sc = named('Star Courier');
    const revs = groupFiles(sc, 'revisions');
    expect(revs.map((f) => f.name)).toEqual([
      'Star Courier (USA) (Rev 1)',
      'Star Courier (USA)',
    ]);
    expect(revs[0]).toMatchObject({ kind: 'revision', isDefault: true });
  });
});
