import { useVirtualizer } from '@tanstack/react-virtual';
import { ChevronRight } from 'lucide-react';
import {
  useCallback,
  useContext,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type FocusEvent,
  type KeyboardEvent,
  type MouseEvent,
  type ReactNode,
} from 'react';

import { SystemLabel } from '@/components/system/SystemLabel';
import { useActiveRange } from '@/hooks/useActiveRange';
import { useElementSize } from '@/hooks/useElementSize';
import { LocaleContext } from '@/i18n/core';
import { useT } from '@/i18n/useT';
import type { EntryView, SystemView } from '@/ipc/types';
import { buildFolderTree, type FolderNode } from '@/lib/folderTree';
import { formatDate, formatSize } from '@/lib/format';
import { relativeDir, type Section } from '@/lib/grouping';
import {
  createTypeAhead,
  findByPrefix,
  nextIndex,
  paneMove,
  type TypeAhead,
} from '@/lib/keyboardNav';
import { cn } from '@/lib/utils';
import { useViewStore } from '@/stores/viewStore';

import { ArtThumb } from '../ArtThumb';
import type { ViewProps } from './types';
import { useViewKeys } from './useViewKeys';

/** Every row of every Columns pane is this tall (spec §B.7). */
export const MILLER_ROW_H = 28;

/** The pane title strip above each list. */
const PANE_HEAD_H = 28;

/** Pixels a folder row is indented per level below the system folder. */
const FOLDER_INDENT = 12;

type PaneId = 'systems' | 'folders' | 'games';

interface FolderRow {
  /** Path relative to the system folder; '' is the system folder itself. */
  path: string;
  label: string;
  depth: number;
  count: number;
}

/** Keys that move inside a pane (← and → move between panes). */
const LIST_KEYS = new Set([
  'ArrowUp',
  'ArrowDown',
  'Home',
  'End',
  'PageUp',
  'PageDown',
]);

/** Group by does not apply here (§B.4.3): the games keep one section. */
const NO_SECTIONS: readonly Section[] = [];

function flattenFolders(
  nodes: readonly FolderNode[],
  depth: number,
  out: FolderRow[],
): void {
  for (const node of nodes) {
    out.push({ path: node.path, label: node.name, depth, count: node.count });
    flattenFolders(node.children, depth + 1, out);
  }
}

function inFolder(rel: string, folder: string | null): boolean {
  if (folder === null) return true;
  if (folder === '') return rel === '';
  return rel === folder || rel.startsWith(`${folder}/`);
}

/** A single printable character without modifiers (not Space). */
function typedChar(ev: KeyboardEvent<HTMLElement>): string | null {
  if (ev.ctrlKey || ev.metaKey || ev.altKey) return null;
  return ev.key !== ' ' && [...ev.key].length === 1 ? ev.key : null;
}

interface PaneListProps {
  kind: PaneId;
  label: string;
  /** Changes when the pane lists something else: scroll back to the top. */
  listKey: string;
  count: number;
  activeIndex: number;
  optionId(i: number): string;
  getKey(i: number): string | number;
  renderOption(i: number, style: CSSProperties): ReactNode;
  tabbable: boolean;
  current: boolean;
  register(kind: PaneId, el: HTMLDivElement | null): void;
  onKeyDown(ev: KeyboardEvent<HTMLDivElement>): void;
  onFocus(kind: PaneId): void;
  onClick(ev: MouseEvent<HTMLDivElement>): void;
}

/** One Miller pane: a titled, virtualised listbox whose active option is
 *  always rendered, so `aria-activedescendant` points at a real element. */
