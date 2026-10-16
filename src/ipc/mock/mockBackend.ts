/**
 * Browser stand-in for the Tauri backend (`pnpm dev` in Chrome, tests):
 * deterministic fixture libraries, fake progress, SVG art, fake identity.
 */
import type { Backend } from '../backend';
import { BackendError } from '../errors';
import type {
  EntryView,
  LibrarySettings,
  LibraryView,
  Media,
  RecentLibrary,
  ScanProgress,
} from '../types';
import { generateLibrary } from './fixture';
import { createMockJobs, type MockExport } from './mockJobs';
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
}

export interface MockBackend extends Backend {
  /** Entry ids passed to `revealEntry`, for tests. */
  readonly revealed: number[];
  /** Files "saved" by the exports, for tests. */
  readonly exports: MockExport[];
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

function normaliseDirs(dirs: readonly string[]): string[] {
  const out: string[] = [];
  for (const raw of dirs) {
    const d = raw.trim();
    if (!d || d === '.' || d === '..' || /[\\/]/.test(d)) continue;
    if (out.some((o) => o.toLowerCase() === d.toLowerCase())) continue;
    out.push(d);
  }
  return out;
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
    },
    {
      pauseAfter: opts.pauseAfter === undefined ? 25 : opts.pauseAfter,
      pick: opts.pick ?? 'image',
    },
  );
  let settings: LibrarySettings = {
    ignored_dirs: ['bios', 'saves', 'states', 'Shortcuts'],
  };
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
    kind: 'mock',
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
    librarySettings: () => Promise.resolve({ ...settings }),
    setLibrarySettings(s) {
      settings = { ignored_dirs: normaliseDirs(s.ignored_dirs) };
      return Promise.resolve({ ...settings });
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
