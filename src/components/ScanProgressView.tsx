import { useContext, useMemo } from 'react';

import { Progress } from '@/components/ui/progress';
import { LocaleContext } from '@/i18n/core';
import { useT } from '@/i18n/useT';
import type { ScanProgress } from '@/ipc/types';

/** One slot per step of the scan, filling left to right (brand §2). */
const SLOTS = 16;

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
  // Until the first folder reports, the slots march (indeterminate).
  const max = progress?.total || 1;
  const value =
    progress && progress.total > 0 ? Math.min(progress.done, max) : null;
  return (
    <div
      role="status"
      className="grid w-full max-w-lg justify-items-center gap-2.5 text-center"
    >
      <p className="m-0 text-dense text-cr-text">
        {progress && progress.done > 0
          ? t('start.scanning', {
              folder: progress.folder,
              done: n.format(progress.done),
              total: n.format(progress.total),
            })
          : t('start.scanningStart')}
      </p>
      <Progress
        aria-label={t('start.progressLabel')}
        slots={SLOTS}
        max={max}
        value={value}
      />
      {progress && (
        <p className="hint tabular-nums">
          {t('start.romsFound', { n: n.format(progress.roms) })}
        </p>
      )}
      {path && (
        <p className="hint mono truncate max-w-full" title={path}>
          {path}
        </p>
      )}
    </div>
  );
}
