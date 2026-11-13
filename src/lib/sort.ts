import type { EntryView } from '@/ipc/types';

import { compareVersions } from './versions';

export type SortKey =
  | 'library'
  | 'name'
  | 'system'
  | 'size'
  | 'mtime'
  /** Details' Versión column (addon views §8.4): display order only. */
  | 'version'
  /** Details' Contenido column: how much a game holds (updates, DLC,
   *  other revisions, demos). */
  | 'content';
export interface Sort {
  key: SortKey;
  dir: 'asc' | 'desc';
}
export const DEFAULT_SORT: Sort = { key: 'library', dir: 'asc' };

const collator = new Intl.Collator(undefined, {
  sensitivity: 'base',
  numeric: true,
});

/** A sorted copy; ties keep library (scan) order. `contentOf` counts
 *  what a game holds for Contenido (its games know its other revisions
 *  and demos); without it, the entry's updates and DLC. */
export function sortEntries(
  entries: readonly EntryView[],
  sort: Sort,
  systemNames: readonly string[],
  contentOf: (e: EntryView) => number = (e) => e.children?.length ?? 0,
): EntryView[] {
  if (sort.key === 'library') {
    return sort.dir === 'asc' ? [...entries] : [...entries].reverse();
  }
  const sign = sort.dir === 'asc' ? 1 : -1;
  if (sort.key === 'version') {
    // A game without a version stays last in both directions.
    const keyed = entries.map((e) => [e, versionKey(e)] as const);
    keyed.sort(
      ([a, ka], [b, kb]) =>
        (ka === undefined ? 1 : 0) - (kb === undefined ? 1 : 0) ||
        sign * compareVersions(ka, kb) ||
        collator.compare(a.name, b.name) ||
        a.id - b.id,
    );
    return keyed.map(([e]) => e);
  }
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
      case 'content':
        return contentOf(a) - contentOf(b);
      case 'mtime':
      case 'library':
      case 'version':
        return a.mtime - b.mtime;
    }
  };
  return [...entries].sort((a, b) => sign * compare(a, b) || a.id - b.id);
}

/** What the Versión column shows first: the installed version, the
 *  entry's own, or its revision. */
function versionKey(e: EntryView): string | undefined {
  return e.installed ?? e.version ?? e.revision;
}
