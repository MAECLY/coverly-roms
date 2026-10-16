import { useContext, useState } from 'react';

import { CandidateImage } from '@/components/fill/CandidateImage';
import { LocaleContext } from '@/i18n/core';
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
import { formatDate, formatSize } from '@/lib/format';
import {
  identityKey,
  useIdentityStore,
  type IdentityState,
} from '@/stores/identityStore';
import { isRunning, useJobStore } from '@/stores/jobStore';
import { useLibraryStore } from '@/stores/libraryStore';
import { useSettingsStore } from '@/stores/settingsStore';

import { ArtThumb } from './ArtThumb';

function revealLabel(): string {
  const ua = typeof navigator === 'undefined' ? '' : navigator.userAgent;
  if (/Mac/i.test(ua)) return 'drawer.revealMac';
  if (/Windows/i.test(ua)) return 'drawer.revealWin';
  return 'drawer.revealOther';
}

/** A title to start a search from: the identified title, else the file
 * name without its `(…)`/`[…]` tags. */
function searchTitle(state: IdentityState | undefined, name: string): string {
  if (state?.status === 'done' && state.identity.title.trim()) {
    return state.identity.title;
  }
  return name.replace(/\s*[([][^)\]]*[)\]]/g, '').trim() || name;
}

function IdentityBlock({
  state,
  generation,
  entryId,
}: {
  state: IdentityState | undefined;
  generation: number;
  entryId: number;
}) {
  const t = useT();
  const [forgetError, setForgetError] = useState<BackendError | null>(null);
  if (!state) return <p className="hint">{t('drawer.identifyHint')}</p>;
  if (state.status === 'loading')
    return <p role="status">{t('drawer.identifying')}</p>;
  if (state.status === 'error') {
    return (
      <p role="alert" className="error-banner">
        {t(errorKey(state.error.code), { detail: state.error.detail ?? '' })}
      </p>
    );
  }
  const { identity } = state;
  const pct = Math.round(identity.confidence * 100);

  /** M4-05: drop the manual identity; the next identification is automatic. */
  async function forget() {
    setForgetError(null);
    try {
      const b = await getBackend();
      await b.clearIdentityOverride(generation, entryId);
    } catch (e) {
      setForgetError(toBackendError(e));
      return;
    }
    await useIdentityStore.getState().identify(generation, entryId);
  }

  return (
    <>
      <dl className="drawer-facts">
        <dt>{t('drawer.canonical')}</dt>
        <dd>{identity.canonical ?? t('drawer.notIdentified')}</dd>
        <dt>{t('drawer.methodLabel')}</dt>
        <dd>{t(`drawer.method.${identity.method}`)}</dd>
        <dt>{t('drawer.confidenceLabel')}</dt>
        <dd className="drawer-confidence">
          <meter
            min={0}
            max={100}
            value={pct}
            aria-label={t('drawer.confidenceLabel')}
          />
          <span>{pct}%</span>
        </dd>
      </dl>
      {identity.offline && <p className="hint">{t('drawer.offline')}</p>}
      {identity.method === 'manual' && (
        <button
          type="button"
          className="ghost-button"
          onClick={() => void forget()}
        >
          {t('drawer.forgetManual')}
        </button>
      )}
      {forgetError && (
        <p role="alert" className="error-banner">
          {t(errorKey(forgetError.code), { detail: forgetError.detail ?? '' })}
        </p>
      )}
    </>
  );
}

