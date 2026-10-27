import { useEffect } from 'react';

import { Button } from '@/components/ui/button';
import { EmptyState } from '@/components/ui/empty-state';
import { NativeSelect } from '@/components/ui/select';
import { useT } from '@/i18n/useT';
import { useLibraryStore } from '@/stores/libraryStore';
import { useSettingsStore } from '@/stores/settingsStore';

import {
  AdvancedFold,
  Field,
  sectionTitle,
  settingId,
  SwitchSetting,
  useFieldError,
} from './Field';
import { ListSetting } from './ListSetting';
import { useProfiles } from './useProfiles';

export function LibrarySection() {
  const t = useT();
  const lib = useSettingsStore((s) => s.settings!.library);
  const update = useSettingsStore((s) => s.update);
  const recents = useLibraryStore((s) => s.recents);
  const forget = useLibraryStore((s) => s.forget);
  const profiles = useProfiles();
  useEffect(() => {
    void useLibraryStore.getState().loadRecents();
  }, []);
  // Until the list loads, or for a profile it no longer has, the stored id
  // still shows (not "Detect automatically").
  const current = lib.default_profile;
  const unlisted =
    current !== null && !profiles.some((p) => p.profile.id === current)
      ? current
      : null;
  const lines = (s: string) =>
    s
      .split('\n')
      .map((x) => x.trim())
      .filter(Boolean);
  const commas = (s: string) =>
    s
      .split(',')
      .map((x) => x.trim())
      .filter(Boolean);
  return (
    <div className="settings-group">
      <Field
        id={settingId('library.default_profile')}
        label={t('settings.library.defaultProfile')}
        error={useFieldError('library.default_profile')}
      >
        <NativeSelect
          name="default-profile"
          className="max-w-xs"
          value={lib.default_profile ?? ''}
          onChange={(e) =>
            void update({
              library: { default_profile: e.target.value || null },
            })
          }
        >
          <option value="">{t('settings.library.defaultProfileAuto')}</option>
          {unlisted !== null && <option value={unlisted}>{unlisted}</option>}
          {profiles.map((p) => (
            <option key={p.profile.id} value={p.profile.id}>
              {p.profile.name}
            </option>
          ))}
        </NativeSelect>
      </Field>
      <SwitchSetting
        id={settingId('library.include_hidden')}
        name="include-hidden"
        label={t('settings.library.includeHidden')}
        checked={lib.include_hidden}
        onCheckedChange={(on) =>
          void update({ library: { include_hidden: on } })
        }
      />
      <SwitchSetting
        id={settingId('library.follow_symlinks')}
        name="follow-symlinks"
        label={t('settings.library.followSymlinks')}
        hint={t('settings.library.followSymlinksHint')}
        more={t('settings.library.followSymlinksDetails')}
        checked={lib.follow_symlinks}
        onCheckedChange={(on) =>
          void update({ library: { follow_symlinks: on } })
        }
      />
      <AdvancedFold
        id="library-advanced"
        count={2}
        errorPaths={['library.ignored_dirs', 'library.ignored_exts']}
      >
        <ListSetting
          path="library.ignored_dirs"
          label={t('settings.library.ignoredDirs')}
          hint={t('settings.library.ignoredDirsHint')}
          value={lib.ignored_dirs}
          split={lines}
          join={(l) => l.join('\n')}
          toPatch={(l) => ({ library: { ignored_dirs: l } })}
        />
        <ListSetting
          path="library.ignored_exts"
          label={t('settings.library.ignoredExts')}
          hint={t('settings.library.ignoredExtsHint')}
          more={t('settings.library.ignoredExtsDetails')}
          value={lib.ignored_exts}
          split={commas}
          join={(l) => l.join(', ')}
          toPatch={(l) => ({ library: { ignored_exts: l } })}
        />
      </AdvancedFold>
      <h2 className={sectionTitle}>{t('settings.library.recent')}</h2>
      {recents.length === 0 ? (
        <EmptyState
          size="sm"
          titleAs="p"
          title={t('settings.library.recentEmpty')}
          className="items-start px-0 py-2 text-left"
        />
      ) : (
        <ul className="recent-list">
          {recents.map((r) => (
            <li key={r.path} className="flex items-center gap-3">
              {/* The name in the UI face, the path as the mono readout
                  (brand §5), the same pair as the start screen rows. */}
              <span className="grid min-w-0 flex-1">
                <span className="truncate font-medium" title={r.name}>
                  {r.name}
                </span>
                <span
                  className="truncate font-mono text-caption text-cr-text-secondary"
                  title={r.path}
                >
                  {r.path}
                </span>
              </span>
              <Button
                size="sm"
                aria-label={t('start.forget', { name: r.name })}
                onClick={() => void forget(r.path)}
              >
                {t('settings.library.forget')}
              </Button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
