import { useDeferredValue, useMemo, useState } from 'react';

import type { EntryView } from '@/ipc/types';
import { applyFilters, computeFacets, type Facets } from '@/lib/filters';
import { inScope } from '@/lib/scope';
import { sortEntries } from '@/lib/sort';

import { useFilterStore } from './filterStore';
import { useLibraryStore } from './libraryStore';
import { useViewStore } from './viewStore';

export interface VisibleEntries {
  /** In scope, filtered and sorted: what the grid/table shows. */
  visible: EntryView[];
  /** In scope, before filters. */
  scoped: EntryView[];
  /** Facet counts within the scope (filter panel). */
  facets: Facets;
  /** Unix seconds the date buckets are relative to. */
  now: number;
}

const NO_ENTRIES: EntryView[] = [];

export function useVisibleEntries(): VisibleEntries {
  const view = useLibraryStore((s) => s.view);
  const keys = useLibraryStore((s) => s.keys);
  const filters = useDeferredValue(useFilterStore((s) => s.filters));
  const scope = useViewStore((s) => s.scope);
  const sort = useViewStore((s) => s.sort);
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
  const visible = useMemo(
    () => sortEntries(applyFilters(scoped, keys, filters), sort, systemNames),
    [scoped, keys, filters, sort, systemNames],
  );
  const facets = useMemo(
    () => computeFacets(scoped, keys, filters),
    [scoped, keys, filters],
  );
  return { visible, scoped, facets, now };
}
