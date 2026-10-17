import * as TogglePrimitive from '@radix-ui/react-toggle';
import { cva, type VariantProps } from 'class-variance-authority';
import {
  forwardRef,
  type ComponentPropsWithoutRef,
  type ElementRef,
} from 'react';

import { cn, focusRing } from '@/lib/utils';

/** Pressed = Buckram subtle fill ("selected", brand §4.1). */
export const toggleVariants = cva(
  [
    'inline-flex select-none items-center justify-center gap-1.5 whitespace-nowrap rounded-md text-ui font-medium text-cr-text-secondary',
    'transition-colors duration-fast ease-standard',
    'hover:bg-cr-hover hover:text-cr-text',
    'data-[state=on]:bg-cr-primary-subtle data-[state=on]:text-cr-primary-subtle-fg',
    'disabled:cursor-not-allowed disabled:text-cr-text-disabled disabled:hover:bg-transparent',
    "[&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4",
    focusRing,
  ],
  {
    variants: {
      variant: {
        ghost: 'bg-transparent',
        outline: 'border border-cr-border bg-cr-surface',
      },
      size: {
        sm: 'h-control-sm min-w-7 px-2',
        md: 'h-control min-w-8 px-2.5',
        lg: 'h-control-lg min-w-10 px-3',
      },
    },
    defaultVariants: { variant: 'ghost', size: 'md' },
  },
);

export type ToggleVariantProps = VariantProps<typeof toggleVariants>;

export const Toggle = forwardRef<
  ElementRef<typeof TogglePrimitive.Root>,
  ComponentPropsWithoutRef<typeof TogglePrimitive.Root> & ToggleVariantProps
>(({ className, variant, size, ...props }, ref) => (
  <TogglePrimitive.Root
    ref={ref}
    className={cn(toggleVariants({ variant, size }), className)}
    {...props}
  />
));
Toggle.displayName = 'Toggle';
