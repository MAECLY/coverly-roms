/**
 * The piles of the adjustments batch (spec §9.4), as plain data the mock
 * organiser turns into pile files. Names and sizes are synthetic; no
 * catalogue, key or game byte is in here.
 *
 * - `huge` (`?mockPick=huge`, `MOCK_HUGE`): three PS4 game folders of 38,
 *   52 and 61 GB and one zipped PS3 disc (a folder game extracted out of
 *   its archive, always asked), plus two PS2 discs that make the small
 *   rest. Its target is another drive, «Juegos», with 120 GB free, so the
 *   automatic mode copies, each big item is asked on its own and the
 *   three PS4 games cannot all fit.
 * - `switch` (`MOCK_SWITCH`): NSP and XCI with and without `[0100…]`
 *   named from the mock catalogue (`MOCK_SWITCH_TITLES`), an XCI without
 *   an id resolved by its NCA id, a loose `.nca`, an `.nro` and one title
 *   nothing names («Cifrado»).
 * - `odd` (`MOCK_ODD`): one loose file per row of spec §8.2.
 * - `discs` (`MOCK_DISCS`): Saturn and GameCube sets with their `.m3u`,
 *   a Sega CD set by name only (review) and a set missing a disc.
 * - `dupes-title` (`MOCK_DUPES_TITLE`): two NSPs with the same title id
 *   and version, two `CUSA00900` folders with equal trees; the copies are
 *   set aside through the duplicates step and undo brings them back.
 */
import type {
  IdLevel,
  ItemNote,
  LeftReason,
  TitleIdRef,
  TitleSource,
} from '../types';
import type { MockArchiveEntry } from './discFixture';

export const MOCK_HUGE = '/Users/demo/Huge';
/** Where the huge pile goes: another drive, so it is copied. */
export const MOCK_HUGE_TARGET = '/Volumes/Juegos';
/** Free space the mock reports on `MOCK_HUGE_TARGET`. */
export const MOCK_HUGE_FREE = 120_000_000_000;

/** A console game folder of an adjustments pile. */
export interface AdjustFolderGame {
  name: string;
  size: number;
  system: string;
  /** The `why` detail of its header. */
  detail: string;
  titleId: TitleIdRef;
  contentId?: string;
}

/** A folder game zipped whole (adjustments D20–D23): extracted on apply. */
export interface AdjustArchiveGame {
  name: string;
  /** The archive's own size. */
  size: number;
  system: string;
  detail: string;
  titleId: TitleIdRef;
  /** The game folder at the archive's root. */
  root: string;
  /** File entries in archive order, under `root`. */
  entries: MockArchiveEntry[];
}

/** A loose disc image of an adjustments pile. */
export interface AdjustDisc {
  name: string;
  size: number;
  system: string;
  detail: string;
}

/**
 * Any other item of an adjustments pile, as the core would identify it.
 * Mirrors the mock organiser's pile file; only what these piles need.
 */
export interface AdjustFile {
  name: string;
  size: number;
  system?: string;
  method?: IdLevel;
  confidence?: number;
  /** The `why` detail of its first clue. */
  detail?: string;
  /** Other systems: the item goes to review. */
  alternatives?: [string, number][];
  notes?: ItemNote[];
  left?: LeftReason;
  /** A catalogue name (DAT or the Switch catalogue). */
  catalogue?: string;
  titleId?: TitleIdRef;
  /** Set when the Switch catalogue named it (`titledb` or `nlib`). */
  titleSource?: TitleSource;
  /** A game that is a folder, moved whole. */
  folder?: true;
  /** An update or DLC: the base it is listed after. */
  baseOf?: string;
  /** A loose multi-disc set (D24): the other discs, in disc order. The
   * item is one `set` whose primary is `name`. */
  discs?: string[];
  /** The `.m3u` the set gets, beside its discs. */
  playlist?: string;
  /** How the duplicates step matched a copy (`left: duplicate`, D19). */
  dupBasis?: 'title-id' | 'folder-tree';
}

export interface AdjustPile {
  path: string;
  folders: AdjustFolderGame[];
  archives: AdjustArchiveGame[];
  discs: AdjustDisc[];
  /** Everything else, already identified. */
  files?: AdjustFile[];
}

