import { Plus, Tag, Trash2, X } from 'lucide-react';
import {
  useContext,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
  type PointerEvent as ReactPointerEvent,
} from 'react';
import { createPortal } from 'react-dom';

import { Alert } from '@/components/ui/alert';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Input } from '@/components/ui/input';
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from '@/components/ui/popover';
import { Progress } from '@/components/ui/progress';
import { Spinner } from '@/components/ui/spinner';
import { toast } from '@/components/ui/toaster';
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group';
import { LocaleContext } from '@/i18n/core';
import { useRichT } from '@/i18n/rich';
import { useT, type TFunction } from '@/i18n/useT';
import { getBackend } from '@/ipc/backend';
import { EXTRAS_DROP_ROOT } from '@/ipc/dropTarget';
import { errorKey, toBackendError, type BackendError } from '@/ipc/errors';
import {
  EXTRAS_CATEGORIES,
  type AddChoice,
  type AddFinishedEvent,
  type AddStarted,
  type CategoryRef,
  type EntryView,
  type ExtraFileView,
  type ExtrasNote,
  type ExtrasView,
  type ExtrasWriteView,
  type Writability,
} from '@/ipc/types';
import {
  categoryKey,
  categoryLabel,
  filterFiles,
  fitsCategory,
  gameTags,
  moveTargets,
  nameKey,
  normalizeTags,
  sameCategory,
  sortCategories,
  splitExt,
  stepCategory,
} from '@/lib/extras';
import { formatSize } from '@/lib/format';
import { isTyping, letterOf } from '@/lib/keyboard';
import { onAddEnd, useExtrasStore } from '@/stores/extrasStore';
import { currentTrack, usePlayerStore } from '@/stores/playerStore';

import { ExtrasCategory, VIRTUALISE_AFTER } from './ExtrasCategory';
import {
  ExtrasAddDialog,
  type AddRequest,
  type AddTags,
} from './ExtrasAddDialog';
import { ExtrasActionsContext, type ExtrasActions } from './extrasContext';
import { ExtrasDropLayer, type SectionDrop } from './ExtrasDropLayer';
import { ExtrasMissing } from './ExtrasMissing';

const REASON: Record<Exclude<Writability['kind'], 'yes'>, string> = {
  'read-only': 'extras.readOnly',
  busy: 'extras.busy',
  newer: 'extras.newer',
};

const NAV_KEYS = new Set([
  'ArrowDown',
  'ArrowUp',
  'ArrowLeft',
  'ArrowRight',
  'Home',
  'End',
]);

/** The library's window-level shortcuts (cover size, info, search): never
 * fired from inside the section (D23). */
const LIBRARY_KEYS = new Set(['+', '=', '-', '_', '0', 'i', 'I', '/']);

/** Why a name was refused, as the copy says it (D1, D14). */
const WHY = new Set([
  'empty',
  'too-long',
  'not-portable',
  'dot',
  'reserved',
  'taken',
  'path-too-long',
]);

/** Where the files go when the drive has no Trash (D8). */
const SET_ASIDE = '.coverly-trash';
/** How long a toast with «Deshacer» stays (§8.3). */
const TOAST_MS = 10_000;
/** How long a moved row keeps its outline where it landed. */
const LANDED_MS = 900;
/** A pointer moved this far (px) from where it pressed: a drag. */
const DRAG_PX = 4;

interface Band {
  category: CategoryRef;
  files: ExtraFileView[];
}

/** The bands with rows (present categories, user ones even when empty,
 * loose files when any) and the fixed categories left as dashed slots. */
function bandsOf(listing: ExtrasView): {
  bands: Band[];
  slots: CategoryRef[];
} {
  const bands: Band[] = [];
  const slots: CategoryRef[] = [];
  for (const c of listing.categories) {
    const files = listing.files.filter((f) =>
      sameCategory(f.category, c.category),
    );
    // A user category is a folder the user made: shown even when empty.
    if (c.category.kind === 'user' || files.length > 0)
      bands.push({ category: c.category, files });
    else if (c.category.kind === 'fixed') slots.push(c.category);
  }
  // A fixed category the listing did not name is a slot too.
  for (const name of EXTRAS_CATEGORIES)
    if (
      !listing.categories.some(
        (c) => c.category.kind === 'fixed' && c.category.name === name,
      )
    )
      slots.push({ kind: 'fixed', name });
  return {
    bands: sortCategories(bands),
    slots: sortCategories(slots.map((category) => ({ category }))).map(
      (s) => s.category,
    ),
  };
}

/** A refused name in words, or null for any other error (D1, D14). */
function refusalText(
  err: BackendError,
  t: TFunction,
  of: 'file' | 'category',
): string | null {
  if (err.code !== 'extras-refused' || !err.detail || !WHY.has(err.detail))
    return null;
  if (of === 'file' && err.detail === 'taken') return t('extras.nameTaken');
  return t(of === 'file' ? 'extras.nameInvalid' : 'extras.categoryInvalid', {
    why: t(`extras.why.${err.detail}`),
  });
}

