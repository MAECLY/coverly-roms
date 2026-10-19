/**
 * Cover tile metrics per size stop (spec §B.4.1): Finder's icon-size
 * slider and Explorer's icon sizes, as five fixed stops so the grid stays
 * virtualised with fixed row heights.
 *
 * A tile is a 2px border and 6px padding on each side (16px in all), the
 * art box (3:4 of the tile width, the cover drawn inside it with
 * `object-fit: contain`), then — if any text shows — a 6px gap, the name
 * lines (16px each), the 18px system/region line and, at XL, a 16px file
 * size line.
 */
import { COVER_SIZE_IDS, type CoverSize } from './viewPrefs';

export interface CoverMetrics {
  tileW: number;
  tileH: number;
  artW: number;
  artH: number;
  nameLines: 0 | 1 | 2;
  /** 'badge': system as a corner badge on the art, no meta line. */
  meta: 'badge' | 'line' | 'line+size';
}

/** Border and padding, both sides. */
const FRAME = 16;
const NAME_GAP = 6;
const LINE_H = 16;
const META_H = 18;
const SIZE_LINE_H = 16;

function metrics(
  tileW: number,
  nameLines: CoverMetrics['nameLines'],
  meta: CoverMetrics['meta'],
): CoverMetrics {
  const artH = Math.round((tileW * 4) / 3);
  const text = nameLines * LINE_H + (meta === 'badge' ? 0 : META_H);
  const tileH =
    FRAME +
    artH +
    (text > 0 ? NAME_GAP : 0) +
    text +
    (meta === 'line+size' ? SIZE_LINE_H : 0);
  return { tileW, tileH, artW: tileW - FRAME, artH, nameLines, meta };
}

export const COVER_SIZES: Readonly<Record<CoverSize, CoverMetrics>> = {
  xs: metrics(96, 0, 'badge'),
  s: metrics(128, 1, 'badge'),
  m: metrics(168, 2, 'line'),
  l: metrics(220, 2, 'line'),
  xl: metrics(300, 2, 'line+size'),
};

/** The gap between tiles and around the grid edge. */
export const COVER_GAP = 12;

/** One stop smaller or larger, clamped at XS and XL. */
export function stepSize(size: CoverSize, delta: -1 | 1): CoverSize {
  const i = COVER_SIZE_IDS.indexOf(size);
  const next = Math.min(COVER_SIZE_IDS.length - 1, Math.max(0, i + delta));
  return COVER_SIZE_IDS[next] ?? size;
}

/** Whole tiles that fit in `width`, with a gap before each and at the end. */
export function columnsFor(
  width: number,
  size: CoverSize,
  gap: number = COVER_GAP,
): number {
  const { tileW } = COVER_SIZES[size];
  return Math.max(1, Math.floor((width - gap) / (tileW + gap)));
}
