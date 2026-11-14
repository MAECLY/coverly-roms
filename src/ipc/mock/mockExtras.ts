/**
 * A game's `Extras/` in the browser mock (part D): listings by `rev`,
 * every write a job that `extrasUndo` reverts, adds from a mock picker or
 * a simulated OS drop (`emitDrop`), background copies with progress and
 * media the viewer can play. It mirrors the Rust semantics: the same
 * refusals and notes (`coverly_core::extras::ops`,
 * `src-tauri/src/library/extras.rs`), the same name rules and the same
 * ` (n)` suffix for copies and moves, never for a typed name.
 *
 * Only the Extras library (`?mockPick=extras`, `extrasFixture.ts`) has
 * game folders with state; any other entry answers an empty, read-only
 * `Extras/` and every write is refused.
 */
import { BackendError } from '../errors';
import type {
  AddChoice,
  AddFileView,
  AddFinishedEvent,
  AddPlanView,
  AddProgressEvent,
  AddStarted,
  CategoryRef,
  CategoryView,
  DroppedItem,
  EntryView,
  ExtraFileView,
  ExtraFormat,
  ExtrasCategory,
  ExtrasDropEvent,
  ExtrasMedia,
  ExtrasMediaKind,
  ExtrasNote,
  ExtrasTextView,
  ExtrasUndone,
  ExtrasView,
  ExtrasWriteView,
  ExtraType,
  IndexState,
  JobHistoryItem,
  LibraryView,
  OrganizeUndoView,
  Playable,
  Provenance,
  RipTags,
} from '../types';
import { EXTRAS_CATEGORIES } from '../types';
import { extrasGames, MOCK_EXTRAS, type SeedGame } from './extrasFixture';
import samplePdf from './media/sample.pdf';
import samplePng from './media/sample.png';
import sampleWebm from './media/sample.webm';
import { toneDataUrl } from './media/tone';

/** What the mock picker answers (`?mockExtrasPick=`, default `two`):
 * two audio files, 60 images (a background copy), 501 files (too many),
 * two files that do not fit, or a cancelled dialog. */
export type MockExtrasPick =
  'two' | 'many' | 'too-many' | 'no-space' | 'cancel';

const PICKS: readonly MockExtrasPick[] = [
  'two',
  'many',
  'too-many',
  'no-space',
  'cancel',
];

export function readMockExtrasPick(): MockExtrasPick {
  try {
    const v = new URLSearchParams(window.location.search).get('mockExtrasPick');
    return PICKS.find((p) => p === v) ?? 'two';
  } catch {
    return 'two';
  }
}

export interface MockExtrasHost {
  current(): LibraryView | null;
  /** The entry, or the error Rust answers for it. */
  entryOf(generation: number, id: number): EntryView;
  tick(): Promise<void>;
  /** Unix seconds the job stamps count from. */
  now: number;
  /** A write changed the listed file count of entry `id`. */
  counted(generation: number, id: number, extras: number): void;
}

export interface MockExtrasApi {
  extrasList(generation: number, entryId: number): Promise<ExtrasView>;
  extrasRename(
    generation: number,
    entryId: number,
    rev: number,
    index: number,
    stem: string,
  ): Promise<ExtrasWriteView>;
  extrasMove(
    generation: number,
    entryId: number,
    rev: number,
    files: number[],
    to: CategoryRef,
  ): Promise<ExtrasWriteView>;
  extrasTrash(
    generation: number,
    entryId: number,
    rev: number,
    files: number[],
  ): Promise<ExtrasWriteView>;
  extrasSetTags(
    generation: number,
    entryId: number,
    rev: number,
    index: number,
    tags: string[],
  ): Promise<ExtrasWriteView>;
  extrasForget(
    generation: number,
    entryId: number,
    rev: number,
    missing: number,
  ): Promise<ExtrasWriteView>;
  extrasCategoryCreate(
    generation: number,
    entryId: number,
    rev: number,
    name: string,
  ): Promise<ExtrasWriteView>;
  extrasCategoryRename(
    generation: number,
    entryId: number,
    rev: number,
    from: string,
    to: string,
  ): Promise<ExtrasWriteView>;
  extrasCategoryRemove(
    generation: number,
    entryId: number,
    rev: number,
    name: string,
  ): Promise<ExtrasWriteView>;
  extrasUndo(
    generation: number,
    entryId: number,
    job: string,
  ): Promise<ExtrasUndone>;
  extrasAddPick(
    generation: number,
    entryId: number,
  ): Promise<AddPlanView | null>;
  extrasAddPlan(
    generation: number,
    entryId: number,
    token: number,
  ): Promise<AddPlanView>;
  extrasAddCommit(
    generation: number,
    entryId: number,
    rev: number,
    plan: number,
    choices: AddChoice[],
  ): Promise<AddStarted>;
  extrasAddDropped(
    generation: number,
    entryId: number,
    rev: number,
    token: number,
    category: CategoryRef,
  ): Promise<AddStarted>;
  extrasAddCancel(add: number): Promise<boolean>;
  extrasMedia(
    generation: number,
    entryId: number,
    rev: number,
    index: number,
  ): Promise<ExtrasMedia>;
  extrasReadText(
    generation: number,
    entryId: number,
    rev: number,
    index: number,
  ): Promise<ExtrasTextView>;
  extrasReveal(
    generation: number,
    entryId: number,
    rev: number,
    index: number,
  ): Promise<void>;
  extrasOpenExternal(
    generation: number,
    entryId: number,
    rev: number,
    index: number,
  ): Promise<void>;
  onExtrasDrop(cb: (e: ExtrasDropEvent) => void): Promise<() => void>;
  onExtrasDragOver(
    cb: (point: { x: number; y: number } | null) => void,
  ): Promise<() => void>;
  onExtrasProgress(cb: (e: AddProgressEvent) => void): Promise<() => void>;
  onExtrasAdded(cb: (e: AddFinishedEvent) => void): Promise<() => void>;
  /** Simulates an OS drop Rust recorded (`extras://drop`); answers its
   * token. `position` is in CSS pixels, as the backend hands it over. */
  emitDrop(drop: {
    names: string[];
    position: { x: number; y: number };
  }): number;
  /** Simulates an OS drag over the window (CSS pixels); `null` leaves. */
  emitDragOver(point: { x: number; y: number } | null): void;
  /** Paths (relative to `Extras/`) revealed in the file manager, for tests. */
  readonly extrasRevealed: string[];
  /** Paths handed to another app, for tests. */
  readonly extrasOpened: string[];
  /** The listed file count of a game folder of the Extras library, by
   * entry `rel_path`; null for an entry without one. */
  extrasCount(relPath: string): number | null;
  /** Extras journals, newest first, for the jobs history. */
  extrasHistory(): JobHistoryItem[];
  ownsExtrasJob(job: string): boolean;
  /** The jobs history's undo of an Extras journal. */
  undoExtrasJob(job: string): OrganizeUndoView;
}

