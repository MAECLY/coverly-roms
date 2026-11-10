/**
 * The emulators a translator knows and the systems each lays out (mods
 * spec D16), mirroring Rust `translate::EmulatorId`. The app only names
 * them; where files go is always the core's plan.
 */
import type { EmulatorId, ProfileFile } from '@/ipc/types';

/** `translate::RETROARCH_SYSTEMS`: every built-in retro system with a
 * libretro name (mods spec D16). */
export const RETROARCH_SYSTEMS: readonly string[] = [
  'a2600',
  'a5200',
  'a7800',
  'a800',
  'amiga',
  'atarist',
  'atomiswave',
  'c64',
  'cps1',
  'cps2',
  'cps3',
  'dos',
  'dreamcast',
  'easyrpg',
  'fbneo',
  'nes',
  'fds',
  'gb',
  'gba',
  'gbc',
  'gamegear',
  'gw',
  'hbmame',
  'lynx',
  'mame',
  'megadrive',
  'segacd',
  'msx',
  'n64',
  'naomi',
  'nds',
  'neocd',
  'neogeo',
  'ngp',
  'ngpc',
  'pcengine',
  'pcenginecd',
  'pgm2',
  'pokemini',
  'psx',
  'psp',
  'saturn',
  'scummvm',
  'sega32x',
  'snes',
  'mastersystem',
  'varcade',
  'virtualboy',
  'vic20',
  'wonderswan',
  'wonderswancolor',
  '3do',
  'pcfx',
  'cdi',
  'amigacd32',
  'cdtv',
  'colecovision',
  'intellivision',
  'vectrex',
  'odyssey2',
  'channelf',
  'arcadia',
  'scv',
  'jaguar',
  'sg1000',
  'pico',
  'supergrafx',
  'n64dd',
  'satellaview',
  'sufami',
  'supervision',
  'gamecom',
  'zxspectrum',
  'zx81',
  'amstradcpc',
  'thomson',
  'x68000',
  'x1',
  'pc98',
  'pc88',
  'plus4',
  'pet',
  'atarixe',
  'msx2',
  'tic80',
  'wasm4',
  'arduboy',
  'lowresnx',
  'vircon32',
  'doom',
  'quake',
  'cavestory',
  'wolf3d',
  'mrboom',
  'dinothawr',
  'lutro',
  'flashback',
  'xrick',
  'model2',
  'model3',
  'naomi2',
];

/** The product name of each emulator (Rust `EmulatorId::display_name`):
 * a product name, never translated and never linked. */
export const EMULATOR_NAMES: Readonly<Record<EmulatorId, string>> = {
  shadps4: 'shadPS4',
  rpcs3: 'RPCS3',
  azahar: 'Azahar',
  cemu: 'Cemu',
  vita3k: 'Vita3K',
  ryujinx: 'Ryujinx',
  'yuzu-family': 'yuzu',
  retroarch: 'RetroArch',
  duckstation: 'DuckStation',
  pcsx2: 'PCSX2',
  dolphin: 'Dolphin',
  ppsspp: 'PPSSPP',
  xenia: 'Xenia',
  flycast: 'Flycast',
  mesen: 'Mesen',
};

/** The system ids each emulator's translator lays out, in Rust's
 * declaration order (`EmulatorId::systems`). */
const EMULATOR_SYSTEMS: readonly (readonly [EmulatorId, readonly string[]])[] =
  [
    ['shadps4', ['ps4']],
    ['rpcs3', ['ps3']],
    ['azahar', ['3ds']],
    ['cemu', ['wiiu']],
    ['vita3k', ['psvita']],
    ['ryujinx', ['switch']],
    ['yuzu-family', ['switch']],
    ['retroarch', RETROARCH_SYSTEMS],
    ['duckstation', ['psx']],
    ['pcsx2', ['ps2']],
    ['dolphin', ['gamecube', 'wii']],
    ['ppsspp', ['psp']],
    ['xenia', ['xbox360']],
    ['flycast', ['dreamcast']],
    ['mesen', ['nes']],
  ];

/** Every emulator that runs `system` (Rust `EmulatorId::for_system`). */
export function emulatorsFor(system: string): EmulatorId[] {
  return EMULATOR_SYSTEMS.filter(([, systems]) => systems.includes(system)).map(
    ([id]) => id,
  );
}

/** The emulator a layout lays `system` out for by default (D16), or
 * `null` when it keeps the library layout. */
export function defaultEmulator(
  profile: Pick<ProfileFile, 'emulators'> | null | undefined,
  system: string,
): EmulatorId | null {
  return profile?.emulators?.[system] ?? null;
}

/** An emulator's product name for the copy. */
export function emulatorName(id: EmulatorId): string {
  return EMULATOR_NAMES[id] ?? id;
}
