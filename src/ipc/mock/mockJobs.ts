/**
 * Fake fill jobs for the browser mock (`pnpm dev`, tests): deterministic
 * plans, review items, failures and one rate-limit pause per job, drawer
 * writes and exports, so the whole fill flow runs end to end in Chrome.
 *
 * Sources (M6): when SteamGridDB is enabled and has a key
 * (`MockJobsHost.sgdbReady`), every review/auto cover item also gets two
 * SteamGridDB grids (votes `{up: 9 + id % 7, down: 1}` and `{up: 2,
 * down: 0}`). Under `popularity` the list is ranked like Rust's Popularity
 * (exact matches one tier, releases before betas, then net votes), so the
 * most voted grid is chosen with `reason: votes`; the other strategies
 * keep libretro first (a simplification: the mock does not rank by source
 * order or region). Index 0 carries the reason: `only` for one candidate,
 * `release` when a release beat a beta, `votes` when votes decided, else
 * `consensus` (popularity), `source-order` (priority) or `region`
 * (rom-region).
 */
import { csvText } from '@/lib/csv';

import type { Backend } from '../backend';
import { BackendError } from '../errors';
import {
  MEDIA,
  REGIONS,
  type ActionKind,
  type ArtImage,
  type ArtMark,
  type ArtMarks,
  type CandidateView,
  type Decision,
  type EntriesUpdated,
  type EntryArtUpdate,
  type EntryView,
  type ItemErrorKind,
  type JobItemView,
  type JobProgressEvent,
  type JobTotals,
  type JobView,
  type LibraryView,
  type Media,
  type Phase,
  type PickReason,
  type PlanSummary,
  type Region,
  type StrategyKind,
} from '../types';
import { labelSvg } from './svg';

export const MOCK_PAUSE_DETAIL =
  'rate limited by thumbnails.libretro.com (HTTP 429)';
const FAIL_KINDS: readonly ItemErrorKind[] = [
  'no-art',
  'network',
  'decode',
  'name-invalid',
];

/** auto (7 in 10), review (2 in 10) or none, per entry and media. */
export function mockClass(id: number, media: Media): ActionKind {
  const k = (id + MEDIA.indexOf(media)) % 10;
  return k < 7 ? 'auto' : k < 9 ? 'review' : 'none';
}

/** How an auto item of entry `id` fails when applied, if it does. */
export function mockFailure(id: number): ItemErrorKind | null {
  return id % 13 === 0 ? (FAIL_KINDS[id % FAIL_KINDS.length] ?? 'io') : null;
}

/** The name without `(…)` and `[…]` tags. */
export function mockTitle(name: string): string {
  return name.replace(/\s*[([][^)\]]*[)\]]/g, '').trim() || name;
}

function cand(
  index: number,
  name: string,
  regions: Region[],
  kind: CandidateView['kind'],
  score: number,
  extra: Partial<Pick<CandidateView, 'source' | 'votes' | 'origin'>> = {},
): CandidateView {
  const confidence =
    kind === 'canonical' || kind === 'name-exact'
      ? 1
      : kind === 'title-exact'
        ? 0.95
        : Math.min(score, 94) / 100;
  return {
    index,
    source: 'libretro',
    name,
    regions,
    kind,
    score,
    confidence,
    votes: null,
    origin: null,
    reason: null,
    ...extra,
  };
}

/** A beta/proto/demo/hack name (like `is_unreleased_variant`). */
const isVariant = (name: string) =>
  /[([](?:beta|proto|prototype|demo|sample|hack)\b[^)\]]*[)\]]/i.test(name);
const net = (c: CandidateView) => (c.votes ? c.votes.up - c.votes.down : 0);
const exact = (c: CandidateView) => (c.kind === 'fuzzy' ? 0 : 1);

