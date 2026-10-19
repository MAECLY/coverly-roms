import { Folder } from 'lucide-react';
import { memo, type CSSProperties, type JSX } from 'react';

import { useT } from '@/i18n/useT';
import { cn } from '@/lib/utils';

import { abbreviationFor } from './systemFallbacks';
import { SYSTEM_ICONS } from './systemIcons.generated';

export interface SystemIconProps {
  /** Canonical id from systems.toml (`SystemView.system_id`). Null means a
   *  folder Coverly did not recognise. */
  systemId: string | null;
  /** Box height: px number or CSS length. Default '1em'. Width is always 1.5×. */
  size?: number | string;
  /** Lone icon: role="img" + aria-label. Absent: aria-hidden. */
  title?: string;
  className?: string;
}

const CORNERS = ['tl', 'tr', 'bl', 'br'] as const;

/** 1.5 × a px number or a CSS length (`'1em'` → `'1.5em'`). */
function widen(size: number | string): number | string {
  if (typeof size === 'number') return size * 1.5;
  const m = /^(\d*\.?\d+)([a-z%]*)$/i.exec(size.trim());
  return m ? `${Number(m[1]) * 1.5}${m[2] ?? ''}` : `calc(${size} * 1.5)`;
}

function cssLength(v: number | string): string {
  return typeof v === 'number' ? `${v}px` : v;
}

function SystemIconImpl({
  systemId,
  size: rawSize = '1em',
  title,
  className,
}: SystemIconProps): JSX.Element {
  const t = useT();
  // A bare numeric string ('16') is pixels, like a number; CSS would drop it.
  const size =
    typeof rawSize === 'string' && /^\s*\d*\.?\d+\s*$/.test(rawSize)
      ? Number(rawSize)
      : rawSize;
  const width = widen(size);
  // String sizes follow the text, which is always below the 20px floor.
  const px = typeof size === 'number' ? size : 0;
  const label =
    title === undefined
      ? undefined
      : title || (systemId === null ? t('systems.unknown') : systemId);
  const a11y = label
    ? { role: 'img' as const, 'aria-label': label }
    : { 'aria-hidden': true as const };

  if (systemId === null) {
    return (
      <span
        data-system-icon="unknown"
        className={cn('system-icon system-icon--folder', className)}
        style={{ width: cssLength(width), height: cssLength(size) }}
        {...a11y}
      >
        <Folder
          aria-hidden="true"
          width={cssLength(size)}
          height="100%"
          style={{ width: cssLength(size), height: '100%' }}
        />
      </span>
    );
  }

  const icon = Object.hasOwn(SYSTEM_ICONS, systemId)
    ? SYSTEM_ICONS[systemId]
    : undefined;
  if (icon) {
    return (
      <svg
        data-system-icon={systemId}
        className={cn('system-icon', className)}
        viewBox={icon.viewBox}
        preserveAspectRatio="xMidYMid meet"
        fill="currentColor"
        // Numbers stay attributes; CSS lengths (em, calc()) only go through
        // style, since an SVG width attribute does not accept calc(). The box
        // is always set in style too: a presentational width loses to any CSS
        // rule, and Button squares every svg it holds to 1rem.
        width={typeof width === 'number' ? width : undefined}
        height={typeof size === 'number' ? size : undefined}
        style={{ width: cssLength(width), height: cssLength(size) }}
        {...a11y}
        // The one allowed dangerouslySetInnerHTML (no lint rule exists for it
        // here): a trusted repo constant from systemIcons.generated.ts, cleaned
        // by the generator and checked by systemIcons.test.ts. Never entry data.
        dangerouslySetInnerHTML={{ __html: icon.body }}
      />
    );
  }

  const style: CSSProperties = {
    width: cssLength(width),
    height: cssLength(size),
    fontSize: cssLength(size),
  };
  return (
    <span
      data-system-icon={systemId}
      className={cn(
        'system-mount',
        px >= 20 && 'system-mount--outline',
        className,
      )}
      style={style}
      {...a11y}
    >
      <span className="system-mount__text">{abbreviationFor(systemId)}</span>
      {px >= 24 &&
        CORNERS.map((c) => (
          <i
            key={c}
            aria-hidden="true"
            className={`system-mount__corner system-mount__corner--${c}`}
          />
        ))}
    </span>
  );
}

/** A system's hardware silhouette, Slot-shaped abbreviation mount, or folder
 *  (spec §A.3–A.4), always in a box 1.5 × its height wide. */
export const SystemIcon = memo(SystemIconImpl);
SystemIcon.displayName = 'SystemIcon';
