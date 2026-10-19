/**
 * Flat rows for the virtualisers (spec §B.7): header rows and rows of
 * `perRow` items. Built from section ranges only, so a column-count change
 * (resize, size stop) never regroups the entries.
 */
import { ALL_SECTION_KEY, type Section } from './grouping';

export type Row =
  | { kind: 'header'; section: Section; sectionIndex: number }
  | { kind: 'items'; start: number; end: number; sectionIndex: number };

export interface RowModel {
  rows: readonly Row[];
  /** Item index → row index (`-1` for an index outside every section). */
  rowOfIndex: Int32Array;
  /** Indices of the header rows, ascending (sticky headers, group jumps). */
  headerRows: readonly number[];
}

/** Sections that cover `count` entries exactly, or one section for all. */
export function coveringSections(
  sections: readonly Section[],
  count: number,
): readonly Section[] {
  let next = 0;
  for (const s of sections) {
    if (s.start !== next) break;
    next += s.count;
  }
  return sections.length > 0 && next === count
    ? sections
    : [{ key: ALL_SECTION_KEY, label: '', start: 0, count }];
}

/** `perRow` items per row (1 for lists). With one 'none' section no header row is emitted. */
export function buildRowModel(
  sections: readonly Section[],
  perRow: number,
  withHeaders: boolean,
): RowModel {
  const step = Math.max(1, Math.floor(perRow) || 1);
  const headers =
    withHeaders &&
    !(sections.length === 1 && sections[0]?.key === ALL_SECTION_KEY);
  let total = 0;
  for (const s of sections) total = Math.max(total, s.start + s.count);
  const rowOfIndex = new Int32Array(total).fill(-1);
  const rows: Row[] = [];
  const headerRows: number[] = [];
  sections.forEach((section, sectionIndex) => {
    if (headers) {
      headerRows.push(rows.length);
      rows.push({ kind: 'header', section, sectionIndex });
    }
    const end = section.start + section.count;
    for (let start = section.start; start < end; start += step) {
      const stop = Math.min(end, start + step);
      rowOfIndex.fill(rows.length, start, stop);
      rows.push({ kind: 'items', start, end: stop, sectionIndex });
    }
  });
  return { rows, rowOfIndex, headerRows };
}

/**
 * ↑/↓ (and a page of them) in a grid of rows: the item in the same column
 * `delta` items rows away, skipping header rows. A shorter row (a section's
 * partial last row) clamps to its last item; at either end the move stops
 * at the furthest row there is, so the index stays put at the edge.
 */
export function moveVertical(
  model: RowModel,
  index: number,
  delta: number,
): number {
  const from = model.rowOfIndex[index] ?? -1;
  const row = model.rows[from];
  if (row?.kind !== 'items' || delta === 0) return index;
  const column = index - row.start;
  const dir = delta > 0 ? 1 : -1;
  let left = Math.abs(delta);
  let target = row;
  for (
    let r = from + dir;
    left > 0 && r >= 0 && r < model.rows.length;
    r += dir
  ) {
    const next = model.rows[r];
    if (next?.kind !== 'items') continue;
    target = next;
    left--;
  }
  return Math.min(target.start + column, target.end - 1);
}
