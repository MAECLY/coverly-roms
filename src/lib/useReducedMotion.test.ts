import { act, renderHook } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { useReducedMotion } from './useReducedMotion';

const original = window.matchMedia;

afterEach(() => {
  window.matchMedia = original;
  vi.restoreAllMocks();
});

describe('useReducedMotion', () => {
  it('reads the query and follows its change events', () => {
    let listener: (() => void) | null = null;
    const mq = {
      matches: true,
      addEventListener: vi.fn((_: string, cb: () => void) => {
        listener = cb;
      }),
      removeEventListener: vi.fn(),
    };
    window.matchMedia = vi.fn(() => mq) as unknown as typeof window.matchMedia;

    const { result, unmount } = renderHook(() => useReducedMotion());
    expect(result.current).toBe(true);
    expect(window.matchMedia).toHaveBeenCalledWith(
      '(prefers-reduced-motion: reduce)',
    );

    act(() => {
      mq.matches = false;
      listener?.();
    });
    expect(result.current).toBe(false);

    unmount();
    expect(mq.removeEventListener).toHaveBeenCalledWith('change', listener);
  });

  it('falls back to addListener on old WebKit', () => {
    let listener: (() => void) | null = null;
    const mq = {
      matches: false,
      addListener: vi.fn((cb: () => void) => {
        listener = cb;
      }),
      removeListener: vi.fn(),
    };
    window.matchMedia = vi.fn(() => mq) as unknown as typeof window.matchMedia;

    const { result, unmount } = renderHook(() => useReducedMotion());
    expect(result.current).toBe(false);
    expect(mq.addListener).toHaveBeenCalledTimes(1);

    act(() => {
      mq.matches = true;
      listener?.();
    });
    expect(result.current).toBe(true);

    unmount();
    expect(mq.removeListener).toHaveBeenCalledWith(listener);
  });

  it('is false without matchMedia', () => {
    window.matchMedia = undefined as unknown as typeof window.matchMedia;
    const { result } = renderHook(() => useReducedMotion());
    expect(result.current).toBe(false);
  });
});
