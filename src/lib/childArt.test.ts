import { describe, expect, it } from 'vitest';

import { addonViewsLibrary } from '@/ipc/mock/addonViewsFixture';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import type { ChildView } from '@/ipc/types';

import { childArtKey, childArtState, trayChildren } from './childArt';
import { buildGames } from './games';

const child = (art: ChildView['art']): ChildView => ({
  index: 0,
  rel_path: 'PS4/G/DLC/G - Pack.pkg',
  name: 'G - Pack',
  role: 'dlc',
  size: 1,
  mtime: 0,
  art,
});

describe('childArtState', () => {
  it('uses its own icon first, with its origin', () => {
    expect(
      childArtState(
        child({ icon: true, banner: true, icon_origin: 'package' }),
        'ps4',
      ),
    ).toEqual({ kind: 'own', slot: 'icon', origin: 'package' });
    expect(childArtState(child({ icon: false, banner: true }), 'ps4')).toEqual({
      kind: 'own',
      slot: 'banner',
    });
  });

  it('shows the base art for Switch, the missing slot elsewhere', () => {
    const none = child({ icon: false, banner: false });
    expect(childArtState(none, 'switch')).toEqual({ kind: 'base' });
    expect(childArtState(none, 'ps4')).toEqual({ kind: 'missing' });
    expect(childArtState(none, null)).toEqual({ kind: 'missing' });
  });
});

describe('trayChildren', () => {
  const lib = addonViewsLibrary(1, FIXTURE_NOW);
  const { games } = buildGames(lib.entries, {});
  const lantern = games.find((g) => g.entry.name === 'Lantern Harbor (USA)')!;

  it('lists updates with their own art, then every DLC', () => {
    expect(trayChildren(lantern).map((c) => c.name)).toEqual([
      'Lantern Harbor - Lighthouse Keeper Pack',
    ]);
    const withArt = {
      ...lantern,
      updates: lantern.updates.map((u, i) =>
        i === 0 ? { ...u, art: { icon: true, banner: false } } : u,
      ),
    };
    expect(trayChildren(withArt).map((c) => c.name)).toEqual([
      'Lantern Harbor (v1.10)',
      'Lantern Harbor - Lighthouse Keeper Pack',
    ]);
  });

  it('keys a slot by generation, entry, child and slot', () => {
    expect(childArtKey(3, 7, 2, 'icon')).toBe('child:3:7:2:icon');
  });
});
