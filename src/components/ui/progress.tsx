import * as ProgressPrimitive from '@radix-ui/react-progress';
import {
  forwardRef,
  type ComponentPropsWithoutRef,
  type ElementRef,
} from 'react';

import { cn } from '@/lib/utils';

export interface ProgressProps extends ComponentPropsWithoutRef<
  typeof ProgressPrimitive.Root
> {
  /**
   * Draw a row of N slots filling left to right (brand §2) instead of a bar.
   * Each slot is dashed until its cover "slides in" from the bottom.
   */
  slots?: number;
  /** Bar thickness. */
  size?: 'sm' | 'md';
  /** Overrides the fill colour role, e.g. a paused job. */
  tone?: 'primary' | 'warning' | 'danger' | 'success';
}

const toneBg = {
  primary: 'bg-primary',
  warning: 'bg-cr-warning-fill',
  danger: 'bg-cr-danger-fill',
  success: 'bg-cr-success',
} as const;

const toneText = {
  primary: 'text-primary',
  warning: 'text-cr-warning-fill',
  danger: 'text-cr-danger-fill',
  success: 'text-cr-success',
} as const;

/**
 * `role="progressbar"` with `aria-valuenow` / `aria-valuemax`; pass
 * `aria-label` (or `aria-labelledby`). `value={null}` is indeterminate.
 */
export const Progress = forwardRef<
  ElementRef<typeof ProgressPrimitive.Root>,
  ProgressProps
>(
  (
    {
      className,
      value = null,
      max = 100,
      slots,
      size = 'md',
      tone = 'primary',
      ...props
    },
    ref,
  ) => {
    const ratio = value === null ? null : Math.min(Math.max(value / max, 0), 1);
    return (
      <ProgressPrimitive.Root
        ref={ref}
        value={value}
        max={max}
        className={cn(
          slots
            ? 'inline-flex items-end gap-[3px]'
            : cn(
                'relative w-full overflow-hidden rounded-full bg-cr-active',
                size === 'sm' ? 'h-1' : 'h-1.5',
              ),
          className,
        )}
        {...props}
      >
        {slots ? (
          Array.from({ length: slots }, (_, i) => {
            const fill =
              ratio === null ? 0 : Math.min(Math.max(ratio * slots - i, 0), 1);
            return (
              <svg
                key={i}
                viewBox="0 0 12 16"
                aria-hidden
                data-fill={fill === 1 ? 'full' : fill > 0 ? 'partial' : 'empty'}
                className="h-4 w-3 shrink-0 overflow-visible"
              >
                <rect
                  x="0.75"
                  y="0.75"
                  width="10.5"
                  height="14.5"
                  rx="1.5"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="1.5"
                  strokeDasharray={fill === 1 ? undefined : '3 2'}
                  className={cn(
                    fill === 1 ? toneText[tone] : 'text-cr-slot-missing',
                    ratio === null &&
                      'animate-slot-march motion-reduce:animate-none',
                  )}
                />
                <rect
                  x="0"
                  y="0"
                  width="12"
                  height="16"
                  rx="2"
                  className={cn(
                    toneText[tone],
                    'fill-current transition-transform duration-slow ease-ink [transform-box:fill-box] [transform-origin:bottom]',
                  )}
                  style={{ transform: `scaleY(${fill})` }}
                />
              </svg>
            );
          })
        ) : (
          <ProgressPrimitive.Indicator
            className={cn(
              'h-full rounded-full',
              toneBg[tone],
              ratio === null
                ? 'w-2/5 animate-shelf-sweep motion-reduce:w-full motion-reduce:animate-none motion-reduce:opacity-40'
                : 'w-full transition-transform duration-slow ease-ink',
            )}
            style={
              ratio === null
                ? undefined
                : { transform: `translateX(-${100 - ratio * 100}%)` }
            }
          />
        )}
      </ProgressPrimitive.Root>
    );
  },
);
Progress.displayName = 'Progress';
