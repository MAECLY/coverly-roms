import { describe, expect, it } from 'vitest';

import { generateLibrary, FIXTURE_NOW } from '@/ipc/mock/fixture';

import {
  buildFillRequest,
  DEFAULT_REGION_ORDER,
  moveItem,
  scopeIds,
} from './fill';

const view = generateLibrary({
  seed: 3,
  count: 12,
  root: '/mock/x',
  generation: 1,
  now: FIXTURE_NOW,
});

describe('fill choices', () => {
  it('turns a scope into entry ids', () => {
    const visible = view.entries.slice(0, 4);
    expect(scopeIds('selection', 7, visible)).toEqual([7]);
    expect(scopeIds('selection', null, visible)).toEqual([]);
    expect(scopeIds('filter', null, visible)).toEqual([0, 1, 2, 3]);
    expect(scopeIds('library', 7, visible)).toBeNull();
  });

  it('moves a region up and down within bounds', () => {
    expect(moveItem(DEFAULT_REGION_ORDER, 2, -1)).toEqual([
      'rom',
      'usa',
      'world',
      'europe',
      'japan',
    ]);
    expect(moveItem(DEFAULT_REGION_ORDER, 0, -1)).toEqual([
      ...DEFAULT_REGION_ORDER,
    ]);
    expect(moveItem(DEFAULT_REGION_ORDER, 4, 1)).toEqual([
      ...DEFAULT_REGION_ORDER,
    ]);
  });

  it('builds the request the backend expects', () => {
    expect(
      buildFillRequest({
        scope: 'library',
        selectedId: null,
        visible: view.entries,
        media: ['cover'],
        strategy: 'priority',
        regions: ['usa', 'rom'],
        overwrite: 'backup',
      }),
    ).toEqual({
      entry_ids: null,
      media: ['cover'],
      strategy: 'priority',
      regions: ['usa', 'rom'],
      overwrite: 'backup',
    });
  });
});
