import { useContext, useMemo, useState } from 'react';

import { Download } from 'lucide-react';

import { Alert } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { EmptyState } from '@/components/ui/empty-state';
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
  // Opened from the toolbar, not a DialogTrigger: remember the opener so
  // focus goes back to it on close.
  const [opener] = useState(() =>
    document.activeElement instanceof HTMLElement
      ? document.activeElement
      : null,
  );

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

  const sectionTitle = 'm-0 mb-2 text-dense font-semibold text-cr-text';

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <DialogContent
        size="lg"
        hideClose
        onCloseAutoFocus={(e) => {
          e.preventDefault();
          opener?.focus();
        }}
      >
        <DialogHeader>
          <DialogTitle>{t('reports.title')}</DialogTitle>
          <DialogDescription>{t('reports.description')}</DialogDescription>
        </DialogHeader>
        <DialogBody className="grid gap-5 pb-1">
          <section className="grid gap-1">
            <h3 className={sectionTitle}>{t('reports.audit')}</h3>
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
                <tr className="font-semibold">
                  <th scope="row">{t('reports.total')}</th>
                  {cells(summary.total)}
                </tr>
              </tbody>
            </table>
            <div className="flex flex-wrap gap-2">
              <Button size="sm" onClick={() => void exportAudit('csv')}>
                <Download aria-hidden />
                {t('reports.exportAuditCsv')}
              </Button>
              <Button size="sm" onClick={() => void exportAudit('json')}>
                <Download aria-hidden />
                {t('reports.exportAuditJson')}
              </Button>
            </div>
          </section>
          <section className="grid gap-1">
            <h3 className={sectionTitle}>{t('reports.jobReport')}</h3>
            {job?.result ? (
              <div className="flex flex-wrap gap-2">
                <Button size="sm" onClick={() => void exportJob('csv')}>
                  <Download aria-hidden />
                  {t('jobs.exportCsv')}
                </Button>
                <Button size="sm" onClick={() => void exportJob('json')}>
                  <Download aria-hidden />
                  {t('jobs.exportJson')}
                </Button>
              </div>
            ) : (
              <EmptyState
                size="sm"
                titleAs="p"
                title={t('reports.noJob')}
                className="rounded-md border border-dashed border-cr-border py-4"
              />
            )}
          </section>
          {saved && (
            <Alert variant="success">
              <p className="m-0">{t('reports.saved', { name: saved })}</p>
            </Alert>
          )}
          {error && (
            <Alert variant="danger">
              <p className="m-0">
                {t(errorKey(error.code), { detail: error.detail ?? '' })}
              </p>
            </Alert>
          )}
        </DialogBody>
        <DialogFooter>
          <Button onClick={onClose}>{t('reports.close')}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
