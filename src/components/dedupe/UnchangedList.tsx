/**
 * "Sin cambios": the groups that stay as they are, each with its reason
 * (report-only groups and the ones you asked to leave alone). Collapsed by
 * default; rows mount only while open, a page at a time. Conflicts are not
 * here: they have their own block.
 *
 * A report-only game is a whole `DupGroup` in its «Solo informe» state:
 * every copy with its tags, facts, cover and reveal, nothing to pick
 * (spec goals 1, 2 and 5; two versions of a game are told apart by their
 * region). A game you asked to leave alone is a one-line reminder: its
 * full group is still on its system's shelf, ticked.
 */
import { ChevronRight } from 'lucide-react';
import { useState } from 'react';

import { DupGroup } from '@/components/dedupe/DupGroup';
import { Button } from '@/components/ui/button';
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible';
import { useT } from '@/i18n/useT';
import type { DedupeOutcomeView, DupGroupView } from '@/ipc/dedupeTypes';
import { fileName, reportWhyText, unchangedItems } from '@/lib/dedupe';

import './dedupe.css';

/** Rows mounted per «Mostrar más»: each report-only row is a whole group
 * that may ask for its cover, so pages stay small. */
export const UNCHANGED_PAGE = 50;

export interface UnchangedListProps {
  /** Every group of the plan; the list picks the ones that stay. */
  groups: DupGroupView[];
  /** Groups under "No tocar este juego". */
  skipped?: ReadonlySet<number> | undefined;
  systemName?: ((id: string) => string) | undefined;
  /** Absolute quarantine folder (the groups' rows need it). */
  quarantine?: string | undefined;
  profileName?: string | undefined;
  /** The kept plan, for the report-only groups' covers. */
  planId?: number | undefined;
  /** Show a copy in the file manager; without it, no reveal. */
  onReveal?: ((path: string) => void) | undefined;
  outcomes?: DedupeOutcomeView[] | null | undefined;
}

export function UnchangedList({
  groups,
  skipped,
  systemName = (id) => id,
  quarantine = '',
  profileName,
  planId,
  onReveal,
  outcomes = null,
}: UnchangedListProps) {
  const t = useT();
  const items = unchangedItems(groups, skipped);
  const [shown, setShown] = useState(UNCHANGED_PAGE);
  if (items.length === 0) return null;

  // The reasons in the trigger, most common first.
  const tally = new Map<string, number>();
  for (const it of items) {
    const text = reportWhyText(t, it.why);
    tally.set(text, (tally.get(text) ?? 0) + 1);
  }
  const reasons = [...tally.entries()]
    .sort((a, b) => b[1] - a[1])
    .map(([text]) => text);
  const rest = items.length - Math.min(shown, items.length);

  return (
    <Collapsible className="dup-unchanged">
      <CollapsibleTrigger asChild>
        <button type="button" className="dup-unchanged__trigger group">
          <ChevronRight aria-hidden size={16} className="dup-system__chevron" />
          <span className="dup-unchanged__count">
            {t('dedupe.unchanged', { count: items.length })}
          </span>{' '}
          <span className="dup-unchanged__reasons">{reasons.join('; ')}</span>
        </button>
      </CollapsibleTrigger>
      <CollapsibleContent>
        <ul className="dup-unchanged__list">
          {items.slice(0, shown).map(({ group: g, why }) => {
            if (why.kind !== 'user-skipped')
              return (
                <li key={g.id} className="dup-unchanged__group">
                  <DupGroup
                    group={g}
                    quarantine={quarantine}
                    profileName={profileName}
                    pickable={false}
                    planId={planId}
                    onReveal={onReveal}
                    outcomes={outcomes}
                    systemName={systemName(g.system)}
                    // Only the rows mounted on this page are here.
                    artEnabled
                  />
                </li>
              );
            const keeper =
              g.members.find((m) => m.id === g.keeper) ?? g.members[0];
            return (
              <li key={g.id} className="dup-unchanged__row">
                <span className="dup-unchanged__title">{g.title}</span>
                <span className="dup-unchanged__system" title={g.system}>
                  {systemName(g.system)}
                </span>
                {keeper && (
                  <span className="dup-mono dup-unchanged__file">
                    {fileName(keeper.unit.primary)}
                  </span>
                )}
                <span className="dup-unchanged__why">
                  {reportWhyText(t, why)}
                </span>
              </li>
            );
          })}
        </ul>
        {rest > 0 && (
          <Button
            variant="ghost"
            size="sm"
            onClick={() => setShown((n) => n + UNCHANGED_PAGE)}
          >
            {t('dedupe.showMore', { count: rest })}
          </Button>
        )}
      </CollapsibleContent>
    </Collapsible>
  );
}
