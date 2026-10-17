import { ChevronDown, ChevronUp } from 'lucide-react';
import {
  useContext,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react';

import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Label } from '@/components/ui/label';
import { Progress } from '@/components/ui/progress';
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group';
import { SimpleTooltip, TooltipProvider } from '@/components/ui/tooltip';
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
  type SourceInfo,
  type StrategyKind,
} from '@/ipc/types';
import {
  buildFillRequest,
  moveItem,
  shouldAutoRun,
  wizardDefaults,
  type FillScope,
} from '@/lib/fill';
import { cn } from '@/lib/utils';
import { useJobStore } from '@/stores/jobStore';
import { useSettingsStore } from '@/stores/settingsStore';
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
    <section aria-label={t('fill.samples')} className="grid gap-2">
      <h3 className={groupTitle}>{t('fill.samples')}</h3>
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
  const settings = useSettingsStore((s) => s.settings);
  const [defaults] = useState(() =>
    wizardDefaults(useSettingsStore.getState().settings, view.profile.media),
  );
  /** Only a plan this wizard started may run by itself, and only once. */
  const startedHere = useRef(false);
  const autoRan = useRef(false);
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
  const [media, setMedia] = useState<Media[]>(defaults.media);
  const [strategy, setStrategy] = useState<StrategyKind>(defaults.strategy);
  const [regions, setRegions] = useState<RegionPref[]>(defaults.regions);
  // `null` while the "ask" setting waits for the user's choice (D25).
  const [overwrite, setOverwrite] = useState<OverwritePolicy | null>(
    defaults.overwrite,
  );
  const index = STEPS.indexOf(step);
  const uid = useId();
  const contentRef = useRef<HTMLDivElement>(null);
  /** The last reorder, said by a live region (the buttons move under focus). */
  const [moved, setMoved] = useState('');
  // Asked once, when Popularity is first chosen: are SteamGridDB votes usable?
  const [sources, setSources] = useState<SourceInfo[] | null>(null);
  const wantSources = strategy === 'popularity' && sources === null;
  useEffect(() => {
    if (!wantSources) return;
    let alive = true;
    void getBackend()
      .then((b) => b.sourcesInfo())
      .then(
        (list) => alive && setSources(list),
        // Unknown readiness reads as "not ready": the hint only informs.
        () => alive && setSources([]),
      );
    return () => {
      alive = false;
    };
  }, [wantSources]);
  const sgdbVotes =
    sources?.some((i) => i.id === 'steamgriddb' && i.ready) === true &&
    (settings?.sources.order.some((e) => e.id === 'steamgriddb' && e.enabled) ??
      false);
  const regionName = (r: RegionPref) =>
    r === 'rom' ? t('fill.regionRom') : t(`regions.${r}`);
  const counts: Record<FillScope, number> = {
    selection: selectedId === null ? 0 : 1,
    filter: visible.length,
    library: view.entries.length,
  };

  async function preview() {
    if (overwrite === null) return;
    setStep('summary');
    startedHere.current = true;
    autoRan.current = false;
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
    if (await run()) setOverlay('jobs');
  }

  const ownJob =
    job !== null && job.generation === view.generation ? job : null;
  useEffect(() => {
    if (
      !startedHere.current ||
      !shouldAutoRun(ownJob, settings, autoRan.current)
    ) {
      return;
    }
    autoRan.current = true;
    void runJob();
    // `runJob` only reads stores; re-running on its identity is not wanted.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ownJob, settings]);

  async function discard() {
    await cancel();
    onClose();
  }

  function moveRegion(i: number, delta: -1 | 1) {
    const r = regions[i];
    const to = i + delta;
    if (r === undefined || to < 0 || to >= regions.length) return;
    setRegions((l) => moveItem(l, i, delta));
    setMoved(
      t('fill.moved', {
        region: regionName(r),
        n: n.format(to + 1),
        total: n.format(regions.length),
      }),
    );
  }

  let body;
  if (step === 'scope') {
    body = (
      <ChoiceGroup title={t('fill.scope.title')}>
        {(labelId) => (
          <RadioGroup
            aria-labelledby={labelId}
            value={scope}
            onValueChange={(v) => setScope(v as FillScope)}
          >
            {SCOPES.map((s) => (
              <RadioRow
                key={s}
                id={`${uid}-scope-${s}`}
                value={s}
                disabled={counts[s] === 0}
                label={t(`fill.scope.${s}`, { n: n.format(counts[s]) })}
              />
            ))}
          </RadioGroup>
        )}
      </ChoiceGroup>
    );
  } else if (step === 'media') {
    body = (
      <fieldset className="m-0 grid min-w-0 gap-2.5 border-0 p-0">
        <legend className={groupTitle}>{t('fill.media.title')}</legend>
        {view.profile.media.map((m) => {
          const id = `${uid}-media-${m}`;
          return (
            <div key={m} className="flex items-center gap-2.5">
              <Checkbox
                id={id}
                name={`fill-media-${m}`}
                checked={media.includes(m)}
                onCheckedChange={() =>
                  setMedia((list) =>
                    list.includes(m)
                      ? list.filter((x) => x !== m)
                      : [...list, m],
                  )
                }
              />
              <Label htmlFor={id} className="font-normal">
                {t(`art.media.${m}`)}
              </Label>
            </div>
          );
        })}
      </fieldset>
    );
  } else if (step === 'strategy') {
    body = (
      <div className="grid gap-5">
        <ChoiceGroup title={t('fill.strategy.title')}>
          {(labelId) => (
            <RadioGroup
              aria-labelledby={labelId}
              value={strategy}
              onValueChange={(v) => setStrategy(v as StrategyKind)}
            >
              {STRATEGIES.map((s) => {
                const id = `${uid}-strategy-${s}`;
                const on = strategy === s;
                return (
                  <div
                    key={s}
                    className={cn(
                      'grid grid-cols-[auto_1fr] gap-x-2.5 gap-y-0.5 rounded-lg border px-3 py-2.5',
                      'transition-colors duration-fast ease-standard',
                      on
                        ? 'border-primary bg-cr-primary-subtle/40'
                        : 'border-cr-border bg-cr-surface',
                    )}
                  >
                    <RadioGroupItem
                      id={id}
                      value={s}
                      aria-describedby={`${id}-hint`}
                      className="mt-0.5"
                    />
                    <Label htmlFor={id}>{t(`fill.strategy.${s}`)}</Label>
                    <p
                      id={`${id}-hint`}
                      className="col-start-2 m-0 text-caption text-cr-text-secondary"
                    >
                      {t(`fill.strategyHint.${s}`)}
                    </p>
                  </div>
                );
              })}
            </RadioGroup>
          )}
        </ChoiceGroup>
        {strategy === 'popularity' && sources !== null && !sgdbVotes && (
          <Alert variant="warning" role="note">
            <AlertDescription>{t('fill.popularityNoKey')}</AlertDescription>
          </Alert>
        )}
        <section className="grid gap-2" aria-labelledby={`${uid}-regions`}>
          <h3 id={`${uid}-regions`} className={groupTitle}>
            {t('fill.regions')}
          </h3>
          <TooltipProvider>
            <ol
              className="m-0 grid list-none gap-1 p-0"
              aria-labelledby={`${uid}-regions`}
            >
              {regions.map((r, i) => (
                <li
                  key={r}
                  className="flex items-center gap-2 rounded-md border border-cr-border bg-cr-surface py-0.5 pl-2.5 pr-0.5"
                >
                  <span
                    aria-hidden
                    className="w-4 font-mono text-caption tabular-nums text-cr-text-secondary"
                  >
                    {i + 1}
                  </span>
                  <span className="min-w-0 flex-1 truncate text-ui">
                    {regionName(r)}
                  </span>
                  <IconAction
                    label={t('fill.moveUp', { region: regionName(r) })}
                    disabled={i === 0}
                    onClick={() => moveRegion(i, -1)}
                  >
                    <ChevronUp aria-hidden />
                  </IconAction>
                  <IconAction
                    label={t('fill.moveDown', { region: regionName(r) })}
                    disabled={i === regions.length - 1}
                    onClick={() => moveRegion(i, 1)}
                  >
                    <ChevronDown aria-hidden />
                  </IconAction>
                </li>
              ))}
            </ol>
          </TooltipProvider>
          <p role="status" className="sr-only">
            {moved}
          </p>
        </section>
      </div>
    );
  } else if (step === 'overwrite') {
    body = (
      <div className="grid gap-3">
        <ChoiceGroup title={t('fill.overwrite.title')}>
          {(labelId) => (
            <RadioGroup
              aria-labelledby={labelId}
              value={overwrite ?? ''}
              onValueChange={(v) => setOverwrite(v as OverwritePolicy)}
            >
              {OVERWRITES.map((o) => (
                <RadioRow
                  key={o}
                  id={`${uid}-overwrite-${o}`}
                  value={o}
                  label={t(`fill.overwrite.${o}`)}
                />
              ))}
            </RadioGroup>
          )}
        </ChoiceGroup>
        {defaults.overwrite === null && (
          <p className="m-0 text-caption text-cr-text-secondary">
            {t('fill.overwrite.askHint')}
          </p>
        )}
        <p className="m-0 text-caption text-cr-text-secondary">
          {t('fill.overwrite.backupHint')}
        </p>
      </div>
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
        <div className="grid gap-2">
          <p role="status" className="m-0">
            {t('fill.gathering', {
              done: n.format(p?.done ?? 0),
              total: n.format(p?.total ?? 0),
            })}
          </p>
          <Progress
            max={p?.total || 1}
            value={Math.min(p?.done ?? 0, p?.total || 1)}
            aria-label={t('fill.summary')}
          />
          {p?.label ? (
            <p className="hint mono truncate" title={p.label}>
              {p.label}
            </p>
          ) : null}
        </div>
      );
    }
    if (job.state === 'paused' && job.paused) {
      return (
        <Alert
          variant="warning"
          role="alert"
          action={
            <Button
              variant="primary"
              size="sm"
              disabled={pending}
              onClick={() => void resume()}
            >
              {t('jobs.resume')}
            </Button>
          }
        >
          <AlertTitle>{t(`jobs.pause.${job.paused.kind}`)}</AlertTitle>
          <AlertDescription>{t('fill.pausedHint')}</AlertDescription>
        </Alert>
      );
    }
    if (job.state === 'failed') {
      return (
        <Alert variant="danger">
          {t('fill.failed', { detail: job.error ?? '' })}
        </Alert>
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
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent
        size="lg"
        hideClose
        ref={contentRef}
        // Focus the dialog itself, as before: the first control may be a
        // choice the user has not read yet.
        onOpenAutoFocus={(ev) => {
          ev.preventDefault();
          contentRef.current?.focus();
        }}
        // A stray click on the scrim must not throw away the choices made.
        onPointerDownOutside={(ev) => ev.preventDefault()}
      >
        <DialogHeader>
          <DialogTitle>{t('fill.title')}</DialogTitle>
          <div className="flex items-center gap-2.5">
            <Progress
              slots={STEPS.length}
              value={index + 1}
              max={STEPS.length}
              aria-hidden
            />
            <DialogDescription>
              {t('fill.stepOf', { n: index + 1, total: STEPS.length })}
            </DialogDescription>
          </div>
        </DialogHeader>
        <DialogBody className="grid content-start gap-4">
          {view.profile.gamelist && (
            <p className="m-0 text-caption text-cr-text-secondary">
              {t('fill.gamelist')}
            </p>
          )}
          {body}
          {error && (
            <Alert variant="danger">
              {t(errorKey(error.code), { detail: error.detail ?? '' })}
            </Alert>
          )}
        </DialogBody>
        <DialogFooter>
          {step !== 'summary' && index > 0 && (
            <Button
              variant="outline"
              className="mr-auto"
              onClick={() => setStep(STEPS[index - 1] ?? 'scope')}
            >
              {t('fill.back')}
            </Button>
          )}
          {step === 'summary' && planned && (
            <Button
              variant="outline"
              className="mr-auto"
              onClick={() => void discard()}
            >
              {t('fill.discard')}
            </Button>
          )}
          <Button variant="ghost" onClick={onClose}>
            {t('fill.close')}
          </Button>
          {index < STEPS.indexOf('overwrite') && (
            <Button
              variant="primary"
              disabled={step === 'media' && media.length === 0}
              onClick={() => setStep(STEPS[index + 1] ?? 'summary')}
            >
              {t('fill.next')}
            </Button>
          )}
          {step === 'overwrite' && (
            <Button
              variant="primary"
              disabled={pending || media.length === 0 || overwrite === null}
              onClick={() => void preview()}
            >
              {t('fill.preview')}
            </Button>
          )}
          {planned && job.plan && job.plan.totals.review > 0 && (
            <Button variant="outline" onClick={() => setOverlay('review')}>
              {t('review.open', { n: n.format(job.plan.totals.review) })}
            </Button>
          )}
          {planned && job.plan && (
            <Button
              variant="primary"
              disabled={pending}
              onClick={() => void runJob()}
            >
              {t('fill.run', {
                n: n.format(job.plan.totals.auto + job.decided - job.skipped),
              })}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

const groupTitle = 'm-0 p-0 text-ui font-semibold text-cr-text';

/** A titled group of choices; the title names the group for screen readers. */
function ChoiceGroup({
  title,
  children,
}: {
  title: string;
  children: (labelId: string) => ReactNode;
}) {
  const labelId = useId();
  return (
    <div className="grid gap-2.5">
      <h3 id={labelId} className={groupTitle}>
        {title}
      </h3>
      {children(labelId)}
    </div>
  );
}

function RadioRow({
  id,
  value,
  label,
  disabled = false,
}: {
  id: string;
  value: string;
  label: string;
  disabled?: boolean;
}) {
  return (
    <div className="flex items-center gap-2.5">
      <RadioGroupItem id={id} value={value} disabled={disabled} />
      <Label htmlFor={id} className="font-normal">
        {label}
      </Label>
    </div>
  );
}

/** An icon-only button: the label is its name and its tooltip. */
function IconAction({
  label,
  disabled,
  onClick,
  children,
}: {
  label: string;
  disabled: boolean;
  onClick(): void;
  children: ReactNode;
}) {
  return (
    <SimpleTooltip content={label}>
      <Button
        variant="ghost"
        size="icon-sm"
        aria-label={label}
        disabled={disabled}
        onClick={onClick}
      >
        {children}
      </Button>
    </SimpleTooltip>
  );
}
