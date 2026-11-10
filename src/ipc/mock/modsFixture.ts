/**
 * The mods pile of the browser mock (`?mockPick=mods`, mods spec §9.3):
 * synthetic names and sizes, no real data, no keys. Each file says what
 * the core would read from it: a ROM or console game (as in
 * `gameFoldersFixture.ts`), or an addon with the link the core would find
 * (`addon`, its base by file name in `of`). `MODS_TRANSLATIONS` is what
 * each emulator's translator would do with an entry, so
 * `mockOrganize.ts` can re-derive the placements for the emulator a run
 * chooses.
 */
import type {
  AddonLink,
  EmulatorId,
  LeftReason,
  PlacementTo,
  TranslateNote,
} from '../types';
import { EMULATOR_NAMES } from '../../lib/emulators';

import type { GameFolderFile } from './gameFoldersFixture';

export const MOCK_MODS = '/Users/demo/Mods';

export interface ModsFile extends GameFolderFile {
  /** A folder moved whole (an extracted game, a mod folder). */
  folder?: true;
  /** The catalogue name a hash match gave (retro ROMs). */
  catalogue?: string;
  /** A linked addon: what it is and how it was linked; `of` names its
   * base and `title` is the base's title for the row's note. */
  addon?: AddonLink;
  title?: string;
}

/** What one emulator's translator does with an entry: `same` keeps the
 * place the profile gave it (a base is never moved). */
export interface MockRule {
  to: PlacementTo | 'same';
  note?: TranslateNote;
}

export { EMULATOR_NAMES };

const SD3 = 'Seiken Densetsu 3 (Japan).sfc';
const TMZ = 'Tengai Makyou Zero (Japan).sfc';
const FF5 = 'Final Fantasy V (Japan).sfc';
const MOTHER = 'Mother (Japan).nes';
const MGS = 'Metal Gear Solid (USA) (Disc 1).chd';
const POKEMON = 'Pokemon Y (World).cia';
const ACNH = 'Animal Crossing New Horizons [01006F8002326000].nsp';
const ACNH_ID = '01006F8002326000';
const ACNH_BUILD = '7F6F4D3C2B1A0908';
const GR2 = 'CUSA04943';
const FOLKLORE = 'Folklore (USA).pkg';
const MK8_US = 'Mario Kart 8 (USA).wua';
const MK8_EU = 'Mario Kart 8 (Europe).wua';
const XENO = 'Xenoblade Chronicles 3 (World)';
const XENO_BASE = `${XENO}/${XENO}.nsp`;
const XENO_ID = '010074F013262000';

const snes = (name: string, crc: string): ModsFile => ({
  name,
  size: 4_194_304,
  system: 'snes',
  detail: `${crc} ${name.slice(0, -4)}`,
  catalogue: name.slice(0, -4),
});

const unlinked = (
  name: string,
  size: number,
  why: Extract<LeftReason, { kind: 'addon-unlinked' }>,
): ModsFile => ({ name, size, left: why });

