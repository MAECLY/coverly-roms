import { useVirtualizer } from '@tanstack/react-virtual';
import { ArrowDown, ArrowUp, ArrowUpDown } from 'lucide-react';
import {
  useCallback,
  useContext,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type KeyboardEvent,
  type MouseEvent,
  type PointerEvent,
  type ReactNode,
} from 'react';

import { SystemLabel } from '@/components/system/SystemLabel';
import { Button } from '@/components/ui/button';
import { stickyHeaderAt, useActiveRange } from '@/hooks/useActiveRange';
import { useElementSize } from '@/hooks/useElementSize';
import { LocaleContext } from '@/i18n/core';
import { useT } from '@/i18n/useT';
import type { EntryView, SystemView } from '@/ipc/types';
import {
  COLUMN_DEFS,
  MAX_COLUMN_WIDTH,
  ROW_HEIGHT,
  fitWidth,
  gridTemplate,
  resizeColumn,
  widthOf,
} from '@/lib/detailsColumns';
import { formatDate, formatSize } from '@/lib/format';
import { relativeDir } from '@/lib/grouping';
import { nextIndex } from '@/lib/keyboardNav';
import { buildRowModel, coveringSections } from '@/lib/rowModel';
import type { Sort, SortKey } from '@/lib/sort';
import { REGION_CODE } from '@/lib/tileLabel';
import { cn } from '@/lib/utils';
import type {
  DetailsColumn,
  DetailsColumnId,
  ViewPrefs,
} from '@/lib/viewPrefs';

import { ArtStatusIcon } from '../ArtStatusIcon';
import { ArtThumb } from '../ArtThumb';
import { ColumnPicker } from './ColumnPicker';
import { SECTION_ROW_H, SectionHeaderRow } from './SectionHeaderRow';
import { entryIdOf, type ViewProps } from './types';
import { useViewKeys } from './useViewKeys';

/** The column header row keeps its height at both densities. */
const HEAD_H = ROW_HEIGHT.comfortable;
/** ←/→ on a focused column edge. */
const KEY_STEP = 8;

export interface DetailsViewProps extends ViewProps {
  sort: Sort;
  onSort(key: SortKey): void;
  /** Column picker, resize: saved with the library's view prefs. */
  setPrefs(patch: Partial<ViewPrefs>): void;
}

interface CellContext {
  t: ReturnType<typeof useT>;
  locale: string;
  systems: readonly SystemView[];
  generation: number;
}

function regionText(e: EntryView, t: CellContext['t']): string {
  return e.regions
    .map((r) => (r === 'world' ? t('regions.world') : REGION_CODE[r]))
    .join(', ');
}

/** What each column shows for an entry. Entry data is React text only. */
const CELLS: Readonly<
  Record<DetailsColumnId, (e: EntryView, ctx: CellContext) => ReactNode>
> = {
  thumb: (e, ctx) => (
    <ArtThumb
      generation={ctx.generation}
      entry={e}
      media="cover"
      className="details-thumb"
      placeholder=""
    />
  ),
  name: (e) => e.name,
  system: (e, ctx) => {
    const system = ctx.systems[e.system];
    return (
      <SystemLabel
        systemId={system?.system_id ?? null}
        name={system?.name ?? ''}
        size={16}
      />
    );
  },
  folder: (e, ctx) => relativeDir(e.dir, ctx.systems[e.system]),
  regions: (e, ctx) => regionText(e, ctx.t),
  ext: (e) => e.ext,
  size: (e, ctx) => formatSize(e.size, ctx.locale),
  mtime: (e, ctx) => formatDate(e.mtime, ctx.locale),
  cover: (e) => <ArtStatusIcon media="cover" mark={e.art.cover} />,
  title: (e) => <ArtStatusIcon media="title" mark={e.art.title} />,
  screenshot: (e) => (
    <ArtStatusIcon media="screenshot" mark={e.art.screenshot} />
  ),
};

/** A plain-text tooltip for cells that can truncate. */
function cellTitle(
  id: DetailsColumnId,
  e: EntryView,
  ctx: CellContext,
): string | undefined {
  switch (id) {
    case 'name':
      return e.name;
    case 'folder':
      return relativeDir(e.dir, ctx.systems[e.system]) || undefined;
    case 'regions':
      return (
        e.regions.map((r) => ctx.t(`regions.${r}`)).join(', ') || undefined
      );
    default:
      return undefined;
  }
}

