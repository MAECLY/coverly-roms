import { describe, expect, it } from 'vitest';

import type { SystemView } from '@/ipc/types';

import { COVER_SIZES } from './coverSizes';
import type { Section } from './grouping';
import {
  SHELF_LIMIT,
  seeAllTarget,
  shelfHeight,
  shelfOfIndex,
} from './shelves';

const systems: SystemView[] = [
  {
    index: 0,
    folder: 'gb',
    dir: '/m/gb',
    system_id: 'gb',
    name: 'Game Boy',
    roms: 3,
  },
  {
    index: 4,
    folder: 'md',
    dir: '/m/md',
    system_id: 'md',
    name: 'Mega Drive',
    roms: 2,
  },
];

const section = (key: string): Section => ({
  key,
  label: '',
  start: 0,
  count: 1,
});

describe('seeAllTarget', () => {
  it('scopes a system shelf to that system', () => {
    expect(seeAllTarget(section('system:4'), systems)).toEqual({
      scope: { kind: 'system', system: 4 },
    });
  });

  it('scopes a folder shelf to its folder, root or below', () => {
    expect(seeAllTarget(section('folder:4:'), systems)).toEqual({
      scope: { kind: 'folder', path: '/m/md' },
    });
    expect(seeAllTarget(section('folder:0:Hacks/EU'), systems)).toEqual({
      scope: { kind: 'folder', path: '/m/gb/Hacks/EU' },
    });
    expect(seeAllTarget(section('folder:9:x'), systems)).toEqual({});
  });

  it('filters a region shelf, but not "no region"', () => {
    expect(seeAllTarget(section('region:japan'), systems)).toEqual({
      region: 'japan',
    });
    expect(seeAllTarget(section('region:none'), systems)).toEqual({});
  });

  it('filters the art shelves a filter can express', () => {
    expect(seeAllTarget(section('art:missingCover'), systems)).toEqual({
      art: 'missing-cover',
    });
    expect(seeAllTarget(section('art:complete'), systems)).toEqual({
      art: 'complete',
    });
    expect(seeAllTarget(section('art:missingOther'), systems)).toEqual({});
  });

  it('only switches the view for letter shelves', () => {
    expect(seeAllTarget(section('letter:A'), systems)).toEqual({});
  });
});

describe('shelf geometry', () => {
  it('is one header plus one tile and its gaps tall', () => {
    expect(shelfHeight('m')).toBe(32 + COVER_SIZES.m.tileH + 24);
    expect(SHELF_LIMIT).toBe(60);
  });

  it('finds the shelf of an index', () => {
    const shelves: Section[] = [
      { key: 'a', label: '', start: 0, count: 3 },
      { key: 'b', label: '', start: 3, count: 2 },
    ];
    expect(shelfOfIndex(shelves, 0)).toBe(0);
    expect(shelfOfIndex(shelves, 2)).toBe(0);
    expect(shelfOfIndex(shelves, 3)).toBe(1);
    expect(shelfOfIndex(shelves, 4)).toBe(1);
  });
});
