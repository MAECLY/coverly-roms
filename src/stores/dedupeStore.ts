/**
 * The duplicates screen's state (duplicates spec §11.3): the plan on
 * screen, the options it was made with, the games you asked to leave
 * alone, and the apply that follows. The backend keeps every plan under
 * its id: picks, skips and option changes go back as the id plus the
 * change, and each answer replaces the plan on screen. A `seq` counter
 * drops answers that a newer request superseded.
 */
import { create } from 'zustand';

import { getBackend, type Backend } from '@/ipc/backend';
import { toBackendError, type BackendError } from '@/ipc/errors';
import {
  DEDUPE_PHASES,
  type DedupeActionSetting,
  type DedupeJobView,
  type DedupePhaseView,
  type DedupePlanView,
  type DedupeProgressEvent,
  type DedupeRequest,
  type DedupeViewOptions,
  type OrganizeUndoView,
  type QuarantineChoiceView,
} from '@/ipc/types';

export type DedupeStatus =
  'idle' | 'planning' | 'ready' | 'applying' | 'done' | 'error';

/** An apply above any of these looks unlike a tidy-up and asks first. */
export const BULK_FILES = 500;
export const BULK_SHARE = 0.25;

/** What the apply button asks before it runs, if anything. */
export type ApplyConfirm =
  | { kind: 'trash' }
  /** `share` of the copies are in `system`, the system with most. */
  | { kind: 'bulk'; system: string; share: number };

/**
 * Whether applying `plan` asks first (spec §11.3 "Apply"): the Trash
 * always; the quarantine above 500 files, above a quarter of the
 * library's ROM bytes (`libraryBytes`, when known), or on another volume
 * (moves become copies).
 */
export function applyConfirm(
  plan: DedupePlanView,
  libraryBytes: number | null,
): ApplyConfirm | null {
  if (plan.trash) return { kind: 'trash' };
  const { extras, bytes, by_system } = plan.totals;
  const bulk =
    extras > BULK_FILES ||
    (libraryBytes !== null &&
      libraryBytes > 0 &&
      bytes > libraryBytes * BULK_SHARE) ||
    !plan.same_volume;
  if (!bulk) return null;
  let system = '';
  let share = 0;
  for (const [id, [, n]] of Object.entries(by_system))
    if (n > share || (n === share && id < system)) {
      system = id;
      share = n;
    }
  return { kind: 'bulk', system, share };
}

/** What the screen was opened with. */
export interface DedupeLaunch {
  /** Bytes of the library's ROMs (the 25 % rule); null when unknown. */
  libraryBytes?: number | null;
  /** Systems the library holds (the empty state names how many). */
  systems?: number;
}

interface DedupeState {
  status: DedupeStatus;
  root: string | null;
  plan: DedupePlanView | null;
  options: DedupeViewOptions;
  action: DedupeActionSetting;
  /** A folder picked with "Cambiar…"; null = `_Duplicados/` in the library. */
  folder: string | null;
  /** Groups under "No tocar este juego" (one by one or by system). */
  skipped: ReadonlySet<number>;
  /** Bumped when the plan changes under the groups (regroup, a system
   * skipped or reset): the groups remount with the new keepers. */
  epoch: number;
  /** Planning phases finished, in order. */
  phasesDone: DedupePhaseView[];
  progress: DedupeProgressEvent | null;
  /** An option change, pick or skip waits for the backend. */
  busy: boolean;
  job: DedupeJobView | null;
  undone: OrganizeUndoView | null;
  error: BackendError | null;
  exported: string | null;
  launch: DedupeLaunch;
  /** Extra copies the last plan of each library found (the toolbar's
   * "Duplicados {count}"); kept when the screen closes, dropped once an
   * apply or an undo changed that library. */
  counts: Readonly<Record<string, number>>;
  start(root: string, launch?: DedupeLaunch): Promise<void>;
  /** Looks again (⌘R), with the options on screen. */
  replan(): Promise<void>;
  setOptions(next: DedupeViewOptions): Promise<void>;
  setAction(action: DedupeActionSetting): Promise<void>;
  setFolder(path: string): Promise<void>;
  setKeeper(group: number, member: number): Promise<void>;
  skipGroup(group: number, skip: boolean): Promise<void>;
  skipSystem(system: string, skip: boolean): Promise<void>;
  /** Forgets your picks (in `system`, or everywhere) and looks again. */
  resetRules(system?: string): Promise<void>;
  apply(): Promise<void>;
  resume(): Promise<void>;
  /** Resumes interrupted job `job` of library `root` (the jobs panel). */
  resumeJob(job: string, root: string): Promise<void>;
  /** Stops planning, or the apply between groups. */
  cancel(): Promise<void>;
  undo(includeLater?: boolean): Promise<void>;
  exportPlan(): Promise<void>;
  reveal(path: string): Promise<void>;
  reset(): void;
}

