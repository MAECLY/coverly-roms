/**
 * Browser stand-in for the Tauri backend (`pnpm dev` in Chrome, tests):
 * deterministic fixture libraries, fake progress, SVG art, fake identity.
 */
import type { Backend } from '../backend';
import { BackendError } from '../errors';
import type {
  EntryView,
  FoundTool,
  LibraryView,
  Media,
  ProfileInfo,
  RecentLibrary,
  ScanProgress,
} from '../types';
import { FIXTURE_SYSTEMS, generateLibrary } from './fixture';
import { createMockDedupe, type MockDedupeScenario } from './mockDedupe';
import { createMockJobs, mockTitle, type MockExport } from './mockJobs';
import {
  createMockOrganize,
  ORGANIZE_SYSTEMS,
  readMockPick,
  type MockExtractRate,
  type MockOrganizeTouch,
} from './mockOrganize';
import { createMockSettings } from './mockSettings';
import {
  createMockUpdates,
  readMockUpdateScenario,
  type MockUpdateScenario,
} from './mockUpdates';
import { escapeXml } from './svg';

export { escapeXml } from './svg';
export { readMockExtract } from './mockOrganize';
export type { MockExport } from './mockJobs';
export type { MockDedupeScenario } from './mockDedupe';
export {
  MOCK_DOWNLOADS,
  MOCK_MIXED,
  MOCK_PICK_KEY,
  MOCK_SAMPLE,
} from './mockOrganize';

export const MOCK_LIBRARIES = [
  { path: '/mock/Fixture SD (12k)', count: 12_000, seed: 12 },
  { path: '/mock/Small library', count: 240, seed: 7 },
  { path: '/mock/Empty library', count: 0, seed: 1 },
] as const;
export const MOCK_EJECTED = '/Volumes/EJECTED';

export interface MockOptions {
  /** Delay per scanned system, in ms (0 in tests). */
  delayMs?: number;
  /** Unix seconds that fixture dates are relative to. */
  now?: number;
  /** The first apply of each job pauses (rate limit) after this many items; null = never (default 25). */
  pauseAfter?: number | null;
  /** What "From file…" picks (default: an image). */
  pick?: 'image' | 'cancel' | 'not-image';
  /** Where settings, custom profiles and credential presence persist
   * (default: `window.localStorage` when usable; null = memory only). */
  storage?: Storage | null;
  /** The first organise apply of each job pauses (disk full) after this
   * many items; null = never (default). */
  organizePauseAfter?: number | null;
  /** How fast the organise mock extracts: `'fast'` (default, tests) or
   * MB/s (the browser passes `?mockExtract=`, default 25). */
  extractRate?: MockExtractRate;
  /** What the updater answers (default: `?mockUpdate=`, then localStorage
   * `coverly.mock.update`, then `up-to-date`). */
  update?: MockUpdateScenario;
  /** What "Open folder…" picks (default: `?mockPick=`, then localStorage
   * `coverly.mock.pick`, then the first fixture library). */
  folder?: string;
  /** The external archive tools Settings → Organise lists as found
   * (default none). */
  externalTools?: readonly FoundTool[];
  /** The duplicates plans' library volume has a Trash (default true). */
  dedupeTrashAvailable?: boolean;
  /** The first duplicates apply of each job pauses (disk full) after this
   * many extras; null = never (default). */
  dedupePauseAfter?: number | null;
}

export interface MockBackend extends Backend {
  /** Entry ids passed to `revealEntry`, for tests. */
  readonly revealed: number[];
  /** Files "saved" by the exports, for tests. */
  readonly exports: MockExport[];
  /** Folders and links "opened" (`folder:logs`, `link:repo`), for tests. */
  readonly opened: string[];
  /** Updater calls (`check:startup`, `check:manual`, `download`, `install`,
   * `restart`), for tests. */
  readonly updateLog: string[];
  /** Adds or removes files of an organise pile between scans (dev, tests). */
  organizeTouch(path: string, change: MockOrganizeTouch): void;
  /** Paths passed to `revealPath`, for tests. */
  readonly revealedPaths: string[];
  /** `dedupeArt` calls as `plan:group:member|g:media`, for tests. */
  readonly artCalls: string[];
  /** What the next duplicates plans and applies do (dev, tests). */
  dedupeScenario(patch: Partial<MockDedupeScenario>): void;
}

