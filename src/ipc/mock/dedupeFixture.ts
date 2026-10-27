/**
 * A deterministic duplicates plan for component tests, the UI gallery and
 * (Phase B) the mock backend. It reproduces the spec's showcase groups:
 *
 * - SMS "Ecco the Dolphin": an exact copy under another name; the keeper
 *   wins on its cover.
 * - PS "Crash Bash": a PBP kept on Anbernic over a BIN of the same disc
 *   (other format, confidence 0.97) and an exact BIN twin; the shared
 *   `Crash Bash.srm` stays with the PBP.
 * - GB "Tetris": World Rev 1 vs Japan, a level-two group held back because
 *   one-ROM-per-game is off (report only).
 * - GB "Pokemon Red": two exact copies with different saves (conflict).
 *
 * Copy facts (tags, saves, art) sit on the showcase groups and on "Disc
 * Game 01" (five regions); every other filler has none (missing cover).
 *
 * Fillers bring it to the spec's numbers: 89 extra copies across 88 games
 * that act (4 of them other formats), and 37 other versions report-only.
 * Names and bytes are synthetic.
 */
import type {
  DedupePlanView,
  DupGroupView,
  KeepReasonView,
  MemberFactsView,
  MemberView,
  RelationView,
  TotalsView,
  TravelView,
  UnitKindView,
} from '../dedupeTypes';

const ROOT = '/Volumes/SD';
const QUARANTINE = `${ROOT}/_Duplicados`;
const KB = 1024;
const MB = 1024 * KB;
const MTIME = 1_700_000_000_000_000_000;
const DAY_NS = 86_400_000_000_000;

interface MemberSpec {
  path: string;
  size: number;
  relation?: RelationView;
  kind?: UnitKindView;
  files?: string[];
  confidence?: number;
  travels?: TravelView[];
  stays?: { path: string; with: number }[];
  catalogue?: string;
  serials?: string[];
  /** Art paths are relative to the ROMs folder, like `path`. */
  facts?: MemberFactsView;
}

function member(id: number, spec: MemberSpec, keeper: boolean): MemberView {
  return {
    id,
    unit: {
      primary: spec.path,
      files: spec.files ?? [spec.path],
      kind: spec.kind ?? 'single',
      mtime_ns: MTIME + id,
      in_library: true,
    },
    relation: spec.relation ?? 'exact',
    size: spec.size,
    travels: spec.travels ?? [],
    stays: spec.stays ?? [],
    dest: keeper ? null : `${QUARANTINE}/${spec.path}`,
    catalogue_name: spec.catalogue ?? null,
    serials: spec.serials ?? [],
    confidence: spec.confidence ?? 1,
    ...(spec.facts ? { facts: spec.facts } : {}),
  };
}

let nextId = 0;

function group(
  system: string,
  title: string,
  reason: KeepReasonView,
  specs: MemberSpec[],
  extra: Partial<DupGroupView> = {},
): DupGroupView {
  nextId += 1;
  return {
    id: 4_200_000 + nextId,
    system,
    title,
    keeper: 0,
    reason,
    runner_up: null,
    members: specs.map((s, i) => member(i, s, i === 0)),
    action: { kind: 'act' },
    tie: false,
    choice_dropped: false,
    ...extra,
  };
}

const pad = (n: number) => String(n).padStart(2, '0');

function showcase(): DupGroupView[] {
  return [
    group('SMS', 'Ecco the Dolphin', { kind: 'has-cover' }, [
      {
        path: 'SMS/Ecco the Dolphin.sms',
        size: 512 * KB,
        facts: {
          art: [
            {
              media: 'cover',
              path: 'SMS/Imgs/Ecco the Dolphin.png',
              size: 48_211,
            },
          ],
        },
      },
      {
        path: 'SMS/Adventures of Dolphins.sms',
        size: 512 * KB,
        facts: {
          flags: {
            verified: false,
            bad: true,
            overdump: false,
            alternate: false,
          },
          flags_from: 'file-name',
        },
        travels: [
          {
            from: 'SMS/Imgs/Adventures of Dolphins.png',
            kind: { kind: 'art', media: 'cover' },
          },
        ],
      },
    ]),
    group(
      'PS',
      'Crash Bash',
      {
        kind: 'container',
        kept: 'pbp',
        other: 'bin',
        profile: 'anbernic-stock',
      },
      [
        {
          path: 'PS/Crash Bash.PBP',
          size: Math.round(79.6 * MB),
          kind: 'image',
          serials: ['SCUS-94570'],
          facts: {
            saves: { count: 1, last_ns: MTIME - 3 * DAY_NS },
            played: { count: 4, last: '20260102T101010' },
          },
        },
        {
          path: 'PS/Crash Bash.bin',
          size: 178 * MB,
          kind: 'image',
          relation: 'format',
          confidence: 0.97,
          serials: ['SCUS-94570'],
          stays: [{ path: 'PS/Crash Bash.srm', with: 0 }],
        },
        {
          path: 'PS/Crash Bash 1.bin',
          size: 178 * MB,
          kind: 'image',
          serials: ['SCUS-94570'],
        },
      ],
      { runner_up: [1, { kind: 'tie' }] },
    ),
    group(
      'GB',
      'Tetris',
      { kind: 'region', kept: 'world', other: 'japan' },
      [
        {
          path: 'GB/Tetris (World) (Rev 1).gb',
          size: 32 * KB,
          catalogue: 'Tetris (World) (Rev 1)',
          facts: {
            tags_from: 'catalogue',
            regions: ['world'],
            revision: 'Rev 1',
            art: [
              {
                media: 'cover',
                path: 'GB/Imgs/Tetris (World) (Rev 1).png',
                size: 12_288,
              },
            ],
          },
        },
        {
          path: 'GB/Tetris (Japan).gb',
          size: 32 * KB,
          relation: 'version',
          catalogue: 'Tetris (Japan)',
          // Its own box (another size): the screen shows its thumbnail.
          facts: {
            tags_from: 'file-name',
            regions: ['japan'],
            art: [
              {
                media: 'cover',
                path: 'GB/Imgs/Tetris (Japan).png',
                size: 9_001,
              },
            ],
          },
        },
      ],
      {
        action: { kind: 'report-only', why: { kind: 'level-two-off' } },
        runner_up: [1, { kind: 'region', kept: 'world', other: 'japan' }],
      },
    ),
    group(
      'GB',
      'Pokemon Red',
      { kind: 'has-save', path: 'GB/Pokemon Red.sav' },
      [
        {
          path: 'GB/Pokemon Red.gb',
          size: 1 * MB,
          facts: { saves: { count: 1, last_ns: MTIME } },
        },
        {
          path: 'GB/Pokemon Red (1).gb',
          size: 1 * MB,
          facts: {
            saves: { count: 1, last_ns: MTIME + DAY_NS },
            modification: 'translation',
            modification_from: 'file-name',
          },
        },
      ],
      {
        action: {
          kind: 'conflict',
          saves: [
            {
              path: 'GB/Pokemon Red.sav',
              size: 32 * KB,
              mtime_ns: MTIME,
              sha1: '1111111111111111111111111111111111111111',
              keyed_by: 'stem',
            },
            {
              path: 'GB/Pokemon Red (1).sav',
              size: 32 * KB,
              mtime_ns: MTIME + 1,
              sha1: '2222222222222222222222222222222222222222',
              keyed_by: 'stem',
            },
          ],
        },
      },
    ),
  ];
}

