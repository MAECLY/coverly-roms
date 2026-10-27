import { CircleHelp, Copy, Disc3, History } from 'lucide-react';
import { describe, expect, it } from 'vitest';

import en from '../../locales/en/ui.json';
import es from '../../locales/es/ui.json';
import { translate, type Locale, type TranslateVars } from '@/i18n/core';
import {
  DUMP_QUALITIES,
  KEEP_REASON_KINDS,
  RELEASE_CLASSES,
  REPORT_WHY_KINDS,
  type DedupeOutcomeView,
  type DupGroupView,
  type MemberFactsView,
  type MemberView,
  type RegionId,
  type ReportWhyView,
  type KeepReasonView,
  type UnitKindView,
  type UnitRefView,
} from '@/ipc/dedupeTypes';
import { dedupeFixture } from '@/ipc/mock/dedupeFixture';

import {
  badgeRelation,
  coverMedia,
  currentPath,
  deltaFor,
  destLabel,
  destOf,
  extrasFor,
  formatBytes,
  formatItems,
  formatOf,
  levelCounts,
  newestMtime,
  ownArtDiffers,
  reasonText,
  regionCode,
  reportWhyText,
  relationMeta,
  ruleKey,
  sortGroups,
  systemStartsOpen,
  systemSummary,
  tagChips,
  weakestExtra,
} from './dedupe';

const tr = (locale: Locale) => (key: string, vars?: TranslateVars) =>
  translate(locale, key, vars);

/** One sample of every reason kind, with every value it can carry. */
function samples(): KeepReasonView[] {
  const out: KeepReasonView[] = [];
  for (const kind of KEEP_REASON_KINDS) {
    switch (kind) {
      case 'user-choice':
      case 'in-library':
      case 'has-cover':
      case 'tie':
        out.push({ kind });
        break;
      case 'has-save':
        out.push({ kind, path: 'GB/Tetris.sav' });
        break;
      case 'played':
        out.push({ kind, playcount: 3 });
        break;
      case 'verified':
        for (const kept of DUMP_QUALITIES)
          for (const other of DUMP_QUALITIES) out.push({ kind, kept, other });
        break;
      case 'retail':
        for (const other of RELEASE_CLASSES) out.push({ kind, other });
        break;
      case 'region':
        out.push({ kind, kept: 'world', other: 'japan' });
        out.push({ kind, kept: 'hong-kong', other: 'united-kingdom' });
        break;
      case 'revision':
        out.push({ kind, kept: 'V1.1', other: 'V1.0' });
        break;
      case 'container':
        out.push({
          kind,
          kept: 'pbp',
          other: 'cue',
          profile: 'anbernic-stock',
        });
        break;
      case 'catalogue-name':
        out.push({ kind, name: 'Tetris (World) (Rev 1)' });
        break;
    }
  }
  return out;
}

describe('reasonText', () => {
  it('every_keep_reason_has_text_in_both_locales', () => {
    const seen = new Set<string>();
    for (const locale of ['es', 'en'] as const) {
      for (const reason of samples()) {
        seen.add(reason.kind);
        const text = reasonText(tr(locale), reason, {});
        expect(text, `${locale} ${JSON.stringify(reason)}`).not.toMatch(
          /dedupe\.|regions\.|\{|\}/,
        );
        expect(text.length).toBeGreaterThan(8);
        expect(text).not.toContain('!');
      }
    }
    expect([...seen].sort()).toEqual([...KEEP_REASON_KINDS].sort());
  });

  it('reason_text_names_what_the_other_had', () => {
    const region: KeepReasonView = {
      kind: 'region',
      kept: 'world',
      other: 'japan',
    };
    expect(reasonText(tr('es'), region, {})).toBe(
      'Se queda: región preferida (Mundo); la otra es Japón.',
    );
    expect(reasonText(tr('en'), region, {})).toBe(
      'Stays: preferred region (World); the other is Japan.',
    );
  });

  it('names the profile and upper-cases containers', () => {
    const text = reasonText(
      tr('es'),
      {
        kind: 'container',
        kept: 'pbp',
        other: 'bin',
        profile: 'anbernic-stock',
      },
      { profileName: 'Anbernic' },
    );
    expect(text).toBe(
      'Se queda: formato preferido en Anbernic (PBP); la otra es BIN.',
    );
  });

  it('shows the save by file name only', () => {
    expect(
      reasonText(
        tr('es'),
        { kind: 'has-save', path: 'GB\\saves/Tetris.sav' },
        {},
      ),
    ).toBe('Se queda: tiene tu partida (Tetris.sav).');
  });

  it('a single play reads in the singular', () => {
    const once: KeepReasonView = { kind: 'played', playcount: 1 };
    expect(reasonText(tr('es'), once, {})).toBe(
      'Se queda: es la que has jugado (1 vez).',
    );
    expect(reasonText(tr('en'), once, {})).toBe(
      "Stays: it's the one you played (once).",
    );
    expect(reasonText(tr('es'), { kind: 'played', playcount: 3 }, {})).toBe(
      'Se queda: es la que has jugado (3 veces).',
    );
  });

  it('never says eliminar or borrar in Spanish', () => {
    for (const reason of samples())
      expect(reasonText(tr('es'), reason, {})).not.toMatch(/elimin|borr/i);
  });
});

