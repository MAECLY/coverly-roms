import { clearMocks, mockIPC } from '@tauri-apps/api/mocks';
import { afterEach, describe, expect, it, vi } from 'vitest';

import sample from './contract.sample.json';
import { BackendError } from './errors';

const dialog = vi.hoisted(() => ({ open: vi.fn() }));
vi.mock('@tauri-apps/plugin-dialog', () => dialog);

const events = vi.hoisted(() => ({
  handlers: new Map<string, (e: { payload: unknown }) => void>(),
}));
// Keep the module's other exports: `@tauri-apps/api/webview` imports them.
vi.mock('@tauri-apps/api/event', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@tauri-apps/api/event')>()),
  listen: vi.fn(async (name: string, cb: (e: { payload: unknown }) => void) => {
    events.handlers.set(name, cb);
    return () => events.handlers.delete(name);
  }),
}));

import {
  createTauriBackend,
  ENTRIES_UPDATED_EVENT,
  JOB_CHANGED_EVENT,
  SCAN_PROGRESS_EVENT,
} from './tauriBackend';

const PNG = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);

describe('Tauri backend', () => {
  afterEach(() => {
    clearMocks();
    dialog.open.mockReset();
  });

  it('opens a library with camelCase arguments', async () => {
    const calls: [string, unknown][] = [];
    mockIPC((cmd, args) => {
      calls.push([cmd, args]);
      return sample.library_view;
    });
    const view = await createTauriBackend().openLibrary('/Volumes/SD');
    expect(view.entries).toHaveLength(1);
    expect(calls).toEqual([
      ['open_library', { path: '/Volumes/SD', profileId: null }],
    ]);
  });

  it('passes entry ids and media to read_art and sniffs the bytes', async () => {
    const calls: [string, unknown][] = [];
    mockIPC((cmd, args) => {
      calls.push([cmd, args]);
      return PNG.buffer;
    });
    const art = await createTauriBackend().readArt(3, 9, 'cover');
    expect(art.mime).toBe('image/png');
    expect(calls).toEqual([
      ['read_art', { generation: 3, entryId: 9, media: 'cover' }],
    ]);
  });

  it('refuses bytes that are not an image', async () => {
    mockIPC(() => new TextEncoder().encode('<svg/>').buffer);
    await expect(
      createTauriBackend().readArt(1, 0, 'cover'),
    ).rejects.toMatchObject({ code: 'art-unreadable' });
  });

  it('maps command errors to BackendError', async () => {
    mockIPC(() => Promise.reject({ code: 'not-found', detail: '/x' }));
    const err = await createTauriBackend()
      .openLibrary('/x')
      .catch((e: unknown) => e);
    expect(err).toBeInstanceOf(BackendError);
    expect(err).toMatchObject({ code: 'not-found', detail: '/x' });
  });

  it('sends ignored folders as ignoredDirs', async () => {
    const calls: [string, unknown][] = [];
    mockIPC((cmd, args) => {
      calls.push([cmd, args]);
      return { ignored_dirs: ['bios'] };
    });
    await createTauriBackend().setLibrarySettings({ ignored_dirs: ['bios'] });
    expect(calls).toEqual([
      ['set_library_settings', { ignoredDirs: ['bios'] }],
    ]);
  });

  it('forwards scan progress events', async () => {
    const seen: unknown[] = [];
    const off = await createTauriBackend().onScanProgress((p) => seen.push(p));
    events.handlers.get(SCAN_PROGRESS_EVENT)!({
      payload: sample.scan_progress,
    });
    expect(seen).toEqual([sample.scan_progress]);
    off();
    expect(events.handlers.has(SCAN_PROGRESS_EVENT)).toBe(false);
  });

  it('picks a single folder with the dialog', async () => {
    dialog.open.mockResolvedValueOnce('/picked');
    await expect(createTauriBackend().pickFolder()).resolves.toBe('/picked');
    expect(dialog.open).toHaveBeenCalledWith({
      directory: true,
      multiple: false,
    });
    dialog.open.mockResolvedValueOnce(null);
    await expect(createTauriBackend().pickFolder()).resolves.toBeNull();
  });

  it('starts a fill with the request object and forwards job events', async () => {
    const calls: [string, unknown][] = [];
    mockIPC((cmd, args) => {
      calls.push([cmd, args]);
      return sample.job_view;
    });
    const b = createTauriBackend();
    const request = {
      entry_ids: null,
      media: ['cover'] as const,
      strategy: 'rom-region' as const,
      regions: ['rom', 'usa'] as const,
      overwrite: 'never' as const,
    };
    await b.startFill(3, {
      ...request,
      media: [...request.media],
      regions: [...request.regions],
    });
    await b.decideFill(4, [{ index: 0, choice: 'accept' }]);
    await b.fillJobItems(4, 'review');
    expect(calls).toEqual([
      [
        'start_fill',
        {
          generation: 3,
          request: { ...request, media: ['cover'], regions: ['rom', 'usa'] },
        },
      ],
      ['decide_fill', { job: 4, decisions: [{ index: 0, choice: 'accept' }] }],
      ['fill_job_items', { job: 4, filter: 'review' }],
    ]);
    const seen: unknown[] = [];
    await b.onJobChanged((v) => seen.push(v));
    await b.onEntriesUpdated((u) => seen.push(u));
    events.handlers.get(JOB_CHANGED_EVENT)!({ payload: sample.job_view });
    events.handlers.get(ENTRIES_UPDATED_EVENT)!({
      payload: sample.entries_updated,
    });
    expect(seen).toEqual([sample.job_view, sample.entries_updated]);
  });

  it('sniffs candidate art and passes drawer arguments in camelCase', async () => {
    const calls: [string, unknown][] = [];
    mockIPC((cmd, args) => {
      calls.push([cmd, args]);
      return cmd.endsWith('_art') ? PNG.buffer : null;
    });
    const b = createTauriBackend();
    expect((await b.candidateArt(4, 2, 1)).mime).toBe('image/png');
    expect((await b.entryCandidateArt(3, 9, 'cover', 0)).mime).toBe(
      'image/png',
    );
    await expect(b.setEntryArtFromFile(3, 9, 'cover')).resolves.toBeNull();
    expect(calls).toEqual([
      ['candidate_art', { job: 4, index: 2, candidate: 1 }],
      [
        'entry_candidate_art',
        { generation: 3, entryId: 9, media: 'cover', candidate: 0 },
      ],
      [
        'set_entry_art_from_file',
        { generation: 3, entryId: 9, media: 'cover' },
      ],
    ]);
  });
});
