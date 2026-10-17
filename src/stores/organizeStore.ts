/**
 * The organise proposal and its apply, as the organise screen follows them.
 * The backend keeps every dry run; this store keeps the request, the user's
 * decisions and the latest plan, and sends back only `plan_id` + decisions.
 * A `seq` counter drops plan answers that a newer request superseded.
 */
import { create } from 'zustand';

import { getBackend, type Backend } from '@/ipc/backend';
import { toBackendError, type BackendError } from '@/ipc/errors';
import type {
  OrganizeDecision,
  OrganizeJobView,
  OrganizePlan,
  OrganizeProgressEvent,
  OrganizeRequest,
  OrganizeUndoView,
} from '@/ipc/types';

import { useLibraryStore } from './libraryStore';
import { useNavStore, type OrganizeLaunch } from './navStore';

export type OrganizeStatus =
  'idle' | 'planning' | 'ready' | 'applying' | 'done' | 'paused' | 'error';

interface OrganizeState {
  status: OrganizeStatus;
  source: string | null;
  request: OrganizeRequest | null;
  planId: number | null;
  plan: OrganizePlan | null;
  /** The user's answers by item id; every re-plan sends all of them. */
  decisions: Record<number, OrganizeDecision>;
  progress: OrganizeProgressEvent | null;
  job: OrganizeJobView | null;
  undo: OrganizeUndoView | null;
  error: BackendError | null;
  /** Organising the loose files of a library: its layout and target stay. */
  locked: boolean;
  start(source: string, launch?: OrganizeLaunch): Promise<void>;
  setOption(patch: Partial<OrganizeRequest>): Promise<void>;
  decide(
    itemIds: number[],
    choice: 'leave' | { system: string },
  ): Promise<void>;
  apply(): Promise<void>;
  cancel(): Promise<void>;
  resume(): Promise<void>;
  /** Resumes an interrupted job of the history (no plan on screen);
   * `source` is the folder the screen shows for it (the journal root). */
  resumeJob(job: string, source: string): Promise<void>;
  undoJob(includeLater?: boolean): Promise<void>;
  openResult(): Promise<void>;
  reset(): void;
}

const initial = {
  status: 'idle' as OrganizeStatus,
  source: null,
  request: null,
  planId: null,
  plan: null,
  decisions: {},
  progress: null,
  job: null,
  undo: null,
  error: null,
  locked: false,
};

/** Bumped by every plan request (and by cancel/reset): older answers are dropped. */
let seq = 0;
/** Highest plan id seen in progress events of `seenOn` (ids restart with a backend). */
let maxSeenPlan = 0;
let seenOn: Backend | null = null;
/** Plan id of the dry run in flight, once its progress named it. */
let planning: number | null = null;
/** Plan id the running apply reports progress under (null: not known yet). */
let applyPlan: number | null = null;
/** Bumped by every `organize-changed` event: an older command result is dropped. */
let jobEvents = 0;
/** The request and decisions the shown plan was made with (restored by cancel). */
let shownWith: {
  request: OrganizeRequest;
  decisions: Record<number, OrganizeDecision>;
} | null = null;
/** A job of the history being resumed: its events are ours. */
let ownJob: string | null = null;
/** Apply/resume event subscription. */
let following: Promise<() => void> | null = null;

function unfollow() {
  const f = following;
  following = null;
  void f?.then(
    (off) => off(),
    () => undefined,
  );
}