const DEFAULT_OPTIONS: DedupeViewOptions = {
  levels: ['exact', 'format', 'version'],
  one_game_one_rom: false,
};

const initial = {
  status: 'idle' as DedupeStatus,
  root: null,
  plan: null,
  options: DEFAULT_OPTIONS,
  action: 'quarantine' as DedupeActionSetting,
  folder: null,
  skipped: new Set<number>() as ReadonlySet<number>,
  epoch: 0,
  phasesDone: [] as DedupePhaseView[],
  progress: null,
  busy: false,
  job: null,
  undone: null,
  error: null,
  exported: null,
  launch: {} as DedupeLaunch,
};

/** Bumped by every request that replaces the plan: older answers drop. */
let seq = 0;
/** Plan id of the dry run in flight, once its progress named it. */
let planning: number | null = null;
/** Plan id the running apply reports progress under. */
let applyPlan: number | null = null;
let following: Promise<() => void> | null = null;

function unfollow() {
  const f = following;
  following = null;
  void f?.then(
    (off) => off(),
    () => undefined,
  );
}

/** Groups the plan leaves alone because you asked. */
function skippedOf(plan: DedupePlanView): Set<number> {
  return new Set(
    plan.groups
      .filter(
        (g) =>
          g.action.kind === 'report-only' &&
          g.action.why.kind === 'user-skipped',
      )
      .map((g) => g.id),
  );
}

