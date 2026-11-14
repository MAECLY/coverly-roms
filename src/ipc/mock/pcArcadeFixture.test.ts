import { afterEach, describe, expect, it } from 'vitest';

import type { OrganizeItem, OrganizePlan } from '../types';
import { FIXTURE_NOW } from './fixture';
import {
  createMockBackend,
  MOCK_BUILTIN_PROFILES,
  type MockBackend,
} from './mockBackend';
import { readMockPick } from './mockOrganize';
import { MOCK_PCARCADE, MOCK_PCARCADE_PILE } from './pcArcadeFixture';

function backend(): MockBackend {
  return createMockBackend({
    delayMs: 0,
    now: FIXTURE_NOW,
    storage: null,
    pauseAfter: null,
  });
}

/** The title `parse_name` makes of an entry's name (spec P7): bracket
 * groups such as «(Type X)» never reach it, and a trailing
 * «for NesicaxLive» is dropped. */
const titleOf = (name: string): string =>
  name
    .replace(/\s*[([][^)\]]*[)\]]/g, '')
    .replace(/\s+for\s+NesicaxLive$/i, '')
    .trim();

const by = (plan: OrganizePlan, name: string): OrganizeItem => {
  const found = plan.items.find((i) => i.sources[0] === name);
  if (!found) throw new Error(`no item ${name}`);
  return found;
};

const folders = (id: string): Record<string, string> => {
  const p = MOCK_BUILTIN_PROFILES.find((x) => x.profile.id === id);
  if (!p) throw new Error(`no profile ${id}`);
  return p.profile.folder_names ?? {};
};

afterEach(() => {
  window.history.replaceState(null, '', '/');
});

describe('mock PC arcade library (spec §10, plan D-3)', () => {
  it('is what ?mockPick=pcarcade opens', () => {
    window.history.replaceState(null, '', '/?mockPick=pcarcade');
    expect(readMockPick()).toBe(MOCK_PCARCADE);
    expect(MOCK_PCARCADE).toBe('/Users/demo/PcArcade');
  });

  it('scans an ES-DE library with type-x and cave3rd shelves', async () => {
    const lib = await backend().openLibrary(MOCK_PCARCADE);
    expect(lib.profile.id).toBe('esde');
    const typex = lib.systems.find((s) => s.folder === 'type-x');
    expect(typex).toMatchObject({ system_id: 'typex', roms: 5 });
    const games = lib.entries.filter((e) => e.system === typex!.index);
    expect(games.map((e) => titleOf(e.name)).sort()).toEqual([
      'Akai Katana Shin',
      'Chaos Code',
      'Empty Game',
      'Homura',
      'Raiden IV',
    ]);
    // Folder games: the folder is the entry, never a file inside it.
    for (const e of games)
      expect(e.rel_path.split('/').at(-1), e.rel_path).not.toMatch(
        /^game\.exe$/i,
      );
    // ES-DE's `Title.exe/` pattern: the suffix is the entry's `ext`.
    expect(games.find((e) => e.name === 'Chaos Code')).toMatchObject({
      rel_path: 'ROMs/type-x/Chaos Code.exe',
      ext: 'exe',
    });
    const cave = lib.systems.find((s) => s.folder === 'cave3rd');
    expect(cave).toMatchObject({ system_id: 'cave3rd', roms: 1 });
    expect(
      lib.entries
        .filter((e) => e.system === cave!.index)
        .map((e) => e.rel_path),
    ).toEqual(['ROMs/cave3rd/mushisam.zip']);
  });
});

describe('mock PC arcade pile (spec §9, plan D-3)', () => {
  it('is what ?mockPick=pcarcade-pile opens', () => {
    window.history.replaceState(null, '', '/?mockPick=pcarcade-pile');
    expect(readMockPick()).toBe(MOCK_PCARCADE_PILE);
    expect(MOCK_PCARCADE_PILE).toBe('/Users/demo/PcArcadePile');
  });

  it('sends a loose set to fbneo and keeps cave3rd and Type X alone on Batocera', async () => {
    const { plan } = await backend().organizePlan({
      source: MOCK_PCARCADE_PILE,
      layout: 'batocera',
    });
    const set = by(plan, 'mushisam.zip');
    expect(set).toMatchObject({ action: 'auto', system_id: 'fbneo' });
    expect(set.target?.split('/').slice(-2)).toEqual(['fbneo', 'mushisam.zip']);
    // The set already in `cave3rd/` stays: no item, a sorted folder.
    expect(plan.sorted_folders).toContainEqual(['roms/cave3rd', 'cave3rd']);
    expect(
      plan.items.flatMap((i) => i.sources).some((s) => s.includes('cave3rd')),
    ).toBe(false);
    // The loose Type X folder and a loose game.exe are left as other
    // files: recognised by location only, never by content (spec §9).
    expect(plan.other_files).toEqual(
      expect.arrayContaining(['Raiden IV (Type X)/game.exe', 'game.exe']),
    );
    for (const i of plan.items) {
      expect(i.system_id, i.sources[0]).not.toBe('typex');
      expect(
        i.alternatives.map(([system]) => system),
        i.sources[0],
      ).not.toContain('typex');
    }
    expect(plan.items).toHaveLength(1);
  });
});

describe('mock PC arcade profile rows (spec P14)', () => {
  it('names type-x on ES-DE and cave3rd on Batocera only', () => {
    expect(folders('esde')).toMatchObject({ typex: 'type-x' });
    expect(folders('esde')).not.toHaveProperty('cave3rd');
    expect(folders('batocera')).toMatchObject({ cave3rd: 'cave3rd' });
    expect(folders('batocera')).not.toHaveProperty('typex');
    for (const id of ['anbernic-stock', 'muos', 'minui', 'onion'])
      for (const sys of ['typex', 'cave3rd'])
        expect(folders(id), `${id} ${sys}`).not.toHaveProperty(sys);
  });
});
