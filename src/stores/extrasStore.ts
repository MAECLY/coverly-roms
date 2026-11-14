/**
 * The Extras section's state (extras spec §6.3): the open game's listing,
 * the write in flight, the last Extras job of each game (for «Deshacer»
 * and `Cmd/Ctrl+Z`) and the selected files. Files are named by the
 * `(rev, index)` handle of the listing on screen (D22); every write
 * answers the next listing, which replaces it. Requests are numbered: an
 * answer older than the listing on screen is dropped, and a listing of a
 * game that is no longer open is never shown (D4). A request that fails
 * or answers for another game supersedes nothing.
 */
import { create } from 'zustand';

import { getBackend, type Backend } from '@/ipc/backend';
import { toBackendError, type BackendError } from '@/ipc/errors';
import type {
  AddChoice,
  AddFinishedEvent,
  AddStarted,
  CategoryRef,
  ExtrasUndone,
  ExtrasView,
  ExtrasWriteView,
} from '@/ipc/types';
import { sameCategory } from '@/lib/extras';

import { usePlayerStore } from './playerStore';

/** A write the section can be waiting for. */
export type ExtrasAction =
  | 'rename'
  | 'move'
  | 'trash'
  | 'tags'
  | 'forget'
  | 'category-create'
  | 'category-rename'
  | 'category-remove'
  | 'add'
  | 'undo';

/** A background add (D13): where it is, for the section's progress line. */
export interface AddRun {
  entryId: number;
  add: number;
  /** Files copied so far, of `n`. */
  i: number;
  n: number;
  /** Bytes copied so far, of `total`. */
  done: number;
  total: number;
  /** «Cancelar» was pressed: it stops after the current file. */
  cancelling: boolean;
}

/** How a click changes the selection: only this file, toggle it, or the
 * range from the last clicked file within its category. */
export type SelectMode = 'only' | 'toggle' | 'range';

interface ExtrasState {
  generation: number | null;
  entryId: number | null;
  listing: ExtrasView | null;
  loading: boolean;
  error: BackendError | null;
  pending: ExtrasAction | null;
  /** The last undoable Extras job of each game, by entry id. */
  lastJob: Record<number, string>;
  /** Selected files, by index of the listing on screen. */
  selection: ReadonlySet<number>;
  anchor: number | null;
  /** Shows game `entryId`'s extras (keeps its last listing while it loads). */
  open(generation: number, entryId: number): Promise<void>;
  /** Lists the open game again («Buscar de nuevo»). */
  refresh(): Promise<void>;
  rename(index: number, stem: string): Promise<ExtrasWriteView>;
  move(files: number[], to: CategoryRef): Promise<ExtrasWriteView>;
  trash(files: number[]): Promise<ExtrasWriteView>;
  setTags(index: number, tags: string[]): Promise<ExtrasWriteView>;
  forget(missing: number): Promise<ExtrasWriteView>;
  createCategory(name: string): Promise<ExtrasWriteView>;
  renameCategory(from: string, to: string): Promise<ExtrasWriteView>;
  removeCategory(name: string): Promise<ExtrasWriteView>;
  /** The background add in flight (one writer at a time), or null. */
  addRun: AddRun | null;
  /** Copies the chosen files of add plan `plan` (D13). A short add answers
   * its write at once; a long one runs in the background (`addRun`) and
   * ends through `onAddEnd`. */
  addCommit(plan: number, choices: AddChoice[]): Promise<AddStarted>;
  /** Copies what OS drop `token` recorded into `category` (D11). */
  addDropped(token: number, category: CategoryRef): Promise<AddStarted>;
  /** «Cancelar» on the background add: stops after the current file. */
  cancelAdd(): Promise<void>;
  /** Takes a write done elsewhere (an add): its listing and its job. */
  applyWrite(write: ExtrasWriteView): void;
  /** Undoes the open game's last Extras job; null when there is none. */
  undo(): Promise<ExtrasUndone | null>;
  /** Undoes Extras job `job` of game `entryId` (a toast's «Deshacer»,
   * which may outlive the game on screen); its listing is shown only when
   * that game is still the open one. */
  undoJob(entryId: number, job: string): Promise<ExtrasUndone>;
  select(index: number, mode: SelectMode): void;
  clearSelection(): void;
  reset(): void;
}

const EMPTY = new Set<number>();

const INITIAL = {
  generation: null,
  entryId: null,
  listing: null,
  loading: false,
  error: null,
  pending: null,
  lastJob: {},
  selection: EMPTY,
  anchor: null,
  addRun: null,
} satisfies Partial<ExtrasState>;

/** Who hears that a background add ended (the section's toast). */
const addEndListeners = new Set<(e: AddFinishedEvent) => void>();

/** Hears every background add that ends, whatever game is open; answers
 * the function that stops it. */
export function onAddEnd(cb: (e: AddFinishedEvent) => void): () => void {
  addEndListeners.add(cb);
  return () => {
    addEndListeners.delete(cb);
  };
}

