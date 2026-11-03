import { useId } from 'react';

import { Alert } from '@/components/ui/alert';
import { useT } from '@/i18n/useT';
import type {
  IntegrationNotice as Notice,
  IntegrationProblem,
} from '@/ipc/types';
import { integration, integrationName } from '@/lib/integrations';

import { IntegrationLink } from './IntegrationLink';

const COPY: Record<IntegrationProblem, string> = {
  missing: 'integrations.notice.missing',
  rejected: 'integrations.notice.rejected',
  quota: 'integrations.notice.quota',
  off: 'integrations.notice.off',
};

/**
 * What a missing or refused service changes on another screen (D27, D28):
 * one line («SteamGridDB sin clave») and «Configurar», which opens its
 * card. An account service says «sin cuenta» rather than «sin clave».
 */
export function IntegrationNotice({ notice }: { notice: Notice }) {
  const t = useT();
  const textId = useId();
  const entry = integration(notice.integration);
  if (!entry) return null;
  const key =
    notice.problem === 'missing' && entry.kind === 'account'
      ? 'integrations.notice.missingAccount'
      : COPY[notice.problem];
  return (
    <Alert
      variant="warning"
      role="note"
      action={<IntegrationLink id={entry.id} describedBy={textId} />}
    >
      <p id={textId} className="m-0">
        {t(key, { name: integrationName(entry, t) })}
      </p>
    </Alert>
  );
}
