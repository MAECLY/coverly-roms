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
 *
 * When any game of the library has updates, DLC, revisions or is a demo
 * (addon views §8.2), M, L and XL add one 18px summary line under the
 * info line; XS and S, which have no info line, mark the art with a dot
 * instead, so their height never changes. A library without content keeps
 * exactly the tiles it had.
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
  /** How a game's add-ons show: none, an 18px line, or a dot on the art. */
  content: 'none' | 'line' | 'dot';
}

/** Border and padding, both sides. */
const FRAME = 16;
const NAME_GAP = 6;
const LINE_H = 16;
const META_H = 18;
const SIZE_LINE_H = 16;
/** The add-on summary line of M, L and XL (§8.2). */
export const CONTENT_LINE_H = 18;

function build(
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
  return {
    tileW,
    tileH,
    artW: tileW - FRAME,
    artH,
    nameLines,
    meta,
    content: 'none',
  };
}

/** The tiles of a library without add-on content. */
export const COVER_SIZES: Readonly<Record<CoverSize, CoverMetrics>> = {
  xs: build(96, 0, 'badge'),
  s: build(128, 1, 'badge'),
  m: build(168, 2, 'line'),
  l: build(220, 2, 'line'),
  xl: build(300, 2, 'line+size'),
};

function withContent(m: CoverMetrics): CoverMetrics {
  return m.meta === 'badge'
    ? { ...m, content: 'dot' }
    : { ...m, tileH: m.tileH + CONTENT_LINE_H, content: 'line' };
}

const WITH_CONTENT: Readonly<Record<CoverSize, CoverMetrics>> = {
  xs: withContent(COVER_SIZES.xs),
  s: withContent(COVER_SIZES.s),
  m: withContent(COVER_SIZES.m),
  l: withContent(COVER_SIZES.l),
  xl: withContent(COVER_SIZES.xl),
};

/**
 * The metrics of a size stop; `content` is a library flag (any game has
 * add-ons), so every row keeps one fixed height.
 */
export function metrics(size: CoverSize, content = false): CoverMetrics {
  return (content ? WITH_CONTENT : COVER_SIZES)[size];
}

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
