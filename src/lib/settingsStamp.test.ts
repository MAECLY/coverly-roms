import { describe, expect, it } from 'vitest';

import { createMockBackend } from '@/ipc/mock/mockBackend';
import type { Settings } from '@/ipc/types';

import { dedupeStamp } from './settingsStamp';

async function settings(): Promise<Settings> {
  const b = createMockBackend({ delayMs: 0, storage: null });
  return (await b.getSettings()).settings;
}

const copy = (s: Settings): Settings => structuredClone(s);

describe('dedupeStamp', () => {
  it('changes with the keeper rules and the region order', async () => {
    const s = await settings();
    const rules = copy(s);
    rules.dedupe.keep_rules = [...rules.dedupe.keep_rules].reverse();
    expect(dedupeStamp(rules)).not.toBe(dedupeStamp(s));
    const regions = copy(s);
    regions.matching.regions = [...regions.matching.regions].reverse();
    expect(dedupeStamp(regions)).not.toBe(dedupeStamp(s));
  });

  it('changes with the left-out systems and the folder name', async () => {
    const s = await settings();
    const out = copy(s);
    out.dedupe.exclude_systems = [...out.dedupe.exclude_systems, 'arcade'];
    expect(dedupeStamp(out)).not.toBe(dedupeStamp(s));
    const name = copy(s);
    name.dedupe.quarantine_name = 'Aparte';
    expect(dedupeStamp(name)).not.toBe(dedupeStamp(s));
  });

  it('ignores what the screen starts with or sets itself', async () => {
    const s = await settings();
    const next = copy(s);
    next.dedupe.action = s.dedupe.action === 'trash' ? 'quarantine' : 'trash';
    next.dedupe.levels = ['exact'];
    next.dedupe.one_game_one_rom = !s.dedupe.one_game_one_rom;
    next.dedupe.quarantine_dir = '/elsewhere';
    expect(dedupeStamp(next)).toBe(dedupeStamp(s));
  });
});
