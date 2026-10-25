import { RotateCw } from 'lucide-react';
import { useState } from 'react';

import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';
import { NativeSelect } from '@/components/ui/select';
import { useT } from '@/i18n/useT';
import type { OrganizeItem, SystemInfo } from '@/ipc/types';
import { ROWS_PER_GROUP, leftReasonText } from '@/lib/organize';

import { NewBadge, newRow, type IsNew } from './NewMarks';
import type { OrganizeChoice } from './ReviewBuckets';
import {
  fileList,
  fileRow,
  hintText,
  listViewport,
  panel,
  pathText,
  sectionTitle,
} from './styles';

interface Props {
  items: OrganizeItem[];
  /** Every known system, by name: an unknown file can still be rescued. */
  systems: SystemInfo[];
  otherFiles: number;
  deepFolders: number;
  disabled?: boolean;
  onDecide(itemIds: number[], choice: OrganizeChoice): void;
  /** Files the last "scan again" found. */
  isNew?: IsNew;
  /** Reads the folder again (the screen's ⌘R): offered next to an archive
   * that a program the user can install would open. */
  onRescan?(): void;
}

/** An archive that a known program would open, once installed. */
const needsTool = (item: OrganizeItem) =>
  item.action === 'left' &&
  item.reason.kind === 'unsupported-archive' &&
  Boolean(item.reason.need);

/** What stays where it is, with the reason (spec §4.2.5). */
export function LeftInPlace({
  items,
  systems,
  otherFiles,
  deepFolders,
  disabled = false,
  onDecide,
  isNew,
  onRescan,
}: Props) {
  const t = useT();
  const [visible, setVisible] = useState(ROWS_PER_GROUP);
  const rest = Math.max(0, items.length - visible);
  if (items.length === 0 && otherFiles === 0 && deepFolders === 0) return null;
  return (
    <section aria-labelledby="organize-left-title" className="grid gap-2.5">
      <h2 id="organize-left-title" className={sectionTitle}>
        {t('organize.left.title')}
      </h2>
      {items.length > 0 && (
        <ScrollArea
          aria-label={t('organize.left.files')}
          className={panel}
          viewportClassName={listViewport}
        >
          <ul className={fileList}>
            {items.slice(0, visible).map((item) => {
              const name = item.sources[0] ?? '';
              const fresh = isNew?.(item) ?? false;
              return (
                <li
                  key={item.id}
                  className={fresh ? `${fileRow} ${newRow}` : fileRow}
                >
                  <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
                    <span className={`${pathText} min-w-0 flex-1`}>{name}</span>
                    {fresh && <NewBadge />}
                    {item.action === 'left' &&
                      item.reason.kind === 'unknown' && (
                        <NativeSelect
                          size="sm"
                          className="w-auto max-w-[16rem]"
                          aria-label={t('organize.review.systemFor', { name })}
                          value=""
                          disabled={disabled}
                          onChange={(e) => {
                            if (e.target.value)
                              onDecide([item.id], { system: e.target.value });
                          }}
                        >
                          <option value="">
                            {t('organize.review.choose')}
                          </option>
                          {systems.map((s) => (
                            <option key={s.id} value={s.id}>
                              {s.name}
                            </option>
                          ))}
                        </NativeSelect>
                      )}
                  </div>
                  {item.action === 'left' &&
                    (onRescan && needsTool(item) ? (
                      <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
                        <p className={hintText}>
                          {leftReasonText(t, item.reason)}
                        </p>
                        <Button
                          variant="outline"
                          size="sm"
                          disabled={disabled}
                          aria-keyshortcuts="Meta+R Control+R"
                          onClick={onRescan}
                        >
                          <RotateCw aria-hidden />
                          {t('organize.refresh.button')}
                        </Button>
                      </div>
                    ) : (
                      <p className={hintText}>
                        {leftReasonText(t, item.reason)}
                      </p>
                    ))}
                </li>
              );
            })}
          </ul>
          {rest > 0 && (
            <div className="border-t border-cr-border px-3 py-2">
              <Button
                variant="ghost"
                size="sm"
                onClick={() => setVisible((v) => v + ROWS_PER_GROUP)}
              >
                {t('organize.showMore', { count: rest })}
              </Button>
            </div>
          )}
        </ScrollArea>
      )}
      {otherFiles > 0 && (
        <p className={hintText}>
          {t('organize.otherFiles', { count: otherFiles })}
        </p>
      )}
      {deepFolders > 0 && (
        <p className={hintText}>
          {t('organize.deepFolders', { count: deepFolders })}
        </p>
      )}
    </section>
  );
}
