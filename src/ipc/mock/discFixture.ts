/**
 * The three disc archives of the owner's download pile (archives spec §1),
 * as `7zz l -slt` lists them: 7z files named `.zip`, each with one game
 * folder holding the tracks and the sheet(s). Names, sizes and CRCs are
 * the real listing (no disc bytes); the mock plans them like Rust does.
 */

/** One file entry of an archive, in the archive's own order. */
export interface MockArchiveEntry {
  /** Position in the archive (directory entries count too). */
  index: number;
  /** `/`-separated, as stored. */
  name: string;
  size: number;
  crc32: number | null;
}

/** A disc game inside an archive, as the disc readers identify it. */
export interface MockDiscArchive {
  /** The archive's file name in the pile. */
  name: string;
  /** The archive's own size. */
  size: number;
  system: string;
  /** Normalised serial (`SLUS-00862`). */
  serial: string;
  source: 'system-cnf' | 'ip-bin';
  /** The `why` detail. */
  detail: string;
  /** What the magic says the container is (the extension says zip). */
  actual: '7z';
  /** The sheet the item is named after (`.gdi` first on Dreamcast). */
  primary: string;
  /** Solid with the sheets last: read on apply, not in the dry run. */
  sheetDeferred: boolean;
  entries: MockArchiveEntry[];
}

/** `[size, crc32]` per track, in track order. */
type Tracks = readonly (readonly [number, number])[];

function disc(
  folder: string,
  tracks: Tracks,
  sheets: readonly (readonly [string, number, number])[],
): MockArchiveEntry[] {
  // Index 0 is the game folder itself, as in the real listings.
  const two = (n: number) => String(n).padStart(2, '0');
  const files: MockArchiveEntry[] = tracks.map(([size, crc32], i) => ({
    index: i + 1,
    name: `${folder}/${folder} (Track ${two(i + 1)}).bin`,
    size,
    crc32,
  }));
  sheets.forEach(([ext, size, crc32], i) => {
    files.push({
      index: tracks.length + 1 + i,
      name: `${folder}/${folder}.${ext}`,
      size,
      crc32,
    });
  });
  return files;
}

const PS_FOLDER = 'Next Tetris, The (USA)';
const DC_FOLDER = 'Next Tetris, The - On-Line Edition (USA)';
const SAT_FOLDER = 'Tetris Plus (USA)';

export const MOCK_DISC_ARCHIVES: readonly MockDiscArchive[] = [
  {
    // 7z LZMA:26, 12 blocks (not solid): the sheet is read in the dry run.
    name: 'Next Tetris, The.zip',
    size: 355_281_551,
    system: 'psx',
    serial: 'SLUS-00862',
    source: 'system-cnf',
    detail: 'SYSTEM.CNF BOOT cdrom:\\SLUS_008.62;1 + licence text',
    actual: '7z',
    primary: `${PS_FOLDER}/${PS_FOLDER}.cue`,
    sheetDeferred: false,
    entries: disc(
      PS_FOLDER,
      [
        [34508544, 0x8b6f8183],
        [31592064, 0x7a0a30b7],
        [22391040, 0x3d51ca5d],
        [20227200, 0x216a95dd],
        [26561136, 0xa5f7eb7e],
        [42455952, 0x19a53bd7],
        [42220752, 0xcda9e912],
        [58868208, 0x8e3750d6],
        [42437136, 0x3d25596b],
        [39560640, 0x06d63f64],
        [44925552, 0x495c5a02],
      ],
      [['cue', 1269, 0xc16424e9]],
    ),
  },
  {
    // 7z LZMA2:24, solid: both sheets sit after 1.19 GB of tracks.
    name: 'Next Tetris, The_ On-Line Edition.zip',
    size: 381_421_246,
    system: 'dreamcast',
    serial: 'T-40214N',
    source: 'ip-bin',
    detail: 'IP.BIN SEGA SEGAKATANA T-40214N',
    actual: '7z',
    primary: `${DC_FOLDER}/${DC_FOLDER}.gdi`,
    sheetDeferred: true,
    entries: disc(
      DC_FOLDER,
      [
        [3205776, 0xda4cad17],
        [2097984, 0xe31a8381],
        [764258880, 0xf162fad8],
        [44923200, 0x0a83d790],
        [31592064, 0x1ee86641],
        [22388688, 0x649273f5],
        [20227200, 0x10de07ec],
        [26561136, 0x3eff16a8],
        [42453600, 0xe97a7658],
        [42220752, 0x53a0564f],
        [58868208, 0xeef5bfd9],
        [42432432, 0x8a6fccc8],
        [39560640, 0x7368054f],
        [50274000, 0x73a05b17],
      ],
      [
        ['cue', 1907, 0x64351f2b],
        ['gdi', 1096, 0x607d6cff],
      ],
    ),
  },
  {
    // 7z LZMA2:24, solid: the sheet is the last of 52 entries.
    name: 'Tetris Plus.zip',
    size: 478_731_748,
    system: 'saturn',
    serial: 'T-5704H',
    source: 'ip-bin',
    detail: 'IP.BIN SEGA SEGASATURN T-5704H',
    actual: '7z',
    primary: `${SAT_FOLDER}/${SAT_FOLDER}.cue`,
    sheetDeferred: true,
    entries: disc(
      SAT_FOLDER,
      [
        [6743184, 0xd771bc8e],
        [5644800, 0x25de8a7f],
        [5644800, 0x7d483674],
        [12171600, 0x685fca74],
        [11642400, 0x76580cf0],
        [15276240, 0xfb7504ee],
        [13813296, 0x6086ae58],
        [12966576, 0xad16c686],
        [14394240, 0x4ad4eae6],
        [15894816, 0x92c12bc7],
        [12665520, 0xd6555664],
        [26020176, 0xfb4dd1c9],
        [22755600, 0xeae57ed8],
        [8114400, 0x62db3ee7],
        [8344896, 0x0b83931c],
        [10584000, 0x89f2a71b],
        [9878400, 0xc66a39c4],
        [21520800, 0x72973d37],
        [11995200, 0x3e6c8fad],
        [11501280, 0x73a05908],
        [10972080, 0x728e87a8],
        [20286000, 0x7f94005f],
        [26248320, 0x85d11f3a],
        [1587600, 0x22ae96d7],
        [1234800, 0xe4a322eb],
        [4939200, 0xc9ae4930],
        [8043840, 0x69aec6e0],
        [19227600, 0xf8bb9001],
        [10936800, 0x66f03c94],
        [6526800, 0x254e6810],
        [11289600, 0xda2d66f4],
        [9525600, 0xcc34abf5],
        [18345600, 0xd7e5367d],
        [5644800, 0xf2354b12],
        [12260976, 0x63b3c290],
        [11590656, 0x8ae9ee54],
        [8114400, 0xae38994e],
        [8361360, 0x466db8dc],
        [21927696, 0x379ac9dd],
        [11995200, 0x95860192],
        [11414256, 0xb2bc3409],
        [10936800, 0x531738a2],
        [19333440, 0x02eb9cfb],
        [1430016, 0x4cc69795],
        [1234800, 0x16d59f59],
        [4939200, 0x2754000b],
        [8062656, 0x830c02e5],
        [19227600, 0xd1b2f016],
        [10901520, 0x71ecb272],
        [6703200, 0x01464f07],
        [11061456, 0xd76197df],
      ],
      [['cue', 5694, 0x1930d089]],
    ),
  },
];
