/**
 * i18n core: catalogs, locale matching and the pure `translate` function.
 *
 * UI strings live only in `locales/{en,es}/ui.json`. A key missing from the
 * active catalog falls back to English, then to the key itself, so a missing
 * translation is visible but never crashes a screen.
 */
import { createContext } from 'react';

import en from '../../locales/en/ui.json';
import es from '../../locales/es/ui.json';

export type Locale = 'en' | 'es';
export type TranslateVars = Record<string, string | number>;

export const DEFAULT_LOCALE: Locale = 'en';

type Catalog = { [key: string]: string | Catalog };

const CATALOGS: Record<Locale, Catalog> = { en, es };

/** `es*` (any region, any case) → Spanish; everything else → English. */
export function pickLocale(language: string | undefined | null): Locale {
  return language?.toLowerCase().startsWith('es') ? 'es' : DEFAULT_LOCALE;
}

/** The OS/webview UI language narrowed to a supported locale. */
export function detectLocale(): Locale {
  if (typeof navigator === 'undefined') return DEFAULT_LOCALE;
  return pickLocale(navigator.language);
}

function lookup(catalog: Catalog, key: string): string | undefined {
  let node: string | Catalog | undefined = catalog;
  for (const part of key.split('.')) {
    if (node === undefined || typeof node === 'string') return undefined;
    node = node[part];
  }
  return typeof node === 'string' ? node : undefined;
}

function interpolate(template: string, vars?: TranslateVars): string {
  if (!vars) return template;
  return template.replace(/\{(\w+)\}/g, (match, name: string) =>
    Object.prototype.hasOwnProperty.call(vars, name)
      ? String(vars[name])
      : match,
  );
}

export function translate(
  locale: Locale,
  key: string,
  vars?: TranslateVars,
): string {
  const template =
    lookup(CATALOGS[locale], key) ?? lookup(CATALOGS[DEFAULT_LOCALE], key);
  return template === undefined ? key : interpolate(template, vars);
}

export const LocaleContext = createContext<Locale>(DEFAULT_LOCALE);
