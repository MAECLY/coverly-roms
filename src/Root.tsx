import App from './App';
import { I18nProvider } from './i18n/I18nProvider';
import { useLocaleStore } from './stores/localeStore';

/** The app inside the i18n provider, following the language switch. */
export function Root() {
  const choice = useLocaleStore((s) => s.choice);
  return (
    <I18nProvider {...(choice === 'auto' ? {} : { locale: choice })}>
      <App />
    </I18nProvider>
  );
}
