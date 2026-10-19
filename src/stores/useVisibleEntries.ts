import { useDeferredValue, useMemo, useState } from 'react';

import { REGIONS, type EntryView } from '@/ipc/types';
import { applyFilters, computeFacets, type Facets } from '@/lib/filters';
import {
  artGroupOf,
  groupEntries,
  type ArtGroup,
  type Section,
} from '@/lib/grouping';
import { inScope } from '@/lib/scope';
import { sortEntries } from '@/lib/sort';

import { useFilterStore } from './filterStore';
import { isRunning, useJobStore } from './jobStore';
import { useLibraryStore } from './libraryStore';
import { useViewStore } from './viewStore';

export interface VisibleEntries {
  /** In scope, filtered, sorted and grouped (groups first): what every
   * view shows, in the order it shows it. */
  visible: readonly EntryView[];
  /** Index ranges of the groups in `visible` (one `all` section ungrouped). */
  sections: readonly Section[];
  /** In scope, before filters. */
  scoped: EntryView[];
  /** Facet counts within the scope (filter panel). */
  facets: Facets;
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
  const filters = useDeferredValue(useFilterStore((s) => s.filters));
  const scope = useViewStore((s) => s.scope);
  const sort = useViewStore((s) => s.sort);
  const groupBy = useViewStore((s) => s.prefs.groupBy);
  const running = useJobStore((s) => isRunning(s.job));
  const [now] = useState(() => Math.floor(Date.now() / 1000));
  const entries = view?.entries ?? NO_ENTRIES;

  const scoped = useMemo(
    () => entries.filter((e) => inScope(e, scope, now)),
    [entries, scope, now],
  );
  const systemNames = useMemo(
    () => view?.systems.map((s) => s.name) ?? [],
    [view],
  );
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
  const facets = useMemo(
    () => computeFacets(scoped, keys, filters),
    [scoped, keys, filters],
  );
  return {
    visible: grouped.ordered,
    sections: grouped.sections,
    scoped,
    facets,
    now,
  };
}
