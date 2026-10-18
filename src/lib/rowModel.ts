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
