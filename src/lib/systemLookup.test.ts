import { describe, expect, it } from 'vitest';

import type { SystemView } from '@/ipc/types';

import { systemByFolder } from './systemLookup';

const sys = (
  index: number,
  folder: string,
  dir: string,
  system_id: string | null,
): SystemView => ({ index, folder, dir, system_id, name: folder, roms: 1 });

const systems: SystemView[] = [
  sys(0, 'GB', '/sd/Roms/GB', 'gb'),
  sys(1, 'gba', '/sd/Roms/gba', 'gba'),
  sys(2, 'GBA', '/sd/Roms/GBA', 'gba'),
  sys(3, 'Homebrew', '/sd/Roms/Homebrew', null),
];

describe('systemByFolder', () => {
  it('matches the folder exactly first', () => {
    expect(systemByFolder(systems, 'GBA')?.index).toBe(2);
    expect(systemByFolder(systems, 'gba')?.index).toBe(1);
  });
  it('matches the directory', () => {
    expect(systemByFolder(systems, '/sd/Roms/GB')?.index).toBe(0);
  });
  it('falls back to a case-insensitive folder match', () => {
    expect(systemByFolder(systems, 'gb')?.index).toBe(0);
    expect(systemByFolder(systems, 'homebrew')?.system_id).toBeNull();
  });
  it('returns undefined for an unknown folder', () => {
    expect(systemByFolder(systems, 'NES')).toBeUndefined();
    expect(systemByFolder([], 'GB')).toBeUndefined();
  });
});
