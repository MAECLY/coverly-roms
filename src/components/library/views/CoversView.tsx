import { useVirtualizer } from '@tanstack/react-virtual';
import {
  useCallback,
  useMemo,
  type KeyboardEvent,
  type MouseEvent,
} from 'react';

import { SystemLabel } from '@/components/system/SystemLabel';
import { Badge } from '@/components/ui/badge';
import { useActiveRange, stickyHeaderAt } from '@/hooks/useActiveRange';
import { useElementSize } from '@/hooks/useElementSize';
import { useT } from '@/i18n/useT';
import { ALL_SECTION_KEY, type Section } from '@/lib/grouping';
import { nextIndex } from '@/lib/keyboardNav';
import { buildRowModel, moveVertical } from '@/lib/rowModel';
import { REGION_CODE, titleWithoutRegions } from '@/lib/tileLabel';

import { ArtStatusIcon } from '../ArtStatusIcon';
import { ArtThumb } from '../ArtThumb';
import { SECTION_ROW_H, SectionHeaderRow } from './SectionHeaderRow';
import { entryIdOf, type ViewProps } from './types';

export const TILE_W = 168;
// Fixed parts, so a tile never grows into the row below (see .cover-tile):
// 2px border + 6px padding each side, the 25:36 cover (152 x 219), a 6px gap,
// two 16px lines of title and the 18px system/region line.
export const TILE_H = 296;
export const GAP = 12;

const VERTICAL: Readonly<Record<string, 1 | -1>> = {
  ArrowDown: 1,
  ArrowUp: -1,
  PageDown: 1,
  PageUp: -1,
};

/** Sections that cover `count` entries exactly, or one section for all. */
function coveringSections(
  sections: readonly Section[],
  count: number,
): readonly Section[] {
  let next = 0;
  for (const s of sections) {
    if (s.start !== next) break;
    next += s.count;
  }
  return sections.length > 0 && next === count
    ? sections
    : [{ key: ALL_SECTION_KEY, label: '', start: 0, count }];
}

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
}: ViewProps) {
  const t = useT();
  const [ref, size, element] = useElementSize<HTMLDivElement>();
  const columns = Math.max(1, Math.floor((size.width - GAP) / (TILE_W + GAP)));
  const pageRows = Math.max(1, Math.floor(size.height / (TILE_H + GAP)));
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
  // count (another filter, another grouping) must hand it a new key function
  // or the old header/tile offsets stay.
  const getItemKey = useCallback(
    (i: number) => {
      const row = rows[i];
      if (!row) return i;
      return row.kind === 'header'
        ? `h:${row.section.key}`
        : `r:${row.sectionIndex}:${row.start}`;
    },
    [rows],
  );
  // eslint-disable-next-line react-hooks/incompatible-library -- TanStack Virtual is not memoisable by the React Compiler
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => element.current,
    estimateSize: (i) =>
      rows[i]?.kind === 'header' ? SECTION_ROW_H : TILE_H + GAP,
    overscan: 2,
    getItemKey,
    rangeExtractor,
    // A row scrolled to never hides under the stuck header.
    scrollPaddingStart: headerRows.length > 0 ? SECTION_ROW_H : 0,
    initialRect: size,
  });
  const stuck = stickyHeaderAt(headerRows, virtualizer.range?.startIndex ?? 0);

  function onKeyDown(ev: KeyboardEvent<HTMLDivElement>) {
    // Keys pressed on a focusable child belong to that child.
    if (ev.target !== ev.currentTarget) return;
    if (ev.key === 'Enter') {
      if (selectedId !== null) {
        ev.preventDefault();
        onOpen(selectedId);
      }
      return;
    }
    const dir = VERTICAL[ev.key];
    const next =
      dir !== undefined && selectedIndex >= 0
        ? moveVertical(
            model,
            selectedIndex,
            ev.key.startsWith('Page') ? dir * pageRows : dir,
          )
        : nextIndex(ev.key, selectedIndex, entries.length, 1, pageRows);
    const entry = next === null ? undefined : entries[next];
    if (next === null || !entry) return;
    ev.preventDefault();
    onSelect(entry.id);
    virtualizer.scrollToIndex(rowOfIndex[next] ?? 0, { align: 'auto' });
  }

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
                height: TILE_H + GAP,
                gap: GAP,
                paddingLeft: GAP,
              }}
            >
              {entries.slice(row.start, row.end).map((entry) => (
                <div
                  key={entry.id}
                  id={`entry-${entry.id}`}
                  role="gridcell"
                  aria-selected={entry.id === selectedId}
                  data-entry-id={entry.id}
                  className="cover-tile"
                  style={{ width: TILE_W, height: TILE_H }}
                  title={entry.name}
                >
                  <div className="cover-tile__art">
                    <ArtThumb
                      generation={generation}
                      entry={entry}
                      media="cover"
                    />
                    <span className="cover-tile__marks">
                      <ArtStatusIcon media="cover" mark={entry.art.cover} />
                      <ArtStatusIcon
                        media="screenshot"
                        mark={entry.art.screenshot}
                      />
                      <ArtStatusIcon media="title" mark={entry.art.title} />
                    </span>
                  </div>
                  <span className="cover-tile__name">
                    {titleWithoutRegions(entry.name)}
                  </span>
                  <span className="cover-tile__info">
                    <span className="cover-tile__system">
                      <SystemLabel
                        systemId={systems[entry.system]?.system_id ?? null}
                        name={systems[entry.system]?.name ?? ''}
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
                        {r === 'world' ? t('regions.world') : REGION_CODE[r]}
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
                </div>
              ))}
            </div>
          );
        })}
      </div>
    </div>
  );
}
