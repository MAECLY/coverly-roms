import { FolderOpen, FolderSearch, FolderTree } from 'lucide-react';
import { useContext, useEffect, useState } from 'react';

import { Button } from '@/components/ui/button';
import { Field, FieldControl, FieldLabel } from '@/components/ui/field';
import { NativeSelect } from '@/components/ui/select';
import { SlotMark } from '@/components/ui/slot-mark';
import { LocaleContext } from '@/i18n/core';
import { useT } from '@/i18n/useT';
import { getBackend } from '@/ipc/backend';
import type { FolderProbe, ProfileInfo } from '@/ipc/types';

import { hintText, panel, sectionTitle } from './styles';

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
  // `truncated` means the probe stopped after PROBE_LIMIT entries of any
  // kind, so the ROMs it saw are a lower bound, not "5 000+ ROMs".
  const found = new Intl.NumberFormat(locale).format(probe.rom_like);
  const count = probe.truncated ? `${found}+` : found;
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
      className={`${panel} grid w-full max-w-2xl gap-3 p-4 text-left`}
    >
      <div className="flex items-start gap-3">
        {/* The folder has no shelf yet: a missing slot (brand §2). */}
        <SlotMark state="missing" size={32} className="mt-0.5" />
        <div className="grid min-w-0 gap-1">
          <h2 id="organize-offer-title" className={sectionTitle}>
            {t('start.organizeOffer.title')}
          </h2>
          {sortedOnly ? (
            <p className={hintText}>{t('start.suggestSystemFolders')}</p>
          ) : (
            <p className="m-0 text-ui text-cr-text">
              {t('start.organizeOffer.body', { roms: count })}
            </p>
          )}
          {probe.broad_root && (
            <p className={hintText}>{t('start.organizeOffer.broadRoot')}</p>
          )}
          {probe.truncated && !sortedOnly && (
            <p className={hintText}>
              {t('start.organizeOffer.truncated', { count: limit })}
            </p>
          )}
        </div>
      </div>
      <div className="flex flex-wrap items-end gap-2">
        {sortedOnly ? (
          <>
            <Field className="w-56">
              <FieldLabel>{t('organize.layout')}</FieldLabel>
              <FieldControl>
                <NativeSelect
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
                </NativeSelect>
              </FieldControl>
            </Field>
            <Button variant="primary" onClick={() => onOpenSorted(layout)}>
              <FolderOpen aria-hidden />
              {t('start.organizeOffer.openSorted')}
            </Button>
          </>
        ) : (
          <Button variant="primary" onClick={onOrganize}>
            <FolderTree aria-hidden />
            {t('start.organizeOffer.action')}
          </Button>
        )}
        <Button variant="outline" onClick={onPickOther}>
          <FolderSearch aria-hidden />
          {t('start.organizeOffer.other')}
        </Button>
      </div>
    </section>
  );
}
