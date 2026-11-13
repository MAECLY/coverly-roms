import { render, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { I18nProvider } from '@/i18n/I18nProvider';
import { setBackend, type Backend } from '@/ipc/backend';
import { MOCK_ADDONS } from '@/ipc/mock/addonViewsFixture';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import { createMockBackend } from '@/ipc/mock/mockBackend';
import type { ChildView, EntryView } from '@/ipc/types';
import { clearArtCache } from '@/lib/artCache';

import { ChildThumb } from './ChildThumb';

describe('ChildThumb (addon views §8.3)', () => {
  let backend: Backend;
  let lib: Awaited<ReturnType<Backend['openLibrary']>>;
  beforeEach(async () => {
    backend = createMockBackend({ delayMs: 0, now: FIXTURE_NOW });
    lib = await backend.openLibrary(MOCK_ADDONS);
    let n = 0;
    vi.spyOn(URL, 'createObjectURL').mockImplementation(() => `blob:${++n}`);
    vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {});
  });
  afterEach(() => {
    vi.restoreAllMocks();
    clearArtCache();
    setBackend(null);
  });

  const named = (prefix: string): EntryView =>
    lib.entries.find((e) => e.name.startsWith(prefix))!;
  const dlcOf = (e: EntryView): ChildView =>
    e.children!.find((c) => c.role === 'dlc')!;
  const thumb = (e: EntryView, c: ChildView, size?: 48 | 24) =>
    render(
      <I18nProvider locale="es">
        <ChildThumb
          generation={lib.generation}
          entry={e}
          child={c}
          systemId={lib.systems[e.system]!.system_id}
          size={size}
        />
      </I18nProvider>,
    );

  it('shows the child’s own icon, read by entry id and child index', async () => {
    const readChildArtBytes = vi.fn(backend.readChildArtBytes);
    setBackend({ ...backend, readChildArtBytes });
    const lantern = named('Lantern Harbor (USA)');
    const dlc = dlcOf(lantern);
    const { container } = thumb(lantern, dlc);
    await waitFor(() => expect(container.querySelector('img')).not.toBeNull());
    expect(readChildArtBytes).toHaveBeenCalledWith(
      lib.generation,
      lantern.id,
      dlc.index,
      'icon',
    );
    const root = container.firstElementChild!;
    expect(root).toHaveAttribute('data-art', 'own');
    expect(root).toHaveAttribute('aria-hidden', 'true');
    expect(root).toHaveClass('child-thumb--48');
  });

  it('dims the base game’s art for a Switch DLC without its own', () => {
    setBackend(backend);
    const tide = named('Tidewalker');
    const { container } = thumb(tide, dlcOf(tide));
    const root = container.firstElementChild!;
    expect(root).toHaveAttribute('data-art', 'base');
    expect(root.querySelector('.child-thumb__inherit')).not.toBeNull();
  });

  it('draws the dashed missing slot with «sin arte» elsewhere', () => {
    setBackend(backend);
    const neon = named('Neon Rhythm');
    const { container } = thumb(neon, dlcOf(neon));
    const root = container.firstElementChild!;
    expect(root).toHaveAttribute('data-art', 'missing');
    expect(root.querySelector('.art-placeholder')).toHaveTextContent(
      'sin arte',
    );
  });

  it('has a 24px size with no words in the slot', () => {
    setBackend(backend);
    const neon = named('Neon Rhythm');
    const { container } = thumb(neon, dlcOf(neon), 24);
    const root = container.firstElementChild!;
    expect(root).toHaveClass('child-thumb--24');
    expect(root.querySelector('.art-placeholder')).toHaveTextContent('');
  });
});