const ps4 = (
  name: string,
  size: number,
  id: string,
  content: string,
): AdjustFolderGame => ({
  name,
  size,
  system: 'ps4',
  detail: 'sce_sys/param.sfo',
  titleId: { id, source: 'param-sfo' },
  contentId: `UP9000-${id}_00-${content}`,
});

export const HUGE_PILE: AdjustPile = {
  path: MOCK_HUGE,
  folders: [
    ps4('Bloodborne', 52_400_000_000, 'CUSA00900', 'BLOODBORNE000000'),
    ps4('Ghost of Tsushima', 61_200_000_000, 'CUSA11456', 'GHOSTOFTSUSHIMA0'),
    ps4(
      'The Last of Us Part II',
      38_300_000_000,
      'CUSA10249',
      'THELASTOFUS2PT20',
    ),
  ],
  archives: [
    {
      name: "Demon's Souls (BLUS-30443).zip",
      size: 6_870_000_000,
      system: 'ps3',
      detail: 'PS3_GAME/PARAM.SFO',
      titleId: { id: 'BLUS30443', source: 'param-sfo' },
      root: 'BLUS30443',
      entries: [
        {
          index: 1,
          name: 'BLUS30443/PS3_DISC.SFB',
          size: 1_024,
          crc32: 0x5f1e2d3c,
        },
        {
          index: 3,
          name: 'BLUS30443/PS3_GAME/PARAM.SFO',
          size: 1_412,
          crc32: 0x1a2b3c4d,
        },
        {
          index: 4,
          name: 'BLUS30443/PS3_GAME/ICON0.PNG',
          size: 61_440,
          crc32: 0x2b3c4d5e,
        },
        {
          index: 6,
          name: 'BLUS30443/PS3_GAME/USRDIR/EBOOT.BIN',
          size: 24_117_248,
          crc32: 0x3c4d5e6f,
        },
        {
          index: 7,
          name: 'BLUS30443/PS3_GAME/USRDIR/dvdroot_ps3.bdt',
          size: 7_386_000_000,
          crc32: 0x4d5e6f70,
        },
      ],
    },
  ],
  discs: [
    {
      name: 'Shadow of the Colossus (USA).iso',
      size: 2_940_000_000,
      system: 'ps2',
      detail: 'SYSTEM.CNF BOOT2',
    },
    {
      name: 'Okami (USA).iso',
      size: 3_980_000_000,
      system: 'ps2',
      detail: 'SYSTEM.CNF BOOT2',
    },
  ],
};

export const MOCK_SWITCH = '/Users/demo/Switch';
export const MOCK_ODD = '/Users/demo/Odd';
export const MOCK_DISCS = '/Users/demo/Discs';
export const MOCK_DUPES_TITLE = '/Users/demo/DupesTitle';

/**
 * The mock's Switch name catalogue (`blawar/titledb`, reduced to id →
 * name): a handful of made-up rows, never a copy of the real list.
 */
export const MOCK_SWITCH_TITLES: Readonly<Record<string, string>> = {
  '01007EF00011E000': 'The Legend of Zelda Breath of the Wild',
  '0100000000010000': 'Super Mario Odyssey',
  '0100152000022000': 'Mario Kart 8 Deluxe',
};
/** The NCA id of Super Mario Odyssey's meta NCA in the mock `ncas.json`. */
export const MOCK_ODYSSEY_NCA = '4b0e7a1f2c3d4e5f60718293a4b5c6d7';

const SWITCH_HEAD = (detail: string): Partial<AdjustFile> => ({
  system: 'switch',
  method: 'header',
  confidence: 0.95,
  detail,
});
const named = (
  id: string,
  source: TitleIdRef['source'],
): Partial<AdjustFile> => ({
  catalogue: MOCK_SWITCH_TITLES[id]!,
  titleId: { id, source },
  titleSource: 'titledb',
});

