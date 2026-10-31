/// <reference types="node" />
import { readFileSync } from 'node:fs';
import path from 'node:path';

import { describe, expect, it } from 'vitest';

// Read from disk: Vitest's CSS handling empties a `?raw` .css import.
const css = readFileSync(path.resolve(__dirname, 'dedupe.css'), 'utf8');

/** The bodies of every `@media <query> { … }` block, joined. */
function mediaBodies(source: string, query: string): string {
  const head = `@media ${query}`;
  let out = '';
  for (
    let at = source.indexOf(head);
    at >= 0;
    at = source.indexOf(head, at + 1)
  ) {
    const open = source.indexOf('{', at);
    let depth = 0;
    for (let j = open; j < source.length; j++) {
      if (source[j] === '{') depth++;
      else if (source[j] === '}' && --depth === 0) {
        out += source.slice(open + 1, j);
        break;
      }
    }
  }
  return out;
}

/** Declarations per selector of a flat rule list, later rules winning. */
function declarations(body: string): Map<string, Map<string, string>> {
  const out = new Map<string, Map<string, string>>();
  const bare = body.replace(/\/\*[\s\S]*?\*\//g, '');
  for (const [, sel, decls] of bare.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    for (const one of sel!.split(',').map((x) => x.trim())) {
      const map = out.get(one) ?? new Map<string, string>();
      for (const d of decls!.split(';')) {
        const i = d.indexOf(':');
        if (i < 0) continue;
        map.set(d.slice(0, i).trim(), d.slice(i + 1).trim());
      }
      out.set(one, map);
    }
  }
  return out;
}

describe('dedupe.css at phone widths', () => {
  const phone = declarations(mediaBodies(css, '(max-width: 599px)'));

  it('gives a conflict save path the whole row, its size beside the date', () => {
    // A 9ch size column beside the name left a save's absolute path a
    // sliver at 390 px, broken every few letters.
    expect(phone.get('.dup-conflict__save')?.get('grid-template-columns')).toBe(
      'auto minmax(0, 1fr)',
    );
    const size = phone.get('.dup-conflict__save .dup-row__size');
    const date = phone.get('.dup-conflict__save .dup-row__meta');
    expect(size?.get('grid-row')).toBe('2');
    expect(size?.get('grid-column')).toBe('2');
    expect(date?.get('grid-row')).toBe('2');
    expect(date?.get('justify-self')).toBe('start');
  });
});
