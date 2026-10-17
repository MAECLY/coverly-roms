import { cva, type VariantProps } from 'class-variance-authority';
import { CircleAlert, CircleCheck, Info, TriangleAlert, X } from 'lucide-react';
import { forwardRef, type HTMLAttributes, type ReactNode } from 'react';

import { useT } from '@/i18n/useT';
import { cn } from '@/lib/utils';

import { Button } from './button';

/**
 * Inline banner. Status colour always comes with an icon (brand §4.1), and
 * a 3px cap on the left (the shelf-rule spine). `danger` is `role="alert"`,
 * the rest `role="status"`. Errors say what was NOT touched (brand §7).
 */
export const alertVariants = cva(
  'relative grid grid-cols-[auto_1fr_auto] items-start gap-x-2.5 rounded-md border-l-[3px] py-2.5 pl-3 pr-2 text-ui text-cr-text',
  {
    variants: {
      variant: {
        neutral:
          'border-l-cr-text-secondary bg-cr-surface-sunken [--alert-icon:var(--cr-text-secondary)]',
        info: 'border-l-cr-info bg-cr-info-bg [--alert-icon:var(--cr-info)]',
        success:
          'border-l-cr-success bg-cr-success-bg [--alert-icon:var(--cr-success)]',
        warning:
          'border-l-cr-warning bg-cr-warning-bg [--alert-icon:var(--cr-warning)]',
        danger:
          'border-l-cr-danger bg-cr-danger-bg [--alert-icon:var(--cr-danger)]',
      },
    },
    defaultVariants: { variant: 'info' },
  },
);

type AlertVariant = NonNullable<VariantProps<typeof alertVariants>['variant']>;

const ICONS: Record<AlertVariant, typeof Info> = {
  neutral: Info,
  info: Info,
  success: CircleCheck,
  warning: TriangleAlert,
  danger: CircleAlert,
};

export interface AlertProps
  extends HTMLAttributes<HTMLDivElement>, VariantProps<typeof alertVariants> {
  /** Replace the status icon (keep one: colour alone is not enough). */
  icon?: ReactNode;
  /** Buttons on the right (e.g. Retry, Undo). */
  action?: ReactNode;
  /** Shows a dismiss button labelled with the translated "Dismiss". */
  onDismiss?: () => void;
}

export const Alert = forwardRef<HTMLDivElement, AlertProps>(
  (
    { className, variant, icon, action, onDismiss, role, children, ...props },
    ref,
  ) => {
    const t = useT();
    const v = variant ?? 'info';
    const Icon = ICONS[v];
    return (
      <div
        ref={ref}
        role={role ?? (v === 'danger' ? 'alert' : 'status')}
        data-variant={v}
        className={cn(alertVariants({ variant: v }), className)}
        {...props}
      >
        <span
          className="mt-px text-[color:var(--alert-icon)] [&_svg]:size-4"
          aria-hidden
        >
          {icon ?? <Icon />}
        </span>
        <div className="grid min-w-0 gap-0.5">{children}</div>
        <div className="-my-1 flex items-center gap-1">
          {action}
          {onDismiss && (
            <Button
              variant="ghost"
              size="icon-sm"
              className="text-cr-text-secondary hover:bg-cr-text/5"
              aria-label={t('ui.dismiss')}
              onClick={onDismiss}
            >
              <X aria-hidden />
            </Button>
          )}
        </div>
      </div>
    );
  },
);
Alert.displayName = 'Alert';

export function AlertTitle({
  className,
  ...props
}: HTMLAttributes<HTMLParagraphElement>) {
  return <p className={cn('m-0 font-semibold', className)} {...props} />;
}

export function AlertDescription({
  className,
  ...props
}: HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      className={cn('text-cr-text-secondary [&_p]:m-0', className)}
      {...props}
    />
  );
}
