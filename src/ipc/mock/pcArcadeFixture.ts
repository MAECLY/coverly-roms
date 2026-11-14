/**
 * The PC arcade fixtures of the browser mock (spec 2026-10-26-pc-arcade
 * §10, plan D-3):
 *
 * - `?mockPick=pcarcade` (`MOCK_PCARCADE`): an ES-DE library with a
 *   `type-x/` shelf (four folder games, ES-DE's `Chaos Code.exe/`
 *   directory-as-file among them, an empty one, and a loose launcher
 *   `Homura.lnk`) and a `cave3rd/` shelf holding `mushisam.zip`.
 * - `?mockPick=pcarcade-pile` (`MOCK_PCARCADE_PILE`): a pile organised with
 *   the Batocera layout: a loose `mushisam.zip` (goes to `fbneo/`), the same
 *   set already in `roms/cave3rd/` (stays, a sorted folder), and a loose
 *   `Raiden IV (Type X)/game.exe` and `game.exe` left as other files: Type X
 *   is recognised by location only, never by content (spec §9, P19).
 *
 * Synthetic names and sizes only: folder games weigh 0 bytes and the sets
 * 22 (an empty zip's end record). No real data, no keys, no licence or
 * dongle files.
 */
import type { ArtMarks, EntryView, LibraryView, SystemView } from '../types';
import type { MockSortedFolder } from './sampleFixture';

export const MOCK_PCARCADE = '/Users/demo/PcArcade';
export const MOCK_PCARCADE_PILE = '/Users/demo/PcArcadePile';

/** Bytes of an empty zip: its end-of-central-directory record. */
const EMPTY_ZIP = 22;
const DAY = 86_400;

const ROMS = 'ROMs';

interface Shelf {
  folder: string;
  id: string;
  name: string;
}

const SHELVES: readonly Shelf[] = [
  { folder: 'cave3rd', id: 'cave3rd', name: 'Cave CV1000' },
  { folder: 'type-x', id: 'typex', name: 'Taito Type X' },
];
const CAVE3RD = 0;
const TYPEX = 1;

const MISSING: ArtMarks = {
  cover: 'missing',
  screenshot: 'missing',
  title: 'missing',
};
const COVER: ArtMarks = { ...MISSING, cover: 'present' };

type Draft = Omit<EntryView, 'id'>;

/** One entry of a shelf, as Rust's view sends it: `name` is the entry's
 * `title_stem` (a folder game's launcher suffix is its `ext`, cut). */
function entry(
  shelf: number,
  file: string,
  name: string,
  ext: string,
  size: number,
  mtime: number,
  art: ArtMarks = MISSING,
): Draft {
  const dir = `${ROMS}/${SHELVES[shelf]!.folder}`;
  return {
    system: shelf,
    rel_path: `${dir}/${file}`,
    dir,
    name,
    ext,
    size,
    mtime,
    regions: [],
    art,
  };
}

function drafts(now: number): Draft[] {
  const old = now - 90 * DAY;
  return [
    entry(CAVE3RD, 'mushisam.zip', 'mushisam', 'zip', EMPTY_ZIP, old, COVER),
    // Folder games: the folder is the game; nothing inside is ever read.
    entry(
      TYPEX,
      'Akai Katana Shin for NesicaxLive',
      'Akai Katana Shin for NesicaxLive',
      '',
      0,
      old,
    ),
    entry(TYPEX, 'Chaos Code.exe', 'Chaos Code', 'exe', 0, old),
    entry(TYPEX, 'Empty Game', 'Empty Game', '', 0, old),
    // A launcher at the shelf's root is a game too (weak extension).
    entry(TYPEX, 'Homura.lnk', 'Homura', 'lnk', 0, old),
    entry(TYPEX, 'Raiden IV (Type X)', 'Raiden IV (Type X)', '', 0, old, COVER),
  ];
}

/** The library view of `?mockPick=pcarcade`. */
export function pcArcadeLibrary(generation: number, now: number): LibraryView {
  const entries: EntryView[] = drafts(now).map((d, id) => ({ id, ...d }));
  const systems: SystemView[] = SHELVES.map((s, index) => ({
    index,
    folder: s.folder,
    dir: `${ROMS}/${s.folder}`,
    system_id: s.id,
    name: s.name,
    roms: entries.filter((e) => e.system === index).length,
  }));
  return {
    generation,
    root: MOCK_PCARCADE,
    name: 'PcArcade',
    profile: {
      id: 'esde',
      name: 'ES-DE',
      media: ['cover', 'screenshot', 'title'],
      detected: true,
      evidence: ['mock fixture'],
      fitted: false,
      gamelist: false,
    },
    systems,
    entries,
    warnings: [],
    scan_ms: 0,
    loose_files: 0,
  };
}

/** The pile's loose sets: `[name, size]`. */
export const PCARCADE_PILE_SETS: readonly (readonly [string, number])[] = [
  ['mushisam.zip', EMPTY_ZIP],
];

/** What the pile already has sorted, on the Batocera layout. */
export const PCARCADE_PILE_SORTED: readonly MockSortedFolder[] = [
  {
    dir: 'roms/cave3rd',
    system: 'cave3rd',
    files: [['mushisam.zip', EMPTY_ZIP]],
  },
];

/** Launchers no rule claims: listed, never moved, never `typex`. */
export const PCARCADE_PILE_OTHER: readonly string[] = [
  'Raiden IV (Type X)/game.exe',
  'game.exe',
];
