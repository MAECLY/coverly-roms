import { useVirtualizer } from '@tanstack/react-virtual';
import { ArrowDown, ArrowUp, ArrowUpDown } from 'lucide-react';
import { useContext, useMemo, type MouseEvent } from 'react';

import { SystemLabel } from '@/components/system/SystemLabel';
import { Button } from '@/components/ui/button';
import { useActiveRange } from '@/hooks/useActiveRange';
import { useElementSize } from '@/hooks/useElementSize';
import { LocaleContext } from '@/i18n/core';
import { useT } from '@/i18n/useT';
import { MEDIA } from '@/ipc/types';
import { formatDate, formatSize } from '@/lib/format';
import type { Section } from '@/lib/grouping';
import { nextIndex } from '@/lib/keyboardNav';
import type { Sort, SortKey } from '@/lib/sort';

import { ArtStatusIcon } from './ArtStatusIcon';
import { entryIdOf, type EntryListProps } from './views/types';
import { useViewKeys } from './views/useViewKeys';

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
  /** Section ranges into `entries`, for ⌥↑/⌥↓; none means one group. */
  sections?: readonly Section[];
  /** Space: quick look at the active entry. */
  onPreview?(id: number): void;
}

const NO_SECTIONS: readonly Section[] = [];
const noPreview = () => undefined;

export function EntryTable({
  entries,
  systems,
  selectedId,
  onSelect,
  onOpen,
  sort,
  onSort,
  sections = NO_SECTIONS,
  onPreview = noPreview,
}: EntryTableProps) {
  const t = useT();
  const locale = useContext(LocaleContext);
  const [ref, size, element] = useElementSize<HTMLDivElement>();
  const pageRows = Math.max(1, Math.floor(size.height / ROW_H) - 1);
  const indexOf = useMemo(
    () => new Map(entries.map((e, i) => [e.id, i])),
    [entries],
  );
  const selectedIndex =
    selectedId === null ? -1 : (indexOf.get(selectedId) ?? -1);
  // The selected row stays mounted when the wheel scrolls it away, so
  // aria-activedescendant always names an element that exists.
  const rangeExtractor = useActiveRange(selectedIndex, null);
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
    rangeExtractor,
    initialRect: size,
  });

  const onKeyDown = useViewKeys({
    entries,
    sections,
    selectedIndex,
    onSelect,
    onOpen,
    onPreview,
    scrollToIndex: (i) => virtualizer.scrollToIndex(i, { align: 'auto' }),
    move: (key) => nextIndex(key, selectedIndex, entries.length, 1, pageRows),
  });

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
        selectedIndex < 0 || selectedId === null
          ? undefined
          : `entry-${selectedId}`
      }
      tabIndex={0}
      className="entry-table"
      onKeyDown={onKeyDown}
      onClick={onClick}
    >
      <div role="row" aria-rowindex={1} className="entry-row entry-row--head">
        {COLUMNS.map((c) => {
          const SortIcon =
            sort.key !== c.key
              ? ArrowUpDown
              : sort.dir === 'asc'
                ? ArrowUp
                : ArrowDown;
          return (
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
              <Button
                variant="ghost"
                size="sm"
                className="-ml-2.5 gap-1 font-semibold text-inherit"
                onClick={() => onSort(c.key)}
              >
                {t(c.label)}
                <SortIcon
                  aria-hidden
                  className={
                    sort.key === c.key ? 'size-3.5' : 'size-3.5 opacity-40'
                  }
                />
              </Button>
            </div>
          );
        })}
        {MEDIA.map((m) => (
          <div
            key={m}
            role="columnheader"
            className="cell cell--mark"
            title={t(`art.media.${m}`)}
          >
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
                <SystemLabel
                  systemId={systems[e.system]?.system_id ?? null}
                  name={systems[e.system]?.name ?? ''}
                  size={16}
                />
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
