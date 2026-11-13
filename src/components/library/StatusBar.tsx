import { Fragment, useContext, useMemo, type ReactNode } from 'react';

import { Button } from '@/components/ui/button';
import { LocaleContext } from '@/i18n/core';
import { useRichT } from '@/i18n/rich';
import { useT } from '@/i18n/useT';
import type { Game, GameCounts } from '@/lib/games';
import { useViewStore } from '@/stores/viewStore';

/** `key` or `keyOne` for a count, numbers formatted and set in mono. */
type Counted = (key: string, n: number) => ReactNode;

/**
 * What the library holds, in games (addon views D4): «9 juegos, 2 de ellos
 * demos · dentro de ellos: 5 actualizaciones, 5 DLC y 1 revisión más (no
 * cuentan como juegos)». Unfiltered it says «9 juegos»; with a scope or
 * filters «4 de 9 juegos», and the inner clause counts what the shown
 * games hold. A clause at 0 is left out. Add-ons without their game are a
 * button that opens their list.
 */
export function StatusBar({
  total,
  shown,
  filtered,
  games,
  scanMs,
}: {
  /** The whole library. */
  total: GameCounts;
  /** The games on screen (the library's when nothing narrows it). */
  shown: GameCounts;
  /** A scope or a filter narrows the games on screen. */
  filtered: boolean;
  /** The games «sin portada» counts, by their default file. */
  games: readonly Game[];
  scanMs: number;
}) {
  const t = useT();
  const tr = useRichT();
  const locale = useContext(LocaleContext);
  const setScope = useViewStore((s) => s.setScope);
  const n = useMemo(() => new Intl.NumberFormat(locale), [locale]);
  const lists = useMemo(
    () => new Intl.ListFormat(locale, { type: 'conjunction' }),
    [locale],
  );
  const missing = useMemo(
    () =>
      games.filter((g) => g.orphan === null && g.entry.art.cover === 'missing')
        .length,
    [games],
  );

  const counted: Counted = (key, count) =>
    count === 1 && key !== 'status.dlc'
      ? tr(`${key}One`)
      : tr(key, { n: n.format(count) });

  const head =
    filtered && shown.games !== total.games
      ? tr('status.gamesOf', {
          shown: n.format(shown.games),
          total: n.format(total.games),
        })
      : counted('status.games', total.games);

  const inner: ReactNode[] = [];
  if (shown.updates > 0) inner.push(counted('status.updates', shown.updates));
  if (shown.dlc > 0) inner.push(counted('status.dlc', shown.dlc));
  if (shown.moreRevisions > 0)
    inner.push(counted('status.revisions', shown.moreRevisions));

  // The list's words («y», «and» and commas) come from the locale; each
  // element keeps its mono number.
  let at = 0;
  const list = lists
    .formatToParts(inner.map(() => '\u0000'))
    .map((part, i) =>
      part.type === 'element' ? (
        <Fragment key={i}>{inner[at++]}</Fragment>
      ) : (
        <Fragment key={i}>{part.value}</Fragment>
      ),
    );
  const [before = '', after = ''] = t('status.inside').split('{list}');

  return (
    <footer className="status-bar" aria-live="polite">
      <span className="status-bar__counts" data-testid="status-counts">
        <span>
          {head}
          {shown.demos > 0 && (
            <>
              {', '}
              {counted('status.demos', shown.demos)}
            </>
          )}
        </span>
        {inner.length > 0 && (
          <>
            <span aria-hidden="true"> · </span>
            <span className="text-cr-text-secondary">
              {before}
              {list}
              {after}
            </span>
          </>
        )}
        {total.orphans > 0 && (
          <>
            <span aria-hidden="true"> · </span>
            <Button
              variant="link"
              size="sm"
              className="status-bar__orphans"
              onClick={() => setScope({ kind: 'orphans' })}
            >
              {/* One phrase: the button's flex gap stays out of it. */}
              <span>{counted('status.orphans', total.orphans)}</span>
            </Button>
          </>
        )}
      </span>
      <span>{t('status.missingCover', { n: n.format(missing) })}</span>
      <span className="text-cr-text-secondary">
        {t('status.scan', { ms: n.format(scanMs) })}
      </span>
    </footer>
  );
}
