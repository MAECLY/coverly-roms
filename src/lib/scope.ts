import type { EntryView } from '@/ipc/types';

export type DateBucket = 'week' | 'month' | 'year' | 'older';
export const DATE_BUCKETS: readonly DateBucket[] = [
  'week',
  'month',
  'year',
  'older',
];

/** What the sidebar shows: one folder subtree, one system, one date bucket, or all. */
export type Scope =
  | { kind: 'all' }
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

export function sameScope(a: Scope, b: Scope): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}
