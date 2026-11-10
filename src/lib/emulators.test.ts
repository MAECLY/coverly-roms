import { describe, expect, it } from 'vitest';

import { defaultEmulator, emulatorName, emulatorsFor } from './emulators';

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

  it('names an emulator by its product name', () => {
    expect(emulatorName('shadps4')).toBe('shadPS4');
    expect(emulatorName('yuzu-family')).toBe('yuzu');
  });
});
