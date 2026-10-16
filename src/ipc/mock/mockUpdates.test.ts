import { afterEach, describe, expect, it } from 'vitest';

import { version as APP_VERSION } from '../../../package.json';
import { createMockBackend } from './mockBackend';
import {
  createMockUpdates,
  MOCK_UPDATE_KEY,
  nextMinor,
  readMockUpdateScenario,
} from './mockUpdates';

describe('mock updates', () => {
  afterEach(() => {
    window.history.replaceState(null, '', '/');
    window.localStorage.removeItem(MOCK_UPDATE_KEY);
  });

  it('offers the next minor version', () => {
    expect(nextMinor('0.0.1')).toBe('0.1.0');
    expect(nextMinor('1.4.2-beta.1')).toBe('1.5.0');
  });

  it('reads the scenario from the URL, then storage, then defaults to up-to-date', () => {
    window.history.replaceState(null, '', '/?mockUpdate=offline');
    expect(readMockUpdateScenario()).toBe('offline');
    window.history.replaceState(null, '', '/?mockUpdate=evil');
    expect(readMockUpdateScenario()).toBe('up-to-date');
    window.localStorage.setItem(MOCK_UPDATE_KEY, 'no-key');
    expect(readMockUpdateScenario()).toBe('no-key');
    window.history.replaceState(null, '', '/?mockUpdate=available');
    expect(readMockUpdateScenario()).toBe('available');
    window.localStorage.setItem(MOCK_UPDATE_KEY, 'evil');
    window.history.replaceState(null, '', '/');
    expect(readMockUpdateScenario()).toBe('up-to-date');
  });

  it('a mock backend is up to date unless a scenario is asked for', async () => {
    const b = createMockBackend({ delayMs: 0, storage: null });
    await expect(b.checkForUpdates('startup')).resolves.toEqual({
      kind: 'up-to-date',
      current: APP_VERSION,
      latest: APP_VERSION,
    });
    expect(b.updateLog).toEqual(['check:startup']);
  });

  it('refuses steps out of order and installs only without a busy job', async () => {
    let busy = true;
    const log: string[] = [];
    const u = createMockUpdates({
      scenario: 'available',
      tick: () => Promise.resolve(),
      log,
      jobBusy: () => busy,
    });
    await expect(u.downloadUpdate()).rejects.toMatchObject({
      code: 'update-not-ready',
    });
    await u.checkForUpdates('manual');
    await expect(u.installUpdate()).rejects.toMatchObject({
      code: 'update-not-ready',
    });
    await u.downloadUpdate();
    await expect(u.installUpdate()).rejects.toMatchObject({
      code: 'job-running',
    });
    busy = false;
    await expect(u.installUpdate()).resolves.toEqual({
      restart_required: true,
    });
    await u.restartApp();
    expect(log).toEqual(['check:manual', 'download', 'install', 'restart']);
  });

  it('like Rust, a check never drops a downloaded or installed update', async () => {
    const log: string[] = [];
    const u = createMockUpdates({
      scenario: 'available',
      tick: () => Promise.resolve(),
      log,
      jobBusy: () => false,
    });
    await u.checkForUpdates('manual');
    await u.downloadUpdate();
    await expect(u.checkForUpdates('manual')).rejects.toMatchObject({
      code: 'update-failed',
    });
    await expect(u.installUpdate()).resolves.toEqual({
      restart_required: true,
    });
    await expect(u.checkForUpdates('startup')).rejects.toMatchObject({
      code: 'update-failed',
    });
    await expect(u.restartApp()).resolves.toBeUndefined();
  });
});
