import * as SeparatorPrimitive from '@radix-ui/react-separator';
import {
  forwardRef,
  type ComponentPropsWithoutRef,
  type ElementRef,
} from 'react';

import { cn } from '@/lib/utils';

export interface SeparatorProps extends ComponentPropsWithoutRef<
  typeof SeparatorPrimitive.Root
> {
  /** The shelf rule (brand §2): a 3px Buckram cap on the left. Horizontal only. */
  shelf?: boolean;
}

/** Decorative by default (hidden from AT); `decorative={false}` makes it a
 * `role="separator"`. */
export const Separator = forwardRef<
  ElementRef<typeof SeparatorPrimitive.Root>,
  SeparatorProps
>(
  (
    {
      className,
      orientation = 'horizontal',
      decorative = true,
      shelf = false,
      ...props
    },
    ref,
  ) => (
    <SeparatorPrimitive.Root
      ref={ref}
      orientation={orientation}
      decorative={decorative}
      className={cn(
        'shrink-0 bg-cr-border',
        orientation === 'horizontal' ? 'h-px w-full' : 'h-full w-px',
        shelf &&
          orientation === 'horizontal' &&
          'relative before:absolute before:-top-px before:left-0 before:h-[3px] before:w-6 before:rounded-r-sm before:bg-primary',
        className,
      )}
      {...props}
    />
  ),
);
Separator.displayName = 'Separator';
