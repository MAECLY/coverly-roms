import { createContext, useContext, type PointerEvent } from 'react';

import type { CategoryRef, ExtraFileView } from '@/ipc/types';
import type { SelectMode } from '@/stores/extrasStore';

/**
 * What the Extras section hands its bands and rows for the panel CRUD
 * (extras spec §8.3): the writes (each one journaled job with undo), the
 * selection, which row is being renamed or tagged, and the pointer drag.
 * Rows name files by the listing's `index` (D22), never by path.
 */
export interface ExtrasActions {
  /** The section may write now; play and view never depend on it. */
  canWrite: boolean;
  /** Where a file can go, in the section's order. */
  targets: readonly CategoryRef[];
  selection: ReadonlySet<number>;
  renaming: number | null;
  tagsFor: number | null;
  /** The game's tags, offered in the tags popover. */
  suggestions: readonly string[];
  /** The path of a row that just landed (a move): briefly outlined. */
  landed: string | null;
  select(index: number, mode: SelectMode): void;
  clearSelection(): void;
  startRename(index: number | null): void;
  /** Renames to `stem`; answers the refusal to show under the input, or
   * null when it is done. */
  rename(file: ExtraFileView, stem: string): Promise<string | null>;
  move(files: number[], to: CategoryRef): void;
  /** `Alt+↑/↓`: to the previous or next category. */
  step(file: ExtraFileView, delta: -1 | 1): void;
  /** Straight to the Trash for one file, else after a confirmation. */
  trash(files: number[]): void;
  openTags(index: number | null): void;
  saveTags(file: ExtraFileView, tags: string[]): void;
  /** A pointer pressed on a row: may become a drag onto another band. */
  pointerDown(e: PointerEvent<HTMLElement>, file: ExtraFileView): void;
  /** True once, right after a drag ended: the click that follows it is
   * not a selection. */
  swallowClick(): boolean;
  renameCategory(from: string, to: string): Promise<string | null>;
  removeCategory(name: string, count: number): void;
}

const NONE: ExtrasActions = {
  canWrite: false,
  targets: [],
  selection: new Set(),
  renaming: null,
  tagsFor: null,
  suggestions: [],
  landed: null,
  select: () => undefined,
  clearSelection: () => undefined,
  startRename: () => undefined,
  rename: () => Promise.resolve(null),
  move: () => undefined,
  step: () => undefined,
  trash: () => undefined,
  openTags: () => undefined,
  saveTags: () => undefined,
  pointerDown: () => undefined,
  swallowClick: () => false,
  renameCategory: () => Promise.resolve(null),
  removeCategory: () => undefined,
};

export const ExtrasActionsContext = createContext<ExtrasActions>(NONE);

export function useExtrasActions(): ExtrasActions {
  return useContext(ExtrasActionsContext);
}
