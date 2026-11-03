import { describe, expect, it } from 'vitest';

import es from '../../locales/es/ui.json';
import { translate } from '@/i18n/core';
import { KEY_CHECKS } from '@/ipc/types';

import { CREDITS } from './credits';
import {
  INTEGRATIONS,
  PLANNED,
  WORKING,
  catalogCardState,
  integration,
  integrationDomId,
  integrationForSource,
  toIntegrations,
} from './integrations';

describe('integrations registry', () => {
  it('lists the working cards in page order', () => {
    expect(WORKING.map((e) => e.id)).toEqual([
      'steamgriddb',
      'screenscraper',
      'switch-catalog',
      'updates',
    ]);
  });

  it('lists the planned services, none with fields or a test', () => {
    expect(PLANNED.map((e) => e.name)).toEqual(
      expect.arrayContaining([
        'IGDB',
        'RetroAchievements',
        'TheGamesDB',
        'RAWG',
        'Microsoft Store',
        'GameTDB',
        'Wikidata',
      ]),
    );
    for (const e of PLANNED) {
      expect(e.fields, e.id).toHaveLength(0);
      expect(e.test, e.id).toBeNull();
      expect(e.roadmap, e.id).not.toBeNull();
    }
    expect(WORKING.length + PLANNED.length).toBe(INTEGRATIONS.length);
  });

  it('normalises absent arrays and nulls', () => {
    const sw = integration('switch-catalog');
    expect(sw?.fields).toEqual([]);
    expect(sw?.needs).toEqual([]);
    expect(sw?.test).toBeNull();
    expect(sw?.help_url).toBeNull();
    expect(sw?.source).toBeNull();
    expect(integration('updates')?.credits).toEqual([]);
  });

  it('names a credit that exists for every credit id', () => {
    const ids = new Set(CREDITS.map((c) => c.id));
    for (const e of INTEGRATIONS)
      for (const c of e.credits) expect(ids.has(c), `${e.id}: ${c}`).toBe(true);
  });

  it('finds the integration behind an art source', () => {
    expect(integrationForSource('steamgriddb')?.id).toBe('steamgriddb');
    expect(integrationForSource('screenscraper')?.id).toBe('screenscraper');
    expect(integrationForSource('libretro')).toBeUndefined();
  });

  it('finds entries by id and builds their DOM id', () => {
    expect(integration('screenscraper')?.kind).toBe('account');
    expect(integration('igdb')?.status).toBe('planned');
    expect(integration('nope')).toBeUndefined();
    expect(integrationDomId('steamgriddb')).toBe('integration-steamgriddb');
  });

  it('refuses a malformed file, naming the entry', () => {
    expect(() => toIntegrations({})).toThrow(/no entries/);
    expect(() =>
      toIntegrations({
        entries: [{ id: 'x', name: 'X', kind: 'api-key', group: 'art' }],
      }),
    ).toThrow(/entry x/);
    expect(() =>
      toIntegrations({
        entries: [
          {
            id: 'x',
            name: 'X',
            kind: 'api-key',
            group: 'art',
            status: 'working',
            fields: [{ key: 'igdb-secret', input: 'secret', required: true }],
          },
        ],
      }),
    ).toThrow(/entry x/);
  });
});

