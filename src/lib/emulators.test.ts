import { readFileSync } from 'node:fs';
import { join } from 'node:path';

import { describe, expect, it } from 'vitest';

import type { EmulatorId } from '@/ipc/types';

import {
  EMULATOR_NAMES,
  RETROARCH_SYSTEMS,
  defaultEmulator,
  emulatorName,
  emulatorsFor,
} from './emulators';

/** The core's translator table, read as text: the TS mirror must not
 * drift from it (mods spec D16). */
const RUST = readFileSync(
  join(
    __dirname,
    '..',
    '..',
    'crates',
    'coverly-core',
    'src',
    'translate',
    'mod.rs',
  ),
  'utf8',
);

/** The body of `fn name` in `impl EmulatorId`, up to its closing brace. */
function rustFn(name: string): string {
  const start = RUST.indexOf(`pub fn ${name}(self)`);
  expect(start, name).toBeGreaterThan(-1);
  return RUST.slice(start, RUST.indexOf('\n    }\n', start));
}

/** `EmulatorId::Variant => <rhs>` arms of a match, by variant. */
function arms(body: string): Map<string, string> {
  const out = new Map<string, string>();
  const re = /((?:EmulatorId::\w+\s*\|?\s*)+)=>\s*(.+?),?$/gm;
  for (const m of body.matchAll(re)) {
    for (const v of m[1]!.matchAll(/EmulatorId::(\w+)/g))
      out.set(v[1]!, m[2]!.trim());
  }
  return out;
}

const quoted = (s: string) => [...s.matchAll(/"([^"]+)"/g)].map((m) => m[1]!);

describe('emulators', () => {
  it('lists the emulators of a system in the core order', () => {
    expect(emulatorsFor('ps4')).toEqual(['shadps4']);
    expect(emulatorsFor('switch')).toEqual(['ryujinx', 'yuzu-family']);
    expect(emulatorsFor('gamecube')).toEqual(['dolphin']);
    expect(emulatorsFor('snes')).toEqual(['retroarch']);
    expect(emulatorsFor('nes')).toEqual(['retroarch', 'mesen']);
    expect(emulatorsFor('psx')).toEqual(['retroarch', 'duckstation']);
  });

  it('has none for a system no translator lays out', () => {
    expect(emulatorsFor('nothing-runs-it')).toEqual([]);
  });

  it('takes the default from the layout', () => {
    expect(defaultEmulator({ emulators: { ps4: 'shadps4' } }, 'ps4')).toBe(
      'shadps4',
    );
    expect(
      defaultEmulator({ emulators: { ps4: 'shadps4' } }, 'ps3'),
    ).toBeNull();
    expect(defaultEmulator({}, 'ps4')).toBeNull();
    expect(defaultEmulator(null, 'ps4')).toBeNull();
  });

  it('mirrors the core table: ids, names and systems', () => {
    const ids = arms(rustFn('id'));
    const names = arms(rustFn('display_name'));
    const systems = arms(rustFn('systems'));
    const list = RUST.slice(
      RUST.indexOf('pub const RETROARCH_SYSTEMS'),
      RUST.indexOf('];', RUST.indexOf('pub const RETROARCH_SYSTEMS')),
    );
    expect(RETROARCH_SYSTEMS).toEqual(quoted(list));
    const ts: Record<string, string> = {};
    const bySystem = new Map<string, EmulatorId[]>();
    for (const [variant, idRhs] of ids) {
      const id = quoted(idRhs)[0] as EmulatorId;
      ts[id] = quoted(names.get(variant)!)[0]!;
      const rhs = systems.get(variant)!;
      const runs = rhs.includes('RETROARCH_SYSTEMS')
        ? [...RETROARCH_SYSTEMS]
        : quoted(rhs);
      expect(runs.length, variant).toBeGreaterThan(0);
      for (const sys of runs)
        bySystem.set(sys, [...(bySystem.get(sys) ?? []), id]);
    }
    expect(EMULATOR_NAMES).toEqual(ts);
    expect(bySystem.get('wii')).toEqual(['dolphin']);
    expect(ids.size).toBe(Object.keys(EMULATOR_NAMES).length);
    for (const [sys, emulators] of bySystem) {
      expect(emulatorsFor(sys), sys).toEqual(emulators);
    }
  });

  it('names an emulator by its product name', () => {
    expect(emulatorName('shadps4')).toBe('shadPS4');
    expect(emulatorName('yuzu-family')).toBe('yuzu');
  });
});
