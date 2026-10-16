import { useT } from '@/i18n/useT';
import type { LanguageChoice } from '@/ipc/types';
import { useSettingsStore } from '@/stores/settingsStore';

const CHOICES: readonly LanguageChoice[] = ['auto', 'en', 'es'];

/** The language setting (General → Language), kept in the header. */
export function LanguageSwitch() {
  const t = useT();
  const choice = useSettingsStore(
    (s) => s.settings?.general.language ?? 'auto',
  );
  const ready = useSettingsStore((s) => s.status === 'ready');
  return (
    <select
      name="language"
      aria-label={t('language.label')}
      className="select"
      value={choice}
      disabled={!ready}
      onChange={(e) =>
        void useSettingsStore
          .getState()
          .update({ general: { language: e.target.value as LanguageChoice } })
      }
    >
      {CHOICES.map((c) => (
        <option key={c} value={c}>
          {t(`language.${c}`)}
        </option>
      ))}
    </select>
  );
}
