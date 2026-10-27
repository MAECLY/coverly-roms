/// <reference types="node" />
import { readFileSync } from 'node:fs';
import path from 'node:path';

import { describe, expect, it } from 'vitest';

// Read from disk: Vitest's CSS handling empties a `?raw` .css import.
const css = readFileSync(path.resolve(__dirname, 'globals.css'), 'utf8');

/** Body of the `selector` rule inside the `@media (max-width: 40rem)` block. */
function narrowRule(selector: string): string {
  const media = /@media \(max-width: 40rem\) \{([\s\S]*?)\n\}/.exec(css);
  expect(media).not.toBeNull();
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const rule = new RegExp(`${escaped} \\{([^}]*)\\}`).exec(media![1]!);
  expect(rule).not.toBeNull();
  return rule![1]!;
}

describe('settings tab row on narrow windows', () => {
  const row = () => narrowRule(".settings-nav [role='tablist']");

  it('scrolls in one row inside itself', () => {
    expect(row()).toMatch(/flex-wrap: nowrap;/);
    expect(row()).toMatch(/overflow-x: auto;/);
    expect(row()).toMatch(/overscroll-behavior-x: contain;/);
  });

  it('keeps a scrollbar so a mouse without a trackpad reaches every tab', () => {
    expect(row()).not.toMatch(/scrollbar-width: none/);
  });
});
