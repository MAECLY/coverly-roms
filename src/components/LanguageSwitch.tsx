import { useT } from '@/i18n/useT';
import { useLocaleStore, type LocaleChoice } from '@/stores/localeStore';

const CHOICES: readonly LocaleChoice[] = ['auto', 'en', 'es'];

export function LanguageSwitch() {
  const t = useT();
  const choice = useLocaleStore((s) => s.choice);
  const setChoice = useLocaleStore((s) => s.setChoice);
  return (
    <select
      aria-label={t('language.label')}
      className="select"
      value={choice}
      onChange={(e) => setChoice(e.target.value as LocaleChoice)}
    >
      {CHOICES.map((c) => (
        <option key={c} value={c}>
          {t(`language.${c}`)}
        </option>
      ))}
    </select>
  );
}
