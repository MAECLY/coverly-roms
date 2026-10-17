import { useState } from 'react';

import { useT } from '@/i18n/useT';
import type { OrganizeItem, SystemInfo } from '@/ipc/types';
import { ROWS_PER_GROUP, leftReasonText } from '@/lib/organize';

import type { OrganizeChoice } from './ReviewBuckets';

interface Props {
  items: OrganizeItem[];
  /** Every known system, by name: an unknown file can still be rescued. */
  systems: SystemInfo[];
  otherFiles: number;
  deepFolders: number;
  disabled?: boolean;
  onDecide(itemIds: number[], choice: OrganizeChoice): void;
}

/** What stays where it is, with the reason (spec §4.2.5). */
export function LeftInPlace({
  items,
  systems,
  otherFiles,
  deepFolders,
  disabled = false,
  onDecide,
}: Props) {
  const t = useT();
  const [visible, setVisible] = useState(ROWS_PER_GROUP);
  const rest = Math.max(0, items.length - visible);
  if (items.length === 0 && otherFiles === 0 && deepFolders === 0) return null;
  return (
    <section aria-labelledby="organize-left-title" className="recent">
      <h2 id="organize-left-title" className="section-title">
        {t('organize.left.title')}
      </h2>
      {items.length > 0 && (
        <ul className="recent-list">
          {items.slice(0, visible).map((item) => {
            const name = item.sources[0] ?? '';
            return (
              <li key={item.id}>
                <span>{name}</span>
                {item.action === 'left' && (
                  <p className="hint">{leftReasonText(t, item.reason)}</p>
                )}
                {item.action === 'left' && item.reason.kind === 'unknown' && (
                  <select
                    className="select"
                    aria-label={t('organize.review.systemFor', { name })}
                    value=""
                    disabled={disabled}
                    onChange={(e) => {
                      if (e.target.value)
                        onDecide([item.id], { system: e.target.value });
                    }}
                  >
                    <option value="">{t('organize.review.choose')}</option>
                    {systems.map((s) => (
                      <option key={s.id} value={s.id}>
                        {s.name}
                      </option>
                    ))}
                  </select>
                )}
              </li>
            );
          })}
        </ul>
      )}
      {rest > 0 && (
        <button
          type="button"
          className="ghost-button"
          onClick={() => setVisible((v) => v + ROWS_PER_GROUP)}
        >
          {t('organize.showMore', { count: rest })}
        </button>
      )}
      {otherFiles > 0 && (
        <p className="hint">
          {t('organize.otherFiles', { count: otherFiles })}
        </p>
      )}
      {deepFolders > 0 && (
        <p className="hint">
          {t('organize.deepFolders', { count: deepFolders })}
        </p>
      )}
    </section>
  );
}
