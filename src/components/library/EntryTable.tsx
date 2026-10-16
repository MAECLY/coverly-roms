import { useVirtualizer } from '@tanstack/react-virtual';
import {
  useContext,
  useMemo,
  type KeyboardEvent,
  type MouseEvent,
} from 'react';

import { useElementSize } from '@/hooks/useElementSize';
import { LocaleContext } from '@/i18n/core';
import { useT } from '@/i18n/useT';
import { MEDIA } from '@/ipc/types';
import { formatDate, formatSize } from '@/lib/format';
import { nextIndex } from '@/lib/keyboardNav';
import type { Sort, SortKey } from '@/lib/sort';

import { ArtStatusIcon } from './ArtStatusIcon';
import { entryIdOf, type EntryListProps } from './CoverGrid';

export const ROW_H = 34;

const COLUMNS: readonly { key: SortKey; label: string }[] = [
  { key: 'name', label: 'columns.name' },
  { key: 'system', label: 'columns.system' },
  { key: 'size', label: 'columns.size' },
  { key: 'mtime', label: 'columns.modified' },
];

export interface EntryTableProps extends EntryListProps {
  sort: Sort;
  onSort(key: SortKey): void;
}

export function EntryTable({
  entries,
  systems,
  selectedId,
  onSelect,
  onOpen,
  sort,
  onSort,
}: EntryTableProps) {
  const t = useT();
  const locale = useContext(LocaleContext);
  const [ref, size, element] = useElementSize<HTMLDivElement>();
  const pageRows = Math.max(1, Math.floor(size.height / ROW_H) - 1);
  // eslint-disable-next-line react-hooks/incompatible-library -- TanStack Virtual is not memoisable by the React Compiler
  const virtualizer = useVirtualizer({
    count: entries.length,
    getScrollElement: () => element.current,
    estimateSize: () => ROW_H,
    overscan: 8,
    // The sticky header row sits inside the scroll element above the list:
    // the list starts ROW_H down, and rows hidden behind the header are not
    // visible.
    scrollMargin: ROW_H,
    scrollPaddingStart: ROW_H,
    initialRect: size,
  });
  const indexOf = useMemo(
    () => new Map(entries.map((e, i) => [e.id, i])),
    [entries],
  );
  const selectedIndex =
    selectedId === null ? -1 : (indexOf.get(selectedId) ?? -1);

  function onKeyDown(ev: KeyboardEvent<HTMLDivElement>) {
    // Keys pressed on a header sort button belong to that button.
    if (ev.target !== ev.currentTarget) return;
    if (ev.key === 'Enter') {
      if (selectedId !== null) {
        ev.preventDefault();
        onOpen(selectedId);
      }
      return;
    }
    const next = nextIndex(ev.key, selectedIndex, entries.length, 1, pageRows);
    const entry = next === null ? undefined : entries[next];
    if (next === null || !entry) return;
    ev.preventDefault();
    onSelect(entry.id);
    virtualizer.scrollToIndex(next, { align: 'auto' });
  }

  function onClick(ev: MouseEvent<HTMLDivElement>) {
    const id = entryIdOf(ev.target);
    if (id !== null) onOpen(id);
  }

  return (
    <div
      ref={ref}
      role="grid"
      aria-label={t('library.table.label')}
      aria-rowcount={entries.length + 1}
      aria-activedescendant={
        selectedId === null ? undefined : `entry-${selectedId}`
      }
      tabIndex={0}
      className="entry-table"
      onKeyDown={onKeyDown}
      onClick={onClick}
    >
      <div role="row" aria-rowindex={1} className="entry-row entry-row--head">
        {COLUMNS.map((c) => (
          <div
            key={c.key}
            role="columnheader"
            aria-sort={
              sort.key === c.key
                ? sort.dir === 'asc'
                  ? 'ascending'
                  : 'descending'
                : 'none'
            }
            className={`cell cell--${c.key}`}
          >
            <button
              type="button"
              className="sort-button"
              onClick={() => onSort(c.key)}
            >
              {t(c.label)}
            </button>
          </div>
        ))}
        {MEDIA.map((m) => (
          <div key={m} role="columnheader" className="cell cell--mark">
            {t(`art.media.${m}`)}
          </div>
        ))}
      </div>
      <div style={{ height: virtualizer.getTotalSize(), position: 'relative' }}>
        {virtualizer.getVirtualItems().map((item) => {
          const e = entries[item.index];
          if (!e) return null;
          return (
            <div
              key={e.id}
              id={`entry-${e.id}`}
              role="row"
              aria-rowindex={item.index + 2}
              aria-selected={e.id === selectedId}
              data-entry-id={e.id}
              className="entry-row"
              style={{
                transform: `translateY(${item.start - ROW_H}px)`,
                height: ROW_H,
              }}
            >
              <div role="gridcell" className="cell cell--name" title={e.name}>
                {e.name}
              </div>
              <div role="gridcell" className="cell cell--system">
                {systems[e.system]?.name ?? ''}
              </div>
              <div role="gridcell" className="cell cell--size">
                {formatSize(e.size, locale)}
              </div>
              <div role="gridcell" className="cell cell--mtime">
                {formatDate(e.mtime, locale)}
              </div>
              {MEDIA.map((m) => (
                <div key={m} role="gridcell" className="cell cell--mark">
                  <ArtStatusIcon media={m} mark={e.art[m]} />
                </div>
              ))}
            </div>
          );
        })}
      </div>
    </div>
  );
}