export function mockArtSvg(e: EntryView, media: Media): string {
  const hue = (e.id * 47) % 360;
  const [w, h] = media === 'cover' ? [250, 360] : [320, 240];
  const label = escapeXml(
    e.name.length > 40 ? `${e.name.slice(0, 39)}…` : e.name,
  );
  return (
    `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}" viewBox="0 0 ${w} ${h}">` +
    `<rect width="100%" height="100%" fill="hsl(${hue} 55% 42%)"/>` +
    `<text x="50%" y="50%" fill="#fff" font-family="system-ui" font-size="16" text-anchor="middle">${label}</text>` +
    `<text x="50%" y="${h - 16}" fill="#fff" opacity=".7" font-family="system-ui" font-size="12" text-anchor="middle">${media}</text>` +
    `</svg>`
  );
}

const handheld = (
  id: string,
  name: string,
  romsRoot: string,
  folderNames?: Record<string, string>,
  hosts: Record<string, string> = {},
): ProfileInfo => ({
  profile: {
    id,
    ...(folderNames && { folder_names: folderNames }),
    name,
    roms_root: romsRoot,
    art_dirs: ['Imgs'],
    gamelist: false,
    host_folders: { ngpc: 'ngp', ...hosts, wonderswancolor: 'wonderswan' },
    slots: {
      cover: {
        template: '{system_dir}/{rel_dir}/Imgs/{stem}.png',
        format: 'png',
        max_width: 250,
        max_height: 360,
      },
    },
  },
  builtin: true,
  mirror_subdirs: true,
});

const png = (template: string) => ({
  template,
  format: 'png' as const,
  max_width: null,
  max_height: null,
});

/** `SYSTEM_FOLDERS` of `profiles/builtin.rs`: the folder each frontend
 * names for the systems of batch 1 (spec §6.1); Absent cells are left out. */
const EARLY_CONSOLES = {
  channelf: 'channelf',
  colecovision: 'colecovision',
  intellivision: 'intellivision',
  odyssey2: 'odyssey2',
  scv: 'scv',
  vectrex: 'vectrex',
};
/** Sega, NEC and Nintendo add-ons (Task 6) named alike on ES-DE,
 * Batocera and muOS. */
const ADD_ONS = {
  n64dd: 'n64dd',
  satellaview: 'satellaview',
  sufami: 'sufami',
  supergrafx: 'supergrafx',
};
/** Handhelds (Task 7) named alike on ES-DE, Batocera and muOS (muOS has
 * no Game.com folder). */
