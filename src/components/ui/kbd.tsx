import { forwardRef, type HTMLAttributes } from 'react';

import { cn } from '@/lib/utils';

/** A key cap: `<Kbd>/</Kbd>`, `<Kbd>⌘</Kbd><Kbd>K</Kbd>`. */
export const Kbd = forwardRef<HTMLElement, HTMLAttributes<HTMLElement>>(
  ({ className, ...props }, ref) => (
    <kbd
      ref={ref}
      className={cn(
        'inline-flex h-5 min-w-5 items-center justify-center rounded-sm border border-cr-border border-b-[2px] bg-cr-surface-raised px-1 font-mono text-[11px] font-medium leading-none text-cr-text-secondary',
        className,
      )}
      {...props}
    />
  ),
);
Kbd.displayName = 'Kbd';

export function KbdGroup({
  className,
  ...props
}: HTMLAttributes<HTMLSpanElement>) {
  return (
    <span
      className={cn('inline-flex items-center gap-0.5', className)}
      {...props}
    />
  );
}
