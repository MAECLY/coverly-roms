import { describe, expect, it } from 'vitest';

import { formatDate, formatSize } from './format';

describe('format', () => {
  it('formats sizes in binary units per locale', () => {
    expect(formatSize(0, 'en')).toBe('0 B');
    expect(formatSize(1536, 'en')).toBe('1.5 KB');
    expect(formatSize(1536, 'es')).toBe('1,5 KB');
    expect(formatSize(5 * 1024 ** 3, 'en')).toBe('5 GB');
  });
  it('formats dates per locale', () => {
    expect(formatDate(1_700_000_000, 'en')).toMatch(/2023/);
    expect(formatDate(1_700_000_000, 'es')).toMatch(/2023/);
  });
});
