/**
 * The media viewer of the Extras section (extras spec D17, D19, D23): the
 * file it shows, named by its game and its path inside `Extras/` so a new
 * listing (a later `rev`) keeps it. While it is open the library's window
 * shortcuts stand down: its `+ - 0` zoom the file, not the covers.
 */
import { create } from 'zustand';

export interface ViewerTarget {
  generation: number;
  entryId: number;
  /** `/`-joined inside `Extras/`. */
  path: string;
}

interface ViewerState {
  open: ViewerTarget | null;
  show(target: ViewerTarget): void;
  close(): void;
}

export const useViewerStore = create<ViewerState>()((set) => ({
  open: null,
  show: (open) => set({ open }),
  close: () => set({ open: null }),
}));