/** The two SteamGridDB grids of entry `e` (no region, title match). */
function grids(e: EntryView): CandidateView[] {
  const t = mockTitle(e.name);
  const grid = (n: number, up: number, down: number) =>
    cand(0, t, [], 'title-exact', 95, {
      source: 'steamgriddb',
      votes: { up, down },
      origin: `game ${1000 + e.id} · grid ${e.id * 10 + n}`,
    });
  return [grid(1, 9 + (e.id % 7), 1), grid(2, 2, 0)];
}

/** Why index 0 beat index 1 (a simplified `matching::explain`). */
function reasonFor(
  list: readonly CandidateView[],
  strategy: StrategyKind,
  romRegions: readonly Region[],
): PickReason | null {
  const [a, b] = list;
  if (!a) return null;
  if (!b) return { why: 'only' };
  if (!isVariant(a.name) && isVariant(b.name)) return { why: 'release' };
  if (strategy === 'popularity' && net(a) !== net(b)) {
    return { why: 'votes', score: net(a), next: net(b) };
  }
  switch (strategy) {
    case 'popularity':
      return { why: 'consensus', count: 3, next: 1 };
    case 'priority':
      return { why: 'source-order', source: a.source };
    case 'rom-region':
      return {
        why: 'region',
        rom: a.regions.some((r) => romRegions.includes(r)),
      };
  }
}

function candidatesFor(
  e: EntryView,
  media: Media,
  strategy: StrategyKind,
  sgdb: boolean,
) {
  const t = mockTitle(e.name);
  const action = mockClass(e.id, media);
  const variant = (e.id + MEDIA.indexOf(media)) % 10 === 8;
  const candidates: CandidateView[] =
    action === 'auto'
      ? [
          cand(0, `${t} (USA)`, ['usa'], 'canonical', 100),
          cand(1, `${t} (Europe)`, ['europe'], 'title-exact', 95),
        ]
      : action === 'none'
        ? []
        : variant
          ? [
              cand(0, `${t} (USA) (Beta)`, ['usa'], 'title-exact', 95),
              cand(1, `${t} (Japan)`, ['japan'], 'fuzzy', 72),
            ]
          : [
              cand(0, `${t} (Europe)`, ['europe'], 'fuzzy', 82),
              cand(1, `${t} (Japan)`, ['japan'], 'fuzzy', 78),
              cand(2, `${t} (USA) (Beta)`, ['usa'], 'title-exact', 95),
            ];
  let ranked = candidates;
  // SteamGridDB has covers only (D35).
  if (sgdb && media === 'cover' && action !== 'none') {
    ranked = [...candidates, ...grids(e)];
    if (strategy === 'popularity') {
      // Stable: equal candidates keep the libretro order.
      ranked = ranked
        .map((c, n) => ({ c, n }))
        .sort(
          (x, y) =>
            exact(y.c) - exact(x.c) ||
            Number(isVariant(x.c.name)) - Number(isVariant(y.c.name)) ||
            net(y.c) - net(x.c) ||
            x.n - y.n,
        )
        .map(({ c }) => c);
    }
  }
  const out: CandidateView[] = ranked.map((c, index) => ({
    ...c,
    index,
    reason: null,
  }));
  const reason = reasonFor(out, strategy, e.regions);
  if (out[0]) out[0].reason = reason;
  // The beta stays first only when nothing better ranked above it.
  const betaFirst = out[0] !== undefined && isVariant(out[0].name);
  return {
    action,
    candidates: out,
    note:
      action === 'review' && variant && betaFirst
        ? ('unreleased-variant' as const)
        : null,
  };
}

function iso(secs: number): string {
  return new Date(secs * 1000).toISOString().replace(/\.\d{3}Z$/, 'Z');
}

function svgImage(label: string, media: Media, hue: number): ArtImage {
  return {
    bytes: new TextEncoder().encode(labelSvg(label, media !== 'cover', hue)),
    mime: 'image/svg+xml',
  };
}

export interface MockExport {
  name: string;
  text: string;
}

