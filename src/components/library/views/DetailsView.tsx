import { useVirtualizer } from '@tanstack/react-virtual';
import { ArrowDown, ArrowUp, ArrowUpDown, ChevronRight } from 'lucide-react';
import {
  useCallback,
  useContext,
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type FocusEvent,
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
  contentCell,
  fitWidth,
  gridMinWidth,
  gridTemplate,
  resizeColumn,
  versionCell,
  widthOf,
} from '@/lib/detailsColumns';
import { formatDate, formatSize } from '@/lib/format';
import type { Game } from '@/lib/games';
import { relativeDir } from '@/lib/grouping';
import {
  buildTreeModel,
  coveringSections,
  treeKey,
  type TreeModel,
  type TreeRow,
} from '@/lib/rowModel';
import type { Sort, SortKey } from '@/lib/sort';
import { REGION_CODE } from '@/lib/tileLabel';
import { useReducedMotion } from '@/lib/useReducedMotion';
import { cn } from '@/lib/utils';
import { displayVersion } from '@/lib/versions';
import type {
  DetailsColumn,
  DetailsColumnId,
  ViewPrefs,
} from '@/lib/viewPrefs';
import { useViewStore } from '@/stores/viewStore';

import { ArtStatusIcon } from '../ArtStatusIcon';
import { ArtThumb } from '../ArtThumb';
import { ChildThumb } from '../game/ChildThumb';
import { ColumnPicker } from './ColumnPicker';
import { SECTION_ROW_H, SectionHeaderRow } from './SectionHeaderRow';
import type { ViewProps } from './types';
import { useViewKeys } from './useViewKeys';

/** The column header row keeps its height at both densities. */
const HEAD_H = ROW_HEIGHT.comfortable;
/** ←/→ on a focused column edge. */
const KEY_STEP = 8;
/** Narrower than this (a phone, 390 px) the view drops three columns. */
const NARROW_PX = 480;
const NARROW_HIDDEN: ReadonlySet<DetailsColumnId> = new Set([
  'system',
  'content',
  'size',
]);
const NO_GAMES: ReadonlySet<number> = new Set();

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
  /** The game an entry stands for, when the view has games. */
  gameOf(e: EntryView): Game | null;
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
  version: (e, ctx) => {
    const cell = versionCell(e, ctx.gameOf(e), ctx.t);
    return (
      <span className="tree-version">
        {cell.text}
        {cell.warning && (
          <>
            {' '}
            <span className="tree-warning">{cell.warning}</span>
          </>
        )}
      </span>
    );
  },
  content: (e, ctx) => contentCell(ctx.gameOf(e), ctx.t, ctx.locale, e),
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

/** A row under a game (§8.4): an update, DLC, revision, demo, the
 *  add-on folder or the ghost of a missing update. */
type ContentTreeRow = Exclude<TreeRow, { kind: 'section' } | { kind: 'game' }>;

const KIND_KEY: Readonly<Record<ContentTreeRow['kind'], string>> = {
  ghost: 'content.kind.update',
  child: '',
  revision: 'content.kind.revision',
  demo: 'content.kind.demo',
  folder: 'content.kind.folder',
};

function kindLabel(row: ContentTreeRow, t: CellContext['t']): string {
  if (row.kind === 'child')
    return t(
      row.child.role === 'update' ? 'content.kind.update' : 'content.kind.dlc',
    );
  return t(KIND_KEY[row.kind]);
}

/** The file a content row stands for, when it is an entry of its own. */
function rowEntry(
  row: ContentTreeRow,
  games: ReadonlyMap<number, Game> | undefined,
): EntryView | null {
  if (row.kind === 'revision') return row.entry;
  if (row.kind === 'demo') return games?.get(row.demo)?.entry ?? null;
  return null;
}

/** The name a content row shows after its kind. */
function rowName(
  row: ContentTreeRow,
  ctx: CellContext,
  games: ReadonlyMap<number, Game> | undefined,
): string {
  switch (row.kind) {
    case 'child':
      return row.child.name;
    case 'ghost':
      return ctx.t('content.notInLibrary');
    case 'folder':
      return ctx.t('content.folderSummary');
    default:
      return rowEntry(row, games)?.name ?? '';
  }
}

/** «Mods: 1, Trucos: 2» for the folders that hold anything. */
function folderCounts(
  addons: NonNullable<EntryView['addons']>,
  ctx: CellContext,
): string {
  const n = new Intl.NumberFormat(ctx.locale);
  return (['mods', 'cheats', 'extras', 'saves'] as const)
    .filter((k) => addons[k] > 0)
    .map((k) =>
      ctx.t('content.cell.count', {
        label: ctx.t(`gameFolder.${k}`),
        n: n.format(addons[k]),
      }),
    )
    .join(', ');
}

