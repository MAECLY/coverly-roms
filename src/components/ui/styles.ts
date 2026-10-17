import { cn } from '@/lib/utils';

/* Class recipes shared by the overlay primitives (Select, DropdownMenu,
   Popover). */

/** Overlay elevation (brand §6): raised surface, overlay shadow, 8px radius;
 * fades and settles 4px from its side. */
export const overlaySurface = cn(
  'z-50 rounded-lg border border-cr-border bg-popover text-popover-foreground shadow-overlay',
  'data-[state=open]:animate-in data-[state=open]:fade-in-0 data-[state=open]:duration-base data-[state=open]:ease-enter',
  'data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=closed]:duration-fast data-[state=closed]:ease-exit',
  'data-[side=bottom]:slide-in-from-top-1 data-[side=top]:slide-in-from-bottom-1 data-[side=left]:slide-in-from-right-1 data-[side=right]:slide-in-from-left-1',
  'motion-reduce:animate-none',
);

/** A menu / listbox row: keyboard highlight is the neutral hover fill. */
export const menuItem = cn(
  'relative flex h-control-sm w-full cursor-default select-none items-center gap-2 rounded-sm px-2 text-ui text-cr-text outline-none',
  'data-[highlighted]:bg-cr-hover data-[state=checked]:font-medium',
  'data-[disabled]:pointer-events-none data-[disabled]:text-cr-text-disabled',
  "[&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4",
);
