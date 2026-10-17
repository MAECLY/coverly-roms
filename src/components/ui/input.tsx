import { cva, type VariantProps } from 'class-variance-authority';
import { forwardRef, type InputHTMLAttributes } from 'react';

import { cn, focusRing } from '@/lib/utils';

/** Shared by Input, Textarea and NativeSelect: a sunken well (brand §6
 * elevation) with a ≥ 3:1 `border-strong` edge (WCAG 1.4.11). */
export const fieldSurface = cn(
  'w-full min-w-0 rounded-md border border-input bg-cr-surface-sunken text-cr-text',
  'placeholder:text-cr-text-secondary',
  'transition-[border-color,background-color] duration-fast ease-standard',
  'hover:border-cr-text-secondary',
  'disabled:cursor-not-allowed disabled:border-cr-border disabled:text-cr-text-disabled',
  'aria-[invalid=true]:border-cr-danger',
  focusRing,
  'focus-visible:outline-offset-0 focus-visible:border-ring',
);

export const inputVariants = cva(fieldSurface, {
  variants: {
    size: {
      sm: 'h-control-sm px-2 text-ui',
      md: 'h-control px-2.5 text-ui',
      lg: 'h-control-lg px-3 text-dense',
    },
    mono: { true: 'font-mono text-caption tabular-nums', false: '' },
  },
  defaultVariants: { size: 'md', mono: false },
});

export interface InputProps extends Omit<
  InputHTMLAttributes<HTMLInputElement>,
  'size'
> {
  size?: NonNullable<VariantProps<typeof inputVariants>['size']>;
  /** Paths, filenames, keys: the label-maker readout. */
  mono?: boolean;
}

export const Input = forwardRef<HTMLInputElement, InputProps>(
  ({ className, size, mono, type = 'text', ...props }, ref) => (
    <input
      ref={ref}
      type={type}
      className={cn(
        inputVariants({ size, mono }),
        'file:mr-2 file:border-0 file:bg-transparent file:font-medium file:text-cr-text',
        '[&::-webkit-search-cancel-button]:cursor-pointer',
        className,
      )}
      {...props}
    />
  ),
);
Input.displayName = 'Input';
