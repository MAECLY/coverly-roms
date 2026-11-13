import { describe, expect, it } from 'vitest';

import { generateLibrary, FIXTURE_NOW } from '@/ipc/mock/fixture';

import { addonViewsLibrary } from '@/ipc/mock/addonViewsFixture';
import { buildGames } from '@/lib/games';
import { cloneSettings, DEFAULT_SETTINGS } from '@/lib/settingsRules';

import {
  buildFillRequest,
  DEFAULT_REGION_ORDER,
  moveItem,
  scopeIds,
  shouldAutoRun,
  wizardDefaults,
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

  it('a folded game fills every revision, default first (D4, Q7)', () => {
    const lib = addonViewsLibrary(1, FIXTURE_NOW);
    const { games, byEntry } = buildGames(lib.entries, {});
    const id = (name: string) => lib.entries.find((e) => e.name === name)!.id;
    const untagged = id('Star Courier (USA)');
    const rev1 = id('Star Courier (USA) (Rev 1)');
    const visible = games.map((g) => g.entry);
    expect(scopeIds('selection', rev1, visible, byEntry)).toEqual([
      rev1,
      untagged,
    ]);
    // Every revision of every visible game, never an update or DLC.
    const filter = scopeIds('filter', null, visible, byEntry)!;
    expect([...filter].sort((a, b) => a - b)).toEqual(
      games.flatMap((g) => g.revisions.map((r) => r.id)).sort((a, b) => a - b),
    );
    expect(filter).toContain(untagged);
    expect(filter).toHaveLength(new Set(filter).size);
    expect(scopeIds('library', rev1, visible, byEntry)).toBeNull();
    expect(
      buildFillRequest({
        scope: 'selection',
        selectedId: rev1,
        visible,
        games: byEntry,
        media: ['cover'],
        strategy: 'priority',
        regions: ['rom'],
        overwrite: 'never',
      }).entry_ids,
    ).toEqual([rev1, untagged]);
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

describe('wizard defaults from settings', () => {
  it('uses the art and matching settings, filtered by the layout', () => {
    const s = cloneSettings(DEFAULT_SETTINGS);
    s.art.media = ['cover', 'title'];
    s.matching.strategy = 'priority';
    s.matching.regions = ['japan', 'rom'];
    expect(wizardDefaults(s, ['cover'])).toEqual({
      media: ['cover'],
      strategy: 'priority',
      regions: ['japan', 'rom'],
      overwrite: 'never',
    });
    s.art.overwrite = 'ask';
    expect(wizardDefaults(s, ['cover']).overwrite).toBeNull();
    s.art.overwrite = 'backup';
    expect(wizardDefaults(s, ['cover']).overwrite).toBe('backup');
    // Falls back to the first media the layout stores.
    s.art.media = ['title'];
    expect(wizardDefaults(s, ['cover']).media).toEqual(['cover']);
    expect(wizardDefaults(null, ['cover', 'screenshot']).media).toEqual([
      'cover',
    ]);
  });

  it('runs by itself only with dry-run-first off and nothing to review', () => {
    const s = cloneSettings(DEFAULT_SETTINGS);
    const job = {
      state: 'planned',
      plan: { totals: { auto: 3, review: 0, none: 1 } },
    } as never;
    expect(shouldAutoRun(job, s, false)).toBe(false);
    s.safety.dry_run_default = false;
    expect(shouldAutoRun(job, s, false)).toBe(true);
    expect(shouldAutoRun(job, s, true)).toBe(false);
    const review = {
      state: 'planned',
      plan: { totals: { auto: 3, review: 2, none: 0 } },
    } as never;
    expect(shouldAutoRun(review, s, false)).toBe(false);
  });
});
