import { act, renderHook, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { setBackend } from '@/ipc/backend';
import { createMockBackend } from '@/ipc/mock/mockBackend';

import { setNoEmulatorIds, useNoEmulator } from './useNoEmulator';

afterEach(() => {
  setNoEmulatorIds(null);
  setBackend(null);
});

describe('useNoEmulator', () => {
  it('is false for no system and for a system with an emulator', () => {
    setNoEmulatorIds(['xboxseries']);
    expect(renderHook(() => useNoEmulator(undefined)).result.current).toBe(
      false,
    );
    expect(renderHook(() => useNoEmulator('ps4')).result.current).toBe(false);
    expect(renderHook(() => useNoEmulator('xboxseries')).result.current).toBe(
      true,
    );
  });

  it('asks the catalogue again after a failed read', async () => {
    const b = createMockBackend({ delayMs: 0 });
    const real = b.listSystems.bind(b);
    const list = vi
      .spyOn(b, 'listSystems')
      .mockRejectedValueOnce(new Error('not ready'))
      .mockImplementation(real);
    setBackend(b);
    const first = renderHook(() => useNoEmulator('xboxseries'));
    await waitFor(() => expect(list).toHaveBeenCalledTimes(1));
    await act(async () => {});
    expect(first.result.current).toBe(false);
    first.unmount();
    // The next shelf header to mount reads it again instead of keeping
    // the failure for the rest of the session.
    const second = renderHook(() => useNoEmulator('xboxseries'));
    await waitFor(() => expect(second.result.current).toBe(true));
    expect(list).toHaveBeenCalledTimes(2);
  });
});
