import {
  ArrowLeft,
  FileJson,
  FolderOpen,
  FolderTree,
  RotateCw,
} from 'lucide-react';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { LeftInPlace } from '@/components/organize/LeftInPlace';
import type { IsNew } from '@/components/organize/NewMarks';
import { OptionsBar } from '@/components/organize/OptionsBar';
import { OrganizeResult } from '@/components/organize/OrganizeResult';
import {
  ReviewBuckets,
  type OrganizeChoice,
} from '@/components/organize/ReviewBuckets';
import { hintText } from '@/components/organize/styles';
import { SystemGroups } from '@/components/organize/SystemGroups';
import { Alert } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import { EmptyState } from '@/components/ui/empty-state';
import { Kbd, KbdGroup } from '@/components/ui/kbd';
import { Progress } from '@/components/ui/progress';
import { Spinner } from '@/components/ui/spinner';
import { SimpleTooltip, TooltipProvider } from '@/components/ui/tooltip';
import { useT, type TFunction } from '@/i18n/useT';
import { getBackend } from '@/ipc/backend';
import { errorKey, toBackendError, type BackendError } from '@/ipc/errors';
import type {
  OrganizePlan,
  OrganizeRequest,
  ProfileInfo,
  SystemInfo,
} from '@/ipc/types';
import { isTyping, modKeyLabel } from '@/lib/keyboard';
import {
  groupBySystem,
  itemKey,
  leftItems,
  reviewBuckets,
} from '@/lib/organize';
import { useNavStore } from '@/stores/navStore';
import { useOrganizeStore, type OrganizeRefresh } from '@/stores/organizeStore';

/** Slots of the apply progress (same row as the library scan). */
const SLOTS = 16;

/** Profiles and systems for the pickers, loaded once per screen. */
function useCatalogues() {
  const [profiles, setProfiles] = useState<ProfileInfo[]>([]);
  const [systems, setSystems] = useState<SystemInfo[]>([]);
  useEffect(() => {
    let alive = true;
    void getBackend().then((b) => {
      b.listProfiles().then(
        (p) => alive && setProfiles(Array.isArray(p) ? p : []),
        () => undefined,
      );
      b.listSystems().then(
        (s) => alive && setSystems(Array.isArray(s) ? s : []),
        () => undefined,
      );
    });
    return () => {
      alive = false;
    };
  }, []);
  return { profiles, systems };
}

/** Plan-wide warnings as text (data from the core, never prose). */
function warningLines(
  t: TFunction,
  plan: OrganizePlan,
  systemName: (id: string) => string,
  profileName: (id: string) => string,
): string[] {
  const lines: string[] = [];
  for (const w of plan.warnings) {
    switch (w.kind) {
      case 'catalogue-missing': {
        const levels = [
          ...new Set(
            plan.items.flatMap((i) =>
              i.action !== 'left' && i.system_id === w.system && i.method
                ? [i.method]
                : [],
            ),
          ),
        ];
        lines.push(
          t('organize.noCatalogue', {
            system: systemName(w.system),
            level: (levels.length > 0 ? levels : ['extension' as const])
              .map((l) => t(`organize.level.${l}`))
              .join(', '),
          }),
        );
        break;
      }
      case 'target-layout-mismatch':
        lines.push(
          t('organize.target.mismatch', { profile: profileName(w.profile) }),
        );
        break;
      case 'move-unavailable':
      case 'sorted-only':
        break;
    }
  }
  if (!plan.move_allowed) lines.push(t('organize.mode.moveDisabled'));
  return lines;
}

