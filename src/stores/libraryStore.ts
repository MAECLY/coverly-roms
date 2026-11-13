/**
 * The opened library and the Start screen state. `open()` is safe against
 * double opens: only the latest request may change the state.
 */
import { create } from 'zustand';

import { getBackend } from '@/ipc/backend';
import { toBackendError, type BackendError } from '@/ipc/errors';
import type {
  ChildView,
  EntryArtUpdate,
  LibraryView,
  RecentLibrary,
  ScanProgress,
} from '@/ipc/types';
import { dropArtUrls, dropChildArtUrls } from '@/lib/artCache';
import { buildSearchKeys } from '@/lib/search';

import { useFilterStore } from './filterStore';
import { useIdentityStore } from './identityStore';
import { useSelectionStore } from './selectionStore';
import { useViewStore } from './viewStore';

export type LibraryStatus = 'idle' | 'opening' | 'ready' | 'error';

interface LibraryState {
  status: LibraryStatus;
  view: LibraryView | null;
  /** Search keys indexed by entry id. */
  keys: readonly string[];
  /** The revision chosen per version group in this library (D7): version
   * group → `rel_path`; empty while none was chosen. */
  revisionDefaults: Readonly<Record<string, string>>;
  /** Folder of the open library, as picked. */
  path: string | null;
  progress: ScanProgress | null;
  error: BackendError | null;
  openingPath: string | null;
  /** The folder of the latest `open()`, kept after it fails (the Start
   * screen probes it on `no-profile`). */
  lastPath: string | null;
  recents: RecentLibrary[];
  loadRecents(): Promise<void>;
  /** Opens `path`; `profileId` skips layout detection (e.g. right after
   * organising a pile into that layout). */
  open(path: string, profileId?: string): Promise<void>;
  forget(path: string): Promise<void>;
  /** Shows an error raised outside `open()` (e.g. the folder picker). */
  fail(e: unknown): void;
  /** New art marks of rewritten entries; another generation is ignored. */
  applyArtUpdates(generation: number, updates: readonly EntryArtUpdate[]): void;
  /** The updates and DLC of entry `entryId` with new art marks (the
   * panel's «Leer el arte incluido» or «Buscar arte»). */
  applyChildren(
    generation: number,
    entryId: number,
    children: readonly ChildView[],
  ): void;
  /** Makes this revision its game's default (D7); the answer of a library
   * that is no longer open is dropped. */
  setDefaultRevision(entryId: number): Promise<void>;
  close(): void;
}

const NO_DEFAULTS: Readonly<Record<string, string>> = Object.freeze({});

const initial = {
  status: 'idle' as LibraryStatus,
  view: null,
  keys: [],
  revisionDefaults: NO_DEFAULTS,
  path: null,
  progress: null,
  error: null,
  openingPath: null,
  lastPath: null,
  recents: [],
};

let requestSeq = 0;
/**
 * Highest scan generation ever seen. Never reset: a superseded scan keeps
 * emitting after a re-open, and its generation is at most this value.
 */
let maxSeenGen = 0;

export const useLibraryStore = create<LibraryState>()((set, get) => ({
  ...initial,
  async loadRecents() {
    try {
      const backend = await getBackend();
      set({ recents: (await backend.recentLibraries()) ?? [] });
    } catch {
      set({ recents: [] });
    }
  },
  async open(path, profileId) {
    const seq = (requestSeq += 1);
    // Every scan started before this open has a generation <= floor.
    const floor = maxSeenGen;
    set({
      status: 'opening',
      openingPath: path,
      lastPath: path,
      progress: null,
      error: null,
    });
    let unlisten: (() => void) | undefined;
    try {
      const backend = await getBackend();
      unlisten = await backend.onScanProgress((progress) => {
        maxSeenGen = Math.max(maxSeenGen, progress.generation);
        // A superseded scan may keep emitting; its generation is lower.
        if (seq !== requestSeq || progress.generation <= floor) return;
        if (progress.generation < maxSeenGen) return;
        set({ progress });
      });
      const view = await backend.openLibrary(path, profileId);
      if (seq !== requestSeq) return;
      useFilterStore.getState().reset();
      useViewStore.getState().reset();
      useSelectionStore.getState().select(null);
      useIdentityStore.getState().reset();
      set({
        status: 'ready',
        view,
        keys: buildSearchKeys(view.entries),
        revisionDefaults: view.revision_defaults ?? NO_DEFAULTS,
        path,
        progress: null,
        openingPath: null,
      });
    } catch (e) {
      if (seq !== requestSeq) return;
      const error = toBackendError(e);
      if (error.code === 'superseded') return;
      set({ status: 'error', error, progress: null, openingPath: null });
    } finally {
      unlisten?.();
      if (seq === requestSeq) await get().loadRecents();
    }
  },
  fail(e) {
    requestSeq += 1;
    set({
      status: 'error',
      error: toBackendError(e),
      progress: null,
      openingPath: null,
    });
  },
  async forget(path) {
    try {
      const backend = await getBackend();
      set({ recents: await backend.forgetRecentLibrary(path) });
    } catch (e) {
      set({ error: toBackendError(e) });
    }
  },
  applyArtUpdates(generation, updates) {
    const view = get().view;
    if (!view || view.generation !== generation || updates.length === 0) return;
    const byId = new Map(updates.map((u) => [u.id, u.art]));
    const entries = view.entries.map((e) => {
      const art = byId.get(e.id);
      return art ? { ...e, art } : e;
    });
    dropArtUrls(generation, [...byId.keys()]);
    set({ view: { ...view, entries } });
  },
  applyChildren(generation, entryId, children) {
    const view = get().view;
    if (!view || view.generation !== generation) return;
    const entries = view.entries.map((e) =>
      e.id === entryId ? { ...e, children: [...children] } : e,
    );
    dropChildArtUrls(generation, entryId);
    set({ view: { ...view, entries } });
  },
  async setDefaultRevision(entryId) {
    const view = get().view;
    if (!view) return;
    const backend = await getBackend();
    const choices = await backend.setDefaultRevision(view.generation, entryId);
    if (get().view?.generation !== view.generation) return;
    set({ revisionDefaults: choices });
    // A game is selected through its default revision (D1): when another
    // revision of the same game was selected, the selection follows the
    // new default so every view still finds the game.
    const chosen = view.entries.find((e) => e.id === entryId);
    const selection = useSelectionStore.getState();
    const selected =
      selection.selectedId === null
        ? undefined
        : view.entries.find((e) => e.id === selection.selectedId);
    if (
      chosen?.version_group !== undefined &&
      selected !== undefined &&
      selected.id !== entryId &&
      selected.version_group === chosen.version_group
    )
      selection.select(entryId);
  },
  close() {
    requestSeq += 1;
    useSelectionStore.getState().select(null);
    useViewStore.getState().closeDrawer();
    set({ ...initial, recents: get().recents });
  },
}));

/** Test helper: back to the initial state. */
export function resetLibraryStore(): void {
  requestSeq += 1;
  maxSeenGen = 0;
  useLibraryStore.setState(initial);
  useFilterStore.getState().reset();
  useViewStore.getState().reset();
  useSelectionStore.getState().select(null);
  useIdentityStore.getState().reset();
}
