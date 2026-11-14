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

/**
 * A section of the game panel a view can open it at (addon views §8.2,
 * §8.3): Versión, or one update or DLC by its child index (`child-2`).
 */
export type PanelSection = 'version' | 'extras' | `child-${number}`;

interface ViewState {
  scope: Scope;
  prefs: ViewPrefs;
  /** The library the prefs belong to; `null` until one is loaded. */
  root: string | null;
  /** @deprecated alias for one release (V-D5): 'grid' for covers, 'list' otherwise. */
  layout: Layout;
  sort: Sort;
  drawerOpen: boolean;
  /**
   * The panel section to scroll to and focus once the panel shows it
   * (`[data-panel-section]`); the panel clears it when done.
   */
  panelSection: PanelSection | null;
  filtersOpen: boolean;
  overlay: Overlay;
  /**
   * The games open in the Details treegrid (game entry ids), for the
   * session; `null` until Details first shows, when the selected game
   * starts open (addon views §8.4).
   */
  expanded: ReadonlySet<number> | null;
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
  /** Opens the panel, at `section` when given. */
  openDrawer(section?: PanelSection): void;
  closeDrawer(): void;
  clearPanelSection(): void;
  toggleFilters(): void;
  setOverlay(overlay: Overlay): void;
  closeOverlay(): void;
  /** Opens or closes one game in the Details treegrid. */
  setExpanded(id: number, open: boolean): void;
  /** The first time Details shows: these games start open. */
  initExpanded(ids: readonly number[]): void;
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
    panelSection: null,
    filtersOpen: false,
    overlay: 'none',
    expanded: null,
    setScope: (scope) => set({ scope }),
    loadFor(root) {
      set({ root });
      apply(prefsFor(file, root));
    },
    setView: (view) => get().setPrefs({ view }),
    setPrefs(patch) {
      const prefs = sanitisePrefs({ ...get().prefs, ...patch });
      // Re-picking the current value neither rewrites storage nor re-renders.
      if (JSON.stringify(prefs) === JSON.stringify(get().prefs)) return;
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
    openDrawer: (section) =>
      set({ drawerOpen: true, panelSection: section ?? null }),
    closeDrawer: () => set({ drawerOpen: false, panelSection: null }),
    clearPanelSection: () => set({ panelSection: null }),
    toggleFilters: () => set((s) => ({ filtersOpen: !s.filtersOpen })),
    setOverlay: (overlay) => set({ overlay }),
    closeOverlay: () => set({ overlay: 'none' }),
    setExpanded(id, open) {
      const now = get().expanded;
      if (now !== null && now.has(id) === open) return;
      const next = new Set(now);
      if (open) next.add(id);
      else next.delete(id);
      set({ expanded: next });
    },
    initExpanded(ids) {
      if (get().expanded === null) set({ expanded: new Set(ids) });
    },
    reset: () =>
      set({
        scope: { kind: 'all' },
        sort: DEFAULT_SORT,
        drawerOpen: false,
        panelSection: null,
        filtersOpen: false,
        overlay: 'none',
        expanded: null,
      }),
  };
});
