import { describe, expect, it } from 'vitest';

import type { Section } from './grouping';
import { buildRowModel, moveVertical, type Row } from './rowModel';

const A: Section = { key: 'system:0', label: 'Game Boy', start: 0, count: 5 };
const B: Section = { key: 'system:1', label: 'Mega Drive', start: 5, count: 3 };

function compact(rows: readonly Row[]): string[] {
  return rows.map((r) =>
    r.kind === 'header' ? `H${r.sectionIndex}` : `${r.start}-${r.end}`,
  );
}

describe('buildRowModel', () => {
  it('emits a header per section and rows of perRow items', () => {
    const m = buildRowModel([A, B], 4, true);
    expect(compact(m.rows)).toEqual(['H0', '0-4', '4-5', 'H1', '5-8']);
    expect(m.headerRows).toEqual([0, 3]);
    expect(m.rows[3]).toMatchObject({
      kind: 'header',
      section: B,
      sectionIndex: 1,
    });
    expect(m.rows[2]).toMatchObject({ sectionIndex: 0 });
  });

  it('maps every item index to its row', () => {
    const m = buildRowModel([A, B], 4, true);
    expect(m.rowOfIndex).toHaveLength(8);
    expect(m.rowOfIndex[6]).toBe(4);
    expect(Array.from(m.rowOfIndex)).toEqual([1, 1, 1, 1, 2, 4, 4, 4]);
  });

  it('emits no header rows without headers', () => {
    const m = buildRowModel([A, B], 4, false);
    expect(compact(m.rows)).toEqual(['0-4', '4-5', '5-8']);
    expect(m.headerRows).toEqual([]);
    expect(m.rowOfIndex[6]).toBe(2);
  });

  it('emits no header for the single none section', () => {
    const all: Section = { key: 'all', label: '', start: 0, count: 3 };
    const m = buildRowModel([all], 1, true);
    expect(compact(m.rows)).toEqual(['0-1', '1-2', '2-3']);
    expect(m.headerRows).toEqual([]);
  });

  it('handles an empty list', () => {
    const m = buildRowModel(
      [{ key: 'all', label: '', start: 0, count: 0 }],
      4,
      true,
    );
    expect(m.rows).toEqual([]);
    expect(m.rowOfIndex).toHaveLength(0);
  });

  it('treats perRow below 1 as 1', () => {
    expect(compact(buildRowModel([B], 0, false).rows)).toEqual([
      '5-6',
      '6-7',
      '7-8',
    ]);
  });
});

describe('moveVertical', () => {
  // perRow 4: H0, 0-4, 4-5, H1, 5-8
  const m = buildRowModel([A, B], 4, true);

  it('keeps the column in the next items row and skips headers', () => {
    expect(moveVertical(m, 1, 1)).toBe(4); // partial row clamps
    expect(moveVertical(m, 4, 1)).toBe(5); // into the next section
    expect(moveVertical(m, 6, -1)).toBe(4);
    expect(moveVertical(m, 4, -1)).toBe(0);
  });

  it('clamps a partial row to its last item', () => {
    expect(moveVertical(m, 3, 1)).toBe(4);
  });

  it('stays put at either end', () => {
    expect(moveVertical(m, 2, -1)).toBe(2);
    expect(moveVertical(m, 7, 1)).toBe(7);
  });

  it('moves several rows for a page and stops at the last row', () => {
    expect(moveVertical(m, 1, 2)).toBe(6);
    expect(moveVertical(m, 1, 99)).toBe(6);
    expect(moveVertical(m, 7, -99)).toBe(2);
  });
});
