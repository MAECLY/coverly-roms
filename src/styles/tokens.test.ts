import { describe, expect, it } from 'vitest';

import {
  OUTPUTS,
  buildAll,
  buildCss,
  contrast,
  contrastFailures,
  readOutputs,
  readTokens,
  tailwindColors,
  themeColours,
} from '../../scripts/brand/build_tokens.mjs';

describe('design token build', () => {
  const tokens = readTokens();

  it('is deterministic: two builds from the same JSON are identical', () => {
    const first = buildAll(tokens);
    const second = buildAll(structuredClone(tokens));
    expect(second).toEqual(first);
    expect(Object.keys(first).sort()).toEqual(
      [OUTPUTS.app, OUTPUTS.site].sort(),
    );
  });

  it('matches the generated files on disk (run pnpm tokens if not)', () => {
    expect(readOutputs()).toEqual(buildAll(tokens));
  });

  it('keeps every contrast pair at or above its target', () => {
    expect(contrastFailures(tokens)).toEqual([]);
    expect(contrast('#000000', '#ffffff')).toBeCloseTo(21, 5);
  });

  it('emits light on :root and dark under the app theme selectors', () => {
    const css = buildCss(tokens, 'app');
    expect(css).toContain(':root {\n');
    expect(css).toContain(":root[data-theme='dark'] {\n");
    expect(css).toContain(':root:not([data-theme]) {\n');
    expect(css).toMatch(/--cr-primary: #0e5e6f;/);
    expect(css).toMatch(/--cr-primary-rgb: 14 94 111;/);
    expect(css).toMatch(/--background: #f2f3ef;/);
    expect(css).toMatch(/--cr-accent: #0e5e6f;/);
  });

  it('emits the landing selectors without app-only channels', () => {
    const css = buildCss(tokens, 'site');
    expect(css).toContain(":root:not([data-theme='light']) {\n");
    expect(css).toContain(":root[data-theme='dark'] {\n");
    expect(css).not.toMatch(/-rgb:/);
  });

  it('maps every Tailwind colour to an emitted channel variable', () => {
    const channels = new Set(
      themeColours(tokens, 'light')
        .filter((c) => c.rgb)
        .map((c) => `${c.name}-rgb`),
    );
    const colors = tailwindColors(tokens);
    const flat = Object.values(colors).flatMap((v) =>
      typeof v === 'string' ? [v] : Object.values(v),
    );
    for (const value of flat) {
      const name = /var\((--[\w-]+)\)/.exec(value)?.[1];
      expect(name && channels.has(name)).toBe(true);
    }
  });
});
