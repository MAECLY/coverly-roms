import { describe, expect, it } from 'vitest';

import type { SystemView } from '@/ipc/types';

import { COVER_SIZES, metrics } from './coverSizes';
import type { Game } from './games';
import type { Section } from './grouping';
import {
  SHELF_EDGE_X,
  SHELF_EDGE_Y,
  SHELF_LIMIT,
  besideIcons,
  seeAllTarget,
  shelfBoxWidth,
  shelfDlc,
  shelfHeight,
  shelfOfIndex,
  shelfStack,
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

  it('keeps the add-on summary line of a library with content', () => {
    expect(shelfHeight('m', true)).toBe(32 + metrics('m', true).tileH + 24);
    expect(shelfHeight('xs', true)).toBe(shelfHeight('xs'));
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

describe('one box per game (addon views §8.7)', () => {
  const game = (over: Partial<Game>): Game =>
    ({
      dlc: [],
      isDemo: false,
      orphan: null,
      ...over,
    }) as unknown as Game;
  const dlc = (n: number) =>
    Array.from({ length: n }, (_, i) => ({
      index: i,
    })) as unknown as Game['dlc'];

  it('stacks up to three DLC edges behind the box, then «+n»', () => {
    expect(shelfStack(0)).toEqual({ edges: 0, more: 0 });
    expect(shelfStack(1)).toEqual({ edges: 1, more: 0 });
    expect(shelfStack(3)).toEqual({ edges: 3, more: 0 });
    expect(shelfStack(5)).toEqual({ edges: 3, more: 2 });
    expect([SHELF_EDGE_X, SHELF_EDGE_Y]).toEqual([9, 7]);
  });

  it('counts the DLC of a full game only: a demo is its own box', () => {
    expect(shelfDlc(game({ dlc: dlc(3) }))).toBe(3);
    expect(shelfDlc(game({ dlc: dlc(2), isDemo: true }))).toBe(0);
    expect(shelfDlc(game({ dlc: dlc(1), orphan: 'dlc' }))).toBe(0);
    expect(shelfDlc(undefined)).toBe(0);
  });

  it('keeps the tile width behind the box and widens a box with DLC beside it', () => {
    expect(shelfBoxWidth('m', 'behind', 3)).toBe(COVER_SIZES.m.tileW);
    expect(shelfBoxWidth('m', 'beside', 0)).toBe(COVER_SIZES.m.tileW);
    expect(shelfBoxWidth('m', 'beside', 3)).toBeGreaterThan(
      COVER_SIZES.m.tileW,
    );
    // Larger stops show larger icons beside the box.
    expect(
      shelfBoxWidth('xl', 'beside', 1) - COVER_SIZES.xl.tileW,
    ).toBeGreaterThan(shelfBoxWidth('m', 'beside', 1) - COVER_SIZES.m.tileW);
  });

  it('shows as many icons beside the box as fit its height, then «+n»', () => {
    const artH = COVER_SIZES.xs.artH; // 128: four 24px icons with 4px gaps
    expect(besideIcons('xs', 2, artH)).toEqual({ shown: 2, more: 0 });
    expect(besideIcons('xs', 4, artH)).toEqual({ shown: 4, more: 0 });
    expect(besideIcons('xs', 9, artH)).toEqual({ shown: 3, more: 6 });
  });
});
