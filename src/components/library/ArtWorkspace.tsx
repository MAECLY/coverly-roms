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

function SearchGlyph() {
  return (
    <svg
      className="art-ws__search-glyph"
      viewBox="0 0 16 16"
      width="14"
      height="14"
      aria-hidden="true"
      focusable="false"
    >
      <circle
        cx="7"
        cy="7"
        r="4.5"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.5"
      />
      <path
        d="M10.5 10.5 14 14"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
      />
    </svg>
  );
}

/**
 * The drawer's art surface: one slot per media (the layout's writable ones
 * are actionable) and a single workspace for the active slot, where the
 * candidates load on their own, a title search replaces them in place and a
 * click applies one.
 */
export function ArtWorkspace({
  view,
  entry,
}: {
  view: LibraryView;
  entry: EntryView;
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
  const [active, setActive] = useState<Media | null>(
    () =>
      writable.find((m) => entry.art[m] === 'missing') ?? writable[0] ?? null,
  );
  const [lookup, setLookup] = useState<Lookup | null>(() =>
    active && entry.art[active] === 'missing' && !busy
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
  const loading = lookup !== null && settled !== lookup.seq;
  const tabRefs = useRef(new Map<Media, HTMLButtonElement>());
  const replaceRef = useRef<HTMLButtonElement>(null);
  const candidateRefs = useRef(new Map<number, HTMLButtonElement>());
  const keepRef = useRef<HTMLButtonElement>(null);
  const fileRef = useRef<HTMLButtonElement>(null);
  const removeRef = useRef<HTMLButtonElement>(null);
  const removeAsked = useRef(false);

  /* Runs the current lookup after its delay; a newer lookup (or closing the
   * drawer) cancels the timer and drops a late answer, so responses that
   * arrive out of order never overwrite the newer list. */
  useEffect(() => {
    if (!lookup) return;
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
  }, [lookup, view.generation, entry.id]);

  useEffect(() => {
    if (pending !== null) replaceRef.current?.focus();
  }, [pending]);

  // The remove question takes the focus on its safe answer and gives it
  // back to the header when it closes.
  useEffect(() => {
    if (confirmRemove) {
      removeAsked.current = true;
      keepRef.current?.focus();
    } else if (removeAsked.current) {
      removeAsked.current = false;
      const back = removeRef.current;
      (back && !back.disabled ? back : fileRef.current)?.focus();
    }
  }, [confirmRemove]);

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

  function activate(m: Media) {
    if (!canWrite(m) || m === active) return;
    setActive(m);
    setConfirmRemove(false);
    setStatus(null);
    setError(null);
    setPending(null);
    setListing(null);
    if (busy) {
      setLookup(null);
      return;
    }
    startLookup(m, 'entry', '', AUTO_LOAD_DELAY_MS);
  }

  function onTabKey(e: KeyboardEvent<HTMLButtonElement>) {
    const order = MEDIA.filter(canWrite);
    if (!active || order.length === 0) return;
    const at = order.indexOf(active);
    let next: Media | undefined;
    if (e.key === 'ArrowRight' || e.key === 'ArrowDown') {
      next = order[(at + 1) % order.length];
    } else if (e.key === 'ArrowLeft' || e.key === 'ArrowUp') {
      next = order[(at - 1 + order.length) % order.length];
    } else if (e.key === 'Home') {
      next = order[0];
    } else if (e.key === 'End') {
      next = order[order.length - 1];
    }
    if (!next) return;
    e.preventDefault();
    activate(next);
    tabRefs.current.get(next)?.focus();
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
      from.kind === 'search'
        ? t('drawer.searchUpdated', { name: c.name })
        : t('drawer.mediaUpdated', { media: mediaName(from.media) }),
    );
    if (!ok) return;
    setApplied({ seq: from.seq, index: c.index });
    if (from.kind === 'search') {
      void useIdentityStore.getState().identify(view.generation, entry.id);
    }
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
    await attempt(
      (b) => b.removeEntryArt(view.generation, entry.id, media),
      t('drawer.mediaRemoved', { media: mediaName(media) }),
    );
  }

  const panelId = `${uid}-panel`;
  const tabId = (m: Media) => `${uid}-tab-${m}`;
  const activeIndex = active ? MEDIA.indexOf(active) : -1;
  const shown = listing && listing.media === active ? listing : null;

  let body: ReactNode = null;
  if (active) {
    if (loading) {
      body = (
        <>
          <p role="status" className="art-ws__note">
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
                <button
                  type="button"
                  className="art-ws__link"
                  disabled={busy}
                  onClick={() => {
                    setTyped(null);
                    startLookup(active, 'entry', '', 0);
                  }}
                >
                  {t('drawer.backToCandidates')}
                </button>
              )}
            </div>
            <div role="group" aria-label={label} className="art-ws__grid">
              {shown.items.map((c) => {
                const isPending = pending === c.index;
                const inUse =
                  applied?.seq === shown.seq && applied.index === c.index;
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
                          <span className="art-ws__inuse">
                            {t('drawer.inUse')}
                          </span>
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
                        <div className="art-ws__row">
                          <button
                            ref={replaceRef}
                            type="button"
                            className="art-ws__btn art-ws__btn--primary"
                            onKeyDown={onConfirmKey}
                            onClick={() => void apply(c, shown)}
                          >
                            {t('drawer.replace')}
                          </button>
                          <button
                            type="button"
                            className="art-ws__btn"
                            onKeyDown={onConfirmKey}
                            onClick={cancelPending}
                          >
                            {t('drawer.cancel')}
                          </button>
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
        <button
          type="button"
          className="art-ws__more"
          disabled={busy}
          onClick={() => startLookup(active, 'entry', '', 0)}
        >
          {t('drawer.showCandidates')}
        </button>
      );
    }
  }

  return (
    <section aria-label={t('drawer.art')} className="art-ws">
      <div
        role="tablist"
        aria-label={t('drawer.art')}
        className="art-ws__slots"
      >
        {MEDIA.map((m) => {
          const mark = entry.art[m];
          const actionable = canWrite(m);
          const selected = m === active;
          const stateId = `${uid}-state-${m}`;
          return (
            <button
              key={m}
              ref={(el) => {
                if (el) tabRefs.current.set(m, el);
                else tabRefs.current.delete(m);
              }}
              id={tabId(m)}
              type="button"
              role="tab"
              className="art-ws__slot"
              data-mark={actionable ? mark : 'unsupported'}
              aria-label={mediaName(m)}
              aria-describedby={stateId}
              aria-selected={selected}
              aria-controls={selected ? panelId : undefined}
              tabIndex={selected ? 0 : -1}
              disabled={!actionable}
              onKeyDown={onTabKey}
              onClick={() => activate(m)}
            >
              <span className="art-ws__thumb">
                <ArtThumb
                  generation={view.generation}
                  entry={entry}
                  media={m}
                />
              </span>
              <span className="art-ws__label">{mediaName(m)}</span>
              <span id={stateId} className="art-ws__state">
                {actionable
                  ? t(`art.mark.${mark}`)
                  : t('drawer.slotUnsupported')}
              </span>
            </button>
          );
        })}
      </div>

      <div
        id={panelId}
        role="tabpanel"
        aria-labelledby={active ? tabId(active) : undefined}
        className="art-ws__panel"
        data-edge={
          activeIndex === 0
            ? 'start'
            : activeIndex === MEDIA.length - 1
              ? 'end'
              : undefined
        }
      >
        {!active ? (
          <p className="art-ws__note">{t('art.mark.unsupported')}</p>
        ) : (
          <>
            <div className="art-ws__head">
              {confirmRemove ? (
                <div className="art-ws__remove" role="group">
                  <p>{t('drawer.removeConfirm')}</p>
                  <div className="art-ws__row">
                    <button
                      type="button"
                      className="art-ws__btn art-ws__btn--danger"
                      onClick={() => void remove()}
                    >
                      {t('drawer.removeYes')}
                    </button>
                    <button
                      ref={keepRef}
                      type="button"
                      className="art-ws__btn"
                      onClick={() => setConfirmRemove(false)}
                    >
                      {t('drawer.removeNo')}
                    </button>
                  </div>
                </div>
              ) : (
                <>
                  <h3 className="art-ws__title">{mediaName(active)}</h3>
                  <div className="art-ws__row">
                    <button
                      ref={fileRef}
                      type="button"
                      className="art-ws__btn"
                      disabled={busy || writing}
                      onClick={() =>
                        void attempt(
                          (b) =>
                            b.setEntryArtFromFile(
                              view.generation,
                              entry.id,
                              active,
                            ),
                          t('drawer.mediaUpdated', {
                            media: mediaName(active),
                          }),
                        )
                      }
                    >
                      {t('drawer.fromFile')}
                    </button>
                    <button
                      ref={removeRef}
                      type="button"
                      className="art-ws__btn"
                      disabled={
                        busy || writing || entry.art[active] !== 'present'
                      }
                      onClick={() => setConfirmRemove(true)}
                    >
                      {t('drawer.remove')}
                    </button>
                  </div>
                </>
              )}
            </div>

            <form
              className="art-ws__search"
              role="search"
              onSubmit={(e) => {
                e.preventDefault();
                submitSearch();
              }}
            >
              <label className="art-ws__field">
                <span className="sr-only">{t('drawer.searchLabel')}</span>
                <SearchGlyph />
                <input
                  type="search"
                  name="drawer-search"
                  className="art-ws__input"
                  value={query}
                  maxLength={120}
                  disabled={busy}
                  onChange={(e) => onQueryChange(e.target.value)}
                />
              </label>
              <button
                type="submit"
                className="art-ws__btn"
                disabled={!query.trim() || busy}
              >
                {t('drawer.searchGo')}
              </button>
            </form>

            {busy && <p className="art-ws__note">{t('drawer.busy')}</p>}
            <p role="status" className="art-ws__status">
              {status}
            </p>
            {error && (
              <p role="alert" className="error-banner">
                {t(errorKey(error.code), { detail: error.detail ?? '' })}
              </p>
            )}
            {body}
          </>
        )}
      </div>
    </section>
  );
}
