import { describe, expect, it } from 'vitest';

import type { EntryView } from '@/ipc/types';

import { buildSearchKeys, foldText, queryTokens } from './search';

const entry = (id: number, name: string, ext = 'sfc') =>
  ({ id, name, ext }) as EntryView;

describe('search', () => {
  it('folds accents and case', () => {
    expect(foldText('Pokémon ÉDICIÓN Ñandú')).toBe('pokemon edicion nandu');
  });

  it('builds keys indexed by entry id', () => {
    const keys = buildSearchKeys([
      entry(0, 'Pokémon Oro'),
      entry(1, 'Zelda', 'smc'),
    ]);
    expect(keys).toEqual(['pokemon oro.sfc', 'zelda.smc']);
  });

  it('a game matches by its own name or any of its updates and DLC', () => {
    const lantern = {
      ...entry(0, 'Lantern Harbor (USA)', 'pkg'),
      children: [
        { name: 'Lantern Harbor (v1.10)' },
        { name: 'Lantern Harbor - Lighthouse Keeper Pack' },
      ],
    } as EntryView;
    const [key] = buildSearchKeys([lantern]);
    expect(key).toContain('lantern harbor (usa).pkg');
    for (const t of queryTokens('lighthouse keeper')) expect(key).toContain(t);
    // A word never runs from one name into the next.
    expect(key).not.toContain('pkglantern');
    expect(key).not.toContain(')lantern');
  });

  it('splits the query into folded tokens', () => {
    expect(queryTokens('  POKÉMON   oro ')).toEqual(['pokemon', 'oro']);
    expect(queryTokens('   ')).toEqual([]);
  });
});
