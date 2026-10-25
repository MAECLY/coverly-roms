import { ChevronRight, FolderOpen, Play, Undo2 } from 'lucide-react';
import { useMemo } from 'react';

import { Alert, AlertTitle } from '@/components/ui/alert';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from '@/components/ui/alert-dialog';
import { Button } from '@/components/ui/button';
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible';
import { ScrollArea } from '@/components/ui/scroll-area';
import { useT, type TFunction } from '@/i18n/useT';
import { errorKey, type BackendError } from '@/ipc/errors';
import type {
  OrganizeItem,
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
import { useLaterJobs } from './laterJobs';
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

/** Whether an extracted item's archive went to the Trash: a move whose
 * plan sent it there (copy mode and leftovers keep it). */
function trashed(
  o: OrganizeItemOutcome,
  item: OrganizeItem | undefined,
  plan: OrganizePlan | null,
): boolean {
  return (
    o.status === 'extracted' &&
    plan?.mode === 'move' &&
    item?.transfer.kind === 'extract' &&
    item.transfer.archive_after === 'trash'
  );
}

function outcomeText(t: TFunction, o: OrganizeItemOutcome): string {
  switch (o.status) {
    case 'moved':
      return t('organize.outcome.moved');
    case 'copied':
      return t('organize.outcome.copied');
    case 'copied-not-removed':
      return t('organize.copiedNotRemoved');
    case 'extracted':
      return t('organize.outcome.extracted');
    case 'extracted-kept':
      return t('organize.outcome.extractedKept');
    case 'quarantined':
      return t('organize.outcome.quarantined');
    case 'trashed':
      return t('organize.outcome.trashed');
    case 'skipped':
      return t(`organize.outcome.skipped.${o.reason}`);
    case 'failed':
      return t('organize.outcome.failed', {
        reason: t(`jobs.error.${o.kind}`),
      });
  }
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
  const items = useMemo(
    () => new Map((plan?.items ?? []).map((i) => [i.id, i])),
    [plan],
  );
  const undone = undoCounts(undo);
  const notRemoved =
    report?.items.filter((o) => o.status === 'copied-not-removed').length ?? 0;
  const setAside =
    report?.items.filter((o) => o.status === 'quarantined').length ?? 0;
  const trashedCopies =
    report?.items.filter((o) => o.status === 'trashed').length ?? 0;
  // Set-aside copies come back with the undo too.
  const placed = report
    ? report.moved + report.copied + report.extracted + report.set_aside
    : 0;
  const kept = report?.items.filter((o) => o.status === 'extracted-kept') ?? [];
  const lost = (undo?.report ?? []).flatMap((r) => r.archive_lost);
  // A verb plus the exact count when it is known (brand §7).
  const undoLabel =
    placed > 0
      ? t('organize.undo.actionCount', { count: placed })
      : t('organize.undo.action');
  const nameOf = (o: OrganizeItemOutcome) => {
    const source = items.get(o.item)?.sources[0] || o.targets[0];
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
      {report &&
        (() => {
          const line = (
            <p className="m-0 text-dense tabular-nums text-cr-text">
              {[
                t('organize.result.counts', {
                  moved: report.moved,
                  copied: report.copied,
                  skipped: report.skipped,
                  failed: report.failed,
                }),
                // Extracted archives join the counts only when there are some.
                ...(report.extracted > 0
                  ? [
                      t('organize.result.extracted', {
                        count: report.extracted,
                      }),
                    ]
                  : []),
                // So do the duplicates set aside (or sent to the Trash).
                ...(setAside > 0
                  ? [t('organize.result.setAside', { count: setAside })]
                  : []),
                ...(trashedCopies > 0
                  ? [t('organize.result.trashed', { count: trashedCopies })]
                  : []),
              ].join(' · ')}
            </p>
          );
          // A finished run reads as a status with its icon (brand §4.1):
          // success when every file landed, warning when some failed.
          if (job.state !== 'done' || undone) return line;
          return (
            <Alert variant={report.failed > 0 ? 'warning' : 'success'}>
              {line}
            </Alert>
          );
        })()}
      {notRemoved > 0 && (
        <p className={hintText}>
          {t('organize.copiedNotRemovedCount', { count: notRemoved })}
        </p>
      )}
      {kept.length > 0 && (
        // The archive a move meant to send to the Trash is still there:
        // said in sight, by name, not only in the details.
        <ul className="m-0 grid list-none gap-0.5 p-0">
          {kept.map((o) => (
            <li key={o.item} className={hintText}>
              {`${nameOf(o)} · ${t('organize.extract.kept')}`}
            </li>
          ))}
        </ul>
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
                  >
                    {`${nameOf(o)} · ${outcomeText(t, o)}`}
                    {trashed(o, items.get(o.item), plan) &&
                      ` · ${t('organize.extract.trashed')}`}
                  </li>
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
      {lost.length > 0 && (
        <Alert variant="warning">
          <p className="m-0">{t('organize.undo.archiveLost')}</p>
          <ul className="m-0 grid list-none gap-0.5 p-0">
            {lost.map((path) => (
              <li key={path} className={pathText}>
                {baseName(path)}
              </li>
            ))}
          </ul>
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
            // Undo asks first, as it does from the job history.
            <AlertDialog>
              <AlertDialogTrigger asChild>
                <Button variant="outline">
                  <Undo2 aria-hidden />
                  {undoLabel}
                </Button>
              </AlertDialogTrigger>
              <AlertDialogContent>
                <AlertDialogHeader>
                  <AlertDialogTitle>
                    {t('organize.undo.confirmTitle')}
                  </AlertDialogTitle>
                  <AlertDialogDescription>
                    {t('jobs.organizeUndo.confirm')}
                  </AlertDialogDescription>
                </AlertDialogHeader>
                <AlertDialogFooter>
                  <AlertDialogCancel>{t('jobs.undoNo')}</AlertDialogCancel>
                  <AlertDialogAction
                    variant="destructive"
                    onClick={() => onUndo(false)}
                  >
                    {undoLabel}
                  </AlertDialogAction>
                </AlertDialogFooter>
              </AlertDialogContent>
            </AlertDialog>
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
