import { describe, expect, it } from 'vitest';

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

  // The copy keys land with the locale work (plan Task 7), which turns these on.
  it.todo(
    'Task 7: every working entry has use, privacy and keywords in both locales',
  );
  it.todo('Task 7: entries with a help_url have steps in both locales');
  it.todo(
    'Task 7: every working entry but updates has unlocks in both locales',
  );
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
