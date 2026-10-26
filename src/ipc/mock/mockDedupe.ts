/**
 * The duplicates backend of the mock (`pnpm dev`, tests): every library
 * holds the deterministic `dedupeFixture` plan under its own root. Plans
 * are kept by id like Rust (four at most), picks are remembered by path
 * per library, and apply moves the extras out of the library "on disk":
 * the next plan no longer finds them, and undo (through `organizeUndo`)
 * brings them back, from the quarantine or from the Trash alike.
 */
import type { Backend } from '../backend';
import { BackendError, type ErrorCode } from '../errors';
import {
  DEDUPE_PHASES,
  RELATIONS,
  type DedupeItemStatusView,
  type DedupeJobView,
  type DedupeOutcomeView,
  type DedupePlanView,
  type DedupeProgressEvent,
  type DedupeReportView,
  type DedupeRequest,
  type DedupeSettings,
  type DedupeSkipView,
  type DupGroupView,
  type JobHistoryItem,
  type KeepReasonView,
  type MemberView,
  type OrganizeUndoReport,
  type OrganizeUndoView,
  type QuarantineChoiceView,
} from '../types';
import { dedupeFixture, dedupeTotals } from './dedupeFixture';
import type { MockExport } from './mockJobs';

export const MOCK_DEDUPE_EXPORT = 'coverly-duplicates-plan.json';
/** Plans kept for picks, regrouping, export and apply (Rust `KEPT_PLANS`). */
const KEPT_PLANS = 4;
/** Files the mock "lists" besides the duplicates, for the progress bars. */
const OTHER_FILES = 542;
const DEFAULT_QUARANTINE = '_Duplicados';

/** What the mock does next; `window.mockDedupeScenario` changes it. */
export interface MockDedupeScenario {
  /** Plans find no duplicates (the empty state). */
  empty: boolean;
  /** The next plans fail with this code (the error state); null = never. */
  failPlan: ErrorCode | null;
  /** The library's volume has a Trash. */
  trashAvailable: boolean;
  /** The first apply of each job pauses (disk full) after this many
   * extras; null = never. */
  pauseAfter: number | null;
}

export interface MockDedupeHost {
  settings(): DedupeSettings;
  /** A fill or an organise job writes or is paused: applies refuse. */
  otherBusy(): boolean;
  tick(): Promise<void>;
  now: number;
  exports: MockExport[];
}

type DedupeApi = Pick<
  Backend,
  | 'dedupePlan'
  | 'dedupeCancel'
  | 'dedupeRegroup'
  | 'dedupeSetKeeper'
  | 'dedupeSkipGroup'
  | 'dedupeSkipSystem'
  | 'dedupeResetChoices'
  | 'dedupeApply'
  | 'dedupeCancelApply'
  | 'dedupeJob'
  | 'dedupeResume'
  | 'dedupeExportPlan'
  | 'revealPath'
  | 'onDedupeProgress'
  | 'onDedupeChanged'
> & {
  /** An apply or resume runs (fills and organise jobs refuse meanwhile). */
  isDedupeApplying(): boolean;
  /** The duplicates journals, newest first. */
  dedupeHistory(): JobHistoryItem[];
  /** Whether `job` is a duplicates journal (undone through `organizeUndo`). */
  ownsDedupeJob(job: string): boolean;
  /** `organizeUndo` of a duplicates journal. */
  dedupeUndo(job: string, includeLater: boolean): Promise<OrganizeUndoView>;
  /** Paths passed to `revealPath`, for tests. */
  readonly revealedPaths: string[];
  /** Dev and tests only: what the next plans and applies do. */
  dedupeScenario(patch: Partial<MockDedupeScenario>): void;
};

/** The options a plan is made (or regrouped) with. */
interface Opts {
  levels: string[];
  oneGameOneRom: boolean;
  systems: string[] | null;
  quarantine: QuarantineChoiceView;
  forced: string[];
}

interface Kept {
  root: string;
  opts: Opts;
  skippedGroups: Set<number>;
  skippedSystems: Set<string>;
  view: DedupePlanView;
}

/** One extra to set aside: its files relative to the ROMs folder. */
interface Work {
  group: number;
  member: number;
  /** The unit's files, then its companions and art. */
  rels: string[];
  size: number;
  dest: string | null;
}

