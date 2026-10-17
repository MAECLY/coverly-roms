import * as ToggleGroupPrimitive from '@radix-ui/react-toggle-group';
import {
  createContext,
  forwardRef,
  useContext,
  useState,
  type ComponentPropsWithoutRef,
  type ElementRef,
} from 'react';

import { cn, focusRing } from '@/lib/utils';

import { toggleVariants, type ToggleVariantProps } from './toggle';

const ToggleGroupContext = createContext<ToggleVariantProps>({});

/** A row of toggles with roving focus (arrow keys). `type="single"` or
 * `"multiple"`. */
export const ToggleGroup = forwardRef<
  ElementRef<typeof ToggleGroupPrimitive.Root>,
  ComponentPropsWithoutRef<typeof ToggleGroupPrimitive.Root> &
    ToggleVariantProps
>(({ className, variant, size, children, ...props }, ref) => (
  <ToggleGroupPrimitive.Root
    ref={ref}
    className={cn('inline-flex items-center gap-1', className)}
    {...props}
  >
    <ToggleGroupContext.Provider value={{ variant, size }}>
      {children}
    </ToggleGroupContext.Provider>
  </ToggleGroupPrimitive.Root>
));
ToggleGroup.displayName = 'ToggleGroup';

export const ToggleGroupItem = forwardRef<
  ElementRef<typeof ToggleGroupPrimitive.Item>,
  ComponentPropsWithoutRef<typeof ToggleGroupPrimitive.Item> &
    ToggleVariantProps
>(({ className, variant, size, ...props }, ref) => {
  const ctx = useContext(ToggleGroupContext);
  return (
    <ToggleGroupPrimitive.Item
      ref={ref}
      className={cn(
        toggleVariants({
          variant: variant ?? ctx.variant,
          size: size ?? ctx.size,
        }),
        className,
      )}
      {...props}
    />
  );
});
ToggleGroupItem.displayName = 'ToggleGroupItem';

/* ---------- SegmentedControl ---------------------------------------------- */

export interface SegmentedControlProps extends Omit<
  ComponentPropsWithoutRef<typeof ToggleGroupPrimitive.Root>,
  'type' | 'value' | 'defaultValue' | 'onValueChange'
> {
  value?: string;
  defaultValue?: string;
  /** Never called with '': a segmented control always has one choice. */
  onValueChange?: (value: string) => void;
  size?: 'sm' | 'md';
}

const SegmentSizeContext = createContext<'sm' | 'md'>('md');

/**
 * Single choice from 2–4 short options (theme, grid / list): a sunken track
 * with the chosen segment raised onto it. Built on ToggleGroup type="single",
 * so items are `radio`s with roving focus and the choice can't be cleared.
 */
export const SegmentedControl = forwardRef<
  ElementRef<typeof ToggleGroupPrimitive.Root>,
  SegmentedControlProps
>(
  (
    {
      className,
      value,
      defaultValue,
      onValueChange,
      size = 'md',
      children,
      ...props
    },
    ref,
  ) => {
    // Held here (not only in Radix) so clicking the chosen item can't clear it.
    const [inner, setInner] = useState(defaultValue ?? '');
    const current = value ?? inner;
    return (
      <ToggleGroupPrimitive.Root
        ref={ref}
        type="single"
        value={current}
        onValueChange={(next) => {
          if (!next) return;
          setInner(next);
          onValueChange?.(next);
        }}
        className={cn(
          'inline-flex items-stretch gap-0.5 rounded-md border border-cr-border bg-cr-surface-sunken p-0.5',
          size === 'sm' ? 'h-control-sm' : 'h-control',
          className,
        )}
        {...props}
      >
        <SegmentSizeContext.Provider value={size}>
          {children}
        </SegmentSizeContext.Provider>
      </ToggleGroupPrimitive.Root>
    );
  },
);
SegmentedControl.displayName = 'SegmentedControl';

export const SegmentedControlItem = forwardRef<
  ElementRef<typeof ToggleGroupPrimitive.Item>,
  ComponentPropsWithoutRef<typeof ToggleGroupPrimitive.Item>
>(({ className, ...props }, ref) => {
  const size = useContext(SegmentSizeContext);
  return (
    <ToggleGroupPrimitive.Item
      ref={ref}
      className={cn(
        'inline-flex select-none items-center justify-center gap-1.5 whitespace-nowrap rounded-[3px] text-ui font-medium text-cr-text-secondary',
        size === 'sm' ? 'px-2' : 'px-2.5',
        'transition-[background-color,color,box-shadow] duration-fast ease-standard',
        'hover:text-cr-text',
        'data-[state=on]:bg-cr-surface-raised data-[state=on]:text-cr-text data-[state=on]:shadow-[0_0_0_1px_rgb(var(--cr-border-rgb)),0_1px_2px_rgb(0_0_0/0.08)]',
        'disabled:cursor-not-allowed disabled:text-cr-text-disabled',
        "[&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4",
        focusRing,
        'focus-visible:outline-offset-0',
        className,
      )}
      {...props}
    />
  );
});
SegmentedControlItem.displayName = 'SegmentedControlItem';
