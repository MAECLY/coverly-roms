import { describe, expect, it } from 'vitest';

import { BackendError } from '../errors';
import { FIXTURE_NOW } from './fixture';
import {
  createMockBackend,
  escapeXml,
  MOCK_EJECTED,
  MOCK_LIBRARIES,
  mockArtSvg,
} from './mockBackend';

const backend = () => createMockBackend({ delayMs: 0, now: FIXTURE_NOW });
const BIG = MOCK_LIBRARIES[0].path;
const SMALL = MOCK_LIBRARIES[1].path;

describe('mock backend', () => {
  it('lists the fixture libraries and an ejected one as recents', async () => {
    const recents = await backend().recentLibraries();
    expect(recents.map((r) => r.path)).toEqual([
      ...MOCK_LIBRARIES.map((l) => l.path),
      MOCK_EJECTED,
    ]);
    expect(recents.find((r) => r.path === MOCK_EJECTED)?.available).toBe(false);
  });

  it('lists one connected drive that opens as a library', async () => {
    const b = backend();
    const drives = await b.mountedVolumes();
    expect(drives).toEqual([
      { path: '/mock/Small library', name: 'Small library' },
    ]);
    const view = await b.openLibrary(drives[0]!.path);
    expect(view.name).toBe('Small library');
  });

  it('opens the 12k library with progress per system', async () => {
    const b = backend();
    const progress: number[] = [];
    const off = await b.onScanProgress((p) => progress.push(p.done));
    const view = await b.openLibrary(BIG);
    off();
    expect(view.entries).toHaveLength(12_000);
    expect(progress[0]).toBe(0);
    expect(progress.at(-1)).toBe(view.systems.length);
    expect((await b.recentLibraries())[0]?.path).toBe(BIG);
  });

  it('announces the new generation before the open can fail', async () => {
    const b = backend();
    const seen: number[] = [];
    const off = await b.onScanProgress((p) => seen.push(p.generation));
    await b.openLibrary('/mock/Broken').catch(() => undefined);
    off();
    expect(seen).toEqual([1]);
  });

  it('lists the fixture systems by name', async () => {
    const systems = await createMockBackend({ delayMs: 0 }).listSystems();
    expect(systems).toContainEqual({
      id: 'gba',
      name: 'Game Boy Advance',
      family: 'handheld',
    });
    // Systems only the organise piles have are listed too (Rust lists its
    // whole catalogue), so the UI never falls back to the id.
    expect(systems).toContainEqual({
      id: 'saturn',
      name: 'Sega Saturn',
      family: 'console',
    });
    const names = systems.map((s) => s.name);
    expect(names).toEqual(
      [...names].sort((a, b) =>
        a.localeCompare(b, 'en', { sensitivity: 'base' }),
      ),
    );
  });

  it('rejects unknown folders as not-found', async () => {
    await expect(backend().openLibrary('/mock/Broken')).rejects.toMatchObject({
      code: 'not-found',
      detail: '/mock/Broken',
    });
  });

  it('serves SVG art only for present media of the current generation', async () => {
    const b = backend();
    const view = await b.openLibrary(SMALL);
    const withCover = view.entries.find((e) => e.art.cover === 'present')!;
    const without = view.entries.find((e) => e.art.cover === 'missing')!;
    const art = await b.readArt(view.generation, withCover.id, 'cover');
    expect(art.mime).toBe('image/svg+xml');
    await expect(
      b.readArt(view.generation, without.id, 'cover'),
    ).rejects.toMatchObject({ code: 'art-missing' });
    await expect(
      b.readArt(view.generation + 1, withCover.id, 'cover'),
    ).rejects.toMatchObject({ code: 'stale-library' });
    await expect(
      b.readArt(view.generation, 99_999, 'cover'),
    ).rejects.toBeInstanceOf(BackendError);
  });

  it('mock_art_escapes_hostile_names', async () => {
    const b = backend();
    const view = await b.openLibrary(SMALL);
    const hostile = view.entries.find((e) => e.name.startsWith('<img'))!;
    const svg = mockArtSvg(hostile, 'cover');
    expect(svg).not.toContain('<img');
    expect(svg).toContain('&lt;img src=x onerror=alert(1)&gt;');
    expect(escapeXml(`"'&<>`)).toBe('&quot;&apos;&amp;&lt;&gt;');
  });

  it('identifies on demand and records reveals', async () => {
    const b = backend();
    const view = await b.openLibrary(SMALL);
    const byHash = await b.identifyEntry(view.generation, 0);
    expect(byHash.method).toBe('hash');
    expect(byHash.confidence).toBe(1);
    const byName = await b.identifyEntry(view.generation, 1);
    expect(byName.method).toBe('name');
    await b.revealEntry(view.generation, 3);
    expect(b.revealed).toEqual([3]);
  });

  it('opens a dropped folder by name', async () => {
    const b = backend();
    const dropped: string[] = [];
    const off = await b.onFolderDrop((p) => dropped.push(p));
    const ev = new Event('drop', { cancelable: true });
    Object.defineProperty(ev, 'dataTransfer', {
      value: { files: [], getData: () => 'Small library' },
    });
    window.dispatchEvent(ev);
    off();
    expect(dropped).toEqual([SMALL]);
  });

  it('keeps and normalises library settings', async () => {
    const b = backend();
    const saved = await b.setLibrarySettings({
      ignored_dirs: [' bios ', '', 'BIOS', 'a/b', 'Saves'],
    });
    expect(saved.ignored_dirs).toEqual(['bios', 'Saves']);
    expect((await b.librarySettings()).ignored_dirs).toEqual(['bios', 'Saves']);
  });
});