/** What a finished write says beside its toast. */
function noteTexts(notes: readonly ExtrasNote[], t: TFunction): string[] {
  return notes.flatMap((n) => {
    switch (n.kind) {
      case 'malformed-saved':
        return [t('extras.malformedSaved', { file: n.file })];
      case 'set-aside':
        return [t('extras.noTrash', { folder: n.folder })];
      case 'stays-no-trash':
        return [t('extras.noTrashStays')];
      case 'renamed-dup':
        return [t('extras.renamedDup', { name: n.name })];
      case 'sanitised':
        return [t('extras.nameSanitised', { names: n.names.join(', ') })];
      case 'keys-skipped':
        return [
          n.n === 1
            ? t('extras.keysSkippedOne')
            : t('extras.keysSkipped', { n: n.n }),
        ];
      case 'hidden-skipped':
        return [
          n.n === 1
            ? t('extras.hiddenSkippedOne')
            : t('extras.hiddenSkipped', { n: n.n }),
        ];
      default:
        return [];
    }
  });
}

/** Where focus goes after a write: a file by path, or by its new place. */
type FocusAfter =
  | { path: string; landed?: false }
  | { category: CategoryRef; name: string; landed: boolean };

type Confirm =
  | { kind: 'trash'; files: number[]; name: string }
  | { kind: 'remove'; name: string; count: number };

/** What an add copies, for its toast and the tags chosen in the dialog. */
interface AddMeta {
  entryId: number;
  /** Where each file goes (by its source name) and the tags to set. */
  files: { name: string; category: CategoryRef; tags: string[] }[];
}

interface Drag {
  files: number[];
  from: string;
  name: string;
  x0: number;
  y0: number;
  active: boolean;
  over: string | null;
}

/**
 * «Extras» in the game panel (extras spec §8): the game folder's `Extras/`
 * as category bands of file rows, the empty suggested categories as one
 * line of dashed slots, «No encontrados» last, and the CRUD of §8.3:
 * inline rename, «Mover a…», `Alt+↑/↓` and pointer drag onto a band, the
 * Trash with an undo toast, tags, tag and text filters, a selection with
 * its bulk bar, own categories and `Cmd/Ctrl+Z`, and the adds of D13:
 * «Añadir archivos» or a category's «+» (the native picker, then the add
 * dialog), OS drops on a category (D11 a) and the background copy's
 * progress line with «Cancelar». Opening it never writes (D3); every
 * write is one journaled job (D10). One roving tab stop.
 */
