/**
 * The Details view's column catalogue (spec §B.4.2): what each column is
 * called, how wide it starts, how narrow it may get and what it sorts by,
 * plus the pure edits the column picker and the resize handles make.
 */
import type { ReactNode } from 'react';

import type { EntryView } from '@/ipc/types';

import type { SortKey } from './sort';
import {
  DETAILS_COLUMN_IDS,
  type Density,
  type DetailsColumn,
  type DetailsColumnId,
} from './viewPrefs';

export interface ColumnDef<Ctx = unknown> {
  id: DetailsColumnId;
  /** i18n key of the header. */
  labelKey: string;
  /** Clicking the header sorts by this; absent for columns that do not sort. */
  sortKey?: SortKey;
  defaultWidth: number;
  minWidth: number;
  render(e: EntryView, ctx: Ctx): ReactNode;
}

type Def = Omit<ColumnDef, 'render'>;

function def(
  id: DetailsColumnId,
  defaultWidth: number,
  minWidth: number,
  sortKey?: SortKey,
): Def {
  const d: Def = {
    id,
    labelKey: `layouts.column.${id}`,
    defaultWidth,
    minWidth,
  };
  if (sortKey) d.sortKey = sortKey;
  return d;
}

export const COLUMN_DEFS: Readonly<Record<DetailsColumnId, Def>> =
  Object.freeze({
    thumb: def('thumb', 40, 32),
    name: def('name', 200, 120, 'name'),
    system: def('system', 160, 96, 'system'),
    folder: def('folder', 160, 80),
    regions: def('regions', 96, 56),
    ext: def('ext', 72, 48),
    size: def('size', 90, 64, 'size'),
    mtime: def('mtime', 128, 88, 'mtime'),
    cover: def('cover', 88, 48),
    title: def('title', 88, 48),
    screenshot: def('screenshot', 88, 48),
  });

/** Rows per density; compact still meets the 24px target size (WCAG 2.5.8). */
export const ROW_HEIGHT: Readonly<Record<Density, number>> = Object.freeze({
  comfortable: 34,
  compact: 24,
});

/** Comfortable rows (today's height); kept for existing imports. */
export const ROW_H = ROW_HEIGHT.comfortable;

/** Double-click on a header edge fits the column within these bounds. */
export const FIT_MIN = 60;
export const FIT_MAX = 600;
/** Dragging never makes a column wider than this. */
export const MAX_COLUMN_WIDTH = 600;
/** The cell's horizontal padding (0.5rem each side) plus a pixel of slack. */
const CELL_PADDING = 17;

const catalogueIndex = (id: DetailsColumnId) => DETAILS_COLUMN_IDS.indexOf(id);

/** Shows or hides `id`. A shown column goes back to its catalogue place,
 *  after the last visible column that comes before it. The name stays. */
export function toggleColumn(
  cols: readonly DetailsColumn[],
  id: DetailsColumnId,
): DetailsColumn[] {
  if (id === 'name') return [...cols];
  if (cols.some((c) => c.id === id)) return cols.filter((c) => c.id !== id);
  const at = catalogueIndex(id);
  let insert = 0;
  cols.forEach((c, i) => {
    if (catalogueIndex(c.id) < at) insert = i + 1;
  });
  return [...cols.slice(0, insert), { id }, ...cols.slice(insert)];
}

/** Moves `id` one place left (-1) or right (1); stays put at the ends. */
export function moveColumn(
  cols: readonly DetailsColumn[],
  id: DetailsColumnId,
  dir: -1 | 1,
): DetailsColumn[] {
  const from = cols.findIndex((c) => c.id === id);
  const to = from + dir;
  const next = [...cols];
  if (from < 0 || to < 0 || to >= cols.length) return next;
  [next[from], next[to]] = [next[to]!, next[from]!];
  return next;
}

/** Sets the width of `id`, rounded and clamped to its minimum. */
export function resizeColumn(
  cols: readonly DetailsColumn[],
  id: DetailsColumnId,
  width: number,
): DetailsColumn[] {
  const { minWidth } = COLUMN_DEFS[id];
  const px = Math.min(MAX_COLUMN_WIDTH, Math.max(minWidth, Math.round(width)));
  return cols.map((c) => (c.id === id ? { id, width: px } : c));
}

/** The width that fits `textWidth` px of content, within 60–600. */
export function fitWidth(textWidth: number): number {
  return Math.min(
    FIT_MAX,
    Math.max(FIT_MIN, Math.ceil(textWidth + CELL_PADDING)),
  );
}

/** The current width of a column. */
export function widthOf(col: DetailsColumn): number {
  return col.width ?? COLUMN_DEFS[col.id].defaultWidth;
}

/** CSS `grid-template-columns`: fixed widths, the name takes what is left. */
export function gridTemplate(cols: readonly DetailsColumn[]): string {
  return cols
    .map((c) =>
      c.id === 'name' ? `minmax(${widthOf(c)}px,1fr)` : `${widthOf(c)}px`,
    )
    .join(' ');
}
