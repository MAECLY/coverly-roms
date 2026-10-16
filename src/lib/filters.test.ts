import { describe, expect, it } from 'vitest';

import { FIXTURE_NOW, generateLibrary } from '@/ipc/mock/fixture';
import type { ArtMark, EntryView } from '@/ipc/types';

import {
  activeFilterCount,
  applyFilters,
  computeFacets,
  dayEnd,
  dayStart,
  EMPTY_FILTERS,
  matchesArt,
  toDateInput,
  type Filters,
} from './filters';
import { buildSearchKeys } from './search';

function e(
  id: number,
  over: Partial<EntryView> & {
    cover?: ArtMark;
    screenshot?: ArtMark;
    title?: ArtMark;
  } = {},
): EntryView {
  const {
    cover = 'present',
    screenshot = 'present',
    title = 'present',
    ...rest
  } = over;
  return {
    id,
    system: 0,
    rel_path: `ROMS/SFC/G${id}.sfc`,
    dir: 'ROMS/SFC',
    name: `Game ${id}`,
    ext: 'sfc',
    size: 1000,
    mtime: 1000,
    regions: [],
    art: { cover, screenshot, title },
    ...rest,
  };
}

const f = (over: Partial<Filters>): Filters => ({ ...EMPTY_FILTERS, ...over });

describe('art filters', () => {
  it('missing-any ignores unsupported media', () => {
    const onlyCoverLayout = e(0, {
      screenshot: 'unsupported',
      title: 'unsupported',
    });
    expect(matchesArt(onlyCoverLayout, 'missing-any')).toBe(false);
    expect(matchesArt(onlyCoverLayout, 'complete')).toBe(true);
    const noCover = e(1, {
      cover: 'missing',
      screenshot: 'unsupported',
      title: 'unsupported',
    });
    expect(matchesArt(noCover, 'missing-any')).toBe(true);
    expect(matchesArt(noCover, 'missing-cover')).toBe(true);
    expect(matchesArt(noCover, 'missing-title')).toBe(false);
    expect(matchesArt(noCover, 'complete')).toBe(false);
  });
});

describe('applyFilters', () => {
  const entries = [
    e(0, {
      name: 'Pokémon Oro',
      system: 0,
      ext: 'gb',
      regions: ['spain'],
      size: 500,
      mtime: 100,
    }),
    e(1, {
      name: 'Zelda',
      system: 1,
      ext: 'sfc',
      regions: ['usa'],
      cover: 'missing',
      size: 5000,
      mtime: 200,
    }),
    e(2, {
      name: 'Mario',
      system: 1,
      ext: 'smc',
      regions: ['usa', 'europe'],
      size: 50_000,
      mtime: 300,
    }),
  ];
  const keys = buildSearchKeys(entries);
  const ids = (filters: Filters) =>
    applyFilters(entries, keys, filters).map((x) => x.id);

  it('searches accent- and case-insensitively', () => {
    expect(ids(f({ query: 'POKEMON' }))).toEqual([0]);
    expect(ids(f({ query: 'pok oro' }))).toEqual([0]);
  });

  it('combines every filter', () => {
    expect(ids(f({ systems: [1] }))).toEqual([1, 2]);
    expect(ids(f({ systems: [1], art: 'missing-cover' }))).toEqual([1]);
    expect(ids(f({ exts: ['smc', 'gb'] }))).toEqual([0, 2]);
    expect(ids(f({ regions: ['europe', 'spain'] }))).toEqual([0, 2]);
    expect(ids(f({ modifiedFrom: 150, modifiedTo: 300 }))).toEqual([1, 2]);
    expect(ids(f({ sizeMin: 1000, sizeMax: 10_000 }))).toEqual([1]);
    expect(ids(f({ systems: [1], regions: ['usa'], query: 'mario' }))).toEqual([
      2,
    ]);
  });

  it('counts active filters, not the query', () => {
    expect(activeFilterCount(EMPTY_FILTERS)).toBe(0);
    expect(
      activeFilterCount(
        f({
          query: 'x',
          art: 'complete',
          systems: [1],
          sizeMin: 5,
          modifiedTo: 9,
        }),
      ),
    ).toBe(4);
  });
});

describe('computeFacets', () => {
  it('counts each facet with every other filter applied', () => {
    const view = generateLibrary({
      seed: 7,
      count: 240,
      root: '/m',
      generation: 1,
      now: FIXTURE_NOW,
    });
    const keys = buildSearchKeys(view.entries);
    const filters = f({ art: 'missing-cover', systems: [3] });
    const facets = computeFacets(view.entries, keys, filters);
    const inSystem3 = view.entries.filter((x) => x.system === 3);
    // The art facet ignores the art filter but keeps the system filter.
    expect(facets.art['missing-cover']).toBe(
      inSystem3.filter((x) => x.art.cover === 'missing').length,
    );
    expect(facets.art.all).toBe(inSystem3.length);
    // The systems facet ignores the system filter but keeps the art filter.
    for (const s of view.systems) {
      expect(facets.systems.get(s.index) ?? 0).toBe(
        view.entries.filter(
          (x) => x.system === s.index && x.art.cover === 'missing',
        ).length,
      );
    }
    // Facet counts agree with applyFilters.
    expect(facets.systems.get(3) ?? 0).toBe(
      applyFilters(view.entries, keys, filters).length,
    );
  });
});

describe('date inputs', () => {
  it('turns local dates into inclusive day bounds and back', () => {
    const start = dayStart('2026-10-16');
    const end = dayEnd('2026-10-16');
    expect(start).not.toBeNull();
    expect(end! - start!).toBe(86_399);
    expect(toDateInput(start)).toBe('2026-10-16');
    expect(dayStart('')).toBeNull();
    expect(toDateInput(null)).toBe('');
  });
});