function extOf(path: string): string {
  const name = path.split(/[\\/]/).pop() ?? '';
  const dot = name.lastIndexOf('.');
  return dot > 0 ? name.slice(dot + 1) : '';
}

/** What each column shows for a content row; blank where it has no fact. */
function contentRowCell(
  id: DetailsColumnId,
  row: ContentTreeRow,
  game: EntryView,
  ctx: CellContext,
  games: ReadonlyMap<number, Game> | undefined,
): ReactNode {
  const own = rowEntry(row, games);
  switch (id) {
    case 'thumb':
      if (row.kind === 'child')
        return (
          <ChildThumb
            generation={ctx.generation}
            entry={game}
            child={row.child}
            systemId={ctx.systems[game.system]?.system_id}
            size={24}
          />
        );
      return own ? (
        <ArtThumb
          generation={ctx.generation}
          entry={own}
          media="cover"
          className="details-thumb"
          placeholder=""
        />
      ) : null;
    case 'name':
      return (
        <span className="tree-name">
          <span className="tree-kind">{kindLabel(row, ctx.t)}</span>
          <span className="tree-name__text">{rowName(row, ctx, games)}</span>
        </span>
      );
    case 'version':
      if (row.kind === 'ghost')
        return (
          <span className="tree-warning">
            {ctx.t('content.ghost', { v: row.version.replace(/^[vV]/, '') })}
          </span>
        );
      if (row.kind === 'child')
        return (
          <span className="tree-version">
            {row.child.version === undefined
              ? '—'
              : displayVersion(row.child.version)}
            {row.newest && (
              <>
                {' '}
                <span className="tree-note">{ctx.t('content.newest')}</span>
              </>
            )}
          </span>
        );
      if (row.kind === 'revision') return row.entry.revision ?? '—';
      return own ? CELLS.version(own, ctx) : null;
    case 'content':
      return row.kind === 'folder' ? folderCounts(row.addons, ctx) : null;
    case 'ext':
      if (row.kind === 'child') return extOf(row.child.rel_path);
      return own?.ext ?? null;
    case 'size':
      if (row.kind === 'child') return formatSize(row.child.size, ctx.locale);
      return own ? formatSize(own.size, ctx.locale) : null;
    case 'mtime':
      if (row.kind === 'child') return formatDate(row.child.mtime, ctx.locale);
      return own ? formatDate(own.mtime, ctx.locale) : null;
    case 'cover':
    case 'title':
    case 'screenshot':
      return own ? CELLS[id](own, ctx) : null;
    default:
      return null;
  }
}

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
    case 'content': {
      const text = contentCell(ctx.gameOf(e), ctx.t, ctx.locale, e);
      return text === '—' ? undefined : text;
    }
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

/** A column's width as drawn. The name fills what the other columns leave,
 *  so its saved width is only a minimum: drags and arrow keys start from
 *  the header's measured width, or the first steps would do nothing. */
function renderedWidth(col: DetailsColumn, edge: Element): number {
  if (col.id !== 'name') return widthOf(col);
  const head = edge.closest('[role="columnheader"]');
  const measured = head ? Math.round(head.getBoundingClientRect().width) : 0;
  return Math.max(widthOf(col), measured);
}

/** The keys the treegrid answers itself, even where they do nothing. */
const TREE_KEYS: ReadonlySet<string> = new Set([
  'ArrowDown',
  'ArrowUp',
  'ArrowLeft',
  'ArrowRight',
  'Home',
  'End',
  'PageDown',
  'PageUp',
]);

/** The mounted row element with `key`. */
function rowElement(root: HTMLElement, key: string): HTMLElement | null {
  for (const el of root.querySelectorAll<HTMLElement>('[data-row-key]'))
    if (el.dataset.rowKey === key) return el;
  return null;
}

