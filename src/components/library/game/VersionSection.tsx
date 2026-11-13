import { useContext } from 'react';

import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { LocaleContext } from '@/i18n/core';
import { useRichT } from '@/i18n/rich';
import { useT } from '@/i18n/useT';
import type { ChildView, GameDetailsView } from '@/ipc/types';
import { formatSize } from '@/lib/format';
import type { Game } from '@/lib/games';

/** `1.10` / `v1.10` → `v1.10`: Rust sends versions without the `v`. */
function vText(version: string): string {
  return `v${version.replace(/^v/i, '')}`;
}

/** The last component of a `/`- or `\`-separated path. */
function fileOf(relPath: string): string {
  const parts = relPath.split(/[\\/]+/).filter((p) => p !== '');
  return parts[parts.length - 1] ?? relPath;
}

/**
 * «Versión» in the game panel (addon views §8.8 2, D5, D6). Every fact is
 * Rust's: `installed` (the newest version in the library, `version_order`),
 * the children already newest first, `newer_elsewhere` (an orphan update
 * newer than the game's folder holds) and, when an offline title list
 * answered, `latest` with `latest_installed`. The webview only matches
 * texts Rust sent; it never orders versions.
 */
export function VersionSection({
  game,
  details,
  onShowElsewhere,
}: {
  game: Game;
  details: GameDetailsView | null;
  /** «Ver dónde está»: the orphan update, under «Complementos sin juego base». */
  onShowElsewhere(entryId: number): void;
}) {
  const t = useT();
  const tr = useRichT();
  const locale = useContext(LocaleContext);
  const updates = game.updates;
  // Rust leaves `installed` out when it cannot order the updates (a tie
  // by `version_order`, or one without a version): then nothing is «the
  // newest», not even the base's own version (D5).
  const big =
    game.installed ??
    (updates.length === 0 ? (game.entry.version ?? null) : null);
  const latest = details?.latest;
  const latestKnown = latest !== undefined && details?.latest_installed;
  const latestMissing =
    latest !== undefined && details?.latest_installed === false;
  const source = latest ? t(`panel.source.${latest.source}`) : '';

  const count =
    updates.length === 0
      ? t('panel.noUpdates')
      : updates.length === 1
        ? t('panel.updatesOne')
        : t('panel.updates', {
            n: new Intl.NumberFormat(locale).format(updates.length),
          });

  const isNewest = (c: ChildView) =>
    game.installed !== null &&
    c.version !== undefined &&
    c.version === game.installed;

  return (
    <section
      id="panel-version"
      data-panel-section="version"
      className="panel-sec"
      aria-labelledby="panel-version-h"
    >
      <h3 id="panel-version-h" className="panel-sec__h">
        {t('panel.version')}
        <small>{count}</small>
      </h3>
      {big !== null && (
        <p className="panel-ver">
          <span className="panel-ver__big">{vText(big)}</span>
          <span className="panel-ver__lbl">
            {t('panel.newestInLibrary')}
            {updates.length === 0 && ` ${t('panel.baseVersion')}`}
          </span>
        </p>
      )}
      {latestKnown && (
        <p className="panel-callout panel-callout--ok">
          <span aria-hidden className="panel-callout__mark">
            ✓
          </span>
          <span>{t('panel.latestOk', { source })}</span>
        </p>
      )}
      {latestMissing && latest && (
        <p className="panel-callout">
          <span aria-hidden className="panel-callout__mark">
            ↑
          </span>
          <span>
            {tr(
              'panel.latestMissing',
              { v: latest.version.replace(/^v/i, ''), source },
              ['v'],
            )}
          </span>
        </p>
      )}
      {!latest && game.missing && (
        <div className="panel-callout">
          <span aria-hidden className="panel-callout__mark">
            ↑
          </span>
          <span className="panel-callout__body">
            <span>
              {tr(
                'panel.latestElsewhere',
                { v: game.missing.version.replace(/^v/i, '') },
                ['v'],
              )}
            </span>
            <Button
              size="sm"
              variant="secondary"
              onClick={() => onShowElsewhere(game.missing!.entry)}
            >
              {t('panel.showIt')}
            </Button>
          </span>
        </div>
      )}
      <ul className="panel-list" aria-label={t('panel.versions')}>
        {latestMissing && latest && (
          <li className="panel-row panel-row--missing">
            <span className="panel-row__main">
              <span className="panel-row__v mono">{vText(latest.version)}</span>
              <span className="panel-row__f">{t('panel.notInLibrary')}</span>
            </span>
            <Badge variant="warning">
              <span aria-hidden>↑</span>
              {t('panel.missingPill')}
            </Badge>
          </li>
        )}
        {updates.map((c) => (
          <li key={c.index} className="panel-row">
            <span className="panel-row__main">
              <span className="panel-row__v mono">
                {c.version !== undefined ? vText(c.version) : c.name}
              </span>
              <span className="panel-row__f mono" title={c.rel_path}>
                {fileOf(c.rel_path)}
              </span>
            </span>
            {isNewest(c) ? (
              <Badge variant="primary">{t('panel.newestPill')}</Badge>
            ) : (
              <span className="panel-row__size mono">
                {formatSize(c.size, locale)}
              </span>
            )}
          </li>
        ))}
        <li className="panel-row">
          <span className="panel-row__main">
            <span className="panel-row__v mono">
              {game.entry.version !== undefined
                ? vText(game.entry.version)
                : '—'}
            </span>
            <span className="panel-row__f mono" title={game.entry.rel_path}>
              {fileOf(game.entry.rel_path)}
            </span>
          </span>
          <Badge variant="outline">{t('panel.basePill')}</Badge>
        </li>
      </ul>
    </section>
  );
}