function PaneList({
  kind,
  label,
  listKey,
  count,
  activeIndex,
  optionId,
  getKey,
  renderOption,
  tabbable,
  current,
  register,
  onKeyDown,
  onFocus,
  onClick,
}: PaneListProps) {
  const headId = useId();
  const [sizeRef, size, element] = useElementSize<HTMLDivElement>();
  const ref = useCallback(
    (el: HTMLDivElement | null) => {
      sizeRef(el);
      register(kind, el);
    },
    [sizeRef, register, kind],
  );
  const rangeExtractor = useActiveRange(activeIndex, null);
  // eslint-disable-next-line react-hooks/incompatible-library -- TanStack Virtual is not memoisable by the React Compiler
  const virtualizer = useVirtualizer({
    count,
    getScrollElement: () => element.current,
    estimateSize: () => MILLER_ROW_H,
    overscan: 8,
    getItemKey: getKey,
    rangeExtractor,
    initialRect: size,
  });
  useEffect(() => {
    if (activeIndex >= 0) {
      virtualizer.scrollToIndex(activeIndex, { align: 'auto' });
    } else {
      virtualizer.scrollToOffset(0);
    }
  }, [activeIndex, listKey, virtualizer]);

  return (
    <div
      className={cn('miller__pane', `miller__pane--${kind}`)}
      data-current={current ? 'true' : undefined}
    >
      <div className="miller__head" id={headId}>
        {label}
      </div>
      <div
        ref={ref}
        role="listbox"
        aria-labelledby={headId}
        aria-activedescendant={
          activeIndex >= 0 && activeIndex < count
            ? optionId(activeIndex)
            : undefined
        }
        tabIndex={tabbable ? 0 : -1}
        className="miller__list"
        onKeyDown={onKeyDown}
        onFocus={(ev: FocusEvent<HTMLDivElement>) => {
          if (ev.target === ev.currentTarget) onFocus(kind);
        }}
        onClick={onClick}
      >
        <div
          style={{ height: virtualizer.getTotalSize(), position: 'relative' }}
        >
          {virtualizer.getVirtualItems().map((item) =>
            renderOption(item.index, {
              transform: `translateY(${item.start}px)`,
              height: MILLER_ROW_H,
            }),
          )}
        </div>
      </div>
    </div>
  );
}

/** The row index of a click inside a pane, if it hit a row. */
function rowOf(target: EventTarget | null): number | null {
  const el = target instanceof Element ? target.closest('[data-index]') : null;
  const raw = el?.getAttribute('data-index');
  return raw == null ? null : Number(raw);
}

/**
 * Columns (⌘3): Finder's Miller columns over data the library already has —
 * systems, then the folders inside the chosen system, then its games, then
 * a preview of the active game. Choosing a system or folder is browse state
 * of this view only; it never changes the sidebar scope (§B.4.3). ↑/↓ move
 * inside a pane, → or Enter on a system or folder goes into the next pane,
 * ← goes back, Enter on a game opens it as in every view.
 */
