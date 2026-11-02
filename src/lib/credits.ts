/**
 * Typed loader of `credits/credits.json` (adjustments D25, D26): the one
 * file the «Agradecimientos» tab, the README and the landing read. The
 * entries' strings live in the file as `{ es, en }`; the tab's chrome stays
 * in `locales/`.
 */
import raw from '../../credits/credits.json';
import type { Locale } from '@/i18n/core';
import { foldText, queryTokens } from './search';

/** The tab's groups, in display order (`TODO.md` §7). */
export const CREDIT_GROUPS = [
  'art-sources',
  'databases',
  'docs',
  'conventions',
  'libraries',
  'fonts-icons',
] as const;

export type CreditGroup = (typeof CREDIT_GROUPS)[number];

export interface LocalText {
  es: string;
  en: string;
}

export interface CreditLicense {
  spdx?: string;
  /** A proper name (`MIT`) or a phrase in both languages. */
  name: string | LocalText;
  url?: string;
}

export interface Credit {
  id: string;
  group: CreditGroup;
  /** A proper name or, when there is none, a phrase in both languages. */
  name: string | LocalText;
  /** Absent for emulators and for sources without a page of their own. */
  url?: string;
  use: LocalText;
  license: CreditLicense;
  /** The exact credit text, present whenever `required`. */
  credit?: LocalText;
  required: boolean;
  endpoints?: string[];
  datasets?: string[];
  sources?: string[];
}

/** One entry in one language, as the tab shows it. */
export interface CreditView {
  id: string;
  name: string;
  url?: string;
  use: string;
  license: { spdx?: string; name: string; url?: string };
  credit?: string;
  required: boolean;
}

export interface CreditGroupView {
  group: CreditGroup;
  entries: CreditView[];
}

function isText(v: unknown): v is LocalText {
  return (
    typeof v === 'object' &&
    v !== null &&
    typeof (v as LocalText).es === 'string' &&
    typeof (v as LocalText).en === 'string'
  );
}

function isOptString(v: unknown): boolean {
  return v === undefined || typeof v === 'string';
}

function isOptStrings(v: unknown): boolean {
  return (
    v === undefined ||
    (Array.isArray(v) && v.every((x) => typeof x === 'string'))
  );
}

/** Checks the shape of a parsed credits file; throws naming the entry. */
export function toCredits(file: unknown): Credit[] {
  const entries = (file as { entries?: unknown } | null)?.entries;
  if (!Array.isArray(entries)) throw new Error('credits: no entries');
  return entries.map((e: unknown, i) => {
    const c = e as Partial<Credit> & { license?: Partial<CreditLicense> };
    const ok =
      typeof c.id === 'string' &&
      (CREDIT_GROUPS as readonly string[]).includes(c.group as string) &&
      (typeof c.name === 'string' || isText(c.name)) &&
      isOptString(c.url) &&
      isText(c.use) &&
      typeof c.license === 'object' &&
      c.license !== null &&
      (typeof c.license.name === 'string' || isText(c.license.name)) &&
      isOptString(c.license.spdx) &&
      isOptString(c.license.url) &&
      (c.credit === undefined || isText(c.credit)) &&
      typeof c.required === 'boolean' &&
      isOptStrings(c.endpoints) &&
      isOptStrings(c.datasets) &&
      isOptStrings(c.sources);
    if (!ok)
      throw new Error(`credits: entry ${String(c.id ?? i)} is malformed`);
    return c as Credit;
  });
}

/** Every entry of the file, in file order. */
export const CREDITS: readonly Credit[] = toCredits(raw);

function pick(label: string | LocalText, locale: Locale): string {
  return typeof label === 'string' ? label : label[locale];
}

function localize(c: Credit, locale: Locale): CreditView {
  const view: CreditView = {
    id: c.id,
    name: pick(c.name, locale),
    use: c.use[locale],
    license: {
      name: pick(c.license.name, locale),
      ...(c.license.spdx !== undefined && { spdx: c.license.spdx }),
      ...(c.license.url !== undefined && { url: c.license.url }),
    },
    required: c.required,
  };
  if (c.url !== undefined) view.url = c.url;
  if (c.credit !== undefined) view.credit = c.credit[locale];
  return view;
}

function grouped(
  entries: readonly Credit[],
  locale: Locale,
): CreditGroupView[] {
  return CREDIT_GROUPS.map((group) => ({
    group,
    entries: entries
      .filter((c) => c.group === group)
      .map((c) => localize(c, locale)),
  }));
}

/** Every group in display order, entries in file order, in `locale`. */
export function groupCredits(locale: Locale): CreditGroupView[] {
  return grouped(CREDITS, locale);
}

/**
 * The groups with entries whose name, use or licence (in `locale`)
 * contain every word of `query`, ignoring accents and case. Groups without
 * a match are left out; an empty query returns {@link groupCredits}.
 */
export function searchCredits(
  query: string,
  locale: Locale,
): CreditGroupView[] {
  const tokens = queryTokens(query);
  if (tokens.length === 0) return groupCredits(locale);
  return grouped(CREDITS, locale)
    .map((g) => ({
      group: g.group,
      entries: g.entries.filter((e) => {
        const key = foldText(
          [e.name, e.use, e.license.name, e.license.spdx ?? ''].join(' '),
        );
        return tokens.every((t) => key.includes(t));
      }),
    }))
    .filter((g) => g.entries.length > 0);
}

/** How many entries the groups hold. */
export function creditCount(groups: readonly CreditGroupView[]): number {
  return groups.reduce((n, g) => n + g.entries.length, 0);
}
