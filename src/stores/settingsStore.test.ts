import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import { setBackend } from '@/ipc/backend';
import { BackendError } from '@/ipc/errors';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import {
  createMockBackend,
  MOCK_LIBRARIES,
  type MockBackend,
} from '@/ipc/mock/mockBackend';
import { resetLibraryStore, useLibraryStore } from '@/stores/libraryStore';
import { MemoryStorage } from '@/test/storage';

import { resetSettingsStore, useSettingsStore } from './settingsStore';

let backend: MockBackend;

describe('settings store', () => {
  beforeEach(() => {
    backend = createMockBackend({
      delayMs: 0,
      now: FIXTURE_NOW,
      storage: new MemoryStorage(),
    });
    setBackend(backend);
    resetSettingsStore();
    resetLibraryStore();
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
    await useLibraryStore.getState().open(MOCK_LIBRARIES[1].path);
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

  it('keeps a failed save as an error instead of throwing', async () => {
    const s = useSettingsStore.getState();
    await s.load();
    const real = backend.updateSettings.bind(backend);
    backend.updateSettings = () =>
      Promise.reject(new BackendError('settings', 'read-only'));
    expect(await s.update({ library: { include_hidden: true } })).toEqual([]);
    expect(useSettingsStore.getState().saveError?.code).toBe('settings');
    expect(useSettingsStore.getState().settings!.library.include_hidden).toBe(
      false,
    );
    backend.resetSettings = () =>
      Promise.reject(new BackendError('settings', 'read-only'));
    await expect(s.reset()).resolves.toBeUndefined();
    expect(useSettingsStore.getState().saveError?.code).toBe('settings');
    backend.updateSettings = real;
    expect(await s.update({ network: { retries: 5 } })).toEqual([]);
    expect(useSettingsStore.getState().saveError).toBeNull();
  });

  it('flags a reopen only while a library is open, and clears it on open', async () => {
    const s = useSettingsStore.getState();
    await s.load();
    await s.update({ library: { include_hidden: true } });
    expect(useSettingsStore.getState().reopen).toBe(false);
    await s.reset();
    expect(useSettingsStore.getState().reopen).toBe(false);
    await useLibraryStore.getState().open(MOCK_LIBRARIES[1].path);
    await s.update({ library: { include_hidden: true } });
    expect(useSettingsStore.getState().reopen).toBe(true);
    await useLibraryStore.getState().open(MOCK_LIBRARIES[1].path);
    expect(useSettingsStore.getState().reopen).toBe(false);
  });

  it('ignores a save answer older than one already applied', async () => {
    const s = useSettingsStore.getState();
    await s.load();
    const real = backend.updateSettings.bind(backend);
    let releaseFirst!: () => void;
    const gate = new Promise<void>((r) => (releaseFirst = r));
    let calls = 0;
    backend.updateSettings = async (patch) => {
      const n = (calls += 1);
      const out = await real(patch);
      if (n > 1) return out;
      await gate;
      // The answer as it stood when this save finished, before the next one.
      const network = { ...out.settings.network, retries: 5 };
      return { ...out, settings: { ...out.settings, network } };
    };
    const first = s.update({ network: { retries: 5 } });
    await s.update({ network: { retries: 7 } });
    releaseFirst();
    await first;
    expect(useSettingsStore.getState().settings!.network.retries).toBe(7);
  });
});
