import { describe, expect, it } from 'vitest';

import { MemoryStorage } from '@/test/storage';

import type { FillRequest, JobState, JobView } from '../types';
import { FIXTURE_NOW } from './fixture';
import {
  createMockBackend,
  MOCK_LIBRARIES,
  type MockBackend,
  type MockOptions,
} from './mockBackend';
import { MOCK_PAUSE_DETAIL, mockClass, mockFailure } from './mockJobs';

const SMALL = MOCK_LIBRARIES[1].path;
const REQUEST: FillRequest = {
  entry_ids: null,
  media: ['cover'],
  strategy: 'rom-region',
  regions: ['rom', 'world', 'usa', 'europe', 'japan'],
  overwrite: 'never',
};

async function opened(options: MockOptions = {}) {
  const b = createMockBackend({
    delayMs: 0,
    now: FIXTURE_NOW,
    pauseAfter: null,
    ...options,
  });
  const view = await b.openLibrary(SMALL);
  return { b, view };
}

/** The next job view in `state` (register before the action). */
function next(b: MockBackend, state: JobState): Promise<JobView> {
  return new Promise((resolve) => {
    void b.onJobChanged((v) => {
      if (v.state === state) resolve(v);
    });
  });
}

async function planned(b: MockBackend, generation: number, request = REQUEST) {
  const done = next(b, 'planned');
  const job = await b.startFill(generation, request);
  expect(job.state).toBe('gathering');
  return done;
}

