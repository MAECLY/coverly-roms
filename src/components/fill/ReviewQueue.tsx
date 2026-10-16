import { useContext, useEffect, useMemo, useRef, useState } from 'react';

import { ArtThumb } from '@/components/library/ArtThumb';
import { Modal } from '@/components/Modal';
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
import { useJobStore } from '@/stores/jobStore';

import { CandidateImage } from './CandidateImage';

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
      if (key === 'ArrowRight' || key === 'ArrowDown' || key === 'j') {
        ev.preventDefault();
        move(1);
      } else if (key === 'ArrowLeft' || key === 'ArrowUp' || key === 'k') {
        ev.preventDefault();
        move(-1);
      } else if (!editable) {
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
    <Modal labelledBy="review-title" onClose={onClose} className="review">
      <h2 id="review-title" className="section-title">
        {t('review.title')}
      </h2>
      {items === null && !loadError && (
        <p role="status">{t('review.loading')}</p>
      )}
      {items !== null && items.length === 0 && <p>{t('review.empty')}</p>}
      {item && items && (
        <>
          <p className="hint">
            {t('review.position', {
              n: n.format(pos + 1),
              total: n.format(items.length),
              decided: n.format(decided),
            })}
          </p>
          <h3 className="review-name truncate" title={item.name}>
            {item.name}
          </h3>
          <p className="hint">
            {item.folder} · {t(`art.media.${item.media}`)} ·{' '}
            {status(decisions[item.index])}
          </p>
          {item.note === 'unreleased-variant' && (
            <p className="hint review-note">{t('review.variantNote')}</p>
          )}
          <div className="review-compare">
            <figure className="review-current">
              {entry ? (
                <ArtThumb
                  generation={view.generation}
                  entry={entry}
                  media={item.media}
                />
              ) : (
                <div className="art-thumb art-placeholder" aria-hidden="true" />
              )}
              <figcaption>{t('review.current')}</figcaption>
            </figure>
            <div
              role="group"
              aria-label={t('review.candidates')}
              className="candidate-grid"
            >
              {item.candidates.map((c) => (
                <button
                  key={c.index}
                  type="button"
                  className="candidate"
                  aria-pressed={chosen === c.index}
                  onClick={() => pick(c.index)}
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
                  <span className="truncate" title={c.name}>
                    {c.name}
                  </span>
                  <span className="hint">
                    {t('review.meta', {
                      key: c.index + 1,
                      source: c.source,
                      regions:
                        c.regions.map((r) => t(`regions.${r}`)).join(', ') ||
                        '—',
                      score: Math.round(c.confidence * 100),
                    })}
                  </span>
                </button>
              ))}
            </div>
          </div>
        </>
      )}
      {error && (
        <p role="alert" className="error-banner">
          {t(errorKey(error.code), { detail: error.detail ?? '' })}
        </p>
      )}
      <p className="hint">{t('review.keys')}</p>
      <div className="modal-actions">
        <button
          type="button"
          className="ghost-button"
          disabled={pos === 0}
          onClick={() => move(-1)}
        >
          {t('review.previous')}
        </button>
        <button
          type="button"
          className="ghost-button"
          disabled={!items || pos >= items.length - 1}
          onClick={() => move(1)}
        >
          {t('review.next')}
        </button>
        <button
          type="button"
          className="ghost-button"
          disabled={!editable || pending || !item}
          onClick={() => void skip()}
        >
          {t('review.skip')}
        </button>
        <button
          type="button"
          className="ghost-button"
          disabled={!editable || pending || !items}
          onClick={() => void acceptAll()}
        >
          {t('review.acceptAll')}
        </button>
        <button
          type="button"
          className="primary-button"
          disabled={!editable || pending || !item}
          onClick={() => void accept()}
        >
          {t('review.accept')}
        </button>
        <button type="button" className="ghost-button" onClick={onClose}>
          {t('review.done')}
        </button>
      </div>
    </Modal>
  );
}
