/**
 * Deterministic fake libraries for the browser mock and the tests: the
 * same seed always gives the same LibraryView (names, sizes, dates, art).
 */
import type {
  ArtMark,
  EntryView,
  LibraryView,
  Region,
  SystemFamily,
  SystemView,
} from '../types';

/** 2026-10-03T00:13:20Z: the `now` of the unit tests. */
export const FIXTURE_NOW = 1_791_000_000;
const DAY = 86_400;

/** Small, fast, seedable PRNG returning floats in [0, 1). */
export function mulberry32(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4_294_967_296;
  };
}

interface FixtureSystem {
  folder: string;
  id: string;
  name: string;
  /** Its `systems.toml` family (the picker group). */
  family: SystemFamily;
  exts: readonly string[];
  weight: number;
}

export const FIXTURE_SYSTEMS: readonly FixtureSystem[] = [
  {
    folder: 'DREAMCAST',
    id: 'dreamcast',
    family: 'console',
    name: 'Dreamcast',
    exts: ['chd', 'gdi'],
    weight: 2,
  },
  {
    folder: 'FC',
    id: 'nes',
    family: 'console',
    name: 'NES / Famicom',
    exts: ['nes'],
    weight: 10,
  },
  {
    folder: 'GB',
    id: 'gb',
    family: 'handheld',
    name: 'Game Boy',
    exts: ['gb'],
    weight: 6,
  },
  {
    folder: 'GBA',
    id: 'gba',
    family: 'handheld',
    name: 'Game Boy Advance',
    exts: ['gba', 'zip'],
    weight: 9,
  },
  {
    folder: 'GBC',
    id: 'gbc',
    family: 'handheld',
    name: 'Game Boy Color',
    exts: ['gbc'],
    weight: 5,
  },
  {
    folder: 'GG',
    id: 'gamegear',
    family: 'handheld',
    name: 'Game Gear',
    exts: ['gg'],
    weight: 3,
  },
  {
    folder: 'LYNX',
    id: 'lynx',
    family: 'handheld',
    name: 'Atari Lynx',
    exts: ['lnx'],
    weight: 1,
  },
  {
    folder: 'MAME',
    id: 'mame',
    family: 'arcade',
    name: 'MAME',
    exts: ['zip'],
    weight: 8,
  },
  {
    folder: 'MD',
    id: 'megadrive',
    family: 'console',
    name: 'Mega Drive',
    exts: ['md', 'bin', 'gen'],
    weight: 8,
  },
  {
    folder: 'N64',
    id: 'n64',
    family: 'console',
    name: 'Nintendo 64',
    exts: ['z64', 'n64'],
    weight: 4,
  },
  {
    folder: 'NDS',
    id: 'nds',
    family: 'handheld',
    name: 'Nintendo DS',
    exts: ['nds'],
    weight: 5,
  },
  {
    folder: 'NEOGEO',
    id: 'neogeo',
    family: 'arcade',
    name: 'Neo Geo',
    exts: ['zip'],
    weight: 2,
  },
  {
    folder: 'PCE',
    id: 'pcengine',
    family: 'console',
    name: 'PC Engine',
    exts: ['pce'],
    weight: 3,
  },
  {
    folder: 'PS',
    id: 'psx',
    family: 'console',
    name: 'PlayStation',
    exts: ['pbp', 'chd', 'm3u'],
    weight: 7,
  },
  {
    folder: 'PSP',
    id: 'psp',
    family: 'handheld',
    name: 'PSP',
    exts: ['iso', 'cso'],
    weight: 3,
  },
  {
    folder: 'SFC',
    id: 'snes',
    family: 'console',
    name: 'Super Nintendo',
    exts: ['sfc', 'smc'],
    weight: 10,
  },
  {
    folder: 'SMS',
    id: 'mastersystem',
    family: 'console',
    name: 'Master System',
    exts: ['sms'],
    weight: 3,
  },
  {
    folder: 'WS',
    id: 'wonderswan',
    family: 'handheld',
    name: 'WonderSwan',
    exts: ['ws'],
    weight: 1,
  },
];

/** Names that must render as text, never as markup (in SFC). */
export const HOSTILE_NAMES = [
  '<img src=x onerror=alert(1)>',
  'Pokémon - Edición Oro (Spain)',
  '..\\..\\escape',
  '‮gnp.exe',
  `${'Very Long Title '.repeat(11).trim()} (USA)`,
] as const;

const HOSTILE_REGIONS: Record<string, Region[]> = {
  'Pokémon - Edición Oro (Spain)': ['spain'],
};

