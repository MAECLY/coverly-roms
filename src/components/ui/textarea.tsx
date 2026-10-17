import { forwardRef, type TextareaHTMLAttributes } from 'react';

import { cn } from '@/lib/utils';

import { fieldSurface } from './input';

export interface TextareaProps extends TextareaHTMLAttributes<HTMLTextAreaElement> {
  /** Paths and lists of filenames read better in the mono face. */
  mono?: boolean;
}

export const Textarea = forwardRef<HTMLTextAreaElement, TextareaProps>(
  ({ className, mono = false, rows = 3, ...props }, ref) => (
    <textarea
      ref={ref}
      rows={rows}
      className={cn(
        fieldSurface,
        'min-h-16 resize-y px-2.5 py-1.5 text-ui',
        mono && 'font-mono text-caption',
        className,
      )}
      {...props}
    />
  ),
);
Textarea.displayName = 'Textarea';