/** The organise proposal for a loose pile: the dry run (organise spec §4.2). */
export function OrganizeScreen() {
  const t = useT();
  const source = useNavStore((s) => s.organizeSource);
  const status = useOrganizeStore((s) => s.status);
  const plan = useOrganizeStore((s) => s.plan);
  const planId = useOrganizeStore((s) => s.planId);
  const request = useOrganizeStore((s) => s.request);
  const progress = useOrganizeStore((s) => s.progress);
  const error = useOrganizeStore((s) => s.error);
  const job = useOrganizeStore((s) => s.job);
  const undo = useOrganizeStore((s) => s.undo);
  const locked = useOrganizeStore((s) => s.locked);
  const lastRefresh = useOrganizeStore((s) => s.lastRefresh);
  const refreshing = useOrganizeStore((s) => s.refreshing);
  const { profiles, systems } = useCatalogues();
  /** The change summary the user closed (the "New" marks stay). */
  const [dismissed, setDismissed] = useState<OrganizeRefresh | null>(null);
  const [detected, setDetected] = useState<string | null>(null);
  const [exported, setExported] = useState<string | null>(null);
  const [exportError, setExportError] = useState<BackendError | null>(null);
  /** The next plan may switch to the layout its target already has. */
  const followTarget = useRef(false);

  // The launch options come with the source and are read once with it.
  useEffect(() => {
    if (source && useOrganizeStore.getState().source !== source)
      void useOrganizeStore
        .getState()
        .start(source, useNavStore.getState().organizeLaunch ?? undefined);
  }, [source]);

  // A target that already is a library brings its layout (spec §4.2.1),
  // unless the user picked a layout on purpose for it.
  useEffect(() => {
    if (status !== 'ready' || !plan) return;
    const follow = followTarget.current || request?.layout == null;
    followTarget.current = false;
    const found = plan.target_detected;
    if (follow && found && found !== plan.layout) {
      setDetected(found);
      void useOrganizeStore.getState().setOption({ layout: found });
    }
  }, [status, plan, request]);

  const names = useMemo(
    () => new Map(systems.map((s) => [s.id, s.name])),
    [systems],
  );
  const systemName = useCallback((id: string) => names.get(id) ?? id, [names]);
  const profileName = useCallback(
    (id: string) =>
      profiles.find((p) => p.profile.id === id)?.profile.name ?? id,
    [profiles],
  );

  const back = () => {
    const s = useOrganizeStore.getState();
    // Loose files of an open library: it rescans with what changed.
    if (s.locked && s.job) {
      void s.openResult();
      return;
    }
    s.reset();
    useNavStore.getState().closeOrganize();
  };
  const setOption = (patch: Partial<OrganizeRequest>) => {
    setExported(null);
    void useOrganizeStore.getState().setOption(patch);
  };
  const pickTarget = async () => {
    const path = await (await getBackend()).pickFolder();
    if (!path) return;
    followTarget.current = true;
    setDetected(null);
    setOption({ target: path });
  };
  const decide = (ids: number[], choice: OrganizeChoice) => {
    setExported(null);
    void useOrganizeStore.getState().decide(ids, choice);
  };
  const exportPlan = async () => {
    if (planId === null) return;
    setExportError(null);
    try {
      setExported(await (await getBackend()).organizeExportPlan(planId));
    } catch (e) {
      setExportError(toBackendError(e));
    }
  };

  const refresh = () => {
    setExported(null);
    void useOrganizeStore.getState().refresh();
  };

  const applying = status === 'applying';
  const showResult = job !== null && !applying;
  const proposing =
    !showResult &&
    (status === 'planning' || status === 'ready' || status === 'error');
  // The folder can be read again while a proposal (or its error) is shown.
  const canRefresh = proposing && request !== null;
  const planFailed = status === 'error' && job === null && error !== null;

  // ⌘R / Ctrl+R scans again; the webview never reloads the page under it.
  useEffect(() => {
    if (!canRefresh) return;
    function onKey(ev: KeyboardEvent) {
      if (ev.key.toLowerCase() !== 'r' || !(ev.metaKey || ev.ctrlKey)) return;
      if (ev.altKey) return;
      ev.preventDefault();
      if (isTyping(ev.target)) return;
      setExported(null);
      void useOrganizeStore.getState().refresh();
    }
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [canRefresh]);
  // Options and decisions wait for the plan in flight; an apply or an
  // export only takes the plan made with the options on screen.
  const busy = status === 'planning';
  const stale = status !== 'ready';
  const shown = error ?? exportError;

  return (
    <main className="settings-screen">
      <header className="settings-header">
        <Button disabled={status === 'applying'} onClick={back}>
          <ArrowLeft aria-hidden />
          {t('organize.back')}
        </Button>
        <h1>{t('organize.title')}</h1>
      </header>
      <div className="mx-auto flex w-full max-w-5xl flex-col gap-4 px-6 py-6">
        {source && (
          // The folder, and the one action that reads it again.
          <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
            <p
              className="m-0 min-w-0 flex-1 truncate font-mono text-caption text-cr-text-secondary"
              title={source}
            >
              {source}
            </p>
            {canRefresh && (
              <TooltipProvider>
                <SimpleTooltip
                  side="bottom"
                  content={
                    <span className="flex items-center gap-2">
                      {t('organize.refresh.tooltip')}
                      <KbdGroup aria-hidden>
                        <Kbd>{modKeyLabel()}</Kbd>
                        <Kbd>R</Kbd>
                      </KbdGroup>
                    </span>
                  }
                >
                  <Button
                    variant="outline"
                    size="sm"
                    className="ml-auto"
                    loading={refreshing}
                    disabled={status === 'planning'}
                    aria-keyshortcuts="Meta+R Control+R"
                    onClick={refresh}
                  >
                    {!refreshing && <RotateCw aria-hidden />}
                    {t('organize.refresh.button')}
                  </Button>
                </SimpleTooltip>
              </TooltipProvider>
            )}
          </div>
        )}
        {status === 'planning' && (
          <div className="flex flex-wrap items-center gap-3">
            <Spinner size="sm" decorative />
            <p role="status" className="m-0 text-ui tabular-nums">
              {progress && progress.total > 0
                ? t('organize.planning', {
                    done: progress.done,
                    total: progress.total,
                  })
                : t('organize.walking')}
            </p>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => void useOrganizeStore.getState().cancel()}
            >
              {t('organize.cancel')}
            </Button>
          </div>
        )}
        {shown && !showResult && !applying && (
          <Alert
            variant="danger"
            action={
              planFailed && (
                <Button variant="outline" size="sm" onClick={refresh}>
                  {t('organize.refresh.retry')}
                </Button>
              )
            }
          >
            <p className="m-0">
              {t(errorKey(shown.code), { detail: shown.detail ?? '' })}
            </p>
            {/* A plan never touches a file (brand §7: say what was not). */}
            {planFailed && (
              <p className="m-0">{t('organize.refresh.nothingMoved')}</p>
            )}
          </Alert>
        )}
        {applying && (
          <div className="grid justify-items-start gap-2.5">
            {/* A row of slots filling left to right (brand §2). */}
            <Progress
              slots={SLOTS}
              max={progress?.total || 1}
              value={progress ? Math.min(progress.done, progress.total) : null}
              aria-label={t('organize.progressLabel')}
            />
            <div className="flex flex-wrap items-center gap-3">
              <p role="status" className="m-0 text-ui tabular-nums">
                {progress
                  ? t(
                      !plan
                        ? 'organize.organizing'
                        : plan.mode === 'copy'
                          ? 'organize.copying'
                          : 'organize.applying',
                      { done: progress.done, total: progress.total },
                    )
                  : t('organize.starting')}
              </p>
              <Button
                variant="ghost"
                size="sm"
                onClick={() => void useOrganizeStore.getState().cancel()}
              >
                {t('organize.cancel')}
              </Button>
            </div>
          </div>
        )}
        {showResult && (
          <OrganizeResult
            job={job}
            plan={plan}
            undo={undo}
            error={error}
            onOpen={() => void useOrganizeStore.getState().openResult()}
            onUndo={(all) => void useOrganizeStore.getState().undoJob(all)}
            onResume={() => void useOrganizeStore.getState().resume()}
          />
        )}
        {proposing && plan && (
          <Proposal
            plan={plan}
            locked={locked}
            profiles={profiles}
            systems={systems}
            busy={busy}
            stale={stale}
            detected={
              detected === plan.layout && plan.target_root !== plan.source_root
                ? profileName(detected)
                : null
            }
            exported={exported}
            lastRefresh={lastRefresh}
            summaryShown={lastRefresh !== null && dismissed !== lastRefresh}
            onDismissSummary={() => setDismissed(lastRefresh)}
            systemName={systemName}
            profileName={profileName}
            onOption={setOption}
            onPickTarget={() => void pickTarget()}
            onDecide={decide}
            onApply={() => void useOrganizeStore.getState().apply()}
            onOpenLibrary={() => void useOrganizeStore.getState().openResult()}
            onExport={() => void exportPlan()}
            onCancel={back}
          />
        )}
      </div>
    </main>
  );
}

