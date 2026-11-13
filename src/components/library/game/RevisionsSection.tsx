import { useContext, useMemo, useState } from 'react';

import { Alert } from '@/components/ui/alert';
import { Toggle } from '@/components/ui/toggle';
import { LocaleContext } from '@/i18n/core';
import { useT } from '@/i18n/useT';
import { errorKey, toBackendError, type BackendError } from '@/ipc/errors';
import type { EntryView } from '@/ipc/types';
import { byRevisionDesc, type Game } from '@/lib/games';

/**
 * «Revisiones» in the game panel (addon views §8.8 3, D7): the files of one
 * retro game, highest revision first in a fixed order (so a choice never
 * moves the row under the pointer or the focus), with a toggle on each.
 * The default is the game's representative entry (`game.revisions[0]`);
 * `onUse` asks Rust to store another one for this library. A failure keeps
 * the previous default and says why; a success is announced politely.
 */
export function RevisionsSection({
  game,
  onUse,
}: {
  game: Game;
  onUse(entryId: number): Promise<void>;
}) {
  const t = useT();
  const locale = useContext(LocaleContext);
  const [said, setSaid] = useState('');
  const [error, setError] = useState<BackendError | null>(null);
  const rows = useMemo(
    () => [...game.revisions].sort(byRevisionDesc),
    [game.revisions],
  );
  const current = game.revisions[0]?.id;
  const label = (e: EntryView) => e.revision ?? t('panel.revUntagged');

  async function use(e: EntryView) {
    setError(null);
    setSaid('');
    try {
      await onUse(e.id);
      setSaid(t('panel.revChanged', { rev: label(e) }));
    } catch (err) {
      setError(toBackendError(err));
    }
  }

  return (
    <section
      id="panel-revisions"
      data-panel-section="revisions"
      className="panel-sec"
      aria-labelledby="panel-revisions-h"
    >
      <h3 id="panel-revisions-h" className="panel-sec__h">
        {t('panel.revisions')}
        <small>
          {t('panel.revisionsSub', {
            n: new Intl.NumberFormat(locale).format(rows.length),
          })}
        </small>
      </h3>
      <ul className="panel-list">
        {rows.map((e) => {
          const isDefault = e.id === current;
          return (
            <li key={e.id} className="panel-row">
              <span className="panel-row__main">
                <span className="panel-row__v mono">{label(e)}</span>
                <span className="panel-row__f mono" title={e.rel_path}>
                  {e.name}
                </span>
              </span>
              <Toggle
                variant="outline"
                size="sm"
                pressed={isDefault}
                aria-label={
                  isDefault
                    ? undefined
                    : t('panel.revUseLabel', { rev: label(e) })
                }
                onPressedChange={(on) => {
                  if (on && !isDefault) void use(e);
                }}
              >
                {isDefault ? t('panel.revDefault') : t('panel.revUse')}
              </Toggle>
            </li>
          );
        })}
      </ul>
      <p className="panel-note">{t('panel.revNote')}</p>
      {error && (
        <Alert variant="danger" className="mt-2">
          {t(errorKey(error.code), { detail: error.detail ?? '' })}
        </Alert>
      )}
      <p aria-live="polite" className="sr-only">
        {said}
      </p>
    </section>
  );
}
