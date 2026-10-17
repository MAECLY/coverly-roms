import {
  Fragment,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react';

import { Modal } from '@/components/Modal';
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
    <Modal labelledBy="jobs-title" onClose={close} className="jobs">
      <h2 id="jobs-title" className="section-title">
        {t('jobs.title')}
      </h2>
      {job ? (
        <section aria-labelledby="jobs-current">
          <h3 id="jobs-current" className="section-title">
            {t('jobs.current')}
          </h3>
          <p>{t(`jobs.state.${job.state}`)}</p>
          {running && (
            <div className="progress-row">
              <progress
                max={p?.total || 1}
                value={p?.done ?? 0}
                aria-label={t('jobs.progressLabel')}
              />
              {p && (
                <p>
                  {t('jobs.progress', {
                    phase: t(`jobs.phase.${p.phase}`),
                    done: n.format(Math.min(p.done + 1, p.total)),
                    total: n.format(p.total),
                  })}
                </p>
              )}
              {p?.label ? (
                <p className="hint truncate" title={p.label}>
                  {p.label}
                </p>
              ) : null}
              <button
                type="button"
                className="ghost-button"
                disabled={pending}
                onClick={() => void cancel()}
              >
                {t('jobs.cancel')}
              </button>
            </div>
          )}
          {job.state === 'paused' && job.paused && (
            <div role="alert" className="pause-banner">
              <p>{t(`jobs.pause.${job.paused.kind}`)}</p>
              <p className="hint truncate" title={job.paused.detail}>
                {job.paused.detail}
              </p>
              <div className="modal-actions">
                <button
                  type="button"
                  className="ghost-button"
                  disabled={pending}
                  onClick={() => void cancel()}
                >
                  {t('jobs.cancel')}
                </button>
                <button
                  type="button"
                  className="primary-button"
                  disabled={pending}
                  onClick={() => void resume()}
                >
                  {t('jobs.resume')}
                </button>
              </div>
            </div>
          )}
          {job.error && (
            <p role="alert" className="error-banner">
              {t('jobs.failed', { detail: job.error })}
            </p>
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
            <div className="modal-actions">
              <button
                type="button"
                className="ghost-button"
                onClick={() => void exportJob('csv')}
              >
                {t('jobs.exportCsv')}
              </button>
              <button
                type="button"
                className="ghost-button"
                onClick={() => void exportJob('json')}
              >
                {t('jobs.exportJson')}
              </button>
            </div>
          )}
          {saved && (
            <p className="hint">{t('reports.saved', { name: saved })}</p>
          )}
        </section>
      ) : (
        <p>{t('jobs.none')}</p>
      )}
      <section aria-labelledby="jobs-undo">
        <h3
          id="jobs-undo"
          className="section-title"
          ref={undoTitleRef}
          tabIndex={-1}
        >
          {t('jobs.undoTitle')}
        </h3>
        {confirming ? (
          <div className="undo-confirm">
            <p>{t('jobs.undoConfirm')}</p>
            <div className="modal-actions">
              <button
                ref={keepRef}
                type="button"
                className="ghost-button"
                onClick={() => setConfirming(false)}
              >
                {t('jobs.undoNo')}
              </button>
              <button
                type="button"
                className="primary-button"
                disabled={pending}
                onClick={() => void undo()}
              >
                {t('jobs.undoYes')}
              </button>
            </div>
          </div>
        ) : (
          <button
            ref={undoRef}
            type="button"
            className="ghost-button"
            disabled={!canUndo || pending}
            onClick={() => setConfirming(true)}
          >
            {t('jobs.undo')}
          </button>
        )}
        {lastUndo && (
          <p role="status">
            {t('jobs.undone', {
              removed: n.format(lastUndo.removed),
              restored: n.format(lastUndo.restored),
              kept: n.format(lastUndo.kept),
            })}
          </p>
        )}
      </section>
      <section aria-labelledby="jobs-history">
        <h3 id="jobs-history" className="section-title">
          {t('jobs.history')}
        </h3>
        {history.length === 0 ? (
          <p className="hint">{t('jobs.noHistory')}</p>
        ) : (
          <ul className="history-list" aria-labelledby="jobs-history">
            {history.map((h) => (
              <li key={h.job}>
                <span>{t(`jobs.kind.${h.kind}`)}</span>
                <span>{when(h.created_at)}</span>
                <span>
                  {h.entries === 1
                    ? t('jobs.file')
                    : t('jobs.files', { n: n.format(h.entries) })}
                </span>
                <span className="hint">
                  {h.undone
                    ? t('jobs.undoneBadge')
                    : h.complete
                      ? t('jobs.complete')
                      : h.kind === 'organize'
                        ? t('organize.interrupted')
                        : t('jobs.incomplete')}
                </span>
                {h.kind === 'organize' &&
                  !h.complete &&
                  !h.undone &&
                  h.undoable && (
                    <button
                      type="button"
                      className="ghost-button"
                      disabled={running}
                      onClick={() => resumeOrganize(h.job, h.root)}
                    >
                      {t('organize.resume')}
                    </button>
                  )}
                {h.kind === 'organize' &&
                  !h.undone &&
                  h.undoable &&
                  organizeConfirm !== h.job && (
                    <button
                      type="button"
                      className="ghost-button"
                      disabled={running || organizeBusy}
                      aria-label={t('jobs.organizeUndo.label', {
                        date: when(h.created_at),
                      })}
                      onClick={() => setOrganizeConfirm(h.job)}
                    >
                      {t('organize.undo.action')}
                    </button>
                  )}
                {organizeConfirm === h.job && (
                  <div className="undo-confirm">
                    <p>{t('jobs.organizeUndo.confirm')}</p>
                    <div className="modal-actions">
                      <button
                        type="button"
                        className="ghost-button"
                        onClick={() => setOrganizeConfirm(null)}
                      >
                        {t('jobs.undoNo')}
                      </button>
                      <button
                        type="button"
                        className="primary-button"
                        disabled={running || organizeBusy}
                        onClick={() => void undoOrganize(h.job, false)}
                      >
                        {t('jobs.undoYes')}
                      </button>
                    </div>
                  </div>
                )}
              </li>
            ))}
          </ul>
        )}
      </section>
      {organizeUndo?.done && (
        <p role="status">{t('organize.undone', organizeUndo.done)}</p>
      )}
      {organizeUndo?.error?.code === 'later-jobs' && (
        <div role="alert" className="error-banner">
          <p>
            {t('organize.undo.laterJobs', {
              jobs: laterJobs(organizeUndo.error.detail),
            })}
          </p>
          <button
            type="button"
            className="ghost-button"
            disabled={running || organizeBusy}
            onClick={() => void undoOrganize(organizeUndo.job, true)}
          >
            {t('organize.undo.all')}
          </button>
        </div>
      )}
      {organizeUndo?.error && organizeUndo.error.code !== 'later-jobs' && (
        <p role="alert" className="error-banner">
          {t(errorKey(organizeUndo.error.code), {
            detail: organizeUndo.error.detail ?? '',
          })}
        </p>
      )}
      {error && (
        <p role="alert" className="error-banner">
          {t(errorKey(error.code), { detail: error.detail ?? '' })}
        </p>
      )}
      <div className="modal-actions">
        <button type="button" className="ghost-button" onClick={close}>
          {t('jobs.close')}
        </button>
      </div>
    </Modal>
  );
}
