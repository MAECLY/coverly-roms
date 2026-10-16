import { useContext, useState } from 'react';

import { LocaleContext } from '@/i18n/core';
import { useT } from '@/i18n/useT';
import { getBackend } from '@/ipc/backend';
import { errorKey, toBackendError, type BackendError } from '@/ipc/errors';
import { MEDIA, type EntryView, type LibraryView } from '@/ipc/types';
import { formatDate, formatSize } from '@/lib/format';
import {
  identityKey,
  useIdentityStore,
  type IdentityState,
} from '@/stores/identityStore';

import { ArtThumb } from './ArtThumb';

const LATER_ACTIONS = ['change', 'search', 'fromFile', 'remove'] as const;

function revealLabel(): string {
  const ua = typeof navigator === 'undefined' ? '' : navigator.userAgent;
  if (/Mac/i.test(ua)) return 'drawer.revealMac';
  if (/Windows/i.test(ua)) return 'drawer.revealWin';
  return 'drawer.revealOther';
}

function IdentityBlock({ state }: { state: IdentityState | undefined }) {
  const t = useT();
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
        <IdentityBlock state={identity} />
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
        <div className="drawer-actions">
          {LATER_ACTIONS.map((a) => (
            <button
              key={a}
              type="button"
              className="ghost-button"
              disabled
              aria-describedby="drawer-later"
            >
              {t(`drawer.${a}`)}
            </button>
          ))}
          <button
            type="button"
            className="ghost-button"
            onClick={() => void reveal()}
          >
            {t(revealLabel())}
          </button>
        </div>
        <p id="drawer-later" className="hint">
          {t('drawer.laterHint')}
        </p>
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
