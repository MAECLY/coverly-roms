import { renderHook, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { setBackend } from '@/ipc/backend';
import { BackendError } from '@/ipc/errors';
import { createMockBackend } from '@/ipc/mock/mockBackend';
import { clearArtCache } from '@/lib/artCache';

import { useDedupeArt } from './useDedupeArt';

const PNG = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);

describe('useDedupeArt', () => {
  afterEach(() => {
    clearArtCache();
    setBackend(null);
    vi.restoreAllMocks();
  });

  it('is ready with an object URL of the art', async () => {
    const dedupeArt = vi.fn(async () => PNG.slice().buffer as ArrayBuffer);
    setBackend({ ...createMockBackend({ delayMs: 0 }), dedupeArt });
    const blobs: Blob[] = [];
    vi.spyOn(URL, 'createObjectURL').mockImplementation((b) => {
      blobs.push(b as Blob);
      return 'blob:art';
    });
    vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => undefined);

    const { result } = renderHook(() =>
      useDedupeArt(1, 2, null, 'cover', true),
    );
    expect(result.current).toEqual({ status: 'loading' });
    await waitFor(() =>
      expect(result.current).toEqual({ status: 'ready', url: 'blob:art' }),
    );
    expect(dedupeArt).toHaveBeenCalledWith(1, 2, null, 'cover');
    expect(blobs[0]?.type).toBe('image/png');
  });

  it('types SVG bytes from the mock as SVG', async () => {
    const svg = new TextEncoder().encode('<svg xmlns="x"></svg>');
    const dedupeArt = vi.fn(async () => svg.buffer as ArrayBuffer);
    setBackend({ ...createMockBackend({ delayMs: 0 }), dedupeArt });
    const blobs: Blob[] = [];
    vi.spyOn(URL, 'createObjectURL').mockImplementation((b) => {
      blobs.push(b as Blob);
      return 'blob:svg';
    });
    const { result } = renderHook(() => useDedupeArt(1, 2, 3, 'cover', true));
    await waitFor(() => expect(result.current.status).toBe('ready'));
    expect(blobs[0]?.type).toBe('image/svg+xml');
  });

  it('remembers a missing image and never asks again', async () => {
    const dedupeArt = vi.fn(async () => {
      throw new BackendError('art-missing', 'cover');
    });
    setBackend({ ...createMockBackend({ delayMs: 0 }), dedupeArt });

    const first = renderHook(() => useDedupeArt(1, 2, 3, 'cover', true));
    await waitFor(() =>
      expect(first.result.current).toEqual({ status: 'missing' }),
    );
    first.unmount();
    const again = renderHook(() => useDedupeArt(1, 2, 3, 'cover', true));
    expect(again.result.current).toEqual({ status: 'missing' });
    expect(dedupeArt).toHaveBeenCalledTimes(1);
  });

  it('asks again after the cache is cleared', async () => {
    const dedupeArt = vi.fn(async () => {
      throw new BackendError('art-missing', 'cover');
    });
    setBackend({ ...createMockBackend({ delayMs: 0 }), dedupeArt });
    const first = renderHook(() => useDedupeArt(1, 2, 3, 'cover', true));
    await waitFor(() => expect(first.result.current.status).toBe('missing'));
    first.unmount();
    clearArtCache();
    renderHook(() => useDedupeArt(1, 2, 3, 'cover', true));
    await waitFor(() => expect(dedupeArt).toHaveBeenCalledTimes(2));
  });

  it('makes no request when disabled or without a plan', async () => {
    const dedupeArt = vi.fn(async () => PNG.slice().buffer as ArrayBuffer);
    setBackend({ ...createMockBackend({ delayMs: 0 }), dedupeArt });
    const off = renderHook(() => useDedupeArt(1, 2, null, 'cover', false));
    const none = renderHook(() =>
      useDedupeArt(undefined, 2, null, 'cover', true),
    );
    await new Promise((r) => setTimeout(r, 10));
    expect(off.result.current).toEqual({ status: 'loading' });
    expect(none.result.current).toEqual({ status: 'loading' });
    expect(dedupeArt).not.toHaveBeenCalled();
  });
});
