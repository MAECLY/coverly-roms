import { NativeSelect } from '@/components/ui/select';
import {
  SegmentedControl,
  SegmentedControlItem,
} from '@/components/ui/toggle-group';
import { useT } from '@/i18n/useT';
import type { LanguageChoice, ThemeChoice, UpdateChannel } from '@/ipc/types';
import { useSettingsStore } from '@/stores/settingsStore';

import { UpdatePanel } from '../update/UpdatePanel';
import { Field, settingId, SwitchSetting } from './Field';

const THEMES: readonly ThemeChoice[] = ['system', 'light', 'dark'];

export function GeneralSection() {
  const t = useT();
  const g = useSettingsStore((s) => s.settings!.general);
  const update = useSettingsStore((s) => s.update);
  const themeLabel = settingId('general.theme');
  return (
    <div className="settings-group">
      <Field
        id={settingId('general.language')}
        label={t('settings.general.language')}
      >
        <NativeSelect
          name="language"
          className="max-w-xs"
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
        </NativeSelect>
      </Field>
      <div className="grid justify-items-start gap-1.5">
        <span
          id={themeLabel}
          className="select-none text-ui font-medium text-cr-text"
        >
          {t('settings.general.theme')}
        </span>
        <SegmentedControl
          aria-labelledby={themeLabel}
          value={g.theme}
          onValueChange={(th) =>
            void update({ general: { theme: th as ThemeChoice } })
          }
        >
          {THEMES.map((th) => (
            <SegmentedControlItem key={th} value={th}>
              {t(`settings.general.themes.${th}`)}
            </SegmentedControlItem>
          ))}
        </SegmentedControl>
      </div>
      <SwitchSetting
        id={settingId('general.check_updates_on_launch')}
        name="check-updates"
        label={t('settings.general.checkUpdates')}
        checked={g.check_updates_on_launch}
        onCheckedChange={(on) =>
          void update({ general: { check_updates_on_launch: on } })
        }
      />
      <Field
        id={settingId('general.update_channel')}
        label={t('settings.general.channel')}
        hint={t('update.channelHint')}
      >
        <NativeSelect
          name="update-channel"
          className="max-w-xs"
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
        </NativeSelect>
      </Field>
      <UpdatePanel compact />
    </div>
  );
}
