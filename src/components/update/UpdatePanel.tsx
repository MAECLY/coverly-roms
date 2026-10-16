import { useContext } from 'react';

import { LocaleContext } from '@/i18n/core';
import { useT } from '@/i18n/useT';
import { getBackend } from '@/ipc/backend';
import { errorKey } from '@/ipc/errors';
import { formatSize } from '@/lib/format';
import { blocksUpdate, useJobStore } from '@/stores/jobStore';
import { useUpdateStore } from '@/stores/updateStore';

const CAN_CHECK = ['idle', 'up-to-date', 'error'];

/** Status and actions of the updater (About; `compact` in General). */
export function UpdatePanel({ compact = false }: { compact?: boolean }) {
  const t = useT();
  const locale = useContext(LocaleContext);
  const u = useUpdateStore();
  const jobRunning = useJobStore((s) => blocksUpdate(s.job));
  const pct = u.progress?.total
    ? Math.min(
        100,
        Math.round((u.progress.downloaded / u.progress.total) * 100),
      )
    : null;
  let line: string;
  switch (u.status) {
    case 'idle':
      line = t('update.neverChecked');
      break;
    case 'checking':
      line = t('update.checking');
      break;
    case 'up-to-date':
      line = t('update.upToDate', { version: u.current ?? '' });
      break;
    case 'available':
      line = t('update.available', { version: u.offer?.version ?? '' });
      break;
    case 'downloading':
      line =
        pct === null
          ? t('update.downloadingSize', {
              size: formatSize(u.progress?.downloaded ?? 0, locale),
            })
          : t('update.downloading', { percent: pct });
      break;
    case 'waiting-for-job':
      line = t('update.waitingForJob');
      break;
    case 'installing':
      line = t('update.installing');
      break;
    case 'ready':
      line = t('update.ready', { version: u.offer?.version ?? '' });
      break;
    case 'error':
      line = t(errorKey(u.error?.code ?? 'unknown'), {
        detail: u.error?.detail ?? '',
      });
      break;
  }
  return (
    <div className="update-panel" aria-live="polite">
      <p role={u.status === 'error' ? 'alert' : undefined}>{line}</p>
      {u.status === 'downloading' && (
        <progress
          aria-label={t('update.progressLabel')}
          max={100}
          value={pct ?? undefined}
        />
      )}
      {!compact && u.status === 'available' && u.offer?.notes && (
        <details open>
          <summary>{t('update.notes')}</summary>
          <p className="update-notes">{u.offer.notes}</p>
        </details>
      )}
      <div className="update-actions">
        {CAN_CHECK.includes(u.status) && (
          <button
            type="button"
            className="ghost-button"
            onClick={() => void u.check('manual')}
          >
            {u.status === 'error' ? t('update.retry') : t('update.checkNow')}
          </button>
        )}
        {u.status === 'available' && (
          <button
            type="button"
            className="primary-button"
            onClick={() => void u.download()}
          >
            {t('update.install')}
          </button>
        )}
        {u.status === 'ready' && (
          <button
            type="button"
            className="primary-button"
            disabled={jobRunning}
            onClick={() => void u.restart()}
          >
            {t('update.restart')}
          </button>
        )}
        {u.error?.code === 'update-no-key' && (
          <button
            type="button"
            className="ghost-button"
            onClick={() =>
              void getBackend().then((b) => b.openLink('releases'))
            }
          >
            {t('about.link.releases')}
          </button>
        )}
      </div>
      {u.status === 'ready' && jobRunning && (
        <p className="hint">{t('update.restartAfterJob')}</p>
      )}
    </div>
  );
}
