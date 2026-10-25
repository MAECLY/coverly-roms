/**
 * Duplicates of the open library (duplicates spec §11.3): a shelf and a
 * tray per game. Planning shows its steps in order; the proposal shows the
 * summary, the options, the conflicts, one panel per system (the copy that
 * stays on the shelf, the copies that leave in the tray) and what stays
 * unchanged, under a sticky bar with the verb and the exact count.
 * Applying shows the slot row; the end shows what happened, with undo.
 */
import {
  ArrowLeft,
  Check,
  FileJson,
  FolderInput,
  RotateCw,
  Trash2,
} from 'lucide-react';
import { useCallback, useContext, useEffect, useMemo, useState } from 'react';

import { DedupeOptions } from '@/components/dedupe/DedupeOptions';
import { DedupeResult } from '@/components/dedupe/DedupeResult';
import { SystemSection } from '@/components/dedupe/SystemSection';
import { UnchangedList } from '@/components/dedupe/UnchangedList';
import { Alert } from '@/components/ui/alert';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';
import { Button } from '@/components/ui/button';
import { EmptyState } from '@/components/ui/empty-state';
import { Kbd, KbdGroup } from '@/components/ui/kbd';
import { Progress } from '@/components/ui/progress';
import { Spinner } from '@/components/ui/spinner';
import { SimpleTooltip, TooltipProvider } from '@/components/ui/tooltip';
import { LocaleContext } from '@/i18n/core';
import { useT, type TFunction } from '@/i18n/useT';
import { getBackend } from '@/ipc/backend';
import { errorKey } from '@/ipc/errors';
import {
  DEDUPE_PHASES,
  type DedupePhaseView,
  type DedupePlanView,
  type DupGroupView,
  type ProfileInfo,
  type SystemInfo,
} from '@/ipc/types';
import { fileName, formatBytes, sortGroups } from '@/lib/dedupe';
import { isTyping, modKeyLabel } from '@/lib/keyboard';
import {
  applyConfirm,
  useDedupeStore,
  type ApplyConfirm,
  type DedupeLaunch,
} from '@/stores/dedupeStore';
import { useLibraryStore } from '@/stores/libraryStore';
import { useNavStore } from '@/stores/navStore';

import '@/components/dedupe/dedupe.css';

/** Slots of the apply progress (same row as the library scan). */
const SLOTS = 16;

const PHASE_KEYS: Record<DedupePhaseView, string> = {
  listing: 'dedupe.phase.list',
  sizes: 'dedupe.phase.sizes',
  fingerprints: 'dedupe.phase.hash',
  serials: 'dedupe.phase.serials',
  catalogues: 'dedupe.phase.catalogue',
};

/** Systems and profiles by name, loaded once per screen. */
function useNames() {
  const [systems, setSystems] = useState<SystemInfo[]>([]);
  const [profiles, setProfiles] = useState<ProfileInfo[]>([]);
  useEffect(() => {
    let alive = true;
    void getBackend().then((b) => {
      b.listSystems().then(
        (s) => alive && setSystems(Array.isArray(s) ? s : []),
        () => undefined,
      );
      b.listProfiles().then(
        (p) => alive && setProfiles(Array.isArray(p) ? p : []),
        () => undefined,
      );
    });
    return () => {
      alive = false;
    };
  }, []);
  const names = useMemo(
    () => new Map(systems.map((s) => [s.id.toLowerCase(), s.name])),
    [systems],
  );
  const systemName = useCallback(
    (id: string) => names.get(id.toLowerCase()) ?? id,
    [names],
  );
  const profileName = useCallback(
    (id: string) =>
      profiles.find((p) => p.profile.id === id)?.profile.name ?? id,
    [profiles],
  );
  return { systemName, profileName };
}