describe('mock fill jobs', () => {
  it('plans every missing cover with deterministic auto, review and none', async () => {
    const { b, view } = await opened();
    const progress: number[] = [];
    await b.onJobProgress((p) => progress.push(p.done));
    const v = await planned(b, view.generation);
    const missing = view.entries.filter((e) => e.art.cover === 'missing');
    const count = (c: string) =>
      missing.filter((e) => mockClass(e.id, 'cover') === c).length;
    expect(v.plan?.totals).toEqual({
      auto: count('auto'),
      review: count('review'),
      none: count('none'),
    });
    expect(progress[0]).toBe(0);
    const items = await b.fillJobItems(v.id, 'all');
    expect(items).toHaveLength(missing.length);
  });

  it('plans only the given entries', async () => {
    const { b, view } = await opened();
    const ids = view.entries
      .filter((e) => e.art.cover === 'missing')
      .slice(0, 3)
      .map((e) => e.id);
    const v = await planned(b, view.generation, {
      ...REQUEST,
      entry_ids: [...ids, ids[0]!, 99_999],
    });
    const items = await b.fillJobItems(v.id, 'all');
    expect(items.map((i) => i.entry_id)).toEqual(ids);
  });

  it('writes auto and decided review items, lists failures and updates the library', async () => {
    const { b, view } = await opened();
    const updated: number[] = [];
    await b.onEntriesUpdated((u) =>
      updated.push(...u.entries.map((e) => e.id)),
    );
    const v = await planned(b, view.generation);
    const review = await b.fillJobItems(v.id, 'review');
    expect(review.length).toBeGreaterThan(1);
    const picked = review[0]!;
    await b.decideFill(v.id, [
      { index: picked.index, choice: 'pick', candidate: 1 },
      { index: review[1]!.index, choice: 'skip' },
    ]);
    const done = next(b, 'done');
    await b.runFill(v.id);
    const result = (await done).result!;
    const autos = await b.fillJobItems(v.id, 'auto');
    const failing = autos.filter(
      (i) => mockFailure(i.entry_id!) !== null,
    ).length;
    expect(result.filled).toBe(autos.length - failing + 1);
    expect(result.no_art + result.failed).toBe(failing);
    expect(await b.fillJobItems(v.id, 'failed')).toHaveLength(failing);
    expect(updated).toContain(picked.entry_id);
    const art = await b.readArt(view.generation, picked.entry_id!, 'cover');
    expect(art.mime).toBe('image/svg+xml');
    const skipped = (await b.fillJobItems(v.id, 'review')).find(
      (i) => i.index === review[1]!.index,
    );
    expect(skipped?.outcome?.status).toBe('skipped-no-match');
  });

  it('pauses once with a rate limit and resumes into the same journal', async () => {
    const { b, view } = await opened({ pauseAfter: 3 });
    const v = await planned(b, view.generation);
    const paused = next(b, 'paused');
    await b.runFill(v.id);
    const p = await paused;
    expect(p.paused).toEqual({
      phase: 'apply',
      kind: 'rate-limit',
      detail: MOCK_PAUSE_DETAIL,
    });
    expect(p.journal).not.toBeNull();
    await expect(b.undoLastJob(view.generation)).rejects.toMatchObject({
      code: 'job-running',
    });
    const done = next(b, 'done');
    await b.resumeFill(v.id);
    expect((await done).journal).toBe(p.journal);
    expect(await b.jobHistory()).toHaveLength(1);
  });

  it.each([
    ['the same root again', SMALL],
    ['another root', MOCK_LIBRARIES[0].path],
  ])(
    'a resumed job never writes into a library opened since (%s)',
    async (_name, path) => {
      const { b, view } = await opened({ pauseAfter: 3 });
      const v = await planned(b, view.generation);
      const paused = next(b, 'paused');
      await b.runFill(v.id);
      await paused;
      const other = await b.openLibrary(path);
      await b.exportAudit(other.generation, 'json');
      const updated: number[] = [];
      await b.onEntriesUpdated((u) => updated.push(u.generation));
      const done = next(b, 'done');
      await b.resumeFill(v.id);
      expect((await done).result!.filled).toBeGreaterThan(3);
      await b.exportAudit(other.generation, 'json');
      expect(b.exports[1]!.text).toBe(b.exports[0]!.text);
      expect(updated).toEqual([view.generation]);
    },
  );

  it('undo restores the marks and history shows it undone', async () => {
    const { b, view } = await opened();
    const v = await planned(b, view.generation);
    const done = next(b, 'done');
    await b.runFill(v.id);
    const result = (await done).result!;
    const undo = await b.undoLastJob(view.generation);
    expect(undo.removed).toBe(result.filled);
    expect(undo.entries.every((e) => e.art.cover === 'missing')).toBe(true);
    expect((await b.jobHistory())[0]?.undone).toBe(true);
    await expect(b.undoLastJob(view.generation)).rejects.toMatchObject({
      code: 'nothing-to-undo',
    });
  });

  it('refuses a second job while one runs, stale libraries and bad requests', async () => {
    const { b, view } = await opened();
    const done = next(b, 'planned');
    await b.startFill(view.generation, REQUEST);
    await expect(b.startFill(view.generation, REQUEST)).rejects.toMatchObject({
      code: 'job-running',
    });
    await expect(b.undoLastJob(view.generation)).rejects.toMatchObject({
      code: 'job-running',
    });
    const v = await done;
    await expect(
      b.startFill(view.generation + 1, REQUEST),
    ).rejects.toMatchObject({ code: 'stale-library' });
    await expect(
      b.startFill(view.generation, { ...REQUEST, regions: ['mars' as 'usa'] }),
    ).rejects.toMatchObject({ code: 'bad-request' });
    await expect(b.resumeFill(v.id)).rejects.toMatchObject({
      code: 'bad-request',
    });
    await expect(b.runFill(v.id + 1)).rejects.toMatchObject({
      code: 'stale-job',
    });
  });

  it('refuses decisions the real backend refuses, recording none of them', async () => {
    const { b, view } = await opened();
    const v = await planned(b, view.generation);
    const review = (await b.fillJobItems(v.id, 'review'))[0]!;
    const none = (await b.fillJobItems(v.id, 'all')).find(
      (i) => i.action === 'none',
    )!;
    const last = review.candidates.length;
    await expect(
      b.decideFill(v.id, [
        { index: review.index, choice: 'accept' },
        { index: review.index, choice: 'pick', candidate: last },
      ]),
    ).rejects.toMatchObject({ code: 'bad-request' });
    await expect(
      b.decideFill(v.id, [{ index: none.index, choice: 'accept' }]),
    ).rejects.toMatchObject({ code: 'bad-request' });
    await expect(
      b.decideFill(v.id, [{ index: none.index, choice: 'skip' }]),
    ).rejects.toMatchObject({ code: 'bad-request' });
    expect((await b.fillJob())?.decided).toBe(0);
    const ok = await b.decideFill(v.id, [
      { index: review.index, choice: 'pick', candidate: last - 1 },
    ]);
    expect(ok.decided).toBe(1);
  });

  it('changes, sets from file and removes art from the drawer', async () => {
    const { b, view } = await opened();
    const e = view.entries.find((x) => x.art.cover === 'missing')!;
    await expect(
      b.setEntryArt(view.generation, e.id, 'cover', 0),
    ).rejects.toMatchObject({ code: 'bad-request' });
    const cands = await b.entryCandidates(view.generation, e.id, 'cover');
    expect(cands).toHaveLength(3);
    const art = await b.entryCandidateArt(view.generation, e.id, 'cover', 1);
    expect(art.mime).toBe('image/svg+xml');
    const [set] = await b.setEntryArt(view.generation, e.id, 'cover', 1);
    expect(set?.art.cover).toBe('present');
    const [removed] = await b.removeEntryArt(view.generation, e.id, 'cover');
    expect(removed?.art.cover).toBe('missing');
    await expect(
      b.removeEntryArt(view.generation, e.id, 'cover'),
    ).rejects.toMatchObject({ code: 'art-missing' });
    const undo = await b.undoLastJob(view.generation);
    expect(undo.restored).toBe(1);
    expect(undo.entries[0]?.art.cover).toBe('present');
    expect(
      await b.setEntryArtFromFile(view.generation, e.id, 'title'),
    ).not.toBeNull();
    const cancel = await opened({ pick: 'cancel' });
    await expect(
      cancel.b.setEntryArtFromFile(cancel.view.generation, e.id, 'cover'),
    ).resolves.toBeNull();
    const bad = await opened({ pick: 'not-image' });
    await expect(
      bad.b.setEntryArtFromFile(bad.view.generation, e.id, 'cover'),
    ).rejects.toMatchObject({ code: 'bad-image' });
  });

  it('exports the audit and the job report as formula-safe text', async () => {
    const { b, view } = await opened();
    expect(await b.exportAudit(view.generation, 'csv')).toBe(
      'coverly-audit.csv',
    );
    const audit = b.exports[0]!.text;
    expect(audit.split('\n')[0]).toBe(
      'system,folder,rel_path,stem,size,modified,cover,screenshot,title',
    );
    expect(audit.split('\n')).toHaveLength(view.entries.length + 2);
    const v = await planned(b, view.generation);
    await expect(b.exportJobReport(v.id, 'csv')).rejects.toMatchObject({
      code: 'bad-request',
    });
    const done = next(b, 'done');
    await b.runFill(v.id);
    await done;
    expect(await b.exportJobReport(v.id, 'json')).toBe(
      `coverly-job-${v.id}.json`,
    );
    expect(JSON.parse(b.exports[1]!.text)).toHaveProperty('items');
  });
});
describe('mock drawer search', () => {
  it('lists candidates for a typed title, writes the pick and remembers it', async () => {
    const b = createMockBackend({
      delayMs: 0,
      now: FIXTURE_NOW,
      storage: null,
    });
    const view = await b.openLibrary(MOCK_LIBRARIES[1].path);
    const entry = view.entries.find((e) => e.art.cover === 'missing')!;
    const found = await b.searchEntryArt(
      view.generation,
      entry.id,
      'cover',
      '  Golden Sun ',
    );
    expect(found.map((c) => c.name)).toEqual([
      'Golden Sun (USA)',
      'Golden Sun (Europe)',
      'Golden Sun (Japan)',
    ]);
    const art = await b.entrySearchArt(view.generation, entry.id, 'cover', 1);
    expect(art.mime).toBe('image/svg+xml');
    const updates = await b.setEntryArtFromSearch(
      view.generation,
      entry.id,
      'cover',
      1,
    );
    expect(updates[0]!.art.cover).toBe('present');
    const id = await b.identifyEntry(view.generation, entry.id);
    expect(id).toMatchObject({
      method: 'manual',
      canonical: 'Golden Sun (Europe)',
      confidence: 1,
    });
    await b.clearIdentityOverride(view.generation, entry.id);
    expect((await b.identifyEntry(view.generation, entry.id)).method).not.toBe(
      'manual',
    );
  });

  it('refuses empty, control-character and overlong queries', async () => {
    const b = createMockBackend({
      delayMs: 0,
      now: FIXTURE_NOW,
      storage: null,
    });
    const view = await b.openLibrary(MOCK_LIBRARIES[1].path);
    for (const q of ['', '   ', 'a\u0007b', 'x'.repeat(121)]) {
      await expect(
        b.searchEntryArt(view.generation, 0, 'cover', q),
      ).rejects.toMatchObject({ code: 'bad-request' });
    }
    await expect(
      b.setEntryArtFromSearch(view.generation, 0, 'cover', 0),
    ).rejects.toMatchObject({ code: 'bad-request' });
  });
});

