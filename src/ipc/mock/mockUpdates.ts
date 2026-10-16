/**
 * Browser stand-in for the updater (`pnpm dev`, tests). Scenarios make every
 * UI state reachable in Chrome: `?mockUpdate=available|up-to-date|offline|
 * bad-signature|no-key` (or localStorage `coverly.mock.update`).
 */
import { version as APP_VERSION } from '../../../package.json';
import type { Backend } from '../backend';
import { BackendError } from '../errors';
import type { UpdateOffer, UpdateProgress } from '../types';

export type MockUpdateScenario =
  'available' | 'up-to-date' | 'offline' | 'bad-signature' | 'no-key';
export const MOCK_UPDATE_SCENARIOS: readonly MockUpdateScenario[] = [
  'available',
  'up-to-date',
  'offline',
  'bad-signature',
  'no-key',
];
export const MOCK_UPDATE_KEY = 'coverly.mock.update';
export const MOCK_UPDATE_TOTAL = 12_582_912;

export function nextMinor(v: string): string {
  const [major = '0', minor = '0'] = (v.split('-')[0] ?? v).split('.');
  return `${major}.${Number(minor) + 1}.0`;
}

const isScenario = (v: unknown): v is MockUpdateScenario =>
  typeof v === 'string' &&
  (MOCK_UPDATE_SCENARIOS as readonly string[]).includes(v);

export function readMockUpdateScenario(): MockUpdateScenario {
  try {
    const fromUrl = new URLSearchParams(window.location.search).get(
      'mockUpdate',
    );
    if (isScenario(fromUrl)) return fromUrl;
    const stored = window.localStorage.getItem(MOCK_UPDATE_KEY);
    if (isScenario(stored)) return stored;
  } catch {
    /* no window or storage */
  }
  // Opt-in: existing tests and `pnpm dev` see no update unless they ask.
  return 'up-to-date';
}

export type MockUpdatesApi = Pick<
  Backend,
  | 'appInfo'
  | 'checkForUpdates'
  | 'downloadUpdate'
  | 'onUpdateProgress'
  | 'installUpdate'
  | 'restartApp'
>;

export function createMockUpdates(opts: {
  scenario: MockUpdateScenario;
  tick: () => Promise<void>;
  log: string[];
  jobBusy: () => boolean;
}): MockUpdatesApi {
  const listeners = new Set<(p: UpdateProgress) => void>();
  let phase: 'idle' | 'available' | 'downloaded' | 'installed' = 'idle';
  const offer: UpdateOffer = {
    version: nextMinor(APP_VERSION),
    notes: 'Faster scans, two new layouts and fixes.',
    date: '2026-10-16T00:00:00Z',
  };
  return {
    appInfo: () =>
      Promise.resolve({
        name: 'Coverly ROMs',
        version: APP_VERSION,
        core_version: APP_VERSION,
      }),
    async checkForUpdates(trigger) {
      opts.log.push(`check:${trigger}`);
      await opts.tick();
      // As Rust: a check would drop the update already on its way.
      if (phase === 'downloaded' || phase === 'installed')
        throw new BackendError(
          'update-failed',
          'an update is already being installed',
        );
      if (opts.scenario === 'offline')
        throw new BackendError('update-offline', 'mock: no connection');
      if (opts.scenario === 'no-key') throw new BackendError('update-no-key');
      if (opts.scenario === 'up-to-date') {
        phase = 'idle';
        return {
          kind: 'up-to-date',
          current: APP_VERSION,
          latest: APP_VERSION,
        };
      }
      phase = 'available';
      return { kind: 'available', current: APP_VERSION, offer };
    },
    async downloadUpdate() {
      if (phase !== 'available') throw new BackendError('update-not-ready');
      opts.log.push('download');
      for (let i = 1; i <= 5; i += 1) {
        await opts.tick();
        const p = {
          downloaded: Math.round((MOCK_UPDATE_TOTAL * i) / 5),
          total: MOCK_UPDATE_TOTAL,
        };
        listeners.forEach((cb) => cb(p));
      }
      if (opts.scenario === 'bad-signature') {
        phase = 'idle';
        throw new BackendError(
          'update-signature',
          'mock: the signature does not match the file',
        );
      }
      phase = 'downloaded';
    },
    onUpdateProgress(cb) {
      listeners.add(cb);
      return Promise.resolve(() => {
        listeners.delete(cb);
      });
    },
    async installUpdate() {
      if (phase !== 'downloaded') throw new BackendError('update-not-ready');
      if (opts.jobBusy()) throw new BackendError('job-running');
      opts.log.push('install');
      await opts.tick();
      phase = 'installed';
      return { restart_required: true };
    },
    async restartApp() {
      if (phase !== 'installed') throw new BackendError('update-not-ready');
      if (opts.jobBusy()) throw new BackendError('job-running');
      opts.log.push('restart');
    },
  };
}
