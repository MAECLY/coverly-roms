import { afterEach, describe, expect, it } from 'vitest';

import { groupBySystem, licencesStaying } from '@/lib/organize';

import type { OrganizeItem, OrganizePlan } from '../types';
import { FIXTURE_NOW } from './fixture';
import { MOCK_GAMEFOLDERS } from './gameFoldersFixture';
import { createMockBackend, type MockBackend } from './mockBackend';
import { readMockPick } from './mockOrganize';

function backend(): MockBackend {
  return createMockBackend({
    delayMs: 0,
    now: FIXTURE_NOW,
    storage: null,
    pauseAfter: null,
  });
}

const by = (plan: OrganizePlan, name: string): OrganizeItem => {
  const found = plan.items.find((i) => i.sources[0] === name);
  if (!found) throw new Error(`no item ${name}`);
  return found;
};

const XENO = 'Xenoblade Chronicles 3 (World)';

afterEach(() => {
  window.history.replaceState(null, '', '/');
});

describe('mock game folders pile (spec §9.3)', () => {
  it('is what ?mockPick=gamefolders opens', () => {
    window.history.replaceState(null, '', '/?mockPick=gamefolders');
    expect(readMockPick()).toBe(MOCK_GAMEFOLDERS);
  });

  it('nests bases, updates and DLC on the Coverly layout', async () => {
    const b = backend();
    const { plan } = await b.organizePlan({
      source: MOCK_GAMEFOLDERS,
      layout: 'coverly-library',
    });
    const base = by(plan, 'Pokemon Y.cia');
    expect(base).toMatchObject({
      action: 'auto',
      target: 'ROMS/3DS/Pokemon Y (World)/Pokemon Y (World).cia',
      rename: { kind: 'game-folder', from: 'Pokemon Y.cia' },
      game: { role: 'base', folder: 'ROMS/3DS/Pokemon Y (World)' },
    });
    expect(by(plan, '0004000E00055D00.cia')).toMatchObject({
      target:
        'ROMS/3DS/Pokemon Y (World)/Updates/Pokemon Y (World) (v1.1.0).cia',
      game: {
        role: 'update',
        of: base.id,
        version: '1.1.0',
        newest_here: true,
      },
    });
    expect(by(plan, 'Pokemon Y [DLC2].cia').target).toBe(
      'ROMS/3DS/Pokemon Y (World)/DLC/Pokemon Y (World) - DLC 0004008C00055D01.cia',
    );
    // A demo is its own game, linked by name only.
    expect(by(plan, 'Resistance (USA) (Demo).pkg')).toMatchObject({
      target: 'ROMS/PS3/Resistance (USA) (Demo)/Resistance (USA) (Demo).pkg',
      game: { role: 'demo', demo_of: 'Resistance (USA)' },
      notes: [{ kind: 'demo-of', of: 'Resistance (USA)' }],
    });
    // The licence goes with its game by a rename (same volume, move).
    const resistance = by(plan, 'Resistance (USA).pkg');
    expect(resistance.companions).toEqual([
      {
        source: 'BLUS30001.rap',
        target: 'ROMS/PS3/Resistance (USA)/BLUS30001.rap',
        licence: true,
      },
    ]);
    expect(by(plan, 'BLUS30001.rap')).toMatchObject({
      action: 'left',
      reason: { kind: 'licence-of', of: 'Resistance (USA).pkg' },
    });
    // A global key file never matches a game.
    expect(by(plan, 'prod.keys')).toMatchObject({
      reason: { kind: 'key-material' },
    });
    // The orphan update is reviewed: its game is not here.
    expect(by(plan, 'Splatoon (USA) [Update v288].wua')).toMatchObject({
      action: 'review',
      notes: [{ kind: 'update', of: 'Splatoon' }, { kind: 'base-missing' }],
    });
    // A game folder in the pile: Updates/ and DLC/ follow, Mods/ and
    // Saves/ stay.
    expect(by(plan, `${XENO}/Updates/${XENO} [v2].nsp`).target).toBe(
      `ROMS/SWITCH/${XENO}/Updates/${XENO} (v131072).nsp`,
    );
    const left = plan.items.filter(
      (i) => i.action === 'left' && i.reason.kind === 'game-folder-content',
    );
    expect(left.map((i) => i.sources[0])).toEqual([
      `${XENO}/Mods/romfs.bin`,
      `${XENO}/Saves/save.dat`,
    ]);
    expect(by(plan, 'PS3UPDAT.PUP')).toMatchObject({
      reason: { kind: 'not-a-rom', what: 'console-system' },
    });
    expect(by(plan, 'shadercache/0100152000022000.bin')).toMatchObject({
      reason: { kind: 'not-a-rom', what: 'emulator-cache' },
    });
    // Retro revisions stay loose files.
    expect(by(plan, 'Sonic the Hedgehog (USA, Europe) (Rev 1).md').target).toBe(
      'ROMS/MD/Sonic the Hedgehog (USA, Europe) (Rev 1).md',
    );
    // Rock Band's 25 DLC fold under it.
    const ps4 = groupBySystem(plan).find((g) => g.systemId === 'ps4')!;
    const rock = ps4.items.findIndex(
      (i) => i.sources[0] === 'Rock Band 4 (USA).pkg',
    );
    expect(
      ps4.items
        .slice(rock + 1, rock + 26)
        .every(
          (i) => i.game?.role === 'dlc' && i.game.of === ps4.items[rock]!.id,
        ),
    ).toBe(true);
  });

  it('keeps the flat layout on a handheld profile, still folded', async () => {
    const b = backend();
    const { plan } = await b.organizePlan({
      source: MOCK_GAMEFOLDERS,
      layout: 'anbernic-stock',
    });
    for (const i of plan.items) expect(i.game, i.sources[0]).toBeUndefined();
    expect(by(plan, '0004000E00055D00.cia').target).toMatch(
      /\/0004000E00055D00\.cia$/,
    );
    expect(by(plan, `${XENO}/Updates/${XENO} [v2].nsp`).target).toMatch(
      new RegExp(`/${XENO.replace(/[()]/g, '\\$&')} \\[v2\\]\\.nsp$`),
    );
    // The licence stays; the base row says so.
    expect(by(plan, 'BLUS30001.rap')).toMatchObject({
      reason: { kind: 'key-material' },
    });
    expect(licencesStaying(plan).has(by(plan, 'Resistance (USA).pkg').id)).toBe(
      true,
    );
    // The orphan moves flat, no review.
    expect(by(plan, 'Splatoon (USA) [Update v288].wua').action).toBe('auto');
    // Folded by name all the same.
    const n3ds = groupBySystem(plan).find((g) => g.systemId === '3ds')!;
    expect(n3ds.items.map((i) => i.sources[0])).toEqual([
      'Pokemon Y.cia',
      '0004000E00055D00.cia',
      'Pokemon Y - Extra Pack (USA).cia',
      'Pokemon Y [DLC2].cia',
    ]);
  });

  it('follows the run override and the user override', async () => {
    const b = backend();
    const on = await b.organizePlan({
      source: MOCK_GAMEFOLDERS,
      layout: 'anbernic-stock',
      game_folders: true,
    });
    expect(by(on.plan, 'Pokemon Y.cia').game?.role).toBe('base');
    await b.updateSettings({
      organize: { game_folders: { 'coverly-library': false } },
    });
    const off = await b.organizePlan({
      source: MOCK_GAMEFOLDERS,
      layout: 'coverly-library',
    });
    expect(by(off.plan, 'Pokemon Y.cia').game).toBeUndefined();
  });

  it('a reviewed orphan accepted by the user moves flat', async () => {
    const b = backend();
    const first = await b.organizePlan({
      source: MOCK_GAMEFOLDERS,
      layout: 'coverly-library',
    });
    const orphan = by(first.plan, 'Splatoon (USA) [Update v288].wua');
    const next = await b.organizePlan(
      { source: MOCK_GAMEFOLDERS, layout: 'coverly-library' },
      [{ item: orphan.id, choice: 'accept' }],
    );
    expect(by(next.plan, 'Splatoon (USA) [Update v288].wua')).toMatchObject({
      action: 'auto',
      target: 'ROMS/WIIU/Splatoon (USA) [Update v288].wua',
    });
  });
});

describe('mock child icons for Organizar (addon views D15)', () => {
  it('serves a child’s icon by plan and item id, none for a base or Switch', async () => {
    const b = backend();
    const { plan_id, plan } = await b.organizePlan({
      source: MOCK_GAMEFOLDERS,
      layout: 'coverly-library',
    });
    const dlc = by(plan, 'Pokemon Y - Extra Pack (USA).cia');
    const icon = await b.organizeChildThumb(plan_id, dlc.id);
    expect(icon.mime).toBe('image/svg+xml');
    await expect(
      b.organizeChildThumb(plan_id, by(plan, 'Pokemon Y.cia').id),
    ).rejects.toMatchObject({ code: 'art-missing' });
    const switchDlc = plan.items.find(
      (i) => i.system_id === 'switch' && i.game?.role === 'dlc',
    )!;
    await expect(
      b.organizeChildThumb(plan_id, switchDlc.id),
    ).rejects.toMatchObject({ code: 'art-missing' });
    await expect(b.organizeChildThumb(plan_id, 9999)).rejects.toMatchObject({
      code: 'unknown-entry',
    });
  });
});
