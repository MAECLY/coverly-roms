import * as RadioGroupPrimitive from '@radix-ui/react-radio-group';
import {
  forwardRef,
  type ComponentPropsWithoutRef,
  type ElementRef,
} from 'react';

import { cn, focusRing } from '@/lib/utils';

/** Arrow keys move and select (roving focus); Tab leaves the group. */
export const RadioGroup = forwardRef<
  ElementRef<typeof RadioGroupPrimitive.Root>,
  ComponentPropsWithoutRef<typeof RadioGroupPrimitive.Root>
>(({ className, ...props }, ref) => (
  <RadioGroupPrimitive.Root
    ref={ref}
    className={cn(
      'grid gap-2 data-[orientation=horizontal]:flex data-[orientation=horizontal]:flex-wrap data-[orientation=horizontal]:gap-4',
      className,
    )}
    {...props}
  />
));
RadioGroup.displayName = 'RadioGroup';

export const RadioGroupItem = forwardRef<
  ElementRef<typeof RadioGroupPrimitive.Item>,
  ComponentPropsWithoutRef<typeof RadioGroupPrimitive.Item>
>(({ className, ...props }, ref) => (
  <RadioGroupPrimitive.Item
    ref={ref}
    className={cn(
      'peer inline-grid size-4 shrink-0 place-items-center rounded-full border-[1.5px] border-input bg-cr-surface-raised',
      'transition-colors duration-fast ease-standard hover:border-cr-text-secondary',
      'data-[state=checked]:border-primary',
      'disabled:cursor-not-allowed disabled:opacity-60',
      'aria-[invalid=true]:border-cr-danger',
      focusRing,
      className,
    )}
    {...props}
  >
    <RadioGroupPrimitive.Indicator className="size-2 rounded-full bg-primary forced-colors:bg-[CanvasText] forced-colors:forced-color-adjust-none" />
  </RadioGroupPrimitive.Item>
));
RadioGroupItem.displayName = 'RadioGroupItem';
