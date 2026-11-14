/**
 * The Extras section's state (extras spec §6.3): the open game's listing,
 * the write in flight, the last Extras job of each game (for «Deshacer»
 * and `Cmd/Ctrl+Z`) and the selected files. Files are named by the
 * `(rev, index)` handle of the listing on screen (D22); every write
 * answers the next listing, which replaces it. A request counter drops
 * any listing a newer request superseded, and a listing of a game that is
 * no longer open is never shown (D4).
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

/** Numbers every request that answers a listing; only the last applies. */
let seq = 0;

export const useExtrasStore = create<ExtrasState>()((set, get) => {
  /** Shows `listing` when it answers request `req` of the open game. */
  function show(req: number, listing: ExtrasView): boolean {
    if (req !== seq || listing.entry_id !== get().entryId) return false;
    set({ listing, loading: false, error: null });
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
    set({ loading: true });
    try {
      const backend = await getBackend();
      show(req, await backend.extrasList(generation, entryId));
    } catch (e) {
      if (req === seq && entryId === get().entryId)
        set({ loading: false, error: toBackendError(e) });
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
      const { generation, entryId, lastJob } = get();
      if (generation === null || entryId === null) return null;
      const job = lastJob[entryId];
      if (job === undefined) return null;
      const req = ++seq;
      set({ pending: 'undo' });
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
        if (get().pending === 'undo') set({ pending: null });
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
      set(INITIAL);
    },
  };
});
