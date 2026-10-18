import type { Section } from './grouping';

/**
 * The index to select after `key` in a list/grid of `count` items laid out
 * in `columns` columns with `pageRows` rows per page, or null when the key
 * does not move the selection. `index` is -1 when nothing is selected.
 */
export function nextIndex(
  key: string,
  index: number,
  count: number,
  columns: number,
  pageRows: number,
): number | null {
  if (count === 0) return null;
  const last = count - 1;
  const clamp = (i: number) => Math.min(last, Math.max(0, i));
  if (index < 0) {
    if (key === 'End') return last;
    return [
      'ArrowRight',
      'ArrowLeft',
      'ArrowDown',
      'ArrowUp',
      'Home',
      'PageDown',
      'PageUp',
    ].includes(key)
      ? 0
      : null;
  }
  switch (key) {
    case 'ArrowRight':
      return clamp(index + 1);
    case 'ArrowLeft':
      return clamp(index - 1);
    case 'ArrowDown':
      return clamp(index + columns);
    case 'ArrowUp':
      return clamp(index - columns);
    case 'PageDown':
      return clamp(index + columns * pageRows);
    case 'PageUp':
      return clamp(index - columns * pageRows);
    case 'Home':
      return 0;
    case 'End':
      return last;
    default:
      return null;
  }
}

/**
 * ⌥↑ / ⌥↓: the first item of the previous / next non-empty section, or null
 * when there is none. With no selection (`index` -1) ⌥↓ goes to the first
 * section.
 */
export function groupJump(
  dir: -1 | 1,
  index: number,
  sections: readonly Section[],
): number | null {
  const filled = sections.filter((s) => s.count > 0);
  if (index < 0) return dir === 1 ? (filled[0]?.start ?? null) : null;
  const here = filled.findIndex(
    (s) => index >= s.start && index < s.start + s.count,
  );
  if (here < 0) return null;
  return filled[here + dir]?.start ?? null;
}

export interface TypeAhead {
  /** Adds `ch` typed at `now` (ms) and returns the whole buffer. */
  push(ch: string, now: number): string;
  /** The buffer still open at `now` (typed within `ms`), else ''. */
  pending(now: number): string;
}

/** Finder-style type-ahead: the buffer resets after `ms` of silence. */
export function createTypeAhead(ms = 500): TypeAhead {
  let buffer = '';
  let last = Number.NEGATIVE_INFINITY;
  return {
    push(ch, now) {
      buffer = now - last > ms ? ch : buffer + ch;
      last = now;
      return buffer;
    },
    pending(now) {
      return now - last > ms ? '' : buffer;
    },
  };
}

/** Lower case without diacritics, so "É" matches "e". */
function fold(s: string): string {
  return s.normalize('NFD').replace(/\p{M}/gu, '').toLocaleLowerCase();
}

/**
 * The next index whose name starts with `prefix` (case and diacritics
 * ignored), searching from `from + 1` and wrapping; `from` itself only when
 * nothing else matches. Null when nothing matches.
 */
export function findByPrefix(
  names: (i: number) => string,
  count: number,
  from: number,
  prefix: string,
): number | null {
  const want = fold(prefix);
  if (count === 0 || want === '') return null;
  const start = from < 0 ? 0 : from + 1;
  for (let k = 0; k < count; k++) {
    const i = (start + k) % count;
    if (fold(names(i)).startsWith(want)) return i;
  }
  return null;
}

/** Shelves ↑↓: the index in `targetLefts` (screen x of each candidate)
 *  nearest to `left`; the first on a tie, -1 with no candidates. */
export function nearestByLeft(
  left: number,
  targetLefts: readonly number[],
): number {
  let best = -1;
  let bestDistance = Number.POSITIVE_INFINITY;
  targetLefts.forEach((x, i) => {
    const d = Math.abs(x - left);
    if (d < bestDistance) {
      best = i;
      bestDistance = d;
    }
  });
  return best;
}

/**
 * Columns: the pane ←/→ moves focus to, or null at the ends. → only goes on
 * from a branch (a system or a folder), as in Finder.
 */
export function paneMove(
  key: 'ArrowLeft' | 'ArrowRight',
  pane: number,
  panes: number,
  onBranch: boolean,
): number | null {
  if (key === 'ArrowLeft') return pane > 0 ? pane - 1 : null;
  return onBranch && pane < panes - 1 ? pane + 1 : null;
}