describe('relationMeta', () => {
  it('relation_meta_uses_lucide_icons', () => {
    expect(relationMeta('exact')).toEqual({
      icon: Copy,
      key: 'dedupe.relation.exact',
    });
    expect(relationMeta('format')).toEqual({
      icon: Disc3,
      key: 'dedupe.relation.format',
    });
    expect(relationMeta('version')).toEqual({
      icon: History,
      key: 'dedupe.relation.version',
    });
    expect(relationMeta('unconfirmed')).toEqual({
      icon: CircleHelp,
      key: 'dedupe.relation.unconfirmed',
    });
    for (const r of ['exact', 'format', 'version', 'unconfirmed'] as const)
      for (const locale of ['es', 'en'] as const)
        expect(translate(locale, relationMeta(r).key)).not.toMatch(/^dedupe\./);
  });

  it('only a confidence below 1.00 changes an exact badge (R1)', () => {
    const g = dedupeFixture().groups.find((x) => x.title === 'Crash Bash')!;
    const byRelation = (r: string) => g.members.find((m) => m.relation === r)!;
    expect(badgeRelation(byRelation('exact'))).toBe('exact');
    expect(badgeRelation({ ...byRelation('exact'), confidence: 0.9 })).toBe(
      'unconfirmed',
    );
    expect(badgeRelation(byRelation('format'))).toBe('format');
  });
});

describe('counts', () => {
  const plan = dedupeFixture();

  it('fixture totals match its groups', () => {
    expect(plan.totals.extras).toBe(89);
    expect(plan.totals.groups - plan.totals.conflicts).toBeGreaterThan(88);
    expect(
      extrasFor(plan, { levels: ['exact', 'format'], one_game_one_rom: false }),
    ).toBe(89);
  });

  it('delta_for_enabling_versions_is_37_on_fixture', () => {
    expect(
      deltaFor(plan, {
        levels: ['exact', 'format', 'version'],
        one_game_one_rom: true,
      }),
    ).toBe(37);
    expect(
      deltaFor(plan, {
        levels: ['exact', 'format', 'version'],
        one_game_one_rom: false,
      }),
    ).toBe(0);
    expect(deltaFor(plan, { levels: ['exact'], one_game_one_rom: false })).toBe(
      -4,
    );
  });

  it('systemSummary sums acting groups per system, bytes leaving', () => {
    const s = systemSummary(plan.groups);
    const sms = s.find((x) => x.system === 'SMS')!;
    const byTotals = plan.totals.by_system.SMS!;
    expect([sms.groups, sms.extras, sms.bytes]).toEqual(byTotals);
    const gb = s.find((x) => x.system === 'GB')!;
    expect(gb.conflicts).toBe(1);
    expect(s.map((x) => x.system)).toEqual(
      [...s.map((x) => x.system)].sort((a, b) => a.localeCompare(b)),
    );
  });

  it('formatBytes is the shared size formatter', () => {
    expect(formatBytes(512 * 1024, 'es')).toBe('512 KB');
  });
});

