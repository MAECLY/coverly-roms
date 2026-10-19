/**
 * Shelves (spec §B.4.6): shelf metrics and where "See all" leads.
 */
import { REGIONS, type Region, type SystemView } from '@/ipc/types';

import { COVER_GAP, COVER_SIZES } from './coverSizes';
import type { ArtFilter } from './filters';
import type { Section } from './grouping';
import type { Scope } from './scope';
import type { CoverSize } from './viewPrefs';

/** The section header row height every view uses (spec §B.7). */
const HEADER_H = 32;
const GAP = COVER_GAP;

/** A shelf shows at most this many games, then a "See all {n}" cell. */
export const SHELF_LIMIT = 60;

/** One shelf: its header rule, then a strip one tile tall with a gap above
 *  and below (fixed per size stop, spec §B.7). */
export function shelfHeight(size: CoverSize): number {
  return HEADER_H + COVER_SIZES[size].tileH + 2 * GAP;
}

/** What "See all" narrows the library to for a shelf (spec §B.4.6). */
export interface SeeAllTarget {
  scope?: Scope;
  art?: ArtFilter;
  region?: Region;
}

const ART_FILTER_OF: Readonly<Record<string, ArtFilter>> = {
  missingCover: 'missing-cover',
  complete: 'complete',
};

/** The scope or filter that shows exactly (or, for regions, at least) a
 *  shelf's games. Groups no filter can express (letters, "other art
 *  missing", no region) return nothing: only the view changes. */
export function seeAllTarget(
  section: Section,
  systems: readonly SystemView[],
): SeeAllTarget {
  const [kind, ...rest] = section.key.split(':');
  const value = rest.join(':');
  switch (kind) {
    case 'system':
      return { scope: { kind: 'system', system: Number(value) } };
    case 'folder': {
      const [index, ...relParts] = rest;
      const rel = relParts.join(':');
      const system = systems.find((s) => s.index === Number(index));
      if (!system) return {};
      return {
        scope: {
          kind: 'folder',
          path: rel === '' ? system.dir : `${system.dir}/${rel}`,
        },
      };
    }
    case 'region':
      return (REGIONS as readonly string[]).includes(value)
        ? { region: value as Region }
        : {};
    case 'art': {
      const art = ART_FILTER_OF[value];
      return art ? { art } : {};
    }
    default:
      return {};
  }
}

/** The shelf holding entry `index` (sections are ascending ranges). */
export function shelfOfIndex(
  sections: readonly Section[],
  index: number,
): number {
  let lo = 0;
  let hi = sections.length - 1;
  let hit = -1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (sections[mid]!.start <= index) {
      hit = mid;
      lo = mid + 1;
    } else {
      hi = mid - 1;
    }
  }
  return hit;
}
