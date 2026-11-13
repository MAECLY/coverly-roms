import { useVirtualizer } from '@tanstack/react-virtual';
import { ArrowUpRight, ChevronRight } from 'lucide-react';
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
import { summaryParts, summaryText } from '@/lib/addonSummary';
import { childArtState, ORIGIN_KEY } from '@/lib/childArt';
import {
  buildFolderTree,
  columnDir,
  gameGroups,
  groupFiles,
  type FolderNode,
  type GameGroup,
  type GroupFile,
} from '@/lib/folderTree';
import { formatDate, formatSize } from '@/lib/format';
import type { Game } from '@/lib/games';
import { relativeDir, type Section } from '@/lib/grouping';
import {
  createTypeAhead,
  findByPrefix,
  nextIndex,
  paneMove,
  typeAheadQuery,
  type TypeAhead,
} from '@/lib/keyboardNav';
import { cn } from '@/lib/utils';
import { useViewStore } from '@/stores/viewStore';

import { ArtThumb } from '../ArtThumb';
import { ChildThumb } from '../game/ChildThumb';
import type { ViewProps } from './types';
import { useViewKeys } from './useViewKeys';

/** Every row of every Columns pane is this tall (spec §B.7). */
export const MILLER_ROW_H = 28;

/** The pane title strip above each list. */
const PANE_HEAD_H = 28;

/** Pixels a folder row is indented per level below the system folder. */
const FOLDER_INDENT = 12;

/** The list panes, left to right; `content` and `files` open a game. */
type PaneId = 'systems' | 'folders' | 'games' | 'content' | 'files';

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