function DrawerActions({
  view,
  entry,
}: {
  view: LibraryView;
  entry: EntryView;
}) {
  const t = useT();
  const job = useJobStore((s) => s.job);
  const applyArtUpdates = useLibraryStore((s) => s.applyArtUpdates);
  const writable = view.profile.media;
  const [media, setMedia] = useState<Media>(writable[0] ?? 'cover');
  const [candidates, setCandidates] = useState<CandidateView[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [confirmRemove, setConfirmRemove] = useState(false);
  const [updated, setUpdated] = useState(false);
  const [error, setError] = useState<BackendError | null>(null);
  const overwrite = useSettingsStore(
    (s) => s.settings?.art.overwrite ?? 'never',
  );
  const identity = useIdentityStore(
    (s) => s.byKey[identityKey(view.generation, entry.id)],
  );
  const [searching, setSearching] = useState(false);
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<CandidateView[] | null>(null);
  const [pending, setPending] = useState<{
    kind: 'entry' | 'search';
    candidate: CandidateView;
  } | null>(null);
  const [matched, setMatched] = useState<string | null>(null);
  const busy = isRunning(job);

  async function attempt(
    f: (b: Backend) => Promise<EntryArtUpdate[] | null>,
  ): Promise<boolean> {
    setError(null);
    setUpdated(false);
    setMatched(null);
    try {
      const b = await getBackend();
      const updates = await f(b);
      if (!updates) return false;
      applyArtUpdates(view.generation, updates);
      setUpdated(true);
      return true;
    } catch (e) {
      setError(toBackendError(e));
      return false;
    }
  }

  async function listCandidates() {
    setError(null);
    setUpdated(false);
    setLoading(true);
    setCandidates(null);
    setSearching(false);
    setResults(null);
    setPending(null);
    try {
      const b = await getBackend();
      setCandidates(await b.entryCandidates(view.generation, entry.id, media));
    } catch (e) {
      setError(toBackendError(e));
    } finally {
      setLoading(false);
    }
  }

  function openSearch() {
    setSearching(true);
    setCandidates(null);
    setResults(null);
    setPending(null);
    setQuery(searchTitle(identity, entry.name));
  }

  async function runSearch() {
    if (!query.trim()) return;
    setError(null);
    setUpdated(false);
    setLoading(true);
    setResults(null);
    setPending(null);
    try {
      const b = await getBackend();
      setResults(
        await b.searchEntryArt(view.generation, entry.id, media, query),
      );
    } catch (e) {
      setError(toBackendError(e));
    } finally {
      setLoading(false);
    }
  }

  /** D25: replacing an existing image is confirmed unless the setting
   * says to replace (with a backup). */
  function pick(kind: 'entry' | 'search', candidate: CandidateView) {
    if (entry.art[media] === 'present' && overwrite !== 'backup') {
      setPending({ kind, candidate });
    } else {
      void write(kind, candidate);
    }
  }

  async function write(kind: 'entry' | 'search', c: CandidateView) {
    setPending(null);
    const ok = await attempt((b) =>
      kind === 'entry'
        ? b.setEntryArt(view.generation, entry.id, media, c.index)
        : b.setEntryArtFromSearch(view.generation, entry.id, media, c.index),
    );
    if (!ok) return;
    if (kind === 'entry') {
      setCandidates(null);
      return;
    }
    setResults(null);
    setSearching(false);
    setMatched(c.name);
    void useIdentityStore.getState().identify(view.generation, entry.id);
  }

  async function remove() {
    setConfirmRemove(false);
    await attempt((b) => b.removeEntryArt(view.generation, entry.id, media));
  }

  return (
    <>
      {writable.length > 1 && (
        <label className="field-label">
          {t('drawer.mediaLabel')}{' '}
          <select
            name="drawer-media"
            className="select"
            value={media}
            onChange={(e) => {
              setMedia(e.target.value as Media);
              setCandidates(null);
              setResults(null);
              setPending(null);
              setConfirmRemove(false);
            }}
          >
            {writable.map((m) => (
              <option key={m} value={m}>
                {t(`art.media.${m}`)}
              </option>
            ))}
          </select>
        </label>
      )}
      <div className="drawer-actions">
        <button
          type="button"
          className="ghost-button"
          disabled={busy || loading}
          onClick={() => void listCandidates()}
        >
          {t('drawer.change')}
        </button>
        <button
          type="button"
          className="ghost-button"
          disabled={busy || loading}
          onClick={openSearch}
        >
          {t('drawer.search')}
        </button>
        <button
          type="button"
          className="ghost-button"
          disabled={busy}
          onClick={() =>
            void attempt((b) =>
              b.setEntryArtFromFile(view.generation, entry.id, media),
            )
          }
        >
          {t('drawer.fromFile')}
        </button>
        <button
          type="button"
          className="ghost-button"
          disabled={busy || entry.art[media] !== 'present'}
          onClick={() => setConfirmRemove(true)}
        >
          {t('drawer.remove')}
        </button>
      </div>
      {busy && <p className="hint">{t('drawer.busy')}</p>}
      {confirmRemove && (
        <div className="undo-confirm">
          <p>{t('drawer.removeConfirm')}</p>
          <div className="modal-actions">
            <button
              type="button"
              className="ghost-button"
              onClick={() => setConfirmRemove(false)}
            >
              {t('drawer.removeNo')}
            </button>
            <button
              type="button"
              className="primary-button"
              onClick={() => void remove()}
            >
              {t('drawer.removeYes')}
            </button>
          </div>
        </div>
      )}
      {loading && <p role="status">{t('drawer.loadingCandidates')}</p>}
      {candidates &&
        (candidates.length === 0 ? (
          <p>{t('drawer.noCandidates')}</p>
        ) : (
          <div
            role="group"
            aria-label={t('drawer.candidates')}
            className="candidate-grid drawer-candidates"
          >
            {candidates.map((c) => (
              <button
                key={c.index}
                type="button"
                className="candidate"
                onClick={() => pick('entry', c)}
              >
                <CandidateImage
                  source={{
                    kind: 'entry',
                    generation: view.generation,
                    entryId: entry.id,
                    media,
                    candidate: c.index,
                  }}
                  label={c.name}
                />
                <span className="truncate" title={c.name}>
                  {c.name}
                </span>
                <span className="hint">
                  {t('review.meta', {
                    key: c.index + 1,
                    source: c.source,
                    regions:
                      c.regions.map((r) => t(`regions.${r}`)).join(', ') || '—',
                    score: Math.round(c.confidence * 100),
                  })}
                </span>
              </button>
            ))}
          </div>
        ))}
      {searching && (
        <form
          className="drawer-search"
          role="search"
          onSubmit={(e) => {
            e.preventDefault();
            void runSearch();
          }}
        >
          <label className="field-label">
            {t('drawer.searchLabel')}{' '}
            <input
              type="search"
              name="drawer-search"
              className="text-input"
              value={query}
              maxLength={120}
              onChange={(e) => setQuery(e.target.value)}
            />
          </label>
          <div className="drawer-actions">
            <button
              type="submit"
              className="ghost-button"
              disabled={!query.trim() || loading || busy}
            >
              {t('drawer.searchGo')}
            </button>
            <button
              type="button"
              className="ghost-button"
              onClick={() => {
                setSearching(false);
                setResults(null);
                setPending(null);
              }}
            >
              {t('drawer.removeNo')}
            </button>
          </div>
        </form>
      )}
      {results &&
        (results.length === 0 ? (
          <p>{t('drawer.searchNone')}</p>
        ) : (
          <div
            role="group"
            aria-label={t('drawer.searchResults')}
            className="candidate-grid drawer-candidates"
          >
            {results.map((c) => (
              <button
                key={c.index}
                type="button"
                className="candidate"
                disabled={busy}
                onClick={() => pick('search', c)}
              >
                <CandidateImage
                  source={{
                    kind: 'search',
                    generation: view.generation,
                    entryId: entry.id,
                    media,
                    candidate: c.index,
                  }}
                  label={c.name}
                />
                <span className="truncate" title={c.name}>
                  {c.name}
                </span>
                <span className="hint">
                  {t('review.meta', {
                    key: c.index + 1,
                    source: c.source,
                    regions:
                      c.regions.map((r) => t(`regions.${r}`)).join(', ') || '—',
                    score: Math.round(c.confidence * 100),
                  })}
                </span>
              </button>
            ))}
          </div>
        ))}
      {pending && (
        <div className="undo-confirm">
          <p>{t('drawer.replaceConfirm', { name: pending.candidate.name })}</p>
          <div className="modal-actions">
            <button
              type="button"
              className="ghost-button"
              onClick={() => setPending(null)}
            >
              {t('drawer.removeNo')}
            </button>
            <button
              type="button"
              className="primary-button"
              onClick={() => void write(pending.kind, pending.candidate)}
            >
              {t('drawer.replaceYes')}
            </button>
          </div>
        </div>
      )}
      {updated && (
        <p role="status" className="hint">
          {matched
            ? t('drawer.searchUpdated', { name: matched })
            : t('drawer.updated')}
        </p>
      )}
      {error && (
        <p role="alert" className="error-banner">
          {t(errorKey(error.code), { detail: error.detail ?? '' })}
        </p>
      )}
    </>
  );
}

export function DetailDrawer({
  view,
  entry,
  onClose,
}: {
  view: LibraryView;
  entry: EntryView;
  onClose(): void;
}) {
  const t = useT();
  const locale = useContext(LocaleContext);
  const identity = useIdentityStore(
    (s) => s.byKey[identityKey(view.generation, entry.id)],
  );
  const identify = useIdentityStore((s) => s.identify);
  const [revealError, setRevealError] = useState<BackendError | null>(null);
  const system = view.systems[entry.system];

  async function reveal() {
    setRevealError(null);
    try {
      const backend = await getBackend();
      await backend.revealEntry(view.generation, entry.id);
    } catch (e) {
      setRevealError(toBackendError(e));
    }
  }

  return (
    <aside className="drawer" aria-labelledby="drawer-title">
      <header className="drawer-header">
        <h2 id="drawer-title" className="drawer-title" title={entry.name}>
          {entry.name}
        </h2>
        <button
          type="button"
          className="icon-button"
          aria-label={t('drawer.close')}
          onClick={onClose}
        >
          ×
        </button>
      </header>
      <section aria-label={t('drawer.art')} className="drawer-art">
        {MEDIA.map((m) => (
          <figure key={m} className="drawer-figure">
            <ArtThumb
              generation={view.generation}
              entry={entry}
              media={m}
              className={m === 'cover' ? '' : 'art-thumb--wide'}
            />
            <figcaption>
              {t(`art.media.${m}`)} · {t(`art.mark.${entry.art[m]}`)}
            </figcaption>
          </figure>
        ))}
      </section>
      <section className="drawer-section">
        <h3 className="section-title">{t('drawer.file')}</h3>
        <dl className="drawer-facts">
          <dt>{t('drawer.path')}</dt>
          <dd className="break-all">{entry.rel_path}</dd>
          <dt>{t('columns.system')}</dt>
          <dd>{system?.name ?? ''}</dd>
          <dt>{t('drawer.ext')}</dt>
          <dd>.{entry.ext}</dd>
          <dt>{t('columns.size')}</dt>
          <dd>{formatSize(entry.size, locale)}</dd>
          <dt>{t('columns.modified')}</dt>
          <dd>{formatDate(entry.mtime, locale)}</dd>
          <dt>{t('filters.regions')}</dt>
          <dd>
            {entry.regions.length > 0
              ? entry.regions.map((r) => t(`regions.${r}`)).join(', ')
              : '—'}
          </dd>
        </dl>
      </section>
      <section className="drawer-section">
        <h3 className="section-title">{t('drawer.identity')}</h3>
        <IdentityBlock
          state={identity}
          generation={view.generation}
          entryId={entry.id}
        />
        <button
          type="button"
          className="ghost-button"
          disabled={identity?.status === 'loading'}
          onClick={() => void identify(view.generation, entry.id)}
        >
          {t('drawer.identify')}
        </button>
      </section>
      <section className="drawer-section">
        <h3 className="section-title">{t('drawer.actions')}</h3>
        <DrawerActions key={entry.id} view={view} entry={entry} />
        <div className="drawer-actions">
          <button
            type="button"
            className="ghost-button"
            onClick={() => void reveal()}
          >
            {t(revealLabel())}
          </button>
        </div>
        {revealError && (
          <p role="alert" className="error-banner">
            {t(errorKey(revealError.code), {
              detail: revealError.detail ?? '',
            })}
          </p>
        )}
      </section>
    </aside>
  );
}
