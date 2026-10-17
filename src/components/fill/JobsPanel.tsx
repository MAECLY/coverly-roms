import { Download, Undo2 } from 'lucide-react';
import {
  Fragment,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react';

import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Progress } from '@/components/ui/progress';
import { LocaleContext } from '@/i18n/core';
import { useT } from '@/i18n/useT';
import { getBackend } from '@/ipc/backend';
import { errorKey } from '@/ipc/errors';
import type {
  ExportFormat,
  JobItemView,
  JobTotals,
  LibraryView,
} from '@/ipc/types';
import { isRunning, useJobStore } from '@/stores/jobStore';

const TOTALS: readonly (keyof JobTotals)[] = [
  'filled',
  'replaced',
  'skipped',
  'no_art',
  'failed',
];

export function JobsPanel({
  view,
  onClose,
}: {
  view: LibraryView;
  onClose(): void;
}) {
  const t = useT();
  const locale = useContext(LocaleContext);
  const n = useMemo(() => new Intl.NumberFormat(locale), [locale]);
  const dates = useMemo(
    () =>
      new Intl.DateTimeFormat(locale, {
        dateStyle: 'medium',
        timeStyle: 'short',
      }),
    [locale],
  );
  const job = useJobStore((s) => s.job);
  const progress = useJobStore((s) => s.progress);
  const history = useJobStore((s) => s.history);
  const lastUndo = useJobStore((s) => s.lastUndo);
  const error = useJobStore((s) => s.error);
  const pending = useJobStore((s) => s.pending);
  const resume = useJobStore((s) => s.resume);
  const cancel = useJobStore((s) => s.cancel);
  const undoLast = useJobStore((s) => s.undoLast);
  const loadHistory = useJobStore((s) => s.loadHistory);
  const exportReport = useJobStore((s) => s.exportReport);
  const [confirming, setConfirming] = useState(false);
  const keepRef = useRef<HTMLButtonElement>(null);
  const undoRef = useRef<HTMLButtonElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  const undoTitleRef = useRef<HTMLHeadingElement>(null);
  const wasConfirming = useRef(false);
  const [failed, setFailed] = useState<{
    job: number;
    items: JobItemView[];
  } | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const running = isRunning(job);
  const jobId = job?.id ?? null;
  const state = job?.state ?? null;
  const finished =
    state === 'done' ||
    state === 'paused' ||
    state === 'cancelled' ||
    state === 'failed';

  useEffect(() => {
    void loadHistory();
  }, [loadHistory, state]);

  useEffect(() => {
    if (jobId === null || !finished) return;
    let alive = true;
    void getBackend()
      .then((b) => b.fillJobItems(jobId, 'failed'))
      .then(
        (items) => alive && setFailed({ job: jobId, items }),
        () => alive && setFailed({ job: jobId, items: [] }),
      );
    return () => {
      alive = false;
    };
  }, [jobId, finished]);

  // The focused button unmounts when the confirmation toggles: hand focus over
  // so keyboard and screen reader users stay inside the dialog.
  useEffect(() => {
    if (confirming) keepRef.current?.focus();
    else if (wasConfirming.current) {
      const back = undoRef.current;
      if (back && !back.disabled) back.focus();
      else undoTitleRef.current?.focus();
    }
    wasConfirming.current = confirming;
  }, [confirming]);

  const errors = finished && failed?.job === jobId ? failed.items : [];
  const p = progress && job && progress.job === job.id ? progress : null;
  // Rust refuses while paused and only undoes journals of the open library.
  const canUndo =
    !running &&
    job?.state !== 'paused' &&
    history.some((h) => !h.undone && h.root === view.root);

  // Journal headers come from disk and may be empty or garbage.
  function when(createdAt: string) {
    const d = new Date(createdAt);
    return Number.isNaN(d.getTime()) ? t('jobs.unknownDate') : dates.format(d);
  }

  async function exportJob(format: ExportFormat) {
    const name = await exportReport(format);
    if (name) setSaved(name);
  }

  async function undo() {
    setConfirming(false);
    await undoLast(view.generation);
  }

  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent
        size="lg"
        hideClose
        aria-describedby={undefined}
        ref={contentRef}
        // Focus the dialog itself, as before: the first button may be Cancel.
        onOpenAutoFocus={(ev) => {
          ev.preventDefault();
          contentRef.current?.focus();
        }}
      >
        <DialogHeader>
          <DialogTitle>{t('jobs.title')}</DialogTitle>
        </DialogHeader>
        <DialogBody className="grid content-start gap-5">
          {job ? (
            <section aria-labelledby="jobs-current" className="grid gap-2.5">
              <h3 id="jobs-current" className={sectionTitle}>
                {t('jobs.current')}
              </h3>
              <p className="m-0">{t(`jobs.state.${job.state}`)}</p>
              {running && (
                <div className="grid gap-2">
                  <Progress
                    max={p?.total || 1}
                    value={Math.min(p?.done ?? 0, p?.total || 1)}
                    aria-label={t('jobs.progressLabel')}
                  />
                  {p && (
                    <p className="m-0 tabular-nums">
                      {t('jobs.progress', {
                        phase: t(`jobs.phase.${p.phase}`),
                        done: n.format(Math.min(p.done + 1, p.total)),
                        total: n.format(p.total),
                      })}
                    </p>
                  )}
                  {p?.label ? (
                    <p className="hint mono truncate" title={p.label}>
                      {p.label}
                    </p>
                  ) : null}
                  <div>
                    <Button
                      variant="outline"
                      size="sm"
                      disabled={pending}
                      onClick={() => void cancel()}
                    >
                      {t('jobs.cancel')}
                    </Button>
                  </div>
                </div>
              )}
              {job.state === 'paused' && job.paused && (
                <Alert
                  variant="warning"
                  role="alert"
                  action={
                    <>
                      <Button
                        variant="ghost"
                        size="sm"
                        disabled={pending}
                        onClick={() => void cancel()}
                      >
                        {t('jobs.cancel')}
                      </Button>
                      <Button
                        variant="primary"
                        size="sm"
                        disabled={pending}
                        onClick={() => void resume()}
                      >
                        {t('jobs.resume')}
                      </Button>
                    </>
                  }
                >
                  <AlertTitle>{t(`jobs.pause.${job.paused.kind}`)}</AlertTitle>
                  <AlertDescription
                    className="truncate"
                    title={job.paused.detail}
                  >
                    {job.paused.detail}
                  </AlertDescription>
                </Alert>
              )}
              {job.error && (
                <Alert variant="danger">
                  {t('jobs.failed', { detail: job.error })}
                </Alert>
              )}
              {job.result && (
                <dl className="jobs-totals">
                  {TOTALS.map((k) => (
                    <Fragment key={k}>
                      <dt>{t(`jobs.totals.${k}`)}</dt>
                      <dd>{n.format(job.result?.[k] ?? 0)}</dd>
                    </Fragment>
                  ))}
                </dl>
              )}
              {errors.length > 0 && (
                <ul
                  className="job-errors"
                  aria-label={t('jobs.errors', { n: n.format(errors.length) })}
                >
                  {errors.map((i) => (
                    <li key={i.index}>
                      <span className="truncate" title={i.name}>
                        {i.name}
                      </span>
                      <span className="hint" title={i.outcome?.message ?? ''}>
                        {t(`art.media.${i.media}`)} ·{' '}
                        {i.outcome?.error
                          ? t(`jobs.error.${i.outcome.error}`)
                          : t('jobs.unknownError')}
                      </span>
                    </li>
                  ))}
                </ul>
              )}
              {job.result && (
                <div className="flex flex-wrap gap-2">
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => void exportJob('csv')}
                  >
                    <Download aria-hidden />
                    {t('jobs.exportCsv')}
                  </Button>
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => void exportJob('json')}
                  >
                    <Download aria-hidden />
                    {t('jobs.exportJson')}
                  </Button>
                </div>
              )}
              {saved && (
                <p className="hint">{t('reports.saved', { name: saved })}</p>
              )}
            </section>
          ) : (
            <p className="m-0">{t('jobs.none')}</p>
          )}
          <section aria-labelledby="jobs-undo" className="grid gap-2.5">
            <h3
              id="jobs-undo"
              className={sectionTitle}
              ref={undoTitleRef}
              tabIndex={-1}
            >
              {t('jobs.undoTitle')}
            </h3>
            {confirming ? (
              <Alert
                variant="warning"
                role="group"
                aria-labelledby="jobs-undo-confirm"
                action={
                  <>
                    <Button
                      ref={keepRef}
                      variant="ghost"
                      size="sm"
                      onClick={() => setConfirming(false)}
                    >
                      {t('jobs.undoNo')}
                    </Button>
                    <Button
                      variant="destructive"
                      size="sm"
                      disabled={pending}
                      onClick={() => void undo()}
                    >
                      {t('jobs.undoYes')}
                    </Button>
                  </>
                }
              >
                <p id="jobs-undo-confirm" className="m-0">
                  {t('jobs.undoConfirm')}
                </p>
              </Alert>
            ) : (
              <div>
                <Button
                  ref={undoRef}
                  variant="outline"
                  disabled={!canUndo || pending}
                  onClick={() => setConfirming(true)}
                >
                  <Undo2 aria-hidden />
                  {t('jobs.undo')}
                </Button>
              </div>
            )}
            {lastUndo && (
              <p role="status" className="m-0">
                {t('jobs.undone', {
                  removed: n.format(lastUndo.removed),
                  restored: n.format(lastUndo.restored),
                  kept: n.format(lastUndo.kept),
                })}
              </p>
            )}
          </section>
          <section aria-labelledby="jobs-history" className="grid gap-2.5">
            <h3 id="jobs-history" className={sectionTitle}>
              {t('jobs.history')}
            </h3>
            {history.length === 0 ? (
              <p className="hint">{t('jobs.noHistory')}</p>
            ) : (
              <ul className="history-list" aria-labelledby="jobs-history">
                {history.map((h) => (
                  <li key={h.job} className="items-center">
                    <span>{when(h.created_at)}</span>
                    <span className="tabular-nums">
                      {h.entries === 1
                        ? t('jobs.file')
                        : t('jobs.files', { n: n.format(h.entries) })}
                    </span>
                    {h.undone ? (
                      <Badge variant="neutral">{t('jobs.undoneBadge')}</Badge>
                    ) : h.complete ? (
                      <Badge variant="success">{t('jobs.complete')}</Badge>
                    ) : (
                      <Badge variant="warning">{t('jobs.incomplete')}</Badge>
                    )}
                  </li>
                ))}
              </ul>
            )}
          </section>
          {error && (
            <Alert variant="danger">
              {t(errorKey(error.code), { detail: error.detail ?? '' })}
            </Alert>
          )}
        </DialogBody>
        <DialogFooter>
          <Button variant="outline" onClick={onClose}>
            {t('jobs.close')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

const sectionTitle = 'm-0 text-ui font-semibold text-cr-text outline-none';
