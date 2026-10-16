import { describe, expect, it } from 'vitest';

import en from '../../locales/en/ui.json';
import es from '../../locales/es/ui.json';

function flatten(node: object, prefix = ''): string[] {
  return Object.entries(node).flatMap(([key, value]) =>
    typeof value === 'string'
      ? [`${prefix}${key}`]
      : flatten(value as object, `${prefix}${key}.`),
  );
}

describe('locales', () => {
  it('English and Spanish have exactly the same keys', () => {
    expect(flatten(es).sort()).toEqual(flatten(en).sort());
  });

  it('never puts a capitalised art name mid-sentence in Spanish', () => {
    const p = es.profiles;
    for (const v of [p.slot, p.template, p.max_width, p.max_height])
      expect(v.startsWith('{media}')).toBe(true);
  });
});
