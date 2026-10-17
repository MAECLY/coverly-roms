import * as AlertDialogPrimitive from '@radix-ui/react-alert-dialog';
import {
  forwardRef,
  type ComponentPropsWithoutRef,
  type ElementRef,
  type HTMLAttributes,
} from 'react';

import { cn } from '@/lib/utils';

import { buttonVariants, type ButtonVariant } from './button';
import { CenterLayer, dialogContentVariants, overlayClass } from './dialog';

/**
 * A confirmation that interrupts (role="alertdialog"). Clicking outside does
 * not dismiss it; focus starts on the safe action (Cancel). Say what will
 * happen to the files in the description (brand §7).
 */
export const AlertDialog = AlertDialogPrimitive.Root;
export const AlertDialogTrigger = AlertDialogPrimitive.Trigger;
export const AlertDialogPortal = AlertDialogPrimitive.Portal;

export const AlertDialogContent = forwardRef<
  ElementRef<typeof AlertDialogPrimitive.Content>,
  ComponentPropsWithoutRef<typeof AlertDialogPrimitive.Content> & {
    size?: 'sm' | 'md';
  }
>(({ className, size = 'sm', ...props }, ref) => (
  <AlertDialogPortal>
    <AlertDialogPrimitive.Overlay className={overlayClass} />
    <CenterLayer>
      <AlertDialogPrimitive.Content
        ref={ref}
        className={cn(dialogContentVariants({ size }), className)}
        {...props}
      />
    </CenterLayer>
  </AlertDialogPortal>
));
AlertDialogContent.displayName = 'AlertDialogContent';

export function AlertDialogHeader({
  className,
  ...props
}: HTMLAttributes<HTMLDivElement>) {
  return <div className={cn('grid gap-1.5', className)} {...props} />;
}

export function AlertDialogFooter({
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

export const AlertDialogTitle = forwardRef<
  ElementRef<typeof AlertDialogPrimitive.Title>,
  ComponentPropsWithoutRef<typeof AlertDialogPrimitive.Title>
>(({ className, ...props }, ref) => (
  <AlertDialogPrimitive.Title
    ref={ref}
    className={cn('m-0 text-title font-semibold text-cr-text', className)}
    {...props}
  />
));
AlertDialogTitle.displayName = 'AlertDialogTitle';

export const AlertDialogDescription = forwardRef<
  ElementRef<typeof AlertDialogPrimitive.Description>,
  ComponentPropsWithoutRef<typeof AlertDialogPrimitive.Description>
>(({ className, ...props }, ref) => (
  <AlertDialogPrimitive.Description
    ref={ref}
    className={cn('m-0 text-dense text-cr-text-secondary', className)}
    {...props}
  />
));
AlertDialogDescription.displayName = 'AlertDialogDescription';

/** The confirming action. `variant="destructive"` for deletes and resets. */
export const AlertDialogAction = forwardRef<
  ElementRef<typeof AlertDialogPrimitive.Action>,
  ComponentPropsWithoutRef<typeof AlertDialogPrimitive.Action> & {
    variant?: Extract<ButtonVariant, 'primary' | 'destructive'>;
  }
>(({ className, variant = 'primary', ...props }, ref) => (
  <AlertDialogPrimitive.Action
    ref={ref}
    className={cn(buttonVariants({ variant, size: 'md' }), className)}
    {...props}
  />
));
AlertDialogAction.displayName = 'AlertDialogAction';

export const AlertDialogCancel = forwardRef<
  ElementRef<typeof AlertDialogPrimitive.Cancel>,
  ComponentPropsWithoutRef<typeof AlertDialogPrimitive.Cancel>
>(({ className, ...props }, ref) => (
  <AlertDialogPrimitive.Cancel
    ref={ref}
    className={cn(
      buttonVariants({ variant: 'outline', size: 'md' }),
      className,
    )}
    {...props}
  />
));
AlertDialogCancel.displayName = 'AlertDialogCancel';
