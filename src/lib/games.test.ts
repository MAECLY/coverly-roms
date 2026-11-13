import { describe, expect, it } from 'vitest';

import type { ArtMarks, ChildView, EntryView } from '@/ipc/types';
import { addonViewsLibrary } from '@/ipc/mock/addonViewsFixture';

import { buildGames, defaultRevision, gameCounts } from './games';

const NO_ART: ArtMarks = {
  cover: 'missing',
  screenshot: 'missing',
  title: 'missing',
};

const e = (over: Partial<EntryView>): EntryView => ({
  id: 0,
  system: 0,
  rel_path: '',
  dir: '',
  name: '',
  ext: 'sfc',
  size: 1,
  mtime: 0,
  regions: [],
  art: NO_ART,
  ...over,
});

const kid = (
  index: number,
  role: ChildView['role'],
  version?: string,
): ChildView => ({
  index,
  rel_path: `G/${role}/${index}`,
  name: `${role} ${index}`,
  role,
  ...(version !== undefined && { version }),
  size: 1,
  mtime: 0,
  art: { icon: false, banner: false },
});

const starCourier = () => [
  e({
    id: 0,
    name: 'Star Courier (USA)',
    rel_path: 'SFC/Star Courier (USA).sfc',
    version_group: 'g',
  }),
  e({
    id: 1,
    name: 'Star Courier (USA) (Rev 1)',
    rel_path: 'SFC/Star Courier (USA) (Rev 1).sfc',
    version_group: 'g',
    revision: 'Rev 1',
  }),
];

describe('buildGames', () => {
  it('folds revisions into one game, highest Rev by default', () => {
    const [a, b] = starCourier();
    const { games, byEntry } = buildGames([a!, b!], {});
    expect(games).toHaveLength(1);
    expect(games[0]!.id).toBe(1);
    expect(games[0]!.entry).toBe(b);
    expect(games[0]!.revisions.map((r) => r.id)).toEqual([1, 0]);
    expect(byEntry.get(0)).toBe(games[0]);
    expect(byEntry.get(1)).toBe(games[0]);
    expect(gameCounts(games, []).moreRevisions).toBe(1);
  });

  it('honours a chosen default and falls back when it is stale', () => {
    const [a, b] = starCourier();
    const chosen = buildGames([a!, b!], { g: 'SFC/Star Courier (USA).sfc' });
    expect(chosen.games[0]!.id).toBe(0);
    expect(chosen.games[0]!.revisions.map((r) => r.id)).toEqual([0, 1]);
    expect(buildGames([a!, b!], { g: 'SFC/gone.sfc' }).games[0]!.id).toBe(1);
  });

  it('compares a chosen path by component, whatever the separator', () => {
    const [a, b] = starCourier();
    expect(
      buildGames([a!, b!], { g: 'SFC\\Star Courier (USA).sfc' }).games[0]!.id,
    ).toBe(0);
  });

  it('a group of one is a plain game', () => {
    const lone = e({ id: 3, name: 'Lone', version_group: 'solo' });
    const { games } = buildGames([lone], {});
    expect(games[0]!.revisions).toEqual([lone]);
  });

  it('keeps games in entry order, the group where its first file is', () => {
    const [a, b] = starCourier();
    const first = e({ id: 2, name: 'Alpha' });
    const last = e({ id: 3, name: 'Omega' });
    const { games } = buildGames([first, a!, last, b!], {});
    expect(games.map((g) => g.id)).toEqual([2, 1, 3]);
  });

  it('splits children into updates and DLC and carries the version facts', () => {
    const game = e({
      id: 0,
      name: 'Tidewalker',
      children: [kid(0, 'update', '1.0.2'), kid(1, 'dlc')],
      installed: '1.0.2',
      newer_elsewhere: { entry: 1, version: '1.3.0' },
    });
    const orphan = e({ id: 1, orphan: 'update', orphan_of: 0 });
    const { games, orphans, byEntry } = buildGames([game, orphan], {});
    expect(games).toHaveLength(1);
    const g = games[0]!;
    expect(g.updates.map((c) => c.index)).toEqual([0]);
    expect(g.dlc.map((c) => c.index)).toEqual([1]);
    expect(g.installed).toBe('1.0.2');
    expect(g.missing).toEqual({ entry: 1, version: '1.3.0' });
    expect(orphans.map((o) => o.id)).toEqual([1]);
    expect(orphans[0]!.orphan).toBe('update');
    expect(byEntry.get(1)).toBe(orphans[0]);
  });

  it('links demos both ways', () => {
    const full = e({ id: 0, name: 'Lantern Harbor (USA)' });
    const demo = e({
      id: 1,
      name: 'Lantern Harbor (Demo)',
      demo: { of: 0, of_name: 'Lantern Harbor (USA)' },
    });
    const alone = e({ id: 2, name: 'Paper Skies (Demo)', demo: {} });
    const { games } = buildGames([full, demo, alone], {});
    expect(games[0]!.demos).toEqual([1]);
    expect(games[1]!.isDemo).toBe(true);
    expect(games[1]!.demoOf).toBe(0);
    expect(games[2]!.isDemo).toBe(true);
    expect(games[2]!.demoOf).toBeNull();
  });

  it('a demo of a non-default revision lands on the game', () => {
    const [a, b] = starCourier();
    const demo = e({ id: 5, name: 'Star Courier (Demo)', demo: { of: 0 } });
    const { games } = buildGames([a!, b!, demo], {});
    expect(games[0]!.demos).toEqual([5]);
  });

  it('is memo-friendly: same input gives equal output', () => {
    const lib = addonViewsLibrary(1, 1_800_000_000);
    const one = buildGames(lib.entries, {});
    const two = buildGames(lib.entries, {});
    expect(two.games).toEqual(one.games);
    expect(two.orphans).toEqual(one.orphans);
  });

  it('builds 10 000 games in one pass', () => {
    const entries = Array.from({ length: 10_000 }, (_, id) =>
      e({
        id,
        name: `Game ${id}`,
        rel_path: `S/Game ${id}`,
        children: [kid(0, 'update', '1.0'), kid(1, 'dlc')],
        ...(id % 10 === 0 && { version_group: `g${id}` }),
      }),
    );
    const { games } = buildGames(entries, {});
    expect(games).toHaveLength(10_000);
  });
});

