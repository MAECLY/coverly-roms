/**
 * The add-ons library of the browser mock (part C, plan P-4): games with
 * their updates, DLC, demos and revisions as the scan sends them, so the
 * six views, the game panel and the filters can be exercised without
 * running Organizar.
 *
 * - `?mockPick=addons` (`MOCK_ADDONS`): the approved mockup's invented
 *   library (Lantern Harbor, Tidewalker, Neon Rhythm…).
 * - `?mockPick=addons-huge` (`MOCK_ADDONS_HUGE`): 10 000 games with
 *   50 000 updates and DLC, seeded, for the D1 performance check.
 *
 * Synthetic names and sizes only. Every ordering (children newest first,
 * `installed`, `newer_elsewhere`) is written here as Rust would send it;
 * the webview never decides it.
 */
import type {
  ArtMarks,
  ChildDetailView,
  ChildView,
  EntryView,
  GameDetailsView,
  LibraryView,
  Region,
  SystemView,
} from '../types';
import { mulberry32 } from './fixture';

export const MOCK_ADDONS = '/Users/demo/Addons';
export const MOCK_ADDONS_HUGE = '/Users/demo/AddonsHuge';
/** Where the mock keeps the revision defaults (library root → version
 * group → `rel_path`), so a choice survives a reload. */
export const MOCK_REVISIONS_KEY = 'coverly.mock.revisions';

interface AddonSystem {
  folder: string;
  id: string;
  name: string;
}

const SYSTEMS: readonly AddonSystem[] = [
  { folder: 'PS4', id: 'ps4', name: 'Sony PlayStation 4' },
  { folder: 'SWITCH', id: 'switch', name: 'Nintendo Switch' },
  { folder: 'SFC', id: 'snes', name: 'Super Nintendo' },
  { folder: '3DS', id: '3ds', name: 'Nintendo 3DS' },
];
const PS4 = 0;
const SWITCH = 1;
const SNES = 2;
const N3DS = 3;

const ROMS = 'ROMS';
const DAY = 86_400;

/** The «Nuevo» candidate (D18): a DLC the mock dates to yesterday. */
export const ADDONS_NEW_CHILD = `${ROMS}/PS4/Neon Rhythm/DLC/Neon Rhythm - Encore Pack.pkg`;

const COVER: ArtMarks = {
  cover: 'present',
  screenshot: 'missing',
  title: 'missing',
};
const NO_COVER: ArtMarks = {
  cover: 'missing',
  screenshot: 'missing',
  title: 'missing',
};

type Draft = Omit<EntryView, 'id'>;

function profile(): LibraryView['profile'] {
  return {
    id: 'coverly-library',
    name: 'Coverly',
    media: ['cover', 'screenshot', 'title'],
    detected: true,
    evidence: ['mock fixture'],
    fitted: false,
    gamelist: false,
  };
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

function file(
  system: number,
  rel: string,
  size: number,
  mtime: number,
): Pick<
  Draft,
  'system' | 'rel_path' | 'dir' | 'name' | 'ext' | 'size' | 'mtime'
> {
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
  };
}

function child(
  index: number,
  rel_path: string,
  role: ChildView['role'],
  size: number,
  mtime: number,
  art: ChildView['art'] = { icon: false, banner: false },
): ChildView {
  const base = rel_path.slice(rel_path.lastIndexOf('/') + 1);
  const name = base.slice(0, base.lastIndexOf('.'));
  const version = /\(v([0-9][0-9.]*)\)$/.exec(name)?.[1];
  return {
    index,
    rel_path,
    name,
    role,
    ...(version !== undefined && { version }),
    size,
    mtime,
    art,
  };
}