describe('sortGroups', () => {
  it('conflicts_sort_first', () => {
    const plan = dedupeFixture();
    const sorted = sortGroups([...plan.groups].reverse());
    expect(sorted[0]!.action.kind).toBe('conflict');
    const rest = sorted.slice(1);
    expect(rest.every((g) => g.action.kind !== 'conflict')).toBe(true);
    for (let i = 1; i < rest.length; i++) {
      const a = rest[i - 1]!;
      const b = rest[i]!;
      const bySystem = a.system.localeCompare(b.system);
      expect(
        bySystem < 0 || (bySystem === 0 && a.title.localeCompare(b.title) <= 0),
      ).toBe(true);
    }
  });

  it('does not mutate its input', () => {
    const groups: DupGroupView[] = dedupeFixture().groups;
    const before = groups.map((g) => g.id);
    sortGroups(groups);
    expect(groups.map((g) => g.id)).toEqual(before);
  });
});

describe('fixture', () => {
  it('reproduces the spec showcase groups', () => {
    const plan = dedupeFixture();
    const titles = plan.groups.map((g) => `${g.system}/${g.title}`);
    expect(titles).toContain('SMS/Ecco the Dolphin');
    expect(titles).toContain('PS/Crash Bash');
    expect(titles).toContain('GB/Tetris');
    const tetris = plan.groups.find((g) => g.title === 'Tetris')!;
    expect(tetris.action).toEqual({
      kind: 'report-only',
      why: { kind: 'level-two-off' },
    });
    expect(typeof plan.plan_id).toBe('number');
    expect(new Set(plan.groups.map((g) => g.id)).size).toBe(plan.groups.length);
  });
});

describe('destinations and badges', () => {
  const crash = () =>
    dedupeFixture().groups.find((g) => g.title === 'Crash Bash')!;

  it('a former keeper goes under the quarantine at its library path', () => {
    const [keeper, bin] = crash().members;
    const q = '/Volumes/SD/_Duplicados';
    expect(destOf(keeper!, q)).toBe(`${q}/PS/Crash Bash.PBP`);
    expect(destOf(bin!, q)).toBe(bin!.dest);
    const abs = {
      ...keeper!,
      unit: { ...keeper!.unit, primary: '/Volumes/SD/ROMS/PS/x.bin' },
    };
    expect(destOf(abs, `${q}/`, [], 'PS')).toBe(`${q}/PS/x.bin`);
    expect(destOf(abs, 'D:\\_Duplicados', [], 'PS')).toBe(
      'D:\\_Duplicados\\PS\\x.bin',
    );
  });

  it('a former keeper with an absolute path keeps its system folder', () => {
    // The core sends absolute primaries and destinations relative to the
    // folder of system folders (`dest_for`): the former keeper must land in
    // the same shape, not loose at the top of the quarantine.
    const g = crash();
    const q = '/Volumes/SD/_Duplicados';
    const lib = '/Volumes/SD/ROMS';
    const absolute = g.members.map((m) => ({
      ...m,
      unit: { ...m.unit, primary: `${lib}/${m.unit.primary}` },
      dest: m.dest === null ? null : `${q}/${m.unit.primary}`,
    }));
    const keeper = {
      ...absolute[0]!,
      unit: { ...absolute[0]!.unit, primary: `${lib}/PS/sub/Crash Bash.PBP` },
    };
    expect(destOf(keeper, q, absolute, g.system)).toBe(
      `${q}/PS/sub/Crash Bash.PBP`,
    );
    // Without a peer to learn the library folder from, the core's own
    // fallback: <quarantine>/<system>/<file name>.
    expect(destOf(keeper, q, [], 'PS')).toBe(`${q}/PS/Crash Bash.PBP`);
  });

  it('a destination outside the quarantine is not cut at a shared prefix', () => {
    const q = '/Volumes/SD/_Duplicados';
    expect(destLabel('/Volumes/SD/_Duplicados2/a.sms', q)).toBe(
      '/Volumes/SD/_Duplicados2/a.sms',
    );
  });

  it('writes destinations from the quarantine folder down', () => {
    const q = '/Volumes/SD/_Duplicados';
    expect(destLabel(`${q}/SMS/a.sms`, q)).toBe('_Duplicados/SMS/a.sms');
    expect(destLabel('/elsewhere/a.sms', q)).toBe('/elsewhere/a.sms');
  });

  it('the weakest link names the badge, then the lowest confidence', () => {
    const g = crash();
    expect(weakestExtra(g.members, 0)?.id).toBe(1);
    expect(weakestExtra(g.members, 1)?.relation).toBe('exact');
    expect(weakestExtra([g.members[0]!], 0)).toBeNull();
  });
});

