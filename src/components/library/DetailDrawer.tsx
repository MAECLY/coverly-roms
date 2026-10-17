import { FolderOpen, X } from 'lucide-react';
import { useContext, useState } from 'react';

import { Alert } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import { Spinner } from '@/components/ui/spinner';
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
import { Tip } from './Tip';

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
  if (!state)
    return (
      <p className="m-0 mb-2 text-ui text-cr-text-secondary">
        {t('drawer.identifyHint')}
      </p>
    );
  if (state.status === 'loading')
    return (
      <p role="status" className="m-0 mb-2 flex items-center gap-2 text-ui">
        <Spinner size="sm" decorative />
        {t('drawer.identifying')}
      </p>
    );
  if (state.status === 'error') {
    return (
      <Alert variant="danger" className="mb-2">
        {t(errorKey(state.error.code), { detail: state.error.detail ?? '' })}
      </Alert>
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
      {identity.offline && (
        <p className="m-0 mb-2 text-ui text-cr-text-secondary">
          {t('drawer.offline')}
        </p>
      )}
      {identity.method === 'manual' && (
        <Button size="sm" className="mb-2" onClick={() => void forget()}>
          {t('drawer.forgetManual')}
        </Button>
      )}
      {forgetError && (
        <Alert variant="danger" className="mb-2">
          {t(errorKey(forgetError.code), { detail: forgetError.detail ?? '' })}
        </Alert>
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
        <Tip content={t(revealLabel())}>
          <Button
            variant="ghost"
            size="icon-sm"
            className="text-cr-text-secondary hover:text-cr-text"
            aria-label={t(revealLabel())}
            onClick={() => void reveal()}
          >
            <FolderOpen aria-hidden />
          </Button>
        </Tip>
        <Tip content={t('drawer.close')}>
          <Button
            variant="ghost"
            size="icon-sm"
            className="text-cr-text-secondary hover:text-cr-text"
            aria-label={t('drawer.close')}
            onClick={onClose}
          >
            <X aria-hidden />
          </Button>
        </Tip>
      </header>
      {revealError && (
        <Alert variant="danger" className="mt-2">
          {t(errorKey(revealError.code), {
            detail: revealError.detail ?? '',
          })}
        </Alert>
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
        <Button
          size="sm"
          disabled={identity?.status === 'loading'}
          onClick={() => void identify(view.generation, entry.id)}
        >
          {t('drawer.identify')}
        </Button>
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