describe('defaultRevision', () => {
  const rev = (id: number, revision?: string, name = `G ${id}`) =>
    e({
      id,
      name,
      rel_path: `S/${name}`,
      version_group: 'g',
      ...(revision !== undefined && { revision }),
    });

  it('orders untagged < Rev 1 < Rev 2', () => {
    expect(defaultRevision([rev(0), rev(1, 'Rev 2'), rev(2, 'Rev 1')]).id).toBe(
      1,
    );
  });

  it('orders Rev A < Rev B and v1.0 < v1.1', () => {
    expect(defaultRevision([rev(0, 'Rev B'), rev(1, 'Rev A')]).id).toBe(0);
    expect(defaultRevision([rev(0, 'v1.0'), rev(1, 'v1.1')]).id).toBe(1);
    expect(defaultRevision([rev(0, 'v1.10'), rev(1, 'v1.9')]).id).toBe(0);
  });

  it('puts unparseable revisions after the parseable ones', () => {
    expect(defaultRevision([rev(0, 'Beta'), rev(1, 'Rev 1'), rev(2)]).id).toBe(
      1,
    );
    expect(defaultRevision([rev(0, 'Beta', 'B'), rev(1, 'Alt', 'A')]).id).toBe(
      1,
    );
  });

  it('takes the chosen rel_path when it is a member', () => {
    expect(defaultRevision([rev(0), rev(1, 'Rev 1')], 'S/G 0').id).toBe(0);
    expect(defaultRevision([rev(0), rev(1, 'Rev 1')], 'S/missing').id).toBe(1);
  });
});

describe('gameCounts', () => {
  it('counts games and demos, never updates, DLC, revisions or orphans', () => {
    const lantern = e({
      id: 0,
      name: 'Lantern Harbor (USA)',
      children: [
        kid(1, 'update', '1.10'),
        kid(0, 'update', '1.05'),
        kid(2, 'dlc'),
      ],
      installed: '1.10',
    });
    const demo = e({ id: 1, name: 'Lantern Harbor (Demo)', demo: { of: 0 } });
    const orphan = e({
      id: 2,
      name: 'Lantern Harbor (v1.20)',
      orphan: 'update',
    });
    const { games, orphans } = buildGames([lantern, demo, orphan], {});
    expect(gameCounts(games, orphans)).toEqual({
      games: 2,
      demos: 1,
      updates: 2,
      dlc: 1,
      moreRevisions: 0,
      orphans: 1,
    });
  });

  it('counts the mock library', () => {
    const lib = addonViewsLibrary(1, 1_800_000_000);
    const { games, orphans } = buildGames(lib.entries, {});
    expect(gameCounts(games, orphans)).toEqual({
      games: 9,
      demos: 2,
      updates: 4,
      dlc: 5,
      moreRevisions: 1,
      orphans: 2,
    });
  });
});
