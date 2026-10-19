import { renderHook } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { stickyHeaderAt, useActiveRange } from './useActiveRange';

const range = (startIndex: number, endIndex: number, count = 100) => ({
  startIndex,
  endIndex,
  overscan: 0,
  count,
});

describe('stickyHeaderAt', () => {
  it('finds the last header at or before a row', () => {
    expect(stickyHeaderAt([0, 5, 9], 0)).toBe(0);
    expect(stickyHeaderAt([0, 5, 9], 7)).toBe(5);
    expect(stickyHeaderAt([0, 5, 9], 9)).toBe(9);
    expect(stickyHeaderAt([0, 5, 9], 40)).toBe(9);
  });

  it('is null without headers or before the first one', () => {
    expect(stickyHeaderAt([], 3)).toBeNull();
    expect(stickyHeaderAt([2], 1)).toBeNull();
  });
});

describe('useActiveRange', () => {
  it('adds the active row and its header, sorted and unique', () => {
    const { result } = renderHook(() => useActiveRange(60, 55));
    expect(result.current(range(10, 12))).toEqual([10, 11, 12, 55, 60]);
  });

  it('adds the header stuck at the top of the range', () => {
    const { result } = renderHook(() => useActiveRange(-1, null, [0, 8, 20]));
    expect(result.current(range(10, 11))).toEqual([8, 10, 11]);
  });

  it('ignores rows outside the list', () => {
    const { result } = renderHook(() => useActiveRange(500, null));
    expect(result.current(range(0, 1, 3))).toEqual([0, 1]);
  });
});
