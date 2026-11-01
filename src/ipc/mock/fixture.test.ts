import { describe, expect, it } from 'vitest';

import {
  FIXTURE_NOW,
  FIXTURE_SYSTEMS,
  generateLibrary,
  HOSTILE_NAMES,
  mulberry32,
} from './fixture';

const make = (count: number, seed = 12) =>
  generateLibrary({
    seed,
    count,
    root: '/mock/Fixture',
    generation: 1,
    now: FIXTURE_NOW,
  });

describe('fixture generator', () => {
  it('is deterministic for a seed', () => {
    expect(JSON.stringify(make(500))).toBe(JSON.stringify(make(500)));
    expect(JSON.stringify(make(500, 1))).not.toBe(JSON.stringify(make(500)));
    const a = mulberry32(3);
    const b = mulberry32(3);
    expect([a(), a(), a()]).toEqual([b(), b(), b()]);
  });

  it('produces exactly 12,000 well-formed entries quickly', () => {
    const started = performance.now();
    const view = make(12_000);
    expect(performance.now() - started).toBeLessThan(1500);
    expect(view.entries).toHaveLength(12_000);
    view.entries.forEach((e, i) => {
      expect(e.id).toBe(i);
      expect(view.systems[e.system]).toBeDefined();
      expect(e.rel_path.endsWith(`${e.name}.${e.ext}`)).toBe(true);
      expect(e.rel_path.startsWith(`${e.dir}/`)).toBe(true);
    });
    expect(view.systems.reduce((n, s) => n + s.roms, 0)).toBe(12_000);
    expect(new Set(view.entries.map((e) => e.rel_path)).size).toBe(12_000);
  });

  it('mixes art states for every media', () => {
    const view = make(12_000);
    for (const media of ['cover', 'screenshot', 'title'] as const) {
      const present = view.entries.filter(
        (e) => e.art[media] === 'present',
      ).length;
      expect(present / 12_000).toBeGreaterThan(0.2);
      expect(present / 12_000).toBeLessThan(0.8);
    }
    expect(view.profile.media).toEqual(['cover', 'screenshot', 'title']);
  });

  it('spreads modification dates and includes recent ones', () => {
    const view = make(12_000);
    const week = view.entries.filter((e) => FIXTURE_NOW - e.mtime < 7 * 86_400);
    const old = view.entries.filter(
      (e) => FIXTURE_NOW - e.mtime > 365 * 86_400,
    );
    expect(week.length).toBeGreaterThan(50);
    expect(old.length).toBeGreaterThan(1000);
  });

  it('includes the hostile names verbatim', () => {
    const names = make(240).entries.map((e) => e.name);
    for (const hostile of HOSTILE_NAMES) expect(names).toContain(hostile);
  });

  it('has PS4 and Xbox Series shelves, the second with no emulator', () => {
    const view = make(12_000);
    for (const [id, folder] of [
      ['ps4', 'PS4'],
      ['xboxseries', 'XBOXSERIES'],
    ] as const) {
      const sys = view.systems.find((s) => s.system_id === id)!;
      expect(sys.folder).toBe(folder);
      expect(sys.roms).toBeGreaterThan(0);
      expect(view.entries.some((e) => e.system === sys.index)).toBe(true);
    }
    expect(
      FIXTURE_SYSTEMS.filter((s) => s.no_emulator).map((s) => s.id),
    ).toEqual(['xboxseries']);
  });

  it('can be empty', () => {
    const view = make(0);
    expect(view.entries).toEqual([]);
    expect(view.systems.every((s) => s.roms === 0)).toBe(true);
  });
});