export const MODS_FILES: readonly ModsFile[] = [
  // SNES: a BPS linked by its source ROM's CRC32.
  snes(SD3, 'D0176B24'),
  {
    name: 'Seiken Densetsu 3 (Japan) [T-En by DeJap].bps',
    size: 1_012_224,
    system: 'snes',
    of: SD3,
    title: 'Seiken Densetsu 3 (Japan)',
    addon: {
      kind: 'patch',
      format: 'bps',
      key_text: 'CRC32 D0176B24',
      via: 'source-crc',
      header: 'matches',
    },
  },
  // SNES: an IPS linked by its readme.
  snes(TMZ, '3A9D6B52'),
  {
    name: 'TMZ English v1.01.ips',
    size: 2_310_144,
    system: 'snes',
    of: TMZ,
    title: 'Tengai Makyou Zero (Japan)',
    addon: {
      kind: 'patch',
      format: 'ips',
      key_text: 'Tengai Makyou Zero (Japan)',
      via: 'readme',
    },
  },
  // SNES: two IPS for one ROM (RetroArch takes one patch per ROM).
  snes(FF5, 'C1BC267D'),
  {
    name: 'FF5 Restored Script.ips',
    size: 524_800,
    system: 'snes',
    of: FF5,
    title: 'Final Fantasy V (Japan)',
    addon: {
      kind: 'patch',
      format: 'ips',
      key_text: 'Final Fantasy V (Japan)',
      via: 'readme',
    },
  },
  {
    name: 'FF5 Sound Restoration.ips',
    size: 98_304,
    system: 'snes',
    of: FF5,
    title: 'Final Fantasy V (Japan)',
    addon: {
      kind: 'patch',
      format: 'ips',
      key_text: 'Final Fantasy V (Japan)',
      via: 'readme',
    },
  },
  // NES: a headered ROM and a patch made for the headerless one.
  {
    name: MOTHER,
    size: 262_160,
    system: 'nes',
    detail: 'iNES header',
    catalogue: 'Mother (Japan)',
  },
  {
    name: 'Mother (Japan) [T-En].bps',
    size: 131_072,
    system: 'nes',
    of: MOTHER,
    title: 'Mother (Japan)',
    addon: {
      kind: 'patch',
      format: 'bps',
      key_text: 'CRC32 7F8E1A20',
      via: 'source-crc',
      header: 'needs-headerless',
    },
  },
  // PS1: a base, its cheats and its texture pack, both by serial.
  {
    name: MGS,
    size: 512_000_000,
    system: 'psx',
    detail: 'SYSTEM.CNF',
  },
  {
    name: 'SLUS-00594.cht',
    size: 6_144,
    system: 'psx',
    of: MGS,
    title: 'Metal Gear Solid (USA)',
    addon: {
      kind: 'cheat',
      format: 'duck-station-cht',
      key_text: 'SLUS-00594',
      via: 'key',
    },
  },
  {
    name: 'textures/SLUS-00594',
    size: 384_000_000,
    folder: true,
    system: 'psx',
    of: MGS,
    title: 'Metal Gear Solid (USA)',
    addon: {
      kind: 'mod',
      format: 'texture-pack',
      key_text: 'SLUS-00594',
      via: 'key',
    },
  },
  // 3DS: a base, an Azahar mod folder and its cheats, by title id.
  {
    name: POKEMON,
    size: 1_720_000_000,
    system: '3ds',
    detail: 'CIA header',
    titleId: { id: '0004000000055D00', source: 'tmd' },
    role: 'base',
    game: 'Pokemon Y (World)',
    region: 'world',
  },
  {
    name: 'load/mods/0004000000055D00',
    size: 46_000_000,
    folder: true,
    system: '3ds',
    of: POKEMON,
    title: 'Pokemon Y',
    addon: {
      kind: 'mod',
      format: 'layered-fs',
      key_text: '0004000000055D00',
      via: 'key',
    },
  },
  {
    name: 'cheats/0004000000055D00.txt',
    size: 3_072,
    system: '3ds',
    of: POKEMON,
    title: 'Pokemon Y',
    addon: {
      kind: 'cheat',
      format: 'ctr-cheats',
      key_text: '0004000000055D00',
      via: 'key',
    },
  },
  // Switch: a base, its update, a romfs mod and cheats by build id.
  {
    name: ACNH,
    size: 6_800_000_000,
    system: 'switch',
    detail: 'NSP, cnmt',
    titleId: { id: ACNH_ID, source: 'cnmt-xml' },
    role: 'base',
    game: 'Animal Crossing New Horizons',
  },
  {
    name: 'Animal Crossing New Horizons [01006F8002326800][v1966080].nsp',
    size: 1_400_000_000,
    system: 'switch',
    detail: 'NSP, cnmt',
    titleId: { id: '01006F8002326800', source: 'cnmt-xml' },
    role: 'update',
    of: ACNH,
    version: '1966080',
    newest: true,
    notes: [{ kind: 'update', of: 'Animal Crossing New Horizons' }],
  },
  {
    name: `atmosphere/contents/${ACNH_ID}/romfs`,
    size: 210_000_000,
    folder: true,
    system: 'switch',
    of: ACNH,
    title: 'Animal Crossing New Horizons',
    addon: {
      kind: 'mod',
      format: 'switch-mod',
      key_text: ACNH_ID,
      via: 'key',
    },
  },
  {
    name: `atmosphere/contents/${ACNH_ID}/cheats/${ACNH_BUILD}.txt`,
    size: 4_096,
    system: 'switch',
    of: ACNH,
    title: 'Animal Crossing New Horizons',
    addon: {
      kind: 'cheat',
      format: 'switch-cheats',
      key_text: `${ACNH_ID} ${ACNH_BUILD}`,
      via: 'key',
    },
  },
  // PS4: an extracted base, its update and DLC folders, its cheats.
  {
    name: GR2,
    size: 38_400_000_000,
    folder: true,
    system: 'ps4',
    detail: 'param.sfo',
    titleId: { id: GR2, source: 'param-sfo' },
    role: 'base',
    game: GR2,
    region: 'usa',
  },
  {
    name: `${GR2}-UPDATE`,
    size: 2_140_000_000,
    folder: true,
    system: 'ps4',
    detail: 'param.sfo',
    titleId: { id: GR2, source: 'param-sfo' },
    role: 'update',
    of: GR2,
    version: '01.03',
    region: 'usa',
    newest: true,
    notes: [{ kind: 'update', of: 'Gravity Rush 2' }],
  },
  {
    name: 'Gravity Rush 2 Raven Pack',
    size: 640_000_000,
    folder: true,
    system: 'ps4',
    detail: 'param.sfo',
    titleId: { id: GR2, source: 'param-sfo' },
    role: 'dlc',
    of: GR2,
    region: 'usa',
    dlcName: 'Raven Pack',
    notes: [{ kind: 'dlc', of: 'Gravity Rush 2' }],
  },
  {
    name: `${GR2}_01.03.json`,
    size: 8_192,
    system: 'ps4',
    of: GR2,
    title: 'Gravity Rush 2',
    addon: {
      kind: 'cheat',
      format: 'shadps4-cheat',
      key_text: `${GR2} 01.03`,
      via: 'key',
    },
  },
  // PS3: a PKG base and its PKG update (RPCS3 installs it).
  {
    name: FOLKLORE,
    size: 4_300_000_000,
    system: 'ps3',
    detail: 'PS3 PKG header',
    titleId: { id: 'BCUS98126', source: 'param-sfo' },
    role: 'base',
    game: 'Folklore (USA)',
    region: 'usa',
  },
  {
    name: 'Folklore (USA) [Update v1.02].pkg',
    size: 96_000_000,
    system: 'ps3',
    detail: 'PS3 PKG header',
    titleId: { id: 'BCUS98126', source: 'param-sfo' },
    role: 'update',
    of: FOLKLORE,
    version: '01.02',
    region: 'usa',
    newest: true,
    notes: [{ kind: 'update', of: 'Folklore' }],
  },
  // Wii U: two regions of one game and a graphic pack listing both.
  {
    name: MK8_US,
    size: 7_200_000_000,
    system: 'wiiu',
    detail: 'meta.xml',
    titleId: { id: '000500001010EC00', source: 'meta-xml' },
    role: 'base',
    game: 'Mario Kart 8 (USA)',
    region: 'usa',
  },
  {
    name: MK8_EU,
    size: 7_200_000_000,
    system: 'wiiu',
    detail: 'meta.xml',
    titleId: { id: '000500001010ED00', source: 'meta-xml' },
    role: 'base',
    game: 'Mario Kart 8 (Europe)',
    region: 'europe',
  },
  unlinked('graphicPacks/MK8 4K Resolution', 12_288, {
    kind: 'addon-unlinked',
    addon: 'mod',
    format: 'cemu-graphic-pack',
    why: { kind: 'several', count: 2 },
  }),
  // RPCS3's shared patch file: patches for many games, never linked.
  unlinked('patch.yml', 2_400_000, {
    kind: 'addon-unlinked',
    addon: 'cheat',
    format: 'rpcs3-patch',
    why: { kind: 'many', count: 214 },
  }),
  // A patch whose game is not known.
  unlinked('Unknown Hack v2.ips', 65_536, {
    kind: 'addon-unlinked',
    addon: 'patch',
    format: 'ips',
    why: { kind: 'no-match' },
  }),
  // A game folder already in the pile, with the user's Mods/ and Cheats/.
  {
    name: XENO_BASE,
    size: 15_400_000_000,
    system: 'switch',
    detail: 'NSP, cnmt',
    titleId: { id: XENO_ID, source: 'cnmt-xml' },
    role: 'base',
    game: XENO,
  },
  {
    name: `${XENO}/Mods/60fps`,
    size: 1_200_000,
    folder: true,
    system: 'switch',
    of: XENO_BASE,
    title: 'Xenoblade Chronicles 3',
    addon: {
      kind: 'mod',
      format: 'exefs-patches',
      key_text: XENO_ID,
      via: 'folder',
    },
  },
  {
    name: `${XENO}/Cheats/A1B2C3D4E5F60718.txt`,
    size: 2_048,
    system: 'switch',
    of: XENO_BASE,
    title: 'Xenoblade Chronicles 3',
    addon: {
      kind: 'cheat',
      format: 'switch-cheats',
      key_text: `${XENO_ID} A1B2C3D4E5F60718`,
      via: 'folder',
    },
  },
];

