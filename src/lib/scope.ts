import type { EntryView } from '@/ipc/types';

export type DateBucket = 'week' | 'month' | 'year' | 'older';
export const DATE_BUCKETS: readonly DateBucket[] = [
  'week',
  'month',
  'year',
  'older',
];

/**
 * What the sidebar shows: one folder subtree, one system, one date bucket,
 * the add-ons without their game (addon views D13), or all.
 */
export type Scope =
  | { kind: 'all' }
  | { kind: 'orphans' }
  | { kind: 'system'; system: number }
  | { kind: 'folder'; path: string }
  | { kind: 'date'; bucket: DateBucket };

const DAY = 86_400;

export function dateBucket(mtime: number, now: number): DateBucket {
  const age = now - mtime;
  if (age < 7 * DAY) return 'week';
  if (age < 30 * DAY) return 'month';
  if (age < 365 * DAY) return 'year';
  return 'older';
}

export function inScope(e: EntryView, scope: Scope, now: number): boolean {
  switch (scope.kind) {
    case 'all':
      return true;
    case 'orphans':
      return e.orphan !== undefined;
    case 'system':
      return e.system === scope.system;
    case 'folder':
      return (
        scope.path === '' ||
        e.dir === scope.path ||
        e.dir.startsWith(`${scope.path}/`)
      );
    case 'date':
      return dateBucket(e.mtime, now) === scope.bucket;
  }
}

const ALL: Scope = Object.freeze({ kind: 'all' });

/**
 * The scope actually shown: «Complementos sin juego base» falls back to
 * all once the library has no orphan left (D13); every other scope is
 * kept as it is.
 */
export function resolveScope(scope: Scope, orphans: number): Scope {
  return scope.kind === 'orphans' && orphans === 0 ? ALL : scope;
}

export function sameScope(a: Scope, b: Scope): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}
