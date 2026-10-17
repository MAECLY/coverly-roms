import * as LabelPrimitive from '@radix-ui/react-label';
import {
  forwardRef,
  type ComponentPropsWithoutRef,
  type ElementRef,
} from 'react';

import { cn } from '@/lib/utils';

export const Label = forwardRef<
  ElementRef<typeof LabelPrimitive.Root>,
  ComponentPropsWithoutRef<typeof LabelPrimitive.Root>
>(({ className, ...props }, ref) => (
  <LabelPrimitive.Root
    ref={ref}
    className={cn(
      'select-none text-ui font-medium text-cr-text',
      'peer-disabled:cursor-not-allowed peer-disabled:text-cr-text-disabled',
      className,
    )}
    {...props}
  />
));
Label.displayName = 'Label';
