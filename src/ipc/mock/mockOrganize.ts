/**
 * Fake organising of loose piles for the browser mock (`pnpm dev`, tests):
 * two deterministic piles that `openLibrary` answers with `no-profile`,
 * dry runs with the identification "why", decisions, an apply with
 * progress (steps of 4 items) that turns the pile into a library of the
 * chosen layout, pause/resume, cancel and undo (refused while later fill or
 * organise jobs wrote under the target, unless asked). Like Rust, an apply,
 * a resume or an undo drops every kept plan.
 *
 * - `MOCK_DOWNLOADS` mirrors the owner's real sample: 16 ROMs that go to
 *   GBA 6 / GB 6 / GBC 2 / SFC 2, a saved web page named like a ROM,
 *   3 partial downloads and a `.DS_Store`.
 * - `MOCK_MIXED` has 4 sure ROMs (one with a name that is markup), 2 for
 *   review, 1 unknown, 1 duplicate and 2 partial downloads.
 *
 * Only these two folders exist for organising; any other source is
 * `not-found`. Targets are not checked.
 *
 * Like Rust, a plan reuses the kept read of its pile (a survey) until the
 * request asks for `rescan`; `organizeTouch` adds or removes files (or the
 * whole folder) between scans, for tests and the browser check.
 */
import type { Backend } from '../backend';
import { BackendError } from '../errors';
import {
  MEDIA,
  type ArtMarks,
  type EntryView,
  type Evidence,
  type IdLevel,
  type JobHistoryItem,
  type LeftReason,
  type LibraryView,
  type Media,
  type NameMode,
  type OrganizeDecision,
  type OrganizeItem,
  type OrganizeItemOutcome,
  type OrganizeJobView,
  type OrganizePlan,
  type OrganizeProgressEvent,
  type OrganizeReport,
  type OrganizeRequest,
  type OrganizeSettings,
  type OrganizeUndoReport,
  type ProfileInfo,
  type Region,
  type SystemView,
  type TransferMode,
} from '../types';
import { FIXTURE_SYSTEMS } from './fixture';
import type { MockExport } from './mockJobs';

export const MOCK_DOWNLOADS = '/Users/demo/Downloads/roms';
export const MOCK_MIXED = '/Users/demo/Mixed';
export const MOCK_PLAN_EXPORT = 'coverly-organize-plan.json';
export const MOCK_PICK_KEY = 'coverly.mock.pick';

/**
 * What "Open folder…" picks in the browser mock, so the organise offer is
 * reachable from the start screen: `?mockPick=downloads|mixed|<absolute
 * path>` (or localStorage `coverly.mock.pick`). Null: the first fixture
 * library, as before.
 */
export function readMockPick(): string | null {
  const resolve = (v: string | null): string | null => {
    if (v === 'downloads') return MOCK_DOWNLOADS;
    if (v === 'mixed') return MOCK_MIXED;
    return v && v.startsWith('/') ? v : null;
  };
  try {
    const fromUrl = resolve(
      new URLSearchParams(window.location.search).get('mockPick'),
    );
    if (fromUrl) return fromUrl;
    return resolve(window.localStorage.getItem(MOCK_PICK_KEY));
  } catch {
    /* no window or storage */
    return null;
  }
}

/** Items moved between two apply progress events. */
const STEP = 4;

interface PileFile {
  name: string;
  size: number;
  /** Where the ladder placed it (absent: left before identification). */
  system?: string;
  method?: IdLevel;
  confidence?: number;
  detail?: string;
  /** Other systems: the item goes to review. */
  alternatives?: [string, number][];
  catalogue?: string;
  left?: LeftReason;
}

interface PileDef {
  path: string;
  files: PileFile[];
  junk: string[];
}

const hash = (
  system: string,
  catalogue: string,
  crc: string,
): Partial<PileFile> => ({
  system,
  method: 'hash',
  confidence: 1,
  detail: `${crc} ${catalogue}`,
  catalogue,
});
const header = (system: string, detail: string): Partial<PileFile> => ({
  system,
  method: 'header',
  confidence: 0.9,
  detail,
});
const GBC_HEADER = header('gbc', 'Nintendo logo, CGB 0xC0');
const SFC_HEADER = header('snes', 'LoROM header, checksum ok');
const PARTIAL: LeftReason = { kind: 'partial-download' };

