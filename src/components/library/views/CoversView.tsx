import { useVirtualizer } from '@tanstack/react-virtual';
import { useCallback, useMemo, type MouseEvent } from 'react';

import { useActiveRange, stickyHeaderAt } from '@/hooks/useActiveRange';
import { useElementSize } from '@/hooks/useElementSize';
import { useT } from '@/i18n/useT';
import { COVER_GAP, COVER_SIZES, columnsFor } from '@/lib/coverSizes';
import { nextIndex } from '@/lib/keyboardNav';
import { buildRowModel, coveringSections, moveVertical } from '@/lib/rowModel';

import { CoverTile } from './CoverTile';
import { SECTION_ROW_H, SectionHeaderRow } from './SectionHeaderRow';
import { entryIdOf, type ViewProps } from './types';
import { useViewKeys } from './useViewKeys';

// Tile metrics per size stop are in `COVER_SIZES` (src/lib/coverSizes.ts).
const GAP = COVER_GAP;

const VERTICAL: Readonly<Record<string, 1 | -1>> = {
  ArrowDown: 1,
  ArrowUp: -1,
  PageDown: 1,
  PageUp: -1,
};

/** Covers (⌘1): the cover grid, in rows of tiles under section headers. */
export function CoversView({
  entries,
  sections,
  prefs,
  systems,
  generation,
  selectedId,
  onSelect,
  onOpen,
  onPreview,
}: ViewProps) {
  const t = useT();
  const [ref, size, element] = useElementSize<HTMLDivElement>();
  const stop = prefs.coverSize;
  const metrics = COVER_SIZES[stop];
  const rowH = metrics.tileH + GAP;
  const columns = columnsFor(size.width, stop);
  const pageRows = Math.max(1, Math.floor(size.height / rowH));
  const withHeaders = prefs.groupBy !== 'none';
  const model = useMemo(
    () =>
      buildRowModel(
        coveringSections(sections, entries.length),
        columns,
        withHeaders,
      ),
    [sections, entries.length, columns, withHeaders],
  );
  const { rows, rowOfIndex, headerRows } = model;
  const indexOf = useMemo(
    () => new Map(entries.map((e, i) => [e.id, i])),
    [entries],
  );
  const selectedIndex =
    selectedId === null ? -1 : (indexOf.get(selectedId) ?? -1);
  const activeRow = selectedIndex < 0 ? -1 : (rowOfIndex[selectedIndex] ?? -1);
  const rangeExtractor = useActiveRange(
    activeRow,
    activeRow < 0 ? null : stickyHeaderAt(headerRows, activeRow),
    headerRows,
  );
  // Row identity, not position: TanStack Virtual memoises offsets on
  // `count` and `getItemKey` only, so a new row model with the same row
  // count (another filter, another grouping, another size stop) must hand
  // it a new key function or the old header/tile offsets stay.
  const getItemKey = useCallback(
    (i: number) => {
      const row = rows[i];
      if (!row) return i;
      return row.kind === 'header'
        ? `h:${row.section.key}`
        : `r:${stop}:${row.sectionIndex}:${row.start}`;
    },
    [rows, stop],
  );
  // eslint-disable-next-line react-hooks/incompatible-library -- TanStack Virtual is not memoisable by the React Compiler
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => element.current,
    estimateSize: (i) => (rows[i]?.kind === 'header' ? SECTION_ROW_H : rowH),
    overscan: 2,
    getItemKey,
    rangeExtractor,
    // A row scrolled to never hides under the stuck header.
    scrollPaddingStart: headerRows.length > 0 ? SECTION_ROW_H : 0,
    initialRect: size,
  });
  const stuck = stickyHeaderAt(headerRows, virtualizer.range?.startIndex ?? 0);

  const onKeyDown = useViewKeys({
    entries,
    sections,
    selectedIndex,
    onSelect,
    onOpen,
    onPreview,
    scrollToIndex: (i) =>
      virtualizer.scrollToIndex(rowOfIndex[i] ?? 0, { align: 'auto' }),
    layout: `${size.width}x${size.height}`,
    move: (key) => {
      const dir = VERTICAL[key];
      return dir !== undefined && selectedIndex >= 0
        ? moveVertical(
            model,
            selectedIndex,
            key.startsWith('Page') ? dir * pageRows : dir,
          )
        : nextIndex(key, selectedIndex, entries.length, 1, pageRows);
    },
  });

  function onClick(ev: MouseEvent<HTMLDivElement>) {
    const id = entryIdOf(ev.target);
    if (id !== null) onOpen(id);
  }

  return (
    <div
      ref={ref}
      role="grid"
      aria-label={t('layouts.announce', { view: t('layouts.covers') })}
      data-view-root=""
      aria-rowcount={rows.length}
      aria-colcount={columns}
      aria-activedescendant={
        selectedIndex < 0 || selectedId === null
          ? undefined
          : `entry-${selectedId}`
      }
      tabIndex={0}
      className="cover-grid"
      onKeyDown={onKeyDown}
      onClick={onClick}
    >
      <div style={{ height: virtualizer.getTotalSize(), position: 'relative' }}>
        {virtualizer.getVirtualItems().map((item) => {
          const row = rows[item.index];
          if (!row) return null;
          if (row.kind === 'header') {
            const sticky = item.index === stuck;
            return (
              <SectionHeaderRow
                key={item.key}
                section={row.section}
                colSpan={columns}
                rowIndex={item.index}
                sticky={sticky}
                style={
                  sticky
                    ? { height: SECTION_ROW_H }
                    : {
                        transform: `translateY(${item.start}px)`,
                        height: SECTION_ROW_H,
                      }
                }
              />
            );
          }
          return (
            <div
              key={item.key}
              role="row"
              aria-rowindex={item.index + 1}
              className="cover-row"
              style={{
                transform: `translateY(${item.start}px)`,
                height: rowH,
                gap: GAP,
                paddingLeft: GAP,
              }}
            >
              {entries.slice(row.start, row.end).map((entry) => (
                <CoverTile
                  key={entry.id}
                  entry={entry}
                  system={systems[entry.system]}
                  generation={generation}
                  art={prefs.art}
                  size={stop}
                  selected={entry.id === selectedId}
                />
              ))}
            </div>
          );
        })}
      </div>
    </div>
  );
}
