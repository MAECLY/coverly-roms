import { useEffect, useState } from 'react';

import { useT } from '@/i18n/useT';
import { getBackend } from '@/ipc/backend';
import type { ProfileInfo } from '@/ipc/types';
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