/** The backend whose add events the store hears (subscribed once each). */
let heard: Backend | null = null;
/** Background adds that ended before their start was answered. */
const endedEarly = new Set<number>();

/** Numbers every request that answers a listing. */
let seq = 0;
/** The request whose listing is on screen: older answers are dropped. */
let applied = 0;
/** The last listing request: the section loads until it settles. */
let listReq = 0;

/** The selection of `prev` carried by path into `next` (indexes are per
 * listing, D22: a stale index could name another file). */
function carrySelection(
  prev: ExtrasView,
  next: ExtrasView,
  selection: ReadonlySet<number>,
  anchor: number | null,
): { selection: ReadonlySet<number>; anchor: number | null } {
  if (selection.size === 0 && anchor === null) return { selection, anchor };
  const pathOf = new Map(prev.files.map((f) => [f.index, f.path]));
  const indexOf = new Map(next.files.map((f) => [f.path, f.index]));
  const follow = (i: number) => {
    const p = pathOf.get(i);
    return p === undefined ? undefined : indexOf.get(p);
  };
  const kept = new Set<number>();
  for (const i of selection) {
    const j = follow(i);
    if (j !== undefined) kept.add(j);
  }
  const a = anchor === null ? undefined : follow(anchor);
  return {
    selection: kept.size > 0 ? kept : EMPTY,
    anchor: a ?? null,
  };
}

