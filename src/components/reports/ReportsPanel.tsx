import { useContext, useMemo, useState } from 'react';

import { Modal } from '@/components/Modal';
import { LocaleContext } from '@/i18n/core';
import { useT } from '@/i18n/useT';
import { getBackend } from '@/ipc/backend';
import { errorKey, toBackendError, type BackendError } from '@/ipc/errors';
import type { ExportFormat, LibraryView } from '@/ipc/types';
import { auditSummary, type AuditRow } from '@/lib/audit';
import { useJobStore } from '@/stores/jobStore';

export function ReportsPanel({
  view,
  onClose,
}: {
  view: LibraryView;
  onClose(): void;
}) {
  const t = useT();
  const locale = useContext(LocaleContext);
  const n = useMemo(() => new Intl.NumberFormat(locale), [locale]);
  const job = useJobStore((s) => s.job);
  const exportReport = useJobStore((s) => s.exportReport);
  const summary = useMemo(() => auditSummary(view), [view]);
  const [saved, setSaved] = useState<string | null>(null);
  const [error, setError] = useState<BackendError | null>(null);

  async function exportAudit(format: ExportFormat) {
    setError(null);
    try {
      const b = await getBackend();
      const name = await b.exportAudit(view.generation, format);
      if (name) setSaved(name);
    } catch (e) {
      setError(toBackendError(e));
    }
  }

  async function exportJob(format: ExportFormat) {
    setError(null);
    useJobStore.getState().clearError();
    const name = await exportReport(format);
    if (name) setSaved(name);
    // The store keeps a failed export as its error; show it here too.
    const failed = useJobStore.getState().error;
    if (!name && failed) setError(failed);
  }

  const cells = (r: AuditRow) => (
    <>
      <td>{n.format(r.roms)}</td>
      {summary.media.map((m) => (
        <td key={m}>{n.format(r.missing[m] ?? 0)}</td>
      ))}
      <td>{n.format(r.complete)}</td>
    </>
  );

  return (
    <Modal labelledBy="reports-title" onClose={onClose} className="reports">
      <h2 id="reports-title" className="section-title">
        {t('reports.title')}
      </h2>
      <h3 className="section-title">{t('reports.audit')}</h3>
      <table className="summary-table">
        <caption className="sr-only">{t('reports.audit')}</caption>
        <thead>
          <tr>
            <th scope="col">{t('reports.system')}</th>
            <th scope="col">{t('reports.roms')}</th>
            {summary.media.map((m) => (
              <th key={m} scope="col">
                {t(`art.none.${m}`)}
              </th>
            ))}
            <th scope="col">{t('reports.complete')}</th>
          </tr>
        </thead>
        <tbody>
          {summary.systems.map((r) => (
            <tr key={r.folder}>
              <th scope="row" title={r.folder}>
                {r.name}
              </th>
              {cells(r)}
            </tr>
          ))}
          <tr>
            <th scope="row">{t('reports.total')}</th>
            {cells(summary.total)}
          </tr>
        </tbody>
      </table>
      <div className="modal-actions">
        <button
          type="button"
          className="ghost-button"
          onClick={() => void exportAudit('csv')}
        >
          {t('reports.exportAuditCsv')}
        </button>
        <button
          type="button"
          className="ghost-button"
          onClick={() => void exportAudit('json')}
        >
          {t('reports.exportAuditJson')}
        </button>
      </div>
      <h3 className="section-title">{t('reports.jobReport')}</h3>
      {job?.result ? (
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
      ) : (
        <p className="hint">{t('reports.noJob')}</p>
      )}
      {saved && <p role="status">{t('reports.saved', { name: saved })}</p>}
      {error && (
        <p role="alert" className="error-banner">
          {t(errorKey(error.code), { detail: error.detail ?? '' })}
        </p>
      )}
      <div className="modal-actions">
        <button type="button" className="ghost-button" onClick={onClose}>
          {t('reports.close')}
        </button>
      </div>
    </Modal>
  );
}
