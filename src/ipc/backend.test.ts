import { clearMocks, mockIPC } from '@tauri-apps/api/mocks';
import { afterEach, describe, expect, it } from 'vitest';

import { getBackend, setBackend } from './backend';
import { createMockBackend } from './mock/mockBackend';

describe('getBackend', () => {
  afterEach(() => {
    setBackend(null);
    clearMocks();
  });

  it('uses the mock in a plain browser', async () => {
    expect((await getBackend()).kind).toBe('mock');
  });

  it('uses Tauri when the IPC bridge exists', async () => {
    mockIPC(() => undefined);
    expect((await getBackend()).kind).toBe('tauri');
  });

  it('returns the backend a test installed', async () => {
    const mock = createMockBackend({ delayMs: 0 });
    setBackend(mock);
    expect(await getBackend()).toBe(mock);
  });

  it('hands concurrent first callers the same instance', async () => {
    // The Start screen asks for the backend from several effects at once;
    // two mocks would split the open library from its art and recents.
    const [a, b] = await Promise.all([getBackend(), getBackend()]);
    expect(a).toBe(b);
  });
});