/** One keeper sold in many regions: its region chips overflow. */
const manyRegions = (): MemberFactsView => ({
  tags_from: 'file-name',
  regions: ['usa', 'europe', 'japan', 'brazil', 'korea'],
  other: ['Virtual Console'],
});

function fillers(): DupGroupView[] {
  const out: DupGroupView[] = [];
  for (let i = 1; i <= 3; i++)
    out.push(
      group(
        'PS',
        `Disc Game ${pad(i)}`,
        {
          kind: 'container',
          kept: 'chd',
          other: 'pbp',
          profile: 'anbernic-stock',
        },
        [
          {
            path: `PS/Disc Game ${pad(i)}.chd`,
            size: 300 * MB,
            kind: 'chd',
            ...(i === 1 ? { facts: manyRegions() } : {}),
          },
          {
            path: `PS/Disc Game ${pad(i)}.pbp`,
            size: 320 * MB,
            kind: 'image',
            relation: 'format',
            confidence: 0.97,
          },
        ],
      ),
    );
  const exact: [string, string, number, number][] = [
    ['SMS', 'sms', 22, 320 * KB],
    ['GB', 'gb', 4, 256 * KB],
    ['GBA', 'gba', 57, 8 * MB],
  ];
  for (const [system, ext, count, size] of exact)
    for (let i = 1; i <= count; i++) {
      const name = `${system} Game ${pad(i)}`;
      out.push(
        group(system, name, { kind: 'catalogue-name', name }, [
          { path: `${system}/${name}.${ext}`, size },
          { path: `${system}/${name} (1).${ext}`, size },
        ]),
      );
    }
  for (let i = 1; i <= 36; i++) {
    const name = `Version Game ${pad(i)}`;
    out.push(
      group(
        'GBA',
        name,
        { kind: 'revision', kept: 'Rev 1', other: 'Rev 0' },
        [
          { path: `GBA/${name} (Rev 1).gba`, size: 4 * MB },
          { path: `GBA/${name}.gba`, size: 4 * MB, relation: 'version' },
        ],
        { action: { kind: 'report-only', why: { kind: 'level-two-off' } } },
      ),
    );
  }
  return out;
}

/** The core's `totals()`: conflicts counted apart, sums over acting groups. */
export function dedupeTotals(groups: DupGroupView[]): TotalsView {
  const t: TotalsView = {
    groups: 0,
    extras: 0,
    bytes: 0,
    conflicts: 0,
    by_system: {},
  };
  for (const g of groups) {
    t.groups += 1;
    if (g.action.kind === 'conflict') t.conflicts += 1;
    if (g.action.kind !== 'act') continue;
    const extras = g.members.filter((m) => m.id !== g.keeper);
    const bytes = extras.reduce((n, m) => n + m.size, 0);
    t.extras += extras.length;
    t.bytes += bytes;
    const s = (t.by_system[g.system] ??= [0, 0, 0]);
    s[0] += 1;
    s[1] += extras.length;
    s[2] += bytes;
  }
  return t;
}

/** A fresh copy of the fixture plan on every call. */
export function dedupeFixture(): DedupePlanView {
  nextId = 0;
  const groups = [...showcase(), ...fillers()];
  return {
    plan_id: 1,
    version: 1,
    root: ROOT,
    roms_dir: `${ROOT}/ROMS`,
    quarantine: QUARANTINE,
    same_volume: true,
    removable: true,
    profile_id: 'anbernic-stock',
    groups,
    unchanged: groups
      .filter((g) => g.action.kind === 'report-only')
      .map((g) => g.id),
    not_roms: ['GB/index.html.zip'],
    unreadable: [],
    no_catalogue: ['NDS', 'PSP', 'N64'],
    chd_unread: 0,
    off_card_saves: [],
    totals: dedupeTotals(groups),
    trash: false,
    trash_available: true,
  };
}
