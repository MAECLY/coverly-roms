import { afterEach, describe, expect, it, vi } from 'vitest';

import { setBackend } from '@/ipc/backend';
import { createMockBackend } from '@/ipc/mock/mockBackend';
import type { ArtImage } from '@/ipc/types';

import {
  ART_CACHE_SIZE,
  artKey,
  clearArtCache,
  dedupeArtKey,
  loadArtUrl,
  loadUrl,
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

  it('revokes instead of caching a load that resolves after a clear', async () => {
    let release: () => void = () => undefined;
    const readArt = vi.fn(
      () =>
        new Promise<ArtImage>((resolve) => {
          release = () =>
            resolve({ bytes: new Uint8Array([1]), mime: 'image/png' });
        }),
    );
    setBackend({ ...createMockBackend({ delayMs: 0 }), readArt });
    vi.spyOn(URL, 'createObjectURL').mockReturnValue('blob:stale');
    const revoke = vi
      .spyOn(URL, 'revokeObjectURL')
      .mockImplementation(() => undefined);

    const pending = loadArtUrl(1, 0, 'cover');
    await vi.waitFor(() => expect(readArt).toHaveBeenCalled());
    clearArtCache();
    release();
    await pending.catch(() => undefined);
    expect(peekArtUrl(artKey(1, 0, 'cover'))).toBeNull();
    expect(revoke).toHaveBeenCalledWith('blob:stale');
  });

  it('loads a generic key once and evicts it with the art LRU', async () => {
    let n = 0;
    vi.spyOn(URL, 'createObjectURL').mockImplementation(
      () => `blob:${(n += 1)}`,
    );
    const revoke = vi
      .spyOn(URL, 'revokeObjectURL')
      .mockImplementation(() => undefined);
    const fetch = vi.fn(async () => new Blob(['x'], { type: 'image/png' }));
    const key = dedupeArtKey(7, 3, null, 'cover');
    expect(key).toBe('dedupe:7:3:g:cover');
    expect(dedupeArtKey(7, 3, 2, 'title')).toBe('dedupe:7:3:2:title');

    const [a, b] = await Promise.all([
      loadUrl(key, fetch),
      loadUrl(key, fetch),
    ]);
    expect(a).toBe(b);
    expect(await loadUrl(key, fetch)).toBe(a);
    expect(fetch).toHaveBeenCalledTimes(1);
    expect(peekArtUrl(key)).toBe(a);

    for (let i = 0; i < ART_CACHE_SIZE; i += 1)
      await loadUrl(`other:${i}`, async () => new Blob(['y']));
    expect(revoke).toHaveBeenCalledWith(a);
    expect(peekArtUrl(key)).toBeNull();
  });
});
