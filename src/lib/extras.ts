/**
 * Pure helpers of a game's Extras/ section (part D): the type of an
 * extension and the category «Añadir» suggests (both the same tables as
 * `coverly_core::extras`), category labels, keys and order, the tag rule
 * (D20, checked against Rust through `contract.requests.json`) and the
 * tag/text filter of the section.
 */
import type { TFunction } from '@/i18n/useT';
import {
  EXTRAS_CATEGORIES,
  type CategoryRef,
  type ExtraFileView,
  type ExtraFormat,
  type ExtrasCategory,
  type ExtraType,
} from '@/ipc/types';

/** Longest tag, in characters (Rust `MAX_TAG_CHARS`). */
export const MAX_TAG_CHARS = 64;
/** Most tags on one file (Rust `MAX_TAGS`). */
export const MAX_TAGS = 32;

const AUDIO = new Set([
  'mp3',
  'm4a',
  'aac',
  'wav',
  'flac',
  'ogg',
  'opus',
  'spc',
  'nsf',
  'nsfe',
  'gbs',
  'vgm',
  'vgz',
  'psf',
  'minipsf',
  'psf2',
  'gsf',
  'minigsf',
  'usf',
  'miniusf',
  '2sf',
  'mini2sf',
  'ssf',
  'dsf',
  'qsf',
]);
const VIDEO = new Set(['mp4', 'm4v', 'webm', 'mov', 'mkv', 'avi']);
const IMAGE = new Set([
  'png',
  'jpg',
  'jpeg',
  'gif',
  'webp',
  'avif',
  'bmp',
  'svg',
  'heic',
  'tif',
  'tiff',
]);
const DOCUMENT = new Set(['pdf', 'txt', 'md', 'nfo', 'cbz', 'epub']);
const PACKAGES = new Set(['p3t', 'pkg', 'ptf', 'cia']);
const MANUAL_WORDS = ['manual', 'manuel', 'handbuch', 'manuale', '説明書'];

/** The NFC, lower-case key names and tags are compared by (Rust `name_key`). */
export function nameKey(s: string): string {
  return s.normalize('NFC').toLowerCase();
}

/** The extension of a file name, lower case, without the dot; a dot file
 * (`.hidden`) has none, like Rust's `Path::extension`. */
export function extOf(name: string): string {
  const dot = name.lastIndexOf('.');
  return dot > 0 ? name.slice(dot + 1).toLowerCase() : '';
}

function stemOf(name: string): string {
  const dot = name.lastIndexOf('.');
  return dot > 0 ? name.slice(0, dot) : name;
}

/** The type of a file with extension `ext` (no dot, any case). */
export function typeOf(ext: string): ExtraType {
  const e = ext.toLowerCase();
  if (AUDIO.has(e)) return 'audio';
  if (VIDEO.has(e)) return 'video';
  if (IMAGE.has(e)) return 'image';
  if (DOCUMENT.has(e)) return 'document';
  return 'other';
}

/** The type of a file by its name. */
export function typeOfName(name: string): ExtraType {
  return typeOf(extOf(name));
}

/** The category «Añadir» suggests for a file name (D13). */
export function suggestedCategory(name: string): ExtrasCategory {
  const ext = extOf(name);
  if (PACKAGES.has(ext)) return 'themes';
  switch (typeOf(ext)) {
    case 'audio':
      return 'soundtrack';
    case 'video':
      return 'videos';
    case 'image':
      return 'artwork';
    case 'document':
      if (ext === 'pdf') {
        const stem = nameKey(stemOf(name));
        return MANUAL_WORDS.some((w) => stem.includes(w))
          ? 'manuals'
          : 'guides';
      }
      return 'guides';
    default:
      return 'guides';
  }
}

/** What a category is called in the section. */
export function categoryLabel(ref: CategoryRef, t: TFunction): string {
  switch (ref.kind) {
    case 'fixed':
      return t(`extras.cat.${ref.name}`);
    case 'user':
      return ref.name;
    case 'root':
      return t('extras.cat.root');
  }
}

/** Whether two categories are the same (user names exactly, as on disk). */
export function sameCategory(a: CategoryRef, b: CategoryRef): boolean {
  if (a.kind === 'root' || b.kind === 'root') return a.kind === b.kind;
  return a.kind === b.kind && a.name === b.name;
}

/** A string key for a category (maps, React keys, DOM ids). */
export function categoryKey(ref: CategoryRef): string {
  return ref.kind === 'root' ? 'root' : `${ref.kind}:${ref.name}`;
}

