import { ExternalLink } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { Wordmark } from '@/components/Wordmark';
import { useAppInfo } from '@/hooks/useAppInfo';
import { useT } from '@/i18n/useT';
import { getBackend } from '@/ipc/backend';
import type { LinkId } from '@/ipc/types';
import { useSettingsStore } from '@/stores/settingsStore';

import { UpdatePanel } from '../update/UpdatePanel';
import { hintText } from './Field';

const LINKS: readonly LinkId[] = [
  'repo',
  'license',
  'notices',
  'trademarks',
  'releases',
  'issues',
];

export function AboutSection() {
  const t = useT();
  const info = useAppInfo();
  const channel = useSettingsStore(
    (s) => s.settings?.general.update_channel ?? 'stable',
  );
  return (
    <div className="settings-group">
      <Wordmark as="h2" size="sm" />
      <p>{t('footer.version', { version: info?.version ?? '—' })}</p>
      <p>{t('footer.coreVersion', { version: info?.core_version ?? '—' })}</p>
      <p>{t('about.license')}</p>
      <p className={hintText}>{t('about.noTelemetry')}</p>
      <p className={hintText}>
        {t('about.updates', {
          channel: t(`settings.general.channels.${channel}`),
        })}
      </p>
      <UpdatePanel />
      <div className="flex flex-wrap gap-x-5 gap-y-2">
        {LINKS.map((l) => (
          <Button
            key={l}
            variant="link"
            onClick={() => void getBackend().then((b) => b.openLink(l))}
          >
            {t(`about.link.${l}`)}
            <ExternalLink aria-hidden className="size-3.5" />
          </Button>
        ))}
      </div>
    </div>
  );
}
