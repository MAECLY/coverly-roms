import type { EntryView, Region } from '@/ipc/types';

import type { Game } from './games';
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

/** The «Mostrar juegos que» chips (addon views D14). */
export type ContentKey = 'updates' | 'missing' | 'dlc' | 'demos';

export const CONTENT_KEYS: readonly ContentKey[] = [
  'updates',
  'missing',
  'dlc',
  'demos',
];

/** Each chip pressed or not; pressed chips combine with AND. */
export type ContentFilters = Readonly<Record<ContentKey, boolean>>;

/**
 * The 3.1 seam (spec §10): true when an offline title list knows a version
 * above the game's installed one. Rust decides it with `version_order`
 * (TypeScript never orders versions); there is no title list yet, so the
 * library passes none and «Le falta la última actualización» compares
 * only within the library (`Game.missing`, D6).
 */
export type BehindLatest = (game: Game) => boolean;

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
  /** Content chips; they work on games, not on scan entries. */
  content: ContentFilters;
}

export const NO_CONTENT: ContentFilters = {
  updates: false,
  missing: false,
  dlc: false,
  demos: false,
};

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
  content: NO_CONTENT,
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

/** Whether `game` has what the chip `key` names. */
export function matchesContentKey(
  game: Game,
  key: ContentKey,
  behind?: BehindLatest,
): boolean {
  switch (key) {
    case 'updates':
      return game.updates.length > 0;
    case 'missing':
      return game.missing !== null || (behind?.(game) ?? false);
    case 'dlc':
      return game.dlc.length > 0;
    case 'demos':
      return game.isDemo;
  }
}

/** Any content chip pressed. */
export function contentActive(content: ContentFilters): boolean {
  return CONTENT_KEYS.some((k) => content[k]);
}

/**
 * The games that pass every pressed content chip (AND). The panel filters
 * and the search run on the games' entries (`applyFilters`), so both
 * combine with AND too. Returns `games` itself when no chip is pressed.
 */
export function applyGameFilters(
  games: readonly Game[],
  filters: Pick<Filters, 'content'>,
  behind?: BehindLatest,
): readonly Game[] {
  const on = CONTENT_KEYS.filter((k) => filters.content[k]);
  if (on.length === 0) return games;
  return games.filter((g) => on.every((k) => matchesContentKey(g, k, behind)));
}

/** How many of `games` each chip would keep on its own; orphans never. */
export function contentFacets(
  games: readonly Game[],
  behind?: BehindLatest,
): Record<ContentKey, number> {
  const out: Record<ContentKey, number> = {
    updates: 0,
    missing: 0,
    dlc: 0,
    demos: 0,
  };
  for (const g of games) {
    if (g.orphan !== null) continue;
    for (const k of CONTENT_KEYS)
      if (matchesContentKey(g, k, behind)) out[k] += 1;
  }
  return out;
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

/** Filter-panel filters set (not the search query, not the content
 * chips, which show their own pressed state). */
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