// ---- Names, types and categories (coverly_core::extras) ----

const DIR_NAMES: Record<ExtrasCategory, string> = {
  soundtrack: 'Soundtrack',
  manuals: 'Manuals',
  artwork: 'Artwork',
  videos: 'Videos',
  themes: 'Themes',
  guides: 'Guides',
};

const MAX_CATEGORY_CHARS = 64;
const MAX_TAG_CHARS = 64;
const MAX_TAGS = 32;
const MAX_NAME_BYTES = 255;
const WINDOWS_MAX_PATH = 259;
const BACKGROUND_FILES = 50;
const BACKGROUND_BYTES = 200_000_000;
const ADD_FILES = 500;
const ADD_BYTES = 20 * 1024 ** 3;
const DROP_KEEP = 4;
const DROP_MS = 2 * 60 * 1000;
const MANUAL_INSIDE = ['3ds', 'wiiu', 'switch', 'psvita'];

const AUDIO = [
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
];
const VIDEO = ['mp4', 'm4v', 'webm', 'mov', 'mkv', 'avi'];
const IMAGE = [
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
];
const DOCUMENT = ['pdf', 'txt', 'md', 'nfo', 'cbz', 'epub'];
const PACKAGES = ['p3t', 'pkg', 'ptf', 'cia'];
const MANUAL_WORDS = ['manual', 'manuel', 'handbuch', 'manuale', '説明書'];
const INLINE = [
  ...['mp3', 'm4a', 'aac', 'wav', 'flac', 'ogg', 'opus'],
  ...['mp4', 'm4v', 'webm', 'mov'],
  ...IMAGE,
  ...['pdf', 'txt', 'md', 'nfo'],
];
const EXTERNAL = [
  ...[
    'spc',
    'nsf',
    'nsfe',
    'gbs',
    'vgm',
    'vgz',
    'psf',
    'minipsf',
    'psf2',
    'minipsf2',
    'gsf',
    'minigsf',
    'usf',
    'miniusf',
    '2sf',
    'mini2sf',
    'ssf',
    'minissf',
    'dsf',
    'minidsf',
    'qsf',
    'miniqsf',
  ],
  'cbz',
  'epub',
];
const RIP_FORMATS: Record<string, ExtraFormat> = {
  spc: 'spc',
  nsf: 'nsf',
  nsfe: 'nsf',
  gbs: 'gbs',
  vgm: 'vgm',
  vgz: 'vgm',
  psf: 'psf',
  minipsf: 'psf',
  psf2: 'psf',
  gsf: 'psf',
  minigsf: 'psf',
  usf: 'psf',
  miniusf: 'psf',
  '2sf': 'psf',
  mini2sf: 'psf',
  ssf: 'psf',
  dsf: 'psf',
  qsf: 'psf',
};
const KEY_NAMES = [
  'prod.keys',
  'title.keys',
  'dev.keys',
  'aes_keys.txt',
  'seeddb.bin',
  'act.dat',
  'work.bin',
  'keys.txt',
  'otp.bin',
  'ticket.db',
  'eeprom.bin',
  'kv.bin',
];
const KEY_EXTS = ['rap', 'rif', 'dkey', 'zrif', 'tik', 'cetk', 'cik', 'edat'];
const LITTER = ['thumbs.db', 'desktop.ini', 'ehthumbs.db'];
const DOS_NAMES = [
  'CON',
  'PRN',
  'AUX',
  'NUL',
  ...Array.from({ length: 9 }, (_, i) => `COM${i + 1}`),
  ...Array.from({ length: 9 }, (_, i) => `LPT${i + 1}`),
];

function extOf(name: string): string {
  const dot = name.lastIndexOf('.');
  return dot > 0 ? name.slice(dot + 1).toLowerCase() : '';
}

function stemOf(name: string): string {
  const dot = name.lastIndexOf('.');
  return dot > 0 ? name.slice(0, dot) : name;
}

function typeOf(name: string): ExtraType {
  const e = extOf(name);
  if (AUDIO.includes(e)) return 'audio';
  if (VIDEO.includes(e)) return 'video';
  if (IMAGE.includes(e)) return 'image';
  if (DOCUMENT.includes(e)) return 'document';
  return 'other';
}

function nameKey(name: string): string {
  return name.normalize('NFC').toLowerCase();
}

