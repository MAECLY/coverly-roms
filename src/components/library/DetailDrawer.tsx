import { FolderOpen, X } from 'lucide-react';
import { useContext, useEffect, useRef, useState } from 'react';

import { SystemLabel } from '@/components/system/SystemLabel';
import { Alert } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import { Spinner } from '@/components/ui/spinner';
import { LocaleContext } from '@/i18n/core';
import { useT } from '@/i18n/useT';
import { getBackend } from '@/ipc/backend';
import { errorKey, toBackendError, type BackendError } from '@/ipc/errors';
import type {
  EntryView,
  GameDetailsView,
  LibraryView,
  Media,
} from '@/ipc/types';
import { formatDate, formatSize } from '@/lib/format';
import type { Game } from '@/lib/games';
import { revealLabelKey } from '@/lib/reveal';
import {
  identityKey,
  useIdentityStore,
  type IdentityState,
} from '@/stores/identityStore';
import { useLibraryStore } from '@/stores/libraryStore';
import { useSelectionStore } from '@/stores/selectionStore';
import { useViewStore } from '@/stores/viewStore';

import { ArtWorkspace } from './ArtWorkspace';
import { DemoSection } from './game/DemoSection';
import { DlcSection } from './game/DlcSection';
import { FixedFoldersSection } from './game/FixedFoldersSection';
import { RegionLine } from './game/RegionLine';
import { RevisionsSection } from './game/RevisionsSection';
import { hasVersionFacts } from './game/versionFacts';
import { VersionSection } from './game/VersionSection';
import { Tip } from './Tip';
import { usePanelSection } from './usePanelSection';

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

let detailsSeq = 0;

/**
 * The game panel's lazy, offline facts (part C D3) for the selected entry.
 * Each request carries its own id and only the answer to the latest one is
 * kept: a slow answer for a game the user already left is dropped. A
 * failure leaves no facts (the panel then shows none of them).
 */
function useGameDetails(
  generation: number,
  entryId: number,
): GameDetailsView | null {
  const [state, setState] = useState<{
    key: string;
    details: GameDetailsView | null;
  } | null>(null);
  const key = `${generation}:${entryId}`;
  useEffect(() => {
    const request = (detailsSeq += 1);
    let live = true;
    void (async () => {
      try {
        const backend = await getBackend();
        const details = await backend.gameDetails(generation, entryId, request);
        if (live && details.request === request) setState({ key, details });
      } catch {
        if (live) setState({ key, details: null });
      }
    })();
    return () => {
      live = false;
    };
  }, [generation, entryId, key]);
  return state?.key === key ? state.details : null;
}

/**
 * The details of one entry, in a pane on the right. Opened with Enter or a
 * click; `docked` (the Inspector switch, ⌘I) keeps it open and following the
 * selection, so it never takes the focus from the view and Escape leaves it
 * in place. Its close button then undocks it (`onClose`).
 */
export function DetailDrawer({
  view,
  entry,
  game,
  docked = false,
  workspaceMedia = null,
  fresh,
  onClose,
}: {
  view: LibraryView;
  entry: EntryView;
  /** The game the entry stands for (addon views D1): its updates, DLC and
   * revisions. Absent, the panel shows the entry alone. */
  game?: Game | undefined;
  docked?: boolean;
  /** Open the art workspace on this slot (Art slots view). */
  workspaceMedia?: Media | null;
  /** Children tagged «Nuevo» (D18), by `rel_path`. */
  fresh?: ReadonlySet<string> | undefined;
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
  const asideRef = useRef<HTMLElement>(null);
  usePanelSection(asideRef);
  const details = useGameDetails(view.generation, entry.id);
  const setDefaultRevision = useLibraryStore((s) => s.setDefaultRevision);
  const applyChildren = useLibraryStore((s) => s.applyChildren);

  /** A demo or its full game: the panel follows the selection. */
  function selectEntry(id: number) {
    useSelectionStore.getState().select(id);
  }

  /** «Ver dónde está»: the orphan update, in «Complementos sin juego base». */
  function showElsewhere(id: number) {
    useViewStore.getState().setScope({ kind: 'orphans' });
    useSelectionStore.getState().select(id);
  }

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
    <aside
      ref={asideRef}
      className="drawer"
      aria-labelledby="drawer-title"
      data-docked={docked ? 'true' : undefined}
    >
      <header className="drawer-header">
        <h2 id="drawer-title" className="drawer-title" title={entry.name}>
          {entry.name}
        </h2>
        <Tip content={t(revealLabelKey())}>
          <Button
            variant="ghost"
            size="icon-sm"
            className="text-cr-text-secondary hover:text-cr-text"
            aria-label={t(revealLabelKey())}
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
      <RegionLine
        systemName={system?.name ?? ''}
        nameHasRegion={entry.regions.length > 0}
        details={details}
      />
      {game && hasVersionFacts(game, details) && (
        <VersionSection
          game={game}
          details={details}
          onShowElsewhere={showElsewhere}
        />
      )}
      {game && game.revisions.length > 1 && (
        <RevisionsSection game={game} onUse={setDefaultRevision} />
      )}
      {game && game.dlc.length > 0 && (
        <DlcSection
          generation={view.generation}
          game={game}
          systemId={system?.system_id}
          fresh={fresh}
          onChildren={(children) =>
            applyChildren(view.generation, game.entry.id, children)
          }
        />
      )}
      {game && (
        <DemoSection
          generation={view.generation}
          game={game}
          entries={view.entries}
          systemId={system?.system_id}
          onSelect={selectEntry}
        />
      )}
      <FixedFoldersSection entry={game?.entry ?? entry} />
      <ArtWorkspace
        key={`${view.generation}:${entry.id}:${workspaceMedia ?? ''}`}
        view={view}
        entry={entry}
        initialMedia={workspaceMedia}
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
          <dd>
            <SystemLabel
              systemId={system?.system_id ?? null}
              name={system?.name ?? ''}
              size={20}
            />
          </dd>
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
