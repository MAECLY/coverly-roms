/**
 * Library view preferences: which view is shown and how, remembered per
 * library (Explorer's per-folder memory, scoped to a library root).
 *
 * Everything read back from storage is validated field by field against
 * whitelists; an unknown or broken value falls back to its default, so a
 * stale or hand-edited key can never break startup.
 */
import { MEDIA, type Media } from '@/ipc/types';

export type ViewId =
  'covers' | 'details' | 'columns' | 'gallery' | 'mounts' | 'shelves';
/** Switcher order (and ⌘1–⌘6). */
export const VIEW_IDS: readonly ViewId[] = [
  'covers',
  'details',
  'columns',
  'gallery',
  'mounts',
  'shelves',
];

/**
 * Views that can dock the inspector (spec §B.5). Columns and Gallery have
 * their own preview pane, so they have no Inspector switch and ⌘I.
 */
export const INSPECTOR_VIEWS: readonly ViewId[] = [
  'covers',
  'details',
  'mounts',
  'shelves',
];

/** Whether `prefs` dock the inspector in the view they show. */
export function inspectorDocked(prefs: ViewPrefs): boolean {
  return prefs.inspector && INSPECTOR_VIEWS.includes(prefs.view);
}

export type CoverSize = 'xs' | 's' | 'm' | 'l' | 'xl';
export const COVER_SIZE_IDS: readonly CoverSize[] = ['xs', 's', 'm', 'l', 'xl'];

export type GroupBy =
  'none' | 'system' | 'folder' | 'region' | 'art' | 'letter';
export const GROUP_BY_IDS: readonly GroupBy[] = [
  'none',
  'system',
  'folder',
  'region',
  'art',
  'letter',
];

export type Density = 'comfortable' | 'compact';
export const DENSITY_IDS: readonly Density[] = ['comfortable', 'compact'];

export type DetailsColumnId =
  | 'thumb'
  | 'name'
  | 'system'
  | 'folder'
  | 'regions'
  | 'ext'
  | 'size'
  | 'mtime'
  | 'cover'
  | 'title'
  | 'screenshot';
export const DETAILS_COLUMN_IDS: readonly DetailsColumnId[] = [
  'thumb',
  'name',
  'system',
  'folder',
  'regions',
  'ext',
  'size',
  'mtime',
  'cover',
  'title',
  'screenshot',
];

export interface DetailsColumn {
  id: DetailsColumnId;
  /** User-set width in px; absent means the column's default. */
  width?: number;
}

export interface ViewPrefs {
  view: ViewId;
  /** Covers, Shelves. */
  coverSize: CoverSize;
  /** Views that group (spec §B.5). */
  groupBy: GroupBy;
  /** Covers, Shelves, Gallery. */
  art: Media;
  /** Details: visible columns, in order, with widths. */
  columns: DetailsColumn[];
  /** Details rows. */
  density: Density;
  /** Docked inspector (not in Columns or Gallery). */
  inspector: boolean;
}

const DEFAULT_COLUMNS: readonly DetailsColumnId[] = [
  'name',
  'system',
  'size',
  'mtime',
  'cover',
  'screenshot',
  'title',
];

export const DEFAULT_PREFS: ViewPrefs = Object.freeze({
  view: 'covers',
  coverSize: 'm',
  groupBy: 'none',
  art: 'cover',
  columns: Object.freeze(DEFAULT_COLUMNS.map((id) => Object.freeze({ id }))),
  density: 'comfortable',
  inspector: false,
}) as ViewPrefs;

export const PREFS_KEY = 'coverly.views.v1';
export const LEGACY_LAYOUT_KEY = 'coverly.layout';
export const MAX_LIBRARIES = 20;
/** Column widths outside this range are treated as unset. */
const MIN_WIDTH = 24;
const MAX_WIDTH = 2000;

export interface PrefsFile {
  last: ViewPrefs;
  byLibrary: Record<string, ViewPrefs>;
  /** Library roots, least recently used first. */
  order: string[];
}

function hasOwn(obj: object, key: string): boolean {
  return Object.prototype.hasOwnProperty.call(obj, key);
}

function isRecord(v: unknown): v is Record<string, unknown> {
  return typeof v === 'object' && v !== null && !Array.isArray(v);
}

function pick<T extends string>(
  allowed: readonly T[],
  value: unknown,
  fallback: T,
): T {
  return allowed.includes(value as T) ? (value as T) : fallback;
}

function defaultColumns(): DetailsColumn[] {
  return DEFAULT_COLUMNS.map((id) => ({ id }));
}

