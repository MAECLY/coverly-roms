import { useEffect, useRef, type KeyboardEvent } from 'react';

import type { EntryView } from '@/ipc/types';
import type { Section } from '@/lib/grouping';
import {
  createTypeAhead,
  findByPrefix,
  groupJump,
  typeAheadQuery,
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
  /** Whether entry `i` can hold the selection in this view (Shelves shows
   *  60 games a shelf): type-ahead skips the others. All by default. */
  canLand?(i: number): boolean;
  /** The view's size, as a key: when it changes (the details pane opens
   *  beside the view, the window is resized) the selection is brought
   *  back into view once the new layout is in place. */
  layout?: string;
}

/** A single printable character (not Space, which previews). */
function isPrintable(key: string): boolean {
  return key !== ' ' && [...key].length === 1;
}

/**
 * The keyboard model every view shares (spec §B.8), in this order: Enter
 * opens, Space previews (or, mid-word, continues the typed name), ⌥↑/⌥↓ jump between groups, a typed letter jumps to
 * the next matching name (500 ms buffer), anything else goes to the view's
 * `move`. Tab is never handled, so it always leaves the view.
 */
export function useViewKeys(
  o: ViewKeysOptions,
): (ev: KeyboardEvent<HTMLElement>) => void {
  const typeAhead = useRef<TypeAhead | null>(null);
  // The selection can move without a key here (Quick look's ←/→, the
  // drawer, a new sort): keep it in view so focus never lands off screen.
  const scrollTo = useRef(o.scrollToIndex);
  useEffect(() => {
    scrollTo.current = o.scrollToIndex;
  });
  const { selectedIndex, layout } = o;
  const lastLayout = useRef(layout);
  useEffect(() => {
    if (selectedIndex < 0) return;
    if (lastLayout.current === layout) {
      scrollTo.current(selectedIndex);
      return;
    }
    lastLayout.current = layout;
    // Wait a frame so the virtualiser has measured the new size.
    const frame = requestAnimationFrame(() => scrollTo.current(selectedIndex));
    return () => cancelAnimationFrame(frame);
  }, [selectedIndex, layout]);

  return (ev) => {
    // Keys pressed on a focusable child belong to that child.
    if (ev.target !== ev.currentTarget || ev.key === 'Tab') return;
    // An input method is still composing: the key is not a letter yet.
    if (ev.nativeEvent.isComposing) return;
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
    const now = Date.now();
    // Mid-word, Space is part of the name ("super m"), as in Finder.
    const typing =
      ev.key === ' ' && (typeAhead.current?.pending(now) ?? '') !== '';
    if (ev.key === ' ' && !typing) {
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
    if (
      (typing || isPrintable(ev.key)) &&
      !ev.ctrlKey &&
      !ev.metaKey &&
      !ev.altKey
    ) {
      typeAhead.current ??= createTypeAhead();
      const buffer = typeAhead.current.push(ev.key, now);
      const { from, prefix } = typeAheadQuery(buffer, o.selectedIndex);
      ev.preventDefault();
      go(
        findByPrefix(
          (i) =>
            o.canLand && !o.canLand(i)
              ? ''
              : titleWithoutRegions(o.entries[i]?.name ?? ''),
          o.entries.length,
          from,
          prefix,
        ),
      );
      return;
    }
    go(o.move(ev.key));
  };
}