interface Journal {
  job: string;
  createdAt: string;
  root: string;
  quarantine: string;
  trash: boolean;
  planId: number;
  /** Where each moved extra's files went (`[from, to]`, absolute), and
   * its outcome: a resume reports it again, as Rust's does. */
  moved: {
    rels: string[];
    placed: string[];
    size: number;
    outcome: DedupeOutcomeView;
  }[];
  remaining: Work[];
  pausedOnce: boolean;
  complete: boolean;
  undone: boolean;
}

const join = (a: string, b: string) => `${a.replace(/\/+$/, '')}/${b}`;
const baseName = (p: string) => p.slice(p.lastIndexOf('/') + 1);
const dirOf = (p: string) => p.slice(0, Math.max(0, p.lastIndexOf('/')));
const romsDirOf = (root: string) => join(root, 'ROMS');
const relationRank = (r: string) => RELATIONS.indexOf(r as never);

function iso(secs: number): string {
  return new Date(secs * 1000).toISOString().replace(/\.\d{3}Z$/, 'Z');
}

/** The fixture's ids follow its order: reset them on every read. */
function fixtureGroups(): DupGroupView[] {
  return dedupeFixture().groups;
}

/** The weakest link of a group (its widest relation). */
function groupRelation(g: DupGroupView): string {
  return g.members.reduce(
    (w, m) => (relationRank(m.relation) > relationRank(w) ? m.relation : w),
    'exact',
  );
}

function rebaseReason(r: KeepReasonView, roms: string): KeepReasonView {
  return r.kind === 'has-save' ? { ...r, path: join(roms, r.path) } : r;
}

/** The fixture's relative paths made absolute under the library. */
function absolute(g: DupGroupView, roms: string): DupGroupView {
  const abs = (p: string) => join(roms, p);
  return {
    ...g,
    reason: rebaseReason(g.reason, roms),
    runner_up: g.runner_up
      ? [g.runner_up[0], rebaseReason(g.runner_up[1], roms)]
      : null,
    action:
      g.action.kind === 'conflict'
        ? {
            kind: 'conflict',
            saves: g.action.saves.map((s) => ({ ...s, path: abs(s.path) })),
          }
        : g.action,
    members: g.members.map((m) => ({
      ...m,
      unit: {
        ...m.unit,
        primary: abs(m.unit.primary),
        files: m.unit.files.map(abs),
      },
      travels: m.travels.map((t) => ({ ...t, from: abs(t.from) })),
      stays: m.stays.map((s) => ({ ...s, path: abs(s.path) })),
    })),
  };
}

/** `g` without the members in `away`, ids renumbered like a new scan. */
function withoutAway(g: DupGroupView, away: Set<string>): DupGroupView | null {
  const kept = g.members.filter((m) => !away.has(m.unit.primary));
  if (kept.length < 2) return null;
  if (kept.length === g.members.length) return g;
  const ids = new Map(kept.map((m, i) => [m.id, i]));
  const keeper = ids.get(g.keeper);
  return {
    ...g,
    keeper: keeper ?? 0,
    runner_up:
      g.runner_up && ids.has(g.runner_up[0])
        ? [ids.get(g.runner_up[0])!, g.runner_up[1]]
        : null,
    members: kept.map((m, i) => ({
      ...m,
      id: i,
      stays: m.stays.flatMap((s) =>
        ids.has(s.with) ? [{ ...s, with: ids.get(s.with)! }] : [],
      ),
    })),
  };
}