/** The mockup's library (`?mockPick=addons`). */
export function addonViewsLibrary(
  generation: number,
  now: number = Math.floor(Date.now() / 1000),
): LibraryView {
  const old = now - 200 * DAY;
  const drafts: { key: string; draft: Draft }[] = [];
  const add = (key: string, draft: Draft) => drafts.push({ key, draft });
  const usa: Region[] = ['usa'];

  // PS4: Lantern Harbor with two updates, a DLC with its art read from
  // the package, its demo and filled Mods/Cheats/Extras folders.
  const lhDir = `${ROMS}/PS4/Lantern Harbor`;
  add('lh', {
    ...file(
      PS4,
      'Lantern Harbor/Lantern Harbor (USA).pkg',
      18_400_000_000,
      old,
    ),
    regions: usa,
    art: COVER,
    children: [
      child(
        1,
        `${lhDir}/Updates/Lantern Harbor (v1.10).pkg`,
        'update',
        1_900_000_000,
        old + 40 * DAY,
      ),
      child(
        0,
        `${lhDir}/Updates/Lantern Harbor (v1.05).pkg`,
        'update',
        1_200_000_000,
        old + 10 * DAY,
      ),
      child(
        2,
        `${lhDir}/DLC/Lantern Harbor - Lighthouse Keeper Pack.pkg`,
        'dlc',
        640_000_000,
        old + 20 * DAY,
        {
          icon: true,
          banner: false,
          icon_origin: 'package',
        },
      ),
    ],
    installed: '1.10',
    addons: { mods: 1, cheats: 2, extras: 3, saves: 0 },
  });
  add('lh-demo', {
    ...file(
      PS4,
      'Lantern Harbor (Demo)/Lantern Harbor (Demo).pkg',
      2_100_000_000,
      old,
    ),
    regions: [],
    art: COVER,
    demo: {},
  });
  // A demo whose full game is not in the library.
  add('ps-demo', {
    ...file(
      PS4,
      'Paper Skies (Demo)/Paper Skies (Demo).pkg',
      1_300_000_000,
      old,
    ),
    regions: [],
    art: NO_COVER,
    demo: {},
  });
  const nrDir = `${ROMS}/PS4/Neon Rhythm`;
  add('nr', {
    ...file(PS4, 'Neon Rhythm/Neon Rhythm.pkg', 9_800_000_000, old),
    regions: [],
    art: COVER,
    children: [
      child(2, ADDONS_NEW_CHILD, 'dlc', 420_000_000, now - DAY),
      child(
        0,
        `${nrDir}/DLC/Neon Rhythm - Night Drive Pack.pkg`,
        'dlc',
        380_000_000,
        old + 30 * DAY,
      ),
      child(
        1,
        `${nrDir}/DLC/Neon Rhythm - Synthwave Pack.pkg`,
        'dlc',
        350_000_000,
        old + 60 * DAY,
      ),
    ],
  });

  // Switch: Tidewalker misses v1.3.0, which sits outside its folder.
  const twDir = `${ROMS}/SWITCH/Tidewalker`;
  add('tw', {
    ...file(SWITCH, 'Tidewalker/Tidewalker.nsp', 6_200_000_000, old),
    regions: [],
    art: COVER,
    children: [
      child(
        0,
        `${twDir}/Updates/Tidewalker (v1.0.2).nsp`,
        'update',
        310_000_000,
        old + 5 * DAY,
      ),
      child(
        1,
        `${twDir}/DLC/Tidewalker - Reef Pack.nsp`,
        'dlc',
        52_000_000,
        old + 8 * DAY,
      ),
    ],
    installed: '1.0.2',
  });
  add('tw-orphan', {
    ...file(
      SWITCH,
      'Tidewalker (Update)/Updates/Tidewalker (v1.3.0).nsp',
      480_000_000,
      now - 3 * DAY,
    ),
    regions: [],
    art: NO_COVER,
    orphan: 'update',
    version: '1.3.0',
  });
  add('kd', {
    ...file(SWITCH, 'Kite Days/Kite Days.nsp', 3_400_000_000, old),
    regions: [],
    art: COVER,
    children: [
      child(
        0,
        `${ROMS}/SWITCH/Kite Days/Updates/Kite Days (v1.1.0).nsp`,
        'update',
        120_000_000,
        old + 12 * DAY,
      ),
    ],
    installed: '1.1.0',
  });
  add('pl', {
    ...file(SWITCH, 'Pebble League/Pebble League.xci', 4_700_000_000, old),
    regions: [],
    art: NO_COVER,
  });
  // A DLC whose game is not in the library at all.
  add('cd-orphan', {
    ...file(
      SWITCH,
      'Comet Derby (DLC)/DLC/Comet Derby - Track Pack.nsp',
      90_000_000,
      old,
    ),
    regions: [],
    art: NO_COVER,
    orphan: 'dlc',
  });

  // SNES: Star Courier untagged and `(Rev 1)`, one version group.
  const group = 'snes:star courier (usa)';
  add('sc', {
    ...file(SNES, 'Star Courier (USA).sfc', 1_048_576, old),
    regions: usa,
    art: COVER,
    version_group: group,
  });
  add('sc-rev1', {
    ...file(SNES, 'Star Courier (USA) (Rev 1).sfc', 1_048_576, old),
    regions: usa,
    art: NO_COVER,
    version_group: group,
    revision: 'Rev 1',
  });

  // 3DS: Moss Garden, whose product code ends in `T` (unknown, §5).
  add('mg', {
    ...file(N3DS, 'Moss Garden/Moss Garden.cia', 512_000_000, old),
    regions: [],
    art: COVER,
  });

  const entries: EntryView[] = drafts.map(({ draft }, id) => ({
    id,
    ...draft,
  }));
  const idOf = (key: string) => drafts.findIndex((d) => d.key === key);
  const lh = entries[idOf('lh')]!;
  entries[idOf('lh-demo')]!.demo = { of: lh.id, of_name: lh.name };
  const tw = entries[idOf('tw')]!;
  const orphan = entries[idOf('tw-orphan')]!;
  orphan.orphan_of = tw.id;
  tw.newer_elsewhere = { entry: orphan.id, version: '1.3.0' };

  return {
    generation,
    root: MOCK_ADDONS,
    name: 'Addons',
    profile: profile(),
    systems: systemsOf(entries),
    entries,
    warnings: [],
    scan_ms: 0,
    loose_files: 0,
  };
}

