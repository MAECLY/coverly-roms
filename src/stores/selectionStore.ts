import { create } from 'zustand';

interface SelectionState {
  selectedId: number | null;
  select(id: number | null): void;
}

export const useSelectionStore = create<SelectionState>()((set) => ({
  selectedId: null,
  select: (selectedId) => set({ selectedId }),
}));