export function createMockDedupe(
  host: MockDedupeHost,
  initial: Partial<MockDedupeScenario> = {},
): DedupeApi {
  const scenario: MockDedupeScenario = {
    empty: false,
    failPlan: null,
    trashAvailable: true,
    pauseAfter: null,
    ...initial,
  };
  const progress = new Set<(e: DedupeProgressEvent) => void>();
  const changed = new Set<(v: DedupeJobView) => void>();
  const listen = <T>(set: Set<(v: T) => void>, cb: (v: T) => void) => {
    const own = (v: T) => cb(v);
    set.add(own);
    return Promise.resolve(() => {
      set.delete(own);
    });
  };
  /** Oldest first, at most `KEPT_PLANS`. */
  const plans = new Map<number, Kept>();
  const cancelled = new Set<number>();
  /** Library root → group id → primary (relative) of the copy picked. */
  const picks = new Map<string, Map<number, string>>();
  /** Library root → primaries (relative) set aside by a job, not undone. */
  const away = new Map<string, Set<string>>();
  const journals: Journal[] = [];
  const revealedPaths: string[] = [];
  let seq = 0;
  let last: DedupeJobView | null = null;
  let applying: { view: DedupeJobView; cancel: boolean } | null = null;

  const emitProgress = (e: DedupeProgressEvent) => {
    if (applying && applying.view.plan_id === e.plan_id)
      applying.view.progress = e;
    progress.forEach((cb) => cb({ ...e }));
  };
  const emitChanged = (v: DedupeJobView) => {
    last = structuredClone(v);
    changed.forEach((cb) => cb(structuredClone(v)));
  };

  function optsFrom(req: DedupeRequest = {}, base?: Opts): Opts {
    const s = host.settings();
    const quarantine: QuarantineChoiceView =
      req.quarantine ??
      base?.quarantine ??
      (s.action === 'trash'
        ? { kind: 'trash' }
        : s.quarantine_dir
          ? { kind: 'folder', path: s.quarantine_dir }
          : { kind: 'next-to-roms' });
    return {
      levels: req.levels ?? base?.levels ?? [...s.levels],
      oneGameOneRom:
        req.one_game_one_rom ?? base?.oneGameOneRom ?? s.one_game_one_rom,
      systems:
        base?.systems ?? req.systems?.map((x) => x.toLowerCase()) ?? null,
      quarantine,
      forced: base?.forced ?? req.forced_keepers ?? [],
    };
  }

  function quarantineOf(root: string, q: QuarantineChoiceView): string {
    if (q.kind === 'folder') return q.path;
    return join(root, host.settings().quarantine_name ?? DEFAULT_QUARANTINE);
  }

  /** The plan of `root` as "disk", picks and skips stand now. */
  function build(planId: number, k: Omit<Kept, 'view'>): DedupePlanView {
    const { root, opts } = k;
    const roms = romsDirOf(root);
    const trash = opts.quarantine.kind === 'trash';
    const quarantine = quarantineOf(root, opts.quarantine);
    const gone = new Set(
      [...(away.get(root) ?? [])].map((rel) => join(roms, rel)),
    );
    const remembered = picks.get(root) ?? new Map<number, string>();
    const groups: DupGroupView[] = [];
    for (const raw of scenario.empty ? [] : fixtureGroups()) {
      if (opts.systems && !opts.systems.includes(raw.system.toLowerCase()))
        continue;
      if (!opts.levels.includes(groupRelation(raw))) continue;
      const g0 = withoutAway(absolute(raw, roms), gone);
      if (!g0) continue;
      let g: DupGroupView = structuredClone(g0);
      const heldBack =
        g.action.kind === 'report-only' &&
        g.action.why.kind === 'level-two-off';
      if (heldBack && opts.oneGameOneRom) g.action = { kind: 'act' };
      // A pick (this run's, an earlier run's, or forced) beats the rules.
      const chosen =
        g.members.find((m) => opts.forced.includes(m.unit.primary)) ??
        g.members.find(
          (m) => join(roms, remembered.get(g.id) ?? '\0') === m.unit.primary,
        );
      if (chosen && chosen.id !== g.keeper && g.action.kind !== 'conflict') {
        g = {
          ...g,
          runner_up: [g.keeper, g.reason],
          keeper: chosen.id,
          reason: { kind: 'user-choice' },
        };
      }
      if (
        g.action.kind === 'act' &&
        (k.skippedGroups.has(g.id) || k.skippedSystems.has(g.system))
      )
        g.action = { kind: 'report-only', why: { kind: 'user-skipped' } };
      const acts = g.action.kind === 'act';
      g.members = g.members.map((m): MemberView => ({
        ...m,
        dest:
          acts && !trash && m.id !== g.keeper
            ? join(quarantine, m.unit.primary.slice(roms.length + 1))
            : null,
      }));
      groups.push(g);
    }
    const fixture = dedupeFixture();
    return {
      ...fixture,
      plan_id: planId,
      root,
      roms_dir: roms,
      quarantine,
      groups,
      unchanged: groups
        .filter((g) => g.action.kind === 'report-only')
        .map((g) => g.id),
      not_roms: scenario.empty
        ? []
        : fixture.not_roms.map((p) => join(roms, p)),
      // No duplicates found does not bring the missing catalogues.
      no_catalogue: fixture.no_catalogue,
      totals: dedupeTotals(groups),
      trash,
      trash_available: scenario.trashAvailable,
    };
  }

  function kept(planId: number): Kept {
    const k = plans.get(planId);
    if (!k) throw new BackendError('no-dedupe-plan');
    return k;
  }

  function edit(planId: number, change: (k: Kept) => void): DedupePlanView {
    const k = kept(planId);
    change(k);
    k.view = build(planId, k);
    return structuredClone(k.view);
  }

  function groupOf(k: Kept, group: number): DupGroupView {
    const g = k.view.groups.find((x) => x.id === group);
    if (!g)
      throw new BackendError('bad-request', `no duplicates group ${group}`);
    return g;
  }

  function notApplying() {
    if (applying || host.otherBusy()) throw new BackendError('job-running');
  }

  /** The extras of the groups that act. */
  function worksOf(view: DedupePlanView): Work[] {
    const roms = view.roms_dir;
    const rel = (p: string) => p.slice(roms.length + 1);
    return view.groups.flatMap((g) =>
      g.action.kind !== 'act'
        ? []
        : g.members
            .filter((m) => m.id !== g.keeper)
            .map((m) => ({
              group: g.id,
              member: m.id,
              rels: [...m.unit.files, ...m.travels.map((t) => t.from)].map(rel),
              size: m.size,
              dest: m.dest,
            })),
    );
  }

  /** The copies of groups that do not act, as Rust reports them. */
  function leftAlone(view: DedupePlanView): DedupeOutcomeView[] {
    return view.groups.flatMap((g) => {
      if (g.action.kind === 'act') return [];
      const reason: DedupeSkipView =
        g.action.kind === 'conflict'
          ? { kind: 'conflict' }
          : g.action.why.kind === 'user-skipped'
            ? { kind: 'excluded' }
            : { kind: 'report-only' };
      return g.members
        .filter((m) => m.id !== g.keeper)
        .map((m) => ({
          group: g.id,
          member: m.id,
          files: [],
          status: 'skipped' as const,
          reason,
        }));
    });
  }

  function trashPlace(root: string, name: string): string {
    const volume = /^\/Volumes\/[^/]+/.exec(root)?.[0];
    return volume
      ? `${volume}/.Trashes/501/${name}`
      : `/Users/demo/.Trash/${name}`;
  }

  async function run(
    j: Journal,
    planId: number,
    works: Work[],
    left: DedupeOutcomeView[],
  ) {
    const own = applying!;
    const outcomes: DedupeOutcomeView[] = [...left];
    // A resume counts what earlier runs of the job set aside (Rust's
    // `finish_started`): its report covers the whole job.
    let bytes = 0;
    for (const m of j.moved) {
      outcomes.push(m.outcome);
      bytes += m.size;
    }
    let state: DedupeJobView['state'] = 'done';
    let paused: DedupeReportView['paused'] = null;
    let resumable: Work[] = [];
    const total = works.length;
    emitProgress({ plan_id: planId, phase: 'apply', done: 0, total });
    for (let i = 0; i < works.length; i++) {
      const w = works[i]!;
      // Checked between groups, never inside one.
      const newGroup = i === 0 || works[i - 1]!.group !== w.group;
      if (newGroup && own.cancel) {
        state = 'cancelled';
        // Like Rust, what the cancel left can be resumed.
        resumable = works.slice(i);
        for (const c of resumable)
          outcomes.push({
            group: c.group,
            member: c.member,
            files: [],
            status: 'skipped',
            reason: { kind: 'cancelled' },
          });
        break;
      }
      if (
        scenario.pauseAfter !== null &&
        !j.pausedOnce &&
        j.moved.length >= scenario.pauseAfter
      ) {
        j.pausedOnce = true;
        state = 'paused';
        paused = 'disk-full';
        resumable = works.slice(i);
        break;
      }
      await host.tick();
      let status: DedupeItemStatusView;
      let placed: string[];
      if (j.trash) {
        if (!scenario.trashAvailable) {
          outcomes.push({
            group: w.group,
            member: w.member,
            files: [],
            status: 'skipped',
            reason: { kind: 'trash-unavailable' },
          });
          continue;
        }
        placed = w.rels.map((r) => trashPlace(j.root, baseName(r)));
        status = { status: 'trashed', location: placed[0]! };
      } else {
        placed = w.rels.map((r) => join(j.quarantine, r));
        status = { status: 'quarantined', to: placed[0]! };
      }
      const outcome: DedupeOutcomeView = {
        group: w.group,
        member: w.member,
        files: placed,
        ...status,
      };
      j.moved.push({ rels: w.rels, placed, size: w.size, outcome });
      (away.get(j.root) ?? away.set(j.root, new Set()).get(j.root)!).add(
        w.rels[0]!,
      );
      bytes += w.size;
      outcomes.push(outcome);
      emitProgress({ plan_id: planId, phase: 'apply', done: i + 1, total });
    }
    j.remaining = resumable;
    j.complete = state === 'done';
    const anything = j.moved.length > 0;
    if (!anything) journals.splice(journals.indexOf(j), 1);
    const report: DedupeReportView = {
      job: anything ? j.job : null,
      outcomes,
      bytes_out: bytes,
      art_left: [],
      paused,
      resumable: resumable.map((w) => [w.group, w.member]),
      notes: [],
    };
    applying = null;
    emitChanged({
      ...own.view,
      job: report.job,
      state,
      progress: null,
      report,
    });
  }

  function start(planId: number, j: Journal): DedupeJobView {
    const view: DedupeJobView = {
      job: null,
      plan_id: planId,
      state: 'applying',
      progress: null,
      report: null,
      root: j.root,
      quarantine: j.quarantine,
      trash: j.trash,
      error: null,
    };
    applying = { view, cancel: false };
    last = structuredClone(view);
    return { ...view };
  }

  function historyItem(j: Journal): JobHistoryItem {
    return {
      job: j.job,
      root: j.root,
      created_at: j.createdAt,
      entries: j.moved.length,
      complete: j.complete,
      undone: j.undone,
      kind: 'dedupe',
      undoable: true,
    };
  }

  function undoOne(j: Journal): OrganizeUndoReport {
    const roms = romsDirOf(j.root);
    const out = away.get(j.root);
    const restored: string[] = [];
    for (const m of [...j.moved].reverse()) {
      for (const r of m.rels) restored.push(join(roms, r));
      out?.delete(m.rels[0]!);
    }
    // Folders the job made in the quarantine, deepest first, then itself.
    const dirs = new Set<string>();
    if (!j.trash) {
      for (const m of j.moved)
        for (let d = dirOf(m.placed[0]!); d.length > j.quarantine.length;)
          d = (dirs.add(d), dirOf(d));
      dirs.add(j.quarantine);
    }
    j.undone = true;
    j.remaining = [];
    return {
      job: j.job,
      removed: [],
      restored,
      skipped_modified: [],
      skipped_occupied: [],
      dirs_removed: [...dirs].sort((a, b) => b.length - a.length),
      archive_lost: [],
    };
  }

  return {
    revealedPaths,
    isDedupeApplying: () => applying !== null,
    dedupeHistory: () => [...journals].reverse().map(historyItem),
    ownsDedupeJob: (job) => journals.some((j) => j.job === job),
    dedupeScenario(patch) {
      Object.assign(scenario, patch);
    },

    async dedupePlan(root, req = {}) {
      seq += 1;
      const planId = seq;
      const files =
        (scenario.empty ? 0 : fixtureGroups().length * 2) + OTHER_FILES;
      const totals: Record<string, number> = {
        listing: files,
        sizes: files,
        fingerprints: scenario.empty ? 0 : 199,
        serials: scenario.empty ? 0 : 9,
        catalogues: scenario.empty ? 0 : 4,
      };
      for (const phase of DEDUPE_PHASES) {
        const total = totals[phase]!;
        for (const done of [0, Math.floor(total / 2), total]) {
          emitProgress({ plan_id: planId, phase, done, total });
          await host.tick();
          if (cancelled.has(planId)) {
            cancelled.delete(planId);
            throw new BackendError('cancelled');
          }
          if (scenario.failPlan) throw new BackendError(scenario.failPlan);
        }
      }
      if (req.reset_choices) picks.delete(root);
      const k: Omit<Kept, 'view'> = {
        root,
        opts: optsFrom(req),
        skippedGroups: new Set(),
        skippedSystems: new Set(),
      };
      const view = build(planId, k);
      plans.set(planId, { ...k, view });
      while (plans.size > KEPT_PLANS) plans.delete(plans.keys().next().value!);
      return structuredClone(view);
    },
    async dedupeCancel(planId) {
      cancelled.add(planId);
    },
    async dedupeRegroup(planId, req) {
      return edit(planId, (k) => {
        // A regroup reads nothing again: the systems stay.
        const { systems: _systems, ...rest } = req;
        k.opts = optsFrom(rest, k.opts);
      });
    },
    async dedupeSetKeeper(planId, group, member) {
      const k = kept(planId);
      const g = groupOf(k, group);
      const m = g.members.find((x) => x.id === member);
      if (!m)
        throw new BackendError(
          'bad-request',
          `duplicates group ${group} has no member ${member}`,
        );
      if (g.action.kind === 'conflict')
        throw new BackendError(
          'keeper-required',
          `duplicates group ${group}: settle the saves first`,
        );
      const rel = m.unit.primary.slice(romsDirOf(k.root).length + 1);
      (picks.get(k.root) ?? picks.set(k.root, new Map()).get(k.root)!).set(
        group,
        rel,
      );
      return edit(planId, () => undefined);
    },
    async dedupeSkipGroup(planId, group, skip) {
      return edit(planId, (k) => {
        groupOf(k, group);
        if (skip) k.skippedGroups.add(group);
        else k.skippedGroups.delete(group);
      });
    },
    async dedupeSkipSystem(planId, system, skip) {
      return edit(planId, (k) => {
        if (skip) k.skippedSystems.add(system);
        else k.skippedSystems.delete(system);
      });
    },
    async dedupeResetChoices(root, systems) {
      const mine = picks.get(root);
      if (!mine) return 0;
      const byGroup = new Map(fixtureGroups().map((g) => [g.id, g.system]));
      let n = 0;
      for (const id of [...mine.keys()]) {
        const system = byGroup.get(id)?.toLowerCase();
        if (systems && !systems.some((s) => s.toLowerCase() === system))
          continue;
        mine.delete(id);
        n += 1;
      }
      return n;
    },
    async dedupeApply(planId) {
      const k = kept(planId);
      notApplying();
      const view = k.view;
      const j: Journal = {
        job: `dedupe-${String(journals.length + 1).padStart(4, '0')}`,
        createdAt: iso(host.now + 7200 + journals.length),
        root: k.root,
        quarantine: view.quarantine,
        trash: view.trash,
        planId,
        moved: [],
        remaining: [],
        pausedOnce: false,
        complete: false,
        undone: false,
      };
      journals.push(j);
      // Like Rust: the library changes, so no kept plan stays valid.
      plans.clear();
      const started = start(planId, j);
      void run(j, planId, worksOf(view), leftAlone(view));
      return started;
    },
    async dedupeCancelApply() {
      if (applying) applying.cancel = true;
    },
    async dedupeJob() {
      return last ? structuredClone(last) : null;
    },
    async dedupeResume(job) {
      const j = journals.find((x) => x.job === job);
      if (!j) throw new BackendError('no-dedupe-plan', job);
      if (j.undone || j.complete || j.remaining.length === 0)
        throw new BackendError(
          'bad-request',
          `job ${job} has nothing left to resume`,
        );
      notApplying();
      plans.clear();
      seq += 1;
      const planId = seq;
      const started = start(planId, j);
      started.job = j.job;
      applying!.view.job = j.job;
      void run(j, planId, j.remaining, []);
      return started;
    },
    async dedupeExportPlan(planId) {
      const { plan_id: _id, trash_available: _t, ...plan } = kept(planId).view;
      host.exports.push({
        name: MOCK_DEDUPE_EXPORT,
        text: JSON.stringify(plan, null, 2),
      });
      return MOCK_DEDUPE_EXPORT;
    },
    async revealPath(path) {
      revealedPaths.push(path);
    },
    onDedupeProgress: (cb) => listen(progress, cb),
    onDedupeChanged: (cb) => listen(changed, cb),

    async dedupeUndo(job, includeLater) {
      notApplying();
      const j = journals.find((x) => x.job === job);
      if (!j || j.undone) throw new BackendError('nothing-to-undo');
      const later = journals.filter(
        (o) =>
          journals.indexOf(o) > journals.indexOf(j) &&
          !o.undone &&
          o.root === j.root,
      );
      if (later.length > 0 && !includeLater)
        throw new BackendError(
          'later-jobs',
          later.map((o) => o.job).join(', '),
        );
      const report = [...later].reverse().map(undoOne);
      report.push(undoOne(j));
      plans.clear();
      return {
        report,
        later_jobs: [...later].reverse().map(historyItem),
      };
    },
  };
}