export interface MockJobsHost {
  /** The open library, with the latest art marks. */
  current(): LibraryView | null;
  /** Replaces the art marks of some entries of the open library. */
  patch(updates: readonly EntryArtUpdate[]): void;
  /** One fake unit of work (a macrotask, so callers see `gathering`). */
  tick(): Promise<void>;
  now: number;
  /** SteamGridDB is enabled and has a key (read when a fill starts). */
  sgdbReady(): boolean;
}

export interface MockJobsOptions {
  pauseAfter: number | null;
  pick: 'image' | 'cancel' | 'not-image';
}

type JobsApi = Pick<
  Backend,
  | 'startFill'
  | 'fillJob'
  | 'fillJobItems'
  | 'decideFill'
  | 'runFill'
  | 'resumeFill'
  | 'cancelFill'
  | 'candidateArt'
  | 'undoLastJob'
  | 'jobHistory'
  | 'onJobProgress'
  | 'onJobChanged'
  | 'onEntriesUpdated'
  | 'entryCandidates'
  | 'entryCandidateArt'
  | 'setEntryArt'
  | 'setEntryArtFromFile'
  | 'removeEntryArt'
  | 'exportAudit'
  | 'exportJobReport'
  | 'searchEntryArt'
  | 'entrySearchArt'
  | 'setEntryArtFromSearch'
  | 'clearIdentityOverride'
> & {
  readonly exports: MockExport[];
  /** The manual identity ("Search by title" pick) of an entry, if any. */
  manualIdentity(generation: number, id: number): string | null;
};

/** Longest title "Search by title" accepts (`MAX_QUERY_CHARS` in Rust). */
export const MAX_QUERY_CHARS = 120;

/** Like `fill::clean_query`: trimmed, 1–120 characters, no control characters. */
function cleanQuery(q: string): string {
  const t = q.trim();
  if (!t || [...t].length > MAX_QUERY_CHARS || /\p{Cc}/u.test(t)) {
    throw new BackendError(
      'bad-request',
      `type a title of 1 to ${MAX_QUERY_CHARS} characters`,
    );
  }
  return t;
}

interface Journal {
  job: string;
  createdAt: number;
  root: string;
  undone: boolean;
  complete: boolean;
  changes: { id: number; media: Media; before: ArtMark }[];
}

interface Job {
  view: JobView;
  items: JobItemView[];
  root: string;
  /** The job's own view of its entries' marks (its session, like Rust). */
  marks: Map<number, ArtMarks>;
  cancel: boolean;
  pausedOnce: boolean;
  journal: Journal | null;
}

