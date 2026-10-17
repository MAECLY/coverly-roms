import * as DialogPrimitive from '@radix-ui/react-dialog';
import { cva, type VariantProps } from 'class-variance-authority';
import { X } from 'lucide-react';
import {
  forwardRef,
  type ComponentPropsWithoutRef,
  type ElementRef,
  type HTMLAttributes,
  type ReactNode,
} from 'react';

import { useT } from '@/i18n/useT';
import { cn } from '@/lib/utils';

import { Button } from './button';

/**
 * Modal dialog: portalled, background inert, scroll locked, Escape and focus
 * scoped to it, focus returned to the opener. Title is required by Radix for
 * the accessible name; DialogDescription becomes `aria-describedby`.
 */
export const Dialog = DialogPrimitive.Root;
export const DialogTrigger = DialogPrimitive.Trigger;
export const DialogPortal = DialogPrimitive.Portal;
export const DialogClose = DialogPrimitive.Close;

/** Modal scrim (brand §6): rgb(13 15 15 / 55%) in both themes. */
export const overlayClass = cn(
  'fixed inset-0 z-50 bg-[rgb(13_15_15/0.55)]',
  'data-[state=open]:animate-in data-[state=open]:fade-in-0 data-[state=open]:duration-slow data-[state=open]:ease-enter',
  'data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=closed]:duration-base data-[state=closed]:ease-exit',
  'motion-reduce:animate-none',
);

export const DialogOverlay = forwardRef<
  ElementRef<typeof DialogPrimitive.Overlay>,
  ComponentPropsWithoutRef<typeof DialogPrimitive.Overlay>
>(({ className, ...props }, ref) => (
  <DialogPrimitive.Overlay
    ref={ref}
    className={cn(overlayClass, className)}
    {...props}
  />
));
DialogOverlay.displayName = 'DialogOverlay';

export const dialogContentVariants = cva(
  [
    'pointer-events-auto relative flex max-h-[calc(100dvh-48px)] w-full flex-col gap-4 overflow-hidden rounded-xl border border-cr-border bg-popover p-5 text-popover-foreground shadow-overlay',
    'data-[state=open]:animate-in data-[state=open]:fade-in-0 data-[state=open]:zoom-in-[0.98] data-[state=open]:slide-in-from-bottom-2 data-[state=open]:duration-slow data-[state=open]:ease-enter',
    'data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=closed]:zoom-out-[0.98] data-[state=closed]:duration-base data-[state=closed]:ease-exit',
    'motion-reduce:animate-none',
    'focus-visible:outline-none',
  ],
  {
    variants: {
      size: {
        sm: 'max-w-[400px]',
        md: 'max-w-[520px]',
        lg: 'max-w-[720px]',
        xl: 'max-w-[960px]',
      },
    },
    defaultVariants: { size: 'md' },
  },
);

/** Centres its child without transforms, so the enter animation's transform
 * never fights the centring. Clicks on it fall through to the overlay. */
export function CenterLayer({ children }: { children: ReactNode }) {
  return (
    <div className="pointer-events-none fixed inset-0 z-50 grid place-items-center p-6">
      {children}
    </div>
  );
}

export interface DialogContentProps
  extends
    ComponentPropsWithoutRef<typeof DialogPrimitive.Content>,
    VariantProps<typeof dialogContentVariants> {
  /** Hide the corner close button (e.g. a wizard with its own footer). */
  hideClose?: boolean;
}

export const DialogContent = forwardRef<
  ElementRef<typeof DialogPrimitive.Content>,
  DialogContentProps
>(({ className, children, size, hideClose = false, ...props }, ref) => {
  const t = useT();
  return (
    <DialogPortal>
      <DialogOverlay />
      <CenterLayer>
        <DialogPrimitive.Content
          ref={ref}
          className={cn(dialogContentVariants({ size }), className)}
          {...props}
        >
          {children}
          {!hideClose && (
            <DialogPrimitive.Close asChild>
              <Button
                variant="ghost"
                size="icon-sm"
                className="absolute right-3 top-3 text-cr-text-secondary"
                aria-label={t('ui.close')}
              >
                <X aria-hidden />
              </Button>
            </DialogPrimitive.Close>
          )}
        </DialogPrimitive.Content>
      </CenterLayer>
    </DialogPortal>
  );
});
DialogContent.displayName = 'DialogContent';

export function DialogHeader({
  className,
  ...props
}: HTMLAttributes<HTMLDivElement>) {
  return <div className={cn('grid gap-1.5 pr-8', className)} {...props} />;
}

/** Scrolls when the content is taller than the viewport. */
export function DialogBody({
  className,
  ...props
}: HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      className={cn(
        '-mx-5 min-h-0 flex-1 overflow-y-auto px-5 text-dense',
        className,
      )}
      {...props}
    />
  );
}

/** Actions, right-aligned; the primary action goes last. */
export function DialogFooter({
  className,
  ...props
}: HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      className={cn(
        'flex flex-wrap items-center justify-end gap-2 pt-1',
        className,
      )}
      {...props}
    />
  );
}

export const DialogTitle = forwardRef<
  ElementRef<typeof DialogPrimitive.Title>,
  ComponentPropsWithoutRef<typeof DialogPrimitive.Title>
>(({ className, ...props }, ref) => (
  <DialogPrimitive.Title
    ref={ref}
    className={cn('m-0 text-title font-semibold text-cr-text', className)}
    {...props}
  />
));
DialogTitle.displayName = 'DialogTitle';

export const DialogDescription = forwardRef<
  ElementRef<typeof DialogPrimitive.Description>,
  ComponentPropsWithoutRef<typeof DialogPrimitive.Description>
>(({ className, ...props }, ref) => (
  <DialogPrimitive.Description
    ref={ref}
    className={cn('m-0 text-dense text-cr-text-secondary', className)}
    {...props}
  />
));
DialogDescription.displayName = 'DialogDescription';
