import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { setBackend } from '@/ipc/backend';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import { createMockBackend, type MockBackend } from '@/ipc/mock/mockBackend';
import type { MockUpdateScenario } from '@/ipc/mock/mockUpdates';
import type { JobView } from '@/ipc/types';
import { MemoryStorage } from '@/test/storage';

import { useJobStore } from './jobStore';
import { resetUpdateStore, useUpdateStore } from './updateStore';

let backend: MockBackend;
function use(update: MockUpdateScenario) {
  backend = createMockBackend({
    delayMs: 0,
    now: FIXTURE_NOW,
    storage: new MemoryStorage(),
    update,
  });
  setBackend(backend);
}
const s = () => useUpdateStore.getState();
const running = { id: 1, state: 'applying' } as unknown as JobView;

describe('update store', () => {
  beforeEach(() => {
    resetUpdateStore();
    useJobStore.getState().reset();
  });
  afterEach(() => setBackend(null));

  it('startup with the setting off makes no request at all', async () => {
    use('available');
    await s().startupCheck(false);
    expect(backend.updateLog).toEqual([]);
    expect(s().status).toBe('idle');
  });

  it('startup with the setting on checks once per page', async () => {
    use('available');
    await s().startupCheck(true);
    await s().startupCheck(true);
    expect(backend.updateLog).toEqual(['check:startup']);
    expect(s().status).toBe('available');
    expect(s().offer?.version).toMatch(/^\d+\.\d+\.0$/);
  });

  it('startup offline is silent; manual offline shows the error', async () => {
    use('offline');
    await s().startupCheck(true);
    expect(s().status).toBe('idle');
    expect(s().error).toBeNull();
    await s().check('manual');
    expect(s().status).toBe('error');
    expect(s().error?.code).toBe('update-offline');
  });

  it('download reports progress, installs and asks for a restart', async () => {
    use('available');
    await s().check('manual');
    const seen: number[] = [];
    const off = useUpdateStore.subscribe(
      (st) => st.progress && seen.push(st.progress.downloaded),
    );
    await s().download();
    off();
    expect(seen.length).toBeGreaterThanOrEqual(5);
    expect(s().status).toBe('ready');
    expect(backend.updateLog).toEqual(['check:manual', 'download', 'install']);
  });

  it('a bad signature is an error and nothing is installed', async () => {
    use('bad-signature');
    await s().check('manual');
    await s().download();
    expect(s().status).toBe('error');
    expect(s().error?.code).toBe('update-signature');
    expect(backend.updateLog).not.toContain('install');
  });

  it('install waits for a running fill job', async () => {
    use('available');
    useJobStore.setState({ job: running });
    await s().check('manual');
    await s().download();
    expect(s().status).toBe('waiting-for-job');
    expect(backend.updateLog).not.toContain('install');
    useJobStore.setState({ job: { ...running, state: 'done' } as JobView });
    await vi.waitFor(() => expect(s().status).toBe('ready'));
    expect(backend.updateLog).toContain('install');
  });

  it('a paused fill job also holds the install', async () => {
    use('available');
    useJobStore.setState({ job: { ...running, state: 'paused' } as JobView });
    await s().check('manual');
    await s().download();
    expect(s().status).toBe('waiting-for-job');
    expect(backend.updateLog).not.toContain('install');
  });

  it('restart is refused while a fill job runs', async () => {
    use('available');
    await s().check('manual');
    await s().download();
    useJobStore.setState({ job: running });
    await s().restart();
    expect(backend.updateLog).not.toContain('restart');
    useJobStore.setState({ job: null });
    await s().restart();
    expect(backend.updateLog).toContain('restart');
  });

  it('a build without a key says so', async () => {
    use('no-key');
    await s().check('manual');
    expect(s().error?.code).toBe('update-no-key');
  });
});
