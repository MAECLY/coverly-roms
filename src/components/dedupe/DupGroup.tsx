/**
 * One duplicate group: the head (cover, title, system and serial, state
 * stamp), the shelf (the copy that stays, raised under the Buckram cap)
 * and the tray (the copies that leave, sunken and indented). The group is one radio group: exactly
 * one copy stays. Picking another moves it to the shelf, slides the cap
 * there, drops the old keeper into the tray and offers "Deshacer" for 10 s.
 *
 * Keys: arrows walk the copies in a stable order (and pick, as radios do),
 * 1–9 keep the n-th copy, Tab leaves the group, X toggles "No tocar este
 * juego", ⌥⌘R / Shift+Alt+R shows the focused copy in the file manager.
 *
 * States (spec §7.1) sit on `data-state` and are never colour alone: a
 * stamp in the head says them, a left edge (a real border, so it survives
 * forced colours) repeats them. `act` has neither; `other` keeps the
 * relation badge with an info edge; `report-only` says «Solo informe» with
 * the reason in a tooltip and a dashed edge; `skipped` says «No se toca»,
 * dashed, and dims the art and chips (never the text).
 */
import { CircleSlash, Eye, type LucideIcon } from 'lucide-react';
import {
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type CSSProperties,
  type KeyboardEvent,
  type ReactNode,
} from 'react';

import { RuleChip } from '@/components/dedupe/CopyFacts';
import { GroupCover, OwnArtThumb } from '@/components/dedupe/GroupCover';
import { KeeperRow } from '@/components/dedupe/KeeperRow';
import { RelationBadge, TrayRow } from '@/components/dedupe/TrayRow';
import { SystemLabel } from '@/components/system/SystemLabel';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { RadioGroup } from '@/components/ui/radio-group';
import { SimpleTooltip } from '@/components/ui/tooltip';
import { useReducedMotion } from '@/hooks/useReducedMotion';
import { useT } from '@/i18n/useT';
import type { DedupeOutcomeView, DupGroupView } from '@/ipc/dedupeTypes';
import {
  badgeRelation,
  coverMedia,
  currentPath,
  fileName,
  reasonText,
  reportWhyText,
  weakestExtra,
} from '@/lib/dedupe';
import { digitOf, isRevealChord, isTyping } from '@/lib/keyboard';
import { revealOs } from '@/lib/reveal';

import './dedupe.css';

/** How long "Deshacer" stays after a pick. */
export const UNDO_PICK_MS = 10_000;

/** What a group will do, at a glance (spec §7.1). */
export type GroupState = 'act' | 'other' | 'report-only' | 'skipped';

/** A word and an icon in the head; `tip` makes it a tab stop. */
export function StateStamp({
  icon: Icon,
  text,
  tone = 'neutral',
  tip,
}: {
  icon: LucideIcon;
  text: string;
  tone?: 'neutral' | 'warning';
  tip?: string | undefined;
}) {
  const badge = (
    <Badge
      variant={tone === 'warning' ? 'warning' : 'outline'}
      className="dup-stamp"
      data-tone={tone}
      tabIndex={tip ? 0 : undefined}
    >
      <Icon size={14} aria-hidden />
      {text}
    </Badge>
  );
  return tip ? <SimpleTooltip content={tip}>{badge}</SimpleTooltip> : badge;
}

/** The game: title in the box-label face, its system and serial under it,
 * the stamps on the right. Shared with the conflict group. */
export function GroupHead({
  titleId,
  title,
  system,
  systemName,
  serial,
  stamps,
}: {
  titleId: string;
  title: string;
  system: string;
  systemName?: string | undefined;
  serial?: string | undefined;
  stamps?: ReactNode;
}) {
  return (
    <div className="dup-group__head">
      <div className="dup-group__titles">
        <h3 id={titleId} className="dup-group__title">
          {title}
        </h3>
        {(systemName || serial) && (
          <p className="dup-group__sub">
            {systemName && (
              <SystemLabel
                systemId={system}
                name={systemName}
                className="dup-group__system"
              />
            )}
            {serial && <span className="dup-group__serial">{serial}</span>}
          </p>
        )}
      </div>
      {stamps && <div className="dup-group__stamps">{stamps}</div>}
    </div>
  );
}