export const useExtrasStore = create<ExtrasState>()((set, get) => {
  /** Shows `listing` when it answers request `req` of the open game. */
  function show(req: number, listing: ExtrasView): boolean {
    const s = get();
    if (req <= applied || listing.entry_id !== s.entryId) return false;
    applied = req;
    const carried =
      s.listing && s.listing.entry_id === listing.entry_id
        ? carrySelection(s.listing, listing, s.selection, s.anchor)
        : { selection: EMPTY, anchor: null };
    set({
      listing,
      error: null,
      ...carried,
      ...(req >= listReq ? { loading: false } : {}),
    });
    usePlayerStore.getState().syncListing(listing);
    return true;
  }

  function remember(entryId: number, job: string | null) {
    if (job === null) return;
    set((s) => ({ lastJob: { ...s.lastJob, [entryId]: job } }));
  }

  async function list(): Promise<void> {
    const { generation, entryId } = get();
    if (generation === null || entryId === null) return;
    const req = ++seq;
    listReq = req;
    set({ loading: true });
    try {
      const backend = await getBackend();
      show(req, await backend.extrasList(generation, entryId));
    } catch (e) {
      if (req !== listReq || entryId !== get().entryId) return;
      // A newer answer is already on screen: only the loading ends.
      if (req <= applied) set({ loading: false });
      else set({ loading: false, error: toBackendError(e) });
    }
  }

  /** Runs a write on the listing on screen, then shows its answer. */
  async function write(
    action: ExtrasAction,
    run: (
      backend: Backend,
      generation: number,
      entryId: number,
      rev: number,
    ) => Promise<ExtrasWriteView>,
  ): Promise<ExtrasWriteView> {
    const { generation, entryId, listing } = get();
    if (generation === null || entryId === null || listing === null)
      throw new Error('no extras listing open');
    const req = ++seq;
    set({ pending: action });
    try {
      const backend = await getBackend();
      const w = await run(backend, generation, entryId, listing.rev);
      remember(entryId, w.job);
      if (show(req, w.listing)) set({ selection: EMPTY, anchor: null });
      return w;
    } catch (e) {
      const err = toBackendError(e);
      // Changed behind the panel's back: show what is there now.
      if (err.code === 'extras-changed' && entryId === get().entryId)
        await list();
      throw err;
    } finally {
      if (get().pending === action) set({ pending: null });
    }
  }

  /** Hears the backend's add progress and ends (once per backend). */
  async function hearAdds(backend: Backend): Promise<void> {
    if (heard === backend) return;
    heard = backend;
    await backend.onExtrasProgress((e) => {
      const run = get().addRun;
      if (!run || run.add !== e.add) return;
      set({ addRun: { ...run, i: e.i, n: e.n, done: e.done, total: e.total } });
    });
    await backend.onExtrasAdded((e) => {
      const run = get().addRun;
      if (run && run.add === e.add) set({ addRun: null });
      else endedEarly.add(e.add);
      if (e.end.kind === 'done') get().applyWrite(e.end.write);
      addEndListeners.forEach((cb) => cb(e));
    });
  }

  /** Starts an add on the listing on screen (D13). */
  async function startAdd(
    run: (
      backend: Backend,
      generation: number,
      entryId: number,
      rev: number,
    ) => Promise<AddStarted>,
  ): Promise<AddStarted> {
    const { generation, entryId, listing } = get();
    if (generation === null || entryId === null || listing === null)
      throw new Error('no extras listing open');
    const req = ++seq;
    set({ pending: 'add' });
    try {
      const backend = await getBackend();
      await hearAdds(backend);
      const started = await run(backend, generation, entryId, listing.rev);
      if (started.kind === 'done') {
        remember(entryId, started.write.job);
        if (show(req, started.write.listing))
          set({ selection: EMPTY, anchor: null });
      } else if (!endedEarly.delete(started.add)) {
        set({
          addRun: {
            entryId,
            add: started.add,
            i: 0,
            n: started.n,
            done: 0,
            total: started.total,
            cancelling: false,
          },
        });
      }
      return started;
    } catch (e) {
      const err = toBackendError(e);
      if (err.code === 'extras-changed' && entryId === get().entryId)
        await list();
      throw err;
    } finally {
      if (get().pending === 'add') set({ pending: null });
    }
  }

  return {
    ...INITIAL,

    addCommit: (plan, choices) =>
      startAdd((b, g, e, rev) => b.extrasAddCommit(g, e, rev, plan, choices)),
    addDropped: (token, category) =>
      startAdd((b, g, e, rev) =>
        b.extrasAddDropped(g, e, rev, token, category),
      ),
    async cancelAdd() {
      const run = get().addRun;
      if (!run || run.cancelling) return;
      set({ addRun: { ...run, cancelling: true } });
      const backend = await getBackend();
      await backend.extrasAddCancel(run.add);
    },

    async open(generation, entryId) {
      const s = get();
      const same = s.generation === generation && s.entryId === entryId;
      if (!same)
        set({
          generation,
          entryId,
          listing: null,
          error: null,
          selection: EMPTY,
          anchor: null,
          // Another library: its jobs are not this one's.
          ...(s.generation !== generation ? { lastJob: {} } : {}),
        });
      await list();
    },

    refresh: list,

    rename: (index, stem) =>
      write('rename', (b, g, e, rev) => b.extrasRename(g, e, rev, index, stem)),
    move: (files, to) =>
      write('move', (b, g, e, rev) => b.extrasMove(g, e, rev, files, to)),
    trash: (files) =>
      write('trash', (b, g, e, rev) => b.extrasTrash(g, e, rev, files)),
    setTags: (index, tags) =>
      write('tags', (b, g, e, rev) => b.extrasSetTags(g, e, rev, index, tags)),
    forget: (missing) =>
      write('forget', (b, g, e, rev) => b.extrasForget(g, e, rev, missing)),
    createCategory: (name) =>
      write('category-create', (b, g, e, rev) =>
        b.extrasCategoryCreate(g, e, rev, name),
      ),
    renameCategory: (from, to) =>
      write('category-rename', (b, g, e, rev) =>
        b.extrasCategoryRename(g, e, rev, from, to),
      ),
    removeCategory: (name) =>
      write('category-remove', (b, g, e, rev) =>
        b.extrasCategoryRemove(g, e, rev, name),
      ),

    applyWrite(w) {
      remember(w.listing.entry_id, w.job);
      if (show(++seq, w.listing)) set({ selection: EMPTY, anchor: null });
    },

    async undo() {
      const { entryId, lastJob } = get();
      if (entryId === null) return null;
      const job = lastJob[entryId];
      if (job === undefined) return null;
      return get().undoJob(entryId, job);
    },

    async undoJob(entryId, job) {
      const { generation } = get();
      if (generation === null) throw new Error('no extras library open');
      const req = ++seq;
      const open = entryId === get().entryId;
      if (open) set({ pending: 'undo' });
      try {
        const backend = await getBackend();
        const u = await backend.extrasUndo(generation, entryId, job);
        set((s) => {
          const rest = { ...s.lastJob };
          if (rest[entryId] === job) delete rest[entryId];
          return { lastJob: rest };
        });
        if (show(req, u.listing)) set({ selection: EMPTY, anchor: null });
        return u;
      } finally {
        if (open && get().pending === 'undo') set({ pending: null });
      }
    },

    select(index, mode) {
      const { listing, selection, anchor } = get();
      if (!listing) return;
      const file = listing.files.find((f) => f.index === index);
      if (!file) return;
      if (mode === 'toggle') {
        const next = new Set(selection);
        if (next.has(index)) next.delete(index);
        else next.add(index);
        set({ selection: next, anchor: index });
        return;
      }
      const from =
        mode === 'range' && anchor !== null
          ? listing.files.find((f) => f.index === anchor)
          : undefined;
      if (!from || !sameCategory(from.category, file.category)) {
        set({ selection: new Set([index]), anchor: index });
        return;
      }
      // List order within the category, from the anchor to the click.
      const inCategory = listing.files.filter((f) =>
        sameCategory(f.category, file.category),
      );
      const a = inCategory.indexOf(from);
      const z = inCategory.indexOf(file);
      const [lo, hi] = a <= z ? [a, z] : [z, a];
      set({
        selection: new Set(inCategory.slice(lo, hi + 1).map((f) => f.index)),
      });
    },

    clearSelection() {
      set({ selection: EMPTY, anchor: null });
    },

    reset() {
      seq++;
      applied = seq;
      listReq = seq;
      set(INITIAL);
    },
  };
});
