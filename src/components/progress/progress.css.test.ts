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
});
