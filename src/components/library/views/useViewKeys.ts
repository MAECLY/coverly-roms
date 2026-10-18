import { useRef, type KeyboardEvent } from 'react';

import type { EntryView } from '@/ipc/types';
import type { Section } from '@/lib/grouping';
import {
  createTypeAhead,
  findByPrefix,
  groupJump,
  type TypeAhead,
} from '@/lib/keyboardNav';
import { titleWithoutRegions } from '@/lib/tileLabel';

export interface ViewKeysOptions {
  entries: readonly EntryView[];
  sections: readonly Section[];
  /** Index of the selected entry in `entries`, -1 for none. */
  selectedIndex: number;
  onSelect(id: number): void;
  onOpen(id: number): void;
  onPreview(id: number): void;
  /** Bring entry `i` into view (the view maps it to its own row). */
  scrollToIndex(i: number): void;
  /** The view's own geometry: the index `key` moves to, or null. */
  move(key: string): number | null;
}

/** A single printable character (not Space, which previews). */
function isPrintable(key: string): boolean {
  return key !== ' ' && [...key].length === 1;
}

/** "sss" is the same letter pressed again: cycle through "s…" names. */
function repeatsOneChar(buffer: string): boolean {
  const chars = [...buffer];
  return chars.length > 1 && chars.every((c) => c === chars[0]);
}

/**
 * The keyboard model every view shares (spec §B.8), in this order: Enter
 * opens, Space previews, ⌥↑/⌥↓ jump between groups, a typed letter jumps to
 * the next matching name (500 ms buffer), anything else goes to the view's
 * `move`. Tab is never handled, so it always leaves the view.
 */
export function useViewKeys(
  o: ViewKeysOptions,
): (ev: KeyboardEvent<HTMLElement>) => void {
  const typeAhead = useRef<TypeAhead | null>(null);

  return (ev) => {
    // Keys pressed on a focusable child belong to that child.
    if (ev.target !== ev.currentTarget || ev.key === 'Tab') return;
    const selected = o.entries[o.selectedIndex];
    const go = (next: number | null) => {
      const entry = next === null ? undefined : o.entries[next];
      if (next === null || !entry) return;
      ev.preventDefault();
      o.onSelect(entry.id);
      o.scrollToIndex(next);
    };

    if (ev.key === 'Enter') {
      if (selected) {
        ev.preventDefault();
        o.onOpen(selected.id);
      }
      return;
    }
    if (ev.key === ' ') {
      // Never scroll the view, even with nothing to preview.
      ev.preventDefault();
      if (selected) o.onPreview(selected.id);
      return;
    }
    if (ev.altKey && (ev.key === 'ArrowUp' || ev.key === 'ArrowDown')) {
      const next = groupJump(
        ev.key === 'ArrowUp' ? -1 : 1,
        o.selectedIndex,
        o.sections,
      );
      if (next === null) ev.preventDefault();
      go(next);
      return;
    }
    if (isPrintable(ev.key) && !ev.ctrlKey && !ev.metaKey && !ev.altKey) {
      typeAhead.current ??= createTypeAhead();
      const buffer = typeAhead.current.push(ev.key, Date.now());
      const cycling = [...buffer].length === 1 || repeatsOneChar(buffer);
      ev.preventDefault();
      go(
        findByPrefix(
          (i) => titleWithoutRegions(o.entries[i]?.name ?? ''),
          o.entries.length,
          // A longer prefix keeps the current match when it still fits.
          cycling ? o.selectedIndex : o.selectedIndex - 1,
          cycling ? ([...buffer][0] ?? '') : buffer,
        ),
      );
      return;
    }
    go(o.move(ev.key));
  };
}