describe('report-only and per-level counts', () => {
  it('every_report_why_has_text_in_both_locales', () => {
    for (const locale of ['es', 'en'] as const)
      for (const kind of REPORT_WHY_KINDS) {
        const why = (
          kind === 'big-group' ? { kind, members: 3000 } : { kind }
        ) as ReportWhyView;
        const text = reportWhyText(tr(locale), why);
        expect(text, `${locale} ${kind}`).not.toMatch(/dedupe\.|\{|\}/);
        expect(text.length).toBeGreaterThan(8);
      }
  });

  it('level_counts_count_the_copies_each_level_finds', () => {
    expect(levelCounts(dedupeFixture())).toEqual({
      exact: 85,
      format: 4,
      version: 37,
    });
  });

  it('systems open with a conflict or at most 50 groups', () => {
    const s = systemSummary(dedupeFixture().groups);
    const open = Object.fromEntries(
      s.map((x) => [x.system, systemStartsOpen(x)]),
    );
    expect(open).toEqual({ GB: true, GBA: false, PS: true, SMS: true });
  });
});

/* ---- Copy facts helpers (Task 11) ------------------------------------ */

function unit(
  primary: string,
  kind: UnitKindView = 'single',
  extra: Partial<UnitRefView> = {},
): UnitRefView {
  return {
    primary,
    files: [primary],
    kind,
    mtime_ns: 1_700_000_000_000_000_000,
    in_library: true,
    ...extra,
  };
}

function member(
  id: number,
  primary: string,
  facts?: MemberFactsView,
): MemberView {
  return {
    id,
    unit: unit(primary),
    relation: 'exact',
    size: 1024,
    travels: [],
    stays: [],
    dest: null,
    catalogue_name: null,
    serials: [],
    confidence: 1,
    ...(facts ? { facts } : {}),
  };
}

function group(
  members: MemberView[],
  keeper = members[0]?.id ?? 0,
): DupGroupView {
  return {
    id: 7,
    system: 'GB',
    title: 'Tetris',
    keeper,
    reason: { kind: 'tie' },
    runner_up: null,
    members,
    action: { kind: 'act' },
    tie: false,
    choice_dropped: false,
  };
}

const ALL_REGIONS: readonly RegionId[] = [
  'world',
  'usa',
  'europe',
  'japan',
  'asia',
  'australia',
  'brazil',
  'canada',
  'china',
  'france',
  'germany',
  'hong-kong',
  'italy',
  'korea',
  'netherlands',
  'spain',
  'sweden',
  'taiwan',
  'united-kingdom',
];

function has(catalog: object, key: string): boolean {
  let node: unknown = catalog;
  for (const part of key.split('.')) {
    if (typeof node !== 'object' || node === null) return false;
    node = (node as Record<string, unknown>)[part];
  }
  return typeof node === 'string' && node.length > 0;
}