export function ExtrasSection({
  generation,
  entry,
  onOpen,
  onPlay,
}: {
  generation: number;
  entry: EntryView;
  onOpen?: ((index: number) => void) | undefined;
  onPlay?: ((index: number) => void) | undefined;
}) {
  const t = useT();
  const tr = useRichT();
  const locale = useContext(LocaleContext);
  const open = useExtrasStore((s) => s.open);
  const stored = useExtrasStore((s) =>
    s.generation === generation && s.listing?.entry_id === entry.id
      ? s.listing
      : null,
  );
  const loading = useExtrasStore((s) => s.loading);
  const error = useExtrasStore((s) => s.error);
  const selection = useExtrasStore((s) => s.selection);
  const addRun = useExtrasStore((s) =>
    s.addRun?.entryId === entry.id ? s.addRun : null,
  );
  const adding = useExtrasStore((s) => s.pending === 'add');
  const [addReq, setAddReq] = useState<AddRequest | null>(null);
  const addMeta = useRef(new Map<number, AddMeta>());
  const track = usePlayerStore(currentTrack);
  const [writeError, setWriteError] = useState<BackendError | null>(null);
  const [focused, setFocused] = useState<number | null>(null);
  // Bands the user opened or closed, by category key; the rest follow the
  // default (a band over 200 files starts closed, §8.2).
  const [toggled, setToggled] = useState<Record<string, boolean>>({});
  const [renaming, setRenaming] = useState<number | null>(null);
  const [tagsFor, setTagsFor] = useState<number | null>(null);
  const [filterTags, setFilterTags] = useState<string[]>([]);
  const [filterText, setFilterText] = useState('');
  const [confirm, setConfirm] = useState<Confirm | null>(null);
  const [newCategory, setNewCategory] = useState(false);
  const [message, setMessage] = useState('');
  const [landed, setLanded] = useState<string | null>(null);
  const [dropKey, setDropKey] = useState<string | null>(null);
  const [ghost, setGhost] = useState<{
    x: number;
    y: number;
    name: string;
    n: number;
  } | null>(null);
  const sectionRef = useRef<HTMLElement>(null);
  const focusAfter = useRef<FocusAfter | null>(null);
  const drag = useRef<Drag | null>(null);
  const swallow = useRef(false);
  const lastAction = useRef<{
    entryId: number;
    jobs: string[];
    toast?: string | number | undefined;
  } | null>(null);
  const landedTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    void open(generation, entry.id);
  }, [open, generation, entry.id]);

  // Another game: its own filters and edits (reset while rendering).
  const [forEntry, setForEntry] = useState(entry.id);
  if (forEntry !== entry.id) {
    setForEntry(entry.id);
    setFilterTags([]);
    setFilterText('');
    setRenaming(null);
    setTagsFor(null);
    setNewCategory(false);
    setWriteError(null);
  }

  const listing = stored;
  const layout = useMemo(() => (listing ? bandsOf(listing) : null), [listing]);
  const allTags = useMemo(
    () => (listing ? gameTags(listing.files) : []),
    [listing],
  );
  const targets = useMemo(
    () => (listing ? moveTargets(listing.categories) : []),
    [listing],
  );

  // A write answered: focus (and outline) the file where it is now.
  useEffect(() => {
    const want = focusAfter.current;
    if (!want || !listing) return;
    const file =
      'path' in want
        ? listing.files.find((f) => f.path === want.path)
        : listing.files.find(
            (f) =>
              sameCategory(f.category, want.category) && f.name === want.name,
          );
    if (!file) {
      focusAfter.current = null;
      return;
    }
    const el = sectionRef.current?.querySelector<HTMLElement>(
      `[data-file="${file.index}"] [data-extras-primary]`,
    );
    if (!el) return;
    focusAfter.current = null;
    el.focus();
    if (want.landed) {
      setLanded(file.path);
      if (landedTimer.current) clearTimeout(landedTimer.current);
      landedTimer.current = setTimeout(() => setLanded(null), LANDED_MS);
    }
  }, [listing, renaming]);

  useEffect(
    () => () => {
      if (landedTimer.current) clearTimeout(landedTimer.current);
    },
    [],
  );

  const canWrite = listing?.writable.kind === 'yes';
  const reason =
    listing && listing.writable.kind !== 'yes'
      ? REASON[listing.writable.kind]
      : null;
  const activeTags = filterTags.filter((f) =>
    allTags.some((a) => nameKey(a) === nameKey(f)),
  );
  const filtering = activeTags.length > 0 || filterText.trim() !== '';
  const shown = layout
    ? layout.bands
        .map((b) => ({
          ...b,
          total: b.files.length,
          files: filtering
            ? filterFiles(b.files, { tags: activeTags, text: filterText })
            : b.files,
        }))
        .filter((b) => !filtering || b.files.length > 0)
    : [];
  // A filter that leaves few rows opens a large band.
  const isOpen = (b: Band) =>
    toggled[categoryKey(b.category)] ?? b.files.length <= VIRTUALISE_AFTER;
  // The tab stop goes only to a row on screen: never into a closed band.
  const files = shown.filter(isOpen).flatMap((b) => b.files);
  const tabStop =
    focused !== null && files.some((f) => f.index === focused)
      ? focused
      : (files[0]?.index ?? null);
  const playingPath =
    track && track.generation === generation && track.entryId === entry.id
      ? track.path
      : null;
  const empty =
    listing !== null &&
    listing.files.length === 0 &&
    layout !== null &&
    layout.bands.length === 0;
  const manualsBand = layout?.bands.some(
    (b) => b.category.kind === 'fixed' && b.category.name === 'manuals',
  );
  const num = new Intl.NumberFormat(locale);
  const fileAt = (i: number) => listing?.files.find((f) => f.index === i);
  const store = () => useExtrasStore.getState();

  /* ---------- writes ------------------------------------------------- */

  function announce(text: string) {
    // The same words twice are still read twice.
    setMessage((m) => (m === text ? `${text}\u00a0` : text));
  }

  async function undoJobs(entryId: number, jobs: readonly string[]) {
    setWriteError(null);
    try {
      for (const job of [...jobs].reverse())
        await store().undoJob(entryId, job);
      if (lastAction.current?.jobs === jobs) {
        // Its toast's «Deshacer» would now undo nothing.
        if (lastAction.current.toast !== undefined)
          toast.dismiss(lastAction.current.toast);
        lastAction.current = null;
      }
      announce(t('extras.undone'));
    } catch (e) {
      setWriteError(toBackendError(e));
    }
  }

  /** A finished write: the toast with «Deshacer», the live region. */
  function done(writes: readonly ExtrasWriteView[], text: string) {
    const jobs = writes.flatMap((w) => (w.job ? [w.job] : []));
    const notes = writes.flatMap((w) => noteTexts(w.notes, t));
    announce(text);
    const entryId = entry.id;
    const id: string | number | undefined = toast(text, {
      description: notes.length > 0 ? notes.join(' ') : undefined,
      duration: TOAST_MS,
      action:
        jobs.length > 0
          ? {
              label: t('extras.undo'),
              onClick: () => void undoJobs(entryId, jobs),
            }
          : undefined,
    });
    if (jobs.length > 0) lastAction.current = { entryId, jobs, toast: id };
  }

  async function run(
    write: () => Promise<ExtrasWriteView>,
    text: (w: ExtrasWriteView) => string,
  ): Promise<boolean> {
    setWriteError(null);
    try {
      const w = await write();
      done([w], text(w));
      return true;
    } catch (e) {
      focusAfter.current = null;
      setWriteError(toBackendError(e));
      return false;
    }
  }

  async function undoLast() {
    const last = lastAction.current;
    if (last && last.entryId === entry.id) {
      await undoJobs(last.entryId, last.jobs);
      return;
    }
    setWriteError(null);
    try {
      if (await store().undo()) announce(t('extras.undone'));
    } catch (e) {
      setWriteError(toBackendError(e));
    }
  }

  function move(indexes: number[], to: CategoryRef) {
    const first = fileAt(indexes[0]!);
    if (!first) return;
    const label = categoryLabel(to, t);
    focusAfter.current = { category: to, name: first.name, landed: true };
    void run(
      () => store().move(indexes, to),
      () =>
        indexes.length === 1
          ? t('extras.moved', { category: label })
          : t('extras.movedMany', { n: indexes.length, category: label }),
    );
  }

  /** The file focus moves to once `gone` left the list: the next row on
   * screen, else the one before. */
  function nextAfter(gone: ReadonlySet<number>): string | null {
    let at = -1;
    files.forEach((f, i) => {
      if (gone.has(f.index)) at = i;
    });
    const after = files.slice(at + 1).find((f) => !gone.has(f.index));
    const before = files
      .slice(0, Math.max(at, 0))
      .reverse()
      .find((f) => !gone.has(f.index));
    return (after ?? before)?.path ?? null;
  }

  function doTrash(indexes: number[]) {
    const gone = new Set(indexes);
    const next = nextAfter(gone);
    focusAfter.current = next ? { path: next } : null;
    const name = fileAt(indexes[0]!)?.name ?? '';
    void run(
      () => store().trash(indexes),
      () =>
        indexes.length === 1
          ? t('extras.trashed', { name })
          : t('extras.trashedMany', { n: indexes.length }),
    );
  }

  async function doRemove(name: string) {
    await run(
      () => store().removeCategory(name),
      () => t('extras.categoryRemoved', { name }),
    );
  }

  async function bulkTag(raw: string) {
    if (!listing) return;
    const paths = listing.files
      .filter((f) => selection.has(f.index))
      .map((f) => f.path);
    const writes: ExtrasWriteView[] = [];
    setWriteError(null);
    try {
      for (const path of paths) {
        const f = store().listing?.files.find((x) => x.path === path);
        if (!f) continue;
        const next = normalizeTags([...f.tags, raw]);
        if (!next.ok || next.tags.length === f.tags.length) continue;
        writes.push(await store().setTags(f.index, next.tags));
      }
    } catch (e) {
      setWriteError(toBackendError(e));
    }
    const tag = raw.trim().normalize('NFC');
    if (writes.length > 0) done(writes, t('extras.tagsSaved', { name: tag }));
  }

  /* ---------- adds (D13, D11 a) ------------------------------------- */

  function addedText(meta: AddMeta, copied: number): string {
    const keys = new Set(meta.files.map((f) => categoryKey(f.category)));
    const first = meta.files[0]?.category;
    if (keys.size === 1 && first) {
      const category = categoryLabel(first, t);
      return copied === 1
        ? t('extras.addedOne', { category })
        : t('extras.added', { n: copied, category });
    }
    return t('extras.addSpread', { n: copied });
  }

  /** The tags chosen in the add dialog, set on the copies once they are
   * listed (a copy renamed on a collision keeps none: said in its note). */
  async function tagAdded(meta: AddMeta): Promise<ExtrasWriteView[]> {
    const writes: ExtrasWriteView[] = [];
    for (const want of meta.files) {
      if (want.tags.length === 0) continue;
      const now = store().listing;
      if (!now || now.entry_id !== meta.entryId) break;
      const f = now.files.find(
        (x) => sameCategory(x.category, want.category) && x.name === want.name,
      );
      if (!f) continue;
      const next = normalizeTags([...f.tags, ...want.tags]);
      if (!next.ok || next.tags.length === f.tags.length) continue;
      writes.push(await store().setTags(f.index, next.tags));
    }
    return writes;
  }

  /** An add answered: its toast with «Deshacer» (the copy and its tags). */
  async function landedAdd(meta: AddMeta, write: ExtrasWriteView) {
    let tagged: ExtrasWriteView[] = [];
    try {
      tagged = await tagAdded(meta);
    } catch (e) {
      setWriteError(toBackendError(e));
    }
    const cancelled = write.notes.find((n) => n.kind === 'cancelled');
    done(
      [write, ...tagged],
      cancelled?.kind === 'cancelled'
        ? t('extras.cancelled', { n: cancelled.copied })
        : addedText(meta, meta.files.length),
    );
  }

  function addStarted(meta: AddMeta, started: AddStarted) {
    if (started.kind === 'done') void landedAdd(meta, started.write);
    else addMeta.current.set(started.add, meta);
  }

  /** «Añadir archivos» or a category's «+»: the native picker (opened by
   * Rust), then the add dialog. */
  async function pickAdd(category?: CategoryRef) {
    setWriteError(null);
    try {
      const b = await getBackend();
      const plan = await b.extrasAddPick(generation, entry.id);
      if (plan) setAddReq({ plan, category });
    } catch (e) {
      setWriteError(toBackendError(e));
    }
  }

  async function commitAdd(
    choices: AddChoice[],
    tags: AddTags,
  ): Promise<BackendError | null> {
    const req = addReq;
    if (!req) return null;
    const meta: AddMeta = {
      entryId: entry.id,
      files: choices.map((c) => ({
        name: req.plan.files.find((f) => f.index === c.index)?.name ?? '',
        category: c.category,
        tags: tags[c.index] ?? [],
      })),
    };
    try {
      const started = await store().addCommit(req.plan.plan, choices);
      setAddReq(null);
      addStarted(meta, started);
      return null;
    } catch (e) {
      return toBackendError(e);
    }
  }

  /** An OS drop on the section (D11 a): straight into the category when
   * every file fits it, else the add dialog (asking for the category when
   * it landed on none). */
  async function dropAdd(d: SectionDrop) {
    setWriteError(null);
    const to = d.key
      ? (targets.find((c) => categoryKey(c) === d.key) ?? null)
      : null;
    const fits =
      to !== null &&
      d.items.length > 0 &&
      d.items.every((i) => !i.is_dir && fitsCategory(i.name, to));
    try {
      if (to && fits) {
        const meta: AddMeta = {
          entryId: entry.id,
          files: d.items.map((i) => ({ name: i.name, category: to, tags: [] })),
        };
        addStarted(meta, await store().addDropped(d.token, to));
        return;
      }
      const b = await getBackend();
      const plan = await b.extrasAddPlan(generation, entry.id, d.token);
      setAddReq({ plan, dropped: to ?? undefined, ask: to === null });
    } catch (e) {
      setWriteError(toBackendError(e));
    }
  }

  /* ---------- pointer drag (D11 b) ----------------------------------- */

  function hit(x: number, y: number): string | null {
    const bands =
      sectionRef.current?.querySelectorAll<HTMLElement>('[data-extras-band]');
    for (const el of bands ?? []) {
      const r = el.getBoundingClientRect();
      if (x >= r.left && x <= r.right && y >= r.top && y <= r.bottom)
        return el.dataset.extrasBand ?? null;
    }
    return null;
  }

  function endDrag(drop: boolean) {
    const d = drag.current;
    drag.current = null;
    document.removeEventListener('pointermove', onDragMove);
    document.removeEventListener('pointerup', onDragUp);
    document.removeEventListener('pointercancel', onDragCancel);
    document.removeEventListener('keydown', onDragKey, true);
    setGhost(null);
    setDropKey(null);
    if (!d?.active) return;
    // The click the pointer up makes is not a selection.
    swallow.current = true;
    setTimeout(() => {
      swallow.current = false;
    }, 0);
    const to = d.over ? targets.find((c) => categoryKey(c) === d.over) : null;
    if (drop && to) move(d.files, to);
  }

  function onDragMove(ev: PointerEvent) {
    const d = drag.current;
    if (!d) return;
    if (!d.active) {
      if (Math.hypot(ev.clientX - d.x0, ev.clientY - d.y0) < DRAG_PX) return;
      d.active = true;
    }
    const key = hit(ev.clientX, ev.clientY);
    d.over = key && key !== d.from ? key : null;
    setDropKey(d.over);
    setGhost({ x: ev.clientX, y: ev.clientY, name: d.name, n: d.files.length });
  }

  function onDragUp() {
    endDrag(true);
  }

  function onDragCancel() {
    endDrag(false);
  }

  function onDragKey(ev: globalThis.KeyboardEvent) {
    if (ev.key !== 'Escape') return;
    // Cancels the drag only: the panel stays open.
    ev.preventDefault();
    ev.stopPropagation();
    endDrag(false);
  }

  function pointerDown(e: ReactPointerEvent<HTMLElement>, file: ExtraFileView) {
    if (!canWrite || e.button !== 0 || drag.current) return;
    const target = e.target as HTMLElement;
    if (target.closest('input, [role="checkbox"], .extras-row__ctls')) return;
    drag.current = {
      files:
        selection.has(file.index) && selection.size > 1
          ? [...selection]
          : [file.index],
      from: categoryKey(file.category),
      name: file.name,
      x0: e.clientX,
      y0: e.clientY,
      active: false,
      over: null,
    };
    document.addEventListener('pointermove', onDragMove);
    document.addEventListener('pointerup', onDragUp);
    document.addEventListener('pointercancel', onDragCancel);
    document.addEventListener('keydown', onDragKey, true);
  }

  /* ---------- the rows' actions -------------------------------------- */

  const actions: ExtrasActions = {
    canWrite,
    targets,
    selection,
    renaming,
    tagsFor,
    suggestions: allTags,
    landed,
    select: (index, mode) => store().select(index, mode),
    clearSelection: () => store().clearSelection(),
    startRename(index, refocus) {
      const was = renaming === null ? undefined : fileAt(renaming);
      if (index === null && refocus && was)
        focusAfter.current = { path: was.path };
      setRenaming(index);
    },
    async rename(file, stem) {
      const [, ext] = splitExt(file.name);
      const name = `${stem.trim().normalize('NFC')}${ext}`;
      setWriteError(null);
      try {
        const w = await store().rename(file.index, stem);
        focusAfter.current = { category: file.category, name, landed: false };
        setRenaming(null);
        done([w], t('extras.renamed', { name }));
        return null;
      } catch (e) {
        const err = toBackendError(e);
        const why = refusalText(err, t, 'file');
        if (why) return why;
        setRenaming(null);
        setWriteError(err);
        return null;
      }
    },
    move,
    step(file, delta) {
      const to = stepCategory(targets, file.category, delta);
      if (to) move([file.index], to);
    },
    trash(indexes) {
      if (indexes.length === 0 || !listing) return;
      if (indexes.length > 1 || !listing.trash)
        setConfirm({
          kind: 'trash',
          files: indexes,
          name: fileAt(indexes[0]!)?.name ?? '',
        });
      else doTrash(indexes);
    },
    openTags: setTagsFor,
    saveTags(file, tags) {
      focusAfter.current = { path: file.path };
      void run(
        () => store().setTags(file.index, tags),
        () => t('extras.tagsSaved', { name: file.name }),
      );
    },
    pointerDown,
    swallowClick() {
      const was = swallow.current;
      swallow.current = false;
      return was;
    },
    async renameCategory(from, to) {
      setWriteError(null);
      try {
        const w = await store().renameCategory(from, to);
        done([w], t('extras.categoryRenamed', { name: to.trim() }));
        return null;
      } catch (e) {
        const err = toBackendError(e);
        const why = refusalText(err, t, 'category');
        if (why) return why;
        setWriteError(err);
        return null;
      }
    },
    removeCategory(name, count) {
      if (count > 0) setConfirm({ kind: 'remove', name, count });
      else void doRemove(name);
    },
  };

  /* ---------- keys --------------------------------------------------- */

  function onKeyDown(e: KeyboardEvent<HTMLElement>) {
    if (e.defaultPrevented) return;
    const mod = e.metaKey || e.ctrlKey;
    if (
      mod &&
      !e.altKey &&
      !e.shiftKey &&
      letterOf(e) === 'z' &&
      !isTyping(e.target)
    ) {
      // This game's last Extras job, never the toolbar's fill undo.
      e.preventDefault();
      e.stopPropagation();
      void undoLast();
      return;
    }
    if (LIBRARY_KEYS.has(e.key) && !mod) {
      e.stopPropagation();
      return;
    }
    if (!NAV_KEYS.has(e.key)) return;
    if (e.altKey || e.metaKey || e.ctrlKey || e.shiftKey) return;
    const target = e.target as HTMLElement;
    const row = target.closest<HTMLElement>('[data-extras-row]');
    if (!row || !target.hasAttribute('data-extras-ctl')) return;
    let next: HTMLElement | undefined;
    if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') {
      const ctls = [...row.querySelectorAll<HTMLElement>('[data-extras-ctl]')];
      const at = ctls.indexOf(target);
      next = ctls[at + (e.key === 'ArrowRight' ? 1 : -1)];
    } else {
      const primaries = [
        ...e.currentTarget.querySelectorAll<HTMLElement>(
          '[data-extras-primary]',
        ),
      ];
      const at = primaries.indexOf(
        row.querySelector<HTMLElement>('[data-extras-primary]')!,
      );
      next =
        e.key === 'Home'
          ? primaries[0]
          : e.key === 'End'
            ? primaries.at(-1)
            : primaries[at + (e.key === 'ArrowDown' ? 1 : -1)];
    }
    // The keys stay in the section: the library behind it never sees them.
    e.preventDefault();
    e.stopPropagation();
    next?.focus();
  }

  async function forget(index: number) {
    setWriteError(null);
    try {
      await store().forget(index);
    } catch (err) {
      setWriteError(toBackendError(err));
    }
  }

  // A background add of this game ended: its toast, its chosen tags.
  const addEnded = useRef<(e: AddFinishedEvent) => void>(() => {});
  useLayoutEffect(() => {
    addEnded.current = (e) => {
      const meta = addMeta.current.get(e.add);
      if (!meta) return;
      addMeta.current.delete(e.add);
      if (e.end.kind === 'done') void landedAdd(meta, e.end.write);
      else setWriteError(e.end.error);
    };
  });
  useEffect(() => onAddEnd((e) => addEnded.current(e)), []);

  if (listing && !listing.game_folder) return null;

  const shownError = writeError ?? error;
  const canAdd = canWrite && addRun === null && !adding;
  const add = (c?: CategoryRef) => void pickAdd(c);
  const selected = selection.size;

  return (
    <ExtrasActionsContext.Provider value={actions}>
      {/* eslint-disable-next-line jsx-a11y/no-noninteractive-element-interactions -- keys bubble from the rows' controls (roving focus, §8.4) */}
      <section
        ref={sectionRef}
        id="panel-extras"
        data-panel-section="extras"
        data-entry={listing?.entry_id}
        data-dragging={ghost !== null || undefined}
        {...{ [EXTRAS_DROP_ROOT]: '' }}
        className="panel-sec panel-extras"
        aria-labelledby="panel-extras-h"
        aria-busy={loading || undefined}
        onKeyDown={onKeyDown}
      >
        <div className="extras-head">
          <h3 id="panel-extras-h" className="panel-sec__h extras-head__title">
            {t('extras.title')}
            {listing && (
              <small className="mono">{num.format(listing.files.length)}</small>
            )}
          </h3>
          <ExtrasDropLayer
            rootRef={sectionRef}
            enabled={canAdd}
            onOver={setDropKey}
            onDrop={(d) => void dropAdd(d)}
            announce={announce}
          />
          {loading && <Spinner size="sm" className="extras-head__spin" />}
          <Button
            variant="ghost"
            size="sm"
            className="extras-head__add"
            disabled={!canAdd}
            onClick={() => add()}
          >
            <Plus aria-hidden />
            {t('extras.add')}
          </Button>
        </div>
        {addRun && (
          <div className="extras-progress">
            <p
              id="panel-extras-progress"
              className="extras-progress__text"
              role="status"
            >
              {tr('extras.copying', {
                i: num.format(Math.min(addRun.i + 1, addRun.n)),
                n: num.format(addRun.n),
                done: formatSize(addRun.done, locale),
                total: formatSize(addRun.total, locale),
              })}
            </p>
            <Button
              variant="ghost"
              size="sm"
              className="extras-progress__cancel"
              disabled={addRun.cancelling}
              onClick={() => void store().cancelAdd()}
            >
              {t('extras.cancel')}
            </Button>
            <Progress
              size="sm"
              className="extras-progress__bar"
              value={addRun.done}
              max={Math.max(addRun.total, 1)}
              aria-labelledby="panel-extras-progress"
            />
          </div>
        )}
        {reason && <p className="panel-note extras-reason">{t(reason)}</p>}
        {listing?.index.state === 'malformed' && (
          <Alert variant="warning" className="extras-alert">
            {t('extras.malformed')}
          </Alert>
        )}
        {shownError && (
          <Alert variant="danger" className="extras-alert">
            {t(errorKey(shownError.code), { detail: shownError.detail ?? '' })}
          </Alert>
        )}
        {(allTags.length > 0 || (listing?.files.length ?? 0) > 12) && (
          <div className="extras-filter">
            {allTags.length > 0 && (
              <ToggleGroup
                type="multiple"
                size="sm"
                className="extras-filter__tags"
                aria-label={t('extras.filterTags')}
                value={activeTags}
                onValueChange={setFilterTags}
              >
                {allTags.map((tag) => (
                  <ToggleGroupItem
                    key={tag}
                    value={tag}
                    className="extras-filter__chip"
                  >
                    {tag}
                  </ToggleGroupItem>
                ))}
              </ToggleGroup>
            )}
            {(listing?.files.length ?? 0) > 12 && (
              <Input
                type="search"
                size="sm"
                className="extras-filter__text"
                aria-label={t('extras.filterText')}
                placeholder={t('extras.filterText')}
                value={filterText}
                onChange={(e) => setFilterText(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Escape' && filterText !== '') {
                    e.preventDefault();
                    e.stopPropagation();
                    setFilterText('');
                  }
                }}
              />
            )}
          </div>
        )}
        {selected > 0 && (
          <div
            role="toolbar"
            aria-label={
              selected === 1
                ? t('extras.selectedOne')
                : t('extras.selected', { n: selected })
            }
            className="extras-bulk"
          >
            <span className="extras-bulk__count">
              {selected === 1
                ? t('extras.selectedOne')
                : tr('extras.selected', { n: num.format(selected) })}
            </span>
            <DropdownMenu modal={false}>
              <DropdownMenuTrigger asChild>
                <Button variant="ghost" size="sm" disabled={!canWrite}>
                  {t('extras.moveTo')}
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="start">
                {targets.map((c) => (
                  <DropdownMenuItem
                    key={categoryKey(c)}
                    onSelect={() => move([...selection], c)}
                  >
                    {categoryLabel(c, t)}
                  </DropdownMenuItem>
                ))}
              </DropdownMenuContent>
            </DropdownMenu>
            <BulkTag
              n={selected}
              disabled={!canWrite}
              onTag={(tag) => void bulkTag(tag)}
            />
            <Button
              variant="ghost"
              size="sm"
              disabled={!canWrite}
              onClick={() => actions.trash([...selection])}
            >
              <Trash2 aria-hidden />
              {t('extras.trash')}
            </Button>
            <button
              type="button"
              className="extras-row__btn extras-bulk__clear"
              aria-label={t('extras.clearSelection')}
              onClick={() => store().clearSelection()}
            >
              <X aria-hidden />
            </button>
          </div>
        )}
        {empty && <p className="extras-empty">{t('extras.empty')}</p>}
        {filtering && shown.length === 0 && (
          <p className="extras-empty">{t('extras.filterNone')}</p>
        )}
        {shown.map((b) => (
          <ExtrasCategory
            key={categoryKey(b.category)}
            category={b.category}
            files={b.files}
            total={b.total}
            dropOver={dropKey === categoryKey(b.category)}
            canWrite={canWrite}
            tabStop={tabStop}
            playingPath={playingPath}
            manualNote={
              listing!.manual_inside &&
              b.category.kind === 'fixed' &&
              b.category.name === 'manuals'
            }
            open={isOpen(b)}
            onToggle={() =>
              setToggled((o) => ({
                ...o,
                [categoryKey(b.category)]: !isOpen(b),
              }))
            }
            onFocusRow={setFocused}
            onAdd={canAdd ? add : undefined}
            onOpen={onOpen}
            onPlay={onPlay}
          />
        ))}
        {newCategory && (
          <NewCategory
            onDone={() => setNewCategory(false)}
            onCreate={async (name) => {
              setWriteError(null);
              try {
                const w = await store().createCategory(name);
                setNewCategory(false);
                done([w], t('extras.categoryCreated', { name: name.trim() }));
                return null;
              } catch (e) {
                const err = toBackendError(e);
                const why = refusalText(err, t, 'category');
                if (why) return why;
                setNewCategory(false);
                setWriteError(err);
                return null;
              }
            }}
          />
        )}
        {layout && (
          <ul className="extras-slots">
            {layout.slots.map((c) => {
              const label = categoryLabel(c, t);
              const key = categoryKey(c);
              return (
                <li key={key}>
                  <button
                    type="button"
                    className="extras-slot"
                    data-extras-band={key}
                    data-drop={dropKey === key || undefined}
                    aria-label={t('extras.addTo', { category: label })}
                    disabled={!canAdd}
                    onClick={() => add(c)}
                  >
                    {label}
                  </button>
                </li>
              );
            })}
            <li className="extras-slots__new">
              <Button
                variant="ghost"
                size="sm"
                disabled={!canWrite || newCategory}
                onClick={() => setNewCategory(true)}
              >
                <Plus aria-hidden />
                {t('extras.newCategory')}
              </Button>
            </li>
          </ul>
        )}
        {listing?.manual_inside && !manualsBand && (
          <p className="panel-note">{t('extras.manualInside')}</p>
        )}
        {listing && listing.missing.length > 0 && (
          <ExtrasMissing
            missing={listing.missing}
            canWrite={canWrite}
            onRescan={() => void store().refresh()}
            onForget={(i) => void forget(i)}
          />
        )}
        <div aria-live="polite" aria-atomic="true" className="sr-only">
          {message}
        </div>
        {ghost &&
          createPortal(
            <div
              className="extras-ghost"
              aria-hidden
              style={{
                transform: `translate(${ghost.x + 12}px, ${ghost.y + 10}px)`,
              }}
            >
              <span className="extras-ghost__name">{ghost.name}</span>
              {ghost.n > 1 && (
                <span className="extras-ghost__n mono">
                  {num.format(ghost.n)}
                </span>
              )}
            </div>,
            document.body,
          )}
        <ExtrasAddDialog
          request={addReq}
          targets={targets}
          onCommit={commitAdd}
          onClose={() => setAddReq(null)}
        />
        <AlertDialog
          open={confirm !== null}
          onOpenChange={(o) => {
            if (!o) setConfirm(null);
          }}
        >
          {confirm && (
            <AlertDialogContent>
              <AlertDialogHeader>
                <AlertDialogTitle>
                  {confirm.kind === 'trash'
                    ? confirm.files.length === 1
                      ? t('extras.trashConfirmOne', { name: confirm.name })
                      : t('extras.trashConfirm', { n: confirm.files.length })
                    : confirm.count === 1
                      ? t('extras.categoryRemoveConfirmOne', {
                          name: confirm.name,
                        })
                      : t('extras.categoryRemoveConfirm', {
                          name: confirm.name,
                          n: confirm.count,
                        })}
                </AlertDialogTitle>
                <AlertDialogDescription>
                  {listing?.trash
                    ? t('extras.trashCaveat')
                    : t('extras.noTrash', { folder: SET_ASIDE })}
                </AlertDialogDescription>
              </AlertDialogHeader>
              <AlertDialogFooter>
                <AlertDialogCancel>{t('extras.cancel')}</AlertDialogCancel>
                <AlertDialogAction
                  variant="destructive"
                  onClick={() => {
                    if (confirm.kind === 'trash') doTrash(confirm.files);
                    else void doRemove(confirm.name);
                  }}
                >
                  {confirm.kind === 'trash'
                    ? t('extras.trash')
                    : t('extras.categoryRemove')}
                </AlertDialogAction>
              </AlertDialogFooter>
            </AlertDialogContent>
          )}
        </AlertDialog>
      </section>
    </ExtrasActionsContext.Provider>
  );
}