/** Header facts of the mock library's games, by `rel_path` (D3). */
const DETAILS: Record<string, Omit<GameDetailsView, 'request' | 'children'>> = {
  [`${ROMS}/PS4/Lantern Harbor/Lantern Harbor (USA).pkg`]: {
    title_id: 'CUSA31415',
  },
  [`${ROMS}/SWITCH/Tidewalker/Tidewalker.nsp`]: {
    title_id: '0100A1B2C3D4E000',
  },
  [`${ROMS}/3DS/Moss Garden/Moss Garden.cia`]: {
    title_id: '00040000000A1B00',
    product_code: 'CTR-P-AMGT',
    // `T` is not in the verified table (§5): flagged, never guessed.
    region: { letter: 'T', code: 'CTR-P-AMGT', certainty: 'unknown' },
  },
};

/** What the mock's `game_details` answers: no title list (`latest` is
 * never set, as with Rust's `NoTitleList`). */
export function addonDetails(e: EntryView, request: number): GameDetailsView {
  const children: ChildDetailView[] = (e.children ?? []).map((c) => ({
    index: c.index,
    kind: c.role,
  }));
  return { request, ...DETAILS[e.rel_path], children };
}

const HUGE_FIRST = [
  'Amber',
  'Brass',
  'Cinder',
  'Delta',
  'Ember',
  'Frost',
  'Glass',
  'Harbor',
  'Iron',
  'Jade',
  'Kestrel',
  'Lumen',
  'Marble',
  'North',
  'Opal',
  'Pine',
] as const;
const HUGE_SECOND = [
  'Run',
  'Quest',
  'Tales',
  'League',
  'Drift',
  'Garden',
  'Courier',
  'Skies',
  'Rhythm',
  'Days',
  'Walker',
  'Derby',
] as const;
const HUGE_SYSTEMS = [PS4, SWITCH, N3DS] as const;
const HUGE_EXT: Record<number, string> = {
  [PS4]: 'pkg',
  [SWITCH]: 'nsp',
  [N3DS]: 'cia',
};
export const HUGE_GAMES = 10_000;
/** Children per game average 5: pairs of games share 10. */
const HUGE_PAIR_CHILDREN = 10;