const HANDHELDS = { megaduck: 'megaduck', supervision: 'supervision' };
/** Computers I (Task 8) named alike on ES-DE, Batocera and muOS. */
const COMPUTERS_I = {
  amstradcpc: 'amstradcpc',
  coco: 'coco',
  zx81: 'zx81',
};
/** Computers II (Task 9) named alike on ES-DE, Batocera and muOS. */
const COMPUTERS_II = { pc98: 'pc98', x1: 'x1', x68000: 'x68000' };
/** Computers III (Task 10) named alike on ES-DE and Batocera. */
const COMPUTERS_III = {
  apple2: 'apple2',
  msx2: 'msx2',
  msxturbor: 'msxturbor',
};
/** Fantasy consoles (Task 11) named alike on ES-DE, Batocera and muOS. */
const FANTASY = {
  arduboy: 'arduboy',
  lowresnx: 'lowresnx',
  tic80: 'tic80',
  uzebox: 'uzebox',
  vircon32: 'vircon32',
  wasm4: 'wasm4',
};
/** Extra arcade (plan Task 13): ES-DE and Batocera only (spec D16). */
const EXTRA_ARCADE = {
  model2: 'model2',
  model3: 'model3',
  naomi2: 'naomi2',
  triforce: 'triforce',
};
const ONION_FOLDERS = {
  channelf: 'FAIRCHILD',
  colecovision: 'COLECO',
  intellivision: 'INTELLIVISION',
  odyssey2: 'ODYSSEY',
  vectrex: 'VECTREX',
  satellaview: 'SATELLAVIEW',
  sg1000: 'SEGASGONE',
  sufami: 'SUFAMI',
  supergrafx: 'SGFX',
  megaduck: 'MEGADUCK',
  supervision: 'SUPERVISION',
  amstradcpc: 'CPC',
  zxspectrum: 'ZXS',
  tic80: 'TIC',
};
const MINUI_FOLDERS = {
  amstradcpc: 'CPC',
  c128: 'C128',
  pet: 'PET',
  plus4: 'PLUS4',
  colecovision: 'COLECO',
  sg1000: 'SG1000',
  doom: 'PRBOOM',
};
/** D23 hosts: a sub-system kept in its parent's folder. */
const ANBERNIC_STOCK_HOSTS = {
  atarixe: 'a800',
  msx2: 'msx',
  satellaview: 'snes',
  supergrafx: 'pcengine',
};
const ONION_HOSTS = { msx2: 'msx' };
const MINUI_HOSTS = { msx2: 'msx', satellaview: 'snes' };
const MUOS_FOLDERS = {
  '3do': '3do',
  cdi: 'cdi',
  pcfx: 'pcfx',
  ...EARLY_CONSOLES,
  jaguar: 'jaguar',
  ...ADD_ONS,
  pico: 'segapico',
  sg1000: 'sg1000',
  ...HANDHELDS,
  ...COMPUTERS_I,
  dragon: 'dragon',
  thomson: 'thomson',
  zxspectrum: 'spectrum',
  ...COMPUTERS_II,
  pc88: 'pc8000',
  c128: 'c128',
  msx2: 'msx2',
  pet: 'pet',
  plus4: 'plus4',
  ...FANTASY,
  cavestory: 'cavestory',
  doom: 'doom',
  quake: 'quake',
  wolf3d: 'wolfenstein3d',
  // Systems batch 2 (spec §6).
  '3ds': '3ds',
};
const ESDE_FOLDERS = {
  '3do': '3do',
  amigacd32: 'amigacd32',
  cdi: 'cdimono1',
  cdtv: 'cdtv',
  fmtowns: 'fmtowns',
  jaguarcd: 'atarijaguarcd',
  pcfx: 'pcfx',
  ps2: 'ps2',
  ...EARLY_CONSOLES,
  arcadia: 'arcadia',
  jaguar: 'atarijaguar',
  ...ADD_ONS,
  sg1000: 'sg-1000',
  ...HANDHELDS,
  gamecom: 'gamecom',
  ...COMPUTERS_I,
  bbcmicro: 'bbcmicro',
  dragon: 'dragon32',
  thomson: 'moto',
  ti99: 'ti99',
  zxspectrum: 'zxspectrum',
  ...COMPUTERS_II,
  pc88: 'pc88',
  ...COMPUTERS_III,
  atarixe: 'atarixe',
  plus4: 'plus4',
  ...FANTASY,
  doom: 'doom',
  lutro: 'lutro',
  quake: 'quake',
  solarus: 'solarus',
  ...EXTRA_ARCADE,
  // Systems batch 2 (spec §6).
  gamecube: 'gc',
  wii: 'wii',
  '3ds': 'n3ds',
  wiiu: 'wiiu',
  switch: 'switch',
  psvita: 'psvita',
  ps3: 'ps3',
  xbox: 'xbox',
  xbox360: 'xbox360',
};
const BATOCERA_FOLDERS = {
  '3do': '3do',
  amigacd32: 'amigacd32',
  cdi: 'cdi',
  cdtv: 'amigacdtv',
  fmtowns: 'fmtowns',
  jaguarcd: 'jaguarcd',
  pcfx: 'pcfx',
  ps2: 'ps2',
  ...EARLY_CONSOLES,
  arcadia: 'arcadia',
  jaguar: 'jaguar',
  ...ADD_ONS,
  pico: 'pico',
  sg1000: 'sg1000',
  ...HANDHELDS,
  gamecom: 'gamecom',
  ...COMPUTERS_I,
  bbcmicro: 'bbcmicro',
  dragon: 'dragon64',
  thomson: 'thomson',
  ti99: 'ti99',
  zxspectrum: 'zxspectrum',
  ...COMPUTERS_II,
  pc88: 'pc88',
  ...COMPUTERS_III,
  atarixe: 'xegs',
  c128: 'c128',
  pet: 'pet',
  plus4: 'cplus4',
  ...FANTASY,
  cavestory: 'cavestory',
  doom: 'prboom',
  flashback: 'reminiscence',
  lutro: 'lutro',
  mrboom: 'mrboom',
  quake: 'quake',
  solarus: 'solarus',
  wolf3d: 'ecwolf',
  xrick: 'xrick',
  ...EXTRA_ARCADE,
  // Systems batch 2 (spec §6).
  gamecube: 'gamecube',
  wii: 'wii',
  '3ds': '3ds',
  wiiu: 'wiiu',
  switch: 'switch',
  psvita: 'psvita',
  ps3: 'ps3',
  xbox: 'xbox',
  xbox360: 'xbox360',
};

