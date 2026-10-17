import { forwardRef, type HTMLAttributes, type ReactNode } from 'react';

import { cn } from '@/lib/utils';

import { SlotMark } from './slot-mark';

export interface EmptyStateProps extends Omit<
  HTMLAttributes<HTMLDivElement>,
  'title'
> {
  title: ReactNode;
  description?: ReactNode;
  /** Buttons; the main one first. */
  actions?: ReactNode;
  /** Replaces the default empty-shelf illustration. */
  illustration?: ReactNode;
  /** `sm` for panels and lists, `md` for a whole screen or region. */
  size?: 'sm' | 'md';
  titleAs?: 'h2' | 'h3' | 'h4' | 'p';
}

/** The default art: solid slots next to one empty mount on a shelf rule. */
function EmptyShelf({ size }: { size: 'sm' | 'md' }) {
  if (size === 'sm') return <SlotMark state="missing" size={32} />;
  return (
    <div className="flex flex-col items-center gap-2" aria-hidden>
      <div className="flex items-end gap-2">
        <SlotMark state="filled" size={48} className="opacity-25" />
        <SlotMark state="filled" size={48} className="opacity-25" />
        <SlotMark state="missing" size={64} />
      </div>
      <div className="relative h-px w-36 bg-cr-border before:absolute before:-top-px before:left-0 before:h-[3px] before:w-6 before:rounded-r-sm before:bg-primary" />
    </div>
  );
}

/** Calm, specific and actionable (brand §7): say what is missing and what
 * to do next. A small wink is allowed here, never in errors. */
export const EmptyState = forwardRef<HTMLDivElement, EmptyStateProps>(
  (
    {
      title,
      description,
      actions,
      illustration,
      size = 'md',
      titleAs: Title = 'h2',
      className,
      ...props
    },
    ref,
  ) => (
    <div
      ref={ref}
      className={cn(
        'flex flex-col items-center text-center',
        size === 'md' ? 'gap-4 px-6 py-12' : 'gap-2.5 px-4 py-6',
        className,
      )}
      {...props}
    >
      {illustration ?? <EmptyShelf size={size} />}
      <div
        className={cn('grid max-w-[44ch]', size === 'md' ? 'gap-1.5' : 'gap-1')}
      >
        <Title
          className={cn(
            'm-0 text-cr-text',
            size === 'md'
              ? 'font-display text-heading font-extrabold [font-stretch:75%]'
              : 'text-ui font-semibold',
          )}
        >
          {title}
        </Title>
        {description && (
          <p
            className={cn(
              'm-0 text-cr-text-secondary',
              size === 'md' ? 'text-dense' : 'text-caption',
            )}
          >
            {description}
          </p>
        )}
      </div>
      {actions && (
        <div className="flex flex-wrap justify-center gap-2">{actions}</div>
      )}
    </div>
  ),
);
EmptyState.displayName = 'EmptyState';
