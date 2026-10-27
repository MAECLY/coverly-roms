import { useContext, useId, useMemo, useState, type ReactNode } from 'react';

import { ExtractStage } from '@/components/progress/ExtractStage';
import { TrackList } from '@/components/progress/TrackList';
import { WorkReadout } from '@/components/progress/WorkReadout';
import { Button } from '@/components/ui/button';
import { LocaleContext } from '@/i18n/core';
import { useRichT } from '@/i18n/rich';
import { useT } from '@/i18n/useT';
import type { OrganizePlan, OrganizeProgressEvent } from '@/ipc/types';
import { useRate } from '@/hooks/useRate';
import { extractView, middleTruncate } from '@/lib/extractProgress';
import { formatDecimalSize } from '@/lib/format';
import { etaKey, etaSeconds, formatRate } from '@/lib/rate';
import { useElementSize } from '@/hooks/useElementSize';

import { hintText } from './styles';

/** Slots of the bar (same row as the library scan). */
const SLOTS = 16;
/** The track list's column (`minmax(22rem, 1fr)`) and the grid gap, in px. */
const COLUMN_PX = 22 * 16;
const GAP_PX = 16;
/** The card's own padding (`p-4`) on both sides. */
const PADDING_PX = 2 * 16;
/** Longest folder shown in the sentence before it is cut in the middle. */
const FOLDER_MAX = 48;

export interface ApplyProgressProps {
  /** Null when the job was resumed from the history (D6: no game counter). */
  plan: OrganizePlan | null;
  progress: OrganizeProgressEvent | null;
  systemName(id: string): string;
  /** Resolving to `false` means the request did not go through: Cancel is
   * offered again. */
  onCancel(): void | Promise<boolean | void>;
}

/**
 * The applying block of «Organizar por sistema» (spec §6): the stage, one
 * status sentence, the readout (bytes, speed, time left, «Juego n de N» apart
 * from «Pista k de M») and the game's track list. Pressing Cancel only marks
 * this block as cancelling (D15); the result replaces it when the job ends.
 */
