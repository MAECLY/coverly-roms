import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { LeftInPlace } from '@/components/organize/LeftInPlace';
import { OptionsBar } from '@/components/organize/OptionsBar';
import { OrganizeResult } from '@/components/organize/OrganizeResult';
import {
  ReviewBuckets,
  type OrganizeChoice,
} from '@/components/organize/ReviewBuckets';
import { SystemGroups } from '@/components/organize/SystemGroups';
import { useT, type TFunction } from '@/i18n/useT';
import { getBackend } from '@/ipc/backend';
import { errorKey, toBackendError, type BackendError } from '@/ipc/errors';
import type {
  OrganizePlan,
  OrganizeRequest,
  ProfileInfo,
  SystemInfo,
} from '@/ipc/types';
import { groupBySystem, leftItems, reviewBuckets } from '@/lib/organize';
import { useNavStore } from '@/stores/navStore';
import { useOrganizeStore } from '@/stores/organizeStore';

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
  const { profiles, systems } = useCatalogues();
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

  const applying = status === 'applying';
  const showResult = job !== null && !applying;
  const proposing =
    !showResult &&
    (status === 'planning' || status === 'ready' || status === 'error');
  // Options and decisions wait for the plan in flight; an apply or an
  // export only takes the plan made with the options on screen.
  const busy = status === 'planning';
  const stale = status !== 'ready';
  const shown = error ?? exportError;

  return (
    <main className="settings-screen">
      <header className="settings-header">
        <button
          type="button"
          className="ghost-button"
          disabled={status === 'applying'}
          onClick={back}
        >
          {t('organize.back')}
        </button>
        <h1>{t('organize.title')}</h1>
      </header>
      <div className="flex flex-col gap-4 px-6 py-6">
        {source && <p className="hint">{source}</p>}
        {status === 'planning' && (
          <div className="flex flex-wrap items-center gap-3">
            <p role="status">
              {progress && progress.total > 0
                ? t('organize.planning', {
                    done: progress.done,
                    total: progress.total,
                  })
                : t('organize.walking')}
            </p>
            <button
              type="button"
              className="ghost-button"
              onClick={() => void useOrganizeStore.getState().cancel()}
            >
              {t('organize.cancel')}
            </button>
          </div>
        )}
        {shown && !showResult && !applying && (
          <p role="alert" className="error-banner">
            {t(errorKey(shown.code), { detail: shown.detail ?? '' })}
          </p>
        )}
        {applying && (
          <div className="flex flex-wrap items-center gap-3">
            <progress
              max={progress?.total || 1}
              value={progress?.done ?? 0}
              aria-label={t('organize.progressLabel')}
            />
            <p role="status">
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
            <button
              type="button"
              className="ghost-button"
              onClick={() => void useOrganizeStore.getState().cancel()}
            >
              {t('organize.cancel')}
            </button>
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
      {detected && (
        <p className="hint">
          {t('organize.target.detected', { profile: detected })}
        </p>
      )}
      {warningLines(t, plan, systemName, profileName).map((line) => (
        <p key={line} className="hint">
          {line}
        </p>
      ))}
      <p>
        {t('organize.summary', {
          roms: counts.roms,
          systems: counts.systems,
          review: counts.review,
          left: counts.left,
        })}
      </p>
      <p className="hint">{t('organize.dryRunHint')}</p>
      <SystemGroups
        groups={groups}
        layoutName={profileName(plan.layout)}
        systemName={systemName}
      />
      {buckets.length > 0 && (
        <ReviewBuckets
          buckets={buckets}
          systemName={systemName}
          disabled={busy}
          onDecide={onDecide}
        />
      )}
      <LeftInPlace
        items={left}
        systems={systems}
        otherFiles={counts.other_files}
        deepFolders={counts.deep_folders}
        disabled={busy}
        onDecide={onDecide}
      />
      <div className="flex flex-wrap items-center gap-3">
        {counts.auto > 0 ? (
          <button
            type="button"
            className="primary-button"
            disabled={stale}
            onClick={onApply}
          >
            {t('organize.apply', { count: counts.auto })}
          </button>
        ) : (
          // Sorted folders live in the source: "open" opens the target,
          // so offer it only when that is where they are, or when the
          // target was detected as a library of its own.
          ((plan.sorted_folders.length > 0 &&
            plan.target_root === plan.source_root) ||
            detected !== null) && (
            <button
              type="button"
              className="primary-button"
              disabled={stale}
              onClick={onOpenLibrary}
            >
              {t('organize.openLibrary')}
            </button>
          )
        )}
        <button
          type="button"
          className="ghost-button"
          disabled={stale}
          onClick={onExport}
        >
          {t('organize.exportPlan')}
        </button>
        <button type="button" className="ghost-button" onClick={onCancel}>
          {t('organize.cancel')}
        </button>
      </div>
      {exported && (
        <p className="hint">{t('organize.exported', { name: exported })}</p>
      )}
    </>
  );
}