export function ColumnsView({
  entries,
  systems,
  generation,
  prefs,
  selectedId,
  onSelect,
  onOpen,
  onPreview,
}: ViewProps) {
  const t = useT();
  const locale = useContext(LocaleContext);
  const scope = useViewStore((s) => s.scope);
  const baseId = useId();
  const previewHeadId = useId();
  const [rootRef, size] = useElementSize<HTMLDivElement>();
  const pageRows = Math.max(
    1,
    Math.floor((size.height - PANE_HEAD_H) / MILLER_ROW_H),
  );
  const number = useMemo(() => new Intl.NumberFormat(locale), [locale]);

  const indexOf = useMemo(
    () => new Map(entries.map((e, i) => [e.id, i])),
    [entries],
  );
  const selectedEntry =
    selectedId === null ? undefined : entries[indexOf.get(selectedId) ?? -1];

  // Pane 1: the systems that hold any of the shown entries, in sidebar order.
  const shownSystems = useMemo(() => {
    const has = new Set(entries.map((e) => e.system));
    return systems.filter((s) => has.has(s.index));
  }, [entries, systems]);
  const counts = useMemo(() => {
    const m = new Map<number, number>();
    for (const e of entries) m.set(e.system, (m.get(e.system) ?? 0) + 1);
    return m;
  }, [entries]);
  const showSystems = scope.kind !== 'system';

  // Browse state: what the user chose in panes 1 and 2, and the pane that
  // holds the view's tab stop. Opening on a selection starts at its game.
  const pathOf = (e: EntryView) => ({
    system: e.system,
    folder: relativeDir(e.dir, systems[e.system]),
  });
  const [browse, setBrowse] = useState<{
    system: number | null;
    folder: string | null;
  }>(() =>
    selectedEntry ? pathOf(selectedEntry) : { system: null, folder: null },
  );
  const [pane, setPane] = useState<PaneId>(selectedEntry ? 'games' : 'systems');

  const system: SystemView | undefined =
    shownSystems.find(
      (s) =>
        s.index === (scope.kind === 'system' ? scope.system : browse.system),
    ) ?? shownSystems[0];
  const systemIndex = system ? shownSystems.indexOf(system) : -1;

  // Pane 2: only this system's entries are looked at (§B.7).
  const { systemEntries, rels } = useMemo(() => {
    const list = system ? entries.filter((e) => e.system === system.index) : [];
    return {
      systemEntries: list,
      rels: list.map((e) => relativeDir(e.dir, system)),
    };
  }, [entries, system]);
  const folderRows = useMemo(() => {
    const tree = buildFolderTree(rels.map((dir) => ({ dir })));
    if (tree.children.length === 0) return [];
    const nested = tree.children.reduce((n, c) => n + c.count, 0);
    const rows: FolderRow[] =
      tree.count > nested
        ? [
            {
              path: '',
              label: t('layouts.folderRoot'),
              depth: 0,
              count: tree.count - nested,
            },
          ]
        : [];
    flattenFolders(tree.children, 0, rows);
    return rows;
  }, [rels, t]);
  const folderIndex = Math.max(
    0,
    folderRows.findIndex((r) => r.path === browse.folder),
  );
  const folder = folderRows[folderIndex]?.path ?? null;

  // Pane 3: the chosen folder's games, in the current sort.
  const games = useMemo(
    () => systemEntries.filter((_, i) => inFolder(rels[i] ?? '', folder)),
    [systemEntries, rels, folder],
  );
  const gameIndexOf = useMemo(
    () => new Map(games.map((e, i) => [e.id, i])),
    [games],
  );
  const gameIndex =
    selectedId === null ? -1 : (gameIndexOf.get(selectedId) ?? -1);

  // The selection moved from outside (Quick look, another view): follow it
  // to its system and folder, unless it is already in the games shown.
  const [seenId, setSeenId] = useState(selectedId);
  if (seenId !== selectedId) {
    setSeenId(selectedId);
    if (selectedEntry && gameIndex < 0) setBrowse(pathOf(selectedEntry));
  }

  const listPanes: PaneId[] = [
    ...(showSystems ? (['systems'] as const) : []),
    ...(folderRows.length > 0 ? (['folders'] as const) : []),
    'games',
  ];
  const activePane = listPanes.includes(pane) ? pane : listPanes[0]!;

  const paneEls = useRef<Partial<Record<PaneId, HTMLDivElement | null>>>({});
  const register = useCallback((kind: PaneId, el: HTMLDivElement | null) => {
    paneEls.current[kind] = el;
  }, []);
  const focusPane = (kind: PaneId) => {
    setPane(kind);
    const el = paneEls.current[kind];
    el?.focus();
    // Too narrow for every pane: bring this one into view, as Finder does.
    el?.parentElement?.scrollIntoView?.({
      block: 'nearest',
      inline: 'nearest',
    });
  };
  const enterFrom = (kind: PaneId) => {
    const next = listPanes[listPanes.indexOf(kind) + 1];
    if (!next) return;
    if (next === 'games' && gameIndex < 0 && games[0]) onSelect(games[0].id);
    focusPane(next);
  };

  // Type-ahead buffers of the two branch panes (games use useViewKeys').
  const typeAheads = useRef<Partial<Record<PaneId, TypeAhead>>>({});
  /** ↑↓/Home/End/Page, type-ahead, → or Enter in, ← back: the systems and
   *  folders panes. */
  function onBranchKeys(
    kind: 'systems' | 'folders',
    ev: KeyboardEvent<HTMLDivElement>,
  ) {
    if (ev.target !== ev.currentTarget || ev.nativeEvent.isComposing) return;
    if (ev.key === 'Tab') return;
    const sys = kind === 'systems';
    const index = sys ? systemIndex : folderIndex;
    const count = sys ? shownSystems.length : folderRows.length;
    const name = (i: number) =>
      (sys ? shownSystems[i]?.name : folderRows[i]?.label) ?? '';
    const choose = sys ? chooseSystem : chooseFolder;
    if (ev.key === 'ArrowRight' || ev.key === 'ArrowLeft') {
      ev.preventDefault();
      const to = paneMove(
        ev.key,
        listPanes.indexOf(kind),
        listPanes.length,
        index >= 0,
      );
      if (to === null) return;
      if (ev.key === 'ArrowRight') enterFrom(kind);
      else focusPane(listPanes[to]!);
      return;
    }
    if (ev.key === 'Enter') {
      ev.preventDefault();
      enterFrom(kind);
      return;
    }
    if (ev.key === ' ') {
      ev.preventDefault();
      return;
    }
    const ch = typedChar(ev);
    if (ch) {
      ev.preventDefault();
      const ahead = (typeAheads.current[kind] ??= createTypeAhead());
      const buffer = ahead.push(ch, ev.timeStamp);
      const hit = findByPrefix(
        name,
        count,
        [...buffer].length === 1 ? index : index - 1,
        buffer,
      );
      if (hit !== null) choose(hit);
      return;
    }
    if (!LIST_KEYS.has(ev.key) || ev.altKey) return;
    const next = nextIndex(ev.key, index, count, 1, pageRows);
    if (next === null) return;
    ev.preventDefault();
    choose(next);
  }

  const chooseSystem = (i: number) => {
    const s = shownSystems[i];
    if (s) setBrowse({ system: s.index, folder: null });
  };
  const chooseFolder = (i: number) => {
    const row = folderRows[i];
    if (row && system) setBrowse({ system: system.index, folder: row.path });
  };

  const gameKeys = useViewKeys({
    entries: games,
    sections: NO_SECTIONS,
    selectedIndex: gameIndex,
    onSelect,
    onOpen,
    onPreview,
    // The pane keeps its active row in view itself.
    scrollToIndex: () => {},
    move: (key) =>
      LIST_KEYS.has(key)
        ? nextIndex(key, gameIndex, games.length, 1, pageRows)
        : null,
  });
  function onGameKeys(ev: KeyboardEvent<HTMLDivElement>) {
    if (
      ev.target === ev.currentTarget &&
      !ev.nativeEvent.isComposing &&
      !ev.altKey &&
      !ev.metaKey &&
      !ev.ctrlKey &&
      (ev.key === 'ArrowLeft' || ev.key === 'ArrowRight')
    ) {
      ev.preventDefault();
      const to = paneMove(
        ev.key,
        listPanes.indexOf('games'),
        listPanes.length,
        false,
      );
      if (to !== null) focusPane(listPanes[to]!);
      return;
    }
    gameKeys(ev);
  }

  const shown = gameIndex >= 0 ? games[gameIndex] : undefined;
  const shownSystem = shown ? systems[shown.system] : undefined;

  return (
    <div
      ref={rootRef}
      role="group"
      aria-label={t('layouts.announce', { view: t('layouts.columns') })}
      tabIndex={-1}
      className="miller"
      onFocus={(ev) => {
        if (ev.target === ev.currentTarget)
          paneEls.current[activePane]?.focus();
      }}
    >
      {showSystems && (
        <PaneList
          kind="systems"
          listKey=""
          label={t('layouts.pane.systems')}
          count={shownSystems.length}
          activeIndex={systemIndex}
          optionId={(i) => `${baseId}-s${i}`}
          getKey={(i) => shownSystems[i]?.index ?? i}
          tabbable={activePane === 'systems'}
          current={activePane === 'systems'}
          register={register}
          onFocus={setPane}
          onKeyDown={(ev) => onBranchKeys('systems', ev)}
          onClick={(ev) => {
            const i = rowOf(ev.target);
            if (i !== null) chooseSystem(i);
            focusPane('systems');
          }}
          renderOption={(i, style) => {
            const s = shownSystems[i];
            if (!s) return null;
            const chosen = i === systemIndex;
            return (
              <div
                key={s.index}
                id={`${baseId}-s${i}`}
                role="option"
                aria-selected={chosen}
                data-index={i}
                className="miller__row"
                style={style}
              >
                <SystemLabel
                  systemId={s.system_id}
                  name={s.name}
                  size={16}
                  className="miller__label"
                />
                <span className="miller__count">
                  {number.format(counts.get(s.index) ?? 0)}
                </span>
                <ChevronRight
                  aria-hidden="true"
                  className="miller__chevron"
                  size={14}
                />
              </div>
            );
          }}
        />
      )}
      {folderRows.length > 0 && (
        <PaneList
          kind="folders"
          listKey={String(system?.index ?? '')}
          label={t('layouts.pane.folders')}
          count={folderRows.length}
          activeIndex={folderIndex}
          optionId={(i) => `${baseId}-f${i}`}
          getKey={(i) => folderRows[i]?.path ?? i}
          tabbable={activePane === 'folders'}
          current={activePane === 'folders'}
          register={register}
          onFocus={setPane}
          onKeyDown={(ev) => onBranchKeys('folders', ev)}
          onClick={(ev) => {
            const i = rowOf(ev.target);
            if (i !== null) chooseFolder(i);
            focusPane('folders');
          }}
          renderOption={(i, style) => {
            const row = folderRows[i];
            if (!row) return null;
            return (
              <div
                key={row.path}
                id={`${baseId}-f${i}`}
                role="option"
                aria-selected={i === folderIndex}
                data-index={i}
                className="miller__row"
                style={{
                  ...style,
                  paddingInlineStart: `calc(0.75rem + ${row.depth * FOLDER_INDENT}px)`,
                }}
              >
                <SystemLabel
                  systemId={null}
                  name={row.label}
                  size={16}
                  className="miller__label"
                />
                <span className="miller__count">
                  {number.format(row.count)}
                </span>
                <ChevronRight
                  aria-hidden="true"
                  className="miller__chevron"
                  size={14}
                />
              </div>
            );
          }}
        />
      )}
      <PaneList
        kind="games"
        listKey={`${system?.index ?? ''}:${folder ?? ''}`}
        label={t('layouts.pane.games')}
        count={games.length}
        activeIndex={gameIndex}
        optionId={(i) => `entry-${games[i]?.id ?? ''}`}
        getKey={(i) => games[i]?.id ?? i}
        tabbable={activePane === 'games'}
        current={activePane === 'games'}
        register={register}
        onFocus={setPane}
        onKeyDown={onGameKeys}
        onClick={(ev) => {
          const i = rowOf(ev.target);
          const entry = i === null ? undefined : games[i];
          setPane('games');
          if (entry) onOpen(entry.id);
        }}
        renderOption={(i, style) => {
          const entry = games[i];
          if (!entry) return null;
          return (
            <div
              key={entry.id}
              id={`entry-${entry.id}`}
              role="option"
              aria-selected={i === gameIndex}
              data-index={i}
              data-entry-id={entry.id}
              className="miller__row"
              style={style}
              title={entry.rel_path}
            >
              <span className="miller__name">{entry.name}</span>
            </div>
          );
        }}
      />
      <div
        role="region"
        aria-labelledby={previewHeadId}
        className="miller__pane miller__pane--preview"
      >
        <div className="miller__head" id={previewHeadId}>
          {t('layouts.pane.preview')}
        </div>
        {shown ? (
          <div className="miller__preview">
            <div
              className={cn(
                'miller__art',
                prefs.art === 'cover'
                  ? 'miller__art--cover'
                  : 'miller__art--wide',
              )}
            >
              <ArtThumb
                generation={generation}
                entry={shown}
                media={prefs.art}
              />
            </div>
            <h3 className="miller__title">{shown.name}</h3>
            <SystemLabel
              systemId={shownSystem?.system_id ?? null}
              name={shownSystem?.name ?? ''}
              size={20}
            />
            <dl className="drawer-facts drawer-facts--compact">
              <dt>{t('drawer.path')}</dt>
              <dd className="break-all">{shown.rel_path}</dd>
              <dt>{t('drawer.ext')}</dt>
              <dd>.{shown.ext}</dd>
              <dt>{t('columns.size')}</dt>
              <dd>{formatSize(shown.size, locale)}</dd>
              <dt>{t('columns.modified')}</dt>
              <dd>{formatDate(shown.mtime, locale)}</dd>
              <dt>{t('filters.regions')}</dt>
              <dd>
                {shown.regions.length > 0
                  ? shown.regions.map((r) => t(`regions.${r}`)).join(', ')
                  : '—'}
              </dd>
            </dl>
          </div>
        ) : (
          <p className="miller__empty">{t('layouts.previewEmpty')}</p>
        )}
      </div>
    </div>
  );
}