/** A band of the section: a category or the «No encontrados» band. */
export type BandRef = CategoryRef | { kind: 'missing' };

const collator = new Intl.Collator(undefined, {
  sensitivity: 'base',
  numeric: true,
});

function rank(ref: BandRef): number {
  switch (ref.kind) {
    case 'fixed':
      return EXTRAS_CATEGORIES.indexOf(ref.name);
    case 'user':
      return EXTRAS_CATEGORIES.length;
    case 'root':
      return EXTRAS_CATEGORIES.length + 1;
    case 'missing':
      return EXTRAS_CATEGORIES.length + 2;
  }
}

/** The section's order: the fixed categories in their order, the user's
 * A→Z, the files loose in `Extras/`, then «No encontrados». */
export function sortCategories<T extends { category: BandRef }>(
  items: readonly T[],
): T[] {
  return [...items].sort((a, b) => {
    const d = rank(a.category) - rank(b.category);
    if (d !== 0) return d;
    if (a.category.kind === 'user' && b.category.kind === 'user')
      return (
        collator.compare(a.category.name, b.category.name) ||
        (a.category.name < b.category.name ? -1 : 1)
      );
    return 0;
  });
}

const OFFICIAL_WHAT: Partial<Record<ExtraFormat, string>> = {
  'ps3-p3t': 'ps3Theme',
  'ps3-theme-pkg': 'ps3Theme',
  'ps3-avatar-pkg': 'ps3Avatar',
  'vita-theme-pkg': 'vitaTheme',
  'psp-ptf': 'pspTheme',
  'x360-theme': 'x360Theme',
  'x360-gamerpic': 'x360Gamerpic',
  'x360-avatar': 'x360Avatar',
  'x360-video': 'x360Video',
  '3ds-soundtrack-cia': 'soundtrack3ds',
};

/** The copy key naming what an official extra is («tema de PS3»); null
 * for a music rip, which is the user's. */
export function officialWhatKey(format: ExtraFormat): string | null {
  const what = OFFICIAL_WHAT[format];
  return what ? `extras.what.${what}` : null;
}

/** Why a tag list is refused (Rust `TagRefusal`). */
export type TagRefusal = 'empty' | 'too-long' | 'too-many';

export type TagsResult =
  { ok: true; tags: string[] } | { ok: false; refused: TagRefusal };

/** Tags trimmed, NFC, deduplicated ignoring case (the first spelling
 * wins) and capped (D20) — the Rust rule, so the panel refuses before it
 * asks. */
export function normalizeTags(raw: readonly string[]): TagsResult {
  const out: string[] = [];
  const keys = new Set<string>();
  for (const r of raw) {
    const t = r.trim().normalize('NFC');
    if (t === '') return { ok: false, refused: 'empty' };
    if ([...t].length > MAX_TAG_CHARS)
      return { ok: false, refused: 'too-long' };
    const key = t.toLowerCase();
    if (!keys.has(key)) {
      keys.add(key);
      out.push(t);
    }
  }
  if (out.length > MAX_TAGS) return { ok: false, refused: 'too-many' };
  return { ok: true, tags: out };
}

/** What the section filters by: chosen tags (all of them) and text. */
export interface ExtrasFilter {
  tags: readonly string[];
  text: string;
}

/** The files carrying every chosen tag whose name or tags hold the text,
 * both ignoring case; list order kept. */
export function filterFiles<T extends Pick<ExtraFileView, 'name' | 'tags'>>(
  files: readonly T[],
  filter: ExtrasFilter,
): T[] {
  const wanted = filter.tags.map(nameKey);
  const text = nameKey(filter.text.trim());
  if (wanted.length === 0 && text === '') return [...files];
  return files.filter((f) => {
    const tags = f.tags.map(nameKey);
    if (!wanted.every((w) => tags.includes(w))) return false;
    if (text === '') return true;
    return nameKey(f.name).includes(text) || tags.some((t) => t.includes(text));
  });
}

/** Every tag of a game's files once (first spelling), A→Z: the filter
 * chips and the tag suggestions. */
export function gameTags(
  files: readonly Pick<ExtraFileView, 'tags'>[],
): string[] {
  const seen = new Map<string, string>();
  for (const f of files)
    for (const t of f.tags) {
      const k = nameKey(t);
      if (!seen.has(k)) seen.set(k, t);
    }
  return [...seen.values()].sort(
    (a, b) => collator.compare(a, b) || (a < b ? -1 : 1),
  );
}