describe('mock fill sources', () => {
  it('popularity with SteamGridDB ready picks voted grids and says why', async () => {
    const { b, view } = await opened({ storage: new MemoryStorage() });
    await b.updateSettings({
      sources: {
        order: [
          { id: 'libretro', enabled: true },
          { id: 'steamgriddb', enabled: true },
          { id: 'screenscraper', enabled: false },
        ],
      },
    });
    await b.setCredential('steamgriddb-api-key', 'sk');
    const v = await planned(b, view.generation, {
      ...REQUEST,
      strategy: 'popularity',
    });
    const items = await b.fillJobItems(v.id, 'all');
    // Variant (beta) items keep their own reason, as in Rust (D22).
    const chosen = items
      .filter((i) => i.action !== 'none' && i.note === null)
      .map((i) => i.candidates[0]!);
    expect(chosen.length).toBeGreaterThan(0);
    expect(
      chosen.every((c) => c.source === 'steamgriddb' && c.votes !== null),
    ).toBe(true);
    expect(chosen.every((c) => c.reason?.why === 'votes')).toBe(true);
    const first = items.find((i) => i.action !== 'none')!;
    expect(first.candidates.map((c) => c.index)).toEqual(
      first.candidates.map((_, n) => n),
    );
    expect(first.candidates.slice(1).every((c) => c.reason === null)).toBe(
      true,
    );
    expect(first.candidates[0]!.origin).toMatch(/^game \d+ · grid \d+$/);
  });

  it('without a SteamGridDB key, popularity explains consensus and priority explains source order', async () => {
    const { b, view } = await opened({ storage: new MemoryStorage() });
    // Enabled but no key stored: not ready, so libretro only.
    await b.updateSettings({
      sources: {
        order: [
          { id: 'libretro', enabled: true },
          { id: 'steamgriddb', enabled: true },
          { id: 'screenscraper', enabled: false },
        ],
      },
    });
    const cases = [
      ['popularity', 'consensus'],
      ['priority', 'source-order'],
      ['rom-region', 'region'],
    ] as const;
    for (const [strategy, why] of cases) {
      const v = await planned(b, view.generation, { ...REQUEST, strategy });
      const items = await b.fillJobItems(v.id, 'all');
      const chosen = items
        .filter((i) => i.action !== 'none' && i.note === null)
        .map((i) => i.candidates[0]!);
      expect(chosen.length).toBeGreaterThan(0);
      expect(
        chosen.every((c) => c.source === 'libretro' && c.votes === null),
      ).toBe(true);
      expect(chosen.some((c) => c.reason?.why === why)).toBe(true);
      await b.cancelFill(v.id);
    }
  });

  it('the drawer list explains its first candidate only', async () => {
    const { b, view } = await opened({ storage: new MemoryStorage() });
    const list = await b.entryCandidates(view.generation, 0, 'cover');
    expect(list[0]!.reason).toEqual({ why: 'better-match' });
    expect(list.slice(1).every((c) => c.reason === null)).toBe(true);
    const found = await b.searchEntryArt(view.generation, 0, 'cover', 'Zelda');
    expect(found.every((c) => c.reason === null)).toBe(true);
  });

  it('a key but SteamGridDB disabled keeps the grids out', async () => {
    const { b, view } = await opened({ storage: new MemoryStorage() });
    await b.setCredential('steamgriddb-api-key', 'sk');
    const v = await planned(b, view.generation, {
      ...REQUEST,
      strategy: 'popularity',
    });
    const items = await b.fillJobItems(v.id, 'all');
    expect(
      items.every((i) => i.candidates.every((c) => c.source === 'libretro')),
    ).toBe(true);
  });
});
