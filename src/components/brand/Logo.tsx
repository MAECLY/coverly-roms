import type { CSSProperties } from 'react';

/**
 * The Coverly ROMs mark ("Lean-To") with an optional wordmark.
 *
 * The glyph is inlined from `design/icon/mark.svg` (same geometry, cropped to
 * its bounding box) and drawn in `currentColor`. The wrapper sets that colour
 * to the brand primary (`--cr-accent`: Buckram `#0E5E6F` light, `#5DB6C4`
 * dark), so it matches `mark.svg` / `mark-dark.svg` without extra CSS.
 * The wordmark inherits the font of its parent (`.wordmark`, a heading).
 *
 * Do not redraw or recolour the glyph here: edit `design/icon/` and keep this
 * path data in sync with `mark.svg`.
 */
export const LOGO_NAME = 'Coverly ROMs';

/** Glyph bounding box in the 1024 master space (after the -22 y shift). */
const VIEW_BOX = '172 246 680 525';
const ASPECT = 680 / 525;

export interface LogoProps {
  /** Height of the mark: a number is px, a string any CSS length. Default `1.4em`. */
  size?: number | string;
  /** Show the "Coverly ROMs" text next to the mark. Default `true`. */
  showWordmark?: boolean;
  /** Wordmark text and accessible name. Default `Coverly ROMs`. */
  name?: string;
  className?: string;
  style?: CSSProperties;
}

export function Logo({
  size = '1.4em',
  showWordmark = true,
  name = LOGO_NAME,
  className,
  style,
}: LogoProps) {
  const height = typeof size === 'number' ? `${size}px` : size;
  const mark = (
    <svg
      data-testid="logo-mark"
      viewBox={VIEW_BOX}
      fill="currentColor"
      focusable="false"
      style={{
        width: `calc(${height} * ${ASPECT.toFixed(4)})`,
        height,
        color: 'var(--cr-accent)',
        flex: 'none',
        display: 'block',
      }}
      {...(showWordmark
        ? { 'aria-hidden': true }
        : { role: 'img', 'aria-label': name })}
    >
      <g transform="translate(0 -22)">
        <rect
          x="165.8"
          y="311.9"
          width="300"
          height="400"
          rx="48"
          transform="rotate(12 465.8 711.9)"
        />
        <rect
          x="568"
          y="327"
          width="246"
          height="348"
          rx="22"
          fill="none"
          stroke="currentColor"
          strokeWidth="56"
        />
        <rect x="172" y="737" width="680" height="56" rx="28" />
      </g>
    </svg>
  );
  return (
    <span
      className={className}
      style={{
        display: 'inline-flex',
        alignItems: 'center',
        gap: `calc(${height} * 0.5)`,
        ...style,
      }}
    >
      {mark}
      {showWordmark ? <span>{name}</span> : null}
    </span>
  );
}
