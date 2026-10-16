import { useContext, useEffect, useMemo, useState } from 'react';

import { Modal } from '@/components/Modal';
import { LocaleContext } from '@/i18n/core';
import { useT } from '@/i18n/useT';
import { getBackend } from '@/ipc/backend';
import { errorKey } from '@/ipc/errors';
import {
  STRATEGIES,
  type EntryView,
  type JobItemView,
  type JobView,
  type LibraryView,
  type Media,
  type OverwritePolicy,
  type RegionPref,
  type StrategyKind,
} from '@/ipc/types';
import {
  buildFillRequest,
  DEFAULT_REGION_ORDER,
  moveItem,
  type FillScope,
} from '@/lib/fill';
import { useJobStore } from '@/stores/jobStore';
import { useViewStore } from '@/stores/viewStore';

import { CandidateImage } from './CandidateImage';

type Step = 'scope' | 'media' | 'strategy' | 'overwrite' | 'summary';
const STEPS: readonly Step[] = [
  'scope',
  'media',
  'strategy',
  'overwrite',
  'summary',
];
const SCOPES: readonly FillScope[] = ['selection', 'filter', 'library'];
const OVERWRITES: readonly OverwritePolicy[] = ['never', 'backup'];

/** A job the summary step can show: a dry run of this library still in play. */
function inPlay(job: JobView | null, generation: number): boolean {
  return (
    job !== null &&
    job.generation === generation &&
    (job.state === 'gathering' ||
      job.state === 'planned' ||
      (job.state === 'paused' && job.paused?.phase === 'gather'))
  );
}

function Samples({ job }: { job: JobView }) {
  const t = useT();
  const [items, setItems] = useState<JobItemView[]>([]);
  useEffect(() => {
    let alive = true;
    void getBackend()
      .then((b) => b.fillJobItems(job.id, 'auto'))
      .then(
        (list) => alive && setItems(list.slice(0, 4)),
        () => alive && setItems([]),
      );
    return () => {
      alive = false;
    };
  }, [job.id]);
  if (items.length === 0) return null;
  return (
    <section aria-label={t('fill.samples')}>
      <h3 className="section-title">{t('fill.samples')}</h3>
      <ul className="sample-grid">
        {items.map((i) => (
          <li key={i.index}>
            <CandidateImage
              source={{
                kind: 'job',
                job: job.id,
                index: i.index,
                candidate: 0,
              }}
              label={i.candidates[0]?.name ?? i.name}
            />
            <span className="truncate" title={i.name}>
              {i.name}
            </span>
          </li>
        ))}
      </ul>
    </section>
  );
}