/** A single printable character (not Space, which only continues a name). */
function isPrintable(key: string): boolean {
  return key !== ' ' && [...key].length === 1;
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

/** The visible add-on suffix of a game row: `+1 DLC` (or `2 rev.`), then
 * the missing-update warning; the version alone is left to the preview. */
function rowSuffix(
  game: Game | undefined,
  t: ReturnType<typeof useT>,
): { text: string; warning: boolean }[] {
  if (!game) return [];
  const parts = summaryParts(game);
  const count = parts.find((p) => p.kind === 'dlc' || p.kind === 'rev');
  const missing = parts.find((p) => p.kind === 'missing');
  return [
    ...(count ? [{ text: summaryText(count, t), warning: false }] : []),
    ...(missing ? [{ text: summaryText(missing, t), warning: true }] : []),
  ];
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
  content,
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
  const gameOf = (id: number): Game | undefined => content?.byEntry.get(id);
  const pathOf = (e: EntryView) => ({
    system: e.system,
    folder: columnDir(relativeDir(e.dir, systems[e.system]), gameOf(e.id)),
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
    // A game folder is listed as its game, in the folder above (§8.5).
    return {
      systemEntries: list,
      rels: list.map((e) =>
        columnDir(relativeDir(e.dir, system), content?.byEntry.get(e.id)),
      ),
    };
  }, [entries, system, content]);
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

  // Panes 4 and 5: the shown game's typed groups and the chosen group's
  // files (§8.5). A demo link is offered only while the demo is listed.
  const shownGame = gameIndex >= 0 ? gameOf(games[gameIndex]!.id) : undefined;
  /** A game's groups as Columns opens them: the demo only while it is
   * listed; none when the base would be alone. */
  const groupsOf = (game: Game | undefined): GameGroup[] => {
    if (!game) return [];
    const all = gameGroups(game).filter(
      (g) => g.kind !== 'demo' || indexOf.has(g.demo),
    );
    return all.length > 1 ? all : [];
  };
  // eslint-disable-next-line react-hooks/exhaustive-deps -- groupsOf reads only indexOf
  const groups = useMemo(() => groupsOf(shownGame), [shownGame, indexOf]);
  const [inner, setInner] = useState<{
    game: number | null;
    group: number;
    file: number;
  }>({ game: null, group: 0, file: 0 });
  // Another game: its groups start at «Juego base».
  const innerOf = inner.game === shownGame?.id ? inner : null;
  const groupIndex = Math.min(innerOf?.group ?? 0, groups.length - 1);
  const group = groupIndex >= 0 ? groups[groupIndex] : undefined;
  const files: GroupFile[] = useMemo(
    () => (shownGame && group ? groupFiles(shownGame, group.kind) : []),
    [shownGame, group],
  );
  const fileIndex =
    files.length > 0 ? Math.min(innerOf?.file ?? 0, files.length - 1) : -1;
  const file = fileIndex >= 0 ? files[fileIndex] : undefined;

  const listPanes: PaneId[] = [
    ...(showSystems ? (['systems'] as const) : []),
    ...(folderRows.length > 0 ? (['folders'] as const) : []),
    'games',
    ...(groups.length > 0 ? (['content'] as const) : []),
    ...(files.length > 0 ? (['files'] as const) : []),
  ];
  const activePane = listPanes.includes(pane)
    ? pane
    : pane === 'content' || pane === 'files'
      ? 'games'
      : listPanes[0]!;

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
    const plain = !ev.ctrlKey && !ev.metaKey && !ev.altKey;
    if (plain && (ev.key === 'ArrowRight' || ev.key === 'ArrowLeft')) {
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
    const ahead = (typeAheads.current[kind] ??= createTypeAhead());
    const now = ev.timeStamp;
    // Mid-word, Space is part of the name ("game b"), as in the games pane;
    // otherwise there is no art to preview on a branch.
    const typing = ev.key === ' ' && ahead.pending(now) !== '';
    if (ev.key === ' ' && !typing) {
      ev.preventDefault();
      return;
    }
    if (plain && (typing || isPrintable(ev.key))) {
      ev.preventDefault();
      const { from, prefix } = typeAheadQuery(ahead.push(ev.key, now), index);
      const hit = findByPrefix(name, count, from, prefix);
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
        groups.length > 0,
      );
      if (to !== null) focusPane(listPanes[to]!);
      return;
    }
    gameKeys(ev);
  }

  const chooseGroup = (i: number) => {
    if (shownGame && groups[i])
      setInner({ game: shownGame.id, group: i, file: 0 });
  };
  const chooseFile = (i: number) => {
    if (shownGame && files[i])
      setInner({ game: shownGame.id, group: groupIndex, file: i });
  };
  /** «Demo»: the demo game's own column path, as if chosen there. */
  const jumpTo = (id: number) => {
    const entry = entries[indexOf.get(id) ?? -1];
    if (!entry) return;
    setBrowse(pathOf(entry));
    onSelect(entry.id);
    focusPane('games');
  };
  /** Enter or a click on a typed group: the demo jumps, a group with files
   * goes into them, anything else opens the game. */
  const activateGroup = (i: number, viaKey: boolean) => {
    const g = groups[i];
    if (!g || !shownGame) return;
    if (g.kind === 'demo') jumpTo(g.demo);
    else if (groupFiles(shownGame, g.kind).length > 0) {
      if (viaKey) focusPane('files');
    } else if (viaKey) onOpen(shownGame.id);
  };
  /** Enter on a file: the panel at that update or DLC, or the game. */
  const activateFile = (i: number) => {
    const f = files[i];
    if (!f || !shownGame) return;
    if (f.kind === 'child' && content?.onOpenChild)
      content.onOpenChild(shownGame.id, f.child.index);
    else onOpen(shownGame.id);
  };
  /** ↑↓/Home/End/Page, → in (a group with files) or the demo, ← back,
   * Enter: the game's typed groups and the files panes. */
  function onInnerKeys(
    kind: 'content' | 'files',
    ev: KeyboardEvent<HTMLDivElement>,
  ) {
    if (ev.target !== ev.currentTarget || ev.nativeEvent.isComposing) return;
    const inContent = kind === 'content';
    const index = inContent ? groupIndex : fileIndex;
    const count = inContent ? groups.length : files.length;
    const plain = !ev.ctrlKey && !ev.metaKey && !ev.altKey;
    if (plain && ev.key === 'ArrowLeft') {
      ev.preventDefault();
      const to = paneMove(
        ev.key,
        listPanes.indexOf(kind),
        listPanes.length,
        false,
      );
      if (to !== null) focusPane(listPanes[to]!);
      return;
    }
    if (plain && ev.key === 'ArrowRight') {
      ev.preventDefault();
      const g = groups[groupIndex];
      if (inContent && g?.kind === 'demo') jumpTo(g.demo);
      else if (inContent && files.length > 0) focusPane('files');
      return;
    }
    if (ev.key === 'Enter') {
      ev.preventDefault();
      if (inContent) activateGroup(index, true);
      else activateFile(index);
      return;
    }
    if (!LIST_KEYS.has(ev.key) || ev.altKey) return;
    const next = nextIndex(ev.key, index, count, 1, pageRows);
    if (next === null) return;
    ev.preventDefault();
    if (inContent) chooseGroup(next);
    else chooseFile(next);
  }

  const shown = gameIndex >= 0 ? games[gameIndex] : undefined;
  const shownSystem = shown ? systems[shown.system] : undefined;

  return (
    <div
      ref={rootRef}
      role="group"
      aria-label={t('layouts.announce', { view: t('layouts.columns') })}
      data-view-root=""
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
                aria-setsize={shownSystems.length}
                aria-posinset={i + 1}
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
                aria-setsize={folderRows.length}
                aria-posinset={i + 1}
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
              aria-setsize={games.length}
              aria-posinset={i + 1}
              role="option"
              aria-selected={i === gameIndex}
              data-index={i}
              data-entry-id={entry.id}
              className="miller__row"
              style={style}
              title={entry.rel_path}
            >
              <span className="miller__name">{entry.name}</span>
              {rowSuffix(gameOf(entry.id), t).map((part) => (
                <span
                  key={part.text}
                  className={cn(
                    'miller__suffix',
                    part.warning && 'miller__suffix--warning',
                  )}
                >
                  {part.text}
                </span>
              ))}
              {groupsOf(gameOf(entry.id)).length > 0 && (
                <ChevronRight
                  aria-hidden="true"
                  className="miller__chevron"
                  size={14}
                />
              )}
            </div>
          );
        }}
      />
      {shownGame && groups.length > 0 && (
        <PaneList
          kind="content"
          listKey={String(shownGame.id)}
          label={t('layouts.pane.content')}
          count={groups.length}
          activeIndex={groupIndex}
          optionId={(i) => `${baseId}-g${i}`}
          getKey={(i) => groups[i]?.kind ?? i}
          tabbable={activePane === 'content'}
          current={activePane === 'content'}
          register={register}
          onFocus={setPane}
          onKeyDown={(ev) => onInnerKeys('content', ev)}
          onClick={(ev) => {
            const i = rowOf(ev.target);
            focusPane('content');
            if (i === null) return;
            chooseGroup(i);
            activateGroup(i, false);
          }}
          renderOption={(i, style) => {
            const g = groups[i];
            if (!g) return null;
            const hasFiles = groupFiles(shownGame, g.kind).length > 0;
            const demo =
              g.kind === 'demo' ? entries[indexOf.get(g.demo) ?? -1] : null;
            return (
              <div
                key={g.kind}
                id={`${baseId}-g${i}`}
                aria-setsize={groups.length}
                aria-posinset={i + 1}
                role="option"
                aria-selected={i === groupIndex}
                data-index={i}
                className={cn(
                  'miller__row',
                  'separated' in g && 'miller__row--separated',
                )}
                style={style}
                title={
                  demo
                    ? t('content.columns.demoLink', { name: demo.name })
                    : undefined
                }
              >
                <span className="miller__name miller__label">
                  {t(`content.columns.group.${g.kind}`)}
                </span>
                {'count' in g && (
                  <span className="miller__count">
                    {number.format(g.count)}
                  </span>
                )}
                {hasFiles && (
                  <ChevronRight
                    aria-hidden="true"
                    className="miller__chevron"
                    size={14}
                  />
                )}
                {demo && (
                  <ArrowUpRight
                    aria-hidden="true"
                    className="miller__chevron"
                    size={14}
                  />
                )}
              </div>
            );
          }}
        />
      )}
      {shownGame && files.length > 0 && (
        <PaneList
          kind="files"
          listKey={`${shownGame.id}:${group?.kind ?? ''}`}
          label={t('layouts.pane.files')}
          count={files.length}
          activeIndex={fileIndex}
          optionId={(i) => `${baseId}-x${i}`}
          getKey={(i) => {
            const f = files[i];
            return f
              ? f.kind === 'child'
                ? f.child.rel_path
                : f.entry.rel_path
              : i;
          }}
          tabbable={activePane === 'files'}
          current={activePane === 'files'}
          register={register}
          onFocus={setPane}
          onKeyDown={(ev) => onInnerKeys('files', ev)}
          onClick={(ev) => {
            const i = rowOf(ev.target);
            focusPane('files');
            if (i !== null) chooseFile(i);
          }}
          renderOption={(i, style) => {
            const f = files[i];
            if (!f) return null;
            const note =
              f.kind === 'revision'
                ? f.isDefault
                  ? t('content.columns.default')
                  : null
                : f.child.role === 'update' &&
                    f.child.version !== undefined &&
                    f.child.version === shownGame.installed
                  ? t('content.newest')
                  : null;
            return (
              <div
                key={i}
                id={`${baseId}-x${i}`}
                aria-setsize={files.length}
                aria-posinset={i + 1}
                role="option"
                aria-selected={i === fileIndex}
                data-index={i}
                className="miller__row"
                style={style}
                title={f.kind === 'child' ? f.child.rel_path : f.entry.rel_path}
              >
                <span className="miller__name miller__label">{f.name}</span>
                {note !== null && <span className="miller__count">{note}</span>}
              </div>
            );
          }}
        />
      )}
      <div
        role="region"
        aria-labelledby={previewHeadId}
        className="miller__pane miller__pane--preview"
      >
        <div className="miller__head" id={previewHeadId}>
          {t('layouts.pane.preview')}
        </div>
        {shownGame && file ? (
          <FilePreview
            file={file}
            game={shownGame}
            generation={generation}
            systemId={shownSystem?.system_id ?? null}
            media={prefs.art}
          />
        ) : shown ? (
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

/**
 * The preview of an update, a DLC or a revision (§8.5): its art, name,
 * «Archivo», «Tamaño» and, for an update or DLC, where its art comes from
 * («Propio, incluido en el paquete», «Del juego base», «Sin arte»).
 */
function FilePreview({
  file,
  game,
  generation,
  systemId,
  media,
}: {
  file: GroupFile;
  game: Game;
  generation: number;
  systemId: string | null;
  media: ViewProps['prefs']['art'];
}) {
  const t = useT();
  const locale = useContext(LocaleContext);
  if (file.kind === 'revision') {
    return (
      <div className="miller__preview">
        <div
          className={cn(
            'miller__art',
            media === 'cover' ? 'miller__art--cover' : 'miller__art--wide',
          )}
        >
          <ArtThumb generation={generation} entry={file.entry} media={media} />
        </div>
        <h3 className="miller__title">{file.entry.name}</h3>
        <dl className="drawer-facts drawer-facts--compact">
          <dt>{t('content.columns.file')}</dt>
          <dd className="break-all font-mono">{file.entry.rel_path}</dd>
          <dt>{t('columns.size')}</dt>
          <dd>{formatSize(file.entry.size, locale)}</dd>
        </dl>
      </div>
    );
  }
  const { child } = file;
  const state = childArtState(child, systemId);
  const artText =
    state.kind === 'own'
      ? state.origin !== undefined
        ? t('content.columns.artOwn', {
            origin: t(`content.origin.${ORIGIN_KEY[state.origin]}`),
          })
        : t('content.columns.artOwnBare')
      : state.kind === 'base'
        ? t('content.columns.artBase')
        : t('content.columns.artNone');
  return (
    <div className="miller__preview">
      <ChildThumb
        generation={generation}
        entry={game.entry}
        child={child}
        systemId={systemId}
        className="miller__child-art"
      />
      <h3 className="miller__title">{child.name}</h3>
      <dl className="drawer-facts drawer-facts--compact">
        <dt>{t('content.columns.file')}</dt>
        <dd className="break-all font-mono">{child.rel_path}</dd>
        <dt>{t('columns.size')}</dt>
        <dd>{formatSize(child.size, locale)}</dd>
        <dt>{t('content.columns.art')}</dt>
        <dd className={cn(state.kind === 'missing' && 'miller__warning')}>
          {artText}
        </dd>
      </dl>
    </div>
  );
}
