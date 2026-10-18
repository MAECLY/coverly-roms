import { create } from 'zustand';

import type { Scope } from '@/lib/scope';
import { DEFAULT_SORT, type Sort, type SortKey } from '@/lib/sort';
import {
  prefsFor,
  readPrefsFile,
  remember,
  sanitisePrefs,
  writePrefsFile,
  type PrefsFile,
  type ViewId,
  type ViewPrefs,
} from '@/lib/viewPrefs';

export type Layout = 'grid' | 'list';
/** The fill surface shown over the library, if any. */
export type Overlay = 'none' | 'wizard' | 'review' | 'jobs' | 'reports';

/** `localStorage`, or nothing when the platform denies it. */
function storage(): Storage | undefined {
  try {
    return globalThis.localStorage;
  } catch {
    return undefined;
  }
}

function layoutOf(view: ViewId): Layout {
  return view === 'covers' ? 'grid' : 'list';
}

interface ViewState {
  scope: Scope;
  prefs: ViewPrefs;
  /** The library the prefs belong to; `null` until one is loaded. */
  root: string | null;
  /** @deprecated alias for one release (V-D5): 'grid' for covers, 'list' otherwise. */
  layout: Layout;
  sort: Sort;
  drawerOpen: boolean;
  filtersOpen: boolean;
  overlay: Overlay;
  setScope(scope: Scope): void;
  /** Load the prefs of a library (Explorer's per-folder memory). */
  loadFor(root: string): void;
  setView(view: ViewId): void;
  /** Sanitised, then saved under the library and as the last used. */
  setPrefs(patch: Partial<ViewPrefs>): void;
  /** @deprecated maps to `setView` ('grid' → covers, 'list' → details). */
  setLayout(layout: Layout): void;
  setSort(sort: Sort): void;
  /** Same key flips the direction; another key sorts ascending. */
  toggleSort(key: SortKey): void;
  openDrawer(): void;
  closeDrawer(): void;
  toggleFilters(): void;
  setOverlay(overlay: Overlay): void;
  closeOverlay(): void;
  /** Per-library state back to defaults (prefs and root are kept). */
  reset(): void;
}

export const useViewStore = create<ViewState>()((set, get) => {
  let file: PrefsFile = readPrefsFile(storage());

  function apply(prefs: ViewPrefs): void {
    set({ prefs, layout: layoutOf(prefs.view) });
  }

  return {
    scope: { kind: 'all' },
    prefs: file.last,
    root: null,
    layout: layoutOf(file.last.view),
    sort: DEFAULT_SORT,
    drawerOpen: false,
    filtersOpen: false,
    overlay: 'none',
    setScope: (scope) => set({ scope }),
    loadFor(root) {
      set({ root });
      apply(prefsFor(file, root));
    },
    setView: (view) => get().setPrefs({ view }),
    setPrefs(patch) {
      const prefs = sanitisePrefs({ ...get().prefs, ...patch });
      const root = get().root;
      file =
        root === null ? { ...file, last: prefs } : remember(file, root, prefs);
      writePrefsFile(storage(), file);
      apply(prefs);
    },
    setLayout: (layout) =>
      get().setView(layout === 'grid' ? 'covers' : 'details'),
    setSort: (sort) => set({ sort }),
    toggleSort: (key) =>
      set((s) => ({
        sort:
          s.sort.key === key
            ? { key, dir: s.sort.dir === 'asc' ? 'desc' : 'asc' }
            : { key, dir: 'asc' },
      })),
    openDrawer: () => set({ drawerOpen: true }),
    closeDrawer: () => set({ drawerOpen: false }),
    toggleFilters: () => set((s) => ({ filtersOpen: !s.filtersOpen })),
    setOverlay: (overlay) => set({ overlay }),
    closeOverlay: () => set({ overlay: 'none' }),
    reset: () =>
      set({
        scope: { kind: 'all' },
        sort: DEFAULT_SORT,
        drawerOpen: false,
        filtersOpen: false,
        overlay: 'none',
      }),
  };
});
