import { useContext, useState } from 'react';

import { Alert } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import { LocaleContext } from '@/i18n/core';
import { useT } from '@/i18n/useT';
import { getBackend } from '@/ipc/backend';
import { errorKey, toBackendError, type BackendError } from '@/ipc/errors';
import type { ChildView } from '@/ipc/types';
import {
  canReadChildArt,
  childArtState,
  childQuery,
  ORIGIN_KEY,
} from '@/lib/childArt';
import type { Game } from '@/lib/games';
import { cn } from '@/lib/utils';

import { ChildArtSearch } from './ChildArtSearch';
import { ChildThumb } from './ChildThumb';

/** The last component of a `/`- or `\`-separated path. */
function fileOf(relPath: string): string {
  const parts = relPath.split(/[\\/]+/).filter((p) => p !== '');
  return parts[parts.length - 1] ?? relPath;
}

/**
 * «DLC» in the game panel (addon views §8.8 4, D11): each DLC with its art
 * (own, the base game's dimmed, or the dashed slot), its name, its file
 * in mono and where its art came from in words; «Nuevo» for content the
 * last open had not seen. «Buscar arte» opens a search aimed at that DLC
 * that writes only on a pick; «Leer el arte incluido» reads what the
 * packages carry in the clear into empty slots, as an undoable job. A
 * library that refuses the write keeps the button off, with the reason.
 */
export function DlcSection({
  generation,
  game,
  systemId,
  fresh,
  onChildren,
}: {
  generation: number;
  game: Game;
  systemId: string | null | undefined;
  /** `rel_path`s tagged «Nuevo» (D18). */
  fresh?: ReadonlySet<string> | undefined;
  /** The game's children with new art marks (after a read or a pick). */
  onChildren(children: ChildView[]): void;
}) {
  const t = useT();
  const locale = useContext(LocaleContext);
  const entry = game.entry;
  // What was said, what failed and which search is open belong to one
  // game: the panel keeps this component when the selection moves.
  const [state, setState] = useState<{
    game: number;
    said: string;
    error: BackendError | null;
    denied: BackendError | null;
    searching: number | null;
    reading: boolean;
  }>({
    game: game.id,
    said: '',
    error: null,
    denied: null,
    searching: null,
    reading: false,
  });
  const mine =
    state.game === game.id
      ? state
      : {
          game: game.id,
          said: '',
          error: null,
          denied: null,
          searching: null,
          reading: false,
        };
  const patch = (p: Partial<typeof mine>) =>
    setState((s) => ({
      ...(s.game === game.id ? s : mine),
      ...p,
      game: game.id,
    }));

  const n = game.dlc.length;
  const sub =
    n === 1
      ? t('panel.dlcSubOne')
      : t('panel.dlcSub', { n: new Intl.NumberFormat(locale).format(n) });
  const offerRead = canReadChildArt(game, systemId);

  async function readArt() {
    const id = game.id;
    patch({ reading: true, error: null, said: '' });
    try {
      const b = await getBackend();
      const out = await b.readChildArt(generation, entry.id);
      onChildren(out.children);
      const said =
        out.written === 0
          ? t('panel.readArtNone')
          : out.written === 1
            ? t('panel.readArtDoneOne')
            : t('panel.readArtDone', {
                n: new Intl.NumberFormat(locale).format(out.written),
              });
      setState((s) => (s.game === id ? { ...s, reading: false, said } : s));
    } catch (e) {
      const err = toBackendError(e);
      setState((s) =>
        s.game === id
          ? {
              ...s,
              reading: false,
              ...(err.code === 'write-denied'
                ? { denied: err }
                : { error: err }),
            }
          : s,
      );
    }
  }

  function origin(c: ChildView): { text: string; warn: boolean } {
    const art = childArtState(c, systemId);
    if (art.kind === 'own')
      return {
        text: art.origin
          ? t(`content.origin.${ORIGIN_KEY[art.origin]}`)
          : t('content.slot.icon'),
        warn: false,
      };
    if (art.kind === 'base')
      return { text: t('content.origin.base'), warn: false };
    return { text: t('content.noArt'), warn: true };
  }

  return (
    <section
      id="panel-dlc"
      data-panel-section="dlc"
      className="panel-sec"
      aria-labelledby="panel-dlc-h"
    >
      <h3 id="panel-dlc-h" className="panel-sec__h">
        {t('panel.dlc')}
        <small>{sub}</small>
      </h3>
      <ul className="panel-list">
        {game.dlc.map((c) => {
          const o = origin(c);
          const isNew = fresh?.has(c.rel_path) ?? false;
          const open = mine.searching === c.index;
          return (
            <li
              key={c.index}
              data-panel-section={`child-${c.index}`}
              className="panel-child"
            >
              <div className="panel-row panel-row--child">
                <ChildThumb
                  generation={generation}
                  entry={entry}
                  child={c}
                  systemId={systemId}
                  size={48}
                />
                <span className="panel-row__main">
                  <span className="panel-row__v" title={c.name}>
                    {c.name}
                  </span>
                  <span className="panel-row__f mono" title={c.rel_path}>
                    {fileOf(c.rel_path)}
                  </span>
                  <span
                    className={cn(
                      'panel-row__origin',
                      o.warn && 'panel-row__origin--warn',
                    )}
                  >
                    {o.text}
                    {isNew && (
                      <span className="addon-new ml-1.5">
                        {t('content.new')}
                      </span>
                    )}
                  </span>
                </span>
                <Button
                  size="sm"
                  variant="secondary"
                  aria-expanded={open}
                  aria-label={t('panel.searchArtLabel', { name: c.name })}
                  onClick={() => patch({ searching: open ? null : c.index })}
                >
                  {t('panel.searchArt')}
                </Button>
              </div>
              {open && (
                <ChildArtSearch
                  generation={generation}
                  entry={entry}
                  child={c}
                  initialQuery={childQuery(entry.name, c.name)}
                  onWritten={(children) => {
                    onChildren(children);
                    patch({
                      searching: null,
                      said: t('panel.searchArtDone', { name: c.name }),
                    });
                  }}
                  onClose={() => patch({ searching: null })}
                />
              )}
            </li>
          );
        })}
      </ul>
      {offerRead && (
        <div className="panel-actions">
          <Button
            size="sm"
            disabled={mine.reading || mine.denied !== null}
            aria-describedby={
              mine.denied ? `panel-dlc-denied-${game.id}` : undefined
            }
            onClick={() => void readArt()}
          >
            {t('panel.readArt')}
          </Button>
          {mine.denied && (
            <p id={`panel-dlc-denied-${game.id}`} className="panel-note">
              {t(errorKey(mine.denied.code), {
                detail: mine.denied.detail ?? '',
              })}
            </p>
          )}
        </div>
      )}
      {mine.error && (
        <Alert variant="danger" className="mt-2">
          {t(errorKey(mine.error.code), { detail: mine.error.detail ?? '' })}
        </Alert>
      )}
      <p aria-live="polite" className="sr-only">
        {mine.said}
      </p>
    </section>
  );
}
