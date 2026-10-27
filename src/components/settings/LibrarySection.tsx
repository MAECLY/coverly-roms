import { useEffect, useState } from 'react';

import { Button } from '@/components/ui/button';
import { EmptyState } from '@/components/ui/empty-state';
import { NativeSelect } from '@/components/ui/select';
import { useT } from '@/i18n/useT';
import { getBackend } from '@/ipc/backend';
import type {
  FoundTool,
  OrganizeModeSetting,
  OrganizeNameMode,
  ProfileInfo,
} from '@/ipc/types';
import { toolName } from '@/lib/organize';
import { useLibraryStore } from '@/stores/libraryStore';
import { useSettingsStore } from '@/stores/settingsStore';

import { DedupeSettings } from './DedupeSettings';
import {
  Field,
  hintText,
  sectionTitle,
  settingId,
  SwitchSetting,
  useFieldError,
} from './Field';
import { ListSetting } from './ListSetting';

/** What was found, each tool as a mono readout (brand §5) inside the
 * translated line; or how to get one. */
function ExternalToolsHint({ found }: { found: FoundTool[] }) {
  const t = useT();
  if (found.length === 0)
    return <>{t('settings.organize.externalToolsNone')}</>;
  const MARK = '\u0000';
  const [before = '', after = ''] = t('settings.organize.externalToolsFound', {
    tools: MARK,
  }).split(MARK);
  const list = found
    .map((f) => `${toolName(f.tool)} ${f.version} (${f.path})`)
    .join(', ');
  return (
    <>
      {before}
      <span className="font-mono [overflow-wrap:anywhere]">{list}</span>
      {after}
    </>
  );
}

/** Defaults of the organise screen (organise spec §12); thresholds and
 * lists stay in the settings file. */
function OrganizeSettings({ profiles }: { profiles: ProfileInfo[] }) {
  const t = useT();
  const org = useSettingsStore((s) => s.settings!.organize);
  const update = useSettingsStore((s) => s.update);
  const found = useSettingsStore((s) => s.externalToolsFound);
  const listed = profiles.some((p) => p.profile.id === org.default_layout);
  return (
    <fieldset className="m-0 grid min-w-0 gap-4 border-0 p-0">
      <legend className={`${sectionTitle} mb-4 p-0`}>
        {t('settings.organize.title')}
      </legend>
      <Field
        id={settingId('organize.default_layout')}
        label={t('settings.organize.defaultLayout')}
        error={useFieldError('organize.default_layout')}
      >
        <NativeSelect
          name="organize-default-layout"
          className="max-w-xs"
          value={org.default_layout}
          onChange={(e) =>
            void update({ organize: { default_layout: e.target.value } })
          }
        >
          {!listed && (
            <option value={org.default_layout}>{org.default_layout}</option>
          )}
          {profiles.map((p) => (
            <option key={p.profile.id} value={p.profile.id}>
              {p.profile.name}
            </option>
          ))}
        </NativeSelect>
      </Field>
      <SwitchSetting
        id={settingId('organize.nest_in_roms_root')}
        name="organize-nest-in-roms-root"
        label={t('settings.organize.nestInRoms')}
        checked={org.nest_in_roms_root}
        onCheckedChange={(on) =>
          void update({ organize: { nest_in_roms_root: on } })
        }
      />
      <Field
        id={settingId('organize.mode')}
        label={t('settings.organize.mode')}
        error={useFieldError('organize.mode')}
      >
        <NativeSelect
          name="organize-mode"
          className="max-w-xs"
          value={org.mode}
          onChange={(e) =>
            void update({
              organize: { mode: e.target.value as OrganizeModeSetting },
            })
          }
        >
          <option value="auto">{t('settings.organize.modeAuto')}</option>
          <option value="move">{t('settings.organize.modeMove')}</option>
          <option value="copy">{t('settings.organize.modeCopy')}</option>
        </NativeSelect>
      </Field>
      <Field
        id={settingId('organize.names')}
        label={t('settings.organize.names')}
        error={useFieldError('organize.names')}
      >
        <NativeSelect
          name="organize-names"
          className="max-w-xs"
          value={org.names}
          onChange={(e) =>
            void update({
              organize: { names: e.target.value as OrganizeNameMode },
            })
          }
        >
          <option value="keep">{t('settings.organize.namesKeep')}</option>
          <option value="catalogue">
            {t('settings.organize.namesCatalogue')}
          </option>
        </NativeSelect>
      </Field>
      <SwitchSetting
        id={settingId('organize.download_dats')}
        name="organize-download-dats"
        label={t('settings.organize.downloadDats')}
        checked={org.download_dats}
        onCheckedChange={(on) =>
          void update({ organize: { download_dats: on } })
        }
      />
      <SwitchSetting
        id={settingId('organize.external_tools')}
        name="organize-external-tools"
        label={t('settings.organize.externalTools')}
        hint={<ExternalToolsHint found={found} />}
        checked={org.external_tools === 'auto'}
        onCheckedChange={(on) =>
          void update({ organize: { external_tools: on ? 'auto' : 'off' } })
        }
      />
      <p className={hintText}>{t('settings.organize.advancedHint')}</p>
    </fieldset>
  );
}

export function LibrarySection() {
  const t = useT();
  const lib = useSettingsStore((s) => s.settings!.library);
  const update = useSettingsStore((s) => s.update);
  const recents = useLibraryStore((s) => s.recents);
  const forget = useLibraryStore((s) => s.forget);
  const [profiles, setProfiles] = useState<ProfileInfo[]>([]);
  useEffect(() => {
    let alive = true;
    void getBackend()
      .then((b) => b.listProfiles())
      .then(
        (p) => alive && setProfiles(p),
        () => undefined,
      );
    void useLibraryStore.getState().loadRecents();
    return () => {
      alive = false;
    };
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
        value={lib.ignored_exts}
        split={commas}
        join={(l) => l.join(', ')}
        toPatch={(l) => ({ library: { ignored_exts: l } })}
      />
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
        checked={lib.follow_symlinks}
        onCheckedChange={(on) =>
          void update({ library: { follow_symlinks: on } })
        }
      />
      <OrganizeSettings profiles={profiles} />
      <DedupeSettings profiles={profiles} />
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
