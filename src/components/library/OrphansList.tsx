import { useId } from 'react';

import { Button } from '@/components/ui/button';
import { useT } from '@/i18n/useT';
import type { SystemView } from '@/ipc/types';
import type { Game } from '@/lib/games';
import { displayVersion } from '@/lib/versions';
import { useSelectionStore } from '@/stores/selectionStore';
import { useViewStore } from '@/stores/viewStore';

/**
 * «Complementos sin juego base» (addon views D13): updates and DLC with no
 * game in their folder. A game-less add-on has no cover to tile, so every
 * view mode shows this one list while the scope is `orphans`. Read-only:
 * Organizar is what moves them (§14 Q4). When Rust linked one to a game by
 * its title (D6), «Ver el juego» selects that game in the whole library.
 */
export function OrphansList({
  orphans,
  systems,
  byEntry,
}: {
  orphans: readonly Game[];
  systems: readonly SystemView[];
  /** Every entry id → its game, to find the game an orphan belongs to. */
  byEntry: ReadonlyMap<number, Game>;
}) {
  const t = useT();
  const titleId = useId();
  const select = useSelectionStore((s) => s.select);
  const setScope = useViewStore((s) => s.setScope);

  function showGame(game: Game) {
    setScope({ kind: 'all' });
    select(game.id);
  }

  return (
    <section className="orphans-list" aria-labelledby={titleId}>
      <h2 id={titleId} className="orphans-list__title">
        {t('orphans.title')}
      </h2>
      <table className="orphans-list__table" aria-labelledby={titleId}>
        <thead>
          <tr>
            <th scope="col">{t('orphans.kind')}</th>
            <th scope="col">{t('layouts.column.name')}</th>
            <th scope="col">{t('layouts.column.version')}</th>
            <th scope="col">{t('layouts.column.folder')}</th>
            <th scope="col">{t('orphans.game')}</th>
          </tr>
        </thead>
        <tbody>
          {orphans.map((o) => {
            const e = o.entry;
            const owner =
              e.orphan_of === undefined ? undefined : byEntry.get(e.orphan_of);
            const system = systems[e.system]?.name;
            return (
              <tr key={e.id} id={`entry-${String(e.id)}`}>
                <td>{o.orphan && t(`content.kind.${o.orphan}`)}</td>
                <th scope="row" className="orphans-list__name">
                  <span title={e.name}>{e.name}</span>
                  {system && (
                    <span className="orphans-list__system">{system}</span>
                  )}
                </th>
                <td>
                  {e.version === undefined ? (
                    <span aria-hidden="true">—</span>
                  ) : (
                    <span className="mono tabular-nums">
                      {displayVersion(e.version)}
                    </span>
                  )}
                </td>
                <td>
                  <span className="mono orphans-list__folder" title={e.dir}>
                    {e.dir}
                  </span>
                </td>
                <td>
                  {owner ? (
                    <span className="orphans-list__owner">
                      <span>
                        {t('orphans.ofGame', { name: owner.entry.name })}
                      </span>
                      <Button
                        variant="link"
                        size="sm"
                        onClick={() => showGame(owner)}
                      >
                        {t('orphans.showGame')}
                      </Button>
                    </span>
                  ) : (
                    <span className="text-cr-text-secondary">
                      {t('orphans.noGame')}
                    </span>
                  )}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </section>
  );
}
