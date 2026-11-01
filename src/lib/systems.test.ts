import { describe, expect, it } from 'vitest';

import { FAMILY_ORDER, systemOptionGroups } from './systems';

const t = (k: string) =>
  ({
    'systems.family.console': 'Consolas',
    'systems.family.computer': 'Ordenadores',
    'systems.family.engine': 'Motores y consolas de fantasía',
  })[k] ?? k;

describe('systemOptionGroups', () => {
  it('keeps the family order, sorts by locale and drops empty groups', () => {
    const g = systemOptionGroups(
      [
        { id: 'b', name: 'Bravo', family: 'console', no_emulator: false },
        { id: 'a', name: 'Ámbar', family: 'console', no_emulator: false },
        {
          id: 'z',
          name: 'ZX Spectrum',
          family: 'computer',
          no_emulator: false,
        },
        { id: 't', name: 'TIC-80', family: 'engine', no_emulator: false },
      ],
      t,
      'es',
    );
    expect(g.map((x) => x.family)).toEqual(['console', 'computer', 'engine']);
    expect(g[0]!.systems.map((s) => s.name)).toEqual(['Ámbar', 'Bravo']);
    expect(g[0]!.label).toBe('Consolas');
  });

  it('lists the five families a collector thinks in, in that order', () => {
    expect(FAMILY_ORDER).toEqual([
      'console',
      'handheld',
      'computer',
      'arcade',
      'engine',
    ]);
  });

  it('never reorders or mutates the input list', () => {
    const input = [
      {
        id: 'b',
        name: 'Bravo',
        family: 'console' as const,
        no_emulator: false,
      },
      { id: 'a', name: 'Alfa', family: 'console' as const, no_emulator: false },
    ];
    systemOptionGroups(input, t, 'en');
    expect(input.map((s) => s.id)).toEqual(['b', 'a']);
  });
});
