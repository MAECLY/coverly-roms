import * as CollapsiblePrimitive from '@radix-ui/react-collapsible';
import {
  forwardRef,
  type ComponentPropsWithoutRef,
  type ElementRef,
} from 'react';

import { cn } from '@/lib/utils';

/** Disclosure: the trigger gets `aria-expanded` + `aria-controls`. Put a
 * `ChevronRight` with `group-data-[state=open]:rotate-90` in the trigger. */
export const Collapsible = CollapsiblePrimitive.Root;
export const CollapsibleTrigger = CollapsiblePrimitive.Trigger;

export const CollapsibleContent = forwardRef<
  ElementRef<typeof CollapsiblePrimitive.Content>,
  ComponentPropsWithoutRef<typeof CollapsiblePrimitive.Content>
>(({ className, ...props }, ref) => (
  <CollapsiblePrimitive.Content
    ref={ref}
    className={cn(
      'overflow-hidden data-[state=closed]:animate-collapsible-up data-[state=open]:animate-collapsible-down motion-reduce:animate-none',
      className,
    )}
    {...props}
  />
));
CollapsibleContent.displayName = 'CollapsibleContent';
