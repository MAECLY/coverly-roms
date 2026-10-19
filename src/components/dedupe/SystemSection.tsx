/**
 * One system: a collapsible panel whose trigger reads as a sentence
 * ("Master System: 23 juegos, 23 copias", R2) with the bytes that leave in
 * the size column, so system totals line up with the row sizes below. The
 * id stays as a `title`. A menu holds "No tocar {system}" and "Volver a
 * aplicar las reglas en {system}".
 *
 * Closed, it mounts no rows. Open with more than 200 groups, the groups are
 * windowed: only those near the viewport are in the DOM, and focus keeps
 * its neighbours mounted so `Tab` follows the data.
 */
import { ChevronRight, Ellipsis, TriangleAlert } from 'lucide-react';
import { useContext, useId, useState } from 'react';

import { ConflictGroup } from '@/components/dedupe/ConflictGroup';
import { DupGroup } from '@/components/dedupe/DupGroup';
import { useGroupWindow } from '@/components/dedupe/useGroupWindow';
import { Button } from '@/components/ui/button';
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { LocaleContext } from '@/i18n/core';
import { useT, type TFunction } from '@/i18n/useT';
import type { DupGroupView } from '@/ipc/dedupeTypes';
import {
  WINDOW_ABOVE,
  formatBytes,
  systemStartsOpen,
  systemSummary,
  type SystemSummary,
} from '@/lib/dedupe';

import './dedupe.css';

export interface SystemSectionProps {
  /** System id (`SMS`): the `title` of the trigger. */
  system: string;
  /** Display name (`Master System`). */
  name: string;
  /** This system's groups that act or conflict, in display order. */
  groups: DupGroupView[];
  quarantine: string;
  profileName?: string | undefined;
  /** Groups under "No tocar este juego". */
  skipped?: ReadonlySet<number> | undefined;
  /** Default: open with a conflict or at most 50 groups. */
  defaultOpen?: boolean | undefined;
  onKeeperChange?: ((groupId: number, keeper: number) => void) | undefined;
  onSkipChange?: ((groupId: number, skipped: boolean) => void) | undefined;
  onSkipSystem?: ((system: string) => void) | undefined;
  onResetSystem?: ((system: string) => void) | undefined;
  onReveal?: ((path: string) => void) | undefined;
}

function sentence(t: TFunction, name: string, s: SystemSummary): string {
  if (s.groups === 0) return t('dedupe.system.summaryNone', { name });
  if (s.groups === 1)
    return s.extras === 1
      ? t('dedupe.system.summaryOne', { name })
      : t('dedupe.system.summaryOneGame', { name, extras: s.extras });
  return t('dedupe.system.summary', {
    name,
    groups: s.groups,
    extras: s.extras,
  });
}

const EMPTY: SystemSummary = {
  system: '',
  groups: 0,
  extras: 0,
  bytes: 0,
  conflicts: 0,
  total: 0,
};

export function SystemSection({
  system,
  name,
  groups,
  defaultOpen,
  onSkipSystem,
  onResetSystem,
  ...row
}: SystemSectionProps) {
  const t = useT();
  const locale = useContext(LocaleContext);
  const uid = useId();
  const summary = systemSummary(groups)[0] ?? EMPTY;
  const [open, setOpen] = useState(
    () => defaultOpen ?? systemStartsOpen(summary),
  );
  const sizeId = `${uid}-size`;
  const triggerId = `${uid}-trigger`;

  return (
    <Collapsible
      open={open}
      onOpenChange={setOpen}
      className="dup-system"
      data-system={system}
    >
      <div className="dup-system__head">
        <CollapsibleTrigger asChild>
          <button
            type="button"
            id={triggerId}
            title={system}
            aria-describedby={sizeId}
            className="dup-system__trigger group"
          >
            <ChevronRight
              aria-hidden
              size={16}
              className="dup-system__chevron"
            />
            <span className="dup-system__name">
              {sentence(t, name, summary)}
            </span>
          </button>
        </CollapsibleTrigger>
        {summary.conflicts > 0 && (
          <span className="dup-system__conflicts">
            <TriangleAlert size={14} aria-hidden />
            {t('dedupe.system.conflicts', { count: summary.conflicts })}
          </span>
        )}
        {(onSkipSystem || onResetSystem) && (
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                variant="ghost"
                size="icon-sm"
                className="dup-system__menu"
                aria-label={t('dedupe.system.menu', { system: name })}
              >
                <Ellipsis aria-hidden />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="start">
              {onSkipSystem && (
                <DropdownMenuItem onSelect={() => onSkipSystem(system)}>
                  {t('dedupe.skipSystem', { system: name })}
                </DropdownMenuItem>
              )}
              {onResetSystem && (
                <DropdownMenuItem onSelect={() => onResetSystem(system)}>
                  {t('dedupe.resetSystem', { system: name })}
                </DropdownMenuItem>
              )}
            </DropdownMenuContent>
          </DropdownMenu>
        )}
        <span id={sizeId} className="dup-system__size">
          {formatBytes(summary.bytes, locale)}
        </span>
      </div>
      {/* Radix unmounts closed content: a closed system has no rows. */}
      <CollapsibleContent className="dup-system__body">
        <Groups groups={groups} {...row} />
      </CollapsibleContent>
    </Collapsible>
  );
}

type GroupsProps = Omit<
  SystemSectionProps,
  'system' | 'name' | 'defaultOpen' | 'onSkipSystem' | 'onResetSystem'
>;

function Groups({
  groups,
  quarantine,
  profileName,
  skipped,
  onKeeperChange,
  onSkipChange,
  onReveal,
}: GroupsProps) {
  const windowed = groups.length > WINDOW_ABOVE;
  const { listRef, range, measure, onFocusIndex } = useGroupWindow(
    groups.length,
    windowed,
  );

  const item = (i: number) => {
    const g = groups[i]!;
    return g.action.kind === 'conflict' ? (
      <ConflictGroup group={g} onReveal={onReveal} />
    ) : (
      <DupGroup
        group={g}
        quarantine={quarantine}
        profileName={profileName}
        skipped={skipped?.has(g.id) ?? false}
        onKeeperChange={onKeeperChange}
        onSkipChange={onSkipChange}
      />
    );
  };

  if (!windowed)
    return (
      <div className="dup-system__groups">
        {groups.map((g, i) => (
          <div key={g.id} className="dup-system__slot">
            {item(i)}
          </div>
        ))}
      </div>
    );

  const out = [];
  let cursor = 0;
  for (const i of range.indices) {
    const gap = range.offsets[i]! - cursor;
    if (gap > 0)
      out.push(
        <div
          key={`gap-${i}`}
          aria-hidden
          className="dup-system__spacer"
          style={{ height: gap }}
        />,
      );
    out.push(
      <div
        key={groups[i]!.id}
        ref={(el) => measure(i, el)}
        className="dup-system__slot"
        onFocus={() => onFocusIndex(i)}
      >
        {item(i)}
      </div>,
    );
    const next = range.offsets[i + 1] ?? range.total;
    cursor = next;
  }
  if (range.total > cursor)
    out.push(
      <div
        key="gap-end"
        aria-hidden
        className="dup-system__spacer"
        style={{ height: range.total - cursor }}
      />,
    );

  return (
    <div ref={listRef} className="dup-system__groups" data-windowed="">
      {out}
    </div>
  );
}
