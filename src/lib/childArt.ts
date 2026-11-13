/**
 * What an update's or DLC's art looks like in the views (addon views §8.3,
 * §8.6, §11): its own icon (read in the clear from its package, or found
 * by the user), the base game's art dimmed when its system cannot carry
 * art in the clear, or the dashed missing slot.
 */
import type { ChildMedia, ChildOrigin, ChildView } from '@/ipc/types';

import type { Game } from './games';

export type ChildArtState =
  | { kind: 'own'; slot: ChildMedia; origin?: ChildOrigin }
  | { kind: 'base' }
  | { kind: 'missing' };

/**
 * Systems whose update and DLC packages never carry art in the clear, so
 * a child with no art of its own shows the base game's: Switch NSP/XCI
 * content is encrypted NCAs (spec §11, «Switch DLC»), and keys are never
 * read.
 */
const INHERITS_BASE_ART: ReadonlySet<string> = new Set(['switch']);

/** The square icon first (the thumbnails are square), then the banner. */
export function childArtState(
  child: ChildView,
  systemId: string | null | undefined,
): ChildArtState {
  for (const slot of ['icon', 'banner'] as const) {
    if (!child.art[slot]) continue;
    const origin = child.art[`${slot}_origin`];
    return origin === undefined
      ? { kind: 'own', slot }
      : { kind: 'own', slot, origin };
  }
  return systemId != null && INHERITS_BASE_ART.has(systemId)
    ? { kind: 'base' }
    : { kind: 'missing' };
}

/** The `content.origin.*` key of a child art origin. */
export const ORIGIN_KEY: Readonly<Record<ChildOrigin, string>> = {
  package: 'package',
  screenscraper: 'ss',
  steamgriddb: 'sgdb',
  user: 'user',
};

/**
 * The Galería tray (§8.3): each update with its own art (newest first, as
 * Rust sent them), then every DLC.
 */
export function trayChildren(game: Game): ChildView[] {
  return [
    ...game.updates.filter((u) => u.art.icon || u.art.banner),
    ...game.dlc,
  ];
}

/** The shared object-URL cache key of a child's art slot. */
export function childArtKey(
  generation: number,
  entryId: number,
  index: number,
  slot: ChildMedia,
): string {
  return `child:${generation}:${entryId}:${index}:${slot}`;
}
