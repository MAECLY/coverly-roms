import { Search } from 'lucide-react';
import {
  useEffect,
  useId,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactNode,
} from 'react';

import { CandidateImage } from '@/components/fill/CandidateImage';
import { CandidateMeta } from '@/components/fill/CandidateMeta';
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
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { SlotMark, type SlotState } from '@/components/ui/slot-mark';
import { Spinner } from '@/components/ui/spinner';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { useT } from '@/i18n/useT';
import { getBackend, type Backend } from '@/ipc/backend';
import { errorKey, toBackendError, type BackendError } from '@/ipc/errors';
import {
  MEDIA,
  type CandidateView,
  type EntryArtUpdate,
  type EntryView,
  type LibraryView,
  type Media,
} from '@/ipc/types';
import {
  identityKey,
  useIdentityStore,
  type IdentityState,
} from '@/stores/identityStore';
import { isRunning, useJobStore } from '@/stores/jobStore';
import { useLibraryStore } from '@/stores/libraryStore';
import { useSettingsStore } from '@/stores/settingsStore';
import { cn } from '@/lib/utils';

import { ArtThumb } from './ArtThumb';

/** Quiet time before an automatic candidate lookup: browsing the library or
 * the slots with the arrow keys does not fire one request per step. */
export const AUTO_LOAD_DELAY_MS = 250;

type ListKind = 'entry' | 'search';

/** One lookup: the automatic candidates of a media, or a title search. */
interface Lookup {
  seq: number;
  media: Media;
  kind: ListKind;
  query: string;
  delay: number;
}

interface Listing {
  seq: number;
  media: Media;
  kind: ListKind;
  query: string;
  items: CandidateView[];
}

/** A title to start a search from: the identified title, else the file
 * name without its `(…)`/`[…]` tags. */
function searchTitle(state: IdentityState | undefined, name: string): string {
  if (state?.status === 'done' && state.identity.title.trim()) {
    return state.identity.title;
  }
  return name.replace(/\s*[([][^)\]]*[)\]]/g, '').trim() || name;
}

const SLOT_STATE: Record<'present' | 'missing' | 'unsupported', SlotState> = {
  present: 'filled',
  missing: 'missing',
  unsupported: 'unsupported',
};

/**
 * The drawer's art surface: one slot per media (the layout's writable ones
 * are actionable) and a single workspace for the active slot, where the
 * candidates load on their own, a title search replaces them in place and a
 * click applies one.
 */