export const SWITCH_PILE: AdjustPile = {
  path: MOCK_SWITCH,
  folders: [],
  archives: [],
  discs: [],
  files: [
    {
      name: 'Zelda BotW [01007EF00011E000][v0].nsp',
      size: 14_300_000_000,
      ...SWITCH_HEAD('PFS0, ticket'),
      ...named('01007EF00011E000', 'ticket-name'),
    },
    {
      name: 'Zelda BotW Update [01007EF00011E800][v786432].nsp',
      size: 1_200_000_000,
      ...SWITCH_HEAD('PFS0, ticket'),
      titleId: { id: '01007EF00011E800', source: 'ticket-name' },
      notes: [{ kind: 'update', of: 'The Legend of Zelda Breath of the Wild' }],
      baseOf: 'Zelda BotW [01007EF00011E000][v0].nsp',
    },
    {
      name: 'MK8D.xci',
      size: 7_100_000_000,
      ...SWITCH_HEAD('HEAD, cnmt.xml'),
      ...named('0100152000022000', 'cnmt-xml'),
    },
    {
      // No `[0100…]`: the secure partition's NCA names, through the
      // catalogue's `ncas` table (D6).
      name: 'Cartridge.xci',
      size: 5_600_000_000,
      ...SWITCH_HEAD('HEAD, secure HFS0'),
      ...named('0100000000010000', 'nca-catalog'),
    },
    {
      name: `${MOCK_ODYSSEY_NCA}.nca`,
      size: 3_400_000_000,
      left: {
        kind: 'game-part',
        part: 'nca',
        system: 'switch',
        title_id: '0100000000010000',
      },
    },
    {
      name: 'hbmenu.nro',
      size: 1_480_000,
      ...SWITCH_HEAD('NRO0, NACP hbmenu'),
      notes: [{ kind: 'homebrew' }],
    },
    {
      // Nothing names it: no header title, not in the catalogue, a file
      // name that is only its id (D11).
      name: '0100F2C0115B6000.nsp',
      size: 2_100_000_000,
      ...SWITCH_HEAD('PFS0'),
      titleId: { id: '0100F2C0115B6000', source: 'file-name' },
      notes: [{ kind: 'encrypted-name' }],
    },
  ],
};

const part = (
  name: string,
  size: number,
  system: string,
  kind: Extract<LeftReason, { kind: 'game-part' }>['part'],
  titleId: string | null = null,
): AdjustFile => ({
  name,
  size,
  left: { kind: 'game-part', part: kind, system, title_id: titleId },
});
const homebrew = (
  name: string,
  size: number,
  system: string,
  detail: string,
): AdjustFile => ({
  name,
  size,
  system,
  method: 'header',
  confidence: 0.95,
  detail,
  notes: [{ kind: 'homebrew' }],
});

/** One loose file per row of spec §8.2, as the core classifies it. */
export const ODD_PILE: AdjustPile = {
  path: MOCK_ODD,
  folders: [],
  archives: [],
  discs: [],
  files: [
    homebrew('Checkpoint.nro', 3_900_000, 'switch', 'NRO0, NACP Checkpoint'),
    part('main.nso', 18_200_000, 'switch', 'exe-fs'),
    part('main.npdm', 1_200, 'switch', 'exe-fs', '0100000000010000'),
    part('9a3f1c2d4e5b6a7980a1b2c3d4e5f607.nca', 820_000_000, 'switch', 'nca'),
    homebrew('Swiss.dol', 2_600_000, 'gamecube', 'DOL, 7 text sections'),
    homebrew('boot.dol', 1_900_000, 'wii', 'DOL, HID4 write'),
    homebrew('XBMC4Xbox.xbe', 6_400_000, 'xbox', 'XBEH, hard disk only'),
    part('xam.xex', 2_300_000, 'xbox360', 'module'),
    {
      name: 'Halo 3 TU3.xex',
      size: 41_000_000,
      system: 'xbox360',
      method: 'header',
      confidence: 0.95,
      detail: 'XEX2, PATCH_DELTA',
      titleId: { id: '4D5307E6', source: 'xex' },
      notes: [{ kind: 'update', of: '4D5307E6' }],
    },
    homebrew('Homebrew Launcher.rpx', 4_100_000, 'wiiu', 'ELF, OSABI 0xCA'),
    part('libgui.rpl', 900_000, 'wiiu', 'library'),
    part('title.tmd', 2_548, 'wiiu', 'title-meta', '0005000010101C00'),
    part(
      'PCSE00120/game.psvimg',
      1_300_000_000,
      'psvita',
      'backup',
      'PCSE00120',
    ),
    homebrew('VitaShell.self', 2_200_000, 'psvita', 'SCE, authid homebrew'),
    part('eboot.bin', 31_000_000, 'ps3', 'exe-fs'),
    {
      name: 'Crash Bandicoot.pbp',
      size: 410_000_000,
      system: 'psx',
      method: 'header',
      confidence: 0.95,
      detail: 'PBP, PSISOIMG0000',
      titleId: { id: 'SCUS94900', source: 'pbp' },
    },
    part('kernel_util.prx', 120_000, 'psp', 'module'),
    homebrew('FBI.3dsx', 3_100_000, '3ds', '3DSX'),
  ],
};

