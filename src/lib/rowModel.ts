/**
 * Flat rows for the virtualisers (spec §B.7): header rows and rows of
 * `perRow` items. Built from section ranges only, so a column-count change
 * (resize, size stop) never regroups the entries.
 */
import type { ChildView, EntryView } from '@/ipc/types';

import type { Game } from './games';
import { ALL_SECTION_KEY, type Section } from './grouping';

export type Row =
  | { kind: 'header'; section: Section; sectionIndex: number }
  | { kind: 'items'; start: number; end: number; sectionIndex: number };

export interface RowModel {
  rows: readonly Row[];
  /** Item index → row index (`-1` for an index outside every section). */
  rowOfIndex: Int32Array;
  /** Indices of the header rows, ascending (sticky headers, group jumps). */
  headerRows: readonly number[];
}

/** Sections that cover `count` entries exactly, or one section for all. */
export function coveringSections(
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

/** `perRow` items per row (1 for lists). With one 'none' section no header row is emitted. */
export function buildRowModel(
  sections: readonly Section[],
  perRow: number,
  withHeaders: boolean,
): RowModel {
  const step = Math.max(1, Math.floor(perRow) || 1);
  const headers =
    withHeaders &&
    !(sections.length === 1 && sections[0]?.key === ALL_SECTION_KEY);
  let total = 0;
  for (const s of sections) total = Math.max(total, s.start + s.count);
  const rowOfIndex = new Int32Array(total).fill(-1);
  const rows: Row[] = [];
  const headerRows: number[] = [];
  sections.forEach((section, sectionIndex) => {
    if (headers) {
      headerRows.push(rows.length);
      rows.push({ kind: 'header', section, sectionIndex });
    }
    const end = section.start + section.count;
    for (let start = section.start; start < end; start += step) {
      const stop = Math.min(end, start + step);
      rowOfIndex.fill(rows.length, start, stop);
      rows.push({ kind: 'items', start, end: stop, sectionIndex });
    }
  });
  return { rows, rowOfIndex, headerRows };
}

/**
 * ↑/↓ (and a page of them) in a grid of rows: the item in the same column
 * `delta` items rows away, skipping header rows. A shorter row (a section's
 * partial last row) clamps to its last item; at either end the move stops
 * at the furthest row there is, so the index stays put at the edge.
 */
export function moveVertical(
  model: RowModel,
  index: number,
  delta: number,
): number {
  const from = model.rowOfIndex[index] ?? -1;
  const row = model.rows[from];
  if (row?.kind !== 'items' || delta === 0) return index;
  const column = index - row.start;
  const dir = delta > 0 ? 1 : -1;
  let left = Math.abs(delta);
  let target = row;
  for (
    let r = from + dir;
    left > 0 && r >= 0 && r < model.rows.length;
    r += dir
  ) {
    const next = model.rows[r];
    if (next?.kind !== 'items') continue;
    target = next;
    left--;
  }
  return Math.min(target.start + column, target.end - 1);
}

/** Where a row sits in the treegrid (`aria-level`, `aria-setsize`,
 *  `aria-posinset`) and the row index of its parent (-1 at the top). */
interface TreePlace {
  /** Unique within the model; the DOM row carries it (`data-row-key`). */
  key: string;
  level: number;
  setsize: number;
  posinset: number;
  parent: number;
}

/** What a game holds, as the rows under it (§8.4). `index` is the game's
 *  entry index in the view's `entries`. */
export type ContentRow =
  | { kind: 'ghost'; index: number; version: string; entry: number }
  | { kind: 'child'; index: number; child: ChildView; newest: boolean }
  | { kind: 'revision'; index: number; entry: EntryView }
  | { kind: 'demo'; index: number; demo: number }
  | {
      kind: 'folder';
      index: number;
      addons: NonNullable<EntryView['addons']>;
    };

export type TreeRow = TreePlace &
  (
    | { kind: 'section'; section: Section; sectionIndex: number }
    | {
        kind: 'game';
        index: number;
        /** The game's entry id (the key of `expanded`). */
        id: number;
        /** `null` when the view has no games (entries only). */
        game: Game | null;
        expandable: boolean;
        expanded: boolean;
      }
    | ContentRow
  );

export interface TreeModel {
  rows: readonly TreeRow[];
  /** Entry index → its game row index. */
  gameRow: Int32Array;
  /** Indices of the section rows, ascending. */
  headerRows: readonly number[];
}

/**
 * The rows under a game, in the approved order: a newer update that is
 * not in the game's folder (the ghost row) first, then its updates newest
 * first, its DLC, its other revisions, its demos and its folder of mods,
 * cheats, extras and saves. «La más nueva» marks the update that is the
 * installed version (Rust, D5), and none while a newer one is missing.
 */
export function contentRowsOf(game: Game, index: number): ContentRow[] {
  const rows: ContentRow[] = [];
  if (game.missing)
    rows.push({
      kind: 'ghost',
      index,
      version: game.missing.version,
      entry: game.missing.entry,
    });
  let newestSeen = game.missing !== null;
  for (const child of game.updates) {
    const newest =
      !newestSeen &&
      child.version !== undefined &&
      child.version === game.installed;
    if (newest) newestSeen = true;
    rows.push({ kind: 'child', index, child, newest });
  }
  for (const child of game.dlc)
    rows.push({ kind: 'child', index, child, newest: false });
  for (const entry of game.revisions.slice(1))
    rows.push({ kind: 'revision', index, entry });
  for (const demo of game.demos) rows.push({ kind: 'demo', index, demo });
  const addons = game.entry.addons;
  if (addons && addons.mods + addons.cheats + addons.extras + addons.saves > 0)
    rows.push({ kind: 'folder', index, addons });
  return rows;
}

function contentKey(row: ContentRow, gameId: number): string {
  switch (row.kind) {
    case 'ghost':
      return `x:${gameId}`;
    case 'child':
      return `c:${gameId}:${row.child.index}`;
    case 'revision':
      return `r:${row.entry.id}`;
    case 'demo':
      return `d:${gameId}:${row.demo}`;
    case 'folder':
      return `f:${gameId}`;
  }
}

/**
 * The Details treegrid's rows (§8.4): section rows when grouped (level 1),
 * one row per game, and under each expanded game its content (one level
 * down). Pure and O(rows); rebuilt only when the games, their order or the
 * expanded set change. `expanded` holds game entry ids.
 */
export function buildTreeModel(
  sections: readonly Section[],
  entries: readonly EntryView[],
  byEntry: ReadonlyMap<number, Game> | undefined,
  expanded: ReadonlySet<number>,
  withHeaders: boolean,
): TreeModel {
  const headers =
    withHeaders &&
    !(sections.length === 1 && sections[0]?.key === ALL_SECTION_KEY);
  const gameRow = new Int32Array(entries.length).fill(-1);
  const rows: TreeRow[] = [];
  const headerRows: number[] = [];
  const gameLevel = headers ? 2 : 1;
  sections.forEach((section, sectionIndex) => {
    let parent = -1;
    if (headers) {
      parent = rows.length;
      headerRows.push(rows.length);
      rows.push({
        kind: 'section',
        key: `s:${section.key}`,
        section,
        sectionIndex,
        level: 1,
        setsize: sections.length,
        posinset: sectionIndex + 1,
        parent: -1,
      });
    }
    const end = Math.min(entries.length, section.start + section.count);
    for (let i = section.start; i < end; i++) {
      const entry = entries[i]!;
      const game = byEntry?.get(entry.id) ?? null;
      const content = game ? contentRowsOf(game, i) : [];
      const open = content.length > 0 && expanded.has(entry.id);
      const at = rows.length;
      gameRow[i] = at;
      rows.push({
        kind: 'game',
        key: `g:${entry.id}`,
        index: i,
        id: entry.id,
        game,
        expandable: content.length > 0,
        expanded: open,
        level: gameLevel,
        setsize: section.count,
        posinset: i - section.start + 1,
        parent,
      });
      if (!open) continue;
      content.forEach((row, n) => {
        rows.push({
          ...row,
          key: contentKey(row, entry.id),
          level: gameLevel + 1,
          setsize: content.length,
          posinset: n + 1,
          parent: at,
        });
      });
    }
  });
  return { rows, gameRow, headerRows };
}

/** What a key does in the treegrid: move to a row, or open or close a game. */
export type TreeMove =
  | { row: number }
  | { expand: number }
  | { collapse: number }
  | Record<string, never>;

/** The next focusable row from `from` going `dir`, `left` steps away (or
 *  as far as there are); section rows are skipped. */
function step(model: TreeModel, from: number, dir: 1 | -1, left: number) {
  let target = from;
  for (
    let r = from + dir;
    left > 0 && r >= 0 && r < model.rows.length;
    r += dir
  ) {
    if (model.rows[r]?.kind === 'section') continue;
    target = r;
    left--;
  }
  return target;
}

/**
 * The APG treegrid keys, row first (§8.4): ↓/↑ the next or previous
 * visible row, → expands a closed game or enters an open one, ← leaves
 * content for its game or closes an open game, Home/End the first or last
 * row, PageDown/PageUp `page` rows. Section rows never hold the focus.
 */
export function treeKey(
  model: TreeModel,
  at: number,
  key: string,
  page: number,
): TreeMove {
  const row = model.rows[at];
  if (!row) return {};
  const go = (to: number): TreeMove => (to === at ? {} : { row: to });
  switch (key) {
    case 'ArrowDown':
      return go(step(model, at, 1, 1));
    case 'ArrowUp':
      return go(step(model, at, -1, 1));
    case 'PageDown':
      return go(step(model, at, 1, Math.max(1, page)));
    case 'PageUp':
      return go(step(model, at, -1, Math.max(1, page)));
    case 'Home':
      return go(step(model, -1, 1, 1));
    case 'End':
      return go(step(model, model.rows.length, -1, 1));
    case 'ArrowRight':
      if (row.kind !== 'game' || !row.expandable) return {};
      return row.expanded ? { row: at + 1 } : { expand: row.id };
    case 'ArrowLeft':
      if (row.kind === 'game') return row.expanded ? { collapse: row.id } : {};
      if (row.kind === 'section' || row.parent < 0) return {};
      return { row: row.parent };
    default:
      return {};
  }
}
