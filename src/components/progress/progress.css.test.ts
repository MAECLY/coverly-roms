/// <reference types="node" />
import { readFileSync } from 'node:fs';
import path from 'node:path';

import { describe, expect, it } from 'vitest';

// Read from disk: Vitest's CSS handling empties a `?raw` .css import.
const css = readFileSync(path.resolve(__dirname, 'progress.css'), 'utf8');

/** `source` with every `@media <query> { … }` block removed (balanced braces). */
function stripMedia(source: string, query: string): string {
  const head = `@media ${query}`;
  let out = '';
  let i = 0;
  for (;;) {
    const at = source.indexOf(head, i);
    if (at < 0) return out + source.slice(i);
    out += source.slice(i, at);
    const open = source.indexOf('{', at);
    let depth = 0;
    let j = open;
    for (; j < source.length; j++) {
      if (source[j] === '{') depth++;
      else if (source[j] === '}' && --depth === 0) break;
    }
    if (depth !== 0) throw new Error(`unbalanced block at ${at}`);
    i = j + 1;
  }
}

/** The body of the first `@media <query> { … }` block (balanced braces). */
function mediaBody(source: string, query: string): string {
  const at = source.indexOf(`@media ${query}`);
  if (at < 0) throw new Error(`no @media ${query}`);
  const open = source.indexOf('{', at);
  let depth = 0;
  for (let j = open; j < source.length; j++) {
    if (source[j] === '{') depth++;
    else if (source[j] === '}' && --depth === 0)
      return source.slice(open + 1, j);
  }
  throw new Error(`unbalanced block at ${at}`);
}

/** Declarations per selector of a flat rule list, later rules winning. */
function declarations(body: string): Map<string, Map<string, string>> {
  const out = new Map<string, Map<string, string>>();
  const bare = body.replace(/\/\*[\s\S]*?\*\//g, '');
  for (const [, sel, decls] of bare.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    for (const one of sel!.split(',').map((x) => x.trim())) {
      const map = out.get(one) ?? new Map<string, string>();
      for (const d of decls!.split(';')) {
        const k = d.indexOf(':');
        if (k > 0) map.set(d.slice(0, k).trim(), d.slice(k + 1).trim());
      }
      out.set(one, map);
    }
  }
  return out;
}

describe('progress.css', () => {
  it('uses token roles only: no hex, rgb(), hsl() or blend modes', () => {
    expect(css).not.toMatch(/#[0-9a-f]{3,8}\b/i);
    expect(css).not.toMatch(/\brgba?\(/i);
    expect(css).not.toMatch(/\bhsla?\(/i);
    expect(css).not.toMatch(/mix-blend-mode/);
  });

  it('never springs', () => {
    expect(css).not.toContain('--cr-ease-spring');
  });

  it('keeps every keyframe and animation inside the no-preference opt-in', () => {
    expect(css).toContain('@media (prefers-reduced-motion: no-preference)');
    expect(css).toMatch(/@keyframes/);
    const rest = stripMedia(css, '(prefers-reduced-motion: no-preference)');
    expect(rest).not.toMatch(/@keyframes/);
    expect(rest).not.toMatch(/\banimation(-name)?\s*:/);
  });

  it('maps the drawing for forced colours', () => {
    expect(css).toMatch(/@media \(forced-colors: active\)\s*\{/);
  });

  it('only reads custom properties the tokens define', () => {
    const tokens = readFileSync(
      path.resolve(__dirname, '../../styles/tokens.css'),
      'utf8',
    );
    const defined = new Set(
      [...tokens.matchAll(/(--cr-[a-z0-9-]+)\s*:/g)].map((m) => m[1]),
    );
    const used = [...css.matchAll(/var\((--cr-[a-z0-9-]+)/g)].map((m) => m[1]);
    expect(used.length).toBeGreaterThan(0);
    expect(used.filter((v) => !defined.has(v))).toEqual([]);
  });

  it('keeps the tick an open stroke and every drawn part visible in forced colours', () => {
    const forced = declarations(mediaBody(css, '(forced-colors: active)'));
    // A filled tick would paint the triangle under its two strokes.
    expect(forced.get('.ws-check')?.get('fill')).toBe('none');
    expect(forced.get('.ws-check')?.get('stroke')).toBe('Highlight');
    // The cap and the tab would otherwise keep brand colours or vanish on Canvas.
    expect(forced.get('.ws-cap')?.get('stroke')).toBe('Highlight');
    expect(forced.get('.ws-tab')?.get('stroke')).toBe('CanvasText');
  });
});
