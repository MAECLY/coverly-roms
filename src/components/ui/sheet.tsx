import * as DialogPrimitive from '@radix-ui/react-dialog';
import { cva, type VariantProps } from 'class-variance-authority';
import { X } from 'lucide-react';
import {
  forwardRef,
  type ComponentPropsWithoutRef,
  type ElementRef,
  type HTMLAttributes,
} from 'react';

import { useT } from '@/i18n/useT';
import { cn } from '@/lib/utils';

import { Button } from './button';
import { DialogDescription, DialogTitle, overlayClass } from './dialog';

/**
 * A modal panel docked to an edge (e.g. Jobs as `side="right"`). Same
 * behaviour as Dialog. The library DetailDrawer is non-modal and is NOT a
 * Sheet (see component-audit §12).
 */
export const Sheet = DialogPrimitive.Root;
export const SheetTrigger = DialogPrimitive.Trigger;
export const SheetClose = DialogPrimitive.Close;
export const SheetTitle = DialogTitle;
export const SheetDescription = DialogDescription;

export const sheetVariants = cva(
  [
    'fixed z-50 flex flex-col gap-4 border-cr-border bg-popover p-5 text-popover-foreground shadow-overlay',
    'data-[state=open]:animate-in data-[state=open]:duration-slow data-[state=open]:ease-enter',
    'data-[state=closed]:animate-out data-[state=closed]:duration-base data-[state=closed]:ease-exit',
    'motion-reduce:animate-none focus-visible:outline-none',
  ],
  {
    variants: {
      side: {
        right:
          'inset-y-0 right-0 h-full w-[min(440px,100vw-48px)] border-l data-[state=open]:slide-in-from-right data-[state=closed]:slide-out-to-right',
        left: 'inset-y-0 left-0 h-full w-[min(440px,100vw-48px)] border-r data-[state=open]:slide-in-from-left data-[state=closed]:slide-out-to-left',
        top: 'inset-x-0 top-0 max-h-[80dvh] border-b data-[state=open]:slide-in-from-top data-[state=closed]:slide-out-to-top',
        bottom:
          'inset-x-0 bottom-0 max-h-[80dvh] border-t data-[state=open]:slide-in-from-bottom data-[state=closed]:slide-out-to-bottom',
      },
    },
    defaultVariants: { side: 'right' },
  },
);

export const SheetContent = forwardRef<
  ElementRef<typeof DialogPrimitive.Content>,
  ComponentPropsWithoutRef<typeof DialogPrimitive.Content> &
    VariantProps<typeof sheetVariants>
>(({ className, side, children, ...props }, ref) => {
  const t = useT();
  return (
    <DialogPrimitive.Portal>
      <DialogPrimitive.Overlay className={overlayClass} />
      <DialogPrimitive.Content
        ref={ref}
        className={cn(sheetVariants({ side }), className)}
        {...props}
      >
        {children}
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
      </DialogPrimitive.Content>
    </DialogPrimitive.Portal>
  );
});
SheetContent.displayName = 'SheetContent';

export function SheetHeader({
  className,
  ...props
}: HTMLAttributes<HTMLDivElement>) {
  return <div className={cn('grid gap-1.5 pr-8', className)} {...props} />;
}

export function SheetFooter({
  className,
  ...props
}: HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      className={cn(
        'mt-auto flex flex-wrap items-center justify-end gap-2',
        className,
      )}
      {...props}
    />
  );
}
