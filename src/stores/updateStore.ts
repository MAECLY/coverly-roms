/**
 * The in-app updater as the UI follows it (M7). The startup check runs at
 * most once per page and only when the setting is on (otherwise nothing is
 * requested); its failures are silent. Installing waits for a running fill
 * job, and restarting is refused while one runs.
 */
import { create } from 'zustand';

import { getBackend } from '@/ipc/backend';
import { BackendError, toBackendError } from '@/ipc/errors';
import type { UpdateOffer, UpdateProgress, UpdateTrigger } from '@/ipc/types';

import { blocksUpdate, useJobStore } from './jobStore';

export type UpdateStatus =
  | 'idle'
  | 'checking'
  | 'up-to-date'
  | 'available'
  | 'downloading'
  | 'waiting-for-job'
  | 'installing'
  | 'ready'
  | 'error';

interface UpdateStore {
  status: UpdateStatus;
  current: string | null;
  offer: UpdateOffer | null;
  progress: UpdateProgress | null;
  error: BackendError | null;
  trigger: UpdateTrigger | null;
  checkedAt: number | null;
  bannerDismissed: boolean;
  startupCheck(enabled: boolean): Promise<void>;
  check(trigger: UpdateTrigger): Promise<void>;
  download(): Promise<void>;
  install(): Promise<void>;
  restart(): Promise<void>;
  dismissBanner(): void;
}

const initial = {
  status: 'idle' as UpdateStatus,
  current: null,
  offer: null,
  progress: null,
  error: null,
  trigger: null,
  checkedAt: null,
  bannerDismissed: false,
};

let startupDone = false;
let waitOff: (() => void) | null = null;

const jobBusy = () => blocksUpdate(useJobStore.getState().job);

export const useUpdateStore = create<UpdateStore>()((set, get) => ({
  ...initial,
  async startupCheck(enabled) {
    if (startupDone) return;
    startupDone = true;
    if (!enabled) return; // no request at all
    await get().check('startup');
  },
  async check(trigger) {
    if (
      ['checking', 'downloading', 'installing', 'waiting-for-job'].includes(
        get().status,
      )
    )
      return;
    set({ status: 'checking', error: null, trigger });
    try {
      const b = await getBackend();
      const r = await b.checkForUpdates(trigger);
      if (r.kind === 'skipped') return set({ status: 'idle' });
      set({
        checkedAt: Date.now(),
        current: r.current,
        offer: r.kind === 'available' ? r.offer : null,
        status: r.kind === 'available' ? 'available' : 'up-to-date',
      });
    } catch (e) {
      if (trigger === 'startup') set({ status: 'idle' });
      else set({ status: 'error', error: toBackendError(e) });
    }
  },
  async download() {
    if (get().status !== 'available') return;
    set({ status: 'downloading', progress: null, error: null });
    let off: (() => void) | null = null;
    try {
      const b = await getBackend();
      off = await b.onUpdateProgress((p) => set({ progress: p }));
      await b.downloadUpdate();
    } catch (e) {
      return set({ status: 'error', error: toBackendError(e) });
    } finally {
      off?.();
    }
    await get().install();
  },
  async install() {
    // Wait for the next job store change that shows no running job.
    const waitForJob = () => {
      set({ status: 'waiting-for-job' });
      waitOff?.();
      waitOff = useJobStore.subscribe((s) => {
        if (blocksUpdate(s.job)) return;
        waitOff?.();
        waitOff = null;
        void get().install();
      });
    };
    if (jobBusy()) return waitForJob();
    set({ status: 'installing' });
    try {
      await (await getBackend()).installUpdate();
      set({ status: 'ready' });
    } catch (e) {
      const err = toBackendError(e);
      // The backend saw a job the store had not heard of yet: wait, never loop.
      if (err.code === 'job-running') return waitForJob();
      set({ status: 'error', error: err });
    }
  },
  async restart() {
    if (get().status !== 'ready' || jobBusy()) return;
    try {
      await (await getBackend()).restartApp();
    } catch (e) {
      set({ error: toBackendError(e) });
    }
  },
  dismissBanner: () => set({ bannerDismissed: true }),
}));

/** Tests only. */
export function resetUpdateStore(): void {
  startupDone = false;
  waitOff?.();
  waitOff = null;
  useUpdateStore.setState({ ...initial });
}