/** What the open library tells the screen (the 25 % rule, the empty state). */
function launchFromLibrary(root: string): DedupeLaunch {
  const view = useLibraryStore.getState().view;
  if (!view || view.root !== root) return { libraryBytes: null };
  return {
    libraryBytes: view.entries.reduce((n, e) => n + e.size, 0),
    systems: view.systems.length,
  };
}

/** A group you asked to leave alone still shows on its shelf, ticked. */
function shownGroups(plan: DedupePlanView, skipped: ReadonlySet<number>) {
  return plan.groups.map((g): DupGroupView =>
    skipped.has(g.id) && g.action.kind === 'report-only'
      ? { ...g, action: { kind: 'act' } }
      : g,
  );
}

export interface DuplicatesScreenProps {
  /** Overrides what the open library says (tests). */
  launch?: DedupeLaunch | undefined;
}

export function DuplicatesScreen({ launch }: DuplicatesScreenProps) {
  const t = useT();
  const root = useNavStore((s) => s.duplicatesRoot);
  const status = useDedupeStore((s) => s.status);
  const plan = useDedupeStore((s) => s.plan);
  const job = useDedupeStore((s) => s.job);
  const error = useDedupeStore((s) => s.error);
  const progress = useDedupeStore((s) => s.progress);
  const undone = useDedupeStore((s) => s.undone);
  const { systemName, profileName } = useNames();

  useEffect(() => {
    if (root && useDedupeStore.getState().root !== root)
      void useDedupeStore
        .getState()
        .start(root, launch ?? launchFromLibrary(root));
    // `launch` is read once, with the root.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [root]);

  const applying = status === 'applying';
  const canReplan = status === 'ready' || (status === 'error' && job === null);
  const replan = useCallback(() => void useDedupeStore.getState().replan(), []);

  // ⌘R / Ctrl+R looks again; the webview never reloads under it.
  useEffect(() => {
    if (!canReplan) return;
    function onKey(ev: KeyboardEvent) {
      if (ev.key.toLowerCase() !== 'r' || !(ev.metaKey || ev.ctrlKey)) return;
      if (ev.altKey) return;
      ev.preventDefault();
      if (isTyping(ev.target)) return;
      replan();
    }
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [canReplan, replan]);

  const back = () => {
    useDedupeStore.getState().reset();
    useNavStore.getState().closeDuplicates();
  };

  const planFailed = status === 'error' && job === null && error !== null;
  const showResult = job !== null && (status === 'done' || status === 'error');

  return (
    // One provider for the replan hint and the relation badges' tooltips.
    <TooltipProvider>
      <main className="settings-screen">
        <header className="settings-header">
          <Button disabled={applying} onClick={back}>
            <ArrowLeft aria-hidden />
            {t('dedupe.back')}
          </Button>
          <h1 className="dup-screen__title">{t('dedupe.title')}</h1>
        </header>
        <div className="mx-auto flex w-full max-w-5xl flex-col gap-4 px-6 py-6">
          {root && (
            <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
              <p
                className="m-0 min-w-0 flex-1 truncate font-mono text-caption text-cr-text-secondary"
                title={root}
              >
                {root}
              </p>
              {canReplan && (
                <SimpleTooltip
                  side="bottom"
                  content={
                    <KbdGroup aria-hidden>
                      <Kbd>{modKeyLabel()}</Kbd>
                      <Kbd>R</Kbd>
                    </KbdGroup>
                  }
                >
                  <Button
                    variant="outline"
                    size="sm"
                    className="ml-auto"
                    aria-keyshortcuts="Meta+R Control+R"
                    onClick={replan}
                  >
                    <RotateCw aria-hidden />
                    {t('dedupe.replan')}
                  </Button>
                </SimpleTooltip>
              )}
            </div>
          )}

          {status === 'planning' && <Planning t={t} />}

          {planFailed && (
            <Alert
              variant="danger"
              action={
                <Button variant="outline" size="sm" onClick={replan}>
                  {t('dedupe.retry')}
                </Button>
              }
            >
              <p className="m-0">
                {t(errorKey(error.code), { detail: error.detail ?? '' })}
              </p>
              {/* A plan never touches a file (brand §7: say what was not). */}
              <p className="m-0">{t('dedupe.nothingMoved')}</p>
            </Alert>
          )}

          {status === 'ready' && error && (
            <Alert variant="danger">
              <p className="m-0">
                {t(errorKey(error.code), { detail: error.detail ?? '' })}
              </p>
            </Alert>
          )}

          {applying && (
            <div className="grid justify-items-start gap-2.5">
              <Progress
                slots={SLOTS}
                max={progress?.total || 1}
                value={
                  progress ? Math.min(progress.done, progress.total) : null
                }
                aria-label={t('dedupe.progressLabel')}
              />
              <div className="flex flex-wrap items-center gap-3">
                <p role="status" className="m-0 text-ui tabular-nums">
                  {progress
                    ? t(
                        job?.trash ? 'dedupe.applyingTrash' : 'dedupe.applying',
                        {
                          done: progress.done,
                          total: progress.total,
                        },
                      )
                    : t('dedupe.starting')}
                </p>
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => void useDedupeStore.getState().cancel()}
                >
                  {t('dedupe.cancel')}
                </Button>
              </div>
            </div>
          )}

          {showResult && plan && (
            <Ended
              t={t}
              plan={plan}
              onBack={back}
              onReplan={replan}
              undone={undone}
            />
          )}

          {status === 'ready' && plan && (
            <Proposal
              plan={plan}
              launch={launch ?? useDedupeStore.getState().launch}
              systemName={systemName}
              profileName={profileName(plan.profile_id)}
              onCancel={back}
            />
          )}
        </div>
      </main>
    </TooltipProvider>
  );
}

/** The planning steps in the order they run, the current one counted. */
function Planning({ t }: { t: TFunction }) {
  const progress = useDedupeStore((s) => s.progress);
  const done = useDedupeStore((s) => s.phasesDone);
  const current =
    progress?.phase === 'apply' ? null : (progress?.phase ?? null);
  const line = !progress
    ? t('dedupe.starting')
    : current === 'fingerprints'
      ? t('dedupe.planning', { done: progress.done, total: progress.total })
      : t('dedupe.phaseProgress', {
          phase: t(PHASE_KEYS[current ?? 'listing']),
          done: progress.done,
          total: progress.total,
        });
  return (
    <div className="grid justify-items-start gap-2.5">
      <Progress
        slots={SLOTS}
        max={progress?.total || 1}
        value={progress ? Math.min(progress.done, progress.total) : null}
        aria-label={t('dedupe.progressLabel')}
      />
      <p role="status" className="m-0 text-ui tabular-nums">
        {line}
      </p>
      <ol className="dup-phases" aria-label={t('dedupe.phasesLabel')}>
        {DEDUPE_PHASES.map((phase) => {
          const state = done.includes(phase)
            ? 'done'
            : phase === current
              ? 'current'
              : 'waiting';
          return (
            <li
              key={phase}
              data-state={state}
              aria-current={state === 'current' ? 'step' : undefined}
            >
              {state === 'done' && <Check size={14} aria-hidden />}
              {state === 'current' && <Spinner size="sm" decorative />}
              {t(PHASE_KEYS[phase])}
              {state === 'done' && (
                <span className="sr-only">, {t('dedupe.phaseDone')}</span>
              )}
            </li>
          );
        })}
      </ol>
      <Button
        variant="ghost"
        size="sm"
        onClick={() => void useDedupeStore.getState().cancel()}
      >
        {t('dedupe.cancel')}
      </Button>
    </div>
  );
}

interface ProposalProps {
  plan: DedupePlanView;
  launch: DedupeLaunch;
  systemName(id: string): string;
  profileName: string;
  onCancel(): void;
}

function Proposal({
  plan,
  launch,
  systemName,
  profileName,
  onCancel,
}: ProposalProps) {
  const t = useT();
  const locale = useContext(LocaleContext);
  const options = useDedupeStore((s) => s.options);
  const action = useDedupeStore((s) => s.action);
  const skipped = useDedupeStore((s) => s.skipped);
  const epoch = useDedupeStore((s) => s.epoch);
  const busy = useDedupeStore((s) => s.busy);
  const exported = useDedupeStore((s) => s.exported);
  const [confirm, setConfirm] = useState<ApplyConfirm | null>(null);
  const store = useDedupeStore.getState;

  const groups = useMemo(() => shownGroups(plan, skipped), [plan, skipped]);
  const systems = useMemo(() => {
    const by = new Map<string, DupGroupView[]>();
    for (const g of sortGroups(groups)) {
      if (g.action.kind === 'report-only') continue;
      const list = by.get(g.system) ?? [];
      list.push(g);
      by.set(g.system, list);
    }
    return [...by.entries()].sort(([a], [b]) =>
      systemName(a).localeCompare(systemName(b), locale),
    );
  }, [groups, systemName, locale]);

  const { totals } = plan;
  const games = Object.values(totals.by_system).reduce((n, s) => n + s[0], 0);
  const extras = totals.extras;
  const bytes = formatBytes(totals.bytes, locale);
  const roms = `${fileName(plan.roms_dir)}/`;
  const folder = `${fileName(plan.quarantine)}/`;
  const trash = plan.trash;
  const countLine =
    extras === 1
      ? t('dedupe.summaryCountOne')
      : games === 1
        ? t('dedupe.summaryCountOneGame', { extras })
        : t('dedupe.summaryCount', { extras, groups: games });
  const applyLabel =
    extras === 0
      ? t('dedupe.apply.none')
      : trash
        ? extras === 1
          ? t('dedupe.applyTrashOne')
          : t('dedupe.apply.trash', { count: extras })
        : extras === 1
          ? t('dedupe.applyOne')
          : t('dedupe.apply.quarantine', { count: extras });

  const offCard = [...new Set(plan.off_card_saves)];
  const conflicts = totals.conflicts;

  const onApply = () => {
    const ask = applyConfirm(plan, launch.libraryBytes ?? null);
    if (ask) setConfirm(ask);
    else void store().apply();
  };
  const changeFolder = async () => {
    const path = await (await getBackend()).pickFolder();
    if (path) void store().setFolder(path);
  };
  const showConflict = () => {
    const first = document.querySelector<HTMLElement>('[data-conflict]');
    first?.scrollIntoView({ block: 'start' });
    first?.focus({ preventScroll: true });
  };

  return (
    <>
      <div className="dup-screen__summary">
        <p className="dup-screen__count">{countLine}</p>
        {extras > 0 && (
          <p className="text-ui tabular-nums text-cr-text">
            {t('dedupe.summaryBytes', { bytes, folder: roms })}
          </p>
        )}
        <p className="text-caption text-cr-text-secondary">
          {t('dedupe.dryRunHint')}
        </p>
      </div>

      <DedupeOptions
        plan={plan}
        options={options}
        action={action}
        trashUnavailable={!plan.trash_available}
        onOptionsChange={(next) => void store().setOptions(next)}
        onActionChange={(next) => void store().setAction(next)}
        onChangeFolder={() => void changeFolder()}
      />

      {conflicts > 0 && (
        <Alert
          variant="warning"
          action={
            <Button variant="outline" size="sm" onClick={showConflict}>
              {t('dedupe.conflicts.show')}
            </Button>
          }
        >
          <p className="m-0 tabular-nums">
            {conflicts === 1
              ? t('dedupe.conflicts.alertOne')
              : t('dedupe.conflicts.alert', { count: conflicts })}
          </p>
        </Alert>
      )}

      {(offCard.length > 0 || plan.chd_unread > 0) && (
        <ul className="dup-notes">
          {offCard.map((s) => (
            <li key={s}>
              {t('dedupe.offCardSaves', { system: systemName(s) })}
            </li>
          ))}
          {plan.chd_unread > 0 && (
            <li className="tabular-nums">
              {t('dedupe.chdUnread', { count: plan.chd_unread })}
            </li>
          )}
        </ul>
      )}

      {plan.groups.length === 0 ? (
        <EmptyState
          size="sm"
          titleAs="p"
          title={t('dedupe.empty.title', { systems: launch.systems ?? 0 })}
          description={
            <>
              {t('dedupe.empty.body')}
              {plan.no_catalogue.length > 0 && (
                <span className="mt-1 block tabular-nums">
                  {t('dedupe.noCatalogue', {
                    count: plan.no_catalogue.length,
                    systems: plan.no_catalogue.join(', '),
                  })}
                </span>
              )}
            </>
          }
          className="rounded-lg border border-dashed border-cr-border-strong"
        />
      ) : (
        <div className="grid gap-3">
          {systems.map(([system, list]) => (
            <SystemSection
              key={`${system}:${epoch}`}
              system={system}
              name={systemName(system)}
              groups={list}
              quarantine={plan.quarantine}
              profileName={profileName}
              skipped={skipped}
              onKeeperChange={(group, member) =>
                void store().setKeeper(group, member)
              }
              onSkipChange={(group, skip) =>
                void store().skipGroup(group, skip)
              }
              onSkipSystem={(s) => void store().skipSystem(s, true)}
              onResetSystem={(s) => void store().resetRules(s)}
              onReveal={(path) => void store().reveal(path)}
            />
          ))}
          <UnchangedList
            groups={groups}
            skipped={skipped}
            systemName={systemName}
          />
        </div>
      )}

      {/* The action bar sits on the page, ruled off like a shelf edge
          (brand §2), as on the organise screen. */}
      <section
        aria-label={applyLabel}
        className="sticky bottom-0 z-[1] flex flex-wrap items-center gap-2 border-t border-cr-border bg-background py-3"
      >
        <Button
          variant="primary"
          disabled={extras === 0 || busy}
          aria-describedby={extras === 0 ? 'dup-none-hint' : undefined}
          onClick={onApply}
        >
          {trash ? <Trash2 aria-hidden /> : <FolderInput aria-hidden />}
          {applyLabel}
        </Button>
        <Button
          variant="outline"
          disabled={busy}
          onClick={() => void store().exportPlan()}
        >
          <FileJson aria-hidden />
          {t('dedupe.exportPlan')}
        </Button>
        <Button variant="ghost" onClick={onCancel}>
          {t('dedupe.cancel')}
        </Button>
        <FreedBytes
          text={(value) => t('dedupe.freed', { folder: roms, bytes: value })}
          bytes={bytes}
        />
        {extras === 0 && (
          <p
            id="dup-none-hint"
            className="m-0 basis-full text-caption text-cr-text-secondary"
          >
            {t('dedupe.noneHint')}
          </p>
        )}
        {exported && (
          <p
            role="status"
            className="m-0 basis-full text-caption text-cr-text-secondary"
          >
            {t('dedupe.exported', { name: exported })}
          </p>
        )}
      </section>

      <AlertDialog
        open={confirm !== null}
        onOpenChange={(open) => !open && setConfirm(null)}
      >
        <AlertDialogContent>
          {confirm?.kind === 'trash' ? (
            <AlertDialogHeader>
              <AlertDialogTitle>
                {t('dedupe.trash.confirmTitle', { count: extras })}
              </AlertDialogTitle>
              <AlertDialogDescription>
                {t('dedupe.trash.confirmBody', { bytes })}
              </AlertDialogDescription>
              {plan.removable && (
                <AlertDialogDescription>
                  {t('dedupe.trash.cardNote')}
                </AlertDialogDescription>
              )}
            </AlertDialogHeader>
          ) : (
            <AlertDialogHeader>
              <AlertDialogTitle>
                {t('dedupe.bulk.confirmTitle', { count: extras })}
              </AlertDialogTitle>
              <AlertDialogDescription className="tabular-nums">
                {t('dedupe.bulk.confirmBody', {
                  bytes,
                  share: confirm?.kind === 'bulk' ? confirm.share : 0,
                  system: `${confirm?.kind === 'bulk' ? confirm.system : ''}/`,
                  folder,
                })}
              </AlertDialogDescription>
              {!plan.same_volume && (
                <AlertDialogDescription>
                  {t('dedupe.bulk.otherVolume', { folder })}
                </AlertDialogDescription>
              )}
            </AlertDialogHeader>
          )}
          <AlertDialogFooter>
            <AlertDialogCancel>{t('dedupe.cancel')}</AlertDialogCancel>
            <AlertDialogAction
              variant={confirm?.kind === 'trash' ? 'destructive' : 'primary'}
              onClick={() => {
                setConfirm(null);
                void store().apply();
              }}
            >
              {applyLabel}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}

/** "Fuera de ROMS/: 1,5 GB" with the number alone in mono (brand: mono
 * for data, never for the words around it). */
function FreedBytes({
  text,
  bytes,
}: {
  text: (value: string) => string;
  bytes: string;
}) {
  const MARK = '\u0000';
  const [before = '', after = ''] = text(MARK).split(MARK);
  return (
    <span className="dup-bar__freed text-dense text-cr-text-secondary">
      {before}
      <span className="dup-mono text-cr-text">{bytes}</span>
      {after}
    </span>
  );
}

interface EndedProps {
  t: TFunction;
  plan: DedupePlanView;
  undone: ReturnType<typeof useDedupeStore.getState>['undone'];
  onBack(): void;
  onReplan(): void;
}

/** After an apply: the result, a resume when it paused, and undo. */
function Ended({ t, plan, undone, onBack, onReplan }: EndedProps) {
  const job = useDedupeStore((s) => s.job)!;
  const error = useDedupeStore((s) => s.error);
  const store = useDedupeStore.getState;
  if (undone) {
    const count = undone.report.reduce((n, r) => n + r.restored.length, 0);
    return (
      <div className="grid justify-items-start gap-3">
        <Alert variant="success">
          <p className="m-0 tabular-nums">
            {count === 1
              ? t('dedupe.undoneOne')
              : t('dedupe.undone', { count })}
          </p>
        </Alert>
        <div className="flex flex-wrap gap-2">
          <Button variant="primary" onClick={onReplan}>
            <RotateCw aria-hidden />
            {t('dedupe.replan')}
          </Button>
          <Button variant="ghost" onClick={onBack}>
            <ArrowLeft aria-hidden />
            {t('dedupe.backToLibrary')}
          </Button>
        </div>
      </div>
    );
  }
  const report = job.report;
  return (
    <>
      {error && (
        <Alert variant="danger">
          <p className="m-0">
            {t(errorKey(error.code), { detail: error.detail ?? '' })}
          </p>
        </Alert>
      )}
      {job.state === 'paused' && report && (
        <Alert
          variant="warning"
          action={
            <Button
              variant="outline"
              size="sm"
              onClick={() => void store().resume()}
            >
              {t('dedupe.resume')}
            </Button>
          }
        >
          <p className="m-0">
            {t('dedupe.paused', {
              reason: report.paused ? t(`jobs.error.${report.paused}`) : '',
            })}
          </p>
        </Alert>
      )}
      {report && (
        <DedupeResult
          plan={plan}
          report={report}
          action={job.trash ? 'trash' : 'quarantine'}
          onOpenFolder={() => void store().reveal(job.quarantine)}
          onUndo={() => void store().undo()}
          onBack={onBack}
        />
      )}
    </>
  );
}
