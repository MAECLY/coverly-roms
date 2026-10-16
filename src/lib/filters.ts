import type { EntryView, Region } from '@/ipc/types';

import { queryTokens } from './search';

export type ArtFilter =
  | 'all'
  | 'missing-any'
  | 'missing-cover'
  | 'missing-screenshot'
  | 'missing-title'
  | 'complete';

export const ART_FILTERS: readonly ArtFilter[] = [
  'all',
  'missing-any',
  'missing-cover',
  'missing-screenshot',
  'missing-title',
  'complete',
];

export interface Filters {
  query: string;
  art: ArtFilter;
  /** System indexes; empty = any. */
  systems: readonly number[];
  exts: readonly string[];
  regions: readonly Region[];
  /** Inclusive bounds, Unix seconds. */
  modifiedFrom: number | null;
  modifiedTo: number | null;
  /** Inclusive bounds, bytes. */
  sizeMin: number | null;
  sizeMax: number | null;
}

export const EMPTY_FILTERS: Filters = {
  query: '',
  art: 'all',
  systems: [],
  exts: [],
  regions: [],
  modifiedFrom: null,
  modifiedTo: null,
  sizeMin: null,
  sizeMax: null,
};

export type FacetField = 'art' | 'systems' | 'exts' | 'regions';

/** Any media the layout supports is missing (unsupported never counts). */
export function isMissingAny(e: EntryView): boolean {
  return (
    e.art.cover === 'missing' ||
    e.art.screenshot === 'missing' ||
    e.art.title === 'missing'
  );
}

export function matchesArt(e: EntryView, art: ArtFilter): boolean {
  switch (art) {
    case 'all':
      return true;
    case 'missing-any':
      return isMissingAny(e);
    case 'missing-cover':
      return e.art.cover === 'missing';
    case 'missing-screenshot':
      return e.art.screenshot === 'missing';
    case 'missing-title':
      return e.art.title === 'missing';
    case 'complete':
      return !isMissingAny(e);
  }
}

/** Whether `e` passes every filter except `skip` (for facet counts). */
export function matches(
  e: EntryView,
  key: string,
  f: Filters,
  tokens: readonly string[],
  skip?: FacetField,
): boolean {
  if (tokens.length > 0 && !tokens.every((t) => key.includes(t))) return false;
  if (skip !== 'art' && !matchesArt(e, f.art)) return false;
  if (
    skip !== 'systems' &&
    f.systems.length > 0 &&
    !f.systems.includes(e.system)
  ) {
    return false;
  }
  if (skip !== 'exts' && f.exts.length > 0 && !f.exts.includes(e.ext))
    return false;
  if (
    skip !== 'regions' &&
    f.regions.length > 0 &&
    !e.regions.some((r) => f.regions.includes(r))
  ) {
    return false;
  }
  if (f.modifiedFrom !== null && e.mtime < f.modifiedFrom) return false;
  if (f.modifiedTo !== null && e.mtime > f.modifiedTo) return false;
  if (f.sizeMin !== null && e.size < f.sizeMin) return false;
  if (f.sizeMax !== null && e.size > f.sizeMax) return false;
  return true;
}

export function applyFilters(
  entries: readonly EntryView[],
  keys: readonly string[],
  f: Filters,
): EntryView[] {
  const tokens = queryTokens(f.query);
  return entries.filter((e) => matches(e, keys[e.id] ?? '', f, tokens));
}

export interface Facets {
  art: Record<ArtFilter, number>;
  systems: Map<number, number>;
  exts: Map<string, number>;
  regions: Map<Region, number>;
}

function bump<K>(map: Map<K, number>, key: K): void {
  map.set(key, (map.get(key) ?? 0) + 1);
}

/** Per-option counts; each facet ignores its own filter only. */
export function computeFacets(
  entries: readonly EntryView[],
  keys: readonly string[],
  f: Filters,
): Facets {
  const tokens = queryTokens(f.query);
  const art = Object.fromEntries(ART_FILTERS.map((a) => [a, 0])) as Record<
    ArtFilter,
    number
  >;
  const systems = new Map<number, number>();
  const exts = new Map<string, number>();
  const regions = new Map<Region, number>();
  for (const e of entries) {
    const key = keys[e.id] ?? '';
    if (matches(e, key, f, tokens, 'art')) {
      for (const a of ART_FILTERS) if (matchesArt(e, a)) art[a] += 1;
    }
    if (matches(e, key, f, tokens, 'systems')) bump(systems, e.system);
    if (matches(e, key, f, tokens, 'exts')) bump(exts, e.ext);
    if (matches(e, key, f, tokens, 'regions')) {
      for (const r of e.regions) bump(regions, r);
    }
  }
  return { art, systems, exts, regions };
}

/** Filters set besides the search query. */
export function activeFilterCount(f: Filters): number {
  return [
    f.art !== 'all',
    f.systems.length > 0,
    f.exts.length > 0,
    f.regions.length > 0,
    f.modifiedFrom !== null || f.modifiedTo !== null,
    f.sizeMin !== null || f.sizeMax !== null,
  ].filter(Boolean).length;
}

function localSeconds(date: string, time: string): number | null {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(date)) return null;
  const ms = new Date(`${date}T${time}`).getTime();
  return Number.isNaN(ms) ? null : Math.floor(ms / 1000);
}

/** `YYYY-MM-DD` (local) → first second of that day. */
export function dayStart(date: string): number | null {
  return localSeconds(date, '00:00:00');
}

/** `YYYY-MM-DD` (local) → last second of that day. */
export function dayEnd(date: string): number | null {
  return localSeconds(date, '23:59:59');
}

/** Unix seconds → `YYYY-MM-DD` in local time (`''` for null). */
export function toDateInput(secs: number | null): string {
  if (secs === null) return '';
  const d = new Date(secs * 1000);
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}