export function ApplyProgress({
  plan,
  progress,
  systemName,
  onCancel,
}: ApplyProgressProps) {
  const t = useT();
  const tr = useRichT();
  const locale = useContext(LocaleContext);
  const [cancelling, setCancelling] = useState(false);
  const [cardRef, card] = useElementSize<HTMLDivElement>();
  const noteId = useId();

  const event = progress?.phase === 'extract' ? progress : null;
  const view = useMemo(
    () => (event ? extractView(plan, event) : null),
    [plan, event],
  );
  // D4: one sample series per item; the hook carries the last rate across.
  const rate = useRate(
    view && progress ? `${progress.plan_id}:${view.itemId}` : null,
    view ? view.bytes.done : null,
  );

  const size = (n: number) => formatDecimalSize(n, locale);
  const twoColumns = card.width - PADDING_PX >= 2 * COLUMN_PX + GAP_PX;

  let sentence: ReactNode;
  if (!progress) sentence = t('organize.starting');
  else if (view && cancelling)
    sentence = tr('organize.progress.cancelling', { name: view.name });
  else if (view?.phase === 'checking')
    sentence = tr('organize.progress.checking', { name: view.name });
  else if (view?.targetDir)
    sentence = tr(
      'organize.progress.extracting',
      { name: view.name, folder: middleTruncate(view.targetDir, FOLDER_MAX) },
      undefined,
      { folder: view.targetDir },
    );
  else if (view)
    sentence = tr('organize.progress.extractingName', { name: view.name });
  else
    sentence = t(
      !plan
        ? 'organize.organizing'
        : plan.mode === 'copy'
          ? 'organize.copying'
          : 'organize.applying',
      { done: progress.done, total: progress.total },
    );

  let readout: Parameters<typeof WorkReadout>[0];
  if (view) {
    const { done, total } = view.bytes;
    const amount = { done: size(done), total: size(total) };
    const timed = total > 0 && view.phase === 'decoding' && !cancelling;
    const left = etaSeconds(total - done, rate);
    const eta = left === null ? null : etaKey(left);
    readout = {
      bar: { value: total > 0 ? done : null, max: total || 1, slots: SLOTS },
      label: t('organize.progressLabel'),
      // An archive with nothing to decode has no amount to read.
      ...(total > 0
        ? {
            amount: tr('organize.progress.amount', amount),
            amountText: t('organize.progress.amount', amount),
          }
        : {}),
      ...(rate !== null && !cancelling
        ? {
            rate: (
              <span>
                {tr('organize.progress.rate', {
                  rate: formatRate(locale, rate),
                })}
              </span>
            ),
          }
        : {}),
      ...(timed
        ? { eta: eta ? <span>{t(eta.key, eta.vars)}</span> : null }
        : {}),
      counters: [
        ...(view.game
          ? [
              <span key="game">
                {tr('organize.progress.game', { ...view.game })}
              </span>,
            ]
          : []),
        ...(view.entries
          ? [
              <span key="entries">
                {tr(
                  view.noun === 'track'
                    ? 'organize.progress.track'
                    : 'organize.progress.file',
                  {
                    n: Math.min(view.entries.done + 1, view.entries.total),
                    total: view.entries.total,
                  },
                )}
              </span>,
            ]
          : []),
      ],
    };
  } else {
    const counts = progress
      ? { done: progress.done, total: progress.total }
      : null;
    readout = {
      bar:
        counts && counts.total > 0
          ? { value: counts.done, max: counts.total, slots: SLOTS }
          : { value: null, max: 1, slots: SLOTS },
      label: t('organize.progressLabel'),
      ...(counts && counts.total > 0
        ? { amountText: t('organize.progress.amount', counts) }
        : {}),
    };
  }

  const stage = view ? (
    <ExtractStage
      label={
        view.systemId
          ? t('organize.progress.stageLabel', {
              name: view.name,
              system: systemName(view.systemId),
            })
          : t('organize.progressLabel')
      }
      itemId={view.itemId}
      systemId={view.systemId}
      entries={view.entries}
      phase={view.phase}
      cancelling={cancelling}
    />
  ) : progress ? (
    <ExtractStage
      label={
        plan
          ? t('organize.progress.movingLabel', { folder: plan.target_root })
          : t('organize.progressLabel')
      }
      itemId={-1}
      systemId={null}
      entries={{ done: progress.done, total: progress.total }}
      phase="decoding"
      cancelling={cancelling}
      variant="move"
    />
  ) : null;

  const showNote = view !== null && !cancelling;
  const action = (
    <div className="grid justify-items-start gap-1">
      <Button
        variant="ghost"
        size="sm"
        disabled={cancelling}
        {...(showNote ? { 'aria-describedby': noteId } : {})}
        onClick={() => {
          setCancelling(true);
          void Promise.resolve(onCancel()).then(
            (sent) => {
              if (sent === false) setCancelling(false);
            },
            () => setCancelling(false),
          );
        }}
      >
        {t(
          cancelling ? 'organize.progress.cancellingButton' : 'organize.cancel',
        )}
      </Button>
      {showNote && (
        <p id={noteId} className={hintText}>
          {t('organize.progress.cancelNote')}
        </p>
      )}
    </div>
  );

  return (
    <div
      ref={cardRef}
      className="grid gap-4 rounded-lg border border-cr-border bg-cr-surface-raised p-4 [grid-template-columns:repeat(auto-fit,minmax(min(22rem,100%),1fr))]"
    >
      <div className="grid min-w-0 content-start gap-3">
        {stage}
        <p role="status" className="m-0 min-w-0 break-words text-ui">
          {sentence}
        </p>
        <WorkReadout {...readout} action={action} />
      </div>
      {view && view.rows.length > 0 && (
        <TrackList
          rows={view.rows}
          noun={view.noun}
          name={view.name}
          collapsible={!twoColumns}
        />
      )}
    </div>
  );
}
