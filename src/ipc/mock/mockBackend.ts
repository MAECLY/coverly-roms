/**
 * Browser stand-in for the Tauri backend (`pnpm dev` in Chrome, tests):
 * deterministic fixture libraries, fake progress, SVG art, fake identity.
 */
import { version as APP_VERSION } from '../../../package.json';
import type { Backend } from '../backend';
import { BackendError } from '../errors';
import type {
  EntryView,
  LibraryView,
  Media,
  ProfileInfo,
  RecentLibrary,
  ScanProgress,
} from '../types';
import { generateLibrary } from './fixture';
import { createMockJobs, mockTitle, type MockExport } from './mockJobs';
import { createMockSettings } from './mockSettings';
import { escapeXml } from './svg';

export { escapeXml } from './svg';
export type { MockExport } from './mockJobs';

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
}

export interface MockBackend extends Backend {
  /** Entry ids passed to `revealEntry`, for tests. */
  readonly revealed: number[];
  /** Files "saved" by the exports, for tests. */
  readonly exports: MockExport[];
  /** Folders and links "opened" (`folder:logs`, `link:repo`), for tests. */
  readonly opened: string[];
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

const handheld = (id: string, name: string, romsRoot: string): ProfileInfo => ({
  profile: {
    id,
    name,
    roms_root: romsRoot,
    art_dirs: ['Imgs'],
    gamelist: false,
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

/** The built-in profiles of `profiles/builtin.rs`, as `list_profiles` sends
 * them (pinned to the Rust ones by the contract sample). */
export const MOCK_BUILTIN_PROFILES: readonly ProfileInfo[] = [
  handheld('anbernic-stock', 'Anbernic stock', 'ROMS'),
  handheld('garlicos', 'GarlicOS', 'Roms'),
  handheld('onion', 'Onion OS', 'Roms'),
  {
    profile: {
      id: 'muos',
      name: 'muOS',
      roms_root: 'ROMS',
      art_dirs: [],
      gamelist: false,
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
      name: 'ES-DE',
      roms_root: 'ROMs',
      art_dirs: [],
      gamelist: false,
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
      name: 'Batocera / Knulli / EmulationStation',
      roms_root: 'roms',
      art_dirs: ['images', 'videos', 'manuals', 'magazines'],
      gamelist: true,
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
      name: 'RetroArch',
      roms_root: '',
      art_dirs: ['thumbnails'],
      gamelist: false,
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
    },
    {
      pauseAfter: opts.pauseAfter === undefined ? 25 : opts.pauseAfter,
      pick: opts.pick ?? 'image',
    },
  );
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
    kind: 'mock',
    appInfo: () =>
      Promise.resolve({
        name: 'Coverly ROMs',
        version: APP_VERSION,
        core_version: APP_VERSION,
      }),
    revealed,
    async openLibrary(path) {
      const gen = (generation += 1);
      const emit = (p: Omit<ScanProgress, 'generation'>) =>
        listeners.forEach((cb) => cb({ generation: gen, ...p }));
      // Like the real backend: announce the generation before any work.
      emit({ done: 0, total: 0, folder: '', roms: 0 });
      const lib = MOCK_LIBRARIES.find((l) => l.path === path);
      if (!lib) throw new BackendError('not-found', path);
      const view = generateLibrary({
        seed: lib.seed,
        count: lib.count,
        root: lib.path,
        generation: gen,
        now,
      });
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
    pickFolder: () => Promise.resolve(MOCK_LIBRARIES[0].path),
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
