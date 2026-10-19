/// <reference types="node" />
import { readFileSync } from 'node:fs';
import path from 'node:path';

import { describe, expect, it } from 'vitest';

// Read from disk: Vitest's CSS handling empties a `?raw` .css import.
const css = readFileSync(path.resolve(__dirname, 'globals.css'), 'utf8');

/** Widths of every `@media (<kind>-width: Npx)` block whose body names `selector`. */
function breakpoints(kind: 'min' | 'max', selector: string): number[] {
  const re = new RegExp(
    `@media \\(${kind}-width: (\\d+)px\\) \\{([\\s\\S]*?)\\n\\}`,
    'g',
  );
  return [...css.matchAll(re)]
    .filter((m) => m[2]!.includes(selector))
    .map((m) => Number(m[1]));
}

describe('library toolbar breakpoints', () => {
  // Measured in the browser (E2E, Spanish, Fixture SD): with the six view
  // segments and every label shown the toolbar needs ~1460px for one row.
  const ONE_ROW_WITH_LABELS = 1460;

  it('collapses secondary labels before the toolbar would wrap', () => {
    const [labels] = breakpoints('max', '.toolbar-label');
    expect(labels).toBeGreaterThanOrEqual(ONE_ROW_WITH_LABELS);
  });

  it('shows the icons exactly where the labels hide', () => {
    const [labels] = breakpoints('max', '.toolbar-label');
    const [icons] = breakpoints('min', '.toolbar-icon');
    expect(icons).toBe(labels! + 1);
  });

  it('keeps the six view segments wider than the label collapse point', () => {
    const [segments] = breakpoints('max', '.view-switcher__segments');
    const [labels] = breakpoints('max', '.toolbar-label');
    expect(segments).toBeLessThan(labels!);
  });
});
