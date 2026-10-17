import { useContext, useState } from 'react';

import { LocaleContext } from '@/i18n/core';
import { useT } from '@/i18n/useT';
import { getBackend } from '@/ipc/backend';
import { errorKey, toBackendError, type BackendError } from '@/ipc/errors';
import type { EntryView, LibraryView } from '@/ipc/types';
import { formatDate, formatSize } from '@/lib/format';
import {
  identityKey,
  useIdentityStore,
  type IdentityState,
} from '@/stores/identityStore';

import { ArtWorkspace } from './ArtWorkspace';

function revealLabel(): string {
  const ua = typeof navigator === 'undefined' ? '' : navigator.userAgent;
  if (/Mac/i.test(ua)) return 'drawer.revealMac';
  if (/Windows/i.test(ua)) return 'drawer.revealWin';
  return 'drawer.revealOther';
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
          className="art-ws__btn"
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
          className="icon-button drawer-reveal"
          aria-label={t(revealLabel())}
          title={t(revealLabel())}
          onClick={() => void reveal()}
        >
          <svg
            viewBox="0 0 16 16"
            width="15"
            height="15"
            aria-hidden="true"
            focusable="false"
          >
            <path
              d="M1.75 4.25v8a1 1 0 0 0 1 1h10.5a1 1 0 0 0 1-1v-6.5a1 1 0 0 0-1-1H8L6.5 3.25H2.75a1 1 0 0 0-1 1Z"
              fill="none"
              stroke="currentColor"
              strokeWidth="1.4"
              strokeLinejoin="round"
            />
          </svg>
        </button>
        <button
          type="button"
          className="icon-button"
          aria-label={t('drawer.close')}
          onClick={onClose}
        >
          ×
        </button>
      </header>
      {revealError && (
        <p role="alert" className="error-banner drawer-reveal-error">
          {t(errorKey(revealError.code), {
            detail: revealError.detail ?? '',
          })}
        </p>
      )}
      <ArtWorkspace
        key={`${view.generation}:${entry.id}`}
        view={view}
        entry={entry}
      />
      <section className="drawer-section drawer-section--quiet">
        <h3 className="drawer-subtitle">{t('drawer.identity')}</h3>
        <IdentityBlock
          state={identity}
          generation={view.generation}
          entryId={entry.id}
        />
        <button
          type="button"
          className="art-ws__btn"
          disabled={identity?.status === 'loading'}
          onClick={() => void identify(view.generation, entry.id)}
        >
          {t('drawer.identify')}
        </button>
      </section>
      <section className="drawer-section drawer-section--quiet">
        <h3 className="drawer-subtitle">{t('drawer.file')}</h3>
        <dl className="drawer-facts drawer-facts--compact">
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
    </aside>
  );
}
