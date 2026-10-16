import { useCallback, useContext } from 'react';

import { LocaleContext, translate, type TranslateVars } from './core';

export type TFunction = (key: string, vars?: TranslateVars) => string;

/** Returns `t(key, vars?)` bound to the active locale. */
export function useT(): TFunction {
  const locale = useContext(LocaleContext);
  return useCallback(
    (key: string, vars?: TranslateVars) => translate(locale, key, vars),
    [locale],
  );
}
