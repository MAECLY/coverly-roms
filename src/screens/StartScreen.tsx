import { useContext, useEffect, useMemo, useState } from 'react';

import { AppFooter } from '@/components/AppFooter';
import { LanguageSwitch } from '@/components/LanguageSwitch';
import {
  DEFAULT_LAYOUT,
  OrganizeOffer,
} from '@/components/organize/OrganizeOffer';
import { ScanProgressView } from '@/components/ScanProgressView';
import { UpdateBanner } from '@/components/update/UpdateBanner';
import { LocaleContext } from '@/i18n/core';
import { useT } from '@/i18n/useT';
import { getBackend } from '@/ipc/backend';
import { errorKey, type BackendError } from '@/ipc/errors';
import type { FolderProbe, Volume } from '@/ipc/types';
import { useLibraryStore } from '@/stores/libraryStore';
import { useNavStore } from '@/stores/navStore';
import { useSettingsStore } from '@/stores/settingsStore';

/** The probe of the folder a `no-profile` error is about. */
interface ProbeState {
  error: BackendError;
  path: string;
  /** `'failed'` keeps the error text. */
  probe: FolderProbe | 'failed';
}

export function StartScreen() {
  const t = useT();
  const locale = useContext(LocaleContext);
  const n = useMemo(() => new Intl.NumberFormat(locale), [locale]);
  const status = useLibraryStore((s) => s.status);
  const recents = useLibraryStore((s) => s.recents);
  const progress = useLibraryStore((s) => s.progress);
  const error = useLibraryStore((s) => s.error);
  const openingPath = useLibraryStore((s) => s.openingPath);
  const lastPath = useLibraryStore((s) => s.lastPath);
  const settings = useSettingsStore((s) => s.settings);
  const loadRecents = useLibraryStore((s) => s.loadRecents);
  const open = useLibraryStore((s) => s.open);
  const forget = useLibraryStore((s) => s.forget);
  const fail = useLibraryStore((s) => s.fail);
  const opening = status === 'opening';
  const [volumes, setVolumes] = useState<Volume[]>([]);
  const [probed, setProbed] = useState<ProbeState | null>(null);
  const noProfile =
    status === 'error' && error?.code === 'no-profile' ? error : null;
  const probePath = noProfile ? (lastPath ?? noProfile.detail ?? null) : null;

  // A folder with no known layout may still be a pile of ROMs: probe it
  // once per error (names only) and offer to organise it. Until the answer
  // for this very error arrives, neither the offer nor the banner shows.
  useEffect(() => {
    if (!noProfile || !probePath) return;
    let alive = true;
    void getBackend()
      .then((b) => b.probeFolder(probePath))
      .then(
        (probe) => {
          if (alive) setProbed({ error: noProfile, path: probePath, probe });
        },
        () => {
          if (alive)
            setProbed({ error: noProfile, path: probePath, probe: 'failed' });
        },
      );
    return () => {
      alive = false;
    };
  }, [noProfile, probePath]);

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

  const current = probed && probed.error === error ? probed : null;
  const probing = noProfile !== null && probePath !== null && !current;
  const offer =
    current &&
    current.probe !== 'failed' &&
    (current.probe.rom_like > 0 || current.probe.sorted_folders.length > 0)
      ? { path: current.path, probe: current.probe }
      : null;
  const defaultLayout =
    settings?.organize?.default_layout ||
    settings?.library?.default_profile ||
    DEFAULT_LAYOUT;

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
        {offer && (
          <OrganizeOffer
            probe={offer.probe}
            defaultLayout={defaultLayout}
            onOrganize={() => useNavStore.getState().openOrganize(offer.path)}
            onOpenSorted={(layout) => void open(offer.path, layout)}
            onPickOther={() => void pick()}
          />
        )}
        {status === 'error' && error && !offer && !probing && (
          <p role="alert" className="error-banner">
            {t(errorKey(error.code), { detail: error.detail ?? '' })}
          </p>
        )}
        <section
          aria-labelledby="volumes-heading"
          className="recent w-full max-w-2xl text-left"
        >
          <h2 id="volumes-heading" className="section-title">
            {t('start.volumes')}
          </h2>
          {volumes.length === 0 ? (
            <p className="hint">{t('start.noVolumes')}</p>
          ) : (
            <ul className="recent-list">
              {volumes.map((v) => (
                <li key={v.path} className="recent-item">
                  <button
                    type="button"
                    className="recent-open"
                    aria-label={t('start.openVolume', { name: v.name })}
                    disabled={opening}
                    onClick={() => void open(v.path)}
                  >
                    <span className="recent-name truncate" title={v.name}>
                      {v.name}
                    </span>
                    <span className="recent-path truncate" title={v.path}>
                      {v.path}
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </section>
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
