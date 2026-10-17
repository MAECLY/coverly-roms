import * as ScrollAreaPrimitive from '@radix-ui/react-scroll-area';
import {
  forwardRef,
  type ComponentPropsWithoutRef,
  type ElementRef,
} from 'react';

import { cn, focusRing } from '@/lib/utils';

/**
 * Overlay scrollbars that match the theme. Not for the virtualised grid and
 * table (they need the real scroller). The viewport is focusable so keyboard
 * users can scroll it.
 */
export const ScrollArea = forwardRef<
  ElementRef<typeof ScrollAreaPrimitive.Root>,
  ComponentPropsWithoutRef<typeof ScrollAreaPrimitive.Root> & {
    viewportClassName?: string;
    /** Accessible name for the focusable scroll region. */
    'aria-label'?: string;
  }
>(
  (
    {
      className,
      viewportClassName,
      children,
      'aria-label': ariaLabel,
      ...props
    },
    ref,
  ) => (
    <ScrollAreaPrimitive.Root
      ref={ref}
      className={cn('relative overflow-hidden', className)}
      {...props}
    >
      <ScrollAreaPrimitive.Viewport
        {...(ariaLabel
          ? { 'aria-label': ariaLabel, role: 'region', tabIndex: 0 }
          : {})}
        className={cn(
          'size-full rounded-[inherit]',
          focusRing,
          'focus-visible:outline-offset-[-2px]',
          viewportClassName,
        )}
      >
        {children}
      </ScrollAreaPrimitive.Viewport>
      <ScrollBar orientation="vertical" />
      <ScrollBar orientation="horizontal" />
      <ScrollAreaPrimitive.Corner />
    </ScrollAreaPrimitive.Root>
  ),
);
ScrollArea.displayName = 'ScrollArea';

export const ScrollBar = forwardRef<
  ElementRef<typeof ScrollAreaPrimitive.Scrollbar>,
  ComponentPropsWithoutRef<typeof ScrollAreaPrimitive.Scrollbar>
>(({ className, orientation = 'vertical', ...props }, ref) => (
  <ScrollAreaPrimitive.Scrollbar
    ref={ref}
    orientation={orientation}
    className={cn(
      'flex touch-none select-none p-0.5 transition-colors duration-fast',
      orientation === 'vertical' ? 'h-full w-2.5' : 'h-2.5 flex-col',
      className,
    )}
    {...props}
  >
    <ScrollAreaPrimitive.Thumb className="relative flex-1 rounded-full bg-cr-border-strong/50 hover:bg-cr-border-strong" />
  </ScrollAreaPrimitive.Scrollbar>
));
ScrollBar.displayName = 'ScrollBar';
