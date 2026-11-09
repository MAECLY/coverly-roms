/**
 * The game folders pile of the browser mock (`?mockPick=gamefolders`,
 * game folders spec §9.3): synthetic names and sizes, no real data, no
 * keys. Each file says what the core would read from its header (role,
 * version, region) and, for a base, the `<Game>` folder name it gets;
 * `mockOrganize.ts` turns them into plan items, nested or flat by the
 * layout's resolved option.
 */
import type {
  GameRole,
  ItemNote,
  LeftReason,
  Region,
  TitleIdRef,
} from '../types';

export const MOCK_GAMEFOLDERS = '/Users/demo/GameFolders';

export interface GameFolderFile {
  name: string;
  size: number;
  system?: string;
  /** The header clue (`why`). */
  detail?: string;
  titleId?: TitleIdRef;
  contentId?: string;
  /** What it is to its game; absent: an ordinary ROM. */
  role?: GameRole;
  /** A child's base, by its file name in this pile. */
  of?: string;
  /** A base's or demo's `<Game>` folder name. */
  game?: string;
  /** Display version (D11). */
  version?: string;
  region?: Region;
  /** Highest update version of its base here (D24). */
  newest?: true;
  /** A DLC's `<DLC name>`. */
  dlcName?: string;
  /** A demo's base title, by name only (D12). */
  demoOf?: string;
  notes?: ItemNote[];
  left?: LeftReason;
}

const KEYS: LeftReason = { kind: 'key-material' };
const XENO = 'Xenoblade Chronicles 3 (World)';

const rockBandDlc: GameFolderFile[] = Array.from({ length: 25 }, (_, i) => {
  const n = String(i + 1).padStart(2, '0');
  return {
    name: `Rock Band 4 - Song ${n}.pkg`,
    size: 48_000_000 + i * 1_000_000,
    system: 'ps4',
    detail: 'PS4 PKG header',
    titleId: { id: 'CUSA02084', source: 'param-sfo' },
    role: 'dlc',
    of: 'Rock Band 4 (USA).pkg',
    region: 'usa',
    dlcName: `Song ${n}`,
    notes: [{ kind: 'dlc', of: 'Rock Band 4' }],
  };
});

