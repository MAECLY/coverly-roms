import { useContext, useMemo } from 'react';

import { LocaleContext } from '@/i18n/core';
import { useT } from '@/i18n/useT';
import type { ScanProgress } from '@/ipc/types';

export function ScanProgressView({
  path,
  progress,
}: {
  path: string | null;
  progress: ScanProgress | null;
}) {
  const t = useT();
  const locale = useContext(LocaleContext);
  const n = useMemo(() => new Intl.NumberFormat(locale), [locale]);
  return (
    <div role="status" className="scan-progress">
      <p>
        {progress && progress.done > 0
          ? t('start.scanning', {
              folder: progress.folder,
              done: n.format(progress.done),
              total: n.format(progress.total),
            })
          : t('start.scanningStart')}
      </p>
      <progress
        aria-label={t('start.progressLabel')}
        max={progress?.total || 1}
        value={progress?.done ?? 0}
      />
      {progress && (
        <p className="hint">
          {t('start.romsFound', { n: n.format(progress.roms) })}
        </p>
      )}
      {path && (
        <p className="hint truncate" title={path}>
          {path}
        </p>
      )}
    </div>
  );
}
