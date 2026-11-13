import { describe, expect, it } from 'vitest';

import { translate } from '@/i18n/core';
import type { TFunction } from '@/i18n/useT';
import { addonViewsLibrary } from '@/ipc/mock/addonViewsFixture';

import {
  COLUMN_DEFS,
  contentCell,
  FIT_MAX,
  FIT_MIN,
  ROW_HEIGHT,
  ensureContentColumns,
  fitWidth,
  gridMinWidth,
  gridTemplate,
  moveColumn,
  resizeColumn,
  toggleColumn,
  versionCell,
} from './detailsColumns';
import { buildGames } from './games';
import {
  sanitisePrefs,
  DEFAULT_PREFS,
  DETAILS_COLUMN_IDS,
  type DetailsColumn,
} from './viewPrefs';

const ids = (cols: readonly DetailsColumn[]) => cols.map((c) => c.id);
const base: DetailsColumn[] = [
  { id: 'name' },
  { id: 'system' },
  { id: 'size' },
];

describe('detailsColumns', () => {
  it('describes every column of the catalogue', () => {
    expect(Object.keys(COLUMN_DEFS).sort()).toEqual(
      [...DETAILS_COLUMN_IDS].sort(),
    );
    for (const def of Object.values(COLUMN_DEFS)) {
      expect(def.minWidth).toBeLessThanOrEqual(def.defaultWidth);
      expect(def.labelKey).toBe(`layouts.column.${def.id}`);
    }
    expect(COLUMN_DEFS.mtime.sortKey).toBe('mtime');
    expect(COLUMN_DEFS.folder.sortKey).toBeUndefined();
  });

  it('adds Versión and Contenido after the name once (D19 b)', () => {
    const saved = sanitisePrefs({
      columns: [{ id: 'name', width: 300 }, { id: 'size' }],
    });
    // A library without updates, DLC, revisions or demos: untouched.
    expect(ensureContentColumns(saved, false)).toBe(saved);
    const once = ensureContentColumns(saved, true);
    expect(once.columns).toEqual([
      { id: 'name', width: 300 },
      { id: 'version' },
      { id: 'content' },
      { id: 'size' },
    ]);
    expect(once.contentColumns).toBe(true);
    expect(ensureContentColumns(once, true)).toBe(once);
    // The user removes Versión: it stays removed.
    const removed = { ...once, columns: toggleColumn(once.columns, 'version') };
    expect(ids(ensureContentColumns(removed, true).columns)).toEqual([
      'name',
      'content',
      'size',
    ]);
    // A column the user already shows is not added twice.
    const has = sanitisePrefs({
      columns: [{ id: 'content' }, { id: 'name' }],
    });
    expect(ids(ensureContentColumns(has, true).columns)).toEqual([
      'content',
      'name',
      'version',
    ]);
  });

  it('hides and shows a column in catalogue order', () => {
    expect(ids(toggleColumn(base, 'system'))).toEqual(['name', 'size']);
    expect(ids(toggleColumn(base, 'folder'))).toEqual([
      'name',
      'system',
      'folder',
      'size',
    ]);
    expect(ids(toggleColumn(base, 'thumb'))).toEqual([
      'thumb',
      'name',
      'system',
      'size',
    ]);
  });

  it('never hides the name', () => {
    expect(ids(toggleColumn(base, 'name'))).toEqual(['name', 'system', 'size']);
  });

  it('shows a hidden column again at its default width', () => {
    const wide = resizeColumn(base, 'size', 140);
    expect(toggleColumn(toggleColumn(wide, 'size'), 'size')).toContainEqual({
      id: 'size',
    });
    expect(wide).toContainEqual({ id: 'size', width: 140 });
  });

  it('moves a column one step and clamps at the ends', () => {
    expect(ids(moveColumn(base, 'size', -1))).toEqual([
      'name',
      'size',
      'system',
    ]);
    expect(ids(moveColumn(base, 'name', 1))).toEqual([
      'system',
      'name',
      'size',
    ]);
    expect(ids(moveColumn(base, 'name', -1))).toEqual(ids(base));
    expect(ids(moveColumn(base, 'size', 1))).toEqual(ids(base));
    expect(ids(moveColumn(base, 'folder', 1))).toEqual(ids(base));
  });

  it('resizes and clamps to the column minimum', () => {
    expect(resizeColumn(base, 'size', 150)).toContainEqual({
      id: 'size',
      width: 150,
    });
    expect(resizeColumn(base, 'size', 3)).toContainEqual({
      id: 'size',
      width: COLUMN_DEFS.size.minWidth,
    });
    expect(resizeColumn(base, 'size', 151.6)).toContainEqual({
      id: 'size',
      width: 152,
    });
    expect(resizeColumn(base, 'folder', 150)).toEqual(base);
  });

  it('fits a measured text width within 60–600', () => {
    expect(fitWidth(10)).toBe(FIT_MIN);
    expect(fitWidth(5_000)).toBe(FIT_MAX);
    expect(fitWidth(100)).toBeGreaterThan(100);
  });

  it('builds the grid template in column order, the name flexible', () => {
    expect(
      gridTemplate([
        { id: 'size', width: 120 },
        { id: 'name' },
        { id: 'cover' },
      ]),
    ).toBe(
      `120px minmax(${COLUMN_DEFS.name.defaultWidth}px,1fr) ${COLUMN_DEFS.cover.defaultWidth}px`,
    );
    expect(gridTemplate(DEFAULT_PREFS.columns).split(' ')).toHaveLength(
      DEFAULT_PREFS.columns.length,
    );
  });

  it('gives every row the same minimum width, whatever its content', () => {
    expect(
      gridMinWidth([
        { id: 'name' },
        { id: 'size', width: 150 },
        { id: 'cover' },
      ]),
    ).toBe(
      COLUMN_DEFS.name.defaultWidth + 150 + COLUMN_DEFS.cover.defaultWidth,
    );
  });

  it('keeps rows at least 24px tall', () => {
    expect(ROW_HEIGHT).toEqual({ comfortable: 34, compact: 24 });
  });
});

