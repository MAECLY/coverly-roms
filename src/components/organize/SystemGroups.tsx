import { useState } from 'react';

import { useT } from '@/i18n/useT';
import type { OrganizeItem, SystemInfo } from '@/ipc/types';
import {
  ROWS_PER_GROUP,
  confidenceKey,
  itemNotes,
  targetDir,
  whyKey,
  type SystemGroup,
} from '@/lib/organize';

import type { OrganizeChoice } from './ReviewBuckets';

const LEAVE = '__leave__';

interface Props {
  groups: SystemGroup[];
  /** Display name of the target layout (for "no folder on device"). */
  layoutName: string;
  systemName(id: string): string;
  /** Every known system: a system the user chose can be changed again. */
  systems?: SystemInfo[];
  disabled?: boolean;
  onDecide?(itemIds: number[], choice: OrganizeChoice): void;
}

interface RowProps {
  layoutName: string;
  systemName(id: string): string;
  systems?: SystemInfo[];
  disabled?: boolean;
  onDecide?(itemIds: number[], choice: OrganizeChoice): void;
}

/** The items that move, one `<details>` per system folder (spec §4.2.3). */
export function SystemGroups({ groups, ...row }: Props) {
  const total = groups.reduce((n, g) => n + g.items.length, 0);
  // Large piles start closed so thousands of rows never reach the DOM at once.
  const startOpen = total <= ROWS_PER_GROUP;
  return (
    <div className="flex flex-col gap-2">
      {groups.map((g) => (
        <Group key={g.folder} group={g} startOpen={startOpen} {...row} />
      ))}
    </div>
  );
}

function Group({
  group,
  startOpen,
  ...row
}: RowProps & {
  group: SystemGroup;
  startOpen: boolean;
}) {
  const t = useT();
  const [open, setOpen] = useState(startOpen);
  const [visible, setVisible] = useState(ROWS_PER_GROUP);
  const rows = group.items.slice(0, visible);
  const rest = group.items.length - rows.length;
  const dir = targetDir(group.items[0]!);
  return (
    <details open={startOpen} onToggle={(e) => setOpen(e.currentTarget.open)}>
      <summary>
        <span>{`${dir}/ (${group.items.length})`}</span>{' '}
        <span className="hint">{row.systemName(group.systemId)}</span>
      </summary>
      {open && (
        <>
          <ul className="recent-list">
            {rows.map((item) => (
              <Row key={item.id} item={item} {...row} />
            ))}
          </ul>
          {rest > 0 && (
            <button
              type="button"
              className="ghost-button"
              onClick={() => setVisible((v) => v + ROWS_PER_GROUP)}
            >
              {t('organize.showMore', { count: rest })}
            </button>
          )}
        </>
      )}
    </details>
  );
}

function Row({
  item,
  layoutName,
  systemName,
  systems,
  disabled = false,
  onDecide,
}: RowProps & { item: OrganizeItem }) {
  const t = useT();
  const confidence = confidenceKey(item.confidence);
  const why = item.why[0];
  const reason = why ? whyKey(why, systemName) : null;
  const name = item.sources[0] ?? '';
  // A system the user chose (now or remembered) stays changeable: a slip in
  // the review list must not need starting over.
  const changeable =
    item.method === 'user' && !!onDecide && !!systems && !!item.system_id;
  return (
    <li>
      <span>{`${name} → ${item.target ?? ''}`}</span>
      {confidence && (
        <>
          {' · '}
          <span>{t(confidence)}</span>
        </>
      )}
      {changeable && (
        <>
          {' '}
          <select
            className="select"
            aria-label={t('organize.review.systemFor', { name })}
            value={item.system_id ?? ''}
            disabled={disabled}
            onChange={(e) => {
              const v = e.target.value;
              if (!v || v === item.system_id) return;
              onDecide([item.id], v === LEAVE ? 'leave' : { system: v });
            }}
          >
            {systems.map((s) => (
              <option key={s.id} value={s.id}>
                {s.name}
              </option>
            ))}
            <option value={LEAVE}>{t('organize.review.leave')}</option>
          </select>
        </>
      )}
      {reason && <p className="hint">{t(reason.key, reason.vars)}</p>}
      {itemNotes(t, item, layoutName, systemName).map((line) => (
        <p key={line} className="hint">
          {line}
        </p>
      ))}
    </li>
  );
}
