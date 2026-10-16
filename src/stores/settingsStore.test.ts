import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import { setBackend } from '@/ipc/backend';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import { createMockBackend } from '@/ipc/mock/mockBackend';
import { MemoryStorage } from '@/test/storage';

import { resetSettingsStore, useSettingsStore } from './settingsStore';

describe('settings store', () => {
  beforeEach(() => {
    setBackend(
      createMockBackend({
        delayMs: 0,
        now: FIXTURE_NOW,
        storage: new MemoryStorage(),
      }),
    );
    resetSettingsStore();
  });
  afterEach(() => setBackend(null));

  it('loads, saves and keeps field errors per field', async () => {
    const s = useSettingsStore.getState();
    await s.load();
    expect(useSettingsStore.getState().status).toBe('ready');
    const bad = await s.update({ network: { timeout_secs: 1 } });
    expect(bad).toHaveLength(1);
    expect(
      useSettingsStore.getState().errors['network.timeout_secs']?.code,
    ).toBe('range');
    expect(useSettingsStore.getState().settings!.network.timeout_secs).toBe(30);
    expect(await s.update({ network: { retries: 5 } })).toEqual([]);
    expect(
      useSettingsStore.getState().errors['network.timeout_secs'],
    ).toBeDefined();
    expect(await s.update({ network: { timeout_secs: 40 } })).toEqual([]);
    expect(
      useSettingsStore.getState().errors['network.timeout_secs'],
    ).toBeUndefined();
    expect(useSettingsStore.getState().settings!.network).toMatchObject({
      retries: 5,
      timeout_secs: 40,
    });
  });

  it('flags a reopen and resets', async () => {
    const s = useSettingsStore.getState();
    await s.load();
    await s.update({ library: { include_hidden: true } });
    expect(useSettingsStore.getState().reopen).toBe(true);
    await s.reset();
    expect(useSettingsStore.getState().settings!.library.include_hidden).toBe(
      false,
    );
  });

  it('moves the old language choice into the settings once', async () => {
    localStorage.setItem('coverly.locale', 'es');
    await useSettingsStore.getState().load();
    expect(useSettingsStore.getState().settings!.general.language).toBe('es');
    expect(localStorage.getItem('coverly.locale')).toBeNull();
  });
});
