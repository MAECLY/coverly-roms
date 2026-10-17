import { forwardRef, type SVGAttributes } from 'react';

import { cn } from '@/lib/utils';

export type SlotState =
  'missing' | 'filled' | 'applied' | 'applied-run' | 'unsupported';

export interface SlotMarkProps extends Omit<
  SVGAttributes<SVGSVGElement>,
  'children'
> {
  state: SlotState;
  /** Height in px; width is 3:4 of it. Photo-corners appear from 24px. */
  size?: number;
  /** Accessible name. Without it the mark is decorative (`aria-hidden`). */
  label?: string;
}

/**
 * The Slot (brand §2), the product's signature: dashed = not yet, solid =
 * done, Buckram spine = Coverly did it, Manila tab = you can still undo it.
 * Drawn in SVG in pixel units, so the 1.5px dash (4 / 3) stays crisp.
 */
export const SlotMark = forwardRef<SVGSVGElement, SlotMarkProps>(
  ({ state, size = 16, label, className, ...props }, ref) => {
    const h = size;
    const w = Math.round(size * 0.75 * 2) / 2;
    const r = Math.max(1, Math.min(3, size / 10));
    const corners = size >= 24;
    const corner = Math.round(size * 0.2);
    const a11y = label
      ? { role: 'img' as const, 'aria-label': label }
      : { 'aria-hidden': true as const };
    return (
      <svg
        ref={ref}
        width={w}
        height={h}
        viewBox={`0 0 ${w} ${h}`}
        data-state={state}
        className={cn('inline-block shrink-0 align-middle', className)}
        {...a11y}
        {...props}
      >
        {state === 'missing' && (
          <>
            <rect
              x="0.75"
              y="0.75"
              width={w - 1.5}
              height={h - 1.5}
              rx={r}
              fill="none"
              className="stroke-cr-slot-missing"
              strokeWidth="1.5"
              strokeDasharray={corners ? '4 3' : size >= 16 ? '3 2' : undefined}
            />
            {corners && (
              <g className="fill-cr-slot-missing">
                <path d={`M0 0H${corner}L0 ${corner}Z`} />
                <path d={`M${w} 0V${corner}L${w - corner} 0Z`} />
                <path d={`M0 ${h}V${h - corner}L${corner} ${h}Z`} />
                <path d={`M${w} ${h}H${w - corner}L${w} ${h - corner}Z`} />
              </g>
            )}
          </>
        )}
        {state === 'unsupported' && (
          <>
            <rect
              x="0.5"
              y="0.5"
              width={w - 1}
              height={h - 1}
              rx={r}
              fill="none"
              className="stroke-cr-text-disabled"
            />
            <path
              d={`M${w * 0.2} ${h * 0.8}L${w * 0.8} ${h * 0.2}`}
              className="stroke-cr-text-disabled"
              strokeWidth="1.25"
              strokeLinecap="round"
            />
          </>
        )}
        {(state === 'filled' ||
          state === 'applied' ||
          state === 'applied-run') && (
          <>
            <rect width={w} height={h} rx={r} className="fill-cr-slot-filled" />
            {state === 'applied' && (
              <rect
                y={h - Math.max(2, Math.round(size / 8))}
                width={w}
                height={Math.max(2, Math.round(size / 8))}
                className="fill-cr-slot-applied"
              />
            )}
            {state === 'applied-run' && (
              <path
                d={`M${w - Math.max(5, w * 0.45)} 0H${w}V${Math.max(5, w * 0.45)}Z`}
                className="fill-cr-slot-applied-run"
              />
            )}
          </>
        )}
      </svg>
    );
  },
);
SlotMark.displayName = 'SlotMark';
