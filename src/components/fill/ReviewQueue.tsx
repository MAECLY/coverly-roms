import { ChevronLeft, ChevronRight } from 'lucide-react';
import { useContext, useEffect, useMemo, useRef, useState } from 'react';

import { ArtThumb } from '@/components/library/ArtThumb';
import { SystemIcon } from '@/components/system/SystemIcon';
import { Alert } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Kbd, KbdGroup } from '@/components/ui/kbd';
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group';
import { LocaleContext } from '@/i18n/core';
import { useT } from '@/i18n/useT';
import { getBackend } from '@/ipc/backend';
import { errorKey, toBackendError, type BackendError } from '@/ipc/errors';
import type {
  Decision,
  DecisionIn,
  JobItemView,
  LibraryView,
} from '@/ipc/types';
import { systemByFolder } from '@/lib/systemLookup';
import { cn } from '@/lib/utils';
import { useJobStore } from '@/stores/jobStore';

import { CandidateImage } from './CandidateImage';
import { CandidateMeta } from './CandidateMeta';

export function ReviewQueue({
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
  const decide = useJobStore((s) => s.decide);
  const storeError = useJobStore((s) => s.error);
  const pending = useJobStore((s) => s.pending);
  const [items, setItems] = useState<JobItemView[] | null>(null);
  const [loadError, setLoadError] = useState<BackendError | null>(null);
  const [pos, setPos] = useState(0);
  const [selected, setSelected] = useState<Record<number, number>>({});
  const [decisions, setDecisions] = useState<Record<number, Decision>>({});
  const jobId = job?.id ?? null;
  const editable = job?.state === 'planned';
  /** A decision is in flight: a second key press must not decide (or move) again. */
  const busy = useRef(false);
  const contentRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (jobId === null) return;
    let alive = true;
    void getBackend()
      .then((b) => b.fillJobItems(jobId, 'review'))
      .then(
        (list) => {
          if (!alive) return;
          setItems(list);
          setDecisions(
            Object.fromEntries(
              list.flatMap((i) => (i.decision ? [[i.index, i.decision]] : [])),
            ),
          );
        },
        (e: unknown) => alive && setLoadError(toBackendError(e)),
      );
    return () => {
      alive = false;
    };
  }, [jobId]);

  const item = items?.[pos] ?? null;
  const chosen = item ? (selected[item.index] ?? 0) : 0;
  const decided = Object.keys(decisions).length;

  function move(delta: number) {
    setPos((p) => Math.max(0, Math.min((items?.length ?? 1) - 1, p + delta)));
  }

  async function send(list: DecisionIn[]): Promise<boolean> {
    if (list.length === 0) return true;
    if (busy.current) return false;
    busy.current = true;
    try {
      if (!(await decide(list))) return false;
    } finally {
      busy.current = false;
    }
    setDecisions((d) => {
      const next = { ...d };
      for (const { index, ...decision } of list)
        next[index] = decision as Decision;
      return next;
    });
    return true;
  }

  async function accept() {
    if (!item) return;
    const d: Decision =
      chosen === 0
        ? { choice: 'accept' }
        : { choice: 'pick', candidate: chosen };
    if (await send([{ ...d, index: item.index }])) move(1);
  }

  async function skip() {
    if (!item) return;
    if (await send([{ choice: 'skip', index: item.index }])) move(1);
  }

  async function acceptAll() {
    if (!items) return;
    await send(
      items
        .filter((i) => !decisions[i.index])
        .map((i) => ({ choice: 'accept' as const, index: i.index })),
    );
  }

  function pick(candidate: number) {
    if (!item || candidate >= item.candidates.length) return;
    setSelected((s) => ({ ...s, [item.index]: candidate }));
  }

  // Re-subscribed on every render so the keys always act on the shown item.
  useEffect(() => {
    function onKey(ev: KeyboardEvent) {
      if (ev.metaKey || ev.ctrlKey || ev.altKey) return;
      const key = ev.key.length === 1 ? ev.key.toLowerCase() : ev.key;
      // Enter and Space belong to the focused control (Skip, Done, a candidate…):
      // taking them here would accept an item the user meant to skip.
      if (
        (key === 'Enter' || key === ' ') &&
        ev.target instanceof Element &&
        ev.target.closest('button, a, input, select, textarea')
      )
        return;
      // Up and Down are left to scroll the dialog at small window heights.
      if (key === 'ArrowRight' || key === 'j') {
        ev.preventDefault();
        move(1);
      } else if (key === 'ArrowLeft' || key === 'k') {
        ev.preventDefault();
        move(-1);
      } else if (!editable || ev.repeat) {
        // A held key must not decide item after item.
        return;
      } else if (key === 'Enter') {
        ev.preventDefault();
        void accept();
      } else if (key === 's') {
        void skip();
      } else if (key === 'a') {
        void acceptAll();
      } else if (/^[1-9]$/.test(key)) {
        pick(Number(key) - 1);
      }
    }
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });

  const entry =
    item?.entry_id !== null && item?.entry_id !== undefined
      ? (view.entries[item.entry_id] ?? null)
      : null;
  const status = (d: Decision | undefined) =>
    !d
      ? t('review.undecided')
      : d.choice === 'skip'
        ? t('review.skipped')
        : t('review.accepted');
  const error = loadError ?? storeError;

  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent
        size="xl"
        hideClose
        aria-describedby={undefined}
        ref={contentRef}
        // Focus the dialog itself, as before: the keys act on the whole queue.
        onOpenAutoFocus={(ev) => {
          ev.preventDefault();
          contentRef.current?.focus();
        }}
      >
        <DialogHeader>
          <DialogTitle>{t('review.title')}</DialogTitle>
          {item && items && (
            <p className="m-0 text-caption tabular-nums text-cr-text-secondary">
              {t('review.position', {
                n: n.format(pos + 1),
                total: n.format(items.length),
                decided: n.format(decided),
              })}
            </p>
          )}
        </DialogHeader>
        <DialogBody className="grid content-start gap-3">
          {jobId === null && <p className="m-0">{t('errors.no-job')}</p>}
          {jobId !== null && items === null && !loadError && (
            <p role="status" className="m-0">
              {t('review.loading')}
            </p>
          )}
          {items !== null && items.length === 0 && (
            <p className="m-0">{t('review.empty')}</p>
          )}
          {item && items && (
            <>
              <div className="grid gap-0.5">
                <h3
                  className="m-0 truncate text-dialog font-semibold text-cr-text [unicode-bidi:plaintext]"
                  title={item.name}
                >
                  {item.name}
                </h3>
                <p className="hint inline-flex items-center gap-1.5">
                  <SystemIcon
                    systemId={
                      systemByFolder(view.systems, item.folder)?.system_id ??
                      null
                    }
                    size={16}
                  />
                  {item.folder} · {t(`art.media.${item.media}`)} ·{' '}
                  {status(decisions[item.index])}
                </p>
              </div>
              {item.note === 'unreleased-variant' && (
                <Alert variant="warning">{t('review.variantNote')}</Alert>
              )}
              <div className="grid grid-cols-[10rem_1fr] items-start gap-4">
                <figure className="review-current m-0 grid gap-1.5">
                  {entry ? (
                    <ArtThumb
                      generation={view.generation}
                      entry={entry}
                      media={item.media}
                    />
                  ) : (
                    <div
                      className="art-thumb art-placeholder"
                      aria-hidden="true"
                    />
                  )}
                  <figcaption className="text-caption text-cr-text-secondary">
                    {t('review.current')}
                  </figcaption>
                </figure>
                <ToggleGroup
                  type="single"
                  // Arrow keys belong to the queue (previous / next item).
                  rovingFocus={false}
                  value={String(chosen)}
                  onValueChange={(v) => v !== '' && pick(Number(v))}
                  aria-label={t('review.candidates')}
                  className="grid grid-cols-[repeat(auto-fill,minmax(11rem,1fr))] items-start gap-3"
                >
                  {item.candidates.map((c) => (
                    <ToggleGroupItem
                      key={c.index}
                      value={String(c.index)}
                      className={cn(
                        'grid h-auto min-w-0 content-start justify-stretch gap-1 whitespace-normal rounded-lg border-2 border-transparent p-2 text-left font-normal text-cr-text [unicode-bidi:plaintext]',
                        'data-[state=on]:border-primary data-[state=on]:bg-cr-primary-subtle/40 data-[state=on]:text-cr-text',
                      )}
                    >
                      {jobId !== null && (
                        <CandidateImage
                          source={{
                            kind: 'job',
                            job: jobId,
                            index: item.index,
                            candidate: c.index,
                          }}
                          label={c.name}
                        />
                      )}
                      <span className="truncate font-medium" title={c.name}>
                        {c.name}
                      </span>
                      <CandidateMeta c={c} position={c.index + 1} />
                    </ToggleGroupItem>
                  ))}
                </ToggleGroup>
              </div>
            </>
          )}
          {error && (
            <Alert variant="danger">
              {t(errorKey(error.code), { detail: error.detail ?? '' })}
            </Alert>
          )}
        </DialogBody>
        <ul
          aria-label={t('review.shortcuts.label')}
          className="m-0 flex list-none flex-wrap gap-x-3 gap-y-1 p-0 text-caption text-cr-text-secondary"
        >
          {SHORTCUTS.map(([keys, what]) => (
            <li key={what} className="inline-flex items-center gap-1">
              <KbdGroup>
                {keys.map((k) => (
                  <Kbd key={k}>{k.startsWith('review.') ? t(k) : k}</Kbd>
                ))}
              </KbdGroup>
              {t(`review.shortcuts.${what}`)}
            </li>
          ))}
        </ul>
        <DialogFooter>
          <Button
            variant="outline"
            disabled={pos === 0}
            onClick={() => move(-1)}
          >
            <ChevronLeft aria-hidden />
            {t('review.previous')}
          </Button>
          <Button
            variant="outline"
            className="mr-auto"
            disabled={!items || pos >= items.length - 1}
            onClick={() => move(1)}
          >
            {t('review.next')}
            <ChevronRight aria-hidden />
          </Button>
          <Button
            variant="outline"
            disabled={!editable || pending || !item}
            onClick={() => void skip()}
          >
            {t('review.skip')}
          </Button>
          <Button
            variant="outline"
            disabled={!editable || pending || !items}
            onClick={() => void acceptAll()}
          >
            {t('review.acceptAll')}
          </Button>
          <Button
            variant="primary"
            disabled={!editable || pending || !item}
            onClick={() => void accept()}
          >
            {t('review.accept')}
          </Button>
          <Button variant="ghost" onClick={onClose}>
            {t('review.done')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

/** The shortcut legend: key caps (literal, or an i18n key) and what they do. */
const SHORTCUTS: readonly [readonly string[], string][] = [
  [['←', '→'], 'move'],
  [['1–9'], 'choose'],
  [['review.shortcuts.enter'], 'accept'],
  [['S'], 'skip'],
  [['A'], 'acceptAll'],
  [['review.shortcuts.esc'], 'back'],
];
