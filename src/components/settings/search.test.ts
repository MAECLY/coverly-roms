import { describe, expect, it } from 'vitest';

import type { SettingEntry } from './registry';
import { foldText, searchSettings } from './search';

describe('foldText', () => {
  it('drops accents, case and placeholders', () => {
    expect(foldText('Organización {count}')).toBe('organizacion');
  });
});

describe('searchSettings', () => {
  it('matches without accents', () => {
    const entry: SettingEntry = {
      id: 'setting-x',
      section: 'organize',
      label: 'jobs.kind.organize',
    };
    expect(searchSettings('organizacion', 'es', [entry])).toEqual([entry]);
  });

  it('ranks label prefix, label word, hint, then keyword', () => {
    const prefix: SettingEntry = {
      id: 'setting-prefix',
      section: 'matching',
      label: 'settings.matching.regions',
    };
    const word: SettingEntry = {
      id: 'setting-word',
      section: 'matching',
      label: 'settings.matching.addRegion',
    };
    const hint: SettingEntry = {
      id: 'setting-hint',
      section: 'general',
      label: 'settings.general.theme',
      hint: 'settings.dedupe.oneGameOneRomDetails',
    };
    const keyword: SettingEntry = {
      id: 'setting-keyword',
      section: 'general',
      label: 'settings.general.language',
      keywords: 'settings.keywords.region',
    };
    expect(
      searchSettings('region', 'en', [keyword, hint, word, prefix]),
    ).toEqual([prefix, word, hint, keyword]);
  });

  it('returns nothing for an empty query', () => {
    expect(searchSettings('', 'es')).toEqual([]);
    expect(searchSettings('   ', 'es')).toEqual([]);
  });

  it('falls back to the other language', () => {
    expect(searchSettings('trash', 'es')[0]?.id).toBe(
      'setting-dedupe-action-trash',
    );
  });

  it('finds the external tools by rar', () => {
    expect(searchSettings('rar', 'es').map((e) => e.id)).toEqual([
      'setting-organize-external-tools',
    ]);
  });

  it('matches at word starts only', () => {
    const ids = searchSettings('rar', 'es').map((e) => e.id);
    expect(ids).not.toContain('setting-safety-clean-apple-double');
    expect(searchSettings('7-zip', 'en').map((e) => e.id)).toEqual([
      'setting-organize-external-tools',
    ]);
    expect(searchSettings('-', 'en')).toEqual([]);
  });

  it('finds the Trash through keywords in English', () => {
    expect(searchSettings('papelera', 'en').map((e) => e.id)).toContain(
      'setting-dedupe-action-trash',
    );
  });
});
