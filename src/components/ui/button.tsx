import { Slot, Slottable } from '@radix-ui/react-slot';
import { cva, type VariantProps } from 'class-variance-authority';
import { forwardRef, type ButtonHTMLAttributes } from 'react';

import { cn, focusRing } from '@/lib/utils';

import { Spinner } from './spinner';

/**
 * Buttons (brand §4.1, §6): Buckram is the only interactive colour, radius
 * 5px, control heights 28 / 32 / 40. Disabled is a real state (sunken fill,
 * disabled ink), not only a fade.
 */
export const buttonVariants = cva(
  [
    'relative inline-flex shrink-0 select-none items-center justify-center gap-1.5 whitespace-nowrap rounded-md font-medium',
    'transition-[background-color,border-color,color,box-shadow] duration-fast ease-standard',
    "[&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4",
    'disabled:cursor-not-allowed aria-disabled:cursor-not-allowed',
    focusRing,
  ],
  {
    variants: {
      variant: {
        primary: [
          'bg-primary text-primary-foreground shadow-[inset_0_-1px_0_rgb(0_0_0/0.18)]',
          'hover:bg-cr-primary-hover active:shadow-none',
          'disabled:bg-cr-surface-sunken disabled:text-cr-text-disabled disabled:shadow-none',
        ],
        secondary: [
          'bg-cr-surface-sunken text-cr-text',
          'hover:bg-cr-hover active:bg-cr-active',
          'disabled:bg-cr-surface-sunken disabled:text-cr-text-disabled',
        ],
        outline: [
          'border border-cr-border bg-cr-surface text-cr-text',
          'hover:border-cr-border-strong hover:bg-cr-hover active:bg-cr-active',
          'disabled:border-cr-border disabled:bg-transparent disabled:text-cr-text-disabled',
        ],
        ghost: [
          'bg-transparent text-cr-text',
          'hover:bg-cr-hover active:bg-cr-active',
          'disabled:bg-transparent disabled:text-cr-text-disabled',
        ],
        destructive: [
          'bg-destructive text-destructive-foreground shadow-[inset_0_-1px_0_rgb(0_0_0/0.18)]',
          'hover:bg-destructive/90 active:shadow-none',
          'disabled:bg-cr-surface-sunken disabled:text-cr-text-disabled disabled:shadow-none',
        ],
        link: [
          'h-auto rounded-sm px-0 text-primary underline decoration-primary/40 underline-offset-[3px]',
          'hover:decoration-primary',
          'disabled:text-cr-text-disabled disabled:no-underline',
        ],
      },
      size: {
        sm: 'h-control-sm px-2.5 text-ui',
        md: 'h-control px-3 text-ui',
        lg: 'h-control-lg px-4 text-dense',
        icon: 'size-8 text-ui',
        'icon-sm': 'size-7 text-ui',
      },
    },
    compoundVariants: [
      { variant: 'link', size: ['sm', 'md', 'lg'], className: 'h-auto px-0' },
    ],
    defaultVariants: { variant: 'outline', size: 'md' },
  },
);

export type ButtonVariant = NonNullable<
  VariantProps<typeof buttonVariants>['variant']
>;
export type ButtonSize = NonNullable<
  VariantProps<typeof buttonVariants>['size']
>;

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: ButtonSize;
  /** Render the only child (e.g. an `<a>`) with button styling instead. */
  asChild?: boolean;
  /** Shows a spinner in place, sets `aria-busy` and disables the button. */
  loading?: boolean;
}

/** A native `<button type="button">` unless `type` or `asChild` say otherwise. */
export const Button = forwardRef<HTMLButtonElement, ButtonProps>(
  (
    {
      className,
      variant,
      size,
      asChild = false,
      loading = false,
      type,
      disabled,
      children,
      ...props
    },
    ref,
  ) => {
    const Comp = asChild ? Slot : 'button';
    return (
      <Comp
        ref={ref}
        className={cn(buttonVariants({ variant, size }), className)}
        {...(asChild ? {} : { type: type ?? 'button' })}
        disabled={disabled || loading || undefined}
        aria-busy={loading || undefined}
        {...props}
      >
        {loading && <Spinner size="sm" decorative />}
        <Slottable>{children}</Slottable>
      </Comp>
    );
  },
);
Button.displayName = 'Button';
