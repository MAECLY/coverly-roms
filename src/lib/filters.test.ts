import { describe, expect, it } from 'vitest';

import { FIXTURE_NOW, generateLibrary } from '@/ipc/mock/fixture';
import type { ArtMark, ChildView, EntryView } from '@/ipc/types';

import {
  activeFilterCount,
  applyFilters,
  applyGameFilters,
  computeFacets,
  contentActive,
  contentFacets,
  dayEnd,
  dayStart,
  EMPTY_FILTERS,
  matchesArt,
  toDateInput,
  type Filters,
} from './filters';
import type { Game } from './games';
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

const child = (index: number, role: ChildView['role']): ChildView => ({
  index,
  rel_path: `G/${role}/${String(index)}`,
  name: `${role} ${String(index)}`,
  role,
  size: 1,
  mtime: 0,
  art: { icon: false, banner: false },
});

function game(id: number, over: Partial<Game> = {}): Game {
  const updates = over.updates ?? [];
  const dlc = over.dlc ?? [];
  const entry = e(id);
  return {
    id,
    entry,
    revisions: [entry],
    children: [...updates, ...dlc],
    updates,
    dlc,
    isDemo: false,
    demoOf: null,
    demos: [],
    installed: null,
    missing: null,
    orphan: null,
    ...over,
  };
}

// Lantern Harbor: updates and DLC. Tidewalker: an update, DLC and a newer
// update outside its folder. Star Courier: nothing. Its demo: a demo.
const lantern = game(0, {
  updates: [child(0, 'update'), child(1, 'update')],
  dlc: [child(2, 'dlc')],
  installed: '1.10',
});
const tidewalker = game(1, {
  updates: [child(0, 'update')],
  dlc: [child(1, 'dlc')],
  installed: '1.0.2',
  missing: { entry: 9, version: '1.3.0' },
});
const courier = game(2);
const demo = game(3, { isDemo: true, demoOf: 0 });
const GAMES = [lantern, tidewalker, courier, demo];

const content = (over: Partial<Filters['content']>): Filters =>
  f({ content: { ...EMPTY_FILTERS.content, ...over } });
const ids = (games: readonly Game[]) => games.map((g) => g.id);

describe('content filters (D14)', () => {
  it('start off and keep every game', () => {
    expect(EMPTY_FILTERS.content).toEqual({
      updates: false,
      missing: false,
      dlc: false,
      demos: false,
    });
    expect(contentActive(EMPTY_FILTERS.content)).toBe(false);
    expect(applyGameFilters(GAMES, EMPTY_FILTERS)).toEqual(GAMES);
  });

  it('each chip keeps the games it names', () => {
    expect(ids(applyGameFilters(GAMES, content({ updates: true })))).toEqual([
      0, 1,
    ]);
    expect(ids(applyGameFilters(GAMES, content({ missing: true })))).toEqual([
      1,
    ]);
    expect(ids(applyGameFilters(GAMES, content({ dlc: true })))).toEqual([
      0, 1,
    ]);
    expect(ids(applyGameFilters(GAMES, content({ demos: true })))).toEqual([3]);
  });

  it('combines chips with AND', () => {
    expect(
      ids(applyGameFilters(GAMES, content({ updates: true, missing: true }))),
    ).toEqual([1]);
    expect(
      ids(applyGameFilters(GAMES, content({ dlc: true, demos: true }))),
    ).toEqual([]);
    expect(contentActive(content({ dlc: true }).content)).toBe(true);
  });

  it('a title list that knows a newer version makes a game miss it (3.1 seam)', () => {
    // Star Courier has no orphan update; the fake list says it is behind.
    const behind = (g: Game) => g.id === 2;
    expect(
      ids(applyGameFilters(GAMES, content({ missing: true }), behind)),
    ).toEqual([1, 2]);
    expect(contentFacets(GAMES, behind).missing).toBe(2);
  });

  it('counts each chip over the games given, never orphans', () => {
    const orphan = game(4, { orphan: 'update' });
    expect(contentFacets([...GAMES, orphan])).toEqual({
      updates: 2,
      missing: 1,
      dlc: 2,
      demos: 1,
    });
    expect(contentFacets([])).toEqual({
      updates: 0,
      missing: 0,
      dlc: 0,
      demos: 0,
    });
  });

  it('are not panel filters: the toolbar badge ignores them', () => {
    expect(activeFilterCount(content({ dlc: true, demos: true }))).toBe(0);
  });
});
