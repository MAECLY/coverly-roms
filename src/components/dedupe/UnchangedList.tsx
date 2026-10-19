/**
 * "Sin cambios": the groups that stay as they are, each with its reason
 * (report-only groups and the ones you asked to leave alone). Collapsed by
 * default; rows mount only while open, 200 at a time. Conflicts are not
 * here: they have their own block.
 */
import { ChevronRight } from 'lucide-react';
import { useState } from 'react';

import { Button } from '@/components/ui/button';
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible';
import { useT } from '@/i18n/useT';
import type { DupGroupView } from '@/ipc/dedupeTypes';
import {
  WINDOW_ABOVE,
  fileName,
  reportWhyText,
  unchangedItems,
} from '@/lib/dedupe';

import './dedupe.css';

export interface UnchangedListProps {
  /** Every group of the plan; the list picks the ones that stay. */
  groups: DupGroupView[];
  /** Groups under "No tocar este juego". */
  skipped?: ReadonlySet<number> | undefined;
  systemName?: ((id: string) => string) | undefined;
}

export function UnchangedList({
  groups,
  skipped,
  systemName = (id) => id,
}: UnchangedListProps) {
  const t = useT();
  const items = unchangedItems(groups, skipped);
  const [shown, setShown] = useState(WINDOW_ABOVE);
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
            onClick={() => setShown((n) => n + WINDOW_ABOVE)}
          >
            {t('dedupe.showMore', { count: rest })}
          </Button>
        )}
      </CollapsibleContent>
    </Collapsible>
  );
}
