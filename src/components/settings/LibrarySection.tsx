import { useEffect, useState } from 'react';

import { useT } from '@/i18n/useT';
import { getBackend } from '@/ipc/backend';
import type {
  OrganizeModeSetting,
  OrganizeNameMode,
  ProfileInfo,
} from '@/ipc/types';
import { useLibraryStore } from '@/stores/libraryStore';
import { useSettingsStore } from '@/stores/settingsStore';

import { Field, settingId, useFieldError } from './Field';

/** A list edited as text, committed on blur. */
function ListSetting({
  path,
  label,
  hint,
  value,
  split,
  join,
  toPatch,
}: {
  path: string;
  label: string;
  hint: string;
  value: string[];
  split(text: string): string[];
  join(list: string[]): string;
  toPatch(
    list: string[],
  ): Parameters<ReturnType<typeof useSettingsStore.getState>['update']>[0];
}) {
  const id = settingId(path);
  const update = useSettingsStore((s) => s.update);
  const error = useFieldError(path);
  const [draft, setDraft] = useState<string | null>(null);
  return (
    <Field id={id} label={label} hint={hint} error={error}>
      <textarea
        id={id}
        name={id}
        className="text-input"
        rows={4}
        value={draft ?? join(value)}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={async () => {
          if (draft === null) return;
          const errors = await update(toPatch(split(draft)));
          if (errors.length === 0) setDraft(null);
        }}
      />
    </Field>
  );
}

/** Defaults of the organise screen (organise spec §12); thresholds and
 * lists stay in the settings file. */
function OrganizeSettings({ profiles }: { profiles: ProfileInfo[] }) {
  const t = useT();
  const org = useSettingsStore((s) => s.settings!.organize);
  const update = useSettingsStore((s) => s.update);
  const layoutId = settingId('organize.default_layout');
  const modeId = settingId('organize.mode');
  const namesId = settingId('organize.names');
  const listed = profiles.some((p) => p.profile.id === org.default_layout);
  return (
    <fieldset className="setting">
      <legend className="setting-label">{t('settings.organize.title')}</legend>
      <Field
        id={layoutId}
        label={t('settings.organize.defaultLayout')}
        error={useFieldError('organize.default_layout')}
      >
        <select
          id={layoutId}
          name="organize-default-layout"
          className="select"
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
        </select>
      </Field>
      <label className="checkbox-row">
        <input
          type="checkbox"
          name="organize-nest-in-roms-root"
          checked={org.nest_in_roms_root}
          onChange={(e) =>
            void update({ organize: { nest_in_roms_root: e.target.checked } })
          }
        />
        {t('settings.organize.nestInRoms')}
      </label>
      <Field
        id={modeId}
        label={t('settings.organize.mode')}
        error={useFieldError('organize.mode')}
      >
        <select
          id={modeId}
          name="organize-mode"
          className="select"
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
        </select>
      </Field>
      <Field
        id={namesId}
        label={t('settings.organize.names')}
        error={useFieldError('organize.names')}
      >
        <select
          id={namesId}
          name="organize-names"
          className="select"
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
        </select>
      </Field>
      <label className="checkbox-row">
        <input
          type="checkbox"
          name="organize-download-dats"
          checked={org.download_dats}
          onChange={(e) =>
            void update({ organize: { download_dats: e.target.checked } })
          }
        />
        {t('settings.organize.downloadDats')}
      </label>
      <p className="hint">{t('settings.organize.advancedHint')}</p>
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
        <select
          id={settingId('library.default_profile')}
          name="default-profile"
          className="select"
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
        </select>
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
      <label className="checkbox-row">
        <input
          type="checkbox"
          name="include-hidden"
          checked={lib.include_hidden}
          onChange={(e) =>
            void update({ library: { include_hidden: e.target.checked } })
          }
        />
        {t('settings.library.includeHidden')}
      </label>
      <label className="checkbox-row">
        <input
          type="checkbox"
          name="follow-symlinks"
          checked={lib.follow_symlinks}
          onChange={(e) =>
            void update({ library: { follow_symlinks: e.target.checked } })
          }
        />
        {t('settings.library.followSymlinks')}
      </label>
      <p className="hint">{t('settings.library.followSymlinksHint')}</p>
      <OrganizeSettings profiles={profiles} />
      <h2 className="section-title">{t('settings.library.recent')}</h2>
      {recents.length === 0 ? (
        <p className="hint">{t('settings.library.recentEmpty')}</p>
      ) : (
        <ul className="recent-list">
          {recents.map((r) => (
            <li key={r.path}>
              <span className="truncate" title={r.path}>
                {r.name}
              </span>
              <button
                type="button"
                className="ghost-button"
                onClick={() => void forget(r.path)}
              >
                {t('settings.library.forget')}
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