/** The built-in profiles of `profiles/builtin.rs`, as `list_profiles` sends
 * them (pinned to the Rust ones by the contract sample). */
export const MOCK_BUILTIN_PROFILES: readonly ProfileInfo[] = [
  handheld(
    'anbernic-stock',
    'Anbernic stock',
    'ROMS',
    undefined,
    ANBERNIC_STOCK_HOSTS,
  ),
  handheld('garlicos', 'GarlicOS', 'Roms', ONION_FOLDERS, ONION_HOSTS),
  handheld('onion', 'Onion OS', 'Roms', ONION_FOLDERS, ONION_HOSTS),
  {
    profile: {
      id: 'muos',
      folder_names: MUOS_FOLDERS,
      name: 'muOS',
      roms_root: 'ROMS',
      art_dirs: [],
      gamelist: false,
      system_folder: 'id',
      slots: {
        cover: png('{root}/MUOS/info/catalogue/{system_id}/box/{stem}.png'),
        screenshot: png(
          '{root}/MUOS/info/catalogue/{system_id}/preview/{stem}.png',
        ),
      },
    },
    builtin: true,
    mirror_subdirs: false,
  },
  {
    profile: {
      id: 'minui',
      accepts_containers: ['zip'],
      disc_layout: { kind: 'game-folder', m3u_for_multi: true },
      folder_names: MINUI_FOLDERS,
      host_folders: MINUI_HOSTS,
      name: 'MinUI / NextUI',
      roms_root: 'Roms',
      art_dirs: ['.media', '.res'],
      gamelist: false,
      slots: { cover: png('{system_dir}/{rel_dir}/.media/{stem}.png') },
    },
    builtin: true,
    mirror_subdirs: true,
  },
  {
    profile: {
      id: 'esde',
      folder_game_suffix: { ps3: '.ps3' },
      folder_stem_keeps_suffix: true,
      disc_layout: { kind: 'game-folder', m3u_for_multi: true },
      folder_names: ESDE_FOLDERS,
      name: 'ES-DE',
      roms_root: 'ROMs',
      art_dirs: [],
      gamelist: false,
      system_folder: 'id',
      slots: {
        cover: png(
          '{root}/downloaded_media/{system_id}/covers/{rel_dir}/{stem}.png',
        ),
        screenshot: png(
          '{root}/downloaded_media/{system_id}/screenshots/{rel_dir}/{stem}.png',
        ),
        title: png(
          '{root}/downloaded_media/{system_id}/titlescreens/{rel_dir}/{stem}.png',
        ),
      },
    },
    builtin: true,
    mirror_subdirs: true,
  },
  {
    profile: {
      id: 'batocera',
      folder_game_suffix: { ps3: '.ps3' },
      disc_layout: { kind: 'flat', m3u_for_multi: true },
      folder_names: BATOCERA_FOLDERS,
      name: 'Batocera / Knulli / EmulationStation',
      roms_root: 'roms',
      art_dirs: ['images', 'videos', 'manuals', 'magazines'],
      gamelist: true,
      system_folder: 'id',
      slots: {
        cover: png('{system_dir}/images/{stem}-image.png'),
        screenshot: png('{system_dir}/images/{stem}-screenshot.png'),
        title: png('{system_dir}/images/{stem}-titleshot.png'),
      },
    },
    builtin: true,
    mirror_subdirs: false,
  },
  {
    profile: {
      id: 'retroarch',
      disc_layout: { kind: 'flat', m3u_for_multi: true },
      name: 'RetroArch',
      roms_root: '',
      art_dirs: ['thumbnails'],
      gamelist: false,
      system_folder: 'id',
      slots: {
        cover: png('{root}/thumbnails/{libretro}/Named_Boxarts/{stem}.png'),
        screenshot: png('{root}/thumbnails/{libretro}/Named_Snaps/{stem}.png'),
        title: png('{root}/thumbnails/{libretro}/Named_Titles/{stem}.png'),
      },
    },
    builtin: true,
    mirror_subdirs: false,
  },
];