function suggestedCategory(name: string): ExtrasCategory {
  const e = extOf(name);
  if (PACKAGES.includes(e)) return 'themes';
  switch (typeOf(name)) {
    case 'audio':
      return 'soundtrack';
    case 'video':
      return 'videos';
    case 'image':
      return 'artwork';
    case 'document':
      if (e === 'pdf') {
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

function isKeyMaterial(name: string): boolean {
  const lower = name.toLowerCase();
  if (KEY_NAMES.includes(lower)) return true;
  const dot = lower.lastIndexOf('.');
  return dot > 0 && KEY_EXTS.includes(lower.slice(dot + 1));
}

function isHidden(name: string): boolean {
  return name.startsWith('.') || LITTER.includes(name.toLowerCase());
}

function playableOf(name: string): Playable {
  if (isKeyMaterial(name)) return 'reveal-only';
  const e = extOf(name);
  if (INLINE.includes(e)) return 'inline';
  if (EXTERNAL.includes(e)) return 'external';
  return 'reveal-only';
}

function mediaOf(name: string): { kind: ExtrasMediaKind; mime: string } | null {
  const e = extOf(name);
  if (['mp3', 'm4a', 'aac', 'wav', 'flac', 'ogg', 'opus'].includes(e)) {
    // Every track plays the generated tone.
    return { kind: 'audio', mime: 'audio/wav' };
  }
  if (['mp4', 'm4v', 'webm', 'mov'].includes(e)) {
    return { kind: 'video', mime: 'video/webm' };
  }
  if (IMAGE.includes(e)) return { kind: 'image', mime: 'image/png' };
  if (e === 'pdf') return { kind: 'pdf', mime: 'application/pdf' };
  return null;
}

const MEDIA_URLS: Record<Exclude<ExtrasMediaKind, 'audio'>, string> = {
  video: sampleWebm,
  image: samplePng,
  pdf: samplePdf,
};

/** `write::safe_file_name`: what the name becomes on every system. */
function safeFileName(name: string): string | null {
  let s = [...name]
    .map((c) => (/[\p{Cc}<>:"/\\|?*]/u.test(c) ? '_' : c))
    .join('');
  s = s.replace(/[ .]+$/u, '');
  if (!s) return null;
  const base = (s.split('.')[0] ?? '').trimEnd().toUpperCase();
  if (DOS_NAMES.includes(base)) s = `_${s}`;
  return s;
}

function portable(name: string): boolean {
  return !/[/\\]/u.test(name) && safeFileName(name) === name;
}

function isReservedKey(key: string): boolean {
  return (
    key === 'extras.json' ||
    (key.startsWith('extras.broken-') && key.endsWith('.json'))
  );
}

const bytesOf = (s: string) => new TextEncoder().encode(s).length;

function refused(why: string): BackendError {
  return new BackendError('extras-refused', why);
}

function fixedOf(dir: string): ExtrasCategory | null {
  const lower = dir.toLowerCase();
  return (
    EXTRAS_CATEGORIES.find((c) => DIR_NAMES[c].toLowerCase() === lower) ?? null
  );
}

function validateCategoryName(raw: string, existingUser: string[]): string {
  const name = raw.trim().normalize('NFC');
  if (!name) throw refused('empty');
  if ([...name].length > MAX_CATEGORY_CHARS) throw refused('too-long');
  if (name.startsWith('.')) throw refused('dot');
  if (!portable(name)) throw refused('not-portable');
  const key = nameKey(name);
  if (isReservedKey(key)) throw refused('reserved');
  if (fixedOf(name) || existingUser.some((u) => nameKey(u) === key)) {
    throw refused('taken');
  }
  return name;
}

function validateFileStem(stem: string, ext: string, dirPath: string): string {
  const s = stem.trim().normalize('NFC');
  if (!s) throw refused('empty');
  const name = ext ? `${s}.${ext}` : s;
  if (bytesOf(name) > MAX_NAME_BYTES) throw refused('too-long');
  if (name.startsWith('.')) throw refused('dot');
  if (!portable(name)) throw refused('not-portable');
  if (isReservedKey(nameKey(name))) throw refused('reserved');
  if (`${dirPath}/${name}`.length > WINDOWS_MAX_PATH) {
    throw refused('path-too-long');
  }
  return name;
}

/** `normalize_tags`: trimmed, NFC, deduplicated ignoring case (the first
 * spelling wins) and capped (D20). */
function normalizeTags(raw: string[]): string[] {
  const out: string[] = [];
  for (const t0 of raw) {
    const t = t0.trim().normalize('NFC');
    if (!t) throw refused('tags-empty');
    if ([...t].length > MAX_TAG_CHARS) throw refused('tags-too-long');
    const key = t.toLowerCase();
    if (!out.some((o) => o.toLowerCase() === key)) out.push(t);
  }
  if (out.length > MAX_TAGS) throw refused('tags-too-many');
  return out;
}

/** `"{stem} ({n}).{ext}"`, the numbered name of a copy or a move. */
function numbered(name: string, n: number): string {
  const dot = name.lastIndexOf('.');
  return dot > 0
    ? `${name.slice(0, dot)} (${n}).${name.slice(dot + 1)}`
    : `${name} (${n})`;
}

function sameJson(a: unknown, b: unknown): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

/** Unix seconds as a job stamp (`20261114T120000Z`). */
function stamp(seconds: number): string {
  return new Date(seconds * 1000)
    .toISOString()
    .replace(/\.\d{3}Z$/u, 'Z')
    .replace(/[-:]/gu, '');
}

// ---- State ----

interface XFile {
  uid: number;
  /** Its folder in `Extras/` as spelled on disk; null: loose. */
  dir: string | null;
  name: string;
  provenance: Provenance;
  format: ExtraFormat | null;
  tags: string[];
  size: number;
  text?: string;
}

interface XMissing {
  uid: number;
  path: string;
  provenance: Provenance;
  tags: string[];
  size: number;
}

interface XGame {
  key: string;
  title: string;
  system: string;
  /** The game folder, relative to the library root. */
  folder: string;
  files: XFile[];
  missing: XMissing[];
  /** Folders in `Extras/` as spelled on disk. */
  dirs: string[];
  index: IndexState;
  readOnly: boolean;
  noTrash: boolean;
  rev: number;
  /** Uids by `index` of the last listing. */
  listed: number[];
  listedMissing: number[];
}

type Place = { dir: string | null; name: string };

type Op =
  | { k: 'add'; uid: number; at: Place }
  | { k: 'move'; uid: number; from: Place; to: Place }
  | { k: 'trash'; file: XFile }
  | { k: 'tags'; uid: number; before: string[]; after: string[] }
  | { k: 'forget'; rec: XMissing; at: number }
  | { k: 'dir-created'; name: string }
  | { k: 'dir-renamed'; from: string; to: string }
  | { k: 'dir-removed'; name: string };

interface Journal {
  job: string;
  game: XGame;
  entryId: number;
  createdAt: string;
  undone: boolean;
  ops: Op[];
  indexBefore: IndexState;
  indexAfter: IndexState;
}

interface Undid {
  removed: string[];
  restored: string[];
  kept: string[];
}

interface PlanFile {
  name: string;
  size: number;
  rip: RipTags | null;
}

interface Plan {
  generation: number;
  entryId: number;
  files: PlanFile[];
  keys: number;
  hidden: number;
}

const sameDir = (a: string | null, b: string | null) =>
  a === null || b === null ? a === b : nameKey(a) === nameKey(b);

/** Whether two refs are one category, whatever the spelling (D1). */
function sameCategory(a: CategoryRef, b: CategoryRef): boolean {
  if (a.kind === 'fixed' && b.kind === 'fixed') return a.name === b.name;
  if (a.kind === 'user' && b.kind === 'user') {
    return nameKey(a.name) === nameKey(b.name);
  }
  return a.kind === 'root' && b.kind === 'root';
}

const pathOf = (p: Place) => (p.dir === null ? p.name : `${p.dir}/${p.name}`);

function categoryOf(dir: string | null): CategoryRef {
  if (dir === null) return { kind: 'root' };
  const fixed = fixedOf(dir);
  return fixed ? { kind: 'fixed', name: fixed } : { kind: 'user', name: dir };
}

/** Fixed in their order, user by name ignoring case, loose last. */
function categoryRank(dir: string | null): [number, number, string] {
  if (dir === null) return [2, 0, ''];
  const fixed = fixedOf(dir);
  return fixed
    ? [0, EXTRAS_CATEGORIES.indexOf(fixed), '']
    : [1, 0, nameKey(dir)];
}

function compareRank(
  a: [number, number, string],
  b: [number, number, string],
): number {
  if (a[0] !== b[0]) return a[0] - b[0];
  if (a[1] !== b[1]) return a[1] - b[1];
  return a[2] < b[2] ? -1 : a[2] > b[2] ? 1 : 0;
}

function sortedFiles(files: readonly XFile[]): XFile[] {
  return [...files].sort((a, b) => {
    const r = compareRank(categoryRank(a.dir), categoryRank(b.dir));
    if (r !== 0) return r;
    const ka = nameKey(pathOf(a));
    const kb = nameKey(pathOf(b));
    return ka < kb ? -1 : ka > kb ? 1 : 0;
  });
}

function ripOf(name: string, game: string): RipTags | null {
  return RIP_FORMATS[extOf(name)]
    ? { game, title: stemOf(name), artist: 'Coverly Mock Ensemble' }
    : null;
}

function sizeFor(name: string): number {
  if (isKeyMaterial(name) || isHidden(name)) return 16;
  switch (typeOf(name)) {
    case 'audio':
      return RIP_FORMATS[extOf(name)] ? 66_048 : 6_000_000;
    case 'video':
      return 40_000_000;
    case 'image':
      return 2_000_000;
    case 'document':
      return 3_000_000;
    default:
      return 1_000_000;
  }
}

export function createMockExtras(
  host: MockExtrasHost,
  opts: { pick: MockExtrasPick },
): MockExtrasApi {
  const games = new Map<string, XGame>();
  const journals: Journal[] = [];
  const plans = new Map<number, Plan>();
  const drops = new Map<number, { items: DroppedItem[]; at: number }>();
  const dropListeners = new Set<(e: ExtrasDropEvent) => void>();
  const overListeners = new Set<
    (point: { x: number; y: number } | null) => void
  >();
  const progressListeners = new Set<(e: AddProgressEvent) => void>();
  const addedListeners = new Set<(e: AddFinishedEvent) => void>();
  const extrasRevealed: string[] = [];
  const extrasOpened: string[] = [];
  let uidSeq = 0;
  let revSeq = 0;
  let jobSeq = 0;
  let addSeq = 0;
  let dropSeq = 0;
  let busy: { add: number; cancel: boolean } | null = null;

  const listen = <T>(set: Set<(v: T) => void>, cb: (v: T) => void) => {
    set.add(cb);
    return Promise.resolve(() => {
      set.delete(cb);
    });
  };

  /** The seeds, made once per mock: edits survive a reopen. */
  const seeds = (() => {
    let made: Map<string, SeedGame> | null = null;
    return () => (made ??= extrasGames(host.now).seeds);
  })();

  function seeded(relPath: string, entry: EntryView | null): XGame | null {
    const known = games.get(relPath);
    if (known) return known;
    const seed = seeds().get(relPath);
    if (!seed) return null;
    const dirs: string[] = [...(seed.dirs ?? [])];
    const files = seed.files.map((f): XFile => {
      const slash = f.path.lastIndexOf('/');
      const dir = slash < 0 ? null : f.path.slice(0, slash);
      if (dir !== null && !dirs.some((d) => sameDir(d, dir))) dirs.push(dir);
      return {
        uid: (uidSeq += 1),
        dir,
        name: f.path.slice(slash + 1),
        provenance: f.provenance ?? 'user',
        format: f.format ?? null,
        tags: [...(f.tags ?? [])],
        size: f.size,
        ...(f.text !== undefined && { text: f.text }),
      };
    });
    const missing = (seed.missing ?? []).map((m): XMissing => ({
      uid: (uidSeq += 1),
      path: m.path,
      provenance: m.provenance ?? 'user',
      tags: [...(m.tags ?? [])],
      size: m.size,
    }));
    const slash = relPath.lastIndexOf('/');
    const g: XGame = {
      key: relPath,
      title: entry?.name ?? relPath,
      system: '',
      folder: relPath.slice(0, slash),
      files,
      missing,
      dirs,
      index:
        seed.index ??
        (files.length > 0 || missing.length > 0
          ? { state: 'ok' }
          : { state: 'absent' }),
      readOnly: seed.readOnly === true,
      noTrash: seed.noTrash === true,
      rev: 0,
      listed: [],
      listedMissing: [],
    };
    games.set(relPath, g);
    return g;
  }

  /** The state of entry `e`'s game folder in the Extras library. */
  function gameOf(e: EntryView): XGame | null {
    if (host.current()?.root !== MOCK_EXTRAS) return null;
    const g = seeded(e.rel_path, e);
    if (g) {
      g.title = e.name;
      g.system = host.current()?.systems[e.system]?.system_id ?? '';
    }
    return g;
  }

  // ---- Listings ----

  function categories(g: XGame): CategoryView[] {
    const count = (dir: string | null) =>
      g.files.filter((f) => sameDir(f.dir, dir)).length;
    const out: CategoryView[] = EXTRAS_CATEGORIES.map((c) => {
      const dir = g.dirs.find((d) => fixedOf(d) === c);
      return {
        category: { kind: 'fixed', name: c },
        present: dir !== undefined,
        count:
          dir === undefined
            ? 0
            : g.files.filter((f) => f.dir !== null && fixedOf(f.dir) === c)
                .length,
        link: false,
        sync: null,
      };
    });
    const users = g.dirs
      .filter((d) => !fixedOf(d))
      .sort((a, b) => (nameKey(a) < nameKey(b) ? -1 : 1));
    for (const d of users) {
      out.push({
        category: { kind: 'user', name: d },
        present: true,
        count: count(d),
        link: false,
        sync: null,
      });
    }
    const loose = count(null);
    if (loose > 0) {
      out.push({
        category: { kind: 'root' },
        present: true,
        count: loose,
        link: false,
        sync: null,
      });
    }
    return out;
  }

  function writability(g: XGame): ExtrasView['writable'] {
    if (g.index.state === 'newer') return { kind: 'newer' };
    if (g.readOnly) return { kind: 'read-only' };
    if (busy) return { kind: 'busy' };
    return { kind: 'yes' };
  }

  function view(g: XGame, entryId: number): ExtrasView {
    g.rev = revSeq += 1;
    const files = sortedFiles(g.files);
    g.listed = files.map((f) => f.uid);
    g.listedMissing = g.missing.map((m) => m.uid);
    return {
      entry_id: entryId,
      rev: g.rev,
      game_folder: true,
      index: g.index,
      writable: writability(g),
      categories: categories(g),
      files: files.map((f, index): ExtraFileView => ({
        index,
        category: categoryOf(f.dir),
        path: pathOf(f),
        name: f.name,
        kind: typeOf(f.name),
        provenance: f.provenance,
        format: f.format,
        tags: [...f.tags],
        size: f.size,
        link: false,
        playable: playableOf(f.name),
      })),
      missing: g.missing.map((m, index) => {
        const slash = m.path.lastIndexOf('/');
        const name = m.path.slice(slash + 1);
        return {
          index,
          category: categoryOf(slash < 0 ? null : m.path.slice(0, slash)),
          path: m.path,
          name,
          kind: typeOf(name),
          provenance: m.provenance,
          tags: [...m.tags],
          size: m.size,
        };
      }),
      capped: false,
      system: g.system,
      manual_inside: MANUAL_INSIDE.includes(g.system),
      trash: !g.noTrash,
    };
  }

  function bareView(e: EntryView, entryId: number): ExtrasView {
    const system = host.current()?.systems[e.system]?.system_id ?? '';
    return {
      entry_id: entryId,
      rev: 0,
      game_folder: false,
      index: { state: 'absent' },
      writable: { kind: 'read-only' },
      categories: [],
      files: [],
      missing: [],
      capped: false,
      system,
      manual_inside: MANUAL_INSIDE.includes(system),
      trash: false,
    };
  }

  // ---- Writes ----

  /** Entry `id`'s game folder once a write may run on listing `rev`, in
   * Rust's order: the one-writer guard, the revision, a game folder, a
   * newer index, a folder the app can write. */
  function writable(generation: number, id: number, rev: number): XGame {
    const e = host.entryOf(generation, id);
    if (busy) throw new BackendError('job-running');
    const g = gameOf(e);
    if (!g) throw new BackendError('write-denied', e.rel_path);
    if (rev !== g.rev) throw new BackendError('extras-changed');
    if (g.index.state === 'newer') throw new BackendError('extras-newer');
    if (g.readOnly) throw new BackendError('write-denied', g.folder);
    return g;
  }

  /** The file at `index` of the game's last listing. */
  function fileAt(g: XGame, index: number): XFile {
    const uid = g.listed[index];
    const f = g.files.find((x) => x.uid === uid);
    if (uid === undefined || !f) throw new BackendError('extras-changed');
    return f;
  }

  interface Job {
    ops: Op[];
    notes: ExtrasNote[];
    id(): string;
  }

  function begin(g: XGame): Job & { indexBefore: IndexState } {
    let id: string | null = null;
    return {
      ops: [],
      notes: [],
      indexBefore: g.index,
      id() {
        if (id === null) {
          jobSeq += 1;
          id = `${stamp(host.now + jobSeq)}-m0ck-${jobSeq.toString(16).padStart(4, '0')}`;
        }
        return id;
      },
    };
  }

  function recount(g: XGame, generation: number, entryId: number) {
    host.counted(generation, entryId, g.files.length);
  }

  /** Ends a job: the index written (a malformed one copied aside
   * first), the journal kept, the new listing answered. */
  function finish(
    g: XGame,
    generation: number,
    entryId: number,
    job: Job & { indexBefore: IndexState },
  ): ExtrasWriteView {
    if (job.ops.length === 0) {
      return { job: null, listing: view(g, entryId), notes: job.notes };
    }
    if (g.index.state === 'malformed') {
      const at = new Date(host.now * 1000).toISOString();
      const file = `extras.broken-${at.slice(0, 10).replace(/-/gu, '')}-${at
        .slice(11, 19)
        .replace(/:/gu, '')}.json`;
      job.notes.push({ kind: 'malformed-saved', file });
    }
    // A new object per write: undo tells a later write's index by identity.
    g.index = { state: 'ok' };
    const id = job.id();
    journals.push({
      job: id,
      game: g,
      entryId,
      createdAt: new Date((host.now + jobSeq) * 1000)
        .toISOString()
        .replace(/\.\d{3}Z$/u, 'Z'),
      undone: false,
      ops: job.ops,
      indexBefore: job.indexBefore,
      indexAfter: g.index,
    });
    recount(g, generation, entryId);
    return { job: id, listing: view(g, entryId), notes: job.notes };
  }

  const takenIn = (g: XGame, dir: string | null, name: string, uid = -1) =>
    g.files.some(
      (f) =>
        f.uid !== uid &&
        sameDir(f.dir, dir) &&
        nameKey(f.name) === nameKey(name),
    );

  /** Whether a copy may not take `name`: a file or a record holds it. */
  const heldIn = (g: XGame, dir: string | null, name: string) =>
    takenIn(g, dir, name) ||
    g.missing.some((m) => nameKey(m.path) === nameKey(pathOf({ dir, name })));

  function freeName(
    held: (name: string) => boolean,
    name: string,
    job: Job,
  ): string {
    if (!held(name)) return name;
    let n = 2;
    while (held(numbered(name, n))) n += 1;
    const next = numbered(name, n);
    job.notes.push({ kind: 'renamed-dup', name: next });
    return next;
  }

  const extrasPath = (g: XGame, dir: string | null) =>
    `${MOCK_EXTRAS}/${g.folder}/Extras${dir === null ? '' : `/${dir}`}`;

  function ensureDir(g: XGame, dir: string | null, job: Job) {
    if (dir === null || g.dirs.some((d) => sameDir(d, dir))) return;
    g.dirs.push(dir);
    job.ops.push({ k: 'dir-created', name: dir });
  }

  /** The folder a move or an add to `to` lands in: the category's exact
   * spelling on disk, else (fixed) its English name; a user category
   * must exist unless `create` validates and makes it. */
  function targetDir(
    g: XGame,
    to: CategoryRef,
    create: boolean,
  ): { dir: string | null; isNew: boolean } {
    if (to.kind === 'root') return { dir: null, isNew: false };
    if (to.kind === 'fixed') {
      const exact = DIR_NAMES[to.name];
      const dir =
        g.dirs.find((d) => d === exact) ??
        g.dirs.find((d) => fixedOf(d) === to.name);
      return dir ? { dir, isNew: false } : { dir: exact, isNew: true };
    }
    const dir = g.dirs.find((d) => !fixedOf(d) && sameDir(d, to.name));
    if (dir) return { dir, isNew: false };
    if (!create) throw new BackendError('extras-changed');
    const name = validateCategoryName(to.name, userNames(g));
    return { dir: name, isNew: true };
  }

  const userNames = (g: XGame) => g.dirs.filter((d) => !fixedOf(d));

  function trashOne(g: XGame, f: XFile, job: Job) {
    if (g.noTrash) {
      const note: ExtrasNote = {
        kind: 'set-aside',
        folder: `.coverly-trash/${job.id()}`,
      };
      if (!job.notes.some((n) => sameJson(n, note))) job.notes.push(note);
    }
    g.files = g.files.filter((x) => x.uid !== f.uid);
    job.ops.push({ k: 'trash', file: { ...f, tags: [...f.tags] } });
  }

  function write(
    generation: number,
    id: number,
    rev: number,
    act: (g: XGame, job: Job) => void,
  ): Promise<ExtrasWriteView> {
    try {
      const g = writable(generation, id, rev);
      const job = begin(g);
      act(g, job);
      return Promise.resolve(finish(g, generation, id, job));
    } catch (e) {
      return Promise.reject(e);
    }
  }

  // ---- Undo ----

  function undoOps(g: XGame, j: Journal): Undid {
    const out: Undid = { removed: [], restored: [], kept: [] };
    for (const op of [...j.ops].reverse()) {
      switch (op.k) {
        case 'add': {
          const f = g.files.find((x) => x.uid === op.uid);
          if (f && sameDir(f.dir, op.at.dir) && f.name === op.at.name) {
            g.files = g.files.filter((x) => x !== f);
            out.removed.push(pathOf(op.at));
          } else out.kept.push(pathOf(op.at));
          break;
        }
        case 'move': {
          const f = g.files.find((x) => x.uid === op.uid);
          if (
            f &&
            sameDir(f.dir, op.to.dir) &&
            f.name === op.to.name &&
            !takenIn(g, op.from.dir, op.from.name, f.uid)
          ) {
            if (
              op.from.dir !== null &&
              !g.dirs.some((d) => sameDir(d, op.from.dir))
            ) {
              g.dirs.push(op.from.dir);
            }
            f.dir = op.from.dir;
            f.name = op.from.name;
            out.restored.push(pathOf(op.from));
          } else out.kept.push(pathOf(op.to));
          break;
        }
        case 'trash': {
          const f = op.file;
          if (takenIn(g, f.dir, f.name)) {
            out.kept.push(pathOf(f));
            break;
          }
          if (f.dir !== null && !g.dirs.some((d) => sameDir(d, f.dir))) {
            g.dirs.push(f.dir);
          }
          g.files.push({ ...f, tags: [...f.tags] });
          out.restored.push(pathOf(f));
          break;
        }
        case 'tags': {
          const f = g.files.find((x) => x.uid === op.uid);
          if (f && sameJson(f.tags, op.after)) f.tags = [...op.before];
          break;
        }
        case 'forget':
          if (!g.missing.some((m) => m.uid === op.rec.uid)) {
            g.missing.splice(op.at, 0, op.rec);
          }
          break;
        case 'dir-created':
          if (!g.files.some((f) => sameDir(f.dir, op.name))) {
            g.dirs = g.dirs.filter((d) => d !== op.name);
          }
          break;
        case 'dir-renamed':
          if (g.dirs.includes(op.to) && !g.dirs.includes(op.from)) {
            g.dirs = g.dirs.map((d) => (d === op.to ? op.from : d));
            for (const f of g.files) if (f.dir === op.to) f.dir = op.from;
          }
          break;
        case 'dir-removed':
          if (!g.dirs.some((d) => sameDir(d, op.name))) g.dirs.push(op.name);
          break;
      }
    }
    // Like a file the undo finds changed since: an index another job wrote
    // after this one stays (each write makes a new state object).
    if (g.index === j.indexAfter) g.index = j.indexBefore;
    return out;
  }

  function journalOf(job: string): Journal | undefined {
    return journals.find((j) => j.job === job);
  }

  // ---- Adds ----

  function planView(plan: number, p: Plan): AddPlanView {
    const files = p.files.map((f, index): AddFileView => ({
      index,
      name: f.name,
      size: f.size,
      kind: typeOf(f.name),
      category: suggestedCategory(f.name),
      rip: f.rip,
    }));
    const total = p.files.reduce((s, f) => s + f.size, 0);
    return {
      plan,
      entry_id: p.entryId,
      files,
      total,
      keys: p.keys,
      hidden: p.hidden,
      too_many: p.files.length > ADD_FILES || total > ADD_BYTES,
    };
  }

  function keepPlan(
    generation: number,
    entryId: number,
    names: { name: string; size: number }[],
    game: string,
  ): AddPlanView {
    let keys = 0;
    let hidden = 0;
    const files: PlanFile[] = [];
    for (const n of names) {
      if (isKeyMaterial(n.name)) keys += 1;
      else if (isHidden(n.name)) hidden += 1;
      else files.push({ name: n.name, size: n.size, rip: ripOf(n.name, game) });
    }
    const plan = (addSeq += 1);
    const p: Plan = { generation, entryId, files, keys, hidden };
    plans.set(plan, p);
    return planView(plan, p);
  }

  /** The entry an add may target (a game folder of the open library). */
  function addEntry(generation: number, id: number): XGame {
    const e = host.entryOf(generation, id);
    const g = gameOf(e);
    if (!g) throw new BackendError('write-denied', e.rel_path);
    return g;
  }

  function takeDrop(token: number): DroppedItem[] {
    const d = drops.get(token);
    drops.delete(token);
    if (!d || Date.now() - d.at > DROP_MS) throw refused('drop-expired');
    return d.items;
  }

  interface Copy {
    name: string;
    size: number;
    to: CategoryRef;
  }

  /** Plans the copies (names checked, categories resolved) before any is
   * made; returns the step that copies file `i`. */
  function startCopies(g: XGame, job: Job, copies: Copy[]) {
    if (copies.length > ADD_FILES) throw refused('too-many');
    if (copies.reduce((s, c) => s + c.size, 0) > ADD_BYTES) {
      throw refused('too-many');
    }
    const planned = copies.map((c) => {
      const { dir, isNew } = targetDir(g, c.to, true);
      const name = safeFileName(c.name);
      if (name === null) throw refused('empty');
      if (dir === null && isReservedKey(nameKey(name))) {
        throw refused('reserved');
      }
      return { ...c, dir, isNew, safe: name };
    });
    const sanitised: string[] = [];
    const step = (i: number) => {
      const c = planned[i]!;
      ensureDir(g, c.dir, job);
      const name = freeName((n) => heldIn(g, c.dir, n), c.safe, job);
      if (c.safe !== c.name) sanitised.push(name);
      const f: XFile = {
        uid: (uidSeq += 1),
        dir: c.dir,
        name,
        provenance: 'user',
        format: RIP_FORMATS[extOf(name)] ?? null,
        tags: [],
        size: c.size,
      };
      g.files.push(f);
      job.ops.push({ k: 'add', uid: f.uid, at: { dir: c.dir, name } });
    };
    const end = (skipped: ExtrasNote[]) => {
      if (sanitised.length > 0) {
        job.notes.push({ kind: 'sanitised', names: sanitised });
      }
      job.notes.push(...skipped);
    };
    return { step, end, n: planned.length };
  }

  function skippedNotes(keys: number, hidden: number): ExtrasNote[] {
    return [
      ...(keys > 0 ? [{ kind: 'keys-skipped' as const, n: keys }] : []),
      ...(hidden > 0 ? [{ kind: 'hidden-skipped' as const, n: hidden }] : []),
    ];
  }

  function runAdd(
    generation: number,
    entryId: number,
    g: XGame,
    copies: Copy[],
    skipped: ExtrasNote[],
  ): AddStarted {
    const job = begin(g);
    const run = startCopies(g, job, copies);
    const total = copies.reduce((s, c) => s + c.size, 0);
    if (run.n <= BACKGROUND_FILES && total <= BACKGROUND_BYTES) {
      for (let i = 0; i < run.n; i += 1) run.step(i);
      run.end(skipped);
      return {
        kind: 'done',
        write: finish(g, generation, entryId, job),
      };
    }
    const add = (addSeq += 1);
    const state = { add, cancel: false };
    busy = state;
    void (async () => {
      let done = 0;
      const emit = (i: number) =>
        progressListeners.forEach((cb) =>
          cb({ entry_id: entryId, add, i, n: run.n, done, total }),
        );
      await host.tick();
      emit(0);
      for (let i = 0; i < run.n; i += 1) {
        await host.tick();
        run.step(i);
        done += copies[i]!.size;
        emit(i + 1);
        if (state.cancel && i + 1 < run.n) {
          job.notes.push({ kind: 'cancelled', copied: i + 1 });
          break;
        }
      }
      run.end(skipped);
      busy = null;
      const write = finish(g, generation, entryId, job);
      addedListeners.forEach((cb) =>
        cb({ entry_id: entryId, add, end: { kind: 'done', write } }),
      );
    })();
    return { kind: 'background', add, n: run.n, total };
  }

  function pickNames(game: string): { name: string; size: number }[] {
    const scans = (n: number) =>
      Array.from({ length: n }, (_, i) => ({
        name: `Scan ${String(i + 1).padStart(3, '0')}.png`,
        size: 4_000_000,
      }));
    switch (opts.pick) {
      case 'many':
        return scans(60);
      case 'too-many':
        return scans(ADD_FILES + 1);
      default:
        return [
          { name: 'Harbor Lights (Bonus).flac', size: 31_000_000 },
          { name: `${game} - Overworld.spc`, size: 66_048 },
        ];
    }
  }

  // ---- Reads ----

  function readable(
    generation: number,
    id: number,
    rev: number,
    index: number,
  ): { g: XGame; f: XFile } {
    const e = host.entryOf(generation, id);
    const g = gameOf(e);
    if (!g || rev !== g.rev) throw new BackendError('extras-changed');
    return { g, f: fileAt(g, index) };
  }

  const undoView = (g: XGame, j: Journal): ExtrasUndone => {
    const undid = undoOps(g, j);
    j.undone = true;
    return {
      job: j.job,
      removed: undid.removed.length,
      restored: undid.restored.length,
      kept: undid.kept.length,
      listing: view(g, j.entryId),
    };
  };

  return {
    extrasRevealed,
    extrasOpened,
    async extrasList(generation, id) {
      const e = host.entryOf(generation, id);
      const g = gameOf(e);
      return g ? view(g, id) : bareView(e, id);
    },
    extrasRename: (generation, id, rev, index, stem) =>
      write(generation, id, rev, (g, job) => {
        const f = fileAt(g, index);
        const ext = extOf(f.name)
          ? f.name.slice(f.name.lastIndexOf('.') + 1)
          : '';
        const name = validateFileStem(stem, ext, extrasPath(g, f.dir));
        if (name === f.name) return;
        if (takenIn(g, f.dir, name, f.uid)) throw refused('taken');
        job.ops.push({
          k: 'move',
          uid: f.uid,
          from: { dir: f.dir, name: f.name },
          to: { dir: f.dir, name },
        });
        f.name = name;
      }),
    extrasMove: (generation, id, rev, files, to) =>
      write(generation, id, rev, (g, job) => {
        const picked: XFile[] = [];
        for (const i of files) {
          const f = fileAt(g, i);
          if (!sameCategory(categoryOf(f.dir), to) && !picked.includes(f)) {
            picked.push(f);
          }
        }
        const { dir } = targetDir(g, to, false);
        for (const f of picked) {
          if (dir === null && isReservedKey(nameKey(f.name))) {
            throw refused('reserved');
          }
        }
        if (picked.length === 0) return;
        ensureDir(g, dir, job);
        for (const f of picked) {
          const name = freeName((n) => takenIn(g, dir, n, f.uid), f.name, job);
          job.ops.push({
            k: 'move',
            uid: f.uid,
            from: { dir: f.dir, name: f.name },
            to: { dir, name },
          });
          f.dir = dir;
          f.name = name;
        }
      }),
    extrasTrash: (generation, id, rev, files) =>
      write(generation, id, rev, (g, job) => {
        const picked: XFile[] = [];
        for (const i of files) {
          const f = fileAt(g, i);
          if (!picked.includes(f)) picked.push(f);
        }
        for (const f of picked) trashOne(g, f, job);
      }),
    extrasSetTags: (generation, id, rev, index, tags) =>
      write(generation, id, rev, (g, job) => {
        const clean = normalizeTags(tags);
        const f = fileAt(g, index);
        job.ops.push({ k: 'tags', uid: f.uid, before: f.tags, after: clean });
        f.tags = clean;
      }),
    extrasForget: (generation, id, rev, missing) =>
      write(generation, id, rev, (g, job) => {
        const uid = g.listedMissing[missing];
        const at = g.missing.findIndex((m) => m.uid === uid);
        if (uid === undefined || at < 0) {
          throw new BackendError('extras-changed');
        }
        job.ops.push({ k: 'forget', rec: g.missing[at]!, at });
        g.missing.splice(at, 1);
      }),
    extrasCategoryCreate: (generation, id, rev, raw) =>
      write(generation, id, rev, (g, job) => {
        const name = validateCategoryName(raw, userNames(g));
        if (takenIn(g, null, name)) throw refused('taken');
        if (extrasPath(g, name).length > WINDOWS_MAX_PATH) {
          throw refused('path-too-long');
        }
        ensureDir(g, name, job);
      }),
    extrasCategoryRename: (generation, id, rev, from, to) =>
      write(generation, id, rev, (g, job) => {
        if (fixedOf(from)) throw refused('reserved');
        const spelling = userNames(g).find((d) => sameDir(d, from));
        if (!spelling) throw new BackendError('extras-changed');
        const others = userNames(g).filter((d) => d !== spelling);
        const name = validateCategoryName(to, others);
        if (name === spelling) return;
        if (takenIn(g, null, name)) throw refused('taken');
        job.ops.push({ k: 'dir-renamed', from: spelling, to: name });
        g.dirs = g.dirs.map((d) => (d === spelling ? name : d));
        for (const f of g.files) if (f.dir === spelling) f.dir = name;
      }),
    extrasCategoryRemove: (generation, id, rev, name) =>
      write(generation, id, rev, (g, job) => {
        if (fixedOf(name)) throw refused('reserved');
        const spelling = userNames(g).find((d) => sameDir(d, name));
        if (!spelling) throw new BackendError('extras-changed');
        for (const f of g.files.filter((x) => x.dir === spelling)) {
          trashOne(g, f, job);
        }
        g.dirs = g.dirs.filter((d) => d !== spelling);
        job.ops.push({ k: 'dir-removed', name: spelling });
      }),
    async extrasUndo(generation, id, job) {
      const e = host.entryOf(generation, id);
      if (busy) throw new BackendError('job-running');
      const g = gameOf(e);
      const j = journalOf(job);
      if (!g || !j || j.undone || j.game !== g) {
        throw new BackendError('nothing-to-undo');
      }
      const done = undoView(g, j);
      recount(g, generation, id);
      return done;
    },
    async extrasAddPick(generation, id) {
      const g = addEntry(generation, id);
      if (opts.pick === 'cancel') return null;
      return keepPlan(generation, id, pickNames(g.title), g.title);
    },
    async extrasAddPlan(generation, id, token) {
      const g = addEntry(generation, id);
      const items = takeDrop(token).filter((i) => !i.is_dir);
      return keepPlan(generation, id, items, g.title);
    },
    async extrasAddCommit(generation, id, rev, plan, choices) {
      const p = plans.get(plan);
      if (!p || p.generation !== generation || p.entryId !== id) {
        throw new BackendError('bad-request', `no add plan ${plan}`);
      }
      const copies = choices.map((c): Copy => {
        const f = p.files[c.index];
        if (!f) {
          throw new BackendError(
            'bad-request',
            `no file ${c.index} in plan ${plan}`,
          );
        }
        return { name: f.name, size: f.size, to: c.category };
      });
      const g = writable(generation, id, rev);
      if (planView(plan, p).too_many) throw refused('too-many');
      if (opts.pick === 'no-space') {
        const needed = copies.reduce((s, c) => s + c.size, 0);
        throw new BackendError(
          'no-space',
          `${needed}/${Math.floor(needed / 3)}`,
        );
      }
      plans.delete(plan);
      return runAdd(generation, id, g, copies, skippedNotes(p.keys, p.hidden));
    },
    async extrasAddDropped(generation, id, rev, token, category) {
      const g = writable(generation, id, rev);
      const items = takeDrop(token).filter((i) => !i.is_dir);
      let keys = 0;
      let hidden = 0;
      const copies: Copy[] = [];
      for (const i of items) {
        if (isKeyMaterial(i.name)) keys += 1;
        else if (isHidden(i.name)) hidden += 1;
        else copies.push({ name: i.name, size: i.size, to: category });
      }
      return runAdd(generation, id, g, copies, skippedNotes(keys, hidden));
    },
    async extrasAddCancel(add) {
      if (!busy || busy.add !== add) return false;
      busy.cancel = true;
      return true;
    },
    async extrasMedia(generation, id, rev, index) {
      const { f } = readable(generation, id, rev, index);
      if (isKeyMaterial(f.name)) throw refused('key-file');
      const m = mediaOf(f.name);
      if (!m) throw refused('not-media');
      return {
        url: m.kind === 'audio' ? toneDataUrl() : MEDIA_URLS[m.kind],
        mime: m.mime,
        kind: m.kind,
      };
    },
    async extrasReadText(generation, id, rev, index) {
      const { f } = readable(generation, id, rev, index);
      if (isKeyMaterial(f.name)) throw refused('key-file');
      if (!['txt', 'md', 'nfo'].includes(extOf(f.name))) {
        throw refused('not-text');
      }
      return {
        text: f.text ?? '',
        truncated: false,
        encoding: extOf(f.name) === 'nfo' ? 'cp437' : 'utf-8',
      };
    },
    async extrasReveal(generation, id, rev, index) {
      const { f } = readable(generation, id, rev, index);
      extrasRevealed.push(pathOf(f));
    },
    async extrasOpenExternal(generation, id, rev, index) {
      const { f } = readable(generation, id, rev, index);
      if (playableOf(f.name) === 'reveal-only') throw refused('reveal-only');
      extrasOpened.push(pathOf(f));
    },
    onExtrasDrop: (cb) => listen(dropListeners, cb),
    onExtrasDragOver: (cb) => listen(overListeners, cb),
    onExtrasProgress: (cb) => listen(progressListeners, cb),
    onExtrasAdded: (cb) => listen(addedListeners, cb),
    emitDragOver(point) {
      overListeners.forEach((cb) => cb(point));
    },
    emitDrop({ names, position }) {
      const token = (dropSeq += 1);
      const items = names.map((name): DroppedItem => ({
        name,
        size: sizeFor(name),
        is_dir: false,
      }));
      drops.set(token, { items, at: Date.now() });
      // Rust keeps the last four drops.
      for (const old of [...drops.keys()]) {
        if (drops.size <= DROP_KEEP) break;
        drops.delete(old);
      }
      dropListeners.forEach((cb) => cb({ token, position, items }));
      return token;
    },
    extrasCount(relPath) {
      return seeded(relPath, null)?.files.length ?? null;
    },
    extrasHistory() {
      return [...journals].reverse().map((j): JobHistoryItem => ({
        job: j.job,
        root: MOCK_EXTRAS,
        created_at: j.createdAt,
        entries: j.ops.length,
        complete: true,
        undone: j.undone,
        kind: 'extras',
        undoable: true,
        game: j.game.title,
      }));
    },
    ownsExtrasJob: (job) => journalOf(job) !== undefined,
    undoExtrasJob(job) {
      const j = journalOf(job);
      if (!j || j.undone) throw new BackendError('nothing-to-undo');
      if (busy) throw new BackendError('job-running');
      const undid = undoOps(j.game, j);
      j.undone = true;
      // Only the Extras library's cells: another open library's entry
      // with the same id is not this game.
      const open = host.current();
      if (open?.root === MOCK_EXTRAS) {
        recount(j.game, open.generation, j.entryId);
      }
      return {
        report: [
          {
            job,
            removed: undid.removed,
            restored: undid.restored,
            skipped_modified: undid.kept,
            skipped_occupied: [],
            dirs_removed: [],
            archive_lost: [],
          },
        ],
        later_jobs: [],
      };
    },
  };
}
