import { describe, expect, it } from 'vitest';

import raw from '../../credits/credits.json';
import schema from '../../credits/credits.schema.json';
import {
  CREDIT_GROUPS,
  CREDITS,
  creditCount,
  groupCredits,
  searchCredits,
  toCredits,
} from './credits';

describe('credits data', () => {
  it('has es and en for every use, credit and phrase name', () => {
    for (const c of CREDITS) {
      if (typeof c.name !== 'string') {
        expect(c.name.es.trim(), `${c.id} name.es`).not.toBe('');
        expect(c.name.en.trim(), `${c.id} name.en`).not.toBe('');
      }
      expect(c.use.es.trim(), `${c.id} use.es`).not.toBe('');
      expect(c.use.en.trim(), `${c.id} use.en`).not.toBe('');
      if (c.credit) {
        expect(c.credit.es.trim(), `${c.id} credit.es`).not.toBe('');
        expect(c.credit.en.trim(), `${c.id} credit.en`).not.toBe('');
      }
      if (typeof c.license.name !== 'string') {
        expect(c.license.name.es.trim(), `${c.id} license.es`).not.toBe('');
        expect(c.license.name.en.trim(), `${c.id} license.en`).not.toBe('');
      }
    }
  });

  it('follows the Spanish copy rules', () => {
    for (const c of CREDITS) {
      const texts = [c.use.es, c.credit?.es ?? ''];
      if (typeof c.name !== 'string') texts.push(c.name.es);
      if (typeof c.license.name !== 'string') texts.push(c.license.name.es);
      for (const t of texts) {
        expect(t, `${c.id}: no exclamation marks`).not.toMatch(/[!¡]/);
        expect(t, `${c.id}: contract «de el» and «a el»`).not.toMatch(
          /\b(de|a) el\b/i,
        );
      }
    }
  });

  it('fills every group', () => {
    for (const locale of ['es', 'en'] as const) {
      const groups = groupCredits(locale);
      expect(groups.map((g) => g.group)).toEqual([...CREDIT_GROUPS]);
      for (const g of groups)
        expect(g.entries.length, g.group).toBeGreaterThan(0);
    }
  });

  it('keeps the groups of the schema and the loader in step', () => {
    const groupEnum = schema.$defs.entry.properties.group.enum;
    expect(groupEnum).toEqual([...CREDIT_GROUPS]);
  });

  it('has the required credits with a credit text', () => {
    for (const id of [
      'screenscraper',
      'retroarch-assets',
      'gametdb',
      'libretro-database',
      'titledb',
    ]) {
      const c = CREDITS.find((x) => x.id === id);
      expect(c, id).toBeDefined();
      expect(c?.required, id).toBe(true);
      expect(c?.credit, id).toBeDefined();
    }
    expect(CREDITS.find((c) => c.id === 'gametdb')?.url).toBe(
      'https://www.gametdb.com/',
    );
  });

  it('localises an entry', () => {
    const [es] = groupCredits('es')
      .flatMap((g) => g.entries)
      .filter((c) => c.id === 'screenscraper');
    const [en] = groupCredits('en')
      .flatMap((g) => g.entries)
      .filter((c) => c.id === 'screenscraper');
    expect(es?.use).toMatch(/Organizar/);
    expect(en?.use).toMatch(/Organize/);
    expect(es?.license.name).toBe('CC BY-NC-SA 4.0');
    expect(es?.credit).toMatch(/^Datos y medios de ScreenScraper/);
  });

  it('rejects a malformed file', () => {
    expect(() => toCredits({ entries: [{ id: 'x', group: 'nope' }] })).toThrow(
      /x/,
    );
    expect(() => toCredits({})).toThrow();
    expect(toCredits(raw)).toHaveLength(CREDITS.length);
  });
});

describe('searchCredits', () => {
  it('finds the RetroArch icons entry', () => {
    const groups = searchCredits('retroarch', 'es');
    const ids = groups.flatMap((g) => g.entries.map((e) => e.id));
    expect(ids).toContain('retroarch-assets');
    expect(groups.every((g) => g.entries.length > 0)).toBe(true);
    expect(
      groups.find((g) => g.entries.some((e) => e.id === 'retroarch-assets'))
        ?.group,
    ).toBe('fonts-icons');
  });

  it('ignores accents and case', () => {
    const plain = searchCredits('identificacion', 'es').flatMap(
      (g) => g.entries,
    );
    const accented = searchCredits('IDENTIFICACIÓN', 'es').flatMap(
      (g) => g.entries,
    );
    expect(plain.length).toBeGreaterThan(0);
    expect(accented.map((e) => e.id)).toEqual(plain.map((e) => e.id));
  });

  it('matches the licence and needs every word', () => {
    const ofl = searchCredits('ofl', 'en').flatMap((g) =>
      g.entries.map((e) => e.id),
    );
    expect(ofl).toEqual(
      expect.arrayContaining([
        'archivo',
        'atkinson-hyperlegible-next',
        'atkinson-hyperlegible-mono',
      ]),
    );
    expect(searchCredits('screenscraper zzzz', 'en')).toEqual([]);
  });

  it('returns every group for an empty query', () => {
    expect(searchCredits('  ', 'en')).toEqual(groupCredits('en'));
    expect(creditCount(searchCredits('', 'en'))).toBe(CREDITS.length);
  });
});
