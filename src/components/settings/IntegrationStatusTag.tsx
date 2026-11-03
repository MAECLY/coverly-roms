import {
  Check,
  CircleDashed,
  CircleSlash,
  Gauge,
  KeyRound,
  TriangleAlert,
  type LucideIcon,
} from 'lucide-react';

import { Badge, type BadgeProps } from '@/components/ui/badge';
import { useT } from '@/i18n/useT';
import type { IntegrationState } from '@/ipc/types';
import type { CardState } from '@/lib/integrations';
import { cn } from '@/lib/utils';

/** The brand's Slot language on a `Badge` (spec §6.2, §6.3): dashed
 * outline for «not yet», solid Buckram for connected, solid Manila for
 * stored, warning/danger for quota and error. Always a word and an icon. */
const LOOK: Record<
  IntegrationState,
  {
    variant: NonNullable<BadgeProps['variant']>;
    icon: LucideIcon;
    dashed?: string;
  }
> = {
  off: {
    variant: 'outline',
    icon: CircleSlash,
    dashed: 'text-cr-text-secondary',
  },
  unavailable: {
    variant: 'outline',
    icon: CircleDashed,
    dashed: 'text-cr-text-secondary',
  },
  'not-configured': {
    variant: 'outline',
    icon: CircleDashed,
    dashed: 'text-cr-warning',
  },
  saved: { variant: 'tag', icon: KeyRound },
  connected: { variant: 'primary', icon: Check },
  error: { variant: 'danger', icon: TriangleAlert },
  quota: { variant: 'warning', icon: Gauge },
};

/** An integration's status tag. A catalogue `CardState` shows its own word
 * («Descargando», «Listo»); `label` replaces the word altogether. */
export function IntegrationStatusTag({
  state,
  label,
  className,
}: {
  state: IntegrationState | CardState;
  label?: string | undefined;
  className?: string | undefined;
}) {
  const t = useT();
  const card = typeof state === 'string' ? { state } : state;
  const look = LOOK[card.state];
  const Icon = look.icon;
  const word =
    label ??
    t(
      `integrations.state.${'label' in card && card.label ? card.label : card.state}`,
    );
  return (
    <Badge
      variant={look.variant}
      data-state={card.state}
      className={cn(
        look.dashed && ['border-dashed border-cr-border-strong', look.dashed],
        className,
      )}
    >
      <Icon aria-hidden />
      {word}
    </Badge>
  );
}
