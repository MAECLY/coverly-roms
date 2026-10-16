import type { EntryView } from '@/ipc/types';

export type SortKey = 'library' | 'name' | 'system' | 'size' | 'mtime';
export interface Sort {
  key: SortKey;
  dir: 'asc' | 'desc';
}
export const DEFAULT_SORT: Sort = { key: 'library', dir: 'asc' };

const collator = new Intl.Collator(undefined, {
  sensitivity: 'base',
  numeric: true,
});

/** A sorted copy; ties keep library (scan) order. */
export function sortEntries(
  entries: readonly EntryView[],
  sort: Sort,
  systemNames: readonly string[],
): EntryView[] {
  if (sort.key === 'library') {
    return sort.dir === 'asc' ? [...entries] : [...entries].reverse();
  }
  const sign = sort.dir === 'asc' ? 1 : -1;
  const compare = (a: EntryView, b: EntryView): number => {
    switch (sort.key) {
      case 'name':
        return collator.compare(a.name, b.name);
      case 'system':
        return (
          collator.compare(
            systemNames[a.system] ?? '',
            systemNames[b.system] ?? '',
          ) || collator.compare(a.name, b.name)
        );
      case 'size':
        return a.size - b.size;
      case 'mtime':
      case 'library':
        return a.mtime - b.mtime;
    }
  };
  return [...entries].sort((a, b) => sign * compare(a, b) || a.id - b.id);
}