export function ArtWorkspace({
  view,
  entry,
  initialMedia = null,
}: {
  view: LibraryView;
  entry: EntryView;
  /** The slot to open on (Art slots: Enter on a slot); else the first
   *  missing one the layout can store. */
  initialMedia?: Media | null;
}) {
  const t = useT();
  const uid = useId();
  const job = useJobStore((s) => s.job);
  const applyArtUpdates = useLibraryStore((s) => s.applyArtUpdates);
  const overwrite = useSettingsStore(
    (s) => s.settings?.art.overwrite ?? 'never',
  );
  const identity = useIdentityStore(
    (s) => s.byKey[identityKey(view.generation, entry.id)],
  );
  const busy = isRunning(job);
  const writable = view.profile.media;
  const canWrite = (m: Media) => writable.includes(m);

  const seq = useRef(0);
  const [active, setActive] = useState<Media | null>(() =>
    initialMedia !== null && writable.includes(initialMedia)
      ? initialMedia
      : (writable.find((m) => entry.art[m] === 'missing') ??
        writable[0] ??
        null),
  );
  // The slot just opened, ahead of the render: moving the focus onto it
  // (advance) makes the Radix tab report it again, which must be a no-op.
  const activeRef = useRef(active);
  const [lookup, setLookup] = useState<Lookup | null>(() =>
    active && entry.art[active] === 'missing'
      ? {
          seq: 0,
          media: active,
          kind: 'entry',
          query: '',
          delay: AUTO_LOAD_DELAY_MS,
        }
      : null,
  );
  const [listing, setListing] = useState<Listing | null>(null);
  const [settled, setSettled] = useState<number | null>(null);
  const [typed, setTyped] = useState<string | null>(null);
  const [pending, setPending] = useState<number | null>(null);
  const [applied, setApplied] = useState<{
    seq: number;
    index: number;
  } | null>(null);
  const [confirmRemove, setConfirmRemove] = useState(false);
  const [writing, setWriting] = useState(false);
  const [status, setStatus] = useState<string | null>(null);
  const [error, setError] = useState<BackendError | null>(null);

  const query = typed ?? searchTitle(identity, entry.name);
  // A lookup waits while a fill job runs and goes out once it ends.
  const loading = lookup !== null && settled !== lookup.seq && !busy;
  const tabRefs = useRef(new Map<Media, HTMLButtonElement>());
  const replaceRef = useRef<HTMLButtonElement>(null);
  const candidateRefs = useRef(new Map<number, HTMLButtonElement>());
  const keepRef = useRef<HTMLButtonElement>(null);
  const fileRef = useRef<HTMLButtonElement>(null);
  const removeRef = useRef<HTMLButtonElement>(null);
  const removeAsked = useRef(false);

  /* Runs the current lookup after its delay; a newer lookup (or closing the
   * drawer) cancels the timer and drops a late answer, so responses that
   * arrive out of order never overwrite the newer list. While a fill job
   * runs nothing goes out; the same lookup starts when the job ends. */
  useEffect(() => {
    if (!lookup || busy) return;
    let live = true;
    const timer = setTimeout(() => {
      void (async () => {
        try {
          const b = await getBackend();
          const items =
            lookup.kind === 'entry'
              ? await b.entryCandidates(view.generation, entry.id, lookup.media)
              : await b.searchEntryArt(
                  view.generation,
                  entry.id,
                  lookup.media,
                  lookup.query,
                );
          if (!live) return;
          setListing({
            seq: lookup.seq,
            media: lookup.media,
            kind: lookup.kind,
            query: lookup.query,
            items,
          });
        } catch (e) {
          if (live) setError(toBackendError(e));
        } finally {
          if (live) setSettled(lookup.seq);
        }
      })();
    }, lookup.delay);
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [lookup, busy, view.generation, entry.id]);

  useEffect(() => {
    if (pending !== null) replaceRef.current?.focus();
  }, [pending]);

  // The remove question takes the focus on its safe answer and gives it
  // back to the header when it closes; after "Remove image" that waits for
  // the write to end, since the header buttons are disabled meanwhile.
  useEffect(() => {
    if (confirmRemove) {
      removeAsked.current = true;
      keepRef.current?.focus();
    } else if (removeAsked.current && !writing) {
      removeAsked.current = false;
      const back = removeRef.current;
      (back && !back.disabled ? back : fileRef.current)?.focus();
    }
  }, [confirmRemove, writing]);

  // The replace bar that held the focus is gone: the applied image keeps it.
  useEffect(() => {
    if (applied) candidateRefs.current.get(applied.index)?.focus();
  }, [applied]);

  function startLookup(
    media: Media,
    kind: ListKind,
    q: string,
    delay: number,
  ): void {
    seq.current += 1;
    setListing(null);
    setPending(null);
    setError(null);
    setLookup({ seq: seq.current, media, kind, query: q, delay });
  }

  /** Opens a slot and starts its lookup (held back while a job runs). */
  function goTo(m: Media, kind: ListKind = 'entry', q = '') {
    activeRef.current = m;
    setActive(m);
    setConfirmRemove(false);
    setApplied(null);
    startLookup(m, kind, q, AUTO_LOAD_DELAY_MS);
  }

  function activate(m: Media) {
    if (!canWrite(m) || m === activeRef.current) return;
    setStatus(null);
    goTo(m);
  }

  /** The next writable slot still missing its image after `from`, if any. */
  function nextMissing(from: Media): Media | undefined {
    const order = MEDIA.filter(canWrite);
    const at = order.indexOf(from);
    return [...order.slice(at + 1), ...order.slice(0, at)].find(
      (m) => entry.art[m] === 'missing',
    );
  }

  /** After filling an empty slot, moves on to the next empty one (with the
   * same kind of list) so filling a ROM is one click per slot. `done` is
   * the message of the write. Returns whether it moved. */
  function advance(
    filled: Media,
    kind: ListKind,
    q: string,
    done: string,
  ): boolean {
    const next = nextMissing(filled);
    if (!next) return false;
    goTo(next, kind, q);
    setStatus(`${done} ${t('drawer.nextSlot', { next: mediaName(next) })}`);
    tabRefs.current.get(next)?.focus();
    return true;
  }

  async function attempt(
    f: (b: Backend) => Promise<EntryArtUpdate[] | null>,
    done: string,
  ): Promise<boolean> {
    setError(null);
    setStatus(null);
    setWriting(true);
    try {
      const b = await getBackend();
      const updates = await f(b);
      if (!updates) return false;
      applyArtUpdates(view.generation, updates);
      setStatus(done);
      return true;
    } catch (e) {
      setError(toBackendError(e));
      return false;
    } finally {
      setWriting(false);
    }
  }

  const mediaName = (m: Media) => t(`art.media.${m}`);

  async function apply(c: CandidateView, from: Listing) {
    setPending(null);
    const wasEmpty = entry.art[from.media] === 'missing';
    const done =
      from.kind === 'search'
        ? t('drawer.searchUpdated', { name: c.name })
        : t('drawer.mediaUpdated', { media: mediaName(from.media) });
    const ok = await attempt(
      (b) =>
        from.kind === 'entry'
          ? b.setEntryArt(view.generation, entry.id, from.media, c.index)
          : b.setEntryArtFromSearch(
              view.generation,
              entry.id,
              from.media,
              c.index,
            ),
      done,
    );
    if (!ok) return;
    if (from.kind === 'search') {
      void useIdentityStore.getState().identify(view.generation, entry.id);
    }
    if (wasEmpty && advance(from.media, from.kind, from.query, done)) return;
    setApplied({ seq: from.seq, index: c.index });
  }

  async function fromFile(media: Media) {
    const wasEmpty = entry.art[media] === 'missing';
    const done = t('drawer.mediaUpdated', { media: mediaName(media) });
    const ok = await attempt(
      (b) => b.setEntryArtFromFile(view.generation, entry.id, media),
      done,
    );
    if (!ok) return;
    setApplied(null);
    if (wasEmpty) advance(media, 'entry', '', done);
  }

  /** D25: replacing an existing image is confirmed unless the setting says
   * to replace (with a backup). An empty slot fills at once. */
  function pick(c: CandidateView, from: Listing) {
    if (busy || writing) return;
    if (entry.art[from.media] === 'present' && overwrite !== 'backup') {
      setPending(c.index);
    } else {
      void apply(c, from);
    }
  }

  function cancelPending() {
    const at = pending;
    setPending(null);
    if (at !== null) candidateRefs.current.get(at)?.focus();
  }

  /** Escape inside the replace bar cancels it (and keeps the drawer open). */
  function onConfirmKey(e: KeyboardEvent<HTMLButtonElement>) {
    if (e.key !== 'Escape') return;
    e.stopPropagation();
    cancelPending();
  }

  function submitSearch() {
    const q = query.trim();
    if (!q || !active || busy) return;
    setStatus(null);
    startLookup(active, 'search', q, 0);
  }

  function onQueryChange(value: string) {
    setTyped(value);
    // Clearing the box goes back to the automatic candidates.
    if (value === '' && active && !busy && lookup?.kind === 'search') {
      startLookup(active, 'entry', '', 0);
    }
  }

  async function remove() {
    if (!active) return;
    const media = active;
    setConfirmRemove(false);
    const ok = await attempt(
      (b) => b.removeEntryArt(view.generation, entry.id, media),
      t('drawer.mediaRemoved', { media: mediaName(media) }),
    );
    if (ok) setApplied(null);
  }

  const activeIndex = active ? MEDIA.indexOf(active) : -1;
  const shown = listing && listing.media === active ? listing : null;

  let body: ReactNode = null;
  if (active) {
    if (loading) {
      body = (
        <>
          <p role="status" className="art-ws__note flex items-center gap-2">
            <Spinner size="sm" decorative />
            {t('drawer.loadingCandidates')}
          </p>
          <div className="art-ws__grid" aria-hidden="true">
            {[0, 1, 2].map((i) => (
              <div key={i} className="art-ws__skeleton" />
            ))}
          </div>
        </>
      );
    } else if (shown) {
      const label =
        shown.kind === 'search'
          ? t('drawer.searchResults')
          : t('drawer.candidates');
      body =
        shown.items.length === 0 ? (
          <p className="art-ws__note">
            {shown.kind === 'search'
              ? t('drawer.searchNone')
              : t('drawer.noCandidates')}
          </p>
        ) : (
          <>
            <div className="art-ws__listhead">
              <span className="art-ws__caption">{label}</span>
              {shown.kind === 'search' && (
                <Button
                  variant="link"
                  size="sm"
                  disabled={busy}
                  onClick={() => {
                    setTyped(null);
                    startLookup(active, 'entry', '', 0);
                  }}
                >
                  {t('drawer.backToCandidates')}
                </Button>
              )}
            </div>
            <div role="group" aria-label={label} className="art-ws__grid">
              {shown.items.map((c) => {
                const isPending = pending === c.index;
                const inUse =
                  entry.art[shown.media] === 'present' &&
                  applied?.seq === shown.seq &&
                  applied.index === c.index;
                return (
                  <div
                    key={c.index}
                    className="art-ws__cell"
                    data-pending={isPending || undefined}
                  >
                    <button
                      ref={(el) => {
                        if (el) candidateRefs.current.set(c.index, el);
                        else candidateRefs.current.delete(c.index);
                      }}
                      type="button"
                      className="candidate art-ws__candidate"
                      aria-pressed={
                        entry.art[shown.media] === 'present'
                          ? isPending
                          : undefined
                      }
                      disabled={busy || writing}
                      onClick={() => pick(c, shown)}
                    >
                      <span
                        className="art-ws__pic"
                        data-wide={shown.media !== 'cover' || undefined}
                      >
                        <CandidateImage
                          source={{
                            kind: shown.kind,
                            generation: view.generation,
                            entryId: entry.id,
                            media: shown.media,
                            candidate: c.index,
                          }}
                          label={c.name}
                          className={
                            shown.media === 'cover' ? '' : 'art-thumb--wide'
                          }
                        />
                      </span>
                      <span className="art-ws__info">
                        <span className="art-ws__name" title={c.name}>
                          {c.name}
                        </span>
                        <CandidateMeta c={c} position={c.index + 1} compact />
                        {inUse && (
                          <Badge variant="success">{t('drawer.inUse')}</Badge>
                        )}
                      </span>
                    </button>
                    {isPending && (
                      <div
                        className="art-ws__confirm"
                        role="group"
                        aria-label={t('drawer.replaceConfirm', {
                          name: c.name,
                        })}
                      >
                        <p>{t('drawer.replaceShort')}</p>
                        <div className="flex flex-wrap gap-1.5">
                          <Button
                            ref={replaceRef}
                            variant="primary"
                            size="sm"
                            onKeyDown={onConfirmKey}
                            onClick={() => void apply(c, shown)}
                          >
                            {t('drawer.replace')}
                          </Button>
                          <Button
                            size="sm"
                            onKeyDown={onConfirmKey}
                            onClick={cancelPending}
                          >
                            {t('drawer.cancel')}
                          </Button>
                        </div>
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          </>
        );
    } else {
      body = (
        <Button
          size="sm"
          className="justify-self-start"
          disabled={busy}
          onClick={() => startLookup(active, 'entry', '', 0)}
        >
          {t('drawer.showCandidates')}
        </Button>
      );
    }
  }

  const panelClass = 'art-ws__panel';
  const edge =
    activeIndex === 0
      ? 'start'
      : activeIndex === MEDIA.length - 1
        ? 'end'
        : undefined;

  return (
    <section aria-label={t('drawer.art')} className="art-ws">
      <Tabs
        value={active ?? ''}
        onValueChange={(v) => activate(v as Media)}
        className="gap-0"
      >
        <TabsList
          aria-label={t('drawer.art')}
          className="grid grid-cols-3 gap-1 data-[orientation=horizontal]:items-stretch data-[orientation=horizontal]:gap-1 data-[orientation=horizontal]:border-b-0"
        >
          {MEDIA.map((m) => {
            const mark = entry.art[m];
            const actionable = canWrite(m);
            const shownMark = actionable ? mark : 'unsupported';
            const stateId = `${uid}-state-${m}`;
            return (
              <TabsTrigger
                key={m}
                value={m}
                ref={(el) => {
                  if (el) tabRefs.current.set(m, el);
                  else tabRefs.current.delete(m);
                }}
                data-mark={shownMark}
                aria-label={mediaName(m)}
                aria-describedby={stateId}
                disabled={!actionable}
                className={cn(
                  'grid h-auto min-w-0 content-start justify-items-stretch gap-0.5 whitespace-normal rounded-lg border border-transparent px-1.5 pb-2 pt-1.5 text-left font-normal text-cr-text after:hidden',
                  'hover:bg-cr-hover disabled:text-cr-text disabled:opacity-50',
                  'data-[state=active]:z-[1] data-[state=active]:border-[color:var(--ws-edge)] data-[state=active]:border-b-[color:var(--ws-tint)] data-[state=active]:bg-[var(--ws-tint)]',
                )}
              >
                <span className="art-ws__thumb">
                  <ArtThumb
                    generation={view.generation}
                    entry={entry}
                    media={m}
                  />
                </span>
                <span className="mt-1 text-ui font-semibold leading-tight [overflow-wrap:anywhere]">
                  {mediaName(m)}
                </span>
                <span
                  id={stateId}
                  className="flex items-center gap-1.5 text-caption leading-tight text-cr-text-secondary"
                >
                  <SlotMark state={SLOT_STATE[shownMark]} size={11} />
                  {actionable
                    ? t(`art.mark.${mark}`)
                    : t('drawer.slotUnsupported')}
                </span>
              </TabsTrigger>
            );
          })}
        </TabsList>

        {!active ? (
          <div className={panelClass}>
            <p className="art-ws__note">{t('art.mark.unsupported')}</p>
          </div>
        ) : (
          <TabsContent value={active} className={panelClass} data-edge={edge}>
            <div className="art-ws__head">
              <h3 className="art-ws__title">{mediaName(active)}</h3>
              <div className="flex flex-wrap gap-1.5">
                <Button
                  ref={fileRef}
                  size="sm"
                  disabled={busy || writing}
                  onClick={() => void fromFile(active)}
                >
                  {t('drawer.fromFile')}
                </Button>
                <Button
                  ref={removeRef}
                  size="sm"
                  disabled={busy || writing || entry.art[active] !== 'present'}
                  onClick={() => setConfirmRemove(true)}
                >
                  {t('drawer.remove')}
                </Button>
              </div>
            </div>

            <form
              className="flex gap-1.5"
              role="search"
              onSubmit={(e) => {
                e.preventDefault();
                submitSearch();
              }}
            >
              <label className="relative flex min-w-0 flex-1 items-center">
                <span className="sr-only">{t('drawer.searchLabel')}</span>
                <Search
                  aria-hidden
                  className="pointer-events-none absolute left-2 size-3.5 text-cr-text-secondary"
                />
                <Input
                  type="search"
                  name="drawer-search"
                  size="sm"
                  className="pl-7"
                  value={query}
                  maxLength={120}
                  disabled={busy}
                  onChange={(e) => onQueryChange(e.target.value)}
                />
              </label>
              <Button type="submit" size="sm" disabled={!query.trim() || busy}>
                {t('drawer.searchGo')}
              </Button>
            </form>

            {busy && <p className="art-ws__note">{t('drawer.busy')}</p>}
            <p role="status" className="art-ws__status">
              {status}
            </p>
            {error && (
              <Alert variant="danger">
                {t(errorKey(error.code), { detail: error.detail ?? '' })}
              </Alert>
            )}
            {body}
          </TabsContent>
        )}
      </Tabs>

      <AlertDialog
        open={confirmRemove}
        onOpenChange={(open) => setConfirmRemove(open)}
      >
        <AlertDialogContent
          // Focus goes back to the header (see the effect above), not to
          // whatever Radix remembers: Remove may be disabled by then.
          onCloseAutoFocus={(e) => e.preventDefault()}
          onOpenAutoFocus={(e) => {
            e.preventDefault();
            keepRef.current?.focus();
          }}
        >
          <AlertDialogHeader>
            <AlertDialogTitle>{t('drawer.removeTitle')}</AlertDialogTitle>
            <AlertDialogDescription>
              {active ? `${mediaName(active)}. ` : ''}
              {t('drawer.replaceShort')}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel ref={keepRef}>
              {t('drawer.removeNo')}
            </AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              onClick={() => void remove()}
            >
              {t('drawer.removeYes')}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </section>
  );
}
