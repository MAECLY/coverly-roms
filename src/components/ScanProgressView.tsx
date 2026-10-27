import { useContext, useMemo } from 'react';

import { ScanStage } from '@/components/progress/ScanStage';
import { WorkReadout } from '@/components/progress/WorkReadout';
import { LocaleContext } from '@/i18n/core';
import { useRichT } from '@/i18n/rich';
import { useT } from '@/i18n/useT';
import type { ScanProgress } from '@/ipc/types';

/** One slot per step of the scan, filling left to right (brand §2). */
const SLOTS = 16;

/**
 * The scan's progress (spec §6.5): a stage, then the shared readout with
 * «Carpeta n de N», the ROMs found and the folder being read. No time left
 * (D12): folders differ by orders of magnitude. Only the sentence is a live
 * region (AP-3); the counts change too often to announce.
 */
export function ScanProgressView({
  path,
  progress,
}: {
  path: string | null;
  progress: ScanProgress | null;
}) {
  const t = useT();
  const tr = useRichT();
  const locale = useContext(LocaleContext);
  const n = useMemo(() => new Intl.NumberFormat(locale), [locale]);
  // Until the first folder reports, the slots march (indeterminate).
  const max = progress?.total || 1;
  const value =
    progress && progress.total > 0 ? Math.min(progress.done, max) : null;
  const started = !!progress && progress.done > 0;
  const folders = started
    ? {
        done: n.format(progress.done),
        total: n.format(progress.total),
      }
    : null;
  const counters = progress
    ? [
        tr('start.romsFound', { n: n.format(progress.roms) }),
        ...(started && progress.folder
          ? [
              <span
                key="folder"
                className="mono inline-block max-w-[24ch] truncate align-bottom"
                title={progress.folder}
              >
                <bdi>{progress.folder}</bdi>
              </span>,
            ]
          : []),
      ]
    : [];
  return (
    <div className="grid w-full max-w-lg justify-items-center gap-2.5 text-center">
      <p role="status" className="m-0 text-dense text-cr-text">
        {t('start.scanningStart')}
      </p>
      <ScanStage
        done={progress?.done ?? 0}
        total={progress?.total ?? 0}
        roms={progress?.roms ?? 0}
        label={t('start.progressLabel')}
      />
      <WorkReadout
        bar={{ value, max, slots: SLOTS }}
        label={t('start.progressLabel')}
        {...(folders
          ? {
              amount: tr('start.progress.folders', folders),
              amountText: t('start.progress.folders', folders),
            }
          : {})}
        counters={counters}
      />
      {path && (
        <p className="hint mono truncate max-w-full" title={path}>
          {path}
        </p>
      )}
    </div>
  );
}
