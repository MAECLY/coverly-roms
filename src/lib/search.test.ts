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

  it('splits the query into folded tokens', () => {
    expect(queryTokens('  POKÉMON   oro ')).toEqual(['pokemon', 'oro']);
    expect(queryTokens('   ')).toEqual([]);
  });
});
