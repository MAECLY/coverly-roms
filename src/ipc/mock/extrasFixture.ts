/**
 * The Extras library of the browser mock (`?mockPick=extras`, part D):
 * game folders whose `Extras/` shows every state of the panel section.
 *
 * - «Tidewalker» (PS3): an official `.p3t` theme in `Themes/`, three
 *   tagged tracks in `Soundtrack/`, art, a trailer, a guide, the user's
 *   `Remixes/`, loose files and a record whose file is gone.
 * - «Star Courier» (SNES): SPC rips, played by another app.
 * - «Lantern Harbor» (PS4): an `extras.json` that cannot be read.
 * - «Paper Skies» (Switch): an `extras.json` from a newer Coverly.
 * - «Comet Derby» (PS4): on a disk with no Trash (files are set aside).
 * - «Moss Garden» (3DS): no `Extras/` yet; the manual is in the game.
 * - «Archive Run» (PS4): more than 200 files.
 * - «Neon Rhythm» (PS4): a game folder the app cannot write.
 * - «Pocket Quest (USA)» (SNES): a loose ROM, no game folder.
 *
 * Synthetic names and sizes only; the media the viewer plays are made by
 * us (`media/`, `scripts/dev/make-mock-media.sh`).
 */
import type {
  ArtMarks,
  EntryView,
  ExtraFormat,
  IndexState,
  LibraryView,
  Provenance,
  SystemView,
} from '../types';

export const MOCK_EXTRAS = '/Users/demo/Extras';

const ROMS = 'ROMS';
const DAY = 86_400;

interface ExtrasSystem {
  folder: string;
  id: string;
  name: string;
}

const SYSTEMS: readonly ExtrasSystem[] = [
  { folder: 'PS3', id: 'ps3', name: 'Sony PlayStation 3' },
  { folder: 'SFC', id: 'snes', name: 'Super Nintendo' },
  { folder: 'PS4', id: 'ps4', name: 'Sony PlayStation 4' },
  { folder: 'SWITCH', id: 'switch', name: 'Nintendo Switch' },
  { folder: '3DS', id: '3ds', name: 'Nintendo 3DS' },
];
const PS3 = 0;
const SNES = 1;
const PS4 = 2;
const SWITCH = 3;
const N3DS = 4;

const COVER: ArtMarks = {
  cover: 'present',
  screenshot: 'missing',
  title: 'missing',
};

/** One file of a game's `Extras/`, by its `/`-joined path inside it. */
export interface SeedFile {
  path: string;
  size: number;
  provenance?: Provenance;
  format?: ExtraFormat;
  tags?: string[];
  /** What the text view shows (txt, md, nfo). */
  text?: string;
}

/** A game folder's `Extras/` as the mock starts it. */
export interface SeedGame {
  files: SeedFile[];
  /** Records whose file is gone («No encontrados»). */
  missing?: SeedFile[];
  /** Folders without files (an empty user category). */
  dirs?: string[];
  /** What reading `extras.json` finds; default `ok` with files, else
   * `absent`. */
  index?: IndexState;
  /** The app cannot write in the game folder. */
  readOnly?: true;
  /** The drive has no Trash: files are set aside in `.coverly-trash`. */
  noTrash?: true;
}

const NFO = [
  '╔════════════╗',
  '║ TIDEWALKER ║',
  '╚════════════╝',
  '',
  'Shown as text, never as a page:',
  "<script>alert('nfo')</script>",
  '',
].join('\n');

const NOTES = [
  'Tidewalker — notas',
  '',
  'El tema del jefe final suena mejor con auriculares.',
  '',
].join('\n');

const tidewalker: SeedGame = {
  files: [
    {
      path: 'Soundtrack/01 Opening Tide.flac',
      size: 28_400_000,
      tags: ['banda sonora original'],
    },
    {
      path: 'Soundtrack/02 Coral Depths.wav',
      size: 41_300_000,
      tags: ['ambiente'],
    },
    {
      path: 'Soundtrack/03 Leviathan.flac',
      size: 33_900_000,
      tags: ['tema del jefe final', 'versión orquestal'],
    },
    { path: 'Artwork/Key Art.png', size: 2_400_000 },
    { path: 'Videos/Launch Trailer.webm', size: 84_000_000 },
    {
      path: 'Themes/Tidewalker Theme.p3t',
      size: 4_812_345,
      provenance: 'official',
      format: 'ps3-p3t',
    },
    { path: 'Guides/Field Guide.pdf', size: 6_100_000 },
    {
      path: 'Remixes/02 Coral Depths.mp3',
      size: 7_200_000,
      tags: ['lo-fi'],
    },
    { path: '02 Coral Depths.mp3', size: 7_200_000 },
    { path: 'Notes.txt', size: NOTES.length, text: NOTES },
    { path: 'Readme.nfo', size: NFO.length, text: NFO },
  ],
  missing: [
    { path: 'Soundtrack/04 Ending.flac', size: 30_100_000, tags: ['créditos'] },
  ],
};

const starCourier: SeedGame = {
  files: [
    {
      path: 'Soundtrack/Star Courier - Title Theme.spc',
      size: 66_048,
      format: 'spc',
    },
    {
      path: 'Soundtrack/Star Courier - Ending.spc',
      size: 66_048,
      format: 'spc',
      tags: ['créditos'],
    },
  ],
};

