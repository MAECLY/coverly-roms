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
});
