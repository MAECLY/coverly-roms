import * as SwitchPrimitive from '@radix-ui/react-switch';
import {
  forwardRef,
  type ComponentPropsWithoutRef,
  type ElementRef,
} from 'react';

import { cn, focusRing } from '@/lib/utils';

/** For settings that apply immediately (the desktop convention). */
export const Switch = forwardRef<
  ElementRef<typeof SwitchPrimitive.Root>,
  ComponentPropsWithoutRef<typeof SwitchPrimitive.Root>
>(({ className, ...props }, ref) => (
  <SwitchPrimitive.Root
    ref={ref}
    className={cn(
      'peer inline-flex h-[18px] w-8 shrink-0 cursor-pointer items-center rounded-full border-[1.5px] p-px',
      'transition-colors duration-fast ease-standard',
      'data-[state=unchecked]:border-input data-[state=unchecked]:bg-cr-surface-sunken',
      'data-[state=checked]:border-primary data-[state=checked]:bg-primary',
      'disabled:cursor-not-allowed disabled:opacity-60',
      focusRing,
      className,
    )}
    {...props}
  >
    <SwitchPrimitive.Thumb
      className={cn(
        'pointer-events-none block size-3 rounded-full shadow-[0_1px_1px_rgb(0_0_0/0.2)]',
        'transition-transform duration-fast ease-standard',
        'data-[state=unchecked]:translate-x-0 data-[state=unchecked]:bg-cr-text-secondary',
        'data-[state=checked]:translate-x-[15px] data-[state=checked]:bg-primary-foreground',
      )}
    />
  </SwitchPrimitive.Root>
));
Switch.displayName = 'Switch';
