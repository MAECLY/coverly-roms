import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

import {
  END,
  READMES,
  START,
  checkReadmes,
  renderBlock,
  replaceBlock,
} from '../../scripts/credits/readme.mjs';

import { CREDITS, CREDIT_GROUPS } from './credits';

const REPO = join(__dirname, '..', '..');

function groupTitles(locale: 'es' | 'en'): Record<string, string> {
  const ui = JSON.parse(
    readFileSync(join(REPO, 'locales', locale, 'ui.json'), 'utf8'),
  ) as { settings: { credits: { groups: Record<string, string> } } };
  return ui.settings.credits.groups;
}

describe('README credits block (adjustments D25)', () => {
  for (const locale of ['es', 'en'] as const) {
    it(`lists every group and every required credit verbatim (${locale})`, () => {
      const block = renderBlock(REPO, locale);
      for (const group of CREDIT_GROUPS) {
        expect(block).toContain(`### ${groupTitles(locale)[group]}`);
      }
      for (const c of CREDITS) {
        const name = typeof c.name === 'string' ? c.name : c.name[locale];
        expect(block).toContain(name);
        if (c.credit) expect(block).toContain(c.credit[locale]);
      }
      const required = CREDITS.filter((c) => c.required);
      expect(required.length).toBeGreaterThan(0);
      for (const c of required) expect(c.credit).toBeDefined();
    });

    it(`never links an entry that has no url (${locale})`, () => {
      const block = renderBlock(REPO, locale);
      for (const c of CREDITS.filter((e) => e.url === undefined)) {
        const name = typeof c.name === 'string' ? c.name : c.name[locale];
        expect(block).not.toContain(`[${name}](`);
      }
    });
  }

  it('refuses a README without the markers', () => {
    expect(() => replaceBlock('# Title\n\nNo markers.\n', 'x')).toThrow(
      /credits:start/,
    );
  });

  it('replaces only what sits between the markers', () => {
    const text = `a\n${START}\nold\n${END}\nb\n`;
    expect(replaceBlock(text, 'new')).toBe(`a\n${START}\nnew\n${END}\nb\n`);
  });

  it('keeps both READMEs in sync with credits.json', () => {
    expect(READMES.map((r) => r.file)).toEqual(['README.md', 'README.es.md']);
    expect(checkReadmes(REPO)).toEqual([]);
  });
});
