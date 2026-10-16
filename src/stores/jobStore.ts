/**
 * The fill job as the UI follows it. The backend owns the job (one at a
 * time); this store mirrors it from events and command results, and keeps
 * the job history and the last undo.
 */
import { create } from 'zustand';

import { getBackend, type Backend } from '@/ipc/backend';
import { BackendError, toBackendError } from '@/ipc/errors';
import type {
  DecisionIn,
  ExportFormat,
  FillRequest,
  JobHistoryItem,
  JobProgressEvent,
  JobView,
  UndoView,
} from '@/ipc/types';

import { useLibraryStore } from './libraryStore';

export function isRunning(job: JobView | null): boolean {
  return job?.state === 'gathering' || job?.state === 'applying';
}

interface JobStore {
  job: JobView | null;
  progress: JobProgressEvent | null;
  error: BackendError | null;
  history: JobHistoryItem[];
  lastUndo: UndoView | null;
  /** A command is in flight. */
  pending: boolean;
  /** Subscribes to job events (shared, ref-counted); returns the release. */
  connect(): Promise<() => void>;
  start(generation: number, request: FillRequest): Promise<boolean>;
  decide(decisions: DecisionIn[]): Promise<boolean>;
  run(): Promise<boolean>;
  resume(): Promise<boolean>;
  cancel(): Promise<boolean>;
  undoLast(generation: number): Promise<boolean>;
  loadHistory(): Promise<void>;
  exportReport(format: ExportFormat): Promise<string | null>;
  clearError(): void;
  reset(): void;
}

const initial = {
  job: null,
  progress: null,
  error: null,
  history: [],
  lastUndo: null,
  pending: false,
};

interface Shared {
  backend: Backend;
  refs: number;
  ready: Promise<() => void>;
}
let shared: Shared | null = null;

/** Bumped by every `changed` event: a command result older than an event is dropped. */
let events = 0;

export const useJobStore = create<JobStore>()((set, get) => {
  function accept(view: JobView) {
    const cur = get().job;
    if (cur && view.id < cur.id) return;
    set({ job: view, progress: isRunning(view) ? get().progress : null });
  }
  function onChanged(view: JobView) {
    events += 1;
    accept(view);
  }
  function jobId(job: JobView | null): number {
    if (!job) throw new BackendError('no-job');
    return job.id;
  }
  async function act(
    f: (b: Backend, job: JobView | null) => Promise<JobView>,
  ): Promise<boolean> {
    const seen = events;
    set({ pending: true, error: null });
    try {
      const b = await getBackend();
      const view = await f(b, get().job);
      if (events === seen || get().job?.id !== view.id) accept(view);
      return true;
    } catch (e) {
      set({ error: toBackendError(e) });
      return false;
    } finally {
      set({ pending: false });
    }
  }
  return {
    ...initial,
    async connect() {
      const b = await getBackend();
      // One shared subscription however many holders (App and the library
      // screen) connect: events are handled once and survive either leaving.
      if (!shared || shared.backend !== b) {
        const sub: Shared = {
          backend: b,
          refs: 0,
          ready: Promise.all([
            b.onJobProgress((p) => {
              const cur = get().job;
              if (!cur || p.job === cur.id) set({ progress: p });
            }),
            b.onJobChanged(onChanged),
            b.onEntriesUpdated((u) =>
              useLibraryStore
                .getState()
                .applyArtUpdates(u.generation, u.entries),
            ),
          ]).then((offs) => () => offs.forEach((off) => off())),
        };
        sub.ready.catch(() => {
          if (shared === sub) shared = null;
        });
        shared = sub;
      }
      const sub = shared;
      sub.refs += 1;
      let released = false;
      const release = () => {
        if (released) return;
        released = true;
        sub.refs -= 1;
        if (sub.refs > 0) return;
        if (shared === sub) shared = null;
        void sub.ready.then(
          (off) => off(),
          () => undefined,
        );
      };
      try {
        await sub.ready;
        const job = await b.fillJob();
        if (job) accept(job);
      } catch (e) {
        release();
        throw e;
      }
      return release;
    },
    start(generation, request) {
      set({ progress: null, lastUndo: null });
      return act((b) => b.startFill(generation, request));
    },
    decide: (decisions) => act((b, job) => b.decideFill(jobId(job), decisions)),
    run: () => act((b, job) => b.runFill(jobId(job))),
    resume: () => act((b, job) => b.resumeFill(jobId(job))),
    cancel: () => act((b, job) => b.cancelFill(jobId(job))),
    async undoLast(generation) {
      set({ pending: true, error: null });
      try {
        const b = await getBackend();
        const undo = await b.undoLastJob(generation);
        useLibraryStore.getState().applyArtUpdates(generation, undo.entries);
        set({ lastUndo: undo, history: await b.jobHistory() });
        return true;
      } catch (e) {
        set({ error: toBackendError(e) });
        return false;
      } finally {
        set({ pending: false });
      }
    },
    async loadHistory() {
      try {
        const b = await getBackend();
        set({ history: await b.jobHistory() });
      } catch (e) {
        set({ error: toBackendError(e) });
      }
    },
    async exportReport(format) {
      const job = get().job;
      if (!job) return null;
      try {
        const b = await getBackend();
        return await b.exportJobReport(job.id, format);
      } catch (e) {
        set({ error: toBackendError(e) });
        return null;
      }
    },
    clearError: () => set({ error: null }),
    reset: () => set(initial),
  };
});
