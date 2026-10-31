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

  it('English and Spanish use the same placeholders in every string', () => {
    const value = (catalog: object, key: string): string =>
      key
        .split('.')
        .reduce<unknown>(
          (node, part) => (node as Record<string, unknown>)[part],
          catalog,
        ) as string;
    const vars = (s: string) =>
      [...new Set([...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1]))].sort();
    for (const key of flatten(es))
      expect(vars(value(en, key)), key).toEqual(vars(value(es, key)));
  });

  it('drops the old extraction line with its misleading file count', () => {
    for (const catalog of [en, es])
      expect(flatten(catalog)).not.toContain('organize.extracting');
  });

  it('never puts a capitalised art name mid-sentence in Spanish', () => {
    const p = es.profiles;
    for (const v of [p.slot, p.template, p.max_width, p.max_height])
      expect(v.startsWith('{media}')).toBe(true);
  });
});
