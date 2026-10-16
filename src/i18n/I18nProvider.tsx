import { useEffect, useMemo, type ReactNode } from 'react';

import { detectLocale, LocaleContext, type Locale } from './core';

interface I18nProviderProps {
  /** Forces a locale; when omitted it is detected from `navigator.language`. */
  locale?: Locale;
  children: ReactNode;
}

export function I18nProvider({ locale, children }: I18nProviderProps) {
  const active = useMemo(() => locale ?? detectLocale(), [locale]);
  // Assistive tech and the web view's spell/hyphenation follow <html lang>.
  useEffect(() => {
    document.documentElement.lang = active;
  }, [active]);
  return (
    <LocaleContext.Provider value={active}>{children}</LocaleContext.Provider>
  );
}
