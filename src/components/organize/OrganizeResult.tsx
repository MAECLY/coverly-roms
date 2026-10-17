import { ChevronRight, FolderOpen, Play, Undo2 } from 'lucide-react';
import { useContext, useEffect, useMemo, useState } from 'react';

import { Alert, AlertTitle } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible';
import { ScrollArea } from '@/components/ui/scroll-area';
import { LocaleContext } from '@/i18n/core';
import { useT, type TFunction } from '@/i18n/useT';
import { getBackend } from '@/ipc/backend';
import { errorKey, type BackendError } from '@/ipc/errors';
import type {
  JobHistoryItem,
  OrganizeItemOutcome,
  OrganizeJobView,
  OrganizePlan,
  OrganizeUndoView,
} from '@/ipc/types';

import {
  chevron,
  fileList,
  hintText,
  listViewport,
  panel,
  pathText,
  sectionTitle,
} from './styles';
import { undoCounts } from './undoCounts';

interface Props {
  job: OrganizeJobView;
  /** The plan that was applied (absent for a job resumed from the history). */
  plan: OrganizePlan | null;
  undo: OrganizeUndoView | null;
  error: BackendError | null;
  onOpen(): void;
  onUndo(includeLater: boolean): void;
  onResume(): void;
}

const baseName = (path: string) => path.split(/[\\/]/).pop() || path;

function outcomeText(t: TFunction, o: OrganizeItemOutcome): string {
  switch (o.status) {
    case 'moved':
      return t('organize.outcome.moved');
    case 'copied':
      return t('organize.outcome.copied');
    case 'copied-not-removed':
      return t('organize.copiedNotRemoved');
    case 'skipped':
      return t(`organize.outcome.skipped.${o.reason}`);
    case 'failed':
      return t('organize.outcome.failed', {
        reason: t(`jobs.error.${o.kind}`),
      });
  }
}

/** The ids of `later-jobs` as "Covers 17 Oct 2026, 10:32", once the
 * history is there (the ids themselves until then). */
function useLaterJobs(ids: string[] | null): string {
  const t = useT();
  const locale = useContext(LocaleContext);
  const [history, setHistory] = useState<JobHistoryItem[]>([]);
  const key = ids?.join(', ') ?? null;
  useEffect(() => {
    if (key === null) return;
    let alive = true;
    void getBackend()
      .then((b) => b.jobHistory())
      .then(
        (h) => alive && setHistory(Array.isArray(h) ? h : []),
        () => undefined,
      );
    return () => {
      alive = false;
    };
  }, [key]);
  return useMemo(() => {
    if (!ids) return '';
    const dates = new Intl.DateTimeFormat(locale, {
      dateStyle: 'medium',
      timeStyle: 'short',
    });
    return ids
      .map((id) => {
        const h = history.find((x) => x.job === id);
        if (!h) return id;
        const d = new Date(h.created_at);
        return t('organize.undo.laterJob', {
          kind: t(`jobs.kind.${h.kind}`),
          date: Number.isNaN(d.getTime())
            ? t('jobs.unknownDate')
            : dates.format(d),
        });
      })
      .join(', ');
  }, [ids, history, locale, t]);
}

/** What an organise apply did, with open, undo and resume (spec §4.3). */
export function OrganizeResult({
  job,
  plan,
  undo,
  error,
  onOpen,
  onUndo,
  onResume,
}: Props) {
  const t = useT();
  const report = job.report;
  const paused = job.state === 'paused';
  const laterIds =
    error?.code === 'later-jobs'
      ? (error.detail ?? '')
          .split(',')
          .map((s) => s.trim())
          .filter(Boolean)
      : null;
  const later = useLaterJobs(laterIds);
  const names = useMemo(
    () => new Map((plan?.items ?? []).map((i) => [i.id, i.sources[0] ?? ''])),
    [plan],
  );
  const undone = undoCounts(undo);
  const notRemoved =
    report?.items.filter((o) => o.status === 'copied-not-removed').length ?? 0;
  const nameOf = (o: OrganizeItemOutcome) => {
    const source = names.get(o.item) || o.targets[0];
    return source ? baseName(source) : `#${o.item}`;
  };

  return (
    <section
      aria-labelledby="organize-result-title"
      className="flex flex-col gap-3"
    >
      <h2 id="organize-result-title" className={sectionTitle}>
        {t(paused ? 'organize.pausedTitle' : 'organize.resultTitle')}
      </h2>
      {report && (
        <p className="m-0 text-dense tabular-nums text-cr-text">
          {t('organize.result', {
            moved: report.moved,
            copied: report.copied,
            skipped: report.skipped,
            failed: report.failed,
          })}
        </p>
      )}
      {notRemoved > 0 && (
        <p className={hintText}>
          {t('organize.copiedNotRemovedCount', { count: notRemoved })}
        </p>
      )}
      {job.state === 'cancelled' && (
        <p className={hintText}>{t('organize.stopped')}</p>
      )}
      {paused && report?.paused && (
        <Alert variant="warning" role="alert">
          <AlertTitle>{t(`jobs.pause.${report.paused}`)}</AlertTitle>
          {report.pause_reason && (
            <p className="m-0 text-cr-text-secondary">
              {t(`jobs.error.${report.pause_reason}`)}
            </p>
          )}
        </Alert>
      )}
      {report && report.items.length > 0 && (
        <Collapsible className={panel}>
          <CollapsibleTrigger asChild>
            <Button
              variant="ghost"
              className="group w-full justify-start rounded-lg px-3"
            >
              <ChevronRight aria-hidden className={chevron} />
              {t('organize.details')}
            </Button>
          </CollapsibleTrigger>
          <CollapsibleContent className="border-t border-cr-border">
            <ScrollArea
              aria-label={t('organize.details')}
              viewportClassName={listViewport}
            >
              <ul className={fileList}>
                {report.items.map((o) => (
                  <li
                    key={o.item}
                    className={`${pathText} px-3 py-1.5 text-cr-text`}
                  >{`${nameOf(o)} · ${outcomeText(t, o)}`}</li>
                ))}
              </ul>
            </ScrollArea>
          </CollapsibleContent>
        </Collapsible>
      )}
      {laterIds && (
        <Alert variant="warning" role="alert">
          <p className="m-0">{t('organize.undo.laterJobs', { jobs: later })}</p>
        </Alert>
      )}
      {error && !laterIds && (
        <Alert variant="danger">
          <p className="m-0">
            {t(errorKey(error.code), { detail: error.detail ?? '' })}
          </p>
        </Alert>
      )}
      {undone && (
        <Alert variant="success">
          <p className="m-0 tabular-nums">{t('organize.undone', undone)}</p>
        </Alert>
      )}
      {!undo && (
        <div className="flex flex-wrap items-center gap-2">
          {paused ? (
            <Button variant="primary" onClick={onResume}>
              <Play aria-hidden />
              {t('organize.resume')}
            </Button>
          ) : (
            job.state !== 'failed' && (
              <Button variant="primary" onClick={onOpen}>
                <FolderOpen aria-hidden />
                {t('organize.openLibrary')}
              </Button>
            )
          )}
          {job.job && (
            <Button variant="outline" onClick={() => onUndo(false)}>
              <Undo2 aria-hidden />
              {t('organize.undo.action')}
            </Button>
          )}
          {laterIds && (
            <Button variant="outline" onClick={() => onUndo(true)}>
              {t('organize.undo.all')}
            </Button>
          )}
        </div>
      )}
    </section>
  );
}
