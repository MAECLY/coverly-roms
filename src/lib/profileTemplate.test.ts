import { describe, expect, it } from 'vitest';

import type { ProfileFile } from '@/ipc/types';

import {
  previewProfile,
  renderTemplate,
  validateProfile,
} from './profileTemplate';

const mine: ProfileFile = {
  id: 'my-sd',
  name: 'My SD',
  roms_root: 'Roms',
  art_dirs: ['Imgs'],
  gamelist: false,
  slots: {
    cover: {
      template: '{system_dir}/{rel_dir}/Imgs/{stem}.png',
      format: 'png',
      max_width: 250,
      max_height: 360,
    },
  },
};
const builtins = ['onion', 'batocera'];

describe('profile templates (mirror of profiles::custom)', () => {
  it('renders without empty or doubled separators', () => {
    expect(
      renderTemplate('{system_dir}/{rel_dir}/Imgs/{stem}.png', {
        root: 'LIB',
        system_dir: 'LIB/Roms/SFC',
        rel_dir: '',
        stem: 'Zelda',
        system_id: 'snes',
        libretro: null,
      }),
    ).toBe('LIB/Roms/SFC/Imgs/Zelda.png');
  });

  it('previews library-relative paths', () => {
    expect(
      previewProfile(mine, {
        system_folder: 'SFC',
        rel_dir: 'Hacks',
        file_name: 'Zelda (USA).sfc',
      }),
    ).toEqual({ cover: 'Roms/SFC/Hacks/Imgs/Zelda (USA).png' });
  });

  it('refuses templates that escape the library and reserved ids', () => {
    const bad: ProfileFile = {
      ...mine,
      id: 'onion',
      slots: {
        cover: { ...mine.slots.cover!, template: '{root}/../{stem}.png' },
      },
    };
    expect(
      validateProfile(bad, builtins).map((e) => `${e.field}:${e.code}`),
    ).toEqual(['id:duplicate', 'slots.cover.template:invalid']);
    expect(validateProfile(mine, builtins)).toEqual([]);
  });
});
