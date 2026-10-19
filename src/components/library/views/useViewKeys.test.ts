import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { useViewKeys, type ViewKeysOptions } from './useViewKeys';

function options(
  over: Partial<ViewKeysOptions> & Pick<ViewKeysOptions, 'scrollToIndex'>,
): ViewKeysOptions {
  return {
    entries: [],
    sections: [],
    selectedIndex: -1,
    onSelect: () => {},
    onOpen: () => {},
    onPreview: () => {},
    move: () => null,
    ...over,
  };
}

describe('useViewKeys keeps the selection in view', () => {
  beforeEach(() => {
    vi.useFakeTimers({ toFake: ['requestAnimationFrame'] });
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('scrolls to the selection when it moves', () => {
    const scrollToIndex = vi.fn();
    const { rerender } = renderHook((o: ViewKeysOptions) => useViewKeys(o), {
      initialProps: options({ scrollToIndex, selectedIndex: 3 }),
    });
    expect(scrollToIndex).toHaveBeenLastCalledWith(3);
    rerender(options({ scrollToIndex, selectedIndex: 7 }));
    expect(scrollToIndex).toHaveBeenLastCalledWith(7);
  });

  it('scrolls again, a frame later, when the view changes size', () => {
    const scrollToIndex = vi.fn();
    const { rerender } = renderHook((o: ViewKeysOptions) => useViewKeys(o), {
      initialProps: options({ scrollToIndex, selectedIndex: 23, layout: 'a' }),
    });
    scrollToIndex.mockClear();
    rerender(options({ scrollToIndex, selectedIndex: 23, layout: 'b' }));
    expect(scrollToIndex).not.toHaveBeenCalled();
    act(() => vi.advanceTimersToNextFrame());
    expect(scrollToIndex).toHaveBeenCalledWith(23);
  });

  it('leaves the scroll alone on a resize with nothing selected', () => {
    const scrollToIndex = vi.fn();
    const { rerender } = renderHook((o: ViewKeysOptions) => useViewKeys(o), {
      initialProps: options({ scrollToIndex, layout: 'a' }),
    });
    rerender(options({ scrollToIndex, layout: 'b' }));
    act(() => vi.advanceTimersToNextFrame());
    expect(scrollToIndex).not.toHaveBeenCalled();
  });
});
