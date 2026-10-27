import { describe, expect, it } from 'vitest';

import { translate } from '../i18n/core';
import {
  etaKey,
  etaSeconds,
  formatRate,
  initialRate,
  pushSample,
  rateOf,
} from './rate';

const feed = (pts: [number, number][], key: number | string = 1) =>
  pts.reduce((s, [t, v]) => pushSample(s, key, v, t), initialRate());

describe('rate', () => {
  it('needs 2 s of samples', () => {
    expect(
      rateOf(
        feed([
          [0, 0],
          [1500, 15],
        ]),
        1500,
      ),
    ).toBeNull();
    expect(
      rateOf(
        feed([
          [0, 0],
          [2000, 20],
        ]),
        2000,
      ),
    ).toBeCloseTo(10);
  });

  it('keeps a 5 s window', () => {
    const s = feed([
      [0, 0],
      [1000, 100],
      [6000, 150],
      [8000, 170],
    ]);
    // Samples older than 5 s are dropped; the newest of them stays as the anchor.
    expect(s.samples.map((x) => x.t)).toEqual([1000, 6000, 8000]);
    expect(rateOf(s, 8000)).toBeCloseTo((170 - 100) / 7);
  });

  it('resets samples when bytes go backwards and on a new item, keeping the last rate', () => {
    let s = feed([
      [0, 0],
      [3000, 30],
    ]);
    s = pushSample(s, 2, 0, 3100);
    expect(s.samples).toHaveLength(1);
    expect(rateOf(s, 3100)).toBeCloseTo(10);
    s = pushSample(s, 2, 5, 3200);
    s = pushSample(s, 2, 1, 3300);
    expect(s.samples).toHaveLength(1);
    expect(rateOf(s, 3300)).toBeCloseTo(10);
  });

  it('does not carry a stalled zero rate into the next item', () => {
    let s = feed([
      [0, 0],
      [3000, 30],
      [3100, 31],
      [9000, 31],
      [9100, 31],
    ]);
    expect(rateOf(s, 9100)).toBe(0);
    s = pushSample(s, 2, 0, 9200);
    expect(rateOf(s, 9200)).toBeCloseTo(10);
  });

  it('falls back to measuring after a 10 s stall', () => {
    const s = feed([
      [0, 0],
      [3000, 30],
      [13_500, 30],
    ]);
    expect(rateOf(s, 13_500)).toBeNull();
  });

  it.each([
    [0, 'organize.progress.leftUnderMinute', undefined],
    [59, 'organize.progress.leftUnderMinute', undefined],
    [60, 'organize.progress.leftUnderMinute', undefined],
    [61, 'organize.progress.leftMinutes', { n: 2 }],
    [119, 'organize.progress.leftMinutes', { n: 2 }],
    [59 * 60, 'organize.progress.leftMinutes', { n: 59 }],
    [3600, 'organize.progress.leftOverHour', undefined],
    [3660, 'organize.progress.leftOverHour', undefined],
    [2 * 3600 + 59 * 60, 'organize.progress.leftOverHours', { n: 2 }],
  ])('eta %i s', (s, key, vars) => expect(etaKey(s)).toEqual({ key, vars }));

  it('never words a count below 2', () => {
    for (let s = 0; s <= 4 * 3600; s += 1) {
      const { vars } = etaKey(s);
      if (vars) expect(vars.n).toBeGreaterThanOrEqual(2);
    }
  });

  it('never renders a singular count in a plural wording', () => {
    for (const locale of ['es', 'en'] as const) {
      for (let s = 0; s <= 4 * 3600; s += 1) {
        const { key, vars } = etaKey(s);
        const text = translate(locale, key, vars);
        expect(text).not.toBe(key);
        expect(text).not.toMatch(/unos 1 |1 horas|about 1 minutes|1 hours/);
      }
    }
  });

  it('etaSeconds is null without a rate', () => {
    expect(etaSeconds(100, null)).toBeNull();
    expect(etaSeconds(100, 0)).toBeNull();
    expect(etaSeconds(100, 10)).toBe(10);
    expect(etaSeconds(-5, 10)).toBe(0);
    expect(etaSeconds(Number.NaN, 10)).toBeNull();
    expect(etaSeconds(100, Number.POSITIVE_INFINITY)).toBeNull();
  });

  it('formats decimal rates per locale', () => {
    expect(formatRate('es', 24_100_000)).toBe('24,1 MB');
    expect(formatRate('en', 24_100_000)).toBe('24.1 MB');
  });
});
