/**
 * The copy that stays, on the shelf: its number and radio (checked), its
 * name and "Se queda", its tags and facts, then the rule chip and the why
 * line, which also describes the radio.
 */
import type { ReactNode } from 'react';

import { CopyFacts } from '@/components/dedupe/CopyFacts';
import { Badge } from '@/components/ui/badge';
import { RadioGroupItem } from '@/components/ui/radio-group';
import { useT } from '@/i18n/useT';
import type { MemberView } from '@/ipc/dedupeTypes';
import { fileName } from '@/lib/dedupe';

export interface KeeperRowProps {
  member: MemberView;
  /** A radio to pick the keeper; `false` for report-only groups. */
  interactive: boolean;
  /** The why line, already localised. */
  why: string;
  /** "Your earlier pick no longer applies…", when the plan says so. */
  choiceDropped?: boolean;
  /** 1-based place in the group's stable order; shown up to 9. */
  copyNumber?: number | undefined;
  /** The rule chip, before the why line. */
  rule?: ReactNode;
  /** The copy's own thumbnail, when its art differs from the cover. */
  art?: ReactNode;
  idPrefix: string;
}

/** The copy's number, the key that keeps it (1–9); hidden from readers. */
export function CopyNumber({ n }: { n: number | undefined }) {
  const t = useT();
  if (n === undefined || n < 1 || n > 9) return null;
  return (
    <span
      className="dup-row__num"
      aria-hidden
      title={t('dedupe.copyNumber', { n })}
    >
      {n}
    </span>
  );
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
  copyNumber,
  rule,
  art,
  idPrefix,
}: KeeperRowProps) {
  const t = useT();
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
        <CopyNumber n={copyNumber} />
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
        {art}
        <Badge variant="primary">{t('dedupe.keeper')}</Badge>
      </span>
      <div className="dup-row__facts">
        <CopyFacts member={member} />
      </div>
      <p className="dup-row__detail dup-row__rule">
        {rule}
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
