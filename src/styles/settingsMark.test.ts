/// <reference types="node" />
import { readFileSync } from 'node:fs';
import path from 'node:path';

import { describe, expect, it } from 'vitest';

// Read from disk: Vitest's CSS handling empties a `?raw` .css import.
const css = readFileSync(path.resolve(__dirname, 'globals.css'), 'utf8');

const escape = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

/** Body of the first top-level `selector` rule (outside any @media). */
function rule(selector: string): string {
  const found = new RegExp(`\\n${escape(selector)} \\{([^}]*)\\}`).exec(css);
  expect(found).not.toBeNull();
  return found![1]!;
}

/** Body of `selector` inside an `@media (forced-colors: active)` block. */
function forcedRule(selector: string): string | null {
  const blocks = css.matchAll(
    /@media \(forced-colors: active\) \{([\s\S]*?)\n\}/g,
  );
  for (const [, body] of blocks) {
    const found = new RegExp(`${escape(selector)} \\{([^}]*)\\}`).exec(body!);
    if (found) return found[1]!;
  }
  return null;
}

describe('arrival mark', () => {
  const marked = () => rule("[data-setting-row][data-arrived='true']");

  it('reaches past the row so the bar and tint never touch the control', () => {
    // A spread shadow in the tint and a bar shadow outside it: room on every
    // side of the row without moving a pixel of its content; the bar stays
    // 3px wide, as before.
    expect(marked()).toMatch(
      /box-shadow:\s*0 0 0 var\(--cr-space-2\) var\(--setting-mark-tint\),\s*-3px 0 0 var\(--cr-space-2\) var\(--cr-manila\);/,
    );
  });

  it('paints an opaque tint so the bar below never shows through', () => {
    expect(marked()).toMatch(
      /--setting-mark-tint:\s*color-mix\(in srgb, var\(--cr-manila\) 14%, var\(--cr-bg\)\);/,
    );
    expect(marked()).toMatch(/background-color:\s*var\(--setting-mark-tint\);/);
  });

  it('never changes the row box', () => {
    expect(marked()).not.toMatch(/(^|[\s;])(padding|margin)/);
  });

  it('is an outline offset from the row in forced colours', () => {
    const forced = forcedRule("[data-setting-row][data-arrived='true']");
    expect(forced).toMatch(/outline: 2px solid Highlight;/);
    expect(forced).toMatch(/outline-offset: var\(--cr-space-1\);/);
    expect(forced).toMatch(/box-shadow: none;/);
  });
});

describe('advanced fold content', () => {
  it('pads out past the mark without moving its rows', () => {
    // The mark reaches var(--cr-space-2) + 3px past a row; the fold content
    // clips, so it gives var(--cr-space-3) on every side and takes it back.
    const body = rule('.settings-fold__content');
    expect(body).toMatch(/margin: calc\(-1 \* var\(--cr-space-3\)\);/);
    expect(body).toMatch(/padding: var\(--cr-space-3\);/);
  });
});

describe('settings tab list in forced colours', () => {
  it('keeps the selected tab bar as a system colour', () => {
    const forced = forcedRule(
      ".settings-nav [role='tab'][data-state='active']::after",
    );
    expect(forced).toMatch(/background-color: Highlight;/);
  });
});
