import { useAppInfo } from '@/hooks/useAppInfo';
import { useT } from '@/i18n/useT';

const NO_VALUE = '—';

export function AppFooter() {
  const t = useT();
  const info = useAppInfo();
  return (
    <footer className="footer flex justify-center gap-4 px-6 py-3">
      <span>{t('footer.version', { version: info?.version ?? NO_VALUE })}</span>
      <span>
        {t('footer.coreVersion', { version: info?.core_version ?? NO_VALUE })}
      </span>
    </footer>
  );
}
