import { useVirtualizer } from '@tanstack/react-virtual';
import {
  useCallback,
  useContext,
  useMemo,
  type CSSProperties,
  type MouseEvent,
} from 'react';

import { SystemIcon } from '@/components/system/SystemIcon';
import { SystemLabel } from '@/components/system/SystemLabel';
import { Badge } from '@/components/ui/badge';
import { useActiveRange, stickyHeaderAt } from '@/hooks/useActiveRange';
import { useElementSize } from '@/hooks/useElementSize';
import { LocaleContext } from '@/i18n/core';
import { useT } from '@/i18n/useT';
import { COVER_GAP, COVER_SIZES, columnsFor } from '@/lib/coverSizes';
import { formatSize } from '@/lib/format';
import { nextIndex } from '@/lib/keyboardNav';
import { buildRowModel, coveringSections, moveVertical } from '@/lib/rowModel';
import { REGION_CODE, titleWithoutRegions } from '@/lib/tileLabel';

import { ArtStatusIcon } from '../ArtStatusIcon';
import { ArtThumb } from '../ArtThumb';
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
  const locale = useContext(LocaleContext);
  const [ref, size, element] = useElementSize<HTMLDivElement>();
  const stop = prefs.coverSize;
  const metrics = COVER_SIZES[stop];
  const badge = metrics.meta === 'badge';
  const rowH = metrics.tileH + GAP;
  const columns = columnsFor(size.width, stop);
  const pageRows = Math.max(1, Math.floor(size.height / rowH));
  const tileStyle = {
    width: metrics.tileW,
    height: metrics.tileH,
    '--cover-art-h': `${metrics.artH}px`,
  } as CSSProperties;
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
      aria-label={t('library.grid.label')}
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
              {entries.slice(row.start, row.end).map((entry) => {
                const system = systems[entry.system];
                const systemName = system?.name ?? '';
                return (
                  <div
                    key={entry.id}
                    id={`entry-${entry.id}`}
                    role="gridcell"
                    aria-selected={entry.id === selectedId}
                    data-entry-id={entry.id}
                    className={`cover-tile cover-tile--${stop}`}
                    style={tileStyle}
                    title={entry.name}
                  >
                    {metrics.nameLines === 0 && (
                      // No name line: the cell is still named by the game
                      // first, then the system badge and the art marks.
                      <span className="sr-only">{entry.name}</span>
                    )}
                    <div className="cover-tile__art">
                      <ArtThumb
                        generation={generation}
                        entry={entry}
                        media={prefs.art}
                        // An empty slot with no name below shows the name.
                        placeholder={
                          metrics.nameLines === 0
                            ? titleWithoutRegions(entry.name)
                            : undefined
                        }
                      />
                      {badge && (
                        <span className="cover-tile__badge">
                          <SystemIcon
                            systemId={system?.system_id ?? null}
                            size={20}
                            title={systemName}
                          />
                        </span>
                      )}
                      <span className="cover-tile__marks">
                        <ArtStatusIcon media="cover" mark={entry.art.cover} />
                        <ArtStatusIcon
                          media="screenshot"
                          mark={entry.art.screenshot}
                        />
                        <ArtStatusIcon media="title" mark={entry.art.title} />
                      </span>
                    </div>
                    {metrics.nameLines > 0 && (
                      <span className="cover-tile__name">
                        {titleWithoutRegions(entry.name)}
                      </span>
                    )}
                    {!badge && (
                      <span className="cover-tile__info">
                        <span className="cover-tile__system">
                          <SystemLabel
                            systemId={system?.system_id ?? null}
                            name={systemName}
                            size={16}
                          />
                        </span>
                        {entry.regions.slice(0, 2).map((r) => (
                          <Badge
                            key={r}
                            variant="outline"
                            className="cover-tile__region"
                            title={t(`regions.${r}`)}
                          >
                            {r === 'world'
                              ? t('regions.world')
                              : REGION_CODE[r]}
                          </Badge>
                        ))}
                        {entry.regions.length > 2 && (
                          <Badge
                            variant="outline"
                            className="cover-tile__region"
                            title={entry.regions
                              .slice(2)
                              .map((r) => t(`regions.${r}`))
                              .join(', ')}
                          >
                            +{entry.regions.length - 2}
                          </Badge>
                        )}
                      </span>
                    )}
                    {metrics.meta === 'line+size' && (
                      <span className="cover-tile__size">
                        {formatSize(entry.size, locale)}
                      </span>
                    )}
                  </div>
                );
              })}
            </div>
          );
        })}
      </div>
    </div>
  );
}
