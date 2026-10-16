import { describe, expect, it } from 'vitest';

import { buildFolderTree } from './folderTree';

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
