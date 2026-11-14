import { useContext } from 'react';

import { Button } from '@/components/ui/button';
import { LocaleContext } from '@/i18n/core';
import { useT } from '@/i18n/useT';
import type { EntryView } from '@/ipc/types';
import { useViewStore } from '@/stores/viewStore';

/** The count Rust stops at per folder (D9): then it says «más de». */
export const FOLDER_COUNT_CAP = 10_000;

const FOLDERS = ['mods', 'cheats', 'extras', 'saves'] as const;

/**
 * «Mods, trucos, extras y partidas» in the game panel (addon views §8.8 6,
 * D9): read-only file counts of the game folder's fixed folders, collapsed
 * by default. No action writes from here: «Ver extras» scrolls to the
 * Extras section (part D) and saves arrive with block 5, which the
 * placeholder says.
 */
export function FixedFoldersSection({ entry }: { entry: EntryView }) {
  const t = useT();
  const locale = useContext(LocaleContext);
  const counts = entry.addons;
  if (!counts) return null;
  const num = new Intl.NumberFormat(locale);
  const folder = entry.folder_root ? entry.rel_path : entry.dir;

  const text = (n: number) => {
    if (counts.capped && n >= FOLDER_COUNT_CAP)
      return t('panel.filesMany', { n: num.format(FOLDER_COUNT_CAP) });
    if (n === 0) return t('panel.folderEmpty');
    if (n === 1) return t('panel.filesOne');
    return t('panel.files', { n: num.format(n) });
  };

  return (
    <details
      id="panel-folders"
      data-panel-section="folders"
      className="panel-sec panel-folders"
    >
      <summary className="panel-folders__summary">
        <h3 className="panel-sec__h">{t('panel.folders')}</h3>
      </summary>
      <dl className="panel-folders__grid">
        {FOLDERS.map((f) => (
          <div
            key={f}
            className="panel-folders__cell"
            data-empty={counts[f] === 0 || undefined}
          >
            <dt>{t(`gameFolder.${f}`)}</dt>
            {f === 'extras' ? (
              <dd className="panel-folders__extras">
                <span className="mono">{text(counts[f])}</span>
                <Button
                  variant="link"
                  size="sm"
                  onClick={() => useViewStore.getState().openDrawer('extras')}
                >
                  {t('extras.seeExtras')}
                </Button>
              </dd>
            ) : (
              <dd className="mono">{text(counts[f])}</dd>
            )}
          </div>
        ))}
      </dl>
      <p className="panel-note">{t('panel.savesLater')}</p>
      <p className="panel-folders__path mono" title={folder}>
        <span className="sr-only">{t('panel.gameFolder')}: </span>
        {folder}
      </p>
    </details>
  );
}
