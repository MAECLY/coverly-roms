import { KeyRound } from 'lucide-react';
import type { ReactNode } from 'react';

import { Button } from '@/components/ui/button';
import { useT } from '@/i18n/useT';
import { integration, integrationDomId } from '@/lib/integrations';
import { useNavStore } from '@/stores/navStore';

/**
 * «Configurar» next to a notice, or «Configurar en Integraciones» in
 * Fuentes: opens Settings on the card of a working integration (D15).
 * Back from Settings focuses it again (`data-settings-opener`). A planned
 * or unknown id has no card, so it renders nothing.
 */
export function IntegrationLink({
  id,
  children,
  variant = 'link',
  describedBy,
}: {
  id: string;
  children?: ReactNode;
  variant?: 'link' | 'button';
  /** The notice it acts on: the link alone only says «Configurar». */
  describedBy?: string | undefined;
}) {
  const t = useT();
  const entry = integration(id);
  if (!entry || entry.status !== 'working') return null;
  return (
    <Button
      size="sm"
      variant={variant === 'link' ? 'link' : 'outline'}
      data-settings-opener
      aria-describedby={describedBy}
      onClick={() =>
        useNavStore.getState().openSettings('integrations', {
          focus: integrationDomId(id),
        })
      }
    >
      {variant === 'button' && <KeyRound aria-hidden />}
      {children ?? t('integrations.configure')}
    </Button>
  );
}