const PILES: readonly PileDef[] = [
  {
    // The owner's real sample as the core plans it (docs/TRACKER.md, task
    // 35): the names mislead (`Tetris 2.zip` is the SNES game, `Tetris 2
    // (1).zip` the Game Boy one), the CRC settles it; no GBC catalogue is
    // cached, so GBC is decided by the header; `3-D Tetris` is a saved web
    // page. Plus the partial downloads a browser leaves behind.
    path: MOCK_DOWNLOADS,
    junk: ['.DS_Store'],
    files: [
      {
        name: '0319 - Crash Bandicoot XS (E)(Paracox).zip',
        size: 4_455_459,
        ...hash(
          'gba',
          'Crash Bandicoot XS (Europe) (En,Fr,De,Es,It,Nl)',
          'CCFD654F',
        ),
      },
      {
        name: '1490 - Sonic Advance 3 (U)(Venom).zip',
        size: 4_417_731,
        ...hash('gba', 'Sonic Advance 3 (USA) (En,Ja,Fr,De,Es,It)', '49DDA5E6'),
      },
      {
        name: '1792 - Tomb Raider - The Prophecy (U)(BatMan).zip',
        size: 4_503_552,
        ...hash(
          'gba',
          'Tomb Raider - The Prophecy (USA) (En,Fr,De,Es,It)',
          '556F40FA',
        ),
      },
      {
        name: 'Fire Emblem (E)(Independent).zip',
        size: 12_076_482,
        ...hash('gba', 'Fire Emblem (Europe) (En,Fr,De)', '9DECC754'),
      },
      {
        name: 'Pokemon Verde Hoja (S)(Rising Sun).zip',
        size: 5_300_837,
        ...hash('gba', 'Pokemon - Edicion Verde Hoja (Spain)', '2CA11D59'),
      },
      {
        name: 'Tetris Worlds (USA).zip',
        size: 1_602_371,
        ...hash('gba', 'Tetris Worlds (USA)', '7B729804'),
      },
      {
        name: 'Tetris (JUE) (V1.0) [!].zip',
        size: 19_459,
        ...hash('gb', 'Tetris (World)', '63F9407D'),
      },
      {
        name: 'Tetris (JUE) (V1.1) [!].zip',
        size: 19_543,
        ...hash('gb', 'Tetris (World) (Rev 1)', '46DF91AD'),
      },
      {
        name: 'Tetris 2 (1).zip',
        size: 60_699,
        ...hash('gb', 'Tetris 2 (USA, Europe)', 'EA19A9D9'),
      },
      {
        name: 'Tetris Attack (1).zip',
        size: 183_388,
        ...hash('gb', 'Tetris Attack (USA, Europe)', 'B76C769B'),
      },
      {
        name: 'Tetris Blast.zip',
        size: 65_877,
        ...hash('gb', 'Tetris Blast (USA, Europe)', '7BE5A73F'),
      },
      {
        name: 'Tetris Plus (1).zip',
        size: 80_891,
        ...hash('gb', 'Tetris Plus (USA, Europe)', 'DAFC3BFF'),
      },
      {
        name: 'Magical Tetris Challenge (USA).zip',
        size: 334_891,
        ...GBC_HEADER,
      },
      {
        name: 'Tetris DX.zip',
        size: 103_156,
        ...header('gbc', 'Nintendo logo, CGB 0x80'),
      },
      {
        name: 'Tetris 2.zip',
        size: 667_294,
        ...hash('snes', 'Tetris 2 (USA)', '3BC7054A'),
      },
      {
        name: 'Tetris Attack.zip',
        size: 720_633,
        ...hash('snes', 'Tetris Attack (USA) (En,Ja)', '6C128210'),
      },
      {
        name: '3-D Tetris (USA).zip',
        size: 5_900,
        left: { kind: 'not-a-rom', what: 'web-page' },
      },
      {
        name: 'Super Tetris 3 (Japan).zip.crdownload',
        size: 1_048_576,
        left: PARTIAL,
      },
      {
        name: 'Tetris Battle Gaiden (Japan).zip.crdownload',
        size: 524_288,
        left: PARTIAL,
      },
      { name: 'Unconfirmed 471203.crdownload', size: 0, left: PARTIAL },
    ],
  },
  {
    path: MOCK_MIXED,
    junk: ['Thumbs.db'],
    files: [
      {
        name: 'Super Mario Land (World).gb',
        size: 65_536,
        ...hash('gb', 'Super Mario Land (World) (Rev 1)', '2C27EC70'),
      },
      {
        name: 'Super Mario Land (World) (1).gb',
        size: 65_536,
        ...hash('gb', 'Super Mario Land (World) (Rev 1)', '2C27EC70'),
        left: { kind: 'duplicate', of: 'Super Mario Land (World).gb' },
      },
      {
        name: 'Metroid II - Return of Samus (World).gb',
        size: 262_144,
        ...hash('gb', 'Metroid II - Return of Samus (World)', 'DEE05370'),
      },
      {
        name: 'Sonic The Hedgehog (USA, Europe).md',
        size: 524_288,
        ...hash('megadrive', 'Sonic The Hedgehog (USA, Europe)', 'F9394E97'),
      },
      {
        name: '<img src=x onerror=alert(1)> Racer (USA).sfc',
        size: 1_048_576,
        ...SFC_HEADER,
      },
      {
        name: 'Pokemon Pinball.zip',
        size: 1_048_576,
        system: 'gb',
        method: 'extension',
        confidence: 0.55,
        detail: '.gb',
        alternatives: [['gbc', 0.45]],
      },
      {
        name: 'Columns.bin',
        size: 131_072,
        system: 'megadrive',
        method: 'extension',
        confidence: 0.5,
        detail: '.bin',
        alternatives: [['mastersystem', 0.3]],
      },
      { name: 'mystery.rom', size: 32_768, left: { kind: 'unknown' } },
      { name: 'Wario Land II (USA).zip.crdownload', size: 0, left: PARTIAL },
      { name: 'Kirby (USA).gbc.part', size: 12_288, left: PARTIAL },
    ],
  },
];

