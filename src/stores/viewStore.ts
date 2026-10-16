import { create } from 'zustand';

import type { Scope } from '@/lib/scope';
import { DEFAULT_SORT, type Sort, type SortKey } from '@/lib/sort';

export type Layout = 'grid' | 'list';
/** The fill surface shown over the library, if any. */
export type Overlay = 'none' | 'wizard' | 'review' | 'jobs' | 'reports';
const LAYOUT_KEY = 'coverly.layout';

function readLayout(): Layout {
  try {
    return localStorage.getItem(LAYOUT_KEY) === 'list' ? 'list' : 'grid';
  } catch {
    return 'grid';
  }
}

interface ViewState {
  scope: Scope;
  layout: Layout;
  sort: Sort;
  drawerOpen: boolean;
  filtersOpen: boolean;
  overlay: Overlay;
  setScope(scope: Scope): void;
  setLayout(layout: Layout): void;
  setSort(sort: Sort): void;
  /** Same key flips the direction; another key sorts ascending. */
  toggleSort(key: SortKey): void;
  openDrawer(): void;
  closeDrawer(): void;
  toggleFilters(): void;
  setOverlay(overlay: Overlay): void;
  closeOverlay(): void;
  /** Per-library state back to defaults (the layout is kept). */
  reset(): void;
}

export const useViewStore = create<ViewState>()((set) => ({
  scope: { kind: 'all' },
  layout: readLayout(),
  sort: DEFAULT_SORT,
  drawerOpen: false,
  filtersOpen: false,
  overlay: 'none',
  setScope: (scope) => set({ scope }),
  setLayout(layout) {
    try {
      localStorage.setItem(LAYOUT_KEY, layout);
    } catch {
      // Remembering the layout is a convenience only.
    }
    set({ layout });
  },
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
}));
