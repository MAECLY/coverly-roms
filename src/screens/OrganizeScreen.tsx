import {
  ArrowLeft,
  ChevronRight,
  FileJson,
  FolderOpen,
  FolderTree,
  RotateCw,
} from 'lucide-react';
import {
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type RefObject,
} from 'react';

import { DupGroup } from '@/components/dedupe/DupGroup';
import { ApplyProgress } from '@/components/organize/ApplyProgress';
import { LargeTransferDialog } from '@/components/organize/LargeTransferDialog';
import { LeftInPlace } from '@/components/organize/LeftInPlace';
import type { IsNew } from '@/components/organize/NewMarks';
import { OptionsBar } from '@/components/organize/OptionsBar';
import { OrganizeResult } from '@/components/organize/OrganizeResult';
import {
  ReviewBuckets,
  type OrganizeChoice,
} from '@/components/organize/ReviewBuckets';
import {
  chevron,
  groupTrigger,
  hintText,
  panel,
  sectionTitle,
} from '@/components/organize/styles';
import { SystemGroups } from '@/components/organize/SystemGroups';
import { SettingsLink } from '@/components/settings/SettingsLink';
import { Alert } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import { EmptyState } from '@/components/ui/empty-state';
import { Kbd, KbdGroup } from '@/components/ui/kbd';
import { Spinner } from '@/components/ui/spinner';
import { SimpleTooltip, TooltipProvider } from '@/components/ui/tooltip';
import { LocaleContext } from '@/i18n/core';
import { useT, type TFunction } from '@/i18n/useT';
import { getBackend } from '@/ipc/backend';
import { errorKey, toBackendError, type BackendError } from '@/ipc/errors';
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible';
import type {
  OrganizeDedupeMode,
  OrganizePlan,
  OrganizeRequest,
  ProfileInfo,
  SystemInfo,
} from '@/ipc/types';
import { formatDecimalSize } from '@/lib/format';
import { isTyping, modKeyLabel } from '@/lib/keyboard';
import {
  duplicateCounts,
  extractSummary,
  groupBySystem,
  itemKey,
  leftItems,
  organizeDupGroups,
  reviewBuckets,
  ROWS_PER_GROUP,
  updatesSummaryKey,
  type OrganizeDupGroup,
} from '@/lib/organize';
import { organizeStamp } from '@/lib/settingsStamp';
import { useNavStore } from '@/stores/navStore';
import { useOrganizeStore, type OrganizeRefresh } from '@/stores/organizeStore';
import { useSettingsStore } from '@/stores/settingsStore';