describe('Versión and Contenido cells (addon views §8.4)', () => {
  const tEs: TFunction = (k, v) => translate('es', k, v);
  const lib = addonViewsLibrary(1, 1_800_000_000);
  const { games } = buildGames(lib.entries, {});
  const named = (prefix: string) =>
    games.find((g) => g.entry.name.startsWith(prefix))!;

  it('writes the installed version, the missing one, the revision or a dash', () => {
    const lantern = named('Lantern Harbor (USA)');
    expect(versionCell(lantern.entry, lantern, tEs)).toEqual({ text: 'v1.10' });
    const tide = named('Tidewalker');
    expect(versionCell(tide.entry, tide, tEs)).toEqual({
      text: 'v1.0.2',
      warning: '↑ falta v1.3.0',
    });
    const star = named('Star Courier');
    expect(versionCell(star.entry, star, tEs)).toEqual({ text: 'Rev 1' });
    const pebble = named('Pebble League');
    expect(versionCell(pebble.entry, pebble, tEs)).toEqual({ text: '—' });
    // Without games, the entry's own facts.
    expect(versionCell({ ...pebble.entry, version: '2.0' }, null, tEs)).toEqual(
      {
        text: 'v2.0',
      },
    );
  });

  it('sums up what a game holds', () => {
    expect(contentCell(named('Lantern Harbor (USA)'), tEs, 'es')).toBe(
      '2 act., 1 DLC, demo aparte',
    );
    expect(contentCell(named('Tidewalker'), tEs, 'es')).toBe('1 act., 1 DLC');
    expect(contentCell(named('Star Courier'), tEs, 'es')).toBe('2 revisiones');
    expect(contentCell(named('Lantern Harbor (Demo)'), tEs, 'es')).toBe(
      'Demo de Lantern Harbor (USA)',
    );
    expect(contentCell(named('Pebble League'), tEs, 'es')).toBe('—');
  });

  it('sorts by Versión and Contenido', () => {
    expect(COLUMN_DEFS.version.sortKey).toBe('version');
    expect(COLUMN_DEFS.content.sortKey).toBe('content');
  });
});