describe('formatOf', () => {
  const t = tr('es');
  it.each([
    ['/SD/ROMS/GB/Tetris.zip', 'zip', 'ZIP'],
    ['/SD/ROMS/GB/Tetris.7z', 'zip', '7Z'],
    ['/SD/ROMS/GB/Tetris.rar', 'zip', 'RAR'],
    ['/SD/ROMS/ARCADE/sf2.zip', 'arcade', 'ZIP (arcade)'],
    ['/SD/ROMS/ARCADE/sf2.7z', 'arcade', '7Z (arcade)'],
    ['/SD/ROMS/PS/Crash.cue', 'disc-set', 'CUE/BIN'],
    ['/SD/ROMS/DC/Crazy Taxi.gdi', 'disc-set', 'GDI'],
    ['/SD/ROMS/PS/Crash.ccd', 'disc-set', 'CCD/IMG'],
    ['/SD/ROMS/PS/Crash.chd', 'chd', 'CHD'],
    ['/SD/ROMS/PS/Crash.pbp', 'image', 'PBP'],
    ['/SD/ROMS/PS2/Ico.iso', 'image', 'ISO'],
    ['/SD/ROMS/PSP/Loco.cso', 'image', 'CSO'],
    ['/SD/ROMS/GC/Melee.rvz', 'image', 'RVZ'],
    ['/SD/ROMS/GB/Tetris.gb', 'single', 'GB'],
    ['C:\\Roms\\PS\\Crash.pbp', 'image', 'PBP'],
  ] as const)('%s (%s) → %s', (primary, kind, want) => {
    expect(formatOf(unit(primary, kind), t)).toBe(want);
  });

  it('multi_disc_names_the_playlist_and_its_disc_count', () => {
    // The core lists the playlist itself first, then its discs.
    const u = unit('/SD/ROMS/PS/FF7.m3u', 'multi-disc', {
      files: [
        '/SD/ROMS/PS/FF7.m3u',
        '/SD/ROMS/PS/FF7 1.chd',
        '/SD/ROMS/PS/FF7 2.chd',
        '/SD/ROMS/PS/FF7 3.chd',
      ],
    });
    expect(formatOf(u, tr('es'))).toBe('M3U · 3 discos');
    expect(formatOf(u, tr('en'))).toBe('M3U · 3 discs');
    expect(formatItems(u, tr('es'))).toEqual(['M3U', '3 discos']);
    expect(formatItems(unit('/SD/ROMS/GB/Tetris.gb', 'single'), t)).toEqual([
      'GB',
    ]);
  });

  it('multi_disc_counts_discs_not_the_tracks_of_their_sheets', () => {
    // A cue/bin playlist: each disc's sheet and its tracks are all files.
    const cueBin = unit('/SD/ROMS/PS/FF7.m3u', 'multi-disc', {
      files: [
        '/SD/ROMS/PS/FF7.m3u',
        '/SD/ROMS/PS/FF7 (Disc 1).cue',
        '/SD/ROMS/PS/FF7 (Disc 2).cue',
        '/SD/ROMS/PS/FF7 (Disc 3).cue',
        '/SD/ROMS/PS/FF7 (Disc 1).bin',
        '/SD/ROMS/PS/FF7 (Disc 2).bin',
        '/SD/ROMS/PS/FF7 (Disc 3).bin',
      ],
    });
    expect(formatOf(cueBin, tr('en'))).toBe('M3U · 3 discs');

    // Multi-track discs, one sheet each, on Windows.
    const tracks = unit('C:\\Roms\\PS\\Rayman.m3u', 'multi-disc', {
      files: [
        'C:\\Roms\\PS\\Rayman.m3u',
        'C:\\Roms\\PS\\Rayman (Disc 1).cue',
        'C:\\Roms\\PS\\Rayman (Disc 2).cue',
        'C:\\Roms\\PS\\Rayman (Disc 1) (Track 1).bin',
        'C:\\Roms\\PS\\Rayman (Disc 1) (Track 2).bin',
        'C:\\Roms\\PS\\Rayman (Disc 2) (Track 1).bin',
      ],
    });
    expect(formatOf(tracks, tr('en'))).toBe('M3U · 2 discs');

    // A playlist naming one sheet and one raw track (core test shape).
    const mixed = unit('/SD/ROMS/PS/Driver 2.m3u', 'multi-disc', {
      files: [
        '/SD/ROMS/PS/Driver 2.m3u',
        '/SD/ROMS/PS/discs/Driver 2 (Disc 1).cue',
        '/SD/ROMS/PS/Driver 2 (Disc 2).bin',
        '/SD/ROMS/PS/discs/Driver 2 (Disc 1).bin',
      ],
    });
    expect(formatOf(mixed, tr('en'))).toBe('M3U · 2 discs');

    // Raw images with no disc numbers: one file per disc.
    const raw = unit('/SD/ROMS/PS/X.m3u', 'multi-disc', {
      files: [
        '/SD/ROMS/PS/X.m3u',
        '/SD/ROMS/PS/X a.img',
        '/SD/ROMS/PS/X b.img',
      ],
    });
    expect(formatOf(raw, tr('en'))).toBe('M3U · 2 discs');
  });
});

