import { Folder, FolderOpen, HardDrive, Settings, X } from 'lucide-react';
import { useContext, useEffect, useMemo, useState } from 'react';

import { AppFooter } from '@/components/AppFooter';
import { LanguageSwitch } from '@/components/LanguageSwitch';
import { ScanProgressView } from '@/components/ScanProgressView';
import { Alert } from '@/components/ui/alert';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { EmptyState } from '@/components/ui/empty-state';
import { SimpleTooltip, TooltipProvider } from '@/components/ui/tooltip';
import { UpdateBanner } from '@/components/update/UpdateBanner';
import { Wordmark } from '@/components/Wordmark';
import { LocaleContext } from '@/i18n/core';
import { useT } from '@/i18n/useT';
import { getBackend } from '@/ipc/backend';
import { errorKey } from '@/ipc/errors';
import type { Volume } from '@/ipc/types';
import { useLibraryStore } from '@/stores/libraryStore';
import { useNavStore } from '@/stores/navStore';

/** A volume or recent library row: an interactive card (whole row opens). */
const rowClass =
  'grid h-auto min-w-0 flex-1 grid-cols-[auto_minmax(0,1fr)_auto] items-center justify-stretch gap-x-3 gap-y-0.5 whitespace-normal rounded-lg px-4 py-3 text-left font-normal';
const rowName = 'col-start-2 truncate text-ui font-semibold text-cr-text';
const rowPath =
  'col-start-2 truncate font-mono text-caption text-cr-text-secondary';
const rowIcon =
  'row-span-2 size-5 text-cr-text-secondary group-disabled:text-cr-text-disabled';
const sectionTitle = 'm-0 mb-2 text-dense font-semibold text-cr-text';

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
  const [volumes, setVolumes] = useState<Volume[]>([]);

  useEffect(() => {
    void loadRecents();
  }, [loadRecents]);

  // Drives come and go: list them now and again whenever the window
  // regains focus (after plugging in an SD card). Only the newest answer
  // counts; a failed or malformed listing shows as no drives.
  useEffect(() => {
    let latest = 0;
    function reload() {
      const ask = ++latest;
      void getBackend()
        .then((b) => b.mountedVolumes())
        .then(
          (list) => {
            if (ask === latest) setVolumes(Array.isArray(list) ? list : []);
          },
          () => {
            if (ask === latest) setVolumes([]);
          },
        );
    }
    reload();
    window.addEventListener('focus', reload);
    return () => {
      latest = -1;
      window.removeEventListener('focus', reload);
    };
  }, []);

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
    <TooltipProvider>
      <div className="flex min-h-screen flex-col">
        <header className="flex items-center justify-end gap-2 border-b border-cr-border bg-cr-surface px-6 py-2.5">
          <Button
            variant="ghost"
            size="sm"
            data-settings-opener
            onClick={() => useNavStore.getState().openSettings()}
          >
            <Settings aria-hidden />
            {t('settings.open')}
          </Button>
          <LanguageSwitch />
        </header>
        <main className="flex flex-1 flex-col items-center gap-6 px-6 py-10 text-center">
          <UpdateBanner className="m-0 w-full max-w-2xl" />
          <Wordmark as="h1" />
          <p className="tagline max-w-xl">{t('app.tagline')}</p>
          <Button
            variant="primary"
            size="lg"
            loading={opening}
            onClick={() => void pick()}
          >
            {!opening && <FolderOpen aria-hidden />}
            {t('start.openFolder')}
          </Button>
          <p className="hint">{t('start.dropHint')}</p>
          {opening && (
            <ScanProgressView path={openingPath} progress={progress} />
          )}
          {status === 'error' && error && (
            <Alert variant="danger" className="w-full max-w-2xl text-left">
              <p className="m-0">
                {t(errorKey(error.code), { detail: error.detail ?? '' })}
              </p>
            </Alert>
          )}
          <section
            aria-labelledby="volumes-heading"
            className="w-full max-w-2xl text-left"
          >
            <h2 id="volumes-heading" className={sectionTitle}>
              {t('start.volumes')}
            </h2>
            {volumes.length === 0 ? (
              <EmptyState
                size="sm"
                titleAs="p"
                title={t('start.noVolumes')}
                className="rounded-lg border border-dashed border-cr-border"
              />
            ) : (
              <ul className="m-0 flex list-none flex-col gap-2 p-0">
                {volumes.map((v) => (
                  <li key={v.path} className="flex items-center gap-2">
                    <Button
                      className={`group ${rowClass}`}
                      aria-label={t('start.openVolume', { name: v.name })}
                      disabled={opening}
                      onClick={() => void open(v.path)}
                    >
                      <HardDrive aria-hidden className={rowIcon} />
                      <span className={rowName} title={v.name}>
                        {v.name}
                      </span>
                      <span className={rowPath} title={v.path}>
                        {v.path}
                      </span>
                    </Button>
                    {/* Keeps drive rows the same width as the recent rows,
                        which carry a forget button in this column. */}
                    <span aria-hidden className="size-8 shrink-0" />
                  </li>
                ))}
              </ul>
            )}
          </section>
          <section
            aria-labelledby="recent-heading"
            className="w-full max-w-2xl text-left"
          >
            <h2 id="recent-heading" className={sectionTitle}>
              {t('start.recent')}
            </h2>
            {recents.length === 0 ? (
              <EmptyState
                size="sm"
                titleAs="p"
                title={t('start.noRecent')}
                className="rounded-lg border border-dashed border-cr-border"
              />
            ) : (
              <ul className="m-0 flex list-none flex-col gap-2 p-0">
                {recents.map((r) => (
                  <li key={r.path} className="flex items-center gap-2">
                    <Button
                      className={`group ${rowClass}`}
                      disabled={!r.available || opening}
                      onClick={() => void open(r.path)}
                    >
                      <Folder aria-hidden className={rowIcon} />
                      <span className={rowName} title={r.name}>
                        {r.name}
                      </span>
                      <span className={rowPath} title={r.path}>
                        {r.path}
                      </span>
                      {r.available ? (
                        <Badge className="col-start-3 row-span-2 row-start-1">
                          {t('start.roms', { n: n.format(r.roms) })}
                        </Badge>
                      ) : (
                        <Badge
                          variant="outline"
                          className="col-start-3 row-span-2 row-start-1"
                        >
                          {t('start.notConnected')}
                        </Badge>
                      )}
                    </Button>
                    <SimpleTooltip
                      content={t('start.forget', { name: r.name })}
                    >
                      <Button
                        variant="ghost"
                        size="icon"
                        className="text-cr-text-secondary"
                        aria-label={t('start.forget', { name: r.name })}
                        onClick={() => void forget(r.path)}
                      >
                        <X aria-hidden />
                      </Button>
                    </SimpleTooltip>
                  </li>
                ))}
              </ul>
            )}
          </section>
        </main>
        <AppFooter />
      </div>
    </TooltipProvider>
  );
}
