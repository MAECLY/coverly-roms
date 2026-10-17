import type { ReactNode } from 'react';

import { SimpleTooltip, TooltipProvider } from '@/components/ui/tooltip';

/**
 * A tooltip that brings its own provider, so library components also work
 * when rendered on their own (tests, other screens) without relying on a
 * TooltipProvider higher up. Nested providers are harmless in Radix.
 */
export function Tip({
  content,
  children,
  side,
}: {
  content: ReactNode;
  children: ReactNode;
  side?: 'top' | 'right' | 'bottom' | 'left';
}) {
  return (
    <TooltipProvider>
      <SimpleTooltip content={content} {...(side ? { side } : {})}>
        {children}
      </SimpleTooltip>
    </TooltipProvider>
  );
}
