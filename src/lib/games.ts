/**
 * The game model of the six views (addon views D1, D4, D7).
 *
 * A game is one top-level entry that is not an orphan add-on, with its
 * updates and DLC (sent by Rust inside the entry) and, for retro
 * revisions, the other entries of its `version_group` folded under the
 * default revision. Demos are games of their own, linked to the full
 * game. Fill, gamelists, audit, Duplicados and Organizar keep working on
 * scan entries: this is only what the user sees.
 *
 * One pass over the entries; pure, so callers memoise it on the library
 * generation and the revision defaults.
 */
import type { ChildView, EntryView } from '@/ipc/types';

export interface Game {
  /** Representative entry id (the default revision's). */
  id: number;
  /** That entry. */
  entry: EntryView;
  /** Every member of its `version_group`, default first; `[entry]` otherwise. */
  revisions: readonly EntryView[];
  /** Updates and DLC, newest first as Rust sent them. */
  children: readonly ChildView[];
  updates: readonly ChildView[];
  dlc: readonly ChildView[];
  isDemo: boolean;
  /** Entry id of the full game, when the library has it. */
  demoOf: number | null;
  /** Entry ids of the demos linked to this game. */
  demos: readonly number[];
  /** The newest version in the library (Rust, D5). */
  installed: string | null;
  /** A newer update outside the game's folder (Rust, D6). */
  missing: { entry: number; version: string } | null;
  orphan: 'update' | 'dlc' | null;
}

export interface GameCounts {
  games: number;
  demos: number;
  updates: number;
  dlc: number;
  moreRevisions: number;
  orphans: number;
}

type RevisionKey =
  | { kind: 'untagged' }
  | { kind: 'parsed'; parts: number[] }
  | { kind: 'other' };

/** `Rev 1` → [1], `Rev A` → [1] (letters from 1), `v1.10` → [1, 10]. */
function revisionKey(revision: string | undefined): RevisionKey {
  if (revision === undefined || revision.trim() === '')
    return { kind: 'untagged' };
  const text = revision
    .trim()
    .replace(/^rev(ision)?\s*/i, '')
    .replace(/^v/i, '');
  if (/^\d+(\.\d+)*$/.test(text))
    return { kind: 'parsed', parts: text.split('.').map(Number) };
  if (/^[a-z]$/i.test(text))
    return { kind: 'parsed', parts: [text.toUpperCase().charCodeAt(0) - 64] };
  return { kind: 'other' };
}

/** Highest first: parsed revisions, then the untagged file, then the rest. */
const DESC_RANK = { parsed: 0, untagged: 1, other: 2 } as const;

const NAMES = new Intl.Collator('en', { numeric: true, sensitivity: 'base' });

/**
 * Display order of revisions, highest first: parsed revisions from the
 * highest, then the untagged file, then revisions that do not parse, by
 * name. The first one is the default when the user chose none.
 */
function byRevisionDesc(a: EntryView, b: EntryView): number {
  const ka = revisionKey(a.revision);
  const kb = revisionKey(b.revision);
  const rank = DESC_RANK[ka.kind] - DESC_RANK[kb.kind];
  if (rank !== 0) return rank;
  if (ka.kind === 'parsed' && kb.kind === 'parsed') {
    const n = Math.max(ka.parts.length, kb.parts.length);
    for (let i = 0; i < n; i++) {
      const d = (kb.parts[i] ?? 0) - (ka.parts[i] ?? 0);
      if (d !== 0) return d;
    }
  }
  return NAMES.compare(a.name, b.name) || a.id - b.id;
}

/** `a\b/c` and `a/b/c` compare equal: rel paths match by component. */
function samePath(a: string, b: string): boolean {
  const split = (p: string) => p.split(/[\\/]+/).filter((s) => s !== '');
  const pa = split(a);
  const pb = split(b);
  return pa.length === pb.length && pa.every((s, i) => s === pb[i]);
}

/** Highest first, the chosen revision moved to the front when it is a member. */
function orderRevisions(
  members: readonly EntryView[],
  chosen: string | undefined,
): EntryView[] {
  const sorted = [...members].sort(byRevisionDesc);
  if (chosen === undefined) return sorted;
  const at = sorted.findIndex((m) => samePath(m.rel_path, chosen));
  if (at <= 0) return sorted;
  const [pick] = sorted.splice(at, 1);
  return [pick!, ...sorted];
}

