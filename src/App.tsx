import { useEffect, useState } from 'react';

import { useT } from './i18n/useT';
import { fetchAppInfo, type AppInfo } from './ipc/appInfo';

const NO_VALUE = '—';

export default function App() {
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
    <div className="flex min-h-screen flex-col">
      <main className="flex flex-1 flex-col items-center justify-center gap-6 px-6 text-center">
        <h1 className="wordmark">{t('app.name')}</h1>
        <p className="tagline max-w-xl">{t('app.tagline')}</p>
        <button
          type="button"
          className="primary-button"
          disabled
          aria-describedby="open-library-hint"
        >
          {t('start.openLibrary')}
        </button>
        <p id="open-library-hint" className="hint">
          {t('start.openLibraryHint')}
        </p>
      </main>
      <footer className="footer flex justify-center gap-4 px-6 py-3">
        <span>
          {t('footer.version', { version: info?.version ?? NO_VALUE })}
        </span>
        <span>
          {t('footer.coreVersion', { version: info?.core_version ?? NO_VALUE })}
        </span>
      </footer>
    </div>
  );
}