/** `window.localStorage` when the page may use it, else null. */
function defaultStorage(): Storage | null {
  try {
    return typeof window !== 'undefined' && window.localStorage
      ? window.localStorage
      : null;
  } catch {
    return null;
  }
}

const DAY = 86_400;

export function createMockBackend(opts: MockOptions = {}): MockBackend {
  const delayMs = opts.delayMs ?? 30;
  const now = opts.now ?? Math.floor(Date.now() / 1000);
  const sleep = (ms: number) =>
    ms > 0 ? new Promise<void>((r) => setTimeout(r, ms)) : Promise.resolve();
  const listeners = new Set<(p: ScanProgress) => void>();
  const revealed: number[] = [];
  let generation = 0;
  let current: LibraryView | null = null;
  const settings = createMockSettings({
    storage: opts.storage === undefined ? defaultStorage() : opts.storage,
    builtins: [...MOCK_BUILTIN_PROFILES],
    ...(opts.externalTools ? { externalTools: opts.externalTools } : {}),
  });
  const jobs = createMockJobs(
    {
      current: () => current,
      patch(updates) {
        if (!current) return;
        const byId = new Map(updates.map((u) => [u.id, u.art]));
        current = {
          ...current,
          entries: current.entries.map((e) => {
            const art = byId.get(e.id);
            return art ? { ...e, art } : e;
          }),
        };
      },
      tick: () => new Promise<void>((r) => setTimeout(r, delayMs)),
      now,
      sgdbReady: () => settings.isSourceReady('steamgriddb'),
      sgdbRefused: () => settings.isSourceRefused('steamgriddb'),
      organizing: () => organize.isApplying() || dedupe.isDedupeApplying(),
    },
    {
      pauseAfter: opts.pauseAfter === undefined ? 25 : opts.pauseAfter,
      pick: opts.pick ?? 'image',
    },
  );
  const organize = createMockOrganize(
    {
      settings: () => settings.current().organize,
      dedupe: () => settings.current().dedupe,
      profiles: () => settings.listProfiles(),
      fillHistory: () => jobs.jobHistory(),
      undoFill: (job) => jobs.undoJournal(job),
      fillBusy: () => jobs.isBusy() || dedupe.isDedupeApplying(),
      tick: () => new Promise<void>((r) => setTimeout(r, delayMs)),
      now,
      exports: jobs.exports,
    },
    {
      pauseAfter: opts.organizePauseAfter ?? null,
      extractRate: opts.extractRate ?? 'fast',
      sortedLayout: MOCK_BUILTIN_PROFILES.find(
        (p) => p.profile.id === 'anbernic-stock',
      ),
    },
  );
  const dedupe = createMockDedupe(
    {
      settings: () => settings.current().dedupe,
      otherBusy: () => jobs.isBusy() || organize.isApplying(),
      tick: () => new Promise<void>((r) => setTimeout(r, delayMs)),
      now,
      exports: jobs.exports,
      revealRoots: () => [
        ...(current ? [current.root] : []),
        ...organize.knownFolders(),
      ],
    },
    {
      trashAvailable: opts.dedupeTrashAvailable ?? true,
      pauseAfter: opts.dedupePauseAfter ?? null,
    },
  );
  const updateLog: string[] = [];
  const updates = createMockUpdates({
    scenario: opts.update ?? readMockUpdateScenario(),
    tick: () => sleep(delayMs),
    log: updateLog,
    jobBusy: () => jobs.isBusy(),
  });
  let recents: RecentLibrary[] = [
    ...MOCK_LIBRARIES.map((l, i) => ({
      path: l.path,
      name: l.path.split('/').pop() ?? l.path,
      profile_id: 'esde',
      roms: l.count,
      opened_at: now - i * DAY,
      available: true,
    })),
    {
      path: MOCK_EJECTED,
      name: 'EJECTED',
      profile_id: 'anbernic-stock',
      roms: 7473,
      opened_at: now - 9 * DAY,
      available: false,
    },
  ];

  const entryOf = (gen: number, id: number): EntryView => {
    if (!current) throw new BackendError('no-library');
    if (current.generation !== gen) throw new BackendError('stale-library');
    const e = current.entries[id];
    if (!e) throw new BackendError('unknown-entry', String(id));
    return e;
  };

  return {
    ...jobs,
    ...settings,
    ...updates,
    ...organize,
    ...dedupe,
    async jobHistory() {
      // Fill, organise and duplicates journals, newest first (ISO stamps
      // sort as text).
      const all = [
        ...(await jobs.jobHistory()),
        ...organize.history(),
        ...dedupe.dedupeHistory(),
      ];
      return all.sort((a, b) => b.created_at.localeCompare(a.created_at));
    },
    // Like Rust, `organize_undo` undoes duplicates journals too.
    organizeUndo: (job, includeLater) =>
      dedupe.ownsDedupeJob(job)
        ? dedupe.dedupeUndo(job, includeLater)
        : organize.organizeUndo(job, includeLater),
    listSystems: () =>
      Promise.resolve(
        // Like Rust, the whole catalogue: the organise piles' systems too.
        [...FIXTURE_SYSTEMS, ...ORGANIZE_SYSTEMS]
          .map((s) => ({
            id: s.id,
            name: s.name,
            family: s.family,
            no_emulator: 'no_emulator' in s && s.no_emulator === true,
          }))
          .sort((a, b) =>
            a.name.localeCompare(b.name, 'en', { sensitivity: 'base' }),
          ),
      ),
    updateLog,
    kind: 'mock',
    revealed,
    async openLibrary(path) {
      const gen = (generation += 1);
      const emit = (p: Omit<ScanProgress, 'generation'>) =>
        listeners.forEach((cb) => cb({ generation: gen, ...p }));
      // Like the real backend: announce the generation before any work.
      emit({ done: 0, total: 0, folder: '', roms: 0 });
      const lib = MOCK_LIBRARIES.find((l) => l.path === path);
      // A pile organised by the mock opens with its layout; a loose one is `no-profile`.
      const view = lib
        ? generateLibrary({
            seed: lib.seed,
            count: lib.count,
            root: lib.path,
            generation: gen,
            now,
          })
        : organize.library(path, gen);
      if (!view) throw new BackendError('not-found', path);
      let roms = 0;
      for (const [i, s] of view.systems.entries()) {
        await sleep(delayMs);
        roms += s.roms;
        emit({
          done: i + 1,
          total: view.systems.length,
          folder: s.folder,
          roms,
        });
      }
      if (gen !== generation) throw new BackendError('superseded');
      current = view;
      recents = [
        {
          path,
          name: view.name,
          profile_id: view.profile.id,
          roms: view.entries.length,
          opened_at: now,
          available: true,
        },
        ...recents.filter((r) => r.path !== path),
      ];
      return view;
    },
    onScanProgress(cb) {
      listeners.add(cb);
      return Promise.resolve(() => {
        listeners.delete(cb);
      });
    },
    recentLibraries: () => Promise.resolve([...recents]),
    forgetRecentLibrary(path) {
      recents = recents.filter((r) => r.path !== path);
      return Promise.resolve([...recents]);
    },
    mountedVolumes: () =>
      Promise.resolve([{ path: '/mock/Small library', name: 'Small library' }]),
    librarySettings: () =>
      Promise.resolve({
        ignored_dirs: settings.current().library.ignored_dirs,
      }),
    async setLibrarySettings(s) {
      // Like Rust: saved (normalised) through the settings update.
      const out = await settings.updateSettings({
        library: { ignored_dirs: s.ignored_dirs },
      });
      const first = out.errors[0];
      if (first)
        throw new BackendError('bad-request', `${first.field} is not valid`);
      return { ignored_dirs: out.settings.library.ignored_dirs };
    },
    async readArt(gen, id, media) {
      const e = entryOf(gen, id);
      if (e.art[media] !== 'present') {
        throw new BackendError('art-missing', media);
      }
      return {
        bytes: new TextEncoder().encode(mockArtSvg(e, media)),
        mime: 'image/svg+xml',
      };
    },
    async identifyEntry(gen, id) {
      const e = entryOf(gen, id);
      await sleep(delayMs);
      const manual = jobs.manualIdentity(gen, id);
      if (manual !== null) {
        return {
          canonical: manual,
          method: 'manual',
          confidence: 1,
          title: mockTitle(manual),
          regions: [],
          offline: false,
        };
      }
      const title = e.name.replace(/\s*\([^)]*\)/g, '').trim();
      return e.id % 3 === 0
        ? {
            canonical: `${title} (USA)`,
            method: 'hash',
            confidence: 1,
            title,
            regions: ['usa'],
            offline: false,
          }
        : {
            canonical: null,
            method: 'name',
            confidence: 0,
            title,
            regions: e.regions,
            offline: e.id % 3 === 2,
          };
    },
    async revealEntry(gen, id) {
      entryOf(gen, id);
      revealed.push(id);
    },
    pickFolder: () =>
      Promise.resolve(opts.folder ?? readMockPick() ?? MOCK_LIBRARIES[0].path),
    onFolderDrop(cb) {
      const over = (ev: Event) => ev.preventDefault();
      const drop = (ev: Event) => {
        ev.preventDefault();
        const dt = (ev as DragEvent).dataTransfer;
        const name = dt?.files[0]?.name || dt?.getData('text/plain') || '';
        if (name) cb(name.startsWith('/') ? name : `/mock/${name}`);
      };
      window.addEventListener('dragover', over);
      window.addEventListener('drop', drop);
      return Promise.resolve(() => {
        window.removeEventListener('dragover', over);
        window.removeEventListener('drop', drop);
      });
    },
  };
}