const collator = new Intl.Collator('en', {
  sensitivity: 'base',
  numeric: true,
});
const SYSTEMS = new Map(FIXTURE_SYSTEMS.map((s) => [s.id, s]));

function iso(secs: number): string {
  return new Date(secs * 1000).toISOString().replace(/\.\d{3}Z$/, 'Z');
}
const extOf = (name: string) => {
  const dot = name.lastIndexOf('.');
  return dot > 0 ? name.slice(dot) : '';
};
const stemOf = (name: string) => {
  const ext = extOf(name);
  return ext ? name.slice(0, -ext.length) : name;
};
const join = (dir: string, name: string) => (dir ? `${dir}/${name}` : name);
const under = (root: string, path: string) =>
  path === root || path.startsWith(`${root}/`);
/** One folder holds the other (Rust `later_jobs`: under or above). */
const overlap = (a: string, b: string) => under(a, b) || under(b, a);

const TAG_REGIONS: [RegExp, Region][] = [
  [/\((USA|U)[),]/, 'usa'],
  [/\((Europe|E)[),]|, Europe\)/, 'europe'],
  [/\((Japan|J)[),]/, 'japan'],
  [/\(World\)/, 'world'],
  [/\((Spain|S)\)/, 'spain'],
];
const regionsOf = (name: string): Region[] =>
  TAG_REGIONS.filter(([re]) => re.test(name)).map(([, r]) => r);

/** Options of one plan, the request resolved against the settings. */
interface Resolved {
  source: string;
  layout: ProfileInfo;
  target: string;
  /** The ROMs folder relative to the target ('' = flat). */
  romsRel: string;
  mode: TransferMode;
  names: NameMode;
  offline: boolean;
  overrides: Record<string, string>;
}

/** A file an apply put in a target. */
interface Placed {
  job: string;
  file: PileFile;
  system: string;
  /** Relative to the target root. */
  rel: string;
}

interface Target {
  root: string;
  layout: ProfileInfo;
  romsRel: string;
  files: Placed[];
}

interface OrgJournal {
  job: string;
  createdAt: string;
  source: string;
  target: string;
  layout: ProfileInfo;
  romsRel: string;
  mode: TransferMode;
  complete: boolean;
  undone: boolean;
  /** Fill journals that existed when it started: later ones block undo. */
  fillsBefore: Set<string>;
  placed: Placed[];
  /** Items not attempted yet (a pause), with their target. */
  remaining: { item: OrganizeItem; file: PileFile }[];
  pausedOnce: boolean;
}

export interface MockOrganizeHost {
  settings(): OrganizeSettings;
  profiles(): Promise<ProfileInfo[]>;
  /** The fill journals, newest first. */
  fillHistory(): Promise<JobHistoryItem[]>;
  undoFill(job: string): OrganizeUndoReport | null;
  /** A fill job gathers, applies or is paused. */
  fillBusy(): boolean;
  tick(): Promise<void>;
  now: number;
  exports: MockExport[];
}

export interface MockOrganizeOptions {
  /** The first apply of each job pauses (disk full) after this many items; null = never. */
  pauseAfter: number | null;
}

/** How `organizeTouch` changes a pile on "disk". */
export interface MockOrganizeTouch {
  /** File names to add, or how many made-up GBA ROMs. */
  add?: string[] | number;
  /** File names to remove, or how many loose files (in name order). */
  remove?: string[] | number;
  /** The folder disappears (true) or comes back (false). */
  gone?: boolean;
}

/** Placement of a file added by `organizeTouch`, by its extension. */
const ADDED_BY_EXT: Record<string, Partial<PileFile>> = {
  '.gba': header('gba', 'Nintendo logo, GBA header'),
  '.gb': header('gb', 'Nintendo logo, DMG'),
  '.gbc': GBC_HEADER,
  '.sfc': SFC_HEADER,
  '.smc': SFC_HEADER,
  '.md': header('megadrive', 'SEGA header'),
  '.nes': header('nes', 'iNES header'),
};

type OrganizeApi = Pick<
  Backend,
  | 'probeFolder'
  | 'organizePlan'
  | 'organizeCancel'
  | 'organizeApply'
  | 'organizeResume'
  | 'organizeCancelApply'
  | 'organizeUndo'
  | 'organizeExportPlan'
  | 'onOrganizeProgress'
  | 'onOrganizeChanged'
> & {
  /** An apply or resume runs (fills refuse to start meanwhile). */
  isApplying(): boolean;
  /** The organise journals, newest first. */
  history(): JobHistoryItem[];
  /** The organised library at `path`; `no-profile` for a pile not organised
   * there, null for a folder the mock does not know. */
  library(path: string, generation: number): LibraryView | null;
  /** Dev and tests only: changes pile `path` on "disk" between scans. */
  organizeTouch(path: string, change: MockOrganizeTouch): void;
};

