import { useDeferredValue, useEffect, useMemo, useState } from 'react';

import { REGIONS, type EntryView } from '@/ipc/types';
import { applyFilters, computeFacets, type Facets } from '@/lib/filters';
import {
  buildGames,
  gameCounts,
  type Game,
  type GameCounts,
} from '@/lib/games';
import {
  artGroupOf,
  groupEntries,
  type ArtGroup,
  type Section,
} from '@/lib/grouping';
import { inScope, resolveScope } from '@/lib/scope';
import { sortEntries } from '@/lib/sort';

import { useFilterStore } from './filterStore';
import { isRunning, useJobStore } from './jobStore';
import { useLibraryStore } from './libraryStore';
import { useViewStore } from './viewStore';

export interface VisibleEntries {
  /** The representative entry of each game in `visibleGames`, in the
   * same order: what every view shows, in the order it shows it. */
  visible: readonly EntryView[];
  /** In scope, filtered, sorted and grouped (groups first): the games
   * every view shows (addon views D1). */
  visibleGames: readonly Game[];
  /** Index ranges of the groups in `visible` (one `all` section ungrouped). */
  sections: readonly Section[];
  /** Representative entries in scope, before filters. */
  scoped: EntryView[];
  /** Facet counts within the scope (filter panel), one per game. */
  facets: Facets;
  /** Every game of the library that is not an orphan, in library order. */
  games: readonly Game[];
  /** Updates and DLC whose game is not in their folder (D13). */
  orphans: readonly Game[];
  /** What the library holds (D4): games, demos, and inside them updates,
   * DLC and more revisions, which never count as games. */
  counts: GameCounts;
  /** The same counts for the shown games only (orphans: the library's). */
  shownCounts: GameCounts;
  /** Every entry id (each revision, each orphan) → its game. */
  byEntry: ReadonlyMap<number, Game>;
  /** Unix seconds the date buckets are relative to. */
  now: number;
}

const NO_ENTRIES: EntryView[] = [];

interface FrozenArt {
  map: ReadonlyMap<number, ArtGroup>;
}

export function useVisibleEntries(): VisibleEntries {
  const view = useLibraryStore((s) => s.view);
  const keys = useLibraryStore((s) => s.keys);
  const revisionDefaults = useLibraryStore((s) => s.revisionDefaults);
  const filters = useDeferredValue(useFilterStore((s) => s.filters));
  const storedScope = useViewStore((s) => s.scope);
  const setScope = useViewStore((s) => s.setScope);
  const sort = useViewStore((s) => s.sort);
  const groupBy = useViewStore((s) => s.prefs.groupBy);
  const running = useJobStore((s) => isRunning(s.job));
  const [now] = useState(() => Math.floor(Date.now() / 1000));
  const entries = view?.entries ?? NO_ENTRIES;

  // Games are folded once per library change and revision choice, never
  // on a keystroke in the search box (10 000-game libraries, D1).
  const { games, orphans, byEntry } = useMemo(
    () => buildGames(entries, revisionDefaults),
    [entries, revisionDefaults],
  );
  const counts = useMemo(() => gameCounts(games, orphans), [games, orphans]);

  // «Complementos sin juego base» once the last orphan is gone: all (D13).
  const scope = resolveScope(storedScope, orphans.length);
  useEffect(() => {
    if (scope !== storedScope) setScope(scope);
  }, [scope, storedScope, setScope]);

  const scoped = useMemo(() => {
    const pool = scope.kind === 'orphans' ? orphans : games;
    const out: EntryView[] = [];
    for (const g of pool) if (inScope(g.entry, scope, now)) out.push(g.entry);
    return out;
  }, [games, orphans, scope, now]);
  const systemNames = useMemo(
    () => view?.systems.map((s) => s.name) ?? [],
    [view],
  );
  // Games sort by their representative entry; a game's updates and DLC
  // stay in the order Rust sent them.
  const sorted = useMemo(
    () => sortEntries(applyFilters(scoped, keys, filters), sort, systemNames),
    [scoped, keys, filters, sort, systemNames],
  );

  // Art groups stay as they were when a fill started, so applied art does
  // not move games under the cursor; they are recomputed when it ends.
  const artMedia = view?.profile.media;
  const freeze = running && groupBy === 'art';
  const [frozen, setFrozen] = useState<FrozenArt | null>(null);
  if (freeze && !frozen) {
    const media = artMedia ?? [];
    setFrozen({
      map: new Map(entries.map((e) => [e.id, artGroupOf(e, media)])),
    });
  } else if (!freeze && frozen) {
    setFrozen(null);
  }
  const frozenArt = freeze ? (frozen?.map ?? null) : null;

  const systems = view?.systems;
  const grouped = useMemo(
    () =>
      groupEntries(sorted, groupBy, {
        systems: systems ?? [],
        regionOrder: REGIONS,
        artMedia: artMedia ?? [],
        frozenArt,
      }),
    [sorted, groupBy, systems, artMedia, frozenArt],
  );
  const visibleGames = useMemo(
    () => grouped.ordered.map((e) => byEntry.get(e.id)!),
    [grouped, byEntry],
  );
  const shownCounts = useMemo(() => {
    if (scope.kind === 'orphans') return gameCounts([], orphans);
    if (visibleGames.length === games.length) return counts;
    return gameCounts(visibleGames, orphans);
  }, [scope, visibleGames, games, orphans, counts]);
  const facets = useMemo(
    () => computeFacets(scoped, keys, filters),
    [scoped, keys, filters],
  );
  return {
    visible: grouped.ordered,
    visibleGames,
    sections: grouped.sections,
    scoped,
    facets,
    games,
    orphans,
    counts,
    shownCounts,
    byEntry,
    now,
  };
}