export interface DupGroupProps {
  group: DupGroupView;
  /** Absolute quarantine folder; destinations are written from it down. */
  quarantine: string;
  /** Display name of the plan's profile, for "preferred format on …". */
  profileName?: string | undefined;
  /** Initial state of "No tocar este juego". */
  skipped?: boolean;
  /** The keeper can be changed and the game left alone (the library
   * screen); `false` shows the rules' decision only (the Organise
   * proposal settles it with its own picker). */
  pickable?: boolean;
  /** Every copy stays, whatever the action (Organise "Solo informar"). */
  reportOnly?: boolean;
  onKeeperChange?: ((groupId: number, keeper: number) => void) | undefined;
  onSkipChange?: ((groupId: number, skipped: boolean) => void) | undefined;
  /** The kept dedupe plan, to ask for covers. Organise has none, so its
   * groups show no cover column and no thumbnails (spec goal 6). */
  planId?: number | undefined;
  /** The row is mounted by the group window: only then do covers load. */
  artEnabled?: boolean;
  /** Show a copy in the file manager (DP-3); without it, no reveal. */
  onReveal?: ((path: string) => void) | undefined;
  /** The last apply's outcomes: a copy set aside is revealed where it
   * went, one sent to the Trash not at all (DP-4). */
  outcomes?: DedupeOutcomeView[] | null | undefined;
  /** The system's display name, under the title with its icon. */
  systemName?: string | undefined;
}

interface Pick {
  from: number;
  to: number;
}

