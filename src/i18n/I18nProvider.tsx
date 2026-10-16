import { useMemo, type ReactNode } from 'react';

import { detectLocale, LocaleContext, type Locale } from './core';

interface I18nProviderProps {
  /** Forces a locale; when omitted it is detected from `navigator.language`. */
  locale?: Locale;
  children: ReactNode;
}

export function I18nProvider({ locale, children }: I18nProviderProps) {
  const active = useMemo(() => locale ?? detectLocale(), [locale]);
  return (
    <LocaleContext.Provider value={active}>{children}</LocaleContext.Provider>
  );
}
