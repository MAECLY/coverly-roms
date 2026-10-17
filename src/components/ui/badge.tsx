import { cva, type VariantProps } from 'class-variance-authority';
import { forwardRef, type HTMLAttributes } from 'react';

import { cn } from '@/lib/utils';

/**
 * Small static labels. `tag` is the Manila label tape (brand §4.1): filled,
 * dark ink, used for source labels and counts, kept rare. Status variants
 * pair with an icon or a word, never colour alone.
 */
export const badgeVariants = cva(
  [
    'inline-flex h-5 shrink-0 items-center gap-1 whitespace-nowrap rounded-sm px-1.5 text-caption font-medium tabular-nums',
    "[&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-3",
  ],
  {
    variants: {
      variant: {
        neutral: 'bg-cr-surface-sunken text-cr-text-secondary',
        outline: 'border border-cr-border-strong text-cr-text-secondary',
        primary: 'bg-cr-primary-subtle text-cr-primary-subtle-fg',
        tag: 'bg-cr-manila text-cr-manila-fg',
        success: 'bg-cr-success-bg text-cr-success',
        warning: 'bg-cr-warning-bg text-cr-warning',
        danger: 'bg-cr-danger-bg text-cr-danger',
        info: 'bg-cr-info-bg text-cr-info',
      },
      mono: {
        true: 'font-mono font-normal tracking-normal',
        false: '',
      },
    },
    defaultVariants: { variant: 'neutral', mono: false },
  },
);

export interface BadgeProps
  extends HTMLAttributes<HTMLSpanElement>, VariantProps<typeof badgeVariants> {}

export const Badge = forwardRef<HTMLSpanElement, BadgeProps>(
  ({ className, variant, mono, ...props }, ref) => (
    <span
      ref={ref}
      className={cn(badgeVariants({ variant, mono }), className)}
      {...props}
    />
  ),
);
Badge.displayName = 'Badge';
