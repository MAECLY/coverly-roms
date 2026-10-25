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
 *   GBA 6 / GB 6 / GBC 2 / SFC 2, three disc games in 7z archives named
 *   `.zip` (PS1, Dreamcast, Saturn; `discFixture.ts`) that are extracted,
 *   a saved web page named like a ROM, 3 partial downloads and a
 *   `.DS_Store`.
 * - `MOCK_SAMPLE` is that sample as it is now (2026-10-24): partly
 *   organised, so it opens as an Anbernic stock library (`ROMS/GB`, `GBA`,
 *   `GBC`, `SFC` of `sampleFixture.ts`, never touched) whose loose files are
 *   5 GBA zips (two ROM hacks, a ` (1)` copy), the three disc archives, the
 *   web page, a patch-only zip and a `.rar` no tool can open.
 * - `MOCK_MIXED` has 4 sure ROMs (one with a name that is markup), 2 for
 *   review, 1 unknown, 1 duplicate and 2 partial downloads.
 *
 * Only these two folders exist for organising; any other source is
 * `not-found`. Targets are not checked.
 *
 * Extraction: disc archives always extract (flat, or one folder per game
 * on `game-folder` layouts); cartridges extract only from a container the
 * profile does not take as it is (`organizeTouch` adds `.tar.gz` ones; a
 * `.rar` is left, no external tool is found in the mock). Progress is per
 * `EXTRACT_CHUNK` bytes. In move mode a verified archive goes to the
 * Trash, except over `TRASH_MAX_BYTES` (the mock's Trash refuses it:
 * `extracted-kept`, the archive stays); undo deletes the extracted files
 * and brings trashed archives back.
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
  type ContainerKind,
  type DiscId,
  type EntryView,
  type Evidence,
  type ExtractEntry,
  type IdLevel,
  type JobHistoryItem,
  type LeftReason,
  type LibraryView,
  type Media,
  type NameMode,
  type OrganizeDecision,
  type ItemNote,
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
  type Transfer,
  type TransferMode,
} from '../types';
import { MOCK_DISC_ARCHIVES, type MockArchiveEntry } from './discFixture';
import { FIXTURE_SYSTEMS } from './fixture';
import type { MockExport } from './mockJobs';
import { MOCK_SAMPLE_SORTED, type MockSortedFolder } from './sampleFixture';

export const MOCK_DOWNLOADS = '/Users/demo/Downloads/roms';
export const MOCK_MIXED = '/Users/demo/Mixed';
export const MOCK_SAMPLE = '/Users/demo/ROMs-Downloaded';
export const MOCK_PLAN_EXPORT = 'coverly-organize-plan.json';
export const MOCK_PICK_KEY = 'coverly.mock.pick';

/**
 * What "Open folder…" picks in the browser mock, so the organise offer is
 * reachable from the start screen: `?mockPick=downloads|loose|mixed|<absolute
 * path>` (or localStorage `coverly.mock.pick`). `downloads` is the owner's
 * pile as it is now (`MOCK_SAMPLE`), `loose` the same pile before any of it
 * was sorted (`MOCK_DOWNLOADS`). Null: the first fixture library, as before.
 */
