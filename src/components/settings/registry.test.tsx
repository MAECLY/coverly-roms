import { act, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import { Root } from '@/Root';
import { setBackend } from '@/ipc/backend';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import { createMockBackend, type MockBackend } from '@/ipc/mock/mockBackend';
import { resetLibraryStore } from '@/stores/libraryStore';
import {
  resetNavStore,
  SETTINGS_SECTIONS,
  useNavStore,
  type SettingsSection,
} from '@/stores/navStore';
import { resetSettingsStore } from '@/stores/settingsStore';
import { stubLayout } from '@/test/layout';
import { MemoryStorage } from '@/test/storage';

import en from '../../../locales/en/ui.json';
import es from '../../../locales/es/ui.json';
import { foldCount, SETTINGS_INDEX, type SettingsFold } from './registry';

const FOLDS: readonly SettingsFold[] = [
  'library-advanced',
  'organize-advanced',
  'duplicates-advanced',
];

/** Ids that belong to a control but are not one (its help, hint, error,
 * details). */
const NOT_A_CONTROL = /-(description|hint|error|more)$/;

function lookup(catalog: object, key: string): string | undefined {
  const value = key
    .split('.')
    .reduce<unknown>(
      (node, part) =>
        node && typeof node === 'object'
          ? (node as Record<string, unknown>)[part]
          : undefined,
      catalog,
    );
  return typeof value === 'string' ? value : undefined;
}

let backend: MockBackend;
let restore: () => void;

describe('settings registry', () => {
  beforeEach(() => {
    backend = createMockBackend({
      delayMs: 0,
      now: FIXTURE_NOW,
      storage: new MemoryStorage(),
    });
    setBackend(backend);
    resetSettingsStore();
    resetLibraryStore();
    resetNavStore();
    restore = stubLayout(1280, 800);
  });
  afterEach(() => {
    restore();
    setBackend(null);
  });

  it('ids are unique', () => {
    const ids = SETTINGS_INDEX.map((e) => e.id);
    expect(ids.filter((id, i) => ids.indexOf(id) !== i)).toEqual([]);
  });

  it('every label, hint and keywords key exists in both locales', () => {
    const missing = SETTINGS_INDEX.flatMap((e) =>
      [e.label, e.hint, e.keywords]
        .filter((k): k is string => k !== undefined)
        .flatMap((k) =>
          (
            [
              ['en', en],
              ['es', es],
            ] as const
          )
            .filter(([, catalog]) => lookup(catalog, k) === undefined)
            .map(([l]) => `${l}:${k} (${e.id})`),
        ),
    );
    expect(missing).toEqual([]);
  });

  it('every search keyword group is used by an entry', () => {
    const used = new Set(SETTINGS_INDEX.map((e) => e.keywords));
    const orphans = [en, es].flatMap((catalog) =>
      Object.keys(catalog.settings.keywords)
        .map((k) => `settings.keywords.${k}`)
        .filter((k) => !used.has(k)),
    );
    expect(orphans).toEqual([]);
  });

  it('counts the entries of each fold', () => {
    expect(foldCount('library-advanced')).toBe(2);
    expect(foldCount('organize-advanced')).toBe(1);
    expect(
      FOLDS.every((f) => foldCount(f) > 0),
      'every fold holds at least one setting',
    ).toBe(true);
  });

  it('has an entry for every setting control in every tab, and no other', async () => {
    const settings = (await backend.getSettings()).settings;
    expect(
      Object.keys(settings.dedupe.container_order).length,
      'the mock seeds at least one format order row',
    ).toBeGreaterThan(0);

    render(<Root />);
    await screen.findByRole('button', { name: 'Settings' });
    act(() => {
      for (const f of FOLDS) useNavStore.getState().setFold(f, true);
    });

    const unindexed: string[] = [];
    const absent: string[] = [];
    const wrongFold: string[] = [];
    for (const s of SETTINGS_SECTIONS.filter(
      (x): x is Exclude<SettingsSection, 'about'> => x !== 'about',
    )) {
      act(() => useNavStore.getState().openSettings(s));
      const entries = SETTINGS_INDEX.filter((e) => e.section === s);
      const panel = await screen.findByRole('tabpanel');
      // Wait for the tab's async content (profiles, settings) to render.
      await waitFor(() => {
        for (const e of entries.filter((x) => !x.family))
          if (!document.getElementById(e.id)) throw new Error(e.id);
      }).catch(() => undefined);

      const controls = [...panel.querySelectorAll('[id^="setting-"]')].filter(
        (el) => !NOT_A_CONTROL.test(el.id),
      );
      for (const el of controls) {
        const entry =
          entries.find((e) => e.id === el.id) ??
          entries.find((e) => e.family && el.id.startsWith(e.id));
        if (!entry) {
          unindexed.push(`${s}: ${el.id}`);
          continue;
        }
        const fold =
          el
            .closest('[data-advanced-fold]')
            ?.getAttribute('data-advanced-fold') ?? undefined;
        if (entry.fold !== fold)
          wrongFold.push(`${el.id}: entry ${entry.fold} vs DOM ${fold}`);
      }
      for (const e of entries)
        if (!document.getElementById(e.id)) absent.push(`${s}: ${e.id}`);
    }

    // One assertion, so a failure lists every gap at once.
    expect({ unindexed, absent, wrongFold }).toEqual({
      unindexed: [],
      absent: [],
      wrongFold: [],
    });
  });
});