export function createMockJobs(
  host: MockJobsHost,
  opts: MockJobsOptions,
): JobsApi {
  const progress = new Set<(e: JobProgressEvent) => void>();
  const changed = new Set<(v: JobView) => void>();
  const updated = new Set<(e: EntriesUpdated) => void>();
  const journals: Journal[] = [];
  const exports: MockExport[] = [];
  let seq = 0;
  let job: Job | null = null;
  let picks: {
    generation: number;
    id: number;
    media: Media;
    candidates: CandidateView[];
  } | null = null;
  let searches: {
    generation: number;
    id: number;
    media: Media;
    candidates: CandidateView[];
  } | null = null;
  /** Manual identities by ROM (library root + relative path), like
   * Rust's `identity-overrides.json` keyed by the ROM's path. */
  const manual = new Map<string, string>();
  const romKey = (generation: number, id: number) => {
    const view = library(generation);
    const e = entry(generation, id);
    return `${view.root}\u0000${e.rel_path}`;
  };
  const searched = (generation: number, id: number, media: Media) => {
    const s = searches;
    return s && s.generation === generation && s.id === id && s.media === media
      ? s.candidates
      : [];
  };

  const running = () =>
    job !== null &&
    (job.view.state === 'gathering' || job.view.state === 'applying');
  const snapshot = (j: Job): JobView => ({
    ...j.view,
    decided: j.items.filter((i) => i.decision !== null).length,
    skipped: j.items.filter((i) => i.decision?.choice === 'skip').length,
  });
  const emitChanged = (j: Job) => {
    const v = snapshot(j);
    changed.forEach((cb) => cb(v));
  };
  const emitProgress = (
    j: Job,
    phase: Phase,
    done: number,
    total: number,
    label: string,
  ) => {
    j.view.progress = { phase, done, total, label };
    progress.forEach((cb) => cb({ job: j.view.id, phase, done, total, label }));
  };

  function library(generation: number): LibraryView {
    const view = host.current();
    if (!view) throw new BackendError('no-library');
    if (view.generation !== generation) throw new BackendError('stale-library');
    return view;
  }
  function entry(generation: number, id: number): EntryView {
    const e = library(generation).entries[id];
    if (!e) throw new BackendError('unknown-entry', String(id));
    return e;
  }
  function fresh(id: number): EntryView | undefined {
    return host.current()?.entries[id];
  }
  function current(id: number): Job {
    if (!job) throw new BackendError('no-job');
    if (job.view.id !== id) throw new BackendError('stale-job');
    return job;
  }
  function notRunning() {
    if (running()) throw new BackendError('job-running');
  }
  function newJournal(root: string): Journal {
    const n = journals.length + 1;
    const j: Journal = {
      job: `mock-${String(n).padStart(4, '0')}`,
      createdAt: host.now + n * 60,
      root,
      undone: false,
      complete: false,
      changes: [],
    };
    journals.push(j);
    return j;
  }
  /**
   * Sets one mark of entry `id` (whose marks were `art` in library
   * `generation`), journaled; returns the entry's new marks. The open
   * library is patched only when it is that same generation: a library
   * opened since is another session and keeps its own marks.
   */
  function write(
    journal: Journal,
    generation: number,
    art: ArtMarks | undefined,
    id: number,
    media: Media,
    mark: ArtMark,
  ): EntryArtUpdate | null {
    if (!art) return null;
    journal.changes.push({ id, media, before: art[media] });
    const update = { id, art: { ...art, [media]: mark } };
    if (host.current()?.generation === generation) host.patch([update]);
    return update;
  }
  /** One update per entry (the last one wins). */
  function merged(updates: (EntryArtUpdate | null)[]): EntryArtUpdate[] {
    const byId = new Map<number, EntryArtUpdate>();
    for (const u of updates) if (u) byId.set(u.id, u);
    return [...byId.values()];
  }
  function summary(items: JobItemView[]): PlanSummary {
    const totals = { auto: 0, review: 0, none: 0 };
    const bySystem = new Map<
      string,
      { auto: number; review: number; none: number }
    >();
    for (const i of items) {
      totals[i.action] += 1;
      const s = bySystem.get(i.folder) ?? { auto: 0, review: 0, none: 0 };
      s[i.action] += 1;
      bySystem.set(i.folder, s);
    }
    const per_system = [...bySystem.entries()]
      .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
      .map(([folder, c]) => ({ folder, ...c }));
    return { totals, per_system, warnings: [] };
  }
  function totals(items: JobItemView[]): JobTotals {
    const t = { filled: 0, replaced: 0, skipped: 0, no_art: 0, failed: 0 };
    for (const i of items) {
      const o = i.outcome;
      if (o?.status === 'filled') t.filled += 1;
      else if (o?.status === 'replaced') t.replaced += 1;
      else if (o?.status === 'failed' && o.error === 'no-art') t.no_art += 1;
      else if (o?.status === 'failed') t.failed += 1;
      else t.skipped += 1;
    }
    return t;
  }
  const wanted = (i: JobItemView) =>
    i.decision?.choice !== 'skip' &&
    (i.action === 'auto' || (i.action === 'review' && i.decision !== null));

  async function gather(j: Job) {
    const total = j.items.length;
    const step = Math.max(25, Math.ceil(total / 40));
    emitProgress(j, 'gather', 0, total, '');
    for (let n = 0; n < total && !j.cancel; n += step) {
      emitProgress(j, 'gather', n, total, j.items[n]?.name ?? '');
      await host.tick();
    }
    await host.tick();
    j.view.progress = null;
    j.view.plan = summary(j.items);
    j.view.state = j.cancel ? 'cancelled' : 'planned';
    emitChanged(j);
  }

  async function apply(j: Job) {
    const todo = j.items.filter(
      (i) => i.outcome === null || i.outcome.status === 'not-attempted',
    );
    for (const i of todo) {
      if (!wanted(i)) {
        i.outcome = {
          status:
            i.action === 'review' && i.decision === null
              ? 'skipped-review'
              : 'skipped-no-match',
          error: null,
          message: null,
        };
      }
    }
    const work = todo.filter(wanted);
    if (work.length > 0) j.journal ??= newJournal(j.root);
    const step = Math.max(10, Math.ceil(work.length / 40));
    const updates: (EntryArtUpdate | null)[] = [];
    let ran = 0;
    for (const [n, i] of work.entries()) {
      if (j.cancel) break;
      if (
        opts.pauseAfter !== null &&
        !j.pausedOnce &&
        ran === opts.pauseAfter
      ) {
        j.pausedOnce = true;
        j.view.paused = {
          phase: 'apply',
          kind: 'rate-limit',
          detail: MOCK_PAUSE_DETAIL,
        };
        break;
      }
      if (n % step === 0) {
        emitProgress(j, 'apply', n, work.length, i.name);
        await host.tick();
      }
      ran += 1;
      const fail =
        i.action === 'auto' && i.decision === null && i.entry_id !== null
          ? mockFailure(i.entry_id)
          : null;
      if (fail) {
        i.outcome = {
          status: 'failed',
          error: fail,
          message: `mock ${fail}: ${i.name}`,
        };
        continue;
      }
      if (i.entry_id === null || !j.journal) continue;
      const update = write(
        j.journal,
        j.view.generation,
        j.marks.get(i.entry_id),
        i.entry_id,
        i.media,
        'present',
      );
      if (update) j.marks.set(update.id, update.art);
      updates.push(update);
      i.outcome = {
        status: i.replaces_existing ? 'replaced' : 'filled',
        error: null,
        message: null,
      };
    }
    for (const i of work) {
      i.outcome ??= { status: 'not-attempted', error: null, message: null };
    }
    await host.tick();
    if (j.journal && !j.view.paused) j.journal.complete = true;
    j.view.journal = j.journal?.job ?? null;
    j.view.result = totals(j.items);
    j.view.progress = null;
    j.view.state = j.cancel ? 'cancelled' : j.view.paused ? 'paused' : 'done';
    const entries = merged(updates);
    if (entries.length > 0) {
      const ev = { generation: j.view.generation, entries };
      updated.forEach((cb) => cb(ev));
    }
    emitChanged(j);
  }

  function picked(
    generation: number,
    id: number,
    media: Media,
  ): CandidateView[] {
    if (
      !picks ||
      picks.generation !== generation ||
      picks.id !== id ||
      picks.media !== media
    ) {
      throw new BackendError('bad-request', 'list the candidates first');
    }
    return picks.candidates;
  }

  function record(name: string, text: string, mime: string): string {
    exports.push({ name, text });
    if (
      typeof document !== 'undefined' &&
      !/jsdom/i.test(navigator.userAgent)
    ) {
      const url = URL.createObjectURL(new Blob([text], { type: mime }));
      const a = document.createElement('a');
      a.href = url;
      a.download = name;
      a.click();
      setTimeout(() => URL.revokeObjectURL(url), 1000);
    }
    return name;
  }

  /**
   * Each call is its own subscription, like Tauri's `listen`: subscribing
   * the same callback twice and dropping one keeps the other.
   */
  const listen = <T>(set: Set<(v: T) => void>, cb: (v: T) => void) => {
    const own = (v: T) => cb(v);
    set.add(own);
    return Promise.resolve(() => {
      set.delete(own);
    });
  };

  return {
    exports,
    async startFill(generation, request) {
      const view = library(generation);
      notRunning();
      const media = MEDIA.filter(
        (m) => request.media.includes(m) && view.profile.media.includes(m),
      );
      if (media.length === 0) {
        throw new BackendError(
          'bad-request',
          'this layout cannot store the chosen art',
        );
      }
      const bad = request.regions.find(
        (r) => r !== 'rom' && !REGIONS.includes(r),
      );
      if (bad !== undefined) {
        throw new BackendError(
          'bad-request',
          `unknown region \`${String(bad)}\``,
        );
      }
      const ids = [
        ...new Set(request.entry_ids ?? view.entries.map((e) => e.id)),
      ].sort((a, b) => a - b);
      const items: JobItemView[] = [];
      const marks = new Map<number, ArtMarks>();
      // Like Rust's per-job snapshot of settings and keys.
      const sgdb = host.sgdbReady();
      for (const id of ids) {
        const e = view.entries[id];
        if (!e) continue;
        marks.set(e.id, e.art);
        for (const m of media) {
          const mark = e.art[m];
          const replace = request.overwrite === 'backup' && mark === 'present';
          if (mark !== 'missing' && !replace) continue;
          const c = candidatesFor(e, m, request.strategy, sgdb);
          items.push({
            index: items.length,
            entry_id: e.id,
            folder: view.systems[e.system]?.folder ?? '',
            name: e.name,
            media: m,
            action: c.action,
            confidence: c.candidates[0]?.confidence ?? null,
            note: c.note,
            none_reason: c.action === 'none' ? 'no-candidates' : null,
            candidates: c.candidates,
            replaces_existing: mark === 'present',
            decision: null,
            outcome: null,
          });
        }
      }
      seq += 1;
      const j: Job = {
        view: {
          id: seq,
          generation,
          state: 'gathering',
          progress: null,
          paused: null,
          plan: null,
          decided: 0,
          skipped: 0,
          journal: null,
          result: null,
          error: null,
        },
        items,
        root: view.root,
        marks,
        cancel: false,
        pausedOnce: false,
        journal: null,
      };
      job = j;
      void gather(j);
      return snapshot(j);
    },
    fillJob: () => Promise.resolve(job ? snapshot(job) : null),
    async fillJobItems(id, filter) {
      const j = current(id);
      return j.items
        .filter((i) =>
          filter === 'all'
            ? true
            : filter === 'failed'
              ? i.outcome?.status === 'failed'
              : i.action === filter,
        )
        .map((i) => ({ ...i }));
    },
    async decideFill(id, decisions) {
      const j = current(id);
      if (j.view.state !== 'planned') {
        throw new BackendError(
          'bad-request',
          'decisions are taken before the job runs',
        );
      }
      // Like Rust: validate every decision before recording any.
      for (const d of decisions) {
        const item = j.items[d.index];
        if (!item) throw new BackendError('bad-request', `no item ${d.index}`);
        const count = item.candidates.length;
        if (count === 0) {
          throw new BackendError(
            'bad-request',
            `item ${d.index} has nothing to decide`,
          );
        }
        if (d.choice === 'pick' && d.candidate >= count) {
          throw new BackendError(
            'bad-request',
            `no candidate ${d.candidate} for item ${d.index}`,
          );
        }
      }
      for (const d of decisions) {
        const { index, ...decision } = d;
        j.items[index]!.decision = decision as Decision;
      }
      return snapshot(j);
    },
    async runFill(id) {
      const j = current(id);
      if (j.view.state !== 'planned') {
        throw new BackendError('bad-request', 'the job is not ready to run');
      }
      j.view.state = 'applying';
      void apply(j);
      return snapshot(j);
    },
    async resumeFill(id) {
      const j = current(id);
      if (j.view.state !== 'paused') {
        throw new BackendError('bad-request', 'the job is not paused');
      }
      j.view.paused = null;
      j.view.state = 'applying';
      void apply(j);
      return snapshot(j);
    },
    async cancelFill(id) {
      const j = current(id);
      j.cancel = true;
      if (j.view.state === 'planned' || j.view.state === 'paused') {
        j.view.state = 'cancelled';
        j.view.paused = null;
        emitChanged(j);
      }
      return snapshot(j);
    },
    async candidateArt(id, index, candidate) {
      const c = current(id).items[index]?.candidates[candidate];
      if (!c)
        throw new BackendError('bad-request', `no candidate ${candidate}`);
      await host.tick();
      return svgImage(
        c.name,
        current(id).items[index]!.media,
        (index * 53 + candidate * 97) % 360,
      );
    },
    async undoLastJob(generation) {
      notRunning();
      // Like Rust: a paused job resumes into its journal; only this library's journals.
      if (job?.view.state === 'paused') throw new BackendError('job-running');
      const view = library(generation);
      const journal = [...journals]
        .reverse()
        .find((j) => !j.undone && j.root === view.root);
      if (!journal) throw new BackendError('nothing-to-undo');
      journal.undone = true;
      const updates: (EntryArtUpdate | null)[] = [];
      let removed = 0;
      let restored = 0;
      for (const c of [...journal.changes].reverse()) {
        if (c.before === 'present') restored += 1;
        else removed += 1;
        const e = fresh(c.id);
        if (e) {
          const update = { id: c.id, art: { ...e.art, [c.media]: c.before } };
          host.patch([update]);
          updates.push(update);
        }
      }
      return {
        job: journal.job,
        removed,
        restored,
        kept: 0,
        entries: merged(updates),
      };
    },
    jobHistory: () =>
      Promise.resolve(
        [...journals].reverse().map((j) => ({
          job: j.job,
          root: j.root,
          created_at: iso(j.createdAt),
          entries: j.changes.length,
          complete: j.complete,
          undone: j.undone,
        })),
      ),
    onJobProgress: (cb) => listen(progress, cb),
    onJobChanged: (cb) => listen(changed, cb),
    onEntriesUpdated: (cb) => listen(updated, cb),
    async entryCandidates(generation, id, media) {
      notRunning();
      const e = entry(generation, id);
      if (!library(generation).profile.media.includes(media)) {
        throw new BackendError(
          'bad-request',
          'this layout cannot store that media',
        );
      }
      const t = mockTitle(e.name);
      const candidates = [
        cand(0, `${t} (USA)`, ['usa'], 'canonical', 100),
        cand(1, `${t} (Europe)`, ['europe'], 'title-exact', 95),
        cand(2, `${t} (Japan)`, ['japan'], 'fuzzy', 80),
        // SteamGridDB has covers only (D35); listed after libretro.
        ...(media === 'cover' && host.sgdbReady() ? grids(e) : []),
      ].map((c, index) => ({ ...c, index }));
      // Like Rust: the plan's reason on index 0 (canonical beat a title match).
      candidates[0]!.reason = { why: 'better-match' };
      picks = { generation, id, media, candidates };
      await host.tick();
      return candidates.map((c) => ({ ...c }));
    },
    async entryCandidateArt(generation, id, media, candidate) {
      entry(generation, id);
      const c = picked(generation, id, media)[candidate];
      if (!c)
        throw new BackendError('bad-request', `no candidate ${candidate}`);
      return svgImage(c.name, media, (id * 31 + candidate * 97) % 360);
    },
    async setEntryArt(generation, id, media, candidate) {
      notRunning();
      const view = library(generation);
      entry(generation, id);
      if (!picked(generation, id, media)[candidate]) {
        throw new BackendError('bad-request', `no candidate ${candidate}`);
      }
      const journal = newJournal(view.root);
      journal.complete = true;
      const art = entry(generation, id).art;
      return merged([write(journal, generation, art, id, media, 'present')]);
    },
    manualIdentity(generation, id) {
      return manual.get(romKey(generation, id)) ?? null;
    },
    async searchEntryArt(generation, id, media, query) {
      notRunning();
      entry(generation, id);
      if (!library(generation).profile.media.includes(media)) {
        throw new BackendError(
          'bad-request',
          'this layout cannot store that media',
        );
      }
      const q = cleanQuery(query);
      const candidates = [
        cand(0, `${q} (USA)`, ['usa'], 'title-exact', 95),
        cand(1, `${q} (Europe)`, ['europe'], 'title-exact', 95),
        cand(2, `${q} (Japan)`, ['japan'], 'fuzzy', 80),
      ];
      searches = { generation, id, media, candidates };
      await host.tick();
      return candidates.map((c) => ({ ...c }));
    },
    async entrySearchArt(generation, id, media, candidate) {
      entry(generation, id);
      const c = searched(generation, id, media)[candidate];
      if (!c) throw new BackendError('bad-request', 'search first');
      return svgImage(c.name, media, (id * 17 + candidate * 61) % 360);
    },
    async setEntryArtFromSearch(generation, id, media, candidate) {
      notRunning();
      const view = library(generation);
      const c = searched(generation, id, media)[candidate];
      if (!c) throw new BackendError('bad-request', 'search first');
      const journal = newJournal(view.root);
      journal.complete = true;
      const art = entry(generation, id).art;
      const out = merged([
        write(journal, generation, art, id, media, 'present'),
      ]);
      manual.set(romKey(generation, id), c.name);
      return out;
    },
    async clearIdentityOverride(generation, id) {
      manual.delete(romKey(generation, id));
    },
    async setEntryArtFromFile(generation, id, media) {
      notRunning();
      const view = library(generation);
      entry(generation, id);
      if (opts.pick === 'cancel') return null;
      if (opts.pick === 'not-image') {
        throw new BackendError('bad-image', 'not an image');
      }
      const journal = newJournal(view.root);
      journal.complete = true;
      const art = entry(generation, id).art;
      return merged([write(journal, generation, art, id, media, 'present')]);
    },
    async removeEntryArt(generation, id, media) {
      notRunning();
      const view = library(generation);
      const art = entry(generation, id).art;
      if (art[media] !== 'present') {
        throw new BackendError('art-missing', media);
      }
      const journal = newJournal(view.root);
      journal.complete = true;
      return merged([write(journal, generation, art, id, media, 'missing')]);
    },
    async exportAudit(generation, format) {
      const view = library(generation);
      const media = view.profile.media;
      if (format === 'csv') {
        const rows = [
          [
            'system',
            'folder',
            'rel_path',
            'stem',
            'size',
            'modified',
            ...media,
          ],
          ...view.entries.map((e) => {
            const s = view.systems[e.system];
            return [
              s?.system_id ?? '',
              s?.folder ?? '',
              e.rel_path,
              e.name,
              e.size,
              iso(e.mtime),
              ...media.map((m) => e.art[m]),
            ];
          }),
        ];
        return record('coverly-audit.csv', csvText(rows), 'text/csv');
      }
      const json = {
        root: view.root,
        profile_id: view.profile.id,
        media,
        entries: view.entries.map((e) => ({
          rel_path: e.rel_path,
          art: e.art,
        })),
      };
      return record(
        'coverly-audit.json',
        JSON.stringify(json, null, 2),
        'application/json',
      );
    },
    async exportJobReport(id, format) {
      const j = current(id);
      if (!j.view.result)
        throw new BackendError('bad-request', 'the job has not run yet');
      const name = `coverly-job-${id}.${format}`;
      if (format === 'json') {
        return record(
          name,
          JSON.stringify({ ...j.view.result, items: j.items }, null, 2),
          'application/json',
        );
      }
      const rows = [
        [
          'folder',
          'rel_path',
          'media',
          'status',
          'source',
          'name',
          'error',
          'message',
        ],
        ...j.items.map((i) => {
          const wrote =
            i.outcome?.status === 'filled' || i.outcome?.status === 'replaced';
          return [
            i.folder,
            i.name,
            i.media,
            i.outcome?.status ?? 'not-attempted',
            wrote ? (i.candidates[0]?.source ?? '') : '',
            wrote ? (i.candidates[0]?.name ?? '') : '',
            i.outcome?.error ?? '',
            i.outcome?.message ?? '',
          ];
        }),
      ];
      return record(name, csvText(rows), 'text/csv');
    },
  };
}
