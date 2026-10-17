import { cva, type VariantProps } from 'class-variance-authority';
import { forwardRef, type HTMLAttributes } from 'react';

import { useT } from '@/i18n/useT';
import { cn } from '@/lib/utils';

const spinnerVariants = cva('inline-block shrink-0 text-current', {
  variants: {
    size: {
      sm: 'h-4 w-3',
      md: 'h-5 w-[15px]',
      lg: 'h-8 w-6',
    },
  },
  defaultVariants: { size: 'md' },
});

export interface SpinnerProps
  extends
    Omit<HTMLAttributes<HTMLSpanElement>, 'children'>,
    VariantProps<typeof spinnerVariants> {
  /** Accessible text; defaults to the translated "Loading". */
  label?: string;
  /** Hide from assistive tech (the parent already says it is busy). */
  decorative?: boolean;
}

/**
 * Loading indicator drawn as the brand slot (3:4) whose dashed outline
 * marches around. Under `prefers-reduced-motion` it holds still.
 */
export const Spinner = forwardRef<HTMLSpanElement, SpinnerProps>(
  ({ className, size, label, decorative = false, ...props }, ref) => {
    const t = useT();
    const a11y = decorative
      ? { 'aria-hidden': true as const }
      : { role: 'status' as const };
    return (
      <span
        ref={ref}
        className={cn(spinnerVariants({ size }), className)}
        {...a11y}
        {...props}
      >
        <svg
          viewBox="0 0 12 16"
          className="size-full overflow-visible"
          aria-hidden
        >
          <rect
            x="1"
            y="1"
            width="10"
            height="14"
            rx="1.5"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.75"
            strokeDasharray="4 3"
            strokeLinecap="round"
            className="animate-slot-march motion-reduce:animate-none"
          />
        </svg>
        {!decorative && (
          <span className="sr-only">{label ?? t('ui.loading')}</span>
        )}
      </span>
    );
  },
);
Spinner.displayName = 'Spinner';
