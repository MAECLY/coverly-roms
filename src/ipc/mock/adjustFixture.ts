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
 */
import type { TitleIdRef } from '../types';
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

export interface AdjustPile {
  path: string;
  folders: AdjustFolderGame[];
  archives: AdjustArchiveGame[];
  discs: AdjustDisc[];
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

/** Every adjustments pile the mock organiser serves. */
export const ADJUST_PILES: readonly AdjustPile[] = [HUGE_PILE];
