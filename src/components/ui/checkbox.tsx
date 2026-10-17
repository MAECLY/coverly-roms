import * as CheckboxPrimitive from '@radix-ui/react-checkbox';
import {
  forwardRef,
  type ComponentPropsWithoutRef,
  type ElementRef,
} from 'react';

import { cn, focusRing } from '@/lib/utils';

/**
 * The checkbox is a tiny mount (brand §2): a 3:4 portrait slot, dashed when
 * off, solid Buckram with a check when on. The dash is SVG, never
 * `border-style: dashed`. Indeterminate fills with a bar.
 */
export const Checkbox = forwardRef<
  ElementRef<typeof CheckboxPrimitive.Root>,
  ComponentPropsWithoutRef<typeof CheckboxPrimitive.Root>
>(({ className, ...props }, ref) => (
  <CheckboxPrimitive.Root
    ref={ref}
    className={cn(
      'group peer relative inline-grid h-[18px] w-[14px] shrink-0 place-items-center rounded-sm text-cr-slot-missing',
      'transition-colors duration-fast ease-standard',
      'hover:text-cr-text-secondary',
      'data-[state=checked]:bg-primary data-[state=checked]:text-primary-foreground',
      'data-[state=indeterminate]:bg-primary data-[state=indeterminate]:text-primary-foreground',
      'disabled:cursor-not-allowed disabled:opacity-60',
      'aria-[invalid=true]:text-cr-danger',
      focusRing,
      className,
    )}
    {...props}
  >
    <svg
      viewBox="0 0 14 18"
      aria-hidden
      className="pointer-events-none absolute inset-0 size-full group-data-[state=checked]:hidden group-data-[state=indeterminate]:hidden"
    >
      <rect
        x="0.75"
        y="0.75"
        width="12.5"
        height="16.5"
        rx="2.25"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeDasharray="3 2"
      />
    </svg>
    <CheckboxPrimitive.Indicator className="grid place-items-center">
      <svg viewBox="0 0 12 12" aria-hidden className="size-[11px]">
        <path
          className="hidden group-data-[state=checked]:block"
          d="M2.25 6.4 4.9 9l4.85-5.75"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.9"
          strokeLinecap="round"
          strokeLinejoin="round"
        />
        <path
          className="hidden group-data-[state=indeterminate]:block"
          d="M3 6h6"
          stroke="currentColor"
          strokeWidth="1.9"
          strokeLinecap="round"
        />
      </svg>
    </CheckboxPrimitive.Indicator>
  </CheckboxPrimitive.Root>
));
Checkbox.displayName = 'Checkbox';
