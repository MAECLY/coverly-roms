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

  it('finds the game folders switch by DLC and by updates', () => {
    expect(searchSettings('DLC', 'es')[0]?.id).toBe(
      'setting-organize-game-folders',
    );
    expect(searchSettings('updates', 'es').map((e) => e.id)).toContain(
      'setting-organize-game-folders',
    );
    expect(
      searchSettings('DLC', 'en').find(
        (e) => e.id === 'setting-organize-game-folders',
      )?.section,
    ).toBe('library');
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

  it('ranks a match in the current language above the same match in another', () => {
    // «Formato de salida» / "Output format" and «Orden de formatos por
    // perfil» / "Format order per profile": each is a label prefix in one
    // language only, so the UI language decides which comes first.
    const output: SettingEntry = {
      id: 'setting-output',
      section: 'art',
      label: 'settings.art.format',
    };
    const order: SettingEntry = {
      id: 'setting-order',
      section: 'duplicates',
      label: 'settings.dedupe.containerOrder',
    };
    expect(searchSettings('format', 'es', [order, output])).toEqual([
      output,
      order,
    ]);
    expect(searchSettings('format', 'en', [output, order])).toEqual([
      order,
      output,
    ]);
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

describe('credit entries', () => {
  it('finds a credit by its name, in the Credits tab', () => {
    const hit = searchSettings('Redump', 'es')[0];
    expect(hit?.id).toBe('credit-redump');
    expect(hit?.section).toBe('credits');
  });

  it('puts a service card ahead of its credit', () => {
    const ids = searchSettings('ScreenScraper', 'es').map((e) => e.id);
    expect(ids[0]).toBe('integration-screenscraper');
    expect(ids).toContain('credit-screenscraper');
  });

  it('keeps a setting ahead of a credit that only shares a word start', () => {
    expect(searchSettings('trash', 'en')[0]?.id).toBe(
      'setting-dedupe-action-trash',
    );
  });
});

describe('the Credits tab', () => {
  it.each([
    ['licencias', 'es'],
    ['créditos', 'es'],
    ['licenses', 'en'],
    ['attribution', 'en'],
  ] as const)('is found by «%s» (%s)', (query, locale) => {
    expect(searchSettings(query, locale).map((e) => e.id)).toContain(
      'setting-credits-search',
    );
  });
});

describe('integrations', () => {
  const ids = (q: string, l: 'es' | 'en') =>
    searchSettings(q, l).map((e) => e.id);

  it.each(['es', 'en'] as const)('finds each card by its name (%s)', (l) => {
    expect(ids('steamgriddb', l)[0]).toBe('integration-steamgriddb');
    expect(ids('screenscraper', l)[0]).toBe('integration-screenscraper');
    expect(ids('actualizaciones', 'es')).toContain('integration-updates');
    expect(ids('updates', 'en')).toContain('integration-updates');
  });

  it.each(['es', 'en'] as const)('finds a card by its field (%s)', (l) => {
    expect(ids('api key', l)[0]).toBe('integration-steamgriddb');
    expect(ids('token', l)[0]).toBe('integration-steamgriddb');
    expect(ids('password', l)[0]).toBe('integration-screenscraper');
  });

  it('finds ScreenScraper by «contraseña» and «usuario»', () => {
    expect(ids('contraseña', 'es')[0]).toBe('integration-screenscraper');
    expect(ids('usuario', 'es')).toContain('integration-screenscraper');
  });

  it.each(['es', 'en'] as const)(
    'finds the coming-later list by a planned name (%s)',
    (l) => {
      expect(ids('igdb', l)[0]).toBe('integrations-planned');
      expect(ids('retroachievements', l)).toContain('integrations-planned');
    },
  );

  it.each(['es', 'en'] as const)(
    'finds the Switch catalogue by titledb and its fallback by nlib (%s)',
    (l) => {
      expect(ids('titledb', l)).toContain(
        'setting-integrations-switch-catalog',
      );
      const nlib = searchSettings('nlib', l);
      const url = nlib.find(
        (e) => e.id === 'setting-integrations-switch-catalog-fallback-url',
      );
      expect(url?.section).toBe('integrations');
      expect(url?.fold).toBe('integrations-switch-advanced');
    },
  );

  it('lands on the moved catalogue, never on a Sources id', () => {
    expect(
      searchSettings('titledb', 'es').some((e) => e.section === 'sources'),
    ).toBe(false);
  });
});
