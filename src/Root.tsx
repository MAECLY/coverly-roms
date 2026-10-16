import { useEffect } from 'react';

import App from './App';
import { ErrorBoundary } from './components/ErrorBoundary';
import { I18nProvider } from './i18n/I18nProvider';
import { applyTheme } from './lib/theme';
import { useSettingsStore } from './stores/settingsStore';
import { useUpdateStore } from './stores/updateStore';

/** The app inside the i18n provider, following the language and theme settings. */
export function Root() {
  const language = useSettingsStore(
    (s) => s.settings?.general.language ?? 'auto',
  );
  const theme = useSettingsStore((s) => s.settings?.general.theme ?? 'system');
  useEffect(() => {
    if (useSettingsStore.getState().status === 'idle')
      void useSettingsStore.getState().load();
  }, []);
  const settingsReady = useSettingsStore((s) => s.status === 'ready');
  const checkOnStart = useSettingsStore(
    (s) => s.settings?.general.check_updates_on_launch ?? false,
  );
  useEffect(() => {
    if (settingsReady)
      void useUpdateStore.getState().startupCheck(checkOnStart);
    // eslint-disable-next-line react-hooks/exhaustive-deps -- once, with the setting as loaded
  }, [settingsReady]);
  useEffect(() => applyTheme(theme), [theme]);
  return (
    <I18nProvider {...(language === 'auto' ? {} : { locale: language })}>
      <ErrorBoundary>
        <App />
      </ErrorBoundary>
    </I18nProvider>
  );
}
