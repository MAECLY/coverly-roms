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
import { errorKey, toBackendError, type BackendError } from '@/ipc/errors';
import type {
  ExportFormat,
  JobHistoryItem,
  JobItemView,
  JobTotals,
  LibraryView,
} from '@/ipc/types';
import { undoCounts } from '@/components/organize/undoCounts';
import { isRunning, useJobStore } from '@/stores/jobStore';
import { useLibraryStore } from '@/stores/libraryStore';
import { useNavStore } from '@/stores/navStore';
import { useOrganizeStore } from '@/stores/organizeStore';

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
  /** The organise job whose undo awaits confirmation, in its history row. */
  const [organizeConfirm, setOrganizeConfirm] = useState<string | null>(null);
  const [organizeBusy, setOrganizeBusy] = useState(false);
  const [organizeUndo, setOrganizeUndo] = useState<{
    job: string;
    done: { restored: number; modified: number } | null;
    error: BackendError | null;
  } | null>(null);
  /** An organise undo moved ROMs: the open library rescans on close. */
  const moved = useRef(false);
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
  // Rust refuses while paused and only undoes fill journals of the open
  // library (an organise job is undone from its history row below, which
  // checks later jobs first).
  const canUndo =
    !running &&
    job?.state !== 'paused' &&
    history.some(
      (h) => !h.undone && h.root === view.root && h.kind !== 'organize',
    );

  // Journal headers come from disk and may be empty or garbage.
  function when(createdAt: string) {
    const d = new Date(createdAt);
    return Number.isNaN(d.getTime()) ? t('jobs.unknownDate') : dates.format(d);
  }

  async function exportJob(format: ExportFormat) {
    const name = await exportReport(format);
    if (name) setSaved(name);
  }

  /** An interrupted organise job resumes on the organise screen. */
  function resumeOrganize(job: string, source: string) {
    void useOrganizeStore.getState().resumeJob(job, source);
    useNavStore.getState().openOrganize(source);
  }

  async function undo() {
    setConfirming(false);
    await undoLast(view.generation);
  }

  /** Organise spec §4.3: an organise job is also undone later from the
   * history; newer jobs that wrote under it are listed first. */
  async function undoOrganize(job: string, includeLater: boolean) {
    setOrganizeConfirm(null);
    setOrganizeBusy(true);
    try {
      const b = await getBackend();
      const result = await b.organizeUndo(job, includeLater);
      moved.current = true;
      setOrganizeUndo({ job, done: undoCounts(result), error: null });
      await loadHistory();
    } catch (e) {
      setOrganizeUndo({ job, done: null, error: toBackendError(e) });
    } finally {
      setOrganizeBusy(false);
    }
  }

  function laterJobs(detail: string | undefined) {
    return (detail ?? '')
      .split(',')
      .map((s) => s.trim())
      .filter(Boolean)
      .map((id) => {
        const h = history.find((x: JobHistoryItem) => x.job === id);
        return h
          ? t('organize.undo.laterJob', {
              kind: t(`jobs.kind.${h.kind}`),
              date: when(h.created_at),
            })
          : id;
      })
      .join(', ');
  }

  function close() {
    if (moved.current) {
      moved.current = false;
      void useLibraryStore.getState().open(view.root, view.profile.id);
    }
    onClose();
  }

  return (
    <Dialog open onOpenChange={(open) => !open && close()}>
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
                    <span>{t(`jobs.kind.${h.kind}`)}</span>
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
                    ) : h.kind === 'organize' ? (
                      <Badge variant="warning">
                        {t('organize.interrupted')}
                      </Badge>
                    ) : (
                      <Badge variant="warning">{t('jobs.incomplete')}</Badge>
                    )}
                    <span className="flex flex-wrap justify-end gap-1">
                      {h.kind === 'organize' &&
                        !h.complete &&
                        !h.undone &&
                        h.undoable && (
                          <Button
                            variant="outline"
                            size="sm"
                            disabled={running}
                            onClick={() => resumeOrganize(h.job, h.root)}
                          >
                            {t('organize.resume')}
                          </Button>
                        )}
                      {h.kind === 'organize' && !h.undone && h.undoable && (
                        <AlertDialog
                          open={organizeConfirm === h.job}
                          onOpenChange={(on) =>
                            setOrganizeConfirm(on ? h.job : null)
                          }
                        >
                          <AlertDialogTrigger asChild>
                            <Button
                              variant="outline"
                              size="sm"
                              disabled={running || organizeBusy}
                              aria-label={t('jobs.organizeUndo.label', {
                                date: when(h.created_at),
                              })}
                            >
                              <Undo2 aria-hidden />
                              {t('organize.undo.action')}
                            </Button>
                          </AlertDialogTrigger>
                          <AlertDialogContent>
                            <AlertDialogHeader>
                              <AlertDialogTitle>
                                {t('jobs.organizeUndo.title', {
                                  date: when(h.created_at),
                                })}
                              </AlertDialogTitle>
                              <AlertDialogDescription>
                                {t('jobs.organizeUndo.confirm')}
                              </AlertDialogDescription>
                            </AlertDialogHeader>
                            <AlertDialogFooter>
                              <AlertDialogCancel>
                                {t('jobs.undoNo')}
                              </AlertDialogCancel>
                              <AlertDialogAction
                                variant="destructive"
                                disabled={running || organizeBusy}
                                onClick={() => void undoOrganize(h.job, false)}
                              >
                                {h.entries > 0
                                  ? t('organize.undo.actionCount', {
                                      count: n.format(h.entries),
                                    })
                                  : t('jobs.undoYes')}
                              </AlertDialogAction>
                            </AlertDialogFooter>
                          </AlertDialogContent>
                        </AlertDialog>
                      )}
                    </span>
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
          {organizeUndo?.done && (
            <Alert variant="success">
              <p className="m-0 tabular-nums">
                {t('organize.undone', organizeUndo.done)}
              </p>
            </Alert>
          )}
          {organizeUndo?.error?.code === 'later-jobs' && (
            <Alert
              variant="warning"
              role="alert"
              action={
                <Button
                  variant="outline"
                  size="sm"
                  disabled={running || organizeBusy}
                  onClick={() => void undoOrganize(organizeUndo.job, true)}
                >
                  {t('organize.undo.all')}
                </Button>
              }
            >
              <p className="m-0">
                {t('organize.undo.laterJobs', {
                  jobs: laterJobs(organizeUndo.error.detail),
                })}
              </p>
            </Alert>
          )}
          {organizeUndo?.error && organizeUndo.error.code !== 'later-jobs' && (
            <Alert variant="danger">
              {t(errorKey(organizeUndo.error.code), {
                detail: organizeUndo.error.detail ?? '',
              })}
            </Alert>
          )}
        </DialogBody>
        <DialogFooter>
          <Button variant="outline" onClick={close}>
            {t('jobs.close')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

const sectionTitle = 'm-0 text-ui font-semibold text-cr-text outline-none';
