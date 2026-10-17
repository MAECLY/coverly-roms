import * as SliderPrimitive from '@radix-ui/react-slider';
import {
  forwardRef,
  type ComponentPropsWithoutRef,
  type ElementRef,
} from 'react';

import { cn, focusRing } from '@/lib/utils';

export interface SliderProps extends ComponentPropsWithoutRef<
  typeof SliderPrimitive.Root
> {
  /** Accessible name per thumb (Radix puts the role on the thumb, not the
   * root). One string names every thumb. */
  thumbLabel?: string | string[];
}

/** Arrow keys step, PageUp/PageDown step ×10, Home/End jump. */
export const Slider = forwardRef<
  ElementRef<typeof SliderPrimitive.Root>,
  SliderProps
>(({ className, thumbLabel, value, defaultValue, ...props }, ref) => {
  const count = (value ?? defaultValue ?? [0]).length;
  const labelFor = (i: number) =>
    Array.isArray(thumbLabel) ? thumbLabel[i] : thumbLabel;
  return (
    <SliderPrimitive.Root
      ref={ref}
      {...(value ? { value } : {})}
      {...(defaultValue ? { defaultValue } : {})}
      className={cn(
        'relative flex w-full touch-none select-none items-center data-[orientation=vertical]:h-full data-[orientation=vertical]:w-auto data-[orientation=vertical]:flex-col',
        'data-[disabled]:opacity-60',
        className,
      )}
      {...props}
    >
      <SliderPrimitive.Track className="relative h-1 w-full grow overflow-hidden rounded-full bg-cr-active data-[orientation=vertical]:h-full data-[orientation=vertical]:w-1">
        <SliderPrimitive.Range className="absolute h-full bg-primary data-[orientation=vertical]:w-full" />
      </SliderPrimitive.Track>
      {Array.from({ length: count }, (_, i) => {
        const label = labelFor(i);
        return (
          <SliderPrimitive.Thumb
            key={i}
            {...(label ? { 'aria-label': label } : {})}
            className={cn(
              'block size-4 rounded-full border-[1.5px] border-primary bg-cr-surface-raised shadow-raised',
              'transition-transform duration-fast ease-standard hover:scale-110',
              'data-[disabled]:cursor-not-allowed',
              focusRing,
            )}
          />
        );
      })}
    </SliderPrimitive.Root>
  );
});
Slider.displayName = 'Slider';
