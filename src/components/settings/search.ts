import { translate, type Locale } from '@/i18n/core';

import { SETTINGS_INDEX, type SettingEntry } from './registry';

const LOCALES: readonly Locale[] = ['es', 'en'];

/** Accent- and case-insensitive text, placeholders removed (D8). */
export function foldText(s: string): string {
  return s
    .replace(/\{\w+\}/g, ' ')
    .normalize('NFD')
    .replace(/\p{M}+/gu, '')
    .toLowerCase()
    .replace(/\s+/g, ' ')
    .trim();
}

interface Folded {
  label: string;
  hint: string;
  keywords: string[];
}

const cache = new Map<Locale, Map<SettingEntry, Folded>>();

/** The entry's texts in `locale`, folded once and memoised. */
function folded(locale: Locale, e: SettingEntry): Folded {
  let byEntry = cache.get(locale);
  if (!byEntry) {
    byEntry = new Map();
    cache.set(locale, byEntry);
  }
  let f = byEntry.get(e);
  if (!f) {
    f = {
      label: foldText(translate(locale, e.label)),
      hint: e.hint ? foldText(translate(locale, e.hint)) : '',
      keywords: e.keywords
        ? translate(locale, e.keywords)
            .split(',')
            .map(foldText)
            .filter((k) => k !== '')
        : [],
    };
    byEntry.set(e, f);
  }
  return f;
}

/** The word-start positions of `text`: «borrar» does not match «rar». */
const wordsOf = (text: string) => text.split(/[^\p{L}\p{N}]+/u);

const allStart = (words: string[], of: string[]) =>
  words.every((w) => of.some((o) => o.startsWith(w)));

function score(f: Folded, q: string, words: string[]): number {
  if (f.label.startsWith(q)) return 40;
  if (allStart(words, wordsOf(f.label))) return 30;
  if (allStart(words, wordsOf(f.hint))) return 20;
  if (allStart(words, f.keywords.flatMap(wordsOf))) return 10;
  return 0;
}

/** Settings that match `query`, best first; other locales count a bit less. */
export function searchSettings(
  query: string,
  locale: Locale,
  index: readonly SettingEntry[] = SETTINGS_INDEX,
): SettingEntry[] {
  const q = foldText(query);
  // Split like the texts, so «7-zip» looks for «7» and «zip».
  const words = wordsOf(q).filter((w) => w !== '');
  if (words.length === 0) return [];
  const order = [locale, ...LOCALES.filter((l) => l !== locale)];
  return index
    .map((e, i) => {
      const best = Math.max(
        ...order.map((l, k) => {
          const s = score(folded(l, e), q, words);
          return s === 0 ? 0 : s - (k === 0 ? 0 : 5);
        }),
      );
      return { e, i, best };
    })
    .filter((x) => x.best > 0)
    .sort((a, b) => b.best - a.best || a.i - b.i)
    .map((x) => x.e);
}
