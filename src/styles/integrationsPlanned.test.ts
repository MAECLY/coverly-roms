/// <reference types="node" />
import { readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';

import { describe, expect, it } from 'vitest';

// Read from disk: Vitest's CSS handling empties a `?raw` .css import.
const css = readFileSync(path.resolve(__dirname, 'globals.css'), 'utf8');

const escape = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

describe('«Próximamente» rows on a narrow window', () => {
  it('lay the name beside what it will do on a wide window', () => {
    const rule = new RegExp(
      `\\n${escape('.integrations-planned__row')} \\{([^}]*)\\}`,
    ).exec(css);
    expect(rule).not.toBeNull();
    expect(rule![1]).toMatch(/display: grid;/);
    expect(rule![1]).toMatch(
      /grid-template-columns: minmax\(9rem, max-content\) 1fr;/,
    );
  });

  it('stack the name over what it will do at 390 px', () => {
    const media = /@media \(max-width: 34rem\) \{([\s\S]*?)\n\}/.exec(css);
    expect(media).not.toBeNull();
    expect(media![1]).toMatch(
      /\.integrations-planned__row \{[^}]*grid-template-columns: 1fr;/,
    );
  });
});

describe('settings components', () => {
  // The Tailwind config mixes px and rem screens, so Tailwind 3.4 drops
  // every arbitrary `max-[…]:` / `min-[…]:` variant without a word: the
  // class is in the markup and no rule exists. Narrow layouts live in CSS.
  it('use no arbitrary breakpoint variant Tailwind would drop', () => {
    const dir = path.resolve(__dirname, '../components/settings');
    const offenders = readdirSync(dir)
      .filter((f) => f.endsWith('.tsx') && !f.includes('.test.'))
      .filter((f) =>
        /\b(?:max|min)-\[[0-9.]+(?:rem|px|em)\]:/.test(
          readFileSync(path.join(dir, f), 'utf8'),
        ),
      );
    expect(offenders).toEqual([]);
  });
});