/** 10 000 games and 50 000 children (`?mockPick=addons-huge`), seeded. */
export function addonViewsHuge(
  generation: number,
  now: number = Math.floor(Date.now() / 1000),
): LibraryView {
  const rand = mulberry32(2101);
  const pick = <T>(xs: readonly T[]): T => xs[Math.floor(rand() * xs.length)]!;
  const entries: EntryView[] = [];
  let pairRest = 0;
  for (let id = 0; id < HUGE_GAMES; id += 1) {
    // A demo follows its full game, on the same system (as the scan links
    // them by title key within one system).
    const prev = entries[id - 1];
    const isDemo = prev !== undefined && !prev.demo && rand() < 0.04;
    const system = isDemo ? prev.system : pick(HUGE_SYSTEMS);
    const title = isDemo
      ? prev.name
      : `${pick(HUGE_FIRST)} ${pick(HUGE_SECOND)} ${String(id).padStart(5, '0')}`;
    const name = isDemo ? `${title} (Demo)` : title;
    const ext = HUGE_EXT[system]!;
    const dir = `${ROMS}/${SYSTEMS[system]!.folder}/${name}`;
    const mtime = now - Math.floor(rand() * 900) * DAY;
    // Even ids take 0..10 children, odd ids the rest of the pair's 10.
    const count =
      id % 2 === 0
        ? (pairRest = Math.floor(rand() * (HUGE_PAIR_CHILDREN + 1)))
        : HUGE_PAIR_CHILDREN - pairRest;
    const updates = Math.floor(rand() * (count + 1));
    const children: ChildView[] = [];
    // Newest first, as Rust sends them.
    for (let u = updates; u >= 1; u -= 1) {
      children.push(
        child(
          children.length,
          `${dir}/Updates/${title} (v1.${u}.0).${ext}`,
          'update',
          50_000_000 + u * 1_000_000,
          mtime + u * DAY,
        ),
      );
    }
    for (let d = 1; d <= count - updates; d += 1) {
      const art = rand() < 0.3;
      children.push(
        child(
          children.length,
          `${dir}/DLC/${title} - Pack ${d}.${ext}`,
          'dlc',
          20_000_000 + d * 100_000,
          mtime + d * DAY,
          art
            ? { icon: true, banner: false, icon_origin: 'package' }
            : { icon: false, banner: false },
        ),
      );
    }
    const e: EntryView = {
      id,
      system,
      rel_path: `${dir}/${name}.${ext}`,
      dir,
      name,
      ext,
      size: Math.round(500_000_000 + rand() * 20_000_000_000),
      mtime,
      regions: [],
      art: rand() < 0.7 ? COVER : NO_COVER,
    };
    if (children.length > 0) e.children = children;
    if (updates > 0) e.installed = `1.${updates}.0`;
    if (isDemo) e.demo = { of: prev.id, of_name: prev.name };
    entries.push(e);
  }
  return {
    generation,
    root: MOCK_ADDONS_HUGE,
    name: 'AddonsHuge',
    profile: profile(),
    systems: systemsOf(entries),
    entries,
    warnings: [],
    scan_ms: 0,
    loose_files: 0,
  };
}