export const GAME_FOLDER_FILES: readonly GameFolderFile[] = [
  // 3DS: a base, its update and two DLC (one named by its id only).
  {
    name: 'Pokemon Y.cia',
    size: 1_720_000_000,
    system: '3ds',
    detail: 'CIA header',
    titleId: { id: '0004000000055D00', source: 'tmd' },
    role: 'base',
    game: 'Pokemon Y (World)',
    region: 'world',
  },
  {
    name: '0004000E00055D00.cia',
    size: 412_000_000,
    system: '3ds',
    detail: 'CIA header',
    titleId: { id: '0004000E00055D00', source: 'tmd' },
    role: 'update',
    of: 'Pokemon Y.cia',
    version: '1.1.0',
    region: 'world',
    newest: true,
    notes: [{ kind: 'update', of: 'Pokemon Y' }],
  },
  {
    name: 'Pokemon Y - Extra Pack (USA).cia',
    size: 28_000_000,
    system: '3ds',
    detail: 'CIA header',
    titleId: { id: '0004008C00055D00', source: 'tmd' },
    role: 'dlc',
    of: 'Pokemon Y.cia',
    region: 'world',
    dlcName: 'Extra Pack',
    notes: [{ kind: 'dlc', of: 'Pokemon Y' }],
  },
  {
    name: 'Pokemon Y [DLC2].cia',
    size: 12_000_000,
    system: '3ds',
    detail: 'CIA header',
    titleId: { id: '0004008C00055D01', source: 'tmd' },
    role: 'dlc',
    of: 'Pokemon Y.cia',
    region: 'world',
    dlcName: 'DLC 0004008C00055D01',
    notes: [{ kind: 'dlc', of: 'Pokemon Y' }],
  },
  // PS4: a base, an update and a DLC.
  {
    name: 'Gravity Rush 2 (USA).pkg',
    size: 38_400_000_000,
    system: 'ps4',
    detail: 'PS4 PKG header',
    titleId: { id: 'CUSA04943', source: 'param-sfo' },
    role: 'base',
    game: 'Gravity Rush 2 (USA)',
    region: 'usa',
  },
  {
    name: 'Gravity Rush 2 (USA) [Update].pkg',
    size: 2_140_000_000,
    system: 'ps4',
    detail: 'PS4 PKG header',
    titleId: { id: 'CUSA04943', source: 'param-sfo' },
    role: 'update',
    of: 'Gravity Rush 2 (USA).pkg',
    version: '01.05',
    region: 'usa',
    newest: true,
    notes: [{ kind: 'update', of: 'Gravity Rush 2' }],
  },
  {
    name: 'Gravity Rush 2 (USA) [DLC].pkg',
    size: 640_000_000,
    system: 'ps4',
    detail: 'PS4 PKG header',
    titleId: { id: 'CUSA04943', source: 'param-sfo' },
    role: 'dlc',
    of: 'Gravity Rush 2 (USA).pkg',
    region: 'usa',
    dlcName: 'Raven Pack',
    notes: [{ kind: 'dlc', of: 'Gravity Rush 2' }],
  },
  // PS4: a game with 25 DLC (the «y 5 más» cap).
  {
    name: 'Rock Band 4 (USA).pkg',
    size: 9_800_000_000,
    system: 'ps4',
    detail: 'PS4 PKG header',
    titleId: { id: 'CUSA02084', source: 'param-sfo' },
    role: 'base',
    game: 'Rock Band 4 (USA)',
    region: 'usa',
  },
  ...rockBandDlc,
  // PS3: a base PKG with its licence beside it, and its demo.
  {
    name: 'Resistance (USA).pkg',
    size: 6_200_000_000,
    system: 'ps3',
    detail: 'PS3 PKG header',
    titleId: { id: 'BLUS30001', source: 'param-sfo' },
    role: 'base',
    game: 'Resistance (USA)',
    region: 'usa',
  },
  { name: 'BLUS30001.rap', size: 16, left: KEYS },
  {
    name: 'Resistance (USA) (Demo).pkg',
    size: 820_000_000,
    system: 'ps3',
    detail: 'PS3 PKG header',
    titleId: { id: 'NPUA80001', source: 'param-sfo' },
    role: 'demo',
    game: 'Resistance (USA) (Demo)',
    region: 'usa',
    demoOf: 'Resistance (USA)',
    notes: [{ kind: 'demo-of', of: 'Resistance (USA)' }],
  },
  // Switch: a base and a DataPatch (DLC of the base, D14 b).
  {
    name: 'Mario Kart 8 Deluxe [0100152000022000].nsp',
    size: 7_100_000_000,
    system: 'switch',
    detail: 'NSP, cnmt',
    titleId: { id: '0100152000022000', source: 'cnmt-xml' },
    role: 'base',
    game: 'Mario Kart 8 Deluxe',
  },
  {
    name: 'Mario Kart 8 Deluxe [0100152000023001][DataPatch].nsp',
    size: 96_000_000,
    system: 'switch',
    detail: 'NSP, cnmt',
    titleId: { id: '0100152000023001', source: 'cnmt-xml' },
    role: 'dlc',
    of: 'Mario Kart 8 Deluxe [0100152000022000].nsp',
    dlcName: 'DLC 0100152000023001',
    notes: [{ kind: 'dlc', of: 'Mario Kart 8 Deluxe' }],
  },
  // A game folder already in the pile: its Updates/ and DLC/ follow the
  // base, its Mods/ and Saves/ stay where they are for now.
  {
    name: `${XENO}/${XENO}.nsp`,
    size: 15_400_000_000,
    system: 'switch',
    detail: 'NSP, cnmt',
    titleId: { id: '010074F013262000', source: 'cnmt-xml' },
    role: 'base',
    game: XENO,
  },
  {
    name: `${XENO}/Updates/${XENO} [v2].nsp`,
    size: 1_900_000_000,
    system: 'switch',
    detail: 'NSP, cnmt',
    titleId: { id: '010074F013262800', source: 'cnmt-xml' },
    role: 'update',
    of: `${XENO}/${XENO}.nsp`,
    version: '131072',
    newest: true,
    notes: [{ kind: 'update', of: 'Xenoblade Chronicles 3' }],
  },
  {
    name: `${XENO}/DLC/${XENO} - Expansion Pass.nsp`,
    size: 4_200_000_000,
    system: 'switch',
    detail: 'NSP, cnmt',
    titleId: { id: '010074F013263001', source: 'cnmt-xml' },
    role: 'dlc',
    of: `${XENO}/${XENO}.nsp`,
    dlcName: 'Expansion Pass',
    notes: [{ kind: 'dlc', of: 'Xenoblade Chronicles 3' }],
  },
  {
    name: `${XENO}/Mods/romfs.bin`,
    size: 22_000_000,
    left: { kind: 'game-folder-content', folder: 'mods', of: XENO },
  },
  {
    name: `${XENO}/Saves/save.dat`,
    size: 4_000_000,
    left: { kind: 'game-folder-content', folder: 'saves', of: XENO },
  },
  // Wii U: an update whose game is neither here nor in the library.
  {
    name: 'Splatoon (USA) [Update v288].wua',
    size: 1_100_000_000,
    system: 'wiiu',
    detail: 'meta.xml',
    titleId: { id: '0005000E10176900', source: 'meta-xml' },
    role: 'update',
    version: '288',
    region: 'usa',
    notes: [{ kind: 'update', of: 'Splatoon' }],
  },
  // Retro revisions: two versions of one game, loose files.
  {
    name: 'Sonic the Hedgehog (USA, Europe).md',
    size: 524_288,
    system: 'megadrive',
    detail: 'SEGA header',
  },
  {
    name: 'Sonic the Hedgehog (USA, Europe) (Rev 1).md',
    size: 524_288,
    system: 'megadrive',
    detail: 'SEGA header',
  },
  // Never games: firmware, a shader cache, a global key file.
  {
    name: 'PS3UPDAT.PUP',
    size: 205_000_000,
    left: { kind: 'not-a-rom', what: 'console-system' },
  },
  {
    name: 'shadercache/0100152000022000.bin',
    size: 3_400_000,
    left: { kind: 'not-a-rom', what: 'emulator-cache' },
  },
  { name: 'prod.keys', size: 8_192, left: KEYS },
];
