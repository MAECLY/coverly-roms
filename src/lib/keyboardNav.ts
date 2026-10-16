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
