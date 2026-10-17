import { ChevronRight } from 'lucide-react';
import { useContext } from 'react';

import { Button } from '@/components/ui/button';
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible';
import { Progress } from '@/components/ui/progress';
import { ScrollArea } from '@/components/ui/scroll-area';
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
    case 'restarting':
      line = t('update.restarting');
      break;
    case 'error':
      line = t(errorKey(u.error?.code ?? 'unknown'), {
        detail: u.error?.detail ?? '',
      });
      break;
  }
  // Without a key no check can succeed: only the Releases link helps.
  const canCheck =
    CAN_CHECK.includes(u.status) && u.error?.code !== 'update-no-key';
  return (
    <div className="update-panel flex flex-col gap-2.5" aria-live="polite">
      <p
        className="m-0 text-ui text-cr-text"
        role={u.status === 'error' ? 'alert' : undefined}
      >
        {line}
      </p>
      {u.status === 'ready' && u.error && (
        <p className="m-0 text-ui text-cr-danger" role="alert">
          {t(errorKey(u.error.code), { detail: u.error.detail ?? '' })}
        </p>
      )}
      {u.status === 'downloading' && (
        <Progress
          aria-label={t('update.progressLabel')}
          className="max-w-sm"
          max={100}
          value={pct}
        />
      )}
      {!compact && u.status === 'available' && u.offer?.notes && (
        <Collapsible defaultOpen className="grid gap-1.5">
          <CollapsibleTrigger asChild>
            <Button
              variant="ghost"
              size="sm"
              className="group -ml-2 justify-self-start text-cr-text-secondary"
            >
              <ChevronRight
                aria-hidden
                className="transition-transform duration-fast ease-standard group-data-[state=open]:rotate-90 motion-reduce:transition-none"
              />
              {t('update.notes')}
            </Button>
          </CollapsibleTrigger>
          <CollapsibleContent>
            <ScrollArea
              className="rounded-md border border-cr-border bg-cr-surface-sunken"
              viewportClassName="max-h-48"
            >
              <p className="m-0 whitespace-pre-wrap px-3 py-2 text-ui text-cr-text-secondary [overflow-wrap:anywhere]">
                {u.offer.notes}
              </p>
            </ScrollArea>
          </CollapsibleContent>
        </Collapsible>
      )}
      <div className="flex flex-wrap items-center gap-2">
        {canCheck && (
          <Button variant="outline" onClick={() => void u.check('manual')}>
            {u.status === 'error' ? t('update.retry') : t('update.checkNow')}
          </Button>
        )}
        {u.status === 'available' && (
          <Button variant="primary" onClick={() => void u.download()}>
            {t('update.install')}
          </Button>
        )}
        {u.status === 'ready' && (
          <Button
            variant="primary"
            disabled={jobRunning}
            onClick={() => void u.restart()}
          >
            {t('update.restart')}
          </Button>
        )}
        {u.error?.code === 'update-no-key' && (
          <Button
            variant="outline"
            onClick={() =>
              void getBackend().then((b) => b.openLink('releases'))
            }
          >
            {t('about.link.releases')}
          </Button>
        )}
      </div>
      {u.status === 'ready' && jobRunning && (
        <p className="hint">{t('update.restartAfterJob')}</p>
      )}
    </div>
  );
}
