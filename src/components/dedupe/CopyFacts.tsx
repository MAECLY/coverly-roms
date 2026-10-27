/**
 * What sets one copy apart from its twins (spec D3, D8–D11): its tags
 * (region codes, revision, languages, release class, dump and modification
 * marks), then its facts (format, size, modified date, save, plays), each
 * an inline item with gap spacing, never a dotted line. `RuleChip` names
 * the rule that kept the copy on the shelf.
 */
import { CalendarDays, Gamepad2, Save } from 'lucide-react';
import { useContext } from 'react';

import { SimpleTooltip } from '@/components/ui/tooltip';
import { LocaleContext } from '@/i18n/core';
import { useT } from '@/i18n/useT';
import type {
  KeepReasonView,
  MemberView,
  TagSourceView,
} from '@/ipc/dedupeTypes';
import {
  formatBytes,
  formatOf,
  newestMtime,
  ruleKey,
  tagChips,
  type TagChip,
} from '@/lib/dedupe';
import { formatDate, formatDateTime, formatRelative } from '@/lib/format';
import { cn } from '@/lib/utils';

const SOURCE_KEYS: Record<TagSourceView, string> = {
  catalogue: 'dedupe.tags.from.catalogue',
  'file-name': 'dedupe.tags.from.fileName',
};

function Tag({ chip }: { chip: TagChip }) {
  return (
    <span
      className="dup-tag"
      data-tone={chip.tone}
      {...(chip.title ? { title: chip.title } : {})}
    >
      {chip.text}
      {chip.hidden && <span className="sr-only">{chip.hidden}</span>}
    </span>
  );
}

/**
 * The chips as one list. Where they were read from is a tooltip on the
 * list, so it is one tab stop, never one per chip.
 */
function Tags({ chips }: { chips: TagChip[] }) {
  const t = useT();
  const sources = [
    ...new Set(chips.flatMap((c) => (c.from ? [c.from] : []))),
  ].map((s) => t(SOURCE_KEYS[s]));
  const list = chips.map((c) => <Tag key={c.key} chip={c} />);
  if (sources.length === 0) return <span className="dup-tags">{list}</span>;
  const label = sources.join(', ');
  return (
    <SimpleTooltip content={label}>
      {/* A focusable list so the tooltip reaches keyboard users. */}
      {/* eslint-disable-next-line jsx-a11y/no-noninteractive-tabindex */}
      <span role="group" aria-label={label} tabIndex={0} className="dup-tags">
        {list}
      </span>
    </SimpleTooltip>
  );
}

export interface CopyFactsProps {
  member: MemberView;
  /** Conflicts say «Sin partida» when a copy has none. */
  showNoSave?: boolean;
}

/** Tag chips, then the facts line: format, size, modified date, save, played. */
export function CopyFacts({ member, showNoSave = false }: CopyFactsProps) {
  const t = useT();
  const locale = useContext(LocaleContext);
  const facts = member.facts;
  const chips = tagChips(facts, t, locale);
  const mtime = newestMtime(member.unit) / 1e9;
  const saves = facts?.saves;
  const played = facts?.played;

  let save: { text: string; when: string } | null = null;
  if (saves && saves.count > 0) {
    const ms = saves.last_ns / 1e6;
    const rel = formatRelative(ms, locale);
    save = {
      text:
        saves.count === 1
          ? t('dedupe.facts.save', { when: rel })
          : t('dedupe.facts.saves', { count: saves.count, when: rel }),
      when: formatDateTime(ms / 1000, locale),
    };
  }

  return (
    <div className="dup-facts">
      {chips.length > 0 && <Tags chips={chips} />}
      <span className="dup-facts__line">
        <span className="dup-facts__item dup-mono">
          {formatOf(member.unit, t)}
        </span>
        <span className="dup-facts__item dup-mono">
          {formatBytes(member.size, locale)}
        </span>
        <span
          className="dup-facts__item"
          title={t('dedupe.facts.modified', {
            date: formatDateTime(mtime, locale),
          })}
        >
          <CalendarDays size={14} aria-hidden />
          <span className="dup-mono">{formatDate(mtime, locale)}</span>
        </span>
        {save ? (
          <span className="dup-facts__item" title={save.when}>
            <Save size={14} aria-hidden />
            <span>{save.text}</span>
            <span className="sr-only">({save.when})</span>
          </span>
        ) : (
          showNoSave && (
            <span className="dup-facts__item dup-facts__none">
              <Save size={14} aria-hidden />
              <span>{t('dedupe.facts.noSave')}</span>
            </span>
          )
        )}
        {played && played.count > 0 && (
          <span className="dup-facts__item">
            <Gamepad2 size={14} aria-hidden />
            <span>
              {played.count === 1
                ? t('dedupe.facts.playedOne')
                : t('dedupe.facts.played', { count: played.count })}
            </span>
          </span>
        )}
      </span>
    </div>
  );
}

export interface RuleChipProps {
  reason: KeepReasonView;
  /** You picked this copy: «Tu elección», whatever the rules said. */
  userPicked: boolean;
  /** Who would stay otherwise, and on which rule. */
  runnerUp: { name: string; reason: KeepReasonView } | null;
}

/** The rule that put the copy on the shelf, in one word (spec D11). */
export function RuleChip({ reason, userPicked, runnerUp }: RuleChipProps) {
  const t = useT();
  const locale = useContext(LocaleContext);
  const kind = userPicked ? 'user-choice' : reason.kind;
  const user = kind === 'user-choice';
  const chip = (
    <span
      className={cn('dup-rule', runnerUp && !user && 'dup-rule--tip')}
      data-user={user ? '' : undefined}
      // Focusable only when it has something to say on focus.
      {...(runnerUp && !user ? { tabIndex: 0 } : {})}
    >
      {t(ruleKey(kind))}
    </span>
  );
  if (!runnerUp || user) return chip;
  return (
    <SimpleTooltip
      content={t('dedupe.rule.runnerUp', {
        name: runnerUp.name,
        rule: t(ruleKey(runnerUp.reason.kind)).toLocaleLowerCase(locale),
      })}
    >
      {chip}
    </SimpleTooltip>
  );
}