export const useDedupeStore = create<DedupeState>()((set, get) => {
  function quarantine(): QuarantineChoiceView {
    const { action, folder } = get();
    if (action === 'trash') return { kind: 'trash' };
    return folder ? { kind: 'folder', path: folder } : { kind: 'next-to-roms' };
  }

  function request(): DedupeRequest {
    const { options } = get();
    return {
      levels: options.levels,
      one_game_one_rom: options.one_game_one_rom,
      quarantine: quarantine(),
    };
  }

  /** Forgets what `root` held: the toolbar asks to look again. */
  function forgetCount(root: string | null) {
    if (!root) return;
    set((s) => {
      const { [root]: _gone, ...rest } = s.counts;
      return { counts: rest };
    });
  }

  function shown(plan: DedupePlanView, remount: boolean) {
    set((s) => ({
      counts: s.root ? { ...s.counts, [s.root]: plan.totals.extras } : s.counts,
      plan,
      skipped: skippedOf(plan),
      epoch: remount ? s.epoch + 1 : s.epoch,
      busy: false,
      exported: null,
    }));
  }

  async function dryRun(extra: DedupeRequest = {}): Promise<void> {
    const root = get().root;
    if (!root) return;
    const previous = planning;
    const mine = (seq += 1);
    planning = null;
    set({
      status: 'planning',
      progress: null,
      phasesDone: [],
      error: null,
      busy: false,
      job: null,
      undone: null,
    });
    let off: (() => void) | null = null;
    try {
      const b = await getBackend();
      if (previous !== null) void b.dedupeCancel(previous).catch(() => {});
      off = await b.onDedupeProgress((p) => {
        if (mine !== seq || p.phase === 'apply') return;
        if (planning !== null && p.plan_id < planning) return;
        planning = p.plan_id;
        const at = DEDUPE_PHASES.indexOf(p.phase);
        const done = DEDUPE_PHASES.slice(0, p.done >= p.total ? at + 1 : at);
        set({ progress: p, phasesDone: done });
      });
      const plan = await b.dedupePlan(root, { ...request(), ...extra });
      if (mine !== seq) return;
      planning = null;
      set({ status: 'ready', progress: null, phasesDone: [...DEDUPE_PHASES] });
      shown(plan, true);
    } catch (e) {
      if (mine !== seq) return;
      planning = null;
      set({ status: 'error', error: toBackendError(e), progress: null });
    } finally {
      off?.();
    }
  }

  /** One change to the plan on screen; the answer replaces it. */
  async function edit(
    f: (b: Backend, planId: number) => Promise<DedupePlanView>,
    remount: boolean,
  ): Promise<void> {
    const { plan, status } = get();
    if (!plan || status !== 'ready') return;
    const mine = (seq += 1);
    set({ busy: true, error: null });
    try {
      const next = await f(await getBackend(), plan.plan_id);
      if (mine !== seq) return;
      shown(next, remount);
    } catch (e) {
      if (mine !== seq) return;
      set({ busy: false, error: toBackendError(e) });
    }
  }

  function accept(view: DedupeJobView) {
    if (applyPlan !== null && view.plan_id !== applyPlan) return;
    applyPlan = view.plan_id;
    const status: DedupeStatus =
      view.state === 'applying'
        ? 'applying'
        : view.state === 'failed'
          ? 'error'
          : 'done';
    set({
      job: view,
      status,
      progress: status === 'applying' ? get().progress : null,
      error: view.error ? toBackendError(view.error) : null,
    });
    if (status !== 'applying') {
      unfollow();
      if (view.report?.job) forgetCount(view.root);
    }
  }

  function follow(b: Backend): Promise<() => void> {
    if (!following) {
      const sub = Promise.all([
        b.onDedupeChanged(accept),
        b.onDedupeProgress((p) => {
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
    f: (b: Backend) => Promise<DedupeJobView>,
    plan: number | null,
  ): Promise<void> {
    const before = get().status;
    applyPlan = plan;
    set({ status: 'applying', progress: null, error: null, undone: null });
    try {
      const b = await getBackend();
      await follow(b);
      const view = await f(b);
      if (applyPlan === null) applyPlan = view.plan_id;
      // The end may have arrived before the answer.
      if (get().status === 'applying') set({ job: view });
    } catch (e) {
      unfollow();
      set({ status: before, error: toBackendError(e), progress: null });
    }
  }

  return {
    ...initial,
    counts: {},
    async start(root, launch = {}) {
      get().reset();
      set({ root, launch });
      try {
        const d = (await (await getBackend()).getSettings()).settings.dedupe;
        set({
          options: {
            levels: [...d.levels],
            one_game_one_rom: d.one_game_one_rom,
          },
          action: d.action,
          folder: d.quarantine_dir,
        });
      } catch {
        // The defaults stand; the plan itself still reads the settings.
      }
      await dryRun();
    },
    replan: () => dryRun(),
    async setOptions(next) {
      set({ options: next });
      await edit(
        (b, id) =>
          b.dedupeRegroup(id, {
            levels: next.levels,
            one_game_one_rom: next.one_game_one_rom,
          }),
        true,
      );
    },
    async setAction(action) {
      set({ action });
      await edit(
        (b, id) => b.dedupeRegroup(id, { quarantine: quarantine() }),
        true,
      );
    },
    async setFolder(path) {
      set({ folder: path, action: 'quarantine' });
      await edit(
        (b, id) => b.dedupeRegroup(id, { quarantine: quarantine() }),
        true,
      );
    },
    setKeeper: (group, member) =>
      edit((b, id) => b.dedupeSetKeeper(id, group, member), false),
    skipGroup: (group, skip) =>
      edit((b, id) => b.dedupeSkipGroup(id, group, skip), false),
    skipSystem: (system, skip) =>
      edit((b, id) => b.dedupeSkipSystem(id, system, skip), true),
    async resetRules(system) {
      const root = get().root;
      if (!root || get().status !== 'ready') return;
      try {
        const b = await getBackend();
        if (system) await b.dedupeResetChoices(root, [system]);
        await dryRun(system ? {} : { reset_choices: true });
      } catch (e) {
        set({ error: toBackendError(e) });
      }
    },
    async apply() {
      const { plan, status, busy } = get();
      if (!plan || status !== 'ready' || busy) return;
      await run((b) => b.dedupeApply(plan.plan_id), plan.plan_id);
    },
    async resume() {
      const job = get().job?.job;
      if (!job) return;
      await run((b) => b.dedupeResume(job), null);
    },
    async resumeJob(job, root) {
      get().reset();
      set({ root });
      await run((b) => b.dedupeResume(job), null);
    },
    async cancel() {
      const { status, plan } = get();
      if (status === 'planning') {
        const id = planning;
        seq += 1;
        planning = null;
        set({ status: plan ? 'ready' : 'idle', progress: null });
        if (id !== null)
          await (await getBackend()).dedupeCancel(id).catch(() => undefined);
      } else if (status === 'applying') {
        try {
          await (await getBackend()).dedupeCancelApply();
        } catch (e) {
          set({ error: toBackendError(e) });
        }
      }
    },
    async undo(includeLater = false) {
      const job = get().job?.job;
      if (!job) return;
      set({ error: null });
      try {
        const b = await getBackend();
        set({ undone: await b.organizeUndo(job, includeLater) });
        forgetCount(get().root);
      } catch (e) {
        set({ error: toBackendError(e) });
      }
    },
    async exportPlan() {
      const plan = get().plan;
      if (!plan) return;
      try {
        const name = await (await getBackend()).dedupeExportPlan(plan.plan_id);
        set({ exported: name });
      } catch (e) {
        set({ error: toBackendError(e) });
      }
    },
    async reveal(path) {
      try {
        await (await getBackend()).revealPath(path);
      } catch (e) {
        set({ error: toBackendError(e) });
      }
    },
    reset() {
      seq += 1;
      // A plan still being made for this screen is no longer wanted.
      const inFlight = planning;
      planning = null;
      if (inFlight !== null)
        void getBackend()
          .then((b) => b.dedupeCancel(inFlight))
          .catch(() => undefined);
      applyPlan = null;
      unfollow();
      set({ ...initial, skipped: new Set(), counts: get().counts });
    },
  };
});
