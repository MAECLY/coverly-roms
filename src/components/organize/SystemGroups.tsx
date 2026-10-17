import { useState } from 'react';

import { useT } from '@/i18n/useT';
import type { OrganizeItem } from '@/ipc/types';
import {
  ROWS_PER_GROUP,
  confidenceKey,
  itemNotes,
  targetDir,
  whyKey,
  type SystemGroup,
} from '@/lib/organize';

interface Props {
  groups: SystemGroup[];
  /** Display name of the target layout (for "no folder on device"). */
  layoutName: string;
  systemName(id: string): string;
}

/** The items that move, one `<details>` per system folder (spec §4.2.3). */
export function SystemGroups({ groups, layoutName, systemName }: Props) {
  const total = groups.reduce((n, g) => n + g.items.length, 0);
  // Large piles start closed so thousands of rows never reach the DOM at once.
  const startOpen = total <= ROWS_PER_GROUP;
  return (
    <div className="flex flex-col gap-2">
      {groups.map((g) => (
        <Group
          key={g.folder}
          group={g}
          startOpen={startOpen}
          layoutName={layoutName}
          systemName={systemName}
        />
      ))}
    </div>
  );
}

function Group({
  group,
  startOpen,
  layoutName,
  systemName,
}: {
  group: SystemGroup;
  startOpen: boolean;
  layoutName: string;
  systemName(id: string): string;
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
        <span className="hint">{systemName(group.systemId)}</span>
      </summary>
      {open && (
        <>
          <ul className="recent-list">
            {rows.map((item) => (
              <Row
                key={item.id}
                item={item}
                layoutName={layoutName}
                systemName={systemName}
              />
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
}: {
  item: OrganizeItem;
  layoutName: string;
  systemName(id: string): string;
}) {
  const t = useT();
  const confidence = confidenceKey(item.confidence);
  const why = item.why[0];
  const reason = why ? whyKey(why, systemName) : null;
  return (
    <li>
      <span>{`${item.sources[0] ?? ''} → ${item.target ?? ''}`}</span>
      {confidence && (
        <>
          {' · '}
          <span>{t(confidence)}</span>
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
