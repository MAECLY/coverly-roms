import { useContext, useId } from 'react';

import { Button } from '@/components/ui/button';
import { LocaleContext } from '@/i18n/core';
import { useRichT } from '@/i18n/rich';
import { useT } from '@/i18n/useT';
import type { MissingView } from '@/ipc/types';
import { formatSize } from '@/lib/format';

/**
 * «No encontrados (n)» (extras spec §8.2, D4): records of `extras.json`
 * whose file is no longer in the folder, kept with their tags until the
 * user looks again or forgets them. Warning tone; never removed silently.
 */
export function ExtrasMissing({
  missing,
  canWrite,
  onRescan,
  onForget,
}: {
  missing: readonly MissingView[];
  canWrite: boolean;
  onRescan(): void;
  onForget(index: number): void;
}) {
  const t = useT();
  const tr = useRichT();
  const locale = useContext(LocaleContext);
  const id = useId();
  return (
    <div
      role="group"
      aria-labelledby={`${id}-h`}
      className="extras-band extras-missing"
    >
      <div className="extras-band__head">
        <h4 id={`${id}-h`} className="extras-band__name">
          {tr('extras.missing', { n: missing.length })}
        </h4>
        <Button
          variant="ghost"
          size="sm"
          className="extras-missing__rescan"
          onClick={onRescan}
        >
          {t('extras.rescan')}
        </Button>
      </div>
      <p className="panel-note extras-band__note">{t('extras.missingNote')}</p>
      <ul className="extras-list">
        {missing.map((m) => (
          <li key={m.index} className="extras-row extras-row--missing">
            <div className="extras-row__main">
              <span className="extras-row__name extras-row__name--static">
                <span className="extras-row__head" title={m.path}>
                  {m.name}
                </span>
              </span>
              {m.path.includes('/') && (
                <span className="extras-row__sub mono">
                  {m.path.slice(0, m.path.lastIndexOf('/') + 1)}
                </span>
              )}
              {m.tags.length > 0 && (
                <ul className="extras-tags">
                  {m.tags.map((tag) => (
                    <li key={tag} className="extras-tag">
                      {tag}
                    </li>
                  ))}
                </ul>
              )}
            </div>
            <span className="extras-row__size mono">
              {formatSize(m.size, locale)}
            </span>
            <Button
              variant="ghost"
              size="sm"
              disabled={!canWrite}
              aria-label={t('extras.forgetFile', { name: m.name })}
              onClick={() => onForget(m.index)}
            >
              {t('extras.forget')}
            </Button>
          </li>
        ))}
      </ul>
    </div>
  );
}
