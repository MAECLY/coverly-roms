import * as TabsPrimitive from '@radix-ui/react-tabs';
import {
  forwardRef,
  type ComponentPropsWithoutRef,
  type ElementRef,
} from 'react';

import { cn, focusRing } from '@/lib/utils';

/**
 * Tabs with the shelf-rule motif (brand §2): horizontal tabs sit on a 1px
 * rule and the active one carries a Buckram spine underneath; vertical tabs
 * get the 3px Buckram cap on the left. Arrow keys move, Home/End jump.
 */
export const Tabs = forwardRef<
  ElementRef<typeof TabsPrimitive.Root>,
  ComponentPropsWithoutRef<typeof TabsPrimitive.Root>
>(({ className, ...props }, ref) => (
  <TabsPrimitive.Root
    ref={ref}
    className={cn(
      'flex flex-col gap-3 data-[orientation=vertical]:flex-row',
      className,
    )}
    {...props}
  />
));
Tabs.displayName = 'Tabs';

export const TabsList = forwardRef<
  ElementRef<typeof TabsPrimitive.List>,
  ComponentPropsWithoutRef<typeof TabsPrimitive.List>
>(({ className, ...props }, ref) => (
  <TabsPrimitive.List
    ref={ref}
    className={cn(
      'flex shrink-0 text-cr-text-secondary',
      'data-[orientation=horizontal]:items-end data-[orientation=horizontal]:gap-1 data-[orientation=horizontal]:border-b data-[orientation=horizontal]:border-cr-border',
      'data-[orientation=vertical]:flex-col data-[orientation=vertical]:gap-0.5',
      className,
    )}
    {...props}
  />
));
TabsList.displayName = 'TabsList';

export const TabsTrigger = forwardRef<
  ElementRef<typeof TabsPrimitive.Trigger>,
  ComponentPropsWithoutRef<typeof TabsPrimitive.Trigger>
>(({ className, ...props }, ref) => (
  <TabsPrimitive.Trigger
    ref={ref}
    className={cn(
      'relative inline-flex h-control select-none items-center gap-1.5 whitespace-nowrap rounded-sm px-3 text-ui font-medium',
      'transition-colors duration-fast ease-standard',
      'hover:text-cr-text data-[state=active]:text-cr-text',
      'disabled:cursor-not-allowed disabled:text-cr-text-disabled',
      "[&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4",
      // Horizontal: Buckram spine under the active tab, over the shelf rule.
      'data-[orientation=horizontal]:-mb-px data-[orientation=horizontal]:rounded-b-none',
      'after:absolute after:bg-primary after:opacity-0 after:transition-opacity after:duration-fast data-[state=active]:after:opacity-100',
      'data-[orientation=horizontal]:after:inset-x-2 data-[orientation=horizontal]:after:bottom-0 data-[orientation=horizontal]:after:h-[3px] data-[orientation=horizontal]:after:rounded-t-sm',
      // Vertical: the shelf-rule cap on the left, hover fill on the row.
      'data-[orientation=vertical]:w-full data-[orientation=vertical]:justify-start data-[orientation=vertical]:pl-3.5',
      'data-[orientation=vertical]:hover:bg-cr-hover data-[orientation=vertical]:data-[state=active]:bg-cr-surface-sunken',
      'data-[orientation=vertical]:after:inset-y-1.5 data-[orientation=vertical]:after:left-0 data-[orientation=vertical]:after:w-[3px] data-[orientation=vertical]:after:rounded-r-sm',
      focusRing,
      'focus-visible:outline-offset-0',
      className,
    )}
    {...props}
  />
));
TabsTrigger.displayName = 'TabsTrigger';

export const TabsContent = forwardRef<
  ElementRef<typeof TabsPrimitive.Content>,
  ComponentPropsWithoutRef<typeof TabsPrimitive.Content>
>(({ className, ...props }, ref) => (
  <TabsPrimitive.Content
    ref={ref}
    className={cn('min-w-0 flex-1 rounded-sm', focusRing, className)}
    {...props}
  />
));
TabsContent.displayName = 'TabsContent';
