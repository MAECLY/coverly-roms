import { CircleHelp, Copy, Disc3, History } from 'lucide-react';
import { describe, expect, it } from 'vitest';

import { translate, type Locale, type TranslateVars } from '@/i18n/core';
import {
  DUMP_QUALITIES,
  KEEP_REASON_KINDS,
  RELEASE_CLASSES,
  type DupGroupView,
  type KeepReasonView,
} from '@/ipc/dedupeTypes';
import { dedupeFixture } from '@/ipc/mock/dedupeFixture';

import {
  badgeRelation,
  deltaFor,
  extrasFor,
  formatBytes,
  reasonText,
  relationMeta,
  sortGroups,
  systemSummary,
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
