import * as SelectPrimitive from '@radix-ui/react-select';
import { Check, ChevronDown, ChevronUp } from 'lucide-react';
import {
  forwardRef,
  type ComponentPropsWithoutRef,
  type ElementRef,
  type SelectHTMLAttributes,
} from 'react';

import { cn } from '@/lib/utils';

import { fieldSurface } from './input';
import { menuItem, overlaySurface } from './styles';

const sizeClass = {
  sm: 'h-control-sm pl-2 pr-7 text-ui',
  md: 'h-control pl-2.5 pr-8 text-ui',
  lg: 'h-control-lg pl-3 pr-9 text-dense',
} as const;

export type SelectSize = keyof typeof sizeClass;

/* ---------- NativeSelect: the default (keeps userEvent.selectOptions) ---- */

export interface NativeSelectProps extends Omit<
  SelectHTMLAttributes<HTMLSelectElement>,
  'size'
> {
  size?: SelectSize;
}

/** A styled native `<select>` with the brand chevron and focus ring. */
export const NativeSelect = forwardRef<HTMLSelectElement, NativeSelectProps>(
  ({ className, size = 'md', children, ...props }, ref) => (
    <span
      data-slot="native-select"
      className={cn('relative inline-flex w-full min-w-0', className)}
    >
      <select
        ref={ref}
        className={cn(
          fieldSurface,
          sizeClass[size],
          'cursor-pointer appearance-none truncate',
        )}
        {...props}
      >
        {children}
      </select>
      <ChevronDown
        aria-hidden
        className="pointer-events-none absolute right-2 top-1/2 size-4 -translate-y-1/2 text-cr-text-secondary"
      />
    </span>
  ),
);
NativeSelect.displayName = 'NativeSelect';

/* ---------- Radix Select: when a native popup can't show what we need ---- */

export const Select = SelectPrimitive.Root;
export const SelectGroup = SelectPrimitive.Group;
export const SelectValue = SelectPrimitive.Value;

export const SelectTrigger = forwardRef<
  ElementRef<typeof SelectPrimitive.Trigger>,
  ComponentPropsWithoutRef<typeof SelectPrimitive.Trigger> & {
    size?: SelectSize;
  }
>(({ className, size = 'md', children, ...props }, ref) => (
  <SelectPrimitive.Trigger
    ref={ref}
    className={cn(
      fieldSurface,
      sizeClass[size],
      'relative inline-flex cursor-pointer items-center text-left',
      'data-[placeholder]:text-cr-text-secondary [&>span]:truncate',
      className,
    )}
    {...props}
  >
    {children}
    <SelectPrimitive.Icon asChild>
      <ChevronDown
        aria-hidden
        className="pointer-events-none absolute right-2 top-1/2 size-4 -translate-y-1/2 text-cr-text-secondary"
      />
    </SelectPrimitive.Icon>
  </SelectPrimitive.Trigger>
));
SelectTrigger.displayName = 'SelectTrigger';

const scrollButton =
  'flex h-6 cursor-default items-center justify-center text-cr-text-secondary';

export const SelectContent = forwardRef<
  ElementRef<typeof SelectPrimitive.Content>,
  ComponentPropsWithoutRef<typeof SelectPrimitive.Content>
>(({ className, children, position = 'popper', ...props }, ref) => (
  <SelectPrimitive.Portal>
    <SelectPrimitive.Content
      ref={ref}
      position={position}
      className={cn(
        overlaySurface,
        'relative max-h-[min(var(--radix-select-content-available-height),360px)] min-w-[8rem] overflow-hidden',
        position === 'popper' &&
          'w-full min-w-[var(--radix-select-trigger-width)] data-[side=bottom]:translate-y-1 data-[side=top]:-translate-y-1',
        className,
      )}
      {...props}
    >
      <SelectPrimitive.ScrollUpButton className={scrollButton}>
        <ChevronUp aria-hidden className="size-4" />
      </SelectPrimitive.ScrollUpButton>
      <SelectPrimitive.Viewport className="p-1">
        {children}
      </SelectPrimitive.Viewport>
      <SelectPrimitive.ScrollDownButton className={scrollButton}>
        <ChevronDown aria-hidden className="size-4" />
      </SelectPrimitive.ScrollDownButton>
    </SelectPrimitive.Content>
  </SelectPrimitive.Portal>
));
SelectContent.displayName = 'SelectContent';

export const SelectLabel = forwardRef<
  ElementRef<typeof SelectPrimitive.Label>,
  ComponentPropsWithoutRef<typeof SelectPrimitive.Label>
>(({ className, ...props }, ref) => (
  <SelectPrimitive.Label
    ref={ref}
    className={cn(
      'px-2 pb-1 pt-1.5 text-caption text-cr-text-secondary',
      className,
    )}
    {...props}
  />
));
SelectLabel.displayName = 'SelectLabel';

export const SelectItem = forwardRef<
  ElementRef<typeof SelectPrimitive.Item>,
  ComponentPropsWithoutRef<typeof SelectPrimitive.Item>
>(({ className, children, ...props }, ref) => (
  <SelectPrimitive.Item
    ref={ref}
    className={cn(menuItem, 'pr-8', className)}
    {...props}
  >
    <SelectPrimitive.ItemText>{children}</SelectPrimitive.ItemText>
    <span className="absolute right-2 inline-flex size-4 items-center justify-center">
      <SelectPrimitive.ItemIndicator>
        <Check aria-hidden className="size-4 text-primary" />
      </SelectPrimitive.ItemIndicator>
    </span>
  </SelectPrimitive.Item>
));
SelectItem.displayName = 'SelectItem';

export const SelectSeparator = forwardRef<
  ElementRef<typeof SelectPrimitive.Separator>,
  ComponentPropsWithoutRef<typeof SelectPrimitive.Separator>
>(({ className, ...props }, ref) => (
  <SelectPrimitive.Separator
    ref={ref}
    className={cn('-mx-1 my-1 h-px bg-cr-border', className)}
    {...props}
  />
));
SelectSeparator.displayName = 'SelectSeparator';
