/**
 * A small window over a list of variable-height rows (the duplicate groups
 * of one system). Only the rows near the viewport, plus a margin, are in
 * the DOM; the rest are spacers sized from measured heights (an estimate
 * until a row has been seen). The row that holds focus and its neighbours
 * always stay mounted, so `Tab` walks the data, not the DOM window.
 *
 * Same idea as the `CoverGrid` virtualiser, without its fixed row height:
 * a group is as tall as its copies.
 */
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type RefObject,
} from 'react';

/** Height of a group not measured yet (a keeper and one tray row).
 * Measured on a two-copy group with a cover in the UI gallery at 1280 px
 * (`getBoundingClientRect`, 2026-10-27): 297–300 px, rounded up to 10 px. */
export const ESTIMATED_GROUP_PX = 300;
/** Rows this far beyond the viewport stay mounted. */
const MARGIN_PX = 400;

export interface WindowRange {
  /** Indices to render, ascending. */
  indices: number[];
  /** Top offset of every index, and the total height. */
  offsets: number[];
  total: number;
}

function scrollParent(el: HTMLElement | null): HTMLElement | null {
  for (let p = el?.parentElement ?? null; p; p = p.parentElement) {
    const { overflowY } = getComputedStyle(p);
    if (overflowY === 'auto' || overflowY === 'scroll') return p;
  }
  return null;
}

export function useGroupWindow(
  count: number,
  enabled: boolean,
): {
  listRef: RefObject<HTMLDivElement | null>;
  range: WindowRange;
  measure: (index: number, el: HTMLElement | null) => void;
  onFocusIndex: (index: number) => void;
  scrollToIndex: (index: number) => void;
} {
  const listRef = useRef<HTMLDivElement>(null);
  const [heights, setHeights] = useState<readonly number[]>([]);
  const observer = useRef<ResizeObserver | null>(null);
  const [viewport, setViewport] = useState<[number, number]>([
    0,
    typeof window === 'undefined' ? 800 : window.innerHeight,
  ]);
  const [focusIndex, setFocusIndex] = useState<number | null>(null);

  const offsets: number[] = new Array<number>(count);
  let total = 0;
  for (let i = 0; i < count; i++) {
    offsets[i] = total;
    total += heights[i] || ESTIMATED_GROUP_PX;
  }

  // Where the viewport is, in list coordinates.
  const update = useCallback(() => {
    const list = listRef.current;
    if (!list) return;
    const parent = scrollParent(list);
    const top = list.getBoundingClientRect().top;
    const [vTop, vBottom] = parent
      ? [
          parent.getBoundingClientRect().top,
          parent.getBoundingClientRect().bottom,
        ]
      : [0, window.innerHeight];
    const next: [number, number] = [vTop - top, vBottom - top];
    setViewport((prev) =>
      prev[0] === next[0] && prev[1] === next[1] ? prev : next,
    );
  }, []);

  useLayoutEffect(() => {
    if (!enabled) return;
    update();
    const parent = scrollParent(listRef.current);
    const target: HTMLElement | Window = parent ?? window;
    let frame = 0;
    const onScroll = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(update);
    };
    target.addEventListener('scroll', onScroll, { passive: true });
    window.addEventListener('resize', onScroll);
    return () => {
      cancelAnimationFrame(frame);
      target.removeEventListener('scroll', onScroll);
      window.removeEventListener('resize', onScroll);
    };
  }, [enabled, update]);

  const record = useCallback((el: Element) => {
    const index = Number((el as HTMLElement).dataset.windowIndex);
    const h = el.getBoundingClientRect().height;
    // Unlaid-out rows (0 px, as in tests) keep the estimate.
    if (!Number.isInteger(index) || h <= 0) return;
    setHeights((prev) => {
      if (Math.abs((prev[index] ?? 0) - h) <= 0.5) return prev;
      const next = [...prev];
      next[index] = h;
      return next;
    });
  }, []);

  // One observer for every mounted row: a group grows when its tray does.
  useEffect(() => {
    if (!enabled || typeof ResizeObserver === 'undefined') return;
    const ro = new ResizeObserver((entries) => {
      for (const e of entries) record(e.target);
    });
    observer.current = ro;
    return () => {
      ro.disconnect();
      observer.current = null;
    };
  }, [enabled, record]);

  const measure = useCallback(
    (index: number, el: HTMLElement | null) => {
      if (!el || !enabled) return;
      el.dataset.windowIndex = String(index);
      observer.current?.observe(el);
      record(el);
    },
    [enabled, record],
  );

  // Mount the row (as focus would) and bring its slot into view, nearest
  // edge first, so J/K reach a group the window has not rendered yet.
  const scrollToIndex = useCallback(
    (index: number) => {
      if (index < 0 || index >= count) return;
      setFocusIndex(index);
      const list = listRef.current;
      if (!list) return;
      let top = 0;
      for (let i = 0; i < index; i++) top += heights[i] || ESTIMATED_GROUP_PX;
      const bottom = top + (heights[index] || ESTIMATED_GROUP_PX);
      const parent = scrollParent(list);
      const listTop = list.getBoundingClientRect().top;
      const [vTop, vBottom] = parent
        ? [
            parent.getBoundingClientRect().top,
            parent.getBoundingClientRect().bottom,
          ]
        : [0, window.innerHeight];
      // Where the slot is relative to the viewport's top edge.
      const from = listTop + top - vTop;
      const to = listTop + bottom - vTop;
      const span = vBottom - vTop;
      const delta = from < 0 ? from : to > span ? Math.min(from, to - span) : 0;
      if (delta !== 0) {
        const scroller =
          parent ?? document.scrollingElement ?? document.documentElement;
        scroller.scrollTop += delta;
      }
      update();
    },
    [count, heights, update],
  );

  let indices: number[];
  if (!enabled) {
    indices = Array.from({ length: count }, (_, i) => i);
  } else {
    const keep = new Set<number>();
    const from = viewport[0] - MARGIN_PX;
    const to = viewport[1] + MARGIN_PX;
    for (let i = 0; i < count; i++) {
      const top = offsets[i]!;
      const bottom = top + (heights[i] || ESTIMATED_GROUP_PX);
      if (bottom >= from && top <= to) keep.add(i);
      else if (top > to) break;
    }
    if (focusIndex !== null)
      for (const i of [focusIndex - 1, focusIndex, focusIndex + 1])
        if (i >= 0 && i < count) keep.add(i);
    indices = [...keep].sort((a, b) => a - b);
  }

  return {
    listRef,
    range: { indices, offsets, total },
    measure,
    onFocusIndex: setFocusIndex,
    scrollToIndex,
  };
}
