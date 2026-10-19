import { describe, expect, it } from 'vitest';

import {
  COLUMN_DEFS,
  FIT_MAX,
  FIT_MIN,
  ROW_HEIGHT,
  fitWidth,
  gridMinWidth,
  gridTemplate,
  moveColumn,
  resizeColumn,
  toggleColumn,
} from './detailsColumns';
import {
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
