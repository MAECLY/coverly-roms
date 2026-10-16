import { useEffect, useState } from 'react';

import { useT } from '@/i18n/useT';
import { fetchAppInfo, type AppInfo } from '@/ipc/appInfo';

const NO_VALUE = '—';

export function AppFooter() {
  const t = useT();
  const [info, setInfo] = useState<AppInfo | null>(null);
  useEffect(() => {
    let cancelled = false;
    void fetchAppInfo().then((result) => {
      if (!cancelled) setInfo(result);
    });
    return () => {
      cancelled = true;
    };
  }, []);
  return (
    <footer className="footer flex justify-center gap-4 px-6 py-3">
      <span>{t('footer.version', { version: info?.version ?? NO_VALUE })}</span>
      <span>
        {t('footer.coreVersion', { version: info?.core_version ?? NO_VALUE })}
      </span>
    </footer>
  );
}
