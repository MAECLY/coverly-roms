/**
 * What an apply did (spec §11.3 "Done"): the count and where the files
 * went, the bytes that left ROMS/ (and, on the same card, that the card is
 * just as full), open the quarantine, undo (asks first) and back to the
 * library. Anything left alone turns the alert to warning with each file
 * and its reason, then "No se ha movido nada más.". Covers that left with
 * a copy offer "Buscar portadas para {n} juegos".
 */
import {
  ArrowLeft,
  ChevronRight,
  FolderOpen,
  Image,
  Undo2,
} from 'lucide-react';
import { useContext, useId } from 'react';

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
import { LocaleContext } from '@/i18n/core';
import { useT, type TFunction } from '@/i18n/useT';
import type {
  DedupeActionSetting,
  DedupeItemStatusView,
  DedupePlanView,
  DedupeReportView,
} from '@/ipc/dedupeTypes';
import { fileName, formatBytes } from '@/lib/dedupe';

import './dedupe.css';

export interface DedupeResultProps {
  plan: DedupePlanView;
  report: DedupeReportView;
  action: DedupeActionSetting;
  onOpenFolder(): void;
  onUndo(): void;
  onBack(): void;
  onFindCovers?: (() => void) | undefined;
}

function statusText(
  t: TFunction,
  s: DedupeItemStatusView,
  folder: string,
): string {
  switch (s.status) {
    case 'quarantined':
      return t('dedupe.outcome.quarantined');
    case 'trashed':
      return t('dedupe.outcome.trashed');
    case 'failed':
      return t('dedupe.outcome.failed', { reason: t(`jobs.error.${s.kind}`) });
    case 'skipped':
      switch (s.reason.kind) {
        case 'keeper-changed':
          return t('dedupe.skip.keeperChanged');
        case 'changed-since-plan':
          return t('dedupe.skip.changed');
        case 'source-missing':
          return t('dedupe.skip.sourceMissing');
        case 'destination-limit':
          return t('dedupe.skip.destinationLimit', {
            folder,
            why: t(`dedupe.limit.${s.reason.why}`),
          });
        case 'trash-unavailable':
          return t('dedupe.skip.trashUnavailable');
        case 'report-only':
          return t('dedupe.skip.reportOnly');
        case 'taken':
          return t('dedupe.skip.taken', { folder });
        case 'excluded':
          return t('dedupe.skip.excluded');
        case 'conflict':
          return t('dedupe.skip.conflict');
        case 'cancelled':
          return t('dedupe.skip.cancelled');
      }
  }
}

const done = (s: DedupeItemStatusView) =>
  s.status === 'quarantined' || s.status === 'trashed';

export function DedupeResult({
  plan,
  report,
  action,
  onOpenFolder,
  onUndo,
  onBack,
  onFindCovers,
}: DedupeResultProps) {
  const t = useT();
  const locale = useContext(LocaleContext);
  const titleId = `${useId()}-title`;
  const folder = `${fileName(plan.quarantine)}/`;
  const roms = `${fileName(plan.roms_dir)}/`;

  const members = new Map<string, string>();
  for (const g of plan.groups)
    for (const m of g.members)
      members.set(`${g.id}:${m.id}`, fileName(m.unit.primary));
  const nameOf = (group: number, member: number) =>
    members.get(`${group}:${member}`) ?? `#${group}/${member}`;

  const moved = report.outcomes.filter(done).length;
  const left = report.outcomes.filter((o) => !done(o));
  const bytes = formatBytes(report.bytes_out, locale);
  const trash = action === 'trash';
  const headline = trash
    ? t('dedupe.doneTrash', { count: moved })
    : t('dedupe.done', { count: moved, folder });
  const covers = report.art_left.length;

  return (
    <section className="dup-result" aria-labelledby={titleId}>
      {left.length === 0 ? (
        <Alert variant="success">
          <AlertTitle id={titleId}>{headline}</AlertTitle>
          <p className="dup-result__freed">
            {t('dedupe.freed', { folder: roms, bytes })}
          </p>
        </Alert>
      ) : (
        <Alert variant="warning">
          <AlertTitle id={titleId} className="tabular-nums">
            {t('dedupe.partial', { done: moved, skipped: left.length })}
          </AlertTitle>
          <ul className="dup-result__left">
            {left.map((o) => (
              <li key={`${o.group}:${o.member}`}>
                <span className="dup-mono">{nameOf(o.group, o.member)}</span>{' '}
                <span>{statusText(t, o, folder)}</span>
              </li>
            ))}
          </ul>
          <p>{t('dedupe.nothingElse')}</p>
          <p className="dup-result__freed">
            {t('dedupe.freed', { folder: roms, bytes })}
          </p>
        </Alert>
      )}

      {!trash && plan.same_volume && plan.removable && moved > 0 && (
        <p className="dup-result__note">
          {t('dedupe.sameVolumeNote', { quarantine: folder })}
        </p>
      )}

      <div className="dup-result__actions">
        {!trash && (
          <Button variant="primary" onClick={onOpenFolder}>
            <FolderOpen aria-hidden />
            {t('dedupe.openFolder', { folder })}
          </Button>
        )}
        {moved > 0 && (
          <AlertDialog>
            <AlertDialogTrigger asChild>
              <Button variant="outline">
                <Undo2 aria-hidden />
                {t('dedupe.undo.button')}
              </Button>
            </AlertDialogTrigger>
            <AlertDialogContent>
              <AlertDialogHeader>
                <AlertDialogTitle>
                  {t('dedupe.undo.title', { count: moved })}
                </AlertDialogTitle>
                {trash && (
                  <AlertDialogDescription>
                    {t('dedupe.trash.caveat')}
                  </AlertDialogDescription>
                )}
              </AlertDialogHeader>
              <AlertDialogFooter>
                <AlertDialogCancel>{t('jobs.undoNo')}</AlertDialogCancel>
                <AlertDialogAction onClick={onUndo}>
                  {t('dedupe.undo.action', { count: moved })}
                </AlertDialogAction>
              </AlertDialogFooter>
            </AlertDialogContent>
          </AlertDialog>
        )}
        {covers > 0 && onFindCovers && (
          <Button variant="outline" onClick={onFindCovers}>
            <Image aria-hidden />
            {covers === 1
              ? t('dedupe.findCoversOne')
              : t('dedupe.findCovers', { count: covers })}
          </Button>
        )}
        <Button variant="ghost" onClick={onBack}>
          <ArrowLeft aria-hidden />
          {t('dedupe.backToLibrary')}
        </Button>
      </div>

      {report.outcomes.length > 0 && (
        <Collapsible className="dup-unchanged">
          <CollapsibleTrigger asChild>
            <button type="button" className="dup-unchanged__trigger group">
              <ChevronRight
                aria-hidden
                size={16}
                className="dup-system__chevron"
              />
              {t('dedupe.details', { count: report.outcomes.length })}
            </button>
          </CollapsibleTrigger>
          <CollapsibleContent>
            <ul className="dup-unchanged__list">
              {report.outcomes.map((o) => (
                <li
                  key={`${o.group}:${o.member}`}
                  className="dup-unchanged__row"
                >
                  <span className="dup-mono dup-unchanged__file">
                    {nameOf(o.group, o.member)}
                  </span>
                  <span className="dup-unchanged__why">
                    {statusText(t, o, folder)}
                  </span>
                </li>
              ))}
            </ul>
          </CollapsibleContent>
        </Collapsible>
      )}
    </section>
  );
}
