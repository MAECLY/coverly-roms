import { defaultRangeExtractor, type Range } from '@tanstack/react-virtual';
import { useCallback } from 'react';

const NO_HEADERS: readonly number[] = [];

/** The last of `headerRows` (ascending) at or before `row`, or null. */
export function stickyHeaderAt(
  headerRows: readonly number[],
  row: number,
): number | null {
  let lo = 0;
  let hi = headerRows.length - 1;
  let hit: number | null = null;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    const value = headerRows[mid]!;
    if (value <= row) {
      hit = value;
      lo = mid + 1;
    } else {
      hi = mid - 1;
    }
  }
  return hit;
}

/**
 * rangeExtractor for TanStack Virtual that always includes `activeRow` and the
 * sticky header row of the active section (`stickyRow`), sorted and unique.
 * With `headerRows`, the header of the section at the top of the range is
 * kept too, so it can stick while its first rows scroll away (spec §B.7).
 * `aria-activedescendant` then always points at an element that exists.
 */
export function useActiveRange(
  activeRow: number,
  stickyRow: number | null,
  headerRows: readonly number[] = NO_HEADERS,
): (range: Range) => number[] {
  return useCallback(
    (range: Range) => {
      const rows = new Set(defaultRangeExtractor(range));
      const top = stickyHeaderAt(headerRows, range.startIndex);
      for (const r of [activeRow, stickyRow, top]) {
        if (r !== null && r >= 0 && r < range.count) rows.add(r);
      }
      return [...rows].sort((a, b) => a - b);
    },
    [activeRow, stickyRow, headerRows],
  );
}
