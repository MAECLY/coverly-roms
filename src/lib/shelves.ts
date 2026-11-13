/**
 * Shelves (spec §B.4.6): shelf metrics and where "See all" leads; one box
 * per game with its DLC behind it or beside it (addon views §8.7).
 */
import { REGIONS, type Region, type SystemView } from '@/ipc/types';

import { COVER_GAP, COVER_SIZES, metrics } from './coverSizes';
import type { ArtFilter } from './filters';
import type { Game } from './games';
import type { Section } from './grouping';
import type { Scope } from './scope';
import type { CoverSize, ShelfAddons } from './viewPrefs';

/** The section header row height every view uses (spec §B.7). */
const HEADER_H = 32;
const GAP = COVER_GAP;

/** A shelf shows at most this many games, then a "See all {n}" cell. */
export const SHELF_LIMIT = 60;

/** One shelf: its header rule, then a strip one tile tall with a gap above
 *  and below (fixed per size stop, spec §B.7). `content` is the library
 *  flag that gives M, L and XL tiles their add-on summary line (§8.2). */
export function shelfHeight(size: CoverSize, content = false): number {
  return HEADER_H + metrics(size, content).tileH + 2 * GAP;
}

/** At most this many DLC edges peek out behind a box; the last one then
 *  carries «+n» (§8.7). */
export const SHELF_EDGES_MAX = 3;
/** Each edge sits this much further right and up than the one before it,
 *  fanned out (hover or selection); at rest it is `SHELF_FAN` px closer. */
export const SHELF_EDGE_X = 9;
export const SHELF_EDGE_Y = 7;
/** How far the edges fan out on hover or selection (D17). */
export const SHELF_FAN = 3;

export interface ShelfStack {
  /** Edges drawn behind the box, 0–3. */
  edges: number;
  /** DLC past the third edge, shown as «+n» on it. */
  more: number;
}

/** The edges behind a box with `dlc` DLC. */
export function shelfStack(dlc: number): ShelfStack {
  const edges = Math.min(Math.max(0, dlc), SHELF_EDGES_MAX);
  return { edges, more: Math.max(0, dlc) - edges };
}

/**
 * The DLC a game's box carries: a full game's own. A demo is a box of its
 * own and an add-on without its game has nothing behind it.
 */
export function shelfDlc(game: Game | undefined): number {
  if (!game || game.isDemo || game.orphan !== null) return 0;
  return game.dlc.length;
}

/** The DLC icons beside a box: 24 px up to M, 48 px at L and XL. */
export function besideIconSize(size: CoverSize): 24 | 48 {
  return size === 'l' || size === 'xl' ? 48 : 24;
}

/** Between the cover and the hairline, and the hairline and the icons. */
const BESIDE_GAP = 6;
const BESIDE_RULE = 1;
/** Between two icons in the column. */
export const BESIDE_ICON_GAP = 4;

/** The width the column of icons adds to a box (gap, rule, gap, icon). */
export function besideWidth(size: CoverSize): number {
  return BESIDE_GAP + BESIDE_RULE + BESIDE_GAP + besideIconSize(size);
}

/**
 * A box's width on the shelf: the tile's, plus the column of DLC icons
 * when they stand beside it. Behind the box, the edges take room inside
 * the art box, so every tile keeps its width.
 */
export function shelfBoxWidth(
  size: CoverSize,
  mode: ShelfAddons,
  dlc: number,
): number {
  const tileW = COVER_SIZES[size].tileW;
  return mode === 'beside' && dlc > 0 ? tileW + besideWidth(size) : tileW;
}

/**
 * The icons beside a box of art height `artH`: as many as fit, and when
 * some do not, one fewer and a «+n» cell in the last place.
 */
export function besideIcons(
  size: CoverSize,
  dlc: number,
  artH: number,
): { shown: number; more: number } {
  const step = besideIconSize(size) + BESIDE_ICON_GAP;
  const fit = Math.max(1, Math.floor((artH + BESIDE_ICON_GAP) / step));
  if (dlc <= fit) return { shown: Math.max(0, dlc), more: 0 };
  return { shown: fit - 1, more: dlc - (fit - 1) };
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
