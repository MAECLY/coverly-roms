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
  | 'undo';

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
} satisfies Partial<ExtrasState>;

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

  return {
    ...INITIAL,

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
