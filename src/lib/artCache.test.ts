import { afterEach, describe, expect, it, vi } from 'vitest';

import { setBackend } from '@/ipc/backend';
import { createMockBackend } from '@/ipc/mock/mockBackend';

import {
  ART_CACHE_SIZE,
  artKey,
  clearArtCache,
  loadArtUrl,
  peekArtUrl,
} from './artCache';

describe('art cache', () => {
  afterEach(() => {
    clearArtCache();
    setBackend(null);
    vi.restoreAllMocks();
  });

  it('dedupes concurrent loads and revokes evicted URLs', async () => {
    const readArt = vi.fn(async () => ({
      bytes: new Uint8Array([1]),
      mime: 'image/png',
    }));
    setBackend({ ...createMockBackend({ delayMs: 0 }), readArt });
    let n = 0;
    vi.spyOn(URL, 'createObjectURL').mockImplementation(
      () => `blob:${(n += 1)}`,
    );
    const revoke = vi
      .spyOn(URL, 'revokeObjectURL')
      .mockImplementation(() => undefined);

    const [a, b] = await Promise.all([
      loadArtUrl(1, 0, 'cover'),
      loadArtUrl(1, 0, 'cover'),
    ]);
    expect(a).toBe(b);
    expect(readArt).toHaveBeenCalledTimes(1);
    expect(peekArtUrl(artKey(1, 0, 'cover'))).toBe(a);

    for (let id = 1; id <= ART_CACHE_SIZE; id += 1)
      await loadArtUrl(1, id, 'cover');
    expect(revoke).toHaveBeenCalledWith(a);
    expect(peekArtUrl(artKey(1, 0, 'cover'))).toBeNull();
  });

  it('does not cache failures', async () => {
    const readArt = vi.fn(async () => {
      throw new Error('nope');
    });
    setBackend({ ...createMockBackend({ delayMs: 0 }), readArt });
    await expect(loadArtUrl(1, 0, 'cover')).rejects.toThrow('nope');
    await expect(loadArtUrl(1, 0, 'cover')).rejects.toThrow('nope');
    expect(readArt).toHaveBeenCalledTimes(2);
  });
});
