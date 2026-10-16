import { useContext, useEffect, useMemo } from 'react';

import { AppFooter } from '@/components/AppFooter';
import { LanguageSwitch } from '@/components/LanguageSwitch';
import { ScanProgressView } from '@/components/ScanProgressView';
import { UpdateBanner } from '@/components/update/UpdateBanner';
import { LocaleContext } from '@/i18n/core';
import { useT } from '@/i18n/useT';
import { getBackend } from '@/ipc/backend';
import { errorKey } from '@/ipc/errors';
import { useLibraryStore } from '@/stores/libraryStore';
import { useNavStore } from '@/stores/navStore';

export function StartScreen() {
  const t = useT();
  const locale = useContext(LocaleContext);
  const n = useMemo(() => new Intl.NumberFormat(locale), [locale]);
  const status = useLibraryStore((s) => s.status);
  const recents = useLibraryStore((s) => s.recents);
  const progress = useLibraryStore((s) => s.progress);
  const error = useLibraryStore((s) => s.error);
  const openingPath = useLibraryStore((s) => s.openingPath);
  const loadRecents = useLibraryStore((s) => s.loadRecents);
  const open = useLibraryStore((s) => s.open);
  const forget = useLibraryStore((s) => s.forget);
  const fail = useLibraryStore((s) => s.fail);
  const opening = status === 'opening';

  useEffect(() => {
    void loadRecents();
  }, [loadRecents]);

  useEffect(() => {
    let alive = true;
    let off: (() => void) | undefined;
    void getBackend()
      .then((b) => b.onFolderDrop((path) => void open(path)))
      .then(
        (unlisten) => {
          if (alive) off = unlisten;
          else unlisten();
        },
        () => undefined,
      );
    return () => {
      alive = false;
      off?.();
    };
  }, [open]);

  async function pick() {
    let path: string | null;
    try {
      const backend = await getBackend();
      path = await backend.pickFolder();
    } catch (e) {
      fail(e);
      return;
    }
    if (path) await open(path);
  }

  return (
    <div className="flex min-h-screen flex-col">
      <header className="app-header flex items-center justify-end gap-3 px-6 py-3">
        <button
          type="button"
          className="ghost-button"
          data-settings-opener
          onClick={() => useNavStore.getState().openSettings()}
        >
          {t('settings.open')}
        </button>
        <LanguageSwitch />
      </header>
      <main className="flex flex-1 flex-col items-center gap-6 px-6 py-10 text-center">
        <UpdateBanner />
        <h1 className="wordmark">{t('app.name')}</h1>
        <p className="tagline max-w-xl">{t('app.tagline')}</p>
        <button
          type="button"
          className="primary-button"
          disabled={opening}
          onClick={() => void pick()}
        >
          {t('start.openFolder')}
        </button>
        <p className="hint">{t('start.dropHint')}</p>
        {opening && <ScanProgressView path={openingPath} progress={progress} />}
        {status === 'error' && error && (
          <p role="alert" className="error-banner">
            {t(errorKey(error.code), { detail: error.detail ?? '' })}
          </p>
        )}
        <section
          aria-labelledby="recent-heading"
          className="recent w-full max-w-2xl text-left"
        >
          <h2 id="recent-heading" className="section-title">
            {t('start.recent')}
          </h2>
          {recents.length === 0 ? (
            <p className="hint">{t('start.noRecent')}</p>
          ) : (
            <ul className="recent-list">
              {recents.map((r) => (
                <li key={r.path} className="recent-item">
                  <button
                    type="button"
                    className="recent-open"
                    disabled={!r.available || opening}
                    onClick={() => void open(r.path)}
                  >
                    <span className="recent-name truncate" title={r.name}>
                      {r.name}
                    </span>
                    <span className="recent-path truncate" title={r.path}>
                      {r.path}
                    </span>
                    <span className="recent-meta">
                      {r.available
                        ? t('start.roms', { n: n.format(r.roms) })
                        : t('start.notConnected')}
                    </span>
                  </button>
                  <button
                    type="button"
                    className="icon-button"
                    aria-label={t('start.forget', { name: r.name })}
                    onClick={() => void forget(r.path)}
                  >
                    ×
                  </button>
                </li>
              ))}
            </ul>
          )}
        </section>
      </main>
      <AppFooter />
    </div>
  );
}
