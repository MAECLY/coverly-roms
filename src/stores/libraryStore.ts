/**
 * The opened library and the Start screen state. `open()` is safe against
 * double opens: only the latest request may change the state.
 */
import { create } from 'zustand';

import { getBackend } from '@/ipc/backend';
import { toBackendError, type BackendError } from '@/ipc/errors';
import type { LibraryView, RecentLibrary, ScanProgress } from '@/ipc/types';
import { buildSearchKeys } from '@/lib/search';

import { useFilterStore } from './filterStore';
import { useSelectionStore } from './selectionStore';
import { useViewStore } from './viewStore';

export type LibraryStatus = 'idle' | 'opening' | 'ready' | 'error';

interface LibraryState {
  status: LibraryStatus;
  view: LibraryView | null;
  /** Search keys indexed by entry id. */
  keys: readonly string[];
  /** Folder of the open library, as picked. */
  path: string | null;
  progress: ScanProgress | null;
  error: BackendError | null;
  openingPath: string | null;
  recents: RecentLibrary[];
  loadRecents(): Promise<void>;
  open(path: string): Promise<void>;
  forget(path: string): Promise<void>;
  close(): void;
}

const initial = {
  status: 'idle' as LibraryStatus,
  view: null,
  keys: [],
  path: null,
  progress: null,
  error: null,
  openingPath: null,
  recents: [],
};

let requestSeq = 0;

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
  async open(path) {
    const seq = (requestSeq += 1);
    set({ status: 'opening', openingPath: path, progress: null, error: null });
    const backend = await getBackend();
    const unlisten = await backend.onScanProgress((progress) => {
      if (seq === requestSeq) set({ progress });
    });
    try {
      const view = await backend.openLibrary(path);
      if (seq !== requestSeq) return;
      useFilterStore.getState().reset();
      useViewStore.getState().reset();
      useSelectionStore.getState().select(null);
      set({
        status: 'ready',
        view,
        keys: buildSearchKeys(view.entries),
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
      unlisten();
      if (seq === requestSeq) await get().loadRecents();
    }
  },
  async forget(path) {
    try {
      const backend = await getBackend();
      set({ recents: await backend.forgetRecentLibrary(path) });
    } catch (e) {
      set({ error: toBackendError(e) });
    }
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
  useLibraryStore.setState(initial);
  useFilterStore.getState().reset();
  useViewStore.getState().reset();
  useSelectionStore.getState().select(null);
}