const SAME: MockRule = { to: 'same' };
const sys = (path: string, note?: TranslateNote): MockRule => ({
  to: { kind: 'system', path },
  ...(note && { note }),
});
const data = (path: string): MockRule => ({
  to: { kind: 'emulator-data', path },
});
const lib = (note?: TranslateNote): MockRule => ({
  to: { kind: 'library' },
  ...(note && { note }),
});
const install = (emulator: EmulatorId): MockRule =>
  lib({ kind: 'install-in-emulator', emulator });

/** What each emulator's translator does with an entry of this pile, by
 * its pile name (Rust `translate/*.rs`). */
export const MODS_TRANSLATIONS: Readonly<
  Record<string, Partial<Record<EmulatorId, MockRule>>>
> = {
  [SD3]: { retroarch: SAME },
  'Seiken Densetsu 3 (Japan) [T-En by DeJap].bps': {
    retroarch: sys('Seiken Densetsu 3 (Japan).bps'),
  },
  [TMZ]: { retroarch: SAME },
  'TMZ English v1.01.ips': {
    retroarch: sys('Tengai Makyou Zero (Japan).ips'),
  },
  [FF5]: { retroarch: SAME },
  'FF5 Restored Script.ips': {
    retroarch: lib({
      kind: 'not-supported',
      emulator: 'retroarch',
      why: 'several-patches',
    }),
  },
  'FF5 Sound Restoration.ips': {
    retroarch: lib({
      kind: 'not-supported',
      emulator: 'retroarch',
      why: 'several-patches',
    }),
  },
  [MOTHER]: { retroarch: SAME },
  'Mother (Japan) [T-En].bps': {
    retroarch: lib({ kind: 'header-mismatch', needs: 'needs-headerless' }),
  },
  [MGS]: { duckstation: SAME, retroarch: SAME },
  'SLUS-00594.cht': { duckstation: data('cheats/SLUS-00594.cht') },
  'textures/SLUS-00594': { duckstation: data('textures/SLUS-00594') },
  [POKEMON]: { azahar: SAME },
  'load/mods/0004000000055D00': {
    azahar: data('load/mods/0004000000055D00'),
  },
  'cheats/0004000000055D00.txt': {
    azahar: data('cheats/0004000000055D00.txt'),
  },
  [ACNH]: { ryujinx: SAME, 'yuzu-family': SAME },
  'Animal Crossing New Horizons [01006F8002326800][v1966080].nsp': {
    ryujinx: sys(
      'Updates/Animal Crossing New Horizons [01006F8002326800][v1966080].nsp',
    ),
    'yuzu-family': sys(
      'Updates/Animal Crossing New Horizons [01006F8002326800][v1966080].nsp',
    ),
  },
  [`atmosphere/contents/${ACNH_ID}/romfs`]: {
    ryujinx: data(`mods/contents/${ACNH_ID}/romfs`),
    'yuzu-family': data(`load/${ACNH_ID}/romfs`),
  },
  [`atmosphere/contents/${ACNH_ID}/cheats/${ACNH_BUILD}.txt`]: {
    ryujinx: data(`mods/contents/${ACNH_ID}/cheats/${ACNH_BUILD}.txt`),
    'yuzu-family': data(`load/${ACNH_ID}/cheats/cheats/${ACNH_BUILD}.txt`),
  },
  [GR2]: { shadps4: SAME },
  [`${GR2}-UPDATE`]: { shadps4: sys(`${GR2}-UPDATE`) },
  'Gravity Rush 2 Raven Pack': {
    shadps4: sys(`addcont/${GR2}/Gravity Rush 2 Raven Pack`, {
      kind: 'point-emulator-here',
      emulator: 'shadps4',
      folder: 'addcont',
    }),
  },
  [`${GR2}_01.03.json`]: { shadps4: data(`cheats/${GR2}_01.03.json`) },
  [FOLKLORE]: { rpcs3: SAME },
  'Folklore (USA) [Update v1.02].pkg': { rpcs3: install('rpcs3') },
  [MK8_US]: { cemu: SAME },
  [MK8_EU]: { cemu: SAME },
  [XENO_BASE]: { ryujinx: SAME, 'yuzu-family': SAME },
  [`${XENO}/Mods/60fps`]: {
    ryujinx: data(`mods/contents/${XENO_ID}/60fps`),
    'yuzu-family': data(`load/${XENO_ID}/60fps`),
  },
  [`${XENO}/Cheats/A1B2C3D4E5F60718.txt`]: {
    ryujinx: data(`mods/contents/${XENO_ID}/cheats/A1B2C3D4E5F60718.txt`),
    'yuzu-family': data(`load/${XENO_ID}/cheats/cheats/A1B2C3D4E5F60718.txt`),
  },
};
