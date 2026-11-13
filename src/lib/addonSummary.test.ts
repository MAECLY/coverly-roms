import { describe, expect, it } from 'vitest';

import { translate } from '@/i18n/core';
import type { TFunction } from '@/i18n/useT';
import { addonViewsLibrary } from '@/ipc/mock/addonViewsFixture';

import {
  fitSummary,
  hasContent,
  spokenSummary,
  summaryParts,
  summaryText,
} from './addonSummary';
import { buildGames, type Game } from './games';

const tEs: TFunction = (k, v) => translate('es', k, v);
const tEn: TFunction = (k, v) => translate('en', k, v);

const lib = addonViewsLibrary(1, 1_800_000_000);
const { games, orphans } = buildGames(lib.entries, {});
const named = (prefix: string): Game => {
  const g = games.find((x) => x.entry.name.startsWith(prefix));
  if (!g) throw new Error(prefix);
  return g;
};
const tidewalker = named('Tidewalker');
const lantern = named('Lantern Harbor (USA)');
const lanternDemo = named('Lantern Harbor (Demo)');
const paperSkies = named('Paper Skies');
const neon = named('Neon Rhythm');
const star = named('Star Courier');
const pebble = named('Pebble League');

describe('summaryParts', () => {
  it('summary parts and the missing warning', () => {
    expect(summaryParts(tidewalker)).toEqual([
      { kind: 'dlc', n: 1 },
      { kind: 'missing', v: '1.3.0' },
    ]);
    expect(summaryParts(lantern)).toEqual([
      { kind: 'dlc', n: 1 },
      { kind: 'version', v: '1.10' },
    ]);
  });

  it('revisions, plain games, demos and orphans', () => {
    expect(summaryParts(star)).toEqual([{ kind: 'rev', n: 2 }]);
    expect(summaryParts(neon)).toEqual([{ kind: 'dlc', n: 3 }]);
    expect(summaryParts(pebble)).toEqual([]);
    expect(summaryParts(lanternDemo)).toEqual([]);
    expect(summaryParts(orphans[0]!)).toEqual([]);
  });
});

describe('spokenSummary', () => {
  it('speaks the whole state in Spanish and English', () => {
    expect(spokenSummary(tidewalker, 'Switch', tEs)).toBe(
      'Tidewalker, Switch. 1 DLC, 1 actualización, la más nueva v1.0.2, falta la última actualización, v1.3.0',
    );
    expect(spokenSummary(tidewalker, 'Switch', tEn)).toBe(
      'Tidewalker, Switch. 1 DLC, 1 update, the newest v1.0.2, missing the latest update, v1.3.0',
    );
  });

  it('counts in plural and names the demo', () => {
    expect(spokenSummary(lantern, 'PS4', tEs)).toBe(
      'Lantern Harbor (USA), PS4. 1 DLC, 2 actualizaciones, la más nueva v1.10, tiene demo',
    );
  });

  it('speaks demos with and without their full game', () => {
    expect(spokenSummary(lanternDemo, 'PS4', tEs)).toBe(
      'Lantern Harbor (Demo), PS4. demo de Lantern Harbor (USA)',
    );
    expect(spokenSummary(lanternDemo, 'PS4', tEs, 'Lantern Harbor')).toBe(
      'Lantern Harbor (Demo), PS4. demo de Lantern Harbor',
    );
    expect(spokenSummary(paperSkies, 'PS4', tEs)).toBe(
      'Paper Skies (Demo), PS4. demo; el juego completo no está en la biblioteca',
    );
  });

  it('speaks revisions with the one in use', () => {
    expect(spokenSummary(star, 'Super Nintendo', tEs)).toBe(
      'Star Courier (USA) (Rev 1), Super Nintendo. 2 revisiones, se usa Rev 1',
    );
  });

  it('says new content and nothing more for a plain game', () => {
    expect(spokenSummary(neon, 'PS4', tEs, undefined, { isNew: true })).toBe(
      'Neon Rhythm, PS4. 3 DLC, contenido nuevo desde el último escaneo',
    );
    expect(spokenSummary(pebble, 'Switch', tEs)).toBe('Pebble League, Switch');
  });

  it('formats numbers for the locale', () => {
    const big: Game = {
      ...neon,
      dlc: Array.from({ length: 1200 }, () => neon.dlc[0]!),
    };
    expect(spokenSummary(big, 'PS4', tEs, undefined, { locale: 'es' })).toBe(
      'Neon Rhythm, PS4. 1200 DLC',
    );
    expect(spokenSummary(big, 'PS4', tEn, undefined, { locale: 'en' })).toBe(
      'Neon Rhythm, PS4. 1,200 DLC',
    );
  });

  it('is memoised per game', () => {
    const one = spokenSummary(lantern, 'PS4', tEs);
    expect(spokenSummary(lantern, 'PS4', tEs)).toBe(one);
    expect(spokenSummary(lantern, 'PS4', tEn)).not.toBe(one);
  });
});

describe('summaryText and fitSummary (§8.2)', () => {
  const all = [
    { kind: 'dlc', n: 3 },
    { kind: 'rev', n: 2 },
    { kind: 'missing', v: '1.3.0' },
  ] as const;

  it('writes each part with the approved copy', () => {
    expect(all.map((p) => summaryText(p, tEs))).toEqual([
      '+3 DLC',
      '2 rev.',
      '↑ falta v1.3.0',
    ]);
    expect(summaryText({ kind: 'version', v: '1.05' }, tEs)).toBe('v1.05');
    expect(summaryText({ kind: 'missing', v: '1.3.0' }, tEn)).toBe(
      '↑ v1.3.0 missing',
    );
  });

  it('keeps every part when they fit', () => {
    expect(fitSummary(all, tEs, 40)).toEqual(all);
  });

  it('drops +N DLC first, then N rev., never the warning', () => {
    // «2 rev. · ↑ falta v1.3.0» is 23 characters.
    expect(fitSummary(all, tEs, 23)).toEqual([all[1], all[2]]);
    expect(fitSummary(all, tEs, 22)).toEqual([all[2]]);
    expect(fitSummary(all, tEs, 3)).toEqual([all[2]]);
  });

  it('never drops the installed version either', () => {
    const v = [
      { kind: 'dlc', n: 1 } as const,
      { kind: 'version', v: '1.10' } as const,
    ];
    expect(fitSummary(v, tEs, 4)).toEqual([v[1]]);
  });
});

describe('hasContent', () => {
  it('is true when any game has add-ons, revisions or is a demo', () => {
    expect(hasContent(games)).toBe(true);
    expect(hasContent([pebble])).toBe(false);
    expect(hasContent([star])).toBe(true);
    expect(hasContent([lanternDemo])).toBe(true);
    expect(hasContent([])).toBe(false);
  });
});