let measureCanvas: HTMLCanvasElement | null = null;

/** The rendered text width of `text` in `el`'s font. */
function textWidth(text: string, el: Element): number {
  measureCanvas ??= document.createElement('canvas');
  const ctx = measureCanvas.getContext('2d');
  if (!ctx) return text.length * 7;
  ctx.font = getComputedStyle(el).font || '13px sans-serif';
  return ctx.measureText(text).width;
}

/** The widest rendered cell (and header) of a column, in px. */
function widestCell(grid: HTMLElement, id: DetailsColumnId): number {
  let widest = 0;
  for (const cell of grid.querySelectorAll(`.cell--${id}`)) {
    const text = cell.textContent ?? '';
    // A system icon or an art mark sits beside (or instead of) the text.
    const icons = cell.querySelectorAll('svg').length * 24;
    widest = Math.max(widest, textWidth(text, cell) + icons);
  }
  return widest;
}

/** Details (⌘2): one row per game, the columns the user picked. */
export function DetailsView({
  entries,
  sections,
  prefs,
  setPrefs,
  systems,
  generation,
  selectedId,
  onSelect,
  onOpen,
  onPreview,
  sort,
  onSort,
}: DetailsViewProps) {
  const t = useT();
  const locale = useContext(LocaleContext);
  const [ref, size, element] = useElementSize<HTMLDivElement>();
  const compact = prefs.density === 'compact';
  const rowH = ROW_HEIGHT[prefs.density];
  const pageRows = Math.max(1, Math.floor((size.height - HEAD_H) / rowH));
  // While a column edge is dragged, its width lives here and is saved once
  // on release.
  const [draft, setDraft] = useState<DetailsColumn[] | null>(null);
  const columns = draft ?? prefs.columns;
  const [menu, setMenu] = useState<{
    x: number;
    y: number;
    column: DetailsColumnId;
  } | null>(null);
  const menuReturn = useRef<HTMLElement | null>(null);
  const drag = useRef<{
    id: DetailsColumnId;
    x: number;
    width: number;
  } | null>(null);

  const withHeaders = prefs.groupBy !== 'none';
  const model = useMemo(
    () =>
      buildRowModel(coveringSections(sections, entries.length), 1, withHeaders),
    [sections, entries.length, withHeaders],
  );
  const { rows, rowOfIndex, headerRows } = model;
  const indexOf = useMemo(
    () => new Map(entries.map((e, i) => [e.id, i])),
    [entries],
  );
  const selectedIndex =
    selectedId === null ? -1 : (indexOf.get(selectedId) ?? -1);
  const activeRow = selectedIndex < 0 ? -1 : (rowOfIndex[selectedIndex] ?? -1);
  // The selected row (and its group row) stays mounted when the wheel
  // scrolls it away, so aria-activedescendant always names an element.
  const rangeExtractor = useActiveRange(
    activeRow,
    activeRow < 0 ? null : stickyHeaderAt(headerRows, activeRow),
    headerRows,
  );
  const getItemKey = useCallback(
    (i: number) => {
      const row = rows[i];
      if (!row) return i;
      return row.kind === 'header'
        ? `h:${row.section.key}`
        : `r:${prefs.density}:${row.start}`;
    },
    [rows, prefs.density],
  );
  // eslint-disable-next-line react-hooks/incompatible-library -- TanStack Virtual is not memoisable by the React Compiler
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => element.current,
    estimateSize: (i) => (rows[i]?.kind === 'header' ? SECTION_ROW_H : rowH),
    overscan: 8,
    getItemKey,
    // The sticky column header sits inside the scroll element above the
    // list: the list starts HEAD_H down, and rows behind it (or behind a
    // stuck group row) are not visible.
    scrollMargin: HEAD_H,
    scrollPaddingStart: HEAD_H + (headerRows.length > 0 ? SECTION_ROW_H : 0),
    rangeExtractor,
    initialRect: size,
  });
  const items = virtualizer.getVirtualItems();
  // The group row stuck under the column header: the one of the first row
  // actually on screen (not of the overscan above it).
  const offset = virtualizer.scrollOffset ?? 0;
  const topRow = items.find((item) => item.end - HEAD_H > offset)?.index ?? 0;
  const stuck = stickyHeaderAt(headerRows, topRow);

  const onKeyDown = useViewKeys({
    entries,
    sections,
    selectedIndex,
    onSelect,
    onOpen,
    onPreview,
    scrollToIndex: (i) =>
      virtualizer.scrollToIndex(rowOfIndex[i] ?? 0, { align: 'auto' }),
    move: (key) => nextIndex(key, selectedIndex, entries.length, 1, pageRows),
  });

  function onClick(ev: MouseEvent<HTMLDivElement>) {
    const id = entryIdOf(ev.target);
    if (id !== null) onOpen(id);
  }

  function saveColumns(next: DetailsColumn[]) {
    setPrefs({ columns: next });
  }

  function openMenu(
    column: DetailsColumnId,
    x: number,
    y: number,
    from: HTMLElement | null,
  ) {
    menuReturn.current = from ?? element.current;
    setMenu({ x, y, column });
  }

  function onHeaderKeyDown(
    ev: KeyboardEvent<HTMLDivElement>,
    column: DetailsColumnId,
  ) {
    if (
      ev.key === 'ContextMenu' ||
      (ev.key === 'F10' && ev.shiftKey && !ev.ctrlKey && !ev.metaKey)
    ) {
      ev.preventDefault();
      const r = ev.currentTarget.getBoundingClientRect();
      openMenu(column, r.left, r.bottom, ev.target as HTMLElement);
    }
  }

  function onEdgeKeyDown(
    ev: KeyboardEvent<HTMLDivElement>,
    col: DetailsColumn,
  ) {
    const dir = ev.key === 'ArrowRight' ? 1 : ev.key === 'ArrowLeft' ? -1 : 0;
    if (dir === 0) return;
    ev.preventDefault();
    saveColumns(resizeColumn(columns, col.id, widthOf(col) + dir * KEY_STEP));
  }

  function onEdgePointerDown(
    ev: PointerEvent<HTMLDivElement>,
    col: DetailsColumn,
  ) {
    if (ev.button !== 0) return;
    ev.preventDefault();
    ev.currentTarget.setPointerCapture(ev.pointerId);
    // The name column fills what is left, so its real width is measured.
    const head = ev.currentTarget.closest('[role="columnheader"]');
    const width =
      col.id === 'name' && head
        ? head.getBoundingClientRect().width
        : widthOf(col);
    drag.current = { id: col.id, x: ev.clientX, width };
    setDraft([...columns]);
  }

  function onEdgePointerMove(ev: PointerEvent<HTMLDivElement>) {
    const d = drag.current;
    if (!d) return;
    setDraft((cols) =>
      resizeColumn(cols ?? columns, d.id, d.width + ev.clientX - d.x),
    );
  }

  function onEdgePointerUp(ev: PointerEvent<HTMLDivElement>) {
    const d = drag.current;
    if (!d) return;
    drag.current = null;
    ev.currentTarget.releasePointerCapture(ev.pointerId);
    const next = resizeColumn(columns, d.id, d.width + ev.clientX - d.x);
    setDraft(null);
    saveColumns(next);
  }

  function onEdgeDoubleClick(col: DetailsColumn) {
    const grid = element.current;
    if (!grid) return;
    saveColumns(
      resizeColumn(columns, col.id, fitWidth(widestCell(grid, col.id))),
    );
  }

  const ctx: CellContext = { t, locale, systems, generation };
  const gridStyle = {
    '--details-cols': gridTemplate(columns),
  } as CSSProperties;
  const rowClass = cn('entry-row', compact && 'entry-row--compact');

  return (
    <>
      <div
        ref={ref}
        role="grid"
        aria-label={t('library.table.label')}
        aria-rowcount={rows.length + 1}
        aria-colcount={columns.length}
        aria-activedescendant={
          selectedIndex < 0 || selectedId === null
            ? undefined
            : `entry-${selectedId}`
        }
        tabIndex={0}
        className="entry-table"
        style={gridStyle}
        onKeyDown={onKeyDown}
        onClick={onClick}
      >
        <div
          role="row"
          aria-rowindex={1}
          className="entry-row entry-row--head"
          style={{ height: HEAD_H }}
        >
          {columns.map((col) => {
            const def = COLUMN_DEFS[col.id];
            const label = t(def.labelKey);
            const { sortKey } = def;
            const active = sortKey !== undefined && sort.key === sortKey;
            const SortIcon = !active
              ? ArrowUpDown
              : sort.dir === 'asc'
                ? ArrowUp
                : ArrowDown;
            return (
              <div
                key={col.id}
                role="columnheader"
                // Focusable from script only (Shift+F10 lands on its
                // button or edge); the grid keeps one Tab stop per control.
                tabIndex={-1}
                aria-label={label}
                aria-sort={
                  sortKey === undefined
                    ? undefined
                    : active
                      ? sort.dir === 'asc'
                        ? 'ascending'
                        : 'descending'
                      : 'none'
                }
                data-column={col.id}
                className={`cell cell--head cell--${col.id}`}
                title={col.id === 'thumb' ? label : undefined}
                onContextMenu={(ev) => {
                  ev.preventDefault();
                  const button =
                    ev.currentTarget.querySelector<HTMLElement>('button');
                  openMenu(col.id, ev.clientX, ev.clientY, button);
                }}
                onKeyDown={(ev) => onHeaderKeyDown(ev, col.id)}
              >
                {sortKey !== undefined ? (
                  <Button
                    variant="ghost"
                    size="sm"
                    className="-ml-2.5 max-w-full gap-1 font-semibold text-inherit"
                    onClick={() => onSort(sortKey)}
                  >
                    <span className="truncate">{label}</span>
                    <SortIcon
                      aria-hidden
                      className={active ? 'size-3.5' : 'size-3.5 opacity-40'}
                    />
                  </Button>
                ) : col.id === 'thumb' ? null : (
                  <span className="cell__label" title={label}>
                    {label}
                  </span>
                )}
                {/* A focusable separator is a widget (WAI-ARIA window splitter);
                    jsx-a11y counts every separator as static. */}
                {/* eslint-disable-next-line jsx-a11y/no-noninteractive-element-interactions -- focusable splitter */}
                <div
                  role="separator"
                  aria-orientation="vertical"
                  aria-label={t('layouts.columnResize', { column: label })}
                  aria-valuenow={widthOf(col)}
                  aria-valuemin={def.minWidth}
                  aria-valuemax={MAX_COLUMN_WIDTH}
                  // eslint-disable-next-line jsx-a11y/no-noninteractive-tabindex -- focusable splitter, resized with ←/→
                  tabIndex={0}
                  className="column-edge"
                  onKeyDown={(ev) => onEdgeKeyDown(ev, col)}
                  onPointerDown={(ev) => onEdgePointerDown(ev, col)}
                  onPointerMove={onEdgePointerMove}
                  onPointerUp={onEdgePointerUp}
                  onPointerCancel={onEdgePointerUp}
                  onDoubleClick={() => onEdgeDoubleClick(col)}
                />
              </div>
            );
          })}
        </div>
        <div
          style={{ height: virtualizer.getTotalSize(), position: 'relative' }}
        >
          {items.map((item) => {
            const row = rows[item.index];
            if (!row) return null;
            if (row.kind === 'header') {
              const sticky = item.index === stuck;
              return (
                <SectionHeaderRow
                  key={item.key}
                  section={row.section}
                  colSpan={columns.length}
                  rowIndex={item.index + 1}
                  sticky={sticky}
                  style={
                    sticky
                      ? { height: SECTION_ROW_H, top: HEAD_H }
                      : {
                          transform: `translateY(${item.start - HEAD_H}px)`,
                          height: SECTION_ROW_H,
                        }
                  }
                />
              );
            }
            const e = entries[row.start];
            if (!e) return null;
            return (
              <div
                key={item.key}
                id={`entry-${e.id}`}
                role="row"
                aria-rowindex={item.index + 2}
                aria-selected={e.id === selectedId}
                data-entry-id={e.id}
                className={rowClass}
                style={{
                  transform: `translateY(${item.start - HEAD_H}px)`,
                  height: rowH,
                }}
              >
                {columns.map((col) => (
                  <div
                    key={col.id}
                    role="gridcell"
                    className={`cell cell--${col.id}`}
                    title={cellTitle(col.id, e, ctx)}
                  >
                    {CELLS[col.id](e, ctx)}
                  </div>
                ))}
              </div>
            );
          })}
        </div>
      </div>
      <ColumnPicker
        at={menu}
        columns={columns}
        onChange={saveColumns}
        onClose={() => setMenu(null)}
        returnFocus={menuReturn}
      />
    </>
  );
}
