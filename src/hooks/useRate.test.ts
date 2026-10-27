import { act, renderHook } from '@testing-library/react';
import { useRef } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { useRate } from './useRate';

let now = 0;

beforeEach(() => {
  vi.useFakeTimers();
  now = 0;
  vi.spyOn(performance, 'now').mockImplementation(() => now);
});
afterEach(() => {
  vi.restoreAllMocks();
  vi.useRealTimers();
});

/** Moves the fake clock and the fake timers together. */
const tick = (ms: number) =>
  act(() => {
    now += ms;
    vi.advanceTimersByTime(ms);
  });

describe('useRate', () => {
  it('is null while measuring, then reports the slope', () => {
    const { result, rerender } = renderHook(
      ({ k, v }: { k: string | null; v: number | null }) => useRate(k, v),
      { initialProps: { k: 'a', v: 0 } },
    );
    expect(result.current).toBeNull();
    tick(1000);
    rerender({ k: 'a', v: 100 });
    tick(1000);
    rerender({ k: 'a', v: 200 });
    tick(500);
    rerender({ k: 'a', v: 250 });
    tick(500);
    expect(result.current).toBeCloseTo(100, 5);
  });

  it('keeps the last rate across a key change', () => {
    const { result, rerender } = renderHook(
      ({ k, v }: { k: string | null; v: number | null }) => useRate(k, v),
      { initialProps: { k: 'a', v: 0 } },
    );
    tick(1000);
    rerender({ k: 'a', v: 50 });
    tick(1000);
    rerender({ k: 'a', v: 100 });
    tick(500);
    expect(result.current).toBeCloseTo(50, 5);
    rerender({ k: 'b', v: 0 });
    tick(500);
    expect(result.current).toBeCloseTo(50, 5);
  });

  it('is idle for a null key', () => {
    const { result } = renderHook(() => useRate(null, null));
    tick(3000);
    expect(result.current).toBeNull();
  });

  it('re-renders at most a few times a second under fast updates', () => {
    let renders = 0;
    const { rerender } = renderHook(
      ({ v }: { v: number }) => {
        const r = useRef(0);
        r.current += 1;
        renders = r.current;
        return useRate('a', v);
      },
      { initialProps: { v: 0 } },
    );
    const updates = 30;
    for (let i = 1; i <= updates; i++) {
      tick(100);
      rerender({ v: i * 10 });
    }
    // 3 s of fake time at 10 updates per second: besides the first render
    // and one per props update, the hook adds at most 3 a second.
    expect(renders - 1 - updates).toBeLessThanOrEqual(3 * 3);
    expect(renders - 1 - updates).toBeGreaterThan(0);
  });
});