describe('regionCode', () => {
  it('every_region_has_a_short_uppercase_code', () => {
    expect(ALL_REGIONS).toHaveLength(19);
    const codes = ALL_REGIONS.map(regionCode);
    for (const [i, code] of codes.entries())
      expect(code, ALL_REGIONS[i]).toMatch(/^[A-Z]{2,3}$/);
    expect(new Set(codes).size).toBe(19);
    expect(regionCode('europe')).toBe('EUR');
    expect(regionCode('united-kingdom')).toBe('UK');
  });
});

describe('ruleKey', () => {
  it('every_keep_reason_has_a_rule_chip_in_both_locales', () => {
    for (const kind of KEEP_REASON_KINDS) {
      const key = ruleKey(kind);
      expect(key, kind).toMatch(/^dedupe\.rule\./);
      expect(has(es, key), `es ${key}`).toBe(true);
      expect(has(en, key), `en ${key}`).toBe(true);
    }
    expect(ruleKey('user-choice')).toBe('dedupe.rule.userChoice');
    expect(ruleKey('catalogue-name')).toBe('dedupe.rule.catalogueName');
    expect(translate('es', ruleKey('container'))).toBe('Formato');
    expect(has(es, 'dedupe.rule.runnerUp')).toBe(true);
  });
});

describe('newestMtime', () => {
  it('takes_the_newest_file_of_a_set', () => {
    expect(newestMtime(unit('/a.gb'))).toBe(1_700_000_000_000_000_000);
    expect(
      newestMtime(
        unit('/a.cue', 'disc-set', { mtime_ns: 5, mtimes_ns: [5, 9, 7] }),
      ),
    ).toBe(9);
  });
});

describe('currentPath', () => {
  const m = member(2, '/SD/ROMS/GB/Tetris (Japan).gb');
  const out = (
    status: DedupeOutcomeView['status'],
    member = 2,
  ): DedupeOutcomeView =>
    status === 'quarantined'
      ? {
          status,
          to: '/SD/_Duplicados/GB/Tetris (Japan).gb',
          group: 7,
          member,
          files: [],
        }
      : { status: 'trashed', location: null, group: 7, member, files: [] };

  it('is_the_primary_before_apply_and_after_undo', () => {
    expect(currentPath(m, 7, null)).toBe(m.unit.primary);
  });
  it('is_the_quarantine_once_moved', () => {
    expect(currentPath(m, 7, [out('quarantined')])).toBe(
      '/SD/_Duplicados/GB/Tetris (Japan).gb',
    );
  });
  it('is_nothing_after_a_trash_apply', () => {
    expect(currentPath(m, 7, [out('trashed')])).toBeNull();
  });
  it('keeper_without_outcome_stays_at_its_primary', () => {
    expect(currentPath(m, 7, [out('quarantined', 3)])).toBe(m.unit.primary);
    expect(currentPath(m, 8, [out('quarantined')])).toBe(m.unit.primary);
  });
});