/**
 * The file that represents a group of revisions: the one the user chose
 * (by `rel_path`) when it is still a member, else the highest revision
 * (untagged < `Rev 1` < `Rev 2`, `Rev A` < `Rev B`, `v1.0` < `v1.1`;
 * revisions that do not parse never win over one that does).
 */
export function defaultRevision(
  members: readonly EntryView[],
  chosen?: string,
): EntryView {
  const first = orderRevisions(members, chosen)[0];
  if (!first) throw new Error('defaultRevision needs at least one member');
  return first;
}

function makeGame(
  entry: EntryView,
  revisions: readonly EntryView[],
  demos: number[],
): Game {
  const children = entry.children ?? [];
  return {
    id: entry.id,
    entry,
    revisions,
    children,
    updates: children.filter((c) => c.role === 'update'),
    dlc: children.filter((c) => c.role === 'dlc'),
    isDemo: entry.demo !== undefined,
    demoOf: entry.demo?.of ?? null,
    demos,
    installed: entry.installed ?? null,
    missing: entry.newer_elsewhere ?? null,
    orphan: entry.orphan ?? null,
  };
}

/** Folds scan entries into games (D1). */
export function buildGames(
  entries: readonly EntryView[],
  revisionDefaults: Readonly<Record<string, string>>,
): {
  games: Game[];
  orphans: Game[];
  byEntry: ReadonlyMap<number, Game>;
} {
  // Pass 1: the members of each version group.
  const groups = new Map<string, EntryView[]>();
  for (const e of entries) {
    if (e.orphan !== undefined || e.version_group === undefined) continue;
    const members = groups.get(e.version_group);
    if (members) members.push(e);
    else groups.set(e.version_group, [e]);
  }

  // Pass 2: one game per plain entry and per group, where its first file is.
  const games: Game[] = [];
  // The demo lists are filled in pass 3, before anyone sees the games.
  const demosOf = new Map<Game, number[]>();
  const orphans: Game[] = [];
  const byEntry = new Map<number, Game>();
  const done = new Set<string>();
  for (const e of entries) {
    if (e.orphan !== undefined) {
      const game = makeGame(e, [e], []);
      orphans.push(game);
      byEntry.set(e.id, game);
      continue;
    }
    const group = e.version_group;
    const members = group === undefined ? undefined : groups.get(group);
    if (group === undefined || !members || members.length < 2) {
      const demos: number[] = [];
      const game = makeGame(e, [e], demos);
      games.push(game);
      demosOf.set(game, demos);
      byEntry.set(e.id, game);
      continue;
    }
    if (done.has(group)) continue;
    done.add(group);
    const chosen = Object.prototype.hasOwnProperty.call(revisionDefaults, group)
      ? revisionDefaults[group]
      : undefined;
    const revisions = orderRevisions(members, chosen);
    const demos: number[] = [];
    const game = makeGame(revisions[0]!, revisions, demos);
    games.push(game);
    demosOf.set(game, demos);
    for (const m of members) byEntry.set(m.id, game);
  }

  // Pass 3: each demo is listed on its full game.
  for (const game of games) {
    if (game.demoOf === null) continue;
    const full = byEntry.get(game.demoOf);
    if (full && full !== game) demosOf.get(full)?.push(game.id);
  }

  return { games, orphans, byEntry };
}

/**
 * What the status bar counts (D4): games are the non-orphan games, demos
 * included; updates, DLC, revisions beyond the default and orphans are
 * counted apart and never as games.
 */
export function gameCounts(
  games: readonly Game[],
  orphans: readonly Game[],
): GameCounts {
  let demos = 0;
  let updates = 0;
  let dlc = 0;
  let moreRevisions = 0;
  for (const g of games) {
    if (g.isDemo) demos++;
    updates += g.updates.length;
    dlc += g.dlc.length;
    moreRevisions += g.revisions.length - 1;
  }
  return {
    games: games.length,
    demos,
    updates,
    dlc,
    moreRevisions,
    orphans: orphans.length,
  };
}
