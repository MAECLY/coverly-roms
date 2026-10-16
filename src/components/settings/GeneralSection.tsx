import { useT } from '@/i18n/useT';
import type { LanguageChoice, ThemeChoice, UpdateChannel } from '@/ipc/types';
import { useSettingsStore } from '@/stores/settingsStore';

import { UpdatePanel } from '../update/UpdatePanel';
import { Field, settingId } from './Field';

const THEMES: readonly ThemeChoice[] = ['system', 'light', 'dark'];

export function GeneralSection() {
  const t = useT();
  const g = useSettingsStore((s) => s.settings!.general);
  const update = useSettingsStore((s) => s.update);
  return (
    <div className="settings-group">
      <Field
        id={settingId('general.language')}
        label={t('settings.general.language')}
      >
        <select
          id={settingId('general.language')}
          name="language"
          className="select"
          value={g.language}
          onChange={(e) =>
            void update({
              general: { language: e.target.value as LanguageChoice },
            })
          }
        >
          {(['auto', 'en', 'es'] as const).map((l) => (
            <option key={l} value={l}>
              {t(`language.${l}`)}
            </option>
          ))}
        </select>
      </Field>
      <fieldset className="setting">
        <legend className="setting-label">{t('settings.general.theme')}</legend>
        {THEMES.map((th) => (
          <label key={th} className="radio-row">
            <input
              type="radio"
              name="theme"
              checked={g.theme === th}
              onChange={() => void update({ general: { theme: th } })}
            />
            {t(`settings.general.themes.${th}`)}
          </label>
        ))}
      </fieldset>
      <label className="checkbox-row">
        <input
          type="checkbox"
          name="check-updates"
          checked={g.check_updates_on_launch}
          onChange={(e) =>
            void update({
              general: { check_updates_on_launch: e.target.checked },
            })
          }
        />
        {t('settings.general.checkUpdates')}
      </label>
      <Field
        id={settingId('general.update_channel')}
        label={t('settings.general.channel')}
        hint={t('update.channelHint')}
      >
        <select
          id={settingId('general.update_channel')}
          name="update-channel"
          className="select"
          value={g.update_channel}
          onChange={(e) =>
            void update({
              general: { update_channel: e.target.value as UpdateChannel },
            })
          }
        >
          {(['stable', 'beta'] as const).map((c) => (
            <option key={c} value={c}>
              {t(`settings.general.channels.${c}`)}
            </option>
          ))}
        </select>
      </Field>
      <UpdatePanel compact />
    </div>
  );
}