export function DupGroup({
  group,
  quarantine,
  profileName,
  skipped = false,
  pickable = true,
  reportOnly = false,
  onKeeperChange,
  onSkipChange,
  planId,
  artEnabled = true,
  onReveal,
  outcomes = null,
  systemName,
}: DupGroupProps) {
  const t = useT();
  const uid = useId();
  const reduce = useReducedMotion();
  const [keeper, setKeeper] = useState(group.keeper);
  const [skip, setSkip] = useState(skipped);
  const [pick, setPick] = useState<Pick | null>(null);
  const shelfRef = useRef<HTMLDivElement>(null);
  const rootRef = useRef<HTMLDivElement>(null);
  const capShift = useRef<number | null>(null);
  const focusNext = useRef(false);

  const acts = group.action.kind === 'act' && !reportOnly;
  /** A radio group to pick the keeper, and "No tocar este juego". */
  const picks = acts && pickable;
  const ids = group.members.map((m) => m.id);
  const keeperMember =
    group.members.find((m) => m.id === keeper) ?? group.members[0]!;
  // Relations are measured against the planned keeper. After a pick, the
  // planned keeper relates to the new one as the new one related to it.
  const members = group.members.map((m) =>
    m.id === group.keeper && m.id !== keeperMember.id
      ? {
          ...m,
          relation: keeperMember.relation,
          confidence: keeperMember.confidence,
        }
      : m,
  );
  const tray = members.filter((m) => m.id !== keeperMember.id);
  const titleId = `${uid}-title`;

  const why =
    keeper !== group.keeper
      ? reasonText(t, { kind: 'user-choice' }, {})
      : group.reason.kind === 'tie' && !picks
        ? // Nothing to pick here: no "pick another if you prefer".
          t('dedupe.why.tieSettled')
        : reasonText(t, group.reason, profileName ? { profileName } : {});
  const userPicked = keeper !== group.keeper;
  const runner = group.runner_up
    ? group.members.find((m) => m.id === group.runner_up![0])
    : undefined;
  const runnerUp =
    runner && group.runner_up
      ? { name: fileName(runner.unit.primary), reason: group.runner_up[1] }
      : null;
  const copyNumber = (id: number) => ids.indexOf(id) + 1;
  const weakest = weakestExtra(members, keeperMember.id);
  const mixed = new Set(tray.map((m) => badgeRelation(m))).size > 1;
  const reportsOnly = group.action.kind === 'report-only' || reportOnly;
  const skipping = picks && skip;
  const weakRelation = weakest ? badgeRelation(weakest) : 'exact';
  const state: GroupState = reportsOnly
    ? 'report-only'
    : skipping
      ? 'skipped'
      : weakest && weakest.relation !== 'exact'
        ? 'other'
        : 'act';
  const coverIn = planId === undefined ? null : coverMedia(group).media;
  const ownArt = (m: (typeof members)[number], dim = false) =>
    planId !== undefined && coverIn ? (
      <OwnArtThumb
        planId={planId}
        group={group}
        member={m}
        media={coverIn}
        enabled={artEnabled}
        dim={dim}
      />
    ) : null;

  function radioOf(id: number): HTMLElement | null {
    return (
      rootRef.current?.querySelector<HTMLElement>(
        `[role="radio"][value="${id}"]`,
      ) ?? null
    );
  }

  function moveKeeper(next: number) {
    if (next === keeper) return;
    const row = radioOf(next)?.closest('.dup-row');
    const shelf = shelfRef.current;
    if (row && shelf)
      capShift.current =
        row.getBoundingClientRect().top - shelf.getBoundingClientRect().top;
    focusNext.current = true;
    setKeeper(next);
    onKeeperChange?.(group.id, next);
  }

  function choose(next: number) {
    if (next === keeper) return;
    setPick({ from: keeper, to: next });
    moveKeeper(next);
  }

  function undo() {
    if (!pick) return;
    setPick(null);
    moveKeeper(pick.from);
  }

  // The cap slides from the row you picked up to the shelf (FLIP), and
  // focus follows the copy that now stays (its row was remounted).
  useLayoutEffect(() => {
    const shelf = shelfRef.current;
    const shift = capShift.current;
    capShift.current = null;
    if (shelf && shift) {
      shelf.style.setProperty('--dup-cap-shift', `${shift}px`);
      shelf.setAttribute('data-cap-jump', '');
      void shelf.offsetHeight;
      shelf.removeAttribute('data-cap-jump');
      shelf.style.setProperty('--dup-cap-shift', '0px');
    }
    if (focusNext.current) {
      focusNext.current = false;
      radioOf(keeper)?.focus();
    }
  }, [keeper]);

  useEffect(() => {
    if (!pick) return;
    const timer = setTimeout(() => setPick(null), UNDO_PICK_MS);
    return () => clearTimeout(timer);
  }, [pick]);

  function setSkipped(next: boolean) {
    setSkip(next);
    onSkipChange?.(group.id, next);
  }

  function onKeyDownCapture(e: KeyboardEvent<HTMLDivElement>) {
    if (!picks || e.altKey || e.ctrlKey || e.metaKey) return;
    const target = e.target as HTMLElement;
    if (target.getAttribute('role') !== 'radio') return;
    const step =
      e.key === 'ArrowDown' || e.key === 'ArrowRight'
        ? 1
        : e.key === 'ArrowUp' || e.key === 'ArrowLeft'
          ? -1
          : 0;
    if (!step) return;
    // Radix would walk the DOM, which reorders on every pick; walk the
    // copies in their own order instead.
    e.preventDefault();
    e.stopPropagation();
    const at = ids.indexOf(keeper);
    choose(ids[(at + step + ids.length) % ids.length]!);
  }

  /** The copy whose row holds focus, else the keeper. */
  function focusedMember() {
    const row = (document.activeElement as HTMLElement | null)?.closest(
      '[data-member]',
    );
    const id =
      row && rootRef.current?.contains(row)
        ? Number((row as HTMLElement).dataset.member)
        : keeperMember.id;
    return members.find((m) => m.id === id) ?? keeperMember;
  }

  function onKeyDown(e: KeyboardEvent<HTMLDivElement>) {
    const target = e.target as HTMLElement;
    if (
      isTyping(target) ||
      target.closest('input, textarea, [contenteditable="true"]')
    )
      return;
    // ⌥⌘R is WebKit's reload from origin: never let it through.
    if (isRevealChord(e.nativeEvent, revealOs())) {
      e.preventDefault();
      if (!onReveal) return;
      const m = focusedMember();
      const path = currentPath(m, group.id, outcomes);
      if (path !== null) onReveal(path);
      return;
    }
    if (!picks || e.altKey || e.ctrlKey || e.metaKey) return;
    // 1–9 by physical key; Shift is allowed (AZERTY types digits with it).
    const n = digitOf(e);
    if (n !== null) {
      const id = ids[n - 1];
      if (id === undefined) return;
      e.preventDefault();
      choose(id);
      return;
    }
    if (e.key !== 'x' && e.key !== 'X') return;
    e.preventDefault();
    setSkipped(!skip);
  }

  const rows = (
    <>
      <div ref={shelfRef} className="dup-shelf">
        <KeeperRow
          key={keeperMember.id}
          member={keeperMember}
          interactive={picks}
          why={why}
          choiceDropped={group.choice_dropped && keeper === group.keeper}
          copyNumber={copyNumber(keeperMember.id)}
          art={ownArt(keeperMember)}
          place={currentPath(keeperMember, group.id, outcomes)}
          onReveal={onReveal}
          rule={
            <RuleChip
              reason={group.reason}
              userPicked={userPicked}
              runnerUp={runnerUp}
            />
          }
          idPrefix={`${uid}-m${keeperMember.id}`}
        />
      </div>
      {tray.length > 0 && (
        <div className="dup-tray">
          {tray.map((m) => (
            <TrayRow
              key={m.id}
              member={m}
              members={members}
              keeper={keeperMember}
              interactive={acts}
              pickable={picks}
              showRelation={mixed}
              quarantine={quarantine}
              system={group.system}
              copyNumber={copyNumber(m.id)}
              art={ownArt(m, skipping)}
              place={currentPath(m, group.id, outcomes)}
              onReveal={onReveal}
              dim={skipping}
              idPrefix={`${uid}-m${m.id}`}
            />
          ))}
        </div>
      )}
    </>
  );

  const style = {
    '--dup-cap-duration': reduce ? '0ms' : 'var(--cr-duration-base)',
  } as CSSProperties;

  const content = (
    <>
      <GroupHead
        titleId={titleId}
        title={group.title}
        system={group.system}
        systemName={systemName}
        serial={keeperMember.serials[0]}
        stamps={
          (weakRelation !== 'exact' || state !== 'act') && (
            <>
              {weakest && weakRelation !== 'exact' && (
                <RelationBadge
                  member={weakest}
                  focusable
                  className="dup-group__badge dup-stamp"
                />
              )}
              {state === 'report-only' && (
                <StateStamp
                  icon={Eye}
                  text={t('dedupe.state.reportOnly')}
                  tip={
                    group.action.kind === 'report-only'
                      ? reportWhyText(t, group.action.why)
                      : undefined
                  }
                />
              )}
              {state === 'skipped' && (
                <StateStamp
                  icon={CircleSlash}
                  text={t('dedupe.state.skipped')}
                />
              )}
            </>
          )
        }
      />

      {picks ? (
        <RadioGroup
          aria-labelledby={titleId}
          value={String(keeper)}
          onValueChange={(v) => choose(Number(v))}
          className="block"
        >
          {rows}
        </RadioGroup>
      ) : (
        <div>{rows}</div>
      )}

      {picks && (
        <div className="dup-group__foot">
          <label className="dup-group__skip">
            <Checkbox
              checked={skip}
              onCheckedChange={(c) => setSkipped(c === true)}
            />
            {t('dedupe.skipGroup')}
          </label>
          <div className="dup-group__notice">
            <p role="status" aria-live="polite">
              {pick
                ? t('dedupe.keeperChanged', {
                    name: fileName(
                      group.members.find((m) => m.id === pick.to)!.unit.primary,
                    ),
                    old: fileName(
                      group.members.find((m) => m.id === pick.from)!.unit
                        .primary,
                    ),
                  })
                : ''}
            </p>
            {pick && (
              <Button variant="link" size="sm" onClick={undo}>
                {t('dedupe.undoPick')}
              </Button>
            )}
          </div>
        </div>
      )}
    </>
  );

  return (
    // The group only delegates keys from its own radios and checkbox
    // (arrows, X); it is not itself focusable or clickable.
    // eslint-disable-next-line jsx-a11y/no-noninteractive-element-interactions
    <div
      ref={rootRef}
      role="group"
      aria-labelledby={titleId}
      className="dup-group"
      data-dup-group={group.id}
      data-state={state}
      data-skipped={skip ? '' : undefined}
      data-cover={planId !== undefined ? '' : undefined}
      style={style}
      onKeyDownCapture={onKeyDownCapture}
      onKeyDown={onKeyDown}
    >
      {planId !== undefined ? (
        <>
          <GroupCover
            planId={planId}
            group={group}
            enabled={artEnabled}
            dim={skipping}
          />
          <div className="dup-group__main">{content}</div>
        </>
      ) : (
        content
      )}
    </div>
  );
}