describe('integrations copy (both locales)', () => {
  const LOCALES = ['es', 'en'] as const;
  /** Fails naming the locale and key when a string is missing or empty. */
  const expectCopy = (key: string) => {
    for (const l of LOCALES) {
      const v = translate(l, key);
      expect(v !== key && v.trim() !== '', `${l}: ${key}`).toBe(true);
    }
  };

  it('every working entry has use, privacy and keywords', () => {
    for (const e of WORKING)
      for (const part of ['use', 'privacy', 'keywords'])
        expectCopy(`integrations.service.${e.id}.${part}`);
  });

  it('entries with a help_url have steps', () => {
    const withHelp = WORKING.filter((e) => e.help_url !== null);
    expect(withHelp.length).toBeGreaterThan(0);
    for (const e of withHelp) expectCopy(`integrations.service.${e.id}.steps`);
  });

  it('every working entry but updates has unlocks', () => {
    for (const e of WORKING.filter((e) => e.id !== 'updates'))
      expectCopy(`integrations.service.${e.id}.unlocks`);
  });

  it('the catalogue and updates cards have a translated name', () => {
    for (const id of ['switch-catalog', 'updates'])
      expectCopy(`integrations.service.${id}.name`);
  });

  it('every planned entry says what it is for and what it will need', () => {
    for (const e of PLANNED) {
      expectCopy(`integrations.service.${e.id}.use`);
      for (const n of e.needs) expectCopy(`integrations.needs.${n}`);
    }
    expectCopy('integrations.needs.none');
  });

  it('every field input has a label, and stored ones a saved line', () => {
    for (const input of ['user', 'password', 'api-key', 'client-id', 'secret'])
      expectCopy(`integrations.field.${input}.label`);
    for (const e of WORKING)
      for (const f of e.fields)
        expectCopy(`integrations.field.${f.input}.saved`);
  });

  it('every group, state tag and test outcome has copy', () => {
    for (const e of INTEGRATIONS) expectCopy(`integrations.group.${e.group}`);
    expectCopy('integrations.group.planned');
    for (const s of [
      'off',
      'unavailable',
      'not-configured',
      'saved',
      'connected',
      'error',
      'quota',
      'pending',
      'downloading',
      'ready',
    ])
      expectCopy(`integrations.state.${s}`);
    for (const k of KEY_CHECKS) expectCopy(`integrations.check.${k}`);
  });

  it('has an off line for every feature a working entry powers', () => {
    const lines: Record<string, string> = {
      fill: 'fill',
      'organize-identify': 'fill',
      'organize-names': 'organize',
    };
    for (const e of WORKING.filter((e) => e.id !== 'updates'))
      expectCopy(`integrations.stateLine.off.${lines[e.powers[0] ?? '']}`);
  });

  it('keeps the same placeholders in the quota and connected lines', () => {
    expect(
      translate('es', 'integrations.stateLine.quota', { used: 1, max: 2 }),
    ).toMatch(/1 de 2/);
    expect(
      translate('en', 'integrations.stateLine.quota', { used: 1, max: 2 }),
    ).toMatch(/1 of 2/);
    expect(
      translate('es', 'integrations.stateLine.connected', {
        time: 'hace 1 minuto',
      }),
    ).toBe('Comprobado hace 1 minuto.');
  });

  it('writes Spanish without exclamation marks', () => {
    const walk = (node: unknown, path: string): void => {
      if (typeof node === 'string') {
        expect(node.includes('!') || node.includes('¡'), path).toBe(false);
        return;
      }
      for (const [k, v] of Object.entries(node as object))
        walk(v, `${path}.${k}`);
    };
    walk(es.integrations, 'integrations');
    walk(es.errors.keychainOs, 'errors.keychainOs');
  });
});

describe('catalogCardState (D23)', () => {
  it('maps off and ready', () => {
    expect(catalogCardState({ state: 'off' })).toEqual({ state: 'off' });
    expect(
      catalogCardState({
        state: 'ready',
        count: 3,
        updated: '2026-10-01',
        fallback: false,
      }),
    ).toEqual({ state: 'connected', label: 'ready' });
  });

  it('shows pending and downloading as saved, with progress', () => {
    expect(catalogCardState({ state: 'pending' })).toEqual({
      state: 'saved',
      label: 'pending',
    });
    expect(
      catalogCardState({ state: 'downloading', done: 2, total: 10 }),
    ).toEqual({
      state: 'saved',
      label: 'downloading',
      progress: { done: 2, total: 10 },
    });
  });

  it('maps every failure to error with its reason', () => {
    for (const s of [
      { state: 'failed-old', updated: '2026-10-01' },
      { state: 'failed-none' },
      { state: 'rejected' },
      { state: 'remove-failed' },
    ] as const)
      expect(catalogCardState(s)).toEqual({ state: 'error', reason: s.state });
  });

  it('reads an unknown status (not loaded yet) as saved', () => {
    expect(catalogCardState(null)).toEqual({ state: 'saved' });
  });
});