const disc = (
  name: string,
  system: string,
  detail: string,
  discs: string[],
  extra: Partial<AdjustFile> = {},
): AdjustFile => ({
  name,
  size: 680_000_000 * (discs.length + 1),
  system,
  method: 'header',
  confidence: 0.95,
  detail,
  discs,
  ...extra,
});

/** Loose multi-disc games (D24). */
export const DISCS_PILE: AdjustPile = {
  path: MOCK_DISCS,
  folders: [],
  archives: [],
  discs: [],
  files: [
    disc(
      'Panzer Dragoon Saga (USA) (Disc 1).chd',
      'saturn',
      'SEGA SEGASATURN, CD-1/4',
      [
        'Panzer Dragoon Saga (USA) (Disc 2).chd',
        'Panzer Dragoon Saga (USA) (Disc 3).chd',
        'Panzer Dragoon Saga (USA) (Disc 4).chd',
      ],
      { playlist: 'Panzer Dragoon Saga (USA).m3u' },
    ),
    disc(
      'Tales of Symphonia (USA) (Disc 1).rvz',
      'gamecube',
      'GALE01, disc 0x00',
      ['Tales of Symphonia (USA) (Disc 2).rvz'],
      { playlist: 'Tales of Symphonia (USA).m3u' },
    ),
    disc(
      'Grandia (Japan) (Disc 1).chd',
      'saturn',
      'SEGA SEGASATURN, CD-1/2',
      [],
      {
        playlist: 'Grandia (Japan).m3u',
        notes: [{ kind: 'missing-disc', n: 2 }],
      },
    ),
    disc(
      'Lunar - The Silver Star (USA) (Disc 1).chd',
      'segacd',
      'CHD',
      ['Lunar - The Silver Star (USA) (Disc 2).chd'],
      {
        method: 'extension',
        confidence: 0.6,
        alternatives: [['megadrive', 0.3]],
        notes: [{ kind: 'disc-set' }],
      },
    ),
  ],
};

const DUP_ZELDA = 'Zelda [01007EF00011E000][v0].nsp';
const DUP_BLOODBORNE = 'CUSA00900';

/** Copies matched by title id and version, and by folder tree (D19). */
export const DUPES_TITLE_PILE: AdjustPile = {
  path: MOCK_DUPES_TITLE,
  folders: [],
  archives: [],
  discs: [],
  files: [
    {
      name: DUP_ZELDA,
      size: 14_300_000_000,
      ...SWITCH_HEAD('PFS0, ticket'),
      ...named('01007EF00011E000', 'ticket-name'),
    },
    {
      name: 'Zelda (copia) [01007EF00011E000][v0].nsp',
      size: 14_300_000_000,
      ...SWITCH_HEAD('PFS0, ticket'),
      ...named('01007EF00011E000', 'ticket-name'),
      left: { kind: 'duplicate', of: DUP_ZELDA },
      dupBasis: 'title-id',
    },
    {
      name: DUP_BLOODBORNE,
      size: 48_000_000_000,
      system: 'ps4',
      method: 'header',
      confidence: 0.95,
      detail: 'sce_sys/param.sfo',
      titleId: { id: 'CUSA00900', source: 'param-sfo' },
      folder: true,
    },
    {
      name: 'Bloodborne (CUSA00900)',
      size: 48_000_000_000,
      system: 'ps4',
      method: 'header',
      confidence: 0.95,
      detail: 'sce_sys/param.sfo',
      titleId: { id: 'CUSA00900', source: 'param-sfo' },
      folder: true,
      left: { kind: 'duplicate', of: DUP_BLOODBORNE },
      dupBasis: 'folder-tree',
    },
  ],
};

/** Every adjustments pile the mock organiser serves. */
export const ADJUST_PILES: readonly AdjustPile[] = [
  HUGE_PILE,
  SWITCH_PILE,
  ODD_PILE,
  DISCS_PILE,
  DUPES_TITLE_PILE,
];
