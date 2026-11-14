/// <reference types="node" />
import { readFileSync } from 'node:fs';
import path from 'node:path';

import { describe, expect, it } from 'vitest';

// Read from disk: Vitest's CSS handling empties a `?raw` .css import.
const css = readFileSync(path.resolve(__dirname, 'globals.css'), 'utf8');

/** Body of the first top-level `selector` rule (outside any @media). */
function rule(selector: string): string {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const found = new RegExp(`\\n${escaped} \\{([^}]*)\\}`).exec(css);
  expect(found).not.toBeNull();
  return found![1]!;
}

describe('game panel drawer', () => {
  it('contains its absolutely placed children, so a screen-reader-only live region at the end of a long Extras list never grows the page', () => {
    // `.sr-only` is `position: absolute`; with a static scroller its
    // containing block is the page, and the extras status line at the bottom
    // of a tall panel gave the whole window a scrollbar.
    expect(rule('.drawer')).toMatch(/position:\s*relative;/);
  });
});