describe('tagChips', () => {
  const t = tr('es');
  it('undefined_facts_have_no_chips', () => {
    expect(tagChips(undefined, t, 'es')).toEqual([]);
    expect(tagChips({}, t, 'es')).toEqual([]);
  });

  it('five_regions_show_three_and_an_overflow_with_the_rest', () => {
    const chips = tagChips(
      {
        tags_from: 'catalogue',
        regions: ['usa', 'europe', 'japan', 'brazil', 'korea'],
      },
      t,
      'es',
    );
    expect(chips.map((c) => c.text)).toEqual(['USA', 'EUR', 'JPN', '+2']);
    expect(chips[1]?.hidden).toBe('Europa');
    expect(chips[3]?.title).toContain('Brasil');
    expect(chips[3]?.title).toContain('Corea');
    expect(chips[0]?.from).toBe('catalogue');
  });

  it('a_bad_dump_is_danger_and_a_translation_is_a_warning', () => {
    const chips = tagChips(
      {
        flags: {
          verified: false,
          bad: true,
          overdump: false,
          alternate: false,
        },
        flags_from: 'file-name',
        modification: 'translation',
        modification_from: 'file-name',
      },
      t,
      'es',
    );
    const bad = chips.find((c) => c.text === 'Volcado malo [b]');
    expect(bad?.tone).toBe('danger');
    const tr8 = chips.find((c) => c.text === 'Traducción');
    expect(tr8?.tone).toBe('warning');
    expect(tr8?.from).toBe('file-name');
  });

  it('languages_are_one_chip_with_their_names_hidden', () => {
    const chips = tagChips({ languages: ['en', 'fr', 'de'] }, tr('en'), 'en');
    expect(chips).toHaveLength(1);
    expect(chips[0]?.text).toBe('EN FR DE');
    expect(chips[0]?.hidden).toContain('English');
    expect(chips[0]?.hidden).toContain('French');
  });

  it('keeps_display_order_revision_class_other', () => {
    const chips = tagChips(
      {
        tags_from: 'file-name',
        regions: ['world'],
        revision: 'Rev 1',
        class: 'proto',
        modification: 'fixed',
        other: ['Virtual Console', 'Switch Online', 'Collection'],
      },
      t,
      'es',
    );
    expect(chips.map((c) => c.text)).toEqual([
      'WLD',
      'Rev 1',
      'Prototipo',
      'Corregida',
      'Virtual Console',
      'Switch Online',
      '+1',
    ]);
    expect(chips.find((c) => c.text === 'Corregida')?.tone).toBe('neutral');
    expect(chips.at(-1)?.title).toBe('Collection');
  });
});

describe('coverMedia', () => {
  it('uses_the_keepers_title_when_that_is_all_it_has', () => {
    const g = group([
      member(1, '/a.gb', {
        art: [{ media: 'title', path: '/Imgs/a-title.png', size: 10 }],
      }),
      member(2, '/b.gb'),
    ]);
    expect(coverMedia(g)).toEqual({
      media: 'title',
      caption: 'dedupe.cover.media.title',
    });
  });
  it('falls_back_to_another_copys_cover', () => {
    const g = group([
      member(1, '/a.gb'),
      member(2, '/b.gb', {
        art: [{ media: 'screenshot', path: '/Imgs/b.png', size: 10 }],
      }),
    ]);
    expect(coverMedia(g)).toEqual({
      media: 'screenshot',
      caption: 'dedupe.cover.media.screenshot',
    });
  });
  it('asks_for_a_cover_with_no_art_anywhere', () => {
    expect(coverMedia(group([member(1, '/a.gb'), member(2, '/b.gb')]))).toEqual(
      { media: 'cover', caption: null },
    );
  });
});

describe('ownArtDiffers', () => {
  const cover = { media: 'cover' as const, path: '/Imgs/a.png', size: 12_288 };
  const withArt = (path: string, size: number) =>
    member(2, '/b.gb', { art: [{ media: 'cover', path, size }] });
  it('same_path_is_the_same_image', () => {
    expect(ownArtDiffers(withArt('/Imgs/a.png', 1), cover, 'cover')).toBe(
      false,
    );
  });
  it('same_media_and_size_is_the_same_image', () => {
    expect(ownArtDiffers(withArt('/Imgs/b.png', 12_288), cover, 'cover')).toBe(
      false,
    );
  });
  it('a_different_size_is_its_own_art', () => {
    expect(ownArtDiffers(withArt('/Imgs/b.png', 9_001), cover, 'cover')).toBe(
      true,
    );
  });
  it('no_art_in_that_media_shows_nothing', () => {
    expect(ownArtDiffers(member(2, '/b.gb'), cover, 'cover')).toBe(false);
    expect(ownArtDiffers(withArt('/Imgs/b.png', 9_001), cover, 'title')).toBe(
      false,
    );
  });
});