/** Profiles and systems for the pickers, loaded once per screen. */
function useCatalogues() {
  const [profiles, setProfiles] = useState<ProfileInfo[]>([]);
  const [systems, setSystems] = useState<SystemInfo[]>([]);
  /** `dedupe.organize_step`: the duplicates step a plan takes by default. */
  const [duplicates, setDuplicates] = useState<OrganizeDedupeMode>('set-aside');
  useEffect(() => {
    let alive = true;
    void getBackend().then((b) => {
      b.getSettings().then(
        (s) =>
          alive &&
          s?.settings?.dedupe &&
          setDuplicates(s.settings.dedupe.organize_step),
        () => undefined,
      );
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
  return { profiles, systems, duplicates };
}

/** Plan-wide warnings as text (data from the core, never prose). */
function warningLines(
  t: TFunction,
  locale: string,
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
      case 'no-space':
        lines.push(
          t('organize.noSpace', {
            needed: formatDecimalSize(w.needed, locale),
            free: formatDecimalSize(w.free, locale),
          }),
        );
        break;
      case 'large-transfer':
        lines.push(
          t('organize.largeTransfer.warning', {
            bytes: formatDecimalSize(w.bytes, locale),
          }),
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
  const stamp = useOrganizeStore((s) => s.settingsStamp);
  const pendingLarge = useOrganizeStore((s) => s.pendingLarge);
  const submitting = useOrganizeStore((s) => s.submitting);
  const settings = useSettingsStore((s) => s.settings);
  const { profiles, systems, duplicates: defaultDuplicates } = useCatalogues();
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

  const refreshButton = useRef<HTMLButtonElement>(null);
  /** Scans again. The button (and the controls) are disabled while the scan
   * runs, which drops keyboard focus to the page: it comes back to the
   * button, so the next Tab starts where the user was. */
  const refresh = useCallback(() => {
    setExported(null);
    void useOrganizeStore
      .getState()
      .refresh()
      .then(() => {
        const active = document.activeElement;
        if (!active || active === document.body) refreshButton.current?.focus();
      });
  }, []);

  // While the apply call answers (the backend re-plans, and may ask to
  // confirm the size), the proposal stays with its apply button busy.
  const applying = status === 'applying' && !submitting;
  const showResult = job !== null && !applying;
  // The progress block unmounts when the job ends, taking keyboard focus
  // (Cancel, the track list) with it: hand it to the result's heading.
  const wasApplying = useRef(applying);
  useEffect(() => {
    const ended = wasApplying.current && !applying;
    wasApplying.current = applying;
    if (!ended || !showResult) return;
    const active = document.activeElement;
    if (!active || active === document.body)
      document.getElementById('organize-result-title')?.focus();
  }, [applying, showResult]);
  const proposing =
    !showResult &&
    (submitting ||
      status === 'planning' ||
      status === 'ready' ||
      status === 'error');
  // The folder can be read again while a proposal (or its error) is shown.
  const canRefresh = proposing && request !== null;
  const planFailed = status === 'error' && job === null && error !== null;

  /** Stamps what the plan falls back to before Settings opens (settings
   * UI spec §8.6). The first stamp stays until a newer plan starts: a
   * later visit must not take a change the plan on screen never used. */
  const stampSettings = () => {
    const s = useSettingsStore.getState().settings;
    if (s && useOrganizeStore.getState().settingsStamp === null)
      useOrganizeStore.setState({ settingsStamp: organizeStamp(s) });
  };
  const settingsChanged =
    canRefresh &&
    status === 'ready' &&
    stamp !== null &&
    settings !== null &&
    organizeStamp(settings) !== stamp;

  // ⌘R / Ctrl+R scans again; the webview never reloads the page under it.
  useEffect(() => {
    if (!canRefresh) return;
    function onKey(ev: KeyboardEvent) {
      if (ev.key.toLowerCase() !== 'r' || !(ev.metaKey || ev.ctrlKey)) return;
      if (ev.altKey) return;
      ev.preventDefault();
      // A select is not text entry for a modified key: the system pickers
      // are selects, and ⌘R right after a choice must still scan.
      if (isTyping(ev.target) && !(ev.target instanceof HTMLSelectElement))
        return;
      refresh();
    }
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [canRefresh, refresh]);
  // Options and decisions wait for the plan in flight; an apply or an
  // export only takes the plan made with the options on screen.
  const busy = status === 'planning' || submitting;
  const applyButton = useRef<HTMLButtonElement>(null);
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
        <SettingsLink
          section="organize"
          label={t('organize.settingsLink')}
          disabled={applying}
          describedBy={
            settingsChanged ? 'organize-settings-changed' : undefined
          }
          onOpen={stampSettings}
        />
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
                    ref={refreshButton}
                    variant="outline"
                    size="sm"
                    className="ml-auto"
                    // Its own scan keeps it focusable (aria-disabled), so a
                    // keyboard user stays on it; a plan from an option
                    // change disables it outright.
                    disabled={status === 'planning' && !refreshing}
                    aria-disabled={refreshing || undefined}
                    aria-busy={refreshing || undefined}
                    aria-keyshortcuts="Meta+R Control+R"
                    onClick={refreshing ? undefined : refresh}
                  >
                    {refreshing ? (
                      <Spinner size="sm" decorative />
                    ) : (
                      <RotateCw aria-hidden />
                    )}
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
        {settingsChanged && (
          <Alert
            variant="info"
            action={
              <Button size="sm" variant="primary" onClick={refresh}>
                {t('organize.refresh.button')}
              </Button>
            }
          >
            <p id="organize-settings-changed" className="m-0">
              {t('organize.settingsChanged')}
            </p>
          </Alert>
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
          <ApplyProgress
            plan={plan}
            progress={progress}
            systemName={systemName}
            onCancel={() => useOrganizeStore.getState().cancel()}
          />
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
            duplicates={request?.duplicates ?? defaultDuplicates}
            locked={locked}
            profiles={profiles}
            systems={systems}
            busy={busy}
            stale={stale}
            submitting={submitting}
            applyRef={applyButton}
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
            onRescan={refresh}
            onCancel={back}
          />
        )}
        {pendingLarge && (
          <LargeTransferDialog
            detail={pendingLarge}
            returnFocus={applyButton}
            onCancel={() => useOrganizeStore.getState().dismissLarge()}
            onConfirm={(bytes) =>
              void useOrganizeStore.getState().confirmApply(bytes)
            }
          />
        )}
      </div>
    </main>
  );
}

interface ProposalProps {
  plan: OrganizePlan;
  /** The duplicates step the plan was made with. */
  duplicates: OrganizeDedupeMode;
  /** Layout and target belong to the library being tidied. */
  locked: boolean;
  profiles: ProfileInfo[];
  systems: SystemInfo[];
  busy: boolean;
  /** The plan on screen is not the answer to the options on screen. */
  stale: boolean;
  /** Its apply call has not answered yet. */
  submitting: boolean;
  /** The apply button (focus comes back to it from the size dialog). */
  applyRef: RefObject<HTMLButtonElement | null>;
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
  /** Reads the folder again (the ⌘R of the screen). */
  onRescan(): void;
  onCancel(): void;
}

function Proposal({
  plan,
  duplicates,
  locked,
  profiles,
  systems,
  busy,
  stale,
  submitting,
  applyRef,
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
  onRescan,
  onCancel,
}: ProposalProps) {
  const t = useT();
  const locale = useContext(LocaleContext);
  const groups = useMemo(() => groupBySystem(plan), [plan]);
  const buckets = useMemo(() => reviewBuckets(plan), [plan]);
  const left = useMemo(() => leftItems(plan), [plan]);
  const extracting = useMemo(() => extractSummary(plan), [plan]);
  const dupGroups = useMemo(() => organizeDupGroups(plan), [plan]);
  const dupCounts = useMemo(() => duplicateCounts(plan), [plan]);
  const updatesLine = updatesSummaryKey(plan);
  const layoutProfile =
    profiles.find((p) => p.profile.id === plan.layout)?.profile ?? null;
  const { counts } = plan;
  const warnings = warningLines(t, locale, plan, systemName, profileName);
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
        duplicates={duplicates}
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
        {updatesLine && (
          <p className="m-0 text-dense tabular-nums text-cr-text">
            {t(updatesLine.key, updatesLine.vars)}
          </p>
        )}
        {(dupCounts.extras > 0 || dupCounts.inLibrary > 0) && (
          <p className="m-0 text-dense tabular-nums text-cr-text">
            {[
              t('organize.dedupe.count', { count: dupCounts.extras }),
              ...(dupCounts.inLibrary > 0
                ? [
                    t('organize.dedupe.inLibrary', {
                      count: dupCounts.inLibrary,
                    }),
                  ]
                : []),
            ].join(' · ')}
          </p>
        )}
        {extracting && (
          <p className="m-0 text-dense tabular-nums text-cr-text">
            {t(
              extracting.count === 1
                ? 'organize.extract.summaryOne'
                : 'organize.extract.summary',
              {
                count: extracting.count,
                bytes: formatDecimalSize(extracting.bytes, locale),
              },
            )}
          </p>
        )}
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
          mode={plan.mode}
          profile={layoutProfile}
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
      {dupGroups.length > 0 && (
        <DuplicatesSection
          groups={dupGroups}
          trash={plan.set_aside_trash}
          profileName={profileName(plan.layout)}
        />
      )}
      <LeftInPlace
        items={left}
        systems={systems}
        otherFiles={counts.other_files}
        deepFolders={counts.deep_folders}
        disabled={busy}
        onDecide={onDecide}
        onRescan={onRescan}
        {...(isNew ? { isNew } : {})}
      />
      {/* The action bar sits on the page, ruled off like a shelf edge
          (brand §2), and lines up with the panels above it. */}
      <div className="sticky bottom-0 z-[1] flex flex-wrap items-center gap-2 border-t border-cr-border bg-background py-3">
        {counts.auto > 0 || counts.set_aside > 0 ? (
          <Button
            ref={applyRef}
            variant="primary"
            // Busy, not disabled, while its call answers: keyboard focus
            // stays on it for the size dialog to give back.
            disabled={stale && !submitting}
            aria-disabled={submitting || undefined}
            aria-busy={submitting || undefined}
            onClick={submitting ? undefined : onApply}
          >
            {submitting ? (
              <Spinner size="sm" decorative />
            ) : (
              <FolderTree aria-hidden />
            )}
            {applyLabel(t, counts.auto, counts.set_aside, plan.set_aside_trash)}
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

/** The primary button: the files that move, and apart the copies set
 * aside (spec §11.2: "Organizar 16 archivos y apartar 3"). */
function applyLabel(
  t: TFunction,
  count: number,
  extras: number,
  trash: boolean,
): string {
  if (extras === 0)
    return t(count === 1 ? 'organize.applyOne' : 'organize.apply', { count });
  if (count === 0)
    return t(
      trash
        ? extras === 1
          ? 'dedupe.applyTrashOne'
          : 'dedupe.apply.trash'
        : extras === 1
          ? 'dedupe.applyOne'
          : 'dedupe.apply.quarantine',
      { count: extras },
    );
  const key = trash
    ? count === 1
      ? 'organize.applyOneWithTrash'
      : 'organize.applyWithTrash'
    : count === 1
      ? 'organize.applyOneWithDedupe'
      : 'organize.applyWithDedupe';
  return t(key, { count, extras });
}

/** Whole groups up to `rows` rows (at least one), then "Show more". */
function groupsWithin(
  groups: OrganizeDupGroup[],
  rows: number,
): OrganizeDupGroup[] {
  const out: OrganizeDupGroup[] = [];
  let used = 0;
  for (const g of groups) {
    if (out.length > 0 && used + g.group.members.length > rows) break;
    out.push(g);
    used += g.group.members.length;
  }
  return out;
}

/** The proposal's duplicates (spec §11.2): the shelf and tray of the
 * library screen, read-only (the picker above settles them), closed
 * above `ROWS_PER_GROUP` rows like the system groups. */
function DuplicatesSection({
  groups,
  trash,
  profileName,
}: {
  groups: OrganizeDupGroup[];
  trash: boolean;
  profileName: string;
}) {
  const t = useT();
  const rows = groups.reduce((n, g) => n + g.group.members.length, 0);
  const [open, setOpen] = useState(rows <= ROWS_PER_GROUP);
  const [visible, setVisible] = useState(ROWS_PER_GROUP);
  const setAside = groups.find((g) => g.setAside);
  const hint = !setAside
    ? t('organize.dedupe.reportHint')
    : trash
      ? t('organize.dedupe.hintTrash')
      : t('organize.dedupe.hint', {
          folder: setAside.quarantine.split(/[\\/]/).pop() ?? '',
        });
  const shown = groupsWithin(groups, visible);
  return (
    // The relation badges carry tooltips.
    <TooltipProvider>
      <section aria-labelledby="organize-dedupe-title" className="grid gap-2">
        <h2 id="organize-dedupe-title" className={sectionTitle}>
          {t('organize.dedupe.picker')}
        </h2>
        <p className={hintText}>{hint}</p>
        <Collapsible open={open} onOpenChange={setOpen} className={panel}>
          <CollapsibleTrigger asChild>
            <Button variant="ghost" className={groupTrigger}>
              <ChevronRight aria-hidden className={chevron} />
              <span className="tabular-nums">
                {t('organize.dedupe.games', { count: groups.length })}
              </span>
            </Button>
          </CollapsibleTrigger>
          <CollapsibleContent className="dup-system__body border-t border-cr-border">
            {shown.map(({ group, quarantine, setAside: leaves }) => (
              <DupGroup
                key={group.id}
                group={group}
                quarantine={quarantine}
                profileName={profileName}
                pickable={false}
                reportOnly={!leaves}
                onReveal={(p) => void useOrganizeStore.getState().reveal(p)}
              />
            ))}
            {shown.length < groups.length && (
              <Button
                variant="link"
                size="sm"
                className="m-2"
                onClick={() => setVisible((v) => v + ROWS_PER_GROUP)}
              >
                {t('organize.showMore', {
                  count: groups.length - shown.length,
                })}
              </Button>
            )}
          </CollapsibleContent>
        </Collapsible>
      </section>
    </TooltipProvider>
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