export const useOrganizeStore = create<OrganizeState>()((set, get) => {
  async function plan(): Promise<void> {
    const { request, decisions } = get();
    if (!request) return;
    const previous = planning;
    const mine = (seq += 1);
    planning = null;
    set({ status: 'planning', progress: null, error: null });
    let off: (() => void) | undefined;
    try {
      const b = await getBackend();
      if (seenOn !== b) {
        seenOn = b;
        maxSeenPlan = 0;
      }
      // Every dry run started before this one has an id <= floor.
      const floor = maxSeenPlan;
      // A superseded dry run is no longer wanted.
      if (previous !== null)
        void b.organizeCancel(previous).catch(() => undefined);
      off = await b.onOrganizeProgress((p) => {
        if (p.phase === 'apply') return;
        maxSeenPlan = Math.max(maxSeenPlan, p.plan_id);
        if (mine !== seq || p.plan_id <= floor || p.plan_id < maxSeenPlan)
          return;
        planning = p.plan_id;
        set({ progress: p });
      });
      const view = await b.organizePlan(request, Object.values(decisions));
      if (mine !== seq) return;
      planning = null;
      shownWith = { request, decisions };
      set({
        status: 'ready',
        planId: view.plan_id,
        plan: view.plan,
        progress: null,
      });
    } catch (e) {
      if (mine !== seq) return;
      planning = null;
      set({ status: 'error', error: toBackendError(e), progress: null });
    } finally {
      off?.();
    }
  }

  function accept(view: OrganizeJobView) {
    const cur = get().job;
    const ours =
      (applyPlan !== null && view.plan_id === applyPlan) ||
      (cur?.job != null && view.job === cur.job) ||
      (ownJob !== null && view.job === ownJob);
    if (!ours) return;
    applyPlan = view.plan_id;
    const status: OrganizeStatus =
      view.state === 'applying'
        ? 'applying'
        : view.state === 'paused'
          ? 'paused'
          : view.state === 'failed'
            ? 'error'
            : 'done';
    set({
      job: view,
      status,
      progress: status === 'applying' ? get().progress : null,
      error:
        view.state === 'failed' && view.error
          ? toBackendError(view.error)
          : null,
    });
    if (status !== 'applying') unfollow();
  }

  function follow(b: Backend): Promise<() => void> {
    if (!following) {
      const sub = Promise.all([
        b.onOrganizeChanged((view) => {
          jobEvents += 1;
          accept(view);
        }),
        b.onOrganizeProgress((p) => {
          if (p.phase !== 'apply' || get().status !== 'applying') return;
          if (applyPlan !== null && p.plan_id !== applyPlan) return;
          set({ progress: p });
        }),
      ]).then((offs) => () => offs.forEach((off) => off()));
      sub.catch(() => {
        if (following === sub) following = null;
      });
      following = sub;
    }
    return following;
  }

  async function run(
    f: (b: Backend) => Promise<OrganizeJobView>,
    plan: number | null,
  ): Promise<void> {
    const before = get().status;
    applyPlan = plan;
    set({ status: 'applying', progress: null, error: null, undo: null });
    const seen = jobEvents;
    try {
      const b = await getBackend();
      await follow(b);
      const view = await f(b);
      if (applyPlan === null) applyPlan = view.plan_id;
      if (jobEvents === seen) accept(view);
    } catch (e) {
      unfollow();
      // The plan (or the paused job) is still there to retry from.
      set({ status: before, error: toBackendError(e), progress: null });
    }
  }

  /** Options and decisions change only before an apply started (and not
   * while one runs): afterwards the job, not a new plan, is on screen. */
  function editable(): boolean {
    const { request, status, job } = get();
    return (
      request !== null &&
      job === null &&
      (status === 'ready' || status === 'planning' || status === 'error')
    );
  }

  return {
    ...initial,
    async start(source, launch) {
      unfollow();
      planning = null;
      applyPlan = null;
      ownJob = null;
      shownWith = null;
      const request: OrganizeRequest = { source };
      if (launch?.layout != null) request.layout = launch.layout;
      if (launch?.mode != null) request.mode = launch.mode;
      if (launch?.flat != null) request.flat = launch.flat;
      set({
        ...initial,
        status: 'planning',
        source,
        request,
        locked: launch?.locked === true,
      });
      await plan();
    },
    async setOption(patch) {
      const { request, locked } = get();
      if (!request || !editable()) return;
      const next = { ...patch };
      if (locked) {
        delete next.layout;
        delete next.target;
        delete next.flat;
      }
      if (Object.keys(next).length === 0) return;
      set({ request: { ...request, ...next } });
      await plan();
    },
    async decide(itemIds, choice) {
      if (!editable()) return;
      const decisions = { ...get().decisions };
      for (const item of itemIds) {
        decisions[item] =
          choice === 'leave'
            ? { item, choice: 'leave' }
            : { item, choice: 'assign', system: choice.system };
      }
      set({ decisions });
      await plan();
    },
    async apply() {
      const { planId, decisions, status } = get();
      // Only the plan on screen, made with the options on screen.
      if (planId === null || status !== 'ready') return;
      await run(
        (b) => b.organizeApply(planId, Object.values(decisions)),
        planId,
      );
    },
    async cancel() {
      const { status } = get();
      if (status === 'planning') {
        const id = planning;
        seq += 1;
        planning = null;
        // Back to the shown plan with the options it was made with, so an
        // apply cannot run it under options the user no longer sees.
        if (get().plan && shownWith) {
          set({
            status: 'ready',
            progress: null,
            request: shownWith.request,
            decisions: shownWith.decisions,
          });
        } else {
          set({ status: 'idle', progress: null });
        }
        if (id === null) return;
        try {
          await (await getBackend()).organizeCancel(id);
        } catch (e) {
          set({ error: toBackendError(e) });
        }
      } else if (status === 'applying') {
        try {
          await (await getBackend()).organizeCancelApply();
        } catch (e) {
          set({ error: toBackendError(e) });
        }
      }
    },
    async resume() {
      const job = get().job?.job;
      if (!job) return;
      await run((b) => b.organizeResume(job), null);
    },
    async resumeJob(job, source) {
      get().reset();
      ownJob = job;
      set({ source });
      await run((b) => b.organizeResume(job), null);
    },
    async undoJob(includeLater = false) {
      const job = get().job?.job;
      if (!job) return;
      set({ error: null });
      try {
        const b = await getBackend();
        set({ undo: await b.organizeUndo(job, includeLater) });
      } catch (e) {
        // `later-jobs` keeps the list of newer jobs in `detail`.
        set({ error: toBackendError(e) });
      }
    },
    async openResult() {
      const { job, plan } = get();
      const target = job?.target_root ?? plan?.target_root;
      const layout = job?.layout ?? plan?.layout;
      if (!target) return;
      await useLibraryStore.getState().open(target, layout);
      useNavStore.getState().closeOrganize();
      get().reset();
    },
    reset() {
      seq += 1;
      planning = null;
      applyPlan = null;
      ownJob = null;
      shownWith = null;
      unfollow();
      set(initial);
    },
  };
});
