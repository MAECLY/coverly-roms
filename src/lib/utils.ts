import { clsx, type ClassValue } from 'clsx';
import { extendTailwindMerge } from 'tailwind-merge';

/** tailwind-merge taught the brand scale from tailwind.config.ts, so
 * `cn('h-control', 'h-control-lg')` keeps only the last height. */
const twMerge = extendTailwindMerge({
  extend: {
    classGroups: {
      'font-size': [
        {
          text: [
            'caption',
            'ui',
            'dense',
            'dialog',
            'title',
            'heading',
            'display',
          ],
        },
      ],
      h: [{ h: ['control-sm', 'control', 'control-lg', 'control-landing'] }],
      shadow: [{ shadow: ['raised', 'overlay'] }],
      duration: [{ duration: ['instant', 'fast', 'base', 'slow', 'slower'] }],
      ease: [{ ease: ['standard', 'enter', 'exit', 'ink', 'spring'] }],
    },
  },
});

/** Joins class names and lets later Tailwind utilities win over earlier ones. */
export function cn(...inputs: ClassValue[]): string {
  return twMerge(clsx(inputs));
}

/**
 * The brand focus ring (brand §6): 2px Buckram outline, 2px offset, keyboard
 * only. Every interactive primitive in src/components/ui uses it.
 */
export const focusRing =
  'focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring';