/** «Etiquetar» of the bulk bar: one tag added to every selected file. */
function BulkTag({
  n,
  disabled,
  onTag,
}: {
  n: number;
  disabled: boolean;
  onTag(tag: string): void;
}) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const [text, setText] = useState('');
  return (
    <Popover
      open={open}
      onOpenChange={(o) => {
        setOpen(o);
        if (!o) setText('');
      }}
    >
      <PopoverTrigger asChild>
        <Button variant="ghost" size="sm" disabled={disabled}>
          <Tag aria-hidden />
          {t('extras.tagBulk')}
        </Button>
      </PopoverTrigger>
      <PopoverContent
        align="start"
        className="extras-tagpop"
        onKeyDown={(e) => e.stopPropagation()}
      >
        <Input
          size="sm"
          type="text"
          // eslint-disable-next-line jsx-a11y/no-autofocus -- the popover opened to type a tag
          autoFocus
          maxLength={64}
          value={text}
          aria-label={t('extras.tagBulkLabel', { n })}
          placeholder={t('extras.tagAdd')}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && text.trim() !== '') {
              e.preventDefault();
              onTag(text);
              setOpen(false);
              setText('');
            }
          }}
        />
      </PopoverContent>
    </Popover>
  );
}

/** «Nueva categoría»: an inline band with the name (D1); `Esc` cancels
 * without closing the panel, a refused name says why. */