export function createMockOrganize(
  host: MockOrganizeHost,
  opts: MockOrganizeOptions,
): OrganizeApi {
  const progress = new Set<(e: OrganizeProgressEvent) => void>();
  const changed = new Set<(v: OrganizeJobView) => void>();
  /** Files still loose in each pile. */
  const loose = new Map(
    PILES.map((p) => [p.path, new Set(p.files.map((f) => f.name))]),
  );
  /** Files `organizeTouch` added to each pile. */
  const added = new Map<string, PileFile[]>(PILES.map((p) => [p.path, []]));
  /** Piles whose folder `organizeTouch` removed. */
  const gone = new Set<string>();
  /** The kept read of each pile: the names its last survey found. */
  const surveyed = new Map<string, Set<string>>();
  let madeUp = 0;
  const targets = new Map<string, Target>();
  const journals: OrgJournal[] = [];
  const plans = new Map<number, Resolved>();
  const cancelled = new Set<number>();
  let seq = 0;
  let applying: { view: OrganizeJobView; cancel: boolean } | null = null;

  const emitProgress = (e: OrganizeProgressEvent) => {
    if (applying && applying.view.plan_id === e.plan_id)
      applying.view.progress = e;
    progress.forEach((cb) => cb({ ...e }));
  };
  const emitChanged = (v: OrganizeJobView) => {
    changed.forEach((cb) => cb(structuredClone(v)));
  };
  const listen = <T>(set: Set<(v: T) => void>, cb: (v: T) => void) => {
    const own = (v: T) => cb(v);
    set.add(own);
    return Promise.resolve(() => {
      set.delete(own);
    });
  };

  function pile(path: string): PileDef {
    const p = PILES.find((d) => d.path === path);
    if (!p || gone.has(path)) throw new BackendError('not-found', path);
    return p;
  }

  /** Every file the pile ever had, added ones included. */
  function allFiles(path: string): PileFile[] {
    return [...pile(path).files, ...(added.get(path) ?? [])];
  }

  /** Like Rust: the pile changed, so no kept plan or survey stays valid. */
  function forget() {
    plans.clear();
    surveyed.clear();
  }

  function touch(path: string, change: MockOrganizeTouch) {
    if (!PILES.some((d) => d.path === path))
      throw new BackendError('not-found', path);
    if (change.gone !== undefined) {
      if (change.gone) gone.add(path);
      else gone.delete(path);
    }
    const names = loose.get(path)!;
    const extra = added.get(path)!;
    const toAdd =
      typeof change.add === 'number'
        ? Array.from({ length: change.add }, () => {
            madeUp += 1;
            return `Added ROM ${madeUp} (USA).gba`;
          })
        : (change.add ?? []);
    for (const name of toAdd) {
      if (!extra.some((f) => f.name === name)) {
        const known = ADDED_BY_EXT[extOf(name)];
        extra.push(
          known
            ? { name, size: 4_194_304, ...known }
            : { name, size: 32_768, left: { kind: 'unknown' } },
        );
      }
      names.add(name);
    }
    const toRemove =
      typeof change.remove === 'number'
        ? [...names]
            .sort((a, b) => collator.compare(a, b))
            .slice(0, change.remove)
        : (change.remove ?? []);
    for (const name of toRemove) names.delete(name);
  }

  async function resolve(req: OrganizeRequest): Promise<Resolved> {
    pile(req.source);
    const s = host.settings();
    const id = req.layout ?? s.default_layout;
    const layout = (await host.profiles()).find((p) => p.profile.id === id);
    if (!layout) throw new BackendError('unknown-profile', id);
    const nest = req.flat == null ? s.nest_in_roms_root : !req.flat;
    return {
      source: req.source,
      layout,
      target: req.target ?? req.source,
      romsRel: nest ? layout.profile.roms_root : '',
      mode: req.mode ?? (s.mode === 'copy' ? 'copy' : 'move'),
      names: req.names ?? s.names,
      offline: req.offline ?? false,
      overrides: s.folder_names[id] ?? {},
    };
  }

  function folderFor(r: Resolved, system: string): string {
    const p = r.layout.profile;
    const sys = SYSTEMS.get(system);
    return (
      r.overrides[system] ??
      p.folder_names?.[system] ??
      (p.system_folder === 'id'
        ? system
        : (sys?.folder ?? system.toUpperCase()))
    );
  }

  function baseItem(file: PileFile, id: number): OrganizeItem {
    const identified =
      file.system !== undefined && file.left?.kind !== 'partial-download';
    const why: Evidence[] =
      identified && file.method
        ? [
            {
              kind: file.method,
              system: file.system!,
              weight: file.confidence ?? 0,
              detail: file.detail ?? '',
            },
          ]
        : [];
    const common = {
      id,
      sources: [file.name],
      kind:
        extOf(file.name) === '.zip' ? ('archive' as const) : ('file' as const),
      size: file.size,
      fps: [{ size: file.size, mtime_ns: host.now * 1e9 - (id + 1) * 6e10 }],
      target: null,
      set_targets: [],
      system_id: identified && !file.left ? file.system! : null,
      folder: null,
      confidence: identified ? (file.confidence ?? 0) : 0,
      method: identified ? (file.method ?? null) : null,
      why,
      alternatives: file.alternatives
        ? file.alternatives.map(([s, w]) => [s, w] as [string, number])
        : [],
      rename: null,
      notes: [],
      companions: [],
      catalogue_name: file.catalogue ?? null,
      transfer: { kind: 'as-is' } as const,
      disc: null,
    };
    if (file.left)
      return {
        ...common,
        action: 'left',
        reason: { ...file.left },
      } as OrganizeItem;
    if (file.alternatives) return { ...common, action: 'review' };
    return { ...common, action: 'auto' };
  }

  /** The loose files of a pile, in name order, with their item. */
  function looseFiles(r: Resolved): PileFile[] {
    const names = surveyed.get(r.source) ?? loose.get(r.source)!;
    return (
      allFiles(r.source)
        .filter((f) => names.has(f.name))
        // Like Rust, a copy whose original is gone is the only one left,
        // so it is filed like any other ROM.
        .map((f): PileFile => {
          if (f.left?.kind !== 'duplicate' || names.has(f.left.of)) return f;
          const { left: _copyOf, ...only } = f;
          return only;
        })
        .sort((a, b) => collator.compare(a.name, b.name))
    );
  }

  function decide(
    items: OrganizeItem[],
    decisions: readonly OrganizeDecision[],
  ) {
    // Like Rust: every decision is checked before any is taken.
    for (const d of decisions) {
      if (!items[d.item])
        throw new BackendError(
          'bad-request',
          `item ${d.item} is not in the plan`,
        );
      if (d.choice === 'assign' && !SYSTEMS.has(d.system)) {
        throw new BackendError('bad-request', `unknown system \`${d.system}\``);
      }
    }
    for (const d of decisions) {
      const item = items[d.item]!;
      const decidable =
        item.action === 'review' ||
        (item.action === 'left' && item.reason.kind === 'unknown');
      if (d.choice === 'leave') {
        items[d.item] = leave(item, { kind: 'unknown' });
        continue;
      }
      if (!decidable) continue;
      if (d.choice === 'assign') {
        items[d.item] = {
          ...item,
          action: 'auto',
          why: [
            { kind: 'user', system: d.system, weight: 1, detail: '' },
            ...item.why,
          ],
          alternatives: item.alternatives.filter(([s]) => s !== d.system),
          system_id: d.system,
          method: 'user',
          confidence: 1,
        } as OrganizeItem;
      } else if (item.system_id !== null) {
        items[d.item] = { ...item, action: 'auto' } as OrganizeItem;
      }
    }
  }

  function leave(item: OrganizeItem, reason: LeftReason): OrganizeItem {
    return {
      ...item,
      action: 'left',
      reason,
      target: null,
      set_targets: [],
      companions: [],
      rename: null,
    } as OrganizeItem;
  }

  function buildPlan(r: Resolved, decisions: readonly OrganizeDecision[]) {
    const files = looseFiles(r);
    const items = files.map((f, i) => baseItem(f, i));
    decide(items, decisions);
    const existing = targets.get(r.target);
    const taken = new Set(
      existing ? existing.files.map((p) => p.rel.toLowerCase()) : [],
    );
    for (const [i, item] of items.entries()) {
      if (item.action === 'left' || item.system_id === null) continue;
      const folder = folderFor(r, item.system_id);
      const dir = join(r.romsRel, folder);
      if (item.action === 'review') {
        items[i] = { ...item, folder };
        continue;
      }
      const source = files[i]!.name;
      let name = source;
      let rename: OrganizeItem['rename'] = null;
      if (
        r.names === 'catalogue' &&
        item.catalogue_name &&
        stemOf(source) !== item.catalogue_name
      ) {
        name = `${item.catalogue_name}${extOf(source)}`;
        rename = { kind: 'catalogue', from: source };
      }
      // Never overwrite: case-insensitive, like a FAT card or macOS.
      const wanted = name;
      for (let n = 2; taken.has(join(dir, name).toLowerCase()); n += 1) {
        name = `${stemOf(wanted)} (${n})${extOf(wanted)}`;
        rename = { kind: 'collision', name };
      }
      taken.add(join(dir, name).toLowerCase());
      items[i] = { ...item, folder, target: join(dir, name), rename };
    }
    return { items, files };
  }

  function planOf(
    r: Resolved,
    decisions: readonly OrganizeDecision[],
  ): {
    plan: OrganizePlan;
    files: PileFile[];
  } {
    const { items, files } = buildPlan(r, decisions);
    const moving = items.filter((i) => i.action !== 'left');
    const systems = [
      ...new Set(moving.flatMap((i) => (i.system_id ? [i.system_id] : []))),
    ].sort();
    const sortedHere =
      r.target === r.source ? targets.get(r.source) : undefined;
    const sortedFolders: [string, string][] = sortedHere
      ? [
          ...new Map(
            sortedHere.files.map((p) => [
              p.rel.slice(0, p.rel.lastIndexOf('/')),
              p.system,
            ]),
          ).entries(),
        ].sort(([a], [b]) => collator.compare(a, b))
      : [];
    const warnings: OrganizePlan['warnings'] = r.offline
      ? systems.map((system) => ({
          kind: 'catalogue-missing' as const,
          system,
        }))
      : [];
    if (moving.length === 0 && sortedFolders.length > 0)
      warnings.push({ kind: 'sorted-only' });
    const plan: OrganizePlan = {
      source_root: r.source,
      target_root: r.target,
      roms_dir: r.romsRel ? `${r.target}/${r.romsRel}` : r.target,
      layout: r.layout.profile.id,
      mode: r.mode,
      names: r.names,
      items,
      counts: {
        roms: moving.length,
        systems: systems.length,
        auto: items.filter((i) => i.action === 'auto').length,
        review: items.filter((i) => i.action === 'review').length,
        left: items.filter((i) => i.action === 'left').length,
        junk: pile(r.source).junk.length,
        other_files: 0,
        deep_folders: 0,
        bytes: moving.reduce((n, i) => n + i.size, 0),
      },
      sorted_folders: sortedFolders,
      deep_folders: [],
      other_files: [],
      warnings,
      broad_root: false,
      fat32: false,
      move_allowed: true,
      target_detected: null,
      created_at: iso(host.now),
    };
    return { plan, files };
  }

  function stored(planId: number): Resolved {
    const r = plans.get(planId);
    if (!r) throw new BackendError('no-plan');
    return r;
  }

  function journalOf(job: string): OrgJournal | undefined {
    return journals.find((j) => j.job === job);
  }

  function report(
    job: OrgJournal,
    outcomes: OrganizeItemOutcome[],
  ): OrganizeReport {
    const count = (s: string) => outcomes.filter((o) => o.status === s).length;
    return {
      job: job.job,
      moved: count('moved'),
      copied: count('copied'),
      skipped: count('skipped'),
      failed: count('failed'),
      items: outcomes,
      paused: null,
      pause_reason: null,
      resumable_items: [],
    };
  }

  /** Moves or copies `work` into the target, STEP items per tick. */
  async function run(
    j: OrgJournal,
    planId: number,
    work: { item: OrganizeItem; file: PileFile }[],
    outcomes: OrganizeItemOutcome[],
  ) {
    const own = applying!;
    const target = targets.get(j.target)!;
    const total = work.length;
    emitProgress({ plan_id: planId, phase: 'apply', done: 0, total });
    let done = 0;
    let state: OrganizeJobView['state'] = 'done';
    const out = [...outcomes];
    let paused: OrganizeReport['paused'] = null;
    try {
      for (; done < total;) {
        await host.tick();
        if (own.cancel) {
          state = 'cancelled';
          break;
        }
        const stop = Math.min(done + STEP, total);
        for (; done < stop; done += 1) {
          if (
            opts.pauseAfter !== null &&
            !j.pausedOnce &&
            j.placed.length === opts.pauseAfter
          ) {
            j.pausedOnce = true;
            paused = 'volume';
            break;
          }
          const { item, file } = work[done]!;
          const placed: Placed = {
            job: j.job,
            file,
            system: item.system_id!,
            rel: item.target!,
          };
          j.placed.push(placed);
          target.files.push(placed);
          if (j.mode === 'move') loose.get(j.source)!.delete(file.name);
          out.push({
            item: item.id,
            status: j.mode === 'move' ? 'moved' : 'copied',
            targets: [`${j.target}/${item.target!}`],
          });
        }
        if (paused) {
          state = 'paused';
          break;
        }
        emitProgress({ plan_id: planId, phase: 'apply', done, total });
      }
    } catch (e) {
      state = 'failed';
      own.view.error = { code: e instanceof BackendError ? e.code : 'io' };
    }
    const rest = work.slice(done);
    j.remaining = state === 'paused' ? rest : [];
    if (state === 'cancelled') {
      for (const { item } of rest) {
        out.push({
          item: item.id,
          status: 'skipped',
          reason: 'cancelled',
          targets: [],
        });
      }
    }
    j.complete = state === 'done' || state === 'cancelled';
    const r = report(j, out);
    if (paused) {
      r.paused = paused;
      r.pause_reason = 'disk-full';
      r.resumable_items = rest.map((w) => w.item.id);
    }
    if (target.files.length === 0) targets.delete(target.root);
    applying = null;
    emitChanged({ ...own.view, job: j.job, state, progress: null, report: r });
  }

  function notApplying() {
    if (applying || host.fillBusy()) throw new BackendError('job-running');
  }

  function start(planId: number, j: OrgJournal): OrganizeJobView {
    const view: OrganizeJobView = {
      job: null,
      plan_id: planId,
      state: 'applying',
      progress: null,
      report: null,
      target_root: j.target,
      layout: j.layout.profile.id,
      error: null,
    };
    applying = { view, cancel: false };
    return { ...view };
  }

  function libraryView(t: Target, generation: number): LibraryView {
    const p = t.layout.profile;
    const media = MEDIA.filter((m) => p.slots[m] !== undefined);
    const art = Object.fromEntries(
      MEDIA.map((m) => [m, media.includes(m) ? 'missing' : 'unsupported']),
    ) as unknown as ArtMarks;
    const bySystem = new Map<string, Placed[]>();
    for (const f of t.files) {
      const folder = f.rel.slice(0, f.rel.lastIndexOf('/'));
      bySystem.set(folder, [...(bySystem.get(folder) ?? []), f]);
    }
    const dirs = [...bySystem.keys()].sort((a, b) => collator.compare(a, b));
    const systems: SystemView[] = [];
    const entries: EntryView[] = [];
    dirs.forEach((dir, index) => {
      const files = [...bySystem.get(dir)!].sort((a, b) =>
        collator.compare(a.rel, b.rel),
      );
      const first = files[0]!;
      systems.push({
        index,
        folder: dir.slice(dir.lastIndexOf('/') + 1),
        dir,
        system_id: first.system,
        name: SYSTEMS.get(first.system)?.name ?? first.system,
        roms: files.length,
      });
      for (const f of files) {
        const file = f.rel.slice(f.rel.lastIndexOf('/') + 1);
        entries.push({
          id: entries.length,
          system: index,
          rel_path: f.rel,
          dir,
          name: stemOf(file),
          ext: extOf(file).slice(1),
          size: f.file.size,
          mtime: host.now - 3_600,
          regions: regionsOf(file),
          art: { ...art },
        });
      }
    });
    return {
      generation,
      root: t.root,
      name: t.root.split('/').filter(Boolean).pop() ?? t.root,
      profile: {
        id: p.id,
        name: p.name,
        media: [...media] as Media[],
        detected: true,
        evidence: ['mock organised'],
        fitted: false,
        gamelist: p.gamelist,
      },
      systems,
      entries,
      warnings: [],
      scan_ms: 0,
      loose_files: 0,
    };
  }

  /** The organise journals, newest first. */
  function historyItems(): JobHistoryItem[] {
    return [...journals].reverse().map((j) => ({
      job: j.job,
      root: j.target,
      created_at: j.createdAt,
      entries: j.placed.length,
      complete: j.complete,
      undone: j.undone,
      kind: 'organize' as const,
      undoable: true,
    }));
  }

  /** Undoes organise journal `j` alone: moves back or removes its files. */
  function undoOne(j: OrgJournal): OrganizeUndoReport {
    const t = targets.get(j.target);
    const restored: string[] = [];
    const removed: string[] = [];
    for (const p of [...j.placed].reverse()) {
      if (j.mode === 'move') {
        loose.get(j.source)!.add(p.file.name);
        restored.push(`${j.source}/${p.file.name}`);
      } else {
        removed.push(`${j.target}/${p.rel}`);
      }
    }
    const dirsRemoved: string[] = [];
    if (t) {
      const before = new Set(
        t.files.map((p) => p.rel.slice(0, p.rel.lastIndexOf('/'))),
      );
      t.files = t.files.filter((p) => p.job !== j.job);
      const after = new Set(
        t.files.map((p) => p.rel.slice(0, p.rel.lastIndexOf('/'))),
      );
      for (const dir of [...before].sort((a, b) => collator.compare(a, b))) {
        if (!after.has(dir)) dirsRemoved.push(`${j.target}/${dir}`);
      }
      if (t.files.length === 0) {
        if (t.romsRel) dirsRemoved.push(`${j.target}/${t.romsRel}`);
        targets.delete(j.target);
      }
    }
    j.undone = true;
    j.remaining = [];
    return {
      job: j.job,
      removed,
      restored,
      skipped_modified: [],
      skipped_occupied: [],
      dirs_removed: dirsRemoved,
    };
  }

  return {
    isApplying: () => applying !== null,
    organizeTouch: touch,
    history: historyItems,
    library(path, generation) {
      const t = targets.get(path);
      if (t) return libraryView(t, generation);
      if (PILES.some((p) => p.path === path))
        throw new BackendError('no-profile', path);
      return null;
    },
    async probeFolder(path) {
      const names = loose.get(path)!;
      const files = allFiles(path).filter((f) => names.has(f.name));
      const partial = files.filter(
        (f) => f.left?.kind === 'partial-download',
      ).length;
      const t = targets.get(path);
      const folders = t
        ? [...new Set(t.files.map((f) => f.rel.split('/').at(-2)!))].sort(
            (a, b) => collator.compare(a, b),
          )
        : [];
      return {
        rom_like: files.length - partial,
        partial_downloads: partial,
        cloud_only: 0,
        sorted_folders: folders,
        writable: true,
        broad_root: false,
        truncated: false,
      };
    },
    async organizePlan(request, decisions = []) {
      const r = await resolve(request);
      // A new read of the pile, or the kept one (like Rust's survey).
      if (request.rescan || !surveyed.has(r.source))
        surveyed.set(r.source, new Set(loose.get(r.source)));
      seq += 1;
      const planId = seq;
      const total = looseFiles(r).length;
      const step = async (phase: 'walk' | 'identify', done: number) => {
        progress.forEach((cb) => cb({ plan_id: planId, phase, done, total }));
        await host.tick();
        if (cancelled.has(planId)) {
          cancelled.delete(planId);
          throw new BackendError('cancelled');
        }
      };
      await step('walk', 0);
      for (let done = 0; ; done = Math.min(done + STEP, total)) {
        await step('identify', done);
        if (done === total) break;
      }
      const { plan } = planOf(r, decisions);
      plans.set(planId, r);
      return { plan_id: planId, plan };
    },
    async organizeCancel(planId) {
      cancelled.add(planId);
    },
    async organizeApply(planId, decisions) {
      const r = stored(planId);
      notApplying();
      const { plan, files } = planOf(r, decisions);
      const fills = await host.fillHistory();
      notApplying();
      // Like Rust: the pile changes, so no kept plan stays valid.
      forget();
      const outcomes: OrganizeItemOutcome[] = plan.items
        .filter((i) => i.action === 'review')
        .map((i) => ({
          item: i.id,
          status: 'skipped',
          reason: 'not-decided',
          targets: [],
        }));
      const work = plan.items.flatMap((item, i) =>
        item.action === 'auto' ? [{ item, file: files[i]! }] : [],
      );
      const j: OrgJournal = {
        job: `organize-${String(journals.length + 1).padStart(4, '0')}`,
        // Between the fill journals written before and after it.
        // (+ its index: two organise jobs in a row still sort apart).
        createdAt: iso(host.now + fills.length * 60 + 30 + journals.length),
        source: r.source,
        target: r.target,
        layout: r.layout,
        romsRel: r.romsRel,
        mode: r.mode,
        complete: false,
        undone: false,
        fillsBefore: new Set(fills.map((f) => f.job)),
        placed: [],
        remaining: [],
        pausedOnce: false,
      };
      journals.push(j);
      if (!targets.has(r.target)) {
        targets.set(r.target, {
          root: r.target,
          layout: r.layout,
          romsRel: r.romsRel,
          files: [],
        });
      }
      const view = start(planId, j);
      void run(j, planId, work, outcomes);
      return view;
    },
    async organizeResume(job) {
      const j = journalOf(job);
      // Rust: no saved plan for the id → `no-plan`; a finished one → `bad-request`.
      if (!j) throw new BackendError('no-plan', job);
      if (j.undone || j.complete) {
        throw new BackendError(
          'bad-request',
          `job ${job} has nothing left to resume`,
        );
      }
      notApplying();
      forget();
      seq += 1;
      const planId = seq;
      if (!targets.has(j.target)) {
        targets.set(j.target, {
          root: j.target,
          layout: j.layout,
          romsRel: j.romsRel,
          files: [],
        });
      }
      const work = j.remaining;
      const view = start(planId, j);
      view.job = j.job;
      applying!.view.job = j.job;
      void run(j, planId, work, []);
      return view;
    },
    async organizeCancelApply() {
      if (applying) applying.cancel = true;
    },
    async organizeUndo(job, includeLater) {
      // Rust takes the manual-write guard: refused while an apply runs or
      // a fill gathers, applies or is paused.
      notApplying();
      const j = journalOf(job);
      if (!j || j.undone) throw new BackendError('nothing-to-undo');
      // Rust `later_jobs`: newer, not undone, writing under (or above) its
      // root, or taking files from there — fills and organise jobs alike.
      const touches = (root: string, source?: string) =>
        overlap(j.target, root) ||
        (source !== undefined && overlap(j.target, source));
      const laterFills = (await host.fillHistory()).filter(
        (f) => !f.undone && !j.fillsBefore.has(f.job) && touches(f.root),
      );
      const laterOrganise = journals.filter(
        (o) =>
          journals.indexOf(o) > journals.indexOf(j) &&
          !o.undone &&
          touches(o.target, o.source),
      );
      const historyOf = new Map(historyItems().map((h) => [h.job, h] as const));
      const later: JobHistoryItem[] = [
        ...laterFills,
        ...laterOrganise.map((o) => historyOf.get(o.job)!),
      ].sort((a, b) => b.created_at.localeCompare(a.created_at));
      if (later.length > 0 && !includeLater) {
        throw new BackendError(
          'later-jobs',
          later.map((f) => f.job).join(', '),
        );
      }
      const reports: OrganizeUndoReport[] = [];
      for (const f of later) {
        const o = journalOf(f.job);
        const r = o ? undoOne(o) : host.undoFill(f.job);
        if (r) reports.push(r);
      }
      reports.push(undoOne(j));
      forget();
      return { report: reports, later_jobs: later };
    },
    async organizeExportPlan(planId) {
      const r = stored(planId);
      const { plan } = planOf(r, []);
      host.exports.push({
        name: MOCK_PLAN_EXPORT,
        text: JSON.stringify(plan, null, 2),
      });
      return MOCK_PLAN_EXPORT;
    },
    onOrganizeProgress: (cb) => listen(progress, cb),
    onOrganizeChanged: (cb) => listen(changed, cb),
  };
}
