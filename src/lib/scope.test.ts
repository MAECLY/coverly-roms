import { describe, expect, it } from 'vitest';

import type { EntryView } from '@/ipc/types';

import { dateBucket, inScope, sameScope } from './scope';

const DAY = 86_400;
const NOW = 1_000_000_000;
const e = (dir: string, system = 0, mtime = NOW) =>
  ({ dir, system, mtime }) as EntryView;

describe('scopes', () => {
  it('buckets dates by age', () => {
    expect(dateBucket(NOW - DAY, NOW)).toBe('week');
    expect(dateBucket(NOW + DAY, NOW)).toBe('week');
    expect(dateBucket(NOW - 10 * DAY, NOW)).toBe('month');
    expect(dateBucket(NOW - 100 * DAY, NOW)).toBe('year');
    expect(dateBucket(NOW - 400 * DAY, NOW)).toBe('older');
  });

  it('matches folders by path prefix on whole segments', () => {
    const folder = { kind: 'folder', path: 'ROMS/SFC' } as const;
    expect(inScope(e('ROMS/SFC'), folder, NOW)).toBe(true);
    expect(inScope(e('ROMS/SFC/Hacks'), folder, NOW)).toBe(true);
    expect(inScope(e('ROMS/SFC2'), folder, NOW)).toBe(false);
    expect(inScope(e('anything'), { kind: 'folder', path: '' }, NOW)).toBe(
      true,
    );
  });

  it('matches systems, dates and all', () => {
    expect(inScope(e('x', 2), { kind: 'system', system: 2 }, NOW)).toBe(true);
    expect(inScope(e('x', 1), { kind: 'system', system: 2 }, NOW)).toBe(false);
    expect(
      inScope(
        e('x', 0, NOW - 400 * DAY),
        { kind: 'date', bucket: 'older' },
        NOW,
      ),
    ).toBe(true);
    expect(inScope(e('x'), { kind: 'all' }, NOW)).toBe(true);
    expect(
      sameScope({ kind: 'system', system: 1 }, { kind: 'system', system: 1 }),
    ).toBe(true);
    expect(sameScope({ kind: 'system', system: 1 }, { kind: 'all' })).toBe(
      false,
    );
  });
});