function NewCategory({
  onCreate,
  onDone,
}: {
  onCreate(name: string): Promise<string | null>;
  onDone(): void;
}) {
  const t = useT();
  const [value, setValue] = useState('');
  const [error, setError] = useState<string | null>(null);
  return (
    <div className="extras-band extras-band--new">
      <div className="extras-band__head">
        <div className="extras-rename extras-rename--band">
          <input
            type="text"
            className="extras-rename__input"
            // eslint-disable-next-line jsx-a11y/no-autofocus -- the user asked for a new category
            autoFocus
            value={value}
            placeholder={t('extras.categoryName')}
            aria-label={t('extras.categoryName')}
            aria-invalid={error !== null || undefined}
            aria-describedby={error ? 'extras-newcat-error' : undefined}
            onChange={(e) => {
              setValue(e.target.value);
              setError(null);
            }}
            onBlur={() => {
              if (value.trim() === '') onDone();
            }}
            onKeyDown={(e) => {
              e.stopPropagation();
              if (e.key === 'Enter') {
                e.preventDefault();
                void onCreate(value).then(setError);
              } else if (e.key === 'Escape') {
                e.preventDefault();
                onDone();
              }
            }}
          />
          {error && (
            <p
              id="extras-newcat-error"
              role="alert"
              className="extras-rename__error"
            >
              {error}
            </p>
          )}
        </div>
      </div>
    </div>
  );
}