const lanternHarbor: SeedGame = {
  index: { state: 'malformed', bad_records: 0, unreadable: null },
  files: [
    { path: 'Artwork/Harbor Map.png', size: 3_100_000 },
    { path: 'Guides/Notes.txt', size: 42, text: 'Lantern Harbor: notas.\n' },
  ],
};

const paperSkies: SeedGame = {
  index: { state: 'newer', version: 2 },
  files: [
    {
      path: 'Soundtrack/Main Theme.mp3',
      size: 6_400_000,
      tags: ['tema principal'],
    },
    { path: 'Artwork/Sky Chart.png', size: 1_900_000 },
  ],
};

const cometDerby: SeedGame = {
  noTrash: true,
  files: [
    { path: 'Soundtrack/Engine Hum.ogg', size: 5_300_000 },
    { path: 'Artwork/Livery.png', size: 2_200_000 },
    { path: 'Guides/Track Notes.md', size: 64, text: '# Comet Derby\n' },
  ],
};

const archiveRun: SeedGame = {
  files: Array.from({ length: 240 }, (_, i) => ({
    path: `Artwork/Scan ${String(i + 1).padStart(3, '0')}.png`,
    size: 1_000_000 + i * 7_919,
  })),
};

const neonRhythm: SeedGame = {
  readOnly: true,
  files: [
    { path: 'Soundtrack/Pulse.mp3', size: 5_100_000, tags: ['favorita'] },
    { path: 'Videos/Stage Clip.mp4', size: 48_000_000 },
  ],
};

type Draft = Omit<EntryView, 'id'>;

function file(system: number, rel: string, size: number, mtime: number) {
  const rel_path = `${ROMS}/${SYSTEMS[system]!.folder}/${rel}`;
  const slash = rel_path.lastIndexOf('/');
  const base = rel_path.slice(slash + 1);
  const dot = base.lastIndexOf('.');
  return {
    system,
    rel_path,
    dir: rel_path.slice(0, slash),
    name: base.slice(0, dot),
    ext: base.slice(dot + 1),
    size,
    mtime,
    regions: [],
    art: COVER,
  } satisfies Draft;
}

/** A game folder entry: the folder is the game (`folder_root`). */
function game(system: number, rel: string, size: number, mtime: number): Draft {
  return { ...file(system, rel, size, mtime), folder_root: true };
}

/** The games of the library and their `Extras/`, by entry `rel_path`
 * (a loose ROM has none). */
export function extrasGames(now: number): {
  drafts: Draft[];
  seeds: Map<string, SeedGame>;
} {
  const old = now - 120 * DAY;
  const drafts: Draft[] = [];
  const seeds = new Map<string, SeedGame>();
  const add = (draft: Draft, seed?: SeedGame) => {
    drafts.push(draft);
    if (seed) seeds.set(draft.rel_path, seed);
  };
  add(game(PS3, 'Tidewalker/Tidewalker.pkg', 9_200_000_000, old), tidewalker);
  add(
    game(SNES, 'Star Courier/Star Courier (USA).sfc', 1_048_576, old),
    starCourier,
  );
  add(
    game(PS4, 'Lantern Harbor/Lantern Harbor.pkg', 18_400_000_000, old),
    lanternHarbor,
  );
  add(
    game(SWITCH, 'Paper Skies/Paper Skies.nsp', 3_300_000_000, old),
    paperSkies,
  );
  add(game(PS4, 'Comet Derby/Comet Derby.pkg', 7_700_000_000, old), cometDerby);
  add(game(N3DS, 'Moss Garden/Moss Garden.cia', 512_000_000, old), {
    files: [],
  });
  add(
    game(PS4, 'Archive Run/Archive Run.pkg', 11_000_000_000, old),
    archiveRun,
  );
  add(game(PS4, 'Neon Rhythm/Neon Rhythm.pkg', 9_800_000_000, old), neonRhythm);
  add(file(SNES, 'Pocket Quest (USA).sfc', 524_288, old));
  return { drafts, seeds };
}

function systemsOf(entries: readonly EntryView[]): SystemView[] {
  return SYSTEMS.map((s, index) => ({
    index,
    folder: s.folder,
    dir: `${ROMS}/${s.folder}`,
    system_id: s.id,
    name: s.name,
    roms: entries.filter((e) => e.system === index).length,
  }));
}

/** The library view (`?mockPick=extras`); `extras(rel_path)` is a game
 * folder's listed file count, for its `addons` cell. */
export function extrasLibrary(
  generation: number,
  now: number,
  extras: (relPath: string) => number | null,
): LibraryView {
  const { drafts } = extrasGames(now);
  const entries: EntryView[] = drafts.map((draft, id) => {
    const n = extras(draft.rel_path);
    return n === null
      ? { id, ...draft }
      : {
          id,
          ...draft,
          addons: { mods: 0, cheats: 0, extras: n, saves: 0 },
        };
  });
  return {
    generation,
    root: MOCK_EXTRAS,
    name: 'Extras',
    profile: {
      id: 'coverly-library',
      name: 'Coverly',
      media: ['cover', 'screenshot', 'title'],
      detected: true,
      evidence: ['mock fixture'],
      fitted: false,
      gamelist: false,
    },
    systems: systemsOf(entries),
    entries,
    warnings: [],
    scan_ms: 0,
    loose_files: 0,
  };
}