export function readMockPick(): string | null {
  const resolve = (v: string | null): string | null => {
    if (v === 'downloads') return MOCK_SAMPLE;
    if (v === 'loose') return MOCK_DOWNLOADS;
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
/** Bytes decoded between two extract progress events. */
const EXTRACT_CHUNK = 128 * 1024 * 1024;
/** The mock's Trash refuses bigger archives (they stay: `extracted-kept`). */
const TRASH_MAX_BYTES = 450_000_000;

/** What an archive holds and how it is extracted. */
interface ArchiveDef {
  /** The container its magic says. */
  container: ContainerKind;
  /** The extension says another container (`ContainerMismatch`). */
  mismatch?: { ext: string; actual: string };
  /** File entries in archive order. */
  entries: MockArchiveEntry[];
  /** A disc game: its id and the sheet the item is named after. */
  disc?: {
    id: DiscId;
    primary: string;
    sheetDeferred: boolean;
  };
}

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
  archive?: ArchiveDef;
}

interface PileDef {
  path: string;
  files: PileFile[];
  junk: string[];
  /** System folders already sorted there (opens as a library). */
  sorted?: readonly MockSortedFolder[];
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
/** What the core's GBA header check reports on the real sample. */
const GBA_HEADER = header('gba', 'fixed 0x96, logo');
const SFC_HEADER = header('snes', 'LoROM header, checksum ok');
const PARTIAL: LeftReason = { kind: 'partial-download' };

/** A disc archive of `discFixture.ts` as a pile file. */
const DISC_FILES: PileFile[] = MOCK_DISC_ARCHIVES.map((d) => ({
  name: d.name,
  size: d.size,
  system: d.system,
  method: 'header',
  confidence: 0.75,
  detail: d.detail,
  archive: {
    container: d.actual,
    mismatch: {
      ext: d.name.slice(d.name.lastIndexOf('.') + 1),
      actual: d.actual,
    },
    entries: d.entries,
    disc: {
      id: {
        serials: [d.serial],
        disc_no: null,
        volume: null,
        source: d.source,
      },
      primary: d.primary,
      sheetDeferred: d.sheetDeferred,
    },
  },
}));

/** A made-up CRC32 for an added file, stable for its name. */
function nameCrc(name: string): number {
  let h = 0x811c9dc5;
  for (const c of name) h = Math.imul(h ^ c.charCodeAt(0), 0x01000193);
  return h >>> 0;
}

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
      ...DISC_FILES,
    ],
  },
  {
    // The same sample as it is now (`coverly organize --offline` on it,
    // docs/TRACKER.md 2026-10-24): what was sorted stays in `ROMS/`; the
    // loose GBA zips are decided by header (no GBA catalogue cached), the
    // patch-only zip and the web page stay, and the `.rar` needs unrar.
    path: MOCK_SAMPLE,
    junk: ['.DS_Store'],
    sorted: MOCK_SAMPLE_SORTED,
    files: [
      {
        name: 'Avatar - the Last Airbender GBA.zip',
        size: 3_022_101,
        ...GBA_HEADER,
      },
      {
        name: 'Dragonball Z - Supersonic Warriors GBA.zip',
        size: 9_192_857,
        ...GBA_HEADER,
      },
      { name: 'FIFA Soccer 07.zip', size: 3_760_849, ...GBA_HEADER },
      {
        name: 'Pokemon - Leaf Green Version (U) (V1.1) (1).zip',
        size: 5_329_069,
        ...GBA_HEADER,
      },
      {
        name: 'Pokemon Jupiter - 6.04 (Ruby Hack) (1).zip',
        size: 2_179_492,
        left: { kind: 'not-a-rom', what: 'patch' },
      },
      {
        name: 'Pokemon Ultra Violet (1.22) LSA (Fire Red Hack).zip',
        size: 5_328_440,
        ...GBA_HEADER,
      },
      {
        name: '3-D Tetris (USA).zip',
        size: 5_900,
        left: { kind: 'not-a-rom', what: 'web-page' },
      },
      {
        name: 'Tetris Collection.rar',
        size: 2_097_152,
        left: { kind: 'unsupported-archive', format: 'rar', need: 'unrar' },
      },
      ...DISC_FILES,
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

const ARCHIVE_EXTS = ['.zip', '.7z', '.rar', '.gz', '.tgz'];
/** An archive as the plan sees it (its kind is `archive`). */
const isArchive = (file: PileFile) =>
  file.archive !== undefined ||
  ARCHIVE_EXTS.some((x) => file.name.toLowerCase().endsWith(x));

const collator = new Intl.Collator('en', {
  sensitivity: 'base',
  numeric: true,
});
/** Systems only the organise piles have (the fixture libraries do not),
 * named like the Rust catalogue (`systems.toml`). */
export const ORGANIZE_SYSTEMS: readonly {
  id: string;
  name: string;
  folder: string;
}[] = [{ id: 'saturn', name: 'Sega Saturn', folder: 'SATURN' }];
const SYSTEMS = new Map<string, { id: string; name: string; folder: string }>(
  [...FIXTURE_SYSTEMS, ...ORGANIZE_SYSTEMS].map((s) => [s.id, s]),
);

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

/** A file (or the files of an archive) an apply put in a target. */
interface Placed {
  job: string;
  file: PileFile;
  system: string;
  /** The system folder, relative to the target root. */
  dir: string;
  /** Every file written, relative to the target root. */
  written: string[];
  /** What the library lists (the sheets of a disc), with its size. */
  shown: { rel: string; size: number }[];
  /** Its archive was extracted (not moved or copied as it is). */
  extracted: boolean;
  /** Its archive went to the Trash (undo brings it back). */
  trashed: boolean;
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
  /** The layout of the folders a pile already has sorted (`PileDef.sorted`);
   * absent: those piles open as loose piles. */
  sortedLayout?: ProfileInfo | undefined;
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

/** A file `organizeTouch` adds, placed by its name. */
function addedFile(name: string): PileFile {
  const lower = name.toLowerCase();
  if (lower.endsWith('.rar')) {
    // No external tool is found in the mock (settings list none).
    return {
      name,
      size: 2_097_152,
      left: { kind: 'unsupported-archive', format: 'rar', need: 'unrar' },
    };
  }
  if (lower.endsWith('.tar.gz')) {
    // A GBA ROM in a container no profile takes as it is: extracted.
    const rom = `${name.slice(0, -'.tar.gz'.length)}.gba`;
    return {
      name,
      size: 2_097_152,
      ...header('gba', 'Nintendo logo, GBA header'),
      archive: {
        container: 'tar.gz',
        entries: [
          { index: 0, name: rom, size: 4_194_304, crc32: nameCrc(rom) },
        ],
      },
    };
  }
  const known = ADDED_BY_EXT[extOf(name)];
  return known
    ? { name, size: 4_194_304, ...known }
    : { name, size: 32_768, left: { kind: 'unknown' } };
}

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
  // What a pile already has sorted: no job wrote it, so no undo touches it.
  for (const p of PILES) {
    const layout = opts.sortedLayout;
    if (!p.sorted || !layout) continue;
    targets.set(p.path, {
      root: p.path,
      layout,
      romsRel: layout.profile.roms_root,
      files: p.sorted.flatMap((f) =>
        f.files.map(([name, size]): Placed => {
          const rel = join(f.dir, name);
          return {
            job: '',
            file: { name, size, system: f.system },
            system: f.system,
            dir: f.dir,
            written: [rel],
            shown: [{ rel, size }],
            extracted: false,
            trashed: false,
          };
        }),
      ),
    });
  }
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
        extra.push(addedFile(name));
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
      kind: isArchive(file) ? ('archive' as const) : ('file' as const),
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

  /** How an archive item extracts on this plan's layout; null: as it is. */
  function extractOf(
    r: Resolved,
    file: PileFile,
    system: string,
    dir: string,
  ): {
    transfer: Extract<Transfer, { kind: 'extract' }>;
    target: string;
    notes: ItemNote[];
    disc: DiscId | null;
  } | null {
    const a = file.archive;
    if (!a) return null;
    const p = r.layout.profile;
    const notes: ItemNote[] = [];
    if (a.mismatch) notes.push({ kind: 'container-mismatch', ...a.mismatch });
    const entry = (e: MockArchiveEntry, target: string): ExtractEntry => ({
      index: e.index,
      name: e.name,
      size: e.size,
      crc32: e.crc32,
      target,
    });
    let entries: ExtractEntry[];
    let target: string;
    if (a.disc) {
      // Paths are relative to the sheet's folder: the archive's game
      // folder is dropped (flat) or becomes `<stem>/` (one per game).
      const { primary } = a.disc;
      const base = primary.slice(0, primary.lastIndexOf('/') + 1);
      const perGame = p.disc_layout?.kind === 'game-folder';
      const into = perGame
        ? join(dir, stemOf(primary.slice(base.length)))
        : dir;
      const to = (name: string) =>
        join(into, name.startsWith(base) ? name.slice(base.length) : name);
      entries = a.entries.map((e) => entry(e, to(e.name)));
      target = to(primary);
      if (a.disc.sheetDeferred) notes.push({ kind: 'sheet-not-read-yet' });
      const exts = new Set(
        a.entries.map((e) => extOf(e.name).slice(1).toLowerCase()),
      );
      // One folder per game: menus list it once.
      if (!perGame && exts.has('gdi') && exts.has('cue'))
        notes.push({ kind: 'twin-sheets', kept: ['gdi', 'cue'] });
    } else {
      const accepts = p.container_overrides?.[system] ??
        p.accepts_containers ?? ['zip', '7z'];
      if (accepts.includes(a.container)) return null;
      entries = a.entries.map((e) =>
        entry(e, join(dir, e.name.slice(e.name.lastIndexOf('/') + 1))),
      );
      target = entries[0]!.target;
      if (entries.length > 1)
        notes.push({ kind: 'multi-rom-extract', count: entries.length });
    }
    return {
      transfer: {
        kind: 'extract',
        entries,
        bytes: entries.reduce((n, e) => n + e.size, 0),
        // Copy mode never touches the archive; nothing is left in these.
        archive_after: r.mode === 'move' ? 'trash' : 'keep',
        source: { kind: 'builtin' },
        sheet_check: a.disc?.sheetDeferred ? 'deferred' : 'read',
        m3u: null,
      },
      target,
      notes,
      disc: a.disc ? structuredClone(a.disc.id) : null,
    };
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
      existing
        ? existing.files.flatMap((p) => p.written.map((w) => w.toLowerCase()))
        : [],
    );
    for (const [i, item] of items.entries()) {
      if (item.action === 'left' || item.system_id === null) continue;
      const folder = folderFor(r, item.system_id);
      const dir = join(r.romsRel, folder);
      if (item.action === 'review') {
        items[i] = { ...item, folder };
        continue;
      }
      const extract = extractOf(r, files[i]!, item.system_id, dir);
      if (extract) {
        // Set members keep their names: any collision leaves the item.
        const keys = extract.transfer.entries.map((e) =>
          e.target.toLowerCase(),
        );
        if (
          keys.some((k) => taken.has(k)) ||
          new Set(keys).size !== keys.length
        ) {
          items[i] = leave(item, { kind: 'collision-set' });
          continue;
        }
        keys.forEach((k) => taken.add(k));
        items[i] = {
          ...item,
          folder,
          target: extract.target,
          notes: [...item.notes, ...extract.notes],
          transfer: extract.transfer,
          disc: extract.disc,
        };
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
          ...new Map(sortedHere.files.map((p) => [p.dir, p.system])).entries(),
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
      extracted: count('extracted') + count('extracted-kept'),
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
          const t = item.transfer;
          if (t.kind === 'extract') {
            // Byte-based, like Rust: 0 first, `bytes_total` last.
            const at = {
              plan_id: planId,
              phase: 'extract' as const,
              item: item.id,
              done,
              total,
              bytes_total: t.bytes,
            };
            emitProgress({ ...at, bytes_done: 0 });
            for (let b = EXTRACT_CHUNK; b < t.bytes; b += EXTRACT_CHUNK) {
              await host.tick();
              // Cancelled mid-item: its temps are rolled back.
              if (own.cancel) break;
              emitProgress({ ...at, bytes_done: b });
            }
            if (own.cancel) {
              state = 'cancelled';
              break;
            }
            emitProgress({ ...at, bytes_done: t.bytes });
          }
          const placed = placedOf(j, item, file);
          j.placed.push(placed);
          target.files.push(placed);
          let status: 'moved' | 'copied' | 'extracted' | 'extracted-kept' =
            j.mode === 'move' ? 'moved' : 'copied';
          if (t.kind === 'extract') {
            const trash = j.mode === 'move' && t.archive_after === 'trash';
            placed.trashed = trash && file.size <= TRASH_MAX_BYTES;
            status = trash && !placed.trashed ? 'extracted-kept' : 'extracted';
          }
          if (j.mode === 'move' && (t.kind === 'as-is' || placed.trashed))
            loose.get(j.source)!.delete(file.name);
          out.push({
            item: item.id,
            status,
            targets: placed.written.map((w) => `${j.target}/${w}`),
          });
        }
        if (paused) {
          state = 'paused';
          break;
        }
        if (state === 'cancelled') break;
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

  /** What applying `item` writes into the target. */
  function placedOf(j: OrgJournal, item: OrganizeItem, file: PileFile): Placed {
    const dir = join(j.romsRel, item.folder!);
    const t = item.transfer;
    if (t.kind === 'as-is') {
      return {
        job: j.job,
        file,
        system: item.system_id!,
        dir,
        written: [item.target!],
        shown: [{ rel: item.target!, size: file.size }],
        extracted: false,
        trashed: false,
      };
    }
    const sheet = (rel: string) => /\.(cue|gdi|m3u)$/i.test(rel);
    const shown = t.entries
      .filter((e) => item.disc === null || sheet(e.target))
      .map((e) => ({ rel: e.target, size: e.size }));
    if (t.m3u) shown.push({ rel: t.m3u, size: 0 });
    return {
      job: j.job,
      file,
      system: item.system_id!,
      dir,
      written: [...t.entries.map((e) => e.target), ...(t.m3u ? [t.m3u] : [])],
      shown,
      extracted: true,
      trashed: false,
    };
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

  /** ROM-like files still loose at the top of pile `path` (what the
   * library's "Organise loose files" offers); 0 for a folder no pile is. */
  function looseCount(path: string): number {
    const names = loose.get(path);
    if (!names || gone.has(path)) return 0;
    return allFiles(path).filter(
      (f) => names.has(f.name) && f.left?.kind !== 'partial-download',
    ).length;
  }

  function libraryView(t: Target, generation: number): LibraryView {
    const p = t.layout.profile;
    const media = MEDIA.filter((m) => p.slots[m] !== undefined);
    const art = Object.fromEntries(
      MEDIA.map((m) => [m, media.includes(m) ? 'missing' : 'unsupported']),
    ) as unknown as ArtMarks;
    const bySystem = new Map<string, Placed[]>();
    for (const f of t.files) {
      bySystem.set(f.dir, [...(bySystem.get(f.dir) ?? []), f]);
    }
    const dirs = [...bySystem.keys()].sort((a, b) => collator.compare(a, b));
    const systems: SystemView[] = [];
    const entries: EntryView[] = [];
    dirs.forEach((dir, index) => {
      const placed = bySystem.get(dir)!;
      const files = placed
        .flatMap((p) => p.shown)
        .sort((a, b) => collator.compare(a.rel, b.rel));
      const first = placed[0]!;
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
        const fileDir = f.rel.slice(0, f.rel.lastIndexOf('/'));
        entries.push({
          id: entries.length,
          system: index,
          rel_path: f.rel,
          dir: fileDir,
          name: stemOf(file),
          ext: extOf(file).slice(1),
          size: f.size,
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
      loose_files: looseCount(t.root),
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

  /** Undoes organise journal `j` alone: moves back or removes its files.
   * Like Rust, nothing is put back over a file now at the source's place:
   * a moved ROM stays at its target (`skipped_occupied`) and a trashed
   * archive is lost, so its extracted files stay (`archive_lost`). */
  function undoOne(j: OrgJournal): OrganizeUndoReport {
    const t = targets.get(j.target);
    const here = loose.get(j.source)!;
    const restored: string[] = [];
    const removed: string[] = [];
    const occupied: string[] = [];
    const lost: string[] = [];
    /** Files of this job that stay in the target. */
    const stay = new Set<Placed>();
    for (const p of [...j.placed].reverse()) {
      const back = (p.extracted ? p.trashed : j.mode === 'move')
        ? !here.has(p.file.name)
        : null;
      if (back === false) {
        stay.add(p);
        if (p.extracted) lost.push(`${j.source}/${p.file.name}`);
        else occupied.push(`${j.target}/${p.written[0]!}`);
        continue;
      }
      if (back) {
        here.add(p.file.name);
        restored.push(`${j.source}/${p.file.name}`);
      }
      // Extracted files and copies are deleted.
      if (p.extracted || j.mode === 'copy')
        for (const w of [...p.written].reverse())
          removed.push(`${j.target}/${w}`);
    }
    const dirsRemoved: string[] = [];
    if (t) {
      const dirsOf = (files: Placed[]) =>
        new Set(
          files.flatMap((p) => [
            p.dir,
            ...p.written.map((w) => w.slice(0, w.lastIndexOf('/'))),
          ]),
        );
      const before = dirsOf(t.files);
      t.files = t.files.filter((p) => p.job !== j.job || stay.has(p));
      const after = dirsOf(t.files);
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
      skipped_occupied: occupied,
      dirs_removed: dirsRemoved,
      archive_lost: lost,
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
        ? [...new Set(t.files.map((f) => f.dir.split('/').at(-1)!))].sort(
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
