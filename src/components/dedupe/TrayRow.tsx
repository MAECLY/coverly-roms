/**
 * A copy in the tray: "Quedarme con esta" (a radio with a visible label, so
 * it never reads as "mark for removal"), its name, tags and facts, where
 * it goes, what travels with it, what stays behind and what that costs the
 * keeper.
 */
import { TriangleAlert } from 'lucide-react';
import type { ReactNode } from 'react';

import { CopyFacts } from '@/components/dedupe/CopyFacts';
import { CopyNumber } from '@/components/dedupe/KeeperRow';
import { Badge } from '@/components/ui/badge';
import { RadioGroupItem } from '@/components/ui/radio-group';
import { SimpleTooltip } from '@/components/ui/tooltip';
import { useT } from '@/i18n/useT';
import type { MemberView } from '@/ipc/dedupeTypes';
import {
  badgeRelation,
  destLabel,
  destOf,
  fileName,
  relationMeta,
} from '@/lib/dedupe';

/**
 * A relation badge: icon and word; the confidence only in its tooltip (R1).
 * `focusable` makes the tooltip reachable from the keyboard.
 */
export function RelationBadge({
  member,
  focusable = false,
  className,
}: {
  member: MemberView;
  focusable?: boolean;
  className?: string;
}) {
  const t = useT();
  const { icon: Icon, key } = relationMeta(badgeRelation(member));
  return (
    <SimpleTooltip
      content={t('dedupe.confidence', {
        value: member.confidence.toFixed(2),
      })}
    >
      <Badge
        variant="outline"
        className={className}
        tabIndex={focusable ? 0 : undefined}
      >
        <Icon size={14} aria-hidden />
        {t(key)}
      </Badge>
    </SimpleTooltip>
  );
}

export interface TrayRowProps {
  member: MemberView;
  /** Everyone in the group, to name who keeps a shared save. */
  members: MemberView[];
  /** The copy on the shelf now. */
  keeper: MemberView;
  /** A destination (and a radio); `false` for report-only groups. */
  interactive: boolean;
  /** The radio to keep this copy instead (default: `interactive`). */
  pickable?: boolean;
  /** Show this row's own relation badge (when the tray mixes relations). */
  showRelation: boolean;
  quarantine: string;
  /** The group's system, for a former keeper's destination fallback. */
  system: string;
  /** 1-based place in the group's stable order; shown up to 9. */
  copyNumber?: number | undefined;
  /** The copy's own thumbnail, when its art differs from the cover. */
  art?: ReactNode;
  idPrefix: string;
}

function Warning({ children }: { children: string }) {
  return (
    <p className="dup-row__detail dup-row__warn">
      <TriangleAlert size={14} aria-hidden />
      <span>{children}</span>
    </p>
  );
}

export function TrayRow({
  member,
  members,
  keeper,
  interactive,
  pickable = interactive,
  showRelation,
  quarantine,
  system,
  copyNumber,
  art,
  idPrefix,
}: TrayRowProps) {
  const t = useT();
  const name = fileName(member.unit.primary);
  const keepId = `${idPrefix}-keep`;
  const nameId = `${idPrefix}-name`;
  const destId = `${idPrefix}-dest`;
  const folder = fileName(quarantine);
  const keeperName = fileName(keeper.unit.primary);

  const covers = member.travels.filter(
    (tr) => tr.kind.kind === 'art' && tr.kind.media === 'cover',
  ).length;
  const saves = member.travels.filter((tr) => tr.kind.kind === 'save');
  const travels = [
    covers === 1
      ? t('dedupe.travels.artOne')
      : covers > 1
        ? t('dedupe.travels.art', { count: covers })
        : null,
    saves.length === 1
      ? t('dedupe.travels.savesOne')
      : saves.length > 1
        ? t('dedupe.travels.saves', { count: saves.length })
        : null,
  ].filter((s): s is string => s !== null);
  const coverLeaves = member.art_leaves?.media.includes('cover') ?? false;

  return (
    <div className="dup-row">
      <span className="dup-row__pick">
        <CopyNumber n={copyNumber} />
        {pickable && (
          <>
            <RadioGroupItem
              id={keepId + '-radio'}
              value={String(member.id)}
              aria-labelledby={`${keepId} ${nameId}`}
              aria-describedby={destId}
            />
            <label
              id={keepId}
              htmlFor={keepId + '-radio'}
              className="dup-row__keep"
            >
              {t('dedupe.keepThis')}
            </label>
          </>
        )}
      </span>
      <span id={nameId} className="dup-row__name">
        {name}
      </span>
      <span className="dup-row__meta">
        {art}
        {showRelation && <RelationBadge member={member} />}
      </span>
      <div className="dup-row__facts">
        <CopyFacts member={member} />
      </div>
      {interactive ? (
        <p className="dup-row__detail">
          <span id={destId} className="dup-row__dest">
            {t('dedupe.dest', {
              path: destLabel(
                destOf(member, quarantine, members, system),
                quarantine,
              ),
            })}
          </span>
          {travels.length > 0 && <span>{travels.join(', ')}</span>}
        </p>
      ) : (
        <p className="dup-row__detail">{t('dedupe.alsoStays')}</p>
      )}
      {member.stays.map((s) => {
        const owner = members.find((m) => m.id === s.with);
        return (
          <p key={s.path} className="dup-row__detail">
            {t('dedupe.sharedSaveStays', {
              save: fileName(s.path),
              keeper: owner ? fileName(owner.unit.primary) : keeperName,
            })}
          </p>
        );
      })}
      {interactive &&
        saves.map((s) => (
          <Warning key={s.from}>
            {t('dedupe.saveLeaves', { save: fileName(s.from), folder })}
          </Warning>
        ))}
      {interactive && coverLeaves && (
        <Warning>{t('dedupe.artLeaves', { keeper: keeperName })}</Warning>
      )}
    </div>
  );
}
