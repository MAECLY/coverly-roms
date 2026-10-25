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
  OrganizePlanView,
  OrganizeProgressEvent,
  OrganizeRequest,
  OrganizeUndoView,
} from '@/ipc/types';
import { itemKey } from '@/lib/organize';

import { useLibraryStore } from './libraryStore';
import { useNavStore, type OrganizeLaunch } from './navStore';

/** What the last "scan again" found, by file (see `itemKey`). */
export interface OrganizeRefresh {
  /** Keys of the files that were not in the previous plan. */
  added: string[];
  /** Files of the previous plan that are no longer in the folder. */
  gone: number;
  /** Decisions carried over to the new plan. */
  kept: number;
  /** Decisions dropped because their file is gone. */
  dropped: number;
}

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
  /** The change summary of the last refresh; cleared by any other edit. */
  lastRefresh: OrganizeRefresh | null;
  /** The plan in flight is a refresh (the button shows it is busy). */
  refreshing: boolean;
  start(source: string, launch?: OrganizeLaunch): Promise<void>;
  setOption(patch: Partial<OrganizeRequest>): Promise<void>;
  decide(
    itemIds: number[],
    choice: 'leave' | { system: string },
  ): Promise<void>;
  /** Reads the folder again (files added or removed), keeping decisions
   * by file. No-op while a plan runs or once an apply started. */
  refresh(): Promise<void>;
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
  lastRefresh: null,
  refreshing: false,
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
  /** Supersedes the dry run in flight; the answer counts while `mine === seq`. */
  function startPlanning(refreshing = false): {
    mine: number;
    previous: number | null;
  } {
    const previous = planning;
    const mine = (seq += 1);
    planning = null;
    set({ status: 'planning', progress: null, error: null, refreshing });
    return { mine, previous };
  }

  async function backendFor(previous: number | null): Promise<Backend> {
    const b = await getBackend();
    if (seenOn !== b) {
      seenOn = b;
      maxSeenPlan = 0;
    }
    // A superseded dry run is no longer wanted.
    if (previous !== null)
      void b.organizeCancel(previous).catch(() => undefined);
    return b;
  }

  /** One `organize_plan` call, following its progress while it is ours. */
  async function dryRun(
    b: Backend,
    request: OrganizeRequest,
    decisions: OrganizeDecision[],
    mine: number,
  ): Promise<OrganizePlanView> {
    // Every dry run started before this one has an id <= floor.
    const floor = maxSeenPlan;
    const off = await b.onOrganizeProgress((p) => {
      // Apply-time phases belong to a job, never to a dry run.
      if (p.phase === 'apply' || p.phase === 'extract') return;
      maxSeenPlan = Math.max(maxSeenPlan, p.plan_id);
      if (mine !== seq || p.plan_id <= floor || p.plan_id < maxSeenPlan) return;
      planning = p.plan_id;
      set({ progress: p });
    });
    try {
      return await b.organizePlan(request, decisions);
    } finally {
      off();
    }
  }

  function shown(
    view: OrganizePlanView,
    request: OrganizeRequest,
    decisions: Record<number, OrganizeDecision>,
    lastRefresh: OrganizeRefresh | null = null,
  ) {
    planning = null;
    shownWith = { request, decisions };
    set({
      status: 'ready',
      planId: view.plan_id,
      plan: view.plan,
      decisions,
      progress: null,
      lastRefresh,
      refreshing: false,
    });
  }

  function failed(e: unknown, mine: number) {
    if (mine !== seq) return;
    planning = null;
    set({
      status: 'error',
      error: toBackendError(e),
      progress: null,
      refreshing: false,
    });
  }

  async function plan(): Promise<void> {
    const { request, decisions } = get();
    if (!request) return;
    const { mine, previous } = startPlanning();
    try {
      const b = await backendFor(previous);
      const view = await dryRun(b, request, Object.values(decisions), mine);
      if (mine !== seq) return;
      shown(view, request, decisions);
    } catch (e) {
      failed(e, mine);
    }
  }

  /** Scan again: decisions travel by file, since ids are positions. */
  async function refresh(): Promise<void> {
    const { request, plan: before, decisions, status } = get();
    if (!request || !editable() || status === 'planning') return;
    const byId = new Map(before?.items.map((i) => [i.id, i]) ?? []);
    const carried = new Map<string, OrganizeDecision>();
    for (const d of Object.values(decisions)) {
      const item = byId.get(d.item);
      if (item) carried.set(itemKey(item), d);
    }
    set({ lastRefresh: null });
    const { mine, previous } = startPlanning(true);
    try {
      const b = await backendFor(previous);
      // `rescan` goes with this call only: the kept request never has it.
      let view = await dryRun(b, { ...request, rescan: true }, [], mine);
      if (mine !== seq) return;
      const remapped: Record<number, OrganizeDecision> = {};
      for (const item of view.plan.items) {
        const d = carried.get(itemKey(item));
        if (d) remapped[item.id] = { ...d, item: item.id };
      }
      const kept = Object.keys(remapped).length;
      // Reuses the scan just made: no file is read again.
      if (kept > 0) {
        view = await dryRun(b, request, Object.values(remapped), mine);
        if (mine !== seq) return;
      }
      const oldKeys = new Set(before?.items.map(itemKey) ?? []);
      const newKeys = new Set(view.plan.items.map(itemKey));
      shown(view, request, remapped, {
        added: before ? [...newKeys].filter((k) => !oldKeys.has(k)) : [],
        gone: [...oldKeys].filter((k) => !newKeys.has(k)).length,
        kept,
        dropped: carried.size - kept,
      });
    } catch (e) {
      failed(e, mine);
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
      set({ request: { ...request, ...next }, lastRefresh: null });
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
      set({ decisions, lastRefresh: null });
      await plan();
    },
    refresh,
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
            refreshing: false,
          });
        } else {
          set({ status: 'idle', progress: null, refreshing: false });
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