interface ProposalProps {
  plan: OrganizePlan;
  /** Layout and target belong to the library being tidied. */
  locked: boolean;
  profiles: ProfileInfo[];
  systems: SystemInfo[];
  busy: boolean;
  /** The plan on screen is not the answer to the options on screen. */
  stale: boolean;
  /** Name of the layout taken from the target, when that happened. */
  detected: string | null;
  exported: string | null;
  /** What the last "scan again" found (null: no refresh since the last edit). */
  lastRefresh: OrganizeRefresh | null;
  summaryShown: boolean;
  onDismissSummary(): void;
  systemName(id: string): string;
  profileName(id: string): string;
  onOption(patch: Partial<OrganizeRequest>): void;
  onPickTarget(): void;
  onDecide(ids: number[], choice: OrganizeChoice): void;
  onApply(): void;
  onOpenLibrary(): void;
  onExport(): void;
  onCancel(): void;
}

function Proposal({
  plan,
  locked,
  profiles,
  systems,
  busy,
  stale,
  detected,
  exported,
  lastRefresh,
  summaryShown,
  onDismissSummary,
  systemName,
  profileName,
  onOption,
  onPickTarget,
  onDecide,
  onApply,
  onOpenLibrary,
  onExport,
  onCancel,
}: ProposalProps) {
  const t = useT();
  const groups = useMemo(() => groupBySystem(plan), [plan]);
  const buckets = useMemo(() => reviewBuckets(plan), [plan]);
  const left = useMemo(() => leftItems(plan), [plan]);
  const { counts } = plan;
  const warnings = warningLines(t, plan, systemName, profileName);
  const nothingToFile = groups.length === 0 && buckets.length === 0;
  const isNew = useMemo<IsNew | undefined>(() => {
    if (!lastRefresh || lastRefresh.added.length === 0) return undefined;
    const added = new Set(lastRefresh.added);
    return (item) => added.has(itemKey(item));
  }, [lastRefresh]);
  return (
    <>
      <OptionsBar
        plan={plan}
        profiles={profiles}
        disabled={busy}
        locked={locked}
        onChange={onOption}
        onPickTarget={onPickTarget}
      />
      {lastRefresh && summaryShown && (
        <RefreshSummary refresh={lastRefresh} onDismiss={onDismissSummary} />
      )}
      {detected && (
        <Alert variant="info">
          <p className="m-0">
            {t('organize.target.detected', { profile: detected })}
          </p>
        </Alert>
      )}
      {warnings.length > 0 && (
        <Alert variant="warning">
          {warnings.map((line) => (
            <p key={line} className="m-0">
              {line}
            </p>
          ))}
        </Alert>
      )}
      <div className="grid gap-1">
        <p className="m-0 text-dense font-medium tabular-nums text-cr-text">
          {t('organize.summary', {
            roms: counts.roms,
            systems: counts.systems,
            review: counts.review,
            left: counts.left,
          })}
        </p>
        <p className={hintText}>{t('organize.dryRunHint')}</p>
      </div>
      {nothingToFile ? (
        <EmptyState
          size="sm"
          titleAs="p"
          title={t('organize.empty.title')}
          description={t('organize.empty.body')}
          className="rounded-lg border border-dashed border-cr-border-strong"
        />
      ) : (
        <SystemGroups
          groups={groups}
          layoutName={profileName(plan.layout)}
          systemName={systemName}
          systems={systems}
          disabled={busy}
          onDecide={onDecide}
          {...(isNew ? { isNew } : {})}
        />
      )}
      {buckets.length > 0 && (
        <ReviewBuckets
          buckets={buckets}
          systemName={systemName}
          disabled={busy}
          onDecide={onDecide}
          {...(isNew ? { isNew } : {})}
        />
      )}
      <LeftInPlace
        items={left}
        systems={systems}
        otherFiles={counts.other_files}
        deepFolders={counts.deep_folders}
        disabled={busy}
        onDecide={onDecide}
        {...(isNew ? { isNew } : {})}
      />
      {/* The action bar sits on the page, ruled off like a shelf edge
          (brand §2), and lines up with the panels above it. */}
      <div className="sticky bottom-0 z-[1] flex flex-wrap items-center gap-2 border-t border-cr-border bg-background py-3">
        {counts.auto > 0 ? (
          <Button variant="primary" disabled={stale} onClick={onApply}>
            <FolderTree aria-hidden />
            {t('organize.apply', { count: counts.auto })}
          </Button>
        ) : (
          // Sorted folders live in the source: "open" opens the target,
          // so offer it only when that is where they are, or when the
          // target was detected as a library of its own.
          ((plan.sorted_folders.length > 0 &&
            plan.target_root === plan.source_root) ||
            detected !== null) && (
            <Button variant="primary" disabled={stale} onClick={onOpenLibrary}>
              <FolderOpen aria-hidden />
              {t('organize.openLibrary')}
            </Button>
          )
        )}
        <Button variant="outline" disabled={stale} onClick={onExport}>
          <FileJson aria-hidden />
          {t('organize.exportPlan')}
        </Button>
        <Button variant="ghost" onClick={onCancel}>
          {t('organize.cancel')}
        </Button>
        {exported && (
          <p role="status" className={`${hintText} basis-full`}>
            {t('organize.exported', { name: exported })}
          </p>
        )}
      </div>
    </>
  );
}

/** What a "scan again" found, in counts (brand §7: exact numbers). */
function RefreshSummary({
  refresh,
  onDismiss,
}: {
  refresh: OrganizeRefresh;
  onDismiss(): void;
}) {
  const t = useT();
  const changed = refresh.added.length > 0 || refresh.gone > 0;
  return (
    <Alert variant="info" onDismiss={onDismiss}>
      <p className="m-0 tabular-nums">
        {changed
          ? t('organize.refresh.changed', {
              added: refresh.added.length,
              gone: refresh.gone,
            })
          : t('organize.refresh.same')}
      </p>
      {refresh.kept > 0 && (
        <p className="m-0 tabular-nums text-cr-text-secondary">
          {t('organize.refresh.kept', { count: refresh.kept })}
        </p>
      )}
      {refresh.dropped > 0 && (
        <p className="m-0 tabular-nums text-cr-text-secondary">
          {t('organize.refresh.dropped', { count: refresh.dropped })}
        </p>
      )}
    </Alert>
  );
}
