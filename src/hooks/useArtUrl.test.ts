import { renderHook, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { setBackend } from '@/ipc/backend';
import { createMockBackend } from '@/ipc/mock/mockBackend';
import { ART_CACHE_SIZE, clearArtCache, loadArtUrl } from '@/lib/artCache';

import { useArtUrl } from './useArtUrl';

describe('useArtUrl', () => {
  afterEach(() => {
    clearArtCache();
    setBackend(null);
    vi.restoreAllMocks();
  });

  it('never returns a URL the cache has revoked', async () => {
    const readArt = vi.fn(async () => ({
      bytes: new Uint8Array([1]),
      mime: 'image/png',
    }));
    setBackend({ ...createMockBackend({ delayMs: 0 }), readArt });
    let n = 0;
    vi.spyOn(URL, 'createObjectURL').mockImplementation(
      () => `blob:${(n += 1)}`,
    );
    const revoked = new Set<string>();
    vi.spyOn(URL, 'revokeObjectURL').mockImplementation((u) => {
      revoked.add(u);
    });

    const { result, rerender } = renderHook(() =>
      useArtUrl(1, 0, 'cover', true),
    );
    await waitFor(() => expect(result.current).toBe('blob:1'));

    for (let id = 1; id <= ART_CACHE_SIZE; id += 1)
      await loadArtUrl(1, id, 'cover');
    expect(revoked.has('blob:1')).toBe(true);
    rerender();
    expect(result.current).not.toBe('blob:1');
    await waitFor(() => expect(result.current).not.toBeNull());
    expect(revoked.has(result.current!)).toBe(false);
  });
});
