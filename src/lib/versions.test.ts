import { describe, expect, it } from 'vitest';

import { compareVersions, displayVersion } from './versions';

describe('compareVersions (display order only)', () => {
  it('orders numbers naturally', () => {
    expect(compareVersions('1.0.2', '1.0.10')).toBeLessThan(0);
    expect(compareVersions('1.0.10', '1.0.2')).toBeGreaterThan(0);
    expect(compareVersions('1.9', '1.10')).toBeLessThan(0);
  });

  it('ignores a leading v and sorts equal text as equal', () => {
    expect(compareVersions('v1.2', '1.2')).toBe(0);
    expect(compareVersions('V2', 'v10')).toBeLessThan(0);
  });

  it('puts a missing version last', () => {
    expect(compareVersions(undefined, '1.0')).toBeGreaterThan(0);
    expect(compareVersions('1.0', undefined)).toBeLessThan(0);
    expect(compareVersions(undefined, undefined)).toBe(0);
  });

  it('sorts a list for display', () => {
    expect(['1.0.10', undefined, '1.0.2', '1.0'].sort(compareVersions)).toEqual(
      ['1.0', '1.0.2', '1.0.10', undefined],
    );
  });
});

describe('displayVersion', () => {
  it('adds one leading v', () => {
    expect(displayVersion('1.05')).toBe('v1.05');
    expect(displayVersion('v1.05')).toBe('v1.05');
    expect(displayVersion('V2')).toBe('V2');
  });
});
