import { useContext, useEffect, useMemo, useState } from 'react';

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
  const ours = undo
    ? (undo.report.find((r) => r.job === job.job) ?? undo.report.at(-1))
    : undefined;
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
      <h2 id="organize-result-title" className="section-title">
        {t(paused ? 'organize.pausedTitle' : 'organize.resultTitle')}
      </h2>
      {report && (
        <p>
          {t('organize.result', {
            moved: report.moved,
            copied: report.copied,
            skipped: report.skipped,
            failed: report.failed,
          })}
        </p>
      )}
      {notRemoved > 0 && (
        <p className="hint">
          {t('organize.copiedNotRemovedCount', { count: notRemoved })}
        </p>
      )}
      {job.state === 'cancelled' && (
        <p className="hint">{t('organize.stopped')}</p>
      )}
      {paused && report?.paused && (
        <div role="alert" className="pause-banner">
          <p>{t(`jobs.pause.${report.paused}`)}</p>
          {report.pause_reason && (
            <p className="hint">{t(`jobs.error.${report.pause_reason}`)}</p>
          )}
        </div>
      )}
      {report && report.items.length > 0 && (
        <details>
          <summary>{t('organize.details')}</summary>
          <ul>
            {report.items.map((o) => (
              <li key={o.item}>{`${nameOf(o)} · ${outcomeText(t, o)}`}</li>
            ))}
          </ul>
        </details>
      )}
      {laterIds && (
        <p role="alert" className="error-banner">
          {t('organize.undo.laterJobs', { jobs: later })}
        </p>
      )}
      {error && !laterIds && (
        <p role="alert" className="error-banner">
          {t(errorKey(error.code), { detail: error.detail ?? '' })}
        </p>
      )}
      {ours && (
        <p role="status">
          {t('organize.undone', {
            restored: ours.restored.length + ours.removed.length,
            modified:
              ours.skipped_modified.length + ours.skipped_occupied.length,
          })}
        </p>
      )}
      {!undo && (
        <div className="flex flex-wrap items-center gap-3">
          {paused ? (
            <button type="button" className="primary-button" onClick={onResume}>
              {t('organize.resume')}
            </button>
          ) : (
            job.state !== 'failed' && (
              <button type="button" className="primary-button" onClick={onOpen}>
                {t('organize.openLibrary')}
              </button>
            )
          )}
          {job.job && (
            <button
              type="button"
              className="ghost-button"
              onClick={() => onUndo(false)}
            >
              {t('organize.undo.action')}
            </button>
          )}
          {laterIds && (
            <button
              type="button"
              className="ghost-button"
              onClick={() => onUndo(true)}
            >
              {t('organize.undo.all')}
            </button>
          )}
        </div>
      )}
    </section>
  );
}
