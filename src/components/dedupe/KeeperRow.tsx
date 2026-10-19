/**
 * The copy that stays, on the shelf: its radio (checked), its name, "Se
 * queda" and its size, then the why line, which also describes the radio.
 */
import { useContext } from 'react';

import { Badge } from '@/components/ui/badge';
import { RadioGroupItem } from '@/components/ui/radio-group';
import { useT } from '@/i18n/useT';
import { LocaleContext } from '@/i18n/core';
import type { MemberView } from '@/ipc/dedupeTypes';
import { fileName, formatBytes } from '@/lib/dedupe';

export interface KeeperRowProps {
  member: MemberView;
  /** A radio to pick the keeper; `false` for report-only groups. */
  interactive: boolean;
  /** The why line, already localised. */
  why: string;
  /** "Your earlier pick no longer applies…", when the plan says so. */
  choiceDropped?: boolean;
  idPrefix: string;
}

function stem(name: string): string {
  const dot = name.lastIndexOf('.');
  return dot > 0 ? name.slice(0, dot) : name;
}

export function KeeperRow({
  member,
  interactive,
  why,
  choiceDropped = false,
  idPrefix,
}: KeeperRowProps) {
  const t = useT();
  const locale = useContext(LocaleContext);
  const name = fileName(member.unit.primary);
  const nameId = `${idPrefix}-name`;
  const whyId = `${idPrefix}-why`;
  const catalogue =
    member.catalogue_name && member.catalogue_name !== stem(name)
      ? member.catalogue_name
      : null;

  return (
    <div className="dup-row" data-keeper>
      <span className="dup-row__pick">
        {interactive && (
          <RadioGroupItem
            value={String(member.id)}
            aria-labelledby={nameId}
            aria-describedby={whyId}
          />
        )}
      </span>
      <span id={nameId} className="dup-row__name">
        {name}
      </span>
      <span className="dup-row__meta">
        <Badge variant="primary">{t('dedupe.keeper')}</Badge>
      </span>
      <span className="dup-row__size">{formatBytes(member.size, locale)}</span>
      <p className="dup-row__detail">
        <span id={whyId} className="dup-row__why">
          {why}
        </span>
      </p>
      {catalogue && (
        <p className="dup-row__detail">
          {t('dedupe.catalogueCalls', { name: catalogue })}
        </p>
      )}
      {choiceDropped && (
        <p className="dup-row__detail">{t('dedupe.choiceDropped')}</p>
      )}
    </div>
  );
}
