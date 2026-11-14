/**
 * What the Extras viewer shows and how (extras spec D17, §8.3): the view
 * for a file, the files ←/→ step through, the zoom stops and the limits
 * past which a file is not fetched or decoded.
 */
import type { ExtraFileView, ExtrasView } from '@/ipc/types';
import { extOf, sameCategory } from '@/lib/extras';

/** Above this many pixels an image is not decoded here (D17). */
export const MAX_PIXELS = 100_000_000;

/** A file larger than this is never fetched into the viewer (D17). */
export const MAX_VIEW_BYTES = 200 * 1024 ** 2;

/** Zoom stops, as scales of the image's own pixels. */
const ZOOMS = [0.1, 0.25, 0.5, 0.75, 1, 1.5, 2, 3, 4, 6, 8] as const;

const TEXT = new Set(['txt', 'md', 'nfo']);

export type ViewRoute = 'image' | 'video' | 'pdf' | 'text' | 'unplayable';

/** Which view shows `file` (extras D17); audio belongs to the player. */
export function routeOf(file: ExtraFileView): ViewRoute {
  if (file.playable !== 'inline' || file.size > MAX_VIEW_BYTES)
    return 'unplayable';
  const ext = extOf(file.name);
  switch (file.kind) {
    case 'image':
      return 'image';
    case 'video':
      return 'video';
    case 'document':
      if (ext === 'pdf') return 'pdf';
      return TEXT.has(ext) ? 'text' : 'unplayable';
    default:
      return 'unplayable';
  }
}

/** What «Reproducir» queues in the mini player instead of the viewer. */
function plays(f: ExtraFileView): boolean {
  return f.kind === 'audio' && f.playable === 'inline';
}

/** The files ←/→ step through: `file`'s category in list order, without
 * the audio the mini player plays. */
export function viewerQueue(
  listing: ExtrasView,
  file: ExtraFileView,
): ExtraFileView[] {
  return listing.files.filter(
    (f) =>
      sameCategory(f.category, file.category) &&
      (f.index === file.index || !plays(f)),
  );
}

/** The next zoom stop from `zoom` (`null` = fitted at `fit`). */
export function zoomStep(zoom: number | null, dir: 1 | -1, fit: number) {
  const from = zoom ?? fit;
  if (dir > 0)
    return ZOOMS.find((z) => z > from + 1e-6) ?? ZOOMS[ZOOMS.length - 1]!;
  return [...ZOOMS].reverse().find((z) => z < from - 1e-6) ?? ZOOMS[0];
}
