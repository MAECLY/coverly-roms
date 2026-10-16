import { describe, expect, it } from 'vitest';

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
