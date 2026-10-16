import { describe, expect, it } from 'vitest';

import { FIXTURE_NOW, generateLibrary } from '@/ipc/mock/fixture';

import { auditSummary } from './audit';

describe('audit summary', () => {
  it('counts ROMs, missing art and complete ROMs per system', () => {
    const view = generateLibrary({
      seed: 7,
      count: 240,
      root: '/mock/a',
      generation: 1,
      now: FIXTURE_NOW,
    });
    const s = auditSummary(view);
    expect(s.media).toEqual(view.profile.media);
    expect(s.total.roms).toBe(view.entries.length);
    expect(s.systems.reduce((n, r) => n + r.roms, 0)).toBe(view.entries.length);
    const sys = view.systems.findIndex((x) => x.roms > 0);
    const mine = view.entries.filter((e) => e.system === sys);
    const row = s.systems.find((r) => r.folder === view.systems[sys]!.folder)!;
    expect(row.roms).toBe(mine.length);
    expect(row.missing.cover).toBe(
      mine.filter((e) => e.art.cover === 'missing').length,
    );
    expect(row.complete).toBe(
      mine.filter((e) =>
        view.profile.media.every((m) => e.art[m] !== 'missing'),
      ).length,
    );
    expect(s.systems.every((r) => r.roms > 0)).toBe(true);
  });
});