/** Details (⌘2): one row per game, the columns the user picked, as a
 *  treegrid whose games open on their updates, DLC, revisions and demos
 *  (addon views §8.4). */
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
  content,
}: DetailsViewProps) {
  const t = useT();
  const locale = useContext(LocaleContext);
  const hintId = useId();
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
  const games = content?.byEntry;
  const reduced = useReducedMotion();
  const expanded = useViewStore((s) => s.expanded) ?? NO_GAMES;
  const setExpanded = useViewStore((s) => s.setExpanded);
  const initExpanded = useViewStore((s) => s.initExpanded);
  const model: TreeModel = useMemo(
    () =>
      buildTreeModel(
        coveringSections(sections, entries.length),
        entries,
        games,
        expanded,
        withHeaders,
      ),
    [sections, entries, games, expanded, withHeaders],
  );
  const { rows, gameRow, headerRows } = model;
  const anyExpandable = useMemo(
    () => rows.some((r) => r.kind === 'game' && r.expandable),
    [rows],
  );
  const rowByKey = useMemo(
    () => new Map(rows.map((r, i) => [r.key, i])),
    [rows],
  );
  const indexOf = useMemo(
    () => new Map(entries.map((e, i) => [e.id, i])),
    [entries],
  );
  // A revision that is not the default (opened from its row) still
  // selects its game's row.
  const selectedGame =
    selectedId === null ? null : (games?.get(selectedId) ?? null);
  const selectedEntry = selectedGame ? selectedGame.entry.id : selectedId;
  const selectedIndex =
    selectedEntry === null ? -1 : (indexOf.get(selectedEntry) ?? -1);
  const selectedRow = selectedIndex < 0 ? -1 : (gameRow[selectedIndex] ?? -1);

  // The first time Details shows, the selected game starts open (§8.4).
  const startOpen =
    selectedGame && selectedRow >= 0 && rows[selectedRow]?.kind === 'game'
      ? selectedGame.entry.id
      : null;
  useEffect(() => {
    initExpanded(startOpen === null ? [] : [startOpen]);
  }, [initExpanded, startOpen]);

  // Row-first roving focus: the focused row is the selected game's row,
  // or one of its content rows the keyboard moved to.
  const [activeKey, setActiveKey] = useState<string | null>(null);
  const keyed = activeKey === null ? undefined : rowByKey.get(activeKey);
  const activeRow =
    keyed !== undefined &&
    rows[keyed]?.kind !== 'game' &&
    rows[keyed]?.parent === selectedRow &&
    selectedRow >= 0
      ? keyed
      : selectedRow;
  const focusRow =
    activeRow >= 0 ? activeRow : rows.findIndex((r) => r.kind !== 'section');
  const focusKey = rows[focusRow]?.key ?? null;
  // The focused row (and its group row) stays mounted when the wheel
  // scrolls it away, so the focus never sits on a removed element.
  const rangeExtractor = useActiveRange(
    focusRow,
    focusRow < 0 ? null : stickyHeaderAt(headerRows, focusRow),
    headerRows,
  );
  const getItemKey = useCallback(
    (i: number) => {
      const row = rows[i];
      if (!row) return i;
      return `${prefs.density}:${row.key}`;
    },
    [rows, prefs.density],
  );
  // eslint-disable-next-line react-hooks/incompatible-library -- TanStack Virtual is not memoisable by the React Compiler
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => element.current,
    estimateSize: (i) => (rows[i]?.kind === 'section' ? SECTION_ROW_H : rowH),
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

  // The focus follows the focused row while it is in the view (a key, a
  // new selection from Quick look), never pulled in from elsewhere.
  useLayoutEffect(() => {
    const root = element.current;
    const now = document.activeElement;
    if (!root || focusKey === null || !(now instanceof HTMLElement)) return;
    const inView =
      now === root ||
      (now.getAttribute('role') === 'row' && root.contains(now));
    if (!inView) return;
    const target = rowElement(root, focusKey);
    if (target && target !== now) target.focus({ preventScroll: true });
  });

  const viewKeys = useViewKeys({
    entries,
    sections,
    selectedIndex,
    onSelect,
    onOpen,
    onPreview,
    // The selected game's row, or its content row the focus is on.
    scrollToIndex: (i) => {
      const row = gameRow[i] ?? 0;
      const to = rows[activeRow]?.parent === row ? activeRow : row;
      virtualizer.scrollToIndex(to, { align: 'auto' });
    },
    // ↑/↓, Home, End and the page keys are the treegrid's (below).
    move: () => null,
    rowFocus: true,
  });

  /** The game entry a row belongs to. */
  function ownerOf(row: TreeRow): EntryView | undefined {
    return row.kind === 'section' ? undefined : entries[row.index];
  }

  function focusTo(r: number) {
    const row = rows[r];
    const owner = row && ownerOf(row);
    if (!row || !owner) return;
    setActiveKey(row.key);
    if (owner.id !== selectedEntry) onSelect(owner.id);
    virtualizer.scrollToIndex(r, { align: 'auto' });
  }

  /** Enter or a click: the panel, at what the row stands for. */
  function activate(row: TreeRow) {
    const owner = ownerOf(row);
    if (!owner) return;
    setActiveKey(row.key);
    switch (row.kind) {
      case 'section':
        return;
      case 'child':
        if (content?.onOpenChild)
          content.onOpenChild(owner.id, row.child.index);
        else onOpen(owner.id);
        return;
      case 'ghost':
        if (content?.onOpenVersion) content.onOpenVersion(owner.id);
        else onOpen(owner.id);
        return;
      case 'revision':
        onOpen(row.entry.id);
        return;
      case 'demo':
        onOpen(row.demo);
        return;
      default:
        onOpen(owner.id);
    }
  }

  function onKeyDown(ev: KeyboardEvent<HTMLDivElement>) {
    const target = ev.target;
    const onRow =
      target === ev.currentTarget ||
      (target instanceof HTMLElement && target.getAttribute('role') === 'row');
    if (!onRow || ev.nativeEvent.isComposing) return;
    const plain = !ev.altKey && !ev.ctrlKey && !ev.metaKey && !ev.shiftKey;
    if (plain && ev.key === 'Enter') {
      const row = rows[focusRow];
      if (row) {
        ev.preventDefault();
        activate(row);
      }
      return;
    }
    if (plain && activeRow < 0 && TREE_KEYS.has(ev.key) && focusRow >= 0) {
      // Nothing selected yet: the first key lands on the first row (the
      // last for End), as in the other views.
      ev.preventDefault();
      const last = treeKey(model, focusRow, 'End', 1);
      focusTo(ev.key === 'End' && 'row' in last ? last.row : focusRow);
      return;
    }
    if (plain && focusRow >= 0) {
      const move = treeKey(model, focusRow, ev.key, pageRows);
      if ('row' in move) {
        ev.preventDefault();
        focusTo(move.row);
        return;
      }
      if ('expand' in move || 'collapse' in move) {
        ev.preventDefault();
        if ('expand' in move) setExpanded(move.expand, true);
        else setExpanded(move.collapse, false);
        setActiveKey(rows[focusRow]?.key ?? null);
        return;
      }
      if (TREE_KEYS.has(ev.key)) {
        // At an edge, or → on a row with nothing under it: stay put.
        ev.preventDefault();
        return;
      }
    }
    viewKeys(ev);
  }

  function onFocus(ev: FocusEvent<HTMLDivElement>) {
    // The view's root takes the focus from the app (after the panel
    // closes): hand it to the focused row.
    if (ev.target !== ev.currentTarget || focusKey === null) return;
    rowElement(ev.currentTarget, focusKey)?.focus({ preventScroll: true });
  }

  function onClick(ev: MouseEvent<HTMLDivElement>) {
    const el =
      ev.target instanceof Element
        ? ev.target.closest<HTMLElement>('[data-row-key]')
        : null;
    const at = el ? rowByKey.get(el.dataset.rowKey ?? '') : undefined;
    const row = at === undefined ? undefined : rows[at];
    if (!row) return;
    if (
      row.kind === 'game' &&
      row.expandable &&
      ev.target instanceof Element &&
      ev.target.closest('[data-tree-toggle]')
    ) {
      setActiveKey(row.key);
      setExpanded(row.id, !row.expanded);
      return;
    }
    activate(row);
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
    saveColumns(
      resizeColumn(
        columns,
        col.id,
        renderedWidth(col, ev.currentTarget) + dir * KEY_STEP,
      ),
    );
  }

  function onEdgePointerDown(
    ev: PointerEvent<HTMLDivElement>,
    col: DetailsColumn,
  ) {
    if (ev.button !== 0) return;
    ev.preventDefault();
    ev.currentTarget.setPointerCapture(ev.pointerId);
    drag.current = {
      id: col.id,
      x: ev.clientX,
      width: renderedWidth(col, ev.currentTarget),
    };
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

  const ctx: CellContext = {
    t,
    locale,
    systems,
    generation,
    gameOf: (e) => games?.get(e.id) ?? null,
  };
  // A phone-width window keeps the name, its version and the art (§8.4).
  const narrow = size.width > 0 && size.width < NARROW_PX;
  const shown = narrow
    ? columns.filter((c) => !NARROW_HIDDEN.has(c.id))
    : columns;
  const gridStyle = {
    '--details-cols': gridTemplate(shown),
    '--details-min-width': `${gridMinWidth(shown)}px`,
  } as CSSProperties;
  const rowClass = cn('entry-row', compact && 'entry-row--compact');
  // Games open on their content: the chevron column and the key hint.
  // The add-on line misses games that hold only mods, cheats, extras or
  // saves, yet those open too.
  const tree = content?.line === true || anyExpandable;
  const chevronClass = (open: boolean) =>
    cn(
      'tree-chevron',
      open && 'tree-chevron--open',
      !reduced && 'tree-chevron--animated',
    );

  return (
    <div className="details-tree">
      <div
        ref={ref}
        role="treegrid"
        aria-label={t('layouts.announce', { view: t('layouts.details') })}
        aria-describedby={tree ? hintId : undefined}
        data-view-root=""
        aria-rowcount={rows.length + 1}
        aria-colcount={shown.length}
        // Rows hold the focus (roving tabIndex); with none, the view does.
        tabIndex={rows.length > 0 ? -1 : 0}
        className="entry-table"
        style={gridStyle}
        onKeyDown={onKeyDown}
        onFocus={onFocus}
        onClick={onClick}
      >
        <div
          role="row"
          aria-rowindex={1}
          className="entry-row entry-row--head"
          style={{ height: HEAD_H }}
        >
          {shown.map((col) => {
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
            const place = {
              'aria-rowindex': item.index + 2,
              'aria-level': row.level,
              'aria-setsize': row.setsize,
              'aria-posinset': row.posinset,
            };
            if (row.kind === 'section') {
              const sticky = item.index === stuck;
              return (
                <SectionHeaderRow
                  key={item.key}
                  section={row.section}
                  colSpan={shown.length}
                  rowIndex={item.index + 1}
                  sticky={sticky}
                  tree={row}
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
            const style = {
              transform: `translateY(${item.start - HEAD_H}px)`,
              height: rowH,
            };
            const tabIndex = item.index === focusRow ? 0 : -1;
            const owner = entries[row.index];
            if (!owner) return null;
            if (row.kind === 'game') {
              const e = owner;
              return (
                <div
                  key={item.key}
                  id={`entry-${e.id}`}
                  role="row"
                  {...place}
                  aria-expanded={row.expandable ? row.expanded : undefined}
                  aria-selected={e.id === selectedEntry}
                  tabIndex={tabIndex}
                  data-entry-id={e.id}
                  data-row-key={row.key}
                  className={cn(rowClass, 'tree-row')}
                  style={style}
                >
                  {shown.map((col) => (
                    <div
                      key={col.id}
                      role="gridcell"
                      className={`cell cell--${col.id}`}
                      title={cellTitle(col.id, e, ctx)}
                    >
                      {col.id === 'name' && tree ? (
                        <span className="tree-name">
                          {row.expandable ? (
                            <span
                              aria-hidden="true"
                              data-tree-toggle=""
                              className={chevronClass(row.expanded)}
                            >
                              <ChevronRight className="size-3.5" />
                            </span>
                          ) : (
                            <span aria-hidden="true" className="tree-chevron" />
                          )}
                          <span className="tree-name__text">
                            {CELLS.name(e, ctx)}
                          </span>
                        </span>
                      ) : (
                        CELLS[col.id](e, ctx)
                      )}
                    </div>
                  ))}
                </div>
              );
            }
            // Content rows are never selected and carry no entry id, so
            // bulk actions and counts never see them.
            return (
              <div
                key={item.key}
                role="row"
                {...place}
                tabIndex={tabIndex}
                data-row-key={row.key}
                className={cn(
                  rowClass,
                  'tree-row tree-row--child',
                  row.kind === 'ghost' && 'tree-row--ghost',
                )}
                style={style}
              >
                {shown.map((col) => (
                  <div
                    key={col.id}
                    role="gridcell"
                    className={`cell cell--${col.id}`}
                    title={
                      col.id === 'name'
                        ? rowName(row, ctx, games)
                        : col.id === 'content' && row.kind === 'folder'
                          ? folderCounts(row.addons, ctx) || undefined
                          : undefined
                    }
                  >
                    {contentRowCell(col.id, row, owner, ctx, games)}
                  </div>
                ))}
              </div>
            );
          })}
        </div>
      </div>
      {tree && (
        // At phone width the table needs the room; the treegrid still
        // names it as its description.
        <p id={hintId} className={narrow ? 'sr-only' : 'tree-hint'}>
          {t('content.keysHint')}
        </p>
      )}
      <ColumnPicker
        at={menu}
        columns={columns}
        onChange={saveColumns}
        onClose={() => setMenu(null)}
        returnFocus={menuReturn}
      />
    </div>
  );
}
