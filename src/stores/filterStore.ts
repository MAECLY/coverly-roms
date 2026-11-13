import { create } from 'zustand';

import type { Region } from '@/ipc/types';
import {
  EMPTY_FILTERS,
  NO_CONTENT,
  type ArtFilter,
  type ContentKey,
  type Filters,
} from '@/lib/filters';

function toggle<T>(list: readonly T[], value: T): T[] {
  return list.includes(value)
    ? list.filter((x) => x !== value)
    : [...list, value];
}

interface FilterState {
  filters: Filters;
  setQuery(query: string): void;
  setArt(art: ArtFilter): void;
  toggleSystem(index: number): void;
  toggleExt(ext: string): void;
  toggleRegion(region: Region): void;
  setDates(from: number | null, to: number | null): void;
  setSizes(min: number | null, max: number | null): void;
  /** Presses or releases a «Mostrar juegos que» chip (D14). Kept in
   * memory like every other filter. */
  toggleContent(key: ContentKey): void;
  /** Releases every content chip, leaving the other filters as they are. */
  clearContent(): void;
  /** Clears every filter but keeps the search text. */
  clear(): void;
  /** Back to no filters and no search. */
  reset(): void;
}

export const useFilterStore = create<FilterState>()((set) => {
  const patch = (p: Partial<Filters>) =>
    set((s) => ({ filters: { ...s.filters, ...p } }));
  return {
    filters: EMPTY_FILTERS,
    setQuery: (query) => patch({ query }),
    setArt: (art) => patch({ art }),
    toggleSystem: (i) =>
      set((s) => ({
        filters: { ...s.filters, systems: toggle(s.filters.systems, i) },
      })),
    toggleExt: (e) =>
      set((s) => ({
        filters: { ...s.filters, exts: toggle(s.filters.exts, e) },
      })),
    toggleRegion: (r) =>
      set((s) => ({
        filters: { ...s.filters, regions: toggle(s.filters.regions, r) },
      })),
    setDates: (modifiedFrom, modifiedTo) => patch({ modifiedFrom, modifiedTo }),
    setSizes: (sizeMin, sizeMax) => patch({ sizeMin, sizeMax }),
    toggleContent: (key) =>
      set((s) => ({
        filters: {
          ...s.filters,
          content: { ...s.filters.content, [key]: !s.filters.content[key] },
        },
      })),
    clearContent: () => patch({ content: NO_CONTENT }),
    clear: () =>
      set((s) => ({ filters: { ...EMPTY_FILTERS, query: s.filters.query } })),
    reset: () => set({ filters: EMPTY_FILTERS }),
  };
});
