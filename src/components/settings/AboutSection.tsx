import { Logo } from '@/components/brand/Logo';
import { useAppInfo } from '@/hooks/useAppInfo';
import { useT } from '@/i18n/useT';
import { getBackend } from '@/ipc/backend';
import type { LinkId } from '@/ipc/types';
import { useSettingsStore } from '@/stores/settingsStore';

import { UpdatePanel } from '../update/UpdatePanel';

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
      <h2 className="wordmark-small">
        <Logo />
      </h2>
      <p>{t('footer.version', { version: info?.version ?? '—' })}</p>
      <p>{t('footer.coreVersion', { version: info?.core_version ?? '—' })}</p>
      <p>{t('about.license')}</p>
      <p className="hint">{t('about.noTelemetry')}</p>
      <p className="hint">
        {t('about.updates', {
          channel: t(`settings.general.channels.${channel}`),
        })}
      </p>
      <UpdatePanel />
      <div className="about-links">
        {LINKS.map((l) => (
          <button
            key={l}
            type="button"
            className="ghost-button"
            onClick={() => void getBackend().then((b) => b.openLink(l))}
          >
            {t(`about.link.${l}`)}
          </button>
        ))}
      </div>
    </div>
  );
}
