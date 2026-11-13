import type { ChildView, EntryView } from '@/ipc/types';

import type { Game } from './games';

export interface FolderNode {
  /** `/`-separated path from the library root (`''` for the root). */
  path: string;
  name: string;
  /** Entries in this folder and below. */
  count: number;
  children: FolderNode[];
}

const collator = new Intl.Collator(undefined, {
  sensitivity: 'base',
  numeric: true,
});

function sortTree(node: FolderNode): void {
  node.children.sort((a, b) => collator.compare(a.name, b.name));
  node.children.forEach(sortTree);
}

/** The folder tree of the entries' `dir`s, with recursive counts. */
export function buildFolderTree(
  entries: readonly { dir: string }[],
): FolderNode {
  const root: FolderNode = { path: '', name: '', count: 0, children: [] };
  const index = new Map<string, FolderNode>([['', root]]);
  for (const e of entries) {
    root.count += 1;
    if (!e.dir) continue;
    let parent = root;
    let path = '';
    for (const part of e.dir.split('/')) {
      path = path ? `${path}/${part}` : part;
      let node = index.get(path);
      if (!node) {
        node = { path, name: part, count: 0, children: [] };
        index.set(path, node);
        parent.children.push(node);
      }
      node.count += 1;
      parent = node;
    }
  }
  sortTree(root);
  return root;
}

// ---------------------------------------------------------------------------
// Game nodes (addon views §8.5). Columns stays a folder tree, but a game
// folder (its base is a game with updates, DLC or Mods/Cheats/Extras/Saves
// folders) is listed as its game, at the folder above it, instead of as a
// folder of its own; that game, and a loose game with revisions, opens
// into typed groups instead of raw sub-folders.

/** A typed group inside a game, in the column's order. */
export type GameGroup =
  | { kind: 'base' }
  | { kind: 'updates' | 'dlc' | 'revisions'; count: number }
  /** A link to the demo's own game (its entry id). */
  | { kind: 'demo'; demo: number }
  /** A fixed folder: read-only counts, not navigable in part C. The first
   * one is drawn after a separator. */
  | {
      kind: FixedGroupKind;
      count: number;
      separated?: true;
    };

export type FixedGroupKind = 'cheats' | 'mods' | 'extras' | 'saves';

export type GameGroupKind = GameGroup['kind'];

/** The fixed folders after the separator, in the column's order. */
const FIXED_GROUPS: readonly FixedGroupKind[] = [
  'cheats',
  'mods',
  'extras',
  'saves',
];

/**
 * A game's typed groups: «Juego base», «Actualizaciones (n)», «DLC (n)»,
 * «Revisiones (n)», «Demo», then «Trucos», «Mods», «Extras», «Partidas»
 * with their counts. Empty groups are left out.
 */
export function gameGroups(game: Game): GameGroup[] {
  const groups: GameGroup[] = [{ kind: 'base' }];
  if (game.updates.length > 0)
    groups.push({ kind: 'updates', count: game.updates.length });
  if (game.dlc.length > 0) groups.push({ kind: 'dlc', count: game.dlc.length });
  if (game.revisions.length > 1)
    groups.push({ kind: 'revisions', count: game.revisions.length });
  const demo = game.demos[0];
  if (demo !== undefined) groups.push({ kind: 'demo', demo });
  const addons = game.entry.addons;
  let first = true;
  for (const kind of FIXED_GROUPS) {
    const count = addons?.[kind] ?? 0;
    if (count <= 0) continue;
    groups.push(first ? { kind, count, separated: true } : { kind, count });
    first = false;
  }
  return groups;
}

/** The game opens into typed groups: it has more than its base file. */
export function isGameNode(game: Game): boolean {
  return gameGroups(game).length > 1;
}

/**
 * The game's folder is a game folder: its updates and DLC sit in its
 * `Updates/` and `DLC/`, or Rust counted its fixed folders.
 */
export function isGameFolder(game: Game): boolean {
  return game.children.length > 0 || game.entry.addons !== undefined;
}

/**
 * The folder Columns lists an entry under (`/`-separated, relative to the
 * system folder): a game folder's base goes up to the folder that holds the
 * game folder, so the game folder is the game, not a folder; anything else
 * stays where it is.
 */
export function columnDir(rel: string, game: Game | undefined): string {
  if (!game || !isGameFolder(game) || rel === '') return rel;
  const slash = rel.lastIndexOf('/');
  return slash < 0 ? '' : rel.slice(0, slash);
}

/** One file of a group, for the file list after the typed groups. */
export type GroupFile =
  | { kind: 'child'; name: string; child: ChildView }
  | {
      kind: 'revision';
      name: string;
      entry: EntryView;
      /** The revision the library shows and copies (D7). */
      isDefault: boolean;
    };

/**
 * The files of a group: updates newest first and DLC as Rust sent them,
 * revisions with the default first. Base, demo and the fixed folders have
 * no file list here.
 */
export function groupFiles(game: Game, kind: GameGroupKind): GroupFile[] {
  switch (kind) {
    case 'updates':
    case 'dlc':
      return (kind === 'updates' ? game.updates : game.dlc).map((child) => ({
        kind: 'child',
        name: child.name,
        child,
      }));
    case 'revisions':
      return game.revisions.map((entry, i) => ({
        kind: 'revision',
        name: entry.name,
        entry,
        isDefault: i === 0,
      }));
    default:
      return [];
  }
}