const FIRST = [
  'Super',
  'Mega',
  'Final',
  'Legend of',
  'Street',
  'Dragon',
  'Metal',
  'Sonic',
  'Star',
  'Shadow',
  'Castle',
  'Ninja',
  'Puzzle',
  'Turbo',
  'Space',
  'Pokémon',
  'Crónicas',
  'Æther',
  'Kirby',
  'Mortal',
];
const SECOND = [
  'Fighter',
  'Quest',
  'Warriors',
  'Adventure',
  'Racing',
  'Soccer',
  'Kombat',
  'Hunter',
  'Island',
  'Saga',
  'Force',
  'Tactics',
  'Dreams',
  'Blaster',
  'Odyssey',
  'Knights',
  'Heroes',
  'Rally',
  'Tennis',
  'Galaxy',
];
const SUFFIX = [
  '',
  '',
  '',
  ' II',
  ' III',
  ' 64',
  ' Advance',
  ' Deluxe',
  ': The Return',
  ' Turbo',
];
const TAGS: readonly { tag: string; regions: Region[] }[] = [
  { tag: ' (USA)', regions: ['usa'] },
  { tag: ' (Europe)', regions: ['europe'] },
  { tag: ' (Japan)', regions: ['japan'] },
  { tag: ' (USA, Europe)', regions: ['usa', 'europe'] },
  { tag: ' (Spain)', regions: ['spain'] },
  { tag: ' (World)', regions: ['world'] },
  { tag: '', regions: [] },
];

export interface FixtureOptions {
  seed: number;
  count: number;
  /** Shown as the library root; its last segment is the library name. */
  root: string;
  generation: number;
  /** Unix seconds that "recent" dates are relative to. */
  now: number;
}

const collator = new Intl.Collator('en', {
  sensitivity: 'base',
  numeric: true,
});

export function generateLibrary(o: FixtureOptions): LibraryView {
  const rand = mulberry32(o.seed);
  const pick = <T>(items: readonly T[]): T =>
    items[Math.floor(rand() * items.length)] as T;
  const mark = (p: number): ArtMark => (rand() < p ? 'present' : 'missing');

  const totalWeight = FIXTURE_SYSTEMS.reduce((n, s) => n + s.weight, 0);
  const counts = FIXTURE_SYSTEMS.map((s) =>
    Math.floor((o.count * s.weight) / totalWeight),
  );
  let rest = o.count - counts.reduce((a, b) => a + b, 0);
  for (let i = 0; rest > 0; i = (i + 1) % counts.length, rest -= 1) {
    counts[i] = (counts[i] ?? 0) + 1;
  }

  const systems: SystemView[] = [];
  const entries: EntryView[] = [];
  FIXTURE_SYSTEMS.forEach((sys, index) => {
    const n = counts[index] ?? 0;
    const seen = new Set<string>();
    const draft: Omit<EntryView, 'id'>[] = [];
    const base = `ROMS/${sys.folder}`;
    const hostile = sys.folder === 'SFC' && n >= HOSTILE_NAMES.length;
    for (let i = 0; i < n; i += 1) {
      const sub = rand();
      const dir =
        sub < 0.08
          ? `${base}/Hacks`
          : sub < 0.13
            ? `${base}/Translations/ES`
            : base;
      const ext = pick(sys.exts);
      const tag = pick(TAGS);
      let name: string;
      let regions: Region[];
      if (hostile && i < HOSTILE_NAMES.length) {
        name = HOSTILE_NAMES[i] as string;
        regions =
          HOSTILE_REGIONS[name] ?? (name.endsWith('(USA)') ? ['usa'] : []);
      } else {
        name = `${pick(FIRST)} ${pick(SECOND)}${pick(SUFFIX)}${tag.tag}`;
        regions = tag.regions;
      }
      let unique = name;
      for (let k = 2; seen.has(`${dir}/${unique}.${ext}`); k += 1) {
        unique = `${name} (Rev ${k})`;
      }
      seen.add(`${dir}/${unique}.${ext}`);
      const recent = rand() < 0.03;
      const age = recent
        ? Math.floor(rand() * 6 * DAY)
        : Math.floor(rand() * 3 * 365 * DAY);
      draft.push({
        system: index,
        rel_path: `${dir}/${unique}.${ext}`,
        dir,
        name: unique,
        ext,
        size: Math.round(65_536 * Math.pow(10_000, rand())),
        mtime: o.now - age,
        regions,
        art: { cover: mark(0.62), screenshot: mark(0.45), title: mark(0.35) },
      });
    }
    draft.sort(
      (a, b) => a.dir.localeCompare(b.dir) || collator.compare(a.name, b.name),
    );
    for (const d of draft) entries.push({ id: entries.length, ...d });
    systems.push({
      index,
      folder: sys.folder,
      dir: base,
      system_id: sys.id,
      name: sys.name,
      roms: n,
    });
  });

  const name = o.root.split('/').filter(Boolean).pop() ?? o.root;
  return {
    generation: o.generation,
    root: o.root,
    name,
    profile: {
      id: 'esde',
      name: 'ES-DE',
      media: ['cover', 'screenshot', 'title'],
      detected: true,
      evidence: ['mock fixture'],
      fitted: false,
      // Only the `batocera` layout reads gamelist.xml; the fixture is ES-DE.
      gamelist: false,
    },
    systems,
    entries,
    warnings: [],
    scan_ms: 0,
    loose_files: 0,
  };
}
