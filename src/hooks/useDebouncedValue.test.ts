import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { useDebouncedValue } from './useDebouncedValue';

describe('useDebouncedValue', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('starts with the first value', () => {
    const { result } = renderHook(() => useDebouncedValue(1, 120));
    expect(result.current).toBe(1);
  });

  it('follows only once the value stays still for the delay', () => {
    const { result, rerender } = renderHook(
      ({ v }) => useDebouncedValue(v, 120),
      { initialProps: { v: 1 } },
    );
    rerender({ v: 2 });
    act(() => vi.advanceTimersByTime(100));
    rerender({ v: 3 });
    act(() => vi.advanceTimersByTime(100));
    expect(result.current).toBe(1);
    act(() => vi.advanceTimersByTime(20));
    expect(result.current).toBe(3);
  });
});
