/**
 * One duplicate group: the title line, the shelf (the copy that stays) and
 * the tray (the copies that leave). The group is one radio group: exactly
 * one copy stays. Picking another moves it to the shelf, slides the cap
 * there, drops the old keeper into the tray and offers "Deshacer" for 10 s.
 *
 * Keys: arrows walk the copies in a stable order (and pick, as radios do),
 * Tab leaves the group, X toggles "No tocar este juego".
 */
import {
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type CSSProperties,
  type KeyboardEvent,
} from 'react';

import { KeeperRow } from '@/components/dedupe/KeeperRow';
import { RelationBadge, TrayRow } from '@/components/dedupe/TrayRow';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { RadioGroup } from '@/components/ui/radio-group';
import { useReducedMotion } from '@/hooks/useReducedMotion';
import { useT } from '@/i18n/useT';
import type { DupGroupView } from '@/ipc/dedupeTypes';
import {
  badgeRelation,
  fileName,
  reasonText,
  weakestExtra,
} from '@/lib/dedupe';

import './dedupe.css';

/** How long "Deshacer" stays after a pick. */
export const UNDO_PICK_MS = 10_000;

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
  const weakest = weakestExtra(members, keeperMember.id);
  const mixed = new Set(tray.map((m) => badgeRelation(m))).size > 1;

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

  function onKeyDown(e: KeyboardEvent<HTMLDivElement>) {
    if (!picks || e.altKey || e.ctrlKey || e.metaKey) return;
    if (e.key !== 'x' && e.key !== 'X') return;
    const target = e.target as HTMLElement;
    if (target.closest('input, textarea, [contenteditable="true"]')) return;
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
      data-skipped={skip ? '' : undefined}
      style={style}
      onKeyDownCapture={onKeyDownCapture}
      onKeyDown={onKeyDown}
    >
      <div className="dup-group__head">
        <h3 id={titleId} className="dup-group__title">
          {group.title}
        </h3>
        {keeperMember.serials[0] && (
          <span className="dup-group__serial">{keeperMember.serials[0]}</span>
        )}
        {weakest && (
          <RelationBadge
            member={weakest}
            focusable
            className="dup-group__badge"
          />
        )}
      </div>

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
    </div>
  );
}
