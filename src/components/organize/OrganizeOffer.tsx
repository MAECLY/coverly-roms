import { useContext, useEffect, useState } from 'react';

import { LocaleContext } from '@/i18n/core';
import { useT } from '@/i18n/useT';
import { getBackend } from '@/ipc/backend';
import type { FolderProbe, ProfileInfo } from '@/ipc/types';

/** Where `probe_folder` stops counting (organise spec §4.1). */
export const PROBE_LIMIT = 5000;

export const DEFAULT_LAYOUT = 'anbernic-stock';

interface Props {
  probe: FolderProbe;
  /** Layout preselected for "Open as organised by system". */
  defaultLayout?: string | undefined;
  onOrganize(): void;
  onOpenSorted(layout: string): void;
  onPickOther(): void;
}

/**
 * Shown on the Start screen instead of the `no-profile` dead end when the
 * folder holds ROM-like files or system folders (organise spec §4.1).
 */
export function OrganizeOffer({
  probe,
  defaultLayout,
  onOrganize,
  onOpenSorted,
  onPickOther,
}: Props) {
  const t = useT();
  const locale = useContext(LocaleContext);
  const limit = new Intl.NumberFormat(locale).format(PROBE_LIMIT);
  const sortedOnly = probe.rom_like === 0 && probe.sorted_folders.length > 0;
  const count = probe.truncated
    ? `${limit}+`
    : new Intl.NumberFormat(locale).format(probe.rom_like);
  const [layout, setLayout] = useState(defaultLayout ?? DEFAULT_LAYOUT);
  const [profiles, setProfiles] = useState<ProfileInfo[]>([]);

  useEffect(() => {
    if (!sortedOnly) return;
    let alive = true;
    void getBackend()
      .then((b) => b.listProfiles())
      .then(
        (p) => alive && setProfiles(Array.isArray(p) ? p : []),
        () => undefined,
      );
    return () => {
      alive = false;
    };
  }, [sortedOnly]);

  // Until the list loads (or for an id it lacks) the chosen id still shows.
  const unlisted = profiles.some((p) => p.profile.id === layout)
    ? null
    : layout;

  return (
    <section
      aria-labelledby="organize-offer-title"
      className="recent w-full max-w-2xl text-left"
    >
      <h2 id="organize-offer-title" className="section-title">
        {t('start.organizeOffer.title')}
      </h2>
      {sortedOnly ? (
        <p className="hint">{t('start.suggestSystemFolders')}</p>
      ) : (
        <p>{t('start.organizeOffer.body', { roms: count })}</p>
      )}
      {probe.broad_root && (
        <p className="hint">{t('start.organizeOffer.broadRoot')}</p>
      )}
      {probe.truncated && (
        <p className="hint">
          {t('start.organizeOffer.truncated', { count: limit })}
        </p>
      )}
      <div className="flex flex-wrap items-center gap-3">
        {sortedOnly ? (
          <>
            <label>
              {t('organize.layout')}{' '}
              <select
                className="select"
                value={layout}
                onChange={(e) => setLayout(e.target.value)}
              >
                {unlisted !== null && (
                  <option value={unlisted}>{unlisted}</option>
                )}
                {profiles.map((p) => (
                  <option key={p.profile.id} value={p.profile.id}>
                    {p.profile.name}
                  </option>
                ))}
              </select>
            </label>
            <button
              type="button"
              className="primary-button"
              onClick={() => onOpenSorted(layout)}
            >
              {t('start.organizeOffer.openSorted')}
            </button>
          </>
        ) : (
          <button type="button" className="primary-button" onClick={onOrganize}>
            {t('start.organizeOffer.action')}
          </button>
        )}
        <button type="button" className="ghost-button" onClick={onPickOther}>
          {t('start.organizeOffer.other')}
        </button>
      </div>
    </section>
  );
}