export function FillWizard({
  view,
  visible,
  selectedId,
  onClose,
}: {
  view: LibraryView;
  visible: readonly EntryView[];
  selectedId: number | null;
  onClose(): void;
}) {
  const t = useT();
  const locale = useContext(LocaleContext);
  const n = useMemo(() => new Intl.NumberFormat(locale), [locale]);
  const job = useJobStore((s) => s.job);
  const progress = useJobStore((s) => s.progress);
  const error = useJobStore((s) => s.error);
  const pending = useJobStore((s) => s.pending);
  const start = useJobStore((s) => s.start);
  const run = useJobStore((s) => s.run);
  const cancel = useJobStore((s) => s.cancel);
  const resume = useJobStore((s) => s.resume);
  const setOverlay = useViewStore((s) => s.setOverlay);
  const [step, setStep] = useState<Step>(() =>
    inPlay(useJobStore.getState().job, view.generation) ? 'summary' : 'scope',
  );
  const [scope, setScope] = useState<FillScope>(
    selectedId !== null
      ? 'selection'
      : visible.length < view.entries.length
        ? 'filter'
        : 'library',
  );
  const [media, setMedia] = useState<Media[]>(view.profile.media.slice(0, 1));
  const [strategy, setStrategy] = useState<StrategyKind>('rom-region');
  const [regions, setRegions] = useState<RegionPref[]>([
    ...DEFAULT_REGION_ORDER,
  ]);
  const [overwrite, setOverwrite] = useState<OverwritePolicy>('never');
  const index = STEPS.indexOf(step);
  const regionName = (r: RegionPref) =>
    r === 'rom' ? t('fill.regionRom') : t(`regions.${r}`);
  const counts: Record<FillScope, number> = {
    selection: selectedId === null ? 0 : 1,
    filter: visible.length,
    library: view.entries.length,
  };

  async function preview() {
    setStep('summary');
    await start(
      view.generation,
      buildFillRequest({
        scope,
        selectedId,
        visible,
        media,
        strategy,
        regions,
        overwrite,
      }),
    );
  }

  async function runJob() {
    if (await run()) setOverlay('none');
  }

  async function discard() {
    await cancel();
    onClose();
  }

  let body;
  if (step === 'scope') {
    body = (
      <fieldset className="wizard-fieldset">
        <legend className="section-title">{t('fill.scope.title')}</legend>
        {SCOPES.map((s) => (
          <label key={s} className="wizard-choice">
            <input
              type="radio"
              name="fill-scope"
              checked={scope === s}
              disabled={counts[s] === 0}
              onChange={() => setScope(s)}
            />
            {t(`fill.scope.${s}`, { n: n.format(counts[s]) })}
          </label>
        ))}
      </fieldset>
    );
  } else if (step === 'media') {
    body = (
      <fieldset className="wizard-fieldset">
        <legend className="section-title">{t('fill.media.title')}</legend>
        {view.profile.media.map((m) => (
          <label key={m} className="wizard-choice">
            <input
              type="checkbox"
              name={`fill-media-${m}`}
              checked={media.includes(m)}
              onChange={() =>
                setMedia((list) =>
                  list.includes(m) ? list.filter((x) => x !== m) : [...list, m],
                )
              }
            />
            {t(`art.media.${m}`)}
          </label>
        ))}
      </fieldset>
    );
  } else if (step === 'strategy') {
    body = (
      <>
        <fieldset className="wizard-fieldset">
          <legend className="section-title">{t('fill.strategy.title')}</legend>
          {STRATEGIES.map((s) => (
            <label key={s} className="wizard-choice">
              <input
                type="radio"
                name="fill-strategy"
                checked={strategy === s}
                onChange={() => setStrategy(s)}
              />
              <span>
                {t(`fill.strategy.${s}`)}
                <span className="hint wizard-hint">
                  {t(`fill.strategyHint.${s}`)}
                </span>
              </span>
            </label>
          ))}
        </fieldset>
        <h3 className="section-title" id="fill-regions">
          {t('fill.regions')}
        </h3>
        <ol className="region-order" aria-labelledby="fill-regions">
          {regions.map((r, i) => (
            <li key={r}>
              <span>{regionName(r)}</span>
              <button
                type="button"
                className="icon-button"
                disabled={i === 0}
                aria-label={t('fill.moveUp', { region: regionName(r) })}
                onClick={() => setRegions((l) => moveItem(l, i, -1))}
              >
                ↑
              </button>
              <button
                type="button"
                className="icon-button"
                disabled={i === regions.length - 1}
                aria-label={t('fill.moveDown', { region: regionName(r) })}
                onClick={() => setRegions((l) => moveItem(l, i, 1))}
              >
                ↓
              </button>
            </li>
          ))}
        </ol>
      </>
    );
  } else if (step === 'overwrite') {
    body = (
      <fieldset className="wizard-fieldset">
        <legend className="section-title">{t('fill.overwrite.title')}</legend>
        {OVERWRITES.map((o) => (
          <label key={o} className="wizard-choice">
            <input
              type="radio"
              name="fill-overwrite"
              checked={overwrite === o}
              onChange={() => setOverwrite(o)}
            />
            {t(`fill.overwrite.${o}`)}
          </label>
        ))}
        <p className="hint">{t('fill.overwrite.backupHint')}</p>
      </fieldset>
    );
  } else {
    body = renderSummary();
  }

  /** The summary step (a plain render function: `Samples` must not remount on every progress event). */
  function renderSummary() {
    if (!job || job.generation !== view.generation) return null;
    if (job.state === 'gathering') {
      const p = progress?.job === job.id ? progress : null;
      return (
        <div className="progress-row">
          <p role="status">
            {t('fill.gathering', {
              done: n.format(p?.done ?? 0),
              total: n.format(p?.total ?? 0),
            })}
          </p>
          <progress
            max={p?.total || 1}
            value={p?.done ?? 0}
            aria-label={t('fill.summary')}
          />
          {p?.label ? (
            <p className="hint truncate" title={p.label}>
              {p.label}
            </p>
          ) : null}
        </div>
      );
    }
    if (job.state === 'paused' && job.paused) {
      return (
        <div role="alert" className="pause-banner">
          <p>{t(`jobs.pause.${job.paused.kind}`)}</p>
          <p className="hint">{t('fill.pausedHint')}</p>
          <button
            type="button"
            className="primary-button"
            disabled={pending}
            onClick={() => void resume()}
          >
            {t('jobs.resume')}
          </button>
        </div>
      );
    }
    if (job.state === 'failed') {
      return (
        <p role="alert" className="error-banner">
          {t('fill.failed', { detail: job.error ?? '' })}
        </p>
      );
    }
    if (!job.plan) return null;
    const { totals, per_system } = job.plan;
    if (totals.auto + totals.review + totals.none === 0) {
      return <p>{t('fill.nothingToDo')}</p>;
    }
    return (
      <>
        <p className="hint">{t('fill.summaryHint')}</p>
        <table className="summary-table">
          <caption className="sr-only">{t('fill.summary')}</caption>
          <thead>
            <tr>
              <th scope="col">{t('fill.system')}</th>
              <th scope="col">{t('fill.auto')}</th>
              <th scope="col">{t('fill.review')}</th>
              <th scope="col">{t('fill.none')}</th>
            </tr>
          </thead>
          <tbody>
            {per_system.map((s) => (
              <tr key={s.folder}>
                <th scope="row">{s.folder}</th>
                <td>{n.format(s.auto)}</td>
                <td>{n.format(s.review)}</td>
                <td>{n.format(s.none)}</td>
              </tr>
            ))}
            <tr>
              <th scope="row">{t('fill.total')}</th>
              <td>{n.format(totals.auto)}</td>
              <td>{n.format(totals.review)}</td>
              <td>{n.format(totals.none)}</td>
            </tr>
          </tbody>
        </table>
        <Samples job={job} />
      </>
    );
  }

  const planned =
    job !== null &&
    job.generation === view.generation &&
    job.state === 'planned';
  return (
    <Modal labelledBy="fill-title" onClose={onClose} className="wizard">
      <h2 id="fill-title" className="section-title">
        {t('fill.title')}
      </h2>
      <p className="hint">
        {t('fill.stepOf', { n: index + 1, total: STEPS.length })}
      </p>
      {view.profile.id === 'batocera' && (
        <p className="hint">{t('fill.batocera')}</p>
      )}
      {body}
      {error && (
        <p role="alert" className="error-banner">
          {t(errorKey(error.code), { detail: error.detail ?? '' })}
        </p>
      )}
      <div className="modal-actions">
        {step !== 'summary' && index > 0 && (
          <button
            type="button"
            className="ghost-button"
            onClick={() => setStep(STEPS[index - 1] ?? 'scope')}
          >
            {t('fill.back')}
          </button>
        )}
        {step === 'summary' && planned && (
          <button
            type="button"
            className="ghost-button"
            onClick={() => void discard()}
          >
            {t('fill.discard')}
          </button>
        )}
        <button type="button" className="ghost-button" onClick={onClose}>
          {t('fill.close')}
        </button>
        {index < STEPS.indexOf('overwrite') && (
          <button
            type="button"
            className="primary-button"
            disabled={step === 'media' && media.length === 0}
            onClick={() => setStep(STEPS[index + 1] ?? 'summary')}
          >
            {t('fill.next')}
          </button>
        )}
        {step === 'overwrite' && (
          <button
            type="button"
            className="primary-button"
            disabled={pending || media.length === 0}
            onClick={() => void preview()}
          >
            {t('fill.preview')}
          </button>
        )}
        {planned && job.plan && (
          <button
            type="button"
            className="primary-button"
            disabled={pending}
            onClick={() => void runJob()}
          >
            {t('fill.run', { n: n.format(job.plan.totals.auto + job.decided) })}
          </button>
        )}
      </div>
    </Modal>
  );
}
