import { describe, expect, it } from 'vitest';

import {
  formatDate,
  formatDateTime,
  formatDecimalSize,
  formatRelative,
  formatSize,
} from './format';

describe('format', () => {
  it('formats sizes in binary units per locale', () => {
    expect(formatSize(0, 'en')).toBe('0 B');
    expect(formatSize(1536, 'en')).toBe('1.5 KB');
    expect(formatSize(1536, 'es')).toBe('1,5 KB');
    expect(formatSize(5 * 1024 ** 3, 'en')).toBe('5 GB');
  });
  it('formats disc sizes in decimal units, two decimals from GB', () => {
    expect(formatDecimalSize(999, 'es')).toBe('999 B');
    expect(formatDecimalSize(1_934, 'es')).toBe('1,9 kB');
    expect(formatDecimalSize(405_749_493, 'es')).toBe('405,7 MB');
    expect(formatDecimalSize(591_881_790, 'es')).toBe('591,9 MB');
    expect(formatDecimalSize(1_191_067_563, 'es')).toBe('1,19 GB');
    expect(formatDecimalSize(1_191_067_563, 'en')).toBe('1.19 GB');
    expect(formatDecimalSize(2_000_000_000, 'en')).toBe('2 GB');
    // Rounding never shows 1000 of a unit: it moves to the next one.
    expect(formatDecimalSize(999_960, 'en')).toBe('1 MB');
    expect(formatDecimalSize(999_996_000, 'en')).toBe('1 GB');
  });
  it('formats dates per locale', () => {
    expect(formatDate(1_700_000_000, 'en')).toMatch(/2023/);
    expect(formatDate(1_700_000_000, 'es')).toMatch(/2023/);
  });

  it('formats a past moment in its largest whole unit', () => {
    const now = 1_700_000_000_000;
    const day = 86_400_000;
    expect(formatRelative(now - 3 * day, 'es', now)).toBe('hace 3 días');
    expect(formatRelative(now - day, 'es', now)).toBe('ayer');
    expect(formatRelative(now - 3_600_000, 'en', now)).toBe('1 hour ago');
    expect(formatRelative(now - 400 * day, 'en', now)).toBe('last year');
    expect(formatRelative(now, 'en', now)).toBe('now');
  });

  it('adds the time to a date for tooltips', () => {
    expect(formatDateTime(1_700_000_000, 'es')).toMatch(/2023.*\d{1,2}:\d{2}/);
  });
});