function sanitiseColumns(raw: unknown): DetailsColumn[] {
  if (!Array.isArray(raw)) return defaultColumns();
  const seen = new Set<DetailsColumnId>();
  const out: DetailsColumn[] = [];
  for (const item of raw) {
    if (!isRecord(item)) continue;
    const id = item.id as DetailsColumnId;
    if (!DETAILS_COLUMN_IDS.includes(id) || seen.has(id)) continue;
    seen.add(id);
    const col: DetailsColumn = { id };
    const w = item.width;
    if (typeof w === 'number' && Number.isFinite(w)) {
      const px = Math.round(w);
      if (px >= MIN_WIDTH && px <= MAX_WIDTH) col.width = px;
    }
    out.push(col);
  }
  // The name is what identifies a row: it can be moved, never hidden.
  if (!seen.has('name')) out.unshift({ id: 'name' });
  return out;
}

/** Field-by-field validation; unknown values → defaults; `name` always kept first-present. */
export function sanitisePrefs(raw: unknown): ViewPrefs {
  const r = isRecord(raw) ? raw : {};
  return {
    view: pick(VIEW_IDS, r.view, DEFAULT_PREFS.view),
    coverSize: pick(COVER_SIZE_IDS, r.coverSize, DEFAULT_PREFS.coverSize),
    groupBy: pick(GROUP_BY_IDS, r.groupBy, DEFAULT_PREFS.groupBy),
    art: pick(MEDIA, r.art, DEFAULT_PREFS.art),
    columns: sanitiseColumns(r.columns),
    density: pick(DENSITY_IDS, r.density, DEFAULT_PREFS.density),
    inspector:
      typeof r.inspector === 'boolean' ? r.inspector : DEFAULT_PREFS.inspector,
  };
}

function emptyFile(last: ViewPrefs): PrefsFile {
  return { last, byLibrary: {}, order: [] };
}

function getItem(storage: Storage | undefined, key: string): string | null {
  try {
    return storage?.getItem(key) ?? null;
  } catch {
    return null;
  }
}

function parseFile(raw: unknown): PrefsFile {
  if (!isRecord(raw)) return emptyFile(sanitisePrefs(undefined));
  const byLibrary: Record<string, ViewPrefs> = {};
  const libs = isRecord(raw.byLibrary) ? raw.byLibrary : {};
  for (const [root, prefs] of Object.entries(libs))
    if (root !== '__proto__') byLibrary[root] = sanitisePrefs(prefs);
  // Known roots in their stored order, then any the order list missed.
  const order: string[] = [];
  if (Array.isArray(raw.order))
    for (const root of raw.order)
      if (
        typeof root === 'string' &&
        hasOwn(byLibrary, root) &&
        !order.includes(root)
      )
        order.push(root);
  for (const root of Object.keys(byLibrary))
    if (!order.includes(root)) order.unshift(root);
  return trim({ last: sanitisePrefs(raw.last), byLibrary, order });
}

/** Reads the prefs file; migrates `coverly.layout` when the file is missing. */
export function readPrefsFile(storage: Storage | undefined): PrefsFile {
  const stored = getItem(storage, PREFS_KEY);
  if (stored === null) {
    const legacy = getItem(storage, LEGACY_LAYOUT_KEY);
    return emptyFile({
      ...sanitisePrefs(undefined),
      view: legacy === 'list' ? 'details' : 'covers',
    });
  }
  try {
    return parseFile(JSON.parse(stored));
  } catch {
    return emptyFile(sanitisePrefs(undefined));
  }
}

/** Saves the prefs file; a failing or missing storage is ignored. */
export function writePrefsFile(
  storage: Storage | undefined,
  file: PrefsFile,
): void {
  try {
    storage?.setItem(PREFS_KEY, JSON.stringify(file));
  } catch {
    // Remembering the view is a convenience only.
  }
}

function trim(file: PrefsFile): PrefsFile {
  if (file.order.length <= MAX_LIBRARIES) return file;
  const drop = file.order.slice(0, file.order.length - MAX_LIBRARIES);
  const byLibrary = { ...file.byLibrary };
  for (const root of drop) delete byLibrary[root];
  return { ...file, byLibrary, order: file.order.slice(drop.length) };
}

/** Stores `prefs` for `root` and as `last`; keeps the 20 most recent libraries. */
export function remember(
  file: PrefsFile,
  root: string,
  prefs: ViewPrefs,
): PrefsFile {
  const clean = sanitisePrefs(prefs);
  return trim({
    last: clean,
    byLibrary: { ...file.byLibrary, [root]: clean },
    order: [...file.order.filter((r) => r !== root), root],
  });
}

/** The prefs a library opens with: its own, or the last ones used. */
export function prefsFor(file: PrefsFile, root: string): ViewPrefs {
  return sanitisePrefs(
    hasOwn(file.byLibrary, root) ? file.byLibrary[root] : file.last,
  );
}
